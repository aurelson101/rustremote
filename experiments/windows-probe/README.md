# RustRemote Windows feasibility probe

Isolated Rust/GTK4 + native ConPTY transport experiment for WIN-01. This is not the
complete Windows application, SSH client, MSI or MSIX installer.
It reuses `rustremote-core` profile models; Linux keeps its existing VTE and protocol adapters.
GPL-3.0-only; root LICENSE/NOTICE apply.

On Windows with an ABI-compatible GTK4 development runtime and Rust toolchain:

```sh
cargo build --locked --manifest-path experiments/windows-probe/Cargo.toml
experiments/windows-probe/target/debug/rustremote-windows-probe.exe --self-test
experiments/windows-probe/target/debug/rustremote-windows-probe.exe --smoke-ui
```

Without arguments the GUI displays a deterministic ANSI preview (no shell) and offers
a local ConPTY self-test on a worker. Its successful output is parsed into the same
selectable GTK/Pango screen, with ANSI colors, bold/italic/underline, cursor and
Unicode cells. The initial preview is a snapshot. “Open local terminal” starts a real local CMD
session on Windows: live ANSI display, Unicode/IME input, terminal keys, measured
GUI resize, copy selection, confirmed paste and disconnect. This explicitly runs
user-entered commands with the current account; it is not a sandbox. The transport
starts only the system cmd.exe with AutoRun disabled, sends fixed built-in commands,
checks input/output UTF-8 and resize, and checks a clean child exit within 10seconds, then waits up to two more seconds for
the output reader to observe the final marker before closing ConPTY.
Output is drained on another thread, limited to64KiB in memory and not logged as a
transcript. It performs no network connection and uses no credential. It is not a
sandbox for arbitrary user commands. The screen parser is pinned to vt100 0.18.1 (MIT), with no scrollback and at most
60×160 cells/64KiB input. Terminal text is escaped before Pango markup; OSC clipboard,
URLs and title changes have no callbacks/actions. Wide/combining characters are
covered at model level; full font/DPI/IME/emoji qualification remains open.
Session I/O runs outside GTK, with bounded input queues and a coalesced latest
screen (no queued transcripts). The reader stays active during console shutdown,
including on older ConPTY versions. A private kill-on-close Job Object contains
the suspended-before-assignment child and descendants. Session close never joins
workers on the GTK thread. OSC buffers are fixed-size (vt100/vte std disabled).
Keyboard/IME/font coverage on real hardware remains to qualify.
Parser API: https://docs.rs/vt100/0.18.1/vt100/ .

The explicit GitHub Actions workflow uses a native Windows runner and MSYS2 UCRT64.
Source dependencies are locked and actions pinned; system package versions are
recorded. This is a feasibility environment, not the final reproducible production
build. GUI smoke proves startup, not visual quality or Windows11 hardware coverage.

The diagnostic artifact contains test/environment reports, not an executable. A
locally built executable needs the matching GTK DLL runtime; it is not a standalone
application or an end-user package. Runtime/dependency notice bundling belongs to
the later distribution step.
Do not publish it as RustRemote's Windows MSI or submit it to WinGet. MSI packaging,
installation/upgrade/uninstall and integration of Windows profile/security adapters
into the application are subsequent work. MSIX and signing follow MSI/WinGet qualification.

ConPTY child startup explicitly uses STARTF_USESTDHANDLES with null standard handles
to avoid copying the parent's redirected CI streams. Background:
https://github.com/microsoft/terminal/discussions/15814 . The CI self-test runs with
redirected stdout intentionally, so the output marker must come from the PTY pipe.

Local validation (Linux host, not native Windows):

```sh
CARGO_TARGET_DIR=target cargo test --locked --manifest-path experiments/windows-probe/Cargo.toml
CARGO_TARGET_DIR=target cargo build --locked --manifest-path experiments/windows-probe/Cargo.toml
GDK_BACKEND=x11 GTK_THEME=Adwaita GSK_RENDERER=cairo xvfb-run -a dbus-run-session -- target/debug/rustremote-windows-probe --smoke-ui
```

The manual Windows workflow uses `windows-2022` (Server 2022 x64) and
`windows-11-arm` (Windows 11 ARM64, x64 executable under emulation). It records
the native OS/CPU separately from the compiler target, and runs parser tests as well as ConPTY validation
and the mapped ANSI GTK screen smoke test. `--self-test-live` verifies streaming
input/resize/clean exit/cancellation. `--smoke-live-ui` verifies the actual IM commit
→ ConPTY → GTK display, window resize and disconnect path. The smoke resizes only its own process window via Windows and checks
the resulting GTK layout and terminal grid. Native run results must be checked before
claiming validation of a particular source commit. Diagnostic artifacts remain reports
only, with no executable/runtime redistribution.

Target baseline: Windows 11 x64 and Windows Server 2022 x64 with Desktop Experience.
Server Core is outside this graphical application target. A Server 2022 CI result
does not certify Windows 11, AD/RDP peripherals, IME hardware or full protocol parity.
The complete Linux application remains separate until the platform adapters and
profile/storage/protocol port have passed their own tests.

Windows profile storage qualification (`--self-test-storage`):

- Shared bounded JSON models, nested folders, portable import/export without credential references.
- Current-user LocalAppData location, protected owner/SYSTEM ACL, exclusive process lock,
  reparse/hard-link rejection and flushed replacement with private backup on failure.
- Optional current-user DPAPI metadata encryption; plaintext metadata is also supported.
- Credential Manager secrets bound to host/port/account/domain and SSH trust configuration.
  Changing the identity refuses the old secret; exports never carry passwords.
- Ephemeral local fixtures exercise Unicode paths, lock contention, plain/encrypted
  saves, corruption rejection, reopen, persisted credential references and vault cleanup.
  No real server, account or user profile is changed by this self-test.

These are backend APIs and tests, not a finished Windows profile manager. GUI wiring,
crash/power-loss recovery qualification, SSH/SFTP, mstsc integration and MSI remain open.
Windows 11 ARM emulation does not qualify Windows 11 x64 hardware, DPI/IME or peripherals.

Backend/terminal qualification at source `10de7c6`:
[run43728388282](https://github.com/aurelson101/rustremote/actions/runs/43728388282)
passed on Server 2022 x64 and Windows 11 Enterprise ARM64 with x64 emulation.
This validates the listed components, not Windows remote protocols or an MSI.
