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
Das Handelslager — was zum Verkauf im Laderaum liegt.

Getrennt vom Werkstatt-Lager (`materials.py`), und zwar mit Absicht: Das dort
ist Baumaterial, das man **behält**. Was hier steht, will man **loswerden**.
Wer 500 SCU Gold für einen Handelsflug bunkert, will sie nicht in der
Herstellungs-Rechnung als Vorrat auftauchen sehen.

## Was hier anders ist als im Werkstatt-Lager

| | Werkstatt (`materials.py`) | Handel (hier) |
|---|---|---|
| Waren | die 26 aus Rezepten | alle rund 150 verkäuflichen |
| Güte | wichtig (Q 500 ist der Nullpunkt) | **gibt es nicht** |
| Menge | cSCU-genau, mit Raffinerie-Zeilen | ganze SCU |
| stattdessen | — | Kennzeichen **als gestohlen markiert** |

⚠ **Keine Qualität.** Zwei Gründe: Der Ankaufpreis am Terminal hängt nicht
daran (im ganzen UEX-Abzug steht `quality` auf 0), und erbeutete Ware hat
ohnehin immer Q 0. Ein Feld, das nie etwas ändert, ist nur ein Feld, das man
falsch ausfüllen kann.

⚠⚠ **Geschlossene Listen, kein Freitext** — dieselbe Regel wie beim Lagerort in
`places.py`, aus demselben Grund: Jemand tippt etwas Beleidigendes hinein, macht
ein Bildschirmfoto und verbreitet es. Am Ende fragt niemand, wer getippt hat;
es steht in diesem Werkzeug. Die Warennamen kommen deshalb aus `selling.py`,
die Orte aus `places.py`.

## Als gestohlen markiert

Erbeutete Ladung nimmt nicht jedes Terminal an. Ist der Haken gesetzt, blendet
der Verkaufs-Reiter auf die Stellen ein, die keine Fragen stellen (`is_nqa` bei
UEX) — 15 Terminals, davon 7 mit Ankaufgeboten.

⚠ **Hinweis, keine Behauptung** — dieselbe Haltung wie im Werkstatt-Lager: Wer
zwei Zugänge zu buchen vergisst, hat ein lückenhaftes Lager. Das Werkzeug sagt
deshalb nie „das hast du nicht", sondern rechnet nur mit dem, was eingetragen
ist.

⚠ Bis zum 11.09.2026 hieß dieses Modul `handelslager` (Sprachumstellung P4,
Stufe 1). **Nur Bezeichner sind umbenannt, keine Zeichenketten.** Bewusst
gleich geblieben, weil sie in der Datei jedes Nutzers stehen: der Dateiname
`handelslager.json` und die Schlüssel `format`, `posten`, `ware`, `menge`,
`ort` und `gestohlen`. Ebenso die Kennungen `'ware'`, `'menge'`,
`'schreiben'` und `'weg'`, über die die Oberfläche ihre Meldung wählt, und der
Seitenname `handelslager` in Reiterleiste und Symbolsatz. `calculate` und
`parse_number` kommen aus `materials.py`.
"""
import json
import os

from . import fehler, pfade

FILE = 'handelslager.json'
FORMAT = 1

# ⭐ Zahleingabe **und Rechner** sind dieselben wie im Werkstatt-Lager: Komma
# und Punkt gelten gleich, das lange Minus vom Ziffernblock wird angenommen,
# und `100+5` ergibt 105. Wiederverwendet statt nachgebaut — zwei Fassungen
# derselben Regel gehen irgendwann auseinander, und das Bedienkonzept darf
# sich zwischen zwei Lagern nicht unterscheiden.
from .materials import calculate, parse_number               # noqa: E402


def _check_amount(amount, previous=0.0):
    """Was im Mengenfeld steht, als geprüfte Zahl — oder `None`.

    ⚠ **Kein negatives Ergebnis und keine Null.** Der Rechner lässt Minus
    ausdrücklich zu (`100-40` ergibt 60, und im Werkstatt-Lager wird damit
    abgebucht) — aber ein Laderaum mit „−40 SCU" ergibt keinen Sinn. Geprüft
    wird deshalb das **Ergebnis**, nicht die Eingabe.
    """
    number = calculate(amount, previous) if isinstance(amount, str) else amount
    if number is None or number <= 0:
        return None
    return float(number)


def load():
    """Alle Posten — oder eine leere Liste."""
    try:
        with open(pfade.app_datei(FILE), encoding='utf-8') as f:
            data = json.load(f)
        if data.get('format') == FORMAT:
            return data.get('posten') or []
    except Exception:
        pass
    return []


def save(entries):
    """Die Posten schreiben. Meldet einen Fehlschlag, statt ihn zu schlucken.

    ⚠ Die **Vorgängerfassung** (`handelslager.bak.json`) legt
    `pfade.json_sichern` an. Bis 31.08.2026 fehlte sie hier: Geschrieben wurde
    atomar, aber ohne Rückfall — ein leer gespeichertes Lager war endgültig
    weg. Ein Lager sind eigene Eingaben, die kein Neuaufbau zurückholt.
    """
    target = pfade.app_datei(FILE)
    try:
        return pfade.json_sichern(target, {'format': FORMAT, 'posten': entries})
    except Exception as exc:
        fehler.merken('trade_cargo.save', exc)
        return False


def as_csv(entries=None):
    """Das Handelslager als Tabelle — Ware, Menge, Kennzeichen, Lagerort.

    Warum CSV und nicht nur JSON: Eine Tabelle oeffnet sich in jedem
    Tabellenprogramm und laesst sich weiterreichen — etwa an den, der den
    Handelsflug fliegt. Das eigene JSON ist zum Zuruecklesen da, die Tabelle
    zum Ansehen und Teilen.

    Semikolon als Trenner und Komma als Dezimalzeichen — so erwartet es ein
    deutsches Excel/LibreOffice. Mit Punkt und Komma-Trenner landet "1,36"
    dort als Datum oder in einer Spalte zu viel. Dieselbe Wahl wie im
    Werkstatt-Lager (`materials.as_csv`); zwei Lager, die sich beim Ausgeben
    unterschiedlich verhalten, waeren nur eine Falle.

    Die Spalte "Gestohlen" steht als `ja`/leer da statt als `True`/`False`:
    In einer Tabelle liest das jeder, und es uebersetzt sich nicht falsch.
    """
    entries = load() if entries is None else entries
    lines = ['Ware;Menge;Gestohlen;Lagerort']
    for p in entries:
        amount = ('%g' % float(p.get('menge') or 0)).replace('.', ',')
        lines.append(';'.join((
            (p.get('ware') or '').replace(';', ','),
            amount,
            'ja' if p.get('gestohlen') else '',
            (p.get('ort') or '').replace(';', ','))))
    return '\n'.join(lines) + '\n'


def as_json(entries=None):
    """Das Handelslager als JSON-Text — dasselbe Format, das `load()` liest.

    Damit ist die Ausgabe zugleich eine Sicherung: Datei wegschreiben, spaeter
    zuruckspielen, fertig.
    """
    entries = load() if entries is None else entries
    return json.dumps({'format': FORMAT, 'posten': entries},
                      ensure_ascii=False, indent=1)


def from_json(text):
    """Ein frueher ausgegebenes Handelslager wieder einlesen.

    Gibt die Postenliste zurueck oder `None`, wenn die Datei nicht passt.

    ⚠ Es wird **nichts** gespeichert — das entscheidet die Oberflaeche. Genau
    wie bei `materials.from_json`.

    ⚠⚠ **Die Datei des anderen Lagers wird abgelehnt.** Beide sind
    `{"format": 1, "posten": [...]}` — an der Huelle sind sie nicht zu
    unterscheiden. Erkannt wird der Unterschied an den Posten selbst: Hier
    heisst das Feld `ware`, im Werkstatt-Lager `material`. Ohne diese Probe
    haette eine Rohstoff-Sicherung hier ein leeres Lager erzeugt (jeder Posten
    ohne `ware` faellt raus) — und der Nutzer haette sein Handelslager gegen
    Nichts getauscht, mit der Meldung "0 Posten eingelesen".
    """
    try:
        data = json.loads(text)
    except Exception:
        return None
    if not isinstance(data, dict) or data.get('format') != FORMAT:
        return None
    entries = data.get('posten')
    if not isinstance(entries, list):
        return None
    # Eine nicht leere Liste, in der kein einziger Posten eine `ware` hat, ist
    # keine Handelslager-Sicherung, sondern die des anderen Lagers.
    if entries and not any(isinstance(p, dict) and str(p.get('ware') or '').strip()
                           for p in entries):
        return None
    clean = []
    for p in entries:
        if not isinstance(p, dict) or not str(p.get('ware') or '').strip():
            continue
        try:
            amount = float(p.get('menge') or 0)
        except (TypeError, ValueError):
            continue
        if amount <= 0:
            continue
        clean.append({'ware': str(p.get('ware')).strip(),
                      'menge': amount,
                      'ort': str(p.get('ort') or '').strip(),
                      'gestohlen': bool(p.get('gestohlen'))})
    return clean


def same_stack(a_goods, a_place, a_stolen, b):
    """Sind das zwei Eintragungen für **denselben** Stapel?

    Gleich heisst: gleiche Ware, gleicher Lagerort, gleiches Kennzeichen. Nur
    dann darf zusammengezählt werden.

    ⚠ **Das Kennzeichen gehört dazu.** Saubere und als gestohlen markierte Ware
    derselben Sorte sind zwei Stapel — sie lassen sich nicht an denselben
    Stellen verkaufen. Wer sie zusammenzählt, schickt jemanden mit heißer Ware
    an ein Terminal, das Fragen stellt.
    """
    return (b.get('ware') == a_goods
            and (b.get('ort') or '') == (a_place or '')
            and bool(b.get('gestohlen')) == bool(a_stolen))


def add(goods, amount, place='', stolen=False):
    """Einen Zugang buchen. Gleiche Stapel werden zusammengezählt.

    Gibt `(Erfolg, Grund)` zurück; `Grund` ist eine Kennung für die Oberfläche
    (`'ware'`, `'menge'`, `'schreiben'`) oder `''`.
    """
    goods = (goods or '').strip()
    if not goods:
        return False, 'ware'
    number = _check_amount(amount)
    if number is None:
        return False, 'menge'
    entries = load()
    for p in entries:
        if same_stack(goods, place, stolen, p):
            p['menge'] = float(p.get('menge') or 0) + float(number)
            return (True, '') if save(entries) else (False, 'schreiben')
    entries.append({'ware': goods, 'menge': float(number),
                    'ort': place or '', 'gestohlen': bool(stolen)})
    return (True, '') if save(entries) else (False, 'schreiben')


def change(index, goods, amount, place='', stolen=False):
    """Einen Posten überschreiben. `index` ist die Stelle in `load()`."""
    entries = load()
    if not 0 <= index < len(entries):
        return False, 'weg'
    goods = (goods or '').strip()
    if not goods:
        return False, 'ware'
    # Beim Ändern zählt die bisherige Menge als Ausgangswert — wer `+5`
    # tippt, bucht dazu, statt die Menge auf 5 zu setzen.
    number = _check_amount(amount, float(entries[index].get('menge') or 0))
    if number is None:
        return False, 'menge'
    entries[index] = {'ware': goods, 'menge': float(number),
                      'ort': place or '', 'gestohlen': bool(stolen)}
    return (True, '') if save(entries) else (False, 'schreiben')


def remove(index):
    """Einen Posten löschen."""
    entries = load()
    if not 0 <= index < len(entries):
        return False
    entries.pop(index)
    return save(entries)


def clear():
    """Alles löschen — nach dem Verkauf der ganzen Ladung."""
    return save([])


def amounts(only_stolen=None):
    """Wie viel von welcher Ware im Lager liegt: `{Ware: SCU}`.

    `only_stolen=True` zählt nur die markierte Ware, `False` nur die saubere,
    `None` alles.
    """
    total = {}
    for p in load():
        if only_stolen is not None and bool(p.get('gestohlen')) != only_stolen:
            continue
        goods = p.get('ware')
        if not goods:
            continue
        total[goods] = total.get(goods, 0.0) + float(p.get('menge') or 0)
    return total


def goods_in_stock(only_stolen=None):
    """Die Warennamen im Lager, alphabetisch — die Vorauswahl für den Verkauf."""
    return sorted(amounts(only_stolen))


def has_stolen():
    """Liegt markierte Ware im Lager? Schaltet den Filter im Reiter vor."""
    return any(p.get('gestohlen') for p in load())
