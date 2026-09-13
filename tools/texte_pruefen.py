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
Findet sichtbare Texte, die nicht durch `t()` laufen.

Warum es dieses Werkzeug gibt: `tools/sprachen_pruefen.py` schaut auf README,
CHANGELOG und ROADMAP — also auf die Dokumente. Die Oberfläche selbst hat es
nie geprüft. Deshalb konnte es passieren, dass die englischen Seiten an einem
guten Dutzend Stellen deutschen Text zeigten, ohne dass etwas Alarm schlug.

Gesucht wird im Quelltext (nicht im laufenden Fenster): Jeder Aufruf, der einen
sichtbaren Text bekommt — `text=`, `title=` und die Hausbausteine `round_button`,
`badge`, `_feld` und Verwandte — und dort eine feste Zeichenkette stehen hat
statt eines `t('schluessel')`.

Rauschen wird ausgesiebt: reine Platzhalter (`%d / %d`), einzelne Zeichen
(`▶`, `·`), Eigennamen und technische Schlüsselwörter. Was übrig bleibt, ist
ein Satz, den ein Mensch liest — und der damit in `scbp/sprache.py` gehört.

Benutzung:

    python3 tools/texte_pruefen.py            # alle Oberflächen-Dateien
    python3 tools/texte_pruefen.py scbp/seiten.py

Rückgabe 0, wenn nichts gefunden wurde — damit taugt es für den Selbsttest.
"""
import ast
import io
import os
import re
import sys

HIER = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Benannte Argumente, deren Wert auf dem Bildschirm landet.
SICHTBAR = ('text', 'title', 'label', 'placeholder', 'platzhalter', 'lead')

# Bausteine aus anderen Dateien — die stehen hier, weil ihre Signatur nicht
# in derselben Datei zu finden ist. Die Zahl ist das wievielte Argument, das
# der Mensch liest, gezählt ab null.
FREMDE_BAUSTEINE = {
    'round_button': (1,),            # (eltern, text, tat, …)
    'badge':     (1,),
    '_chip':     (1,),
}

# Parameternamen, hinter denen ein sichtbarer Text steckt. Bausteine der
# geprüften Datei werden darüber **selbst gefunden** — sonst müsste man die
# Tabelle bei jedem neuen Baustein von Hand nachziehen, und genau das geht
# schief: `_wertzeile` fehlte, und deshalb stand „Baupläne bekannt" monatelang
# unübersetzt auf der englischen Über-Seite, während die Prüfung grün meldete.
TEXTNAMEN = ('text', 'titel', 'bez', 'bezeichnung', 'hilfe', 'lead', 'fett',
             'rest', 'beschriftung', 'wofuer', 'platzhalter')

# Methoden, die eine Meldung in die Statuszeile schreiben.
MELDER = ('say',)

# `_wahl(fenster, eltern, eintraege, …)` bekommt Paare (Schlüssel, Anzeige).
# Nur die zweite Hälfte steht auf dem Bildschirm.
WAHL = '_wahl'

# Wörter, die zwar wie Text aussehen, aber keiner sind: technische Schlüssel,
# Eigennamen, Dateiendungen.
KEINE_TEXTE = (
    'win', 'darwin', 'linux', 'version', 'datum', 'offen', 'unbekannt',
    'Xharig', 'SC BP Watcher', 'normal', 'bold', 'center', 'left', 'right',
    # Argumente von `decode`/`open` — sie stehen an einer Stelle, die sonst
    # Text trägt, sind aber keiner.
    'utf-8', 'utf-8-sig', 'latin-1', 'ignore', 'strict', 'replace', 'surrogateescape',
    # Der Sprachumschalter selbst: Jede Sprache steht dort in ihrer eigenen
    # Schreibweise, sonst findet sich niemand wieder. Wer Englisch spricht und
    # versehentlich auf Deutsch gelandet ist, sucht „English" — nicht
    # „Englisch". Diese beiden gehören deshalb NICHT durch t().
    'Deutsch', 'English',
)

# Bausteine, die in jeder Sprache gleich bleiben: der Name des Werkzeugs, das
# Lizenzkürzel, Adressen. Bleibt nach ihrem Abzug nichts übrig, ist die Zeile
# kein zu übersetzender Satz.
UNVERAENDERLICH = re.compile(
    r'(https?://\S+|(?:www\.|github\.com/)\S+|SC BP Watcher|GPL-[\d.]+-only'
    r'|Xharig(?:-1)?|scmdb\.net|KRT Profit Basetool)', re.I)


def _ist_t(knoten):
    """Steht hier ein t('…')-Aufruf?"""
    return (isinstance(knoten, ast.Call)
            and isinstance(knoten.func, ast.Name)
            and knoten.func.id == 't')


def _literale(knoten):
    """Alle festen Zeichenketten eines Ausdrucks — ohne die in t()-Aufrufen."""
    if _ist_t(knoten):
        return []
    if isinstance(knoten, ast.Constant) and isinstance(knoten.value, str):
        return [(knoten.lineno, knoten.value)]
    # ⚠ `eintrag.get('material')` liest ebenfalls ein Wörterbuch aus — der
    # Schlüssel ist Technik, angezeigt wird der Wert. Ohne diese Ausnahme
    # meldete die Prüfung am 29.08.2026 zwei Fehlalarme auf der Lager-Seite
    # (`text=p.get('material')`). Eng gefasst: nur der **erste** Parameter, und
    # nur bei den drei Wörterbuch-Methoden.
    schluessel_holer = ('get', 'setdefault', 'pop')
    if (isinstance(knoten, ast.Call)
            and isinstance(knoten.func, ast.Attribute)
            and knoten.func.attr in schluessel_holer
            and knoten.args):
        treffer = _literale(knoten.func)
        for weiteres in knoten.args[1:]:
            treffer += _literale(weiteres)
        for schluesselwort in knoten.keywords:
            treffer += _literale(schluesselwort.value)
        return treffer

    treffer = []
    for kind in ast.iter_child_nodes(knoten):
        # `bericht['grund']` liest ein Wörterbuch aus — der Schlüssel dazwischen
        # steht nie auf dem Bildschirm, nur der Wert. Also nicht mitzählen.
        if isinstance(knoten, ast.Subscript) and kind is knoten.slice:
            continue
        treffer += _literale(kind)
    return treffer


def _ist_satz(text):
    """Liest ein Mensch das, oder ist es Technik?"""
    ohne = re.sub(r'%[sd%.0-9]*', '', text)
    ohne = UNVERAENDERLICH.sub('', ohne).strip(' ·—…\n\t')
    if len(ohne) < 3:
        return False
    # Mindestens drei Buchstaben am Stück — sonst ist es ein Symbol.
    if not re.search(r'[A-Za-zÄÖÜäöüß]{3}', ohne):
        return False
    if ohne in KEINE_TEXTE:
        return False
    return True


def _bausteine_finden(baum):
    """Welche Funktionen dieser Datei bekommen sichtbaren Text — und wo?

    Gelesen wird die Signatur: Heißt ein Parameter `titel`, `hilfe`, `text`
    (siehe TEXTNAMEN), steht an seiner Stelle etwas, das ein Mensch liest.
    """
    gefunden = dict(FREMDE_BAUSTEINE)
    for knoten in ast.walk(baum):
        if not isinstance(knoten, ast.FunctionDef):
            continue
        stellen = tuple(nummer for nummer, arg in enumerate(knoten.args.args)
                        if arg.arg in TEXTNAMEN)
        if stellen:
            gefunden[knoten.name] = stellen
    return gefunden


def pruefe(pfad):
    """Liefert die Fundstellen einer Datei als (Zeile, Stelle, Text)."""
    quelle = io.open(pfad, encoding='utf-8').read()
    baum = ast.parse(quelle)
    BAUSTEINE = _bausteine_finden(baum)
    funde = set()

    for knoten in ast.walk(baum):
        if not isinstance(knoten, ast.Call):
            continue

        for arg in knoten.keywords:
            if arg.arg in SICHTBAR:
                for zeile, text in _literale(arg.value):
                    if _ist_satz(text):
                        funde.add((zeile, arg.arg, text))

        name = getattr(knoten.func, 'id', None)
        methode = getattr(knoten.func, 'attr', None)

        if name in BAUSTEINE:
            for stelle in BAUSTEINE[name]:
                if stelle < len(knoten.args):
                    for zeile, text in _literale(knoten.args[stelle]):
                        if _ist_satz(text):
                            funde.add((zeile, name, text))

        if methode in MELDER and knoten.args:
            for zeile, text in _literale(knoten.args[0]):
                if _ist_satz(text):
                    funde.add((zeile, methode + '()', text))

        if name == WAHL and len(knoten.args) > 2:
            eintraege = knoten.args[2]
            if isinstance(eintraege, (ast.List, ast.Tuple)):
                for paar in eintraege.elts:
                    if isinstance(paar, (ast.List, ast.Tuple)) and len(paar.elts) > 1:
                        for zeile, text in _literale(paar.elts[1]):
                            if _ist_satz(text):
                                funde.add((zeile, WAHL, text))

    # ⚠ **Bekannte Lücke: Text in Datenstrukturen sieht dieser Prüfer nicht.**
    # Gesucht wird in Aufrufen (`ast.Call`) — an Schlüsselwörtern, Bausteinen,
    # Meldern. Steht eine Beschriftung dagegen als Tupelpaar in einer Schleife
    # oder in einem Wörterbuch auf Modulebene, ist sie unsichtbar.
    #
    # Genau so sind am 27.08.2026 vier Wörter durchgerutscht — „Alles / Neu /
    # Verbessert / Behoben" auf dem Reiter „Was ist neu", monatelang auch in der
    # englischen Oberfläche deutsch. Aufgefallen ist es erst auf einem
    # Bildschirmfoto.
    #
    # Ein Versuch, das Muster „Kennung + Beschriftung" mitzuprüfen, wurde am
    # selben Tag wieder verworfen: Er meldete 55 Stellen, fast alle falsch —
    # überwiegend die zweisprachigen Spieltext-Tabellen, die längst richtig
    # gebaut sind. Eine Prüfung, die so oft danebenliegt, wird übergangen, und
    # dann fällt auch das Echte nicht mehr auf.
    #
    # Geschlossen wird die Lücke von der anderen Seite: `oberflaeche_pruefen.py`
    # baut das Fenster auf **Englisch** auf und sieht nach, ob ein sichtbarer
    # Text wörtlich in der deutschen Spalte von `sprache.py` steht. Das braucht
    # keine Heuristik und fand alle vier Stellen sofort.
    return sorted(funde)


def main(argumente):
    if argumente:
        dateien = argumente
    else:
        ordner = os.path.join(HIER, 'scbp')
        dateien = [os.path.join(ordner, n) for n in sorted(os.listdir(ordner))
                   if n.endswith('.py')]

    gesamt = 0
    for pfad in dateien:
        funde = pruefe(pfad)
        if not funde:
            continue
        kurz = os.path.relpath(pfad, HIER)
        print('\n%s — %d Stellen:' % (kurz, len(funde)))
        for zeile, wo, text in funde:
            einzeilig = text.replace('\n', ' ')
            if len(einzeilig) > 62:
                einzeilig = einzeilig[:62] + '…'
            print('  %4d  %-10s %s' % (zeile, wo, einzeilig))
        gesamt += len(funde)

    print('')
    if gesamt:
        print('%d feste Texte. Sie gehören nach scbp/sprache.py und dann als' % gesamt)
        print("t('schluessel') an ihre Stelle — sonst zeigt die englische")
        print('Oberfläche deutschen Text.')
        return 1

    print('Alle sichtbaren Texte laufen durch t(). Zweisprachig ist vollständig.')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
