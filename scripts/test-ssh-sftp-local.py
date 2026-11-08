#!/usr/bin/env python3
"""SSH/SFTP réel sur loopback, sans shell ni compte système, fichiers éphémères.
Prérequis de test : python3-paramiko, openssh-client. Ne teste pas un moteur Rust.
"""
import errno
import json
import logging
import os
from pathlib import Path, PurePosixPath
import socket
import subprocess
import tempfile
import threading
import time
import sys
import secrets

import paramiko

os.umask(0o077)
# Ne pas conserver de trames ni de clés : uniquement des compteurs d'événements.
logging.getLogger("paramiko").addHandler(logging.NullHandler())
events = {"publickey_accepted": 0, "publickey_rejected": 0, "pty_requested": 0,
          "resize_requested": 0, "unsafe_path_rejected": 0, "shell_commands": 0}
shell_workers=[]


def command(args, **kwargs):
    return subprocess.run(args, capture_output=True, timeout=15, **kwargs)


with tempfile.TemporaryDirectory(prefix="rustremote-ssh-") as directory:
    base = Path(directory)
    data = base / "server-data"
    data.mkdir()
    for name in ("identity", "wrong-identity"):
        assert command(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(base/name)]).returncode == 0
    identity = paramiko.Ed25519Key.from_private_key_file(str(base/"identity"))
    host = paramiko.ECDSAKey.generate()
    authorized = identity.asbytes()
    geometry = base/"geometry.json"
    password = secrets.token_urlsafe(24)
    (base/"password.secret").write_text(password)


    class Server(paramiko.ServerInterface):
        def get_allowed_auths(self, username):
            return "publickey,password"

        def check_auth_password(self, username, candidate):
            valid = username == "lab" and secrets.compare_digest(candidate, password)
            if valid:
                events["password_accepted"] = events.get("password_accepted", 0) + 1
                (base/"password-accepted.count").write_text(str(events["password_accepted"]))
            return paramiko.AUTH_SUCCESSFUL if valid else paramiko.AUTH_FAILED


        def check_auth_publickey(self, username, key):
            valid = username == "lab" and key.asbytes() == authorized
            events["publickey_accepted" if valid else "publickey_rejected"] += 1
            return paramiko.AUTH_SUCCESSFUL if valid else paramiko.AUTH_FAILED

        def check_channel_request(self, kind, channel_id):
            return paramiko.OPEN_SUCCEEDED if kind == "session" else paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED

        def check_channel_exec_request(self, channel, value):
            if value != b"rustremote-probe":
                return False
            def reply():
                time.sleep(0.05)  # Laisser partir l'acquittement de la requête exec.
                channel.sendall("SSH local — été\n".encode())
                channel.send_exit_status(0)
                channel.close()
            threading.Thread(target=reply).start()
            return True

        def check_channel_pty_request(self, *args):
            events["pty_requested"] += 1
            return True

        def check_channel_shell_request(self, channel):
            def shell():
                try:
                    channel.settimeout(0.2)
                    channel.sendall("\r\n\x1b[32mSSH intégré — laboratoire été\x1b[0m\r\nlab> ".encode())
                    pending=b""
                    while not stop.is_set() and not channel.closed:
                        try:
                            content=channel.recv(4096)
                        except socket.timeout:
                            continue
                        if not content: break
                        channel.sendall(content.replace(b"\r",b"\r\n"))
                        pending+=content.replace(b"\r",b"\n")
                        if len(pending)>65536: break
                        while b"\n" in pending:
                            line,pending=pending.split(b"\n",1)
                            command=line.decode(errors="replace").strip()
                            if not command: continue
                            events["shell_commands"]+=1
                            if command=="echo RUSTREMOTE_UTF8": reply="RÉPONSE_DISTANTE — été"
                            elif command=="pwd": reply="/laboratoire"
                            elif command=="exit": channel.send_exit_status(0);channel.close();return
                            else: reply="Commande simulée refusée"
                            channel.sendall(("\r\n"+reply+"\r\nlab> ").encode())
                except (OSError,EOFError):
                    pass
            worker=threading.Thread(target=shell)
            shell_workers.append(worker);worker.start()
            return True  # Réponses prédéfinies, jamais de shell système.

        def check_channel_window_change_request(self, channel, width, height, pixelwidth, pixelheight):
            events["resize_requested"] += 1
            geometry.with_suffix(".tmp").write_text(json.dumps({"width":width,"height":height,"changes":events["resize_requested"]}))
            geometry.with_suffix(".tmp").replace(geometry)
            return True

    class Handle(paramiko.SFTPHandle):
        def write(self, offset, content):
            if offset < 0 or offset + len(content) > 2 * 1024 * 1024:
                return paramiko.SFTP_PERMISSION_DENIED
            return super().write(offset, content)

    class Files(paramiko.SFTPServerInterface):
        def path(self, name):
            parts = PurePosixPath(name).parts
            if ".." in parts:
                events["unsafe_path_rejected"] += 1
                raise PermissionError(errno.EACCES, "path rejected")
            result = data.joinpath(*(p for p in parts if p != "/"))
            cursor = data
            for part in result.relative_to(data).parts:
                cursor /= part
                if cursor.is_symlink():
                    events["unsafe_path_rejected"] += 1
                    raise PermissionError(errno.EACCES, "link rejected")
            return result

        def list_folder(self, name):
            try:
                result = []
                for item in self.path(name).iterdir():
                    attr = paramiko.SFTPAttributes.from_stat(item.lstat())
                    attr.filename = item.name
                    result.append(attr)
                return result
            except OSError as error:
                return paramiko.SFTPServer.convert_errno(error.errno)

        def stat(self, name):
            try:
                return paramiko.SFTPAttributes.from_stat(self.path(name).stat())
            except OSError as error:
                return paramiko.SFTPServer.convert_errno(error.errno)

        lstat = stat

        def open(self, name, flags, attr):
            try:
                if len(list(data.rglob("*"))) > 128:
                    return paramiko.SFTP_PERMISSION_DENIED
                fd = os.open(self.path(name), flags | os.O_NOFOLLOW, 0o600)
                handle = Handle(flags)
                stream = os.fdopen(fd, "r+b" if flags & os.O_RDWR else "wb" if flags & os.O_WRONLY else "rb")
                handle.readfile = stream
                handle.writefile = stream
                return handle
            except OSError as error:
                return paramiko.SFTPServer.convert_errno(error.errno)

        def mkdir(self, name, attr):
            try:
                self.path(name).mkdir(mode=0o700)
                return paramiko.SFTP_OK
            except OSError as error:
                return paramiko.SFTPServer.convert_errno(error.errno)

        def rename(self, old, new):
            try:
                self.path(old).rename(self.path(new))
                return paramiko.SFTP_OK
            except OSError as error:
                return paramiko.SFTPServer.convert_errno(error.errno)

        def remove(self, name):
            try:
                self.path(name).unlink()
                return paramiko.SFTP_OK
            except OSError as error:
                return paramiko.SFTPServer.convert_errno(error.errno)

        def rmdir(self, name):
            try:
                self.path(name).rmdir()
                return paramiko.SFTP_OK
            except OSError as error:
                return paramiko.SFTPServer.convert_errno(error.errno)

        def chattr(self, name, attr):
            try:
                if attr.st_mode is None or attr.st_mode & ~0o777:
                    return paramiko.SFTP_PERMISSION_DENIED
                self.path(name).chmod(attr.st_mode)
                return paramiko.SFTP_OK
            except OSError as error:
                return paramiko.SFTPServer.convert_errno(error.errno)

        posix_rename = rename

    listener = socket.socket()
    listener.bind(("127.0.0.1", 0))
    listener.listen(4)
    listener.settimeout(0.2)
    port = listener.getsockname()[1]
    known = base / "known_hosts"
    known.write_text(f"[127.0.0.1]:{port} {host.get_name()} {host.get_base64()}\n")
    stop = threading.Event()
    transports = []

    def serve():
        while not stop.is_set():
            try:
                connection, _ = listener.accept()
            except socket.timeout:
                continue
            transport = paramiko.Transport(connection)
            transports.append(transport)
            transport.add_server_key(host)
            transport.set_subsystem_handler("sftp", paramiko.SFTPServer, Files)
            try:
                transport.start_server(server=Server())
            except (paramiko.SSHException, EOFError, OSError):
                transport.close()

    worker = threading.Thread(target=serve)
    worker.start()
    summary = {"target": "127.0.0.1", "system_account_or_shell_used": False}
    client = None
    agent_process = None
    try:
        options = ["-F", "/dev/null", "-oBatchMode=yes", "-oStrictHostKeyChecking=yes",
                   "-oUserKnownHostsFile="+str(known), "-oGlobalKnownHostsFile=/dev/null",
                   "-oIdentityAgent=none", "-oIdentitiesOnly=yes", "-oConnectTimeout=5"]
        result = command(["ssh", *options, "-i", str(base/"identity"), "-p", str(port), "-l", "lab", "127.0.0.1", "rustremote-probe"])
        assert result.returncode == 0 and result.stdout.decode() == "SSH local — été\n", "SSH probe failed"
        summary["openssh_utf8_exec"] = True
        result = command(["ssh", *options, "-i", str(base/"wrong-identity"), "-p", str(port), "-l", "lab", "127.0.0.1", "rustremote-probe"])
        assert result.returncode != 0, "Invalid key accepted"
        summary["wrong_identity_rejected"] = True
        wrong_host = paramiko.ECDSAKey.generate()
        known.write_text(f"[127.0.0.1]:{port} {wrong_host.get_name()} {wrong_host.get_base64()}\n")
        result = command(["ssh", *options, "-i", str(base/"identity"), "-p", str(port), "-l", "lab", "127.0.0.1", "rustremote-probe"])
        assert result.returncode != 0 and b"HOST IDENTIFICATION HAS CHANGED" in result.stderr
        summary["changed_host_key_rejected"] = True
        known.write_text(f"[127.0.0.1]:{port} {host.get_name()} {host.get_base64()}\n")
        source = base / "source"
        source.mkdir()
        (source/"texte.txt").write_text("Fichier UTF-8 — été\n")
        (source/"dossier").mkdir()
        (source/"dossier/enfant.txt").write_text("Copie récursive\n")
        batch = 'put texte.txt /texte.txt\nmkdir /Linux\nput -r dossier /Linux/\nget /texte.txt retour.txt\nget -r /Linux retour-dossier\n'
        result = command(["sftp", *options, "-i", str(base/"identity"), "-P", str(port), "-b", "-", "lab@127.0.0.1"], input=batch.encode(), cwd=source)
        assert result.returncode == 0, "SFTP batch failed"
        assert (source/"retour.txt").read_bytes() == (source/"texte.txt").read_bytes()
        assert (source/"retour-dossier/dossier/enfant.txt").read_bytes() == (source/"dossier/enfant.txt").read_bytes()
        summary["opensftp_bidirectional_files_folders"] = True
        client = paramiko.SSHClient()
        client.load_host_keys(str(known))
        client.set_missing_host_key_policy(paramiko.RejectPolicy())
        client.connect("127.0.0.1", port, "lab", pkey=identity, allow_agent=False, look_for_keys=False, timeout=5)
        channel = client.invoke_shell(width=80, height=24)
        channel.resize_pty(width=132, height=40)
        time.sleep(0.1)
        assert events["pty_requested"] and events["resize_requested"]
        summary["ssh_pty_and_resize"] = True
        sftp = client.open_sftp()
        with sftp.open("/texte.txt", "w") as stream:
            stream.write("Édition distante — test\n".encode())
        assert sftp.open("/texte.txt").read().decode() == "Édition distante — test\n"
        summary["sftp_edit_readback"] = True
        (data/"escape").symlink_to(base/"identity")
        for unsafe in ("/../identity", "/escape"):
            try:
                sftp.open(unsafe).close()
            except OSError:
                pass
            else:
                raise AssertionError("SFTP unsafe path accepted")
        summary["traversal_and_symlink_rejected"] = True
        sftp.close()
        channel.close()
        if "--gui" in sys.argv:
            image=Path(sys.argv[sys.argv.index("--gui")+1]).resolve()
            config=base/"gui-config"/"rustremote"
            config.mkdir(parents=True,mode=0o700)
            gui_known=base/"gui known_hosts";gui_known.write_text(known.read_text())
            gui_env=dict(os.environ,SSH_AUTH_SOCK=str(base/"agent.sock"))
            agent_process=subprocess.Popen(["ssh-agent","-D","-a",gui_env["SSH_AUTH_SOCK"]],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            deadline=time.monotonic()+5
            while not Path(gui_env["SSH_AUTH_SOCK"]).exists() and time.monotonic()<deadline:time.sleep(.1)
            assert Path(gui_env["SSH_AUTH_SOCK"]).exists()
            subprocess.run(["ssh-add",str(base/"identity")],env=gui_env,check=True,capture_output=True,timeout=5)
            primary={"name":"SSH intégré — laboratoire","host":"127.0.0.1","port":port,"username":"lab","favorite":False,"folder":"Linux","platform":"Linux","simulation":False,"options":{"protocol":"Ssh","identity_file":str(base/"identity"),"agent":False,"known_hosts_file":str(gui_known),"config_file":"/dev/null"}}
            agent_profile=dict(primary,name="SSH Agent — laboratoire",options=dict(primary["options"],identity_file=None,agent=True))
            (config/"profiles.json").write_text(json.dumps({"version":1,"folders":["Linux"],"profiles":[primary,agent_profile]}))
            gui=subprocess.run(["dbus-run-session","--","python3",str(Path(__file__).with_name("smoke-connected.py")),str(image),str(config.parent)],env=gui_env,timeout=240)
            assert gui.returncode==0,"Parcours SSH intégré échoué"
            summary["application_terminal_gui"]=True
        summary["success"] = True
    finally:
        if client:
            client.close()
        if agent_process:
            agent_process.terminate();agent_process.wait(timeout=5)
        stop.set()
        worker.join(timeout=6)
        listener.close()
        for transport in transports:
            transport.close()
            transport.join(timeout=3)
        for worker in shell_workers:
            worker.join(timeout=3)
        assert not worker.is_alive() and all(not t.is_alive() for t in transports)
        summary["services_stopped"] = True
        summary["events"] = events
        (Path(__file__).resolve().parent.parent/"dist/ssh-sftp-local-result.json").write_text(json.dumps(summary, indent=2))
        print(json.dumps(summary, ensure_ascii=False))
