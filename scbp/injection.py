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

from . import specs
from . import asop as asop_modul
from . import errors, collection as bestand_datei
from . import catalog as katalog_modul
from . import paths
from .language import t

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
# ⚠ **Beide Formen**: die zählende `[BP 3/12]` und das heutige `[BP]`. Wer
# mit einer älteren Version injiziert hat, trägt die zählende in seiner Datei —
# ohne sie hier bliebe sie beim Zurücksetzen für immer stehen.
#
# ⚠ Und genau deshalb greift dieser Notnagel **nur bei einer Datei, in der wir
# schon einmal geschrieben haben** (`ist_frisch()`): Das heutige blanke `[BP]`
# ist nicht unser Alleinstellungsmerkmal — MrKraken schreibt in StarStrings
# dasselbe. Als eigener Nachweis dient `EIGENER_NACHWEIS` weiter unten.

# ⚠ Nicht einfach ab der Bindestrich-Linie alles wegschneiden: CIG benutzt
# solche Linien **selbst** als Gliederung. `Battaglia_RPT_BoardShip_01_desc`
# verlöre dadurch 589 seiner 870 Zeichen — ein ganzer Abschnitt verschwände
# stillschweigend, und niemand bemerkt es, bis der Text im Spiel fehlt.
#
# Geschnitten wird deshalb nur, wenn nach der Linie auch eine **unserer**
# Überschriften steht — die eigene und die der SCDL-Vertragsdaten, je zweisprachig.
#
# ⚠ Es sind **vier** Formen, nicht zwei. Die Vertragsdaten kennen neben der
# Bauplan-Liste noch einen zweiten Blocktyp: den Hinweis auf dynamisch erzeugte
# Missionstypen (84 der 363 Blöcke). Fehlt seine Überschrift hier, bleiben diese
# Blöcke ohne Merkdatei halb stehen. Gezählt: 279 + 84 je Sprache.
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
# schreibt in StarStrings genau denselben. Damit hielte der Watcher eine frisch
# eingesetzte StarStrings-Datei für schon eingetragen.
#
# Sicher sind: die alte Marke, unsere Block-Überschriften (in keiner der beiden
# Fremdquellen enthalten — in beiden nachgezählt: 0) und
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
    # gar nicht: Wem die Merkdatei fehlt (anderer Rechner, aufgeräumt), der
    # bekäme den Block beim Zurücksetzen nicht mehr aus seiner `global.ini`
    # heraus. Prüfung 102 spricht den Notnagel ohne Merkdatei an.
    r'|(?:\\n){1,2}?\s*-{20,}(?:\\n|\s|<EM\d>)*(?:%s).*$)'
    % '|'.join(re.escape(u) for u in _HEADINGS), re.S)

# Aufbau nach dem Vorbild des SC Deutsch Launchers — die **Gliederung** ist die
# nützliche Erkenntnis (was ein Spieler vor dem Annehmen wissen will), die
# Formulierungen sind eigene. Alle Angaben stammen aus scmdb.
TEXTS = {
    'de': {
        'kurz':      'BP',
        # ⚠ **Missionstyp, nicht dieser Auftrag.** Eine Überschrift, die
        # Baupläne aus *diesem Auftrag* verspricht, sagt mehr, als die Daten
        # hergeben: Die Liste führt alle Preisstufen zusammen, weil sich 123 von
        # 353 Aufträgen den Textschlüssel über ihre Stufen hinweg teilen. Wer
        # sie wörtlich nimmt, nimmt den Auftrag an und bekommt womöglich nichts.
        #
        # Der SC Deutsch Launcher verwendet dieselbe Überschrift (367 mal in
        # seiner Datei). Sie verspricht nichts, was sie nicht halten kann.
        'ueberschr': 'MÖGLICHE BAUPLÄNE FÜR DIESEN MISSIONSTYP',
        'chance':    'Chance auf Bauplan',
        'rep_min':   'Min. Reputation',
        'rep_max':   'Max. Reputation',
        'lohn':      'Belohnung',
        'ruf':       'Rufpunkte',
        # ⚠ Kurz halten: Die Zeile traegt schon Fraktionsnamen und Art
        # (`Citizens For Prosperity +100 Standing`), und sie steht in einer
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
        'region':    'Region',
        'ja':        'Ja', 'nein': 'Nein',
        'liste':     'Baupläne — angehakt ist, was du hast',
        'ab_rang':   'erst ab',
        'leere_stufen': ('Achtung: %d der %d Stufen dieses Auftrags geben '
                         'gar keine Baupläne.'),
        'quelle':    'Angaben von scmdb.net · eingefügt von Verse-Kit',
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
        'region':    'Region',
        'ja':        'Yes', 'nein': 'No',
        'liste':     'Blueprints — ticked means you have it',
        'ab_rang':   'needs',
        'leere_stufen': ('Note: %d of the %d tiers of this contract give no '
                         'blueprints at all.'),
        'quelle':    'Data from scmdb.net · added by Verse-Kit',
        'trenner':   ',',
    },
}

# Kästchen wie beim Launcher: leer, wenn der Bauplan fehlt — hervorgehoben,
# wenn man ihn hat. Das Auge findet dadurch sofort, was noch offen ist.
# ---------------------------------------------------------------------------
# Wo ein **fremder** Anhang beginnt — damit unserer davor landet, nicht dahinter
# ---------------------------------------------------------------------------
#
# ⚠ Warum es das gibt (gemessen mit `tools/smartcitizen_pruefen.py`):
# Smart Citizen (Osiris-DevWorks) hängt eigene Blöcke an dieselben
# Beschreibungen und räumt vor jedem Lauf seinen alten Block ab — indem es den
# **ersten** eigenen Marker sucht und ab dort ALLES wegwirft:
#
#     for marker in ("\\n\\n--- STATS ---", "\\n\\n<EM3>MISSION DETAILS</EM3>", …):
#         if marker in existing_value:
#             existing_value = existing_value[:existing_value.index(marker)]
#
# Auf ihrer Seite ist das richtig. Steht unser Block aber dahinter, ist er
# bei jedem ihrer Läufe still verschwunden: **398 von 398** gemeinsamen
# Einträgen. Der Nutzer merkt nur, dass die Baupläne weg sind.
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
    abräumen, schneiden ab dem eigenen Marker bis zum Ende. Alles davor
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
# ⚠ Warum es diese Datei gibt: Ein Marken-Paar um jede Einfügung
# (`[SCBPW] … [/SCBPW]`) ließe sich zwar auf den Buchstaben genau entfernen —
# aber **man sieht die Marken im Spiel**, etwa mitten im Auftragstitel.
#
# Ein unsichtbareres Zeichen ist kein Ausweg — was die Spiel-Engine mit
# unbekannten Zeichen macht, weiß man erst, wenn es zu spät ist. Stattdessen wird
# der **Originaltext** jeder angefassten Zeile hier festgehalten. Damit braucht es
# im Spieltext gar keine Marke, und das Zurücksetzen stellt den Wortlaut wieder
# her, statt eine Einfügung herauszuschneiden.
ORIGTEXT_FILE = 'injektion-urtext.json'

# Merker im Urtext für eine Zeile, die es in der Grundlage NICHT gab und die
# wir ergänzt haben (fehlende Schiffsnamen, siehe `_added_ship_names`). Beim
# Zurücksetzen und vor jedem neuen Schreiben fällt eine so markierte Zeile weg.
ADDED = '\x00versekit-ergaenzt'


def _append_ship_names(new_lines, added_ships, own_ships, origtext_new):
    """Die fehlenden Schiffsnamen hinten anhängen — mit eigenem Namen, wenn es
    einen gibt — und als ergänzt merken. Gibt die Zahl der Zeilen zurück."""
    for key, value in added_ships.items():
        text = value
        if key in own_ships:
            own, star = own_ships[key]
            text = asop_modul.display_name(value, own, star)
        new_lines.append('%s=%s' % (key, text))
        origtext_new[key] = ADDED
    return len(added_ships)


def _added_ship_names(ini_path, lines, origtext_old):
    """Schiffsnamen aus der englischen Datei, die dieser Datei fehlen.

    Gibt `{schluessel: englischer Name}` zurück — leer, wenn die Datei selbst
    englisch ist oder es keine englische daneben gibt. Zeilen, die wir beim
    letzten Mal selbst ergänzt haben, zählen dabei als fehlend: Sie werden
    gleich weggeräumt und frisch geschrieben.
    """
    try:
        folder = os.path.dirname(ini_path)
        if os.path.basename(folder).lower() == 'english':
            return {}
        english = os.path.join(os.path.dirname(folder), 'english', 'global.ini')
        if os.path.isfile(english):
            with open(english, encoding='utf-8', errors='ignore') as f:
                reference = f.read().splitlines()
        else:
            # ⭐⭐ **Keine englische Datei daneben — und das ist der Normalfall.**
            # Sie entsteht nur, wenn jemand StarStrings oder die Originaltexte
            # eingerichtet hat; wer bei einer Übersetzung bleibt, hat sie nie.
            # Ohne Ergaenzung stuende im Flottenmanager weiter `@vehicle_Name…`.
            #
            # Die Originaltexte liegen in JEDER Installation in der
            # `Data.p4k` — von dort, ohne am Spiel etwas zu veraendern.
            from . import gametext
            namen = gametext.names_or_fetch()
            reference = ['%s=%s' % (k, v) for k, v in namen.items()]
        if not reference:
            return {}
        own = [line for line in lines
               if origtext_old.get(line.split('=', 1)[0]) != ADDED]
        return asop_modul.missing_names(own, reference)
    except Exception as exc:
        errors.record('injection._added_ship_names', exc)
        return {}


def _origtext_file():
    """Der ganze Inhalt der Merkdatei — leer, wenn es sie nicht gibt."""
    try:
        with open(paths.app_file(ORIGTEXT_FILE), encoding='utf-8') as f:
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
    Titelzusatzes ist `<EM4>[BP]</EM4>`, **genau das**, was MrKraken in
    StarStrings selbst an 314 Titel schreibt. In einer frisch eingesetzten
    Fremddatei kann nichts von uns stehen; wer dort trotzdem schneidet,
    löscht fremden Text (an der echten Datei gemessen: 17 Kennzeichnungen) —
    und weil dann der bereits geschnittene Wortlaut als Urtext gemerkt wird,
    kommen sie auch beim Zurücksetzen nie wieder.
    """
    return bool(_origtext_file().get('frisch'))


def discard_origtext():
    """Die gemerkten Originaltexte wegwerfen und die Datei als frisch merken.

    Gehört zu **jedem** Einsetzen einer neuen Grundlage (`translation.fetch()`):
    Die alten Merktexte gehören zur alten Datei und würden auf einen überholten
    Stand zurückschreiben; das Kennzeichen `frisch` schützt den fremden Text
    beim ersten Lauf (siehe `ist_frisch()`)."""
    try:
        target = paths.app_file(ORIGTEXT_FILE)
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(target, 'w', encoding='utf-8') as f:
            json.dump({'stand': time.strftime('%Y-%m-%d %H:%M:%S'),
                       'frisch': True, 'texte': {}}, f, ensure_ascii=False)
        return True
    except Exception as exc:
        errors.record('injection.discard_origtext', exc)
        return False


def save_origtext(texts_map, ini_path):
    """Die Originaltexte festhalten. Fehlschlag ist kein Grund abzubrechen."""
    try:
        target = paths.app_file(ORIGTEXT_FILE)
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(target, 'w', encoding='utf-8') as f:
            json.dump({'datei': ini_path, 'stand': time.strftime('%Y-%m-%d %H:%M:%S'),
                       'texte': texts_map}, f, ensure_ascii=False)
        return True
    except Exception as exc:
        errors.record('injection.save_origtext', exc)
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
        # ⚠ Der Notnagel schneidet ab hier bis zum Ende — da unser Block
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


def _rep_table():
    """Wem ein Auftrag Ruf bringt — die Tabelle aus `reputation`, oder None.

    ⚠⚠ Der Auftragstext zeigt, ob ein Auftrag Standing oder Rep bringt —
    dieselbe Angabe, die scmdb führt.

    ⚠ Scheitert der Abruf, läuft alles Übrige weiter: Eine fehlende Ruf-Zeile
    ist ein Verlust, ein abgebrochener Einbau wäre ein Schaden."""
    try:
        from . import reputation, gamebuild
        try:
            version = gamebuild.live() or ''
        except Exception:
            version = ''
        reputation.refresh(version)
        return reputation.load()
    except Exception as exc:
        errors.record('injection._rep_table', exc)
        return None


def _rep_line(entry, words, rep_table):
    """`# Ruf: Headhunters +150 Standing` für diesen Auftrag — oder ''."""
    if not rep_table:
        return ''
    try:
        from . import reputation
        return reputation.line(reputation._key(entry.get('titel_key')),
                               words['ruf_bei'], rep_table)
    except Exception as exc:
        errors.record('injection._rep_line', exc)
        return ''


def _region_line(entry, words, rep_table):
    """`# Region: Pyro (Bloom)` für diesen Auftrag — oder ''.

    Aus derselben scmdb-Tabelle wie die Ruf-Zeile (`reputation`). Eine
    Gefahrenstufe gibt es in den scmdb-Daten nicht und wird deshalb nicht
    angegeben."""
    if not rep_table:
        return ''
    try:
        from . import reputation
        return reputation.region_line(entry.get('titel_key'), words['region'],
                                      rep_table)
    except Exception as exc:
        errors.record('injection._region_line', exc)
        return ''


def _build_block(entry, owned, words, rep_table=None):
    """Der Textblock, der an die Beschreibung gehängt wird.

    Erst die Eckdaten als kurze Liste, dann die Baupläne mit Kästchen. Die
    Reihenfolge ist Absicht: Ob sich der Auftrag überhaupt lohnt, entscheidet
    man an Chance und Reputation — die Namensliste liest man erst danach."""
    z = ['', LINE, '', '<EM4>%s</EM4>' % words['ueberschr'], '']

    # ⚠ Ruf-Zeilen blau (`<EM4>`, die Auszeichnung des Spiels), damit sie
    # im Auftragstext sofort ins Auge fallen.
    def blue(text):
        return '<EM4>%s</EM4>' % text

    chance = entry.get('chance')
    if chance:
        z.append('# %s: %d%%' % (words['chance'], round(chance * 100)))
    if entry.get('rang'):
        z.append(blue('# %s: %s (%s XP)' % (words['rep_min'], entry['rang'],
                                            _group_digits(entry.get('rep') or 0, words))))
    if entry.get('rang_max'):
        z.append(blue('# %s: %s (%s XP)' % (words['rep_max'], entry['rang_max'],
                                            _group_digits(entry.get('rep_max') or 0, words))))
    if entry.get('uec'):
        z.append('# %s: %s aUEC' % (words['lohn'], _group_digits(entry['uec'], words)))
    if entry.get('ruf'):
        z.append(blue('# %s: %s XP' % (words['ruf'], _group_digits(entry['ruf'], words))))
    rep = _rep_line(entry, words, rep_table)
    if rep:
        z.append(blue(rep))
    if entry.get('cooldown'):
        z.append('# %s: %s %s' % (words['cooldown'],
                                  _group_digits(entry['cooldown'], words),
                                  words['minuten']))
    if 'teilbar' in entry:
        z.append('# %s: %s' % (words['teilbar'],
                               words['ja'] if entry['teilbar'] else words['nein']))
    region = _region_line(entry, words, rep_table)
    if region:
        z.append(region)

    # ⚠ **Ohne Zählung (3 von 12)** — aus demselben Grund wie im Titel (siehe
    # `_titel_zusatz`): Die Liste führt alle Preisstufen zusammen, die Zahl
    # wäre eine Behauptung über etwas, das gar nicht auflösbar ist. Die
    # Kästchen sagen dasselbe, nur ehrlich: angehakt heißt vorhanden.
    z += ['', '# ' + words['liste'] + ':']

    # Wo sich die Stufen unterscheiden, steht der nötige Rang hinter dem Namen.
    # ⚠ Das ist **nur Text**. Es blendet nichts aus und hakt nichts anders ab:
    # Wer den Bauplan hat, hat ihn — auch wenn diese Stufe ihn nicht hergibt.
    frm = entry.get('ab') or {}
    width = max([len(n) for n in entry['bp']] or [0])

    def box_line(name):
        inside = katalog_modul._norm(name) in owned
        cond = frm.get(name)
        line = '   %s %s' % (BOX_HAVE if inside else BOX_MISSING, name)
        if cond:
            line += '%s  %s %s (%s XP)' % (' ' * (width - len(name)),
                                            words['ab_rang'], cond['rang'],
                                            _group_digits(cond['rep'], words))
        return line

    per_system = entry.get('je_system') or {}
    if per_system:
        for system, names in per_system.items():
            z += ['', blue('# %s:' % system)]
            z += [box_line(name) for name in names]
    else:
        z += [box_line(name) for name in entry['bp']]

    # Gibt es Stufen dieses Auftrags, die leer ausgehen, gehört das dazu —
    # sonst fliegt jemand für eine Liste hin, die seine Stufe nie hergibt.
    if entry.get('leer'):
        z += ['', '# ' + (words['leere_stufen']
                          % (entry['leer'], entry['stufen']))]
    z += ['', words['quelle']]
    # Ohne Marken — sie standen sichtbar im Spiel. Zurückgesetzt wird über die
    # gemerkten Originaltexte (siehe `URTEXT_DATEI`).
    return '\\n' + '\\n'.join(z)


# Ein Kürzel, das am Anfang eines Namens schon dasteht — `[CS1] Spark-G Missile`.
# So schreibt MrKraken seine Angaben (136 Namen in StarStrings),
# der Watcher setzt seine dahinter in runde Klammern. Ohne diese Prüfung stünde
# im Spiel `[CS1] Spark-G Missile (CS1)`.
FOREIGN_TAG = re.compile(r'^\[[A-Za-z0-9/. -]{1,14}\]\s')


def _has_box(text):
    """Steht in diesem Stück ein Kästchen von uns?

    **Das ist das Unterscheidungsmerkmal.** Der SC Deutsch Launcher schreibt
    Blöcke mit derselben Überschrift und derselben Liste wie VerseKit.
    Der einzige Unterschied ist der, der das Werkzeug ausmacht: In den Rohdaten
    steht `    - Atzkav Sniper Rifle`, bei uns `    [x] Atzkav Sniper Rifle`.
    Wo kein Kästchen steht, hat nicht der Watcher geschrieben."""
    return '[x]' in text or BOX_MISSING in text


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

    ⚠ **Ohne Zählung.** Ein `[BP 3/12]` sähe nützlich aus, wäre aber nicht
    wahr: Die Liste eines Auftrags führt alle Baupläne **aller** Preisstufen
    zusammen, und welche davon die eigene Stufe hergibt, lässt sich nicht
    auflösen — 123 von 353 Aufträgen teilen sich den Textschlüssel über ihre
    Stufen hinweg. Die Zahl hieße nur: 3 von 12, die irgendjemand irgendwo
    bekommen kann.

    Ein schlichtes `[BP]` sagt, was stimmt: Hier gibt es Baupläne. Was man
    davon hat, sagen die Kästchen in der Liste.
    """
    chars = '!' if (entry.get('bpnote') or '').strip() else ''
    return ' <EM4>[%s%s]</EM4>' % (words['kurz'], chars)


# Name der Einstellung, mit der sich die Angaben am Gegenstand abschalten
# lassen. Standard ist **an**: Wer die Injektion einschaltet, will Angaben im
# Spiel sehen — und genau dafür ist dieses Werkzeug da.
SETTING_DETAILS = 'angaben_am_gegenstand'


def _name_table(lines, remove_only=False):
    """Tabelle *Namensschlüssel → Kürzel* — oder leer, wenn abgeschaltet.

    Beim reinen Entfernen bleibt sie leer: Dann stellt der Urtext-Weg die
    ursprünglichen Namen wieder her, und es soll nichts Neues dazukommen."""
    if remove_only or not paths.setting_bool(SETTING_DETAILS, True):
        return {}
    try:
        return specs.build_table(lines)
    except Exception as exc:
        errors.record('injection._name_table', exc)
        return {}


def _asop_table(lines):
    """Tabelle *Fahrzeugschlüssel → (eigener Name, Stern)* — oder leer.

    Eigene Fehler dürfen die Injektion nicht anhalten: Wer seine Schiffe nicht
    umbenannt hat, soll trotzdem seine Bauplan-Angaben bekommen."""
    try:
        return asop_modul.build_table(lines)
    except Exception as exc:
        errors.record('injection._asop_table', exc)
        return {}


def _rank_table(lang_code, remove_only=False):
    """Tabelle *Rang-Schlüssel → ` [ab 10.000]`* — oder leer.

    Holt die Schwellen bei Bedarf (einmal je Spielversion, siehe
    `rank_thresholds`). Ein Fehler hier darf die übrige Injektion nicht
    aufhalten: Ohne Ruf-Stufen stehen die Bauplan-Angaben trotzdem da."""
    if remove_only:
        return {}
    try:
        from . import rank_thresholds, gamebuild
        try:
            version = gamebuild.live() or ''
        except Exception:
            version = ''
        rank_thresholds.refresh(version)
        return rank_thresholds.build_table(lang_code)
    except Exception as exc:
        errors.record('injection._rank_table', exc)
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
    zeigt, ob sich seit dem letzten Einspielen etwas getan hat, ohne die
    Texte jedes Mal neu zu bauen.

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


# Der Versionskasten im Hauptmenü des Spiels. Andere Werkzeuge hängen dort
# ebenfalls ihre Zeilen an; die eigene kommt dahinter.
MENU_KEY = 'Frontend_PU_Version'
WEBSITE = 'https://versekit.xharig.com'
MENU_MARK = '\\nVerse-Kit'


def menu_lines():
    """Die eigene Zeile für den Menükasten — eine Zeile, mit dem `\\n`
    der `global.ini` davor. Jede Zeile mehr schiebt das Hauptmenü nach unten."""
    try:
        from sc_bp_watcher import __version__ as version
    except Exception:
        version = ''
    suffix = (' v%s' % version) if version else ''
    return '%s%s · %s' % (MENU_MARK, suffix, WEBSITE.split('//', 1)[-1])


def apply_texts(ini_path, language, catalog_data=None, stock=None,
               remove_only=False, progress=None):
    """Die Angaben in eine `global.ini` schreiben.

    Gibt (Erfolg, Anzahl geänderter Zeilen, Meldung) zurück. Die Datei wird
    erst vollständig neu geschrieben und dann umbenannt — bricht etwas ab,
    bleibt die alte Version unversehrt."""
    if not ini_path or not os.path.isfile(ini_path):
        return False, 0, t('m_keine_ini')

    given = catalog_data is not None
    catalog_data = catalog_data if given else katalog_modul.load()
    missions = catalog_data.get('missionen') or {}
    if not missions and not remove_only and not given:
        # ⚠⚠ **Bei einer frischen Installation ist der Katalog noch leer** —
        # etwa im Einrichtungsassistenten. Er wird sonst erst nach der
        # Einrichtung geholt, hier aber schon gebraucht. Also jetzt holen,
        # statt aufzugeben.
        try:
            if progress:
                progress(t('m_katalog_holen'))
            katalog_modul.update()
            catalog_data = katalog_modul.load()
            missions = catalog_data.get('missionen') or {}
        except Exception as exc:
            errors.record('injection.apply_texts', exc)
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
    added_ships = {} if remove_only else _added_ship_names(ini_path, lines,
                                                           origtext_old)
    own_ships = {} if remove_only else _asop_table(
        lines + ['%s=%s' % kv for kv in added_ships.items()])
    # Ruf-Schwellen an den Rangnamen.
    rank_suffix = _rank_table(_lang_code(language), remove_only)
    # Wem ein Auftrag Ruf bringt (Partei und Art) — nur beim Eintragen.
    rep_table = None if remove_only else _rep_table()

    # ⚠ Eine Mission hat im Spiel **mehr** Beschreibungen, als der Katalog
    # kennt. Gemessen: `Covalex_HaulCargo_SingleToMulti` führt drei
    # Beschreibungs-Schlüssel, in der `global.ini` stehen **acht** —
    # verschiedene Zielorte und Waren derselben Mission. Wer eine der fünf
    # übrigen erwischt, sähe die Marke im Titel und darunter **nichts**.
    #
    # Gelöst über den gemeinsamen Namensanfang: Zu jedem Titel, der Angaben
    # bekommt, bekommen **alle**
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
        if orig == ADDED:
            # Von uns ergänzt — fällt weg; ohne `remove_only` kommt sie unten
            # frisch wieder dazu.
            changed += 1
            continue
        if orig != text:
            changed += 1
        clean = orig
        if not remove_only:
            # Ein fremder Block (SC Deutsch Launcher) wird abgetrennt und durch
            # unseren ersetzt — der Urtext behält ihn, also kommt er beim
            # Zurücksetzen wieder.
            base_text, _foreign = _split_foreign_block(orig)
            touched = False
            # ⚠ Der Menükasten steht oft in der ersten Zeile; ein BOM davor
            # gehört zum Schlüssel und bleibt beim Zurückschreiben erhalten.
            if key.lstrip('\ufeff') == MENU_KEY:
                # Fremde Zeilen im Menükasten bleiben vollständig stehen. Eine
                # eigene Zeile, die ohne gemerkten Urtext übrig ist, wird
                # vorher abgeschnitten, damit sie nicht doppelt erscheint.
                orig = orig.split(MENU_MARK, 1)[0]
                # Zeilenumbrüche am Ende (Rest einer mehrzeiligen eigenen
                # Zeile) fallen weg.
                while orig.endswith('\\n'):
                    orig = orig[:-2]
                clean = orig + menu_lines()
                touched = True
            elif key in own_ships:
                # ⚠ `grundlage` ist der **zurückgesetzte** Werksname. Nur so
                # bleibt ein zweiter Lauf folgenlos; mit dem Wert aus der
                # laufenden Datei stünde beim nächsten Mal ein Stern vor dem
                # Stern.
                own, star = own_ships[key]
                clean = asop_modul.display_name(base_text, own, star)
                touched = clean != base_text
            elif key in rank_suffix:
                from . import rank_thresholds
                clean = rank_thresholds.with_suffix(base_text, rank_suffix[key])
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
                                    _build_block(text_keys[key], owned, words,
                                                 rep_table))
                touched = True
            elif '_desc' in key.lower():
                # Keine eigene Angabe — aber vielleicht gehört die Beschreibung
                # zu einem Auftrag, für den wir welche haben (siehe oben).
                entry = stem_block.get(_stem(key))
                if entry:
                    clean = _append_block(base_text,
                                        _build_block(entry, owned, words,
                                                     rep_table))
                    touched = True
            if touched:
                origtext_new[key] = orig
                changed += 1
            else:
                clean = orig
        new.append('%s%s=%s' % (key, suffix, clean))
    changed += _append_ship_names(new, added_ships, own_ships, origtext_new)

    try:
        # ⚠⚠ **`newline=''` ist Pflicht — sonst wird die ganze Datei umgeschrieben.**
        # Der Code setzt hier bewusst `\n`, weil das Spiel seine `global.ini` mit
        # Unix-Zeilenenden ausliefert. Ohne diesen Parameter uebersetzt Python
        # unter **Windows** jedes `\n` still in `\r\n` — und damit aendert sich
        # JEDE der 90.363 Zeilen einer 10-MB-Fremddatei, obwohl inhaltlich nichts
        # anders ist (+90.363 Bytes, genau ein Byte je Zeile). Unter Linux
        # passiert das nicht; `tools/starstrings_pruefen.py` erkennt es unter
        # Windows am abweichenden Wortlaut nach dem Zuruecksetzen.
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
    """Die Bauplan-Angaben eintragen — aus den eigenen Daten (scmdb).

    ⚠⚠ Es gibt nur **einen** Schreibweg: `apply_texts`. Vertragsdaten
    des SC Deutsch Launchers werden nicht genutzt (Prüfung 175)."""
    return apply_texts(ini_path, language, stock=stock, progress=progress)


def refresh(ini_path, language, progress=None, stock=None):
    """Neu eintragen — mit frisch geholtem Katalog, falls erreichbar.

    Gebraucht nach jedem Übersetzungs-Update und nach jedem Spiel-Patch: Beide
    schreiben die `global.ini` neu, die Angaben sind dann stillschweigend weg."""
    katalog_modul.update(progress)       # wirft nie; holt nur bei neuer Spielversion
    return setup(ini_path, language, progress, stock)


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
    Sprachordner: `deutsch` nach `german_(germany)`, `starstrings` und
    `original` nach `english`. Wer wechselt, lässt in der alten Datei unsere
    Einfügungen stehen — und niemand pflegt sie mehr. Lädt das Spiel
    ausgerechnet die, sieht der Spieler **dauerhaft einen alten Stand**, ohne
    dass irgendetwas darauf hindeutet — auch eine später geschriebene Datei
    kann so die alte Form tragen.

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
        errors.record('injection.clean_leftover', exc)
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

    Im Text stehen keine Marken (sie wären im Spiel sichtbar), also wird nach
    der **Form** der Einfügung gesucht. Die alte Marke gilt weiter — in der
    Datei von jemandem, der von einer älteren Version kommt, steht sie noch.

    ⚠ Gesucht wird nur nach **eindeutig eigenen** Formen. Den blanken
    Titelzusatz `<EM4>[BP]</EM4>` schreiben MrKrakens StarStrings und der SC
    Deutsch Launcher genauso — wer eines von beiden benutzt, bekäme sonst
    „eingetragen" angezeigt, ohne dass der Watcher je etwas eingetragen hätte.
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
# ⚠ Diese beiden Funktionen stehen frei, nicht als Methoden im
# Einstellungsfenster: Auch der **Diagnosebericht** muss den Zustand der
# Injektion erfahren, ohne dass ein Fenster offen ist. Fenster wie Bericht
# fragen dieselbe Stelle.

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
        errors.record('injection._lang_order', exc)
        return order
    if not language:
        return order
    if language in order:
        order.remove(language)
    return [language] + order


def ini_file():
    """Die `global.ini`, um die es geht. (Pfad, Sprachordner, Quelle).

    ⚠ Maßgeblich ist die **gewählte** Textquelle, nicht die zuerst gefundene.
    Mit einer festen Reihenfolge (erst `deutsch`, dann `starstrings`) gewänne
    die erste eingerichtete — wer beide einmal benutzt hat und auf
    StarStrings umstellt, sähe weiter die deutsche Quelle angezeigt. Die
    Reihenfolge greift nur, solange nichts gewählt wurde.
    """
    from . import translation
    chosen = paths.setting('inj_quelle')
    # Alle Quellen samt eigener Adresse — eine feste Liste `deutsch`,
    # `starstrings` übersähe eine französische Wahl.
    order_list = list(translation.SOURCES) + [translation.CUSTOM]
    if chosen in order_list:
        order_list.remove(chosen)
        order_list.insert(0, chosen)
    elif chosen == 'original':
        # Die Originaltexte kommen aus dem Spiel selbst, nicht aus einem
        # fremden Projekt — dort gibt es keine Version zu vermerken.
        #
        # ⚠⚠ **Die Spielsprache entscheidet, nicht die Reihenfolge.** Mit
        # fester Folge `('english', 'german_(germany)')` gewänne die erste
        # vorhandene Datei — beide gibt es fast immer, also **immer Englisch**.
        # Wer sein Spiel auf Deutsch stellt (`g_language = german_(germany)` in
        # der `user.cfg`), bekäme die Angaben in die englische Datei, die das
        # Spiel nie liest — und die Statuszeile zeigte trotzdem Erfolg.
        for lang_folder in _lang_order():
            path = translation.target_ini(lang_folder)
            if path and os.path.isfile(path):
                return path, lang_folder, None
    for source in order_list:
        if translation.installed(source):
            lang_folder = translation.language_folder(source)
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
    # `stand` ist für Spieler lesbar (Datum, aktuell?) — nie die Kennung.
    return {'datei': path, 'drin': inside, 'quelle': source,
            'stand': translation.status_text(source) if source else None}
