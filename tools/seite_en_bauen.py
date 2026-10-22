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
Builds the English website under `en/` from the German pages.

The German pages are the only source: `index.html` and one `index.html` per
sub-page (SEITEN below), each built to the same place under `en/`. Every
text element carries `data-de` and `data-en`; this script puts the `data-en`
text into the element, sets every attribute that has a `data-en-<attribute>`
twin (meta description, labels, placeholders) to the English value, switches
`lang`, canonical address, `og:url`/`og:locale`, the language buttons, the
pictures and the video that exist in an English version, and points relative
asset paths one folder further up.

    python3 tools/seite_en_bauen.py            # write every en/.../index.html
    python3 tools/seite_en_bauen.py --pruefen  # exit 1 if one is outdated

Fails if an element with `data-de` contains markup or lacks `data-en`, so no
German text slips through unnoticed.
"""

import html
import os
import re
import sys

WURZEL = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ADRESSE = "https://versekit.xharig.com/"
# German pages, relative to the site root ("" is the start page).
SEITEN = ["", "rundgang/", "herunterladen/", "fragen/", "aenderungen/"]

KOPF = ("<!-- Generated from %sindex.html by tools/seite_en_bauen.py."
        " Do not edit; change the German page and run the script. -->\n")

TEXT_ELEMENT = re.compile(
    r'(<(?P<tag>[a-zA-Z][a-zA-Z0-9]*)\b(?P<attr>[^>]*\bdata-de="[^"]*"[^>]*)>)'
    r'(?P<inhalt>[^<]*)'
    r'(</(?P=tag)>)')
DATA_EN = re.compile(r'\bdata-en="([^"]*)"')


class Fehler(Exception):
    pass


def _ersetzen(muster, ersatz, text, anzahl, was):
    neu, n = re.subn(muster, ersatz, text)
    if n != anzahl:
        raise Fehler("%s: expected %d match(es), found %d" % (was, anzahl, n))
    return neu


def _attribute_tauschen(m):
    """`x="de" data-en-x="en"` becomes `x="en"` inside one tag."""
    tag = m.group(0)
    for name, wert in re.findall(r'\sdata-en-([a-zA-Z-]+)="([^"]*)"', tag):
        tag = re.sub(r'\s+data-en-%s="[^"]*"' % re.escape(name), "", tag, count=1)
        tag, n = re.subn(r'(?<![\w-])%s="[^"]*"' % re.escape(name),
                         lambda _m: '%s="%s"' % (name, wert), tag, count=1)
        if n != 1:
            raise Fehler('data-en-%s without %s="…": %s' % (name, name, tag[:120]))
    return tag


def bauen(quelle, pfad):
    """English version of the German page at `pfad` ("" or "rundgang/")."""
    gesamt = len(re.findall(r'\bdata-de="', quelle))
    getroffen = [0]

    def text_tauschen(m):
        en = DATA_EN.search(m.group("attr"))
        if en is None:
            raise Fehler("data-en missing: %s" % m.group(1)[:120])
        getroffen[0] += 1
        text = html.escape(html.unescape(en.group(1)), quote=False)
        return m.group(1) + text + m.group(5)

    seite = TEXT_ELEMENT.sub(text_tauschen, quelle)
    if getroffen[0] != gesamt:
        raise Fehler("%d of %d data-de elements could not be translated "
                     "(element contains markup?)" % (gesamt - getroffen[0], gesamt))

    seite = re.sub(r'<[a-zA-Z][^>]*\bdata-en-[^>]*>', _attribute_tauschen, seite)
    if re.search(r'\sdata-en-[a-zA-Z-]+="', seite):
        raise Fehler("data-en-<attribute> left over")

    hier = ADRESSE + pfad
    seite = _ersetzen(r'<html lang="de">', '<html lang="en">', seite, 1, "lang")
    seite = _ersetzen(r'<link rel="canonical" href="%s">' % re.escape(hier),
                      '<link rel="canonical" href="%sen/%s">' % (ADRESSE, pfad),
                      seite, 1, "canonical")
    seite = _ersetzen(r'<meta property="og:url" content="%s">' % re.escape(hier),
                      '<meta property="og:url" content="%sen/%s">' % (ADRESSE, pfad),
                      seite, 1, "og:url")
    seite = _ersetzen(r'<meta property="og:locale" content="de_DE">',
                      '<meta property="og:locale" content="en_US">\n'
                      '<meta property="og:locale:alternate" content="de_DE">',
                      seite, 1, "og:locale")
    seite = _ersetzen(r'<meta property="og:locale:alternate" content="en_US">\n',
                      '', seite, 1, "og:locale:alternate")
    seite = _ersetzen(r'(<a id="knopf-de" href="/%s"[^>]*?) class="an"' % re.escape(pfad),
                      r'\1', seite, 1, "button de")
    seite = _ersetzen(r'(<a id="knopf-en" href="/en/%s"[^>]*?)>' % re.escape(pfad),
                      r'\1 class="an">', seite, 1, "button en")

    seite = re.sub(r'(data-bild="([^"]+)\.png" src="((?:\.\./)*)assets/)[^"]+"',
                   lambda m: m.group(1) + m.group(2) + '-en.png"', seite)
    if "versekit-de." in seite:
        seite = _ersetzen(r'versekit-de\.(mp4|jpg)', r'versekit-en.\1', seite, 2, "video")

    seite = re.sub(r'\b(src|href|poster)="((?:\.\./)*)assets/', r'\1="../\2assets/', seite)

    if "<!DOCTYPE html>\n" not in seite:
        raise Fehler("DOCTYPE line not found")
    return seite.replace("<!DOCTYPE html>\n", "<!DOCTYPE html>\n" + KOPF % pfad, 1)


def bilder_pruefen(seite, ziel):
    fehlend = []
    ordner = os.path.dirname(ziel)
    for pfad in re.findall(r'\b(?:src|poster)="((?:\.\./)+assets/[^"?]+)', seite):
        if not os.path.exists(os.path.normpath(os.path.join(ordner, pfad))):
            fehlend.append(pfad)
    if fehlend:
        raise Fehler("missing files: " + ", ".join(sorted(set(fehlend))))


def main(argv):
    veraltet = False
    for pfad in SEITEN:
        quelle_datei = os.path.join(WURZEL, pfad, "index.html")
        ziel = os.path.join(WURZEL, "en", pfad, "index.html")
        name = "en/%sindex.html" % pfad
        with open(quelle_datei, encoding="utf-8") as f:
            quelle = f.read()
        try:
            seite = bauen(quelle, pfad)
            bilder_pruefen(seite, ziel)
        except Fehler as e:
            print("%s: %s" % (name, e))
            return 1

        if "--pruefen" in argv:
            try:
                with open(ziel, encoding="utf-8") as f:
                    vorhanden = f.read()
            except OSError:
                vorhanden = None
            if vorhanden != seite:
                print("%s is outdated: run python3 tools/seite_en_bauen.py" % name)
                veraltet = True
            else:
                print("%s is up to date" % name)
            continue

        os.makedirs(os.path.dirname(ziel), exist_ok=True)
        with open(ziel, "w", encoding="utf-8", newline="\n") as f:
            f.write(seite)
        print("written: " + name)
    return 1 if veraltet else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
