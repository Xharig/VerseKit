# -*- coding: utf-8 -*-
"""Meldet eine neue Version im eigenen Discord — als lesbare Karte.

**Warum nicht die GitHub-Discord-Anbindung?** Die richtet man in Discord ein und
sie postet immer denselben Einzeiler: `[Repo] New release published: v3.0.0-rc60`.
Wer das liest, weiß nicht, ob sich das Laden lohnt. Gemeldet am 27.08.2026 nach
dem Vergleich mit dem StarStrings-Kanal: „finde nur das das hier besser aussieht,
wäre das bei mir auch machbar mit bissl mehr infos, statt einfach nur dem nakten
Link?"

Dieses Skript baut stattdessen eine **Embed** aus dem CHANGELOG — dieselbe Quelle,
aus der auch die Release-Beschreibung und „Was ist neu" im Werkzeug gespeist
werden. Ein Text, drei Orte.

⚠ **Es steht nur drin, was in DIESER Version neu ist.** `release_text.abschnitt()`
fällt bei einer Vorabfassung ohne eigenen Abschnitt auf die **Grundversion**
zurück — sinnvoll für die Release-Seite, hier aber falsch: Dann stünde in der
Meldung zu `rc60` alles, was seit `v3.0.0` je dazukam. Deshalb prüft dieses
Skript **exakt** auf den Tag und schweigt lieber, als zu viel zu erzählen.
Gemeldet: „wichtig ist das da dann wirklich nur das steht was in der version neu
ist."

**Aufruf** (aus dem Bau-Ablauf, `DISCORD_WEBHOOK` als Secret):

    python3 .github/scripts/discord_release.py v3.0.0-rc60
"""

import json
import os
import re
import sys
import urllib.request

HIER = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HIER)

import release_text                                          # noqa: E402

REPO = 'Xharig/SC-BP-Watcher'
GRUEN = 0x9ce430          # Xharig-Neongrün für dunklen Grund
GOLD = 0xd8a03a           # Testfassungen
LOGO = ('https://raw.githubusercontent.com/%s/main/assets/icon.png' % REPO)

# Discord-Grenzen. Wer sie reißt, bekommt keine Fehlermeldung, sondern eine
# abgeschnittene Karte — deshalb lieber selbst kürzen.
MAX_BESCHREIBUNG = 3800
MAX_TITEL = 250


def nur_diese_fassung(tag):
    """Der Changelog-Block **genau** zu diesem Tag — sonst nichts.

    Gibt `None` zurück, wenn es keinen eigenen Abschnitt gibt. Der Aufrufer
    postet dann nur den Link statt eines fremden Textes.
    """
    pfad = os.path.join(os.path.dirname(HIER), '..', 'CHANGELOG.md')
    pfad = os.path.normpath(pfad)
    try:
        with open(pfad, encoding='utf-8') as f:
            text = f.read()
    except OSError:
        return None
    zahl = tag.lstrip('v')

    def suche(gesuchte_zahl):
        for block in re.split(r'^## ', text, flags=re.M)[1:]:
            kopf, _, rest = block.partition('\n')
            # Wortgrenze: „3.0.0" darf nicht in „3.0.0-rc1" fassen und umgekehrt.
            if re.search(r'(?<![\w.-])v?%s(?![\w.-])' % re.escape(gesuchte_zahl),
                         kopf):
                return rest.strip()
        return None

    treffer = suche(zahl)
    if treffer:
        return treffer

    # ⚠ **Testfassungen bekommen den Block ihrer Zielversion.**
    #
    # Ein `rc` hat fast nie einen eigenen Changelog-Abschnitt — gesammelt wird
    # unter der Version, auf die er hinarbeitet (`v3.3.0-rc1` → `## v3.3.0`).
    # Ohne diesen Rückgriff stand im Discord nur „Im Changelog steht zu dieser
    # Version noch nichts", ausgerechnet bei der Ankündigung, die Tester lesen
    # sollen (29.08.2026 bei rc1 genau so passiert).
    #
    # Das ist **nicht** der Sammelblock einer fremden Version, vor dem die
    # Vorsicht hier schützen soll: 3.3.0-rc1 gehört zu 3.3.0. Fremd wäre
    # 3.2.1 — und das findet dieser Rückgriff nicht.
    grund = re.sub(r'-(rc|beta|alpha|dev)[0-9.]*$', '', zahl, flags=re.I)
    if grund != zahl:
        return suche(grund)
    return None


def fuer_discord(text):
    """Changelog-Markdown so umformen, dass Discord es lesbar darstellt."""
    zeilen = []
    for zeile in text.split('\n'):
        # Aufklapp-Hinweise im Markdown kennt Discord nicht — der Titel bleibt, die
        # Auszeichnung wird zu einem Zeichen davor.
        m = re.match(r'>\s*\[!(\w+)\]\s*(.*)', zeile)
        if m:
            art, titel = m.group(1).lower(), m.group(2).strip()
            marke = {'warning': '⚠️', 'important': '❗', 'success': '✅'}.get(art, 'ℹ️')
            zeilen.append('> %s **%s**' % (marke, titel) if titel else '> %s' % marke)
            continue
        zeilen.append(zeile)
    text = '\n'.join(zeilen)

    # ⚠ Weiche Umbrüche zusammenziehen. Der Changelog bricht bei ~80 Zeichen um,
    # damit er sich im Editor lesen lässt. Discord bricht selbst um — die harten
    # Umbrüche ergäben dort ein zerhacktes Schriftbild.
    # ⚠ Der Lookahead muss die **Einrückung überspringen**. Ein Blockzitat im
    # Changelog ist oft eingerückt („  > Bei zwei Zeilen …"); direkt nach dem
    # Umbruch steht dann ein Leerzeichen, nicht das `>`. Ohne `\s*` im
    # Lookahead wurden solche Zitate zusammengezogen, und die `>` standen
    # mitten im Satz — so geschehen bei v3.3.0-rc1.
    text = re.sub(r'\n(?!\s*[\n\-*>#])\s*', ' ', text)

    if len(text) > MAX_BESCHREIBUNG:
        # An einem Absatz abschneiden, nicht mitten im Wort.
        schnitt = text.rfind('\n', 0, MAX_BESCHREIBUNG)
        text = text[:schnitt if schnitt > 0 else MAX_BESCHREIBUNG].rstrip()
        text += '\n\n*(gekürzt — der ganze Text steht im Release)*'
    return text.strip()


def bauen(tag):
    vorab = release_text.ist_vorab(tag)
    inhalt = nur_diese_fassung(tag)
    link = 'https://github.com/%s/releases/tag/%s' % (REPO, tag)

    if inhalt:
        beschreibung = fuer_discord(inhalt)
    else:
        # Lieber ehrlich kurz als versehentlich der Sammelblock einer
        # anderen Version.
        beschreibung = ('Im Changelog steht zu dieser Version noch nichts — '
                        'was drin ist, sagt die [Release-Seite](%s).' % link)

    einbettung = {
        'title': ('VerseKit %s' % tag.lstrip('v'))[:MAX_TITEL],
        'url': link,
        'description': beschreibung,
        'color': GOLD if vorab else GRUEN,
        'thumbnail': {'url': LOGO},
        'footer': {'text': (
            'Testfassung · läuft normal, ist aber weniger lange erprobt'
            if vorab else 'Fertige Version')},
    }
    return {'embeds': [einbettung]}


def main():
    if len(sys.argv) < 2:
        sys.exit('Aufruf: discord_release.py <tag>')
    tag = sys.argv[1]
    haken = os.environ.get('DISCORD_WEBHOOK', '').strip()
    if not haken:
        # ⚠ Kein Fehler: Wer den Schlüssel nicht hinterlegt hat, soll trotzdem
        # bauen können. Der Bau darf an einer Discord-Meldung nicht scheitern.
        print('DISCORD_WEBHOOK nicht gesetzt — keine Meldung verschickt.')
        return 0

    daten = json.dumps(bauen(tag)).encode('utf-8')
    anfrage = urllib.request.Request(
        haken, data=daten,
        headers={'Content-Type': 'application/json',
                 'User-Agent': 'SC-BP-Watcher-Release'})
    try:
        with urllib.request.urlopen(anfrage, timeout=20) as antwort:
            print('Discord: gemeldet (HTTP %s)' % antwort.status)
    except Exception as ausnahme:
        # ⚠ Auch hier kein Abbruch. Die Version ist gebaut und veröffentlicht;
        # eine gescheiterte Chat-Meldung darf das nicht rot färben.
        print('Discord-Meldung fehlgeschlagen: %s' % ausnahme)
    return 0


if __name__ == '__main__':
    sys.exit(main())
