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
Abgleich der Baupläne mit dem KRT Profit Basetool — der Teil ohne Netz.

Das Basetool baut dafür eine eigene Schnittstelle (Exchange API v1, Epic
krt-profit/basetool#2078, VerseKit als erstes fremdes Programm). Ihr
Datenformat steht fest und liegt unter `tools/basetool-vertrag/`; Anmeldung und
Server kommen erst noch. Dieses Modul enthält deshalb nur, was ohne Verbindung
geht: VerseKits Bestand in das Format bringen, den Abgleich **planen** und die
Antwort des Servers deuten. Es schickt nichts, öffnet nichts und schreibt nichts
auf die Platte — die Netzseite kommt als eigener Baustein darüber.

**Die Regeln des Abgleichs** (ADR-0218 des Basetools und unsere Leitplanken):

1. **Aus Fehlen wird nie gelöscht.** Fehlt ein Bauplan auf einer Seite, ist das
   kein Auftrag, ihn auf der anderen zu entfernen — ein unvollständiger Stand
   (zurückgesetzter Bestand, neue Installation) würde sonst alles abräumen.
   Entfernt wird beim Basetool nur, was der Spieler **hier ausdrücklich**
   abgehakt hat (`pending_removals`).
2. **Was woanders entfernt wurde, kommt nicht still zurück.** Hat das Mitglied
   einen Bauplan im Basetool gelöscht, bleibt er hier zwar stehen (die
   Game.log belegt ihn ja), wird aber nicht wieder hochgeschickt. Er landet
   als **Konflikt** beim Spieler; nur wenn der zustimmt, geht er mit
   `override` erneut hinaus.
3. **Startbaupläne schickt VerseKit nicht.** Die vergibt das Basetool jedem
   Mitglied selbst, und entfernen lassen sie sich dort nicht.
4. **Höchstens 500 Anweisungen je Sendung** — mehr lehnt der Server ab.

**Der gemerkte Stand** (`state`) sieht so aus — gespeichert wird er erst mit
der Netzseite::

    {'cursor': None,            # Position im Änderungsfeed des Servers
     'links': {},               # Bestandsschlüssel -> Schlüssel beim Basetool
     'pending_removals': [],    # hier abgehakt, beim Basetool noch zu entfernen
     'own_removed_keys': [],    # Basetool-Schlüssel, die WIR entfernt haben
     'installation_id': None}   # unsere Kennung laut Basetool, sobald bekannt

Eine Löschmarke ist unsere eigene, wenn ihr `removedBy.installationId` unsere
`installation_id` ist — die liefert das Basetool in der Antwort auf
`POST /me/installation` und im Service-Dokument (krt-profit/basetool#2118).
Solange sie fehlt, hilft `own_removed_keys`: Was wir selbst weggeschickt
haben, wissen wir. Beide Wege gelten laut Basetool.
"""
import time
import uuid

from . import paths

BATCH_MAX = 500
FORMAT_VERSION = '1.0'
TOOL_NAME = 'VerseKit'

# Quelle im Bestand -> `provenance.source` im Vertrag.
PROVENANCE = {
    'log': 'log',
    'nachlese': 'log',
    'hand': 'manual',
    'import': 'import',
    'launcher': 'import',
    'start': 'default',
}

# Diese Quellen gehen nie hinaus — siehe Regel 3 oben.
NEVER_SENT = ('start',)


def empty_state():
    return {'cursor': None, 'links': {}, 'pending_removals': [],
            'own_removed_keys': [], 'installation_id': None}


def _own_stone(stone, own_keys, installation_id):
    """Hat VerseKit selbst — genau diese Installation — den Eintrag entfernt?"""
    if stone.get('key') in own_keys:
        return True
    removed_by = stone.get('removedBy') or {}
    return bool(installation_id) and (
        removed_by.get('installationId') == installation_id)


def iso_utc(local_time):
    """`JJJJ-MM-TT hh:mm:ss` (Ortszeit, wie im Bestand) -> RFC 3339 in UTC.

    ⚠ Der Bestand hält die **Ortszeit**. Der alte Basetool-Export hängt nur ein
    `Z` an und verschiebt damit jede Zeit um die Zeitzone; hier wird wirklich
    umgerechnet. Nicht deutbar -> None, dann bleibt das Feld weg."""
    if not local_time:
        return None
    try:
        stamp = time.mktime(time.strptime(str(local_time),
                                          '%Y-%m-%d %H:%M:%S'))
    except (ValueError, TypeError, OverflowError):
        return None
    return time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(stamp))


def _now_utc():
    return time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())


def item_ref(name, tag=None):
    """Ein Bauplan als `item-ref`: der Tag (`scRecord`), wo er eindeutig ist,
    dazu immer der Name. Der Server nimmt den Tag vor dem Namen und fällt ohne
    Treffer auf den Namen zurück."""
    ref = {}
    if tag:
        ref['scRecord'] = tag[:200]
    ref['name'] = name[:200]
    return ref


def local_blueprints(collection, tags):
    """Bestand -> {Schlüssel: {'name', 'tag', 'source', 'time'}}.

    `tags` ist die Tabelle Vergleichsname -> Tag, nur für eindeutige Namen
    (`export._unique_tags()`); ein geratener Tag landete sicher beim falschen
    Teil."""
    out = {}
    for key, entry in (collection.get('bauplaene') or {}).items():
        name = (entry.get('name') or '').strip()
        if not name:
            continue
        out[key] = {'name': name,
                    'tag': tags.get(paths.name_key(name)),
                    'source': entry.get('quelle') or 'hand',
                    'time': entry.get('zeit')}
    return out


# Quellen, die aus den Spielprotokollen stammen.
FROM_LOGS = ('log', 'nachlese')


def own_blueprints(local, foreign, links):
    """Nur die Baupläne des eigenen Accounts.

    Ein Bauplan, der ausschließlich in Protokollen eines anderen Accounts steht
    (`foreign`, Vergleichsform), geht nicht hinaus. Ist er schon verknüpft,
    bleibt er stehen — sonst sähe der Abgleich ihn als hier entfernt und
    schlüge vor, ihn im Basetool zu löschen."""
    out = {}
    for key, entry in local.items():
        if (entry['source'] in FROM_LOGS and key not in links
                and _compare(entry['name'], key) & foreign):
            continue
        out[key] = entry
    return out


def _compare(name, key):
    from . import collection
    return {collection.norm(name), collection.norm(key)}


def _add_op(entry, op_id, override=False):
    op = {'opId': op_id, 'op': 'add', 'ref': item_ref(entry['name'],
                                                      entry['tag'])}
    acquired = iso_utc(entry['time'])
    if acquired:
        op['acquiredAt'] = acquired
    provenance = {'source': PROVENANCE.get(entry['source'], 'other')}
    if acquired:
        provenance['observedAt'] = acquired
    op['provenance'] = provenance
    if override:
        op['override'] = True
    return op


def blueprint_file(collection, tags, version, when=None):
    """Der Bestand als Offline-Datei im Vertragsformat (`basetool.blueprints`).

    Für den Import im Browser, bei dem das Mitglied vorher alles prüft. ⚠ Die
    Hülle trägt laut Vertrag **keinen** Spielernamen, keinen Ordner, keinen
    Pfad — nur Programm und Version."""
    items = []
    for key, entry in sorted(local_blueprints(collection, tags).items()):
        if entry['source'] in NEVER_SENT:
            continue
        op = _add_op(entry, 'x')
        item = {'ref': op['ref'], 'provenance': op['provenance']}
        if 'acquiredAt' in op:
            item['acquiredAt'] = op['acquiredAt']
        items.append(item)
    return {'format': 'basetool.blueprints', 'formatVersion': FORMAT_VERSION,
            'generator': {'name': TOOL_NAME, 'version': version or '0'},
            'generatedAt': when or _now_utc(), 'items': items}


def _match(local, server_items, links):
    """Bestandsschlüssel -> Basetool-Schlüssel für alles, was sich findet.

    Reihenfolge: gemerkte Verknüpfung, dann Tag, dann Name. Jeder
    Basetool-Eintrag wird höchstens einmal vergeben."""
    matched = {}
    taken = set()
    by_key = {item['key']: item for item in server_items}
    for local_key, server_key in links.items():
        if local_key in local and server_key in by_key:
            matched[local_key] = server_key
            taken.add(server_key)
    by_tag, by_name = {}, {}
    for item in server_items:
        if item['key'] in taken:
            continue
        ref = item.get('ref') or {}
        if ref.get('scRecord'):
            by_tag.setdefault(ref['scRecord'].casefold(), item['key'])
        if ref.get('name'):
            by_name.setdefault(paths.name_key(ref['name']), item['key'])
    for local_key, entry in sorted(local.items()):
        if local_key in matched:
            continue
        server_key = None
        if entry['tag']:
            server_key = by_tag.get(entry['tag'].casefold())
        if server_key is None:
            server_key = by_name.get(local_key)
        if server_key is not None and server_key not in taken:
            matched[local_key] = server_key
            taken.add(server_key)
    return matched


def plan_blueprints(local, server_items, tombstones, state):
    """Was zu tun ist — ohne etwas zu tun.

    `local`: aus `local_blueprints()`. `server_items`: die Baupläne des
    Mitglieds laut Basetool (Einträge nach `blueprint.schema.json`).
    `tombstones`: Löschmarken aus dem Änderungsfeed. Rückgabe::

        {'add': [Schlüssel], 'remove': [(Schlüssel, Basetool-Schlüssel)],
         'conflicts': [(Schlüssel, Grund)], 'from_server': [Eintrag],
         'links': {Schlüssel: Basetool-Schlüssel}}

    Gründe eines Konflikts: `removed_elsewhere` (Löschmarke eines anderen
    Kanals) und `missing` (war abgeglichen, fehlt jetzt ohne Löschmarke — etwa
    nach `CURSOR_EXPIRED`, wenn der Feed die Marke schon vergessen hat)."""
    links = dict(state.get('links') or {})
    pending = set(state.get('pending_removals') or ())
    own = set(state.get('own_removed_keys') or ())
    by_key = {item['key']: item for item in server_items}
    matched = _match(local, server_items, links)
    linked_back = {server_key: local_key
                   for local_key, server_key in links.items()}

    # ⚠ Eigene Löschmarken zählen auch für die Regel zu Fehlendem ohne Marke
    # darunter — sonst ergäbe eine per Kennung erkannte eigene Entfernung
    # dort doch noch einen Konflikt.
    own |= {stone.get('key') for stone in tombstones
            if _own_stone(stone, own, state.get('installation_id'))}
    conflicts = {}
    for stone in tombstones:
        local_key = linked_back.get(stone.get('key'))
        if (local_key in local and local_key not in pending
                and not _own_stone(stone, own,
                                   state.get('installation_id'))):
            conflicts[local_key] = 'removed_elsewhere'
    for local_key, server_key in links.items():
        if (local_key in local and local_key not in pending
                and local_key not in conflicts
                and server_key not in by_key and server_key not in own):
            conflicts[local_key] = 'missing'

    add = [k for k, e in sorted(local.items())
           if k not in matched and k not in conflicts and k not in pending
           and e['source'] not in NEVER_SENT]

    remove = []
    for local_key in sorted(pending):
        server_key = matched.get(local_key) or links.get(local_key)
        item = by_key.get(server_key)
        if item is not None and not item.get('isDefault'):
            remove.append((local_key, server_key))

    taken = set(matched.values()) | {s for _k, s in remove}
    from_server = [item for key, item in sorted(by_key.items())
                   if key not in taken]

    return {'add': add, 'remove': remove,
            'conflicts': sorted(conflicts.items()),
            'from_server': from_server, 'links': matched}


def build_change_sets(plan, local, override=()):
    """Der Plan als Sendungen: [(change_set, targets)].

    `targets[i]` ist der Bestandsschlüssel zu Anweisung `i` — damit lässt sich
    die Antwort des Servers wieder zuordnen. `override`: Konflikte, bei denen
    der Spieler zugestimmt hat, sie trotzdem erneut zu schicken."""
    ops, targets = [], []
    for n, local_key in enumerate(plan['add'], 1):
        ops.append(_add_op(local[local_key], 'a%d' % n))
        targets.append(local_key)
    for n, local_key in enumerate(sorted(set(override)), 1):
        if local_key in local and local_key in dict(plan['conflicts']):
            ops.append(_add_op(local[local_key], 'o%d' % n, override=True))
            targets.append(local_key)
    for n, (local_key, server_key) in enumerate(plan['remove'], 1):
        ops.append({'opId': 'r%d' % n, 'op': 'remove', 'key': server_key})
        targets.append(local_key)
    batches = []
    for start in range(0, len(ops), BATCH_MAX):
        batches.append(({'ops': ops[start:start + BATCH_MAX]},
                        targets[start:start + BATCH_MAX]))
    return batches


def read_result(result, targets):
    """Die Antwort auf eine Sendung (`change-result`) -> was daraus folgt.

    Der Server meldet nur, was **nicht** geklappt hat, mit Index. Rückgabe::

        {'applied': n, 'conflicts': [...], 'unmatched': [...],
         'ambiguous': [...], 'rejected': [(Schlüssel, Grund)],
         'cursor': ...}"""
    out = {'applied': result.get('applied', 0), 'conflicts': [],
           'unmatched': [], 'ambiguous': [], 'rejected': [],
           'cursor': result.get('cursor')}
    for entry in result.get('results') or ():
        index = entry.get('index')
        if not isinstance(index, int) or not 0 <= index < len(targets):
            continue
        local_key = targets[index]
        kind, reason = entry.get('result'), entry.get('reason')
        if reason == 'REMOVED_ELSEWHERE':
            out['conflicts'].append(local_key)
        elif kind == 'unmatched' or reason == 'UNMATCHED':
            out['unmatched'].append(local_key)
        elif kind == 'ambiguous' or reason == 'AMBIGUOUS':
            out['ambiguous'].append(local_key)
        elif kind == 'rejected':
            out['rejected'].append((local_key, reason or ''))
    return out


def merge_pages(pages):
    """Mehrere Seiten eines Abrufs -> (Einträge, Löschmarken, nächster Cursor).

    Ein Eintrag, der auf einer späteren Seite als gelöscht steht, fällt raus —
    und umgekehrt: Taucht er nach der Löschmarke wieder auf, gilt er."""
    items, stones = {}, {}
    cursor = None
    for page in pages:
        for stone in page.get('removed') or ():
            items.pop(stone['key'], None)
            stones[stone['key']] = stone
        for item in page.get('items') or ():
            items[item['key']] = item
            stones.pop(item['key'], None)
        cursor = page.get('nextCursor') or cursor
    return list(items.values()), list(stones.values()), cursor


def plan_sync(local, resolved, server, stones, state, resync=False):
    """Ein Abgleich-Durchgang nach gelucs Sync-Anleitung — ohne Netz.

    Anders als `plan_blueprints` (Entwurf vom 26.09.) arbeitet dieser Plan
    über den **`bt`-Schlüssel des Basetools**, den `catalog/resolve` für jeden
    Bauplan liefert. Über Namen allein ließe sich nicht sicher zuordnen: Das
    Basetool nennt manche Baupläne anders, und ein falsch zugeordneter Eintrag
    tauchte doppelt auf.

    `local`: aus `local_blueprints()`. `resolved`: Bestandsschlüssel -> `bt`
    (oder None, wenn unbekannt). `server`: `bt` -> Eintrag, der Stand des
    Basetools **nach** dem Holen. `stones`: die Löschmarken seit dem letzten
    Stand. `resync`: nach `CURSOR_EXPIRED` — dann gilt, was im letzten Stand
    war und jetzt fehlt, als woanders entfernt.

    `state` trägt den **letzten abgeglichenen Stand** (`baseline`: `bt` ->
    Eintrag), die Verknüpfungen (`links`), offene Konflikte, was der Spieler
    zum Entfernen freigegeben hat (`pending_removals`), was er trotz Konflikt
    wieder hochschicken will (`overrides`) und was **wir** entfernt haben.

    Rückgabe::

        {'add': [Schlüssel], 'override': [Schlüssel],
         'remove': [(Schlüssel, bt)], 'conflicts': {Schlüssel: bt},
         'local_add': [bt], 'local_remove': [Schlüssel],
         'local_removed': {Schlüssel: bt}, 'links': {Schlüssel: bt},
         'unresolved': [Schlüssel]}
    """
    baseline = state.get('baseline') or {}
    old_links = dict(state.get('links') or {})
    pending = set(state.get('pending_removals') or ())
    overrides = set(state.get('overrides') or ())
    own = set(state.get('own_removed_keys') or ())
    installation_id = state.get('installation_id')

    links = {}
    for key in local:
        bt = resolved.get(key) or old_links.get(key)
        if bt:
            links[key] = bt

    # Was woanders entfernt wurde: Löschmarken fremder Kanäle — und nach
    # einem abgelaufenen Cursor alles, was im letzten Stand war und fehlt.
    elsewhere = {stone.get('key') for stone in stones
                 if not _own_stone(stone, own, installation_id)}
    if resync:
        elsewhere |= {bt for bt in baseline if bt not in server
                      and bt not in own}

    conflicts, local_remove = {}, []
    for key, bt in sorted(links.items()):
        if key in pending:
            continue
        was_conflict = (state.get('conflicts') or {}).get(key) == bt
        if bt in server and not was_conflict:
            continue
        if bt in elsewhere or (was_conflict and bt not in server):
            if local[key]['source'] == 'basetool':
                # Kam nur aus dem Basetool, kein Beleg im Spiel: dann gilt
                # dessen Entfernung auch hier.
                local_remove.append(key)
            else:
                conflicts[key] = bt

    # Hier entfernt, beim letzten Abgleich verknüpft, drüben noch da: das
    # entscheidet der Spieler. Nie still löschen, nie still zurückholen.
    local_removed = {}
    for key, bt in old_links.items():
        if key not in local and bt in server and key not in pending:
            local_removed[key] = bt

    add, override, unresolved = [], [], []
    for key, entry in sorted(local.items()):
        if entry['source'] in NEVER_SENT or entry['source'] == 'basetool':
            continue
        if key in pending:
            continue
        bt = links.get(key)
        if not bt:
            unresolved.append(key)
            continue
        if bt in server:
            continue
        if key in conflicts:
            if key in overrides:
                override.append(key)
            continue
        add.append(key)

    remove = []
    for key in sorted(pending):
        bt = old_links.get(key) or links.get(key)
        item = server.get(bt)
        if item is not None and not item.get('isDefault'):
            remove.append((key, bt))

    taken = set(links.values()) | set(local_removed.values())
    local_add = sorted(bt for bt in server if bt not in taken)

    return {'add': add, 'override': override, 'remove': remove,
            'conflicts': conflicts, 'local_add': local_add,
            'local_remove': sorted(local_remove),
            'local_removed': local_removed, 'links': links,
            'unresolved': unresolved}


def sync_change_sets(plan, local):
    """Der Plan als Sendungen: [(change_set, targets)], höchstens 500 je
    Sendung. Hinzufügen trägt `bt` UND den Namen — der Server nimmt `bt`."""
    ops, targets = [], []

    def add_op(key, op_id, override=False):
        op = _add_op(local[key], op_id, override)
        op['ref'] = {'bt': plan['links'][key], 'name': local[key]['name'][:200]}
        return op

    for n, key in enumerate(plan['add'], 1):
        ops.append(add_op(key, 'a%d' % n))
        targets.append(key)
    for n, key in enumerate(plan['override'], 1):
        ops.append(add_op(key, 'o%d' % n, override=True))
        targets.append(key)
    for n, (key, bt) in enumerate(plan['remove'], 1):
        ops.append({'opId': 'r%d' % n, 'op': 'remove', 'key': bt})
        targets.append(key)
    return [({'ops': ops[i:i + BATCH_MAX]}, targets[i:i + BATCH_MAX])
            for i in range(0, len(ops), BATCH_MAX)]


def new_idempotency_key():
    """Ein frischer Schlüssel je Sendung. ⚠ Beim Wiederholen DERSELBEN
    Sendung denselben Schlüssel nehmen, sonst bucht der Server doppelt."""
    return str(uuid.uuid4())


def next_state(state, plan, sent_removals, cursor):
    """Der neue gemerkte Stand nach einer vollständig angenommenen Sendung.

    `sent_removals`: die Basetool-Schlüssel, die wir entfernt haben."""
    done = {local_key for local_key, _s in plan['remove']}
    return {'cursor': cursor,
            'installation_id': state.get('installation_id'),
            'links': dict(plan['links']),
            'pending_removals': sorted(set(state.get('pending_removals') or ())
                                       - done),
            'own_removed_keys': sorted(set(state.get('own_removed_keys') or ())
                                       | set(sent_removals))}

