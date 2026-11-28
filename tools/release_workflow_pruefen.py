# -*- coding: utf-8 -*-
"""Prüft `.github/workflows/release.yml`: Jedes Release trägt das ZIP mit den Stream-Deck-Profilen.

Aufruf:  python tools/release_workflow_pruefen.py              prüft release.yml
         python tools/release_workflow_pruefen.py --selbsttest  prüft die Prüfung (Gegenprobe: absichtliche Fehler)

Geprüft wird, ohne YAML-Bibliothek (nur die Schreibweise dieser Datei), im Job `release`:
1. ein Schritt baut das ZIP mit `tools/streamdeck_profile_zip.py` an seinen festen Ort, vor den Prüfsummen;
2. `SHA256SUMS.txt` bekommt eine Zeile für das ZIP (bloßer Dateiname) und die Zeilenzahl-Probe zählt alle Anhänge;
3. das ZIP steht in der Herkunft (`subject-path`), in den Anhängen (`files:`) und in der Liste beim Nachprüfen
   (`files="…"`), jeweils neben Setup, AppImage und `SHA256SUMS.txt`.
"""
import re
import sys
from pathlib import Path

WURZEL = Path(__file__).resolve().parent.parent
RELEASE = WURZEL / '.github' / 'workflows' / 'release.yml'
ZIP = 'VerseKit-StreamDeck-Profile.zip'
ORT = 'profile-zip/' + ZIP
SKRIPT = 'tools/streamdeck_profile_zip.py'
ANHAENGE = ('dateien/windows/VerseKit-Setup.exe', 'dateien/linux/VerseKit-x86_64.AppImage', ORT, 'SHA256SUMS.txt')


def job_release(text):
    """Der Text des Jobs `release` (ab `  release:` bis zum nächsten Job oder Dateiende)."""
    m = re.search(r'^  release:\n(.*?)(?=^  \S[^\n]*:\n|\Z)', text, re.M | re.S)
    return m.group(1) if m else ''


def block(text, schluessel):
    """Die Zeilen unter `schluessel: |` (eingerückter Block), ohne Leerzeichen am Rand."""
    m = re.search(r'^(\s*)' + re.escape(schluessel) + r':\s*\|\s*\n((?:\1\s+\S.*\n)+)', text, re.M)
    return [z.strip() for z in m.group(2).splitlines()] if m else []


def pruefe(text):
    """Die Funde als Liste von Texten; leer, wenn alles in Ordnung ist."""
    funde = []
    job = job_release(text)
    if not job:
        return ['release.yml hat keinen Job release']
    bau = re.search(r'^\s*run:\s*python3? ' + re.escape(SKRIPT) + r' ' + re.escape(ORT) + r'\s*$', job, re.M)
    if not bau:
        funde.append(f'kein Schritt baut {ORT} mit {SKRIPT}')
    summen = re.search(r'^\s*- name: Checksums\n(.*?)(?=^\s*- (?:name|uses):|\Z)', job, re.M | re.S)
    if not summen:
        funde.append('kein Schritt Checksums')
    else:
        s = summen.group(1)
        if not re.search(r'\(\s*cd profile-zip && sha256sum ' + re.escape(ZIP) + r'\s*\)\s*>>\s*SHA256SUMS\.txt', s):
            funde.append(f'SHA256SUMS.txt bekommt keine Zeile für {ZIP}')
        zahl = re.search(r'wc -l < SHA256SUMS\.txt\)"\s*=\s*"(\d+)"', s)
        zeilen = len(re.findall(r'sha256sum [^)]+\)\s*>>?\s*SHA256SUMS\.txt', s))
        if not zahl or int(zahl.group(1)) != zeilen:
            funde.append(f'die Zeilenzahl-Probe von SHA256SUMS.txt passt nicht zu den {zeilen} Prüfsummen')
        if bau and bau.start() > summen.start():
            funde.append('das ZIP wird erst nach den Prüfsummen gebaut')
    for schluessel, name in (('subject-path', 'Herkunft (subject-path)'), ('files', 'Anhänge (files:)')):
        liste = block(job, schluessel)
        for datei in ANHAENGE:
            if datei not in liste:
                funde.append(f'{datei} fehlt in {name}')
    nach = re.search(r'^\s*files="([^"]*)"', job, re.M)
    for datei in ANHAENGE:
        if not nach or datei not in nach.group(1).split():
            funde.append(f'{datei} fehlt in der Liste beim Nachprüfen (files="…")')
    return funde


def selbsttest(text):
    """Die Gegenprobe: die echte Datei besteht, jeder absichtlich eingebaute Fehler wird gemeldet."""
    probleme = []
    if pruefe(text):
        probleme.append('die unveränderte release.yml besteht nicht: %s' % pruefe(text))
    fehler = {
        'ZIP-Schritt fehlt': (f'        run: python3 {SKRIPT} {ORT}\n', ''),
        'ZIP an anderem Ort': (f'run: python3 {SKRIPT} {ORT}', f'run: python3 {SKRIPT} dist/{ZIP}'),
        'keine Prüfsumme des ZIPs': (f'          ( cd profile-zip && sha256sum {ZIP} ) >> SHA256SUMS.txt\n', ''),
        'Zeilenzahl alt': ('test "$(wc -l < SHA256SUMS.txt)" = "3"', 'test "$(wc -l < SHA256SUMS.txt)" = "2"'),
        'ZIP nicht in der Herkunft': (f'            {ORT}\n            SHA256SUMS.txt\n\n      - name: Build provenance failed',
                                      '            SHA256SUMS.txt\n\n      - name: Build provenance failed'),
        'ZIP nicht angehängt': (f'            {ORT}\n            SHA256SUMS.txt\n\n      # Every attachment',
                                '            SHA256SUMS.txt\n\n      # Every attachment'),
        'ZIP nicht nachgeprüft': (f' {ORT} SHA256SUMS.txt"', ' SHA256SUMS.txt"'),
    }
    for name, (alt, neu) in fehler.items():
        if text.count(alt) != 1:
            probleme.append('Gegenprobe „%s“: die Stelle steht nicht genau einmal in release.yml (Selbsttest veraltet)' % name)
        elif not pruefe(text.replace(alt, neu, 1)):
            probleme.append('Gegenprobe „%s“ wird NICHT gemeldet' % name)
    return probleme


def main():
    sys.stdout.reconfigure(encoding='utf-8')  # type: ignore[attr-defined]
    text = RELEASE.read_text(encoding='utf-8')
    if sys.argv[1:] == ['--selbsttest']:
        probleme = selbsttest(text)
        for t in probleme:
            print('FEHLER:', t)
        print('Selbsttest bestanden.' if not probleme else 'Selbsttest NICHT bestanden.')
        return 1 if probleme else 0
    funde = pruefe(text)
    for t in funde:
        print('::error::' + t)
    print('release.yml: %s' % ('in Ordnung' if not funde else '%d Funde' % len(funde)))
    return 1 if funde else 0


if __name__ == '__main__':
    sys.exit(main())
