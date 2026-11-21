#!/usr/bin/env python3
"""Build a pinned IronRDP example as a disposable CI peer."""
import hashlib
import io
from pathlib import Path
import subprocess
import sys
import tarfile
import urllib.request

REVISION = "11a0810cfbbabd8b8023875a05e3041216d4b01b"
SHA256 = "733bc2c61d13f63d7f4103baa2458d8185c0d54d98a72ea7d2098f92f0f47b57"
LIMIT = 20 * 1024 * 1024


def replace(path, before, after):
    text = path.read_text(encoding="utf-8")
    if text.count(before) != 1:
        raise ValueError("Pinned reference source differs")
    path.write_text(text.replace(before, after), encoding="utf-8")


def main(root):
    root = root.resolve()
    root.mkdir(parents=True, exist_ok=False)
    url = f"https://codeload.github.com/Devolutions/IronRDP/tar.gz/{REVISION}"
    with urllib.request.urlopen(url, timeout=30) as response:
        archive_data = response.read(LIMIT + 1)
    if len(archive_data) > LIMIT or hashlib.sha256(archive_data).hexdigest() != SHA256:
        raise ValueError("Reference source hash mismatch")
    with tarfile.open(fileobj=io.BytesIO(archive_data), mode="r:gz") as archive:
        archive.extractall(root, filter="data")
    source = root / f"IronRDP-{REVISION}"
    example = source / "crates/ironrdp/examples/server.rs"
    replace(example, 'let pass = args.opt_value_from_str("--pass")?.unwrap_or_else(|| "pass".to_owned());', """let mut pass = String::new();
        use std::io::Read as _;
        std::io::stdin().take(129).read_to_string(&mut pass)?;
        anyhow::ensure!(!pass.is_empty() && pass.len() <= 128, "Invalid fixture password");
        anyhow::ensure!(bind_addr.ip() == std::net::Ipv4Addr::LOCALHOST && bind_addr.port() == 0
                        && hybrid && cert.is_some() && key.is_some() && user == "rr-lab", "Invalid fixture arguments");""")
    replace(example, "domain: None,", 'domain: Some("RRTEST".to_owned()),')
    server = source / "crates/ironrdp-server/src/server.rs"
    replace(server, 'debug!("Listening for connections on {local_addr}");',
            'println!("RR_REFERENCE_READY {}", local_addr.port());')
    replace(server, 'debug!("Client accepted");', 'println!("RR_REFERENCE_ACTIVATED");')
    subprocess.run(
        ["cargo", "build", "--locked", "--target-dir", str(root / "target"), "-p", "ironrdp",
         "--example", "server", "--features", "cliprdr,connector,rdpsnd,server"],
        cwd=source, check=True, timeout=720,
    )


if __name__ == "__main__":
    main(Path(sys.argv[1]))
