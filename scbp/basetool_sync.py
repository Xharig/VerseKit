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
gelucs Sync-Anleitung (`docs/exchange/sync-guide.md`) und werden bei der
Freigabe gegen genau diese Liste geprüft:

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
import json
import os
import re
import threading
import time

from . import basetool, collection as collection_file, exchange_sync, paths

SETTING_BLUEPRINTS = 'basetool_bauplaene'
SETTING_STOCK = 'basetool_lager'
SETTING_SHIPS = 'basetool_hangar'
SETTING_LABEL = 'basetool_name'

# Die Bereiche: Schalter und die Rechte, die er braucht. Jeder ist ab Werk
# aus, und angefragt wird nur, was eingeschaltet ist.
AREAS = ((SETTING_BLUEPRINTS, basetool.SCOPES_BLUEPRINTS),
         (SETTING_STOCK, basetool.SCOPES_STOCK),
         (SETTING_SHIPS, basetool.SCOPES_HANGAR))

CADENCE = 300           # höchstens alle 5 Minuten ohne Anlass
AFTER_CHANGE = 20       # eigene Änderungen kurz sammeln, dann einmal abgleichen
PAGE_LIMIT = 500
RESOLVE_MAX = 500
# Nicht Zuordenbares wird höchstens einmal am Tag neu angefragt.
RESOLVE_AGAIN = 24 * 3600
# Die Bestätigungsadresse einer Massenänderung gilt 30 Minuten und ist ein
# Geheimnis — sie steht nur im Speicher, nie in einer Datei.
CONFIRM_LIFETIME = 30 * 60

_lock = threading.Lock()
STATUS = {'state': 'idle', 'code': '', 'running': False, 'last_sync': None,
          'counts': {}, 'confirm_url': None, 'confirm_until': 0.0,
          'account': None}
_schedule = {'next': 0.0, 'attempt': 0, 'changed_at': None}
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
    return out


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


def save_state(state):
    paths.save_json(_state_file(state['installation_id']), state)


def current_state():
    """Der Stand der Installation, mit der zuletzt abgeglichen wurde."""
    installation_id = STATUS.get('installation_id')
    return load_state(installation_id) if installation_id else None


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
    """Der eigene Bestand hat sich geändert — bald abgleichen (gesammelt)."""
    if enabled():
        with _lock:
            _schedule['changed_at'] = time.time()


def request_now():
    """Knopf „Jetzt abgleichen" — ohne Warten, aber nie parallel."""
    with _lock:
        _schedule['next'] = 0.0
        _schedule['attempt'] = 0


def due(now=None):
    now = now or time.time()
    with _lock:
        if STATUS['running']:
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
    _set(running=True)
    threading.Thread(target=_run_guarded, args=(watcher,), daemon=True).start()


def _run_guarded(watcher):
    try:
        run(watcher)
        with _lock:
            _schedule['attempt'] = 0
            _schedule['next'] = time.time() + CADENCE
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
    # ⚠ Ins Fehlerprotokoll — sonst stand auf der Seite nur „abgelehnt
    # (HTTP_503)", und niemand konnte sagen, welche Anfrage es war
    # (erster Test, 28.09.2026).
    from . import errors
    errors.record('basetool.sync', RuntimeError(error.describe()))
    action = error.action
    with _lock:
        if action == 'retry':
            _schedule['attempt'] += 1
            wait = basetool.backoff(_schedule['attempt'], error.retry_after)
        else:
            # Halt: kein neuer Versuch im Takt. Erst beim nächsten Start oder
            # auf Knopfdruck — so will es die Anleitung etwa bei einer
            # Sperre des Programms.
            _schedule['attempt'] = 0
            wait = 24 * 3600
        _schedule['next'] = time.time() + wait
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
        return None                     # noch kein Account in den Protokollen
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

    ⚠⚠ **Nie zwei gleichzeitig** (gefunden am 28.09.2026 im Selbsttest): Liefen
    der Takt und ein zweiter Anstoß zugleich, las der eine den Feed, während
    der andere schon sendete — die Löschmarke kam beim zweiten nicht mehr an,
    und ein im Web gelöschter Bauplan ging wieder hinaus. Wer die Sperre nicht
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
    installation_id = doc.get('installationId') or 'ohne-kennung'
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
        save_state(state)
    if SETTING_SHIPS in ready:
        counts['ships'] = _sync_ships(conn, state)
        save_state(state)
    _set(state='ok', code='SCOPE_MISSING' if missing else '', counts=counts,
         missing=missing, last_sync=time.strftime('%Y-%m-%d %H:%M:%S'))


def _sync_blueprints(conn, state, watcher):
    """Baupläne — der Durchgang, wie er seit v3.60.0 läuft."""
    # 1) Holen
    server, stones, cursor, resync = _pull(conn, state)

    # 2) Vergleichen
    stock = collection_file.load()
    local = exchange_sync.local_blueprints(stock, _tags())
    resolved = _resolve(conn, local, state)
    plan = exchange_sync.plan_sync(local, resolved, server, stones, state,
                                   resync=resync)

    # 3) Senden
    sent_removals, counts = [], {'added': 0, 'removed': 0, 'unchanged': 0,
                                  'unmatched': 0, 'ambiguous': 0}
    new_conflicts = dict(plan['conflicts'])
    for change_set, targets in exchange_sync.sync_change_sets(plan, local):
        try:
            result = conn.request('POST', '/me/blueprints/changes', change_set,
                                  idempotency_key=basetool.new_idempotency_key())
        except basetool.ApiError as error:
            if error.code == 'MASS_CHANGE_CONFIRMATION_REQUIRED':
                _set(confirm_url=(error.problem or {}).get('confirmationUrl'),
                     confirm_until=time.time() + CONFIRM_LIFETIME)
                break
            raise
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

    done = {k for k, _bt in plan['remove']}
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
        'pending_removals': sorted(set(state.get('pending_removals') or ())
                                   - done),
        'overrides': sorted(set(state.get('overrides') or ())
                            - set(plan['override'])),
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

    ⚠⚠ Gefunden beim Bau (28.09.2026, Prüfung 294): `fleet.load()` und
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


def _send(conn, route, change_sets):
    """Sendungen abschicken. Rückgabe: ([(Kennung, Ergebnis, Grund)] der
    NICHT angewandten Anweisungen, Anzahl angewandt, Zusatzzähler)."""
    failed, applied, extra = [], 0, {}
    for change_set, keys in change_sets:
        try:
            result = conn.request('POST', route, change_set,
                                  idempotency_key=basetool.new_idempotency_key())
        except basetool.ApiError as error:
            if error.code == 'MASS_CHANGE_CONFIRMATION_REQUIRED':
                _set(confirm_url=(error.problem or {}).get('confirmationUrl'),
                     confirm_until=time.time() + CONFIRM_LIFETIME)
                break
            raise
        applied += int(result.get('applied') or 0)
        for name in ('offersReduced', 'offersRemoved', 'detachedFromMissions'):
            extra[name] = extra.get(name, 0) + int(result.get(name) or 0)
        for entry in result.get('results') or ():
            index = entry.get('index')
            if isinstance(index, int) and 0 <= index < len(keys) \
                    and entry.get('result') != 'unchanged':
                failed.append((keys[index], entry.get('result'),
                               entry.get('reason') or ''))
    return failed, applied, extra


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
    server_raw, _resync = _pull_generic(conn, sub, '/me/stock', 'key')
    places = _locations(conn)
    raw, trade = materials.load(), trade_cargo.load()
    names = {(r.get('material') or '').strip() for r in raw} \
        | {(r.get('ware') or '').strip() for r in trade}
    cache = sub.setdefault('resolved', {})
    resolved = _resolve_refs(conn, 'MATERIAL',
                             {n.lower(): {'name': n[:200]} for n in names if n},
                             cache)

    def resolver(name):
        return resolved.get((name or '').strip().lower())

    local, skipped = exchange_stock.local_lots(raw, trade, resolver, places,
                                               crafting.is_piece)
    server = exchange_stock.server_lots(server_raw.values())
    decisions = dict(sub.get('decisions') or {})
    result = exchange_stock.plan(local, server, sub.get('baseline'), decisions,
                                 open_conflicts=set(sub.get('conflicts') or ()))
    overrides = {k for k, v in decisions.items() if v == 'mine'}
    failed, applied, extra = _send(
        conn, '/me/stock/changes',
        exchange_stock.change_sets(result, local, server, overrides=overrides))
    rejected = {key: reason or kind for key, kind, reason in failed}

    if applied:
        sub['server'] = server_raw
        server_raw, _resync = _pull_generic(conn, sub, '/me/stock', 'key')
        server = exchange_stock.server_lots(server_raw.values())

    if result['take']:
        _apply_stock(result['take'], server, resolver, places)

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
                'decisions': {}, 'rejected': rejected,
                'skipped': {k: sorted(set(v))[:20] for k, v in skipped.items()}})
    return {'sent': applied, 'taken': len(result['take']),
            'conflicts': len(conflicts), 'rejected': len(rejected),
            'skipped_location': len(set(skipped['location'])),
            'skipped_material': len(set(skipped['material'])),
            'offers': extra.get('offersReduced', 0)
            + extra.get('offersRemoved', 0)}


def _apply_stock(takes, server, resolver, places):
    """Übernehmen, was sich im Basetool geändert hat — im Tk-Faden.

    ⚠ Die Zeilen werden dort NEU gesucht, nicht über die Listenposition von
    vorhin: Zwischen Holen und Schreiben kann der Spieler etwas geändert
    haben."""
    from . import crafting, exchange_stock, materials, trade_cargo

    def work():
        # Noch einmal direkt vor dem Schreiben — siehe `_store_ok`.
        if not (_store_ok(materials.FILE, 'posten')
                and _store_ok(trade_cargo.FILE, 'posten')):
            return
        raw, trade = materials.load(), trade_cargo.load()
        local, _skipped = exchange_stock.local_lots(raw, trade, resolver,
                                                    places, crafting.is_piece)
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
            if source.get('commodity'):
                trade.append({'ware': name, 'menge': amount, 'ort': place,
                              'gestohlen': bool(source.get('stolen'))})
            else:
                raw.append({'material': name, 'menge': amount,
                            'qualitaet': source.get('quality') or 0,
                            'ort': place})
        materials.save([r for i, r in enumerate(raw) if i not in drop_raw])
        trade_cargo.save([r for i, r in enumerate(trade)
                          if i not in drop_trade])
        _pages_changed(['lager', 'handelslager'])

    _in_tk(work)


def _sync_ships(conn, state):
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
    local = exchange_ships.local_ships(data)
    cache = sub.setdefault('resolved', {})
    resolved = _resolve_refs(conn, 'SHIP_TYPE',
                             {ext: ship['ref'] for ext, ship in local.items()},
                             cache)
    result = exchange_ships.plan(local, resolved, server, sub)
    failed, applied, extra = _send(
        conn, '/me/ships/changes',
        exchange_ships.change_sets(result, local, resolved, server))
    rejected = {ext: reason or kind for ext, kind, reason in failed}
    elsewhere = set(sub.get('removed_elsewhere') or ()) \
        | set(result['removed_elsewhere']) \
        | {ext for ext, reason in rejected.items()
           if reason == 'REMOVED_ELSEWHERE'}

    if applied:
        sub['server'] = server
        server, _resync = _pull_generic(conn, sub, '/me/ships', 'shipId')

    if result['take'] or result['drop'] or result['pull']:
        _apply_ships(result, server, local)

    links = {}
    for ship_id, ship in server.items():
        if ship.get('externalId'):
            links[ship['externalId']] = ship_id
    for ext, ship_id in result['local_removed'].items():
        links.setdefault(ext, ship_id)
    baseline = {}
    for ext, ship_id in links.items():
        ship = server.get(ship_id)
        if ship is not None:
            baseline[ext] = ship.get('insurance')
    done = {ext for ext, _sid in result['remove']}
    created = set(result['create']) - set(rejected)
    sub.update({'server': server, 'links': links, 'baseline': baseline,
                'removed_elsewhere': sorted(elsewhere - created),
                'local_removed': {k: v for k, v in result['local_removed'].items()
                                  if v in server and k not in done},
                'pending_removals': sorted(set(sub.get('pending_removals') or ())
                                           - done),
                'overrides': sorted(set(sub.get('overrides') or ())
                                    - set(result.get('override') or ())),
                'rejected': rejected,
                'names': {ext: local[ext]['name'] for ext in local}})
    return {'linked': len(result['link']), 'created': len(created),
            'updated': len(result['update']), 'taken': len(result['take']),
            'removed': len(done), 'rejected': len(rejected),
            'unresolved': len([e for e in local if e not in resolved]),
            'detached': extra.get('detachedFromMissions', 0)}


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
        for ext, ship_id in result['pull']:
            entry, cover = by_ext.get(ext), (server.get(ship_id) or {}).get(
                'insurance') or {}
            if entry is not None:
                entry['lti'] = cover.get('kind') == 'LTI'
                entry['versicherung'] = (cover.get('months')
                                         if cover.get('kind') == 'MONTHS'
                                         else None)
        for ext in result['drop']:
            entry = by_ext.get(ext)
            if entry is not None and entry.get('herkunft') == fleet.BASETOOL:
                data['schiffe'] = [e for e in data['schiffe'] if e is not entry]
        fleet.save(data)
        _pages_changed(['hangar'])

    _in_tk(work)


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
def release_removals(keys):
    """Der Spieler gibt frei: diese hier entfernten Baupläne auch im Basetool
    entfernen. Erst jetzt werden sie gesendet."""
    state = current_state()
    if state is None:
        return
    state['pending_removals'] = sorted(set(state.get('pending_removals') or ())
                                       | set(keys))
    save_state(state)
    request_now()


def keep_removed(keys, watcher=None):
    """Der Spieler will sie behalten: aus dem Basetool zurück in den Bestand."""
    watcher = watcher or _WATCHER[0]
    state = current_state()
    if state is None:
        return
    changes = []
    for key in keys:
        bt = (state.get('local_removed') or {}).pop(key, None)
        item = (state.get('baseline') or {}).get(bt) or {}
        name = (item.get('ref') or {}).get('name')
        if name:
            changes.append(('add', name, None))
    save_state(state)
    if changes and watcher is not None:
        watcher.basetool_apply(changes)


def override_conflicts(keys):
    """Trotz Entfernung woanders wieder hochschicken — nur auf Zustimmung."""
    state = current_state()
    if state is None:
        return
    state['overrides'] = sorted(set(state.get('overrides') or ()) | set(keys))
    save_state(state)
    request_now()


def stock_decide(keys, choice):
    """Lager-Konflikt: 'mine' (hier gilt) oder 'theirs' (Basetool gilt)."""
    state = current_state()
    if state is None:
        return
    sub = state.setdefault('stock', {})
    decisions = sub.setdefault('decisions', {})
    for key in keys:
        decisions[key] = choice
    sub['conflicts'] = {k: v for k, v in (sub.get('conflicts') or {}).items()
                        if k not in keys}
    save_state(state)
    request_now()


def ships_release(exts):
    """Hier fehlende Schiffe auch im Basetool entfernen — nur auf Freigabe."""
    state = current_state()
    if state is None:
        return
    sub = state.setdefault('ships', {})
    sub['pending_removals'] = sorted(set(sub.get('pending_removals') or ())
                                     | set(exts))
    save_state(state)
    request_now()


def ships_keep(exts):
    """Im Basetool behalten: die Verknüpfung lösen, nicht mehr fragen."""
    state = current_state()
    if state is None:
        return
    sub = state.setdefault('ships', {})
    for ext in exts:
        (sub.get('local_removed') or {}).pop(ext, None)
    # ⚠ Die Verknüpfung steht auch beim Server (`externalId`) — sie käme mit
    # dem nächsten Holen zurück. Deshalb gemerkt: für diese nicht mehr fragen.
    sub['kept'] = sorted(set(sub.get('kept') or ()) | set(exts))
    save_state(state)
    request_now()


def ships_override(exts):
    """Woanders entfernte Schiffe trotzdem wieder anlegen — mit Zustimmung."""
    state = current_state()
    if state is None:
        return
    sub = state.setdefault('ships', {})
    sub['overrides'] = sorted(set(sub.get('overrides') or ()) | set(exts))
    save_state(state)
    request_now()


def forget_installation():
    """Nach dem Trennen: der Stand dieser Installation ist wertlos."""
    installation_id = STATUS.get('installation_id')
    if installation_id:
        try:
            os.remove(_state_file(installation_id))
        except OSError:
            pass
    _set(state='idle', code='', counts={}, installation_id=None,
         last_sync=None, confirm_url=None, account=None)
