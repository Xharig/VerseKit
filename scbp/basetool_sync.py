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
Der Bauplan-Abgleich mit dem KRT Profit Basetool — Takt, Ablauf, Stand.

Die Entscheidungen trifft `exchange_sync.plan_sync` (ohne Netz, geprüft);
dieses Modul holt und schickt und merkt sich den Stand. Die Regeln stammen aus
der Sync-Anleitung des Basetools (`docs/exchange/sync-guide.md`) und werden
gegen genau diese Liste geprüft:

1. **Erst holen, dann senden.** Jeder Durchgang liest zuerst den
   Änderungsfeed bis zum Ende, danach wird verglichen und gesendet, danach
   noch einmal geholt — das ist der neue abgeglichene Stand.
2. **Der erste Abgleich fügt nur hinzu.** Ohne abgeglichenen Stand lässt sich
   „entfernt" nicht von „nie da gewesen" unterscheiden.
3. **Entfernt wird nur aus dem Vergleich mit dem letzten Stand** — und bei
   VerseKit zusätzlich nur, wenn der Spieler es auf der Seite freigibt.
4. **Was woanders entfernt wurde, kommt nie still zurück.** Es wird als
   Konflikt gezeigt; `override` geht nur mit Zustimmung hinaus.
5. **Account-Prüfung vor dem ersten Abgleich eines neu erkannten Accounts** —
   bei `mismatch` wird nicht abgeglichen.
6. **Stand, Verknüpfungen und Cursor je Installation**, benannt nach deren
   `installationId`. ⚠ Wichtig bei einem Datenordner, den Windows und Linux
   teilen: Jede Installation hat ihre eigene Datei, keine liest die der
   anderen.
7. **Takt:** beim Start, nach einer eigenen Änderung, sonst höchstens alle
   5 Minuten. Nach Fehlern 5 s, verdoppelt bis 5 min, mit Zufall, nie kürzer
   als `Retry-After`.

⚠⚠ **Nur der Watcher-Faden schreibt den Bestand.** Der Abgleich läuft in
einem eigenen Faden und reicht seine Änderungen über `watcher.basetool_apply`
weiter — sonst schriebe der Watcher beim nächsten Fund seinen alten Stand
zurück, und was aus dem Basetool kam, wäre wieder weg.
"""
import hashlib
import json
import os
import re
import threading
import time

from . import basetool, collection as collection_file, exchange_sync, paths

SETTING_BLUEPRINTS = 'basetool_bauplaene'
SETTING_STOCK = 'basetool_lager'
SETTING_SHIPS = 'basetool_hangar'
SETTING_DEMAND = 'basetool_bedarf'
SETTING_ORGSTOCK = 'basetool_staffellager'
# Buchen im Staffellager als Logistiker — nur zusammen mit dem Lesen.
SETTING_ORGSTOCK_WRITE = 'basetool_staffellager_buchen'
SETTING_LABEL = 'basetool_name'
# Einheit für neue Schiffe: `''` = wie bisher (das Basetool entscheidet),
# sonst die Kennung einer Einheit.
SETTING_NEW_SHIP_UNIT = 'basetool_einheit_neu'

# Die Bereiche: Schalter und die Rechte, die er braucht. Jeder ist ab Werk
# aus, und angefragt wird nur, was eingeschaltet ist.
AREAS = ((SETTING_BLUEPRINTS, basetool.SCOPES_BLUEPRINTS),
         (SETTING_STOCK, basetool.SCOPES_STOCK),
         (SETTING_SHIPS, basetool.SCOPES_HANGAR),
         (SETTING_DEMAND, basetool.SCOPES_DEMAND),
         (SETTING_ORGSTOCK, basetool.SCOPES_ORGSTOCK))

CADENCE = 300           # höchstens alle 5 Minuten ohne Anlass
AFTER_CHANGE = 20       # eigene Änderungen kurz sammeln, dann einmal abgleichen
PAGE_LIMIT = 500
RESOLVE_MAX = 500
# Nicht Zuordenbares wird höchstens einmal am Tag neu angefragt.
RESOLVE_AGAIN = 24 * 3600
# Die Bestätigungsadresse einer Massenänderung gilt 30 Minuten und ist ein
# Geheimnis — sie steht nur im Speicher, nie in einer Datei.
CONFIRM_LIFETIME = 30 * 60

# Die Sendewege, die das Basetool mit einer Massenänderungs-Sperre anhalten kann.
ROUTE_BLUEPRINTS = '/me/blueprints/changes'
ROUTE_STOCK = '/me/stock/changes'
ROUTE_SHIPS = '/me/ships/changes'
ROUTES = {SETTING_BLUEPRINTS: ROUTE_BLUEPRINTS, SETTING_STOCK: ROUTE_STOCK,
          SETTING_SHIPS: ROUTE_SHIPS}

_lock = threading.Lock()
# `confirm`: {Sendeweg: {'url', 'until'}} — die Bestätigungsseite je Weg.
STATUS = {'state': 'idle', 'code': '', 'running': False, 'last_sync': None,
          'counts': {}, 'confirm': {}, 'account': None}
# `halted`: nach einem Stopp-Code (Programm gesperrt, Recht fehlt …) gibt es
# keinen neuen Versuch von selbst — auch nicht nach einer eigenen Änderung.
_schedule = {'next': 0.0, 'attempt': 0, 'changed_at': None, 'halted': False}
LISTENERS = []
# Der laufende Watcher — die Seite reicht Entscheidungen über ihn weiter.
_WATCHER = [None]
# Lager und Hangar werden im Tk-Faden geschrieben (`Overlay._im_tk`) — dort
# bearbeitet die Seite sie über die Listenposition. Danach baut das
# Hauptfenster die betroffenen Seiten neu (`MainWindow.pages_changed`).
IN_TK = [None]
PAGES_CHANGED = [None]


# ------------------------------------------------------------- Zustand
def area_on(setting):
    return paths.setting_bool(setting, False)


def enabled():
    return any(area_on(s) for s, _ in AREAS)


def wanted_scopes():
    """Die Rechte aller eingeschalteten Bereiche — nur die werden angefragt."""
    out = []
    for setting, scopes in AREAS:
        if area_on(setting):
            out += [s for s in scopes if s not in out]
    # Zu Hangar und Lager das freiwillige Recht für die Einheiten-Liste —
    # erst nach der Freigabe (siehe `basetool.REQUEST_ORGUNITS`). Das Lager
    # braucht sie für die Auswahl beim Teilen in den Pool einer Einheit.
    if (area_on(SETTING_SHIPS) or area_on(SETTING_STOCK)) \
            and basetool.REQUEST_ORGUNITS:
        out += [s for s in basetool.SCOPES_ORGUNITS if s not in out]
    for scopes in optional_scopes():
        out += [s for s in scopes if s not in out]
    return out


def optional_scopes():
    """Die freiwilligen Rechte eingeschalteter Bereiche — ohne sie läuft der
    Bereich, nur ohne das Zusätzliche (Aufträge einzeln, Einträge je
    Besitzer, Buchen)."""
    out = []
    if area_on(SETTING_DEMAND):
        out.append(basetool.SCOPES_DEMAND_DETAILS)
    if area_on(SETTING_ORGSTOCK):
        out.append(basetool.SCOPES_ORGSTOCK_DETAILS)
        if area_on(SETTING_ORGSTOCK_WRITE):
            out.append(basetool.SCOPES_ORGSTOCK_WRITE)
    return out


def missing_optional(granted):
    """Freiwillige Rechte, die eingeschaltet, aber nicht erteilt sind."""
    granted = set(granted or ())
    return [s for scopes in optional_scopes() for s in scopes
            if s not in granted]


def _in_tk(work):
    """Im Tk-Faden erledigen — ohne Oberfläche (Prüflauf) gleich hier."""
    if IN_TK[0] is None:
        work()
    else:
        IN_TK[0](work)


def _pages_changed(ids):
    hook = PAGES_CHANGED[0]
    if hook is not None:
        try:
            hook(ids)
        except Exception as exc:
            from . import errors
            errors.record('basetool_sync.seiten', exc)


def label():
    return paths.setting(SETTING_LABEL) or basetool.default_label()


def _state_file(installation_id):
    safe = re.sub(r'[^A-Za-z0-9_.-]', '_', installation_id or 'unbekannt')
    return paths.app_file('basetool-%s.json' % safe)


def _empty_state(installation_id):
    return {'format': 1, 'installation_id': installation_id, 'cursor': None,
            'baseline': {}, 'links': {}, 'resolved': {}, 'conflicts': {},
            'pending_removals': [], 'overrides': [], 'own_removed_keys': [],
            'local_removed': {}, 'accounts': {}, 'label_sent': None}


def load_state(installation_id):
    try:
        with open(_state_file(installation_id), encoding='utf-8') as handle:
            data = json.load(handle)
        if isinstance(data, dict) and data.get('format') == 1:
            base = _empty_state(installation_id)
            base.update(data)
            return base
    except (OSError, ValueError):
        pass
    return _empty_state(installation_id)


# Vorgemerkte Änderungen am Stand aus der Basetool-Seite, jede mehrfach
# anwendbar. Ein laufender Durchgang hat den Stand schon geladen und schreibt
# ihn am Ende zurück — `save_state` wendet deshalb alles Vorgemerkte vor dem
# Schreiben noch einmal an. Geleert wird beim Laden zu Beginn des nächsten
# Durchgangs.
_inbox = []


def save_state(state):
    with _lock:
        pending = list(_inbox)
    for change in pending:
        change(state)
    paths.save_json(_state_file(state['installation_id']), state)


def _decide(change):
    """Eine Änderung am Stand sofort speichern, für einen laufenden Durchgang
    vormerken und einen Abgleich anfordern. Gibt False zurück, wenn es noch
    keinen Stand gibt."""
    state = current_state()
    if state is None:
        return False
    change(state)
    save_state(state)
    with _lock:
        _inbox.append(change)
    request_now()
    return True


def current_state():
    """Der Stand der Installation, mit der zuletzt abgeglichen wurde."""
    installation_id = STATUS.get('installation_id')
    return load_state(installation_id) if installation_id else None


def _confirm_address(error):
    """Die Bestätigungsseite aus der Fehlerantwort — nur eine https-Adresse
    beim Basetool, sonst `None` (dann erscheint kein Knopf)."""
    url = (error.problem or {}).get('confirmationUrl')
    safe = basetool.trusted_address(url)
    if url and not safe:
        from . import errors
        errors.record('basetool_sync.confirm_address',
                      ValueError('Adresse außerhalb des Basetools'), str(url)[:80])
    return safe or None


# ------------------------------------------- Zurückgehaltene Sendungen
# Antwortet das Basetool mit `MASS_CHANGE_CONFIRMATION_REQUIRED`, hält es die
# Sendung zurück, bis das Mitglied sie im Browser bestätigt. Jede neue Sendung
# ersetzt die zurückgehaltene und macht den Bestätigungslink ungültig. Darum
# steht im Stand je Sendeweg `held[route] = {'until', 'fingerprint'}`:
#
# * Bis `until` geht über diesen Weg nichts hinaus; geholt wird weiter.
# * Danach geht dieselbe Sendung (gleicher Fingerabdruck) nicht von selbst
#   noch einmal hinaus — nur, wenn sich der Plan geändert hat oder der
#   Spieler einen Abgleich-Knopf drückt (`release_held`).
# * Ist nichts mehr zu senden (bestätigt oder hier zurückgenommen), fällt der
#   Eintrag weg.
#
# Die Bestätigungsadresse steht nur im Speicher (`STATUS['confirm']`), nie im
# Stand. Nach einem Neustart gibt es zu einer laufenden Sperre keinen Knopf.

# {Sendeweg}: gesetzt von `release_held`, übernommen und geleert zu Beginn
# eines Durchgangs.
_release_requested = [set()]
# {Sendeweg}: für den laufenden Durchgang — dort darf eine abgelaufene Sperre
# einmal senden.
_release_now = [set()]


def op_hash(op):
    """Eine Anweisung ohne `opId` als kurzer Abdruck."""
    body = json.dumps({k: v for k, v in op.items() if k != 'opId'},
                      sort_keys=True, ensure_ascii=True)
    return hashlib.sha256(body.encode('ascii')).hexdigest()


def fingerprint(change_sets, skip=()):
    """Ein Abdruck der Anweisungen, ohne `opId` und ohne die Aufteilung in
    Sendungen — gleich, solange dasselbe hinausginge. Anweisungen, deren
    `op_hash` in `skip` steht, zählen nicht mit."""
    skip = set(skip)
    hashes = [h for change_set, _keys in change_sets
              for h in map(op_hash, change_set.get('ops') or ())
              if h not in skip]
    return hashlib.sha256('|'.join(hashes).encode('ascii')).hexdigest()


def _refused_ops(change_set, keys, refused_keys):
    """Die Anweisungen einer angenommenen Sendung, die das Basetool für
    `refused_keys` abgelehnt hat — sie stehen im nächsten Plan wieder."""
    return [op for op, key in zip(change_set.get('ops') or (), keys)
            if key in refused_keys]


def _hold(state, route, change_sets, error=None, refused=(), now=None):
    """`route` sperren. `change_sets` sind die Sendungen ab der
    zurückgehaltenen; `refused` die abgelehnten Anweisungen der angenommenen
    davor — beide zusammen sind, was der nächste Plan wieder enthält."""
    now = now or time.time()
    skip = sorted(op_hash(op) for op in refused)
    state.setdefault('held', {})[route] = {
        'until': now + CONFIRM_LIFETIME, 'skip': skip,
        'fingerprint': fingerprint(change_sets)}
    with _lock:
        confirm = dict(STATUS.get('confirm') or {})
    confirm[route] = {'url': _confirm_address(error) if error else None,
                      'until': now + CONFIRM_LIFETIME}
    _set(confirm=confirm)


def _clear_hold(state, route):
    if (state.get('held') or {}).pop(route, None) is None:
        return
    with _lock:
        confirm = dict(STATUS.get('confirm') or {})
    if confirm.pop(route, None) is not None:
        _set(confirm=confirm)


def confirm_link(route, now=None):
    """Die Bestätigungsseite für `route`, solange sie gilt — sonst `None`."""
    now = now or time.time()
    with _lock:
        entry = (STATUS.get('confirm') or {}).get(route) or {}
    if entry.get('url') and now < float(entry.get('until') or 0):
        return entry['url']
    return None


def may_send(state, route, change_sets, now=None):
    """Darf über `route` jetzt gesendet werden? Siehe oben."""
    hold = (state.get('held') or {}).get(route)
    if hold is None:
        return True
    if not change_sets:
        _clear_hold(state, route)
        return True
    now = now or time.time()
    if now < float(hold.get('until') or 0):
        return False
    if route in _release_now[0] or fingerprint(
            change_sets, hold.get('skip') or ()) != hold.get('fingerprint'):
        _clear_hold(state, route)
        return True
    return False


def held_routes(state, now=None):
    """{Sendeweg: 'pending' | 'expired'} — was gerade zurückgehalten ist."""
    now = now or time.time()
    return {route: ('pending' if now < float(hold.get('until') or 0)
                    else 'expired')
            for route, hold in (state.get('held') or {}).items()}


def _set(**values):
    with _lock:
        STATUS.update(values)
    for listener in list(LISTENERS):
        try:
            listener()
        except Exception:
            pass


# ------------------------------------------------------------------ Takt
def local_changed():
    """Der eigene Bestand hat sich geändert — bald abgleichen (gesammelt).

    Schreibt der Abgleich selbst (`_applying`), zählt das nicht."""
    if enabled() and not _applying[0]:
        with _lock:
            _schedule['changed_at'] = time.time()


# Gesetzt, solange der Abgleich Übernommenes in die eigenen Lager schreibt.
_applying = [False]


def store_saved():
    """Rohstofflager, Handelslager oder Hangar wurde geschrieben."""
    if area_on(SETTING_STOCK) or area_on(SETTING_SHIPS):
        local_changed()


def request_now():
    """Sofort abgleichen — ohne Warten, aber nie parallel. Läuft gerade ein
    Durchgang, folgt der nächste direkt danach."""
    with _lock:
        _schedule['next'] = 0.0
        _schedule['attempt'] = 0
        _schedule['halted'] = False
        _schedule['again'] = True


def release_held(routes=None):
    """Ein Abgleich-Knopf: sofort abgleichen, und eine abgelaufene
    Massenänderungs-Sperre auf `routes` (ohne Angabe: alle Wege) darf im
    nächsten Durchgang einmal neu senden. Eine laufende Sperre bleibt."""
    with _lock:
        _release_requested[0] |= set(ROUTES.values() if routes is None
                                     else routes)
    request_now()


def due(now=None):
    now = now or time.time()
    with _lock:
        if STATUS['running'] or _schedule['halted']:
            return False
        changed = _schedule['changed_at']
        if changed and now - changed >= AFTER_CHANGE and _schedule['attempt'] == 0:
            return True
        return now >= _schedule['next']


def tick(watcher):
    """Aus dem Watcher-Faden: ist ein Abgleich fällig, läuft er im
    Hintergrund an. Ohne Freischaltung, ohne Verbindung — nichts."""
    _WATCHER[0] = watcher
    if not enabled() or not due():
        return
    if not basetool.CONNECTION.connected():
        with _lock:
            _schedule['next'] = time.time() + CADENCE
        return
    with _lock:
        _schedule['again'] = False
    _set(running=True)
    threading.Thread(target=_run_guarded, args=(watcher,), daemon=True).start()


def _run_guarded(watcher):
    try:
        run(watcher)
        with _lock:
            _schedule['attempt'] = 0
            # Während des Durchgangs angefordert oder vorgemerkt: gleich noch
            # einmal, nicht erst im nächsten Takt.
            nochmal = _schedule.get('again') or bool(_inbox)
            _schedule['next'] = 0.0 if nochmal else time.time() + CADENCE
            _schedule['changed_at'] = None
    except basetool.ApiError as error:
        _after_error(error)
    except Exception as exc:
        from . import errors
        errors.record('basetool_sync.run', exc)
        _after_error(basetool.ApiError('INTERNAL'))
    finally:
        _set(running=False)


def _after_error(error):
    # ⚠ Ins Fehlerprotokoll — sonst steht auf der Seite nur „abgelehnt
    # (HTTP_503)", und niemand kann sagen, welche Anfrage es war.
    from . import errors
    errors.record('basetool.sync', RuntimeError(error.describe()))
    action = error.action
    with _lock:
        if action == 'retry':
            _schedule['attempt'] += 1
            _schedule['next'] = time.time() + basetool.backoff(
                _schedule['attempt'], error.retry_after)
        else:
            # Halt ohne Zeitgeber: weiter erst beim nächsten Programmstart
            # (`_schedule` beginnt dann bei 0) oder über `request_now()`.
            _schedule['attempt'] = 0
            _schedule['next'] = float('inf')
            _schedule['halted'] = True
    _set(state='error', code=error.code)


# ------------------------------------------------------------- Ablauf
def _fetch_all(conn, cursor, route='/me/blueprints'):
    """Den Feed (mit Cursor) oder einen Schnappschuss (ohne) bis zum Ende."""
    pages = []
    while True:
        query = {'limit': PAGE_LIMIT}
        if cursor:
            query['cursor'] = cursor
        page = conn.request('GET', route, query=query)
        pages.append(page)
        cursor = page.get('nextCursor') or cursor
        if not page.get('hasMore'):
            return pages, cursor


def _pull(conn, state):
    """Holen. Rückgabe: (Server-Stand, Löschmarken, Cursor, resync?)."""
    baseline = dict(state.get('baseline') or {})
    if state.get('cursor'):
        try:
            pages, cursor = _fetch_all(conn, state['cursor'])
        except basetool.ApiError as error:
            if error.code != 'CURSOR_EXPIRED':
                raise
            pages, cursor = _fetch_all(conn, None)
            items, _stones, cursor = exchange_sync.merge_pages(pages)
            server = {item['key']: item for item in items}
            # Nach einem abgelaufenen Cursor wird gegen den LETZTEN Stand
            # abgeglichen — das ist ausdrücklich kein erster Abgleich.
            return server, [], cursor, bool(baseline)
        items, stones, cursor = exchange_sync.merge_pages(pages)
        server = dict(baseline)
        for stone in stones:
            server.pop(stone.get('key'), None)
        for item in items:
            server[item['key']] = item
        return server, stones, cursor, False
    pages, cursor = _fetch_all(conn, None)
    items, _stones, cursor = exchange_sync.merge_pages(pages)
    return {item['key']: item for item in items}, [], cursor, False


def _resolve(conn, local, state):
    """`bt` für jeden Bauplan, der noch keinen hat — über `catalog/resolve`."""
    resolved = state.setdefault('resolved', {})
    now = time.time()
    todo = []
    for key, entry in sorted(local.items()):
        known = resolved.get(key)
        if isinstance(known, str):
            continue
        if isinstance(known, dict) and now - known.get('at', 0) < RESOLVE_AGAIN:
            continue
        if entry['source'] in exchange_sync.NEVER_SENT:
            continue
        todo.append(key)
    for start in range(0, len(todo), RESOLVE_MAX):
        chunk = todo[start:start + RESOLVE_MAX]
        refs = [exchange_sync.item_ref(local[k]['name'], local[k]['tag'])
                for k in chunk]
        answer = conn.request('POST', '/catalog/resolve',
                              {'kind': 'BLUEPRINT', 'refs': refs})
        for result in answer.get('results') or ():
            index = result.get('index')
            if not isinstance(index, int) or not 0 <= index < len(chunk):
                continue
            key = chunk[index]
            if result.get('status') == 'resolved' and (
                    (result.get('ref') or {}).get('bt')):
                resolved[key] = result['ref']['bt']
            else:
                resolved[key] = {'status': result.get('status') or 'unmatched',
                                 'at': now}
    return {k: v for k, v in resolved.items() if isinstance(v, str)}


def _account_ok(conn, state):
    """Account-Prüfung vor dem ersten Abgleich eines Accounts.

    Rückgabe: None (weiter) oder der Grund zum Anhalten."""
    from . import logsource
    handle = logsource.own_account()
    if not handle:
        return 'account_none'           # ohne eigenen Account kein Abgleich
    if handle == logsource.ALL_ACCOUNTS:
        return 'account_all'
    lowered = handle.lower()
    accounts = state.setdefault('accounts', {})
    result = accounts.get(lowered)
    if result is None:
        result = conn.account_check(handle)
        accounts[lowered] = result
    _set(account=result)
    if result in ('match', 'confirmed'):
        return None
    return 'account_' + result          # account_mismatch / account_unknown


_FOREIGN = {'at': 0.0, 'account': None, 'names': frozenset()}
FOREIGN_AGAIN = 3600


def _foreign_only():
    """Baupläne, die nur in Protokollen anderer Accounts stehen (Vergleichsform).

    Das Durchlesen aller Sicherungen ist teuer; das Ergebnis gilt eine Stunde
    und wird bei einem Account-Wechsel neu ermittelt."""
    from . import logsource
    own = logsource.own_account()
    now = time.time()
    if _FOREIGN['account'] == own and now - _FOREIGN['at'] < FOREIGN_AGAIN:
        return _FOREIGN['names']
    names = frozenset(collection_file.norm(n)
                      for n in logsource.foreign_only_blueprints(own=own))
    _FOREIGN.update(at=now, account=own, names=names)
    return names


def confirm_account():
    """Der Spieler bestätigt auf der Seite: „Das ist mein Account"."""
    from . import logsource
    state = current_state()
    handle = logsource.own_account()
    if state is None or not handle or handle == logsource.ALL_ACCOUNTS:
        return False
    state.setdefault('accounts', {})[handle.lower()] = 'confirmed'
    save_state(state)
    request_now()
    return True


def _local_time(iso):
    """RFC 3339 in UTC -> Ortszeit, wie der Bestand sie führt."""
    if not iso:
        return None
    try:
        import calendar
        stamp = calendar.timegm(time.strptime(iso[:19], '%Y-%m-%dT%H:%M:%S'))
        return time.strftime('%Y-%m-%d %H:%M:%S', time.localtime(stamp))
    except (ValueError, OverflowError):
        return None


_run_lock = threading.Lock()


def run(watcher):
    """Ein vollständiger Durchgang. Wirft `basetool.ApiError` bei Absagen.

    ⚠⚠ **Nie zwei gleichzeitig.** Laufen der Takt und ein zweiter Anstoß
    zugleich, liest der eine den Feed, während der andere schon sendet — die
    Löschmarke kommt beim zweiten nicht mehr an, und ein im Web gelöschter
    Bauplan geht wieder hinaus. Wer die Sperre nicht
    bekommt, lässt den Durchgang aus; der nächste Takt holt ihn nach."""
    if not _run_lock.acquire(blocking=False):
        return
    try:
        _run(watcher)
    finally:
        _run_lock.release()


def _run(watcher):
    conn = basetool.CONNECTION
    _set(state='running', code='')

    doc = conn.service_document()
    installation_id = (doc.get('installationId') or '').strip()
    if not installation_id:
        # Ohne Kennung keine eigene Zustandsdatei — und eine gemeinsame
        # Ausweichdatei würde Stände verschiedener Installationen mischen.
        raise basetool.ApiError('INSTALLATION_ID_MISSING', 200)
    capabilities = set(doc.get('capabilities') or ())
    _set(installation_id=installation_id, capabilities=sorted(capabilities),
         min_version=doc.get('minClientVersion'))
    # Zu alt fürs Basetool? Dann gar nicht erst anfangen, sondern zum
    # Aktualisieren auffordern — das Gateway lehnte ohnehin jede Anfrage ab.
    minimum = doc.get('minClientVersion')
    if minimum:
        from . import errors, updater
        own = errors.VERSION[0]
        if own and updater.is_newer(minimum, own):
            raise basetool.ApiError('CLIENT_VERSION_UNSUPPORTED', 403)
    # Je eingeschaltetem Bereich: ist das Recht da? Fehlt es für einen,
    # laufen die anderen trotzdem — die Seite sagt, welches fehlt.
    ready = [setting for setting, scopes in AREAS
             if area_on(setting) and set(scopes) <= capabilities]
    missing = [setting for setting, scopes in AREAS
               if area_on(setting) and not set(scopes) <= capabilities]
    if not ready:
        raise basetool.ApiError('SCOPE_MISSING', 403)

    # Was bis hierher vorgemerkt wurde, steht schon in der Datei.
    with _lock:
        del _inbox[:]
        _release_now[0] = set(_release_requested[0])
        _release_requested[0] = set()
    state = load_state(installation_id)
    wanted_label = label()
    if state.get('label_sent') != wanted_label:
        conn.label_installation(wanted_label)
        state['label_sent'] = wanted_label
        save_state(state)

    stop = _account_ok(conn, state)
    save_state(state)
    if stop:
        _set(state='stopped', code=stop)
        return

    counts = {}
    if SETTING_BLUEPRINTS in ready:
        counts.update(_sync_blueprints(conn, state, watcher))
    if SETTING_STOCK in ready:
        counts['stock'] = _sync_stock(conn, state)
        changed = (state.get('stock') or {}).pop('split_changed', False)
        save_state(state)
        if changed:
            _in_tk(_split_changed)
    from . import exchange_orgunits
    def _units_seen():
        return [(u['id'], u['name'], u['shorthand'])
                for u in exchange_orgunits.units()]
    units_before = _units_seen()
    units_ok = False
    if (SETTING_SHIPS in ready or SETTING_STOCK in ready) \
            and set(basetool.SCOPES_ORGUNITS) <= capabilities:
        # ⚠ Abgefangen: Ohne die Liste fehlt nur die Auswahl — der
        # Abgleich von Hangar und Lager darf daran nicht scheitern.
        try:
            exchange_orgunits.refresh(conn)
            units_ok = True
        except Exception as exc:
            exchange_orgunits.forget()
            from . import errors
            errors.record('basetool_sync.orgunits', exc)
    else:
        exchange_orgunits.forget()
    if units_before != _units_seen():
        # Die Auswahl auf den Schiffskarten und beim Teilen kommt, geht oder
        # heißt anders.
        _in_tk(lambda: _pages_changed(['hangar']))
        _in_tk(_split_changed)
    if SETTING_SHIPS in ready:
        counts['ships'] = _sync_ships(conn, state, units_ok)
        save_state(state)
    from . import exchange_demand
    if SETTING_DEMAND in ready:
        counts['demand'] = exchange_demand.refresh(conn)
    else:
        exchange_demand.forget()
    _in_tk(lambda: _pages_changed(['bedarf']))
    from . import exchange_orgstock
    if SETTING_ORGSTOCK in ready:
        # ⚠ Abgefangen wie die Einheiten: Ein Fehler hier hält die übrigen
        # Bereiche nicht an, die Seite zeigt dann den letzten Stand.
        try:
            counts['orgstock'] = exchange_orgstock.refresh(
                conn,
                details=set(basetool.SCOPES_ORGSTOCK_DETAILS) <= capabilities,
                write=area_on(SETTING_ORGSTOCK_WRITE)
                and set(basetool.SCOPES_ORGSTOCK_WRITE) <= capabilities)
        except basetool.ApiError as error:
            if error.action != 'retry':
                exchange_orgstock.forget()
            from . import errors
            errors.record('basetool_sync.orgstock',
                          RuntimeError(error.describe()))
            counts['orgstock'] = {'error': error.code}
    else:
        exchange_orgstock.forget()
    _in_tk(lambda: _pages_changed(['staffellager']))
    # Gemahnt wird nur, wo eine neue Anmeldung hilft: Rechte, die das Basetool
    # noch nicht anbietet, kann niemand erteilen.
    askable = [setting for setting, scopes in AREAS if setting in missing
               and set(scopes) - capabilities - set(conn.not_offered(scopes))]
    _set(state='ok', code='SCOPE_MISSING' if askable else '', counts=counts,
         missing=missing, last_sync=time.strftime('%Y-%m-%d %H:%M:%S'))


def _sync_blueprints(conn, state, watcher):
    """Baupläne: holen, vergleichen, senden, neu holen, übernehmen."""
    # 1) Holen
    server, stones, cursor, resync = _pull(conn, state)

    # 2) Vergleichen
    stock = collection_file.load()
    local = exchange_sync.own_blueprints(
        exchange_sync.local_blueprints(stock, _tags()),
        _foreign_only(), state.get('links') or {})
    resolved = _resolve(conn, local, state)
    plan = exchange_sync.plan_sync(local, resolved, server, stones, state,
                                   resync=resync)

    # 3) Senden
    sent_removals, counts = [], {'added': 0, 'removed': 0, 'unchanged': 0,
                                  'unmatched': 0, 'ambiguous': 0}
    new_conflicts = dict(plan['conflicts'])
    change_sets = exchange_sync.sync_change_sets(plan, local)
    # Kennungen der Sendungen, die das Basetool angenommen hat — nur deren
    # Löschungen und `override` gelten als erledigt.
    posted, refused = set(), []
    if not may_send(state, ROUTE_BLUEPRINTS, change_sets):
        counts['paused'] = 1
        change_sets = []
    for number, (change_set, targets) in enumerate(change_sets):
        try:
            result = conn.request('POST', ROUTE_BLUEPRINTS, change_set,
                                  idempotency_key=basetool.new_idempotency_key())
        except basetool.ApiError as error:
            if error.code == 'MASS_CHANGE_CONFIRMATION_REQUIRED':
                _hold(state, ROUTE_BLUEPRINTS, change_sets[number:], error,
                      refused)
                counts['paused'] = 1
                break
            raise
        posted.update(targets)
        read = exchange_sync.read_result(result, targets)
        counts['unchanged'] += int(result.get('unchanged') or 0)
        counts['unmatched'] += len(read['unmatched'])
        counts['ambiguous'] += len(read['ambiguous'])
        for key in read['conflicts']:
            bt = plan['links'].get(key)
            if bt:
                new_conflicts[key] = bt
        unchanged = {targets[e['index']] for e in result.get('results') or ()
                     if e.get('result') == 'unchanged'
                     and isinstance(e.get('index'), int)
                     and 0 <= e['index'] < len(targets)}
        failed = set(read['conflicts']) | set(read['unmatched']) \
            | set(read['ambiguous']) | {k for k, _r in read['rejected']} \
            | unchanged
        refused += _refused_ops(change_set, targets, failed)
        for op, key in zip(change_set['ops'], targets):
            if key in failed:
                continue
            if op['op'] == 'remove':
                sent_removals.append(op['key'])
                counts['removed'] += 1
            else:
                counts['added'] += 1

    # 4) Noch einmal holen: der neue abgeglichene Stand
    # ⚠ Der Feed ab dem gerade geholten Cursor setzt auf dem gerade geholten
    # Stand auf, nicht auf dem alten — sonst fehlte, was Schritt 1 brachte.
    if counts['added'] or counts['removed'] or plan['override']:
        state['baseline'], state['cursor'] = server, cursor
        server, _stones, cursor, _resync = _pull(conn, state)

    # 5) Hier übernehmen, was drüben dazukam oder woanders entfernt wurde
    changes, new_links = [], dict(plan['links'])
    known = _catalog_names()
    for bt in plan['local_add']:
        item = server.get(bt) or {}
        name = ((item.get('ref') or {}).get('name') or '').strip()
        if not name:
            continue
        changes.append(('add', name, _local_time(item.get('acquiredAt'))))
        key = collection_file.norm(collection_file.catalog_name(name, known))
        new_links[key] = bt
        state['resolved'][key] = bt
    for key in plan['local_remove']:
        changes.append(('remove', key, None))
        new_links.pop(key, None)
    if changes and watcher is not None:
        watcher.basetool_apply(changes)

    done = {k for k, _bt in plan['remove'] if k in posted}
    # Freigaben, die drüben schon erledigt sind (etwa eine im Browser
    # bestätigte Sendung): Entfernt ist, was nicht mehr im Stand steht;
    # zurückgeholt, was wieder drin steht.
    linked = dict(state.get('links') or {})
    linked.update(new_links)
    pending = {k for k in set(state.get('pending_removals') or ()) - done
               if linked.get(k) in server}
    overrides = {k for k in set(state.get('overrides') or ())
                 - (set(plan['override']) & posted)
                 if linked.get(k) not in server}
    state.update({
        'cursor': cursor,
        'baseline': {bt: {'key': bt, 'ref': (item.get('ref') or {}),
                          'isDefault': bool(item.get('isDefault'))}
                     for bt, item in server.items()},
        'links': {k: bt for k, bt in new_links.items() if bt in server
                  or k in new_conflicts},
        'conflicts': {k: bt for k, bt in new_conflicts.items()
                      if bt not in server},
        'local_removed': {k: bt for k, bt in plan['local_removed'].items()
                          if bt in server},
        'pending_removals': sorted(pending),
        'overrides': sorted(overrides),
        'own_removed_keys': sorted(set(state.get('own_removed_keys') or ())
                                   | set(sent_removals)),
    })
    # Was hier entfernt und drüben freigegeben ist, bleibt verknüpft, bis es
    # drüben wirklich weg ist — sonst hielte der nächste Durchgang es für neu.
    for key, bt in state['local_removed'].items():
        state['links'][key] = bt
    save_state(state)
    counts['local_added'] = len(plan['local_add'])
    counts['local_removed'] = len(plan['local_remove'])
    counts['unresolved'] = len(plan['unresolved'])
    return counts


# ------------------------------------------------ Lager und Hangar
def _store_ok(file_name, list_key):
    """Ist die eigene Datei lesbar — oder fehlt sie ganz (dann ist leer richtig)?

    ⚠⚠ Prüfung 294: `fleet.load()` und
    `materials.load()` liefern bei einer beschädigten Datei still eine LEERE
    Liste. Der Abgleich hielte den Hangar dann für leer, übernähme alles aus
    dem Basetool und schriebe die Datei darüber — die eigenen Einträge wären
    weg. Deshalb: vorhandene, aber unlesbare Datei → Bereich nicht anfassen."""
    target = paths.app_file(file_name)
    if not os.path.exists(target):
        return True
    try:
        with open(target, encoding='utf-8') as handle:
            data = json.load(handle)
        return data.get('format') == 1 and isinstance(data.get(list_key), list)
    except (OSError, ValueError, AttributeError):
        return False


def _pull_generic(conn, sub, route, key_field):
    """Holen für Lager und Hangar. `sub` trägt `cursor` und `server` (der
    Stand des Basetools nach dem letzten Holen). Rückgabe: (Stand, resync?)."""
    known = dict(sub.get('server') or {})
    cursor = sub.get('cursor')

    def items_of(pages):
        return [item for page in pages for item in (page.get('items') or ())]

    if cursor:
        try:
            pages, cursor = _fetch_all(conn, cursor, route)
        except basetool.ApiError as error:
            if error.code != 'CURSOR_EXPIRED':
                raise
            pages, cursor = _fetch_all(conn, None, route)
            sub['cursor'] = cursor
            return {i[key_field]: i for i in items_of(pages)}, True
        for page in pages:
            for stone in page.get('removed') or ():
                known.pop(stone.get('key'), None)
            for item in page.get('items') or ():
                known[item[key_field]] = item
        sub['cursor'] = cursor
        return known, False
    pages, cursor = _fetch_all(conn, None, route)
    sub['cursor'] = cursor
    return {i[key_field]: i for i in items_of(pages)}, False


def _resolve_refs(conn, kind, refs, cache):
    """{Schlüssel: item-ref} -> {Schlüssel: bt}; merkt sich Ergebnisse in
    `cache`, Unbekanntes wird höchstens einmal am Tag neu angefragt."""
    now = time.time()
    todo = [k for k in sorted(refs)
            if not isinstance(cache.get(k), str)
            and not (isinstance(cache.get(k), dict)
                     and now - cache[k].get('at', 0) < RESOLVE_AGAIN)]
    for start in range(0, len(todo), RESOLVE_MAX):
        chunk = todo[start:start + RESOLVE_MAX]
        answer = conn.request('POST', '/catalog/resolve',
                              {'kind': kind, 'refs': [refs[k] for k in chunk]})
        for result in answer.get('results') or ():
            index = result.get('index')
            if not isinstance(index, int) or not 0 <= index < len(chunk):
                continue
            bt = (result.get('ref') or {}).get('bt')
            cache[chunk[index]] = (bt if result.get('status') == 'resolved'
                                   and bt else {'status': result.get('status'),
                                                'at': now})
    return {k: v for k, v in cache.items() if isinstance(v, str)}


_LOCATIONS = {'at': 0.0, 'places': {}}


def _locations(conn):
    """Die Lagerorte des Basetools — die einzigen, an die gebucht werden darf.
    Einmal je Stunde reicht; die Liste ändert sich selten."""
    if time.time() - _LOCATIONS['at'] > 3600 or not _LOCATIONS['places']:
        answer = conn.request('GET', '/catalog/locations')
        _LOCATIONS['places'] = {
            (item.get('name') or '').strip().lower():
                {k: v for k, v in item.items() if k in ('name', 'uex') and v}
            for item in answer.get('items') or () if item.get('name')}
        _LOCATIONS['at'] = time.time()
    return _LOCATIONS['places']


def _send_unless_held(conn, route, change_sets, state):
    """`_send` hinter der Massenänderungs-Sperre (siehe `may_send`).

    Im Zusatzzähler steht `paused`, wenn eine Sendung auf Bestätigung wartet,
    und `posted` mit den Kennungen der angenommenen Sendungen davor (bei einer
    Sperre vor dem ersten Senden leer)."""
    change_sets = list(change_sets)
    if not may_send(state, route, change_sets):
        return [], 0, {'paused': 1, 'posted': []}
    failed, applied, extra = _send(conn, route, change_sets)
    refused = extra.pop('refused', [])
    if 'held' in extra:
        held = extra.pop('held')
        _hold(state, route, change_sets[held:], extra.pop('held_error', None),
              refused)
        extra['paused'] = 1
    return failed, applied, extra


def _send(conn, route, change_sets):
    """Sendungen abschicken. Rückgabe: ([(Kennung, Ergebnis, Grund)] der
    NICHT angewandten Anweisungen, Anzahl angewandt, Zusatzzähler).

    Hält das Basetool eine Sendung zur Bestätigung zurück, endet die Schleife
    dort: Der Zusatzzähler trägt dann `held` (ihre Nummer) und `held_error`.
    `posted` nennt die Kennungen aller angenommenen Sendungen, `refused` die
    darin abgelehnten Anweisungen."""
    failed, applied, extra = [], 0, {'posted': [], 'refused': []}
    for number, (change_set, keys) in enumerate(change_sets):
        try:
            result = conn.request('POST', route, change_set,
                                  idempotency_key=basetool.new_idempotency_key())
        except basetool.ApiError as error:
            if error.code == 'MASS_CHANGE_CONFIRMATION_REQUIRED':
                extra['held'], extra['held_error'] = number, error
                break
            raise
        extra['posted'] += list(keys)
        applied += int(result.get('applied') or 0)
        for name in ('offersReduced', 'offersRemoved', 'detachedFromMissions'):
            extra[name] = extra.get(name, 0) + int(result.get(name) or 0)
        refused_keys = set()
        for entry in result.get('results') or ():
            index = entry.get('index')
            if isinstance(index, int) and 0 <= index < len(keys) \
                    and entry.get('result') != 'unchanged':
                failed.append((keys[index], entry.get('result'),
                               entry.get('reason') or ''))
                refused_keys.add(keys[index])
        extra['refused'] += _refused_ops(change_set, keys, refused_keys)
    return failed, applied, extra


def resolve_uex_items(conn, trade, sub):
    """Von Hand eingetragene Items (`stueck` mit `uex`) über ihre UEX-Nummer
    auflösen, nicht über den Namen — ein Name kann auf mehrere Gegenstände
    passen. Der bt je Nummer bleibt im Stand (`resolved_uex`) und wird danach
    ohne Anfrage benutzt.

    Gibt `{uex:<Nummer>: bt}` zurück — je Zeile über ihren Schlüssel
    (`exchange_stock.uex_key`), nie über den Namen: Eine gleichnamige Ware
    bleibt eine Ware."""
    from . import exchange_stock
    refs = {}
    for row in trade:
        key = exchange_stock.uex_key(row)
        name = (row.get('ware') or '').strip()
        if key and name:
            refs[key] = {'uexId': row['uex'], 'name': name[:200]}
    if not refs:
        return {}
    found = _resolve_refs(conn, 'ITEM', refs,
                          sub.setdefault('resolved_uex', {}))
    return {key: found[key] for key in refs if found.get(key)}


# Höchstens so oft der volle Lager-Stand, wenn Posten ohne `split` dastehen.
SPLIT_AGAIN = 24 * 3600


def _without_split(server_raw):
    return sorted(key for key, lot in (server_raw or {}).items()
                  if not isinstance((lot or {}).get('split'), dict))


def _split_complete(conn, sub, server_raw, now=None):
    """Posten ohne `split` im gemerkten Stand: einmal den vollen Stand holen
    (ohne Cursor), höchstens alle `SPLIT_AGAIN` Sekunden.

    ⚠ Der Feed liefert nur geänderte Posten. Ein Posten, der zuletzt ohne
    `split` ankam, bliebe sonst so, und Mein Lager zeigte ihn als ganz
    persönlich. Was danach noch ohne `split` dasteht, steht in
    `split_missing` (höchstens 50 Kennungen)."""
    now = now or time.time()
    missing = _without_split(server_raw)
    if missing and now - float(sub.get('split_full_at') or 0) >= SPLIT_AGAIN:
        sub['split_full_at'] = now
        sub['cursor'] = None
        server_raw, _resync = _pull_generic(conn, sub, '/me/stock', 'key')
        missing = _without_split(server_raw)
    sub['split_missing'] = missing[:50]
    return server_raw


def _sync_stock(conn, state):
    """Rohstoff- und Handelslager <-> Posten beim Basetool."""
    from . import crafting, exchange_stock, materials, trade_cargo
    if not (_store_ok(materials.FILE, 'posten')
            and _store_ok(trade_cargo.FILE, 'posten')):
        from . import errors
        errors.record('basetool_sync.lager', RuntimeError(
            'Lagerdatei unlesbar — Lager wird nicht abgeglichen'))
        return {'unreadable': 1}
    sub = state.setdefault('stock', {})
    before = sub.get('server') or {}
    server_raw, _resync = _pull_generic(conn, sub, '/me/stock', 'key')
    server_raw = _split_complete(conn, sub, server_raw)
    places = _locations(conn)
    raw, trade = materials.load(), trade_cargo.load()
    uex_bts = resolve_uex_items(conn, trade, sub)
    # Items mit UEX-Nummer gehen nicht in die Namenssuche; eine gleichnamige
    # Ware oder ein gleichnamiger Rohstoff schon.
    names = ({(r.get('material') or '').strip() for r in raw}
             | {(r.get('ware') or '').strip() for r in trade
                if not exchange_stock.uex_key(r)})
    cache = sub.setdefault('resolved', {})
    resolved = _resolve_refs(conn, 'MATERIAL',
                             {n.lower(): {'name': n[:200]} for n in names if n},
                             cache)
    resolved.update(uex_bts)
    uex_items = dict.fromkeys(uex_bts.values(), True)

    # Was dort schon steht, trägt sein `bt` selbst — auch ein Item, das als
    # MATERIAL nicht aufzulösen ist.
    for lot in server_raw.values():
        material = lot.get('material') or {}
        name = (material.get('name') or '').strip().lower()
        if name and material.get('bt') and name not in resolved:
            resolved[name] = material['bt']
    items = exchange_stock.item_flags(server_raw.values(), sub.get('items'))
    items.update(uex_items)
    rest = {n for n in names if n and n.lower() not in resolved}
    if rest:
        found = _resolve_refs(conn, 'ITEM',
                              {n.lower(): {'name': n[:200]} for n in rest},
                              sub.setdefault('resolved_items', {}))
        for name, bt in found.items():
            resolved.setdefault(name, bt)
            items.setdefault(bt, True)
    sub['items'] = items
    sub.pop('commodity', None)

    def resolver(name):
        return resolved.get((name or '').strip().lower())

    server = exchange_stock.server_lots(server_raw.values())
    raw, trade, moved = exchange_stock.relocate(raw, trade, resolver, places,
                                                server, items)
    local, skipped = exchange_stock.local_lots(raw, trade, resolver, places,
                                               crafting.is_piece, items)
    decisions = dict(sub.get('decisions') or {})
    result = exchange_stock.plan(local, server, sub.get('baseline'), decisions,
                                 open_conflicts=set(sub.get('conflicts') or ()))
    result, guarded = exchange_stock.keep_unmapped(result, local, server,
                                                   skipped)
    overrides = {k for k, v in decisions.items() if v == 'mine'}
    failed, applied, extra = _send_unless_held(
        conn, ROUTE_STOCK,
        exchange_stock.change_sets(result, local, server, overrides=overrides),
        state)
    rejected = {key: reason or kind for key, kind, reason in failed}

    if applied:
        sub['server'] = server_raw
        server_raw, _resync = _pull_generic(conn, sub, '/me/stock', 'key')
        server = exchange_stock.server_lots(server_raw.values())

    if result['take'] or moved:
        _apply_stock(result['take'], server, resolver, places, items)

    conflicts = dict(result['conflicts'])
    for key, reason in rejected.items():
        if reason == 'REMOVED_ELSEWHERE':
            here = local[key]['amount'] if key in local else 0
            there = server[key]['amount'] if key in server else 0
            conflicts[key] = (here, there)
    # Stand: was jetzt drüben steht. ⚠ Offene Konflikte bleiben mit ihrem
    # ALTEN Stand stehen — sonst hielte der nächste Durchgang die Menge hier
    # für eine eigene Änderung und schickte sie still hinaus.
    old = sub.get('baseline') or {}
    baseline = {key: lot['amount'] for key, lot in server.items()}
    for key in conflicts:
        if key in old:
            baseline[key] = old[key]
        else:
            baseline.pop(key, None)
    names_of = {}
    for key in conflicts:
        lot = local.get(key) or server.get(key) or {}
        names_of[key] = '%s @ %s (Q%s)' % (
            (lot.get('material') or {}).get('name') or '?',
            (lot.get('location') or {}).get('name') or '?', lot.get('quality', 0))
    sub.update({'server': server_raw, 'baseline': baseline,
                'conflicts': {k: list(v) for k, v in conflicts.items()},
                'conflict_names': names_of,
                # Bei einer Sperre bleibt `decisions` für alles stehen, was
                # nicht hinausging.
                'decisions': {k: v for k, v in decisions.items()
                              if k not in set(extra.get('posted') or ())}
                if extra.get('paused') else {},
                'rejected': rejected,
                'skipped': {k: sorted(set(v))[:20] for k, v in skipped.items()}})
    # Persönlich/gemeinsam steht nur drüben (`split`). Ändert sich das —
    # oder kommt ein Posten dazu oder fällt weg —, zeichnet Mein Lager seine
    # Liste neu, sobald der Stand gespeichert ist (`_run`). Kein Neubau der
    # Seite: Eingaben, Filter und offene Formulare bleiben stehen.
    sub['split_changed'] = _splits(server_raw) != _splits(before)
    return {'sent': applied, 'taken': len(result['take']), 'moved': moved,
            'conflicts': len(conflicts), 'rejected': len(rejected),
            'skipped_location': len(set(skipped['location'])),
            'skipped_material': len(set(skipped['material'])),
            'guarded': len(guarded), 'paused': extra.get('paused', 0),
            'offers': extra.get('offersReduced', 0)
            + extra.get('offersRemoved', 0)}


def _splits(server_raw):
    return {key: (lot or {}).get('split')
            for key, lot in (server_raw or {}).items()}


# Wer bei geänderter Aufteilung neu zeichnen will (Mein Lager): Aufrufe im
# Tk-Faden; wer `False` zurückgibt, ist weg und fliegt aus der Liste.
SPLIT_LISTENERS = []


def _split_changed():
    for listener in list(SPLIT_LISTENERS):
        try:
            if listener() is False:
                SPLIT_LISTENERS.remove(listener)
        except Exception as exc:
            from . import errors
            errors.record('basetool_sync.aufteilung', exc)


def _apply_stock(takes, server, resolver, places, items=None):
    """Übernehmen, was sich im Basetool geändert hat, und falsch einsortierte
    Zeilen umziehen (`exchange_stock.relocate`) — im Tk-Faden.

    Neues kommt nach seiner Art: ein Material ins Rohstofflager mit seiner
    Qualität, ein Item ins Handelslager.

    ⚠ Die Zeilen werden dort NEU gesucht, nicht über die Listenposition von
    vorhin: Zwischen Holen und Schreiben kann der Spieler etwas geändert
    haben."""
    from . import crafting, exchange_stock, materials, trade_cargo

    def work():
        # Noch einmal direkt vor dem Schreiben — siehe `_store_ok`.
        if not (_store_ok(materials.FILE, 'posten')
                and _store_ok(trade_cargo.FILE, 'posten')):
            return
        raw, trade, _moved = exchange_stock.relocate(
            materials.load(), trade_cargo.load(), resolver, places, server,
            items)
        local, _skipped = exchange_stock.local_lots(raw, trade, resolver,
                                                    places, crafting.is_piece,
                                                    items)
        drop_raw, drop_trade = set(), set()
        for key, amount in takes:
            lot = local.get(key)
            if lot is not None:
                store = raw if lot['store'] == 'raw' else trade
                drop = drop_raw if lot['store'] == 'raw' else drop_trade
                first, rest = lot['rows'][0], lot['rows'][1:]
                if amount > 0:
                    store[first]['menge'] = amount
                else:
                    drop.add(first)
                drop.update(rest)
                continue
            source = server.get(key)
            if source is None or amount <= 0:
                continue
            name = (source['material'] or {}).get('name') or ''
            place = (source['location'] or {}).get('name') or ''
            if source.get('item'):
                trade.append({'ware': name, 'menge': amount, 'ort': place,
                              'gestohlen': bool(source.get('stolen')),
                              'stueck': True})
            else:
                row = {'material': name, 'menge': amount,
                       'qualitaet': source.get('quality') or 0, 'ort': place}
                if source.get('stolen'):
                    row['gestohlen'] = True
                raw.append(row)
        _applying[0] = True
        try:
            materials.save([r for i, r in enumerate(raw)
                            if i not in drop_raw])
            trade_cargo.save([r for i, r in enumerate(trade)
                              if i not in drop_trade])
        finally:
            _applying[0] = False
        _pages_changed(['meinlager'])

    _in_tk(work)


def _new_ship_unit(units_ok):
    """Die Einheit für neue Schiffe aus der Einstellung — `None` = wie bisher.

    Nur mit geholter Einheiten-Liste, und nur eine Einheit, in der das
    Mitglied noch ist: Eine verlassene ergäbe bei jedem neuen Schiff
    `ORG_UNIT_NOT_ALLOWED`."""
    if not units_ok:
        return None
    from . import exchange_orgunits
    wanted = (paths.setting(SETTING_NEW_SHIP_UNIT) or '').strip()
    if wanted and wanted in {u['id'] for u in exchange_orgunits.units()}:
        return wanted
    return None


def _known_units():
    from . import exchange_orgunits
    return {u['id'] for u in exchange_orgunits.units()}


# Ablehnungen, bei denen das Basetool bei der Einheit gewinnt: VerseKit
# übernimmt dann dessen Einheit, statt bei jedem Abgleich neu zu senden.
UNIT_REJECTIONS = ('ORG_UNIT_NOT_ALLOWED', 'ORG_UNIT_VERSION_CONFLICT')


def _sync_ships(conn, state, units_ok=False):
    """Hangar <-> Schiffe beim Basetool."""
    from . import exchange_ships, fleet
    if not _store_ok(fleet.FILE, 'schiffe'):
        from . import errors
        errors.record('basetool_sync.hangar', RuntimeError(
            'Hangardatei unlesbar — Hangar wird nicht abgeglichen'))
        return {'unreadable': 1}
    sub = state.setdefault('ships', {})
    server, _resync = _pull_generic(conn, sub, '/me/ships', 'shipId')
    data = fleet.load()
    local = exchange_ships.local_ships(data, _own_ship_names(data))
    cache = sub.setdefault('resolved', {})
    resolved = _resolve_refs(conn, 'SHIP_TYPE',
                             {ext: ship['ref'] for ext, ship in local.items()},
                             cache)
    result = exchange_ships.plan(local, resolved, server, sub)
    failed, applied, extra = _send_unless_held(
        conn, ROUTE_SHIPS,
        exchange_ships.change_sets(
            result, local, resolved, server,
            new_unit=_new_ship_unit(units_ok),
            known_units=_known_units() if units_ok else None),
        state)
    rejected = {ext: reason or kind for ext, kind, reason in failed}
    # Bei einer Sperre zählt nur, was in einer angenommenen Sendung stand.
    if extra.get('paused'):
        posted = set(extra.get('posted') or ())

        def sent(entry):
            return (entry[0] if isinstance(entry, (tuple, list))
                    else entry) in posted
        result = dict(result, **{
            name: [e for e in result.get(name) or () if sent(e)]
            for name in ('link', 'create', 'update', 'remove', 'override')})
    # Beim Verknüpfen geholte Versicherung (`link_pull`) nur, wenn das `link`
    # angenommen wurde.
    linked = {ext for ext, _sid in result['link']} - set(rejected)
    result['pull'] = [e for e in result['pull']
                      if e[0] not in result.get('link_pull', ())
                      or e[0] in linked]
    # Einheit abgelehnt: die des Basetools übernehmen (siehe UNIT_REJECTIONS).
    unit_reset = sorted(ext for ext, reason in rejected.items()
                        if reason in UNIT_REJECTIONS
                        and ext in result['links'])
    for ext in unit_reset:
        if ext not in dict(result['pull']):
            result['pull'].append((ext, result['links'][ext]))
        fields = set(result['pull_fields'].get(ext) or ())
        fields.add('orgUnit')
        result['pull_fields'][ext] = sorted(fields)
    elsewhere = set(sub.get('removed_elsewhere') or ()) \
        | set(result['removed_elsewhere']) \
        | {ext for ext, reason in rejected.items()
           if reason == 'REMOVED_ELSEWHERE'}

    if applied:
        sub['server'] = server
        server, _resync = _pull_generic(conn, sub, '/me/ships', 'shipId')

    # Beim Anlegen weggelassene Einheit (unbekannt oder Liste nicht geholt):
    # im Hangar auf *wie im Basetool* zurück — sonst ginge sie einen Abgleich
    # später doch als Änderung hinaus.
    known = _known_units() if units_ok else None
    result['unit_clear'] = sorted(
        ext for ext in result['create']
        if local.get(ext, {}).get('unit') is not None
        and not (known is not None
                 and (local[ext]['unit'] == '' or local[ext]['unit'] in known)))
    if result['take'] or result['drop'] or result['pull'] \
            or result['unit_clear']:
        _apply_ships(result, server, local)

    links = {}
    for ship_id, ship in server.items():
        if ship.get('externalId'):
            links[ship['externalId']] = ship_id
    for ext, ship_id in result['local_removed'].items():
        links.setdefault(ext, ship_id)
    baseline, baseline_name, baseline_fitted = {}, {}, {}
    baseline_unit = {}
    for ext, ship_id in links.items():
        ship = server.get(ship_id)
        if ship is not None:
            baseline[ext] = ship.get('insurance')
            baseline_name[ext] = ship.get('name') or ''
            baseline_fitted[ext] = bool(ship.get('fitted'))
            baseline_unit[ext] = exchange_ships.server_unit(ship)
    done = {ext for ext, _sid in result['remove']}
    created = set(result['create']) - set(rejected)
    # Freigaben, die drüben schon erledigt sind (etwa eine im Browser
    # bestätigte Sendung): Entfernt ist, was nicht mehr im Stand steht;
    # zurückgeholt, was wieder verknüpft ist.
    pending = {ext for ext in set(sub.get('pending_removals') or ()) - done
               if links.get(ext) in server}
    overrides = {ext for ext in set(sub.get('overrides') or ())
                 - set(result.get('override') or ())
                 if links.get(ext) not in server}
    sub.update({'server': server, 'links': links, 'baseline': baseline,
                'baseline_name': baseline_name,
                'baseline_fitted': baseline_fitted,
                'baseline_unit': baseline_unit,
                'removed_elsewhere': sorted(elsewhere - created),
                'local_removed': {k: v for k, v in result['local_removed'].items()
                                  if v in server and k not in done},
                'pending_removals': sorted(pending),
                'overrides': sorted(overrides),
                'rejected': {k: v for k, v in rejected.items()
                             if k not in unit_reset},
                'names': {ext: local[ext]['name'] for ext in local}})
    return {'linked': len(result['link']), 'created': len(created),
            'updated': len(result['update']), 'taken': len(result['take']),
            'removed': len(done),
            # Einheit-Ablehnungen sind aufgelöst (Basetool gewinnt) und zählen
            # als übernommen, nicht als abgelehnt.
            'rejected': len(set(rejected) - set(unit_reset)),
            'unit_taken': len(set(result['unit_conflicts'])
                              - set(unit_reset)),
            'unit_refused': len(unit_reset),
            'unresolved': len([e for e in local if e not in resolved]),
            'detached': extra.get('detachedFromMissions', 0),
            'paused': extra.get('paused', 0)}


def _apply_ships(result, server, local):
    """Übernehmen im Hangar — im Tk-Faden, Einträge über ihre Kennung
    gesucht, nie über die Listenposition."""
    from . import exchange_ships, fleet

    def work():
        if not _store_ok(fleet.FILE, 'schiffe'):
            return                      # siehe `_store_ok`
        data = fleet.load()
        by_ext ={exchange_ships.external_id(e): e
                  for e in data.get('schiffe') or ()}
        for ship_id in result['take']:
            ship = server.get(ship_id) or {}
            name = ((ship.get('shipType') or {}).get('name') or '').strip()
            if not name:
                continue
            cover = ship.get('insurance') or {}
            fleet.add(data, name, origin=fleet.BASETOOL,
                      lti=cover.get('kind') == 'LTI',
                      versicherung=cover.get('months')
                      if cover.get('kind') == 'MONTHS' else None)
        names = []
        for ext, ship_id in result['pull']:
            ship = server.get(ship_id) or {}
            fields = (result.get('pull_fields') or {}).get(ext, ('insurance',))
            entry, cover = by_ext.get(ext), ship.get('insurance') or {}
            if entry is None:
                continue
            if 'insurance' in fields:
                entry['lti'] = cover.get('kind') == 'LTI'
                entry['versicherung'] = (cover.get('months')
                                         if cover.get('kind') == 'MONTHS'
                                         else None)
                # Die Versicherung des Basetools ändert kein Import mehr
                # (`fleet._take_over`).
                if exchange_ships.covered(cover):
                    entry[fleet.BASETOOL_COVER] = True
                else:
                    entry.pop(fleet.BASETOOL_COVER, None)
            if 'fitted' in fields:
                entry['einsatzbereit'] = bool(ship.get('fitted'))
            if 'orgUnit' in fields:
                entry['einheit'] = exchange_ships.server_unit(ship)
            key = (local.get(ext) or {}).get('asop_key')
            if 'name' in fields and key:
                names.append((key, ship.get('name') or ''))
        for ext in result.get('unit_clear') or ():
            entry = by_ext.get(ext)
            if entry is not None:
                entry.pop('einheit', None)
        for ext in result['drop']:
            entry = by_ext.get(ext)
            if entry is not None and entry.get('herkunft') == fleet.BASETOOL:
                data['schiffe'] = [e for e in data['schiffe'] if e is not entry]
        _applying[0] = True
        try:
            fleet.save(data)
        finally:
            _applying[0] = False
        if names:
            _take_ship_names(names)
        _pages_changed(['hangar'])

    _in_tk(work)


def _own_ship_names(fleet_data):
    """`{Hangar-Name: (Schlüssel, eigener Name)}` für alle Schiffe, die sich
    einem Spielnamen zuordnen lassen — ohne Sprachdatei leer."""
    from . import asop, errors
    try:
        keys = asop.hangar_keys(fleet_data.get('schiffe') or [])
        stand = asop.load()
    except Exception as exc:
        errors.record('basetool_sync.schiffsnamen', exc)
        return {}
    return {name: (key, asop.entry(stand, key)[0])
            for name, key in keys.items()}


def _take_ship_names(names):
    """Im Basetool geänderte Namen in `asop.json` übernehmen und in die
    Sprachdatei schreiben. Ein Stern bleibt, wie er war."""
    from . import asop, errors, injection
    stand = asop.load()
    for key, name in names:
        asop.set_name(stand, key, name, asop.entry(stand, key)[1])
    _applying[0] = True
    try:
        saved = asop.save(stand)
    finally:
        _applying[0] = False
    if not saved:
        return
    try:
        ini, folder, _source = injection.ini_file()
        if ini:
            injection.refresh(ini, folder)
    except Exception as exc:
        errors.record('basetool_sync.schiffsnamen_schreiben', exc)


def _tags():
    from . import export
    return export._unique_tags()


def _catalog_names():
    try:
        from . import catalog
        return catalog.load().get('bauplaene') or {}
    except Exception:
        return {}


# --------------------------------------------------- Entscheidungen
def _add_to(container, key, values):
    container[key] = sorted(set(container.get(key) or ()) | set(values))


def release_removals(keys):
    """Der Spieler gibt frei: diese hier entfernten Baupläne auch im Basetool
    entfernen. Erst jetzt werden sie gesendet."""
    keys = list(keys)
    return _decide(lambda state: _add_to(state, 'pending_removals', keys))


def keep_removed(keys, watcher=None):
    """Der Spieler will sie behalten: aus dem Basetool zurück in den Bestand."""
    watcher = watcher or _WATCHER[0]
    keys = list(keys)
    state = current_state()
    if state is None:
        return False
    changes = []
    for key in keys:
        bt = (state.get('local_removed') or {}).get(key)
        item = (state.get('baseline') or {}).get(bt) or {}
        name = (item.get('ref') or {}).get('name')
        if name:
            changes.append(('add', name, None))

    def change(state):
        for key in keys:
            (state.get('local_removed') or {}).pop(key, None)

    done = _decide(change)
    if changes and watcher is not None:
        watcher.basetool_apply(changes)
    return done


def override_conflicts(keys):
    """Trotz Entfernung woanders wieder hochschicken — nur auf Zustimmung."""
    keys = list(keys)
    return _decide(lambda state: _add_to(state, 'overrides', keys))


def stock_decide(keys, choice):
    """Lager-Konflikt: 'mine' (hier gilt) oder 'theirs' (Basetool gilt)."""
    keys = list(keys)

    def change(state):
        sub = state.setdefault('stock', {})
        decisions = sub.setdefault('decisions', {})
        for key in keys:
            decisions[key] = choice
        sub['conflicts'] = {k: v for k, v in (sub.get('conflicts') or {}).items()
                            if k not in keys}

    return _decide(change)


def ships_release(exts):
    """Hier fehlende Schiffe auch im Basetool entfernen — nur auf Freigabe."""
    exts = list(exts)
    return _decide(lambda state: _add_to(state.setdefault('ships', {}),
                                         'pending_removals', exts))


def ships_keep(exts):
    """Im Basetool behalten: die Verknüpfung lösen, nicht mehr fragen."""
    exts = list(exts)

    def change(state):
        sub = state.setdefault('ships', {})
        for ext in exts:
            (sub.get('local_removed') or {}).pop(ext, None)
        # ⚠ Die Verknüpfung steht auch beim Server (`externalId`) — sie käme
        # mit dem nächsten Holen zurück. Deshalb gemerkt: für diese nicht mehr
        # fragen.
        _add_to(sub, 'kept', exts)

    return _decide(change)


def ships_override(exts):
    """Woanders entfernte Schiffe trotzdem wieder anlegen — mit Zustimmung."""
    exts = list(exts)
    return _decide(lambda state: _add_to(state.setdefault('ships', {}),
                                         'overrides', exts))


def forget_installation():
    """Nach dem Trennen: der Stand dieser Installation ist wertlos."""
    installation_id = STATUS.get('installation_id')
    if installation_id:
        state_file = _state_file(installation_id)
        # Mit der Vorgängerfassung, die `paths.save_json` daneben ablegt.
        for target in (state_file, state_file[:-5] + '.bak.json'):
            try:
                os.remove(target)
            except OSError:
                pass
    _set(state='idle', code='', counts={}, installation_id=None,
         last_sync=None, confirm={}, account=None)
