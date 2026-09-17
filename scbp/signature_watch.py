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
Die Wache, die im Hintergrund die Scan-Signatur mitliest.

⚠⚠ **Warum von allein und nicht auf Knopfdruck.** Um einen Knopf zu drücken,
muss der Spieler aus Star Citizen heraus — und dann schaltet das Spiel den
Bergbau-Scanner ab. Die Zahl wäre weg, bevor sie gelesen ist (so im Entwurf
vom 10.09.2026 zweimal gebaut und zweimal umsonst).

| Regel | Warum |
|---|---|
| Nur, solange Star Citizen **vorn** ist | sonst liest sie den Desktop — eine Zahl im Browser wäre ein Treffer |
| Gemeldet erst bei **zwei gleichen** Lesungen aus den letzten drei | das HUD flackert; eine einzelne Lesung kann danebenliegen |
| Weg ist die Anzeige erst nach `CLEAR_S` ohne Zahl | beim Drehen des Schiffs verschwindet die Pille für Augenblicke |
| **Standard aus** (`SETTING`) | Bildabgriff ohne Zustimmung wäre ein Vertrauensbruch |
"""
import threading
import time

SETTING = 'signatur_wache'
INTERVAL_S = 0.4
CLEAR_S = 4.0
VOTES = 3
NEEDED = 2
RELOAD_S = 60.0          # Vorlagen und Wertemenge so oft neu laden

_lock = threading.Lock()
_listeners = []
_state = {'thread': None, 'stop': None, 'shown': None, 'last_frame': None}


def listen(callback):
    """`callback(wert_oder_None)` bei jeder Änderung — aus dem Wach-Faden!"""
    with _lock:
        if callback not in _listeners:
            _listeners.append(callback)


def _publish(value):
    _state['shown'] = value
    for callback in list(_listeners):
        try:
            callback(value)
        except Exception:
            pass


def shown():
    """Die zuletzt gemeldete Signatur oder None."""
    return _state['shown']


def last_frame():
    """Das letzte Bild, in dem eine Ziffernreihe stand — fürs Anlernen.

    Gibt (raster, zeitpunkt) oder None.
    """
    return _state['last_frame']


def running():
    thread = _state['thread']
    return bool(thread and thread.is_alive())


def start():
    """Die Wache anwerfen (tut nichts, wenn sie schon läuft oder nicht geht)."""
    from . import screen_grab
    if not screen_grab.supported() or running():
        return running()
    stop = threading.Event()
    thread = threading.Thread(target=_loop, args=(stop,), daemon=True,
                              name='signatur-wache')
    _state['stop'], _state['thread'] = stop, thread
    thread.start()
    return True


def stop():
    event = _state['stop']
    if event:
        event.set()
    _state['thread'] = None
    if _state['shown'] is not None:
        _publish(None)


def start_if_enabled():
    from . import paths
    if paths.setting_bool(SETTING, False):
        return start()
    return False


def step(grab, read, foreground, region, history, now, last_seen):
    """Ein Takt der Wache — ohne Faden und ohne Bildschirm prüfbar.

    Gibt (zu_meldender_wert_oder_KEIN, neues_last_seen). `KEIN` (der Wert
    `False`) heißt „nichts ändern", `None` heißt „Anzeige leeren".
    """
    if region is None or not foreground():
        if _state['shown'] is not None and now - last_seen > CLEAR_S:
            return None, last_seen
        return False, last_seen
    raster = grab(*region)
    result = read(raster)
    if result.get('ziffern'):
        _state['last_frame'] = (raster, now)
    history.append(result.get('wert'))
    del history[:-VOTES]
    value = result.get('wert')
    if value is not None and history.count(value) >= NEEDED:
        last_seen = now
        if value != _state['shown']:
            return value, last_seen
        return False, last_seen
    if _state['shown'] is not None and now - last_seen > CLEAR_S:
        return None, last_seen
    return False, last_seen


def _loop(stop_event):
    from . import errors, screen_grab, signature_scan
    history, last_seen = [], 0.0
    cache = {'at': 0.0, 'known': None, 'values': None, 'region': None}
    failures = 0
    while not stop_event.is_set():
        started = time.time()
        try:
            if started - cache['at'] > RELOAD_S:
                cache.update(at=started, known=signature_scan.templates(),
                             values=signature_scan.possible_values())
            cache['region'] = signature_scan.region()
            value, last_seen = step(
                screen_grab.grab,
                lambda r: signature_scan.read(r, cache['known'], cache['values']),
                screen_grab.foreground_is_game, cache['region'], history,
                started, last_seen)
            if value is not False:
                _publish(value)
            failures = 0
        except screen_grab.GrabError:
            failures += 1
        except Exception as exc:
            failures += 1
            if failures == 1:
                errors.record('signature_watch.loop', exc)
        # Nach wiederholten Fehlern langsamer, nie ganz aus: Ein Vollbildwechsel
        # kann den Abgriff kurz scheitern lassen.
        pause = INTERVAL_S if failures < 5 else 2.0
        stop_event.wait(max(0.05, pause - (time.time() - started)))
