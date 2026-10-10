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
Rechner laut Spiel · Dienste (CIG, Basetool, Update) · Raffinerie (Restzeit
aus dem Scan, nur Windows) · Lager · Offene
Aufträge · Neu im Patch · Merkliste · Bedarf der Einheit (nur mit
Basetool) · Letzte Funde · Sicherung. Jede lässt sich unter
der Kachelliste ein- und ausblenden und an ihrer Überschrift auf eine andere
ziehen, um die Plätze zu tauschen; beides wird gemerkt.

Die Zahlen rechnen die reinen Funktionen oben (`blueprint_numbers`,
`playtime_week`, `storage_top`, `open_contracts`, `patch_numbers`,
`watch_numbers`, `latest_finds`) — ohne Tk, damit sie sich prüfen lassen.
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


def storage_top(posts, most=3, piece=None):
    """Die größten Rohstoffe im Lager: `[(name, menge, ort)]`, absteigend.

    Die Menge ist die Summe über alle Orte und Güten; `ort` ist der Ort, an
    dem davon am meisten liegt. `piece` (`name -> bool`) stellt gezählte
    Rohstoffe hinter die gemessenen — Stück und SCU lassen sich nicht
    gegeneinander aufwiegen."""
    piece = piece or (lambda _name: False)
    totals, places, names = {}, {}, {}
    for post in posts or ():
        name = (post.get('material') or '').strip()
        if not name:
            continue
        key = name.lower()
        amount = post.get('menge') or 0
        names.setdefault(key, name)
        totals[key] = totals.get(key, 0) + amount
        place = (post.get('ort') or '').strip()
        by_place = places.setdefault(key, {})
        by_place[place] = by_place.get(place, 0) + amount
    order = sorted(totals, key=lambda k: (bool(piece(names[k])), -totals[k],
                                          names[k].lower()))
    result = []
    for key in order[:most]:
        place = max(places[key].items(), key=lambda kv: kv[1])[0]
        result.append((names[key], totals[key], place))
    return result


def open_contracts(entries, check, owned):
    """Offene Aufträge, die Baupläne geben: `[(name, gesamt, fehlend)]`, die
    mit den meisten fehlenden zuerst. `check` ist `contracts.check`, `owned`
    eine Funktion `name -> bool`. Jeder Titel zählt einmal."""
    from . import mission_log
    seen, result = set(), []
    for entry in entries or ():
        if (entry.get('zustand') or mission_log.RUNNING) != mission_log.RUNNING:
            continue
        name = (entry.get('name') or '').strip()
        if not name or name in seen:
            continue
        seen.add(name)
        found = check(name, owned)
        if found:
            total, missing = found
            result.append((name, total, len(missing)))
    result.sort(key=lambda r: (-r[2], r[0].lower()))
    return result


def patch_numbers(catalog_data, stock):
    """`(neu, davon_eigen, version)` für die zuletzt geholte Spielversion."""
    from . import catalog
    new = catalog.new_ones(catalog_data)
    own = (stock or {}).get('bauplaene') or {}
    version = catalog.version_short((catalog_data or {}).get('version') or '')
    return len(new), sum(1 for key in new if key in own), version


def watch_numbers(data, stock):
    """`(gefunden, gesamt, [offene Titel])` der Merkliste. Ein Name gilt als
    gefunden, sobald er im Bestand steht; ein Muster-Eintrag verschwindet
    beim Fund und ist deshalb immer offen."""
    from . import paths
    own = (stock or {}).get('bauplaene') or {}
    names = list((data or {}).get('namen') or ())
    patterns = list((data or {}).get('eintraege') or ())
    found = [n for n in names if paths.name_key(n) in own]
    waiting = [n for n in sorted(names) if paths.name_key(n) not in own]
    waiting += [e.get('titel') or '?' for e in patterns]
    return len(found), len(names) + len(patterns), waiting


# Herkünfte, die kein Fund im Spiel sind.
NOT_FOUND_IN_GAME = ('basetool', 'start', 'import', 'launcher')


def latest_finds(stock, most=3):
    """Die zuletzt im Spiel gefundenen Baupläne: `[(name, zeitpunkt)]`."""
    finds = []
    for key, entry in ((stock or {}).get('bauplaene') or {}).items():
        entry = entry or {}
        if entry.get('quelle') in NOT_FOUND_IN_GAME:
            continue
        at = _stamp(entry.get('zeit') or '')
        if at is not None:
            finds.append((entry.get('name') or key, at))
    finds.sort(key=lambda f: -f[1])
    return finds[:most]


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
                          'dienste': _tile_services,
                          'raffinerie': _tile_refinery, 'lager': _tile_storage,
                          'auftraege': _tile_contracts,
                          'neu_patch': _tile_patch, 'merkliste': _tile_watchlist,
                          'bedarf': _tile_demand, 'funde': _tile_finds,
                          'sicherung': _tile_backup}
        visible = [key for key in visible_tiles() if available(key)]
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
         ('rechner', 's_uv_rechner'), ('dienste', 's_uv_dienste'),
         ('raffinerie', 's_uv_raffinerie'), ('lager', 's_uv_lager'), ('auftraege', 's_uv_auftraege'),
         ('neu_patch', 's_uv_neu_patch'), ('merkliste', 's_uv_merkliste'),
         ('bedarf', 's_uv_bedarf'), ('funde', 's_uv_funde'),
         ('sicherung', 's_uv_sicherung'))
HIDDEN_SETTING = 'uebersicht_aus'


def available(key):
    """Gibt es die Kachel gerade? Der Bedarf der Einheit nur, solange es
    seinen Reiter gibt (Basetool verbunden, Bereich an, Recht erteilt); die
    Raffinerie nur, wo das Terminal gelesen werden kann."""
    if key == 'raffinerie':
        from . import refinery_scan
        return refinery_scan.supported()
    if key != 'bedarf':
        return True
    try:
        from . import exchange_demand
        return bool(exchange_demand.visible())
    except Exception:
        return False


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
    # Zwei Spalten wie die Kacheln darüber, Schalter rechts am Namen.
    table = tk.Frame(frame, bg=theme.BG)
    table.pack(fill='x', pady=(4, 0))
    table.columnconfigure(0, weight=1, uniform='auswahl')
    table.columnconfigure(1, weight=1, uniform='auswahl')
    keys = [(key, label) for key, label in TILES if available(key)]
    for index, (key, label) in enumerate(keys):
        row = tk.Frame(table, bg=theme.BG)
        row.grid(row=index // 2, column=index % 2, sticky='ew',
                 padx=(14, 2) if index % 2 else (2, 14), pady=(6, 0))

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
    checks = []

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
        row_name = tk.Label(row, text=name, bg=theme.SURFACE, fg=theme.FG,
                            font=window.f_bold, anchor='w')
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

        # Erst die Auswahl, dann der Name: Tk kürzt bei Platzmangel das
        # zuletzt gepackte Element, und das darf nur der kurze Name sein.
        select = round_select(row, entries, chosen, pick, window.f_small,
                              bg=theme.SURFACE)
        select.pack(side='right')
        select.tile_select = True
        row_name.pack(side='left')
        state.pack(fill='x')
        _status(state, chosen, channel)
        if chosen and chosen != 'original':
            checks.append((chosen, channel, state, pick))

    if checks:
        _check_link(window, body, checks)


def _check_link(window, body, checks):
    """Link zum Nachsehen: fragt für jeden Kanal mit gewählter Quelle nach
    einer neueren Fassung und holt sie gleich. Die Abfrage läuft im
    Hintergrund; geholt wird über denselben Weg wie bei der Auswahl."""
    from . import translation
    from .pages import _from_thread
    link = tk.Label(body, text=t('s_uv_nachsehen'), bg=theme.SURFACE,
                    fg=theme.ACCENT, font=window.f_small, cursor='hand2',
                    anchor='w')
    link.pack(fill='x', pady=(8, 0))
    busy = [False]

    def done(results):
        busy[0] = False
        link.configure(text=t('s_uv_nachsehen'), fg=theme.ACCENT)
        fetched = False
        for (source, channel, state, pick), newer in zip(checks, results):
            if newer:
                pick(source)
                fetched = True
            else:
                _status(state, source, channel)
        if not fetched:
            window.say(t('inj_aktuell'))

    def work():
        results = []
        for source, channel, _state, _pick in checks:
            newer = False
            try:
                newer = translation.update_available(source, channel)[0]
            except Exception as exc:
                errors.record('overview.translation_check', exc)
            results.append(newer)
        _from_thread(link, lambda: done(results))

    def start(_e=None):
        if busy[0]:
            return
        busy[0] = True
        link.configure(text=t('s_uv_sehe_nach'), fg=theme.SUB)
        threading.Thread(target=work, daemon=True).start()
    link.bind('<Button-1>', start)
    link.check = start


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


def _amount(name, value):
    """Menge mit Einheit — Stück oder SCU, Dezimalzeichen je Sprache."""
    from . import crafting, materials
    piece = False
    try:
        piece = crafting.is_piece(name)
    except Exception:
        pass
    text = materials.amount_text(value, piece=piece,
                                 decimal=',' if current() == 'de' else '.')
    return text, t('s_lg_stueck') if piece else 'SCU'


def countdown_text(seconds):
    """Restzeit wie im Spiel: `1h 3m`, `43m 57s`, `2d 4h`."""
    seconds = max(0, int(seconds))
    days, rest = divmod(seconds, 86400)
    hours, rest = divmod(rest, 3600)
    minutes, secs = divmod(rest, 60)
    if days:
        return '%dd %dh' % (days, hours)
    if hours:
        return '%dh %dm' % (hours, minutes)
    if minutes:
        return '%dm %ds' % (minutes, secs)
    return '%ds' % secs


# Takt, in dem die Restzeit auf der Kachel nachgezogen wird.
TICK_MS = 1000


def _tile_refinery(window, cell):
    from . import paths, refinery_jobs, refinery_scan
    from .hotkey import DEFAULT_SCAN
    body = _card(window, cell, 's_uv_raffinerie', 'lager')
    running, done = refinery_jobs.current()
    if not running and not done:
        keys = paths.setting('hotkey_scan') or DEFAULT_SCAN
        _line(window, body, t('s_uv_raff_leer') % keys)
        return
    _big(window, body, str(len(running)),
         t('s_uv_raff_zahl') % (len(running), len(done)))
    rows = []
    for job in running[:3] + done[:max(0, 3 - len(running))]:
        label = refinery_scan.job_label(job)
        name = _line(window, body, label, theme.FG)
        state = _line(window, body, '')
        rows.append((job['ends'], state))
        name.pack_configure(pady=(6, 0))

    def tick():
        now = time.time()
        for ends, label in rows:
            try:
                if not label.winfo_exists():
                    return
                if ends > now:
                    label.configure(
                        text=t('s_uv_raff_noch') % (
                            countdown_text(ends - now),
                            time.strftime('%H:%M', time.localtime(ends))),
                        fg=theme.SUB)
                else:
                    label.configure(text=t('s_uv_raff_fertig'),
                                    fg=theme.ACCENT)
            except tk.TclError:
                return
        if any(ends > now for ends, _l in rows):
            try:
                body.after(TICK_MS, tick)
            except tk.TclError:
                pass
    tick()


def _tile_storage(window, cell):
    from . import materials, trade_cargo
    body = _card(window, cell, 's_uv_lager', 'lager')
    posts = materials.load()
    goods = trade_cargo.load()
    if not posts and not goods:
        _line(window, body, t('s_uv_lager_leer'))
        return
    kinds = len({(p.get('material') or '').strip().lower() for p in posts
                 if (p.get('material') or '').strip()})
    wares = len({(g.get('ware') or '').strip().lower() for g in goods
                 if (g.get('ware') or '').strip()})
    _big(window, body, str(kinds + wares))
    _line(window, body, t('s_uv_lager_posten') % (kinds, wares))
    from . import crafting
    for name, amount, place in storage_top(posts, piece=crafting.is_piece):
        number, unit = _amount(name, amount)
        text = t('s_uv_menge') % (name, number, unit)
        if place:
            text += ' · ' + place
        _line(window, body, text, theme.FG)


def _tile_contracts(window, cell):
    from . import collection, contracts, mission_log
    from .pages import _from_thread
    body = _card(window, cell, 's_uv_auftraege', 'auftragslog')
    waiting = _line(window, body, t('s_uv_lese'))

    def show(found):
        waiting.destroy()
        if not found:
            _line(window, body, t('s_uv_auftraege_leer'))
            return
        _big(window, body, str(len(found)))
        _line(window, body, t('s_uv_auftraege_zahl') % len(found))
        for name, total, missing in found[:3]:
            if missing:
                _line(window, body, t('s_uv_auftrag_fehlt')
                      % (name, missing, total), theme.ACCENT)
            else:
                _line(window, body, t('s_uv_auftrag_alle') % (name, total))

    def work():
        found = []
        try:
            own = collection.load().get('bauplaene') or {}
            found = open_contracts(
                mission_log.load(), contracts.check,
                lambda n: collection.norm(n) in own)
        except Exception as exc:
            errors.record('overview.contracts', exc)
        _from_thread(body, lambda: show(found))

    threading.Thread(target=work, daemon=True).start()


def _tile_patch(window, cell):
    from . import catalog, collection
    body = _card(window, cell, 's_uv_neu_patch', 'liste')
    new, owned, version = patch_numbers(catalog.load(), collection.load())
    if not new:
        _line(window, body, t('s_uv_neu_patch_leer') % (version or '—'))
        return
    _big(window, body, str(new), t('s_uv_neu_patch_zahl') % version)
    _line(window, body, t('s_uv_neu_patch_hast') % owned,
          theme.ACCENT if owned else None)


def _tile_watchlist(window, cell):
    from . import collection, watchlist
    from .main_window import round_bar
    body = _card(window, cell, 's_uv_merkliste', 'fortschritt')
    found, total, waiting = watch_numbers(watchlist.load(), collection.load())
    if not total:
        _line(window, body, t('s_uv_merkliste_leer'))
        return
    _big(window, body, str(found), t('s_uv_merkliste_zahl') % (found, total))
    round_bar(body, 7, found / float(total), theme.SURFACE, theme.HOVER,
              theme.ACCENT).pack(fill='x', pady=(6, 6))
    for title in waiting[:3]:
        _line(window, body, title, theme.FG)


def _tile_demand(window, cell):
    from . import exchange_demand, materials
    body = _card(window, cell, 's_uv_bedarf', 'bedarf')
    data = exchange_demand.current()
    if not data:
        _line(window, body, t('s_uv_bedarf_leer'))
        return
    wanted = data.get('materials') or []
    items = data.get('items') or []
    complete = 0
    for entry in wanted:
        try:
            have, _low = materials.amount_with_quality(
                entry['name'], entry.get('min_quality') or 0)
        except Exception:
            have = 0
        if have and have >= (entry.get('amount') or 0):
            complete += 1
    _big(window, body, str(len(wanted) + len(items)))
    _line(window, body, t('s_uv_bedarf_zahl') % (len(wanted), len(items)))
    _line(window, body, t('s_uv_bedarf_da') % complete,
          theme.ACCENT if complete else None)


def _tile_finds(window, cell):
    from . import collection
    body = _card(window, cell, 's_uv_funde', 'liste')
    finds = latest_finds(collection.load())
    if not finds:
        _line(window, body, t('s_uv_funde_leer'))
        return
    for name, at in finds:
        _line(window, body, t('s_uv_fund') % (name, _ago(at)), theme.FG)


def _tile_backup(window, cell):
    from . import backup, file_picker
    body = _card(window, cell, 's_uv_sicherung', 'bestand')
    last = _stamp(backup.last_written())
    if last:
        _big(window, body, _ago(last))
        _line(window, body, t('s_uv_sicherung_zuletzt') % time.strftime(
            '%d.%m.%Y %H:%M' if current() == 'de' else '%Y-%m-%d %H:%M',
            time.localtime(last)))
    else:
        _line(window, body, t('s_uv_sicherung_nie'), theme.GOLD)
    now = tk.Label(body, text=t('s_uv_sicherung_jetzt'), bg=theme.SURFACE,
                   fg=theme.ACCENT, font=window.f_small, cursor='hand2',
                   anchor='w')
    now.pack(fill='x', pady=(6, 0))

    def write(_e=None):
        try:
            window._backup_write(file_picker, backup)
        except Exception as exc:
            errors.record('overview.backup', exc)
        if window.on_show.get('uebersicht'):
            window.on_show['uebersicht']()
    now.bind('<Button-1>', write)
