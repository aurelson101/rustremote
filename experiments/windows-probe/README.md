# RustRemote Windows feasibility probe

Isolated Rust/GTK4 + native ConPTY transport experiment for WIN-01. This is not the
complete Windows application, terminal renderer, SSH client, MSI or MSIX installer.
It does not alter the Linux application. GPL-3.0-only; root LICENSE/NOTICE apply.

On Windows with an ABI-compatible GTK4 development runtime and Rust toolchain:

```sh
cargo build --locked --manifest-path experiments/windows-probe/Cargo.toml
experiments/windows-probe/target/debug/rustremote-windows-probe.exe --self-test
experiments/windows-probe/target/debug/rustremote-windows-probe.exe --smoke-ui
```

Without arguments the GUI offers a local ConPTY self-test on a worker. The transport
starts only the system cmd.exe with AutoRun disabled, sends fixed built-in commands,
checks input/output UTF-8 and resize, and checks a clean child exit within 10seconds.
Output is drained on another thread, limited to64KiB in memory and not logged as a
transcript. It performs no network connection and uses no credential. It is not a
sandbox for arbitrary user commands. No interactive ANSI renderer is implemented.

The explicit GitHub Actions workflow uses a native Windows runner and MSYS2 UCRT64.
Source dependencies are locked and actions pinned; system package versions are
recorded. This is a feasibility environment, not the final reproducible production
build. GUI smoke proves startup, not visual quality or Windows11 hardware coverage.

The diagnostic artifact contains the probe executable and reports. It requires the
matching GTK DLL runtime; it is not a standalone application or an end-user package.
Do not publish it as RustRemote's Windows MSI or submit it to WinGet. MSI packaging,
installation/upgrade/uninstall, Windows profile/security adapters and the terminal
renderer are subsequent work. MSIX and signing follow MSI/WinGet qualification.
