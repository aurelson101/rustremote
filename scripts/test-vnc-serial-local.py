#!/usr/bin/env python3
"""VNC par tunnel SSH loopback et switch série PTY : aucun équipement réel."""
import json
import os
from pathlib import Path
import pty
import secrets
import select
import signal
import socket
import struct
import subprocess
import tempfile
import threading
import time

import paramiko
import logging
logging.getLogger("paramiko").addHandler(logging.NullHandler())
os.umask(0o077)
processes, streams, transports, channels, sockets, threads = [], [], [], [], [], []
stop = threading.Event()
summary = {"target": "127.0.0.1", "physical_serial_accessed": False, "personal_desktop_accessed": False}


def run(args, **kwargs):
    return subprocess.run(args, check=True, capture_output=True, timeout=15, **kwargs)


def start(args, log, **kwargs):
    stream = open(log, "wb")
    streams.append(stream)
    process = subprocess.Popen(args, stdout=stream, stderr=stream, start_new_session=True, **kwargs)
    processes.append(process)
    return process


def wait(predicate, message, seconds=15):
    deadline = time.monotonic()+seconds
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.1)
    raise AssertionError(message)


with tempfile.TemporaryDirectory(prefix="rustremote-vnc-serial-") as temporary:
    base = Path(temporary)
    env = dict(os.environ, HOME=str(base), XDG_CONFIG_HOME=str(base/"config"))
    try:
        def display(binary, label, extra):
            auth = base/(label+".Xauthority")
            auth.touch(mode=0o600)
            cookie = secrets.token_hex(16)
            run(["xauth", "-f", str(auth)], input=f"add :65534 . {cookie}\n".encode())
            reader, writer = os.pipe()
            start([binary, "-displayfd", str(writer), "-auth", str(auth), "-nolisten", "tcp", "-noreset", *extra], base/(label+".log"), env=env, pass_fds=(writer,))
            os.close(writer)
            assert select.select([reader], [], [], 10)[0], "Virtual display unavailable"
            number = int(os.read(reader, 32).strip())
            os.close(reader)
            assert f":{number}" != os.environ.get("DISPLAY"), "Personal display forbidden"
            run(["xauth", "-f", str(auth)], input=f"add :{number} . {cookie}\n".encode())
            return dict(env, DISPLAY=f":{number}", XAUTHORITY=str(auth))

        with socket.socket() as probe:
            probe.bind(("127.0.0.1",0))
            vnc_port = probe.getsockname()[1]
        password = secrets.token_urlsafe(6)[:8]
        password_file = base/"vnc-passwd"
        password_file.write_bytes(run(["tigervncpasswd", "-f"], input=(password+"\n").encode()).stdout)
        server_env = display("Xtigervnc", "vnc-server", ["-geometry","1024x768","-rfbport",str(vnc_port),"-localhost","yes","-SecurityTypes","VncAuth","-PasswordFile",str(password_file)])
        client_env = display("Xvfb", "vnc-client", ["-screen","0","1280x900x24"])
        run(["xsetroot", "-solid", "#315d81"], env=server_env)
        start(["xmessage","-geometry","500x120+40+40","VNC local isole - RustRemote\nTunnel SSH et authentification VNC"], base/"desktop.log", env=server_env)
        listen = run(["ss","-H","-lnt",f"sport = :{vnc_port}"]).stdout.decode()
        assert listen and all(line.split()[3].startswith(("127.0.0.1:","[::1]:")) for line in listen.splitlines()), listen
        summary["vnc_loopback_only"] = True
        keyfile = base/"identity"
        run(["ssh-keygen","-q","-t","ed25519","-N","","-f",str(keyfile)])
        key = paramiko.Ed25519Key.from_private_key_file(str(keyfile))
        host = paramiko.ECDSAKey.generate()
        listener = socket.socket()
        listener.bind(("127.0.0.1",0))
        listener.listen(2)
        listener.settimeout(0.2)
        sockets.append(listener)
        ssh_port = listener.getsockname()[1]
        known = base/"known_hosts"
        known.write_text(f"[127.0.0.1]:{ssh_port} {host.get_name()} {host.get_base64()}\n")
        forwarded = threading.Event()
        forward_ids = set()

        class Gateway(paramiko.ServerInterface):
            def get_allowed_auths(self, username):
                return "publickey"

            def check_auth_publickey(self, username, presented):
                return paramiko.AUTH_SUCCESSFUL if username == "lab" and presented.asbytes() == key.asbytes() else paramiko.AUTH_FAILED

            def check_channel_request(self, kind, channel_id):
                return paramiko.OPEN_SUCCEEDED if kind == "session" else paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED

            def check_channel_exec_request(self, channel, command):
                return command == b"sleep 20"  # Maintien du canal, aucun shell exécuté.

            def check_channel_direct_tcpip_request(self, channel_id, origin, destination):
                if destination == ("127.0.0.1", vnc_port):
                    forwarded.set()
                    forward_ids.add(channel_id)
                    return paramiko.OPEN_SUCCEEDED
                return paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED

        def bridge(channel):
            remote = socket.create_connection(("127.0.0.1",vnc_port), timeout=3)
            sockets.append(remote)
            try:
                while not stop.is_set():
                    readable,_,_ = select.select([channel,remote],[],[],0.2)
                    for source in readable:
                        content = source.recv(65536)
                        if not content:
                            return
                        (remote if source is channel else channel).sendall(content)
            finally:
                channel.close()
                remote.close()

        def serve():
            while not stop.is_set():
                try:
                    connection,_ = listener.accept()
                except socket.timeout:
                    continue
                transport = paramiko.Transport(connection)
                transports.append(transport)
                transport.add_server_key(host)
                transport.start_server(server=Gateway())
                while transport.is_active() and not stop.is_set():
                    channel = transport.accept(0.2)
                    if channel:
                        channels.append(channel)
                        if channel.get_id() in forward_ids:
                            worker = threading.Thread(target=bridge,args=(channel,))
                            threads.append(worker)
                            worker.start()

        worker = threading.Thread(target=serve)
        threads.append(worker)
        worker.start()
        # Même modèle que SessionProfile::vnc_tunnel_command ; clé/known_hosts privés du test.
        via = f'/usr/bin/ssh -F /dev/null -i "{keyfile}" -oIdentitiesOnly=yes -oIdentityAgent=none -oUserKnownHostsFile="{known}" -oGlobalKnownHostsFile=/dev/null -oBatchMode=yes -T -oStrictHostKeyChecking=yes -oForwardAgent=no -oPermitLocalCommand=no -oExitOnForwardFailure=yes -oConnectTimeout=5 -p {ssh_port} -f -L "127.0.0.1:$L:$H:$R" "$G" sleep 20'
        viewer = start(["xtigervncviewer","-via","lab@127.0.0.1","-SecurityTypes","VncAuth","-PasswordFile",str(password_file),"-AcceptClipboard=0","-SendClipboard=0","-AlertOnFatalError=0","-ReconnectOnError=0",f"127.0.0.1::{vnc_port}"],base/"viewer.log",env=dict(client_env,VNC_VIA_CMD=via))
        wait(lambda: b"Enabling continuous updates" in (base/"viewer.log").read_bytes(), "VNC not authenticated through SSH")
        assert forwarded.is_set() and viewer.poll() is None
        summary["vnc_authenticated_through_ssh"] = True
        time.sleep(1)
        capture = Path("dist/vnc-local.png").resolve()
        result = run(["python3","-c","from PIL import ImageGrab; import os,sys; im=ImageGrab.grab(xdisplay=os.environ['DISPLAY']); im.save(sys.argv[1]); print(sum(abs(r-49)<10 and abs(g-93)<10 and abs(b-129)<10 for r,g,b in im.convert('RGB').getdata()))",str(capture)],env=client_env)
        assert int(result.stdout.strip()) > 1000, "Remote VNC rendering absent"
        summary["vnc_remote_pixels_verified"] = True
        with socket.create_connection(("127.0.0.1",vnc_port),timeout=3) as rejected:
            def receive(length):
                content = b""
                while len(content) < length:
                    chunk = rejected.recv(length-len(content))
                    assert chunk, "VNC authentication response truncated"
                    content += chunk
                return content
            assert receive(12) == b"RFB 003.008\n"
            rejected.sendall(b"RFB 003.008\n")
            count = receive(1)[0]
            assert 2 in receive(count)
            rejected.sendall(b"\x02")
            receive(16)  # Défi éphémère ; réponse invalide volontaire, aucun mot de passe.
            rejected.sendall(bytes(16))
            assert struct.unpack("!I",receive(4))[0] != 0
        summary["vnc_invalid_authentication_rejected"] = True
        # Deux PTY : liaison série du faux switch et terminal du client picocom.
        master, slave = pty.openpty()
        terminal_master, terminal_slave = pty.openpty()
        device = os.ttyname(slave)
        stream = open(base/"serial.log","wb")
        streams.append(stream)
        serial = subprocess.Popen(["picocom","--baud","115200","--databits","8","--parity","n","--stopbits","1","--flow","n","--",device],stdin=terminal_slave,stdout=terminal_slave,stderr=stream,start_new_session=True,env=env)
        processes.append(serial)
        def read_until(fd, expected):
            result = b""
            deadline = time.monotonic()+8
            while expected not in result and time.monotonic()<deadline:
                if select.select([fd],[],[],0.2)[0]:
                    result += os.read(fd,8192)
            assert expected in result, "Serial data missing"
            return result
        read_until(terminal_master,b"Terminal ready")
        os.write(terminal_master,b"show version\r")
        read_until(master,b"show version")
        os.write(master,b"\r\nRustRemote Switch Simulator 1.0\r\nswitch-lab> ")
        read_until(terminal_master,b"RustRemote Switch Simulator 1.0")
        os.write(terminal_master,b"show interfaces\r")
        read_until(master,b"show interfaces")
        os.write(master,b"\r\nGi0/1 up 1000Mb/s\r\nGi0/2 down\r\nswitch-lab> ")
        read_until(terminal_master,b"Gi0/1 up 1000Mb/s")
        os.write(terminal_master,b"\x01\x18")
        serial.wait(timeout=5)
        assert serial.returncode == 0
        for fd in (master,slave,terminal_master,terminal_slave):
            os.close(fd)
        summary["serial_115200_8n1_bidirectional"] = True
        summary["serial_clean_exit"] = True
        summary["log_review"] = {
            "viewer_panics": (base/"viewer.log").read_text(errors="replace").count("panicked"),
            "serial_stderr_bytes": (base/"serial.log").stat().st_size,
            "lab_warnings": "X display probing, XOpenIM/Fontconfig and non-post-quantum Paramiko key exchange may be logged; no system SSH service used"
        }
        summary["success"] = True
    except Exception as error:
        summary["success"] = False
        summary["error"] = str(error)
        for name in ("viewer.log", "vnc-server.log", "serial.log"):
            path = base/name
            if path.exists():
                content = path.read_text(errors="replace")[-2000:]
                content = content.replace(locals().get("password", "\x00"), "[redacted]").replace(str(base), "[temporary]")
                print(name, content)
        raise
    finally:
        stop.set()
        for channel in channels:
            channel.close()
        for transport in transports:
            transport.close()
        for worker in threads:
            worker.join(timeout=4)
        for sock in sockets:
            sock.close()
        for process in reversed(processes):
            if process.poll() is None:
                os.killpg(process.pid,signal.SIGTERM)
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid,signal.SIGKILL)
                    process.wait(timeout=3)
        for stream in streams:
            stream.close()
        summary["services_stopped"] = all(not worker.is_alive() for worker in threads)
        Path("dist/vnc-serial-local-result.json").write_text(json.dumps(summary,indent=2))
        print(json.dumps(summary,ensure_ascii=False))
