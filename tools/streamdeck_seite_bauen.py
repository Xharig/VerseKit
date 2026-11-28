# -*- coding: utf-8 -*-
#
# VerseKit — companion for Star Citizen.
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
Builds the website page `streamdeck/index.html` from the guide of the Stream
Deck profiles.

The guide has one source per language, the same texts that go into the
profile ZIP: `streamdeck-plugin/zip/LIESMICH.md` (German) and `README.md`
(English). Both must have the same structure: the same sections in the same
order, and in every section the same blocks (paragraphs, lists, tables,
questions) with the same number of entries. The script pairs them block by
block into one German page whose elements carry `data-de` and `data-en`;
`tools/seite_en_bauen.py` then builds `en/streamdeck/`.

Each `###` section is known by its English heading (ABSCHNITTE gives its id);
a heading not listed there stops the script. The device table becomes the
device choice, the questions (`####` per question) become folding entries,
the version history (`####` per version) folding versions.

    python3 tools/streamdeck_seite_bauen.py            # write streamdeck/index.html
    python3 tools/streamdeck_seite_bauen.py --pruefen  # exit 1 if it is outdated

Fails if the two sources differ in structure, so no German text is missing an
English twin and the other way round.
"""

import html
import os
import re
import sys

WURZEL = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
QUELLE = os.path.join(WURZEL, "streamdeck-plugin", "zip")
ZIEL = os.path.join(WURZEL, "streamdeck", "index.html")
ADRESSE = "https://versekit.xharig.com/"
REPO = "https://github.com/Xharig/VerseKit"
# The profile ZIP attached to every release (tools/streamdeck_profile_zip.py), always the newest.
ZIP_NAME = "VerseKit-StreamDeck-Profile.zip"
ZIP_URL = REPO + "/releases/latest/download/" + ZIP_NAME

# Section id per English `###` heading. "geraet", "faq", "updates" and
# "rueckmeldung" must be there; the profile keys "Open guide" and "FAQ" point
# to this page and to #faq.
ABSCHNITTE = {
    "What you need to know": "ueberblick",
    "Which file for which device?": "geraet",
    "Set up": "einrichten",
    "Keys you must enter in the game yourself": "im-spiel",
    "Stream Deck + XL": "plus-xl",
    "Where am I?": "wo-bin-ich",
    "Frequently asked questions": "faq",
    "Version history": "updates",
    "Feedback": "rueckmeldung",
}
PFLICHT = ("geraet", "faq", "updates", "rueckmeldung")
# In the list of contents below the page heading.
INHALT = ("geraet", "einrichten", "im-spiel", "faq", "updates")

# The page's own heading and introduction (the guide's title names the files).
H1 = ("VerseKit für Stream Deck", "VerseKit for Stream Deck")
EINLEITUNG = (
    "Fertige Profile für jedes Stream Deck, vom Mini bis zum Stream Deck + XL, auf Deutsch und Englisch. "
    "Die Spieltasten gehen sofort, die VerseKit-Tasten öffnen Seiten und zeigen, was VerseKit gerade tut.",
    "Ready-made profiles for every Stream Deck, from the Mini to the Stream Deck + XL, in English and German. "
    "The game keys work right away, the VerseKit keys open pages and show what VerseKit is doing.")

# Device id from the profile file name, in the order of the device table.
GERAET_ID = re.compile(r"VerseKit-StreamDeck-([A-Za-z0-9]+)-DE\.streamDeckProfile")

KOPF = ("<!-- Generated from streamdeck-plugin/zip/LIESMICH.md and README.md by"
        " tools/streamdeck_seite_bauen.py. Do not edit; change the guide and run the script. -->\n")

DOWNLOAD_SVG = ('<svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" '
                'stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 15V3"/>'
                '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/></svg>')


class Fehler(Exception):
    pass


# ---------- Reading the guide ----------

def _zeilen(datei):
    with open(os.path.join(QUELLE, datei), encoding="utf-8") as f:
        return f.read().splitlines()


def _bloecke(zeilen):
    """Blocks of one piece of Markdown: ('p', text), ('ol'|'ul', [items]),
    ('table', [rows of cells]), ('h4', text)."""
    bloecke = []
    i = 0
    while i < len(zeilen):
        z = zeilen[i].rstrip()
        if not z.strip():
            i += 1
        elif z.startswith("#### "):
            bloecke.append(("h4", z[5:].strip()))
            i += 1
        elif z.lstrip().startswith("|"):
            zeilen_tab = []
            while i < len(zeilen) and zeilen[i].lstrip().startswith("|"):
                zellen = [c.strip() for c in zeilen[i].strip().strip("|").split("|")]
                if not all(re.fullmatch(r":?-+:?", c) for c in zellen):
                    zeilen_tab.append(zellen)
                i += 1
            bloecke.append(("table", zeilen_tab))
        elif re.match(r"(\d+\.|-) ", z):
            art = "ol" if z[0].isdigit() else "ul"
            # A numbered list may go on after a table: keep its first number.
            start = int(z.split(".")[0]) if art == "ol" else 1
            punkte = []
            while i < len(zeilen) and zeilen[i].strip() and not zeilen[i].lstrip().startswith("|"):
                m = re.match(r"(\d+\.|-) (.*)", zeilen[i])
                if m:
                    punkte.append(m.group(2).strip())
                else:
                    punkte[-1] += " " + zeilen[i].strip()
                i += 1
            bloecke.append((art, punkte, start))
        else:
            text = []
            while (i < len(zeilen) and zeilen[i].strip() and not zeilen[i].startswith("#")
                   and not zeilen[i].lstrip().startswith("|") and not re.match(r"(\d+\.|-) ", zeilen[i])):
                text.append(zeilen[i].strip())
                i += 1
            bloecke.append(("p", " ".join(text)))
    return bloecke


def lesen(datei):
    """Title, introduction and the sections (heading, blocks) of one source."""
    zeilen = _zeilen(datei)
    titel = None
    vorne = []
    abschnitte = []
    for z in zeilen:
        if z.startswith("# ") and titel is None:
            titel = z[2:].strip()
        elif z.startswith("### "):
            abschnitte.append([z[4:].strip(), []])
        elif abschnitte:
            abschnitte[-1][1].append(z)
        else:
            vorne.append(z)
    if titel is None:
        raise Fehler("%s: no title (# …)" % datei)
    # Before the first section stands only the line naming the other language.
    return {"titel": titel, "abschnitte": [(k, _bloecke(z)) for k, z in abschnitte]}


def paaren(de, en):
    """Checks that both sources have the same structure; the section ids in order."""
    if len(de["abschnitte"]) != len(en["abschnitte"]):
        raise Fehler("%d German and %d English sections (###)" % (len(de["abschnitte"]), len(en["abschnitte"])))
    ids = []
    for _, (ke, _) in zip(de["abschnitte"], en["abschnitte"]):
        if ke not in ABSCHNITTE:
            raise Fehler("unknown section %r: give it an id in ABSCHNITTE" % ke)
        ids.append(ABSCHNITTE[ke])
    fehlt = [p for p in PFLICHT if p not in ids]
    if fehlt:
        raise Fehler("sections missing: %s" % ", ".join(fehlt))
    for nr, ((kd, bd), (ke, be)) in enumerate(zip(de["abschnitte"], en["abschnitte"])):
        if [b[0] for b in bd] != [b[0] for b in be]:
            raise Fehler("section %d (%s / %s): blocks differ: %s / %s"
                         % (nr + 1, kd, ke, [b[0] for b in bd], [b[0] for b in be]))
        for x, y in zip(bd, be):
            if x[0] in ("ol", "ul") and len(x[1]) != len(y[1]):
                raise Fehler("section %s: list with %d / %d items" % (kd, len(x[1]), len(y[1])))
            if x[0] == "table" and [len(r) for r in x[1]] != [len(r) for r in y[1]]:
                raise Fehler("section %s: tables differ in rows or columns" % kd)
            if x[0] == "ol" and x[2] != y[2]:
                raise Fehler("section %s: numbered list starts at %d / %d" % (kd, x[2], y[2]))
    return ids


# ---------- Inline text ----------

def _rein(text, streifen=True):
    """Plain text: links keep their text, emphasis and code marks go, straight
    double quotes (the English guide) become typographic ones."""
    text = re.sub(r"\[([^\]]+)\]\([^)]+\)", r"\1", text)
    text = re.sub(r'"([^"]+)"', "“\\1”", text)
    text = text.replace("**", "").replace("`", "")
    return text.strip() if streifen else text.rstrip()


def _attr(de, en):
    return 'data-de="%s" data-en="%s"' % (html.escape(de, quote=True), html.escape(en, quote=True))


def element(tag, de, en, klasse="", streifen=True):
    """One element with German text and its English twin."""
    de, en = _rein(de, streifen), _rein(en, streifen)
    k = ' class="%s"' % klasse if klasse else ""
    return "<%s%s %s>%s</%s>" % (tag, k, _attr(de, en), html.escape(de, quote=False), tag)


_FETT_VORN = re.compile(r"^\*\*(.+?)\*\*\s*(.*)$")


def inhalt(tag, de, en, klasse=""):
    """Like `element`, but a bold start ("**Step:** text") becomes its own <strong>."""
    md, me = _FETT_VORN.match(de), _FETT_VORN.match(en)
    if not (md and me):
        return element(tag, de, en, klasse)
    teile = element("strong", md.group(1), me.group(1))
    if md.group(2) or me.group(2):
        # The space after the bold start belongs to the text, so a comma right
        # after it ("**Step**, text") gets none, in each language on its own.
        def weiter(t):
            return t if re.match(r"[,.;:]", t) else " " + t
        teile += element("span", weiter(md.group(2)), weiter(me.group(2)), streifen=False)
    k = ' class="%s"' % klasse if klasse else ""
    return "<%s%s>%s</%s>" % (tag, k, teile, tag)


def _nur_code(zelle):
    """The code names of a cell made only of `code` / `code`, else None."""
    teile = [t.strip() for t in zelle.split("/")]
    if all(re.fullmatch(r"`[^`]+`", t) for t in teile):
        return [t.strip("`") for t in teile]
    return None


# ---------- Blocks ----------

def tabelle(de, en):
    kopf = "".join(element("th", a, b) for a, b in zip(de[0], en[0]))
    zeilen = []
    for zd, ze in zip(de[1:], en[1:]):
        zellen = []
        for nr, (a, b) in enumerate(zip(zd, ze)):
            if nr == len(zd) - 1 and len(zd) == 2:
                zellen.append("<td>%s</td>" % element("kbd", a, b))
            else:
                zellen.append(element("td", a, b))
        zeilen.append("<tr>%s</tr>" % "".join(zellen))
    return ('<div class="tabelle"><table class="tasten sd-tasten"><thead><tr>%s</tr></thead>\n<tbody>\n%s\n</tbody></table></div>'
            % (kopf, "\n".join(zeilen)))


def bloecke_html(bd, be, ebene=3):
    teile = []
    for x, y in zip(bd, be):
        if x[0] == "p":
            teile.append(inhalt("p", x[1], y[1]))
        elif x[0] in ("ol", "ul"):
            punkte = "\n".join("  " + inhalt("li", a, b) for a, b in zip(x[1], y[1]))
            start = ' start="%d"' % x[2] if x[0] == "ol" and x[2] != 1 else ""
            teile.append("<%s%s>\n%s\n</%s>" % (x[0], start, punkte, x[0]))
        elif x[0] == "table":
            teile.append(tabelle(x[1], y[1]))
        elif x[0] == "h4":
            teile.append(element("h%d" % ebene, x[1], y[1]))
    return "\n".join(teile)


def geraete(bd, be):
    """The device table as a choice: one radio button per device, one panel
    with the two profile files per device. Without :has() every panel shows."""
    tab_d = next(b for b in bd if b[0] == "table")[1]
    tab_e = next(b for b in be if b[0] == "table")[1]
    knoepfe, felder = [], []
    for nr, (zd, ze) in enumerate(zip(tab_d[1:], tab_e[1:])):
        dateien = _nur_code(zd[1])
        if dateien is None or sorted(dateien) != sorted(_nur_code(ze[1]) or []) or len(dateien) != 2:
            raise Fehler("device table: the file column must hold the same two `file` names in both languages")
        m = next(filter(None, (GERAET_ID.fullmatch(d) for d in dateien)), None)
        if not m:
            raise Fehler("device table: no German profile file in %s" % dateien)
        gid = m.group(1).lower()
        an = " checked" if nr == 0 else ""
        knoepfe.append('<label class="geraet-knopf"><input type="radio" name="geraet" value="%s"%s>%s</label>'
                       % (gid, an, element("span", zd[0], ze[0])))
        de_datei = next(d for d in dateien if d.endswith("-DE.streamDeckProfile"))
        en_datei = next(d for d in dateien if d.endswith("-EN.streamDeckProfile"))
        felder.append(
            '<div class="geraet-feld" data-geraet="%s">\n'
            '  %s\n'
            '  <dl>\n'
            '    <div>%s<dd><code translate="no">%s</code></dd></div>\n'
            '    <div>%s<dd><code translate="no">%s</code></dd></div>\n'
            '  </dl>\n'
            '</div>'
            % (gid, element("h3", zd[0], ze[0]),
               element("dt", "Deutsch: deutsche Tastaturlage und Spieltexte",
                       "German: German keyboard layout and game texts"), de_datei,
               element("dt", "Englisch: US-Tastaturlage und englische Spieltexte",
                       "English: US keyboard layout and English game texts"), en_datei))
    legende = element("legend", "Dein Gerät", "Your device")
    return ('<fieldset class="geraete-wahl">\n%s\n<div class="geraet-knoepfe">\n%s\n</div>\n</fieldset>\n'
            '<div class="geraet-felder">\n%s\n</div>'
            % (legende, "\n".join(knoepfe), "\n".join(felder)))


def fragen(bd, be):
    """Questions: each `####` with the blocks below it becomes one <details>."""
    gruppen = []
    for x, y in zip(bd, be):
        if x[0] == "h4":
            gruppen.append([x[1], y[1], [], []])
        elif gruppen:
            gruppen[-1][2].append(x)
            gruppen[-1][3].append(y)
        else:
            raise Fehler("questions: text before the first question (####)")
    teile = []
    for fd, fe, ad, ae in gruppen:
        teile.append("<details>\n  %s\n%s\n</details>" % (element("summary", fd, fe), bloecke_html(ad, ae)))
    return '<div class="faq">\n%s\n</div>' % "\n".join(teile)


def verlauf(bd, be):
    """Version history: each `####` version with its list, the newest open."""
    teile = []
    version = None
    for x, y in zip(bd, be):
        if x[0] == "h4":
            if x[1] != y[1]:
                raise Fehler("version history: %s / %s" % (x[1], y[1]))
            if version is None:
                version = x[1]
            teile.append([x[1], [], []])
        elif teile:
            teile[-1][1].append(x)
            teile[-1][2].append(y)
    if version is None or not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise Fehler("version history: the first entry must be a version like 5.7.0")
    html_teile = []
    for nr, (v, ad, ae) in enumerate(teile):
        offen = " open" if nr == 0 else ""
        html_teile.append('<details class="version"%s>\n  <summary><span class="nummer">%s</span></summary>\n'
                          '  <div class="inhalt">\n%s\n  </div>\n</details>' % (offen, v, bloecke_html(ad, ae)))
    return '<div class="sd-verlauf">\n%s\n</div>' % "\n".join(html_teile), version


# ---------- Page ----------

def seite(de, en):
    ids = paaren(de, en)
    ab_d = dict(zip(ids, de["abschnitte"]))
    ab_e = dict(zip(ids, en["abschnitte"]))
    verlauf_html, version = verlauf(ab_d["updates"][1], ab_e["updates"][1])

    def ueberschrift(sid):
        return element("h2", ab_d[sid][0], ab_e[sid][0])

    def sektion(sid, inhalt_html, klasse="sd-abschnitt"):
        return ('<section id="%s" class="%s">\n  <div class="mitte">\n%s\n%s\n  </div>\n</section>'
                % (sid, klasse, ueberschrift(sid), inhalt_html))

    # Device section: first paragraph, the choice, then everything after the table.
    bd, be = ab_d["geraet"][1], ab_e["geraet"][1]
    t = next(i for i, b in enumerate(bd) if b[0] == "table")
    geraet_html = "\n".join([bloecke_html(bd[:t], be[:t]), geraete(bd, be),
                             '<div class="sd-text">\n%s\n</div>' % bloecke_html(bd[t + 1:], be[t + 1:])])

    inhaltsliste = "\n".join(
        '      <li><a href="#%s" %s>%s</a></li>'
        % (sid, _attr(_rein(ab_d[sid][0]), _rein(ab_e[sid][0])), html.escape(_rein(ab_d[sid][0]), quote=False))
        for sid in INHALT if sid in ab_d)

    laden = """<section id="laden" class="laden">
  <div class="mitte">
    %s
    <div class="dateien">
      <div class="datei">
        %s
        %s
        %s
        <a class="knopf" href="../herunterladen/">%s</a>
      </div>
      <div class="datei">
        %s
        %s
        <code translate="no">%s</code>
        <a class="knopf voll" href="%s">%s%s</a>
      </div>
    </div>
    <p class="fassung"><span %s>%s</span> <strong>%s</strong>
      <a class="raus" href="%s/releases/tag/v%s" %s>%s</a></p>
  </div>
</section>""" % (
        element("h2", "Herunterladen", "Download"),
        element("h3", "Mit VerseKit", "With VerseKit"),
        element("p", "Plugin und Profil", "Plugin and profile", "art"),
        element("p", "VerseKit 5.7.0 oder neuer installiert das Plugin und bietet das Profil für dein Gerät an: "
                     "Seite „Tasten & Stream Deck“, Knopf „Stream Deck einrichten“.",
                "VerseKit 5.7.0 or newer installs the plugin and offers the profile for your device: "
                "page “Keys & Stream Deck”, button “Set up Stream Deck”."),
        element("span", "VerseKit herunterladen", "Download VerseKit"),
        element("h3", "Profile als ZIP", "Profiles as ZIP"),
        element("p", "Alle Profile, Deutsch und Englisch, immer die neueste Fassung",
                "All profiles, English and German, always the newest version", "art"),
        ZIP_NAME, ZIP_URL, DOWNLOAD_SVG,
        element("span", "ZIP herunterladen", "Download ZIP"),
        _attr("Profilstand", "Profile version"), "Profilstand", version,
        REPO, version, _attr("Fassung auf GitHub ansehen", "See the version on GitHub"),
        "Fassung auf GitHub ansehen")

    ruck_d, ruck_e = ab_d["rueckmeldung"][1], ab_e["rueckmeldung"][1]
    rueckmeldung = sektion("rueckmeldung", "%s\n<p class=\"knopfreihe\"><a class=\"knopf\" href=\"https://xharig.com/discord\">%s</a></p>"
                           % (bloecke_html(ruck_d, ruck_e), element("span", "Zum Discord", "Go to Discord")))

    # Sections in the order of the guide, the download first.
    teile = [laden]
    for sid in ids:
        if sid == "geraet":
            teile.append(sektion(sid, geraet_html))
        elif sid == "faq":
            teile.append(sektion(sid, fragen(ab_d[sid][1], ab_e[sid][1])))
        elif sid == "updates":
            teile.append(sektion(sid, verlauf_html))
        elif sid == "rueckmeldung":
            teile.append(rueckmeldung)
        else:
            teile.append(sektion(sid, bloecke_html(ab_d[sid][1], ab_e[sid][1])))
    hauptteil = "\n\n".join(teile)

    with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "streamdeck_seite_vorlage.html"),
              encoding="utf-8") as f:
        vorlage = f.read()
    ersetzt = {
        "{{KOPF}}": KOPF,
        "{{H1}}": element("h1", *H1),
        "{{EINLEITUNG}}": element("p", *EINLEITUNG, klasse="einleitung"),
        "{{INHALTSLISTE}}": inhaltsliste,
        "{{HAUPTTEIL}}": hauptteil,
    }
    for platz, wert in ersetzt.items():
        if vorlage.count(platz) != 1:
            raise Fehler("template: %s expected once" % platz)
        vorlage = vorlage.replace(platz, wert)
    return vorlage


def main(argv):
    try:
        neu = seite(lesen("LIESMICH.md"), lesen("README.md"))
    except Fehler as e:
        print("streamdeck/index.html: %s" % e)
        return 1
    if "--pruefen" in argv:
        try:
            with open(ZIEL, encoding="utf-8") as f:
                vorhanden = f.read()
        except OSError:
            vorhanden = None
        if vorhanden != neu:
            print("streamdeck/index.html is outdated: run python3 tools/streamdeck_seite_bauen.py")
            return 1
        print("streamdeck/index.html is up to date")
        return 0
    os.makedirs(os.path.dirname(ZIEL), exist_ok=True)
    with open(ZIEL, "w", encoding="utf-8", newline="\n") as f:
        f.write(neu)
    print("written: streamdeck/index.html")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
