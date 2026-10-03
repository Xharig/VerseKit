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
Welche Seiten im Hauptfenster geöffnet werden, auf welchem Weg und wie viele
Klicks bis dorthin nötig waren — als Zähler, die mit der nächsten
Nutzungsmeldung (`usage_ping`) hinausgehen und danach geleert werden.

**Was gezählt wird** (die vier Felder der Meldung):

| Feld | Inhalt | Beispiel |
|---|---|---|
| `pages` | Aufrufe je Seite | `{"liste": 12, "laeden": 3}` |
| `entry` | auf welchem Weg eine Seite geöffnet wurde | `{"laeden": {"seitenleiste": 3}}` |
| `clicks` | Klicks bis zum Ziel, als Verteilung je Zielseite | `{"laeden": {"2": 5, "4": 1}}` |
| `misses` | Fehlgriff → nächstes Ziel | `{"verkauf>laeden": 7}` |

- **Klick:** jeder Seitenwechsel und jedes Auf- oder Zuklappen einer Gruppe
  in der Seitenleiste, ab dem Öffnen des Hauptfensters. Nach jedem
  erreichten Ziel beginnt die Zählung von vorn; was auf der Zielseite selbst
  geklappt wird, zählt schon zum nächsten Weg. Das automatische Öffnen beim
  Start (`start`) ist kein Klick.
- **Ziel:** eine Seite, auf der man mindestens `MISS_SECONDS` bleibt.
- **Fehlgriff:** eine Seite, die schneller wieder verlassen wird. Jeder
  Fehlgriff seit dem letzten Ziel wird mit dem nächsten Ziel gepaart.
- **Weg:** `ROUTES` — Seitenleiste, Sprung innerhalb des Fensters, Overlay,
  Tray-Menü oder Programmstart.

Nur Seiten-Kennungen aus `pages.page_ids()`: keine Inhalte, keine
Suchbegriffe, keine Namen, keine Zeitpunkte. Die Verweildauer lebt nur im
Speicher, solange das Fenster offen ist; auf der Platte stehen nur Zähler.

Gezählt wird nur, solange die Meldung auch hinausgehen würde
(`usage_ping.would_send()`). Ist sie abgeschaltet, mit `SC_BP_NO_NET`
unterdrückt oder läuft der Quellcode, entsteht kein Zähler, und ein
vorhandener wird verworfen.
"""
import json
import os
import re
import threading
import time

from . import paths

FILE = 'seitennutzung.json'
FIELDS = ('pages', 'entry', 'clicks', 'misses')
ROUTES = ('seitenleiste', 'sprung', 'overlay', 'tray', 'start')

MISS_SECONDS = 3.0
MAX_CLICKS = 10                 # Stufe 10 steht für 10 und mehr Klicks
MAX_COUNT = 9999                # höchster Wert je Zähler in einer Meldung
MAX_MISS_PAIRS = 20             # so viele Paare gehen höchstens hinaus
MAX_STORED_PAIRS = 200          # so viele Paare werden höchstens aufgehoben
MAX_MISSES_PER_TARGET = 5       # Fehlgriffe, die sich ein Weg höchstens merkt

PAGE_RE = re.compile(r'^[a-z_]{1,32}$')

_lock = threading.Lock()
_data = None                    # die Zähler, wie sie auf der Platte stehen
_session = None                 # der laufende Weg im offenen Hauptfenster
_known = None                   # die Seiten-Kennungen aus `pages`


def _guarded(function):
    """Für die Aufrufe aus der Oberfläche: Ein Fehler beim Zählen wird
    mitgeschrieben und darf nie einen Seitenwechsel aufhalten."""
    def run(*args, **kwargs):
        try:
            return function(*args, **kwargs)
        except Exception as exc:
            try:
                from . import errors
                errors.record('page_usage.%s' % function.__name__, exc)
            except Exception:
                pass
            return None
    run.__name__ = function.__name__
    run.__doc__ = function.__doc__
    return run


def empty():
    return {field: {} for field in FIELDS}


def active():
    """Wird überhaupt gezählt? Nur, wenn die Meldung hinausgehen würde."""
    try:
        from . import usage_ping
        return usage_ping.would_send()
    except Exception:
        return False


def _known_pages():
    global _known
    if _known is None:
        from . import pages
        _known = frozenset(pages.page_ids())
    return _known


def _count(value):
    return isinstance(value, int) and not isinstance(value, bool) and value > 0


def clean(raw):
    """Nur die bekannte Form übernehmen: Kennungen, Wege und Zahlen größer 0.
    Alles andere fällt weg."""
    out = empty()
    if not isinstance(raw, dict):
        return out
    views = raw.get('pages')
    if isinstance(views, dict):
        for page, n in views.items():
            if PAGE_RE.match(str(page)) and _count(n):
                out['pages'][page] = n
    for field, key_ok in (('entry', lambda k: k in ROUTES),
                          ('clicks', lambda k: k.isdigit()
                           and 0 <= int(k) <= MAX_CLICKS
                           and str(int(k)) == k)):
        block = raw.get(field)
        if not isinstance(block, dict):
            continue
        for page, inner in block.items():
            if not PAGE_RE.match(str(page)) or not isinstance(inner, dict):
                continue
            kept = {k: n for k, n in inner.items()
                    if isinstance(k, str) and key_ok(k) and _count(n)}
            if kept:
                out[field][page] = kept
    misses = raw.get('misses')
    if isinstance(misses, dict):
        for pair, n in misses.items():
            parts = str(pair).split('>')
            if (len(parts) == 2 and all(PAGE_RE.match(p) for p in parts)
                    and parts[0] != parts[1] and _count(n)
                    and len(out['misses']) < MAX_STORED_PAIRS):
                out['misses'][pair] = n
    return out


def _path():
    return paths.app_file(FILE)


def _load():
    global _data
    if _data is None:
        try:
            with open(_path(), encoding='utf-8') as f:
                _data = clean(json.load(f))
        except Exception:
            _data = empty()
    return _data


def _save():
    target = _path()
    temp = target + '.tmp'
    try:
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump(_data, f, ensure_ascii=False, separators=(',', ':'))
        paths.replace_file(temp, target)
    except OSError:
        try:
            os.remove(temp)
        except OSError:
            pass


def _drop():
    """Zähler verwerfen — im Speicher und auf der Platte."""
    global _data, _session
    _data = empty()
    _session = None
    try:
        os.remove(_path())
    except OSError:
        pass


def _add(block, key, n=1):
    block[key] = block.get(key, 0) + n


@_guarded
def window_opened():
    """Ein neues Hauptfenster: der Weg beginnt bei null Klicks."""
    global _session
    with _lock:
        if not active():
            _drop()
            return
        _session = {'page': None, 'since': 0.0, 'clicks': 0,
                    'clicks_at_entry': 0, 'misses': []}


@_guarded
def group_toggled():
    """Eine Gruppe der Seitenleiste wurde per Klick auf- oder zugeklappt."""
    with _lock:
        if _session is not None:
            _session['clicks'] += 1


def _finish_page(now):
    """Die bisher offene Seite abschließen: Ziel oder Fehlgriff."""
    page = _session['page']
    if page is None:
        return False
    if now - _session['since'] < MISS_SECONDS:
        if page not in _session['misses']:
            _session['misses'] = (_session['misses']
                                  + [page])[-MAX_MISSES_PER_TARGET:]
        return False
    data = _load()
    steps = min(_session['clicks_at_entry'], MAX_CLICKS)
    _add(data['clicks'].setdefault(page, {}), str(steps))
    for miss in _session['misses']:
        pair = '%s>%s' % (miss, page)
        if miss != page and (pair in data['misses']
                             or len(data['misses']) < MAX_STORED_PAIRS):
            _add(data['misses'], pair)
    _session['misses'] = []
    _session['clicks'] -= _session['clicks_at_entry']
    return True


@_guarded
def page_opened(page, route, clock=None):
    """Eine Seite steht. `route` ist einer der `ROUTES`; `None` heißt, das
    Programm hat die Seite selbst neu gezeigt (Neuaufbau) — das zählt nicht."""
    if route is None:
        return
    with _lock:
        if not active():
            _drop()
            return
        if (_session is None or route not in ROUTES
                or page not in _known_pages() or page == _session['page']):
            return
        now = time.monotonic() if clock is None else clock
        _finish_page(now)
        if route != 'start':
            _session['clicks'] += 1
        data = _load()
        _add(data['pages'], page)
        _add(data['entry'].setdefault(page, {}), route)
        _session.update(page=page, since=now,
                        clicks_at_entry=_session['clicks'])
        _save()


@_guarded
def window_closed(clock=None):
    """Das Hauptfenster geht zu: die letzte Seite abschließen. Fehlgriffe
    ohne folgendes Ziel fallen weg."""
    global _session
    with _lock:
        if _session is None:
            return
        if active() and _finish_page(time.monotonic() if clock is None
                                     else clock):
            _save()
        _session = None


def stored():
    """Eine Abschrift der aufgehobenen Zähler — das, was als Nächstes
    hinausginge, noch ungekappt."""
    with _lock:
        if not active():
            return empty()
        return json.loads(json.dumps(_load()))


def outgoing(state):
    """Die Felder für die Meldung: Paare auf die häufigsten `MAX_MISS_PAIRS`
    gekappt, jeder Zähler höchstens `MAX_COUNT`."""
    state = clean(state)

    def cap(n):
        return min(n, MAX_COUNT)

    out = {
        'pages': {p: cap(n) for p, n in sorted(state['pages'].items())},
        'entry': {p: {r: cap(n) for r, n in sorted(inner.items())}
                  for p, inner in sorted(state['entry'].items())},
        'clicks': {p: {k: cap(n) for k, n in sorted(inner.items(),
                                                     key=lambda x: int(x[0]))}
                   for p, inner in sorted(state['clicks'].items())},
    }
    top = sorted(state['misses'].items(), key=lambda x: (-x[1], x[0]))
    out['misses'] = {pair: cap(n) for pair, n in top[:MAX_MISS_PAIRS]}
    return out


def forget(state):
    """Nach dem Senden: genau das abziehen, was in `state` stand. Was
    inzwischen dazukam, bleibt für die nächste Meldung."""
    state = clean(state)
    with _lock:
        data = _load()
        for page, n in state['pages'].items():
            _take(data['pages'], page, n)
        for field in ('entry', 'clicks'):
            for page, inner in state[field].items():
                block = data[field].get(page)
                if block is None:
                    continue
                for key, n in inner.items():
                    _take(block, key, n)
                if not block:
                    del data[field][page]
        for pair, n in state['misses'].items():
            _take(data['misses'], pair, n)
        if any(data[field] for field in FIELDS):
            _save()
        else:
            try:
                os.remove(_path())
            except OSError:
                pass


def _take(block, key, n):
    left = block.get(key, 0) - n
    if left > 0:
        block[key] = left
    else:
        block.pop(key, None)


def reset_for_test():
    """Zustand im Speicher vergessen — für Prüfläufe mit eigenem Ordner."""
    global _data, _session, _known
    with _lock:
        _data = None
        _session = None
        _known = None
