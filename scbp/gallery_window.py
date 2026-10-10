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
"""
Das Fenster der Galerie „Was ist neu" (Daten: `gallery`).

Eine Seite je Höhepunkt (Bild, Art, Titel, Text, Bedienweg, Knopf zur Seite)
und zum Schluss die übrigen Punkte in zwei Spalten. Blättern mit den Knöpfen
unten, den Balken oder den Pfeiltasten. Nicht modal: Wer weiterarbeiten will,
schließt es oder lässt es stehen.
"""
import tkinter as tk

from . import errors, gallery, theme
from .language import current, t

BG = theme.BG
SURFACE = theme.SURFACE
FG = theme.FG
SUB = theme.SUB
ACCENT = theme.ACCENT
GOLD = theme.GOLD
LINE = theme.LINE

WIDTH = 760
_KIND_KEYS = {'neu': 'gal_art_neu', 'bess': 'gal_art_bess', 'fix': 'gal_art_fix'}


def show(window):
    """Galerie über dem Hauptfenster `window` öffnen. Gibt das Fenster zurück
    oder None, wenn die eigene Version keine Höhepunkte hat."""
    points = gallery.highlights(window.version)
    if not points:
        return None
    language = current()
    rest = []
    try:
        rest = gallery.rest(window.version, language)
    except Exception as exc:
        errors.record('gallery_window.rest', exc)
    slides = list(points) + ([None] if rest else [])

    win = tk.Toplevel(window.root)
    win.title(t('gal_titel') % window.version)
    win.configure(bg=BG, highlightthickness=1, highlightbackground=LINE,
                  highlightcolor=LINE)
    win.resizable(False, False)
    win.transient(window.root)
    state = {'index': 0, 'images': []}

    head = tk.Frame(win, bg=BG)
    head.pack(fill='x', padx=24, pady=(18, 8))
    tk.Label(head, text=t('gal_titel') % window.version, bg=BG, fg=FG,
             font=window.f_title, anchor='w').pack(side='left')

    foot = tk.Frame(win, bg=BG)
    foot.pack(side='bottom', fill='x', padx=24, pady=(8, 18))
    body = tk.Frame(win, bg=BG, width=WIDTH, height=660)
    body.pack(fill='both', expand=True, padx=24)
    body.pack_propagate(False)

    dots = tk.Frame(foot, bg=BG)
    dots.pack(side='left')

    def close():
        gallery.mark_shown(window.version)
        try:
            win.destroy()
        except tk.TclError:
            pass

    def go(index):
        state['index'] = max(0, min(len(slides) - 1, index))
        draw()

    def open_tab(tab):
        close()
        try:
            window.open_page(tab, via='sprung')
        except Exception as exc:
            errors.record('gallery_window.open_tab', exc)

    from .main_window import _dialog_button
    _dialog_button(foot, t('gal_schliessen'), close, window.f_small,
                   strong=True).pack(side='right')
    look = tk.Frame(foot, bg=BG)
    look.pack(side='right', padx=(0, 10))

    def label(parent, text, color=FG, font=None, wrap=WIDTH - 40):
        widget = tk.Label(parent, text=text, bg=BG, fg=color,
                          font=font or window.f_small, anchor='w',
                          justify='left', wraplength=wrap)
        widget.pack(fill='x')
        return widget

    def draw():
        for part in (body, dots, look):
            for child in part.winfo_children():
                child.destroy()
        state['images'] = []
        index = state['index']
        slide = slides[index]

        nav = tk.Frame(body, bg=BG)
        nav.pack(fill='x')
        if slide is not None:
            path = gallery.image_path(slide['bild'], language)
            if path:
                try:
                    image = tk.PhotoImage(master=win, file=path)
                    state['images'].append(image)
                    tk.Label(nav, image=image, bg=BG, bd=0).pack()
                except tk.TclError as exc:
                    errors.record('gallery_window.image', exc)
            label(body, t(_KIND_KEYS.get(slide['art'], 'gal_art_neu')).upper(),
                  color=ACCENT).pack_configure(pady=(14, 2))
            label(body, t(slide['titel']), font=window.f_title)
            label(body, t(slide['text'])).pack_configure(pady=(6, 0))
            label(body, t(slide['weg']), color=GOLD,
                  font=window.f_bold).pack_configure(pady=(8, 0))
            if slide.get('reiter'):
                _dialog_button(look, t('gal_ansehen'),
                               lambda tab=slide['reiter']: open_tab(tab),
                               window.f_small).pack()
        else:
            label(body, t('gal_weitere'), font=window.f_title).pack_configure(
                pady=(6, 10))
            columns = tk.Frame(body, bg=BG)
            columns.pack(fill='both', expand=True)
            half = (len(rest) + 1) // 2
            for part in (rest[:half], rest[half:]):
                column = tk.Frame(columns, bg=BG)
                column.pack(side='left', fill='both', expand=True, anchor='n')
                for _kind, title in part:
                    tk.Label(column, text='•  ' + title, bg=BG, fg=FG,
                             font=window.f_small, anchor='w', justify='left',
                             wraplength=WIDTH // 2 - 30).pack(fill='x',
                                                              pady=(0, 6))

        for n in range(len(slides)):
            dot = tk.Frame(dots, bg=ACCENT if n == index else LINE,
                           width=28, height=4, cursor='hand2')
            dot.pack(side='left', padx=3, pady=8)
            dot.bind('<Button-1>', lambda _e, n=n: go(n))
        arrows = tk.Frame(dots, bg=BG)
        arrows.pack(side='left', padx=(12, 0))
        if index > 0:
            _dialog_button(arrows, t('gal_zurueck'), lambda: go(index - 1),
                           window.f_small).pack(side='left')
        if index < len(slides) - 1:
            _dialog_button(arrows, t('gal_weiter'), lambda: go(index + 1),
                           window.f_small).pack(side='left', padx=(6, 0))

    win.bind('<Left>', lambda _e: go(state['index'] - 1))
    win.bind('<Right>', lambda _e: go(state['index'] + 1))
    win.bind('<Escape>', lambda _e: close())
    win.protocol('WM_DELETE_WINDOW', close)
    draw()
    win.update_idletasks()
    try:
        root = window.root
        x = root.winfo_rootx() + (root.winfo_width() - win.winfo_reqwidth()) // 2
        y = root.winfo_rooty() + (root.winfo_height() - win.winfo_reqheight()) // 3
        win.geometry('+%d+%d' % (max(0, x), max(0, y)))
    except tk.TclError:
        pass
    return win
