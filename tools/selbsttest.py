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
Selbsttest — prüft die Erkennung ohne Star Citizen und ohne den Launcher.

Baut in einem Wegwerf-Ordner eine Spielinstallation nach (Game.log plus zwei
aufgehobene Sitzungen), lässt den Watcher darauf los und vergleicht, was
herauskommt. Nichts davon fasst echte Daten an.

Aufruf:
    python3 tools/selbsttest.py

Sinn der Sache: Die Erkennung hat ein paar Fallstricke, die man beim Lesen des
Codes nicht sieht und die schon einmal Fehler verursacht haben — abgeschnittene
Namensklammern, doppelt gezählte Meldungen, verlorene Lesestände. Sie stehen
hier als Fälle drin, damit ein Umbau sie nicht unbemerkt wieder einreißt.
"""
import importlib
import json
import io
import os
import re
import shutil
import subprocess
import sys
import time as _zeit
import tempfile
import time

WURZEL = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, WURZEL)

# Prueflaeufe bauen echte Fenster. Ohne diese Umleitung blitzen sie ueber
# einem laufenden Spiel auf und reissen den Fokus mit — siehe unsichtbar.py.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import unsichtbar                                              # noqa: E402
unsichtbar.sicherstellen()


# Die Zeilen, wie Star Citizen sie wirklich schreibt.
def zeile(text, nummer=1, art='Added'):
    return ('<2026-08-20T21:23:49.123Z> [Notice] <SHUDEvent_OnNotification> '
            '%s notification "%s: " [%d] to queue. '
            '[Team_CoreGameplayFeatures][Missions][Comms]\n' % (art, text, nummer))


SITZUNG_1 = [
    zeile('Bauplan erhalten: Attrition-5 Repeater', 1),
    # Schiffskomponente mit Klassen-Zusatz — der muss abgeschnitten werden
    zeile("Bauplan erhalten: 7CA 'Nargun' (Civ/3/A)", 2),
    # Dieselbe Meldung als Ausblende-Ereignis — darf NICHT doppelt zählen
    zeile("Bauplan erhalten: 7CA 'Nargun' (Civ/3/A)", 2, art='Removed'),
    # Echte Namensklammer — die muss stehen bleiben
    zeile('Bauplan erhalten: Arclight Pistol Battery (30 cap)', 3),
    # Anführungszeichen mitten im Namen
    zeile('Bauplan erhalten: CF-117 Bulldog "Hazard-Zone" Repeater', 4),
    # Andere Meldung — geht uns nichts an
    zeile('Mission abgeschlossen: Irgendwas', 5),
]
SITZUNG_2 = [
    zeile('Blueprint Received: Singe Cannon (S2)', 1),        # englischer Client
    zeile('Bauplan erhalten: Attrition-5 Repeater', 2),       # Dublette über Dateien
]
LAUFEND = [zeile('Bauplan erhalten: Scalpel Sniper Rifle Magazine (12 Schuss)', 1)]

ERWARTET = {
    'attrition-5 repeater',
    "7ca 'nargun'",
    # ⚠ `(30)` statt `(30 cap)`: `pfade.namensform()` laesst die ZAHL stehen und
    # wirft nur das Wort weg — sonst waeren `(16 cap)` (Launcher, englisch) und
    # `(16 Schuss)` (Log-Nachlese, deutsch) zwei Eintraege fuer dieselbe Kiste.
    'arclight pistol battery (30)',
    # ⚠ Einfache Anführungszeichen, obwohl die Log-Zeile oben doppelte hat:
    # `pfade.namensform()` zieht alle Anführungszeichen auf ein einfaches `'`,
    # damit derselbe Bauplan aus Launcher-Export und scmdb-Katalog denselben
    # Schlüssel bekommt.
    "cf-117 bulldog 'hazard-zone' repeater",
    'singe cannon (s2)',
    # ⚠ `(12)`, nicht `(12 schuss)`: Die Log-Zeile oben ist DEUTSCH — genau der
    # Fall, der den Bauplan frueher doppelt in den Bestand gelegt hat, weil der
    # Launcher dieselbe Kiste als `(12 cap)` fuehrt.
    'scalpel sniper rifle magazine (12)',
}

fehler = []
# ⚠ Die Bilanz am Ende sagte immer „N von N fehlgeschlagen", weil sie die
# Gesamtzahl aus der Fehlerzahl selbst errechnete (`len(fehler) + 0`). Ein
# einzelner Fehler unter zweihundert Prüfungen las sich damit als „1 von 1" —
# also als hätte gar nichts geklappt. Deshalb wird jetzt wirklich gezählt.
geprueft = [0]


def hat_anzeige():
    """Lässt sich hier überhaupt ein Fenster öffnen?

    Auf einem Bau-Rechner gibt es keinen Bildschirm — dort scheitert schon
    `tk.Tk()` mit „no display name and no $DISPLAY". Die Erkennung, der Bestand
    und die Pfade brauchen kein Fenster; nur die paar Prüfungen, die eines
    aufmachen, werden dann übersprungen statt den ganzen Lauf zu versenken."""
    try:
        import tkinter
        r = tkinter.Tk()
        r.withdraw()
        r.destroy()
        return True
    except Exception:
        return False


def uebersprungen(was, grund='kein Bildschirm vorhanden'):
    print('  [--]   %s — übersprungen (%s)' % (was, grund))


def pruefe(bedingung, was):
    geprueft[0] += 1
    print(('  [ok]   ' if bedingung else '  [FEHL] ') + was)
    if not bedingung:
        fehler.append(was)


# ---------------------------------------------------------------------------
# ⚠ Am 28.08.2026 stand in `release.yml` zweimal `shell: bash` untereinander.
# YAML verbietet denselben Schlüssel zweimal in einer Map — GitHub lehnte die
# **ganze Datei** ab. Folge: Jeder Bau brach nach 0 Sekunden ab („workflow file
# issue"), über eine Stunde lang unbemerkt, weil niemand hinsah. Die Commits
# von 00:03 bis 00:57 wurden nie gebaut.
#
# Genau die Sorte Fehler, die der Selbsttest sonst sichtbar macht — nur prüfte
# er die Workflow-Dateien nicht.
#
# ⚠ Und PyYAML hilft hier NICHT: `safe_load` meldet doppelte Schlüssel nicht,
# es nimmt still den letzten Wert. Gemessen, nicht vermutet. Also von Hand über
# die Zeilen — was zugleich heißt: keine Fremdbibliothek, die fehlen kann.
_YAML_SCHLUESSEL = re.compile(r'^(\s*)(-\s+)?([A-Za-z_][\w.\- ]*):(\s|$)')
_YAML_BLOCK = re.compile(
    r'^(\s*)(?:-\s+)?[A-Za-z_][\w.\- ]*:\s*[|>][-+]?\d*\s*(?:#.*)?$')


def doppelte_schluessel(text):
    """Schlüssel, die in derselben Map zweimal stehen. [(zeile, name, erste), …]

    Zwei Fallen, an denen eine naive Zeilensuche scheitert:

    1. **Listeneinträge sind eigene Maps.** In `steps:` darf `name` bei jedem
       Schritt wieder stehen — ein `- ` beginnt eine neue Map, der Zähler wird
       zurückgesetzt.
    2. **Textblöcke enthalten alles Mögliche.** Hinter `run: |` stehen bei uns
       Shell- und Python-Zeilen, Heredocs inklusive; `on: 1` darin ist Text,
       kein Schlüssel. Alles, was tiefer eingerückt ist als der Blockschlüssel,
       wird deshalb übersprungen.
    """
    funde = []
    stapel = []          # [(einrueckung, {schluessel: zeile}), …]
    block_ein = None     # in einem |- oder >-Textblock: dessen Einrückung
    for nr, zeile in enumerate(text.splitlines(), 1):
        if block_ein is not None:
            if not zeile.strip():
                continue
            if len(zeile) - len(zeile.lstrip()) > block_ein:
                continue                      # gehört noch zum Textblock
            block_ein = None
        roh = zeile.rstrip()
        if not roh.strip() or roh.lstrip().startswith('#'):
            continue
        treffer = _YAML_SCHLUESSEL.match(roh)
        if not treffer:
            continue
        vor, strich, name = treffer.group(1), treffer.group(2), treffer.group(3)
        tiefe = len(vor) + (len(strich) if strich else 0)
        while stapel and stapel[-1][0] > tiefe:
            stapel.pop()
        if strich:
            # Neuer Listeneintrag = neue Map; was davor stand, zählt nicht mehr.
            while stapel and stapel[-1][0] >= tiefe:
                stapel.pop()
            stapel.append((tiefe, {}))
        elif not stapel or stapel[-1][0] < tiefe:
            stapel.append((tiefe, {}))
        map_ = stapel[-1][1]
        if name in map_:
            funde.append((nr, name, map_[name]))
        else:
            map_[name] = nr
        if _YAML_BLOCK.match(roh):
            block_ein = tiefe
    return funde


def baue(basis):
    live = os.path.join(basis, 'LIVE')
    os.makedirs(os.path.join(live, 'logbackups'))
    with open(os.path.join(live, 'logbackups', 'Game.log.1'), 'w',
              encoding='utf-8') as f:
        f.writelines(SITZUNG_1)
    with open(os.path.join(live, 'logbackups', 'Game.log.2'), 'w',
              encoding='utf-8') as f:
        f.writelines(SITZUNG_2)
    with open(os.path.join(live, 'Game.log'), 'w', encoding='utf-8') as f:
        f.writelines(LAUFEND)
    return live


def main():
    global ANZEIGE
    ANZEIGE = hat_anzeige()
    if not ANZEIGE:
        print('Hinweis: kein Bildschirm — Fenster-Prüfungen werden übersprungen.')
    basis = tempfile.mkdtemp(prefix='sc-bp-selbsttest-')
    live = baue(basis)
    os.environ['SC_INSTALL_DIR'] = live
    os.environ['SC_BP_HOME'] = os.path.join(basis, 'eigene')
    os.environ['SC_BP_NO_NET'] = '1'
    # Leer heisst ausdruecklich 'kein Launcher' - nur zu loeschen reicht
    # nicht: dann sucht pfade.py weiter und findet womoeglich einen
    # echten Launcher-Stand auf einer eingehaengten Windows-Platte.
    os.environ['SC_BP_LAUNCHER'] = ''
    os.environ.pop('SC_BP_OVERRIDES', None)

    try:
        import queue
        import sc_bp_watcher as w
        from scbp import bestand as bd

        print('\n1. Pfade finden')
        pruefe(w.pfade.spiel_ordner() == live, 'Spielordner gefunden')
        pruefe(len(w.pfade.log_sicherungen()) == 2, 'beide Sicherungen gefunden')
        pruefe(not w.HAT_LAUNCHER, 'läuft ohne SC Deutsch Launcher')

        print('\n2. Nachlese und laufende Sitzung')
        q = queue.Queue()
        wa = w.Watcher(q)
        wa.start()
        time.sleep(1.5)
        b = bd.laden()
        gefunden = set(b['bauplaene'])
        pruefe(gefunden == ERWARTET,
               'genau die %d erwarteten Baupläne (gefunden: %d)'
               % (len(ERWARTET), len(gefunden)))
        if gefunden != ERWARTET:
            for x in sorted(gefunden ^ ERWARTET):
                print('         Abweichung:', x)

        print('\n3. Neuer Fund im laufenden Spiel')
        # ⚠ Erst die Schlange leeren. Seit v3.1.0 meldet die Nachlese, was sie
        #   gefunden hat (bis zu `NACHLESE_MELDEN_BIS` Stück) — das sind hier
        #   sieben, und die stünden sonst in der Auswertung unten und ließen den
        #   frischen Fund wie einen von acht aussehen. Der Test prüft, dass
        #   **dieser eine** gemeldet wird, nicht wie viele vorher kamen.
        _vorher = []
        while not q.empty():
            _vorher.append(q.get())
        _nachgelesen = [m for m in _vorher if m[0] == 'new']
        pruefe(bool(_nachgelesen),
               'die Nachlese meldet ihre Funde (%d Zeilen)' % len(_nachgelesen))
        # ⚠ Und sie sind als nachgelesen gekennzeichnet, sonst sehen sie aus wie
        #   ein Fund von eben — wer gerade nichts freigeschaltet hat, wundert sich.
        pruefe(all(m[-1] is True for m in _nachgelesen),
               'und zwar als nachgelesen gekennzeichnet')

        with open(os.path.join(live, 'Game.log'), 'a', encoding='utf-8') as f:
            f.write(zeile('Bauplan erhalten: Behring FS-9 LMG', 2))
        time.sleep(w.POLL_SEC + 2)
        meldungen = []
        while not q.empty():
            meldungen.append(q.get())
        neu = [m[1] for m in meldungen if m[0] == 'new']
        pruefe(neu == ['Behring FS-9 LMG'],
               'genau eine Meldung, und zwar die richtige (war: %s)' % neu)
        pruefe(any(m[0] == 'hinweis' for m in meldungen)
               or bd.anzahl(bd.laden()) > 0, 'Lückenhinweis wurde ausgegeben')
        wa.stop()

        print('\n4. Neustart — nichts doppelt, nichts verloren')
        vorher = bd.anzahl(bd.laden())
        q2 = queue.Queue()
        wa2 = w.Watcher(q2)
        wa2.start()
        time.sleep(1.5)
        wa2.stop()
        doppelt = [m for m in list(q2.queue) if m[0] == 'new']
        pruefe(not doppelt, 'keine Meldung wiederholt (waren: %d)' % len(doppelt))
        pruefe(bd.anzahl(bd.laden()) == vorher, 'Bestand unverändert (%d)' % vorher)

        print('\n5. Eigener Pfad statt Suche')
        import json
        from scbp import pfade as pf
        os.environ.pop('SC_INSTALL_DIR')        # Suche muss jetzt scheitern
        anders = os.path.join(basis, 'woanders', 'LIVE')
        os.makedirs(anders)
        open(os.path.join(anders, 'Game.log'), 'w').close()

        # ⚠ Die Suche darf hier nichts finden — sonst prüft der Test nur, ob auf
        # DIESEM Rechner zufällig kein Star Citizen liegt. Auf einem Spielrechner
        # war er deshalb rot, obwohl das Programm richtig arbeitete. Also werden
        # die Suchwurzeln für diesen Abschnitt geleert; gesucht wird gleich
        # nochmal ausdrücklich MIT Wurzel.
        echte_wurzeln = pf._spiel_wurzeln
        pf._spiel_wurzeln = lambda: []
        pruefe(pf.spiel_ordner() is None,
               'ohne Eintrag und ohne Fundort wird nichts gefunden')
        datei = pf.vorlage_anlegen()
        pruefe(os.path.exists(datei), 'Einstellungsdatei wird zum Ausfüllen angelegt')
        d = json.load(open(datei, encoding='utf-8'))
        d['spiel_ordner'] = anders
        json.dump(d, open(datei, 'w', encoding='utf-8'), ensure_ascii=False, indent=2)
        pruefe(pf.spiel_ordner() == anders, 'selbst eingetragener Pfad wird genommen')
        orte = pf.gesuchte_spielorte()
        pruefe(bool(orte), 'Suchorte werden genannt, auch wenn nichts gefunden wurde')
        d2 = json.load(open(datei, encoding='utf-8'))
        pruefe('_spiel_ordner_gesucht_wird_hier' in d2,
               'die Vorlage nennt die Suchorte beim Feld')

        # Und die Gegenprobe: Liegt an einem Suchort wirklich ein Spiel, muss es
        # gefunden werden. Ohne diese Hälfte wäre „nichts gefunden" wertlos —
        # eine kaputte Suche fände auch nichts.
        with open(datei, encoding='utf-8') as f:
            ohne = json.load(f)
        ohne['spiel_ordner'] = ''
        with open(datei, 'w', encoding='utf-8') as f:
            json.dump(ohne, f, ensure_ascii=False, indent=2)
        wurzel_mit_spiel = os.path.join(basis, 'installiert')
        echt = os.path.join(wurzel_mit_spiel, pf.SC_UNTERPFAD, 'LIVE')
        os.makedirs(echt)
        open(os.path.join(echt, 'Game.log'), 'w').close()
        pf._spiel_wurzeln = lambda: [wurzel_mit_spiel]
        pruefe(pf.spiel_ordner() == echt, 'ein Spiel an einem Suchort wird gefunden')
        pf._spiel_wurzeln = echte_wurzeln

        print('\n6. Erster Start nimmt dem Spieler die Arbeit ab')
        from scbp import assistent as assi, pfade as pf2
        # Frischer Ordner, damit "erster Start" wirklich zutrifft
        frisch = os.path.join(basis, 'frisch')
        os.makedirs(frisch)
        os.environ['SC_BP_HOME'] = frisch
        os.environ.pop('SC_INSTALL_DIR', None)
        echte_wurzeln6 = pf2._spiel_wurzeln
        pf2._spiel_wurzeln = lambda: []        # siehe Abschnitt 5
        pruefe(assi.noetig(), 'Assistent meldet sich beim ersten Start')
        pruefe(pf2.spiel_ordner() is None, 'ohne Angabe und ohne Fundort: nichts')
        pf2._spiel_wurzeln = echte_wurzeln6
        # Der Spieler wählt irgendeine Ebene — auch die falsche muss reichen
        gedeutet = pf2.spielordner_deuten(os.path.dirname(live))
        pruefe(gedeutet == live,
               'Elternordner wird zum richtigen Ordner gedeutet')
        pf2.einstellung_setzen('spiel_ordner', gedeutet)
        pruefe(pf2.spiel_ordner() == live, 'Angabe wirkt sofort, ohne Neustart')
        # Und jetzt der Punkt: Der Bestand füllt sich von allein
        from scbp import logquelle as lq
        funde, _ = lq.nachlesen(lq.Lesestand())
        frischer_bestand = bd.leer()
        for n, _z in funde:
            bd.hinzufuegen(frischer_bestand, n, 'nachlese')
        # +1, weil Schritt 3 dem laufenden Log noch einen Bauplan angehängt hat
        pruefe(bd.anzahl(frischer_bestand) == len(ERWARTET) + 1,
               'Bestand kommt aus den Logs, ohne dass jemand etwas eintippt (%d)'
               % bd.anzahl(frischer_bestand))
        pruefe(not assi.noetig(),
               'beim nächsten Mal läuft der Assistent nicht mehr von allein')

        # ⚠⚠ Ein Lesefehler darf den ganzen Lauf nicht kippen. Bis 01.09.2026
        # flog eine unerwartete Ausnahme aus `_lies_datei` bis hinauf in
        # `_nachlese()`, das sie **still** verschluckt — `stand.speichern()`
        # wurde nie erreicht, und ALLE in diesem Lauf gelesenen Dateien galten
        # wieder als ungelesen. Am alten Stand gemessen: 0 von 23 gemerkt,
        # beim naechsten Start dasselbe von vorn, ohne jede Meldung.
        _sicherungen6 = pf2.log_sicherungen()
        pruefe(len(_sicherungen6) > 0,
               'es liegen Sicherungen zum Pruefen bereit (%d)' % len(_sicherungen6))
        _echt6, _erste6 = lq._lies_datei, _sicherungen6[0]
        _kaputt6 = os.path.basename(str(_erste6))

        def _stolpert6(datei, muster, _e=_echt6, _k=_kaputt6):
            if os.path.basename(str(datei)) == _k:
                raise MemoryError('erzwungen fuer den Selbsttest')
            return _e(datei, muster)

        # ⚠ Den Eintrag dieser Datei vorher wegnehmen. Der Abschnitt oben hat
        # sie schon gemerkt; ohne das misst die Pruefung unten den ALTEN
        # Eintrag und wird gruen, egal wie sich der Code verhaelt.
        _vor6 = lq.Lesestand()
        _vor6.daten['sicherungen'].pop(_kaputt6, None)
        _vor6.speichern()

        lq._lies_datei = _stolpert6
        try:
            _f6, _b6 = lq.nachlesen(lq.Lesestand(), nur_neue=False)
            pruefe(_b6['unlesbar'] == 1,
                   'eine unlesbare Datei wird gezaehlt statt den Lauf zu sprengen')
            pruefe(not lq.Lesestand().kennt(_erste6),
                   'und bleibt ungemerkt, damit sie beim naechsten Lauf drankommt')
        except Exception as _e6:
            pruefe(False, 'nachlesen() ueberlebt einen Lesefehler (%s)'
                   % type(_e6).__name__)
        finally:
            lq._lies_datei = _echt6

        # Dasselbe fuer die laufende Game.log: Sie wird NACH der Schleife
        # gelesen — eine Ausnahme dort haette die eben gelesenen Sicherungen um
        # ihren Eintrag gebracht, obwohl mit ihnen alles in Ordnung war.
        _echt_log6 = pf2.game_log
        pf2.game_log = lambda: (_ for _ in ()).throw(OSError('erzwungen'))
        try:
            lq.nachlesen(lq.Lesestand(), nur_neue=False)
            pruefe(lq.Lesestand().kennt(_erste6),
                   'faellt die laufende Game.log aus, bleiben die Sicherungen '
                   'trotzdem festgehalten')
        except Exception as _e6b:
            pruefe(False, 'nachlesen() ueberlebt eine ausgefallene Game.log (%s)'
                   % type(_e6b).__name__)
        finally:
            pf2.game_log = _echt_log6

        # Der Assistent muss sich wiederholen lassen — für Leute, die sich nicht
        # durch Menüs klicken wollen. Vier Schritte, ohne Absturz durchgereicht.
        if ANZEIGE:
            a = assi.Assistent()
            a.root.withdraw()
            titel = []
            for _ in range(assi.SCHRITTE):
                titel.append(a.titel.cget('text'))
                if a.schritt == 2:
                    a.pfad.set(live)
                a._weiter()
            pruefe(len(set(titel)) == assi.SCHRITTE,
                   'Assistent hat %d unterschiedliche Schritte' % len(set(titel)))
            pruefe(assi.noetig() is False, 'nach dem Durchlauf ist alles gesetzt')
        else:
            uebersprungen('Assistent-Durchlauf')

        print('\n7. Sprache')
        from scbp import sprache
        luecken = [k for k, v in sprache.TEXTE.items()
                   if len(v) != 2 or not all(v)]
        pruefe(not luecken, 'jeder Text hat beide Sprachen (%d Einträge)'
               % len(sprache.TEXTE))
        for k in luecken[:5]:
            print('         unvollständig:', k)
        sprache.setzen('de'); deutsch = sprache.t('filter_fehlt')
        sprache.setzen('en'); englisch = sprache.t('filter_fehlt')
        pruefe(deutsch != englisch,
               'Umschalten wirkt (%s / %s)' % (deutsch, englisch))
        pruefe(sprache.t('gibtesnicht') == 'gibtesnicht',
               'fehlender Schlüssel stürzt nicht ab, sondern fällt auf')
        # Arten aus dem Katalog müssen alle eine Übersetzung haben — nach einem
        # SC-Patch können neue dazukommen, und dann steht sonst „Char_Armor_…"
        # mitten in der Liste.
        from scbp import katalog
        kat = katalog.laden()
        if kat['bauplaene']:
            roh = {e.get('a') for e in kat['bauplaene'].values()}
            offen = [r for r in roh if ('art_%s' % r) not in sprache.TEXTE]
            pruefe(not offen, 'alle %d Bauplan-Arten übersetzt %s'
                   % (len(roh), offen or ''))
        else:
            print('  [--]   Katalog nicht vorhanden, Arten nicht prüfbar')
        sprache.setzen('de')

        print('\n8. Spielsprache selbst erkennen')
        from scbp import phrasen as ph
        # Eine Sprache, die nirgends im Code steht: Der Katalog mit den
        # Bauplan-Namen verrät, welcher Text davor die Bauplan-Meldung ist.
        fremd = os.path.join(basis, 'fremd')
        os.makedirs(os.path.join(fremd, 'logbackups'))
        open(os.path.join(fremd, 'Game.log'), 'w').close()
        with open(os.path.join(fremd, 'logbackups', 'alt.log'), 'w',
                  encoding='utf-8') as f:
            f.write(zeile('Plan de construction reçu: Attrition-5 Repeater', 1))
            f.write(zeile('Mission terminée: Irgendwas', 2))
            f.write(zeile('Plan de construction reçu: Singe Cannon (S2)', 3))
        katalognamen = ['Attrition-5 Repeater', 'Singe Cannon (S2)',
                        '10-Series Greatsword Cannon']
        sicherungen = [os.path.join(fremd, 'logbackups', 'alt.log')]
        gefunden = ph.selbst_finden(katalognamen, sicherungen)
        pruefe(gefunden == 'Plan de construction reçu',
               'unbekannte Sprache wird erkannt (%r)' % gefunden)
        # Ein einzelner Treffer reicht nicht — das könnte Zufall sein
        with open(os.path.join(fremd, 'logbackups', 'einzeln.log'), 'w',
                  encoding='utf-8') as f:
            f.write(zeile('Irgendein Text: Attrition-5 Repeater', 1))
        einzeln = ph.selbst_finden(
            katalognamen, [os.path.join(fremd, 'logbackups', 'einzeln.log')])
        pruefe(einzeln is None, 'ein einzelner Treffer gilt nicht als Beleg')

        print('\n9. Merkliste')
        from scbp import merkliste as mk
        os.environ['SC_BP_HOME'] = os.path.join(basis, 'merk')
        os.makedirs(os.environ['SC_BP_HOME'], exist_ok=True)
        pruefe(mk.anzahl() == 0, 'startet leer')
        pruefe(mk.umschalten('Wunschteil') is True, 'ein Klick trägt ein')
        pruefe(mk.enthaelt('wunschteil'), 'Groß- und Kleinschreibung egal')
        pruefe(mk.umschalten('Wunschteil') is False, 'zweiter Klick trägt aus')
        mk.umschalten('Wunschteil')
        # Muster-Einträge von außen (ein eigenes Werkzeug des Autors schreibt so)
        d = mk.laden()
        d['eintraege'].append({'titel': 'Beispielsatz',
                               'muster': ['adp-mk4', 'woodland']})
        mk.speichern(d)
        pruefe(mk.treffer('ADP-mk4 Woodland Helmet') == 'Beispielsatz',
               'Muster von außen greifen weiter')
        pruefe(mk.erledigen('Wunschteil') == 'Wunschteil',
               'erfüllter Wunsch wird ausgetragen')
        pruefe(not mk.enthaelt('Wunschteil'), 'und ist danach wirklich weg')
        pruefe(mk.erledigen('Irgendwas anderes') is None,
               'was nie beobachtet wurde, ändert nichts')

        print('\n10. Deutsch und Englisch decken sich')
        import sprachen_pruefen
        beanstandungen = sprachen_pruefen.pruefe(melden=lambda *_: None)
        pruefe(not beanstandungen,
               'Projektseite, Changelog und Roadmap sind in beiden Sprachen gleich')
        for b in beanstandungen[:5]:
            print('        ·', b)

        # Der Bericht zählte einmal die Felder der Datei statt der Baupläne
        # darin: „3 Baupläne" bei 394 im Bestand, weil die Datei drei Felder
        # oben hat (version, stand, bauplaene). Eine falsche Zahl, die
        # plausibel aussieht — genau die Sorte, die niemand nachprüft.
        # Geprüft wird die Zählfunktion selbst, nicht der Bericht: Sie hängt
        # nicht davon ab, wie viel gerade im Bestand steht.
        import json as json_pruef
        import scbp.bericht as bericht_pruef
        probe = os.path.join(basis, 'zaehlprobe.json')
        with open(probe, 'w', encoding='utf-8') as f:
            json_pruef.dump({'version': 1, 'stand': 'x',
                             'bauplaene': {'a': 1, 'b': 2, 'c': 3, 'd': 4}}, f)
        pruefe(bericht_pruef._json_groesse(probe, 'bauplaene') == 4,
               'die Zählung im Bericht nimmt die Einträge, nicht die Felder')
        pruefe(bericht_pruef._json_groesse(probe, 'gibtsnicht') == '—',
               'ein fehlender Schlüssel gibt „—" statt einer erfundenen Zahl')

        # Testdaten mit ausgedachten Art-Kennungen sehen aus wie ein Fehler
        # der Oberfläche: Alles landet in „Sonstiges", und der Filter „nur
        # FPS-Waffen" zeigt nichts. Genau so ist es einmal gelaufen.
        import probe_daten
        unbekannt = probe_daten.arten_pruefen()
        pruefe(not unbekannt,
               'die Beispieldaten benutzen echte Art-Kennungen')
        for art in unbekannt[:5]:
            print('        · %s kennt katalog.ART_GRUPPE nicht' % art)

        # ⚠ Die Namensform stand dreimal im Programm und lief auseinander.
        # Folge: Der SC Deutsch Launcher schreibt 7MA "Lorica" mit geraden
        # Anführungszeichen, scmdb mit einfachen — der Bauplan galt als
        # „fehlt", obwohl er im Bestand stand. Hier wird geprüft, dass alle
        # drei Module dieselbe Form liefern.
        from scbp import bestand as b_norm, katalog as k_norm
        from scbp import merkliste as m_norm, pfade as p_norm
        proben = ('7MA "Lorica"', "7MA 'Lorica'", 'CF-117 „Hazard" Repeater',
                  'Test\xa0Name')
        gleich = all(b_norm.norm(x) == k_norm._norm(x) == m_norm._norm(x)
                     == p_norm.namensform(x) for x in proben)
        pruefe(gleich, 'alle Module vergleichen Namen gleich')
        pruefe(p_norm.namensform('7MA "Lorica"')
               == p_norm.namensform("7MA 'Lorica'"),
               'gerade und einfache Anführungszeichen gelten als derselbe Name')

        formatfehler = probe_daten.formate_pruefen()
        pruefe(not formatfehler,
               'die Beispieldaten haben die Formate des echten Katalogs')
        for satz in formatfehler[:5]:
            print('        · ' + satz)

        # Die Dokumente allein reichen nicht: Die Oberfläche zeigte an über
        # hundert Stellen deutschen Text, während oben alles grün meldete.
        import texte_pruefen
        feste = []
        for name in sorted(os.listdir(os.path.join(WURZEL, 'scbp'))):
            if name.endswith('.py'):
                feste += texte_pruefen.pruefe(
                    os.path.join(WURZEL, 'scbp', name))
        pruefe(not feste,
               'jeder sichtbare Text der Oberfläche läuft durch t()')

        # ⚠⚠ **Kein Schlüssel darf zweimal vergeben sein.** Ein Wörterbuch
        # nimmt das klaglos hin: Der zweite Eintrag verdrängt den ersten, und
        # niemand merkt es — bis eine Stelle mit `%s` rechnet, während der
        # gewinnende Eintrag `{preis}` benutzt. Genau so ist am 04.09.2026 die
        # Bestenliste im Verkaufs-Reiter stumm geblieben: Überschrift da,
        # Liste leer, kein Hinweis worauf.
        #
        # Gefunden wurden dabei **drei** Doppelungen, eine davon Monate alt.
        import re as _re_dop
        _sprachdatei = os.path.join(WURZEL, 'scbp', 'sprache.py')
        with open(_sprachdatei, encoding='utf-8') as _f_dop:
            _roh_dop = _f_dop.read()
        _schluessel = _re_dop.findall(r"^    '([a-z0-9_]+)':", _roh_dop,
                                      _re_dop.M)
        _doppelt = sorted({s for s in _schluessel
                           if _schluessel.count(s) > 1})
        pruefe(not _doppelt,
               'kein Sprachschlüssel ist doppelt vergeben (%s)'
               % (', '.join(_doppelt) if _doppelt else 'keiner'))
        # ⚠ Nicht `zeile` als Schleifenvariable — so heißt weiter oben eine
        # Hilfsfunktion, und Python macht daraus für die ganze Funktion eine
        # lokale Variable. Der Selbsttest stirbt dann Hunderte Zeilen früher.
        for nr, stelle, roh in feste[:5]:
            print('        · Zeile %d (%s): %s' % (nr, stelle, roh[:50]))

        print('\n11. Fensterlage von einem fremden Rechner')
        if ANZEIGE:
            kaputt = w.geometrie_pruefen('440x1098+999999+-999999', _wurzel())
            pruefe('+999999' not in kaputt,
                   'unsinnige Position verworfen (%s)' % kaputt)
        else:
            uebersprungen('Fensterlage von einem fremden Rechner')

        # ------------------------------------------------------------------ 12
        # Fehler mitschreiben. Der Sinn der Sache ist, dass ein Nutzer den
        # Bericht in ein **öffentliches** Issue kopieren kann — deshalb wird
        # hier vor allem geprüft, dass kein Benutzername durchrutscht.
        print()
        print('12. Fehler werden mitgeschrieben')
        os.environ['SC_BP_HOME'] = os.path.join(basis, 'fehlerbuch')
        os.makedirs(os.environ['SC_BP_HOME'], exist_ok=True)
        from scbp import fehler as fehlerbuch, bericht
        importlib.reload(fehlerbuch)

        fehlerbuch.leeren()
        with fehlerbuch.gefangen('probe.stelle'):
            raise ValueError('etwas ging schief in %s'
                             % os.path.expanduser('~/geheim/pfad'))
        eintraege = fehlerbuch.letzte(1)
        pruefe(len(eintraege) == 1, 'ein gefangener Fehler wird festgehalten')
        pruefe(eintraege and eintraege[0].get('stelle') == 'probe.stelle',
               'die Stelle steht dabei')
        pruefe(eintraege and eintraege[0].get('art') == 'ValueError',
               'die Art des Fehlers steht dabei')

        name = os.path.basename(os.path.expanduser('~').rstrip('/\\'))
        roh = json.dumps(eintraege, ensure_ascii=False)
        pruefe(len(name) < 3 or name.lower() not in roh.lower(),
               'kein Benutzername im Protokoll')
        pruefe('<heim>' in roh, 'der Heimatpfad ist ersetzt')

        # Der Ringpuffer darf die Datei nicht wachsen lassen.
        for i in range(fehlerbuch.HOECHSTENS + 12):
            fehlerbuch.merken('probe.viele', ValueError('Nummer %d' % i))
        pruefe(fehlerbuch.anzahl() == fehlerbuch.HOECHSTENS,
               'es bleiben höchstens %d Einträge liegen' % fehlerbuch.HOECHSTENS)

        text = bericht.bauen(version='0.0.0-test')
        pruefe(bool(text) and 'SC BP Watcher' in text, 'der Bericht wird gebaut')

        # ⚠ Ein Schreibfehler darf nicht spurlos verschwinden. Bis zum
        # 26.08.2026 gab `einstellungen_schreiben` nur `False` zurück — und
        # **kein einziger Aufrufer** wertet das aus. Eine Einstellung war nach
        # dem Neustart einfach wieder alt, ohne jeden Hinweis.
        # ⚠ Jede Datei, die der Code über `_mitgeliefert()` lädt, muss der Bau
        # auch einpacken. Sonst fehlt sie NUR in der fertigen Version — beim
        # Start aus dem Quellcode fällt es nie auf. Genau so fehlte das Logo auf
        # der Seite „Update & Über": Der Code lud `assets/xharig.png`, der Bau
        # lieferte nur `assets/icon.png`. Gemeldet am 26.08.2026 ,
        # dem es im Bild eines Testers auffiel.
        import re as re_
        bauplan = open(os.path.join(WURZEL, '.github', 'workflows',
                                    'release.yml'), encoding='utf-8').read()
        gebraucht = set()
        for datei in ('sc_bp_watcher.py',) + tuple(
                os.path.join('scbp', n) for n in os.listdir(
                    os.path.join(WURZEL, 'scbp')) if n.endswith('.py')):
            quelle_ = open(os.path.join(WURZEL, datei), encoding='utf-8').read()
            for treffer in re_.finditer(
                    r"_mitgeliefert\(\s*(?:os\.path\.join\()?([^)]+)\)", quelle_):
                teile = re_.findall(r"'([^']+)'", treffer.group(1))
                if teile:
                    gebraucht.add(teile[-1])
        for name in sorted(gebraucht):
            pruefe(name in bauplan,
                   'der Bau liefert „%s" mit' % name)

        # ⚠ Zwei Fallen stecken in diesem Test, beide am 26.08.2026 erlebt:
        #
        # 1. **Nicht per `chmod` sperren.** Auf den Bau-Rechnern läuft alles als
        #    root, und root schreibt auch in einen Ordner mit entzogenen
        #    Rechten. Der Test war dort grün, ohne etwas zu prüfen.
        # 2. **Nicht den ganzen Ablageordner unbrauchbar machen.** Dann kann
        #    auch das Fehlerprotokoll nicht mehr geschrieben werden — und genau
        #    das soll ja geprüft werden.
        #
        # Deshalb wird **nur die Einstellungsdatei** blockiert: Dort, wo die
        # Nebendatei `…json.tmp` entstehen müsste, liegt ein Ordner. Daran
        # scheitert das Schreiben, unabhängig von Rechten und Benutzer — der
        # Rest der Ablage bleibt heil.
        sperr = os.path.join(basis, 'sperrprobe')
        os.makedirs(sperr, exist_ok=True)
        os.makedirs(os.path.join(sperr, 'einstellungen.json.tmp'),
                    exist_ok=True)
        alt_home = os.environ.get('SC_BP_HOME')
        os.environ['SC_BP_HOME'] = sperr
        try:
            from scbp import pfade as pf_sperr
            fehlerbuch.leeren()
            geschrieben = pf_sperr.einstellung_setzen('probe', 2)
            pruefe(not geschrieben,
                   'ein blockiertes Ziel meldet einen Fehlschlag')
            stellen = [e.get('stelle') for e in fehlerbuch.letzte(3)]
            pruefe('pfade.einstellungen_schreiben' in stellen,
                   'und der Grund steht im Fehlerprotokoll')
        finally:
            if alt_home:
                os.environ['SC_BP_HOME'] = alt_home

        # ⚠ Die Zeile „Spielsprache" stand drei Übergaben lang auf „—", weil
        # `phrasen.sammeln()` ein Tupel liefert und der Bericht es wie eine
        # Liste behandelte. Der TypeError wurde von `_sicher()` verschluckt.
        # Geprüft wird deshalb der Wert selbst, nicht nur dass der Bericht baut.
        pruefe(bericht._spielsprache() and 'Bauplan erhalten'
               in bericht._spielsprache(),
               'die Spielsprache-Zeile nennt die gesuchten Formulierungen')
        for zeile_ in text.split('\n'):
            if zeile_.startswith('Spielsprache') or zeile_.startswith('Game language'):
                pruefe(zeile_.strip().rstrip() not in
                       ('Spielsprache —', 'Game language —')
                       and '—' != zeile_.split()[-1],
                       'im Bericht steht bei der Spielsprache kein Strich')
                break
        pruefe(len(name) < 3 or name.lower() not in text.lower(),
               'kein Benutzername im Bericht')
        pruefe('Letzte Fehler' in text, 'die letzten Fehler stehen im Bericht')

        fehlerbuch.leeren()
        pruefe(fehlerbuch.anzahl() == 0, 'das Protokoll lässt sich leeren')

        # ------------------------------------------------------------------ 13
        # Bestand einlesen. Wichtig ist vor allem, dass NICHTS verloren geht:
        # zusammenführen heißt zusammenführen.
        print()
        print('13. Vorhandenen Bestand einlesen')
        from scbp import importieren, bestand as bestandsmodul

        proben = {
            'eigen': {'werkzeug': 'SC BP Watcher',
                      'bauplaene': [{'name': 'XL-1', 'zeit': '2026-08-01 10:00:00'}]},
            'scmdb': {'exportSchemaVersion': 1,
                      'blueprints': [{'productName': 'XL-1', 'ts': 1756000000}]},
            'basetool': {'blueprints': [{'productName': 'XL-1',
                                         'receivedAt': '2026-08-02T01:49:03.322Z'}]},
            'launcher': {'blueprints': [{'key': 'XL-1'}]},
        }
        erkannt = all(importieren.erkennen(d) == art for art, d in proben.items())
        pruefe(erkannt, 'alle vier Formate werden am Inhalt erkannt')
        pruefe(importieren.erkennen({'irgendwas': [1, 2, 3]}) is None,
               'eine fremde Datei wird nicht erkannt')

        datei = os.path.join(basis, 'einlesen.json')
        with open(datei, 'w', encoding='utf-8') as f:
            json.dump({'blueprints': [
                {'productName': 'Attrition-5 Repeater',
                 'receivedAt': '2026-08-02T01:49:03.322Z'},
                {'productName': 'Attrition-5 Repeater'},          # Dublette
                {'productName': 'Voll Neuer Bauplan'},
            ]}, f)
        art, eintraege = importieren.lesen(datei)
        pruefe(art == 'basetool', 'die Datei wird als Basetool-Ausgabe gelesen')
        pruefe(len(eintraege) == 3, 'alle Zeilen kommen an')

        vorher = bestandsmodul.leer()
        bestandsmodul.hinzufuegen(vorher, 'Attrition-5 Repeater', 'log')
        bestandsmodul.hinzufuegen(vorher, 'Nur Im Bestand', 'log')
        v = importieren.vorschau(eintraege, vorher,
                                 katalog_namen=['Attrition-5 Repeater',
                                                'Scalpel Sniper Rifle Magazine (12 cap)'])
        pruefe(v['gesamt'] == 2, 'Dubletten in der Datei zählen einmal')
        pruefe(v['neu'] == ['Voll Neuer Bauplan'], 'nur wirklich Neues gilt als neu')
        pruefe(v['schon_da'] == ['Attrition-5 Repeater'], 'Vorhandenes wird erkannt')
        pruefe(v['unbekannt'] == ['Voll Neuer Bauplan'],
               'ein dem Katalog unbekannter Name wird gemeldet')

        dazu = importieren.uebernehmen(eintraege, vorher, speichern=False)
        pruefe(dazu == 1, 'genau ein Eintrag kommt dazu')
        pruefe('nur im bestand' in vorher['bauplaene'],
               'der vorhandene Bestand bleibt vollständig erhalten')
        pruefe(vorher['bauplaene']['attrition-5 repeater']['quelle'] == 'log',
               'ein Import überschreibt keine bessere Quelle')

        # ----------------------------------------------------------------- 13b
        # Der Ablage-Ort muss einen Neustart ueberleben. Gemeldet am
        # 04.09.2026: „bei jedem Neustart ist der alte Pfad wieder drin".
        #
        # ⚠ Der Fehler zeigt sich NUR, wenn bereits ein eigener Ort gesetzt
        # ist. Dann schreibt `einstellung_setzen` ueber `app_datei()` in den
        # ALTEN Ordner, waehrend `_ablage_aus_datei()` weiter den unveraenderten
        # Zeiger unter Dokumente liest. Ein Test, der bei Standard-Ablage
        # anfaengt, laeuft gruen durch und beweist nichts — dort sind Zeiger
        # und Ablage dieselbe Datei.
        print()
        print('13b. Der gewaehlte Ablage-Ort ueberlebt den Neustart')
        _pf13b = importlib.import_module('scbp.pfade')
        _dok13b = os.path.join(basis, 'dokumente13b')
        _alt13b = os.path.join(basis, 'ablage_alt13b')
        _neu13b = os.path.join(basis, 'ablage_neu13b')
        for _o in (_dok13b, _alt13b, _neu13b):
            os.makedirs(_o, exist_ok=True)
        _home13b = os.environ.pop('SC_BP_HOME', None)   # sonst sticht sie alles
        _echt13b = _pf13b._dokumente
        _pf13b._dokumente = lambda: _dok13b
        try:
            # Ausgangslage: ein eigener Ort ist bereits gesetzt.
            _zeiger13b = _pf13b.zeiger_datei()
            os.makedirs(os.path.dirname(_zeiger13b), exist_ok=True)
            with open(_zeiger13b, 'w', encoding='utf-8') as _f13b:
                json.dump({'ablage_ordner': _alt13b}, _f13b)
            pruefe(_pf13b.app_ordner() == _alt13b,
                   'Ausgangslage: der Watcher liegt im alten Ordner')

            # Der Nutzer stellt um.
            _pf13b.einstellung_setzen('ablage_ordner', _neu13b)

            with open(_zeiger13b, encoding='utf-8') as _f13b:
                _steht13b = json.load(_f13b).get('ablage_ordner')
            pruefe(_steht13b == _neu13b,
                   'der neue Ort steht in der Zeiger-Datei')
            pruefe(_pf13b.app_ordner() == _neu13b,
                   'nach dem Neustart gilt der neue Ort')

            # ⚠ Und er darf NICHT zusaetzlich im alten Ordner liegen — zwei
            # befuellte Einstellungsdateien laufen garantiert auseinander.
            _altdatei13b = os.path.join(_alt13b, 'Einstellungen',
                                        'einstellungen.json')
            _rest13b = {}
            if os.path.isfile(_altdatei13b):
                with open(_altdatei13b, encoding='utf-8') as _f13b:
                    _rest13b = json.load(_f13b)
            pruefe('ablage_ordner' not in _rest13b,
                   'der alte Ordner behaelt keinen zweiten Ablage-Ort')
        finally:
            _pf13b._dokumente = _echt13b
            if _home13b is not None:
                os.environ['SC_BP_HOME'] = _home13b

        # ------------------------------------------------------------------ 14
        # "Neu"-Marken. Der ganze Nutzen haengt daran, dass sie wieder
        # verschwinden — sonst ist nach drei Versionen alles markiert.
        print()
        print('14. „Neu"-Marken an den Bereichen')
        os.environ['SC_BP_HOME'] = os.path.join(basis, 'neu1')
        os.makedirs(os.environ['SC_BP_HOME'], exist_ok=True)
        from scbp import neuheiten
        importlib.reload(neuheiten)

        neuheiten.erster_start('3.0.0')
        pruefe(neuheiten.offene('3.0.0') == [],
               'frische Installation bekommt keine Marken')

        os.environ['SC_BP_HOME'] = os.path.join(basis, 'neu2')
        os.makedirs(os.environ['SC_BP_HOME'], exist_ok=True)
        neuheiten.erster_start('2.0.0')
        # ⚠ Gegen die **höchste** Version in NEU_SEIT prüfen, nicht gegen eine
        # feste Nummer. Sonst schlägt der Test fehl, sobald ein Bereich für eine
        # spätere Version einträgt (bei „herstellung" = 3.3.0 genau so passiert):
        # Der Bereich ist bei 3.0.0 zu Recht noch nicht offen.
        hoechste = max(neuheiten.NEU_SEIT.values(),
                       key=lambda v: [int(x) for x in v.split('.')])
        offen = sorted(neuheiten.offene(hoechste))
        pruefe(offen == sorted(neuheiten.NEU_SEIT),
               'wer von 2.0.0 kommt, sieht die neuen Bereiche')
        neuheiten.gesehen('bestand', hoechste)
        pruefe('bestand' not in neuheiten.offene(hoechste),
               'die Marke verschwindet, sobald der Bereich offen war')
        pruefe(len(neuheiten.offene(hoechste)) == len(offen) - 1,
               'die übrigen Marken bleiben stehen')
        pruefe(not neuheiten.ist_neu('bestand', '2.0.0'),
               'was es in der eigenen Version noch nicht gibt, wird nicht markiert')

        # ------------------------------------------------------------------ 14a
        # Der Änderungstext wird für „Was ist neu" zerlegt. Zwei Fallen, beide
        # schon zugeschnappt: Unterpunkte als eigene Zeilen (Liste doppelt so
        # lang) und verworfene Fortsetzungszeilen (Sätze enden mittendrin).
        print()
        print('14a. Änderungstext zerlegen')
        from scbp import aktualisierung as akt
        probe = """### Hinzugefügt
- **Ein Fenster mit Reitern.** Oben die Baupläne, darunter die Einstellungen,
  ganz unten eingeklappt, was nur Fortgeschrittene brauchen.
  - Ein Unterpunkt, der nicht als eigene Zeile zählt
### Behoben
- **Das Icon fehlte.**
"""
        punkte = akt.punkte_nach_art(probe)
        pruefe(len(punkte) == 2, 'zwei Punkte, nicht vier')
        pruefe(punkte and punkte[0][0] == 'neu' and punkte[1][0] == 'fix',
               'die Art kommt aus der Zwischenüberschrift')
        pruefe(punkte and punkte[0][1].endswith('brauchen.'),
               'die Fortsetzungszeile gehört zum Satz')
        pruefe(punkte and 'Unterpunkt' not in punkte[0][1],
               'ein Unterpunkt wird nicht angehängt')

        # ------------------------------------------------------------------ 14b
        # Sprachwechsel im Hauptfenster. Es darf dabei KEIN zweites Fenster
        # aufgehen — das alte Einstellungsfenster baute sich bei einem Wechsel
        # komplett neu auf, und als Seite im Hauptfenster wurde daraus ein
        # eigenes Fenster mit halbem Inhalt.
        if ANZEIGE:
            print()
            print('14b. Sprachwechsel im Hauptfenster')
            from scbp import hauptfenster, seiten as seitenmodul, sprache as spr
            import tkinter as _tk
            spr.setzen('de')
            hf = hauptfenster.Hauptfenster(version='3.0.0')
            hf.root.withdraw()
            try:
                hf.oeffnen('allgemein')
                hf.root.update()
                vorher = hf.knoepfe['allgemein'][3].cget('text')

                def fenster_zaehlen(w):
                    n = 0
                    for k in w.winfo_children():
                        if isinstance(k, (_tk.Toplevel, _tk.Tk)):
                            n += 1
                        n += fenster_zaehlen(k)
                    return n

                # ⚠⚠ **Vorher zählen, nachher vergleichen.** Hier stand eine
                # feste Zahl (zuletzt 22) — und die ist umgebungsabhängig:
                # lokal 22, im nächsten Lauf 23, auf dem Bau-Rechner wieder
                # anders. Die Prüfung schlug damit an, ohne dass etwas kaputt
                # war, und musste bei jedem neuen Reiter von Hand nachgezogen
                # werden.
                #
                # Geprüft werden soll ohnehin etwas anderes: dass der
                # Sprachwechsel **keinen Reiter verschluckt**. Dafür ist die
                # Zahl davor das richtige Maß, nicht eine notierte Konstante.
                _vorher_reiter = len(hf.knoepfe)
                seitenmodul._einstellungen(hf)._sprache_waehlen('en')
                hf.root.update()
                pruefe(fenster_zaehlen(hf.root) == 0,
                       'kein zweites Fenster beim Sprachwechsel')
                pruefe(vorher == 'Allgemein'
                       and hf.knoepfe['allgemein'][3].cget('text') == 'General',
                       'die Reiter sind übersetzt')
                pruefe(hf.aktuell == 'allgemein',
                       'die geöffnete Seite bleibt geöffnet')
                # Feste Zahl mit Absicht: Der Test soll auffallen, wenn beim
                # Sprachwechsel ein Reiter verschwindet. Kommt einer dazu,
                # wird sie hier mitgezogen. 11 = die Hauptleiste ohne die zwei
                # unter „Für Fortgeschrittene".
                #
                # ⚠ Am 28.08.2026 von 10 auf 11: **Diagnose ist nach oben
                # gewandert.** Wer die Seite braucht, hat ein Problem — und
                # sucht sie nicht in einem zugeklappten Menü namens
                # „Fortgeschritten". Seit dem Knopf „Fehlerbericht absenden"
                # ist sie zudem der Weg, auf dem Meldungen ankommen.
                #
                # ⚠ Am 30.08.2026 von 14 auf 16: die Gruppe **Handel** mit
                # „Handelslager" und „Verkauf". Kurz darauf zurueck auf 15:
                # **Bauplan-Bestand** ist hinter „Fuer Fortgeschrittene"
                # gewandert, weil die Seite am eigenen Bestand schreibt und im
                # Vorbeigehen angeklickt wurde.
                #
                # ⚠ Am 04.09.2026 von 15 auf 16: das **Auftrags-Protokoll**
                # unter „Baupläne". Auftraege sind die Quelle der Bauplaene —
                # deshalb dort und nicht in einer eigenen Gruppe.
                #
                # ⚠ Am 04.09.2026 von 16 auf 17: **Joysticks** unter
                # „Einstellungen" — welcher Stick welche Nummer hat und was
                # darauf liegt.
                #
                # ⚠ Am 04.09.2026 von 17 auf 18: **Läden** unter „Werkstatt".
                # Dort und nicht bei „Handel": Die Kette der Werkstatt endet
                # bei „wo hole ich das", und ein fertig gekauftes Teil ist die
                # Antwort auf dieselbe Frage — nur der andere Weg. Bei „Handel"
                # geht es um Ware, die man loswerden will.
                #
                # ⚠ Am 04.09.2026 von 18 auf 19: **Routen** unter „Handel",
                # hinter „Verkauf". Dort geht es um Ware, die man schon hat —
                # hier um die Fahrt, die man erst plant.
                #
                # ⚠ Am 06.09.2026 von 20 auf 21: **Bergung** in einer
                # eigenen Gruppe. Bergbau ist Erz aus Felsen, Bergung ist ein
                # Wrack ausschlachten — im Werkzeug zwei verschiedene Fragen.
                #
                # ⚠ Am 06.09.2026 von 19 auf 20: **Mein Hangar** unter
                # „Werkstatt", noch vor dem Lager. Die Kette dort beginnt bei
                # „was habe ich", und das sind zwei Dinge — die Schiffe und
                # das Material. Die Schiffe zuerst, weil sie die Frage
                # beantworten, die auf einen neuen Bauplan sofort folgt:
                # passt das überhaupt irgendwo hinein?
                #
                # ⚠ Am 06.09.2026 von 21 auf 22: **Achsen & Kurven** unter
                # „Einstellungen", direkt unter „Joysticks". Der eine Reiter
                # sagt, welcher Stick welche Nummer hat und was darauf liegt —
                # der andere, wie die Achse reagiert. Im Spiel stehen die
                # beiden Fragen ebenfalls an zwei getrennten Stellen.
                # ⚠ Beide Zahlen stehen im Text. Ohne sie meldet ein
                # Bau-Lauf nur „[FEHL] alle Reiter sind wieder da", und
                # niemand weiß, ob einer fehlt oder zwanzig.
                pruefe(len(hf.knoepfe) == _vorher_reiter,
                       'alle Reiter sind wieder da (vorher %d, jetzt %d)'
                       % (_vorher_reiter, len(hf.knoepfe)))

                # Die Wahl muss festgehalten werden — ohne Speichern-Knopf gibt
                # es keinen zweiten Versuch. Vorher stand die Markierung
                # danach weiter auf der alten Sprache.
                from scbp import pfade as pf4
                pruefe(pf4.einstellung('sprache') == 'en',
                       'die gewählte Sprache ist gespeichert')
                pruefe(seitenmodul._einstellungen(hf).sprache_wahl.get() == 'en',
                       'und die Markierung steht darauf')
            finally:
                spr.setzen('de')
                hf.root.destroy()
        else:
            uebersprungen('Sprachwechsel im Hauptfenster')

        # ------------------------------------------------------------------ 15
        # Umzug in den sichtbaren Ordner. Hier hängt der Bauplan-Bestand dran —
        # geht das schief, steht ein Nutzer nach dem Update vor einer leeren
        # Liste, obwohl er nichts verloren hat.
        print()
        print('15. Umzug in den sichtbaren Ordner')
        import json as _json
        from scbp import pfade as pf3
        importlib.reload(pf3)

        alt_ordner = os.path.join(basis, 'alt-appdata')
        neu_ordner = os.path.join(basis, 'Dokumente')
        os.makedirs(alt_ordner, exist_ok=True)
        os.makedirs(neu_ordner, exist_ok=True)
        os.environ.pop('SC_BP_HOME', None)
        # ⚠⚠ **Auch den Zweitzeiger stilllegen.** Seit dem 06.09.2026 liest
        # `app_ordner()` einen zweiten Zeiger im Konfigurationsordner (siehe
        # `pfade._zweitzeiger`). Auf einem Rechner, auf dem der gesetzt ist,
        # zog der Umzug sonst in den ECHTEN Ablageordner des Benutzers statt
        # in den Testordner — die Pruefung fiel um und hatte recht damit.
        _kon3 = tempfile.mkdtemp(prefix='umzug-konf-')
        _sicher3 = {k: os.environ.get(k)
                    for k in ('XDG_CONFIG_HOME', 'APPDATA')}
        os.environ['XDG_CONFIG_HOME'] = _kon3
        os.environ['APPDATA'] = _kon3
        echte_alt, echte_dok = pf3.alter_app_ordner, pf3._dokumente
        pf3.alter_app_ordner = lambda: alt_ordner
        pf3._dokumente = lambda: neu_ordner
        try:
            with open(os.path.join(alt_ordner, 'bestand.json'), 'w',
                      encoding='utf-8') as f:
                _json.dump({'bauplaene': {'xl-1': {'name': 'XL-1'}}}, f)
            with open(os.path.join(alt_ordner, 'katalog-cache.json'), 'w',
                      encoding='utf-8') as f:
                _json.dump({'x': 1}, f)

            pruefe(pf3.umzug_noetig(), 'ein alter Ordner wird erkannt')
            anzahl = pf3.umziehen()
            pruefe(anzahl == 2, 'beide Dateien wandern mit')
            pruefe(os.path.exists(os.path.join(neu_ordner, 'SC BP Watcher',
                                               'Bauplaene', 'bestand.json')),
                   'der Bestand landet unter „Bauplaene"')
            pruefe(os.path.exists(os.path.join(neu_ordner, 'SC BP Watcher',
                                               'Intern', 'katalog-cache.json')),
                   'technischer Kleinkram landet unter „Intern"')
            pruefe(os.path.exists(os.path.join(alt_ordner, 'bestand.json')),
                   'der alte Ordner bleibt unangetastet liegen')
            pruefe(not pf3.umzug_noetig(), 'ein zweiter Umzug ist nicht nötig')
            pruefe(pf3.umziehen() == 0, 'und überschreibt nichts')

            # ⚠ Die Ablage-Einstellung darf `app_ordner()` nicht in eine Schleife
            # schicken. Ein scharfes Rekursionslimit macht das sofort sichtbar.
            grenze = sys.getrecursionlimit()
            sys.setrecursionlimit(120)
            try:
                pf3.app_datei('bestand.json')
                pruefe(True, 'kein Kreisverkehr zwischen Ordner und Einstellungen')
            except RecursionError:
                pruefe(False, 'kein Kreisverkehr zwischen Ordner und Einstellungen')
            finally:
                sys.setrecursionlimit(grenze)
        finally:
            pf3.alter_app_ordner, pf3._dokumente = echte_alt, echte_dok
            for _k3, _v3 in _sicher3.items():
                if _v3 is None:
                    os.environ.pop(_k3, None)
                else:
                    os.environ[_k3] = _v3
            shutil.rmtree(_kon3, ignore_errors=True)
            os.environ['SC_BP_HOME'] = os.path.join(basis, 'eigene')

        # Der Klammer-Abgleich: (12 Schuss) gegen (12 cap) — derselbe Bauplan.
        v2 = importieren.vorschau(
            [{'name': 'Scalpel Sniper Rifle Magazine (12 Schuss)', 'zeit': None}],
            bestandsmodul.leer(),
            katalog_namen=['Scalpel Sniper Rifle Magazine (12 cap)'])
        pruefe(v2['unbekannt'] == [],
               'abweichender Klammer-Zusatz gilt nicht als unbekannt')

        # ------------------------------------------------------------------ 16
        # Neustart nach dem Update. Dieser Fehler ist dreimal aufgetreten und
        # war jedes Mal schwer zu sehen, weil er nur in der verpackten Version
        # unter Windows auftritt — hier wird deshalb die Entscheidung geprüft,
        # nicht das Ergebnis.
        print()
        print('16. Neustart nach dem Update')
        from scbp import aktualisierung as akt

        gestartet = []

        umgebungen = []

        class _FalschesPopen(object):
            def __init__(self, *a, **k):
                gestartet.append(a[0] if a else None)
                umgebungen.append(k.get('env') or {})

            def poll(self):
                return None          # tut so, als lebe die neue Version

        echtes_popen = subprocess.Popen
        echte_verpackung = akt.verpackung
        merker_vorher = akt._TAUSCH_LAEUFT[0]
        try:
            subprocess.Popen = _FalschesPopen
            akt.verpackung = lambda: 'exe'

            # Wartet ein Hilfsskript auf den Dateitausch, darf `neu_starten()`
            # NICHT selbst starten: Auf der Platte liegt dann noch die ALTE
            # `.exe`, und ein eigener Start fährt genau die wieder hoch. Sie
            # hält danach den Temp-Ordner fest, der Tausch scheitert endgültig,
            # und der Nutzer sieht die alte Version weiterlaufen.
            akt._TAUSCH_LAEUFT[0] = True
            akt.neu_starten()
            pruefe(gestartet == [],
                   'wartet ein Dateitausch, wird nichts selbst gestartet')

            # Ohne wartenden Tausch (AppImage: schon getauscht) muss gestartet
            # werden — sonst bliebe das Programm nach dem Update einfach zu.
            akt._TAUSCH_LAEUFT[0] = False
            akt.neu_starten()
            pruefe(len(gestartet) == 1,
                   'ohne wartenden Tausch startet die neue Version')

            # ⚠ **Die Umgebung muss gewaschen sein.** Genau hier ist der
            # Neustart unter Linux monatelang gescheitert: `LD_LIBRARY_PATH`,
            # `PYTHONHOME` und `PYTHONPATH` zeigen im AppImage in den entpackten
            # Mount der ALTEN Version. Zwei Sekunden spaeter beendet sie sich,
            # der Mount verschwindet, und die neue Version findet ihre
            # Bibliotheken nicht mehr. Fuer den Nutzer: „es geht aus, startet
            # aber nicht" (Bomb20, 27.08.2026).
            geerbt = umgebungen[-1] if umgebungen else {}
            uebrig = [n for n in ('LD_LIBRARY_PATH', 'PYTHONHOME', 'PYTHONPATH',
                                  'APPIMAGE', 'APPDIR', 'ARGV0', '_MEIPASS')
                      if n in geerbt]
            pruefe(not uebrig,
                   'die neue Version erbt keine Pfade der alten (%s)'
                   % (', '.join(uebrig) or 'keine'))

            # Und: Stirbt die neue Version sofort, darf die alte NICHT abtreten.
            class _TotesPopen(_FalschesPopen):
                returncode = 1

                def poll(self):
                    return 1         # schon gestorben
            akt._GESTARTET[0] = _TotesPopen('x')
            pruefe(akt.neue_fassung_laeuft(wartezeit=0.3) is False,
                   'eine sofort gestorbene neue Version wird erkannt')
            akt._GESTARTET[0] = _FalschesPopen('x')
            pruefe(akt.neue_fassung_laeuft(wartezeit=0.3) is True,
                   'eine laufende neue Version gilt als geglueckt')
            akt._GESTARTET[0] = None
        finally:
            subprocess.Popen = echtes_popen
            akt.verpackung = echte_verpackung
            akt._TAUSCH_LAEUFT[0] = merker_vorher

        # Den Spiel-Starter neben dem Spielordner finden. Feste Pfadlisten
        # gehen genau dann schief, wenn jemand woanders installiert hat — und
        # das ist der Normalfall, nicht die Ausnahme.
        starter_basis = os.path.join(basis, 'starterprobe')
        rsi = os.path.join(starter_basis, 'Program Files',
                           'Roberts Space Industries')
        spiel_pfad = os.path.join(rsi, 'StarCitizen', 'LIVE')
        os.makedirs(spiel_pfad)
        os.makedirs(os.path.join(rsi, 'RSI Launcher'))
        launcher = os.path.join(rsi, 'RSI Launcher', 'RSI Launcher.exe')
        open(launcher, 'w').close()

        from scbp import pfade as pf_start
        alt_windows = pf_start.WINDOWS
        alt_ordner = pf_start.spiel_ordner
        alt_einst = pf_start.einstellung
        # ⚠ Die Registry-Suche muss ebenfalls stillgelegt werden. Sie geht an
        # den umgebogenen Umgebungsvariablen vorbei und findet auf einem Rechner
        # mit echtem Spiel den richtigen Launcher — der Test praeft sonst wieder
        # den Rechner statt den Code.
        alt_registry = pf_start._launcher_aus_registry
        pf_start._launcher_aus_registry = lambda: None

        # ⚠ Die Umgebungsvariablen MÜSSEN mit umgebogen werden. `spielstarter()`
        # sucht nach dem Spielordner noch feste Orte unter `LOCALAPPDATA`,
        # `PROGRAMFILES` und `PROGRAMW6432` ab — und auf einem Rechner, auf dem
        # Star Citizen wirklich installiert ist, findet es dort den **echten**
        # RSI Launcher. Die zweite Prüfung unten schlug deshalb bei der Autor
        # unter Windows immer fehl, während sie auf Linux und Mac grün war: Der
        # Test löschte seinen Schein-Launcher, und `spielstarter()` lieferte
        # trotzdem einen Pfad — nur eben den vom richtigen Spiel.
        #
        # Ein Test, der vom Rechner abhängt, auf dem er läuft, prüft nichts.
        alt_umgebung = {}
        for schluessel in ('LOCALAPPDATA', 'PROGRAMFILES', 'PROGRAMW6432'):
            alt_umgebung[schluessel] = os.environ.get(schluessel)
            os.environ[schluessel] = starter_basis
        try:
            pf_start.WINDOWS = True
            pf_start.spiel_ordner = lambda: spiel_pfad
            pf_start.einstellung = lambda name: None
            pruefe(pf_start.spielstarter() == launcher,
                   'der Launcher wird neben dem Spielordner gefunden')

            # Ohne Launcher darf KEIN Pfad zurückkommen — sonst erschiene ein
            # Knopf, der nichts tut.
            os.remove(launcher)
            pruefe(pf_start.spielstarter() is None,
                   'ohne Launcher gibt es keinen Knopf')
        finally:
            pf_start.WINDOWS = alt_windows
            pf_start.spiel_ordner = alt_ordner
            pf_start.einstellung = alt_einst
            pf_start._launcher_aus_registry = alt_registry
            for schluessel, wert in alt_umgebung.items():
                if wert is None:
                    os.environ.pop(schluessel, None)
                else:
                    os.environ[schluessel] = wert

        # Und dasselbe unter Linux: Dort ist der Starter **nicht** der
        # `lug-helper` (der verwaltet nur und kann gar nicht starten), sondern
        # das `sc-launch.sh` im Wine-Präfix — eine Ebene über `drive_c`.
        # Der Fehler dahinter kostete am 27.08.2026 zwei Melder und einen
        # halben Vormittag: Der Knopf war da, meldete „wird gestartet …" und
        # nichts geschah.
        linux_basis = os.path.join(basis, 'linuxprobe', 'star-citizen')
        linux_spiel = os.path.join(linux_basis, 'drive_c', 'Program Files',
                                   'Roberts Space Industries', 'StarCitizen',
                                   'LIVE')
        os.makedirs(linux_spiel)
        skript = os.path.join(linux_basis, 'sc-launch.sh')
        open(skript, 'w').close()
        os.chmod(skript, 0o755)
        # ⚠ Auch `HOME` umbiegen — aus demselben Grund wie oben bei den
        # Windows-Variablen: Der Rückfall sieht in `~/Games/star-citizen` nach,
        # und auf einem Rechner mit echtem Spiel liegt dort ein echtes Skript.
        # Die zweite Prüfung unten fände es und wäre wertlos.
        alt_heim = os.environ.get('HOME')
        os.environ['HOME'] = os.path.join(basis, 'linuxprobe', 'leeres-heim')
        try:
            pf_start.WINDOWS = False
            pf_start.spiel_ordner = lambda: linux_spiel
            pf_start.einstellung = lambda name: None
            pruefe(pf_start.spielstarter() == skript,
                   'unter Linux wird sc-launch.sh über drive_c gefunden')

            # Ohne Startskript darf KEIN Pfad kommen — auch dann nicht, wenn auf
            # dem Rechner ein `lug-helper` im Suchpfad liegt. Genau der wurde
            # früher zurückgegeben, und der Knopf tat nichts.
            os.remove(skript)
            pruefe(pf_start.spielstarter() is None,
                   'ohne sc-launch.sh gibt es unter Linux keinen Knopf')
        finally:
            pf_start.WINDOWS = alt_windows
            pf_start.spiel_ordner = alt_ordner
            pf_start.einstellung = alt_einst
            if alt_heim is None:
                os.environ.pop('HOME', None)
            else:
                os.environ['HOME'] = alt_heim

        # Jeder Ausgang beim Ablagesymbol muss im Startverlauf landen. Der
        # Fehler war zweimal nicht zu finden, weil weder ein Fehler noch eine
        # Spur im Bericht stand — geprüft wird hier deshalb, dass überhaupt
        # gemeldet wird, nicht was dabei herauskommt (das geht nur unter
        # Windows).
        quelle_start = open(os.path.join(WURZEL, 'sc_bp_watcher.py'),
                            encoding='utf-8').read()
        block = quelle_start.split('def ablagesymbol_starten')[1].split('\n    def ')[0]
        for erwartet, wofuer in (
                ("fehler.spur('Ablagesymbol: entfällt", 'nicht Windows'),
                ("fehler.spur('Ablagesymbol: abgeschaltet", 'abgeschaltet'),
                ("fehler.spur('Ablagesymbol: %s'", 'angelegt oder nicht'),
                ("fehler.spur('Ablagesymbol: Fehler", 'Ausnahme')):
            pruefe(erwartet in block,
                   'Ablagesymbol meldet den Fall „%s"' % wofuer)

        symbol_quelle = open(os.path.join(WURZEL, 'scbp', 'ablagesymbol.py'),
                             encoding='utf-8').read()
        pruefe('except Exception:\n            bereit.set()' not in symbol_quelle,
               'der Faden verschluckt Fehler nicht mehr stillschweigend')

        # Der Notausgang darf nicht an Tk hängen: Feuert der `after`-Rückruf
        # nicht, würde ein dort gestarteter Faden nie laufen — und der Prozess
        # liefe weiter, während sein Temp-Ordner schon abgeräumt wird.
        #
        # ⚠ Geprüft wird **innerhalb** von `_abtreten()`. Früher lag der
        # Notausgang direkt in `_fassung_holen`, und der Test schnitt die Quelle
        # bei `def _abtreten` ab — damals der Name der dortigen *lokalen*
        # Funktion. Seit `_abtreten()` eine eigene Funktion ist (beide
        # Abtritts-Wege teilen sie sich), traf dieser Schnitt ins Leere.
        quelle = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                      encoding='utf-8').read()
        block = quelle.split('def _abtreten')[1].split('\ndef ')[0]
        vor_rueckruf = block.split('fenster.root.after')[0]
        pruefe('os._exit(0)' in vor_rueckruf,
               'der Notausgang steht vor dem Tk-Rückruf, nicht darin')


        # ---------------------------------------------------------------- 17
        print()
        print('17. Zweisprachigkeit: kein fester Text in der Oberfläche')
        # ⚠ Warum das geprüft wird: Am 26.08.2026 stellte der Autor auf Englisch um
        # und bekam ein englisches Hauptfenster mit einer **deutschen** Melde-
        # Leiste. Die Übersetzungen dafür gab es längst — `ueberwache`,
        # `mit_launcher`, `ohne_launcher`, `nachgelesen`, `vorlaeufig` —, nur
        # benutzt hat sie niemand. Der Code setzte die deutschen Sätze weiter fest
        # zusammen.
        #
        # Deshalb prüft das hier nicht „gibt es unbenutzte Schlüssel", sondern die
        # eigentliche Ursache: **Steht sichtbarer Text fest im Code?**
        import ast as _ast
        import re as _re

        _zeichen = _re.compile(r'^[\W\d_]+$', _re.UNICODE)   # ✕ ▾ ⏻ · ✓ – …

        # Eigennamen bleiben in jeder Sprache gleich — die gehören nicht
        # übersetzt, sondern stehen genau so da.
        _namen = ('Xharig', 'Star Citizen', 'SC BP Watcher', 'GitHub',
                  'Windows', 'Linux', 'Discord')

        def _verdaechtig(wert):
            """Ist das ein sichtbarer Satz statt eines Symbols?"""
            if not isinstance(wert, str) or len(wert) < 4:
                return False
            if wert.strip() in _namen:
                return False
            if _zeichen.match(wert):        # reine Symbole sind keine Sprache
                return False
            return bool(_re.search(r'[A-Za-zÄÖÜäöüß]{3}', wert))

        def _feste_texte(datei):
            """Alle Stellen, die einem Element **wörtlich** Text mitgeben."""
            quelle = open(datei, encoding="utf-8").read()
            gefunden = []
            for knoten in _ast.walk(_ast.parse(quelle)):
                if not isinstance(knoten, _ast.Call):
                    continue
                # a) text='…' an einem Widget oder in .config()
                for wort in knoten.keywords:
                    if wort.arg != 'text':
                        continue
                    if (isinstance(wort.value, _ast.Constant)
                            and _verdaechtig(wort.value.value)):
                        gefunden.append((wort.value.lineno, wort.value.value))
                # b) q.put(('status', '…')) und ('hinweis', '…')
                for arg in knoten.args:
                    if not isinstance(arg, _ast.Tuple) or len(arg.elts) != 2:
                        continue
                    erst, zweit = arg.elts
                    if (isinstance(erst, _ast.Constant)
                            and erst.value in ('status', 'hinweis')
                            and isinstance(zweit, _ast.Constant)
                            and _verdaechtig(zweit.value)):
                        gefunden.append((zweit.lineno, zweit.value))
            return gefunden

        # ⚠ Zweiter Anlauf: Die erste Version dieser Prüfung sah nur
        # `text='…'` direkt am Widget — und übersah dadurch
        #     unten = f'{titel} — jetzt craftbar!' if titel else 'neu …'
        # weil der Satz erst in eine Variable geht und später zusammengesetzt
        # wird. Genau so lag der Fehler im Overlay. Deshalb prüfen die
        # Oberflächen-Dateien zusätzlich **jedes** String-Literal auf deutsche
        # Wörter, Docstrings ausgenommen.
        _deutsch = _re.compile(
            r'[äöüßÄÖÜ]|\b(?:jetzt|neu|nicht|kein[e]?|wird|wurde|von|aus|mit'
            r'|noch|schon|hier|dein|alle)\b')

        def _docstrings(baum):
            raus = set()
            for k in _ast.walk(baum):
                if isinstance(k, (_ast.Module, _ast.FunctionDef,
                                  _ast.AsyncFunctionDef, _ast.ClassDef)):
                    kopf = k.body[0] if k.body else None
                    if (isinstance(kopf, _ast.Expr)
                            and isinstance(kopf.value, _ast.Constant)
                            and isinstance(kopf.value.value, str)):
                        raus.add(id(kopf.value))
            return raus

        def _deutsche_saetze(datei):
            """Deutscher Satz irgendwo im Code — auch über eine Variable."""
            quelle = open(datei, encoding='utf-8').read()
            baum = _ast.parse(quelle)
            weg = _docstrings(baum)
            # Interne Protokolle (`fehler.merken`, `fehler.spur`) sind kein
            # Oberflächentext. Über den Baum ausschließen, nicht über die
            # Zeile: Ein Aufruf darf sich über mehrere Zeilen ziehen.
            for _k in _ast.walk(baum):
                if (isinstance(_k, _ast.Call)
                        and getattr(_k.func, 'attr', '') in ('merken', 'spur')):
                    for _teil in _ast.walk(_k):
                        if isinstance(_teil, _ast.Constant):
                            weg.add(id(_teil))
                # Der `if __name__ == '__main__'`-Block ist der Aufruf von der
                # Kommandozeile — den sieht kein Spieler, nur der Entwickler.
                if (isinstance(_k, _ast.If) and isinstance(_k.test, _ast.Compare)
                        and getattr(_k.test.left, 'id', '') == '__name__'):
                    for _teil in _ast.walk(_k):
                        if isinstance(_teil, _ast.Constant):
                            weg.add(id(_teil))
            gefunden = []
            for k in _ast.walk(baum):
                if not isinstance(k, _ast.Constant) or not isinstance(k.value, str):
                    continue
                if id(k) in weg:
                    continue
                wert = k.value.strip()
                if len(wert) < 8 or not _deutsch.search(wert):
                    continue
                gefunden.append((k.lineno, wert))
            return gefunden

        _wurzelpfad = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
        _zu_pruefen = [os.path.join(_wurzelpfad, 'sc_bp_watcher.py')]
        for _name in sorted(os.listdir(os.path.join(_wurzelpfad, 'scbp'))):
            if _name.endswith('.py') and _name not in ('sprache.py', 'fehler.py'):
                _zu_pruefen.append(os.path.join(_wurzelpfad, 'scbp', _name))

        _treffer = []
        for _datei in _zu_pruefen:
            for _zeilennr, _text in _feste_texte(_datei):
                _treffer.append('%s:%d  %r' % (os.path.basename(_datei), _zeilennr,
                                               _text[:45]))
        if _treffer:
            for _t in _treffer[:8]:
                print('       ' + _t)
        pruefe(not _treffer,
               'kein fest eingebauter Anzeigetext (%d gefunden)' % len(_treffer))

        # ⚠ ALLE Module, nicht nur die mit „Fenster" im Namen.
        #
        # Die erste Version prüfte eine Handauswahl von Oberflächen-Dateien —
        # und ließ `logquelle.py` aus, weil das nach Hintergrund klingt. Genau
        # von dort kam aber „Zwischen … hat Star Citizen Logs weggeräumt", und
        # der Satz stand fest auf Deutsch im Overlay. Auch `pfade.py` gab „kein
        # Starter gefunden" in die Statuszeile.
        #
        # Wer entscheidet, was „sichtbar" ist, irrt sich. Deshalb: alles
        # prüfen, Ausnahmen einzeln benennen und begründen.
        _AUSNAHMEN = {
            # Suchwörter und Datenzuordnung — werden nie angezeigt
            ('scbp/aktualisierung.py', 'geändert'),
            ('scbp/aktualisierung.py', 'hinzugefügt'),
            ('scbp/katalog.py', 'CDS-Rüstung'),
            ('scbp/katalog.py', 'geschütz'),
            # Wortlaut des SPIELS, mit dem im Log GESUCHT wird — angezeigt
            # wird er nie. Rueckfall, falls die `global.ini` fehlt.
            ('scbp/auftraege.py', 'Auftrag zurückgezogen'),
            # ⚠ Die schweizerdeutsche Fassung (live-CH) derselben
            # Rueckfall-Tabelle. Kein Anzeigetext, sondern ein Suchmuster
            # fuer die Log-Zeile.
            ('scbp/auftraege.py', 'Uftrag zurückgezogen'),
            # Datenfeld der Übersetzungsquellen, nirgends angezeigt (geprüft)
            ('scbp/uebersetzung.py', 'Deutsche Übersetzung (rjcncpt)'),
            ('scbp/uebersetzung.py', 'StarStrings (aufgeräumte englische Texte)'),
        }
        # Ganze Dateien, deren deutsche Texte begründet fest sind
        _AUSNAHME_DATEIEN = {
            # Was ins SPIEL geschrieben wird, folgt der Spielsprache — nicht
            # der Sprache des Werkzeugs. Wer das deutsche Sprachpaket fährt,
            # will deutsche Auftragstexte, auch wenn das Fenster englisch ist.
            'scbp/injektion.py',
            # `.desktop`-Dateien: Das Betriebssystem zeigt sie, nicht wir.
            'scbp/autostart.py', 'scbp/verknuepfung.py',
            # Kommentare in der einstellungen.json und eine Entwickler-Hilfe
            # zum fehlenden Entpacker — beides kein Oberflächentext.
            'scbp/pfade.py', 'scbp/spieltexte.py', 'scbp/phrasen.py',
            # Feldnamen der `global.ini` („Gütegrad:", „Verfolgungssignal:") —
            # damit wird in der Spieldatei GESUCHT, angezeigt wird nichts
            # davon. Gleiche Lage wie bei `phrasen.py` eine Zeile höher.
            'scbp/angaben.py',
            # Erklärender Kopf in der patch-historie.json. Steht in der Datei,
            # damit man sie im Repo ohne Quelltext versteht — nie im Fenster.
            'scbp/patchhistorie.py',
        }
        _oberflaeche = ['sc_bp_watcher.py'] + [
            'scbp/' + _n for _n in sorted(os.listdir(os.path.join(_wurzelpfad, 'scbp')))
            if _n.endswith('.py') and _n not in ('sprache.py', 'fehler.py')
            and ('scbp/' + _n) not in _AUSNAHME_DATEIEN]
        _saetze = []
        for _rel in _oberflaeche:
            _voll = os.path.join(_wurzelpfad, _rel)
            if not os.path.exists(_voll):
                continue
            for _nr, _satz in _deutsche_saetze(_voll):
                if (_rel, _satz) in _AUSNAHMEN:
                    continue
                _saetze.append('%s:%d  %r' % (_rel, _nr, _satz[:44]))
        for _s in _saetze[:14]:
            print('       ' + _s)
        pruefe(not _saetze,
               'kein deutscher Satz fest in der Oberfläche (%d gefunden)'
               % len(_saetze))

        sys.path.insert(0, _wurzelpfad)
        from scbp import sprache as _spr

        # Der schärfste Test: jede Seite in **beiden** Sprachen wirklich
        # bauen. `sprache.t()` gibt bei einem fehlenden Schlüssel dessen
        # Namen zurück statt abzustürzen — sichtbar wird das erst, wenn die
        # Seite vor einem steht. Genau so ließe sich ein zu viel gelöschter
        # Eintrag sofort erkennen: Dann stünde `e_gespeichert` als
        # Beschriftung da.
        if not hat_anzeige():
            uebersprungen('Seiten in beiden Sprachen bauen')
        else:
            import tkinter as _tk
            from scbp import hauptfenster as _hf, seiten as _st
            _schluesselartig = _re.compile(r'^[a-z][a-z0-9]*(_[a-z0-9]+){1,}$')

            def _durchsuchen(widget, gefunden):
                try:
                    _text = widget.cget('text')
                except Exception:
                    _text = None
                if isinstance(_text, str) and _schluesselartig.match(_text.strip()):
                    gefunden.append(_text.strip())
                for _kind in widget.winfo_children():
                    _durchsuchen(_kind, gefunden)

            _SEITEN = ('liste', 'fortschritt', 'allgemein', 'anzeige', 'pfade',
                       'spieltexte', 'bestand', 'wasistneu', 'ueber')
            _vorher = _spr.aktuelle()
            _kaputt, _rohe = [], []
            for _kuerzel in ('de', 'en'):
                _spr.setzen(_kuerzel)
                _f = _hf.Hauptfenster(version='0.0.0-test')
                _f.root.geometry('900x600+3000+3000')       # aus dem Blick
                for _seite in _SEITEN:
                    _rahmen = _tk.Frame(_f.root)
                    try:
                        _st.bauen(_f, _seite, _rahmen)
                        _f.root.update()
                        _durchsuchen(_rahmen, _rohe)
                    except Exception as _fehler:
                        _kaputt.append('%s/%s: %s' % (_kuerzel, _seite,
                                                      type(_fehler).__name__))
                    _rahmen.destroy()
                _f.root.destroy()
            _spr.setzen(_vorher)
            if _kaputt:
                print('       ' + '; '.join(_kaputt[:4]))
            pruefe(not _kaputt,
                   'jede Seite baut auf Deutsch und Englisch (%d Fehler)'
                   % len(_kaputt))
            if _rohe:
                print('       roh angezeigt: %s' % ', '.join(sorted(set(_rohe))[:6]))
            pruefe(not _rohe,
                   'kein Schlüsselname als Beschriftung (%d gefunden)'
                   % len(set(_rohe)))

            # ⚠⚠ **Ein Suchfeld darf sich beim Tippen nicht selbst wegbauen.**
            #
            # Gemeldet am 04.09.2026 zur Joystick-Seite: „springt die Maus
            # raus, ich muss immer erneut reinklicken und kann nur einen
            # Buchstaben eingeben." Ursache war eine Zeichenfunktion, die an
            # der Suchvariablen hing und dabei die **ganze** Seite neu baute —
            # samt des Feldes, in das gerade getippt wurde.
            #
            # Es ist im Projekt nicht das erste Mal passiert, deshalb wird es
            # ab jetzt geprüft statt erinnert: Nach einem simulierten
            # Tastendruck muss das Eingabefeld **dasselbe Objekt** sein.
            # Gegengeprüft mit eingebautem Fehler — schlägt dann an.
            _felder_kaputt = []
            _spr.setzen('de')
            _f = _hf.Hauptfenster(version='0.0.0-test')
            _f.root.geometry('900x600+3000+3000')
            for _seite in ('joysticks',):
                _rahmen = _tk.Frame(_f.root)
                try:
                    _st.bauen(_f, _seite, _rahmen)
                    _f.root.update()

                    def _eingaben(w, raus):
                        if w.winfo_class() == 'Entry':
                            raus.append(w)
                        for _k in w.winfo_children():
                            _eingaben(_k, raus)

                    _vor = []
                    _eingaben(_rahmen, _vor)
                    if _vor:
                        _ziel = _vor[0]
                        _ziel.insert(0, 'a')          # ein Buchstabe
                        _f.root.update()
                        _nach = []
                        _eingaben(_rahmen, _nach)
                        # Dasselbe Objekt? `winfo_exists` allein genügt nicht:
                        # Ein neu gebautes Feld existiert auch.
                        if not _nach or _nach[0] is not _ziel:
                            _felder_kaputt.append(_seite)
                except Exception as _fehler:
                    _felder_kaputt.append('%s: %s' % (_seite,
                                                      type(_fehler).__name__))
                _rahmen.destroy()
            _f.root.destroy()
            _spr.setzen(_vorher)
            if _felder_kaputt:
                print('       Suchfeld wird beim Tippen neu gebaut: %s'
                      % ', '.join(_felder_kaputt))
            pruefe(not _felder_kaputt,
                   'das Suchfeld überlebt einen Tastendruck')

        # Und die Gegenrichtung: Ein Schlüssel, den es nur auf Deutsch gibt, ist
        # eine halbe Übersetzung — die wirkt schlechter als gar keine.
        _halbe = [k for k, v in _spr.TEXTE.items()
                  if not isinstance(v, tuple) or len(v) < 2 or not v[1]]
        if _halbe:
            print('       ohne englische Version: %s' % ', '.join(sorted(_halbe)[:6]))
        pruefe(not _halbe,
               'jeder Text hat eine englische Version (%d ohne)' % len(_halbe))

        # ------------------------------------------------------------------ 18
        # Meldungen ziehen beim Sprachwechsel mit.
        #
        # ⚠ Abschnitt 17 prüft, dass kein Text **fest** in der Oberfläche
        # steht. Das reicht nicht: Ein Text kann sauber durch `t()` laufen und
        # trotzdem falsch stehen bleiben — nämlich dann, wenn er einmal fertig
        # zusammengesetzt in ein Label geschrieben wurde. Wer danach die
        # Sprache wechselt, hat ein englisches Fenster mit einer deutschen
        # Zeile darin. Genau so gefunden am 26.08.2026 bei „Keine
        # Log-Sicherungen gefunden".
        #
        # Der Weg dagegen: `sprache.Satz` trägt Schlüssel und Werte mit, das
        # Label merkt sich den Träger, `_neu_beschriften()` wertet ihn neu aus.
        print()
        print('18. Meldungen ziehen beim Sprachwechsel mit')
        from scbp import sprache as spr18, logquelle as lq18

        # a) Die Quelle liefert einen Träger, keinen fertigen Satz.
        grund = lq18._luecke_pruefen(0.0, [__file__])['grund']
        pruefe(spr18.auffrischbar(grund),
               'die Lücken-Meldung kommt als Träger, nicht als fertiger Text')

        spr18.setzen('de'); deutsch = str(grund)
        spr18.setzen('en'); englisch = str(grund)
        spr18.setzen('de')
        pruefe(deutsch != englisch and 'First run' in englisch,
               'derselbe Träger spricht beide Sprachen')
        # Das Datum steckt mit drin: im Deutschen 22.08.2026, im Englischen
        # 2026-08-22. Ein fertig formatiertes Datum bliebe deutsch.
        pruefe(englisch.count('-') >= 2,
               'auch das Datum wechselt seine Schreibweise')

        # b) Am echten Fenster — nicht nur an der Datenschicht.
        if ANZEIGE:
            import tkinter as _tk18
            spr18.setzen('de')
            _wz = _tk18.Tk(); _wz.withdraw()
            ov18 = None
            try:
                import sc_bp_watcher as _w18
                ov18 = _w18.Overlay(wurzel=_wz)
                ov18.root.withdraw()
                ov18.add_hinweis(grund)
                ov18._status_setzen(spr18.Satz('katalog_holt'))
                ov18.root.update()

                def _zeilen():
                    raus = []
                    for zeile in ov18.list.pack_slaves():
                        for teil in zeile.winfo_children():
                            if getattr(teil, '_quelle', None) is not None:
                                raus.append(teil.cget('text'))
                    return raus

                vorher_h = _zeilen()
                vorher_s = ov18.status.cget('text')
                spr18.setzen('en')
                ov18.root.update()
                nachher_h = _zeilen()
                nachher_s = ov18.status.cget('text')

                pruefe(vorher_h and nachher_h and vorher_h != nachher_h
                       and 'First run' in nachher_h[0],
                       'eine stehende Hinweiszeile wird mit übersetzt')
                pruefe(vorher_s != nachher_s and 'Fetching' in nachher_s,
                       'die Statuszeile wird mit übersetzt')
            finally:
                spr18.setzen('de')
                if ov18 is not None:
                    try:
                        ov18.root.destroy()
                    except Exception:
                        pass
                else:
                    _wz.destroy()
        else:
            uebersprungen('Sprachwechsel am Overlay')

        # c) Rückfallschutz. Beides sind Fehler, die sich beim nächsten Umbau
        #    leicht wieder einschleichen — und die man am laufenden Programm
        #    erst merkt, wenn jemand die Sprache umstellt.
        import re as _re18
        _quelle18 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'),
                         encoding='utf-8').read()
        _alt_puts = _re18.findall(
            r"q\.put\(\('(?:status|hinweis)', sprache\.t\(", _quelle18)
        pruefe(not _alt_puts,
               'keine Meldung geht als fertiger Text in die Warteschlange '
               '(%d gefunden)' % len(_alt_puts))

        # Jeder Schreibzugriff auf die Statuszeile muss durch `_status_setzen`
        # gehen, sonst merkt sich niemand die Quelle — und beim nächsten
        # Sprachwechsel springt eine **ältere** Meldung zurück auf den Schirm.
        _direkt = [n for n, z in enumerate(_quelle18.splitlines(), 1)
                   if 'self.status.config(' in z]
        # Erlaubt: die Zeile in `_status_setzen` selbst und die beiden in
        # `_neu_beschriften`, die genau dort bewusst neu setzen.
        pruefe(len(_direkt) <= 3,
               'die Statuszeile wird nicht an der Merkstelle vorbei gesetzt '
               '(%d Direktzugriffe)' % len(_direkt))

        # ------------------------------------------------------------------
        # ⚠ Am 27.08.2026 stand im Auswahlfeld „4.10.0 (21)" und in der Liste
        # darunter „Nichts gefunden". Grund: Das Feld liest die Patch-Historie
        # direkt, der Filter prüft den Stempel `seit` im Katalog — und gestempelt
        # wurde nur beim Neubau. Wer seinen Katalog vor rc55 geholt hat, wartet
        # sonst bis zum nächsten Patch, und der wäre obendrein stumm geblieben.
        print()
        print('19. Der Katalog holt fehlende Patch-Stempel nach')
        from scbp import katalog as kat19, patchhistorie as ph19

        os.environ['SC_BP_HOME'] = os.path.join(basis, 'stempel')
        os.makedirs(os.environ['SC_BP_HOME'], exist_ok=True)
        _kat19 = os.path.join(os.environ['SC_BP_HOME'], 'katalog-cache.json')
        _hist19 = {'4.9.9-live.1': {'datum': '2026-01-01',
                                    'neu': ['Alter Bauplan']},
                   '4.10.0-live.2': {'datum': '2026-08-26',
                                     'neu': ['Neuer Bauplan']}}
        ph19._schreib(os.path.join(os.environ['SC_BP_HOME'],
                           'patch-historie.json'), _hist19)

        def _katalog_schreiben(version, **zusatz):
            eintraege = {'alter bauplan': {'n': 'Alter Bauplan'},
                         'neuer bauplan': {'n': 'Neuer Bauplan'}}
            eintraege.update(zusatz)
            with open(_kat19, 'w', encoding='utf-8') as f:
                json.dump({'version': version, 'geholt': '',
                           'bauplaene': eintraege, 'missionen': {}}, f)

        # a) Ein Katalog ohne jeden Stempel — wie bei jedem Bestandsnutzer.
        _katalog_schreiben('4.10.0-live.2')
        pruefe(kat19.stempel_nachziehen() == 2,
               'beide fehlenden Stempel werden nachgetragen')

        _d19 = kat19.laden()
        pruefe(_d19['bauplaene']['neuer bauplan'].get('seit') == '4.10.0-live.2',
               'der Neuzugang trägt die Version, die ihn gebracht hat')
        pruefe(kat19.neue(_d19) == {'neuer bauplan'},
               '„neu im Spiel" zeigt genau den einen Zugang')

        # b) Zweiter Start: nichts zu tun. Sonst schriebe das Werkzeug bei
        #    jedem Start eine Megabyte-Datei neu, ohne dass sich etwas ändert.
        pruefe(kat19.stempel_nachziehen() == 0,
               'ein zweiter Start schreibt nicht noch einmal')

        # c) Ohne Katalog darf nichts passieren und nichts fliegen.
        os.remove(_kat19)
        pruefe(kat19.stempel_nachziehen() == 0,
               'ohne Katalog bleibt es ruhig')

        # d) ⚠ Der teurere Fehler: Fehlt die Vergleichsgrundlage, hielte
        #    `erzeugen()` jeden Bauplan für „schon immer da" und der nächste
        #    Patch meldete NULL Zugänge. Der vorhandene Katalog ist die
        #    richtige Grundlage — was darin steht, war vorher im Spiel.
        _katalog_schreiben('4.10.0-live.2')
        pruefe(not ph19.gesehen(), 'Ausgangslage: keine Vergleichsgrundlage')
        pruefe(kat19._vergleichsgrundlage() == {'alter bauplan', 'neuer bauplan'},
               'ersatzweise gilt der vorhandene Katalog als Grundlage')
        pruefe('quantum drive' not in kat19._vergleichsgrundlage(),
               'was der Katalog nicht kennt, bleibt ein Zugang')

        # Ist die Grundlage vorhanden, gilt sie — und nicht der Katalog.
        ph19.gesehen_setzen({'alter bauplan'})
        pruefe(kat19._vergleichsgrundlage() == {'alter bauplan'},
               'die eigene Grundlage schlaegt den Katalog')

        # ⚠ Beim allerersten Katalogbau gibt es beides nicht — dann MUSS die
        # Grundlage leer bleiben, sonst staenden alle 738 als „neu" da.
        os.remove(kat19.pfade.app_datei('bauplaene-gesehen.json'))
        os.remove(_kat19)
        pruefe(kat19._vergleichsgrundlage() == set(),
               'beim allerersten Bau bleibt sie leer')

        # e) ⚠ Und wird das Nachziehen ueberhaupt angestossen? Die Funktion
        #    allein nuetzt nichts, wenn sie niemand ruft — und sie muss VOR dem
        #    Netz drankommen, sonst bleibt der Stempel aus, sobald die Leitung
        #    weg ist. Deshalb hier ohne Netz: Die Versionsabfrage wird
        #    stillgelegt, gestempelt werden muss trotzdem.
        _katalog_schreiben('4.10.0-live.2')
        _echte_version = kat19.aktuelle_version
        kat19.aktuelle_version = lambda: ''
        try:
            kat19.aktualisieren()
        finally:
            kat19.aktuelle_version = _echte_version
        pruefe(kat19.laden()['bauplaene']['neuer bauplan'].get('seit')
               == '4.10.0-live.2',
               'auch ohne Netz stempelt der Start nach')

        # ------------------------------------------------------------------
        # ⚠ Am 27.08.2026 antwortete „Auf Aktualität prüfen" mit
        # `name 'datei' is not defined` — ein Rückruf griff auf eine Variable
        # zu, die es in seiner Funktion nie gab. Python merkt das erst beim
        # **Klicken**; im Selbsttest lief die Zeile nie. Zwei weitere Fälle
        # derselben Art steckten still im Code (`os` im Bestandsfenster, `t`
        # statt `sprache.t` beim Ordner-Umzug) — beide in einem `except`
        # begraben, also unsichtbar.
        #
        # Ein undefinierter Name ist ohne Ausführen findbar. Genau das prüft
        # `pyflakes`. Fehlt es, wird die Prüfung übersprungen statt zu scheitern:
        # Der Selbsttest soll auf jedem Rechner laufen, auch ohne Zusatzpaket.
        print()
        print('20. Kein Zugriff auf Namen, die es nicht gibt')
        try:
            from pyflakes import api as _pfapi, reporter as _pfrep
        except ImportError:
            print('  [--]   pyflakes fehlt — Prüfung übersprungen '
                  '(pip install pyflakes)')
        else:
            import io as _io20
            _wurzel20 = os.path.dirname(os.path.dirname(
                os.path.abspath(__file__)))
            _aus, _err = _io20.StringIO(), _io20.StringIO()
            for _ort in ('scbp', 'tools', 'sc_bp_watcher.py'):
                _pfapi.checkRecursive([os.path.join(_wurzel20, _ort)],
                                      _pfrep.Reporter(_aus, _err))
            _offen = [z for z in _aus.getvalue().splitlines()
                      if 'undefined name' in z]
            for _z in _offen:
                print('         ' + _z.replace(_wurzel20 + os.sep, ''))
            pruefe(not _offen,
                   'kein undefinierter Name im ganzen Programm (%d gefunden)'
                   % len(_offen))

        # ------------------------------------------------------------------
        # ⚠ Am 27.08.2026 meldete gemeldet, dass bei „sehr gross" die Knoepfe
        # der Overlay-Wahl abgeschnitten sind. Ein benanntes Tk-Font wirkt
        # sofort auf jeden Text — aber die gezeichneten Rundknoepfe legen ihre
        # Leinwand beim Bauen **einmal** auf `schrift.measure(text)` fest.
        # Gemessen: 177 px Kasten, 206 px Text. 29 px fehlten.
        print()
        print('21. Groessere Schrift sprengt keine Knoepfe mehr')
        import tkinter as tk21
        import tkinter.font as tkfont21
        from scbp import seiten as se21
        from scbp.hauptfenster import Hauptfenster as HF21

        wurzel = _wurzel()
        _sch21 = tkfont21.Font(root=wurzel, family='Segoe UI', size=10)

        class _Traeger21:
            f_klein = _sch21

        _wahl21 = se21._wahl(_Traeger21(), tk21.Frame(wurzel),
                             [('popup', 'nur bei einem Neuzugang')],
                             'popup', lambda k: None)
        wurzel.update_idletasks()
        _vorher21 = _wahl21.winfo_children()[0].winfo_reqwidth()
        _sch21.configure(size=12)              # klein -> sehr gross
        wurzel.update_idletasks()
        _nachher21 = _wahl21.winfo_children()[0].winfo_reqwidth()
        _noetig21 = _sch21.measure('nur bei einem Neuzugang') + 26

        # a) Die Falle gibt es wirklich — sonst prueft (b) ins Leere.
        pruefe(_nachher21 == _vorher21 and _noetig21 > _nachher21,
               'ein fertiger Rundknopf waechst NICHT von allein (%d px fehlen)'
               % (_noetig21 - _nachher21))

        # b) Deshalb muss das Umstellen der Schriftgroesse neu aufbauen — und
        #    die Rueckmeldung DANACH sagen, sonst zerstoert der Aufbau sie.
        _ablauf21 = []

        class _Fenster21:
            f_grund = f_fett = f_klein = f_titel = f_zeichen = _sch21
            beim_schriftwechsel = None
            root = wurzel
            neu_aufbauen = lambda self: _ablauf21.append('aufbauen')
            sagen = lambda self, text: _ablauf21.append('sagen')

        HF21.schriftgroesse_setzen(_Fenster21(), 'gross')
        wurzel.update()                        # die `after`-Schlange abarbeiten
        pruefe(_ablauf21 == ['aufbauen', 'sagen'],
               'Schriftwechsel baut neu auf und meldet danach (%s)'
               % (' -> '.join(_ablauf21) or 'nichts passiert'))

        # c) ⚠ Die Mindestgroesse haengt an der Seitenleiste, die Seitenleiste
        #    an der Schrift. Ohne Nachziehen ragen bei „sehr gross" die unteren
        #    Eintraege („Star Citizen starten", „Kaffee spendieren", „Discord")
        #    aus dem Fenster — sie werden von unten gepackt und fallen heraus.
        #    Gerechnet wurde immer richtig; der Aufruf fehlte im Neuaufbau.
        import inspect as _ins21
        _quelle21 = _ins21.getsource(HF21.neu_aufbauen)
        pruefe('_mindesthoehe_nachziehen' in _quelle21,
               'der Neuaufbau zieht die Mindestgroesse nach')

        # d) ⚠ Die zwei Kanal-Kaesten muessen gleich gross sein. `pack` kann das
        #    nicht: Es verteilt nur den UEBERSCHUSS gleichmaessig, der laengere
        #    Text bleibt breiter. Nur `grid` mit `uniform` sagt Gleichheit zu.
        # ⚠ Ohne echte Fenstergroesse meldet Tk fuer beide Kaesten 1 Pixel —
        # dann waeren sie „gleich gross" und die Pruefung ginge immer durch.
        # Deshalb eine Groesse setzen und das Layout wirklich rechnen lassen.
        # ⚠ `_wurzel()` liefert ein verstecktes Fenster — ein verstecktes Fenster
        # rechnet Tk nicht aus, beide Kaesten meldeten 1 Pixel. Dann waeren sie
        # „gleich gross" und die Pruefung ginge immer durch. Also kurz zeigen.
        # ⚠ **Weit ausserhalb des Bildschirms** zeigen, nicht mittendrin.
        # Tk rechnet ein verstecktes Fenster nicht aus, gezeigt werden muss es
        # also — aber es muss niemand sehen. Der Selbsttest laeuft nach jeder
        # Aenderung, und jedes Mal sprang hier ein 1100x760-Fenster ueber den
        # Bildschirm und riss den Fokus mit. Gemeldet am 28.08.2026: „du hast
        # mich staendig aus dem rausgezogen was ich mache, den ganzen Abend
        # schon." Negative Koordinaten loesen das auf beiden Systemen.
        wurzel.geometry('1100x760+-4000+-4000')
        wurzel.attributes('-alpha', 0.0)
        wurzel.deiconify()
        _rahmen21 = tk21.Frame(wurzel)
        _rahmen21.pack(fill='both', expand=True)

        class _Traeger21b:
            f_klein = _sch21
            f_fett = _sch21
            version = '0.0.0'

        _t21 = _Traeger21b()
        se21._kanalkasten(_t21, _rahmen21, 'Kurz', 'Zwei Woerter.',
                          True, lambda: None, platz=0)
        se21._kanalkasten(_t21, _rahmen21, 'Deutlich laenger',
                          'Ein merklich laengerer Satz, der mehr Platz braucht '
                          'als der andere Kasten daneben.',
                          False, lambda: None, platz=1)
        wurzel.update()
        _br21 = [k.winfo_width() for k in _rahmen21.winfo_children()]
        _ho21 = [k.winfo_height() for k in _rahmen21.winfo_children()]
        # ⚠ Erst pruefen, dass ueberhaupt gezeichnet wurde. Sonst vergliche man
        # zwei Einsen und haette nichts geprueft.
        pruefe(len(_br21) == 2 and min(_br21) > 100,
               'die Kanal-Kaesten wurden wirklich gezeichnet (%s px)' % _br21)
        pruefe(len(_br21) == 2 and _br21[0] == _br21[1] and _ho21[0] == _ho21[1],
               'beide Kanal-Kaesten sind gleich gross (%s px breit, %s hoch)'
               % (_br21, _ho21))

        wurzel.withdraw()
        wurzel.destroy()

        print()
        print('26. Ein Absturz und die Bedienung hinterlassen eine Spur')
        # ⚠ Bomb20 meldete am 27.08.2026 einen reproduzierbaren Absturz beim
        # Oeffnen von "Was ist neu" — und sein Bericht wusste NICHTS davon. Die
        # Fehlerhaken fangen Python-Ausnahmen; ein harter Abbruch ist keine, und
        # die Spur endete beim letzten Startschritt.
        #
        # ⚠ Der erste Anlauf (rc74) hat den Fehler halb wiederholt: Start und
        # Bedienung landeten in EINEM Topf, der Bericht nahm die letzten zwoelf
        # Zeilen — fuenf Klicks genuegten, und der Startverlauf war weg.
        # Ein rc74-Bericht zeigte keinen einzigen Startschritt mehr.
        import os as os26
        from scbp import fehler as fe26
        from scbp import pfade as pf26

        ordner26 = os.path.join(basis, 'spur26')
        os26.makedirs(ordner26, exist_ok=True)
        alt_datei26 = pf26.app_datei
        try:
            pf26.app_datei = lambda name: os26.path.join(ordner26, name)
            if hasattr(fe26.spur, '_offen'):
                del fe26.spur._offen

            fe26.spur('Start, Version 3.0.0-test, testos')
            fe26.spur('Tk-Wurzel steht')
            # ⚠ Genau die Zeile, an der getrennt wird — nicht eine
            # nachgetippte Fassung davon. Bis rc42 stand hier
            # „Hauptschleife laeuft" ohne Umlaut; die Pruefung lief gruen,
            # obwohl das Programm etwas anderes schreibt.
            fe26.spur(fe26.SPUR_GRENZE)
            for _ in range(40):
                fe26.spur('Seite liste: bauen beginnt')
                fe26.spur('Seite liste: steht')

            start26, seiten26 = fe26.spur_geteilt()
            pruefe(len(start26) == 3,
                   'Start und Bedienung werden getrennt (%d Startzeilen)' % len(start26))
            # ⚠⚠ **Nur die eigenen Zeilen zaehlen.** Diese Pruefung schrieb
            # bis zum 04.09.2026 gegen `len(seiten26) == 80` — und schlug
            # sporadisch mit 81 fehl, ohne dass sich am Programm etwas
            # geaendert haette.
            #
            # Ursache: `Hauptfenster._seiten_vorbauen` laeuft **400 ms nach dem
            # Oeffnen** ueber `after()` und schreibt dann `Vorbau xy: N ms` in
            # die Spur. Hat eine fruehere Pruefung ein Fenster gebaut, faellt
            # dieser Rueckruf mitten in diese hier — und landet in der Datei,
            # auf die `app_datei` gerade umgebogen ist. Ob er trifft, haengt am
            # Zeitpunkt; deshalb mal 80, mal 81.
            #
            # Die Zahl aufzuweichen (`>= 80`) waere der falsche Ausweg: Dann
            # wuerde die Pruefung eine echte Kuerzung nicht mehr bemerken.
            # Gezaehlt wird stattdessen, was diese Pruefung selbst geschrieben
            # hat — das ist genau die Frage, um die es geht.
            _eigene26 = [z for z in seiten26 if 'Seite liste:' in z]
            _fremd26 = [z for z in seiten26 if 'Seite liste:' not in z]
            pruefe(len(_eigene26) == 80,
                   'die Seitenwechsel stehen vollstaendig da (%d%s)'
                   % (len(_eigene26),
                      '' if not _fremd26
                      else '; dazu %d fremde Zeile(n): %s'
                           % (len(_fremd26), _fremd26[0][:50])))

            # Und jetzt der Punkt, der in rc74 fehlte.
            fe26._spur_kuerzen(pf26.app_datei(fe26.SPUR_DATEI))
            start27, seiten27 = fe26.spur_geteilt()
            pruefe(len(start27) == 3,
                   'der Startverlauf ueberlebt das Kuerzen')
            # ⚠⚠ **Dieselbe Sporadik wie oben — hier war sie nur nicht behoben.**
            # Der `after()`-Rueckruf aus einer frueheren Pruefung kann auch
            # NACH dem Kuerzen noch eine Zeile hineinschreiben; dann sind es
            # SPUR_REST + 1, und die Pruefung faellt, ohne dass am Programm
            # etwas falsch waere. Am 04.09.2026 zweimal gemessen: derselbe
            # Code, ein Lauf rot, einer gruen.
            #
            # Gezaehlt wird deshalb auch hier nur, was diese Pruefung selbst
            # geschrieben hat. Die Zahl aufzuweichen waere der falsche Ausweg —
            # dann bemerkte sie eine echte Kuerzung nicht mehr.
            # ⚠⚠ **Gezaehlt wird die GESAMTZAHL, nicht nur die eigenen Zeilen.**
            # Genau das ist die Eigenschaft, um die es geht: Nach dem Kuerzen
            # stehen SPUR_REST Zeilen da — wer sie geschrieben hat, ist dafuer
            # gleichgueltig.
            #
            # Der erste Anlauf zaehlte nur die eigenen und schlug deshalb
            # weiterhin fehl, sobald der `after()`-Rueckruf einer frueheren
            # Pruefung eine Zeile beisteuerte: 59 eigene + 1 fremde = 60
            # gekuerzte, und die Pruefung sah 59. Die Meldung nennt die fremden
            # trotzdem — sonst waere nicht zu erkennen, woher eine Abweichung
            # kaeme.
            _fremd27 = [z for z in seiten27 if 'Seite liste:' not in z]
            pruefe(len(seiten27) == fe26.SPUR_REST,
                   'gekuerzt wird nur der Bedienteil (%d Zeilen%s)'
                   % (len(seiten27),
                      '' if not _fremd27
                      else ', davon %d fremde' % len(_fremd27)))

            # Der Absturzfaenger legt einen vorigen Lauf beiseite.
            with open(pf26.app_datei(fe26.ABSTURZ_DATEI), 'w', encoding='utf-8') as f26:
                f26.write('Current thread 0x0000 (most recent call first):\n')
            pruefe(fe26.absturzfaenger(), 'der Absturzfaenger laesst sich setzen')
            pruefe(len(fe26.letzter_absturz()) == 1,
                   'der Abbruch des vorigen Laufs ist lesbar')
            pruefe(fe26.absturz_abhaken() and not fe26.letzter_absturz(),
                   'und laesst sich abhaken')
        finally:
            pf26.app_datei = alt_datei26
            if hasattr(fe26.spur, '_offen'):
                del fe26.spur._offen

        # Beides muss auch wirklich im Bericht landen, sonst nuetzt es nichts.
        quelle26 = open(os.path.join(WURZEL, 'scbp', 'bericht.py'),
                        encoding='utf-8').read()
        pruefe("t('b_spur_seiten')" in quelle26,
               'der Bericht hat einen eigenen Abschnitt fuer die Seiten')
        pruefe("fehler.letzter_absturz" in quelle26,
               'und einen fuer den harten Abbruch')
        pruefe("'Seite diagnose' in seiten[-1]" in quelle26,
               'die Diagnose-Seite selbst steht nicht als letzte Zeile drin')
        quelle26b = open(os.path.join(WURZEL, 'scbp', 'hauptfenster.py'),
                         encoding='utf-8').read()
        # ⚠ Drei Stellen seit dem 28.08.2026: „bauen beginnt" beim ersten
        # Aufbauen, „zeigen" beim erneuten Einblenden, „steht" am Ende. Vorher
        # gab es die mittlere nicht — ging beim zweiten Besuch etwas schief,
        # fehlte die Zeile GANZ statt zur Haelfte, und der Bericht verspricht,
        # dass die letzte Zeile ohne „steht" die ist, an der es hing.
        pruefe(quelle26b.count("fehler.spur('Seite ") == 3,
               'jeder Seitenwechsel schreibt zwei Zeilen (bauen bzw. zeigen, dann steht)')
        pruefe("Seite %s: zeigen" in quelle26b,
               'auch der zweite Besuch hinterlaesst eine Spur')
        quelle26c = open(os.path.join(WURZEL, 'sc_bp_watcher.py'),
                         encoding='utf-8').read()
        pruefe('fehler.absturzfaenger()' in quelle26c,
               'der Faenger wird beim Start gesetzt')

        print()
        print('27. Angaben am Gegenstand: Kuerzel aus der Beschreibung')
        # ⚠ Die Fallen hier sind Datenfallen, keine Programmierfehler — sie
        # fallen nur auf, wenn man die echte `global.ini` daneben legt. Beim Bau
        # (27.08.2026) stand sechsmal `Individuell angefertigt` und dreimal
        # `N/A` im Feld Guetegrad; wer den ersten Buchstaben nimmt, schreibt
        # `(Ind/4/I)` in einen Spielnamen. So etwas sieht man erst im Spiel.
        from scbp import angaben as an27

        def besch27(**felder):
            """Eine Beschreibungszeile bauen — `\\n` ist die ZEICHENFOLGE."""
            return '\\n'.join('%s: %s' % (k, v) for k, v in felder.items())

        pruefe(an27.aus_beschreibung(
                   besch27(**{'Größe': 'S1', 'Gütegrad': 'A',
                              'Klasse': 'Military (Militär)'})) == '(Mil/1/A)',
               'Komponente wird zu Klasse/Groesse/Guete')
        pruefe(an27.aus_beschreibung(
                   besch27(**{'Größe': 'S2', 'Verfolgungssignal': 'Infrarot'}))
               == '(IR2)',
               'Rakete bekommt den Suchkopf, keine Fraktion')
        pruefe(an27.aus_beschreibung(
                   besch27(**{'Klasse': 'Ballistisch'})) == '(Bal)',
               'Waffe: die Klasse allein genuegt (FPS-Waffen haben keine Groesse)')
        pruefe(an27.aus_beschreibung(besch27(**{'Größe': 'S3'})) is None,
               'Groesse allein gibt KEINEN Zusatz (waere Laerm im Namen)')
        pruefe(an27.aus_beschreibung(
                   besch27(**{'Größe': 'S4', 'Gütegrad': 'Individuell angefertigt',
                              'Klasse': 'Industrial (Industrie)'})) == '(Ind/4/–)',
               'ein Guetegrad, den es nicht gibt, wird zum Strich')
        pruefe(an27.aus_beschreibung(
                   besch27(**{'Größe': 'S1', 'Gütegrad': 'N/A',
                              'Klasse': 'Zivil'})) == '(Civ/1/–)',
               '`N/A` ebenso — und die Kurzform `Zivil` wird erkannt')
        pruefe(an27.aus_beschreibung(
                   besch27(**{'Größe': 'S2 (Nur Fahrzeuge)',
                              'Klasse': 'Military', 'Gütegrad': 'B'}))
               == '(Mil/–/B)',
               'eine Groesse mit Zusatztext gehoert nicht ins Kuerzel')
        # Die Uebersetzung ist uneinheitlich: dieselbe Klasse in drei Formen.
        pruefe(len({an27.aus_beschreibung(
                        besch27(**{'Größe': 'S1', 'Gütegrad': 'C', 'Klasse': k}))
                    for k in ('Civilian (Zivil)', 'Zivil', 'Civilian')}) == 1,
               'alle drei Schreibweisen derselben Klasse ergeben dasselbe')
        pruefe(an27.zusatz_entfernen('Spark I-G Missile (CS1)')
               == 'Spark I-G Missile',
               'ein Zusatz des SC Deutsch Launchers wird abgeschnitten')
        pruefe(an27.zusatz_entfernen('Inspire Advanced (Ind/2/C)')
               == 'Inspire Advanced',
               'und der eigene ebenso — sonst stapeln sie sich')
        pruefe(an27.zusatz_entfernen('Omnisky III Cannon')
               == 'Omnisky III Cannon',
               'ein Name ohne Zusatz bleibt unangetastet')
        # Der ganze Weg: Tabelle aus Rohzeilen, ueber den gemeinsamen Stamm.
        zeilen27 = ['item_DescXY_Test=' + besch27(**{'Größe': 'S3',
                                                     'Gütegrad': 'B',
                                                     'Klasse': 'Stealth (Tarnung)'}),
                    'item_NameXY_Test=Testkuehler',
                    'item_NameOhne_Beschreibung=Einsam']
        tab27 = an27.tabelle_bauen(zeilen27)
        pruefe(tab27.get('item_NameXY_Test') == '(Sth/3/B)',
               'Beschreibung und Name finden ueber den Schluesselstamm zusammen')
        pruefe('item_NameOhne_Beschreibung' not in tab27,
               'ein Name ohne Beschreibung kommt nicht in die Tabelle')

        print()
        print('28. Ohne Launcher: Ordner und user.cfg entstehen selbst')
        # ⚠ Gemeldet am 27.08.2026: „das hat bei mir und meinem bruder nur
        # geklappt WEIL wir vorher den launcher hatten von sc deutsch." Genau
        # das ist der ungetestete Fall — wer den SC Deutsch Launcher nie hatte,
        # hat **keinen** Ordner `data/Localization/<sprache>/`, und ohne den
        # landet die Datei irgendwo, wo Star Citizen sie nicht sucht.
        #
        # Dazu die Tonspur: Star Citizen hat **keine deutsche Sprachausgabe**.
        # Ohne `g_languageAudio = english` neben der deutschen Textsprache
        # fehlt der Ton. Der Launcher setzt beides, also müssen wir es auch.
        from scbp import uebersetzung as ue28
        frisch28 = os.path.join(basis, 'frischeinstallation', 'LIVE')
        os.makedirs(frisch28)
        open(os.path.join(frisch28, 'Data.p4k'), 'w').close()

        ziel28 = ue28.ziel_ini('german_(germany)', frisch28)
        pruefe(ziel28.endswith(os.path.join('data', 'Localization',
                                            'german_(germany)', 'global.ini')),
               'der Zielpfad steht dort, wo Star Citizen sucht')
        os.makedirs(os.path.dirname(ziel28), exist_ok=True)
        pruefe(os.path.isdir(os.path.dirname(ziel28)),
               'die ganze Ordnerkette entsteht ohne Launcher')

        ue28.user_cfg_setzen('german_(germany)', 'english', frisch28)
        cfg28 = open(os.path.join(frisch28, 'user.cfg'), encoding='utf-8').read()
        pruefe('g_language = german_(germany)' in cfg28,
               'g_language wird gesetzt — sonst liest das Spiel die Datei nicht')
        pruefe('g_languageAudio = english' in cfg28,
               'g_languageAudio = english MUSS mit rein (SC hat keinen deutschen Ton)')

        # Eine vorhandene user.cfg voller Grafikeinstellungen darf das nicht
        # verlieren — dort steht die Arbeit des Spielers drin.
        cfgpfad28 = os.path.join(frisch28, 'user.cfg')
        with open(cfgpfad28, 'w', encoding='utf-8') as f28:
            f28.write('r_DisplayInfo = 3\nsys_maxfps = 0\n'
                      'g_language = english\n')
        ue28.user_cfg_setzen('german_(germany)', 'english', frisch28)
        cfg28b = open(cfgpfad28, encoding='utf-8').read()
        pruefe('r_DisplayInfo = 3' in cfg28b and 'sys_maxfps = 0' in cfg28b,
               'vorhandene Grafikeinstellungen bleiben unangetastet')
        pruefe('g_language = english' not in cfg28b,
               'eine alte Sprachzeile wird ersetzt, nicht verdoppelt')
        pruefe(cfg28b.count('g_language =') == 1,
               'g_language steht genau einmal da')

        # Der Weg ueber die EINSTELLUNGEN (Assistent abgebrochen) muss dasselbe
        # tun wie der Assistent. Beide laufen ueber `uebersetzung.holen()`.
        quelle28 = open(os.path.join(WURZEL, 'scbp', 'uebersetzung.py'),
                        encoding='utf-8').read()
        holen28 = quelle28[quelle28.index('def holen('):]
        holen28 = holen28[:holen28.index('\ndef ', 1)] if '\ndef ' in holen28[1:] else holen28
        pruefe('os.makedirs(' in holen28,
               'holen() legt die Ordnerkette selbst an')
        pruefe('user_cfg_setzen(' in holen28,
               'holen() setzt die user.cfg — auch wenn der Assistent uebersprungen wurde')
        pruefe(ue28.QUELLEN['deutsch']['ton'] == 'english',
               'die deutsche Quelle bringt den englischen Ton mit')

        # StarStrings (MrKraken) ist derselbe Fall — nur mit englischem
        # Zielordner. Gemeldet: „ist ja wie die deutsche im grunde."
        ss28 = os.path.join(basis, 'starstringsprobe', 'LIVE')
        os.makedirs(ss28)
        ziel_ss = ue28.ziel_ini(ue28.QUELLEN['starstrings']['sprache'], ss28)
        pruefe(ziel_ss.endswith(os.path.join('data', 'Localization',
                                             'english', 'global.ini')),
               'StarStrings landet im englischen Ordner, ebenfalls selbst angelegt')
        ue28.user_cfg_setzen(ue28.QUELLEN['starstrings']['sprache'],
                             ue28.QUELLEN['starstrings']['ton'], ss28)
        cfg_ss = open(os.path.join(ss28, 'user.cfg'), encoding='utf-8').read()
        pruefe('g_language = english' in cfg_ss,
               'auch StarStrings traegt seine Sprache in die user.cfg ein')

        # ⚠ Der Wechsel deutsch → StarStrings: Die Tonzeile stammt aus der
        # deutschen Einrichtung und muss stehen bleiben. `ton` ist bei
        # StarStrings None — eine Fassung, die dabei alles anfasst, wuerde sie
        # verlieren, und der Spieler saesse ohne Ton da.
        with open(os.path.join(ss28, 'user.cfg'), 'w', encoding='utf-8') as f_ss:
            f_ss.write('g_language = german_(germany)\n'
                       'g_languageAudio = english\n'
                       'r_VSync = 0\n')
        ue28.user_cfg_setzen('english', None, ss28)
        cfg_ss2 = open(os.path.join(ss28, 'user.cfg'), encoding='utf-8').read()
        pruefe('g_language = english' in cfg_ss2,
               'beim Wechsel wird die Textsprache umgestellt')
        pruefe('g_languageAudio = english' in cfg_ss2,
               'die Tonzeile ueberlebt den Wechsel (ton=None fasst sie nicht an)')
        pruefe('r_VSync = 0' in cfg_ss2,
               'und die Grafikeinstellung ebenso')

        # ⚠ Der dritte Weg — und der eigentliche „ohne Launcher"-Fall: Wer
        # **englisch original** spielt, will vielleicht nur die Angaben am
        # Gegenstand und gar keine Übersetzung. Der hat **gar keine**
        # `global.ini` auf der Platte, nur die `Data.p4k`. Ohne `g_language`
        # liest Star Citizen eine dort abgelegte Datei nicht einmal an.
        # Gemeldet: „sonst kann man das nie ohne eine übersetzung nutzen."
        from scbp import spieltexte as st28
        quelle_st = open(os.path.join(WURZEL, 'scbp', 'spieltexte.py'),
                         encoding='utf-8').read()
        pruefe('_sprache_eintragen(' in quelle_st,
               'holen() traegt g_language selbst ein, nicht der Aufrufer')
        pruefe(quelle_st.count('_sprache_eintragen(sprache, spielordner)') >= 2,
               'auch wenn die Datei schon da war — sonst bleibt sie ungelesen')
        # Kein Aufrufer darf sich mehr darauf verlassen, es selbst zu tun.
        for datei_st in ('assistent.py', 'einstellungsfenster.py'):
            inhalt_st = open(os.path.join(WURZEL, 'scbp', datei_st),
                             encoding='utf-8').read()
            block_st = inhalt_st[inhalt_st.index('spieltexte.holen('):][:900]
            pruefe('user_cfg_setzen(' not in block_st,
                   '%s verlaesst sich auf holen(), statt es zu wiederholen'
                   % datei_st)
        # Und der englische Zielordner entsteht genauso von selbst.
        orig28 = os.path.join(basis, 'englischoriginal', 'LIVE')
        os.makedirs(orig28)
        ziel_or = ue28.ziel_ini('english', orig28)
        os.makedirs(os.path.dirname(ziel_or), exist_ok=True)
        st28._sprache_eintragen('english', orig28)
        cfg_or = open(os.path.join(orig28, 'user.cfg'), encoding='utf-8').read()
        pruefe('g_language = english' in cfg_or,
               'englisch original: g_language wird ebenfalls gesetzt')

        print()
        print('29. Bedienelemente stehen einheitlich — Symmetrie')
        # ⚠ Gemeldet am 27.08.2026: „im gleichen tab sind die einstellings
        # schalter mal mittig mal rechts, das muss einheitlich sein, im gesamten
        # projekt gilt das natuerlich." Und: „Symetrie ist fuer mich EXTREM
        # wichtig bei eigentlich allem."
        #
        # Woher der Unterschied kam: `_feld(..., breit=True)` legt das
        # Bedienelement UNTER die Beschreibung, ueber die volle Breite — ein
        # `.pack()` ohne Anker sitzt darin **mittig**. Ohne `breit` steht es
        # rechts neben dem Text. Auf der Seite „Texte im Spiel" standen dadurch
        # drei Schiebeschalter untereinander: mittig, rechts, mittig.
        #
        # `breit=True` ist fuer BREITE Bedienelemente da (Knopfreihen, die auf
        # Englisch sonst abgeschnitten werden). Ein Schiebeschalter ist schmal
        # und gehoert nach rechts — wie in jeder Einstellungsliste.
        import re as _re29
        quelle29 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                        encoding='utf-8').read().split('\n')

        def _aufruf29(zeilen, start):
            """Der VOLLSTAENDIGE `_feld(...)`-Aufruf ab `start`.

            ⚠ Nicht bei der ersten `)` abschneiden — die schliesst `t('...')`,
            und `breit=True` steht dahinter. Genau daran ist die erste Fassung
            dieser Pruefung gescheitert: Sie meldete brav 0 Ausreisser, auch
            als absichtlich einer eingebaut wurde. Deshalb zaehlen."""
            text, tiefe = '', 0
            for _z in zeilen[start:start + 4]:
                for _c in _z:
                    text += _c
                    if _c == '(':
                        tiefe += 1
                    elif _c == ')':
                        tiefe -= 1
                        if tiefe == 0:
                            return text
                text += ' '
            return text

        offen29, falsch29 = None, []
        for _i29, _z29 in enumerate(quelle29):
            _m29 = _re29.search(r"_feld\(fenster, \w+, t\('([^']+)'\)", _z29)
            if _m29:
                _ab29 = _z29.index('_feld(') + 5
                _voll29 = _aufruf29([_z29[_ab29:]] + quelle29[_i29 + 1:], 0)
                offen29 = (_m29.group(1), 'breit=True' in _voll29, _i29 + 1)
            elif 'schiebeschalter(' in _z29 and offen29:
                if offen29[1]:
                    falsch29.append('%s (Zeile %d)' % (offen29[0], offen29[2]))
                offen29 = None
        for _f29 in falsch29:
            print('       mittig statt rechts: ' + _f29)
        pruefe(not falsch29,
               'jeder Schiebeschalter steht rechts, keiner mittig (%d Ausreisser)'
               % len(falsch29))

        print()
        print('30. Nur noch der Installer — und v2.0.0 kommt trotzdem mit')
        # Entscheidung Gemeldet am 27.08.2026: „ich will die exe ohne install
        # loswerden … sie belastet mich nur und war damals deine Entscheidung,
        # als wir sagten, wir machen es so, um Vertrauen aufzubauen. ABER das
        # haben wir doch schon, nun wollen wir es funktionierend. Und einfach."
        #
        # Zwei Auslieferungswege heissen zwei Fehlerquellen und doppelte
        # Unterstuetzung. Ab v3.0.0 gibt es unter Windows nur den Installer.
        #
        # ⚠ Der Haken, den das aufwirft: **v2.0.0 gab es NUR als nackte .exe.**
        # Ihre Update-Logik nimmt die erste Datei auf `.exe` — jetzt also den
        # Installer. Das ging frueher schief, weil die alte `einspielen()` den
        # Fund roh ueber das laufende Programm schob. ABER ihr Hilfsskript
        # startet die getauschte Datei anschliessend (`start "" "<ziel>"`) —
        # der Installer laeuft also und richtet alles ein. Was frueher der
        # Fehler war, ist jetzt der Weg hinaus.
        from scbp import aktualisierung as ak30
        yml30 = open(os.path.join(WURZEL, '.github', 'workflows',
                                  'release.yml'), encoding='utf-8').read()
        anhang30 = yml30[yml30.index('files: |'):][:400]
        pruefe('SC-BP-Watcher-Setup.exe' in anhang30,
               'der Installer haengt am Release')
        pruefe('windows/SC-BP-Watcher.exe' not in anhang30,
               'die nackte .exe haengt NICHT mehr daran')
        pruefe('AppImage' in anhang30,
               'Linux bekommt weiter sein AppImage')
        # Was gebaut wird, muss zu dem passen, was gesucht wird.
        iss30 = open(os.path.join(WURZEL, 'packaging', 'installer.iss'),
                     encoding='utf-8').read()
        pruefe('OutputBaseFilename=SC-BP-Watcher-Setup' in iss30,
               'der Installer heisst so, wie rc39-rc75 ihn suchen')
        pruefe(ak30.WINDOWS_INSTALLER[0] == '-setup.exe',
               'und die Suche faengt genau damit an')
        # Der Weg von v2.0.0: erste Datei auf .exe — das MUSS der Installer sein.
        anhaenge30 = sorted(['SC-BP-Watcher-Setup.exe',
                             'SC-BP-Watcher-x86_64.AppImage'])
        erste_exe30 = next((n for n in anhaenge30
                            if n.lower().endswith('.exe')), None)
        pruefe(erste_exe30 == 'SC-BP-Watcher-Setup.exe',
               'v2.0.0 greift den Installer — und startet ihn (%s)' % erste_exe30)
        # ⚠ Und der Installer muss dorthin, wo das Programm liegt: sonst
        # entsteht eine zweite Fassung neben der alten Datei.
        ak30q = open(os.path.join(WURZEL, 'scbp', 'aktualisierung.py'),
                     encoding='utf-8').read()
        start30 = ak30q[ak30q.index("schalter = '/SILENT"):][:1400]
        pruefe('/DIR=' in start30,
               'der Installer bekommt /DIR — ersetzen statt danebenlegen')
        pruefe('sys.executable' in start30,
               'und zwar den Ordner des laufenden Programms')

        print()
        print('31. Das Schloss holt einen aus dem Durchreichen zurueck')
        # ⚠ Gemeldet am 27.08.2026: „der zweite Programmstart ist die denkbar
        # duemmste Loesung, weil man dann raustabben muss aus dem Spiel."
        #
        # Und er hat recht: Wer Klicks durchreichen laesst, will im Spiel
        # bleiben. Bis dahin fuehrte der einzige Rueckweg genau dort hinaus.
        # Ryze loest es beim TeamSpeak-Plugin mit einem Schloss, das anklickbar
        # bleibt — dasselbe macht jetzt ein eigenes kleines Fenster, das nie
        # durchlaessig gemacht wird.
        from scbp import overlay as ov31
        pruefe(hasattr(ov31, 'SCHLOSS_RUECKRUF'),
               'overlay kennt den Rueckruf fuers Schloss')
        # Der Rueckruf MUSS beim Umschalten kommen — sonst bliebe das Schloss
        # stehen, obwohl niemand mehr durchklickt (oder umgekehrt).
        gerufen31 = []
        alt31 = ov31.SCHLOSS_RUECKRUF[0]
        ov31.SCHLOSS_RUECKRUF[0] = lambda an: gerufen31.append(an)
        try:
            ov31.durchklickbar_setzen(None, False)
        except Exception:
            pass
        ov31.SCHLOSS_RUECKRUF[0] = alt31
        pruefe(len(gerufen31) == 1,
               'jedes Umschalten meldet sich beim Schloss (%d Rufe)' % len(gerufen31))
        # ⚠ Scheitert das Durchreichen, darf KEIN Schloss stehen — es waere ein
        # Schloss an einer Tuer, die offen ist.
        pruefe(gerufen31 == [False],
               'ohne wirksames Durchreichen kommt auch kein Schloss')
        # Ein Rueckruf, der wirft, darf das Schalten nicht kippen.
        ov31.SCHLOSS_RUECKRUF[0] = lambda an: 1 / 0
        try:
            ov31.durchklickbar_setzen(None, False)
            heil31 = True
        except ZeroDivisionError:
            heil31 = False
        ov31.SCHLOSS_RUECKRUF[0] = alt31
        pruefe(heil31, 'ein Fehler im Schloss reisst das Umschalten nicht mit')
        # Die Symbole muessen da sein — sonst ist das Schloss unsichtbar, und
        # genau das ist heute schon einmal passiert (das X im Herkunftskasten).
        for name31 in ('schloss_zu', 'schloss_auf'):
            pfad31 = os.path.join(WURZEL, 'assets', 'symbole', '18',
                                  name31 + '-gruen.png')
            pruefe(os.path.isfile(pfad31), 'Symbol %s liegt in 18 px vor' % name31)
        from scbp import sprache as sp31
        for schl31 in ('hinweis_schloss', 'ov_schloss_offen'):
            pruefe(bool(sp31.t(schl31)) and schl31 not in sp31.t(schl31),
                   'Text %s ist gesetzt, nicht der Schluesselname' % schl31)

        print()
        print('32. Die Log-Erkennung kennt UNSERE eigenen Zusaetze')
        # ⚠ Der gefaehrlichste Fehler dieser Nacht, gefunden am 28.08.2026 beim
        # Nachgehen einer Frage von Morkhan.
        #
        # Seit rc76 schreibt das Werkzeug die Angaben selbst an die
        # Gegenstandsnamen (`scbp/angaben.py`). Das Spiel schreibt den Namen
        # anschliessend **mitsamt Zusatz** in die Game.log:
        #
        #     Bauplan erhalten: Spectre (Sth/1/A)
        #
        # `SUFFIX_RE` kannte aber nur `Civ|Mil|Ind|Sth|Cmp` mit Grad `A-D` —
        # also genau die Form, die der SC Deutsch Launcher erzeugte. Alles, was
        # wir zusaetzlich schreiben, blieb am Namen kleben: Der Bauplan landet
        # unter falschem Namen im Bestand und wird **nie abgehakt**.
        #
        # Betroffen waeren 344 Waffen und 62 Raketen gewesen — und niemand
        # haette es gemerkt, weil das Werkzeug ja etwas anzeigt.
        from scbp.logquelle import teile_namen as tn32
        faelle32 = [
            ('Spectre (Sth/1/A)',            'Spectre'),
            ('7CA \'Nargun\' (Civ/3/A)',      "7CA 'Nargun'"),
            ('Omnisky III Cannon (Las/2/A)', 'Omnisky III Cannon'),
            ('Inspire Advanced (Ind/2/C)',   'Inspire Advanced'),
            ('P4-AR Rifle (Bal)',            'P4-AR Rifle'),
            ('Arrowhead Sniper Rifle (Las)', 'Arrowhead Sniper Rifle'),
            ("'Arrow' I Missile (IR1)",      "'Arrow' I Missile"),
            ('Argos IX Torpedo (CS9)',       'Argos IX Torpedo'),
            ('Pioneer I-G Missile (EM1)',    'Pioneer I-G Missile'),
            ('Glacis (Ind/4/\u2013)',          'Glacis'),
            ('V60-26 (Mil/\u2013/B)',          'V60-26'),
        ]
        for roh32, erwartet32 in faelle32:
            pruefe(tn32(roh32)[0] == erwartet32,
                   'abgeschnitten: %s' % roh32)
        # ⚠ Und die Gegenrichtung: Echte Namensklammern duerfen NICHT fallen.
        # Sonst hiesse „Singe Cannon (S2)" plötzlich nur noch „Singe Cannon",
        # und zwei verschiedene Waffen waeren derselbe Eintrag.
        for roh32 in ('Singe Cannon (S2)', 'Irgendwas (30 cap)',
                      'Ding (Alpha/1/A)', 'Sache (Mil/1/Z)'):
            pruefe(tn32(roh32)[0] == roh32,
                   'unangetastet: %s' % roh32)
        # Die Kuerzel-Liste MUSS zu angaben.py passen — sonst reisst genau
        # diese Luecke beim naechsten neuen Kuerzel wieder auf.
        from scbp import angaben as an32, logquelle as lq32
        for _teile32, kurz32 in an32.KLASSEN:
            pruefe(kurz32.lower() in lq32._KUERZEL.lower(),
                   'logquelle kennt das Kuerzel %s aus angaben.py' % kurz32)

        print()
        print('33. Bestand und Liste finden zueinander, egal woher der Name kam')
        # ⚠ Der Fehler, der Morkhans leere Kaestchen erklaert (28.08.2026).
        #
        # `pfade.namensform()` nennt sich selbst „die EINZIGE Stelle" fuer
        # Vergleichsschluessel — schnitt den Klassen-Zusatz aber nicht ab. Das
        # tat nur `logquelle.teile_namen()`. Also:
        #
        #     aus der Game.log:        'xl-1'            ✅ geschnitten
        #     aus der Launcher-Datei:  'xl-1 (mil/2/a)'  ❌ ungeschnitten
        #     aus einem Import:        'xl-1 (mil/2/a)'  ❌ ungeschnitten
        #
        # Zwei Schluessel, die nie zueinander finden: Der Bauplan galt als
        # fehlend, obwohl er im Bestand stand. Betroffen war jeder, der seinen
        # Stand aus dem SC Deutsch Launcher oder einer Sicherung mitbrachte —
        # also genau die Leute, die schon laenger spielen.
        from scbp.pfade import namensform as nfm33
        gleich33 = [
            ('XL-1 (Mil/2/A)',            'XL-1'),
            ('7CA \'Nargun\' (Civ/3/A)',   "7CA 'Nargun'"),
            ('7MA "Lorica" (Civ/3/B)',    "7MA 'Lorica'"),
            ('P4-AR Rifle (Bal)',         'P4-AR Rifle'),
            ("'Arrow' I Missile (IR1)",   "'Arrow' I Missile"),
            ('Argos IX Torpedo (CS9)',    'Argos IX Torpedo'),
            ('Glacis (Ind/4/\u2013)',       'Glacis'),
            ('V60-26 (Mil/\u2013/B)',       'V60-26'),
        ]
        for mit33, ohne33 in gleich33:
            pruefe(nfm33(mit33) == nfm33(ohne33),
                   'mit und ohne Kuerzel derselbe Schluessel: %s' % mit33)
        # ⚠ Gegenrichtung: Echte Namensklammern MUESSEN bleiben, sonst waeren
        # zwei verschiedene Waffen plötzlich derselbe Eintrag.
        for roh33 in ('Singe Cannon (S2)', 'Ding (Alpha/1/A)'):
            pruefe(nfm33(roh33) == roh33.lower(),
                   'unangetastet: %s' % roh33)
        # ⚠ Die Mengenangabe ist der Sonderfall: Das WORT faellt weg, die ZAHL
        # bleibt. Sonst zaehlt derselbe Bauplan doppelt, sobald das Spiel auf
        # Deutsch laeuft (gemessen 29.08.2026: 405 angezeigt, 403 echt).
        pruefe(nfm33('Ravager-212 Magazine (16 cap)')
               == nfm33('Ravager-212 Magazine (16 Schuss)'),
               'deutsch und englisch ergeben denselben Schluessel')
        pruefe(nfm33('Irgendwas (30 cap)') == 'irgendwas (30)',
               'die Zahl bleibt stehen, nur das Wort faellt weg')
        pruefe(nfm33('Magazin (40 cap)') != nfm33('Magazin (60 cap)'),
               'verschiedene Kapazitaeten bleiben verschiedene Bauplaene')
        pruefe(nfm33('Singe Cannon (S1)') != nfm33('Singe Cannon (S2)'),
               'Klammern ohne fuehrende Ziffer bleiben unangetastet')
        # ⚠ Und der wichtigste Teil: Ein **schon gespeicherter** Bestand muss
        # mitziehen. `namensform()` zu reparieren hilft nur neuen Eintraegen —
        # Morkhans 320 Bauplaene lagen mit den alten Schluesseln auf der Platte.
        import json as js33, tempfile as tf33, shutil as sh33
        heim33 = tf33.mkdtemp(prefix='bestand33-')
        alt_heim33 = os.environ.get('SC_BP_HOME')
        os.environ['SC_BP_HOME'] = heim33
        try:
            import importlib as im33
            from scbp import pfade as pf33
            im33.reload(pf33)
            from scbp import bestand as be33
            im33.reload(be33)
            with open(be33.pfad(), 'w', encoding='utf-8') as f33:
                js33.dump({'version': 1, 'stand': '2026-08-01 12:00:00',
                           'bauplaene': {
                               'xl-1 (mil/2/a)': {'name': 'XL-1 (Mil/2/A)',
                                                  'quelle': 'launcher',
                                                  'zeit': '2026-08-01 10:00:00'},
                               'guardian (ind/1/b)': {'name': 'Guardian (Ind/1/B)',
                                                      'quelle': 'launcher',
                                                      'zeit': '2026-08-05 09:00:00'},
                               'guardian': {'name': 'Guardian', 'quelle': 'log',
                                            'zeit': '2026-08-02 08:00:00'},
                               'ravager-212 magazine (16 cap)': {
                                   'name': 'Ravager-212 Magazine (16 cap)',
                                   'quelle': 'launcher', 'zeit': '2026-08-03 07:00:00'},
                               'ravager-212 magazine (16 schuss)': {
                                   'name': 'Ravager-212 Magazine (16 Schuss)',
                                   'quelle': 'nachlese', 'zeit': '2026-08-04 07:00:00'},
                           }}, f33, ensure_ascii=False)
            d33 = be33.laden()
            pruefe('xl-1' in d33['bauplaene'],
                   'ein gespeicherter Schluessel mit Kuerzel zieht um')
            pruefe('xl-1 (mil/2/a)' not in d33['bauplaene'],
                   'und der alte bleibt nicht daneben stehen')
            pruefe(len([k for k in d33['bauplaene'] if k.startswith('guardian')]) == 1,
                   'eine Dublette wird zu einem Eintrag zusammengefuehrt')
            pruefe(d33['bauplaene']['guardian'].get('zeit') == '2026-08-02 08:00:00',
                   'dabei gewinnt der aeltere Fund, nicht der zuletzt gelesene')
            pruefe(d33.get('version') == be33.DATEI_VERSION,
               'die Datei-Version wird hochgesetzt')
            # ⚠ Nur EINMAL umziehen — sonst schreibt jeder Start die Datei neu.
            auf_platte33 = js33.load(open(be33.pfad(), encoding='utf-8'))
            pruefe(auf_platte33.get('version') == be33.DATEI_VERSION,
                   'der Umzug wird auf die Platte geschrieben, nicht nur gedacht')
            # ⚠ Und der Umzug muss die Sprach-Dublette einsammeln — genau die,
            # die am 29.08.2026 in einem echten Bestand lag. Nur `namensform()` zu
            # reparieren haette den gespeicherten Bestand nicht angefasst.
            pruefe(len([k for k in d33['bauplaene']
                        if k.startswith('ravager-212')]) == 1,
                   'die deutsche und die englische Fassung werden zusammengefuehrt')
        finally:
            if alt_heim33 is None:
                os.environ.pop('SC_BP_HOME', None)
            else:
                os.environ['SC_BP_HOME'] = alt_heim33
            sh33.rmtree(heim33, ignore_errors=True)
            im33.reload(pf33)
            im33.reload(be33)

        # Und der Weg, um den es eigentlich geht: Ein Bestand aus der
        # Launcher-Datei muss die Liste abhaken koennen.
        from scbp import katalog as kat33
        habe33 = {nfm33('XL-1 (Mil/2/A)'), nfm33('Siren (Mil/1/B)')}
        pruefe(kat33._norm('XL-1') in habe33,
               'ein Launcher-Bestand hakt die Bauplan-Liste ab')
        pruefe(kat33._norm('Siren') in habe33,
               'und zwar fuer jeden Namen, nicht nur zufaellig einen')

        print()
        print('34. Fehlerbericht absenden — ein Knopf statt einer Erklaerstunde')
        # ⚠ Gemeldet am 28.08.2026: „ich will nicht jedem eine Stunde erklaeren,
        # wie ich zu dem Bericht komme, das ist nervenaufreibend." Und sein
        # Bruder, um den es ging: „weil ich kein nerd bin … ich installiere und
        # es funktioniert, wenn nicht, unbrauchbar."
        #
        # Kopieren und in Discord einfuegen scheitert dreifach: Der Bericht
        # steckt unter „Fortgeschritten", er ist zu lang fuer eine Nachricht,
        # und man muss wissen, wohin damit.
        from scbp import berichtziel as bz34, bericht as be34
        pruefe(bz34.ziel() == '',
               'im Repo steht KEINE Adresse — sie ist ein Geheimnis')
        pruefe(not bz34.moeglich(),
               'ohne Adresse meldet moeglich() sauber False')
        # ⚠ Der Knopf wird trotzdem GEZEIGT — er sagt beim Druecken, was fehlt.
        # Ihn auszublenden traf nur den Quellcode, also den Entwickler selbst:
        # „nicht mal ICH finde den" (28.08.2026). Ein fehlender Knopf sieht aus
        # wie ein Fehler.
        quelle34 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                        encoding='utf-8').read()
        stelle34 = quelle34[quelle34.index("s_di_absenden"):][:200]
        pruefe('if ' not in stelle34.split(chr(10))[0],
               'der Knopf haengt an keiner Bedingung')
        ok34, grund34 = be34.absenden('Probe', '3.0.0-test')
        pruefe(ok34 is False, 'ohne Ziel wird nichts gesendet')
        pruefe('http' not in grund34.lower(),
               'und die Meldung verraet die Adresse nicht')
        # ⚠ Der Bau MUSS die Datei ersetzen — sonst hat niemand den Knopf.
        yml34 = open(os.path.join(WURZEL, '.github', 'workflows',
                                  'release.yml'), encoding='utf-8').read()
        pruefe(yml34.count('scbp/berichtziel.py') >= 2,
               'Windows UND Linux setzen das Ziel beim Bau ein')
        pruefe('BERICHT_WEBHOOK' in yml34,
               'und zwar aus dem Secret, nicht aus dem Quelltext')
        # Die Adresse darf nirgends im Repo stehen.
        for _wo34, _unter34, _dateien34 in os.walk(os.path.join(WURZEL, 'scbp')):
            for _d34 in _dateien34:
                if not _d34.endswith('.py'):
                    continue
                _inh34 = open(os.path.join(_wo34, _d34),
                              encoding='utf-8', errors='ignore').read()
                pruefe('discord.com/api/webhooks' not in _inh34,
                       'keine Webhook-Adresse in scbp/%s' % _d34)

        print()
        print('35. Ein Textfeld rollt sich selbst, nicht die Seite dahinter')
        # ⚠ Von zwei Leuten unabhaengig gemeldet (28.08.2026): Im Bericht auf
        # der Diagnose-Seite liess sich erst rollen, NACHDEM man die ganze
        # Seite nach unten geschoben hatte. Das Rad ging an die Rollflaeche
        # dahinter, weil ein `tk.Text` keine registrierte Flaeche ist.
        from scbp import hauptfenster as hf35
        import tkinter as tk35
        w35 = tk35.Tk()
        # ⚠ Zeigen, sonst rechnet Tk das Layout nicht — `yview()` liefert dann
        # (0.0, 0.0), und jedes Feld saehe nach Ueberlauf aus. Weit ausserhalb
        # des Bildschirms und durchsichtig, damit niemand es sieht (siehe 23).
        w35.geometry('300x200+-4000+-4000')
        w35.attributes('-alpha', 0.0)
        w35.deiconify()
        try:
            rahmen35 = tk35.Frame(w35)
            rahmen35.pack(fill='both', expand=True)
            feld35 = tk35.Text(rahmen35, height=3)
            feld35.pack()
            # Kurzer Inhalt: passt hinein, also soll die SEITE rollen.
            feld35.insert('1.0', 'kurz')
            # ⚠ `update()`, nicht nur `update_idletasks()`. Unter Windows
            # rechnet Tk das Layout eines Fensters ausserhalb des Bildschirms
            # sonst nicht zu Ende: `yview()` gibt dann (0.0, 0.0), das sieht
            # wie Ueberlauf aus, und die Pruefung schlug im Bau fehl, obwohl
            # sie hier gruen war (28.08.2026).
            w35.update()
            oben35, unten35 = feld35.yview()
            if (unten35 - oben35) <= 0.0:
                # Tk hat trotzdem nicht gerechnet — dann ist hier nichts zu
                # pruefen. Lieber offen ueberspringen als falschen Alarm geben.
                print('  [--]   Tk rechnet dieses Fenster nicht durch — '
                      'Rollpruefung uebersprungen')
            else:
                pruefe(hf35._eigenes_rollen(feld35, rahmen35) is None,
                       'ein Feld ohne Ueberlauf gibt das Rad an die Seite weiter')
            # Langer Inhalt: laeuft ueber, also gehoert ihm das Rad.
            feld35.insert('end', '\n'.join('Zeile %d' % i for i in range(60)))
            w35.update()
            pruefe(hf35._eigenes_rollen(feld35, rahmen35) is feld35,
                   'ein ueberlaufendes Feld rollt sich selbst')
            # Und Widgets ohne Textfeld dazwischen aendern nichts.
            marke35 = tk35.Label(rahmen35, text='x')
            pruefe(hf35._eigenes_rollen(marke35, rahmen35) is None,
                   'eine Beschriftung faengt das Rad nicht ab')
        finally:
            w35.destroy()

        print()
        print('36. Der Reiter „Fehler melden“ faellt auf, ohne zu luegen')
        # ⚠ Zwei Stufen, damit Rot etwas bedeutet (Entscheidung 28.08.2026):
        #   * Das Wort ist IMMER rot — wer ein Problem hat, soll den Reiter
        #     finden, ohne ein Menue zu durchsuchen.
        #   * Das Symbol wird NUR rot, wenn wirklich Fehler mitgeschrieben
        #     wurden. Sonst stuende der Reiter dauerhaft auf Alarm, obwohl
        #     alles laeuft — und niemand naehme ihn noch ernst.
        quelle36 = open(os.path.join(WURZEL, 'scbp', 'hauptfenster.py'),
                        encoding='utf-8').read()
        stelle36 = quelle36[quelle36.index('def _reiter_faerben'):][:2200]
        pruefe("rot = (kennung == 'diagnose')" in stelle36,
               'der Reiter diagnose wird gesondert behandelt')
        pruefe('_fehler_liegen_an()' in stelle36,
               'das Symbol haengt an tatsaechlichen Fehlern, nicht am Reiter')
        pruefe('fg=ROT if rot' in stelle36,
               'das Wort ist unabhaengig davon rot')
        # Die Farbe muss es als Bild wirklich geben, sonst bleibt es unsichtbar
        # — genau so ist heute Nacht schon einmal ein X verschwunden.
        from scbp import zeichen as zi36
        pruefe(zi36.ROT == 'rot', 'zeichen kennt die Farbe rot')
        for n36 in ('diagnose',):
            pfad36 = os.path.join(WURZEL, 'assets', 'symbole', '22',
                                  n36 + '-rot.png')
            pruefe(os.path.isfile(pfad36),
                   'das Symbol %s liegt in Rot vor' % n36)
        # Und der Reiter heisst, was er tut.
        from scbp import sprache as sp36
        sp36.setzen('de')
        pruefe(sp36.t('hf_diagnose') == 'Fehler melden',
               'der Reiter heisst „Fehler melden“, nicht „Diagnose“')

        print()
        print('25. Eigener Startbefehl und die Starter-Zeile im Bericht')
        # ⚠ Wer ueber Lutris, Heroic oder Flatpak spielt, bekam GAR KEINEN
        # Startknopf. Der Ausweg (Einstellung `spielstarter`) existierte, stand
        # aber nur in der einstellungen.json — fuer jemanden, der spielen und
        # nicht schrauben will, heisst das: gibt es nicht.
        from scbp import pfade as pf25
        from scbp import bericht as be25

        # Ein Befehl mit Argumenten muss zerlegt werden, eine echte Datei NICHT.
        skript25 = os.path.join(basis, 'mein start skript.sh')
        open(skript25, 'w').close()
        pruefe(pf25._startbefehl(skript25) == [skript25],
               'eine vorhandene Datei mit Leerzeichen bleibt ganz')
        pruefe(pf25._startbefehl('lutris rungame/star-citizen')
               == ['lutris', 'rungame/star-citizen'],
               'ein Befehl mit Argumenten wird zerlegt')
        pruefe(pf25._startbefehl('flatpak run org.starcitizen-lug.Helper')
               == ['flatpak', 'run', 'org.starcitizen-lug.Helper'],
               'auch der Flatpak-Aufruf')
        # Unpaariges Anfuehrungszeichen darf nicht in eine Ausnahme laufen.
        pruefe(pf25._startbefehl('kaputt "offen') == ['kaputt "offen'],
               'ein unpaariges Anfuehrungszeichen wirft nicht')

        # Der eingetragene Befehl schlaegt die Suche.
        alt_einst25 = pf25.einstellung
        try:
            pf25.einstellung = lambda name: ('mein-eigener-start --jetzt'
                                             if name == 'spielstarter' else None)
            pruefe(pf25.spielstarter() == 'mein-eigener-start --jetzt',
                   'der eingetragene Startbefehl schlaegt die Suche')
        finally:
            pf25.einstellung = alt_einst25

        # ⚠ Und er muss im BERICHT stehen. Ohne diese Zeile ist "der Startknopf
        # tut nichts" nicht zu beantworten, ohne den Nutzer auszufragen — genau
        # das kostete am 27.08.2026 zwei Stunden.
        pruefe(hasattr(be25, '_spielstarter'),
               'der Bericht kennt eine Starter-Zeile')
        quelle25 = open(os.path.join(WURZEL, 'scbp', 'bericht.py'),
                        encoding='utf-8').read()
        pruefe("zeile(t('b_starter')" in quelle25,
               'und gibt sie auch aus')
        pruefe('kuerzen(' in quelle25.split('def _spielstarter')[1][:900],
               'gekuerzt — kein Benutzername im oeffentlichen Bericht')

        print()
        print('24. Der Waechter gibt den Port wirklich frei')
        # ⚠ **Der Kern des Selbst-Neustarts.** Steht im Lausch-Faden ein
        # `accept()`, weckt ein `close()` aus einem anderen Faden es NICHT: Der
        # Deskriptor bleibt gueltig, der Port belegt. Die frisch gestartete
        # Fassung kann sich dann nicht binden, haelt sich fuer die zweite
        # Instanz und beendet sich planmaessig — fuer den Nutzer sieht das aus
        # wie "geht aus und kommt nicht wieder".
        #
        # Drei Anlaeufe (rc67, rc68, rc70) haben das nicht geloest, weil geraten
        # statt gemessen wurde. Der Beweis kam aus einem Bericht vom
        # 27.08.2026: "neustart_tot, Rueckgabewert 0 — keine Ausgabe". Kein
        # Absturz, sondern ein geordneter Abgang.
        import socket as so24
        from scbp import overlay as ov24
        alt_port24 = ov24.WAECHTER_PORT
        ov24.WAECHTER_PORT = 47990
        try:
            gestartet24 = ov24.waechter_starten(lambda: None)
            pruefe(gestartet24, 'der Waechter laesst sich starten')
            time.sleep(0.2)
            ov24.waechter_stoppen()
            time.sleep(0.3)
            probe24 = so24.socket(so24.AF_INET, so24.SOCK_STREAM)
            probe24.setsockopt(so24.SOL_SOCKET, so24.SO_REUSEADDR, 1)
            frei24 = True
            grund24 = ''
            try:
                probe24.bind(('127.0.0.1', ov24.WAECHTER_PORT))
                probe24.listen(4)
            except OSError as ausnahme24:
                frei24 = False
                grund24 = str(ausnahme24)
            finally:
                probe24.close()
            pruefe(frei24,
                   'nach dem Stoppen laesst sich der Port neu binden%s'
                   % (' (%s)' % grund24 if grund24 else ''))

            # Und der Weg dorthin: ohne `shutdown()` bleibt der Faden haengen.
            quelle24 = open(os.path.join(WURZEL, 'scbp', 'overlay.py'),
                            encoding='utf-8').read()
            # ⚠ Bis zur naechsten Funktion schneiden, nicht auf Zeichenzahl —
            # ein langer Kommentar schob den Aufruf sonst aus dem Fenster.
            block24 = quelle24.split('def waechter_stoppen')[1].split('\ndef ')[0]
            pruefe('shutdown(' in block24,
                   'waechter_stoppen bricht das wartende accept() ab')
        finally:
            ov24.WAECHTER_PORT = alt_port24
            try:
                ov24.waechter_stoppen()
            except Exception:
                pass

        print()
        print('23. Bei der Mindestgroesse ist alles Wichtige sichtbar')
        # ⚠ Die Seite „Update & Ueber" ist die einzige, auf der ein nicht
        # gefundener Knopf richtig weh tut: Wer den Update-Knopf nicht sieht,
        # updatet nicht. Gemeldet am 27.08.2026: „das nervt user weil die den
        # button zum updaten nicht sofort finden."
        #
        # Geprueft wird bei der MINDESTGROESSE des Fensters (1100x760) — nicht
        # bei der Groesse, die der Entwickler zufaellig offen hat.
        import tkinter as tk23
        import tkinter.font as tkfont23
        from scbp import seiten as se23
        from scbp.hauptfenster import MIN_BREITE as MB23, MIN_HOEHE as MH23

        wurzel23 = tk23.Tk()
        _k23 = tkfont23.Font(root=wurzel23, family='Segoe UI', size=10)
        _t23 = tkfont23.Font(root=wurzel23, family='Segoe UI', size=12,
                             weight='bold')

        class _Traeger23:
            f_klein = _k23; f_titel = _t23; f_fett = _t23; f_gross = _t23
            f_mittel = _k23; f_normal = _k23; version = '3.0.0'
            def sagen(self, *a, **k): pass
            def oeffnen(self, *a, **k): pass
            def _einrichtung(self, *a, **k): pass

        try:
            rahmen23 = tk23.Frame(wurzel23)
            rahmen23.pack(fill='both', expand=True)
            # ⚠ **Feste Probehoehe, nicht `MIN_HOEHE`.** Geprueft wird, ob die
            # Update-Seite bei vernuenftiger Fenstergroesse vollstaendig
            # hineinpasst — das hat mit der **Mindest**hoehe nichts zu tun.
            # Seit die Leiste rollt, darf die bei 380 px liegen (30.08.2026);
            # die Pruefung schlug daraufhin fehl, obwohl am Fenster nichts
            # falsch war. 760 war die fruehere Mindesthoehe und bleibt das
            # sinnvolle Mass fuer „passt die Seite".
            PROBE_HOEHE23 = 760
            wurzel23.geometry('%dx%d' % (MB23, PROBE_HOEHE23))
            se23._ueber(_Traeger23(), rahmen23)
            wurzel23.update_idletasks()
            wurzel23.update()

            hoehe23 = wurzel23.winfo_height()
            abgeschnitten = []
            def _sammeln(w):
                for kind in w.winfo_children():
                    if (kind.winfo_class() == 'Canvas'
                            and kind.winfo_height() > 20
                            and kind.winfo_width() > 300):
                        y = kind.winfo_rooty() - wurzel23.winfo_rooty()
                        unten = y + kind.winfo_height()
                        # Die Rollflaeche selbst reicht bis zum Rand — die zaehlt
                        # nicht als abgeschnitten.
                        if unten > hoehe23 + 2 and kind.winfo_height() < hoehe23 - 50:
                            abgeschnitten.append((y, unten))
                    _sammeln(kind)
            _sammeln(rahmen23)
            # ⚠ **Nicht auf MIN_HOEHE bestehen.** Der Windows-Runner hat einen
            # virtuellen Bildschirm, auf dem Tk das Fenster nur 749 px hoch
            # bekommt — die Pruefung schlug dort fehl und brach den Bau von
            # rc68 ab, obwohl am Code nichts falsch war. Ist das Fenster
            # kleiner als die Mindestgroesse, wird die Kanten-Pruefung darunter
            # sogar STRENGER; verlangt wird deshalb nur ein echtes Fenster.
            pruefe(hoehe23 >= 600,
                   'die Probe hat ein echtes Fenster (%d px, Probehoehe %d)'
                   % (hoehe23, PROBE_HOEHE23))
            pruefe(not abgeschnitten,
                   'kein Knopf der Update-Seite faellt unter die Kante (%s)'
                   % (abgeschnitten or 'keiner'))
        finally:
            try:
                wurzel23.destroy()
            except Exception:
                pass

        print()
        print('22. Die Ablage schreibt bei jedem neuen Bauplan mit')
        # Bis rc65 wurden die drei Ausgabe-Dateien NUR auf Knopfdruck
        # geschrieben. Wer einmal geklickt hatte, hielt sie fuer aktuell — sie
        # standen aber fuer immer auf dem Stand jenes Klicks.
        import importlib as _imp22
        heim22 = os.path.join(basis, 'ablageprobe')
        os.makedirs(heim22)
        alt_heim22 = os.environ.get('SC_BP_HOME')
        os.environ['SC_BP_HOME'] = heim22
        try:
            from scbp import pfade as pf22
            _imp22.reload(pf22)
            from scbp import bestand as be22, export as ex22
            _imp22.reload(ex22)
            _imp22.reload(be22)

            ordner22 = ex22.ablage_ordner()
            # Altbestand aus der Zeit der datierten Namen — und eine fremde
            # Datei, die auf keinen Fall angefasst werden darf.
            for name in ('SC-Blueprints-Basetool-2026-08-01.json',
                         'scmdb-import-2026-07-30.json'):
                open(os.path.join(ordner22, name), 'w').close()
            open(os.path.join(ordner22, 'meine-notiz.json'), 'w').close()

            daten22 = be22.leer()
            be22.hinzufuegen(daten22, 'Testbauplan Alpha', 'log')
            be22.speichern(daten22)

            liegt = set(os.listdir(ordner22))
            pruefe({'SC-Blueprints-Basetool.json', 'scmdb-import.json',
                    'SC-BP-Watcher-Bestand.json'} <= liegt,
                   'speichern() schreibt alle drei Versionen mit')

            # ⚠ Ohne Datum im Namen — sonst entstuenden taeglich drei neue
            # Dateien, und niemand wuesste, welche die aktuelle ist.
            be22.hinzufuegen(daten22, 'Testbauplan Beta', 'log')
            be22.speichern(daten22)
            # ⚠ Nur zaehlen, was zur Ausgabe gehoert. Unter `SC_BP_HOME` legt
            # `pfade.app_datei()` ALLES flach in denselben Ordner — im
            # Normalbetrieb liegen die internen Dateien dagegen unter
            # `Intern/`. Ohne diese Ausnahme meldet die Pruefung jedes neue
            # interne Modul als „zweite Garnitur", obwohl es keine ist
            # (05.09.2026 mit `spielzeit.json` genau so passiert).
            NICHT_AUSGABE = ('spielzeit.json',)
            json_dateien = [d for d in os.listdir(ordner22)
                            if d.endswith('.json') and d not in NICHT_AUSGABE]
            pruefe(len(json_dateien) == 4,      # drei Versionen + fremde Datei
                   'zweimal speichern erzeugt keine zweite Garnitur (%d Dateien: %s)'
                   % (len(json_dateien), ', '.join(sorted(json_dateien))))

            pruefe(os.path.isfile(os.path.join(ordner22, 'meine-notiz.json')),
                   'eine fremde Datei im Ordner bleibt unangetastet')
            aelter = os.path.join(ordner22, ex22.ALTORDNER)
            pruefe(os.path.isdir(aelter) and len(os.listdir(aelter)) == 2,
                   'die alten datierten Versionen sind weggeraeumt, nicht geloescht')

            # Der Speichern-Dialog dagegen behaelt das Datum: Dort haelt jemand
            # bewusst einen Stand fest.
            pruefe('2026' in ex22.vorschlag('scmdb') or
                   time.strftime('%Y') in ex22.vorschlag('scmdb'),
                   'der Speichern-Dialog schlaegt weiterhin einen Namen mit Datum vor')

            # Und der Knopf je Zeile muss die Version durchreichen, statt
            # 'basetool' fest verdrahtet zu haben.
            quelle22 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                            encoding='utf-8').read()
            pruefe('def einzeln(art):' in quelle22,
                   'Einzeln speichern nimmt die Version entgegen')
            pruefe("export.schreiben(ziel, art=art)" in quelle22,
                   'und gibt sie auch weiter (nicht mehr fest basetool)')
        finally:
            if alt_heim22 is None:
                os.environ.pop('SC_BP_HOME', None)
            else:
                os.environ['SC_BP_HOME'] = alt_heim22
            from scbp import pfade as pf22b
            _imp22.reload(pf22b)

    finally:
        shutil.rmtree(basis, ignore_errors=True)

    print()
    print('37. Ein Auftrag mit mehreren Preisstufen verliert keine Bauplaene')
    # ⚠ Der Fehler vom 28.08.2026, gemeldet von Morkhan. `_missionen()` legte
    # die Auftraege unter ihrem Textschluessel ab — und Vertraege, die sich
    # einen teilen (123 von 353), ueberschrieben sich gegenseitig. Der zuletzt
    # gelesene gewann, 797 Bauplan-Eintraege sah nie jemand.
    #
    # Geprueft wird an einem winzigen Dump mit genau dieser Falle: zwei
    # Stufen, ein Schluessel, verschiedene Toepfe. Kommt nur eine Seite an,
    # ist der alte Fehler zurueck.
    from scbp import katalog as k37
    dump37 = {
        'blueprintPools': {
            'p-klein': {'blueprints': [{'name': 'Kleiner Plan'}]},
            'p-gross': {'blueprints': [{'name': 'Grosser Plan'}]},
        },
        'factionRewardsPools': [],
        'contracts': [
            {'titleLocKey': 'geteilt_title', 'descriptionLocKey': 'geteilt_desc',
             'rewardUEC': 50000, 'blueprintRewards': [{'blueprintPool': 'p-klein',
                                                       'chance': 1}],
             'minStanding': {'name': 'Neuling', 'minReputation': 800}},
            {'titleLocKey': 'geteilt_title', 'descriptionLocKey': 'geteilt_desc',
             'rewardUEC': 260000, 'blueprintRewards': [{'blueprintPool': 'p-gross',
                                                        'chance': 1}],
             'minStanding': {'name': 'Meister', 'minReputation': 38000}},
            # Dritte Stufe, die gar nichts ausschuettet — der Fall, wegen dem
            # jemand fuer eine Liste hinfliegt, die seine Stufe nie hergibt.
            {'titleLocKey': 'geteilt_title', 'descriptionLocKey': 'geteilt_desc',
             'rewardUEC': 20000, 'blueprintRewards': [],
             'minStanding': {'name': 'Anwaerter', 'minReputation': 1}},
        ],
    }
    m37 = k37._missionen(dump37)
    e37 = m37.get('geteilt_title') or {}
    pruefe(sorted(e37.get('bp') or []) == ['Grosser Plan', 'Kleiner Plan'],
           'beide Stufen kommen an, keine ueberschreibt die andere')
    pruefe(e37.get('leer') == 1 and e37.get('stufen') == 3,
           'die Stufe ohne Bauplaene wird vermerkt (1 von 3)')
    pruefe((e37.get('ab') or {}).get('Grosser Plan', {}).get('rep') == 38000,
           'der hoehere Plan traegt seinen eigenen Rang')
    pruefe((e37.get('ab') or {}).get('Kleiner Plan', {}).get('rep') == 800,
           'und der kleine seinen')
    # Gegenprobe: Brauchen alle Plaene denselben Rang, faellt die Angabe weg —
    # sonst stuende zwoelfmal dieselbe Zeile untereinander.
    gleich37 = json.loads(json.dumps(dump37))
    gleich37['contracts'][1]['minStanding'] = {'name': 'Neuling',
                                               'minReputation': 800}
    pruefe(not (k37._missionen(gleich37).get('geteilt_title') or {}).get('ab'),
           'bei gleichem Rang steht die Angabe NICHT an jedem Plan')
    # Und der Katalog auf der Platte muss den Umbau ueberhaupt mitbekommen.
    pruefe(k37.FORMAT >= 2,
           'der Katalog hat eine Aufbau-Nummer (sonst greift der Umbau nie)')

    # ------------------------------------------------------------------
    # Die Bau-Anleitungen selbst. Ein Tippfehler darin kostet keinen Fehler
    # im Programm, sondern **jeden Bau** — und zwar stumm: GitHub meldet nur
    # „workflow file issue", nichts davon steht im Fehlerbericht eines
    # Nutzers. Am 28.08.2026 lief das über eine Stunde so.
    print()
    print('38. Die Bau-Anleitungen sind gueltiges YAML')
    _wf = os.path.join(os.path.dirname(os.path.dirname(
        os.path.abspath(__file__))), '.github', 'workflows')
    _dateien = sorted(f for f in (os.listdir(_wf) if os.path.isdir(_wf) else [])
                      if f.endswith(('.yml', '.yaml')))
    pruefe(bool(_dateien), 'es gibt ueberhaupt Bau-Anleitungen zu pruefen')
    for _name in _dateien:
        with open(os.path.join(_wf, _name), encoding='utf-8') as _f:
            _funde = doppelte_schluessel(_f.read())
        pruefe(not _funde, '%s hat keinen doppelten Schluessel%s'
               % (_name, '' if not _funde else
                  ' (Zeile %d: „%s“ stand schon in Zeile %d)'
                  % (_funde[0][0], _funde[0][1], _funde[0][2])))

    # Gegenprobe — eine Prüfung, die nie anschlägt, prüft nichts. Das hier ist
    # der echte Fehler vom 28.08.2026, Zeichen für Zeichen.
    _kaputt = ('jobs:\n  bau:\n    steps:\n'
               '      - name: Berichtsziel einsetzen\n'
               '        shell: bash\n        shell: bash\n'
               '        run: echo hi\n')
    pruefe([f[1] for f in doppelte_schluessel(_kaputt)] == ['shell'],
           'und der Fehler von damals wird auch wirklich gefunden')
    # Und die Gegenrichtung: Was erlaubt ist, darf nicht gemeldet werden —
    # sonst schaltet man die Prüfung nach dem dritten Fehlalarm ab.
    _erlaubt = ('jobs:\n  bau:\n    steps:\n'
                '      - name: A\n        run: |\n'
                '          cat <<X\n          on: 1\n          on: 2\n          X\n'
                '      - name: B\n        run: echo b\n')
    pruefe(not doppelte_schluessel(_erlaubt),
           'gleiche Namen in getrennten Schritten sind KEIN Fehler')


    # ------------------------------------------------------------------
    # ⚠ Der häufigste Support-Fall: „ich sehe deine Angaben im Spiel nicht
    # mehr". Ein Übersetzungs-Update oder ein Spiel-Patch schreibt die
    # `global.ini` neu und wirft die Angaben dabei stillschweigend hinaus.
    # Am 28.08.2026 stand in Morkhans Bericht nur `inj_quelle=deutsch` — ob
    # etwas eingetragen war, musste erschlossen werden statt abgelesen.
    print()
    print('39. Der Bericht sagt, ob die Angaben im Spiel stehen')
    from scbp import bericht as ber39, injektion as inj39
    # ⚠ Eigener Ordner statt `basis`: Der ist an dieser Stelle bereits
    # aufgeräumt, und ein Schreibversuch darin bricht den ganzen Lauf ab.
    _ordner39 = tempfile.mkdtemp(prefix='sc-bp-inj39-')
    _ini39 = os.path.join(_ordner39, 'global39.ini')
    _echt39 = inj39.ini_datei
    try:
        # Datei da, Angaben vom Launcher hinausgeworfen — Morkhans Lage.
        with open(_ini39, 'w', encoding='utf-8') as f:
            f.write('mission_a_desc=Deliver cargo.\n')
        inj39.ini_datei = lambda: (_ini39, 'german_(germany)', 'deutsch')
        _l39 = ber39._injektionslage()
        pruefe('NICHT' in _l39 or 'NOT' in _l39,
               'ohne Angaben in der Datei sagt der Bericht das auch')

        # ⚠ MrKrakens Kennzeichnung allein ist KEINE Injektion. Er schreibt in
        # StarStrings dasselbe blanke `<EM4>[BP]</EM4>` an seine Titel (314 in
        # der Fassung vom 29.08.2026). Bis dahin meldete der Bericht deshalb
        # „steht drin", sobald jemand StarStrings frisch eingesetzt hatte.
        with open(_ini39, 'a', encoding='utf-8') as f:
            f.write('mission_b_title=Bounty <EM4>[BP]</EM4>\n')
        _l39ss = ber39._injektionslage()
        pruefe('NICHT' in _l39ss or 'NOT' in _l39ss,
               'MrKrakens blankes [BP] allein gilt NICHT als eigene Injektion')

        # Dieselbe Datei, eigene Angaben drin — die Block-Überschrift ist die
        # Form, die jede echte Injektion hinterlässt.
        with open(_ini39, 'a', encoding='utf-8') as f:
            f.write('mission_c_desc=Deliver cargo.\\n\\n--------------------'
                    '\\nMÖGLICHE BAUPLÄNE FÜR DIESEN MISSIONSTYP\\n'
                    '    [x] Atzkav Sniper Rifle\n')
        _l39b = ber39._injektionslage()
        pruefe('NICHT' not in _l39b and 'NOT' not in _l39b,
               'und mit Angaben meldet er sie als eingetragen')

        # ⚠ Gar keine Datei ist NICHT dasselbe wie „nicht eingetragen": Unter
        # Linux ohne Übersetzung ist das der Normalzustand, und eine Warnung
        # davor wäre eine Warnung vor nichts.
        inj39.ini_datei = lambda: (None, 'english', None)
        _l39c = ber39._injektionslage()
        pruefe('NICHT' not in _l39c and 'NOT' not in _l39c,
               'ohne Textdatei warnt er NICHT vor dem Normalzustand')
    finally:
        inj39.ini_datei = _echt39
        shutil.rmtree(_ordner39, ignore_errors=True)


    print()
    print('40. Der Installer haelt das Programm auch UNTEN, nicht nur zu')
    # ⚠ Gemessen am 28.08.2026 (beim Update rc75 -> rc83). Im
    # Setup-Protokoll steht die ganze Kette:
    #
    #     05:43:47  Shutting down applications using our files. (forced)
    #     05:43:55  << Watcher laeuft wieder, Elternprozess explorer.exe >>
    #     05:44:17  DeleteFile: The existing file appears to be in use (5).
    #
    # `CloseApplications=force` hat sauber geschlossen. Acht Sekunden spaeter
    # hat der **Autostart** das Programm wieder hochgefahren, und das Kopieren
    # lief gegen Code 5. Bewiesen ueber den Elternprozess: `explorer.exe`
    # arbeitet die Run-Werte verzoegert nach seinem eigenen Start ab.
    #
    # `CloseApplications` kann das prinzipiell nicht loesen — es schliesst
    # einmal. Deshalb faehrt `PrepareToInstall` direkt vor dem Kopieren nach.
    # Ohne diese Pruefung faellt der Fix bei der naechsten Ueberarbeitung
    # unbemerkt heraus, und der Fehler kommt bei Nutzern wieder — dort, wo
    # ihn niemand messen kann.
    iss40 = open(os.path.join(WURZEL, 'packaging', 'installer.iss'),
                 encoding='utf-8').read()
    pruefe('[Code]' in iss40 and 'PrepareToInstall' in iss40,
           'PrepareToInstall faehrt vor dem Kopieren nach')
    pruefe('taskkill' in iss40,
           'und beendet dabei einen wieder hochgefahrenen Watcher')
    pruefe('FileExists' in iss40,
           'nur beim Update — Erstinstallationen warten nicht')
    # Die zwei Direktiven, an denen der Weg schon zweimal gescheitert ist.
    aktiv40 = [z.strip() for z in iss40.splitlines()
               if z.strip() and not z.strip().startswith(';')]
    pruefe(not any(z.startswith('AppMutex=') for z in aktiv40),
           'AppMutex steht NICHT drin (blockierte den Weg am 26.08.2026)')
    pruefe('RestartApplications=no' in aktiv40,
           'RestartApplications=no — der RM faehrt nichts von selbst hoch')
    # ⚠ Und die Erklaerung im Code muss dazu passen. Sie tat es bis zum
    # 28.08.2026 nicht und schickte die Fehlersuche in die falsche Richtung.
    ak40 = open(os.path.join(WURZEL, 'scbp', 'aktualisierung.py'),
                encoding='utf-8').read()
    kopf40 = ak40[ak40.index('Der Eigenbau ist deshalb weg'):][:3000]
    # ⚠ Auf Wortabwesenheit zu pruefen waere falsch: Der Kommentar ZITIERT die
    # beiden alten Falschaussagen, um sie zu widerlegen. Geprueft wird deshalb,
    # ob er den echten Stand nennt — daran haengt, ob der naechste Leser richtig
    # informiert wird.
    pruefe('RestartApplications=no' in kopf40,
           'der Code nennt den echten Stand: RestartApplications=no')
    pruefe('PrepareToInstall' in kopf40,
           'und verweist auf das Nachfassen im Installer')

    print()
    print('41. Ein Schalter, der aus sagt, macht auch aus')
    # ⚠ Gemessen am 28.08.2026 (gemessen): „Angaben am Gegenstand“ abgeschaltet,
    # Statuszeile meldete „aus“ — und die `global.ini` blieb unangetastet. 1.217
    # Angaben standen weiter drin, das Spiel zeigte sie unverändert.
    #
    # Schlimmer noch der Kasten darüber: „Änderungen wirken beim nächsten
    # Spielstart“ — wer danach neu startete und alles unverändert vorfand, hielt
    # das Werkzeug für kaputt. Gemeldet: „ein user erwartet das was er liest und
    # sieht, ist es aus angaben weg also muss das auch so sein.“
    #
    # Der Schalter stößt das Neuschreiben jetzt selbst an. Diese Prüfung hält das
    # fest — fällt es heraus, ist der Fehler zurück, und zwar unsichtbar.
    se41 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                encoding='utf-8').read()
    i41 = se41.index('def angaben_um():')
    rumpf41 = se41[i41:se41.index('return neu_wert', i41)]
    pruefe('_inj_erneuern' in rumpf41,
           'Umlegen schreibt die Textdatei neu')
    pruefe('lage_zeigen' in rumpf41,
           'und der Zustandskasten wird danach aufgefrischt')
    # ⚠ Zwei Riegel, sonst stößt ein Formatschalter eine Einfügung an, die
    # niemand wollte — der obere Schalter lässt Vorhandenes mit Absicht stehen.
    pruefe("inj_an" in rumpf41,
           'aber nur, wenn das Schreiben überhaupt eingeschaltet ist')
    pruefe("drin" in rumpf41,
           'und nur, wenn schon etwas in der Datei steht')

    # ⚠ Derselbe Anspruch für den Hauptschalter — der Autor fiel im eigenen Test
    # darauf herein und hat damit den Punkt bewiesen: „ich hab das fette gelesen
    # aber nicht das kleinere“. Der Hinweis stand im Kleingedruckten, und genau
    # das liest niemand. Aus heißt jetzt weg, an heißt da.
    i41b = se41.index('def inj_an_um():')
    rumpf41b = se41[i41b:se41.index('return neu_wert', i41b)]
    pruefe('_inj_entfernen' in rumpf41b,
           'Ausschalten nimmt vorhandene Angaben heraus')
    pruefe('_inj_erneuern' in rumpf41b,
           'und Einschalten trägt sie wieder ein')
    # ⚠ Der Hilfetext MUSS mitziehen, sonst behauptet er das Gegenteil des
    # Verhaltens — schlimmer als gar kein Hinweis.
    from scbp import sprache as sp41
    hilfe41 = sp41.TEXTE['s_sp_an_h']
    pruefe('entfernt vorhandene Angaben nicht' not in hilfe41[0],
           'der Hilfetext behauptet nicht mehr das Gegenteil (de)')
    pruefe('does not remove' not in hilfe41[1],
           'dasselbe auf Englisch')
    # ⚠ Und der Kasten muss den Rest zugeben, statt „nichts geschrieben“ zu sagen.
    pruefe('s_sp_aus_rest' in sp41.TEXTE and 's_sp_aus_rest_h' in sp41.TEXTE,
           'der Kasten kann sagen, dass noch Angaben im Spiel stehen')
    pruefe('s_sp_aus_rest' in se41,
           'und benutzt das auch')

    # ⚠ Der Autostart wird an ZWEI Stellen gesetzt: vom [Registry]-Abschnitt des
    # Installers (nur bei gewaehltem Haekchen) und vom Programm selbst
    # (`scbp/autostart.py`). `uninsdeletevalue` raeumt nur den ersten Fall weg.
    #
    # Gemessen am 28.08.2026 (gemessen): Nach dem Deinstallieren stand der Wert
    # weiter in der Registry und zeigte auf eine geloeschte Datei. Windows
    # versucht sie bei jeder Anmeldung zu starten und scheitert still.
    #
    # Derselbe Autostart hat morgens den Update-Fehler (Code 5) ausgeloest — er
    # war an beiden Enden nur halb geregelt.
    pruefe('CurUninstallStepChanged' in iss40 and 'RegDeleteValue' in iss40,
           'der Deinstaller raeumt den Autostart-Eintrag weg')
    # ⚠ Beide Seiten MUESSEN denselben Wertnamen meinen, sonst raeumt der
    # Deinstaller ins Leere und der echte Eintrag bleibt liegen.
    from scbp import autostart as as41
    pruefe("'" + as41.NAME + "'" in iss40,
           'und zwar genau den Namen, den das Programm schreibt (%s)' % as41.NAME)

    print()
    print('42. Ein eigener Fund ergaenzt einen Patch, er ersetzt ihn nicht')
    # ⚠ Gemessen am 28.08.2026 (gemessen): Im Filter stand „4.10.0 (3)", und
    # darunter drei Schiffswaffen. Mitgeliefert waren 21 Baupläne für dieselbe
    # Version — der ganze Patch war aus der Anzeige verschwunden.
    #
    # Ursache: `laden()` legte die eigene Historie per `update()` über die
    # mitgelieferte. Bei gleichem Versionsschlüssel gewann die eigene komplett.
    # Nur: Was `eintragen()` schreibt, ist immer bloß der **Zuwachs seit dem
    # letzten Lauf** — hier drei Waffen, die scmdb zwei Tage später nachreichte.
    # Als vollständige Patch-Liste gelesen ist das zwangsläufig falsch.
    #
    # Diese Prüfung hält beide Richtungen fest: mitgeliefert + eigen, und eigen
    # + eigen. Fällt eine heraus, frisst der nächste Nachzügler wieder den Patch.
    import tempfile as _tf42
    from scbp import patchhistorie as ph42
    _alt_home42 = os.environ.get('SC_BP_HOME')
    os.environ['SC_BP_HOME'] = _tf42.mkdtemp(prefix='sc-bp-historie-')
    try:
        _mit42 = ph42._lies(ph42.pfade.programm_datei(ph42.MITGELIEFERT))
        _v42 = sorted(_mit42, key=ph42.rang)[-1]
        _vorher42 = len(_mit42[_v42].get('neu') or [])
        pruefe(_vorher42 > 1,
               'die mitgelieferte Historie fuehrt mehrere Bauplaene (%d)'
               % _vorher42)

        # a) Der Fall vom 28.08.2026: zwei Nachzuegler in derselben Version.
        ph42.eintragen(_v42, ['Testwaffe A', 'Testwaffe B'], datum='2099-12-31')
        pruefe(len(ph42.laden()[_v42]['neu']) == _vorher42 + 2,
               'eigene Funde kommen dazu, statt den Patch zu ersetzen')
        pruefe(ph42.laden()[_v42]['datum'] == _mit42[_v42].get('datum'),
               'und das fruehere Datum bleibt stehen')

        # b) Und der zweite eigene Fund wirft den ersten nicht weg.
        ph42.eintragen(_v42, ['Testwaffe C'])
        pruefe(len(ph42.laden()[_v42]['neu']) == _vorher42 + 3,
               'ein zweiter eigener Fund loescht den ersten nicht')

        # c) Was schon dasteht, darf nicht doppelt gezaehlt werden.
        ph42.eintragen(_v42, [_mit42[_v42]['neu'][0], 'Testwaffe A'])
        pruefe(len(ph42.laden()[_v42]['neu']) == _vorher42 + 3,
               'bekannte Namen kommen nicht ein zweites Mal hinein')

        # d) ⚠ Und der Bericht muss die Zahlen zeigen. Ohne diese Zeile stand im
        #    Bericht nur der Katalogstand — der war in Ordnung, die Historie
        #    darunter nicht. Genau deshalb blieb der Fehler unsichtbar.
        from scbp import bericht as ber42
        pruefe('(%d)' % (_vorher42 + 3) in (ber42._patchhistorie() or ''),
               'der Bericht nennt die Anzahl je Patch')

        # e) ⚠ Zwei Spielversionen mit derselben Nummer duerfen im Bericht nicht
        #    zu zwei gleich aussehenden Eintraegen verkuerzt werden. Genau das
        #    stand am 01.09.2026 dort: "4.10.0 (24), 4.10.0 (34)" — beide Male
        #    dieselbe Beschriftung, und niemand konnte zuordnen, welche Zahl zu
        #    welchem Patch gehoert. Ausgerechnet in der Zeile, die es zum
        #    Zuordnen gibt (siehe d).
        _kurz42 = _v42.split('-')[0]
        _zwei42 = _kurz42 + '-live.99999999'
        ph42.eintragen(_zwei42, ['Testwaffe D'])
        _zeile42 = ber42._patchhistorie() or ''
        pruefe(_kurz42 + ' (' not in _zeile42,
               'gleiche Patch-Nummern werden nicht auf die Kurzform verkuerzt')
        pruefe(_v42 in _zeile42 and _zwei42 in _zeile42,
               'beide vollen Versionen stehen im Bericht')
    finally:
        if _alt_home42 is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _alt_home42

    print()
    print('43. Das Auswahlfeld verspricht nur, was die Liste zeigen kann')
    # ⚠ Gemessen am 28.08.2026 (gemessen), direkt nach dem Fix an der Historie:
    # Im Feld stand „4.10.0 (24)", darunter drei Zeilen. Die Zahl in Klammern
    # ist eine Zusage, wie viele Zeilen kommen — und sie kam aus einer anderen
    # Quelle als die Zeilen selbst: `patches()` las die Historie, der Filter
    # prueft den Stempel `seit` im Katalog.
    #
    # Zwei Quellen fuer dieselbe Frage gehen irgendwann auseinander. Das Feld
    # zaehlt jetzt den Katalog. Damit dort auch alles gestempelt ist, zieht das
    # Fenster den Stempel nach, BEVOR es den Katalog liest — vorher hing das
    # allein am Netz-Takt, der irgendwann nach dem Start in einem eigenen Faden
    # laeuft (gemessen: Fenster 10:44:02, Stempel 10:44:03 — eine Sekunde zu
    # spaet, und die Liste blieb bis zum naechsten Oeffnen falsch).
    import tempfile as _tf43
    from scbp import katalog as kat43, patchhistorie as ph43
    _alt_home43 = os.environ.get('SC_BP_HOME')
    os.environ['SC_BP_HOME'] = _tf43.mkdtemp(prefix='sc-bp-patchfeld-')
    try:
        # Die Historie kennt DREI Bauplaene, der Katalog fuehrt nur zwei davon.
        ph43._schreib(ph43.pfade.app_datei('patch-historie.json'),
                      {'4.10.0-live.7': {'datum': '2026-08-26',
                                         'neu': ['Erster Bauplan',
                                                 'Zweiter Bauplan',
                                                 'Nicht im Katalog']}})
        with open(kat43.pfade.app_datei('katalog-cache.json'), 'w',
                  encoding='utf-8') as f:
            json.dump({'version': '4.10.0-live.7', 'geholt': '',
                       'bauplaene': {'erster bauplan': {'n': 'Erster Bauplan'},
                                     'zweiter bauplan': {'n': 'Zweiter Bauplan'},
                                     'alter bauplan': {'n': 'Alter Bauplan'}},
                       'missionen': {}}, f)

        # a) Ungestempelt darf das Feld gar nichts versprechen.
        pruefe(kat43.patches(kat43.laden()) == [],
               'ohne Stempel bleibt das Feld leer, statt zu versprechen')

        # b) Nach dem Nachziehen: genau die zwei, die es im Katalog gibt.
        pruefe(kat43.stempel_nachziehen() == 2,
               'zwei Stempel werden nachgetragen')
        _p43 = kat43.patches(kat43.laden())
        pruefe(_p43 == [('4.10.0-live.7', '4.10.0', 2)],
               'das Feld zaehlt den Katalog (2), nicht die Historie (3)')

        # c) Und die Liste kommt auf dieselbe Zahl — das ist der ganze Punkt.
        _d43 = kat43.laden()
        pruefe(len(kat43.neue(_d43)) == _p43[0][2],
               'Feld und Liste kommen auf dieselbe Zahl')

        # d) ⚠ Und das Fenster muss stempeln, BEVOR es liest. Andersherum sieht
        #    es beim ersten Start nach einem Update den alten Stand.
        _q43 = open(os.path.join(WURZEL, 'scbp', 'bestandsfenster.py'),
                    encoding='utf-8').read()
        pruefe('katalog_modul.stempel_nachziehen()' in _q43
               and (_q43.index('katalog_modul.stempel_nachziehen()')
                    < _q43.index('self.katalog = katalog_modul.laden()')),
               'das Fenster stempelt, BEVOR es den Katalog liest')
    finally:
        if _alt_home43 is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _alt_home43

    print()
    print('44. Das Schloss laesst sich auch wieder ZUsperren')
    # ⚠ Haldjas (pr0) am 28.08.2026: „man kann das durckclicken entfernen, aber
    # eventuell kann der button zum locken stehen bleiben? sonst muss man ja
    # erst wieder in die einstellungen."
    #
    # Er hat den blinden Fleck getroffen: Gebaut war nur der Rueckweg. Das
    # schwebende Schloss erscheint, solange durchgereicht wird — schaltet man ab,
    # ist es weg, und der Hinweg fuehrte allein ueber Einstellungen -> Overlay.
    # Ein Weg hin und her gehoert an dieselbe Stelle.
    import tempfile as _tf44
    from scbp import pfade as pf44, sprache as sp44
    import sc_bp_watcher as w44
    _q44 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'), encoding='utf-8').read()

    pruefe("zeichen.knopf(bar, 'schloss_auf'" in _q44,
           'in der Overlay-Leiste steht ein offenes Schloss')
    # ⚠ Nur, wo das System es kann. Unter nativem Wayland waere ein Knopf ohne
    #   Wirkung schlimmer als keiner — dieselbe Regel wie beim Schalter.
    pruefe(_q44.index('overlay.durchklickbar_moeglich()')
           < _q44.index("zeichen.knopf(bar, 'schloss_auf'"),
           'und zwar nur, wenn das System Klicks durchreichen kann')
    pruefe(os.path.exists(os.path.join(WURZEL, 'assets', 'symbole',
                                       'schloss_auf.png'))
           or "'schloss_auf'" in open(os.path.join(WURZEL, 'tools',
                                                   'symbole_bauen.py'),
                                      encoding='utf-8').read(),
           'das Symbol dafuer gibt es')
    for _sl44 in ('hinweis_schloss_zu', 'ov_schloss_zu'):
        _e44 = sp44.TEXTE.get(_sl44)
        pruefe(bool(_e44) and len(_e44) == 2 and all(_e44),
               'Text %s steht in beiden Sprachen' % _sl44)

    # ⚠ Der teurere Teil: Klappt das Durchreichen nicht, MUSS die Einstellung
    #   zurueckgenommen werden. Ein gespeichertes „an", waehrend in Wahrheit
    #   nichts durchgereicht wird, ist das schlechteste von beidem — der Nutzer
    #   sieht einen Zustand, den es nicht gibt. Der Schalter in den
    #   Einstellungen macht es genauso (`seiten._durchklick_um`).
    _alt_home44 = os.environ.get('SC_BP_HOME')
    os.environ['SC_BP_HOME'] = _tf44.mkdtemp(prefix='sc-bp-schloss-')
    try:
        class _Ohne44:
            """Ein Overlay ohne Tk — die Methode braucht nur diese zwei Dinge."""
            gemeldet = None
            klappt = False

            def _status_setzen(self, satz):
                self.gemeldet = satz

            def durchklick_anwenden(self):
                return self.klappt

        _o44 = _Ohne44()
        w44.Overlay._schloss_zusperren(_o44)
        pruefe(pf44.einstellung_wahrheit('durchklickbar', False) is False,
               'geht das Durchreichen nicht, wird die Einstellung zurueckgenommen')
        pruefe(_o44.gemeldet is None,
               'und es wird kein Erfolg gemeldet, den es nicht gab')

        _o44.klappt = True
        w44.Overlay._schloss_zusperren(_o44)
        pruefe(pf44.einstellung_wahrheit('durchklickbar', False) is True,
               'klappt es, bleibt das Durchreichen an')
        pruefe(_o44.gemeldet is not None,
               'und der Nutzer erfaehrt, wie er zurueckkommt')

        # ⚠ Zweiter Wunsch von Gemeldet am selben Tag: „am besten waere das
        #   gleiche schloss gruen zu faerben was eh in der leiste ist, und es
        #   damit auch wieder zu entsperren."
        #
        #   Ein eigenes Fenster MUSS es bleiben — durchgereicht wird immer fuer
        #   das ganze Fenster, ein Knopf in der Leiste waere in dem Moment
        #   genauso tot wie der Rest. Also liegt es passgenau darueber. Diese
        #   Pruefung haelt fest, dass die Lage vom Leisten-Knopf kommt und nicht
        #   wieder in die Ecke rutscht.
        _ank44 = _q44.index('def _schloss_anwenden')
        _bis44 = _q44.index('def _leistenschloss')
        _rumpf44 = _q44[_ank44:_bis44]
        pruefe('knopf.winfo_rootx()' in _rumpf44,
               'das schwebende Schloss nimmt die Lage vom Leisten-Knopf')
        pruefe('winfo_ismapped()' in _rumpf44,
               'und faellt auf die Ecke zurueck, wenn der Knopf nicht da ist')

        # ⚠ Der Fall, an dem rc92 noch scheiterte — gemeldet von Haldjas (pr0)
        #   am 28.08.2026, belegt durch seinen Bericht: `overlay_modus=popup`.
        #
        #   Im Pop-up-Betrieb ruft `verhalten_anwenden()` `withdraw()`, BEVOR je
        #   gezeichnet wurde. Der Knopf ist dann dauerhaft nicht gemappt, das
        #   Nachfassen laeuft zehnmal leer — und danach rechnete die Stelle aus
        #   der Lage eines UNSICHTBAREN Fensters. Gemessen:
        #
        #       versteckt (war sichtbar):  ismapped=0  w=56  rootx=1161
        #       nie gemalt, dann versteckt: ismapped=0  w=1   rootx=0
        #
        #   `_anfasser_zeigen()` loest denselben Fall seit jeher richtig: aus
        #   `self._letzte_lage`. Das Schloss geht jetzt denselben Weg.
        # ⚠ Ein Toplevel erbt die Deckkraft des Hauptfensters NICHT. Ohne diese
        #   Zeile lag ein voll deckendes Schloss ueber einem zu 93 % durch-
        #   scheinenden Knopf — zwei Symbole mit verschiedener Saettigung.
        # ⚠ Der Feinausgleich gilt NUR im sichtbaren Fall. Der Aufblend-Betrieb
        #   rechnet aus der Streifen-Position — wer ihn dort mit einrechnet,
        #   bricht das eine, waehrend er das andere geradezieht.
        pruefe('SCHLOSS_FEIN_X' in _rumpf44,
               'der Feinausgleich steht als benannte Konstante im sichtbaren Fall')
        _versteckt44 = _rumpf44[_rumpf44.index('ANFASSER_BREITE + 4'):]
        pruefe('SCHLOSS_FEIN_X' not in _versteckt44,
               'und fasst den Aufblend-Betrieb nicht an')
        pruefe('DECKKRAFT' in _rumpf44,
               'das Schloss traegt dieselbe Deckkraft wie das Overlay')
        pruefe('self._letzte_lage' in _rumpf44,
               'im Pop-up-Betrieb gilt die gemerkte Lage, nicht das '
               'versteckte Fenster')
        # ⚠ Und dort gehoert es an den Anfasser-Streifen. An der rechten Ecke
        #   der gemerkten Lage saesse es einsam, weit weg von der einzigen Marke,
        #   die im Aufblend-Betrieb ueberhaupt zu sehen ist.
        pruefe('ANFASSER_BREITE' in _rumpf44,
               'und haengt am Anfasser-Streifen, nicht in der leeren Ecke')
        # ⚠ Und OHNE die Hoehe auf null zu klemmen. `_current_geom()` bewahrt
        #   negatives Y ausdruecklich („so bleibt negatives Y als absolute
        #   Position erhalten") — auf mehreren Bildschirmen ist das eine
        #   gueltige Angabe, keine kaputte. `max(0, oben)` warf Streifen und
        #   Schloss auf den Hauptmonitor.
        for _wo, _name in ((_rumpf44, 'das Schloss'),
                           (_q44[_q44.index('def _anfasser_zeigen'):]
                            [:_q44[_q44.index('def _anfasser_zeigen'):]
.index('    def ', 10)], 'der Anfasser')):
            pruefe('max(0, oben)' not in _wo,
                   '%s klemmt die Hoehe nicht auf null (zweiter Monitor)'
                   % _name)
        # Und wenn sich der Bezugspunkt aendert, muss es mitkommen.
        for _wo44, _was44 in (('def _popup_zeigen', 'beim Aufblenden'),
                              ('def _popup_verstecken', 'beim Zublenden')):
            _teil44 = _q44[_q44.index(_wo44):]
            _teil44 = _teil44[:_teil44.index('    def ', 10)]
            pruefe('_schloss_nachziehen()' in _teil44,
                   'das Schloss zieht %s mit' % _was44)
        # ⚠⚠ **Die Titelleiste haengt an der Seite, die zur Ecke passt.**
        #   Gemeldet von Haldjas (pr0) am 02.09.2026: Bei einer unteren Ecke
        #   sass die Leiste eine Fensterhoehe ueber dem Bildschirmrand.
        #
        #   Vier Anlaeufe (v3.9.2-rc3 bis rc6) sind daran gescheitert, dass ein
        #   EINGEKLAPPTES Fenster beim Neupacken „von 22 auf 120 px" wuchs und
        #   „86 px unter den Bildschirmrand" ragte. Beide Zahlen waren die
        #   Loesung, nur hat sie niemand gelesen: 120 ist die Mindesthoehe aus
        #   `_mindestgroesse_setzen()`, 86 ihr Ueberstand in einer unteren
        #   Ecke. Es lag nie am Neupacken — `minsize` blieb beim Einklappen
        #   stehen. Seit rc9 zieht sie mit, seither funktioniert der Umbau
        #   (gemessen in `tools/entwurf_leiste_pruefen.py`).
        #
        #   Diese Pruefung bewacht die drei Stellen, an denen es kippen kann.
        #   Die Leiste ist im eingeklappten Zustand der EINZIGE Bedienweg —
        #   ist sie ausserhalb des Bildes, kommt niemand mehr an das Werkzeug.
        if 'def _leiste_ausrichten' in _q44:
            _teil45 = _q44[_q44.index('def _leiste_ausrichten'):]
            _teil45 = _teil45[:_teil45.index('    def ', 10)]
            pruefe("'bottom'" in _teil45,
                   'die Titelleiste kann nach unten wechseln')
            pruefe('startswith' in _teil45 and 'unten' in _teil45,
                   'und zwar abhaengig von der gewaehlten Ecke')
            pruefe('_leiste_seite' in _teil45,
                   'ein Wechsel wird gemerkt, statt bei jedem Klappen '
                   'umzupacken')
        # ⚠ Die Reihenfolge ist der Kern: erst umpacken, dann die Geometrie.
        #   Andersherum rechnet Tk nach dem Setzen neu, und die eben gesetzte
        #   Ecke gilt nicht mehr.
        if 'def klappzustand_setzen' in _q44:
            _teil46 = _q44[_q44.index('def klappzustand_setzen'):]
            _teil46 = _teil46[:_teil46.index('    def ', 10)]
            pruefe('_leiste_ausrichten()' in _teil46,
                   'beim Klappen wird die Leistenseite gesetzt')
            if ('_leiste_ausrichten()' in _teil46
                    and 'self.root.geometry(' in _teil46):
                pruefe(_teil46.index('_leiste_ausrichten()')
                       < _teil46.index('self.root.geometry('),
                       'und zwar VOR der Geometrie, nicht danach')
            pruefe('minsize' in _teil46,
                   'die Mindestgroesse zieht beim Klappen mit '
                   '(der Fehler hinter rc3-rc6)')
        # ⚠ Und der Ziehgriff darf nicht auf der Leiste landen: Bei unten
        #   haengender Leiste sitzt dort das ✕.
        if 'def _grip_nachziehen' in _q44:
            _teil47 = _q44[_q44.index('def _grip_nachziehen'):]
            _teil47 = _teil47[:_teil47.index('    def ', 10)]
            pruefe('_verankert()' in _teil47,
                   'der Ziehgriff sitzt an der freien Ecke')
        # ⚠ Und das Ziehen selbst muss dieselbe Ecke kennen. Sonst wird gegen
        #   den Bildschirmrand gezogen, an dem das Fenster klebt — gemeldet
        #   am 02.09.2026: „laesst sich nur nach unten ziehen".
        if 'def _resize' in _q44:
            _teil48 = _q44[_q44.index('def _resize'):]
            _teil48 = _teil48[:_teil48.index('    def ', 10)]
            pruefe('_verankert()' in _teil48,
                   'das Ziehen kennt die verankerten Kanten')
            pruefe('rechte_kante' in _teil48 and 'untere_kante' in _teil48,
                   'und haelt sie fest, statt von oben links zu rechnen')
        # ⚠ Der Streifen im Aufblend-Betrieb braucht BEIDE Achsen. Die
        #   senkrechte fehlte und sass deshalb auf halber Bildhoehe.
        if 'def _anfasser_zeigen' in _q44:
            _teil49 = _q44[_q44.index('def _anfasser_zeigen'):]
            _teil49 = _teil49[:_teil49.index('    def ', 10)]
            pruefe('_anfasser_y(' in _teil49,
                   'der Anfasser-Streifen richtet sich auch senkrecht '
                   'nach der Ecke')
        # ⚠ Beim Eckenwechsel ist im Aufblend-Betrieb NUR der Streifen zu
        #   sehen — das Overlay ist versteckt. Wird er nicht mitgezogen,
        #   bleibt er stehen, bis einmal auf- und zugeblendet wurde.
        if 'def ecke_anwenden' in _q44:
            _teil50 = _q44[_q44.index('def ecke_anwenden'):]
            _teil50 = _teil50[:_teil50.index('    def ', 10)]
            pruefe('_anfasser_zeigen()' in _teil50,
                   'ein Eckenwechsel zieht den Streifen sofort mit')
            # ⚠ Und mit den BERECHNETEN Werten, nicht mit `_current_geom()`:
            #   Tk liefert nach `geometry()` noch die alte Lage zurueck.
            #
            # ⚠⚠ Auf die ZUWEISUNG pruefen, nicht auf den blossen Namen — der
            #   steht als Warnung im Kommentar daneben, und die erste Fassung
            #   dieser Pruefung ist genau darueber gestolpert.
            pruefe('_letzte_lage = self._current_geom()' not in _teil50,
                   'und rechnet dabei nicht mit der noch alten Geometrie')
            pruefe("_letzte_lage = '%dx%d+%d+%d'" in _teil50,
                   'sondern mit den eben gesetzten Werten')

        # ⚠⚠ Zwei Patches koennen auf dieselbe Kurzform kuerzen — ein Hotfix im
        #   Live-Kanal erzeugt genau das: 4.10.0-live.12519617 und
        #   4.10.0-live.12545750 heissen beide „4.10.0". Wo die Kurzform allein
        #   steht, sind sie nicht auseinanderzuhalten. In v3.9.1 im BERICHT
        #   behoben, im Patch-MENUE aber nicht — gemeldet 02.09.2026. Diese
        #   Pruefung deckt beide Stellen ab, damit es nicht an einer dritten
        #   wieder auftaucht.
        for _datei48, _funktion48, _wo48 in (
                ('bericht.py', 'def _patchhistorie', 'im Bericht'),
                ('bestandsfenster.py', 'def _patches', 'im Patch-Menue')):
            _p48 = os.path.join(os.path.dirname(os.path.dirname(
                os.path.abspath(__file__))), 'scbp', _datei48)
            if not os.path.isfile(_p48):
                continue
            with open(_p48, encoding='utf-8') as _f48:
                _q48 = _f48.read()
            if _funktion48 not in _q48:
                pruefe(False, 'die Patch-Liste %s ist auffindbar' % _wo48)
                continue
            _t48 = _q48[_q48.index(_funktion48):]
            _t48 = _t48[:_t48.index('\ndef ', 10) if '\ndef ' in _t48[10:]
                        else min(len(_t48), 3000)]
            pruefe('count(' in _t48,
                   'die volle Version erscheint %s, wenn die Kurzform doppelt '
                   'vorkommt' % _wo48)

        # ⚠ Seiten im Leerlauf vorbauen (02.09.2026). Jede Seite entsteht beim
        #   ersten Aufruf und braucht dafuer bis zu einer Sekunde — gemessen im
        #   Startverlauf eines echten Berichts. Der Vorbau nimmt diese
        #   Wartezeit vorweg. Zwei Dinge muessen dabei stimmen, sonst wird es
        #   schlimmer statt besser.
        _hf46 = os.path.join(os.path.dirname(os.path.dirname(
            os.path.abspath(__file__))), 'scbp', 'hauptfenster.py')
        if os.path.isfile(_hf46):
            with open(_hf46, encoding='utf-8') as _f46:
                _q46 = _f46.read()
            pruefe('def _seiten_vorbauen' in _q46,
                   'die Seiten werden im Leerlauf vorgebaut')
            if 'def _seiten_vorbauen' in _q46:
                _t46 = _q46[_q46.index('def _seiten_vorbauen'):]
                _t46 = _t46[:_t46.index('    def ', 10)]
                # EINE Seite je Durchlauf — sonst haelt der Vorbau das Fenster
                # sekundenlang fest, statt es freizugeben.
                pruefe('after(' in _t46,
                       'und gibt zwischen den Seiten die Bedienung frei')
                pruefe('in self.gezeichnet' in _t46,
                       'und baut keine Seite doppelt')
                # ⚠⚠ Der Fehler, den der Vorbau selbst erzeugt hat
                #   (02.09.2026, direkt nach rc4): Manche Seiten rufen beim
                #   Bauen `focus_set()` — das Suchfeld der Bauplan-Liste tut
                #   es. Im Hintergrund gebaut, klaut eine unsichtbare Seite
                #   damit den Eingabefokus, und im sichtbaren Feld kommt
                #   nichts mehr an. Der Vorbau muss ihn zurueckgeben.
                pruefe('focus_get()' in _t46 and 'focus_set()' in _t46,
                       'und gibt den Eingabefokus zurueck, den eine Seite '
                       'sich beim Bauen nimmt')
            pruefe('_vorbau_laeuft' in _q46,
                   'er laeuft nur einmal je Fenster an')
            # ⚠ Die Sperre muss beim Neuaufbau zurueck — sonst laeuft der
            #   Vorbau nach einem Sprachwechsel nie wieder.
            if 'self.seiten, self.gezeichnet' in _q46:
                _t47 = _q46[_q46.index('self.seiten, self.gezeichnet'):][:400]
                pruefe('_vorbau_laeuft = False' in _t47,
                       'und wird beim Neuaufbau des Fensters zurueckgesetzt')
        # ⚠ Gemessen am 28.08.2026: Ein ungezeichnetes Widget meldet Breite 1 und
        #   Position 0. `ismapped()` allein reicht deshalb nicht — sonst saesse
        #   das Schloss in der Bildschirmecke statt auf der Leiste.
        pruefe('winfo_width() > 1' in _rumpf44,
               'und prueft die Masse mit, nicht nur ismapped')

        # Und das Schloss darunter sagt dasselbe — sonst stuende dort das
        # Gegenteil des wahren Zustands, falls das Fenster darueber ausbleibt.
        from scbp import zeichen as zn44

        class _Knopf44:
            symbol, farbe = 'schloss_auf', zn44.GRAU

            def symbol_tauschen(self, name):
                self.symbol = name

            def faerben(self, farbe):
                self.farbe = farbe

        class _Leiste44:
            pass

        _l44 = _Leiste44()
        _l44.schloss_lbl = _Knopf44()
        w44.Overlay._leistenschloss(_l44, True)
        pruefe(_l44.schloss_lbl.symbol == 'schloss_zu'
               and _l44.schloss_lbl.farbe == zn44.GRUEN,
               'beim Zusperren wird das Leisten-Schloss zu und gruen')
        w44.Overlay._leistenschloss(_l44, False)
        pruefe(_l44.schloss_lbl.symbol == 'schloss_auf'
               and _l44.schloss_lbl.farbe == zn44.GRAU,
               'und danach wieder offen und grau')

        # ⚠ Gemeldet von Haldjas (pr0) am 28.08.2026 zu rc91: „nach dem ersten
        #   start ist das schloss symbol weiterhin in der ecke wie vorher auch,
        #   erst wenn man es einmal benutzt hat aendert es die position in die
        #   leiste."
        #
        #   Grund: `verhalten_anwenden()` laeuft unmittelbar vor `mainloop()`.
        #   Die Leiste steht dann im Baum, aber Tk hat noch nichts gemalt — also
        #   meldet `winfo_ismapped()` falsch, und der Rueckfall auf die Ecke
        #   greift bei JEDEM Start, sobald jemand das Durchreichen eingeschaltet
        #   gespeichert hat. Es wird deshalb nachgefasst.
        pruefe('_nachfassen' in _rumpf44,
               'ist die Leiste noch nicht gezeichnet, wird nachgefasst')
        # ⚠ Und zwar OHNE vorher eines an der falschen Stelle zu bauen. Genau
        #   das hat Haldjas gesehen: „Schloss ist an 2 Positionen". Ein kurz
        #   aufblitzendes falsches Schloss waere nur die halbe Reparatur.
        _warte44 = _rumpf44.split('_nachfassen(versuch + 1)', 1)[1]
        pruefe(_warte44.lstrip(') ' + chr(10)).startswith('return'),
               'und zwar ohne vorher eines an der falschen Stelle zu bauen')
        _nach44 = _q44[_q44.index('def _nachfassen'):]
        _nach44 = _nach44[:_nach44.index('def _leistenschloss')]
        pruefe("einstellung_wahrheit('durchklickbar'" in _nach44,
               'und zwar nur, solange das Durchreichen ueberhaupt noch an ist')
        # ⚠ Begrenzt — sonst liefe es ewig weiter, solange das Overlay
        #   eingeklappt oder im Pop-up-Betrieb versteckt ist.
        pruefe('versuch < 10' in _rumpf44,
               'und begrenzt, damit es nicht ewig weiterlaeuft')
        # Und NUR, solange die Leiste ueberhaupt noch kommt. Gemeldet von
        # Haldjas (pr0) zu rc95: Beim Zublenden lief das Nachfassen blind mit
        # und verzoegerte den Ruecksprung um genau 10 x 300 ms = 3 Sekunden.
        # Gemessen trennt root.winfo_ismapped() die Faelle:
        #     Start, nach update_idletasks   root=1  knopf=0  -> wird gemalt
        #     nach withdraw()                root=0  knopf=0  -> soll weg sein
        pruefe('_wird_noch_gezeichnet()' in _rumpf44,
               'und nur, solange die Leiste ueberhaupt noch kommt')
        _wnz = _q44[_q44.index('def _wird_noch_gezeichnet'):]
        _wnz = _wnz[:_wnz.index('    def ', 10)]
        pruefe('self.root.winfo_ismapped()' in _wnz,
               'unterschieden am Fenster selbst, nicht am Knopf')
    finally:
        if _alt_home44 is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _alt_home44

    print()
    print('45. Kein totes Bild in der Anleitung')
    # ⚠ Gefunden am 28.08.2026 beim Ergaenzen der Funktionsliste: Drei Bilder
    # in der Tabelle zeigten ins Leere — `symbole/22/punkt-blau.png` (zweimal)
    # und `symbole/22/gemerkt-gruen.png`. Beides sind **Zeilen**-Symbole, und die
    # werden nur bis 18 px gebaut (`tools/symbole_bauen.py`: ZEILE geht bis 18,
    # KNOPF bis 30). Auf GitHub stand dort ein kaputtes Bild-Kaestchen — in der
    # Funktionsliste, also auf dem, was ein Interessierter als Erstes sieht.
    #
    # Niemand hat es gemeldet, weil ein fehlendes Bild niemandem wehtut. Genau
    # deshalb gehoert es in den Selbsttest: Wer ein Symbol umbenennt oder eine
    # Groesse nicht baut, erfaehrt es hier statt gar nicht.
    import re as _re45
    _tot45 = []
    for _d45 in ('README.en.md', 'README.md', 'CHANGELOG.en.md', 'CHANGELOG.md',
                 'ROADMAP.en.md', 'ROADMAP.md'):
        _pfad45 = os.path.join(WURZEL, _d45)
        if not os.path.exists(_pfad45):
            continue
        _inhalt45 = open(_pfad45, encoding='utf-8').read()
        _bilder45 = (_re45.findall(r'src="([^":]+)"', _inhalt45)
                     + _re45.findall(r'!\[[^\]]*\]\(([^):]+)\)', _inhalt45))
        for _b45 in _bilder45:
            if not os.path.exists(os.path.join(WURZEL, _b45)):
                _tot45.append('%s -> %s' % (_d45, _b45))
    for _z45 in _tot45:
        print('         ' + _z45)
    pruefe(not _tot45,
           'jedes Bild in der Doku liegt auch im Repo (%d tote)' % len(_tot45))

    print()
    print('46. Ein Fund ist ein Fund - kein Wartezustand mehr')
    # ⚠ Bis v3.0.0-rc94 stand ein Bauplan aus der Game.log GELB da: „vorlaeufig",
    # bis die Launcher-Datei ihn auf Gruen bestaetigt. Diese Bestaetigung kann es
    # nicht mehr geben — die Game.log ist die Quelle, der Launcher nur noch eine
    # Ergaenzung. Uebrig blieb ein Zustand, aus dem nichts mehr herausfuehrt:
    # Wer den Launcher hatte, sah dauerhaft Gelb; wer ihn nicht hat, dauerhaft
    # Gruen — bei genau derselben Sicherheit.
    #
    # Gemeldet am 28.08.2026: die Bestaetigung wird „nicht nur nicht mehr
    # gebraucht, sondern kann auch gar nicht mehr geben".
    #
    # Diese Pruefung haelt fest, dass die Mechanik WEG ist und nicht nur
    # stillgelegt — halb entfernter Code kommt sonst beim naechsten Umbau
    # zurueck.
    _q46 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'), encoding='utf-8').read()
    for _rest46 in ('provisional', '_match_prov', 'self.prov',
                    "'vorlaeufig'", "'confirm'"):
        pruefe(_rest46 not in _q46,
               'keine Spur mehr von %s im Programm' % _rest46)
    from scbp import sprache as sp46
    pruefe('vorlaeufig' not in sp46.TEXTE,
           'und der Text „vorlaeufig" ist aus der Sprachdatei raus')
    # ⚠ Und die Anleitung darf die zwei Stufen nicht weiter versprechen — sonst
    #   sucht jemand einen gelben Punkt, den es nicht gibt.
    for _d46 in ('README.md', 'README.en.md'):
        _t46 = open(os.path.join(WURZEL, _d46), encoding='utf-8').read()
        pruefe('vorlaeufig-gelb' not in _t46,
               '%s zeigt keinen gelben Wartepunkt mehr' % _d46)

    print()
    print('47. Protokolle lassen sich erneut einlesen')
    # ⚠ Gemeldet am 28.08.2026, wenige Stunden nach v3.0.0: Ein
    # Bauplan kam an, waehrend der Watcher zu war und Star Citizen weiterlief.
    # Beim naechsten Start war er weg — und zwar dauerhaft.
    #
    # Der Grund: `nachlesen()` fasste die laufende Game.log nur an, wenn sie
    # NOCH NIE gelesen war. Danach galt sie als erledigt, das Mitlesen setzte
    # beim gemerkten Stand an, und alles davor war unerreichbar. In
    # `logbackups/` landet die Datei erst beim naechsten Spielstart.
    #
    # Gemessen: Bauplan bei Byte 11.987.664, Lesestand 12.759.872.
    _q47 = open(os.path.join(WURZEL, 'scbp', 'logquelle.py'),
                encoding='utf-8').read()
    _lauf47 = _q47[_q47.index('if auch_laufende:'):]
    _lauf47 = _lauf47[:_lauf47.index('bericht[')]
    # ⚠ Nur den Code ansehen. Die alte Bedingung steht als Zitat im Kommentar
    #   daneben — wer die Zeilen nicht filtert, prueft die Erklaerung statt der
    #   Sache und meldet einen Fehler, den es nicht gibt.
    _code47 = chr(10).join(z for z in _lauf47.split(chr(10))
                           if not z.lstrip().startswith('#'))
    pruefe("aktiv_holen(aktiv) is None" not in _code47,
           'die laufende Game.log wird immer gelesen, nicht nur beim ersten Mal')
    from scbp import logquelle as lq47
    pruefe(hasattr(lq47, 'alles_neu'),
           'es gibt einen Weg, alles noch einmal einzulesen')

    # ⚠ Und beides muss BEDIENBAR sein — an zwei Stellen, wie gewuenscht:
    #    am Overlay (dort merkt man den fehlenden Bauplan) und in den
    #    Einstellungen (dort sucht man danach).
    from scbp import overlay as ov47
    pruefe(hasattr(ov47, 'neu_einlesen_anstossen'),
           'der Anstoss geht ueber einen Rueckruf wie beim Schloss')
    _w47 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'), encoding='utf-8').read()
    pruefe('self.neulesen_lbl' in _w47, 'ein Knopf sitzt in der Overlay-Leiste')
    _s47 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'), encoding='utf-8').read()
    pruefe("t('s_be_neu')" in _s47, 'und einer in den Einstellungen')
    # ⚠ Die Arbeit gehoert in den Watcher-Faden. Laese die Seite selbst ein und
    #   speicherte, ueberschriebe der Faden das beim naechsten Fund mit seinem
    #   aelteren Stand — die gefundenen Bauplaene waeren wieder weg.
    pruefe('alles_neu' not in _s47,
           'die Seite liest NICHT selbst ein (der Bestand hat einen Besitzer)')

    print()
    print('48. Nach einer neuen Fassung wird immer wieder gesehen')
    # ⚠ Gemeldet am 28.08.2026: v3.0.1 war draussen, der laufende
    # Watcher schwieg — obwohl er die Fassung laengst abgerufen hatte und sie in
    # seinem Zwischenspeicher stand.
    #
    # Der Grund: `_nach_version_sehen()` wurde GENAU EINMAL gerufen, zwei
    # Sekunden nach dem Start. Der Stundenabstand in `aktualisierung.nachsehen()`
    # begrenzt nur, wie oft gefragt werden DARF — fragen muss trotzdem jemand.
    # Wer den Watcher durchlaufen liess, erfuhr nie von einer neuen Fassung.
    _w48 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'), encoding='utf-8').read()
    _f48 = _w48[_w48.index('def _nach_version_sehen'):]
    _f48 = _f48[:_f48.index('    def ', 10)]
    pruefe('_nach_version_sehen' in _f48.split('def _nach_version_sehen', 1)[1],
           'die Pruefung plant sich selbst wieder ein')
    pruefe('VERSION_TAKT' in _f48, 'und zwar in einem benannten Takt')

    # ⚠ Und ein erwarteter Fehler darf das Protokoll nicht fluten: Beim Download
    #   kommt der Fortschritt im Sekundentakt; geht dabei das Fenster zu, wirft
    #   jeder Aufruf. Ein Bericht zeigte 50 von 50 Plaetzen mit derselben
    #   Meldung — jeder echte Fehler war daraus verdraengt.
    _s48 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'), encoding='utf-8').read()
    pruefe('_IM_TK_GEMELDET' in _s48,
           'derselbe erwartete Fehler wird nur einmal gemerkt')

    print()
    print('49. Jeder Sprachschluessel, der gerufen wird, gibt es auch')
    # ⚠ Gemeldet am 28.08.2026: Am Raketen-Symbol stand als Hinweis
    # woertlich `s_sp_start` — der Schluesselname statt des Textes.
    #
    # `t()` gibt den Schluessel zurueck, wenn die Tabelle ihn nicht kennt. Das
    # ist als Notnagel richtig (besser als ein Absturz), macht den Fehler aber
    # unsichtbar, bis ihn jemand im laufenden Programm sieht. Der Selbsttest
    # pruefte bis dahin nur, ob deutscher Text in der englischen Oberflaeche
    # steht — ein FEHLENDER Schluessel ist etwas anderes.
    #
    # Die Pruefung fand auf einen Schlag drei: `s_sp_start`, `m_keine_fassung`
    # und `aktuelle_fassung`. Von Hand ist das bei ueber 600 Eintraegen nicht zu
    # halten.
    import ast as _ast49
    from scbp import sprache as _sp49

    _RUFER49 = ('t', 'Satz', 'text')
    _fehlend49 = []
    for _ordner49, _unter49, _dateien49 in os.walk(WURZEL):
        if any(_teil in _ordner49 for _teil in
               ('.git', 'build', 'dist', '__pycache__', 'tools')):
            continue
        for _name49 in _dateien49:
            if not _name49.endswith('.py'):
                continue
            _pfad49 = os.path.join(_ordner49, _name49)
            try:
                _baum49 = _ast49.parse(open(_pfad49, encoding='utf-8').read())
            except Exception:
                continue

            # ⚠⚠ Ein Schluessel muss NICHT direkt im Aufruf stehen. Assistent
            # und Einstellungsseite holen ihre drei Knopftexte ueber eine
            # Schleifenvariable:
            #
            #     for schluessel, quelle in (('inj_quelle_de', 'deutsch'), …):
            #         tk.Label(text='  %s  ' % t(schluessel))
            #
            # Bis 03.09.2026 sah diese Pruefung nur `t('literal')` und ging
            # daran vorbei. Folge: Beim Aufraeumen am 26.08.2026 galten
            # `inj_quelle_de/_ss/_orig` als tot und flogen raus — acht Tage lang
            # standen im Setup (Schritt 4 von 5) die nackten Schluesselnamen als
            # Knopfbeschriftung. Gemeldet von Haldjas, 03.09.2026.
            #
            # Deshalb werden Schleifen ueber feste Listen mit aufgeloest: Was die
            # Variable annehmen KANN, wird geprueft. Steht ein Tupel als Ziel
            # (`for a, b in …`), zaehlt nur die Stelle, die auch wirklich in den
            # Aufruf geht — sonst laege hier gleich `'deutsch'` mit im Netz.
            #
            # ⚠ **Gemerkt wird der AUFRUF, nicht der Variablenname.** Der erste
            # Entwurf ordnete die Werte dem Namen zu und galt damit fuer die
            # ganze Datei. In `seiten.py` heisst ein Funktionsparameter ebenfalls
            # `schluessel` (`def form(anzahl, schluessel)`) — der bekam prompt die
            # Werte einer ganz anderen Schleife untergeschoben und wurde dreimal
            # grundlos angemahnt. Eine Pruefung, die falsch anschlaegt, schaltet
            # man ab; deshalb zaehlt nur, was im Koerper DIESER Schleife steht.
            _ausschleife49 = {}
            for _kn in _ast49.walk(_baum49):
                if not isinstance(_kn, _ast49.For):
                    continue
                _elts49 = getattr(_kn.iter, 'elts', None)
                if not _elts49:
                    continue
                _ziel49 = _kn.target
                if isinstance(_ziel49, _ast49.Name):
                    _stellen49 = [(_ziel49.id, None)]
                elif isinstance(_ziel49, _ast49.Tuple):
                    _stellen49 = [(_e.id, _i)
                                  for _i, _e in enumerate(_ziel49.elts)
                                  if isinstance(_e, _ast49.Name)]
                else:
                    continue
                for _nam49, _pos49 in _stellen49:
                    _werte49 = set()
                    for _w49 in _elts49:
                        if _pos49 is None:
                            _kand49 = [_w49]
                        elif (isinstance(_w49, (_ast49.Tuple, _ast49.List))
                              and len(_w49.elts) > _pos49):
                            _kand49 = [_w49.elts[_pos49]]
                        else:
                            _kand49 = []
                        for _k49 in _kand49:
                            if (isinstance(_k49, _ast49.Constant)
                                    and isinstance(_k49.value, str)
                                    and _k49.value):
                                _werte49.add(_k49.value)
                    if not _werte49:
                        continue
                    for _rumpf49 in _kn.body:
                        for _c49 in _ast49.walk(_rumpf49):
                            if (isinstance(_c49, _ast49.Call) and _c49.args
                                    and isinstance(_c49.args[0], _ast49.Name)
                                    and _c49.args[0].id == _nam49):
                                _ausschleife49.setdefault(
                                    id(_c49), set()).update(_werte49)

            for _kn49 in _ast49.walk(_baum49):
                if not isinstance(_kn49, _ast49.Call) or not _kn49.args:
                    continue
                _f49 = _kn49.func
                _ruf49 = (_f49.attr if isinstance(_f49, _ast49.Attribute)
                          else (_f49.id if isinstance(_f49, _ast49.Name) else None))
                if _ruf49 not in _RUFER49:
                    continue
                _erst49 = _kn49.args[0]
                if isinstance(_erst49, _ast49.Constant):
                    _moeglich49 = ([_erst49.value]
                                   if isinstance(_erst49.value, str)
                                   and _erst49.value else [])
                elif isinstance(_erst49, _ast49.Name):
                    # Der Schluessel kommt aus einer Schleife (siehe oben).
                    _moeglich49 = sorted(_ausschleife49.get(id(_kn49), ()))
                else:
                    _moeglich49 = []
                for _wert49 in _moeglich49:
                    if _wert49 not in _sp49.TEXTE:
                        _fehlend49.append('%s:%d  %s(%r)' % (
                            os.path.relpath(_pfad49, WURZEL), _kn49.lineno,
                            _ruf49, _wert49))
    for _z49 in sorted(set(_fehlend49)):
        print('         ' + _z49)
    pruefe(not _fehlend49,
           'kein Aufruf zeigt den Schluesselnamen statt des Textes (%d)'
           % len(set(_fehlend49)))

    print()
    print('50. Der Autostart merkt sich einen Pfad, den es morgen noch gibt')
    # ⚠ Gefunden am 29.08.2026 auf einem Linux-Rechner: In der Autostart-Datei
    # stand `Exec=/tmp/.mount_SC-BP-ji95vH/usr/bin/SC-BP-Watcher` — der temporaere
    # Einhaengepunkt des AppImage. Der bekommt bei JEDEM Start einen neuen
    # Zufallsnamen. Folge: Der Watcher startete nach einem Neustart nie wieder,
    # ohne Fehlermeldung — die Datei sah ja voellig richtig aus.
    #
    # Ursache war die Reihenfolge: Ein AppImage ist ebenfalls `sys.frozen`, also
    # gewann die frozen-Abfrage und `APPIMAGE` kam nie dran. Genau das wird hier
    # geprueft, weil es sich nur an der Reihenfolge entscheidet und ein spaeteres
    # Umsortieren den Fehler lautlos zurueckholen wuerde.
    import importlib as _im50
    from scbp import autostart as _as50
    _alt_appimage50 = os.environ.get('APPIMAGE')
    _alt_frozen50 = getattr(sys, 'frozen', None)
    try:
        os.environ['APPIMAGE'] = '/home/wer/Programme/SC-BP-Watcher.AppImage'
        sys.frozen = True
        _im50.reload(_as50)
        _befehl50 = _as50.befehl()
        pruefe('/tmp/.mount' not in _befehl50,
               'kein Wegwerf-Pfad aus dem AppImage-Einhaengepunkt')
        pruefe(_befehl50 == '/home/wer/Programme/SC-BP-Watcher.AppImage',
               'die echte AppImage-Datei gewinnt gegen die frozen-Abfrage')
    finally:
        if _alt_appimage50 is None:
            os.environ.pop('APPIMAGE', None)
        else:
            os.environ['APPIMAGE'] = _alt_appimage50
        if _alt_frozen50 is None:
            try:
                del sys.frozen
            except AttributeError:
                pass
        else:
            sys.frozen = _alt_frozen50
        _im50.reload(_as50)
    print()
    print('51. Angenommene Auftraege: bringt der etwas, das mir fehlt?')
    # Der Weg hat vier Glieder (Log -> Phrase -> Missionsschluessel -> Katalog).
    # Geprueft wird jedes einzeln, damit ein Bruch benannt werden kann statt nur
    # "meldet nichts". Die Daten werden nachgebaut — auf dem Bau-Rechner gibt es
    # weder Spiel noch Katalog.
    import importlib as _im51
    from scbp import auftraege as _au51

    # a) Die Marken des eigenen Werkzeugs muessen aus dem Titel verschwinden.
    _faelle51 = [
        ('Retake Platforms From Nine Tails <EM4>[BP!]</EM4>',
         'Retake Platforms From Nine Tails'),
        ('Retake Platforms[SCBPW] <EM4>[BP 4/8]</EM4>[/SCBPW]', 'Retake Platforms'),
        # ⚠ Das Zeichen steht HINTER der eckigen Klammer, nicht darin. Beide
        # Formen blieben in gespeicherten Protokollen stehen, weil die Regel
        # nur `[BP!]` kannte — also nur das Zeichen INNEN.
        ('Blackbox Retrieval <EM4>[BP]?</EM4>', 'Blackbox Retrieval'),
        ('Verified Bounty: Chen Bey <EM4>[BP]*</EM4>', 'Verified Bounty: Chen Bey'),
        # Fremde Marken mit Vorspann — `injektion.py` kennt sie laengst.
        ('Bounty <EM4>[10 Rep] [BP]</EM4>', 'Bounty'),
        # ⚠ Gegenprobe: Was KEIN Bauplan-Zusatz ist, muss stehen bleiben.
        ('Auftrag <EM4>[x]</EM4>', 'Auftrag <EM4>[x]</EM4>'),
        ('Ganz normaler Titel', 'Ganz normaler Titel'),
    ]
    for _roh51, _soll51 in _faelle51:
        pruefe(_au51.sauber(_roh51) == _soll51, 'Marken entfernt: %s' % _soll51[:34])

    # b) ⚠ Die Phrase kommt aus der global.ini MIT Platzhalter (`... : %s`).
    #    Bliebe er stehen, passte die Zeile nie — die Funktion waere tot und
    #    niemand haette es gemerkt.
    pruefe(_au51._phrase_kuerzen('Auftrag angenommen: %s') == 'Auftrag angenommen',
           'der Platzhalter am Phrasen-Ende faellt weg')

    # c) Das Suchmuster muss die echte Logzeile treffen — und die Zwischenziele
    #    in Ruhe lassen. ⚠ Auf Deutsch heissen `MissionEvent_Available` UND
    #    `ObjectiveEvent_Activated` beide "Neuer Auftrag"; wer darauf hoert,
    #    meldet bei jedem Etappenziel.
    _m51 = _au51.muster()
    _treffer51 = _m51.findall(
        'Added notification "Auftrag angenommen: Retake Platforms: "\n'
        'Added notification "Contract Accepted: Data Transfer: "\n')
    pruefe(_treffer51 == ['Retake Platforms', 'Data Transfer'],
           'Annahme wird erkannt, deutsch und englisch')
    pruefe(not _m51.findall('Added notification "Neuer Auftrag: Koerper durchsuchen: "'),
           'ein Zwischenziel loest NICHTS aus')
    pruefe(not _m51.findall('Added notification "Auftrag zurueckgezogen: Irgendwas: "'),
           'ein zurueckgezogener Auftrag loest NICHTS aus')

    # d) Die Auswertung selbst, mit nachgebautem Katalog.
    _alt51 = _au51._missionen, _au51._index, _au51._muster_index
    try:
        _au51._missionen = {'test_title_001': {'bp': ['Alpha BP', 'Beta BP', 'Gamma BP']}}
        _au51._index = {'testauftrag': 'test_title_001'}
        _au51._muster_index = []
        _hat51 = lambda n: n in ('Alpha BP', 'Beta BP')
        pruefe(_au51.pruefen('Testauftrag', _hat51) == (3, ['Gamma BP']),
               'meldet Gesamtzahl und was davon fehlt')
        pruefe(_au51.pruefen('Testauftrag', lambda n: True) == (3, []),
               'hat man alles, bleibt die Liste leer')
        pruefe(_au51.pruefen('Voellig unbekannter Auftrag', _hat51) is None,
               'unbekannter Auftrag: es wird GESCHWIEGEN, nicht geraten')
        # ⚠ Platzhalter-Titel: 58 von 353 tragen `~mission(...)`, ein woertlicher
        #    Vergleich scheitert dort. Der Rest muss trotzdem woertlich passen.
        _au51._index = {}
        _au51._muster_index = [(__import__('re').compile(r'^High\-Risk Bounty: .+$'),
                                'test_title_001')]
        pruefe(_au51.pruefen('High-Risk Bounty: Jemand', _hat51) == (3, ['Gamma BP']),
               'Platzhalter-Titel werden ueber ein Muster gefunden')
        pruefe(_au51.pruefen('Low-Risk Bounty: Jemand', _hat51) is None,
               'und das Muster passt nicht auf einen anderen Auftragstyp')
    finally:
        _au51._missionen, _au51._index, _au51._muster_index = _alt51

    # e) Jeder Text der neuen Zeile muss in BEIDEN Sprachen dastehen.
    from scbp import sprache as _sp51
    for _k51 in ('auftrag_zeile', 'auftrag_fehlt', 'auftrag_fehlt_mehr',
                 'auftrag_komplett'):
        _w51 = _sp51.TEXTE.get(_k51)
        pruefe(bool(_w51) and len(_w51) == 2 and all(_w51),
               'Text %s gibt es deutsch und englisch' % _k51)

    # f) Der Log-Leser darf den Bauplan-Weg nicht angetastet haben.
    from scbp import logquelle as _lq51
    _tail51 = _lq51.LogTail(_lq51.Lesestand())
    pruefe(getattr(_tail51, 'auftrag_muster', 'fehlt') is None,
           'ein frischer LogTail sucht KEINE Auftraege (muss gesetzt werden)')
    pruefe(_tail51.auftraege == [],
           'und traegt eine leere Auftragsliste')
    # ⚠ Der Bauplan-Weg darf sich nicht veraendert haben: `new_names()` liefert
    #    weiterhin Paare (Name, Zusatz) — mehrere Stellen verlassen sich darauf.
    pruefe(_lq51.LogTail.new_names.__doc__ and
           'Name, Zusatz' in _lq51.LogTail.new_names.__doc__,
           'new_names() liefert unveraendert (Name, Zusatz)')

    # ------------------------------------------------------------------
    # 52. Kaestchen nur an Bauplaene — nicht an Regionen und Abgabeorte
    #
    # Die Bloecke des SCDL-Teams gliedern mit '#'-Ueberschriften, und unter
    # dreien davon stehen Listen: '# Baupläne' (4379 Zeilen), '# Abgabe' (323)
    # und '# Region' (239). Bis zum 29.08.2026 bekam jede davon ein Kaestchen —
    # im Spiel stand '[  ] Stanton-System - Gefahr 4-6/10', als koennte man eine
    # Region besitzen. Rund 620 Zeilen in den Rohdaten, 838 in der fertigen
    # Datei (Bloecke werden mehrfach verwendet).
    print()
    print('52. Kaestchen nur an Bauplaenen, nicht an Regionen')
    from scbp import injektion as _inj52
    _block52 = ('\\n# Baupläne:\\n    - Atzkav Sniper Rifle\\n    - Aril Arms'
                '\\n\\n# Region: \\n    - Stanton-System - Gefahr 4-6/10'
                '\\n    - \\n    - Nyx-System - Gefahr 3-6/10'
                '\\n\\n# Abgabe:\\n    - Port Olisar')
    _habe52 = {katalog._norm('Aril Arms')}
    _neu52, _meine52, _gesamt52 = _inj52._kaestchen_setzen(_block52, _habe52)
    pruefe('[  ] Atzkav Sniper Rifle' in _neu52,
           'ein Bauplan, den man nicht hat, bekommt ein leeres Kaestchen')
    pruefe('[x]' in _neu52 and 'Aril Arms' in _neu52,
           'ein Bauplan, den man hat, wird angehakt')
    pruefe('- Stanton-System - Gefahr 4-6/10' in _neu52
           and '[  ] Stanton-System' not in _neu52,
           'eine REGION bekommt KEIN Kaestchen')
    pruefe('- Nyx-System - Gefahr 3-6/10' in _neu52,
           'auch die zweite Region bleibt unangetastet')
    pruefe('- Port Olisar' in _neu52 and '[  ] Port Olisar' not in _neu52,
           'ein ABGABEORT bekommt KEIN Kaestchen')
    pruefe((_meine52, _gesamt52) == (1, 2),
           'gezaehlt werden nur die Bauplaene (1 von 2)')
    # Englisch ist derselbe Aufbau, nur andere Ueberschriften.
    _en52 = ('\\n# Blueprints:\\n    - Atzkav Sniper Rifle'
             '\\n\\n# Region: \\n    - Stanton System'
             '\\n\\n# Delivery:\\n    - Port Olisar')
    _neu52en, _m52en, _g52en = _inj52._kaestchen_setzen(_en52, set())
    pruefe('[  ] Atzkav Sniper Rifle' in _neu52en and _g52en == 1,
           'englisch: nur unter "# Blueprints" wird angekreuzt')
    pruefe('- Stanton System' in _neu52en and '- Port Olisar' in _neu52en,
           'englisch: Region und Delivery bleiben unangetastet')

    # 52b. Kein Knopf schneidet seine Beschriftung ab
    #
    # `_knopf` bemisst die Leinwand mit `schrift.measure()`. Gezeichnet wird
    # aber mit der Schrift, die das System hergibt — weichen die ab, steht der
    # Text ueber den Rand und wird beidseitig abgeschnitten. Am 29.08.2026 in
    # rc7 gemeldet: Auf dem Knopf stand „erung speichern".
    print()
    print('52b. Knoepfe schneiden ihre Beschriftung nicht ab')
    import tkinter as _tk52b
    from scbp import seiten as _se52b
    from scbp.hauptfenster import Hauptfenster as _HF52b
    _w52b = _tk52b.Tk()
    try:
        _f52b = _HF52b(_w52b, version='knopfprobe')
        _w52b.update_idletasks()
        # ⚠ `s_lg_trotzdem` gibt es nicht mehr (der Ausweg ist entfallen).
        # Statt seiner der laengste verbliebene Lager-Knopf.
        _lang = [_sp51.TEXTE[k][0] for k in
                 ('s_lg_speichern', 's_lg_abbrechen', 's_lg_posten_weg',
                  's_lg_eintragen')]
        _lang += [_sp51.TEXTE[k][1] for k in
                  ('s_lg_speichern', 's_lg_posten_weg')]
        _eng52b = []
        for _txt in _lang:
            _k = _se52b._knopf(_f52b, _w52b, _txt, lambda: None)
            # ⚠ Nur den TEXT messen. `bbox('all')` nimmt den Rahmen mit, und
            # der ist naturgemaess so breit wie die Leinwand — die Pruefung
            # schluege dann immer an.
            _text_ids = [_i for _i in _k.find_all()
                         if _k.type(_i) == 'text']
            _kasten = _k.bbox(_text_ids[0]) if _text_ids else None
            _breit = int(_k['width'])
            if _kasten and (_kasten[2] - _kasten[0]) > _breit:
                _eng52b.append('%r braucht %d, hat %d'
                               % (_txt, _kasten[2] - _kasten[0], _breit))
            _k.destroy()
        pruefe(not _eng52b,
               'jeder Knopf ist breit genug fuer seinen Text (%d zu eng)'
               % len(_eng52b))
        for _e in _eng52b[:4]:
            print('       ·', _e)
    finally:
        _w52b.destroy()

    # 52c. Die Mindestbreite des Overlays ist keine Fantasiezahl
    #
    # Der erste Anlauf fragte die Kopfleiste nach ihrer Wunschbreite. Die laeuft
    # aber mit `pack_propagate(False)` und meldete **1 Pixel** — die Grenze war
    # damit wirkungslos, und im Overlay war kein Symbol mehr zu sehen.
    print()
    print('52c. Mindestbreite des Overlays deckt die Symbolleiste')
    import importlib.util as _ilu52c
    _spec52c = _ilu52c.spec_from_file_location(
        '_scbpw52c', os.path.join(_wurzelpfad, 'sc_bp_watcher.py'))
    _m52c = _ilu52c.module_from_spec(_spec52c)
    sys.modules['_scbpw52c'] = _m52c
    _spec52c.loader.exec_module(_m52c)
    _ov52c = _m52c.Overlay()
    try:
        _ov52c.root.update_idletasks()
        _kinder52c = _ov52c.kopf.winfo_children()
        pruefe(len(_kinder52c) >= 5,
               'die Kopfleiste hat ihre Elemente (%d)' % len(_kinder52c))
        _summe52c = sum(_k.winfo_reqwidth() for _k in _kinder52c)
        _min52c = _ov52c._mindestbreite()
        pruefe(_min52c >= _summe52c,
               'die Mindestbreite deckt alle Elemente (%d >= %d)'
               % (_min52c, _summe52c))
        pruefe(_min52c > _ov52c.kopf.winfo_reqwidth(),
               'sie stuetzt sich NICHT auf die Wunschbreite der Leiste')
        pruefe(_ov52c.root.winfo_width() >= _min52c,
               'und das Fenster ist mindestens so breit')
    finally:
        _ov52c.root.destroy()

    # 52d. Suchfelder vergessen ihren Inhalt beim naechsten Aufruf
    #
    # Seiten werden EINMAL gebaut und danach nur ein- und ausgeblendet. Ohne
    # Rueckruf stand der Suchbegriff von vorhin noch da: „da sollte man den
    # Titan-Eintrag im Suchfeld nicht speichern" (29.08.2026).
    print()
    print('52d. Suchfelder sind beim erneuten Aufrufen leer')
    _w52d = _tk52b.Tk()
    try:
        _f52d = _HF52b(_w52d, version='suchprobe')
        for _seite in ('bergbau', 'herstellung'):
            _f52d.oeffnen(_seite)
        _w52d.update_idletasks()
        pruefe(hasattr(_f52d, 'beim_zeigen'),
               'das Fenster fuehrt ein Verzeichnis fuer das erneute Anzeigen')
        # ⚠ Im Wegwerf-Ordner fehlen Bergbau- und Rezeptdaten; die Seiten
        # brechen dann vor dem Suchfeld ab. Ob sie sich anmelden, steht
        # deshalb im Quelltext — datenunabhaengig und trotzdem verbindlich.
        with open(os.path.join(_wurzelpfad, 'scbp', 'seiten.py'),
                  encoding='utf-8') as _fh52d:
            _qu52d = _fh52d.read()
        for _seite in ('bergbau', 'herstellung'):
            pruefe("beim_zeigen['%s']" % _seite in _qu52d,
                   'Seite %s meldet sich fuers erneute Anzeigen an' % _seite)
        pruefe(_qu52d.count('_suche_leeren_kreuz(') >= 3,
               'beide Suchfelder haben ein Kreuz zum Leeren')
        # Und der Rueckruf muss auch wirklich leeren.
        _leer52d = []
        for _seite, _ruf in _f52d.beim_zeigen.items():
            try:
                _ruf()
            except Exception as _a:
                _leer52d.append('%s: %s' % (_seite, _a))
        pruefe(not _leer52d, 'die Rueckrufe laufen fehlerfrei (%d Fehler)'
               % len(_leer52d))
    finally:
        _w52d.destroy()

    # 52e. Unterarten — Waffenart und Ruestungsrolle
    #
    # Der Katalog kennt nur `WeaponGun`; welche davon ballistisch sind und
    # welche Laser, steht ausschliesslich in den Rezeptdaten. Umgekehrt kennt
    # er die Koerperteile der Ruestung, die dort fehlen. Erst beide zusammen
    # ergeben die Filter, nach denen am 29.08.2026 gefragt wurde: „ich weiss
    # grad nicht, welche Ballistik sind, welche Laser".
    print()
    print('52e. Unterarten aus den Rezeptdaten')
    from scbp import herstellung as _he52e
    _echt52e = _he52e.einordnung
    _he52e.einordnung = lambda: {
        'zehnserieskanone': ('weapons', 'ballistic'),
        'laserkanone': ('weapons', 'laser'),
        'kampfhelm': ('armour', 'combat'),
        'kuehlerzwei': ('cooler', 'size2'),
    }
    try:
        pruefe(_he52e.unterart_von('Zehn-Series Kanone') == 'ballistic',
               'die Waffenart kommt aus den Rezeptdaten')
        pruefe(_he52e.art_von('Kampfhelm') == 'armour',
               'und die Art dazu')
        pruefe(_he52e.unterart_von('gibt es nicht') == '',
               'ein unbekannter Name ergibt keine Unterart')
    finally:
        _he52e.einordnung = _echt52e
    # Anzeigenamen: zweisprachig und mit Rueckfall auf den Rohwert
    for _k52e in ('he_art_weapons', 'he_art_armour', 'he_sub_ballistic',
                  'he_sub_laser', 'he_sub_combat', 'he_sub_stealth'):
        _w52e = _sp51.TEXTE.get(_k52e)
        pruefe(bool(_w52e) and len(_w52e) == 2 and all(_w52e),
               'Anzeigename %s gibt es deutsch und englisch' % _k52e)
    pruefe(_he52e.unterartname('gibtsnichtimmer') == 'gibtsnichtimmer',
           'eine unbekannte Unterart wird roh gezeigt statt verschluckt')
    for _k52e in ('ff_alle_unterarten', 'ff_alle_rollen', 'ff_alle_hersteller',
                  'ff_alle_zustaende', 'ff_zustand_habe', 'ff_zustand_fehlt',
                  's_bg_alle_erze', 's_bg_alle_orte', 'merk_eigene',
                  'merk_wartet', 'merk_eigene_h'):
        _w52e = _sp51.TEXTE.get(_k52e)
        pruefe(bool(_w52e) and len(_w52e) == 2 and all(_w52e),
               'Text %s gibt es deutsch und englisch' % _k52e)

    # 52f. Zwei Ebenen statt einer langen Liste
    #
    # Die Art-Auswahl hatte dreissig Eintraege — „Ruestung (Arme)",
    # „Ruestung (Beine)", „Helm", „Rucksack" je einzeln. Die Gliederung folgt
    # jetzt der gepflegten Vergleichsliste: sieben Gruppen, darunter die feinen Arten.
    # Gemessen an echten Daten deckt sie sich mit dieser Liste exakt.
    print()
    print('52f. Ober- und Unterkategorie')
    from scbp import kategorien as _ka52f
    # Die feine Waffenart steckt im Tag — nur dort.
    pruefe(_ka52f.einordnen(tag='BP_CRAFT_APAR_BallisticGatling_S4')
           == (_ka52f.SCHIFFSWAFFE, 'ballistic_gatling'),
           'die ballistische Gatling wird aus dem Tag erkannt')
    pruefe(_ka52f.einordnen(tag='BP_CRAFT_HRST_LaserScatterGun_S1')
           == (_ka52f.SCHIFFSWAFFE, 'scatter_gun'),
           'auch die Scattergun — ihr Tag heisst LaserScatterGun')
    pruefe(_ka52f.einordnen(tag='BP_CRAFT_APAR_BallisticScatterGun_S1')
           == (_ka52f.SCHIFFSWAFFE, 'scatter_gun'),
           'und die ballistische Fassung ebenso')
    # ⚠ Ohne die Reihenfolge im Muster wuerde `ScatterGun` das laengere Wort
    # schlucken — sechs von sieben Scatterguns fielen durch.
    pruefe(_ka52f.einordnen(tag='BP_CRAFT_behr_lmg_ballistic_01_mag')
           == (_ka52f.AUSRUESTUNG, 'magazin'),
           'Magazine erkennt man am Tag-Ende, nicht an der Katalog-Art')
    pruefe(_ka52f.einordnen(art='Char_Armor_Helmet')
           == (_ka52f.RUESTUNG, 'helm'),
           'Koerperteile kommen aus der Katalog-Art')
    pruefe(_ka52f.einordnen(art='Char_Armor_Legs')
           == (_ka52f.RUESTUNG, 'beine'), 'Beine ebenso')
    pruefe(_ka52f.einordnen(unterart='sniper')
           == (_ka52f.FPS_WAFFE, 'sniper'),
           'FPS-Waffen kommen aus dem Rezept-Untertyp')
    # Was sich nicht buendeln laesst, bleibt allein stehen — nicht in einem
    # Sammeltopf.
    _einzeln = _ka52f.einordnen(art='DockingCollarXY')
    pruefe(not _ka52f.ist_gruppe(_einzeln[0])
           and _ka52f.rohe_art(_einzeln[0]) == 'DockingCollarXY',
           'eine unbekannte Art bleibt als eigener Eintrag stehen')
    for _k52f in ('kat_ober_schiffswaffe', 'kat_ober_ruestung',
                  'kat_unter_ballistic_gatling', 'kat_unter_scatter_gun',
                  'kat_unter_helm', 'kat_unter_magazin',
                  'ff_unterart_waehlen'):
        _w52f = _sp51.TEXTE.get(_k52f)
        pruefe(bool(_w52f) and len(_w52f) == 2 and all(_w52f),
               'Text %s gibt es deutsch und englisch' % _k52f)

    # 52g. Beobachtungs-Muster treffen an Wortgrenzen
    #
    # Ein blosses „steckt drin" liefert falsche Treffer, die niemand als solche
    # erkennt: `arden backpack` traf am 29.08.2026 auf *Warden Backpack
    # Purgatory Camo*, und der Watcher meldete ein Ruestungsteil als
    # verfuegbar, das mit der gesuchten Ausruestung nichts zu tun hat. Bei
    # einer Staffelruestung geht es um genau ein Teil je Platz — die Farben
    # sind ueber Monate auf Tarnung getestet.
    print()
    print('52g. Muster treffen nur an Wortgrenzen')
    from scbp import merkliste as _mk52g
    _eintrag52g = {'titel': 'Probe', 'muster': ['xyz-cl backpack beispiel']}
    pruefe(_mk52g._muster_trifft(_eintrag52g, 'xyz-cl backpack beispiel'),
           'das gesuchte Teil wird erkannt')
    pruefe(not _mk52g._muster_trifft(
               {'titel': 'P', 'muster': ['yz backpack']},
               'xyz backpack muster camo'),
           'ein Muster mitten im Wort trifft NICHT')
    pruefe(_mk52g._muster_trifft(
               {'titel': 'P', 'muster': ['abc-mk4 legs grey']},
               'abc-mk4 legs grey'),
           'Bindestriche und Leerzeichen zaehlen als Grenze')
    pruefe(not _mk52g._muster_trifft({'titel': 'P', 'muster': []}, 'irgendwas'),
           'ein Eintrag ohne Muster trifft nichts')
    pruefe(not _mk52g._muster_trifft({'titel': 'P', 'muster': ['']}, 'irgendwas'),
           'ein leeres Muster ebenso wenig')

    # 52h. Die Kategorie wird an genau EINER Stelle geprueft
    #
    # Bis rc19 gab es eine zweite: eine Abkuerzung, die ganze Gruppen vorab
    # aussortierte und dabei Katalog-Art gegen Oberkategorie verglich. Das
    # trifft nie zu — jede Gruppe fiel heraus, die Liste zeigte „Nichts
    # gefunden" bei 157 vorhandenen Bauplaenen. Zwei Stellen fuer dieselbe
    # Frage waren genau eine zu viel.
    print()
    print('52h. Kategorie-Pruefung nur an einer Stelle')
    with open(os.path.join(_wurzelpfad, 'scbp', 'bestandsfenster.py'),
              encoding='utf-8') as _fh52h:
        _qu52h = _fh52h.read()
    pruefe("art_kennung(liste[0])" not in _qu52h,
           'keine Gruppen-Vorpruefung ueber die Katalog-Art mehr')
    pruefe(_qu52h.count("!= self.fein['art']") <= 1,
           'die Art wird hoechstens an einer Stelle verglichen')

    # 52i. Suche nach dem Auftrag
    #
    # „Retake" fand bis rc21 nichts, obwohl sechs Bauplaene aus Auftraegen mit
    # diesem Wort stammen. Wer eine Quest fliegt, will wissen, was dabei
    # herausspringt.
    print()
    print('52i. Nach Auftrag, Fraktion und Auftragsart suchen')
    from scbp import bestandsfenster as _bf52i
    _bp52i = {'n': 'Test-Bauplan', 'a': 'Cooler', 'q': [
        {'auftrag': 'Retake Platforms From Nine Tails', 'typ': 'Mercenary',
         'fraktion': 'Headhunters', 'wo': {'ort': 'Stanton'}}]}
    pruefe(_bf52i._passt(_bp52i, 'retake'), 'der Auftragsname wird gefunden')
    pruefe(_bf52i._passt(_bp52i, 'headhunters'), 'die Fraktion ebenso')
    pruefe(_bf52i._passt(_bp52i, 'mercenary'), 'und die Auftragsart')
    pruefe(not _bf52i._passt(_bp52i, 'xenothreat'),
           'ein fremder Begriff trifft nicht')
    # ⚠ `wo` ist ein Objekt, kein Text — ohne Pruefung stuerzt die Suche bei
    # jedem Tastendruck ab, und weil das im Zeichnen passiert, haengt das
    # Fenster.
    pruefe(_bf52i._passt(_bp52i, 'test-bauplan'),
           'ein Objekt in den Herkunftsangaben laesst die Suche nicht abstuerzen')
    _kat52i = {'bauplaene': {'x': _bp52i, 'y': dict(_bp52i, n='Zweiter')}}
    pruefe(_bf52i.auftraege_zu('retake', _kat52i)
           == [('Retake Platforms From Nine Tails', 2)],
           'die Uebersicht zaehlt die Bauplaene je Auftrag')
    pruefe(_bf52i.auftraege_zu('', _kat52i) == [],
           'ohne Suchbegriff keine Auftragsliste')
    # ⚠ Die Auftragszeile muss anklickbar sein: „die Quest muss natuerlich
    # anklickbar sein, sonst bringt das nichts." Ein Filter, aus dem man nicht
    # herauskommt, waere allerdings schlimmer als keiner — deshalb schaltet
    # derselbe Auftrag beim zweiten Klick wieder ab.
    with open(os.path.join(_wurzelpfad, 'scbp', 'bestandsfenster.py'),
              encoding='utf-8') as _fh52j:
        _qu52j = _fh52j.read()
    pruefe('_auftrag_waehlen' in _qu52j,
           'die Auftragszeilen sind anklickbar')
    pruefe("self.auftrag = '' if self.auftrag == name else name" in _qu52j,
           'ein zweiter Klick loest den Auftrag wieder')
    pruefe("or bool(self.auftrag)" in _qu52j,
           'der Zuruecksetzen-Knopf erscheint auch bei gewaehltem Auftrag')

    # 52k. Ein alter Katalog bekommt neue Schluessel
    #
    # Der Katalog auf der Platte kann Monate alt sein. Am 29.08.2026 standen
    # dort Magazine noch als „… magazine (15 cap)", waehrend der Bestand sie
    # als „… magazine (15)" fuehrt — die Angleichung der Mengenangabe kam
    # spaeter dazu. Ergebnis: Das Overlay meldete 405 Bauplaene, der
    # Fortschritt 382 von 738, und niemand konnte die Zahlen erklaeren.
    print()
    print('52k. Alte Katalog-Schluessel werden angeglichen')
    from scbp import katalog as _ka52k
    _alt52k = {'a03 sniper rifle magazine (15 cap)':
               {'n': 'A03 Sniper Rifle Magazine (15 cap)', 'a': 'WeaponAttachment'},
               'bolide': {'n': 'Bolide', 'a': 'PowerPlant'}}
    _neu52k = _ka52k._schluessel_angleichen(_alt52k)
    pruefe('a03 sniper rifle magazine (15)' in _neu52k,
           'der Schluessel wird aus dem Namen neu gebildet')
    pruefe('bolide' in _neu52k, 'unauffaellige Schluessel bleiben, wie sie sind')
    pruefe(len(_neu52k) == len(_alt52k), 'kein Bauplan geht dabei verloren')
    # Passt schon alles, wird nichts angefasst — dasselbe Verzeichnis zurueck.
    _sauber52k = {'bolide': {'n': 'Bolide'}}
    pruefe(_ka52k._schluessel_angleichen(_sauber52k) is _sauber52k,
           'ein frischer Katalog wird nicht unnoetig umgebaut')

    # 52m. Der Ziehgriff ueberlebt ein niedriges Overlay
    #
    # Er hing an der Liste — eine gute Idee, solange die Liste den Rest des
    # Fensters bekam. Seit die Auftragsleiste darueber Platz nimmt, kann die
    # Liste niedriger werden als der Griff selbst: Bei einem schmalen Overlay
    # mit einem laufenden Auftrag blieben ihr rund 20 Pixel, der Griff braucht
    # 26 — und war weg. Zweimal gemeldet am 29.08.2026.
    print()
    print('52m. Ziehgriff bleibt sichtbar')
    import importlib.util as _ilu52m
    _spec52m = _ilu52m.spec_from_file_location(
        '_scbpw52m', os.path.join(_wurzelpfad, 'sc_bp_watcher.py'))
    _m52m = _ilu52m.module_from_spec(_spec52m)
    sys.modules['_scbpw52m'] = _m52m
    _spec52m.loader.exec_module(_m52m)
    _ov52m = _m52m.Overlay()
    try:
        _ov52m.auftraege_zeigen([('X', 'Auftrag angenommen: Testauftrag')])
        _fehlt52m = []
        for _h52m in (190, 130, 110):
            _ov52m.root.geometry('660x%d' % _h52m)
            _ov52m.root.update_idletasks()
            if not _ov52m.grip.winfo_ismapped():
                _fehlt52m.append(_h52m)
        pruefe(not _fehlt52m,
               'der Griff bleibt auch im niedrigen Fenster sichtbar (%s)'
               % (_fehlt52m or 'alle Hoehen'))
        pruefe(_ov52m.grip.master is _ov52m.root,
               'er haengt am Fenster, nicht an der Liste')
        _ov52m.eingeklappt = True
        _ov52m._grip_nachziehen()
        _ov52m.root.update_idletasks()
        pruefe(not _ov52m.grip.winfo_ismapped(),
               'eingeklappt verschwindet er weiterhin')
    finally:
        _ov52m.root.destroy()

    # 52n. Abbauart im Lager und die neuen Filter
    print()
    print('52n. Abbauart und Herstellungs-Filter')
    from scbp import bergbau as _bg52n
    _echt52n = _bg52n.erze
    _bg52n.erze = lambda: [
        {'name': 'Iron (Ore)', 'orte': [('Daymar', 'Stanton', {'schiff'}),
                                        ('Yela', 'Stanton', {'schiff_selten'})]},
        {'name': 'Aphorite', 'orte': [('Daymar', 'Stanton', {'fps'})]},
    ]
    try:
        pruefe(_bg52n.abbauart('Iron') == {'schiff'},
               'Schiffsabbau wird erkannt — auch aus schiff_selten')
        pruefe(_bg52n.abbauart('Aphorite') == {'fps'},
               'Handabbau ebenso')
        pruefe(_bg52n.abbauart('Gibtsnicht') == set(),
               'ein unbekannter Rohstoff ergibt keine Art')
    finally:
        _bg52n.erze = _echt52n
    for _k52n in ('s_lg_sp_abbau', 's_lg_abbau_fps', 's_lg_abbau_fahrzeug',
                  's_lg_abbau_schiff', 's_lg_posten_weg', 's_lg_posten_frage',
                  's_lg_leeren', 's_lg_leeren_frage', 's_lg_geleert',
                  'ff_alle_material', 'ff_material_reicht', 'ff_material_fehlt'):
        _w52n = _sp51.TEXTE.get(_k52n)
        pruefe(bool(_w52n) and len(_w52n) == 2 and all(_w52n),
               'Text %s gibt es deutsch und englisch' % _k52n)
    # ⚠ Das Suchfeld im Lager erscheint nicht mehr erst ab fuenf Posten — wer
    # viel hat, findet sonst nichts mehr.
    with open(os.path.join(_wurzelpfad, 'scbp', 'seiten.py'),
              encoding='utf-8') as _fh52n:
        _qu52n = _fh52n.read()
    pruefe('if len(posten) > 5:' not in _qu52n,
           'das Suchfeld im Lager haengt nicht mehr an einer Postenzahl')

    # 52p. Eingabefelder ueberleben das Neuzeichnen
    #
    # Das Suchfeld im Lager stand IN der Zeichenfunktion, und die raeumt bei
    # jeder Aenderung den Listenbereich leer: Mit jedem getippten Buchstaben
    # zerstoerte sich das Feld selbst und der Cursor war weg — „im Lager bei
    # Eingabe im Suchfeld tabt man automatisch raus" (30.08.2026).
    print()
    print('52p. Suchfelder werden nicht beim Zeichnen neu gebaut')
    with open(os.path.join(_wurzelpfad, 'scbp', 'seiten.py'),
              encoding='utf-8') as _fh52p:
        _qu52p = _fh52p.read()
    # Der Lager-Abschnitt: zwischen `def _lager(` und der naechsten Seite.
    _von52p = _qu52p.index('def _lager(')
    _lager52p = _qu52p[_von52p:]
    _zeichnen52p = _lager52p[_lager52p.index('    def zeichnen():'):]
    # Bis zum Ende der Zeichenfunktion — der naechste Ausdruck auf gleicher
    # Ebene ist die Anmeldung des Filters.
    _zeichnen52p = _zeichnen52p.split('filter_var.trace_add')[0]
    pruefe('rundes_feld' not in _zeichnen52p,
           'im Lager baut die Zeichenfunktion kein Eingabefeld mehr')
    pruefe('_such_feld' in _lager52p,
           'das Suchfeld entsteht einmal, ausserhalb')

    # 52q. Die gemerkte Fenstergroesse ueberlebt den Start
    #
    # Die Mindestbreiten-Pruefung lief ueber `after_idle` — da meldet Tk fuer
    # ein noch nicht angezeigtes Fenster die Breite 1. Der Vergleich traf immer
    # zu, das Overlay wurde auf die Mindestbreite gesetzt, und die Groesse aus
    # dem letzten Lauf war weg: „er startet bei mir immer mit der kleinsten
    # Groesse" (30.08.2026).
    print()
    print('52q. Gemerkte Fenstergroesse bleibt erhalten')
    _m52q = _m52c            # dasselbe Modul wie in 52c
    _alt52q = _m52q.load_geometry()
    try:
        _m52q.save_geometry('900x400+150+120')
        _ov52q = _m52q.Overlay()
        try:
            _ov52q.root.update_idletasks()
            _ov52q.root.update()
            _ov52q.root.update_idletasks()
            pruefe(_ov52q.root.winfo_width() == 900,
                   'die gemerkte Breite bleibt (900, ist %d)'
                   % _ov52q.root.winfo_width())
            pruefe(_ov52q.root.winfo_height() == 400,
                   'die gemerkte Hoehe bleibt (400, ist %d)'
                   % _ov52q.root.winfo_height())
            # Zu schmal gemerkt? Dann greift die Grenze trotzdem — sobald das
            # Fenster wirklich steht.
            _ov52q.root.geometry('300x150')
            for _ in range(3):
                _ov52q.root.update_idletasks()
                _ov52q.root.update()
            _ov52q._mindestgroesse_setzen()
            _ov52q.root.update_idletasks()
            pruefe(_ov52q.root.winfo_width() >= _ov52q._mindestbreite(),
                   'ein zu schmales Fenster wird weiterhin angehoben')
        finally:
            _ov52q.root.destroy()
    finally:
        if _alt52q:
            _m52q.save_geometry(_alt52q)

    # 52r. Kein Entwicklername im CHANGELOG
    #
    # ⚠ Die Regel „jeden Fehlerfinder namentlich nennen" gilt fuer Tester von
    # aussen, nicht fuer den Entwickler selbst — es ist sein Projekt. Zweimal
    # aufgeraeumt, zweimal wieder hineingerutscht: Beim ersten Mal war nur nach
    # nur nach dem Pseudonym gesucht worden — die Stellen mit dem Klarnamen
    # blieben stehen. Diese Pruefung sucht nach dem Klarnamen, und zwar im
    # ganzen Projekt statt nur in zwei Dateien.
    print()
    print('52r. Kein Klarname im ganzen Projekt')
    import re as _re52r
    # ⚠ `Xharig` allein ist erlaubt: Copyright-Zeile, Repo-Adresse, der
    # Autoren-Block der README. Der **Klarname** ist es nie.
    _NAMEN52r = _re52r.compile(r'\bRoberts?\b')
    _alle52r = []
    # ⚠⚠ **Der Klarname gehoert NIRGENDS hin** — nicht in den CHANGELOG, nicht
    # in Kommentare, nicht in die Danksagung: „es geht niemanden was an, wie
    # ich heisse" (30.08.2026). Deshalb sucht diese Pruefung im ganzen Projekt,
    # nicht nur in zwei Dateien. Beim ersten Aufraeumen war nur der CHANGELOG
    # geprueft worden — im Quelltext standen danach noch dreizehn Stellen.
    for _datei52r in sorted(_versionierte_dateien(_wurzelpfad)):
        _pfad52r = os.path.join(_wurzelpfad, _datei52r)
        if not os.path.exists(_pfad52r):
            continue
        with open(_pfad52r, encoding='utf-8') as _fh52r:
            _text52r = _fh52r.read()
        _treffer52r = []
        for _nr52r, _zeile52r in enumerate(_text52r.splitlines(), 1):
            # ⚠ „Roberts Space Industries" ist der Hersteller im Spiel und
            # muss stehen bleiben.
            #
            # ⚠⚠ **Auch halbiert.** In langen Texten bricht der Name ueber
            # zwei Quelltextzeilen („… or Roberts Space " + "Industries."),
            # und dann greift die Ausnahme oben nicht mehr — die Pruefung
            # meldete einen Klarnamen, wo der Hersteller stand. Am 30.08.2026
            # passiert. Genau derselbe Fehler hat frueher schon einmal den
            # Herstellernamen in 174 Commits auseinandergerissen, weil jemand
            # den Fehlalarm „bereinigt" hat.
            _sauber52r = _zeile52r.replace('Roberts Space Industries', '')
            _sauber52r = _sauber52r.replace('Roberts Space', '')
            if _NAMEN52r.search(_sauber52r):
                _treffer52r.append('%s:%d %s' % (_datei52r, _nr52r,
                                                 _zeile52r.strip()[:60]))
        _alle52r.extend(_treffer52r)
    pruefe(not _alle52r,
           'kein Klarname im Projekt (%d Stellen)' % len(_alle52r))
    for _x52r in _alle52r[:6]:
        print('       ·', _x52r)

    # 52s. Keine privaten Angaben im Projekt
    #
    # ⚠⚠ Nicht nur der Klarname (52r). Auch alles andere, was aus dem
    # Arbeitsalltag stammt und niemanden etwas angeht: die persoenliche
    # Wissenssammlung und ihr Programm, Adressen im Heimnetz, Passwort- und
    # Dokumentenverwaltung, die eigene Spielorganisation, Wohnort, Arbeitgeber.
    # Am 30.08.2026 stand solches im CHANGELOG, in sechzehn Release-Texten und
    # als fester Pfad im Quelltext.
    #
    # ⚠ Die Begriffe stehen hier zusammengesetzt, damit diese Datei nicht
    # selbst als Treffer gilt.
    print()
    print('52s. Keine privaten Angaben im Projekt')
    _PRIVAT52s = [
        'obsid' + 'ian', 'va' + 'ult', 'keep' + 'ass', 'paper' + 'less',
        'xharig' + 'ds', '192.168.' + '178', 'fritz.' + 'box',
        'kirch' + 'hain', 'gar' + 'the', 'das kar' + 'tell',
        'staffel ma' + 'mba', 'pi-' + 'hole',
    ]
    # Was im Spiel wirklich so heisst, darf nicht anschlagen.
    _ERLAUBT52s = ('racing helmet obsid', 'helmetobsid')
    _funde52s = []
    for _rel52s in sorted(_versionierte_dateien(_wurzelpfad)):
        if _rel52s.endswith('selbsttest.py'):
            continue              # hier stehen die Suchbegriffe selbst
        if _rel52s.startswith('daten' + os.sep) or _rel52s.startswith('daten/'):
            continue              # Spieldaten — dort heisst ein Helm wirklich so
        _voll52s = os.path.join(_wurzelpfad, _rel52s)
        if not os.path.exists(_voll52s):
            continue
        with open(_voll52s, encoding='utf-8') as _fh52s:
            for _nr52s, _zeile52s in enumerate(_fh52s, 1):
                _klein52s = _zeile52s.lower()
                if any(_e in _klein52s for _e in _ERLAUBT52s):
                    continue
                for _b52s in _PRIVAT52s:
                    if _b52s in _klein52s:
                        _funde52s.append('%s:%d %s'
                                         % (_rel52s, _nr52s,
                                            _zeile52s.strip()[:60]))
                        break
    pruefe(not _funde52s,
           'keine privaten Angaben im Projekt (%d Stellen)' % len(_funde52s))
    for _x52s in _funde52s[:6]:
        print('       ·', _x52s)

    # 53. Lagerbestand berichtigen — und Namen, die wirklich passen
    #
    # Eintragen ohne Berichtigen war halb fertig: Wer sich vertippt oder
    # Material weitergegeben hatte, konnte den Posten nur loeschen und neu
    # tippen. Und beim Neutippen entstand leicht ein zweiter Name fuer
    # dasselbe Material — der Bestand sieht dann richtig aus, wird von den
    # Rezepten aber nicht mehr gefunden. Am 29.08.2026 gemeldet.
    print()
    print('53. Lagerbestand berichtigen und Namen abgleichen')
    from scbp import rohstoffe as _ro53
    from scbp import herstellung as _he53

    _alt53 = _ro53.laden()
    try:
        _ro53.sichern([])
        _ro53.eintragen('Aslarite', 10, 500, 'Zuhause')
        _ro53.eintragen('Quantainium', 4, 800, 'Schiff')

        pruefe(len(_ro53.laden()) == 2, 'zwei Posten liegen im Lager')

        # Menge berichtigen, alles andere behalten
        _ro53.aendern(0, 'Aslarite', 8, 500, 'Zuhause')
        _p53 = _ro53.laden()[0]
        pruefe(_p53.get('menge') == 8, 'die Menge laesst sich berichtigen')
        pruefe(_p53.get('qualitaet') == 500,
               'dabei bleibt die Qualitaet stehen')

        # Umlagern und Qualitaet nachtragen
        _ro53.aendern(1, 'Quantainium', 4, 950, 'Lagerhaus Area18')
        _p53b = _ro53.laden()[1]
        pruefe(_p53b.get('ort') == 'Lagerhaus Area18',
               'der Lagerort laesst sich aendern (umlagern)')
        pruefe(_p53b.get('qualitaet') == 950,
               'die Qualitaet laesst sich anpassen')

        # Der Nachbarposten bleibt unberuehrt
        pruefe(_ro53.laden()[0].get('material') == 'Aslarite',
               'die andere Zeile bleibt unangetastet')

        # Unsinnige Nummer aendert nichts
        pruefe(_ro53.aendern(99, 'Irgendwas', 1, 1, '') is False,
               'eine Nummer ausserhalb der Liste aendert nichts')
        pruefe(len(_ro53.laden()) == 2,
               'und legt auch keinen neuen Posten an')
    finally:
        _ro53.sichern(_alt53)

    # Mehrfach herstellen — einmal klicken statt zehnmal
    _ro53.sichern([{'material': 'Iron', 'menge': 10.0, 'qualitaet': 500,
                    'ort': ''}])
    _zut53 = [('Frame', 'Iron', 2.0, 0)]
    _ok53, _fehlt53 = _ro53.abziehen(_zut53, 3)
    pruefe(_ok53 and abs(_ro53.menge_von('Iron') - 4.0) < 0.001,
           'dreimal herstellen zieht dreimal die Zutaten ab (10 - 3x2 = 4)')
    _ro53.sichern([{'material': 'Iron', 'menge': 10.0, 'qualitaet': 500,
                    'ort': ''}])
    _ro53.abziehen(_zut53)
    pruefe(abs(_ro53.menge_von('Iron') - 8.0) < 0.001,
           'ohne Angabe bleibt es bei einem Stueck')

    # Ausgeben und wieder einlesen
    _probe53 = [{'material': 'Iron', 'menge': 1.36, 'qualitaet': 540,
                 'ort': 'Zuhause'},
                {'material': 'Riccite', 'menge': 2.91, 'qualitaet': 800,
                 'ort': ''}]
    _csv53 = _ro53.als_csv(_probe53)
    pruefe(_csv53.startswith('Material;Menge;Qualitaet;Lagerort'),
           'die Tabelle hat eine Kopfzeile')
    pruefe('1,36' in _csv53,
           'Mengen stehen mit Komma darin (deutsches Tabellenprogramm)')
    pruefe(_csv53.count(chr(10)) == 3, 'zwei Posten ergeben zwei Zeilen')
    _zurueck53 = _ro53.aus_json(_ro53.als_json(_probe53))
    pruefe(_zurueck53 == _probe53,
           'was ausgegeben wurde, kommt unveraendert zurueck')
    pruefe(_ro53.aus_json('kein json') is None,
           'Unsinn wird nicht eingelesen')
    pruefe(_ro53.aus_json('{"format": 99, "posten": []}') is None,
           'und ein fremdes Format auch nicht')

    # Komma und Punkt gelten gleich — die einen tippen 12,5, die anderen 12.5
    pruefe(_ro53.zahl_lesen('12,5') == 12.5, 'ein Komma wird als Zahl gelesen')
    pruefe(_ro53.zahl_lesen('12.5') == 12.5, 'ein Punkt genauso')
    pruefe(_ro53.zahl_lesen(' 8 ') == 8.0, 'Leerzeichen stoeren nicht')
    pruefe(_ro53.zahl_lesen('-2,5') == -2.5, 'ein Minus bleibt erhalten')
    pruefe(_ro53.zahl_lesen('-2') == -2.0,
           'auch das lange Minus vom Ziffernblock')
    pruefe(_ro53.zahl_lesen('12 SCU') is None,
           'was keine Zahl ist, gibt None statt eines Absturzes')
    pruefe(_ro53.zahl_lesen('') is None, 'und ein leeres Feld ebenso')

    # Namensabgleich — der Schluessel zwischen Lager und Rezept.
    # ⚠ Mit eingespeister Namensliste pruefen. Im Wegwerf-Ordner gibt es keine
    # Rezeptdaten; ohne diesen Griff pruefte man nur, dass nichts geladen ist.
    _echt53 = _he53.rohstoffnamen
    _he53.rohstoffnamen = lambda: ['Aslarite', 'Quantainium', 'Aluminum',
                                   'Agricium', 'Titanium']
    pruefe(_he53.offizieller_name('aslarite') == 'Aslarite',
           'Kleinschreibung wird auf den richtigen Namen gezogen')
    pruefe(_he53.offizieller_name('  ASLARITE  ') == 'Aslarite',
           'Grossschreibung und Leerzeichen stoeren nicht')
    pruefe(_he53.offizieller_name('Aslarite (Raw)') == 'Aslarite',
           'die Bergbau-Schreibweise mit Klammer passt auch')
    pruefe(_he53.offizieller_name('aslerite') == 'Aslarite',
           'ein knapper Vertipper wird berichtigt')
    pruefe(_he53.offizieller_name('Bratkartoffeln') is None,
           'ein voellig fremder Name wird NICHT geraten')
    pruefe(_he53.offizieller_name('') is None,
           'und eine leere Eingabe ergibt nichts')
    pruefe(_he53.offizieller_name('Aluminium') == 'Aluminum',
           'die britische Schreibweise trifft die amerikanische')
    # ⚠ Ohne geladene Rezeptdaten darf NICHTS abgewiesen werden — sonst kann
    # beim ersten Start ohne Netz niemand sein Lager fuellen.
    _he53.rohstoffnamen = lambda: []
    pruefe(_he53.offizieller_name('Irgendwas') == 'Irgendwas',
           'ohne Rezeptdaten wird die Eingabe durchgelassen')
    _he53.rohstoffnamen = _echt53
    for _k53 in ('s_lg_speichern', 's_lg_abbrechen', 's_lg_geaendert',
                 's_lg_rechnen', 's_lg_zu_wenig', 's_lg_alles_weg',
                 's_lg_name_fremd', 's_lg_keine_guete',
                 's_lg_berichtigt', 's_lg_zeile_klick', 's_lg_bearbeite'):
        _w53 = _sp51.TEXTE.get(_k53)
        pruefe(bool(_w53) and len(_w53) == 2 and all(_w53),
               'Text %s gibt es deutsch und englisch' % _k53)
    # Der Lagerort heisst Lagerort — „Fundort" gehoert zum Bergbau und hat
    # hier jemanden ratlos gemacht.
    pruefe('Lagerort' in _sp51.TEXTE['s_lg_ort'][0],
           'das Ortsfeld heisst Lagerort, nicht Fundort')
    pruefe('freiwillig' not in _sp51.TEXTE['s_lg_qualitaet'][0],
           'die Qualitaet ist nicht mehr als freiwillig ausgewiesen')

    # ------------------------------------------------------------------
    # 58. Ein abgeschlossener Auftrag darf nicht als frisch angenommen gelten
    #
    # Am 30.08.2026 gemeldet: „Retake Platforms From Nine Tails" um 01:18
    # angenommen, um 01:59 abgeschlossen — der Watcher um 02:22 gestartet und
    # der Auftrag stand als laufend da. Zwei Ursachen, beide hier geprueft:
    #
    #   a) `new_names()` stieg aus, wenn nichts Neues in der Log stand — ohne
    #      die Auftragslisten des VORIGEN Abschnitts zu leeren. Der Aufrufer
    #      wertete sie ein zweites Mal aus.
    #   b) Die Auswertung nahm erst alle Enden und dann alle Annahmen. In einem
    #      Abschnitt, der beides enthaelt (jeder Neustart bei laufendem Spiel
    #      liest so etwas nach), traf das Ende ins Leere und die Annahme stellte
    #      den Auftrag danach wieder hin.
    print()
    print('58. Abgeschlossener Auftrag bleibt abgeschlossen')
    from scbp import logquelle as _lq58
    from scbp import auftraege as _au58
    from scbp import pfade as _pf58

    _log58 = os.path.join(tempfile.mkdtemp(), 'Game.log')
    with open(_log58, 'w', encoding='utf-8') as _f58:
        _f58.write('Added notification "Auftrag angenommen: Retake Platforms: " ...\n'
                   'irgendwas dazwischen\n'
                   'Added notification "Auftrag abgeschlossen: Retake Platforms: " ...\n')

    class _Stand58:
        def __init__(_s): _s.o = 0
        def aktiv_holen(_s, _p): return 0
        def aktiv_setzen(_s, _p, _o): _s.o = _o
        def speichern(_s): pass

    _echt58 = _pf58.game_log
    try:
        _pf58.game_log = lambda: _log58
        _t58 = _lq58.LogTail(_Stand58())
        _t58.auftrag_muster = _au58.muster()
        _t58.auftrag_ende_muster = _au58.ende_muster()

        _t58.new_names()
        pruefe(len(_t58.auftraege) == 1 and len(_t58.auftraege_beendet) == 1,
               'der erste Abschnitt bringt Annahme und Ende')

        # a) Zweiter Aufruf, nichts Neues in der Datei: die Listen MUESSEN leer
        #    sein. Bis v3.3.0-rc33 standen sie noch voll da.
        _t58.new_names()
        pruefe(_t58.auftraege == [] and _t58.auftraege_beendet == []
               and _t58.auftrag_ereignisse == [],
               'ohne neuen Text bleiben die Auftragslisten LEER')

        # b) Die Reihenfolge muss stimmen: Annahme, dann Ende.
        with open(_log58, 'a', encoding='utf-8') as _f58:
            _f58.write('Added notification "Auftrag angenommen: Zweiter Job: " ...\n'
                       'Added notification "Auftrag abgeschlossen: Zweiter Job: " ...\n')
        _t58.new_names()
        pruefe([e[0] for e in _t58.auftrag_ereignisse] == [True, False],
               'die Ereignisse kommen in der Reihenfolge des Logs')
        pruefe([e[1] for e in _t58.auftrag_ereignisse] == ['Zweiter Job', 'Zweiter Job'],
               'und tragen beide denselben Titel')

        # c) Und die Gesamtrechnung ueber die ganze Datei: nichts offen.
        _text58 = open(_log58, encoding='utf-8').read()
        pruefe(_au58.offene_aus_text(_text58, _t58.auftrag_muster,
                                     _t58.auftrag_ende_muster) == [],
               'ueber die ganze Log gerechnet ist KEIN Auftrag mehr offen')
    finally:
        _pf58.game_log = _echt58

    # d) Die Auswertung im Hauptprogramm muss der Reihenfolge folgen und darf
    #    nicht mehr auf die beiden getrennten Listen zurueckgreifen.
    _quelle58 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'),
                     encoding='utf-8').read()
    _ab58 = _quelle58.split('def _auftraege_melden')[1].split('def _emit')[0]
    pruefe('auftrag_ereignisse' in _ab58,
           'die Auswertung geht ueber die geordneten Ereignisse')
    pruefe('for titel in beendet:' not in _ab58,
           'und NICHT mehr erst ueber alle Enden')

    # e) Die Zeile in der Liste gehoert zum Auftrag — sonst bleibt sie stehen,
    #    wenn er endet, und traegt kein Zeichen zum Wegnehmen.
    pruefe("('hinweis', zeile, rein)" in _quelle58,
           'die Hinweiszeile bekommt den Auftrag mit')
    pruefe('def hinweis_entfernen' in _quelle58,
           'es gibt einen Weg, die Zeile wieder herauszunehmen')
    pruefe("'auftrag_weg'" in _quelle58,
           'und eine Meldung, die das ausloest')
    _ah58 = _quelle58.split('def add_hinweis')[1].split('def add_catalog')[0]
    pruefe("zeichen.zeile(row, 'ausblenden'" in _ah58,
           'die Zeile traegt dasselbe festgelegte Zeichen wie die Auftragsleiste')

    # ------------------------------------------------------------------
    # 59. Eine aufgeklappte Auswahlliste bleibt ueberschaubar
    #
    # Am 30.08.2026 gemeldet: Die Ortsliste im Bergbau (48 Eintraege) reichte
    # vom Auswahlfeld bis weit unter das Fenster ins Bild hinein. Die Hoehe war
    # bis dahin nur nach dem *Platz* begrenzt — und auf einem grossen Bildschirm
    # ist der riesig. Jetzt gilt zusaetzlich eine feste Zeilenzahl; alles
    # darueber wird gerollt, und die Rollleiste zeigt, dass mehr kommt.
    print()
    print('59. Aufgeklappte Auswahlliste bleibt ueberschaubar')
    import tkinter as _tk59
    from scbp import hauptfenster as _hf59

    pruefe(getattr(_hf59, 'MAX_WAHLZEILEN', 0) >= 8,
           'es gibt eine Obergrenze fuer die Zeilenzahl (%s)'
           % getattr(_hf59, 'MAX_WAHLZEILEN', '—'))

    _w59 = _tk59.Tk()
    try:
        _w59.geometry('1200x1130+0+0')
        _w59.update_idletasks()
        _hoehen59 = {}
        for _n59 in (5, _hf59.MAX_WAHLZEILEN, 48):
            _ein59 = ([('', 'Alle Orte')] +
                      [('o%d' % _i59, 'Ort Nummer %d' % _i59)
                       for _i59 in range(_n59 - 1)])
            _f59 = _hf59.rundwahl(_w59, _ein59, '', lambda _v: None,
                                  ('TkDefaultFont', 10))
            _f59.pack()
            _w59.update_idletasks()
            _f59.event_generate('<Button-1>', x=5, y=5)
            _w59.update_idletasks()
            _auf59 = [k for k in _f59.winfo_children()
                      if isinstance(k, _tk59.Toplevel)]
            _hoehen59[_n59] = (int(_auf59[0].wm_geometry().split('x')[1].split('+')[0])
                               if _auf59 else 0)
            for _tl59 in _auf59:
                _tl59.destroy()
            _f59.destroy()

        pruefe(_hoehen59[5] > 0, 'eine kurze Liste klappt auf')
        pruefe(_hoehen59[48] <= _hoehen59[_hf59.MAX_WAHLZEILEN],
               'eine lange Liste wird NICHT hoeher als die Obergrenze '
               '(48 Eintraege: %d px, Grenze: %d px)'
               % (_hoehen59[48], _hoehen59[_hf59.MAX_WAHLZEILEN]))
        pruefe(_hoehen59[48] < 1090,
               'und bleibt deutlich unter der Fensterhoehe (%d px)'
               % _hoehen59[48])
        pruefe(_hoehen59[5] < _hoehen59[48],
               'eine kurze Liste wird trotzdem nicht kuenstlich aufgeblaeht')

        # ⚠ Und die harte Grenze: NIE hoeher als das kleinstmoegliche Fenster.
        # Sonst passt die Liste nach dem Verkleinern nicht mehr hinein und
        # waere unten abgeschnitten. Geprueft bei GROSSEM Fenster — genau da
        # war die alte Rechnung grosszuegig.
        _w59.geometry('1600x1400')
        _w59.update_idletasks()
        _ein59 = [('', 'Alle')] + [('o%d' % _i59, 'Eintrag %d' % _i59)
                                   for _i59 in range(199)]
        _f59 = _hf59.rundwahl(_w59, _ein59, '', lambda _v: None,
                              ('TkDefaultFont', 10))
        _f59.pack()
        _w59.update_idletasks()
        _f59.event_generate('<Button-1>', x=5, y=5)
        _w59.update_idletasks()
        _auf59 = [k for k in _f59.winfo_children()
                  if isinstance(k, _tk59.Toplevel)]
        _hoch59 = (int(_auf59[0].wm_geometry().split('x')[1].split('+')[0])
                   if _auf59 else 0)
        pruefe(0 < _hoch59 <= _hf59.MIN_HOEHE,
               'auch bei 200 Eintraegen und grossem Fenster nie hoeher als das '
               'kleinstmoegliche Fenster (%d px, Grenze %d px)'
               % (_hoch59, _hf59.MIN_HOEHE))
        for _tl59 in _auf59:
            _tl59.destroy()
        _f59.destroy()

        # ⚠ Und der Fall, der den Fehler ueberhaupt sichtbar gemacht hat:
        # Fenster buendig am unteren Bildschirmrand. Dann ist unter dem Feld
        # kein Platz — die Liste MUSS nach oben aufklappen, sonst laege sie
        # unter dem Bildrand und waere abgeschnitten.
        _schirm59 = _w59.winfo_screenheight()
        _w59.geometry('1200x760+0+%d' % max(0, _schirm59 - 762))
        _w59.update_idletasks()
        _unten59 = _tk59.Frame(_w59)
        _unten59.pack(side='bottom', fill='x')
        _ein59 = [('', 'Alle')] + [('o%d' % _i59, 'Ort %d' % _i59)
                                   for _i59 in range(47)]
        _f59 = _hf59.rundwahl(_unten59, _ein59, '', lambda _v: None,
                              ('TkDefaultFont', 10))
        _f59.pack()
        _w59.update_idletasks()
        _f59.event_generate('<Button-1>', x=5, y=5)
        _w59.update_idletasks()
        _auf59 = [k for k in _f59.winfo_children()
                  if isinstance(k, _tk59.Toplevel)]
        if _auf59:
            _g59 = _auf59[0].wm_geometry().split('+')
            _bh59 = int(_g59[0].split('x')[1])
            _oben59 = int(_g59[2])
            pruefe(_oben59 < _f59.winfo_rooty(),
                   'am unteren Bildrand klappt die Liste nach OBEN auf')
            pruefe(_oben59 + _bh59 <= _f59.winfo_rooty() + 4,
                   'und endet ueber dem Feld statt unter dem Bildrand '
                   '(y=%d bis %d, Feld bei %d)'
                   % (_oben59, _oben59 + _bh59, _f59.winfo_rooty()))
            for _tl59 in _auf59:
                _tl59.destroy()
        _f59.destroy()
        _unten59.destroy()
    finally:
        _w59.destroy()

    # ------------------------------------------------------------------
    # 60. Das Mausrad rollt die aufgeklappte Liste — nicht die Seite dahinter
    #
    # Am 30.08.2026 gemeldet: „das dropdown laesst sich NICHT scrollen … wenn
    # man so wie jeder user es versucht zu scrollen, scrollt das fenster
    # dahinter und man kann die abgeschnittenen daten NICHT erreichen."
    #
    # Ursache: `rad_anschliessen` haengt global am Programm und sucht die
    # Rollflaeche, indem es vom Element unter dem Zeiger durch die Elternkette
    # nach oben geht. Die aufgeklappte Liste ist ein eigenes Fenster, ihr
    # Elternteil ist aber das Auswahlfeld — und das steht mitten in der
    # rollbaren Seite. Die Kette lief also aus der Liste heraus in die Seite
    # dahinter. Die rollte weg, das Feld wanderte mit, die Liste klappte zu.
    #
    # ⚠ Der Aufbau hier muss das nachstellen: Das Feld MUSS in der Rollflaeche
    # stecken. Ein Feld daneben zeigt den Fehler nicht — daran ist die erste
    # Messung vorbeigelaufen.
    print()
    print('60. Mausrad rollt die Klappliste, nicht die Seite dahinter')
    import tkinter as _tk60
    from scbp import hauptfenster as _hf60

    _w60 = _tk60.Tk()
    try:
        _w60.geometry('1200x1000+0+0')
        _seite60 = _tk60.Canvas(_w60, height=600)
        _seite60.pack(fill='both', expand=True)
        _inhalt60 = _tk60.Frame(_seite60)
        _seite60.create_window((0, 0), window=_inhalt60, anchor='nw')

        _ein60 = [('', 'Alle')] + [('h%d' % _i60, 'Eintrag %d' % _i60)
                                   for _i60 in range(47)]
        _feld60 = _hf60.rundwahl(_inhalt60, _ein60, '', lambda _v: None,
                                 ('TkDefaultFont', 10))
        _feld60.pack()
        for _i60 in range(200):
            _tk60.Label(_inhalt60, text='Zeile %d' % _i60).pack()
        _w60.update_idletasks()
        _seite60.configure(scrollregion=(0, 0, 400, _inhalt60.winfo_reqheight()))
        _hf60.rad_anschliessen(_seite60)

        _w60.update_idletasks()
        _feld60.event_generate('<Button-1>', x=5, y=5)
        _w60.update_idletasks()
        _auf60 = [k for k in _feld60.winfo_children()
                  if isinstance(k, _tk60.Toplevel)]
        pruefe(bool(_auf60), 'die Liste klappt auf')

        if _auf60:
            _auf60[0].update_idletasks()

            def _rollflaeche60(w):
                if isinstance(w, _tk60.Canvas) and w.winfo_width() > 20:
                    return w
                for _k in w.winfo_children():
                    _t = _rollflaeche60(_k)
                    if _t is not None:
                        return _t
                return None

            def _etiketten60(w, sammlung):
                if isinstance(w, _tk60.Label):
                    sammlung.append(w)
                for _k in w.winfo_children():
                    _etiketten60(_k, sammlung)
                return sammlung

            _liste60 = _rollflaeche60(_auf60[0])
            _zeilen60 = _etiketten60(_auf60[0], [])
            _ziel60 = _zeilen60[3]

            # ⚠ **Das Rad heisst auf jedem System anders.** Linux meldet es
            # als Maustaste 4/5, Windows und macOS als `<MouseWheel>` mit einem
            # Ausschlag — unter Windows ±120, auf dem Mac ±1. Ein Test, der nur
            # `<Button-5>` schickt, faellt unter Windows durch, obwohl das
            # Programm dort in Ordnung ist. Genau so am 30.08.2026 im Bau-Lauf
            # passiert: Linux gruen, Windows rot.
            def _radeln60(male):
                for _ in range(male):
                    if sys.platform.startswith('linux'):
                        _ziel60.event_generate(
                            '<Button-5>', x=5, y=5,
                            rootx=_ziel60.winfo_rootx() + 5,
                            rooty=_ziel60.winfo_rooty() + 5)
                    else:
                        _ziel60.event_generate(
                            '<MouseWheel>',
                            delta=(-1 if sys.platform == 'darwin' else -120),
                            x=5, y=5,
                            rootx=_ziel60.winfo_rootx() + 5,
                            rooty=_ziel60.winfo_rooty() + 5)
                _w60.update_idletasks()

            _vl60, _vs60 = _liste60.yview(), _seite60.yview()
            _radeln60(5)
            _nl60, _ns60 = _liste60.yview(), _seite60.yview()

            pruefe(_nl60[0] > _vl60[0],
                   'das Rad rollt die Klappliste (%.3f -> %.3f)'
                   % (_vl60[0], _nl60[0]))
            pruefe(abs(_ns60[0] - _vs60[0]) < 1e-6,
                   'und die Seite dahinter bleibt stehen (%.3f -> %.3f)'
                   % (_vs60[0], _ns60[0]))

            _radeln60(80)
            pruefe(_liste60.yview()[1] > 0.999,
                   'der letzte Eintrag ist erreichbar (Ende bei %.3f)'
                   % _liste60.yview()[1])
            for _tl60 in _auf60:
                _tl60.destroy()
    finally:
        _w60.destroy()

    # Und: Jedes Auswahlfeld im Programm muss ueber `rundwahl` laufen — nur
    # dort steckt die Rad-Behandlung. Ein selbstgebautes `OptionMenu` oder eine
    # `ttk.Combobox` haette den Fehler sofort wieder.
    _fremde60 = []
    for _p60 in _versionierte_dateien(WURZEL, ('.py',)):
        if _p60.endswith('selbsttest.py'):
            continue
        _q60 = open(_p60, encoding='utf-8', errors='ignore').read()
        for _muster60 in ('OptionMenu(', 'Combobox('):
            if _muster60 in _q60:
                _fremde60.append('%s: %s' % (os.path.relpath(_p60, WURZEL),
                                             _muster60))
    pruefe(not _fremde60,
           'kein Auswahlfeld am Hausstil vorbei (%d gefunden)' % len(_fremde60))
    for _x60 in _fremde60[:5]:
        print('       ·', _x60)

    # ------------------------------------------------------------------
    # 61. Stueckzahl, Abzug und die Grenze des Lagers
    #
    # Am 30.08.2026 gemeldet, drei Fragen auf einmal:
    #   „10 als Menge eingegeben sollte auch 10fache Menge an benoetigtem
    #    Material sein, angezeigt wird es nicht — wuerde es ueberhaupt richtig
    #    abgezogen? Kann der Bestand im Lager ins Minus gehen? (Darf er nicht,
    #    wenn was fehlt ist es ja nicht herstellbar.)"
    #
    # Der Abzug rechnete richtig, die Anzeige nicht. Ins Minus konnte der
    # Bestand nie geraten — aber er wurde LEERGERAEUMT, wenn etwas fehlte.
    print()
    print('61. Stueckzahl, Abzug und die Grenze des Lagers')
    from scbp import rohstoffe as _ro61

    _sichern61 = _ro61.laden()
    try:
        _zut61 = [('Frame', 'Iron', 1.16, 0), ('Cycler', 'Riccite', 0.17, 0)]

        # a) Die ANZEIGE muss die Stueckzahl mitrechnen. Genau das fehlte.
        _ro61.sichern([])
        _eins61 = {m: br for m, br, _da, _f, _zg, _mq
                   in _ro61.pruefen(_zut61, 1)}
        _zehn61 = {m: br for m, br, _da, _f, _zg, _mq
                   in _ro61.pruefen(_zut61, 10)}
        pruefe(abs(_eins61['Iron'] - 1.16) < 1e-6,
               'ein Stueck braucht 1,16 Iron')
        pruefe(abs(_zehn61['Iron'] - 11.6) < 1e-6,
               'zehn Stueck brauchen das Zehnfache (%.2f)' % _zehn61['Iron'])
        _fehl61 = {m: f for m, _br, _da, f, _zg, _mq
                   in _ro61.pruefen(_zut61, 10)}
        pruefe(abs(_fehl61['Iron'] - 11.6) < 1e-6,
               'und bei leerem Lager fehlt auch das Zehnfache')

        # b) Der ABZUG rechnet die Stueckzahl mit — das war schon richtig.
        _ro61.sichern([{'material': 'Iron', 'menge': 20.0, 'qualitaet': 500,
                        'ort': ''},
                       {'material': 'Riccite', 'menge': 5.0, 'qualitaet': 500,
                        'ort': ''}])
        _ok61, _weg61 = _ro61.abziehen(_zut61, 10)
        pruefe(_ok61, 'zehn Stueck lassen sich abziehen, wenn genug da ist')
        pruefe(abs(_ro61.menge_von('Iron') - 8.4) < 1e-6,
               '20 - 10x1,16 = 8,40 Iron bleiben (%.2f)'
               % _ro61.menge_von('Iron'))

        # c) ⚠⚠ Reicht es NICHT, wird GAR NICHTS genommen.
        _ro61.sichern([{'material': 'Iron', 'menge': 3.0, 'qualitaet': 500,
                        'ort': ''},
                       {'material': 'Riccite', 'menge': 5.0, 'qualitaet': 500,
                        'ort': ''}])
        _ok61, _weg61 = _ro61.abziehen(_zut61, 10)
        pruefe(not _ok61, 'zehn Stueck aus zu wenig Material gehen NICHT')
        pruefe(abs(_ro61.menge_von('Iron') - 3.0) < 1e-6,
               'das Iron bleibt UNANGETASTET im Lager (%.2f statt 0)'
               % _ro61.menge_von('Iron'))
        pruefe(abs(_ro61.menge_von('Riccite') - 5.0) < 1e-6,
               'und das Riccite auch — kein halber Abzug (%.2f)'
               % _ro61.menge_von('Riccite'))
        pruefe(any(n == 'Iron' and abs(f - 8.6) < 1e-6 for n, f in _weg61),
               'gemeldet wird die FEHLMENGE, nicht nur der Name (%s)'
               % (_weg61,))

        # d) Und nie ins Minus — auch nicht bei einer unsinnigen Stueckzahl.
        _ro61.sichern([{'material': 'Iron', 'menge': 3.0, 'qualitaet': 500,
                        'ort': ''}])
        _ro61.abziehen([('Frame', 'Iron', 1.0, 0)], 9999)
        pruefe(_ro61.menge_von('Iron') >= 0,
               'der Bestand kann nicht negativ werden (%.2f)'
               % _ro61.menge_von('Iron'))
        pruefe(abs(_ro61.menge_von('Iron') - 3.0) < 1e-6,
               'und bleibt bei 9999 Stueck unberuehrt stehen')

        # e) Zutat zweimal im Rezept: die Summe zaehlt, nicht jede fuer sich.
        _ro61.sichern([{'material': 'Iron', 'menge': 3.0, 'qualitaet': 500,
                        'ort': ''}])
        _ok61, _weg61 = _ro61.abziehen(
            [('A', 'Iron', 2.0, 0), ('B', 'Iron', 2.0, 0)], 1)
        pruefe(not _ok61,
               'zweimal 2 aus 3 im Lager geht nicht — die Summe zaehlt')
        pruefe(abs(_ro61.menge_von('Iron') - 3.0) < 1e-6,
               'und auch hier bleibt alles liegen')
    finally:
        _ro61.sichern(_sichern61)

    # f) Die Oberflaeche muss die Stueckzahl wirklich durchreichen — und je
    #    Material einen eigenen Regler bauen.
    _seiten61 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                     encoding='utf-8').read()
    pruefe('lager.pruefen(stufe[\'zutaten\'], wie_viele)' in _seiten61,
           'die Zutatenliste rechnet mit der eingegebenen Stueckzahl')
    pruefe("anzahl_var.trace_add('write', mengen_setzen)" in _seiten61,
           'und rechnet sofort neu, wenn man die Zahl aendert')
    pruefe('def mengen_setzen' in _seiten61
           and 'neu_zeichnen()' not in _seiten61.split('def mengen_setzen')[1]
           .split('anzahl_var.trace_add')[0],
           'ohne die Seite neu zu bauen (sonst verliert das Feld den Cursor)')
    # ⚠ **Nicht auf die ersten 1500 Zeichen begrenzen.** Genau daran ist die
    # Pruefung am 30.08.2026 gescheitert: Ein Einschub zwischen Kommentar und
    # Schleife schob die gesuchte Zeile aus dem Fenster, und der Test meldete
    # einen Fehler, den es nicht gab. Ein Suchfenster mit fester Groesse ist
    # eine Wette darauf, dass niemand mehr etwas dazwischenschreibt.
    _regler61 = _seiten61.split('Ein Regler je Material')[1]
    pruefe('for _mat in alle_materialien' in _regler61,
           'es gibt einen Regler JE MATERIAL, nicht einen fuer alle')

    # ------------------------------------------------------------------
    # 62. Nicht jede Eigenschaft wird durch eine hoehere Zahl besser
    #
    # Am 30.08.2026 gemeldet: „ist es realistisch das sich bei niedrigerer
    # Qualitaet die Werte erhoehen? Und bei besserer Qualitaet die Werte
    # verschlechtern?"
    #
    # Die Daten sind in Ordnung, die Anzeige war es nicht. Bei 852 der 6524
    # Modifikatoren (Spielstand 4.10.0) SINKT der Faktor mit steigender
    # Qualitaet — Rueckstoss, Quantum-Treibstoff — und genau das ist dort die
    # Verbesserung. Die Anzeige faerbte stur „>= 1 ist gut": Der bestmoegliche
    # Rueckstoss (x 0.800) stand in der Warnfarbe, der schlechteste (x 1.200)
    # in Gruen.
    print()
    print('62. Richtung der Qualitaetswirkung')
    from scbp import herstellung as _he62

    # a) Die Richtung kommt aus dem Modifikator, nicht aus dem Namen.
    _hoch62 = [{'startQuality': 0, 'endQuality': 1000,
                'modifierAtStart': 0.925, 'modifierAtEnd': 1.075}]
    _runter62 = [{'startQuality': 0, 'endQuality': 1000,
                  'modifierAtStart': 1.2, 'modifierAtEnd': 0.8}]
    pruefe(_he62.besser_ist_hoch(_hoch62) is True,
           'steigt der Faktor mit der Qualitaet, ist hoeher besser')
    pruefe(_he62.besser_ist_hoch(_runter62) is False,
           'faellt er, ist NIEDRIGER besser (Rueckstoss, Treibstoff)')

    # b) Mehrteilige Spannen beschreiben EINE Kurve — Anfang gegen Ende.
    #    Ein flaches Teilstueck in der Mitte darf die Richtung nicht drehen.
    _geteilt62 = [{'startQuality': 501, 'endQuality': 1000,
                   'modifierAtStart': 1.0, 'modifierAtEnd': 0.8},
                  {'startQuality': 0, 'endQuality': 500,
                   'modifierAtStart': 1.2, 'modifierAtEnd': 1.0}]
    pruefe(_he62.besser_ist_hoch(_geteilt62) is False,
           'ueber mehrere Spannen zaehlt die Gesamtrichtung (1,2 -> 0,8)')
    pruefe(_he62.besser_ist_hoch([]) is True,
           'ohne Modifikator wird nichts behauptet (Vorgabe: hoeher ist besser)')

    # c) Und die Probe aufs Ganze an echten Daten, wenn welche da sind:
    #    Bei Qualitaet 0 muss JEDER Wert schlecht sein, bei 1000 JEDER gut.
    #    Ein Rezept, bei dem das nicht gilt, waere ein Widerspruch.
    def _gut62(w):
        return (w['faktor'] >= 1 if w.get('besser_hoch', True)
                else w['faktor'] <= 1)

    _daten62 = _he62.laden().get('blueprints') or []
    if _daten62:
        _schlecht62 = _falsch62 = 0
        _geprueft62 = 0
        for _b62 in _daten62[:400]:
            _name62 = _b62.get('productName')
            _mats62 = {}
            for _t62 in _b62.get('tiers') or []:
                for _s62 in _t62.get('slots') or []:
                    for _o62 in _s62.get('options') or []:
                        if _o62.get('resourceName'):
                            _mats62[_o62['resourceName']] = 0
            if not _mats62:
                continue
            _unten62 = _he62.werte_mit_lager(
                _name62, {m: 0 for m in _mats62})
            _oben62 = _he62.werte_mit_lager(
                _name62, {m: 1000 for m in _mats62})
            if not _unten62:
                continue
            _geprueft62 += 1
            for _w62 in _unten62:
                if _gut62(_w62):
                    _schlecht62 += 1
            for _w62 in _oben62:
                if not _gut62(_w62):
                    _falsch62 += 1
        pruefe(_geprueft62 > 0,
               'es liessen sich %d Bauplaene durchrechnen' % _geprueft62)
        pruefe(_schlecht62 == 0,
               'bei Qualitaet 0 gilt KEIN Wert als gut (%d Ausreisser)'
               % _schlecht62)
        pruefe(_falsch62 == 0,
               'bei Qualitaet 1000 gilt JEDER Wert als gut (%d Ausreisser)'
               % _falsch62)
    else:
        print('  [–]    keine Rezeptdaten vorhanden — uebersprungen')

    # d) ⚠ Nicht jede Wirkung ist ueberhaupt ein Multiplikator.
    #    „Power Pips" (itemresource_powergeneration) fuehrt Werte von -3 bis
    #    +3 in festen Qualitaetsstufen — Stueckzahlen. Als Faktor gelesen stand
    #    dort „× -1.000", ein Multiplikator, den es nicht geben kann. 598 der
    #    6524 Modifikatoren im Spielstand 4.10.0 sind so gebaut, das betrifft
    #    saemtliche Kraftwerke.
    pruefe(_he62.ist_absolut([{'modifierAtStart': -1.0,
                               'modifierAtEnd': -1.0}]) is True,
           'ein negativer Wert kann kein Multiplikator sein')
    pruefe(_he62.ist_absolut([{'modifierAtStart': 0.0,
                               'modifierAtEnd': 0.0}]) is True,
           'eine Null auch nicht (sie wuerde den Wert ausloeschen)')
    pruefe(_he62.ist_absolut([{'modifierAtStart': 0.925,
                               'modifierAtEnd': 1.075}]) is False,
           'ein Wert um 1 herum dagegen schon')
    pruefe(_he62.ist_absolut([]) is False,
           'ohne Angaben wird nichts behauptet')

    # e) Die Anzeige muss die Richtung auch benutzen.
    _seiten62 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                     encoding='utf-8').read()
    pruefe("w.get('besser_hoch', True)" in _seiten62,
           'die Anzeige faerbt nach der Richtung, nicht stur nach der Zahl')
    pruefe("fg=(ACCENT if w['faktor'] >= 1 else GOLD)" not in _seiten62,
           'die alte Regel „groesser als 1 ist gut" steht nicht mehr da')
    pruefe("w.get('absolut')" in _seiten62,
           'und unterscheidet Stueckzahl von Multiplikator')
    from scbp import sprache as _sp62
    for _k62 in ('s_he_weniger_gut', 's_he_absolut', 's_he_absolut_null'):
        _w62 = _sp62.TEXTE.get(_k62)
        pruefe(bool(_w62) and len(_w62) == 2 and all(_w62),
               'Text %s gibt es deutsch und englisch' % _k62)

    # ------------------------------------------------------------------
    # 63. Raffinerien — wohin mit dem Erz?
    #
    # Die Bergbau-Seite beantwortete nur die halbe Frage. Zwanzig Raffinerien
    # teilen sich zehn Profile, und der Unterschied ist kein Rundungsfehler:
    # Bei Bexalite liegen 18 Prozentpunkte zwischen bester und schlechtester
    # Wahl. Die Daten standen die ganze Zeit im selben Abruf — der Watcher hat
    # sie beim Sichern weggeworfen.
    print()
    print('63. Raffinerien')
    from scbp import bergbau as _bg63

    _daten63 = _bg63.laden()
    if not _daten63.get('refineryProfiles'):
        print('  [–]    keine Raffineriedaten vorhanden — uebersprungen')
    else:
        # Gegen die Tabelle auf scmdb.net gerechnet (Stand 4.10.0):
        _soll63 = {'Quartz': ('ARC-L1', 11), 'Titanium': ('MIC-L5', 13),
                   'Bexalite': ('MIC-L5', 12)}
        for _erz63, (_beste63, _bonus63) in _soll63.items():
            _r63 = _bg63.raffinerien_fuer(_erz63)
            pruefe(bool(_r63), '%s findet Raffinerien' % _erz63)
            if _r63:
                _namen63, _sys63, _wert63 = _r63[0]
                pruefe(_wert63 == _bonus63,
                       '%s: bester Bonus %+d %% (erwartet %+d)'
                       % (_erz63, _wert63, _bonus63))
                pruefe(any(n.startswith(_beste63) for n in _namen63),
                       '%s: beste Raffinerie ist %s' % (_erz63, _beste63))
        # ⚠ Was nicht im Profil steht, ist 0 % — nicht „unbekannt".
        _r63 = _bg63.raffinerien_fuer('Riccite')
        pruefe(_r63 and all(w == 0 for _n, _s, w in _r63),
               'ein Erz ohne Profileintrag steht ueberall auf 0 %')
        # ⚠ Schreibweisen: Profile sagen „Aluminum (Ore)", Rezepte „Aluminium".
        pruefe(bool(_bg63.raffinerien_fuer('Aluminium')),
               'die britische Schreibweise findet dieselben Raffinerien')
        # Und die Reihenfolge: beste zuerst.
        _r63 = _bg63.raffinerien_fuer('Bexalite')
        pruefe(all(_r63[i][2] >= _r63[i+1][2] for i in range(len(_r63)-1)),
               'die Liste steht nach Bonus sortiert, beste zuerst')

    # Die Daten muessen beim Sichern erhalten bleiben — genau daran lag es.
    _q63 = open(os.path.join(WURZEL, 'scbp', 'bergbau.py'), encoding='utf-8').read()
    pruefe("'refineries': roh.get('refineries')" in _q63,
           'die Raffinerien werden beim Sichern behalten')
    pruefe("da.get('refineries') is not None" in _q63,
           'und eine alte Ablage ohne sie wird einmal neu geholt')
    _q63b = open(os.path.join(WURZEL, 'scbp', 'herstellung.py'), encoding='utf-8').read()
    pruefe("'dismantle': roh.get('dismantle')" in _q63b,
           'dasselbe fuer die Zerlege-Sperrliste')
    from scbp import sprache as _sp63
    for _k63 in ('s_bg_raff_kopf', 's_bg_raff_zeile', 's_bg_raff_egal',
                 's_bg_raff_spanne', 's_bg_raff_weitere', 's_he_prozent',
                 's_he_spanne', 's_he_zerlegen'):
        _w63 = _sp63.TEXTE.get(_k63)
        pruefe(bool(_w63) and len(_w63) == 2 and all(_w63),
               'Text %s gibt es deutsch und englisch' % _k63)

    # ------------------------------------------------------------------
    # 64. Scan-Signatur — aus der Zahl des Scanners das Erz bestimmen
    #
    # Der Bergbau-Scanner im Spiel zeigt eine Zahl und verraet nicht, was
    # dahintersteckt. Die Zahl ist die Signatur des Rohstoffs mal der Zahl der
    # Brocken; wie viele es hoechstens sein koennen, sagt die Seltenheit.
    # Gegengerechnet gegen die Tabelle auf scmdb.net (Stand 4.10.0).
    print()
    print('64. Scan-Signatur')
    from scbp import bergbau as _bg64

    if not (_bg64.laden().get('elemente') or {}):
        print('  [–]    keine Rohstoff-Stammdaten vorhanden — uebersprungen')
    else:
        # a) Punktgenaue Treffer aus der Tabelle.
        for _eingabe64, _soll64, _anz64 in (
                ('7080', 'Beryl', 2),        # scmdb: „1 MATCH — 2x Beryl"
                ('3170', 'Quantainium', 1),
                ('4270', 'Iron', 1),
                ('25800', 'Ice', 6),
                ('19500', 'Torite', 5)):
            _tr64 = _bg64.signatur_suchen(_eingabe64)
            pruefe(bool(_tr64) and _tr64[0][0].startswith(_soll64)
                   and _tr64[0][1] == _anz64,
                   '%s -> %d× %s (gefunden: %s)'
                   % (_eingabe64, _anz64, _soll64,
                      ('%d× %s' % (_tr64[0][1], _tr64[0][0])) if _tr64 else 'nichts'))

        # b) ⚠ Ohne Toleranz wird NICHTS gerundet. Wer daneben liegt, soll das
        #    erfahren statt einen falschen Treffer vorgesetzt zu bekommen.
        pruefe(_bg64.signatur_suchen('9999') == [],
               'ein Wert ohne Entsprechung liefert nichts, statt zu raten')
        pruefe(len(_bg64.signatur_suchen('~8600')) > 1,
               'mit ~ davor kommen die Nachbarn dazu')
        pruefe(len(_bg64.signatur_suchen('12000-13000')) > 1,
               'eine Bereichssuche findet mehrere')

        # c) Die Seltenheit begrenzt die Vielfachen. Quantainium ist legendaer
        #    (hoechstens 2 Brocken) — ein drittes Vielfaches darf es NICHT
        #    geben, sonst behauptet das Werkzeug unmoegliche Vorkommen.
        _drei64 = _bg64.signatur_suchen('9510')      # 3170 x 3
        pruefe(not any(n.startswith('Quantainium') for n, _a, _g, _ab in _drei64),
               'legendaeres Erz wird nicht mit 3 Brocken gemeldet')
        _zwei64 = _bg64.signatur_suchen('6340')      # 3170 x 2
        pruefe(any(n.startswith('Quantainium') for n, _a, _g, _ab in _zwei64),
               'mit 2 Brocken dagegen schon')

        # d) Sortierung: die genaueste Uebereinstimmung zuerst.
        _tr64 = _bg64.signatur_suchen('~8600')
        pruefe(abs(_tr64[0][3]) <= abs(_tr64[-1][3]),
               'die genaueste Uebereinstimmung steht oben')

    # Die Stammdaten muessen beim Sichern erhalten bleiben.
    _q64 = open(os.path.join(WURZEL, 'scbp', 'bergbau.py'), encoding='utf-8').read()
    pruefe("'elemente': roh.get('mineableElements')" in _q64,
           'die Rohstoff-Stammdaten werden beim Sichern behalten')
    pruefe("da.get('elemente') is not None" in _q64,
           'und eine alte Ablage ohne sie wird einmal neu geholt')
    # ⚠ Das Eingabefeld darf NICHT im Neuzeichnen gebaut werden.
    _q64b = open(os.path.join(WURZEL, 'scbp', 'seiten.py'), encoding='utf-8').read()
    _vor64 = _q64b.split('def sig_zeichnen')[0]
    pruefe('sig_feld = rundes_feld' in _vor64,
           'das Scan-Feld steht ausserhalb des Neuzeichnens (Cursor bleibt)')
    from scbp import sprache as _sp64
    for _k64 in ('s_bg_sig_feld', 's_bg_sig_hilfe', 's_bg_sig_treffer',
                 's_bg_sig_nichts', 's_bg_sig_anzahl', 's_bg_sig_genau'):
        _w64 = _sp64.TEXTE.get(_k64)
        pruefe(bool(_w64) and len(_w64) == 2 and all(_w64),
               'Text %s gibt es deutsch und englisch' % _k64)

    # ------------------------------------------------------------------
    # 65. Auch eine umgestellte Uebersetzung wird erkannt
    #
    # Bis v3.3.0-rc37 wurde aus der `global.ini` nur der Teil VOR dem `%s`
    # genommen. Bei „Bauplan erhalten: %s" ist das richtig. Bei einer
    # umgestellten Formulierung — „%s ist eingetroffen" — waere davor nichts,
    # die Erkennung fiele auf die mitgelieferte Tabelle zurueck und faende
    # NICHTS: keine Fehlermeldung, keine uebersprungene Datei, einfach null
    # Bauplaene. Die gefaehrlichste Art zu scheitern.
    #
    # ⚠⚠ Das ist der Weg, auf dem JEDER Bauplanfund laeuft. Deshalb prueft (a)
    # zuerst, dass der heutige Fall zeichengleich geblieben ist.
    print()
    print('65. Umgestellte Uebersetzung')
    import re as _re65
    from scbp import phrasen as _ph65
    from scbp import logquelle as _lq65

    # a) ⚠ Der Normalfall MUSS unveraendert sein — Zeichen fuer Zeichen.
    _liste65 = _ph65.sammeln()[0]
    _alt65 = _ph65.RAHMEN % '|'.join(_re65.escape(_p) for _p in _liste65)
    pruefe(_ph65.muster().pattern == _alt65,
           'ohne umgestellte Formulierung ist der Ausdruck zeichengleich '
           'mit dem alten')

    # a2) ⚠ Der Bericht darf keine falsche Herkunft behaupten. Er zeigte eine
    #     einzige Quelle hinter der GANZEN Liste — „aus der global.ini des
    #     Spiels" — obwohl die Liste gemischt ist: belegte Formulierungen und
    #     die eingebaute Rueckfalltabelle. Am 01.09.2026 kostete das drei
    #     Suchlaeufe in einer 12-MB-Datei nach „Bauplan ueberchoo", das dort
    #     gar nicht stehen kann (Schweizerdeutsch, aus der Tabelle).
    from scbp import bericht as _ber65, sprache as _sp65
    _zeile65 = _ber65._spielsprache() or ''
    _eigene65, _ini65 = _ph65.gemessene()
    _rueck65 = [_p for _p in _liste65 if _p not in _eigene65 + _ini65]
    if _rueck65:
        pruefe(_sp65.t('b_woher_tabelle') in _zeile65,
               'Rueckfall-Formulierungen sind als Tabelle gekennzeichnet')
        # Und sie stehen HINTER der Kennzeichnung der belegten Quelle, nicht
        # davor — sonst liest sich die Tabelle wieder wie die global.ini.
        if _ini65:
            pruefe(_zeile65.index(_rueck65[-1])
                   > _zeile65.index(_sp65.t('b_woher_ini')),
                   'die Tabellen-Formulierungen stehen nicht unter der '
                   'global.ini-Angabe')
    if _ini65:
        pruefe(_sp65.t('b_woher_ini') in _zeile65,
               'die belegte Quelle wird weiterhin genannt')

    # b) Zerlegen in Vor- und Nachtext.
    for _phrase65, _soll65 in (
            ('Bauplan erhalten: %s', ('Bauplan erhalten', '')),
            ('%s ist eingetroffen', ('', 'ist eingetroffen')),
            ('Bauplan: %s erhalten', ('Bauplan', 'erhalten')),
            ('Received Blueprint', ('Received Blueprint', ''))):
        pruefe(_ph65.zerlegen(_phrase65) == _soll65,
               'zerlegen(%r) -> %r' % (_phrase65, _ph65.zerlegen(_phrase65)))

    # c) Und die Erkennung an echten Zeilenformen.
    _m65 = _ph65.muster(['Bauplan erhalten', '%s ist eingetroffen',
                         'Bauplan: %s erhalten'])
    for _zeile65, _soll65 in (
            ('Added notification "Bauplan erhalten: Yubarev Pistol: " [3] to queue.',
             'Yubarev Pistol'),
            ('Added notification "Attrition-5 Repeater ist eingetroffen: " [1] to queue.',
             'Attrition-5 Repeater'),
            ('Added notification "Bauplan: Aves Shrike Helmet erhalten: " [2] to queue.',
             'Aves Shrike Helmet')):
        _funde65 = _lq65._namen_aus_text(_zeile65, _m65)
        pruefe(bool(_funde65) and _funde65[0][0] == _soll65,
               'erkannt: %s' % (_funde65[0][0] if _funde65 else 'NICHTS'))

    # d) ⚠ Auftrags-Meldungen duerfen NICHT mitgehen — sie haben dieselbe
    #    Zeilenform und wuerden den Bestand mit Auftragsnamen fluten.
    for _zeile65 in (
            'Added notification "Auftrag angenommen: Retake Platforms: " [4] to queue.',
            'Added notification "Neuer Auftrag: Koerper durchsuchen: " [5] to queue.'):
        pruefe(not _lq65._namen_aus_text(_zeile65, _m65),
               'eine Auftrags-Meldung loest nichts aus')

    # e) Die schweizerdeutsche Fassung steht in der Rueckfall-Tabelle.
    pruefe(any('überchoo' in _p for _p in _ph65.TABELLE.get('de', [])),
           'die live-CH-Formulierung ist dabei')

    # f) Ohne jede Formulierung darf der Ausdruck NIE treffen — ein Muster,
    #    das auf alles passt, waere schlimmer als gar keines.
    pruefe(not _ph65.muster([]).findall(
               'Added notification "Irgendwas: Irgendwer: " [9] to queue.'),
           'eine leere Liste ergibt einen Ausdruck, der nie trifft')

    # ------------------------------------------------------------------
    # 66. Preise — „kaufen oder abbauen?"
    #
    # Die Herstellung sagte, WAS fehlt, aber nicht, ob man es ueberhaupt kaufen
    # kann. Gemessen am 30.08.2026 ueber alle 26 Rohstoffe in Rezepten: 19
    # kaufbar, **7 nicht** (Aslarite, Lindinium, Ouratite, Quantainium,
    # Riccite, Savrilium, Torite). Fuenf davon stehen zusaetzlich auf der
    # Zerlege-Sperrliste — weder kaufbar noch zurueckzugewinnen.
    print()
    print('66. Rohstoffpreise')
    from scbp import preise as _pr66

    # a) ⚠ Ohne Netz und ohne Ablage darf NICHTS passieren.
    _echt66 = _pr66.laden
    try:
        _pr66.laden = lambda: {}
        pruefe(_pr66.preis('Iron') is None,
               'ohne Preisdaten kommt None zurueck, kein Absturz')
        pruefe(_pr66.alter() is None,
               'und das Alter ist None statt einer erfundenen Zahl')
    finally:
        _pr66.laden = _echt66

    # b) ⚠⚠ Jedes Material steht bei UEX ZWEIMAL — veredelt und als Erz. Wer
    #    beim Einlesen ueberschreibt, bekommt zufaellig die falsche Form: Beim
    #    ersten Versuch stand bei Iron „Kaufpreis 0", obwohl es fuer 2.643 im
    #    Regal liegt.
    _bau66 = {'format': _pr66.FORMAT, 'geholt': 1.0, 'waren': {
        'iron': [{'name': 'Iron', 'kauf': 2643.0, 'verkauf': 3376.0},
                 {'name': 'Iron (Ore)', 'kauf': 0.0, 'verkauf': 1000.0}],
        'borase': [{'name': 'Borase', 'kauf': 0.0, 'verkauf': 27266.0},
                   {'name': 'Borase (Ore)', 'kauf': 5520.0, 'verkauf': 14000.0}],
        'quantainium': [{'name': 'Quantainium', 'kauf': 0.0,
                         'verkauf': 145789.0}]}}
    _pr66.laden = lambda: _bau66
    try:
        pruefe(_pr66.preis('Iron')[0] == 2643.0,
               'Iron nimmt die veredelte Form (2643), nicht das Erz')
        pruefe(_pr66.preis('Borase')[0] == 5520.0
               and _pr66.preis('Borase')[2] == 'Borase (Ore)',
               'Borase nimmt das Erz — dort steht der einzige Kaufpreis')
        pruefe(_pr66.preis('Quantainium')[0] == 0.0,
               'Quantainium ist nicht kaufbar (Kaufpreis 0)')
        pruefe(_pr66.preis('Quantainium')[1] == 145789.0,
               'der Verkaufspreis kommt trotzdem mit')
        # ⚠ Die Namensangleichung muss auch hier greifen.
        pruefe(_pr66.preis('Iron (Ore)')[0] == 2643.0,
               'die Erz-Schreibweise findet denselben Eintrag')
        pruefe(_pr66.preis('Voellig Unbekanntes') is None,
               'ein unbekannter Name ergibt None, keinen Nullpreis')
    finally:
        _pr66.laden = _echt66

    # c) Die Anzeige darf „nicht kaufbar" NIE als „0 aUEC" schreiben.
    _seiten66 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                     encoding='utf-8').read()
    pruefe("t('s_he_nur_abbau')" in _seiten66,
           'fuer nicht kaufbare Rohstoffe steht ein eigener Text da')
    pruefe('def _geld' in _seiten66,
           'Betraege bekommen Tausenderpunkte (145789 liest sonst niemand)')
    # d) Der Abruf laeuft im Hintergrund, nicht beim Seitenaufbau.
    _haupt66 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'),
                    encoding='utf-8').read()
    pruefe('def _preise_tick' in _haupt66 and 'self._preise_tick()' in _haupt66,
           'die Preise werden im Hintergrund-Faden geholt')
    # ⚠ Die Qualitaet MUSS am Preis stehen. Ohne sie liest sich „kaufen" wie
    #   ein gleichwertiger Weg, der nur Geld statt Zeit kostet — und das ist
    #   falsch: Am Terminal gekaufte Ware hat immer Q 500, den Nullpunkt, also
    #   exakt x1,000 auf jede Eigenschaft.
    pruefe(_pr66.KAUF_QUALITAET == 500,
           'die Qualitaet gekaufter Ware ist als 500 festgehalten')
    pruefe('preis_modul.KAUF_QUALITAET' in _seiten66,
           'und steht in der Anzeige neben dem Preis')
    pruefe("t('s_he_kauf_q')" in _seiten66,
           'dazu der Satz, der die Regler einordnet')

    from scbp import sprache as _sp66
    for _k66 in ('s_he_kaufen', 's_he_nur_abbau', 's_he_kauf_q'):
        _w66 = _sp66.TEXTE.get(_k66)
        pruefe(bool(_w66) and len(_w66) == 2 and all(_w66),
               'Text %s gibt es deutsch und englisch' % _k66)

    # ------------------------------------------------------------------
    # 67. Ein Rezept wirklich AUFKLAPPEN
    #
    # ⚠⚠ Der Fehler, der diese Pruefung erzwungen hat (rc37 und rc38
    # ausgeliefert): Beim Auspacken der Zerlege-Angaben bekam eine Variable den
    # Namen `_dauer` — und ueberschrieb damit die gleichnamige Funktion in
    # derselben Datei. Ein paar Zeilen spaeter warf `_dauer(stufe['zeit'])`
    # dann `TypeError: 'int' object is not callable`.
    #
    # Sichtbar wurde das als **verschwundener Qualitaets-Block**: Die Ausnahme
    # brach den Aufbau mitten drin ab, die Herstellzeit blieb ohne Wert, und
    # alles danach — Regler, Wirkungen, Hinweise — fehlte ersatzlos.
    #
    # Der Selbsttest hat es nicht gesehen, weil er die Seite **baute**, aber
    # nie eine Zeile aufklappte. Genau das tut er jetzt: Ohne aufgeklapptes
    # Rezept laeuft `_herstellung_zeile` gar nicht bis zu der Stelle.
    print()
    print('67. Ein Rezept aufklappen')
    import tkinter as _tk67
    from scbp import seiten as _se67
    from scbp import herstellung as _he67

    # ⚠⚠ **Notfalls eigene Daten hinlegen.** Die Rezepte sind ein
    # heruntergeladener Zwischenspeicher im Ablageordner — der Selbsttest
    # arbeitet in einem Wegwerf-Ordner, dort liegt keiner. Bis rc42 hiess das:
    # Diese Pruefung wurde **immer uebersprungen**, auf jedem frischen Rechner
    # und im Bau-Lauf sowieso. Sie war fuer den `_dauer`-Fehler gebaut worden,
    # der zwei ausgelieferte Fassungen unbrauchbar gemacht hat — und lief nie.
    # Eine Pruefung, die nur bei ihrem Autor anschlaegt, ist keine.
    if not (_he67.laden().get('blueprints') or []):
        _mini67 = {
            'format': _he67.FORMAT, 'build': 'selbsttest',
            'blueprints': [{
                'tag': 'BP_TEST_Pruefung67', 'productName': 'Testgegenstand',
                'manufacturer': 'Behring', 'gear': 'fpsgear',
                'type': 'armour', 'subtype': 'combat',
                'tiers': [{'craftTimeSeconds': 200, 'slots': [
                    {'name': 'Armored Carapace',
                     'options': [{'type': 'resource', 'quantity': 0.04,
                                  'minQuality': 0, 'resourceName': 'Iron'}],
                     'modifiers': [{'startQuality': 0, 'endQuality': 1000,
                                    'modifierAtStart': 0.9,
                                    'modifierAtEnd': 1.1,
                                    'propertyName': 'Damage Mitigation',
                                    'propertyKey': 'armor_damagemitigation'}]},
                    {'name': 'Insulative Liner',
                     'options': [{'type': 'resource', 'quantity': 0.02,
                                  'minQuality': 0, 'resourceName': 'Aslarite'}],
                     'modifiers': [{'startQuality': 0, 'endQuality': 1000,
                                    'modifierAtStart': 0.8,
                                    'modifierAtEnd': 1.2,
                                    'propertyName': 'Min Temp',
                                    'propertyKey': 'armor_temperaturemin'},
                                   {'startQuality': 0, 'endQuality': 1000,
                                    'modifierAtStart': 0.8,
                                    'modifierAtEnd': 1.2,
                                    'propertyName': 'Max Temp',
                                    'propertyKey': 'armor_temperaturemax'}]}]}]}],
            'dismantle': {'returnPercentage': 50, 'blacklistedResources': []}}
        from scbp import pfade as _pf67
        with open(_pf67.app_datei(_he67.CACHE), 'w', encoding='utf-8') as _f67:
            json.dump(_mini67, _f67)
        _he67.vergessen()

    _rez67 = _he67.laden().get('blueprints') or []
    if not _rez67:
        print('  [–]    keine Rezeptdaten vorhanden — uebersprungen')
    else:
        # Ein Bauplan mit Zutaten UND Qualitaetswirkungen — nur der laeuft
        # durch alle Zweige.
        _kandidat67 = None
        for _b67 in _rez67:
            for _t67 in _b67.get('tiers') or []:
                for _s67 in _t67.get('slots') or []:
                    if _s67.get('modifiers') and _s67.get('options'):
                        _kandidat67 = _b67.get('productName')
                        break
                if _kandidat67:
                    break
            if _kandidat67:
                break
        pruefe(bool(_kandidat67), 'ein Bauplan mit Qualitaetswirkungen gefunden')

        if _kandidat67:
            _w67 = _tk67.Tk()
            _w67.withdraw()          # ⚠ kein Fenster ins Bild schieben
            try:
                # ⚠ **Echte Schrift-Objekte, keine Tupel.** Die Regler und
                # Auswahlfelder rufen `.metrics()` und `.measure()` darauf auf;
                # mit einem Tupel bricht der Aufbau mit
                # `AttributeError: 'tuple' object has no attribute 'metrics'`
                # ab — und der Test wuerde einen Fehler melden, den es im
                # Programm gar nicht gibt.
                import tkinter.font as _tkfont67
                _schrift67 = _tkfont67.Font(root=_w67, family='TkDefaultFont',
                                            size=10)

                class _Fenster67:
                    # ⚠ **Das Ersatzfenster muss alle Schriften kennen, die das
                    # echte hat.** Am 31.08.2026 fehlte `f_fett`, und die neue
                    # Kopfzeile ueber dem Rezept liess die ganze Pruefung
                    # auffliegen — im Bau-Lauf, nicht auf dem Entwicklerrechner,
                    # weil der Selbsttest dort schon vorher abbricht. Kommt eine
                    # Schrift dazu, gehoert sie hierher.
                    f_grund = f_klein = f_item = f_fett = _schrift67
                    beim_zeigen = {}
                    bergbau_suche = ''

                    def oeffnen(self, _name):
                        pass

                _rahmen67 = _tk67.Frame(_w67)
                _eintrag67 = {'name': _kandidat67, 'basis': _kandidat67,
                              'habe': True, 'hersteller': 'Behring'}
                _offen67 = {'name': _kandidat67}      # ⭐ AUFGEKLAPPT
                _fehler67 = None
                try:
                    _se67._herstellung_zeile(_Fenster67(), _rahmen67,
                                             _eintrag67, _offen67,
                                             lambda: None)
                except Exception as _aus67:
                    _fehler67 = '%s: %s' % (type(_aus67).__name__, _aus67)
                pruefe(_fehler67 is None,
                       'ein aufgeklapptes Rezept baut ohne Ausnahme (%s)'
                       % (_fehler67 or 'sauber'))

                # Und der Qualitaets-Block muss wirklich dastehen — nicht nur
                # „keine Ausnahme". Genau der war ja verschwunden.
                def _texte67(w, raus):
                    try:
                        raus.append(str(w.cget('text')))
                    except Exception:
                        pass
                    for _k in w.winfo_children():
                        _texte67(_k, raus)
                    return raus

                _alle67 = ' | '.join(_texte67(_rahmen67, []))
                from scbp import sprache as _sp67
                pruefe(_sp67.t('s_he_regler_kopf') in _alle67,
                       'die Ueberschrift der Qualitaetsregler steht da')
                pruefe('Q ' in _alle67 or '×' in _alle67,
                       'und die Spannen-Angaben darunter')

                # ⚠⚠ Und JEDE Spanne steht unter IHRER Zeile.
                #
                # Bis rc42 bekam das Spannen-Etikett den Behaelter eine Ebene
                # hoeher als Elternteil. Es baute sich fehlerfrei auf, es stand
                # auch alles da — nur sammelten sich alle Spannen am Ende des
                # Blocks, waehrend die Werte oben blieben. Drei gleich
                # aussehende Zeilen `Q 0-1000 · x0.9-1.1`, und keine sagte mehr,
                # zu welchem Wert sie gehoert. Kein Absturz, keine Ausnahme —
                # nur eine Anzeige, die nichts mehr aussagt.
                #
                # Der Massstab ist deshalb die **Reihenfolge**: Im Behaelter der
                # Wertezeilen muessen sich Zeile (Frame) und Spanne (Label)
                # abwechseln.
                def _ist_wertezeile67(w):
                    # Eine Wertezeile ist ein Rahmen aus genau vier Etiketten:
                    # Eigenschaft, Faktor, Prozent, Herkunft. Nichts sonst
                    # darin — sonst waere es ein Behaelter, kein Zeile.
                    kinder = w.winfo_children()
                    return (w.winfo_class() == 'Frame' and len(kinder) == 4
                            and all(_x.winfo_class() == 'Label'
                                    for _x in kinder))

                def _wertebehaelter67(w):
                    for _k in w.winfo_children():
                        if _ist_wertezeile67(_k):
                            return _k.master
                        _tiefer = _wertebehaelter67(_k)
                        if _tiefer is not None:
                            return _tiefer
                    return None

                _halter67 = _wertebehaelter67(_rahmen67)
                pruefe(_halter67 is not None,
                       'der Behaelter mit den Wertezeilen ist auffindbar')
                if _halter67 is not None:
                    _folge67 = [_s.winfo_class() for _s in _halter67.pack_slaves()]
                    _zeilen67 = _folge67.count('Frame')
                    _spannen67 = _folge67.count('Label')
                    # Nach jeder Zeile genau ein Etikett — dann wechseln sich
                    # Frame und Label ab, und keine Spanne ist verrutscht.
                    _wechsel67 = _folge67[:2 * _zeilen67] == (
                        ['Frame', 'Label'] * _zeilen67)
                    pruefe(_zeilen67 > 0 and _wechsel67,
                           'jede Spanne steht direkt unter ihrer Wertezeile '
                           '(%d Zeilen, %d Spannen: %s)'
                           % (_zeilen67, _spannen67,
                              ' '.join(_folge67[:6]) or 'leer'))
            finally:
                _w67.destroy()

    # ⚠ Kein lokaler Name darf eine Funktion derselben Datei verdecken.
    # Statische Gegenprobe fuer genau diesen Fehler.
    import re as _re67
    _q67 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'), encoding='utf-8').read()
    _funktionen67 = set(_re67.findall(r'^def (_?\w+)\(', _q67, _re67.M))
    _verdeckt67 = []
    for _m67 in _re67.finditer(r'^\s+([_a-zA-Z][\w, ]*?)\s*=\s*\S', _q67, _re67.M):
        for _name67 in (_n.strip() for _n in _m67.group(1).split(',')):
            if _name67 in _funktionen67:
                _zeile67 = _q67[:_m67.start()].count('\n') + 1
                _verdeckt67.append('%s (Zeile %d)' % (_name67, _zeile67))
    pruefe(not _verdeckt67,
           'kein lokaler Name verdeckt eine Funktion derselben Datei (%d)'
           % len(_verdeckt67))
    for _x67 in _verdeckt67[:5]:
        print('       ·', _x67)

    # ------------------------------------------------------------------
    # 68. Der Namensvorschlag steht NEBEN dem Eingabefeld
    #
    # Bis v3.3.0-rc39 hing er ganz unten unter den Knoepfen — 557 Pixel unter
    # dem Feld, in das getippt wird. Am 30.08.2026 gemeldet: „wenn ich
    # Savrilium einlagern will, suche ich nicht dort unten nach dem Begriff um
    # drauf zu klicken." Ein Vorschlag, den man suchen muss, ist keiner.
    print()
    print('68. Namensvorschlag am Eingabefeld')
    import tkinter as _tk68
    import tkinter.font as _tkfont68
    from scbp import seiten as _se68

    _w68 = _tk68.Tk()
    _w68.withdraw()                      # ⚠ kein Fenster ins Bild schieben
    try:
        _w68.geometry('1200x900')
        _s68 = _tkfont68.Font(root=_w68, family='TkDefaultFont', size=10)

        class _Fenster68:
            f_grund = f_klein = f_item = f_fett = f_titel = f_sub = _s68
            beim_zeigen = {}
            bergbau_suche = ''

            def oeffnen(self, _n):
                pass

            def sagen(self, *_a):
                pass

        _rahmen68 = _tk68.Frame(_w68)
        _rahmen68.pack(fill='both', expand=True)
        _se68._lager(_Fenster68(), _rahmen68)
        _w68.update_idletasks()

        def _sammeln68(w, art, raus):
            if isinstance(w, art):
                raus.append(w)
            for _k in w.winfo_children():
                _sammeln68(_k, art, raus)
            return raus

        def _mit_text68(w, text, raus):
            try:
                if text in str(w.cget('text')):
                    raus.append(w)
            except Exception:
                pass
            for _k in w.winfo_children():
                _mit_text68(_k, text, raus)
            return raus

        _felder68 = _sammeln68(_rahmen68, _tk68.Entry, [])
        pruefe(bool(_felder68), 'die Lager-Seite hat Eingabefelder')
        # ⚠ Ohne Rezeptdaten gibt es keine Namen, zu denen etwas vorgeschlagen
        # werden koennte — auf dem Bau-Laeufer ist das der Normalfall.
        from scbp import herstellung as _he68
        if _felder68 and _he68.aehnliche_rohstoffe('sa'):
            _felder68[0].insert(0, 'sa')
            _w68.update_idletasks()
            _v68 = _mit_text68(_rahmen68, 'Savrilium', [])
            pruefe(bool(_v68), 'nach „sa" erscheint ein Vorschlag')
            if _v68:
                _abstand68 = abs(_v68[0].winfo_rooty()
                                 - _felder68[0].winfo_rooty())
                pruefe(_abstand68 < 80,
                       'der Vorschlag steht auf Hoehe des Feldes (%d px, '
                       'vorher 557)' % _abstand68)
                pruefe(_v68[0].winfo_rootx() < _felder68[0].winfo_rootx(),
                       'und links davon')
        elif _felder68:
            print('  [–]    keine Rezeptdaten — Vorschlagstest uebersprungen')

        # ---- Rechnen im Mengenfeld ----
        # ⚠⚠ Beim Bearbeiten steht die aktuelle Menge schon im Feld. Wer drei
        # dazulegen will, tippt hinten „+3" an — und hat „1.04+3" dastehen.
        # Bis v3.3.0-rc39 zaehlte nur ein FUEHRENDES Vorzeichen; genau die
        # natuerliche Eingabe wurde abgelehnt („Trag eine Menge ein, zum
        # Beispiel 12,5"). Am 30.08.2026 gemeldet.
        from scbp import rohstoffe as _ro68
        for _eingabe68, _vorher68, _soll68 in (
                ('4,5', 1.04, 4.5),          # blosse Zahl
                ('+3', 1.04, 4.04),          # nur Buchung
                ('1.04+3', 1.04, 4.04),      # angehaengt — das war der Fehler
                ('1,04+3', 1.04, 4.04),      # mit Komma genauso
                ('12,5-0,5', 0.0, 12.0),     # Minus mitten drin
                ('-0,5', 1.04, 0.54)):       # abbuchen
            _ist68 = _ro68.rechnen(_eingabe68, _vorher68)
            pruefe(_ist68 is not None and abs(_ist68 - _soll68) < 1e-9,
                   '%r bei Bestand %g ergibt %s (erwartet %g)'
                   % (_eingabe68, _vorher68, _ist68, _soll68))
        # ⚠ Beide Wege muessen dasselbe ergeben — das ist der Punkt: Niemand
        #   muss wissen, welchen das Programm meint.
        pruefe(_ro68.rechnen('+3', 1.04) == _ro68.rechnen('1.04+3', 1.04),
               '„+3" und „1.04+3" ergeben dasselbe')
        for _unsinn68 in ('12 SCU', '', '+abc', 'abc'):
            pruefe(_ro68.rechnen(_unsinn68, 1.0) is None,
                   'Unsinn (%r) ergibt None statt einer Zahl' % _unsinn68)

        # ---- Und der Hinweistext darf nicht wieder abstrakt werden ----
        from scbp import sprache as _sp68
        _hinweis68 = _sp68.TEXTE['s_lg_rechnen'][0]
        pruefe('+3' in _hinweis68 and '-3' in _hinweis68,
               'der Hinweis nennt die Zeichen konkret')
        pruefe('abgebucht' not in _hinweis68,
               'und benutzt keine Buchhaltersprache mehr')
        for _k68 in ('s_lg_ergibt', 's_lg_ergibt_null', 's_lg_ergibt_minus'):
            _w68t = _sp68.TEXTE.get(_k68)
            pruefe(bool(_w68t) and len(_w68t) == 2 and all(_w68t),
                   'Text %s gibt es deutsch und englisch' % _k68)
    finally:
        _w68.destroy()

    # ------------------------------------------------------------------
    # 69. Kein abgeschnittener Text im aufgeklappten Rezept
    #
    # ⚠⚠ Der Fehler, der diese Pruefung erzwungen hat: Das Etikett fuer den
    # Qualitaetsfaktor hatte `width=9` — eine **Zusage ueber den Inhalt**. Als
    # in v3.3.0-rc37 die Prozentzahl in dasselbe Etikett geschrieben wurde,
    # schnitt Tk sie stumm ab: Auf dem Bildschirm stand „× 1.047  +4.(" statt
    # „+4,70 %". Kein Fehler, keine Meldung — nur eine halbe Zahl.
    #
    # Wer Inhalt zu einem Feld fester Breite dazutut, muss die Breite anfassen.
    # Diese Pruefung merkt es, wenn er es vergisst — an JEDEM Etikett, nicht
    # nur an diesem einen.
    print()
    print('69. Nichts wird abgeschnitten')
    import tkinter as _tk69
    import tkinter.font as _tkfont69
    from scbp import seiten as _se69
    from scbp import herstellung as _he69

    _rez69 = _he69.laden().get('blueprints') or []
    if not _rez69:
        print('  [–]    keine Rezeptdaten vorhanden — uebersprungen')
    else:
        _kandidat69 = None
        for _b69 in _rez69:
            for _t69 in _b69.get('tiers') or []:
                for _s69 in _t69.get('slots') or []:
                    if _s69.get('modifiers') and _s69.get('options'):
                        _kandidat69 = _b69.get('productName')
                        break
                if _kandidat69:
                    break
            if _kandidat69:
                break

        _w69 = _tk69.Tk()
        _w69.withdraw()                  # ⚠ kein Fenster ins Bild schieben
        try:
            _w69.geometry('1300x1000')
            _s69f = _tkfont69.Font(root=_w69, family='TkDefaultFont', size=10)

            class _Fenster69:
                f_grund = f_klein = f_item = f_fett = f_titel = f_sub = _s69f
                beim_zeigen = {}
                bergbau_suche = ''

                def oeffnen(self, _n):
                    pass

                def sagen(self, *_a):
                    pass

            _rahmen69 = _tk69.Frame(_w69)
            _rahmen69.pack(fill='both', expand=True)
            _se69._herstellung_zeile(
                _Fenster69(), _rahmen69,
                {'name': _kandidat69, 'basis': _kandidat69, 'habe': True,
                 'hersteller': 'Behring'},
                {'name': _kandidat69}, lambda: None)
            _w69.update_idletasks()

            _kurz69 = []

            def _messen69(w):
                if isinstance(w, _tk69.Label):
                    _txt69 = str(w.cget('text'))
                    # ⚠ Nur Etiketten OHNE Umbruch pruefen. Fliesstext soll
                    #   umbrechen, nicht in eine Zeile passen.
                    try:
                        _umbruch69 = int(w.cget('wraplength') or 0)
                    except Exception:
                        _umbruch69 = 0
                    if _txt69.strip() and not _umbruch69:
                        _noetig69 = _s69f.measure(_txt69)
                        # ⚠⚠ **`winfo_width()` taugt hier NICHT.** Das Fenster
                        # ist bewusst nicht angezeigt (sonst schoebe der Test
                        # ein Fenster ins Bild); fuer alles Unangezeigte
                        # liefert Tk stur **1**. Ein Vergleich dagegen
                        # ueberspringt jede Zeile und die Pruefung meldet
                        # zufrieden „nichts abgeschnitten", waehrend auf dem
                        # Bildschirm eine halbe Zahl steht. Genau so lief mein
                        # erster Anlauf am 30.08.2026 ins Leere.
                        #
                        # `winfo_reqwidth()` ist die Breite, die Tk dem
                        # Etikett geben WIRD — bei `width=9` sind das 76 px,
                        # der Text braucht 112. Das ist messbar, ohne etwas
                        # anzuzeigen.
                        _hat69 = w.winfo_reqwidth()
                        if _hat69 > 1 and _noetig69 > _hat69:
                            _kurz69.append('%r braucht %d px, hat %d'
                                           % (_txt69, _noetig69, _hat69))
                for _k69 in w.winfo_children():
                    _messen69(_k69)

            _messen69(_rahmen69)
            pruefe(not _kurz69,
                   'kein Etikett im Rezept schneidet seinen Text ab (%d)'
                   % len(_kurz69))
            for _x69 in _kurz69[:6]:
                print('       ·', _x69)

            # Und die Prozentzahl muss VOLLSTAENDIG dastehen — genau die war
            # es ja.
            def _prozente69(w, raus):
                try:
                    _x = str(w.cget('text'))
                    if '%' in _x and any(_z in _x for _z in '+-−'):
                        raus.append(_x)
                except Exception:
                    pass
                for _k in w.winfo_children():
                    _prozente69(_k, raus)
                return raus

            _p69 = _prozente69(_rahmen69, [])
            pruefe(bool(_p69), 'die Prozentangaben stehen da (%d)' % len(_p69))
            pruefe(all(_x.rstrip().endswith('%') for _x in _p69),
                   'und enden auf das Prozentzeichen — keine halbe Zahl (%s)'
                   % (_p69[:2],))
        finally:
            _w69.destroy()

    # ------------------------------------------------------------------
    # 70. Nur Echtes ins Lager — Rohstoff UND Lagerort
    #
    # ⚠⚠ Der Grund ist nicht Ordnungssinn. Ein freies Textfeld heisst, dass
    # jemand Schimpfwoerter, Religioeses oder Politisches eintraegt, ein
    # Bildschirmfoto macht und es verbreitet — und am Ende fragt niemand, wer
    # getippt hat: Es steht in diesem Werkzeug. Am 30.08.2026 festgelegt:
    # „NUR was auch in der Rohstoff-Liste ist darf speicherbar sein, sonst
    # nichts." Und: „Lagerort gilt exakt das Gleiche."
    print()
    print('70. Nur Echtes ins Lager')
    from scbp import herstellung as _he70
    from scbp import orte as _or70

    # a) Der Ausweg-Knopf ist WEG und darf nicht zurueckkommen.
    _q70 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'), encoding='utf-8').read()
    pruefe("t('s_lg_trotzdem')" not in _q70,
           'es gibt keinen Knopf „Trotzdem eintragen" mehr')
    # ⚠ Auch der TEXT muss weg. Beim Aufraeumen blieb `s_lg_unbekannt` stehen
    # und behauptete weiter „Du kannst es trotzdem eintragen" — das Programm
    # versprach also etwas, das es nicht mehr tut (30.08.2026 aufgefallen).
    # Wer eine Funktion entfernt, sucht nach ALLEN Stellen, die sie
    # beschreiben, nicht nur nach dem Knopf.
    from scbp import sprache as _sp70
    pruefe('s_lg_trotzdem' not in _sp70.TEXTE,
           'und keinen Text mehr dafuer')
    for _k70 in ('s_lg_unbekannt', 's_lg_name_fremd'):
        _w70 = _sp70.TEXTE.get(_k70) or ('', '')
        pruefe('trotzdem' not in _w70[0].lower()
               and 'still add' not in _w70[1].lower(),
               '%s verspricht keinen Ausweg mehr' % _k70)
    pruefe('h_modul.lager_name(name)' in _q70,
           'der Name wird gegen die Lagerliste geprueft')
    pruefe('orte_modul.offizieller_name(ort.get())' in _q70,
           'und der Lagerort gegen die Ortsliste')

    # b) Die Liste selbst.
    _liste70 = _he70.einlagerbar()
    if len(_liste70) > 30:
        pruefe(len(_liste70) >= 39,
               'die Lagerliste hat %d Namen (Mineralien + Pflanzen)' % len(_liste70))
        for _pflanze70 in ('Flareweed', 'Heart of the Woods', 'Sunset Berry'):
            pruefe(_he70.darf_ins_lager(_pflanze70),
                   'Pflanze %s ist einlagerbar' % _pflanze70)
        for _erz70 in ('Sadaryx', 'Saldynium', 'Jaclium'):
            pruefe(_he70.darf_ins_lager(_erz70),
                   'Mineral ohne Rezept (%s) ist einlagerbar' % _erz70)
        for _mist70 in ('savratum', 'Bei Oma im Keller', 'Politik', 'xyz123'):
            pruefe(not _he70.darf_ins_lager(_mist70),
                   '%r wird abgelehnt' % _mist70)
        # ⚠ Vorschlaege muessen aus der GANZEN Liste kommen. Sadaryx kam nicht,
        #   weil sie nur aus den Rezept-Materialien stammten.
        pruefe(_he70.aehnliche_lagernamen('Sad') == ['Sadaryx'],
               'Sadaryx wird vorgeschlagen (kam frueher nicht)')
    else:
        print('  [–]    keine Rezept-/Bergbaudaten — Listentest uebersprungen')

    # c) Der Lagerort.
    if _or70.alle():
        pruefe(len(_or70.alle()) > 100,
               'die Ortsliste hat %d Eintraege' % len(_or70.alle()))
        pruefe(_or70.kennt('Orison') and _or70.kennt('Lorville'),
               'bekannte Orte werden erkannt')
        pruefe(not _or70.kennt('Bei Oma im Keller'),
               'ein erfundener Ort wird abgelehnt')
        pruefe(_or70.kennt(''), 'leer bleibt erlaubt — das Feld ist freiwillig')
        # ⚠ Teiltext, nicht nur Wortanfang: UEX schreibt „Pyro Gateway
        #   (Stanton)" und „Checkmate Station".
        pruefe(any('Pyro Gateway' in o for o in _or70.aehnliche('pyro')),
               '„pyro" schlaegt die Gateways vor')
        pruefe(_or70.aehnliche('checkmate') == ['Checkmate Station'],
               '„checkmate" findet die Station')
    else:
        # ⚠ Ohne Liste darf NICHTS blockieren — sonst laesst sich bei einem
        #   ersten Start ohne Netz gar nichts eintragen.
        pruefe(_or70.kennt('Irgendwo'),
               'ohne Ortsliste blockiert das Feld nicht')
        print('  [–]    keine Ortsliste vorhanden — Rest uebersprungen')

    # d) Qualitaet: nur 0 bis 1000.
    pruefe('0 <= q <= 1000' in _q70,
           'die Qualitaet ist auf 0–1000 begrenzt')

    # ------------------------------------------------------------------
    # 71. Keine fremde Uebersetzung im Paket
    #
    # ⚠⚠ Die deutsche Uebersetzung des Spiels stammt von rjcncpt
    # (StarCitizen-Deutsch-INI) und steht unter **CC BY-NC-SA 4.0**. Der Autor
    # setzt das durch: Am 10.04.2025 wurde ein Repository nach einer
    # DMCA-Beschwerde von GitHub entfernt — Grund war „nicht-konforme
    # Weitergabe unter CC-BY-NC-SA-4.0" und fehlende Namensnennung.
    #
    # Der Watcher ist davon nicht betroffen, weil er die Uebersetzung **nicht
    # weitergibt**: Er liest die Datei auf dem Rechner des Nutzers und ergaenzt
    # sie dort. Damit das so bleibt, prueft das hier bei jedem Bau nach — eine
    # mitgelieferte `global.ini` waere genau der Fehler, der ein Repo kostet.
    print()
    print('71. Keine fremde Uebersetzung im Paket')
    _versioniert71 = _versionierte_dateien(WURZEL, ('.ini', '.json', '.txt'))
    _verdaechtig71 = []
    for _p71 in _versioniert71:
        _rel71 = os.path.relpath(_p71, WURZEL)
        if _rel71.endswith('.ini'):
            _verdaechtig71.append('%s (eine .ini gehoert nicht ins Repo)' % _rel71)
            continue
        if not _rel71.endswith('.json'):
            continue
        try:
            _txt71 = open(_p71, encoding='utf-8', errors='ignore').read()
        except OSError:
            continue
        # ⚠ Deutsche Spieltexte erkennt man an Umlauten und ß. Ein Katalog aus
        #   der ENGLISCHEN global.ini (CIGs eigene Datei) hat davon keinen.
        _umlaute71 = sum(_txt71.count(_z) for _z in 'äöüßÄÖÜ')
        if _umlaute71 > 40:
            _verdaechtig71.append('%s (%d Umlaute — uebersetzte Spieltexte?)'
                                  % (_rel71, _umlaute71))
    pruefe(not _verdaechtig71,
           'keine fremde Uebersetzung im Repo (%d Funde)' % len(_verdaechtig71))
    for _x71 in _verdaechtig71[:5]:
        print('       ·', _x71)

    # Der mitgelieferte Katalog muss sagen, woher er stammt — und das muss die
    # ENGLISCHE Datei sein.
    _kat71 = os.path.join(WURZEL, 'daten', 'katalog.json')
    if os.path.exists(_kat71):
        import json as _json71
        _d71 = _json71.load(open(_kat71, encoding='utf-8'))
        pruefe('englisch' in str(_d71.get('quelle', '')).lower(),
               'der mitgelieferte Katalog stammt aus der englischen Datei (%r)'
               % _d71.get('quelle'))
        pruefe(_d71.get('weitergabe') is True,
               'und ist ausdruecklich als weitergebbar gekennzeichnet')

    # Und der Urheber muss genannt sein — Name UND Repository, so verlangt es
    # die Lizenz.
    _q71 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'), encoding='utf-8').read()
    pruefe('rjcncpt' in _q71,
           'der Urheber der Uebersetzung ist im Programm genannt')
    pruefe('CC BY-NC-SA 4.0' in _q71,
           'mit seiner Lizenz')
    pruefe('github.com/rjcncpt/StarCitizen-Deutsch-INI' in _q71,
           'und mit seinem Repository')
    for _readme71 in ('README.en.md', 'README.md'):
        _r71 = open(os.path.join(WURZEL, _readme71), encoding='utf-8').read()
        pruefe('rjcncpt' in _r71 and 'CC BY-NC-SA' in _r71,
               '%s nennt Urheber und Lizenz' % _readme71)

    # ⚠⚠ **Die erste Zeile der `global.ini` muss stehen bleiben.** Der Autor
    # verlangt das ausdruecklich: „belasse in der global.ini-Datei die erste
    # Zeile mit der Angabe zur Ursprungsuebersetzung bestehen. Das hilft
    # anderen Spielern ohne Umwege an die urspruengliche Uebersetzung zu
    # gelangen."
    #
    # Bisher blieb sie stehen, weil die Injektion den Schluessel schlicht nicht
    # anfasst — also zufaellig. Diese Pruefung macht daraus eine Zusage.
    _inj71 = open(os.path.join(WURZEL, 'scbp', 'injektion.py'),
                  encoding='utf-8').read()
    pruefe('Frontend_PU_Version' not in _inj71
           or 'nicht anfassen' in _inj71,
           'die Injektion fasst die Quellenangabe nicht an')

    # Und am echten Fall gegengeprueft — an **jeder** vorhandenen Sprachdatei.
    #
    # ⚠ Nur Dateien mit einer Quellenangabe zaehlen. Die englische `global.ini`
    # ist CIGs eigene und traegt keine; sie mit zu pruefen hiesse, einen Fehler
    # zu melden, wo keiner sein kann. Genau so lief mein erster Anlauf: Er nahm
    # die erste Datei im Ordner — die englische — und schlug an.
    from scbp import pfade as _pf71
    _dateien71 = []
    try:
        _basis71 = os.path.join(_pf71.spiel_ordner() or '', 'data', 'Localization')
        for _ordner71 in (sorted(os.listdir(_basis71))
                          if os.path.isdir(_basis71) else []):
            _kandidat71 = os.path.join(_basis71, _ordner71, 'global.ini')
            if os.path.isfile(_kandidat71):
                _dateien71.append((_ordner71, _kandidat71))
    except Exception:
        _dateien71 = []

    _geprueft71 = 0
    for _name71, _pfad71 in _dateien71:
        with open(_pfad71, encoding='utf-8-sig', errors='ignore') as _fh71:
            _zeile1_71 = _fh71.readline()
            _rest71 = _fh71.read()
        # Eine Quellenangabe erkennt man am Schluessel UND daran, dass sie auf
        # die Herkunft verweist.
        if not _zeile1_71.startswith('Frontend_PU_Version'):
            continue
        _geprueft71 += 1
        _marken71 = _rest71.count('[SCBPW]') + _rest71.count('<EM4>')
        pruefe('[SCBPW]' not in _zeile1_71 and '<EM4>' not in _zeile1_71,
               '%s: die Quellenangabe traegt keine unserer Marken '
               '(%d Marken in der Datei)' % (_name71, _marken71))
        pruefe('sc-deutsch-launcher' in _zeile1_71.lower()
               or 'übersetzung' in _zeile1_71.lower()
               or 'übersetzig' in _zeile1_71.lower(),
               '%s: der Verweis auf die Ursprungsuebersetzung steht noch da'
               % _name71)
    if not _geprueft71:
        print('  [–]    keine Datei mit Quellenangabe gefunden — '
              'Gegenprobe uebersprungen')

    # ------------------------------------------------------------------
    # 72. Der Startverlauf im Bericht bleibt lesbar
    #
    # ⚠ Die Spur ist bei einem harten Absturz das Einzige, was uebrig bleibt —
    # die letzte Zeile sagt, wie weit der Start kam. Im rc42-Bericht stand
    # davon **kein einziger Schritt** mehr: zwoelfmal „Liste: zeichnen
    # beginnt" hatte den ganzen Ausschnitt gefuellt. Zwei Ursachen, beide hier
    # abgesichert:
    #
    #   a) Getrennt wurde per Vorsilbe („Seite ") — jeder neue Spur-Aufruf
    #      irgendwo im Programm galt damit als Startschritt. Jetzt ist die
    #      Grenze die Zeile, mit der der Start endet.
    #   b) Wiederholungen wurden Zeile fuer Zeile gezeigt. Jetzt zaehlt der
    #      Bericht sie zusammen.
    print()
    print('72. Der Startverlauf im Bericht bleibt lesbar')
    from scbp import fehler as _fh72
    from scbp import bericht as _br72

    # a) Die Grenze muss die Zeile sein, die das Programm wirklich schreibt.
    _quelle72 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'),
                     encoding='utf-8').read()
    pruefe("fehler.spur('%s')" % _fh72.SPUR_GRENZE in _quelle72,
           'die Grenzzeile „%s" wird beim Start auch wirklich geschrieben'
           % _fh72.SPUR_GRENZE)

    # b) Ein Bedien-Ereignis nach der Grenze darf nicht im Start landen —
    #    auch dann nicht, wenn es nicht mit „Seite " anfaengt.
    _echt72 = _fh72.letzte_spur
    try:
        _spur72 = ['05:10:00  Start, Version 9.9.9, test',
                   '05:10:01  Tk-Wurzel steht',
                   '05:10:02  Overlay wird gebaut',
                   '05:10:03  Overlay steht',
                   '05:10:04  ' + _fh72.SPUR_GRENZE]
        _spur72 += ['05:11:%02d  Liste: zeichnen beginnt' % _i
                    for _i in range(12)]
        _spur72 += ['05:12:00  Seite lager: zeigen']
        _fh72.letzte_spur = lambda: _spur72
        _start72, _bedien72 = _fh72.spur_geteilt()
    finally:
        _fh72.letzte_spur = _echt72
    pruefe(len(_start72) == 5 and _start72[-1].endswith(_fh72.SPUR_GRENZE),
           'der Startverlauf endet an der Grenzzeile (%d Zeilen)'
           % len(_start72))
    pruefe(not [_z for _z in _start72 if 'Liste: zeichnen' in _z],
           'Bedienung nach dem Start faellt nicht in den Startverlauf')

    # c) Zwoelf gleiche Zeilen werden zu einer mit Zaehler.
    _knapp72 = _br72._gedraengt(_bedien72)
    pruefe(len(_knapp72) == 2,
           'zwoelf gleiche Zeilen werden zusammengefasst (%d Zeilen uebrig)'
           % len(_knapp72))
    pruefe('(12×)' in _knapp72[0],
           'die Zusammenfassung nennt die Anzahl')

    # d) Und der Ausschnitt, den der Bericht zeigt, enthaelt den Start noch.
    _sichtbar72 = _br72._gedraengt(_start72)[-12:]
    pruefe(any('Start, Version' in _z for _z in _sichtbar72)
           and any(_fh72.SPUR_GRENZE in _z for _z in _sichtbar72),
           'im sichtbaren Ausschnitt stehen erster und letzter Startschritt')

    # ------------------------------------------------------------------
    # 73. Die Zahlen in der Anleitung stimmen noch
    #
    # ⚠ In der README stehen Zahlen: „für 670 der 738 Bauplaene steht, woher
    # sie kommen", „zu jedem der 1.597 herstellbaren Gegenstaende". Die sind
    # kein Beiwerk — sie sind das Versprechen, das jemand vor dem Herunterladen
    # liest. Und sie veralten mit **jedem** Spiel-Patch, ohne dass irgendetwas
    # anschlaegt: Am 30.08.2026 stand dort 655 von 722, waehrend die Daten
    # laengst 670 von 738 hergaben. Aufgefallen ist es nur, weil jemand von
    # Hand nachgezaehlt hat.
    #
    # ⚠ Diese Pruefung braucht die heruntergeladenen Daten und wird ohne sie
    # uebersprungen — im Bau-Lauf also immer. Sie greift dort, wo sie greifen
    # muss: auf dem Rechner, auf dem veroeffentlicht wird.
    print()
    print('73. Die Zahlen in der Anleitung stimmen noch')
    import re as _re73
    from scbp import katalog as _ka73
    from scbp import herstellung as _he73

    # ⚠ Kurz aus dem Wegwerf-Ordner heraustreten. Der Selbsttest arbeitet in
    # einem leeren `SC_BP_HOME`; die echten Daten liegen im Ablageordner des
    # Nutzers, und genau die zeigt die Anleitung.
    _heim73 = os.environ.pop('SC_BP_HOME', None)
    try:
        _ka73.vergessen() if hasattr(_ka73, 'vergessen') else None
        _he73.vergessen()
        try:
            _bp73 = (_ka73.laden().get('bauplaene') or {})
        except Exception:
            _bp73 = {}
        _gezeigt73 = []
        try:
            _gezeigt73 = _he73.mit_bestand(set())
        except Exception:
            pass
    finally:
        if _heim73 is not None:
            os.environ['SC_BP_HOME'] = _heim73
        _ka73.vergessen() if hasattr(_ka73, 'vergessen') else None
        _he73.vergessen()

    if not _bp73 or not _gezeigt73:
        print('  [–]    keine Katalog- oder Rezeptdaten — uebersprungen')
    else:
        _mitq73 = sum(1 for _v in _bp73.values() if _v.get('q'))
        _soll73 = {'baupläne': len(_bp73), 'herkunft': _mitq73,
                   'herstellbar': len(_gezeigt73)}
        print('       Daten: %d Baupläne, %d mit Herkunft, %d herstellbar'
              % (_soll73['baupläne'], _soll73['herkunft'],
                 _soll73['herstellbar']))

        def _zahlen73(text):
            # „670 der 738", „670 of 738", „670 von 738" — beide Zahlen.
            paare = set()
            for _m in _re73.finditer(
                    r'\*\*([\d.,]+)\s+(?:der|von|of(?: the)?)\s+([\d.,]+)\*\*',
                    text):
                paare.add((int(_m.group(1).replace('.', '').replace(',', '')),
                           int(_m.group(2).replace('.', '').replace(',', ''))))
            einzeln = set()
            for _m in _re73.finditer(r'\*\*([\d][\d.,]{2,})\*\*', text):
                einzeln.add(int(_m.group(1).replace('.', '').replace(',', '')))
            return paare, einzeln

        for _name73 in ('README.md', 'README.en.md'):
            _txt73 = open(os.path.join(WURZEL, _name73), encoding='utf-8').read()
            _paare73, _einzeln73 = _zahlen73(_txt73)
            _falsch73 = [pa for pa in _paare73
                         if pa != (_soll73['herkunft'], _soll73['baupläne'])]
            pruefe(not _falsch73,
                   '%s: „X von Y Bauplaenen" stimmt (%s)'
                   % (_name73, _falsch73 or 'alles aktuell'))
            # Die Zahl der herstellbaren Gegenstaende steht allein da.
            _herst73 = [z for z in _einzeln73 if 1000 <= z <= 5000]
            pruefe(all(z == _soll73['herstellbar'] for z in _herst73),
                   '%s: die Zahl der herstellbaren Gegenstaende stimmt (%s)'
                   % (_name73, sorted(_herst73) or 'keine genannt'))

    # ------------------------------------------------------------------
    # 74. `SC_BP_NO_NET` gilt ueberall
    #
    # ⚠ Die Anleitung verspricht: „Beides laesst sich mit `SC_BP_NO_NET=1`
    # abschalten." Bis rc42 stimmte das nur zur Haelfte — Katalog, Preise,
    # Orte, Serverstatus und Update-Frage hielten sich daran, die
    # Uebersetzungsquellen und die Auftragsdaten des SCDL-Teams nicht. Wer die
    # Schalterstellung ernst nimmt, muss sich darauf verlassen koennen.
    #
    # Ausgenommen ist einzig `bericht.py`: Es sendet nur, wenn jemand den Knopf
    # drueckt, und sagt dabei selbst, was es tut.
    print()
    print('74. Netzabrufe halten sich an SC_BP_NO_NET')
    _ausnahmen74 = {'bericht.py'}       # nur auf Knopfdruck, siehe oben
    _offen74 = []
    for _name74 in sorted(os.listdir(os.path.join(WURZEL, 'scbp'))):
        if not _name74.endswith('.py') or _name74 in _ausnahmen74:
            continue
        _q74 = open(os.path.join(WURZEL, 'scbp', _name74),
                    encoding='utf-8').read()
        if 'urlopen(' not in _q74:
            continue
        if 'AUS' not in _q74 and 'SC_BP_NO_NET' not in _q74:
            _offen74.append(_name74)
    pruefe(not _offen74,
           'jedes Modul mit Netzabruf kennt den Schalter (%s)'
           % (', '.join(_offen74) or 'alle'))

    # ------------------------------------------------------------------
    # 75. Verweise gehen ueber EINEN Weg — und der wäscht die Umgebung
    #
    # ⚠⚠ Gemeldet am 30.08.2026: „Kaffee spendieren" und „Discord" taten
    # **nichts**. Kein Fehler im Bericht, keine Ausnahme — `webbrowser.open()`
    # meldet Erfolg, sobald es ein Programm gestartet hat, und im AppImage
    # stirbt genau dieses Programm sofort an unseren Bibliothekspfaden.
    #
    # Die Hälfte der Verweise hatte die Umgebungswaesche schon, die andere
    # nicht. Deshalb geht jetzt **alles** durch `pfade.im_browser` — und hier
    # wird nachgezaehlt, dass kein `webbrowser.open()` daran vorbei geht.
    #
    # ⚠ Geprueft wird **ohne** irgendetwas zu oeffnen: `browser_befehle` liefert
    # nur die Liste, und `im_browser` bekommt ein untergeschobenes `Popen`.
    print()
    print('75. Verweise gehen ueber pfade.im_browser')
    from scbp import pfade as _pf75

    # ⚠ Über den Syntaxbaum, nicht über Textsuche: In den Kommentaren steht
    # `webbrowser.open()` mehrfach als Begründung, warum es NICHT benutzt wird.
    # Eine Textsuche meldet genau die Erklärung als Verstoß.
    import ast as _ast75
    _direkt75 = []
    for _name75 in sorted(os.listdir(os.path.join(WURZEL, 'scbp'))):
        if not _name75.endswith('.py') or _name75 == 'pfade.py':
            continue
        _baum75 = _ast75.parse(open(os.path.join(WURZEL, 'scbp', _name75),
                                    encoding='utf-8').read())
        for _k75 in _ast75.walk(_baum75):
            if (isinstance(_k75, _ast75.Call)
                    and isinstance(_k75.func, _ast75.Attribute)
                    and _k75.func.attr == 'open'
                    and isinstance(_k75.func.value, _ast75.Name)
                    and _k75.func.value.id == 'webbrowser'):
                _direkt75.append('%s:%d' % (_name75, _k75.lineno))
    pruefe(not _direkt75,
           'kein Modul ruft webbrowser.open() direkt (%s)'
           % (', '.join(_direkt75) or 'alle über pfade.im_browser'))

    _befehle75 = _pf75.browser_befehle('https://example.invalid/x')
    if sys.platform.startswith('linux'):
        pruefe(_befehle75 and _befehle75[0][0] == 'xdg-open',
               'unter Linux wird zuerst xdg-open versucht (%s)' % _befehle75)
    else:
        pruefe(True, 'Befehlsliste für %s: %s' % (sys.platform, _befehle75))

    # Und die Umgebung, mit der gestartet wird, darf unsere Pfade nicht tragen.
    class _Lauf75:
        def __init__(self, *_a, **_kw):
            _Lauf75.gesehen = _kw.get('env') or {}

        def wait(self, _zeit):
            return 0

    import subprocess as _sp75
    _echt75 = _sp75.Popen
    _altumg75 = dict(os.environ)
    try:
        os.environ['LD_LIBRARY_PATH'] = '/pfad/im/appimage'
        os.environ['PYTHONHOME'] = '/pfad/im/appimage'
        _sp75.Popen = _Lauf75
        _geklappt75 = _pf75.im_browser('https://example.invalid/x')
    finally:
        _sp75.Popen = _echt75
        os.environ.clear()
        os.environ.update(_altumg75)

    if sys.platform.startswith('linux'):
        pruefe(_geklappt75, 'ein laufender Öffner gilt als Erfolg')
        _umg75 = getattr(_Lauf75, 'gesehen', {})
        pruefe('LD_LIBRARY_PATH' not in _umg75 and 'PYTHONHOME' not in _umg75,
               'der Öffner bekommt eine saubere Umgebung (%d Variablen, '
               'ohne unsere Pfade)' % len(_umg75))
    else:
        print('  [–]    Umgebungsprobe nur unter Linux sinnvoll')

    # ------------------------------------------------------------------
    # 76. „Protokolle neu einlesen" darf nicht die Einrichtung zuruecksetzen
    #
    # ⚠⚠ Gemeldet am 30.08.2026, und es kostete beinahe die Veroeffentlichung:
    # Nach einem Klick auf „alte Protokolle neu einlesen" kam beim naechsten
    # Start der **komplette Einrichtungsassistent** — bei einem Werkzeug, das
    # seit Wochen eingerichtet war. Wer ihn dann zumachte, hatte gar nichts
    # mehr: Das Programm beendete sich wortlos, ohne Overlay, ohne Meldung, und
    # im Fehlerbericht stand keine Zeile.
    #
    # Zwei Fehler in einer Kette:
    #   a) `noetig()` nahm das Fehlen von `logstand.json` als „erster Start" —
    #      dabei ist das der **Lesestand**, und genau den loescht der Knopf.
    #   b) Abbrechen beendete das Programm **immer**, nicht nur beim ersten Mal.
    print()
    print('76. Der Lesestand ist kein Einrichtungsmerkmal')
    from scbp import assistent as _as76
    from scbp import pfade as _pf76

    _heim76 = os.environ.get('SC_BP_HOME')
    _ordner76 = os.path.join(basis, 'einrichtung76')
    _spiel76 = os.path.join(basis, 'spiel76')
    os.makedirs(_ordner76, exist_ok=True)
    os.makedirs(_spiel76, exist_ok=True)
    # ⚠ Der eingetragene Ordner muss eine **echte** `Game.log` enthalten —
    # `pfade.spiel_ordner()` prueft das und raet nicht. Ein leerer Wegwerf-Ordner
    # reicht nicht: Auf dem Entwicklungsrechner sprang dann die automatische
    # Suche ein und fand die echte Installation, im Bau-Laeufer nicht. Die
    # Pruefung war gruen, wo sie nichts prueft, und rot, wo sie zaehlt.
    with open(os.path.join(_spiel76, 'Game.log'), 'w', encoding='utf-8') as _f76:
        _f76.write('')
    _wurzeln76 = _pf76._spiel_wurzeln
    _pf76._spiel_wurzeln = lambda: []      # keine automatische Suche dazwischen
    try:
        os.environ['SC_BP_HOME'] = _ordner76
        # Ein eingerichtetes Werkzeug: Spielordner eingetragen, Lesestand da.
        _pf76.einstellung_setzen('spiel_ordner', _spiel76)
        with open(_pf76.app_datei('logstand.json'), 'w', encoding='utf-8') as _f76:
            _f76.write('{}')
        pruefe(not _as76.noetig(),
               'ein eingerichtetes Werkzeug meldet keinen Assistenten')

        # Und jetzt genau das, was der Knopf tut.
        os.remove(_pf76.app_datei('logstand.json'))
        pruefe(_as76.eingerichtet(),
               'ohne Lesestand gilt es weiterhin als eingerichtet')
        pruefe(not _as76.noetig(),
               'nach „Protokolle neu einlesen" kommt KEIN Assistent')

        # Gegenprobe: ein wirklich frischer Ordner meldet ihn sehr wohl.
        _frisch76 = os.path.join(basis, 'frisch76')
        os.makedirs(_frisch76, exist_ok=True)
        os.environ['SC_BP_HOME'] = _frisch76
        pruefe(_as76.noetig(),
               'beim echten ersten Start meldet er sich weiterhin')
    finally:
        _pf76._spiel_wurzeln = _wurzeln76
        if _heim76 is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _heim76

    # b) Abbrechen darf nur beim echten ersten Start beenden.
    _q76 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'), encoding='utf-8').read()
    pruefe('if not fertig and not assistent.eingerichtet():' in _q76,
           'Abbrechen beendet nur, wenn noch nichts eingerichtet ist')
    # ⚠ Nicht alle `sys.exit(0)` zaehlen — der zweite ist die zweite Instanz,
    # die dem laufenden Fenster Bescheid sagt und sich dann verabschiedet. Der
    # gehoert dahin. Geprueft wird der Ausstieg des Assistenten, an seiner Spur.
    pruefe(_q76.count('Assistent abgebrochen — erster Start, Ende') == 1,
           'und der Abbruch hinterlaesst eine Spur, bevor er beendet')
    pruefe('Assistent abgebrochen — weiter mit dem Overlay' in _q76,
           'ein Abbruch mit vorhandener Einrichtung wird ebenfalls vermerkt')

    # ------------------------------------------------------------------
    # 77. Verschickt wird, was im Kasten steht
    #
    # ⚠⚠ Gemeldet am 30.08.2026 von **Morkhan (KRT)**: „bei mir stehts drin,
    # aber wenn ichs verschicke wohl nicht." Er hatte seinen Namen eingetragen,
    # der Kasten zeigte ihn — im abgesendeten Bericht stand trotzdem
    # „nicht angegeben".
    #
    # Ursache: Alle vier Knoepfe arbeiteten mit `text`, dem Bericht vom
    # **Oeffnen der Seite**. Das Nachzeichnen des Kastens aenderte nur die
    # Anzeige. Der Kasten verspricht „Du siehst vorher genau, was du
    # verschickst" — dann darf darunter nichts anderes rausgehen.
    print()
    print('77. Verschickt wird, was im Kasten steht')
    import ast as _ast77
    _q77 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                encoding='utf-8').read()
    _baum77 = _ast77.parse(_q77)
    _versand77 = {'issue_oeffnen', 'in_die_ablage', 'speichern', 'absenden'}
    _falsch77 = []
    _richtig77 = 0
    for _k77 in _ast77.walk(_baum77):
        if not (isinstance(_k77, _ast77.Call)
                and isinstance(_k77.func, _ast77.Attribute)
                and _k77.func.attr in _versand77
                and isinstance(_k77.func.value, _ast77.Name)
                and _k77.func.value.id == 'bericht'):
            continue
        if not _k77.args:
            continue
        erst = _k77.args[0]
        # Erlaubt ist nur der frisch geholte Text aus dem Kasten.
        ok = (isinstance(erst, _ast77.Call)
              and isinstance(erst.func, _ast77.Name)
              and erst.func.id == 'aktueller_bericht')
        if ok:
            _richtig77 += 1
        else:
            _falsch77.append('%s(...) in Zeile %d' % (_k77.func.attr,
                                                      _k77.lineno))
    # ⚠ Drei seit dem 05.09.2026: „Als Datei speichern" und „Eigenen Ordner
    # oeffnen" sind gestrichen — in ueber einem Jahr hat sie niemand benutzt.
    # Uebrig sind Absenden, Melden und Kopieren, und jeder von ihnen muss den
    # Text weiterhin FRISCH aus dem Kasten holen: Sonst verschickt jemand einen
    # Bericht ohne den Satz, den er gerade eingetippt hat.
    pruefe(_richtig77 >= 3,
           'alle Knoepfe holen den Text aus dem Kasten (%d gefunden)'
           % _richtig77)
    pruefe(not _falsch77,
           'keiner nimmt eine aeltere Fassung (%s)'
           % (', '.join(_falsch77) or 'keiner'))
    pruefe('melder_uebernehmen()' in _q77,
           'und der eingetippte Name wird vorher uebernommen')

    # ------------------------------------------------------------------
    # 78. Die Angaben im Spiel duerfen die Erkennung nicht vergiften
    #
    # ⚠⚠ Der schwerste Fund des Tages, gemeldet von **Morkhan (KRT)**:
    # Das Werkzeug schreibt Klasse, Groesse und Guetegrad an die
    # Gegenstandsnamen im Spiel. Schaltet man danach frei, steht in der
    # `Game.log` „Balandin (S3 B Military)" statt „Balandin" — und genau das
    # wurde gespeichert. Der Katalog kennt den Namen nicht, der Bauplan galt
    # als **nicht vorhanden**, der Fortschritt blieb zu niedrig. Bei ihm zwoelf
    # Stueck, und mit jedem neuen Fund einer mehr.
    #
    # ⚠ Die Klammer darf NICHT blind abgeschnitten werden: 39 Katalognamen
    # tragen selbst eine („A03 Sniper Rifle Magazine (15 cap)").
    print()
    print('78. Angaben im Namen verderben den Abgleich nicht')
    from scbp import bestand as _bd78
    from scbp import katalog as _ka78

    _heim78 = os.environ.get('SC_BP_HOME')
    _ordner78 = os.path.join(basis, 'angleich78')
    os.makedirs(_ordner78, exist_ok=True)
    try:
        os.environ['SC_BP_HOME'] = _ordner78
        # Ein kleiner eigener Katalog — kein Netz, keine Nutzerdaten.
        # ⚠ Die Schluessel mit `norm()` bilden, nicht von Hand tippen: Es
        # kuerzt mehr als nur Kleinschreibung (aus „(15 cap)" wird „(15)").
        # Ein selbst getippter Schluessel passt dann nirgends, und die Pruefung
        # misst am Ende nur ihren eigenen Tippfehler.
        _kat78 = {'format': 2, 'stand': 'test', 'bauplaene': {
            _bd78.norm('Balandin'): {'n': 'Balandin', 'a': 'WeaponGun'},
            _bd78.norm('Cirrus'): {'n': 'Cirrus', 'a': 'WeaponGun'},
            _bd78.norm('A03 Sniper Rifle Magazine (15 cap)'): {
                'n': 'A03 Sniper Rifle Magazine (15 cap)',
                'a': 'Ammunition'}}}
        with open(_ka78.pfad() if hasattr(_ka78, 'pfad')
                  else os.path.join(_ordner78, 'katalog-cache.json'),
                  'w', encoding='utf-8') as _f78:
            json.dump(_kat78, _f78)
        _ka78.vergessen() if hasattr(_ka78, 'vergessen') else None

        pruefe(bool(_ka78.laden().get('bauplaene')),
               'der Testkatalog wird gelesen')

        # a) Beim Eintragen wird der Anhang abgeschnitten …
        _d78 = _bd78.leer()
        _bd78.hinzufuegen(_d78, 'Balandin (S3 B Military)', 'log')
        pruefe(_bd78.norm('Balandin') in _d78['bauplaene'],
               'ein Bauplan mit angehaengten Angaben landet unter seinem '
               'Katalognamen')

        # b) … aber nur, wenn die Klammer die Ursache ist.
        _bd78.hinzufuegen(_d78, 'A03 Sniper Rifle Magazine (15 cap)', 'log')
        pruefe(_bd78.norm('A03 Sniper Rifle Magazine (15 cap)')
               in _d78['bauplaene'],
               'ein Katalogname MIT Klammer bleibt unangetastet')

        # c) Was der Katalog gar nicht kennt, bleibt wie gefunden.
        _bd78.hinzufuegen(_d78, 'Voellig Unbekannt (Irgendwas)', 'log')
        pruefe(_bd78.norm('Voellig Unbekannt (Irgendwas)')
               in _d78['bauplaene'],
               'ein unbekannter Name wird nicht auf Verdacht gekuerzt')

        # d) Und der alte Stand wird nachtraeglich angeglichen.
        _alt78 = _bd78.leer()
        _alt78['bauplaene']['balandin (s3 b military)'] = {
            'name': 'Balandin (S3 B Military)', 'quelle': 'log',
            'zeit': '2026-08-01'}
        _alt78['bauplaene']['cirrus (s2 c stealth)'] = {
            'name': 'Cirrus (S2 C Stealth)', 'quelle': 'log',
            'zeit': '2026-08-02'}
        _zahl78 = _bd78.angleichen(_alt78)
        pruefe(_zahl78 == 2 and sorted(_alt78['bauplaene'])
               == sorted([_bd78.norm('Balandin'), _bd78.norm('Cirrus')]),
               'ein alter Stand wird beim Start angeglichen (%d berichtigt)'
               % _zahl78)
    finally:
        if _heim78 is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _heim78
        _ka78.vergessen() if hasattr(_ka78, 'vergessen') else None

    _q78 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'), encoding='utf-8').read()
    pruefe('bestand_datei.angleichen(self.bestand)' in _q78,
           'und der Watcher stoesst das beim Start an')

    # ------------------------------------------------------------------
    # 79. Altnamen zuordnen — aber nur, wenn es eindeutig ist
    #
    # ⚠ Die Uebersetzung benennt Gegenstaende gelegentlich um. Wer den Bauplan
    # vorher bekommen hat, traegt den alten Namen fuer immer im Bestand:
    # `BlackFire Racing Flight Suit`, waehrend der Katalog heute
    # `Neutrino Racing Flight Suit BlackFire` sagt. In der echten deutschen
    # `global.ini` kommt die alte Wortstellung **0 mal** vor, die neue 2 mal —
    # es ist also ein Altbestand, kein aktueller Fehler.
    #
    # ⚠⚠ **Hier darf nicht geraten werden.** `Parallax` allein steckt in fuenf
    # Katalognamen. Ein falsch zugeordneter Bauplan ist schlimmer als ein offen
    # ausgewiesener — deshalb: genau ein Treffer, sonst gar keiner.
    print()
    print('79. Altnamen nur bei Eindeutigkeit zuordnen')
    _heim79 = os.environ.get('SC_BP_HOME')
    _ordner79 = os.path.join(basis, 'altnamen79')
    os.makedirs(_ordner79, exist_ok=True)
    try:
        os.environ['SC_BP_HOME'] = _ordner79
        _kat79 = {'format': 2, 'stand': 'test', 'bauplaene': {}}
        for _n79 in ('Neutrino Racing Flight Suit BlackFire',
                     'Neutrino Racing Helmet BlackFire',
                     'Parallax Energy Assault Rifle',
                     'Parallax "Sanguine" Energy Assault Rifle',
                     'Tailwind Flight Suit'):
            _kat79['bauplaene'][_bd78.norm(_n79)] = {'n': _n79, 'a': 'Armor'}
        with open(os.path.join(_ordner79, 'katalog-cache.json'),
                  'w', encoding='utf-8') as _f79:
            json.dump(_kat79, _f79)
        _ka78.vergessen() if hasattr(_ka78, 'vergessen') else None

        pruefe(_bd78.katalogname('BlackFire Racing Flight Suit')
               == 'Neutrino Racing Flight Suit BlackFire',
               'ein eindeutiger Altname wird zugeordnet')
        pruefe(_bd78.katalogname('Parallax') == 'Parallax',
               'ein mehrdeutiger Altname bleibt stehen (Parallax passt auf zwei)')
        pruefe(_bd78.katalogname('Tailwind Flight Suit') == 'Tailwind Flight Suit',
               'ein Name, den es genau so gibt, wird nicht angefasst')
        pruefe(_bd78.katalogname('Voellig Fremder Gegenstand')
               == 'Voellig Fremder Gegenstand',
               'ein Name ohne jeden Treffer bleibt, wie er ist')
    finally:
        if _heim79 is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _heim79
        _ka78.vergessen() if hasattr(_ka78, 'vergessen') else None

    # ------------------------------------------------------------------
    # 80. Anfuehrungszeichen duerfen Namen nicht trennen
    #
    # ⚠⚠ Bis zum 30.08.2026 fehlte in `pfade.ANFUEHRUNG` ausgerechnet das
    # **oeffnende** typografische Anfuehrungszeichen. Aus
    # `SW16BR1 “Buzzsaw” Repeater` wurde `sw16br1 “buzzsaw' repeater` — das
    # schliessende angeglichen, das oeffnende nicht. Drei Katalog-Bauplaene
    # tragen es; keiner konnte je zu einem Fund aus einer anderen Quelle passen.
    #
    # Aufgefallen ist es beim Abgleich einer von Hand gefuehrten Liste gegen den
    # Katalog — **nicht** durch eine Meldung. Der Bauplan gilt einfach als
    # „fehlt", und niemand vermutet ein Anfuehrungszeichen dahinter.
    print()
    print('80. Anfuehrungszeichen trennen keine Namen')
    from scbp import pfade as _pf80

    _formen80 = ['SW16BR1 "Buzzsaw" Repeater']
    for _paar80 in (('\u201c', '\u201d'), ('\u2018', '\u2019'),
                    ('\u201e', '\u201c'), ('\u00ab', '\u00bb'),
                    ('\u2039', '\u203a'), ("'", "'")):
        _formen80.append('SW16BR1 %sBuzzsaw%s Repeater' % _paar80)
    _keys80 = {_pf80.namensform(_f80) for _f80 in _formen80}
    pruefe(len(_keys80) == 1,
           'alle Anfuehrungs-Schreibweisen ergeben denselben Schluessel (%d '
           'verschiedene: %s)' % (len(_keys80), sorted(_keys80)[:3]))

    # Und die Tabelle muss jedes Zeichen kennen, das ueberhaupt als
    # Anfuehrungszeichen auftreten kann — sonst faellt die naechste Luecke
    # genauso lange nicht auf.
    _fehlend80 = [c for c in '\u201c\u201d\u201e\u2018\u2019\u201a'
                            '\u00ab\u00bb\u2039\u203a"'
                  if ord(c) not in _pf80.ANFUEHRUNG]
    pruefe(not _fehlend80,
           'die Tabelle kennt alle gaengigen Anfuehrungszeichen (%s)'
           % (', '.join('U+%04X' % ord(c) for c in _fehlend80) or 'alle'))

    # ------------------------------------------------------------------
    # 81. Geschrieben wird in die Datei, die das Spiel LIEST
    #
    # ⚠⚠ Gemeldet am 29.08.2026: Bei der Textquelle „Original" nahm
    # `ini_datei()` eine feste Reihenfolge (`english`, dann `german_(germany)`)
    # und die erste vorhandene Datei. Beide gibt es fast immer — also immer
    # Englisch. Wer sein Spiel auf Deutsch stellt, bekam die Angaben in eine
    # Datei geschrieben, die das Spiel nie liest: eingetragen korrekt,
    # angekommen nichts, Statuszeile trotzdem gruen. Erklaert vermutlich
    # monatelang nicht ankommende Auftragstexte.
    #
    # Massgeblich ist `g_language` in der `user.cfg`. Das Werkzeug **schrieb**
    # die Zeile seit jeher — gelesen hat es sie nie.
    print()
    print('81. Die Spielsprache entscheidet ueber die Zieldatei')
    from scbp import injektion as _in81
    from scbp import uebersetzung as _ue81
    from scbp import pfade as _pf81

    _spiel81 = os.path.join(basis, 'spiel81', 'LIVE')
    for _s81 in ('english', 'german_(germany)'):
        _o81 = os.path.join(_spiel81, 'data', 'Localization', _s81)
        os.makedirs(_o81, exist_ok=True)
        with open(os.path.join(_o81, 'global.ini'), 'w', encoding='utf-8') as _f81:
            _f81.write('item_Name_test=Test\n')
    with open(os.path.join(_spiel81, 'Game.log'), 'w', encoding='utf-8') as _f81:
        _f81.write('')

    _altspiel81 = os.environ.get('SC_INSTALL_DIR')
    _altquelle81 = None
    try:
        os.environ['SC_INSTALL_DIR'] = _spiel81
        _altquelle81 = _pf81.einstellung('inj_quelle')
        _pf81.einstellung_setzen('inj_quelle', 'original')

        def _cfg81(wert):
            with open(os.path.join(_spiel81, 'user.cfg'), 'w',
                      encoding='utf-8') as _f:
                _f.write('g_languageAudio = english\n')
                if wert:
                    _f.write('g_language = %s\n' % wert)

        _cfg81('german_(germany)')
        pruefe(_ue81.spielsprache() == 'german_(germany)',
               'g_language wird aus der user.cfg gelesen (%s)'
               % _ue81.spielsprache())
        _pfad81, _spr81, _ = _in81.ini_datei()
        pruefe(_spr81 == 'german_(germany)',
               'deutsches Spiel -> deutsche global.ini (%s)' % _spr81)

        # Gegenprobe: englisches Spiel, dieselben zwei Dateien.
        _cfg81('english')
        _pfad81, _spr81, _ = _in81.ini_datei()
        pruefe(_spr81 == 'english',
               'englisches Spiel -> englische global.ini (%s)' % _spr81)

        # Ohne Eintrag bleibt es beim Rueckfall — ohne g_language startet
        # Star Citizen auf Englisch.
        _cfg81(None)
        pruefe(_ue81.spielsprache() is None,
               'ohne Eintrag meldet die Sprache sich als unbekannt')
        _pfad81, _spr81, _ = _in81.ini_datei()
        pruefe(_spr81 == 'english',
               'ohne Eintrag gilt der Rueckfall Englisch (%s)' % _spr81)
    finally:
        if _altquelle81 is None:
            _pf81.einstellung_setzen('inj_quelle', None)
        else:
            _pf81.einstellung_setzen('inj_quelle', _altquelle81)
        if _altspiel81 is None:
            os.environ.pop('SC_INSTALL_DIR', None)
        else:
            os.environ['SC_INSTALL_DIR'] = _altspiel81

    # ------------------------------------------------------------------
    # 82. Ruf-Obergrenze: was man sich verbauen kann
    #
    # ⚠⚠ 280 der 353 Auftraege haben eine Ruf-OBERGRENZE (`maxStanding`).
    # Steigt der Ruf bei der Fraktion darueber, wird der Auftrag **nicht mehr
    # angeboten** — und seine Bauplaene sind fuer diesen Spielstand weg. Im
    # Spiel steht das nirgends, und man merkt es erst, wenn es zu spaet ist.
    #
    # ⚠ Der EIGENE Ruf steht nicht in der `Game.log` — am 30.08.2026 ueber 22
    # Protokolle nachgemessen: `reputation` kommt dort ausschliesslich als
    # Verbindungszeile zu CIGs Dienst vor, nie ein Wert. Deshalb sagt die
    # Auskunft „ab wann zu", nicht „dir bleiben noch 4.200".
    #
    # Die Regel, die hier abgesichert wird: **Ein offener Weg genuegt.** Fuehren
    # fuenf Auftraege zu einem Bauplan und einer davon hat keine Obergrenze,
    # ist nichts in Gefahr.
    print()
    print('82. Ruf-Obergrenze wird nur bei echter Gefahr gemeldet')
    from scbp import katalog as _ka82

    _kat82 = {'bauplaene': {}, 'missionen': {
        'nur_gedeckelt': {'bp': ['Testteil A'], 'rep_max': 15000,
                          'rang_max': 'Veteran Contractor'},
        'auch_gedeckelt': {'bp': ['Testteil A', 'Testteil B'],
                           'rep_max': 95250, 'rang_max': 'Elite Contractor'},
        'ohne_deckel': {'bp': ['Testteil B']},
    }}
    _a82 = _ka82.ruf_deckel(_kat82, _ka82._norm('Testteil A'))
    pruefe(_a82 == (95250, 'Elite Contractor'),
           'alle Wege gedeckelt -> der grosszuegigste zaehlt (%s)' % (_a82,))
    pruefe(_ka82.ruf_deckel(_kat82, _ka82._norm('Testteil B')) is None,
           'ein Weg ohne Obergrenze genuegt -> keine Warnung')
    pruefe(_ka82.ruf_deckel(_kat82, _ka82._norm('Gibt es nicht')) is None,
           'ein Bauplan ohne Auftrag meldet nichts')

    # Und der Filter muss in der Liste angeboten werden.
    _q82 = open(os.path.join(WURZEL, 'scbp', 'bestandsfenster.py'),
                encoding='utf-8').read()
    pruefe("('deckel', t('filter_deckel'))" in _q82,
           'die Bauplan-Liste bietet den Filter an')
    pruefe("if self.filter == 'deckel':" in _q82,
           'und filtert danach')
    pruefe('drin or not katalog_modul.ruf_deckel' in _q82,
           'was man schon hat, taucht dabei nicht auf')

    # --- Die drei uebrigen Auskuenfte aus denselben Vertragsdaten ---
    #
    # ⚠ „Teilbar" nur, wenn ALLE Wege es sind: „den koennt ihr zu fuenft laufen"
    # darf nicht dastehen, wenn es fuer einen von vier Auftraegen gilt — dann
    # steht die Staffel am falschen Auftrag.
    _kat82['missionen']['nicht_teilbar'] = {'bp': ['Testteil C'],
                                            'teilbar': False, 'cooldown': 60}
    _kat82['missionen']['nur_gedeckelt']['teilbar'] = True
    _kat82['missionen']['nur_gedeckelt']['cooldown'] = 15
    _kat82['missionen']['auch_gedeckelt']['teilbar'] = True
    _kat82['missionen']['auch_gedeckelt']['cooldown'] = 240

    _m82 = _ka82.auftragsmerkmale(_kat82, _ka82._norm('Testteil A'))
    pruefe(_m82['teilbar'] is True and _m82['sperre'] == 15,
           'teilbar wenn alle Wege es sind, Sperre ist die kuerzeste (%s)'
           % _m82)
    _m82 = _ka82.auftragsmerkmale(_kat82, _ka82._norm('Testteil C'))
    pruefe(_m82['teilbar'] is False,
           'ein nicht teilbarer Weg genuegt fuer „nicht teilbar"')

    # „Was bringt am meisten" — gezaehlt wird ueber die Bezugsquellen der
    # Bauplaene, weil nur die den aufgeloesten Auftragstitel tragen.
    _kat82['bauplaene'] = {
        _ka82._norm('Fehlt A'): {'n': 'Fehlt A', 'q': [
            {'auftrag': 'Grosser Auftrag', 'fraktion': 'Testfraktion',
             'uec': 1000, 'rang': 'Contractor'}]},
        _ka82._norm('Fehlt B'): {'n': 'Fehlt B', 'q': [
            {'auftrag': 'Grosser Auftrag', 'fraktion': 'Testfraktion',
             'uec': 2000, 'rang': 'Contractor'}]},
        _ka82._norm('Habe ich'): {'n': 'Habe ich', 'q': [
            {'auftrag': 'Kleiner Auftrag', 'fraktion': 'Testfraktion',
             'uec': 500}]},
    }
    _lohnt82 = _ka82.lohnende_auftraege(_kat82, {_ka82._norm('Habe ich')})
    pruefe(len(_lohnt82) == 1 and _lohnt82[0][0] == 'Grosser Auftrag'
           and _lohnt82[0][2] == 2,
           'der Auftrag mit den meisten FEHLENDEN Bauplaenen steht oben (%s)'
           % (_lohnt82,))
    pruefe(_lohnt82[0][3] == 2000,
           'die hoechste Belohnung des Auftrags wird genannt (%s)'
           % _lohnt82[0][3])

    _q82b = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                 encoding='utf-8').read()
    pruefe('_lohnende_auftraege(fenster, innen, katalog, habe)' in _q82b,
           'die Fortschritt-Seite zeigt es an')

    # ------------------------------------------------------------------
    # 83. Raffinerie-Ausbeute abtippen
    #
    # ⚠ Der Raffinerie-Auftrag steht **nicht** in der `Game.log` — am
    # 30.08.2026 ueber 22 Protokolle nachgemessen: `Refinery` kommt 58-mal vor,
    # ausschliesslich als Ladezeile fuer die 3D-Modelle des Decks; `Aslarite`,
    # `Agricium` und `cSCU` **kein einziges Mal**. Automatisch geht also nichts,
    # und Bilderkennung braeuchte Zusatzpakete. Bleibt: das Abtippen ertraeglich
    # machen.
    #
    # ⚠⚠ Zwei Fallen, die hier abgesichert werden:
    #   a) **Die Zahlen von hinten lesen.** Fuenf der 52 einlagerbaren Namen
    #      haben Leerzeichen (`Heart of the Woods`). Wer am ersten Leerzeichen
    #      trennt, verliert sie alle.
    #   b) **cSCU ist die Voreinstellung.** Das Terminal rechnet so. Bei der
    #      falschen Annahme steht alles um den Faktor 100 daneben.
    print()
    print('83. Raffinerie-Ausbeute abtippen')
    from scbp import rohstoffe as _ro83
    from scbp import herstellung as _he83
    from scbp import bergbau as _bg83

    # ⚠ Die Liste der einlagerbaren Namen kommt aus Rezept- und Bergbaudaten —
    # beides Zwischenspeicher, die es im Wegwerf-Ordner nicht gibt. Ohne eigene
    # Daten waere hier jeder Name „unbekannt" und die Pruefung gruen, ohne
    # etwas geprueft zu haben. (Dieselbe Falle wie bei Pruefung 67 und 76.)
    _echt83 = _he83.einlagerbar
    _he83.einlagerbar = lambda: ['Titanium', 'Iron', 'Heart of the Woods']
    try:

        _text83 = ('Titanium 295 188\n'
                   'Heart of the Woods 500 12\n'
                   '\n'
                   'Quatsch 100 5\n'
                   'Iron 1200 3\n'
                   'Iron 500\n'
                   'Iron 500 0')
        _posten83, _fehl83 = _ro83.raffinerie_zeilen(_text83)
        pruefe(len(_posten83) == 2,
               'die gueltigen Zeilen kommen durch (%d)' % len(_posten83))
        pruefe(('Heart of the Woods', 0.12, 500) in _posten83,
               'ein Name mit Leerzeichen bleibt heil (%s)' % (_posten83,))
        pruefe(('Titanium', 1.88, 295) in _posten83,
               'cSCU wird in SCU umgerechnet (188 -> 1.88)')
        pruefe(len(_fehl83) == 4,
               'jede kaputte Zeile wird einzeln gemeldet (%d)' % len(_fehl83))

        _scu83, _ = _ro83.raffinerie_zeilen('Titanium 295 188', 'scu')
        pruefe(_scu83 == [('Titanium', 188.0, 295)],
               'in SCU bleibt die Zahl, wie sie ist (%s)' % (_scu83,))

    finally:
        _he83.einlagerbar = _echt83

    _q83 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                encoding='utf-8').read()
    pruefe('_raffinerie_block(fenster, innen, lager, ort, zeichnen, meldung)'
           in _q83, 'die Lager-Seite bietet die Maske an')

    # 84. Verkauf — wo die Ware hin soll
    #
    # ⚠⚠ Diese Pruefung arbeitet mit **eingeschleusten** Daten, nicht mit einem
    # echten Abruf. Zwei Gruende: Ohne Netz waere sie sonst stumm gruen (die
    # Falle aus Pruefung 83), und die beiden Faellen unten lassen sich nur mit
    # bekannten Zahlen belegen.
    print()
    print('84. Verkauf — wo die Ware hin soll')
    from scbp import verkauf as _vk84
    from scbp import handelslager as _hl84

    # ⚠ `format` und `geholt` setzt die Ablage selbst (siehe `scbp/uex.py`) —
    # hier stehen nur die eigenen Felder.
    _vk84._ablage.sichern({
        'terminals': {'1': {'o': 'Area 18', 's': 'Stanton', 'q': 0},
                      '2': {'o': 'GrimHEX', 's': 'Stanton', 'q': 1},
                      '3': {'o': 'Ashland', 's': 'Pyro', 'q': 0}},
        'waren': {
            # Ein Ort, der alles nimmt, aber schlechter zahlt (1), gegen einen,
            # der nur eine Ware nimmt und dafuer das Doppelte bietet (3).
            'Gold':         [{'t': '3', 'n': 'Teuer', 'p': 90000, 'd': 0, 'k': ''},
                             {'t': '1', 'n': 'Alles', 'p': 30000, 'd': 0, 'k': ''},
                             {'t': '2', 'n': 'Heiss', 'p': 20000, 'd': 0, 'k': ''}],
            'Copper':       [{'t': '1', 'n': 'Alles', 'p': 4400, 'd': 0, 'k': ''},
                             {'t': '2', 'n': 'Heiss', 'p': 3000, 'd': 0, 'k': ''}],
            'Iron':         [{'t': '1', 'n': 'Alles', 'p': 3400, 'd': 0, 'k': ''}],
            # Die beiden Fallen, als Daten:
            'Copper (Ore)': [{'t': '1', 'n': 'Alles', 'p': 1200, 'd': 0, 'k': ''}],
            'Golden Medmon': [{'t': '1', 'n': 'Alles', 'p': 71000, 'd': 0,
                               'k': ''}],
        }})

    # ⭐ Falle 1: UEX filtert Warennamen als Teiltext. Wer `Gold` sucht, bekommt
    # dort `Golden Medmon` mit — und dessen Preis sieht aus wie ein
    # sagenhaftes Goldgebot. Hier muss exakt verglichen werden.
    _treffer84 = [tr['ware'] for e in _vk84.orte_fuer(['Gold'])
                  for tr in e['treffer']]
    pruefe(_treffer84 and set(_treffer84) == {'Gold'},
           'Gold liefert nicht Golden Medmon mit (%s)' % sorted(set(_treffer84)))

    # ⭐ Falle 2: Erz und veredelte Ware sind verschiedene Waren mit
    # verschiedenen Preisen. `norm_rohstoff()` wuerfe sie zusammen.
    pruefe(_vk84.bester_preis('Copper') == 4400
           and _vk84.bester_preis('Copper (Ore)') == 1200,
           'Copper und Copper (Ore) bleiben getrennt')

    # ⚠ Nicht auf das Wort pruefen — es steht als **Warnung** im Kopf und in
    # den Kommentaren, und das soll es auch. Geprueft wird, ob die Funktion
    # ueberhaupt erreichbar ist: ohne Import aus `herstellung` kann sie nicht
    # benutzt werden.
    _q84 = open(os.path.join(WURZEL, 'scbp', 'verkauf.py'),
                encoding='utf-8').read()
    pruefe('from .herstellung import' not in _q84
           and 'import herstellung' not in _q84,
           'verkauf.py kann norm_rohstoff gar nicht erreichen')

    # ⭐ Der Kern des Reiters: mehr abgenommene Waren schlagen den hoeheren
    # Preis. Gemessen am 30.08.2026 bringt der Umweg ueber mehrere Terminals
    # nur 2 % mehr — dafuer aber zwei zusaetzliche Anfluege.
    _orte84 = _vk84.orte_fuer(['Gold', 'Copper', 'Iron'])
    pruefe(_orte84[0]['terminal'] == 'Alles' and _orte84[0]['anzahl'] == 3,
           'der Ort mit den meisten Waren steht oben (%s)'
           % _orte84[0]['terminal'])
    pruefe(_orte84[1]['terminal'] == 'Heiss',
           'danach wird nach Preis sortiert (%s)' % _orte84[1]['terminal'])

    _heiss84 = _vk84.orte_fuer(['Gold', 'Copper', 'Iron'], nur_nqa=True)
    pruefe([e['terminal'] for e in _heiss84] == ['Heiss'],
           'gestohlene Ware sieht nur Orte ohne Fragen (%s)'
           % [e['terminal'] for e in _heiss84])

    # Die Stundensperre. ⚠ Gegenprobe gegen den alten Stand: Ohne sie waere
    # `aktualisieren(erzwingen=True)` sofort wieder durchgelaufen.
    pruefe(_vk84.wartezeit() > 0, 'nach dem Abruf laeuft die Sperre')

    # ⚠⚠ **Mit Gegenprobe.** Im Selbsttest ist der Netzzugriff abgeschaltet
    # (`AUS`) — die Pruefung waere also auch dann gruen gewesen, wenn die
    # Sperre gar nicht griffe, nur eben mit dem Grund `'aus'`. Deshalb wird
    # `AUS` hier abgeschaltet und der Abruf durch eine Falle ersetzt: Kommt die
    # Anfrage trotz Sperre durch, fliegt sie und die Pruefung faellt durch.
    #
    # ⚠ Seit dem gemeinsamen Unterbau haengt der Abruf an `uex.holen` — die
    # Falle gehoert also dorthin, nicht mehr an ein `_holen` in `verkauf`.
    from scbp import uex as _uex84

    def _falle84(*_a, **_k):
        raise AssertionError('trotz Sperre ins Netz gegriffen')

    _aus84, _echt84 = _vk84.AUS, _uex84.holen
    _vk84.AUS, _uex84.holen = False, _falle84
    try:
        _ergebnis84 = _vk84.aktualisieren(erzwingen=True)
    except AssertionError:
        _ergebnis84 = ('ins Netz gegriffen',)
    finally:
        _vk84.AUS, _uex84.holen = _aus84, _echt84
    pruefe(_ergebnis84 == (False, 'gesperrt'),
           'der Knopf laesst sich nicht zweimal druecken (%s)' % (_ergebnis84,))

    # Das Handelslager: gleiche Stapel zusammen, markierte getrennt.
    _hl84.leeren()
    _hl84.eintragen('Gold', '100', 'Area 18')
    _hl84.eintragen('Gold', '50', 'Area 18')
    _hl84.eintragen('Gold', '20', 'Area 18', gestohlen=True)
    _posten84 = _hl84.laden()
    pruefe(len(_posten84) == 2 and _posten84[0]['menge'] == 150,
           'gleiche Posten werden zusammengezaehlt (%d Stapel)' % len(_posten84))
    pruefe(_hl84.mengen(nur_gestohlen=True) == {'Gold': 20.0},
           'markierte Ware ist ein eigener Stapel')
    pruefe(_hl84.eintragen('Gold', 'abc')[1] == 'menge'
           and _hl84.eintragen('', '5')[1] == 'ware',
           'unsinnige Eingaben werden abgewiesen')

    # ⚠ **Keine negativen Mengen und keine Null.** Ein Laderaum mit „-40 SCU"
    # ergibt keinen Sinn, und `zahl_lesen` laesst das Minus bewusst durch (im
    # Werkstatt-Lager wird damit abgebucht). Hier muss es also abgefangen
    # werden — auch das lange Minus vom Ziffernblock.
    _vorher84 = len(_hl84.laden())
    pruefe(all(_hl84.eintragen('Gold', wert)[1] == 'menge'
               for wert in ('-40', '-40', '0', '-0,5')),
           'negative Mengen und Null werden abgewiesen')
    pruefe(len(_hl84.laden()) == _vorher84,
           'und es landet nichts davon im Lager')

    # ⭐ Der Rechner im Mengenfeld — dasselbe wie im Werkstatt-Lager.
    pruefe(_hl84.eintragen('Copper', '100+5')[0]
           and _hl84.mengen()['Copper'] == 105.0,
           'im Mengenfeld darf gerechnet werden (100+5)')
    pruefe(_hl84.eintragen('Iron', '100-40')[0]
           and _hl84.mengen()['Iron'] == 60.0,
           'auch mit Minus (100-40 ergibt 60)')

    # ⚠ **Beim Aendern zaehlt die bisherige Menge als Ausgangswert**: `-5` ist
    # dort eine Buchung („fuenf abbuchen"), kein Fehler. Abgewiesen wird erst,
    # wenn das **Ergebnis** null oder kleiner waere.
    _nr84 = [i for i, p in enumerate(_hl84.laden())
             if p['ware'] == 'Gold' and not p['gestohlen']][0]
    _stand84 = _hl84.laden()[_nr84]['menge']
    pruefe(_hl84.aendern(_nr84, 'Gold', '-5')[0]
           and _hl84.laden()[_nr84]['menge'] == _stand84 - 5,
           'beim Aendern bucht -5 ab (%s -> %s)'
           % (_stand84, _hl84.laden()[_nr84]['menge']))
    pruefe(_hl84.aendern(_nr84, 'Gold', '-9999')[1] == 'menge',
           'aber nicht unter null')
    _hl84.leeren()

    # ⚠ **Beide Felder sperren gleich.** Ware und Lagerort kommen aus
    # geschlossenen Listen — sonst steht in dem einen Feld eine Liste und im
    # anderen darf jeder tippen, was er will.
    _q84s = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                 encoding='utf-8').read()
    _hlseite84 = _q84s.split('def _handelslager(')[-1]
    pruefe('preisdaten.bekannt(name)' in _hlseite84,
           'die Ware wird gegen die Warenliste geprueft')
    pruefe('ortsliste.kennt(ort.get())' in _hlseite84,
           'der Lagerort wird gegen die Ortsliste geprueft')
    # ⚠ Seit dem Auswahlfeld gibt es keine „Meintest du"-Zeile mehr: Das Feld
    # filtert beim Tippen selbst und laesst sich per Pfeil ganz aufklappen.
    # Geprueft wird deshalb, dass **beide** Felder ihre geschlossene Liste
    # bekommen — Waren aus den Preisdaten, Orte aus der Ortsliste.
    pruefe('_auswahlfeld(fenster, block, var, quelle)' in _hlseite84
           and 'preisdaten.waren if var is ware else ortsliste.alle'
           in _hlseite84,
           'Ware und Ort sind Auswahlfelder mit geschlossener Liste')
    # ⚠ Nicht auf das Wort pruefen — es steht als **Warnung** im Kopf des
    # Bausteins, und das soll es auch. Geprueft wird der Import: ohne ihn kann
    # kein Systemelement benutzt werden. (Dieselbe Falle wie bei
    # `norm_rohstoff` weiter oben — beim ersten Anlauf prompt wieder getappt.)
    pruefe('import ttk' not in _q84s and 'from tkinter.ttk' not in _q84s,
           'kein ttk-Systemelement in der Oberflaeche')
    pruefe("'verkauf':     _verkauf," in _q84s
           and "'handelslager': _handelslager," in _q84s,
           'beide Seiten sind angemeldet')
    pruefe("self._reiter('verkauf', 'verkauf'" in open(
        os.path.join(WURZEL, 'scbp', 'hauptfenster.py'),
        encoding='utf-8').read(), 'der Reiter steht in der Leiste')

    # ⚠ Kein Rot im herunterzaehlenden Knopf: Der ist gesperrt, *weil* der
    # Abruf geklappt hat — Rot ist in diesem Programm die Fehlerfarbe.
    from scbp import seiten as _se84
    pruefe(_se84._warteton(59 * 60) == _se84.SUB
           and _se84._warteton(10 * 60) == _se84.GOLD
           and _se84._warteton(30) == _se84.ACCENT,
           'der Timer reift von grau ueber gold nach gruen')
    pruefe(_se84._warteton(59 * 60) != _se84.ROT
           and _se84._warteton(30) != _se84.ROT,
           'der Timer wird nie rot')
    pruefe(_se84._wartetext(3599) == '59:59' and _se84._wartetext(0) == '',
           'die Restzeit steht als mm:ss da (%s)' % _se84._wartetext(3599))

    # 85. Das Fenster passt auf den Bildschirm
    #
    # ⚠⚠ Am 30.08.2026 gemeldet: „das Einstellungsfenster ist zu gross, er
    # kommt nicht mehr an alles ran." Mit der Gruppe „Handel" brauchte die
    # Seitenleiste 1020 px; daraus wurde eine Mindesthoehe groesser als der
    # 1080er Bildschirm — und ein `minsize` haelt Tk gegen **jedes**
    # `geometry()`, auch gegen das Zurechtruecken beim Start.
    print()
    print('85. Das Fenster passt auf den Bildschirm')
    from scbp import bildschirm as _bs85
    from scbp import hauptfenster as _hf85

    # ⚠ **`Hauptfenster` legt ein eigenes Toplevel an** — `hf.root` ist nicht
    # das uebergebene Fenster. Wer die uebergebene Wurzel misst, liest immer
    # `minsize (1, 1)` und haelt die Pruefung faelschlich fuer gruen.
    _wurzel85 = _wurzel()
    _wurzel85.geometry('1160x760+0+0')
    _fenster85 = _hf85.Hauptfenster(_wurzel85)
    _echt_fenster85 = _fenster85.root
    _echt_fenster85.deiconify()
    for _ in range(8):
        _wurzel85.update()
        _wurzel85.update_idletasks()

    _bedarf85 = _fenster85._seitenleiste_bedarf()
    pruefe(_bedarf85 > 400,
           'die Seitenleiste braucht messbar Platz (%d px)' % _bedarf85)

    # ⭐ Ein **kleiner** Bildschirm wird vorgegaukelt. Ohne die Deckelung wuerde
    # die Mindesthoehe aus dem Leistenbedarf gesetzt und waere sofort groesser.
    # ⚠⚠ **Erst pruefen, ob die Rechnung ueberhaupt lief.** `_mindesthoehe_
    # nachziehen` steigt aus, solange die Leiste noch nicht gezeichnet ist, und
    # `minsize` bleibt dann auf 1 — die Pruefung waere gruen gewesen, ohne
    # irgendetwas geprueft zu haben. Genau die Falle aus Pruefung 83.
    _echt85 = _bs85.schirm_fuer
    _bs85.schirm_fuer = lambda *_a, **_k: (0, 0, 1280, 700)
    try:
        for _versuch85 in range(40):
            _fenster85._mindesthoehe_nachziehen()
            for _ in range(3):
                _wurzel85.update()
                _wurzel85.update_idletasks()
            if _echt_fenster85.minsize()[1] > 1:
                break
        _mb85, _mh85 = _echt_fenster85.minsize()
    finally:
        _bs85.schirm_fuer = _echt85

    pruefe(_mh85 > 1,
           'die Mindesthoehe wurde ueberhaupt gesetzt (%d)' % _mh85)
    pruefe(1 < _mh85 <= 700,
           'die Mindesthoehe bleibt auf dem Schirm (%d von 700)' % _mh85)

    # Gegenprobe: **ohne** die Deckelung waere sie groesser als der Schirm
    # gewesen. Sonst belegt nichts, dass die Deckelung die Ursache ist.
    _kopf85 = max(0, _echt_fenster85.winfo_height()
                  - _fenster85.leisten_flaeche.winfo_height())
    pruefe(_bedarf85 + _kopf85 > 700,
           'ohne Deckelung waere sie ueber dem Schirm gewesen (%d > 700)'
           % (_bedarf85 + _kopf85))

    # Und weil die Leiste dann nicht mehr ganz hineinpasst: Sie muss rollen,
    # sonst waeren die unteren Reiter unerreichbar — das Problem waere nur
    # verschoben statt behoben.
    _roll85 = str(_fenster85.leisten_flaeche.cget('scrollregion') or '')
    _teile85 = _roll85.split()
    pruefe(len(_teile85) == 4 and float(_teile85[3]) > 100,
           'die Seitenleiste hat einen Rollbereich (%s)' % _roll85)
    pruefe(hasattr(_fenster85, 'leisten_flaeche')
           and _fenster85.leisten_flaeche.winfo_class() == 'Canvas',
           'die Leiste sitzt auf einer Rollflaeche')

    _q85 = open(os.path.join(WURZEL, 'scbp', 'hauptfenster.py'),
                encoding='utf-8').read()
    pruefe('rad_anschliessen(self.leisten_flaeche)' in _q85,
           'das Mausrad haengt an der gemeinsamen Stelle, nicht am Eigenbau')

    # ⭐ Klappbare Gruppen — der dritte Hebel gegen die Fensterhoehe.
    pruefe(set(_fenster85.gruppen) >= {'werkstatt', 'handel', 'einstellungen'},
           'die Gruppen sind klappbar angelegt (%s)'
           % sorted(_fenster85.gruppen))

    _offen85 = _fenster85._seitenleiste_bedarf()
    for _g85 in ('werkstatt', 'handel', 'einstellungen'):
        _fenster85._gruppe_um(_g85, auf=False)
    for _ in range(6):
        _wurzel85.update()
        _wurzel85.update_idletasks()
    _zu85 = _fenster85._seitenleiste_bedarf()

    # ⚠⚠ **Mit Zahl, nicht mit „kleiner gleich".** Beim ersten Bau brachte das
    # Zuklappen **null** Ersparnis (1020 px vorher wie nachher):
    # `winfo_reqheight()` meldet auch fuer einen weggeklappten Rahmen die volle
    # Hoehe seines Inhalts. Eine Pruefung auf `<=` waere gruen geblieben.
    pruefe(_zu85 < _offen85 - 200,
           'zugeklappte Gruppen sparen echte Hoehe (%d -> %d px)'
           % (_offen85, _zu85))

    # ⚠⚠ **Die Knoepfe unten duerfen NICHT mitrollen.** Ein „Star Citizen
    # starten", das man erst herunterrollen muss, ist keiner.
    _fuss85 = _fenster85.leisten_fuss
    _in_fuss85 = []

    def _sammeln85(w):
        for k in w.winfo_children():
            _in_fuss85.append(k)
            _sammeln85(k)
    _sammeln85(_fuss85)
    pruefe(_fenster85.discordknopf in _in_fuss85
           or any(getattr(k, 'master', None) is _fuss85 for k in _in_fuss85),
           'die Knoepfe sitzen im festen Fuss, nicht in der Rollflaeche')
    pruefe(_fuss85.master is _fenster85.leisten_spalte,
           'der Fuss haengt an der Spalte, nicht am rollenden Teil')

    # Ohne sichtbaren Balken sieht eine ueberlaufende Leiste kaputt aus:
    # Eine offene Gruppe wirkt leer, und niemand kommt auf die Idee zu rollen.
    pruefe(hasattr(_fenster85, 'leisten_balken'),
           'die Leiste hat einen sichtbaren Rollbalken')

    # „Fuer Fortgeschrittene" gehoert in eine Gruppe wie alles andere —
    # sonst ist es das einzige Element der Leiste ohne eine.
    # ⚠ **Einstellungen, nicht Info.** Dahinter liegen Spielordner und
    # Erkennung — Dinge, die man einstellt. „Info" erzaehlt etwas.
    pruefe(_fenster85.klapp.master
           is _fenster85.gruppen['einstellungen']['inhalt'],
           'Fortgeschrittenes sitzt in der Gruppe Einstellungen')
    pruefe(hasattr(_fenster85, 'klapppfeil'),
           'und traegt denselben Klapp-Pfeil wie die Gruppen')

    # ⚠ **Bauplan-Bestand steht NICHT in der offenen Liste.** Die Seite
    # schreibt am eigenen Bestand; sie stand zwischen harmlosen Einstellungen
    # und wurde im Vorbeigehen angeklickt (30.08.2026).
    pruefe('bestand' not in _fenster85.knoepfe,
           'Bauplan-Bestand liegt hinter „Fuer Fortgeschrittene"')
    _fenster85._klapp_umschalten()
    for _ in range(4):
        _wurzel85.update()
        _wurzel85.update_idletasks()
    pruefe('bestand' in _fenster85.knoepfe,
           'und ist nach dem Aufklappen da')

    # ⚠⚠ **Umgedreht am 31.08.2026.** Hier stand bis v3.5.1 das Gegenteil:
    # „Protokolle erneut einlesen" MUSSTE rot sein. Das war falsch — der Knopf
    # legt nur an und kann nichts wegnehmen. Direkt darunter steht das
    # ebenfalls rote „Bestand zuruecksetzen", das wirklich loescht; zwei
    # Bedeutungen fuer dieselbe Farbe heissen, dass die Farbe nicht mehr warnt.
    # Gemeldet von Haldjas, der den harmlosen drueckte. Das Ganze steht jetzt
    # in Pruefung 95.
    _q85s = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                 encoding='utf-8').read()
    pruefe("t('s_be_neu'), neu_einlesen, gefahr=True" not in _q85s,
           'der Knopf „Protokolle erneut einlesen" ist NICHT rot')

    _q85p = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                 encoding='utf-8').read()
    pruefe("zeichen.zeile(zeile, 'aufklappen'" in _q85p,
           'auch das Auswahlfeld nutzt das Klapp-Symbol des Projekts')

    # ⚠ **Ein Bild im ganzen Programm**: Werkstatt-Lager und Handelslager
    # benutzen denselben Baustein fuer Ware/Rohstoff und Lagerort.
    # ⚠ Beim naechsten **Modul**-`def` schneiden (Zeilenanfang), nicht beim
    # naechsten `def` ueberhaupt: Die Lager-Seite hat innere Funktionen, und
    # der Block endete sonst vor der Stelle, die geprueft werden soll.
    _lagerseite85 = _q85p.split('def _lager(')[-1].split('\ndef ')[0]
    pruefe('_auswahlfeld(fenster, block, var,' in _lagerseite85,
           'auch „Mein Lager" nutzt das Auswahlfeld')
    pruefe('vorschlag_rahmen' not in _lagerseite85,
           'und nicht mehr die alte Vorschlagszeile daneben')

    # ⚠⚠ **Der Lagerort der Raffinerie-Ausbeute darf NICHT durch
    # `lager_name()` laufen.** Die Funktion zieht eine Eingabe auf einen
    # bekannten **Rohstoff** (sie vergleicht gegen `einlagerbar()`); ein
    # Ortsname steht dort nie drin. Ergebnis war `None`, und `or ''` machte
    # daraus einen leeren Lagerort: Wer „Levski" gewaehlt hatte, bekam die
    # ganze Ausbeute ohne Ort eingebucht (30.08.2026 gemeldet, mit zwei
    # Bildschirmfotos belegt).
    from scbp import herstellung as _he85
    pruefe(_he85.lager_name('Levski') is None,
           'lager_name() kennt keine Orte — das war die Ursache')

    _raffblock85 = _q85p.split('def _raffinerie_block(')[-1].split('\ndef ')[0]
    # ⚠ Nicht auf `lager_name(` allein pruefen — fuer die **Materialnamen** ist
    # sie genau richtig und wird dort weiter gebraucht. Falsch war nur, sie auf
    # den **Ort** anzuwenden.
    pruefe('lager_name((ort' not in _raffblock85
           and 'lager_name(ort' not in _raffblock85,
           'der Raffinerie-Block zieht den ORT nicht durch lager_name()')
    pruefe('ort_raff' in _raffblock85 and '_orte_modul.kennt(' in _raffblock85,
           'er hat ein eigenes Ortsfeld und prueft es gegen die Ortsliste')

    # ⚠⚠ **Die Mindesthoehe haengt NICHT mehr am Leistenbedarf.** Sie tat es,
    # solange die Leiste ein fester Rahmen war; mit jedem neuen Reiter wuchs
    # das Fenster mit, und am Ende liess es sich nicht mehr kleiner ziehen.
    from scbp.hauptfenster import MIN_HOEHE as _MH85
    pruefe(_MH85 <= 400,
           'die Mindesthoehe ist klein genug zum Kleinerziehen (%d px)' % _MH85)
    pruefe(_MH85 < _bedarf85,
           'und liegt unter dem Platzbedarf der Leiste (%d < %d)'
           % (_MH85, _bedarf85))
    pruefe('noetig = MIN_HOEHE' in _q85
           and 'noetig = max(MIN_HOEHE' not in _q85,
           'die Mindesthoehe wird nicht mehr aus dem Bedarf gerechnet')

    # ⚠ **Rollstelle beim Loeschen halten.** Wer einen Posten weit unten
    # loescht, soll nicht oben landen.
    pruefe('_rollstelle_halten(' in _q85p,
           'Loeschen haelt die Rollstelle')
    pruefe(_q85p.count('_rollstelle_halten(') >= 4,
           'an allen Loeschstellen, nicht nur an einer (%d)'
           % _q85p.count('_rollstelle_halten('))

    # ⚠ **„Wird noch gebaut" ist etwas anderes als „hol es selbst".** Wer in der
    # Luecke zwischen Tag und fertigem Bau auf „holen" klickt, findet auf der
    # Releases-Seite auch nichts — die alte Meldung schickte ihn ins Leere.
    _q85u = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                 encoding='utf-8').read()
    pruefe("if not (freigabe.get('dateien') or []):" in _q85u
           and "t('s_ub_wird_gebaut')" in _q85u,
           'eine Freigabe ohne Dateien meldet „wird noch gebaut"')

    # Und der Reiter einer zugeklappten Gruppe muss sie wieder aufmachen —
    # sonst steht man auf einer Seite, deren Eintrag nicht zu sehen ist.
    _fenster85.oeffnen('verkauf')
    for _ in range(4):
        _wurzel85.update()
        _wurzel85.update_idletasks()
    pruefe(_fenster85.gruppen['handel']['offen'],
           'wer einen Reiter oeffnet, sieht ihn auch in der Leiste')

    # Der Bericht muss den Fehler zeigen koennen — sonst raet man beim
    # naechsten Mal wieder.
    _q85b = open(os.path.join(WURZEL, 'scbp', 'bericht.py'),
                 encoding='utf-8').read()
    pruefe("t('b_fenstermass')" in _q85b and "t('b_fenster_zu_hoch')" in _q85b,
           'der Bericht nennt Fenstermass und Mindestmass')

    # ------------------------------------------------------------------
    # 85b. Eine breite Knopfreihe macht das Fenster BREITER, nicht hoeher
    # ------------------------------------------------------------------
    # ⚠⚠ **Der Fehler, den diese Pruefung bewacht.** `_knopfreihe` in
    # `seiten.py` fordert Breite an, wenn die Knoepfe nebeneinander nicht
    # hineinpassen — und setzte dabei bis 3.9.5
    # `minsize(noetig, oben.winfo_height())`. Die zweite Zahl ist die GERADE
    # AKTUELLE Fensterhoehe. Wer sein Fenster hoch gezogen hatte und danach
    # eine Seite mit breiter Knopfreihe oeffnete, konnte es nie wieder
    # niedriger ziehen.
    #
    # Gemeldet als `Fenster 1770x899, mindestens 1770x899` — beide Masse
    # gleich, das Fenster sass in seiner eigenen Groesse fest, obwohl
    # `MIN_HOEHE` 380 ist.
    #
    # ⚠ Die vorhandene Warnung `b_fenster_zu_hoch` schlaegt dabei NICHT an:
    # Sie greift erst, wenn die Mindesthoehe den BILDSCHIRM ueberschreitet.
    # 899 von 2880 ist weit davon entfernt — der Fehler blieb unter der
    # Schwelle und trotzdem spuerbar.
    #
    # ⚠⚠ **Und Pruefung 87 lief daran vorbei**, weil sie
    # `minsize(MIN_BREITE, MIN_HOEHE)` im QUELLTEXT sucht. Die Zeile stand da
    # und stimmte — sie wurde nur zwei Dateien weiter wieder ueberschrieben.
    # Deshalb misst diese Pruefung den WERT am fertigen Fenster.
    #
    # ⚠ Bewusst ein EIGENES Toplevel statt des Hauptfensters: Dort haengt
    # `_mindesthoehe_nachziehen` am `<Configure>` und setzt `minsize` gleich
    # wieder auf `MIN_HOEHE` zurueck. Die Pruefung waere gruen geworden, ohne
    # dass der Fehler behoben ist — genau die Falle aus Pruefung 83.
    print()
    print('85b. Die Knopfreihe hebt die Mindesthoehe nicht an')
    import tkinter as tk85k
    from scbp import seiten as _st85k

    _fenster85k = tk85k.Toplevel(_wurzel85)
    # Definierte Ausgangslage: eine Mindesthoehe, die klein genug ist, dass
    # das Fenster deutlich darueber liegen kann.
    _MIN_HOCH85k = 380
    _fenster85k.minsize(400, _MIN_HOCH85k)
    # Schmal genug, dass die geforderte Breite unter der Bildschirmgrenze
    # bleibt (`grenze = screenwidth - 40`), sonst laeuft der Zweig gar nicht.
    _breit85k = max(400, _fenster85k.winfo_screenwidth() // 2)
    _hoch85k = _MIN_HOCH85k + 260
    _fenster85k.geometry('%dx%d' % (_breit85k, _hoch85k))
    _fenster85k.deiconify()
    for _ in range(8):
        _wurzel85.update()
        _wurzel85.update_idletasks()

    # Ein schmaler Rahmen mit Knoepfen, die nebeneinander nicht hineinpassen —
    # genau die Lage, die `_knopfreihe` zum Verbreitern bringt.
    _rahmen85k = tk85k.Frame(_fenster85k, width=200, height=40)
    _rahmen85k.pack_propagate(False)
    _rahmen85k.pack(side='top', anchor='w')
    _knoepfe85k = [tk85k.Button(_rahmen85k,
                                text='Einrichtung wiederholen %d' % _n)
                   for _n in range(3)]
    for _k85k in _knoepfe85k:
        _k85k.pack(side='left')
    for _ in range(4):
        _wurzel85.update()
        _wurzel85.update_idletasks()

    _st85k._knopfreihe(_rahmen85k, _knoepfe85k)
    for _ in range(8):
        _wurzel85.update()
        _wurzel85.update_idletasks()

    _mb85k, _mh85k = _fenster85k.minsize()

    # ⚠⚠ **Erst pruefen, ob ueberhaupt etwas passiert ist.** Bleibt die
    # Mindestbreite auf ihrem Ausgangswert, hat `_knopfreihe` den kritischen
    # Zweig nie betreten (zu wenig Ueberstand, oder die Bildschirmgrenze hat
    # ihn abgeschnitten) — dann prueft alles Weitere nichts. Ohne diesen
    # Waechter waere die Pruefung immer gruen.
    pruefe(_mb85k > 400,
           'die Knopfreihe hat wirklich Breite angefordert (400 -> %d px)'
           % _mb85k)

    # ⭐ Der eigentliche Punkt: die Hoehe wurde NICHT angefasst.
    pruefe(_mh85k == _MIN_HOCH85k,
           'die Mindesthoehe bleibt bei %d (ist %d, Fenster war %d hoch)'
           % (_MIN_HOCH85k, _mh85k, _hoch85k))

    # Und die Folge davon, in der Sprache des Nutzers: Das Fenster laesst sich
    # wieder auf seine Mindesthoehe zusammenziehen.
    _fenster85k.geometry('%dx%d' % (_mb85k, _MIN_HOCH85k))
    for _ in range(8):
        _wurzel85.update()
        _wurzel85.update_idletasks()
    pruefe(_fenster85k.winfo_height() <= _MIN_HOCH85k + 4,
           'das Fenster laesst sich wieder verkleinern (%d px)'
           % _fenster85k.winfo_height())

    try:
        _fenster85k.destroy()
    except Exception:
        pass

    try:
        _wurzel85.destroy()
    except Exception:
        pass

    # 86. Beide Lager lassen sich sichern und zurueckholen
    #
    # ⚠⚠ **Der teure Fall ist die verwechselte Datei.** Werkstatt-Lager und
    # Handelslager schreiben beide `{"format": 1, "posten": [...]}` — an der
    # Huelle sind sie nicht zu unterscheiden. Ohne Weiche haette das
    # Handelslager eine Rohstoff-Sicherung klaglos angenommen, jeden Posten
    # mangels `ware` weggeworfen und mit „0 Posten eingelesen" ein LEERES
    # Lager gespeichert. Ein Lager ist Handarbeit, die kein Neuaufbau
    # zurueckholt.
    #
    # Die Prueflinge legt sich diese Pruefung selbst hin — keine Nutzerdatei,
    # kein Abruf.
    print()
    print('86. Beide Lager: sichern, zurueckholen, leeren')
    from scbp import handelslager as _hl86, rohstoffe as _rs86

    _hl86.leeren()
    _hl86.eintragen('Gold', '500', 'Orison', False)
    _hl86.eintragen('Gold', '100', 'Orison', False)
    _hl86.eintragen('Laranite', '12,5', 'Area18', True)

    _csv86 = _hl86.als_csv()
    pruefe(_csv86.startswith('Ware;Menge;Gestohlen;Lagerort'),
           'die Tabelle hat eine Kopfzeile')
    pruefe('Gold;600;;Orison' in _csv86,
           'gleiche Stapel stehen zusammengezaehlt darin')
    # ⚠ Komma als Dezimalzeichen: Mit Punkt macht ein deutsches Excel aus
    # „12.5" ein Datum.
    pruefe('Laranite;12,5;ja;Area18' in _csv86,
           'Menge mit Komma, gestohlene Ware als „ja" gekennzeichnet')

    pruefe(_hl86.aus_json(_hl86.als_json()) == _hl86.laden(),
           'was ausgegeben wurde, kommt unveraendert zurueck')

    # Der Kernfall: die Sicherung des ANDEREN Lagers.
    _fremd86 = _rs86.als_json([{'material': 'Iron', 'menge': 5,
                                'qualitaet': 800, 'ort': 'Daymar'}])
    pruefe(_hl86.aus_json(_fremd86) is None,
           'eine Werkstatt-Sicherung wird im Handelslager ABGELEHNT')
    # Gegenprobe zur Gegenprobe — ohne die Weiche waere das Ergebnis leer
    # gewesen, und genau das ist der Datenverlust.
    import json as _json86
    pruefe(not any(_p.get('ware') for _p
                   in _json86.loads(_fremd86)['posten']),
           'sie haette sonst ein leeres Lager ergeben')
    pruefe(_rs86.aus_json(_hl86.als_json()) == [],
           'und umgekehrt bringt eine Handels-Sicherung dem Werkstatt-Lager '
           'nichts')

    pruefe(_hl86.aus_json('kein json') is None
           and _hl86.aus_json('{"format": 9, "posten": []}') is None,
           'kaputte und fremde Formate werden abgelehnt')
    pruefe(_hl86.aus_json('{"format": 1, "posten": []}') == [],
           'ein leeres Lager ist aber erlaubt')
    pruefe(_hl86.aus_json('{"format": 1, "posten": [{"ware": "Tin", '
                          '"menge": "viel"}, {"ware": "Gold", "menge": 5}]}')
           == [{'ware': 'Gold', 'menge': 5.0, 'ort': '', 'gestohlen': False}],
           'unbrauchbare Zeilen fallen raus, der Rest bleibt')

    _anzahl86 = len(_hl86.laden())
    _hl86.leeren()
    pruefe(_anzahl86 == 2 and _hl86.laden() == [],
           'nach einem Patch-Wisch raeumt „leeren" alles in einem Zug weg')

    # Und die Oberflaeche muss die Griffe auch anbieten — beide Lager
    # dieselben, in derselben Reihenfolge.
    _q86 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                encoding='utf-8').read()
    _hlblock86 = _q86.split('def _handelslager(')[-1].split('\ndef ')[0]
    for _schluessel86 in ("s_lg_aus_json", "s_lg_aus_csv", "s_lg_einlesen",
                          "s_lg_leeren"):
        pruefe("t('%s')" % _schluessel86 in _hlblock86,
               'das Handelslager hat den Knopf %s' % _schluessel86)
    pruefe('gefahr=True' in _hlblock86,
           'und „Lager loeschen" steht in Rot')
    pruefe("frage_stellen(" in _hlblock86,
           'das Leeren fragt vorher nach')

    # Der Raffinerie-Block ist einklappbar — und merkt sich die Lage.
    _raffblock86 = _q86.split('def _raffinerie_block(')[-1].split('\ndef ')[0]
    pruefe("symbol_tauschen('zuklappen')" in _raffblock86
           and "symbol_tauschen('aufklappen')" in _raffblock86,
           'die Raffinerie-Ausbeute laesst sich ein- und ausklappen')
    # ⚠⚠ **Diese Pruefung hat den Fehler bis zum 03.09.2026 FESTGESCHRIEBEN.**
    # Sie verlangte woertlich `einstellung('lager_raffinerie_offen')` — also
    # genau den falschen Aufruf: Die Funktion liefert einen PFAD und ruft
    # `.strip()` auf dem Wert. Bei `True` warf das einen AttributeError und
    # riss den Aufbau der ganzen Lager-Seite ab.
    #
    # Eine Pruefung, die eine Schreibweise vorschreibt, statt eine Wirkung zu
    # pruefen, haelt einen Fehler fest, sobald er einmal drin ist. Hier steht
    # deshalb jetzt, WAS gelten muss: ein Ja/Nein wird mit dem Ja/Nein-Leser
    # gelesen, nie mit dem Pfad-Leser.
    pruefe(_raffblock86.count("einstellung_setzen('lager_raffinerie_offen'") >= 2
           and "einstellung_wahrheit('lager_raffinerie_offen'" in _raffblock86,
           'und die Lage ueberlebt den Neustart — in BEIDE Richtungen')
    pruefe("einstellung('lager_raffinerie_offen')" not in _raffblock86,
           'gelesen wird sie als Ja/Nein, nicht als Pfad')

    # 87. Das Fenster behaelt die eingestellte Groesse
    #
    # ⭐ Wer mit langen Listen arbeitet, zieht das Fenster gross — und fand es
    # bis 31.08.2026 bei jedem Start wieder auf 1160x380 zurueckgesetzt.
    #
    # ⚠ Gemerkt wird **nur die Groesse, nie die Lage**: Eine gespeicherte
    # Position zeigt auf einem anderen Rechner ins Nichts (dieselbe Falle wie
    # beim Overlay, siehe `geometrie_pruefen`).
    print()
    print('87. Das Fenster behaelt die eingestellte Groesse')
    from scbp import hauptfenster as _hf87
    from scbp import pfade as _pf87

    # ⚠⚠ **Geprueft wird die RECHNUNG, nicht das gezeichnete Fenster.** Die
    # erste Fassung mass `winfo_width()` an einem echten Fenster — und fiel auf
    # beiden Bau-Rechnern durch, obwohl der Code stimmte: Ein verstecktes
    # Fenster meldet dort keine brauchbaren Masse (dieselbe Falle wie bei den
    # Pruefungen 59 und 60). Was zaehlt, ist ohnehin `gemerkte_groesse()`:
    # Genau ihr Ergebnis setzt `Hauptfenster.__init__` als Startgroesse.
    class _Schirm87:
        """Ein Bildschirm bekannter Groesse — statt des echten."""

        def __init__(self, breite, hoehe):
            self._b, self._h = breite, hoehe

        def winfo_screenwidth(self):
            return self._b

        def winfo_screenheight(self):
            return self._h

    _gross87 = _Schirm87(3840, 2160)
    _klein87 = _Schirm87(1024, 768)          # kleiner als die Mindestgroesse!

    _pf87.einstellung_setzen(_hf87.GROESSE_SCHLUESSEL, None)
    pruefe(_hf87.gemerkte_groesse(_gross87) == (_hf87.MIN_BREITE, _hf87.MIN_HOEHE),
           'ohne gemerkte Groesse gilt die Mindestgroesse')

    _b87, _h87 = _hf87.MIN_BREITE + 240, _hf87.MIN_HOEHE + 300
    _pf87.einstellung_setzen(_hf87.GROESSE_SCHLUESSEL, '%dx%d' % (_b87, _h87))
    pruefe(_hf87.gemerkte_groesse(_gross87) == (_b87, _h87),
           'eine gemerkte Groesse wird unveraendert zurueckgegeben')

    # Unbrauchbares faellt zurueck — sonst verkruemelt ein kaputter Eintrag das
    # Fenster unter seine eigene Mindestgroesse.
    for _muell87 in ('', 'kaputt', '0x0', '12x9', '-100x-100', '1160', 'axb'):
        _pf87.einstellung_setzen(_hf87.GROESSE_SCHLUESSEL, _muell87)
        if _hf87.gemerkte_groesse(_gross87) != (_hf87.MIN_BREITE, _hf87.MIN_HOEHE):
            pruefe(False, 'unbrauchbarer Eintrag %r faellt nicht zurueck' % _muell87)
            break
    else:
        pruefe(True, 'unbrauchbare Eintraege fallen auf die Mindestgroesse zurueck')

    # Eine Groesse vom grossen Schirm darf am kleinen nicht ueberstehen …
    _pf87.einstellung_setzen(_hf87.GROESSE_SCHLUESSEL, '3000x1800')
    _bg87, _hg87 = _hf87.gemerkte_groesse(_klein87)
    pruefe(_bg87 <= max(1024, _hf87.MIN_BREITE) and _hg87 <= max(768, _hf87.MIN_HOEHE),
           'eine Groesse groesser als der Bildschirm wird gedeckelt')

    # ⚠⚠ … und die Deckelung darf die Mindestgroesse NICHT unterbieten. Genau
    # daran ist der erste Bau-Lauf von v3.4.2 gescheitert: Der Bau-Rechner hat
    # einen kleineren Schirm als jeder echte Nutzer, heraus kam 1024x768 —
    # unterhalb des eigenen `minsize` von 1160x380.
    for _eintrag87 in (None, '', '3000x1800', 'kaputt'):
        _pf87.einstellung_setzen(_hf87.GROESSE_SCHLUESSEL, _eintrag87)
        _bk87, _hk87 = _hf87.gemerkte_groesse(_klein87)
        if _bk87 < _hf87.MIN_BREITE or _hk87 < _hf87.MIN_HOEHE:
            pruefe(False, 'kleiner Schirm + Eintrag %r ergibt %dx%d — unter der '
                          'Mindestgroesse' % (_eintrag87, _bk87, _hk87))
            break
    else:
        pruefe(True, 'auf einem Schirm kleiner als die Mindestgroesse gewinnt '
                     'trotzdem die Mindestgroesse')

    # Und der Weg von der Rechnung ins Fenster muss auch gegangen werden.
    _q87 = open(os.path.join(WURZEL, 'scbp', 'hauptfenster.py'),
                encoding='utf-8').read()
    pruefe('_b_start, _h_start = gemerkte_groesse(self.root)' in _q87
           and 'bildschirm.mittig(self.root, _b_start, _h_start)' in _q87,
           'der Start benutzt die gemerkte Groesse — und bleibt mittig')
    pruefe('self.root.minsize(MIN_BREITE, MIN_HOEHE)' in _q87,
           'die Mindestgroesse wird unveraendert gesetzt')
    pruefe("self.root.bind('<Configure>', self._groesse_beobachten" in _q87
           and 'after_cancel' in _q87,
           'Groessenaenderungen werden verfolgt und gedrosselt gespeichert')
    pruefe("self.root.state() != 'normal'" in _q87,
           'im maximierten Zustand wird nichts gemerkt')

    _pf87.einstellung_setzen(_hf87.GROESSE_SCHLUESSEL, None)

    # 88. Kein Funktionsname zweimal in derselben Funktion
    #
    # ⚠⚠ **Die Wurzel des Fehlers in v3.4.2.** Im Handelslager gab es zweimal
    # `_leeren`: einmal den Helfer, der die Kinder eines Rahmens wegraeumt
    # (**mit** Argument), und einmal — neu dazugebaut — das Leeren des ganzen
    # Lagers (**ohne**). In Python gewinnt die spaetere Definition, ohne Warnung.
    # Ergebnis: Jeder Aufbau der Liste starb mit „takes 0 positional arguments
    # but 1 was given", die Seite blieb ohne Tabelle — und ging so an die Nutzer.
    #
    # Gesucht wird mit dem Syntaxbaum, nicht mit Textsuche: Nur so ist klar,
    # welche Definition zu welcher Funktion gehoert.
    print()
    print('88. Kein Funktionsname zweimal in derselben Funktion')
    import ast as _ast88

    _doppelte88 = []
    for _datei88 in sorted(os.listdir(os.path.join(WURZEL, 'scbp'))):
        if not _datei88.endswith('.py'):
            continue
        _pfad88 = os.path.join(WURZEL, 'scbp', _datei88)
        try:
            _baum88 = _ast88.parse(open(_pfad88, encoding='utf-8').read())
        except Exception:
            continue
        for _knoten88 in _ast88.walk(_baum88):
            if not isinstance(_knoten88, (_ast88.FunctionDef,
                                          _ast88.AsyncFunctionDef)):
                continue
            _gesehen88 = {}
            for _kind88 in _knoten88.body:
                if isinstance(_kind88, (_ast88.FunctionDef,
                                        _ast88.AsyncFunctionDef)):
                    if _kind88.name in _gesehen88:
                        _doppelte88.append(
                            '%s: %s() definiert %s() zweimal (Zeile %d und %d)'
                            % (_datei88, _knoten88.name, _kind88.name,
                               _gesehen88[_kind88.name], _kind88.lineno))
                    _gesehen88[_kind88.name] = _kind88.lineno

    pruefe(not _doppelte88,
           'kein lokaler Funktionsname wird ueberschrieben%s'
           % ('' if not _doppelte88 else ' — ' + '; '.join(_doppelte88[:3])))

    # 89. Ein Ende meint den Auftrag — oder nur ein Zwischenziel
    #
    # ⚠⚠ **Beim Zurückziehen meldet das Spiel oft das ZIEL, nicht den
    # Auftrag.** Angenommen wird „Retake Platforms From Nine Tails",
    # zurückgezogen wird „Obere Plattform erreichen". Beide Meldungen tragen
    # dieselbe MissionId — die Ziel-Meldung zusätzlich eine **ObjectiveId**.
    # Genau daran hängt der Unterschied.
    #
    # Zweimal ist das hier schon schiefgegangen:
    #   * v3.4.3 und davor: nur über den Titel gestrichen. Ein von Hand
    #     abgebrochener Auftrag lief ewig weiter (Morkhan/KRT, 31.08.2026).
    #   * v3.4.4: jedes unzuordenbare Ende räumte die ganze Liste. Damit
    #     verschwand ein Auftrag, der im Spiel sichtbar aktiv war
    #     (31.08.2026, mit Bildschirmfoto gemeldet).
    #
    # Über alle 153 Protokolle gemessen: 473 Enden, davon 111 mit ObjectiveId
    # (Zwischenziele, die Mission lief jedes Mal weiter) und 362 echte
    # Missions-Enden — **alle 362 zuordenbar**, 300 über den Titel, 62 über
    # die MissionId. Es muss also weder geraten noch geräumt werden.
    print()
    print('89. Ein Ende meint den Auftrag — oder nur ein Zwischenziel')
    from scbp import auftraege as _au89

    def _zeile89(art, titel, mid='11111111-1111-1111-1111-111111111111', oid=''):
        return ('Added notification "%s: %s: " [1] to queue. New queue size: 1,'
                ' MissionId: [%s], ObjectiveId: [%s]'
                ' [Team_CoreGameplayFeatures][Missions][Comms]'
                % (art, titel, mid, oid)) + '\n'

    _an89 = _zeile89('Auftrag angenommen', 'Retake Platforms From Nine Tails')
    _ziel89 = _zeile89('Neuer Auftrag', 'Obere Plattform erreichen',
                       oid='22222222-2222-2222-2222-222222222222')
    _zielweg89 = _zeile89('Auftrag zurückgezogen', 'Obere Plattform erreichen',
                          oid='22222222-2222-2222-2222-222222222222')
    _fertig89 = _zeile89('Auftrag abgeschlossen', 'Retake Platforms From Nine Tails')
    # Ein echter Abbruch: dieselbe MissionId, KEINE ObjectiveId — und ein
    # Titel, der nicht zur Annahme passt. Genau die 62 aus der Messung.
    _weg89 = _zeile89('Auftrag zurückgezogen', 'Ganz anderer Wortlaut')
    _neu89 = _zeile89('Auftrag angenommen', 'Kill the king',
                      mid='33333333-3333-3333-3333-333333333333')

    # a) Der Kern des Fehlers vom 31.08.2026.
    _laeuft89 = _au89.offene_aus_text(_an89 + _ziel89 + _zielweg89)
    pruefe(len(_laeuft89) == 1 and 'Retake Platforms' in _laeuft89[0],
           'ein zurueckgezogenes ZIEL laesst den Auftrag stehen')

    # b) Und er reisst auch keinen zweiten mit — das war v3.4.4.
    pruefe(len(_au89.offene_aus_text(_an89 + _neu89 + _zielweg89)) == 2,
           'und raeumt schon gar nicht die ganze Liste')

    # c) Der echte Abbruch verschwindet trotzdem — ueber die MissionId, auch
    #    wenn der Endtitel voellig anders lautet (Morkhans Fall).
    _nach89 = _au89.offene_aus_text(_an89 + _neu89 + _weg89)
    pruefe(len(_nach89) == 1 and 'Kill the king' in _nach89[0],
           'ein echter Abbruch trifft ueber die MissionId genau seinen Auftrag')

    pruefe(_au89.offene_aus_text(_an89 + _fertig89) == [],
           'ein sauber abgeschlossener verschwindet weiterhin')
    pruefe(len(_au89.offene_aus_text(_an89)) == 1,
           'ein laufender Auftrag bleibt stehen')

    # d) Ohne die Kennungen — fremdes Format, aeltere Spielfassung — muss
    #    weiterhin der Titel entscheiden. Sonst faellt das Werkzeug bei einer
    #    kuenftigen Log-Aenderung still auf „nichts geht mehr" zurueck.
    _alt89 = 'Added notification "Auftrag angenommen: Secure Our Airspace: " [1]' + '\n'
    _altweg89 = 'Added notification "Auftrag abgeschlossen: Secure Our Airspace: " [2]' + '\n'
    pruefe(len(_au89.offene_aus_text(_alt89)) == 1
           and _au89.offene_aus_text(_alt89 + _altweg89) == [],
           'ohne MissionId zaehlt weiterhin der Titel')

    # e) Die Kennungen kommen auch wirklich mit heraus — der laufende Betrieb
    #    braucht sie, um ein spaeteres Ende zuzuordnen.
    _ereig89 = _au89.ereignisse_aus_text(_an89 + _zielweg89)
    pruefe(len(_ereig89) == 2 and len(_ereig89[0]) == 4
           and _ereig89[0][2].startswith('11111111')
           and _ereig89[0][3] == '' and _ereig89[1][3].startswith('22222222'),
           'ereignisse_aus_text liefert MissionId und ObjectiveId mit')

    _offen89, _mid89 = _au89.stand_aus_text(_an89)
    pruefe(list(_mid89) == ['11111111-1111-1111-1111-111111111111'],
           'stand_aus_text gibt die Missions-Kennungen zurueck')

    # f) Und das Hauptprogramm darf NICHT pauschal raeumen.
    #
    # ⚠ Bis zum 31.08.2026 stand hier eine Textsuche nach `offen.clear()` im
    # Quelltext. Die hat das falsche geprueft: Sie verbot ein **Wort**, nicht
    # ein **Verhalten** — und schlug damit auch bei einem Raeumen an, das
    # richtig ist. Geprueft wird jetzt, was herauskommt.
    #
    # Der Unterschied, um den es geht:
    #
    # | Auslöser | räumen? | warum |
    # |---|---|---|
    # | Ende, das sich keinem Auftrag zuordnen lässt | **nein** | geraten — das war v3.4.4 |
    # | Spielwelt verlassen (`VERLASSEN`) | **ja** | das Spiel sagt es selbst |
    _fremd89 = _zeile89('Auftrag abgeschlossen', 'Nie angenommener Auftrag',
                        mid='99999999-9999-9999-9999-999999999999')
    pruefe(len(_au89.offene_aus_text(_an89 + _neu89 + _fremd89)) == 2,
           'ein unzuordenbares Ende raeumt NICHTS (die v3.4.4-Falle)')

    # g) Ausloggen dagegen raeumt — und zwar alles.
    #
    # Gemeldet am 31.08.2026: Star Citizen war nicht einmal gestartet, und in
    # der Leiste stand ein Auftrag von vorgestern. Beim Verlassen der
    # Spielwelt meldet das Spiel **kein** Auftrags-Ende, das Auftragsbuch ist
    # trotzdem leer. An 23 Protokollen gemessen: 39 Marker, kein einziger
    # Auftrag hat eines ueberlebt.
    _raus89 = ('<2026-08-30T12:27:22.352Z> [CSessionManager::RequestFrontEnd]'
               ' Started - RequestFrontEndReason="OnLobbyPostGameUnload"!') + '\n'
    pruefe(_au89.offene_aus_text(_an89 + _neu89 + _raus89) == [],
           'Ausloggen raeumt die Liste')
    pruefe(len(_au89.offene_aus_text(_an89 + _raus89 + _neu89)) == 1,
           'was DANACH angenommen wird, bleibt stehen')

    # ⚠ Die Gegenprobe: Ohne den Marker muesste der alte Fehler wieder da
    # sein. Ist er das nicht, prueft der Test oben nichts.
    import re as _re89
    _merk89 = _au89.VERLASSEN
    _au89.VERLASSEN = _re89.compile(r'(?!x)x')
    _ohne89 = _au89.offene_aus_text(_an89 + _neu89 + _raus89)
    _au89.VERLASSEN = _merk89
    pruefe(len(_ohne89) == 2,
           'ohne den Marker stuenden sie wieder da (Gegenprobe)')

    # h) Das Ereignis traegt keinen Titel — wer `sauber(titel)` zuerst prueft,
    #    wirft es weg und raeumt nie. Genau diese Reihenfolge ist die Falle.
    _ev89 = _au89.ereignisse_aus_text(_an89 + _raus89)
    pruefe([e[0] for e in _ev89] == [True, None],
           'das Verlassen kommt als eigenes Ereignis (ist_annahme is None)')
    _w89 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'),
                encoding='utf-8').read()
    _ab89 = _w89.split('def _auftraege_melden')[1].split('def _emit')[0]
    pruefe('beendet_welchen' in _ab89,
           'der laufende Betrieb entscheidet ueber dieselbe Stelle wie der Start')

    # 90. Der Seitenwechsel zeichnet nur, wenn es etwas zu zeichnen gibt
    #
    # ⚠⚠ **Am 31.08.2026 gemeldet: „reagiert etwas langsamer".** Gemessen kam
    # heraus: Der Wechsel auf die Bauplan-Liste kostete **642 ms**, obwohl die
    # Seite laengst gebaut war. Ursache war die Routine, die beim erneuten
    # Anzeigen die Filter zuruecksetzt — sie zeichnete **immer** alle 738
    # Zeilen neu, auch wenn gar kein Filter gesetzt war. Und `set('')` auf ein
    # bereits leeres Suchfeld loest den `trace` trotzdem aus.
    #
    # Jetzt: 0,4 ms, wenn nichts gesetzt war. ⚠ Der Zweck darf dabei nicht
    # verloren gehen — war etwas gesetzt, MUSS weiter zurueckgestellt werden,
    # sonst steht der Suchbegriff von vorhin wieder da (29.08.2026 gemeldet).
    print()
    print('90. Der Seitenwechsel zeichnet nur, wenn noetig')
    _q90 = open(os.path.join(WURZEL, 'scbp', 'bestandsfenster.py'),
                encoding='utf-8').read()
    _fein90 = _q90.split('def _fein_leeren(')[1].split('\n    def ')[0]
    pruefe('etwas_gesetzt' in _fein90 and 'if etwas_gesetzt:' in _fein90,
           'die Filter-Ruecksetzung zeichnet nur bei gesetztem Filter')
    _suche90 = _q90.split('def _suche_leeren(')[1].split('\n    def ')[0]
    pruefe('if self.suche.get():' in _suche90,
           'und das Suchfeld wird nur angefasst, wenn etwas drinsteht')

    _q90s = open(os.path.join(WURZEL, 'scbp', 'seiten.py'), encoding='utf-8').read()
    _herst90 = _q90s.split('def _herst_frisch(')[1].split('\n    fenster.beim_zeigen')[0]
    pruefe('if not etwas_gesetzt:' in _herst90 and 'return' in _herst90,
           'dasselbe auf der Herstellungs-Seite')

    # ⚠ Und die Ruecksetzung selbst muss erhalten bleiben — sonst ist der
    # Geschwindigkeitsgewinn mit einem alten Fehler bezahlt.
    pruefe("self.suche.set('')" in _suche90,
           'ein gesetzter Suchbegriff wird weiterhin geleert')
    pruefe("self.fein[schluessel] = ''" in _fein90
           and 'self._zeichnen(nach_oben=True)' in _fein90,
           'und gesetzte Filter werden weiterhin zurueckgestellt und gezeichnet')

    # 91. Ein Auftrag steht nur EINMAL im Overlay
    #
    # ⚠⚠ **Am 31.08.2026 mit Bildschirmfoto gemeldet: „wieso sehe ich ne quest
    # jetzt 2 mal".** Derselbe Satz stand in der Auftragsleiste und direkt
    # darunter noch einmal als Hinweiszeile. Der Watcher schickte beides: die
    # Leiste (`auftraege`) und den Hinweis (`hinweis`) — mit demselben Text.
    #
    # ⚠ Geprueft wird die **Anzeige**, nicht der Quelltext. Der Fehler war auf
    # einem Bild zu sehen und im Code nicht: Beide Aufrufe fuer sich sind
    # richtig, erst zusammen ergeben sie die Dopplung.
    print()
    print('91. Ein Auftrag steht nur EINMAL im Overlay')
    import sc_bp_watcher as _w91

    def _texte91(fenster, teil):
        """Wie oft steht dieser Text sichtbar im Overlay?"""
        treffer = []

        def suchen(widget):
            try:
                t = widget.cget('text')
                if isinstance(t, str) and teil in t:
                    treffer.append(t)
            except Exception:
                pass
            for kind in widget.winfo_children():
                suchen(kind)

        suchen(fenster.root)
        return treffer

    _wz91 = _wurzel()
    _ov91 = _w91.Overlay(_wz91)
    _satz91 = 'Auftrag angenommen: Retake Platforms  ->  3 Bauplaene'
    _schluessel91 = 'retake platforms'

    _ov91.auftraege_zeigen([(_schluessel91, _satz91)])
    for _ in range(4):
        _wz91.update()
        _wz91.update_idletasks()
    _ov91.add_hinweis(_satz91, _schluessel91)
    for _ in range(4):
        _wz91.update()
        _wz91.update_idletasks()
    _gefunden91 = len(_texte91(_ov91, 'Retake Platforms'))
    pruefe(_gefunden91 == 1,
           'der Auftrag steht genau einmal im Overlay (gefunden: %d)' % _gefunden91)

    # ⚠ Ohne Leiste muss der Hinweis weiterhin kommen — sonst faellt die
    # Meldung ganz weg, sobald jemand den Auftrag wegklickt.
    _ov91.auftraege_zeigen([])
    for _ in range(4):
        _wz91.update()
        _wz91.update_idletasks()
    _ov91.add_hinweis('Auftrag angenommen: Kill the king  ->  1 Bauplan', 'kill the king')
    for _ in range(4):
        _wz91.update()
        _wz91.update_idletasks()
    pruefe(len(_texte91(_ov91, 'Kill the king')) == 1,
           'ohne Leisteneintrag erscheint der Hinweis weiterhin')

    # Und ein gewoehnlicher Hinweis ohne Auftrag bleibt unberuehrt.
    _ov91.add_hinweis('Keine Log-Sicherungen gefunden')
    for _ in range(4):
        _wz91.update()
        _wz91.update_idletasks()
    pruefe(len(_texte91(_ov91, 'Keine Log-Sicherungen')) == 1,
           'gewoehnliche Hinweise sind nicht betroffen')

    # 92. Was gerade zu tun ist — die Zwischenziele unter dem Auftrag
    #
    # ⚠⚠ **Der Auftrag sagt, ob Baupläne drin sind. Das Ziel sagt, wofür man
    # gerade fliegt.** Beides steht im Protokoll, aber an zwei Stellen: Der
    # Zustand kommt aus der sprachneutralen Zeile `<ObjectiveUpserted> … state
    # …`, der Wortlaut aus der übersetzten Meldung — zugeordnet über die
    # `ObjectiveId`, nie über die Formulierung.
    #
    # ⚠ **Nicht auf den Wortlaut hören.** Auf Deutsch heißt die Ziel-Annahme
    # „Neuer Auftrag" — wortgleich mit einer Auftragsmeldung. Wer danach geht,
    # zählt Ziele als Aufträge.
    print()
    print('92. Was gerade zu tun ist — Zwischenziele unter dem Auftrag')
    from scbp import auftraege as _au92

    _m92 = 'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee'

    def _mld92(art, titel, oid=''):
        return ('Added notification "%s: %s: " [1] to queue. New queue size: 1,'
                ' MissionId: [%s], ObjectiveId: [%s] [Missions][Comms]'
                % (art, titel, _m92, oid)) + '\n'

    def _zst92(oid, zustand, flags='ShowInLog|RespectInheritedVisibility|'):
        return ('<Notice> <ObjectiveUpserted> Received ObjectiveUpserted push'
                ' message for: mission_id %s - objective_id %s - state'
                ' MISSION_OBJECTIVE_STATE_%s - created 0 - flags=%s'
                ' [Team_GameServices][Missions]'
                % (_m92, oid, zustand, flags)) + '\n'

    _z92 = _au92.Ziele()
    _z92.aufnehmen(_au92.ziel_ereignisse_aus_text(
        _mld92('Neuer Auftrag', 'Solanki-Plattform erreichen', 'aaaa1111')
        + _zst92('aaaa1111', 'INPROGRESS')))
    pruefe(_z92.offen(_m92) == ['Solanki-Plattform erreichen'],
           'ein angefangenes Ziel steht da')

    # a) Der halbe Maschinenraum bleibt draussen: Zaehler und Zonenwaechter
    #    laufen als Ziele mit, gehoeren aber in kein Auftragsbuch.
    _z92.aufnehmen(_au92.ziel_ereignisse_aus_text(
        _zst92('cccc3333', 'INPROGRESS', 'SilentUpdates|')))
    pruefe(_z92.offen(_m92) == ['Solanki-Plattform erreichen'],
           'ein internes Ziel ohne ShowInLog taucht NICHT auf')

    # b) Ohne Wortlaut wird geschwiegen — dieselbe Linie wie ueberall.
    _z92.aufnehmen(_au92.ziel_ereignisse_aus_text(_zst92('eeee5555', 'INPROGRESS')))
    pruefe(_z92.offen(_m92) == ['Solanki-Plattform erreichen'],
           'ein Ziel ohne bekannten Wortlaut wird nicht erfunden')

    # c) Erledigt heisst weg — und zurueckgezogen auch.
    _geaendert92 = _z92.aufnehmen(_au92.ziel_ereignisse_aus_text(
        _zst92('aaaa1111', 'COMPLETED')
        + _mld92('Neuer Auftrag', 'Dach erreichen', 'dddd4444')
        + _zst92('dddd4444', 'INPROGRESS')))
    pruefe(_z92.offen(_m92) == ['Dach erreichen'],
           'ein erledigtes Ziel macht dem naechsten Platz')
    pruefe(_geaendert92 is True,
           'die Buchfuehrung meldet, dass sich etwas geaendert hat')
    pruefe(_z92.aufnehmen([]) is False,
           'und meldet nichts, wenn nichts kam')

    # d) ⚠⚠ **Das war der Fehler vom 31.08.2026.** Ein zurueckgezogenes ZIEL
    #    darf den Auftrag nicht mitreissen — die beiden Ebenen muessen auch
    #    hier getrennt bleiben.
    _text92 = (_mld92('Auftrag angenommen', 'Retake Platforms')
               + _mld92('Neuer Auftrag', 'Dach erreichen', 'dddd4444')
               + _zst92('dddd4444', 'INPROGRESS')
               + _mld92('Auftrag zurückgezogen', 'Dach erreichen', 'dddd4444')
               + _zst92('dddd4444', 'WITHDRAWN'))
    _offen92, _mid92 = _au92.stand_aus_text(_text92)
    _z92b = _au92.Ziele()
    _z92b.aufnehmen(_au92.ziel_ereignisse_aus_text(_text92))
    pruefe(len(_offen92) == 1 and _z92b.offen(_m92) == [],
           'das Ziel ist weg, der Auftrag bleibt')

    # e) Und der Auftrag nimmt seine Ziele mit, wenn er endet.
    _z92b.vergessen(_m92)
    pruefe(_z92b.offen(_m92) == [],
           'ein beendeter Auftrag laesst keine Ziele zurueck')

    # f) Die Anzeige. @ Geprueft wird, was dasteht — nicht, was im Quelltext
    #    steht. Genau daran ist der Doppel-Eintrag (Pruefung 91) nur auf einem
    #    Bildschirmfoto aufgefallen.
    _wz92 = _wurzel()
    _ov92 = _w91.Overlay(_wz92)
    _ov92.auftraege_zeigen([('retake', 'Auftrag: Retake Platforms',
                             ['Dach erreichen', 'Remy Kettle eliminieren'])])
    for _ in range(4):
        _wz92.update()
        _wz92.update_idletasks()
    pruefe(len(_texte91(_ov92, 'Dach erreichen')) == 1
           and len(_texte91(_ov92, 'Remy Kettle')) == 1,
           'beide Ziele stehen unter ihrem Auftrag')

    # g) ⚠⚠ **Die alte Form muss weiter gehen.** Eine Anzeige, die nur Paare
    #    bekommt, darf nicht mit einem Fehler enden, nur weil kein Ziel anliegt.
    _ov92.auftraege_zeigen([('kill', 'Auftrag: Kill the king')])
    for _ in range(4):
        _wz92.update()
        _wz92.update_idletasks()
    pruefe(len(_texte91(_ov92, 'Kill the king')) == 1,
           'ein Auftrag ohne Ziele wird weiterhin angezeigt')

    # h) Was nicht mehr passt, wird gezaehlt statt verschwiegen.
    _viele92 = ['Ziel %d' % _i for _i in range(_au92.ZIELE_MAX + 3)]
    _ov92.auftraege_zeigen([('viel', 'Auftrag: Viele Ziele', _viele92)])
    for _ in range(4):
        _wz92.update()
        _wz92.update_idletasks()
    pruefe(len(_texte91(_ov92, 'Ziel ')) == _au92.ZIELE_MAX,
           'hoechstens %d Ziele stehen untereinander' % _au92.ZIELE_MAX)
    pruefe(len(_texte91(_ov92, '3')) >= 1,
           'und der Rest wird gezaehlt, nicht verschwiegen')


    # 93. „Bestand zurücksetzen" sagt immer, was passiert ist
    #
    # ⚠⚠ **Am 31.08.2026 aus einem Nutzerbericht** (Linux, CachyOS, v3.4.2,
    # „Inventory 0 blueprints"):
    #
    #     seiten.bestand.zuruecksetzen
    #     FileNotFoundError: .../Bauplaene/bestand.json
    #
    # Wer noch keinen einzigen Bauplan hat, hat auch keine Bestandsdatei. Das
    # `os.remove` warf, der Fehler ging still in die Diagnose — und auf dem
    # Bildschirm passierte nach dem roten Knopf und der Warnfrage **nichts**.
    # Kein Haken, keine Meldung. Von einem kaputten Knopf nicht zu
    # unterscheiden, obwohl der Zustand genau der gewünschte war.
    #
    # ⚠ **Geprüft wird das Modul, nicht die Oberfläche.** Genau dafür ist die
    # Entscheidung aus `seiten.py` herausgezogen worden: So läuft die Prüfung
    # ohne Fenster — auf jedem System und im Bau-Lauf.
    print()
    print('93. „Bestand zuruecksetzen" sagt immer, was passiert ist')
    from scbp import bestand as _bd93, pfade as _pf93

    _datei93 = _pf93.app_datei('bestand.json')
    _bd93.speichern({'bauplaene': {'xl-1': {'name': 'XL-1'}}})
    pruefe(os.path.exists(_datei93), 'ein Bestand liegt da')

    pruefe(_bd93.zuruecksetzen() is None, 'das Zuruecksetzen meldet Erfolg')
    pruefe(not os.path.exists(_datei93), 'und die Datei ist weg')

    # ⭐ Der gemeldete Fall: noch einmal, jetzt ohne Datei.
    pruefe(_bd93.zuruecksetzen() is None,
           'ein zweites Mal ist ebenfalls Erfolg — „war schon weg" ist weg')

    # Und der Bestand liest sich danach als leer, faellt also NICHT auf die
    # Vorgaengerfassung zurueck. Sonst waere das Zuruecksetzen wirkungslos.
    pruefe(_bd93.laden().get('bauplaene') == {},
           'danach ist der Bestand wirklich leer')

    # ⚠ Eine echte Stoerung muss dagegen zurueckkommen — sonst schluckt der
    # Knopf ein „keine Rechte" und behauptet Erfolg.
    _echt93 = os.remove
    os.remove = _machs93 = lambda *_a, **_k: (_ for _ in ()).throw(
        PermissionError(13, 'kein Zugriff'))
    try:
        _stoerung93 = _bd93.zuruecksetzen()
    finally:
        os.remove = _echt93
    pruefe(isinstance(_stoerung93, OSError)
           and not isinstance(_stoerung93, FileNotFoundError),
           'eine echte Stoerung kommt zurueck statt verschluckt zu werden')

    # Und die Oberflaeche muss sie auch ZEIGEN, nicht nur wegschreiben.
    _q93 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                encoding='utf-8').read()
    _ab93 = _q93.split('def zuruecksetzen():')[-1].split('_knopf(')[0]
    pruefe(_ab93.count('fenster.sagen') == 2,
           'beide Ausgaenge melden sich beim Nutzer')
    pruefe("t('s_be_reset_fehler'" in _ab93,
           'und der Fehlschlag hat einen eigenen Text')


    # 94. Der Bericht sagt selbst, ob die Log-Erkennung greift
    #
    # ⚠⚠ **Weil Rueckfragen oft nicht gehen.** Am 31.08.2026 kam ein Bericht
    # mit „462 Protokolle" und „0 Baupläne" — ohne Absender, ohne Nachricht.
    # Daraus war NICHT zu erkennen, ob die Erkennung bei dem Menschen versagt
    # oder ob er einfach neu im Spiel ist. Genau das ist aber der Unterschied
    # zwischen „alles in Ordnung" und „das Werkzeug ist fuer ihn wertlos".
    #
    # | Was dasteht | Was es heisst |
    # |---|---|
    # | 462 · 462 durchgesehen · 0 daraus | die Erkennung findet nichts |
    # | 462 · 0 durchgesehen · 0 daraus | die Nachlese lief nie |
    # | 462 · 462 durchgesehen · 380 daraus | alles in Ordnung |
    print()
    print('94. Der Bericht sagt selbst, ob die Log-Erkennung greift')
    from scbp import bericht as _be94, bestand as _bd94

    def _zahlen94(text):
        return [int(_x) for _x in re.findall(r'\d+', text)]

    _bd94.speichern(_bd94.leer())
    _zeile94 = _be94._protokollzeile()
    _z94 = _zahlen94(_zeile94)
    pruefe(len(_z94) == 3,
           'die Zeile nennt drei Zahlen: vorhanden, gelesen, gefunden (%r)'
           % _zeile94)
    pruefe(_z94[0] == len(w.pfade.log_sicherungen()),
           'die erste Zahl ist die Zahl der Protokolle')
    pruefe(_z94[2] == 0, 'ohne Bestand steht hinten eine Null')

    # ⭐ Der Kern: Nur Funde AUS PROTOKOLLEN zaehlen. Was vom Launcher, von
    # Hand oder aus den Startbauplaenen kam, sagt ueber die Log-Erkennung
    # nichts — und genau die steht hier zur Frage.
    _daten94 = _bd94.leer()
    _bd94.hinzufuegen(_daten94, 'Aus dem Log', 'log')
    _bd94.hinzufuegen(_daten94, 'Aus der Nachlese', 'nachlese')
    _bd94.hinzufuegen(_daten94, 'Vom Launcher', 'launcher')
    _bd94.hinzufuegen(_daten94, 'Von Hand', 'hand')
    _bd94.hinzufuegen(_daten94, 'Startbauplan', 'start')
    _bd94.speichern(_daten94)
    _z94b = _zahlen94(_be94._protokollzeile())
    pruefe(_z94b[2] == 2,
           'nur die zwei aus Protokollen werden gezaehlt, nicht alle fuenf '
           '(gezaehlt: %d)' % _z94b[2])

    # ⚠ Und die Zeile darf NIE stuerzen — sie steht in einem Bericht, den
    # jemand abschickt, weil ohnehin schon etwas kaputt ist.
    _echt94 = w.pfade.log_sicherungen
    w.pfade.log_sicherungen = lambda *_a, **_k: (_ for _ in ()).throw(
        OSError('Platte weg'))
    try:
        _kaputt94 = None
        try:
            _be94._protokollzeile()
        except Exception as _f94:
            _kaputt94 = _f94
    finally:
        w.pfade.log_sicherungen = _echt94
    pruefe(_kaputt94 is None,
           'die Zeile bricht den Bericht nicht ab (%s)' % _kaputt94)

    _bd94.speichern(_bd94.leer())


    # 95. Rot heisst „weg", nicht „irgendwas Wichtiges"
    #
    # ⚠⚠ **Am 31.08.2026 gemeldet von Haldjas** — zwei Sachen auf einmal:
    #
    # 1. Es gab **zwei** Knoepfe, die die Protokolle neu lesen: einer unter
    #    „Erkennung" (wirkte erst beim naechsten Start) und einer unter
    #    „Bestand" (sofort). Der erste konnte strikt weniger. Wer ihn
    #    erwischte, glaubte, das Werkzeug koenne es nicht.
    # 2. Der verbliebene war **rot** — und direkt darunter steht das ebenfalls
    #    rote „Bestand zuruecksetzen", das wirklich loescht. Haldjas drueckte
    #    den harmlosen, und es brauchte einen Zuruf hinterher. Zwei
    #    Bedeutungen fuer dieselbe Farbe heissen: Die Farbe warnt nicht mehr.
    #
    # ⚠ Nachgesehen, nicht geglaubt: `neu_einlesen` landet ueber den Watcher
    # bei `bestand.hinzufuegen` — und das LEGT AN. Nichts wird entfernt,
    # nichts ueberschrieben, doppelt kann nichts werden.
    print()
    print('95. Rot heisst „weg", nicht „irgendwas Wichtiges"')
    from scbp import bestand as _bd95

    # a) Der Beleg, dass der Knopf harmlos ist: zweimal dasselbe einlesen
    #    aendert nichts, und Vorhandenes bleibt stehen.
    _stand95 = _bd95.leer()
    _bd95.hinzufuegen(_stand95, 'Vom Launcher', 'launcher')
    _bd95.hinzufuegen(_stand95, 'Aus dem Log', 'log')
    _vorher95 = dict(_stand95['bauplaene'])
    for _ in range(3):
        _bd95.hinzufuegen(_stand95, 'Aus dem Log', 'nachlese')
        _bd95.hinzufuegen(_stand95, 'Vom Launcher', 'nachlese')
    pruefe(_stand95['bauplaene'].keys() == _vorher95.keys(),
           'erneutes Einlesen legt nichts doppelt an')
    pruefe(_stand95['bauplaene'][_bd95.norm('Vom Launcher')]['quelle']
           == 'launcher',
           'und stuft eine bessere Quelle nicht herunter')

    # b) Die Farben. @ Geprueft wird der Aufruf, denn `gefahr=True` ist der
    #    einzige Unterschied — am fertigen Knopf ist er nur noch Pixel.
    _q95 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                encoding='utf-8').read()
    _be95 = _q95.split('def _bestand(')[1].split(chr(10) + 'def ')[0]

    def _knopfzeile95(schluessel):
        for _z95 in _be95.split(chr(10)):
            if '_knopf(' in _z95 and schluessel in _z95:
                return _z95
        return ''

    _neu95 = _knopfzeile95("t('s_be_neu')")
    _reset95 = _knopfzeile95("t('s_zuruecksetzen')")
    pruefe(_neu95 and 'gefahr' not in _neu95,
           '„Protokolle erneut einlesen" ist NICHT rot (%s)' % _neu95.strip())
    pruefe(_reset95 and 'gefahr=True' in _reset95,
           '„Bestand zuruecksetzen" ist weiterhin rot (%s)' % _reset95.strip())

    # c) Und es gibt nur noch EINEN Weg, die Protokolle neu zu lesen.
    #
    # ⚠ **Kommentare zaehlen nicht mit.** Beim Schreiben dieser Pruefung sind
    # beide Zeilen zuerst an der Erklaerung haengengeblieben, warum der zweite
    # Knopf weg ist — der Text erwaehnt ihn ja. Geprueft wird der CODE.
    _code95 = chr(10).join(_z for _z in _q95.split(chr(10))
                           if not _z.strip().startswith('#'))
    pruefe(_code95.count('neu_einlesen_anstossen') == 1,
           'nur eine Stelle stoesst das erneute Einlesen an (%d)'
           % _code95.count('neu_einlesen_anstossen'))
    pruefe('logstand.json' not in _code95,
           'der zweite Knopf unter „Erkennung" ist weg — kein Loeschen des '
           'Lesestands mehr in der Oberflaeche')

    # d) Und keine verwaisten Sprachschluessel zurueckgelassen.
    _sp95 = open(os.path.join(WURZEL, 'scbp', 'sprache.py'),
                 encoding='utf-8').read()
    pruefe('s_er_alt' not in _sp95,
           'die Texte des entfernten Knopfes sind mitgegangen')


    # 96. Das Ergebnis des Einlesens geht nicht in der Leiste unter
    #
    # ⚠⚠ **Am 31.08.2026 gemeldet:** „waere eine Meldung mit Fenster
    # sinnvoller, in der Leiste steht es zu kurz oder gar nicht." Die Fusszeile
    # zeigt vier Sekunden und ist dann leer — und genau in diesen vier Sekunden
    # sieht niemand hin, der gerade einen Lauf ueber hunderte Protokolle
    # angestossen hat. Er hat den Knopf gedrueckt und wartet.
    #
    # ⚠ **Die Leiste bekommt es trotzdem.** Den Knopf gibt es auch am Overlay;
    # ist das Hauptfenster zu, existiert kein Fenster, ueber dem ein Dialog
    # stehen koennte. Dann bleibt die Zeile — verschluckt wird das Ergebnis nie.
    print()
    print('96. Das Ergebnis des Einlesens geht nicht in der Leiste unter')
    from scbp import hauptfenster as _hf96

    pruefe(hasattr(_hf96, 'bescheid_geben'),
           'es gibt einen Weg, ein Ergebnis als Fenster zu zeigen')

    _q96 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'),
                encoding='utf-8').read()
    _code96 = chr(10).join(_z for _z in _q96.split(chr(10))
                           if not _z.strip().startswith('#'))

    # a) Der Watcher schickt das Ergebnis als Bescheid — nicht als blosse Zeile.
    _ab96 = _code96.split('def _alles_neu_einlesen')[1].split(chr(10) + '    def ')[0]
    pruefe("'bescheid'" in _ab96,
           'das Ergebnis des Einlesens kommt als Bescheid')
    pruefe("('status', sprache.Satz('neu_gelesen'" not in _ab96,
           'und nicht mehr nur als Statuszeile')
    pruefe('neu_gelesen_fehler' in _ab96 and _ab96.count("'bescheid'") == 2,
           'auch der Fehlschlag meldet sich als Bescheid')

    # b) Die Anzeige kennt die Meldungsart — sonst faellt sie stumm durch.
    pruefe("msg[0] == 'bescheid'" in _code96,
           'die Anzeige wertet die neue Meldungsart aus')

    # c) ⚠ Und sie setzt IMMER auch die Leiste, bevor sie ein Fenster
    #    versucht. Ohne das waere ein zugeklapptes Hauptfenster gleich­
    #    bedeutend mit „Ergebnis weg".
    _bz96 = _code96.split('def _bescheid_zeigen')[1].split(chr(10) + '    def ')[0]
    pruefe(_bz96.index('_status_setzen') < _bz96.index('bescheid_geben'),
           'die Leiste wird gesetzt, BEVOR ein Fenster versucht wird')
    pruefe('if fenster is None' in _bz96,
           'ohne Hauptfenster bleibt es bei der Leiste, ohne Fehler')

    # d) Der Dialog braucht seinen einen Knopf — und einen Text dafuer.
    from scbp import sprache as _sp96
    pruefe(_sp96.t('e_ok') and _sp96.t('e_ok') != 'e_ok',
           'der Knopf des Bescheids hat einen Text')
    _hq96 = open(os.path.join(WURZEL, 'scbp', 'hauptfenster.py'),
                 encoding='utf-8').read()
    pruefe('nur_ok' in _hq96 and 'if not nur_ok:' in _hq96,
           'beim Bescheid entfaellt der zweite Knopf — es gibt nichts zu waehlen')

    # e) Und die Zusage in der Leiste darf nicht mehr „steht in der Leiste"
    #    versprechen, wenn ein Fenster kommt.
    pruefe('Leiste' not in _sp96.t('s_be_neu_los'),
           'der Zwischenstand verspricht nicht mehr die Leiste')


    # 97. Von der Herstellung zum Bauplan — und den Knopf auch finden
    #
    # ⚠⚠ **Beides von Bushwick4712 (KRT) am 31.08.2026.**
    #
    # 1. „Ich kann das nicht bauen — woher bekomme ich den Bauplan?" Die
    #    Antwort stand schon im Werkzeug, aber auf einer anderen Seite: Man
    #    musste wissen, dass es sie gibt, und den Namen von Hand
    #    hinuebertippen.
    # 2. Den Knopf dorthin hat er **nicht gefunden**. Es war ein Symbol am
    #    rechten Rand der Zeile, ohne Wort. Ein Symbol erklaert sich nur dem,
    #    der es gebaut hat.
    print()
    print('97. Von der Herstellung zum Bauplan — und den Knopf auch finden')
    from scbp import seiten as _se97, sprache as _sp97

    # a) Der Knopf erscheint nur, wo er hinfuehrt. ⚠ Der Katalog kennt 738
    #    Bauplaene, die Rezepte sind 1607 — ein Knopf auf eine leere Liste
    #    waere schlimmer als keiner.
    pruefe(_se97._hat_herkunft('gibt es nicht') is False,
           'ein unbekannter Name bekommt keinen Knopf')
    pruefe(_se97._hat_herkunft('') is False and _se97._hat_herkunft(None) is False,
           'und ein leerer Name auch nicht')

    from scbp import katalog as _kat97
    from scbp import pfade as _pf97

    # ⚠⚠ **Notfalls eigene Daten hinlegen.** Der Selbsttest arbeitet in einem
    # Wegwerf-Ordner, dort liegt kein Katalog — ohne das hier waeren genau die
    # zwei interessanten Pruefungen IMMER uebersprungen worden, auf jedem
    # frischen Rechner und im Bau-Lauf sowieso. Dieselbe Falle wie bei
    # Pruefung 67: Eine Pruefung, die nur bei ihrem Autor anschlaegt, ist keine.
    if not (_kat97.laden().get('bauplaene') or {}):
        with open(_pf97.app_datei(_kat97.CACHE), 'w', encoding='utf-8') as _f97:
            json.dump({'bauplaene': {
                'mit-quelle': {'n': 'Pruefling Mit Quelle',
                               'q': [{'f': 'Foxwell', 'a': 'Testauftrag'}]},
                'ohne-quelle': {'n': 'Pruefling Ohne Quelle'}}}, _f97)

    _mit_q97 = [e.get('n') for e in (_kat97.laden().get('bauplaene') or {}).values()
                if e.get('q') and e.get('n')]
    _ohne_q97 = [e.get('n') for e in (_kat97.laden().get('bauplaene') or {}).values()
                 if not e.get('q') and e.get('n')]
    if _mit_q97:
        pruefe(_se97._hat_herkunft(_mit_q97[0]),
               'ein Bauplan MIT Bezugsquelle bekommt ihn (%s)' % _mit_q97[0])
    else:
        print('  [-]    kein Katalog mit Bezugsquellen — uebersprungen')
    if _ohne_q97:
        pruefe(not _se97._hat_herkunft(_ohne_q97[0]),
               'ein Bauplan OHNE Bezugsquelle bekommt keinen (%s)' % _ohne_q97[0])

    # b) Der Weg dorthin gibt es, und er sagt ehrlich, wenn er nichts findet.
    from scbp import bestandsfenster as _bf97
    pruefe(hasattr(_bf97.Bestandsfenster, 'zum_bauplan'),
           'die Bauplan-Liste laesst sich von aussen auf einen Bauplan stellen')

    _wz97 = _wurzel()
    _liste97 = _bf97.Bestandsfenster(_wz97)
    for _ in range(3):
        _wz97.update(); _wz97.update_idletasks()
    _liste97.suche.set('etwas anderes')
    pruefe(_liste97.zum_bauplan('gibt es nicht') is False,
           'ein unbekannter Bauplan meldet Fehlanzeige')
    # ⚠⚠ **Und ruehrt die Liste nicht an.** Ein Sprung auf eine leere Liste
    # saehe aus, als sei das Werkzeug kaputt — und der Suchbegriff von vorhin
    # waere obendrein weg.
    pruefe(_liste97.suche.get() == 'etwas anderes',
           'und laesst die Suche stehen, wie sie war')
    if _mit_q97:
        pruefe(_liste97.zum_bauplan(_mit_q97[0]) is True,
               'ein bekannter Bauplan wird angesprungen')
        pruefe(_liste97.suche.get() == _mit_q97[0]
               and getattr(_liste97, 'gewaehlt', None) == _mit_q97[0],
               'er steht in der Suche UND ist ausgewaehlt — die Herkunft '
               'schlaegt gleich auf')

    # c) Und derselbe Weg fuer einen ganzen AUFTRAG.
    #
    # „Was bringt am meisten?" nennt einen Auftrag mit einer Zahl daneben —
    # die naechste Frage ist immer „und welche Bauplaene sind das?". Die Liste
    # kann darauf filtern, war von dort aus aber nicht erreichbar.
    pruefe(hasattr(_bf97.Bestandsfenster, 'zum_auftrag'),
           'die Liste laesst sich auch auf einen ganzen Auftrag stellen')
    _auf97 = sorted({(q.get('auftrag') or '').strip()
                     for e in (_kat97.laden().get('bauplaene') or {}).values()
                     for q in (e.get('q') or [])
                     if (q.get('auftrag') or '').strip()})
    _liste97.auftrag = ''
    _liste97.suche.set('etwas anderes')
    pruefe(_liste97.zum_auftrag('Auftrag den es nicht gibt') is False
           and _liste97.auftrag == '',
           'ein unbekannter Auftrag meldet Fehlanzeige und setzt nichts')
    pruefe(_liste97.suche.get() == 'etwas anderes',
           'und laesst auch hier die Suche stehen')
    pruefe(_liste97.zum_auftrag('') is False,
           'ein leerer Auftragsname ebenso — nicht die ganze Liste filtern')
    if _auf97:
        pruefe(_liste97.zum_auftrag(_auf97[0]) is True
               and _liste97.auftrag == _auf97[0],
               'ein bekannter Auftrag wird gesetzt (%s)' % _auf97[0][:40])
        # ⚠ **Gesetzt, nicht umgeschaltet.** `_auftrag_waehlen` loest denselben
        # Auftrag beim zweiten Klick wieder — richtig in der Liste, falsch fuer
        # einen Sprung von aussen: Wer zweimal herspringt, will zweimal
        # dasselbe sehen, nicht beim zweiten Mal alles.
        pruefe(_liste97.zum_auftrag(_auf97[0]) is True
               and _liste97.auftrag == _auf97[0],
               'zweimal hintereinander loest ihn NICHT wieder')
        # Und der Zustandsfilter darf nicht dazwischenfunken: Steht er auf
        # „fehlt mir", fehlen genau die Zeilen, die man schon hat — und die
        # Zahl auf der Fortschritt-Seite passt nicht mehr zum Bild.
        pruefe(_liste97.filter == 'alle',
               'der Zustandsfilter steht auf „alle", sonst fehlen Zeilen')
    try:
        _liste97.root.destroy()
        _wz97.destroy()
    except Exception:
        pass

    # c) Der Knopf traegt jetzt ein Wort, nicht nur ein Zeichen.
    _bq97 = open(os.path.join(WURZEL, 'scbp', 'bestandsfenster.py'),
                 encoding='utf-8').read()
    pruefe("text=t('hk_knopf')" in _bq97,
           'der Herkunfts-Knopf in der Liste ist beschriftet')
    pruefe(_sp97.t('hk_knopf') and _sp97.t('hk_knopf') != 'hk_knopf',
           'und die Beschriftung gibt es in beiden Sprachen')

    # d) Und in der Herstellung steht er nur bei einem FEHLENDEN Bauplan.
    _sq97 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                 encoding='utf-8').read()
    _hz97 = _sq97.split('def _herstellung_zeile')[1].split(chr(10) + 'def ')[0]
    pruefe("eintrag['habe'] is not True and _hat_herkunft" in _hz97,
           'der Knopf steht nur, wo der Bauplan fehlt UND es ihn irgendwo gibt')


    # 98. Die Titelleiste ist wirklich dunkel — nicht nur „erfolgreich gesetzt"
    #
    # ⚠⚠ **v3.6.0 hat genau hier gelogen.** `DwmSetWindowAttribute` gab S_OK
    # zurueck, das Werkzeug hielt sich fuer fertig — und auf dem Bildschirm
    # sass weiter eine weisse Leiste. Am 31.08.2026 mit Bildschirmfoto
    # gemeldet: „Meine Leiste ist weiss."
    #
    # Zwei Dinge kamen zusammen, beide erst durch Nachmessen sichtbar:
    #
    # 1. Der Aufruf **vor** dem ersten Anzeigen ging ins Leere — zu dem
    #    Zeitpunkt gibt es das Fenster-Handle noch gar nicht (`GetParent`
    #    liefert 0), und er meldete das nicht.
    # 2. Beim Anzeigen wurde die Einstellung zwar gesetzt, aber Windows
    #    zeichnet einen Rahmen, der schon steht, nicht von selbst neu.
    #
    # ⚠ **Deshalb prueft das hier den ZUSTAND, nicht den Rueckgabewert.** Die
    # Einstellung wird zurueckgelesen. Ein „hat geklappt" vom System war ja
    # gerade das, was in die Irre gefuehrt hat.
    print()
    print('98. Die Titelleiste ist wirklich dunkel')
    from scbp import titelleiste as _tl98

    if not sys.platform.startswith('win'):
        print('  [-]    kein Windows — die Leiste macht hier der Fenstermanager')
    else:
        import ctypes as _ct98
        from ctypes import wintypes as _wt98

        pruefe(_tl98.einrichten() is True, 'der Haken laesst sich setzen')

        import tkinter as tk98
        _wz98 = _wurzel()
        _top98 = tk98.Toplevel(_wz98)
        _top98.title('Pruefung 98')
        # ⚠ Weit neben jeden Bildschirm. Die Pruefung MUSS das Fenster
        # anzeigen — vorher gibt es kein Handle —, aber niemand soll es sehen.
        _top98.geometry('300x120+9000+9000')
        pruefe(not _tl98._griff(_top98),
               'vor dem Anzeigen gibt es noch kein Handle — der frueher hier '
               'stehende Aufruf war wirkungslos')
        _top98.deiconify()
        for _ in range(6):
            _wz98.update()
            _wz98.update_idletasks()

        _h98 = _tl98._griff(_top98)
        pruefe(bool(_h98), 'nach dem Anzeigen gibt es eines')

        # ⚠⚠ **Hier wird von Hand ausgeloest, nicht auf `<Map>` gewartet.**
        # Der Pruefbetrieb laeuft unter `unsichtbar`, und das legt `deiconify`
        # still — sonst blitzten die Fenster auf dem Bildschirm des Nutzers
        # auf. Damit feuert `<Map>` nie. Dass der Haken daran haengt, prueft
        # weiter unten der Quelltext; hier geht es um die WIRKUNG.
        _tl98._einmal(_top98)
        pruefe(getattr(_top98, '_scbp_leiste_gesetzt', False) is True,
               'der Haken merkt sich, dass er dieses Fenster erledigt hat')

        _wert98 = _ct98.c_int(-1)
        _ct98.windll.dwmapi.DwmGetWindowAttribute(
            _wt98.HWND(_h98), _ct98.c_uint(20),
            _ct98.byref(_wert98), _ct98.sizeof(_wert98))
        pruefe(_wert98.value == 1,
               'die dunkle Leiste steht wirklich am Fenster (zurueckgelesen: '
               '%d)' % _wert98.value)

        # ⭐ Und das Neuzeichnen ist da — ohne das blieb sie weiss.
        pruefe(_tl98.rahmen_neu(_top98) is True,
               'der Rahmen laesst sich zum Neuzeichnen zwingen')
        _q98 = open(os.path.join(WURZEL, 'scbp', 'titelleiste.py'),
                    encoding='utf-8').read()
        _code98 = chr(10).join(_z for _z in _q98.split(chr(10))
                               if not _z.strip().startswith('#'))
        pruefe('rahmen_neu(fenster)' in _code98.split('def _einmal')[1],
               'und wird beim Anzeigen auch gerufen')
        # ⚠ Und der Haken haengt wirklich am Anzeigen — genau das laesst sich
        # unter `unsichtbar` nicht ausloesen, also wird es hier gelesen.
        pruefe("bind('<Map>'" in _code98,
               'jedes Fenster bekommt den Haken beim Anzeigen')
        # ⚠ Ohne NOACTIVATE risse das Neuzeichnen den Fokus an sich — wer
        # gerade Star Citizen fliegt, landet mitten im Kampf auf dem Desktop.
        pruefe('SWP_NOACTIVATE' in _code98,
               'und tut das, ohne den Fokus zu klauen')

        try:
            _top98.destroy()
            _wz98.destroy()
        except Exception:
            pass

    # ⭐⭐ Der Merker steht ERST nach dem Erfolg (Fund vom 02.09.2026)
    #
    # Bis dahin setzte `_einmal` ihn eine Zeile zu frueh — vor dem Versuch.
    # Lieferte `GetParent` in diesem Moment noch 0, galt das Fenster trotzdem
    # als erledigt und bekam **nie wieder** einen Versuch: dauerhaft helle
    # Leiste. Weil es ein Wettlauf war, traf es mal das eine Fenster und mal
    # keines — am laufenden Programm gemessen: drei Fenster mit gesetztem
    # Attribut, das sichtbare Hauptfenster mit 0.
    #
    # ⚠ Bewusst OHNE echtes Fenster und ohne Windows: Der Fehler steckt in der
    # Reihenfolge, nicht in der Systemschnittstelle. So greift die Pruefung
    # auch unter Linux, wo der Rueckfall sonst niemandem auffiele — genau die
    # Sorte Pruefung, die sich sonst selbst ueberspringt.
    print()
    print('98b. Ein misslungener Faerbe-Versuch sperrt das Fenster nicht aus')

    class _Attrappe98:
        """Nur so viel Fenster, wie `_einmal` anfasst."""

        def __init__(self):
            self.geplant = []

        def after(self, ms, rueckruf):
            self.geplant.append((ms, rueckruf))

    _dunkel98, _rahmen98 = _tl98.dunkel, _tl98.rahmen_neu
    try:
        _tl98.dunkel = lambda _f: False
        _a98 = _Attrappe98()
        _tl98._einmal(_a98)
        pruefe(getattr(_a98, '_scbp_leiste_gesetzt', False) is False,
               'scheitert das Faerben, gilt das Fenster NICHT als erledigt')
        pruefe(len(_a98.geplant) == 1,
               'stattdessen wird nachgefasst (%d geplant)' % len(_a98.geplant))

        # Und das Nachfassen laeuft nicht ewig: nach NACHFASSEN Versuchen ist
        # Ruhe, bis das naechste <Map> kommt.
        _b98 = _Attrappe98()
        _tl98._einmal(_b98, _tl98.NACHFASSEN - 1)
        pruefe(not _b98.geplant,
               'beim letzten Versuch wird nicht weiter nachgefasst')

        # Klappt es, wird gemerkt — sonst liefe das Neuzeichnen bei jedem
        # Wiederherstellen aus der Taskleiste erneut.
        _tl98.dunkel = lambda _f: True
        _tl98.rahmen_neu = lambda _f: True
        _c98 = _Attrappe98()
        _tl98._einmal(_c98)
        pruefe(getattr(_c98, '_scbp_leiste_gesetzt', False) is True,
               'klappt es, wird es gemerkt')
        pruefe(not _c98.geplant, 'und nicht weiter nachgefasst')
    finally:
        _tl98.dunkel, _tl98.rahmen_neu = _dunkel98, _rahmen98


    # 98c. Das Fenster baut NUR die gewuenschte Seite — und zeigt sich fertig
    #
    # ⚠⚠ Gemeldet von Haldjas (pr0): „Er braucht eben recht lang, um die Icons
    # und co zu laden, wenn man die Einstellungen oeffnet." Zwei Ursachen, beide
    # am 02.09.2026 gefunden — und beide standen laengst im Fehlerbericht:
    #
    #   Seite liste: steht (205 ms)     <- gar nicht angefordert
    #   Seite allgemein: steht (7 ms)   <- das war der Wunsch
    #
    # 1. Der Konstruktor baute fest `oeffnen('liste')`, der Aufrufer oeffnete
    #    die gewollte Seite erst danach. Wer die Einstellungen aufmachte,
    #    wartete auf den Aufbau der ganzen Bauplan-Liste.
    # 2. Ein `Toplevel` steht ab der Erzeugung auf dem Bildschirm — man sah dem
    #    Fenster beim Bauen zu. Deshalb `withdraw()` am Anfang, `deiconify()`
    #    als letzte Zeile.
    #
    # ⚠ Die Zeit war nie das Problem: 20 Symbolbilder brauchen 8 ms. Wer hier
    # etwas „optimiert", sucht an der falschen Stelle — es ging um eine Seite
    # zu viel und um den Zeitpunkt des Anzeigens.
    print()
    print('98c. Das Fenster baut nur die gewuenschte Seite')

    from scbp import hauptfenster as _hf98c
    _wz98c = _wurzel()
    _f98c = _hf98c.Hauptfenster(_wz98c, version='pruefung',
                                startseite='allgemein')
    pruefe('allgemein' in _f98c.seiten,
           'die gewuenschte Seite ist da')
    pruefe('liste' not in _f98c.seiten,
           'und die Bauplan-Liste wurde NICHT nebenbei mitgebaut '
           '(gebaut: %s)' % ', '.join(sorted(_f98c.seiten)))

    # Der Standard bleibt die Liste — sonst aendert sich das Verhalten fuer
    # alle anderen Aufrufer (Werkzeuge unter tools/, Bilder, Randpruefung).
    _f98d = _hf98c.Hauptfenster(_wz98c, version='pruefung')
    pruefe('liste' in _f98d.seiten,
           'ohne Angabe bleibt es bei der Liste')

    _q98c = open(os.path.join(WURZEL, 'scbp', 'hauptfenster.py'),
                 encoding='utf-8').read()
    _init98c = _q98c.split('def __init__')[1].split(chr(10) + '    def ')[0]
    _code98c = chr(10).join(_z for _z in _init98c.split(chr(10))
                            if not _z.strip().startswith('#'))
    pruefe('withdraw()' in _code98c,
           'das Fenster wird beim Bauen versteckt')
    pruefe('deiconify()' in _code98c,
           'und am Ende wieder gezeigt')
    # ⚠ Die Reihenfolge ist der ganze Punkt: zeigen NACH dem Bauen.
    pruefe(_code98c.index('withdraw()') < _code98c.index('oeffnen(startseite)')
           < _code98c.index('deiconify()'),
           'und zwar in dieser Reihenfolge: verstecken, bauen, zeigen')

    for _f in (_f98c, _f98d):
        try:
            _f.root.destroy()
        except Exception:
            pass
    try:
        _wz98c.destroy()
    except Exception:
        pass


    # 98d. Ein Rezept-Nachschlag fragt nicht jedes Mal das Dateisystem
    #
    # ⚠⚠ **Der groesste Einzelposten beim Oeffnen der Bauplan-Liste.**
    # `rezept_roh()` rief bei JEDEM Nachschlag `laden()`, und das macht ein
    # `os.stat`. Einzeln belanglos, ueber den Katalog toedlich: am 02.09.2026
    # gemessen **738 Nachschlaege = 51 ms, davon 50 ms allein `laden()`** —
    # der reine Verzeichnis-Zugriff kostete 0,1 ms. Nach der Drosselung
    # (`_ROH_FRISCH_S`) waren es 0,7 ms.
    #
    # ⚠ Gezaehlt wird, nicht gestoppt: Eine Zeitmessung im Selbsttest haengt
    # von der Tagesform des Rechners ab und wird frueher oder spaeter zur
    # Zufallspruefung. Die Zahl der `laden()`-Aufrufe ist dagegen eindeutig.
    #
    # ⚠ Und die Gegenrichtung gehoert dazu: Eine Drosselung, die eine echte
    # Aenderung verschluckt, waere schlimmer als die Langsamkeit. Deshalb
    # prueft der zweite Teil, dass `_sichern()` sofort durchschlaegt.
    print()
    print('98d. Rezept-Nachschlaege fragen das Dateisystem nur einmal')

    from scbp import herstellung as _hs98d

    # ⚠ Eigene Daten hinlegen — im Wegwerf-Ordner gibt es keine Rezepte, und
    # eine Pruefung, die sich mangels Daten selbst ueberspringt, prueft nichts.
    _daten98d = {'format': _hs98d.FORMAT, 'blueprints': [
        {'productName': 'Pruefteil A', 'tag': 'BP_TEST_A', 'type': 'weapons'},
        {'productName': 'Pruefteil B', 'tag': 'BP_TEST_B', 'type': 'cooler'},
    ]}
    pruefe(_hs98d._sichern(_daten98d) is True, 'Pruefdaten liegen bereit')

    _echt_laden98d = _hs98d.laden
    _zaehler98d = [0]

    def _laden_gezaehlt98d():
        _zaehler98d[0] += 1
        return _echt_laden98d()

    _hs98d.laden = _laden_gezaehlt98d
    try:
        for _ in range(50):
            _hs98d.rezept_roh('Pruefteil A')
        pruefe(_zaehler98d[0] == 1,
               '50 Nachschlaege = 1 Dateisystem-Abfrage (gemessen: %d)'
               % _zaehler98d[0])
        pruefe(_hs98d.rezept_roh('Pruefteil B') is not None,
               'und gefunden wird trotzdem alles')

        # Jetzt die Gegenrichtung: Aenderung muss SOFORT sichtbar sein.
        _daten98d['blueprints'][0]['tag'] = 'BP_TEST_A_NEU'
        _hs98d._sichern(_daten98d)
        _neu98d = _hs98d.rezept_roh('Pruefteil A') or {}
        pruefe(_neu98d.get('tag') == 'BP_TEST_A_NEU',
               'nach dem Speichern gilt sofort der neue Stand (%s)'
               % _neu98d.get('tag'))
    finally:
        _hs98d.laden = _echt_laden98d


    # 98e. Der Seiten-Vorbau bleibt abgeschaltet
    #
    # ⚠ Er baute alle Seiten im Hintergrund vor und hielt Tk dabei **1,7 s**
    # am Stueck fest (17 Seiten; `wasistneu` 181 ms, `diagnose` 162 ms).
    # Getroffen wurde jeweils das, was der Nutzer gerade anfasste — gemeldet
    # mal als traege Seitenleiste, mal als traege Bauplan-Liste. Beschleunigt
    # hat er nie etwas, er verlagerte nur (steht so in seiner eigenen
    # Beschreibung). Abgeschaltet am 02.09.2026 auf Ansage.
    #
    # ⚠ Diese Pruefung verbietet ihn NICHT — sie sorgt dafuer, dass ein
    # Wiedereinschalten eine bewusste Entscheidung ist und nicht aus Versehen
    # passiert. Wer ihn zurueckholt, muss zuerst das Zeichnen der angeklickten
    # Seite sicherstellen und diese Pruefung mit anfassen.
    print()
    print('98e. Der Seiten-Vorbau ist abgeschaltet')
    _q98e = open(os.path.join(WURZEL, 'scbp', 'hauptfenster.py'),
                 encoding='utf-8').read()
    pruefe('VORBAU_AN = False' in _q98e,
           'die Abschaltung steht als eigene Konstante da')
    _code98e = chr(10).join(_z for _z in _q98e.split(chr(10))
                            if not _z.strip().startswith('#'))
    pruefe('if VORBAU_AN:' in _code98e,
           'und der Start haengt wirklich daran')
    _start98e = _code98e.index('after(400, self._seiten_vorbauen)')
    _schalter98e = _code98e.index('if VORBAU_AN:')
    pruefe(_schalter98e < _start98e,
           'der Schalter steht VOR dem Start, nicht daneben')


    # 99. Man sieht, welcher Bauplan in der Herstellung aufgeklappt ist
    #
    # ⚠⚠ **Am 31.08.2026 gemeldet:** „nicht klar genug, welcher Bauplan bei
    # Herstellung ausgewaehlt ist, steht auch nirgends." Die aufgeklappte
    # Zeile sah aus wie jede andere, und der Rezeptkasten darunter ist lang —
    # Zutaten, Herstellzeit, Regler, Werte. Wer bis dorthin gerollt hatte,
    # wusste nicht mehr, wovon er die Zutaten liest.
    #
    # ⚠ Zwei Antworten, beide noetig: Die Zeile hebt sich ab UND der Name
    # steht noch einmal ueber dem Rezept. Nur das Hervorheben haette nichts
    # genuetzt, sobald die Zeile aus dem Bild gerollt ist.
    print()
    print('99. Man sieht, welcher Bauplan in der Herstellung aufgeklappt ist')
    _q99 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                encoding='utf-8').read()
    _hz99 = _q99.split('def _herstellung_zeile')[1].split(chr(10) + 'def ')[0]
    _code99 = chr(10).join(_z for _z in _hz99.split(chr(10))
                           if not _z.strip().startswith('#'))

    pruefe("fg=ACCENT if _offen else FG" in _code99,
           'die aufgeklappte Zeile ist farblich abgesetzt')
    pruefe('f_fett if _offen else' in _code99,
           'und fett — Farbe allein reicht nicht, wenn jemand schlecht sieht')

    # ⚠ Der Name im Kasten: geprueft wird, dass er VOR den Zutaten steht.
    # Dahinter waere er wertlos — dann hat man ihn erst gefunden, wenn man
    # ihn nicht mehr braucht.
    _kopf99 = _code99.find("text=eintrag['name'], bg='#0c1017'")
    _zutat99 = _code99.find('rez = herst_modul.rezept')
    pruefe(_kopf99 > 0, 'der Name steht noch einmal ueber dem Rezept')
    pruefe(_kopf99 < _zutat99,
           'und zwar VOR den Zutaten, nicht darunter')
    pruefe("eintrag['hersteller']" in _code99[_kopf99:_zutat99],
           'mit dem Hersteller daneben — „5SA Rhada" allein sagt niemandem, '
           'worum es geht')


    # 100. Die Tastenkombination — und was sie NICHT tut
    #
    # ⚠⚠ **Nutzerwunsch vom 31.08.2026:** „Hotkey um die Bauplanliste
    # aufzurufen, da man in SC erst raustabben muss um dann mit der Maus das
    # Fenster zu suchen und zu klicken, da die Maus nicht sichtbar ist ueber
    # dem SC Fenster."
    #
    # ⚠⚠ **Es wird NICHT mitgehoert.** Angemeldet wird genau EINE Kombination
    # (`RegisterHotKey` unter Windows, `XGrabKey` unter X11); alles andere
    # sieht das Programm nie. Das ist der Unterschied zu einem Tastatur-Haken —
    # und der Grund, warum nur dieser Weg in Frage kam. Diese Pruefung haelt
    # das fest, damit es niemand spaeter „vereinfacht".
    print()
    print('100. Die Tastenkombination — und was sie NICHT tut')
    from scbp import hotkey as _hk100

    pruefe(_hk100.zerlegen('Strg+Alt+B') == ({'strg', 'alt'}, 'B'),
           'eine gewoehnliche Kombination wird verstanden')
    pruefe(_hk100.zerlegen('ctrl-shift-F5') == ({'strg', 'umschalt'}, 'F5'),
           'englische Namen, Bindestriche und F-Tasten auch')
    pruefe(_hk100.zerlegen('  alt + 7 ') == ({'alt'}, '7'),
           'Leerzeichen und Ziffern stoeren nicht')

    # ⭐ Der wichtigste Fall: OHNE Modifikator wird abgelehnt.
    # ⚠ Eine nackte Taste global zu belegen hiesse, sie im Spiel unbrauchbar
    # zu machen — und der Nutzer sucht den Grund dann ueberall, nur nicht hier.
    pruefe(_hk100.zerlegen('B') == (None, None),
           'eine nackte Taste wird ABGELEHNT')
    pruefe(_hk100.zerlegen('Strg+Alt') == (None, None),
           'Modifikatoren allein ergeben keine Kombination')
    pruefe(_hk100.zerlegen('Strg+A+B') == (None, None),
           'zwei gewoehnliche Tasten auch nicht')
    pruefe(_hk100.zerlegen('') == (None, None)
           and _hk100.zerlegen(None) == (None, None),
           'und leer erst recht nicht')
    pruefe(_hk100.zerlegen('Strg+F13') == (None, None),
           'F13 gibt es nicht — es wird nicht durchgereicht')

    # Der Standard muss selbst durch die eigene Pruefung kommen.
    pruefe(_hk100.zerlegen(_hk100.STANDARD)[0],
           'die voreingestellte Kombination ist gueltig (%s)' % _hk100.STANDARD)

    # ⚠⚠ **Ehrlich sagen, was nicht geht.** Unter Wayland kann kein Programm
    # eine systemweite Kombination selbst belegen. Ein leeres Feld, das nichts
    # bewirkt, waere schlimmer als gar keins.
    _geht100, _grund100 = _hk100.moeglich()
    pruefe(isinstance(_geht100, bool) and isinstance(_grund100, str),
           'das System sagt, ob es geht — und wenn nicht, warum (%s)'
           % (_grund100 or 'geht'))

    _sq100 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                  encoding='utf-8').read()
    pruefe("grund == 'wayland'" in _sq100 and "t('s_hk_wayland')" in _sq100,
           'unter Wayland steht die Erklaerung statt eines toten Feldes')

    # Die Wache selbst: anmelden, nachsehen, abmelden — ohne Tastendruck.
    _w100 = _hk100.Wache()
    pruefe(_w100.nachsehen() is False,
           'ohne Anmeldung meldet die Wache nichts')
    if _geht100:
        _ok100, _warum100 = _w100.anmelden(_hk100.STANDARD)
        pruefe(_ok100 or _warum100 == 'belegt',
               'die Kombination laesst sich anmelden — oder sie ist belegt, '
               'und das wird gesagt (%s)' % (_warum100 or 'angemeldet'))
        pruefe(_w100.nachsehen() is False,
               'und meldet nichts, solange niemand drueckt')
        _w100.abmelden()
        pruefe(_w100.helfer is None, 'abmelden raeumt auf')
    pruefe(_w100.anmelden('kein Hotkey')[1] == 'kombination',
           'Unsinn wird als Unsinn gemeldet, nicht als Systemfehler')

    # ⚠ Und der Weg nach vorn ist der, den es schon gab.
    _wq100 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'),
                  encoding='utf-8').read()
    _code100 = chr(10).join(_z for _z in _wq100.split(chr(10))
                            if not _z.strip().startswith('#'))
    pruefe('self.hervorholen()' in _code100.split('def _hotkey_nachsehen')[1]
           .split(chr(10) + '    def ')[0],
           'ein Druck holt das Fenster mit der Bauplan-Liste nach vorn')
    pruefe('_hotkey_nachsehen()' in _code100.split('def _poll_queue')[1]
           .split(chr(10) + '    def ')[0],
           'gefragt wird im Tk-Takt, im selben wie die uebrige Warteschlange')


    # ⭐⭐ Und der Fehler, der am 31.08.2026 gemeldet wurde: „bei mir geht
    # er nicht".
    #
    # ⚠⚠ **Angemeldet war alles richtig — der Druck kam nur nie an.**
    # `RegisterHotKey(None, ...)` liefert eine FADEN-Nachricht. Lief die im
    # Tk-Faden, raeumte Tk sie mit seiner eigenen Pumpe
    # (`PeekMessage(NULL, 0, 0, PM_REMOVE)`) weg, bevor der 300-ms-Takt
    # nachsah — eine Faden-Nachricht hat kein Fenster, also stellt
    # `DispatchMessage` sie niemandem zu, sie ist einfach fort. Gemessen:
    # ohne Tk 3 von 3 angekommen, mit laufendem Tk 0 von 3.
    _hq100 = open(os.path.join(WURZEL, 'scbp', 'hotkey.py'),
                  encoding='utf-8').read()
    _nachsehen100 = (_hq100.split('class _Windows')[1].split('def nachsehen')[1]
                     .split(chr(10) + chr(10) + chr(10))[0])
    pruefe('Message' not in _nachsehen100,
           'nachgesehen wird an einer Fahne, NICHT in der Nachrichtenschlange '
           'des Tk-Fadens — dort raeumt Tk vorher weg')
    pruefe('threading.Thread' in _hq100 and 'GetMessageW' in _hq100,
           'gewartet wird in einem eigenen Faden, auf seiner eigenen Schlange')

    # ⚠ Die Gegenprobe geht nur unter Windows — unter Linux gibt es weder
    # `RegisterHotKey` noch die Schlange, um die es hier geht. Der Weg dorthin
    # (X11) ist ein anderer und war nie betroffen.
    #
    # ⚠⚠ **Kein `mainloop()`, sondern `update()` im Takt.** Die erste Fassung
    # dieser Pruefung rief `mainloop()` — und blieb auf dem Windows-Laeufer von
    # GitHub haengen, wo kein Mensch am Bildschirm sitzt (Bau-Lauf v3.8.1, nach
    # elf Minuten abgebrochen). `update()` pumpt dieselbe Nachrichtenschlange,
    # um die es hier geht, und kommt garantiert zurueck. Der ganze Selbsttest
    # macht es ueberall sonst genauso — diese Pruefung war die Ausnahme.
    if sys.platform.startswith('win'):
        import ctypes as _ct100
        import tkinter as _tkk100
        _w100b = _hk100.Wache()
        # ⚠ Bewusst NICHT die Standardkombination: Laeuft der Watcher gerade,
        # ist die belegt, und die Pruefung wuerde sich selbst ueberspringen.
        _ok100b, _warum100b = _w100b.anmelden('Strg+Alt+Umschalt+F9')
        try:
            if _ok100b:
                _wz100 = _tkk100.Tk()
                _wz100.withdraw()       # kein Fenster, kein Fokusklau
                _ct100.windll.user32.PostThreadMessageW(
                    _ct100.c_uint(_w100b.helfer._tid),
                    _ct100.c_uint(_hk100.WM_HOTKEY),
                    _ct100.c_size_t(_hk100.KENNUNG), _ct100.c_ssize_t(0))
                _an100 = 0
                for _ in range(20):     # hoechstens 2 Sekunden, dann ist es weg
                    _wz100.update()     # <- genau hier raeumte Tk frueher ab
                    if _w100b.nachsehen():
                        _an100 += 1
                        break
                    time.sleep(0.1)
                _wz100.destroy()
                pruefe(_an100 == 1,
                       'ein Druck kommt an, WAEHREND Tk seine Schlange pumpt '
                       '(%d von 1) — genau das ging bis v3.8.0 verloren'
                       % _an100)
            else:
                pruefe(_warum100b == 'belegt',
                       'die Gegenprobe entfaellt, weil die Testkombination '
                       'belegt ist — und das wird gesagt, statt still zu '
                       'schweigen')
        finally:
            # ⚠ Immer abmelden: Bleibt der Faden stehen, haengt am Ende der
            # Prozess statt der Pruefung.
            _w100b.abmelden()



    # 101. Das Overlay laesst sich in eine Ecke legen — und klappt schmal ein
    #
    # ⚠⚠ **Am 31.08.2026 gemeldet:** „stoert mich irgendwie, dass es nicht
    # komplett in der Ecke sitzt … der Balken sitzt ja aber mittig vom Watcher
    # Fenster."
    #
    # Zwei Ursachen:
    #
    # 1. Beim Einklappen schrumpfte nur die HOEHE. Der Streifen blieb so breit
    #    wie das offene Fenster — bei 1160 Pixeln ein Balken quer ueber den
    #    halben Bildschirm, den man in keine Ecke bekommt.
    # 2. Ziehen geht im Pop-up-Betrieb ueberhaupt nicht: Dort ist das Overlay
    #    durchklickbar, damit es im Kampf nicht stoert — und was Mausklicks
    #    durchreicht, laesst sich nicht anfassen. Diese Nutzer konnten das
    #    Overlay also GAR NICHT positionieren.
    #
    # ⚠⚠ **Gerechnet, nicht gemessen.** Pruefung 87 steht als Mahnmal daneben:
    # Versteckte Fenster liefern auf den Bau-Rechnern keine Masse, und eine
    # Pixel-Pruefung waere dort gruen, ohne etwas geprueft zu haben. Also wird
    # hier die Rechnung geprueft und der Bildschirm vorgegaukelt.
    print()
    print('101. Das Overlay laesst sich in eine Ecke legen')
    import sc_bp_watcher as _w101
    from scbp import bildschirm as _bs101, pfade as _pf101

    class _Wurzel101:
        def winfo_x(self): return 500
        def winfo_y(self): return 400

    class _Ov101:
        ECKEN = _w101.Overlay.ECKEN
        root = _Wurzel101()
        _klapp_ecke = _w101.Overlay._klapp_ecke

    # ⚠⚠ **`arbeitsflaeche` vortaeuschen, nicht `schirm_fuer`.** Seit
    # v3.9.4 rechnet `_klapp_ecke` mit der nutzbaren Flaeche (ohne
    # Taskleiste) statt mit der ganzen. Wer hier weiter `schirm_fuer`
    # ersetzt, taeuscht eine Funktion vor, die gar nicht mehr gefragt wird —
    # und bekommt die echten Werte des Rechners. Auf dem Windows-Bauserver
    # sind das andere als 1920x1080, und drei Pruefungen fielen um
    # (gemessen 02.09.2026 beim Bau von v3.9.4). Der Fehlschlag war richtig:
    # Der Aufrufweg hatte sich geaendert.
    _echt101 = _bs101.arbeitsflaeche
    _bs101.arbeitsflaeche = lambda *_a, **_k: (0, 0, 1920, 1080)
    _o101 = _Ov101()
    try:
        _pf101.einstellung_setzen('overlay_ecke', 'frei')
        pruefe(_o101._klapp_ecke(300, 30) == (500, 400),
               '„frei" laesst das Fenster stehen, wo es steht')

        # ⭐ Die vier Ecken, mit 8 px Rand.
        for _kennung101, _soll101 in (('oben-links',   (8, 8)),
                                      ('oben-rechts',  (1920 - 300 - 8, 8)),
                                      ('unten-links',  (8, 1080 - 30 - 8)),
                                      ('unten-rechts', (1920 - 300 - 8,
                                                        1080 - 30 - 8))):
            _pf101.einstellung_setzen('overlay_ecke', _kennung101)
            _ist101 = _o101._klapp_ecke(300, 30)
            pruefe(_ist101 == _soll101,
                   '%s sitzt richtig (%s)' % (_kennung101, _ist101))

        # ⚠ Auf DEM Schirm, auf dem es steht — nicht auf dem ersten. Bei drei
        # Monitoren nebeneinander waere „oben rechts" sonst immer der linke.
        _bs101.arbeitsflaeche = lambda *_a, **_k: (1920, 0, 2560, 1440)
        _pf101.einstellung_setzen('overlay_ecke', 'oben-rechts')
        pruefe(_o101._klapp_ecke(300, 30) == (1920 + 2560 - 300 - 8, 8),
               'und auf dem zweiten Bildschirm genauso')

        # ⚠⚠ Unsinn in der Einstellung darf nichts verschieben — sonst landet
        # das Overlay nach einem Tippfehler in der Datei im Nirgendwo.
        _pf101.einstellung_setzen('overlay_ecke', 'schraeg-hinten')
        pruefe(_o101._klapp_ecke(300, 30) == (500, 400),
               'eine unbekannte Ecke laesst alles, wie es ist')
    finally:
        _bs101.arbeitsflaeche = _echt101
        _pf101.einstellung_setzen('overlay_ecke', 'frei')

    # Und das Einklappen nimmt die Breite mit.
    _q101 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'),
                 encoding='utf-8').read()
    _code101 = chr(10).join(_z for _z in _q101.split(chr(10))
                            if not _z.strip().startswith('#'))
    _kz101 = _code101.split('def klappzustand_setzen')[1].split(chr(10) + '    def ')[0]
    # ⚠ Nur die geometry-Zeile ansehen. `winfo_width()` kommt weiter vor —
    # dort wird die OFFENE Breite gemerkt, und das ist richtig so.
    _geo101 = [_z for _z in _kz101.split(chr(10)) if 'root.geometry' in _z]
    pruefe(_geo101 and 'breite' in _geo101[0] and 'winfo_width' not in _geo101[0],
           'eingeklappt wird die gemessene Breite gesetzt, nicht die alte '
           '(%s)' % (_geo101[0].strip() if _geo101 else 'keine Zeile'))
    pruefe('self.breite_offen' in _kz101,
           'und die offene Breite wird gemerkt')
    pruefe('_klapp_ecke' in _kz101,
           'beim Klappen wird die Ecke angewandt')

    try:
        _ov92.root.destroy()
        _wz92.destroy()
    except Exception:
        pass

    try:
        _ov91.root.destroy()
        _wz91.destroy()
    except Exception:
        pass

    # -----------------------------------------------------------------------
    print()
    print('102. Unser Block sitzt VOR einem fremden Anhang')
    # ⚠ Warum das geprueft wird: Werkzeuge, die ihren eigenen Anhang abraeumen
    # (Smart Citizen), schneiden "ab dem eigenen Marker bis zum Ende". Alles
    # davor ueberlebt, alles dahinter nicht. Gemessen am 02.09.2026 verlor
    # unser Block dadurch 398 von 398 gemeinsamen Eintraegen.
    from scbp import injektion as _in102

    # ⚠ Die **echte** Ueberschrift nehmen, nicht eine ausgedachte: Der Notnagel
    # beim Zuruecksetzen erkennt den eigenen Block an genau diesen Woertern
    # (`_UEBERSCHRIFTEN`). Mit einer erfundenen Ueberschrift prueft der Test
    # das Zuruecksetzen gar nicht — beim ersten Anlauf am 02.09.2026 genau so
    # passiert, und die Pruefung meldete einen Fehler, der keiner war.
    # ⚠ Das Kaestchen muss mit. Ein Block mit unserer Ueberschrift, aber **ohne**
    # Kaestchen, gilt absichtlich als fremder Block des SC Deutsch Launchers und
    # bleibt beim Zuruecksetzen stehen (siehe `_saeubern`). Ohne das Kaestchen
    # prueft der Test also das Gegenteil dessen, was er soll — am 02.09.2026
    # genau so passiert, zweimal hintereinander.
    _UNSER = ('\\n\\n' + ('-' * 57) + '\\n\\n<EM4>%s</EM4>\\n\\n%s Atzkav'
              % (_in102._UEBERSCHRIFTEN[0], _in102.KASTEN_HAB))

    # -- Richtung 1: erkennen, was erkannt werden MUSS ----------------------
    for _marke102 in ('\\n\\n--- STATS ---', '\\n\\n<EM3>MISSION DETAILS</EM3>',
                      '\\n\\n<EM3>STATS</EM3>', '\\n\\n== Stats ==',
                      '\\n\\n<EM3>== Mission Details ==</EM3>'):
        pruefe(bool(_in102.FREMDER_ANHANG.search('CIG-Text' + _marke102
                                                 + '\\nWert: 5')),
               'ein fremder Anhang %s wird erkannt' % _marke102.strip())

    # -- Richtung 2: NICHT erkennen, was uns gehoert ------------------------
    pruefe(not _in102.FREMDER_ANHANG.search('CIG-Text' + _UNSER),
           'die eigene Linie gilt nicht als fremder Anhang')
    pruefe(not _in102.FREMDER_ANHANG.search(
        'Ein Satz.\\n\\nEin zweiter Absatz ohne jede Ueberschrift.'),
        'ein gewoehnlicher Absatz von CIG gilt nicht als fremder Anhang')

    # -- Die Wirkung: wo landet der Block? ----------------------------------
    _mit102 = 'CIG-Text\\n\\n--- STATS ---\\nDPS: 157'
    _erg102 = _in102._anhaengen(_mit102, _UNSER)
    pruefe(_erg102.index('AUFTRAG') < _erg102.index('--- STATS ---'),
           'bei fremdem Anhang steht unser Block davor')
    pruefe('DPS: 157' in _erg102,
           'und der fremde Anhang bleibt vollstaendig erhalten')

    _ohne102 = _in102._anhaengen('Nur CIG-Text', _UNSER)
    pruefe(_ohne102 == 'Nur CIG-Text' + _UNSER,
           'ohne fremden Anhang bleibt es schlichtes Anhaengen')

    # -- Und das Zuruecksetzen darf den Fremdtext nicht mitnehmen -----------
    # ⚠ Der Notnagel schnitt frueher "ab unserer Linie bis zum Ende". Seit
    # unser Block davor sitzt, laege der fremde Text mit im Schnitt.
    _zurueck102 = _in102._saeubern(_erg102)
    pruefe('AUFTRAG' not in _zurueck102,
           'das Zuruecksetzen entfernt unseren Block')
    pruefe('DPS: 157' in _zurueck102,
           'aber laesst den fremden Anhang stehen')

    # -----------------------------------------------------------------------
    print()
    print('103. Kein Symbol wird erzeugt und dann nicht benutzt')
    # ⚠ Anlass (02.09.2026): In `symbole_bauen.py` stand seit langem
    # `'ziehgriff': 'grip'`. Die Bilder wurden brav in allen Groessen und
    # Farben erzeugt — eingebaut war das Symbol nie, der Ziehgriff blieb ein
    # Schriftzeichen. **Ein vorbereitetes Symbol sieht in der Zuordnungstabelle
    # aus wie erledigt.** Genau das faellt sonst niemandem auf.
    #
    # Gesucht wird der Name als Zeichenkette im Quelltext — in einem Aufruf,
    # einem Woerterbuch oder einer Liste. Die Dateien, in denen die Namen
    # DEFINIERT werden, zaehlen dabei nicht als Verwendung.
    from scbp import zeichen as _z103

    # Bekannte Ausnahmen. ⚠ Diese Liste ist zum LEEREN da, nicht zum Wachsen:
    # Jeder Eintrag ist ein Symbol, das erzeugt wird, ohne dass es jemand
    # sieht. Wer eines ergaenzt, sollte den Grund danebenschreiben.
    _AUSNAHMEN103 = {
        # ⚠ **Bewusste Ausnahme, kein Versehen** (entschieden 02.09.2026).
        # Der Ko-fi-Knopf malt seine Tasse selbst (`kaffee_zeichen` in
        # `hauptfenster.py`), obwohl `coffee.svg` im Satz liegt und 24 fertige
        # Bilder daraus erzeugt werden.
        #
        # Grund: Direkt daneben sitzt der Discord-Knopf, und **Discord gibt es
        # bei Lucide nicht** — der Satz fuehrt grundsaetzlich keine
        # Markenlogos. Dieses Zeichen muss also gemalt bleiben. Eine gemalte
        # Tasse daneben ist stimmiger als ein Bildsymbol neben einem
        # gezeichneten Logo. Die beiden sind damit die EINZIGE Ausnahme von
        # der Regel „Symbole kommen aus dem Satz".
        'kaffee',
        # Gegenstueck zu 'aufklappen'/'einklappen', die beide benutzt werden.
        # Dieses dritte Motiv wurde nie gebraucht.
        'ausklappen',
        # Der alte Ziehgriff (Punktraster ohne Richtung). Seit 02.09.2026
        # ersetzt durch 'ziehen_ol/or/ul/ur'.
        'ziehgriff',
        # Angelegt, aber nie eingebaut — der Update-Knopf traegt Text.
        'herunterladen',
        # Der gelbe Zustand „vorlaeufig" ist mit v3.0.0-rc95 abgeschafft
        # worden: Ein Fund aus dem Log gilt seither sofort als sicher. Das
        # Symbol ist der Rest davon.
        'vorlaeufig',
    }

    _wurzel103 = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    _nicht103 = {
        os.path.join(_wurzel103, 'scbp', 'zeichen.py'),
        os.path.join(_wurzel103, 'tools', 'symbole_bauen.py'),
        # ⚠⚠ **Diese Datei selbst gehoert dazu** — sonst findet die Pruefung
        # ihre eigene Ausnahmeliste und haelt jedes Symbol darin fuer benutzt.
        # Genau so ist sie beim ersten Lauf hereingefallen. Nebenbei richtig:
        # Ein Symbol, das nur im Selbsttest vorkommt, sieht kein Nutzer.
        os.path.abspath(__file__),
    }
    _quellen103 = []
    for _ordner103, _unter103, _namen103 in os.walk(_wurzel103):
        _unter103[:] = [u for u in _unter103
                        if u not in ('.git', '__pycache__', 'assets', 'daten')]
        for _n103 in _namen103:
            if not _n103.endswith('.py'):
                continue
            # ⚠ `entwurf_*` ist per `.gitignore` ausgenommen und damit kein
            # Teil des Programms. Ein Symbol, das nur in einem Entwurf
            # vorkommt, sieht kein Nutzer — und ausgerechnet das Werkzeug,
            # das die toten Symbole SUCHT, nennt sie alle beim Namen.
            if _n103.startswith('entwurf_'):
                continue
            _p103 = os.path.join(_ordner103, _n103)
            if _p103 in _nicht103:
                continue
            try:
                with open(_p103, encoding='utf-8') as _f103:
                    _quellen103.append(_f103.read())
            except Exception:
                pass

    pruefe(len(_quellen103) > 5,
           'die Quelldateien wurden gefunden (%d)' % len(_quellen103))
    _tot103 = []
    for _name103 in _z103.ALLE:
        _m103 = re.compile(r'["\']%s["\']' % re.escape(_name103))
        if not any(_m103.search(q) for q in _quellen103):
            _tot103.append(_name103)
    _neu103 = [n for n in _tot103 if n not in _AUSNAHMEN103]
    pruefe(not _neu103,
           'kein neues unbenutztes Symbol (%s)'
           % (', '.join(_neu103) if _neu103 else 'keines'))
    # ⚠ Und andersherum: Steht eine Ausnahme wieder in Benutzung, gehoert sie
    # aus der Liste. Sonst waechst dort eine Sammlung von Unwahrheiten.
    _wieder103 = [n for n in _AUSNAHMEN103 if n not in _tot103]
    pruefe(not _wieder103,
           'keine Ausnahme ist ueberholt (%s)'
           % (', '.join(_wieder103) if _wieder103 else 'keine'))

    # -----------------------------------------------------------------------
    print()
    print('104. Kein Aufruf `self.xyz(...)`, den es gar nicht gibt')
    # ⚠⚠ **Der teuerste Fehler des 02.09.2026.** In `verhalten_anwenden` stand
    # `self._klappen(...)` — einen solchen Namen gab es nie, die Methode heisst
    # `klappzustand_setzen`. Der Aufruf starb bei JEDEM Programmstart mit
    # AttributeError, das `except Exception` darunter fing ihn und schrieb ihn
    # stumm ins Protokoll. Folge: Die gewaehlte Ecke wurde beim Start nie
    # angewandt. Aufgefallen ist es erst durch einen Fehlerbericht von aussen
    # (Haldjas, pr0) — mit einer ausgelieferten Version.
    #
    # Python selbst merkt so etwas nie: Attributzugriffe loesen sich zur
    # Laufzeit auf, und wo ein `except` drumherum steht, faellt gar nichts auf.
    import ast as _ast104

    def _bekannte_namen(klasse):
        """Alles, was die Klasse selbst mitbringt."""
        # ⚠ Von aussen angehaengte Namen (`w.faerben = ...` in `zeichen.py`)
        # kann diese Pruefung nicht sehen — sie stehen hier.
        namen = {'faerben', 'symbol_tauschen', 'groesse_nachziehen',
                 'zeichnen', 'setzen'}
        for k in _ast104.walk(klasse):
            if isinstance(k, (_ast104.FunctionDef, _ast104.AsyncFunctionDef)):
                namen.add(k.name)
            elif isinstance(k, _ast104.Assign):
                for ziel in _ast104.walk(k):
                    if (isinstance(ziel, _ast104.Attribute)
                            and isinstance(ziel.value, _ast104.Name)
                            and ziel.value.id == 'self'):
                        namen.add(ziel.attr)
            elif isinstance(k, _ast104.AnnAssign):
                if (isinstance(k.target, _ast104.Attribute)
                        and isinstance(k.target.value, _ast104.Name)
                        and k.target.value.id == 'self'):
                    namen.add(k.target.attr)
            # ⚠ `getattr(self, 'melder', None)` heisst: Der Name wird von
            # AUSSEN gesetzt und ist absichtlich optional. Wer so fragt, hat
            # den Fall bedacht — das ist ein Rueckruf, kein toter Aufruf.
            elif (isinstance(k, _ast104.Call)
                    and isinstance(k.func, _ast104.Name)
                    and k.func.id == 'getattr'
                    and len(k.args) >= 2
                    and isinstance(k.args[0], _ast104.Name)
                    and k.args[0].id == 'self'
                    and isinstance(k.args[1], _ast104.Constant)):
                namen.add(k.args[1].value)
        return namen

    _wurzel104 = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    _tot104 = []
    _dateien104 = 0
    for _ordner104, _unter104, _namen104 in os.walk(_wurzel104):
        _unter104[:] = [u for u in _unter104
                        if u not in ('.git', '__pycache__', 'assets', 'daten')]
        for _n104 in sorted(_namen104):
            if not _n104.endswith('.py') or _n104.startswith('entwurf_'):
                continue
            _p104 = os.path.join(_ordner104, _n104)
            try:
                with open(_p104, encoding='utf-8') as _f104:
                    _baum104 = _ast104.parse(_f104.read(), _p104)
            except (SyntaxError, OSError):
                continue
            _dateien104 += 1
            for _kl104 in _ast104.walk(_baum104):
                if not isinstance(_kl104, _ast104.ClassDef):
                    continue
                # ⚠ Erbt die Klasse von etwas anderem als `object`, koennen
                # Namen aus der Oberklasse kommen — dann ist nichts zu holen.
                if [b for b in _kl104.bases
                        if not (isinstance(b, _ast104.Name)
                                and b.id == 'object')]:
                    continue
                _hat104 = _bekannte_namen(_kl104)
                for _k104 in _ast104.walk(_kl104):
                    if not isinstance(_k104, _ast104.Call):
                        continue
                    _f = _k104.func
                    if (isinstance(_f, _ast104.Attribute)
                            and isinstance(_f.value, _ast104.Name)
                            and _f.value.id == 'self'
                            and _f.attr not in _hat104):
                        _tot104.append('%s:%d %s.%s'
                                       % (_n104, _f.lineno, _kl104.name,
                                          _f.attr))

    pruefe(_dateien104 > 5, 'die Quelldateien wurden gelesen (%d)' % _dateien104)
    pruefe(not _tot104,
           'kein Aufruf ins Leere (%s)'
           % ('; '.join(_tot104) if _tot104 else 'keiner'))

    # -----------------------------------------------------------------------
    print()
    print('105. Die Kopfzahl der Herstellung verschweigt die unklaren nicht')
    # ⚠⚠ **Der Anlass (03.09.2026).** Ueber der Liste stand „404 von 1597
    # herstellbar", der Bestand hatte zeitgleich 405 Bauplaene. Einer fehlte
    # scheinbar — tatsaechlich war er nur als *unklar* eingestuft: Traegt ein
    # Bauplan einen Namen, den mehrere Gegenstaende fuehren (Idris- und
    # Reclaimer-Kraftwerk, BroadSpec in zwei Groessen), zaehlt er bewusst
    # nicht als „sicher". Das ist richtig so.
    #
    # Falsch war die ANZEIGE: `zaehlung()` gibt `unklar` seit jeher zurueck,
    # die Kopfzeile warf den Wert weg. Oben stand also eine Zahl kleiner als
    # der eigene Bestand, und der Hinweis dazu (`s_he_unklar`) stand erst am
    # AUFGEKLAPPTEN Eintrag — dort findet ihn nur, wer schon weiss, wonach er
    # sucht.
    #
    # ⚠ Diese Pruefung legt sich ihre Daten SELBST hin (Regel aus Pruefung
    # 67): Die Rezeptdaten sind ein heruntergeladener Zwischenspeicher und
    # liegen im Wegwerf-Ordner nie. Eine Pruefung, die sich deshalb
    # ueberspringt, prueft nichts.
    from scbp import herstellung as _he105
    from scbp import sprache as _sp105

    _echt_alle105 = _he105.alle

    def _vorrat105():
        """Zwei Gegenstaende mit demselben Namen, dazu ein eindeutiger."""
        return [
            {'basis': 'Main Powerplant', 'name': 'Main Powerplant (Idris)',
             'hersteller': '', 'art': '', 'unterart': '', 'stufen': 1,
             'tag': 'idris_pp', 'tags': ['idris_pp'], 'entity': ''},
            {'basis': 'Main Powerplant', 'name': 'Main Powerplant (Reclaimer)',
             'hersteller': '', 'art': '', 'unterart': '', 'stufen': 1,
             'tag': 'recl_pp', 'tags': ['recl_pp'], 'entity': ''},
            {'basis': 'Testlampe', 'name': 'Testlampe',
             'hersteller': '', 'art': '', 'unterart': '', 'stufen': 1,
             'tag': 'lampe', 'tags': ['lampe'], 'entity': ''},
        ]

    try:
        _he105.alle = _vorrat105
        # Der Spieler hat BEIDE Bauplaene — den mehrdeutigen und den klaren.
        _bestand105 = {_he105._norm('Main Powerplant'),
                       _he105._norm('Testlampe')}
        _sicher105, _gesamt105, _unklar105 = _he105.zaehlung(_bestand105)
    finally:
        _he105.alle = _echt_alle105

    # ⚠⚠ **Erst pruefen, dass der unklare Fall ueberhaupt entsteht.** Ohne
    # diesen Waechter wuerde die Pruefung auch dann gruen, wenn die
    # Mehrdeutigkeits-Erkennung ausfaellt und alles als „sicher" durchgeht.
    pruefe(_sicher105 == 1,
           'der eindeutige Bauplan zaehlt als sicher (%d)' % _sicher105)

    # ⚠⚠ **EIN Bauplan, nicht zwei Eintraege.** Der Spieler hat einen einzigen
    # Bauplan „Main Powerplant"; dass zwei Gegenstaende so heissen, ist SEIN
    # Problem nicht. Bis zum 03.09.2026 zaehlte `zaehlung()` hier die
    # Listeneintraege — an den echten Daten kamen so `404 · 2 unklar` bei 405
    # Bauplaenen heraus, und die Rechnung ging wieder nicht auf. Genau der
    # Fehler, den der Zusatz eigentlich beheben sollte, nur eine Stelle
    # weiter.
    pruefe(_unklar105 == 1,
           'ein mehrdeutiger Bauplan zaehlt EINMAL, nicht je Eintrag (%d)'
           % _unklar105)

    # ⭐ Die Probe, auf die es ankommt: Kopfzahl plus unklare ergibt genau
    # den eigenen Bestand. Solange das stimmt, bleibt keine Luecke offen.
    pruefe(_sicher105 + _unklar105 == len(_bestand105),
           'sicher + unklar = Bestand (%d + %d = %d)'
           % (_sicher105, _unklar105, len(_bestand105)))
    pruefe(_sicher105 < len(_bestand105),
           'und die Kopfzahl allein ist kleiner als der Bestand (%d < %d) — '
           'genau darum braucht sie den Zusatz'
           % (_sicher105, len(_bestand105)))

    # Der Zusatz muss es in beiden Sprachen geben …
    _txt105 = _sp105.TEXTE.get('s_he_dazu_unklar') or ('', '')
    pruefe(all(_txt105) and '%d' in _txt105[0] and '%d' in _txt105[1],
           'der Zusatztext steht deutsch und englisch bereit (%r)'
           % (_txt105[0],))

    # … und die Kopfzeile muss ihn auch benutzen, abhaengig von `unklar`.
    _q105 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                 encoding='utf-8').read()
    pruefe("t('s_he_dazu_unklar') % unklar" in _q105 and 'if unklar:' in _q105,
           'die Kopfzeile zeigt ihn, sobald es unklare gibt')

    # -----------------------------------------------------------------------
    print()
    print('106. Schritte einer Auftragsreihe finden ihre Bauplan-Angabe')
    # ⚠⚠ **Der Anlass (03.09.2026).** Das Overlay meldete „Willkommen im System
    # → 1 Bauplan, dir fehlt: Clearcut Module". Im aufgeschlagenen Auftrag
    # („Bergbau-Gelegenheit") stand davon nichts. Es fehlten KEINE Daten — die
    # Marke sass am Nachbarschluessel:
    #
    #     Battaglia_Story01_title  = Willkommen im System <EM4>[BP!]</EM4>
    #     Battaglia_Story01B_title = Bergbau-Gelegenheit      ← das sieht man
    #
    # Die Quelle kennt nur den Schluessel der REIHE; im Spiel sieht man den
    # SCHRITT. `_reihen_stamm` schlaegt die Bruecke.
    #
    # ⚠⚠ **Und die Gegenrichtung ist der teurere Teil der Pruefung.** Eine
    # erste Fassung erlaubte auch einen Unterstrich im Kuerzel und traf damit
    # `headhunters_defend_xt_h` und `…_m` — das sind Schwierigkeitsstufen,
    # keine Reihenschritte, und sie geben andere Bauplaene. Gefunden wurde das
    # nur, weil die Wirkung VOR dem scharfen Lauf an den echten Daten
    # gemessen wurde. Diese Faelle stehen hier, damit die Grenze nicht wieder
    # aufweicht.
    from scbp import injektion as _inj106

    # Die bekannten Hauptauftraege, wie sie `einspielen_scdl` aufbaut.
    _bekannt106 = {
        'battaglia_story01', 'battaglia_story02', 'battaglia_story03',
        'headhunters_defend_xt', 'headhunters_defend_xt_vh',
        'covalex_haulcargo_atob',
    }

    # (Stamm, erwarteter Hauptauftrag oder None, Beschreibung)
    FAELLE106 = [
        ('battaglia_story01b', 'battaglia_story01',
         'DER ANLASS: Schritt B gehoert zur Reihe'),
        ('battaglia_story01c', 'battaglia_story01', 'Schritt C ebenso'),
        ('battaglia_story02b', 'battaglia_story02', 'andere Reihe, Schritt B'),
        ('battaglia_story03b', 'battaglia_story03', 'dritte Reihe'),

        # --- Die Grenze: was NICHT zusammengehoert -----------------------
        ('headhunters_defend_xt_h', None,
         'DIE FEHLZUORDNUNG: _h ist eine Schwierigkeitsstufe'),
        ('headhunters_defend_xt_m', None, 'dasselbe fuer _m'),
        ('headhunters_defend_xt_vh', None,
         'und _vh hat ohnehin eigene Daten'),
        ('battaglia_story01_zusatzauftrag', None,
         'ein langer Rest ist ein eigener Auftrag'),
        ('battaglia_story01', None,
         'wer selbst bekannt ist, braucht keine Bruecke'),
        ('voelligandererauftrag', None, 'ohne gemeinsamen Anfang: nichts'),
        ('', None, 'leerer Stamm faellt nicht auf die Nase'),
    ]

    for _stamm106, _soll106, _was106 in FAELLE106:
        _ist106 = _inj106._reihen_stamm(_stamm106, _bekannt106)
        pruefe(_ist106 == _soll106,
               '%s (%r -> %r)' % (_was106, _stamm106, _ist106))

    # ⭐ Der laengste passende Stamm gewinnt — sonst landet ein Schritt bei der
    # falschen Reihe, sobald es `…story0` und `…story01` nebeneinander gibt.
    pruefe(_inj106._reihen_stamm('battaglia_story01b',
                                 {'battaglia_story0', 'battaglia_story01'})
           == 'battaglia_story01',
           'der laengste passende Stamm gewinnt')

    # Und der Weg von der Funktion in die Injektion muss auch gegangen werden:
    # eine Funktion, die niemand aufruft, behebt nichts.
    _q106 = open(os.path.join(WURZEL, 'scbp', 'injektion.py'),
                 encoding='utf-8').read()
    pruefe('_reihen_stamm(_stamm(schluessel), titel_stamm_an)' in _q106,
           'die Titel benutzen die Reihen-Zuordnung')
    pruefe('_reihen_stamm(_stamm(schluessel), stamm_an)' in _q106,
           'die Beschreibungen ebenso — sonst Marke ohne Liste darunter')
    pruefe('titel_stamm_an[stamm] = zusatz' in _q106,
           'und die Stamm-Tabelle fuer Titel wird ueberhaupt gefuellt')

    # -----------------------------------------------------------------------
    print()
    print('107. Wer das Overlay verschiebt, nimmt Schloss UND Streifen mit')
    # ⚠⚠ **Der Anlass (Haldjas, pr0, 02.09.2026).** „Overlay war auf links
    # unten eingestellt, balken war rechts unten und hat den watcher aber
    # links unten geoeffnet."
    #
    # Das Overlay hat DREI eigene Fenster: die Hauptflaeche, das Schloss und
    # den Anfasser-Streifen. Die beiden letzten folgen nicht von allein — jede
    # Stelle, die das Overlay bewegt, muss sie mitnehmen.
    #
    # Es gab zwei solche Stellen, und nur eine tat es vollstaendig:
    #   `ecke_anwenden()`        Eckenwechsel auf der Einstellungsseite  ✓
    #   `klappzustand_setzen()`  beim PROGRAMMSTART                      ✗
    #
    # Der Streifen blieb dort auf der gespeicherten alten Lage stehen. Nicht
    # reproduzierbar war es, weil der Fall nur beim ersten Start nach einem
    # Eckenwechsel eintritt und sich danach selbst wegraeumt.
    #
    # ⚠ Diese Pruefung liest den Quelltext, statt ein Overlay zu bauen: Der
    # Fehler ist ein FEHLENDER Aufruf, und genau den findet man am
    # zuverlaessigsten dort, wo er fehlen wuerde. Sie haelt die beiden Wege
    # gegeneinander — laeuft einer auseinander, faellt es auf.
    import ast as _ast107

    _quelle107 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'),
                      encoding='utf-8').read()
    _baum107 = _ast107.parse(_quelle107)

    def _rumpf107(name):
        """Der Quelltext einer Methode, ueber ihren Namen gefunden."""
        for knoten in _ast107.walk(_baum107):
            if (isinstance(knoten, _ast107.FunctionDef)
                    and knoten.name == name):
                return _ast107.get_source_segment(_quelle107, knoten) or ''
        return ''

    # Beide Wege, die das Overlay an eine Ecke setzen.
    for _weg107 in ('ecke_anwenden', 'klappzustand_setzen'):
        _text107 = _rumpf107(_weg107)
        pruefe(bool(_text107), '%s gefunden' % _weg107)
        pruefe('_klapp_ecke(' in _text107,
               '%s setzt das Fenster in die Ecke' % _weg107)
        pruefe('_schloss_nachziehen()' in _text107,
               '%s nimmt das Schloss mit' % _weg107)
        # ⭐ Der eigentliche Fund: DAS hier fehlte in `klappzustand_setzen`.
        pruefe('_anfasser_zeigen()' in _text107,
               '%s nimmt den Streifen mit' % _weg107)
        pruefe('self._letzte_lage =' in _text107,
               '%s zieht die gemerkte Lage nach' % _weg107)

    # ⚠ Und die gemerkte Lage muss aus den EBEN BERECHNETEN Werten kommen,
    # nicht aus `_current_geom()`: Tk uebernimmt eine frisch gesetzte
    # Geometrie erst im naechsten Durchlauf der Ereignisschleife, gefragt
    # kaeme also die alte zurueck — der Streifen saesse eine Ecke hinterher.
    for _weg107 in ('ecke_anwenden', 'klappzustand_setzen'):
        _text107 = _rumpf107(_weg107)
        _stelle107 = _text107.find('self._letzte_lage =')
        _zeile107 = _text107[_stelle107:_text107.find('\n', _stelle107)]
        pruefe('_current_geom' not in _zeile107 and '%d' in _zeile107,
               '%s merkt die berechnete Lage, nicht die abgefragte' % _weg107)

    # -----------------------------------------------------------------------
    print()
    print('108. Ja/Nein-Einstellungen killen keine Seite mehr')
    # ⚠⚠ **Der Fehler, der eine ganze Seite gefressen hat (03.09.2026).**
    # `_raffinerie_block` rief `pfade.einstellung('lager_raffinerie_offen')`.
    # Diese Funktion liefert einen PFAD und ruft dafuer `.strip()` auf dem
    # Wert. Sobald der Block einmal aufgeklappt war, stand `True` in der
    # Datei — `True.strip()` warf einen AttributeError, und der riss den
    # Aufbau der GANZEN Lager-Seite ab. Die Posten waren unversehrt, man sah
    # sie nur nicht mehr. Drin seit v3.4.1, aufgefallen erst in v3.9.7.
    #
    # Zwei Wachen, weil beide Ebenen falsch waren:
    from scbp import pfade as _pf108

    # 1) `einstellung()` darf an KEINEM Werttyp mehr sterben. Ein Pfad ist
    #    immer Text; alles andere ist ein Aufruf an der falschen Adresse.
    _echt108 = _pf108.einstellungen
    try:
        for _wert108 in (True, False, 0, 1, 42, 3.5, None, [], {}, ['a']):
            _pf108.einstellungen = lambda w=_wert108: {'probe': w}
            try:
                _ist108 = _pf108.einstellung('probe')
                _ok108 = _ist108 is None
            except Exception as _a108:
                _ok108 = False
                _ist108 = '%s: %s' % (type(_a108).__name__, _a108)
            pruefe(_ok108,
                   'einstellung() vertraegt %-14r -> %r'
                   % (_wert108, _ist108))
        # Und Text funktioniert weiterhin wie bisher.
        _pf108.einstellungen = lambda: {'probe': '  /tmp/pfad  '}
        pruefe(_pf108.einstellung('probe') == '/tmp/pfad',
               'ein echter Pfad kommt weiter sauber zurueck')
        _pf108.einstellungen = lambda: {'probe': '   '}
        pruefe(_pf108.einstellung('probe') is None,
               'nur Leerzeichen gelten als nicht gesetzt')
    finally:
        _pf108.einstellungen = _echt108

    # 2) ⭐ Und niemand darf `einstellung()` mehr fuer ein Ja/Nein benutzen.
    #    Diese Wache findet den naechsten Fall von selbst — sie sucht jeden
    #    Schluessel, der irgendwo mit einem bool GESETZT wird, und prueft, ob
    #    er anderswo als Pfad GELESEN wird.
    # ⚠⚠ **Über `ast`, nicht über Suchmuster.** Die erste Fassung suchte per
    # Regex — und meldete prompt einen Treffer in `pfade.py`, wo der
    # Schlüsselname nur im KOMMENTAR steht, der den Fehler erklärt. Dieselbe
    # Falle wie beim Riegel und bei `tee`: Ein Wort im Text ist kein Aufruf.
    # Der Syntaxbaum kennt den Unterschied.
    import ast as _ast108
    import glob as _glob108

    _bool_schluessel108 = set()
    _gelesen108 = {}
    for _datei108 in sorted(_glob108.glob(os.path.join(WURZEL, 'scbp', '*.py'))
                            + [os.path.join(WURZEL, 'sc_bp_watcher.py')]):
        try:
            _baum108 = _ast108.parse(open(_datei108, encoding='utf-8').read())
        except SyntaxError:
            continue
        for _k108 in _ast108.walk(_baum108):
            if not isinstance(_k108, _ast108.Call):
                continue
            _f108 = _k108.func
            _name108 = getattr(_f108, 'attr', None) or getattr(_f108, 'id', None)
            _erstes108 = _k108.args[0] if _k108.args else None
            if not (isinstance(_erstes108, _ast108.Constant)
                    and isinstance(_erstes108.value, str)):
                continue
            _schluessel108 = _erstes108.value
            if _name108 == 'einstellung_setzen' and len(_k108.args) > 1:
                _zweites108 = _k108.args[1]
                if (isinstance(_zweites108, _ast108.Constant)
                        and isinstance(_zweites108.value, bool)):
                    _bool_schluessel108.add(_schluessel108)
            elif _name108 == 'einstellung':
                _gelesen108.setdefault(_schluessel108,
                                       os.path.basename(_datei108))

    _falsch108 = sorted(set(_gelesen108) & _bool_schluessel108)
    pruefe(bool(_bool_schluessel108),
           'die Wache findet ueberhaupt Ja/Nein-Schluessel (%d)'
           % len(_bool_schluessel108))
    pruefe(not _falsch108,
           'kein Ja/Nein wird als Pfad gelesen (%s)'
           % (', '.join('%s in %s' % (k, _gelesen108[k]) for k in _falsch108)
              if _falsch108 else 'keiner'))

    print()
    print('109. Raffinerie-Methoden: das Raster steht')
    # ⚠ Die Werte sind **im Spiel abgelesen** und werden von Hand nachgetragen.
    # Genau dort passieren Zahlendreher — und ein falscher Rat faellt niemandem
    # auf, weil das Ergebnis plausibel aussieht. Diese Wache prueft nicht die
    # Zahlen selbst (das kann nur ein Mensch mit dem Spiel), sondern die
    # **Struktur**, die sie haben muessen.
    from scbp import raffinerie as _rf109

    pruefe(len(_rf109.STUFEN) == 9,
           'es sind neun Methoden (%d)' % len(_rf109.STUFEN))
    pruefe(set(_rf109.STUFEN) == set(_rf109.NAMEN),
           'zu jeder Methode gibt es einen Namen')

    # ⭐ **Der Kern:** Drei Ertragsstufen mal drei Kostenstufen, jede
    # Kombination genau einmal. Faellt das auseinander, wurde beim Nachtragen
    # etwas verwechselt — und die Empfehlung waere ab da geraten.
    _raster109 = _rf109.raster()
    pruefe(len(_raster109) == 9,
           'das Raster Ertrag x Kosten ist vollstaendig (%d von 9 Zellen)'
           % len(_raster109))

    # Jede Empfehlung muss auf der WICHTIGSTEN Achse das Beste liefern. Das ist
    # die eigentliche Zusage an den Spieler: Wer „Ertrag" sagt, bekommt nichts
    # mit weniger Ertrag, egal was sonst passiert.
    for _erste109 in _rf109.ACHSEN:
        for _zweite109 in _rf109.ACHSEN:
            if _erste109 == _zweite109:
                continue
            _best109, _alle109 = _rf109.empfehlung(_erste109, _zweite109)
            _max109 = max(_rf109.stufe(k, _erste109) for k in _rf109.STUFEN)
            pruefe(_rf109.stufe(_best109, _erste109) == _max109
                   and len(_alle109) == 9,
                   '%-6s dann %-6s -> %s' % (_erste109, _zweite109,
                                             _rf109.NAMEN[_best109]))

    # Ohne jede Angabe muss trotzdem etwas Sinnvolles herauskommen.
    _standard109 = _rf109.empfehlung()[0]
    pruefe(_rf109.stufe(_standard109, 'ertrag') == 3
           and _rf109.stufe(_standard109, 'kosten') == 3,
           'ohne Auswahl kommt die Methode mit viel Ertrag und wenig Kosten '
           '(%s)' % _rf109.NAMEN[_standard109])

    # Eine unterlegene Methode darf nicht selbst als besserer Ersatz auftreten
    # — sonst schickt das Werkzeug den Spieler im Kreis.
    _unter109 = _rf109.unterlegen()
    pruefe(not (set(_unter109) & set(_unter109.values())),
           'kein Ersatzvorschlag ist selbst unterlegen (%d unterlegene)'
           % len(_unter109))

    # ⚠ **Gegenprobe** — eine Pruefung, die nie anschlaegt, prueft nichts.
    # Hier wird absichtlich eine Stufe verbogen; das Raster MUSS brechen.
    _echt109 = dict(_rf109.STUFEN)
    try:
        _rf109.STUFEN['kazen'] = _rf109.STUFEN['cormack']
        pruefe(len(_rf109.raster()) < 9,
               'die Wache schlaegt bei einer verbogenen Stufe wirklich an')
    finally:
        _rf109.STUFEN.clear()
        _rf109.STUFEN.update(_echt109)
    pruefe(len(_rf109.raster()) == 9,
           'nach der Gegenprobe steht das Raster wieder')

    print()
    print('110. Die Ankuendigung nimmt den handgeschriebenen Vorspann')
    # ⚠⚠ **Bis zum 03.09.2026 ist das NIE passiert** — bei keiner einzigen
    # Version. Zwei Zeilen in `vorspann_aus` haben zusammengewirkt:
    #   * `> `-Zeilen wurden uebersprungen, und im CHANGELOG ist der Vorspann
    #     immer ein Blockzitat;
    #   * was uebrig blieb, fiel unter „faengt mit `*` an, also ein verirrter
    #     Aufzaehlungspunkt" — denn ein Vorspann beginnt mit `**fett**`.
    # Der Fehler war unsichtbar: Es kam ja ein Text heraus, nur eben die
    # Stichpunktliste statt der Ankuendigung.
    import importlib.util as _il110

    _spec110 = _il110.spec_from_file_location(
        'discord_post110', os.path.join(WURZEL, 'tools', 'discord_post.py'))
    _dp110 = _il110.module_from_spec(_spec110)
    _spec110.loader.exec_module(_dp110)

    # 1) Echter Block mit Blockzitat-Vorspann -> muss gefunden werden.
    _echt110 = ('> **Etwas Wichtiges.** Und ein zweiter Satz, der\n'
                '> ueber zwei Zeilen laeuft.\n'
                '\n'
                '### Neu\n'
                '\n'
                '- ein Punkt\n')
    _gefunden110 = _dp110.vorspann_aus(_echt110)
    pruefe(_gefunden110.startswith('**Etwas Wichtiges.**')
           and 'zweiter Satz' in _gefunden110,
           'ein Vorspann als Blockzitat wird gefunden')

    # 2) Ein Hinweiskasten davor darf ihn nicht verdecken.
    _kasten110 = ('> [!important]\n'
                  '> Betrifft alle Fassungen seit v1.0.\n'
                  '\n'
                  '> **Die Ankuendigung.** Steht hinter dem Kasten.\n'
                  '\n'
                  '### Neu\n')
    pruefe(_dp110.vorspann_aus(_kasten110).startswith('**Die Ankuendigung.**'),
           'ein Hinweiskasten davor verdeckt den Vorspann nicht')

    # 3) Gegenprobe: kein Vorspann da -> leer, damit die Stichpunkte greifen.
    pruefe(_dp110.vorspann_aus('### Behoben\n\n- nur ein Punkt\n') == '',
           'ohne Vorspann bleibt es leer (die Aufzaehlung springt ein)')
    pruefe(_dp110.vorspann_aus('- ein verirrter Punkt\n\n### Neu\n') == '',
           'ein verirrter Aufzaehlungspunkt gilt nicht als Vorspann')

    # 4) Und die echte, aktuelle Version — die Wache soll am scharfen Fall
    #    haengen, nicht nur an Kunstbeispielen.
    sys.path.insert(0, os.path.join(WURZEL, '.github', 'scripts'))
    import release_text as _rt110

    # Die Version aus der Quelldatei lesen statt sie zu importieren — der
    # Import zoege die ganze Oberflaeche nach.
    _quelle110 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'),
                      encoding='utf-8').read()
    _treffer110 = re.search(r"__version__\s*=\s*'([^']+)'", _quelle110)
    _aktuell110 = 'v' + (_treffer110.group(1) if _treffer110 else '')
    for _sprache110, _datei110 in _rt110.DATEIEN.items():
        _block110 = _rt110.abschnitt(_datei110, _aktuell110)
        pruefe(bool(_dp110.vorspann_aus(_block110)),
               '%s hat einen Vorspann in %s' % (_aktuell110, _sprache110))

    print()
    print('111. Wird LIVE in HOTFIX umbenannt, faellt das auf')
    # ⚠⚠ **Gemeldet von Haldjas am 03.09.2026.** Kommt eine ausgebesserte
    # Fassung neben LIVE auf denselben Server, laedt kaum jemand 100 GB neu —
    # man benennt den LIVE-Ordner in HOTFIX um, damit der Launcher nur die
    # Unterschiede holt. Der eingetragene Spielordner ist damit weg.
    #
    # In seinem Bericht stand `spiel_ordner=…\StarCitizen\LIVE`, dazu „Spiel
    # nicht gefunden", keine Game.log und 0 gelesene Protokolle — der Watcher
    # stand ohne Erklaerung da, obwohl in den Einstellungen ein Pfad steht.
    # Zwei Ursachen, beide hier abgedeckt:
    #   a) `HOTFIX` fehlte in `KANAELE`.
    #   b) Gesucht wurde **im** eingetragenen Ordner und darunter, nie
    #      **daneben** — wer sein Spiel nicht am Standardort hat, fand seinen
    #      Nachbarkanal auch mit (a) nicht.
    import shutil as _sh111
    from scbp import pfade as _pf111

    pruefe('HOTFIX' in _pf111.KANAELE,
           'HOTFIX steht in der Kanalliste')

    _wiese111 = tempfile.mkdtemp(prefix='sc-bp-kanal-')
    _altordner111 = _pf111.einstellung('spiel_ordner')
    # ⚠⚠ **Nur im Wegwerf-Ordner suchen — sonst zaehlt das echte Spiel mit.**
    # `kanaele_vorhanden()` geht die ueblichen Installationsorte ab. Auf einem
    # Rechner, auf dem Star Citizen liegt, findet es dort LIVE und PTU — und
    # diese Pruefung, die genau eine Liste erwartet, war damit **auf dem
    # Entwicklungsrechner nie gruen zu bekommen**. Im Bau-Lauf lief sie durch,
    # weil dort kein Spiel installiert ist: zwei verschiedene Wahrheiten ueber
    # dieselbe Frage, und die roten Zeilen standen wochenlang im Protokoll.
    #
    # Eine Pruefung darf sich nicht darauf verlassen, dass etwas NICHT auf dem
    # Rechner ist. Sie schneidet die Suche deshalb auf ihren eigenen Ordner zu —
    # so wie sie es mit `KANAELE` weiter unten ohnehin schon tut.
    _altbasen111 = _pf111._kanal_basen
    try:
        def _kanal111(name):
            _o = os.path.join(_wiese111, 'Roberts Space Industries',
                              'StarCitizen', name)
            os.makedirs(_o, exist_ok=True)
            open(os.path.join(_o, 'Game.log'), 'w', encoding='utf-8').write(
                'Added notification "Blueprint Received: Test: " [1]' + chr(10))
            return _o

        _live111 = _kanal111('LIVE')
        _pf111.einstellung_setzen('spiel_ordner', _live111)
        # Ab hier sucht `kanaele_vorhanden()` ausschliesslich hier.
        _basis111 = os.path.dirname(_live111)
        _pf111._kanal_basen = lambda: [_basis111]

        # a) Solange LIVE steht, darf nichts gemeldet werden — eine Wache, die
        #    im Normalfall anschlaegt, wird weggeklickt.
        pruefe(_pf111.kanal_abweichung() is None,
               'ein vorhandener Spielordner loest keine Frage aus')

        # b) Jetzt der echte Fall.
        _hotfix111 = os.path.join(os.path.dirname(_live111), 'HOTFIX')
        _sh111.move(_live111, _hotfix111)
        _lage111 = _pf111.kanal_abweichung()
        pruefe(_lage111 is not None,
               'LIVE ist weg, HOTFIX daneben -> es wird gefragt')
        if _lage111:
            _eingetragen111, _benutzt111, _kanaele111 = _lage111
            pruefe(_benutzt111 == _hotfix111,
                   'und zwar mit HOTFIX als Vorschlag')
            pruefe([k for k, _o, _s in _kanaele111] == ['HOTFIX'],
                   'die Auswahl zeigt genau die Kanaele, die es gibt')

        # c) ⚠ Gegenprobe mit dem ALTEN Stand: ohne HOTFIX in der Liste findet
        #    derselbe Aufbau gar nichts. Ohne diesen Lauf wuerde (b) nur zeigen,
        #    dass der neue Code laeuft — nicht, dass er etwas repariert.
        _merk111 = _pf111.KANAELE
        _pf111.KANAELE = tuple(k for k in _merk111 if k != 'HOTFIX')
        try:
            pruefe(not _pf111.kanaele_vorhanden(),
                   'Gegenprobe: ohne HOTFIX in der Liste war der Ordner unsichtbar')
        finally:
            _pf111.KANAELE = _merk111

        # d) Und der zuletzt bespielte Kanal gewinnt — gemessen, nicht geraten.
        _ptu111 = _kanal111('PTU')
        os.utime(os.path.join(_hotfix111, 'Game.log'), (2000000, 2000000))
        _neu111 = _pf111.kanaele_vorhanden()
        pruefe(_neu111 and _neu111[0][0] == 'PTU',
               'der zuletzt bespielte Kanal steht oben (PTU vor altem HOTFIX)')
    finally:
        _pf111._kanal_basen = _altbasen111
        _pf111.einstellung_setzen('spiel_ordner', _altordner111 or '')
        _sh111.rmtree(_wiese111, ignore_errors=True)

    print()
    print('112. Ein Erklaertext kostet beim Anhaengen nur EIN Binding')
    # ⚠⚠ **Warum das eine Pruefung wert ist.** Eine Zeile der Bauplan-Liste
    # haengt bis zu vier Erklaertexte an. Mit vier Bindings je Text waren das 16
    # pro Zeile — bei 40 Zeilen ueber 600, alle gesetzt, bevor das Fenster
    # steht. In Haldjas' Bericht vom 03.09.2026 kosteten die Zeilen 55 der 112
    # ms. Drei der vier Bindings raeumen einen Anzeige-Auftrag ab, den es vor
    # der ersten Mausberuehrung gar nicht geben kann — die kommen jetzt erst
    # beim ersten `<Enter>`.
    #
    # ⚠ Ohne Wache faellt ein Rueckbau auf „alle vier sofort" niemandem auf:
    # Das Programm funktioniert weiter, es wird nur wieder langsam. Genau die
    # Sorte Verschlechterung, die man erst Monate spaeter bemerkt.
    import tkinter as _tk112
    from scbp import hinweis as _hw112

    _root112 = _wurzel()
    try:
        _w112 = _tk112.Label(_root112, text='x')
        _hw112.anhaengen(_w112, 'Erklaertext')
        _vorher112 = set(_w112.bind())
        pruefe(_vorher112 == {'<Enter>'},
               'beim Anhaengen wird nur <Enter> gesetzt (%s)'
               % (', '.join(sorted(_vorher112)) or 'nichts'))

        # Gegenprobe: Die Abraeumer duerfen jetzt noch NICHT da sein — sonst
        # prueft der Satz oben nur, dass ueberhaupt etwas gebunden wurde.
        pruefe('<Leave>' not in _vorher112 and '<Destroy>' not in _vorher112,
               'die Abraeumer haengen vorher noch nicht dran')

        # Und die Abraeumer muessen beim ersten `<Enter>` nachgezogen werden —
        # sonst bliebe ein Erklaertext stehen, wenn die Maus weiterzieht.
        #
        # ⚠⚠ **Das wird am Quelltext geprueft, nicht am laufenden Fenster.**
        # Zwei Anlaeufe ueber `event_generate` sind daran gescheitert, dass Tk
        # einem nie sichtbar gemachten Widget kein `<Enter>` zustellt — auch
        # mit `when='now'` nicht. Die Pruefung meldete dann „haengt nur <Enter>
        # dran" und sah aus wie ein echter Fund, obwohl gar nichts ausgeloest
        # worden war. Ein Fenster dafuer wirklich sichtbar zu machen ist keine
        # Option: Auf dem Rechner des Autors reisst das den Tastaturfokus aus
        # dem laufenden Spiel.
        #
        # Statisch ist schwaecher — es beweist nicht, dass Tk die Bindings
        # tatsaechlich setzt. Es faengt aber genau den Rueckbau ab, um den es
        # geht: dass jemand die drei Abraeumer wieder nach oben zieht (dann
        # sind sie wieder sofort da) oder ganz streicht (dann fehlen sie).
        _quelle112 = open(os.path.join(WURZEL, 'scbp', 'hinweis.py'),
                          encoding='utf-8').read()
        _fn112 = _quelle112.split('def anhaengen')[1]
        _imenter112 = _fn112.split('def betreten')[1].split('def abbrechen')[0]
        for _ev112 in ('<Leave>', '<Button-1>', '<Destroy>'):
            pruefe("bind('%s'" % _ev112 in _imenter112,
                   '%s wird beim ersten <Enter> nachgezogen' % _ev112)
        # Gegenprobe: ausserhalb von `betreten` darf nur noch <Enter> stehen.
        _ausserhalb112 = (_fn112.split('def betreten')[0]
                          + _fn112.split('def abbrechen')[1])
        pruefe(_ausserhalb112.count('widget.bind') == 1,
               'beim Anhaengen selbst steht genau ein bind (%d)'
               % _ausserhalb112.count('widget.bind'))
    finally:
        try:
            _root112.destroy()
        except Exception:
            pass

    print()
    print('113. Das Auftrags-Protokoll zaehlt jeden Durchlauf genau einmal')
    # ⚠⚠ **Fuenf Anlaeufe, jeder an echten Sicherungen widerlegt.** Die Zahl in
    # Klammern ist, was „Retake Platforms From Nine Tails" jeweils faelschlich
    # zeigte — richtig waren fuenf Durchlaeufe:
    #
    #   1. Jede Logdatei fuer sich ausgewertet (32) — ein Auftrag ueber zwei
    #      Abende steht in zwei Dateien.
    #   2. Ueber die Meldungsnummer entdoppelt (35) — das Spiel schickt
    #      dieselbe Annahme zweimal in derselben Millisekunde mit
    #      verschiedenen Nummern.
    #   3. Marken nicht geputzt (3 + 2 getrennt) — derselbe Auftrag mal mit,
    #      mal ohne die eingespielte Bauplan-Marke im Titel.
    #   4. Zwischenziele als Auftraege gezaehlt (8 fuer „Obere Plattform
    #      erreichen") — das ist ein Ziel innerhalb des Auftrags.
    #   5. Nach Dateidatum sortiert — auf einer Sicherung tragen alle Kopien
    #      den Zeitpunkt des Kopierens, die Reihenfolge war zufaellig und ein
    #      Auftrag bekam das Ende eines fremden Durchlaufs.
    #
    # Jede dieser fuenf Fallen hat hier ihren eigenen Fall. Ohne sie faellt
    # niemandem auf, wenn eine davon zurueckkommt: Das Protokoll sieht immer
    # plausibel aus, es zaehlt nur falsch.
    from scbp import missionslog as _ml113

    _wiese113 = tempfile.mkdtemp(prefix='sc-bp-protokoll-')
    _altheim113 = os.environ.get('SC_BP_HOME')
    os.environ['SC_BP_HOME'] = os.path.join(_wiese113, 'ablage')
    os.makedirs(os.environ['SC_BP_HOME'], exist_ok=True)
    try:
        def _zeile113(zeit, text, nr):
            return ('<%sZ> [Notice] <SHUDEvent_OnNotification> '
                    'Added notification "%s: " [%d] [Team_Missions]'
                    % (zeit, text, nr))

        def _log113(name, zeilen):
            p = os.path.join(_wiese113, name)
            with open(p, 'w', encoding='utf-8') as f:
                f.write(chr(10).join(zeilen) + chr(10))
            return p

        # Sitzung 1: angenommen — mit der Doppelmeldung (Falle 2) und der
        # eigenen Bauplan-Marke im Titel (Falle 3).
        _log1 = _log113('a.log', [
            _zeile113('2026-08-01T10:00:00.100',
                      'Auftrag angenommen: Testauftrag <EM4>[BP!]</EM4>', 41),
            _zeile113('2026-08-01T10:00:00.100',
                      'Auftrag angenommen: Testauftrag <EM4>[BP!]</EM4>', 44),
            # Ein Zwischenziel — darf NIE als eigener Auftrag zaehlen (Falle 4).
            _zeile113('2026-08-01T10:05:00.000',
                      'Neuer Auftrag: Obere Plattform erreichen', 45),
        ])
        # Sitzung 2: Wiederaufnahme beim Einloggen (kein neuer Durchlauf),
        # danach abgeschlossen — und der Titel traegt jetzt die ANDERE
        # Markenform.
        _log2 = _log113('b.log', [
            _zeile113('2026-08-02T09:00:00.000',
                      'Auftrag angenommen: '
                      'Testauftrag[SCBPW] <EM4>[BP 0/3]</EM4>[/SCBPW]', 12),
            _zeile113('2026-08-02T11:30:00.000',
                      'Auftrag abgeschlossen: Testauftrag', 13),
        ])
        # Sitzung 3: derselbe Auftrag noch einmal — DAS ist ein zweiter
        # Durchlauf und muss zaehlen.
        _log3 = _log113('c.log', [
            _zeile113('2026-08-03T20:00:00.000',
                      'Auftrag angenommen: Testauftrag', 7),
        ])

        # ⚠ Falle 5: Alle drei Dateien bekommen DIESELBE Aenderungszeit — so
        # sieht eine Sicherung aus, die in einem Rutsch kopiert wurde. Wer
        # danach sortiert, bekommt eine zufaellige Reihenfolge.
        for _p113 in (_log1, _log2, _log3):
            os.utime(_p113, (1750000000, 1750000000))

        _alle113 = _ml113.aus_ordner(_wiese113)
        _namen113 = [e['name'] for e in _alle113]

        pruefe(all(n == 'Testauftrag' for n in _namen113),
               'die eigenen Marken sind aus jedem Titel heraus (%s)'
               % ', '.join(sorted(set(_namen113))))
        pruefe('Obere Plattform erreichen' not in _namen113,
               'ein Zwischenziel gilt nicht als Auftrag')
        pruefe(len(_alle113) == 2,
               'zwei Durchlaeufe, nicht mehr (%d) — Doppelmeldung und '
               'Wiederaufnahme zaehlen nicht mit' % len(_alle113))

        _zustaende113 = {e['wann'][:10]: e['zustand'] for e in _alle113}
        pruefe(_zustaende113.get('2026-08-01') == _ml113.ABGESCHLOSSEN,
               'der erste Durchlauf ist abgeschlossen')
        pruefe(_zustaende113.get('2026-08-03') == _ml113.LAEUFT,
               'der zweite laeuft noch')
        # Falle 5 schlaegt genau hier zu: Bei falscher Reihenfolge bekommt der
        # spaetere Durchlauf das Ende des frueheren.
        for _e113 in _alle113:
            if _e113.get('bis'):
                # ⚠ Kein Pfeil und keine Anfuehrungszeichen in der Begruendung:
                # Unter Windows laeuft die Ausgabe ueber cp1252 und bricht bei
                # jedem Zeichen ab, das dort fehlt — der Selbsttest stirbt dann
                # mitten im Lauf, statt einen Fehler zu melden.
                pruefe(_e113['bis'] > _e113['wann'],
                       'kein Ende steht vor seinem Anfang (%s bis %s)'
                       % (_e113['wann'][:16], _e113['bis'][:16]))

        # Gegenprobe: Ohne das Putzen der Marken zerfaellt derselbe Auftrag in
        # mehrere. Wenn die Wache das NICHT bemerkt, prueft sie nichts.
        _echt113 = _ml113.auftraege.sauber
        _ml113.auftraege.sauber = lambda t: (t or '').strip()
        try:
            _roh113 = _ml113.aus_ordner(_wiese113)
            pruefe(len({e['name'] for e in _roh113}) > 1,
                   'Gegenprobe: ohne Putzen zerfaellt der Auftrag in mehrere '
                   '(%d Namen)' % len({e['name'] for e in _roh113}))
        finally:
            _ml113.auftraege.sauber = _echt113

        # Fortschreiben: Das Protokoll muss stehen bleiben, wenn die Logs fort
        # sind — genau dafuer gibt es die Datei.
        _ml113.nachtragen(_wiese113)
        _gespeichert113 = _ml113.laden()
        pruefe(len(_gespeichert113) == 2,
               'das Protokoll steht in der eigenen Datei (%d)'
               % len(_gespeichert113))
        _ml113.nachtragen(_wiese113)
        pruefe(len(_ml113.laden()) == 2,
               'ein zweiter Lauf legt nichts doppelt an (%d)'
               % len(_ml113.laden()))
        _ml113.nachtragen(os.path.join(_wiese113, 'gibtsnicht'))
        pruefe(len(_ml113.laden()) == 2,
               'ohne Logs bleibt das Protokoll erhalten (%d)'
               % len(_ml113.laden()))

        # Ein abgeschlossener Auftrag darf nicht zurueckfallen, bloss weil in
        # einem noch vorhandenen Log nur sein Anfang steht.
        _zurueck113 = _ml113.zusammenfuehren(
            [{'name': 'X', 'wann': '2026-01-01T10:00',
              'zustand': _ml113.ABGESCHLOSSEN, 'bis': '2026-01-01T11:00'}],
            [{'name': 'X', 'wann': '2026-01-01T10:00',
              'zustand': _ml113.LAEUFT}])
        pruefe(_zurueck113[0]['zustand'] == _ml113.ABGESCHLOSSEN,
               'ein abgeschlossener Auftrag faellt nicht auf „laeuft" zurueck')

        pruefe(len(_ml113.suchen(_gespeichert113, 'testauf')) == 2
               and not _ml113.suchen(_gespeichert113, 'gibtsnicht'),
               'die Suche findet ueber den Auftragsnamen')

        # ⭐ Der Bauplan gehoert an den Auftrag, bei dem er herauskam.
        #
        # ⚠ **Die Belohnung faellt NACH dem Abgeben.** Gemessen am 29.08.2026:
        # Auftrag endete 17:42:00, der Bauplan kam 17:42:54. Wer nur waehrend
        # des Auftrags sucht, findet keinen einzigen — deshalb der Nachlauf.
        _log4 = _log113('d.log', [
            _zeile113('2026-09-01T10:00:00.000',
                      'Auftrag angenommen: Beuteauftrag', 1),
            _zeile113('2026-09-01T10:30:00.000',
                      'Auftrag abgeschlossen: Beuteauftrag', 2),
            # 54 Sekunden nach dem Ende — genau der gemessene Abstand.
            _zeile113('2026-09-01T10:30:54.000',
                      'Bauplan erhalten: Testhelm', 3),
            # Und einer, der VIEL zu spaet kommt: Der gehoert zu keinem Auftrag
            # mehr, sonst saugt ein alter Auftrag jeden spaeteren Fund an.
            _zeile113('2026-09-01T18:00:00.000',
                      'Bauplan erhalten: Spaetzuender', 4),
        ])
        os.utime(_log4, (1750000000, 1750000000))
        _beute113 = [e for e in _ml113.aus_dateien([_log4])
                     if e['name'] == 'Beuteauftrag']
        pruefe(len(_beute113) == 1 and _beute113[0].get('bauplaene') ==
               ['Testhelm'],
               'der Bauplan haengt am Auftrag, bei dem er herauskam (%s)'
               % (_beute113[0].get('bauplaene') if _beute113 else 'kein Auftrag'))
        pruefe(all('Spaetzuender' not in (e.get('bauplaene') or [])
                   for e in _ml113.aus_dateien([_log4])),
               'ein Fund lange nach dem Ende wird keinem Auftrag angehaengt')

        # ⭐⭐ Ein Auftrag gibt HOECHSTENS EINEN Bauplan her — Spielregel.
        # Ohne diese Grenze sammelt ein Auftrag, der faelschlich offen bleibt,
        # jeden spaeteren Fund ein: gemessen zwoelf Stueck an einem einzigen.
        _log5 = _log113('e.log', [
            _zeile113('2026-09-02T10:00:00.000',
                      'Auftrag angenommen: Sammelauftrag', 1),
            _zeile113('2026-09-02T10:05:00.000',
                      'Bauplan erhalten: Erster', 2),
            _zeile113('2026-09-02T10:06:00.000',
                      'Bauplan erhalten: Zweiter', 3),
        ])
        os.utime(_log5, (1750000100, 1750000100))
        _sammel113 = [e for e in _ml113.aus_dateien([_log5])
                      if e['name'] == 'Sammelauftrag']
        pruefe(_sammel113 and len(_sammel113[0].get('bauplaene') or []) == 1,
               'ein Auftrag bekommt hoechstens EINEN Bauplan (%d)'
               % len(_sammel113[0].get('bauplaene') or []) if _sammel113 else 0)

        # ⭐⭐ Ein Auftrag, den eine spaetere Sitzung nicht mehr nennt, ist
        # vorbei. Ohne das stand der aelteste seit ueber zwei Monaten auf
        # „laeuft" — und sammelte dabei fremde Bauplaene ein.
        _log6a = _log113('f1.log', [
            _zeile113('2026-09-03T10:00:00.000',
                      'Auftrag angenommen: Vergessener', 1),
        ])
        _log6b = _log113('f2.log', [
            # Naechste Sitzung: ein anderer Auftrag, der alte kommt nicht vor.
            _zeile113('2026-09-04T10:00:00.000',
                      'Auftrag angenommen: Ganz anderer', 1),
            _zeile113('2026-09-04T10:20:00.000',
                      'Bauplan erhalten: Beute des Neuen', 2),
        ])
        os.utime(_log6a, (1750000200, 1750000200))
        os.utime(_log6b, (1750000300, 1750000300))
        _spaet113 = {e['name']: e for e in _ml113.aus_dateien([_log6a, _log6b])}
        pruefe(_spaet113.get('Vergessener', {}).get('zustand')
               == _ml113.VERFALLEN,
               'ein Auftrag, den die naechste Sitzung nicht kennt, gilt als '
               'nicht mehr offen')
        pruefe(not (_spaet113.get('Vergessener', {}).get('bauplaene')),
               'und er saugt den Fund der naechsten Sitzung NICHT an')
        pruefe(_spaet113.get('Ganz anderer', {}).get('bauplaene')
               == ['Beute des Neuen'],
               'der Fund gehoert dem Auftrag, der wirklich lief')

        # ⚠ Gegenprobe: Eine Sitzung OHNE jede Auftragsmeldung beweist nichts.
        # Wer sich einloggt und herumfliegt, beendet damit keinen Auftrag.
        _log6c = _log113('f3.log', [
            _zeile113('2026-09-05T10:00:00.000', 'Nichts von Belang', 1),
        ])
        os.utime(_log6c, (1750000400, 1750000400))
        _stumm113 = {e['name']: e for e in _ml113.aus_dateien([_log6a, _log6c])}
        pruefe(_stumm113.get('Vergessener', {}).get('zustand') == _ml113.LAEUFT,
               'eine stumme Sitzung beendet keinen Auftrag')
    finally:
        if _altheim113 is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _altheim113
        shutil.rmtree(_wiese113, ignore_errors=True)

    # --------------------------------------------------------------------- 113b
    # ⚠⚠ Der Bestandsabgleich beim Start darf den Katalog nur EINMAL lesen.
    # Gemessen am 04.09.2026: Er las die 1-MB-Datei einmal je Bauplan und
    # brauchte bei 406 Stueck **3,6 Sekunden bei jedem Programmstart** — und
    # berichtigte dabei nichts. Nach dem Fix: 11 ms.
    #
    # ⚠ Gezaehlt statt gestoppt: Eine Zeitmessung im Selbsttest wackelt mit der
    # Maschine und schlaegt irgendwann grundlos an. Die Zahl der Dateizugriffe
    # ist dagegen eindeutig — und sie ist die Ursache, nicht das Symptom.
    print()
    print('113b. Der Bestandsabgleich liest den Katalog nur einmal')
    _bes113b = importlib.import_module('scbp.bestand')
    _kat113b = importlib.import_module('scbp.katalog')
    _zaehler113b = [0]
    _echt113b = _kat113b.laden

    def _gezaehlt113b(*a, **k):
        _zaehler113b[0] += 1
        return _echt113b(*a, **k)

    _kat113b.laden = _gezaehlt113b
    try:
        _daten113b = {'bauplaene': {
            'testhelm %d' % i: {'name': 'Testhelm %d' % i} for i in range(50)}}
        _bes113b.angleichen(_daten113b)
        pruefe(_zaehler113b[0] <= 1,
               'bei 50 Bauplaenen wird der Katalog %dx geladen (erlaubt: 1)'
               % _zaehler113b[0])
    finally:
        _kat113b.laden = _echt113b

    # ⚠⚠ Ein zweites Kuerzel-Muster: MrKraken StarStrings stellt Klasse, Groesse
    # und Grad VORAN statt sie anzuhaengen (`Ind/2/B Citadel`). Gemessen in der
    # ausgelieferten Datei: 465 Eintraege. Ohne Angleichung findet keiner davon
    # seinen Katalog-Eintrag — bei einem Melder vier von 26 Bauplaenen.
    print()
    print('113c. Das Kuerzel wird auch vorangestellt erkannt')
    _pf113c = importlib.import_module('scbp.pfade')
    for _roh113c, _soll113c in (
            ('Ind/2/B Citadel', 'citadel'),
            ('Sth/1/B Zephyr', 'zephyr'),
            ('Citadel (Ind/2/B)', 'citadel'),      # die alte Form bleibt
            # ⚠ Gegenproben: Was KEIN Kuerzel ist, bleibt stehen.
            ('Singe Cannon (S2)', 'singe cannon (s2)'),
            ('Ind/2/B', 'ind/2/b'),                # nur das Kuerzel, kein Name
    ):
        pruefe(_pf113c.namensform(_roh113c) == _soll113c,
               'Namensform: %r -> %r' % (_roh113c, _soll113c))

    # --------------------------------------------------------------------- 113d
    # ⚠⚠ Die Kaestchen in den Auftragstexten muessen dem Bestand folgen.
    # Bis zum 04.09.2026 wurde nur bei FREMDEN Aenderungen neu geschrieben
    # (neue Uebersetzung, neue Vertragsdaten, Auszeichnung weg) — der eigene
    # Bestand stand nicht auf der Liste. Damit hoerten die Kaestchen still auf
    # zu stimmen, sobald ein Bauplan dazukam: Gemeldet mit zwei Bauplaenen, die
    # seit zehn Tagen im Bestand lagen und im Spiel ungehakt blieben.
    print()
    print('113d. Die Kaestchen folgen dem eigenen Bestand')
    _inj113d = importlib.import_module('scbp.injektion')
    _a113d = {'bauplaene': {'marlin': {'name': 'Marlin'}}}
    _b113d = {'bauplaene': {'marlin': {'name': 'Marlin'},
                            'rn-7s': {'name': 'RN-7s'}}}
    # ⚠ Gleiche Anzahl, andere Namen — genau das erzeugt `bestand.angleichen`
    # beim Umbenennen. Eine Marke ueber die Anzahl wuerde das nicht bemerken.
    _c113d = {'bauplaene': {'marlin (ind/1/a)': {'name': 'Marlin (Ind/1/A)'}}}
    pruefe(_inj113d.bestand_marke(_a113d) == _inj113d.bestand_marke(_a113d),
           'derselbe Bestand ergibt dieselbe Marke')
    pruefe(_inj113d.bestand_marke(_a113d) != _inj113d.bestand_marke(_b113d),
           'ein neuer Bauplan aendert die Marke')
    pruefe(_inj113d.bestand_marke(_a113d) != _inj113d.bestand_marke(_c113d),
           'ein umbenannter Bauplan aendert die Marke (gleiche Anzahl)')

    # ⚠ Und die Kaestchen selbst: Was im Bestand liegt, wird angekreuzt.
    _block113d = ('# Baupläne:' + chr(92) + 'n'
                  '    - Marlin' + chr(92) + 'n'
                  '    - RN-7s' + chr(92) + 'n')
    _neu113d, _meine113d, _ges113d = _inj113d._kaestchen_setzen(
        _block113d, set(_b113d['bauplaene']))
    pruefe(_ges113d == 2 and _meine113d == 2,
           'beide Bauplaene werden als vorhanden erkannt (%d von %d)'
           % (_meine113d, _ges113d))
    _neu113e, _meine113e, _ges113e = _inj113d._kaestchen_setzen(
        _block113d, set(_a113d['bauplaene']))
    pruefe(_ges113e == 2 and _meine113e == 1,
           'was fehlt, bleibt ungehakt (%d von %d)' % (_meine113e, _ges113e))

    # --------------------------------------------------------------------- 113f
    # ⚠⚠ Ein Auftrag kann enden, ohne dass es eine Meldung dazu gibt.
    # Gemeldet am 04.09.2026: Ein Auftrag wurde angenommen und war vier
    # Sekunden spaeter weg, weil ein anderer Spieler schneller war. Dazu steht
    # im Protokoll nur `<EndMission> … CompletionType[Abandon]` — keine
    # „Auftrag abgeschlossen"-Meldung. Im Overlay standen daraufhin zwei
    # laufende Auftraege, im Spiel war einer.
    print()
    print('113f. Ein Auftrag endet auch ohne Meldung')
    _au113f = importlib.import_module('scbp.auftraege')
    _mid113f = 'e0b968d5-7575-48b6-9a50-5eaa1ad96745'
    _log113f = (
        '<2026-09-04T11:27:47.000Z> [Notice] <SHUDEvent_OnNotification> Added '
        'notification "Auftrag angenommen: Recover Vanduul Tech: " [4] to '
        'queue. New queue size: 1, MissionId: [%s], ObjectiveId: []\n'
        '<2026-09-04T11:27:51.878Z> [Notice] <EndMission> Ending mission for '
        'player. MissionId[%s] Player[Spieler] CompletionType[Abandon] '
        'Reason[Player left] [Team_MissionFeatures][Missions]\n'
        % (_mid113f, _mid113f))
    _offen113f, _ = _au113f.stand_aus_text(_log113f)
    pruefe(_offen113f == [],
           'ein stilles Ende raeumt den Auftrag weg (offen: %r)'
           % [_au113f.sauber(t) for t in _offen113f])

    # ⚠ Gegenprobe 1: OHNE das Ende muss er stehen bleiben — sonst raeumt die
    # Regel Auftraege weg, die wirklich laufen.
    _nur_an113f = _log113f.split(chr(10))[0] + chr(10)
    _offen113g, _ = _au113f.stand_aus_text(_nur_an113f)
    pruefe(len(_offen113g) == 1,
           'ohne das Ende bleibt der Auftrag offen (%d)' % len(_offen113g))

    # ⚠ Gegenprobe 2: Ein Ende mit FREMDER Kennung darf nichts wegnehmen.
    _fremd113f = _nur_an113f + (
        '<2026-09-04T11:30:00.000Z> [Notice] <EndMission> Ending mission for '
        'player. MissionId[11111111-2222-3333-4444-555555555555] '
        'CompletionType[Complete]\n')
    _offen113h, _ = _au113f.stand_aus_text(_fremd113f)
    pruefe(len(_offen113h) == 1,
           'ein fremdes Ende laesst den Auftrag in Ruhe (%d)'
           % len(_offen113h))

    # --------------------------------------------------------------------- 113e
    # ⚠⚠ Das Overlay muss seine Groesse ueber den Neustart behalten.
    # Bis zum 04.09.2026 schrumpfte es bei jedem Start auf die Mindestgroesse:
    # `klappzustand_setzen(False)` rechnet `max(hoehe_offen, Leiste + 120)`,
    # und `hoehe_offen` stand beim Start auf None. Aus 620x316 wurden 564x150.
    #
    # ⚠ Getroffen hat es nur, wer eine feste Ecke eingestellt hat — sonst wird
    # `klappzustand_setzen` beim Start gar nicht gerufen. Deshalb setzt diese
    # Pruefung die Ecke ausdruecklich.
    print()
    print('113e. Das Overlay behaelt seine Groesse ueber den Neustart')
    _alt113e = os.environ.get('SC_BP_HOME')
    _wiese113e = tempfile.mkdtemp(prefix='sc-bp-olgroesse-')
    try:
        os.environ['SC_BP_HOME'] = _wiese113e
        with open(os.path.join(_wiese113e, 'einstellungen.json'), 'w',
                  encoding='utf-8') as _f:
            json.dump({'eingeklappt': False, 'overlay_ecke': 'unten-links',
                       'overlay_modus': 'immer'}, _f)
        with open(os.path.join(_wiese113e, 'watcher.json'), 'w',
                  encoding='utf-8') as _f:
            json.dump({'geometry': '620x316+100+100'}, _f)

        _w113e = importlib.import_module('sc_bp_watcher')
        importlib.reload(_w113e)
        _ol113e = _w113e.Overlay()
        try:
            _ol113e.root.update_idletasks()
            pruefe(_ol113e.hoehe_offen == 316 and _ol113e.breite_offen == 620,
                   'die gemerkte Groesse steht beim Start bereit (%sx%s)'
                   % (_ol113e.breite_offen, _ol113e.hoehe_offen))
            # Genau der Aufruf, den der Start ausloest.
            _ol113e.klappzustand_setzen(False, merken=False)
            _ol113e.root.update_idletasks()
            _h113e = _ol113e.root.winfo_height()
            pruefe(_h113e >= 300,
                   'nach dem Aufklappen steht die alte Hoehe (%d px)' % _h113e)
        finally:
            _ol113e.root.destroy()
    finally:
        if _alt113e is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _alt113e
        shutil.rmtree(_wiese113e, ignore_errors=True)

    # --------------------------------------------------------------------- 114
    # Die Sicherung. ⚠⚠ Ein kaputtes Backup faellt erst im Ernstfall auf — dann,
    # wenn der alte Rechner schon neu aufgesetzt ist. Deshalb wird hier der
    # ganze Weg gegangen: schreiben, einspielen, Inhalte vergleichen.
    print()
    # --------------------------------------------------------------------- 115
    # ⚠⚠ Hier wird in die Datei geschrieben, an der die **komplette Steuerung**
    # des Spielers haengt. Ein Fehler kostet ihn seine Belegung — deshalb wird
    # jeder Schritt geprueft, und zwar an einer selbst hingelegten Datei, nie
    # an einer echten (siehe die Regel „Pruefungen nie in Livedaten").
    print()
    print('115. Belegen: nur das Gemeinte anfassen')
    _js115 = importlib.import_module('scbp.joysticks')
    _wiese115 = tempfile.mkdtemp(prefix='sc-bp-belegen-')
    try:
        _datei115 = os.path.join(_wiese115, 'actionmaps.xml')
        # Ein Minimalbeispiel im Code der Pruefung — keine Nutzerdatei, kein
        # Abruf. Es enthaelt genau die Faelle, die schiefgehen koennen:
        # dieselbe Aktion auf Stick UND Tastatur, und eine zweite Aktion.
        with open(_datei115, 'w', encoding='utf-8') as _f115:
            _f115.write(
                '<ActionMaps>' + chr(10) +
                ' <ActionProfiles version="1" profileName="default">' + chr(10) +
                '  <options type="joystick" instance="1" Product="Stick '
                '{AAAA1111-0000-0000-0000-504944564944}"/>' + chr(10) +
                '  <actionmap name="spaceship_general">' + chr(10) +
                '   <action name="v_eject">' + chr(10) +
                '    <rebind input="js1_button5"/>' + chr(10) +
                '    <rebind input="kb1_f5"/>' + chr(10) +
                '   </action>' + chr(10) +
                '   <action name="v_lights">' + chr(10) +
                '    <rebind input="js1_button9"/>' + chr(10) +
                '   </action>' + chr(10) +
                '  </actionmap>' + chr(10) +
                ' </ActionProfiles>' + chr(10) +
                '</ActionMaps>' + chr(10))

        _vorher115 = _js115.belegungen(datei=_datei115)
        pruefe(len(_vorher115.get('js1', [])) == 2
               and len(_vorher115.get('kb1', [])) == 1,
               'die Ausgangslage wird richtig gelesen')

        # 1. Eine Stick-Belegung aendern.
        _ok115, _m115, _ = _js115.belegen('v_eject', 'spaceship_general',
                                          'js1', 'button22',
                                          datei=_datei115)
        _nach115 = _js115.belegungen(datei=_datei115)
        _eject_js = [e for e in _nach115.get('js1', [])
                     if e['aktion'] == 'v_eject']
        pruefe(_ok115 and len(_eject_js) == 1
               and _eject_js[0]['eingabe'] == 'button22',
               'die Stick-Belegung wird gesetzt')
        # ⚠⚠ Der eigentliche Fallstrick: Die Tastenbelegung DERSELBEN Aktion
        # darf dabei nicht verschwinden. Ein Filter, der nur nach der Aktion
        # geht statt nach Aktion UND Geraeteart, loescht sie stillschweigend.
        _eject_kb = [e for e in _nach115.get('kb1', [])
                     if e['aktion'] == 'v_eject']
        pruefe(len(_eject_kb) == 1 and _eject_kb[0]['eingabe'] == 'f5',
               'die Tastenbelegung derselben Aktion bleibt stehen')
        pruefe(len([e for e in _nach115.get('js1', [])
                    if e['aktion'] == 'v_lights']) == 1,
               'eine andere Aktion bleibt unangetastet')

        # 2. Eine Sicherung muss entstanden sein — sonst gibt es keinen Rueckweg.
        pruefe(any(n.startswith('actionmaps.xml.scbpw-')
                   for n in os.listdir(_wiese115)),
               'vor dem Schreiben entsteht eine Sicherung')

        # 3. Konflikte werden gemeldet, BEVOR etwas passiert.
        _konflikt115 = _js115.konflikte('v_eject', 'js1', 'button9',
                                        datei=_datei115)
        pruefe(any(k['aktion'] == 'v_lights' for k in _konflikt115),
               'eine doppelte Belegung wird vorher gemeldet')

        # 4. Loeschen heisst leere Eingabe — nicht Eintrag weg. Nur so laesst
        #    das Spiel die Werkseinstellung ebenfalls aus.
        _js115.belegen('v_eject', 'spaceship_general', 'js1', '',
                       datei=_datei115)
        with open(_datei115, encoding='utf-8') as _f115:
            _roh115 = _f115.read()
        pruefe('js1_"' in _roh115 or 'js1_' in _roh115,
               'das Entfernen schreibt eine leere Eingabe')
        _leer115 = _js115.belegungen(datei=_datei115)
        pruefe(not [e for e in _leer115.get('js1', [])
                    if e['aktion'] == 'v_eject' and e['eingabe']],
               'die entfernte Belegung erscheint nicht mehr als belegt')

        # 5. Die Datei muss lesbares XML geblieben sein.
        _heil115 = True
        try:
            __import__('xml.etree.ElementTree',
                       fromlist=['ElementTree']).parse(_datei115)
        except Exception:
            _heil115 = False
        pruefe(_heil115, 'die Datei ist gueltiges XML geblieben')

        # 6. Sichern und wieder einspielen — der Weg, den man sonst nur ueber
        #    die Spielkonsole hat.
        _kopie115 = os.path.join(_wiese115, 'gesichert.xml')
        _ok115, _ = _js115.ausgeben(_kopie115, 'de', datei=_datei115)
        pruefe(_ok115 and os.path.isfile(_kopie115),
               'die Belegung laesst sich als Datei sichern')
        _liste115 = os.path.join(_wiese115, 'liste.csv')
        _js115.ausgeben(_liste115, 'de', datei=_datei115)
        with open(_liste115, encoding='utf-8-sig') as _f115:
            _csv115 = _f115.read()
        pruefe(_csv115.startswith('Geraet;') and 'v_lights' in _csv115,
               'die Liste als CSV enthaelt die Belegungen')
        # ⚠ Eine fremde XML-Datei darf NICHT als Belegung durchgehen — sonst
        # landet irgendetwas als Steuerung im Spiel.
        _fremd115 = os.path.join(_wiese115, 'fremd.xml')
        with open(_fremd115, 'w', encoding='utf-8') as _f115:
            _f115.write('<Etwas><Anderes/></Etwas>')
        _ok115, _, _ = _js115.einlesen(_fremd115, datei=_datei115)
        pruefe(not _ok115, 'eine fremde XML-Datei wird abgelehnt')

        # 7. ⚠⚠ Zuruecksetzen wirft die Belegungen weg — aber NICHT die
        #    Geraeteeinstellungen. Wer „Belegung zuruecksetzen" drueckt, will
        #    seine Totzonen nicht neu einmessen.
        with open(_datei115, encoding='utf-8') as _f115:
            _vor115 = _f115.read()
        _dev115 = ('<deviceoptions name="Stick">' + chr(10) +
                   '   <option input="x" deadzone="0.1"/>' + chr(10) +
                   '  </deviceoptions>' + chr(10) + '  ')
        with open(_datei115, 'w', encoding='utf-8') as _f115:
            _f115.write(_vor115.replace('  <actionmap', '  ' + _dev115
                                        + '<actionmap', 1))
        _ok115, _m115, _n115 = _js115.zuruecksetzen(datei=_datei115)
        with open(_datei115, encoding='utf-8') as _f115:
            _nach115 = _f115.read()
        pruefe(_ok115 and _n115 >= 1, 'das Zuruecksetzen entfernt die Gruppen')
        pruefe(not _js115.belegungen(datei=_datei115),
               'danach ist keine eigene Belegung mehr da')
        pruefe('deadzone' in _nach115,
               'Totzonen und Kurven bleiben beim Zuruecksetzen stehen')

        # 8. Und die Sicherung von vorhin laesst sich wieder einspielen.
        _ok115, _m115, _n115 = _js115.einlesen(_kopie115, datei=_datei115)
        pruefe(_ok115 and _js115.belegungen(datei=_datei115),
               'die gesicherte Belegung laesst sich zurueckholen')

        # 9. ⚠⚠ **Eigene Belegung verdraengt den Standard nur auf DEM GERAET.**
        #
        # Am 04.09.2026 gemeldet: In „noch nicht belegt" standen Scheinwerfer,
        # Hocken, Respawn und die linke Maustaste — alles Aktionen, die ab Werk
        # laengst eine Taste haben. Ursache war ein Zusammenfuehren, das die
        # Werksvorgabe fuer **alle** Geraete wegwarf, sobald die Aktion
        # irgendwo eigen belegt war. Wer „Respawn" auf den Stick legte, verlor
        # in der Anzeige die Taste `F` — die im Spiel weiter funktioniert.
        #
        # Der Standard kommt sonst aus dem `Data.p4k`, das hier nicht liegt.
        # Deshalb wird er fuer diese Pruefung untergeschoben — kein Abruf,
        # keine Nutzerdatei.
        _echt115 = _js115._profil
        try:
            _js115._profil = lambda *a, **k: {
                'etiketten': {'v_lights': ['@ui_x', ''],
                              'crouch': ['@ui_y', '']},
                'standard': {'v_lights': {'keyboard': 'l', 'joystick': 'button3'},
                             'crouch': {'keyboard': 'c'}},
                'gruppen': {}}
            # Eigene Datei: `v_lights` NUR auf dem Stick umbelegt.
            with open(_datei115, 'w', encoding='utf-8') as _f115:
                _f115.write(
                    '<ActionMaps><ActionProfiles profileName="default">'
                    '<actionmap name="spaceship_general">'
                    '<action name="v_lights">'
                    '<rebind input="js1_button31"/>'
                    '</action></actionmap>'
                    '</ActionProfiles></ActionMaps>')
            _alles115 = _js115.sicht(_js115.ALLES, datei=_datei115)
            _wo115 = [(k, e['eingabe']) for k, li in _alles115.items()
                      for e in li if e['aktion'] == 'v_lights']
            pruefe(('js1', 'button31') in _wo115,
                   'die eigene Stick-Belegung steht in der Gesamtsicht')
            pruefe(('kb1', 'l') in _wo115,
                   'die Werks-TASTE derselben Aktion bleibt trotzdem stehen')
            pruefe(('js1', 'button3') not in _wo115,
                   'die Werks-STICK-Belegung wird dagegen verdraengt')
            _frei115 = _js115.sicht(_js115.FREI, datei=_datei115)
            _freinamen = {e['aktion'] for li in _frei115.values() for e in li}
            pruefe('v_lights' not in _freinamen and 'crouch' not in _freinamen,
                   'was ab Werk belegt ist, steht nicht unter „nicht belegt"')
        finally:
            _js115._profil = _echt115
            _js115.vergessen()
    finally:
        shutil.rmtree(_wiese115, ignore_errors=True)

    print()
    print('114. Sicherung: alles Eigene rein, alles wieder raus')
    _sich = importlib.import_module('scbp.sicherung')
    _altheim114 = os.environ.get('SC_BP_HOME')
    _wiese114 = tempfile.mkdtemp(prefix='sc-bp-sicherung-')
    try:
        _quell114 = os.path.join(_wiese114, 'quelle')
        _ziel114 = os.path.join(_wiese114, 'ziel')
        for _o in (_quell114, _ziel114):
            os.makedirs(os.path.join(_o, 'Bauplaene'), exist_ok=True)
            os.makedirs(os.path.join(_o, 'Intern'), exist_ok=True)
        os.environ['SC_BP_HOME'] = _quell114
        importlib.reload(_sich)

        # Eigene Daten …
        with open(os.path.join(_quell114, 'Bauplaene', 'bestand.json'),
                  'w', encoding='utf-8') as _f:
            json.dump({'bauplaene': {'testhelm': {}}}, _f)
        with open(os.path.join(_quell114, 'Intern', 'handelslager.json'),
                  'w', encoding='utf-8') as _f:
            json.dump([{'ware': 'Gold'}], _f)
        # … und eine Datei, die es neu gibt und die NIEMAND aufgezaehlt hat.
        # ⚠ Genau daran ist die frühere Liste gescheitert: Das Auftrags-
        # Protokoll kam dazu und fiel stillschweigend heraus.
        with open(os.path.join(_quell114, 'Intern', 'ganz-neu.json'),
                  'w', encoding='utf-8') as _f:
            json.dump({'kommt': 'spaeter dazu'}, _f)
        # … und ein Zwischenspeicher, der draussen bleiben soll.
        with open(os.path.join(_quell114, 'Intern', 'preise.json'),
                  'w', encoding='utf-8') as _f:
            json.dump({'gross': 'x' * 5000}, _f)

        _datei114 = os.path.join(_wiese114, 'sicherung.zip')
        _ok114, _m114, _n114 = _sich.schreiben(_datei114, '9.9.9')
        pruefe(_ok114, 'die Sicherung wird geschrieben')

        import zipfile as _zip114
        with _zip114.ZipFile(_datei114) as _z:
            _drin114 = set(_z.namelist())
        pruefe('Bauplaene/bestand.json' in _drin114, 'der Bestand ist dabei')
        pruefe('Intern/handelslager.json' in _drin114,
               'das Handelslager ist dabei')
        pruefe('Intern/ganz-neu.json' in _drin114,
               'eine NEUE eigene Datei ist ohne Zutun dabei')
        pruefe('Intern/preise.json' not in _drin114,
               'der nachladbare Zwischenspeicher bleibt draussen')

        _g114, _anz114, _wann114 = _sich.pruefen(_datei114)
        pruefe(_g114 and _wann114, 'die eigene Datei wird als gueltig erkannt')

        # Einspielen in eine LEERE Ablage — der Rechnerwechsel.
        os.environ['SC_BP_HOME'] = _ziel114
        importlib.reload(_sich)
        _ok114, _m114, _n114 = _sich.zurueckholen(_datei114)
        pruefe(_ok114, 'die Sicherung laesst sich einspielen')
        with open(os.path.join(_ziel114, 'Bauplaene', 'bestand.json'),
                  encoding='utf-8') as _f:
            pruefe('testhelm' in json.load(_f).get('bauplaene', {}),
                   'der Bestand ist nach dem Einspielen da')
        pruefe(os.path.isfile(os.path.join(_ziel114, 'Intern',
                                           'ganz-neu.json')),
               'auch die neue Datei kam mit')

        # ⚠ Gegenprobe 1: Eine fremde ZIP darf NICHTS ueberschreiben.
        _fremd114 = os.path.join(_wiese114, 'fremd.zip')
        with _zip114.ZipFile(_fremd114, 'w') as _z:
            _z.writestr('beliebig.txt', 'nicht von uns')
        pruefe(_sich.zurueckholen(_fremd114)[0] is False,
               'eine fremde Datei wird abgelehnt')

        # ⚠ Gegenprobe 2: Ein Pfad, der aus der Ablage herausfuehrt („Zip
        # Slip"). Ohne Abwehr schreibt eine praeparierte Datei irgendwohin.
        _boese114 = os.path.join(_wiese114, 'boese.zip')
        with _zip114.ZipFile(_boese114, 'w') as _z:
            _z.writestr(_sich.INFODATEI,
                        _sich.KENNUNG + '\nErstellt am 01.01.2026 mit x')
            _z.writestr('../entkommen.txt', 'darf nicht landen')
        _sich.zurueckholen(_boese114)
        pruefe(not os.path.exists(os.path.join(_wiese114, 'entkommen.txt')),
               'ein Pfad aus der Ablage heraus wird abgewehrt')

        # ⚠ Gegenprobe 3: Pfade des alten Rechners werden geleert — sonst
        # sucht das Programm am neuen Ort ins Leere.
        os.makedirs(os.path.join(_quell114, 'Einstellungen'), exist_ok=True)
        with open(os.path.join(_quell114, 'Einstellungen',
                               'einstellungen.json'), 'w',
                  encoding='utf-8') as _f:
            json.dump({'spiel_ordner': '/gibt/es/hier/nicht',
                       'ablage_ordner': '/alter/rechner'}, _f)
        os.environ['SC_BP_HOME'] = _quell114
        importlib.reload(_sich)
        _sich.schreiben(_datei114, '9.9.9')
        os.environ['SC_BP_HOME'] = _ziel114
        importlib.reload(_sich)
        _sich.zurueckholen(_datei114)
        with open(os.path.join(_ziel114, 'Einstellungen',
                               'einstellungen.json'), encoding='utf-8') as _f:
            _e114 = json.load(_f)
        pruefe(_e114.get('spiel_ordner') == '',
               'ein Spielordner, den es hier nicht gibt, wird geleert')
        pruefe('ablage_ordner' not in _e114,
               'der Ablage-Ort des alten Rechners kommt NICHT mit')
    finally:
        if _altheim114 is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _altheim114
        shutil.rmtree(_wiese114, ignore_errors=True)

    print()
    print('116. Der eigene Bestand haengt NICHT am Takt der fremden Quellen')
    # ⚠⚠ **Gemeldet von Bushwick4712 am 05.09.2026** — sichtbar an zwei Zahlen
    # in seinem Bericht: `Bestand 304`, `inj_bestand=303-…`. Die Kaestchen im
    # Spiel hinkten also einen Bauplan hinterher, obwohl das automatische
    # Auffrischen eingeschaltet war.
    #
    # Die Bedingung „hat sich der Bestand geaendert?" gab es seit dem
    # 04.09.2026 — sie hing nur im selben Sechs-Stunden-Takt wie die
    # Netzabfragen nach neuer Uebersetzung und neuen Vertragsdaten. Sein Lauf
    # dauerte 20 Minuten. Nach dem Durchlauf beim Start wurde nie wieder
    # geschaut, und beim naechsten Start stand dieselbe Wartezeit erneut an.
    #
    # ⚠ Was diese Pruefung wirklich festhaelt, ist nicht die Zahl 30, sondern
    # das VERHAELTNIS: Der eigene Bestand aendert sich, waehrend gespielt wird;
    # fremde Quellen aendern sich im Tagesrhythmus. Wer die beiden Takte
    # wieder zusammenlegt, faellt hier auf.
    _quelle116 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'),
                      encoding='utf-8').read()

    def _takt116(name):
        treffer = re.search(
            r'^%s\s*=\s*([0-9]+)\s*(?:\*\s*([0-9]+))?' % name,
            _quelle116, re.M)
        if not treffer:
            return None
        wert = int(treffer.group(1))
        return wert * int(treffer.group(2)) if treffer.group(2) else wert

    _eigen116 = _takt116('BESTAND_POLL_SEC')
    _fremd116 = _takt116('TEXTE_POLL_SEC')
    pruefe(_eigen116 is not None,
           'es gibt einen eigenen Takt fuer den Bestand')
    pruefe(_fremd116 is not None,
           'es gibt einen Takt fuer die fremden Quellen')
    if _eigen116 and _fremd116:
        pruefe(_eigen116 < _fremd116,
               'der Bestand wird oefter geprueft als die fremden Quellen '
               '(%ds gegen %ds)' % (_eigen116, _fremd116))
        # Eine Viertelstunde ist die Grenze, ab der es sich wie „kaputt“
        # anfuehlt: So lange spielt man mit falschen Kaestchen weiter.
        pruefe(_eigen116 <= 900,
               'der Bestand wird mindestens alle 15 Minuten geprueft '
               '(%ds)' % _eigen116)

    # Und die zweite Haelfte: Ein gefundener Bauplan darf die Netzabfrage
    # NICHT verschieben — sonst bekaeme ein Vielspieler die neue Uebersetzung
    # nie. `texte_next` wird deshalb nur im faelligen Lauf neu gesetzt.
    _tick116 = re.search(r'def _texte_tick\(self\):(.*?)\n    def ',
                         _quelle116, re.S)
    pruefe(bool(_tick116), 'der Takt-Abschnitt ist auffindbar')
    if _tick116:
        _rumpf116 = _tick116.group(1)
        pruefe('if faellig:' in _rumpf116,
               'die Netzabfrage schiebt ihren Termin nur im faelligen Lauf')
        pruefe('nur_bestand=not faellig' in _rumpf116.replace(' ', '')
               .replace('nur_bestand=notfaellig', 'nur_bestand=not faellig'),
               'ein reiner Bestands-Lauf ueberspringt die fremden Pruefungen')

    print()
    print('117. Jede Seite mit Bestandszahlen zieht nach')
    # ⚠⚠ **Gemeldet von Bushwick4712 am 05.09.2026:** Bauplan faellt, Werkzeug
    # meldet ihn — und in der Liste stand weiter die alte Anzahl, ohne gruenen
    # Haken. Der Bestand wurde beim Bauen der Seite gelesen und danach nie
    # wieder; die Seite selbst wird nur ein- und ausgeblendet.
    #
    # Vier weitere Seiten hatten denselben Fehler. Sie stehen jetzt in
    # `BESTANDSSEITEN` und werden bei einer Bestandsaenderung verworfen.
    #
    # ⚠ Diese Pruefung haelt die LISTE vollstaendig: Wer morgen eine Seite
    # baut, die `bestand_datei` liest, faellt hier auf, statt es niemandem zu
    # sagen. Genau so ist der gemeldete Fehler entstanden — die Seiten kamen
    # nach und nach dazu, und niemand ging die alten noch einmal durch.
    from scbp.hauptfenster import Hauptfenster as _HF117

    _seiten117 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                      encoding='utf-8').read()
    # Jede Seitenfunktion heisst `_<kennung>(fenster, rahmen)` — der Rumpf
    # reicht bis zur naechsten Funktion auf Modulebene.
    _stellen117 = [m.start() for m in
                   re.finditer(r'^def _[a-z_]+\(fenster, rahmen\)',
                               _seiten117, re.M)]
    _stellen117.append(len(_seiten117))
    _liest117 = set()
    for _i117 in range(len(_stellen117) - 1):
        _stueck117 = _seiten117[_stellen117[_i117]:_stellen117[_i117 + 1]]
        _name117 = re.match(r'^def _([a-z_]+)\(', _stueck117).group(1)
        if 'bestand_datei.laden()' in _stueck117 or '_zahl_bestand()' in _stueck117:
            _liest117.add(_name117)

    pruefe(bool(_liest117),
           'die Seiten mit Bestandszahlen sind auffindbar (%d gefunden)'
           % len(_liest117))
    # `liste` geht einen eigenen Weg (`neu_laden`) und steht deshalb nicht in
    # der Menge — sie liest den Bestand im Bestandsfenster, nicht hier.
    #
    # ⚠ **`diagnose` ist eine bewusste Ausnahme.** Sie zeigt die Bestandszahl
    # im Fehlerbericht, darf aber nicht verworfen werden: Ein Neubau leerte das
    # Feld „Was ist passiert?" — jemand tippt seine Beschreibung, sieht kurz
    # woanders nach, und der Text waere weg. Sie frischt stattdessen ueber
    # `beim_zeigen['diagnose']` nur den Berichtstext auf. Wer diese Zeile
    # entfernt, muss dort einen anderen Weg bauen, nicht die Seite verwerfen.
    _ausnahmen117 = {'diagnose'}
    pruefe("beim_zeigen['diagnose']" in _seiten117,
           'die Diagnose-Seite frischt ihren Bericht beim Oeffnen auf')
    _fehlt117 = sorted(_liest117 - set(_HF117.BESTANDSSEITEN) - _ausnahmen117)
    pruefe(not _fehlt117,
           'jede Seite mit Bestandszahlen steht in BESTANDSSEITEN '
           '(fehlt: %s)' % (', '.join(_fehlt117) or 'keine'))
    # Und andersherum: Kein Eintrag, den es gar nicht gibt — ein Tippfehler
    # dort waere still, die Seite zoege einfach nie nach.
    _reiter117 = set(re.findall(r"_reiter\('([a-z_]+)'",
                                open(os.path.join(WURZEL, 'scbp',
                                                  'hauptfenster.py'),
                                     encoding='utf-8').read()))
    _tot117 = sorted(set(_HF117.BESTANDSSEITEN) - _reiter117)
    pruefe(not _tot117,
           'kein Eintrag in BESTANDSSEITEN ohne Seite (tot: %s)'
           % (', '.join(_tot117) or 'keine'))

    # Die Liste selbst muss den eigenen Weg wirklich haben.
    from scbp import bestandsfenster as _bf117
    pruefe(hasattr(_bf117.Bestandsfenster, 'neu_laden'),
           'die Bauplan-Liste kann sich ohne Neubau auffrischen')

    # Und der Weg dorthin: Wer speichert, meldet. Sonst bleibt der Fehler
    # bestehen, auch wenn alle Seiten eingetragen sind.
    _wq117 = open(os.path.join(WURZEL, 'sc_bp_watcher.py'),
                  encoding='utf-8').read()
    _direkt117 = len(re.findall(r'bestand_datei\.speichern\(self\.bestand\)',
                                _wq117))
    pruefe(_direkt117 == 1,
           'der Bestand wird nur ueber _bestand_sichern() geschrieben '
           '(%d direkte Aufrufe, erlaubt ist 1)' % _direkt117)
    pruefe("'liste_frisch'" in _wq117,
           'das Signal nach dem Speichern kommt in der Anzeige an')

    print()
    print('118. Jeder Weg aus dem Fehlerbericht leert das Meldungsfeld')
    # ⚠⚠ Der Satz in „Was ist passiert?" gehoert zu EINEM Bericht. Bleibt er
    # stehen, haengt er unbemerkt am naechsten — und der Entwickler sucht einen
    # Fehler, den der Melder vor einer Woche hatte.
    #
    # Beim ersten Anlauf am 05.09.2026 hing das Leeren nur am Absenden.
    # Gemeldet noch am selben Tag: „bei Angaben kopieren wird der text nicht
    # geloescht" — und beim Melde-Knopf ebenso wenig. Drei Wege aus dem
    # Bericht heraus, einer davon aufgeraeumt: Das ist kein halber Fix, das
    # ist ein Fix, der beim naechsten Nutzer wieder auffaellt.
    #
    # ⚠ Diese Wache faengt den VIERTEN Weg, den jemand spaeter baut.
    _sei118 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                   encoding='utf-8').read()

    # Die Diagnose-Seite herausschneiden — `melden` und `kopieren` heissen
    # anderswo genauso, und ohne den Schnitt zaehlte die Pruefung fremde
    # Funktionen mit.
    _start118 = _sei118.find('def _diagnose(')
    pruefe(_start118 > 0, 'die Diagnose-Seite ist auffindbar')
    _ende118 = _sei118.find('\ndef ', _sei118.find('def aktueller_bericht'))
    _block118 = _sei118[_start118:_ende118 if _ende118 > 0 else len(_sei118)]

    pruefe('def _meldung_verbraucht' in _block118,
           'es gibt eine Stelle, die das Meldungsfeld leert')

    # Jede der drei Knopf-Funktionen muss sie rufen. Der Rumpf reicht bis zur
    # naechsten Definition auf derselben Einrueckung.
    for _knopf118 in ('melden', 'kopieren', 'absenden'):
        _pos118 = _block118.find('\n    def %s():' % _knopf118)
        if _pos118 < 0:
            pruefe(False, 'die Funktion %s() ist auffindbar' % _knopf118)
            continue
        _naechste118 = _block118.find('\n    def ', _pos118 + 10)
        _rumpf118 = _block118[_pos118:_naechste118 if _naechste118 > 0
                              else len(_block118)]
        pruefe('_meldung_verbraucht()' in _rumpf118,
               '%s() leert das Meldungsfeld' % _knopf118)

    # ⚠ Und die Gegenrichtung: NUR bei Erfolg. Scheitert das Senden, muss der
    # Text stehen bleiben — ihn dann zu loeschen naehme dem Melder seine
    # Arbeit, genau bevor er es noch einmal versucht.
    _abs118 = _block118.find('\n    def absenden():')
    if _abs118 > 0:
        _rumpf118 = _block118[_abs118:_block118.find('\n    def ', _abs118 + 10)]
        pruefe('if geklappt:' in _rumpf118,
               'beim Absenden wird nur nach Erfolg geleert')

    print()
    print('119. Die Melde-Adresse kommt nicht in den oeffentlichen Bericht')
    # ⚠⚠ **Gemessen am 05.09.2026, nicht vermutet.** `bericht.absenden()` gibt
    # den Grund eines gescheiterten Sendeversuchs bewusst NICHT zurueck, weil
    # die Adresse geheim ist — eine Zeile darueber steht aber
    # `fehler.merken('bericht.absenden', ausnahme)`, und das Fehlerprotokoll
    # steht im Bericht, und der Bericht landet in einem oeffentlichen Issue.
    #
    # Vier realistische Fehlerfaelle durchgespielt: drei harmlos (urllib nennt
    # die Adresse nicht), einer nicht — jede Meldung, die eine Adresse selbst
    # in ihren Text schreibt. Genau dieses Muster gibt es im Programm bereits:
    # Die Netzabrufe haengen die abgerufene Adresse an ihre Meldung.
    #
    # Wer den Webhook hat, kann in den Kanal schreiben. Deshalb eine Wache und
    # nicht nur ein Kommentar.
    from scbp import pfade as _pf119

    for _name119, _text119, _darf_nicht119 in (
            ('Discord-Webhook',
             'Senden an https://discord.com/api/webhooks/123/GeHeIm_xyz weg',
             'GeHeIm_xyz'),
            ('discordapp-Schreibweise',
             'POST https://discordapp.com/api/webhooks/9/ZuGaNg42 -> 404',
             'ZuGaNg42'),
            ('Schluessel als Parameter',
             'https://dienst.de/abruf?api_key=abc123geheim&format=json',
             'abc123geheim'),
            ('Token als Parameter',
             'https://dienst.de/x?token=eyJhbGciOiJIUzI1NiJ9geheim',
             'eyJhbGciOiJIUzI1NiJ9geheim')):
        pruefe(_darf_nicht119 not in _pf119.kuerzen(_text119),
               '%s wird unkenntlich gemacht' % _name119)

    # ⚠⚠ **Und die Gegenrichtung — sonst waere die Wache eine Verschlechterung.**
    # Die Adresse in einem Netzfehler ist das Wertvollste am ganzen Eintrag:
    # Sie sagt, WELCHER Abruf schiefging. Eine Wache, die alles schwaerzt,
    # macht den Bericht wertlos.
    for _name119, _bleibt119 in (
            ('UEX-Kategorie', 'https://api.uexcorp.uk/2.0/items?id_category=3'),
            ('UEX mit uuid',
             'https://api.uexcorp.uk/2.0/items_prices?uuid=abc-def-123'),
            ('UEX-Route',
             'https://api.uexcorp.uk/2.0/commodities_routes'
             '?id_terminal_origin=42')):
        pruefe(_pf119.kuerzen(_bleibt119) == _bleibt119,
               '%s bleibt lesbar' % _name119)

    print()
    print('120. Melde-Seite: Feld gross genug, Zusicherung vor dem Klick')
    # ⚠⚠ Zwei Meldungen vom 05.09.2026, beide zur selben Seite:
    #
    # a) Das Meldungsfeld war ein einzeiliges `Entry` rechts neben dem Text —
    #    halb so breit wie die Seite. Wer zwei Saetze tippte, sah nur das Ende
    #    und konnte vor dem Absenden nicht nachlesen, was er meldet.
    # b) Die Zusicherung „Du siehst vorher genau, was du verschickst" stand
    #    UNTER der Knopfreihe, also hinter dem Klick — und konnte am unteren
    #    Rand wegfallen.
    #
    # ⚠ Nach OBEN gehoert sie trotzdem nicht: Ihr Text lautet „Der Block oben
    # ist der ganze Inhalt". Ueber dem Bericht stimmte der Bezug nicht mehr.
    # Die Pruefung haelt deshalb BEIDE Grenzen fest — hinter dem Bericht,
    # vor den Knoepfen.
    import tkinter as tk
    _w120 = _wurzel()
    try:
        _w120.deiconify()
        _w120.geometry('1200x900')
        from scbp import hauptfenster as _hf120
        _f120 = _hf120.Hauptfenster(_w120, version='0.0.0-pruefung')
        _f120.oeffnen('diagnose')
        _w120.update_idletasks()

        _alle120 = []

        def _sammeln120(knoten):
            for kind in knoten.winfo_children():
                _alle120.append(kind)
                _sammeln120(kind)

        _sammeln120(_f120.seiten['diagnose'])

        _texte120 = [w for w in _alle120 if isinstance(w, tk.Text)]
        _eingabe120 = [w for w in _texte120 if int(w.cget('height')) == 4]
        _kasten120 = [w for w in _texte120 if w not in _eingabe120]

        pruefe(bool(_eingabe120),
               'das Meldungsfeld ist mehrzeilig (ein Text, kein Entry)')
        if _eingabe120:
            _b120 = _eingabe120[0].winfo_width()
            _seite120 = _f120.seiten['diagnose'].winfo_width()
            pruefe(_b120 > _seite120 * 0.6,
                   'das Meldungsfeld nimmt die volle Breite (%d von %d px)'
                   % (_b120, _seite120))

        # Die Zusicherung ueber ihren Text finden.
        _sicher120 = None
        _gesucht120 = sprache.t('s_di_sicher')
        for _w in _alle120:
            try:
                if isinstance(_w, tk.Label) and _w.cget('text') == _gesucht120:
                    _sicher120 = _w
                    break
            except tk.TclError:
                pass
        pruefe(_sicher120 is not None, 'die Zusicherung ist auf der Seite')

        # ⚠ NICHT ueber y-Koordinaten messen: Die Zusicherung enthaelt selbst
        # eine Leinwand (das Haekchen), und die sieht wie ein Knopf aus — der
        # erste Anlauf dieser Pruefung meldete deshalb einen Fehler, den es
        # nicht gab. Gemessen wird die Aufbaureihenfolge im gemeinsamen
        # Rahmen, also genau das, was `pack` untereinander setzt.
        def _vorfahr120(widget, eltern):
            lauf = widget
            while lauf is not None and lauf.master is not eltern:
                lauf = lauf.master
            return lauf

        if _sicher120 is not None and _kasten120:
            _innen120 = _sicher120.master
            while _innen120 is not None:
                if _vorfahr120(_kasten120[0], _innen120) is not None:
                    break
                _innen120 = _innen120.master
            pruefe(_innen120 is not None,
                   'Bericht und Zusicherung stehen im selben Rahmen')
            if _innen120 is not None:
                _folge120 = _innen120.pack_slaves()
                _i_kasten120 = _folge120.index(
                    _vorfahr120(_kasten120[0], _innen120))
                _i_sicher120 = _folge120.index(
                    _vorfahr120(_sicher120, _innen120))
                pruefe(_i_sicher120 > _i_kasten120,
                       'die Zusicherung steht HINTER dem Bericht '
                       '(ihr Text sagt „der Block oben")')
                _knopf120 = None
                for _n120 in range(_i_sicher120 + 1, len(_folge120)):
                    _lein120 = [k for k in _folge120[_n120].winfo_children()
                                if isinstance(k, tk.Canvas)]
                    if len(_lein120) >= 3:
                        _knopf120 = _n120
                        break
                pruefe(_knopf120 is not None,
                       'die Knopfreihe kommt NACH der Zusicherung '
                       '(sonst steht sie hinter dem Klick)')
    finally:
        try:
            _w120.destroy()
        except Exception:
            pass

    print()
    print('121. Eingeklapptes Overlay: ein Fund holt es kurz heraus')
    # ⚠⚠ **Die Funktion gab es, bewacht war sie nicht.** Am 05.09.2026
    # nachgefragt — und beim Nachsehen stand im Selbsttest zu
    # `bei_fund_zeigen` und `_wieder_zuklappen` keine Zeile. Eine Funktion
    # ohne Wache ist eine Funktion auf Zeit: Der naechste Umbau am Klappen
    # nimmt sie mit, und gemerkt wird es erst, wenn jemand im Kampf einen
    # Bauplan verpasst.
    #
    # Worum es geht: Wer „Immer sichtbar" gewaehlt und die Leiste zugeklappt
    # hat, bekam frueher nur den Signalton. Mit durchgereichten Mausklicks
    # war das doppelt aergerlich — man hoert etwas, kann aber nichts
    # anklicken. Ein zugeklapptes Overlay schaltete damit genau die Funktion
    # ab, fuer die es da ist.
    import tkinter as tk121
    from scbp import pfade as _pf121

    _alt121 = {
        'sek': _pf121.einstellung('popup_sekunden'),
        'modus': _pf121.einstellung('overlay_modus'),
    }
    try:
        # Kurze Zeit, damit die Pruefung nicht sekundenlang dasteht.
        _pf121.einstellung_setzen('popup_sekunden', 2)
        _pf121.einstellung_setzen('overlay_modus', 'immer')

        import sc_bp_watcher as _w121
        _ov121 = _w121.Overlay()
        _ov121.root.update_idletasks()

        pruefe(_ov121.anzeigeart != 'popup',
               'die Pruefung laeuft im Dauerbetrieb, nicht im Aufblenden')

        _ov121.klappzustand_setzen(True)
        _ov121.root.update_idletasks()
        pruefe(_ov121.eingeklappt, 'das Overlay laesst sich einklappen')
        pruefe(_pf121.einstellung_wahrheit('eingeklappt', False),
               'der Wunsch „zugeklappt" wird gemerkt')

        _ov121.add_new('Pruef-Bauplan', 'WeaponGun', '', '12:00:00')
        _ov121.root.update_idletasks()
        pruefe(not _ov121.eingeklappt,
               'ein Fund holt das eingeklappte Overlay heraus')
        pruefe(_ov121._zuklapp_uhr is not None,
               'und stellt eine Uhr, damit es nicht offen stehen bleibt')
        # ⚠ Der Blick darf den gemerkten Wunsch NICHT umschreiben — sonst
        # staende das Overlay beim naechsten Start offen, obwohl der Spieler
        # es zugeklappt haben wollte.
        pruefe(_pf121.einstellung_wahrheit('eingeklappt', False),
               'der gemerkte Wunsch bleibt trotzdem „zugeklappt"')

        # ⚠⚠ **Der Mauszeiger muss festgehalten werden.** Unter Xvfb steht er
        # bei (0,0); das aufklappende Overlay landet genau dort, loest
        # `<Enter>` aus und setzt `_maus_drauf` — dann WARTET das Zuklappen,
        # zu Recht, und die Pruefung misst ihre eigene Umgebung statt des
        # Programms. Genau so meldete der erste Anlauf einen Fehler, den es
        # nicht gab.
        _ov121._maus_drauf = False

        class _Fern121(object):
            def __get__(self, _o, _t=None):
                return False

            def __set__(self, _o, _w):
                pass

        type(_ov121)._maus_drauf = _Fern121()
        try:
            _ov121.root.after(2400, _ov121.root.quit)
            _ov121.root.mainloop()
            pruefe(_ov121.eingeklappt,
                   'nach der eingestellten Zeit geht es zurueck in die Leiste')
        finally:
            del type(_ov121)._maus_drauf

        # Und die Ruecksicht: Wer gerade liest, dem klappt nichts unter dem
        # Zeiger weg.
        _ov121.klappzustand_setzen(True)
        _ov121.add_new('Zweiter Pruef-Bauplan', 'WeaponGun', '', '12:00:05')
        _ov121._maus_drauf = True
        _ov121.root.after(2400, _ov121.root.quit)
        _ov121.root.mainloop()
        pruefe(not _ov121.eingeklappt,
               'mit der Maus darauf bleibt es offen, statt wegzuklappen')

        try:
            _ov121.root.destroy()
        except tk121.TclError:
            pass
    finally:
        for _s121, _wert121 in (('popup_sekunden', _alt121['sek']),
                                ('overlay_modus', _alt121['modus'])):
            if _wert121 is not None:
                _pf121.einstellung_setzen(_s121, _wert121)

    print()
    print('122. „Info" laesst sich nicht zuklappen — dort steht Fehler melden')
    # ⚠⚠ **Gemeldet am 05.09.2026:** „Info sollte auch nicht einklappbar sein,
    # sonst blendet jemand Fehler melden aus, und findet es nicht mehr."
    #
    # Genau so: Wer die Gruppe zuklappt, blendet den Weg aus, auf dem er ein
    # Problem loswird — und sucht ihn dann, wenn etwas klemmt und die Geduld
    # ohnehin am Ende ist.
    #
    # ⚠ Die uebrigen Gruppen bleiben klappbar, und das gehoert mitgeprueft:
    # Das Zuklappen gibt es aus einem guten Grund — die Seitenleiste bestimmt
    # die Mindesthoehe des Fensters, zugeklappte Gruppen sparen rund 400 px.
    # Wer hier alles festnagelt, holt den alten Fehler zurueck.
    import tkinter as tk122
    from scbp import pfade as _pf122, hauptfenster as _hf122

    _alt122 = {k: _pf122.einstellung('gruppe_zu_%s' % k)
               for k in ('info', 'werkstatt')}
    _w122 = _wurzel()
    try:
        # ⚠ Der harte Fall: Jemand hatte die Gruppe FRUEHER zugeklappt. Genau
        # bei dem muss sie jetzt wieder aufgehen — sonst hilft die Aenderung
        # niemandem, den sie betrifft.
        _pf122.einstellung_setzen('gruppe_zu_info', 'ja')
        _pf122.einstellung_setzen('gruppe_zu_werkstatt', 'ja')

        _w122.deiconify()
        _w122.geometry('1200x900')
        _f122 = _hf122.Hauptfenster(_w122, version='0.0.0-pruefung')
        _w122.update_idletasks()

        pruefe('info' in _hf122.Hauptfenster.IMMER_OFFEN,
               '„info" steht in der Liste der festen Gruppen')

        _gi122 = _f122.gruppen.get('info')
        _gw122 = _f122.gruppen.get('werkstatt')
        pruefe(bool(_gi122 and _gi122['offen']),
               'Info steht offen, auch mit einem alten „zu" in den '
               'Einstellungen')
        pruefe(bool(_gw122 and not _gw122['offen']),
               'eine gewoehnliche Gruppe bleibt dagegen zugeklappt')

        if _gi122:
            pruefe(not _gi122['pfeil'].winfo_manager(),
                   'Info zeigt keinen Klapp-Pfeil (ein Pfeil ist ein '
                   'Versprechen)')
            pruefe(not _gi122['kopf'].cget('cursor'),
                   'und keinen Zeigefinger, wo nichts zu klicken ist')

        # ⚠ Der Riegel muss in `_gruppe_um` sitzen, nicht nur an der Bindung:
        # Die Funktion wird auch von `_gruppe_von_reiter_oeffnen` gerufen.
        _f122._gruppe_um('info')
        _w122.update_idletasks()
        pruefe(_f122.gruppen['info']['offen'],
               'auch ein Aufruf von _gruppe_um klappt Info nicht zu')
        _f122._gruppe_um('info', auf=False)
        _w122.update_idletasks()
        pruefe(_f122.gruppen['info']['offen'],
               'und ein erzwungenes Zuklappen ebenso wenig')

        # Gegenprobe: Die uebrigen lassen sich weiterhin klappen.
        _f122._gruppe_um('werkstatt', auf=True)
        _w122.update_idletasks()
        _f122._gruppe_um('werkstatt')
        _w122.update_idletasks()
        pruefe(not _f122.gruppen['werkstatt']['offen'],
               'die uebrigen Gruppen lassen sich weiterhin zuklappen')
    finally:
        try:
            _w122.destroy()
        except tk122.TclError:
            pass
        for _k122, _v122 in _alt122.items():
            if _v122 is not None:
                _pf122.einstellung_setzen('gruppe_zu_%s' % _k122, _v122)

    print()
    print('123. „laeuft" nur, solange das Spiel wirklich schreibt')
    # ⚠⚠ **Gemeldet am 05.09.2026:** „Spiel ist aus, und die Quest die da auf
    # laeuft steht ist von gestern nacht, da bin ich ohne ab zu brechen
    # ausgeloggt weil ich zu muede war."
    #
    # Ausloggen beendet keinen Auftrag — das Spiel schreibt dafuer nichts.
    # Aufgeraeumt wird so ein Fall erst, wenn eine SPAETERE Sitzung ihn nicht
    # mehr nennt (`_verfallene_schliessen`); beim letzten Auftrag vor dem
    # Ausloggen gibt es die noch nicht. Gemessen an 381 echten Auftraegen:
    # 68 waren so bereits aufgeloest, genau einer blieb uebrig — der juengste.
    #
    # ⚠ Der Zustand bleibt richtig, nur das Wort war es nicht: Der Auftrag ist
    # im Spiel weiter angenommen, beim naechsten Einloggen meldet SC ihn
    # erneut. Ihn zu beenden waere gelogen. „laeuft" behauptet aber „jetzt
    # gerade" — „noch offen" stimmt in beiden Faellen.
    import time as _t123
    from scbp import pfade as _pf123

    _wiese123 = tempfile.mkdtemp(prefix='sc-bp-laeuft-')
    _altordner123 = _pf123.einstellung('spiel_ordner')
    try:
        _spiel123 = os.path.join(_wiese123, 'StarCitizen', 'LIVE')
        os.makedirs(_spiel123)
        _log123 = os.path.join(_spiel123, 'Game.log')
        with open(_log123, 'w', encoding='utf-8') as _d123:
            _d123.write('<2026-09-04T20:31:27.000Z> Probe\n')
        _pf123.einstellung_setzen('spiel_ordner', _spiel123)

        os.utime(_log123, None)
        pruefe(_pf123.spiel_laeuft(),
               'ein gerade geschriebenes Log heisst: das Spiel laeuft')

        _alt123 = _t123.time() - 1800
        os.utime(_log123, (_alt123, _alt123))
        pruefe(not _pf123.spiel_laeuft(),
               'ein 30 Minuten stilles Log heisst: das Spiel ist aus')

        # ⚠ Der Grenzfall gehoert dazu: Ein haengender Ladebildschirm darf
        # nicht schon als „Spiel aus" durchgehen.
        _knapp123 = _t123.time() - (_pf123.SPIEL_STILL_SEK - 30)
        os.utime(_log123, (_knapp123, _knapp123))
        pruefe(_pf123.spiel_laeuft(),
               'ein kurzer Haenger gilt noch nicht als „Spiel aus"')

        # ⚠⚠ **Den Ordner ausdruecklich uebergeben, nicht eintragen.**
        # `pfade.spiel_ordner()` faellt auf eine SUCHE zurueck, wenn der
        # eingetragene Pfad nichts hergibt — auf einem Rechner mit Star
        # Citizen findet es dann das echte Spiel, und die Pruefung misst
        # dessen Zustand statt des Programms. Zweimal genau so fehlgeschlagen:
        # erst mit einem erfundenen Pfad, dann mit geloeschter `Game.log`.
        # Mit `ordner=` wird die Suche gar nicht erst gefragt.
        _leer123 = os.path.join(_wiese123, 'ohne-spiel')
        os.makedirs(_leer123)
        pruefe(not _pf123.spiel_laeuft(ordner=_leer123),
               'ohne Game.log im Ordner wird nichts behauptet')
    finally:
        if _altordner123 is not None:
            _pf123.einstellung_setzen('spiel_ordner', _altordner123)
        shutil.rmtree(_wiese123, ignore_errors=True)

    # Die beiden Woerter muessen in die Spalte passen — sie ist 17 Zeichen
    # breit, und ein laengeres Wort stuende dort als Stumpf.
    for _k123 in ('s_al_laeuft', 's_al_offen'):
        pruefe(_k123 in sprache.TEXTE, 'es gibt den Text %s' % _k123)
        if _k123 in sprache.TEXTE:
            for _sp123 in (0, 1):
                pruefe(len(sprache.TEXTE[_k123][_sp123]) <= 17,
                       '%s passt in die Spalte (%d Zeichen)'
                       % (_k123, len(sprache.TEXTE[_k123][_sp123])))

    # Und die Anzeige muss die Unterscheidung wirklich benutzen — sonst steht
    # die Funktion da und keiner ruft sie.
    _sei123 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                   encoding='utf-8').read()
    pruefe('spiel_laeuft()' in _sei123,
           'die Auftragsliste fragt nach, ob das Spiel laeuft')
    pruefe("'s_al_offen'" in _sei123,
           'und benutzt dafuer das andere Wort')

    print()
    print('124. Eine LANGE stumme Sitzung raeumt auf — eine kurze nicht')
    # ⚠⚠ **Gemeldet am 05.09.2026:** Nach dem Ausloggen ohne Abgabe stand der
    # letzte Auftrag fuer immer auf „laeuft" — auch nachdem danach 162 Minuten
    # gespielt worden war, ohne dass ein einziger Auftrag vorkam. Dazu: „er
    # wurde nicht wieder gemeldet, kann er auch nicht da er weg ist."
    #
    # ⚠⚠ **Der erste Loesungsversuch war FALSCH und wurde gemessen widerlegt.**
    # „Spieler war im Spiel, nannte aber keinen Auftrag" allein haette an 188
    # echten Protokollen ACHT Auftraege geschlossen, die kurz danach wieder
    # auftauchten — kurze Fehlstarts nennen den Auftrag eben doch nicht immer.
    # Erst die Mindestdauer macht die Regel sicher (0 Fehlschliessungen ab
    # einer Stunde, genommen sind 90 Minuten).
    #
    # Diese Pruefung haelt BEIDE Seiten fest. Ohne die zweite waere der alte
    # Fehler nur gegen einen schlimmeren getauscht: Ein faelschlich
    # geschlossener Auftrag ist schlimmer als eine ehrliche Karteileiche.
    import importlib as _il124
    _ml124 = _il124.import_module('scbp.missionslog')

    _wiese124 = tempfile.mkdtemp(prefix='sc-bp-stumm-')
    _altheim124 = os.environ.get('SC_BP_HOME')
    os.environ['SC_BP_HOME'] = _wiese124
    try:
        def _log124(name, zeilen):
            pfad = os.path.join(_wiese124, name)
            with open(pfad, 'w', encoding='utf-8') as d:
                d.write(''.join(zeilen))
            return pfad

        def _zeile124(zeit, text):
            return '<%s.000Z> %s\n' % (zeit, text)

        # Sitzung 1: ein Auftrag wird angenommen und NICHT beendet — genau der
        # Fall „ausgeloggt, ohne abzugeben".
        _annahme124 = _zeile124(
            '2026-09-04T20:31:27',
            '[42] <SHUDEvent_OnNotification> Added notification '
            '"Contract Accepted: Retake Platforms From Nine Tails: "')
        _s1 = _log124('s1.log', [
            _zeile124('2026-09-04T20:00:00',
                      '[CSessionManager::OnClientSpawned] Spawned!'),
            _annahme124,
            _zeile124('2026-09-04T23:00:00', 'Ende der Sitzung'),
        ])
        os.utime(_s1, (1750000000, 1750000000))

        # Sitzung 2a: KURZ (2 Minuten) und ohne Auftrag — ein Fehlstart.
        _kurz = _log124('s2a.log', [
            _zeile124('2026-09-05T10:00:00',
                      '[CSessionManager::OnClientSpawned] Spawned!'),
            _zeile124('2026-09-05T10:02:00', 'Nichts von Belang'),
        ])
        os.utime(_kurz, (1750000400, 1750000400))

        stand = {e['name']: e for e in _ml124.aus_dateien([_s1, _kurz])}
        pruefe(stand.get('Retake Platforms From Nine Tails', {})
               .get('zustand') == _ml124.LAEUFT,
               'ein kurzer Fehlstart beendet KEINEN Auftrag')

        # Sitzung 2b: LANG (3 Stunden) und ohne Auftrag — hier zaehlt das
        # Schweigen.
        _lang = _log124('s2b.log', [
            _zeile124('2026-09-05T10:00:00',
                      '[CSessionManager::OnClientSpawned] Spawned!'),
            _zeile124('2026-09-05T13:00:00', 'Nichts von Belang'),
        ])
        os.utime(_lang, (1750000400, 1750000400))

        stand = {e['name']: e for e in _ml124.aus_dateien([_s1, _lang])}
        pruefe(stand.get('Retake Platforms From Nine Tails', {})
               .get('zustand') == _ml124.VERFALLEN,
               'eine lange Sitzung ohne Auftrag raeumt die Karteileiche ab')

        # ⚠ Und ohne Spawn zaehlt auch eine lange Datei nicht: Ein Log, das
        # weiterlief, ohne dass jemand ins Spiel kam, beweist gar nichts.
        _ohne_spawn = _log124('s2c.log', [
            _zeile124('2026-09-05T10:00:00', 'Start'),
            _zeile124('2026-09-05T13:00:00', 'Nichts von Belang'),
        ])
        os.utime(_ohne_spawn, (1750000400, 1750000400))
        stand = {e['name']: e for e in _ml124.aus_dateien([_s1, _ohne_spawn])}
        pruefe(stand.get('Retake Platforms From Nine Tails', {})
               .get('zustand') == _ml124.LAEUFT,
               'ohne Spawn beweist auch eine lange Datei nichts')

        # Die Grenze selbst — sie ist gemessen, nicht geschaetzt, und darf
        # nicht unbemerkt unter eine Stunde rutschen.
        pruefe(_ml124.SITZUNG_ZAEHLT_SEK >= 3600,
               'die Mindestdauer bleibt bei mindestens einer Stunde (%d s)'
               % _ml124.SITZUNG_ZAEHLT_SEK)
    finally:
        if _altheim124 is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _altheim124
        shutil.rmtree(_wiese124, ignore_errors=True)

    print()
    print('125. Spielzeit: fortgeschrieben, ohne doppelt zu zaehlen')
    # ⚠⚠ **Gewuenscht am 05.09.2026**, mitsamt der Begruendung, warum es eine
    # eigene Datei braucht: „dazu muesste es aber auch eine Datenbank geben die
    # Fortgeschrieben wird (die muesste dann auch Exportierbar sein bei
    # Systemumzug oder neuinstallation)".
    #
    # Genau so: Star Citizen hebt seine Protokolle nur begrenzt auf — gemessen
    # decken 188 Sicherungen 88 Tage ab. Wer die Spielzeit allein aus den
    # vorhandenen Logs rechnet, bekommt jeden Monat eine kleinere
    # Vergangenheit.
    import time as _t125
    from scbp import spielzeit as _sz125

    _wiese125 = tempfile.mkdtemp(prefix='sc-bp-zeit-')
    _altheim125 = os.environ.get('SC_BP_HOME')
    os.environ['SC_BP_HOME'] = _wiese125
    try:
        def _log125(name, von, bis, mit_spawn=True):
            pfad = os.path.join(_wiese125, name)
            zeilen = ['<%s.000Z> Start\n' % von]
            if mit_spawn:
                zeilen.append('<%s.000Z> [CSessionManager::OnClientSpawned] '
                              'Spawned!\n' % von)
            zeilen.append('<%s.000Z> Ende\n' % bis)
            with open(pfad, 'w', encoding='utf-8') as d:
                d.write(''.join(zeilen))
            return pfad

        _a125 = _log125('a.log', '2026-09-01T10:00:00', '2026-09-01T12:00:00')
        _b125 = _log125('b.log', '2026-09-02T10:00:00', '2026-09-02T11:30:00')

        _sz125.nachtragen([_a125, _b125])
        # ⚠⚠ **`mit_laufender=False`, sonst misst die Pruefung den Rechner,
        # auf dem sie laeuft.** `pfade.spiel_ordner()` faellt auf eine Suche
        # zurueck, wenn kein gueltiger Pfad eingetragen ist — auf einem
        # Rechner MIT Star Citizen findet es das echte Spiel, und wenn dort
        # gerade gespielt wird, zaehlt die laufende Sitzung mit. Genau so
        # meldete diese Pruefung „3 h 31 min statt 3 h 30 min": ein Fehler in
        # der Pruefung, nicht im Programm.
        pruefe(_sz125.gesamt(mit_laufender=False) == 3600 * 3.5,
               'zwei Sitzungen ergeben 3 h 30 min (%s)'
               % _sz125.als_text(_sz125.gesamt(mit_laufender=False)))

        # ⚠ Der zweite Lauf darf NICHTS dazuzaehlen. Sonst waechst die Zahl bei
        # jedem Programmstart, und niemand merkt es, bis sie absurd ist.
        _vorher125 = _sz125.gesamt(mit_laufender=False)
        _neu125 = _sz125.nachtragen([_a125, _b125])
        pruefe(_neu125 == 0
               and _sz125.gesamt(mit_laufender=False) == _vorher125,
               'ein zweiter Durchlauf zaehlt nichts doppelt')

        # Ein Start, der nie ins Spiel kam, ist keine Spielzeit.
        _c125 = _log125('c.log', '2026-09-03T10:00:00', '2026-09-03T14:00:00',
                        mit_spawn=False)
        _sz125.nachtragen([_c125])
        pruefe(_sz125.gesamt(mit_laufender=False) == _vorher125,
               'ein Start ohne Spawn zaehlt nicht als Spielzeit')

        # Kurze Sitzungen zaehlen dagegen mit — eine Grenze waere eine
        # Behauptung ueber „richtiges" Spielen.
        _d125 = _log125('d.log', '2026-09-04T10:00:00', '2026-09-04T10:02:00')
        _sz125.nachtragen([_d125])
        pruefe(_sz125.gesamt(mit_laufender=False) == _vorher125 + 120,
               'auch zwei Minuten zaehlen mit')

        # ⚠ Ueberlappungen verschmelzen, statt sich zu addieren. In den echten
        # Daten gab es genau so einen Fall.
        _sz125.sichern({'format': _sz125.FORMAT, 'sitzungen': [
            {'von': 1000, 'bis': 1000 + 3600},
            {'von': 1000 + 1800, 'bis': 1000 + 7200},
        ]})
        pruefe(_sz125.gesamt(mit_laufender=False) == 7200,
               'ueberlappende Zeitraeume werden zusammengefuehrt (%s)'
               % _sz125.als_text(_sz125.gesamt(mit_laufender=False)))

        # Ein Ausreisser (verstellte Uhr, zwei Laeufe in einer Datei) darf die
        # Summe nicht verderben.
        _e125 = _log125('e.log', '2026-09-05T10:00:00', '2026-09-08T10:00:00')
        _stand125 = _sz125.gesamt(mit_laufender=False)
        _sz125.nachtragen([_e125])
        pruefe(_sz125.gesamt(mit_laufender=False) == _stand125,
               'eine 72-Stunden-Sitzung wird verworfen statt gezaehlt')

        for _s125, _soll125 in ((0, '0 min'), (59, '0 min'), (60, '1 min'),
                                (3600, '1 h 00 min'), (3660, '1 h 01 min')):
            pruefe(_sz125.als_text(_s125) == _soll125,
                   '%d s steht als „%s"' % (_s125, _soll125))

        # ⚠⚠ **Die Datei darf NICHT auf der Nachladbar-Liste stehen.** Sie
        # laesst sich nicht neu beschaffen, sobald die Logs rotiert sind —
        # stuende sie dort, waere die ganze aufgezeichnete Vergangenheit beim
        # Rechnerwechsel weg, und zwar lautlos.
        from scbp import sicherung as _si125
        pruefe(not any(_sz125.DATEI in eintrag
                       for eintrag in _si125.NACHLADBAR),
               'die Spielzeit gilt NICHT als nachladbar — sie kommt in die '
               'Sicherung')
    finally:
        if _altheim125 is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _altheim125
        shutil.rmtree(_wiese125, ignore_errors=True)

    print()
    print('126. Ein Klick ins Leere beendet die Eingabe — ueberall')
    # ⚠⚠ **Gemeldet am 05.09.2026, mit dem entscheidenden Zusatz:** „das
    # vergisst du jedesmal aufs neue wo man text eingeben kann." Zu Recht:
    # Erst blieb das Auswahlfeld beim Klick ins Leere offen, dann uebernahm
    # das Meldungsfeld seinen Text nicht, dann das Namensfeld. Dreimal
    # dieselbe Ursache, dreimal einzeln geflickt.
    #
    # Die Ursache: Tk gibt den Fokus nur ab, wenn ihn ein anderes
    # FOKUSSIERBARES Bedienelement uebernimmt. Ein Klick auf eine Flaeche
    # oder Beschriftung tut das nicht — der Zeiger blinkt weiter im Feld, und
    # jedes `<FocusOut>`, das den Text uebernehmen soll, feuert nie.
    #
    # Diese Pruefung haelt die Loesung am FENSTER fest, nicht an einer Seite.
    # Sonst gilt sie beim naechsten neuen Eingabefeld wieder nicht.
    import tkinter as tk126
    _w126 = _wurzel()
    try:
        _w126.deiconify()
        _w126.geometry('1200x900')
        from scbp import hauptfenster as _hf126
        _f126 = _hf126.Hauptfenster(_w126, version='0.0.0-pruefung')
        _f126.oeffnen('diagnose')
        _w126.update_idletasks()

        pruefe(hasattr(_hf126.Hauptfenster, '_klick_ins_leere_einrichten'),
               'das Fenster kennt die Regel (und nicht nur eine Seite)')

        _alle126 = []

        def _sammeln126(knoten):
            for kind in knoten.winfo_children():
                _alle126.append(kind)
                _sammeln126(kind)

        _sammeln126(_f126.seiten['diagnose'])
        _texte126 = [w for w in _alle126 if isinstance(w, tk126.Text)]
        _eingabe126 = [w for w in _texte126 if int(w.cget('height')) == 4]
        _kasten126 = [w for w in _texte126 if w not in _eingabe126]
        _namen126 = [w for w in _alle126 if isinstance(w, tk126.Entry)]
        _label126 = [w for w in _alle126 if isinstance(w, tk126.Label)]

        pruefe(bool(_eingabe126 and _kasten126 and _namen126),
               'Meldungsfeld, Namensfeld und Berichtskasten sind da')

        if _eingabe126 and _kasten126 and _namen126:
            def _ins_leere126():
                ziel = _label126[0] if _label126 else _f126.seiten['diagnose']
                ziel.event_generate('<Button-1>', x=2, y=2)
                _w126.update()
                _w126.update_idletasks()

            # ⚠ Der Fokus muss ERZWUNGEN werden. Unter Xvfb vergibt kein
            # Fenstermanager ihn, `focus_get()` gibt dann None — und die
            # Pruefung wuerde „nicht im Feld" melden, ohne je drin gewesen zu
            # sein. Genau so lief der erste Anlauf ins Leere.
            _w126.focus_force()
            _w126.update()
            _eingabe126[0].focus_set()
            _eingabe126[0].delete('1.0', 'end')
            _eingabe126[0].insert('1.0', 'Pruefsatz eins')
            _w126.update_idletasks()
            pruefe(_w126.focus_get() is _eingabe126[0],
                   'der Fokus laesst sich ins Meldungsfeld setzen')

            _ins_leere126()
            pruefe(_w126.focus_get() is not _eingabe126[0],
                   'ein Klick ins Leere nimmt den Fokus aus dem Meldungsfeld')
            pruefe('Pruefsatz eins' in _kasten126[0].get('1.0', 'end-1c'),
                   'und der eingetippte Satz steht danach im Bericht')

            _w126.focus_force()
            _w126.update()
            _namen126[0].focus_set()
            _namen126[0].delete(0, 'end')
            _namen126[0].insert(0, 'PruefMelder')
            # ⚠⚠ **Ein voller `update()`, nicht nur `update_idletasks()`.**
            # Der Fokuswechsel ist ein Ereignis; ohne Durchlauf sitzt er noch
            # nicht, und der Klick danach nimmt einen Fokus weg, der nie da
            # war — dann feuert auch kein `<FocusOut>`, und der Name bleibt
            # draussen. Genau daran scheiterte der erste Anlauf dieser
            # Pruefung, waehrend das Programm richtig arbeitete.
            _w126.update()
            pruefe(_w126.focus_get() is _namen126[0],
                   'der Fokus laesst sich ins Namensfeld setzen')
            _ins_leere126()
            pruefe(_w126.focus_get() is not _namen126[0],
                   'ein Klick ins Leere nimmt den Fokus aus dem Namensfeld')

            # ⚠⚠ **Warum hier NICHT geprueft wird, ob der Name im Bericht
            # landet.** Nicht aus Bequemlichkeit — die Ursache ist gemessen:
            # Das Namensfeld haengt an einer `tk.StringVar()` ohne
            # ausdrueckliche Wurzel, und die bindet sich an die ERSTE
            # Tk-Instanz des Prozesses. Im Selbsttest ist die laengst
            # zerstoert (jede Pruefung baut ihr eigenes Fenster), also liefert
            # `.get()` dort nichts, und `melder_uebernehmen` schreibt eine
            # leere Zeichenkette.
            #
            # Im Programm gibt es genau EINE Wurzel; dort tritt das nicht auf.
            # In einem eigenen Lauf am 05.09.2026 gegengeprueft, mit echtem
            # Klick auf eine Beschriftung, in beiden Reihenfolgen — der Name
            # kam jedes Mal im Bericht an.
            #
            # Der Meldungstext oben deckt dieselbe Kette ab (er haengt nicht
            # an einer StringVar). Eine Zeile, die je nach Wurzel gruen oder
            # rot ist, waere schlimmer als keine: Sie wuerde irgendwann
            # ignoriert.
            pruefe('<FocusOut>' in (_namen126[0].bind() or ()),
                   'am Namensfeld haengt die Uebernahme am Fokusverlust')

            # ⚠ Gegenprobe: Ein Klick INS Feld darf den Fokus nicht nehmen.
            # Ohne sie waere die Regel „Fokus immer weg" ebenso gruen — und
            # tippen unmoeglich.
            _w126.focus_force()
            _w126.update()
            _eingabe126[0].focus_set()
            _w126.update_idletasks()
            _eingabe126[0].event_generate('<Button-1>', x=5, y=5)
            _w126.update()
            pruefe(_w126.focus_get() is _eingabe126[0],
                   'ein Klick ins Feld selbst laesst den Fokus dort')
    finally:
        try:
            _w126.destroy()
        except tk126.TclError:
            pass

    print()
    print('127. Rufpunkte in JEDEN Auftrag, hervorgehoben')
    # ⚠⚠ **Gemeldet am 05.09.2026 ueber Bushwick4712:** In einem Auftrag ohne
    # Bauplaene standen keine Rufpunkte, waehrend eine fremde Uebersetzung sie
    # dort anzeigte. Nachgemessen an den Vertragsdaten: **816 von 818**
    # Auftraegen bringen Rufpunkte und Abklingzeit mit — bedient wurden aber
    # nur die **367** mit eigenem Beschreibungsblock. Die uebrigen **449**
    # gingen verloren, obwohl die Daten dalagen.
    #
    # Dazu die Hervorhebung: „Mach die XP blau geschrieben … damit allgemein
    # spieler es schneller sehen." Der Melder hatte sie uebersehen, weil sie
    # unauffaellig mitten im Text standen.
    from scbp import injektion as _inj127

    pruefe(_inj127.FARBE_AUF == '<EM4>',
           'die Hervorhebung ist die, die das Spiel benutzt')
    pruefe(_inj127._blau('# Rufpunkte: 5') ==
           '<EM4># Rufpunkte: 5</EM4>',
           'eine Zeile wird hervorgehoben')
    # ⚠ Doppelte Auszeichnung zeigt das Spiel als TEXT an — aus zwei <EM4>
    # wird kein kraeftigeres Blau, sondern ein sichtbares „<EM4>".
    pruefe(_inj127._blau('<EM4>schon da</EM4>') == '<EM4>schon da</EM4>',
           'und keine zweite darueber')

    # Dubletten: Was schon dasteht, kommt nicht noch einmal — auch dann nicht,
    # wenn es beim letzten Lauf noch ungefaerbt war.
    _e127 = {'contractInfo': '# Zu erwartende Rufpunkte: 50 XP'}
    pruefe(_inj127._angabenzeilen(_e127, '') ==
           ['<EM4># Zu erwartende Rufpunkte: 50 XP</EM4>'],
           'in leeren Text wird eingesetzt')
    pruefe(_inj127._angabenzeilen(
        _e127, 'Text # Zu erwartende Rufpunkte: 50 XP') == [],
        'was schon dasteht, kommt nicht doppelt')
    pruefe(_inj127._angabenzeilen(
        _e127, 'Text <EM4># Zu erwartende Rufpunkte: 50 XP</EM4>') == [],
        'auch wenn es bereits hervorgehoben ist')

    # ⚠⚠ **Fremder Text wird nicht verdoppelt.** MrKraken StarStrings schreibt
    # eine eigene Reputationszeile; wo eine steht, kommt keine zweite dazu.
    pruefe(_inj127._hat_angaben('Reputation Awarded: 50'),
           'eine fremde Reputationszeile wird erkannt')
    pruefe(_inj127._hat_angaben('<EM4># Zu erwartende Rufpunkte: 5</EM4>'),
           'die eigene ebenso')
    pruefe(not _inj127._hat_angaben('Ein ganz gewoehnlicher Auftragstext'),
           'ein gewoehnlicher Text gilt NICHT als schon versorgt')

    print()
    print('128. Wem der Auftrag Ruf bringt — und welcher Art')
    # ⚠⚠ **Gewuenscht am 05.09.2026:** „auf SCMDB sieht man auch ob es Standing
    # oder Rep bekommt, das muss auf jeden fall mit in den Questtext." Als
    # Beispiele genannt: Headhunters und Citizens For Prosperity, „da gibt es
    # beides".
    #
    # Die Vertragsdaten geben das NICHT her — gemessen an allen 818 Eintraegen
    # kennen sie die Rufpunkte nur als Zahl, ohne Partei und ohne Art. Deshalb
    # eine zweite Quelle (scmdb.net) und ein eigenes Modul.
    from scbp import auftragsruf as _ar128

    # ⚠ Der Schluesselvergleich ist der Angelpunkt: scmdb schreibt `@` davor
    # und eine andere Gross-/Kleinschreibung. Ohne Angleichung gibt es NULL
    # Treffer — gemessen, bevor es gebaut wurde.
    pruefe(_ar128._schluessel('@Shubin_Nyx_M_Title_001') ==
           'shubin_nyx_m_title_001',
           'der scmdb-Schluessel wird angeglichen')

    _roh128 = {
        'contracts': [
            {'titleLocKey': '@Headhunters_Test_title_001',
             'factionRewardsIndex': 0},
            {'titleLocKey': '@CFP_Test_title_001',
             'factionRewardsIndex': 1},
            {'titleLocKey': '@Ohne_Test_title_001',
             'factionRewardsIndex': 2},
        ],
        'factionRewardsPools': [
            [{'factionGuid': 'f1', 'scopeGuid': 's1', 'amount': 150}],
            # ⚠ Zwei Parteien in EINEM Auftrag — genau der genannte Fall.
            [{'factionGuid': 'f2', 'scopeGuid': 's1', 'amount': 100},
             {'factionGuid': 'f2', 'scopeGuid': 's2', 'amount': 50}],
            [],
        ],
        'factions': {'f1': {'name': 'Headhunters'},
                     'f2': {'name': 'Citizens For Prosperity'}},
        'scopes': {'s1': {'displayName': 'Standing'},
                   's2': {'displayName': 'Affinity'}},
    }
    _tab128 = {'format': _ar128.FORMAT, 'version': 'probe',
               'auftraege': _ar128.aufbereiten(_roh128)}

    pruefe(len(_tab128['auftraege']) == 2,
           'nur Auftraege mit Rufeintrag kommen in die Tabelle (%d)'
           % len(_tab128['auftraege']))

    _z128 = _ar128.zeile('headhunters_test_title_001', 'Ruf', _tab128)
    pruefe(_z128 == '# Ruf: Headhunters +150 Standing',
           'eine Partei: %r' % _z128)

    _z128 = _ar128.zeile('cfp_test_title_001', 'Ruf', _tab128)
    pruefe(_z128 == ('# Ruf: Citizens For Prosperity +100 Standing, '
                     'Citizens For Prosperity +50 Affinity'),
           'zwei Parteien in einem Auftrag: %r' % _z128)

    pruefe(_ar128.zeile('ohne_test_title_001', 'Ruf', _tab128) == '',
           'ohne Rufeintrag bleibt die Zeile leer')
    pruefe(_ar128.zeile('gibtesnicht', 'Ruf', _tab128) == '',
           'ein unbekannter Auftrag bekommt nichts erfunden')

    # ⚠ Und die Verbindung zur Injektion: Ohne sie stuende das Modul da und
    # niemand riefe es.
    from scbp import injektion as _inj128
    _e128 = {'titleLocKey': 'headhunters_test_title_001',
             'contractInfo': '# Zu erwartende Rufpunkte: 150 XP'}
    _zeilen128 = _inj128._angabenzeilen(_e128, '', {'ruf_bei': 'Ruf'},
                                        _tab128)
    pruefe(any('Headhunters +150 Standing' in z for z in _zeilen128),
           'die Injektion setzt die Ruf-Zeile ein')
    pruefe(all(z.startswith(_inj128.FARBE_AUF) for z in _zeilen128),
           'und hebt sie hervor wie die uebrigen Angaben')
    # Dublettenschutz: Steht schon eine Ruf-Zeile da, kommt keine zweite —
    # auch wenn sich die Zahl geaendert hat.
    _zeilen128 = _inj128._angabenzeilen(
        _e128, '# Ruf: Headhunters +99 Standing',
        {'ruf_bei': 'Ruf'}, _tab128)
    pruefe(not any('# Ruf:' in z for z in _zeilen128),
           'eine vorhandene Ruf-Zeile wird nicht verdoppelt')

    print()
    print('129. Die neuere Ausfuhr von scmdb.net wird erkannt')
    # ⚠⚠ **Gemeldet am 05.09.2026:** Eine Datei von einem Mitspieler wurde mit
    # „Diese Datei kenne ich nicht" abgewiesen. Zu Recht — scmdb hat das Format
    # gewechselt, und wir kannten nur das alte:
    #
    #     alt:  {"exportSchemaVersion": …, "blueprints": [{"productName", "ts"}]}
    #     neu:  {"version": 3, "blueprints": [{"tag", "name", "completed"}]}
    #
    # An der echten Datei gemessen: 349 Eintraege, 348 davon im Katalog
    # wiedergefunden.
    from scbp import importieren as _imp129

    _neu129 = {'version': 3, 'blueprints': [
        {'tag': 'BP_CRAFT_X', 'name': 'Omnisky VI Cannon', 'completed': True},
        {'tag': 'BP_CRAFT_Y', 'name': 'Nur beobachtet', 'completed': False},
    ], 'missions': [{'hash': 'a', 'name': 'Auftrag', 'completed': True}]}
    pruefe(_imp129.erkennen(_neu129) == 'scmdb2',
           'das neue Format wird erkannt')

    # ⚠ Das ALTE Format darf dabei nicht verloren gehen — es gibt Nutzer mit
    # aelteren Ausfuhren, und eine Erkennung, die das eine gegen das andere
    # tauscht, verschiebt den Fehler nur.
    pruefe(_imp129.erkennen(
        {'exportSchemaVersion': 1,
         'blueprints': [{'productName': 'Alt', 'ts': 1}]}) == 'scmdb',
        'das alte scmdb-Format weiterhin auch')
    pruefe(_imp129.erkennen(
        {'blueprints': [{'productName': 'B', 'receivedAt': 1}]}) == 'basetool',
        'und das Basetool-Format')
    pruefe(_imp129.erkennen(
        {'blueprints': [{'key': 'irgendwas'}]}) == 'launcher',
        'und das des Launchers')

    _wiese129 = tempfile.mkdtemp(prefix='sc-bp-imp-')
    try:
        _datei129 = os.path.join(_wiese129, 'scmdb-tracking.json')
        with open(_datei129, 'w', encoding='utf-8') as _d129:
            json.dump(_neu129, _d129)
        _art129, _eintraege129 = _imp129.lesen(_datei129)
        pruefe(_art129 == 'scmdb2', 'die Datei wird als solche gelesen')
        # ⚠⚠ **Nur `completed` zaehlt.** Die Ausfuhr enthaelt auch Bauplaene,
        # die jemand nur beobachtet — waeren die dabei, staende die halbe
        # Datenbank im Bestand und das Werkzeug meldete nie wieder einen Fund.
        pruefe([e['name'] for e in _eintraege129] == ['Omnisky VI Cannon'],
               'nur erledigte Bauplaene kommen mit (%r)'
               % [e['name'] for e in _eintraege129])
    finally:
        shutil.rmtree(_wiese129, ignore_errors=True)

    print()
    print('130. Zuruecksetzen sagt VORHER, was es kostet')
    # ⚠⚠ **Am 05.09.2026 hat ein Melder seinen Bestand von 232 auf 3 gesetzt.**
    # Die Warnung war da und sachlich richtig („was aelter ist als deine
    # Protokolle, kommt nicht zurueck") — sie nannte nur keine Zahlen. Bei ihm
    # gaben 221 Protokolle ganze 3 Bauplaene her; 229 waren weg.
    #
    # Wer „232 → 3" liest, bricht ab. Wer einen Satz liest, klickt weiter.
    from scbp import bestand as _b130

    _wiese130 = tempfile.mkdtemp(prefix='sc-bp-reset-')
    _altheim130 = os.environ.get('SC_BP_HOME')
    os.environ['SC_BP_HOME'] = _wiese130
    try:
        _d130 = _b130.leer()
        for _i130 in range(3):
            _b130.hinzufuegen(_d130, 'Aus Log %d' % _i130, 'log')
        for _i130 in range(229):
            _b130.hinzufuegen(_d130, 'Vom Launcher %d' % _i130, 'launcher')
        _b130.speichern(_d130)

        _g130 = _b130.laden()
        _q130 = _b130.nach_quelle(_g130)
        _bleibt130 = _q130.get('log', 0) + _q130.get('nachlese', 0)
        pruefe(len(_g130['bauplaene']) == 232,
               'die Lage ist nachgestellt (232 Bauplaene)')
        # ⚠ Nur was aus Protokollen stammt, kommt beim Neuaufbau zurueck —
        # Launcher, Import und Handeintraege nicht.
        pruefe(_bleibt130 == 3,
               'aus den Protokollen kaemen 3 zurueck (%d)' % _bleibt130)

        pruefe('s_be_reset_zahlen' in sprache.TEXTE,
               'es gibt einen Text, der die Zahlen nennt')
        if 's_be_reset_zahlen' in sprache.TEXTE:
            for _sp130 in (0, 1):
                pruefe(sprache.TEXTE['s_be_reset_zahlen'][_sp130].count('%d')
                       == 3,
                       'er nennt DREI Zahlen (haben, zurueck, verloren) [%d]'
                       % _sp130)

        # ⚠ Gegenprobe: Ein Bestand nur aus Protokollen verliert nichts —
        # sonst waere die Warnung eine Panikmache, die man wegklickt.
        _d130b = _b130.leer()
        for _i130 in range(5):
            _b130.hinzufuegen(_d130b, 'Nur Log %d' % _i130, 'log')
        _b130.speichern(_d130b)
        _g130b = _b130.laden()
        _q130b = _b130.nach_quelle(_g130b)
        pruefe((_q130b.get('log', 0) + _q130b.get('nachlese', 0))
               == len(_g130b['bauplaene']),
               'ein reiner Protokoll-Bestand verliert nichts')

        # Und die Anzeige muss die Zahlen wirklich benutzen.
        _sei130 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                       encoding='utf-8').read()
        pruefe("s_be_reset_zahlen" in _sei130,
               'die Warnfrage benutzt den Text')
    finally:
        if _altheim130 is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _altheim130
        shutil.rmtree(_wiese130, ignore_errors=True)

    print()
    print('131. Die Protokolle der Nachbarkanaele kommen mit')
    # ⚠⚠ **Am 05.09.2026 gemeldet:** Nach einem Wechsel von HOTFIX auf LIVE
    # gaben 221 Protokolle nur DREI Bauplaene her — die uebrigen lagen im
    # HOTFIX-Ordner, den der Watcher nie ansah. Dazu: „er hat im HOTFIX noch
    # alle logs liegen … koennen wir die aus allen Ordner also Live und HOTFIX
    # in die log durchsuchung einbeziehen?"
    #
    # Es ist dieselbe Person mit demselben Spielstand; nur der Kanal ist ein
    # anderer. Ein Kanalwechsel darf die Vorgeschichte nicht kosten.
    from scbp import pfade as _pf131

    _wiese131 = tempfile.mkdtemp(prefix='sc-bp-kanal-')
    try:
        _sc131 = os.path.join(_wiese131, 'Roberts Space Industries',
                              'StarCitizen')

        def _kanal131(name, anzahl):
            ordner = os.path.join(_sc131, name, 'logbackups')
            os.makedirs(ordner)
            for i in range(anzahl):
                with open(os.path.join(ordner, 'G %s %02d.log' % (name, i)),
                          'w', encoding='utf-8') as d:
                    d.write('<2026-09-01T10:00:00.000Z> Probe\n')
            return os.path.join(_sc131, name)

        _live131 = _kanal131('LIVE', 3)
        _kanal131('HOTFIX', 221)
        _kanal131('PTU', 7)

        _gefunden131 = _pf131.log_sicherungen(_live131)
        # ⚠⚠ **PTU bleibt DRAUSSEN** — 224, nicht 231. Die Testumgebungen
        # laufen auf eigenen Spielstaenden; dort freigeschaltete Bauplaene hat
        # man auf LIVE nicht. Sie mitzulesen wuerde einen Bestand behaupten,
        # den es nicht gibt — und ein zu viel eingetragener Bauplan ist
        # schlimmer als ein fehlender: Man plant damit und steht ohne da.
        # Am 05.09.2026 richtiggestellt, nachdem der erste Anlauf alle Kanaele
        # zusammenwarf.
        pruefe(len(_gefunden131) == 224,
               'LIVE (3) + HOTFIX (221) = 224, PTU bleibt draussen (%d)'
               % len(_gefunden131))
        pruefe(not any(os.sep + 'PTU' + os.sep in p for p in _gefunden131),
               'kein einziges PTU-Protokoll ist dabei')
        pruefe(len(set(_gefunden131)) == len(_gefunden131),
               'und nichts doppelt')
        # ⚠ Wer SELBST auf PTU spielt, bekommt sein eigenes Protokoll — nur
        # Nachbarn werden gefiltert, nicht der eingetragene Ordner.
        _ptu131 = os.path.join(_sc131, 'PTU')
        pruefe(len(_pf131.log_sicherungen(_ptu131)) == 7,
               'wer auf PTU spielt, bekommt seine eigenen 7 (%d)'
               % len(_pf131.log_sicherungen(_ptu131)))

        # ⚠⚠ **Gegenprobe: NICHT wildern.** Ohne sie waere „nimm alles aus der
        # Nachbarschaft" ebenso gruen — und wer sein Spiel woanders liegen hat,
        # bekaeme fremde Ordner mitgelesen.
        _fremd131 = os.path.join(_wiese131, 'IrgendwoAnders')
        os.makedirs(os.path.join(_fremd131, 'logbackups'))
        with open(os.path.join(_fremd131, 'logbackups', 'a.log'), 'w') as _d:
            _d.write('x')
        pruefe(len(_pf131.log_sicherungen(_fremd131)) == 1,
               'ein Ordner, der kein Kanal ist, bekommt nur sich selbst')

        # Und ein Ordner, der zwar LIVE heisst, aber nicht unter StarCitizen
        # liegt — der Name allein reicht nicht.
        _falsch131 = os.path.join(_wiese131, 'Sonstwo', 'LIVE')
        os.makedirs(os.path.join(_falsch131, 'logbackups'))
        with open(os.path.join(_falsch131, 'logbackups', 'a.log'), 'w') as _d:
            _d.write('x')
        os.makedirs(os.path.join(_wiese131, 'Sonstwo', 'HOTFIX', 'logbackups'))
        pruefe(len(_pf131.log_sicherungen(_falsch131)) == 1,
               'ein LIVE ausserhalb von StarCitizen zieht keine Nachbarn')
    finally:
        shutil.rmtree(_wiese131, ignore_errors=True)

    print()
    print('132. Der scmdb-Export hat das Format, das scmdb heute schreibt')
    # ⚠⚠ Am 06.09.2026 aufgefallen, waehrend am IMPORT gearbeitet wurde:
    # scmdb.net exportiert inzwischen `version: 3` mit `tag`/`name`/`url`/
    # `completed`/`favorite`. Unser Export schrieb weiter `exportSchemaVersion:
    # 1` mit `productName` und `ts` — abgelesen an ihrem alten Log-Watcher
    # v0.1.9. Der Import wurde angepasst, der Export nicht: eine Richtung
    # angefasst, die andere vergessen.
    #
    # Der TAG ist bei ihnen der Schluessel, nicht der Name. Ein Export ohne
    # Tags waere syntaktisch richtig und trotzdem wertlos.
    from scbp import export as _ex132

    # ⚠ Eigene Minimaldaten statt der echten Rezeptdaten: Die liegen im
    # Ablageordner und fehlen im Wegwerf-Ordner des Selbsttests. Eine
    # Pruefung, die sich dann ueberspringt, prueft nichts (siehe Pruefung 67).
    _tags132 = {'omnisky vi cannon': 'BP_CRAFT_AMRS_LaserCannon_S2'}
    _bestand132 = {'bauplaene': {
        'a': {'name': 'Omnisky VI Cannon'},
        'b': {'name': 'Kennt-scmdb-nicht'},
    }}
    _doc132 = _ex132.fuer_scmdb(_bestand132, version='9.9.9', tags=_tags132)

    pruefe(_doc132.get('version') == 3,
           'der Umschlag traegt version 3 (ist: %r)' % _doc132.get('version'))
    pruefe('exportSchemaVersion' not in _doc132,
           'das alte Feld exportSchemaVersion ist weg')

    _bp132 = {b['name']: b for b in _doc132.get('blueprints') or []}
    pruefe(len(_bp132) == 2, 'beide Bauplaene sind dabei')

    _omni132 = _bp132.get('Omnisky VI Cannon') or {}
    pruefe(_omni132.get('tag') == 'BP_CRAFT_AMRS_LaserCannon_S2',
           'der Tag steht am Bauplan (ist: %r)' % _omni132.get('tag'))
    pruefe(_omni132.get('completed') is True, 'completed ist gesetzt')
    pruefe(sorted(_omni132) == ['completed', 'favorite', 'name', 'tag', 'url'],
           'die Felder sind genau die von scmdb (sind: %s)' % sorted(_omni132))
    pruefe('productName' not in _omni132 and 'ts' not in _omni132,
           'die alten Felder productName und ts sind weg')

    # ⚠ Wer keinen Tag hat, faellt trotzdem nicht heraus — sonst verschwaenden
    # vier Bauplaene stillschweigend aus einem Bestand von 413.
    pruefe('Kennt-scmdb-nicht' in _bp132,
           'ein Bauplan ohne Tag geht trotzdem mit')
    pruefe('url' not in (_bp132.get('Kennt-scmdb-nicht') or {}),
           'ohne Tag wird keine Adresse erfunden')

    # ⚠ Gegenprobe: Wuerde die Pruefung auch anschlagen? Ein Export nach dem
    # ALTEN Muster muss hier durchfallen — sonst ist alles oben nur Deko.
    _alt132 = {'exportSchemaVersion': 1,
               'blueprints': [{'productName': 'Omnisky VI Cannon',
                               'ts': 1756000000}]}
    _durchgefallen132 = (_alt132.get('version') != 3
                         or 'exportSchemaVersion' in _alt132
                         or 'productName' in (_alt132['blueprints'][0]))
    pruefe(_durchgefallen132,
           'Gegenprobe: das alte Format faellt hier durch')

    # ⚠ Und ohne Rezeptdaten? Der Export darf nicht am Netz haengen — er
    # laeuft dann eben ohne Tags weiter, statt zu scheitern.
    _leer132 = _ex132.fuer_scmdb(_bestand132, version='9.9.9', tags={})
    pruefe(len(_leer132.get('blueprints') or []) == 2,
           'ohne Rezeptdaten laeuft der Export trotzdem durch')

    print()
    print('133. Ein Auftrags-Ende ohne Titel wird nicht weggeworfen')
    # ⚠⚠ Am 06.09.2026 gemeldet: Ein abgebrochener Auftrag stand im
    # Auftrags-Protokoll richtig als „abgebrochen" und im Overlay weiter als
    # laufend. Beim Abbruch schreibt das Spiel nur:
    #
    #     <EndMission> … MissionId[7dc679f3-…] CompletionType[Abandon]
    #
    # Kein Titel. Der Watcher warf jedes titellose Ereignis weg, bevor
    # `beendet_welchen` gefragt wurde — und deren dritter Schritt haette es
    # ueber die MissionId aufgeloest.
    from scbp import auftraege as _au133

    _echt133 = (
        '<2026-09-05T22:20:29.057Z> [Notice] <SHUDEvent_OnNotification> Added'
        ' notification "Auftrag angenommen: Bounty Assignment: Kosami Nordquist'
        ' (HRT) <EM4>[BP!]</EM4>: " [2] to queue. New queue size: 1, MissionId:'
        ' [7dc679f3-cb7b-4d86-b579-57f2aaad6a42], ObjectiveId: []'
        ' [Team_CoreGameplayFeatures][Missions][Comms]\n'
        '<2026-09-05T22:20:35.430Z> [Notice] <EndMission> Ending mission for'
        ' player. MissionId[7dc679f3-cb7b-4d86-b579-57f2aaad6a42]'
        ' Player[Xharig] PlayerId[207671730209] CompletionType[Abandon]'
        ' Reason[Player left] [Team_MissionFeatures][Missions]\n')

    _ev133 = list(_au133.ereignisse_aus_text(_echt133))
    pruefe(len(_ev133) == 2, 'beide Ereignisse werden gelesen')
    pruefe(_ev133[1][1] == '' and _ev133[1][2],
           'das Ende kommt titellos, aber mit MissionId')

    def _lauf133(wirft_titellose_weg):
        """Der Ablauf aus `sc_bp_watcher`, auf das Noetige eingedampft."""
        offen, missionen = {}, {}
        for _annahme, _titel, _mid, _oid in _ev133:
            _rein = _au133.sauber(_titel)
            if _annahme:
                if not _rein:
                    continue
                offen[_rein] = _titel
                if _mid:
                    missionen[_mid] = _rein
                continue
            if wirft_titellose_weg:
                if not _rein:
                    continue
            elif not _rein and not _mid:
                continue
            _weg = _au133.beendet_welchen(_rein, _mid, _oid, offen, missionen)
            if _weg is not None:
                offen.pop(_weg, None)
        return offen

    pruefe(not _lauf133(False),
           'der abgebrochene Auftrag verschwindet aus der Leiste')
    # ⚠ Gegenprobe: Mit dem alten Verhalten muss er stehenbleiben — sonst
    # prueft die Zeile darueber nichts.
    pruefe(bool(_lauf133(True)),
           'Gegenprobe: alt blieb er als laufend stehen')

    # ⚠ Und es wird nichts geraten: ohne Titel UND ohne Kennung passiert nichts.
    _blind133 = list(_ev133)
    _blind133[1] = (False, '', '', '')
    _offen133 = {'Irgendein Auftrag': 'Irgendein Auftrag'}
    for _a, _t, _m, _o in _blind133[1:]:
        _r = _au133.sauber(_t)
        if not _r and not _m:
            continue
        _offen133.pop(_au133.beendet_welchen(_r, _m, _o, _offen133, {}), None)
    pruefe(len(_offen133) == 1,
           'ohne Titel und ohne Kennung wird nichts geraeumt')

    print()
    print('134. Ein gescheiterter Auftrag gilt nicht als abgeschlossen')
    # ⚠⚠ Am 06.09.2026 aufgefallen: Das Spiel kennt vier Ausgaenge, der Watcher
    # wertete nur `Abandon` aus — `Fail` und `Deactivate` fielen unter
    # „abgeschlossen". An einem gewachsenen Protokoll waren das **52**
    # gescheiterte Auftraege, die gruen als Erfolg dastanden.
    from scbp import missionslog as _ml134

    pruefe(_ml134._zustand_zu('Complete') == _ml134.ABGESCHLOSSEN,
           'Complete bleibt abgeschlossen')
    pruefe(_ml134._zustand_zu('Abandon') == _ml134.ABGEBROCHEN,
           'Abandon bleibt abgebrochen')
    pruefe(_ml134._zustand_zu('Fail') == _ml134.FEHLGESCHLAGEN,
           'Fail ist fehlgeschlagen — nicht abgeschlossen')
    pruefe(_ml134._zustand_zu('Deactivate') == _ml134.VERFALLEN,
           'Deactivate ist verfallen — das Spiel zog ihn selbst zurueck')
    # ⚠ Ein unbekannter oder fehlender Ausgang faellt auf den alten Stand
    # zurueck. Ein Ende ohne <EndMission> steht nur als Mitteilung im Log.
    pruefe(_ml134._zustand_zu('') == _ml134.ABGESCHLOSSEN,
           'ohne Angabe gilt weiter abgeschlossen')
    pruefe(_ml134._zustand_zu('WasGanzNeues') == _ml134.ABGESCHLOSSEN,
           'ein unbekannter Ausgang faellt nicht durch')

    # ⚠ Und die Anzeige muss den neuen Zustand kennen — ein Zustand ohne Farbe
    # und ohne Wort waere im Protokoll eine leere Zelle.
    _quelle134 = open(os.path.join(WURZEL, 'scbp', 'seiten.py'),
                      encoding='utf-8').read()
    pruefe('missionslog.FEHLGESCHLAGEN: ROT_BLASS' in _quelle134,
           'fehlgeschlagen hat eine Farbe')
    pruefe("missionslog.FEHLGESCHLAGEN: 's_al_fehl'" in _quelle134,
           'fehlgeschlagen hat ein Wort')
    pruefe('missionslog.ABGEBROCHEN: ROT_BLASS' in _quelle134,
           'abgebrochen steht in blassem Rot, nicht mehr in Grau')
    pruefe('missionslog.VERFALLEN: SUB' in _quelle134,
           'nicht mehr offen bleibt grau')

    print()
    print('135. Jeder Zustand hat einen Filterknopf in seiner Farbe')
    # Gewuenscht am 06.09.2026: „Buttons wie bei was ist neu in den farben ob
    # abgeschlossen, abgebrochen, fehlgeschlagen, das man dann nur die art
    # sieht."
    #
    # ⚠ Geprueft wird die KOPPLUNG, nicht das Aussehen. Dass die Knoepfe
    # klicken und filtern, ist am gebauten Fenster nachgemessen (sechs
    # Knoepfe, Farben auf den Hex-Wert, Trefferzahlen passend). Was dabei
    # NICHT auffaellt und spaeter still bricht: Ein neuer Zustand bekommt eine
    # Farbe in der Liste, aber keinen Knopf — dann gibt es Zeilen, die durch
    # keinen Filter zu erreichen sind. Genau das faengt diese Pruefung.
    _fa135 = re.search(r'farben = \{(.*?)\}', _quelle134, re.S)
    _sc135 = re.search(r'schalter = \[(.*?)\]', _quelle134, re.S)
    pruefe(bool(_fa135 and _sc135),
           'Farbtabelle und Knopfliste sind beide da')
    if _fa135 and _sc135:
        _zust135 = dict(re.findall(r'missionslog\.(\w+):\s*(\w+)',
                                   _fa135.group(1)))
        _knopf135 = dict((k, f) for k, _w, f in
                         re.findall(r'missionslog\.(\w+),\s*\'([^\']+)\',\s*(\w+)',
                                    _sc135.group(1)))
        _ohne = sorted(set(_zust135) - set(_knopf135))
        pruefe(not _ohne,
               'jeder Zustand der Liste hat einen Knopf (ohne: %s)' % _ohne)
        _falsch135 = sorted(k for k in _knopf135
                            if k in _zust135 and _knopf135[k] != _zust135[k])
        pruefe(not _falsch135,
               'jeder Knopf traegt die Farbe seines Zustands (falsch: %s)'
               % _falsch135)
        # ⚠ Gegenprobe: Wuerde sie auch anschlagen? Ein erfundener Zustand
        # ohne Knopf muss auffallen — sonst prueft die Zeile darueber nichts.
        _probe135 = dict(_zust135)
        _probe135['ERFUNDEN'] = 'GOLD'
        pruefe(bool(set(_probe135) - set(_knopf135)),
               'Gegenprobe: ein Zustand ohne Knopf faellt auf')

    # ⚠⚠ **Die Reihenfolge in `zeichnen` ist entscheidend.** Die Meldung „Noch
    # kein Auftrag aufgezeichnet" darf NUR beim wirklich leeren Protokoll
    # kommen. Stuende sie hinter dem Filter, hiesse ein sauberes Konto ohne
    # Fehlschlaege „du hast noch nie einen Auftrag gespielt" — schlicht falsch.
    _leer135 = _quelle134.find("t('s_al_leer')")
    _filter135 = _quelle134.find("stand['art'] != 'alle'")
    pruefe(_leer135 > 0 and _filter135 > _leer135,
           'die Leer-Meldung wird vor dem Filtern entschieden')

    print()
    print('136. Auch die Ruf-Zeilen im Bauplan-Block sind blau')
    # ⚠⚠ Gemeldet am 06.09.2026, nachdem die Angabe zweimal als „fehlt"
    # durchgegangen war: „da ist keine Reputation in den Questtexten."
    #
    # Sie WAR da — nur nicht hervorgehoben. Die Rohdaten liefern zwei Felder:
    # `contractInfo` (wird seit v3.17.0 blau eingesetzt) und `description`,
    # der Bauplan-Block. In letzterem stehen zwei weitere Ruf-Zeilen, die
    # unveraendert uebernommen wurden — also schwarz, mitten zwischen den
    # blauen. Gemessen in einer echten global.ini: 435 + 435 + 129 Zeilen.
    from scbp import injektion as _in136

    _block136 = ('MÖGLICHE BAUPLÄNE FÜR DIESEN MISSIONSTYP\\n'
                 '# Min. Reputation: Auftragnehmer Junior (800 XP)\\n'
                 '# Max. Reputation: Auftragnehmer Elite (95.250 XP)\\n'
                 '# Baupläne:\\n'
                 '    [  ] Atzkav Sniper Rifle\\n'
                 '# Region: Stanton-System - Gefahr 4-6/10')
    _neu136 = _in136._ruf_einfaerben(_block136)

    pruefe(_neu136.count(_in136.FARBE_AUF) == 2,
           'beide Ruf-Zeilen sind blau (gefunden: %d)'
           % _neu136.count(_in136.FARBE_AUF))
    pruefe('<EM4># Min. Reputation: Auftragnehmer Junior (800 XP)</EM4>'
           in _neu136, 'die Min-Zeile steht vollstaendig in Blau')

    # ⚠ Nur Ruf-Zeilen. Gliederung bleibt schwarz — waere alles blau, waere
    # nichts hervorgehoben.
    for _wort136 in ('# Baupläne:', '# Region:'):
        _zeile136 = [z for z in _neu136.split('\\n') if z.startswith(_wort136)]
        pruefe(_zeile136 and _in136.FARBE_AUF not in _zeile136[0],
               '%s bleibt schwarz' % _wort136)

    # ⚠ Die Bauplan-Zeile darf sich nicht veraendern — an ihrem Kaestchen
    # haengt die Erkennung, welcher Block uns gehoert.
    pruefe('    [  ] Atzkav Sniper Rifle' in _neu136,
           'die Bauplan-Zeile bleibt unangetastet')

    # ⚠ Nichts doppelt: Ein zweiter Lauf darf nicht `<EM4><EM4>` erzeugen —
    # das zeigt das Spiel als sichtbaren Text an.
    pruefe(_in136._ruf_einfaerben(_neu136) == _neu136,
           'ein zweiter Lauf aendert nichts mehr')

    # ⚠ Gegenprobe: Ohne den Aufruf bliebe alles schwarz — sonst prueft die
    # erste Zeile nichts.
    pruefe(_in136.FARBE_AUF not in _block136,
           'Gegenprobe: der Ausgangsblock ist schwarz')

    print()
    print('137. Ohne Rufwerte steht „Keine Angaben" statt gar nichts')
    # ⚠⚠ Am 06.09.2026 gemessen: 109 Auftraege bekamen ueberhaupt keine
    # Ruf-Zeile, weil die Quelle fuer sie keine Rufwerte fuehrt. Im Spiel
    # standen dort nur Abklingzeit und Teilbarkeit — und die Luecke sah aus
    # wie ein Aussetzer des Werkzeugs statt wie fehlende Daten.
    _worte137 = _in136.TEXTE['de']

    # Ein Auftrag, dessen Quelle keine Rufangabe hat.
    _leer137 = {'titleLocKey': 'probe_ohne_ruf',
                'contractInfo': '# Cooldown für Mission: 1 Minute\\n'
                                '# Mission kann geteilt werden? Ja'}
    _z137 = _in136._angabenzeilen(_leer137, '', _worte137, None)
    _ruf137 = [z for z in _z137
               if any(w in z.lower() for w in _in136.RUF_WORTE)]
    pruefe(len(_ruf137) == 1,
           'genau eine Ruf-Zeile kommt dazu (gefunden: %d)' % len(_ruf137))
    pruefe(_ruf137 and 'Keine Angaben' in _ruf137[0],
           'sie sagt „Keine Angaben"')
    pruefe(_ruf137 and _ruf137[0].startswith(_in136.FARBE_AUF),
           'auch der Platzhalter ist blau')
    # ⚠ Er steht OBEN, bei den anderen Angaben — nicht unten angehaengt.
    pruefe(_z137 and _z137[0] is _ruf137[0] if _ruf137 else False,
           'der Platzhalter steht bei den uebrigen Angaben')

    # ⚠⚠ Kein Platzhalter, wo schon eine Rufangabe steht — weder eine eigene…
    _hat137 = {'titleLocKey': 'probe_mit_ruf',
               'contractInfo': '# Zu erwartende Rufpunkte: 150 XP\\n'
                               '# Cooldown für Mission: 1 Minute'}
    _z137b = _in136._angabenzeilen(_hat137, '', _worte137, None)
    _ruf137b = [z for z in _z137b
                if any(w in z.lower() for w in _in136.RUF_WORTE)]
    pruefe(len(_ruf137b) == 1 and 'Keine Angaben' not in _ruf137b[0],
           'wo Rufwerte da sind, kommt kein Platzhalter dazu')

    # …noch eine, die schon im Text des Spiels steht (anderes Werkzeug).
    _z137c = _in136._angabenzeilen(
        _leer137, '# Min. Reputation: Auftragnehmer Junior', _worte137, None)
    _ruf137c = [z for z in _z137c
                if any(w in z.lower() for w in _in136.RUF_WORTE)]
    pruefe(not _ruf137c,
           'steht schon eine Rufangabe im Text, kommt keine zweite')

    # ⚠ Gegenprobe: Ohne die Ergaenzung waere die Liste ruflos — sonst
    # prueft die erste Zeile nichts.
    pruefe(not [z for z in (_leer137['contractInfo'] or '').split('\\n')
                if any(w in z.lower() for w in _in136.RUF_WORTE)],
           'Gegenprobe: die Quelle selbst nennt keinen Ruf')

    print()
    print('138. Das Bilder-Werkzeug ist auf beiden Systemen einsatzbereit')
    # ⚠⚠ Am 06.09.2026: Die Bilder der Anleitung waren anderthalb Wochen alt,
    # das Overlay-Bild zeigte eine Fassung von vor 18 Versionen — mit einem
    # Verhalten, das es seit v3.0.0-rc95 nicht mehr gibt. Der Grund war nicht
    # Nachlaessigkeit: `bilder_machen.py` lief nur unter Windows und brach hier
    # mit „braucht Windows" ab. Im Kopf des Werkzeugs steht „Was von Hand
    # gemacht wird, verrottet" — das galt auch fuer es selbst.
    #
    # Diese Pruefung schaut auf den Quelltext, nicht auf einen Lauf: Bilder zu
    # machen dauert Minuten und braucht einen Bildschirm. Sie faengt die drei
    # Dinge ab, die still brechen koennen, ohne dass es jemand merkt — bis zum
    # naechsten Bilderlauf, und der ist selten.
    _bm138 = open(os.path.join(WURZEL, 'tools', 'bilder_machen.py'),
                  encoding='utf-8').read()

    # ⚠⚠ Das Wichtigste: Ein Werkzeug mit Fenster MUSS sich unsichtbar machen.
    # Claudes Shell haengt an `DISPLAY=:0`, also am Monitor des Nutzers — ohne
    # diesen Aufruf blitzt das Fenster dort auf und reisst den Tastaturfokus
    # mit. Wer gerade Star Citizen fliegt, landet im Desktop und stirbt.
    #
    # ⚠⚠ **Gefragt wird der SYNTAXBAUM, nicht der Text.** Die erste Fassung
    # dieser Zeile suchte schlicht nach `'unsichtbar.sicherstellen(' in …` —
    # und blieb gruen, als der Aufruf zum Ausprobieren auskommentiert wurde:
    # In `# unsichtbar.sicherstellen(...)` steht der gesuchte Text ja weiter
    # drin. Eine Pruefung, die eine auskommentierte Sicherung fuer vorhanden
    # haelt, prueft genau das Gegenteil von dem, wofuer sie gebaut wurde.
    import ast as _ast138
    _baum138 = _ast138.parse(_bm138)
    _rufe138 = set()
    for _k138 in _ast138.walk(_baum138):
        if isinstance(_k138, _ast138.Call):
            _f138 = _k138.func
            if isinstance(_f138, _ast138.Attribute):
                _rufe138.add(_f138.attr)
            elif isinstance(_f138, _ast138.Name):
                _rufe138.add(_f138.id)
    pruefe('sicherstellen' in _rufe138,
           'das Werkzeug startet sich unsichtbar neu')
    pruefe('deiconify' in _rufe138,
           'das versteckte Overlay wird sichtbar gemacht (echter Aufruf)')

    # Beide Wege muessen da sein — sonst faellt ein System wieder heraus.
    pruefe('def abgreifen_x11(' in _bm138, 'es gibt einen Linux-Abgriff')
    pruefe('PrintWindow' in _bm138, 'der Windows-Abgriff ist noch da')
    pruefe("if sys.platform != 'win32':\n        return abgreifen_x11" in _bm138,
           'unter Linux wird auf den X11-Weg umgeschaltet')

    # ⚠ Das Overlay ist keine Seite und faellt sonst wieder heraus — genau so
    # ist sein Bild anderthalb Wochen alt geworden.
    pruefe('def overlay_bild(' in _bm138, 'das Overlay hat einen eigenen Weg')
    pruefe("'--nur-overlay'" in _bm138,
           'und einen eigenen Prozess (zwei tk.Tk() vertraegt Tk nicht)')
    # ⚠⚠ Und die Kopie muss auch das SPIEL schuetzen, nicht nur die eigenen
    # Daten: `SC_BP_HOME` lenkt Bestand und Einstellungen um, die `global.ini`
    # liegt woanders — `inj_auto` schreibt beim Start hinein.
    pruefe('_gefaehrliches_abschalten' in _bm138,
           'in der Kopie wird abgeschaltet, was ausserhalb wirkt')
    pruefe("daten['inj_auto'] = False" in _bm138,
           'die Auto-Injektion ist dabei ausgeschaltet')

    # ⚠⚠ **Gegenprobe am selben Weg, nicht an einem bequemeren.** Der Aufruf
    # wird auskommentiert und der Baum neu gelesen: Genau so wurde die erste
    # Fassung dieser Pruefung ueberfuehrt, die den Text durchsucht hatte.
    _kaputt138 = _bm138.replace('    unsichtbar.sicherstellen(',
                                '    pass  # unsichtbar.sicherstellen(')
    _rufe_kaputt138 = set()
    for _k138 in _ast138.walk(_ast138.parse(_kaputt138)):
        if isinstance(_k138, _ast138.Call) and isinstance(
                _k138.func, _ast138.Attribute):
            _rufe_kaputt138.add(_k138.func.attr)
    pruefe('sicherstellen' not in _rufe_kaputt138,
           'Gegenprobe: ein auskommentierter Aufruf gilt NICHT als vorhanden')

    import re as _re138

    # Zu jeder Seite in `bilder_machen.SEITEN` muss ein Bild vorliegen.
    _bilder138 = [n for n in os.listdir(os.path.join(WURZEL, 'assets'))
                  if n.startswith('screenshot-') and n.endswith('.png')]
    # ⚠⚠ **Keine feste Zahl mehr.** Bis zum 06.09.2026 stand hier
    # `len(...) == 34` — und wer eine Seite dazunahm, bekam eine rote Prüfung,
    # obwohl er alles richtig gemacht hatte. Dasselbe Muster wie bei der
    # Prüfung, die einmal den falschen Aufruf **festgeschrieben** hat:
    # Geprüft gehört die **Wirkung** („zu jeder Seite gibt es ein Bild"), nicht
    # eine Zahl, die jemand nachpflegen muss.
    _erwartet138 = set()
    for _z138 in _bm138.split('\n'):
        _m138 = _re138.match(r"\s*'([a-z]+)':\s*'(screenshot-[a-z-]+)'", _z138)
        if _m138:
            _erwartet138.add(_m138.group(2))
    _fehlt138 = sorted(n for n in _erwartet138
                       if '%s.png' % n not in _bilder138)
    pruefe(not _fehlt138,
           'zu jeder Seite gibt es ein Bild (fehlt: %s)'
           % (', '.join(_fehlt138[:4]) if _fehlt138 else 'keins'))
    _paare138 = sorted(n[:-4].replace('-en', '') for n in _bilder138)
    pruefe(all(_paare138.count(n) == 2 for n in set(_paare138)),
           'zu jedem deutschen Bild gibt es das englische')

    # ⚠ **Erklaerbilder sind KEINE Bildschirmfotos** und werden oben nicht
    # mitgezaehlt — der Filter dort greift `screenshot-*`. Sie zeigen keine
    # Seite, sondern eine Sache: was Totzone, Saettigung und Empfindlichkeit
    # mit der Kurve machen. Gebaut aus demselben Bauteil, das die Kurve auch
    # im Programm zeichnet, damit beides nie auseinanderlaeuft.
    #
    # Zweisprachig sind sie trotzdem, aus demselben Grund wie alles andere:
    # Eine halbe Uebersetzung wirkt schlechter als gar keine.
    _erkl138 = [n for n in os.listdir(os.path.join(WURZEL, 'assets'))
                if n.startswith('erklaerung-') and n.endswith('.png')]
    _erklpaare138 = sorted(n[:-4].replace('-en', '') for n in _erkl138)
    pruefe(_erkl138 and all(_erklpaare138.count(n) == 2
                            for n in set(_erklpaare138)),
           'jedes Erklaerbild gibt es deutsch und englisch (%d)'
           % len(_erkl138))

    print()
    print('139. Schiffe finden ihre Steckplätze — auch bei krummen Namen')
    # ⚠⚠ **Diese Prüfung gibt es, weil die Zuordnung STILL falsch war.**
    # Sie hat nicht gekracht und nichts gemeldet — sie hat nur nichts gefunden,
    # und die Anzeige machte daraus „noch nicht im Spiel". Zwei Schiffe, die
    # längst fliegen, standen so als Konzept da (gemeldet 06.09.2026).
    #
    # ⚠ Sie **legt sich ihre Daten selbst hin**: ein Dutzend erkul-Kennungen im
    # Code, kein Abruf, keine Nutzerdatei. Sonst wäre sie eine Prüfung, die
    # sich im Wegwerf-Ordner selbst überspringt — und damit keine.
    from scbp import erkul as _erk139

    _ids139 = {
        'anvl_arrow': 1, 'anvl_hornet_f7cm': 1, 'anvl_hornet_f7cm_mk2': 1,
        'anvl_hornet_f7c_mk2': 1, 'anvl_hornet_f7cm_mk2_heartseeker': 1,
        'drak_ironclad': 1, 'drak_ironclad_assault': 1,
        'aegs_gladius': 1, 'aegs_gladius_valiant': 1,
        'krig_l22_alphawolf': 1, 'rsi_ursa_rover': 1, 'rsi_ursa_medivac': 1,
    }
    # (Name, Hersteller ausgeschrieben, Herstellerkürzel) -> erwartete Kennung
    _faelle139 = [
        # Der Hersteller steht ausgeschrieben da, erkul kürzt ihn.
        (('Drake Ironclad Assault', '', ''), 'drak_ironclad_assault'),
        # Römische Zahl gegen Ziffer — und die Mk I darf NICHT gewinnen.
        (('F7C-M Super Hornet Mk II', 'Anvil Aerospace', 'ANVL'),
         'anvl_hornet_f7cm_mk2'),
        # Kürzel statt ausgeschriebenem Hersteller.
        (('Ursa Medivac', 'Roberts Space Industries', 'RSI'),
         'rsi_ursa_medivac'),
        # Unterstriche und Ziffern verklebt.
        (('L-22 Alpha Wolf', '', 'KRIG'), 'krig_l22_alphawolf'),
        (('Gladius Valiant', 'Aegis Dynamics', 'AEGS'), 'aegs_gladius_valiant'),
        (('Arrow', 'Anvil Aerospace', 'ANVL'), 'anvl_arrow'),
    ]
    _daneben139 = []
    for (_n139, _h139, _hk139), _soll139 in _faelle139:
        _ist139 = _erk139._wortweise_suchen(_ids139, _n139, _h139, '', _hk139)
        if _ist139 != _soll139:
            _daneben139.append('%s -> %s statt %s' % (_n139, _ist139 or '—',
                                                      _soll139))
    # ⭐ **Die Trefferquote steht im Text, nicht nur im Ergebnis.** Eine
    # Zuordnung, die „alles in Ordnung" meldet, ohne dass jemand die Zahl
    # gesehen hat, ist dieselbe stille Falle noch einmal.
    pruefe(not _daneben139,
           'alle %d krummen Namen finden ihr Schiff (daneben: %s)'
           % (len(_faelle139), _daneben139 or 'keiner'))

    # ⚠ Gegenprobe: Ein Name, der auf zwei Kennungen gleich gut passt, darf
    # **keine** liefern. Raten ist schlimmer als „keine Daten" — sonst zeigt
    # das Werkzeug die Steckplätze des falschen Schiffs, und das merkt niemand.
    _zwei139 = _erk139._wortweise_suchen({'aegs_gladius': 1, 'aegs_gladius_pirat': 1},
                                         'Gladius', 'Aegis Dynamics', '', 'AEGS')
    pruefe(_zwei139 == 'aegs_gladius',
           'bei einem klaren Sieger wird zugeordnet')
    _patt139 = _erk139._wortweise_suchen({'anvl_hornet_f7c': 1, 'anvl_hornet_f7a': 1},
                                         'Hornet', '', '', 'ANVL')
    pruefe(_patt139 == '',
           'bei Gleichstand wird NICHT geraten (bekam: %r)' % _patt139)

    # ⚠ Und die Falle, die den halben Tag gekostet hat: Wortgrenzen. Läuft die
    # Suche gegen geschliffene Schlüssel (`drakironcladassault`), gibt es nur
    # noch ein einziges Wort — sie findet dann nie etwas und sagt nicht warum.
    _geschliffen139 = {'drakironcladassault': 1}
    pruefe(_erk139._wortweise_suchen(_geschliffen139, 'Drake Ironclad Assault',
                                     '', '', '') == '',
           'Gegenprobe: ohne Wortgrenzen findet die Suche nichts')

    print()
    print('140. Der Ablage-Ordner nimmt die Daten mit')
    # ⚠⚠ **Diese Prüfung bewacht einen Datenverlust, keinen Schönheitsfehler.**
    # Bis v3.19.0 setzte „Ablage-Ordner umstellen" nur die Einstellung — die
    # Dateien blieben liegen. Wer umstellte, sah nach dem Neustart ein leeres
    # Programm und hielt seinen Bauplan-Bestand für verloren.
    import shutil as _sh140
    import tempfile as _tf140
    from scbp import pfade as _pf140

    _von140 = _tf140.mkdtemp(prefix='sc-bp-alt-')
    _nach140 = _tf140.mkdtemp(prefix='sc-bp-neu-')
    try:
        os.makedirs(os.path.join(_von140, 'Bauplaene'), exist_ok=True)
        for _n140 in ('bestand.json', 'watchlist.json'):
            with open(os.path.join(_von140, 'Bauplaene', _n140), 'w',
                      encoding='utf-8') as _f140:
                json.dump({'probe': list(range(50))}, _f140)

        # ⚠ Rekursiv: Die Ablage sortiert seit v3.0.0 in Unterordner. Ein
        # flacher Durchlauf fände hier **nichts** und meldete „nichts zu tun",
        # während der ganze Bestand danebenliegt.
        pruefe(len(_pf140._dateien_der_ablage(_von140)) == 2,
               'Dateien werden auch in Unterordnern gefunden')

        _schreibbar140, _fremde140, _ = _pf140.ablage_lage(_nach140)
        pruefe(_schreibbar140 and _fremde140 == 0,
               'ein leeres, beschreibbares Ziel wird als solches erkannt')

        _kop140, _ueber140, _fehl140 = _pf140.ablage_umziehen(_von140, _nach140)
        pruefe((_kop140, _ueber140, _fehl140) == (2, 0, 0),
               'beide Dateien kommen an (bekam: %s)'
               % ((_kop140, _ueber140, _fehl140),))

        # ⚠⚠ **Der alte Ordner bleibt.** Er ist der einzige Rückweg, wenn beim
        # Wechsel etwas schiefgeht — ein Bauplan-Bestand sind Monate Spielzeit.
        pruefe(sorted(os.listdir(os.path.join(_von140, 'Bauplaene'))) ==
               ['bestand.json', 'watchlist.json'],
               'der alte Ordner bleibt vollstaendig liegen')

        # ⚠ Zweiter Lauf: Vorhandenes am Ziel wird NICHT ueberschrieben. Wer
        # auf einen Ordner wechselt, in dem schon ein Bestand liegt, will
        # dessen Daten behalten.
        _zweit140 = _pf140.ablage_umziehen(_von140, _nach140)
        pruefe(_zweit140 == (0, 2, 0),
               'ein zweiter Lauf ueberschreibt nichts (bekam: %s)' % (_zweit140,))

        _belegt140 = _pf140.ablage_lage(_nach140)
        pruefe(_belegt140[1] == 2,
               'ein belegtes Ziel wird als belegt gemeldet')

        # Gegenprobe: Ein Ziel ohne Schreibrecht muss VORHER auffallen, nicht
        # erst mitten im Kopieren.
        _ro140 = _tf140.mkdtemp(prefix='sc-bp-ro-')
        try:
            os.chmod(_ro140, 0o500)
            _ok140, _, _grund140 = _pf140.ablage_lage(_ro140)
            # ⚠ Als root ist jeder Ordner beschreibbar — dann sagt die Probe
            # nichts aus und wird uebersprungen statt falsch bestanden.
            if os.name != 'nt' and os.geteuid() != 0:
                pruefe(not _ok140 and _grund140,
                       'Gegenprobe: ein gesperrtes Ziel faellt vorher auf')
        finally:
            try:
                os.chmod(_ro140, 0o700)
            except OSError:
                pass
            _sh140.rmtree(_ro140, ignore_errors=True)
    finally:
        _sh140.rmtree(_von140, ignore_errors=True)
        _sh140.rmtree(_nach140, ignore_errors=True)

    # ------------------------------------------------------------------
    # 146. Totzone, Sättigung und Kurve — und was davon überhaupt gilt
    #
    # ⚠ Diese Prüfung baut sich ihre `actionmaps.xml` SELBST. Sie darf nicht
    # von der Datei des Entwicklers abhängen: Auf dem Bau-Rechner gibt es
    # keine, und die Prüfung liefe dort still ins Leere — genau der Fehler,
    # der Prüfung 67 von ihrem ersten Tag an wertlos gemacht hat.
    #
    # Die Kennungen sind frei erfunden (`AAAA1111…`). Damit kann kein echtes
    # Gerät des laufenden Rechners hineinfunken: `kurven.gueltige_kennungen`
    # zieht auch die Game.log heran, und die sieht auf jedem Rechner anders
    # aus. Lokal grün, im Bau rot wäre hier besonders tückisch.
    print()
    # ⚠ Nummer 146, obwohl sie weit vor den 140ern steht: Drei Sitzungen
    # haben am selben Tag Prüfungen angelegt, und 142 wie 144 gab es dadurch
    # doppelt. Am 06.09.2026 glattgezogen — der Joystick-Strang wanderte ans
    # Ende (146-148), weil der andere in sich fortlaufend war. Die Nummer sagt,
    # **wann** eine Prüfung dazukam, nicht wo sie in der Datei steht.
    print('146. Achsen: Totzone, Sättigung, tote Kennungen')
    import shutil as _sh141
    import tempfile as _tf141
    from scbp import kurven as _kv141

    _AKTIV141 = 'AAAA1111-0000-0000-0000-504944564944'
    _ALT141 = 'BBBB2222-0000-0000-0000-504944564944'
    _WEG141 = 'CCCC3333-0000-0000-0000-504944564944'

    # Der Aufbau bildet genau die Lagen nach, die an einer echten Datei
    # gemessen wurden (06.09.2026):
    #   · ein Gerät mit aktiver UND überholter Kennung (Sättigung verloren)
    #   · Sättigung doppelt geschrieben — der Normalfall, kein Fehler
    #   · zwei Blöcke mit DERSELBEN Kennung und widersprüchlichem Wert
    #   · ein Gerät, das es gar nicht mehr gibt
    _xml141 = (
        '<ActionMaps version="1" optionsVersion="2" rebindVersion="2" '
        'profileName="default">\n'
        ' <CustomisationUIHeader label="test">\n'
        '  <deviceoptions name="Testknueppel  {%s}">\n'
        '   <option input="x" deadzone="0.1"/>\n'
        '   <option input="y" deadzone="0.1"/>\n'
        '  </deviceoptions>\n'
        '  <deviceoptions name="Testknueppel  {%s}">\n'
        '   <option input="x" deadzone="0.1"/>\n'
        '   <option input="x" saturation="0.75"/>\n'
        '   <option input="x" saturation="0.75"/>\n'
        '  </deviceoptions>\n'
        '  <deviceoptions name="Altgeraet  {%s}">\n'
        '   <option input="x" deadzone="0.3"/>\n'
        '  </deviceoptions>\n'
        '  <deviceoptions name="Altgeraet  {%s}">\n'
        '   <option input="x" deadzone="0.4"/>\n'
        # ⚠ `y` MIT Sättigung, obwohl die Quelle dort keine hat. Ohne diese
        # Zeile war die Prüfung „ein fehlender Wert wird als Löschen
        # übertragen" wertlos: Sie verglich auf einer Achse, die es beim Ziel
        # gar nicht gab, also None mit None — und meldete grün, ohne je
        # etwas geprüft zu haben.
        '   <option input="y" deadzone="0.4"/>\n'
        '   <option input="y" saturation="0.55"/>\n'
        '  </deviceoptions>\n'
        ' </CustomisationUIHeader>\n'
        ' <options type="joystick" instance="1" Product="Testknueppel  {%s}">\n'
        '  <flight_move_pitch exponent="1.5"/>\n'
        '  <flight_move_yaw invert="1"/>\n'
        ' </options>\n'
        ' <options type="joystick" instance="2"/>\n'
        ' <ActionProfiles>\n'
        '  <actionmap name="spaceship_general">\n'
        '   <action name="v_eject"><rebind input="js1_button1"/></action>\n'
        '  </actionmap>\n'
        ' </ActionProfiles>\n'
        '</ActionMaps>\n'
    ) % (_AKTIV141, _ALT141, _WEG141, _WEG141, _AKTIV141)

    _ordner141 = _tf141.mkdtemp(prefix='scbp-kurven-')
    _datei141 = os.path.join(_ordner141, 'actionmaps.xml')
    try:
        with open(_datei141, 'w', encoding='utf-8') as _f141:
            _f141.write(_xml141)

        _bloecke141 = _kv141.geraete_achsen(datei=_datei141)
        _nach141 = {}
        for _b141 in _bloecke141:
            _nach141.setdefault(_b141['kennung'], []).append(_b141)

        pruefe(len(_bloecke141) == 4,
               'alle vier Geräteblöcke gefunden (sind: %d)' % len(_bloecke141))

        # Lebendig ist, was in der Belegung eine Nummer hat.
        pruefe(_nach141.get(_AKTIV141) and _nach141[_AKTIV141][0]['aktiv'],
               'der Block mit Nummer im Spiel gilt als aktiv')
        pruefe(_nach141.get(_ALT141) and not _nach141[_ALT141][0]['aktiv'],
               'der Block mit unbekannter Kennung gilt als tot')

        # ⭐ Der Kern: „Gerät ist da, Einstellung hängt an alter Kennung."
        pruefe(_nach141.get(_ALT141) and _nach141[_ALT141][0]['ueberholt'],
               'gleicher Name + aktiver Zwilling -> überholt, nicht verwaist')
        pruefe(_nach141.get(_WEG141) and _nach141[_WEG141][0]['verwaist'],
               'Gerät ohne aktiven Zwilling -> verwaist')

        _uebern141 = _kv141.uebernehmbar(datei=_datei141)
        pruefe(len(_uebern141) == 1,
               'genau ein übernehmbarer Fall (sind: %d)' % len(_uebern141))
        _verloren141 = [z for z in (_uebern141[0]['werte'] if _uebern141 else [])
                        if z[1] == 'saturation' and z[3] is None]
        pruefe(len(_verloren141) == 1,
               'die verlorene Sättigung wird als solche erkannt')

        # Zwei Blöcke, dieselbe Kennung, verschiedene Werte — der Widerspruch
        # geht ÜBER die Blöcke und fällt in einem einzelnen nicht auf.
        pruefe(all('x' in _b141['mehrfach'] for _b141 in _nach141.get(_WEG141, [])),
               'Widerspruch über zwei Blöcke derselben Kennung erkannt')

        # ⚠ Der Selbsttreffer-Fehler: Ein Kinder-Regex, der das Elternelement
        # sehen kann, verschlingt den ganzen Block und findet nie eine Achse.
        # Diese Prüfung ist genau dafür da — sie war bei ihrer Entstehung rot.
        _spiel141 = [b for b in _kv141.spielachsen(datei=_datei141)
                     if b['nummer'] == 1 and b['art'] == 'joystick']
        pruefe(_spiel141 and len(_spiel141[0]['achsen']) == 2,
               'beide Spielachsen gelesen (sind: %d)'
               % (len(_spiel141[0]['achsen']) if _spiel141 else 0))
        pruefe(_spiel141 and _spiel141[0]['achsen'].get(
            'flight_move_pitch', {}).get('exponent') == 1.5,
            'der Exponent kommt richtig an')

        # Schreiben: über die Kennung, Doppel einsammeln, Rest heil lassen
        #
        # ⚠⚠ Vorher den Zustand ALLER Blöcke festhalten. Die erste Fassung
        # prüfte nur den einen toten Block, den sie im Verdacht hatte — und
        # blieb in der Gegenprobe grün: Der eingebaute Fehler (Schreiben ohne
        # Kennungsprüfung) traf den *letzten* Block der Datei, nicht diesen
        # einen. Eine Wache, die nur eine Tür bewacht, meldet nichts, wenn
        # jemand durch die andere geht.
        _vorher_alle141 = {}
        for _b141 in _kv141.geraete_achsen(datei=_datei141):
            _vorher_alle141.setdefault(_b141['kennung'], []).append(
                {a: dict(w) for a, w in _b141['achsen'].items()})

        _ok141, _meld141, _n141 = _kv141.setzen(
            _AKTIV141, 'x', 'saturation', 0.6, datei=_datei141)
        pruefe(_ok141, 'Schreiben gelingt (%s)' % ('ok' if _ok141 else _meld141))

        _nachher_alle141 = {}
        for _b141 in _kv141.geraete_achsen(datei=_datei141):
            _nachher_alle141.setdefault(_b141['kennung'], []).append(
                {a: dict(w) for a, w in _b141['achsen'].items()})

        _jetzt141 = [b for b in _kv141.geraete_achsen(datei=_datei141)
                     if b['kennung'] == _AKTIV141]
        pruefe(_jetzt141 and abs((_jetzt141[0]['achsen']['x'].get('saturation')
                                  or 0) - 0.6) < 1e-6,
               'der neue Wert steht im richtigen Block')

        _beruehrt141 = [k for k in _vorher_alle141
                        if k != _AKTIV141
                        and _vorher_alle141[k] != _nachher_alle141.get(k)]
        pruefe(not _beruehrt141,
               '* KEIN anderer Block wurde angefasst (berührt: %d)'
               % len(_beruehrt141))

        with open(_datei141, encoding='utf-8') as _f141:
            _inhalt141 = _f141.read()
        pruefe(_inhalt141.count('<rebind') == 1,
               'die Belegung hat den Schreibvorgang überlebt')
        pruefe(_inhalt141.count('saturation="0.6"') == 1,
               'der Wert steht genau einmal da, nicht doppelt')

        # Ein Wert ausserhalb 0..1 und eine unbekannte Kennung müssen
        # abgelehnt werden — ohne die Datei anzufassen.
        _vorher141 = _inhalt141
        pruefe(not _kv141.setzen(_AKTIV141, 'x', 'deadzone', 2.0,
                                 datei=_datei141)[0],
               'ein Wert über 1 wird abgelehnt')
        pruefe(not _kv141.setzen('FFFF9999', 'x', 'deadzone', 0.2,
                                 datei=_datei141)[0],
               'eine unbekannte Kennung wird abgelehnt')
        with open(_datei141, encoding='utf-8') as _f141:
            pruefe(_f141.read() == _vorher141,
                   'nach zwei abgelehnten Versuchen ist die Datei unverändert')
        # ⭐ Zwei Sticks angleichen — Totzone UND Sättigung, in einem Zug.
        # Die Quelle (aktiv) hat jetzt Sättigung 0,6 und Totzone 0,1; das
        # Ziel `_WEG141` hat nur eine Totzone. Danach müssen beide gleich sein.
        _ok141, _meld141, _n141 = _kv141.angleichen(
            _AKTIV141, _WEG141, datei=_datei141)
        pruefe(_ok141, 'Angleichen gelingt (%s)' % ('ok' if _ok141
                                                    else _meld141))
        _danach141 = {b['kennung']: b for b in
                      _kv141.geraete_achsen(datei=_datei141)}
        _q141 = _danach141.get(_AKTIV141, {}).get('achsen', {})
        _z141 = _danach141.get(_WEG141, {}).get('achsen', {})
        _gemeinsam141 = [a for a in _kv141.ACHSEN if a in _q141 and a in _z141]
        pruefe(_gemeinsam141,
               'es gibt gemeinsame Achsen (%d)' % len(_gemeinsam141))
        _ungleich141 = [a for a in _gemeinsam141
                        if _q141[a].get('deadzone') != _z141[a].get('deadzone')
                        or _q141[a].get('saturation')
                        != _z141[a].get('saturation')]
        pruefe(not _ungleich141,
               '* danach sind beide Geräte gleich eingestellt (ungleich: %d)'
               % len(_ungleich141))
        # ⚠ Auch das Löschen muss übertragen worden sein: Die Quelle hat auf
        # `y` keine Sättigung — hätte das Ziel dort eine behalten, wären die
        # beiden eben NICHT gleich, und der ganze Zweck wäre verfehlt.
        pruefe(_q141.get('y', {}).get('saturation')
               == _z141.get('y', {}).get('saturation'),
               'ein fehlender Wert wird als Löschen übertragen')
        pruefe(not _kv141.angleichen(_AKTIV141, _AKTIV141,
                                     datei=_datei141)[0],
               'ein Gerät auf sich selbst anzugleichen wird abgelehnt')
        pruefe(not _kv141.angleichen(_AKTIV141, 'FFFF9999',
                                     datei=_datei141)[0],
               'eine unbekannte Zielkennung wird abgelehnt')
    finally:
        _sh141.rmtree(_ordner141, ignore_errors=True)

    # Die Antwortkurve — die Rechnung hinter der gezeichneten Linie.
    # ⚠ Geprüft werden nur Punkte, deren Wert sich von Hand nachrechnen lässt.
    # Eine falsch rechnende Kurve sieht immer noch nach einer Kurve aus; das
    # Bild taugt hier nicht als Beleg.
    def _gl141(ist, soll):
        return abs(ist - soll) < 1e-9

    pruefe(_gl141(_kv141.antwort(0.0), 0.0) and _gl141(_kv141.antwort(1.0), 1.0)
           and _gl141(_kv141.antwort(-1.0), -1.0),
           'ohne Einstellungen ist die Kurve die Gerade')
    pruefe(_gl141(_kv141.antwort(0.05, totzone=0.1), 0.0)
           and _gl141(_kv141.antwort(0.55, totzone=0.1), 0.5),
           'die Totzone schneidet ab und spannt den Rest neu auf')
    pruefe(_gl141(_kv141.antwort(0.5, saettigung=0.5), 1.0)
           and _gl141(_kv141.antwort(0.25, saettigung=0.5), 0.5),
           'ab der Sättigung gilt Vollausschlag')
    pruefe(_gl141(_kv141.antwort(0.5, exponent=2.0), 0.25),
           'der Exponent macht die Mitte feiner')
    pruefe(_gl141(_kv141.antwort(0.5, 0.1, 0.9, 2.0), 0.25),
           'alle drei zusammen, von Hand nachgerechnet')
    pruefe(_gl141(_kv141.antwort(0.95, totzone=0.9, saettigung=0.2), 1.0),
           'Sättigung unter der Totzone knallt nicht (keine Division durch 0)')
    pruefe(_gl141(_kv141.antwort(5.0), 1.0),
           'eine Eingabe über 1 wird begrenzt')
    _knick141 = [(0.0, 0.0), (0.5, 0.1), (1.0, 1.0)]
    pruefe(_gl141(_kv141.antwort(0.5, kurve=_knick141), 0.1)
           and _gl141(_kv141.antwort(0.25, kurve=_knick141), 0.05),
           'gesetzte Kurvenpunkte gewinnen über den Exponenten')
    # ⚠⚠ Die gefährlichste Verwechslung im ganzen Bereich: „nicht gesetzt"
    # ist bei der Sättigung **1,0**, nicht 0. Ein Regler, der bei fehlender
    # Sättigung auf 0 stünde, schriebe beim ersten Anfassen einen Wert, nach
    # dem der Stick fast nicht mehr steuert. Die Oberfläche holt sich den
    # Ruhewert aus dieser Tabelle — deshalb wird sie hier festgenagelt.
    pruefe(_kv141.STANDARD['saturation'] == 1.0
           and _kv141.STANDARD['deadzone'] == 0.0,
           '* Ruhewerte: Sättigung 1,0 und Totzone 0,0')
    pruefe(_gl141(_kv141.antwort(1.0, saettigung=None), 1.0)
           and _gl141(_kv141.antwort(0.5, saettigung=None), 0.5),
           'eine fehlende Sättigung rechnet wie 1,0, nicht wie 0')

    _voll141 = _kv141.verlauf(schritte=10, ganz=True)
    _quad141 = _kv141.verlauf(schritte=10)
    pruefe(_gl141(_quad141[0][0], 0.0) and _gl141(_voll141[0][0], -1.0)
           and _gl141(_voll141[len(_voll141) // 2][1], 0.0),
           'Quadrant und Vollansicht decken ihren Bereich ab')

    print()
    print('141. Die Werksausstattung wird vollstaendig gelesen')
    # ⚠⚠ **Diese Pruefung legt sich ihre Daten selbst hin.** Erkuls Schiffe
    # liegen als heruntergeladener Zwischenspeicher im Ablageordner; im
    # Wegwerf-Ordner des Selbsttests gibt es keinen. Eine Pruefung, die sich
    # dann ueberspringt, prueft nie etwas — genau der Fehler, der bei
    # Pruefung 67 monatelang unbemerkt blieb.
    #
    # Nachgebaut ist der Aufbau der Cutlass Black, gemessen am 06.09.2026:
    # ein fester Turm mit tauschbaren Kindern, ein Rack mit Raketen, und
    # zweimal derselbe Steckplatzname auf verschiedenen Ebenen.
    from scbp import erkul as _erk141

    _schiff141 = {
        'vehicle': {'hardpoints': [
            {'name': 'hardpoint_cooler_left', 'minSize': 2, 'maxSize': 2,
             'accepts': [{'type': 'Cooler'}], 'flags': {}},
            {'name': 'hardpoint_battery', 'minSize': 1, 'maxSize': 1,
             'accepts': [{'type': 'Battery'}], 'flags': {}},
            {'name': 'hardpoint_turret', 'minSize': 5, 'maxSize': 5,
             'accepts': [{'type': 'TurretBase'}],
             'flags': {'uneditable': True}},
        ]},
        'slots': [
            {'kind': 'swappable', 'portName': 'hardpoint_cooler_left',
             'item': {'ref': 'ref-coldsnap', 'type': 'Cooler', 'size': 2,
                      'i18n': {'name': 'ColdSnap'}}},
            # Ab Werk leer — und trotzdem ein Steckplatz, in den etwas gehoert.
            {'kind': 'swappable', 'portName': 'hardpoint_battery'},
            # ⚠ Fest, aber mit tauschbaren Kindern: die Turmwaffen.
            {'kind': 'fixed', 'portName': 'hardpoint_turret',
             'item': {'ref': 'ref-turmbasis', 'type': 'TurretBase', 'size': 5,
                      'i18n': {'name': 'Manned Turret'}},
             'children': [
                 {'kind': 'swappable', 'portName': 'hardpoint_weapon_left',
                  'item': {'ref': 'ref-gimbal', 'type': 'Turret', 'size': 3,
                           'i18n': {'name': 'VariPuck'}},
                  'children': [
                      {'kind': 'swappable', 'portName': 'hardpoint_class_2',
                       'item': {'ref': 'ref-panther', 'type': 'WeaponGun',
                                'size': 3,
                                'i18n': {'name': 'CF-337 Panther'}}}]},
                 {'kind': 'swappable', 'portName': 'hardpoint_weapon_right',
                  'item': {'ref': 'ref-gimbal', 'type': 'Turret', 'size': 3,
                           'i18n': {'name': 'VariPuck'}},
                  'children': [
                      {'kind': 'swappable', 'portName': 'hardpoint_class_2',
                       'item': {'ref': 'ref-panther', 'type': 'WeaponGun',
                                'size': 3,
                                'i18n': {'name': 'CF-337 Panther'}}}]}]},
        ]}

    _hp141 = {}
    _erk141._hardpoint_verzeichnis(_schiff141['vehicle']['hardpoints'], _hp141)
    _slots141 = []
    _erk141._ausstattung_sammeln(_schiff141['slots'], _hp141, _slots141)
    _pfade141 = [s['pfad'] for s in _slots141]

    # Kuehler + Batterie + 2 Gimbal + 2 Waffen = 6. Der feste Turm selbst
    # zaehlt nicht mit.
    pruefe(len(_slots141) == 6,
           '6 tauschbare Steckplaetze gefunden (bekam: %d)' % len(_slots141))

    # ⚠⚠ **Der Kern: ein fester Platz darf die Rekursion nicht beenden.**
    # Genau hier gingen bei der Cutlass Black die Turmwaffen verloren.
    pruefe('hardpoint_turret/hardpoint_weapon_left/hardpoint_class_2'
           in _pfade141,
           'die Waffe im FESTEN Turm wird gefunden')

    # ⚠⚠ **Und der zweite Kern: gleiche Namen duerfen sich nicht ueberschreiben.**
    pruefe(len([p for p in _pfade141 if p.endswith('hardpoint_class_2')]) == 2,
           'zweimal derselbe Steckplatzname bleiben zwei Plaetze')
    pruefe(len(set(_pfade141)) == len(_pfade141),
           'jeder Pfad kommt genau einmal vor')

    # Ab Werk leer ist ein gueltiger Zustand, kein Grund zum Weglassen.
    _batterie141 = [s for s in _slots141 if s['pfad'] == 'hardpoint_battery']
    pruefe(len(_batterie141) == 1 and not _batterie141[0].get('werk'),
           'ein ab Werk leerer Platz bleibt in der Liste, ohne Werksteil')

    # Zusammengefasste Werksausstattung: 2 Gimbal, 2 Waffen, 1 Kuehler.
    _werk141 = {}
    for _s141 in _slots141:
        _w141 = (_s141.get('werk') or {}).get('ref')
        if _w141:
            _werk141[_w141] = _werk141.get(_w141, 0) + 1
    pruefe(_werk141 == {'ref-coldsnap': 1, 'ref-gimbal': 2, 'ref-panther': 2},
           'Werksteile werden ueber die Kennung gezaehlt (bekam: %s)'
           % (_werk141,))

    # ⚠ GEGENPROBE 1 — mit eingebautem Fehler: Bricht die Rekursion an einem
    # festen Platz ab, muessen die Turmwaffen fehlen. Tut sie es nicht, prueft
    # die Pruefung oben nichts.
    _nur_oben141 = []
    for _s141 in _schiff141['slots']:
        if _s141.get('kind') == 'swappable':
            _e141 = _erk141._ein_slot(_s141, _hp141, _s141['portName'])
            if _e141:
                _nur_oben141.append(_e141)
    pruefe(len(_nur_oben141) == 2,
           'Gegenprobe: ohne Rekursion bleiben nur 2 statt 6 Plaetze '
           '(bekam: %d)' % len(_nur_oben141))

    # ⚠ GEGENPROBE 2: Nach dem blossen Namen geschluesselt, fallen die beiden
    # gleichnamigen Waffenplaetze zu einem zusammen.
    _nach_name141 = {}
    for _s141 in _slots141:
        _nach_name141[_s141['pfad'].split('/')[-1]] = _s141
    pruefe(len(_nach_name141) == 5,
           'Gegenprobe: nach blossem Namen geschluesselt geht ein Platz '
           'verloren (bekam: %d)' % len(_nach_name141))

    print()
    print('142. Der Warenkorb haelt seine vier Zustaende auseinander')
    # ⚠⚠ **Die teuerste Verwechslung dieses Projekts.** „keine Daten" und
    # „nichts zu tun" sehen im Code gleich aus — beides ist eine leere Liste.
    # Am 06.09.2026 stand deshalb bei jedem Bauplan „passt in keines deiner
    # Schiffe", auch wenn nur die Steckplatz-Daten fehlten.
    from scbp import warenkorb as _wk142

    _abgelegt142 = {'spielversion': 'probe', 'hersteller': {}, 'schiffe': {
        'probeschiff': {'name': 'Probeschiff', 'id': 'probe_schiff',
                        'plaetze': [], 'slots': _slots141}}}
    _echt142 = _erk141.laden
    _erk141.laden = lambda: _abgelegt142
    try:
        _mein142 = {'name': 'Probeschiff', 'hersteller': '', 'kurz': '',
                    'hkurz': '', 'belegung': {}}

        # 1. Schiff ohne Steckplatz-Daten -> KEINE_DATEN, niemals NICHTS_OFFEN.
        _z142, _l142 = _wk142.posten({'name': 'Gibtesnicht'})
        pruefe(_z142 == _wk142.KEINE_DATEN,
               'ein unbekanntes Schiff meldet KEINE_DATEN (bekam: %s)' % _z142)

        # 2. Auslegung = Werksausstattung -> NICHTS_OFFEN.
        _z142, _l142 = _wk142.posten(_mein142)
        pruefe(_z142 == _wk142.NICHTS_OFFEN,
               'ohne Aenderung meldet der Korb NICHTS_OFFEN (bekam: %s)'
               % _z142)

        # ⚠⚠ **Der eigentliche Punkt: die beiden duerfen NICHT gleich sein.**
        pruefe(_wk142.KEINE_DATEN != _wk142.NICHTS_OFFEN,
               '„keine Daten" und „nichts offen" sind verschiedene Zustaende')

        # 3. Das Werksteil selbst einlegen ist KEINE Aenderung.
        _wk142.setzen(_mein142, 'hardpoint_cooler_left', 'ref-coldsnap',
                      'ColdSnap')
        _z142, _l142 = _wk142.posten(_mein142)
        pruefe(_z142 == _wk142.NICHTS_OFFEN and not _l142,
               'das Werksteil einzulegen erzeugt keinen Posten')

        # 4. Ein anderes Teil -> ein Posten, mit dem Werksteil daneben.
        _wk142.setzen(_mein142, 'hardpoint_cooler_left', 'ref-blastchill',
                      'BlastChill')
        _wk142.setzen(_mein142, 'hardpoint_battery', 'ref-akku', 'Akku')
        _z142, _l142 = _wk142.posten(_mein142)
        pruefe(_z142 == _wk142.OFFEN and len(_l142) == 2,
               'zwei Aenderungen ergeben zwei Posten (bekam: %d)' % len(_l142))
        _kuehler142 = [p for p in _l142 if p['pfad'] == 'hardpoint_cooler_left']
        pruefe(_kuehler142 and _kuehler142[0]['werk_name'] == 'ColdSnap',
               'am Posten steht, was stattdessen ab Werk drinsteckt')
        _akku142 = [p for p in _l142 if p['pfad'] == 'hardpoint_battery']
        pruefe(_akku142 and not _akku142[0]['werk_ref'],
               'ein ab Werk leerer Platz wird zum Posten ohne Werksteil')

        # 5. Ein Steckplatz, den es nicht mehr gibt, wird uebergangen —
        # nach einem Patch moeglich, und der Spieler kann nichts dafuer.
        _wk142.setzen(_mein142, 'hardpoint_gibtsnichtmehr', 'ref-x', 'X')
        _z142, _l142 = _wk142.posten(_mein142)
        pruefe(len(_l142) == 2,
               'ein verschwundener Steckplatz erzeugt keinen Posten '
               '(bekam: %d)' % len(_l142))

        # ⚠ GEGENPROBE: Wuerde `posten()` bei fehlenden Daten eine leere Liste
        # mit NICHTS_OFFEN zurueckgeben, waere der Unterschied weg — und die
        # Anzeige saehe fuer beide Faelle gleich aus.
        _leer142 = _wk142.posten({'name': 'Gibtesnicht'})
        pruefe(_leer142 == (_wk142.KEINE_DATEN, []),
               'Gegenprobe: fehlende Daten geben KEINE_DATEN mit leerer Liste')
    finally:
        _erk141.laden = _echt142

    print()
    print('143. Die Kaufroute nimmt Deckung vor Preis')
    # ⚠⚠ **Wer stur den billigsten Laden je Posten nimmt, schickt den Spieler
    # durch drei Systeme.** Rechnerisch das Beste, in der Praxis ein verlorener
    # Abend. Diese Pruefung haelt fest, dass die Route nach Deckung waehlt.
    _posten143 = [
        {'pfad': 'a', 'name': 'Teil A', 'weg': _wk142.KAUFEN, 'ref': 'r-a'},
        {'pfad': 'b', 'name': 'Teil B', 'weg': _wk142.KAUFEN, 'ref': 'r-b'},
        {'pfad': 'c', 'name': 'Teil C', 'weg': _wk142.KAUFEN, 'ref': 'r-c'},
        # ⚠ Dieser wird gebaut, gehoert also NICHT auf die Kaufroute.
        {'pfad': 'd', 'name': 'Teil D', 'weg': _wk142.BAUEN, 'ref': 'r-d'},
    ]
    # Ein Ort fuehrt alles (etwas teurer), ein zweiter nur eines (billiger).
    _regale143 = {
        'r-a': [{'laden': 'Alles-Laden', 'ort': 'Area18', 'system': 'Stanton',
                 'preis': 1000.0},
                {'laden': 'Billig', 'ort': 'Weit weg', 'system': 'Pyro',
                 'preis': 700.0}],
        'r-b': [{'laden': 'Alles-Laden', 'ort': 'Area18', 'system': 'Stanton',
                 'preis': 1000.0}],
        'r-c': [{'laden': 'Nebenan', 'ort': 'Area18', 'system': 'Stanton',
                 'preis': 500.0}],
        'r-d': [{'laden': 'Egal', 'ort': 'Nirgendwo', 'system': 'Pyro',
                 'preis': 9999.0}],
    }
    from scbp import laeden as _ld143
    _echt143 = _ld143.laeden
    _ld143.laeden = lambda k: _regale143.get(k)
    try:
        _stopps143, _ohne143 = _wk142.route(_posten143)
        pruefe(len(_stopps143) == 1,
               'alles an einem Ort ergibt EINEN Stopp (bekam: %d)'
               % len(_stopps143))
        pruefe(_stopps143 and _stopps143[0]['ort'] == 'Area18',
               'gewaehlt wird der Ort mit der groessten Deckung')
        # ⚠ Ein Stopp, zwei Laeden — genau die Unterscheidung, die erkul mit
        # „1 shop · 1 stop" trifft.
        pruefe(_stopps143 and len(_stopps143[0]['laeden']) == 2,
               'zwei Laeden am selben Ort bleiben EIN Stopp')
        _summe143 = _wk142.route_summe(_stopps143)
        pruefe(_summe143['gesamt'] == 2500.0,
               'die Routensumme rechnet mit den Laeden der Route '
               '(bekam: %s)' % _summe143['gesamt'])

        # ⚠⚠ **Der zu bauende Posten darf NICHT auf der Kaufroute stehen.**
        _alle_pfade143 = [e['pfad'] for s in _stopps143 for e in s['posten']]
        pruefe('d' not in _alle_pfade143,
               'ein selbst gebauter Posten steht nicht auf der Kaufroute')

        # ⚠ GEGENPROBE: Nach dem billigsten Preis je Posten gewaehlt, waeren es
        # ZWEI Stopps in zwei Systemen — und in Summe nicht einmal viel
        # billiger. Genau das soll die Route nicht tun.
        _billigste143 = set()
        for _p143 in _posten143:
            if _p143['weg'] != _wk142.KAUFEN:
                continue
            _z143 = min(_regale143[_p143['ref']], key=lambda z: z['preis'])
            _billigste143.add((_z143['system'], _z143['ort']))
        pruefe(len(_billigste143) == 2,
               'Gegenprobe: stur nach Preis waeren es 2 Stopps (bekam: %d)'
               % len(_billigste143))

        # Ein Posten, den kein Laden fuehrt, wird benannt statt verschwiegen.
        _ohne_laden143 = [{'pfad': 'x', 'name': 'Teil X',
                           'weg': _wk142.KAUFEN, 'ref': 'r-unbekannt'}]
        _s143, _o143 = _wk142.route(_ohne_laden143)
        pruefe(_s143 == [] and _o143 == ['x'],
               'ein Posten ohne Laden wird als solcher gemeldet')
    finally:
        _ld143.laeden = _echt143

    print()
    print('144. Jeder Prüftext lässt sich unter Windows ausgeben')
    # ⚠⚠ **Diese Prüfung gibt es, weil der Bau daran gescheitert ist.**
    # Am 06.09.2026 brach der Selbsttest unter Windows mitten im Lauf ab:
    #
    #     UnicodeEncodeError: 'charmap' codec can't encode character
    #     '\u2192' in position 42
    #
    # Ein Pfeil `→` in einem Prüftext. Die Windows-Konsole schreibt in cp1252,
    # und was dort fehlt, lässt sich nicht ausgeben — der Lauf endet mit einer
    # Ausnahme, nicht mit einem roten Haken. Auf Linux fällt das **nie** auf,
    # weil dort UTF-8 gilt.
    #
    # ⚠ Betroffen sind nur die **Ausgabetexte**. In Kommentaren und in
    # Prüfdaten darf jedes Zeichen stehen; die werden nie gedruckt.
    with open(os.path.join(WURZEL, 'tools', 'selbsttest.py'),
              encoding='utf-8') as _f144:
        _roh144 = _f144.read()
    # ⚠⚠ **Über den Syntaxbaum, nicht zeilenweise.** Die erste Fassung sah nur
    # Zeilen an, in denen `pruefe(` steht — und ging genau an dem Fall vorbei,
    # der den Bau abgebrochen hatte: Der Text stand in der **Fortsetzungszeile**
    # darunter. Sie meldete „keine" und war grün, während Windows weiter
    # abbrach. Dieselbe Lehre wie bei der ast-Falle in Prüfung 138.
    import ast as _ast144
    _schlimm144 = []
    for _knoten144 in _ast144.walk(_ast144.parse(_roh144)):
        if not (isinstance(_knoten144, _ast144.Call)
                and isinstance(_knoten144.func, _ast144.Name)
                and _knoten144.func.id in ('pruefe', 'print')):
            continue
        for _teil144 in _ast144.walk(_knoten144):
            if isinstance(_teil144, _ast144.Constant) \
                    and isinstance(_teil144.value, str):
                try:
                    _teil144.value.encode('cp1252')
                except UnicodeEncodeError:
                    _schlimm144.append(_teil144.lineno)
    pruefe(not _schlimm144,
           'kein Sonderzeichen in einem Ausgabetext (Zeilen: %s)'
           % (_schlimm144[:5] or 'keine'))

    # Gegenprobe: Ein Pfeil in einem Ausgabetext muss auffallen.
    _probe144 = "    pruefe(True, 'a -> b')".replace('->', '\u2192')
    _faellt144 = False
    try:
        _probe144.encode('cp1252')
    except UnicodeEncodeError:
        _faellt144 = True
    pruefe(_faellt144, 'Gegenprobe: ein Pfeil im Prüftext faellt auf')

    # ------------------------------------------------------------------
    # 147. Zwei Sticks über Kreuz tauschen, und Gerätesätze
    #
    # Baut sich die Datei selbst — dieselbe Begründung wie bei 142.
    print()
    print('147. Bindings tauschen und Gerätesätze')
    import shutil as _sh145
    import tempfile as _tf145
    from scbp import geraetesatz as _gs145
    from scbp import joysticks as _js145
    from scbp import kurven as _kv145

    _A145 = 'AAAA1111-0000-0000-0000-504944564944'
    _B145 = 'BBBB2222-0000-0000-0000-504944564944'
    _xml145 = (
        '<ActionMaps version="1" optionsVersion="2" rebindVersion="2" '
        'profileName="default">\n'
        ' <CustomisationUIHeader label="test">\n'
        '  <deviceoptions name="Links  {%s}">\n'
        '   <option input="x" deadzone="0.1"/>\n'
        '   <option input="x" saturation="0.8"/>\n'
        '  </deviceoptions>\n'
        '  <deviceoptions name="Rechts  {%s}">\n'
        '   <option input="x" deadzone="0.2"/>\n'
        '  </deviceoptions>\n'
        ' </CustomisationUIHeader>\n'
        ' <options type="joystick" instance="1" Product="Links  {%s}">\n'
        '  <flight_move_pitch exponent="1.5"/>\n'
        ' </options>\n'
        ' <options type="joystick" instance="2" Product="Rechts  {%s}"/>\n'
        ' <ActionProfiles>\n'
        '  <actionmap name="spaceship_general">\n'
        '   <action name="v_eject"><rebind input="js1_button1"/></action>\n'
        '   <action name="v_brake"><rebind input="js2_button2"/></action>\n'
        '  </actionmap>\n'
        ' </ActionProfiles>\n'
        '</ActionMaps>\n'
    ) % (_A145, _B145, _A145, _B145)

    _o145 = _tf145.mkdtemp(prefix='scbp-tausch-')
    _d145 = os.path.join(_o145, 'actionmaps.xml')
    _heim145 = os.environ.get('SC_BP_HOME')
    try:
        with open(_d145, 'w', encoding='utf-8') as _f145:
            _f145.write(_xml145)
        # ⚠ Die Gerätesätze landen in einer eigenen Datei im Ablageordner.
        # Ohne eigenes `SC_BP_HOME` schriebe die Prüfung in die echten
        # Einstellungen des Entwicklers — genau das darf ein Selbsttest nie.
        os.environ['SC_BP_HOME'] = _o145

        def _produkte145():
            with open(_d145, encoding='utf-8') as _f:
                _t = _f.read()
            _h = {}
            for _m in re.finditer(
                    r'<options\b[^>]*type="joystick"[^>]*instance="(\d+)"'
                    r'[^>]*Product="([^"]*)"', _t):
                _k = re.search(r'\{([0-9A-Fa-f-]+)\}', _m.group(2))
                _h[int(_m.group(1))] = _k.group(1).upper() if _k else ''
            return _h

        def _totzone145(kennung):
            for _b in _kv145.geraete_achsen(datei=_d145):
                if _b['kennung'] == kennung:
                    return (_b['achsen'].get('x') or {}).get('deadzone')
            return None

        _vor145 = _produkte145()
        _tz_a145, _tz_b145 = _totzone145(_A145), _totzone145(_B145)
        _ok145, _m145, _n145 = _js145.belegungen_tauschen(_A145, _B145,
                                                          datei=_d145)
        pruefe(_ok145, 'Tausch gelingt (%s)' % ('ok' if _ok145 else _m145))
        _nach145 = _produkte145()
        pruefe(_nach145.get(1) == _B145 and _nach145.get(2) == _A145,
               '*die Kennungen stehen über Kreuz')
        with open(_d145, encoding='utf-8') as _f145:
            _inh145 = _f145.read()
        pruefe(_inh145.count('<rebind') == 2
               and 'js1_button1' in _inh145 and 'js2_button2' in _inh145,
               '*KEINE Belegungszeile wurde angefasst')
        pruefe(_totzone145(_A145) == _tz_a145
               and _totzone145(_B145) == _tz_b145,
               '*die Totzonen blieben beim physischen Gerät')
        _js145.belegungen_tauschen(_A145, _B145, datei=_d145)
        pruefe(_produkte145() == _vor145,
               'zweimal tauschen ergibt wieder den Anfang')
        pruefe(not _js145.belegungen_tauschen(_A145, _A145, datei=_d145)[0],
               'ein Gerät mit sich selbst wird abgelehnt')
        pruefe(not _js145.belegungen_tauschen(_A145, 'FFFF9999',
                                              datei=_d145)[0],
               'eine unbekannte Kennung wird abgelehnt')

        # --- Gerätesätze ---
        pruefe(_gs145.speichern('Satz', datei=_d145)[0], 'ein Satz lässt sich anlegen')
        pruefe(not _gs145.speichern('Satz', datei=_d145)[0],
               'derselbe Name nicht zweimal')
        pruefe(_gs145.speichern('Satz', ueberschreiben=True, datei=_d145)[0],
               'mit ausdrücklicher Erlaubnis schon')
        pruefe(not _gs145.speichern('', datei=_d145)[0],
               'ein leerer Name wird abgelehnt')

        _kv145.setzen(_A145, 'x', 'deadzone', 0.5, datei=_d145)
        _schreibt145, _fehlt145 = _gs145.vorschau('Satz', datei=_d145)
        pruefe(len(_schreibt145) == 1,
               'die Vorschau kündigt genau eine Änderung an (sind: %d)'
               % len(_schreibt145))
        pruefe(not _fehlt145, 'kein Gerät fehlt')
        _ok145, _m145, _n145 = _gs145.anwenden('Satz', datei=_d145)
        pruefe(_ok145 and _totzone145(_A145) == 0.1,
               '*der Satz stellt den alten Wert wieder her')

        # ⭐ Ein Satz muss auch LÖSCHEN: Ein Wert, den er nicht kennt, darf
        # nach dem Anwenden nicht stehenbleiben — sonst sind zwei Zustände,
        # die gleich heißen, eben nicht gleich.
        _kv145.setzen(_B145, 'x', 'saturation', 0.66, datei=_d145)
        _gs145.anwenden('Satz', datei=_d145)
        _satB145 = None
        for _b in _kv145.geraete_achsen(datei=_d145):
            if _b['kennung'] == _B145:
                _satB145 = (_b['achsen'].get('x') or {}).get('saturation')
        pruefe(_satB145 is None,
               '*ein fremder Wert wird entfernt (ist: %r)' % _satB145)

        # ⚠ Werte dürfen sich beim Schreiben NICHT verändern. `%g` kürzte
        # 0.098999992 auf 0.099 — ein anderer Wert an einer Achse, die
        # niemand angefasst hat.
        _kv145.setzen(_A145, 'x', 'deadzone', 0.098999992, datei=_d145)
        pruefe(_totzone145(_A145) == 0.098999992,
               '*ein geschriebener Wert kommt unverändert zurück (ist: %r)'
               % _totzone145(_A145))

        _fehlend145 = _gs145.satz('Satz')
        pruefe(_fehlend145 and len(_fehlend145.get('geraete') or {}) == 2,
               'der Satz kennt beide Geräte')
        pruefe(_gs145.loeschen('Satz')[0] and not _gs145.saetze(),
               'löschen räumt den Satz weg')
        pruefe(not _gs145.anwenden('Gibt es nicht', datei=_d145)[0],
               'ein unbekannter Satz wird abgelehnt')

        # ⚠⚠ **Kein Dialog des Betriebssystems in den neuen Seiten.**
        #
        # `tkinter.messagebox` öffnet einen weißen Kasten mit grauen Knöpfen
        # mitten in einem dunklen, deutschen Fenster. Für die Bergung wurde
        # das schon einmal behoben; beim Bau der Achsen- und
        # Blickwinkel-Seite ist es trotzdem wieder hineingerutscht und fiel
        # erst auf einem Bildschirmfoto auf: *„ALLE Fenster müssen IMMER so
        # aussehen wie das Design, das das Tool hat."*
        #
        # `frage_stellen()` gibt es genau dafür. Diese Wache hält fest, dass
        # die neuen Seiten es auch benutzen.
        # ⭐ Alte Geräte-Einträge wegräumen. Ohne das wird man den Hinweis
        # „diese Einstellungen wirken nicht mehr" nie los — Star Citizen legt
        # bei jeder neuen Kennung einen weiteren Block an und räumt nie auf.
        # ⚠ Gibt es nichts wegzuräumen, meldet die Funktion das als
        # „nichts zu tun" — und NICHT als Erfolg mit null Treffern. Die
        # Oberfläche zeigt daraus einen Hinweis statt einer Rückfrage über
        # null Einträge.
        _ok145, _m145, _zahl145 = _kv145.aufraeumen(datei=_d145,
                                                    nur_zaehlen=True)
        pruefe(not _ok145 and _m145 == 's_gs_f_nichts_zu_tun',
               'ohne tote Eintraege wird "nichts zu tun" gemeldet')

        # Einen toten Block hinzufügen: gleiche Bauart, Kennung kennt niemand.
        with open(_d145, encoding='utf-8') as _f145:
            _t145 = _f145.read()
        _t145 = _t145.replace(
            ' </CustomisationUIHeader>',
            '  <deviceoptions name="Altgeraet  {DDDD4444-0000-0000-0000-5049}">\n'
            '   <option input="x" deadzone="0.9"/>\n'
            '  </deviceoptions>\n </CustomisationUIHeader>')
        with open(_d145, 'w', encoding='utf-8') as _f145:
            _f145.write(_t145)

        _ok145, _m145, _zahl145 = _kv145.aufraeumen(datei=_d145,
                                                    nur_zaehlen=True)
        pruefe(_ok145 and _zahl145 == 1,
               'der tote Eintrag wird gezählt (%d)' % _zahl145)
        _lebend145 = [b['kennung'] for b in _kv145.geraete_achsen(datei=_d145)
                      if b['aktiv']]
        _ok145, _m145, _zahl145 = _kv145.aufraeumen(datei=_d145)
        pruefe(_ok145 and _zahl145 == 1,
               'aufräumen entfernt genau einen Block (%d)' % _zahl145)
        _danach145 = _kv145.geraete_achsen(datei=_d145)
        pruefe([b['kennung'] for b in _danach145 if b['aktiv']] == _lebend145,
               '* die lebenden Geräte sind alle noch da')
        pruefe(all('DDDD4444' not in b['kennung'] for b in _danach145),
               '* der tote Block ist weg')
        with open(_d145, encoding='utf-8') as _f145:
            _nach145 = _f145.read()
        pruefe(_nach145.count('<rebind') == 2,
               'die Belegungen haben das Aufräumen überlebt')
        pruefe('</CustomisationUIHeader>' in _nach145
               and '</ActionMaps>' in _nach145,
               'die Datei ist noch vollständig')

        import inspect as _ins145
        from scbp import seiten as _st145
        for _name145 in ('_achsen', '_blickwinkel'):
            _quelle145 = _ins145.getsource(getattr(_st145, _name145))
            pruefe('from tkinter import messagebox' not in _quelle145,
                   '*%s öffnet keinen System-Dialog' % _name145)
            pruefe('frage_stellen' in _quelle145,
                   '%s benutzt den Dialog im Programmstil' % _name145)
    finally:
        if _heim145 is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _heim145
        _sh145.rmtree(_o145, ignore_errors=True)

    # ------------------------------------------------------------------
    # 148. Blickwinkel: Bildschirm ausmessen, Sitzabstand bewerten
    #
    # Reine Rechnung, keine Fremddaten — die Prüfung läuft überall gleich.
    # Geprüft wird an Punkten, die sich von Hand nachrechnen lassen.
    #
    # ⚠ Nummer 148 — siehe die Erklärung bei 146.
    print()
    print('148. Blickwinkel und Sitzabstand')
    from scbp import fov as _fv143

    def _gl143(ist, soll, toleranz=1e-6):
        return ist is not None and abs(ist - soll) < toleranz

    # Ein Bildschirm, der genau so breit ist wie der Abstand: Der Winkel muss
    # 2·arctan(0,5) = 53,13° sein. Von Hand nachrechenbar.
    pruefe(_gl143(_fv143.blickwinkel(1000, 1000), 53.13010235, 1e-6),
           'gleich breit wie weit -> 53,13°')
    # Der klassische rechte Winkel: Breite = 2 × Abstand → 90°.
    pruefe(_gl143(_fv143.blickwinkel(2000, 1000), 90.0, 1e-9),
           'doppelt so breit wie weit -> genau 90°')
    # Hin und zurück muss dasselbe herauskommen.
    _w143 = _fv143.blickwinkel(1193, 900)
    pruefe(_gl143(_fv143.abstand_fuer(1193, _w143), 900.0, 1e-6),
           'Winkel und Abstand rechnen sauber ineinander um')

    # ⚠ Unbrauchbare Eingaben dürfen NICHTS liefern, nicht abstürzen und
    # nicht raten. Ein Rechner, der bei Abstand 0 eine Zahl ausgibt, ist
    # schlimmer als einer, der schweigt.
    pruefe(_fv143.blickwinkel(1193, 0) is None
           and _fv143.blickwinkel(0, 900) is None
           and _fv143.blickwinkel('x', 900) is None,
           'unbrauchbare Eingaben liefern nichts')
    pruefe(_fv143.abstand_fuer(1193, 0) is None
           and _fv143.abstand_fuer(1193, 180) is None,
           'ein Winkel von 0 oder 180 Grad wird abgelehnt')

    # Die Umrechnung waagerecht ↔ senkrecht muss sich aufheben.
    _s143 = _fv143.senkrecht_aus_waagerecht(90.0, 16 / 9)
    pruefe(_gl143(_fv143.waagerecht_aus_senkrecht(_s143, 16 / 9), 90.0, 1e-9),
           'waagerecht und senkrecht rechnen sauber ineinander um')
    # Breiter Bildschirm heißt mehr waagerecht bei gleichem senkrecht.
    pruefe(_fv143.waagerecht_aus_senkrecht(50.0, 32 / 9)
           > _fv143.waagerecht_aus_senkrecht(50.0, 16 / 9),
           'ein breiterer Bildschirm ergibt mehr waagerechten Winkel')

    # Die Kartenmessung.
    pruefe(_gl143(_fv143.mm_pro_pixel(367.2), 85.60 / 367.2, 1e-9),
           'aus der Kartenbreite wird die Pixelgröße')
    pruefe(_gl143(_fv143.bildschirmbreite_mm(5120, 85.60 / 367.2),
                  5120 * 85.60 / 367.2, 1e-6),
           'daraus die Bildschirmbreite')
    pruefe(_fv143.mm_pro_pixel(0) is None
           and _fv143.bildschirmbreite_mm(5120, 0) is None,
           'auch hier liefern unbrauchbare Eingaben nichts')

    # ⭐ Die Ampel. Das Vorzeichen sagt die Richtung — positiv heißt zu weit
    # weg. Wer das verdreht, schickt den Spieler in die falsche Richtung.
    pruefe(_fv143.bewertung(900, 900)[0] == 'gruen',
           'genau am Punkt ist grün')
    pruefe(_fv143.bewertung(940, 900)[0] == 'gruen',
           'ein paar Zentimeter daneben bleibt grün')
    _n143, _a143 = _fv143.bewertung(1050, 900)
    pruefe(_n143 == 'gelb' and _a143 > 0,
           '* deutlich zu weit weg ist gelb, mit positivem Vorzeichen')
    _n143, _a143 = _fv143.bewertung(500, 900)
    pruefe(_n143 == 'rot' and _a143 < 0,
           '* viel zu nah ist rot, mit negativem Vorzeichen')
    pruefe(_fv143.bewertung(900, 0)[0] == 'rot',
           'ein unmöglicher Sollwert wird nicht schöngerechnet')

    # Die Kartenmaße sind eine Norm, kein Schätzwert.
    pruefe(_fv143.KARTE_BREITE_MM == 85.60 and _fv143.KARTE_HOEHE_MM == 53.98,
           'die Kartenmaße entsprechen ISO/IEC 7810 ID-1')

    # ------------------------------------------------------------------
    # 149. Ein Wunschschiff laesst sich ausstatten — ohne Besitz zu werden
    #
    # Zwei Dinge muessen gleichzeitig gelten, und sie ziehen in
    # entgegengesetzte Richtungen:
    #
    # | Es muss gehen | Es darf NICHT gehen |
    # |---|---|
    # | ein Wunschschiff belegen und in den Warenkorb legen | ein Wunschschiff in „passt in dein Schiff" auftauchen |
    #
    # Waeren beide Listen zusammengelegt, faellt der zweite Punkt lautlos um:
    # Das Werkzeug gaebe dann Auskunft ueber ein Schiff, das dem Spieler gar
    # nicht gehoert — und niemandem faellt es auf, weil die Anzeige richtig
    # aussieht. Deshalb steht hier eine Gegenprobe fuer die Trennung.
    #
    # Baut sich die Daten selbst, kein Netz, keine Nutzerdatei.
    print()
    print('149. Wunschschiffe sind ausstattbar, ohne Besitz zu werden')
    from scbp import hangar as _hg149
    from scbp import warenkorb as _wk149

    _daten149 = {'schiffe': [{'name': 'Vulture', 'hersteller': 'Drake',
                              'kurz': 'vulture', 'hkurz': 'DRAK',
                              'belegung': {}}],
                 'wunsch': []}

    pruefe(_hg149.wunsch_hinzufuegen(_daten149, 'Prospector', 'MISC'),
           'ein Wunsch laesst sich eintragen')
    _w149 = _hg149.wunsch_liste(_daten149)[0]
    pruefe(_w149.get('hersteller') == 'MISC',
           'der Hersteller wird mitgespeichert (ohne ihn findet erkul nicht '
           'jedes Schiff)')
    pruefe(isinstance(_w149.get('belegung'), dict),
           'das Feld fuer die Ausstattung liegt leer bereit')

    # Der Warenkorb arbeitet auf dem Wunsch-Eintrag wie auf einem Hangar-Schiff.
    pruefe(_wk149.setzen(_w149, 'hardpoint_power', 'ref-abc', 'Fortitude'),
           'ein Teil laesst sich in einen Steckplatz des Wunschschiffs legen')
    pruefe(_wk149.belegung(_w149).get('hardpoint_power', {}).get('name')
           == 'Fortitude',
           '* und steht danach auch drin')
    pruefe(_wk149.loeschen(_w149, 'hardpoint_power'),
           '* und laesst sich wieder herausnehmen')

    # Die beiden Listen bleiben getrennt.
    _hs149 = _hg149.kennsaetze(_daten149)
    _ws149 = _hg149.wunsch_kennsaetze(_daten149)
    pruefe([s[0] for s in _hs149] == ['Vulture'],
           'die Hangar-Liste enthaelt nur, was der Spieler wirklich hat')
    pruefe([s[0] for s in _ws149] == ['Prospector'],
           'die Wunsch-Liste steht daneben, nicht darin')
    pruefe(not (set(s[0] for s in _hs149) & set(s[0] for s in _ws149)),
           'kein Schiff steht in beiden Listen')

    # Gegenprobe: Waere ein Wunsch faelschlich im Hangar, muesste das auffallen.
    _falsch149 = dict(_daten149)
    _falsch149['schiffe'] = _daten149['schiffe'] + [{'name': 'Prospector',
                                                     'hersteller': 'MISC'}]
    pruefe(bool(set(s[0] for s in _hg149.kennsaetze(_falsch149))
                & set(s[0] for s in _ws149)),
           'Gegenprobe: ein Wunsch im Hangar wuerde als Ueberschneidung '
           'auffallen')

    # ------------------------------------------------------------------
    # 150. Der Geräte-Hub: drei Sichten auf dasselbe Gerät
    #
    # ⚠ Die Quellen werden **gestellt**, nicht gelesen. `eingabe.geraete()`
    # fragt das echte System ab — auf dem Bau-Rechner hängt kein Joystick,
    # und auf dem Entwicklerrechner hingen am Prüftag drei. Beides taugt
    # nicht als Grundlage für eine Prüfung, die überall dasselbe sagen soll.
    print()
    print('150. Geräte-Hub: System, Protokoll und Belegung zusammenführen')
    from scbp import eingabe as _ei150
    from scbp import geraetehub as _hub150
    from scbp import joysticks as _js150

    _A150 = 'AAAA1111-0000-0000-0000-504944564944'
    _B150 = 'BBBB2222-0000-0000-0000-504944564944'
    _C150 = 'CCCC3333-0000-0000-0000-504944564944'
    _D150 = 'DDDD4444-0000-0000-0000-504944564944'

    _echt150 = (_ei150.geraete, _js150.geraete, _js150.zuordnung)
    try:
        # ⭐ Der Aufbau bildet genau die vier Lagen ab, die es geben kann:
        #   A — angesteckt, bekannt, hat eine Nummer        → bereit
        #   B — in der Belegung, aber nicht angesteckt      → abgesteckt
        #   C — angesteckt und bekannt, ohne Nummer         → ohne_nummer
        #   D — angesteckt, das Spiel kennt es gar nicht    → unbekannt
        _ei150.geraete = lambda: [
            {'pfad': '/dev/input/js0', 'name': 'System A', 'kennung': _A150},
            {'pfad': '/dev/input/js1', 'name': 'System C', 'kennung': _C150},
            {'pfad': '/dev/input/js2', 'name': 'System D', 'kennung': _D150},
        ]
        _js150.geraete = lambda ordner=None: [
            {'name': 'Log A', 'kennung': _A150},
            {'name': 'Log B', 'kennung': _B150},
            {'name': 'Log C', 'kennung': _C150},
        ]
        _js150.zuordnung = lambda datei=None, ordner=None: [
            {'nummer': 2, 'name': 'Belegung A', 'kennung': _A150},
            {'nummer': 1, 'name': 'Belegung B', 'kennung': _B150},
        ]

        _u150 = {g['kennung']: g for g in _hub150.uebersicht()}
        pruefe(len(_u150) == 4,
               'alle vier Geräte tauchen genau einmal auf (sind: %d)'
               % len(_u150))
        pruefe(_u150[_A150]['zustand'] == _hub150.BEREIT,
               'angesteckt + bekannt + Nummer = bereit')
        pruefe(_u150[_B150]['zustand'] == _hub150.ABGESTECKT,
               '* die Belegung erwartet es, es ist nicht da = abgesteckt')
        pruefe(_u150[_C150]['zustand'] == _hub150.OHNE_NUMMER,
               '* angesteckt, aber ohne Nummer in der Belegung')
        pruefe(_u150[_D150]['zustand'] == _hub150.UNBEKANNT,
               '* angesteckt, dem Spiel noch nie begegnet')

        # ⚠ Der Name der BELEGUNG gewinnt — den hat der Spieler zuletzt
        # gesehen, und oft selbst vergeben.
        pruefe(_u150[_A150]['name'] == 'Belegung A',
               'der Name aus der Belegung hat Vorrang')
        pruefe(_u150[_C150]['name'] == 'Log C',
               'ohne Belegung gilt der Name aus dem Protokoll')
        pruefe(_u150[_D150]['name'] == 'System D',
               'ohne beides der Name des Systems')

        # ⭐ Der eigentliche Zweck: Die Nummern der beiden Welten sind
        # verschieden, und der Hub hält beide nebeneinander.
        pruefe(_u150[_A150]['nummer'] == 2
               and _u150[_A150]['systempfad'] == '/dev/input/js0',
               '* Spiel-Nummer und System-Pfad stehen nebeneinander')

        _z150 = _hub150.zusammenfassung()
        pruefe((_z150['bereit'], _z150['abgesteckt'], _z150['ohne_nummer'],
                _z150['unbekannt']) == (1, 1, 1, 1),
               'die Zusammenfassung zählt richtig')
        pruefe(not _z150['alles_gut'],
               'mit fehlendem Gerät ist nicht alles gut')

        # Sortierung: was eine Nummer hat, steht vorn und nach Nummer.
        _liste150 = _hub150.uebersicht()
        _nummern150 = [g['nummer'] for g in _liste150 if g['nummer']]
        pruefe(_nummern150 == sorted(_nummern150),
               'die Geräte mit Nummer stehen der Reihe nach vorn')

        # --- Die Wache ---
        _wache150 = _hub150.Wache()
        pruefe(_wache150.pruefen() == ([], []),
               '* der erste Blick meldet nichts (sonst Fehlalarm beim Start)')
        pruefe(_wache150.pruefen() == ([], []),
               'ohne Änderung bleibt es dabei')
        _ei150.geraete = lambda: [
            {'pfad': '/dev/input/js0', 'name': 'System A', 'kennung': _A150},
        ]
        _dazu150, _weg150 = _wache150.pruefen()
        pruefe(not _dazu150 and len(_weg150) == 2,
               '* zwei abgezogene Geräte werden gemeldet (%d)' % len(_weg150))
        _ei150.geraete = lambda: [
            {'pfad': '/dev/input/js0', 'name': 'System A', 'kennung': _A150},
            {'pfad': '/dev/input/js9', 'name': 'Neu', 'kennung': _D150},
        ]
        _dazu150, _weg150 = _wache150.pruefen()
        pruefe(len(_dazu150) == 1 and not _weg150,
               '* ein neu angestecktes Gerät wird gemeldet')
        pruefe(_wache150.pruefen(mindestabstand=60) == ([], []),
               'der Mindestabstand bremst die Abfrage')

        # --- Der Zuordnungs-Assistent ---
        #
        # ⭐ Der Fall, um den es geht: EIN Gerät fehlt, EIN neues steht ohne
        # Nummer da. Dann ist es fast immer derselbe Stick mit neuer Kennung,
        # und ein einziger Handgriff hängt die Belegung um.
        _ei150.geraete = lambda: [
            {'pfad': '/dev/input/js0', 'name': 'Neu', 'kennung': _C150},
        ]
        _js150.geraete = lambda ordner=None: [
            {'name': 'Log C', 'kennung': _C150},
        ]
        _js150.zuordnung = lambda datei=None, ordner=None: [
            {'nummer': 1, 'name': 'Belegung B', 'kennung': _B150},
        ]
        _v150 = _hub150.vorschlaege()
        pruefe(len(_v150) == 1 and _v150[0]['art'] == _hub150.TAUSCH,
               '* eins fehlt, eins ist neu -> Umhaengen vorgeschlagen')
        pruefe(_v150 and _v150[0]['alt']['kennung'] == _B150
               and _v150[0]['geraet']['kennung'] == _C150,
               'der Vorschlag nennt beide Seiten richtig')

        # ⚠⚠ Die Gegenprobe, auf die es ankommt: Bei ZWEI fehlenden Geräten
        # darf NICHT geraten werden. Ein falsch geratener Ersatz vertauscht
        # zwei Sticks, und das merkt man erst im Gefecht.
        _js150.zuordnung = lambda datei=None, ordner=None: [
            {'nummer': 1, 'name': 'Belegung A', 'kennung': _A150},
            {'nummer': 2, 'name': 'Belegung B', 'kennung': _B150},
        ]
        _v150 = _hub150.vorschlaege()
        pruefe(all(v['art'] != _hub150.TAUSCH for v in _v150),
               '* bei zwei fehlenden Geraeten wird NICHT geraten')
        pruefe(len(_v150) == 2
               and all(v['art'] == _hub150.ANSTECKEN for v in _v150),
               'stattdessen steht bei jedem, dass es fehlt (%d)' % len(_v150))

        # Ein Gerät, das das Spiel noch nie gesehen hat, aber auch nichts
        # ersetzt: Da hilft nur, einmal damit zu starten.
        _js150.geraete = lambda ordner=None: []
        _js150.zuordnung = lambda datei=None, ordner=None: []
        _ei150.geraete = lambda: [
            {'pfad': '/dev/input/js0', 'name': 'Frisch', 'kennung': _D150},
        ]
        _v150 = _hub150.vorschlaege()
        pruefe(len(_v150) == 1 and _v150[0]['art'] == _hub150.STARTEN,
               'ein voellig neues Geraet -> einmal starten')

        # Alles in Ordnung heisst: kein Vorschlag.
        _js150.geraete = lambda ordner=None: [
            {'name': 'Log A', 'kennung': _A150}]
        _js150.zuordnung = lambda datei=None, ordner=None: [
            {'nummer': 1, 'name': 'Belegung A', 'kennung': _A150}]
        _ei150.geraete = lambda: [
            {'pfad': '/dev/input/js0', 'name': 'System A', 'kennung': _A150}]
        pruefe(_hub150.vorschlaege() == [],
               '* wenn alles passt, schlaegt der Assistent nichts vor')
    finally:
        _ei150.geraete, _js150.geraete, _js150.zuordnung = _echt150

    print()
    print('151. Die Einkaufsliste rechnet ueber alle Schiffe')
    # ⚠⚠ **Der Kern: Was man HAT, kostet nichts mehr.** Ein Hangar-Schiff
    # gehoert dem Spieler — auf die Rechnung kommen nur die Teile, die ihm
    # fehlen. Ein Wunschschiff kostet dagegen erst sich selbst und dann seine
    # Ausstattung. Wer das verwechselt, stellt dem Spieler sein eigenes Schiff
    # noch einmal in Rechnung.
    from scbp import erkul as _erk151, warenkorb as _wk151
    from scbp import laeden as _ld151, herstellung as _he151
    from scbp import preise as _pr151, schiffe as _sf151

    _slots151 = [
        {'pfad': 'hardpoint_cooler_left', 'art': 'Cooler', 'groesse': 2,
         'werk': {'ref': 'ref-coldsnap', 'name': 'ColdSnap'}},
        {'pfad': 'hardpoint_battery', 'art': 'Battery', 'groesse': 1},
    ]
    _regale151 = {
        'ref-blast': [{'laden': 'Depot', 'ort': 'Area18', 'system': 'Stanton',
                       'preis': 22730.0}],
    }
    _echt151 = (_erk151.laden, _ld151.bekannt, _ld151.laeden,
                _ld151.guenstigster, _wk151._bauplan_verzeichnis,
                _he151.rezept, _pr151.preis, _sf151.kaufen)
    try:
        _erk151.laden = lambda: {'spielversion': 'p', 'hersteller': {},
                                 'schiffe': {
            'cutlassblack': {'name': 'Cutlass Black', 'id': 'drak_cutlass',
                             'plaetze': [], 'slots': _slots151},
            'polaris': {'name': 'Polaris', 'id': 'rsi_polaris',
                        'plaetze': [], 'slots': _slots151}}}
        _ld151.bekannt = lambda k: k in _regale151
        _ld151.laeden = lambda k: _regale151.get(k)
        _ld151.guenstigster = lambda k: (
            (_regale151[k][0]['preis'], _regale151[k][0]['laden'],
             _regale151[k][0]['ort']) if _regale151.get(k) else None)
        _wk151._bauplan_verzeichnis = lambda: {}
        _he151.rezept = lambda n: None
        _pr151.preis = lambda r: None
        # ⚠ `schiffe.kaufen()` gibt eine **Liste** von Verkaufsstellen zurueck,
        # billigste zuerst — kein Tupel wie `laeden.guenstigster()`. Wer das
        # verwechselt, liest den Preis aus einem Zeichen statt aus einer Zahl.
        _sf151.kaufen = lambda n: ([{'stelle': 'Astro Armada', 'ort': 'Area18',
                                     'system': 'Stanton', 'preis': 20250000.0}]
                                   if n == 'Polaris' else [])

        _hangar151 = {'name': 'Cutlass Black', 'hersteller': 'Drake',
                      'kurz': '', 'hkurz': '', 'belegung': {}}
        _wunsch151 = {'name': 'Polaris', 'hersteller': 'RSI', 'belegung': {}}
        # Ein Wunschschiff ohne Steckplatzdaten UND ohne Auslegung.
        _leer151 = {'name': 'Gibtsnochnicht', 'hersteller': '', 'belegung': {}}
        _wk151.setzen(_hangar151, 'hardpoint_cooler_left', 'ref-blast', 'Blast')
        _wk151.setzen(_wunsch151, 'hardpoint_cooler_left', 'ref-blast', 'Blast')
        _daten151 = {'format': 1, 'schiffe': [_hangar151],
                     'wunsch': [_wunsch151, _leer151]}

        _r151 = _wk151.rechnung(_daten151)
        _schiffsposten151 = [p for p in _r151['posten']
                             if p['sorte'] == _wk151.SCHIFF]

        # ⚠⚠ Das Hangar-Schiff darf NICHT als Posten auftauchen.
        pruefe(all(p['quelle'] == _wk151.WUNSCH for p in _schiffsposten151),
               'nur Wunschschiffe stehen selbst auf der Rechnung')
        pruefe(len(_schiffsposten151) == 2,
               'beide Wunschschiffe stehen drauf (bekam: %d)'
               % len(_schiffsposten151))
        pruefe(not any(p['schiff'] == 'Cutlass Black'
                       and p['sorte'] == _wk151.SCHIFF
                       for p in _r151['posten']),
               'das eigene Schiff wird NICHT noch einmal berechnet')

        # Das Wunschschiff bringt seinen Kaufpreis mit.
        _pol151 = [p for p in _schiffsposten151 if p['schiff'] == 'Polaris']
        pruefe(_pol151 and _pol151[0]['kauf']['preis'] == 20250000.0,
               'der Schiffspreis steht am Posten (bekam: %s)'
               % (_pol151[0]['kauf']['preis'] if _pol151 else None))

        # ⚠ Jeder Posten muss sagen, wozu er gehoert — eine Rechnung ohne
        # Position ist eine Zahl.
        pruefe(all(p.get('schiff') for p in _r151['posten']),
               'jeder Posten nennt sein Schiff')
        _teile151 = [p for p in _r151['posten'] if p['sorte'] == _wk151.TEIL]
        pruefe(_teile151 and all(p.get('position') for p in _teile151),
               'jedes Teil nennt seinen Steckplatz')
        pruefe(any(p['position'] == 'Cooler S2' for p in _teile151),
               'die Position traegt Art UND Groesse')

        # Zwei Schiffe tragen denselben Steckplatz — die Posten duerfen sich
        # nicht vermischen.
        _blast151 = [p for p in _teile151 if p['name'] == 'Blast']
        pruefe(sorted(p['schiff'] for p in _blast151)
               == ['Cutlass Black', 'Polaris'],
               'dasselbe Teil an zwei Schiffen bleiben zwei Posten')

        # ⚠ Ein Wunschschiff ohne Auslegung ist kein Mangel.
        pruefe('Gibtsnochnicht' not in _r151['ohne_steckplatzdaten'],
               'ein Schiff ohne Auslegung wird nicht als Luecke gemeldet')

        # Summe: 2× Blast (22.730) + Polaris (20.250.000). Das preislose
        # Wunschschiff zaehlt als offen, nicht als 0.
        pruefe(_r151['summe']['gesamt'] == 20295460.0,
               'die Summe stimmt (bekam: %s)' % _r151['summe']['gesamt'])
        pruefe(_r151['summe']['offen'] == 1,
               'der Posten ohne Preis wird gezaehlt (bekam: %d)'
               % _r151['summe']['offen'])

        # ⚠ GEGENPROBE: Wuerde ein Posten ohne Preis als 0 verrechnet, waere
        # `offen` null — und die Summe saehe vollstaendig aus, obwohl ein
        # Schiff fehlt.
        pruefe(_r151['summe']['offen'] > 0,
               'Gegenprobe: ein preisloser Posten wird NICHT als 0 gerechnet')

        print()
        print('152. Was der Rechnung an Preisen noch fehlt')
        # ⚠⚠ **Ohne diesen Schritt bleibt eine Rechnung auf „wird
        # nachgeschlagen" stehen.** Der Abruf gehoert nicht ins Rechenmodul —
        # zwoelf Posten waeren zwoelf Netzrunden, waehrend die Oberflaeche
        # steht. Also sagt das Modul, WAS fehlt, und die Anzeige holt es nach.
        _offen151 = _wk151.fehlende_preise(_r151['posten'])
        pruefe(_offen151 == [],
               'was schon nachgeschlagen ist, wird nicht erneut gemeldet')

        # Jetzt ein Teil, das noch nie nachgeschlagen wurde.
        _wk151.setzen(_hangar151, 'hardpoint_battery', 'ref-neu', 'Neuteil')
        _r2_151 = _wk151.rechnung(_daten151)
        _offen2_151 = _wk151.fehlende_preise(_r2_151['posten'])
        pruefe([k for k, _n in _offen2_151] == ['ref-neu'],
               'ein ungeprueftes Teil wird gemeldet (bekam: %s)'
               % ([k for k, _n in _offen2_151],))
        pruefe(_offen2_151 and _offen2_151[0][1] == 'Neuteil',
               'der Name kommt mit — `laeden.holen` braucht ihn als Rueckfall')

        # ⚠ Schiffe gehoeren NICHT dazu: Ihre Preise kommen aus `schiffe.py`,
        # nicht aus `laeden.py`. Wer sie mitgibt, schlaegt eine Schiffskennung
        # im Teilekatalog nach und bekommt nie einen Treffer.
        pruefe(not any(k == 'Polaris' for k, _n in _offen2_151),
               'Schiffe stehen nicht in der Nachschlage-Liste')

        # ⚠ GEGENPROBE: Ohne den Zustandsfilter kaeme JEDES Teil zurueck, auch
        # die laengst bekannten — die Anzeige wuerde bei jedem Zeichnen erneut
        # abrufen.
        pruefe(len(_offen2_151) == 1,
               'Gegenprobe: nur das ungepruefte Teil, nicht alle (bekam: %d)'
               % len(_offen2_151))
    finally:
        (_erk151.laden, _ld151.bekannt, _ld151.laeden, _ld151.guenstigster,
         _wk151._bauplan_verzeichnis, _he151.rezept, _pr151.preis,
         _sf151.kaufen) = _echt151

    # ------------------------------------------------------------------
    # 153. Guete und Klasse an der Teileauswahl
    #
    # Man baut ein Schiff auf einen Zweck hin — Tarnung, Kampf, Bergbau. Eine
    # reine Namensliste sagt darueber nichts, und die Namen kennt fast niemand
    # auswendig. Geprueft wird beides, weil beides einzeln nutzlos ist: die
    # Guete ohne Klasse und die Klasse ohne Guete.
    #
    # ⚠ Legt sich die Daten SELBST hin. Der Laden-Katalog ist ein geholter
    # Zwischenspeicher und liegt im Wegwerf-Ordner des Pruflaufs nicht — eine
    # Pruefung, die deshalb ueberspringt, prueft nichts (siehe 67).
    print()
    print('153. Guete und Klasse stehen an der Teileauswahl')
    from scbp import seiten as _st153
    from scbp import sprache as _sp153

    _alt153 = _sp153.aktuelle()
    _vorher153 = _st153._TEIL_VERZEICHNIS[0]
    _st153._TEIL_VERZEICHNIS[0] = {
        'ref-tarn': {'guete': 'A', 'klasse': 'Stealth'},
        'ref-ohne': {'guete': '', 'klasse': ''},
    }
    try:
        _sp153.setzen('de')
        pruefe(_st153._teil_kennzeichen({'guete': 'C', 'klasse': 'Industrial'})
               == 'C · Industrie',
               'Guete und Klasse stehen zusammen, Klasse uebersetzt')
        _sp153.setzen('en')
        pruefe(_st153._teil_kennzeichen({'guete': 'C', 'klasse': 'Industrial'})
               == 'C · Industrial',
               '* und auf Englisch ebenso')
        _sp153.setzen('de')

        # Nur die Kennung bekannt — so liegt es in der Steckplatz-Zeile vor.
        pruefe(_st153._teil_kennzeichen({'kennung': 'ref-tarn'})
               == 'A · Tarnung',
               'aus der blossen Kennung werden beide Angaben nachgeschlagen')

        # Die Guete bleibt ein Buchstabe — sie wird nicht uebersetzt.
        pruefe(_st153._teil_kennzeichen({'guete': 'A'}) == 'A',
               'die Guete allein bleibt der Buchstabe')
        pruefe(_st153._teil_kennzeichen({'klasse': 'Stealth'}) == 'Tarnung',
               'die Klasse allein steht auch fuer sich')

        # Gegenproben: Nichts erfinden, wo nichts ist.
        pruefe(_st153._teil_kennzeichen({'kennung': 'ref-ohne'}) == '',
               'Gegenprobe: ein Teil ohne beide Angaben liefert nichts')
        pruefe(_st153._teil_kennzeichen({'kennung': 'gibt-es-nicht'}) == '',
               'Gegenprobe: eine unbekannte Kennung liefert nichts')
        pruefe(_st153._teil_kennzeichen(None) == '',
               'Gegenprobe: gar kein Teil liefert nichts')

        # ⚠ Und die Gegenprobe zur Pruefung selbst: Wuerde die Klasse NICHT
        # uebersetzt, muesste das hier auffallen.
        pruefe('Stealth' not in _st153._teil_kennzeichen({'klasse': 'Stealth'}),
               'Gegenprobe: die englische Klasse steht nicht im deutschen Text')
    finally:
        _st153._TEIL_VERZEICHNIS[0] = _vorher153
        _sp153.setzen(_alt153)

    # ------------------------------------------------------------------
    # 155. Der Warenkorb-Knopf bleibt sichtbar, und der Ort steht einmal
    #
    # ⚠⚠ Zwei Fehler, die zusammen auftraten und einander verstaerkt haben:
    #
    # 1. Der Preistext wurde VOR dem Knopf gepackt. In tkinter bekommt das
    #    zuerst gepackte Element seinen Platz — ein langer Text mit
    #    `side='left'` schiebt einen spaeter gepackten `side='right'`-Knopf aus
    #    dem Fenster. Der Knopf war da, nur unerreichbar.
    # 2. Der Ort stand doppelt im Text: UEX schreibt ihn schon in den
    #    Ladennamen („Ship Weapons - Pyro Gateway (Stanton)"), und daneben
    #    stand er noch einmal. Das machte den Text erst lang genug, damit der
    #    erste Fehler zuschlug.
    #
    # Folge: Wer einmal auf „Selbst herstellen" gewechselt hatte, kam nie
    # zurueck auf „Kaufen" — ohne Fehlermeldung, der Knopf war schlicht nicht
    # zu sehen. Am 06.09.2026 gemeldet.
    #
    # ⚠⚠ **Geprueft wird die PACK-REIHENFOLGE, nicht die Pixellage.** Die
    # erste Fassung dieser Pruefung mass die rechte Kante des Knopfes in einem
    # echten Fenster. Lokal unter Linux war sie gruen, im Bau-Lauf unter
    # Windows fiel sie mit „rechte Kante None" um: Dort war zum Messzeitpunkt
    # nichts gemappt, der Knopf also gar nicht auffindbar — und die
    # Folgepruefungen fielen mit.
    #
    # Eine Pruefung, die auf einer Plattform falsch rot ist, blockiert den Bau
    # und sagt trotzdem nichts. Die Ursache war ohnehin nie eine Pixelzahl,
    # sondern die Reihenfolge in `pack()`: Der Knopf muss VOR dem Textlabel
    # kommen. Genau das steht in `pack_slaves()` — plattformunabhaengig, ohne
    # sichtbares Fenster.
    print()
    print('155. Der Warenkorb-Knopf steht vor dem Preistext')
    import tkinter as _tk155
    from tkinter import font as _fo155
    from scbp import seiten as _st155
    from scbp import warenkorb as _wk155

    class _F155:
        pass

    def _bauen155(kauf):
        """Baut einen Warenkorb-Posten auf und gibt (zeilen, texte) zurueck."""
        rahmen = _tk155.Frame(wurzel155, bg='#0d1117')
        posten = {'pfad': 'p1', 'name': 'M6A Cannon', 'ref': 'r1',
                  'werk_name': 'CF-447 Rhino Repeater', 'weg': _wk155.BAUEN,
                  'kauf': kauf,
                  'bau': {'zustand': _wk155.BEKANNT, 'material': 17120,
                          'dauer': 2940}}
        _st155._warenkorb_posten(_f155, rahmen, {'name': 'T', 'belegung': {}},
                                 posten, lambda: None)
        wurzel155.update_idletasks()
        texte = []
        zeilen = []

        def geh(w):
            kinder = w.winfo_children()
            # Eine „Zeile" ist ein Rahmen, in dem ein Canvas (der Knopf) und
            # ein Label nebeneinander liegen.
            arten = [k.winfo_class() for k in kinder]
            if 'Canvas' in arten and 'Label' in arten:
                # ⚠ Die Klassen SOFORT einsammeln, nicht die Widgets aufheben:
                # Der Rahmen wird gleich zerstoert, und ein `winfo_class()`
                # danach wirft `bad window path name`.
                zeilen.append([s.winfo_class() for s in w.pack_slaves()])
            for k in kinder:
                try:
                    if k.cget('text'):
                        texte.append(k.cget('text'))
                except Exception:
                    pass
                geh(k)

        geh(rahmen)
        rahmen.destroy()
        return zeilen, texte

    wurzel155 = _tk155.Tk()
    wurzel155.withdraw()
    _f155 = _F155()
    _f155.f_klein = _fo155.Font(family='Calibri', size=10)
    _f155.f_fett = _fo155.Font(family='Calibri', size=10, weight='bold')
    _f155.beim_zeigen = {}
    try:
        _lang155 = {'zustand': _wk155.BEKANNT, 'preis': 160626,
                    'laden': 'Ship Weapons - Pyro Gateway (Stanton)',
                    'ort': 'Pyro Gateway (Stanton)'}
        _zeilen155, _texte155 = _bauen155(_lang155)

        pruefe(len(_zeilen155) == 1,
               'die Zeile mit dem Knopf wird gefunden (bekam: %d)'
               % len(_zeilen155))
        if _zeilen155:
            _arten155 = _zeilen155[0]
            # Erwartet: Label (Beschriftung) · Canvas (Knopf) · Label (Preis)
            pruefe('Canvas' in _arten155,
                   'der Knopf ist in der Zeile (Reihenfolge: %s)'
                   % ' '.join(_arten155))
            if 'Canvas' in _arten155:
                _k155 = _arten155.index('Canvas')
                _spaeter155 = _arten155[_k155 + 1:]
                pruefe('Label' in _spaeter155,
                       '* und wird VOR dem Preistext gepackt — sonst schiebt '
                       'der Text ihn aus dem Fenster')

        # Der Ort darf nur einmal vorkommen.
        _kauf155 = [x for x in _texte155 if 'aUEC bei' in x]
        pruefe(len(_kauf155) == 1, 'die Kaufzeile steht genau einmal da')
        pruefe(bool(_kauf155) and _kauf155[0].count('Pyro Gateway') == 1,
               'der Ort steht einmal im Text, nicht zweimal (bekam: %d)'
               % (_kauf155[0].count('Pyro Gateway') if _kauf155 else -1))

        # Gegenprobe: Ein Ort, der NICHT im Ladennamen steckt, muss dazu.
        _eigen155 = {'zustand': _wk155.BEKANNT, 'preis': 1000,
                     'laden': 'Platinum Bay', 'ort': 'Area18'}
        _zeilen155, _texte155 = _bauen155(_eigen155)
        _kauf155 = [x for x in _texte155 if 'aUEC bei' in x]
        pruefe(bool(_kauf155) and 'Area18' in _kauf155[0],
               'Gegenprobe: ein eigenstaendiger Ort wird weiter genannt')
        pruefe(bool(_kauf155) and 'Platinum Bay' in _kauf155[0],
               '* und der Laden ebenso')
    finally:
        wurzel155.destroy()

    print()
    print('156. Die Teileauswahl kennt auch Herstellbares')
    # ⚠⚠ **Militaer ist nicht kaufbar, aber herstellbar.** Bis zum 06.09.2026
    # speiste sich die Auswahl nur aus UEX — und UEX fuehrt Ladenware. Bei den
    # Quantenantrieben der Groesse 2 standen dadurch 0 Militaer-Teile zur Wahl,
    # obwohl es drei gibt. Wer Bauplaene sammelt, will genau die sehen.
    from scbp import warenkorb as _wk156, laeden as _ld156
    from scbp import herstellung as _he156, katalog as _kt156

    _echt156 = (_ld156.katalog_teile, _he156.alle, _kt156.laden)
    try:
        # Zwei kaufbare Teile, davon eines auch herstellbar; dazu ein rein
        # herstellbares Militaer-Teil, das UEX gar nicht kennt.
        _ld156.katalog_teile = lambda: [
            {'name': 'Civi-QD', 'kennung': 'ref-civi', 'kategorie':
             'Quantum Drives', 'abschnitt': 'Propulsion', 'hersteller': 'Acme',
             'groesse': '2', 'klasse': 'Civilian', 'guete': 'A'},
            {'name': 'Doppel-QD', 'kennung': 'ref-doppel', 'kategorie':
             'Quantum Drives', 'abschnitt': 'Propulsion', 'hersteller': 'Acme',
             'groesse': '2', 'klasse': 'Industrial', 'guete': 'B'},
        ]
        _he156.alle = lambda: [
            {'basis': 'Doppel-QD', 'name': 'Doppel-QD', 'entity': 'ref-doppel',
             'hersteller': 'Acme', 'art': 'quantumdrive', 'unterart': 'size2'},
            {'basis': 'Militaer-QD', 'name': 'Militaer-QD',
             'entity': 'ref-mil', 'hersteller': 'Wei-Tek',
             'art': 'quantumdrive', 'unterart': 'size2'},
            # Falsche Groesse — darf NICHT erscheinen.
            {'basis': 'Gross-QD', 'name': 'Gross-QD', 'entity': 'ref-gross',
             'hersteller': 'Acme', 'art': 'quantumdrive', 'unterart': 'size4'},
        ]
        # Der Katalog kennt nur eines der drei — die anderen muessen ueber den
        # Rueckfall aus den Rezeptdaten kommen.
        # ⚠ Der Schluessel muss durch `katalog._norm()` gegangen sein.
        # Beim ersten Anlauf stand hier `militaerqd` ohne Bindestrich — `_norm`
        # macht aber `militaer-qd` daraus, die Merkmale wurden nicht gefunden,
        # und die Pruefung meldete einen Fehler im Code, den es nicht gab.
        _kt156.laden = lambda: {'bauplaene': {
            'militaer-qd': {'n': 'Militaer-QD', 'a': 'QuantumDrive', 's': 2,
                            'g': 1, 'c': 'Military', 'm': 'Wei-Tek'}}}

        _a156 = _wk156.auswahl('QuantumDrive', 2)
        _namen156 = sorted(x['name'] for x in _a156)
        pruefe(_namen156 == ['Civi-QD', 'Doppel-QD', 'Militaer-QD'],
               'kaufbare UND herstellbare Teile stehen zur Wahl (bekam: %s)'
               % (_namen156,))

        # ⚠⚠ Der Kern: das Militaer-Teil, das UEX nicht fuehrt.
        _mil156 = [x for x in _a156 if x['name'] == 'Militaer-QD']
        pruefe(_mil156 and _mil156[0]['klasse'] == 'Military',
               'das nur herstellbare Militaer-Teil ist dabei, mit Klasse')
        pruefe(_mil156 and _mil156[0]['herkunft'] == _wk156.HERSTELLBAR,
               'und ist als herstellbar gekennzeichnet')

        # ⚠ Ein Teil aus beiden Quellen steht EINMAL da, nicht zweimal.
        _dop156 = [x for x in _a156 if x['name'] == 'Doppel-QD']
        pruefe(len(_dop156) == 1,
               'ein Teil aus beiden Quellen steht einmal da (bekam: %d)'
               % len(_dop156))
        pruefe(_dop156 and _dop156[0]['herkunft'] == _wk156.BEIDES,
               'und traegt die Herkunft „beides"')
        pruefe(len(set(x['kennung'] for x in _a156)) == len(_a156),
               'jede Kennung kommt genau einmal vor')

        # ⚠ Die Groesse wird auch im Rueckfall geprueft.
        pruefe(not any(x['name'] == 'Gross-QD' for x in _a156),
               'ein Teil der falschen Groesse bleibt draussen')

        # ⚠ GEGENPROBE 1: Ohne die Craft-Quelle waeren es nur die zwei
        # kaufbaren — und Militaer fehlte, genau wie vor dem 06.09.2026.
        _ohne156 = [x for x in _a156 if x['herkunft'] != _wk156.HERSTELLBAR]
        pruefe(len(_ohne156) == 2,
               'Gegenprobe: nur aus dem Laden waeren es 2 statt 3 (bekam: %d)'
               % len(_ohne156))

        # ⚠ GEGENPROBE 2: Eine Art, die keine Quelle kennt, gibt eine LEERE
        # Liste — nicht wahllos den halben Katalog.
        pruefe(_wk156.auswahl('GibtsNicht', 2) == [],
               'Gegenprobe: eine unbekannte Art liefert nichts')
    finally:
        _ld156.katalog_teile, _he156.alle, _kt156.laden = _echt156

    print()
    print('157. Die Farmliste zaehlt ueber ALLE Posten zusammen')
    # ⚠⚠ **Die Falle, gegen die diese Pruefung geschrieben ist:**
    # `rohstoffe.pruefen()` rechnet EIN Rezept gegen das Lager. Bei zwei
    # Posten mit je 2 Iron und 3 Iron im Lager meldet es zweimal „reicht" —
    # zusammen fehlt aber eines. Wer die Fehlmengen einzeln addiert, rechnet
    # dasselbe Erz mehrfach an und schickt den Spieler mit zu wenig Material
    # los.
    from scbp import erkul as _erk157, rohstoffe as _ro157
    from scbp import preise as _pr157

    _slots157 = [
        {'pfad': 'a', 'art': 'Cooler', 'groesse': 2,
         'werk': {'ref': 'ref-werk', 'name': 'Werk'}},
        {'pfad': 'b', 'art': 'Cooler', 'groesse': 2,
         'werk': {'ref': 'ref-werk', 'name': 'Werk'}},
    ]
    _echt157 = (_erk157.laden, _ld156.bekannt, _ld156.laeden,
                _ld156.guenstigster, _wk156._bauplan_verzeichnis,
                _he156.rezept, _pr157.preis, _ro157.laden)
    try:
        _erk157.laden = lambda: {'spielversion': 'p', 'hersteller': {},
                                 'schiffe': {'probe': {
                                     'name': 'Probe', 'id': 'probe',
                                     'plaetze': [], 'slots': _slots157}}}
        _ld156.bekannt = lambda k: False
        _ld156.laeden = lambda k: None
        _ld156.guenstigster = lambda k: None
        _wk156._bauplan_verzeichnis = lambda: {'ref-blast': 'BlastChill'}
        _pr157.preis = lambda r: (2643.0, 2000.0, 'Iron')
        _he156.rezept = lambda n: ({'name': 'BlastChill', 'stufen': [
            {'zeit': 100, 'zutaten': [('Frame', 'Iron', 2.0, 0)]}]}
            if n == 'BlastChill' else None)
        # 3 Iron im Lager — reicht fuer EINEN der beiden Posten.
        _ro157.laden = lambda: [{'material': 'Iron', 'menge': 3.0,
                                 'qualitaet': 500, 'ort': ''}]

        _mein157 = {'name': 'Probe', 'hersteller': '', 'kurz': '',
                    'hkurz': '', 'belegung': {}}
        _wk156.setzen(_mein157, 'a', 'ref-blast', 'Blast', _wk156.BAUEN)
        _wk156.setzen(_mein157, 'b', 'ref-blast', 'Blast', _wk156.BAUEN)
        _daten157 = {'format': 1, 'schiffe': [_mein157], 'wunsch': []}

        _f157 = _wk156.farmliste(_daten157)
        pruefe(_f157['posten'] == 2,
               'beide Posten stehen auf bauen (bekam: %d)' % _f157['posten'])
        pruefe(len(_f157['fehlt']) == 1
               and _f157['fehlt'][0]['benoetigt'] == 4.0,
               'der Bedarf beider Posten wird addiert (bekam: %s)'
               % ([z['benoetigt'] for z in _f157['fehlt']],))
        pruefe(_f157['fehlt'] and _f157['fehlt'][0]['differenz'] == 1.0,
               'es fehlt genau 1 (bekam: %s)'
               % (_f157['fehlt'][0]['differenz'] if _f157['fehlt'] else None))

        # ⚠ GEGENPROBE: Einzeln gerechnet meldet `pruefen()` „nichts fehlt" —
        # genau der Fehler, den die Farmliste vermeiden muss.
        _einzeln157 = _ro157.pruefen([('Frame', 'Iron', 2.0, 0)], 1)
        pruefe(_einzeln157[0][3] == 0.0,
               'Gegenprobe: einzeln gerechnet faellt der Mangel NICHT auf')

        # ⚠ Erz, das die geforderte Guete nicht erreicht, zaehlt nicht als
        # Bestand — wird aber ausgewiesen statt verschwiegen.
        _ro157.laden = lambda: [{'material': 'Iron', 'menge': 10.0,
                                 'qualitaet': 100, 'ort': ''}]
        _he156.rezept = lambda n: ({'name': 'BlastChill', 'stufen': [
            {'zeit': 100, 'zutaten': [('Frame', 'Iron', 2.0, 500)]}]}
            if n == 'BlastChill' else None)
        _g157 = _wk156.farmliste(_daten157)
        pruefe(_g157['fehlt'] and _g157['fehlt'][0]['vorhanden'] == 0.0,
               'zu schlechtes Erz zaehlt nicht als Bestand')
        pruefe(_g157['fehlt'] and _g157['fehlt'][0]['zu_gering'] == 10.0,
               'es wird aber als „zu geringe Guete" ausgewiesen (bekam: %s)'
               % (_g157['fehlt'][0]['zu_gering'] if _g157['fehlt'] else None))

        # ⚠⚠ **Ein Posten ohne Rezept steht gar nicht erst auf „bauen".**
        # `rechnung()` setzt den Weg auf „kaufen" zurueck, sobald kein Rezept
        # vorliegt — die Farmliste sieht ihn deshalb nie. Beim ersten Anlauf
        # erwartete diese Pruefung ihn unter `ohne_rezept` und war rot, obwohl
        # der Code sich richtig verhielt.
        #
        # Geprueft wird deshalb das **tatsaechliche** Verhalten: Er erzeugt
        # keinen Materialbedarf und wird nicht als Bau-Posten gezaehlt.
        _he156.rezept = lambda n: None
        _wk156._bauplan_verzeichnis = lambda: {}
        _h157 = _wk156.farmliste(_daten157)
        pruefe(_h157['fehlt'] == [] and _h157['vollstaendig'] == [],
               'ohne Rezept entsteht kein erfundener Materialbedarf')
        pruefe(_h157['posten'] == 0,
               'und der Posten zaehlt nicht als Bau-Posten (bekam: %d)'
               % _h157['posten'])
    finally:
        (_erk157.laden, _ld156.bekannt, _ld156.laeden, _ld156.guenstigster,
         _wk156._bauplan_verzeichnis, _he156.rezept, _pr157.preis,
         _ro157.laden) = _echt157

    # ------------------------------------------------------------------
    # 158. Guete als Buchstabe, und was ohne Klasse dasteht
    #
    # ⚠⚠ Zwei Fehler an derselben Angabe, beide am 06.09.2026 gemessen:
    #
    # 1. Der Bauplan-Katalog fuehrt die Guete als **Zahl**, das Spiel als
    #    Buchstabe. Ungewandelt standen 224 von 304 Teilen mit `1`-`4` in der
    #    Liste, gemischt mit denen, die ihr `A`-`D` aus UEX hatten.
    # 2. Beim Zusammenfuehren gewann die Katalog-Angabe, sobald sie irgendetwas
    #    enthielt. `Bolt` stand dadurch als „2 · Tarnung" da, wo vorher richtig
    #    „B · Tarnung" stand — eine Verschlechterung durch eine Erweiterung.
    #
    # Dazu die dritte Frage: Was steht da, wenn die Klasse fehlt? Geraten wird
    # nichts (der Katalog fuehrt sie nur bei 240 von 738 Eintraegen) — dafuer
    # sagt die Herkunft, dass es das Teil in keinem Laden gibt.
    print()
    print('158. Guete als Buchstabe, Klasse nie geraten')
    from scbp import seiten as _st158
    from scbp import sprache as _sp158
    from scbp import warenkorb as _wk158

    pruefe(_wk158._guete_buchstabe('2') == 'B',
           'die Zahl 2 wird zum Buchstaben B')
    pruefe(_wk158._guete_buchstabe(1) == 'A', '* auch als Zahl statt Text')
    pruefe(_wk158._guete_buchstabe('4') == 'D', '* und 4 zu D')
    pruefe(_wk158._guete_buchstabe('C') == 'C',
           'ein vorhandener Buchstabe bleibt unangetastet')
    pruefe(_wk158._guete_buchstabe('') == '', 'nichts bleibt nichts')
    # ⚠ Gegenprobe: Etwas Unerwartetes wird gezeigt, nicht verschluckt — eine
    # 7 waere der Hinweis, dass sich die Quelle geaendert hat.
    pruefe(_wk158._guete_buchstabe('7') == '7',
           'Gegenprobe: ein unerwarteter Wert wird nicht verworfen')

    _alt158 = _sp158.aktuelle()
    _vorher158 = _st158._TEIL_VERZEICHNIS[0]
    _st158._TEIL_VERZEICHNIS[0] = {}
    try:
        _sp158.setzen('de')
        pruefe(_st158._teil_kennzeichen(
            {'guete': 'A', 'klasse': 'Military',
             'herkunft': _wk158.HERSTELLBAR}) == 'A · Militär · nur über Bauplan',
               'ein Militaerteil zeigt Guete, Klasse und die Herkunft')
        # ⭐ Der Fall Crossfield: keine Klasse, aber eine nuetzliche Auskunft.
        pruefe(_st158._teil_kennzeichen(
            {'guete': '', 'klasse': '',
             'herkunft': _wk158.HERSTELLBAR}) == 'nur über Bauplan',
               'ohne Klasse steht die Herkunft da, keine geratene Klasse')
        pruefe('Zivil' not in _st158._teil_kennzeichen(
            {'guete': '', 'klasse': '', 'herkunft': _wk158.HERSTELLBAR}),
               'Gegenprobe: es wird KEINE Standardklasse eingesetzt')
        # Der Normalfall bekommt keinen Zusatz — sonst staende an jedem
        # zweiten Teil dasselbe Wort.
        pruefe(_st158._teil_kennzeichen(
            {'guete': 'B', 'klasse': 'Civilian',
             'herkunft': _wk158.BEIDES}) == 'B · Zivil',
               'ein auch kaufbares Teil bekommt keinen Herkunfts-Zusatz')
        pruefe(_st158._teil_kennzeichen(
            {'guete': 'B', 'klasse': 'Civilian',
             'herkunft': _wk158.KAUFBAR}) == 'B · Zivil',
               '* und ein nur kaufbares ebenso wenig')
    finally:
        _st158._TEIL_VERZEICHNIS[0] = _vorher158
        _sp158.setzen(_alt158)

    # ------------------------------------------------------------------
    # 159. Abhaken, offene Posten und „fertig gefittet"
    #
    # ⭐⭐ Die wichtigste Pruefung dieses Bereichs, und zwar wegen der
    # Spielmechanik dahinter: Ein neu geclaimtes Schiff kommt in seiner
    # **Werksausstattung** zurueck. Wer ein aufgeruestetes Schiff ohne die
    # passende Versicherung claimt, verliert alles Eingebaute — mehrere
    # hunderttausend aUEC. Am 06.09.2026 erklaert: „ich habe Super Hornet
    # gefittet und versichert, und wenn ich das Schiff ohne Versicherung neu
    # claime, wuerde ich die Komponenten verlieren."
    #
    # Deshalb muss „fertig gefittet" zuverlaessig sein — eine falsche Marke
    # waere hier schlimmer als gar keine.
    #
    # ⚠ Und der Fall, der beides auseinanderhaelt: Ein Schiff **ohne jede
    # Planung** hat ebenfalls keine offenen Posten, ist aber NICHT fertig
    # gefittet, sondern unberuehrt. Im Code sieht das gleich aus.
    print()
    print('159. Abhaken, offene Posten und fertig gefittet')
    from scbp import warenkorb as _wk159

    _leer159 = {'name': 'Arrow', 'belegung': {}}
    _offen159 = {'name': 'Cutlass', 'belegung': {
        'p1': {'ref': 'r1', 'name': 'A', 'weg': _wk159.KAUFEN},
        'p2': {'ref': 'r2', 'name': 'B', 'weg': _wk159.BAUEN}}}
    _halb159 = {'name': 'Vulture', 'belegung': {
        'p1': {'ref': 'r1', 'name': 'A', 'weg': _wk159.KAUFEN,
               'erledigt': True},
        'p2': {'ref': 'r2', 'name': 'B', 'weg': _wk159.BAUEN}}}
    _fertig159 = {'name': 'Super Hornet', 'belegung': {
        'p1': {'ref': 'r1', 'name': 'A', 'weg': _wk159.KAUFEN,
               'erledigt': True},
        'p2': {'ref': 'r2', 'name': 'B', 'weg': _wk159.BAUEN,
               'erledigt': True}}}

    pruefe(_wk159.offene_anzahl(_offen159) == 2, 'zwei offene Posten')
    pruefe(_wk159.offene_anzahl(_halb159) == 1, 'einer abgehakt, einer offen')
    pruefe(_wk159.offene_anzahl(_fertig159) == 0, 'alles abgehakt')
    pruefe(_wk159.offene_anzahl(_leer159) == 0, 'nichts geplant, nichts offen')

    pruefe(_wk159.fertig_gefittet(_fertig159),
           'ein durchgehend abgehaktes Schiff gilt als fertig gefittet')
    pruefe(not _wk159.fertig_gefittet(_halb159),
           'ein halb erledigtes nicht')
    pruefe(not _wk159.fertig_gefittet(_offen159), 'ein unberuehrtes nicht')
    # ⚠⚠ Die Gegenprobe, auf die es ankommt.
    pruefe(not _wk159.fertig_gefittet(_leer159),
           'Gegenprobe: ein Schiff OHNE Planung ist nicht fertig gefittet, '
           'obwohl es auch keine offenen Posten hat')

    # Haken setzen und wieder wegnehmen.
    pruefe(_wk159.erledigt_setzen(_offen159, 'p1', True), 'abhaken wirkt')
    pruefe(_wk159.erledigt(_offen159, 'p1'), '* und steht danach drin')
    pruefe(not _wk159.erledigt_setzen(_offen159, 'p1', True),
           'zweimal dasselbe abhaken aendert nichts')
    pruefe(_wk159.erledigt_setzen(_offen159, 'p1', False),
           'der Haken laesst sich zurueckziehen')
    pruefe(not _wk159.erledigt(_offen159, 'p1'), '* und ist dann weg')
    pruefe(not _wk159.erledigt_setzen(_offen159, 'gibt-es-nicht', True),
           'Gegenprobe: ein unbekannter Platz laesst sich nicht abhaken')

    # Ein abgehakter Posten kostet nichts mehr, zaehlt aber nicht als Luecke.
    _liste159 = [
        {'weg': _wk159.KAUFEN, 'erledigt': True,
         'kauf': {'zustand': _wk159.BEKANNT, 'preis': 1000}},
        {'weg': _wk159.KAUFEN, 'erledigt': False,
         'kauf': {'zustand': _wk159.BEKANNT, 'preis': 500}},
    ]
    _s159 = _wk159.summe(_liste159)
    pruefe(abs(float(_s159.get('gesamt') or 0) - 500.0) < 0.01,
           'die Summe laesst Abgehaktes aus (bekam: %s)' % _s159.get('gesamt'))
    pruefe(not _s159.get('offen'),
           '* und zaehlt es nicht als Posten ohne Preis')

    # ------------------------------------------------------------------
    # 160. Ein Klick auf den Haken — und was die Anzeige danach sagt
    #
    # ⚠⚠⚠ **Diese Pruefung gibt es, weil drei Fehler durchgerutscht sind, die
    # alle sichtbar gewesen waeren.** Am 06.09.2026 gefragt: „testest du deine
    # Funktionen gar nicht mehr in echt, oder machst du das immer erst nachdem
    # ich Fehler gefunden habe?" Die ehrliche Antwort war: Die Datenschicht
    # war gemessen (Abhaken senkte die Summe von 540.540 auf 405.405), die
    # **Oberflaeche nach einem Klick** aber nie.
    #
    # Genau dort lagen alle drei:
    #
    # | Fehler | was zu sehen war |
    # |---|---|
    # | Marke zog nicht mit | alle vier abgehakt, Zeile sagte „4 noch zu besorgen" |
    # | Kopf zaehlte Erledigtes | „8 Positionen", zwei davon fertig |
    # | Farmliste zaehlte Erledigtes | „fuer 8 Bauteile", vier schon gebaut |
    #
    # Eine Pruefung, die nur Funktionen aufruft, findet so etwas nie: Jede
    # einzelne Funktion war richtig. Falsch war, **wer nach einer Aenderung
    # neu zeichnet**. Deshalb wird hier wirklich geklickt und danach der Text
    # der Widgets gelesen.
    print()
    print('160. Ein Klick auf den Haken zieht die Anzeige nach')
    import tkinter as _tk160
    from tkinter import font as _fo160
    from scbp import seiten as _st160
    from scbp import warenkorb as _wk160

    class _F160:
        pass

    def _texte160(w, raus=None):
        """Alle sichtbaren Beschriftungen unterhalb eines Widgets."""
        raus = [] if raus is None else raus
        for k in w.winfo_children():
            try:
                if k.cget('text'):
                    raus.append(k.cget('text'))
            except Exception:
                pass
            _texte160(k, raus)
        return raus

    _wurzel160 = _tk160.Tk()
    _wurzel160.withdraw()
    _f160 = _F160()
    _f160.f_klein = _fo160.Font(family='Calibri', size=10)
    _f160.f_fett = _fo160.Font(family='Calibri', size=10, weight='bold')
    _f160.beim_zeigen = {}
    try:
        # Zwei Posten, beide offen — die Marke muss „2" sagen.
        _schiff160 = {'name': 'Testschiff', 'hersteller': 'X',
                      'belegung': {
                          'p1': {'ref': 'r1', 'name': 'Teil A',
                                 'weg': _wk160.KAUFEN},
                          'p2': {'ref': 'r2', 'name': 'Teil B',
                                 'weg': _wk160.KAUFEN}}}
        _rahmen160 = _tk160.Frame(_wurzel160, bg='#0d1117')
        _st160._zeichne_marke(_f160, _rahmen160, _schiff160)
        _wurzel160.update_idletasks()
        _vorher160 = ' '.join(_texte160(_rahmen160))
        pruefe('2' in _vorher160,
               'die Marke nennt zwei offene Posten (steht da: %r)'
               % _vorher160)

        # Jetzt einen abhaken und die Marke neu zeichnen — wie es der
        # Rueckruf tut.
        _wk160.erledigt_setzen(_schiff160, 'p1', True)
        for _k160 in _rahmen160.winfo_children():
            _k160.destroy()
        _st160._zeichne_marke(_f160, _rahmen160, _schiff160)
        _wurzel160.update_idletasks()
        _nachher160 = ' '.join(_texte160(_rahmen160))
        pruefe('1' in _nachher160 and '2' not in _nachher160,
               'nach einem Haken nennt sie einen (steht da: %r)'
               % _nachher160)

        # Beide abgehakt: aus der Zaehlung wird die Claim-Warnung.
        _wk160.erledigt_setzen(_schiff160, 'p2', True)
        for _k160 in _rahmen160.winfo_children():
            _k160.destroy()
        _st160._zeichne_marke(_f160, _rahmen160, _schiff160)
        _wurzel160.update_idletasks()
        _fertig160 = ' '.join(_texte160(_rahmen160))
        pruefe('Claim' in _fertig160 or 'claim' in _fertig160,
               'ist alles abgehakt, steht die Claim-Warnung da (%r)'
               % _fertig160)
        # ⚠⚠ Die Gegenprobe, die den gemeldeten Fehler gefunden haette.
        pruefe('besorgen' not in _fertig160,
               'Gegenprobe: „noch zu besorgen" steht NICHT mehr da, wenn '
               'alles abgehakt ist')

        # Und ein unberuehrtes Schiff bekommt gar keine Marke.
        _leer160 = {'name': 'Leer', 'belegung': {}}
        for _k160 in _rahmen160.winfo_children():
            _k160.destroy()
        _st160._zeichne_marke(_f160, _rahmen160, _leer160)
        _wurzel160.update_idletasks()
        pruefe(not _texte160(_rahmen160),
               'ein Schiff ohne Planung bekommt keine Marke')
    finally:
        _wurzel160.destroy()

    # ------------------------------------------------------------------
    # 161. Kein System-Dialog im Programm
    #
    # ⚠⚠⚠ **Das war der schlimmste Fehler des Tages.** Am 06.09.2026 erschien
    # beim Speichern der Joystick-Belegung ein `messagebox`-Fenster
    # **ausserhalb aller Bildschirme**. Weil es modal ist, war das Programm
    # danach unbedienbar — es liess sich nicht einmal mehr beenden. Vorher
    # hatte derselbe Dialog schon dafuer gesorgt, dass sich die Gruppen in der
    # Seitenleiste nicht mehr auf- und zuklappen liessen: Er stand unsichtbar
    # am unteren Rand und hielt die Oberflaeche fest.
    #
    # Dazu die beiden aelteren Beschwerden ueber denselben Dialog: heller
    # Kasten im dunklen Programm („sieht kacke aus") und Knoepfe in der
    # Systemsprache statt der eingestellten („der Dialog zeigt im Deutschen
    # englische Woerter").
    #
    # Es gab bereits drei oertliche Ersatzklassen — und fuenf Stellen, die
    # sich den echten mit `from tkinter import messagebox` zurueckgeholt
    # haben. Eine Regel, die man an jeder Stelle einzeln befolgen muss, wird
    # irgendwo nicht befolgt. Deshalb prueft das hier den ganzen Quelltext.
    #
    # ⚠ Die **eine** erlaubte Stelle ist der Notnagel in `hauptfenster.py`:
    # Scheitert `frage_stellen` selbst, ist ein haesslicher Dialog besser als
    # gar keiner.
    print()
    print('161. Kein System-Dialog im Programm')
    import re as _re161

    _erlaubt161 = 'scbp/hauptfenster.py'
    _fund161 = []
    for _datei161 in sorted(_versionierte_dateien(WURZEL, ('.py',))):
        if not _datei161.startswith('scbp/'):
            continue
        with open(os.path.join(WURZEL, _datei161), 'r',
                  encoding='utf-8') as _f161:
            _text161 = _f161.read()
        for _nr161, _zeile161 in enumerate(_text161.split('\n'), 1):
            # Kommentare und Doku zaehlen nicht — dort steht die Begruendung.
            _blank161 = _zeile161.strip()
            if _blank161.startswith('#') or _blank161.startswith('⚠'):
                continue
            if _re161.search(r'messagebox\.(show\w+|ask\w+)\(', _zeile161):
                if _datei161 == _erlaubt161:
                    continue
                _fund161.append('%s:%d' % (_datei161, _nr161))

    pruefe(not _fund161,
           'kein `messagebox` ausserhalb des Notnagels (gefunden: %s)'
           % (', '.join(_fund161[:4]) if _fund161 else 'keine'))

    # ⚠ Und der Import zaehlt mit: Er ist es, der die oertliche Ersatzklasse
    # ueberschreibt. Genau so kamen die fuenf Stellen zurueck.
    _importe161 = []
    for _datei161 in sorted(_versionierte_dateien(WURZEL, ('.py',))):
        if not _datei161.startswith('scbp/') or _datei161 == _erlaubt161:
            continue
        with open(os.path.join(WURZEL, _datei161), 'r',
                  encoding='utf-8') as _f161:
            for _nr161, _zeile161 in enumerate(_f161, 1):
                if _re161.match(r'\s*from tkinter import messagebox',
                                _zeile161):
                    _importe161.append('%s:%d' % (_datei161, _nr161))
    pruefe(not _importe161,
           'und kein `from tkinter import messagebox` (gefunden: %s)'
           % (', '.join(_importe161[:4]) if _importe161 else 'keine'))

    # Gegenprobe: Das Muster muss so eine Zeile auch wirklich finden.
    pruefe(bool(_re161.search(r'messagebox\.(show\w+|ask\w+)\(',
                              'x = messagebox.showinfo(a, b)')),
           'Gegenprobe: das Muster erkennt einen echten Aufruf')

    # ------------------------------------------------------------------
    # 162. Jedes Fenster sagt, WO es steht — nicht nur wie gross es ist
    #
    # ⚠⚠⚠ **Der Standard, statt sechsmal derselbe Fehler.** Am 06.09.2026:
    # „kannst du da nicht im Programm einen Standard festlegen?" — nachdem ein
    # Fenster ausserhalb aller Bildschirme aufgegangen war und das Programm
    # unbedienbar machte.
    #
    # Ursache war jedes Mal dieselbe Zeile: `geometry('520x340')` ohne `+x+y`.
    # Wer nur eine Groesse setzt, ueberlaesst die Platzierung dem
    # Fenstermanager — und der weiss nichts vom Hauptfenster. Auf einem
    # Arbeitsplatz mit drei Bildschirmen ist das ein Gluecksspiel.
    #
    # Diese Pruefung ist der Standard: Sie geht jedes `geometry(` im Programm
    # durch und verlangt entweder eine Position im Aufruf oder `mittig_ueber`
    # in der Naehe. Eine Regel, die man an jeder Stelle einzeln befolgen muss,
    # wird irgendwo nicht befolgt — deshalb prueft es der Selbsttest.
    print()
    print('162. Jedes Fenster wird auch positioniert')
    import re as _re162

    _fund162 = []
    for _datei162 in sorted(_versionierte_dateien(WURZEL, ('.py',))):
        if not _datei162.startswith('scbp/'):
            continue
        with open(os.path.join(WURZEL, _datei162), 'r',
                  encoding='utf-8') as _f162:
            _zeilen162 = _f162.read().split('\n')
        for _nr162, _z162 in enumerate(_zeilen162):
            _m162 = _re162.search(r"\.geometry\(['\"](\d+)x(\d+)['\"]\)",
                                  _z162)
            if not _m162 or _z162.strip().startswith('#'):
                continue
            # Steht in den drei Zeilen davor ein `mittig_ueber`, ist es der
            # Rueckfall fuer den Start ohne Elternfenster — das ist richtig so.
            _umfeld162 = '\n'.join(_zeilen162[max(0, _nr162 - 4):_nr162 + 2])
            if 'mittig_ueber' in _umfeld162:
                continue
            _fund162.append('%s:%d' % (_datei162, _nr162 + 1))

    pruefe(not _fund162,
           'kein `geometry` ohne Position und ohne `mittig_ueber` '
           '(gefunden: %s)'
           % (', '.join(_fund162[:4]) if _fund162 else 'keine'))

    # Gegenprobe: Das Muster muss so eine Zeile auch wirklich erkennen.
    pruefe(bool(_re162.search(r"\.geometry\(['\"](\d+)x(\d+)['\"]\)",
                              "        self.root.geometry('520x340')")),
           'Gegenprobe: das Muster erkennt eine Groesse ohne Position')
    pruefe(not _re162.search(r"\.geometry\(['\"](\d+)x(\d+)['\"]\)",
                             "top.geometry('%dx%d+%d+%d' % (b, h, x, y))"),
           'Gegenprobe: eine Zeile MIT Position schlaegt nicht an')

    # Und die Funktion selbst rechnet richtig.
    from scbp import hauptfenster as _hf162
    pruefe(callable(getattr(_hf162, 'mittig_ueber', None)),
           '`mittig_ueber` steht als gemeinsamer Standard bereit')

    # ------------------------------------------------------------------
    # 163. Speichern wirft nichts weg
    #
    # ⚠⚠⚠ **Der einzige Datenverlust dieses Projekts.** Am 06.09.2026 gemeldet:
    # „Gebe ich ein Schiff auf der Wunschliste ein, bleibt es nur so lange
    # stehen, bis ich Komponenten dazu eintrage." Die Ursache war eine Zeile:
    #
    #     meine.speichern({'format': …, 'schiffe': _hangar_liste(eintrag)})
    #
    # Die Datei enthaelt zwei Listen — `schiffe` UND `wunsch`. Wer nur eine
    # davon schreibt, loescht die andere. Jede Aenderung an der Ausstattung
    # irgendeines Schiffs hat die komplette Wunschliste vernichtet.
    #
    # Dazu der zweite Teil: Ein Wunsch-Eintrag wurde in `schiffe` gesucht,
    # dort nie gefunden — seine Aenderung ging ebenfalls verloren.
    #
    # ⚠ Eine Pruefung, die nur `speichern()` aufruft, findet das nicht: Die
    # Funktion war richtig. Falsch war, WAS ihr uebergeben wurde. Geprueft
    # wird deshalb der Weg ueber `_eintrag_speichern`, so wie die Oberflaeche
    # ihn geht.
    print()
    print('163. Speichern wirft weder Schiffe noch Wuensche weg')
    import json as _js163
    import shutil as _sh163
    import tempfile as _tf163
    from scbp import seiten as _st163
    from scbp import warenkorb as _wk163

    _ordner163 = _tf163.mkdtemp(prefix='sc-bp-speichern-')
    _altheim163 = os.environ.get('SC_BP_HOME')
    os.environ['SC_BP_HOME'] = _ordner163
    try:
        from scbp import hangar as _hg163
        _hg163.vergessen() if hasattr(_hg163, 'vergessen') else None
        with open(os.path.join(_ordner163, 'hangar.json'), 'w',
                  encoding='utf-8') as _f163:
            _js163.dump({'format': _hg163.FORMAT,
                         'schiffe': [{'name': 'Cutlass Black',
                                      'hersteller': 'Drake',
                                      'belegung': {}}],
                         'wunsch': [{'name': 'Avenger Warlock',
                                     'hersteller': 'Aegis',
                                     'belegung': {}}]}, _f163)

        _stand163 = _hg163.laden()
        pruefe(len(_stand163.get('schiffe') or []) == 1
               and len(_stand163.get('wunsch') or []) == 1,
               'Ausgangslage: ein Schiff, ein Wunsch')

        # An einem HANGAR-Schiff etwas aendern.
        _schiff163 = _stand163['schiffe'][0]
        _wk163.setzen(_schiff163, 'p1', 'ref-1', 'BlastChill')
        _st163._eintrag_speichern(_schiff163)
        _neu163 = _hg163.laden()
        pruefe(len(_neu163.get('wunsch') or []) == 1,
               'die Wunschliste ueberlebt eine Aenderung am Hangar-Schiff')
        pruefe(len(_wk163.belegung(_neu163['schiffe'][0])) == 1,
               '* und die Aenderung selbst ist gespeichert')

        # An einem WUNSCH-Schiff etwas aendern.
        _wunsch163 = _neu163['wunsch'][0]
        _wk163.setzen(_wunsch163, 'p9', 'ref-2', 'FR-66')
        _st163._eintrag_speichern(_wunsch163)
        _zuletzt163 = _hg163.laden()
        pruefe(len(_zuletzt163.get('schiffe') or []) == 1,
               'der Hangar ueberlebt eine Aenderung am Wunschschiff')
        pruefe(len(_wk163.belegung(_zuletzt163['wunsch'][0])) == 1,
               '* und die Aenderung am Wunschschiff ist gespeichert')
        pruefe(len(_wk163.belegung(_zuletzt163['schiffe'][0])) == 1,
               '* die des Hangar-Schiffs steht auch noch da')

        # Gegenprobe: Ein Eintrag, den es nirgends gibt, wird gemeldet.
        pruefe(not _st163._eintrag_speichern({'name': 'Gibt es nicht',
                                              'hersteller': 'X'}),
               'Gegenprobe: ein unbekannter Eintrag meldet, dass er fehlt')
    finally:
        if _altheim163 is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _altheim163
        _sh163.rmtree(_ordner163, ignore_errors=True)

    # ------------------------------------------------------------------
    # 164. Der Zerlege-Rechner
    #
    # ⭐⭐ Die Frage eines Bergungsspielers vor dem Ausbauen: Was gibt der
    # Fabricator zurueck? Vorschlag vom 06.09.2026.
    #
    # ⚠⚠ **Die halbe Wahrheit waere hier die gefaehrlichere.** „Man bekommt
    # 50 % zurueck" stimmt — aber sechs Rohstoffe stehen auf der Sperrliste
    # und kommen GAR NICHT wieder, darunter Quantainium und Stileron.
    # Gemessen: Bei 258 von 400 Teilen ist mindestens einer davon dabei. Ein
    # Rechner, der stumpf halbiert, schickt zwei Drittel der Spieler mit
    # falschen Erwartungen los.
    #
    # ⚠ Legt sich die Daten SELBST hin — die Craftdaten sind ein geholter
    # Zwischenspeicher und liegen im Wegwerf-Ordner nicht.
    print()
    print('164. Der Zerlege-Rechner')
    from scbp import bergung as _bg164
    from scbp import herstellung as _he164

    _echt164 = (_he164.laden, _he164.rezept)
    try:
        _he164.laden = lambda: {'dismantle': {
            'efficiency': 0.5,
            'dismantleTimeSeconds': 15,
            'blacklistedResources': [{'name': 'Quantainium'},
                                     {'name': 'Riccite'}],
            'blacklistedEntityClasses': [{'name': 'Saldynium (Ore)'}]}}
        _he164.rezept = lambda name: {
            'stufen': [{'zeit': 960,
                        'zutaten': [['Frame', 'Iron', 0.64, 1],
                                    ['Cycler', 'Riccite', 0.09, 1],
                                    ['Barrel', 'Titanium', 0.32, 1]]},
                       {'zeit': 120,
                        'zutaten': [['Kern', 'Iron', 0.36, 1]]}]}

        _regeln164 = _bg164.zerlege_regeln()
        pruefe(abs(_regeln164['anteil'] - 0.5) < 0.001,
               'der Anteil kommt aus den Spieldaten (%.2f)'
               % _regeln164['anteil'])
        pruefe(_regeln164['dauer'] == 15, 'die Dauer ebenso')
        pruefe('quantainium' in _regeln164['gesperrt'],
               'die Sperrliste ist da')
        # ⚠ „Saldynium (Ore)" und „Saldynium" sind dasselbe Erz.
        pruefe('saldynium' in _regeln164['gesperrt'],
               'auch die Kurzform eines gesperrten Erzes gilt')

        _zeilen164, _dauer164 = _bg164.zerlegen('Testteil')
        _nach164 = dict((z['rohstoff'], z) for z in _zeilen164)
        pruefe(len(_zeilen164) == 3,
               'drei verschiedene Rohstoffe (bekam: %d)' % len(_zeilen164))
        # ⚠ Ueber ALLE Stufen: Iron steckt zweimal drin (0,64 + 0,36 = 1,0).
        pruefe(abs(_nach164['Iron']['drin'] - 1.0) < 0.001,
               'Mengen aus mehreren Stufen werden addiert (%.2f)'
               % _nach164['Iron']['drin'])
        pruefe(abs(_nach164['Iron']['zurueck'] - 0.5) < 0.001,
               '* und die Haelfte kommt zurueck (%.2f)'
               % _nach164['Iron']['zurueck'])
        # ⚠⚠ Der Punkt, um den es geht.
        pruefe(_nach164['Riccite']['verloren'],
               'ein gesperrter Rohstoff ist als verloren gekennzeichnet')
        pruefe(_nach164['Riccite']['zurueck'] == 0,
               '* und gibt NICHTS zurueck, nicht die Haelfte')
        pruefe(not _nach164['Titanium']['verloren'],
               'Gegenprobe: ein nicht gesperrter Rohstoff ist nicht verloren')

        # Ohne Rezept keine Behauptung.
        _he164.rezept = lambda name: None
        _leer164, _ = _bg164.zerlegen('Gibt es nicht')
        pruefe(_leer164 == [],
               'Gegenprobe: ohne Rezept wird nichts erfunden')
    finally:
        _he164.laden, _he164.rezept = _echt164

    # ------------------------------------------------------------------
    # 165. Die Kurve zeigt den Exponenten DIESER Achse
    #
    # ⚠⚠⚠ **Zwei Anzeigen auf einer Seite widersprachen sich.** Am 06.09.2026
    # stand am rechten Stick auf der Achse `x` eine deutlich gebogene Kurve —
    # und direkt darunter „Auf dieser Achse liegt keine Flugfunktion". Die
    # Biegung kam von `z`, `rotx` und `roty` desselben Geräts.
    #
    # Ursache: `_exponent_fuer` nahm den Exponenten des ganzen **Geräts**,
    # wenn es dort genau einen gab. Die Funktion nannte sich selbst „eine
    # Näherung" und schrieb „lieber nichts anzeigen als das Falsche" — und tat
    # dann genau das. Dazu die Frage, auf die es keine gute Antwort gab: „Wie
    # soll ich einem User erklären, dass er da was sieht, was gar nicht
    # stimmt?"
    #
    # ⚠ Geprüft wird die **Regel**, nicht der Zahlenwert: Ohne Funktion auf der
    # Achse muss der Exponent 1 sein (gerade Linie). Bei uneinigen Funktionen
    # ebenfalls — dann gibt es keine eine Wahrheit.
    print()
    print('165. Die Kurve zeigt den Exponenten DIESER Achse')
    from scbp import kurven as _kv165

    _echt165 = _kv165.spielachsen_auf

    def _tue_so(treffer):
        """`spielachsen_auf` vortäuschen, damit die Regel prüfbar wird."""
        _kv165.spielachsen_auf = lambda n, a, datei=None, ordner=None: treffer

    # Die Regel als Funktion nachbauen — dieselbe wie in `_exponent_fuer`.
    def _regel():
        exponenten = set()
        for eintrag in _kv165.spielachsen_auf(1, 'x'):
            wert = eintrag.get('exponent')
            if wert is not None:
                exponenten.add(wert)
        return exponenten.pop() if len(exponenten) == 1 else 1.0

    try:
        _tue_so([])
        pruefe(_regel() == 1.0,
               'ohne Flugfunktion auf der Achse ist der Exponent 1 '
               '(die Kurve also gerade)')

        _tue_so([{'achse': 'flight_move_roll', 'exponent': 2.0}])
        pruefe(_regel() == 2.0,
               'eine Funktion mit Exponent 2 wird gezeigt')

        _tue_so([{'achse': 'a', 'exponent': 2.0},
                 {'achse': 'b', 'exponent': 2.0}])
        pruefe(_regel() == 2.0,
               'zwei Funktionen mit demselben Wert ebenso')

        # ⚠⚠ Die Gegenprobe, um die es geht.
        _tue_so([{'achse': 'a', 'exponent': 2.0},
                 {'achse': 'b', 'exponent': 3.0}])
        pruefe(_regel() == 1.0,
               'Gegenprobe: uneinige Funktionen -> gerade Linie, kein '
               'geratener Wert')

        _tue_so([{'achse': 'a', 'exponent': None}])
        pruefe(_regel() == 1.0,
               'Gegenprobe: eine Funktion OHNE eigenen Exponenten zaehlt '
               'nicht als Wert')
    finally:
        _kv165.spielachsen_auf = _echt165

    # ⚠ Und die Rechnung dahinter: Bei 1 muss die Kurve wirklich gerade sein.
    pruefe(all(abs(_kv165.antwort(x, 0.0, 1.0, 1.0) - x) < 0.001
               for x in (0.1, 0.25, 0.5, 0.75, 0.9)),
           'bei Exponent 1 ist die Kurve eine Gerade')
    pruefe(abs(_kv165.antwort(0.25, 0.0, 1.0, 2.0) - 0.0625) < 0.001,
           'bei Exponent 2 liegt Viertelausschlag bei 6,25 %% (gebogen)')

    # ------------------------------------------------------------------
    # 166. Bei mehreren Belegungsdateien gewinnt die juengste
    #
    # ⚠⚠⚠ **Der Fehler, der alles andere wertlos machte.** Am 06.09.2026 lagen
    # in EINER Installation zwei `actionmaps.xml` nebeneinander:
    # `LIVE/user/client/…` (dort schreibt das Spiel) und `LIVE/USER/client/…`
    # (eine Karteileiche aus der Windows-Installation). Der Watcher nahm stur
    # die erste Schreibweise, die es gab — `USER` — und zeigte damit eine
    # Empfindlichkeit von 2, waehrend im Spiel ueberall 1,00 stand.
    #
    # ⚠ Schlimmer noch: In der alten Datei waren die Geraete anders
    # durchnummeriert (`instance=1` war der rechte Stick statt des linken).
    # Der Watcher zeigte also nicht nur alte Werte, sondern die des falschen
    # Geraets — und das sah aus wie ein Bedienfehler des Spielers.
    #
    # Unter Windows sind `USER` und `user` derselbe Ordner, unter Linux nicht.
    # Die Lehre stand zu dem Zeitpunkt schon im Code: `_pfad_mappings` nimmt
    # seit dem 04.09.2026 den zuletzt geaenderten Ordner, aus genau diesem
    # Grund. Bei den Belegungsdateien war sie nicht gezogen worden.
    # **Eine Lehre, die nur an einer von zwei Stellen sitzt, ist keine.**
    print()
    print('166. Bei mehreren Belegungsdateien gewinnt die juengste')
    import tempfile as _tf166
    from scbp import joysticks as _js166

    _ordner166 = _tf166.mkdtemp(prefix='actionmaps-')

    def _lege_an(oben, mitte='client'):
        """Eine Belegungsdatei anlegen und ihren Weg zurueckgeben."""
        _wo = os.path.join(_ordner166, oben, mitte, '0', 'Profiles', 'default')
        # ⚠ `exist_ok` ist Pflicht: Auf einem Dateisystem ohne Gross-/
        # Kleinschreibung (Windows) ist `USER` derselbe Ordner wie `user`.
        os.makedirs(_wo, exist_ok=True)
        _weg = os.path.join(_wo, 'actionmaps.xml')
        with io.open(_weg, 'w', encoding='utf-8') as _f:
            _f.write('<ActionMaps/>')
        return _weg

    try:
        _gross = _lege_an('USER')
        _klein = _lege_an('user')

        # ⚠⚠ **Unterscheidet dieses Dateisystem ueberhaupt?** Unter Linux sind
        # es zwei Dateien, unter Windows eine. Beides ist gueltig — aber es
        # sind zwei verschiedene Pruefungen. Gemessen statt angenommen:
        _getrennt = len(_js166.alle_actionmaps(_ordner166)) == 2

        if _getrennt:
            # Der Fall, der den Fehler ausgeloest hat: zwei echte Dateien.
            # ⚠ Die GROSSE aelter machen — sie steht in der Suchreihenfolge
            # vorn. Genau das war der Fehler: Reihenfolge schlug Alter.
            os.utime(_gross, (1000, 1000))
            os.utime(_klein, (2000, 2000))
            pruefe(_js166._pfad_actionmaps(_ordner166) == _klein,
                   'die juengere (klein geschrieben) gewinnt gegen die '
                   'zuerst gesuchte')

            os.utime(_gross, (3000, 3000))
            pruefe(_js166._pfad_actionmaps(_ordner166) == _gross,
                   'Gegenprobe: ist die grosse juenger, gewinnt sie')

            pruefe(len(_js166.alle_actionmaps(_ordner166)) == 2,
                   'beide Dateien werden gefunden, nicht nur eine')
            pruefe(_js166.alle_actionmaps(_ordner166)[0] == _gross,
                   'die Liste kommt sortiert, neueste zuerst')
        else:
            # ⚠⚠ **Windows — und hier zaehlt das Gegenteil.** Dort zeigen alle
            # vier Schreibweisen auf dieselbe Datei. Wuerde ueber den NAMEN
            # entdoppelt statt ueber die Datei-Kennung, meldete der Watcher
            # vier Dateien und einen Hinweis auf ein Problem, das es nicht
            # gibt. Die Entdopplung ist also nicht Kosmetik.
            pruefe(len(_js166.alle_actionmaps(_ordner166)) == 1,
                   'auf diesem Dateisystem ist es EINE Datei — keine '
                   'Schein-Dubletten durch Schreibweisen')
            pruefe(_js166._pfad_actionmaps(_ordner166) is not None,
                   'und sie wird gefunden')

        # Das gilt auf beiden Systemen: aus dem Nichts kommt nichts.
        pruefe(os.path.basename(_js166._pfad_actionmaps(_ordner166))
               == 'actionmaps.xml',
               'zurueck kommt eine actionmaps.xml, kein Ordner')
    finally:
        shutil.rmtree(_ordner166, ignore_errors=True)

    # ⚠ Und ohne jede Datei darf es nicht knallen, sondern `None` geben.
    _leer = _tf166.mkdtemp(prefix='actionmaps-leer-')
    try:
        pruefe(_js166._pfad_actionmaps(_leer) is None,
               'ohne Datei kommt None zurueck, kein Absturz')
        pruefe(_js166.alle_actionmaps(_leer) == [],
               'und eine leere Liste')
    finally:
        shutil.rmtree(_leer, ignore_errors=True)

    # ------------------------------------------------------------------
    # 167. Ein Geraet mit zwei Namen ist EIN Reiter
    #
    # ⚠⚠⚠ Am 06.09.2026 zeigte „Achsen & Kurven" fuenf Reiter fuer drei
    # Sticks: `L-VPC Stick` (Linux-Name) und `LEFT VPC Stick` (alter
    # Windows-Name) standen beide da — dieselbe Kennung, verschiedene Werte.
    # Wer etwas einstellte, traf womoeglich den toten Eintrag.
    print()
    print('167. Ein Geraet mit zwei Namen ist EIN Reiter')
    from scbp import kurven as _kv167

    _bloecke = [
        {'name': 'L-VPC Stick', 'kennung': '{AAA}', 'aktiv': True},
        {'name': 'LEFT VPC Stick', 'kennung': '{AAA}', 'aktiv': True},
        {'name': 'VPC Rudder Pedals', 'kennung': '{BBB}', 'aktiv': True},
    ]
    _echt167 = _kv167._gefuehrte_namen
    try:
        _kv167._gefuehrte_namen = lambda weg: {'{AAA}': 'L-VPC Stick',
                                               '{BBB}': 'VPC Rudder Pedals'}
        _kv167._nur_der_gefuehrte_bleibt(_bloecke, 'egal')
        pruefe([b['aktiv'] for b in _bloecke] == [True, False, True],
               'der gefuehrte Name bleibt, der alte wird zur Leiche')

        # Gegenprobe: Kennt das Spiel den Namen nicht, wird NICHTS weggeraeumt.
        _zwei = [{'name': 'A', 'kennung': '{X}', 'aktiv': True},
                 {'name': 'B', 'kennung': '{X}', 'aktiv': True}]
        _kv167._gefuehrte_namen = lambda weg: {'{X}': 'ganz was anderes'}
        _kv167._nur_der_gefuehrte_bleibt(_zwei, 'egal')
        pruefe(all(b['aktiv'] for b in _zwei),
               'Gegenprobe: passt kein Name, bleibt alles stehen')

        # Gegenprobe: ohne gefuehrte Namen ebenso.
        _drei = [{'name': 'A', 'kennung': '{Y}', 'aktiv': True},
                 {'name': 'B', 'kennung': '{Y}', 'aktiv': True}]
        _kv167._gefuehrte_namen = lambda weg: {}
        _kv167._nur_der_gefuehrte_bleibt(_drei, 'egal')
        pruefe(all(b['aktiv'] for b in _drei),
               'Gegenprobe: ohne Angabe des Spiels bleibt alles stehen')

        # Ein einzelner Block darf nie deaktiviert werden.
        _einer = [{'name': 'Nur ich', 'kennung': '{Z}', 'aktiv': True}]
        _kv167._gefuehrte_namen = lambda weg: {'{Z}': 'anders'}
        _kv167._nur_der_gefuehrte_bleibt(_einer, 'egal')
        pruefe(_einer[0]['aktiv'],
               'ein einzelnes Geraet bleibt aktiv, auch bei anderem Namen')
    finally:
        _kv167._gefuehrte_namen = _echt167

    # ------------------------------------------------------------------
    # 168. Weniger Bauplaene als je zuvor faellt auf
    #
    # ⚠⚠⚠ Am 06.09.2026 zeigte der Watcher nach einem Neustart 406 statt 413
    # Bauplaenen — er las stillschweigend einen anderen Ordner, weil die
    # Zeiger-Datei beim Aufraeumen im Dateimanager mit weggeworfen worden war.
    # Kein Wort dazu; zurueck blieb nur eine kleinere Zahl.
    print()
    print('168. Weniger Bauplaene als je zuvor faellt auf')
    from scbp import bestand as _bs168

    _heim = _tf166.mkdtemp(prefix='schwund-')
    _konf = _tf166.mkdtemp(prefix='schwund-konf-')
    _alt_home = os.environ.get('SC_BP_HOME')
    _alt_konf = os.environ.get('XDG_CONFIG_HOME')
    _alt_appdata = os.environ.get('APPDATA')
    try:
        os.environ['SC_BP_HOME'] = _heim
        os.environ['XDG_CONFIG_HOME'] = _konf
        os.environ['APPDATA'] = _konf          # damit es auch auf Windows greift

        def _lege(n):
            with io.open(_bs168.pfad(), 'w', encoding='utf-8') as _f:
                json.dump({'bauplaene': {'t%03d' % i: {'name': 'T%d' % i}
                                         for i in range(n)},
                           'stand': 'x', 'version': 2}, _f)

        _lege(413)
        pruefe(_bs168.schwund_pruefen(_bs168.laden()) is None,
               'ein voller Bestand meldet nichts und setzt die Marke')

        _lege(406)
        _fund = _bs168.schwund_stand()
        pruefe(_fund is not None and _fund[0] == 406 and _fund[1] == 413,
               'ein geschrumpfter Bestand wird gemeldet (406 statt 413)')

        # ⚠⚠ Das Ansehen darf die Marke NICHT verstellen — sonst waere die
        # Meldung nach einmal Hinsehen fuer immer weg.
        _bs168.schwund_stand()
        pruefe(_bs168.schwund_stand() is not None,
               'Gegenprobe: Ansehen loescht die Meldung nicht')

        _lege(413)
        pruefe(_bs168.schwund_stand() is None,
               'ist alles wieder da, verschwindet die Meldung')

        # ⚠ Ein GEWOLLTES Zuruecksetzen ist kein Verlust.
        _bs168.schwund_pruefen(_bs168.laden())
        _bs168.zuruecksetzen()
        _lege(0)
        pruefe(_bs168.schwund_stand() is None,
               'Gegenprobe: nach bewusstem Zuruecksetzen keine Warnung')
    finally:
        for _name, _wert in (('SC_BP_HOME', _alt_home),
                             ('XDG_CONFIG_HOME', _alt_konf),
                             ('APPDATA', _alt_appdata)):
            if _wert is None:
                os.environ.pop(_name, None)
            else:
                os.environ[_name] = _wert
        shutil.rmtree(_heim, ignore_errors=True)
        shutil.rmtree(_konf, ignore_errors=True)

    # ------------------------------------------------------------------
    # 169. Zwei Zeiger auf den Datenordner — und sie heilen einander
    #
    # ⚠⚠⚠ **Der ganze Datenbestand hing an einer Datei mit einer Zeile.** Am
    # 06.09.2026 wurde sie beim Aufraeumen im Dateimanager mit weggeworfen —
    # zusammen mit zwei fast gleich heissenden Altstaenden daneben, die
    # wirklich Muell waren. Danach schaute das Programm in den Standardordner
    # und zeigte einen kleineren Bestand, ohne ein Wort dazu.
    #
    # Der zweite Zeiger liegt im Konfigurationsordner, wo niemand mit dem
    # Dateimanager aufraeumt. Fehlt einer, wird er aus dem anderen wieder
    # angelegt.
    print()
    print('169. Zwei Zeiger auf den Datenordner heilen einander')
    from scbp import pfade as _pf169

    _daten = _tf166.mkdtemp(prefix='ablage-')
    _dok = _tf166.mkdtemp(prefix='dokumente-')
    _kon = _tf166.mkdtemp(prefix='konfig-')
    _sicher = {k: os.environ.get(k) for k in
               ('SC_BP_HOME', 'XDG_CONFIG_HOME', 'APPDATA', 'HOME')}
    _echt_dok = _pf169._dokumente
    try:
        os.environ.pop('SC_BP_HOME', None)     # sonst gewinnt die Umgebung
        os.environ['XDG_CONFIG_HOME'] = _kon
        os.environ['APPDATA'] = _kon
        _pf169._dokumente = lambda: _dok

        erst = _pf169.zeiger_datei()
        zweit = _pf169._zweitzeiger()
        pruefe(os.path.dirname(erst) != os.path.dirname(zweit),
               'die beiden Zeiger liegen an verschiedenen Orten')

        # 1. Nur der erste ist da -> der zweite entsteht von selbst.
        os.makedirs(os.path.dirname(erst), exist_ok=True)
        with io.open(erst, 'w', encoding='utf-8') as _f:
            json.dump({'ablage_ordner': _daten}, _f)
        pruefe(_pf169._ablage_aus_datei() == _daten,
               'der sichtbare Zeiger wird gelesen')
        pruefe(os.path.isfile(zweit),
               'und legt dabei die Zweitschrift an')

        # 2. ⚠⚠ Der Fall, um den es geht: der sichtbare wird weggeraeumt.
        shutil.rmtree(os.path.dirname(erst), ignore_errors=True)
        pruefe(_pf169._ablage_aus_datei() == _daten,
               'nach dem Loeschen des sichtbaren springt die Zweitschrift ein')
        pruefe(os.path.isfile(erst),
               'und stellt den sichtbaren wieder her')

        # 3. Gegenprobe: Ein Zeiger auf einen Ordner, den es NICHT gibt, darf
        #    nicht gewinnen — sonst entsteht ein leerer Bestand am falschen Ort.
        with io.open(erst, 'w', encoding='utf-8') as _f:
            json.dump({'ablage_ordner': os.path.join(_daten, 'gibtsnicht')}, _f)
        pruefe(_pf169._ablage_aus_datei() == _daten,
               'Gegenprobe: ein Zeiger ins Leere verliert gegen den gueltigen')

        # 4. Gar kein Zeiger -> None, und der Standardordner greift.
        shutil.rmtree(os.path.dirname(erst), ignore_errors=True)
        os.remove(zweit)
        pruefe(_pf169._ablage_aus_datei() is None,
               'ohne jeden Zeiger kommt None zurueck')
    finally:
        _pf169._dokumente = _echt_dok
        for _k, _v in _sicher.items():
            if _v is None:
                os.environ.pop(_k, None)
            else:
                os.environ[_k] = _v
        for _o in (_daten, _dok, _kon):
            shutil.rmtree(_o, ignore_errors=True)

    # ------------------------------------------------------------------
    # 170. Der Merkzettel — Bauplaene farmen ohne Umweg ueber ein Schiff
    #
    # ⭐⭐ Bis v3.20.0 fuehrte jede Materialliste ueber die Wunschliste: erst ein
    # Schiff eintragen, dann Steckplaetze belegen. Fuer einen Helm, eine
    # Ruestung oder eine FPS-Waffe gab es diesen Weg **gar nicht**, obwohl das
    # genauso Bauplaene mit Rohstoffbedarf sind.
    #
    # Gemeldet von Haldjas am 06.09.2026: „‚What to farm' ist irgendwie bisschen
    # unnoetig komplex — man geht da rein, wird dann zu ‚still missing'
    # geschickt und weiss dann aber nicht so genau, was man machen soll. […] Es
    # waere naemlich auch ganz nuetzlich, wenn man nicht nur Schiffsteile,
    # sondern auch Ruestungen/Waffen fuer FPS hinzufuegen koennte zum Workshop."
    print()
    print('170. Der Merkzettel — farmen ohne Umweg ueber ein Schiff')
    from scbp import hangar as _hg170

    _stand = {'format': 1, 'schiffe': []}
    pruefe(_hg170.merkzettel(_stand) == [],
           'ein Stand ohne Merkzettel-Feld gilt als leere Liste')
    pruefe(_hg170.merkzettel_hinzufuegen(_stand, 'BUL-H4 Helmet',
                                         ref='abc', anzahl=2) is True,
           'ein Gegenstand laesst sich vormerken')
    # ⚠⚠ Zweimal dasselbe darf KEINE zweite Zeile geben — sonst zaehlt die
    # Materialliste doppelt und die Anzeige ist unbrauchbar.
    pruefe(_hg170.merkzettel_hinzufuegen(_stand, 'BUL-H4 Helmet',
                                         anzahl=1) is False,
           'derselbe Gegenstand legt keine zweite Zeile an')
    pruefe(_hg170.merkzettel(_stand)[0]['anzahl'] == 3,
           'sondern erhoeht die Stueckzahl (2 + 1 = 3)')
    # ⚠ Gross-/Kleinschreibung darf keinen zweiten Eintrag erzeugen.
    _hg170.merkzettel_hinzufuegen(_stand, 'bul-h4 helmet', anzahl=1)
    pruefe(len(_hg170.merkzettel(_stand)) == 1,
           'Gegenprobe: andere Schreibweise ist derselbe Gegenstand')
    pruefe(_hg170.merkzettel_anzahl_setzen(_stand, 'BUL-H4 Helmet', 5) is True,
           'die Stueckzahl laesst sich setzen')
    pruefe(_hg170.merkzettel(_stand)[0]['anzahl'] == 5, 'und steht dann da')
    pruefe(_hg170.merkzettel_anzahl_setzen(_stand, 'BUL-H4 Helmet', 0) is True,
           'Stueckzahl 0 streicht den Eintrag')
    pruefe(_hg170.merkzettel(_stand) == [], 'und er ist weg')
    pruefe(_hg170.merkzettel_hinzufuegen(_stand, '   ') is False,
           'Gegenprobe: ein leerer Name wird nicht aufgenommen')

    # ⚠⚠ **Die Stueckzahl muss bis ins Material durchschlagen.** Genau daran
    # haengt der Nutzen: „drei Helme" heisst dreifaches Erz, nicht dreimal
    # dieselbe Zeile.
    from scbp import warenkorb as _wk170
    _posten = [{'sorte': _wk170.TEIL, 'weg': _wk170.BAUEN, 'anzahl': 3,
                'bau': {'zustand': _wk170.BEKANNT, 'material': 100.0,
                        'dauer': 60, 'ohne_preis': []},
                'kauf': {'zustand': _wk170.BEKANNT, 'preis': 50.0}}]
    _summe = _wk170.summe(_posten)
    pruefe(abs(_summe['bauen'] - 300.0) < 0.01,
           'drei Stueck kosten dreimal so viel Material (100 -> 300)')
    pruefe(_summe['dauer'] == 180,
           'und dauern dreimal so lange (60 -> 180)')
    _posten[0]['weg'] = _wk170.KAUFEN
    pruefe(abs(_wk170.summe(_posten)['kaufen'] - 150.0) < 0.01,
           'beim Kaufen ebenso (50 -> 150)')
    # Gegenprobe: Ein Schiffsteil OHNE `anzahl` bleibt bei einfach.
    del _posten[0]['anzahl']
    pruefe(abs(_wk170.summe(_posten)['kaufen'] - 50.0) < 0.01,
           'Gegenprobe: ohne Stueckzahl bleibt es bei einfach')

    # ⚠⚠⚠ **Ein Merkzettel-Posten hat KEINE Entitaets-Kennung.** Er entsteht in
    # der Herstellungsliste, und die kennt nur den Bauplannamen. Beim ersten
    # Anlauf landete der Name im `ref`-Feld, das Bauplan-Verzeichnis fand
    # nichts — und der Posten fiel stillschweigend auf „kaufen" zurueck.
    #
    # Auf „Was ich farmen muss" stand daraufhin gleichzeitig „2 Teile konnten
    # nicht gerechnet werden" UND „Alles da — du kannst sofort loslegen", bei
    # null Erz im Lager. Zwei Saetze, die sich widersprechen, und beide falsch.
    # Gemeldet am 06.09.2026: „Man sieht da aber kein Material, was man farmen
    # muss."
    _echt_rez = None
    try:
        from scbp import herstellung as _hs170
        _echt_rez = _hs170.rezept
        _hs170.rezept = lambda name: (
            {'stufen': [{'zutaten': [(0, 'Agricium', 2.0, 0)]}], 'dauer': 60}
            if name == 'Testwaffe' else None)

        _bau = _wk170.bauweg('gibtsnicht', {}, name='Testwaffe')
        pruefe(_bau['zustand'] == _wk170.BEKANNT,
               'ohne Kennung findet der Bauweg das Rezept ueber den NAMEN')
        pruefe(_bau['bauplan'] == 'Testwaffe',
               'und nennt den Bauplan beim Namen')

        # Gegenprobe: Ein Name, zu dem es kein Rezept gibt, darf NICHT als
        # bekannt gelten — sonst behauptet die Seite eine Materialliste, die
        # es nicht gibt.
        _leer = _wk170.bauweg('', {}, name='Gibtsnichtwaffe')
        pruefe(_leer['zustand'] != _wk170.BEKANNT,
               'Gegenprobe: ohne Rezept bleibt es beim Zustand KEIN_REZEPT')
        pruefe(_wk170.bauweg('', {}, name='')['zustand'] != _wk170.BEKANNT,
               'Gegenprobe: ganz ohne Angabe ebenso')
    finally:
        if _echt_rez is not None:
            _hs170.rezept = _echt_rez

    # ------------------------------------------------------------------
    # 171. Erledigte Merkposten fliegen beim Start raus
    #
    # ⚠⚠ **`erledigen()` greift nur beim FUND.** Wer einen Bauplan merkt, den
    # er laengst hat, behaelt den Merkposten fuer immer. Am 06.09.2026 stand
    # `H4-PBF Ammo Carrier` unter „beobachtet", obwohl er in derselben Liste
    # ein Haekchen trug: „da wird einer beobachtet, den ich schon habe."
    print()
    print('171. Erledigte Merkposten fliegen beim Start raus')
    from scbp import merkliste as _mk171

    _heim171 = _tf166.mkdtemp(prefix='merk-')
    _alt171 = os.environ.get('SC_BP_HOME')
    try:
        os.environ['SC_BP_HOME'] = _heim171
        _mk171.speichern({'namen': ['Habe Ich', 'Fehlt Mir'],
                          'eintraege': [{'titel': 'Muster',
                                         'muster': ['morozov']}]})
        _weg = _mk171.aufraeumen(['habe ich'])
        pruefe(_weg == 1, 'genau ein erledigter Posten wird ausgetragen')
        _jetzt = _mk171.laden()
        pruefe(_jetzt['namen'] == ['Fehlt Mir'],
               'der noch fehlende bleibt stehen')
        # ⚠ Muster bleiben unangetastet: „Morozov" steht fuer mehrere Teile,
        # von denen erst eines da sein kann.
        pruefe(len(_jetzt['eintraege']) == 1,
               'Muster-Eintraege bleiben unberuehrt')
        pruefe(_mk171.aufraeumen([]) == 0,
               'Gegenprobe: ohne Bestand wird nichts ausgetragen')
        pruefe(_mk171.aufraeumen(['gibtsnicht']) == 0,
               'Gegenprobe: ein fremder Name traegt nichts aus')

    finally:
        if _alt171 is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _alt171
        shutil.rmtree(_heim171, ignore_errors=True)

    # ------------------------------------------------------------------
    # 172. Die Mengen am Eintrag addieren sich zur Summe darunter
    #
    # ⚠⚠⚠ **Zwei Zahlen mit derselben Beschriftung auf einer Seite.** Am
    # 06.09.2026 stand bei einem Bauteil „hast 0,00" und zehn Zeilen tiefer
    # „hast 3,44" fuer denselben Rohstoff — Ursache war ein `float()` auf das
    # TUPEL aus `menge_mit_guete()`, dessen Ausnahme ein `except` daneben
    # stillschweigend zu `0.00` machte.
    #
    # Nach dem Fix stand oben der volle Lagerbestand (8,01) und unten der
    # zugeteilte Anteil (3,44) — beide richtig gerechnet und trotzdem ein
    # Widerspruch. Der Eintrag nennt deshalb nur noch den **Bedarf**; ob es
    # reicht, sagt die Farbe aus derselben Rechnung.
    #
    # ⚠ Geprueft wird die Rechenregel: Die Einzelbedarfe muessen sich zum
    # Gesamtbedarf addieren. Tun sie das nicht, zeigt die Seite zwei
    # verschiedene Wahrheiten, egal wie sie beschriftet sind.
    print()
    print('172. Die Mengen am Eintrag addieren sich zur Summe darunter')

    # ⚠ `menge_mit_guete` gibt ein TUPEL — das ist die Falle von oben.
    _probe = _mz172 = None
    from scbp import rohstoffe as _rs172
    _probe = _rs172.menge_mit_guete('Gibtsnichterz', 0)
    pruefe(isinstance(_probe, tuple) and len(_probe) == 2,
           'menge_mit_guete gibt ein Tupel (passend, zu_gering) zurueck')
    _fehler172 = False
    try:
        float(_probe)
    except TypeError:
        _fehler172 = True
    pruefe(_fehler172,
           'Gegenprobe: float() darauf wirft — genau das war der Fehler')

    _echt_rez172 = _hs170.rezept
    _echt_bp172 = _wk170._bauplan_verzeichnis
    _heim172 = _tf166.mkdtemp(prefix='mengen-')
    _alt172 = os.environ.get('SC_BP_HOME')
    try:
        os.environ['SC_BP_HOME'] = _heim172
        _hs170.rezept = lambda name: (
            {'stufen': [{'zutaten': [(0, 'Agricium', 2.0, 0)]}], 'dauer': 60}
            if name in ('Waffe A', 'Waffe B') else None)
        _wk170._bauplan_verzeichnis = lambda: {}

        from scbp import hangar as _hg172
        _stand172 = {'format': 1, 'schiffe': []}
        _hg172.merkzettel_hinzufuegen(_stand172, 'Waffe A', anzahl=4)
        _hg172.merkzettel_hinzufuegen(_stand172, 'Waffe B', anzahl=2)

        # Einzelbedarfe, wie sie am Eintrag stehen: 4 x 2,0 und 2 x 2,0
        _einzeln = 4 * 2.0 + 2 * 2.0
        _liste = _wk170.farmliste(_stand172)
        _gesamt = 0.0
        for _e in ((_liste.get('fehlt') or [])
                   + (_liste.get('vollstaendig') or [])):
            if (_e.get('rohstoff') or '').lower() == 'agricium':
                _gesamt = float(_e.get('benoetigt') or 0)
        pruefe(abs(_gesamt - _einzeln) < 0.001,
               'Summe (%.1f) = Einzelmengen (%.1f)' % (_gesamt, _einzeln))
        pruefe((_liste.get('posten') or 0) == 6,
               'sechs geplante Bauteile (4 + 2), nicht zwei Zeilen')
    finally:
        _hs170.rezept = _echt_rez172
        _wk170._bauplan_verzeichnis = _echt_bp172
        if _alt172 is None:
            os.environ.pop('SC_BP_HOME', None)
        else:
            os.environ['SC_BP_HOME'] = _alt172
        shutil.rmtree(_heim172, ignore_errors=True)

    print()
    if fehler:
        print('%d von %d Prüfungen fehlgeschlagen:' % (len(fehler), geprueft[0]))
        for f in fehler:
            print('  ·', f)
        return 1
    print('Alle Prüfungen bestanden.')
    return 0


def _versionierte_dateien(wurzel, endungen=('.py', '.md', '.yml')):
    """Die Dateien, die wirklich veroeffentlicht werden — laut Git.

    ⚠ **Nicht `os.walk`.** Der Maßstab ist nicht, was auf der Platte liegt,
    sondern was im Repo landet: Eine Anleitung, die per `.gitignore`
    ausgeschlossen ist, darf privates Beiwerk enthalten — sie geht niemanden
    an, weil sie nirgends hinkommt. Am 30.08.2026 meldete die Pruefung genau
    so eine Datei, waehrend der Bau-Laeufer sie gar nicht kannte: lokal rot,
    im Bau gruen. Zwei verschiedene Wahrheiten ueber dieselbe Frage.

    Ohne Git (entpacktes Archiv) faellt die Pruefung auf das Dateisystem
    zurueck — dann lieber zu viel pruefen als zu wenig.
    """
    import subprocess
    try:
        roh = subprocess.run(['git', '-C', wurzel, 'ls-files'],
                             capture_output=True, text=True, timeout=30)
        if roh.returncode == 0 and roh.stdout.strip():
            return [z for z in roh.stdout.splitlines() if z.endswith(endungen)]
    except Exception:
        pass
    raus = []
    for ordner, _o, namen in os.walk(wurzel):
        if any(x in ordner for x in ('.git', 'assets', 'build', 'dist')):
            continue
        for n in namen:
            if n.endswith(endungen):
                raus.append(os.path.relpath(os.path.join(ordner, n), wurzel))
    return raus


def _wurzel():
    """Ein unsichtbares Fenster, nur um die Bildschirmgröße erfragen zu können."""
    import tkinter as tk
    r = tk.Tk()
    r.withdraw()
    return r


if __name__ == '__main__':
    sys.exit(main())
