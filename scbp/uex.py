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
Der gemeinsame Unterbau für alle Abrufe bei [UEX Corp](https://uexcorp.space).

Holen, ablegen, Alter bestimmen — das ist bei jeder Liste dasselbe. Was sich
unterscheidet, ist nur, **was** aus der Antwort behalten wird. Genau diese
Trennung macht dieses Modul: Es kennt den Abruf und die Ablage, nicht die
Bedeutung der Daten. Die bleibt im jeweiligen Fachmodul.

## Warum es das gibt

`preise.py`, `orte.py` und `verkauf.py` trugen bis v3.15 **jeweils dieselbe**
Maschinerie: `QUELLE`, `CACHE`, `FORMAT`, `ZEITLIMIT`, `HALTBAR` und dazu
`laden()`, `alter()`, `_holen()`, `_sichern()`. Dreimal derselbe Code, und mit
jedem weiteren Endpunkt eine Kopie mehr.

Das ist nicht nur Schreibarbeit. An jeder Kopie hängen **Regeln, die niemand
sieht**: höchstens einmal am Tag holen, ohne Netz nicht krachen, bei
`SC_BP_NO_NET` gar nichts tun, die Ablage atomar schreiben. Bei sieben Kopien
wird eine davon vergessen — und zwar die, an die niemand denkt.

## Wie ein Fachmodul es benutzt

Eine `Ablage` je Endpunkt, als Modulvariable::

    from . import uex

    _ablage = uex.Ablage('preise.json', format_nr=1, haltbar=uex.TAG)
    QUELLE = 'https://api.uexcorp.uk/2.0/commodities'

    def laden():
        return _ablage.laden()

    def alter():
        return _ablage.alter()

    def aktualisieren():
        if not _ablage.veraltet():
            return True
        liste = uex.holen(QUELLE, 'preise')
        if not liste:
            return False
        _ablage.sichern({'waren': _auswerten(liste)})
        return True

`sichern()` setzt `format` und `geholt` selbst — das Fachmodul gibt nur seine
eigenen Felder mit.

## ⚠ Was hier bewusst NICHT hineingehört

**Die Auswertung.** Jedes Fachmodul weiß selbst, welche Felder es braucht und
welche Fallen darin stecken — dass bei UEX jedes Material zweimal steht
(veredelt und als Erz), dass Namen nur **exakt** verglichen werden dürfen, dass
Zeilen ohne Ankaufgebot wegfallen. Käme das hierher, entstünde ein Modul, das
alles ein bisschen kann und nichts richtig.

## ⚠⚠ Zwei Eigenheiten der Schnittstelle, die hier festgehalten sind

**1. Ohne Kennung antwortet UEX mit HTTP 403.** Ein blanker Abruf ohne
`User-Agent` wird abgewiesen. Gemessen am 04.09.2026.

**2. Eine Antwort ist bei 500 Zeilen abgeschnitten.** Kein Fehler, keine
Meldung — die Liste hört einfach auf. Wer einen zu weiten Zuschnitt wählt,
bekommt stillschweigend ein Bruchstück und merkt es nie. `holen()` meldet
deshalb einen Verdacht ins Fehlerprotokoll, sobald genau `DECKEL` Zeilen
zurückkommen. Der richtige Umgang ist **enger zuschneiden**, nicht mehr
abrufen.
"""
import html
import json
import os
import threading
import time
import urllib.error
import urllib.request

from . import fehler, pfade
from .katalog import AUS, KENNUNG

# Die übliche Frist zwischen zwei Abrufen derselben Liste.
TAG = 24 * 60 * 60
WOCHE = 7 * TAG

# Wie lange auf eine Antwort gewartet wird.
ZEITLIMIT = 30

# ⚠ Ab so vielen Zeilen ist die Antwort vermutlich abgeschnitten. Gemessen am
# 04.09.2026: `commodities_routes?id_planet_origin=…` lieferte bei 7 von 10
# Planeten **exakt** 500 Zeilen. Das ist keine Zufallszahl, das ist der Deckel.
DECKEL = 500


def _entschluesseln(wert):
    """`&apos;` → `'`, rekursiv durch Listen und Wörterbücher.

    ⚠ Nur Zeichenketten werden angefasst; Zahlen und `None` bleiben, wie sie
    sind. Ein Wert ohne `&` wird unverändert zurückgegeben, damit der
    Durchlauf über zehntausende Felder nichts kostet.
    """
    if isinstance(wert, str):
        return html.unescape(wert) if '&' in wert else wert
    if isinstance(wert, list):
        return [_entschluesseln(x) for x in wert]
    if isinstance(wert, dict):
        return dict((k, _entschluesseln(v)) for k, v in wert.items())
    return wert


def holen(adresse, stelle, zeitlimit=ZEITLIMIT):
    """Eine UEX-Liste abrufen.

    Gibt die Datenliste zurück, oder `None`, wenn der Abruf scheitert —
    **nie eine Ausnahme**. Ohne Netz läuft alles weiter wie vorher; das ist der
    Grund, warum hier so großzügig gefangen wird.

    `stelle` ist der Name fürs Fehlerprotokoll, etwa `'preise'`.
    """
    if AUS:
        return None
    try:
        anfrage = urllib.request.Request(
            adresse, headers={'User-Agent': KENNUNG})
        with urllib.request.urlopen(anfrage, timeout=zeitlimit) as antwort:
            rohtext = antwort.read().decode('utf-8')
        roh = json.loads(rohtext)
    except Exception as ausnahme:
        fehler.merken('uex.holen.' + stelle, ausnahme)
        return None
    liste = roh.get('data')
    if liste is None:
        return []
    # ⚠⚠ **HTML-Zeichen aus den Daten holen.** UEX liefert Apostrophe als
    # `&apos;` — im Werkzeug stand deshalb „Grey&apos;s Market" statt „Grey's
    # Market" (gemeldet 05.09.2026). Das betrifft jeden Namen aus der Quelle,
    # nicht nur Hersteller: Terminals, Orte, Waren, Teile.
    #
    # Deshalb hier zentral und nicht in fünf Modulen einzeln — sonst taucht
    # derselbe Fehler beim nächsten neuen Feld wieder auf.
    #
    # ⚠ Der Vortest auf `&` spart den Durchlauf bei den allermeisten Antworten:
    # Wo kein `&` im Rohtext steht, gibt es auch nichts zu entschlüsseln.
    if '&' in rohtext:
        liste = _entschluesseln(liste)
    # ⚠ Siehe `DECKEL` oben: Abgeschnitten wird still. Wer es nicht merkt,
    # rechnet mit einem Bruchstück weiter und hält es für das Ganze.
    #
    # ⚠⚠ **`==`, nicht `>=`** — und der Unterschied ist der ganze Sinn der
    # Prüfung. Mit `>=` schlug sie bei **jeder vollständigen** Antwort an, die
    # zufällig groß ist: `terminals` liefert 826 Zeilen, `commodities_prices_all`
    # 2.593 — beides ungekürzt, beides täglich als Fehler ins Protokoll.
    #
    # Am 04.09.2026 im Fehlerbericht aufgefallen. Der Schaden ist nicht die
    # falsche Zeile, sondern die Gewöhnung: Ein Protokoll, in dem jeden Tag
    # zwei erfundene Fehler stehen, liest bald niemand mehr — und der echte
    # geht darin unter.
    #
    # Abgeschnitten ist eine Antwort **genau dann**, wenn sie exakt auf dem
    # Deckel sitzt. Alles darüber beweist, dass es für diese Abfrage keinen
    # gibt.
    if isinstance(liste, list) and len(liste) == DECKEL:
        fehler.merken(
            'uex.holen.' + stelle,
            RuntimeError('Antwort bei %d Zeilen — vermutlich abgeschnitten, '
                         'Abruf enger zuschneiden: %s' % (len(liste), adresse)))
    return liste


class Ablage:
    """Eine abgelegte UEX-Liste auf der Platte, mit Alter und Formatstand.

    Ein Fachmodul legt sich davon **eine** an und behält sie als Modulvariable.
    """

    def __init__(self, dateiname, format_nr, haltbar, stempeln=True,
                 patch_bindet=False):
        self.dateiname = dateiname
        self.format_nr = format_nr
        self.haltbar = haltbar
        # ⚠ `stempeln=False` nur für die Ablage des Spielstands selbst — sie
        # würde sich sonst mit ihrem eigenen alten Wert stempeln.
        self.stempeln = stempeln
        # ⭐⭐ **`patch_bindet=True`: Der Patch entscheidet, nicht die Uhr.**
        #
        # Ladenpreise, Schiffspreise und der Warengruppen-Katalog ändern sich
        # mit einem Spiel-Patch, nicht mit der Tageszeit — anders als die
        # Warenpreise im Handel, die täglich schwanken. Eine Wochenfrist warf
        # deshalb jede Woche einen noch gültigen Stand weg und kostete den
        # Spieler eine Minute Wartezeit für nichts.
        #
        # Mit dieser Bindung bleibt die Ablage stehen, bis CIG wirklich etwas
        # geändert hat; `haltbar` ist dann nur noch die Notfrist für den Fall,
        # dass sich die Spielversion gar nicht ermitteln lässt.
        self.patch_bindet = patch_bindet
        # Zuletzt gelesener Inhalt, damit nicht bei jedem Zugriff die Datei
        # neu geparst wird. Der Schlüssel ist (Änderungszeit, Größe): Ändert
        # eine andere Stelle die Datei, fällt das auf und es wird neu gelesen.
        self._gemerkt = {'stand': None, 'daten': None}

    def pfad(self):
        return pfade.app_datei(self.dateiname)

    def laden(self):
        """Der abgelegte Stand — oder `{}`, wenn keiner (brauchbar) da ist.

        Ein anderer Formatstand gilt als „nicht da": Lieber einmal neu holen
        als eine alte Struktur falsch deuten.
        """
        pfad = self.pfad()
        try:
            st = os.stat(pfad)
            kennung = (st.st_mtime_ns, st.st_size)
        except OSError:
            return {}
        if self._gemerkt['stand'] == kennung:
            return self._gemerkt['daten']
        try:
            with open(pfad, encoding='utf-8') as f:
                daten = json.load(f)
            if daten.get('format') == self.format_nr:
                self._gemerkt['stand'] = kennung
                self._gemerkt['daten'] = daten
                return daten
        except Exception:
            pass
        return {}

    def alter(self):
        """Wie alt die Ablage ist, in Sekunden — oder `None`, wenn keine da ist."""
        geholt = (self.laden() or {}).get('geholt')
        try:
            return (time.time() - float(geholt)) if geholt else None
        except (TypeError, ValueError):
            return None

    def veraltet(self):
        """Muss neu geholt werden? Ohne Ablage: ja.

        ⚠ Bei `patch_bindet=True` zählt zusätzlich der Spielstand: Ein neuer
        Patch macht die Ablage sofort ungültig, ein gleichbleibender hält sie
        bis zur Notfrist am Leben.
        """
        a = self.alter()
        if a is None or a >= self.haltbar:
            return True
        if self.patch_bindet:
            # ⚠ Lokal importiert: `spielstand` benutzt selbst eine `Ablage`.
            try:
                from . import spielstand
                ja, _damals, _jetzt = spielstand.ueberholt(self)
                return bool(ja)
            except Exception:
                # Im Zweifel gilt der abgelegte Stand — siehe `ueberholt`.
                return False
        return False

    def sichern(self, felder, kompakt=False):
        """Die eigenen Felder ablegen; `format` und `geholt` kommen von hier.

        ⚠ Geschrieben wird über eine `.tmp` und `os.replace()` — **atomar**.
        Bricht der Vorgang ab, liegt entweder der alte oder der neue Stand da,
        nie eine halbe Datei. Genau die wäre beim nächsten Start eine kaputte
        Ablage, die niemand einem abgebrochenen Schreibvorgang zuordnet.
        """
        daten = dict(felder)
        daten['format'] = self.format_nr
        daten['geholt'] = time.time()
        if self.stempeln:
            # ⚠⚠ **Unter welchem Spielstand wurde das geholt?** Ohne diese
            # Zeile ist eine Ablage einen Tag lang „frisch" — auch wenn
            # zwischendurch ein Patch die halben Preise umgeworfen hat. Der
            # Zeitstempel allein sagt nichts darüber, ob die Zahlen noch
            # gelten. Siehe `scbp/spielstand.py`.
            #
            # Lokal importiert: `spielstand` hängt seinerseits an diesem Modul.
            from . import spielstand
            stand = spielstand.live()
            if stand:
                daten['spielstand'] = stand
        ziel = self.pfad()
        trenner = (',', ':') if kompakt else None
        # ⚠⚠ **Ein eigener Zwischenname je Schreibvorgang.** Bis zum
        # 06.09.2026 hieß die Datei fest `<ziel>.tmp` — schreiben zwei Fäden
        # gleichzeitig dieselbe Ablage (etwa zwei Abrufe der Steckplatzdaten),
        # legen beide dieselbe `.tmp` an, der erste benennt sie um, und dem
        # zweiten fehlt sie: `No such file or directory: …tmp -> …json`.
        #
        # Der Schreibvorgang war also atomar, aber nicht nebenläufig-sicher.
        # Mit Prozess- und Faden-Nummer im Namen stören sie sich nicht mehr;
        # das abschließende `os.replace` bleibt atomar wie zuvor.
        zwischen = '%s.%d.%d.tmp' % (ziel, os.getpid(),
                                     threading.get_ident() % 100000)
        try:
            os.makedirs(os.path.dirname(ziel), exist_ok=True)
            with open(zwischen, 'w', encoding='utf-8') as f:
                if trenner:
                    json.dump(daten, f, ensure_ascii=False, separators=trenner)
                else:
                    json.dump(daten, f, ensure_ascii=False)
            os.replace(zwischen, ziel)
            self._gemerkt['stand'] = None
            return True
        except Exception as ausnahme:
            fehler.merken('uex.sichern.' + self.dateiname, ausnahme)
            # ⚠ Die halbe Datei nicht liegen lassen — sie hieße sonst für immer
            # `…12345.tmp` im Ablageordner des Nutzers.
            try:
                os.remove(zwischen)
            except OSError:
                pass
            return False

    def vergessen(self):
        """Den gemerkten Inhalt verwerfen — die Datei bleibt liegen."""
        self._gemerkt['stand'] = None
        self._gemerkt['daten'] = None
