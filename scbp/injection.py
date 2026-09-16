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

from . import specs
from . import asop as asop_modul
from . import fehler, collection as bestand_datei
from . import catalog as katalog_modul
from . import pfade
from .language import t

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
SCDL_RAW = ('https://raw.githubusercontent.com/rjcncpt/'
            'StarCitizen-Deutsch-INI/master/blueprints/Data/%s')
SCDL_FILE = {'de': 'bp-contracts_short.json',
              'en': 'bp-contracts_short_en.json'}
SCDL_CACHE = 'bp-contracts-%s.json'
BP_LINE = re.compile(r'^(\s*)- (.+)$')

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
HEADING_LINE = re.compile(r'^\s*#')
BP_HEADING = re.compile(r'^\s*#\s*(?:Baupläne|Blueprints)', re.I)

# Die Marken. Bewusst unauffällig und ohne Sonderzeichen, damit sie das Spiel
# nicht stören, aber eindeutig genug, um sie sicher wiederzufinden.
OPEN = '[SCBPW]'
CLOSE = '[/SCBPW]'
MARK = re.compile(re.escape(OPEN) + '.*?' + re.escape(CLOSE))

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
_HEADINGS = (
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
COUNTING_TITLE = re.compile(r'<EM4>\[(?:BP|Bauplan)(?:\s+\d+/\d+|!)\]</EM4>')

# Eine Bauplan-Marke am Titel — **von wem auch immer**. Deckt die eigene Form
# `[BP]`/`[BP!]`, die alte `[BP 3/12]`, MrKrakens kombinierte
# `<EM4>[10 Rep] [BP]</EM4>` und die des SC Deutsch Launchers ab.
TITLE_MARK = re.compile(r'<EM4>[^<>]*\[(?:BP|Bauplan)[^\]]*\][^<>]*</EM4>')

OWN_TRACE = re.compile(
    '|'.join(re.escape(u) for u in _HEADINGS))

# Derselbe Notnagel **ohne** den Titelzusatz — für Grundlagen, die die Titel
# selbst kennzeichnen (StarStrings). Dort setzt der Watcher gar keinen
# Titelzusatz, also gibt es dort auch keinen von ihm zu entfernen; was am Titel
# steht, gehört MrKraken. Der Block darunter dagegen ist unserer und muss weg.
UNMARKED_BLOCK = re.compile(
    # ⚠ `<EM\d>` wie bei OHNE_MARKE — siehe die Begründung dort.
    r'(?:\\n){1,2}?\s*-{20,}(?:\\n|\s|<EM\d>)*(?:%s).*$'
    % '|'.join(re.escape(u) for u in _HEADINGS), re.S)

UNMARKED = re.compile(
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
    % '|'.join(re.escape(u) for u in _HEADINGS), re.S)

# Aufbau nach dem Vorbild des SC Deutsch Launchers — die **Gliederung** ist die
# nützliche Erkenntnis (was ein Spieler vor dem Annehmen wissen will), die
# Formulierungen sind eigene. Alle Angaben stammen aus scmdb.
TEXTS = {
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
        # ⚠ Kurz halten: Die Zeile traegt schon Fraktionsnamen und Art
        # („Citizens For Prosperity +100 Standing"), und sie steht in einer
        # Spalte, die das Spiel nicht umbricht.
        'ruf_bei':   'Ruf',
        # ⚠ Wortgleich mit dem, was die Quelle selbst schreibt („# Zu
        # erwartende Rufpunkte: 150 XP") — sonst stuenden im Spiel zwei
        # verschiedene Bezeichnungen fuer dieselbe Angabe.
        'ruf_erwartet': 'Zu erwartende Rufpunkte',
        'keine_angabe': 'Keine Angaben',
        'cooldown':  'Wartezeit',
        'minuten':   'Minuten',
        'teilbar':   'Mission teilbar',
        'ja':        'Ja', 'nein': 'Nein',
        'liste':     'Baupläne — angehakt ist, was du hast',
        'ab_rang':   'erst ab',
        'leere_stufen': ('Achtung: %d der %d Stufen dieses Auftrags geben '
                         'gar keine Baupläne.'),
        'quelle':    'Angaben von scmdb.net · eingefügt von VerseKit',
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
        'ruf_bei':   'Reputation',
        'ruf_erwartet': 'Expected reputation',
        'keine_angabe': 'No data',
        'cooldown':  'Cooldown',
        'minuten':   'minutes',
        'teilbar':   'Shareable',
        'ja':        'Yes', 'nein': 'No',
        'liste':     'Blueprints — ticked means you have it',
        'ab_rang':   'needs',
        'leere_stufen': ('Note: %d of the %d tiers of this contract give no '
                         'blueprints at all.'),
        'quelle':    'Data from scmdb.net · added by VerseKit',
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
FOREIGN_APPENDIX = re.compile(
    r'(?:\\n\s*){2}(?:'
    r'-{3,20}\s*[A-Za-z][A-Za-z ]{1,30}\s*-{3,20}'      # --- STATS ---
    r'|={2,20}\s*[A-Za-z][A-Za-z ]{1,30}\s*={2,20}'     # == Stats ==
    r'|<EM\d>\s*(?:={2,20}\s*)?[A-Za-z][A-Za-z ]{1,30}'  # <EM3>MISSION DETAILS…
    r'(?:\s*={2,20})?\s*</EM\d>'
    r')')


def _append_block(base_text, block):
    """Unseren Block anhängen — aber **vor** einem fremden Anhang, wenn da einer ist.

    Ohne Fremdanhang ist das schlichtes `grundlage + block`, wie bisher.
    Steht dahinter fremder Text im Blockformat, schiebt sich unserer davor.

    ⚠ Der Unterschied ist nicht kosmetisch: Werkzeuge, die ihren eigenen Block
    abräumen, schneiden „ab dem eigenen Marker bis zum Ende". Alles davor
    überlebt, alles dahinter nicht.
    """
    hit = FOREIGN_APPENDIX.search(base_text)
    if not hit:
        return base_text + block
    return base_text[:hit.start()] + block + base_text[hit.start():]


BOX_HAVE = '<EM4>[x]</EM4>'
BOX_MISSING = '[  ]'
LINE = '-' * 57


def _lang_code(language):
    """`german_(germany)` -> 'de', alles andere -> 'en'."""
    return 'de' if str(language).lower().startswith('german') else 'en'


def _split_line(line):
    """'SCHLUESSEL,P=Text' -> ('SCHLUESSEL', ',P', 'Text'). Sonst None."""
    sep = line.find('=')
    if sep < 1:
        return None
    head, text = line[:sep], line[sep + 1:]
    if ',' in head:
        key, _, suffix = head.partition(',')
        return key, ',' + suffix, text
    return head, '', text


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
ORIGTEXT_FILE = 'injektion-urtext.json'


def _origtext_file():
    """Der ganze Inhalt der Merkdatei — leer, wenn es sie nicht gibt."""
    try:
        with open(pfade.app_datei(ORIGTEXT_FILE), encoding='utf-8') as f:
            data = json.load(f)
        return data if isinstance(data, dict) else {}
    except Exception:
        return {}


def load_origtext():
    """Die gemerkten Originaltexte — leer, wenn es noch keine gibt."""
    return _origtext_file().get('texte') or {}


def is_fresh():
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
    return bool(_origtext_file().get('frisch'))


def discard_origtext():
    """Die gemerkten Originaltexte wegwerfen und die Datei als frisch merken.

    Gehört zu **jedem** Einsetzen einer neuen Grundlage (`translation.fetch()`):
    Die alten Merktexte gehören zur alten Datei und würden auf einen überholten
    Stand zurückschreiben; das Kennzeichen `frisch` schützt den fremden Text
    beim ersten Lauf (siehe `ist_frisch()`)."""
    try:
        target = pfade.app_datei(ORIGTEXT_FILE)
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(target, 'w', encoding='utf-8') as f:
            json.dump({'stand': time.strftime('%Y-%m-%d %H:%M:%S'),
                       'frisch': True, 'texte': {}}, f, ensure_ascii=False)
        return True
    except Exception as exc:
        fehler.merken('injection.discard_origtext', exc)
        return False


def save_origtext(texts_map, ini_path):
    """Die Originaltexte festhalten. Fehlschlag ist kein Grund abzubrechen."""
    try:
        target = pfade.app_datei(ORIGTEXT_FILE)
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(target, 'w', encoding='utf-8') as f:
            json.dump({'datei': ini_path, 'stand': time.strftime('%Y-%m-%d %H:%M:%S'),
                       'texte': texts_map}, f, ensure_ascii=False)
        return True
    except Exception as exc:
        fehler.merken('injection.save_origtext', exc)
        return False


def _strip_old(text, key='', origtext=None, fallback=UNMARKED):
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
    if origtext and key in origtext:
        return origtext[key]
    if OPEN in text:
        return MARK.sub('', text).rstrip()
    if not fallback:
        return text
    hit = fallback.search(text)
    if hit:
        drop = text[hit.start():]
        # ⚠ Ein Block mit unserer Überschrift, aber **ohne Kästchen**, ist nicht
        # unserer — er kommt aus derselben Quelle, gesetzt vom SC Deutsch
        # Launcher. Der bleibt stehen; er gehört dem Spieler.
        if OWN_TRACE.search(drop) and not _has_box(drop):
            return text
        # ⚠ Der Notnagel schneidet „ab hier bis zum Ende" — seit unser Block
        # **vor** einem fremden Anhang sitzt (`_anhaengen`), läge der fremde
        # Text mit im Schnitt. Also nur bis dorthin schneiden und den Rest
        # wieder anfügen. Ohne das nähme das Zurücksetzen Smart Citizens
        # Stats-Blöcke mit — genau der Schaden, den wir gerade verhindern.
        foreign = FOREIGN_APPENDIX.search(drop)
        rest = drop[foreign.start():] if foreign else ''
        return text[:hit.start()].rstrip() + rest
    return text


def _group_digits(value, words):
    """1234567 -> '1.234.567' bzw. '1,234,567' — je nach Sprache."""
    return format(int(value), ',d').replace(',', words['trenner'])


def _build_block(entry, owned, words):
    """Der Textblock, der an die Beschreibung gehängt wird.

    Erst die Eckdaten als kurze Liste, dann die Baupläne mit Kästchen. Die
    Reihenfolge ist Absicht: Ob sich der Auftrag überhaupt lohnt, entscheidet
    man an Chance und Reputation — die Namensliste liest man erst danach."""
    z = ['', LINE, '', '<EM4>%s</EM4>' % words['ueberschr'], '']

    chance = entry.get('chance')
    if chance:
        z.append('# %s: %d%%' % (words['chance'], round(chance * 100)))
    if entry.get('rang'):
        z.append('# %s: %s (%s XP)' % (words['rep_min'], entry['rang'],
                                       _group_digits(entry.get('rep') or 0, words)))
    if entry.get('rang_max'):
        z.append('# %s: %s (%s XP)' % (words['rep_max'], entry['rang_max'],
                                       _group_digits(entry.get('rep_max') or 0, words)))
    if entry.get('uec'):
        z.append('# %s: %s aUEC' % (words['lohn'], _group_digits(entry['uec'], words)))
    if entry.get('ruf'):
        z.append('# %s: %s XP' % (words['ruf'], _group_digits(entry['ruf'], words)))
    if entry.get('cooldown'):
        z.append('# %s: %s %s' % (words['cooldown'],
                                  _group_digits(entry['cooldown'], words),
                                  words['minuten']))
    if 'teilbar' in entry:
        z.append('# %s: %s' % (words['teilbar'],
                               words['ja'] if entry['teilbar'] else words['nein']))

    # ⚠ **Ohne „3 von 12", seit dem 28.08.2026** — aus demselben Grund wie im
    # Titel (siehe `_titel_zusatz`): Die Liste führt alle Preisstufen zusammen,
    # die Zahl wäre eine Behauptung über etwas, das gar nicht auflösbar ist.
    # Die Kästchen sagen dasselbe, nur ehrlich: angehakt heißt „hab ich".
    z += ['', '# ' + words['liste'] + ':']

    # Wo sich die Stufen unterscheiden, steht der nötige Rang hinter dem Namen.
    # ⚠ Das ist **nur Text**. Es blendet nichts aus und hakt nichts anders ab:
    # Wer den Bauplan hat, hat ihn — auch wenn diese Stufe ihn nicht hergibt.
    frm = entry.get('ab') or {}
    width = max([len(n) for n in entry['bp']] or [0])
    for name in entry['bp']:
        inside = katalog_modul._norm(name) in owned
        cond = frm.get(name)
        line = '   %s %s' % (BOX_HAVE if inside else BOX_MISSING, name)
        if cond:
            line += '%s  %s %s (%s XP)' % (' ' * (width - len(name)),
                                            words['ab_rang'], cond['rang'],
                                            _group_digits(cond['rep'], words))
        z.append(line)

    # Gibt es Stufen dieses Auftrags, die leer ausgehen, gehört das dazu —
    # sonst fliegt jemand für eine Liste hin, die seine Stufe nie hergibt.
    # Genau das ist Morkhan am 28.08.2026 passiert.
    if entry.get('leer'):
        z += ['', '# ' + (words['leere_stufen']
                          % (entry['leer'], entry['stufen']))]
    z += ['', words['quelle']]
    # Ohne Marken — sie standen sichtbar im Spiel. Zurückgesetzt wird über die
    # gemerkten Originaltexte (siehe `URTEXT_DATEI`).
    return '\\n' + '\\n'.join(z)


# Ein Kürzel, das am Anfang eines Namens schon dasteht — `[CS1] Spark-G Missile`.
# So schreibt MrKraken seine Angaben (136 Namen in der Fassung vom 29.08.2026),
# der Watcher setzt seine dahinter in runde Klammern. Ohne diese Prüfung stünde
# im Spiel `[CS1] Spark-G Missile (CS1)`.
FOREIGN_TAG = re.compile(r'^\[[A-Za-z0-9/. -]{1,14}\]\s')


def _has_box(text):
    """Steht in diesem Stück ein Kästchen von uns?

    **Das ist das Unterscheidungsmerkmal.** Watcher und SC Deutsch Launcher
    schöpfen aus derselben Quelle (`bp-contracts_short.json` des SCDL-Teams) und
    schreiben deshalb wortgleiche Blöcke — dieselbe Überschrift, dieselbe Liste.
    Der einzige Unterschied ist der, der das Werkzeug ausmacht: In den Rohdaten
    steht `    - Atzkav Sniper Rifle`, bei uns `    [x] Atzkav Sniper Rifle`.
    Wo kein Kästchen steht, hat nicht der Watcher geschrieben."""
    return '[x]' in text or BOX_MISSING in text


def _has_details(text):
    """Stehen die Auftragsangaben schon im Text?

    ⚠⚠ **Fremder Text wird nicht verdoppelt.** MrKraken StarStrings schreibt
    bei denselben Auftraegen eine eigene Reputationszeile; wo eine steht,
    kommt keine zweite dazu — dieselbe Regel wie bei der `[BP]`-Marke.
    Entschieden am 05.09.2026: „Nicht schreiben, wenn MrKraken schon da ist."

    ⚠ Erkannt wird an den Schlagwoertern beider Werkzeuge, nicht an unserem
    Wortlaut allein: Sonst gaelte fremder Text als „noch nichts da", und der
    Spieler haette die Angabe zweimal untereinander.
    """
    plain = (text or '').replace(COLOR_OPEN, '').replace(COLOR_CLOSE, '')
    lowered = plain.lower()
    return any(word in lowered for word in (
        'rufpunkte', 'cooldown', 'reputation awarded', 'reputation gain',
        'abklingzeit'))


def _has_title_mark(text):
    """Trägt dieser Titel schon eine Bauplan-Marke — von wem auch immer?

    Drei Werkzeuge schreiben dieselbe: der Watcher, MrKrakens StarStrings
    (`<EM4>[BP]</EM4>`, auch als `<EM4>[10 Rep] [BP]</EM4>`) und der SC Deutsch
    Launcher (die Rohdaten geben `title` = ` <EM4>[BP]</EM4>` für 369 der 818
    Aufträge vor). Steht sie schon da, kommt keine zweite dazu — egal, wer sie
    gesetzt hat. Sie bedeutet ohnehin dasselbe: hier gibt es Baupläne."""
    return bool(TITLE_MARK.search(text))


def _split_foreign_block(text):
    """Einen fremden Bauplan-Block abtrennen: (Text ohne ihn, der Block).

    Fremd heißt: unsere Überschrift, aber **keine Kästchen** — also der Block
    des SC Deutsch Launchers, der aus derselben Quelle stammt. Er wird nicht
    stehengelassen (sonst stünde die Liste zweimal untereinander) und nicht
    verworfen (er gehört dem Spieler), sondern **ersetzt**: Unserer tritt an
    seine Stelle, und weil der Urtext ihn behält, kommt er beim Zurücksetzen
    wieder."""
    hit = UNMARKED_BLOCK.search(text)
    if hit and not _has_box(text[hit.start():]):
        return text[:hit.start()].rstrip(), text[hit.start():]
    return text, ''


def _fallback_form(origtext_old, ini_path):
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
    if is_fresh() or origtext_old:
        return None
    return UNMARKED if is_applied(ini_path) else UNMARKED_BLOCK


def _title_suffix(entry, owned, words):
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
    chars = '!' if (entry.get('bpnote') or '').strip() else ''
    return ' <EM4>[%s%s]</EM4>' % (words['kurz'], chars)


def scdl_fetch(lang_code, progress=None):
    """Die Vertragsdaten des SCDL-Teams holen und ablegen. (Erfolg, Anzahl)."""
    from .catalog import OFF
    filename = SCDL_FILE.get(lang_code)
    if not filename or OFF:          # ⚠ SC_BP_NO_NET gilt auch hier
        return False, 0
    try:
        if progress:
            progress('Bauplan-Daten werden geladen …')
        req = urllib.request.Request(
            SCDL_RAW % filename,
            headers={'User-Agent': 'SC-BP-Watcher'})
        with urllib.request.urlopen(req, timeout=60) as r:
            raw = json.loads(r.read().decode('utf-8'))
        entries = raw.get('entries') or []
        if not entries:
            return False, 0
        target = pfade.app_datei(SCDL_CACHE % lang_code)
        with open(target + '.tmp', 'w', encoding='utf-8') as f:
            json.dump(raw, f, ensure_ascii=False)
        os.replace(target + '.tmp', target)
        return True, len(entries)
    except Exception:
        return False, 0


def scdl_load(lang_code):
    """Die abgelegten Vertragsdaten — oder None."""
    try:
        with open(pfade.app_datei(SCDL_CACHE % lang_code),
                  encoding='utf-8') as f:
            raw = json.load(f)
        return raw if raw.get('entries') else None
    except Exception:
        return None


# Name der Einstellung, mit der sich die Angaben am Gegenstand abschalten
# lassen. Standard ist **an**: Wer die Injektion einschaltet, will Angaben im
# Spiel sehen — und genau dafür ist dieses Werkzeug da.
SETTING_DETAILS = 'angaben_am_gegenstand'


def _name_table(lines, remove_only=False):
    """Tabelle *Namensschlüssel → Kürzel* — oder leer, wenn abgeschaltet.

    Beim reinen Entfernen bleibt sie leer: Dann stellt der Urtext-Weg die
    ursprünglichen Namen wieder her, und es soll nichts Neues dazukommen."""
    if remove_only or not pfade.einstellung_wahrheit(SETTING_DETAILS, True):
        return {}
    try:
        return specs.build_table(lines)
    except Exception as exc:
        fehler.merken('injection._name_table', exc)
        return {}


def _asop_table(lines):
    """Tabelle *Fahrzeugschlüssel → (eigener Name, Stern)* — oder leer.

    Eigene Fehler dürfen die Injektion nicht anhalten: Wer seine Schiffe nicht
    umbenannt hat, soll trotzdem seine Bauplan-Angaben bekommen."""
    try:
        return asop_modul.build_table(lines)
    except Exception as exc:
        fehler.merken('injection._asop_table', exc)
        return {}


def _name_with_detail(text, tag):
    """Den Zusatz an einen Namen hängen — vorhandene Klammer vorher abschneiden.

    Der SC Deutsch Launcher hängt seinerseits `(CS1)` an. Ohne das Abschneiden
    stünde danach `Spark I-G Missile (CS1) (IR1)` im Spiel.

    ⚠ Steht das Kürzel schon **vorn** (`[CS1] Spark-G Missile`), bleibt der Name
    unangetastet. Das ist MrKrakens Schreibweise, und es ist dieselbe Angabe —
    sie ein zweites Mal anzuhängen, macht den Namen nur länger und falscher."""
    if FOREIGN_TAG.match(text):
        return text
    return '%s %s' % (specs.strip_tag(text).rstrip(), tag)


def stock_mark(stock=None):
    """Ein kurzer Fingerabdruck des eigenen Bestands.

    ⚠ Wozu: Die Kästchen in den Auftragstexten zeigen, was der Spieler schon
    hat. Ändert sich sein Bestand, müssen sie neu geschrieben werden — sonst
    stimmen sie ab dem nächsten Fund nicht mehr. Ein Vergleich dieser Marke
    beantwortet die Frage „hat sich seit dem letzten Einspielen etwas getan?"
    ohne die Texte jedes Mal neu zu bauen.

    ⚠ Über die **Namen**, nicht über die Anzahl: `collection.align()` benennt
    beim Start Einträge um, ohne dass die Zahl sich ändert — und genau die
    Namen stehen in den Kästchen.
    """
    import hashlib
    from . import collection as bestand_datei
    names = bestand_datei.keys(
        stock if stock is not None else bestand_datei.load())
    raw = '\n'.join(sorted(names)).encode('utf-8', 'replace')
    return '%d-%s' % (len(names), hashlib.sha1(raw).hexdigest()[:12])


def _set_boxes(text, owned):
    """In einem fertigen SCDL-Block die Bauplan-Zeilen ankreuzen.

    Aus `    - Atzkav Sniper Rifle` wird `    [x] Atzkav Sniper Rifle`, wenn er
    im Bestand liegt — sonst `    [  ] …`. Der übrige Text bleibt unangetastet;
    er gehört dem SCDL-Team, wir hängen nur das Häkchen dran.

    ⚠ **Nur unter der Bauplan-Überschrift.** Abgabeorte und Regionen stehen im
    selben Block als Liste; sie anzukreuzen ergibt keinen Sinn (siehe
    `BP_UEBERSCHRIFT`).

    Gezählt wird nebenbei, damit das Titel-Kürzel dieselbe Zahl zeigt."""
    lines = text.split('\\n')
    mine = total = 0
    in_list = False
    for i, line in enumerate(lines):
        if HEADING_LINE.match(line):
            in_list = bool(BP_HEADING.match(line))
            continue
        if not in_list:
            continue
        m = BP_LINE.match(line)
        if not m:
            continue
        indent, name = m.group(1), m.group(2).strip()
        if name.startswith('#') or not name:
            continue
        total += 1
        inside = katalog_modul._norm(name) in owned
        if inside:
            mine += 1
        lines[i] = '%s%s %s' % (indent, BOX_HAVE if inside else BOX_MISSING, name)
    return '\\n'.join(lines), mine, total


# Die Auszeichnung, mit der das Spiel Text hervorhebt — dasselbe Blau, in dem
# auch die `[BP!]`-Marke steht.
#
# ⚠⚠ **Gewuenscht am 05.09.2026:** „Mach die XP blau geschrieben … damit
# allgemein spieler es schneller sehen." Der Anlass war ein Melder, der die
# Rufpunkte uebersah, weil sie mitten im uebrigen Text standen — sie waren da,
# nur unauffaellig.
#
# ⚠ Gemessen in der `global.ini` eines Spielers: `<EM4>` kommt 3.974 Mal vor,
# die uebrigen Stufen zusammen achtmal. Es ist die Auszeichnung, die das Spiel
# wirklich benutzt — nicht geraten.
COLOR_OPEN = '<EM4>'
COLOR_CLOSE = '</EM4>'


def _highlight(line):
    """Eine Zeile hervorheben — aber nur, wenn sie es nicht schon ist.

    ⚠ Doppelte Auszeichnung zeigt das Spiel als Text an: Aus zwei `<EM4>`
    wird kein kraeftigeres Blau, sondern ein sichtbares `<EM4>` im Fenster.
    """
    line = line.strip()
    if not line or COLOR_OPEN in line:
        return line
    return '%s%s%s' % (COLOR_OPEN, line, COLOR_CLOSE)


REP_WORDS = ('reputation', 'rufpunkte')


def _highlight_rep(block):
    """Die Ruf-Zeilen im eigenen Block blau setzen.

    ⚠⚠ **Warum das noetig ist (06.09.2026).** Die Rohdaten liefern zwei
    getrennte Felder: `contractInfo` (Rufpunkte, Abklingzeit, Teilbarkeit) und
    `description` (der Bauplan-Block). Die Zeilen aus `contractInfo` faerben
    wir seit v3.17.0 blau — in `description` stehen aber ZWEI WEITERE
    Ruf-Zeilen, und die uebernahmen wir unveraendert, also ungefaerbt:

        # Min. Reputation: Auftragnehmer Junior (800 XP)
        # Max. Reputation: Auftragnehmer Elite (95.250 XP)

    Gemessen in einer echten `global.ini`: 435 Zeilen `# Min. Reputation`,
    435 `# Max. Reputation`, 129 `# Min. / Max. Reputation` — alle in Weiss,
    mitten zwischen unseren blauen. Genau das war gemeldet worden: „da ist
    keine Reputation in den Questtexten", weil sie im uebrigen Text unterging.

    ⚠ **Das ist kein Eingriff in fremde Arbeit.** Diese Zeilen stehen in dem
    Block, den der Watcher selbst einsetzt; sie stammen aus derselben Quelle
    wie der Rest. Wo ein anderes Werkzeug seinen eigenen Block geschrieben hat
    (erkennbar an fehlenden Kaestchen), wird hier nichts angefasst — der
    Aufrufer setzt die Kaestchen unmittelbar davor.

    ⚠ Nur Ruf-Zeilen. `# Baupläne:` und `# Region:` bleiben schwarz: Sie
    gliedern den Block, sie sind keine Angabe. Waere alles blau, waere nichts
    hervorgehoben.
    """
    lines = (block or '').split('\\n')
    for i, line in enumerate(lines):
        bare = line.strip()
        if not bare.startswith('#') or COLOR_OPEN in line:
            continue
        if any(w in bare.lower() for w in REP_WORDS):
            lines[i] = _highlight(bare)
    return '\\n'.join(lines)


def _detail_lines(entry, present='', words=None, rep_table=None):
    """Die Angabezeilen eines Auftrags — hervorgehoben und ohne Dubletten.

    ⚠ Verglichen wird gegen den Text OHNE Auszeichnung: Sonst gilt eine Zeile
    als neu, nur weil sie beim letzten Lauf noch ungefaerbt war — und stuende
    danach zweimal da.

    ⚠⚠ **Die Ruf-Zeile kommt aus einer ANDEREN Quelle** (`reputation`). Die
    Vertragsdaten nennen die Rufpunkte nur als Zahl; bei WEM sie anfallen und
    ob es Standing, Affinity oder Bounty Hunting ist, steht dort in keinem
    einzigen Feld — gemessen an allen 818 Eintraegen. Gewuenscht wurde genau
    diese Unterscheidung: „auf SCMDB sieht man auch ob es Standing oder Rep
    bekommt, das muss auf jeden fall mit in den Questtext."
    """
    plain = (present or '').replace(COLOR_OPEN, '').replace(COLOR_CLOSE, '')
    out = []
    for field in ('contractInfo', 'dropChance'):
        for line in (entry.get(field) or '').split('\\n'):
            line = line.strip()
            if line and line not in plain:
                out.append(_highlight(line))

    if rep_table is not None:
        try:
            from . import reputation
            line = reputation.line(
                entry.get('titleLocKey') or '',
                (words or {}).get('ruf_bei') or 'Ruf', rep_table)
            # ⚠ Der Dublettenschutz vergleicht nur den ANFANG bis zum
            # Doppelpunkt: Der Rest wechselt mit den Zahlen, und nach einem
            # Patch stuenden sonst zwei Ruf-Zeilen untereinander.
            if line and line.split(':')[0] not in plain:
                out.append(_highlight(line))
        except Exception as exc:
            fehler.merken('injection.ruf_zeile', exc)

    # ⚠⚠ **Lieber „keine Angaben" als gar nichts (06.09.2026).** 109 Auftraege
    # bekamen ueberhaupt keine Ruf-Zeile — die Quelle fuehrt fuer sie keine
    # Rufwerte (CleanAir-Kurierfahrten und aehnliche). Im Spiel stand dort
    # dann nur Abklingzeit und Teilbarkeit, und die Luecke sah aus wie ein
    # Aussetzer des Werkzeugs. Genau diese Frage kam auf: „da ist keine
    # Reputation in den Questtexten."
    #
    # Eine fehlende Zeile und eine leere Angabe sehen gleich aus, meinen aber
    # Verschiedenes. Steht sie da, weiss der Spieler: nachgesehen wurde, es
    # gibt schlicht nichts. Dieselbe Zurueckhaltung wie beim Zustand
    # `mission_log.EXPIRED` — feststellen, nicht behaupten.
    #
    # ⚠ Nur wenn WIRKLICH keine steht — weder eine eigene noch eine, die
    # schon im Text ist. Sonst stuenden zwei Ruf-Zeilen untereinander, eine
    # davon leer.
    _has_rep = any(any(w in z.lower() for w in REP_WORDS) for z in out)
    if not _has_rep and not any(w in plain.lower() for w in REP_WORDS):
        out.insert(0, _highlight('# %s: %s' % (
            (words or {}).get('ruf_erwartet') or 'Zu erwartende Rufpunkte',
            (words or {}).get('keine_angabe') or 'Keine Angaben')))
    return out


def _contract_details(block, entry, words=None, rep_table=None):
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
    suffix = _detail_lines(entry, block, words, rep_table)
    if not suffix:
        return block

    lines = block.split('\\n')
    # Vor die Bauplan-Überschrift, sonst ans Ende der Kopfzeilen.
    pos = None
    for i, line in enumerate(lines):
        if BP_HEADING.match(line):
            pos = i
            break
    if pos is None:
        return '\\n'.join(lines + [''] + suffix)
    # Eine Leerzeile davor, wenn dort nicht schon eine steht — sonst kleben
    # die neuen Zeilen an der Reputationsangabe.
    before = suffix + ['']
    if pos > 0 and lines[pos - 1].strip():
        before = [''] + before
    lines[pos:pos] = before
    return '\\n'.join(lines)


def _stem(key):
    """Der Namensanfang, den Titel und Beschreibungen eines Auftrags teilen.

    Aus `Covalex_HaulCargo_AToB_title` und `Covalex_HaulCargo_AtoB_desc_ToRuinStation`
    wird beide Male `covalex_haulcargo_atob`. Alles ab `_title` bzw. `_desc` fällt
    weg, der Rest wird kleingeschrieben — in den Spieldaten wechselt die
    Schreibweise mitten im Wort.
    """
    lowered = (key or '').lower()
    for sep in ('_title', '_desc'):
        pos = lowered.find(sep)
        if pos > 0:
            return lowered[:pos]
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
SERIES_SUFFIX = re.compile(r'^[A-Za-z0-9]{1,2}$')


def _series_stem(stem, known):
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
    if not stem or stem in known:
        return None
    best = None
    for candidate in known:
        if candidate == stem or not stem.startswith(candidate):
            continue
        rest = stem[len(candidate):]
        if not rest or not SERIES_SUFFIX.match(rest):
            continue
        if best is None or len(candidate) > len(best):
            best = candidate
    return best


def apply_scdl(ini_path, lang_code, stock=None):
    """Injektion aus den SCDL-Vertragsdaten — der vollständigere Weg.

    Gibt (Erfolg, Anzahl, Meldung) zurück wie `einspielen()`."""
    data = scdl_load(lang_code)
    if not data:
        return False, 0, t('m_keine_scdl')
    if not ini_path or not os.path.isfile(ini_path):
        return False, 0, t('m_keine_ini')

    owned = bestand_datei.keys(stock if stock is not None
                                    else bestand_datei.load())
    words = TEXTS[lang_code]

    # ⚠⚠ **Wem der Auftrag Ruf bringt — aus einer eigenen Quelle.** Die
    # Vertragsdaten kennen nur die Zahl („150 XP"), nicht die Partei und nicht
    # die Art. Beides kommt von scmdb.net; das Modul holt es einmal je
    # Spielversion und legt eine kleine Tabelle an (71 KB statt 12,5 MB).
    #
    # ⚠ Scheitert der Abruf, laeuft alles Uebrige weiter: Eine fehlende
    # Ruf-Zeile ist ein Verlust, ein abgebrochener Einbau waere ein Schaden.
    rep_table = None
    try:
        from . import reputation, gamebuild
        try:
            version = gamebuild.live() or ''
        except Exception:
            version = ''
        reputation.refresh(version)
        rep_table = reputation.load()
    except Exception as exc:
        fehler.merken('injection.reputation', exc)

    title_by_key, text_by_key = {}, {}
    # ⚠⚠ **Auftraege OHNE eigenen Beschreibungstext bekommen die Angaben
    # trotzdem** (05.09.2026). Gemessen an den Vertragsdaten: **816 von 818**
    # Auftraegen bringen Rufpunkte und Abklingzeit mit, aber nur **367** haben
    # einen eigenen Beschreibungsblock — und nur die wurden bedient. Die
    # uebrigen **449** gingen verloren, obwohl die Daten dalagen und ein
    # Beschreibungs-Schluessel vorhanden ist.
    #
    # Genau so gemeldet: Ein Auftrag ohne Bauplaene zeigte nichts, waehrend
    # eine fremde Uebersetzung dort Rufpunkte anzeigte. „bau es bitte endlich
    # bei der SC BP Watcher Injektion mit ein … in JEDE quest wie mrkraken."
    #
    # ⚠ Der Unterschied zum Fall darunter: Hier gibt es keinen eigenen Block,
    # den wir setzen koennten — die Zeilen werden an den **vorhandenen
    # Spieltext angehaengt**. Deshalb eine eigene Tabelle statt `text_an`:
    # `text_an` ERSETZT, das hier ERGAENZT.
    details_by_key = {}
    for e in data['entries']:
        if not e.get('description') and e.get('descriptionLocKey'):
            entry_lines = _detail_lines(e, '', words, rep_table)
            if entry_lines:
                # ⚠ **Eine Leerzeile davor.** Ohne sie klebt die erste Angabe
                # unmittelbar am letzten Satz des Auftragstextes — gemessen
                # kam „…erinnert daran.# Zu erwartende Rufpunkte: 20 XP"
                # heraus. Der Block ist eine eigene Auskunft, keine
                # Fortsetzung des Auftraggeber-Textes.
                details_by_key[e['descriptionLocKey']] = (
                    '\\n\\n' + '\\n'.join(entry_lines))

    for e in data['entries']:
        block = e.get('description') or ''
        if not block:
            continue
        block, mine, total = _set_boxes(block, owned)
        # ⚠ Erst jetzt einfaerben: Die Kaestchen sind gesetzt, der Block ist
        # damit nachweislich unserer. Siehe `_ruf_einfaerben`.
        block = _highlight_rep(block)
        # ⭐ Rufpunkte, Abklingzeit, Teilbarkeit, Bauplan-Chance — sie standen
        # in der Quelle, aber nicht im Spiel. Siehe `_auftragsangaben`.
        block = _contract_details(block, e, words, rep_table)
        if e.get('descriptionLocKey'):
            text_by_key[e['descriptionLocKey']] = block
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
            chars = '!' if (e.get('bpnote') or '').strip() else ''
            title_by_key[e['titleLocKey']] = (' <EM4>[%s%s]</EM4>'
                                          % (words['kurz'], chars))

    changed = 0
    try:
        with open(ini_path, encoding='utf-8', errors='ignore') as f:
            lines = f.read().splitlines()
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
    stem_by_key = {}
    for e in data['entries']:
        block = text_by_key.get(e.get('descriptionLocKey') or '')
        stem = _stem(e.get('titleLocKey') or e.get('descriptionLocKey') or '')
        if block and stem and stem not in stem_by_key:
            stem_by_key[stem] = block

    # Dasselbe für die TITEL — die Voraussetzung für mehrteilige Reihen.
    # Ohne diese Tabelle gäbe es nur den exakten Schlüsselvergleich, und ein
    # Teilauftrag (`…Story01B_title`) findet den Zusatz seiner Reihe nie.
    title_stem_by_key = {}
    for key, suffix in title_by_key.items():
        stem = _stem(key)
        if stem and stem not in title_stem_by_key:
            title_stem_by_key[stem] = suffix

    # ⚠ Ohne Marken im Text: Was hier angefasst wird, kommt vorher in die
    # Merkdatei. Siehe `URTEXT_DATEI` — die Marken waren im Spiel sichtbar.
    origtext_old = load_origtext()
    origtext_new = {}
    fallback = _fallback_form(origtext_old, ini_path)
    name_suffix = _name_table(lines)
    # ⚠⚠ **Es gibt ZWEI Schreibwege, und beide brauchen das hier.**
    # `einrichten()` nimmt bevorzugt diesen (die gepflegten SCDL-Vertragstexte)
    # und fällt nur ohne sie auf `einspielen()` zurück. In v3.28.0 hingen die
    # eigenen Schiffsnamen nur am Rückfallweg — bei jedem, der die SCDL-Daten
    # hat (also fast jedem), wurde der Name **nie** geschrieben. Gemeldet mit
    # Bildschirmfoto: im Flottenmanager stand weiter der Werksname, obwohl die
    # Datei nachweislich neu geschrieben worden war.
    #
    # ⚠ Wer hier eine neue Art von Einfügung baut, baut sie an **beiden**
    # Stellen ein — oder er baut sie für die Hälfte der Nutzer gar nicht.
    own_ships = _asop_table(lines)

    new = []
    for line in lines:
        parts = _split_line(line)
        if not parts:
            new.append(line)
            continue
        key, suffix, text = parts
        # Der Wortlaut ohne UNSERE Einfügung. Ein fremder Block (Launcher) kann
        # darin noch stehen — er wird gleich abgetrennt, aber nicht verworfen.
        orig = _strip_old(text, key, origtext_old, fallback)
        base_text, _foreign = _split_foreign_block(orig)
        clean = base_text
        touched = False
        if key in own_ships:
            own, star = own_ships[key]
            clean = asop_modul.display_name(base_text, own, star)
            touched = clean != base_text
        elif key in name_suffix:
            clean = _name_with_detail(base_text, name_suffix[key])
            touched = True
        elif key in title_by_key:
            # ⚠ Steht die Marke schon da, kommt keine zweite dazu — gleich, ob
            # StarStrings oder der SC Deutsch Launcher sie gesetzt hat.
            if not _has_title_mark(base_text):
                clean, touched = base_text + title_by_key[key], True
        elif key in text_by_key:
            clean, touched = _append_block(base_text, text_by_key[key]), True
        elif key in details_by_key:
            # ⚠ Ein Auftrag ohne eigenen Block: Die Angaben kommen an den
            # SPIELTEXT, der schon dasteht. Steht die Angabe dort bereits
            # (weil ein anderes Werkzeug sie geschrieben hat oder wir beim
            # letzten Lauf), bleibt sie stehen — dieselbe Regel wie bei den
            # Marken.
            if not _has_details(base_text):
                clean = _append_block(base_text, details_by_key[key])
                touched = True
        elif key.lower().endswith('_title'):
            # Keine eigene Angabe — aber vielleicht ist es ein SCHRITT einer
            # Reihe, deren Hauptauftrag Baupläne bringt (siehe
            # `_reihen_stamm`). Der Spieler sieht im Auftragsfenster genau
            # diesen Schritt; ohne den Zusatz erfährt er dort nichts.
            main = _series_stem(_stem(key), title_stem_by_key)
            if main and not _has_title_mark(base_text):
                clean, touched = base_text + title_stem_by_key[main], True
        elif '_desc' in key.lower():
            # Keine eigene Angabe — aber vielleicht gehört die Beschreibung zu
            # einem Auftrag, für den wir welche haben.
            block = stem_by_key.get(_stem(key))
            if not block:
                # Wie beim Titel: auch Schritte einer Reihe versorgen, sonst
                # steht im Schritt `[BP!]` und darunter keine Bauplan-Liste.
                main = _series_stem(_stem(key), stem_by_key)
                if main:
                    block = stem_by_key[main]
            if block:
                clean, touched = _append_block(base_text, block), True
        if touched:
            # Den Wortlaut VOR der Einfügung merken, nicht danach — und **mit**
            # dem fremden Block, damit das Zurücksetzen ihn wiederbringt.
            origtext_new[key] = orig
            changed += 1
        else:
            # Nichts beigesteuert: dann bleibt auch der fremde Block, wo er war.
            clean = orig
        new.append('%s%s=%s' % (key, suffix, clean))

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
        with open(ini_path + '.tmp', 'w', encoding='utf-8', newline='') as f:
            f.write('\n'.join(new) + '\n')
        os.replace(ini_path + '.tmp', ini_path)
    except OSError as e:
        return False, 0, 'Schreiben fehlgeschlagen: %s' % e
    save_origtext(origtext_new, ini_path)
    meta = data.get('_meta') or {}
    return True, changed, '%d Textstellen (SCDL %s)' % (changed,
                                                          meta.get('version', '?'))


def apply_texts(ini_path, language, catalog_data=None, stock=None,
               remove_only=False):
    """Die Angaben in eine `global.ini` schreiben.

    Gibt (Erfolg, Anzahl geänderter Zeilen, Meldung) zurück. Die Datei wird
    erst vollständig neu geschrieben und dann umbenannt — bricht etwas ab,
    bleibt die alte Version unversehrt."""
    if not ini_path or not os.path.isfile(ini_path):
        return False, 0, t('m_keine_ini')

    catalog_data = catalog_data if catalog_data is not None else katalog_modul.load()
    missions = catalog_data.get('missionen') or {}
    if not missions and not remove_only:
        return False, 0, t('m_keine_missionen')

    owned = bestand_datei.keys(stock if stock is not None
                                    else bestand_datei.load())
    words = TEXTS[_lang_code(language)]

    # Beide Schlüssel-Arten in eine Tabelle: Titel bekommen das Kürzel,
    # Beschreibungen die Liste.
    title_keys, text_keys = {}, {}
    for entry in missions.values():
        if entry.get('titel_key'):
            title_keys[entry['titel_key']] = entry
        if entry.get('text_key'):
            text_keys[entry['text_key']] = entry

    changed = 0
    try:
        with open(ini_path, encoding='utf-8', errors='ignore') as f:
            lines = f.read().splitlines()
    except OSError as e:
        return False, 0, 'Lesen fehlgeschlagen: %s' % e

    origtext_old = load_origtext()
    origtext_new = {}
    fallback = _fallback_form(origtext_old, ini_path)
    name_suffix = _name_table(lines, remove_only)
    # Eigene Schiffsnamen im Fleet Manager. Beim reinen Entfernen bleibt die
    # Tabelle leer — dann stellt der Urtext-Weg die Werksnamen wieder her.
    own_ships = {} if remove_only else _asop_table(lines)

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
    stem_block = {}
    if not remove_only:
        for entry in missions.values():
            stem = _stem(entry.get('titel_key')
                           or entry.get('text_key') or '')
            if stem and stem not in stem_block:
                stem_block[stem] = entry

    new = []
    for line in lines:
        parts = _split_line(line)
        if not parts:
            new.append(line)
            continue
        key, suffix, text = parts
        orig = _strip_old(text, key, origtext_old, fallback)
        if orig != text:
            changed += 1
        clean = orig
        if not remove_only:
            # Ein fremder Block (SC Deutsch Launcher) wird abgetrennt und durch
            # unseren ersetzt — der Urtext behält ihn, also kommt er beim
            # Zurücksetzen wieder.
            base_text, _foreign = _split_foreign_block(orig)
            touched = False
            if key in own_ships:
                # ⚠ `grundlage` ist der **zurückgesetzte** Werksname. Nur so
                # bleibt ein zweiter Lauf folgenlos; mit dem Wert aus der
                # laufenden Datei stünde beim nächsten Mal ein Stern vor dem
                # Stern.
                own, star = own_ships[key]
                clean = asop_modul.display_name(base_text, own, star)
                touched = clean != base_text
            elif key in name_suffix:
                clean = _name_with_detail(base_text, name_suffix[key])
                touched = True
            elif key in title_keys:
                # ⚠ Keine zweite Marke, wo schon eine steht.
                if not _has_title_mark(base_text):
                    clean = base_text + _title_suffix(title_keys[key],
                                                       owned, words)
                    touched = True
            elif key in text_keys:
                clean = _append_block(base_text,
                                    _build_block(text_keys[key], owned, words))
                touched = True
            elif '_desc' in key.lower():
                # Keine eigene Angabe — aber vielleicht gehört die Beschreibung
                # zu einem Auftrag, für den wir welche haben (siehe oben).
                entry = stem_block.get(_stem(key))
                if entry:
                    clean = _append_block(base_text,
                                        _build_block(entry, owned, words))
                    touched = True
            if touched:
                origtext_new[key] = orig
                changed += 1
            else:
                clean = orig
        new.append('%s%s=%s' % (key, suffix, clean))

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
        with open(ini_path + '.tmp', 'w', encoding='utf-8', newline='') as f:
            f.write('\n'.join(new) + '\n')
        os.replace(ini_path + '.tmp', ini_path)
    except OSError as e:
        return False, 0, 'Schreiben fehlgeschlagen: %s' % e
    # Beim reinen Entfernen ist nichts mehr zu merken — die Datei wird geleert,
    # damit ein späterer Lauf nicht auf einen überholten Stand zurücksetzt.
    save_origtext(origtext_new, ini_path)

    return True, changed, '%d Textstellen' % changed


def setup(ini_path, language, progress=None, stock=None):
    """Die Bauplan-Angaben eintragen — auf dem jeweils besten Weg.

    Zuerst die Vertragsdaten des SCDL-Teams: 813 Verträge mit gepflegten
    Texten. Sind sie nicht erreichbar, tut es der eigene Aufbau aus den
    scmdb-Daten (349 Verträge) — dann fehlen Feinheiten wie Region und
    Gefahrenstufe, aber die Baupläne stehen da, und darum geht es."""
    tag = _lang_code(language)
    if not scdl_load(tag):
        scdl_fetch(tag, progress)
    if scdl_load(tag):
        ok, n, message = apply_scdl(ini_path, tag, stock)
        if ok:
            return ok, n, message
    return apply_texts(ini_path, language, stock=stock)


def refresh(ini_path, language, progress=None, stock=None):
    """Frische Vertragsdaten holen und neu eintragen.

    Gebraucht nach jedem Übersetzungs-Update und nach jedem Spiel-Patch: Beide
    schreiben die `global.ini` neu, die Angaben sind dann stillschweigend weg."""
    scdl_fetch(_lang_code(language), progress)
    return setup(ini_path, language, progress, stock)


def scdl_update_available(lang_code):
    """Gibt es bei den Vertragsdaten etwas Neueres? (ja/nein, neue Kennung).

    Verglichen wird die Kennung aus `_meta.version` (z. B. „LIVE 20.08.2026").
    Geholt wird dafür die ganze Datei — sie hat keine eigene Versionsauskunft,
    und 2,4 MB einmal am Tag sind kein Grund, dafür etwas zu bauen."""
    from .catalog import OFF
    old = scdl_version(lang_code)
    filename = SCDL_FILE.get(lang_code)
    if not filename or OFF:          # ⚠ SC_BP_NO_NET gilt auch hier
        return False, None
    try:
        req = urllib.request.Request(SCDL_RAW % filename,
                                     headers={'User-Agent': 'SC-BP-Watcher'})
        with urllib.request.urlopen(req, timeout=60) as r:
            raw = json.loads(r.read().decode('utf-8'))
    except Exception as exc:
        fehler.merken('injection.scdl_fetch', exc, filename)
        return False, None
    new_id = (raw.get('_meta') or {}).get('version')
    if not raw.get('entries') or new_id == old:
        return False, old
    # Schon mal ablegen — der Abruf ist gelaufen, ein zweiter wäre Verschwendung.
    try:
        target = pfade.app_datei(SCDL_CACHE % lang_code)
        with open(target + '.tmp', 'w', encoding='utf-8') as f:
            json.dump(raw, f, ensure_ascii=False)
        os.replace(target + '.tmp', target)
    except Exception:
        return False, old
    return True, new_id


def scdl_version(lang_code):
    """Welche Version der Vertragsdaten liegt hier? Oder None."""
    d = scdl_load(lang_code)
    return (d.get('_meta') or {}).get('version') if d else None


def leftover_file(new_path):
    """Die Datei, in der noch unsere Einfügungen stehen, obwohl sie niemand
    mehr pflegt — oder `None`.

    Die Merkdatei `injektion-urtext.json` hält fest, in **welcher** Datei
    zuletzt geschrieben wurde. Zeigt sie woandershin als das neue Ziel, ist das
    genau die verwaiste.
    """
    old = (_origtext_file().get('datei') or '').strip()
    if not old or not (_origtext_file().get('texte') or {}):
        return None
    try:
        if new_path and os.path.samefile(old, new_path):
            return None
    except OSError:
        # Eine der beiden Dateien gibt es nicht mehr — dann entscheidet der
        # Wortlaut. `samefile` braucht beide.
        if new_path and os.path.normpath(old) == os.path.normpath(new_path):
            return None
    return old if os.path.isfile(old) else None


def clean_leftover(new_path):
    """Vor einem Quellenwechsel die alte Datei zurücksetzen.

    ⚠⚠ **Warum das sein muss.** Die Textquellen schreiben in **verschiedene**
    Sprachordner: „deutsch" nach `german_(germany)`, „StarStrings" und
    „Original" nach `english`. Wer wechselt, lässt in der alten Datei unsere
    Einfügungen stehen — und niemand pflegt sie mehr. Lädt das Spiel
    ausgerechnet die, sieht der Spieler **dauerhaft einen alten Stand**, ohne
    dass irgendetwas darauf hindeutet.

    Genau so am 29.08.2026 gemessen: Die deutsche Datei war **später**
    geschrieben (06:53) als die englische (06:34) und trug trotzdem die alte
    Form — der Wechsel auf „Original" an jenem Morgen hatte sie liegen lassen.

    ⚠ Die Reihenfolge ist Pflicht: **erst aufräumen, dann einrichten.** Das
    Zurücksetzen braucht den Urtext der alten Datei, und `einrichten()`
    überschreibt ihn mit dem der neuen. Andersherum wäre die alte Datei für
    immer verloren.

    Gibt `(aufgeraeumt, anzahl)` zurück — `aufgeraeumt` ist der Pfad oder None.
    """
    old = leftover_file(new_path)
    if not old:
        return None, 0
    folder = os.path.basename(os.path.dirname(old)) or 'english'
    try:
        ok, count, _message = remove_texts(old, folder)
    except Exception as exc:
        # ⚠ Ein misslungenes Aufräumen darf den Wechsel nicht anhalten. Der
        # Spieler steht sonst ohne beides da: alte Quelle weg, neue nicht
        # eingerichtet.
        fehler.merken('injection.clean_leftover', exc)
        return None, 0
    return (old, count) if ok else (None, 0)


def remove_texts(ini_path, language='english'):
    """Alle Einfügungen zurücknehmen — die Datei bleibt sonst unverändert.

    ⚠ Zurück heißt: so, wie der Spieler die Datei hatte. Hat der SC Deutsch
    Launcher oder StarStrings dort etwas stehen, bleibt das stehen — der Urtext
    bewahrt es."""
    return apply_texts(ini_path, language, remove_only=True)


def is_applied(ini_path):
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
        with open(ini_path, encoding='utf-8', errors='ignore') as f:
            for line in f:
                if OPEN in line or COUNTING_TITLE.search(line):
                    return True
                # ⚠ Die Überschrift allein genügt nicht: Der SC Deutsch Launcher
                # schreibt dieselbe, aus derselben Quelle. Erst das Kästchen
                # macht den Block zu unserem.
                if OWN_TRACE.search(line) and _has_box(line):
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

def _lang_order(fallback_lang=('english', 'german_(germany)')):
    """In welcher Reihenfolge die Sprachordner geprüft werden.

    Vorn steht, was in der `user.cfg` als `g_language` eingetragen ist — das
    ist die Datei, die das Spiel wirklich liest. Steht dort nichts, bleibt es
    beim Rückfall: Ohne Eintrag startet Star Citizen auf Englisch, dann ist die
    bisherige Reihenfolge richtig.
    """
    from . import translation
    order = list(fallback_lang)
    try:
        language = translation.game_language()
    except Exception as exc:
        fehler.merken('injection.spielsprache', exc)
        return order
    if not language:
        return order
    if language in order:
        order.remove(language)
    return [language] + order


def ini_file():
    """Die `global.ini`, um die es geht. (Pfad, Sprachordner, Quelle).

    ⚠ Maßgeblich ist die **gewählte** Textquelle, nicht die zuerst gefundene.
    Hier stand eine feste Reihenfolge: erst „deutsch", dann „starstrings", und
    die erste eingerichtete gewann. Wer beide einmal benutzt hatte und dann auf
    StarStrings umstellte, bekam trotzdem weiter „Quelle: Deutsch (rjcncpt)"
    angezeigt — die deutsche war ja auch noch eingerichtet. Genau so gemeldet.
    Die Reihenfolge greift nur, solange nichts gewählt wurde.
    """
    from . import translation
    chosen = pfade.einstellung('inj_quelle')
    order_list = ['deutsch', 'starstrings']
    if chosen in order_list:
        order_list.remove(chosen)
        order_list.insert(0, chosen)
    elif chosen == 'original':
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
        for lang_folder in _lang_order():
            path = translation.target_ini(lang_folder)
            if path and os.path.isfile(path):
                return path, lang_folder, None
    for source in order_list:
        if translation.installed(source):
            lang_folder = translation.SOURCES[source]['sprache']
            return translation.target_ini(lang_folder), lang_folder, source
    # Nichts vermerkt: dann die Datei nehmen, die tatsächlich daliegt — aber in
    # der Reihenfolge, die das Spiel vorgibt. Hier stand `german_(germany)`
    # zuerst; für dieses eine Haus richtig, für jeden mit englischem Spiel
    # falsch. Geraten wird nicht mehr.
    for lang_folder in _lang_order(('german_(germany)', 'english')):
        p = translation.target_ini(lang_folder)
        if p and os.path.isfile(p):
            return p, lang_folder, None
    return None, 'english', None


def status():
    """Steht etwas im Spiel, und aus welcher Quelle? (dict)"""
    from . import translation
    path, _language, source = ini_file()
    exists = bool(path and os.path.isfile(path))
    inside = bool(exists and is_applied(path))
    return {'datei': path, 'drin': inside, 'quelle': source,
            'stand': translation.installed(source) if source else None}
