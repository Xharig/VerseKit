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


# Welcher der beiden unteren Bereiche oben steht: 'kacheln' oder 'karten'.
ORDER_SECTIONS_SETTING = 'startseite_zuerst'
# Mindestbreite einer Kachel bei Schriftstufe 0.
TILE_MIN_WIDTH = 300
TILE_PAD = 12


def sections_first():
    """'kacheln' (Auf einen Blick zuerst) oder 'karten' (Deine Karten)."""
    from . import paths
    return 'karten' if paths.setting(ORDER_SECTIONS_SETTING) == 'karten' \
        else 'kacheln'


def _section_title(window, parent, text):
    label = tk.Label(parent, text=text, bg=theme.BG, fg=theme.SUB,
                     font=window.f_bold, anchor='w')
    label.pack(fill='x', pady=(4, 8))
    return label


def overview_page(window, frame):
    """Die Startseite. Kartenansicht: Hauptbereiche als Karten (fest oben),
    darunter Auf einen Blick (Kacheln) und Deine Karten (angeheftete
    Unterpunkte) — die beiden in wählbarer Reihenfolge. Reiteransicht: nur
    die beiden unteren Bereiche, die Hauptbereiche stehen links."""
    from . import cards
    from .pages import _heading, _scroll_area
    before = set(frame.winfo_children())
    _heading(window, frame, t('hf_uebersicht'), t('s_uv_lead'))
    title = [w for w in frame.winfo_children() if w not in before][0]
    parent = _scroll_area(frame)
    chooser = tk.Frame(parent, bg=theme.BG)
    karten = bool(getattr(window, 'card_mode', lambda: False)())

    sections = []
    main_grid = None
    if karten:
        main_section = tk.Frame(parent, bg=theme.BG)
        main_section.pack(fill='x')
        _section_title(window, main_section, t('k_bereich_versekit'))
        main_grid = cards.CardGrid(main_section,
                                   cards.scaled(window, cards.MAIN_MIN_WIDTH))
        main_grid.frame.pack(fill='x', pady=(0, 8))
        frame.main_grid = main_grid
        sections.append(main_section)

    def draw_main():
        if main_grid is None:
            return
        main_grid.clear()
        for kennung, symbol, name_key, _pages in cards.visible_areas():
            def build(cell, kennung=kennung, symbol=symbol, name_key=name_key):
                cards.card(window, cell, symbol, t(name_key),
                           t('ka_' + kennung),
                           lambda: window.open_page(
                               cards.area_page_id(kennung), via='karte'))
            cell = main_grid.add(build)
            cell.area_key = kennung

    tiles_section = tk.Frame(parent, bg=theme.BG)
    _section_title(window, tiles_section, t('k_bereich_blick'))
    grid = cards.CardGrid(tiles_section, cards.scaled(window, TILE_MIN_WIDTH))
    grid.frame.pack(fill='x', pady=(0, 8))
    pinned_section = tk.Frame(parent, bg=theme.BG)
    _section_title(window, pinned_section, t('k_bereich_deine'))
    pin_grid = cards.CardGrid(pinned_section,
                              cards.scaled(window, cards.SUB_MIN_WIDTH))
    pin_hint = tk.Label(pinned_section, text=t('k_deine_leer'), bg=theme.BG,
                        fg=theme.SUB, font=window.f_small, anchor='w',
                        justify='left')
    def reorder():
        lower = [tiles_section, pinned_section]
        if sections_first() == 'karten':
            lower.reverse()
        for section in lower:
            section.pack_forget()
        for section in lower:
            section.pack(fill='x', pady=(8, 0))
        if getattr(chooser, 'opened', False):
            chooser.pack_forget()
            chooser.pack(fill='x', pady=(0, 12),
                         before=sections[0] if karten else lower[0])
        frame.section_order = ['kacheln' if s is tiles_section else 'karten'
                               for s in lower]

    reorder()
    frame.tiles_grid = grid
    frame.pin_grid = pin_grid
    link = _chooser_link(window, frame, title, chooser,
                         sections[0] if karten else tiles_section, parent,
                         lambda: redraw(), reorder)

    def draw():
        draw_main()
        grid.clear()
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
        for key in shown:
            build = builders_by_id[key]

            def tile(cell, build=build):
                try:
                    build(window, cell)
                except Exception as exc:
                    errors.record('overview.%s' % build.__name__, exc)
            cell = grid.add(tile)
            cell.tile_key = key
            _draggable(window, cell, redraw)
        _draw_pinned(window, pin_grid, pin_hint, redraw)
        _chooser(window, chooser, redraw, reorder)

    def redraw():
        # Neu zeichnen, ohne dass die Rollstelle verloren geht: Beim Abbau
        # aller Kacheln schrumpft der Inhalt kurz, und die Leinwand bliebe
        # sonst auf einer leeren Stelle stehen.
        from .pages import _keep_scroll
        _keep_scroll(grid.frame, draw)

    draw()
    window.on_show['uebersicht'] = redraw
    if _REOPEN[0]:
        _REOPEN[0] = False
        link.flip()


def _draw_pinned(window, pin_grid, hint, redraw):
    """Deine Karten: die angehefteten Unterpunkte, sonst ein Hinweis."""
    from . import cards
    pin_grid.clear()
    pages = cards.pinned()
    if not pages:
        pin_grid.frame.pack_forget()
        hint.pack(fill='x', pady=(0, 12))
        return
    hint.pack_forget()
    pin_grid.frame.pack(fill='x', pady=(0, 8))
    for page in pages:
        def build(cell, page=page):
            def unpin():
                cards.set_pinned(page, False)
                cell.after_idle(redraw)
                return False
            cards.card(window, cell, cards.PAGES[page][0],
                       cards.page_title(page),
                       t(cards.description_key(page)),
                       lambda: window.open_page(page, via='karte'),
                       pin=(True, unpin))
        cell = pin_grid.add(build)
        cell.page_key = page
        _draggable(window, cell, redraw, 'page_key', cards.swap_pinned)


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


def _draggable(window, cell, redraw, key_attr='tile_key', swap_keys=None):
    """Die Kachel an ihrer Überschrift greifen und auf eine andere ziehen —
    die beiden tauschen die Plätze. Der Rand der Zielkachel leuchtet dabei.

    `key_attr` ist das Merkmal, an dem eine Zelle ihre Kennung trägt;
    `swap_keys(a, b)` tauscht und merkt sich das (Standard: `swap`, die
    Kacheln). Für Karten (`cards.card`) wird deren Rand eingefärbt."""
    head = getattr(cell, 'tile_head', None)
    if head is None:
        return
    swap_keys = swap_keys or swap
    state = {'target': None}

    def cell_under(event):
        widget = window.root.winfo_containing(event.x_root, event.y_root)
        while widget is not None and not hasattr(widget, key_attr):
            widget = getattr(widget, 'master', None)
        return widget

    def paint(c, on):
        box = getattr(c, 'card', None)
        try:
            if box is not None:
                box.canvas.itemconfigure(
                    box.shape, outline=theme.ACCENT if on else theme.LINE)
            else:
                c.configure(bg=theme.ACCENT if on else theme.BG)
        except tk.TclError:
            pass

    def mark(target):
        old = state['target']
        if old is target:
            return
        for c, on in ((old, False), (target, True)):
            if c is not None and c is not cell:
                paint(c, on)
        state['target'] = target

    def motion(event):
        mark(cell_under(event))

    def release(event):
        target = cell_under(event)
        mark(None)
        if target is not None and target is not cell:
            swap_keys(getattr(cell, key_attr), getattr(target, key_attr))
            redraw()

    for part in [head] + list(head.winfo_children()):
        if getattr(part, 'tile_link', False):
            continue
        part.configure(cursor='fleur')
        part.bind('<B1-Motion>', motion, add='+')
        part.bind('<ButtonRelease-1>', release, add='+')


def set_visible(key, on):
    from . import paths
    hidden = [k for k, _l in TILES if k not in visible_tiles()]
    if on and key in hidden:
        hidden.remove(key)
    elif not on and key not in hidden:
        hidden.append(key)
    return paths.set_setting(HIDDEN_SETTING, hidden)


def _chooser_link(window, frame, title, chooser, grid, area, redraw,
                  reorder=None):
    """Der Link rechts neben der Überschrift — außerhalb der Rollfläche, also
    auch auf kleinen Bildschirmen immer zu sehen. Er klappt die Auswahl über
    den Kacheln auf und zu und rollt dabei nach oben."""
    link = tk.Label(frame, text=t('s_uv_anpassen'), bg=theme.BG,
                    fg=theme.ACCENT, font=window.f_small, cursor='hand2')
    link.place(in_=title, relx=1.0, rely=0.5, anchor='e')
    link.lift()
    frame.chooser_link = link

    def flip(_e=None):
        chooser.opened = not getattr(chooser, 'opened', False)
        link.configure(text=t('s_uv_anpassen_zu' if chooser.opened
                              else 's_uv_anpassen'))
        _chooser(window, chooser, redraw, reorder)
        if chooser.opened:
            chooser.pack(fill='x', pady=(0, 12), before=grid)
        else:
            chooser.pack_forget()
        canvas = getattr(area, 'canvas', None)
        if canvas is not None:
            canvas.yview_moveto(0)
    link.bind('<Button-1>', flip)
    link.flip = flip
    return link


# Nach einem Neuaufbau des Fensters (Modul umgeschaltet) die Auswahl gleich
# wieder aufklappen.
_REOPEN = [False]


def _switch_table(window, frame, entries):
    """Zwei Spalten mit je Name links und Schalter rechts.
    `entries`: (Text, an?, umschalten) — `umschalten()` gibt den neuen Wert."""
    from .main_window import toggle_switch
    table = tk.Frame(frame, bg=theme.BG)
    table.pack(fill='x', pady=(0, 6))
    table.columnconfigure(0, weight=1, uniform='auswahl')
    table.columnconfigure(1, weight=1, uniform='auswahl')
    for index, (text, on, flip) in enumerate(entries):
        row = tk.Frame(table, bg=theme.BG)
        row.grid(row=index // 2, column=index % 2, sticky='ew',
                 padx=(14, 2) if index % 2 else (2, 14), pady=(6, 0))
        toggle_switch(row, on, flip).pack(side='right')
        tk.Label(row, text=text, bg=theme.BG, fg=theme.FG,
                 font=window.f_small, anchor='w').pack(side='left')
    return table


def _chooser(window, frame, redraw, reorder=None):
    """Startseite anpassen, über den Bereichen, nur solange `frame.opened`
    gilt (umgeschaltet über den Link neben der Überschrift, `_chooser_link`).

    Kartenansicht: zuerst die Hauptbereiche (dieselben Schalter wie unter
    Einstellungen → Module, dazu Basetool), dann die Kacheln, dann welcher
    der beiden unteren Bereiche oben steht. Reiteransicht: Kacheln und
    Reihenfolge. Jede Änderung zeigt sich sofort."""
    from . import cards, modules, paths
    from .main_window import round_select
    for child in frame.winfo_children():
        child.destroy()
    if not getattr(frame, 'opened', False):
        return
    karten = bool(getattr(window, 'card_mode', lambda: False)())

    def heading(key):
        tk.Label(frame, text=t(key), bg=theme.BG, fg=theme.SUB,
                 font=window.f_bold, anchor='w').pack(fill='x', pady=(8, 0))

    if karten:
        heading('k_anpassen_bereiche')
        entries = []
        for group in modules.SWITCHABLE:
            def flip(group=group):
                new_value = not modules.enabled(group)
                modules.set_enabled(group, new_value)
                _REOPEN[0] = True
                # Die Reiterliste wird beim Aufbau zusammengesetzt — erst
                # der Neuaufbau nimmt die Gruppe dort weg oder zeigt sie.
                window.root.after(60, window.rebuild)
                return new_value
            entries.append((t(modules.LABELS[group]), modules.enabled(group),
                            flip))

        def flip_basetool():
            new_value = not cards.area_enabled('basetool')
            paths.set_setting(cards.BASETOOL_SETTING, new_value)
            redraw()
            return new_value
        entries.append((t('k_basetool_titel'), cards.area_enabled('basetool'),
                        flip_basetool))
        _switch_table(window, frame, entries)
        heading('k_anpassen_kacheln')

    current_tiles = visible_tiles()
    entries = []
    for key, label in TILES:
        if not available(key):
            continue

        def flip(key=key):
            on = key not in visible_tiles()
            set_visible(key, on)
            redraw()
            return on
        entries.append((t(label), key in current_tiles, flip))
    _switch_table(window, frame, entries)

    row = tk.Frame(frame, bg=theme.BG)
    row.pack(fill='x', pady=(10, 0))
    tk.Label(row, text=t('k_anpassen_zuerst'), bg=theme.BG, fg=theme.FG,
             font=window.f_small, anchor='w').pack(side='left', padx=(2, 0))

    def pick(value):
        paths.set_setting(ORDER_SECTIONS_SETTING, value)
        if reorder:
            reorder()
    round_select(row, [('kacheln', t('k_bereich_blick')),
                       ('karten', t('k_bereich_deine'))],
                 sections_first(), pick, window.f_small,
                 bg=theme.BG).pack(side='right', padx=(0, 14))


def _card(window, cell, title_key, target=None, extra=None):
    """Eine Kachel mit Überschrift und — wenn `target` — einem Weg zur Seite.
    `extra` ist ein Paar `(Text, Aktion)` für einen zweiten Knopf links
    daneben."""
    from .main_window import round_button, round_frame
    card = round_frame(cell, theme.SURFACE, theme.LINE, radius=12,
                       base_color=theme.BG, stretch=True)
    # 2 px Rand: dort leuchtet die Zielkachel beim Ziehen (`_draggable`).
    card.holder.pack(fill='both', expand=True, padx=2, pady=2)
    head = tk.Frame(card, bg=theme.SURFACE)
    # 12 px Innenabstand: Der Inhalt rückt schon um die Rundung (12 px) ein,
    # zusammen 24 px bis zum Rand. Mehr Rand schneidet bei großer Schrift die
    # Auswahlfelder der Übersetzungs-Kachel ab.
    head.pack(fill='x', padx=TILE_PAD, pady=(12, 6))
    cell.tile_head = head
    tk.Label(head, text=t(title_key).upper(), bg=theme.SURFACE, fg=theme.SUB,
             font=window.f_small, anchor='w').pack(side='left')
    if target:
        go = round_button(head, t('s_uv_mehr'),
                          lambda: window.open_page(target, via='sprung'),
                          window.f_small, theme.SURFACE, theme.SURFACE,
                          theme.ACCENT, theme.ACCENT, radius=8,
                          padding=(12, 3))
        go.tile_link = True
        go.pack(side='right')
    if extra:
        more = round_button(head, extra[0], extra[1], window.f_small,
                            theme.SURFACE, theme.SURFACE, theme.ACCENT,
                            theme.ACCENT, radius=8, padding=(12, 3))
        more.tile_link = True
        more.pack(side='right', padx=(0, 6))
        cell.extra_button = more
    body = tk.Frame(card, bg=theme.SURFACE)
    body.pack(fill='both', expand=True, padx=TILE_PAD, pady=(0, 12))
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


def translation_rows():
    """Die Kanäle mit ihrer gewählten Textquelle: (Name, Ordner, Haupt?,
    Kanal, Quelle). Der erste ist die Hauptinstallation."""
    from . import paths, translation, usercfg
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
    result = []
    for name, folder, main in rows:
        channel = None if main else name
        chosen = (paths.setting('inj_quelle') or '') if main else \
            translation.channel_sources().get(name, '')
        result.append((name, folder, main, channel, chosen))
    return result


def translation_pick(window, parts, name, folder, main, key):
    """Eine Textquelle für einen Kanal setzen und holen — derselbe Weg wie
    auf der Übersetzungsseite."""
    from .pages import _choose_source, _fetch_side
    if not key:
        return

    class _Choice:
        def select(self, _key):
            pass
    if main:
        _choose_source(window, parts, _Choice(), key, lambda: None)
    else:
        _fetch_side(window, parts, name, folder, key, window.say)


def translation_reload(window):
    """Jede gewählte Übersetzung noch einmal holen und einsetzen — etwa wenn
    ein Spiel-Patch die Textdatei überschrieben hat."""
    from .pages import _settings_parts
    parts = _settings_parts(window)
    done = False
    for name, folder, main, _channel, chosen in translation_rows():
        if chosen:
            translation_pick(window, parts, name, folder, main, chosen)
            done = True
    if not done:
        window.say(t('s_tq_nichts_zu_pruefen'))
    return done


def translation_check(window, widget, checks=None, started=None,
                      finished=None):
    """Für jeden Kanal mit gewählter Quelle nach einer neueren Fassung fragen
    und sie gleich holen. Die Abfrage läuft im Hintergrund; `checks` sind
    (Quelle, Kanal, Statuszeile oder None, holen), ohne Angabe alle Kanäle.
    `started()`/`finished()` melden Anfang und Ende."""
    from . import translation
    from .pages import _from_thread, _settings_parts
    if checks is None:
        parts = _settings_parts(window)
        checks = [(chosen, channel, None,
                   lambda key, n=name, f=folder, m=main:
                   translation_pick(window, parts, n, f, m, key))
                  for name, folder, main, channel, chosen in translation_rows()
                  if chosen and chosen != 'original']
    if not checks:
        window.say(t('s_tq_nichts_zu_pruefen'))
        return False

    def done(results):
        if finished:
            finished()
        fetched = False
        for (source, channel, state, pick), newer in zip(checks, results):
            if newer:
                pick(source)
                fetched = True
            elif state is not None:
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
        _from_thread(widget, lambda: done(results))

    if started:
        started()
    window.say(t('inj_laeuft'))
    threading.Thread(target=work, daemon=True).start()
    return True


def _tile_translation(window, cell):
    """Je Kanal eine Auswahlliste der Textquellen — dieselben Wege wie auf
    der Übersetzungsseite. Eigene Adresse und das Unberührt-Lassen gibt es
    nur dort."""
    from . import translation
    from .main_window import round_select
    from .pages import _settings_parts
    body = _card(window, cell, 's_uv_uebersetzung', 'uebersetzung')
    parts = _settings_parts(window)
    checks = []

    for name, folder, main, channel, chosen in translation_rows():
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

        def pick(key, main=main, name=name, folder=folder, state=state,
                 channel=channel):
            if not key:
                return
            translation_pick(window, parts, name, folder, main, key)
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
    """Link zum Nachsehen (`translation_check`) unter den Kanälen."""
    link = tk.Label(body, text=t('s_uv_nachsehen'), bg=theme.SURFACE,
                    fg=theme.ACCENT, font=window.f_small, cursor='hand2',
                    anchor='w')
    link.pack(fill='x', pady=(8, 0))
    busy = [False]

    def started():
        busy[0] = True
        link.configure(text=t('s_uv_sehe_nach'), fg=theme.SUB)

    def finished():
        busy[0] = False
        link.configure(text=t('s_uv_nachsehen'), fg=theme.ACCENT)

    def start(_e=None):
        if busy[0]:
            return
        translation_check(window, link, checks, started, finished)
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


def refinery_scan_now(window):
    """Lager öffnen, Raffinerie-Block aufklappen und das Lesen starten — es
    liest, sobald Star Citizen vorn ist."""
    window.open_page('lager', via='sprung')
    read = getattr(window, 'refinery_read', None)
    if read is not None:
        read()


def _tile_refinery(window, cell):
    from . import paths, refinery_jobs, refinery_scan
    from .hotkey import DEFAULT_SCAN
    def scan():
        refinery_scan_now(window)
    cell.scan = scan
    body = _card(window, cell, 's_uv_raffinerie', 'lager',
                 extra=(t('s_uv_scannen'), scan))
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
