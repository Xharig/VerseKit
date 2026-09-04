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
Rohstoffpreise — „kaufen oder abbauen?"

Die Herstellung sagt seit v3.3.0, **was fehlt**. Was sie nicht sagt: ob man das
Zeug überhaupt kaufen kann. Genau das entscheidet aber, was man als Nächstes
tut — losfliegen und schürfen, oder am nächsten Terminal einkaufen.

## Der eine Befund, der das Modul rechtfertigt

Gemessen am 30.08.2026 über alle 26 Rohstoffe, die in Rezepten vorkommen:

| | |
|---|---|
| kaufbar | 18 |
| **nur abbaubar** | **8** — Aslarite, Borase, Lindinium, Ouratite, Quantainium, Riccite, Savrilium, Torite |

Und **fünf davon stehen gleichzeitig auf der Zerlege-Sperrliste** (Lindinium,
Ouratite, Quantainium, Riccite, Savrilium): weder kaufbar noch aus einem
zerlegten Stück zurückzuholen. Das sind die echten Engpässe beim Herstellen,
und bisher stand das nirgends.

## Woher

[UEX Corp](https://uexcorp.space) API 2.0, Endpunkt `commodities` — 205 Waren,
rund 134 KB. Kein Schlüssel nötig, ein einfacher GET.

⚠ **Die Daten werden NICHT mitgeliefert**, sondern auf dem Rechner des Nutzers
geholt — dieselbe Regel wie bei scmdb. Und **höchstens einmal am Tag**: Preise
ändern sich im Spiel laufend, aber nicht im Minutentakt, und ein Werkzeug, das
bei jedem Seitenaufruf eine fremde Schnittstelle anfasst, ist ein schlechter
Gast.

⚠ **Ohne Netz passiert nichts Schlimmes.** Liegt eine alte Ablage da, wird sie
benutzt; liegt keine da, bleibt die Preisangabe einfach weg. Kein Fehler, kein
Absturz — die Herstellung funktioniert ohne Preise genauso wie vorher.

## Was hier bewusst NICHT steht

Keine Handelsrouten, keine Frachtplanung. Dieses Modul beantwortet **eine**
Frage: „kaufen oder abbauen?"

⚠ **Die Preise je Terminal stehen seit v3.4.0 in `verkauf.py`** — bis dahin
waren sie hier ausdrücklich ausgeschlossen („weitere 2,1 MB Daten und ein
anderes Werkzeug"). Der Satz stimmte nicht mehr: Gemessen am 30.08.2026 ist
der volle Abzug 1,04 MB und aufgeräumt abgelegt 293 KB, und die Frage „wo werde
ich das los" gehört zum Handelslager, das der Watcher ohnehin führt.

Getrennt bleiben die beiden trotzdem, und zwar an der Bedeutung von
`price_buy` und `price_sell` (siehe `KAUF_QUALITAET` weiter unten): Hier zählt,
was das Terminal **verlangt**, dort, was es **zahlt**.
"""
from . import uex
from .katalog import AUS
from .herstellung import norm_rohstoff

QUELLE = 'https://api.uexcorp.uk/2.0/commodities'
CACHE = 'preise.json'
FORMAT = 1
ZEITLIMIT = 20

# Wie lange eine Ablage als frisch gilt. Ein Tag — siehe Kopf.
HALTBAR = uex.TAG

# Abruf und Ablage liegen im gemeinsamen Unterbau — siehe `scbp/uex.py`.
_ablage = uex.Ablage(CACHE, format_nr=FORMAT, haltbar=HALTBAR)

# ⭐⭐ **Am Terminal gekaufte Ware hat immer Qualität 500.**
#
# Das ist der Punkt, der die Preisangabe erst ehrlich macht. Ohne ihn liest
# sich „kaufen: 22.730 aUEC" wie ein gleichwertiger Weg, der nur Geld statt
# Zeit kostet. Ist er nicht: Q 500 ist der **Nullpunkt** der Qualitätswirkung —
# der Faktor ist dort exakt 1,000, auf jede Eigenschaft. Wer kauft, baut
# garantiert einen Standard-Gegenstand. Besser wird er ausschliesslich mit
# selbst abgebautem Erz über 500.
#
# Gemessen über alle Rezepte des Spielstands 4.10.0:
#
# | Nullpunkt | Wirkungen |
# |---|---|
# | **Q 500** | **5.025** |
# | Q 499 (Rundung) | 12 |
# | echte Ausreisser (571, 600, 625, 750) | 29 |
#
# ⚠ Die beiden Preise bedeuten Verschiedenes, und nur einer taugt hier:
#
# | Feld | heisst | Qualität |
# |---|---|---|
# | `price_buy` | was das **Terminal verlangt** | immer 500 |
# | `price_sell` | was das Terminal dir **zahlt** | jede — auch selbst abgebautes |
#
# Für „kaufen oder abbauen?" zählt deshalb `price_buy`. Der Verkaufspreis wird
# nur mitgeführt, weil er zur selben Ware gehört.
#
# Der Wert 500 steht nicht in den Handelsdaten — er ergibt sich aus der
# Bauweise der Rezepte (siehe Tabelle oben) und wurde von einem Spieler
# bestätigt.
KAUF_QUALITAET = 500

def laden():
    """Der abgelegte Stand — aus dem Speicher, wenn die Datei unverändert ist."""
    return _ablage.laden()


def alter():
    """Wie alt die Ablage ist, in Sekunden — oder None, wenn keine da ist."""
    return _ablage.alter()


def aktualisieren(fortschritt=None):
    """Die Preise holen, wenn die Ablage fehlt oder älter als ein Tag ist.

    Gibt `(Erfolg, Meldung)` zurück. **Sparsam**: Ist die Ablage frisch, wird
    gar nichts abgerufen.
    """
    # ⚠ Die Abfrage steht hier, obwohl `uex.holen()` sie auch kennt: Sonst
    # käme bei abgeschaltetem Netz und frischer Ablage ein `True` zurück, wo
    # vorher ein `False` stand. Ein Umbau soll die Struktur ändern, nicht das
    # Verhalten — auch wenn den Wert hier gerade niemand auswertet.
    if AUS:
        return False, ''
    if not _ablage.veraltet():
        return True, ''
    if fortschritt:
        fortschritt('')
    # ⚠ Kein lautes Scheitern. Ohne Preise laeuft alles weiter wie vorher.
    liste = uex.holen(QUELLE, 'preise', zeitlimit=ZEITLIMIT)
    if not liste:
        return False, ''
    # Nur die drei Felder behalten, die gebraucht werden — aus 134 KB werden so
    # rund 10 KB, und es liegt nichts herum, das niemand benutzt.
    #
    # ⚠⚠ **Jedes Material steht bei UEX ZWEIMAL**: veredelt (`Iron`, kaufbar
    # für 2.643) und als Erz (`Iron (Ore)`, nur verkaufbar). Unsere
    # Namensangleichung macht aus beiden denselben Schlüssel — wer dabei
    # einfach überschreibt, bekommt zufällig die eine oder die andere Form und
    # damit falsche Preise. Beim ersten Versuch stand deshalb bei Iron
    # „Kaufpreis 0" da, obwohl es für 2.643 im Regal liegt.
    #
    # Also **beide Formen behalten** und erst beim Abfragen entscheiden.
    schlank = {}
    for x in liste:
        name = (x.get('name') or '').strip()
        if not name:
            continue
        schlank.setdefault(norm_rohstoff(name), []).append({
            'name': name,
            'kauf': float(x.get('price_buy') or 0),
            'verkauf': float(x.get('price_sell') or 0),
        })
    _ablage.sichern({'waren': schlank})
    return True, ''


def preis(rohstoff):
    """Was dieser Rohstoff kostet und bringt.

    Gibt `(Kaufpreis, Verkaufspreis, Form)` in aUEC je SCU — oder `None`, wenn
    keine Preisdaten vorliegen. `Form` ist der Name, unter dem UEX ihn führt
    (`Iron` oder `Iron (Ore)`).

    ⚠ **Ein Kaufpreis von 0 heisst „nicht kaufbar"**, nicht „kostenlos". Wer
    diese Rohstoffe braucht, muss abbauen. Die Anzeige muss den Unterschied
    machen, sonst steht dort „0 aUEC" und jemand sucht nach dem Schnäppchen.

    ⚠ Von den beiden Formen (veredelt / Erz) wird für den Kaufpreis die
    **günstigste tatsächlich kaufbare** genommen — meist die veredelte, bei
    Borase aber das Erz. Gibt es gar keine kaufbare, kommt der beste
    Verkaufspreis zurück und `kauf = 0`.
    """
    waren = (laden() or {}).get('waren') or {}
    if not waren:
        return None
    formen = waren.get(norm_rohstoff(rohstoff))
    if not formen:
        return None
    kaufbar = [f for f in formen if f.get('kauf')]
    if kaufbar:
        beste = min(kaufbar, key=lambda f: f['kauf'])
        return beste['kauf'], beste.get('verkauf') or 0.0, beste['name']
    beste = max(formen, key=lambda f: f.get('verkauf') or 0)
    return 0.0, beste.get('verkauf') or 0.0, beste['name']
