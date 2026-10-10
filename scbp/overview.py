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
Die Startseite: das Wichtigste aus allen Bereichen auf einen Blick, jede
Kachel mit einem Weg zur Seite, auf der es genauer steht.

Kacheln: Spiel · Baupläne · Übersetzung (Auswahl je Kanal) · Spielzeit ·
Rechner laut Spiel · Dienste (CIG, Basetool, Update). Jede lässt sich unter
der Kachelliste ein- und ausblenden und an ihrer Überschrift auf eine andere
ziehen, um die Plätze zu tauschen; beides wird gemerkt.

Die Zahlen rechnen die reinen Funktionen oben (`blueprint_numbers`,
`playtime_week`) — ohne Tk, damit sie sich prüfen lassen.
"""
import os
import threading
import time
import tkinter as tk

from . import errors, theme
from .language import current, t

WEEK_SEC = 7 * 24 * 3600


# ------------------------------------------------------------------ Rechnen
def _stamp(text):
    try:
        return time.mktime(time.strptime(text[:19], '%Y-%m-%d %H:%M:%S'))
    except (TypeError, ValueError):
        return None


def blueprint_numbers(stock, catalog, now=None):
    """`{'mine', 'total', 'week', 'newest', 'newest_at'}` aus Bestand und
    Katalog. Gezählt wird nur, was im Katalog steht — sonst stimmt der
    Anteil nicht."""
    now = now or time.time()
    blueprints = (catalog or {}).get('bauplaene') or {}
    own = (stock or {}).get('bauplaene') or {}
    mine = sum(1 for key in own if key in blueprints)
    week, newest, newest_at = 0, '', None
    for key, entry in own.items():
        at = _stamp((entry or {}).get('zeit') or '')
        if at is None:
            continue
        if now - at <= WEEK_SEC:
            week += 1
        if newest_at is None or at > newest_at:
            newest, newest_at = (entry or {}).get('name') or key, at
    return {'mine': mine, 'total': len(blueprints), 'week': week,
            'newest': newest, 'newest_at': newest_at}


def playtime_week(spans, now=None):
    """Spielzeit der letzten sieben Tage in Sekunden — `spans` sind
    zusammengeführte `(von, bis)`, Teile davor werden abgeschnitten."""
    now = now or time.time()
    start = now - WEEK_SEC
    return sum(max(0, min(end, now) - max(begin, start))
               for begin, end in spans)


# ------------------------------------------------------------------ Oberfläche
def builders():
    return {'uebersicht': overview_page}


def overview_page(window, frame):
    from .pages import _heading, _scroll_area
    _heading(window, frame, t('hf_uebersicht'), t('s_uv_lead'))
    parent = _scroll_area(frame)
    grid = tk.Frame(parent, bg=theme.BG)
    grid.pack(fill='x')
    grid.columnconfigure(0, weight=1, uniform='spalte')
    grid.columnconfigure(1, weight=1, uniform='spalte')

    chooser = tk.Frame(parent, bg=theme.BG)
    chooser.pack(fill='x', pady=(4, 16))

    def draw():
        for child in grid.winfo_children():
            child.destroy()
        builders_by_id = {'spiel': _tile_game, 'bauplaene': _tile_blueprints,
                          'uebersetzung': _tile_translation,
                          'spielzeit': _tile_playtime, 'rechner': _tile_machine,
                          'dienste': _tile_services}
        visible = visible_tiles()
        shown = [key for key in tile_order() if key in visible]
        for index, key in enumerate(shown):
            build = builders_by_id[key]
            cell = tk.Frame(grid, bg=theme.BG)
            cell.tile_key = key
            cell.grid(row=index // 2, column=index % 2, sticky='nsew',
                      padx=(6, 0) if index % 2 else (0, 6), pady=(0, 12))
            try:
                build(window, cell)
            except Exception as exc:
                errors.record('overview.%s' % build.__name__, exc)
            _draggable(window, cell, draw)
        _chooser(window, chooser, draw)

    draw()
    window.on_show['uebersicht'] = draw


# Die Kacheln in ihrer Reihenfolge: (Kennung, Textschlüssel).
TILES = (('spiel', 's_uv_spiel'), ('bauplaene', 's_uv_bauplaene'),
         ('uebersetzung', 's_uv_uebersetzung'), ('spielzeit', 's_uv_spielzeit'),
         ('rechner', 's_uv_rechner'), ('dienste', 's_uv_dienste'))
HIDDEN_SETTING = 'uebersicht_aus'


def visible_tiles():
    """Die eingeblendeten Kacheln. Ab Werk alle — außer der Spielzeit, wenn
    sie auch oben in der Kopfzeile aus ist (dort ist sie ab Werk aus)."""
    from . import paths
    hidden = (paths.settings() or {}).get(HIDDEN_SETTING)
    if not isinstance(hidden, list):
        hidden = [] if paths.setting_bool('spielzeit_zeigen', False) \
            else ['spielzeit']
    return [key for key, _label in TILES if key not in hidden]


ORDER_SETTING = 'uebersicht_reihenfolge'


def tile_order():
    """Alle Kachel-Kennungen in der gemerkten Reihenfolge; Unbekanntes fällt
    weg, Neues kommt hinten dazu."""
    from . import paths
    known = [key for key, _label in TILES]
    saved = (paths.settings() or {}).get(ORDER_SETTING)
    saved = [k for k in saved if k in known] if isinstance(saved, list) else []
    return saved + [k for k in known if k not in saved]


def swap(first, second):
    """Zwei Kacheln tauschen die Plätze — gemerkt in den Einstellungen."""
    from . import paths
    order = tile_order()
    if first not in order or second not in order or first == second:
        return order
    i, j = order.index(first), order.index(second)
    order[i], order[j] = order[j], order[i]
    paths.set_setting(ORDER_SETTING, order)
    return order


def _draggable(window, cell, redraw):
    """Die Kachel an ihrer Überschrift greifen und auf eine andere ziehen —
    die beiden tauschen die Plätze. Der Rand der Zielkachel leuchtet dabei."""
    head = getattr(cell, 'tile_head', None)
    if head is None:
        return
    state = {'target': None}

    def cell_under(event):
        widget = window.root.winfo_containing(event.x_root, event.y_root)
        while widget is not None and not hasattr(widget, 'tile_key'):
            widget = getattr(widget, 'master', None)
        return widget

    def mark(target):
        old = state['target']
        if old is target:
            return
        for c, color in ((old, theme.BG), (target, theme.ACCENT)):
            if c is not None and c is not cell:
                try:
                    c.configure(bg=color)
                except tk.TclError:
                    pass
        state['target'] = target

    def motion(event):
        mark(cell_under(event))

    def release(event):
        target = cell_under(event)
        mark(None)
        if target is not None and target is not cell:
            swap(cell.tile_key, target.tile_key)
            redraw()

    for part in [head] + list(head.winfo_children()):
        if getattr(part, 'tile_link', False):
            continue
        part.configure(cursor='fleur')
        part.bind('<B1-Motion>', motion)
        part.bind('<ButtonRelease-1>', release)


def set_visible(key, on):
    from . import paths
    hidden = [k for k, _l in TILES if k not in visible_tiles()]
    if on and key in hidden:
        hidden.remove(key)
    elif not on and key not in hidden:
        hidden.append(key)
    return paths.set_setting(HIDDEN_SETTING, hidden)


def _chooser(window, frame, redraw):
    """Die Kachelauswahl — zugeklappt ein Link, aufgeklappt ein Schalter je
    Kachel. Jede Änderung zeichnet die Übersicht sofort neu."""
    from .main_window import toggle_switch
    for child in frame.winfo_children():
        child.destroy()
    opened = getattr(frame, 'opened', False)
    link = tk.Label(frame, text=t('s_uv_anpassen_zu' if opened
                                  else 's_uv_anpassen'),
                    bg=theme.BG, fg=theme.ACCENT, font=window.f_small,
                    cursor='hand2', anchor='w')
    link.pack(fill='x')

    def flip_open(_e=None):
        frame.opened = not getattr(frame, 'opened', False)
        _chooser(window, frame, redraw)
    link.bind('<Button-1>', flip_open)
    if not opened:
        return
    current_tiles = visible_tiles()
    for key, label in TILES:
        row = tk.Frame(frame, bg=theme.BG)
        row.pack(fill='x', pady=(6, 0))

        def flip(key=key):
            on = key not in visible_tiles()
            set_visible(key, on)
            redraw()
            return on
        toggle_switch(row, key in current_tiles, flip).pack(side='right')
        tk.Label(row, text=t(label), bg=theme.BG, fg=theme.FG,
                 font=window.f_small, anchor='w').pack(side='left')


def _card(window, cell, title_key, target=None):
    """Eine Kachel mit Überschrift und — wenn `target` — einem Weg zur Seite."""
    from .main_window import round_frame
    card = round_frame(cell, theme.SURFACE, theme.LINE, radius=8,
                       base_color=theme.BG, stretch=True)
    # 2 px Rand: dort leuchtet die Zielkachel beim Ziehen (`_draggable`).
    card.holder.pack(fill='both', expand=True, padx=2, pady=2)
    head = tk.Frame(card, bg=theme.SURFACE)
    head.pack(fill='x', padx=16, pady=(12, 6))
    cell.tile_head = head
    tk.Label(head, text=t(title_key).upper(), bg=theme.SURFACE, fg=theme.SUB,
             font=window.f_small, anchor='w').pack(side='left')
    if target:
        go = tk.Label(head, text=t('s_uv_mehr'), bg=theme.SURFACE,
                      fg=theme.ACCENT, font=window.f_small, cursor='hand2')
        go.tile_link = True
        go.pack(side='right')
        go.bind('<Button-1>', lambda _e: window.open_page(target, via='sprung'))
    body = tk.Frame(card, bg=theme.SURFACE)
    body.pack(fill='both', expand=True, padx=16, pady=(0, 12))
    return body


def _big(window, parent, text, small=''):
    row = tk.Frame(parent, bg=theme.SURFACE)
    row.pack(fill='x')
    tk.Label(row, text=text, bg=theme.SURFACE, fg=theme.FG,
             font=window.f_title, anchor='w').pack(side='left')
    if small:
        tk.Label(row, text=small, bg=theme.SURFACE, fg=theme.SUB,
                 font=window.f_small, anchor='w').pack(side='left',
                                                       padx=(8, 0), pady=(4, 0))
    return row


def _line(window, parent, text, color=None):
    label = tk.Label(parent, text=text, bg=theme.SURFACE,
                     fg=color or theme.SUB, font=window.f_small, anchor='w',
                     justify='left')
    label.pack(fill='x', pady=(2, 0))
    from .pages import _wrap
    _wrap(label, inset=32)
    return label


def _ago(stamp):
    from .pages import _relative_time
    return _relative_time(stamp)


def _tile_game(window, cell):
    from . import auto_update, catalog, paths
    body = _card(window, cell, 's_uv_spiel', 'ordner')
    running = False
    try:
        running = auto_update.game_running()
    except Exception as exc:
        errors.record('overview.game_running', exc)
    _big(window, body, t('s_uv_laeuft') if running else t('s_uv_laeuft_nicht'))
    folder = paths.game_folder() or ''
    if folder:
        _line(window, body, t('s_uv_kanal') % os.path.basename(
            os.path.normpath(folder)))
    try:
        version = (catalog.load() or {}).get('version') or ''
    except Exception:
        version = ''
    if version:
        _line(window, body, t('s_uv_stand') % version)


def _tile_blueprints(window, cell):
    from . import catalog, collection
    from .main_window import round_bar
    body = _card(window, cell, 's_uv_bauplaene', 'fortschritt')
    numbers = blueprint_numbers(collection.load(), catalog.load())
    total = numbers['total'] or 1
    _big(window, body, str(numbers['mine']),
         t('s_uv_von') % (numbers['total'], 100.0 * numbers['mine'] / total))
    round_bar(body, 7, numbers['mine'] / float(total), theme.SURFACE,
              theme.HOVER, theme.ACCENT).pack(fill='x', pady=(6, 6))
    _line(window, body, t('s_uv_woche') % numbers['week'],
          theme.ACCENT if numbers['week'] else None)
    if numbers['newest']:
        _line(window, body, t('s_uv_neuester') % (numbers['newest'],
                                                  _ago(numbers['newest_at'])))


def _tile_translation(window, cell):
    """Je Kanal eine Auswahlliste der Textquellen — dieselben Wege wie auf
    der Übersetzungsseite. Eigene Adresse und das Unberührt-Lassen gibt es
    nur dort."""
    from . import paths, translation, usercfg
    from .main_window import round_select
    from .pages import _choose_source, _fetch_side, _settings_parts
    body = _card(window, cell, 's_uv_uebersetzung', 'uebersetzung')
    main_folder = os.path.normcase(os.path.normpath(paths.game_folder() or ''))
    channels = []
    try:
        channels = [(name, folder) for name, folder, present
                    in usercfg.installed_channels()
                    if present and name in translation.CHANNELS]
    except Exception as exc:
        errors.record('overview.channels', exc)
    main_name = next((name for name, folder in channels
                      if os.path.normcase(os.path.normpath(folder))
                      == main_folder), None)
    rows = [(main_name or os.path.basename(main_folder) or 'LIVE',
             paths.game_folder(), True)]
    rows += [(name, folder, False) for name, folder in channels
             if name != main_name]
    parts = _settings_parts(window)

    for name, folder, main in rows:
        channel = None if main else name
        chosen = (paths.setting('inj_quelle') or '') if main else \
            translation.channel_sources().get(name, '')
        entries = [(key, translation.display_name(key))
                   for group in translation.grouped_sources(channel)
                   for key in group]
        if chosen and chosen not in [k for k, _n in entries]:
            entries.append((chosen, translation.display_name(chosen)))
        if not chosen:
            entries.insert(0, ('', t('s_uv_keine_quelle')))
        row = tk.Frame(body, bg=theme.SURFACE)
        row.pack(fill='x', pady=(4, 0))
        tk.Label(row, text=name, bg=theme.SURFACE, fg=theme.FG,
                 font=window.f_bold, width=12, anchor='w').pack(side='left')
        state = tk.Label(body, text='', bg=theme.SURFACE, fg=theme.SUB,
                         font=window.f_small, anchor='w')

        class _Choice:
            def select(self, _key):
                pass

        def pick(key, main=main, name=name, folder=folder, state=state,
                 channel=channel):
            if not key:
                return
            if main:
                _choose_source(window, parts, _Choice(), key, lambda: None)
            else:
                _fetch_side(window, parts, name, folder, key, window.say)
            _status(state, key, channel)

        round_select(row, entries, chosen, pick, window.f_small,
                     bg=theme.SURFACE).pack(side='right')
        state.pack(fill='x')
        _status(state, chosen, channel)


def _status(label, source, channel):
    from . import translation
    try:
        text = translation.status_text(source, channel=channel) if source else ''
    except Exception:
        text = ''
    label.configure(text=text)


def _tile_playtime(window, cell):
    from . import playtime
    body = _card(window, cell, 's_uv_spielzeit', 'statistik')
    data = playtime.load()
    spans = playtime._merge_spans(
        [(e.get('von'), e.get('bis')) for e in data.get('sitzungen', [])
         if e.get('von') and e.get('bis')]
        + ([playtime._running_span()] if playtime._running_span() else []))
    _big(window, body, playtime.as_text(playtime.total(data)))
    _line(window, body, t('s_uv_sieben_tage') % playtime.as_text(
        playtime_week(spans)))


def _tile_machine(window, cell):
    from . import machine_info
    from .pages import _from_thread, _mix
    body = _card(window, cell, 's_uv_rechner', 'grafik')
    waiting = _line(window, body, t('s_uv_lese'))

    def show(data):
        waiting.destroy()
        if not data or data.get('machine_class') is None:
            _line(window, body, t('s_gr_klasse_leer'))
            return
        level = data['machine_class']
        _big(window, body, t('s_gr_stufe_%d' % level))
        bar = tk.Frame(body, bg=theme.SURFACE)
        bar.pack(fill='x', pady=(6, 4))
        for n in range(1, machine_info.CLASSES + 1):
            color = _mix(theme.RED, theme.ACCENT,
                         (n - 1) / float(machine_info.CLASSES - 1))
            tk.Frame(bar, bg=color if n <= level else theme.LINE,
                     height=6).pack(side='left', fill='x', expand=True,
                                    padx=(0 if n == 1 else 2, 0))
        if data.get('cpu_index') is not None:
            comma = current() == 'de'

            def number(value):
                text = '%.0f' % value
                return text.replace('.', ',') if comma else text
            _line(window, body, t('s_uv_index') % (number(data['cpu_index']),
                                                   number(data['gpu_index'])))

    def work():
        data = None
        try:
            data = machine_info.read()
        except Exception as exc:
            errors.record('overview.machine', exc)
        _from_thread(body, lambda: show(data))

    threading.Thread(target=work, daemon=True).start()


def _tile_services(window, cell):
    from . import basetool, serverstatus, updater
    from .pages import _from_thread
    body = _card(window, cell, 's_uv_dienste', 'serverstatus')
    cig = _line(window, body, '')

    def cig_text(state):
        overall = (state or {}).get('gesamt') or ''
        if not overall:
            return t('s_uv_cig_unbekannt'), None
        good = overall.lower() == 'operational'
        return (t('s_uv_cig') % overall,
                theme.ACCENT if good else theme.GOLD)

    text, color = cig_text(serverstatus.stored_state())
    cig.configure(text=text, fg=color or theme.SUB)

    def refresh():
        state = None
        try:
            state = serverstatus.state()
        except Exception as exc:
            errors.record('overview.serverstatus', exc)

        def apply():
            text, color = cig_text(state)
            cig.configure(text=text, fg=color or theme.SUB)
        _from_thread(cig, apply)

    threading.Thread(target=refresh, daemon=True).start()

    try:
        connected = basetool.CONNECTION.connected()
    except Exception:
        connected = False
    if connected:
        from . import basetool_sync
        last = _stamp(basetool_sync.STATUS.get('last_sync') or '')
        _line(window, body, t('s_uv_basetool') % _ago(last) if last
              else t('s_uv_basetool_neu'), theme.ACCENT)
    else:
        _line(window, body, t('s_uv_basetool_aus'))

    newer = None
    try:
        newer = updater.known_newer(window.version)
    except Exception as exc:
        errors.record('overview.update', exc)
    if newer:
        _line(window, body, t('s_uv_update_da') % newer.get('version'),
              theme.GOLD)
    else:
        _line(window, body, t('s_uv_update_aktuell') % window.version,
              theme.ACCENT)
