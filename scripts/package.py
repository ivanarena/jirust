#!/usr/bin/env python3
"""Build a LOCAL native bundle from a clean committed tree. Never publishes."""
import gzip, hashlib, io, json, os, pathlib, platform, re, subprocess, tarfile, tempfile, urllib.request
ROOT = pathlib.Path(__file__).resolve().parent.parent

def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT)

def main():
    if git('status', '--porcelain').strip():
        raise SystemExit('Packaging requires a clean committed tree; no ignored/private files are bundled')
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'], cwd=ROOT))
    package = next(p for p in metadata['packages'] if p['name'] == 'jirust')
    version = package['version']
    binary = ROOT / 'bin/jirust'
    if subprocess.check_output([str(binary), '--version'], text=True).strip() != f'jirust {version}':
        raise SystemExit('Binary/version mismatch; rebuild')
    sources = json.loads((ROOT / 'THIRD_PARTY_SOURCES.json').read_text())
    if sources['cargo_lock_sha256'] != hashlib.sha256((ROOT / 'Cargo.lock').read_bytes()).hexdigest():
        raise SystemExit('Source inventory is stale')
    label = f"jirust-{version}-{platform.system().lower()}-{platform.machine().lower()}"
    if not re.fullmatch(r'[A-Za-z0-9_.+-]+', label):
        raise SystemExit('Invalid artifact label')
    dist = ROOT / 'dist'
    dist.mkdir(exist_ok=True)
    timestamp = int(git('show', '-s', '--format=%ct', 'HEAD'))
    with tempfile.TemporaryDirectory() as temporary:
        tree = pathlib.Path(temporary) / label
        tree.mkdir()
        for name in ['LICENSE', 'THIRD_PARTY_NOTICES.md', 'THIRD_PARTY_SOURCES.json', 'SECURITY.md']:
            (tree / name).write_bytes((ROOT / name).read_bytes())
        (tree / 'jirust').write_bytes(binary.read_bytes())
        (tree / 'jirust').chmod(0o755)
        for name in ['SUPPORT.md', 'RELEASING.md']:
            (tree / name).write_bytes((ROOT / 'docs' / name).read_bytes())
        with (tree / 'jirust-source.tar.gz').open('wb') as output:
            subprocess.run(['git', 'archive', '--format=tar.gz', f'--prefix=jirust-{version}/', 'HEAD'], cwd=ROOT, stdout=output, check=True)
        covered = tree / 'covered-source'
        covered.mkdir()
        for p in sources['packages']:
            if 'MPL-2.0' not in p['license']:
                continue
            expected = f"https://crates.io/api/v1/crates/{p['name']}/{p['version']}/download"
            if p['source'] != expected or not re.fullmatch(r'[A-Za-z0-9_.+-]+', f"{p['name']}-{p['version']}"):
                raise SystemExit('Unexpected source archive metadata')
            with urllib.request.urlopen(expected, timeout=45) as response:
                data = response.read(64 * 1024 * 1024 + 1)
            if len(data) > 64 * 1024 * 1024 or hashlib.sha256(data).hexdigest() != p['sha256']:
                raise SystemExit('Covered-source archive checksum/size mismatch')
            (covered / f"{p['name']}-{p['version']}.crate").write_bytes(data)
        archive = dist / f'{label}.tar.gz'
        staging = archive.with_suffix('.new')
        try:
            with staging.open('wb') as output, gzip.GzipFile(fileobj=output, mode='wb', filename='', mtime=0) as compressed, tarfile.open(fileobj=compressed, mode='w') as tar:
                for path in [tree] + sorted(tree.rglob('*')):
                    info = tar.gettarinfo(str(path), arcname=str(path.relative_to(tree.parent)))
                    info.uid = info.gid = 0
                    info.uname = info.gname = ''
                    info.mtime = timestamp
                    info.mode = 0o755 if path.is_dir() or path.name == 'jirust' else 0o644
                    if path.is_file():
                        with path.open('rb') as source:
                            tar.addfile(info, source)
                    else:
                        tar.addfile(info)
            staging.replace(archive)
        finally:
            staging.unlink(missing_ok=True)
        (dist / 'SHA256SUMS').write_text(f'{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n')
        print(f'Local bundle ready: dist/{archive.name} (nothing uploaded)')

if __name__ == '__main__':
    main()
