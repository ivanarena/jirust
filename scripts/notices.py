#!/usr/bin/env python3
"""Deterministic notices/source inventory. Never emits local metadata paths."""
import argparse, hashlib, json, os, pathlib, subprocess, tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
about = os.environ.get('CARGO_ABOUT', 'cargo-about')
version = subprocess.check_output([about, '--version'], text=True).strip()
if '0.9.2' not in version:
    raise SystemExit('Use cargo-about 0.9.2 (--locked --features cli) for reproducible notices')
# Scan only locked packages; no automatic lockfile changes or account access.
notice = subprocess.check_output([about, 'generate', '--locked', '--offline', 'about.hbs'], cwd=ROOT)
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--offline', '--format-version', '1'], cwd=ROOT))
import tomllib
lock = tomllib.loads((ROOT / 'Cargo.lock').read_text())
checksums = {(p['name'], p['version']): p.get('checksum') for p in lock['package']}
packages = []
for p in metadata['packages']:
    if p['source'] is None:
        continue
    if p['source'] != 'registry+https://github.com/rust-lang/crates.io-index':
        raise SystemExit('Non-registry dependencies require a reviewed source distribution plan')
    checksum = checksums.get((p['name'], p['version']))
    if checksum is None:
        raise SystemExit('Missing locked checksum')
    packages.append({'name': p['name'], 'version': p['version'], 'license': p['license'],
                     'sha256': checksum,
                     'source': f"https://crates.io/api/v1/crates/{p['name']}/{p['version']}/download"})
packages.sort(key=lambda p: (p['name'], p['version']))
sources = (json.dumps({'cargo_lock_sha256': hashlib.sha256((ROOT / 'Cargo.lock').read_bytes()).hexdigest(),
                      'packages': packages}, indent=2, sort_keys=True) + '\n').encode()
for name, content in [('THIRD_PARTY_NOTICES.md', notice), ('THIRD_PARTY_SOURCES.json', sources)]:
    path = ROOT / name
    if args.check:
        if not path.exists() or path.read_bytes() != content:
            raise SystemExit(f'{name} is stale; run make notices')
    else:
        with tempfile.NamedTemporaryFile(dir=ROOT, delete=False) as temporary:
            temporary.write(content)
            staging = pathlib.Path(temporary.name)
        staging.chmod(0o644)
        staging.replace(path)
print(f'Notices and source inventory verified for {len(packages)} registry packages')
