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
Die Seite „Basetool" — Verbindung und Bauplan-Abgleich mit dem KRT Profit
Basetool.

Seit v3.61.0 für alle sichtbar (Freigabe krt-profit/basetool#2273). Alles
daran ist ab Werk aus: Wer das Basetool nicht nutzt, verbindet nie, und dann
geht auch nichts hinaus.

⚠⚠ **Angezeigt wird nur der Code und die nackte Adresse** — nie ein Link mit
eingebautem Code, auch nicht als Knopf. Der Knopf „Im Browser öffnen" öffnet
die Seite, auf der man den Code **selbst eintippt**; genau dort warnt das
Basetool vor Betrug.
"""
import threading
import time
import tkinter as tk

from .language import t
from .pages import (BG, SURFACE, FG, SUB, ACCENT, LINE, RED, _TK_CALLS,
                    _heading, _scroll_area, _body_text, _button, _wrap, _link)
from . import basetool, basetool_sync, paths, secret_store, theme


def builders():
    return {'basetool': basetool_page}


def _card(parent):
    from .main_window import round_frame
    card = round_frame(parent, SURFACE, LINE, radius=8, base_color=BG)
    card.holder.pack(fill='x', pady=(0, 10))
    return card


def _title(window, card, text):
    tk.Label(card, text=text, bg=SURFACE, fg=FG, font=window.f_bold,
             anchor='w').pack(fill='x', padx=16, pady=(12, 0))


def _note(window, card, text, color=SUB, bottom=12):
    line = tk.Label(card, text=text, bg=SURFACE, fg=color, font=window.f_small,
                    anchor='w', justify='left')
    line.pack(fill='x', padx=16, pady=(4, bottom))
    _wrap(line, inset=32)
    return line


def _buttons(window, card, entries):
    row = tk.Frame(card, bg=SURFACE)
    row.pack(fill='x', padx=16, pady=(0, 12))
    for text, action, kind in entries:
        _button(window, row, text, action, strong=kind == 'strong',
                danger=kind == 'danger').pack(side='left', padx=(0, 8))
    return row


# Fehlercode -> Text. Was hier fehlt, zeigt den allgemeinen Satz mit Code.
ERROR_TEXTS = {
    'NETWORK': 's_bt_f_netz',
    'NOT_CONNECTED': 's_bt_f_getrennt',
    'invalid_grant': 's_bt_f_abgelaufen',
    'UNAUTHENTICATED': 's_bt_f_abgelaufen',
    'CLIENT_REVOKED': 's_bt_f_getrennt',
    'INSTALLATION_REVOKED': 's_bt_f_getrennt',
    'CLIENT_NOT_ALLOWED': 's_bt_f_nicht_zugelassen',
    'unauthorized_client': 's_bt_f_nicht_zugelassen',
    'invalid_client': 's_bt_f_nicht_zugelassen',
    'CLIENT_SUSPENDED': 's_bt_f_gesperrt',
    'CLIENT_VERSION_UNSUPPORTED': 's_bt_f_version',
    'TERMS_NOT_ACCEPTED': 's_bt_f_bedingungen',
    'PENDING_APPROVAL': 's_bt_f_wartet',
    'NO_ROLE': 's_bt_f_rolle',
    'ACTING_MEMBER_REFUSED': 's_bt_f_rolle',
    'SCOPE_MISSING': 's_bt_f_rechte',
    'ISSUER_MISMATCH': 's_bt_f_aussteller',
    'QUOTA_EXCEEDED': 's_bt_f_kontingent',
    'RATE_LIMITED': 's_bt_f_bremse',
    'account_mismatch': 's_bt_konto_falsch',
    'account_unknown': 's_bt_konto_unbekannt',
    'account_all': 's_bt_konto_alle',
    'account_none': 's_bt_konto_keiner',
    'INSTALLATION_ID_MISSING': 's_bt_f_kennung',
    'SECRET_REFUSED': 's_bt_f_schluesselbund',
    'SECRET_UNSAFE': 's_bt_f_datei_offen',
    'SECRET_FAILED': 's_bt_f_schluesselbund',
}


def error_text(code):
    # Ein 5xx ohne eigenen Code des Basetools (etwa von der Vorschaltung):
    # „gerade nicht erreichbar", nicht „abgelehnt" — VerseKit versucht es
    # ohnehin von selbst wieder (erster Test, 28.09.2026: „HTTP_503").
    if (code or '').startswith('HTTP_5'):
        return t('s_bt_f_netz')
    key = ERROR_TEXTS.get(code)
    return t(key) if key else t('s_bt_f_allgemein', code)


def basetool_page(window, frame):
    _heading(window, frame, t('hf_basetool'), t('s_bt_lead'))
    inner = _scroll_area(frame)
    area = tk.Frame(inner, bg=BG)
    area.pack(fill='x')
    login = {'current': None, 'message': ''}

    def redraw():
        try:
            if not area.winfo_exists():
                return
        except tk.TclError:
            return
        # ⚠⚠ Erst nach oben, dann neu bauen, dann die Stelle wiederherstellen
        # (28.09.2026: „beim Klicken auf Buttons wird das Fenster schwarz").
        # Wer heruntergerollt hatte, stand nach dem Neubau im Leeren: Außerhalb
        # der Ansicht blendet die Leinwand den Inhalt aus, er misst sich dann
        # nicht neu (183 statt 813 px), und die Ansicht zeigte unter ihm.
        canvas = _canvas_of(area)
        top = canvas.canvasy(0) if canvas is not None else 0
        if canvas is not None:
            canvas.yview_moveto(0)
        for child in area.winfo_children():
            child.destroy()
        _draw(window, area, login, redraw)
        if canvas is not None and top > 0:
            def back():
                try:
                    canvas.update_idletasks()
                    canvas.configure(scrollregion=canvas.bbox('all'))
                    height = (canvas.bbox('all') or (0, 0, 0, 0))[3]
                    if height > 0:
                        canvas.yview_moveto(top / height)
                except tk.TclError:
                    pass
            canvas.after_idle(back)

    def on_status():
        # „Gleiche gerade ab …" vom Knopfdruck gilt, bis ein Durchgang fertig
        # ist — dann steht dort sein Ergebnis.
        if not basetool_sync.STATUS.get('running'):
            login['requested'] = False
        _TK_CALLS.put(redraw)

    basetool_sync.LISTENERS.append(on_status)
    area.bind('<Destroy>', lambda _e: (
        on_status in basetool_sync.LISTENERS
        and basetool_sync.LISTENERS.remove(on_status)))
    redraw()


def _canvas_of(widget):
    """Die Rollfläche, in der dieses Element steckt — oder None."""
    while widget is not None:
        if isinstance(widget, tk.Canvas):
            return widget
        widget = widget.master
    return None


def _draw(window, area, login, redraw):
    conn = basetool.CONNECTION
    connected = conn.connected()
    status = dict(basetool_sync.STATUS)

    # ---------------------------------------------------------- Verbindung
    card = _card(area)
    _title(window, card, t('s_bt_verbindung'))
    cfg = basetool.connection_config()
    if cfg['sandbox']:
        _note(window, card, t('s_bt_sandbox'), color=theme.YELLOW, bottom=0)
    elif cfg['override']:
        _note(window, card, t('s_bt_override'), color=theme.YELLOW, bottom=0)
    if secret_store.uses_file():
        _note(window, card, t('s_bt_datei', secret_store.folder()),
              color=theme.YELLOW, bottom=0)

    pending = login['current']
    if pending is not None:
        _login_box(window, card, login, redraw)
    elif connected:
        _note(window, card, t('s_bt_verbunden', basetool_sync.label()))
    else:
        _note(window, card, t('s_bt_nicht_verbunden'))
    if login['message']:
        _note(window, card, login['message'], color=theme.YELLOW)

    _label_row(window, card, redraw)

    def connect():
        _start_login(window, login, redraw)

    def disconnect():
        def work():
            try:
                conn.disconnect()
            finally:
                basetool_sync.forget_installation()
                login['message'] = t('s_bt_getrennt')
                _TK_CALLS.put(redraw)
        threading.Thread(target=work, daemon=True).start()

    if pending is None:
        if connected:
            _buttons(window, card, [(t('s_bt_trennen'), disconnect, 'danger')])
        elif not basetool_sync.enabled():
            # ⚠ Erst ein Bereich, dann verbinden (28.09.2026, erster Test):
            # Angefragt werden nur die Rechte eingeschalteter Bereiche. Wer mit
            # ausgeschaltetem Schalter verband, bekam nur `exchange.connect`
            # und musste danach ein zweites Mal im Browser zustimmen.
            _note(window, card, t('s_bt_erst_bereich'), color=theme.YELLOW)
        else:
            _buttons(window, card, [(t('s_bt_verbinden'), connect, 'strong')])

    # --------------------------------------- Baupläne, Lager, Hangar
    # ⚠⚠ Beim ersten Test (28.09.2026): „sehe nicht, ob Erlaubnis erteilen was
    # tut" und „weiß nicht, wie ich bemerke, ob es klappt". Deshalb je Bereich
    # genau EINE Aussage — Code läuft oben / Recht fehlt / Stand — und darunter
    # eine gemeinsame Zeile mit dem letzten Abgleich.
    granted = set(conn.granted or status.get('capabilities') or ())
    for setting, scopes, title_key, help_key in (
            (basetool_sync.SETTING_BLUEPRINTS, basetool.SCOPES_BLUEPRINTS,
             's_bt_bauplaene', 's_bt_bauplaene_h'),
            (basetool_sync.SETTING_STOCK, basetool.SCOPES_STOCK,
             's_bt_lager', 's_bt_lager_h'),
            (basetool_sync.SETTING_SHIPS, basetool.SCOPES_HANGAR,
             's_bt_hangar', 's_bt_hangar_h')):
        _area_card(window, area, login, redraw, status, connected, granted,
                   setting, scopes, title_key, help_key)

    if connected and basetool_sync.enabled() and login['current'] is None:
        card = _card(area)
        _title(window, card, t('s_bt_abgleich'))
        if status.get('running') or login.get('requested'):
            _note(window, card, t('s_bt_laeuft'), bottom=0)
        elif status.get('last_sync'):
            _note(window, card, t('s_bt_zuletzt', status['last_sync']),
                  color=FG, bottom=0)
        if status.get('code') and not status.get('running'):
            _note(window, card, error_text(status['code']),
                  color=theme.YELLOW, bottom=0)
        tk.Frame(card, bg=SURFACE, height=8).pack(fill='x')

        def sync_now():
            # Sofort sichtbar machen, dass etwas passiert — der Durchgang
            # selbst startet erst im nächsten Takt des Watchers (Sekunden).
            login['requested'] = True
            basetool_sync.request_now()
            window.say(t('s_bt_laeuft'))
            redraw()

        _buttons(window, card, [(t('s_bt_jetzt'), sync_now, '')])

    # ------------------------------------------------ Account-Prüfung
    code = status.get('code') or ''
    if code.startswith('account_'):
        card = _card(area)
        _title(window, card, t('s_bt_konto'))
        _note(window, card, error_text(code),
              color=RED if code == 'account_mismatch' else theme.YELLOW)
        if code == 'account_unknown':
            _buttons(window, card, [(t('s_bt_konto_bestaetigen'),
                                     basetool_sync.confirm_account, 'strong')])

    # ---------------------------------------- Bestätigung im Browser
    url = status.get('confirm_url')
    if url and time.time() < (status.get('confirm_until') or 0):
        card = _card(area)
        _title(window, card, t('s_bt_masse'))
        _note(window, card, t('s_bt_masse_h'))
        _buttons(window, card, [(t('s_bt_masse_oeffnen'),
                                 lambda: paths.open_in_browser(url),
                                 'strong')])

    state = basetool_sync.current_state() if connected else None
    if state:
        _decisions(window, area, state)

    # ------------------------------------------------------- Hinweis
    _body_text(area, t('s_bt_datenschutz'), window.f_small, fill='x',
               pady=(6, 20))


def _area_card(window, area, login, redraw, status, connected, granted,
               setting, scopes, title_key, help_key):
    """Ein Bereich: Schalter, was er tut, und sein Stand — eine Aussage."""
    from .main_window import toggle_switch
    card = _card(area)
    head = tk.Frame(card, bg=SURFACE)
    head.pack(fill='x', padx=16, pady=(12, 0))
    on = paths.setting_bool(setting, False)

    def flip():
        new_value = not paths.setting_bool(setting, False)
        paths.set_setting(setting, new_value)
        if new_value:
            basetool_sync.request_now()
        window.root.after(50, redraw)
        return new_value

    toggle_switch(head, on, flip, bg=SURFACE).pack(side='right')
    tk.Label(head, text=t(title_key), bg=SURFACE, fg=FG, font=window.f_bold,
             anchor='w').pack(side='left', fill='x', expand=True)
    _note(window, card, t(help_key))
    if not (on and connected):
        return
    if login['current'] is not None:
        _note(window, card, t('s_bt_code_oben'), color=theme.YELLOW)
        return
    if granted and not set(scopes) <= granted:
        _note(window, card, t('s_bt_rechte_fehlen'), color=theme.YELLOW,
              bottom=0)
        _buttons(window, card, [(t('s_bt_rechte_erweitern'),
                                 lambda: _start_login(window, login, redraw),
                                 'strong')])
        return
    state = basetool_sync.current_state() or {}
    counts = status.get('counts') or {}
    lines = []
    if setting == basetool_sync.SETTING_BLUEPRINTS:
        # ⭐ Woran man sieht, dass es klappt: der abgeglichene Stand,
        # dauerhaft, nicht nur als Meldung nach einem Durchgang.
        if state.get('baseline'):
            lines.append(t('s_bt_ueberblick', len(state['baseline']),
                           len(state.get('links') or {})))
        if 'added' in counts:
            lines.append(t('s_bt_stand_bp', counts.get('added', 0),
                           counts.get('local_added', 0),
                           counts.get('removed', 0)))
        if counts.get('unresolved'):
            lines.append(t('s_bt_unbekannt', counts['unresolved']))
    elif setting == basetool_sync.SETTING_STOCK:
        sub = state.get('stock') or {}
        c = counts.get('stock') or {}
        if 'server' in sub:
            lines.append(t('s_bt_lager_ueberblick', len(sub.get('server') or {})))
        if c:
            lines.append(t('s_bt_lager_stand', c.get('sent', 0),
                           c.get('taken', 0)))
            if c.get('moved'):
                lines.append(t('s_bt_lager_umgezogen', c['moved']))
            if c.get('skipped_location'):
                lines.append(t('s_bt_lager_ort',
                               c['skipped_location'],
                               ', '.join((sub.get('skipped') or {})
                                         .get('location', [])[:5])))
            if c.get('skipped_material'):
                lines.append(t('s_bt_lager_material', c['skipped_material']))
            if c.get('offers'):
                lines.append(t('s_bt_lager_angebote', c['offers']))
            if c.get('rejected'):
                lines.append(t('s_bt_abgelehnt_n', c['rejected'],
                               ', '.join(sorted(set(
                                   (sub.get('rejected') or {}).values()))[:3])))
    else:
        sub = state.get('ships') or {}
        c = counts.get('ships') or {}
        if 'server' in sub:
            lines.append(t('s_bt_hangar_ueberblick', len(sub.get('server') or {}),
                           len(sub.get('links') or {})))
        if c:
            lines.append(t('s_bt_hangar_stand', c.get('linked', 0),
                           c.get('created', 0), c.get('taken', 0)))
            if c.get('unresolved'):
                lines.append(t('s_bt_hangar_unbekannt', c['unresolved']))
            if c.get('detached'):
                lines.append(t('s_bt_hangar_missionen', c['detached']))
            if c.get('rejected'):
                lines.append(t('s_bt_abgelehnt_n', c['rejected'],
                               ', '.join(sorted(set(
                                   (sub.get('rejected') or {}).values()))[:3])))
    for line in lines:
        _note(window, card, line, color=FG, bottom=0)
    tk.Frame(card, bg=SURFACE, height=10).pack(fill='x')


def _decisions(window, area, state):
    """Was nur der Spieler entscheiden darf: Konflikte und hier Entferntes."""
    names = {bt: (item.get('ref') or {}).get('name') or bt
             for bt, item in (state.get('baseline') or {}).items()}

    conflicts = state.get('conflicts') or {}
    if conflicts:
        card = _card(area)
        _title(window, card, t('s_bt_konflikte', len(conflicts)))
        _note(window, card, t('s_bt_konflikte_h'), bottom=4)
        _note(window, card, _listing(conflicts, names), color=FG)
        _buttons(window, card, [(t('s_bt_wieder_hoch'),
                                 lambda: basetool_sync.override_conflicts(
                                     list(conflicts)), '')])

    removed = state.get('local_removed') or {}
    if removed:
        card = _card(area)
        _title(window, card, t('s_bt_hier_entfernt', len(removed)))
        _note(window, card, t('s_bt_hier_entfernt_h'), bottom=4)
        _note(window, card, _listing(removed, names), color=FG)
        _buttons(window, card, [
            (t('s_bt_auch_dort'),
             lambda: basetool_sync.release_removals(list(removed)), 'danger'),
            (t('s_bt_behalten'),
             lambda: basetool_sync.keep_removed(list(removed)), '')])

    # Lager: Menge auf beiden Seiten verschieden — der Spieler entscheidet.
    stock = state.get('stock') or {}
    conflicts = stock.get('conflicts') or {}
    if conflicts:
        labels = stock.get('conflict_names') or {}
        card = _card(area)
        _title(window, card, t('s_bt_lager_konflikte', len(conflicts)))
        _note(window, card, t('s_bt_lager_konflikte_h'), bottom=4)
        shown = sorted(conflicts.items())[:12]
        _note(window, card, '\n'.join(
            t('s_bt_lager_konflikt_zeile', labels.get(k, k), here, there)
            for k, (here, there) in shown)
            + ('\n' + t('s_bt_und_mehr', len(conflicts) - 12)
               if len(conflicts) > 12 else ''), color=FG)
        keys = list(conflicts)
        _buttons(window, card, [
            (t('s_bt_meine'), lambda: basetool_sync.stock_decide(keys, 'mine'),
             ''),
            (t('s_bt_deren'),
             lambda: basetool_sync.stock_decide(keys, 'theirs'), '')])

    # Hangar: hier fehlende Schiffe — nur mit Freigabe drüben entfernen.
    ships = state.get('ships') or {}
    ship_names = ships.get('names') or {}
    removed = ships.get('local_removed') or {}
    if removed:
        card = _card(area)
        _title(window, card, t('s_bt_hangar_weg', len(removed)))
        _note(window, card, t('s_bt_hangar_weg_h'), bottom=4)
        server = ships.get('server') or {}
        _note(window, card, ', '.join(sorted(
            (((server.get(sid) or {}).get('shipType') or {}).get('name')
             or ship_names.get(ext) or ext) for ext, sid in removed.items())),
            color=FG)
        exts = list(removed)
        _buttons(window, card, [
            (t('s_bt_auch_dort'), lambda: basetool_sync.ships_release(exts),
             'danger'),
            (t('s_bt_hangar_behalten'), lambda: basetool_sync.ships_keep(exts),
             '')])
    elsewhere = ships.get('removed_elsewhere') or []
    if elsewhere:
        card = _card(area)
        _title(window, card, t('s_bt_hangar_dort_weg', len(elsewhere)))
        _note(window, card, t('s_bt_hangar_dort_weg_h'), bottom=4)
        _note(window, card, ', '.join(sorted(ship_names.get(e, e)
                                             for e in elsewhere)), color=FG)
        _buttons(window, card, [
            (t('s_bt_wieder_hoch'),
             lambda: basetool_sync.ships_override(list(elsewhere)), '')])


def _listing(entries, names, limit=12):
    shown = sorted(names.get(bt) or key for key, bt in entries.items())
    text = ', '.join(shown[:limit])
    if len(shown) > limit:
        text += ' ' + t('s_bt_und_mehr', len(shown) - limit)
    return text


def _label_row(window, card, redraw):
    """Der Name dieser Installation — so steht sie im Basetool unter
    „Verbundene Anwendungen". ⚠ Nie der Rechnername."""
    from .main_window import round_entry
    row = tk.Frame(card, bg=SURFACE)
    row.pack(fill='x', padx=16, pady=(0, 10))
    tk.Label(row, text=t('s_bt_name'), bg=SURFACE, fg=SUB,
             font=window.f_small).pack(side='left', padx=(0, 8))
    variable = tk.StringVar(master=row, value=basetool_sync.label())
    box = round_entry(row, variable, window.f_small, theme.FIELD, LINE,
                      ACCENT, FG, width=24)
    box.holder.pack(side='left')
    hint = tk.Label(row, text='', bg=SURFACE, fg=theme.YELLOW,
                    font=window.f_small)
    hint.pack(side='left', padx=(8, 0))

    def keep(_event=None):
        value = variable.get().strip()
        if not value:
            value = basetool.default_label()
            variable.set(value)
        if not basetool.label_valid(value):
            hint.configure(text=t('s_bt_name_falsch'))
            return
        hint.configure(text='')
        if value != basetool_sync.label():
            paths.set_setting(basetool_sync.SETTING_LABEL, value)
            basetool_sync.request_now()

    box.bind('<FocusOut>', keep)
    box.bind('<Return>', keep)
    # ⚠ Erster Test (28.09.2026): „Was muss da stehen? Mein Handle?" — nein.
    _note(window, card, t('s_bt_name_h'))


def _start_login(window, login, redraw):
    """Geräte-Anmeldung im Hintergrund; der Code erscheint auf der Seite."""
    scopes = [basetool.SCOPE_CONNECT] + basetool_sync.wanted_scopes()
    login['message'] = ''
    login['current'] = 'starting'
    redraw()

    def work():
        conn = basetool.CONNECTION
        try:
            device = conn.start_login(scopes)
        except basetool.ApiError as error:
            from . import errors
            errors.record('basetool_page.login', RuntimeError(error.describe()))
            login['current'] = None
            login['message'] = error_text(error.code)
            _TK_CALLS.put(redraw)
            return
        except secret_store.SecretError as error:
            from . import errors
            errors.record('basetool_page.login', error)
            login['current'] = None
            login['message'] = error_text(error.code)
            _TK_CALLS.put(redraw)
            return
        except Exception as exc:
            from . import errors
            errors.record('basetool_page.login', exc)
            login['current'] = None
            login['message'] = error_text('INTERNAL')
            _TK_CALLS.put(redraw)
            return
        login['current'] = device
        _TK_CALLS.put(redraw)
        while login['current'] is device:
            time.sleep(device.interval)
            if login['current'] is not device:
                return                  # abgebrochen
            try:
                result = conn.poll_login(device)
            except basetool.ApiError as error:
                from . import errors
                errors.record('basetool_page.poll', RuntimeError(error.describe()))
                result = 'error:' + error.code
            except secret_store.SecretError as error:
                from . import errors
                errors.record('basetool_page.poll', error)
                result = 'error:' + error.code
            if result in ('pending', 'slow_down'):
                continue
            login['current'] = None
            if result == 'ok':
                login['message'] = t('s_bt_angemeldet')
                basetool_sync.request_now()
            elif result == 'denied':
                login['message'] = t('s_bt_abgelehnt')
            elif result == 'expired':
                login['message'] = t('s_bt_abgelaufen')
            else:
                login['message'] = error_text(result.split(':', 1)[-1])
            _TK_CALLS.put(redraw)
            return

    threading.Thread(target=work, daemon=True).start()


def _copy(window, text):
    """In die Zwischenablage — und sagen, dass es geklappt hat.

    Über `report.to_archive`, denselben Weg wie „Bericht kopieren": Der hält
    die Ablage auch nach dem Beenden (`update()`) und schluckt Fehler."""
    from . import report
    if report.to_archive(text, window.root):
        window.say(t('s_bt_kopiert'))


def _login_box(window, card, login, redraw):
    device = login['current']
    if device == 'starting':
        _note(window, card, t('s_bt_hole_code'))
        return
    _note(window, card, t('s_bt_code_h'), bottom=4)
    # Code und Adresse je mit „Kopieren" — gewünscht beim ersten Test
    # (28.09.2026: „Adresse nicht anklickbar", „auch nicht kopierbar").
    # ⚠ Die Adresse ist die NACKTE `verification_uri` ohne Code; einen Link
    # mit eingebautem Code gibt es hier nirgends, auch nicht zum Kopieren.
    code_row = tk.Frame(card, bg=SURFACE)
    code_row.pack(fill='x', padx=16)
    tk.Label(code_row, text=device.user_code, bg=SURFACE, fg=ACCENT,
             font=window.f_title, anchor='w').pack(side='left')
    _button(window, code_row, t('s_bt_kopieren'),
            lambda: _copy(window, device.user_code)).pack(side='left',
                                                          padx=(12, 0))
    address_row = tk.Frame(card, bg=SURFACE)
    address_row.pack(fill='x', padx=16, pady=(6, 4))
    tk.Label(address_row, text=t('s_bt_adresse_h'), bg=SURFACE, fg=FG,
             font=window.f_small).pack(side='left', padx=(0, 6))
    _button(window, address_row, t('s_bt_kopieren'),
            lambda: _copy(window, device.verification_uri)).pack(side='right')
    link_box = tk.Frame(address_row, bg=SURFACE)
    link_box.pack(side='left', fill='x', expand=True)
    _link(window, link_box, device.verification_uri, device.verification_uri,
          SURFACE)
    _note(window, card, t('s_bt_code_warnung'), color=theme.YELLOW)

    def cancel():
        login['current'] = None
        redraw()

    _buttons(window, card, [
        (t('s_bt_browser'),
         lambda: paths.open_in_browser(device.verification_uri), 'strong'),
        (t('s_bt_abbrechen'), cancel, '')])
