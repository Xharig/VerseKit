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
Wo liegt was — je nach Betriebssystem.

Das ist die einzige Stelle im Programm, die Windows- und Linux-Pfade kennt.
Alles andere fragt hier nach und muss nicht wissen, auf welchem System es läuft.

Drei Sorten Pfade:

  1. **Eigene Dateien** (Einstellungen, Bestand, Lesestand)
     Windows:  %APPDATA%\\sc-bp-watcher\\
     Linux:    ~/.config/sc-bp-watcher/     (bzw. $XDG_CONFIG_HOME)

  2. **Star Citizen selbst** (Game.log und die Sicherungen)
     Windows:  C:\\Program Files\\Roberts Space Industries\\StarCitizen\\LIVE\\
     Linux:    im Wine-Präfix, z. B. ~/Games/star-citizen/drive_c/Program Files/…

  3. **SC Deutsch Launcher** (optional, nur wenn vorhanden)
     Er ist kein Muss — wenn er da ist, wird er weiter genutzt,
     weil er den vollständigen Bestand und einen gepflegten Katalog liefert.

**Wer die Sachen woanders liegen hat, trägt die Pfade selbst ein.** Dafür gibt es
`einstellungen.json` im eigenen Ordner:

    {
      "spiel_ordner":    "/mnt/spiele/StarCitizen/LIVE",
      "launcher_ordner": "D:\\SCDL\\blueprints"
    }

Die Datei wird angelegt, sobald das Spiel nicht gefunden wird — mit Kommentar
und leeren Feldern zum Ausfüllen. Ein leeres Feld heißt „bitte suchen".

Rangfolge, wenn mehrere Angaben da sind:

  1. Umgebungsvariable — `SC_BP_HOME`, `SC_INSTALL_DIR`, `SC_BP_LAUNCHER`
     (für einen einmaligen Sonderfall, ohne etwas zu ändern)
  2. `einstellungen.json` — der normale Weg für einen dauerhaft eigenen Pfad
  3. die Suche an den üblichen Stellen
"""
import glob
import json
import os
import re
import sys

WINDOWS = sys.platform.startswith('win')

# Spielkanäle in der Reihenfolge, in der gesucht wird. LIVE zuerst — wer PTU
# spielt, hat meist beides installiert, gemeint ist aber fast immer LIVE.
#
# ⚠⚠ **HOTFIX gehört dazu.** Legt CIG neben LIVE eine ausgebesserte Fassung auf
# denselben Server, ist das eine eigene Installation daneben. Und der übliche
# Weg dorthin ist nicht „nochmal 100 GB laden", sondern: **den vorhandenen
# LIVE-Ordner in HOTFIX umbenennen**, damit der Launcher nur die Unterschiede
# holt. Damit ist der eingetragene Spielordner von einem Tag auf den anderen
# **weg** — und der Watcher fand gar nichts mehr, weil HOTFIX in dieser Liste
# fehlte. Genau so gesehen am 03.09.2026 bei Haldjas: eingetragen war
# `…\StarCitizen\LIVE`, im Bericht standen „Spiel nicht gefunden", keine
# Game.log und 0 gelesene Protokolle.
KANAELE = ('LIVE', 'HOTFIX', 'PTU', 'EPTU', 'TECH-PREVIEW')

# Unterpfad ab dem Wurzelverzeichnis eines Laufwerks bis zum Spielkanal.
SC_UNTERPFAD = os.path.join('Roberts Space Industries', 'StarCitizen')


# ------------------------------------------------------------ 1. Eigene Dateien
# Wohin welche Datei gehört. Zweck: Wer den Ordner öffnet, soll sehen, was
# seins ist — Baupläne und Ausgaben getrennt vom technischen Kleinkram.
# Was hier nicht steht, landet in „Intern"; das sind Zwischenspeicher und
# Lesestände, die niemanden interessieren.
UNTERORDNER = {
    'bestand.json':      'Bauplaene',
    'bestand.bak.json':  'Bauplaene',
    'merkliste.json':    'Bauplaene',
    'watchlist.json':    'Bauplaene',
    'catalog-seen.json': 'Bauplaene',
    'bp-overrides.json': 'Bauplaene',
    # Das Auftrags-Protokoll wird fortgeschrieben, nicht neu gebaut: Die
    # Game-Logs reichen nur wenige Sitzungen zurueck, das Protokoll soll bleiben.
    'auftragslog.json':     'Intern',
    'auftragslog.bak.json': 'Intern',
    'einstellungen.json': 'Einstellungen',
    'phrasen.json':       'Einstellungen',
    'gesehen.json':       'Einstellungen',
    'fehler.json':        'Diagnose',
    'bericht.txt':        'Diagnose',
    # Das Protokoll des Setups beim Selbst-Update. Gehoert zur Diagnose:
    # Meldet jemand 'das Update geht nicht', steht hier, woran es lag.
    'update-setup.txt':   'Diagnose',
}
ORDNERNAME = 'SC BP Watcher'
EINSTELLUNGEN = 'einstellungen.json'


def _dokumente():
    """Der Dokumente-Ordner des Nutzers — oder das Heimatverzeichnis."""
    heim = os.path.expanduser('~')
    if WINDOWS:
        # Der Ordner kann umbenannt oder verschoben sein; die Registry weiß es.
        try:
            import winreg
            schluessel = (r'Software\Microsoft\Windows\CurrentVersion'
                          r'\Explorer\Shell Folders')
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, schluessel) as k:
                wert = winreg.QueryValueEx(k, 'Personal')[0]
                if wert and os.path.isdir(os.path.expandvars(wert)):
                    return os.path.expandvars(wert)
        except Exception:
            pass
    else:
        # Unter Linux sagt es die XDG-Angabe; sie ist übersetzt („Dokumente").
        try:
            konfig = os.path.join(os.environ.get('XDG_CONFIG_HOME')
                                  or os.path.join(heim, '.config'),
                                  'user-dirs.dirs')
            with open(konfig, encoding='utf-8') as f:
                for zeile in f:
                    if zeile.startswith('XDG_DOCUMENTS_DIR'):
                        wert = zeile.split('=', 1)[1].strip().strip('"')
                        wert = wert.replace('$HOME', heim)
                        if os.path.isdir(wert):
                            return wert
        except Exception:
            pass
    for name in ('Documents', 'Dokumente'):
        p = os.path.join(heim, name)
        if os.path.isdir(p):
            return p
    return heim


def alter_app_ordner():
    """Wo die Dateien bis v2.x lagen — Rückfall und Quelle für den Umzug."""
    if WINDOWS:
        return os.path.join(os.environ.get('APPDATA', os.path.expanduser('~')),
                            'sc-bp-watcher')
    basis = os.environ.get('XDG_CONFIG_HOME') or os.path.expanduser('~/.config')
    return os.path.join(basis, 'sc-bp-watcher')


def _ablage_aus_datei():
    """Einen selbst gewählten Ablage-Ort lesen — **ohne** `einstellung()`.

    ⚠ Hier lauert eine Schleife: `einstellung()` liest ihre Datei über
    `app_datei()`, und das fragt wieder `app_ordner()`. Wer an dieser Stelle die
    normale Einstellungs-Funktion benutzt, baut eine Endlosrekursion — die
    obendrein unsichtbar bleibt, weil ringsherum `try/except` steht. Deshalb
    wird die Datei hier am Standardort direkt gelesen.
    """
    try:
        standard = os.path.join(_dokumente(), ORDNERNAME, 'Einstellungen',
                                EINSTELLUNGEN)
        if not os.path.isfile(standard):
            return None
        with open(standard, encoding='utf-8') as f:
            wert = json.load(f).get('ablage_ordner')
        return wert.strip() if isinstance(wert, str) and wert.strip() else None
    except Exception:
        return None


def app_ordner():
    """Ordner für unsere eigenen Dateien. Wird bei Bedarf angelegt.

    Seit v3.0.0 liegt er **sichtbar** unter Dokumente statt versteckt in
    `%APPDATA%` bzw. `~/.config` — dort sucht kein normaler Spieler, und seinen
    Bauplan-Bestand sollte er finden können. Ein eigener Ort geht weiterhin über
    `SC_BP_HOME` oder die Einstellung `ablage_ordner`.
    """
    eigen = os.environ.get('SC_BP_HOME') or _ablage_aus_datei()
    p = (os.path.expanduser(eigen) if eigen
         else os.path.join(_dokumente(), ORDNERNAME))
    try:
        os.makedirs(p, exist_ok=True)
    except OSError:
        pass
    return p


def programm_datei(name):
    """Voller Pfad zu einer **mitgelieferten** Datei aus dem Ordner `daten/`.

    Das sind Dateien, die zum Programm gehören und nur gelesen werden — im
    Gegensatz zu `app_datei()`, wo die Daten des Nutzers liegen.

    ⚠ Zwei Fälle: Läuft das Programm aus dem Quellcode, liegt `daten/` neben
    `scbp/`. Ist es zu einer Datei gepackt (PyInstaller, AppImage), entpackt
    sich alles in einen Wegwerf-Ordner, dessen Pfad in `sys._MEIPASS` steht.
    Wer das nicht abfängt, sucht im gepackten Programm an der falschen Stelle
    und findet nichts."""
    gepackt = getattr(sys, '_MEIPASS', None)
    basis = gepackt or os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    return os.path.join(basis, 'daten', name)


def app_datei(name):
    """Voller Pfad zu einer eigenen Datei, z. B. app_datei('bestand.json').

    Sortiert nach `UNTERORDNER` in Unterordner ein. Wer ein `SC_BP_HOME` gesetzt
    hat (Selbsttest, Sonderfälle), bekommt den flachen Ordner von früher — dort
    geht es um Wegwerf-Ordner, nicht um Übersicht.
    """
    basis = app_ordner()
    if os.environ.get('SC_BP_HOME'):
        return os.path.join(basis, name)
    unter = UNTERORDNER.get(name, 'Intern')
    ziel = os.path.join(basis, unter)
    try:
        os.makedirs(ziel, exist_ok=True)
    except OSError:
        return os.path.join(basis, name)
    return os.path.join(ziel, name)


def umzug_noetig():
    """Liegen im alten Ordner Dateien, die im neuen fehlen?"""
    alt = alter_app_ordner()
    if not os.path.isdir(alt) or os.environ.get('SC_BP_HOME'):
        return False
    try:
        vorhanden = [n for n in os.listdir(alt) if n.endswith(('.json', '.txt'))]
    except OSError:
        return False
    if not vorhanden:
        return False
    # Schon umgezogen? Dann liegt der Bestand am neuen Ort.
    return not os.path.exists(app_datei('bestand.json'))


def umziehen():
    """Die Dateien aus dem alten Ordner in den neuen **kopieren**.

    Kopieren, nicht verschieben: Geht beim Umzug etwas schief — Rechte, ein
    Virenscanner, ein abgebrochener Start — ist der mühsam gesammelte
    Bauplan-Bestand sonst weg. Der alte Ordner bleibt unangetastet liegen; er
    kostet ein paar Kilobyte und ist der Rückweg.

    Gibt die Zahl der kopierten Dateien zurück.
    """
    import shutil
    alt = alter_app_ordner()
    kopiert = 0
    try:
        namen = sorted(os.listdir(alt))
    except OSError:
        return 0
    for name in namen:
        quelle = os.path.join(alt, name)
        if not os.path.isfile(quelle):
            continue
        ziel = app_datei(name)
        if os.path.exists(ziel):
            continue                     # nichts überschreiben
        try:
            os.makedirs(os.path.dirname(ziel), exist_ok=True)
            shutil.copy2(quelle, ziel)
            kopiert += 1
        except OSError:
            pass
    return kopiert


# ------------------------------------------------------- Selbst gesetzte Pfade

def gesuchte_spielorte(hoechstens=6):
    """Die Orte, an denen tatsächlich nach Star Citizen gesucht wird.

    Wird dem Nutzer angezeigt, wenn nichts gefunden wurde. Ohne diese Angabe
    weiß er nicht, wonach er suchen soll — und ein Pfad, den er selbst eintragen
    soll, ist ohne Vorbild schwer zu erraten. Vorhandene Ordner kommen zuerst:
    Wer seine Installation dort halb wiederfindet, sieht sofort, wie der Rest
    aussehen muss."""
    kandidaten = []
    for wurzel in _spiel_wurzeln():
        p = os.path.join(wurzel, SC_UNTERPFAD, KANAELE[0])
        if p not in kandidaten:
            kandidaten.append(p)
    # Wenn gar nichts zusammenkommt, sind auf diesem Rechner weder Wine-Präfixe
    # noch Programmordner da — und ausgerechnet dann braucht der Nutzer das
    # Vorbild am dringendsten. Also die typischen Orte zeigen, auch wenn es sie
    # hier nicht gibt.
    if not kandidaten:
        heim = os.path.expanduser('~')
        if WINDOWS:
            kandidaten = [os.path.join('C:\\Program Files', SC_UNTERPFAD,
                                       KANAELE[0])]
        else:
            for praefix in (os.path.join(heim, 'Games', 'star-citizen'),
                            os.path.join(heim, '.wine')):
                kandidaten.append(os.path.join(praefix, 'drive_c', 'Program Files',
                                               SC_UNTERPFAD, KANAELE[0]))
            kandidaten.append(os.path.join(
                heim, '.local', 'share', 'lutris', 'prefixes', '<Name>', 'drive_c',
                'Program Files', SC_UNTERPFAD, KANAELE[0]))
    # existierende zuerst, Reihenfolge sonst beibehalten
    da = [p for p in kandidaten if os.path.isdir(os.path.dirname(p))]
    rest = [p for p in kandidaten if p not in da]
    return (da + rest)[:hoechstens]


def gesuchte_launcherorte(hoechstens=3):
    """Dasselbe für den Blueprint-Ordner des SC Deutsch Launchers."""
    if WINDOWS:
        return [os.path.join(os.environ.get('APPDATA', '%APPDATA%'),
                             'sc-deutsch-launcher', 'blueprints')]
    orte = list(_windows_launcher())
    for praefix in _wine_praefixe()[:hoechstens]:
        orte.append(os.path.join(praefix, 'drive_c', 'users', '<Benutzer>',
                                 'AppData', 'Roaming', 'sc-deutsch-launcher',
                                 'blueprints'))
    return orte[:hoechstens]


def _vorlage():
    """Der Inhalt der Einstellungsdatei — mit den echten Suchorten dieses Rechners.

    Die Hinweiszeilen stehen bewusst **direkt unter** dem jeweiligen Feld: In
    einer JSON-Datei gibt es keine ausgegraute Beschriftung, das Nächstliegende
    ist ein Feld daneben, das man beim Ausfüllen zwangsläufig liest. Sie werden
    nicht ausgewertet — was drinsteht, ändert nichts."""
    return {
        '_hinweis': 'Eigene Pfade eintragen, wenn Star Citizen oder der '
                    'SC Deutsch Launcher nicht an den ueblichen Stellen liegen. '
                    'Leeres Feld = automatisch suchen. Nach dem Aendern den '
                    'Watcher neu starten. Zeilen mit _ sind nur Erklaerung.',
        'spiel_ordner': '',
        '_spiel_ordner_gemeint_ist': 'Der Ordner, in dem die Game.log liegt — '
                                     'meist "LIVE".',
        '_spiel_ordner_gesucht_wird_hier': gesuchte_spielorte(),
        'sprache': 'auto',
        '_sprache_moeglich': 'auto (Systemsprache), de, en',
        'pruefintervall_sekunden': 3,
        '_pruefintervall_gemeint_ist': 'Wie oft die Game.log angesehen wird. '
                                       'Erlaubt 1 bis 60.',
        'signalton': True,
        '_signalton_gemeint_ist': 'Kurzer Ton, wenn ein Bauplan erscheint.',
        'deckkraft_prozent': 93,
        '_deckkraft_gemeint_ist': 'Wie undurchsichtig das Fenster ist. 100 = '
                                  'blickdicht, 30 = stark durchscheinend. '
                                  'Erlaubt 30 bis 100.',
        'launcher_ordner': '',
        '_launcher_ordner_gemeint_ist': 'Optional. Der Ordner "blueprints" des '
                                        'SC Deutsch Launchers. Ohne ihn laeuft '
                                        'der Watcher trotzdem.',
        '_launcher_ordner_gesucht_wird_hier': gesuchte_launcherorte(),
    }


def _melden(stelle, ausnahme):
    """Einen Fehler ins Protokoll geben, ohne dabei selbst zu scheitern.

    ⚠ Der Import steht **absichtlich** in der Funktion: `scbp/fehler.py`
    importiert seinerseits `pfade`. Auf Modulebene wäre das ein Zirkelbezug und
    keines der beiden Module ließe sich mehr laden.

    ⚠ Und das Melden hängt in einem eigenen `try`: Wenn schon das Schreiben der
    Einstellungen scheitert, kann auch das Fehlerprotokoll klemmen. Dann ist der
    ursprüngliche Fehler zwar verloren — aber das Programm läuft weiter, und
    darum geht es.
    """
    try:
        from . import fehler
        fehler.merken(stelle, ausnahme)
    except Exception:
        pass


def einstellungen():
    """Die selbst eingetragenen Pfade. Fehlt die Datei, ist sie leer."""
    try:
        with open(app_datei(EINSTELLUNGEN), encoding='utf-8') as f:
            daten = json.load(f)
        return daten if isinstance(daten, dict) else {}
    except Exception:
        return {}


def einstellung(name):
    """Ein einzelner selbst gesetzter Pfad — oder None, wenn nichts eingetragen ist.

    ⚠⚠ **Alles, was kein Text ist, gilt als „nicht gesetzt".** Vorher stand
    hier `(… or '').strip()`. Bei einem Ja/Nein-Wert überlebt `True` das
    `or ''` und `.strip()` fliegt mit einem AttributeError — und zwar nicht
    leise: Am 03.09.2026 hat ein einziger falscher Aufruf
    (`einstellung('lager_raffinerie_offen')` statt `einstellung_wahrheit`) den
    Aufbau der ganzen Lager-Seite abgerissen. Die Liste der Posten fehlte,
    die Daten waren unversehrt, und weil eine Seite nur einmal gebaut wird,
    half auch Zuklappen nicht mehr.

    Ein Pfad ist immer Text. Kommt etwas anderes, ist das ein Aufruf an der
    falschen Adresse — dann `None` zurückzugeben ist richtig und kostet
    niemanden eine Seite. Für Ja/Nein gibt es `einstellung_wahrheit`, für
    Zahlen `einstellung_zahl`.
    """
    wert = einstellungen().get(name)
    if not isinstance(wert, str):
        return None
    wert = wert.strip()
    return os.path.expanduser(wert) if wert else None


def json_sichern(ziel, daten, einzug=1, sortiert=False):
    """JSON schreiben — ohne Halbfertiges und mit Vorgängerfassung.

    Erst in eine Nebendatei, dann umbenennen: Stürzt der Rechner mitten im
    Schreiben ab, ist die alte Datei noch vollständig da. Die vorige Fassung
    bleibt als `….bak.json` liegen.

    ⚠ **Warum das hier steht und nicht in jedem Modul einzeln.** `bestand.py`
    hatte diese Sicherung von Anfang an, die beiden Lager (Werkstatt und
    Handel) nicht — sie schrieben zwar atomar, aber ohne Rückfall. Genau dort
    stehen **eigene Eingaben**, die es nirgends sonst zu holen gibt: Ein
    Bauplan-Bestand liesse sich aus der Game.log neu aufbauen, ein Lager nicht.
    Zwei Fassungen derselben Regel gehen irgendwann auseinander, deshalb eine.

    ⚠ **Meldet nichts selbst.** Der Aufrufer weiss, unter welchem Namen der
    Fehler ins Protokoll gehört (`rohstoffe.sichern` gegen
    `handelslager.sichern`) — und `pfade` darf `fehler` nicht einbinden, das
    gäbe einen Ringschluss. Bei einem Fehlschlag fliegt die Ausnahme; die
    Nebendatei ist dann schon weggeräumt.
    """
    temp = ziel + '.tmp'
    try:
        ordner = os.path.dirname(ziel)
        if ordner:
            os.makedirs(ordner, exist_ok=True)
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump(daten, f, ensure_ascii=False, indent=einzug,
                      sort_keys=sortiert)
        if os.path.exists(ziel):
            # ⚠ Nicht `ziel.replace('.json', …)` — das trifft auch einen
            # Ordnernamen, in dem „.json" vorkommt. Nur die Endung zählt.
            sicherung = (ziel[:-5] + '.bak.json' if ziel.endswith('.json')
                         else ziel + '.bak')
            try:
                os.replace(ziel, sicherung)
            except OSError:
                # Kein Grund abzubrechen: Die Vorgängerfassung ist der Gürtel,
                # das atomare Schreiben der Hosenträger. Einer genügt.
                pass
        os.replace(temp, ziel)
        return True
    except Exception:
        try:
            os.remove(temp)
        except OSError:
            pass
        raise


def zeiger_datei():
    """Die Datei, aus der der Ablage-Ort gelesen wird — immer am Standardort.

    ⚠ Sie ist NICHT dieselbe wie die Einstellungsdatei in der Ablage, sobald
    ein eigener Ablage-Ort gesetzt ist. `_ablage_aus_datei()` liest ausschliesslich
    hier; alles andere steht in der Ablage selbst.
    """
    return os.path.join(_dokumente(), ORDNERNAME, 'Einstellungen',
                        EINSTELLUNGEN)


def _ablage_ordner_setzen(wert):
    """Den Ablage-Ort in die Zeiger-Datei schreiben — nicht in die Ablage.

    ⚠⚠ **Sonst merkt sich das Programm den neuen Ort an einer Stelle, die es
    nie wieder liest.** `einstellung_setzen()` schreibt ueber `app_datei()`,
    und das zeigt in den **aktuellen** Ablage-Ordner. Der neue Ort landete
    damit in der Einstellungsdatei des ALTEN Ordners, waehrend
    `_ablage_aus_datei()` weiter den unveraenderten Zeiger unter Dokumente las.
    Ergebnis: Die Umstellung liess sich speichern, war aber nach jedem Neustart
    wieder weg — gemeldet am 04.09.2026, „bei jedem Neustart ist der alte Pfad
    wieder drin".

    In der Zeiger-Datei steht **nur** dieses eine Feld. Zwei befuellte
    Einstellungsdateien wuerden garantiert auseinanderlaufen, und gelesen wird
    hier ohnehin nichts anderes.
    """
    ziel = zeiger_datei()
    vorhanden = {}
    try:
        with open(ziel, encoding='utf-8') as f:
            geladen = json.load(f)
        if isinstance(geladen, dict):
            vorhanden = geladen
    except Exception:
        pass
    vorhanden['ablage_ordner'] = wert
    temp = ziel + '.tmp'
    try:
        os.makedirs(os.path.dirname(ziel), exist_ok=True)
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump(vorhanden, f, ensure_ascii=False, indent=2)
        os.replace(temp, ziel)
        return True
    except OSError as ausnahme:
        _melden('pfade.ablage_ordner_setzen', ausnahme)
        return False


def einstellung_setzen(name, wert):
    """Einen Pfad dauerhaft merken — ohne die Erklärzeilen zu verlieren.

    Gelesen wird die vorhandene Datei (oder die Vorlage), geändert nur das eine
    Feld. So bleiben die Hinweise mit den Suchorten stehen, auch wenn das
    Programm die Datei schreibt."""
    if name == 'ablage_ordner':
        # ⚠ Sonderweg, siehe `_ablage_ordner_setzen`: Dieses eine Feld gehoert
        # in die Zeiger-Datei, sonst wirkt die Umstellung nur bis zum Neustart.
        return _ablage_ordner_setzen(wert)
    daten = einstellungen() or _vorlage()
    daten[name] = wert
    ziel = app_datei(EINSTELLUNGEN)
    temp = ziel + '.tmp'
    try:
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump(daten, f, ensure_ascii=False, indent=2)
        os.replace(temp, ziel)
        return True
    except OSError as ausnahme:
        # ⚠ Niemand prüft den Rückgabewert dieser Funktion — geprüft am
        # 26.08.2026, alle Aufrufer werfen ihn weg. Scheitert das Schreiben
        # (volle Platte, fehlende Rechte, Ordner weg), wäre die Einstellung
        # nach dem Neustart einfach wieder alt, ohne jeden Hinweis.
        _melden('pfade.einstellungen_schreiben', ausnahme)
        return False


def einstellung_zahl(name, standard, kleinstes=None, groesstes=None):
    """Eine Zahl aus den Einstellungen, mit Grenzen.

    Unsinnige Werte werden auf den erlaubten Bereich gezogen statt abgelehnt:
    Wer 0 einträgt, meint „so oft wie möglich" und soll kein Programm bekommen,
    das die Platte durchdreht — aber auch keine Fehlermeldung."""
    wert = einstellungen().get(name)
    try:
        zahl = int(wert)
    except (TypeError, ValueError):
        return standard
    if kleinstes is not None:
        zahl = max(kleinstes, zahl)
    if groesstes is not None:
        zahl = min(groesstes, zahl)
    return zahl


def einstellung_wahrheit(name, standard):
    """Ein Ja/Nein aus den Einstellungen. Fehlt es, gilt der Standard."""
    wert = einstellungen().get(name)
    if isinstance(wert, bool):
        return wert
    if isinstance(wert, str):
        return wert.strip().lower() in ('ja', 'yes', 'true', '1', 'an', 'on')
    return standard


def vorlage_anlegen():
    """Legt `einstellungen.json` zum Ausfüllen an, falls sie noch fehlt.

    Passiert genau dann, wenn das Spiel nicht gefunden wurde: Dann braucht der
    Nutzer die Datei, und sie soll schon dastehen, statt dass er sie nach
    Anleitung selbst erzeugen muss."""
    ziel = app_datei(EINSTELLUNGEN)
    if os.path.exists(ziel):
        return ziel
    try:
        with open(ziel, 'w', encoding='utf-8') as f:
            json.dump(_vorlage(), f, ensure_ascii=False, indent=2)
    except OSError:
        pass
    return ziel


# --------------------------------------------------------- 2. Star Citizen selbst
def _wine_praefixe():
    """Mögliche Wine-Präfixe unter Linux — die Orte, an denen die verbreiteten
    Installationswege (lug-helper, Lutris, Bottles, Heroic) landen. Reihenfolge
    ist Absicht: der lug-helper-Standard zuerst, er ist unter Linux der übliche Weg."""
    heim = os.path.expanduser('~')
    fest = [
        os.path.join(heim, 'Games', 'star-citizen'),
        os.path.join(heim, '.wine'),
        os.path.join(heim, 'Games', 'star-citizen-live'),
    ]
    muster = [
        os.path.join(heim, '.local', 'share', 'lutris', 'prefixes', '*'),
        os.path.join(heim, '.var', 'app', 'net.lutris.Lutris', 'data', 'lutris',
                     'prefixes', '*'),
        os.path.join(heim, '.local', 'share', 'bottles', 'bottles', '*'),
        os.path.join(heim, 'Games', '*'),
    ]
    gefunden = list(fest)
    for m in muster:
        gefunden.extend(sorted(glob.glob(m)))
    # Doppelte raus, Reihenfolge behalten
    gesehen, ergebnis = set(), []
    for p in gefunden:
        if p not in gesehen:
            gesehen.add(p)
            ergebnis.append(p)
    return ergebnis


def _spiel_wurzeln():
    """Verzeichnisse, unter denen `Roberts Space Industries\\StarCitizen` liegen kann."""
    if WINDOWS:
        wurzeln = []
        for laufwerk in 'CDEFGH':
            for programme in ('Program Files', 'Program Files (x86)'):
                wurzeln.append('%s:\\%s' % (laufwerk, programme))
        return wurzeln
    wurzeln = []
    for praefix in _wine_praefixe():
        c = os.path.join(praefix, 'drive_c')
        if not os.path.isdir(c):
            continue
        wurzeln.append(os.path.join(c, 'Program Files'))
        wurzeln.append(os.path.join(c, 'Program Files (x86)'))
        wurzeln.append(c)                      # manche installieren direkt nach C:\
    return wurzeln


def spiel_ordner():
    """Ordner des Spielkanals (enthält die Game.log) oder None.

    Erst die Umgebungsvariable, dann die üblichen Orte. Es wird nur nachgesehen,
    ob die Game.log dort liegt — geraten wird nicht."""
    for eigen in (os.environ.get('SC_INSTALL_DIR'), einstellung('spiel_ordner')):
        if not eigen:
            continue
        eigen = os.path.expanduser(eigen)
        if os.path.isfile(os.path.join(eigen, 'Game.log')):
            return eigen
        for k in KANAELE:                      # auch der Ordner darüber ist erlaubt
            p = os.path.join(eigen, k)
            if os.path.isfile(os.path.join(p, 'Game.log')):
                return p
        # ⚠ Und **neben** ihm. Wird LIVE in HOTFIX umbenannt (der uebliche Weg
        # zu einer ausgebesserten Fassung, siehe KANAELE), zeigt der eingetragene
        # Pfad ins Leere, waehrend der Nachbarordner danebensteht. Ohne diesen
        # Zweig fand nur derjenige sein Spiel wieder, der es am Standardort
        # installiert hat — wer es auf einer zweiten Platte liegen hat, stand
        # ohne Grund vor „Star Citizen nicht gefunden".
        eltern = os.path.dirname(eigen.rstrip(os.sep))
        if eltern and os.path.isdir(eltern):
            geschwister = []
            for k in KANAELE:
                p = os.path.join(eltern, k)
                log = os.path.join(p, 'Game.log')
                if not os.path.isfile(log):
                    continue
                try:
                    geschwister.append((os.path.getmtime(log), p))
                except OSError:
                    geschwister.append((0.0, p))
            if geschwister:
                # Der zuletzt bespielte Kanal gewinnt — gemessen, nicht geraten.
                return max(geschwister)[1]
    for wurzel in _spiel_wurzeln():
        basis = os.path.join(wurzel, SC_UNTERPFAD)
        if not os.path.isdir(basis):
            continue
        for k in KANAELE:
            p = os.path.join(basis, k)
            if os.path.isfile(os.path.join(p, 'Game.log')):
                return p
    return None


def _kanal_basen():
    """Ordner, in denen die Kanäle nebeneinander liegen können.

    Der eingetragene Spielordner zuerst: Wer sein Spiel auf einer anderen Platte
    hat, findet sich in den Standardorten nicht wieder — sein Nachbarkanal liegt
    aber genau eine Ebene über dem, was er eingetragen hat.
    """
    basen = []
    for eigen in (os.environ.get('SC_INSTALL_DIR'), einstellung('spiel_ordner')):
        if not eigen:
            continue
        eigen = os.path.expanduser(eigen).rstrip(os.sep)
        # Der eingetragene Pfad kann der Kanal selbst sein oder der Ordner
        # darüber — beide Deutungen kommen mit hinein, doppelt schadet nicht.
        for kandidat in (os.path.dirname(eigen), eigen):
            if kandidat and kandidat not in basen:
                basen.append(kandidat)
    for wurzel in _spiel_wurzeln():
        basis = os.path.join(wurzel, SC_UNTERPFAD)
        if basis not in basen:
            basen.append(basis)
    return basen


def kanaele_vorhanden():
    """Jeder Spielkanal, in dem wirklich eine Game.log liegt — neueste zuerst.

    Liefert Tupel `(Kanalname, Ordner, Zeitstempel der Game.log)`.

    ⚠ Sortiert wird nach dem **Alter der Game.log**, nicht nach der Reihenfolge
    in `KANAELE`. Wer zuletzt gespielt hat, hat dort die frischeste Datei — das
    ist eine gemessene Tatsache und keine Annahme darüber, was jemand „meint".
    """
    gefunden = {}
    for basis in _kanal_basen():
        if not os.path.isdir(basis):
            continue
        for k in KANAELE:
            ordner = os.path.join(basis, k)
            log = os.path.join(ordner, 'Game.log')
            if not os.path.isfile(log):
                continue
            if ordner in gefunden:
                continue
            try:
                stempel = os.path.getmtime(log)
            except OSError:
                stempel = 0.0
            gefunden[ordner] = (k, ordner, stempel)
    return sorted(gefunden.values(), key=lambda e: e[2], reverse=True)


def kanal_abweichung():
    r"""Es wird aus einem anderen Kanal gelesen als eingetragen ist — oder None.

    ⚠⚠ **Der Fall, für den es das gibt.** Legt CIG eine ausgebesserte Fassung
    neben LIVE, ist der übliche Weg dorthin nicht ein zweiter 100-GB-Download,
    sondern: den vorhandenen LIVE-Ordner in HOTFIX umbenennen, damit der
    Launcher nur die Unterschiede holt. Von einem Moment auf den anderen gibt es
    den eingetragenen Ordner also nicht mehr.

    Ohne diese Meldung merkt das niemand: Der Watcher findet den Nachbarkanal
    zwar von allein, liest dann aber stillschweigend woanders — oder er meldet
    „Star Citizen nicht gefunden", obwohl in den Einstellungen ein Pfad steht.
    Beides sieht für den Spieler nach einem kaputten Programm aus. Gemeldet von
    Haldjas am 03.09.2026, der genau das getan und es danach vergessen hatte.

    Gibt `(eingetragen, benutzt, alle_kanaele)` zurück; `benutzt` kann None
    sein, wenn gar nichts mehr gefunden wird.
    """
    eingetragen = einstellung('spiel_ordner')
    if not eingetragen:
        return None                       # nie etwas eingetragen: nichts zu melden
    eingetragen = os.path.expanduser(eingetragen).rstrip(os.sep)
    if os.path.isfile(os.path.join(eingetragen, 'Game.log')):
        return None                       # der eingetragene Ordner trägt noch
    kanaele = kanaele_vorhanden()
    if not kanaele:
        return None                       # gar kein Kanal da — das ist die
                                          # bekannte Meldung „nicht gefunden",
                                          # dafür braucht es keine Auswahl
    benutzt = spiel_ordner()
    return (eingetragen, benutzt, kanaele)


def spielordner_deuten(gewaehlt):
    """Aus einem vom Nutzer gewählten Ordner den tatsächlichen Spielordner machen.

    Nimmt ihm die Sucherei ab: Er darf den LIVE-Ordner treffen, den darüber
    (`StarCitizen`), den Programme-Ordner oder gleich das ganze Wine-Präfix —
    solange irgendwo darunter eine `Game.log` liegt, wird sie gefunden.
    Gibt den Ordner mit der Game.log zurück oder None."""
    if not gewaehlt:
        return None
    gewaehlt = os.path.expanduser(gewaehlt.strip().rstrip(os.sep)) or os.sep
    if os.path.isfile(gewaehlt):                 # jemand hat die Game.log selbst gewählt
        gewaehlt = os.path.dirname(gewaehlt)
    if os.path.isfile(os.path.join(gewaehlt, 'Game.log')):
        return gewaehlt
    # Eine Ebene tiefer: der Kanal (LIVE/PTU/…)
    for k in KANAELE:
        p = os.path.join(gewaehlt, k)
        if os.path.isfile(os.path.join(p, 'Game.log')):
            return p
    # Tiefer suchen, aber begrenzt — ein ganzes Laufwerk durchzugehen wäre
    # unhöflich. Vier Ebenen decken Wine-Präfix -> drive_c -> Programme ->
    # Roberts Space Industries -> StarCitizen -> LIVE ab.
    wurzel_tiefe = gewaehlt.rstrip(os.sep).count(os.sep)
    for basis, ordner, dateien in os.walk(gewaehlt):
        if basis.count(os.sep) - wurzel_tiefe > 5:
            ordner[:] = []
            continue
        if 'Game.log' in dateien:
            return basis
    return None



def _launcher_aus_registry():
    r"""Wo der RSI Launcher laut Windows installiert ist — oder None.

    ⚠ Feste Pfadlisten gehen genau dann schief, wenn jemand woanders
    installiert hat. Genau das ist am 26.08.2026 passiert: Auf einem fremden Rechner
    fehlte der Startknopf im Overlay, weil keiner der abgesuchten Orte passte.

    Der Eintrag in der Deinstallations-Liste ist verlässlicher, hat aber zwei
    Tücken, die beide geprüft sind:

    * **Der Schlüsselname ist eine GUID** und bei jeder Installation anders —
      es hilft nur, alle Einträge durchzugehen und den `DisplayName` zu prüfen.
    * **`InstallLocation` ist leer.** Der Pfad steckt statt dessen in
      `DisplayIcon`, das auf `…\RSI Launcher\uninstallerIcon.ico` zeigt. Aus
      dessen Ordner ergibt sich der Launcher.

    Gesucht wird in allen drei Zweigen — der Launcher trägt sich unter HKLM ein,
    aber eine Installation nur für den angemeldeten Nutzer landet unter HKCU.
    """
    if not WINDOWS:
        return None
    try:
        import winreg
    except ImportError:
        return None

    zweige = (
        (winreg.HKEY_LOCAL_MACHINE,
         r'SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall'),
        (winreg.HKEY_LOCAL_MACHINE,
         r'SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall'),
        (winreg.HKEY_CURRENT_USER,
         r'Software\Microsoft\Windows\CurrentVersion\Uninstall'),
    )
    for wurzel, pfad in zweige:
        try:
            with winreg.OpenKey(wurzel, pfad) as liste:
                anzahl = winreg.QueryInfoKey(liste)[0]
                for i in range(anzahl):
                    try:
                        name = winreg.EnumKey(liste, i)
                        with winreg.OpenKey(liste, name) as eintrag:
                            gefunden = _launcher_aus_eintrag(winreg, eintrag)
                            if gefunden:
                                return gefunden
                    except OSError:
                        continue      # einzelner Eintrag unlesbar — weiter
        except OSError:
            continue                  # Zweig gibt es nicht
    return None


def _launcher_aus_eintrag(winreg, eintrag):
    """Aus einem Deinstallations-Eintrag den Launcher-Pfad ziehen — oder None."""
    def wert(feld):
        try:
            return str(winreg.QueryValueEx(eintrag, feld)[0] or '')
        except OSError:
            return ''

    if 'rsi launcher' not in wert('DisplayName').lower():
        return None

    kandidaten = []
    ort = wert('InstallLocation').strip('" ')
    if ort:
        kandidaten.append(os.path.join(ort, 'RSI Launcher.exe'))
    # `DisplayIcon` zeigt auf eine Datei **im** Launcher-Ordner.
    symbol = wert('DisplayIcon').split(',')[0].strip('" ')
    if symbol:
        kandidaten.append(os.path.join(os.path.dirname(symbol),
                                       'RSI Launcher.exe'))
    for pfad in kandidaten:
        if pfad and os.path.isfile(pfad):
            return pfad
    return None



def saubere_umgebung():
    """Umgebung für fremde Programme — ohne unsere eigenen Bibliothekspfade.

    ⚠ Das ist im AppImage entscheidend. Dort zeigen `LD_LIBRARY_PATH`,
    `PYTHONHOME` und `PYTHONPATH` in das entpackte Paket. Startet man daraus ein
    Systemprogramm wie `zenity`, lädt es unsere mitgelieferten Bibliotheken statt
    seiner eigenen und stirbt sofort — der Dialog erscheint nicht, und für den
    Nutzer sieht es aus, als täte der Knopf nichts. AppImage legt die
    ursprünglichen Werte unter `*_ORIG` ab; die gelten hier wieder.
    """
    umgebung = dict(os.environ)
    for name in ('LD_LIBRARY_PATH', 'PYTHONHOME', 'PYTHONPATH',
                 'PYTHONDONTWRITEBYTECODE', 'QT_PLUGIN_PATH', 'GTK_PATH',
                 'GDK_PIXBUF_MODULE_FILE', 'GI_TYPELIB_PATH', 'XDG_DATA_DIRS',
                 'PERLLIB', 'GSETTINGS_SCHEMA_DIR'):
        urspruenglich = umgebung.pop(name + '_ORIG', None)
        if urspruenglich:
            umgebung[name] = urspruenglich
        else:
            umgebung.pop(name, None)
    return umgebung


def browser_befehle(adresse):
    """Die Wege, eine Adresse zu öffnen — in der Reihenfolge, in der es
    versucht wird. Getrennt von `im_browser()`, damit sich das **prüfen** lässt,
    ohne dabei einen Browser aufzureißen: Ein Prüflauf, der Fenster öffnet,
    reißt den Tastaturfokus mit und wirft den Spieler aus dem Spiel.
    """
    if sys.platform.startswith('linux'):
        return [['xdg-open', adresse], ['gio', 'open', adresse]]
    if sys.platform == 'darwin':
        return [['open', adresse]]
    return []                      # Windows: `webbrowser` macht es richtig


def im_browser(adresse):
    """Eine Adresse im Browser aufmachen. Gibt zurück, ob es geklappt hat.

    ⚠⚠ **Warum nicht einfach `webbrowser.open()`.** Im AppImage zeigen
    `LD_LIBRARY_PATH` und `PYTHONHOME` in unser entpacktes Paket. Jedes daraus
    gestartete Systemprogramm lädt unsere Bibliotheken statt seiner eigenen und
    stirbt sofort — `webbrowser.open()` meldet trotzdem Erfolg, weil es nur das
    Starten prüft, nicht das Überleben. Für den Nutzer sieht das aus, als täte
    der Knopf **gar nichts**: Genau so gemeldet am 30.08.2026 für „Kaffee
    spendieren" und „Discord", und im Fehlerbericht stand dazu **keine Zeile**,
    weil auch keine Ausnahme flog.

    Deshalb: `xdg-open` selbst starten, mit der Umgebung von `saubere_umgebung()`,
    und kurz nachsehen, ob es überlebt. Erst danach `webbrowser` als Rückfall.
    """
    import subprocess
    import webbrowser
    umgebung = saubere_umgebung()
    for befehl in browser_befehle(adresse):
        try:
            lauf = subprocess.Popen(befehl, env=umgebung,
                                    stdout=subprocess.DEVNULL,
                                    stderr=subprocess.DEVNULL)
        except OSError:
            continue               # Programm gibt es hier nicht — nächstes
        try:
            # Läuft es nach einer knappen Sekunde noch, hat es die Adresse
            # angenommen. Beendet es sich mit einem Fehler, war es nichts.
            if lauf.wait(1.0) != 0:
                continue
        except subprocess.TimeoutExpired:
            pass
        return True
    alt = dict(os.environ)
    try:
        os.environ.clear()
        os.environ.update(umgebung)
        return bool(webbrowser.open(adresse))
    except Exception:
        return False
    finally:
        os.environ.clear()
        os.environ.update(alt)


def spielstarter():
    """Womit sich Star Citizen starten lässt — oder `None`.

    Auf beiden Systemen dieselbe Frage, nur ein anderer Ort:

    * **Windows** — der RSI Launcher unter `%LOCALAPPDATA%\\Programs`. Das ist
      derselbe Weg, den auch der SC-Deutsch-Launcher geht.
    * **Linux** — das Startskript `sc-launch.sh` im Wine-Präfix, das der
      `lug-helper` der Star Citizen Linux Users Group beim Einrichten anlegt.
      Es bringt Präfix, Wine-Version und Umgebung schon mit. Ein eigener
      Wine-Aufruf wäre hier falsch. **Nicht** der `lug-helper` selbst: Der
      verwaltet nur und kann das Spiel gar nicht starten (siehe unten).

    Ein eigener Weg geht über die Einstellung `spielstarter` — wer Lutris oder
    Heroic benutzt, trägt dort seinen Startbefehl ein.

    Wird nichts gefunden, gibt es auch keinen Knopf. Ein Knopf, der nichts tut,
    ist schlimmer als keiner.
    """
    eigen = (einstellung('spielstarter') or '').strip()
    if eigen:
        return eigen if os.path.exists(os.path.expanduser(eigen)) else eigen

    if WINDOWS:
        orte = []

        # ⚠ **Zuerst neben dem Spielordner suchen** — das ist der einzige Ort,
        # den wir sicher kennen. Der Launcher legt sich standardmäßig neben die
        # Spielinstallation:
        #
        #     …\Roberts Space Industries\StarCitizen\LIVE   ← das Spiel
        #     …\Roberts Space Industries\RSI Launcher\      ← der Launcher
        #
        # Vorher wurden nur feste Orte unter %LOCALAPPDATA% und %PROGRAMFILES%
        # abgesucht. Bei Haldjas liegt das Spiel in
        # `C:\Program Files\Roberts Space Industries\…` — der Launcher damit an
        # einer Stelle, die nicht in der Liste stand, und der Knopf erschien gar
        # nicht erst: „nicht sicher wo sich die funktion versteckt, aber ich hab
        # sie nicht gefunden" (25.08.2026).
        #
        # Vom Spielordner aus zu suchen trifft jede Installation, egal wohin sie
        # gelegt wurde — statt immer neue feste Pfade nachzutragen.
        spiel = spiel_ordner()
        if spiel:
            # …\StarCitizen\LIVE  →  zwei Ebenen hoch  →  …\Roberts Space Industries
            rsi = os.path.dirname(os.path.dirname(spiel))
            orte.append(os.path.join(rsi, 'RSI Launcher', 'RSI Launcher.exe'))
            # Eine Ebene weiter hoch, falls jemand ohne Zweig-Ordner installiert
            orte.append(os.path.join(os.path.dirname(rsi), 'RSI Launcher',
                                     'RSI Launcher.exe'))

        # ⚠ **Vor** den festen Orten: Was Windows selbst weiss, schlaegt jede
        # Liste. Wer den Launcher auf ein anderes Laufwerk gelegt hat, faellt
        # sonst durch — genau so fehlte auf einem fremden Rechner der Startknopf.
        aus_registry = _launcher_aus_registry()
        if aus_registry:
            orte.append(aus_registry)


        for umgebung in ('LOCALAPPDATA', 'PROGRAMFILES', 'PROGRAMW6432'):
            wurzel = os.environ.get(umgebung)
            if not wurzel:
                continue
            orte.append(os.path.join(wurzel, 'Programs', 'RSI Launcher',
                                     'RSI Launcher.exe'))
            orte.append(os.path.join(wurzel, 'RSI Launcher',
                                     'RSI Launcher.exe'))
            orte.append(os.path.join(wurzel, 'Roberts Space Industries',
                                     'RSI Launcher', 'RSI Launcher.exe'))
        for ort in orte:
            if os.path.isfile(ort):
                return ort
        return None

    # ⚠ **Der `lug-helper` startet das Spiel nicht.** Hier stand er trotzdem —
    # gefunden wurde er auch zuverlässig, nur ist er das falsche Programm. Sein
    # `--help` kennt Präfix, Wine-Runner, DXVK und Launcher-Reparatur und
    # **keine einzige Startoption**; ohne Argumente öffnet er sein
    # Zenity-Verwaltungsmenü.
    #
    # Aufgefallen am 27.08.2026: Bomb20 meldete „das Starten von SC klappt
    # nicht", der Autor sah dasselbe auf einem System, auf dem der Helper unter
    # `/usr/bin/lug-helper` liegt — Knopf da, Meldung „Star Citizen wird
    # gestartet …", und nichts geschah.
    #
    # Gestartet wird über das Skript, das der Helper anlegt: `sc-launch.sh`,
    # direkt im Wine-Präfix. Und das Präfix steht immer über dem Spielordner:
    #
    #     ~/Games/star-citizen/                   ← Präfix, hier liegt das Skript
    #     ~/Games/star-citizen/drive_c/Program Files/…/StarCitizen/LIVE
    #
    # Deshalb derselbe Gedanke wie im Windows-Zweig oben: **vom Spielordner aus
    # suchen**, statt Orte zu raten. Über `drive_c` liegt das Präfix, egal wohin
    # jemand installiert hat und wie er den Helper eingerichtet hat.
    SKRIPT = 'sc-launch.sh'
    orte = []
    spiel = spiel_ordner()
    if spiel:
        pfad = os.path.abspath(spiel)
        # Hochsteigen, bis `drive_c` erreicht ist — eine Ebene darüber liegt das
        # Präfix. Die Schleife endet spätestens an der Wurzel.
        while True:
            eltern = os.path.dirname(pfad)
            if eltern == pfad:
                break
            if os.path.basename(pfad).lower() == 'drive_c':
                orte.append(os.path.join(eltern, SKRIPT))
                break
            pfad = eltern

    # Rückfall: der Standardort des LUG Helper, falls der Spielordner (noch)
    # nicht bekannt ist.
    heim = os.path.expanduser('~')
    orte.append(os.path.join(heim, 'Games', 'star-citizen', SKRIPT))

    for ort in orte:
        if os.path.isfile(ort) and os.access(ort, os.X_OK):
            return ort

    # ⚠ Bewusst **kein** Rückfall auf `lug-helper`: Er würde gefunden, der Knopf
    # erschiene — und täte wieder nichts. „Ein Knopf, der nichts tut, ist
    # schlimmer als keiner" gilt auch hier. Wer über Lutris oder Heroic spielt,
    # trägt seinen Startbefehl in der Einstellung `spielstarter` ein.
    return None


def _startbefehl(starter):
    """Aus dem eingetragenen Text die Liste machen, die `Popen` braucht.

    ⚠ **Ein Startbefehl ist nicht immer ein Dateiname.** Hier stand
    `Popen([starter])` — damit gilt der ganze Text als **eine** Datei. Wer
    `flatpak run org.starcitizen-lug.Helper` oder `lutris rungame/star-citizen`
    einträgt, bekommt „Datei nicht gefunden", weil nach einer Datei mit
    Leerzeichen im Namen gesucht wird.

    `shlex.split` zerlegt so, wie eine Shell es täte — samt
    Anführungszeichen für Pfade mit Leerzeichen.

    ⚠ **Eine vorhandene Datei wird NICHT zerlegt.** Sonst zerfiele
    `/home/ich/Meine Spiele/sc-launch.sh` in drei Teile. Erst wenn es die Datei
    so nicht gibt, ist es ein Befehl mit Argumenten.
    """
    if os.path.exists(starter):
        return [starter]
    try:
        import shlex
        teile = shlex.split(starter, posix=not WINDOWS)
    except ValueError:
        return [starter]              # unpaariges Anführungszeichen o. ä.
    return teile or [starter]


def spiel_starten():
    """Star Citizen starten. Gibt (True, '') oder (False, Grund) zurück."""
    starter = spielstarter()
    if not starter:
        # ⚠ Dieser Grund landet über `s_sp_start_nein` sichtbar in der
        # Statuszeile — also übersetzen. `sprache` lokal holen: `pfade` wird
        # sehr früh geladen, ein Import oben wäre ein Zirkelbezug.
        from . import sprache
        return False, sprache.t('s_sp_kein_starter')
    try:
        import subprocess
        # Losgelöst starten: Der Watcher soll weiterlaufen und nicht am Spiel
        # hängen — und beim Beenden das Spiel nicht mitreißen.
        zusatz = {}
        if WINDOWS:
            zusatz['creationflags'] = getattr(subprocess, 'DETACHED_PROCESS', 0)
        else:
            zusatz['start_new_session'] = True
        subprocess.Popen(_startbefehl(starter), stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL, **zusatz)
        return True, ''
    except Exception as ausnahme:
        return False, str(ausnahme)


def game_log(ordner=None):
    """Pfad zur aktiven Game.log oder None."""
    ordner = ordner or spiel_ordner()
    if not ordner:
        return None
    p = os.path.join(ordner, 'Game.log')
    return p if os.path.isfile(p) else None


def log_sicherungen(ordner=None):
    """Die aufgehobenen Logs vergangener Sitzungen, älteste zuerst.

    Star Citizen legt bei jedem Spielstart die vorige Game.log unter
    `logbackups/` ab. Daraus lässt sich nachlesen, was ohne laufenden
    Watcher freigeschaltet wurde."""
    ordner = ordner or spiel_ordner()
    if not ordner:
        return []
    # Bewusst alles nehmen, was dort liegt: Star Citizen hat die Benennung der
    # Sicherungen über die Jahre mehrfach geändert (mal `Game.log.<Datum>`, mal
    # mit Endung dahinter). Ein Muster auf `*.log` verpasst dann die Hälfte.
    # Ausgenommen sind nur Dinge, die sicher kein Text sind.
    ausser = ('.zip', '.7z', '.gz', '.rar', '.dmp', '.mdmp', '.png', '.jpg')
    treffer = []
    for p in glob.glob(os.path.join(ordner, 'logbackups', '*')):
        if os.path.isfile(p) and not p.lower().endswith(ausser):
            treffer.append(p)
    return sorted(treffer, key=lambda p: (_mtime(p), p))


def _mtime(p):
    try:
        return os.path.getmtime(p)
    except OSError:
        return 0.0


def lokalisierung_ordner(ordner=None):
    """`data/Localization` im Spielordner — dort liegen die entpackten `global.ini`,
    sofern welche vorhanden sind (der SC Deutsch Launcher legt die deutsche dort ab)."""
    ordner = ordner or spiel_ordner()
    if not ordner:
        return None
    p = os.path.join(ordner, 'data', 'Localization')
    return p if os.path.isdir(p) else None


# ---------------------------------------------------- 3. SC Deutsch Launcher (optional)
def launcher_ordner():
    """Blueprint-Ordner des SC Deutsch Launchers oder None.

    Unter Windows liegt er in %APPDATA%. Unter Linux nur dann, wenn jemand den
    Launcher unter Wine betreibt — dann steckt dasselbe AppData im Wine-Präfix."""
    # Eine **gesetzte** Angabe gilt allein — auch wenn der Ordner dort nicht
    # existiert. Wer einen Pfad einträgt, will keine Suche woanders; sonst
    # nimmt das Programm klammheimlich einen anderen Launcher-Stand her als den
    # angegebenen. (Fiel im Selbsttest auf: Der baut eine Installation ohne
    # Launcher nach, bekam aber den echten von der Windows-Platte untergeschoben.)
    for eigen in (os.environ.get('SC_BP_LAUNCHER'), einstellung('launcher_ordner')):
        if eigen is not None and eigen != '':
            eigen = os.path.expanduser(eigen)
            return eigen if os.path.isdir(eigen) else None
    if os.environ.get('SC_BP_LAUNCHER') == '':
        return None                    # ausdrücklich abgeschaltet
    if WINDOWS:
        p = os.path.join(os.environ.get('APPDATA', ''), 'sc-deutsch-launcher',
                         'blueprints')
        return p if os.path.isdir(p) else None
    for praefix in _wine_praefixe():
        muster = os.path.join(praefix, 'drive_c', 'users', '*', 'AppData',
                              'Roaming', 'sc-deutsch-launcher', 'blueprints')
        for p in sorted(glob.glob(muster)):
            if os.path.isdir(p):
                return p
    # Dual-Boot: Der Launcher läuft unter Windows, seine Daten liegen auf der
    # Windows-Platte — die unter Linux meist eingehängt ist. Ohne diesen Blick
    # steht ein umgestiegener Spieler ohne seinen alten Bauplan-Stand da,
    # obwohl der zwei Ordner weiter vollständig vorliegt. Genau so passiert.
    for p in _windows_launcher():
        return p
    return None


def _windows_launcher():
    """Launcher-Daten auf einer eingehängten Windows-Platte."""
    heim = os.path.expanduser('~')
    orte = ['/run/media/*/*', '/media/*/*', '/mnt/*',
            os.path.join(heim, '.local', 'share', '*')]
    for ort in orte:
        muster = os.path.join(ort, 'Users', '*', 'AppData', 'Roaming',
                              'sc-deutsch-launcher', 'blueprints')
        for p in sorted(glob.glob(muster)):
            if os.path.isdir(p):
                yield p


def launcher_datei(name, ordner=None):
    """Pfad zu einer Launcher-Datei, auch wenn es den Launcher nicht gibt.

    Gibt immer einen Pfad zurück (nie None), damit die aufrufende Stelle wie
    bisher einfach versuchen kann, ihn zu öffnen. Ohne Launcher zeigt er ins
    Leere und das Öffnen scheitert — genau das ist gewollt."""
    ordner = ordner if ordner is not None else (launcher_ordner() or '')
    return os.path.join(ordner, name)


# ------------------------------------------------------------------ Übersicht
def kuerzen(text):
    """Persönliches aus einem Text nehmen — für Fehlerprotokoll und Bericht.

    Pfade verraten den Benutzernamen (`C:\\Users\\Spieler\\…`,
    `/home/spieler/…`), und genau solche Texte landen in einem **öffentlichen**
    Issue. Ersetzt werden das Heimatverzeichnis und danach jedes weitere
    Vorkommen des Benutzernamens.

    Lieber einmal zu viel ersetzt als ein Name zu viel im Netz.
    """
    try:
        text = str(text)
        heim = os.path.expanduser('~')
        name = os.path.basename(heim.rstrip('\\/'))

        for was in (heim, heim.replace('\\', '/'), heim.replace('/', '\\')):
            if was and len(was) > 3:
                text = text.replace(was, '<heim>')

        if name and len(name) > 2:
            text = re.sub(re.escape(name), '<benutzer>', text, flags=re.I)

        return _geheimnisse_kuerzen(text)
    except Exception:
        return str(text)


# Adressen, die ein Geheimnis IM PFAD tragen. Bei einem Discord-Webhook ist der
# hintere Teil der Schlüssel: Wer ihn hat, kann in den Melde-Kanal schreiben.
_WEBHOOK = re.compile(
    r'https://[\w.-]*discord(?:app)?\.com/api/webhooks/\S+', re.I)
# Und Parameter, die nach Zugang klingen. ⚠ Bewusst eng: `?id_category=3` und
# `?uuid=…` bleiben stehen — sie sagen, WELCHER Abruf schiefging, und ohne sie
# ist ein Netzfehler nicht mehr zu deuten.
_ZUGANG = re.compile(
    r'([?&](?:token|key|api[_-]?key|apikey|secret|auth|password|passwd|pw|'
    r'access[_-]?token|signature|sig)=)[^&\s]+', re.I)


def _geheimnisse_kuerzen(text):
    """Zugangsdaten aus einem Text nehmen, der öffentlich werden kann.

    ⚠⚠ **Warum das nötig ist — gemessen, nicht vermutet (05.09.2026).** Der
    Fehlerbericht landet in einem öffentlichen Issue. `absenden()` gibt den
    Grund eines gescheiterten Sendeversuchs bewusst nicht zurück, weil die
    Adresse geheim ist — schreibt die Ausnahme aber eine Zeile darüber mit
    `fehler.merken()` ins Protokoll, und das Protokoll steht im Bericht.

    Vier realistische Fehlerfälle durchgespielt: Drei sind harmlos (`urllib`
    nennt die Adresse nicht), einer nicht — jede Meldung, die eine Adresse
    selbst in ihren Text schreibt. Genau dieses Muster gibt es bereits: Die
    Netzabrufe hängen die abgerufene Adresse an ihre Meldung. Dort ist sie
    öffentlich und nützlich; beim Melde-Kanal wäre sie ein Schlüssel.

    ⚠ **Eng gefasst mit Absicht.** Eine Adresse im Fehlertext ist das
    Wertvollste am ganzen Eintrag — sie sagt, welcher Abruf schiefging.
    Gekürzt wird deshalb nur, was ein Zugang IST: der Webhook-Pfad und
    Parameter, die so heißen. Alles andere bleibt lesbar.
    """
    # ⚠⚠ **Zuerst die EIGENE Adresse — sie ist die einzige, die wirklich weh
    # tut.** Das Muster darunter erkennt Discord-Webhooks; steht in
    # `SC_BP_BERICHT_ZIEL` aber etwas anderes (ein eigener Dienst, eine
    # Weiterleitung), greift es nicht. Hier wird ersetzt, was tatsächlich
    # eingetragen ist, ganz gleich wie es aussieht.
    #
    # ⚠ Lokal importiert: `berichtziel` kommt ohne `pfade` aus, aber ein
    # Import auf Modulebene würde diese Reihenfolge für immer festschreiben.
    try:
        from . import berichtziel
        adresse = berichtziel.ziel()
        # Die Längenschwelle ist kein Schmuck: Ohne sie würde ein leeres Ziel
        # jede Stelle im Text treffen und den ganzen Bericht zerlegen.
        if adresse and len(adresse) > 12:
            text = text.replace(adresse, '<meldeadresse>')
    except Exception:
        pass                    # lieber ungekürzt als gar kein Bericht

    text = _WEBHOOK.sub('<meldeadresse>', text)
    return _ZUGANG.sub(r'\1<geheim>', text)


def uebersicht():
    """Was wurde gefunden — für Statusanzeige und Fehlersuche."""
    spiel = spiel_ordner()
    return {
        'system': 'Windows' if WINDOWS else sys.platform,
        'app_ordner': app_ordner(),
        'spiel_ordner': spiel,
        'game_log': game_log(spiel),
        'sicherungen': len(log_sicherungen(spiel)),
        'launcher': launcher_ordner(),
        'einstellungen': app_datei(EINSTELLUNGEN),
        'selbst_gesetzt': {k: v for k, v in einstellungen().items()
                           if not k.startswith('_') and v},
    }


if __name__ == '__main__':
    for k, v in uebersicht().items():
        print('%-14s %s' % (k, v))


# --------------------------------------------------------- Namen vergleichen
# Alle Anführungszeichen, die in Bauplan-Namen vorkommen — gerade,
# typografische und die französischen. Beim Vergleichen werden sie auf ein
# einfaches `'` gezogen.
#
# ⚠⚠ **Vollständig halten — eine Lücke fällt nie von allein auf.** Bis zum
# 30.08.2026 fehlte ausgerechnet das **öffnende** typografische
# Anführungszeichen `\u201c`. Aus `SW16BR1 “Buzzsaw” Repeater` wurde dadurch
# `sw16br1 “buzzsaw' repeater` — das schließende war angeglichen, das öffnende
# nicht. Drei Baupläne im Katalog tragen es, und keiner von ihnen konnte je zu
# einem Fund aus einer anderen Quelle passen. Gefunden beim Abgleich
# einer von Hand geführten Bauplanliste gegen den Katalog, nicht durch eine
# Meldung: Der Bauplan gilt einfach als „fehlt", und niemand kommt auf die Idee,
# dass ein Anführungszeichen daran schuld ist.
#
# Prüfung 80 im Selbsttest zieht alle hier gelisteten Zeichen durch
# `namensform()` und verlangt dasselbe Ergebnis.
ANFUEHRUNG = str.maketrans({
    '"': "'",
    '\u201c': "'",      # “  oeffnend, typografisch — fehlte bis 30.08.2026
    '\u201d': "'",      # ”  schliessend, typografisch
    '\u201e': "'",      # „  deutsches oeffnendes unten
    '\u2018': "'",      # ‘  einfach, oeffnend
    '\u2019': "'",      # ’  einfach, schliessend (auch Apostroph)
    '\u201a': "'",      # ‚  einfach unten
    '\u00ab': "'",      # «  franzoesisch
    '\u00bb': "'",      # »  franzoesisch
    '\u2039': "'",      # ‹  franzoesisch, einfach
    '\u203a': "'",      # ›  franzoesisch, einfach
})


# Der Klassen-Zusatz am Namensende — `7CA 'Nargun' (Civ/3/A)`, `XL-1 (Mil/2/A)`,
# `P4-AR Rifle (Bal)`, `'Arrow' I Missile (IR1)`.
#
# ⚠ **Warum das hierher gehört und nicht nur ins Log-Lesen.** Bis zum 28.08.2026
# schnitt nur `logquelle.teile_namen()` den Zusatz ab. Namen aus der
# **Launcher-Datei** und aus **Importen** (Basetool, scmdb, eigene Sicherung)
# gingen ungeschnitten in den Bestand — und `XL-1 (Mil/2/A)` findet `XL-1` nie.
# Der Bauplan galt als fehlend, obwohl er dastand.
#
# Aufgefallen an Morkhan: Er hatte die Baupläne gemeinsam mit dem Autor gefarmt,
# hatte den SC Deutsch Launcher mit gepflegter Datei — und im Spiel standen die
# Kästchen trotzdem leer. Gemeldet: „vergleich doch mal die Logik, was habe ich,
# mit meiner BP-Liste, und hör auf zu raten."
#
# Bewusst eng: Nur die bekannten Kürzel, damit echte Namensklammern wie
# `Singe Cannon (S2)` oder `(30 cap)` stehen bleiben. Die Liste muss zu
# `scbp/angaben.py` passen — Selbsttest 32 wacht darüber.
_KLASSEN_KURZ = ('civ|mil|ind|sth|cmp'
                 '|las|ele|pla|dis|mic|bal'
                 '|nah|min|slv|med|tool|trc')
KUERZEL_RE = re.compile(
    r'\s*\((?:(?:%s)/(?:\d{1,2}|\u2013|-)/(?:[a-d]|\u2013|-)'
    r'|(?:%s)'
    r'|(?:ir|em|cs)\d{1,2})\)\s*$' % (_KLASSEN_KURZ, _KLASSEN_KURZ))

# \u26a0\u26a0 **MrKraken StarStrings stellt dasselbe K\u00fcrzel VORAN \u2014 ohne Klammern.**
# Wer StarStrings einsetzt, hat im Spiel `Ind/2/B Citadel` stehen, wo der
# Katalog `Citadel` kennt; die uebliche Schreibweise waere `Citadel (Ind/2/B)`.
# `KUERZEL_RE` oben faengt nur die Klammerform. Die vorangestellte blieb stehen,
# der Name fand seinen Katalog-Eintrag nicht und galt als \u201enicht im Katalog".
#
# Gemessen am 04.09.2026 in der ausgelieferten StarStrings-Datei: **465**
# Eintraege in dieser Form \u2014 Kuehler, Schilde, Kraftwerke. Bei einem Melder
# waren vier von 26 Bauplaenen betroffen, also jeder sechste.
#
# \u26a0 Es liegt NICHT an der Spielsprache. Das war der erste Verdacht (der Melder
# spielt auf Englisch), und er war falsch \u2014 er benutzt StarStrings, und das
# schreibt die Namen so. Wer hier etwas aendert, prueft es an der echten Datei
# nach (`tools/starstrings_pruefen.py` zeigt, woher sie kommt).
#
# Genauso eng gehalten wie oben: nur die bekannten Kuerzel, nur am Anfang, und
# es muss ein Leerzeichen samt Namen folgen. `Ind/2/B` allein bleibt stehen \u2014
# ein Name, der nur aus dem Kuerzel besteht, waere sonst leer.
KUERZEL_VORN_RE = re.compile(
    r'^\s*(?:%s)/(?:\d{1,2}|\u2013|-)/(?:[a-d]|\u2013|-)\s+(?=\S)'
    % _KLASSEN_KURZ)


# Die Mengenangabe am Namensende — `(16 cap)`, `(16 Schuss)`, `(40 rounds)`.
#
# ⚠ **Warum das Wort weg muss und die Zahl bleibt.** Der SC Deutsch Launcher
# liest den **englischen** Katalog und schreibt `Ravager-212 Twin Shotgun
# Magazine (16 cap)`. Die **Log-Nachlese** liest dieselbe Kiste in der Sprache,
# in der das Spiel laeuft — bei der Autor auf Deutsch: `... (16 Schuss)`.
# Ergebnis: derselbe Bauplan zweimal im Bestand, und die angezeigte Zahl ist zu
# hoch. Gemessen am 29.08.2026 an einem echten Bestand: 405 angezeigt, 403 echt.
#
# Die **Zahl** ist Teil der Identitaet — ein 40er- und ein 60er-Magazin sind
# verschiedene Bauplaene. Deshalb wird `(16 Schuss)` zu `(16)`, nicht zu nichts.
# Und es greift nur, wenn die Klammer mit einer Ziffer beginnt und danach ein
# Wort folgt: `Singe Cannon (S2)` und `(1/A)` bleiben unangetastet.
MENGE_RE = re.compile(r'\((\d+)\s+[^)]*\)')


def namensform(s):
    """Ein Bauplan-Name als Vergleichsschlüssel — die EINZIGE Stelle dafür.

    ⚠ Diese Funktion stand dreimal im Programm: in `bestand.py`, `katalog.py`
    und `merkliste.py`. Der Kommentar in `bestand.py` behauptete „identisch zum
    Hauptprogramm" — und war es nicht mehr. Wer eine davon anfasst, verschiebt
    stillschweigend, welche Baupläne noch zueinander finden.

    Angeglichen wird viererlei:

    * **Groß- und Kleinschreibung.**
    * **Geschützte und kaputte Leerzeichen** (`\xa0`, `\ufffd`).
    * **Anführungszeichen.** Der SC Deutsch Launcher exportiert
      `7MA "Lorica"` mit geraden doppelten, scmdb führt denselben Bauplan als
      `7MA 'Lorica'` mit einfachen. Ohne Angleichung sind das zwei Schlüssel,
      und der Bauplan gilt als „fehlt", obwohl er im Bestand steht. Gefunden
      an einem echten Bestand mit 392 Bauplänen — genau einer fiel durch.
    * **Die Sprache der Mengenangabe** (`(16 cap)` ↔ `(16 Schuss)`) — siehe
      `MENGE_RE` oben. Die Zahl bleibt stehen, nur das Wort faellt weg.
    """
    return MENGE_RE.sub(r'(\1)',
                        KUERZEL_VORN_RE.sub(
                            '',
                            KUERZEL_RE.sub('', str(s).lower()
.replace('\xa0', ' ')
.replace('\ufffd', ' ')
.translate(ANFUEHRUNG)).strip())).strip()
