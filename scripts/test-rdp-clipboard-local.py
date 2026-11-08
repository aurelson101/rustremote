#!/usr/bin/env python3
"""CLIPRDR réel GNOME/FreeRDP dans des namespaces privés, sans bureau personnel.
Nécessite GNOME Shell/Remote Desktop, PipeWire, GTK4/PyGObject, FreeRDP, Xvfb,
xauth et FUSE. Ni sudo, ni service système, ni connexion hors loopback.
"""
import json
import os
import pty
import termios
from pathlib import Path
import secrets
import select
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import threading
import time
from urllib.parse import unquote, urlparse

os.umask(0o077)
SCRIPT = Path(__file__).resolve()
ROOT = SCRIPT.parent.parent


def clipboard_probe():
    assert os.environ.get("RUSTREMOTE_RDP_NS") == "1"
    import gi
    gi.require_version("Gtk", "4.0")
    from gi.repository import Gtk, Gdk, GLib
    Gtk.init()
    window = Gtk.Window(title="RustRemote — laboratoire CLIPRDR isolé")
    window.set_child(Gtk.Label(label="CLIPRDR — texte et fichiers de test uniquement"))
    window.set_default_size(1000, 700)
    if os.environ.get('GDK_BACKEND') == 'wayland':
        window.present()
    loop = GLib.MainLoop()
    clipboard = window.get_display().get_clipboard()

    def emit(value):
        print(json.dumps(value, ensure_ascii=False), flush=True)

    def handle(command):
        kind = command["op"]
        if kind == "focus":
            monitors = window.get_display().get_monitors()
            if monitors.get_n_items():
                window.fullscreen_on_monitor(monitors.get_item(monitors.get_n_items()-1))
            window.present()
            emit({"ok": True})
        elif kind == "state":
            emit({"active":window.is_active(),"mapped":window.get_mapped(),"formats":clipboard.get_formats().to_string()})
        elif kind == "set":
            if set(command["data"]).issubset({"text/plain", "text/plain;charset=utf-8"}):
                assert clipboard.set_content(Gdk.ContentProvider.new_for_value(next(iter(command["data"].values()))))
            else:
                providers = []
                for mime, value in command["data"].items():
                    providers.append(Gdk.ContentProvider.new_for_bytes(mime, GLib.Bytes.new(value.encode())))
                assert clipboard.set_content(Gdk.ContentProvider.new_union(providers))
            window.get_display().flush()
            emit({"ok": True})
        elif kind == "get":
            def opened(source, result):
                try:
                    stream, mime = source.read_finish(result)
                    chunks = []
                    total = 0
                    def received(source, result):
                        nonlocal total
                        try:
                            data = bytes(source.read_bytes_finish(result).get_data())
                            total += len(data)
                            if total > 2*1024*1024:
                                raise ValueError("Presse-papiers de test trop volumineux")
                            if not data:
                                stream.close(None)
                                emit({"mime": mime, "text": b"".join(chunks).decode()})
                            else:
                                chunks.append(data)
                                stream.read_bytes_async(16384, 0, None, received)
                        except Exception as error:
                            emit({"error": type(error).__name__})
                    stream.read_bytes_async(16384, 0, None, received)
                except Exception as error:
                    emit({"error": type(error).__name__, "formats": clipboard.get_formats().to_string()})
            clipboard.read_async(command["mimes"], 0, None, opened)
        elif kind == "quit":
            emit({"ok": True})
            loop.quit()
        return False

    def reader():
        for line in sys.stdin:
            if len(line) > 65536:
                break
            GLib.idle_add(handle, json.loads(line))
    threading.Thread(target=reader, daemon=True).start()
    emit({"ready": True})
    loop.run()


def wait_for(predicate, description, timeout=20):
    deadline = time.monotonic()+timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(.1)
    raise AssertionError(description)


def inside(base):
    assert os.environ.get("RUSTREMOTE_RDP_NS") == "1"
    assert sorted(name for _, name in socket.if_nameindex()) == ['lo']
    subprocess.run(['ip', 'link', 'set', 'lo', 'up'], check=True)
    children = []
    streams = []
    report = {"network_namespace": True, "personal_desktop_accessed": False,
              "target": "127.0.0.1", "success": False}

    def spawn(args, env, name, **kwargs):
        log = open(base/(name+'.log'), 'wb')
        streams.append(log)
        process = subprocess.Popen(args, env=env, stderr=log,
                                   stdout=kwargs.pop('stdout', log), start_new_session=True, **kwargs)
        children.append(process)
        return process

    def run(args, env, **kwargs):
        result = subprocess.run(args, env=env, capture_output=True, timeout=15, **kwargs)
        if result.returncode:
            raise RuntimeError('Commande de laboratoire refusée : '+' '.join(args[:4])+' : '+result.stderr.decode(errors='replace')[-1200:])
        return result.stdout

    def environment(name):
        home = base/name
        home.mkdir(mode=0o700)
        env = {'PATH': os.environ['PATH'], 'LANG': 'C.UTF-8', 'LC_ALL': 'C.UTF-8',
               'HOME': str(home), 'RUSTREMOTE_RDP_NS': '1', 'GTK_A11Y': 'none',
               'GIO_USE_VFS': 'local', 'GSETTINGS_BACKEND': 'dconf',
               'LIBGL_ALWAYS_SOFTWARE': '1', 'GSK_RENDERER': 'cairo'}
        for key, folder in [('XDG_RUNTIME_DIR','runtime'), ('XDG_CONFIG_HOME','config'),
                            ('XDG_DATA_HOME','data'), ('XDG_CACHE_HOME','cache')]:
            path = home/folder
            path.mkdir(mode=0o700)
            env[key] = str(path)
        return env

    def bus(env, name):
        path = base/(name+'.bus')
        spawn(['dbus-daemon','--session','--nofork','--address=unix:path='+str(path)], env, name)
        wait_for(path.exists, 'Bus privé indisponible')
        return 'unix:path='+str(path)

    def request(process, command):
        process.stdin.write(json.dumps(command)+'\n')
        process.stdin.flush()
        if not select.select([process.stdout], [], [], 20)[0]:
            raise TimeoutError('Délai du presse-papiers RDP dépassé')
        line = process.stdout.readline()
        if not line:
            raise RuntimeError('Sonde du presse-papiers interrompue')
        return json.loads(line)

    def probe(env, name):
        p = spawn([sys.executable, str(SCRIPT), '--probe'], env, name,
                  stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
        assert select.select([p.stdout], [], [], 20)[0], 'Sonde GTK non démarrée'
        assert json.loads(p.stdout.readline()) == {'ready': True}
        return p

    try:
        server = environment('server')
        client = environment('client')
        system_bus = bus(server, 'dummy-system')
        for env in (server, client):
            env['DBUS_SYSTEM_BUS_ADDRESS'] = system_bus
        server['DBUS_SESSION_BUS_ADDRESS'] = bus(server, 'server-session')
        client['DBUS_SESSION_BUS_ADDRESS'] = bus(client, 'client-session')
        spawn(['pipewire'], server, 'pipewire')
        wait_for(lambda: (Path(server['XDG_RUNTIME_DIR'])/'pipewire-0').exists(), 'PipeWire privé absent')
        shell = spawn(['gnome-shell','--headless','--wayland','--no-x11',
                       '--virtual-monitor','1280x800','--wayland-display','rr-rdp-lab'], server, 'shell')
        server['WAYLAND_DISPLAY'] = 'rr-rdp-lab'
        server['GDK_BACKEND'] = 'wayland'
        wait_for(lambda: (Path(server['XDG_RUNTIME_DIR'])/'rr-rdp-lab').exists(), 'Bureau Wayland privé absent', 30)
        assert shell.poll() is None
        tlsdir = Path(server['XDG_DATA_HOME'])/'gnome-remote-desktop'
        tlsdir.mkdir(mode=0o700)
        certificate, key = tlsdir/'tls.crt', tlsdir/'tls.key'
        run(['openssl','req','-x509','-newkey','rsa:2048','-nodes','-days','1','-subj','/CN=localhost',
             '-addext','subjectAltName=IP:127.0.0.1','-keyout',str(key),'-out',str(certificate)], server)
        user, password = 'rdplab', secrets.token_urlsafe(24)
        for args in [('set-tls-key',str(key)), ('set-tls-cert',str(certificate)),
                     ('set-auth-methods','credentials'), ('disable-view-only',),
                     ('disable-port-negotiation',)]:
            run(['grdctl','--headless','rdp',*args], server)
        master, slave = pty.openpty()
        attributes = termios.tcgetattr(slave)
        attributes[3] &= ~termios.ECHO
        termios.tcsetattr(slave, termios.TCSANOW, attributes)
        credential_process = spawn(['grdctl','--headless','rdp','set-credentials'], server,
                                   'credentials', stdin=slave, stdout=slave)
        os.close(slave)
        def answer(token, value):
            received = b''
            deadline = time.monotonic()+10
            while token not in received and time.monotonic()<deadline:
                if select.select([master],[],[],.2)[0]:received += os.read(master,4096)
            assert token in received, 'Invite de credentials du laboratoire absente'
            os.write(master,(value+'\n').encode())
        try:
            answer(b'Username:',user)
            answer(b'Password:',password)
            assert credential_process.wait(timeout=10)==0,'Credentials privés non enregistrés'
        finally:os.close(master)
        for key_name, value in [('tls-cert',str(certificate)),('tls-key',str(key))]:
            run(['gsettings','set','org.gnome.desktop.remote-desktop.rdp',key_name,value],server)
            assert value in run(['gsettings','get','org.gnome.desktop.remote-desktop.rdp',key_name],server).decode()
        run(['gsettings','set','org.gnome.desktop.remote-desktop.rdp.headless','enable','true'], server)
        status = subprocess.run(['grdctl','--headless','status'],env=server,capture_output=True,timeout=10)
        (base/'configuration.log').write_bytes(status.stdout+status.stderr)
        grd = spawn(['/usr/libexec/gnome-remote-desktop-daemon','--headless','--rdp-port','3391'], server, 'grd')
        def listening():
            if grd.poll() is not None:
                raise RuntimeError('Le serveur RDP privé a quitté')
            with socket.socket() as sock:
                sock.settimeout(.1)
                return sock.connect_ex(('127.0.0.1',3391)) == 0
        try:wait_for(listening, 'Serveur RDP privé absent', 25)
        finally:
            with (base/'configuration.log').open('ab') as log:
                log.write(run(['ss','-lntp'],server))
        # Le seul serveur X est un Xvfb privé avec authentification et sans TCP.
        authority = base/'Xauthority'
        authority.touch(mode=0o600)
        cookie = secrets.token_hex(16)
        run(['xauth','-f',str(authority)], client, input=f'add :65534 . {cookie}\n'.encode())
        reader, writer = os.pipe()
        spawn(['Xvfb','-displayfd',str(writer),'-screen','0','1400x1000x24','-noreset','-nolisten','tcp',
               '-auth',str(authority)], client, 'xvfb', pass_fds=(writer,))
        os.close(writer)
        assert select.select([reader],[],[],10)[0]
        number = int(os.read(reader,32).strip()); os.close(reader)
        run(['xauth','-f',str(authority)], client, input=f'add :{number} . {cookie}\n'.encode())
        client.update(DISPLAY=f':{number}', XAUTHORITY=str(authority), GDK_BACKEND='x11')
        pin = run(['openssl','x509','-in',str(certificate),'-noout','-fingerprint','-sha256'], server).decode().split('=')[1].strip().replace(':','').lower()
        rdp = spawn(['xfreerdp3','/v:127.0.0.1:3391','/u:'+user,'/from-stdin:force',
                     '/sec:nla','/auth-pkg-list:none,ntlm','/cert:fingerprint:sha256:'+pin,
                     '/clipboard:direction-to:all,files-to:all','-drives','/audio-mode:2',
                     '/size:1280x800','/t:RustRemote-CLIPRDR-Lab','/log-level:WARN'], client, 'client', stdin=subprocess.PIPE)
        rdp.stdin.write((password+'\n').encode());rdp.stdin.close()
        wait_for(lambda: b'RustRemote-CLIPRDR-Lab' in run(['xwininfo','-root','-tree'], client), 'Session RDP non établie', 30)
        report['nla_certificate_pinned'] = True
        server_probe = probe(server, 'server-probe')
        client_probe = probe(client, 'client-probe')
        request(server_probe, {'op':'focus'})
        time.sleep(.5)
        # Donner un véritable focus/serial Wayland à la surface de laboratoire via RDP.
        import ctypes as c
        x = c.CDLL('libX11.so.6'); xt = c.CDLL('libXtst.so.6')
        x.XOpenDisplay.argtypes=[c.c_char_p];x.XOpenDisplay.restype=c.c_void_p
        x.XFlush.argtypes=[c.c_void_p];x.XCloseDisplay.argtypes=[c.c_void_p]
        xt.XTestFakeMotionEvent.argtypes=[c.c_void_p,c.c_int,c.c_int,c.c_int,c.c_ulong]
        xt.XTestFakeButtonEvent.argtypes=[c.c_void_p,c.c_uint,c.c_int,c.c_ulong]
        os.environ['XAUTHORITY'] = str(authority)
        xd = x.XOpenDisplay(client['DISPLAY'].encode()); assert xd
        tree=run(['xwininfo','-root','-tree'],client).decode()
        import re
        window_id=int(next(re.search(r'0x[0-9a-f]+',line).group() for line in tree.splitlines() if 'RustRemote-CLIPRDR-Lab' in line),16)
        x.XSetInputFocus.argtypes=[c.c_void_p,c.c_ulong,c.c_int,c.c_ulong]
        x.XSetInputFocus(xd,window_id,1,0)
        xt.XTestFakeKeyEvent.argtypes=[c.c_void_p,c.c_uint,c.c_int,c.c_ulong]
        xt.XTestFakeKeyEvent(xd,9,1,0);xt.XTestFakeKeyEvent(xd,9,0,0)
        x.XFlush(xd);time.sleep(.2)
        xt.XTestFakeMotionEvent(xd,-1,400,300,0)
        xt.XTestFakeButtonEvent(xd,1,1,0);xt.XTestFakeButtonEvent(xd,1,0,0)
        x.XFlush(xd);x.XCloseDisplay(xd)
        time.sleep(.5)
        report['server_window']=request(server_probe,{'op':'state'})
        run([sys.executable,'-c','from PIL import ImageGrab;import os,sys;ImageGrab.grab(xdisplay=os.environ["DISPLAY"]).save(sys.argv[1])',str(base/'desktop.png')],client)
        for label, source, destination in [('client_to_server',client_probe,server_probe),
                                            ('server_to_client',server_probe,client_probe)]:
            value = 'CLIPRDR réel — été '+label
            assert request(source, {'op':'set','data':{'text/plain;charset=utf-8':value,'text/plain':value}}).get('ok')
            time.sleep(.5)
            result = request(destination, {'op':'get','mimes':['text/plain;charset=utf-8','text/plain']})
            assert result.get('text') == value, (label, result)
            report['text_'+label] = True
        report['file_cases'] = []
        for label, source, destination, home in [('client_to_server',client_probe,server_probe,client['HOME']),
                                               ('server_to_client',server_probe,client_probe,server['HOME'])]:
            fixtures = Path(home)/'fixtures';fixtures.mkdir()
            for name in ('un.txt','deux.txt'):
                (fixtures/name).write_text('Fichier de test — '+name)
            for name in ('dossier-un','dossier-deux'):
                (fixtures/name/'sous-dossier').mkdir(parents=True)
                (fixtures/name/'sous-dossier/été.txt').write_text('Contenu de test — '+name)
            cases = [('one_file',['un.txt']),('multiple_files',['un.txt','deux.txt']),
                     ('one_folder',['dossier-un']),('multiple_folders',['dossier-un','dossier-deux'])]
            for case, names in cases:
                paths=[fixtures/name for name in names]
                uris='\r\n'.join(path.as_uri() for path in paths)+'\r\n'
                request(source, {'op':'set','data':{'text/uri-list':uris,
                         'x-special/gnome-copied-files':'copy\n'+'\n'.join(path.as_uri() for path in paths)}})
                time.sleep(.5)
                received=request(destination, {'op':'get','mimes':['text/uri-list']})
                assert 'text' in received, (label,case,received)
                urls=[line for line in received['text'].splitlines() if line and not line.startswith('#')]
                assert len(urls)==len(paths), (label,case,urls)
                for source_path, url in zip(paths,urls):
                    parsed=urlparse(url);assert parsed.scheme=='file' and parsed.netloc in ('','localhost')
                    remote=Path(unquote(parsed.path))
                    assert remote!=source_path, 'URI du chemin source réutilisée : faux positif de copie'
                    mountpoints=[Path(line.split()[4].replace('\\040',' ')) for line in Path('/proc/self/mountinfo').read_text().splitlines() if ' - fuse' in line]
                    assert any(remote.is_relative_to(m) for m in mountpoints), 'Destination hors du montage CLIPRDR FUSE'
                    if source_path.is_file():
                        assert remote.read_bytes()==source_path.read_bytes()
                    else:
                        expected={str(p.relative_to(source_path)):p.read_bytes() for p in source_path.rglob('*') if p.is_file()}
                        actual={str(p.relative_to(remote)):p.read_bytes() for p in remote.rglob('*') if p.is_file()}
                        assert actual==expected
                report['file_cases'].append(label+':'+case)
        report['success'] = True
    except Exception as error:
        report['error'] = str(error)
        raise
    finally:
        for process in reversed(children):
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try:process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid,signal.SIGKILL);process.wait(timeout=3)
        for stream in streams:stream.close()
        (base/'result.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
        print(json.dumps(report,ensure_ascii=False),flush=True)


if __name__ == '__main__':
    if '--probe' in sys.argv:
        clipboard_probe()
    elif '--inside' in sys.argv:
        inside(Path(sys.argv[-1]))
    else:
        with tempfile.TemporaryDirectory(prefix='rustremote-cliprdr-') as directory:
            base=Path(directory)
            env={'PATH':os.environ['PATH'],'LANG':'C.UTF-8','RUSTREMOTE_RDP_NS':'1'}
            log=ROOT/'dist/rdp-clipboard-local.log'
            log.parent.mkdir(exist_ok=True)
            with log.open('wb') as output:
                result=subprocess.run(['unshare','--user','--map-root-user','--net','--mount',
                     '--propagation','private','--pid','--fork','--kill-child','--mount-proc',
                     sys.executable,str(SCRIPT),'--inside',str(base)],env=env,stdout=output,stderr=output,timeout=240)
            if (base/'desktop.png').is_file():
                shutil.copyfile(base/'desktop.png',ROOT/'dist/rdp-clipboard-desktop.png')
            if (base/'result.json').is_file():
                (ROOT/'dist/rdp-clipboard-result.json').write_bytes((base/'result.json').read_bytes())
            if result.returncode:
                for name in ('configuration','grd','shell','client','server-probe','client-probe'):
                    source=base/(name+'.log')
                    if source.exists():
                        with log.open('ab') as output:output.write(('\n'+name+'\n').encode()+source.read_bytes()[-7000:])
            print('Résultat CLIPRDR :',result.returncode,'·',log.relative_to(ROOT))
            raise SystemExit(result.returncode)
