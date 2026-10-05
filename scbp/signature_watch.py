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
Bergbau-Scanner ab. Die Zahl wäre weg, bevor sie gelesen ist.

| Regel | Warum |
|---|---|
| Nur, solange Star Citizen **vorn** ist | sonst liest sie den Desktop — eine Zahl im Browser wäre ein Treffer |
| Angezeigt erst bei **zwei gleichen** Lesungen aus den letzten drei | das HUD flackert; eine einzelne Lesung kann danebenliegen |
| Weg ist die Anzeige erst nach `CLEAR_S` ohne Zahl | beim Drehen des Schiffs verschwindet die Pille für Augenblicke |
| **Standard aus** (`SETTING`) | Bildabgriff ohne Zustimmung wäre ein Vertrauensbruch |
| Nach einem Fund nur **um die Pille herum** gesucht (`Tracker`) | die volle Suche über das halbe Spielbild kostet ein Vielfaches |
| Ohne Fund seltener voll gesucht (`FULL_BUSY_S`, `FULL_IDLE_S`) | die Suche läuft neben dem Spiel und nimmt ihm Rechenzeit |
"""
import threading
import time

SETTING = 'signatur_wache'
INTERVAL_S = 0.5
CLEAR_S = 4.0
VOTES = 3
NEEDED = 2
RELOAD_S = 60.0          # Vorlagen und Wertemenge so oft neu laden

# Das Fenster um die zuletzt gefundene Pille: so viele Pillenbreiten bzw.
# -höhen zu jeder Seite, mindestens so viele Bildpunkte.
TRACK_MARGIN_W, TRACK_MARGIN_MIN_X = 3.0, 240
TRACK_MARGIN_H, TRACK_MARGIN_MIN_Y = 4.0, 120
TRACK_MISSES = 2         # so viele Takte ohne Wert, dann gilt die Pille als weg
RECHECK_S = 5.0         # auch mit Pille so oft voll suchen — eine mittigere gewinnt
FULL_BUSY_S = 1.0        # ohne Pille, aber kürzlich eine gesehen: so oft voll suchen
FULL_IDLE_S = 2.0        # lange keine Pille: so oft voll suchen
IDLE_AFTER_S = 8.0       # so lange nach dem letzten Wert gilt „kürzlich"

_lock = threading.Lock()
_listeners = []
_state = {'thread': None, 'stop': None, 'shown': None, 'last_frame': None,
          'reload': False}


def listen(callback):
    """`callback(wert_oder_None)` bei jeder Änderung — aus dem Wach-Faden!"""
    with _lock:
        if callback not in _listeners:
            _listeners.append(callback)


def unlisten(callback):
    with _lock:
        if callback in _listeners:
            _listeners.remove(callback)


def _publish(value):
    _state['shown'] = value
    for callback in list(_listeners):
        try:
            callback(value)
        except Exception:
            pass


def shown():
    """Die zuletzt angezeigte Signatur oder None."""
    return _state['shown']


def last_frame():
    """Das letzte Bild, in dem eine Ziffernreihe stand — fürs Anlernen.

    Gibt (raster, zeitpunkt) oder None.
    """
    return _state['last_frame']


def reload_templates():
    """Nach dem Anlernen: Vorlagen beim nächsten Takt neu laden, nicht erst in 60 s."""
    _state['reload'] = True


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


_switch_listeners = []


def on_switch(callback):
    """`callback(an)` nach jedem Umschalten — für alle Anzeigen des Schalters."""
    if callback not in _switch_listeners:
        _switch_listeners.append(callback)


def set_enabled(on):
    """Den Scanner ein- oder ausschalten — der EINE Weg dafür.

    ⚠⚠ Es gibt zwei Schalter für dieselbe Sache: das Auge in der Overlay-Leiste
    und den Schalter zur automatischen Erkennung auf der Bergbau-Seite.
    Schaltete jeder für sich, liefen sie auseinander. Deshalb gehen beide hier
    durch, und jede Anzeige meldet sich über `on_switch`.
    """
    from . import paths
    paths.set_setting(SETTING, bool(on))
    if on:
        start()
    else:
        stop()
    for callback in list(_switch_listeners):
        try:
            callback(bool(on))
        except Exception:
            # Eine Anzeige, deren Fenster zu ist, fliegt heraus.
            _switch_listeners.remove(callback)
    return bool(on)


def start_if_enabled():
    from . import paths
    if paths.setting_bool(SETTING, False):
        return start()
    return False


def step(grab, read, foreground, region, history, now, last_seen):
    """Ein Takt der Wache — ohne Faden und ohne Bildschirm prüfbar.

    Gibt (zu_meldender_wert_oder_KEIN, neues_last_seen). `KEIN` (der Wert
    `False`) heißt: nichts ändern, `None` heißt: Anzeige leeren.
    """
    if region is None or not foreground():
        if _state['shown'] is not None and now - last_seen > CLEAR_S:
            return None, last_seen
        return False, last_seen
    raster = grab(*region)
    result = read(raster)
    if result.get('ziffern'):
        # Der Ausschnitt um die Pille (bei der Suche `bild`), dazu der gelesene
        # Wert — das Anlern-Fenster zeigt genau dieses Bild, stehend.
        _state['last_frame'] = (result.get('bild') or raster, now, result.get('wert'))
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


def search_area(game):
    """Der Suchbereich im Spielfenster (links, oben, breite, höhe) — oder None.

    ⚠⚠ Gesucht wird in der Bildmitte des Spielfensters, nicht in einem festen
    Bereich — die Pille wandert mit dem Brocken.
    """
    if not game:
        return None
    from . import signature_scan
    fx, fy, fw, fh = signature_scan.SEARCH_AREA
    area = (game[0] + int(game[2] * fx), game[1] + int(game[3] * fy),
            int(game[2] * fw), int(game[3] * fh))
    return area if area[2] > 0 and area[3] > 0 else None


class Tracker(object):
    """Entscheidet je Takt, WO gesucht wird — ohne Faden und Bildschirm prüfbar.

    | Lage | Suche |
    |---|---|
    | Pille zuletzt gefunden | nur das Fenster um sie (`TRACK_MARGIN_*`), alle `RECHECK_S` der ganze Bereich |
    | Pille `TRACK_MISSES` Takte weg | der ganze Bereich, höchstens alle `FULL_BUSY_S` |
    | seit `IDLE_AFTER_S` kein Wert | der ganze Bereich, höchstens alle `FULL_IDLE_S` |

    `grab_raw(links, oben, breite, höhe)` liefert BGRA-Bytes,
    `search(roh, breite, höhe)` das dict von `signature_scan.search`.
    `look` gibt dieses dict (Kasten in Bildschirmpunkten unter `kasten_abs`)
    oder None, wenn in diesem Takt nicht gesucht wird.
    """

    def __init__(self, grab_raw, search):
        self.grab_raw, self.search = grab_raw, search
        self.box = None             # zuletzt gefundene Pille, Bildschirmpunkte
        self.misses = 0
        self.last_full = None
        self.last_hit = None
        self.full_searches = 0
        self.tracked_searches = 0

    def _due(self, now, every):
        # Eine Uhr, die zurückspringt, hält die volle Suche nicht an.
        return (self.last_full is None or now < self.last_full
                or now - self.last_full >= every)

    def window(self, area):
        """Das Fenster um die zuletzt gefundene Pille, beschnitten auf `area`."""
        left, top, width, height = self.box
        margin_x = max(TRACK_MARGIN_MIN_X, int(width * TRACK_MARGIN_W))
        margin_y = max(TRACK_MARGIN_MIN_Y, int(height * TRACK_MARGIN_H))
        x0 = max(area[0], left - margin_x)
        y0 = max(area[1], top - margin_y)
        x1 = min(area[0] + area[2], left + width + margin_x)
        y1 = min(area[1] + area[3], top + height + margin_y)
        if x1 - x0 < 8 or y1 - y0 < 8:
            return None
        return (x0, y0, x1 - x0, y1 - y0)

    def _run(self, rect, now):
        result = self.search(self.grab_raw(*rect), rect[2], rect[3])
        box = result.get('kasten')
        result['kasten_abs'] = (rect[0] + box[0], rect[1] + box[1], box[2], box[3]) \
            if box else None
        if result.get('wert') is not None and box:
            self.box, self.misses, self.last_hit = result['kasten_abs'], 0, now
        elif self.box is not None:
            self.misses += 1
            if self.misses >= TRACK_MISSES:
                self.box = None
        return result

    def look(self, game, now):
        area = search_area(game)
        if area is None:
            return None
        window = self.window(area) if self.box is not None else None
        if self.box is not None and window is None:
            self.box = None
        if window is not None and not self._due(now, RECHECK_S):
            self.tracked_searches += 1
            result = self._run(window, now)
            # Steht die Pille nicht mehr im Fenster, kann sie gesprungen sein:
            # dann gleich im ganzen Bereich nachsehen, sofern dafür Zeit ist.
            if result.get('wert') is not None or not self._due(now, FULL_BUSY_S):
                return result
        elif window is None:
            recent = self.last_hit is not None and now - self.last_hit < IDLE_AFTER_S
            if not self._due(now, FULL_BUSY_S if recent else FULL_IDLE_S):
                return None
        self.full_searches += 1
        self.last_full = now
        return self._run(area, now)


def _loop(stop_event):
    from . import errors, screen_grab, signature_scan
    history, last_seen = [], 0.0
    cache = {'at': 0.0, 'known': None, 'values': None, 'region': None}
    tracker = Tracker(screen_grab.grab_raw,
                      lambda raw, w, h: signature_scan.search(
                          raw, w, h, cache['known'], cache['values']))
    failures = 0
    while not stop_event.is_set():
        started = time.time()
        try:
            if started - cache['at'] > RELOAD_S or _state['reload']:
                _state['reload'] = False
                cache.update(at=started, known=signature_scan.templates(),
                             values=signature_scan.possible_values())
            game = screen_grab.game_rect()
            result = tracker.look(game, time.monotonic()) if game else None
            # Ein Takt ohne Suche geht wie ein Takt ohne Spiel durch `step`:
            # keine Stimme, nur die Frist bis zum Leeren läuft weiter.
            value, last_seen = step(
                lambda *_rect: result, lambda found: found,
                lambda: result is not None, game, history, started, last_seen)
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
