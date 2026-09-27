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
Die neuen Einstellungsseiten „Module" und „Startprogramme" (v3.58.0-rc4).

Eigene Datei aus demselben Grund wie `stats_pages.py`: `pages.py` ist groß
genug. Die Bausteine kommen aus `pages`, damit alles gleich aussieht.
"""
import os
import tkinter as tk

from .language import t
from .pages import (BG, SURFACE, FG, SUB, ACCENT, LINE,
                    _heading, _scroll_area, _body_text, _button, _wrap)


def builders():
    return {'module': modules_page, 'startprogramme': start_programs_page}


def _card(window, parent):
    from .main_window import round_frame
    card = round_frame(parent, SURFACE, LINE, radius=8, base_color=BG)
    card.holder.pack(fill='x', pady=(0, 10))
    return card


def _switch_head(window, card, title, on, flip, note=''):
    """Kopfzeile einer Karte: Titel links, Schiebeschalter rechts."""
    from .main_window import toggle_switch
    head = tk.Frame(card, bg=SURFACE)
    head.pack(fill='x', padx=16, pady=(12, 0))
    toggle_switch(head, on, flip, bg=SURFACE).pack(side='right')
    tk.Label(head, text=title, bg=SURFACE, fg=FG, font=window.f_bold,
             anchor='w').pack(side='left', fill='x', expand=True)
    if note:
        line = tk.Label(card, text=note, bg=SURFACE, fg=SUB,
                        font=window.f_small, anchor='w', justify='left')
        line.pack(fill='x', padx=16, pady=(4, 12))
        _wrap(line, inset=32)
    else:
        tk.Frame(card, bg=SURFACE, height=12).pack(fill='x')


# ----------------------------------------------------------------- Module
def modules_page(window, frame):
    """Gruppen der Seitenleiste ein- und ausblenden — siehe `modules`."""
    from . import modules
    _heading(window, frame, t('hf_module'), t('s_mo_lead'))
    inner = _scroll_area(frame)
    for group in modules.SWITCHABLE:
        pages = ', '.join(t(_TAB_LABELS.get(p, p))
                          for p in modules.SWITCHABLE[group])

        def flip(group=group):
            new_value = not modules.enabled(group)
            modules.set_enabled(group, new_value)
            window.say('%s: %s' % (t(modules.LABELS[group]),
                                   t('e_an') if new_value else t('e_aus')))
            # Die Leiste wird beim Aufbau zusammengesetzt — erst der Neuaufbau
            # zeigt die Gruppe oder nimmt sie weg.
            window.root.after(60, window.rebuild)
            return new_value

        _switch_head(window, _card(window, inner), t(modules.LABELS[group]),
                     modules.enabled(group), flip,
                     t('s_mo_enthaelt') % pages)
    _body_text(inner, t('s_mo_hinweis'), window.f_small, fill='x',
               pady=(6, 20))


# Beschriftung je Seite — dieselben Texte wie in der Leiste.
_TAB_LABELS = {'hangar': 'hf_hangar', 'wunschliste': 'hf_wunschliste',
               'einkaufsliste': 'hf_einkaufsliste', 'asop': 'hf_asop',
               'lager': 'hf_lager', 'herstellung': 'hf_herstellung',
               'bergbau': 'hf_bergbau', 'raffinerien': 'hf_raffinerien',
               'laeden': 'hf_laeden', 'farmliste': 'hf_farmliste',
               'bergung': 'hf_bergung', 'zerlegen': 'hf_zerlegen',
               'handelslager': 'hf_handelslager', 'verkauf': 'hf_verkauf',
               'routen': 'hf_routen'}


# ---------------------------------------------------------- Startprogramme
def start_programs_page(window, frame):
    """Programme mit dem Spiel starten — siehe `start_programs`.

    ⚠ Alles wird sofort gespeichert (wie jede Einstellung im Programm), es
    gibt keinen Speichern-Knopf. Felder speichern beim Verlassen."""
    from . import paths, start_programs
    _heading(window, frame, t('hf_startprogramme'), t('s_pg_lead'))
    inner = _scroll_area(frame)

    def flip_all():
        new_value = not start_programs.enabled()
        paths.set_setting(start_programs.SETTING_ON, new_value)
        window.say('%s: %s' % (t('s_pg_aktiv'),
                               t('e_an') if new_value else t('e_aus')))
        return new_value

    _switch_head(window, _card(window, inner), t('s_pg_aktiv'),
                 start_programs.enabled(), flip_all, t('s_pg_aktiv_h'))
    listing = tk.Frame(inner, bg=BG)
    listing.pack(fill='x')

    def store(index, **changes):
        items = start_programs.entries()
        if 0 <= index < len(items):
            items[index].update(changes)
            start_programs.save(items)

    def draw():
        for child in listing.winfo_children():
            child.destroy()
        items = start_programs.entries()
        if not items:
            _body_text(listing, t('s_pg_leer'), window.f_small,
                       fill='x', pady=(4, 10))
        for index, entry in enumerate(items):
            _entry_card(window, listing, index, entry, store, draw)

    def add():
        items = start_programs.entries()
        items.append(start_programs.new_entry())
        start_programs.save(items)
        draw()

    draw()
    _button(window, inner, t('s_pg_neu'), add, strong=True).pack(
        anchor='w', pady=(4, 10))
    _body_text(inner, t('s_pg_risiko'), window.f_small, fill='x',
               pady=(6, 20))


WHEN_LABELS = (('launcher', 's_pg_w_launcher'), ('spiel', 's_pg_w_spiel'),
               ('ersetzt', 's_pg_w_ersetzt'))


def _entry_card(window, parent, index, entry, store, redraw):
    """Eine Karte je Eintrag: Schalter, Name, Datei, Argumente, wann, Warten,
    beim Spielende beenden, jetzt starten, entfernen."""
    from . import file_picker, start_programs
    from .main_window import round_entry, toggle_switch
    from .pages import _choice, _setting_row
    card = _card(window, parent)

    def flip():
        new_value = not entry['an']
        entry['an'] = new_value
        store(index, an=new_value)
        return new_value

    title = entry['name'] or os.path.basename(entry['datei'] or '') \
        or t('s_pg_neuer')
    _switch_head(window, card, title, entry['an'], flip,
                 entry['datei'] or t('s_pg_keine_datei'))
    body = tk.Frame(card, bg=SURFACE)
    body.pack(fill='x', padx=16, pady=(0, 12))

    def field(label, key, placeholder=''):
        row = tk.Frame(body, bg=SURFACE)
        row.pack(fill='x', pady=(6, 0))
        tk.Label(row, text=label, bg=SURFACE, fg=FG, font=window.f_small,
                 width=14, anchor='w').pack(side='left')
        # ⚠ Gelesen wird über die Variable, nie über `box.get()`: Steht der
        # graue Hinweis im Feld, lieferte `get()` ihn als Wert (`fields.hint`).
        variable = tk.StringVar(value=str(entry.get(key) or ''))
        box = round_entry(row, variable, window.f_small, '#0c1017', LINE,
                          ACCENT, FG, placeholder=placeholder or None)
        box.variable = variable
        box.holder.pack(side='left', fill='x', expand=True)

        def keep(_event=None):
            value = variable.get().strip()
            if key == 'warten':
                try:
                    value = max(0, int(value or 0))
                except ValueError:
                    value = 0
            store(index, **{key: value})

        box.bind('<FocusOut>', keep)
        box.bind('<Return>', keep)
        return row, box

    field(t('s_pg_name'), 'name')
    row, path_box = field(t('s_pg_datei'), 'datei')

    def browse():
        chosen = file_picker.open_file(
            t('s_pg_datei'), patterns=((t('s_pg_alle'), '*'),),
            start=os.path.dirname(entry['datei'] or '') or None)
        if chosen:
            path_box.variable.set(chosen)
            store(index, datei=chosen)
            redraw()

    _button(window, row, t('s_pg_suchen'), browse).pack(side='left',
                                                          padx=(8, 0))
    field(t('s_pg_argumente'), 'argumente', t('s_pg_keine'))
    field(t('s_pg_warten'), 'warten', '0')

    when_row = _setting_row(window, body, t('s_pg_wann'), '', wide=True)
    choice = _choice(window, when_row,
                     [(key, t(label)) for key, label in WHEN_LABELS],
                     entry['wann'],
                     lambda k: (choice.select(k), store(index, wann=k)))
    choice.pack(anchor='w')
    for key, label in WHEN_LABELS:
        if key == entry['wann']:
            note = tk.Label(body, text=t(label + '_h'), bg=SURFACE, fg=SUB,
                            font=window.f_small, anchor='w', justify='left')
            note.pack(fill='x', pady=(4, 0))
            _wrap(note, inset=32)

    end_row = tk.Frame(body, bg=SURFACE)
    end_row.pack(fill='x', pady=(8, 0))

    def flip_end():
        entry['beenden'] = not entry['beenden']
        store(index, beenden=entry['beenden'])
        return entry['beenden']

    toggle_switch(end_row, entry['beenden'], flip_end,
                  bg=SURFACE).pack(side='right')
    tk.Label(end_row, text=t('s_pg_beenden'), bg=SURFACE, fg=FG,
             font=window.f_small, anchor='w').pack(side='left')

    actions = tk.Frame(body, bg=SURFACE)
    actions.pack(fill='x', pady=(10, 0))

    def run_now():
        ok, reason = start_programs.launch(start_programs.entries()[index],
                                           delay=False)
        window.say(t('s_pg_gestartet') if ok
                   else t('s_pg_nicht') % reason)

    def remove():
        items = start_programs.entries()
        if 0 <= index < len(items):
            del items[index]
            start_programs.save(items)
        redraw()

    _button(window, actions, t('s_pg_jetzt'), run_now).pack(side='left')
    _button(window, actions, t('s_pg_entfernen'), remove,
            danger=True).pack(side='left', padx=(8, 0))
