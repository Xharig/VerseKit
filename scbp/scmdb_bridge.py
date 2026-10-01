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
Verse-Kit als „Log Watcher" für scmdb.net (SCMDB Log Watcher Protocol 0.1.9,
https://scmdb.net/docs/watcher-protocol.md).

Ein kleiner Webserver **nur auf dem eigenen Rechner** (`127.0.0.1:23456`). Die
scmdb-Seite im Browser des Spielers verbindet sich damit, sobald dort der
Schalter „Watcher" an ist, und übernimmt erhaltene Baupläne ins scmdb-Konto
und startet Stoppuhren für laufende Aufträge. Verse-Kit selbst schickt nichts
ins Netz.

| Adresse | Inhalt |
|---|---|
| `GET /ping` | Protokollversion, Kanal, `client`, `clientVersion` |
| `GET /events` | Server-Sent Events: erst der Verlauf der laufenden Sitzung (`replay: true`), dann der aktuelle Stand, dann neue Ereignisse |

Woher die Ereignisse kommen (alle aus der laufenden `Game.log`):

| Ereignis | Log-Zeile |
|---|---|
| `mission_start` | erste `CreateMarker`-Zeile einer `missionId` — trägt Vertrag, Generator und `contractDefinitionId` |
| `mission_complete` / `mission_ended` | `<EndMission> … CompletionType[…] Reason[…]` |
| `mission_ended` (`Disconnect`) | Spielwelt verlassen (`RequestFrontEnd`) — das Spiel meldet dabei kein Ende |
| `blueprint_received` | die Bauplan-Meldung (`phrases.pattern()`) |
| `session_reset` | die `Game.log` beginnt neu |

Regeln:

- **Ab Werk aus**, Schalter `scmdb_bruecke`.
- **Bauplannamen:** geschickt wird der **englische Katalogname**, nicht der
  Name aus dem Log — eine deutsche oder um Klasse/Größe ergänzte Bezeichnung
  kann scmdb nicht sicher zuordnen. Lässt sich ein Name keinem Katalogeintrag
  eindeutig zuordnen, wird er **nicht** geschickt und nur im Fehlerprotokoll
  vermerkt.
- **Nur die laufende Sitzung**, keine alten `logbackups/`.
- **Kanal ehrlich** auf jedem Ereignis; scmdb übernimmt nur LIVE und HOTFIX.
- **Port belegt** (z. B. vom offiziellen Watcher): nicht starten, nur melden.
- CORS nur für `https://scmdb.net` und `https://www.scmdb.net`, nie `*`.
"""
import collections
import json
import os
import queue
import re
import threading
import time
from calendar import timegm
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from . import paths

SETTING = 'scmdb_bruecke'
HOST = '127.0.0.1'
PORT = 23456
PROTOCOL = '0.1.9'
CLIENT = 'versekit'
ORIGINS = ('https://scmdb.net', 'https://www.scmdb.net')
HISTORY = 500
HEARTBEAT = 15.0
POLL = 1.0
# Ein Bauplan wird dem Auftrag zugeordnet, der so kurz davor angenommen oder
# abgeschlossen wurde.
MISSION_WINDOW = 120.0

TIMESTAMP = re.compile(r'^<(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})(\.\d+)?Z>')
MARKER = re.compile(
    r'CreateMarker>[^\n]*?missionId \[([0-9a-fA-F-]+)\][^\n]*?'
    r'generator name \[([^\]]*)\][^\n]*?contract \[([^\]]*)\][^\n]*?'
    r'contractDefinitionId\[([0-9a-fA-F-]+)\]')
END = re.compile(
    r'<EndMission>[^\n]*?MissionId\[([0-9a-fA-F-]+)\][^\n]*?'
    r'CompletionType\[([^\]]*)\](?:[^\n]*?Reason\[([^\]]*)\])?')
LEFT_GAME = re.compile(r'CSessionManager::RequestFrontEnd\]\s*Started')


def enabled():
    return paths.setting_bool(SETTING, False)


def port():
    try:
        return int(os.environ.get('SC_BP_SCMDB_PORT') or PORT)
    except ValueError:
        return PORT


def channel_of(folder):
    """LIVE, HOTFIX, PTU, EPTU, TECH-PREVIEW — oder UNKNOWN."""
    name = os.path.basename(os.path.normpath(folder or '')).upper()
    return name if name in paths.CHANNELS else 'UNKNOWN'


def line_time(line):
    """Zeitpunkt einer Log-Zeile in Unix-Sekunden — oder jetzt."""
    m = TIMESTAMP.match(line)
    if not m:
        return round(time.time(), 3)
    stamp = timegm(time.strptime(m.group(1), '%Y-%m-%dT%H:%M:%S'))
    return round(stamp + float(m.group(2) or 0), 3)


def _version():
    from . import errors
    return (errors.VERSION[0] or '0')[:16]


# ------------------------------------------------------------- Auswertung
class Reader(object):
    """Macht aus Log-Zeilen Protokoll-Ereignisse. Ohne Netz, ohne Faden."""

    def __init__(self, channel, catalog_names=None, pattern=None):
        self.channel = channel
        self.active = collections.OrderedDict()   # guid -> Auftrag
        self.ended = set()
        self.last_mission = None                  # (guid, info, Auslöser, Zeit)
        self._catalog = catalog_names
        self._pattern = pattern
        self.unmatched = []

    # Nachgeladen, damit ein Prüflauf beides selbst setzen kann.
    def _known(self):
        if self._catalog is None:
            try:
                from . import catalog
                self._catalog = catalog.load().get('bauplaene') or {}
            except Exception:
                self._catalog = {}
        return self._catalog

    def _blueprint_pattern(self):
        if self._pattern is None:
            from . import phrases
            self._pattern = phrases.pattern()
        return self._pattern

    def english_name(self, name):
        """Der Katalogname (englisch) — oder None, wenn nicht eindeutig."""
        from . import collection
        known = self._known()
        if not known or not name:
            return None
        found = collection.catalog_name(name, known)
        entry = known.get(collection.norm(found or ''))
        if not entry:
            return None
        return entry.get('n') or found

    def _event(self, kind, **fields):
        event = {'type': kind, 'channel': self.channel}
        event.update(fields)
        return event

    def _mission_fields(self, guid, info):
        info = info or {}
        return {'guid': guid, 'debugName': info.get('debugName'),
                'generator': info.get('generator'),
                'contractDefinitionId': info.get('contractDefinitionId')}

    def feed(self, line):
        """Eine Log-Zeile -> Liste von Ereignissen (meist leer)."""
        out = []
        if 'CreateMarker>' in line:
            m = MARKER.search(line)
            if m and m.group(1) not in self.active and m.group(1) not in self.ended:
                guid = m.group(1)
                info = {'debugName': m.group(3) or None,
                        'generator': m.group(2) or None,
                        'contractDefinitionId': m.group(4) or None,
                        'startTs': line_time(line)}
                self.active[guid] = info
                self.last_mission = (guid, info, 'accept', info['startTs'])
                fields = self._mission_fields(guid, info)
                fields['startTs'] = info['startTs']
                out.append(self._event('mission_start', **fields))
        elif '<EndMission>' in line:
            m = END.search(line)
            if m:
                guid, completion = m.group(1), m.group(2) or 'Unknown'
                info = self.active.pop(guid, None)
                self.ended.add(guid)
                kind = 'mission_complete' if completion == 'Complete' else 'mission_ended'
                fields = self._mission_fields(guid, info)
                fields.update(completion=completion, reason=m.group(3) or '',
                              endTs=line_time(line))
                if completion == 'Complete':
                    self.last_mission = (guid, info or {}, 'complete', fields['endTs'])
                out.append(self._event(kind, **fields))
        elif LEFT_GAME.search(line):
            when = line_time(line)
            for guid, info in list(self.active.items()):
                fields = self._mission_fields(guid, info)
                fields.update(completion='Disconnect', reason='Left game',
                              endTs=when)
                out.append(self._event('mission_ended', **fields))
                self.ended.add(guid)
            self.active.clear()
        elif 'Added notification' in line:
            from . import logsource
            for name, _extra in logsource._names_from_text(
                    line, self._blueprint_pattern()):
                english = self.english_name(name)
                if not english:
                    self.unmatched.append(name)
                    continue
                when = line_time(line)
                fields = {'productName': english, 'missionGuid': None,
                          'missionDebugName': None,
                          'missionContractDefinitionId': None,
                          'missionTrigger': None, 'ts': when}
                if self.last_mission and when - self.last_mission[3] <= MISSION_WINDOW:
                    guid, info, trigger, _at = self.last_mission
                    fields.update(missionGuid=guid,
                                  missionDebugName=info.get('debugName'),
                                  missionContractDefinitionId=info.get(
                                      'contractDefinitionId'),
                                  missionTrigger=trigger)
                out.append(self._event('blueprint_received', **fields))
        return out

    def snapshot(self):
        active = []
        for guid, info in self.active.items():
            fields = self._mission_fields(guid, info)
            fields['startTs'] = info.get('startTs')
            active.append(fields)
        return self._event('state_snapshot', active=active)


# ------------------------------------------------------------------ Server
STATUS = {'state': 'off', 'clients': 0, 'channel': 'UNKNOWN', 'sent': 0,
          'unmatched': 0}
LISTENERS = []


def _set(**values):
    STATUS.update(values)
    for listener in list(LISTENERS):
        try:
            listener()
        except Exception:
            pass


class Bridge(object):
    """Server und Mitlesen. `start()` / `stop()` — beides darf mehrfach kommen."""

    def __init__(self):
        self._lock = threading.Lock()
        self.history = collections.deque(maxlen=HISTORY)
        self.subscribers = []
        self.reader = Reader('UNKNOWN')
        self.server = None
        self._stop = threading.Event()
        self._threads = []

    # -------------------------------------------------- Ereignisse verteilen
    def publish(self, events, live=True):
        with self._lock:
            listeners = list(self.subscribers) if live else []
            for event in events:
                self.history.append(event)
                for sub in listeners:
                    sub.put(event)
        if events and listeners:
            _set(sent=STATUS['sent'] + len(events))

    def reset(self, channel):
        """Neue Spielsitzung: erst den Verlauf leeren, dann melden."""
        with self._lock:
            self.history.clear()
            self.reader = Reader(channel)
            subs = list(self.subscribers)
        reset = {'type': 'session_reset', 'channel': channel}
        for sub in subs:
            sub.put(reset)

    def subscribe(self):
        """Neuer Zuhörer -> (Schlange, Verlauf als Replay, aktueller Stand)."""
        sub = queue.Queue()
        with self._lock:
            replay = [dict(e, replay=True) for e in self.history]
            snapshot = self.reader.snapshot()
            self.subscribers.append(sub)
            count = len(self.subscribers)
        _set(clients=count, state='connected')
        return sub, replay, snapshot

    def unsubscribe(self, sub):
        with self._lock:
            if sub in self.subscribers:
                self.subscribers.remove(sub)
            count = len(self.subscribers)
        _set(clients=count, state='connected' if count else 'waiting')

    # ---------------------------------------------------------- Mitlesen
    def _tail(self):
        path, position, head, rest = None, 0, b'', b''
        while not self._stop.is_set():
            try:
                folder = paths.game_folder()
                current = paths.game_log(folder)
                channel = channel_of(folder)
                if current:
                    size = os.path.getsize(current)
                    with open(current, 'rb') as handle:
                        first = handle.read(256)
                        rotated = (current != path or size < position
                                   or first[:len(head)] != head)
                        if rotated:
                            if path is not None:
                                self.reset(channel)
                            else:
                                self.reader = Reader(channel)
                            path, position, head, rest = current, 0, first, b''
                            _set(channel=channel)
                        handle.seek(position)
                        chunk = handle.read()
                    position += len(chunk)
                    data = rest + chunk
                    cut = data.rfind(b'\n')
                    if cut >= 0:
                        rest = data[cut + 1:]
                        text = data[:cut].decode('utf-8', 'replace')
                        events = []
                        for line in text.split('\n'):
                            events += self.reader.feed(line)
                        self.publish(events)
                        if self.reader.unmatched:
                            from . import errors
                            for name in self.reader.unmatched:
                                errors.trail('scmdb: Bauplan nicht zugeordnet, '
                                             'nicht geschickt: %s' % name)
                            _set(unmatched=STATUS['unmatched']
                                 + len(self.reader.unmatched))
                            self.reader.unmatched = []
                    else:
                        rest = data
            except Exception as exc:
                from . import errors
                errors.record('scmdb_bridge.tail', exc)
            self._stop.wait(POLL)

    # ------------------------------------------------------------ Server
    def start(self):
        if self.server is not None:
            return True
        try:
            server = ThreadingHTTPServer((HOST, port()), _handler(self))
        except OSError:
            _set(state='busy', clients=0)
            return False
        server.daemon_threads = True
        self.server = server
        self._stop.clear()
        for target in (server.serve_forever, self._tail):
            thread = threading.Thread(target=target, daemon=True)
            thread.start()
            self._threads.append(thread)
        _set(state='waiting', clients=0)
        return True

    def stop(self):
        self._stop.set()
        server, self.server = self.server, None
        if server is not None:
            server.shutdown()
            server.server_close()
        with self._lock:
            subs, self.subscribers = list(self.subscribers), []
        for sub in subs:
            sub.put(None)
        self._threads = []
        _set(state='off', clients=0)


_Base = BaseHTTPRequestHandler


def _handler(bridge):
    class Handler(_Base):
        server_version = 'VerseKit'
        protocol_version = 'HTTP/1.1'
        # Setzt `StreamRequestHandler.setup()` je Verbindung; hier nur
        # angekündigt. Geerbte Methoden laufen ausdrücklich über `_Base`.
        wfile = None

        def log_message(self, *_args):
            pass

        def _head(self, status, headers):
            _Base.send_response(self, status)
            origin = self.headers.get('Origin')
            if origin in ORIGINS:
                headers = [('Access-Control-Allow-Origin', origin),
                           ('Vary', 'Origin'),
                           # Chrome fragt bei einer öffentlichen Seite, die den
                           # eigenen Rechner anspricht, vorher eigens nach
                           # (Private Network Access).
                           ('Access-Control-Allow-Private-Network', 'true')
                           ] + list(headers)
            for name, value in headers:
                _Base.send_header(self, name, value)
            _Base.end_headers(self)

        def _out(self, data):
            self.wfile.write(data)
            self.wfile.flush()

        def do_OPTIONS(self):
            self._head(204, [('Access-Control-Allow-Methods', 'GET, OPTIONS'),
                             ('Access-Control-Allow-Headers', '*'),
                             ('Content-Length', '0')])

        def do_GET(self):
            route = self.path.split('?', 1)[0]
            if route == '/ping':
                body = json.dumps({
                    'status': 'ok', 'version': PROTOCOL,
                    'channel': bridge.reader.channel, 'client': CLIENT,
                    'clientVersion': _version()}).encode('utf-8')
                self._head(200, [('Content-Type', 'application/json'),
                                 ('Cache-Control', 'no-cache'),
                                 ('Content-Length', str(len(body)))])
                self._out(body)
                return
            if route == '/events':
                self._events()
                return
            self._head(404, [('Content-Length', '0')])

        def _send(self, event):
            line = 'data: %s\n\n' % json.dumps(event, ensure_ascii=False)
            self._out(line.encode('utf-8'))

        def _events(self):
            self.close_connection = True
            self._head(200, [('Content-Type', 'text/event-stream'),
                             ('Cache-Control', 'no-cache'),
                             ('Connection', 'close')])
            sub, replay, snapshot = bridge.subscribe()
            try:
                for event in replay:
                    self._send(event)
                self._send(snapshot)
                while True:
                    try:
                        event = sub.get(timeout=HEARTBEAT)
                    except queue.Empty:
                        self._out(b': heartbeat\n\n')
                        continue
                    if event is None:
                        return
                    self._send(event)
            except (BrokenPipeError, ConnectionResetError, OSError):
                pass
            finally:
                bridge.unsubscribe(sub)

    return Handler


BRIDGE = Bridge()


def apply_setting():
    """Schalter umgesetzt: an -> starten, aus -> anhalten."""
    if enabled():
        BRIDGE.start()
    else:
        BRIDGE.stop()
