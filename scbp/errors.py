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
Fehler mitschreiben, damit aus „geht nicht" ein Befund wird.

**Das Problem, das dieses Modul löst:** Im Programm stehen über 60 Stellen, die
`except Exception` abfangen und weitermachen. Das ist richtig so — ein Overlay
darf nicht abstürzen, weil eine Netzabfrage klemmt. Nur war der Fehler danach
**spurlos weg**: Wer „bei mir kommt nichts an" meldet, hatte nichts zu schicken,
und hier war nichts nachzustellen.

Ab jetzt landet jeder unerwartete Fehler in `fehler.json` im eigenen Ordner —
mit Zeitpunkt, Stelle, Art und Meldung. Aufgehoben werden die **letzten 50**;
alles ältere fällt hinten heraus, damit die Datei nicht wächst.

Drei Wege hinein:

  1. **Zentrale Haken** (`install_hooks`) — fangen, was sonst niemand fängt:
     Fehler im Hauptstrang, im Watcher-Thread und in den Rückrufen der
     Oberfläche. Gerade der letzte Fall ist bei `tkinter` der übliche Weg, auf
     dem Fehler verschwinden: Tk schreibt sie auf die Standardausgabe, und die
     sieht in einer `.exe` oder einem AppImage **niemand**.
  2. **`with gefangen('stelle'):`** — für Abschnitte, die weiterlaufen sollen,
     deren Scheitern aber etwas bedeutet.
  3. **`merken(stelle, ausnahme)`** — von Hand in einem vorhandenen `except`.

**Dieses Modul darf niemals selbst etwas kaputt machen.** Jede Funktion fängt
ihre eigenen Fehler ab: Ein Protokoll, das den Programmstart verhindert, wäre
schlimmer als gar keines.

Geschrieben wird **nur lokal**. Verschickt wird nichts — was in einen
Fehlerbericht wandert, entscheidet der Spieler in `scbp/report.py`.
"""
import faulthandler
import json
import os
import sys
import threading
import traceback
from datetime import datetime

from . import paths

FILE = 'fehler.json'
MAX_ENTRIES = 50          # so viele Einträge bleiben aufgehoben
TRACE_LINES = 6          # so viele Zeilen Rückverfolgung je Eintrag

_lock = threading.Lock()


def _path():
    return paths.app_file(FILE)


def _read():
    try:
        with open(_path(), encoding='utf-8') as f:
            data = json.load(f)
        entries = data.get('eintraege')
        return entries if isinstance(entries, list) else []
    except Exception:
        return []


# Die laufende Version. Das Hauptprogramm trägt sie beim Start ein.
#
# ⚠ Warum das im Fehlerspeicher steht: Er hebt die letzten zehn Einträge auf,
# über Programmstarts hinweg. Nach einem Update stehen dort also Fehler, die
# längst behoben sind — und im Bericht sieht das aus, als sei alles noch kaputt.
# Genau so passiert: Ein Bericht aus rc9 führte acht `AttributeError` auf, die in
# rc1 behoben worden waren. Mit der Version daneben ist das auf einen Blick
# erkennbar.
VERSION = ['']


TRAIL_FILE = 'start-spur.txt'


def trail(step):
    """Festhalten, wie weit der Start gekommen ist — überlebt einen Absturz.

    ⚠ Wozu: Ein `SIGSEGV` beendet den Prozess **sofort**. Kein `except` greift,
    kein Fehlerbericht wird geschrieben, und der Nutzer kann nur sagen „es stürzt
    ab". Genau das ist am 25.08.2026 passiert: Ein Tester meldete einen Absturz
    beim ersten Start, reproduzierbar bei ihm — und auf dem Entwicklungsrechner
    ließ er sich nicht nachstellen. Ohne Spur bleibt nur Raten.

    Deshalb schreibt jeder Startschritt eine Zeile, **sofort auf die Platte**
    (`flush` + `fsync`, sonst steht bei einem Absturz nur ein leerer Puffer da).
    Beim nächsten Start steht die letzte Zeile im Diagnose-Bericht: Was danach
    käme, ist die Stelle, an der es geknallt hat.

    Die Datei wird bei jedem Start neu angelegt — sie soll den letzten Lauf
    zeigen, kein Tagebuch sein.
    """
    try:
        path = paths.app_file(TRAIL_FILE)
        mode = 'a' if getattr(trail, '_offen', False) else 'w'
        trail._offen = True
        with open(path, mode, encoding='utf-8') as f:
            f.write('%s  %s\n' % (datetime.now().strftime('%H:%M:%S'), step))
            f.flush()
            os.fsync(f.fileno())
        # ⚠ Seit die Spur auch die Bedienung mitschreibt, wächst sie mit jedem
        # Klick. Nach oben deckeln, sonst steht am Ende ein Tagebuch aus
        # hunderten Reiterwechseln da. Gekürzt wird selten und nur um Zeilen,
        # die der Bericht ohnehin nicht mehr zeigt — er nimmt die letzten zwölf.
        trail._zahl = getattr(trail, '_zahl', 0) + 1
        if trail._zahl >= TRAIL_CAP:
            trail._zahl = 0
            _trim_trail(path)
    except Exception:
        pass


# Ab so vielen neuen Zeilen wird nachgesehen und auf `TRAIL_KEEP` gekürzt.
TRAIL_CAP = 200
TRAIL_KEEP = 60


def _trim_trail(path):
    """Die Spur eindampfen — **ohne** den Startverlauf zu opfern.

    ⚠ Vorne abzuschneiden wäre das Naheliegende und wäre falsch: Vorne steht
    der Start, und der ist bei einem Absturz das Wertvollste. Gekürzt wird
    deshalb nur der Bedienteil.
    """
    try:
        with open(path, encoding='utf-8') as f:
            all_lines = f.readlines()
        if len(all_lines) <= TRAIL_KEEP:
            return
        boundary = _boundary_index(all_lines)
        if boundary is None:
            # Der Start ist noch nicht durch und schreibt trotzdem schon
            # hunderte Zeilen — dann steckt die Ursache am Anfang, nicht am
            # Ende. Hier ausnahmsweise hinten abschneiden.
            head, tail = all_lines[:TRAIL_KEEP], []
        else:
            head, tail = all_lines[:boundary + 1], all_lines[boundary + 1:]
        with open(path, 'w', encoding='utf-8') as f:
            f.writelines(head + tail[-TRAIL_KEEP:])
    except OSError:
        pass



# Die letzte Zeile des Starts. Alles danach ist Bedienung.
#
# ⚠ Warum ein fester Satz und keine Liste von Vorsilben: Bis rc42 galt die
# umgekehrte Regel — „was mit ‚Seite ‘ anfängt, ist Bedienung, alles andere ist
# Start". Das hält nur, solange niemand woanders im Programm einen neuen
# Spur-Aufruf einbaut. Genau das passierte: `Liste: zeichnen beginnt` aus der
# Bauplan-Liste galt als Startschritt und verdrängte mit zwölf gleichen Zeilen
# den kompletten Startverlauf aus dem Bericht (rc42, 30.08.2026) — ausgerechnet
# den Teil, für den die Spur gebaut wurde.
#
# Der Start endet an genau **einer** Stelle: wenn die Hauptschleife anläuft.
# Dort wird getrennt. Ein neuer Spur-Aufruf irgendwo im laufenden Programm kann
# das nicht mehr kaputtmachen — er steht zwangsläufig danach.
#
# ⚠ Wer die Zeile in `sc_bp_watcher.py` umbenennt, muss sie hier mitziehen;
# Prüfung 72 im Selbsttest schlägt sonst an.
TRAIL_BOUNDARY = 'Hauptschleife läuft'


def _boundary_index(lines):
    """Wo der Start endet — Platz der Grenzzeile, oder `None`.

    `None` heißt: Der Start ist gar nicht durchgelaufen. Dann ist alles
    Startverlauf, und das ist die richtige Antwort — bei einem Absturz während
    des Starts gibt es keine Bedienung.
    """
    for i, line in enumerate(lines):
        if line.rstrip().endswith(TRAIL_BOUNDARY):
            return i
    return None


def last_trail():
    """Die Spur des letzten Laufs — Startschritte und Bedienung, wie sie kam."""
    try:
        with open(paths.app_file(TRAIL_FILE), encoding='utf-8') as f:
            return [z.rstrip() for z in f if z.strip()]
    except Exception:
        return []


def split_trail():
    """Die Spur in zwei Teile: (Startschritte, Seitenwechsel).

    ⚠ Wozu die Trennung: Der Bericht zeigt nur die letzten Zeilen, sonst wird
    er unlesbar. Seit die Bedienung mitschreibt, drängten schon **fünf Klicks**
    den kompletten Startverlauf hinaus — und genau der ist der Grund, warum es
    die Spur überhaupt gibt. Im ersten rc74-Bericht (27.08.2026) stand kein
    einziger Startschritt mehr. Beide Teile werden deshalb getrennt gedeckelt.

    Getrennt wird an `TRAIL_BOUNDARY` — siehe die Begründung dort.
    """
    all_lines = last_trail()
    boundary = _boundary_index(all_lines)
    if boundary is None:
        return all_lines, []
    return all_lines[:boundary + 1], all_lines[boundary + 1:]


CRASH_FILE = 'absturz.txt'
CRASH_PREVIOUS = 'absturz-letzter.txt'

# Der offene Schreibkanal, in den `faulthandler` schreibt. Er muss den ganzen
# Lauf offen bleiben — deshalb steht er hier und nicht in einer Funktion.
_CRASH_STREAM = [None]


def install_crash_handler():
    """Einen harten Abbruch festhalten — dort, wo kein `except` mehr greift.

    ⚠ Wozu, obwohl es `install_hooks` schon gibt: Die drei Haken dort fangen
    **Python**-Ausnahmen. Ein `SIGSEGV` aus der Tk-Bibliothek ist keine —
    der Prozess ist weg, mitten im Befehl. Es gibt dann keinen Fehlereintrag,
    keine Meldung, nichts; der Nutzer kann nur sagen „es stürzt ab".

    Genau dieser Fall ist zweimal aufgetreten: am 25.08.2026 beim ersten Start
    (zwei Tk-Instanzen) und am 27.08.2026 beim Öffnen von „Was ist neu" —
    beide Male reproduzierbar beim Melder, beide Male auf dem
    Entwicklungsrechner nicht nachstellbar, und beide Male stand im
    Diagnose-Bericht **kein Wort** davon.

    `faulthandler` schreibt beim Signal den C-nahen Aufrufweg aller Fäden in
    eine Datei — die einzige Spur, die ein solcher Abbruch hinterlässt. Beim
    nächsten Start wird sie zur Seite gelegt und landet im Bericht.
    """
    try:
        current = paths.app_file(CRASH_FILE)
        previous = paths.app_file(CRASH_PREVIOUS)
        # Was vom letzten Lauf noch drinsteht, ist ein Absturz — beiseitelegen,
        # damit der Bericht ihn zeigen kann, auch wenn dieser Lauf sauber ist.
        try:
            if os.path.isfile(current) and os.path.getsize(current) > 0:
                if os.path.isfile(previous):
                    os.remove(previous)
                os.replace(current, previous)
            elif os.path.isfile(current):
                os.remove(current)
        except OSError:
            pass
        stream = open(current, 'w', encoding='utf-8')
        _CRASH_STREAM[0] = stream
        faulthandler.enable(file=stream, all_threads=True)
        return True
    except Exception:
        # Ohne Fänger läuft das Programm normal weiter — er ist Diagnose,
        # keine Voraussetzung.
        return False


def last_crash():
    """Der Aufrufweg des letzten harten Abbruchs — leer, wenn es keinen gab."""
    try:
        with open(paths.app_file(CRASH_PREVIOUS), encoding='utf-8') as f:
            return [z.rstrip() for z in f if z.strip()]
    except Exception:
        return []


def crash_time():
    """Wann der festgehaltene Abbruch geschah — als Zeitstempel, oder None.

    ⚠ Die Datei bleibt liegen, bis ein neuer Abbruch sie ersetzt. Ohne Datum
    stand ein Absturz vom 12.09.2026 am 16.09. noch als „beim vorigen Lauf" im
    Bericht — aus einer Fassung, deren Dateinamen es längst nicht mehr gab.
    """
    try:
        return os.path.getmtime(paths.app_file(CRASH_PREVIOUS))
    except Exception:
        return None


def clear_crash():
    """Den festgehaltenen Abbruch wegräumen — er ist gemeldet und erledigt."""
    try:
        os.remove(paths.app_file(CRASH_PREVIOUS))
        return True
    except Exception:
        return False


def record(label, exc=None, note=''):
    """Einen Fehler festhalten. Gibt True zurück, wenn es geklappt hat.

    `stelle` ist der Ort im Programm ('catalog.update') — er sagt beim
    Lesen mehr als jede Fehlermeldung. `hinweis` ist Platz für eine Angabe, die
    aus der Ausnahme nicht hervorgeht (welche Datei, welche Adresse).
    """
    try:
        if exc is None:
            exc = sys.exc_info()[1]

        entry = {
            'zeit': datetime.now().strftime('%Y-%m-%d %H:%M:%S'),
            'fassung': VERSION[0],
            'stelle': str(label),
            'art': type(exc).__name__ if exc else 'Hinweis',
            'meldung': paths.redact(str(exc) if exc else note),
        }
        if note and exc is not None:
            entry['hinweis'] = paths.redact(str(note))

        if exc is not None:
            tb = traceback.format_exception(type(exc), exc,
                                              exc.__traceback__)
            # Nur der Schwanz der Rückverfolgung — dort steht, wo es knallte.
            # Die Zeilen davor sind bei einem Overlay fast immer dieselben.
            entry['spur'] = paths.redact(''.join(tb[-TRACE_LINES:]).strip())

        with _lock:
            entries = _read()
            entries.append(entry)
            entries = entries[-MAX_ENTRIES:]
            with open(_path(), 'w', encoding='utf-8') as f:
                json.dump({'eintraege': entries}, f, ensure_ascii=False, indent=1)
        return True
    except Exception:
        return False          # ein Protokoll darf nie das Programm mitreißen


def latest(count=10):
    """Die jüngsten Einträge, neueste zuerst."""
    try:
        return list(reversed(_read()))[:max(0, int(count))]
    except Exception:
        return []


def count():
    """Wie viele Einträge liegen vor?"""
    return len(_read())


def clear():
    """Alles vergessen — z. B. nachdem ein Problem behoben wurde."""
    try:
        with _lock:
            with open(_path(), 'w', encoding='utf-8') as f:
                json.dump({'eintraege': []}, f)
        return True
    except Exception:
        return False


class caught(object):
    """Kontextmanager: Der Abschnitt darf scheitern, aber nicht schweigen.

        with errors.caught('catalog.update'):
            catalog.holen()

    Der Fehler wird festgehalten und **verschluckt** — der Aufrufer läuft
    weiter, so wie es die vorhandenen `except Exception`-Stellen tun. Wer den
    Fehler weiterreichen will, nimmt `gefangen(..., weiterreichen=True)`.
    """

    def __init__(self, label, note='', reraise=False):
        self.label = label
        self.note = note
        self.reraise = reraise

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_value, tb):
        if exc_value is None:
            return False
        record(self.label, exc_value, self.note)
        return not self.reraise


def install_hooks(root=None):
    """Die drei Wege abfangen, auf denen Fehler sonst unbemerkt verschwinden.

    `wurzel` ist das Tk-Hauptfenster, falls schon eines da ist. Ohne Oberfläche
    (Selbsttest, Werkzeuge) werden nur die ersten beiden Haken gesetzt.
    """
    try:
        previous_hook = sys.excepthook

        def main_hook(exc_type, exc_value, tb):
            record('unbehandelt', exc_value)
            previous_hook(exc_type, exc_value, tb)

        sys.excepthook = main_hook
    except Exception:
        pass

    try:
        # Ohne diesen Haken stirbt der Watcher-Thread still, und das Overlay
        # steht danach da, als liefe alles — es kommt nur nie wieder etwas an.
        def thread_hook(hook_args):
            record('thread:%s' % getattr(hook_args.thread, 'name', '?'),
                   hook_args.exc_value)

        threading.excepthook = thread_hook
    except Exception:
        pass

    if root is not None:
        try:
            def ui_hook(exc_type, exc_value, tb):
                record('oberflaeche', exc_value)

            root.report_callback_exception = ui_hook
        except Exception:
            pass


if __name__ == '__main__':
    print('Protokoll:', _path())
    with caught('probe'):
        raise ValueError('nur ein Versuch')
    for e in latest(3):
        print('  %s  %-22s %s: %s' % (e['zeit'], e['stelle'], e['art'], e['meldung']))
