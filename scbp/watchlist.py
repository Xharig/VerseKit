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
Die Merkliste — Baupläne, auf die man wartet.

Trägt man einen Bauplan hier ein, meldet der Watcher ihn **auffällig**, sobald
er auftaucht: gold statt grün, mit Stern.

⭐⭐ **Danach bleibt er stehen — als erledigt** (seit 16.09.2026). Bis dahin
flog ein freigeschalteter Bauplan von selbst raus. Dann aber lässt sich kein
Fortschritt über die Merkliste zeigen: Aeternitas26 (KRT) wünschte sich den
Bauplan-Fortschritt „nur für die als Favoriten markierten Baupläne", und mit
einer Liste, aus der alles Erreichte verschwindet, stünde der immer bei null.
Entschieden am 16.09.2026: Erledigte bleiben, die Anzeige hakt sie ab (erledigt
ist, was im Bestand steht — das wird nicht zusätzlich gespeichert).

⚠ Das hebt die Regel vom 06.09.2026 auf („da wird einer beobachtet, den ich
schon habe"): Ein gemerkter Bauplan, den man hat, steht jetzt **mit Haken**
in der Liste, nicht als offen. **Eigene Beobachtungen mit Suchmuster** fliegen
beim Fund weiter raus — sie stehen für „irgendein passendes Teil", nicht für
einen Bauplan, und zählen im Fortschritt nicht mit.

Gepflegt wird sie **im Fenster mit einem Klick** — niemand soll dafür eine
Datei bearbeiten müssen. Die Datei (`watchlist.json`) bleibt trotzdem lesbar
und von Hand änderbar, denn ein eigenes Werkzeug des Autors schreibt dort Teile der
Ausrüstungsliste hinein.

Zwei Arten von Einträgen leben nebeneinander:

  **Namen** — was im Fenster angeklickt wurde. Genauer Abgleich.
  **Muster** — Teilstücke eines Namens, von außen eingetragen (der Skill kennt
  die endgültigen Namen künftiger Gegenstände ja noch nicht). Trifft ein Muster,
  gilt der Eintrag als erfüllt.

Format:

    {
      "namen": ["Attrition-5 Repeater"],
      "eintraege": [{"titel": "Helm meiner Wahl", "muster": ["adp-mk4", "woodland"]}]
    }

⚠ Bis zum 11.09.2026 hieß dieses Modul `merkliste` (Sprachumstellung P4,
Stufe 1). **Nur Bezeichner sind umbenannt, keine Zeichenketten.** Bewusst
gleich geblieben, weil sie in der Datei jedes Nutzers stehen: die Schlüssel
`namen`, `eintraege`, `titel` und `muster`. Ebenso die Schlüssel, die
`all_entries()` für die Anzeige liefert (`titel`, `art`, `muster`), und die
Fehlerkennung wechselt nur ihren Namen, nicht ihre Bedeutung. Der alte
Dateiname `merkliste.json` steht weiter in `paths.SUBFOLDERS` — er gehört zu
Ablagen aus früheren Fassungen, nicht zu diesem Modul.
"""
import re
import json
import os

from . import paths

FILE = 'watchlist.json'


def _norm(s):
    """Vergleichsform eines Namens — siehe `paths.name_key`."""
    return paths.name_key(s)


def path():
    return paths.app_file(FILE)


def load():
    """Die Merkliste. Fehlt die Datei, ist sie leer — das ist kein Fehler."""
    try:
        with open(path(), encoding='utf-8') as f:
            d = json.load(f)
    except Exception:
        return {'namen': [], 'eintraege': []}
    if not isinstance(d, dict):
        return {'namen': [], 'eintraege': []}
    d.setdefault('namen', [])
    d.setdefault('eintraege', [])
    if not isinstance(d['namen'], list):
        d['namen'] = []
    if not isinstance(d['eintraege'], list):
        d['eintraege'] = []
    return d


def save(data):
    """Schreibt über eine Nebendatei, damit ein Absturz nichts zerreißt."""
    target = path()
    tmp = target + '.tmp'
    try:
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(tmp, 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False, indent=1)
        os.replace(tmp, target)
        return True
    except OSError as exc:
        try:
            from . import errors
            errors.record('watchlist.save', exc)
        except Exception:
            pass
        try:
            os.remove(tmp)
        except OSError:
            pass
        return False


# ---------------------------------------------------------------- Nach außen
def names(data=None):
    """Die angeklickten Namen in Vergleichsform."""
    return {_norm(n) for n in (data or load())['namen']}


def contains(name, data=None):
    return _norm(name) in names(data)


def add(name, data=None):
    """Aufnehmen. Gibt die geänderten Daten zurück (noch nicht gespeichert)."""
    data = data or load()
    if not contains(name, data):
        data['namen'].append(name.strip())
    return data


def remove(name, data=None):
    """Herausnehmen — auch aus den Muster-Einträgen, falls einer greift."""
    data = data or load()
    key = _norm(name)
    data['namen'] = [n for n in data['namen'] if _norm(n) != key]
    data['eintraege'] = [e for e in data['eintraege']
                         if not _pattern_matches(e, key)]
    return data


def remove_entry(title, data=None):
    """Eine **eigene Beobachtung** herausnehmen — über ihren Titel.

    ⚠ Nicht dasselbe wie `remove()`. Das nimmt einen Bauplan-Namen heraus
    und wirft dabei jede Muster-Beobachtung mit weg, die auf ihn passt. Hier
    geht es um die Beobachtung selbst: „Helm meiner Wahl" abwählen,
    weil die Staffel ein anderes Teil nimmt — die Baupläne, die das Muster
    zufällig trifft, gehen niemanden etwas an.

    Gibt die geänderten Daten zurück (noch nicht gespeichert).
    """
    data = data or load()
    wanted = (title or '').strip().lower()
    data['eintraege'] = [e for e in data['eintraege']
                         if (e.get('titel') or '').strip().lower() != wanted]
    return data


def toggle(name):
    """Klick im Fenster: rein oder raus. Gibt zurück, ob er jetzt drin ist."""
    data = load()
    inside = contains(name, data)
    data = remove(name, data) if inside else add(name, data)
    save(data)
    return not inside


def _pattern_matches(entry, name_norm):
    """Passt eine Beobachtung auf diesen Namen?

    ⚠ **An Wortgrenzen, nicht mitten im Wort.** Ein blosses „steckt drin"
    liefert falsche Treffer, die niemand als solche erkennt: Das Muster
    `arden backpack` traf am 29.08.2026 auf *W**arden** Backpack Purgatory
    Camo* — und der Watcher meldete ein Rüstungsteil als verfügbar, das mit
    der gesuchten Ausrüstung nichts zu tun hat. Wer sich darauf verlässt,
    fliegt umsonst los.

    Vor und hinter dem Muster darf deshalb kein weiterer Buchstabe und keine
    Ziffer stehen. Bindestriche und Leerzeichen zählen als Grenze, damit
    `abc-mk4 legs grey` weiter passt.
    """
    patterns = [str(p).lower().strip() for p in (entry.get('muster') or [])]
    for p in patterns:
        if not p:
            continue
        if re.search(r'(?<![a-z0-9])%s(?![a-z0-9])' % re.escape(p), name_norm):
            return True
    return False


def match(name, data=None):
    """Wird auf diesen Bauplan gewartet? Rückgabe: Titel des Eintrags oder None.

    Bei einem angeklickten Namen ist der Titel der Name selbst, bei einem
    Muster-Eintrag dessen Titel („Helm meiner Wahl")."""
    data = data or load()
    key = _norm(name)
    for n in data['namen']:
        if _norm(n) == key:
            return n
    for e in data['eintraege']:
        if _pattern_matches(e, key):
            return e.get('titel') or name
    return None


def fulfill(name):
    """Ein Wunsch ist erfüllt. Gibt den Titel zurück, auf den gewartet wurde.

    Wird aufgerufen, sobald ein Bauplan im eigenen Bestand landet.

    ⚠ **Ein angeklickter Name bleibt stehen** — er zählt ab jetzt als erledigt,
    weil er im Bestand steht (siehe Modulkopf). **Nur Muster-Beobachtungen
    fliegen raus**: Sie warten auf „ein passendes Teil", und das ist jetzt da.
    """
    data = load()
    title = match(name, data)
    if not title:
        return None
    key = _norm(name)
    kept = [e for e in data['eintraege'] if not _pattern_matches(e, key)]
    if len(kept) != len(data['eintraege']):
        data['eintraege'] = kept
        save(data)
    return title


def all_entries(data=None):
    """Alles, worauf gewartet wird — für die Anzeige. Namen zuerst."""
    data = data or load()
    items = [{'titel': n, 'art': 'name'} for n in sorted(data['namen'])]
    items += [{'titel': e.get('titel') or '?', 'art': 'muster',
               'muster': e.get('muster') or []}
              for e in data['eintraege']]
    return items


def count(data=None):
    data = data or load()
    return len(data['namen']) + len(data['eintraege'])


if __name__ == '__main__':
    print('Datei:', path())
    for e in all_entries():
        print('  %-8s %s %s' % (e['art'], e['titel'], e.get('muster') or ''))
    print('Gesamt:', count())
