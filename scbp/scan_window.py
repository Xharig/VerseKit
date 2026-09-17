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
import threading
import time
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
HOLE = '#010203'           # die Farbe, die Windows durchsichtig macht

START_W, START_H = 220, 44
MIN_W, MIN_H = 40, 14
PREVIEW_MS = 400
PANEL_W, PANEL_H = 360, 250
GRIP = 12
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
    """Ein einziges Scan-Fenster zur Zeit — ein zweiter Aufruf holt es nach vorn."""

    def __init__(self, master, on_saved=None):
        self.master = master
        self.on_saved = on_saved
        self.raster = None
        self.preview_image = None
        self.last_error = None
        self.drag = None
        # ⚠⚠ Abgreifen und Lesen laufen in einem **eigenen Faden**. Im RC 1
        # lagen sie im Tk-Faden: 150 ms je Lesung, und das Fenster ruckelte
        # beim Ziehen. Getauscht wird nur über diese zwei Felder.
        self.rect = None             # physisch, gesetzt vom Tk-Faden
        self.latest = None           # (raster, ergebnis) vom Lese-Faden
        self.stop = threading.Event()
        self.win = tk.Toplevel(master)
        self.win.overrideredirect(True)
        self.win.configure(bg=ACCENT)
        try:
            self.win.attributes('-topmost', True)
            self.win.attributes('-transparentcolor', HOLE)
        except tk.TclError:
            pass
        font = ('Segoe UI', 9)

        bar = tk.Frame(self.win, bg=ACCENT, cursor='fleur')
        bar.pack(fill='x')
        self.bar_label = tk.Label(bar, text=t('scan_ziehen'), bg=ACCENT, fg=BG,
                                  font=(font[0], font[1], 'bold'), anchor='w')
        self.bar_label.pack(side='left', fill='x', expand=True, padx=4)
        for widget in (bar, self.bar_label):
            widget.bind('<ButtonPress-1>', self._drag_start)
            widget.bind('<B1-Motion>', self._drag_move)
            widget.bind('<ButtonRelease-1>', self._drag_end)

        frame = tk.Frame(self.win, bg=ACCENT)
        frame.pack(padx=2)
        self.hole = tk.Frame(frame, bg=HOLE, width=START_W, height=START_H)
        self.hole.pack()
        # ⚠ Der Griff sitzt UNTER dem Loch, nicht darin: Im RC 2 lag er im
        # Loch und wurde als weißes Quadrat mit abfotografiert.
        edge = tk.Frame(frame, bg=ACCENT, height=GRIP)
        edge.pack(fill='x')
        self.grip = tk.Frame(edge, bg=BG, width=GRIP, height=GRIP,
                             cursor='size_nw_se')
        self.grip.pack(side='right')
        self.grip.bind('<ButtonPress-1>', self._resize_start)
        self.grip.bind('<B1-Motion>', self._resize_move)
        self.grip.bind('<ButtonRelease-1>', self._drag_end)

        # ⚠⚠ **Feste Größe.** Im RC 1 wuchs und schrumpfte das Fenster mit
        # jeder Meldung („springt"), weil die Tafel sich nach dem Text richtete.
        panel = tk.Frame(self.win, bg=SURFACE, width=PANEL_W, height=PANEL_H)
        panel.pack_propagate(False)
        panel.pack()
        self.result = tk.Label(panel, text='', bg=SURFACE, fg=FG, font=font,
                               anchor='nw', justify='left', height=2,
                               wraplength=PANEL_W - 12)
        self.result.pack(fill='x', padx=6, pady=(4, 0))
        self.blank = tk.PhotoImage(width=PANEL_W - 12, height=PREVIEW_H)
        self.preview = tk.Label(panel, bg=BG, image=self.blank, anchor='w',
                                width=PANEL_W - 12, height=PREVIEW_H)
        self.preview.pack(padx=6, pady=2, anchor='w')

        learn = tk.Frame(panel, bg=SURFACE)
        learn.pack(fill='x', padx=6, pady=2)
        tk.Label(learn, text=t('scan_richtig'), bg=SURFACE, fg=SUB,
                 font=font).pack(side='left')
        self.typed = tk.Entry(learn, width=9, bg=BG, fg=FG, insertbackground=FG,
                              relief='flat', font=font)
        self.typed.pack(side='left', padx=4)
        self._link(learn, t('scan_anlernen'), self._learn).pack(side='left')

        buttons = tk.Frame(panel, bg=SURFACE)
        buttons.pack(fill='x', padx=6, pady=(2, 6))
        self._link(buttons, t('scan_uebernehmen'), self._save, ACCENT).pack(side='left')
        self._link(buttons, t('scan_abbrechen'), self.close, SUB).pack(side='left', padx=12)
        self.note = tk.Label(panel, text='', bg=SURFACE, fg=SUB, font=font,
                             anchor='nw', justify='left', height=4,
                             wraplength=PANEL_W - 12)
        self.note.pack(fill='x', padx=6, pady=(0, 4))

        self._place()
        self.win.bind('<Escape>', lambda _e: self.close())
        threading.Thread(target=self._reader, daemon=True,
                         name='scan-fenster').start()
        self._tick()

    @staticmethod
    def _link(parent, text, action, color=FG):
        label = tk.Label(parent, text=text, bg=SURFACE, fg=color, cursor='hand2',
                         font=('Segoe UI', 9, 'underline'))
        label.bind('<Button-1>', lambda _e: action())
        return label

    # --- Lage -------------------------------------------------------------

    def _place(self):
        """Das Loch dorthin legen, wo der gemerkte Bereich liegt.

        ⚠ Gemerkt ist **physisch**, Tk rechnet **logisch** — umgerechnet wird
        allein über `screen_grab.to_logical`.
        """
        saved = signature_scan.region()
        self.win.update_idletasks()
        if saved:
            left, top, width, height = screen_grab.to_logical(saved)
            self.hole.configure(width=max(MIN_W, width), height=max(MIN_H, height))
            self.win.update_idletasks()
            offset_x = self.hole.winfo_rootx() - self.win.winfo_rootx()
            offset_y = self.hole.winfo_rooty() - self.win.winfo_rooty()
            self.win.geometry('+%d+%d' % (left - offset_x, top - offset_y))
        else:
            width = self.master.winfo_screenwidth()
            height = self.master.winfo_screenheight()
            self.win.geometry('+%d+%d' % (width // 2 - START_W // 2, height // 3))

    def _drag_start(self, event):
        self.drag = (event.x_root - self.win.winfo_rootx(),
                     event.y_root - self.win.winfo_rooty())

    def _drag_end(self, _event=None):
        self.drag = None

    def _drag_move(self, event):
        if self.drag and len(self.drag) == 2:
            self.win.geometry('+%d+%d' % (event.x_root - self.drag[0],
                                          event.y_root - self.drag[1]))

    def _resize_start(self, event):
        self.drag = (event.x_root, event.y_root,
                     self.hole.winfo_width(), self.hole.winfo_height())

    def _resize_move(self, event):
        if self.drag and len(self.drag) == 4:
            self.hole.configure(
                width=max(MIN_W, self.drag[2] + event.x_root - self.drag[0]),
                height=max(MIN_H, self.drag[3] + event.y_root - self.drag[1]))

    # --- Lesen, Anlernen, Übernehmen --------------------------------------

    def _reader(self):
        """Lese-Faden: holt den Bereich hinter dem Loch und liest ihn."""
        while not self.stop.is_set():
            started = time.time()
            rect = self.rect
            try:
                if rect:
                    raster = screen_grab.grab(*rect)
                    self.latest = (raster, signature_scan.read(raster), None)
            except screen_grab.GrabError as exc:
                self.latest = (None, None, exc.reason)
            except Exception as exc:
                if str(exc) != self.last_error:
                    self.last_error = str(exc)
                    errors.record('scan_window.reader', exc)
            self.stop.wait(max(0.05, PREVIEW_MS / 1000.0 - (time.time() - started)))

    def _tick(self):
        """Tk-Faden: Lage weitergeben, letztes Ergebnis zeigen."""
        try:
            if not self.win.winfo_exists():
                return
        except tk.TclError:
            return
        try:
            # Während des Ziehens nicht lesen — das Bild zeigt sonst den Weg.
            self.rect = None if self.drag else screen_grab.widget_rect(self.hole)
            latest, self.latest = self.latest, None
            if latest is not None:
                raster, found, reason = latest
                if reason:
                    self.result.configure(text=t('scan_grund_' + reason), fg=RED)
                elif found['ziffern'] or self.raster is None:
                    # ⚠⚠ **Nur ein Bild MIT Zahl ersetzt das angezeigte.** Wer das
                    # Fenster anklickt, holt es nach vorn — Star Citizen schaltet
                    # dann den Scanner ab, und die Zahl ist weg (Einwand vom
                    # 17.09.2026: „so kann das ja gar nicht klappen"). Stehen
                    # bleibt deshalb das letzte Bild, in dem eine Zahl stand;
                    # Anlernen und Übernehmen beziehen sich darauf.
                    self.raster = raster
                    if found['wert'] is not None:
                        self.result.configure(text=describe(found['wert']), fg=ACCENT)
                        self.bar_label.configure(text='{:,}'.format(found['wert']))
                    elif found['ziffern']:
                        self.result.configure(text=t('scan_grund_unsicher'), fg=SUB)
                        self.bar_label.configure(text=t('scan_ziehen'))
                    else:
                        self.result.configure(text=t('scan_zurueck_ins_spiel'), fg=SUB)
                    self.preview_image = tk.PhotoImage(
                        data=png_data(fit_preview(raster, PANEL_W - 12, PREVIEW_H)))
                    self.preview.configure(image=self.preview_image)
        except Exception as exc:
            # ⚠ Einmal je Fehlerart, nicht alle 0,4 s.
            if str(exc) != self.last_error:
                self.last_error = str(exc)
                errors.record('scan_window.tick', exc)
        self.win.after(PREVIEW_MS // 2, self._tick)

    def _learn(self):
        if not self.raster:
            return
        ok, reason, added = signature_scan.learn(self.raster, self.typed.get())
        if ok:
            self.note.configure(text=t('scan_gelernt') % added, fg=ACCENT)
            self.typed.delete(0, 'end')
        else:
            self.note.configure(text=t('scan_grund_' + reason), fg=RED)

    def _save(self):
        rect = screen_grab.widget_rect(self.hole)
        if not rect:
            self.note.configure(text=t('scan_grund_bereich_ungueltig'), fg=RED)
            return
        signature_scan.set_region(rect)
        if self.on_saved:
            try:
                self.on_saved(rect)
            except Exception as exc:
                errors.record('scan_window.on_saved', exc)
        self.close()

    def close(self):
        self.stop.set()
        _open[0] = None
        try:
            self.win.destroy()
        except tk.TclError:
            pass


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
