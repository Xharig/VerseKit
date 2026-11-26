# -*- coding: utf-8 -*-
"""Prüft `.github/workflows/beta.yml`: Aus dem Beta-Lauf geht nie eine Klartext-Datei hinaus.

Aufruf:  python tools/beta_workflow_pruefen.py              prüft beta.yml
         python tools/beta_workflow_pruefen.py --selbsttest  prüft die Prüfung (Gegenprobe: absichtliche Fehler)

Geprüft wird, ohne YAML-Bibliothek (nur die Schreibweise dieser Datei):
1. jeder `upload-artifact` lädt nur `*.vkbeta` und `*.vkkey` hoch (Artifacts kann jeder angemeldete GitHub-Nutzer laden);
2. die Anhänge des Releases (`files:`) und die Dateien beim Nachprüfen/Nachladen sind nur `*.vkbeta`, `*.vkkey`,
   `SHA256SUMS.txt`;
3. kein Aufruf von `release.yml` (der lädt den Klartext-Installer hoch);
4. `RUST_VERSION` ist dieselbe wie in `release.yml`;
5. jede Aktion ist auf einen Commit festgenagelt;
6. kein `${{ }}` in `run:`; die beiden Geheimnisse der Beta stehen nur in `env:`.
"""
import re
import sys
from pathlib import Path

WURZEL = Path(__file__).resolve().parent.parent
BETA = WURZEL / '.github' / 'workflows' / 'beta.yml'
RELEASE = WURZEL / '.github' / 'workflows' / 'release.yml'
VERSCHLUESSELT = re.compile(r'\.(vkbeta|vkkey)$')
ERLAUBT_ANHANG = re.compile(r'(\.(vkbeta|vkkey)|^SHA256SUMS\.txt)$')
FESTGENAGELT = re.compile(r'^\S+@[0-9a-f]{40}(\s|$)')
GEHEIMNISSE = ('VERSEKIT_BETA_TESTER', 'VERSEKIT_BETA_SCHLUESSEL')


def schritte(text):
    """Die Schritte (`- name:`/`- uses:` auf der Einrückung der Schritte) als Listen von Zeilen mit ihrer Nummer."""
    aktuell, ergebnis = [], []
    for nummer, zeile in enumerate(text.splitlines(), 1):
        if re.match(r'^ {6}- ', zeile):
            if aktuell:
                ergebnis.append(aktuell)
            aktuell = []
        aktuell.append((nummer, zeile))
    if aktuell:
        ergebnis.append(aktuell)
    return ergebnis


def werte(schritt, schluessel):
    """Die Einträge von `schluessel:` in einem Schritt, in einer Zeile oder als Block (`|`): (Nummer, Wert)."""
    ergebnis, i = [], 0
    while i < len(schritt):
        nummer, zeile = schritt[i]
        m = re.match(r'^(\s*)(?:- )?' + re.escape(schluessel) + r':\s*(.*)$', zeile)
        if m:
            rest = m.group(2).strip()
            if rest and rest not in ('|', '>', '|-', '>-'):
                ergebnis.append((nummer, rest))
            else:
                einzug = len(m.group(1))
                i += 1
                while i < len(schritt) and (not schritt[i][1].strip() or len(schritt[i][1]) - len(schritt[i][1].lstrip()) > einzug):
                    if schritt[i][1].strip():
                        ergebnis.append((schritt[i][0], schritt[i][1].strip()))
                    i += 1
                continue
        i += 1
    return ergebnis


def rust_version(text):
    m = re.search(r"^\s*RUST_VERSION:\s*'([^']+)'", text, re.M)
    return m.group(1) if m else None


def pruefe(beta, release):
    """Die Funde als Liste von Texten; leer, wenn alles in Ordnung ist."""
    funde = []
    if re.search(r'^\s*uses:\s*\./\.github/workflows/release\.yml', beta, re.M):
        funde.append('beta.yml ruft release.yml auf (das lädt den Klartext-Installer hoch)')
    if rust_version(beta) is None:
        funde.append('beta.yml hat keine RUST_VERSION')
    elif rust_version(beta) != rust_version(release):
        funde.append('RUST_VERSION in beta.yml (%s) ist nicht die von release.yml (%s)' % (rust_version(beta), rust_version(release)))
    hochgeladen = 0
    for schritt in schritte(beta):
        kopf = '\n'.join(z for _, z in schritt)
        aktion = re.search(r'uses:\s*(\S+)', kopf)
        if aktion and not aktion.group(1).startswith('./') and not FESTGENAGELT.match(aktion.group(1) + ' '):
            funde.append('Zeile %d: %s ist nicht auf einen Commit festgenagelt' % (schritt[0][0], aktion.group(1)))
        if aktion and aktion.group(1).startswith('actions/upload-artifact@'):
            hochgeladen += 1
            pfade = werte(schritt, 'path')
            if not pfade:
                funde.append('Zeile %d: upload-artifact ohne path' % schritt[0][0])
            for nummer, pfad in pfade:
                if not VERSCHLUESSELT.search(pfad):
                    funde.append('Zeile %d: upload-artifact lädt %s hoch (nur *.vkbeta und *.vkkey erlaubt)' % (nummer, pfad))
        if aktion and aktion.group(1).startswith('softprops/action-gh-release@'):
            for nummer, datei in werte(schritt, 'files'):
                if not ERLAUBT_ANHANG.search(datei):
                    funde.append('Zeile %d: Anhang %s ist nicht verschlüsselt' % (nummer, datei))
        for nummer, zeile in schritt:
            # Dateilisten beim Nachprüfen und Nachladen: `files="…"`.
            m = re.match(r'^\s*files="([^"]*)"', zeile)
            if m:
                for datei in m.group(1).split():
                    if not ERLAUBT_ANHANG.search(datei):
                        funde.append('Zeile %d: %s in der Dateiliste ist nicht verschlüsselt' % (nummer, datei))
        # `${{ }}` nur als Wert von `env:`/`with:`/`name:` einer Zeile, nie in einem `run:`-Block.
        for nummer, text in werte(schritt, 'run'):
            if '${{' in text:
                funde.append('Zeile %d: ${{ }} in run: (Werte kommen über env:)' % nummer)
        for nummer, zeile in schritt:
            for name in GEHEIMNISSE:
                if 'secrets.' + name in zeile and not re.match(r'^\s*%s:\s*\$\{\{ secrets\.%s \}\}\s*$' % (name, name), zeile):
                    funde.append('Zeile %d: das Geheimnis %s steht nicht als env:-Wert' % (nummer, name))
    if hochgeladen == 0:
        funde.append('beta.yml lädt gar nichts hoch: die Prüfung wäre leer')
    return funde


def selbsttest(beta, release):
    """Die Gegenprobe: die echte Datei besteht, jeder absichtlich eingebaute Fehler wird gemeldet."""
    probleme = []
    if pruefe(beta, release):
        probleme.append('die unveränderte beta.yml besteht nicht: %s' % pruefe(beta, release))
    # Die Rust-Version, die beta.yml gerade nennt: die Gegenprobe ändert genau sie.
    rust = re.search(r"RUST_VERSION: '[^']*'", beta)
    rust = rust.group(0) if rust else "RUST_VERSION: '?'"
    fehler = {
        'Klartext-Installer als Artifact': ('path: |\n            beta-aus/VerseKit-Setup.exe.vkbeta',
                                           'path: |\n            dist/VerseKit-Setup.exe'),
        'AppImage als Artifact': ('path: beta-aus/VerseKit-x86_64.AppImage.vkbeta', 'path: dist/VerseKit-x86_64.AppImage'),
        'Klartext im Release-Anhang': ('            SHA256SUMS.txt\n', '            SHA256SUMS.txt\n            dist/VerseKit-Setup.exe\n'),
        'Klartext in der Dateiliste': ('files="beta-aus/VerseKit-Beta.vkkey', 'files="dist/VerseKit-Setup.exe beta-aus/VerseKit-Beta.vkkey'),
        'Aufruf von release.yml': ('jobs:\n', 'jobs:\n  fremd:\n    uses: ./.github/workflows/release.yml\n'),
        'andere RUST_VERSION': (rust, "RUST_VERSION: '1.0.0'"),
        'Aktion ohne Commit': ('actions/upload-artifact@b7c566a772e6b6bfb58ed0dc250532a479d7789f', 'actions/upload-artifact@v6'),
        '${{ }} in run:': ('run: echo "::warning title=Build provenance::No build provenance for $TAG',
                           'run: echo "::warning title=${{ env.TAG }}::No build provenance for $TAG'),
        'Geheimnis in run:': ('python .github/scripts/beta_masken.py "$liste"',
                              'python .github/scripts/beta_masken.py "${{ secrets.VERSEKIT_BETA_TESTER }}"'),
    }
    for name, (alt, neu) in fehler.items():
        if alt not in beta:
            probleme.append('Gegenprobe „%s“: die Stelle steht nicht in beta.yml (Selbsttest veraltet)' % name)
        elif not pruefe(beta.replace(alt, neu, 1), release):
            probleme.append('Gegenprobe „%s“ wird NICHT gemeldet' % name)
    return probleme


def main():
    beta, release = BETA.read_text(encoding='utf-8'), RELEASE.read_text(encoding='utf-8')
    if sys.argv[1:] == ['--selbsttest']:
        probleme = selbsttest(beta, release)
        for text in probleme:
            print('FEHLER:', text)
        print('Selbsttest bestanden.' if not probleme else 'Selbsttest NICHT bestanden.')
        return 1 if probleme else 0
    funde = pruefe(beta, release)
    for text in funde:
        print('::error::' + text)
    print('beta.yml: %s' % ('in Ordnung' if not funde else '%d Funde' % len(funde)))
    return 1 if funde else 0


if __name__ == '__main__':
    sys.exit(main())
