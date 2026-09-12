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
Angaben am Gegenstand — Klasse, Größe, Güte, Lenkung.

Visiert man im Spiel etwas mit dem Traktorstrahl an, steht dort nur der **Name**.
Ob der Kühler militärisch oder zivil ist, welche Größe er hat und welchen
Gütegrad — das steht in der Beschreibung, die man erst aufklappen muss. Im
Gefecht klappt niemand etwas auf.

Dabei liegt die Angabe direkt daneben. Dieselbe Datei, derselbe Schlüssel, nur
`Desc` statt `Name`::

    item_DescAMRS_LaserCannon_S1 = … Größe: S1 · Gütegrad: A · Klasse: Energie (Laser)
    item_NameAMRS_LaserCannon_S1 = Omnisky III Cannon

Dieses Modul liest die Beschreibungen und baut daraus das Kürzel, das an den
Namen gehängt wird — `Omnisky III Cannon (Las/1/A)`.

**Die Daten stammen von CIG**, nicht von uns und nicht aus einem fremden
Werkzeug: Sie stehen in der Datei, die jeder Spieler ohnehin auf der Platte hat.
Wir schreiben sie nur dorthin um, wo man sie im Gefecht auch sieht.

Zwei Formate, weil die sinnvolle Angabe von der Art abhängt
-----------------------------------------------------------

============ ======================== ==============================
Art          Format                   Beispiel
============ ======================== ==============================
Komponenten, ``(Klasse/Größe/Güte)``  ``Inspire Advanced (Ind/2/C)``
Waffen       ``(Klasse/Größe/Güte)``  ``Omnisky III Cannon (Las/1/A)``
Raketen      ``(Lenkung+Größe)``      ``'Arrow' I Missile (IR1)``
============ ======================== ==============================

Raketen haben **keine** Civ/Mil-Klasse — dort zählt der Suchkopf. der Autor am
27.08.2026: „Raketen sind auch nicht civ oder mil, die info kann bei raketen
also weg, dafür brauchen wir em z.b. … im Kampf ist das entscheidend."

⚠ Die Übersetzung ist uneinheitlich
------------------------------------

Dieselbe Klasse steht in bis zu drei Schreibweisen in derselben Datei:
„Civilian (Zivil)" 169-mal, „Zivil" 10-mal, „Civilian" 3-mal. Deshalb wird nicht
auf ganze Wörter verglichen, sondern auf **Wortteile** — und zwar auf beide
Sprachen gleichzeitig. Eine Schreibweise, die es in der eigenen Sprache nicht
gibt, kann keinen Fehltreffer erzeugen.

Wann es einen Zusatz gibt — und wann nicht
------------------------------------------

Gemessen an der echten Datei (27.08.2026) bekommen **856** Gegenstände einen:

=================== ===== =========================================
Fall                Zahl  Beispiel
=================== ===== =========================================
Klasse+Größe+Güte     450  ``Khaos (Civ/2/C)``
Waffenklasse allein   344  ``P4-AR "Warhawk" Rifle (Bal)``
Rakete                 62  ``'Arrow' I Missile (IR1)``
=================== ===== =========================================

**Bei Waffen genügt die Klasse für sich.** FPS-Waffen haben in Star Citizen
weder Größe noch Gütegrad — 342 von ihnen tragen nur dieses eine Feld. Eine
starre Zwei-Felder-Regel hätte ausgerechnet sie ausgeschlossen, und ob eine
Waffe ballistisch oder Laser ist, entscheidet im Gefecht mehr als ihre Größe.

**Sonst braucht es mindestens zwei der drei Felder.** 726 Gegenstände tragen nur
eine Größe — und die steht dort meist schon im Namen („Eclipse **20xS3**
Bombengestell"). Ein `(–/3/–)` daneben wäre Lärm, kein Wert.
"""
import re


# --------------------------------------------------------------- Feldnamen
#
# Die deutschen sind **gemessen** an der Datei des SC-Deutsch-Launchers
# (27.08.2026, 4957 Schlüssel mit Name und Beschreibung). Die englischen sind
# Kandidaten: CIGs Originaldatei steckt in der `Data.p4k` und lag zum Zeitpunkt
# des Baus nicht vor. Es werden alle gleichzeitig gesucht — ein Feldname, den es
# in der eigenen Sprache nicht gibt, kostet nichts.
FELDER = {
    'groesse': ('Größe', 'Grösse', 'Size'),
    'guete':   ('Gütegrad', 'Guetegrad', 'Grade'),
    'klasse':  ('Klasse', 'Class'),
    'lenkung': ('Verfolgungssignal', 'Tracking Signal', 'Signature Type',
                'Signature'),
}

# Wortteile → Kürzel. Kleingeschrieben verglichen, deshalb hier auch klein.
# Die ersten fünf sind **CIGs eigene** Kürzel: Star Citizen schreibt sie selbst
# so in die Game.log (`7CA 'Nargun' (Civ/3/A)`), und `logsource.py` zerlegt sie
# seit Langem. Die übrigen folgen demselben Muster.
KLASSEN = (
    (('civilian', 'zivil'),                        'Civ'),
    (('military', 'militär', 'militaer'),          'Mil'),
    (('industrial', 'industrie'),                  'Ind'),
    (('stealth', 'tarnung'),                       'Sth'),
    (('competition', 'wettkampf', 'wettbewerb'),   'Cmp'),
    # Waffen — hier ist nicht die Fraktion gefragt, sondern die Wirkung.
    (('laser',),                                   'Las'),
    (('elektron', 'electron'),                     'Ele'),
    (('plasma',),                                  'Pla'),
    (('distortion', 'verzerrung'),                 'Dis'),
    (('microwave', 'mikrowelle'),                  'Mic'),
    (('ballist',),                                 'Bal'),
    (('melee', 'nahkampf'),                        'Nah'),
    (('mining', 'bergbau'),                        'Min'),
    (('salvage', 'verwertung'),                    'Slv'),
    (('medical', 'medizin'),                       'Med'),
)

# Welche Kürzel eine Waffenart bezeichnen (im Gegensatz zur Fraktion). Bei
# diesen genügt die Klasse allein für einen Zusatz — siehe `aus_beschreibung()`.
WAFFENKLASSEN = frozenset(
    ('Las', 'Ele', 'Pla', 'Dis', 'Mic', 'Bal', 'Nah', 'Min', 'Slv', 'Med'))

# Suchkopf einer Rakete. `CS` = Cross Section (Querschnitt) — der Radarquerschnitt.
LENKUNGEN = (
    (('infrarot', 'infrared'),                     'IR'),
    (('elektromagnet', 'electromagnet'),           'EM'),
    (('querschnitt', 'cross'),                     'CS'),
)

# Ein bereits vorhandener Zusatz in Klammern am Namensende. Zwei Formen: unsere
# eigene (`(Mil/3/A)`, auch die des SC Deutsch Launchers) und die der Raketen
# (`(IR1)`). Wird vor dem Anhängen entfernt — sonst stünde nach einem zweiten
# Lauf `Spark I-G Missile (CS1) (CS1)` da.
VORHANDEN_RE = re.compile(
    r'\s*\((?:[A-Za-z]{2,4}|–|-)/(?:\d{1,2}|–|-)/(?:[A-Z]|–|-)\)\s*$'
    r'|\s*\((?:IR|EM|CS)\d{1,2}\)\s*$')


def _feld(text, name):
    """Den Wert eines Feldes aus einer Beschreibung ziehen — oder None.

    Die Zeilen einer Beschreibung sind in der `global.ini` nicht getrennt,
    sondern durch die zwei Zeichen `\\n` aneinandergehängt. Deshalb wird bis
    dorthin gelesen, nicht bis zum Zeilenende."""
    for schreibweise in FELDER[name]:
        treffer = re.search(re.escape(schreibweise) + r':\s*([^\\]+?)(?:\\n|$)',
                            text)
        if treffer:
            wert = treffer.group(1).strip()
            if wert:
                return wert
    return None


def _kuerzel(wert, tabelle):
    """Wortteil-Vergleich gegen eine der Tabellen oben. None, wenn nichts passt."""
    if not wert:
        return None
    klein = wert.lower()
    for teile, kurz in tabelle:
        if any(teil in klein for teil in teile):
            return kurz
    return None


def _zahl(groesse):
    """`'S3'` → `'3'`. Alles, was keine schlichte Größe ist, fällt weg.

    In der Datei stehen auch Werte wie `'S2 (Nur Fahrzeuge)'` oder `'Large'` —
    die gehören nicht in ein Kürzel, das drei Zeichen breit sein soll."""
    if not groesse:
        return None
    treffer = re.fullmatch(r'S(\d{1,2})', groesse.strip())
    return treffer.group(1) if treffer else None


def _guete(wert):
    """`'A'` … `'D'` — sonst None.

    ⚠ Es steht nicht immer ein Buchstabe da. Gemessen an der echten Datei:
    sechsmal `'Individuell angefertigt'` und dreimal `'N/A'`. Wer einfach den
    ersten Buchstaben nimmt, schreibt `(Ind/4/I)` und `(Civ/1/N)` in den Namen —
    Gütegrade, die es nicht gibt."""
    if not wert:
        return None
    kurz = wert.strip().upper()
    return kurz if kurz in ('A', 'B', 'C', 'D') else None


def aus_beschreibung(text):
    """Das Kürzel für einen Gegenstand — oder None, wenn es sich nicht lohnt.

    Rückgabe ist der fertige Zusatz **ohne** führendes Leerzeichen, also
    `'(Mil/3/A)'` oder `'(IR1)'`."""
    if not text:
        return None

    # Raketen zuerst: Sie haben einen Suchkopf und keine Fraktions-Klasse.
    lenkung = _kuerzel(_feld(text, 'lenkung'), LENKUNGEN)
    if lenkung:
        groesse = _zahl(_feld(text, 'groesse'))
        return '(%s%s)' % (lenkung, groesse) if groesse else '(%s)' % lenkung

    klasse = _kuerzel(_feld(text, 'klasse'), KLASSEN)
    groesse = _zahl(_feld(text, 'groesse'))
    guete = _guete(_feld(text, 'guete'))

    # Bei einer Waffe ist die Klasse für sich schon die Auskunft: ballistisch
    # oder Laser entscheidet, ob sie gegen Schilde etwas ausrichtet. Gemessen
    # haben **342** Waffen nur dieses eine Feld — Größe und Gütegrad gibt es bei
    # FPS-Waffen gar nicht. Eine starre Zwei-Felder-Regel hätte ausgerechnet die
    # ausgeschlossen, um die es ging.
    if klasse in WAFFENKLASSEN and not (groesse or guete):
        return '(%s)' % klasse

    # Sonst: mindestens zwei der drei. Ein `(–/3/–)` wäre Lärm statt Angabe —
    # und die Größe steht bei Gestellen und Türmen ohnehin schon im Namen.
    if sum(1 for wert in (klasse, groesse, guete) if wert) < 2:
        return None
    return '(%s/%s/%s)' % (klasse or '–', groesse or '–', guete or '–')


def zusatz_entfernen(name):
    """Einen früher angehängten Zusatz wieder abschneiden.

    Fängt auch den des SC Deutsch Launchers (`(CS1)`, `(Mil/3/A)`) — wer von
    dort kommt, soll keinen doppelten Klammerausdruck im Namen haben."""
    return VORHANDEN_RE.sub('', name) if name else name


def tabelle_bauen(zeilen):
    """Aus den Zeilen einer `global.ini` die Tabelle *Namensschlüssel → Kürzel*.

    Erwartet die Zeilen roh, wie sie in der Datei stehen. Gearbeitet wird über
    den **Schlüsselstamm**: `item_DescAMRS_LaserCannon_S1` und
    `item_NameAMRS_LaserCannon_S1` teilen sich alles nach dem Wortanfang.
    Deshalb braucht es keine Namenssuche und keinen Katalog — die Zuordnung ist
    eindeutig."""
    beschreibungen = {}
    namen_stamm = {}
    for zeile in zeilen:
        trenner = zeile.find('=')
        if trenner < 1:
            continue
        kopf = zeile[:trenner]
        schluessel = kopf.split(',', 1)[0]
        klein = schluessel.lower()
        if klein.startswith('item_desc'):
            beschreibungen[klein[9:]] = zeile[trenner + 1:]
        elif klein.startswith('item_name'):
            namen_stamm[klein[9:]] = schluessel

    tabelle = {}
    for stamm, schluessel in namen_stamm.items():
        text = beschreibungen.get(stamm)
        if not text:
            continue
        kuerzel = aus_beschreibung(text)
        if kuerzel:
            tabelle[schluessel] = kuerzel
    return tabelle
