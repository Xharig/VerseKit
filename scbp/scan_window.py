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
import tkinter as tk

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


def _pgm(raster, zoom):
    """Graustufenraster → PGM-Daten für `tk.PhotoImage` (vergrößert)."""
    height = len(raster)
    width = len(raster[0]) if height else 0
    body = bytearray()
    for row in raster:
        line = bytearray()
        for value in row:
            line.extend(bytes((value,)) * zoom)
        for _ in range(zoom):
            body.extend(line)
    header = ('P5 %d %d 255\n' % (width * zoom, height * zoom)).encode('ascii')
    return base64.b64encode(header + bytes(body))


class ScanWindow(object):
    """Ein einziges Scan-Fenster zur Zeit — ein zweiter Aufruf holt es nach vorn."""

    def __init__(self, master, on_saved=None):
        self.master = master
        self.on_saved = on_saved
        self.raster = None
        self.preview_image = None
        self.drag = None
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

        frame = tk.Frame(self.win, bg=ACCENT)
        frame.pack(padx=2, pady=(0, 2))
        self.hole = tk.Frame(frame, bg=HOLE, width=START_W, height=START_H)
        self.hole.pack()
        self.grip = tk.Frame(frame, bg=ACCENT, width=12, height=12,
                             cursor='size_nw_se')
        self.grip.place(relx=1.0, rely=1.0, anchor='se')
        self.grip.bind('<ButtonPress-1>', self._resize_start)
        self.grip.bind('<B1-Motion>', self._resize_move)

        panel = tk.Frame(self.win, bg=SURFACE)
        panel.pack(fill='x')
        self.result = tk.Label(panel, text='', bg=SURFACE, fg=FG, font=font,
                               anchor='w', justify='left')
        self.result.pack(fill='x', padx=6, pady=(4, 0))
        self.preview = tk.Label(panel, bg=SURFACE)
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
                             anchor='w', justify='left', wraplength=260)
        self.note.pack(fill='x', padx=6, pady=(0, 4))

        self._place()
        self.win.bind('<Escape>', lambda _e: self.close())
        self._tick()

    @staticmethod
    def _link(parent, text, action, color=FG):
        label = tk.Label(parent, text=text, bg=SURFACE, fg=color, cursor='hand2',
                         font=('Segoe UI', 9, 'underline'))
        label.bind('<Button-1>', lambda _e: action())
        return label

    # --- Lage -------------------------------------------------------------

    def _scale(self):
        """Physische Punkte je Tk-Punkt (125 % → 1,25)."""
        rect = screen_grab.window_rect(self.hole)
        width = self.hole.winfo_width()
        if rect and width > 1:
            return rect[2] / float(width)
        return 1.0

    def _place(self):
        saved = signature_scan.region()
        self.win.update_idletasks()
        if saved:
            # Gemerkt ist physisch; Tk rechnet logisch.
            self.hole.configure(width=START_W, height=START_H)
            self.win.geometry('+%d+%d' % (self.master.winfo_screenwidth() // 2,
                                          self.master.winfo_screenheight() // 3))
            self.win.update_idletasks()
            scale = self._scale()
            self.hole.configure(width=max(MIN_W, int(saved[2] / scale)),
                                height=max(MIN_H, int(saved[3] / scale)))
            self.win.update_idletasks()
            offset_x = self.hole.winfo_rootx() - self.win.winfo_rootx()
            offset_y = self.hole.winfo_rooty() - self.win.winfo_rooty()
            self.win.geometry('+%d+%d' % (int(saved[0] / scale) - offset_x,
                                          int(saved[1] / scale) - offset_y))
        else:
            width = self.master.winfo_screenwidth()
            height = self.master.winfo_screenheight()
            self.win.geometry('+%d+%d' % (width // 2 - START_W // 2, height // 3))

    def _drag_start(self, event):
        self.drag = (event.x_root - self.win.winfo_rootx(),
                     event.y_root - self.win.winfo_rooty())

    def _drag_move(self, event):
        if self.drag:
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

    def _tick(self):
        if not self.win.winfo_exists():
            return
        try:
            rect = screen_grab.window_rect(self.hole)
            if rect:
                self.raster = screen_grab.grab(*rect)
                found = signature_scan.read(self.raster)
                if found['wert'] is not None:
                    self.result.configure(text=describe(found['wert']), fg=ACCENT)
                    self.bar_label.configure(text='{:,}'.format(found['wert']))
                else:
                    self.result.configure(
                        text=t('scan_grund_' + (found['grund'] or 'kein_text')),
                        fg=SUB)
                    self.bar_label.configure(text=t('scan_ziehen'))
                zoom = max(1, min(3, 240 // max(1, rect[2])))
                self.preview_image = tk.PhotoImage(data=_pgm(self.raster, zoom),
                                                   format='PPM')
                self.preview.configure(image=self.preview_image)
        except screen_grab.GrabError as exc:
            self.result.configure(text=t('scan_grund_' + exc.reason), fg=RED)
        except Exception as exc:
            errors.record('scan_window.tick', exc)
        self.win.after(PREVIEW_MS, self._tick)

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
        rect = screen_grab.window_rect(self.hole)
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
