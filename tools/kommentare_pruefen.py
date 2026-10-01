#!/usr/bin/env python3
# -*- coding: utf-8 -*-
#
# SC BP Watcher — zeigt live neue Star-Citizen-Baupläne an.
# Copyright (C) 2026 Xharig
#
# SPDX-License-Identifier: GPL-3.0-only
"""
Findet Kommentare und Docstrings, die Vorgeschichte statt Funktion
beschreiben: Datumsangaben, „gemeldet", Namen von Meldern, Zitate.

    python3 tools/kommentare_pruefen.py              # alle Fundstellen
    python3 tools/kommentare_pruefen.py scbp/x.py    # nur diese Datei
    python3 tools/kommentare_pruefen.py --zaehlen    # je Datei die Anzahl

Rückgabe 0 = nichts gefunden, 1 = Fundstellen.
"""
import io
import os
import re
import subprocess
import sys
import tokenize

WURZEL = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ENDUNGEN = ('.py', '.js', '.mjs', '.sh')

# Bausteine zusammengesetzt, damit diese Datei sich nicht selbst findet.
MUSTER = re.compile('|'.join([
    r'\b\d{1,2}\.\d{1,2}\.20\d\d\b',
    'gemel' + 'det', 'gewün' + 'scht', 'wun' + r'sch\b', 'wunsch vom', 'ange' + 'regt',
    'vorschl' + 'ag', 'entsch' + 'ieden', 'entschei' + 'dung', 'ansa' + 'ge',
    'der au' + 'tor', 'des au' + 'tors', 'der entwick' + 'ler',
    r'„[^“\n]{12,}“', r'\*„[^“\n]{6,}', r'„[^“\n]{12,}"',
    'haldj' + 'as', 'bomb' + '20', 'morkh' + 'an', 'zwaer' + 'sch', 'choo' + 'pa',
    'bushw' + 'ick', 'grel' + 'uc', 'pars' + 'ul', 'krov' + 'ax', 'alyx' + 'one',
    'blackd' + '0g', 'rurud' + 'otorg', 'ry' + r'ze\b', 'horth' + 'y', 'brax' + 'ton',
    r'\bxhar' + r'ig\b(?!\.)',
]), re.I)


def dateien(auswahl=None):
    if auswahl:
        return auswahl
    liste = subprocess.run(['git', 'ls-files'], cwd=WURZEL, capture_output=True,
                           text=True, encoding='utf-8').stdout.split('\n')
    return [f for f in liste if f.endswith(ENDUNGEN) and not f.startswith('daten/')
            and os.path.basename(f) != 'kommentare_pruefen.py']


def kommentare_py(text):
    """(Zeile, Text) aller Kommentare und Docstrings einer Python-Datei."""
    aus = []
    try:
        toks = list(tokenize.generate_tokens(io.StringIO(text).readline))
    except (tokenize.TokenError, SyntaxError, IndentationError):
        return aus
    vorher = tokenize.NEWLINE
    for t in toks:
        if t.type == tokenize.COMMENT:
            aus.append((t.start[0], t.string))
        elif t.type == tokenize.STRING and vorher in (tokenize.NEWLINE, tokenize.NL,
                                                      tokenize.INDENT, tokenize.DEDENT):
            if t.string.lstrip('rRbBuU').startswith(('"""', "'''")):
                for i, zeile in enumerate(t.string.split('\n')):
                    aus.append((t.start[0] + i, zeile))
        if t.type not in (tokenize.COMMENT, tokenize.NL):
            vorher = t.type
    return aus


def kommentare_js(text):
    aus = []
    im_block = False
    for nr, zeile in enumerate(text.split('\n'), 1):
        st = zeile.strip()
        if im_block or st.startswith('/*'):
            aus.append((nr, zeile))
            im_block = '*/' not in st
        elif st.startswith('//') or ' // ' in zeile:
            aus.append((nr, zeile[zeile.index('//'):]))
        elif st.startswith('#') and not st.startswith('#!'):
            aus.append((nr, zeile))
    return aus


def funde(datei):
    pfad = os.path.join(WURZEL, datei)
    try:
        text = open(pfad, encoding='utf-8').read()
    except (OSError, UnicodeDecodeError):
        return []
    teile = kommentare_py(text) if datei.endswith('.py') else kommentare_js(text)
    return [(nr, z.strip()) for nr, z in teile if MUSTER.search(z)]


def main(argv):
    zaehlen = '--zaehlen' in argv
    auswahl = [a for a in argv if not a.startswith('--')]
    gesamt = 0
    for datei in dateien(auswahl):
        f = funde(datei)
        gesamt += len(f)
        if not f:
            continue
        if zaehlen:
            print('%5d  %s' % (len(f), datei))
        else:
            for nr, z in f:
                print('%s:%d: %s' % (datei, nr, z[:150]))
    print('\n%d Fundstellen' % gesamt)
    return 1 if gesamt else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
