#!/usr/bin/env python3
"""Ephemeral loopback SSH/Apple-profile lab; never starts a shell or a system account."""
import argparse
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import sys
import tempfile
import threading
import time
import paramiko


class Case:
    def __init__(self, password, key):
        self.password, self.key = password, key
        self.state = dict(password_ok=0, password_rejected=0, key_ok=0, key_rejected=0,
                          pty=0, resize=0, probe=False)


class Session(paramiko.ServerInterface):
    def __init__(self, case):
        self.case, self.shell = case, threading.Event()

    def get_allowed_auths(self, username):
        return "publickey,password"

    def check_auth_password(self, username, password):
        allowed = username == "lab@realm.example" and secrets.compare_digest(password, self.case.password)
        self.case.state["password_ok" if allowed else "password_rejected"] += 1
        return paramiko.AUTH_SUCCESSFUL if allowed else paramiko.AUTH_FAILED

    def check_auth_publickey(self, username, key):
        allowed = username == "lab@realm.example" and key.asbytes() == self.case.key.asbytes()
        self.case.state["key_ok" if allowed else "key_rejected"] += 1
        return paramiko.AUTH_SUCCESSFUL if allowed else paramiko.AUTH_FAILED

    def check_channel_request(self, kind, channel_id):
        return paramiko.OPEN_SUCCEEDED if kind == "session" else paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED

    def check_channel_pty_request(self, *args):
        self.case.state["pty"] += 1
        return True

    def check_channel_window_change_request(self, *args):
        self.case.state["resize"] += 1
        return True

    def check_channel_shell_request(self, channel):
        self.shell.set()
        return True


class Server:
    def __init__(self, key):
        self.key = key
        self.listener = socket.socket()
        self.listener.bind(("127.0.0.1", 0))
        self.port = self.listener.getsockname()[1]
        self.listener.listen(4)
        self.listener.settimeout(.2)
        self.stop = threading.Event()
        self.case = None
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()

    def run(self):
        while not self.stop.is_set():
            try:
                conn, _ = self.listener.accept()
            except socket.timeout:
                continue
            except OSError:
                return
            transport = paramiko.Transport(conn)
            transport.local_version = "SSH-2.0-RustRemote_Apple_SIMULATION"
            transport.add_server_key(self.key)
            handler = Session(self.case)
            try:
                transport.start_server(server=handler)
                channel = transport.accept(12)
                if channel is None or not handler.shell.wait(5):
                    continue
                channel.settimeout(.2)
                channel.sendall(b"RUSTREMOTE_SSH_READY\r\n")
                data, deadline = b"", time.monotonic() + 12
                while transport.is_active() and time.monotonic() < deadline and not self.stop.is_set():
                    try:
                        part = channel.recv(1024)
                    except socket.timeout:
                        continue
                    if not part:
                        break
                    data += part
                    if len(data) > 4096:
                        break
                    if b"\r" in data or b"\n" in data:
                        if data.strip() == "probe-é".encode():
                            self.case.state["probe"] = True
                            # Wait briefly for the actual SSH window-change callback, never execute input.
                            until = time.monotonic() + 2
                            while not self.case.state["resize"] and time.monotonic() < until:
                                time.sleep(.02)
                            channel.sendall("RUSTREMOTE_SSH_OK_été\r\n".encode())
                        data = b""
            except (EOFError, OSError, paramiko.SSHException):
                pass  # Rejected host keys close before authentication; expected negative scenarios.
            finally:
                transport.close()

    def close(self):
        self.stop.set()
        self.listener.close()
        self.thread.join(3)
        if self.thread.is_alive():
            raise RuntimeError("SSH laboratory failed to stop")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--probe", type=Path)
    parser.add_argument("--verify-fixture", action="store_true")
    args = parser.parse_args()
    if not args.verify_fixture and (sys.platform != "win32" or args.probe is None):
        parser.error("native qualification requires Windows and --probe")
    host_key, client_key = paramiko.RSAKey.generate(2048), paramiko.RSAKey.generate(2048)
    server = Server(host_key)
    reports = []
    try:
        with tempfile.TemporaryDirectory(prefix="RustRemote Apple é ") as folder:
            root = Path(folder)
            known, wrong, unknown = root / "known hosts", root / "changed host", root / "unknown host"
            def trust(path, key):
                path.write_text(f"[127.0.0.1]:{server.port} {key.get_name()} {key.get_base64()}\n", encoding="utf-8")
            trust(known, host_key)
            trust(wrong, paramiko.RSAKey.generate(2048))
            unknown.write_text("", encoding="utf-8")
            password = secrets.token_urlsafe(32)
            key_path = root / "clé privée SSH"
            client_key.write_private_key_file(str(key_path), password=password)
            if sys.platform == "win32":
                # MSYS2 can add an explicit OWNER RIGHTS ACE. Replacing only inherited ACEs
                # leaves it behind on Windows 11; use a fresh DACL on this generated lab key.
                acl_script = """
$ErrorActionPreference = 'Stop'
$owner = [Security.Principal.WindowsIdentity]::GetCurrent().User
$acl = [Security.AccessControl.FileSecurity]::new()
$acl.SetOwner($owner)
$acl.SetAccessRuleProtection($true, $false)
$rule = [Security.AccessControl.FileSystemAccessRule]::new($owner, [Security.AccessControl.FileSystemRights]::Read, [Security.AccessControl.AccessControlType]::Allow)
$acl.AddAccessRule($rule)
Set-Acl -LiteralPath $env:RUSTREMOTE_LAB_KEY -AclObject $acl
$actual = Get-Acl -LiteralPath $env:RUSTREMOTE_LAB_KEY
$rules = @($actual.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]))
if (-not $actual.AreAccessRulesProtected -or $rules.Count -ne 1 -or $rules[0].IdentityReference -ne $owner) { throw 'Lab private-key DACL verification failed' }
"""
                environment = dict(os.environ, RUSTREMOTE_LAB_KEY=str(key_path))
                subprocess.run(["pwsh.exe", "-NoProfile", "-NonInteractive", "-Command", acl_script],
                               env=environment, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            else:
                key_path.chmod(0o600)
            if args.verify_fixture:
                server.case = Case(password, client_key)
                client = paramiko.SSHClient()
                client.load_host_keys(str(known))
                client.connect("127.0.0.1", server.port, "lab@realm.example", password=password,
                               allow_agent=False, look_for_keys=False, timeout=5)
                channel = client.invoke_shell(width=80, height=24)
                channel.settimeout(5)
                assert b"RUSTREMOTE_SSH_READY" in channel.recv(4096)
                channel.resize_pty(width=92, height=28)
                channel.sendall("probe-é\r".encode())
                assert "RUSTREMOTE_SSH_OK_été".encode() in channel.recv(4096)
                client.close()
                print(json.dumps({"fixture": "PASS", "simulation": "Apple SSH protocol, not a real Mac", **server.case.state}))
            else:
                probe = args.probe.resolve(strict=True)
                for name, trust_path, key, secret, reject in [
                    ("password", known, None, password, False),
                    ("encrypted_key", known, str(key_path), password, False),
                    ("wrong_password", known, None, secrets.token_urlsafe(32), True),
                    ("changed_host", wrong, None, password, True),
                    ("unknown_host", unknown, None, password, True),
                ]:
                    server.case = Case(password, client_key)
                    profile = dict(name="Apple SSH laboratory", host="127.0.0.1", port=server.port,
                                   username="lab@realm.example", favorite=False, platform="MacOs",
                                   options=dict(protocol="Ssh", agent=False, identity_file=key,
                                                known_hosts_file=str(trust_path), config_file=None))
                    result = subprocess.run([str(probe), "--self-test-ssh"],
                                            input=json.dumps(dict(profile=profile, secret=secret, expect_reject=reject)),
                                            text=True, encoding="utf-8", capture_output=True, timeout=35)
                    if result.returncode or ("WINDOWS_SSH_REJECT_OK" if reject else "WINDOWS_SSH_OK") not in result.stdout:
                        # No secret enters this fixed BatchMode preflight. It diagnoses before authentication.
                        client = probe.parent / "OpenSSH" / "ssh.exe"
                        keep = ("PATH", "PATHEXT", "TEMP", "TMP", "USERPROFILE", "LOCALAPPDATA", "APPDATA", "COMSPEC", "SYSTEMROOT", "WINDIR", "PROGRAMDATA", "ALLUSERSPROFILE")
                        environment = {key: value for key, value in os.environ.items() if key.upper() in keep}
                        command = [str(client), "-vv", "-F", "none", "-oBatchMode=yes", "-oIdentityFile=none", "-oIdentityAgent=none",
                                   "-oStrictHostKeyChecking=yes", "-oConnectTimeout=5", '-oUserKnownHostsFile="' + str(trust_path).replace(chr(92), "/") + '"',
                                   "-p", str(server.port), "-l", "lab@realm.example", "--", "127.0.0.1"]
                        try:
                            diagnostic = subprocess.run(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                                        env=environment, timeout=10, encoding="utf-8", errors="replace").stderr[-2500:]
                        except subprocess.TimeoutExpired:
                            diagnostic = "BatchMode preflight timed out"
                        raise RuntimeError(f"Native SSH case {name} failed (exit {result.returncode}): " + result.stderr[-2000:] + "\nBatchMode diagnostic: " + diagnostic)
                    state = server.case.state.copy()
                    if name in ("changed_host", "unknown_host"):
                        assert state["password_ok"] + state["password_rejected"] + state["key_ok"] + state["key_rejected"] == 0
                    elif name == "wrong_password":
                        assert state["password_rejected"] >= 1 and state["password_ok"] == 0
                    else:
                        assert state["probe"] and state["pty"] >= 1 and state["resize"] >= 1
                        assert state["key_ok" if key else "password_ok"] >= 1
                    reports.append(dict(case=name, result="PASS", **state))
                print(json.dumps(dict(result="PASS", target="127.0.0.1", real_mac=False,
                                      system_account_or_shell=False, paramiko=paramiko.__version__, cases=reports)))
    finally:
        server.close()


if __name__ == "__main__":
    main()
