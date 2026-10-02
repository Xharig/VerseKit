#!/usr/bin/env python3
# -*- coding: utf-8 -*-
#
# SC BP Watcher — zeigt live neue Star-Citizen-Baupläne an.
# Copyright (C) 2026 Xharig
#
# SPDX-License-Identifier: GPL-3.0-only
"""
Findet Kommentare und Docstrings, die Vorgeschichte statt Funktion
beschreiben: Datumsangaben, „gemeldet", Namen von Meldern, Zitate, frühere
Fassungen.

    python3 tools/kommentare_pruefen.py              # alle Fundstellen
    python3 tools/kommentare_pruefen.py scbp/x.py    # nur diese Datei
    python3 tools/kommentare_pruefen.py --zaehlen    # je Datei die Anzahl
    python3 tools/kommentare_pruefen.py --neu        # nur, was nicht im Bestand steht
    python3 tools/kommentare_pruefen.py --bestand-kuerzen

Der Bestand (`kommentare_bestand.txt`) hält die vorhandenen Fundstellen als
Prüfsummen von Datei und Zeilentext — ohne den Text selbst. Was dort steht,
gilt als bekannt; jede andere Fundstelle ist neu. `--bestand-kuerzen`
streicht Einträge, deren Fundstelle es nicht mehr gibt. Hinzugefügt wird nie.

Rückgabe 0 = nichts gefunden, 1 = Fundstellen.
"""
import collections
import hashlib
import io
import os
import re
import subprocess
import sys
import tokenize

WURZEL = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ENDUNGEN = ('.py', '.js', '.mjs', '.sh', '.yml')
BESTAND = os.path.join(WURZEL, 'tools', 'kommentare_bestand.txt')

# Bausteine zusammengesetzt, damit diese Datei sich nicht selbst findet.
MUSTER = re.compile('|'.join([
    r'\b\d{1,2}\.\d{1,2}\.20\d\d\b',
    r'\b(alte|frühere|vorige|bisherige) fass' + 'ung', 'vorher w' + 'ar',
    r'\b(bis|seit) (zum|dem) \d', r'\brob' + r'ert\b', 'wol' + r'lte\b',
    'auf zu' + 'ruf', r'\bwie gewün' + 'scht',
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
    try:
        liste = subprocess.run(['git', 'ls-files'], cwd=WURZEL,
                               capture_output=True, text=True,
                               encoding='utf-8').stdout.split('\n')
    except OSError:
        liste = []
    if not any(liste):
        liste = []
        for ordner, unter, namen in os.walk(WURZEL):
            unter[:] = [u for u in unter if not u.startswith(('.', '__'))]
            for name in namen:
                liste.append(os.path.relpath(os.path.join(ordner, name), WURZEL)
                             .replace(os.sep, '/'))
    return sorted(f for f in liste if f.endswith(ENDUNGEN)
                  and not f.startswith('daten/')
                  and os.path.basename(f) != 'kommentare_pruefen.py')


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


LIZENZKOPF = re.compile(r'Copyright \(C\) 20\d\d ')


def funde_text(datei, text):
    """[(Zeile, Text)] der Fundstellen in `text`, gelesen als `datei`.
    Die Zeile `Copyright (C) <Jahr> …` des Lizenzkopfs zählt nicht."""
    teile = kommentare_py(text) if datei.endswith('.py') else kommentare_js(text)
    return [(nr, z.strip()) for nr, z in teile
            if MUSTER.search(z) and not LIZENZKOPF.search(z)]


def funde(datei):
    pfad = os.path.join(WURZEL, datei)
    try:
        text = open(pfad, encoding='utf-8').read()
    except (OSError, UnicodeDecodeError):
        return []
    return funde_text(datei, text)


def kennung(datei, text):
    """Prüfsumme einer Fundstelle: Datei und Zeilentext, ohne Zeilennummer —
    sie bleibt gleich, wenn darüber Zeilen dazukommen."""
    roh = '%s\0%s' % (datei, ' '.join(text.split()))
    return hashlib.sha256(roh.encode('utf-8')).hexdigest()[:20]


def bestand_lesen(pfad=BESTAND):
    """{Prüfsumme: Anzahl} — leer, wenn es keinen Bestand gibt."""
    zaehler = collections.Counter()
    try:
        with open(pfad, encoding='utf-8') as handle:
            for zeile in handle:
                zeile = zeile.strip()
                if zeile and not zeile.startswith('#'):
                    zaehler[zeile] += 1
    except OSError:
        pass
    return zaehler


def neue_funde(alle, bestand):
    """Aus {Datei: [(Zeile, Text)]} das, was über den Bestand hinausgeht.

    Gezählt wird je Prüfsumme: Steht derselbe Text zweimal im Bestand und
    dreimal in der Datei, ist die dritte Stelle neu."""
    rest = collections.Counter(bestand)
    neu = []
    for datei in sorted(alle):
        for nr, text in alle[datei]:
            k = kennung(datei, text)
            if rest[k] > 0:
                rest[k] -= 1
            else:
                neu.append((datei, nr, text))
    return neu


def alle_funde(auswahl=None):
    return {d: funde(d) for d in dateien(auswahl)}


def bestand_kuerzen(pfad=BESTAND):
    """Streicht Einträge, deren Fundstelle es nicht mehr gibt. Fügt nie
    etwas hinzu. Rückgabe: Anzahl gestrichen."""
    alt = bestand_lesen(pfad)
    jetzt = collections.Counter(kennung(d, t) for d, f in alle_funde().items()
                                for _nr, t in f)
    neu = alt & jetzt
    gestrichen = sum(alt.values()) - sum(neu.values())
    if gestrichen:
        _bestand_schreiben(neu, pfad)
    return gestrichen


def _bestand_schreiben(zaehler, pfad=BESTAND):
    with open(pfad, 'w', encoding='utf-8', newline='\n') as handle:
        handle.write('# Bekannte Fundstellen von tools/kommentare_pruefen.py '
                     'als Prüfsummen.\n# Wird nur gekürzt '
                     '(--bestand-kuerzen), nie ergänzt.\n')
        for k in sorted(zaehler.elements()):
            handle.write(k + '\n')


def main(argv):
    if '--bestand-kuerzen' in argv:
        print('%d Einträge gestrichen' % bestand_kuerzen())
        return 0
    zaehlen = '--zaehlen' in argv
    auswahl = [a for a in argv if not a.startswith('--')]
    alle = alle_funde(auswahl)
    if '--neu' in argv:
        neu = neue_funde(alle, bestand_lesen())
        for datei, nr, z in neu:
            print('%s:%d: %s' % (datei, nr, z[:150]))
        print('\n%d neue Fundstellen' % len(neu))
        return 1 if neu else 0
    gesamt = 0
    for datei, f in alle.items():
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
