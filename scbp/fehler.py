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

  1. **Zentrale Haken** (`haken_setzen`) — fangen, was sonst niemand fängt:
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
Fehlerbericht wandert, entscheidet der Spieler in `scbp/bericht.py`.
"""
import faulthandler
import json
import os
import sys
import threading
import traceback
from datetime import datetime

from . import pfade

DATEI = 'fehler.json'
HOECHSTENS = 50          # so viele Einträge bleiben aufgehoben
SPUR_ZEILEN = 6          # so viele Zeilen Rückverfolgung je Eintrag

_schloss = threading.Lock()


def _pfad():
    return pfade.app_datei(DATEI)


def _lesen():
    try:
        with open(_pfad(), encoding='utf-8') as f:
            daten = json.load(f)
        eintraege = daten.get('eintraege')
        return eintraege if isinstance(eintraege, list) else []
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


SPUR_DATEI = 'start-spur.txt'


def spur(schritt):
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
        pfad = pfade.app_datei(SPUR_DATEI)
        art = 'a' if getattr(spur, '_offen', False) else 'w'
        spur._offen = True
        with open(pfad, art, encoding='utf-8') as f:
            f.write('%s  %s\n' % (datetime.now().strftime('%H:%M:%S'), schritt))
            f.flush()
            os.fsync(f.fileno())
        # ⚠ Seit die Spur auch die Bedienung mitschreibt, wächst sie mit jedem
        # Klick. Nach oben deckeln, sonst steht am Ende ein Tagebuch aus
        # hunderten Reiterwechseln da. Gekürzt wird selten und nur um Zeilen,
        # die der Bericht ohnehin nicht mehr zeigt — er nimmt die letzten zwölf.
        spur._zahl = getattr(spur, '_zahl', 0) + 1
        if spur._zahl >= SPUR_DECKEL:
            spur._zahl = 0
            _spur_kuerzen(pfad)
    except Exception:
        pass


# Ab so vielen neuen Zeilen wird nachgesehen und auf `SPUR_REST` gekürzt.
SPUR_DECKEL = 200
SPUR_REST = 60


def _spur_kuerzen(pfad):
    """Die Spur eindampfen — **ohne** den Startverlauf zu opfern.

    ⚠ Vorne abzuschneiden wäre das Naheliegende und wäre falsch: Vorne steht
    der Start, und der ist bei einem Absturz das Wertvollste. Gekürzt wird
    deshalb nur der Bedienteil.
    """
    try:
        with open(pfad, encoding='utf-8') as f:
            alle = f.readlines()
        if len(alle) <= SPUR_REST:
            return
        stelle = _grenz_stelle(alle)
        if stelle is None:
            # Der Start ist noch nicht durch und schreibt trotzdem schon
            # hunderte Zeilen — dann steckt die Ursache am Anfang, nicht am
            # Ende. Hier ausnahmsweise hinten abschneiden.
            kopf, rest = alle[:SPUR_REST], []
        else:
            kopf, rest = alle[:stelle + 1], alle[stelle + 1:]
        with open(pfad, 'w', encoding='utf-8') as f:
            f.writelines(kopf + rest[-SPUR_REST:])
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
SPUR_GRENZE = 'Hauptschleife läuft'


def _grenz_stelle(zeilen):
    """Wo der Start endet — Platz der Grenzzeile, oder `None`.

    `None` heißt: Der Start ist gar nicht durchgelaufen. Dann ist alles
    Startverlauf, und das ist die richtige Antwort — bei einem Absturz während
    des Starts gibt es keine Bedienung.
    """
    for i, zeile in enumerate(zeilen):
        if zeile.rstrip().endswith(SPUR_GRENZE):
            return i
    return None


def letzte_spur():
    """Die Spur des letzten Laufs — Startschritte und Bedienung, wie sie kam."""
    try:
        with open(pfade.app_datei(SPUR_DATEI), encoding='utf-8') as f:
            return [z.rstrip() for z in f if z.strip()]
    except Exception:
        return []


def spur_geteilt():
    """Die Spur in zwei Teile: (Startschritte, Seitenwechsel).

    ⚠ Wozu die Trennung: Der Bericht zeigt nur die letzten Zeilen, sonst wird
    er unlesbar. Seit die Bedienung mitschreibt, drängten schon **fünf Klicks**
    den kompletten Startverlauf hinaus — und genau der ist der Grund, warum es
    die Spur überhaupt gibt. Im ersten rc74-Bericht (27.08.2026) stand kein
    einziger Startschritt mehr. Beide Teile werden deshalb getrennt gedeckelt.

    Getrennt wird an `SPUR_GRENZE` — siehe die Begründung dort.
    """
    alle = letzte_spur()
    stelle = _grenz_stelle(alle)
    if stelle is None:
        return alle, []
    return alle[:stelle + 1], alle[stelle + 1:]


ABSTURZ_DATEI = 'absturz.txt'
ABSTURZ_VORIG = 'absturz-letzter.txt'

# Der offene Schreibkanal, in den `faulthandler` schreibt. Er muss den ganzen
# Lauf offen bleiben — deshalb steht er hier und nicht in einer Funktion.
_ABSTURZ_KANAL = [None]


def absturzfaenger():
    """Einen harten Abbruch festhalten — dort, wo kein `except` mehr greift.

    ⚠ Wozu, obwohl es `haken_setzen` schon gibt: Die drei Haken dort fangen
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
        jetzt = pfade.app_datei(ABSTURZ_DATEI)
        vorig = pfade.app_datei(ABSTURZ_VORIG)
        # Was vom letzten Lauf noch drinsteht, ist ein Absturz — beiseitelegen,
        # damit der Bericht ihn zeigen kann, auch wenn dieser Lauf sauber ist.
        try:
            if os.path.isfile(jetzt) and os.path.getsize(jetzt) > 0:
                if os.path.isfile(vorig):
                    os.remove(vorig)
                os.replace(jetzt, vorig)
            elif os.path.isfile(jetzt):
                os.remove(jetzt)
        except OSError:
            pass
        kanal = open(jetzt, 'w', encoding='utf-8')
        _ABSTURZ_KANAL[0] = kanal
        faulthandler.enable(file=kanal, all_threads=True)
        return True
    except Exception:
        # Ohne Fänger läuft das Programm normal weiter — er ist Diagnose,
        # keine Voraussetzung.
        return False


def letzter_absturz():
    """Der Aufrufweg des letzten harten Abbruchs — leer, wenn es keinen gab."""
    try:
        with open(pfade.app_datei(ABSTURZ_VORIG), encoding='utf-8') as f:
            return [z.rstrip() for z in f if z.strip()]
    except Exception:
        return []


def absturz_abhaken():
    """Den festgehaltenen Abbruch wegräumen — er ist gemeldet und erledigt."""
    try:
        os.remove(pfade.app_datei(ABSTURZ_VORIG))
        return True
    except Exception:
        return False


def merken(stelle, ausnahme=None, hinweis=''):
    """Einen Fehler festhalten. Gibt True zurück, wenn es geklappt hat.

    `stelle` ist der Ort im Programm ('catalog.update') — er sagt beim
    Lesen mehr als jede Fehlermeldung. `hinweis` ist Platz für eine Angabe, die
    aus der Ausnahme nicht hervorgeht (welche Datei, welche Adresse).
    """
    try:
        if ausnahme is None:
            ausnahme = sys.exc_info()[1]

        eintrag = {
            'zeit': datetime.now().strftime('%Y-%m-%d %H:%M:%S'),
            'fassung': VERSION[0],
            'stelle': str(stelle),
            'art': type(ausnahme).__name__ if ausnahme else 'Hinweis',
            'meldung': pfade.kuerzen(str(ausnahme) if ausnahme else hinweis),
        }
        if hinweis and ausnahme is not None:
            eintrag['hinweis'] = pfade.kuerzen(str(hinweis))

        if ausnahme is not None:
            spur = traceback.format_exception(type(ausnahme), ausnahme,
                                              ausnahme.__traceback__)
            # Nur der Schwanz der Rückverfolgung — dort steht, wo es knallte.
            # Die Zeilen davor sind bei einem Overlay fast immer dieselben.
            eintrag['spur'] = pfade.kuerzen(''.join(spur[-SPUR_ZEILEN:]).strip())

        with _schloss:
            eintraege = _lesen()
            eintraege.append(eintrag)
            eintraege = eintraege[-HOECHSTENS:]
            with open(_pfad(), 'w', encoding='utf-8') as f:
                json.dump({'eintraege': eintraege}, f, ensure_ascii=False, indent=1)
        return True
    except Exception:
        return False          # ein Protokoll darf nie das Programm mitreißen


def letzte(anzahl=10):
    """Die jüngsten Einträge, neueste zuerst."""
    try:
        return list(reversed(_lesen()))[:max(0, int(anzahl))]
    except Exception:
        return []


def anzahl():
    """Wie viele Einträge liegen vor?"""
    return len(_lesen())


def leeren():
    """Alles vergessen — z. B. nachdem ein Problem behoben wurde."""
    try:
        with _schloss:
            with open(_pfad(), 'w', encoding='utf-8') as f:
                json.dump({'eintraege': []}, f)
        return True
    except Exception:
        return False


class gefangen(object):
    """Kontextmanager: Der Abschnitt darf scheitern, aber nicht schweigen.

        with fehler.gefangen('catalog.update'):
            catalog.holen()

    Der Fehler wird festgehalten und **verschluckt** — der Aufrufer läuft
    weiter, so wie es die vorhandenen `except Exception`-Stellen tun. Wer den
    Fehler weiterreichen will, nimmt `gefangen(..., weiterreichen=True)`.
    """

    def __init__(self, stelle, hinweis='', weiterreichen=False):
        self.stelle = stelle
        self.hinweis = hinweis
        self.weiterreichen = weiterreichen

    def __enter__(self):
        return self

    def __exit__(self, art, wert, spur):
        if wert is None:
            return False
        merken(self.stelle, wert, self.hinweis)
        return not self.weiterreichen


def haken_setzen(wurzel=None):
    """Die drei Wege abfangen, auf denen Fehler sonst unbemerkt verschwinden.

    `wurzel` ist das Tk-Hauptfenster, falls schon eines da ist. Ohne Oberfläche
    (Selbsttest, Werkzeuge) werden nur die ersten beiden Haken gesetzt.
    """
    try:
        frueher = sys.excepthook

        def haupt(art, wert, spur):
            merken('unbehandelt', wert)
            frueher(art, wert, spur)

        sys.excepthook = haupt
    except Exception:
        pass

    try:
        # Ohne diesen Haken stirbt der Watcher-Thread still, und das Overlay
        # steht danach da, als liefe alles — es kommt nur nie wieder etwas an.
        def im_thread(angaben):
            merken('thread:%s' % getattr(angaben.thread, 'name', '?'),
                   angaben.exc_value)

        threading.excepthook = im_thread
    except Exception:
        pass

    if wurzel is not None:
        try:
            def in_der_oberflaeche(art, wert, spur):
                merken('oberflaeche', wert)

            wurzel.report_callback_exception = in_der_oberflaeche
        except Exception:
            pass


if __name__ == '__main__':
    print('Protokoll:', _pfad())
    with gefangen('probe'):
        raise ValueError('nur ein Versuch')
    for e in letzte(3):
        print('  %s  %-22s %s: %s' % (e['zeit'], e['stelle'], e['art'], e['meldung']))
