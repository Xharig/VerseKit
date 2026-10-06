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
Die Bildschirmfotos für die Anleitung machen — **ohne** den Bildschirm zu belegen.

## Wozu

Jedes Bild der Anleitung entsteht auf Knopfdruck, in derselben Auflösung und
mit dem aktuellen Stand der Oberfläche — statt je Bild Fenster aufziehen, Seite
anklicken, fotografieren, zuschneiden, benennen.

## ⚠⚠ Es reisst den Bildschirm NICHT an sich

Das ist die eigentliche Schwierigkeit. Ein Bildschirmfoto braucht normalerweise
ein sichtbares Fenster im Vordergrund — und genau das ist hier verboten: Wer
gerade Star Citizen fliegt, landet sonst mitten im Kampf auf dem Desktop (siehe
`tools/unsichtbar.py`).

Der Ausweg ist `PrintWindow` aus der Windows-API: Es lässt ein Fenster **sich
selbst neu zeichnen**, in einen Speicherbereich statt auf den Schirm. Mit dem
Kennzeichen `PW_RENDERFULLCONTENT` (2) gilt das auch für Fenster, die niemand
sieht. Das Fenster wird deshalb weit ausserhalb des sichtbaren Bereichs
aufgebaut und nie nach vorn geholt.

⚠ **`SetProcessDpiAwareness` muss VOR dem ersten Tk-Fenster stehen.** Ohne das
rechnet Windows die Angaben um, und man greift am Fenster vorbei — bei 125 %
Skalierung fehlt rechts und unten ein Fünftel.

## Womit gearbeitet wird

Mit einer **Kopie** des echten Datenstands, nicht mit ihm selbst. Die Bilder
sollen gefüllte Listen zeigen — ein leeres Lager erklärt niemandem, wozu die
Seite gut ist. Aber ein Werkzeug, das für ein Bild den eigenen Bestand anfasst,
ist ein Werkzeug zu viel: Beim Start schreibt der Watcher Lesestand,
Katalog-Zwischenspeicher und Einstellungen.

## Aufruf

    python tools/bilder_machen.py             # alle Seiten, deutsch
    python tools/bilder_machen.py --en        # alle Seiten, englisch
    python tools/bilder_machen.py liste lager # nur diese beiden

Die Bilder landen in `assets/` unter `screenshot-<seite>.png`; auf Englisch
hängt `-en` an. Vorhandene werden überschrieben.
"""
import ctypes
import os
import shutil
import sys
import tempfile
import time

# ⛔ Vor der ersten Ausgabe: Die Windows-Konsole kann kein `⚠` — siehe ausgabe.py.
import ausgabe                                                 # noqa: E402
ausgabe.utf8()

HIER = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, HIER)

# ⚠⚠ **Vor jedem Tk-Import und vor jedem Fenster.** Siehe oben.
if sys.platform == 'win32':
    try:
        ctypes.windll.shcore.SetProcessDpiAwareness(2)      # PROCESS_PER_MONITOR_DPI_AWARE
    except Exception:
        try:
            ctypes.windll.user32.SetProcessDPIAware()
        except Exception:
            pass

# Fenstergrösse der Bilder. Dieselbe für **alle** — zwei Auflösungen im selben
# Dokument sehen nach Zufall aus, und die Anleitung ist die Visitenkarte.
#
# ⚠ **Bewusst über der Mindestbreite (1160).** Bei genau der Mindestbreite
# wächst die Seitenleiste beim Seitenwechsel noch einmal nach, und der Abgriff
# erwischt den Moment dazwischen: Auf dem Bild fehlen dann rechts Knöpfe und ein
# Stück der Kopfzeile. Gemessen ragt bei 1160 zwar nichts über den Rand — aber
# ein Bild, das in dieser Sekunde entsteht, zeigt es trotzdem. Mit Luft passiert das nicht, und die
# Bilder zeigen nebenbei mehr Inhalt.
BREITE, HOEHE = 1400, 860

# Weit weg vom sichtbaren Bereich. Nicht „hinter" anderen Fenstern: dort würde
# jemand es beim Umschalten sehen.
WEIT_WEG = (9000, 9000)

# ⚠ Unter Linux ist der ganze Schirm virtuell (`xvfb-run`), dort gibt es kein
# „ausserhalb" — und ein Fenster bei 9000,9000 waere schlicht nicht abgreifbar.
# Es steht deshalb bei 0,0; gesehen wird es trotzdem von niemandem.
if sys.platform != 'win32':
    WEIT_WEG = (0, 0)

# ⚠⚠ Auf dem eigenen, unsichtbaren Desktop (`eigener_desktop.py`) steht das
# Fenster ebenfalls bei 0,0. Ausserhalb des Bildschirms bekommt es von Windows
# keine Zeichen-Auftraege: Was nach dem Rollen neu sichtbar wird, zeichnet
# niemand, und `PrintWindow` liefert die alten Pixel an der neuen Stelle —
# Seitenleiste und Hangar standen so doppelt uebereinander.
if sys.platform == 'win32' and os.environ.get('SC_BP_UNSICHTBAR'):
    WEIT_WEG = (0, 0)

# Welche Seite unter welchem Namen abgelegt wird. Die Kennungen sind die aus
# `scbp/pages.py`.
SEITEN = {
    'liste':        'screenshot-liste',
    'fortschritt':  'screenshot-fortschritt',
    'auftragslog':  'screenshot-auftragslog',
    'laeden':       'screenshot-laeden',
    # ⚠ Diese Seite zeigt **erfundene** Daten, nicht den kopierten Stand —
    # siehe `beispiel_hangar()`. Der echte Hangar verriete Pledge-Pakete.
    'hangar':       'screenshot-hangar',
    # ⚠ Dieselbe erfundene Ablage wie beim Hangar — auch hier stünden sonst
    # die echten Schiffe und Wunschzettel-Einträge der Kopie im Bild.
    'wunschliste':  'screenshot-wunschliste',
    'einkaufsliste': 'screenshot-einkaufsliste',
    'farmliste':    'screenshot-farmliste',
    'bergung':      'screenshot-bergung',
    'zerlegen':     'screenshot-zerlegen',
    'blickwinkel':  'screenshot-blickwinkel',
    'herstellung':  'screenshot-herstellung',
    'bergbau':      'screenshot-bergbau',
    'lager':        'screenshot-lager',
    'handelslager': 'screenshot-handelslager',
    'verkauf':      'screenshot-verkauf',
    'bestand':      'screenshot-bestand',
    'allgemein':    'screenshot-allgemein',
    'anzeige':      'screenshot-anzeige',
    'spiel':        'screenshot-auftragstexte',
    'serverstatus': 'screenshot-serverstatus',
    'wasistneu':    'screenshot-wasistneu',
    'ueber':        'screenshot-ueber',
    'danke':        'screenshot-danke',
    # ⭐ Jede Seite der Reiterleiste bekommt ein Bild.
    'asop':         'screenshot-schiffe-benennen',
    'raffinerien':  'screenshot-raffinerien',
    'routen':       'screenshot-routen',
    'patchaenderungen': 'screenshot-patchaenderungen',
    # ⚠ Erfundene Sitzungen, siehe `beispiel_statistik()` — die echten verrieten,
    # wann gespielt wird.
    'statistik':    'screenshot-statistik',
    'darstellung':  'screenshot-darstellung',
    # ⭐⭐ **Wirklich JEDER Reiter** — die Webseite zeigt das Werkzeug mit
    # allen Original-Reitern. Seiten, die sonst Echtes zeigten, bekommen
    # erfundene Daten; siehe `beispiel_daten`.
    'statistik_auswertung': 'screenshot-statistik-auswertung',
    'statistik_schiffe':    'screenshot-statistik-schiffe',
    'statistik_auftraege':  'screenshot-statistik-auftraege',
    'statistik_quantum':    'screenshot-statistik-quantum',
    'statistik_stabil':     'screenshot-statistik-stabil',
    'ordner':       'screenshot-ordner',
    'joysticks':    'screenshot-joysticks',
    'achsen':       'screenshot-achsen',
    'module':       'screenshot-module',
    'basetool':     'screenshot-basetool',
    'uebersetzung': 'screenshot-uebersetzung',
    'erkennung':    'screenshot-erkennung',
    'startprogramme': 'screenshot-startprogramme',
    'diagnose':     'screenshot-diagnose',
}

# ⚠⚠ **Das Overlay ist keine Seite.** Es ist ein eigenes Fenster einer eigenen
# Klasse und braucht deshalb eine eigene Tk-Instanz — die Seiten teilen sich
# eine, und zwei `tk.Tk()` in einem Prozess vertraegt Tk nicht verlaesslich
# (siehe den Kommentar in `Overlay.__init__`, ein Tester bekam dadurch
# reproduzierbar SIGSEGV). Es laeuft deshalb in einem EIGENEN PROZESS.
#
# Genau weil es nicht in diese Tabelle passte, fiel es aus dem Werkzeug heraus
# und blieb als einziges von Hand gemacht — mit dem Ergebnis, dass sein Bild
# eineinhalb Wochen alt war und eine Fassung von vor 18 Versionen zeigte.
OVERLAY_NAME = 'screenshot-overlay'


def io_lesen(pfad):
    with open(pfad, encoding='utf-8') as f:
        return f.read()


def _version():
    """Die echte Versionsnummer — sie steht auf dem Bild neben dem Namen.

    Gelesen statt importiert: `sc_bp_watcher` zu importieren zoege den ganzen
    Watcher samt Ueberwachungs-Thread hoch.
    """
    import re
    treffer = re.search(r"__version__ = '([^']+)'",
                        io_lesen(os.path.join(HIER, 'sc_bp_watcher.py')))
    return treffer.group(1) if treffer else ''


def datenstand_kopieren():
    """Eine Wegwerf-Kopie des echten Datenstands anlegen und `SC_BP_HOME` setzen.

    Gibt den Pfad zurück. Der echte Ordner wird **nur gelesen**.
    """
    from scbp import paths
    # ⚠⚠ **`app_folder()`, nicht `app_datei('')`.** Der zweite Weg landet im
    # Unterordner „Intern" — dort liegen Zwischenspeicher, aber weder Bestand
    # (`Bauplaene/`) noch Einstellungen (`Einstellungen/`). Die Bilder zeigten
    # dann einen leeren Bestand (0 %): ein Werkzeug, das aussieht, als könne
    # es nichts.
    #
    # ⚠ Mit `SC_BP_HOME` legt der Watcher alles **flach** ab (siehe
    # `app_file`) — die Unterordner der Vorlage werden deshalb eingeebnet.
    quelle = paths.app_folder()
    # ⚠⚠ **Nicht unter %TEMP%, sondern in einem Ordner ohne Benutzernamen.**
    # Der Pfad steht auf der Seite „Update & Über" im Bild — unter Windows
    # hiesse er `C:\Users\<name>\AppData\Local\Temp\…`, und die Bilder sind
    # oeffentlich. Unter Linux ist es ohnehin `/tmp/…`.
    basis = None
    if sys.platform == 'win32':
        try:
            basis = os.path.join(os.environ.get('SystemDrive', 'C:') + os.sep,
                                 'sc-bp-bilder')
            os.makedirs(basis, exist_ok=True)
        except OSError:
            basis = None
    ziel = tempfile.mkdtemp(prefix='sc-bp-bilder-', dir=basis)
    # Die Kopie enthaelt den echten Datenstand — nach dem Lauf wegraeumen.
    import atexit
    atexit.register(shutil.rmtree, ziel, True)
    for wurzel_, _unter, dateien in os.walk(quelle):
        for name in dateien:
            if not name.endswith(('.json', '.txt')):
                continue
            try:
                shutil.copy2(os.path.join(wurzel_, name),
                             os.path.join(ziel, name))
            except Exception:
                pass
    _gefaehrliches_abschalten(ziel)
    # ⚠⚠ Hier setzen, nicht dem Aufrufer überlassen: Wer die Kopie anlegt und
    # das Setzen vergisst, schreibt mit dem nächsten `save()` in die echten
    # Daten.
    os.environ['SC_BP_HOME'] = ziel
    return ziel


def _gefaehrliches_abschalten(ordner):
    """In der Kopie alles ausschalten, was ausserhalb der Kopie wirkt.

    ⚠⚠ **Die Kopie schuetzt die eigenen Daten, nicht das Spiel.** `SC_BP_HOME`
    lenkt Bestand, Einstellungen und Zwischenspeicher in den Wegwerf-Ordner —
    die `global.ini` von Star Citizen liegt aber woanders, und `inj_auto`
    (Schalter „Selbst aktuell halten") schreibt beim Start hinein. Ein Werkzeug, das
    fuer ein Bildschirmfoto die Spieldateien anfasst, ist ein Werkzeug zu viel.

    ⚠ Ebenso der Autostart: Er traegt sich in Registry bzw. `.desktop` ein,
    beides ausserhalb jeder Kopie.
    """
    import json
    pfad = os.path.join(ordner, 'einstellungen.json')
    try:
        with open(pfad, encoding='utf-8') as datei:
            daten = json.load(datei)
    except Exception:
        daten = {}
    if not isinstance(daten, dict):
        daten = {}
    daten['inj_auto'] = False
    daten['autostart'] = False
    # Für die Bilder: kein Name im Fehlerbericht, und ein Beispiel-Eintrag
    # unter den Startprogrammen statt einer leeren Liste. Er
    # startet nichts — Startprogramme laufen nur beim Klick auf den Launcher.
    daten['melder_name'] = ''
    # ⚠⚠ **Immer `original` — schon hier, beim Kopieren.** Das Overlay-Bild
    # entsteht in einem EIGENEN Prozess (`--nur-overlay`), der `main()` nicht
    # erreicht; stünde das Schema erst dort, zeigte das Overlay das Schema der
    # Kopie statt der Markenfarben. `--schema=` überschreibt das danach für
    # die Seite Darstellung.
    daten['farbschema'] = 'original'
    # Ebenso die Größe — fest, nicht die der Kopie. ⚠ `sehrgross`: Die
    # Webseite zeigt die Bilder auf rund 800 px verkleinert; mit der
    # Grundstufe (100 %) ist die Schrift dort nicht mehr lesbar.
    daten['schriftgroesse'] = 'sehrgross'
    daten['startprogramme_an'] = True
    daten['startprogramme'] = [{
        'an': True, 'name': 'VoiceAttack',
        'datei': 'C:\\Program Files\\VoiceAttack\\VoiceAttack.exe',
        'argumente': '', 'wann': 'launcher', 'warten': 0, 'beenden': True}]
    try:
        with open(pfad, 'w', encoding='utf-8', newline='\n') as datei:
            json.dump(daten, datei, ensure_ascii=False, indent=1)
    except Exception:
        pass


def beispiel_daten():
    """Erfundene Beispieldaten für JEDE Seite, die sonst leer bliebe.

    ⭐ Jedes Fenster bekommt Beispieldaten. Eine leere Seite erklärt
    niemandem, wozu sie gut ist.

    ⚠⚠ **Immer, nicht nur wenn die Hangar-Seite dran ist.** Auch andere
    Seiten (`wunschliste`) lesen den Hangar — ohne Beispiel-Hangar käme der
    ECHTE aus der Kopie ins Bild. Ebenso die eigenen Schiffsnamen
    (`asop.json`) auf der Seite zum Schiffe-Benennen.

    Die Schiffe sind so gewählt, dass ihre Steckplätze schon im kopierten
    erkul-Zwischenspeicher liegen — dann braucht es kein Netz, und die Seite
    der fehlenden Teile hat echte Plätze, in die Teile gelegt werden können.
    """
    import json
    from scbp import fleet, cart, erkul, trade_cargo
    heim = os.environ['SC_BP_HOME']

    def schiff(name, hersteller, kurz, hkurz, herkunft, **mehr):
        eintrag = {'name': name, 'hersteller': hersteller, 'kurz': kurz,
                   'hkurz': hkurz, 'herkunft': herkunft, 'belegung': {}}
        eintrag.update(mehr)
        return eintrag

    schiffe = [
        schiff('Arrow', 'Anvil Aerospace', 'ANVL_Arrow', 'ANVL', 'pledge', lti=True),
        schiff('Gladius', 'Aegis Dynamics', 'AEGS_Gladius', 'AEGS', 'ingame'),
        schiff('Asgard', 'Anvil Aerospace', 'ANVL_Asgard', 'ANVL', 'pledge'),
        schiff('C8R Pisces Rescue', 'Anvil Aerospace', 'ANVL_C8R_Pisces', 'ANVL',
               'ingame'),
    ]
    wunsch = [
        schiff('Eclipse', 'Aegis Dynamics', 'AEGS_Eclipse', 'AEGS', 'ingame'),
        schiff('Dragonfly', 'Drake Interplanetary', 'DRAK_Dragonfly', 'DRAK',
               'ingame'),
    ]

    # Ein paar Teile in die Plätze legen — gekauft und selbst gebaut, damit
    # die Seiten der fehlenden und zu farmenden Teile etwas zu rechnen haben.
    erkul_schiffe = (erkul.load().get('schiffe') or {})
    for eintrag, schluessel, wie in ((schiffe[1], 'aegsgladius', cart.BUY),
                                     (schiffe[2], 'anvlasgard', cart.CRAFT),
                                     (wunsch[0], 'aegseclipse', cart.BUY)):
        slots = (erkul_schiffe.get(schluessel) or {}).get('slots') or []
        gesetzt = set()
        for platz in slots:
            art = platz.get('art')
            if art not in ('Cooler', 'Shield', 'PowerPlant') or art in gesetzt:
                continue
            werk = (platz.get('werk') or {}).get('name')
            moeglich = [m for m in cart.choices(art, platz.get('groesse'))
                        if m.get('name') != werk
                        and (wie != cart.CRAFT or m.get('herkunft') != cart.BUYABLE)]
            if moeglich:
                m = moeglich[0]
                cart.set_part(eintrag, platz['pfad'], m['kennung'], m['name'], wie)
                gesetzt.add(art)

    daten = {'format': fleet.FORMAT, 'schiffe': schiffe, 'wunsch': wunsch,
             'merkzettel': [
                 {'name': 'Aves Helmet', 'ref': '', 'anzahl': 1, 'weg': 'bauen'},
                 {'name': 'FS-9 Magazine (75 cap)', 'ref': '', 'anzahl': 3,
                  'weg': 'bauen'}]}
    with open(os.path.join(heim, fleet.FILE), 'w', encoding='utf-8') as f:
        json.dump(daten, f, ensure_ascii=False, indent=1)

    # Eigene Schiffsnamen — erfunden, nicht die aus der Kopie.
    with open(os.path.join(heim, 'asop.json'), 'w', encoding='utf-8') as f:
        from scbp import asop
        json.dump({'format': asop.FORMAT, 'namen': {
            'vehicle_NameANVL_Arrow': {'name': 'Kestrel', 'stern': True},
            'vehicle_NameANVL_Asgard': {'name': 'Longhaul', 'stern': False}}},
            f, ensure_ascii=False, indent=1)

    # Ladung im Handelslager — Waren, die die kopierten Preisdaten kennen.
    # ⚠ Die Schlüssel in `waren` sind kleingeschrieben — angezeigt und
    # zugeordnet wird über den `name` darin (`Agricium`, nicht `agricium`).
    try:
        with open(os.path.join(heim, 'preise.json'), encoding='utf-8') as f:
            waren = json.load(f).get('waren') or {}
    except Exception:
        waren = {}
    posten = []
    for ware, menge, ort in (('Agricium', 24, 'Area 18'), ('Laranite', 12, 'Orison'),
                             ('Medical Supplies', 8, 'Lorville')):
        formen = waren.get(ware.lower()) or []
        if any((x.get('name') or '') == ware for x in formen):
            posten.append({'ware': ware, 'menge': float(menge), 'ort': ort,
                           'gestohlen': False})
    with open(os.path.join(heim, trade_cargo.FILE), 'w', encoding='utf-8') as f:
        json.dump({'format': trade_cargo.FORMAT, 'posten': posten}, f,
                  ensure_ascii=False, indent=1)
    beispiel_statistik(heim)


def beispiel_statistik(heim):
    """Erfundene Sitzungen für die Statistik (und die Spielzeit oben).

    ⚠⚠ **Nie die echten.** Die Kopie trüge sonst die echte Spielzeit samt
    Wärmekarte — wann gespielt wird — in ein öffentliches Bild.
    Fester Startwert, damit jeder Lauf dasselbe Bild ergibt: abends unter der
    Woche, nachmittags am Wochenende, über acht Wochen."""
    import json
    import random
    import time as _ti
    from scbp import playtime, play_stats
    zufall = random.Random(7)
    heute = _ti.mktime(_ti.strptime(_ti.strftime('%Y-%m-%d'), '%Y-%m-%d'))
    spielzeit, statistik = [], {}
    for tage_zurueck in range(56, 0, -1):
        tag = heute - tage_zurueck * 86400
        wochenende = _ti.localtime(tag).tm_wday >= 5
        if zufall.random() > (0.8 if wochenende else 0.55):
            continue
        beginn = tag + (zufall.randint(13, 16) if wochenende
                        else zufall.randint(18, 21)) * 3600 \
            + zufall.randint(0, 59) * 60
        dauer = zufall.randint(45, 240 if wochenende else 170) * 60
        von, bis = int(beginn), int(beginn + dauer)
        spielzeit.append({'von': von, 'bis': bis})
        # Seit rc2 auch die Felder der Unterseiten — ebenso erfunden.
        absturz = zufall.random() < 0.04
        statistik[str(von)] = {
            'von': von, 'bis': bis, 'sauber': not absturz, 'account': '',
            'auftraege': zufall.randint(0, 6),
            'fehlgeschlagen': zufall.randint(0, 1),
            'abgebrochen': zufall.randint(0, 2),
            'spruenge': zufall.randint(1, 9),
            'schiffe': sorted(set(zufall.sample(BEISPIEL_SCHIFFE,
                                                zufall.randint(1, 3)))),
            'verloren': ([zufall.choice(BEISPIEL_SCHIFFE)]
                         if zufall.random() < 0.2 else []),
            'waffen': {platz: {zufall.choice(liste): 1}
                       for platz, liste in BEISPIEL_WAFFEN.items()},
            'zielwahlen': zufall.randint(2, 11),
            'ziele': {zufall.choice(BEISPIEL_ZIELE): zufall.randint(1, 3)},
            'starts': {zufall.choice(BEISPIEL_STARTS): zufall.randint(1, 3)},
            'absturz': absturz,
            'abbrueche': {'Nub destroyed': zufall.randint(1, 4),
                          'Remote Disconnect - Player requested disconnect':
                              zufall.randint(0, 2)}}
    with open(os.path.join(heim, playtime.FILE), 'w', encoding='utf-8') as f:
        json.dump({'format': playtime.FORMAT, 'sitzungen': spielzeit,
                   'gelesen': {}}, f, ensure_ascii=False)
    # ⚠⚠ **Die echten Logs als gelesen eintragen.** Die Seiten lesen
    # beim Öffnen die laufende `Game.log` nach — der Spielordner steht in der
    # kopierten Einstellung, und ohne diesen Eintrag landete eine ECHTE
    # Sitzung zwischen den erfundenen im Bild.
    gelesen = {}
    for pfad in play_stats.log_files():
        try:
            gelesen[os.path.basename(pfad)] = os.path.getsize(pfad)
        except OSError:
            pass
    with open(os.path.join(heim, play_stats.FILE), 'w', encoding='utf-8') as f:
        json.dump({'format': play_stats.FORMAT, 'sitzungen': statistik,
                   'gelesen': gelesen, 'stand': int(heute)}, f,
                  ensure_ascii=False)
    beispiel_auftragszeiten(spielzeit, zufall)


def beispiel_auftragszeiten(spielzeit, zufall):
    """Die Aufträge der Kopie in die erfundenen Sitzungen verlegen.

    ⚠⚠ Die Namen bleiben (ein Auftragstitel verrät nichts), die **Uhrzeiten
    nicht**: Das Auftrags-Protokoll und die Statistik der Aufträge zeigten
    sonst mit Datum und Uhrzeit, wann gespielt wird. Jeder Auftrag landet
    in einer der erfundenen Sitzungen, in derselben Reihenfolge wie vorher.
    ⚠ Der Lesestand (`gelesen`) bleibt stehen — ohne ihn läse die Seite die
    echten Logs nach und brächte die echten Zeiten zurück."""
    import json
    from scbp import mission_log
    if not spielzeit:
        return
    pfad = mission_log.file_path()
    try:
        with open(pfad, encoding='utf-8') as f:
            daten = json.load(f)
    except Exception:
        return
    eintraege = sorted(daten.get('auftraege') or [],
                       key=lambda e: e.get('wann') or '')
    for nr, eintrag in enumerate(eintraege):
        sitzung = spielzeit[nr * len(spielzeit) // len(eintraege)]
        laenge = max(60, sitzung['bis'] - sitzung['von'])
        beginn = sitzung['von'] + zufall.randint(0, laenge * 2 // 3)
        ende = min(sitzung['bis'], beginn + zufall.randint(5, 40) * 60)
        eintrag['wann'] = time.strftime('%Y-%m-%dT%H:%M:%S',
                                        time.localtime(beginn))
        if eintrag.get('bis'):
            eintrag['bis'] = time.strftime('%Y-%m-%dT%H:%M:%S',
                                           time.localtime(ende))
        eintrag['quelle'] = 'Game.log'
    # Wie gespeichert: neueste zuerst — die Liste zeigt die Dateireihenfolge.
    daten['auftraege'] = eintraege[::-1]
    with open(pfad, 'w', encoding='utf-8') as f:
        json.dump(daten, f, ensure_ascii=False)


# Erfundene Geräte für die Seiten Steuerung und Achsen: gängige Modelle,
# Kennung wie im Spiel (Produkt + Hersteller, dahinter die feste Endung).
BEISPIEL_GERAETE = [('VKB-Sim Gladiator NXT EVO R', '0200231D'),
                    ('VKB-Sim Gladiator NXT EVO L', '0201231D'),
                    ('T-Rudder', 'B679044F'),
                    ('VKB-Sim Gunfighter MCG', '0127231D'),
                    ('Thrustmaster TWCS Throttle', 'B687044F')]


def beispiel_steuerung(heim):
    """Erfundene Geräte für die Seiten Steuerung und Achsen.

    ⚠⚠ **Nie die echten.** Beide Seiten lesen die Dateien des Spiels selbst
    (Startprotokoll und `actionmaps.xml`), nicht die Wegwerf-Kopie — im Bild
    stünden sonst die echten Geräte samt Kennungen. Die Belegung selbst
    bleibt die echte (sie zeigt, wie die Seite arbeitet, und verrät nichts);
    getauscht werden Name und Kennung jedes Geräts. Umgelenkt wird nur in
    DIESEM Prozess, und nur zum Lesen: Die Seiten schreiben erst, wenn jemand
    etwas einstellt."""
    import re
    from scbp import joysticks
    try:
        echt = joysticks.devices()
        quelle = joysticks._actionmaps_path()
        text = io_lesen(quelle) if quelle else ''
    except Exception:
        echt, text = [], ''
    log = []
    for nr, geraet in enumerate(echt[:len(BEISPIEL_GERAETE)]):
        name, kennung = BEISPIEL_GERAETE[nr]
        kennung += '-0000-0000-0000-504944564944'
        text = re.sub(re.escape(geraet['kennung']), kennung, text, flags=re.I)
        text = text.replace(geraet['name'], name)
        log.append('<2026-09-01T18:00:00.000Z> - Connected joystick%d: %s  '
                   '{%s}\n' % (geraet['platz'], name, kennung))
    ziel = os.path.join(heim, 'beispiel-steuerung', 'actionmaps.xml')
    os.makedirs(os.path.dirname(ziel), exist_ok=True)
    with open(ziel, 'w', encoding='utf-8') as f:
        f.write(text)
    log_text = ''.join(log)
    # ⚠ Die dritte Quelle: was JETZT angesteckt ist, fragt die Seite beim
    # System ab (`input_device.devices`) — ohne diese Zeile stünden die echten
    # Sticks als dem Spiel unbekannt unter den erfundenen.
    from scbp import input_device
    angesteckt = [{'pfad': '', 'name': g['name'], 'kennung': g['kennung']}
                  for g in joysticks.devices_from_text(log_text)]
    input_device.devices = lambda: list(angesteckt)
    joysticks.all_actionmaps = lambda folder=None: [ziel] if text else []
    joysticks.devices = \
        lambda folder=None: joysticks.devices_from_text(log_text)
    # Gespeicherte Profile tragen selbst vergebene Namen.
    joysticks.profiles = lambda folder=None: []


def nachlesen_abschalten():
    """Spielzeit und Statistik lesen während der Bilder NICHTS nach.

    ⚠⚠ Das Auftrags-Protokoll liest beim Öffnen die echten Spielprotokolle
    nach und schreibt dabei Spielzeit und Statistik mit fort (`mission_log`,
    `_sz.catch_up`/`_st.startup_catch_up`) — jedes spätere Bild zeigte sonst
    die echte Spielzeit statt der erfundenen. Dazu zählt eine gerade
    laufende Sitzung mit, wenn nebenher gespielt wird.
    Umgelenkt wird nur in diesem Prozess."""
    from scbp import playtime, play_stats
    playtime.catch_up = lambda files: 0
    playtime._running_span = lambda: None
    play_stats.catch_up = lambda files: 0
    play_stats.startup_catch_up = lambda files: 0
    play_stats.rescan = lambda files=None: 0


def beispiel_bericht():
    """Die Seite zum Fehlermelden ohne den Bildschirm des Bau-Rechners.

    Der Name ist in der Kopie schon geleert (`beispiel_daten`); die Zeile
    „Bildschirm" nennt sonst Auflösung und Aufbau des Rechners, auf dem das
    Bild entsteht."""
    from scbp import report
    from scbp.language import t
    report._screens = lambda root: t('b_skalierung') % (2560, 1440, 100)


def leiste_zeigen(fenster, kennung, wurzel):
    """Die Seitenleiste so rollen, dass der gewählte Reiter im Bild steht.

    Ohne das zeigte jedes Bild die Leiste von oben — bei Reitern weiter unten
    (Einstellungen, Info) sah man nicht, wo man ist."""
    try:
        if kennung in ('erkennung', 'startprogramme') \
                and not fenster.advanced_open:
            fenster._collapse_toggle()
            for _ in range(6):
                wurzel.update()
        fenster._open_group_of_tab(kennung)
        wurzel.update_idletasks()
        knopf = (fenster.buttons.get(kennung) or (None,))[0]
        leinwand = fenster.sidebar_canvas
        if knopf is None:
            return
        innen = leinwand.nametowidget(
            leinwand.itemcget(leinwand.find_all()[0], 'window'))
        gesamt = max(1, innen.winfo_height())
        lage = knopf.winfo_rooty() - innen.winfo_rooty()
        oben = max(0, lage - leinwand.winfo_height() // 2)
        leinwand.yview_moveto(oben / gesamt)
        for _ in range(4):
            wurzel.update()
    except Exception as ausnahme:
        print('         (Leiste nicht gerollt: %s)' % ausnahme)


# Für die erfundene Statistik: gängige Schiffe, Waffen und Ziele unter ihren
# Spielschlüsseln, damit die Anzeigenamen wie im Programm aufgelöst werden.
BEISPIEL_SCHIFFE = ['ANVL_Asgard', 'AEGS_Gladius', 'DRAK_Cutlass_Black',
                    'MISC_Prospector', 'RSI_Constellation_Andromeda',
                    'ORIG_300i']
BEISPIEL_WAFFEN = {
    'ruecken': ['behr_lmg_ballistic_01', 'klwe_rifle_energy_01',
                'behr_rifle_ballistic_01'],
    'seite': ['klwe_pistol_energy_01', 'crlf_medgun_01'],
    'hand': ['klwe_rifle_energy_01', 'behr_lmg_ballistic_01',
             'crlf_medgun_01'],
}
BEISPIEL_STARTS = ['Stanton-Gateway', 'microTech', 'Crusader', 'Pyro',
                   'Baijini Point']
# ⚠ Echte Schlüsselformen aus dem Log, sonst zeigt das Bild die Namensregeln
# nicht (`play_stats.place_parts`).
BEISPIEL_ZIELE = ['rs_ext_cru-leo1', 'rs_ext_pyro3_leo', 'levski_all-001',
                  'RR_S4_L1', 'rs_ext_stan-pyro_jp1', 'NavPoint_Dynamic']


def beispiel_hangar():
    """Einen **erfundenen** Hangar in die Wegwerf-Kopie legen.

    ⚠⚠ **Der echte Hangar darf NICHT ins Bild.** Alle anderen Seiten zeigen
    den kopierten Datenstand, und das ist unbedenklich: ein
    Bauplan-Fortschritt oder ein Erzlager verrät nichts über die Person. Der
    Hangar schon — dort stehen die **Pledge-Pakete** und LTI-Markierungen, also
    was jemand ausgegeben hat. Das gehört in kein öffentliches Repo.

    Deshalb wird die Datei in der Kopie **überschrieben**, nicht ergänzt. Der
    echte Hangar liegt woanders und wird nur gelesen.

    ⚠ Die vier Schiffe sind mit Bedacht gewählt: je eines für Kampf, Bergbau,
    Bergung und Fracht — das Bild soll zeigen, wofür die Seite gut ist, nicht
    eine Sammlung. Zwei aus dem Pledge-Store, zwei im Spiel gekauft, damit man
    beide Herkünfte sieht. **Kein `paket`, kein `preis`** — genau die Felder,
    um die es oben geht.
    """
    import json
    beispiele = [
        {'name': 'Anvil Arrow', 'hersteller': 'Anvil Aerospace',
         'herkunft': 'pledge', 'lti': True, 'belegung': {}},
        {'name': 'MISC Prospector', 'hersteller': 'Musashi Industrial & '
         'Starflight Concern', 'herkunft': 'ingame', 'belegung': {}},
        {'name': 'Drake Vulture', 'hersteller': 'Drake Interplanetary',
         'herkunft': 'ingame', 'belegung': {}},
        {'name': 'Drake Cutlass Black', 'hersteller': 'Drake Interplanetary',
         'herkunft': 'pledge', 'belegung': {}},
    ]
    from scbp import fleet
    daten = {'format': fleet.FORMAT, 'schiffe': beispiele}
    ziel = os.path.join(os.environ['SC_BP_HOME'], fleet.FILE)
    try:
        with open(ziel, 'w', encoding='utf-8') as f:
            json.dump(daten, f, ensure_ascii=False, indent=1)
    except Exception as ausnahme:
        print('  Hinweis: Beispiel-Hangar misslungen (%s)' % ausnahme)
        return
    # ⚠ Die Steckplätze müssen dazu passen, sonst steht auf dem Bild viermal
    # der Hinweis auf fehlende Steckplatz-Daten — das Gegenteil dessen, was
    # die Seite kann. Vier Abrufe bei erkul, und nur beim Bilderbau.
    try:
        geholt = fleet.fetch_missing(daten)
        if geholt:
            print('  Steckplätze für %d Beispielschiffe geholt' % geholt)
    except Exception as ausnahme:
        print('  Hinweis: Steckplätze fehlen (%s)' % ausnahme)


def marken_loeschen():
    """Die Neu-Marken in der Wegwerf-Kopie auf gesehen setzen.

    ⚠ **Sie gehören nicht in die Anleitung.** Eine Marke ist eine Nachricht an
    *einen* Nutzer (dieser Bereich ist seit seinem letzten Besuch neu) — auf
    einem Bild in der Anleitung behauptet sie dasselbe gegenüber jedem Leser,
    für immer. Ausserdem verbreitern sie die Seitenleiste, wodurch die Bilder
    unterschiedlich breit würden.
    """
    import json
    from scbp import news
    stand = {'bereiche': {b: '999.0.0' for b in news.NEW_SINCE},
             'zuletzt': '999.0.0'}
    ziel = os.path.join(os.environ['SC_BP_HOME'], news.FILE)
    try:
        with open(ziel, 'w', encoding='utf-8') as f:
            json.dump(stand, f)
    except Exception as ausnahme:
        print('  Hinweis: Marken liessen sich nicht abschalten (%s)' % ausnahme)


# ------------------------------------------------ Handgriffe auf einer Seite
# ⚠ Alles über die echte Oberfläche — tippen, klicken, auswählen —, nicht über
# interne Variablen. So zeigt das Bild, was ein Spieler nach denselben
# Handgriffen sieht.

def _unter(knoten):
    for kind in knoten.winfo_children():
        yield kind
        yield from _unter(kind)


def _texte_von(w):
    import tkinter as tk
    try:
        if isinstance(w, tk.Canvas):
            return [w.itemcget(i, 'text') for i in w.find_all() if w.type(i) == 'text']
        return [w.cget('text')]
    except Exception:
        return []


def _warten(wurzel, sekunden=0.4):
    """Die ECHTE Ereignisschleife kurz laufen lassen.

    ⚠⚠ Nicht `update()` in einer Schleife: Seiten holen Daten in einem
    Hintergrund-Faden und melden sich per `after()` zurück — das geht nur,
    wenn `mainloop` läuft. Mit `update()` allein wirft der Faden
    `main thread is not in main loop`, und die Seite bleibt für immer beim
    Hinweis `Wird nachgeschlagen …` stehen.
    """
    wurzel.after(int(sekunden * 1000), wurzel.quit)
    wurzel.mainloop()
    wurzel.update_idletasks()


def tippen(seite, text, nummer=0):
    """In das `nummer`-te Eingabefeld der Seite tippen (Hinweis räumt sich)."""
    import tkinter as tk
    felder = [w for w in _unter(seite) if isinstance(w, tk.Entry)]
    if len(felder) <= nummer:
        return False
    feld = felder[nummer]
    feld.focus_set()
    # Den grauen Hinweis räumen wie beim ersten Tastendruck — ein erzeugtes
    # `<Key>` erreicht ein Fenster ausserhalb des Bildschirms nicht zuverlässig.
    getattr(feld, 'hint_hide', lambda: None)()
    feld.delete(0, 'end')
    feld.insert(0, text)
    return True


def klicken(seite, text, genau=True):
    """Das erste Bedienelement mit diesem Text anklicken."""
    import tkinter as tk
    for w in _unter(seite):
        if not isinstance(w, (tk.Label, tk.Canvas)):
            continue
        for s in _texte_von(w):
            if (s or '').strip() == text if genau else text in (s or ''):
                w.event_generate('<Button-1>', x=3, y=3)
                # ⚠ Der Klick kann die Zeile neu aufbauen (Aufklappen) — dann
                # gibt es das Element fürs Loslassen schon nicht mehr.
                try:
                    if w.winfo_exists():
                        w.event_generate('<ButtonRelease-1>', x=3, y=3)
                except tk.TclError:
                    pass
                return True
    return False


def auswaehlen(seite, beschriftung, wert):
    """In einem Auswahlfeld (`round_select`) mit dieser Beschriftung wählen."""
    import tkinter as tk
    for w in _unter(seite):
        if isinstance(w, tk.Canvas) and getattr(w, 'select', None) \
                and beschriftung in _texte_von(w):
            w.select(wert)
            return True
    return False


def rollen_zu(seite, textanfang):
    """Die Rollfläche so stellen, dass das Element mit diesem Text oben steht."""
    import tkinter as tk
    ziel = None
    for w in _unter(seite):
        if any((s or '').startswith(textanfang) for s in _texte_von(w)):
            ziel = w
            break
    if ziel is None:
        return False
    flaeche = ziel
    while flaeche is not None and not (isinstance(flaeche, tk.Canvas)
                                       and flaeche.cget('yscrollcommand')):
        flaeche = flaeche.master
    if flaeche is None:
        return False
    flaeche.update_idletasks()
    region = flaeche.bbox('all')
    if not region or region[3] <= 0:
        return False
    y = ziel.winfo_rooty() - flaeche.winfo_rooty() + flaeche.canvasy(0) - 12
    flaeche.yview_moveto(max(0.0, y / float(region[3])))
    return True


def warten_bis_weg(seite, wurzel, text, hoechstens=25.0):
    """Warten, bis kein Element mehr diesen Text zeigt (etwa `Wird nachgeschlagen`)."""
    ende = time.time() + hoechstens
    while time.time() < ende:
        _warten(wurzel, 0.5)
        if not any(text in (s or '') for w in _unter(seite) for s in _texte_von(w)):
            return True
    return False


def _t(schluessel, *a):
    from scbp.language import t
    return t(schluessel, *a)


def vorbereiten(kennung, seite, wurzel):
    """Die Seite so bedienen, dass das Bild zeigt, wozu sie gut ist."""
    if kennung == 'hangar':
        rollen_zu(seite, _t('s_hg_meine').split('(')[0].strip())
    elif kennung == 'wunschliste':
        rollen_zu(seite, _t('s_hg_wunsch_meine').split('(')[0].strip())
    elif kennung == 'herstellung':
        # ⚠ KEIN Klick: Der Sprung (`crafting_search`) klappt den Bauplan schon
        # selbst auf — ein Klick klappte ihn wieder zu.
        _warten(wurzel)
    elif kennung == 'bergbau':
        auswaehlen(seite, _t('s_bg_alle_erze'), 'Iron (Ore)')
        _warten(wurzel)
        rollen_zu(seite, _t('s_bg_alle_erze'))
    elif kennung == 'laeden':
        tippen(seite, 'Aves')
    elif kennung == 'bergung':
        # ⚠ Ein Schiff, dessen Steckplätze im kopierten Zwischenspeicher liegen
        # — ohne Netz stünde sonst nur `Wird nachgeschlagen …` im Bild.
        tippen(seite, 'Ironclad')
        _warten(wurzel)
        klicken(seite, 'Drake Ironclad')
        _warten(wurzel)
        klicken(seite, _t('s_wr_nachsehen'))
        warten_bis_weg(seite, wurzel, _t('s_wr_hole'))
    elif kennung == 'zerlegen':
        # ⚠ Ein Teilwort: Steht der volle Name im Feld, klappt die Liste gar
        # nicht erst auf, und es gibt nichts anzuklicken.
        tippen(seite, 'Aves Helm')
        _warten(wurzel)
        klicken(seite, 'Aves Helmet')
        _warten(wurzel)
        tippen(seite, 'Aves Helmet')          # voller Name: die Liste klappt zu
    elif kennung == 'patchaenderungen':
        klicken(seite, 'LIVE', genau=False)
    elif kennung == 'routen':
        # ⚠ „Area 18" mit Leerzeichen — so heisst der Ort in den Handelsdaten.
        tippen(seite, 'Area 18')
        _warten(wurzel)
        klicken(seite, 'TDD', genau=False)
        _warten(wurzel, 1.5)
        rollen_zu(seite, 'TDD')
    elif kennung == 'lager':
        # Der offene Eingabebereich füllt sonst das ganze Bild — zugeklappt
        # rückt die Lagerliste mit den Sammelzeilen darunter ins Bild.
        klicken(seite, _t('s_rf_titel'))
        _warten(wurzel)
    _warten(wurzel, 1.0)


# Was vor dem Öffnen einer Seite gesetzt wird — Sprungziele, die die Seite
# beim Aufbau liest (wie ein Klick aus der Bauplan-Liste).
VOR_DEM_OEFFNEN = {
    'herstellung': ('crafting_search', 'Aves Helmet'),
    'bergbau': ('mining_search', 'Iron'),
}


def fenster_richten(fenster, wurzel):
    """Fenstergrösse setzen und wirklich fertig zeichnen lassen.

    ⚠⚠ **Nach jedem Seitenwechsel nötig, nicht nur einmal am Anfang.** Die
    Seitenleiste misst sich je Seite neu, und mit ihr wächst `minsize` — das
    Fenster wird dabei breiter, ohne dass der Inhalt schon neu gezeichnet
    wäre. Wer genau in diesem Moment abgreift, bekommt rechts schwarze Blöcke,
    wo der Inhalt sein sollte.

    Deshalb wird die Grösse **nach** dem Seitenwechsel gesetzt (mindestens so
    breit, wie `minsize` verlangt) und danach mehrfach durchgezeichnet.
    """
    breite = hoehe = 0
    # ⚠⚠ **Mehrere Runden, bis sich nichts mehr rührt.** Einmal richten reicht
    # nicht: Die Seitenleiste misst sich nach dem Seitenwechsel noch einmal
    # nach und schiebt `minsize` dabei hoch — das Fenster wird also NACH dem
    # Richten breiter, und gezeichnet ist der Inhalt noch in der alten Breite —
    # rechts wären dann Knöpfe abgeschnitten.
    for _runde in range(4):
        try:
            min_b, min_h = (int(x) for x in fenster.root.minsize())
        except Exception:
            min_b, min_h = BREITE, HOEHE
        neu_b, neu_h = max(BREITE, min_b), max(HOEHE, min_h)
        if (neu_b, neu_h) == (breite, hoehe):
            break
        breite, hoehe = neu_b, neu_h
        fenster.root.geometry('%dx%d+%d+%d'
                              % (breite, hoehe, WEIT_WEG[0], WEIT_WEG[1]))
        for _ in range(14):
            wurzel.update()
            wurzel.update_idletasks()
    return breite, hoehe


def puffer_leeren(fenster, wurzel):
    """Das Fenster zu einem vollstaendigen Neuaufbau zwingen.

    ⚠⚠ **Ein `update()` genuegt nicht.** Tk tauscht die Seiten mit
    `pack_forget`/`pack`; das Fenster ausserhalb des Bildschirms bekommt
    danach keinen Zeichen-Auftrag von Windows, und `PrintWindow` gibt heraus,
    was zuletzt im Puffer stand — die alte Seite also.

    Der Griff, der wirklich hilft: die Fenstergroesse kurz veraendern und
    zuruecksetzen. Das erzeugt echte `<Configure>`-Ereignisse fuer **alle**
    Kinder, und Tk baut die Flaeche neu auf.
    """
    breite, hoehe = fenster.root.winfo_width(), fenster.root.winfo_height()
    for masse in ((breite - 40, hoehe - 30), (breite, hoehe)):
        fenster.root.geometry('%dx%d+%d+%d'
                              % (masse[0], masse[1], WEIT_WEG[0], WEIT_WEG[1]))
        for _ in range(8):
            wurzel.update()
            wurzel.update_idletasks()
    neu_zeichnen(fenster.root)
    for _ in range(6):
        wurzel.update()
        wurzel.update_idletasks()


def neu_zeichnen(fenster):
    """Windows anweisen, das Fenster samt aller Kinder frisch zu zeichnen."""
    if sys.platform != 'win32':
        return
    try:
        hwnd = int(fenster.wm_frame(), 16)
        ctypes.windll.user32.RedrawWindow(
            hwnd, None, None, 0x0001 | 0x0004 | 0x0080 | 0x0100 | 0x0400)
    except Exception:
        pass


def abgreifen_x11(fenster, ziel):
    """Dasselbe unter Linux — vom unsichtbaren Bildschirm.

    ⚠⚠ `PrintWindow` gibt es nur unter Windows; dieser Weg macht dieselben
    Bilder unter Linux.

    ⚠ **Der Bildschirm des Nutzers wird nicht angefasst.** Der ganze Lauf
    startet sich unter `xvfb-run` neu (`unsichtbar.sicherstellen`); das Fenster
    steht auf einem Schirm, den es nur im Speicher gibt. Deshalb darf es hier
    — anders als unter Windows — ganz normal an Position 0,0 stehen: Sichtbar
    ist dort ohnehin niemand.

    Abgegriffen wird der Schirm und auf das Fenster zugeschnitten. Unter Xvfb
    liegt nichts anderes darauf, es kann also nichts Fremdes ins Bild geraten.
    """
    from PIL import ImageGrab

    # ⚠ `fenster` ist hier das Tk-Fenster selbst (der Aufrufer uebergibt
    # `fenster.root`), nicht das Hauptfenster-Objekt.
    fenster.update_idletasks()
    x, y = fenster.winfo_rootx(), fenster.winfo_rooty()
    breite, hoehe = fenster.winfo_width(), fenster.winfo_height()
    if breite < 100 or hoehe < 100:
        return False

    schirm = os.environ.get('DISPLAY')
    if not schirm:
        raise RuntimeError('Kein DISPLAY — bitte unter xvfb-run starten.')
    bild = ImageGrab.grab(xdisplay=schirm)
    bild = bild.crop((x, y, x + breite, y + hoehe))

    # ⚠ Dieselbe Wache wie unter Windows: Ein leeres Bild heisst, dass das
    # Fenster nicht gezeichnet hat. Lieber nichts ablegen als ein schwarzes
    # Rechteck in der Anleitung.
    if not bild.getbbox():
        return False
    bild.convert('RGB').save(ziel)
    return True


def abgreifen(fenster, ziel):
    """Das Fenster in eine PNG-Datei zeichnen lassen. Gibt True bei Erfolg.

    ⚠ Abgegriffen wird **das Fenster**, nicht ein Bildschirmausschnitt — sonst
    landet dort, was gerade davor liegt.
    """
    from PIL import Image

    if sys.platform != 'win32':
        return abgreifen_x11(fenster, ziel)

    hwnd = int(fenster.wm_frame(), 16)

    user32, gdi32 = ctypes.windll.user32, ctypes.windll.gdi32

    class RECT(ctypes.Structure):
        _fields_ = [('left', ctypes.c_long), ('top', ctypes.c_long),
                    ('right', ctypes.c_long), ('bottom', ctypes.c_long)]

    rect = RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(rect))
    breite, hoehe = rect.right - rect.left, rect.bottom - rect.top
    if breite < 100 or hoehe < 100:
        return False

    # ⚠⚠ **Das Fenster zum vollstaendigen Neuzeichnen zwingen.** Ein Fenster
    # ausserhalb des Bildschirms bekommt von Windows keine Zeichen-Auftraege
    # mehr; `PrintWindow` liefert dann, was zuletzt im Puffer stand. Beim
    # Handelslager lagen dadurch **vier Seiten uebereinander** — Liste, Lager,
    # Handelslager und Verkauf gleichzeitig, unlesbar.
    #
    # RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW | RDW_FRAME
    user32.RedrawWindow(hwnd, None, None, 0x0001 | 0x0004 | 0x0080 | 0x0100 | 0x0400)

    fenster_dc = user32.GetWindowDC(hwnd)
    speicher_dc = gdi32.CreateCompatibleDC(fenster_dc)
    bitmap = gdi32.CreateCompatibleBitmap(fenster_dc, breite, hoehe)
    gdi32.SelectObject(speicher_dc, bitmap)

    # 2 = PW_RENDERFULLCONTENT — lässt auch ein unsichtbares Fenster zeichnen.
    geglueckt = user32.PrintWindow(hwnd, speicher_dc, 2)

    class BITMAPINFOHEADER(ctypes.Structure):
        _fields_ = [('biSize', ctypes.c_uint32), ('biWidth', ctypes.c_long),
                    ('biHeight', ctypes.c_long), ('biPlanes', ctypes.c_uint16),
                    ('biBitCount', ctypes.c_uint16), ('biCompression', ctypes.c_uint32),
                    ('biSizeImage', ctypes.c_uint32), ('biXPelsPerMeter', ctypes.c_long),
                    ('biYPelsPerMeter', ctypes.c_long), ('biClrUsed', ctypes.c_uint32),
                    ('biClrImportant', ctypes.c_uint32)]

    kopf = BITMAPINFOHEADER()
    kopf.biSize = ctypes.sizeof(BITMAPINFOHEADER)
    kopf.biWidth, kopf.biHeight = breite, -hoehe      # negativ = von oben nach unten
    kopf.biPlanes, kopf.biBitCount = 1, 32
    kopf.biCompression = 0

    puffer = ctypes.create_string_buffer(breite * hoehe * 4)
    gdi32.GetDIBits(speicher_dc, bitmap, 0, hoehe, puffer,
                    ctypes.byref(kopf), 0)

    gdi32.DeleteObject(bitmap)
    gdi32.DeleteDC(speicher_dc)
    user32.ReleaseDC(hwnd, fenster_dc)

    bild = Image.frombuffer('RGB', (breite, hoehe), puffer, 'raw', 'BGRX', 0, 1)

    # ⚠ **Nur der Fensterinhalt — ohne Titelleiste und Rahmen.** `PrintWindow`
    # zeichnet das ganze Fenster samt weisser Windows-Titelleiste. Die Bilder
    # aus dem Linux-Weg haben keinen Rahmen (Xvfb zeichnet keinen), und zwei
    # Sorten nebeneinander sehen nach Zufall aus.
    class POINT(ctypes.Structure):
        _fields_ = [('x', ctypes.c_long), ('y', ctypes.c_long)]

    ecke, innen = POINT(0, 0), RECT()
    if user32.ClientToScreen(hwnd, ctypes.byref(ecke)) and \
            user32.GetClientRect(hwnd, ctypes.byref(innen)):
        links, oben = ecke.x - rect.left, ecke.y - rect.top
        if innen.right > 100 and innen.bottom > 100:
            bild = bild.crop((links, oben, links + innen.right,
                              oben + innen.bottom))
    # ⚠ Ein völlig schwarzes Bild heisst: das Fenster hat nicht gezeichnet.
    # Lieber nichts ablegen als ein schwarzes Rechteck in die Anleitung.
    if not bild.getbbox():
        return False
    bild.save(ziel)
    return bool(geglueckt)


def overlay_bild(ziel, englisch=False):
    """Das Overlay selbst fotografieren — es ist kein Seiten-Fenster.

    ⚠⚠ `SEITEN` kennt nur die Seiten des Hauptfensters; das Overlay ist eine
    eigene Klasse mit eigenem Fenster und braucht deshalb diesen eigenen Weg.

    ⚠ **Der Watcher-Thread laeuft dabei mit** — er gehoert zur Klasse. Was er
    anfassen koennte, ist vorher abgeschaltet (`_gefaehrliches_abschalten`);
    seine Daten liegen ohnehin in der Wegwerf-Kopie.

    Die Zeilen werden von Hand gesetzt statt abgewartet: Ein echter Fund
    braucht ein laufendes Spiel, und ein leeres Overlay erklaert niemandem,
    wozu das Werkzeug gut ist.
    """
    from scbp import language
    import sc_bp_watcher

    language.set_language('en' if englisch else 'de')
    # ⚠ **Keine eigene `tk.Tk()`.** `Overlay` legt selbst eine an und haelt sie
    # in `.root` — eine zweite waere genau der Fall, den Tk nicht vertraegt.
    overlay = sc_bp_watcher.Overlay()
    fenster = overlay.root

    # ⚠⚠ **`deiconify()` nicht vergessen.** Das Overlay startet versteckt und
    # zeigt sich erst, wenn alles bereit ist. Ohne diesen Aufruf misst Tk
    # **1x1 Pixel**, der Abgriff liefert ein leeres Bild und meldet wortlos
    # „FEHL".
    fenster.deiconify()
    # ⚠ Knapp gehalten. Das alte Bild war 1240x888 und bestand zu zwei Dritteln
    # aus leerer Flaeche — das Overlay ist im Betrieb schmal und niedrig, so
    # soll es auch aussehen.
    fenster.geometry('%dx%d+%d+%d' % (760, 300, WEIT_WEG[0], WEIT_WEG[1]))

    # ⚠ Dem Watcher-Faden Zeit lassen: Die Kopfzeile (`413 Bauplaene · Log ✓`)
    # entsteht erst, wenn er den Bestand gelesen hat. Wer zu frueh abgreift,
    # fotografiert `Starte ...`.
    ende_zeit = time.time() + 4.0
    while time.time() < ende_zeit:
        fenster.update()
        fenster.update_idletasks()
        time.sleep(0.02)

    # Ein glaubwuerdiger Stand: zwei eigene Funde, ein Katalog-Zuwachs. Von
    # Hand gesetzt — ein echter Fund braucht ein laufendes Spiel.
    jetzt = time.strftime('%H:%M:%S')
    overlay.add_new('Arclight "Midnight" Pistol', 'FPS, Pistol', '–/A/1', jetzt)
    overlay.add_new('CF-337 Panther Repeater', 'Laser Repeater', '–/–/3', jetzt)
    overlay.add_catalog('Zephyr', 'Quantum Drive', jetzt, '')
    for _ in range(14):
        fenster.update()
        fenster.update_idletasks()
        time.sleep(0.02)

    # ⚠⚠ **Die Groesse NACH dem Anlaufen noch einmal setzen.**
    # Das Overlay richtet sich beim Start selbst ein (Ecke, Arbeitsflaeche,
    # Klappzustand) und ueberschreibt dabei die Groesse von oben — heraus
    # kaeme ein zu kleines Bild mit abgeschnittenen Funden und Rollleiste.
    fenster.minsize(1, 1)
    fenster.geometry('%dx%d+%d+%d' % (760, 300, WEIT_WEG[0], WEIT_WEG[1]))
    for _ in range(14):
        fenster.update()
        fenster.update_idletasks()
        time.sleep(0.02)
    print('  Overlay-Groesse beim Abgriff: %dx%d'
          % (fenster.winfo_width(), fenster.winfo_height()))

    geglueckt = abgreifen(fenster, ziel)
    try:
        fenster.destroy()
    except Exception:
        pass
    return geglueckt


def main():
    # ⚠⚠ **Zuerst, vor jedem Tk-Fenster.** Unter Windows genuegte es, das
    # Fenster weit ausserhalb aufzubauen; unter Linux haengt die Shell an
    # `DISPLAY=:0`, also am echten Monitor — ein Fenster blitzt dort auf und
    # reisst den Tastaturfokus mit. Wer gerade Star Citizen fliegt, landet im
    # Desktop und stirbt.
    #
    # ⚠ Der Schirm muss groesser sein als das Fenster, sonst schneidet Xvfb ab.
    #
    # ⚠⚠ **`messend=True` — sonst liefert Windows kein einziges Bild.** Ohne
    # die Kennzeichnung versteckt `unsichtbar.py` unter Windows jedes Fenster
    # mit `withdraw()`, und ein verstecktes Fenster zeichnet nichts, auch nicht
    # fuer `PrintWindow`. Mit der Kennzeichnung bleibt es aufgebaut,
    # wird aber voellig durchsichtig und weit neben den Schirm geschoben — und
    # nach vorn holen bleibt gesperrt. Unter Linux aendert sich nichts, dort
    # greift vorher Xvfb.
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import unsichtbar
    unsichtbar.sicherstellen(BREITE + 100, HOEHE + 90, messend=True)

    # ⚠⚠ **Immer 100 % — gleich wie auf jedem anderen Rechner.** Dieses Werkzeug
    # macht sich oben DPI-bewusst (fuer den richtigen Abgriff). Dadurch sieht Tk
    # die echte Bildschirm-Skalierung, bei 125 % also 120 DPI, und zeichnet
    # alles ein Viertel groesser: groessere Schrift als in den anderen
    # Bildern, und die Seitenleiste klappt mangels Hoehe zu. Der Watcher
    # selbst laeuft dort mit 100 % (sein Fehlerbericht sagt es).
    # Deshalb bekommt JEDE Tk-Wurzel 96 DPI — auch die, die das Overlay selbst
    # anlegt. Unter Linux arbeitet Xvfb ohnehin mit 96 DPI.
    if sys.platform == 'win32':
        import tkinter as _tk_wurzel
        _anlegen = _tk_wurzel.Tk.__init__

        def _mit_hundert_prozent(self, *a, _anlegen=_anlegen, **k):
            _anlegen(self, *a, **k)
            try:
                self.tk.call('tk', 'scaling', 96 / 72)
            except Exception:
                pass

        _tk_wurzel.Tk.__init__ = _mit_hundert_prozent

    # ⭐ `--schema=<kennung>`: die Bilder in einem bestimmten Farbschema — je
    # Schema ein Bild der Seite Darstellung, damit man die Farben vorab sieht.
    # Ohne Seitenangabe entsteht nur `darstellung`, unter
    # eigenem Namen `screenshot-farbschema-<kennung>` — die übrigen Bilder
    # bleiben im gewohnten Schema.
    schema = None
    for a in sys.argv[1:]:
        if a.startswith('--schema='):
            schema = a.split('=', 1)[1]
    argumente = [a for a in sys.argv[1:] if not a.startswith('--')]
    englisch = '--en' in sys.argv
    if schema and not argumente:
        argumente = ['darstellung']

    # ⚠ Der eigene Prozess fuer das Overlay — siehe `OVERLAY_NAME`.
    if '--nur-overlay' in sys.argv:
        os.environ['SC_BP_HOME'] = datenstand_kopieren()
        os.environ['SC_BP_NO_NET'] = '1'
        marken_loeschen()
        ziel = os.path.join(HIER, 'assets',
                            OVERLAY_NAME + ('-en' if englisch else '') + '.png')
        ok = overlay_bild(ziel, englisch)
        print('  [%s]   %s' % ('ok' if ok else 'FEHL', os.path.basename(ziel)))
        return 0 if ok else 1

    gewuenscht = argumente or list(SEITEN)

    unbekannt = [s for s in gewuenscht if s not in SEITEN]
    if unbekannt:
        print('Unbekannte Seite(n): %s' % ', '.join(unbekannt))
        print('Bekannt sind: %s' % ', '.join(SEITEN))
        return 2

    os.environ['SC_BP_HOME'] = datenstand_kopieren()
    # Das Schema steht in der Kopie, bevor irgendetwas die Farben liest —
    # `theme` liest sie beim Import, danach wirkt eine Änderung nicht mehr.
    # Ohne Angabe gilt immer `original`: Die Webseite zeigt das gewohnte
    # Aussehen, egal welches Schema in der Kopie eingestellt ist.
    import json as _json
    _pfad = os.path.join(os.environ['SC_BP_HOME'], 'einstellungen.json')
    try:
        with open(_pfad, encoding='utf-8') as _datei:
            _daten = _json.load(_datei)
    except Exception:
        _daten = {}
    _daten['farbschema'] = schema or 'original'
    with open(_pfad, 'w', encoding='utf-8', newline='\n') as _datei:
        _json.dump(_daten, _datei, ensure_ascii=False, indent=1)
    # ⚠⚠ **Der Beispiel-Hangar VOR `SC_BP_NO_NET`.** Er holt die Steckplätze
    # seiner vier Schiffe, und dafür braucht er das Netz — `catalog.OFF` liest
    # die Sperre beim Import und behält sie danach. Wer die Reihenfolge dreht,
    # bekommt viermal den Hinweis auf fehlende Steckplatz-Daten ins Bild, also
    # ausgerechnet das Gegenteil dessen, was die Seite zeigen soll.
    # ⚠ Mit Netz, ausser `--offline`: Die Bestückung und die Preise der
    # fehlenden Teile holen ihre Daten erst beim Aufruf — ohne Netz stünde
    # dort nur `Wird nachgeschlagen …` im Bild.
    if '--offline' in sys.argv:
        os.environ['SC_BP_NO_NET'] = '1'
    # ⚠ Immer und für jede Seite — siehe `beispiel_daten`. Die Beispielschiffe
    # liegen im kopierten erkul-Zwischenspeicher, das Netz braucht es nicht.
    beispiel_daten()
    beispiel_steuerung(os.environ['SC_BP_HOME'])
    beispiel_bericht()
    nachlesen_abschalten()
    marken_loeschen()

    import tkinter as tk
    from scbp import language
    from scbp.main_window import MainWindow

    language.set_language('en' if englisch else 'de')

    wurzel = tk.Tk()
    wurzel.withdraw()

    # ⚠⚠ **EIN Fenster fuer alle Seiten — nicht je Seite ein frisches.**
    # Was die beiden Wege ergeben:
    #
    # | Weg | was dabei herauskam |
    # |---|---|
    # | ein Fenster, Seiten nacheinander | saubere Flaechen, aber die alte Seite blieb im Puffer stehen — auf einem Bild lagen **vier Seiten uebereinander** |
    # | je Seite ein frisches Fenster | keine Ueberlagerung mehr, dafuer **halb gezeichnete Flaechen**: helle Rechtecke der Bedienelemente auf ungezeichnetem Grund |
    #
    # Ein frisch erzeugtes Fenster ausserhalb des Bildschirms zeichnet seine
    # Flaechen nie fertig; eines, das schon ein paar Runden gelaufen ist, tut
    # es. Also: ein Fenster, und der Puffer wird vor jedem Bild durch
    # `puffer_leeren` wirklich geraeumt.
    ziel_ordner = os.path.join(HIER, 'assets')
    gemacht, misslungen = [], []

    for kennung in gewuenscht:
        grundname = (('screenshot-farbschema-%s' % schema) if schema
                     else SEITEN[kennung])
        name = grundname + ('-en' if englisch else '') + '.png'
        ziel = os.path.join(ziel_ordner, name)
        fenster = None
        try:
            # Frisches Fenster **und** Puffer raeumen — erst beides zusammen
            # liefert brauchbare Bilder (siehe die Tabelle oben).
            fenster = MainWindow(wurzel, version=_version())
            fenster_richten(fenster, wurzel)
            if kennung in VOR_DEM_OEFFNEN:
                setattr(fenster, *VOR_DEM_OEFFNEN[kennung])
            fenster.open_page(kennung)
            for _ in range(12):
                wurzel.update()
                wurzel.update_idletasks()
            fenster_richten(fenster, wurzel)
            leiste_zeigen(fenster, kennung, wurzel)
            seite = fenster.pages.get(kennung)
            if seite is not None:
                vorbereiten(kennung, seite, wurzel)
            puffer_leeren(fenster, wurzel)
            # Die Seiten holen ihre Daten ueber `after`-Rueckrufe nach — wer zu
            # frueh abgreift, fotografiert eine halbfertige Seite.
            ende_zeit = time.time() + 1.0
            while time.time() < ende_zeit:
                wurzel.update()
                wurzel.update_idletasks()
                time.sleep(0.02)
            neu_zeichnen(fenster.root)
            wurzel.update()
            if abgreifen(fenster.root, ziel):
                gemacht.append(name)
                print('  [ok]   %s' % name)
            else:
                misslungen.append(name)
                print('  [leer] %s — das Fenster hat nichts gezeichnet' % name)
        except Exception as ausnahme:
            misslungen.append(name)
            print('  [FEHL] %s — %s' % (name, ausnahme))
        finally:
            try:
                if fenster is not None:
                    fenster.root.destroy()
            except Exception:
                pass

    try:
        wurzel.destroy()
    except Exception:
        pass

    print()
    print('%d Bild(er) in assets/ — %d misslungen' % (len(gemacht), len(misslungen)))
    return 1 if misslungen else 0


if __name__ == '__main__':
    sys.exit(main())
