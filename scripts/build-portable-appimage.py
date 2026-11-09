#!/usr/bin/env python3
"""Build in a pinned Debian 12 rootfs, without sudo, Docker daemon or host installs.
Only .tools/portable and dist are written. No home directory or credentials are
mounted into the build. Rust crates are vendored using Cargo's checksum metadata.
"""
import copy
import fcntl
import hashlib
import json
import os
from pathlib import Path
import posixpath
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
CACHE = ROOT / '.tools/portable'
CACHE.mkdir(parents=True, exist_ok=True)
LOCK = json.loads((ROOT / 'packaging/portable-build.lock.json').read_text())

class Redirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        if not newurl.startswith('https://'):
            raise ValueError('Only HTTPS redirects are allowed')
        result = super().redirect_request(req, fp, code, msg, headers, newurl)
        if result and urllib.parse.urlsplit(req.full_url).netloc != urllib.parse.urlsplit(newurl).netloc:
            result.remove_header('Authorization')
        return result

OPENER = urllib.request.build_opener(Redirect())
def fetch(url, headers=None):
    return OPENER.open(urllib.request.Request(url, headers=headers or {}), timeout=120)

def download(url, destination, checksum, headers=None):
    if destination.is_file() and hashlib.file_digest(destination.open('rb'), 'sha256').hexdigest() == checksum:
        return
    print('Downloading verified build input:', destination.name, flush=True)
    with tempfile.NamedTemporaryFile(dir=CACHE, delete=False) as out:
        temporary = Path(out.name)
        with fetch(url, headers) as response:
            shutil.copyfileobj(response, out, 1024 * 1024)
    try:
        with temporary.open('rb') as stream:
            if hashlib.file_digest(stream, 'sha256').hexdigest() != checksum:
                raise ValueError('Checksum mismatch: ' + destination.name)
        temporary.replace(destination)
    finally:
        temporary.unlink(missing_ok=True)

def safe_member(member, destination):
    member = copy.copy(member)
    # Absolute distro symlinks become equivalent relative links confined to rootfs.
    if member.issym() and member.linkname.startswith('/'):
        member.linkname = posixpath.relpath(member.linkname, posixpath.dirname('/' + member.name))
    if member.islnk():
        member.linkname = member.linkname.lstrip('/')
    return tarfile.data_filter(member, destination)

def run():
    root = CACHE / 'debian-12-amd64'
    stamp = root / '.rustremote-base.json'
    if not stamp.is_file():
        if root.exists():
            raise RuntimeError('Unrecognized build root; no files were deleted: ' + str(root))
        with fetch('https://auth.docker.io/token?service=registry.docker.io&scope=repository:library/debian:pull') as response:
            token = json.load(response)['token']
        headers = {'Authorization': 'Bearer ' + token}
        with tempfile.TemporaryDirectory(prefix='base-', dir=CACHE) as staging:
            extracted = Path(staging) / 'root'; extracted.mkdir()
            for layer in LOCK['layers']:
                digest = layer['digest'].removeprefix('sha256:')
                archive = CACHE / (digest + '.tar.gz')
                download('https://registry-1.docker.io/v2/library/debian/blobs/sha256:' + digest, archive, digest, headers)
                with tarfile.open(archive) as source:
                    source.extractall(extracted, filter=safe_member)
            (extracted / '.rustremote-base.json').write_text(json.dumps(LOCK))
            extracted.rename(root)
    if json.loads(stamp.read_text()) != LOCK:
        raise RuntimeError('Build lock changed; use a new cache directory, existing root kept')
    downloads = root / 'build/downloads'; downloads.mkdir(parents=True, exist_ok=True)
    for name, checksum in LOCK['components'].items():
        download('https://static.rust-lang.org/dist/' + name, downloads / name, checksum)
    work = root / 'work'; work.mkdir(exist_ok=True)
    for name in ['Cargo.toml', 'Cargo.lock', 'build.rs', 'LICENSE', 'NOTICE']:
        shutil.copy2(ROOT / name, work / name)
    for name in ['src', 'assets', 'packaging', 'scripts']:
        shutil.copytree(ROOT / name, work / name, dirs_exist_ok=True)
    tools = work / '.tools'; tools.mkdir(exist_ok=True)
    for name in ['appimagetool-x86_64.AppImage', 'runtime-x86_64']:
        source = ROOT / '.tools' / name
        if source.is_file(): shutil.copy2(source, tools / name)
    cargo = root / 'build/cargo'; cargo.mkdir(exist_ok=True)
    vendor = root / 'build/vendor'
    if not vendor.exists() or (work / 'Cargo.lock').read_bytes() != (root / 'build/vendor.lock').read_bytes():
        # Cargo itself verifies crate archives and emits a complete directory source.
        subprocess.run(['cargo', 'vendor', '--locked', str(vendor)], cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
        shutil.copy2(work / 'Cargo.lock', root / 'build/vendor.lock')
    (cargo / 'config.toml').write_text('[source.crates-io]\nreplace-with="vendored"\n[source.vendored]\ndirectory="/build/vendor"\n')
    (root / 'etc/resolv.conf').unlink(missing_ok=True)
    shutil.copyfile('/etc/resolv.conf', root / 'etc/resolv.conf')
    (root / 'usr/sbin/policy-rc.d').write_text('#!/bin/sh\nexit 101\n')
    (root / 'usr/sbin/policy-rc.d').chmod(0o755)
    script = root / 'build/run.sh'
    script.write_text('''#!/bin/bash
set -euo pipefail
if [[ -z ${FAKEROOTKEY:-} ]]; then
    if [[ ! -x /usr/bin/fakeroot-sysv ]]; then
        cd /build
        apt-get -o APT::Sandbox::User=root download fakeroot libfakeroot
        for archive in /build/fakeroot_*.deb /build/libfakeroot_*.deb; do dpkg-deb -x "$archive" /; done
    fi
    exec /usr/bin/fakeroot-sysv -- /bin/bash /build/run.sh
fi
# No system bus activation or setgid helper is needed in a compiler rootfs.
helper=/usr/lib/dbus-1.0/dbus-daemon-launch-helper
if dpkg-statoverride --list "$helper" >/dev/null 2>&1; then dpkg-statoverride --remove "$helper"; fi
dpkg-statoverride --add root root 0755 "$helper"
if [[ ! -f /build/prerequisites-v5 ]]; then
    apt-get -o APT::Sandbox::User=root update
    apt-get -o APT::Sandbox::User=root install -y --no-install-recommends build-essential pkg-config python3 ca-certificates curl xz-utils libgtk-4-dev libvte-2.91-gtk4-dev libssl-dev libssh2-1-dev libgdk-pixbuf2.0-bin librsvg2-common fakeroot file desktop-file-utils xvfb xauth dbus-x11 python3-gi python3-pil gir1.2-atspi-2.0 at-spi2-core libxcomposite1 fonts-dejavu-core ibus-gtk4 fcitx5-frontend-gtk4
    touch /build/prerequisites-v5
fi
if [[ ! -x /opt/rust/bin/rustc ]]; then
    mkdir -p /build/install
    for archive in /build/downloads/*.tar.xz; do tar -xJf "$archive" -C /build/install; done
    for installer in /build/install/*/install.sh; do bash "$installer" --prefix=/opt/rust --disable-ldconfig; done
fi
cd /work
bash scripts/build-appimage.sh
dpkg-query -W -f='${binary:Package} ${Version}\\n' > dist/build-packages.txt
if [[ ${RUSTREMOTE_PORTABLE_VALIDATE:-0} == 1 ]]; then
    version=$(python3 -c 'import tomllib;print(tomllib.load(open("Cargo.toml","rb"))["package"]["version"])')
    env -u LD_PRELOAD -u FAKEROOTKEY GSK_RENDERER=cairo APPIMAGE_EXTRACT_AND_RUN=1 RUSTREMOTE_TEST_BACKEND=x11 xvfb-run -a dbus-run-session -- python3 scripts/smoke-profiles.py "dist/RustRemote-${version}-preview-x86_64.AppImage" --startup-only
fi
''')
    prepare = '''set -euo pipefail
root=$1
mount --make-rprivate /
mkdir -p "$root/dev" "$root/proc"
mount -t tmpfs -o mode=755 tmpfs "$root/dev"
for name in null zero random urandom; do touch "$root/dev/$name"; mount --bind "/dev/$name" "$root/dev/$name"; done
mkdir -p "$root/dev/pts"
mount -t devpts -o newinstance,ptmxmode=0666,mode=0600 devpts "$root/dev/pts"
ln -s pts/ptmx "$root/dev/ptmx"
mount -t proc proc "$root/proc"
exec chroot "$root" /usr/bin/env -i PATH=/opt/rust/bin:/usr/bin:/bin HOME=/root DEBIAN_FRONTEND=noninteractive CARGO_HOME=/build/cargo CARGO_NET_OFFLINE=true CARGO_BUILD_JOBS=4 RUSTREMOTE_PORTABLE_WORKER=1 RUSTREMOTE_PORTABLE_VALIDATE=$2 /bin/bash /build/run.sh
'''
    subprocess.run(['unshare', '--user', '--map-root-user', '--mount', '--pid', '--ipc', '--fork', '/bin/bash', '-c', prepare, 'portable-build', str(root), '1' if '--validate' in sys.argv else '0'], check=True)
    output = ROOT / 'dist'; output.mkdir(exist_ok=True)
    version = __import__('tomllib').loads((ROOT / 'Cargo.toml').read_text())['package']['version']
    image = f'RustRemote-{version}-preview-x86_64.AppImage'
    for name in [image, image + '.sha256', f'RustRemote-{version}-ABI.json', 'build-packages.txt']:
        source = work / 'dist' / name
        with tempfile.NamedTemporaryFile(dir=output, delete=False) as temporary:
            target=Path(temporary.name)
            with source.open('rb') as incoming: shutil.copyfileobj(incoming, temporary)
        target.chmod(source.stat().st_mode & 0o777)
        target.replace(output / name)
    print('Portable AppImage built on Debian 12:', output / image, flush=True)

if __name__ == '__main__':
    with (CACHE / 'build.lock').open('w') as handle:
        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        run()
