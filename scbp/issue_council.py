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
Einen Auftrag im Issue Council suchen — für den Fall, dass er verbuggt ist.

Gebaut wird nur die Adresse der Suche; geöffnet wird sie im Browser des
Spielers. Abgerufen wird vom Issue Council nichts.

## Gesucht wird mit dem englischen Titel

Das Issue Council ist englisch. Im Auftragsverlauf steht der Titel aber so,
wie ihn das Spiel angezeigt hat — mit einer Übersetzung also womöglich
deutsch, französisch oder spanisch. Mit diesem Wortlaut findet die Suche fast
nichts.

| Schritt | Woher |
|---|---|
| 1. Angezeigter Titel → Textschlüssel | Rückwärtssuche in den `global.ini` des Spielordners |
| 2. Textschlüssel → englischer Titel | die Original-`global.ini` aus der `Data.p4k`, einmal gelesen und als kleine Datei abgelegt (Titel und Wortschatz) |
| 3. Kein Schlüssel, aber englische Wörter im Titel | `guess()` — ein alter Titel aus einer Übersetzung, die nicht mehr eingerichtet ist |
| 4. Rückfall | der angezeigte Titel selbst, ohne Marken |

## ⚠ Was vor der Suche aus dem Titel heraus muss

1. **Marken in eckigen Klammern** — `[BP]`, `[10 Rep] [BP]*`, `[800 Rep]`.
   Sie stehen nur in der Textdatei, nicht im Issue Council.
2. **Namen und übersetzte Orte in Platzhaltern** — `Pro Tem Bounty
   Assignment: ~mission(TargetName)`. Dort steht in jeder Meldung ein anderer
   Name; gesucht wird nur mit dem festen Teil. Ein englischer Inhalt, der
   keine Person ist (`Protect Fuel Tanks and Escort Employees`), bleibt — er
   sagt, um welchen Auftrag es geht. Siehe `fill()`.
"""
import json
import os
import re
import threading
import urllib.parse

from . import contracts, errors, paths

BASE_URL = ('https://issue-council.robertsspaceindustries.com'
            '/projects/STAR-CITIZEN/issues')
# Das Spieler-Forum — für alles, was im Issue Council keine Meldung hat.
SPECTRUM_URL = 'https://robertsspaceindustries.com/spectrum/community/SC'

# Die englischen Auftragstitel — `{schlüssel: titel}`. Rund 4.000 Einträge,
# gut ein Viertel Megabyte; die ganze `global.ini` hat zehn.
TITLES_FILE = 'titel-original.json'

# Jede Marke in eckigen Klammern samt angehängtem `*`, `!` oder `?`.
_BRACKETS = re.compile(r'\[[^\]]*\][*!?]*')
# Farbauszeichnungen des Spiels (`<EM4>…</EM4>`), die nach dem Entfernen der
# Klammern leer zurückbleiben.
_EMPHASIS = re.compile(r'</?EM\d*>', re.I)
_PLACEHOLDER = re.compile(r'~mission\([^)]*\)')
# Leere Klammern, die ein herausgenommener Platzhalter hinterlässt:
# `Verified Bounty: ~mission(A) | ~mission(B) (~mission(C))`.
_EMPTY_PARENS = re.compile(r'\(\s*\)')
# Was nach dem Herausnehmen eines Platzhalters am Rand hängen bleibt.
_EDGE = ' \t:;,.-–—|/'
# Verbindungswörter, die ohne den Platzhalter dahinter ins Leere zeigen:
# `Help Headhunters at ~mission(Location)` → `Help Headhunters`.
_DANGLING = {'at', 'in', 'on', 'near', 'from', 'for', 'to', 'of', 'the',
             'and', 'with', 'by'}
# Steht beim Aufräumen dort, wo ein Platzhalter war.
_GAP = '\x00'
_PLACEHOLDER_NAME = re.compile(r'~mission\(([^)]*)\)')
# Platzhalter für Personen — ihr Inhalt ist in jedem Auftrag ein anderer und
# hilft der Suche nie, auch wenn der Name englisch klingt.
_PERSON = re.compile(r'name|target', re.I)
_WORD = re.compile(r"[A-Za-zÀ-ÿ][A-Za-zÀ-ÿ'’-]*")
# Wie viele englische Wörter ein fremdsprachiger Titel mindestens enthalten
# muss, damit danach gesucht wird. Ein einzelnes Wort trifft zu viel.
_MIN_GUESS_WORDS = 2

_ARCHIVE_TRIED = [False]
_LOCAL_CACHE = {}
# Hält Vorladen und Klick auseinander: Wer zuerst kommt, baut den Index, der
# andere wartet und nimmt ihn.
_BUILD_LOCK = threading.RLock()
_WARMED = [False]


def warm_up():
    """Originaltitel und Index im Hintergrund vorbereiten — einmal je Lauf.

    Der allererste Klick läse sonst die `Data.p4k` (gemessen 2,4 s), jeder
    erste Klick nach einem Programmstart die Sprachdateien (0,2 s). Gerufen
    beim Öffnen des Auftragsverlaufs, damit beides fertig ist, bevor jemand
    auf den Käfer klickt.
    """
    if _WARMED[0]:
        return
    _WARMED[0] = True

    def work():
        try:
            originals, _words = _original_data()
            if originals:
                _cached_index(originals)
        except Exception as exception:
            errors.record('issue_council.warm_up', exception)

    threading.Thread(target=work, daemon=True).start()


def clean(title):
    """Der Titel ohne Marken, Auszeichnungen und Platzhalter."""
    text = contracts.clean(title or '')
    text = _BRACKETS.sub(' ', text)
    text = _EMPHASIS.sub(' ', text)
    # ⚠ Ein Verbindungswort fällt nur dort weg, wo ein Platzhalter stand.
    # `Crew Hasn't Checked In` endet von Haus aus auf `In` und behält es.
    text = _PLACEHOLDER.sub(' %s ' % _GAP, text)
    text = _EMPTY_PARENS.sub(' ', text)
    words = text.split()
    while words and words[-1].strip(_EDGE) in ('', _GAP):
        gap = words.pop().strip(_EDGE) == _GAP
        while words and not words[-1].strip(_EDGE):
            words.pop()
        if gap and words and words[-1].strip(_EDGE).lower() in _DANGLING:
            words.pop()
    while words and words[0].strip(_EDGE) in ('', _GAP):
        words.pop(0)
    # Was nach einer Lücke mitten im Titel nur noch Satzzeichen ist (`|`),
    # trennt nichts mehr. ⚠ Nur wenn es eine Lücke gab: `Combat Gauntlet -
    # Scenario #5` behält seinen Strich.
    if any(w.strip(_EDGE) == _GAP for w in words):
        words = [w for w in words if w.strip(_EDGE) not in ('', _GAP)]
    return ' '.join(words).strip(_EDGE)


def _common_start(candidates):
    """Die Wörter, mit denen alle Kandidaten beginnen — mindestens zwei.

    `Verified Bounty: …` gibt es in mehreren Fassungen. Die gemeinsame
    Anfangsfolge trifft sie alle, ohne einen davon zu erfinden.
    """
    split = [c.split() for c in candidates]
    common = []
    for words in zip(*split):
        if len(set(w.lower() for w in words)) != 1:
            break
        common.append(words[0])
    if len(common) < 2:
        return ''
    return clean(' '.join(common))


def titles_from_ini(data):
    """Aus dem Inhalt einer `global.ini` die Titelzeilen — `{schlüssel: text}`.

    `schluessel,P=Text` — das `,P` ist eine Textvariante und gehört nicht zum
    Schlüssel (siehe `contracts._VARIANT`).
    """
    if isinstance(data, bytes):
        data = data.decode('utf-8-sig', 'ignore')
    out = {}
    for line in data.splitlines():
        key, sep, value = line.partition('=')
        if not sep or 'title' not in key.lower():
            continue
        key = key.split(',', 1)[0].strip()
        value = value.strip()
        if key and value:
            out.setdefault(key, value)
    return out


def words_of(text):
    """Die Wörter eines Textes, klein geschrieben."""
    return [w.lower() for w in _WORD.findall(text or '')]


def words_from_ini(data):
    """Jedes Wort, das in der englischen `global.ini` vorkommt.

    Daran lässt sich ablesen, ob ein Wort englisch ist: `Fuel Tanks` steht
    darin, `Asteroiden Bergbaubasis` nicht.
    """
    if isinstance(data, bytes):
        data = data.decode('utf-8-sig', 'ignore')
    found = set()
    for line in data.splitlines():
        _key, sep, value = line.partition('=')
        if sep:
            found.update(words_of(value))
    return found


def original_titles(fetch=True):
    """Die englischen Originaltitel — abgelegt, sonst einmal aus der `Data.p4k`."""
    return _original_data(fetch)[0]


def original_words(fetch=True):
    """Der englische Wortschatz des Spiels — siehe `words_from_ini`."""
    return _original_data(fetch)[1]


def _original_data(fetch=True):
    """`(titel, wörter)` — siehe `_load_original_data`, nie zweimal zugleich."""
    with _BUILD_LOCK:
        return _load_original_data(fetch)


def _load_original_data(fetch=True):
    """`(titel, wörter)` aus der Ablage, sonst einmal aus der `Data.p4k`.

    ⚠ Bewusst nicht aus einer losen `english/global.ini` im Spielordner: Dort
    liegt oft eine bearbeitete Fassung (StarStrings), manchmal sogar eine
    Übersetzung. Das Archiv ist immer das Original.
    """
    target = paths.app_file(TITLES_FILE)
    stamp = _archive_stamp()
    saved = ({}, set())
    try:
        with open(target, encoding='utf-8') as f:
            data = json.load(f)
        if (isinstance(data, dict) and isinstance(data.get('titel'), dict)
                and isinstance(data.get('woerter'), list)):
            saved = (data['titel'], set(data['woerter']))
            # ⚠ Ein Spiel-Patch bringt neue Aufträge. Gilt die Ablage noch für
            # dieselbe `Data.p4k`, oder ist das Archiv nicht da (dann gibt es
            # nichts Neueres), bleibt sie.
            if saved[0] and (stamp is None or data.get('archiv') == stamp):
                return saved
    except (OSError, ValueError):
        pass
    if not fetch or _ARCHIVE_TRIED[0]:
        return saved
    _ARCHIVE_TRIED[0] = True
    try:
        from . import gametext
        raw, _message = gametext.read_from_archive('english')
    except Exception as exception:
        errors.record('issue_council.original_titles', exception)
        return saved
    found = titles_from_ini(raw) if raw else {}
    if not found:
        return saved
    words = words_from_ini(raw)
    try:
        with open(target + '.neu', 'w', encoding='utf-8') as f:
            json.dump({'archiv': stamp, 'titel': found,
                       'woerter': sorted(words)}, f, ensure_ascii=False)
        os.replace(target + '.neu', target)
    except OSError as exception:
        errors.record('issue_council.original_titles', exception)
    return found, words


def _archive_stamp():
    """Größe und Änderungszeit der `Data.p4k` — oder None ohne Archiv."""
    try:
        from . import gametext
        path = gametext.p4k_path()
        if not path:
            return None
        info = os.stat(path)
        return [info.st_size, info.st_mtime_ns]
    except Exception:
        return None


def build_index(texts, known):
    """Angezeigter Titel → Schlüssel, aus den Texten der Sprachdateien.

    `texts` sind die Inhalte der `global.ini` (Zeichenketten), `known` die
    Schlüssel, die es im Original gibt. Rückgabe `(wörtlich, muster)`:
    `wörtlich` ist `{titel_klein: {schlüssel}}`, `muster` eine Liste
    `(kompiliertes Muster, schlüssel, platzhalter, rangmuster)` für Titel mit
    Platzhalter. Das Muster fängt jeden Platzhalter als Gruppe; `platzhalter`
    nennt ihre Namen in derselben Reihenfolge, `rangmuster` ist dieselbe Form
    ohne Gruppen für die Rangfolge aus `contracts`.
    """
    exact, patterns = {}, []
    for text in texts:
        for key, value in titles_from_ini(text).items():
            if key not in known:
                continue
            plain = ' '.join(contracts.clean(value).split())
            if not plain:
                continue
            if '~mission(' in plain:
                # ⚠ Eine Vorlage ohne eigenes Wort (`~mission(Title)
                # (~mission(Item))`) passt auf jeden Titel und füllte ihn mit
                # dem, was gerade dasteht — samt Namen und Übersetzung.
                if not words_of(_PLACEHOLDER.sub(' ', plain)):
                    continue
                parts = [re.escape(part) for part in _PLACEHOLDER.split(plain)]
                try:
                    patterns.append((
                        re.compile('^' + '(.+)'.join(parts) + '$', re.I), key,
                        _PLACEHOLDER_NAME.findall(plain),
                        '^' + '.+'.join(parts) + '$'))
                except re.error:
                    pass
            else:
                exact.setdefault(plain.lower(), set()).add(key)
    return exact, patterns


def matches_for(title, exact, patterns):
    """Was zu einem angezeigten Titel passt — `{schlüssel: {platzhalter: wert}}`.

    Erst wörtlich (dann ohne Werte), dann über die Muster; unter den Mustern
    zählen nur die genauesten (dieselbe Rangfolge wie `contracts.key_for`).
    Leer, wenn nichts passt.
    """
    plain = ' '.join(contracts.clean(title or '').split())
    if not plain:
        return {}
    hit = exact.get(plain.lower())
    if hit:
        return {k: {} for k in hit}
    matching = []
    for pattern, key, names, rank_pattern in patterns:
        found = pattern.match(plain)
        if found:
            matching.append((rank_pattern, key,
                             dict(zip(names, found.groups()))))
    if not matching:
        return {}
    rank = max((not contracts._pattern_weak(p), contracts._weight(p))
               for p, _k, _v in matching)
    return {k: v for p, k, v in matching
            if (not contracts._pattern_weak(p), contracts._weight(p)) == rank}


def keys_for(title, exact, patterns):
    """Die Schlüssel, die zu einem angezeigten Titel passen — leer, wenn keiner."""
    return set(matches_for(title, exact, patterns))


def fill(template, values, vocabulary):
    """Den englischen Titel mit den Werten aus dem Spiel füllen — wo es hilft.

    Ein Wert bleibt nur, wenn er ganz aus englischen Wörtern besteht und keine
    Person ist: `Protect Fuel Tanks and Escort Employees` behält die
    Treibstofftanks, `Help Headhunters at Asteroiden Bergbaubasis` verliert den
    deutschen Ort, `High-Risk Bounty: Brendon Broad` den Namen. Alles, was
    wegfällt, räumt `clean()` auf.
    """
    def _substitute(found):
        name = found.group(1)
        value = (values.get(name) or '').strip()
        words = words_of(value)
        if (value and words and not _PERSON.search(name)
                and all(w in vocabulary for w in words)):
            return value
        return found.group(0)
    return _PLACEHOLDER_NAME.sub(_substitute, template)


def guess(title, originals, vocabulary):
    """Ein fremdsprachiger Titel ohne Eintrag in den Sprachdateien.

    Das passiert, wenn das Spiel den Titel mit einer Übersetzung geschrieben
    hat, die heute nicht mehr eingerichtet ist: `Notfall: Blinding Hope in
    Schwierigkeiten` steht im Verlauf, aber in keiner Datei mehr. Die
    englischen Wörter darin — hier `Blinding Hope` — führen zum Original,
    wenn genau ein englischer Titel sie alle enthält.

    Zweiter Weg, für übersetzte Platzhalter-Inhalte: `Verified Bounty: Name |
    HRT (Großes Mehrbesatzungsschiff …)`. Dort passt der englische Titel, dessen
    eigene Wörter (ohne Platzhalter) **alle** im angezeigten Titel stehen — bei
    mehreren der mit den meisten.

    Gibt `''` zurück, wenn der Titel schon englisch ist oder nichts eindeutig
    passt; dann gilt der angezeigte Titel.
    """
    plain = clean(title)
    shown = _WORD.findall(plain)
    if not shown or all(w.lower() in vocabulary for w in shown):
        return ''
    shown_lower = set(w.lower() for w in shown)

    significant = set(w.lower() for w in shown
                      if w.lower() in vocabulary and w.lower() not in _DANGLING
                      and len(w) > 2)
    if len(significant) >= _MIN_GUESS_WORDS:
        candidates = {clean(v) for v in originals.values()
                      if significant <= set(words_of(v))}
        candidates.discard('')
        if len(candidates) == 1:
            return candidates.pop()
        if len(candidates) > 1:
            common = _common_start(candidates)
            if common:
                return common

    best, best_size = set(), 0
    for value in originals.values():
        own = set(words_of(_PLACEHOLDER.sub(' ', value))) - _DANGLING
        if len(own) < _MIN_GUESS_WORDS or not own <= shown_lower:
            continue
        if len(own) > best_size:
            best, best_size = {clean(value)}, len(own)
        elif len(own) == best_size:
            best.add(clean(value))
    best.discard('')
    if len(best) == 1:
        return best.pop()
    return _common_start(best) if best else ''


def _local_texts():
    """Die Inhalte aller `global.ini` im Spielordner, gemerkt bis zur Änderung."""
    files = contracts._ini_files()
    stamp = []
    for path in files:
        try:
            info = os.stat(path)
            stamp.append((path, info.st_mtime_ns, info.st_size))
        except OSError:
            stamp.append((path, None, None))
    stamp = tuple(stamp)
    if _LOCAL_CACHE.get('stamp') == stamp:
        return _LOCAL_CACHE['texts']
    texts = []
    for path in files:
        try:
            with open(path, encoding='utf-8', errors='ignore') as f:
                texts.append(f.read())
        except OSError:
            continue
    _LOCAL_CACHE.clear()
    _LOCAL_CACHE.update(stamp=stamp, texts=texts)
    return texts


def _cached_index(originals):
    """`build_index` über den Spielordner — einmal gebaut, bis sich eine
    Sprachdatei ändert. Zwei Dateien zu je zehn Megabyte bei jedem Klick neu zu
    lesen, kostet spürbar Zeit."""
    with _BUILD_LOCK:
        return _build_cached_index(originals)


def _build_cached_index(originals):
    """Der eigentliche Bau hinter `_cached_index` — nur unter der Sperre."""
    texts = _local_texts()
    stamp = (_LOCAL_CACHE.get('stamp'), len(originals))
    if _LOCAL_CACHE.get('index_stamp') != stamp:
        _LOCAL_CACHE['index'] = build_index(
            list(texts) + [originals_as_ini(originals)], originals)
        _LOCAL_CACHE['index_stamp'] = stamp
    return _LOCAL_CACHE['index']


def search_text(title, originals=None, texts=None, vocabulary=None):
    """Der Suchbegriff für einen Auftrag — englisch, wenn er sich finden lässt.

    `originals`, `texts` und `vocabulary` sind nur zum Prüfen da; sonst kommen
    sie aus dem Archiv und dem Spielordner. Zeigen mehrere Schlüssel auf
    verschiedene englische Titel, gilt ihre gemeinsame Anfangsfolge, sonst der
    angezeigte Titel — lieber ungenau suchen als nach einem anderen Auftrag.
    """
    fallback = clean(title)
    try:
        if originals is None:
            originals, vocabulary = _original_data()
        if not originals:
            return fallback
        if vocabulary is None:
            vocabulary = set()
            for value in originals.values():
                vocabulary.update(words_of(value))
        if texts is None:
            exact, patterns = _cached_index(originals)
        else:
            exact, patterns = build_index(
                list(texts) + [originals_as_ini(originals)], originals)
        found = matches_for(title, exact, patterns)
        english = {clean(fill(originals[k], values, vocabulary))
                   for k, values in found.items()}
        # ⚠ Ein Titel, der nur aus Platzhaltern besteht (`~mission(Title)`),
        # passt auf jeden Auftrag und lässt nach dem Aufräumen nichts übrig.
        english.discard('')
        if len(english) == 1:
            return english.pop()
        if len(english) > 1:
            return _common_start(english) or fallback
        return guess(title, originals, vocabulary) or fallback
    except Exception as exception:
        errors.record('issue_council.search_text', exception)
    return fallback


def originals_as_ini(originals):
    """Die Originaltitel wieder als `global.ini`-Text.

    So findet auch ein Spieler ohne Übersetzung seinen Titel — er hat im
    Spielordner meist gar keine `global.ini`.
    """
    return '\n'.join('%s=%s' % (k, v) for k, v in originals.items())


def url_for(title, originals=None, texts=None, vocabulary=None):
    """Die Adresse der Issue-Council-Suche nach diesem Auftrag."""
    query = urllib.parse.urlencode(
        {'search': search_text(title, originals, texts, vocabulary),
         'sort': 'relevance', 'statuses': 'open'},
        quote_via=urllib.parse.quote)
    return BASE_URL + '?' + query
