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
Prüft, ob die deutschen und englischen Versionen zusammenpassen.

Zweisprachig ist entweder ganz oder gar nicht. Eine Projektseite auf Englisch
mit einer Änderungsliste auf Deutsch dahinter sieht nicht nach Zweisprachigkeit
aus, sondern nach halber Arbeit — und genau so ist es hier einmal passiert.

Geprüft wird:
  * Gibt es zu jeder Datei die Gegenfassung?
  * Haben beide dieselben Abschnitte (bei Doku) bzw. dieselben Versionen
    (beim Changelog)?
  * Steht in beiden oben der Sprachumschalter, und zeigt er richtig?
  * Verlinkt jede Version ihre eigenen Geschwister — keine Sprachsprünge?

Aufruf:  python3 tools/sprachen_pruefen.py
Läuft auch als Teil von `tools/selbsttest.py`.
"""
import os
import re
import sys

# ⛔ Vor der ersten Ausgabe: Die Windows-Konsole kann kein `↔` — siehe ausgabe.py.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ausgabe                                                 # noqa: E402
ausgabe.utf8()

WURZEL = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

PAARE = [
    ('README.en.md', 'README.md', 'abschnitte'),
    ('CHANGELOG.en.md', 'CHANGELOG.md', 'versionen'),
    ('ROADMAP.en.md', 'ROADMAP.md', 'abschnitte'),
    # ⚠ Seit 31.08.2026 auch die beiden Nebenseiten. Sie lagen bis dahin **nur**
    # auf Englisch — in einem Projekt, dessen Hauptsprache Deutsch ist, ist
    # ausgerechnet die Sicherheitsseite die falsche Stelle zum Sparen.
    ('SECURITY.en.md', 'SECURITY.md', 'abschnitte'),
    ('CODE_OF_CONDUCT.en.md', 'CODE_OF_CONDUCT.md', 'abschnitte'),
]


def _lies(name):
    try:
        with open(os.path.join(WURZEL, name), encoding='utf-8') as f:
            return f.read()
    except OSError:
        return None


def _abschnitte(text):
    return [m.group(1).strip() for m in re.finditer(r'^#{2,3} (.+)$', text, re.M)]


def _versionen(text):
    return [m.group(1) for m in re.finditer(r'^## (v[\d.]+[\w.-]*)', text, re.M)]


def pruefe(melden=print):
    """Gibt eine Liste der Beanstandungen zurück (leer = alles in Ordnung)."""
    fehler = []
    for en, de, art in PAARE:
        t_en, t_de = _lies(en), _lies(de)
        if t_en is None or t_de is None:
            fehler.append('%s oder %s fehlt' % (en, de))
            continue

        # Gleiche Gliederung?
        hol = _versionen if art == 'versionen' else _abschnitte
        a, b = hol(t_en), hol(t_de)
        if art == 'versionen':
            if set(a) != set(b):
                fehler.append('%s/%s: unterschiedliche Versionen (%s)'
                              % (en, de, set(a) ^ set(b)))
        elif len(a) != len(b):
            fehler.append('%s hat %d Abschnitte, %s hat %d'
                          % (en, len(a), de, len(b)))

        # Umschalter oben, und zwar auf die Gegenfassung
        if de not in t_en:
            fehler.append('%s verlinkt %s nicht (Umschalter fehlt)' % (en, de))
        if en not in t_de:
            fehler.append('%s verlinkt %s nicht (Umschalter fehlt)' % (de, en))

        melden('  %-16s %2d ↔ %2d  %s' % (en, len(a), len(b),
                                          'ok' if not fehler else ''))

    # Keine Sprachsprünge: Die deutsche Version darf nicht auf englische
    # Geschwister zeigen (außer im Umschalter) und umgekehrt.
    for en, de, _ in PAARE:
        t_de = _lies(de) or ''
        for anderes_en, anderes_de, _x in PAARE:
            if anderes_en == en:
                continue
            # Verweis auf die englische Version eines ANDEREN Dokuments
            muster = r'\]\(%s\)' % re.escape(anderes_en)
            if re.search(muster, t_de):
                fehler.append('%s verweist auf %s statt auf %s'
                              % (de, anderes_en, anderes_de))

    # ⚠ Bildschirmfotos sind auch Sprache. Die englische Anleitung zeigte lange
    # die **deutsche** Oberfläche — dem Prüfer war das entgangen, weil er nur
    # Abschnitte zählt. Wer die Bilder tauscht, tauscht damit einen Teil der
    # Übersetzung; genau darauf achtet dieser Block.
    t_en = _lies('README.en.md') or ''
    t_de = _lies('README.md') or ''
    bilder_en = set(re.findall(r'assets/(screenshot-[a-z0-9-]+\.png)', t_en))
    bilder_de = set(re.findall(r'assets/(screenshot-[a-z0-9-]+\.png)', t_de))

    # Bewusst geteilte Bilder kämen hier hinein. Zurzeit gibt es keine: Auch das
    # Overlay liegt in beiden Sprachen vor — die Funde mussten dafür simuliert
    # werden (`tools/drops_vorfuehren.py`), sonst bliebe die Leiste leer und das
    # Bild zeigte nichts von dem, worum es geht.
    GETEILT_OK = set()

    geteilt = sorted(b for b in bilder_en & bilder_de
                     if not b.endswith('-en.png') and b not in GETEILT_OK)
    if geteilt:
        # ⚠ Bewusst **keine** Beanstandung, sondern ein Hinweis: Fehlende
        # englische Bilder sind ein Schönheitsfehler, kein Grund, den Bau
        # anzuhalten. Als Fehler gezählt stünde der Selbsttest so lange rot, bis
        # jemand elf Bildschirmfotos gemacht hat — und ein dauerhaft roter Test
        # wird irgendwann ignoriert, dann fällt auch das Echte nicht mehr auf.
        melden('  HINWEIS  README.en.md zeigt %d deutsche Bilder — englische '
               'Version fehlt noch' % len(geteilt))
        melden('           (%s%s)'
               % (', '.join(geteilt[:3]), ' …' if len(geteilt) > 3 else ''))

    # Jedes eingebundene Bild muss es auch geben.
    for datei, bilder in (('README.en.md', bilder_en), ('README.md', bilder_de)):
        for b in sorted(bilder):
            if not os.path.exists(os.path.join('assets', b)):
                fehler.append('%s bindet assets/%s ein — Datei fehlt'
                              % (datei, b))
    return fehler


def main():
    # ⚠ Unter Windows steht die Konsole auf cp1252 — und daran starb dieses
    # Werkzeug mit `UnicodeEncodeError`, sobald es das erste `↔` ausgeben
    # wollte. Geprueft wurde damit nichts mehr; wer unter Windows entwickelt,
    # bekam nur einen Stapelabzug zu sehen. Gefunden am 27.08.2026.
    try:
        sys.stdout.reconfigure(encoding='utf-8', errors='replace')
    except Exception:
        pass                      # aeltere Pythons, umgeleitete Ausgabe

    print('Sprachfassungen:')
    fehler = pruefe()
    print()
    if fehler:
        print('%d Beanstandung(en):' % len(fehler))
        for f in fehler:
            print('  ·', f)
        return 1
    print('Deutsch und Englisch decken sich.')
    return 0


if __name__ == '__main__':
    sys.exit(main())
