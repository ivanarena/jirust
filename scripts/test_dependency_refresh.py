"""Exercise the actual maintenance shell with local Git and fake network tools."""
import os
import pathlib
import subprocess
import tempfile
import textwrap
import unittest

WORKFLOW = pathlib.Path(__file__).resolve().parent.parent / '.github/workflows/dependencies.yml'
# The workflow's final step is its only multiline run block. Keep this test
# dependency-free so the same checks run on Linux and macOS without pip installs.
SCRIPT = textwrap.dedent(WORKFLOW.read_text().split('        run: |\n', 1)[1])

class RefreshTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = pathlib.Path(self.tmp.name)
        self.git_env = dict(os.environ, GIT_CONFIG_NOSYSTEM='1', GIT_CONFIG_GLOBAL=os.devnull)
        self.remote = self.root / 'remote.git'
        self.repo = self.root / 'repo'
        self.run_cmd(['git', 'init', '--bare', '--initial-branch=main', str(self.remote)], self.root)
        self.run_cmd(['git', 'clone', str(self.remote), str(self.repo)], self.root)
        self.git('config', 'user.name', 'Test')
        self.git('config', 'user.email', 'test@example.invalid')
        for name in ['Cargo.lock', 'THIRD_PARTY_NOTICES.md', 'THIRD_PARTY_SOURCES.json']:
            (self.repo / name).write_text('old\n')
        self.git('add', '.')
        self.git('commit', '-m', 'Initial')
        self.git('push', 'origin', 'main')
        tools = self.root / 'tools'
        tools.mkdir()
        stubs = {
            'cargo': '''#!/bin/bash
set -eu
if [[ $1 == update ]]; then printf '%s\\n' "$VERSION" > Cargo.lock; fi
''',
            'make': '''#!/bin/bash
set -eu
if [[ $1 == notices ]]; then
  cp Cargo.lock THIRD_PARTY_NOTICES.md
  cp Cargo.lock THIRD_PARTY_SOURCES.json
fi
if [[ $1 == notices-check ]]; then
  cmp Cargo.lock THIRD_PARTY_NOTICES.md
  cmp Cargo.lock THIRD_PARTY_SOURCES.json
fi
''',
            'gh': '''#!/bin/bash
set -eu
printf '%s\\n' "$*" >> "$CALL_LOG"
if [[ $1 == pr && $2 == list && -f "$PR_EXISTS" ]]; then echo 42; fi
if [[ $1 == pr && $2 == create ]]; then
  if [[ ${FAIL_CREATE:-0} == 1 ]]; then exit 1; fi
  touch "$PR_EXISTS"
fi
''',
        }
        for name, text in stubs.items():
            file = tools / name
            file.write_text(text)
            file.chmod(0o755)
        self.log = self.root / 'calls'
        self.env = dict(self.git_env, PATH=f'{tools}:{os.environ["PATH"]}', BASE_BRANCH='main',
                        VERSION='new', CALL_LOG=str(self.log), PR_EXISTS=str(self.root / 'pr-exists'))

    def run_cmd(self, args, cwd, **kwargs):
        return subprocess.run(args, cwd=cwd, env=self.git_env, text=True,
                              capture_output=True, check=True, **kwargs).stdout.strip()

    def git(self, *args):
        return self.run_cmd(['git', *args], self.repo)

    def refresh(self, succeeds=True):
        result = subprocess.run(['bash', '-c', SCRIPT], cwd=self.repo, env=self.env, text=True, capture_output=True)
        self.assertEqual(result.returncode == 0, succeeds, result.stdout + result.stderr)
        return result

    def reset_to_main(self):
        self.git('switch', 'main')
        self.git('branch', '-D', 'automation/dependencies')
        self.git('fetch', 'origin')

    def test_current_dependencies_do_not_open_pr_or_branch(self):
        self.env['VERSION'] = 'old'
        self.refresh()
        self.assertFalse(self.log.exists())
        self.assertEqual(self.git('ls-remote', '--heads', 'origin', 'automation/dependencies'), '')

    def test_update_pushes_inventories_dispatches_ci_and_opens_pr(self):
        self.refresh()
        self.assertEqual((self.repo / 'Cargo.lock').read_text(), 'new\n')
        self.assertEqual(self.git('status', '--porcelain'), '')
        calls = self.log.read_text().splitlines()
        self.assertEqual(calls[0], 'workflow run check.yml --ref automation/dependencies')
        self.assertTrue(any(call.startswith('pr create ') for call in calls))
        self.assertTrue(self.git('ls-remote', '--heads', 'origin', 'automation/dependencies'))

    def test_repeat_reuses_pr_and_preserves_commit(self):
        self.refresh()
        first = self.git('rev-parse', 'HEAD')
        self.reset_to_main()
        self.refresh()
        self.assertEqual(first, self.git('rev-parse', 'HEAD'))
        calls = self.log.read_text()
        self.assertEqual(calls.count('pr create '), 1)
        self.assertEqual(calls.count('workflow run '), 2)

    def test_next_update_is_fast_forward(self):
        self.refresh()
        first = self.git('rev-parse', 'HEAD')
        self.reset_to_main()
        self.env['VERSION'] = 'newer'
        self.refresh()
        self.git('merge-base', '--is-ancestor', first, 'HEAD')
        self.assertEqual((self.repo / 'Cargo.lock').read_text(), 'newer\n')
        self.assertEqual(self.log.read_text().count('pr create '), 1)

    def test_blocked_pr_creation_still_dispatches_ci_and_can_recover(self):
        self.env['FAIL_CREATE'] = '1'
        self.refresh(succeeds=False)
        self.assertTrue(self.log.read_text().startswith('workflow run check.yml --ref automation/dependencies\n'))
        self.reset_to_main()
        self.env['FAIL_CREATE'] = '0'
        self.refresh()
        self.assertTrue((self.root / 'pr-exists').exists())

    def test_conflict_fails_without_overwriting_remote(self):
        self.refresh()
        first = self.git('rev-parse', 'HEAD')
        self.reset_to_main()
        (self.repo / 'Cargo.lock').write_text('conflicting main update\n')
        self.git('add', 'Cargo.lock')
        self.git('commit', '-m', 'Conflicting update')
        self.git('push', 'origin', 'main')
        self.git('fetch', 'origin')
        self.log.unlink()
        self.refresh(succeeds=False)
        self.assertFalse(self.log.exists())
        remote_head = self.git('ls-remote', '--heads', 'origin', 'automation/dependencies').split()[0]
        self.assertEqual(remote_head, first)

if __name__ == '__main__':
    unittest.main(verbosity=2)
