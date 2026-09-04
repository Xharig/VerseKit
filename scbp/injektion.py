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
Bauplan-Angaben in die Texte des Spiels schreiben.

An jede Mission, die Baupläne ausschüttet, kommt die Liste dessen, was sie
geben kann — mit einem **Kästchen** davor: angehakt, was man schon hat, leer,
was fehlt. Dazu ein Kürzel im Missionstitel, damit man es schon in der
Auftragsliste sieht, ohne jede Mission aufzuklappen.

Das Vorbild ist der SC Deutsch Launcher, der genau das seit Langem macht
(`bp_contractInfo`, `bp_erledigt`). Zwei Gründe, es hier trotzdem zu haben:
Er läuft nur unter Windows und nur auf Deutsch — unter Linux gibt es ihn
schlicht nicht, und englische Clients bekommen von ihm gar nichts.

**Womit gearbeitet wird**

  * die `global.ini` des Spielers — gleich welcher Herkunft: die deutsche
    Übersetzung, StarStrings oder das Original aus dem `Data.p4k`
  * `katalog-cache.json` → welche Mission welche Baupläne gibt
  * `bestand.json` → was man selbst schon hat

Angehängt wird am **Textschlüssel** (`titleLocKey`, `descriptionLocKey`), und
der ist in jeder Sprache derselbe. Dieselbe Injektion greift deshalb für
Deutsch, Englisch und die neun weiteren Sprachen im Spiel.

**Zeilenformat der global.ini**

    SCHLUESSEL=Text mit \\n als Zeilenumbruch
    SCHLUESSEL,P=Text            (Zweitfassung, wird genauso behandelt)

Der Umbruch ist die **Zeichenfolge** `\\n`, kein echter Zeilenumbruch — eine
Zeile der Datei ist immer ein Eintrag. Wer hier ein echtes Newline einfügt,
zerreißt die Datei.

**Wiederholbar und rückgängig**

Alles Eingefügte steht zwischen zwei Marken. Vor dem Schreiben wird zuerst
alles zwischen den Marken entfernt — dadurch kann man beliebig oft injizieren,
ohne dass sich die Angaben stapeln, und `entfernen()` stellt den Ursprungstext
wieder her, ohne die Datei neu laden zu müssen.
"""
import json
import os
import re
import time
import urllib.request

from . import angaben as angaben_modul
from . import fehler, bestand as bestand_datei
from . import katalog as katalog_modul
from . import pfade
from .sprache import t

# ---------------------------------------------------------------------------
# Zweite, bessere Datenquelle: das SCDL-Team veröffentlicht seine aufbereiteten
# Vertragsdaten offen im Übersetzungs-Repo — **813 Verträge** mit fertigen
# Texten, deutsch und englisch, samt Angaben, die scmdb so nicht hat (Region,
# Gefahrenstufe, Wartezeit in Worten). Aus scmdb allein kämen 349 zusammen.
#
# Die Arbeitsteilung, die sich daraus ergibt, ist die sinnvolle: Das SCDL-Team
# pflegt, was es ohnehin pflegt. Dieses Werkzeug steuert das bei, was nur es
# kann — das **Kästchen**, also den Abgleich mit dem eigenen Bauplan-Bestand.
# In den Rohdaten stehen die Baupläne neutral als „    - Name".
#
# Lizenz CC-BY-NC-SA-4.0: geholt wird zur Laufzeit von der Original-Adresse,
# nichts davon liegt in diesem Repo. Die Herkunft wird im eingefügten Text
# genannt.
SCDL_ROH = ('https://raw.githubusercontent.com/rjcncpt/'
            'StarCitizen-Deutsch-INI/master/blueprints/Data/%s')
SCDL_DATEI = {'de': 'bp-contracts_short.json',
              'en': 'bp-contracts_short_en.json'}
SCDL_CACHE = 'bp-contracts-%s.json'
BP_ZEILE = re.compile(r'^(\s*)- (.+)$')

# ⚠ Eine Listenzeile ist **nicht** automatisch ein Bauplan. Die Blöcke des
# SCDL-Teams gliedern mit `#`-Überschriften, und unter dreien davon stehen
# Listen. Gezählt an den echten Vertragsdaten vom 29.08.2026, in beiden
# Sprachen gleich:
#
#     # Baupläne / # Blueprints     4379 Zeilen  <- Baupläne
#     # Abgabe   / # Delivery        323 Zeilen  <- Abgabeorte
#     # Region   / # Region          239 Zeilen  <- Regionen
#     # Abgabe für … aUEC Mission    ~50 Zeilen  <- Abgabeorte je Preisstufe
#
# Bis zum 29.08.2026 bekam **jede** davon ein Kästchen. Im Spiel stand dann
# `[  ] Stanton-System - Gefahr 4-6/10`, als könnte man eine Region besitzen —
# rund 620 falsche Kästchen. Angekreuzt wird deshalb nur, was unter der
# Bauplan-Überschrift steht.
#
# Ohne `#`-Überschrift steht keine einzige Listenzeile (nachgezählt: 0), der
# Zustand ist also immer bekannt.
UEBERSCHRIFT_ZEILE = re.compile(r'^\s*#')
BP_UEBERSCHRIFT = re.compile(r'^\s*#\s*(?:Baupläne|Blueprints)', re.I)

# Die Marken. Bewusst unauffällig und ohne Sonderzeichen, damit sie das Spiel
# nicht stören, aber eindeutig genug, um sie sicher wiederzufinden.
AUF = '[SCBPW]'
ZU = '[/SCBPW]'
MARKE = re.compile(re.escape(AUF) + '.*?' + re.escape(ZU))

# Wie eine Einfügung **ohne** Marke aussieht — der Notnagel beim Entfernen.
#
# Beide Formen sind eindeutig genug: Der Titelzusatz ist ` <EM4>[BP 3/6]</EM4>`,
# der Textblock beginnt mit einer Zeile aus lauter Bindestrichen. So etwas steht
# in keinem Text von CIG. Gesucht wird ab der **letzten** Fundstelle, damit ein
# doppelt eingetragener Block ganz verschwindet und nicht nur zur Hälfte.
# ⚠ Das `!?` muss mit: Seit dem Rufzeichen für eingeschränkte Aufträge heißt
# der Zusatz auch `[BP 0/19!]`. Ohne das bliebe er beim Zurücksetzen stehen —
# und zwar genau bei den 332 Aufträgen, die eine Einschränkung haben.
# ⚠ **Beide Formen**: `[BP 3/12]` von vor dem 28.08.2026 und das heutige
# `[BP]`. Wer schon einmal injiziert hat, trägt die alte in seiner Datei —
# ohne sie hier bliebe sie beim Zurücksetzen für immer stehen.
#
# ⚠ Und genau deshalb greift dieser Notnagel **nur bei einer Datei, in der wir
# schon einmal geschrieben haben** (`ist_frisch()`): Das heutige blanke `[BP]`
# ist nicht unser Alleinstellungsmerkmal — MrKraken schreibt in StarStrings
# dasselbe. Als eigener Nachweis dient `EIGENER_NACHWEIS` weiter unten.

# ⚠ Beim ersten Anlauf stand hier „ab der Bindestrich-Linie alles weg". Das ging
# schief: CIG benutzt solche Linien **selbst** als Gliederung. Im Test auf einer
# Kopie verlor `Battaglia_RPT_BoardShip_01_desc` dadurch 589 seiner 870 Zeichen —
# der ganze Abschnitt „GENEHMIGUNG: Battaglia, Recco" wäre stillschweigend
# verschwunden. Genau die Sorte Schaden, die niemand bemerkt, bis der Text im
# Spiel fehlt.
#
# Geschnitten wird deshalb nur, wenn nach der Linie auch eine **unserer**
# Überschriften steht — die eigene und die der SCDL-Vertragsdaten, je zweisprachig.
#
# ⚠ Es sind **vier** Formen, nicht zwei. Die Vertragsdaten kennen neben der
# Bauplan-Liste noch einen zweiten Blocktyp: den Hinweis „Dieser Missionstyp wird
# vom Spiel dynamisch erzeugt" (84 der 363 Blöcke). Im Test ohne Merkdatei blieben
# genau diese 90 Zeilen halb stehen — der Anfang war weg, der Rest stand noch da.
# Gezählt, nicht geraten: 279 + 84 je Sprache.
_UEBERSCHRIFTEN = (
    'BAUPLÄNE AUS DIESEM AUFTRAG', 'BLUEPRINTS FROM THIS CONTRACT',
    'MÖGLICHE BAUPLÄNE FÜR DIESEN MISSIONSTYP',
    'POSSIBLE BLUEPRINTS FOR THIS MISSION TYPE',
    '<EM4>Dieser Missionstyp wird vom Spiel dynamisch erzeugt',
    '<EM4>This mission type is dynamically generated by the game',
)
# Woran eine Einfügung **zweifelsfrei als unsere** zu erkennen ist. Gebraucht
# von `ist_drin()`.
#
# ⚠ Der blanke Titelzusatz `<EM4>[BP]</EM4>` taugt dafür **nicht** — MrKraken
# schreibt in StarStrings genau denselben. Vor dem 29.08.2026 stand er hier, und
# damit meldete der Watcher „steht schon drin", sobald jemand StarStrings frisch
# eingesetzt hatte, ohne dass je etwas eingetragen worden wäre.
#
# Sicher sind: die alte Marke, unsere Block-Überschriften (in keiner der beiden
# Fremdquellen enthalten — am 29.08.2026 in beiden Fassungen nachgezählt: 0) und
# die zählende bzw. rufende Titelform, die es nur bei uns gibt.
ZAEHLENDER_TITEL = re.compile(r'<EM4>\[(?:BP|Bauplan)(?:\s+\d+/\d+|!)\]</EM4>')

# Eine Bauplan-Marke am Titel — **von wem auch immer**. Deckt die eigene Form
# `[BP]`/`[BP!]`, die alte `[BP 3/12]`, MrKrakens kombinierte
# `<EM4>[10 Rep] [BP]</EM4>` und die des SC Deutsch Launchers ab.
TITELMARKE = re.compile(r'<EM4>[^<>]*\[(?:BP|Bauplan)[^\]]*\][^<>]*</EM4>')

EIGENER_NACHWEIS = re.compile(
    '|'.join(re.escape(u) for u in _UEBERSCHRIFTEN))

# Derselbe Notnagel **ohne** den Titelzusatz — für Grundlagen, die die Titel
# selbst kennzeichnen (StarStrings). Dort setzt der Watcher gar keinen
# Titelzusatz, also gibt es dort auch keinen von ihm zu entfernen; was am Titel
# steht, gehört MrKraken. Der Block darunter dagegen ist unserer und muss weg.
OHNE_MARKE_BLOCK = re.compile(
    # ⚠ `<EM\d>` wie bei OHNE_MARKE — siehe die Begründung dort.
    r'(?:\\n){1,2}?\s*-{20,}(?:\\n|\s|<EM\d>)*(?:%s).*$'
    % '|'.join(re.escape(u) for u in _UEBERSCHRIFTEN), re.S)

OHNE_MARKE = re.compile(
    r'(?:\s*<EM4>\[(?:BP|Bauplan)(?:\s+\d+/\d+)?!?\]</EM4>\s*$'
    # ⚠ Höchstens **zwei** Umbrüche vor der Linie schlucken, nicht beliebig viele.
    # So viele bringt unser Block selbst mit; alles darüber gehört zu CIGs Text.
    # Mit `*` fehlten am Ende zweier Aufträge je zwei Zeichen — winzig, aber es ist
    # fremder Text, den wir nicht anfassen dürfen.
    # ⚠ Bekannte Ungenauigkeit, gemessen an der echten Datei: Bei **2 von 743**
    # Aufträgen bleibt am Ende ein Umbruch zu wenig stehen, weil CIGs Text selbst
    # mit einem endet und unserer mit einem beginnt — auseinanderhalten lassen die
    # sich nicht. Das betrifft nur diesen Notnagel; der reguläre Weg über die
    # Merkdatei stellt den Wortlaut **auf das Zeichen genau** wieder her (geprüft).
    # Zwei fehlende Umbrüche in zwei Auftragstexten sind der Preis dafür, dass
    # Aufräumen auch ohne Merkdatei funktioniert.
    # ⚠ `<EM\d>` muss mit: Unser Block schreibt die Überschrift als
    # `<EM4>MÖGLICHE BAUPLÄNE …</EM4>`, also steht das Tag ZWISCHEN Linie und
    # Überschrift. Ohne diese Alternative greift der Notnagel am eigenen Block
    # gar nicht — gemessen am 02.09.2026, und zwar seit jeher. Folge: Wem die
    # Merkdatei fehlt (anderer Rechner, aufgeräumt), der bekam den Block beim
    # Zurücksetzen nicht mehr aus seiner `global.ini` heraus. Gefunden erst,
    # als Prüfung 102 den Notnagel zum ersten Mal ohne Merkdatei ansprach.
    r'|(?:\\n){1,2}?\s*-{20,}(?:\\n|\s|<EM\d>)*(?:%s).*$)'
    % '|'.join(re.escape(u) for u in _UEBERSCHRIFTEN), re.S)

# Aufbau nach dem Vorbild des SC Deutsch Launchers — die **Gliederung** ist die
# nützliche Erkenntnis (was ein Spieler vor dem Annehmen wissen will), die
# Formulierungen sind eigene. Alle Angaben stammen aus scmdb.
TEXTE = {
    'de': {
        'kurz':      'BP',
        # ⚠ **„Missionstyp", nicht „dieser Auftrag" (28.08.2026).** Hier stand
        # „BAUPLÄNE AUS DIESEM AUFTRAG" — und das versprach mehr, als die Daten
        # hergeben: Die Liste führt alle Preisstufen zusammen, weil sich 123 von
        # 353 Aufträgen den Textschlüssel über ihre Stufen hinweg teilen.
        # Morkhan las die Überschrift wörtlich, nahm den Auftrag an und bekam
        # nichts — „is trotzdem verwirrend, egal wie man's dreht." Die
        # Verwirrung saß in der Überschrift, nicht in der Liste.
        #
        # Der SC Deutsch Launcher schreibt aus demselben Grund „MÖGLICHE
        # BAUPLÄNE FÜR DIESEN MISSIONSTYP" (367 mal in seiner Datei). Eine
        # Überschrift, die nichts verspricht, was sie nicht halten kann.
        'ueberschr': 'MÖGLICHE BAUPLÄNE FÜR DIESEN MISSIONSTYP',
        'chance':    'Chance auf Bauplan',
        'rep_min':   'Min. Reputation',
        'rep_max':   'Max. Reputation',
        'lohn':      'Belohnung',
        'ruf':       'Rufpunkte',
        'cooldown':  'Wartezeit',
        'minuten':   'Minuten',
        'teilbar':   'Mission teilbar',
        'ja':        'Ja', 'nein': 'Nein',
        'liste':     'Baupläne — angehakt ist, was du hast',
        'ab_rang':   'erst ab',
        'leere_stufen': ('Achtung: %d der %d Stufen dieses Auftrags geben '
                         'gar keine Baupläne.'),
        'quelle':    'Angaben von scmdb.net · eingefügt vom SC BP Watcher',
        'trenner':   '.',
    },
    'en': {
        'kurz':      'BP',
        'ueberschr': 'POSSIBLE BLUEPRINTS FOR THIS MISSION TYPE',
        'chance':    'Blueprint chance',
        'rep_min':   'Min. reputation',
        'rep_max':   'Max. reputation',
        'lohn':      'Payout',
        'ruf':       'Reputation gain',
        'cooldown':  'Cooldown',
        'minuten':   'minutes',
        'teilbar':   'Shareable',
        'ja':        'Yes', 'nein': 'No',
        'liste':     'Blueprints — ticked means you have it',
        'ab_rang':   'needs',
        'leere_stufen': ('Note: %d of the %d tiers of this contract give no '
                         'blueprints at all.'),
        'quelle':    'Data from scmdb.net · added by SC BP Watcher',
        'trenner':   ',',
    },
}

# Kästchen wie beim Launcher: leer, wenn der Bauplan fehlt — hervorgehoben,
# wenn man ihn hat. Das Auge findet dadurch sofort, was noch offen ist.
# ---------------------------------------------------------------------------
# Wo ein **fremder** Anhang beginnt — damit unserer davor landet, nicht dahinter
# ---------------------------------------------------------------------------
#
# ⚠ Warum es das gibt (gemessen 02.09.2026, `tools/smartcitizen_pruefen.py`):
# Smart Citizen (Osiris-DevWorks) hängt eigene Blöcke an dieselben
# Beschreibungen und räumt vor jedem Lauf seinen alten Block ab — indem es den
# **ersten** eigenen Marker sucht und ab dort ALLES wegwirft:
#
#     for marker in ("\\n\\n--- STATS ---", "\\n\\n<EM3>MISSION DETAILS</EM3>", …):
#         if marker in existing_value:
#             existing_value = existing_value[:existing_value.index(marker)]
#
# Auf ihrer Seite ist das richtig. Nur stand unser Block dahinter — und war
# damit bei jedem ihrer Läufe still verschwunden: **398 von 398** gemeinsamen
# Einträgen. Der Nutzer merkt nur, dass „die Baupläne weg sind".
#
# ⚠ **Ein Muster, keine Namensliste.** Eine Liste ihrer Marker wäre beim ersten
# Umbenennen tot, ohne dass es auffällt. Erkannt wird deshalb die **Form**, in
# der solche Blöcke überall geschrieben werden: eine Leerzeile, dann eine
# Überschrift zwischen Bindestrichen, Gleichheitszeichen oder `<EMn>`-Klammern.
# Das deckt alle sieben heutigen Marken Smart Citizens ab und überlebt eine
# Umbenennung innerhalb derselben Form.
#
# ⚠ Und es bleibt eine Annahme über fremden Code. Deshalb ist die dritte
# Sicherung Pflicht: `tools/smartcitizen_pruefen.py` lädt deren Generator und
# schlägt fehl, wenn sich die Form ändert. Die Prüfung gehört vor jedes Release.
#
# Absichtlich NICHT erfasst: unsere eigene Linie (57 Bindestriche, dahinter
# `<EM4>`) und die des SC Deutsch Launchers — die werden schon von
# `_fremdblock_trennen` behandelt. `{3,20}` schließt sie aus.
FREMDER_ANHANG = re.compile(
    r'(?:\\n\s*){2}(?:'
    r'-{3,20}\s*[A-Za-z][A-Za-z ]{1,30}\s*-{3,20}'      # --- STATS ---
    r'|={2,20}\s*[A-Za-z][A-Za-z ]{1,30}\s*={2,20}'     # == Stats ==
    r'|<EM\d>\s*(?:={2,20}\s*)?[A-Za-z][A-Za-z ]{1,30}'  # <EM3>MISSION DETAILS…
    r'(?:\s*={2,20})?\s*</EM\d>'
    r')')


def _anhaengen(grundlage, block):
    """Unseren Block anhängen — aber **vor** einem fremden Anhang, wenn da einer ist.

    Ohne Fremdanhang ist das schlichtes `grundlage + block`, wie bisher.
    Steht dahinter fremder Text im Blockformat, schiebt sich unserer davor.

    ⚠ Der Unterschied ist nicht kosmetisch: Werkzeuge, die ihren eigenen Block
    abräumen, schneiden „ab dem eigenen Marker bis zum Ende". Alles davor
    überlebt, alles dahinter nicht.
    """
    treffer = FREMDER_ANHANG.search(grundlage)
    if not treffer:
        return grundlage + block
    return grundlage[:treffer.start()] + block + grundlage[treffer.start():]


KASTEN_HAB = '<EM4>[x]</EM4>'
KASTEN_FEHLT = '[  ]'
LINIE = '-' * 57


def _sprachkuerzel(sprache):
    """`german_(germany)` -> 'de', alles andere -> 'en'."""
    return 'de' if str(sprache).lower().startswith('german') else 'en'


def _zeile_zerlegen(zeile):
    """'SCHLUESSEL,P=Text' -> ('SCHLUESSEL', ',P', 'Text'). Sonst None."""
    trenner = zeile.find('=')
    if trenner < 1:
        return None
    kopf, text = zeile[:trenner], zeile[trenner + 1:]
    if ',' in kopf:
        schluessel, _, zusatz = kopf.partition(',')
        return schluessel, ',' + zusatz, text
    return kopf, '', text


# Wo die Originaltexte liegen, bevor etwas eingefügt wird.
#
# ⚠ Warum es diese Datei gibt: Bis v3.0.0 stand um jede Einfügung ein Marken-Paar
# `[SCBPW] … [/SCBPW]`, damit sie sich auf den Buchstaben genau wieder entfernen
# lässt. Das funktionierte — nur **sieht man die Marken im Spiel**. Im
# Auftragstitel stand „Security Patrol[SCBPW] [BP 3/6][/SCBPW]", und das ist
# nichts, was jemand in seinem Spiel haben will.
#
# Der Ausweg ist nicht ein unsichtbareres Zeichen — was die Spiel-Engine mit
# unbekannten Zeichen macht, weiß man erst, wenn es zu spät ist. Stattdessen wird
# der **Originaltext** jeder angefassten Zeile hier festgehalten. Damit braucht es
# im Spieltext gar keine Marke mehr, und das Zurücksetzen ist genauer als vorher:
# Es stellt den Wortlaut wieder her, statt eine Einfügung herauszuschneiden.
URTEXT_DATEI = 'injektion-urtext.json'


def _urtext_datei():
    """Der ganze Inhalt der Merkdatei — leer, wenn es sie nicht gibt."""
    try:
        with open(pfade.app_datei(URTEXT_DATEI), encoding='utf-8') as f:
            daten = json.load(f)
        return daten if isinstance(daten, dict) else {}
    except Exception:
        return {}


def urtext_laden():
    """Die gemerkten Originaltexte — leer, wenn es noch keine gibt."""
    return _urtext_datei().get('texte') or {}


def ist_frisch():
    """Liegt dort eine eben erst eingesetzte Grundlage, in der noch nie
    injiziert wurde?

    ⚠ Diese Auskunft entscheidet, ob der **Notnagel** in `_saeubern()` greifen
    darf. Er erkennt frühere Einfügungen an ihrer Form — und die Form des
    Titelzusatzes ist seit dem 28.08.2026 `<EM4>[BP]</EM4>`, **genau das**, was
    MrKraken in StarStrings selbst an 314 Titel schreibt. In einer frisch
    eingesetzten Fremddatei kann nichts von uns stehen; wer dort trotzdem
    schneidet, löscht fremden Text. Gemessen an der echten Datei vom
    29.08.2026: 17 seiner Kennzeichnungen fielen so weg — und weil als „Urtext"
    der bereits geschnittene Wortlaut gemerkt wurde, kamen sie auch beim
    Zurücksetzen nie wieder.
    """
    return bool(_urtext_datei().get('frisch'))


def urtext_verwerfen():
    """Die gemerkten Originaltexte wegwerfen und die Datei als frisch merken.

    Gehört zu **jedem** Einsetzen einer neuen Grundlage (`uebersetzung.holen()`):
    Die alten Merktexte gehören zur alten Datei und würden auf einen überholten
    Stand zurückschreiben; das Kennzeichen `frisch` schützt den fremden Text
    beim ersten Lauf (siehe `ist_frisch()`)."""
    try:
        ziel = pfade.app_datei(URTEXT_DATEI)
        os.makedirs(os.path.dirname(ziel), exist_ok=True)
        with open(ziel, 'w', encoding='utf-8') as f:
            json.dump({'stand': time.strftime('%Y-%m-%d %H:%M:%S'),
                       'frisch': True, 'texte': {}}, f, ensure_ascii=False)
        return True
    except Exception as ausnahme:
        fehler.merken('injektion.urtext_verwerfen', ausnahme)
        return False


def urtext_sichern(texte, ini_pfad):
    """Die Originaltexte festhalten. Fehlschlag ist kein Grund abzubrechen."""
    try:
        ziel = pfade.app_datei(URTEXT_DATEI)
        os.makedirs(os.path.dirname(ziel), exist_ok=True)
        with open(ziel, 'w', encoding='utf-8') as f:
            json.dump({'datei': ini_pfad, 'stand': time.strftime('%Y-%m-%d %H:%M:%S'),
                       'texte': texte}, f, ensure_ascii=False)
        return True
    except Exception as ausnahme:
        fehler.merken('injektion.urtext_sichern', ausnahme)
        return False


def _saeubern(text, schluessel='', urtext=None, notnagel=OHNE_MARKE):
    """Frühere Einfügungen entfernen — damit sich nichts stapelt.

    Drei Wege, in dieser Reihenfolge:

      1. **Der gemerkte Originaltext.** Der genaueste: Er stellt den Wortlaut
         wieder her, statt etwas herauszuschneiden.
      2. **Das alte Marken-Paar.** Für alles, was frühere Versionen eingetragen
         haben — die stehen ja noch in der Datei von jemandem, der aktualisiert.
      3. **Der eingefügte Block an seiner Form erkannt.** Der Notnagel, wenn die
         Merkdatei fehlt (anderer Rechner, aufgeräumt) und keine Marke dasteht.

    ⚠ Der dritte Weg wird mit `notnagel=None` abgeschaltet — bei einer eben
    eingesetzten Grundlage (siehe `ist_frisch()`). Er erkennt die Einfügung nur
    an ihrer Form, und die ist nicht unser Eigentum: MrKraken schreibt in
    StarStrings dasselbe `<EM4>[BP]</EM4>` an seine Titel. In einer frischen
    Datei kann ohnehin nichts von uns stehen, also gibt es dort nichts zu
    schneiden — nur fremden Text zu verlieren.

    ⚠ Nur anfassen, wenn wirklich etwas gefunden wurde. Ein `rstrip()` auf jeder
    Zeile hätte auch Leerzeichen entfernt, die CIG **absichtlich** gesetzt hat
    (`ASD_FluffText_Eng_5,P=HIGH LEVELS OF\\nRADIATION DETECTED `). Beim ersten
    Vergleichslauf waren das über 3 KB stiller Textschaden an Stellen, mit denen
    dieses Werkzeug nichts zu tun hat."""
    if urtext and schluessel in urtext:
        return urtext[schluessel]
    if AUF in text:
        return MARKE.sub('', text).rstrip()
    if not notnagel:
        return text
    treffer = notnagel.search(text)
    if treffer:
        weg = text[treffer.start():]
        # ⚠ Ein Block mit unserer Überschrift, aber **ohne Kästchen**, ist nicht
        # unserer — er kommt aus derselben Quelle, gesetzt vom SC Deutsch
        # Launcher. Der bleibt stehen; er gehört dem Spieler.
        if EIGENER_NACHWEIS.search(weg) and not _hat_kaestchen(weg):
            return text
        # ⚠ Der Notnagel schneidet „ab hier bis zum Ende" — seit unser Block
        # **vor** einem fremden Anhang sitzt (`_anhaengen`), läge der fremde
        # Text mit im Schnitt. Also nur bis dorthin schneiden und den Rest
        # wieder anfügen. Ohne das nähme das Zurücksetzen Smart Citizens
        # Stats-Blöcke mit — genau der Schaden, den wir gerade verhindern.
        fremd = FREMDER_ANHANG.search(weg)
        rest = weg[fremd.start():] if fremd else ''
        return text[:treffer.start()].rstrip() + rest
    return text


def _zahl(wert, worte):
    """1234567 -> '1.234.567' bzw. '1,234,567' — je nach Sprache."""
    return format(int(wert), ',d').replace(',', worte['trenner'])


def _block(eintrag, habe, worte):
    """Der Textblock, der an die Beschreibung gehängt wird.

    Erst die Eckdaten als kurze Liste, dann die Baupläne mit Kästchen. Die
    Reihenfolge ist Absicht: Ob sich der Auftrag überhaupt lohnt, entscheidet
    man an Chance und Reputation — die Namensliste liest man erst danach."""
    z = ['', LINIE, '', '<EM4>%s</EM4>' % worte['ueberschr'], '']

    chance = eintrag.get('chance')
    if chance:
        z.append('# %s: %d%%' % (worte['chance'], round(chance * 100)))
    if eintrag.get('rang'):
        z.append('# %s: %s (%s XP)' % (worte['rep_min'], eintrag['rang'],
                                       _zahl(eintrag.get('rep') or 0, worte)))
    if eintrag.get('rang_max'):
        z.append('# %s: %s (%s XP)' % (worte['rep_max'], eintrag['rang_max'],
                                       _zahl(eintrag.get('rep_max') or 0, worte)))
    if eintrag.get('uec'):
        z.append('# %s: %s aUEC' % (worte['lohn'], _zahl(eintrag['uec'], worte)))
    if eintrag.get('ruf'):
        z.append('# %s: %s XP' % (worte['ruf'], _zahl(eintrag['ruf'], worte)))
    if eintrag.get('cooldown'):
        z.append('# %s: %s %s' % (worte['cooldown'],
                                  _zahl(eintrag['cooldown'], worte),
                                  worte['minuten']))
    if 'teilbar' in eintrag:
        z.append('# %s: %s' % (worte['teilbar'],
                               worte['ja'] if eintrag['teilbar'] else worte['nein']))

    # ⚠ **Ohne „3 von 12", seit dem 28.08.2026** — aus demselben Grund wie im
    # Titel (siehe `_titel_zusatz`): Die Liste führt alle Preisstufen zusammen,
    # die Zahl wäre eine Behauptung über etwas, das gar nicht auflösbar ist.
    # Die Kästchen sagen dasselbe, nur ehrlich: angehakt heißt „hab ich".
    z += ['', '# ' + worte['liste'] + ':']

    # Wo sich die Stufen unterscheiden, steht der nötige Rang hinter dem Namen.
    # ⚠ Das ist **nur Text**. Es blendet nichts aus und hakt nichts anders ab:
    # Wer den Bauplan hat, hat ihn — auch wenn diese Stufe ihn nicht hergibt.
    ab = eintrag.get('ab') or {}
    breite = max([len(n) for n in eintrag['bp']] or [0])
    for name in eintrag['bp']:
        drin = katalog_modul._norm(name) in habe
        bed = ab.get(name)
        zeile = '   %s %s' % (KASTEN_HAB if drin else KASTEN_FEHLT, name)
        if bed:
            zeile += '%s  %s %s (%s XP)' % (' ' * (breite - len(name)),
                                            worte['ab_rang'], bed['rang'],
                                            _zahl(bed['rep'], worte))
        z.append(zeile)

    # Gibt es Stufen dieses Auftrags, die leer ausgehen, gehört das dazu —
    # sonst fliegt jemand für eine Liste hin, die seine Stufe nie hergibt.
    # Genau das ist Morkhan am 28.08.2026 passiert.
    if eintrag.get('leer'):
        z += ['', '# ' + (worte['leere_stufen']
                          % (eintrag['leer'], eintrag['stufen']))]
    z += ['', worte['quelle']]
    # Ohne Marken — sie standen sichtbar im Spiel. Zurückgesetzt wird über die
    # gemerkten Originaltexte (siehe `URTEXT_DATEI`).
    return '\\n' + '\\n'.join(z)


# Ein Kürzel, das am Anfang eines Namens schon dasteht — `[CS1] Spark-G Missile`.
# So schreibt MrKraken seine Angaben (136 Namen in der Fassung vom 29.08.2026),
# der Watcher setzt seine dahinter in runde Klammern. Ohne diese Prüfung stünde
# im Spiel `[CS1] Spark-G Missile (CS1)`.
FREMDES_KUERZEL = re.compile(r'^\[[A-Za-z0-9/. -]{1,14}\]\s')


def _hat_kaestchen(text):
    """Steht in diesem Stück ein Kästchen von uns?

    **Das ist das Unterscheidungsmerkmal.** Watcher und SC Deutsch Launcher
    schöpfen aus derselben Quelle (`bp-contracts_short.json` des SCDL-Teams) und
    schreiben deshalb wortgleiche Blöcke — dieselbe Überschrift, dieselbe Liste.
    Der einzige Unterschied ist der, der das Werkzeug ausmacht: In den Rohdaten
    steht `    - Atzkav Sniper Rifle`, bei uns `    [x] Atzkav Sniper Rifle`.
    Wo kein Kästchen steht, hat nicht der Watcher geschrieben."""
    return '[x]' in text or KASTEN_FEHLT in text


def _hat_titelmarke(text):
    """Trägt dieser Titel schon eine Bauplan-Marke — von wem auch immer?

    Drei Werkzeuge schreiben dieselbe: der Watcher, MrKrakens StarStrings
    (`<EM4>[BP]</EM4>`, auch als `<EM4>[10 Rep] [BP]</EM4>`) und der SC Deutsch
    Launcher (die Rohdaten geben `title` = ` <EM4>[BP]</EM4>` für 369 der 818
    Aufträge vor). Steht sie schon da, kommt keine zweite dazu — egal, wer sie
    gesetzt hat. Sie bedeutet ohnehin dasselbe: hier gibt es Baupläne."""
    return bool(TITELMARKE.search(text))


def _fremdblock_trennen(text):
    """Einen fremden Bauplan-Block abtrennen: (Text ohne ihn, der Block).

    Fremd heißt: unsere Überschrift, aber **keine Kästchen** — also der Block
    des SC Deutsch Launchers, der aus derselben Quelle stammt. Er wird nicht
    stehengelassen (sonst stünde die Liste zweimal untereinander) und nicht
    verworfen (er gehört dem Spieler), sondern **ersetzt**: Unserer tritt an
    seine Stelle, und weil der Urtext ihn behält, kommt er beim Zurücksetzen
    wieder."""
    treffer = OHNE_MARKE_BLOCK.search(text)
    if treffer and not _hat_kaestchen(text[treffer.start():]):
        return text[:treffer.start()].rstrip(), text[treffer.start():]
    return text, ''


def _notnagel(urtext_alt, ini_pfad):
    """Welcher Formen-Notnagel darf greifen — oder gar keiner?

    Er erkennt frühere Einfügungen **nur an ihrer Form** und ist deshalb der
    unsicherste der drei Wege in `_saeubern()`. Gebraucht wird er nur, wenn
    nichts Besseres da ist:

      * **frisch eingesetzte Grundlage** → gar keiner. Dort kann nichts von uns
        stehen; was da ist, gehört dem fremden Projekt.
      * **Merktexte vorhanden** → gar keiner. Dann ist der gemerkte Wortlaut
        maßgeblich, und er ist auf das Zeichen genau.
      * **keine eigene Spur in der Datei** → nur der Block, nie der Titel. Ein
        blankes `<EM4>[BP]</EM4>` ist nicht unser Alleinstellungsmerkmal; hat der
        Watcher hier nie geschrieben, gehört es jemand anderem.
      * sonst → der volle Notnagel (fehlende Merkdatei, anderer Rechner).
    """
    if ist_frisch() or urtext_alt:
        return None
    return OHNE_MARKE if ist_drin(ini_pfad) else OHNE_MARKE_BLOCK


def _titel_zusatz(eintrag, habe, worte):
    """Kürzel für die Auftragsliste: sieht man, ohne aufzuklappen.

    ⚠ **Ohne Zählung, seit dem 28.08.2026.** Hier stand `[BP 3/12]`. Die Zahl
    sah nützlich aus, war aber nicht wahr: Die Liste eines Auftrags führt alle
    Baupläne **aller** Preisstufen zusammen, und welche davon die eigene Stufe
    hergibt, lässt sich nicht auflösen — 123 von 353 Aufträgen teilen sich den
    Textschlüssel über ihre Stufen hinweg. „3 von 12" hieß damit in Wahrheit
    „3 von 12, die irgendjemand irgendwo bekommen kann".

    gemeldet, nachdem die Meldungen kamen: „die zählung war meine idee und ich
    fand sie gut, bis die fehlermeldungen kamen — nun weiß ich, sie ist Schrott
    und eh nicht wahr." Ein schlichtes `[BP]` sagt, was stimmt: Hier gibt es
    Baupläne. Was man davon hat, sagen die Kästchen in der Liste.
    """
    zeichen = '!' if (eintrag.get('bpnote') or '').strip() else ''
    return ' <EM4>[%s%s]</EM4>' % (worte['kurz'], zeichen)


def scdl_holen(sprachkuerzel, fortschritt=None):
    """Die Vertragsdaten des SCDL-Teams holen und ablegen. (Erfolg, Anzahl)."""
    from .katalog import AUS
    datei = SCDL_DATEI.get(sprachkuerzel)
    if not datei or AUS:          # ⚠ SC_BP_NO_NET gilt auch hier
        return False, 0
    try:
        if fortschritt:
            fortschritt('Bauplan-Daten werden geladen …')
        req = urllib.request.Request(
            SCDL_ROH % datei,
            headers={'User-Agent': 'SC-BP-Watcher'})
        with urllib.request.urlopen(req, timeout=60) as r:
            roh = json.loads(r.read().decode('utf-8'))
        eintraege = roh.get('entries') or []
        if not eintraege:
            return False, 0
        ziel = pfade.app_datei(SCDL_CACHE % sprachkuerzel)
        with open(ziel + '.tmp', 'w', encoding='utf-8') as f:
            json.dump(roh, f, ensure_ascii=False)
        os.replace(ziel + '.tmp', ziel)
        return True, len(eintraege)
    except Exception:
        return False, 0


def scdl_laden(sprachkuerzel):
    """Die abgelegten Vertragsdaten — oder None."""
    try:
        with open(pfade.app_datei(SCDL_CACHE % sprachkuerzel),
                  encoding='utf-8') as f:
            roh = json.load(f)
        return roh if roh.get('entries') else None
    except Exception:
        return None


# Name der Einstellung, mit der sich die Angaben am Gegenstand abschalten
# lassen. Standard ist **an**: Wer die Injektion einschaltet, will Angaben im
# Spiel sehen — und genau dafür ist dieses Werkzeug da.
EINSTELLUNG_ANGABEN = 'angaben_am_gegenstand'


def _namens_tabelle(zeilen, nur_entfernen=False):
    """Tabelle *Namensschlüssel → Kürzel* — oder leer, wenn abgeschaltet.

    Beim reinen Entfernen bleibt sie leer: Dann stellt der Urtext-Weg die
    ursprünglichen Namen wieder her, und es soll nichts Neues dazukommen."""
    if nur_entfernen or not pfade.einstellung_wahrheit(EINSTELLUNG_ANGABEN, True):
        return {}
    try:
        return angaben_modul.tabelle_bauen(zeilen)
    except Exception as ausnahme:
        fehler.merken('injektion._namens_tabelle', ausnahme)
        return {}


def _name_mit_angabe(text, kuerzel):
    """Den Zusatz an einen Namen hängen — vorhandene Klammer vorher abschneiden.

    Der SC Deutsch Launcher hängt seinerseits `(CS1)` an. Ohne das Abschneiden
    stünde danach `Spark I-G Missile (CS1) (IR1)` im Spiel.

    ⚠ Steht das Kürzel schon **vorn** (`[CS1] Spark-G Missile`), bleibt der Name
    unangetastet. Das ist MrKrakens Schreibweise, und es ist dieselbe Angabe —
    sie ein zweites Mal anzuhängen, macht den Namen nur länger und falscher."""
    if FREMDES_KUERZEL.match(text):
        return text
    return '%s %s' % (angaben_modul.zusatz_entfernen(text).rstrip(), kuerzel)


def bestand_marke(bestand=None):
    """Ein kurzer Fingerabdruck des eigenen Bestands.

    ⚠ Wozu: Die Kästchen in den Auftragstexten zeigen, was der Spieler schon
    hat. Ändert sich sein Bestand, müssen sie neu geschrieben werden — sonst
    stimmen sie ab dem nächsten Fund nicht mehr. Ein Vergleich dieser Marke
    beantwortet die Frage „hat sich seit dem letzten Einspielen etwas getan?"
    ohne die Texte jedes Mal neu zu bauen.

    ⚠ Über die **Namen**, nicht über die Anzahl: `bestand.angleichen()` benennt
    beim Start Einträge um, ohne dass die Zahl sich ändert — und genau die
    Namen stehen in den Kästchen.
    """
    import hashlib
    from . import bestand as bestand_datei
    namen = bestand_datei.schluessel(
        bestand if bestand is not None else bestand_datei.laden())
    roh = '\n'.join(sorted(namen)).encode('utf-8', 'replace')
    return '%d-%s' % (len(namen), hashlib.sha1(roh).hexdigest()[:12])


def _kaestchen_setzen(text, habe):
    """In einem fertigen SCDL-Block die Bauplan-Zeilen ankreuzen.

    Aus `    - Atzkav Sniper Rifle` wird `    [x] Atzkav Sniper Rifle`, wenn er
    im Bestand liegt — sonst `    [  ] …`. Der übrige Text bleibt unangetastet;
    er gehört dem SCDL-Team, wir hängen nur das Häkchen dran.

    ⚠ **Nur unter der Bauplan-Überschrift.** Abgabeorte und Regionen stehen im
    selben Block als Liste; sie anzukreuzen ergibt keinen Sinn (siehe
    `BP_UEBERSCHRIFT`).

    Gezählt wird nebenbei, damit das Titel-Kürzel dieselbe Zahl zeigt."""
    zeilen = text.split('\\n')
    meine = gesamt = 0
    in_liste = False
    for i, zeile in enumerate(zeilen):
        if UEBERSCHRIFT_ZEILE.match(zeile):
            in_liste = bool(BP_UEBERSCHRIFT.match(zeile))
            continue
        if not in_liste:
            continue
        m = BP_ZEILE.match(zeile)
        if not m:
            continue
        einzug, name = m.group(1), m.group(2).strip()
        if name.startswith('#') or not name:
            continue
        gesamt += 1
        drin = katalog_modul._norm(name) in habe
        if drin:
            meine += 1
        zeilen[i] = '%s%s %s' % (einzug, KASTEN_HAB if drin else KASTEN_FEHLT, name)
    return '\\n'.join(zeilen), meine, gesamt


def _auftragsangaben(block, eintrag):
    """Rufpunkte, Abklingzeit, Teilbarkeit und Bauplan-Chance einsetzen.

    ⚠⚠ **Gewünscht von Bushwick4712 (KRT) am 04.09.2026:** „XP und Abklingzeit
    fehlen in den Questtexten, der SC Deutsch Launcher liefert diese wohl, dann
    brauchen wir das auch."

    Er hat recht, und die Daten lagen längst vor — wir haben sie nur nicht
    benutzt. Gemessen über alle 367 Aufträge mit Beschreibung:

    | Angabe | stand im Spiel | liegt in der Quelle |
    |---|---|---|
    | Zu erwartende Rufpunkte | **0** | 311 |
    | Abklingzeit | **0** | 367 |
    | Mission teilbar | **0** | 367 |
    | Chance auf Bauplan | **0** | 367 |

    Eingesetzt wird **direkt vor der Bauplan-Überschrift** — dort stehen schon
    die Reputationszeilen im selben `#`-Stil, und wer die Liste liest, hat die
    Rahmenbedingungen dann darüber statt irgendwo darunter.

    ⚠⚠ **Die Zeilen werden mit LITERALEM `\\n` getrennt, nicht mit einem echten
    Zeilenumbruch.** Die `global.ini` des Spiels führt Umbrüche als zwei
    Zeichen (Backslash + n); ein echter Umbruch zerreißt den Eintrag und das
    Spiel zeigt den Rest gar nicht mehr. Der ganze Block wird deshalb überall
    mit `'\\n'` zerlegt und wieder zusammengesetzt.

    ⚠ **Nichts doppelt einsetzen.** Steht eine Angabe schon da (weil ein
    anderes Werkzeug sie geschrieben hat oder wir selbst beim letzten Lauf),
    bleibt sie stehen — dieselbe Regel wie bei den Marken.
    """
    zusatz = []
    for feld in ('contractInfo', 'dropChance'):
        for zeile in (eintrag.get(feld) or '').split('\\n'):
            zeile = zeile.strip()
            if zeile and zeile not in block:
                zusatz.append(zeile)
    if not zusatz:
        return block

    zeilen = block.split('\\n')
    # Vor die Bauplan-Überschrift, sonst ans Ende der Kopfzeilen.
    stelle = None
    for i, zeile in enumerate(zeilen):
        if BP_UEBERSCHRIFT.match(zeile):
            stelle = i
            break
    if stelle is None:
        return '\\n'.join(zeilen + [''] + zusatz)
    # Eine Leerzeile davor, wenn dort nicht schon eine steht — sonst kleben
    # die neuen Zeilen an der Reputationsangabe.
    davor = zusatz + ['']
    if stelle > 0 and zeilen[stelle - 1].strip():
        davor = [''] + davor
    zeilen[stelle:stelle] = davor
    return '\\n'.join(zeilen)


def _stamm(schluessel):
    """Der Namensanfang, den Titel und Beschreibungen eines Auftrags teilen.

    Aus `Covalex_HaulCargo_AToB_title` und `Covalex_HaulCargo_AtoB_desc_ToRuinStation`
    wird beide Male `covalex_haulcargo_atob`. Alles ab `_title` bzw. `_desc` fällt
    weg, der Rest wird kleingeschrieben — in den Spieldaten wechselt die
    Schreibweise mitten im Wort.
    """
    klein = (schluessel or '').lower()
    for trenner in ('_title', '_desc'):
        stelle = klein.find(trenner)
        if stelle > 0:
            return klein[:stelle]
    return ''


# Ein Teilauftrag heißt wie seine Reihe plus ein **direkt angehängtes** Kürzel:
# aus `battaglia_story01` wird `battaglia_story01b`, `…01c`.
#
# ⚠⚠ **Der Unterstrich ist die Grenze, und zwar aus einem gemessenen Grund.**
# Eine erste Fassung erlaubte auch `_h`, `_m` — und traf damit prompt
# `headhunters_defend_xt_h` und `…_m`. Das sind aber keine Schritte einer
# Reihe, sondern **Schwierigkeitsstufen** (VE/E/M/H/VH/S), und die geben
# unterschiedliche Baupläne. Dass die Quelle für `…_VH` einen **eigenen**
# Eintrag führt, beweist es: Sie behandelt Stufen als eigenständige Aufträge.
# Fehlen `_H` und `_M` dort, ist das eine Lücke in der Quelle — sie mit den
# Daten der Grundstufe zu füllen wäre geraten, nicht gewusst.
#
# Damit bleibt die Linie des Werkzeugs gewahrt: Was wir nicht wissen,
# behaupten wir nicht.
REIHEN_SUFFIX = re.compile(r'^[A-Za-z0-9]{1,2}$')


def _reihen_stamm(stamm, bekannte):
    """Zu einem Teilauftrag den Stamm seiner Reihe — oder `None`.

    ⚠⚠ **Warum es das braucht** (03.09.2026): Mehrteilige Auftragsreihen
    tragen ihre Bauplan-Angabe nur am Schlüssel der *Reihe*. Im Spiel sieht
    der Spieler aber den *Schritt*, an dem er gerade steht:

        Battaglia_Story01_title  = Willkommen im System <EM4>[BP!]</EM4>
        Battaglia_Story01B_title = Bergbau-Gelegenheit      ← das steht im Log
        Battaglia_Story01C_title = Notruf

    Das Overlay meldete „Willkommen im System → 1 Bauplan, dir fehlt: Clearcut
    Module", und im aufgeschlagenen Auftrag stand nichts davon. Es fehlten
    keine Daten — die Marke saß am Nachbarschlüssel.

    Der längste passende Stamm gewinnt: Gäbe es `battaglia_story0` und
    `battaglia_story01`, gehört `battaglia_story01b` zum zweiten.
    """
    if not stamm or stamm in bekannte:
        return None
    beste = None
    for kandidat in bekannte:
        if kandidat == stamm or not stamm.startswith(kandidat):
            continue
        rest = stamm[len(kandidat):]
        if not rest or not REIHEN_SUFFIX.match(rest):
            continue
        if beste is None or len(kandidat) > len(beste):
            beste = kandidat
    return beste


def einspielen_scdl(ini_pfad, sprachkuerzel, bestand=None):
    """Injektion aus den SCDL-Vertragsdaten — der vollständigere Weg.

    Gibt (Erfolg, Anzahl, Meldung) zurück wie `einspielen()`."""
    daten = scdl_laden(sprachkuerzel)
    if not daten:
        return False, 0, t('m_keine_scdl')
    if not ini_pfad or not os.path.isfile(ini_pfad):
        return False, 0, t('m_keine_ini')

    habe = bestand_datei.schluessel(bestand if bestand is not None
                                    else bestand_datei.laden())
    worte = TEXTE[sprachkuerzel]

    titel_an, text_an = {}, {}
    for e in daten['entries']:
        block = e.get('description') or ''
        if not block:
            continue
        block, meine, gesamt = _kaestchen_setzen(block, habe)
        # ⭐ Rufpunkte, Abklingzeit, Teilbarkeit, Bauplan-Chance — sie standen
        # in der Quelle, aber nicht im Spiel. Siehe `_auftragsangaben`.
        block = _auftragsangaben(block, e)
        if e.get('descriptionLocKey'):
            text_an[e['descriptionLocKey']] = block
        if e.get('titleLocKey'):
            # Statt des schlichten [BP] die eigene Zählung — das ist der
            # Mehrwert gegenüber der reinen Fremdfassung.
            #
            # ⚠ Und ein **Rufzeichen**, wenn die Baupläne an Bedingungen hängen.
            # Gemessen an den Vertragsdaten: **332 von 818** Aufträgen (41 %)
            # geben ihre Baupläne nur in bestimmten Preisstufen oder ab einem
            # Rang — „Baupläne nur für 256.500 / 264.000 aUEC Mission", „nur ab
            # Meister-Rang". Das steht zwar im Beschreibungstext, aber in der
            # **Auftragsliste** sah man bisher nur `[BP 0/19]`, und genau danach
            # entscheidet man, ob man annimmt.
            #
            # Morkhan am 28.08.2026 genau so hereingefallen: Auftrag angenommen
            # (Neuling, 49.750 aUEC), Bauplan-Zähler im Titel gesehen — geben
            # konnte die Stufe nie einen. Ein Zeichen im Titel kostet nichts und
            # erspart die vergebliche Mission.
            zeichen = '!' if (e.get('bpnote') or '').strip() else ''
            titel_an[e['titleLocKey']] = (' <EM4>[%s%s]</EM4>'
                                          % (worte['kurz'], zeichen))

    geaendert = 0
    try:
        with open(ini_pfad, encoding='utf-8', errors='ignore') as f:
            zeilen = f.read().splitlines()
    except OSError as e:
        return False, 0, 'Lesen fehlgeschlagen: %s' % e

    # ⚠ Ein Auftrag hat EINEN Titel, aber oft ein Dutzend Beschreibungen: je eine
    # für „zur Ruinenstation", „zum Verteilzentrum", „von A nach B" und so weiter.
    # Die Vertragsdaten nennen dazu immer nur **eine** — die übrigen blieben leer.
    # Im Spiel stand dann im Titel „[BP 0/12]", und wer die Beschreibung öffnete,
    # um zu sehen *welche* zwölf, fand nichts. Genau so gemeldet.
    #
    # Gemessen an einer echten Installation: allein bei Covalex 51 Beschreibungen im
    # Spiel, davon 7 mit Angaben.
    #
    # Deshalb ein zweiter Weg über den gemeinsamen Namensanfang: Zu jedem Titel,
    # der Angaben bekommt, werden alle Beschreibungen desselben Auftrags mit
    # demselben Block versehen. Groß- und Kleinschreibung zählt dabei nicht —
    # in den Spieldaten steht `Covalex_HaulCargo_AToB_title` neben
    # `Covalex_HaulCargo_AtoB_desc_ToRuinStation`, mit unterschiedlichem „to".
    stamm_an = {}
    for e in daten['entries']:
        block = text_an.get(e.get('descriptionLocKey') or '')
        stamm = _stamm(e.get('titleLocKey') or e.get('descriptionLocKey') or '')
        if block and stamm and stamm not in stamm_an:
            stamm_an[stamm] = block

    # Dasselbe für die TITEL — die Voraussetzung für mehrteilige Reihen.
    # Ohne diese Tabelle gäbe es nur den exakten Schlüsselvergleich, und ein
    # Teilauftrag (`…Story01B_title`) findet den Zusatz seiner Reihe nie.
    titel_stamm_an = {}
    for schluessel, zusatz in titel_an.items():
        stamm = _stamm(schluessel)
        if stamm and stamm not in titel_stamm_an:
            titel_stamm_an[stamm] = zusatz

    # ⚠ Ohne Marken im Text: Was hier angefasst wird, kommt vorher in die
    # Merkdatei. Siehe `URTEXT_DATEI` — die Marken waren im Spiel sichtbar.
    urtext_alt = urtext_laden()
    urtext_neu = {}
    notnagel = _notnagel(urtext_alt, ini_pfad)
    namens_zusatz = _namens_tabelle(zeilen)

    neu = []
    for zeile in zeilen:
        teile = _zeile_zerlegen(zeile)
        if not teile:
            neu.append(zeile)
            continue
        schluessel, zusatz, text = teile
        # Der Wortlaut ohne UNSERE Einfügung. Ein fremder Block (Launcher) kann
        # darin noch stehen — er wird gleich abgetrennt, aber nicht verworfen.
        ur = _saeubern(text, schluessel, urtext_alt, notnagel)
        grundlage, _fremd = _fremdblock_trennen(ur)
        sauber = grundlage
        angefasst = False
        if schluessel in namens_zusatz:
            sauber = _name_mit_angabe(grundlage, namens_zusatz[schluessel])
            angefasst = True
        elif schluessel in titel_an:
            # ⚠ Steht die Marke schon da, kommt keine zweite dazu — gleich, ob
            # StarStrings oder der SC Deutsch Launcher sie gesetzt hat.
            if not _hat_titelmarke(grundlage):
                sauber, angefasst = grundlage + titel_an[schluessel], True
        elif schluessel in text_an:
            sauber, angefasst = _anhaengen(grundlage, text_an[schluessel]), True
        elif schluessel.lower().endswith('_title'):
            # Keine eigene Angabe — aber vielleicht ist es ein SCHRITT einer
            # Reihe, deren Hauptauftrag Baupläne bringt (siehe
            # `_reihen_stamm`). Der Spieler sieht im Auftragsfenster genau
            # diesen Schritt; ohne den Zusatz erfährt er dort nichts.
            haupt = _reihen_stamm(_stamm(schluessel), titel_stamm_an)
            if haupt and not _hat_titelmarke(grundlage):
                sauber, angefasst = grundlage + titel_stamm_an[haupt], True
        elif '_desc' in schluessel.lower():
            # Keine eigene Angabe — aber vielleicht gehört die Beschreibung zu
            # einem Auftrag, für den wir welche haben.
            block = stamm_an.get(_stamm(schluessel))
            if not block:
                # Wie beim Titel: auch Schritte einer Reihe versorgen, sonst
                # steht im Schritt `[BP!]` und darunter keine Bauplan-Liste.
                haupt = _reihen_stamm(_stamm(schluessel), stamm_an)
                if haupt:
                    block = stamm_an[haupt]
            if block:
                sauber, angefasst = _anhaengen(grundlage, block), True
        if angefasst:
            # Den Wortlaut VOR der Einfügung merken, nicht danach — und **mit**
            # dem fremden Block, damit das Zurücksetzen ihn wiederbringt.
            urtext_neu[schluessel] = ur
            geaendert += 1
        else:
            # Nichts beigesteuert: dann bleibt auch der fremde Block, wo er war.
            sauber = ur
        neu.append('%s%s=%s' % (schluessel, zusatz, sauber))

    try:
        # ⚠⚠ **`newline=''` ist Pflicht — sonst wird die ganze Datei umgeschrieben.**
        # Der Code setzt hier bewusst `\n`, weil das Spiel seine `global.ini` mit
        # Unix-Zeilenenden ausliefert. Ohne diesen Parameter uebersetzt Python
        # unter **Windows** jedes `\n` still in `\r\n` — und damit aendert sich
        # JEDE der 90.363 Zeilen einer 10-MB-Fremddatei, obwohl inhaltlich nichts
        # anders ist. Gemessen am 02.09.2026: +90.363 Bytes, genau ein Byte je
        # Zeile. Unter Linux passiert das nicht, deshalb ist es dort nie
        # aufgefallen — `tools/starstrings_pruefen.py` schlug unter Windows
        # trotzdem fehl („Nach dem Zuruecksetzen weicht der Wortlaut ab"), und
        # zwar schon in v3.9.4.
        with open(ini_pfad + '.tmp', 'w', encoding='utf-8', newline='') as f:
            f.write('\n'.join(neu) + '\n')
        os.replace(ini_pfad + '.tmp', ini_pfad)
    except OSError as e:
        return False, 0, 'Schreiben fehlgeschlagen: %s' % e
    urtext_sichern(urtext_neu, ini_pfad)
    meta = daten.get('_meta') or {}
    return True, geaendert, '%d Textstellen (SCDL %s)' % (geaendert,
                                                          meta.get('version', '?'))


def einspielen(ini_pfad, sprache, katalog=None, bestand=None,
               nur_entfernen=False):
    """Die Angaben in eine `global.ini` schreiben.

    Gibt (Erfolg, Anzahl geänderter Zeilen, Meldung) zurück. Die Datei wird
    erst vollständig neu geschrieben und dann umbenannt — bricht etwas ab,
    bleibt die alte Version unversehrt."""
    if not ini_pfad or not os.path.isfile(ini_pfad):
        return False, 0, t('m_keine_ini')

    katalog = katalog if katalog is not None else katalog_modul.laden()
    missionen = katalog.get('missionen') or {}
    if not missionen and not nur_entfernen:
        return False, 0, t('m_keine_missionen')

    habe = bestand_datei.schluessel(bestand if bestand is not None
                                    else bestand_datei.laden())
    worte = TEXTE[_sprachkuerzel(sprache)]

    # Beide Schlüssel-Arten in eine Tabelle: Titel bekommen das Kürzel,
    # Beschreibungen die Liste.
    titel_keys, text_keys = {}, {}
    for eintrag in missionen.values():
        if eintrag.get('titel_key'):
            titel_keys[eintrag['titel_key']] = eintrag
        if eintrag.get('text_key'):
            text_keys[eintrag['text_key']] = eintrag

    geaendert = 0
    try:
        with open(ini_pfad, encoding='utf-8', errors='ignore') as f:
            zeilen = f.read().splitlines()
    except OSError as e:
        return False, 0, 'Lesen fehlgeschlagen: %s' % e

    urtext_alt = urtext_laden()
    urtext_neu = {}
    notnagel = _notnagel(urtext_alt, ini_pfad)
    namens_zusatz = _namens_tabelle(zeilen, nur_entfernen)

    # ⚠ Eine Mission hat im Spiel **mehr** Beschreibungen, als der Katalog
    # kennt. Gemessen am 28.08.2026: `Covalex_HaulCargo_SingleToMulti` führt
    # drei Beschreibungs-Schlüssel, in der `global.ini` stehen **acht** —
    # verschiedene Zielorte und Waren derselben Mission. Wer eine der fünf
    # übrigen erwischt, sah `[BP 0/12]` im Titel und darunter **nichts**.
    #
    # Genau so gemeldet von Morkhan: „bei ner anderen mission steht, dass man
    # 12 Pläne bekommen kann, aber da werden keine angezeigt."
    #
    # `einspielen_scdl()` löst das seit Langem über den gemeinsamen
    # Namensanfang; hier fehlte es. Deshalb derselbe Weg auch für den eigenen
    # Katalog: Zu jedem Titel, der Angaben bekommt, bekommen **alle**
    # Beschreibungen desselben Auftrags denselben Block.
    stamm_block = {}
    if not nur_entfernen:
        for eintrag in missionen.values():
            stamm = _stamm(eintrag.get('titel_key')
                           or eintrag.get('text_key') or '')
            if stamm and stamm not in stamm_block:
                stamm_block[stamm] = eintrag

    neu = []
    for zeile in zeilen:
        teile = _zeile_zerlegen(zeile)
        if not teile:
            neu.append(zeile)
            continue
        schluessel, zusatz, text = teile
        ur = _saeubern(text, schluessel, urtext_alt, notnagel)
        if ur != text:
            geaendert += 1
        sauber = ur
        if not nur_entfernen:
            # Ein fremder Block (SC Deutsch Launcher) wird abgetrennt und durch
            # unseren ersetzt — der Urtext behält ihn, also kommt er beim
            # Zurücksetzen wieder.
            grundlage, _fremd = _fremdblock_trennen(ur)
            angefasst = False
            if schluessel in namens_zusatz:
                sauber = _name_mit_angabe(grundlage, namens_zusatz[schluessel])
                angefasst = True
            elif schluessel in titel_keys:
                # ⚠ Keine zweite Marke, wo schon eine steht.
                if not _hat_titelmarke(grundlage):
                    sauber = grundlage + _titel_zusatz(titel_keys[schluessel],
                                                       habe, worte)
                    angefasst = True
            elif schluessel in text_keys:
                sauber = _anhaengen(grundlage,
                                    _block(text_keys[schluessel], habe, worte))
                angefasst = True
            elif '_desc' in schluessel.lower():
                # Keine eigene Angabe — aber vielleicht gehört die Beschreibung
                # zu einem Auftrag, für den wir welche haben (siehe oben).
                eintrag = stamm_block.get(_stamm(schluessel))
                if eintrag:
                    sauber = _anhaengen(grundlage,
                                        _block(eintrag, habe, worte))
                    angefasst = True
            if angefasst:
                urtext_neu[schluessel] = ur
                geaendert += 1
            else:
                sauber = ur
        neu.append('%s%s=%s' % (schluessel, zusatz, sauber))

    try:
        # ⚠⚠ **`newline=''` ist Pflicht — sonst wird die ganze Datei umgeschrieben.**
        # Der Code setzt hier bewusst `\n`, weil das Spiel seine `global.ini` mit
        # Unix-Zeilenenden ausliefert. Ohne diesen Parameter uebersetzt Python
        # unter **Windows** jedes `\n` still in `\r\n` — und damit aendert sich
        # JEDE der 90.363 Zeilen einer 10-MB-Fremddatei, obwohl inhaltlich nichts
        # anders ist. Gemessen am 02.09.2026: +90.363 Bytes, genau ein Byte je
        # Zeile. Unter Linux passiert das nicht, deshalb ist es dort nie
        # aufgefallen — `tools/starstrings_pruefen.py` schlug unter Windows
        # trotzdem fehl („Nach dem Zuruecksetzen weicht der Wortlaut ab"), und
        # zwar schon in v3.9.4.
        with open(ini_pfad + '.tmp', 'w', encoding='utf-8', newline='') as f:
            f.write('\n'.join(neu) + '\n')
        os.replace(ini_pfad + '.tmp', ini_pfad)
    except OSError as e:
        return False, 0, 'Schreiben fehlgeschlagen: %s' % e
    # Beim reinen Entfernen ist nichts mehr zu merken — die Datei wird geleert,
    # damit ein späterer Lauf nicht auf einen überholten Stand zurücksetzt.
    urtext_sichern(urtext_neu, ini_pfad)

    return True, geaendert, '%d Textstellen' % geaendert


def einrichten(ini_pfad, sprache, fortschritt=None, bestand=None):
    """Die Bauplan-Angaben eintragen — auf dem jeweils besten Weg.

    Zuerst die Vertragsdaten des SCDL-Teams: 813 Verträge mit gepflegten
    Texten. Sind sie nicht erreichbar, tut es der eigene Aufbau aus den
    scmdb-Daten (349 Verträge) — dann fehlen Feinheiten wie Region und
    Gefahrenstufe, aber die Baupläne stehen da, und darum geht es."""
    kuerzel = _sprachkuerzel(sprache)
    if not scdl_laden(kuerzel):
        scdl_holen(kuerzel, fortschritt)
    if scdl_laden(kuerzel):
        ok, n, meldung = einspielen_scdl(ini_pfad, kuerzel, bestand)
        if ok:
            return ok, n, meldung
    return einspielen(ini_pfad, sprache, bestand=bestand)


def aktualisieren(ini_pfad, sprache, fortschritt=None, bestand=None):
    """Frische Vertragsdaten holen und neu eintragen.

    Gebraucht nach jedem Übersetzungs-Update und nach jedem Spiel-Patch: Beide
    schreiben die `global.ini` neu, die Angaben sind dann stillschweigend weg."""
    scdl_holen(_sprachkuerzel(sprache), fortschritt)
    return einrichten(ini_pfad, sprache, fortschritt, bestand)


def scdl_update_da(sprachkuerzel):
    """Gibt es bei den Vertragsdaten etwas Neueres? (ja/nein, neue Kennung).

    Verglichen wird die Kennung aus `_meta.version` (z. B. „LIVE 20.08.2026").
    Geholt wird dafür die ganze Datei — sie hat keine eigene Versionsauskunft,
    und 2,4 MB einmal am Tag sind kein Grund, dafür etwas zu bauen."""
    from .katalog import AUS
    alt = scdl_stand(sprachkuerzel)
    datei = SCDL_DATEI.get(sprachkuerzel)
    if not datei or AUS:          # ⚠ SC_BP_NO_NET gilt auch hier
        return False, None
    try:
        req = urllib.request.Request(SCDL_ROH % datei,
                                     headers={'User-Agent': 'SC-BP-Watcher'})
        with urllib.request.urlopen(req, timeout=60) as r:
            roh = json.loads(r.read().decode('utf-8'))
    except Exception as ausnahme:
        fehler.merken('injektion.scdl_holen', ausnahme, datei)
        return False, None
    neu_kennung = (roh.get('_meta') or {}).get('version')
    if not roh.get('entries') or neu_kennung == alt:
        return False, alt
    # Schon mal ablegen — der Abruf ist gelaufen, ein zweiter wäre Verschwendung.
    try:
        ziel = pfade.app_datei(SCDL_CACHE % sprachkuerzel)
        with open(ziel + '.tmp', 'w', encoding='utf-8') as f:
            json.dump(roh, f, ensure_ascii=False)
        os.replace(ziel + '.tmp', ziel)
    except Exception:
        return False, alt
    return True, neu_kennung


def scdl_stand(sprachkuerzel):
    """Welche Version der Vertragsdaten liegt hier? Oder None."""
    d = scdl_laden(sprachkuerzel)
    return (d.get('_meta') or {}).get('version') if d else None


def entfernen(ini_pfad, sprache='english'):
    """Alle Einfügungen zurücknehmen — die Datei bleibt sonst unverändert.

    ⚠ Zurück heißt: so, wie der Spieler die Datei hatte. Hat der SC Deutsch
    Launcher oder StarStrings dort etwas stehen, bleibt das stehen — der Urtext
    bewahrt es."""
    return einspielen(ini_pfad, sprache, nur_entfernen=True)


def ist_drin(ini_pfad):
    """Steckt in dieser Datei schon eine Injektion?

    Seit v3.0.0 stehen keine Marken mehr im Text (sie waren im Spiel sichtbar),
    also wird nach der **Form** der Einfügung gesucht. Die alte Marke gilt
    weiter — in der Datei von jemandem, der von einer früheren Version kommt,
    steht sie noch.

    ⚠ Gesucht wird nur nach **eindeutig eigenen** Formen. Hier stand der blanke
    Titelzusatz `<EM4>[BP]</EM4>` — und den schreiben MrKrakens StarStrings und
    der SC Deutsch Launcher genauso. Wer eines von beiden benutzte, bekam „steht
    schon drin" gemeldet, ohne dass der Watcher je etwas eingetragen hätte.
    Auch die Block-Überschrift zählt nur **mit Kästchen** — ohne sie stammt der
    Block aus derselben Quelle, aber von fremder Hand.

    Nur die ersten Zeilen zu lesen genügt nicht: Die Auftragstexte liegen mitten
    in einer Datei mit über hunderttausend Zeilen.
    """
    try:
        with open(ini_pfad, encoding='utf-8', errors='ignore') as f:
            for zeile in f:
                if AUF in zeile or ZAEHLENDER_TITEL.search(zeile):
                    return True
                # ⚠ Die Überschrift allein genügt nicht: Der SC Deutsch Launcher
                # schreibt dieselbe, aus derselben Quelle. Erst das Kästchen
                # macht den Block zu unserem.
                if EIGENER_NACHWEIS.search(zeile) and _hat_kaestchen(zeile):
                    return True
    except OSError:
        pass
    return False


# ---------------------------------------------------------------------------
# ⚠ Diese beiden Funktionen lagen bis zum 28.08.2026 als Methoden im
# Einstellungsfenster. Damit war der Zustand der Injektion nur zu erfahren,
# wenn ein Fenster offen war — der **Diagnosebericht** kam nicht heran.
#
# Das kostete echte Zeit: Als Morkhan am 28.08. meldete, er sehe die
# Bauplan-Angaben im Spiel nicht mehr, stand in seinem Bericht nur
# `inj_quelle=deutsch`. Ob überhaupt etwas eingetragen war, ließ sich daraus
# nicht ablesen — es musste erschlossen werden. Die Antwort lag im Programm
# vor, nur nicht dort, wo man im Fehlerfall nachsieht.
#
# Jetzt stehen sie frei, und Fenster wie Bericht fragen dieselbe Stelle.

def _sprachreihenfolge(rueckfall=('english', 'german_(germany)')):
    """In welcher Reihenfolge die Sprachordner geprüft werden.

    Vorn steht, was in der `user.cfg` als `g_language` eingetragen ist — das
    ist die Datei, die das Spiel wirklich liest. Steht dort nichts, bleibt es
    beim Rückfall: Ohne Eintrag startet Star Citizen auf Englisch, dann ist die
    bisherige Reihenfolge richtig.
    """
    from . import uebersetzung
    ordnung = list(rueckfall)
    try:
        sprache = uebersetzung.spielsprache()
    except Exception as ausnahme:
        fehler.merken('injektion.spielsprache', ausnahme)
        return ordnung
    if not sprache:
        return ordnung
    if sprache in ordnung:
        ordnung.remove(sprache)
    return [sprache] + ordnung


def ini_datei():
    """Die `global.ini`, um die es geht. (Pfad, Sprachordner, Quelle).

    ⚠ Maßgeblich ist die **gewählte** Textquelle, nicht die zuerst gefundene.
    Hier stand eine feste Reihenfolge: erst „deutsch", dann „starstrings", und
    die erste eingerichtete gewann. Wer beide einmal benutzt hatte und dann auf
    StarStrings umstellte, bekam trotzdem weiter „Quelle: Deutsch (rjcncpt)"
    angezeigt — die deutsche war ja auch noch eingerichtet. Genau so gemeldet.
    Die Reihenfolge greift nur, solange nichts gewählt wurde.
    """
    from . import uebersetzung
    gewaehlt = pfade.einstellung('inj_quelle')
    reihenfolge = ['deutsch', 'starstrings']
    if gewaehlt in reihenfolge:
        reihenfolge.remove(gewaehlt)
        reihenfolge.insert(0, gewaehlt)
    elif gewaehlt == 'original':
        # Die Originaltexte kommen aus dem Spiel selbst, nicht aus einem
        # fremden Projekt — dort gibt es keine Version zu vermerken.
        #
        # ⚠⚠ **Die Spielsprache entscheidet, nicht die Reihenfolge.** Hier stand
        # fest `('english', 'german_(germany)')`, und die erste vorhandene Datei
        # gewann — beide gibt es fast immer, also **immer Englisch**. Wer sein
        # Spiel auf Deutsch stellt (`g_language = german_(germany)` in der
        # `user.cfg`), bekam die Angaben in die englische Datei geschrieben, die
        # das Spiel nie liest. Eingetragen wurde korrekt, angekommen ist nichts,
        # und die Statuszeile meldete trotzdem Erfolg. Am 29.08.2026 gemeldet.
        for sprache_ordner in _sprachreihenfolge():
            pfad = uebersetzung.ziel_ini(sprache_ordner)
            if pfad and os.path.isfile(pfad):
                return pfad, sprache_ordner, None
    for quelle in reihenfolge:
        if uebersetzung.installiert(quelle):
            sprache_ordner = uebersetzung.QUELLEN[quelle]['sprache']
            return uebersetzung.ziel_ini(sprache_ordner), sprache_ordner, quelle
    # Nichts vermerkt: dann die Datei nehmen, die tatsächlich daliegt — aber in
    # der Reihenfolge, die das Spiel vorgibt. Hier stand `german_(germany)`
    # zuerst; für dieses eine Haus richtig, für jeden mit englischem Spiel
    # falsch. Geraten wird nicht mehr.
    for sprache_ordner in _sprachreihenfolge(('german_(germany)', 'english')):
        p = uebersetzung.ziel_ini(sprache_ordner)
        if p and os.path.isfile(p):
            return p, sprache_ordner, None
    return None, 'english', None


def lage():
    """Steht etwas im Spiel, und aus welcher Quelle? (dict)"""
    from . import uebersetzung
    pfad, _sprache, quelle = ini_datei()
    da = bool(pfad and os.path.isfile(pfad))
    drin = bool(da and ist_drin(pfad))
    return {'datei': pfad, 'drin': drin, 'quelle': quelle,
            'stand': uebersetzung.installiert(quelle) if quelle else None}
