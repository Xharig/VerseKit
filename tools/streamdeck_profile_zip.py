# -*- coding: utf-8 -*-
"""Packt das ZIP mit den Stream-Deck-Profilen, das jedes Release trägt: `VerseKit-StreamDeck-Profile.zip`.

Aufruf:  python tools/streamdeck_profile_zip.py [ZIEL]       baut das ZIP (Standard dist/VerseKit-StreamDeck-Profile.zip) und prüft es
         python tools/streamdeck_profile_zip.py --pruefen ZIP  prüft ein fertiges ZIP
         python tools/streamdeck_profile_zip.py --selbsttest   Gegenprobe: jeder absichtlich eingebaute Fehler wird gemeldet

Quelle: die Profile, die auch das Plugin mitbringt (`streamdeck-plugin/profiles`, ein Stand für Plugin und ZIP), dazu
`streamdeck-plugin/zip/` (LIESMICH.md Deutsch, README.md English, LIZENZEN.md). Aufbau wie das Weitergabe-Paket des
Profil-Repos, damit LIESMICH und README stimmen:

    VerseKit-StreamDeck/LIESMICH.md, README.md, LIZENZEN.md
    VerseKit-StreamDeck/Profile/<Deutsch|English>/<Gerät>/VerseKit-StreamDeck-<Gerät>-<DE|EN>.streamDeckProfile

Das ZIP ist bei gleichen Quellen Byte für Byte gleich (feste Zeitstempel, feste Reihenfolge). Geprüft wird: genau diese
15 Dateien; jedes Profil ein lesbares ZIP mit genau einem `<UUID>.sdProfile/manifest.json`, dem Gerätemodell seines
Namens, jeder Seite aus `Pages` vorhanden; nur die deutschen 8 × 4-Profile mit dem Standardpfad von StarCitizen.exe
verknüpft; kein privater Pfad und kein Beta-Wort in den Texten; höchstens 8 MiB je Profil.
"""
import io
import json
import re
import sys
import zipfile
from pathlib import Path

WURZEL = Path(__file__).resolve().parent.parent
PROFILE = WURZEL / 'streamdeck-plugin' / 'profiles'
TEXTE = WURZEL / 'streamdeck-plugin' / 'zip'
NAME = 'VerseKit-StreamDeck-Profile.zip'
OBEN = 'VerseKit-StreamDeck'
TEXTDATEIEN = ('LIESMICH.md', 'README.md', 'LIZENZEN.md')
SPRACHE = {'DE': 'Deutsch', 'EN': 'English'}
# Gerät im Dateinamen: (Ordner Deutsch, Ordner English, Modell in manifest.json)
GERAETE = {
    'XL': ('Stream Deck XL', 'Stream Deck XL', '20GAT9901'),
    'PlusXL': ('Stream Deck + XL', 'Stream Deck + XL', '20GBX9901'),
    'Mobile': ('Stream Deck Mobile (iPad, Handy)', 'Stream Deck Mobile (iPad, phone)', 'VSD2/WiFi'),
    'MK2': ('Stream Deck MK.2', 'Stream Deck MK.2', '20GBA9901'),
    'Neo': ('Stream Deck Neo', 'Stream Deck Neo', '20GBJ9901'),
    'Mini': ('Stream Deck Mini', 'Stream Deck Mini', '20GAI9901'),
}
MIT_SPIEL = {'XL', 'PlusXL', 'Mobile'}   # nur deren deutsche Profile öffnen sich mit dem Spiel (LIESMICH, Einrichten 6.)
STANDARDPFAD = 'C:\\Program Files\\Roberts Space Industries\\StarCitizen\\LIVE\\Bin64\\StarCitizen.exe'
MAX = 8 * 1024 * 1024
ZEIT = (2026, 1, 1, 0, 0, 0)
PRIVAT = re.compile(r'(?i)[a-z]:[\\/]+users[\\/]|/home/|E:[\\/]+(backups|github|obsidian)')
BETA = re.compile(r'(?i)\bbeta\b|testprofil|test profile')


def dateiname(geraet, sprache):
    return f'VerseKit-StreamDeck-{geraet}-{sprache}.streamDeckProfile'


def soll():
    """{Pfad im ZIP: Quelldatei} in der festen Reihenfolge des ZIPs."""
    ergebnis = {f'{OBEN}/{t}': TEXTE / t for t in TEXTDATEIEN}
    for sprache in ('DE', 'EN'):
        for geraet, (de, en, _) in GERAETE.items():
            ordner = de if sprache == 'DE' else en
            ergebnis[f'{OBEN}/Profile/{SPRACHE[sprache]}/{ordner}/{dateiname(geraet, sprache)}'] = PROFILE / dateiname(geraet, sprache)
    return ergebnis


def bauen(ziel):
    ziel.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(ziel, 'w', zipfile.ZIP_DEFLATED) as z:
        for pfad, quelle in soll().items():
            info = zipfile.ZipInfo(pfad, ZEIT)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            z.writestr(info, quelle.read_bytes())


def profil_funde(name, daten):
    """Funde in einer Profildatei (Bytes)."""
    funde = []
    m = re.fullmatch(r'VerseKit-StreamDeck-(\w+)-(DE|EN)\.streamDeckProfile', name)
    if not m or m.group(1) not in GERAETE:
        return [f'{name}: unbekannter Dateiname']
    geraet, sprache = m.group(1), m.group(2)
    if len(daten) > MAX:
        funde.append(f'{name}: größer als 8 MiB')
    try:
        z = zipfile.ZipFile(io.BytesIO(daten))
        namen = z.namelist()
    except zipfile.BadZipFile:
        return funde + [f'{name}: kein lesbares ZIP']
    kopf = [n for n in namen if re.fullmatch(r'[0-9A-Fa-f-]{36}\.sdProfile/manifest\.json', n)]
    if len(kopf) != 1:
        return funde + [f'{name}: {len(kopf)} Profil-Manifeste statt einem']
    wurzel = kopf[0].split('/')[0]
    try:
        man = json.loads(z.read(kopf[0]))
    except ValueError:
        return funde + [f'{name}: manifest.json ist kein JSON']
    if (man.get('Device') or {}).get('Model') != GERAETE[geraet][2]:
        funde.append(f'{name}: Gerätemodell {(man.get("Device") or {}).get("Model")} statt {GERAETE[geraet][2]}')
    seiten = (man.get('Pages') or {}).get('Pages') or []
    if not seiten:
        funde.append(f'{name}: keine Seiten')
    vorhanden = {n.lower() for n in namen}
    for seite in seiten:
        if f'{wurzel}/Profiles/{seite}/manifest.json'.lower() not in vorhanden:
            funde.append(f'{name}: Seite {seite} fehlt')
    verknuepft = man.get('AppIdentifier')
    erwartet = STANDARDPFAD if (sprache == 'DE' and geraet in MIT_SPIEL) else None
    if verknuepft != erwartet:
        funde.append(f'{name}: verknüpft mit {verknuepft!r} statt {erwartet!r}')
    for n in namen:
        if n.endswith('.json') and PRIVAT.search(z.read(n).decode('utf-8', 'replace')):
            funde.append(f'{name}: privater Pfad in {n}')
    return funde


def pruefen_daten(dateien):
    """Funde für {Pfad im ZIP: Bytes}; leer, wenn alles in Ordnung ist."""
    funde = []
    erwartet = list(soll())
    if sorted(dateien) != sorted(erwartet):
        fehlt = sorted(set(erwartet) - set(dateien))
        zuviel = sorted(set(dateien) - set(erwartet))
        funde.append(f'Inhalt weicht ab: fehlt {fehlt}, zu viel {zuviel}')
    for pfad, daten in dateien.items():
        if pfad.endswith('.md'):
            text = daten.decode('utf-8', 'replace')
            if PRIVAT.search(text):
                funde.append(f'{pfad}: privater Pfad')
            beta = BETA.search(text)
            if beta:
                funde.append(f'{pfad}: Beta- oder Test-Wort ({beta.group(0)})')
            if not text.strip():
                funde.append(f'{pfad}: leer')
        elif pfad.endswith('.streamDeckProfile'):
            funde += profil_funde(pfad.rsplit('/', 1)[1], daten)
    return funde


def lesen(zip_pfad):
    with zipfile.ZipFile(zip_pfad) as z:
        return {n: z.read(n) for n in z.namelist() if not n.endswith('/')}


def selbsttest():
    """Gegenprobe: das echte ZIP besteht, jede absichtliche Verfälschung wird gemeldet."""
    probleme = []
    echt = {pfad: quelle.read_bytes() for pfad, quelle in soll().items()}
    if pruefen_daten(echt):
        probleme.append(f'die echten Dateien bestehen nicht: {pruefen_daten(echt)}')
    mk2 = f'{OBEN}/Profile/Deutsch/Stream Deck MK.2/{dateiname("MK2", "DE")}'
    xl_en = f'{OBEN}/Profile/English/Stream Deck XL/{dateiname("XL", "EN")}'

    def profil_umbauen(daten, aendern):
        quelle = zipfile.ZipFile(io.BytesIO(daten))
        aus = io.BytesIO()
        with zipfile.ZipFile(aus, 'w') as z:
            for n in quelle.namelist():
                inhalt = quelle.read(n)
                if re.fullmatch(r'[0-9A-Fa-f-]{36}\.sdProfile/manifest\.json', n):
                    man = json.loads(inhalt)
                    aendern(man)
                    inhalt = json.dumps(man).encode()
                z.writestr(n, inhalt)
        return aus.getvalue()

    def ohne(pfad):
        return {k: v for k, v in echt.items() if k != pfad}

    def mit(pfad, daten):
        return {**echt, pfad: daten}

    faelle = {
        'ein Profil fehlt': ohne(mk2),
        'LIESMICH fehlt': ohne(f'{OBEN}/LIESMICH.md'),
        'fremde Datei': mit(f'{OBEN}/Profile/notiz.txt', b'x'),
        'Profil kein ZIP': mit(mk2, b'kein zip'),
        'falsches Gerätemodell': mit(mk2, profil_umbauen(echt[mk2], lambda m: m['Device'].update({'Model': '20GAT9901'}))),
        'Seite fehlt': mit(mk2, profil_umbauen(echt[mk2], lambda m: m['Pages']['Pages'].append('00000000-0000-0000-0000-000000000000'))),
        'EN mit dem Spiel verknüpft': mit(xl_en, profil_umbauen(echt[xl_en], lambda m: m.update({'AppIdentifier': STANDARDPFAD}))),
        'privater Pfad im Profil': mit(mk2, profil_umbauen(echt[mk2], lambda m: m.update({'Name': 'C:\\Users\\jemand\\x'}))),
        'Beta-Wort im README': mit(f'{OBEN}/README.md', echt[f'{OBEN}/README.md'] + b'\nThese are test profiles.\n'),
        'privater Pfad in LIESMICH': mit(f'{OBEN}/LIESMICH.md', echt[f'{OBEN}/LIESMICH.md'] + b'\nE:\\Backups\\x\n'),
        'Profil zu groß': mit(mk2, echt[mk2] + b'\0' * MAX),
    }
    for name, dateien in faelle.items():
        if not pruefen_daten(dateien):
            probleme.append(f'Gegenprobe „{name}“ wird NICHT gemeldet')
    return probleme


def ausgabe(funde, was):
    for text in funde:
        print('::error::' + text)
    print(f'{was}: ' + ('in Ordnung' if not funde else f'{len(funde)} Funde'))
    return 1 if funde else 0


def main():
    sys.stdout.reconfigure(encoding='utf-8')  # type: ignore[attr-defined]
    argumente = sys.argv[1:]
    if argumente == ['--selbsttest']:
        probleme = selbsttest()
        for text in probleme:
            print('FEHLER:', text)
        print('Selbsttest bestanden.' if not probleme else 'Selbsttest NICHT bestanden.')
        return 1 if probleme else 0
    if argumente[:1] == ['--pruefen'] and len(argumente) == 2:
        return ausgabe(pruefen_daten(lesen(argumente[1])), argumente[1])
    if len(argumente) > 1 or (argumente and argumente[0].startswith('--')):
        print(__doc__)
        return 2
    ziel = Path(argumente[0]) if argumente else WURZEL / 'dist' / NAME
    bauen(ziel)
    dateien = lesen(ziel)
    for pfad, daten in dateien.items():
        print(f'{len(daten):>9}  {pfad}')
    print(f'{ziel.stat().st_size} Bytes: {ziel}')
    return ausgabe(pruefen_daten(dateien), ziel.name)


if __name__ == '__main__':
    sys.exit(main())
