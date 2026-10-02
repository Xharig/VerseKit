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
Einmal am Tag „ich laufe" melden — damit sich zählen lässt, wie viele
Installationen es wirklich gibt.

**Was hinausgeht, vollständig** (`payload()`):

| Feld | Inhalt |
|---|---|
| `v` | Programmversion |
| `os` | `windows` oder `linux` |
| `ui` | Sprache der Oberfläche (`de`, `en`) |
| `game` | Sprache des Spiels (`de`, `en`, … — `xx`, wenn unbekannt) |
| `rc` | ob Testversionen angeboten werden (ja/nein) |
| `mods` | welche abschaltbaren Bereiche an sind (`schiffe`, `handel` …) |
| `overlay` | wie das Overlay läuft (`immer` oder `popup`) |
| `autostart` | ob VerseKit mit dem Rechner startet (ja/nein) |
| `update` | ob neue Versionen von selbst eingespielt werden (ja/nein) |

Keine Kennung, kein Name, kein RSI-Handle, keine Pfade. Der Empfänger
(`tools/nutzung-worker/`, ein Cloudflare Worker) zählt je Tag nur `+1` für
jeden Wert und legt die Absender-Adresse nirgends ab; das Land leitet
Cloudflare selbst ab, es wird nicht mitgeschickt. Eine einzelne Installation
lässt sich damit über Tage **nicht** verfolgen — gezählt wird „wie viele
waren heute da", nicht „wer".

**Warum es das gibt:** Die Download-Zahlen von GitHub sagen nur, wie viele
Installationen es *mindestens* gibt (das Auto-Update lädt jede Version einmal).
Wie viele das Werkzeug wirklich benutzen, sagen sie nicht.

- Ab Werk **an**, abschaltbar unter *Einstellungen → Allgemein*
  (`nutzung_melden`), und wie jeder Netzabruf aus mit `SC_BP_NO_NET=1`.
- Der Quellcode-Start meldet **nie** — das wären nur Entwickler-Rechner.
- Höchstens einmal je Kalendertag in **UTC**, so wie der Worker zählt:
  Mit Ortszeit könnte ein Abend über Mitternacht zweimal zählen.
- Scheitert die Meldung (kein Netz, Firewall), bleibt das still und wird beim
  nächsten Versuch nachgeholt. Ins Fehlerprotokoll gehört es nicht: Wer eine
  Firewall hat, bekäme sonst jeden Tag einen „Fehler", der keiner ist.
"""
import json
import os
import re
import threading
import time
import urllib.request

from . import paths

OFF = os.environ.get('SC_BP_NO_NET', '') not in ('', '0')

TARGET = 'https://nutzung-versekit.xharig.com/ping'
SETTING = 'nutzung_melden'
LAST = 'nutzung_zuletzt'
VERSION_RE = re.compile(r'^\d{1,3}\.\d{1,3}\.\d{1,3}(?:-rc\d{1,4})?$')
CHECK_EVERY = 3600          # Sekunden — fängt einen Lauf über Mitternacht ab


def target():
    """Die Adresse. `SC_BP_NUTZUNG_ZIEL` schlägt sie — zum Prüfen gegen einen
    eigenen Testdienst; alles ohne `https://` davor (etwa `aus`) schaltet ab."""
    override = os.environ.get('SC_BP_NUTZUNG_ZIEL')
    if override is not None:
        return override.strip()
    return TARGET


def enabled():
    """Hat der Spieler die Meldung an gelassen? Ab Werk: ja."""
    return paths.setting_bool(SETTING, True)


def today():
    return time.strftime('%Y-%m-%d', time.gmtime())


def system():
    import sys
    return 'windows' if sys.platform.startswith('win') else 'linux'


FIELDS = ('v', 'os', 'ui', 'game', 'rc', 'mods', 'overlay', 'autostart',
          'update')

# Sprachordner des Spiels (`german_(germany)`) -> Kürzel. Unbekanntes wird
# `xx` — nie der Ordnername selbst, der könnte alles Mögliche enthalten.
_GAME_LANGUAGES = {'english': 'en', 'german': 'de', 'french': 'fr',
                   'spanish': 'es', 'italian': 'it', 'portuguese': 'pt',
                   'polish': 'pl', 'chinese': 'zh', 'japanese': 'ja',
                   'korean': 'ko', 'russian': 'ru'}


def _safe(read, fallback):
    try:
        return read()
    except Exception:
        return fallback


def _ui_language():
    from . import language
    code = (language.current() or '')[:2].lower()
    return code if re.match(r'^[a-z]{2}$', code) else 'xx'


def _game_language():
    from . import translation
    folder = (translation.game_language() or 'english').lower()
    return _GAME_LANGUAGES.get(re.split(r'[_(]', folder)[0], 'xx')


def _modules():
    from . import modules
    return sorted(g for g in modules.SWITCHABLE if modules.enabled(g))


def _overlay():
    mode = paths.setting('overlay_modus') or 'immer'
    return mode if mode in ('immer', 'popup') else 'immer'


def _autostart():
    from . import autostart
    return bool(autostart.is_on())


def _auto_update():
    from . import auto_update
    return bool(auto_update.enabled())


def payload(version, system_name=None):
    """Genau das, was hinausgeht — als eigene Funktion, damit es sich prüfen
    lässt und nirgends heimlich mehr dazukommt. Jede Angabe für sich
    abgesichert: Scheitert eine, geht ein neutraler Wert statt keiner Meldung."""
    return {
        'v': version,
        'os': system_name or system(),
        'ui': _safe(_ui_language, 'xx'),
        'game': _safe(_game_language, 'xx'),
        'rc': _safe(lambda: paths.setting_bool('vorabversionen', False), False),
        'mods': _safe(_modules, []),
        'overlay': _safe(_overlay, 'immer'),
        'autostart': _safe(_autostart, False),
        'update': _safe(_auto_update, True),
    }


def _packaging():
    try:
        from . import updater
        return updater.packaging()
    except Exception:
        return 'quellcode'


def send_if_due(version, day=None, opener=None):
    """Sendet, wenn heute noch nicht gesendet wurde. Gibt zurück, was geschah:
    'aus', 'quellcode', 'version', 'schon', 'gesendet' oder 'fehler'."""
    if OFF or not enabled() or not target().startswith('https://'):
        return 'aus'
    if _packaging() == 'quellcode':
        return 'quellcode'
    if not VERSION_RE.match(version or ''):
        return 'version'
    day = day or today()
    if paths.setting(LAST) == day:
        return 'schon'
    data = json.dumps(payload(version)).encode('utf-8')
    request = urllib.request.Request(
        target(), data=data, method='POST',
        headers={'Content-Type': 'application/json',
                 'User-Agent': 'VerseKit/%s' % version})
    try:
        with (opener or urllib.request.urlopen)(request, timeout=15) as answer:
            if answer.status not in (200, 204):
                return 'fehler'
    except Exception:
        return 'fehler'
    paths.set_setting(LAST, day)
    return 'gesendet'


def start(version):
    """Im Hintergrund: gleich einmal, danach stündlich nachsehen."""
    def run():
        while True:
            try:
                result = send_if_due(version)
                if result == 'gesendet':
                    from . import errors
                    errors.trail('Nutzung gemeldet')
            except Exception:
                pass
            time.sleep(CHECK_EVERY)
    threading.Thread(target=run, daemon=True, name='nutzung').start()
