"""Capturer uniquement la fenêtre X11 du processus de test indiqué."""
import argparse
import ctypes as c
import re
import subprocess
from PIL import Image

parser = argparse.ArgumentParser()
parser.add_argument("--pid", required=True, type=int)
parser.add_argument("--output", default="dist/RustRemote-preview.png")
parser.add_argument("--title", help="Titre exact de la fenêtre de test (PID toujours obligatoire)")
args = parser.parse_args()
listing = subprocess.check_output(["xwininfo", "-root", "-tree"], text=True)
window = None
for line in listing.splitlines():
    if args.title and ('"'+args.title+'"') not in line:
        continue
    if not args.title and (not any(title in line for title in ['"RustRemote"', '"RustRemote — Démonstration"', '"RustRemote — Aperçu"']) or '"rustremote"' not in line):
        continue
    match = re.search(r"0x[0-9a-f]+", line)
    if not match:
        continue
    candidate = match.group()
    prop = subprocess.check_output(["xprop", "-id", candidate, "_NET_WM_PID"], text=True)
    if re.search(r"= " + str(args.pid) + r"\s*$", prop):
        window = candidate
        break
assert window, "Fenêtre du processus de test introuvable ; aucune autre fenêtre capturée"
s = subprocess.check_output(["xwininfo", "-id", window], text=True)
w = int(re.search(r"Width: (\d+)", s).group(1))
h = int(re.search(r"Height: (\d+)", s).group(1))

class XImage(c.Structure):
    _fields_ = [("width", c.c_int), ("height", c.c_int), ("xoffset", c.c_int), ("format", c.c_int), ("data", c.c_void_p), ("byte_order", c.c_int), ("bitmap_unit", c.c_int), ("bitmap_bit_order", c.c_int), ("bitmap_pad", c.c_int), ("depth", c.c_int), ("bytes_per_line", c.c_int), ("bits_per_pixel", c.c_int)]

x = c.CDLL("libX11.so.6")
x.XOpenDisplay.argtypes = [c.c_char_p]
x.XOpenDisplay.restype = c.c_void_p
x.XGetImage.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_int, c.c_uint, c.c_uint, c.c_ulong, c.c_int]
x.XGetImage.restype = c.POINTER(XImage)
x.XDestroyImage.argtypes = [c.POINTER(XImage)]
x.XCloseDisplay.argtypes = [c.c_void_p]
d = x.XOpenDisplay(None)
assert d
p = x.XGetImage(d, int(window, 16), 0, 0, w, h, c.c_ulong(-1).value, 2)
assert p
i = p.contents
assert i.bits_per_pixel == 32 and i.byte_order == 0
Image.frombytes("RGB", (w,h), c.string_at(i.data, i.bytes_per_line*h), "raw", "BGRX", i.bytes_per_line, 1).save(args.output)
x.XDestroyImage(p)
x.XCloseDisplay(d)
