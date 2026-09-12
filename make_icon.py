# -*- coding: utf-8 -*-
"""
Erzeugt das App-Icon für VerseKit (Xharig-Stil: dunkel + Xharig-Grün).
Motiv: Scope/Watcher-Ring mit grünem „neu"-Punkt.

Ausgabe:
  icon.ico        — Multi-Size-Icon für die EXE (16…256)
  assets/icon.png — 256er-Vorschau (für README)

Aufruf:  python make_icon.py   (braucht Pillow; nur zum Icon-Bauen)
"""
import os
from PIL import Image, ImageDraw, ImageFilter

S = 4                      # Supersampling
N = 256 * S                # Arbeitsauflösung
BG1   = (16, 20, 28, 255)  # #10141c
BG2   = (27, 34, 48, 255)  # #1b2230
GREEN = (156, 228, 48)     # #9ce430  (Xharig-Grün, Neon für dunklen Grund)
BRIGHT = (104, 214, 96)
DIM   = (54, 110, 52)

img  = Image.new('RGBA', (N, N), (0, 0, 0, 0))
draw = ImageDraw.Draw(img)

# --- Hintergrund: abgerundetes Quadrat mit sanftem Vertikal-Verlauf ---
radius = int(N * 0.22)
grad = Image.new('RGBA', (N, N))
gp = grad.load()
for y in range(N):
    t = y / (N - 1)
    r = int(BG1[0] + (BG2[0] - BG1[0]) * t)
    g = int(BG1[1] + (BG2[1] - BG1[1]) * t)
    b = int(BG1[2] + (BG2[2] - BG1[2]) * t)
    for x in range(N):
        gp[x, y] = (r, g, b, 255)
mask = Image.new('L', (N, N), 0)
ImageDraw.Draw(mask).rounded_rectangle([0, 0, N - 1, N - 1], radius=radius, fill=255)
img.paste(grad, (0, 0), mask)
draw = ImageDraw.Draw(img)

# dünner grüner Rahmen innen
draw.rounded_rectangle([int(N*0.045)]*2 + [N - int(N*0.045)]*2,
                       radius=int(radius*0.85), outline=GREEN + (60,), width=max(2, S))

cx = cy = N // 2

# --- Scope-Ring ---
R = int(N * 0.30)
ring_w = int(N * 0.052)
# äußerer dunkler Schatten-Ring für Tiefe
draw.ellipse([cx - R - ring_w, cy - R - ring_w, cx + R + ring_w, cy + R + ring_w],
             outline=DIM + (255,), width=max(1, S))
draw.ellipse([cx - R, cy - R, cx + R, cy + R], outline=GREEN + (255,), width=ring_w)

# --- Fadenkreuz-Ticks (N/O/S/W) ---
tick_in, tick_out = int(R * 0.78), int(R * 1.28)
tw = int(N * 0.030)
for dx, dy in ((0, -1), (0, 1), (-1, 0), (1, 0)):
    draw.line([cx + dx * tick_in, cy + dy * tick_in,
               cx + dx * tick_out, cy + dy * tick_out], fill=GREEN, width=tw)

# --- Glow + grüner „neu"-Punkt in der Mitte ---
glow = Image.new('RGBA', (N, N), (0, 0, 0, 0))
gd = ImageDraw.Draw(glow)
gr = int(N * 0.165)
gd.ellipse([cx - gr, cy - gr, cx + gr, cy + gr], fill=GREEN + (170,))
glow = glow.filter(ImageFilter.GaussianBlur(int(N * 0.045)))
img = Image.alpha_composite(img, glow)
draw = ImageDraw.Draw(img)
dot = int(N * 0.105)
draw.ellipse([cx - dot, cy - dot, cx + dot, cy + dot], fill=BRIGHT)
# kleines Glanzlicht
hl = int(dot * 0.42)
draw.ellipse([cx - dot*0.45 - hl, cy - dot*0.45 - hl, cx - dot*0.45 + hl, cy - dot*0.45 + hl],
             fill=(190, 240, 185, 230))

# --- Herunterskalieren (Anti-Aliasing) ---
base = img.resize((256, 256), Image.LANCZOS)

os.makedirs('assets', exist_ok=True)
base.save('assets/icon.png')
sizes = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
base.save('icon.ico', sizes=sizes)
print('Icon erzeugt: icon.ico  +  assets/icon.png')
