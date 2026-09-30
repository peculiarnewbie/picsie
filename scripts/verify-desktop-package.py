"""Extract real release artifacts and exercise their native executable outside the checkout."""
import argparse
import hashlib
import io
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import zipfile

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--target', required=True)
args = parser.parse_args()
distribution = root / 'dist' / args.target
output = root / 'artifacts/desktop-package-verification' / args.target
if output.exists():
    shutil.rmtree(output)
output.mkdir(parents=True)
version = __import__('json').loads((root / 'package.json').read_text())['version']

for line in (distribution / f'SHA256SUMS-{args.target}.txt').read_text().splitlines():
    digest, name = line.split('  ', 1)
    assert Path(name).name == name
    assert hashlib.sha256((distribution / name).read_bytes()).hexdigest() == digest, name
print('PASS release artifact checksums', flush=True)

def executable(binary):
    # Deliberately run from an unrelated directory with no runtime Node/QuickGUI files.
    result = subprocess.run([str(binary), '--version'], cwd=output, capture_output=True, text=True, check=True)
    assert result.stdout.strip() == f'Picsie {version}', result.stdout
    result = subprocess.run([str(binary), '--help'], cwd=output, capture_output=True, text=True, check=True)
    assert '.electropic' in result.stdout and '.comp' in result.stdout
    result = subprocess.run([str(binary), '--open'], cwd=output, capture_output=True, text=True)
    assert result.returncode != 0 and 'requires a project path' in result.stderr
    print('PASS native executable version, help and invalid arguments:', binary, flush=True)

if args.target.startswith('linux-'):
    archives = list(distribution.glob('*.tar.gz'))
    assert len(archives) == 1
    portable = output / 'portable'
    with tarfile.open(archives[0]) as archive:
        archive.extractall(portable, filter='data')
    binaries = list(portable.glob('*/picsie'))
    assert len(binaries) == 1
    binary = binaries[0]
    assert os.access(binary, os.X_OK)
    assert (binary.parent / 'THIRD_PARTY_NOTICES.md').is_file()
    assert (binary.parent / 'licenses/dependencies.json').is_file()
    assert not list(portable.rglob('*.node'))
    executable(binary)

    packages = list(distribution.glob('*.deb'))
    assert len(packages) == 1
    members = subprocess.check_output(['ar', 't', str(packages[0])], text=True).splitlines()
    member = next(name for name in members if name.startswith('data.tar.'))
    data = subprocess.check_output(['ar', 'p', str(packages[0]), member])
    installed = output / 'deb'
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        archive.extractall(installed, filter='data')
    binary = installed / 'usr/bin/picsie'
    assert binary.read_bytes() == binaries[0].read_bytes()
    desktop = (installed / 'usr/share/applications/picsie.desktop').read_text()
    assert 'Exec=picsie %F' in desktop and 'Name=Picsie' in desktop
    assert 'application/x-picsie' in desktop and 'application/x-electropic' in desktop
    assert (installed / 'usr/share/mime/packages/picsie.xml').is_file()
    assert list((installed / 'usr/share/icons').rglob('picsie.png'))
    assert list(installed.rglob('THIRD_PARTY_NOTICES.md'))
    assert list(installed.rglob('LICENSE-APACHE'))
    executable(binary)
    print('PASS Debian binary, file associations, icon and notices', flush=True)
elif args.target.startswith('windows-'):
    archives = list(distribution.glob('*.zip'))
    assert len(archives) == 1 and list(distribution.glob('*.exe'))
    with zipfile.ZipFile(archives[0]) as archive:
        archive.extractall(output / 'portable')
    binaries = list((output / 'portable').glob('*/picsie.exe'))
    assert len(binaries) == 1
    assert (binaries[0].parent / 'THIRD_PARTY_NOTICES.md').is_file()
    assert (binaries[0].parent / 'licenses/dependencies.json').is_file()
    assert not list(output.rglob('*.node'))
    executable(binaries[0])
else:
    raise SystemExit('Package verification currently supports Linux and Windows release targets.')
