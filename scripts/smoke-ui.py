#!/usr/bin/env python3
"""Test GTK via AT-SPI, à lancer après ouverture du prototype (python3-gi requis)."""
import time
import subprocess
import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402


def descendants(node):
    yield node
    for index in range(node.get_child_count()):
        child = node.get_child_at_index(index)
        if child:
            yield from descendants(child)


def find(root, name):
    return next((n for n in descendants(root) if n.get_name() == name), None)


root = None
for _ in range(30):
    desktop = Atspi.get_desktop(0)
    for i in range(desktop.get_child_count()):
        app = desktop.get_child_at_index(i)
        if app and app.get_name().lower() == "rustremote":
            root = app
            break
    if root:
        break
    time.sleep(0.5)
assert root, "Application RustRemote introuvable"
button = find(root, "Nouvelle session")
assert button, "Bouton de création introuvable"
assert find(root, "SFTP — Démonstration"), "Panneau SFTP introuvable"
before = sum(n.get_role() == Atspi.Role.LABEL and n.get_name() == "SSH · Aperçu"
             for n in descendants(root))
subprocess.run(["gapplication", "action", "com.rustremote.Preview", "new-demo"], check=True)
time.sleep(0.5)
assert find(root, "SSH · Aperçu"), "Onglet non ouvert"
assert any(n.get_role() == Atspi.Role.TEXT
           and "Données de démonstration" in Atspi.Text.get_text(n, 0, -1)
           for n in descendants(root)), "Contenu du terminal absent"
# Le bouton de fermeture se trouve dans l'en-tête de l'onglet.
tab_labels = [n for n in descendants(root)
              if n.get_name() == "SSH · Aperçu" and n.get_role() == Atspi.Role.LABEL]
assert len(tab_labels) == before + 1, "Nombre d’onglets incorrect"
tab_label = tab_labels[-1]
container = tab_label.get_parent()
close = next(n for n in descendants(container) if n.get_role() == Atspi.Role.PUSH_BUTTON)
assert close.get_action_iface().do_action(0)
time.sleep(0.5)
assert sum(n.get_role() == Atspi.Role.LABEL and n.get_name() == "SSH · Aperçu"
           for n in descendants(root)) == before, "Onglet non fermé"
assert find(root, "Accueil"), "Onglet Accueil perdu"
print("OK : trois panneaux présents, ouverture et fermeture d’onglet, onglet Accueil conservé")
