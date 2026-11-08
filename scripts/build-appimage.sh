#!/usr/bin/env bash
# Prototype construit pour Ubuntu 26.04 x86_64, pas encore une publication portable.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/.."
# Repli non privilégié : en-têtes APT extraits localement, bibliothèques système inchangées.
native_dev="$PWD/.tools/native-dev/root"
if ! pkg-config --exists openssl libssh2 && [[ -d "$native_dev/usr/lib/x86_64-linux-gnu/pkgconfig" ]]; then
    export PKG_CONFIG_PATH="$native_dev/usr/lib/x86_64-linux-gnu/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
    export CFLAGS="-I$native_dev/usr/include/x86_64-linux-gnu${CFLAGS:+ $CFLAGS}"
fi
[[ $(uname -m) == x86_64 ]] || { echo 'Architecture x86_64 requise' >&2; exit 1; }
cargo build --release --locked
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
output="RustRemote-${version}-preview-x86_64.AppImage"
mkdir -p .tools dist
# Empreinte publiée par l'API GitHub AppImage/appimagetool, relevée le 2026-11-07.
tool=.tools/appimagetool-x86_64.AppImage
if [[ ! -f $tool ]]; then
    curl --fail --location --proto '=https' --tlsv1.2 \
        https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage \
        -o "$tool"
fi
printf '%s  %s\n' e8c1169c4af66c3184d7ee110713d4be04befe671fa358e2c1591ee840553b03 "$tool" | sha256sum --check
chmod +x "$tool"
# Empreinte de l'asset officiel AppImage/type2-runtime (API GitHub, 2026-11-08).
# Le cache vérifié évite le téléchargement implicite à chaque construction.
runtime=.tools/runtime-x86_64
runtime_sha=617aa77314d57c8b8e4fae9c3f0b35af7b30d5529cb69e98f64f92510ed98a46
if [[ ! -f $runtime ]]; then
    runtime_download=$(mktemp "$PWD/.tools/.runtime-x86_64.XXXXXX")
    trap 'rm -f -- "$runtime_download"' EXIT
    curl --fail --location --retry 3 --proto '=https' --tlsv1.2 \
        https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-x86_64 \
        -o "$runtime_download"
    printf '%s  %s\n' "$runtime_sha" "$runtime_download" | sha256sum --check
    mv -f -- "$runtime_download" "$runtime"
    trap - EXIT
fi
printf '%s  %s\n' "$runtime_sha" "$runtime" | sha256sum --check
# Un nouvel AppDir à chaque build, sans suppression d'un répertoire existant.
appdir=$(mktemp -d "$PWD/target/RustRemote.AppDir.XXXXXX")
chmod 755 "$appdir"
mkdir -p "$appdir/usr/bin" "$appdir/usr/lib" "$appdir/usr/share/licenses"
cp target/release/rustremote target/release/rustremote-askpass "$appdir/usr/bin/"
cp packaging/rustremote.desktop packaging/rustremote.png "$appdir/"
cp packaging/rustremote.png "$appdir/.DirIcon"
python3 - "$appdir" <<'PY'
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys

appdir = Path(sys.argv[1])
output = subprocess.check_output(['ldd', 'target/release/rustremote'], text=True)
if 'not found' in output:
    raise SystemExit('Dépendance système introuvable : ' + output)
# Conserver glibc et le chargeur du système cible ; inclure les autres liens ELF.
excluded = {'libc.so.6', 'libm.so.6', 'libdl.so.2', 'libpthread.so.0',
            'librt.so.1', 'libresolv.so.2'}
manifest = []
for name, path in re.findall(r'^\s*(\S+) => (/\S+)', output, re.M):
    if name in excluded:
        continue
    shutil.copy2(path, appdir / 'usr/lib' / name)
    manifest.append(f'{name}: {path}')
    result = subprocess.run(['dpkg-query', '-S', path], capture_output=True, text=True)
    if result.returncode != 0:
        result = subprocess.run(['dpkg-query', '-S', str(Path(path).resolve())],
                                capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit('Paquet de licence introuvable : ' + path)
    package = result.stdout.split(': ')[0].split(':')[0]
    copyright_file = Path('/usr/share/doc') / package / 'copyright'
    if not copyright_file.is_file():
        raise SystemExit('Copyright introuvable : ' + str(copyright_file))
    shutil.copy2(copyright_file, appdir / 'usr/share/licenses' / (package + '.copyright'))
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version=1', '--filter-platform=x86_64-unknown-linux-gnu']))
for package in metadata['packages']:
    manifest.append(f"{package['name']} {package['version']}: {package.get('license') or package.get('license_file') or 'licence non renseignée'}")
    if package['name'] == 'rustremote':
        for notice in ('LICENSE', 'NOTICE'):
            shutil.copy2(notice, appdir / 'usr/share/licenses' / ('RustRemote-' + notice))
        continue
    folder = Path(package['manifest_path']).parent
    licenses = [p for p in folder.iterdir() if p.is_file() and p.name.upper().startswith(('LICENSE', 'COPYING', 'COPYRIGHT'))]
    if not licenses:
        licenses = [p for p in Path('packaging/third-party').glob(f"{package['name']}-{package['version']}-LICENSE*") if not p.name.endswith('.source')]
    if not licenses:
        raise SystemExit('Licence crate introuvable : ' + package['name'])
    for source in licenses:
        shutil.copy2(source, appdir / 'usr/share/licenses' / f"{package['name']}-{source.name}")
(appdir / 'usr/share/licenses/MANIFEST.txt').write_text('\n'.join(manifest) + '\n')
PY
cat > "$appdir/AppRun" <<'APPRUN'
#!/bin/sh
set -eu
appdir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
export LD_LIBRARY_PATH="$appdir/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$appdir/usr/bin/rustremote" "$@"
APPRUN
chmod +x "$appdir/AppRun"
temporary_output=$(mktemp "$PWD/dist/.RustRemote-${version}-XXXXXX.AppImage")
trap 'rm -f -- "$temporary_output"' EXIT
ARCH=x86_64 VERSION="${version}-preview" APPIMAGE_EXTRACT_AND_RUN=1 "$tool" \
    --runtime-file "$PWD/$runtime" "$appdir" "$temporary_output"
# Remplacement atomique : les anciennes instances montées gardent leur image intacte.
mv -f -- "$temporary_output" "$PWD/dist/$output"
(cd dist && sha256sum "$output" > "$output.sha256")
