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
| 2. Textschlüssel → englischer Titel | die Original-`global.ini` aus der `Data.p4k`, einmal gelesen und als kleine Datei abgelegt |
| 3. Rückfall | der angezeigte Titel selbst, ohne Marken |

## ⚠ Was vor der Suche aus dem Titel heraus muss

1. **Marken in eckigen Klammern** — `[BP]`, `[10 Rep] [BP]*`, `[800 Rep]`.
   Sie stehen nur in der Textdatei, nicht im Issue Council.
2. **Platzhalter** — `Pro Tem Bounty Assignment: ~mission(TargetName)`. Im
   Spiel steht dort ein Name, der in jeder Meldung ein anderer ist. Gesucht
   wird nur mit dem festen Teil, sonst findet die Suche genau diesen einen
   Auftrag und keinen der gleichen Art.
"""
import json
import os
import re
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

_ARCHIVE_TRIED = [False]
_LOCAL_CACHE = {}


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
    # trennt nichts mehr.
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


def original_titles(fetch=True):
    """Die englischen Originaltitel — abgelegt, sonst einmal aus der `Data.p4k`.

    ⚠ Bewusst nicht aus einer losen `english/global.ini` im Spielordner: Dort
    liegt oft eine bearbeitete Fassung (StarStrings), manchmal sogar eine
    Übersetzung. Das Archiv ist immer das Original.
    """
    target = paths.app_file(TITLES_FILE)
    stamp = _archive_stamp()
    saved = {}
    try:
        with open(target, encoding='utf-8') as f:
            data = json.load(f)
        if isinstance(data, dict) and isinstance(data.get('titel'), dict):
            saved = data['titel']
            # ⚠ Ein Spiel-Patch bringt neue Aufträge. Gilt die Ablage noch für
            # dieselbe `Data.p4k`, oder ist das Archiv nicht da (dann gibt es
            # nichts Neueres), bleibt sie.
            if saved and (stamp is None or data.get('archiv') == stamp):
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
    try:
        with open(target + '.neu', 'w', encoding='utf-8') as f:
            json.dump({'archiv': stamp, 'titel': found}, f, ensure_ascii=False)
        os.replace(target + '.neu', target)
    except OSError as exception:
        errors.record('issue_council.original_titles', exception)
    return found


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
    `(kompiliertes Muster, schlüssel)` für Titel mit Platzhalter.
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
                raw = '^' + '.+'.join(
                    re.escape(part) for part in _PLACEHOLDER.split(plain)) + '$'
                try:
                    patterns.append((re.compile(raw, re.I), key))
                except re.error:
                    pass
            else:
                exact.setdefault(plain.lower(), set()).add(key)
    return exact, patterns


def keys_for(title, exact, patterns):
    """Die Schlüssel, die zu einem angezeigten Titel passen — leer, wenn keiner.

    Erst wörtlich, dann über die Muster; unter den Mustern zählen nur die
    genauesten (dieselbe Rangfolge wie `contracts.key_for`).
    """
    plain = ' '.join(contracts.clean(title or '').split())
    if not plain:
        return set()
    hit = exact.get(plain.lower())
    if hit:
        return set(hit)
    matching = [(m.pattern, key) for m, key in patterns if m.match(plain)]
    if not matching:
        return set()
    rank = max((not contracts._pattern_weak(p), contracts._weight(p))
               for p, _k in matching)
    return {k for p, k in matching
            if (not contracts._pattern_weak(p), contracts._weight(p)) == rank}


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
    texts = _local_texts()
    stamp = (_LOCAL_CACHE.get('stamp'), len(originals))
    if _LOCAL_CACHE.get('index_stamp') != stamp:
        _LOCAL_CACHE['index'] = build_index(
            list(texts) + [originals_as_ini(originals)], originals)
        _LOCAL_CACHE['index_stamp'] = stamp
    return _LOCAL_CACHE['index']


def search_text(title, originals=None, texts=None):
    """Der Suchbegriff für einen Auftrag — englisch, wenn er sich finden lässt.

    `originals` und `texts` sind nur zum Prüfen da; sonst kommen sie aus dem
    Archiv und dem Spielordner. Zeigen mehrere Schlüssel auf verschiedene
    englische Titel, gilt der angezeigte Titel — lieber ungenau suchen als nach
    einem anderen Auftrag.
    """
    fallback = clean(title)
    try:
        if originals is None:
            originals = original_titles()
        if not originals:
            return fallback
        if texts is None:
            exact, patterns = _cached_index(originals)
        else:
            exact, patterns = build_index(
                list(texts) + [originals_as_ini(originals)], originals)
        english = {clean(originals[k]) for k in keys_for(title, exact, patterns)}
        # ⚠ Ein Titel, der nur aus Platzhaltern besteht (`~mission(Title)`),
        # passt auf jeden Auftrag und lässt nach dem Aufräumen nichts übrig.
        english.discard('')
        if len(english) == 1:
            return english.pop()
        if len(english) > 1:
            return _common_start(english) or fallback
    except Exception as exception:
        errors.record('issue_council.search_text', exception)
    return fallback


def originals_as_ini(originals):
    """Die Originaltitel wieder als `global.ini`-Text.

    So findet auch ein Spieler ohne Übersetzung seinen Titel — er hat im
    Spielordner meist gar keine `global.ini`.
    """
    return '\n'.join('%s=%s' % (k, v) for k, v in originals.items())


def url_for(title, originals=None, texts=None):
    """Die Adresse der Issue-Council-Suche nach diesem Auftrag."""
    query = urllib.parse.urlencode(
        {'search': search_text(title, originals, texts),
         'sort': 'relevance', 'statuses': 'open'},
        quote_via=urllib.parse.quote)
    return BASE_URL + '?' + query
