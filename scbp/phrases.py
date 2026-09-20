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
Die Bauplan-Meldung in der Game.log erkennen — in jeder Spielsprache.

Beim Freischalten schreibt Star Citizen eine Zeile wie

    <SHUDEvent_OnNotification> Added notification "Bauplan erhalten: Attrition-5 Repeater: " [136] …

Der Text davor ist **übersetzt**. Bis v1.5.0 stand die deutsche Formulierung fest
im Code — bei englischem Client griff die Sofort-Meldung deshalb gar nicht. Das
ist unter Linux keine Randerscheinung mehr, dort spielen die meisten auf Englisch.

Drei Quellen, in dieser Rangfolge:

  1. **Eigene Ergänzung** — `phrasen.json` im App-Ordner. Wer eine Formulierung
     findet, die hier fehlt, trägt sie ein, ohne auf eine neue Version zu warten.
  2. **Die `global.ini` der eigenen Installation** — die genaueste Quelle, denn
     sie ist die Datei, aus der das Spiel den Text selbst nimmt. Dort steht

         crafting_hud_notification_received_blueprint,P=Bauplan erhalten: %s

     Daraus lässt sich das Suchmuster exakt bauen. Sie liegt aber nur entpackt
     vor, wenn jemand sie ausgepackt hat (beim deutschen Client tut das der
     SC Deutsch Launcher) — sonst steckt sie in `Data.p4k`.
  3. **Die mitgelieferte Tabelle** unten — greift immer.

> Stand: **Deutsch und Englisch sind beide gemessen.** Deutsch an 127
> Log-Sicherungen gegengeprüft; Englisch am 24.08.2026 an einem echten
> englischen Client bestätigt — der Client schreibt:
>
>     Added notification "Received Blueprint: Aves Shrike Helmet: "
>
> Damit ist die Rateliste erledigt: `Received Blueprint` steht vorn, die vier
> übrigen Kandidaten bleiben als Rückfall stehen. Andere Sprachen erschließt
> der Watcher sich selbst aus den Logs (siehe `selbst_finden`); wer nachhelfen
> will, holt den Wortlaut mit `tools/extract_global_ini.py --sprache <name>`
> aus der eigenen Installation.
"""
import json
import os
import re

from . import paths

# Der sprachneutrale Schlüssel — er ist in allen Sprachen derselbe.
INI_KEY = 'crafting_hud_notification_received_blueprint'

# Mitgelieferte Formulierungen. Alle werden gleichzeitig gesucht: Eine Phrase, die
# es in der eigenen Sprache nicht gibt, kann keinen Fehltreffer erzeugen — die
# Zeilenform drumherum ist zu eigen, als dass sie zufällig entstünde.
TABLE = {
    # ⚠ Schweizerdeutsch ist eine **eigene Fassung** derselben Übersetzung
    # (`live-CH`) und formuliert anders. Ohne den Eintrag scheitert der Watcher
    # dort **still**: keine Fehlermeldung, keine übersprungene Datei, einfach
    # null Baupläne. Belegt über den Bauplan-Ausleser des KRT-Basetools
    # (GPL-3.0), der ihn seinerseits gegen `rjcncpt/StarCitizen-Deutsch-INI`
    # geprüft hat — die Quelle, die auch der SC Deutsch Launcher einspielt.
    #
    # Greift nur als Rückfall: Liegt eine lesbare `global.ini` vor, gewinnt die
    # immer. Für eine englische Werksinstallation (deren Textdatei in der
    # `Data.p4k` steckt) ist diese Liste aber das Einzige, was bleibt.
    'de': ['Bauplan erhalten',                      # gemessen
           'Bauplan überchoo'],                     # live-CH
    # 'Received Blueprint' ist seit 24.08.2026 **gemessen** — an einem echten
    # englischen Client, Zeile:
    #   Added notification "Received Blueprint: Aves Shrike Helmet: "
    # Deshalb steht es vorn. Die vier dahinter waren die übrigen Kandidaten und
    # bleiben stehen: Sie kosten nichts, und sollte CIG die Formulierung einmal
    # ändern, ist die Chance nicht schlecht, dass eine davon dann zutrifft.
    'en': ['Received Blueprint',                    # gemessen
           'Blueprint Received', 'Blueprint Acquired',
           'Blueprint Obtained', 'Blueprint Unlocked'],   # weitere Kandidaten
}

# Nur diese Zeilen zählen. Die anderen Notification-Zeilen sind Ein- und
# Ausblende-Ereignisse — wer sie mitzählt, meldet jeden Bauplan mehrfach.
FRAME = r'Added notification "(?:%s):\s*(.+?)\s*:\s*"'

# Dasselbe ohne feste Phrase: Damit lässt sich herausfinden, WIE die Meldung in
# einer unbekannten Sprache lautet — siehe `selbst_finden()`.
FRAME_OPEN = re.compile(r'Added notification "([^":]{3,60}):\s*(.+?)\s*:\s*"')

# ⚠⚠ **Ein Name, den das Spiel nicht übersetzen konnte** — Star Citizen
# schreibt dann den rohen Textschlüssel mit `@` davor, auch in die
# Bauplan-Meldung. Gemeldet am 20.09.2026 (Bushwick4712, v3.53.3): Drei
# Baupläne standen als `@Nozzle_FuelGiver_GRIN_NozzleSecure_Name` im Bestand —
# **dauerhaft**, denn der Name IST dort der Schlüssel. Die Zeile im
# Flottenmanager heilt von selbst, sobald die Übersetzung nachzieht; ein
# einmal so gespeicherter Bauplan nicht.
UNRESOLVED = re.compile(r'^@([A-Za-z0-9_]+)$')

# Aufgelöste Schlüssel — ein Fund kostet einen Dateidurchlauf, der kommt nicht
# zweimal.
_KEY_CACHE = {}


def _ini_files():
    """Alle entpackten `global.ini` der Installation (kann leer sein)."""
    folder = paths.localization_folder()
    if not folder:
        return []
    found = []
    try:
        for language in sorted(os.listdir(folder)):
            p = os.path.join(folder, language, 'global.ini')
            if os.path.isfile(p):
                found.append(p)
    except OSError:
        pass
    return found


# {Quellenmarke: Formulierung} — siehe `_aus_ini()`.
_INI_CACHE = {}


def _ini_mark(path):
    """Woran man erkennt, dass diese `global.ini` sich geändert hat.

    ⚠ Absichtlich **billig**: Pfad, Größe, Zeitstempel in Nanosekunden —
    kein Lesen. Dieselbe Technik wie bei den Joystick-Klarnamen; die dortigen
    Lehren gelten hier genauso (Sekundenrundung reicht nicht, und der Merker
    gehört in das Modul, dem die Daten gehören).
    """
    try:
        z = os.stat(path)
        return '%s:%d:%d' % (os.path.realpath(path), z.st_size, z.st_mtime_ns)
    except OSError:
        return None


def _from_ini(path):
    """Die Formulierung aus einer `global.ini` — oder None.

    Gelesen wird zeilenweise und nur bis zum Treffer: Die Datei ist mehrere
    Megabyte groß, sie komplett in den Speicher zu holen wäre unnötig.

    ⭐⭐ **Und das Ergebnis wird gemerkt.** Der Fehlerbericht fragt zweimal
    nach den Formulierungen (`sammeln()` und `gemessene()`), und die
    Diagnose-Seite baut den Bericht bei jedem Anzeigen neu. Gemessen am
    13.09.2026: **97 ms allein für diese Funktion**, bei jedem Klick auf die
    Seite — für eine Datei, die sich nur mit einem Spiel-Patch ändert.

    ⚠ Der Merker hängt an der **Quellenmarke**, nicht an der Programmsitzung:
    Ein Patch während des Betriebs wird erkannt.
    """
    mark = _ini_mark(path)
    if mark is not None and mark in _INI_CACHE:
        return _INI_CACHE[mark]
    result = _read_ini(path)
    if mark is not None:
        _INI_CACHE[mark] = result
    return result


def _read_ini(path):
    """Der eigentliche Lesevorgang — ohne Merker, für `_aus_ini()`."""
    try:
        with open(path, encoding='utf-8-sig', errors='ignore') as f:
            for line in f:
                if not line.startswith(INI_KEY):
                    continue
                # Format: schluessel,P=Text mit %s   (das ,P ist optional)
                value = line.split('=', 1)[1].strip() if '=' in line else ''
                before, _separator, after = value.partition('%s')
                # ⚠⚠ **Steht Text HINTER dem Namen, muss die ganze Formulierung
                # erhalten bleiben.** Bisher wurde nur der Teil davor genommen —
                # bei „Bauplan erhalten: %s" ist das richtig und bleibt es. Eine
                # umgestellte Übersetzung wie „%s ist eingetroffen" hätte davor
                # aber gar nichts stehen: `vorne` wäre leer, die Erkennung fiele
                # auf die mitgelieferte Tabelle zurück und fände **nichts** —
                # ohne Fehlermeldung, ohne übersprungene Datei, einfach null
                # Baupläne. Genau diese stille Art zu scheitern ist die
                # gefährlichste. (Kniff aus dem Bauplan-Ausleser des
                # KRT-Basetools, GPL-3.0.)
                #
                # Heute formuliert keine Sprache so — der Zweig kostet nichts
                # und deckt den Tag ab, an dem CIG es tut.
                if after.strip().strip(':').strip():
                    return value.strip() or None
                # Ein abschließender Doppelpunkt gehört zum Rahmen, nicht zur Phrase
                front = before.strip().rstrip(':').strip()
                return front or None
    except OSError:
        return None
    return None


def _lookup_key(key):
    """Den Text zu einem Schlüssel aus den vorhandenen `global.ini` — oder None.

    Durchsucht alle entpackten Sprachdateien der Installation, die deutsche
    wie die englische: Fehlt der Eintrag in der einen, steht er vielleicht in
    der anderen. Ein englischer Name ist allemal besser als ein roher
    Schlüssel.
    """
    needle = key.lower() + '='
    short = key.lower() + ',p='
    for path in _ini_files():
        try:
            with open(path, encoding='utf-8-sig', errors='ignore') as f:
                for line in f:
                    low = line[:len(key) + 3].lower()
                    if low.startswith(needle) or low.startswith(short):
                        # Ein Stern vorn ist eine fremde Marke (StarStrings,
                        # SC Deutsch Launcher) — er gehört nicht zum Namen.
                        text = line.split('=', 1)[1].strip().lstrip('*').strip()
                        if text:
                            return text
        except OSError:
            continue
    # ⭐ Letzte Quelle: die Originalnamen aus der `Data.p4k`. Sie sind der
    # einzige Weg für alle, die weder eine gepflegte Übersetzung noch eine
    # entpackte englische `global.ini` haben — und das ist der Normalfall
    # (gemeldet 20.09.2026, `inj_quelle=original`).
    try:
        from . import gametext
        namen = gametext.saved_names()
        if not namen:
            namen = _archive_names_once()
        return namen.get(key)
    except Exception:
        return None


# Ein Versuch je Programmlauf — nicht je Name. Ohne den Riegel läse ein
# Log-Abschnitt mit drei unbekannten Namen dreimal dasselbe Archiv.
_ARCHIVE_TRIED = [False]


def _archive_names_once():
    """Die Originalnamen einmalig aus der `Data.p4k` holen und ablegen.

    ⚠⚠ **Bestandsnutzer gehen nie wieder durch die Einrichtung.** Genau sie
    sind aber betroffen: Wer das Werkzeug seit Monaten benutzt, hat den
    Assistenten einmal gesehen und danach nie wieder (`einrichtung_fertig`).
    Deshalb holt sich das Werkzeug die Namen beim ersten unauflösbaren
    Schlüssel selbst, statt eine Hinweiszeile zu setzen, die niemand liest.

    ⚠ Gemessen an einer echten Installation (4.10.1): **1,0 s** für 10 MB aus
    einem 144-GB-Archiv. Der frühere Verdacht „das dauert Minuten" war eine
    Rechnung über 1,3 Mio Verzeichniseinträge, keine Messung. Auf einer
    langsamen Platte kann der erste, kalte Zugriff länger dauern — deshalb
    läuft der Aufruf dort, wo auch die Log-Auswertung läuft (eigener Faden),
    und **nicht** in der Oberfläche.
    """
    if _ARCHIVE_TRIED[0]:
        return {}
    _ARCHIVE_TRIED[0] = True
    from . import gametext
    daten, _meldung = gametext.read_from_archive('english')
    if not daten:
        return {}
    gametext.save_names(daten)
    return gametext.saved_names()


def _readable(key):
    """Aus einem Schlüssel einen lesbaren Namen bauen — der letzte Ausweg.

    `Nozzle_FuelGiver_GRIN_NozzleSecure_Name` → `Nozzle FuelGiver GRIN
    NozzleSecure`. Kein schöner Name, aber einer, den ein Mensch vorlesen
    kann — und vor allem keiner, der wie ein Programmfehler aussieht.
    """
    text = key
    for tail in ('_Name', '_name', '_Title', '_title'):
        if text.endswith(tail):
            text = text[:-len(tail)]
            break
    return text.replace('_', ' ').strip() or key


def resolve_key(name):
    """Einen unaufgelösten Textschlüssel in einen lesbaren Namen wandeln.

    Alles andere kommt unverändert zurück — die Funktion darf an **jedem**
    Namen vorbeilaufen, ohne etwas anzufassen.

    ⚠⚠ **Warum das beim EINLESEN passiert und nicht erst beim Anzeigen:** Der
    Name ist im Bestand der Schlüssel. Was einmal als `@…_Name` gespeichert
    wurde, bleibt es auch dann, wenn die Übersetzung später nachzieht — und
    würde beim nächsten Fund als **zweiter** Eintrag danebenstehen.

    ⚠ Findet sich nichts, wird der Schlüssel lesbar gemacht statt verworfen.
    Der Bauplan ist ja echt: Wer ihn freigeschaltet hat, soll ihn im Bestand
    finden, auch wenn CIG den Text noch nicht übersetzt hat.
    """
    if not name:
        return name
    match = UNRESOLVED.match(name.strip())
    if not match:
        return name
    key = match.group(1)
    if key not in _KEY_CACHE:
        _KEY_CACHE[key] = _lookup_key(key) or _readable(key)
    return _KEY_CACHE[key]


def _own():
    """Selbst ergänzte Formulierungen aus `phrasen.json` im App-Ordner.

    Format:  {"phrasen": ["Blueprint Received"]}"""
    try:
        with open(paths.app_file('phrasen.json'), encoding='utf-8') as f:
            values = json.load(f).get('phrasen') or []
        return [str(p).strip() for p in values if str(p).strip()]
    except Exception:
        return []


def find_self(catalog_names, backups, at_most=40):
    """Die Bauplan-Phrase aus den eigenen Logs erschließen — in jeder Sprache.

    Der Kniff: Wir kennen alle 714 Bauplan-Namen. Steht in einer Logzeile

        Added notification "IRGENDWAS: Attrition-5 Repeater: "

    und ist „Attrition-5 Repeater" ein bekannter Bauplan, dann ist IRGENDWAS die
    gesuchte Formulierung. Das funktioniert für Französisch und Spanisch genauso
    wie für Englisch — ohne dass jemand die Sprache vorher kennen muss.

    Verlangt werden **mindestens zwei** verschiedene Treffer für dieselbe Phrase.
    Bei nur einem könnte es Zufall sein: Ein Bauplan-Name taucht auch in anderen
    Meldungen auf („Auftrag abgeschlossen: Attrition-5 Repeater geliefert").

    Rückgabe: die gefundene Phrase oder None.
    """
    if not catalog_names or not backups:
        return None
    known = {str(n).lower().strip() for n in catalog_names}
    counter = {}
    for file in backups[-at_most:]:
        try:
            with open(file, 'rb') as f:
                text = f.read().decode('utf-8', 'ignore')
        except OSError:
            continue
        for m in FRAME_OPEN.finditer(text):
            phrase, name = m.group(1).strip(), m.group(2).strip()
            # Klassen-Zusatz abschneiden, sonst passt kein Name auf den Katalog
            name = re.sub(r'\s*\((?:Civ|Mil|Ind|Sth|Cmp)/\d+/[A-D]\)\s*$', '',
                          name, flags=re.I).strip()
            if name.lower() in known:
                counter.setdefault(phrase, set()).add(name.lower())
    hit = [(len(names), p) for p, names in counter.items() if len(names) >= 2]
    if not hit:
        return None
    hit.sort(reverse=True)
    return hit[0][1]


def remember(phrase):
    """Eine gefundene Formulierung dauerhaft festhalten.

    Sie landet in derselben `phrasen.json`, die auch von Hand gepflegt werden
    kann — es gibt keine zweite, versteckte Wahrheit."""
    if not phrase:
        return False
    existing = _own()
    if phrase in existing:
        return False
    try:
        with open(paths.app_file('phrasen.json'), 'w', encoding='utf-8') as f:
            json.dump({'phrasen': existing + [phrase],
                       '_hinweis': 'Formulierungen, an denen ein neuer Bauplan '
                                   'im Spiel-Log erkannt wird. Selbst gefundene '
                                   'stehen hier mit drin.'},
                      f, ensure_ascii=False, indent=1)
        return True
    except OSError:
        return False


def collect():
    """Alle Formulierungen, nach denen gesucht wird — samt Herkunft.

    Rückgabe: (liste_der_phrasen, herkunft) — Herkunft ist 'ini', 'eigen'
    oder 'tabelle', je nachdem, was den genauesten Beitrag geliefert hat."""
    phrase_list, origin = [], 'tabelle'
    for p in _own():
        if p not in phrase_list:
            phrase_list.append(p)
            origin = 'eigen'
    for file in _ini_files():
        p = _from_ini(file)
        if p and p not in phrase_list:
            phrase_list.append(p)
            origin = 'ini'
    for language in TABLE.values():
        for p in language:
            if p not in phrase_list:
                phrase_list.append(p)
    return phrase_list, origin


def measured():
    """Nur die belegten Formulierungen, getrennt: (eigene, aus_ini).

    ⚠ Für den Bericht. `sammeln()` liefert eine **gemischte** Liste — belegte
    Formulierungen und die eingebaute Rückfalltabelle — dazu **eine** Herkunft
    für alles. Im Bericht stand deshalb hinter der ganzen Liste „aus der
    global.ini des Spiels", obwohl dort genau eine davon herkam. Wer die
    übrigen dort sucht, sucht umsonst: am 01.09.2026 kostete das drei
    Suchläufe, bis klar war, dass „Bauplan überchoo" aus der Tabelle stammt
    (Schweizerdeutsch) und gar nicht in der `global.ini` stehen kann."""
    own = []
    for p in _own():
        if p not in own:
            own.append(p)
    from_ini = []
    for file in _ini_files():
        p = _from_ini(file)
        if p and p not in own and p not in from_ini:
            from_ini.append(p)
    return own, from_ini


def split_phrase(phrase):
    """Eine Formulierung in Vor- und Nachtext um den Bauplan-Namen herum.

    `Bauplan erhalten: %s`  →  `('Bauplan erhalten', '')`
    `%s ist eingetroffen`   →  `('', 'ist eingetroffen')`
    `Bauplan erhalten`      →  `('Bauplan erhalten', '')`  (bloße Beschriftung)
    """
    if '%s' not in phrase:
        return phrase.strip().rstrip(':').strip(), ''
    before, _separator, after = phrase.partition('%s')
    return (before.strip().rstrip(':').strip(),
            after.strip().strip(':').strip())


def pattern(phrase_list=None):
    """Fertiger regulärer Ausdruck für die Log-Zeilen.

    ⚠ **Der Ausdruck kann mehrere Klammergruppen haben.** Beschriftungen vor
    dem Namen teilen sich eine (der Normalfall, unverändert); jede umgestellte
    Formulierung bekommt eine eigene, weil ihr Muster anders gebaut ist.
    `logsource._names_from_text` nimmt deshalb die **erste gefüllte** Gruppe und
    nicht stur Gruppe 1.
    """
    if phrase_list is None:
        phrase_list, _ = collect()
    front, tail = [], []
    for p in phrase_list:
        v, n = split_phrase(p)
        if n:
            tail.append((v, n))
        elif v:
            front.append(v)
    parts = []
    # Der Normalfall — alle Beschriftungen in EINER Alternative, exakt wie
    # bisher. Solange nichts Umgestelltes dazukommt, ist der Ausdruck
    # zeichengleich mit dem von vorher.
    if front:
        parts.append(FRAME % '|'.join(re.escape(v) for v in front))
    for v, n in tail:
        head = (re.escape(v) + r':?\s*') if v else r'\s*'
        parts.append(r'Added notification "%s(.+?)\s+%s\s*:\s*"'
                     % (head, re.escape(n)))
    if not parts:
        # Nichts zu suchen — ein Ausdruck, der nie trifft, ist besser als einer,
        # der auf jede Meldung passt.
        return re.compile(r'(?!)')
    return re.compile('|'.join(parts))


def confirmed():
    """Steht die Formulierung fest — oder wird geraten?

    True, sobald sie aus der eigenen `global.ini` oder aus `phrasen.json` stammt.
    Bei einem deutschen Client ist sie auch aus der Tabelle heraus verlässlich,
    weil genau diese gemessen wurde."""
    _, origin = collect()
    return origin in ('ini', 'eigen')


if __name__ == '__main__':
    ps, whence = collect()
    print('Herkunft:', whence, '· bestätigt:', confirmed())
    for p in ps:
        print(' ·', p)
