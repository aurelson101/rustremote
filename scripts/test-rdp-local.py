#!/usr/bin/env python3
"""RDP/NLA réel sur loopback, avec deux affichages Xvfb privés et secrets éphémères.
Ne partage jamais DISPLAY/XAUTHORITY du bureau personnel. Aucun service persistant.
"""
import json
import os
from pathlib import Path
import re
import secrets
import select
import signal
import socket
import subprocess
import tempfile
import time
import tomllib
import sys

os.umask(0o077)
processes = []
logs = []
root_dir = Path(__file__).resolve().parent.parent


def run(args, **kw):
    return subprocess.run(args, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kw)


def start(args, env, log_path, **kw):
    stream = open(log_path, "wb")
    logs.append(stream)
    child = subprocess.Popen(args, env=env, stdout=stream, stderr=stream, start_new_session=True, **kw)
    processes.append(child)
    return child


def wait_for(predicate, seconds, message):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if predicate():
            return
        time.sleep(0.2)
    raise RuntimeError(message)


summary = {"target": "127.0.0.1", "personal_desktop_accessed": False}
with tempfile.TemporaryDirectory(prefix="rustremote-rdp-") as temporary:
    base = Path(temporary)
    common = dict(os.environ, HOME=str(base / "home"), XDG_CONFIG_HOME=str(base / "config"), PULSE_SERVER="unix:" + str(base / "no-audio"), PIPEWIRE_REMOTE="rustremote-no-audio", CUPS_SERVER=str(base / "no-printers"))
    (base / "home").mkdir()
    (base / "config").mkdir()
    (base / "tmp").mkdir()
    common["TMPDIR"] = str(base / "tmp")
    krb=base/"krb5.conf"
    krb.write_text("[libdefaults]\n default_realm = RUSTREMOTE\n dns_lookup_kdc = false\n dns_lookup_realm = false\n[realms]\n RUSTREMOTE = {\n  kdc = 127.0.0.1:1\n }\n")
    common["KRB5_CONFIG"]=str(krb)
    common["KRB5CCNAME"]="FILE:"+str(base/"krb-cache")
    try:
        def display(name):
            auth = base / (name + ".Xauthority")
            auth.touch(mode=0o600)
            cookie = secrets.token_hex(16)
            run(["xauth", "-f", str(auth)], input=f"add :65534 . {cookie}\n".encode())
            reader, writer = os.pipe()
            process = start(["Xvfb", "-displayfd", str(writer), "-screen", "0", "1600x1000x24", "-noreset", "-nolisten", "tcp", "-auth", str(auth)], common, base / (name + "-x.log"), pass_fds=(writer,))
            os.close(writer)
            assert select.select([reader], [], [], 10)[0], "Xvfb non disponible"
            number = int(os.read(reader, 32).strip())
            os.close(reader)
            assert number != int(os.environ.get("DISPLAY", ":0").split(":")[-1].split(".")[0]), "Affichage personnel refusé"
            run(["xauth", "-f", str(auth)], input=f"add :{number} . {cookie}\n".encode())
            return dict(common, DISPLAY=f":{number}", XAUTHORITY=str(auth))

        server_env = display("server")
        client_env = display("client")
        run(["xsetroot", "-solid", "#24436c"], env=server_env)
        start(["xmessage", "-geometry", "600x170+40+40", "-bg", "#24436c", "-fg", "white", "-buttons", "Fermer:0", "RustRemote - RDP local isole\nBureau virtuel de test uniquement\nNLA et certificat epingle"], server_env, base / "desktop.log")
        version=tomllib.loads((root_dir/"Cargo.toml").read_text())["package"]["version"]
        image=root_dir / f"dist/RustRemote-{version}-preview-x86_64.AppImage"
        if image.is_file():
            app_env=dict(server_env,GDK_BACKEND="x11",GSK_RENDERER="cairo",GIO_USE_VFS="local",GTK_USE_PORTAL="0")
            start(["dbus-run-session","--",str(image),"--demo"],app_env,base / "native-app.log")
            wait_for(lambda:b"RustRemote" in run(["xwininfo","-root","-tree"],env=server_env).stdout,45,"Interface RustRemote non affichée dans le bureau virtuel")
            summary["native_app_in_virtual_desktop"] = True
        secret = secrets.token_urlsafe(32)
        nt_hash = run(["openssl", "dgst", "-md4", "-provider", "legacy", "-binary"], input=secret.encode("utf-16-le")).stdout.hex()
        sam = base / "SAM"
        sam.write_text(f"rdplab:RUSTREMOTE::{nt_hash}:::\n")
        sam.chmod(0o600)
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            port = probe.getsockname()[1]
        assert port > 1024
        server = start(["freerdp-shadow-cli3", "/bind-address:127.0.0.1", f"/port:{port}", "/sec:nla", "+auth", "+may-view", "+may-interact", "/max-connections:2", f"/sam-file:{sam}", "/log-level:WARN"], server_env, base / "shadow.log")

        def listening():
            if server.poll() is not None:
                return False
            output = run(["ss", "-H", "-lntp", f"sport = :{port}"]).stdout.decode()
            return any(line.split()[3] == f"127.0.0.1:{port}" and f"pid={server.pid}," in line for line in output.splitlines())
        wait_for(listening, 15, "Le serveur n'écoute pas exclusivement sur loopback")
        summary["bind_verified"] = True
        cert = next((base / "config").rglob("shadow.crt"))
        digest = run(["openssl", "x509", "-in", str(cert), "-noout", "-fingerprint", "-sha256"]).stdout.decode()
        pin = digest.split("=", 1)[1].strip().replace(":", "").lower()
        assert re.fullmatch("[0-9a-f]{64}", pin)

        def client(pin_value, password, filename):
            p = start(["xfreerdp3", f"/v:127.0.0.1:{port}", "/u:rdplab", "/d:RUSTREMOTE", "/from-stdin:force", "/sec:nla", "/auth-pkg-list:none,ntlm", f"/cert:fingerprint:sha256:{pin_value}", "/audio-mode:2", "-drives", "-clipboard", "/size:1600x1000", "/t:RustRemote-RDP-Lab", "/log-level:WARN"], client_env, base / filename, stdin=subprocess.PIPE)
            p.stdin.write((password + "\n").encode())
            p.stdin.close()
            return p

        bad_pin = client("0" * 64, secret, "bad-pin.log")
        bad_pin.wait(timeout=15)
        assert bad_pin.returncode != 0, "Un mauvais certificat a été accepté"
        pin_log = (base / "bad-pin.log").read_text(errors="replace")
        assert "Host key verification failed." in pin_log and "certificate not trusted, aborting." in pin_log, "Refus RDP sans diagnostic de clé/certificat non approuvé"
        summary["wrong_certificate_rejected"] = True
        bad_password = client(pin, secrets.token_urlsafe(32), "bad-password.log")
        bad_password.wait(timeout=15)
        assert bad_password.returncode != 0, "Un mauvais mot de passe a été accepté"
        password_log = (base / "bad-password.log").read_text(errors="replace")
        assert "ERRCONNECT_LOGON_FAILURE" in password_log or "ERRCONNECT_AUTHENTICATION_FAILED" in password_log, "Refus RDP sans diagnostic d’authentification"
        summary["negative_failure_causes_verified"] = True
        summary["wrong_password_rejected"] = True
        good = client(pin, secret, "client.log")

        def connected():
            if good.poll() is not None:
                return False
            tree = run(["xwininfo", "-root", "-tree"], env=client_env).stdout
            return b"RustRemote-RDP-Lab" in tree
        wait_for(connected, 20, "Session NLA / bureau RDP non établi")
        time.sleep(2)
        summary["nla_authenticated"] = True
        summary["certificate_pinned"] = True
        capture = root_dir / "dist/rdp-local.png"
        rendered=run(["python3", "-c", "from PIL import ImageGrab; import os,sys; im=ImageGrab.grab(xdisplay=os.environ['DISPLAY']); im.save(sys.argv[1]); n=sum(abs(r-36)<12 and abs(g-67)<12 and abs(b-108)<12 for r,g,b in im.convert('RGB').getdata()); print(n)", str(capture)], env=client_env)
        assert int(rendered.stdout.strip()) > 1000, "Aucune image distante décodée"
        summary["remote_pixels_verified"] = True
        summary["desktop_capture"] = str(capture.relative_to(root_dir))
        if "--app" in sys.argv:
            good.terminate();good.wait(timeout=10)
            fixture=base/"profile-test"; (fixture/"rustremote").mkdir(parents=True)
            profile={"name":"RDP profil — laboratoire","host":"127.0.0.1","port":port,"username":"rdplab","favorite":False,"options":{"protocol":"Rdp","domain":"RUSTREMOTE","width":1200,"height":800,"fullscreen":False,"clipboard":False,"clipboard_files":False,"settings":{"size_mode":0,"certificate_sha256":pin}}}
            (fixture/"rustremote/profiles.json").write_text(json.dumps({"version":1,"folders":[],"profiles":[profile]}))
            (fixture/"password.secret").write_text(secret)
            app_env=dict(client_env,RUSTREMOTE_RDP_FIXTURE=str(fixture),XDG_DATA_HOME=str(base/"data"))
            app_binary=Path(sys.argv[sys.argv.index("--app")+1]).resolve()
            outcome=subprocess.run(["python3",str(root_dir/"scripts/smoke-profiles.py"),str(app_binary),"--rdp-credentials-only"],env=app_env,timeout=100,capture_output=True)
            if outcome.returncode:
                print(outcome.stderr.decode(errors="replace")[-4000:],flush=True)
                raise AssertionError("Parcours GUI RDP du profil échoué")
            assert not re.search(rb"CRITICAL|panicked at",outcome.stderr),"Erreur critique de l’application pendant le parcours RDP"
            summary["profile_domain_password_stdin_connection_disconnect"]=True
            summary["tracked_client_closed_on_application_exit"]=True
        summary["clipboard_real"] = "non vérifié : le backend Shadow 3.32 ne gère pas CLIPRDR"
        summary["audio_printers_dual_screen"] = "options testables par configuration ; périphériques réels non validés"
        summary["log_error_counts"] = {
            name: len(re.findall(r"\[ERROR\]|CRITICAL|panicked at", (base/name).read_text(errors="replace")))
            for name in ("shadow.log", "client.log", "bad-pin.log", "bad-password.log", "native-app.log")
            if (base/name).exists()
        }
        native_errors = summary["log_error_counts"].get("native-app.log", 0)
        assert native_errors == 0, "Erreur critique GTK dans le bureau virtuel"
        summary["log_errors_redacted"] = {}
        for name in ("shadow.log", "client.log", "bad-pin.log", "bad-password.log"):
            lines = []
            for line in (base/name).read_text(errors="replace").splitlines():
                if "[ERROR]" in line:
                    line = re.sub(r"^.*?\[ERROR\]", "[ERROR]", line)
                    line = line.replace(secret, "[redacted]").replace(nt_hash, "[redacted]").replace(str(base), "[temporary]")
                    if line not in lines:
                        lines.append(line)
            summary["log_errors_redacted"][name] = lines
        summary["success"] = True
    except Exception as error:
        summary["success"] = False
        summary["error"] = str(error).replace(locals().get("secret", "\x00"), "[redacted]").replace(locals().get("nt_hash", "\x00"), "[redacted]")
        # Les logs bruts, SAM et clés restent temporaires et privés.
        for name in ["shadow.log", "client.log", "bad-password.log"]:
            p = base / name
            if p.exists():
                content = p.read_text(errors="replace")[-3000:]
                for sensitive in [locals().get("secret"), locals().get("nt_hash")]:
                    if sensitive:
                        content = content.replace(sensitive, "[redacted]")
                print(name, content)
    finally:
        for p in reversed(processes):
            if p.poll() is None:
                os.killpg(p.pid, signal.SIGTERM)
                try:
                    p.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    os.killpg(p.pid, signal.SIGKILL)
                    p.wait(timeout=3)
        for stream in logs:
            stream.close()
        summary["services_stopped"] = True
        Path("dist/rdp-local-result.json").write_text(json.dumps(summary, indent=2, ensure_ascii=False))
        print(json.dumps(summary, ensure_ascii=False))
if not summary.get("success"):
    raise SystemExit(1)
