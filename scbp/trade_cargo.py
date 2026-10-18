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
deshalb nie, dass etwas fehlt, sondern rechnet nur mit dem, was eingetragen
ist.

⚠ Die gespeicherten Namen sind deutsch, obwohl das Modul englisch heißt —
sie stehen in der Datei jedes Nutzers und dürfen sich nicht ändern: der Dateiname
`handelslager.json` und die Schlüssel `format`, `posten`, `ware`, `menge`,
`ort` und `gestohlen`. Ein Posten kann zusätzlich `"stueck": true` tragen:
ein Item aus dem Lager-Abgleich mit dem Basetool, gezählt in ganzen Stück
statt in SCU — und `"einkauf": <Zahl>`: was der Spieler je SCU (bzw. je Stück)
bezahlt hat, Grundlage für den Gewinn. Beide Felder stehen nur da, wenn sie
gesetzt sind. Ebenso die Kennungen `'ware'`, `'menge'`, `'preis'`,
`'schreiben'` und `'weg'`, über die die Oberfläche ihre Meldung wählt, und der
Seitenname `handelslager` in Reiterleiste und Symbolsatz. `calculate` und
`parse_number` kommen aus `materials.py`.
"""
import json

from . import errors, paths

FILE = 'handelslager.json'
FORMAT = 1
# Optionales Feld eines Postens: bezahlter Preis je SCU bzw. je Stück.
COST = 'einkauf'
# Für `change`: den bisherigen Einkaufspreis behalten.
KEEP = object()

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


def _check_cost(cost):
    """Ein Einkaufspreis als `(gültig, Zahl oder None)`.

    Ein leeres Feld ist gültig und bedeutet: Preis unbekannt. Eine Zahl muss
    größer als null sein; der Rechner gilt wie im Mengenfeld (`3400+50`)."""
    if cost is None or (isinstance(cost, str) and not cost.strip()):
        return True, None
    number = calculate(cost, 0) if isinstance(cost, str) else cost
    try:
        number = float(number)
    except (TypeError, ValueError):
        return False, None
    if number <= 0:
        return False, None
    return True, number


def cost_of(entry):
    """Der Einkaufspreis eines Postens je SCU bzw. Stück — oder `None`."""
    try:
        value = float(entry.get(COST))
    except (TypeError, ValueError):
        return None
    return value if value > 0 else None


def _set_cost(entry, cost):
    if cost is None:
        entry.pop(COST, None)
    else:
        entry[COST] = float(cost)


def load():
    """Alle Posten — oder eine leere Liste. Eine kaputte Datei wird protokolliert
    und durch die Vorgängerfassung ersetzt (`paths.load_json`)."""
    data = paths.load_json(paths.app_file(FILE),
                           valid=lambda d: isinstance(d, dict),
                           report_as='trade_cargo.load')
    if data and data.get('format') == FORMAT:
        return data.get('posten') or []
    return []


def save(entries):
    """Die Posten schreiben. Meldet einen Fehlschlag, statt ihn zu schlucken.

    ⚠ Die **Vorgängerfassung** (`handelslager.bak.json`) legt
    `paths.save_json` an. Atomar ohne Rückfall wäre ein leer gespeichertes
    Lager endgültig weg — und ein Lager sind eigene Eingaben, die kein
    Neuaufbau zurückholt.
    """
    target = paths.app_file(FILE)
    try:
        saved = paths.save_json(target, {'format': FORMAT, 'posten': entries})
    except Exception as exc:
        errors.record('trade_cargo.save', exc)
        return False
    if saved:
        _notify_basetool()
    return saved


def _notify_basetool():
    try:
        from . import basetool_sync
        basetool_sync.store_saved()
    except Exception as exc:
        errors.record('trade_cargo.abgleich_melden', exc)


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
        if p.get('stueck'):
            clean[-1]['stueck'] = True
        if isinstance(p.get('uex'), int) and not isinstance(p.get('uex'),
                                                             bool):
            clean[-1]['uex'] = p['uex']
        _set_cost(clean[-1], cost_of(p))
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


def _mixed_cost(old_amount, old_cost, new_amount, new_cost):
    """Der Einkaufspreis eines zusammengelegten Stapels.

    Sind beide Preise bekannt, gilt der nach Menge gewichtete Mittelwert. Fehlt
    einer, ist der Preis des ganzen Stapels unbekannt — sonst gälte er auch für
    Ware, für die nie ein Preis eingetragen wurde, und der Gewinn wäre falsch."""
    if old_cost is None or new_cost is None:
        return None
    total = old_amount + new_amount
    if total <= 0:
        return new_cost
    return (old_amount * old_cost + new_amount * new_cost) / total


def add(goods, amount, place='', stolen=False, cost=None):
    """Einen Zugang buchen. Gleiche Stapel werden zusammengezählt.

    `cost` ist der bezahlte Preis je SCU (frei lassen, wenn unbekannt).
    Gibt `(Erfolg, Grund)` zurück; `Grund` ist eine Kennung für die Oberfläche
    (`'ware'`, `'menge'`, `'preis'`, `'schreiben'`) oder `''`.
    """
    goods = (goods or '').strip()
    if not goods:
        return False, 'ware'
    number = _check_amount(amount)
    if number is None:
        return False, 'menge'
    valid, cost = _check_cost(cost)
    if not valid:
        return False, 'preis'
    entries = load()
    for p in entries:
        if same_stack(goods, place, stolen, p):
            before = float(p.get('menge') or 0)
            _set_cost(p, _mixed_cost(before, cost_of(p), float(number), cost))
            p['menge'] = before + float(number)
            return (True, '') if save(entries) else (False, 'schreiben')
    entries.append({'ware': goods, 'menge': float(number),
                    'ort': place or '', 'gestohlen': bool(stolen)})
    _set_cost(entries[-1], cost)
    return (True, '') if save(entries) else (False, 'schreiben')


def add_item(name, amount, place='', stolen=False, uex=None, index=None):
    """Ein Item buchen (`stueck`), ganze Stück. `uex` ist die Nummer aus der
    UEX-Warenliste — über sie gleicht der Abgleich mit dem Basetool ab.
    Mit `index` wird dieser Posten überschrieben statt neu gebucht.
    Rückgabe wie `add`."""
    name = (name or '').strip()
    if not name:
        return False, 'ware'
    entries = load()
    if index is not None and not 0 <= index < len(entries):
        return False, 'weg'
    before = (float(entries[index].get('menge') or 0) if index is not None
              else 0.0)
    number = _check_amount(amount, before)
    if number is None or number < 1:
        return False, 'menge'
    pieces = float(int(round(number)))
    if index is not None:
        entry = entries[index]
        entry.update(ware=name, menge=pieces, ort=place or '',
                     gestohlen=bool(stolen), stueck=True)
    else:
        entry = next((p for p in entries if is_pieces(p)
                      and same_stack(name, place, stolen, p)), None)
        if entry is not None:
            entry['menge'] = float(entry.get('menge') or 0) + pieces
        else:
            entry = {'ware': name, 'menge': pieces, 'ort': place or '',
                     'gestohlen': bool(stolen), 'stueck': True}
            entries.append(entry)
    if isinstance(uex, int) and not isinstance(uex, bool) and uex > 0:
        entry['uex'] = uex
    elif index is not None:
        entry.pop('uex', None)
    return (True, '') if save(entries) else (False, 'schreiben')


def change(index, goods, amount, place='', stolen=False, cost=KEEP):
    """Einen Posten überschreiben. `index` ist die Stelle in `load()`.

    `cost=KEEP` lässt den Einkaufspreis, wie er ist; leer löscht ihn."""
    entries = load()
    if not 0 <= index < len(entries):
        return False, 'weg'
    goods = (goods or '').strip()
    if not goods:
        return False, 'ware'
    # Beim Ändern zählt die bisherige Menge als Ausgangswert — wer `+5`
    # tippt, bucht dazu, statt die Menge auf 5 zu setzen.
    number = _check_amount(amount, float(entries[index].get('menge') or 0))
    pieces = bool(entries[index].get('stueck'))
    if number is not None and pieces:
        number = round(number) or None
    if number is None:
        return False, 'menge'
    if cost is KEEP:
        cost = cost_of(entries[index])
    else:
        valid, cost = _check_cost(cost)
        if not valid:
            return False, 'preis'
    entries[index] = {'ware': goods, 'menge': float(number),
                      'ort': place or '', 'gestohlen': bool(stolen)}
    if pieces:
        entries[index]['stueck'] = True
    _set_cost(entries[index], cost)
    return (True, '') if save(entries) else (False, 'schreiben')


def is_pieces(entry):
    """Zählt dieser Posten in ganzen Stück (ein Item) statt in SCU?"""
    return bool(entry.get('stueck'))


def remove(index):
    """Einen Posten löschen."""
    entries = load()
    if not 0 <= index < len(entries):
        return False
    entries.pop(index)
    return save(entries)


def clear():
    """Alles löschen — Ware und Items."""
    return save([])


def clear_goods():
    """Nur die Ware löschen, Items bleiben — nach dem Verkauf der Ladung
    oder einem Patch, der den Laderaum leert. Gibt die Zahl der entfernten
    Posten zurück, `None`, wenn das Speichern scheitert."""
    entries = load()
    kept = [p for p in entries if is_pieces(p)]
    return len(entries) - len(kept) if save(kept) else None


def clear_items():
    """Nur die Items löschen, die Ware bleibt. Rückgabe wie `clear_goods`."""
    entries = load()
    kept = [p for p in entries if not is_pieces(p)]
    return len(entries) - len(kept) if save(kept) else None


def restore(found):
    """Eine Sicherung einlesen: Ihre Ware ersetzt die Ware, die Items hier
    bleiben stehen. Items der Sicherung kommen dazu, wenn hier kein gleicher
    Stapel liegt — eine ältere Sicherung räumt so keine später eingetragenen
    Items weg. Gibt `(Ware, neue Items)` zurück, `None`, wenn das Speichern
    scheitert."""
    items = [p for p in load() if is_pieces(p)]
    goods = [p for p in found if not is_pieces(p)]
    added = [p for p in found if is_pieces(p) and not any(
        same_stack(p.get('ware'), p.get('ort'), p.get('gestohlen'), q)
        for q in items)]
    if not save(goods + items + added):
        return None
    return len(goods), len(added)


def goods_rows():
    """Die Posten, die Ware sind — ohne Items (`stueck`)."""
    return [p for p in load() if not is_pieces(p)]


def amounts(only_stolen=None):
    """Wie viel von welcher Ware im Lager liegt: `{Ware: SCU}`.

    `only_stolen=True` zählt nur die markierte Ware, `False` nur die saubere,
    `None` alles. Items zählen nicht als Ware.
    """
    total = {}
    for p in goods_rows():
        if only_stolen is not None and bool(p.get('gestohlen')) != only_stolen:
            continue
        goods = p.get('ware')
        if not goods:
            continue
        total[goods] = total.get(goods, 0.0) + float(p.get('menge') or 0)
    return total


def costs(only_stolen=None):
    """Was die Ware im Lager im Schnitt gekostet hat: `{Ware: Preis je SCU}`.

    Gewichtet nach Menge, nur über Posten mit Einkaufspreis. Waren ohne
    bekannten Preis fehlen im Ergebnis."""
    paid, weight = {}, {}
    for p in goods_rows():
        if only_stolen is not None and bool(p.get('gestohlen')) != only_stolen:
            continue
        goods, cost = p.get('ware'), cost_of(p)
        if not goods or cost is None:
            continue
        amount = float(p.get('menge') or 0)
        paid[goods] = paid.get(goods, 0.0) + amount * cost
        weight[goods] = weight.get(goods, 0.0) + amount
    return {g: paid[g] / weight[g] for g in paid if weight[g] > 0}


def costed_amounts(only_stolen=None):
    """Wie viel je Ware einen Einkaufspreis hat: `{Ware: SCU}`."""
    total = {}
    for p in goods_rows():
        if only_stolen is not None and bool(p.get('gestohlen')) != only_stolen:
            continue
        if p.get('ware') and cost_of(p) is not None:
            total[p['ware']] = total.get(p['ware'], 0.0) + float(p.get('menge') or 0)
    return total


def goods_in_stock(only_stolen=None):
    """Die Warennamen im Lager, alphabetisch — die Vorauswahl für den Verkauf."""
    return sorted(amounts(only_stolen))


def has_stolen():
    """Liegt markierte Ware im Lager? Schaltet den Filter im Reiter vor."""
    return any(p.get('gestohlen') for p in goods_rows())
