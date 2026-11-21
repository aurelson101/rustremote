#!/usr/bin/env python3
"""Stage the Windows preview and the UCRT libraries it imports."""
import re
import shutil
import subprocess
import sys
from pathlib import Path

prefix, executable, output = map(Path, sys.argv[1:])
output.mkdir(parents=True, exist_ok=False)
runtime = prefix / "bin"
libraries = {path.name.lower(): path for path in runtime.glob("*.dll")}
pending = [executable]
copied = set()
while pending:
    source = pending.pop()
    name = source.name.lower()
    if name in copied:
        continue
    copied.add(name)
    shutil.copy2(source, output / source.name)
    headers = subprocess.check_output([str(runtime / "objdump.exe"), "-p", str(source)], text=True)
    for name in re.findall(r"DLL Name:\s*(\S+)", headers, re.IGNORECASE):
        if name.lower() in libraries:
            pending.append(libraries[name.lower()])

for relative in ("share/glib-2.0/schemas", "share/icons/hicolor", "share/licenses"):
    source = prefix / relative
    if source.is_dir():
        shutil.copytree(source, output / relative)
for name in ("LICENSE", "NOTICE"):
    shutil.copy2(name, output / name)
shutil.copy2("experiments/windows-probe/README.md", output / "README.md")
