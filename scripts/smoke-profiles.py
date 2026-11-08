#!/usr/bin/env python3
"""Parcours GTK isolé : aucune connexion réseau, uniquement des profils fictifs."""
import os
import json
import re
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import tomllib
import gi

# Le bus AT-SPI utilise XDG_RUNTIME_DIR : un D-Bus privé seul ne suffit pas.
if os.environ.get("RUSTREMOTE_PRIVATE_TEST_RUNTIME") != "1":
    with tempfile.TemporaryDirectory(prefix="rustremote-gui-runtime-") as private_runtime:
        test_env = dict(os.environ, XDG_RUNTIME_DIR=private_runtime, RUSTREMOTE_PRIVATE_TEST_RUNTIME="1", GIO_USE_VFS="local")
        wayland = os.environ.get("WAYLAND_DISPLAY", "wayland-0")
        if not os.path.isabs(wayland):
            test_env["WAYLAND_DISPLAY"] = str(Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")) / wayland)
        outcome = subprocess.run(["dbus-run-session", "--", sys.executable, str(Path(__file__).resolve()), *sys.argv[1:]], env=test_env)
        mounts = {line.split()[4] for line in Path("/proc/self/mountinfo").read_text().splitlines()}
        for name in ("doc", "gvfs"):
            mount = Path(private_runtime) / name
            if str(mount) in mounts:
                cleanup = subprocess.run(["fusermount3", "-uz", str(mount)], capture_output=True, timeout=5)
                remaining = {line.split()[4] for line in Path("/proc/self/mountinfo").read_text().splitlines()}
                if cleanup.returncode and str(mount) in remaining:
                    raise RuntimeError("Démontage du portail privé de test impossible : " + str(mount))
        raise SystemExit(outcome.returncode)

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi
Atspi.set_timeout(2000, 3000)


def walk(node):
    yield node
    for i in range(node.get_child_count()):
        child = node.get_child_at_index(i)
        if child:
            yield from walk(child)


def app():
    desktop = Atspi.get_desktop(0)
    for i in range(desktop.get_child_count()):
        item = desktop.get_child_at_index(i)
        if item and item.get_name().lower() == "rustremote":
            return item
    return None


def find(name):
    root = app()
    return next((n for n in walk(root) if n.get_name() == name), None) if root else None


def wait(name):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        node = find(name)
        if node:
            return node
        time.sleep(0.1)
    raise AssertionError("Élément absent : " + name)


def absent(name):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        if find(name) is None:
            return
        time.sleep(0.1)
    raise AssertionError("Fenêtre toujours ouverte : " + name)


def click(name):
    node = wait(name)
    actionable = next((n for n in walk(node)
                       if any(i.endswith("Action") for i in n.get_interfaces())
                       and any(Atspi.Action.get_action_name(n.get_action_iface(), i) in ("click", "activate", "press", "toggle") for i in range(n.get_action_iface().get_n_actions()))), None)
    assert actionable, "Élément sans action : " + name
    action_iface = actionable.get_action_iface()
    index = next(i for i in range(action_iface.get_n_actions()) if Atspi.Action.get_action_name(action_iface, i) in ("click", "activate", "press", "toggle"))
    assert action_iface.do_action(index), name
    time.sleep(0.2)


def editable(name):
    wait(name)
    return next(n for n in walk(app()) if n.get_name() == name
                and any(i.endswith("EditableText") for i in n.get_interfaces()))


def text(name, value):
    assert Atspi.EditableText.set_text_contents(editable(name), value), name


def action(name):
    subprocess.run(["gapplication", "action", "com.rustremote.Preview", name], check=True)
    time.sleep(0.3)


def tab(name):
    node = wait(name)
    if node.get_role() != Atspi.Role.PAGE_TAB:
        node = next(n for n in walk(app()) if n.get_name() == name and n.get_role() == Atspi.Role.PAGE_TAB)
    parent = node.get_parent()
    assert Atspi.Selection.select_child(parent, node.get_index_in_parent()), name
    time.sleep(0.3)


def select_file(name, occurrence=0):
    wait(name)
    labels = [n for n in walk(app()) if n.get_name() == name and n.get_role() == Atspi.Role.LABEL]
    row = labels[occurrence].get_parent()
    while row.get_role() != Atspi.Role.LIST_ITEM:
        row = row.get_parent()
    assert Atspi.Selection.select_child(row.get_parent(), row.get_index_in_parent())
    time.sleep(0.2)


def value(name):
    return Atspi.Text.get_text(editable(name), 0, -1)


def plaintext_startup(image):
    """Démarrage sans chiffrement ; réutilisable via --startup-only."""
    with tempfile.TemporaryDirectory(prefix="rustremote-plaintext-smoke-") as fixture:
        env = dict(os.environ, XDG_CONFIG_HOME=fixture, GDK_BACKEND=os.environ.get("RUSTREMOTE_TEST_BACKEND", "x11"))
        process = subprocess.Popen([str(image), "--offline"], env=env)
        try:
            wait("Protection des profils")
            click("Chiffrer les profils (recommandé)")
            click("Créer le magasin de profils")
            absent("Protection des profils")
            wait("Aucun profil enregistré")
            path = Path(fixture) / "rustremote/profiles.json"
            data = json.loads(path.read_text())
            assert data["version"] == 1 and data["profiles"] == []
            assert path.stat().st_mode & 0o777 == 0o600
            assert path.parent.stat().st_mode & 0o777 == 0o700
            assert not path.with_suffix(".age").exists()
            action("lock")
            wait("Ouvrir les profils")
            click("Déverrouiller / ouvrir")
            absent("Ouvrir les profils")
            wait("Aucun profil enregistré")
            if "--display-checks" in sys.argv:
                def fullscreen_state():
                    return subprocess.check_output(["gdbus","call","--session","--dest","com.rustremote.Preview","--object-path","/com/rustremote/Preview","--method","org.gtk.Actions.Describe","fullscreen"],text=True,timeout=5)
                for expected in ("<true>","<false>"):
                    action("fullscreen")
                    deadline=time.monotonic()+15
                    while expected not in fullscreen_state() and time.monotonic()<deadline:time.sleep(.15)
                    assert expected in fullscreen_state(),"Plein écran non confirmé par le compositeur"
                print("OK : plein écran confirmé par le compositeur puis restauration",flush=True)
            for entry in ("Configurer SSH…", "Configurer RDP…", "Configurer VNC…", "Configurer série / switch…"):
                click("Sessions"); click(entry)
                wait("Configuration de connexion"); click("Annuler")
            if "--icon-capture" in sys.argv and env["GDK_BACKEND"] == "x11":
                click("RDP")
                text("Nom du profil", "RDP — contrôle visuel")
                text("Hôte / adresse IP", "windows.example.invalid")
                text("Utilisateur", "demo")
                click("Enregistrer le profil")
                absent("Configuration de connexion")
                wait("• RDP — contrôle visuel")
                reply = subprocess.check_output(["gdbus", "call", "--session", "--dest", "org.freedesktop.DBus", "--object-path", "/org/freedesktop/DBus", "--method", "org.freedesktop.DBus.GetConnectionUnixProcessID", "com.rustremote.Preview"], text=True, timeout=5)
                pid = re.search(r"uint32 (\d+)", reply).group(1)
                subprocess.run([sys.executable, str(Path(__file__).with_name("capture-preview.py")), "--pid", pid, "--output", "dist/RustRemote-rdp-icon.png"], check=True, timeout=10)
            print("OK : création sans chiffrement, permissions privées et verrouillage/réouverture sans phrase secrète.", flush=True)
        finally:
            if process.poll() is None:
                subprocess.run(["gapplication", "action", "com.rustremote.Preview", "quit"], check=False, timeout=5)
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.terminate()
                    process.wait(timeout=5)


def settings_and_demo(image):
    forced_offline = "--forced-offline" in sys.argv
    with tempfile.TemporaryDirectory(prefix="rustremote-mode-settings-") as fixture:
        env=dict(os.environ,XDG_CONFIG_HOME=fixture,GDK_BACKEND=os.environ.get("RUSTREMOTE_TEST_BACKEND","x11"))
        process=subprocess.Popen([str(image),"--demo", *(["--offline"] if forced_offline else [])],env=env)
        def number(name,value=None):
            wait(name)
            node=next(n for n in walk(app()) if n.get_name()==name and any(i.endswith("Value") for i in n.get_interfaces()))
            if value is not None:assert Atspi.Value.set_current_value(node,float(value))
            return Atspi.Value.get_current_value(node)
        try:
            wait("★ Serveur IA — simulation")
            click("Quitter la démonstration");click("Annuler")
            assert not (Path(fixture)/"rustremote/profiles.json").exists()
            click("Quitter la démonstration");click("Ouvrir mes profils persistants")
            wait("Protection des profils")
            badge=find("MODE HORS LIGNE · connexions réseau et diagnostics réels interdits")
            assert badge is None or not badge.get_state_set().contains(Atspi.StateType.SHOWING)
            if forced_offline:
                wait("MODE HORS LIGNE · connexions réseau interdites")
            click("Chiffrer les profils (recommandé)");click("Créer le magasin de profils")
            absent("Protection des profils");wait("Aucun profil enregistré")
            for folder in ("Linux","Windows"):
                click("Nouveau dossier");text("Nom du dossier",folder);click("Créer le dossier");wait(folder)
            for button,name,host in (("Connexion Linux (SSH)","Linux personnel — test","linux.example.invalid"),("Connexion Windows (RDP)","Windows personnel — test","windows.example.invalid")):
                click(button);text("Nom du profil",name);text("Hôte / adresse IP",host);text("Utilisateur","demo");click("Enregistrer le profil");absent("Configuration de connexion");wait("• "+name)
            data=json.loads((Path(fixture)/"rustremote/profiles.json").read_text())
            assert set(data["folders"])=={"Linux","Windows"} and len(data["profiles"])==2
            assert {p["platform"] for p in data["profiles"]}=={"Linux","Windows"}
            assert all(p["folder"]==p["platform"] and not p["simulation"] for p in data["profiles"])
            click("Actions Linux personnel — test");click("Se connecter")
            if forced_offline:
                wait("Connexion bloquée hors ligne");click("Fermer")
            else:
                wait("Confirmer la connexion");click("Annuler la connexion")
            click("Paramètres");wait("Paramètres et sécurité")
            number("Taille de police du terminal",15);number("Lignes d’historique",2000)
            click("Fond noir du terminal");click("Activer l’outil Ping");click("Activer l’outil Traceroute");click("Enregistrer les paramètres");absent("Paramètres et sécurité")
            path=Path(fixture)/"rustremote/settings.json"
            saved=json.loads(path.read_text());assert saved["font_size"]==15 and saved["scrollback"]==2000 and saved["black_terminal"] and not saved["ping_enabled"] and not saved["traceroute_enabled"]
            assert path.stat().st_mode&0o777==0o600
            click("Outils");click("Ping / Traceroute…");click("Exécuter le diagnostic sur cette cible")
            assert "Outil désactivé" in Atspi.Text.get_text(wait("Résultat du diagnostic"),0,-1)
            click("Fermer le diagnostic")
            action("quit");process.wait(timeout=5)
            process=subprocess.Popen([str(image)],env=env)
            wait("Ouvrir les profils");click("Déverrouiller / ouvrir");wait("• Linux personnel — test");wait("• Windows personnel — test")
            click("Paramètres");assert number("Taille de police du terminal")==15 and number("Lignes d’historique")==2000
            assert wait("Fond noir du terminal").get_state_set().contains(Atspi.StateType.PRESSED)
            click("Fermer")
            print("OK : sortie de démonstration, annulation, profils persistants Linux/Windows et dossiers, confirmation normale sans réseau, paramètres persistants et outils désactivables.",flush=True)
        finally:
            if process.poll() is None:
                subprocess.run(["gapplication","action","com.rustremote.Preview","quit"],check=False,timeout=5)
                try:process.wait(timeout=5)
                except subprocess.TimeoutExpired:process.terminate();process.wait(timeout=5)


def responsive_rdp(image):
    import ctypes as c
    with tempfile.TemporaryDirectory(prefix="rustremote-responsive-") as fixture:
        env=dict(os.environ,XDG_CONFIG_HOME=fixture,GDK_BACKEND="x11")
        process=subprocess.Popen([str(image),"--demo"],env=env)
        try:
            wait("★ Serveur IA — simulation")
            click("RDP");text("Nom du profil","Windows — responsive");text("Hôte / adresse IP","windows.example.invalid");text("Utilisateur","demo")
            click("Enregistrer le profil");absent("Configuration de connexion");click("• Windows — responsive")
            frame=wait("Configuration de connexion")
            reply=subprocess.check_output(["gdbus","call","--session","--dest","org.freedesktop.DBus","--object-path","/org/freedesktop/DBus","--method","org.freedesktop.DBus.GetConnectionUnixProcessID","com.rustremote.Preview"],text=True,timeout=5)
            pid=re.search(r"uint32 (\d+)",reply).group(1)
            tree=subprocess.check_output(["xwininfo","-root","-tree"],text=True)
            window_id=None
            for line in tree.splitlines():
                if '"Configuration de connexion"' not in line:continue
                candidate=re.search(r"0x[0-9a-f]+",line).group()
                prop=subprocess.check_output(["xprop","-id",candidate,"_NET_WM_PID"],text=True)
                if re.search(r"= "+pid+r"\s*$",prop):window_id=int(candidate,16);break
            assert window_id is not None
            x=c.CDLL("libX11.so.6");x.XOpenDisplay.argtypes=[c.c_char_p];x.XOpenDisplay.restype=c.c_void_p;x.XResizeWindow.argtypes=[c.c_void_p,c.c_ulong,c.c_uint,c.c_uint];x.XSync.argtypes=[c.c_void_p,c.c_int];x.XCloseDisplay.argtypes=[c.c_void_p]
            xd=x.XOpenDisplay(os.environ["DISPLAY"].encode());assert xd
            measurements=[]
            try:
                for width,height in ((600,480),(480,520),(800,680)):
                    x.XResizeWindow(xd,window_id,width,height);x.XSync(xd,0);time.sleep(.6)
                    frame.clear_cache()
                    rect=frame.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
                    assert rect.width<=width+64 and rect.height<=height+64,(width,height,rect.width,rect.height)
                    for name in ("Enregistrer le profil","Annuler","Se connecter…"):
                        button=wait(name);button.clear_cache();r=button.get_component_iface().get_extents(Atspi.CoordType.SCREEN)
                        assert r.width>0 and r.height>0 and button.get_state_set().contains(Atspi.StateType.SHOWING) and rect.x<=r.x and rect.y<=r.y and r.x+r.width<=rect.x+rect.width and r.y+r.height<=rect.y+rect.height,(name,width,height,r.x,r.y,r.width,r.height)
                    measurements.append({"requested":[width,height],"actual":[rect.width,rect.height],"footer_inside_window":True})
                    subprocess.run([sys.executable,str(Path(__file__).with_name("capture-preview.py")),"--pid",pid,"--title","Configuration de connexion","--output",f"dist/rdp-responsive-{width}x{height}.png"],check=True,timeout=10)
                subprocess.run([sys.executable,str(Path(__file__).with_name("capture-preview.py")),"--pid",pid,"--title","Configuration de connexion","--output","dist/rdp-responsive.png"],check=True,timeout=10)
            finally:x.XCloseDisplay(xd)
            click("Annuler")
            Path("dist/rdp-responsive-result.json").write_text(json.dumps({"sizes":measurements,"success":True},indent=2))
            print("OK : fenêtre RDP redimensionnée, pied de fenêtre et trois boutons visibles à toutes les tailles testées.",flush=True)
        finally:
            if process.poll() is None:
                subprocess.run(["gapplication","action","com.rustremote.Preview","quit"],check=False,timeout=5)
                try:process.wait(timeout=5)
                except subprocess.TimeoutExpired:process.terminate();process.wait(timeout=5)


def rdp_credentials(image):
    fixture=Path(os.environ["RUSTREMOTE_RDP_FIXTURE"])
    assert fixture.is_absolute() and fixture.is_dir()
    data=json.loads((fixture/"rustremote/profiles.json").read_text())
    profile=data["profiles"][0]
    assert profile["host"]=="127.0.0.1" and profile["options"]["domain"]=="RUSTREMOTE"
    password=(fixture/"password.secret").read_text()
    env=dict(os.environ,XDG_CONFIG_HOME=str(fixture),GDK_BACKEND="x11",GSK_RENDERER="cairo")
    process=subprocess.Popen([str(image)],env=env)
    try:
        wait("Ouvrir les profils");click("Déverrouiller / ouvrir")
        click("Actions "+profile["name"]);click("Modifier")
        wait("Configuration de connexion")
        assert value("Domaine AD / realm")=="RUSTREMOTE"
        text("Mot de passe de la connexion",password)
        click("Enregistrer le profil");absent("Configuration de connexion")
        assert password not in (fixture/"rustremote/profiles.json").read_text()
        click("Actions "+profile["name"]);click("Se connecter");click("Autoriser et connecter")
        deadline=time.monotonic()+25
        while time.monotonic()<deadline:
            tree=subprocess.check_output(["xwininfo","-root","-tree"],text=True,timeout=5)
            if '127.0.0.1' in tree:break
            time.sleep(.2)
        else:
            root=app()
            messages=[n.get_name() for n in walk(root) if n.get_role()==Atspi.Role.LABEL and any(word in n.get_name() for word in ("RDP", "passe", "Authentification", "refus", "impossible", "certificat"))]
            raise AssertionError("Client RDP sans bureau authentifié : "+repr(messages))
        time.sleep(1)
        from PIL import ImageGrab
        im=ImageGrab.grab(xdisplay=os.environ["DISPLAY"])
        assert sum(abs(r-36)<12 and abs(g-67)<12 and abs(b-108)<12 for r,g,b in im.convert("RGB").getdata())>1000
        click("Actions "+profile["name"]);click("Déconnecter")
        deadline=time.monotonic()+10
        while time.monotonic()<deadline:
            tree=subprocess.check_output(["xwininfo","-root","-tree"],text=True,timeout=5)
            if '127.0.0.1' not in tree:break
            time.sleep(.2)
        else:raise AssertionError("Déconnexion du client RDP non effectuée")
        click("Actions "+profile["name"]);click("Se connecter");click("Autoriser et connecter")
        deadline=time.monotonic()+25
        while time.monotonic()<deadline:
            if '127.0.0.1' in subprocess.check_output(["xwininfo","-root","-tree"],text=True,timeout=5):break
            time.sleep(.2)
        else:raise AssertionError("Reconnexion RDP échouée")
        action("quit");process.wait(timeout=5)
        deadline=time.monotonic()+10
        while time.monotonic()<deadline:
            if '127.0.0.1' not in subprocess.check_output(["xwininfo","-root","-tree"],text=True,timeout=5):break
            time.sleep(.2)
        else:raise AssertionError("Client RDP laissé ouvert après fermeture de RustRemote")
        print("OK : RDP depuis le profil, domaine, mot de passe sur pipe, certificat épinglé, pixels, déconnexion et fermeture de RustRemote",flush=True)
    finally:
        if process.poll() is None:
            subprocess.run(["gapplication","action","com.rustremote.Preview","quit"],check=False,timeout=5)
            try:process.wait(timeout=5)
            except subprocess.TimeoutExpired:process.terminate();process.wait(timeout=5)

assert app() is None, "Fermez d’abord RustRemote pour éviter de tester vos vrais profils."
image = Path(sys.argv[1] if len(sys.argv) > 1 else f"dist/RustRemote-{tomllib.loads(Path("Cargo.toml").read_text())["package"]["version"]}-preview-x86_64.AppImage").resolve()
if "--rdp-credentials-only" in sys.argv:
    rdp_credentials(image)
    raise SystemExit(0)
if "--responsive-only" in sys.argv:
    responsive_rdp(image)
    raise SystemExit(0)
if "--startup-only" in sys.argv:
    plaintext_startup(image)
    raise SystemExit(0)
if "--settings-demo-only" in sys.argv:
    settings_and_demo(image)
    raise SystemExit(0)
with tempfile.TemporaryDirectory(prefix="rustremote-smoke-") as fixture:
    env = dict(os.environ, XDG_CONFIG_HOME=fixture, GDK_BACKEND=os.environ.get("RUSTREMOTE_TEST_BACKEND", "x11"))
    process = subprocess.Popen([str(image), "--offline"], env=env)
    try:
        wait("MODE HORS LIGNE · connexions réseau et diagnostics réels interdits")
        second = subprocess.run([str(image), "--offline"], env=env, capture_output=True, text=True)
        assert second.returncode != 0 and "instance est déjà ouverte" in second.stderr
        wait("Protection des profils")
        # Phrase de fixture publique, jamais un secret utilisateur.
        for name in ["Phrase secrète des profils", "Confirmation de la phrase secrète"]:
            text(name, "public-test-fixture-123")
        click("Créer le magasin de profils")
        wait("Aucun profil enregistré")
        assert (Path(fixture) / "rustremote/profiles.age").is_file()
        click("SSH")
        click("Enregistrer le profil")
        wait("Identifiant SSH requis")
        text("Nom du profil", "Fixture SSH")
        text("Hôte / adresse IP", "example.invalid")
        text("Utilisateur", "demo")
        click("Enregistrer le profil")
        wait("• Fixture SSH")
        key = Path(fixture) / "test key"
        subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(key)], check=True)
        click("• Fixture SSH")
        text("Clé SSH", str(Path(fixture) / "absent"))
        click("Enregistrer le profil")
        wait("Le fichier de clé SSH n’existe pas.")
        text("Clé SSH", str(key))
        click("Utiliser SSH Agent (sans transfert vers le serveur)")
        click("Enregistrer le profil")
        absent("Configuration de connexion")
        click("• Fixture SSH")
        assert value("Clé SSH") == str(key)
        click("Annuler")
        click("RDP")
        text("Nom du profil", "Fixture RDP")
        text("Hôte / adresse IP", "windows.example.invalid")
        text("Utilisateur", "demo")
        text("Domaine AD / realm", "LAB")
        tab("Affichage RDP")
        click("Multi-écrans (plein écran)")
        text("Écrans (vide = tous)", "0,1")
        tab("Périphériques RDP")
        click("Partager le microphone local (opt-in)")
        click("Rediriger les imprimantes locales (opt-in)")
        tab("Sécurité RDP")
        click("Enregistrer le profil")
        wait("• Fixture RDP")
        # Vérifier le verrouillage et la relecture chiffrée, sans connexion.
        action("lock")
        wait("Ouvrir les profils")
        text("Phrase secrète des profils", "wrong-public-fixture")
        click("Déverrouiller / ouvrir")
        wait("Phrase secrète incorrecte ou fichier endommagé")
        text("Phrase secrète des profils", "public-test-fixture-123")
        click("Déverrouiller / ouvrir")
        wait("• Fixture SSH")
        wait("• Fixture RDP")
        action("profiles")
        wait("Gestionnaire de profils")
        click("Fixture RDP (Rdp) — windows.example.invalid:3389")
        assert Atspi.Text.get_text(editable("Port"), 0, -1) == "3389"
        assert Atspi.Text.get_text(editable("Domaine AD / realm"), 0, -1) == "LAB"
        tab("Affichage RDP")
        assert value("Écrans (vide = tous)") == "0,1"
        tab("Périphériques RDP")
        for control in ("Partager le microphone local (opt-in)", "Rediriger les imprimantes locales (opt-in)"):
            assert wait(control).get_state_set().contains(Atspi.StateType.PRESSED)
        click("Se connecter…")
        wait("Connexion bloquée hors ligne")
        click("Fermer")
        registered = subprocess.check_output(["gdbus", "call", "--session", "--dest", "com.rustremote.Preview", "--object-path", "/com/rustremote/Preview", "--method", "org.gtk.Actions.DescribeAll"], text=True)
        for expected in ["new-profile", "new-ssh", "new-rdp", "new-vnc", "new-serial", "new-folder", "import", "export", "simulation", "sftp", "diagnostics", "exit-demo", "new-demo", "profiles", "settings", "tools", "about", "lock", "quit"]:
            assert "'" + expected + "':" in registered, expected
        action("settings")
        wait("Paramètres et sécurité")
        click("Fermer")
        action("tools")
        wait("Outils")
        click("Fermer")
        action("about")
        wait("À propos de RustRemote")
        click("Fermer")
        click("Fichier")
        click("Nouveau profil…")
        wait("Configuration de connexion")
        click("Annuler")
        for entry in ("Configurer SSH…", "Configurer RDP…", "Configurer VNC…", "Configurer série / switch…"):
            click("Sessions"); click(entry)
            wait("Configuration de connexion"); click("Annuler")
        click("Aide"); click("À propos…")
        wait("À propos de RustRemote"); click("Fermer")
        print("Étape : dossiers et import", flush=True)
        click("VNC")
        text("Nom du profil", "Fixture VNC")
        text("Hôte / adresse IP", "127.0.0.1")
        text("Passerelle SSH", "gateway.example.invalid")
        text("Utilisateur de la passerelle SSH", "lab")
        text("Port de la passerelle SSH", "2222")
        click("Simulation locale (aucun réseau)")
        click("Enregistrer le profil")
        wait("• Fixture VNC")
        click("• Fixture VNC")
        assert value("Port de la passerelle SSH") == "2222"
        click("Annuler")
        click("Actions Fixture VNC"); click("Se connecter")
        wait("Simulation — Fixture VNC"); click("Déconnecter la simulation")
        click("Série")
        text("Nom du profil", "Switch Lab")
        text("Débit série", "9600")
        click("Simulation locale (aucun réseau)")
        click("Enregistrer le profil")
        wait("• Switch Lab")
        click("• Switch Lab")
        assert value("Débit série") == "9600"
        click("Annuler")
        click("Actions Switch Lab"); click("Se connecter")
        wait("Simulation — Switch Lab")
        for probe, expected in (("show version", "Switch Simulator"), ("show interfaces", "Gi0/1 up"), ("reload", "lecture seule")):
            text("Commande du switch simulé", probe)
            click("Envoyer au switch simulé")
            assert expected in Atspi.Text.get_text(wait("Console du switch simulé"), 0, -1)
        click("Déconnecter la simulation")
        click("Fichier")
        click("Nouveau dossier…")
        text("Nom du dossier", "Linux")
        click("Créer le dossier")
        wait("Linux")
        click("Fichier")
        click("Importer des profils…")
        text("Fichier à importer", str(Path("assets/simulations/serveur-ia.json").resolve()))
        click("Vérifier le fichier")
        wait("1 profil(s), 1 dossier(s) validés : Serveur IA — simulation")
        click("Confirmer l’import")
        wait("★ Serveur IA — simulation")
        action("import")
        click("Charger l’exemple Serveur IA (simulation)")
        click("Confirmer l’import")
        wait("Nom de profil dupliqué.")
        click("Fermer l’import")
        action("export")
        export = Path(fixture) / "export.json"
        text("Fichier d’export", str(export))
        click("J’accepte l’export de métadonnées non chiffrées")
        click("Créer l’export JSON")
        absent("Exporter les profils RustRemote")
        exported = export.read_bytes()
        payload = json.loads(exported)
        assert "Linux" in payload["folders"] and any(p["simulation"] for p in payload["profiles"])
        assert b"PRIVATE KEY" not in exported and b'"password"' not in exported
        action("export")
        text("Fichier d’export", str(export))
        click("J’accepte l’export de métadonnées non chiffrées")
        click("Créer l’export JSON")
        wait("Destination existante ou export impossible : aucun remplacement effectué.")
        assert export.read_bytes() == exported
        click("Annuler l’export")
        action("lock")
        text("Phrase secrète des profils", "public-test-fixture-123")
        click("Déverrouiller / ouvrir")
        wait("Linux")
        wait("★ Serveur IA — simulation")
        click("Actions Serveur IA — simulation")
        click("Modifier")
        assert value("Hôte / adresse IP") == "ia.aurelson.com"
        click("Annuler")
        click("Actions Serveur IA — simulation")
        click("Se connecter")
        wait("Simulation — Serveur IA — simulation")
        print("Étape : fichiers/dossiers et presse-papiers simulés", flush=True)
        click("Créer dossier local…")
        text("Nom du dossier simulé", "Dossier utilisateur")
        click("Créer dans le laboratoire")
        wait("Dossier utilisateur")
        select_file("note.txt")
        click("Envoyer vers le serveur simulé →")
        wait("Transfert simulé terminé · fichiers/dossiers copiés dans le laboratoire")
        select_file("note.txt", 1)
        click("Ouvrir la sélection distante")
        text("Éditeur SFTP simulé", "Modifié sur le serveur simulé — été\n")
        click("Enregistrer le fichier simulé")
        text("Nom à destination", "note-retour.txt")
        click("← Télécharger vers le poste local")
        wait("note-retour.txt")
        select_file("note-retour.txt")
        click("Ouvrir la sélection locale")
        assert value("Éditeur SFTP simulé") == "Modifié sur le serveur simulé — été\n"
        text("Nom à destination", "")
        select_file("dossier-test")
        click("Envoyer vers le serveur simulé →")
        select_file("dossier-test", 1)
        text("Nom à destination", "retour-dossier")
        click("← Télécharger vers le poste local")
        wait("retour-dossier")
        tab("Presse-papiers simulé")
        text("Texte local", "Bonjour IA — été\nLigne 2")
        click("Copier local")
        click("Coller distant")
        assert value("Texte distant simulé") == "Bonjour IA — été\nLigne 2"
        text("Texte distant simulé", "Retour IA — simulation locale")
        click("Copier distant")
        click("Coller local")
        assert value("Texte local") == "Retour IA — simulation locale"
        click("Actions Serveur IA — simulation")
        click("Déconnecter")
        assert find("Simulation — Serveur IA — simulation") is None
        print("Étape : RDP simulé et diagnostics", flush=True)
        click("• Fixture RDP")
        click("Simulation locale (aucun réseau)")
        state = wait("Simulation locale (aucun réseau)").get_state_set()
        assert state.contains(Atspi.StateType.PRESSED) or state.contains(Atspi.StateType.CHECKED)
        click("Enregistrer le profil")
        absent("Configuration de connexion")
        click("Actions Fixture RDP")
        click("Se connecter")
        wait("Simulation — Fixture RDP")
        tab("Presse-papiers simulé")
        assert not wait("Copier local").get_state_set().contains(Atspi.StateType.SENSITIVE)
        click("Autoriser le presse-papiers de la simulation")
        text("Texte local", "Test RDP simulé")
        click("Copier local")
        click("Coller distant")
        assert value("Texte distant simulé") == "Test RDP simulé"
        click("Déconnecter la simulation")
        click("Outils"); click("Ping / Traceroute…")
        click("Exécuter le diagnostic sur cette cible")
        assert "Aucun paquet réseau envoyé" in Atspi.Text.get_text(wait("Résultat du diagnostic"), 0, -1)
        click("Simulation locale — aucun paquet réseau")
        click("Exécuter le diagnostic sur cette cible")
        assert "bloqué par --offline" in Atspi.Text.get_text(wait("Résultat du diagnostic"), 0, -1)
        click("Fermer le diagnostic")
        click("Affichage"); click("Ouvrir un onglet de démonstration")
        wait("SSH · Aperçu")
        # Échantillonner uniquement le processus de test, pas les autres applications.
        bus_pid = subprocess.check_output(["gdbus", "call", "--session", "--dest", "org.freedesktop.DBus", "--object-path", "/org/freedesktop/DBus", "--method", "org.freedesktop.DBus.GetConnectionUnixProcessID", "com.rustremote.Preview"], text=True)
        pid = int(re.search(r"uint32 (\d+)", bus_pid).group(1))
        def rss():
            return int(re.search(r"VmRSS:\s+(\d+)", Path(f"/proc/{pid}/status").read_text()).group(1)) / 1024
        def ticks():
            fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
            return int(fields[11]) + int(fields[12])
        action("settings"); click("Fermer")
        initial_rss = rss()
        def threads():
            return len(list(Path(f"/proc/{pid}/task").iterdir()))
        def fds():
            return len(list(Path(f"/proc/{pid}/fd").iterdir()))
        initial_threads, initial_fds = threads(), fds()
        for _ in range(10):
            action("settings"); click("Fermer")
        for _ in range(3):
            action("simulation"); click("Déconnecter la simulation")
        time.sleep(0.5)
        before = ticks(); start = time.monotonic(); time.sleep(1)
        metrics = {"backend": env["GDK_BACKEND"], "rss_mib": round(rss(), 2), "rss_delta_after_10_dialogs_3_labs_mib": round(rss()-initial_rss, 2), "idle_cpu_percent_one_second": round((ticks()-before)/os.sysconf("SC_CLK_TCK")/(time.monotonic()-start)*100, 2), "open_fds": fds(), "fd_delta": fds()-initial_fds, "threads": threads(), "thread_delta": threads()-initial_threads}
        assert metrics["thread_delta"] <= 2 and metrics["fd_delta"] <= 3, metrics
        Path(f"dist/runtime-{env['GDK_BACKEND']}.json").write_text(json.dumps(metrics, indent=2))
        print("Mesures :", json.dumps(metrics), flush=True)
        ciphertext = (Path(fixture) / "rustremote/profiles.age").read_bytes()
        assert b"example.invalid" not in ciphertext
        assert not (Path(fixture) / "rustremote/profiles.json").exists()
        if "--capture" in sys.argv and env["GDK_BACKEND"] == "x11":
            subprocess.run([sys.executable, str(Path(__file__).with_name("capture-preview.py")), "--pid", str(pid)], check=True)
        tab_label = next(n for n in walk(app()) if n.get_name() == "SSH · Aperçu" and n.get_role() == Atspi.Role.LABEL)
        close = next(n for n in walk(tab_label.get_parent()) if n.get_role() == Atspi.Role.PUSH_BUTTON)
        assert close.get_action_iface().do_action(0)
        time.sleep(0.3)
        assert find("SSH · Aperçu") is None
        print("OK : profils/chiffrement, dossier Linux, import IA, anti-doublon, menus/contextes, édition/transferts fichiers et dossiers bidirectionnels, presse-papiers SSH/RDP simulé et diagnostics ; aucun serveur contacté.")
    finally:
        if process.poll() is None:
            subprocess.run(["gapplication", "action", "com.rustremote.Preview", "quit"], check=False)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=5)

plaintext_startup(image)
