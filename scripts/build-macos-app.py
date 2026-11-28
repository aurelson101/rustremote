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
args = parser.parse_args()
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
        }, plist)

    subprocess.run(["codesign", "--force", "--sign", "-", str(executables / "rustremote-askpass")], check=True)
    subprocess.run(["codesign", "--force", "--sign", "-", str(bundle)], check=True)
    subprocess.run(["codesign", "--verify", "--strict", "--deep", str(bundle)], check=True)
    bundle.rename(destination)

print(destination)
