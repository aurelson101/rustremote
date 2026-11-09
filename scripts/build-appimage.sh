#!/usr/bin/env bash
# Native worker; default builds use a pinned older Linux userspace.
set -euo pipefail
cd "$(dirname "$(readlink -f "$0")")/.."
if [[ ${RUSTREMOTE_PORTABLE_WORKER:-0} != 1 && ${RUSTREMOTE_NATIVE_BUILD:-0} != 1 ]]; then
    exec python3 scripts/build-portable-appimage.py
fi
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
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

appdir = Path(sys.argv[1])
clean_env = dict(os.environ)
for name in ('LD_PRELOAD', 'LD_LIBRARY_PATH'):
    clean_env.pop(name, None)
loader_dir = Path(subprocess.check_output(['pkg-config', '--variable=gdk_pixbuf_moduledir', 'gdk-pixbuf-2.0'], text=True).strip())
svg_loader = loader_dir / 'libpixbufloader-svg.so'
if not svg_loader.is_file():
    raise SystemExit('SVG loader missing: install librsvg2-common')
modules = appdir / 'usr/lib/gdk-pixbuf-loaders'
modules.mkdir()
shutil.copy2(svg_loader, modules / svg_loader.name)
query = subprocess.check_output(['pkg-config', '--variable=gdk_pixbuf_query_loaders', 'gdk-pixbuf-2.0'], text=True).strip()
cache = subprocess.check_output([query, str(svg_loader)], env=clean_env, text=True)
cache = cache.replace(str(svg_loader), '$ORIGIN/gdk-pixbuf-loaders/' + svg_loader.name)
(modules / 'loaders.cache').write_text(cache)
(appdir / 'usr/lib/gio/modules').mkdir(parents=True)
gtk_libdir = Path(subprocess.check_output(['pkg-config', '--variable=libdir', 'gtk4'], text=True).strip())
gtk_version = subprocess.check_output(['pkg-config', '--variable=gtk_binary_version', 'gtk4'], text=True).strip()
im_sources = list((gtk_libdir / 'gtk-4.0' / gtk_version / 'immodules').glob('*.so'))
im_target = appdir / 'usr/lib/gtk-4.0' / gtk_version / 'immodules'
im_target.mkdir(parents=True)
for module in im_sources: shutil.copy2(module, im_target / module.name)
query_modules = subprocess.check_output(['pkg-config', '--variable=gio_querymodules', 'gio-2.0'], text=True).strip()
subprocess.run([query_modules, str(im_target)], env=clean_env, check=True)
output = '\n'.join(subprocess.check_output(['ldd', str(binary)], env=clean_env, text=True)
                   for binary in [Path('target/release/rustremote'), svg_loader, *im_sources])
if 'not found' in output:
    raise SystemExit('Dépendance système introuvable : ' + output)
# Conserver glibc et le chargeur du système cible ; inclure les autres liens ELF.
excluded = {'libc.so.6', 'libm.so.6', 'libdl.so.2', 'libpthread.so.0',
            'librt.so.1', 'libresolv.so.2',
            # Match the host graphics driver and its C++/Wayland ABI.
            'libEGL.so.1', 'libGL.so.1', 'libGLX.so.0', 'libGLdispatch.so.0',
            'libOpenGL.so.0', 'libdrm.so.2', 'libgbm.so.1', 'libglapi.so.0',
            'libstdc++.so.6', 'libgcc_s.so.1',
            'libwayland-client.so.0', 'libwayland-cursor.so.0', 'libwayland-egl.so.1'}
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
for module in [svg_loader, *im_sources]:
    owner = subprocess.check_output(['dpkg-query', '-S', str(module)], text=True).split(': ')[0].split(':')[0]
    shutil.copy2(Path('/usr/share/doc') / owner / 'copyright', appdir / 'usr/share/licenses' / (owner + '.copyright'))
    manifest.append('GTK/SVG module: ' + owner)
versions = []
for binary in [* (appdir / 'usr/bin').iterdir(), * (appdir / 'usr/lib').iterdir(), modules / svg_loader.name, *im_target.glob('*.so')]:
    if not binary.is_file(): continue
    with binary.open('rb') as stream:
        if stream.read(4) != b'\x7fELF': continue
    version_info = subprocess.check_output(['readelf', '--version-info', str(binary)], env=clean_env, text=True)
    versions += [tuple(map(int, v.split('.'))) for v in re.findall(r'Name: GLIBC_([0-9.]+)', version_info)]
minimum = max(versions)
if os.environ.get('RUSTREMOTE_PORTABLE_WORKER') == '1' and minimum > (2, 36):
    raise SystemExit('Portable build requires a newer glibc than the selected baseline')
abi = {'architecture': 'x86_64', 'minimum_glibc': '.'.join(map(str, minimum)),
       'gtk': subprocess.check_output(['pkg-config', '--modversion', 'gtk4'], text=True).strip(),
       'vte': subprocess.check_output(['pkg-config', '--modversion', 'vte-2.91-gtk4'], text=True).strip()}
(appdir / 'usr/share/licenses/ABI.json').write_text(json.dumps(abi, indent=2) + '\n')
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
export GDK_PIXBUF_MODULE_FILE="$appdir/usr/lib/gdk-pixbuf-loaders/loaders.cache"
export GIO_MODULE_DIR="$appdir/usr/lib/gio/modules"
export GTK_PATH="$appdir/usr/lib/gtk-4.0"
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
cp "$appdir/usr/share/licenses/ABI.json" "dist/RustRemote-${version}-ABI.json"
