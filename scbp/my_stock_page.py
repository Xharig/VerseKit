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
Die Seite Mein Lager (`meinlager`) — das eigene Lager an einer Stelle.

Die Daten liegen weiter in zwei Dateien: Rohstoffe zum Herstellen in
`rohstoffe.json` (`materials`), Ware zum Verkaufen in `handelslager.json`
(`trade_cargo`). Die Seite zeigt beide in einer Liste:

* Reiter **Material** — alle Rohstoff-Posten und die Handelsware,
* Reiter **Items** — Gegenstände (Handelslager-Posten mit `stueck`).

Darunter filtern Art, Ort und Zweck (Herstellen = Rohstofflager, Verkaufen =
Handelslager), dazu die Suche. Ein Posten ist eine Zeile; mehrere Posten
derselben Ware stehen unter einer Sammelzeile, die aufklappt.

Oben die Klappbereiche zum Eintragen (Formular je Zweck, Raffinerie-Ausbeute),
unten Sichern und Löschen. Alles geht ohne Basetool; mit eingeschaltetem
Abgleich gleicht der Knopf an der Überschrift beide Dateien ab.

⚠ Alte Seitennamen (`lager`, `handelslager`) führen über `ALIASES` hierher —
gespeicherte Karten, Sprünge, die Raffinerie-Taste im Spiel.
"""
import json
import os
import time
import tkinter as tk

from .language import t
from .pages import (BG, SURFACE, FG, SUB, ACCENT, LINE, RED, GOLD, _heading,
                    _scroll_area, _body_text, _button, _wrap, _stripe,
                    _stripe_reset, _choice, _combo_box, _checkbox,
                    _storage_fold, _refinery_box, _keep_scroll, _money, _auec,
                    _amount_text, after_typing, sync_targets)
from . import icons, materials, notice, paths, stock_kinds, theme
from . import stock_split, trade_cargo

PAGE = 'meinlager'
# Alte Seitennamen -> diese Seite, mit der Vorwahl, die sie mitbringen.
ALIASES = {'lager': PAGE, 'handelslager': PAGE}
PRESETS = {'handelslager': {'purpose': 'sale', 'view': 'material'}}

VIEW_MATERIAL = 'material'
VIEW_ITEMS = 'items'
PURPOSE_CRAFT = 'craft'
PURPOSE_SALE = 'sale'

SETTING_VIEW = 'meinlager_sicht'
SETTING_FORM_OPEN = 'meinlager_eintragen_offen'
SETTING_FORM_KIND = 'meinlager_eintragen_zweck'
SETTING_BACKUP_OPEN = 'meinlager_sichern_offen'
SETTING_MOVED_SEEN = 'lager_umzug_gesehen'

STORE_RAW = 'raw'
STORE_TRADE = 'trade'
# Das dritte Formular: Items, gespeichert im Handelslager mit `stueck`.
FORM_ITEM = 'item'

# Grund aus `trade_cargo.add/change` -> Textschlüssel der Meldung.
TRADE_REASONS = {'ware': 's_hl_fehlt_ware', 'menge': 's_hl_fehlt_menge',
                 'preis': 's_hl_fehlt_preis'}


def builders():
    return {PAGE: my_stock_page}


def resolve(page_id):
    """`(Seite, Vorwahl)` — alte Namen werden zu `meinlager`."""
    return ALIASES.get(page_id, page_id), PRESETS.get(page_id)


# ----------------------------------------------------------------- Daten
def _number(value):
    try:
        return float(value or 0)
    except (TypeError, ValueError):
        return 0.0


def _is_piece(name):
    try:
        from . import crafting
        return crafting.is_piece(name)
    except Exception:
        return False


def entries():
    """Alle Posten beider Dateien in einer Form:

    `{store, index, name, amount, piece, quality, place, stolen, cost, item,
    leftover, lookup}`

    `index` ist die Stelle in der jeweiligen Datei — über sie wird
    bearbeitet und gelöscht. `lookup` ist der Schlüssel, unter dem der
    Abgleich das Material auflöst (`stock_split.row_key`)."""
    rows = []
    for index, entry in enumerate(materials.load()):
        name = (entry.get('material') or '').strip()
        quality = entry.get('qualitaet')
        rows.append({'store': STORE_RAW, 'index': index, 'name': name or '?',
                     'amount': _number(entry.get('menge')),
                     'piece': _is_piece(name),
                     'quality': (int(round(_number(quality)))
                                 if quality not in (None, '') else None),
                     'place': (entry.get('ort') or '').strip(),
                     'stolen': bool(entry.get('gestohlen')),
                     'cost': None, 'item': False,
                     'leftover': materials.is_leftover(entry),
                     'lookup': name})
    from . import exchange_stock
    for index, entry in enumerate(trade_cargo.load()):
        name = (entry.get('ware') or '').strip()
        piece = trade_cargo.is_pieces(entry)
        rows.append({'store': STORE_TRADE, 'index': index,
                     'name': name or '?',
                     'lookup': exchange_stock.trade_lookup(entry),
                     'amount': _number(entry.get('menge')), 'piece': piece,
                     'quality': None,
                     'place': (entry.get('ort') or '').strip(),
                     'stolen': bool(entry.get('gestohlen')),
                     'cost': trade_cargo.cost_of(entry), 'item': piece,
                     'leftover': False})
    return rows


def kind_of(row, server, cache):
    """Die Art eines Postens, je Name einmal bestimmt."""
    key = (row['item'], row['store'], row['name'].lower())
    if key not in cache:
        if row['item']:
            cache[key] = stock_kinds.item_kind(row['name'])
        else:
            cache[key] = stock_kinds.material_kind(
                row['name'], server, for_sale=row['store'] == STORE_TRADE)
    return cache[key]


def visible_rows(rows, state, server, cache):
    """Die Posten, die Reiter, Filter und Suche durchlassen."""
    text = (state['search'].get() if state.get('search') else '')
    text = (text or '').strip().lower()
    items = state['view'] == VIEW_ITEMS
    found = []
    for row in rows:
        if row['item'] != items:
            continue
        if state['kind'] and kind_of(row, server, cache) != state['kind']:
            continue
        if state['place'] and row['place'].lower() != state['place'].lower():
            continue
        if not items and state['purpose'] == PURPOSE_CRAFT \
                and row['store'] != STORE_RAW:
            continue
        if not items and state['purpose'] == PURPOSE_SALE \
                and row['store'] != STORE_TRADE:
            continue
        if text and text not in row['name'].lower() \
                and text not in row['place'].lower():
            continue
        found.append(row)
    return stock_split.scoped(found, state.get('scope') or
                              stock_split.SCOPE_ALL)


def amount_text(row_or_value, piece, unit=True):
    from .language import current
    value = row_or_value if isinstance(row_or_value, (int, float)) \
        else row_or_value['amount']
    number = materials.amount_text(value, piece,
                                   ',' if current() == 'de' else '.')
    if not unit:
        return number
    return '%s %s' % (number, t('s_lg_stueck') if piece else 'SCU')


def is_old_user():
    """Hat dieser Spieler Rohstoff- oder Handelslager schon benutzt? Dann
    zeigt die Seite einmal, wo beides jetzt liegt."""
    if paths.setting_bool(SETTING_MOVED_SEEN, False):
        return False
    try:
        if materials.load() or trade_cargo.load():
            return True
        from . import news
        return news.was_seen('lager') or news.was_seen('handelslager')
    except Exception:
        return False


# ----------------------------------------------------------------- Seite
def my_stock_page(window, frame):
    from .main_window import round_entry
    _heading(window, frame, t('hf_meinlager'), t('s_ml_lead'))
    inner = _scroll_area(frame)
    view = paths.setting(SETTING_VIEW) or VIEW_MATERIAL
    page = {
        'window': window, 'inner': inner,
        'state': {'view': view if view in (VIEW_MATERIAL, VIEW_ITEMS)
                  else VIEW_MATERIAL,
                  'kind': '', 'place': '', 'purpose': '',
                  'scope': stock_split.SCOPE_ALL,
                  'search': tk.StringVar(master=inner, value=''),
                  'sort': 'name', 'descending': False},
        'open': set(), 'editing': None, 'forms': {},
        'split': None, 'share_form': None, 'busy': False,
    }

    if is_old_user():
        _moved_notice(window, inner)
    else:
        # Wer beim ersten Öffnen nichts Altes hat, bekommt den Hinweis auch
        # später nicht — sonst brächte ihn der erste Abgleich.
        paths.set_setting(SETTING_MOVED_SEEN, True)

    message = tk.Label(inner, text='', bg=BG, fg=SUB, font=window.f_small,
                       anchor='w', justify='left')
    page['message'] = message

    def say(text, color=SUB):
        message.configure(text=text, fg=color)
    page['say'] = say

    # Feste Teile oben: Eintragen, Raffinerie.
    form_body, form_open = _storage_fold(window, inner, 's_lg_hand_titel',
                                         SETTING_FORM_OPEN, False)
    page['form_open'] = form_open
    _forms(window, form_body, page)
    _refinery_box(window, inner, materials, page['forms']['raw']['place'],
                  lambda: page['redraw'](), message)
    message.pack(fill='x', pady=(4, 0))
    _wrap(message)

    top = tk.Frame(inner, bg=BG)
    top.pack(fill='x', pady=(4, 0))
    controls = tk.Frame(top, bg=BG)
    controls.pack(fill='x')
    box = round_entry(top, page['state']['search'], window.f_small,
                      theme.FIELD, LINE, ACCENT, FG,
                      placeholder=t('s_ml_suche'))
    box.holder.pack(fill='x', pady=(8, 0))
    page['search_box'] = box
    listing = tk.Frame(inner, bg=BG)
    listing.pack(fill='x', pady=(8, 0))
    page['listing'] = listing

    def redraw():
        try:
            if not listing.winfo_exists():
                return
        except tk.TclError:
            return
        drop_stale_edit(page)
        # Persönlich/gemeinsam nur mit Verbindung; ohne gilt alles als
        # persönlich und die Sicht steht auf Alle.
        snap = stock_split.snapshot()
        page['split'] = snap
        if snap is None:
            page['state']['scope'] = stock_split.SCOPE_ALL
            page['share_form'] = None
        rows = stock_split.attach(entries(), snap)
        _check_share_form(page, rows)
        server = stock_kinds.server_kinds()
        cache = {}
        for child in controls.winfo_children():
            child.destroy()
        _draw_controls(window, controls, page, rows, server, cache)
        for child in listing.winfo_children():
            child.destroy()
        _draw_list(window, listing, page, rows, server, cache)
    page['redraw'] = redraw
    page['state']['search'].trace_add('write', after_typing(inner, redraw))

    _backup_section(window, inner, page)
    sync_targets(window)[PAGE] = (message, redraw)

    def split_changed():
        # Neue Aufteilung beim Basetool: nur die Liste neu, die Seite bleibt.
        try:
            if not listing.winfo_exists():
                return False
        except tk.TclError:
            return False
        _keep_scroll(listing, redraw)
        return True
    from . import basetool_sync
    basetool_sync.SPLIT_LISTENERS.append(split_changed)

    def outside():
        """Was der Wächter nicht über die eigenen Dateien sieht: ob es eine
        Aufteilung gibt, mit welcher Installation abgeglichen wurde und die
        Einheiten mit ihren Namen (Marken in der Sicht „Gemeinsam") — das
        steht nur im Speicher."""
        from . import basetool_sync, exchange_orgunits
        return (stock_split.available(),
                basetool_sync.STATUS.get('installation_id'),
                exchange_orgunits.fetched(),
                repr(exchange_orgunits.units()))

    # Beim Wiederkommen nur neu zeichnen, wenn sich etwas geändert hat
    # (`freshness.Guard`). Eine Vorgabe von außen (Sprung aus einer anderen
    # Seite) zeichnet immer neu.
    from . import freshness
    guard = freshness.Guard(extra=outside)

    def on_show():
        preset = getattr(window, 'my_stock_preset', None)
        window.my_stock_preset = None
        if preset:
            page['state'].update(preset)
        elif guard.fresh():
            return
        guard.draw(redraw)
    window.on_show = getattr(window, 'on_show', {})
    window.on_show[PAGE] = on_show
    page['apply_preset'] = on_show
    window.my_stock_page = page
    on_show()


def _moved_notice(window, parent):
    """Die Karte für alle, die Rohstoff- und Handelslager kennen — einmal,
    mit X weg."""
    card = tk.Frame(parent, bg=SURFACE)
    card.pack(fill='x', pady=(10, 0))
    head = tk.Frame(card, bg=SURFACE)
    head.pack(fill='x', padx=14, pady=(10, 0))

    def close():
        paths.set_setting(SETTING_MOVED_SEEN, True)
        try:
            card.destroy()
        except tk.TclError:
            pass
    cross = icons.line(head, 'schliessen', close, background=SURFACE,
                       font=window.f_small)
    cross.pack(side='right')
    notice.attach(cross, lambda: t('s_ml_umzug_weg'))
    tk.Label(head, text=t('s_ml_umzug_titel'), bg=SURFACE, fg=ACCENT,
             font=window.f_bold, anchor='w').pack(side='left')
    text = tk.Label(card, text=t('s_ml_umzug'), bg=SURFACE, fg=FG,
                    font=window.f_small, anchor='w', justify='left')
    text.pack(fill='x', padx=14, pady=(4, 12))
    _wrap(text, inset=28)
    card.moved_notice = True
    return card


# ---------------------------------------------------------------- Filter
def _draw_controls(window, parent, page, rows, server, cache):
    from .main_window import round_select
    state = page['state']
    # Zähler, Arten und Orte folgen der Sicht Alle/Persönlich/Gemeinsam.
    rows = stock_split.scoped(rows, state['scope'])
    counts = {VIEW_MATERIAL: 0, VIEW_ITEMS: 0}
    for row in rows:
        counts[VIEW_ITEMS if row['item'] else VIEW_MATERIAL] += 1

    def choose_view(view):
        if view == state['view']:
            return
        state['view'] = view
        state['kind'] = ''
        paths.set_setting(SETTING_VIEW, view)
        page['redraw']()
    tabs = _choice(window, parent, [
        (VIEW_MATERIAL, '%s (%d)' % (t('s_ml_material'),
                                     counts[VIEW_MATERIAL])),
        (VIEW_ITEMS, '%s (%d)' % (t('s_ml_items'), counts[VIEW_ITEMS]))],
        state['view'], choose_view)
    tabs.pack(fill='x')
    page['tabs'] = tabs

    items = state['view'] == VIEW_ITEMS
    here = [r for r in rows if r['item'] == items]
    kinds = stock_kinds.ITEM_KINDS if items else stock_kinds.MATERIAL_KINDS
    present = {kind_of(r, server, cache) for r in here}
    if state['kind'] and state['kind'] not in present:
        state['kind'] = ''
    places = sorted({r['place'] for r in here if r['place']}, key=str.lower)
    if state['place'] and state['place'] not in places:
        state['place'] = ''

    line = tk.Frame(parent, bg=BG)
    line.pack(fill='x', pady=(8, 0))
    page['filters'] = {}

    def pick(key):
        def run(value):
            # `''` ist bei jedem Feld die Grundstellung — beim Umschalter
            # Alle, damit er nicht wie ein gesetzter Filter aussieht.
            state[key] = (value or stock_split.SCOPE_ALL) if key == 'scope' \
                else value
            page['redraw']()
        return run
    selects = []
    # Alle / Persönlich / Gemeinsam nur mit Verbindung (`stock_split`).
    if page.get('split') is not None:
        selects.append(('scope', [
            ('', t('s_ml_sicht_alle')),
            (stock_split.SCOPE_PERSONAL, t('s_ml_sicht_persoenlich')),
            (stock_split.SCOPE_SHARED, t('s_ml_sicht_gemeinsam'))]))
    selects.append(('kind', [('', t('s_ml_alle_arten'))]
                    + [(k, stock_kinds.label(k)) for k in kinds
                       if k in present]))
    selects.append(('place', [('', t('s_ml_alle_orte'))]
                    + [(p, p) for p in places]))
    if not items:
        selects.append(('purpose', [('', t('s_ml_alle_zwecke')),
                                    (PURPOSE_CRAFT, t('s_ml_herstellen')),
                                    (PURPOSE_SALE, t('s_ml_verkaufen'))]))
    for key, choices in selects:
        current = state[key]
        if key == 'scope' and current == stock_split.SCOPE_ALL:
            current = ''
        select = round_select(line, choices, current, pick(key),
                              window.f_small)
        select.pack(side='left', padx=(0, 8))
        page['filters'][key] = select


# ----------------------------------------------------------------- Liste
def _sort_key(state, rows):
    first = rows[0]
    by = state['sort']
    if by == 'amount':
        return sum(r['amount'] for r in rows)
    if by == 'quality':
        return max((r['quality'] or 0) for r in rows)
    if by == 'place':
        return ' '.join(sorted(r['place'].lower() for r in rows))
    return first['name'].lower()


def _draw_list(window, listing, page, rows, server, cache):
    state = page['state']
    _stripe_reset(listing)
    shown = visible_rows(rows, state, server, cache)
    page['shown'] = shown
    if not rows:
        _body_text(listing, t('s_ml_leer'), window.f_small, fill='x')
        return
    if not shown and state['view'] == VIEW_ITEMS \
            and not any(r['item'] for r in rows):
        _body_text(listing, t('s_ml_keine_items'), window.f_small, fill='x')
        return
    if not shown:
        _body_text(listing, t('s_lg_nichts_da'), window.f_small, fill='x')
        return
    items = state['view'] == VIEW_ITEMS
    selling = not items and state['purpose'] == PURPOSE_SALE
    groups = {}
    # Je Name und Einheit: Hadanite in Stück (Rohstoff) und in SCU (Ware)
    # sind zwei Gruppen — eine Summe aus beidem hätte keine Einheit.
    for row in shown:
        groups.setdefault((row['name'].lower(), row['piece']),
                          []).append(row)
    order = sorted(groups.values(), key=lambda g: _sort_key(state, g),
                   reverse=state['descending'])
    if not items:
        # Stückware (Edelsteine) als eigener Block oben, wie im Spiel.
        order.sort(key=lambda g: materials.piece_first(g[0]['name']))

    table = tk.Frame(listing, bg=BG)
    table.pack(fill='x')
    table.grid_columnconfigure(0, weight=1)
    columns = [('name', 's_ml_sp_bestand', 'w'), ('place', 's_ml_sp_ort', 'w')]
    # Verkaufsware hat keine Qualität — mit PURPOSE_SALE fällt die Spalte
    # weg.
    if not items and not selling:
        columns.append(('quality', 's_ml_sp_q', 'e'))
    columns.append(('amount', 's_ml_sp_menge', 'e'))
    if selling:
        columns += [('price', 's_hl_sp_je_scu', 'e'),
                    ('total', 's_hl_sp_gesamt', 'e'),
                    ('profit', 's_hl_sp_gewinn', 'e')]
    columns.append(('action', None, 'e'))
    page['columns'] = [c[0] for c in columns]

    def sort_by(key):
        if state['sort'] == key:
            state['descending'] = not state['descending']
        else:
            state['sort'], state['descending'] = key, False
        _keep_scroll(listing, page['redraw'])

    for col, (key, text_key, anchor) in enumerate(columns):
        text = t(text_key) if text_key else ''
        sortable = key in ('name', 'place', 'quality', 'amount')
        active = sortable and state['sort'] == key
        if active:
            text += ' ▾' if state['descending'] else ' ▴'
        head = tk.Label(table, text=text.upper() if text_key else '', bg=BG,
                        fg=ACCENT if active else SUB, font=window.f_small,
                        anchor=anchor, padx=8,
                        cursor='hand2' if sortable else '')
        head.grid(row=0, column=col, sticky='ew', pady=(0, 2))
        if sortable:
            head.bind('<Button-1>', lambda _e, k=key: sort_by(k))

    from . import selling as prices
    builder = {'row': 1, 'table': table, 'columns': page['columns'],
               'server': server, 'cache': cache, 'prices': prices,
               'selling': selling, 'items': items,
               'sale_badge': not items and not selling}
    page['row_widgets'] = []
    page['builder'] = builder
    page['window'] = window
    page['folds'] = {}
    page['slots'] = {}
    text = (state['search'].get() or '').strip().lower()
    for group in order:
        if len(group) == 1:
            _post_row(window, page, builder, group[0], nested=False)
            continue
        key = (items, group[0]['name'].lower())
        if group[0]['piece'] and not items:
            key += ('stueck',)
        editing = page.get('editing')
        auto = (editing is not None and any(
            (r['store'], r['index']) == editing for r in group)) or (
            text and any(text in r['place'].lower() for r in group))
        is_open = key in page['open'] or bool(auto)
        _group_row(window, page, builder, group, key, is_open)
        if is_open:
            _fold_rows(window, page, page['folds'][key])

    # Items im Handelslager sind keine Ware: keine Preise, keine Summen.
    trade_rows = [r for r in shown
                  if r['store'] == STORE_TRADE and not r['item']]
    if trade_rows:
        total = sum(prices.best_price(r['name']) * r['amount']
                    for r in trade_rows)
        if total:
            tk.Label(listing, text=t('s_hl_gesamt').format(
                summe=_auec(total)), bg=BG, fg=ACCENT, font=window.f_small,
                anchor='e').pack(fill='x', pady=(8, 0))
        profit = _profit(trade_rows, prices.best_price)
        if profit is not None:
            tk.Label(listing, text=t('s_hl_gewinn_gesamt').format(
                summe=('+' if profit > 0 else '') + _auec(profit)),
                bg=BG, fg=ACCENT if profit >= 0 else RED,
                font=window.f_small, anchor='e').pack(fill='x')
        if total or profit is not None:
            tk.Label(listing, text=t('s_hl_gesamt_hinweis'), bg=BG, fg=SUB,
                     font=window.f_small, anchor='e').pack(fill='x')


def _profit(rows, price_of):
    total, known = 0.0, False
    for row in rows:
        price = price_of(row['name'])
        if row['cost'] is None or not price:
            continue
        total += (price - row['cost']) * row['amount']
        known = True
    return total if known else None


def table_cells(window, builder, base, values):
    """Eine Zeile der Tabelle: `values` je Spalte `(Inhalt, Farbe)`; ein
    Inhalt ist Text oder eine Funktion, die in einen Rahmen baut."""
    row = builder['row']
    builder['row'] += 1
    widgets = []
    for col, key in enumerate(builder['columns']):
        content, color = values.get(key, ('', SUB))
        anchor = 'w' if key in ('name', 'place') else 'e'
        if callable(content):
            cell = tk.Frame(builder['table'], bg=base)
            content(cell)
        else:
            cell = tk.Label(builder['table'], text=content, bg=base, fg=color,
                            font=window.f_small if key != 'name'
                            else window.f_base, anchor=anchor, padx=8)
        cell.grid(row=row, column=col, sticky='nsew', ipady=3)
        widgets.append(cell)
    return widgets


def bind_row(widgets, action):
    for widget in widgets:
        stack = [widget]
        while stack:
            current = stack.pop()
            if getattr(current, 'is_action', False):
                continue
            try:
                current.configure(cursor='hand2')
            except tk.TclError:
                pass
            current.bind('<Button-1>', lambda _e: action())
            stack.extend(current.winfo_children())


def name_cell(window, base, name, badges, tag=None, indent=False, extra='',
              color=FG):
    """Name, Kürzel und Abzeichen in einer Zelle; `extra` steht klein und
    blass dahinter (Notiz, Reservierung)."""
    from .main_window import badge

    def build(cell):
        if indent:
            tk.Frame(cell, bg=base, width=28).pack(side='left')
        if name:
            tk.Label(cell, text=name, bg=base, fg=color,
                     font=window.f_small if indent else window.f_base,
                     anchor='w', padx=0 if indent else 8).pack(side='left')
        if tag:
            tk.Label(cell, text=tag, bg=base, fg=SUB, font=window.f_small,
                     anchor='w').pack(side='left', padx=(2, 0))
        for text, badge_color in badges:
            badge(cell, text, badge_color, window.f_small, bg=base).pack(
                side='left', padx=(6, 0))
        if extra:
            tk.Label(cell, text=extra, bg=base, fg=SUB, font=window.f_small,
                     anchor='w').pack(side='left', padx=(8, 0))
    return build


def mining_text(name):
    """Womit man den Rohstoff abbaut — Hand · Fahrzeug · Schiff — oder leer,
    wenn die Bergbaudaten ihn nicht kennen."""
    try:
        from . import mining
        kinds = mining.mining_kinds(name)
    except Exception:
        return ''
    return ' · '.join(t(key) for kind, key in (
        ('fps', 's_lg_abbau_fps'), ('fahrzeug', 's_lg_abbau_fahrzeug'),
        ('schiff', 's_lg_abbau_schiff')) if kind in kinds)


def _post_badges(row, kind, sale_badge, nested):
    """Art (nicht in aufgeklappten Zeilen), Verkauf (nur wo Herstellen und
    Verkaufen gemischt stehen), gestohlen, Rest."""
    badges = []
    if not nested:
        badges.append((stock_kinds.label(kind), SUB))
    if sale_badge and row['store'] == STORE_TRADE:
        badges.append((t('s_ml_verkauf'), ACCENT))
    if row['stolen']:
        badges.append((t('s_hl_marke'), GOLD))
    if row['leftover']:
        badges.append((t('s_ml_rest'), GOLD))
    return badges


def _split_badges(row, scope):
    """In Alle: wie viel davon im Lager der Einheit liegt. In Gemeinsam:
    in welcher Einheit."""
    if not row.get('shared') or row.get('lot') is None:
        return []
    if scope == stock_split.SCOPE_SHARED:
        _personal, pools = stock_split.split_of(row['lot'])
        return [(stock_split.unit_label(unit), ACCENT) for unit, _a in pools]
    if scope == stock_split.SCOPE_ALL:
        return [(t('s_ml_b_geteilt') % amount_text(row['shared'],
                                                    row['piece']), ACCENT)]
    return []


def _post_row(window, page, builder, row, nested):
    base = _stripe(page['listing'], BG)
    editing = page.get('editing') == (row['store'], row['index'])
    if editing:
        base = theme.HOVER
    items = builder['items']
    kind = kind_of(row, builder['server'], builder['cache'])
    tag = stock_kinds.tag(row['name']) if items and not nested else None
    badges = _post_badges(row, kind, builder['sale_badge'], nested) \
        + _split_badges(row, page['state']['scope'])
    values = {
        'name': (name_cell(window, base, '' if nested else row['name'],
                           badges, tag, indent=nested,
                           extra='' if nested or items
                           else mining_text(row['name'])), FG),
        'place': (row['place'] or '—', SUB),
        'quality': ('%d' % row['quality'] if row['quality'] is not None
                    else '—', SUB),
        'amount': (amount_text(row, row['piece']),
                   GOLD if row['leftover'] else ACCENT),
        'action': (_actions(window, page, row, base), SUB),
    }
    if builder['selling']:
        price = builder['prices'].best_price(row['name'])
        value = price * row['amount']
        profit = ((price - row['cost']) * row['amount']
                  if price and row['cost'] is not None else None)
        values['price'] = (_money(price) if price else '—', SUB)
        values['total'] = (_money(value) if value else '—', FG)
        values['profit'] = ('—', SUB) if profit is None else (
            ('+' if profit > 0 else '') + _money(profit),
            ACCENT if profit > 0 else RED if profit < 0 else SUB)
    widgets = table_cells(window, builder, base, values)
    # Eine freie Tabellenzeile direkt darunter für das Teilen-Feld
    # (`_place_share_form`).
    slot = builder['row']
    builder['row'] += 1
    page.setdefault('slots', {})[(row['store'], row['index'])] = slot
    if whole_row(page, row):
        bind_row(widgets, lambda: edit(page, row))
    page['row_widgets'].append(('post', row, widgets))
    form = page.get('share_form')
    if form and form['row'] == (row['store'], row['index']):
        _share_form(window, page, dict(builder, row=slot), row, form)


def whole_row(page, row):
    """Zeigt die Zeile ihre ganze Menge? In Alle immer; in Persönlich nur,
    wenn nichts davon geteilt ist; in Gemeinsam nie. Nur dann gelten
    Bearbeiten und Löschen der Zeile."""
    scope = page['state']['scope']
    if scope == stock_split.SCOPE_SHARED:
        return False
    if scope == stock_split.SCOPE_PERSONAL:
        return not row.get('shared')
    return True


def share_actions(page, row):
    """Welche Teilen-Knöpfe die Zeile bekommt: `share`, wenn ihr Posten
    beim Basetool einen persönlichen Teil hat und es eine Einheit gibt;
    `unshare`, wenn davon etwas in einem Pool liegt."""
    lot = row.get('lot')
    # Ohne `split` am Posten kennt das Basetool auch die beiden Ops nicht —
    # dann keine Knöpfe.
    if page.get('split') is None or lot is None \
            or not isinstance(lot.get('split'), dict):
        return []
    from . import exchange_orgunits
    personal, pools = stock_split.split_of(lot)
    out = []
    if personal > 0 and exchange_orgunits.units() \
            and page['state']['scope'] != stock_split.SCOPE_SHARED:
        out.append(stock_split.SHARE)
    if pools and page['state']['scope'] != stock_split.SCOPE_PERSONAL:
        out.append(stock_split.UNSHARE)
    return out


def _actions(window, page, row, base):
    def build(cell):
        cell.action_icons = {}
        # In Gemeinsam und Persönlich steht ein Anteil als Menge — Bearbeiten
        # und Löschen gälten aber der ganzen Zeile. Liegt etwas davon im
        # Lager der Einheit, dort nur Teilen und Zurücknehmen.
        shared_view = not whole_row(page, row)
        icons_here = [] if shared_view else [
            ('loeschen', lambda _e=None: remove(page, row), 's_lg_weg'),
            ('bearbeiten', lambda _e=None: edit(page, row),
             's_ml_bearbeiten')]
        for kind in reversed(share_actions(page, row)):
            icons_here.append((
                'teilen' if kind == stock_split.SHARE else 'zuruecknehmen',
                lambda _e=None, k=kind: open_share(page, row, k),
                's_ml_teilen' if kind == stock_split.SHARE
                else 's_ml_zuruecknehmen'))
        for position, (name, run, text_key) in enumerate(icons_here):
            icon = icons.tappable(cell, name, run, background=base,
                                  font=window.f_small)
            icon.is_action = True
            icon.pack(side='right', padx=(4, 8) if position == 0 else (4, 0))
            notice.attach(icon, lambda k=text_key: t(k))
            cell.action_icons[name] = icon
    return build


def _check_share_form(page, rows):
    """Das offene Teilen-Formular gilt dem Posten, unter dem es geöffnet
    wurde. Steht an seiner Stelle inzwischen eine andere Zeile (gelöscht,
    verbraucht, Ort geändert) oder hat sie keinen Posten beim Basetool mehr,
    geht es zu — sonst baute es die Op aus einem fremden Posten."""
    form = page.get('share_form')
    if not form:
        return
    here = next((r for r in rows
                 if (r['store'], r['index']) == form['row']), None)
    lot = here.get('lot') if here else None
    if lot is None or lot.get('key') != form.get('lot_key') \
            or here['name'] != form.get('name'):
        page['share_form'] = None


def open_share(page, row, kind):
    """Das kleine Formular unter der Zeile öffnen (oder schließen, wenn es
    dort schon offen ist)."""
    current = page.get('share_form')
    here = (row['store'], row['index'])
    if current and current['row'] == here and current['kind'] == kind:
        page['share_form'] = None
    else:
        page['share_form'] = {'row': here, 'kind': kind, 'unit': None,
                              'amount': None, 'name': row['name'],
                              'lot_key': (row.get('lot') or {}).get('key')}
    _place_share_form(page)


def _place_share_form(page):
    """Das Teilen-Feld nur an seiner Stelle unter der Zeile bauen oder
    abbauen, die Liste bleibt stehen. Steht die Zeile gerade nicht in der
    Liste, wird neu gezeichnet."""
    holder = page.get('share_holder')
    page['share_holder'] = None
    try:
        if holder is not None and holder.winfo_exists():
            holder.destroy()
    except tk.TclError:
        pass
    form = page.get('share_form')
    if form is None:
        return
    slot = page.get('slots', {}).get(form['row'])
    target = next((what for kind, what, _w in page.get('row_widgets', [])
                   if kind == 'post'
                   and (what['store'], what['index']) == form['row']), None)
    if slot is None or target is None or 'builder' not in page:
        _keep_scroll(page['listing'], page['redraw'])
        return
    _share_form(page['window'], page, dict(page['builder'], row=slot),
                target, form)


def share_buttons(window, buttons, page, store, index):
    """Im Bearbeiten-Feld dieselben Teilen-Knöpfe wie an der Zeile — nur die,
    die `share_actions` dort auch anbietet. Ein Klick öffnet das Teilen-Feld
    unter der Zeile und rollt dorthin."""
    row = next((r for r in page.get('shown') or []
                if (r['store'], r['index']) == (store, index)), None)
    if row is None or not whole_row(page, row):
        return
    for kind in share_actions(page, row):
        def run(k=kind):
            # Ist das Feld schon offen, nur hinrollen — `open_share` schlösse
            # es.
            current = page.get('share_form') or {}
            if current.get('row') != (store, index) or current.get('kind') != k:
                open_share(page, row, k)
            _bring_into_view(page, page.get('share_holder'))
        _button(window, buttons, t('s_ml_form_teilen' if kind ==
                                   stock_split.SHARE else 's_ml_form_zurueck'),
                run).pack(side='left', padx=(8, 0))


def _bring_into_view(page, widget):
    """Die Seite so rollen, dass `widget` oben im sichtbaren Teil steht."""
    canvas = getattr(page.get('inner'), 'canvas', None)
    if canvas is None or widget is None:
        return
    try:
        widget.update_idletasks()
        inner = page['inner']
        height = max(1, inner.winfo_height())
        top = widget.winfo_rooty() - inner.winfo_rooty() - 40
        canvas.yview_moveto(max(0.0, top / height))
    except tk.TclError:
        pass


def _share_form(window, page, builder, row, form):
    """Ins Lager der Einheit geben oder zurücknehmen: Einheit, Menge (ab
    Werk alles), Knöpfe. Höchstens, was der Posten beim Basetool persönlich
    bzw. im gewählten Pool hält."""
    from .main_window import round_entry, round_select
    from . import exchange_orgunits
    lot = row['lot']
    sharing = form['kind'] == stock_split.SHARE
    personal, pools = stock_split.split_of(lot)
    piece = (lot.get('quantity') or {}).get('unit') == stock_split.PIECE
    if sharing:
        choices = [(u['id'], exchange_orgunits.label(u))
                   for u in exchange_orgunits.units()]
    else:
        choices = [(unit, stock_split.unit_label(unit)) for unit, _a in pools]
    if not choices:
        page['share_form'] = None
        return
    if form['unit'] not in {c[0] for c in choices}:
        form['unit'] = choices[0][0]

    def limit():
        if sharing:
            return personal
        return next((a for u, a in pools if u == form['unit']), 0.0)
    if form['amount'] is None:
        # Am Seitenrahmen, nicht an der Tabelle: Die wird bei jedem
        # Neuzeichnen neu gebaut, die Eingabe soll stehen bleiben.
        form['amount'] = tk.StringVar(master=page['inner'],
                                      value=amount_text(limit(), piece,
                                                        unit=False))
    holder = tk.Frame(builder['table'], bg=SURFACE)
    holder.grid(row=builder['row'], column=0,
                columnspan=len(builder['columns']), sticky='ew',
                pady=(0, 6))
    builder['row'] += 1
    page['share_holder'] = holder
    tk.Label(holder, text=t('s_ml_teilen_t' if sharing else 's_ml_zurueck_t')
             % row['name'], bg=SURFACE, fg=ACCENT, font=window.f_bold,
             anchor='w').pack(fill='x', padx=12, pady=(8, 4))
    line = tk.Frame(holder, bg=SURFACE)
    line.pack(fill='x', padx=12)

    def pick(unit):
        form['unit'] = unit
        form['amount'].set(amount_text(limit(), piece, unit=False))
        hint.configure(text=t('s_ml_hoechstens') % amount_text(limit(), piece))
    select = round_select(line, choices, form['unit'], pick, window.f_small)
    select.pack(side='left')
    box = round_entry(line, form['amount'], window.f_small, theme.FIELD, LINE,
                      ACCENT, FG, placeholder=t('s_ml_menge_ph'))
    box.holder.pack(side='left', padx=(8, 0))
    hint = tk.Label(line, text=t('s_ml_hoechstens') % amount_text(limit(),
                                                                  piece),
                    bg=SURFACE, fg=SUB, font=window.f_small)
    hint.pack(side='left', padx=(8, 0))
    buttons = tk.Frame(holder, bg=SURFACE)
    buttons.pack(fill='x', padx=12, pady=(8, 10))

    def go():
        value = stock_split.check_amount(form['amount'].get(), limit(), piece)
        if value is None:
            page['say'](t('s_ml_menge_teilen') % amount_text(limit(), piece),
                        RED)
            return
        op = (stock_split.share_op if sharing else stock_split.unshare_op)(
            lot, form['unit'], value)
        submit_share(window, page, op)

    def cancel():
        page['share_form'] = None
        _place_share_form(page)
    _button(window, buttons, t('s_ml_teilen_knopf' if sharing
                               else 's_ml_zurueck_knopf'), go,
            strong=True).pack(side='left')
    _button(window, buttons, t('s_hl_abbrechen'), cancel).pack(
        side='left', padx=(8, 0))
    form['go'], form['pick'] = go, pick


def submit_share(window, page, op):
    """Die Op im Hintergrund schicken; danach gleicht der Abgleich ab, und
    mit dem neuen Stand zeichnet die Seite die Aufteilung neu."""
    if page['busy']:
        page['say'](t('s_sl_noch_dran'), SUB)
        return
    page['busy'] = True
    page['say'](t('s_sl_bucht'), SUB)
    from .pages import _from_thread

    def work():
        from . import basetool, basetool_sync
        done = False
        try:
            ok, reason = stock_split.send(basetool.CONNECTION, op)
            if ok:
                done = True
                message, color = t('s_ml_geteilt_ok' if op['op'] ==
                                   stock_split.SHARE else 's_ml_zurueck_ok'), \
                    ACCENT
            else:
                message, color = stock_split.reason_text(reason), RED
        except basetool.ApiError as error:
            from . import errors
            from .basetool_page import error_text
            errors.record('my_stock_page.teilen', RuntimeError(
                error.describe()))
            message, color = error_text(error.code), RED
        except Exception as exc:
            from . import errors
            errors.record('my_stock_page.teilen', exc)
            message, color = t('s_sl_r_anders', type(exc).__name__), RED
        # Neuer Stand: Der Abgleich holt den Feed mit der neuen Aufteilung.
        basetool_sync.request_now()

        def show():
            page['busy'] = False
            if done:
                page['share_form'] = None
                page['redraw']()
            page['say'](message, color)
        _from_thread(page['listing'], show)
    import threading
    threading.Thread(target=work, daemon=True, name='teilen').start()


def _group_row(window, page, builder, group, key, is_open):
    """Die Summenzeile einer Gruppe. Dahinter bleiben Tabellenzeilen für die
    Unterzeilen frei (`_fold_rows`); ein Klick klappt nur diese um, die
    übrige Liste bleibt stehen."""
    base = _stripe(page['listing'], BG)
    items = builder['items']
    first = group[0]
    kind = kind_of(first, builder['server'], builder['cache'])
    tag = stock_kinds.tag(first['name']) if items else None
    places = sorted({r['place'] for r in group if r['place']},
                    key=str.lower)
    qualities = sorted({r['quality'] for r in group
                        if r['quality'] is not None})
    if not qualities:
        quality = '—'
    elif qualities[0] == qualities[-1]:
        quality = '%d' % qualities[0]
    else:
        quality = '%d–%d' % (qualities[0], qualities[-1])
    badges = [(stock_kinds.label(kind), SUB)]
    if builder['sale_badge'] and any(r['store'] == STORE_TRADE
                                     for r in group):
        badges.append((t('s_ml_verkauf'), ACCENT))

    def toggle():
        fold = page.get('folds', {}).get(key)
        if fold is None or fold['head'] not in page['row_widgets']:
            if key in page['open']:
                page['open'].discard(key)
            else:
                page['open'].add(key)
            _keep_scroll(page['listing'], page['redraw'])
            return
        # Nach dem, was zu sehen ist: Auch eine von der Suche oder vom
        # Bearbeiten geöffnete Gruppe klappt beim ersten Klick zu.
        show = not fold['entries']
        if show:
            page['open'].add(key)
        else:
            page['open'].discard(key)
        _fold_show(window, page, fold, show)

    def arrow(cell):
        icon = icons.line(cell, 'zuklappen' if is_open else 'aufklappen',
                          background=base, font=window.f_small)
        icon.pack(side='right', padx=(4, 10))
        cell.arrow = icon

    values = {
        'name': (name_cell(window, base, '%s (%d)' % (first['name'],
                                                      len(group)),
                           badges, tag, extra='' if items
                           else mining_text(first['name'])), FG),
        'place': (places[0] if len(places) == 1 else
                  (t('s_lg_orte_n') % len(places) if places else '—'), SUB),
        'quality': (quality, SUB),
        'amount': (amount_text(round(sum(r['amount'] for r in group), 3),
                               first['piece']), ACCENT),
        'action': (arrow, SUB),
    }
    if builder['selling']:
        price = builder['prices'].best_price(first['name'])
        value = price * sum(r['amount'] for r in group)
        profit = _profit(group, builder['prices'].best_price)
        values['price'] = (_money(price) if price else '—', SUB)
        values['total'] = (_money(value) if value else '—', FG)
        values['profit'] = ('—', SUB) if profit is None else (
            ('+' if profit > 0 else '') + _money(profit),
            ACCENT if profit > 0 else RED if profit < 0 else SUB)
    widgets = table_cells(window, builder, base, values)
    bind_row(widgets, toggle)
    head = ('group', key, widgets)
    page['row_widgets'].append(head)
    rows = sorted(group, key=lambda r: (r['place'].lower(),
                                        -(r['quality'] or 0)))
    # Je Unterzeile zwei Tabellenzeilen: sie selbst und ihr Teilen-Feld.
    page['folds'][key] = {'head': head, 'rows': rows,
                          'start': builder['row'], 'size': 2 * len(rows),
                          'entries': [],
                          'arrow': getattr(widgets[builder['columns'].index(
                              'action')], 'arrow', None)}
    builder['row'] += 2 * len(rows)


def _fold_rows(window, page, fold):
    """Die Unterzeilen einer Gruppe in ihre freien Tabellenzeilen bauen und in
    `row_widgets` direkt hinter die Summenzeile stellen."""
    builder = dict(page['builder'], row=fold['start'])
    before = len(page['row_widgets'])
    for row in fold['rows']:
        _post_row(window, page, builder, row, nested=True)
    entries = page['row_widgets'][before:]
    del page['row_widgets'][before:]
    at = page['row_widgets'].index(fold['head']) + 1
    page['row_widgets'][at:at] = entries
    fold['entries'] = entries


def _fold_show(window, page, fold, show):
    """Nur die Unterzeilen dieser Gruppe zeigen oder abbauen, den Pfeil
    tauschen und die Streifen neu verteilen — ohne Neuaufbau der Liste."""
    if show == bool(fold['entries']):
        return
    if show:
        _fold_rows(window, page, fold)
    else:
        table = page['builder']['table']
        end = fold['start'] + fold['size']
        for widget in table.grid_slaves():
            try:
                if fold['start'] <= int(widget.grid_info()['row']) < end:
                    widget.destroy()
            except (tk.TclError, KeyError, ValueError):
                pass
        page['row_widgets'] = [e for e in page['row_widgets']
                               if e not in fold['entries']]
        fold['entries'] = []
    arrow = fold.get('arrow')
    if arrow is not None:
        try:
            arrow.swap_symbol('zuklappen' if show else 'aufklappen')
        except (tk.TclError, AttributeError):
            pass
    _restripe(page)


def _restripe(page):
    """Die Streifen so verteilen, wie `_stripe` sie beim Bauen der Liste
    vergibt: Zeile für Zeile in der Reihenfolge von `row_widgets`. Die
    Zeile im Bearbeiten (`page['editing']`) steht auf `theme.HOVER`."""
    editing = page.get('editing')
    for n, (kind, what, widgets) in enumerate(page['row_widgets']):
        if kind == 'post' and editing == (what['store'], what['index']):
            new = theme.HOVER
        else:
            new = SURFACE if n % 2 else BG
        try:
            old = str(widgets[0].cget('bg'))
        except tk.TclError:
            continue
        if old != new:
            for widget in widgets:
                _repaint(widget, old, new)
    page['listing'].stripe_n = len(page['row_widgets'])


def _show_editing(page):
    """Die Zeile im Bearbeiten hervorheben bzw. die Hervorhebung abnehmen —
    liegt sie in einer zugeklappten Gruppe, klappt nur diese auf. Ohne
    gezeichnete Liste wird neu gezeichnet."""
    try:
        drawn = 'builder' in page and page['listing'].winfo_exists() \
            and page['builder']['table'].winfo_exists()
    except tk.TclError:
        drawn = False
    if not drawn:
        page['redraw']()
        return
    editing = page.get('editing')
    if editing is not None:
        for fold in page.get('folds', {}).values():
            if not fold['entries'] and any(
                    (r['store'], r['index']) == editing for r in fold['rows']):
                _fold_show(page['window'], page, fold, True)
    _restripe(page)


def _repaint(widget, old, new):
    """Alles in `widget`, das auf `old` steht, auf `new` stellen — Blasen
    über ihr `set_background`."""
    try:
        if str(widget.cget('bg')) == old:
            setter = getattr(widget, 'set_background', None)
            if setter is not None:
                setter(new)
            else:
                widget.configure(bg=new)
    except tk.TclError:
        return
    for child in widget.winfo_children():
        _repaint(child, old, new)


def remove(page, row):
    """Einen Posten löschen, ohne dass die Seite nach oben springt."""
    def run():
        if row['store'] == STORE_RAW:
            materials.remove(row['index'])
        else:
            trade_cargo.remove(row['index'])
        page['share_form'] = None
        if page.get('editing') is not None:
            for form in page['forms'].values():
                form['reset']()
        page['redraw']()
    _keep_scroll(page['listing'], run)


def _entry_at(store, index):
    rows = materials.load() if store == STORE_RAW else trade_cargo.load()
    return rows[index] if 0 <= index < len(rows) else None


def start_editing(page, store, index, entry):
    """Merkt, welcher Posten im Formular liegt — Stelle und Inhalt."""
    page['editing'] = (store, index)
    page['editing_entry'] = dict(entry)


def still_editing(page):
    """Liegt an der gemerkten Stelle noch derselbe Posten? Ohne offenes
    Bearbeiten immer wahr.

    ⚠ Die Stelle allein genügt nicht: Verbraucht die Herstellung einen
    Posten ganz oder holt der Abgleich Neues, rücken die übrigen nach — ein
    Speichern schriebe dann über einen fremden Posten."""
    editing = page.get('editing')
    if editing is None:
        return True
    return _entry_at(*editing) == page.get('editing_entry')


def drop_stale_edit(page):
    """Hat sich der Posten im Formular inzwischen geändert, das Bearbeiten
    abbrechen und es sagen. `True`, wenn abgebrochen wurde."""
    if still_editing(page):
        return False
    for form in page['forms'].values():
        form['reset'](quiet=True)
    page['editing'] = None
    page['say'](t('s_ml_veraendert'), GOLD)
    return True


def edit(page, row):
    """Einen Posten in sein Formular holen — Rohstoff, Ware oder Item."""
    form_kind = (FORM_ITEM if row['item'] else
                 STORE_RAW if row['store'] == STORE_RAW else STORE_TRADE)
    page['form_open']()
    page['show_form'](form_kind)
    for kind, form in page['forms'].items():
        if kind != form_kind:
            form['reset'](quiet=True)
    page['forms'][form_kind]['edit'](row['index'])
    canvas = getattr(page['inner'], 'canvas', None)
    if canvas is not None:
        try:
            canvas.yview_moveto(0)
        except tk.TclError:
            pass


# ------------------------------------------------------------ Formulare
def _forms(window, parent, page):
    """Umschalter Herstellen | Verkaufen | Item und darunter das passende
    Formular. Alle werden einmal gebaut; gezeigt wird eines."""
    holder = tk.Frame(parent, bg=BG)
    chosen = paths.setting(SETTING_FORM_KIND) or STORE_RAW
    if chosen not in (STORE_RAW, STORE_TRADE, FORM_ITEM):
        chosen = STORE_RAW
    frames = {}

    def show(kind):
        page['form_kind'] = kind
        paths.set_setting(SETTING_FORM_KIND, kind)
        for key, frame in frames.items():
            if key == kind:
                frame.pack(fill='x')
            else:
                frame.pack_forget()
        switch.select(kind)
    switch = _choice(window, parent, [(STORE_RAW, t('s_ml_herstellen')),
                                      (STORE_TRADE, t('s_ml_verkaufen')),
                                      (FORM_ITEM, t('s_ml_item'))],
                     chosen, show)
    switch.pack(fill='x', pady=(8, 0))
    page['form_switch'] = switch
    holder.pack(fill='x')
    for kind in (STORE_RAW, STORE_TRADE, FORM_ITEM):
        frames[kind] = tk.Frame(holder, bg=BG)
    page['forms'][STORE_RAW] = _raw_form(window, frames[STORE_RAW], page)
    page['forms'][STORE_TRADE] = _trade_form(window, frames[STORE_TRADE],
                                             page)
    page['forms'][FORM_ITEM] = _item_form(window, frames[FORM_ITEM], page)
    page['show_form'] = show
    show(chosen)


def _field_block(window, parent, label, pady=(12, 0)):
    block = tk.Frame(parent, bg=BG)
    block.pack(fill='x', pady=pady)
    head = tk.Label(block, text=label, bg=BG, fg=FG, font=window.f_bold,
                    anchor='w')
    head.pack(fill='x')
    return block, head


def _raw_form(window, parent, page):
    """Ein Rohstoff zum Herstellen: Material, Menge (SCU oder cSCU), Güte,
    Ort. Nur Namen und Orte aus den Spieldaten werden angenommen."""
    from .main_window import round_entry, ask_yes_no
    from . import crafting, places
    material = tk.StringVar(parent)
    amount = tk.StringVar(parent)
    quality = tk.StringVar(parent)
    place = tk.StringVar(parent, value=paths.setting('lager_ort') or '')
    cscu = [paths.setting('lager_einheit') == 'cscu']
    state = {'index': None}

    def unit_word(name=None):
        piece = _is_piece(name if name is not None else material.get())
        return t('s_lg_stueck') if piece else 'SCU'

    def factor():
        # Stückware nie umrechnen: 75 Hadanite sind 75, nicht 0,75.
        if _is_piece(material.get()):
            return 1.0
        return materials.CSCU if cscu[0] else 1.0

    block, _head = _field_block(window, parent, t('s_lg_material'))

    def material_names():
        try:
            return sorted(crafting.storable())
        except Exception:
            return []
    line, drop, redraw_material = _combo_box(
        window, block, material, material_names,
        placeholder=t('s_pl_rohstoff'))
    line.pack(fill='x', pady=(4, 0))
    drop.pack(fill='x')

    pair = tk.Frame(parent, bg=BG)
    pair.columnconfigure(0, weight=1, uniform='lager_paar')
    pair.columnconfigure(1, weight=1, uniform='lager_paar')
    pair.pack(fill='x', pady=(12, 0))
    amount_block = tk.Frame(pair, bg=BG)
    amount_block.grid(row=0, column=0, sticky='new', padx=(0, 8))
    amount_label = tk.Label(amount_block, text=t('s_lg_menge'), bg=BG, fg=FG,
                            font=window.f_bold, anchor='w')
    amount_label.pack(fill='x')
    amount_line = tk.Frame(amount_block, bg=BG)
    amount_line.pack(fill='x', pady=(4, 0))
    amount_box = round_entry(amount_line, amount, window.f_small, theme.FIELD,
                             LINE, ACCENT, FG, width=8,
                             placeholder=t('s_pl_lager_menge'))

    def unit_changed(on):
        paths.set_setting('lager_einheit', 'cscu' if on else 'scu')
        unit_follow()

    # Erst das Kästchen packen, dann das Feld: Das zuletzt gepackte Feld mit
    # `expand=True` nähme sonst den ganzen Platz.
    cscu_box = _checkbox(amount_line, t('s_lg_cscu'), cscu, unit_changed,
                         window.f_small)
    cscu_box.pack(side='right', padx=(10, 0))
    amount_box.holder.pack(side='left', fill='both', expand=True)
    preview = tk.Label(amount_block, text='', bg=BG, fg=ACCENT,
                       font=window.f_small, anchor='w', justify='left')
    _wrap(preview, reference=amount_block)

    quality_block = tk.Frame(pair, bg=BG)
    quality_block.grid(row=0, column=1, sticky='new', padx=(8, 0))
    tk.Label(quality_block, text=t('s_lg_qualitaet'), bg=BG, fg=FG,
             font=window.f_bold, anchor='w').pack(fill='x')
    round_entry(quality_block, quality, window.f_small, theme.FIELD, LINE,
                ACCENT, FG, width=8,
                placeholder=t('s_pl_qualitaet')).holder.pack(fill='x',
                                                             pady=(4, 0))

    place_block, _head = _field_block(window, parent, t('s_lg_ort'))
    line, drop, redraw_place = _combo_box(window, place_block, place,
                                          places.all_places,
                                          placeholder=t('s_pl_lagerort'))
    line.pack(fill='x', pady=(4, 0))
    drop.pack(fill='x')

    def unit_follow(*_a):
        """Beschriftung und cSCU-Kästchen zum gewählten Material: Bei
        Stückware gibt es kein cSCU, das Kästchen verschwindet."""
        if _is_piece(material.get()):
            cscu_box.pack_forget()
            amount_label.configure(text=t('s_lg_menge_stueck'))
        else:
            if not cscu_box.winfo_manager():
                cscu_box.pack(side='right', padx=(10, 0),
                              before=amount_box.holder)
            amount_label.configure(text=t('s_lg_menge_cscu') if cscu[0]
                                   else t('s_lg_menge'))
        show_preview()
    material.trace_add('write', unit_follow)

    def amount_before():
        index = state['index']
        if index is None:
            return 0.0
        rows = materials.load()
        return _number(rows[index].get('menge')) if 0 <= index < len(rows) \
            else 0.0

    def show_preview(*_a):
        """Was beim Tippen herauskommt — nur bei einer Rechnung, und bei
        einer Zahl, deren Einheit verwechselt aussieht."""
        raw = (amount.get() or '').strip()
        calculating = any(c in raw[1:] for c in '+-−') or raw[:1] in '+-−'
        if raw and not calculating and not _is_piece(material.get()):
            value = materials.calculate(raw, 0.0)
            if value is not None and value > 0:
                hint = materials.unit_hint(value, 'cscu' if cscu[0] else 'scu')
                if hint:
                    preview.configure(text=t(hint) % _amount_text(
                        value * factor()), fg=GOLD)
                    preview.pack(fill='x', pady=(4, 0))
                    return
                if cscu[0]:
                    preview.configure(text=t('s_lg_wird_zu') % _amount_text(
                        value * factor()), fg=ACCENT)
                    preview.pack(fill='x', pady=(4, 0))
                    return
        if not raw or not calculating:
            preview.pack_forget()
            return
        before = amount_before() / factor()
        value = materials.calculate(raw, before)
        if value is None:
            preview.pack_forget()
            return
        if value < 0:
            preview.configure(text=t('s_lg_ergibt_minus') % (before,
                                                             unit_word()),
                              fg=GOLD)
        elif value == 0:
            preview.configure(text=t('s_lg_ergibt_null'), fg=GOLD)
        else:
            preview.configure(text=t('s_lg_ergibt') % (round(value, 3),
                                                       unit_word()),
                              fg=ACCENT)
        preview.pack(fill='x', pady=(4, 0))
    amount.trace_add('write', show_preview)
    if cscu[0]:
        amount_label.configure(text=t('s_lg_menge_cscu'))

    calc_hint = tk.Label(parent, text='', bg=BG, fg=SUB, font=window.f_small,
                         anchor='w', justify='left')
    calc_hint.pack(fill='x', pady=(8, 4))
    buttons = tk.Frame(parent, bg=BG)
    buttons.pack(anchor='w', pady=(0, 10))

    def clear_fields():
        material.set('')
        amount.set('')
        quality.set('')

    def reset(quiet=False):
        if state['index'] is None and quiet:
            return
        state['index'] = None
        clear_fields()
        place.set(paths.setting('lager_ort') or '')
        calc_hint.configure(text='')
        page['editing'] = None
        set_buttons()
        if not quiet:
            page['say']('')
            _show_editing(page)

    def edit_entry(index):
        rows = materials.load()
        if not 0 <= index < len(rows):
            return
        entry = rows[index]
        state['index'] = index
        start_editing(page, STORE_RAW, index, entry)
        material.set(entry.get('material') or '')
        # In der Einheit vorlegen, in der das Feld gerade rechnet.
        amount.set('%g' % round(_number(entry.get('menge')) / factor(), 4))
        quality.set('%g' % _number(entry['qualitaet'])
                    if entry.get('qualitaet') else '')
        place.set(entry.get('ort') or '')
        open_name = entry.get('material') or '?'
        page['say'](t('s_lg_bearbeite') % open_name, ACCENT)
        calc_hint.configure(text=t('s_lg_rechnen'))
        set_buttons()
        _show_editing(page)

    def delete_entry(*_a):
        if drop_stale_edit(page):
            page['redraw']()
            return
        index = state['index']
        rows = materials.load()
        if index is None or not 0 <= index < len(rows):
            return
        entry = rows[index]
        if not ask_yes_no(window.root, t('s_lg_posten_frage_t'),
                          t('s_lg_posten_frage') % (
                              entry.get('material') or '?',
                              _number(entry.get('menge')),
                              unit_word(entry.get('material') or ''))):
            return
        _keep_scroll(page['inner'], lambda: (materials.remove(index),
                                             reset(), page['redraw']()))

    def save(*_a):
        say = page['say']
        if drop_stale_edit(page):
            page['redraw']()
            return
        name = material.get().strip()
        if not name:
            say(t('s_lg_kein_material'), GOLD)
            return
        # Speicherbar ist nur, was in der Rohstoff-Liste steht.
        proper = crafting.storage_name(name)
        if proper is None:
            say(t('s_lg_name_fremd') % name, GOLD)
            redraw_material()
            return
        corrected = proper != name and proper.lower() != name.lower()
        name = proper
        raw = (amount.get() or '0').strip()
        before = amount_before()
        calculating = bool(raw) and (raw[:1] in '+-−'
                                     or any(c in raw[1:] for c in '+-−'))
        value = materials.calculate(raw, before)
        if value is None:
            say(t('s_lg_keine_menge'), GOLD)
            return
        index = state['index']
        if calculating:
            if value < 0 and index is None:
                say(t('s_lg_nicht_negativ'), GOLD)
                return
            if value < 0:
                say(t('s_lg_zu_wenig') % (before, unit_word()), GOLD)
                return
            if value == 0 and index is not None:
                materials.remove(index)
                state['index'] = None
                page['editing'] = None
                clear_fields()
                place.set(paths.setting('lager_ort') or '')
                say(t('s_lg_alles_weg') % name, SUB)
                set_buttons()
                page['redraw']()
                return
        elif value <= 0:
            say(t('s_lg_keine_menge'), GOLD)
            return
        proper_place = places.official_name(place.get())
        if proper_place is None:
            say(t('s_lg_ort_fremd') % place.get().strip(), GOLD)
            redraw_place()
            return
        place.set(proper_place)
        number = materials.parse_number(quality.get())
        if number is None or not 0 <= int(round(number)) <= 1000:
            say(t('s_lg_keine_guete'), GOLD)
            return
        grade = int(round(number))
        value = round(value * factor(), 4)
        from . import page_usage
        page_usage.action('lager_hand')
        if index is None:
            materials.add(name, value, grade, place.get())
            text = t('s_lg_eingetragen') % (name, value, unit_word(name))
        else:
            materials.change(index, name, value, grade, place.get())
            text = t('s_lg_geaendert') % (name, value, unit_word(name))
        if corrected:
            text = t('s_lg_berichtigt') % (material.get().strip(), name) \
                + ' ' + text
        state['index'] = None
        page['editing'] = None
        # Der Ort bleibt stehen und wird gemerkt: Eine Ausbeute sind meist
        # mehrere Posten am selben Ort.
        clear_fields()
        paths.set_setting('lager_ort', place.get().strip())
        say(text, SUB)
        calc_hint.configure(text='')
        set_buttons()
        page['redraw']()

    def set_buttons():
        # Neu gebaut, nicht umbeschriftet: Ein Knopf ist eine Leinwand fester
        # Breite.
        for child in buttons.winfo_children():
            child.destroy()
        if state['index'] is None:
            _button(window, buttons, t('s_lg_eintragen'), save).pack(
                side='left')
        else:
            _button(window, buttons, t('s_lg_speichern'), save,
                    strong=True).pack(side='left')
            _button(window, buttons, t('s_lg_abbrechen'), reset).pack(
                side='left', padx=(8, 0))
            share_buttons(window, buttons, page, STORE_RAW, state['index'])
            _button(window, buttons, t('s_lg_posten_weg'), delete_entry,
                    danger=True).pack(side='left', padx=(24, 0))
    set_buttons()
    return {'edit': edit_entry, 'reset': reset, 'save': save,
            'place': place, 'material': material, 'amount': amount,
            'quality': quality, 'state': state}


def _trade_form(window, parent, page):
    """Ware zum Verkaufen: Ware aus der UEX-Liste, Menge, Einkaufspreis,
    Ort, gestohlen."""
    from .main_window import round_entry
    from . import places, selling
    goods = tk.StringVar(parent)
    amount = tk.StringVar(parent)
    cost = tk.StringVar(parent)
    place = tk.StringVar(parent, value=paths.setting('handel_ort') or '')
    stolen = [False]
    state = {'index': None}

    block, _head = _field_block(window, parent, t('s_hl_ware'))
    line, drop, redraw_goods = _combo_box(window, block, goods, selling.goods,
                                          placeholder=t('s_pl_ware'))
    line.pack(fill='x', pady=(4, 0))
    drop.pack(fill='x')
    block, amount_label = _field_block(window, parent, t('s_hl_menge'))
    round_entry(block, amount, window.f_small, theme.FIELD, LINE, ACCENT, FG,
                placeholder=t('s_pl_lager_menge')).holder.pack(fill='x',
                                                               pady=(4, 0))
    block, _head = _field_block(window, parent, t('s_hl_einkauf'))
    round_entry(block, cost, window.f_small, theme.FIELD, LINE, ACCENT, FG,
                placeholder=t('s_pl_einkauf')).holder.pack(fill='x',
                                                           pady=(4, 0))
    block, _head = _field_block(window, parent, t('s_hl_ort'))
    line, drop, redraw_place = _combo_box(window, block, place,
                                          places.all_places,
                                          placeholder=t('s_pl_lagerort'))
    line.pack(fill='x', pady=(4, 0))
    drop.pack(fill='x')

    switches = tk.Frame(parent, bg=BG)
    switches.pack(fill='x', pady=(12, 4))
    holder = {'box': None}

    def build_stolen():
        if holder['box'] is not None:
            holder['box'].destroy()
        holder['box'] = _checkbox(switches, t('s_hl_gestohlen'), stolen,
                                  lambda on: stolen.__setitem__(0, on),
                                  window.f_small)
        holder['box'].pack(side='left')
    build_stolen()

    preview = tk.Label(parent, text='', bg=BG, fg=SUB, font=window.f_small,
                       anchor='w')
    preview.pack(fill='x')

    def amount_before():
        index = state['index']
        rows = trade_cargo.load() if index is not None else []
        return (_number(rows[index].get('menge'))
                if index is not None and 0 <= index < len(rows) else 0.0)

    def name_before():
        index = state['index']
        rows = trade_cargo.load() if index is not None else []
        return ((rows[index].get('ware') or '').strip()
                if index is not None and 0 <= index < len(rows) else None)

    def show_preview(*_a):
        raw = (amount.get() or '').strip()
        calculating = any(c in raw[1:] for c in '+-−') or raw[:1] in '+-−'
        if not raw or not calculating:
            preview.configure(text='')
            return
        value = trade_cargo.calculate(raw, amount_before())
        if value is None:
            preview.configure(text=t('s_hl_rechnung_kaputt'), fg=GOLD)
        elif value <= 0:
            preview.configure(text=t('s_hl_unter_null'), fg=RED)
        else:
            preview.configure(text=t('s_hl_ergibt').format(
                menge=_amount_text(value)), fg=SUB)
    amount.trace_add('write', show_preview)

    buttons = tk.Frame(parent, bg=BG)
    buttons.pack(fill='x', pady=(8, 10))

    def set_amount_label(piece):
        amount_label.configure(text=t('s_hl_menge_stueck') if piece
                               else t('s_hl_menge'))

    def reset(quiet=False):
        if state['index'] is None and quiet:
            return
        state['index'] = None
        goods.set('')
        amount.set('')
        cost.set('')
        stolen[0] = False
        build_stolen()
        set_amount_label(False)
        page['editing'] = None
        set_buttons()
        if not quiet:
            page['say']('')
            _show_editing(page)

    def edit_entry(index):
        rows = trade_cargo.load()
        if not 0 <= index < len(rows):
            return
        entry = rows[index]
        state['index'] = index
        start_editing(page, STORE_TRADE, index, entry)
        goods.set(entry.get('ware') or '')
        amount.set(_amount_text(entry.get('menge') or 0))
        paid = trade_cargo.cost_of(entry)
        cost.set('' if paid is None
                 else ('%.2f' % paid).rstrip('0').rstrip('.'))
        place.set(entry.get('ort') or '')
        stolen[0] = bool(entry.get('gestohlen'))
        build_stolen()
        set_amount_label(trade_cargo.is_pieces(entry))
        open_name = entry.get('ware') or '?'
        page['say'](t('s_lg_bearbeite') % open_name, ACCENT)
        set_buttons()
        _show_editing(page)

    def save():
        say = page['say']
        if drop_stale_edit(page):
            page['redraw']()
            return
        name = (goods.get() or '').strip()
        # Geschlossene Liste: angenommen wird nur, was UEX kennt — oder der
        # Name, den der Posten schon trägt (ein Item aus dem Basetool).
        if not selling.known(name) and name != name_before():
            say(t('s_hl_unbekannt'), RED)
            redraw_goods()
            return
        if not places.knows(place.get()):
            say(t('s_hl_ort_unbekannt'), RED)
            redraw_place()
            return
        if state['index'] is None:
            ok, reason = trade_cargo.add(name, amount.get(), place.get(),
                                         stolen[0], cost=cost.get())
        else:
            ok, reason = trade_cargo.change(state['index'], name,
                                            amount.get(), place.get(),
                                            stolen[0], cost=cost.get())
        if not ok:
            say(t(TRADE_REASONS.get(reason, 's_hl_fehler')), RED)
            return
        state['index'] = None
        page['editing'] = None
        paths.set_setting('handel_ort', place.get() or '')
        goods.set('')
        amount.set('')
        cost.set('')
        set_amount_label(False)
        say(t('s_hl_gebucht'), ACCENT)
        set_buttons()
        page['redraw']()

    def set_buttons():
        for child in buttons.winfo_children():
            child.destroy()
        if state['index'] is None:
            _button(window, buttons, t('s_hl_buchen'), save,
                    strong=True).pack(side='left')
        else:
            _button(window, buttons, t('s_hl_speichern'), save,
                    strong=True).pack(side='left')
            _button(window, buttons, t('s_hl_abbrechen'), reset).pack(
                side='left', padx=(8, 0))
            share_buttons(window, buttons, page, STORE_TRADE, state['index'])
    set_buttons()
    return {'edit': edit_entry, 'reset': reset, 'save': save,
            'place': place, 'goods': goods, 'amount': amount, 'cost': cost,
            'stolen': stolen, 'state': state}


def item_choices():
    """`{Name klein: Eintrag}` der Gegenstände in der UEX-Warenliste
    (`stock_catalog`), ohne Netz aus der Ablage oder der mitgelieferten
    Liste."""
    from . import stock_catalog
    try:
        return {k: e for k, e in stock_catalog.choices().items()
                if e.get('item') is not None}
    except Exception:
        return {}


def _item_form(window, parent, page):
    """Ein Item: Name aus der Warenliste (geschlossene Liste), Stückzahl,
    Ort, gestohlen. Gespeichert wird im Handelslager als Stück-Posten mit der
    UEX-Nummer, über die der Abgleich das Item findet."""
    from .main_window import round_entry
    from . import places
    name = tk.StringVar(parent)
    amount = tk.StringVar(parent)
    place = tk.StringVar(parent, value=paths.setting('handel_ort') or '')
    stolen = [False]
    state = {'index': None, 'name': None, 'uex': None}

    def names():
        return sorted((e['name'] for e in item_choices().values()),
                      key=str.lower)

    def kind_text(text):
        return stock_kinds.label(stock_kinds.item_kind(text))

    block, _head = _field_block(window, parent, t('s_ml_item'))
    line, drop, redraw_name = _combo_box(
        window, block, name, names, extra=kind_text,
        placeholder=t('s_ml_item_h'),
        empty_text=lambda: None if item_choices() else t('s_ml_keine_liste'))
    line.pack(fill='x', pady=(4, 0))
    drop.pack(fill='x')
    block, _head = _field_block(window, parent, t('s_hl_menge_stueck'))
    round_entry(block, amount, window.f_small, theme.FIELD, LINE, ACCENT, FG,
                placeholder=t('s_ml_stueck_h')).holder.pack(fill='x',
                                                             pady=(4, 0))
    block, _head = _field_block(window, parent, t('s_hl_ort'))
    line, drop, redraw_place = _combo_box(window, block, place,
                                          places.all_places,
                                          placeholder=t('s_pl_lagerort'))
    line.pack(fill='x', pady=(4, 0))
    drop.pack(fill='x')
    switches = tk.Frame(parent, bg=BG)
    switches.pack(fill='x', pady=(12, 4))
    holder = {'box': None}

    def build_stolen():
        if holder['box'] is not None:
            holder['box'].destroy()
        holder['box'] = _checkbox(switches, t('s_hl_gestohlen'), stolen,
                                  lambda on: stolen.__setitem__(0, on),
                                  window.f_small)
        holder['box'].pack(side='left')
    build_stolen()
    buttons = tk.Frame(parent, bg=BG)
    buttons.pack(fill='x', pady=(8, 10))

    def reset(quiet=False):
        if state['index'] is None and quiet:
            return
        state.update(index=None, name=None, uex=None)
        name.set('')
        amount.set('')
        stolen[0] = False
        build_stolen()
        page['editing'] = None
        set_buttons()
        if not quiet:
            page['say']('')
            _show_editing(page)

    def edit_entry(index):
        rows = trade_cargo.load()
        if not 0 <= index < len(rows):
            return
        entry = rows[index]
        state.update(index=index, name=(entry.get('ware') or '').strip(),
                     uex=entry.get('uex'))
        start_editing(page, STORE_TRADE, index, entry)
        name.set(entry.get('ware') or '')
        amount.set('%d' % int(_number(entry.get('menge'))))
        place.set(entry.get('ort') or '')
        stolen[0] = bool(entry.get('gestohlen'))
        build_stolen()
        page['say'](t('s_lg_bearbeite') % (entry.get('ware') or '?'), ACCENT)
        set_buttons()
        _show_editing(page)

    def save():
        say = page['say']
        if drop_stale_edit(page):
            page['redraw']()
            return
        text = (name.get() or '').strip()
        found = item_choices().get(text.lower())
        # Geschlossene Liste: nur Gegenstände der Warenliste — oder der Name,
        # den der Posten schon trägt (ein Item aus dem Basetool).
        if found is None and text != state['name']:
            say(t('s_ml_item_unbekannt') % text, RED)
            redraw_name()
            return
        if not places.knows(place.get()):
            say(t('s_hl_ort_unbekannt'), RED)
            redraw_place()
            return
        # Fehlt der unveränderte Name in der Warenliste (umbenannt, Liste
        # leer), bleibt die gemerkte Nummer stehen — sonst fiele der Abgleich
        # auf die Suche über den Namen zurück.
        uex = (found['item'] if found
               else state.get('uex') if text == state['name'] else None)
        ok, reason = trade_cargo.add_item(found['name'] if found else text,
                                          amount.get(), place.get(),
                                          stolen[0], uex=uex,
                                          index=state['index'])
        if not ok:
            say(t('s_ml_item_menge' if reason == 'menge'
                  else 's_hl_fehlt_ware' if reason == 'ware'
                  else 's_hl_fehler'), RED)
            return
        state.update(index=None, name=None, uex=None)
        page['editing'] = None
        paths.set_setting('handel_ort', place.get() or '')
        name.set('')
        amount.set('')
        say(t('s_hl_gebucht'), ACCENT)
        set_buttons()
        page['redraw']()

    def set_buttons():
        for child in buttons.winfo_children():
            child.destroy()
        if state['index'] is None:
            _button(window, buttons, t('s_hl_buchen'), save,
                    strong=True).pack(side='left')
        else:
            _button(window, buttons, t('s_hl_speichern'), save,
                    strong=True).pack(side='left')
            _button(window, buttons, t('s_hl_abbrechen'), reset).pack(
                side='left', padx=(8, 0))
            share_buttons(window, buttons, page, STORE_TRADE, state['index'])
    set_buttons()
    return {'edit': edit_entry, 'reset': reset, 'save': save,
            'place': place, 'name': name, 'amount': amount,
            'stolen': stolen, 'state': state}


# -------------------------------------------------------- Sichern, Löschen
def detect_backup(text):
    """`(Teil, Posten)` einer Sicherung — Rohstoffe (`raw`) oder Verkauf
    (`trade`) — oder `(None, None)`. Erkannt am Feld der Posten: `material`
    bzw. `ware`. Eine Datei ohne Posten wird nicht angenommen: Sie leerte
    sonst ein Lager, ohne dass man es merkt."""
    try:
        data = json.loads(text)
    except Exception:
        return None, None
    entries_ = data.get('posten') if isinstance(data, dict) else None
    if not isinstance(entries_, list):
        return None, None
    if any(isinstance(e, dict) and (e.get('ware') or '').strip()
           for e in entries_):
        found = trade_cargo.from_json(text)
        return (STORE_TRADE, found) if found else (None, None)
    if any(isinstance(e, dict) and (e.get('material') or '').strip()
           for e in entries_):
        found = materials.from_json(text)
        return (STORE_RAW, found) if found else (None, None)
    return None, None


def _backup_section(window, parent, page):
    from .main_window import ask_yes_no
    body, _open = _storage_fold(window, parent, 's_ml_sichern_titel',
                                SETTING_BACKUP_OPEN, False)
    page['backup_body'] = body

    def write_out(store, kind):
        from . import file_picker
        module = materials if store == STORE_RAW else trade_cargo
        ending = '.csv' if kind == 'csv' else '.json'
        stem = 'lager' if store == STORE_RAW else 'handelslager'
        target = file_picker.save_file(
            t('s_lg_ausgeben'),
            suggestion='%s-%s%s' % (stem, time.strftime('%Y-%m-%d'), ending),
            extension=ending, start=None)
        if not target:
            return
        try:
            content = module.as_csv() if kind == 'csv' else module.as_json()
            with open(target, 'w', encoding='utf-8') as handle:
                handle.write(content)
            page['say'](t('s_lg_gespeichert') % os.path.basename(target))
        except Exception as exc:
            from . import errors
            errors.record('my_stock_page.ausgeben', exc)
            page['say'](t('s_hl_fehler'), RED)

    def read_in():
        from . import file_picker
        source = file_picker.open_file(t('s_lg_einlesen'))
        if not source:
            return
        try:
            with open(source, encoding='utf-8') as handle:
                store, found = detect_backup(handle.read())
        except Exception as exc:
            from . import errors
            errors.record('my_stock_page.einlesen', exc)
            store, found = None, None
        if store is None:
            page['say'](t('s_ml_datei_falsch'), GOLD)
            return
        for form in page['forms'].values():
            form['reset'](quiet=True)
        if store == STORE_RAW:
            if materials.save(found):
                page['say'](t('s_ml_eingelesen_raw') % len(found))
            else:
                page['say'](t('s_hl_fehler'), RED)
        else:
            counts = trade_cargo.restore(found)
            if counts is None:
                page['say'](t('s_hl_fehler'), RED)
            else:
                page['say'](t('s_ml_eingelesen_trade') % counts)
        page['redraw']()

    # Was geleert wird: Rohstoffe, die Ware im Handelslager oder seine Items
    # — Ware und Items liegen in derselben Datei, gelöscht wird je für sich.
    clear_parts = {
        STORE_RAW: (lambda: len(materials.load()),
                    lambda: 0 if materials.save([]) else None,
                    's_lg_leeren_frage_t', 's_lg_leeren_frage'),
        STORE_TRADE: (lambda: len(trade_cargo.goods_rows()),
                      trade_cargo.clear_goods,
                      's_ml_ware_frage_t', 's_ml_ware_frage'),
        FORM_ITEM: (lambda: len(trade_cargo.load())
                    - len(trade_cargo.goods_rows()),
                    trade_cargo.clear_items,
                    's_ml_items_frage_t', 's_ml_items_frage')}

    def clear(part):
        count_of, run, title_key, ask_key = clear_parts[part]
        count = count_of()
        if not count:
            return
        if not ask_yes_no(window.root, t(title_key), t(ask_key) % count):
            return
        for form in page['forms'].values():
            form['reset'](quiet=True)
        # `None` heißt: nicht gespeichert — dann nicht „geleert" melden.
        if run() is None:
            page['say'](t('s_hl_fehler'), RED)
        else:
            page['say'](t('s_lg_geleert') % count, GOLD)
        page['redraw']()

    page['backup'] = {'write_out': write_out, 'read_in': read_in,
                      'clear': clear}
    for store, label_key, clears in (
            (STORE_RAW, 's_ml_teil_herstellen',
             ((STORE_RAW, 's_lg_leeren'),)),
            (STORE_TRADE, 's_ml_teil_verkaufen',
             ((STORE_TRADE, 's_ml_ware_leeren'),
              (FORM_ITEM, 's_ml_items_leeren')))):
        tk.Label(body, text=t(label_key), bg=BG, fg=SUB, font=window.f_small,
                 anchor='w').pack(fill='x', pady=(10, 2))
        row = tk.Frame(body, bg=BG)
        row.pack(fill='x')
        _button(window, row, t('s_lg_aus_json'),
                lambda s=store: write_out(s, 'json')).pack(side='left')
        _button(window, row, t('s_lg_aus_csv'),
                lambda s=store: write_out(s, 'csv')).pack(side='left',
                                                          padx=(8, 0))
        # Löschen in einer eigenen Reihe — vier Knöpfe nebeneinander passen
        # in der Mindestbreite nicht.
        row = tk.Frame(body, bg=BG)
        row.pack(fill='x', pady=(6, 0))
        for number, (part, text_key) in enumerate(clears):
            _button(window, row, t(text_key), lambda p=part: clear(p),
                    danger=True).pack(side='left',
                                      padx=(0 if not number else 8, 0))
    row = tk.Frame(body, bg=BG)
    row.pack(fill='x', pady=(12, 0))
    _button(window, row, t('s_lg_einlesen'), read_in).pack(side='left')
    _body_text(body, t('s_ml_sichern_hilfe'), window.f_small, fill='x',
               pady=(8, 0))
