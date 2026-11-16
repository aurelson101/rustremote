# RustRemote Windows feasibility probe

Rust/GTK4 Windows integration preview: local ConPTY terminal, saved profiles and
OpenSSH connections, including macOS/Apple SSH profiles. This is not yet the complete
Windows application, RDP/VNC client, MSI or MSIX installer. An experimental
SFTP browser is now connected to the shared backend.
It reuses `rustremote-core` profile models; Linux keeps its existing VTE and protocol adapters.
GPL-3.0-only; root LICENSE/NOTICE apply.

On Windows with an ABI-compatible GTK4 development runtime and Rust toolchain:

```sh
cargo build --locked --manifest-path experiments/windows-probe/Cargo.toml
experiments/windows-probe/target/debug/rustremote-windows-probe.exe --self-test
experiments/windows-probe/target/debug/rustremote-windows-probe.exe --smoke-ui
```

Without arguments the GUI displays a deterministic ANSI preview (no shell) and offers
a local ConPTY self-test on a worker. On Windows, “Manage connections…” opens the
persistent profile manager: SSH/RDP metadata, domains, SSH file pickers, nested
logical folders, edit/delete, favorites, JSON import/export and optional DPAPI.
Password fields write references to Windows Credential Manager; no password is
exported. Closing during a save is refused until the worker commits or reports
an error. “Connect selected” opens an SSH terminal for an SSH profile. RDP/VNC launch is not yet connected. Its successful output is parsed into the same
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

The profile GUI is connected to these APIs. `--smoke-profiles-ui` runs in a fresh
temporary store and cleans up generated vault entries; on Linux it uses an explicit
simulated backend and does not validate DPAPI or Credential Manager. Native profile
GUI results must be checked for the corresponding source commit. Crash/power-loss
recovery qualification, SSH/SFTP, mstsc integration and MSI remain open.
Windows 11 ARM emulation does not qualify Windows 11 x64 hardware, DPI/IME or peripherals.

Backend/terminal qualification at source `10de7c6`:
[run43728388282](https://github.com/aurelson101/rustremote/actions/runs/43728388282)
passed on Server 2022 x64 and Windows 11 Enterprise ARM64 with x64 emulation.
This validates the listed components, not Windows remote protocols or an MSI.

Profile manager integration at source `26702dc`:
[run43731588185](https://github.com/aurelson101/rustremote/actions/runs/43731588185)
passed on both runners, including 12 prototype/service tests and the temporary GUI
profile/vault workflow with fatal GTK warnings. Save stays outside the scrolled
form. Native file chooser interaction, physical input and remote protocol launch
remain outside this qualification. No MSI has been produced.

## SSH integration qualification — 2026-11-16

[run43822234477](https://github.com/aurelson101/rustremote/actions/runs/43822234477),
source `6de7344`, passed on Server 2022 x64 and Windows 11 ARM64 with x64 emulation.
The real loopback SSH laboratory verifies password authentication from Credential
Manager through askpass, encrypted RSA key authentication with Unicode/spaced file
paths, terminal UTF-8, PTY resize and cancellation. Wrong passwords are refused;
unknown/changed host keys are refused before any authentication request.
The Apple profile talks to a simulated SSH peer, not a real Mac or an ARD server.

The launch policy invokes an absolute trusted OpenSSH path, with no intermediate
shell. A bundled `OpenSSH/ssh.exe` beside the application takes precedence over the
Windows system client. CI uses the official Win32-OpenSSH 10.4.0.0p1-Preview package,
verified by SHA256; its source/version is recorded. CI artifacts contain reports
and own-window screenshots, not binaries or runtime redistribution. Inbox clients
and hardware-specific behavior have not been separately qualified.

Host verification is strict; existing/default or explicitly selected `known_hosts`
is required. Unknown keys must first be verified and added by the user. Forwarding,
agent forwarding and local commands are disabled. Implicit config is disabled with
`-F none`; custom config and ProxyJump are explicitly refused in this increment.
Agent usage remains to qualify. A selected private key uses public-key authentication
only, so its passphrase is never retried as a server password. Stored SSH secrets
are limited to 1000 UTF-8 bytes. Passwords enter neither argv nor environment: only
profile metadata and a scoped vault reference are passed to the askpass helper.

The connection flow is wired to the profile manager; the network qualification
exercises the transport through the CLI rather than physical Windows GUI input.
Final SFTP GUI qualification, RDP, Apple graphical sharing, hardware/DPI/endurance
and MSI remain open.

## SFTP integration — 2026-11-16 (partial qualification)

`Open SFTP` uses the shared Rust backend with an explicit Windows filesystem
boundary: exclusive private files, no replacement of existing downloads, refusal
of reparse points, device namespaces, reserved DOS names and alternate streams.
Encrypted key data is read through Rust with a read-only ACL check, then passed
in memory to libssh2; user key permissions are never rewritten. Custom SSH
configuration and proxies remain refused by the direct Windows adapter.

Server 2022 [run43830077411](https://github.com/aurelson101/rustremote/actions/runs/43830077411)
passes all five native SFTP transport scenarios, including encrypted keys and
unknown/changed host rejection before authentication. GUI transfer/edit/delete
operations completed without the previous IME critical after releasing dialog
focus; that run failed on its empty screenshot, so it is not an overall pass.
A rendered-frame capture guard is being qualified. Linux isolated GUI and editor
labs pass against a strict SFTP v3 simulator. No real Mac was contacted.

Remote editing now negotiates `posix-rename@openssh.com` version1 on a separate
channel of the same authenticated session. Ordinary SFTP v3 rename is insufficient
for atomic replacement. The small extension parser caps packets at64KiB, validates
version/request/status and refuses unsupported servers without deleting the
original. Server-provided error text is not logged. The stale-content check is
best effort, not a transactional lock against concurrent remote writers.

CI currently links MSYS2 libssh2/OpenSSL through pkg-config because the Rust
Windows OpenSSL feature otherwise requests unavailable static archives. These
runtime libraries are not redistributed. Review/update their security advisories
and package licenses before any MSI build. Defaults remain16MiB/128entries/8levels/
30seconds; Windows queue persistence, resumable GUI controls, drag-and-drop,
configurable limits and physical/DPI/IME/UNC qualification remain future work.
