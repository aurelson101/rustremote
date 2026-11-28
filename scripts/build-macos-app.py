#!/usr/bin/env python3
"""Build and sign a local RustRemote.app from the workspace."""

import argparse
import json
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output-dir", type=Path, default=Path("dist"))
parser.add_argument("--homebrew-runtime", action="store_true",
                    help="link against Homebrew at run time instead of bundling GTK/VTE")
args = parser.parse_args()

HOMEBREW = Path("/opt/homebrew")
PIXBUF_LOADERS = "lib/gdk-pixbuf-2.0/2.10.0/loaders"
CACHE_PLACEHOLDER = "@RUSTREMOTE_PIXBUF_LOADERS@"


def macho_links(path):
    """Return (install_name_or_None, [dependency names]) from otool."""
    lines = subprocess.check_output(["otool", "-L", str(path)], text=True).splitlines()[1:]
    names = [line.strip().split(" (compatibility")[0] for line in lines]
    own = subprocess.check_output(["otool", "-D", str(path)], text=True).splitlines()[1:]
    own = own[0].strip() if own else None
    return own, [name for name in names if name != own]


def rpaths(path):
    found, lines = [], subprocess.check_output(["otool", "-l", str(path)], text=True).splitlines()
    for index, line in enumerate(lines):
        if line.strip() == "cmd LC_RPATH":
            entry = lines[index + 2].strip().split(" (offset")[0].removeprefix("path ")
            found.append(entry.replace("@loader_path", str(Path(path).resolve().parent)))
    return found


def resolve(reference, referrer):
    if reference.startswith(str(HOMEBREW)):
        return Path(reference).resolve()
    if reference.startswith("@rpath/"):
        name = reference.removeprefix("@rpath/")
        for base in rpaths(referrer) + [str(HOMEBREW / "lib")]:
            candidate = Path(base) / name
            if candidate.exists():
                return candidate.resolve()
        raise SystemExit(f"Cannot resolve {reference} from {referrer}")
    if reference.startswith("@loader_path/"):
        return (Path(referrer).resolve().parent / reference.removeprefix("@loader_path/")).resolve()
    return None  # system library


def minimum_macos(paths):
    """Highest LC_BUILD_VERSION minos among the given Mach-O files."""
    best = (11, 0)
    for path in paths:
        lines = subprocess.check_output(["otool", "-l", str(path)], text=True).splitlines()
        for line in lines:
            parts = line.split()
            if len(parts) == 2 and parts[0] == "minos":
                version = tuple(int(x) for x in parts[1].split("."))[:2]
                best = max(best, version)
    return ".".join(map(str, best))


def bundle_runtime(contents, executables, resources):
    """Copy every non-system dylib into Frameworks and make the bundle relocatable."""
    frameworks = contents / "Frameworks"
    frameworks.mkdir()
    loaders_dst = resources / PIXBUF_LOADERS
    loaders_dst.mkdir(parents=True)
    for loader in (HOMEBREW / PIXBUF_LOADERS).iterdir():
        if loader.suffix == ".so":
            shutil.copy2(loader.resolve(), loaders_dst / loader.name)
    cache = (HOMEBREW / PIXBUF_LOADERS).parent / "loaders.cache"
    text = cache.read_text().replace(str(HOMEBREW / PIXBUF_LOADERS), CACHE_PLACEHOLDER)
    (loaders_dst.parent / "loaders.cache.in").write_text(text)
    schemas = resources / "share/glib-2.0/schemas"
    schemas.mkdir(parents=True)
    shutil.copy2(HOMEBREW / "share/glib-2.0/schemas/gschemas.compiled", schemas)
    icons = HOMEBREW / "share/icons"
    for theme in ("hicolor", "Adwaita"):
        if (icons / theme).is_dir():
            shutil.copytree(icons / theme, resources / "share/icons" / theme, symlinks=False)
    (resources / "lib/gio/modules").mkdir(parents=True)

    roots = [executables / "rustremote", executables / "rustremote-askpass",
             *sorted(loaders_dst.iterdir())]
    copied, queue = {}, [(path, path) for path in roots]
    while queue:
        path, origin = queue.pop()
        for reference in macho_links(path)[1]:
            source = resolve(reference, origin)
            if source is None:
                continue
            # Name the copy after the reference (often a versioned symlink),
            # because that is the name rewritten into every referrer below.
            name = Path(reference).name
            target = frameworks / name
            if copied.setdefault(name, source) != source:
                raise SystemExit(f"Two libraries named {name}: {copied[name]} and {source}")
            if not target.exists():
                shutil.copy2(source, target)
                target.chmod(0o755)
                queue.append((target, source))
    if len(set(copied.values())) != len(copied):
        raise SystemExit("A library would be loaded twice under different names")
    for path in roots + sorted(frameworks.iterdir()):
        own, references = macho_links(path)
        command = ["install_name_tool"]
        if own and path.parent == frameworks:
            command += ["-id", f"@executable_path/../Frameworks/{path.name}"]
        for reference in references:
            if reference.startswith(("/opt/", "@rpath/", "@loader_path/")):
                command += ["-change", reference,
                            f"@executable_path/../Frameworks/{Path(reference).name}"]
        if len(command) > 1:
            subprocess.run(command + [str(path)], check=True, stderr=subprocess.DEVNULL)
        subprocess.run(["codesign", "--force", "--sign", "-", str(path)],
                       check=True, stderr=subprocess.DEVNULL)
    leftovers = [str(p) for p in roots + sorted(frameworks.iterdir())
                 if any(r.startswith(("/opt/", "@rpath/")) for r in macho_links(p)[1])]
    if leftovers:
        raise SystemExit(f"Unbundled references remain: {leftovers}")
    print(f"Bundled {len(copied)} libraries", file=sys.stderr)
if sys.platform != "darwin":
    parser.error("macOS is required to build the application bundle")

root = Path(__file__).resolve().parent.parent
destination = args.output_dir.resolve() / "RustRemote.app"
if destination.exists() or destination.is_symlink():
    parser.error(f"Bundle already exists: {destination}")

metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--format-version=1", "--no-deps", "--locked"], cwd=root
))
version = next(p["version"] for p in metadata["packages"] if p["name"] == "rustremote")
binaries = Path(metadata["target_directory"]) / "release"
subprocess.run(["cargo", "build", "--release", "--locked", "--bins"], cwd=root, check=True)
destination.parent.mkdir(parents=True, exist_ok=True)

with tempfile.TemporaryDirectory(prefix=".rustremote-build-", dir=destination.parent) as directory:
    staging = Path(directory)
    bundle = staging / destination.name
    contents = bundle / "Contents"
    executables = contents / "MacOS"
    resources = contents / "Resources"
    executables.mkdir(parents=True)
    resources.mkdir()
    for name in ("rustremote", "rustremote-askpass"):
        shutil.copy2(binaries / name, executables / name)
    if not args.homebrew_runtime:
        bundle_runtime(contents, executables, resources)

    iconset = staging / "RustRemote.iconset"
    iconset.mkdir()
    logo = root / "assets/icons/rustremote.png"
    for size in (16, 32, 128, 256, 512):
        for scale, suffix in ((1, ""), (2, "@2x")):
            pixels = str(size * scale)
            rendition = iconset / f"icon_{size}x{size}{suffix}.png"
            subprocess.run(["sips", "-z", pixels, pixels, str(logo), "--out", str(rendition)],
                           stdout=subprocess.DEVNULL, check=True)
    subprocess.run(["iconutil", "-c", "icns", str(iconset), "-o", str(resources / "RustRemote.icns")], check=True)

    with (contents / "Info.plist").open("wb") as plist:
        plistlib.dump({
            "CFBundleDisplayName": "RustRemote",
            "CFBundleExecutable": "rustremote",
            "CFBundleIconFile": "RustRemote.icns",
            "CFBundleIdentifier": "com.rustremote.Preview",
            "CFBundleName": "RustRemote",
            "CFBundlePackageType": "APPL",
            "CFBundleShortVersionString": version,
            "CFBundleVersion": version,
            "NSHighResolutionCapable": True,
            "LSMinimumSystemVersion": minimum_macos(
                [*executables.iterdir(), *(contents / "Frameworks").glob("*")]),
        }, plist)

    subprocess.run(["codesign", "--force", "--sign", "-", str(executables / "rustremote-askpass")], check=True)
    subprocess.run(["codesign", "--force", "--sign", "-", str(bundle)], check=True)
    subprocess.run(["codesign", "--verify", "--strict", "--deep", str(bundle)], check=True)
    bundle.rename(destination)

print(destination)
