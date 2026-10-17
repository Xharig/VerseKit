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

Oben die Einheit (bei mehr als einer), die vier Sichten Items · Rohstoffe ·
Handelsware · Komponenten und die Suche. Darunter die Posten. Wer in der
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
                    _heading, _scroll_area, _body_text, _button, _wrap,
                    _stripe, _stripe_reset, _chip, _card)
from . import basetool, basetool_sync, exchange_orgstock as orgstock
from . import icons, materials, paths, theme

PAGE = 'staffellager'
SETTING_UNIT = 'staffellager_einheit'
SETTING_VIEW = 'staffellager_sicht'

VIEW_KEYS = {orgstock.ITEMS: 's_sl_items', orgstock.RAW: 's_sl_rohstoffe',
             orgstock.TRADE: 's_sl_handelsware',
             orgstock.COMPONENTS: 's_sl_komponenten'}

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
    piece = unit == orgstock.PIECE
    return '%s %s' % (materials.amount_text(value, piece=piece),
                      t('s_lg_stueck') if piece else 'SCU')


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
def orgstock_page(window, frame):
    from .main_window import round_entry
    _heading(window, frame, t('hf_staffellager'), t('s_sl_lead'))
    inner = _scroll_area(frame)
    page = {'unit': paths.setting(SETTING_UNIT) or '',
            'view': paths.setting(SETTING_VIEW) or orgstock.RAW,
            'search': tk.StringVar(master=inner, value=''),
            'open': set(), 'form': None, 'values': {}, 'busy': False,
            'message': '', 'message_color': FG, 'confirm_url': None,
            'locations': None, 'targets': {}}
    if page['view'] not in orgstock.CATEGORIES:
        page['view'] = orgstock.RAW
    top = tk.Frame(inner, bg=BG)
    top.pack(fill='x', padx=24, pady=(10, 0))
    listing = tk.Frame(inner, bg=BG)
    listing.pack(fill='x', padx=24, pady=(6, 20))

    def redraw():
        try:
            if not top.winfo_exists():
                return
        except tk.TclError:
            return
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
                    canvas.configure(scrollregion=canvas.bbox('all'))
                    height = (canvas.bbox('all') or (0, 0, 0, 0))[3]
                    if height > 0:
                        canvas.yview_moveto(position / height)
                except tk.TclError:
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

    page['redraw_all'] = redraw
    page['redraw_list'] = redraw_list
    page['search'].trace_add('write', lambda *_a: redraw_list())

    def on_status():
        _TK_CALLS.put(redraw)

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
        page['unit'] = units[0]['id']

    row = tk.Frame(top, bg=BG)
    row.pack(fill='x')
    if len(units) > 1:
        def pick(value):
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

    chips = tk.Frame(top, bg=BG)
    chips.pack(fill='x', pady=(10, 0))
    counts = orgstock.counts(data, unit)
    for view in orgstock.CATEGORIES:
        chip = _chip(window, chips, '%s (%d)' % (t(VIEW_KEYS[view]),
                                                 counts[view]),
                     page['view'] == view)
        chip.pack(side='left', padx=(0, 6))

        def choose(_e=None, v=view):
            page['view'] = v
            paths.set_setting(SETTING_VIEW, v)
            redraw()
        chip.bind('<Button-1>', choose)

    box = round_entry(top, page['search'], window.f_small, theme.FIELD, LINE,
                      ACCENT, FG, placeholder=t('s_sl_suche'))
    box.holder.pack(fill='x', pady=(10, 0))

    if page['message']:
        line = tk.Label(top, text=page['message'], bg=BG,
                        fg=page['message_color'], font=window.f_small,
                        anchor='w', justify='left')
        line.pack(fill='x', pady=(8, 0))
        _wrap(line)
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

    form = page['form']
    if form and form[0] == 'book_in':
        holder = tk.Frame(top, bg=BG)
        holder.pack(fill='x', pady=(10, 0))
        _form_book_in(window, holder, page, data, unit, redraw, redraw_list)


def _draw_list(window, listing, page, redraw, redraw_list):
    data = orgstock.current()
    if data is None or not data['units']:
        return
    unit = page['unit']
    lots = orgstock.lots_of(data, unit, page['view'], page['search'].get())
    _stripe_reset(listing)
    if not lots:
        _body_text(listing, t('s_sl_leer'), window.f_small, fill='x')
        return
    writable = orgstock.can_write(data, unit)
    for lot in lots:
        _lot_row(window, listing, page, data, lot, writable, redraw,
                 redraw_list)
    _body_text(listing, t('s_sl_hinweis'), window.f_small, fill='x',
               pady=(16, 0))


def _lot_row(window, listing, page, data, lot, writable, redraw,
             redraw_list):
    base = _stripe(listing, BG)
    row = tk.Frame(listing, bg=base)
    row.pack(fill='x')
    head = tk.Frame(row, bg=base)
    head.pack(fill='x', padx=8, pady=(5, 0))
    expandable = lot['entries'] is not None
    is_open = lot['key'] in page['open']
    arrow = None
    if expandable:
        arrow = icons.line(head, 'zuklappen' if is_open else 'aufklappen',
                           background=base, font=window.f_small)
        arrow.pack(side='left', padx=(0, 4))
    title = tk.Label(head, text=lot['name'], bg=base,
                     fg=GOLD if lot['stolen'] else FG, font=window.f_base,
                     anchor='w')
    title.pack(side='left')
    tk.Label(head, text=_amount(lot['amount'], lot['unit_q']), bg=base, fg=FG,
             font=window.f_small, anchor='e').pack(side='right')
    parts = [lot['place'] or '?']
    if not orgstock.is_item(lot):
        parts.append(t('s_sl_q') % lot['quality'])
    if lot['stolen']:
        parts.append(t('s_sl_gestohlen'))
    if lot['job'] > 0:
        parts.append(t('s_sl_res_auftraege') % _amount(lot['job'],
                                                       lot['unit_q']))
    if lot['mission'] > 0:
        parts.append(t('s_sl_res_missionen') % _amount(lot['mission'],
                                                       lot['unit_q']))
    tk.Label(row, text='  ·  '.join(parts), bg=base, fg=SUB,
             font=window.f_small, anchor='w').pack(fill='x', padx=8,
                                                   pady=(0, 5))
    if expandable:
        def toggle(_e=None):
            if lot['key'] in page['open']:
                page['open'].discard(lot['key'])
            else:
                page['open'].add(lot['key'])
            redraw_list()
        for widget in (row, head, title, arrow):
            widget.configure(cursor='hand2')
            widget.bind('<Button-1>', toggle)
    if not (expandable and is_open):
        return
    if not lot['entries']:
        tk.Label(row, text=t('s_sl_keine_eintraege'), bg=base, fg=SUB,
                 font=window.f_small, anchor='w').pack(fill='x', padx=28,
                                                       pady=(0, 6))
    for entry in lot['entries']:
        _entry_row(window, row, base, page, data, lot, entry, writable,
                   redraw, redraw_list)


def _entry_row(window, parent, base, page, data, lot, entry, writable,
               redraw, redraw_list):
    box = tk.Frame(parent, bg=base)
    box.pack(fill='x', padx=(28, 8), pady=(0, 6))
    line = tk.Frame(box, bg=base)
    line.pack(fill='x')
    tk.Label(line, text=_holder(entry), bg=base, fg=FG, font=window.f_small,
             anchor='w').pack(side='left')
    tk.Label(line, text=_amount(entry['amount'], lot['unit_q']), bg=base,
             fg=FG, font=window.f_small, anchor='e').pack(side='right')
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
    if details:
        text = tk.Label(box, text='\n'.join(details), bg=base, fg=SUB,
                        font=window.f_small, anchor='w', justify='left')
        text.pack(fill='x')
        _wrap(text, inset=80)
    if not writable:
        return
    actions = tk.Frame(box, bg=base)
    actions.pack(fill='x', pady=(4, 0))
    for kind, key in (('book_out', 's_sl_ausbuchen'), ('sell', 's_sl_verkaufen'),
                      ('move', 's_sl_verschieben'), ('note', 's_sl_notiz'),
                      ('stolen', 's_sl_marke'), ('earmark', 's_sl_reservieren')):
        button = _button(window, actions, t(key),
                         lambda k=kind: _open_form(page, (k, entry['id'])))
        button.configure(bg=base)
        button.pack(side='left', padx=(0, 6), pady=(0, 2))
    form = page['form']
    if form and form[1] == entry['id']:
        holder = tk.Frame(box, bg=BG)
        holder.pack(fill='x', pady=(6, 0))
        builder = {'book_out': _form_take, 'sell': _form_take,
                   'move': _form_move, 'note': _form_note,
                   'stolen': _form_stolen, 'earmark': _form_earmark}[form[0]]
        builder(window, holder, page, data, lot, entry, form[0], redraw,
                redraw_list)


def _open_form(page, form):
    """Ein Formular auf- oder (beim zweiten Klick) wieder zuklappen. Das
    Einbuchen steht oben, die übrigen am Eintrag in der Liste."""
    page['form'] = None if page['form'] == form else form
    page['values'] = {}
    page['message'] = ''
    page['redraw_all' if form[0] == 'book_in' else 'redraw_list']()


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

    def buttons(self, entries):
        row = tk.Frame(self.card, bg=self.BG)
        row.pack(fill='x', padx=14, pady=(8, 12))
        for text, action, kind in entries:
            _button(self.window, row, text, action, strong=kind == 'strong',
                    danger=kind == 'danger',
                    brand=theme.KRT_ORANGE).pack(side='left', padx=(0, 8))

    def value(self, key, default=''):
        return self.page['values'].get(key, default)


def _cancel(page, redraw):
    def run():
        page['form'] = None
        page['values'] = {}
        redraw()
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


def _say(page, text, color=FG):
    page['message'] = text
    page['message_color'] = color
    redraw = page.get('redraw_all')
    if redraw:
        redraw()


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
            redraw = page.get('redraw_all')
            if redraw:
                _TK_CALLS.put(redraw)
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
    line, drop, _refresh = _combo_box(
        window, holder, var,
        lambda: list(page['locations'] or ()),
        placeholder=t('s_sl_ort_h'))
    line.pack(fill='x')
    drop.pack(fill='x')
    if page.get('locations_loading'):
        form.note(t('s_sl_orte_laden'))


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
            redraw = page.get('redraw_all')
            if redraw:
                _TK_CALLS.put(redraw)
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


def _form_book_in(window, parent, page, data, unit_id, redraw, redraw_list):
    form = _Form(window, parent, page, t('s_sl_einbuchen_titel'))
    roster = data['rosters'].get(unit_id) or []
    if not roster:
        form.note(t('s_sl_keine_mitglieder'), theme.YELLOW)
    else:
        form.select('holder', t('s_sl_besitzer'),
                    [(m['ref'], m['name']) for m in roster])
    form.field('material', t('s_sl_ware'), placeholder=t('s_sl_ware_h'))
    _location_field(window, form, page, 'location', t('s_sl_ort'))
    unit = form.select('unit', t('s_sl_einheit_menge'),
                       [(orgstock.SCU, 'SCU'),
                        (orgstock.PIECE, t('s_lg_stueck'))],
                       on_change=lambda _v: redraw())
    form.field('amount', t('s_sl_menge'), width=12)
    if unit != orgstock.PIECE:
        form.field('quality', t('s_sl_qualitaet'), width=8, placeholder='0')
        form.toggle('merge', t('s_sl_zusammenlegen'))
    form.toggle('stolen', t('s_sl_gestohlen_feld'))
    form.field('note', t('s_sl_notiz'))

    def go():
        holder = form.value('holder')
        name = (form.value('material') or '').strip()
        place = (form.value('location') or '').strip()
        if not holder:
            _say(page, t('s_sl_besitzer_fehlt'), RED)
            return
        if not name:
            _say(page, t('s_sl_ware_fehlt'), RED)
            return
        if not place:
            _say(page, t('s_sl_ort_fehlt'), RED)
            return
        amount = _amount_ok(page, form, number(form.value('amount')), unit)
        if amount is None:
            return
        quality = number(form.value('quality')) or 0
        if not 0 <= quality <= 1000 or quality != int(quality):
            _say(page, t('s_sl_qualitaet_falsch'), RED)
            return
        known = _known_material(data, name)
        op = orgstock.op_book_in(unit_id, holder, known or {'name': name},
                                 {'name': place}, int(quality),
                                 bool(form.value('stolen')), amount, unit,
                                 form.value('note'), bool(form.value('merge')))
        _submit(window, page, data, [op], redraw)

    form.buttons([(t('s_sl_einbuchen'), go, 'strong'),
                  (t('s_bt_abbrechen'), _cancel(page, redraw), '')])


def _known_material(data, name):
    """`{bt, name}` einer Ware, die schon in einem Pool liegt — sonst
    `None` (dann löst das Basetool den Namen auf)."""
    wanted = name.strip().lower()
    for lot in (data or {}).get('lots', {}).values():
        if lot['name'].lower() == wanted and lot['material'].get('bt'):
            return dict(lot['material'])
    return None


# --------------------------------------------------------------- Senden
def _submit(window, page, data, ops, redraw):
    """Den Änderungssatz im Hintergrund schicken, danach den Feed holen."""
    if page['busy']:
        return
    page['busy'] = True
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
            if done:
                page['form'] = None
                page['values'] = {}
            _say(page, message, color)
        _TK_CALLS.put(show)

    threading.Thread(target=work, daemon=True).start()
