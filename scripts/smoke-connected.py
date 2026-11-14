#!/usr/bin/env python3
"""Parcours UI réel vers le serveur loopback éphémère du test SSH/SFTP.
Affichage et clipboard X11 privés ; aucune frappe dans le bureau personnel.
"""
import json
import os
os.environ.setdefault("RUSTREMOTE_LANGUAGE", "fr")
from pathlib import Path
import re
import secrets
import select
import signal
import subprocess
import sys
import tempfile
import time
import pty
import threading
import termios

os.umask(0o077)
image=Path(sys.argv[1]).resolve()
config=Path(sys.argv[2]).resolve()
profile=json.loads((config/"rustremote/profiles.json").read_text())["profiles"][0]
assert profile["host"]=="127.0.0.1" and profile["options"]["config_file"]=="/dev/null"
known=Path(profile["options"]["known_hosts_file"])
assert known.is_relative_to(config.parent),"Le fichier de confiance doit être éphémère"
with tempfile.TemporaryDirectory(prefix="rustremote-ssh-display-") as directory:
    base=Path(directory)
    auth=base/"Xauthority";auth.touch(mode=0o600)
    cookie=secrets.token_hex(16)
    subprocess.run(["xauth","-f",str(auth)],input=f"add :65534 . {cookie}\n".encode(),check=True,capture_output=True)
    reader,writer=os.pipe()
    stream=open(base/"x.log","wb")
    display=subprocess.Popen(["Xvfb","-displayfd",str(writer),"-screen","0","1600x1000x24","-noreset","-nolisten","tcp","-auth",str(auth)],pass_fds=(writer,),stdout=stream,stderr=stream,start_new_session=True)
    os.close(writer)
    assert select.select([reader],[],[],10)[0]
    number=int(os.read(reader,32).strip());os.close(reader)
    subprocess.run(["xauth","-f",str(auth)],input=f"add :{number} . {cookie}\n".encode(),check=True,capture_output=True)
    runtime=base/"runtime";runtime.mkdir(mode=0o700)
    home=base/"home";home.mkdir(mode=0o700)
    os.environ.update(DISPLAY=f":{number}",XAUTHORITY=str(auth),GDK_BACKEND="x11",GSK_RENDERER="cairo",HOME=str(home),XDG_CACHE_HOME=str(base/"cache"),XDG_DATA_HOME=str(base/"data"),XDG_RUNTIME_DIR=str(runtime),XDG_CONFIG_HOME=str(config),GTK_USE_PORTAL="0",GIO_USE_VFS="local")
    subprocess.run(["dbus-update-activation-environment","DISPLAY","XAUTHORITY","GDK_BACKEND","HOME","XDG_CACHE_HOME","XDG_RUNTIME_DIR","GTK_USE_PORTAL","GIO_USE_VFS"],check=True,capture_output=True,timeout=5)
    import gi
    gi.require_version("Atspi","2.0")
    from gi.repository import Atspi, GLib
    Atspi.set_timeout(2000,3000)

    def walk(node):
        node.clear_cache()
        yield node
        for i in range(node.get_child_count()):
            child=node.get_child_at_index(i)
            if child: yield from walk(child)

    def app():
        context=GLib.MainContext.default()
        for _ in range(64):
            if not context.pending():break
            context.iteration(False)
        desktop=Atspi.get_desktop(0)
        root=next((n for n in (desktop.get_child_at_index(i) for i in range(desktop.get_child_count())) if n and n.get_name().lower()=="rustremote"),None)
        if root:root.set_cache_mask(Atspi.Cache.NONE)
        return root

    def find(name):
        root=app()
        return next((n for n in walk(root) if n.get_name()==name),None) if root else None

    def wait(predicate,message):
        deadline=time.monotonic()+25
        while time.monotonic()<deadline:
            try:
                value=predicate()
            except GLib.Error:
                assert process.poll() is None,f"L’application a quitté pendant le parcours (code {process.returncode})"
                time.sleep(.15)
                continue
            if value:return value
            time.sleep(.15)
        raise AssertionError(message)

    def click(name, scope=None):
        def current_button(node):
            if name not in {"Tout sélectionner", "Copier la sélection", "Coller dans le terminal…", "Confirmer le collage dans cette session"}:
                return True
            rect=node.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
            return rect.width>0 and rect.height>0
        def actionable():
            root=scope if scope is not None else app()
            nodes=[]
            for label in walk(root) if root else []:
                if label.get_name()!=name:continue
                n=label
                for _ in range(5):
                    if any(i.endswith("Action") for i in n.get_interfaces()) and any(Atspi.Action.get_action_name(n.get_action_iface(),i) in ("click","activate","press","toggle") for i in range(n.get_action_iface().get_n_actions())):
                        if n.get_state_set().contains(Atspi.StateType.SENSITIVE) and current_button(n):nodes.append(n)
                        break
                    n=n.get_parent()
                    if not n or n.get_role() in (Atspi.Role.FRAME,Atspi.Role.APPLICATION):break
            shown=[n for n in nodes if n.get_state_set().contains(Atspi.StateType.SHOWING) or n.get_state_set().contains(Atspi.StateType.VISIBLE)]
            return (shown or nodes)[-1] if nodes else None
        node=wait(actionable,"Action absente : "+name)
        root=app()
        shown=next((n for n in walk(root) if n.get_name()==name and n.get_state_set().contains(Atspi.StateType.SHOWING) and n.get_state_set().contains(Atspi.StateType.SENSITIVE) and "Action" in [i.rsplit('.',1)[-1] for i in n.get_interfaces()] and n.get_action_iface().get_n_actions() and current_button(n)),None)
        if shown:node=shown
        interface=node.get_action_iface()
        index=next(i for i in range(interface.get_n_actions()) if Atspi.Action.get_action_name(interface,i) in ("click","activate","press","toggle"))
        assert interface.do_action(index),name
        time.sleep(.2)

    def pointer_click(node, button):
        import ctypes as c
        rect=node.get_component_iface().get_extents(Atspi.CoordType.WINDOW)
        assert rect.width>0 and rect.height>0
        frame=node
        while frame and frame.get_role() not in (Atspi.Role.FRAME,Atspi.Role.DIALOG):frame=frame.get_parent()
        assert frame,"Fenêtre privée de la cible absente"
        origin=frame.get_component_iface().get_extents(Atspi.CoordType.WINDOW)
        px=rect.x-origin.x+rect.width//2
        py=rect.y-origin.y+rect.height//2
        x=c.CDLL("libX11.so.6");xt=c.CDLL("libXtst.so.6")
        x.XOpenDisplay.argtypes=[c.c_char_p];x.XOpenDisplay.restype=c.c_void_p
        x.XFlush.argtypes=[c.c_void_p];x.XCloseDisplay.argtypes=[c.c_void_p]
        xt.XTestFakeMotionEvent.argtypes=[c.c_void_p,c.c_int,c.c_int,c.c_int,c.c_ulong]
        xt.XTestFakeButtonEvent.argtypes=[c.c_void_p,c.c_uint,c.c_int,c.c_ulong]
        xd=x.XOpenDisplay(os.environ["DISPLAY"].encode());assert xd
        try:
            assert xt.XTestFakeMotionEvent(xd,-1,px,py,0)
            x.XFlush(xd);time.sleep(.1)
            assert xt.XTestFakeButtonEvent(xd,button,1,0)
            x.XFlush(xd);time.sleep(.1)
            assert xt.XTestFakeButtonEvent(xd,button,0,0);x.XFlush(xd)
        finally:x.XCloseDisplay(xd)

    def focus_window(title):
        import ctypes as c
        def mapped():
            result=subprocess.run(["xwininfo","-name",title],text=True,capture_output=True)
            return result.stdout if result.returncode==0 else None
        info=wait(mapped,"Window not mapped: "+title)
        wid=int(re.search(r"Window id: (0x[0-9a-fA-F]+)",info).group(1),16)
        x=c.CDLL("libX11.so.6")
        x.XOpenDisplay.argtypes=[c.c_char_p];x.XOpenDisplay.restype=c.c_void_p
        x.XSetInputFocus.argtypes=[c.c_void_p,c.c_ulong,c.c_int,c.c_ulong]
        x.XFlush.argtypes=[c.c_void_p];x.XCloseDisplay.argtypes=[c.c_void_p]
        display=x.XOpenDisplay(os.environ["DISPLAY"].encode());assert display
        try:x.XSetInputFocus(display,wid,1,0);x.XFlush(display)
        finally:x.XCloseDisplay(display)

    def select_combo(title,index):
        focus_window(title)
        current="English" if title=="Settings and security" else "Unassigned"
        label=wait(lambda:next((n for n in walk(app()) if n.get_role()==Atspi.Role.LABEL and n.get_name()==current and n.get_state_set().contains(Atspi.StateType.VISIBLE)),None),"Selected combo label missing")
        pointer_click(label,1)
        keypress("Home")
        for _ in range(index):keypress("Down")
        keypress("Return")

    def close_window(title):
        # Private Xvfb only: address the modal itself, not a same-named main-window button.
        import ctypes as c
        info=wait(lambda:subprocess.run(["xwininfo","-name",title],text=True,capture_output=True).stdout,"Modal absent: "+title)
        wid=int(re.search(r"Window id: (0x[0-9a-fA-F]+)",info).group(1),16)
        class Message(c.Structure):
            _fields_=[("type",c.c_int),("serial",c.c_ulong),("send_event",c.c_int),("display",c.c_void_p),("window",c.c_ulong),("message_type",c.c_ulong),("format",c.c_int),("data",c.c_long*5)]
        x=c.CDLL("libX11.so.6");x.XOpenDisplay.argtypes=[c.c_char_p];x.XOpenDisplay.restype=c.c_void_p
        x.XInternAtom.argtypes=[c.c_void_p,c.c_char_p,c.c_int];x.XInternAtom.restype=c.c_ulong
        x.XSendEvent.argtypes=[c.c_void_p,c.c_ulong,c.c_int,c.c_long,c.c_void_p];x.XFlush.argtypes=[c.c_void_p];x.XCloseDisplay.argtypes=[c.c_void_p]
        display=x.XOpenDisplay(os.environ["DISPLAY"].encode());assert display
        try:
            event=(c.c_long*24)();msg=c.cast(event,c.POINTER(Message)).contents
            msg.type=33;msg.window=wid;msg.display=display;msg.message_type=x.XInternAtom(display,b"WM_PROTOCOLS",0);msg.format=32;msg.data[0]=x.XInternAtom(display,b"WM_DELETE_WINDOW",0)
            assert x.XSendEvent(display,wid,0,0,c.byref(event));x.XFlush(display)
        finally:x.XCloseDisplay(display)
        time.sleep(.3)

    def keypress(name, ctrl=False):
        import ctypes as c
        x=c.CDLL("libX11.so.6");xt=c.CDLL("libXtst.so.6")
        x.XOpenDisplay.argtypes=[c.c_char_p];x.XOpenDisplay.restype=c.c_void_p
        x.XStringToKeysym.argtypes=[c.c_char_p];x.XStringToKeysym.restype=c.c_ulong
        x.XKeysymToKeycode.argtypes=[c.c_void_p,c.c_ulong];x.XKeysymToKeycode.restype=c.c_uint
        x.XFlush.argtypes=[c.c_void_p];x.XCloseDisplay.argtypes=[c.c_void_p]
        xt.XTestFakeKeyEvent.argtypes=[c.c_void_p,c.c_uint,c.c_int,c.c_ulong]
        xd=x.XOpenDisplay(os.environ["DISPLAY"].encode());assert xd
        try:
            key=x.XKeysymToKeycode(xd,x.XStringToKeysym(name.encode()))
            control=x.XKeysymToKeycode(xd,x.XStringToKeysym(b"Control_L"))
            if ctrl:xt.XTestFakeKeyEvent(xd,control,1,0)
            xt.XTestFakeKeyEvent(xd,key,1,0);xt.XTestFakeKeyEvent(xd,key,0,0)
            if ctrl:xt.XTestFakeKeyEvent(xd,control,0,0)
            x.XFlush(xd)
        finally:x.XCloseDisplay(xd)
        time.sleep(.3)

    def chooser_location(path):
        keypress("l",ctrl=True)
        def entry():
            root=app()
            return next((n for n in walk(root) if n.get_state_set().contains(Atspi.StateType.FOCUSED) and any(i.endswith("EditableText") for i in n.get_interfaces())),None) if root else None
        node=wait(entry,"Champ de chemin du sélecteur absent")
        assert Atspi.EditableText.set_text_contents(node,str(path))

    def context_menu():
        root=app()
        nodes=[n for n in walk(root) if n.get_name().startswith("Terminal — ") and n.get_state_set().contains(Atspi.StateType.SHOWING)]
        pointer_click(nodes[-1],3)
        wait(lambda:find("Coller"),"Menu contextuel natif du terminal absent")

    copy_reads=False

    def contents(name=None):
        root=app()
        nodes=[n for n in walk(root) if n.get_name()=="Terminal — "+(name or profile["name"])] if root else []
        if not nodes:return ""
        shown=[n for n in nodes if n.get_state_set().contains(Atspi.StateType.SHOWING)]
        node=(shown or nodes)[-1]
        if copy_reads:
            # Après bascule, VTE peut exposer un buffer AT-SPI ancien. Cette
            # observation utilise le clipboard privé, jamais pour attendre la série.
            click("Tout sélectionner");click("Copier la sélection")
            selection=subprocess.run(["xclip","-selection","clipboard","-out"],timeout=3,capture_output=True)
            value=selection.stdout.decode() if selection.returncode==0 else ""
            contents.last=value
            return value
        count=Atspi.Text.get_character_count(node)
        value=Atspi.Text.get_text(node,0,count)
        contents.last=value
        return value

    def paste(text, right_click=False):
        writer=subprocess.Popen(["xclip","-selection","clipboard","-in"],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        writer.communicate(text.encode(),timeout=3)
        assert subprocess.check_output(["xclip","-selection","clipboard","-out"],timeout=3).decode()==text
        time.sleep(.3)  # Laisser GDK traiter le changement de propriétaire X11.
        if right_click:
            context_menu();click("Coller")
        else:
            click("Coller dans le terminal…")
        if "\n" in text or "\r" in text:
            # Attendre la vraie surface X11 : un ancien nœud AT-SPI de dialogue
            # fermé peut encore être présent, tandis que GDK lit le clipboard.
            def mapped_prompt():
                info=subprocess.run(["xwininfo","-name","Confirmer le collage multiligne"],capture_output=True,text=True)
                return info.returncode==0 and "Map State: IsViewable" in info.stdout
            wait(mapped_prompt,"Confirmation de collage non affichée sur le bureau privé")
            def preview_text():
                nodes=[n for n in walk(app()) if n.get_name()=="Texte du collage à confirmer" and n.get_state_set().contains(Atspi.StateType.SHOWING)]
                nodes=[n for n in nodes if n.get_component_iface().get_extents(Atspi.CoordType.SCREEN).height>0]
                return Atspi.Text.get_text(nodes[-1],0,-1) if nodes else None
            wait(lambda:preview_text() is not None,"Aperçu du collage absent")
            actual=preview_text()
            external=subprocess.run(["xclip","-selection","clipboard","-out"],capture_output=True,timeout=3).stdout.decode()
            assert actual==text,("Le collage ne correspond pas au clipboard privé",repr(actual),repr(text),"clipboard X11 encore correct",external==text)

    def text(name,value):
        def target():
            root=app()
            return next((n for n in walk(root) if n.get_name()==name and n.get_state_set().contains(Atspi.StateType.SENSITIVE) and any(i.endswith("EditableText") for i in n.get_interfaces())),None) if root else None
        node=wait(target,"Champ absent ou occupé : "+name)
        assert Atspi.EditableText.set_text_contents(node,value),name

    def field(name):
        node=find(name)
        if not node and name=="SFTP status":
            root=app()
            node=next((n for n in walk(root) if n.get_role()==Atspi.Role.LABEL and n.get_name().startswith(("Authenticated SFTP", "SFTP in progress", "Remote text loaded", "SFTP refusé", "Erreur SFTP", "Lot terminé", "Envoi terminé", "Téléchargement terminé"))),None) if root else None
            return node.get_name() if node else ""
        if not node and name=="État SFTP réel":
            root=app()
            node=next((n for n in walk(root) if n.get_role()==Atspi.Role.LABEL and n.get_name().startswith(("Listing SFTP", "SFTP en cours", "SFTP refusé", "Erreur SFTP", "Texte distant", "Dossier distant", "Envoi terminé", "Téléchargement terminé", "Destination existante", "Déconnexion /", "Le fichier distant", "Liens et fichiers", "Élément distant", "Permissions distantes", "Suppression terminée", "Lot terminé"))),None) if root else None
            return node.get_name() if node else ""
        return Atspi.Text.get_text(node,0,-1) if node else ""

    def remote(name):
        node=wait(lambda:next((n for n in walk(app()) if n.get_role()==Atspi.Role.LABEL and n.get_name().startswith((name+" ·",name+" (lien/spécial) ·"))),None),"Fichier distant absent : "+name)
        while node.get_role()!=Atspi.Role.LIST_ITEM:node=node.get_parent()
        Atspi.Selection.clear_selection(node.get_parent())
        assert Atspi.Selection.select_child(node.get_parent(),node.get_index_in_parent())
        time.sleep(.2)

    def test_choosers(data_root):
        chooser_files=config.parent/"chooser-uploads";chooser_files.mkdir()
        for file in ("chooser-un.txt","chooser-deux.txt"):(chooser_files/file).write_text(file+" — été")
        click("Choisir les fichiers à envoyer…")
        wait(lambda:find("Envoyer vers le dossier SFTP courant"),"Sélecteur local de fichiers absent")
        chooser_location(chooser_files);keypress("Return")
        node=wait(lambda:find("chooser-un.txt"),"Fichiers locaux du sélecteur absents")
        pointer_click(node,1);keypress("a",ctrl=True);click("Envoyer")
        wait(lambda:all((data_root/file).exists() for file in ("chooser-un.txt","chooser-deux.txt")),"Envoi multiple depuis le sélecteur échoué")
        for file in ("chooser-un.txt","chooser-deux.txt"):assert (data_root/file).read_bytes()==(chooser_files/file).read_bytes()
        chooser_folder=config.parent/"chooser-folder";chooser_folder.mkdir();(chooser_folder/"contenu.txt").write_text("Dossier choisi — été")
        click("Choisir les dossiers à envoyer…")
        wait(lambda:find("Envoyer vers le dossier SFTP courant"),"Sélecteur local de dossiers absent")
        chooser_location(chooser_folder);click("Envoyer")
        wait(lambda:(data_root/"chooser-folder/contenu.txt").exists(),"Envoi de dossier depuis le sélecteur échoué")
        assert (data_root/"chooser-folder/contenu.txt").read_bytes()==(chooser_folder/"contenu.txt").read_bytes()
        result["sftp_native_choosers_multiple_files_and_folder"]=True

    launch=[str(image)]
    if os.environ.get("RUSTREMOTE_GDB")=="1":launch=["gdb","--batch","-ex","run","-ex","thread apply all bt 10","--args",str(image)]
    serial_master,serial_slave=pty.openpty()
    def test_tunnels():
        # Trois modes OpenSSH réels, bornés à l’écho de la fixture loopback.
        import socket
        echo_port=int((config.parent/"forward-echo-port").read_text())
        for mode in (0,1,2):
            with socket.socket() as probe:
                probe.bind(("127.0.0.1",0));listen_port=probe.getsockname()[1]
            click("Tunnels");text("Port d’écoute du tunnel",str(listen_port));text("Port destination du tunnel",str(echo_port))
            if mode:
                focus_window("Tunnels SSH")
                click("Local — poste vers serveur")
                choice=("Local — poste vers serveur","Distant — serveur vers poste","SOCKS — proxy local")[mode]
                keypress("Home")
                for _ in range(mode):keypress("Down")
                keypress("Return")
                wait(lambda:any(n.get_role()==Atspi.Role.COMBO_BOX and n.get_name()==choice for n in walk(app())),"Type de tunnel non sélectionné")
            click("Vérifier le tunnel SSH");click("Autoriser et ouvrir le tunnel")
            def tunnel_echo():
                try:
                    with socket.create_connection(("127.0.0.1",listen_port),timeout=1) as stream:
                        if mode==2:
                            stream.sendall(b"\x05\x01\x00")
                            greeting=b""
                            while len(greeting)<2:
                                block=stream.recv(2-len(greeting))
                                if not block:return False
                                greeting+=block
                            assert greeting==b"\x05\x00",greeting.hex()
                            stream.sendall(b"\x05\x01\x00\x01"+socket.inet_aton("127.0.0.1")+echo_port.to_bytes(2,"big"))
                            reply=b""
                            while len(reply)<10:
                                block=stream.recv(10-len(reply))
                                if not block:return False
                                reply+=block
                            assert reply[:2]==b"\x05\x00"
                        stream.sendall(b"RustRemote tunnel fixture")
                        return stream.recv(128)==b"RustRemote tunnel fixture"
                except OSError:return False
            wait(tunnel_echo,"Tunnel SSH sans écho bidirectionnel, mode "+str(mode))
            click("Déconnecter le terminal")
            def closed_tunnel():
                with socket.socket() as stream:
                    stream.settimeout(.2)
                    return stream.connect_ex(("127.0.0.1",listen_port))!=0
            wait(closed_tunnel,"Port du tunnel encore actif après déconnexion")
            result["ssh_forward_mode_"+str(mode)+"_echo_and_disconnect"]=True

    workspace=json.loads((config/"rustremote/profiles.json").read_text())
    serial_name="Switch série — laboratoire"
    workspace["profiles"].append({"name":serial_name,"host":"","port":22,"username":"","favorite":False,"folder":"Linux","platform":"Unknown","simulation":False,"options":{"protocol":"Serial","device":os.ttyname(serial_slave),"baud":115200,"data_bits":8,"parity":"n","stop_bits":1,"flow":"n"}})
    (config/"rustremote/profiles.json").write_text(json.dumps(workspace))
    serial_received=bytearray()
    serial_commands=[]
    serial_replies=[]
    serial_stop=threading.Event()
    def switch():
        pending=b""
        while not serial_stop.is_set():
            if not select.select([serial_master],[],[],.2)[0]:continue
            received=os.read(serial_master,4096)
            serial_received.extend(received[:max(0,65536-len(serial_received))])
            pending+=received.replace(b"\r",b"\n")
            while b"\n" in pending:
                line,pending=pending.split(b"\n",1)
                if not line.strip():continue
                serial_commands.append(line.strip().decode(errors="replace") if line.strip() in (b"show version",b"show interfaces") else "Commande inconnue")
                if line.strip()==b"show version":
                    os.write(serial_master,b"\r\nRustRemote Serial Switch 1.0\r\nswitch> ");serial_replies.append("version")
                elif line.strip()==b"show interfaces":
                    os.write(serial_master,b"\r\nGi0/1 up 1000Mb/s\r\nswitch> ");serial_replies.append("interfaces")
    serial_worker=threading.Thread(target=switch);serial_worker.start()
    keyring=None
    if os.environ.get("RUSTREMOTE_TEST_PASSWORDS")=="1":
        keyring=subprocess.Popen(["gnome-keyring-daemon","--foreground","--components=secrets","--unlock"],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,env=os.environ)
        keyring.stdin.write(secrets.token_urlsafe(24).encode());keyring.stdin.close()
        time.sleep(.5)
        assert keyring.poll() is None,"Coffre de laboratoire indisponible"
    app_env=dict(os.environ)
    if os.environ.get("RUSTREMOTE_ENGLISH_GUI")=="1":app_env.pop("RUSTREMOTE_LANGUAGE",None)
    process=subprocess.Popen(launch,env=app_env)
    result={"target":"127.0.0.1","personal_desktop_accessed":False}
    try:
        if os.environ.get("RUSTREMOTE_ENGLISH_GUI")=="1":
            def workspace():return json.loads((config/"rustremote/profiles.json").read_text())
            def capture(name,title=None):
                reply=subprocess.check_output(["gdbus","call","--session","--dest","org.freedesktop.DBus","--object-path","/org/freedesktop/DBus","--method","org.freedesktop.DBus.GetConnectionUnixProcessID","com.rustremote.Preview"],text=True)
                args=[sys.executable,str(Path(__file__).with_name("capture-preview.py")),"--pid",re.search(r"uint32 (\d+)",reply).group(1),"--output","dist/english-"+name+".png"]
                if title:args += ["--title",title]
                subprocess.run(args,check=True,timeout=12)
            def select_row(label):
                node=wait(lambda:next((n for n in walk(find("Remote files")) if n.get_role()==Atspi.Role.LABEL and n.get_name().startswith(label+" ·")),None),"Missing file: "+label)
                while node.get_role()!=Atspi.Role.LIST_ITEM:node=node.get_parent()
                assert Atspi.Selection.select_child(node.get_parent(),node.get_index_in_parent())
                return node
            def drag(source,target,title="RustRemote"):
                import ctypes as c
                focus_window(title)
                start=source.get_component_iface().get_extents(Atspi.CoordType.WINDOW);end=target.get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                x=c.CDLL("libX11.so.6");xt=c.CDLL("libXtst.so.6")
                x.XOpenDisplay.argtypes=[c.c_char_p];x.XOpenDisplay.restype=c.c_void_p;x.XFlush.argtypes=[c.c_void_p];x.XCloseDisplay.argtypes=[c.c_void_p]
                xt.XTestFakeMotionEvent.argtypes=[c.c_void_p,c.c_int,c.c_int,c.c_int,c.c_ulong];xt.XTestFakeButtonEvent.argtypes=[c.c_void_p,c.c_uint,c.c_int,c.c_ulong]
                display=x.XOpenDisplay(os.environ["DISPLAY"].encode());assert display
                try:
                    def origin(node):
                        while node.get_role() not in (Atspi.Role.FRAME,Atspi.Role.DIALOG):node=node.get_parent()
                        return node.get_component_iface().get_extents(Atspi.CoordType.WINDOW)
                    so=origin(source);eo=origin(target)
                    sx,sy=start.x-so.x+start.width//2,start.y-so.y+start.height//2;ex,ey=end.x-eo.x+end.width//2,end.y-eo.y+end.height//2
                    print("Private DND coordinates:",sx,sy,ex,ey,flush=True)
                    xt.XTestFakeMotionEvent(display,-1,sx,sy,0);x.XFlush(display);time.sleep(.2);xt.XTestFakeButtonEvent(display,1,1,0);x.XFlush(display)
                    for n in range(1,21):xt.XTestFakeMotionEvent(display,-1,int(sx+(ex-sx)*n/20),int(sy+(ey-sy)*n/20),0);x.XFlush(display);time.sleep(.06)
                    xt.XTestFakeButtonEvent(display,1,0,0);x.XFlush(display)
                finally:x.XCloseDisplay(display)
            def exercise_discovery():
                pointer_click(wait(lambda:find("Linux"),"Folder menu target absent"),3)
                click("New Apple connection in this folder")
                text("Profile name","Folder Mac");text("Host / IP address","folder-mac.example.invalid");text("Username","folderadmin")
                click("Local simulation (no network)");click("Save profile")
                wait(lambda:any(p["name"]=="Folder Mac" for p in workspace()["profiles"]),"Folder Apple profile missing")
                folder_mac=next(p for p in workspace()["profiles"] if p["name"]=="Folder Mac")
                assert folder_mac["folder"]=="Linux" and folder_mac["platform"]=="MacOs"
                assert folder_mac["options"]["tunnel"]["username"]=="folderadmin"
                assert folder_mac["options"]["tunnel"]["host"]==folder_mac["host"]
                result["folder_right_click_apple_and_gateway_defaults"]=True
                click("Tools");click("Discover devices…");wait(lambda:find("Network discovery"),"Discovery absent")
                pointer_click(wait(lambda:find("Local simulation — no network packets"),"Simulation checkbox absent"),1)
                click("Find devices")
                wait(lambda:any("Real scan blocked" in n.get_name() for n in walk(app())),"Unapproved real scan was not blocked")
                pointer_click(wait(lambda:find("I am authorized to scan this network range"),"Consent checkbox absent"),1)
                text("Authorized IPv4 range","8.8.8.8")
                assert not find("I am authorized to scan this network range").get_state_set().contains(Atspi.StateType.CHECKED),"Consent was retained for a changed scope"
                click("Find devices")
                wait(lambda:any("Only private and loopback" in n.get_name() for n in walk(app())),"Public scan was not refused")
                text("Authorized IPv4 range","127.0.0.1")
                pointer_click(wait(lambda:find("Local simulation — no network packets"),"Simulation checkbox absent"),1);click("Find devices")
                wait(lambda:any(n.get_name().startswith("192.0.2.11 ·") for n in walk(app())),"Discovery fixtures missing")
                capture("discovery","Network discovery")
                pointer_click(wait(lambda:find("Local simulation — no network packets"),"Simulation checkbox absent"),1)
                wait(lambda:not any(n.get_name().startswith("192.0.2.") for n in walk(find("Discovered devices"))),"Old results survived a mode change")
                pointer_click(wait(lambda:find("Local simulation — no network packets"),"Simulation checkbox absent"),1)
                click("Find devices")
                wait(lambda:any(n.get_name().startswith("192.0.2.11 ·") for n in walk(app())),"Fresh simulation absent")
                text("Authorized IPv4 range","127.0.0.2")
                wait(lambda:not any(n.get_name().startswith("192.0.2.") for n in walk(find("Discovered devices"))),"Old results survived a scope change")
                click("Find devices")
                wait(lambda:any(n.get_name().startswith("192.0.2.11 ·") for n in walk(app())),"Discovery not restartable after invalidation")
                result["discovery_results_invalidated_on_scope_and_mode_change"]=True
                for address,choice,name,user in [("192.0.2.11","Add as Apple / ARD…","Discovered Mac","macscan"),("192.0.2.10","Add as Windows / RDP…","Discovered Windows","Administrator"),("192.0.2.12","Add as Linux / SSH…","Discovered Linux","linuxscan")]:
                    node=wait(lambda:next((n for n in walk(app()) if n.get_name().startswith(address+" ·")),None),"Device row missing")
                    if name=="Discovered Linux":click("Add as connection…")
                    else:pointer_click(node,3)
                    click(choice);wait(lambda:find("Connection settings"),"Discovered profile editor absent")
                    text("Profile name",name);text("Username",user)
                    if name=="Discovered Windows":text("AD domain / realm","LAB")
                    if name=="Discovered Mac":
                        select_combo("Connection settings",workspace()["folders"].index("Linux")+1)
                    else:text("New session folder","Discovered/"+("Windows" if name=="Discovered Windows" else "Linux"))
                    click("Save profile")
                    wait(lambda:any(p["name"]==name for p in workspace()["profiles"]),"Discovered profile not saved")
                    saved=next(p for p in workspace()["profiles"] if p["name"]==name)
                    assert saved["host"]==address and saved["username"]==user and saved["simulation"]
                    if name=="Discovered Mac":assert saved["folder"]=="Linux" and saved["platform"]=="MacOs" and saved["options"]["tunnel"]["username"]==user
                    elif name=="Discovered Windows":assert saved["options"]["domain"]=="LAB" and saved["platform"]=="Windows"
                    else:assert saved["platform"]=="Linux" and saved["options"]["protocol"]=="Ssh"
                result["discovery_simulated_context_profiles_accounts_folders"]=True
                result["discovery_simulated_results_stay_offline"]=True
                result["discovery_real_scan_consent_and_public_scope_blocked"]=True
                close_window("Network discovery")
            wait(lambda:find("Open profiles"),"Default English startup absent");click("Unlock / open")
            wait(lambda:find("Actions "+profile["name"]),"Profiles absent")
            result["english_default_without_override"]=True
            if os.environ.get("RUSTREMOTE_DISCOVERY_ONLY")=="1":
                exercise_discovery();result["success"]=True;raise SystemExit(0)

            for name in ("Linux/Servers/Production","Archive"):
                click("New session folder");text("Folder name",name);click("Create folder")
                wait(lambda:name in workspace()["folders"],"Folder not saved: "+name)
            click("Actions "+profile["name"]);click("Move…")
            position=workspace()["folders"].index("Linux/Servers/Production")+1
            select_combo("Move to folder",position);click("Move")
            wait(lambda:workspace()["profiles"][0]["folder"]=="Linux/Servers/Production","Context move not persisted")
            result["profile_context_move_and_nested_creation"]=True
            drag(wait(lambda:find("Servers"),"Folder drag source absent"),wait(lambda:find("Archive"),"Folder drop target absent"))
            wait(lambda:"Archive/Servers/Production" in workspace()["folders"],"Folder drag and drop failed")
            assert workspace()["profiles"][0]["folder"]=="Archive/Servers/Production"
            result["folder_drag_moves_descendants_and_profiles"]=True
            time.sleep(.5);app()
            source=wait(lambda:next((n for n in walk(app()) if n.get_role()==Atspi.Role.LABEL and n.get_name().endswith(profile["name"]) and not n.get_name().startswith("Actions")),None),"Profile drag source absent")
            drag(source,wait(lambda:find("Linux"),"Root folder absent"))
            wait(lambda:workspace()["profiles"][0]["folder"]=="Linux","Profile drag failed")
            result["profile_drag_and_drop"]=True
            click("Actions "+profile["name"]);click("Edit");text("Profile name","Linux lab");click("Save profile");profile["name"]="Linux lab"
            wait(lambda:find("Actions Linux lab"),"Renamed English profile absent")
            for action in ("collapse-folders","expand-folders","focus-terminal","focus-terminal","reset-layout"):
                subprocess.run(["gapplication","action","com.rustremote.Preview",action],check=True,timeout=5)
            click("View");wait(lambda:find("Expand all folders"),"English View menu absent");click("Expand all folders")
            result["view_actions"]=True
            click("Apple");wait(lambda:find("Connection settings"),"Apple profile absent")
            text("Profile name","Mac simulation");text("Host / IP address","mac.example.invalid");text("Username","macadmin")
            text("SSH gateway","mac.example.invalid");text("SSH gateway username","macadmin")
            click("Local simulation (no network)");capture("apple","Connection settings");click("Save profile")
            wait(lambda:any(p["name"]=="Mac simulation" for p in workspace()["profiles"]),"Apple profile was not saved")
            mac=next(p for p in workspace()["profiles"] if p["name"]=="Mac simulation")
            assert mac["platform"]=="MacOs" and mac["options"]["protocol"]=="Vnc" and mac["simulation"]
            assert mac["port"]==5900 and mac["options"]["tunnel"]["host"]==mac["host"]
            result["apple_profile_preset_and_simulation_saved"]=True
            exercise_discovery()
            click("RDP");wait(lambda:find("Connection settings"),"RDP form absent");text("Profile name","Windows lab");text("Host / IP address","windows.example.invalid");text("Username","Administrator");text("AD domain / realm","LAB");capture("rdp","Connection settings");click("Cancel")
            click("Tools");click("System information…")
            wait(lambda:find("System information"),"System information absent");capture("system","System information");close_window("System information")
            subprocess.run(["gapplication","action","com.rustremote.Preview","key-fingerprint"],check=True)
            text("SSH public key",profile["options"]["identity_file"]+".pub");click("Calculate SHA256 fingerprint")
            wait(lambda:any("SHA256:" in n.get_name() for n in walk(app())),"Fingerprint tool failed");close_window("SSH public key fingerprint")
            result["tools_system_information_and_public_key_fingerprint"]=True
            click("Actions Linux lab");click("Connect");click("Allow and connect")
            def copied_terminal():
                click("Select all");click("Copy selection")
                out=subprocess.run(["xclip","-selection","clipboard","-out"],timeout=3,capture_output=True)
                return out.stdout.decode(errors="replace") if out.returncode==0 else ""
            wait(lambda:"SSH intégré — laboratoire été" in copied_terminal(),"Real SSH banner absent")
            terminal=next(n for n in walk(app()) if n.get_name()=="Terminal — Linux lab")
            pointer_click(terminal,1);capture("ssh")
            click("SFTP");click("Connect / Refresh SFTP")
            wait(lambda:"Authenticated SFTP listing" in field("SFTP status"),"Native SFTP absent")
            upload=config.parent/"graphical-local";upload.mkdir();(upload/"docs").mkdir();(upload/"docs/readme.txt").write_text("Graphical folder transfer — local lab")
            (upload/"client-note.txt").write_text("Graphical upload — local lab")
            text("Local folder",str(upload));click("Refresh local folder")
            local_list=wait(lambda:find("Local files"),"Local pane absent")
            wait(lambda:any(n.get_role()==Atspi.Role.LABEL and n.get_name().startswith("client-note.txt ·") for n in walk(local_list)),"Local listing missing")
            assert Atspi.Selection.select_child(local_list,0);assert Atspi.Selection.select_child(local_list,1);click("Upload →")
            data=config.parent/"server-data"
            wait(lambda:(data/"docs/readme.txt").is_file() and (data/"client-note.txt").is_file(),"Graphical uploads missing")
            assert (data/"docs/readme.txt").read_bytes()==(upload/"docs/readme.txt").read_bytes()
            wait(lambda: any(n.get_role()==Atspi.Role.LABEL and n.get_name().startswith("client-note.txt ·") for n in walk(find("Remote files"))),"Remote pane did not refresh")
            result["sftp_graphical_multiple_files_folders_upload"]=True
            filtered_row=select_row("client-note.txt")
            click("Refresh local folder");time.sleep(.4)
            assert find("Remote files").get_selection_iface().get_n_selected_children()==1,"Local refresh cleared the remote selection"
            text("SFTP filter","docs");time.sleep(.4)
            assert not find("Remote files").get_selection_iface().get_n_selected_children(),"Hidden transfer selection retained"
            text("SFTP filter","");time.sleep(.4)
            result["sftp_filter_clears_hidden_selection"]=True

            (upload/"drag-note.txt").write_text("Transferred by graphical drag and drop")
            click("Refresh local folder")
            local_node=wait(lambda:next((n for n in walk(find("Local files")) if n.get_role()==Atspi.Role.LABEL and n.get_name().startswith("drag-note.txt ·")),None),"Local drag file absent")
            local_row=local_node
            while local_row.get_role()!=Atspi.Role.LIST_ITEM:local_row=local_row.get_parent()
            Atspi.Selection.clear_selection(find("Local files"));assert Atspi.Selection.select_child(find("Local files"),local_row.get_index_in_parent())
            drag(local_node,find("Remote files"),"SFTP — Linux lab")
            wait(lambda:(data/"drag-note.txt").is_file(),"Graphical file drag did not upload")
            assert (data/"drag-note.txt").read_bytes()==(upload/"drag-note.txt").read_bytes()
            wait(lambda:find("Save remote file") or any(n.get_role()==Atspi.Role.LABEL and n.get_name().startswith("drag-note.txt ·") for n in walk(find("Remote files"))),"Remote listing did not refresh after drag")
            result["sftp_graphical_drag_and_drop_upload"]=True
            row=select_row("client-note.txt");focus_window("SFTP — Linux lab");pointer_click(row,1);keypress("Return")
            wait(lambda:"Graphical upload" in field("Remote SFTP editor"),"Remote editor did not open")
            text("Remote SFTP editor","Edited remotely from the graphical browser")
            click("Save remote file")
            wait(lambda:(data/"client-note.txt").read_text()=="Edited remotely from the graphical browser","Remote edit was not saved")
            result["sftp_graphical_edit_readback"]=True
            download=config.parent/"graphical-download";download.mkdir();text("Local folder",str(download));click("Refresh local folder")
            node=find("Remote files");Atspi.Selection.clear_selection(node);select_row("client-note.txt");select_row("docs");click("← Download")
            wait(lambda:(download/"docs/readme.txt").is_file() and (download/"client-note.txt").is_file(),"Graphical download missing")
            assert (download/"client-note.txt").read_bytes()==(data/"client-note.txt").read_bytes()
            assert (download/"docs/readme.txt").read_bytes()==(data/"docs/readme.txt").read_bytes()
            result["sftp_graphical_multiple_files_folders_download"]=True
            wait(lambda:any(n.get_role()==Atspi.Role.LABEL and n.get_name().startswith("docs ·") for n in walk(find("Remote files"))),"Final SFTP refresh incomplete")
            wait(lambda:any(n.get_role()==Atspi.Role.LABEL and n.get_name().startswith("docs ·") for n in walk(find("Local files"))),"Downloaded local folder absent from pane")
            time.sleep(.3);capture("sftp","SFTP — Linux lab");click("Close SFTP browser")
            click("Settings");wait(lambda:find("Settings and security"),"Settings absent");capture("settings","Settings and security")
            select_combo("Settings and security",1);click("Save settings")
            settings=config/"rustremote/settings.json";wait(lambda:settings.exists() and json.loads(settings.read_text())["language"]=="fr","French preference not saved")
            subprocess.run(["gapplication","action","com.rustremote.Preview","quit"],check=True);process.wait(timeout=10)
            process=subprocess.Popen(launch,env=app_env)
            wait(lambda:find("Ouvrir les profils"),"French setting did not apply after restart");click("Déverrouiller / ouvrir")
            wait(lambda:find("Paramètres"),"French menu absent after restart")
            result["french_setting_persisted_after_restart"]=True
            result["success"]=True
            raise SystemExit(0)
        wait(lambda:find("Ouvrir les profils"),"Déverrouillage absent")
        click("Déverrouiller / ouvrir")
        wait(lambda:find("Actions "+profile["name"]),"Profil non chargé")
        if os.environ.get("RUSTREMOTE_TEST_REQUESTS")=="1":
            click("Nouveau dossier de connexions");text("Nom du dossier","À classer");click("Créer le dossier")
            folder=wait(lambda:find("À classer"),"Nouveau dossier de la barre latérale absent")
            pointer_click(folder,3);click("Renommer ce dossier")
            text("Nouveau nom du dossier","À ranger");click("Renommer le dossier")
            wait(lambda:"À ranger" in json.loads((config/"rustremote/profiles.json").read_text())["folders"],"Renommage du dossier non enregistré")
            pointer_click(wait(lambda:find("À ranger"),"Dossier renommé absent"),3);click("Supprimer ce dossier vide");click("Confirmer la suppression du dossier vide")
            wait(lambda:"À ranger" not in json.loads((config/"rustremote/profiles.json").read_text())["folders"],"Suppression du dossier vide non enregistrée")
            result["session_folder_create_rename_delete_via_context"]=True
            def state(name):
                return subprocess.check_output(["gdbus","call","--session","--dest","com.rustremote.Preview","--object-path","/com/rustremote/Preview","--method","org.gtk.Actions.Describe",name],text=True,timeout=5)
            for title,name in [("Panneau Sessions","view-sessions"),("Panneau SFTP","view-sftp"),("Barre d’outils","view-toolbar"),("Barre d’état","view-status")]:
                before=state(name)
                click("Affichage");pointer_click(wait(lambda:find(title),"Entrée Affichage absente"),1);time.sleep(.3)
                assert state(name)!=before,(name,"État inchangé")
                subprocess.run(["gapplication","action","com.rustremote.Preview",name],check=True,timeout=5)
                wait(lambda:state(name)==before,"État Affichage non rétabli")
            click("Coffre");wait(lambda:find("Coffre des identifiants"),"Coffre absent");click("Fermer le coffre")
            click("Paramètres");wait(lambda:find("Paramètres et sécurité"),"Paramètres absents");click("Fermer")
            result["display_menu_toggles_and_vault_distinct_settings"]=True
            print("OK : dossiers, menus contextuels, Affichage, Coffre et Paramètres",flush=True)
        if os.environ.get("RUSTREMOTE_SERIAL_ONLY") != "1" and os.environ.get("RUSTREMOTE_PASSWORD_ONLY") != "1":
            click("Actions "+profile["name"]);click("Se connecter")
            wait(lambda:find("Confirmer la connexion"),"Confirmation absente")
            click("Autoriser et connecter")
            wait(lambda:"SSH intégré — laboratoire été" in contents(),"Bannière du terminal SSH absente")
            result["authenticated_terminal_utf8_ansi"]=True
            if os.environ.get("RUSTREMOTE_TUNNELS_ONLY")=="1":
                test_tunnels()
                result["success"]=True
                raise SystemExit(0)
            if os.environ.get("RUSTREMOTE_CHOOSERS_ONLY")=="1":
                click("SFTP");click("Connecter / Actualiser SFTP")
                wait(lambda:"Listing SFTP authentifié" in field("État SFTP réel"),"SFTP initial du sélecteur absent")
                test_choosers(config.parent/"server-data")
                result["success"]=True
                raise SystemExit(0)

            if os.environ.get("RUSTREMOTE_AUTOMATION_ONLY")=="1":
                import paramiko
                original=known.read_text();wrong=paramiko.ECDSAKey.generate()
                copy_reads=True
                click("Actions "+profile["name"]);click("Déconnecter")
            else:
                paste("echo RUSTREMOTE_UTF8\npwd\n", right_click=True)
                wait(lambda:find("Confirmer le collage multiligne"),"Confirmation multiligne absente")
                assert "RÉPONSE_DISTANTE" not in contents(),"Commandes envoyées sans consentement"
                click("Confirmer le collage dans cette session")
                wait(lambda:"RÉPONSE_DISTANTE — été" in contents() and "/laboratoire" in contents(),"Réponses aux commandes absentes")
                result["commands_and_multiline_consent"]=True
                for name in ("zoom-in","zoom-out","zoom-reset"):
                    subprocess.run(["gapplication","action","com.rustremote.Preview",name],check=True,timeout=5)
                wait(lambda:find("Zoom du terminal : 100 %"),"Zoom du terminal non réinitialisé")
                result["display_terminal_zoom"]=True
                reply=subprocess.check_output(["gdbus","call","--session","--dest","org.freedesktop.DBus","--object-path","/org/freedesktop/DBus","--method","org.freedesktop.DBus.GetConnectionUnixProcessID","com.rustremote.Preview"],text=True,timeout=5)
                pid=re.search(r"uint32 (\d+)",reply).group(1)
                # Redimensionnement X11 de cette seule fenêtre, sur l'affichage privé.
                import ctypes as c
                tree=subprocess.check_output(["xwininfo","-root","-tree"],text=True)
                window_id=None
                for line in tree.splitlines():
                    if not any(title in line for title in ['"RustRemote"','"RustRemote — Aperçu"']):continue
                    candidate=re.search(r"0x[0-9a-f]+",line).group()
                    prop=subprocess.check_output(["xprop","-id",candidate,"_NET_WM_PID"],text=True)
                    if re.search(r"= "+pid+r"\s*$",prop):window_id=int(candidate,16);break
                assert window_id is not None
                geometry=config.parent/"geometry.json"
                changes=json.loads(geometry.read_text())["changes"] if geometry.exists() else 0
                x=c.CDLL("libX11.so.6");x.XOpenDisplay.argtypes=[c.c_char_p];x.XOpenDisplay.restype=c.c_void_p;x.XResizeWindow.argtypes=[c.c_void_p,c.c_ulong,c.c_uint,c.c_uint];x.XSync.argtypes=[c.c_void_p,c.c_int];x.XCloseDisplay.argtypes=[c.c_void_p]
                xd=x.XOpenDisplay(os.environ["DISPLAY"].encode());assert xd
                x.XResizeWindow(xd,window_id,1500,900);x.XSync(xd,0);x.XCloseDisplay(xd)
                wait(lambda:geometry.exists() and json.loads(geometry.read_text())["changes"]>changes,"Redimensionnement PTY depuis GTK non reçu par le serveur")
                result["pty_resize_from_native_window"]=True
                subprocess.run([sys.executable,str(Path(__file__).with_name("capture-preview.py")),"--pid",pid,"--output","dist/ssh-integrated.png"],check=True,timeout=10)
                click("Tout sélectionner");context_menu();click("Copier")
                result["terminal_right_click_copy_paste"]=True
                assert find("Sélection copiée dans le presse-papiers"), "Copie non effectuée : "+str([n.get_name() for n in walk(app()) if "lection" in n.get_name()][:8])
                copied=subprocess.check_output(["xclip","-selection","clipboard","-out"],timeout=5).decode()
                assert "RÉPONSE_DISTANTE — été" in copied,repr(copied[:1000])
                result["clipboard_bidirectional"]=True
                click("Ouvrir le navigateur SFTP de l’onglet SSH")
                wait(lambda:find("SFTP — "+profile["name"]),"Navigateur SFTP absent")
                click("Connecter / Actualiser SFTP")
                wait(lambda:"Listing SFTP authentifié" in field("État SFTP réel"),"Connexion SFTP native échouée")
                remote("texte.txt")
                text("Chemin distant SFTP","Linux")
                click("Ouvrir la sélection SFTP")
                wait(lambda:find("Sélectionnez un fichier ou dossier distant"),"Ancienne sélection SFTP encore utilisable après changement de chemin")
                assert field("Éditeur SFTP réel")==""
                stale_download=config.parent/"must-not-download"
                text("Chemin local absolu pour transfert",str(stale_download))
                click("Télécharger la sélection SFTP")
                wait(lambda:find("Sélectionnez un ou plusieurs éléments distants"),"Ancienne sélection encore téléchargeable")
                assert not stale_download.exists()
                text("Chemin distant SFTP",".");click("Connecter / Actualiser SFTP")
                remote("texte.txt");click("Ouvrir la sélection SFTP")
                result["sftp_path_change_invalidates_selection"]=True
                wait(lambda:"Édition distante — test" in field("Éditeur SFTP réel"),"Lecture SFTP native échouée")
                data_root=config.parent/"server-data"
                text("Éditeur SFTP réel","MODIFIÉ DEPUIS RUSTREMOTE — été\n")
                click("Enregistrer le fichier distant")
                wait(lambda:(data_root/"texte.txt").read_text()=="MODIFIÉ DEPUIS RUSTREMOTE — été\n","Enregistrement SFTP natif échoué")
                text("Nom distant pour création ou envoi","ui-created")
                click("Créer le dossier distant")
                wait(lambda:(data_root/"ui-created").is_dir(),"Création du dossier SFTP échouée")
                source=config.parent/"gui-upload.txt";source.write_text("Fichier envoyé par le navigateur natif — été\n")
                text("Chemin local absolu pour transfert",str(source));text("Nom distant pour création ou envoi","ui-upload.txt")
                click("Envoyer le fichier ou dossier local")
                wait(lambda:(data_root/"ui-upload.txt").exists() and (data_root/"ui-upload.txt").read_bytes()==source.read_bytes(),"Envoi SFTP natif échoué")
                click("Connecter / Actualiser SFTP")
                wait(lambda:next((n for n in walk(app()) if n.get_name().startswith("ui-upload.txt ·")),None),"Listing SFTP non actualisé")
                remote("Linux")
                destination=config.parent/"gui-download"
                text("Chemin local absolu pour transfert",str(destination));click("Télécharger la sélection SFTP")
                wait(lambda:(destination/"dossier/enfant.txt").exists(),"Téléchargement récursif SFTP échoué")
                assert (destination/"dossier/enfant.txt").read_bytes()==(data_root/"Linux/dossier/enfant.txt").read_bytes()
                click("Télécharger la sélection SFTP")
                wait(lambda:"aucun écrasement" in field("État SFTP réel"),"Écrasement local non refusé")
                assert (destination/"dossier/enfant.txt").read_bytes()==(data_root/"Linux/dossier/enfant.txt").read_bytes()
                local_folder=config.parent/"gui-folder";(local_folder/"sub").mkdir(parents=True);(local_folder/"sub/note.txt").write_text("Dossier UTF-8 — été\n")
                text("Chemin local absolu pour transfert",str(local_folder));text("Nom distant pour création ou envoi","ui-folder")
                click("Envoyer le fichier ou dossier local")
                wait(lambda:(data_root/"ui-folder/sub/note.txt").exists(),"Envoi récursif SFTP échoué")
                assert (data_root/"ui-folder/sub/note.txt").read_bytes()==(local_folder/"sub/note.txt").read_bytes()
                result["sftp_native_editor_mkdir_bidirectional_transfers_no_clobber"]=True
                click("Connecter / Actualiser SFTP");remote("ui-upload.txt")
                click("Renommer la sélection SFTP");text("Nouveau nom SFTP","ui-renamed.txt");click("Appliquer la modification SFTP")
                wait(lambda:(data_root/"ui-renamed.txt").exists() and not (data_root/"ui-upload.txt").exists(),"Renommage SFTP échoué")
                click("Connecter / Actualiser SFTP");remote("ui-renamed.txt")
                click("Permissions de la sélection SFTP");text("Permissions octales SFTP","640");click("Appliquer la modification SFTP")
                wait(lambda:(data_root/"ui-renamed.txt").stat().st_mode&0o777==0o640,"Permissions SFTP échouées")
                click("Renommer la sélection SFTP");text("Nouveau nom SFTP","texte.txt");click("Appliquer la modification SFTP")
                wait(lambda:"aucun écrasement" in field("État SFTP réel"),"Collision renommage non refusée")
                click("Connecter / Actualiser SFTP");remote("ui-folder")
                click("Supprimer la sélection SFTP…");click("Annuler la suppression SFTP")
                assert (data_root/"ui-folder/sub/note.txt").exists()
                click("Supprimer la sélection SFTP…");click("Supprimer définitivement la sélection SFTP")
                wait(lambda:not (data_root/"ui-folder").exists(),"Suppression récursive SFTP échouée")
                result["sftp_rename_collision_permissions_delete_cancel_and_recursive"]=True
                click("Connecter / Actualiser SFTP");remote("ui-renamed.txt")
                node=next(n for n in walk(app()) if n.get_role()==Atspi.Role.LABEL and n.get_name().startswith("Linux ·"))
                while node.get_role()!=Atspi.Role.LIST_ITEM:node=node.get_parent()
                assert Atspi.Selection.select_child(node.get_parent(),node.get_index_in_parent())
                target=config.parent/"multi-download";target.mkdir()
                text("Chemin local absolu pour transfert",str(target));click("Télécharger la sélection SFTP")
                wait(lambda:(target/"Linux/dossier/enfant.txt").exists() and (target/"ui-renamed.txt").exists(),"Téléchargement multiple SFTP échoué")
                assert (target/"ui-renamed.txt").read_bytes()==(data_root/"ui-renamed.txt").read_bytes()
                assert (target/"Linux/dossier/enfant.txt").read_bytes()==(data_root/"Linux/dossier/enfant.txt").read_bytes()
                result["sftp_multiple_selection_files_and_folder"]=True
                test_choosers(data_root)

                # Reprise explicite dans les deux sens, préfixes différents refusés.
                remote("ui-renamed.txt")
                resumed=config.parent/"resumed.txt"
                original=(data_root/"ui-renamed.txt").read_bytes()
                resumed.write_bytes(original[:7])
                text("Chemin local absolu pour transfert",str(resumed))
                click("Reprendre le téléchargement SFTP…");click("Confirmer la reprise du fichier SFTP")
                wait(lambda:resumed.read_bytes()==original,"Reprise téléchargement échouée")
                upload_source=config.parent/"resume-upload.txt";upload_source.write_bytes(original+" — suite de test".encode())
                text("Chemin local absolu pour transfert",str(upload_source))
                click("Reprendre l’envoi du fichier SFTP…");click("Confirmer la reprise du fichier SFTP")
                wait(lambda:(data_root/"ui-renamed.txt").read_bytes()==upload_source.read_bytes(),"Reprise envoi échouée")
                resumed.write_bytes(b"PREFIXE DIFFERENT")
                text("Chemin local absolu pour transfert",str(resumed))
                click("Reprendre le téléchargement SFTP…");click("Confirmer la reprise du fichier SFTP")
                wait(lambda:any("Préfixe existant différent" in n.get_name() for n in walk(app())),"Préfixe incohérent non refusé")
                assert resumed.read_bytes()==b"PREFIXE DIFFERENT"
                result["sftp_resume_both_directions_prefix_mismatch_rejected"]=True
                (data_root/"progress.txt").write_bytes(b"RustRemote fixture\n"*50000)
                click("Connecter / Actualiser SFTP");remote("progress.txt")
                continued=config.parent/"progress-download.txt"
                text("Chemin local absolu pour transfert",str(continued));click("Télécharger la sélection SFTP")
                def progressed():
                    node=find("Progression SFTP")
                    description=node.get_description() if node else ""
                    return bool(re.match(r"[1-9][0-9]* octets",description))
                wait(progressed,"Progression réelle des octets SFTP absente")
                click("Déconnecter / Annuler SFTP")
                partial=continued.with_name("."+continued.name+".rustremote-part")
                wait(lambda:partial.exists(),"Copie partielle non conservée après annulation")
                assert not continued.exists() and 0<partial.stat().st_size<(data_root/"progress.txt").stat().st_size
                click("Reprendre le téléchargement SFTP…");click("Confirmer la reprise du fichier SFTP")
                wait(lambda:continued.exists(),"Publication atomique après reprise échouée")
                assert continued.read_bytes()==(data_root/"progress.txt").read_bytes() and not partial.exists()
                result["sftp_live_progress_cancel_preserve_partial_resume"]=True



                text("Éditeur SFTP réel","Tentative après modification concurrente\n")
                (data_root/"texte.txt").write_text("Modification concurrente à conserver\n")
                click("Enregistrer le fichier distant")
                wait(lambda:"Le fichier distant a changé" in field("État SFTP réel"),"Conflit d’édition SFTP non détecté")
                assert (data_root/"texte.txt").read_text()=="Modification concurrente à conserver\n"
                result["sftp_concurrent_edit_preserved"]=True
                remote("escape");click("Ouvrir la sélection SFTP")
                wait(lambda:"Liens et fichiers spéciaux refusés" in field("État SFTP réel"),"Lien SFTP non refusé")
                result["sftp_symlink_not_opened"]=True
                subprocess.run([sys.executable,str(Path(__file__).with_name("capture-preview.py")),"--pid",pid,"--title","SFTP — "+profile["name"],"--output","dist/sftp-integrated.png"],check=True,timeout=10)
                click("Fermer le navigateur SFTP")
                reply=subprocess.check_output(["gdbus","call","--session","--dest","org.freedesktop.DBus","--object-path","/org/freedesktop/DBus","--method","org.freedesktop.DBus.GetConnectionUnixProcessID","com.rustremote.Preview"],text=True,timeout=5)
                pid=re.search(r"uint32 (\d+)",reply).group(1)
                subprocess.run([sys.executable,str(Path(__file__).with_name("capture-preview.py")),"--pid",pid,"--output","dist/ssh-integrated.png"],check=True,timeout=10)
                paste_button=find("Coller dans le terminal…")
                sftp_button=find("Ouvrir SFTP")
                disconnect_button=find("Déconnecter le terminal")
                click("Actions "+profile["name"]);click("Déconnecter")
                def paste_disabled():
                    paste_button.clear_cache()
                    return not paste_button.get_state_set().contains(Atspi.StateType.SENSITIVE)
                wait(paste_disabled,"Collage encore actif après déconnexion")
                for control in (sftp_button,disconnect_button):
                    control.clear_cache()
                    assert not control.get_state_set().contains(Atspi.StateType.SENSITIVE)
                result["inactive_terminal_controls_disabled"]=True
                result["paste_disabled_after_disconnect"]=True
                wait(lambda:any("session terminée" in n.get_name() for n in walk(app())),"Session SSH non terminée après déconnexion")
                print("OK : terminal et SFTP clé, opérations, protections, déconnexion",flush=True)
                result["context_disconnect"]=True
                copy_reads=True
                # Refus réel d'une clé d'hôte changée depuis ce même profil dans l'UI.
                original=known.read_text()
                import paramiko
                wrong=paramiko.ECDSAKey.generate()
                known.write_text(f"[127.0.0.1]:{profile['port']} {wrong.get_name()} {wrong.get_base64()}\n")
                click("Actions "+profile["name"]);click("Se connecter");click("Autoriser et connecter")
                wait(lambda:"REMOTE HOST IDENTIFICATION HAS CHANGED" in contents(),"Changement d'hôte non signalé par le terminal")
                print("OK : clé d’hôte modifiée refusée dans le terminal",flush=True)
                result["changed_host_key_rejected_in_app"]=True
                known.write_text(original)
            agent_name="SSH Agent — laboratoire"
            click("Actions "+agent_name);click("Se connecter");click("Autoriser et connecter")
            wait(lambda:"SSH intégré — laboratoire été" in contents(agent_name),"Authentification SSH Agent intégrée échouée")
            click("SFTP");click("Connecter / Actualiser SFTP")
            wait(lambda:"Listing SFTP authentifié" in field("État SFTP réel"),"SFTP via SSH Agent échoué")
            click("Fermer le navigateur SFTP")
            # Alias et ProxyJump : terminal puis navigateur libssh2 via le bastion local.
            config_file=config.parent/"bastion.conf"
            config_text=(f'Host serveur-proxy-lab.invalid\n HostName 127.0.0.1\n ProxyJump bastion-lab\n'
                         f'Host bastion-lab\n HostName 127.0.0.1\n Port {profile["port"]}\n User lab\n'
                         f' IdentityFile "{profile["options"]["identity_file"]}"\n IdentitiesOnly yes\n UserKnownHostsFile "{known}"\n')
            config_file.write_text(config_text)
            proxy_name="SSH via bastion — laboratoire"
            click("SSH");text("Nom du profil",proxy_name);text("Hôte / adresse IP","serveur-proxy-lab.invalid")
            text("Port",str(profile["port"]));text("Utilisateur","lab");text("Clé SSH",profile["options"]["identity_file"])
            text("Fichier known_hosts SSH",str(known));text("Configuration OpenSSH",str(config_file));click("Enregistrer le profil")
            wait(lambda:find("Actions "+proxy_name),"Profil de bastion non enregistré")
            click("Actions "+proxy_name);click("Se connecter");click("Autoriser et connecter")
            wait(lambda:"SSH intégré — laboratoire été" in contents(proxy_name),"Terminal via bastion échoué")
            click("SFTP");click("Connecter / Actualiser SFTP")
            wait(lambda:"Listing SFTP authentifié" in field("État SFTP réel"),"SFTP via ProxyJump échoué")
            click("Fermer le navigateur SFTP")
            unknown=config.parent/"unknown-bastion-hosts";unknown.write_text("")
            config_file.write_text(config_text.replace(str(known),str(unknown)))
            click("SFTP");click("Connecter / Actualiser SFTP")
            wait(lambda:any("Connexion SSH SFTP refusée" in n.get_name() for n in walk(app())),"Empreinte de bastion inconnue non refusée")
            click("Fermer le navigateur SFTP");config_file.write_text(config_text)
            click("Actions "+proxy_name);click("Déconnecter")
            click("Actions "+agent_name);click("Se connecter")
            result["ssh_sftp_alias_proxyjump_and_unknown_bastion_rejected"]=True
            print("OK : authentification agent SSH et SFTP",flush=True)
            result["ssh_and_sftp_agent_in_application"]=True
            # Multi-Exec : deux sessions locales, prévisualisation et annulation sans envoi.
            click("Actions "+profile["name"]);click("Se connecter");click("Autoriser et connecter")
            wait(lambda:"SSH intégré — laboratoire été" in contents(),"Deuxième session pour Multi-Exec absente")
            click("Multi-Exec")
            text("Commande SSH à vérifier","echo RUSTREMOTE_UTF8")
            click(profile["name"]+" — lab@127.0.0.1:"+str(profile["port"]))
            click(agent_name+" — lab@127.0.0.1:"+str(profile["port"]))
            commands=config.parent/"shell-commands.count";before=int(commands.read_text())
            click("Vérifier les cibles et la commande");click("Annuler les commandes SSH")
            assert int(commands.read_text())==before
            click("Vérifier les cibles et la commande");click("Exécuter sur 2 session(s) SSH")
            wait(lambda:int(commands.read_text())==before+2,"Multi-Exec non reçu par les deux sessions")
            click("Fermer les outils de commandes")
            result["multi_exec_local_two_targets_preview_cancel_and_send"]=True
            click("Macros");click("Macro : système")
            assert field("Commande SSH à vérifier")=="uname -a"
            click("Fermer les outils de commandes")
            result["macro_catalogue_populates_command"]=True
            test_tunnels()
            click("Actions "+profile["name"]);click("Déconnecter")
            # Revenir à l’agent pour les contrôles stricts known_hosts ci-dessous.
            click("Actions "+agent_name);click("Se connecter")

            known.write_text(f"[127.0.0.1]:{profile['port']} {wrong.get_name()} {wrong.get_base64()}\n")
            click("SFTP");click("Connecter / Actualiser SFTP")
            wait(lambda:"SFTP refusé : clé d’hôte inconnue ou modifiée" in field("État SFTP réel"),"Le navigateur SFTP n’a pas refusé la mauvaise clé d’hôte")
            result["sftp_changed_host_key_rejected_in_app"]=True
            click("Fermer le navigateur SFTP");known.write_text(original)
            click("Actions "+agent_name);click("Déconnecter")
        if os.environ.get("RUSTREMOTE_TEST_PASSWORDS")=="1":
            name="SSH mot de passe — laboratoire"
            password=(config.parent/"password.secret").read_text()
            click("SSH");text("Nom du profil",name);text("Hôte / adresse IP","127.0.0.1");text("Port",str(profile["port"]));text("Utilisateur","lab")
            text("Nouveau dossier du profil","Authentification")
            text("Fichier known_hosts SSH",str(known));text("Configuration OpenSSH","/dev/null")
            click("Utiliser SSH Agent (sans transfert vers le serveur)")
            text("Mot de passe de la connexion",password)
            click("Enregistrer le profil")
            wait(lambda:find("Actions "+name),"Profil mot de passe non enregistré")
            assert password not in (config/"rustremote/profiles.json").read_text()
            accepted=config.parent/"password-accepted.count"
            count=int(accepted.read_text()) if accepted.exists() else 0
            click("Actions "+name);click("Se connecter");click("Autoriser et connecter")
            wait(lambda:accepted.exists() and int(accepted.read_text())>count,"Mot de passe SSH du profil non transmis via askpass")
            wait(lambda:"SSH intégré — laboratoire été" in contents(name),"Terminal par mot de passe absent")
            click("SFTP");click("Connecter / Actualiser SFTP")
            wait(lambda:"Listing SFTP authentifié" in field("État SFTP réel"),"SFTP par mot de passe échoué")
            assert int(accepted.read_text())>=count+2
            click("Fermer le navigateur SFTP");click("Actions "+name);click("Déconnecter")
            result["ssh_sftp_profile_password_temporary"]=True
            click("Actions "+name);click("Modifier")
            wait(lambda:find("Configuration de connexion"),"Éditeur du profil absent")
            click("Enregistrer le mot de passe avec cette connexion");click("Enregistrer le profil")
            wait(lambda:not find("Configuration de connexion"),"Enregistrement dans le coffre non terminé")
            saved=json.loads((config/"rustremote/profiles.json").read_text())
            item=next(p for p in saved["profiles"] if p["name"]==name)
            assert re.fullmatch(r"[a-f0-9-]{36}",item["credential_ref"])
            assert password not in json.dumps(saved)
            subprocess.run(["gapplication","action","com.rustremote.Preview","quit"],check=True,timeout=5);process.wait(timeout=5)
            process=subprocess.Popen(launch,env=os.environ)
            wait(lambda:find("Ouvrir les profils"),"Réouverture magasin absente");click("Déverrouiller / ouvrir")
            wait(lambda:find("Actions "+name),"Profil enregistré absent après redémarrage")
            count=int(accepted.read_text())
            click("Actions "+name);click("Se connecter");click("Autoriser et connecter")
            wait(lambda:int(accepted.read_text())>count,"Mot de passe non relu depuis le coffre après redémarrage")
            wait(lambda:"SSH intégré — laboratoire été" in contents(name),"Terminal enregistré absent")
            click("Actions "+name);click("Déconnecter")
            result["profile_password_keyring_restart_no_json_secret"]=True
            click("Coffre");wait(lambda:find("Coffre des identifiants"),"Coffre confondu avec les paramètres");click("Fermer le coffre")
            result["vault_separate_from_settings"]=True
            print("OK : mots de passe SSH/SFTP dans le profil, cache temporaire, coffre privé et redémarrage",flush=True)
        if os.environ.get("RUSTREMOTE_SSH_SFTP_ONLY") != "1":
            click("Actions "+serial_name);click("Se connecter");click("Autoriser et connecter")
            def serial_ready():
                attributes=termios.tcgetattr(serial_slave)
                return attributes[5]==termios.B115200 and not attributes[3]&termios.ICANON
            wait(serial_ready,"picocom n’a pas initialisé le pseudo-port série")
            wait(lambda:find("Série locale active · liaison non chiffrée"),"État série confondu avec la sécurité SSH")
            result["serial_security_status_correct"]=True
            paste("show version\nshow interfaces\n");click("Confirmer le collage dans cette session")
            wait(lambda:serial_commands==["show version","show interfaces"] and serial_replies==["version","interfaces"],"Échange bidirectionnel série intégré échoué")
            reply=subprocess.check_output(["gdbus","call","--session","--dest","org.freedesktop.DBus","--object-path","/org/freedesktop/DBus","--method","org.freedesktop.DBus.GetConnectionUnixProcessID","com.rustremote.Preview"],text=True,timeout=5)
            serial_pid=re.search(r"uint32 (\d+)",reply).group(1)
            time.sleep(.3)
            subprocess.run([sys.executable,str(Path(__file__).with_name("capture-preview.py")),"--pid",serial_pid,"--output","dist/serial-integrated.png"],check=True,timeout=10)
            result["serial_switch_in_native_terminal"]=True
            click("Actions "+serial_name);click("Déconnecter")
        else:
            result["serial_test"]="Non exécuté : parcours SSH/SFTP ciblé"
        result["success"]=True
    finally:
        if not result.get("success") and process.poll() is None:
            print("Dernière copie du terminal de laboratoire :", repr(getattr(contents,"last","")),flush=True)
            print("Échec terminal série (laboratoire uniquement) :", repr(contents(serial_name)), [(n.get_name(),str(n.get_role())) for n in walk(app()) if "Terminal" in n.get_name() or "session terminée" in n.get_name() or "Impossible" in n.get_name()], flush=True)
            reply=subprocess.check_output(["gdbus","call","--session","--dest","org.freedesktop.DBus","--object-path","/org/freedesktop/DBus","--method","org.freedesktop.DBus.GetConnectionUnixProcessID","com.rustremote.Preview"],text=True,timeout=5)
            app_pid=re.search(r"uint32 (\d+)",reply).group(1)
            subprocess.run([sys.executable,str(Path(__file__).with_name("capture-preview.py")),"--pid",app_pid,"--output","dist/native-ui-failure.png"],check=False,timeout=10)
        result["serial_received_bytes"]=len(serial_received)
        result["serial_commands"]=serial_commands
        result["serial_replies"]=serial_replies
        result["application_exit_before_cleanup"]=process.poll()
        if process.poll() is None:
            subprocess.run(["gapplication","action","com.rustremote.Preview","quit"],check=False,timeout=5)
            try:process.wait(timeout=5)
            except subprocess.TimeoutExpired:process.terminate();process.wait(timeout=5)
        if keyring and keyring.poll() is None:keyring.terminate();keyring.wait(timeout=5)
        serial_stop.set();serial_worker.join(timeout=2);os.close(serial_master);os.close(serial_slave)
        os.killpg(display.pid,signal.SIGTERM);display.wait(timeout=5);stream.close()
        if (runtime/"doc").is_mount():
            subprocess.run(["fusermount3","-u",str(runtime/"doc")],capture_output=True,timeout=5,check=True)
        Path("dist/ssh-ui-result.json").write_text(json.dumps(result,indent=2))
        print(json.dumps(result,ensure_ascii=False),flush=True)
