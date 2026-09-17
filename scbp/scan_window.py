# -*- coding: utf-8 -*-
#
# SC BP Watcher — zeigt live neue Star-Citizen-Baupläne an.
# Copyright (C) 2026 Xharig
#
# SPDX-License-Identifier: GPL-3.0-only
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, version 3.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
#
# Das aufziehbare Scan-Fenster ist angelehnt an eine Arbeit von ryze
# (MIT License, Copyright (c) 2026 ryze) — voller Lizenztext in
# `scbp/signature_scan.py`.
"""
Das Scan-Fenster: der Spieler zeigt, wo die Signatur steht.

⭐⭐ **Vorgabe (16.09.2026):** Der Spieler zieht ein Fenster über die Zahl — in
Lage UND Größe. Gemessen: Die Anzeige sitzt je Schiff woanders; ein fest
eingebauter Bereich wäre beim zweiten Schiff falsch.

| Teil | Wozu |
|---|---|
| Leiste oben | zum Verschieben, mit der gerade gelesenen Zahl |
| Loch in der Mitte | durchsichtig **und** durchklickbar (`-transparentcolor`) — das Spiel bleibt bedienbar |
| Griff unten rechts | Größe ändern |
| Tafel darunter | Vorschau, Anlernen, Übernehmen |

⚠ Das Loch fotografiert sich nicht selbst: geschichtete Fenster fehlen im
Abgriff (siehe `screen_grab`). Die Vorschau zeigt also, was **hinter** dem
Fenster steht.

⚠ Gilt vorerst nur unter Windows — `-transparentcolor` gibt es unter Linux
nicht. Dort wird es gebaut, sobald der Abgriff über das Portal steht.
"""
import base64
import struct
import tkinter as tk
import zlib

from . import errors, screen_grab, signature_scan
from .language import t

BG = '#10141c'
SURFACE = '#161c28'
FG = '#e6edf3'
SUB = '#8b98a5'
ACCENT = '#9ce430'
RED = '#e05252'
GOLD = '#e8c353'
HOLE = '#010203'           # die Farbe, die Windows durchsichtig macht

START_W, START_H = 220, 44
MIN_W, MIN_H = 40, 14
PREVIEW_MS = 400
PANEL_W, PANEL_H = 360, 250
GRIP = 12
ALPHA = 0.6
PREVIEW_H = 60

_open = [None]


def describe(value):
    """„12,680 → 4× Quantainium" — was hinter einer Signatur steckt."""
    if value is None:
        return ''
    from . import mining
    try:
        hits = [h for h in mining.find_signature(str(value)) if abs(h[3]) < 0.05]
    except Exception:
        hits = []
    number = '{:,}'.format(int(value))
    if not hits:
        return number
    parts = ['%d× %s' % (count, name) for name, count, _total, _dev in hits[:3]]
    return '%s → %s' % (number, ' · '.join(parts))


def fit_preview(raster, width, height):
    """Den Ausschnitt so vergrößern oder ausdünnen, dass er in die Vorschau passt."""
    rows = len(raster)
    cols = len(raster[0]) if rows else 0
    if not rows or not cols:
        return [[0]]
    zoom = max(1, min(4, width // cols, height // rows))
    if zoom > 1:
        return [[v for v in row for _ in range(zoom)]
                for row in raster for _ in range(zoom)]
    step = max(1, -(-cols // width), -(-rows // height))
    return [row[::step] for row in raster[::step]]


def png_data(raster, zoom=1):
    """Graustufenraster → PNG als Base64 für `tk.PhotoImage(data=…)`.

    ⚠ PNG, nicht PGM: Tk nimmt PGM über `data=` nicht an („couldn't recognize
    image data") — im RC 1 schrieb das die Vorschau alle 0,4 s ins
    Fehlerprotokoll, und sie blieb leer.
    """
    height = len(raster)
    width = len(raster[0]) if height else 0
    raw = bytearray()
    for row in raster:
        line = bytearray()
        for value in row:
            line.extend(bytes((max(0, min(255, int(value))),)) * zoom)
        for _ in range(zoom):
            raw.append(0)
            raw.extend(line)

    def chunk(kind, body):
        block = kind + body
        return (struct.pack('>I', len(body)) + block
                + struct.pack('>I', zlib.crc32(block) & 0xFFFFFFFF))

    png = (b'\x89PNG\r\n\x1a\n'
           + chunk(b'IHDR', struct.pack('>IIBBBBB', width * zoom, height * zoom,
                                        8, 0, 0, 0, 0))
           + chunk(b'IDAT', zlib.compress(bytes(raw), 6))
           + chunk(b'IEND', b''))
    return base64.b64encode(png)


class ScanWindow(object):
    """Das Anlern-Fenster — ein einziges zur Zeit.

    ⚠⚠ **Umgebaut am 17.09.2026.** Vorher zog der Spieler ein Fenster über die
    Zahl, und das Fenster las live. Zwei Gründe, warum das nicht trug:
    **die Pille wandert mit dem gescannten Brocken** (ein fester Bereich traf nur
    zufällig), und der gelesene Wert **stand oft unter einer Sekunde** da —
    „Lotto spielen, ob man beim Speichern die Zahl noch trifft".

    Jetzt sucht die Wache die Pille selbst (`signature_scan.search`), und dieses
    Fenster zeigt das zuletzt gefundene Bild **stehend**. Erst „Nächstes Bild"
    holt ein neues. So bleibt Zeit, die Zahl in Ruhe einzutippen.
    """

    def __init__(self, master, on_saved=None):
        self.master = master
        self.on_saved = on_saved
        self.raster = None
        self.frame_time = None
        self.preview_image = None
        self.last_error = None
        self.win = tk.Toplevel(master)
        self.win.title(t('scan_titel'))
        self.win.configure(bg=SURFACE)
        self.win.resizable(False, False)
        try:
            self.win.attributes('-topmost', True)
        except tk.TclError:
            pass
        font = ('Segoe UI', 9)
        width = PANEL_W - 12

        tk.Label(self.win, text=t('scan_hinweis'), bg=SURFACE, fg=SUB, font=font,
                 anchor='w', justify='left', wraplength=width
                 ).pack(fill='x', padx=6, pady=(6, 2))
        self.result = tk.Label(self.win, text='', bg=SURFACE, fg=FG, font=font,
                               anchor='nw', justify='left', height=2,
                               wraplength=width)
        self.result.pack(fill='x', padx=6)
        self.blank = tk.PhotoImage(width=width, height=PREVIEW_H)
        self.preview = tk.Label(self.win, bg=BG, image=self.blank,
                                width=width, height=PREVIEW_H)
        self.preview.pack(padx=6, pady=2)

        learn = tk.Frame(self.win, bg=SURFACE)
        learn.pack(fill='x', padx=6, pady=2)
        tk.Label(learn, text=t('scan_richtig'), bg=SURFACE, fg=SUB,
                 font=font).pack(side='left')
        self.typed = tk.Entry(learn, width=9, bg=BG, fg=FG, insertbackground=FG,
                              relief='flat', font=font)
        self.typed.pack(side='left', padx=4)
        self.typed.bind('<Return>', lambda _e: self._learn())
        self._link(learn, t('scan_anlernen'), self._learn, ACCENT).pack(side='left')

        buttons = tk.Frame(self.win, bg=SURFACE)
        buttons.pack(fill='x', padx=6, pady=(2, 2))
        self._link(buttons, t('scan_naechstes'), self._next).pack(side='left')
        self._link(buttons, t('scan_ordner'), open_sample_folder, SUB).pack(side='left', padx=12)
        self._link(buttons, t('scan_schliessen'), self.close, SUB).pack(side='left')
        self.note = tk.Label(self.win, text='', bg=SURFACE, fg=SUB, font=font,
                             anchor='nw', justify='left', height=4,
                             wraplength=width)
        self.note.pack(fill='x', padx=6, pady=(0, 6))

        self.win.protocol('WM_DELETE_WINDOW', self.close)
        self.win.bind('<Escape>', lambda _e: self.close())
        self._tick()

    @staticmethod
    def _link(parent, text, action, color=FG):
        label = tk.Label(parent, text=text, bg=SURFACE, fg=color, cursor='hand2',
                         font=('Segoe UI', 9, 'underline'))
        label.bind('<Button-1>', lambda _e: action())
        return label

    def _tick(self):
        """Ein neues Bild übernehmen — aber nur, solange keins steht."""
        try:
            if not self.win.winfo_exists():
                return
        except tk.TclError:
            return
        try:
            from . import signature_watch
            if not signature_watch.running():
                self.result.configure(text=t('scan_wache_aus'), fg=GOLD)
            elif self.raster is None:
                frame = signature_watch.last_frame()
                if frame and frame[1] != self.frame_time:
                    self._show(frame)
                elif not frame:
                    self.result.configure(text=t('scan_warte'), fg=SUB)
        except Exception as exc:
            if str(exc) != self.last_error:
                self.last_error = str(exc)
                errors.record('scan_window.tick', exc)
        self.win.after(PREVIEW_MS, self._tick)

    def _show(self, frame):
        raster, stamp, value = frame[0], frame[1], (frame[2] if len(frame) > 2 else None)
        self.raster, self.frame_time = raster, stamp
        if value is not None:
            self.result.configure(text=t('scan_gelesen') % describe(value), fg=ACCENT)
        else:
            self.result.configure(text=t('scan_grund_unsicher'), fg=GOLD)
        self.preview_image = tk.PhotoImage(
            data=png_data(fit_preview(raster, PANEL_W - 12, PREVIEW_H)))
        self.preview.configure(image=self.preview_image)

    def _next(self):
        """Das stehende Bild freigeben — das nächste gefundene wird gezeigt."""
        self.raster = None
        self.note.configure(text='')
        self.result.configure(text=t('scan_warte'), fg=SUB)
        self.preview.configure(image=self.blank)

    def _learn(self):
        if not self.raster:
            self.note.configure(text=t('scan_warte'), fg=SUB)
            return
        typed = self.typed.get()
        ok, reason, stats = signature_scan.learn(self.raster, typed)
        if not ok:
            self.note.configure(text=t('scan_grund_' + reason), fg=RED)
            return
        try:
            from . import signature_watch
            signature_watch.reload_templates()
        except Exception:
            pass
        # ⭐ „Neu" heißt: VerseKit hätte diese Ziffer vorher NICHT richtig
        # gelesen (siehe `signature_scan.learn`). Sind alle bekannt, weiß der
        # Spieler: genug angelernt. Gespeichert wird in jedem Fall.
        digits = ''.join(c for c in typed if c.isdigit())
        number = '{:,}'.format(int(digits))
        if stats['neu'] == 0 and not stats['unklar']:
            lines = [t('scan_gelernt_alle') % (number, stats['erkannt'])]
        else:
            lines = [t('scan_gelernt') % (number, stats['neu'], stats['erkannt'])]
        if stats['unklar']:
            lines.append(t('scan_gelernt_unklar')
                         % ', '.join(str(p) for p in stats['unklar']))
        check = signature_scan.read(self.raster)['wert']
        if check is not None and str(check) == digits:
            lines.append(t('scan_probe_ok') % number)
            color = ACCENT
        else:
            lines.append(t('scan_probe_nein'))
            color = GOLD
        self.note.configure(text='\n'.join(lines), fg=color)
        self.typed.delete(0, 'end')

    def close(self):
        _open[0] = None
        try:
            self.win.destroy()
        except tk.TclError:
            pass


def open_sample_folder():
    """Den Ordner mit den angelernten Bildern öffnen (17.09.2026: „wo findet ein
    User die gespeicherten Signaturen und Bilder?" — vorher nirgends)."""
    import os
    folder = signature_scan.sample_folder()
    try:
        os.makedirs(folder, exist_ok=True)
    except OSError:
        pass
    from .pages import _show_folder
    return _show_folder(folder)


def open_window(master, on_saved=None):
    """Das Scan-Fenster zeigen — oder das offene nach vorn holen."""
    current = _open[0]
    if current is not None:
        try:
            if current.win.winfo_exists():
                current.win.lift()
                return current
        except tk.TclError:
            pass
    _open[0] = ScanWindow(master, on_saved)
    return _open[0]


def available():
    """Gibt es das Scan-Fenster auf diesem System?"""
    return screen_grab.supported()
