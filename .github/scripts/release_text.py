# -*- coding: utf-8 -*-
"""Baut den Release-Text aus den beiden CHANGELOG-Dateien.

Englisch steht oben, Deutsch in einem aufklappbaren Block darunter: Auf der
Release-Seite lesen überwiegend Menschen, die kein Deutsch können, aber die
deutschsprachige Community soll ihre Version auch bekommen.

Dieselben Texte zeigt das Werkzeug unter „Was ist neu" — dort allerdings jedem
nur in seiner Sprache.

Bewusst eine eigene Datei statt eines Heredocs im Workflow: Eingerückter
Python-Code in YAML ist brüchig, und Fehler fallen erst beim Bauen auf.

Aufruf:  python3 .github/scripts/release_text.py v2.0.0 > release-text.md
"""
import os
import re
import sys

DATEIEN = {'en': 'CHANGELOG.en.md', 'de': 'CHANGELOG.md'}


def grundversion(tag):
    """Aus `v3.0.0-rc5` wird `3.0.0` — die Version, auf die es hinausläuft."""
    return tag.lstrip('v').split('-', 1)[0]


def ist_vorab(tag):
    return any(w in tag for w in ('-rc', '-beta', '-alpha'))


def abschnitt(pfad, tag):
    """Der Block zur getaggten Version aus einer CHANGELOG-Datei.

    ⚠ Für eine **Vorabfassung** gibt es keinen eigenen Abschnitt: Im Changelog
    steht `## v3.0.0`, getaggt wird aber `v3.0.0-rc5`. Vorher fand das Skript
    dann gar nichts und schrieb den Rückfallsatz „siehe Changelog" ins Release —
    wer testen soll, erfuhr also nicht, was zu testen ist. Deshalb wird bei einer
    Vorabfassung der Abschnitt der Grundversion genommen.
    """
    try:
        with open(pfad, encoding='utf-8') as f:
            text = f.read()
    except OSError:
        return ''
    kandidaten = [tag.lstrip('v')]
    if ist_vorab(tag):
        kandidaten.append(grundversion(tag))
    for zahl in kandidaten:
        for block in re.split(r'^## ', text, flags=re.M)[1:]:
            kopf, _, rest = block.partition('\n')
            # Auf Wortgrenze prüfen: „3.0.0" darf nicht in „3.0.0-rc1" fassen
            # und umgekehrt.
            if re.search(r'(?<![\w.-])v?%s(?![\w.-])' % re.escape(zahl), kopf):
                return rest.strip()
    return ''


def vorab_kopf(tag):
    """Der Hinweis über einer Testfassung — was sie ist und was Tester brauchen."""
    if not ist_vorab(tag):
        return ''
    grund = grundversion(tag)
    vergleich = ('https://github.com/Xharig/VerseKit/compare/'
                 'v%s...%s' % (VORIGER[0], tag)) if VORIGER[0] else ''
    zeilen = [
        '> ### 🧪 Testfassung für v%s' % grund,
        '>',
        '> Das ist eine **Testfassung** für v%s. Sie wird niemandem als Update '
        'angeboten — sie ist zum Ausprobieren da. Darunter steht alles, was v%s '
        'bisher bringt.' % (grund, grund),
    ]
    if vergleich:
        zeilen += ['>', '> **Was sich seit der vorigen Testfassung geändert hat:** %s'
                   % vergleich]
    zeilen += [
        '>',
        '> <details><summary><b>English</b></summary>',
        '>',
        '> This is a **pre-release**. It is not offered as an update to anyone; '
        'it is here to be tried out. The list below is everything v%s brings so '
        'far — the parts already in this build.' % grund,
    ]
    if vergleich:
        zeilen += ['>', '> **Changed since the previous test build:** %s' % vergleich]
    zeilen += ['>', '> </details>', '']
    return '\n'.join(zeilen) + '\n'


# Die vorige Vorabfassung — für den Vergleichslink. Wird in `main()` gesetzt.
VORIGER = ['']


def zusammensetzen(englisch, deutsch):
    """Deutsch oben, Englisch aufklappbar darunter.

    ⚠ **Umgedreht am 31.08.2026**, zusammen mit den Doku-Dateien:
    Deutsch ist die Hauptsprache des Projekts, und wer die
    Releases-Seite aufschlaegt, soll sie zuerst lesen. Englisch bleibt
    vollstaendig — nur eine Klappe tiefer.
    """
    if not deutsch:
        return englisch or ''
    if not englisch:
        return deutsch
    return ('%s\n\n---\n\n<details>\n<summary><b>English</b></summary>\n\n%s\n\n</details>'
            % (deutsch, englisch))


# Fester Anhang unter jeder Release-Notiz.
#
# Warum bei **jedem** Release und nicht nur einmal: Die SmartScreen-Meldung
# kommt bei jeder neuen Version wieder — jede Datei ist neu und damit wieder
# „unbekannt". Auch beim Selbst-Update aus dem Werkzeug heraus. Ohne den Hinweis
# an der Stelle, wo die Leute die Datei holen, ist das jede Woche dieselbe Frage.
#
# Ein Tester hielt die Meldung für einen Virenfund und schickte ein
# „Downloading Virus"-Bild in den Discord — verständlich, wenn Windows sagt
# „Der Computer wurde durch Windows geschützt".
HINWEIS = """
---

### ⚠️ Windows: „Der Computer wurde durch Windows geschützt" / "Windows protected your PC"

**Das ist kein Virenfund.** **Weitere Informationen → Trotzdem ausführen** — danach kommt
die Meldung nicht wieder.

SmartScreen prüft nicht, *ob* ein Programm schädlich ist, sondern ob es **bekannt** ist.
Bekannt wird eine Datei durch eine gekaufte Code-Signatur oder sehr viele Downloads — ein
kostenloses Fan-Werkzeug hat beides nicht, und jede neue Version fängt wieder bei null an.
Der Quellcode ist offen, die Datei wird **nicht von mir** gebaut, sondern von GitHub
Actions aus genau diesem Quellcode, und jede Datei oben trägt ihre SHA-256-Prüfsumme.
Unter Linux gibt es diese Meldung nicht.

Für dieses Projekt ist eine kostenlose Code-Signatur bei der
[SignPath Foundation](https://signpath.org/) beantragt. Sobald sie bewilligt ist, werden
die Windows-Dateien oben von SignPath unterschrieben.

<details>
<summary><b>English</b></summary>

**This is not a virus detection.** Click **More info → Run anyway** — it will not ask again.

SmartScreen does not check whether a program is harmful, only whether it is *known*. That
takes a paid code-signing certificate or a great many downloads; a free fan tool has
neither, and every new version starts from zero. The source is open, the file is built by
**GitHub Actions** from exactly that source, and every asset above carries its SHA-256
checksum. On Linux this message does not exist.

This project has applied to the [SignPath Foundation](https://signpath.org/) for free code
signing for open source projects. Once approved, the Windows binaries below will be signed
by SignPath.

</details>"""


def voriger_tag(tag):
    """Der Tag davor — für „was hat sich seit der letzten Testfassung getan".

    Aus Git, nicht geraten. Fehlt die Historie (flacher Klon), bleibt der Link
    einfach weg; er ist eine Zugabe, kein Muss.
    """
    import subprocess
    try:
        alle = subprocess.run(['git', 'tag', '--sort=-creatordate'],
                              capture_output=True, text=True,
                              timeout=30).stdout.split()
    except Exception:
        return ''
    grund = grundversion(tag)
    for kandidat in alle:
        if kandidat == tag:
            continue
        if grundversion(kandidat) == grund and ist_vorab(kandidat):
            return kandidat.lstrip('v')
    return ''


def main():
    tag = sys.argv[1] if len(sys.argv) > 1 else os.environ.get('GITHUB_REF_NAME', '')
    VORIGER[0] = voriger_tag(tag)
    text = zusammensetzen(abschnitt(DATEIEN['en'], tag),
                          abschnitt(DATEIEN['de'], tag))
    if not text:
        text = ('Was diese Fassung gebracht hat, steht im '
                '[Änderungsprotokoll](../blob/main/CHANGELOG.md).')
    print(vorab_kopf(tag) + text + HINWEIS)


if __name__ == '__main__':
    main()
