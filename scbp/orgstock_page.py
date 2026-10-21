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
Die Seite Lager der Einheit (`staffellager`) — das Lager der eigenen Einheiten aus dem KRT Profit
Basetool (`exchange_orgstock`).

Oben die Einheit (bei mehr als einer), die Reiter Material | Items wie in
Mein Lager, die Filter Art und Ort und die Suche. Darunter die Posten, je
Posten eine Zeile. Wer in der
Einheit Logistiker ist, klappt einen Posten auf und sieht die Einträge je
Besitzer; mit eingeschaltetem Buchen stehen an jedem Eintrag die Knöpfe zum
Ausbuchen, Verkaufen, Verschieben, für Notiz, Gestohlen-Marke und
Reservierungen, oben der Knopf zum Einbuchen.

⚠ **Die Formulare stehen in der Liste, kein eigenes Fenster.** Ein Fenster
darüber nähme den Fokus und verdeckte genau den Eintrag, um den es geht.

⚠ **Gebucht wird im eigenen Faden**, danach wird der Feed geholt und die Liste
neu gezeichnet. Was das Basetool ablehnt, steht als Satz über der Liste — je
Grund ein eigener.
"""
import threading
import tkinter as tk

from .language import t
from .pages import (BG, SURFACE, FG, SUB, ACCENT, LINE, RED, GOLD, _TK_CALLS,
                    _heading, _body_text, _button, _wrap,
                    _stripe, _stripe_reset, _card)
from . import basetool, basetool_sync, canvas_table
from . import exchange_orgstock as orgstock
from . import icons, materials, paths, stock_catalog, theme

PAGE = 'staffellager'
SETTING_UNIT = 'staffellager_einheit'
SETTING_VIEW = 'staffellager_sicht'

# Die Reiter wie in Mein Lager. Eine gespeicherte Sicht der früheren vier
# Reiter wird auf ihren neuen abgebildet.
VIEW_MATERIAL = 'material'
VIEW_ITEMS = 'items'
OLD_VIEWS = {orgstock.ITEMS: VIEW_ITEMS, orgstock.COMPONENTS: VIEW_ITEMS,
             orgstock.RAW: VIEW_MATERIAL, orgstock.TRADE: VIEW_MATERIAL}


def view_of(saved):
    """Die Sicht zu einem gespeicherten Wert — alte Werte eingeschlossen."""
    if saved in (VIEW_MATERIAL, VIEW_ITEMS):
        return saved
    return OLD_VIEWS.get(saved, VIEW_MATERIAL)

REASON_TEXTS = {code: 's_sl_r_' + code.lower() for code in orgstock.REASONS}


def builders():
    return {PAGE: orgstock_page}


# ------------------------------------------------------------- Helfer
def number(raw):
    """Eine eingegebene Zahl — Komma oder Punkt; `None`, wenn keine."""
    value = (raw or '').strip().replace(' ', '').replace(',', '.')
    if not value:
        return None
    try:
        return float(value)
    except ValueError:
        return None


def _amount(value, unit):
    """Eine Menge mit Einheit — dasselbe Format wie in Mein Lager."""
    from .my_stock_page import amount_text
    return amount_text(value, unit == orgstock.PIECE)


def _holder(entry):
    return t('s_sl_verborgen') if entry['withheld'] else (entry['holder']
                                                         or '?')


def _unit_label(unit):
    from . import exchange_orgunits
    return exchange_orgunits.label(unit)


def reason_text(code):
    key = REASON_TEXTS.get(code)
    return t(key) if key else t('s_sl_r_anders', code or '?')


def _canvas_of(widget):
    while widget is not None:
        if isinstance(widget, tk.Canvas):
            return widget
        widget = widget.master
    return None


# --------------------------------------------------------------- Seite
def data_key(data):
    """Was am Stand des Lagers die Seite verändert — `None` ohne Stand."""
    if data is None:
        return None
    return (data.get('at'), bool(data.get('write')), bool(data.get('details')),
            tuple(sorted(data.get('logistician') or ())),
            tuple(u['id'] for u in data.get('units') or ()))


def orgstock_page(window, frame):
    from .main_window import round_entry
    _heading(window, frame, t('hf_staffellager'), t('s_sl_lead'))
    area = canvas_table.Area(frame)
    inner = area.top
    page = {'unit': paths.setting(SETTING_UNIT) or '', 'area': area,
            'view': view_of(paths.setting(SETTING_VIEW)),
            'kind': '', 'place': '',
            'search': tk.StringVar(master=inner, value=''),
            'open': set(), 'form': None, 'values': {}, 'busy': False,
            'message': '', 'message_color': FG, 'confirm_url': None,
            'locations': None, 'targets': {}, 'materials': None}
    window.orgstock_page = page
    # Derselbe Einzug wie in Mein Lager: Die Rollfläche bringt ihn mit.
    top = tk.Frame(inner, bg=BG)
    top.pack(fill='x', pady=(10, 0))
    listing = tk.Frame(area.bottom, bg=BG)
    listing.pack(fill='x', pady=(0, 20))

    def redraw():
        try:
            if not top.winfo_exists():
                return
        except tk.TclError:
            return
        page['drawn_key'] = data_key(orgstock.current())
        canvas = _canvas_of(top)
        position = canvas.canvasy(0) if canvas is not None else 0
        if canvas is not None:
            canvas.yview_moveto(0)
        for child in top.winfo_children():
            child.destroy()
        _draw_top(window, top, page, redraw, redraw_list, round_entry)
        redraw_list()
        if canvas is not None and position > 0:
            def back():
                try:
                    canvas.update_idletasks()
                    area.layout()
                    height = float(canvas.cget('scrollregion').split()[3])
                    if height > 0:
                        canvas.yview_moveto(position / height)
                except (tk.TclError, IndexError, ValueError):
                    pass
            canvas.after_idle(back)

    def redraw_list():
        try:
            if not listing.winfo_exists():
                return
        except tk.TclError:
            return
        for child in listing.winfo_children():
            child.destroy()
        _draw_list(window, listing, page, redraw, redraw_list)
        area.refresh()

    page['redraw_all'] = redraw
    page['redraw_list'] = redraw_list
    page['search'].trace_add('write', lambda *_a: redraw_list())

    # Der Abgleich meldet sich mehrmals je Lauf. Neu gezeichnet wird nur, wenn
    # sich der Stand geändert hat; ein offenes Formular bleibt dabei stehen,
    # solange in der Einheit noch gebucht werden darf — nur die Liste wird neu.
    def on_change():
        data = orgstock.current()
        key = data_key(data)
        if key == page.get('drawn_key'):
            return
        if (page['form'] is not None and data is not None
                and orgstock.can_write(data, page['unit'])):
            page['drawn_key'] = key
            redraw_list()
        else:
            redraw()

    def on_status():
        _TK_CALLS.put(on_change)

    basetool_sync.LISTENERS.append(on_status)
    top.bind('<Destroy>', lambda _e: (
        on_status in basetool_sync.LISTENERS
        and basetool_sync.LISTENERS.remove(on_status)))
    redraw()


def _draw_top(window, top, page, redraw, redraw_list, round_entry):
    from .main_window import round_select
    data = orgstock.current()
    if data is None:
        _body_text(top, t('s_sl_warten'), window.f_small, fill='x')

        def now():
            basetool_sync.request_now()
            window.say(t('s_bt_laeuft'))
        _button(window, top, t('s_bt_jetzt'), now).pack(anchor='w',
                                                        pady=(8, 0))
        return
    units = data['units']
    if not units:
        _body_text(top, t('s_sl_keine_einheit'), window.f_small, fill='x')
        return
    if page['unit'] not in {u['id'] for u in units}:
        # Die gewählte Einheit ist weg: Ein Formular für sie und ein
        # laufendes Nachschlagen gelten nicht mehr.
        _drop_pending(page)
        page['unit'] = units[0]['id']
        page['form'] = None
        page['values'] = {}

    row = tk.Frame(top, bg=BG)
    row.pack(fill='x')
    if len(units) > 1:
        def pick(value):
            _drop_pending(page)
            page['unit'] = value
            page['open'].clear()
            page['form'] = None
            paths.set_setting(SETTING_UNIT, value)
            redraw()
        round_select(row, [(u['id'], _unit_label(u)) for u in units],
                     page['unit'], pick, window.f_small).pack(side='left')
    else:
        tk.Label(row, text=_unit_label(units[0]), bg=BG, fg=FG,
                 font=window.f_bold).pack(side='left')
    unit = page['unit']
    if orgstock.can_write(data, unit):
        _button(window, row, t('s_sl_einbuchen'),
                lambda: _open_form(page, ('book_in', unit)),
                strong=True, brand=theme.KRT_ORANGE).pack(side='right')

    logistician = unit in data['logistician']
    if logistician and not data['details']:
        _body_text(top, t('s_sl_details_fehlt'), window.f_small,
                   color=theme.YELLOW, fill='x', pady=(6, 0))
    elif logistician and not data['write']:
        _body_text(top, t('s_sl_buchen_aus'), window.f_small, fill='x',
                   pady=(6, 0))
    elif not logistician:
        # Ohne Logistiker-Rolle gibt es hier nichts zu buchen — der Hinweis
        # sagt, wie Sachen trotzdem ins Lager der Einheit kommen. Ist Buchen
        # schon eingeschaltet, hat das Basetool die Rolle verneint.
        _body_text(top, t('s_sl_kein_logistiker' if data['write']
                          else 's_sl_mitglied'),
                   window.f_small, fill='x', pady=(6, 0))

    _draw_filters(window, top, page, data, unit, redraw)
    box = round_entry(top, page['search'], window.f_small, theme.FIELD, LINE,
                      ACCENT, FG, placeholder=t('s_sl_suche'))
    box.holder.pack(fill='x', pady=(8, 0))

    # Fester Platz für die Meldung: `_say` füllt ihn, ohne die Seite neu zu
    # zeichnen.
    page['window'] = window
    page['message_slot'] = tk.Frame(top, bg=BG)
    page['message_slot'].pack(fill='x')
    _show_message(page)
    if page['confirm_url']:
        card = _card(top, pady=(8, 0))
        tk.Label(card, text=t('s_bt_masse'), bg=SURFACE, fg=FG,
                 font=window.f_bold, anchor='w').pack(fill='x', padx=16,
                                                      pady=(10, 0))
        hint = tk.Label(card, text=t('s_sl_masse_h'), bg=SURFACE, fg=SUB,
                        font=window.f_small, anchor='w', justify='left')
        hint.pack(fill='x', padx=16, pady=(4, 8))
        _wrap(hint, inset=32)
        url = page['confirm_url']
        _button(window, card, t('s_bt_masse_oeffnen'),
                lambda: paths.open_in_browser(url), strong=True,
                brand=theme.KRT_ORANGE).pack(anchor='w', padx=16,
                                             pady=(0, 12))

    # Fester Platz für das Einbuchen-Feld (`_place_form`).
    page['book_in_slot'] = tk.Frame(top, bg=BG)
    page['book_in_slot'].pack(fill='x')
    form = page['form']
    if form and form[0] == 'book_in' and not orgstock.can_write(data, unit):
        # Buchen geht hier nicht mehr — ein offenes Formular gilt nicht mehr.
        _drop_pending(page)
        page['form'] = None
    elif form and form[0] == 'book_in':
        holder = tk.Frame(page['book_in_slot'], bg=BG)
        holder.pack(fill='x', pady=(10, 0))
        page['form_holder'] = holder
        _form_book_in(window, holder, page, data, unit, redraw,
                      lambda: _place_form(page))


def visible_lots(page, data, unit):
    """Die Posten, die Reiter, Art, Ort und Suche durchlassen."""
    items = page['view'] == VIEW_ITEMS
    found = []
    for lot in orgstock.lots_of(data, unit, page['search'].get()):
        if orgstock.is_item(lot) != items:
            continue
        if page['kind'] and lot_kind(lot) != page['kind']:
            continue
        if page['place'] and (lot['place'] or '').lower() \
                != page['place'].lower():
            continue
        found.append(lot)
    return found


def _draw_filters(window, top, page, data, unit, redraw):
    """Reiter Material | Items und die Filter Art und Ort — wie in Mein
    Lager."""
    from .main_window import round_select
    from .pages import _choice
    from . import stock_kinds
    every = orgstock.lots_of(data, unit)
    counts = {VIEW_MATERIAL: 0, VIEW_ITEMS: 0}
    for lot in every:
        counts[VIEW_ITEMS if orgstock.is_item(lot) else VIEW_MATERIAL] += 1

    def choose_view(view):
        if view == page['view']:
            return
        page['view'] = view
        page['kind'] = ''
        paths.set_setting(SETTING_VIEW, view)
        redraw()
    tabs = _choice(window, top, [
        (VIEW_MATERIAL, '%s (%d)' % (t('s_ml_material'),
                                     counts[VIEW_MATERIAL])),
        (VIEW_ITEMS, '%s (%d)' % (t('s_ml_items'), counts[VIEW_ITEMS]))],
        page['view'], choose_view)
    tabs.pack(fill='x', pady=(10, 0))
    page['tabs'] = tabs
    items = page['view'] == VIEW_ITEMS
    here = [lot for lot in every if orgstock.is_item(lot) == items]
    present = {lot_kind(lot) for lot in here}
    if page['kind'] and page['kind'] not in present:
        page['kind'] = ''
    places = sorted({lot['place'] for lot in here if lot['place']},
                    key=str.lower)
    if page['place'] and page['place'] not in places:
        page['place'] = ''
    kinds = stock_kinds.ITEM_KINDS if items else stock_kinds.MATERIAL_KINDS
    line = tk.Frame(top, bg=BG)
    line.pack(fill='x', pady=(8, 0))
    page['filters'] = {}

    def pick(key):
        def run(value):
            page[key] = value
            page['redraw_list']()
        return run
    for key, choices in (
            ('kind', [('', t('s_ml_alle_arten'))]
             + [(k, stock_kinds.label(k)) for k in kinds if k in present]),
            ('place', [('', t('s_ml_alle_orte'))] + [(p, p) for p in places])):
        select = round_select(line, choices, page[key], pick(key),
                              window.f_small)
        select.pack(side='left', padx=(0, 8))
        page['filters'][key] = select


def _draw_list(window, listing, page, redraw, redraw_list):
    _drop_entry_form(page)
    page['lot_rows'] = []
    page['entry_rows'] = []
    page['row_widgets'] = []
    page['folds'] = {}
    page['entry_slots'] = {}
    page.pop('table', None)
    page.pop('builder', None)
    page['area'].set_table(None)
    data = orgstock.current()
    if data is None or not data['units']:
        return
    unit = page['unit']
    lots = visible_lots(page, data, unit)
    if not lots:
        _body_text(listing, t('s_sl_leer'), window.f_small, fill='x')
        return
    writable = orgstock.can_write(data, unit)
    materials_view = page['view'] == VIEW_MATERIAL
    columns = ['name', 'place'] + (['quality'] if materials_view else []) \
        + ['amount', 'action']
    heads = {'name': 's_ml_sp_bestand', 'place': 's_ml_sp_ort',
             'quality': 's_ml_sp_q', 'amount': 's_ml_sp_menge'}
    page['window'] = window
    page['builder'] = {'columns': columns}
    lines, measured = [], []
    for index, lot in enumerate(lots):
        head = _lot_row(window, page, data, lot, writable, columns, index)
        lines.append(head)
        measured.append(head)
        fold = page['folds'].get(lot['key'])
        if fold is None:
            continue
        measured.extend(fold['specs'])
        if lot['key'] in page['open']:
            fold['built'] = True
            lines.extend(fold['specs'])
    table = canvas_table.Table(
        page['area'], window,
        [canvas_table.Column(key, t(heads[key]) if key in heads else '',
                             'w' if key in ('name', 'place') else 'e')
         for key in columns],
        color_of=lambda line, _n: SURFACE if line.lot_index % 2 else BG)
    table.measure(measured)
    table.show(lines)
    page['table'] = table
    page['area'].set_table(table)
    _sync_rows(page)
    _body_text(listing, t('s_sl_hinweis'), window.f_small, fill='x',
               pady=(16, 0))
    form = page.get('form')
    if form and form[0] != 'book_in' and form[1] in page['entry_slots']:
        _entry_form(window, page, page['entry_slots'][form[1]], form,
                    page['redraw_all'], lambda: _place_form(page))


def _sync_rows(page):
    """`row_widgets`, `lot_rows` und `entry_rows` aus der Tabelle, in
    Tabellenreihenfolge; `entry_slots` nur für gezeigte Einträge."""
    table = page.get('table')
    lines = table.data_rows() if table is not None else []
    page['row_widgets'] = [(line.kind, line.what, line) for line in lines]
    page['lot_rows'] = [(line.what, line) for line in lines
                        if line.kind == 'lot']
    page['entry_rows'] = [(line.what[1], line) for line in lines
                          if line.kind == 'entry']
    page['entry_slots'] = {
        line.what[1]['id']: {'line': line, 'lot': line.what[0],
                             'entry': line.what[1], 'data': line.data}
        for line in lines if line.kind == 'entry'}


def lot_kind(lot):
    """Die Art eines Postens (`stock_kinds`) — bei Materialien aus dem
    `materialKind`, das der Posten mitbringt."""
    from . import stock_kinds
    if orgstock.is_item(lot):
        return stock_kinds.item_kind(lot['name'])
    return stock_kinds.material_kind(lot['name'],
                                     {lot['name'].lower(): lot['kind']})


def _lot_row(window, page, data, lot, writable, columns, index):
    """Die Zeile eines Postens als `canvas_table.Row`. Hat er Einträge,
    stehen sie fertig beschrieben in `page['folds'][key]['specs']`; ein
    Klick klappt nur sie um, die übrige Liste bleibt stehen."""
    from . import stock_kinds
    expandable = lot['entries'] is not None
    is_open = lot['key'] in page['open']
    item = orgstock.is_item(lot)
    badges = [(stock_kinds.label(lot_kind(lot)), SUB)]
    if lot['stolen']:
        badges.append((t('s_sl_gestohlen'), GOLD))
    reserved = []
    if lot['job'] > 0:
        reserved.append(t('s_sl_res_auftraege') % _amount(lot['job'],
                                                          lot['unit_q']))
    if lot['mission'] > 0:
        reserved.append(t('s_sl_res_missionen') % _amount(lot['mission'],
                                                          lot['unit_q']))
    values = {
        'name': (canvas_table.Name(
            lot['name'], badges,
            stock_kinds.tag(lot['name']) if item else None,
            extra=' · '.join(reserved),
            color=GOLD if lot['stolen'] else FG), FG),
        'place': (lot['place'] or '?', SUB),
        'quality': ('' if item else '%d' % lot['quality'], SUB),
        'amount': (_amount(lot['amount'], lot['unit_q']), FG),
    }
    if expandable:
        values['action'] = (canvas_table.Icons(
            [('zuklappen' if is_open else 'aufklappen', None, None)],
            right=10), SUB)

    def toggle():
        fold = page.get('folds', {}).get(lot['key'])
        table = page.get('table')
        if fold is None or table is None or fold['head'] not in table.rows:
            if lot['key'] in page['open']:
                page['open'].discard(lot['key'])
            else:
                page['open'].add(lot['key'])
            page['redraw_list']()
            return
        show = not fold['built']
        if show:
            page['open'].add(lot['key'])
        else:
            page['open'].discard(lot['key'])
        _lot_fold_show(page, fold, show)

    head = canvas_table.Row(
        'lot', lot, {k: v for k, v in values.items() if k in columns},
        on_click=toggle if expandable else None)
    head.lot_index = index
    if not expandable:
        return head
    if lot['entries']:
        specs = [_entry_row(page, data, lot, entry, writable, columns, index)
                 for entry in lot['entries']]
    else:
        empty = canvas_table.Row('empty', lot, {'name': (canvas_table.Name(
            t('s_sl_keine_eintraege'), indent=True, color=SUB), SUB)})
        empty.lot_index = index
        specs = [empty]
    page['folds'][lot['key']] = {'lot': lot, 'head': head, 'specs': specs,
                                 'data': data, 'writable': writable,
                                 'built': False}
    return head


def _lot_fold_show(page, fold, show):
    """Nur die Einträge dieses Postens zeigen oder wegnehmen und den Pfeil
    tauschen — die übrige Liste bleibt stehen. Die Einträge tragen den Grund
    ihres Postens, die Streifen der Posten verschieben sich dabei nicht. Ein
    offenes Formular an einem der Einträge steht beim Aufklappen wieder
    darunter."""
    if show == fold['built']:
        return
    table = page.get('table')
    if table is None or fold['head'] not in table.rows:
        return
    if show:
        table.insert(fold['head'], fold['specs'])
    else:
        slot = page.get('form_slot')
        if slot is not None and slot in table.rows:
            at = table.rows.index(slot)
            if at and table.rows[at - 1] in fold['specs']:
                _drop_entry_form(page)
        table.remove(fold['specs'])
    fold['built'] = show
    head = fold['head']
    if head.arrow is not None:
        head.arrow.swap_symbol('zuklappen' if show else 'aufklappen')
    if 'action' in head.cells:
        head.cells['action'][0].entries[0] = (
            'zuklappen' if show else 'aufklappen', None, None)
    _sync_rows(page)
    form = page.get('form')
    if show and form and form[0] != 'book_in' \
            and form[1] in page['entry_slots'] \
            and page.get('form_slot') is None and page.get('window'):
        _entry_form(page['window'], page, page['entry_slots'][form[1]], form,
                    page['redraw_all'], lambda: _place_form(page))


# Die Buchungen am Eintrag: Art, Symbol, Erklärtext.
ENTRY_ACTIONS = (('book_out', 'ausbuchen', 's_sl_ausbuchen'),
                 ('sell', 'verkaufen', 's_sl_verkaufen'),
                 ('move', 'umbuchen', 's_sl_verschieben'),
                 ('note', 'notiz', 's_sl_notiz'),
                 ('stolen', 'gestohlen', 's_sl_marke'),
                 ('earmark', 'reservieren', 's_sl_reservieren'))


def _entry_row(page, data, lot, entry, writable, columns, index):
    """Ein Eintrag als `canvas_table.Row`: Besitzer, Notiz und
    Reservierungen klein dahinter, die Menge, rechts die Buchungen als
    Symbole (grün das des offenen Formulars)."""
    details = []
    if entry['note']:
        details.append(t('s_sl_notiz_text') % entry['note'])
    for job in entry['jobs']:
        label = '#%s' % job['number'] if job['number'] is not None \
            else t('s_bd_auftrag')
        details.append(t('s_sl_fuer_auftrag') % (
            _amount(job['amount'], lot['unit_q']), label)
            + (' ' + t('s_sl_geliefert') if job['delivered'] else ''))
    for mission in entry['missions']:
        details.append(t('s_sl_fuer_mission') % (
            _amount(mission['amount'], lot['unit_q']),
            mission['name'] or '?'))
    values = {
        'name': (canvas_table.Name(_holder(entry), [], indent=True,
                                   extra=' · '.join(details)), FG),
        'amount': (_amount(entry['amount'], lot['unit_q']), FG),
    }
    if writable:
        form = page['form']
        values['action'] = (canvas_table.Icons(
            [(symbol, lambda k=kind: _open_form(page, (k, entry['id'])),
              lambda k=key: t(k),
              icons.GREEN if (form and form[0] == kind
                              and form[1] == entry['id']) else icons.GREY,
              kind)
             for kind, symbol, key in reversed(ENTRY_ACTIONS)],
            right=10), SUB)
    line = canvas_table.Row('entry', (lot, entry),
                            {k: v for k, v in values.items() if k in columns})
    line.lot_index = index
    line.data = data
    return line


def _entry_form(window, page, spot, form, redraw, redraw_list):
    """Das Formular eines Eintrags bauen und direkt unter seine Zeile
    setzen."""
    holder = tk.Frame(page['area'].canvas, bg=BG)
    page['form_holder'] = holder
    form_builder = {'book_out': _form_take, 'sell': _form_take,
                    'move': _form_move, 'note': _form_note,
                    'stolen': _form_stolen, 'earmark': _form_earmark}[form[0]]
    form_builder(window, holder, page, spot['data'], spot['lot'],
                 spot['entry'], form[0], redraw, redraw_list)
    page['form_slot'] = page['table'].insert_widget(
        spot['line'], holder, pady=(4, 8), padx=(36, 8))


def _drop_entry_form(page):
    """Das Formular an einem Eintrag abbauen; welches offen ist
    (`page['form']`), bleibt gemerkt."""
    slot = page.get('form_slot')
    if slot is None:
        return
    page['form_slot'] = None
    if page.get('form_holder') is slot.widget:
        page['form_holder'] = None
    table = page.get('table')
    if table is not None and slot in table.rows:
        table.remove([slot])
    try:
        if slot.widget.winfo_exists():
            slot.widget.destroy()
    except tk.TclError:
        pass


def _open_form(page, form):
    """Ein Formular auf- oder (beim zweiten Klick) wieder zuklappen. Das
    Einbuchen steht oben, die übrigen am Eintrag in der Liste."""
    _drop_pending(page)
    page['form'] = None if page['form'] == form else form
    page['values'] = {}
    page['message'] = ''
    _place_form(page)


def _place_form(page):
    """Das offene Formular an seiner Stelle neu bauen oder abbauen — oben
    das Einbuchen, sonst in der freien Zeile unter seinem Eintrag. Liste und
    Kopf bleiben stehen; die Meldung und die Farbe der Symbole folgen. Ist
    die Stelle nicht gebaut, wird die Seite neu gezeichnet."""
    _drop_entry_form(page)
    holder = page.get('form_holder')
    page['form_holder'] = None
    try:
        if holder is not None and holder.winfo_exists():
            holder.destroy()
    except tk.TclError:
        pass
    _paint_form_icons(page)
    _show_message(page)
    form = page['form']
    if form is None:
        return
    data = orgstock.current()
    window = page.get('window')
    try:
        if form[0] == 'book_in':
            slot = page.get('book_in_slot')
            if data is None or window is None or slot is None \
                    or not slot.winfo_exists() \
                    or not orgstock.can_write(data, form[1]):
                page['redraw_all']()
                return
            holder = tk.Frame(slot, bg=BG)
            holder.pack(fill='x', pady=(10, 0))
            page['form_holder'] = holder
            _form_book_in(window, holder, page, data, form[1],
                          page['redraw_all'], lambda: _place_form(page))
            return
        spot = page.get('entry_slots', {}).get(form[1])
        table = page.get('table')
        if window is None or spot is None or table is None \
                or not table.alive:
            page['redraw_list']()
            return
        _entry_form(window, page, spot, form, page['redraw_all'],
                    lambda: _place_form(page))
    except tk.TclError:
        page['redraw_all']()


def _paint_form_icons(page):
    """Das Symbol des offenen Formulars grün, die übrigen grau."""
    form = page.get('form')

    def color(kind, entry):
        active = bool(form and form[0] == kind and form[1] == entry['id'])
        return icons.GREEN if active else icons.GREY
    for _lot, fold in page.get('folds', {}).items():
        for line in fold['specs']:
            if line.kind != 'entry' or 'action' not in line.cells:
                continue
            spec = line.cells['action'][0]
            spec.entries = [e[:3] + (color(e[4], line.what[1]), e[4])
                            for e in spec.entries]
    for entry, line in page.get('entry_rows', []):
        for kind, icon in line.action_icons.items():
            try:
                icon.recolor(color(kind, entry))
            except (tk.TclError, AttributeError):
                pass


def _show_message(page):
    """Die Meldung (`page['message']`) in ihren festen Platz oben schreiben —
    ohne die Seite neu zu zeichnen."""
    slot = page.get('message_slot')
    try:
        if slot is None or not slot.winfo_exists():
            return
        for child in slot.winfo_children():
            child.destroy()
        if page['message']:
            line = tk.Label(slot, text=page['message'], bg=BG,
                            fg=page['message_color'],
                            font=page['window'].f_small, anchor='w',
                            justify='left')
            line.pack(fill='x', pady=(8, 0))
            _wrap(line)
    except tk.TclError:
        pass
    area = page.get('area')
    if area is not None:
        area.refresh()


# ------------------------------------------------------------- Formulare
class _Form:
    """Ein Formular in der Liste: Felder untereinander, unten die Knöpfe.

    Die Werte liegen in `page['values']`, damit ein Neuzeichnen (etwa nach
    einem Wechsel der Art beim Verschieben) nichts Eingetipptes verliert."""

    # ⚠ Seitengrund, nicht `SURFACE`: Die Auswahlliste mit Tippen
    # (`pages._combo_box`) zeichnet sich auf `BG`; auf einer helleren Karte
    # stünde sie als dunkler Streifen.
    BG = BG

    def __init__(self, window, parent, page, title):
        from .main_window import round_frame
        self.window, self.page = window, page
        self.card = round_frame(parent, self.BG, theme.KRT_ORANGE, radius=8,
                                base_color=BG)
        self.card.holder.pack(fill='x')
        tk.Label(self.card, text=title, bg=self.BG, fg=FG,
                 font=window.f_bold, anchor='w').pack(fill='x', padx=14,
                                                      pady=(10, 4))
        self.vars = {}

    def _row(self, label):
        row = tk.Frame(self.card, bg=self.BG)
        row.pack(fill='x', padx=14, pady=3)
        tk.Label(row, text=label, bg=self.BG, fg=SUB,
                 font=self.window.f_small, width=18,
                 anchor='w').pack(side='left')
        return row

    def field(self, key, label, default='', placeholder=None, width=None):
        from .main_window import round_entry
        value = self.page['values'].get(key, default)
        # ⚠ Der Vorgabewert gehört gleich in die Werte — sonst gilt ein
        # unverändert abgeschicktes Feld als leer.
        self.page['values'][key] = value
        var = tk.StringVar(master=self.card, value=value)
        var.trace_add('write', lambda *_a, k=key, v=var:
                      self.page['values'].__setitem__(k, v.get()))
        row = self._row(label)
        box = round_entry(row, var, self.window.f_small, theme.FIELD, LINE,
                          ACCENT, FG, width=width, placeholder=placeholder)
        box.holder.pack(side='left', fill='x', expand=width is None)
        self.vars[key] = var
        return var

    def select(self, key, label, entries, default='', on_change=None):
        from .main_window import round_select
        value = self.page['values'].get(key, default)
        if entries and value not in {v for v, _ in entries}:
            value = entries[0][0]
        self.page['values'][key] = value
        row = self._row(label)

        def pick(new):
            self.page['values'][key] = new
            if on_change:
                on_change(new)
        round_select(row, entries, value, pick, self.window.f_small,
                     bg=self.BG).pack(side='left')
        return value

    def toggle(self, key, label, default=False):
        from .main_window import toggle_switch
        value = bool(self.page['values'].get(key, default))
        self.page['values'][key] = value
        # Schalter rechts, Beschriftung links über die volle Breite — wie in
        # jeder Einstellungsliste des Programms.
        row = tk.Frame(self.card, bg=self.BG)
        row.pack(fill='x', padx=14, pady=3)

        def flip():
            self.page['values'][key] = not self.page['values'].get(key)
            return self.page['values'][key]
        toggle_switch(row, value, flip, bg=self.BG).pack(side='right')
        tk.Label(row, text=label, bg=self.BG, fg=SUB,
                 font=self.window.f_small, anchor='w').pack(
                     side='left', fill='x', expand=True)

    def note(self, text, color=SUB):
        line = tk.Label(self.card, text=text, bg=self.BG, fg=color,
                        font=self.window.f_small, anchor='w', justify='left')
        line.pack(fill='x', padx=14, pady=(2, 2))
        _wrap(line, inset=28)
        return line

    def buttons(self, entries):
        row = tk.Frame(self.card, bg=self.BG)
        row.pack(fill='x', padx=14, pady=(8, 12))
        for text, action, kind in entries:
            _button(self.window, row, text, action, strong=kind == 'strong',
                    danger=kind == 'danger',
                    brand=theme.KRT_ORANGE).pack(side='left', padx=(0, 8))

    def value(self, key, default=''):
        return self.page['values'].get(key, default)


def _cancel(page, _redraw=None):
    """Abbrechen: nur das Formular schließen (`_place_form`)."""
    def run():
        _drop_pending(page)
        page['form'] = None
        page['values'] = {}
        _place_form(page)
    return run


def _earmark_fields(form, lot, entry):
    """Je Reservierung des Eintrags ein Mengenfeld (`s_sl_davon_aus`) —
    leer heißt 0."""
    for job in entry['jobs']:
        label = '#%s' % job['number'] if job['number'] is not None \
            else t('s_bd_auftrag')
        form.field('job:' + job['id'], t('s_sl_davon_aus') % label,
                  placeholder=_amount(job['amount'], lot['unit_q']), width=12)
    for mission in entry['missions']:
        form.field('mission:' + mission['id'],
                  t('s_sl_davon_aus') % (mission['name'] or '?'),
                  placeholder=_amount(mission['amount'], lot['unit_q']),
                  width=12)


def _earmark_values(page, entry):
    jobs, missions = {}, {}
    for job in entry['jobs']:
        amount = number(page['values'].get('job:' + job['id']))
        if amount:
            jobs[job['id']] = amount
    for mission in entry['missions']:
        amount = number(page['values'].get('mission:' + mission['id']))
        if amount:
            missions[mission['id']] = amount
    return jobs, missions


def _amount_ok(page, form, value, unit, maximum=None, optional=False):
    """Die eingetippte Menge prüfen — `None` mit Meldung, wenn sie nicht
    taugt."""
    if value is None:
        if optional:
            return 0.0
        _say(page, t('s_sl_menge_fehlt'), RED)
        return None
    if value <= 0 or (unit == orgstock.PIECE and value != int(value)):
        _say(page, t('s_sl_menge_falsch'), RED)
        return None
    if maximum is not None and value > maximum + 1e-9:
        _say(page, t('s_sl_menge_zu_viel') % _amount(maximum, unit), RED)
        return None
    return value


def _say(page, text, color=FG, form=False):
    """Eine Meldung oben zeigen — nur an ihrem Platz. `form`: das offene
    Formular dazu neu bauen (es zeigt dann etwa Vorschläge). Ohne gebauten
    Platz wird die Seite neu gezeichnet."""
    page['message'] = text
    page['message_color'] = color
    slot = page.get('message_slot')
    try:
        drawn = slot is not None and slot.winfo_exists()
    except tk.TclError:
        drawn = False
    if not drawn:
        redraw = page.get('redraw_all')
        if redraw:
            redraw()
        return
    if form:
        _place_form(page)
    else:
        _show_message(page)


def _form_take(window, parent, page, data, lot, entry, kind, redraw,
               redraw_list):
    selling = kind == 'sell'
    form = _Form(window, parent, page,
                 t('s_sl_verkaufen_titel' if selling else 's_sl_ausbuchen_titel')
                 % (lot['name'], _holder(entry)))
    free = min(entry['free_jobs'], entry['free_missions'])
    form.field('amount', t('s_sl_menge'),
              default=materials.amount_text(free, piece=lot['unit_q']
                                            == orgstock.PIECE) if free > 0
              else '', width=12)
    form.note(t('s_sl_frei') % _amount(free, lot['unit_q']))
    if entry['jobs'] or entry['missions']:
        form.note(t('s_sl_aus_reservierung_h'))
        _earmark_fields(form, lot, entry)
    if selling:
        form.field('terminal', t('s_sl_terminal'),
                  placeholder=t('s_sl_terminal_h'))
        form.field('proceeds', t('s_sl_erloes'), placeholder=t('s_sl_auec'),
                  width=14)

    def go():
        amount = _amount_ok(page, form, number(form.value('amount')),
                            lot['unit_q'], maximum=entry['amount'])
        if amount is None:
            return
        jobs, missions = _earmark_values(page, entry)
        if selling:
            terminal = (form.value('terminal') or '').strip()
            proceeds = number(form.value('proceeds'))
            if not terminal or proceeds is None or proceeds < 0:
                _say(page, t('s_sl_verkauf_fehlt'), RED)
                return
            op = orgstock.op_sell(lot['unit']['id'], entry, amount,
                                  lot['unit_q'], terminal, proceeds, jobs,
                                  missions)
        else:
            op = orgstock.op_book_out(lot['unit']['id'], entry, amount,
                                      lot['unit_q'], jobs, missions)
        _submit(window, page, data, [op], redraw)

    form.buttons([(t('s_sl_verkaufen' if selling else 's_sl_ausbuchen'), go,
                   'strong'),
                  (t('s_bt_abbrechen'), _cancel(page, redraw_list), '')])


def _location_choices(page):
    """Die Lagerorte des Basetools — im Hintergrund geholt, bis dahin leer."""
    if page['locations'] is None:
        page['locations'] = []
        page['locations_loading'] = True

        def work():
            try:
                places = basetool_sync._locations(basetool.CONNECTION)
                page['locations'] = sorted(
                    (v.get('name') for v in places.values() if v.get('name')),
                    key=str.lower)
            except Exception as exc:
                from . import errors
                errors.record('orgstock_page.locations', exc)
                page['locations'] = []
            page['locations_loading'] = False

            # Im Tk-Faden gelesen, damit es auch ein eben gebautes Feld trifft.
            def loaded():
                done = page.get('locations_done')
                if done:
                    done()
            _TK_CALLS.put(loaded)
        threading.Thread(target=work, daemon=True).start()
    return page['locations']


def _location_field(window, form, page, key, label):
    """Lagerort tippen oder aussuchen; geht als Name hinaus, das Basetool
    löst ihn auf (`LOCATION_UNKNOWN`, wenn es ihn nicht kennt)."""
    from .pages import _combo_box
    _location_choices(page)
    value = page['values'].get(key, '')
    var = tk.StringVar(master=form.card, value=value)
    var.trace_add('write', lambda *_a: page['values'].__setitem__(key,
                                                                   var.get()))
    row = form._row(label)
    holder = tk.Frame(row, bg=form.BG)
    holder.pack(side='left', fill='x', expand=True)
    line, drop, refresh = _combo_box(
        window, holder, var,
        lambda: list(page['locations'] or ()),
        placeholder=t('s_sl_ort_h'))
    line.pack(fill='x')
    drop.pack(fill='x')
    if page.get('locations_loading'):
        note = form.note(t('s_sl_orte_laden'))

        # Fertig geladen: nur der Hinweis geht, die Liste zeigt die Orte —
        # kein Neuzeichnen, das Getipptes und den Fokus nähme.
        def done():
            try:
                if note.winfo_exists():
                    note.destroy()
                if line.winfo_exists():
                    refresh()
            except tk.TclError:
                pass
        page['locations_done'] = done


def _material_choices(page):
    """Die Warenliste (`stock_catalog`) — im Hintergrund aus der Ablage
    gelesen und, wenn fällig, bei UEX geholt. Bis dahin leer; ein älterer
    Stand gilt schon, während der neue geholt wird."""
    if page['materials'] is None:
        page['materials'] = {}
        page['materials_loading'] = True

        # Läuft im Tk-Faden und liest `materials_done` erst dann — so greift
        # es auch für ein Feld, das gerade erst gebaut wird.
        def loaded():
            done = page.get('materials_done')
            if done:
                done()

        def work():
            try:
                found = stock_catalog.choices()
                if found:
                    page['materials'] = found
                    page['materials_loading'] = False
                    _TK_CALLS.put(loaded)
                if stock_catalog.due():
                    stock_catalog.update()
                    page['materials'] = stock_catalog.choices()
            except Exception as exc:
                from . import errors
                errors.record('orgstock_page.materials', exc)
            page['materials_loading'] = False
            _TK_CALLS.put(loaded)
        threading.Thread(target=work, daemon=True).start()
    return page['materials']


def _material_names(page, data):
    """Alle Namen für die Auswahl: die Warenliste und die Waren, die schon in
    einem Pool liegen — sortiert, je Name einmal."""
    lots = (data or {}).get('lots') or {}
    key = (id(page['materials']), len(lots))
    if page.get('materials_key') != key:
        names = {e['name'].lower(): e['name']
                 for e in page['materials'].values()}
        for lot in lots.values():
            if lot['name']:
                names.setdefault(lot['name'].lower(), lot['name'])
        page['materials_names'] = sorted(names.values(), key=str.lower)
        page['materials_key'] = key
    return page['materials_names']


def _material_field(window, form, page, data, redraw):
    """Ware tippen oder aus der Warenliste aussuchen. Eine ausgewählte Ware
    stellt die Mengeneinheit passend: Gegenstand -> Stück, Ware -> SCU. Hat
    das Basetool beim Nachschlagen mehrere Treffer genannt, stehen sie
    darunter zum Anklicken."""
    from .pages import _combo_box
    _material_choices(page)
    var = tk.StringVar(master=form.card, value=page['values'].get('material',
                                                                  ''))
    # Die Treffer-Liste unten: Weicht der Text vom Namen der Anfrage ab,
    # verschwindet sie sofort — ohne Neuzeichnen, das den Fokus nähme.
    shown = []

    def typed(*_a):
        text = var.get()
        page['values']['material'] = text
        if ('candidates' in page['values'] and text.strip().lower()
                != page['values'].get('candidates_for')):
            page['values'].pop('candidates', None)
            for widget in shown:
                try:
                    widget.destroy()
                except tk.TclError:
                    pass
            shown[:] = []
    var.trace_add('write', typed)
    row = form._row(t('s_sl_ware'))
    holder = tk.Frame(row, bg=form.BG)
    holder.pack(side='left', fill='x', expand=True)

    def extra(name):
        entry = page['materials'].get(name.lower())
        return stock_catalog.label(entry) if entry else ''

    def pick(name):
        changed = page['values'].pop('candidates', None) is not None
        var.set(name)
        changed = _drop_pending(page) or changed
        page['values'].pop('material_ref', None)
        entry = page['materials'].get(name.lower())
        if entry is not None and (entry['item'] is None
                                  or entry['material'] is None):
            unit = orgstock.PIECE if entry['material'] is None \
                else orgstock.SCU
            if page['values'].get('unit') != unit:
                page['values']['unit'] = unit
                changed = True
        if changed:
            form.card.after_idle(redraw)

    # Solange die Warenliste lädt, steht statt der Leermeldung der Ladehinweis.
    line, drop, refresh = _combo_box(
        window, holder, var, lambda: _material_names(page, data),
        on_pick=pick, extra=extra, placeholder=t('s_sl_ware_h'),
        empty_text=lambda: (t('s_sl_waren_laden')
                            if page.get('materials_loading') else None))
    line.pack(fill='x')
    drop.pack(fill='x')
    if page.get('materials_loading'):
        note = form.note(t('s_sl_waren_laden'))

        # Fertig geladen: nur der Hinweis geht und die Liste zeigt die Namen —
        # kein Neuzeichnen, das Getipptes und den Fokus nähme.
        def done():
            try:
                if not page.get('materials_loading') and note.winfo_exists():
                    note.destroy()
                if line.winfo_exists():
                    refresh()
            except tk.TclError:
                pass
        page['materials_done'] = done

    candidates = page['values'].get('candidates') or []
    if not candidates:
        return
    shown.append(form.note(t('s_sl_ware_vorschlaege'), theme.YELLOW))
    rows = tk.Frame(form.card, bg=form.BG)
    rows.pack(fill='x', padx=14, pady=(0, 4))
    shown.append(rows)
    _stripe_reset(rows)

    def choose(candidate):
        _drop_pending(page)
        page['values']['material'] = candidate['name']
        page['values']['material_ref'] = dict(candidate)
        page['values'].pop('candidates', None)
        page['message'] = ''
        redraw()

    for candidate in candidates:
        base = _stripe(rows, form.BG)
        hit = tk.Frame(rows, bg=base, cursor='hand2')
        hit.pack(fill='x', ipady=3)
        label = tk.Label(hit, text=candidate['name'], bg=base, fg=FG,
                         font=window.f_small, anchor='w', cursor='hand2',
                         padx=8)
        label.pack(side='left')
        kind = tk.Label(hit, text=stock_catalog.kind_label(
                            candidate.get('kind')), bg=base, fg=SUB,
                        font=window.f_small, anchor='e', cursor='hand2',
                        padx=8)
        kind.pack(side='right')
        for widget in (hit, label, kind):
            widget.bind('<Button-1>', lambda _e, c=candidate: choose(c))
            widget.bind('<Enter>', lambda _e, w=label: w.configure(fg=ACCENT))
            widget.bind('<Leave>', lambda _e, w=label: w.configure(fg=FG))


def _drop_pending(page):
    """Ein laufendes Nachschlagen verwerfen: Seine Antwort bucht nicht mehr,
    und die Seite ist wieder frei. `True`, wenn eines lief."""
    if page.get('resolve_ticket') is None:
        return False
    page['resolve_ticket'] = None
    page['busy'] = False
    page['message'] = ''
    return True


# Was im Formular steht, ohne das, was erst das Nachschlagen dazuschreibt.
_RESOLVE_KEYS = ('candidates', 'candidates_for', 'material_ref')


def _form_state(values):
    return {k: v for k, v in values.items() if k not in _RESOLVE_KEYS}


def _resolve_then(page, name, known, piece, form_key, snapshot, book):
    """Die Ware beim Basetool nachschlagen, bevor gebucht wird: eindeutig ->
    `book({bt, name, kind})`, mehrdeutig -> Auswahl im Formular, unbekannt ->
    Hinweis. In den letzten beiden Fällen wird nichts gesendet.

    ⚠ Die Antwort bucht nur, solange ihre Marke (`resolve_ticket`) gilt, das
    Formular `form_key` noch offen ist, in der Einheit noch gebucht werden
    darf und das Formular noch so dasteht wie beim Klick (`snapshot`).
    Abbrechen, ein anderes Formular, ein Einheitswechsel oder eine andere
    Auswahl (`_drop_pending`) nehmen die Marke weg."""
    if page['busy']:
        _say(page, t('s_sl_noch_dran'), SUB)
        return
    ticket = object()
    page['resolve_ticket'] = ticket
    page['busy'] = True
    _say(page, t('s_sl_ware_pruefen'), SUB)

    def work():
        status, found, error = None, None, ''
        try:
            status, found = orgstock.resolve_material(basetool.CONNECTION,
                                                      name, known, piece)
        except basetool.ApiError as exc:
            from . import errors
            from .basetool_page import error_text
            errors.record('orgstock_page.resolve', RuntimeError(exc.describe()))
            error = error_text(exc.code)
        except Exception as exc:
            from . import errors
            errors.record('orgstock_page.resolve', exc)
            error = t('s_sl_r_anders', type(exc).__name__)

        def show():
            if page.get('resolve_ticket') is not ticket:
                return
            page['resolve_ticket'] = None
            page['busy'] = False
            if page['form'] != form_key:
                _say(page, '', FG)
                return
            current = orgstock.current()
            if current is None or not orgstock.can_write(current,
                                                         form_key[1]):
                _say(page, t('s_sl_nicht_mehr'), RED)
                return
            if _form_state(page['values']) != _form_state(snapshot):
                _say(page, t('s_sl_ware_geaendert'), theme.YELLOW)
                return
            if status == 'resolved':
                page['values']['material_ref'] = found
                page['values'].pop('candidates', None)
                book(found)
            elif status == 'ambiguous':
                page['values']['candidates'] = found
                page['values']['candidates_for'] = name.strip().lower()
                _say(page, t('s_sl_ware_mehrdeutig') % name, theme.YELLOW,
                     form=True)
            elif status == 'unmatched':
                page['values'].pop('candidates', None)
                _say(page, t('s_sl_ware_unbekannt') % name, RED, form=True)
            else:
                _say(page, error, RED)
        _TK_CALLS.put(show)

    threading.Thread(target=work, daemon=True).start()


def _form_move(window, parent, page, data, lot, entry, kind, redraw,
               redraw_list):
    form = _Form(window, parent, page, t('s_sl_verschieben_titel')
                 % (lot['name'], _holder(entry)))
    writable_units = [u for u in data['units']
                      if u['id'] in data['logistician']]
    modes = [('location', t('s_sl_nach_ort')),
             ('holder', t('s_sl_an_mitglied'))]
    if len(writable_units) > 1:
        modes.append(('pool', t('s_sl_in_einheit')))
    mode = form.select('mode', t('s_sl_wie'), modes,
                       on_change=lambda _v: redraw_list())
    piece = lot['unit_q'] == orgstock.PIECE
    if mode == 'pool':
        others = [(u['id'], _unit_label(u)) for u in writable_units
                  if u['id'] != lot['unit']['id']]
        form.select('to_unit', t('s_sl_ziel_einheit'), others)
        form.field('amount', t('s_sl_menge'), width=12,
                  placeholder=t('s_sl_alles'))
        form.note(t('s_sl_einheit_h'))
    else:
        if mode == 'holder':
            units = [(u['id'], _unit_label(u)) for u in writable_units]
            to_unit = form.select('to_unit', t('s_sl_ziel_einheit'), units,
                                  default=lot['unit']['id'],
                                  on_change=lambda _v: redraw_list())
            roster = data['rosters'].get(to_unit) or []
            if roster:
                form.select('holder', t('s_sl_besitzer'),
                            [(m['ref'], m['name']) for m in roster])
            else:
                form.note(t('s_sl_keine_mitglieder'), theme.YELLOW)
        else:
            _location_field(window, form, page, 'location', t('s_sl_ort'))
        free = min(entry['free_jobs'], entry['free_missions'])
        form.field('amount', t('s_sl_menge'),
                  default=materials.amount_text(entry['amount'], piece=piece),
                  width=12)
        form.note(t('s_sl_frei') % _amount(free, lot['unit_q']))
        if entry['jobs'] or entry['missions']:
            form.note(t('s_sl_mit_reservierung_h'))
            _earmark_fields(form, lot, entry)
    if not piece:
        form.toggle('merge', t('s_sl_zusammenlegen'))

    def go():
        merge = bool(form.value('merge'))
        unit_id = lot['unit']['id']
        if mode == 'pool':
            target = form.value('to_unit')
            amount = _amount_ok(page, form, number(form.value('amount')),
                                lot['unit_q'], maximum=entry['amount'],
                                optional=True)
            if amount is None or not target:
                return
            op = orgstock.op_move_pool(unit_id, entry, target,
                                       amount or None, lot['unit_q'], merge)
        else:
            amount = _amount_ok(page, form, number(form.value('amount')),
                                lot['unit_q'], maximum=entry['amount'])
            if amount is None:
                return
            jobs, missions = _earmark_values(page, entry)
            if mode == 'holder':
                holder = form.value('holder')
                if not holder:
                    _say(page, t('s_sl_besitzer_fehlt'), RED)
                    return
                op = orgstock.op_move_holder(unit_id, entry, amount,
                                             lot['unit_q'],
                                             form.value('to_unit') or unit_id,
                                             holder, jobs, missions, merge)
            else:
                place = (form.value('location') or '').strip()
                if not place:
                    _say(page, t('s_sl_ort_fehlt'), RED)
                    return
                op = orgstock.op_move_location(unit_id, entry, amount,
                                               lot['unit_q'], {'name': place},
                                               jobs, missions, merge)
        _submit(window, page, data, [op], redraw)

    form.buttons([(t('s_sl_verschieben'), go, 'strong'),
                  (t('s_bt_abbrechen'), _cancel(page, redraw_list), '')])


def _form_note(window, parent, page, data, lot, entry, kind, redraw,
               redraw_list):
    form = _Form(window, parent, page, t('s_sl_notiz_titel')
                 % (lot['name'], _holder(entry)))
    form.field('note', t('s_sl_notiz'), default=entry['note'])

    def save():
        _submit(window, page, data, [orgstock.op_note(
            lot['unit']['id'], entry, form.value('note'))], redraw)

    def clear():
        _submit(window, page, data, [orgstock.op_note(
            lot['unit']['id'], entry, '')], redraw)

    form.buttons([(t('s_sl_speichern'), save, 'strong'),
                  (t('s_sl_notiz_weg'), clear, ''),
                  (t('s_bt_abbrechen'), _cancel(page, redraw_list), '')])


def _form_stolen(window, parent, page, data, lot, entry, kind, redraw,
                 redraw_list):
    target = not lot['stolen']
    form = _Form(window, parent, page, t(
        's_sl_als_gestohlen' if target else 's_sl_nicht_gestohlen')
        % (lot['name'], _holder(entry)))
    form.field('amount', t('s_sl_menge'), width=12,
              placeholder=t('s_sl_alles'))

    def go():
        amount = _amount_ok(page, form, number(form.value('amount')),
                            lot['unit_q'], maximum=entry['amount'],
                            optional=True)
        if amount is None:
            return
        _submit(window, page, data, [orgstock.op_stolen(
            lot['unit']['id'], entry, target, amount or None,
            lot['unit_q'])], redraw)

    form.buttons([(t('s_sl_markieren'), go, 'strong'),
                  (t('s_bt_abbrechen'), _cancel(page, redraw_list), '')])


def _targets_for(page, unit_id, material_bt):
    """Aufträge und Missionen für Reservierungen — im Hintergrund geholt."""
    key = (unit_id, material_bt)
    if key not in page['targets']:
        page['targets'][key] = None

        def work():
            try:
                found = orgstock.targets(basetool.CONNECTION, unit_id,
                                         material_bt)
            except Exception as exc:
                from . import errors
                errors.record('orgstock_page.targets', exc)
                found = {'jobs': [], 'missions': [], 'error': True}
            page['targets'][key] = found
            # Nur das offene Formular neu — es zeigt dann die Ziele.
            _TK_CALLS.put(lambda: _place_form(page))
        threading.Thread(target=work, daemon=True).start()
    return page['targets'][key]


def _form_earmark(window, parent, page, data, lot, entry, kind, redraw,
                  redraw_list):
    form = _Form(window, parent, page, t('s_sl_reservieren_titel')
                 % (lot['name'], _holder(entry)))
    unit_id = lot['unit']['id']
    piece = lot['unit_q'] == orgstock.PIECE

    def send(op):
        _submit(window, page, data, [op], redraw)

    for job in entry['jobs']:
        label = '#%s' % job['number'] if job['number'] is not None \
            else t('s_bd_auftrag')
        _earmark_line(window, form, page, lot, entry, 'jobOrder', job['id'],
                      label, job['amount'], send)
    for mission in entry['missions']:
        _earmark_line(window, form, page, lot, entry, 'mission',
                      mission['id'], mission['name'] or '?',
                      mission['amount'], send)

    bt = (lot['material'] or {}).get('bt')
    found = _targets_for(page, unit_id, bt) if bt else {'jobs': [],
                                                        'missions': []}
    if found is None:
        form.note(t('s_sl_ziele_laden'))
    else:
        have = {j['id'] for j in entry['jobs']} | {m['id']
                                                    for m in entry['missions']}
        choices = [('job:' + j['id'], '#%s · %s' % (
            j['number'] if j['number'] is not None else '?',
            t('s_bd_status_' + j['status'].lower())
            if j['status'] in ('OPEN', 'IN_PROGRESS') else j['status']))
            for j in found['jobs'] if j['id'] not in have]
        if not piece:
            choices += [('mission:' + m['id'], t('s_sl_mission') % (
                m['name'] or '?')) for m in found['missions']
                if m['id'] not in have]
        if choices:
            form.select('new_target', t('s_sl_neu_fuer'), choices)
            form.field('new_amount', t('s_sl_menge'), width=12)

            def add():
                amount = _amount_ok(page, form,
                                    number(form.value('new_amount')),
                                    lot['unit_q'], maximum=entry['amount'])
                choice = form.value('new_target') or ''
                if amount is None or ':' not in choice:
                    return
                kind_, ident = choice.split(':', 1)
                send(orgstock.op_allocation(
                    'add', unit_id, entry,
                    'jobOrder' if kind_ == 'job' else 'mission', ident,
                    amount, lot['unit_q']))
            form.buttons([(t('s_sl_reservieren'), add, 'strong')])
        else:
            form.note(t('s_sl_keine_ziele'))
    form.buttons([(t('s_bt_abbrechen'), _cancel(page, redraw_list), '')])


def _earmark_line(window, form, page, lot, entry, target_kind, target_id,
                  label, amount, send):
    """Eine bestehende Reservierung: Menge ändern oder entfernen."""
    from .main_window import round_entry
    row = form._row(label)
    key = 'set:' + target_id
    var = tk.StringVar(master=form.card, value=page['values'].get(
        key, materials.amount_text(amount, piece=lot['unit_q']
                                   == orgstock.PIECE)))
    var.trace_add('write', lambda *_a: page['values'].__setitem__(key,
                                                                   var.get()))
    box = round_entry(row, var, window.f_small, theme.FIELD, LINE, ACCENT, FG,
                      width=10, placeholder=t('s_sl_menge'))
    box.holder.pack(side='left')

    def change():
        new = _amount_ok(page, form, number(var.get()), lot['unit_q'],
                         maximum=entry['amount'])
        if new is None:
            return
        send(orgstock.op_allocation('change', lot['unit']['id'], entry,
                                    target_kind, target_id, new,
                                    lot['unit_q']))

    def remove():
        send(orgstock.op_allocation('remove', lot['unit']['id'], entry,
                                    target_kind, target_id))

    _button(window, row, t('s_sl_aendern'), change,
            brand=theme.KRT_ORANGE).pack(side='left', padx=(8, 0))
    _button(window, row, t('s_sl_entfernen'), remove,
            danger=True).pack(side='left', padx=(8, 0))


def own_member(roster, account=None):
    """Die `ref` des eigenen Eintrags in der Mitgliederliste — gefunden über
    den Spiel-Account, ohne Rücksicht auf Groß- und Kleinschreibung. Leer,
    wenn der Account unbekannt ist oder in der Liste fehlt."""
    if account is None:
        from . import logsource
        account = logsource.own_account()
        if account == logsource.ALL_ACCOUNTS:
            account = None
    wanted = (account or '').strip().casefold()
    if not wanted:
        return ''
    for member in roster:
        if (member.get('name') or '').strip().casefold() == wanted:
            return member['ref']
    return ''


def _form_book_in(window, parent, page, data, unit_id, redraw, redraw_list):
    """Einbuchen. `redraw` zeichnet nach dem Buchen neu, `redraw_list` baut
    nur dieses Formular neu (Ware gewählt, Einheit umgestellt)."""
    form = _Form(window, parent, page, t('s_sl_einbuchen_titel'))
    roster = data['rosters'].get(unit_id) or []
    if not roster:
        form.note(t('s_sl_keine_mitglieder'), theme.YELLOW)
    else:
        form.select('holder', t('s_sl_besitzer'),
                    [(m['ref'], m['name']) for m in roster],
                    default=own_member(roster))
    _material_field(window, form, page, data, redraw_list)
    _location_field(window, form, page, 'location', t('s_sl_ort'))
    unit = form.select('unit', t('s_sl_einheit_menge'),
                       [(orgstock.SCU, 'SCU'),
                        (orgstock.PIECE, t('s_lg_stueck'))],
                       on_change=lambda _v: _unit_changed(page, redraw_list))
    form.field('amount', t('s_sl_menge'), width=12)
    if unit != orgstock.PIECE:
        form.field('quality', t('s_sl_qualitaet'), width=8, placeholder='0')
        form.toggle('merge', t('s_sl_zusammenlegen'))
    form.toggle('stolen', t('s_sl_gestohlen_feld'))
    form.field('note', t('s_sl_notiz'))

    def go():
        # Alles, was gebucht wird, gilt so, wie es beim Klick dastand — auch
        # wenn die Antwort vom Nachschlagen erst später kommt.
        values = dict(page['values'])
        holder = values.get('holder')
        name = (values.get('material') or '').strip()
        place = (values.get('location') or '').strip()
        if not holder:
            _say(page, t('s_sl_besitzer_fehlt'), RED)
            return
        if not name:
            _say(page, t('s_sl_ware_fehlt'), RED)
            return
        if not place:
            _say(page, t('s_sl_ort_fehlt'), RED)
            return
        amount = _amount_ok(page, form, number(values.get('amount')), unit)
        if amount is None:
            return
        piece = unit == orgstock.PIECE
        # Bei Stück steht kein Qualitätsfeld da — ein alter Wert zählt nicht.
        quality = 0 if piece else number(values.get('quality')) or 0
        if not 0 <= quality <= 1000 or quality != int(quality):
            _say(page, t('s_sl_qualitaet_falsch'), RED)
            return
        stolen = bool(values.get('stolen'))
        note = values.get('note') or ''
        merge = bool(values.get('merge'))

        def book(material):
            # Ein Gegenstand zählt in Stück; in SCU lehnte das Basetool ihn ab.
            if material.get('kind') == stock_catalog.ITEM and not piece:
                _say(page, t('s_sl_ware_stueck') % material['name'], RED)
                return
            # Nur der bt: Das Basetool sucht `material` erst unter den Waren;
            # ein Name daneben ergibt dort ungenaue Vorschläge, und die gelten
            # als mehrdeutig, bevor der bt unter den Gegenständen greift.
            op = orgstock.op_book_in(unit_id, holder, {'bt': material['bt']},
                                     {'name': place}, int(quality), stolen,
                                     amount, unit, note, merge)
            _submit(window, page, data, [op], redraw)

        chosen = values.get('material_ref')
        if chosen and chosen['name'].lower() == name.lower():
            book(chosen)
            return
        known = _known_material(data, name, unit)
        if known:
            book(known)
            return
        _resolve_then(page, name, stock_catalog.reference(name, piece), piece,
                      ('book_in', unit_id), values, book)

    form.buttons([(t('s_sl_einbuchen'), go, 'strong'),
                  (t('s_bt_abbrechen'), _cancel(page, redraw), '')])


def _unit_changed(page, redraw):
    """Stück/SCU umgestellt: Eine gemerkte Auflösung gilt für die andere
    Einheit nicht mehr, ein laufendes Nachschlagen auch nicht."""
    page['values'].pop('material_ref', None)
    _drop_pending(page)
    redraw()


def _known_material(data, name, unit):
    """`{bt, name}` einer Ware, die schon in einem Pool liegt und dort in
    `unit` (Stück/SCU) gezählt wird — sonst `None` (dann löst das Basetool
    den Namen auf)."""
    wanted = name.strip().lower()
    for lot in (data or {}).get('lots', {}).values():
        if (lot['name'].lower() == wanted and lot['unit_q'] == unit
                and lot['material'].get('bt')):
            return {'bt': lot['material']['bt'],
                    'name': lot['material'].get('name') or lot['name']}
    return None


# --------------------------------------------------------------- Senden
def _submit(window, page, data, ops, redraw):
    """Den Änderungssatz im Hintergrund schicken, danach den Feed holen.
    Gebucht schließt das Formular nur, wenn noch dasselbe offen ist."""
    if page['busy']:
        _say(page, t('s_sl_noch_dran'), SUB)
        return
    page['busy'] = True
    form_at = page['form']
    _say(page, t('s_sl_bucht'), SUB)

    def work():
        conn = basetool.CONNECTION
        done = False
        try:
            result = orgstock.send(conn, ops)
            if result.get('confirm_url'):
                page['confirm_url'] = result['confirm_url']
                message, color = t('s_sl_masse'), theme.YELLOW
            elif result['failed']:
                reasons = sorted({reason or kind
                                  for _i, kind, reason in result['failed']})
                message = ' '.join(reason_text(r) for r in reasons)
                color = RED
            else:
                done = True
                message, color = t('s_sl_gebucht'), ACCENT
                if result.get('offers'):
                    message += ' ' + t('s_sl_angebote') % result['offers']
        except basetool.ApiError as error:
            from . import errors
            from .basetool_page import error_text
            errors.record('orgstock_page.send', RuntimeError(error.describe()))
            message, color = error_text(error.code), RED
        except Exception as exc:
            from . import errors
            errors.record('orgstock_page.send', exc)
            message, color = t('s_sl_r_anders', type(exc).__name__), RED
        try:
            current = orgstock.current() or {}
            orgstock.refresh(conn, details=current.get('details', False),
                             write=current.get('write', False))
        except Exception as exc:
            from . import errors
            errors.record('orgstock_page.refresh', exc)

        def show():
            page['busy'] = False
            # `is`: Ein abgebrochenes und neu geöffnetes Formular ist ein
            # neues Tupel, auch wenn es gleich aussieht.
            if done and page['form'] is form_at:
                page['form'] = None
                page['values'] = {}
            # Gebucht (neuer Stand) oder eine Rückfrage im Browser (Karte
            # oben): neu zeichnen. Sonst bleibt alles stehen, nur die
            # Meldung kommt.
            if done or page.get('confirm_url'):
                page['message'], page['message_color'] = message, color
                page['redraw_all']()
            else:
                _say(page, message, color)
        _TK_CALLS.put(show)

    threading.Thread(target=work, daemon=True).start()
