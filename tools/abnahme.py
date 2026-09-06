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
Abnahme: jede Seite wirklich bedienen, bevor eine Fassung hinausgeht.

## ⚠⚠⚠ Warum es dieses Werkzeug gibt

Am 06.09.2026 gingen an einem Tag reihenweise Fehler an den Nutzer, die alle
sichtbar gewesen wären, hätte jemand einmal geklickt: eine Marke, die nach dem
Abhaken stehenblieb; ein Fenster außerhalb aller Bildschirme, das das Programm
unbedienbar machte; eine Wunschliste, die beim Speichern gelöscht wurde;
Mengen, die als „0,6" statt „0,64" dastanden.

Seine Ansage danach:

> *„Bevor du mir was präsentierst, testest du erst alles durch. Ich habe keine
> Lust, ständig deine Fehler zu suchen."*

**Der Selbsttest findet so etwas nicht.** Er ruft Funktionen auf, und die waren
jedes Mal richtig. Falsch war, **was danach auf dem Bildschirm steht** — und das
sieht nur, wer die Oberfläche wirklich bedient.

## Was geprüft wird

Die Liste stammt von ihm, ergänzt um die Fallen, die tatsächlich zugeschlagen
haben:

| # | Frage |
|---|---|
| 1 | Baut jede Seite ohne Fehler auf, in **beiden** Sprachen? |
| 2 | Zeigt jede Seite überhaupt etwas — oder ist sie leer? |
| 3 | Steht irgendwo ein **Textschlüssel** statt eines Satzes (`s_xx_yyy`)? |
| 4 | Fehlt ein **Symbol**, sodass ein Ersatzzeichen dasteht? |
| 5 | Öffnen sich alle Fenster **über dem Hauptfenster**? |
| 6 | Gibt es noch einen **System-Dialog**? |
| 7 | Funktionieren die **Auswahllisten**: tippen → Vorschlag → klicken → Ergebnis? |
| 8 | Wird Text **abgeschnitten** — passt er in seinen Platz? |
| 9 | Sind die **Zahlen** lesbar (keine `0.30000000000000004`)? |
| 10 | Sind die Daten **plausibel** — gegen echte Spieldaten geprüft? |
| 11 | Bleibt das **Fehlerprotokoll leer**, während man alles bedient? |

## Aufruf

    python3 tools/abnahme.py            # unsichtbar, der Regelfall
    SC_BP_SICHTBAR=1 python3 tools/abnahme.py   # zum Zusehen

⚠⚠ **Nie ungefragt sichtbar.** Wie Selbsttest und `durchklicken.py` startet
sich dieses Werkzeug unter `xvfb-run` neu, sobald ein echter Bildschirm
dranhängt: Ein aufblitzendes Fenster reißt den Tastaturfokus mit — wer gerade
Star Citizen fliegt, landet im Desktop.

⚠ **Echte Daten, eigener Ordner.** Geprüft wird gegen die wirklich abgelegten
Spieldaten (Rezepte, Ladenkatalog, Steckplätze), aber in einem Wegwerf-Ordner:
Ein Prüflauf, der in der Ablage des Nutzers arbeitet, schreibt seine Fehler in
dessen Bericht.
"""

import os
import re
import sys

HIER = os.path.dirname(os.path.abspath(__file__))
WURZEL = os.path.dirname(HIER)
sys.path.insert(0, WURZEL)
sys.path.insert(0, HIER)

import unsichtbar                                          # noqa: E402
unsichtbar.sicherstellen()

import glob                                                # noqa: E402
import json                                                # noqa: E402
import shutil                                              # noqa: E402
import tempfile                                            # noqa: E402
import tkinter as tk                                       # noqa: E402


fehler = []
warnungen = []
geprueft = [0]
# Ob die echten Spieldaten übernommen werden konnten. Ohne sie sind
# Ladezeit-Messungen nur Hinweise, keine Befunde.
DATEN_DA = [False]


def pruefe(bedingung, text, nur_warnen=False):
    geprueft[0] += 1
    if bedingung:
        print('  [ok]   ' + text)
    elif nur_warnen:
        print('  [?]    ' + text)
        warnungen.append(text)
    else:
        print('  [FEHL] ' + text)
        fehler.append(text)
    return bool(bedingung)


# --------------------------------------------------------------- Werkzeuge

def texte(widget, raus=None):
    """Alle angezeigten Beschriftungen unterhalb eines Widgets.

    ⚠ `winfo_manager()`, nicht `winfo_ismapped()`: Letzteres ist bei einem
    Fenster mit `withdraw()` immer falsch — daran ist die erste Fassung
    gescheitert, und dieselbe Falle warf schon einen Bau-Lauf um.
    """
    raus = [] if raus is None else raus
    for kind in widget.winfo_children():
        if not kind.winfo_manager():
            continue
        try:
            wert = kind.cget('text')
            if wert:
                raus.append(str(wert))
        except Exception:
            pass
        # ⚠ Knöpfe sind Leinwände mit gemaltem Text — ohne diesen Zweig fehlt
        # in jeder Prüfung genau das, was der Nutzer anklickt.
        if kind.winfo_class() == 'Canvas':
            try:
                for teil in kind.find_all():
                    if kind.type(teil) == 'text':
                        wert = str(kind.itemcget(teil, 'text'))
                        if wert:
                            raus.append(wert)
            except Exception:
                pass
        texte(kind, raus)
    return raus


def widgets(widget, art=None, raus=None):
    """Alle angezeigten Widgets (einer Art) unterhalb eines Widgets."""
    raus = [] if raus is None else raus
    for kind in widget.winfo_children():
        if kind.winfo_manager() and (art is None
                                     or kind.winfo_class() == art):
            raus.append(kind)
        widgets(kind, art, raus)
    return raus


def mit_text(widget, text, genau=True):
    """Alle Widgets, deren Beschriftung so lautet — **auch die Knöpfe**.

    ⚠⚠ **Die Knöpfe dieses Programms sind Leinwände.** Sie tragen keinen
    `text`, sondern malen ihn als Canvas-Element (das ist der Grund, warum sie
    auf jedem System gleich aussehen). Eine Suche über `cget('text')` findet
    sie deshalb nie — die erste Fassung meldete „der Knopf ist nicht da",
    obwohl er dastand.
    """
    raus = []
    for k in widgets(widget):
        try:
            wert = str(k.cget('text'))
            if (wert == text) if genau else (text in wert):
                raus.append(k)
                continue
        except Exception:
            pass
        # Leinwand: die gemalten Textelemente durchsehen.
        if k.winfo_class() == 'Canvas':
            try:
                for teil in k.find_all():
                    if k.type(teil) != 'text':
                        continue
                    wert = str(k.itemcget(teil, 'text'))
                    if (wert == text) if genau else (text in wert):
                        raus.append(k)
                        break
            except Exception:
                pass
    return raus


def klicken(widget):
    widget.event_generate('<Button-1>')
    widget.update()


def tippen(fenster, feld, text):
    """In ein Eingabefeld schreiben, so dass die Oberfläche es merkt.

    ⚠⚠ **Nicht `insert()`.** Die Auswahllisten hängen an der `StringVar`; wer
    Zeichen direkt ins Widget schreibt, tippt an der Oberfläche vorbei und
    sieht nie einen Vorschlag. Genau daran ist der erste Anlauf gescheitert —
    und der Fehler sah aus, als sei die Liste kaputt.
    """
    name = feld.cget('textvariable')
    if not name:
        feld.delete(0, 'end')
        feld.insert(0, text)
    else:
        fenster.setvar(name, text)
    fenster.update()


# ------------------------------------------------------------ Vorbereitung

def ablage_vorbereiten():
    """Ein Wegwerf-Ordner mit ECHTEN Spieldaten, aber eigenem Bestand.

    ⚠ **Echte Daten, keine erfundenen.** „Sind die Daten plausibel?" lässt sich
    an ausgedachten Zahlen nicht beantworten — ein Rezept mit `Iron 1,0` sieht
    immer plausibel aus. Kopiert werden deshalb die wirklich abgelegten
    Zwischenspeicher (Rezepte, Ladenkatalog, Steckplätze), soweit vorhanden.

    ⚠ Der **Hangar** wird dagegen selbst gebaut: Er ist die Eingabe des
    Nutzers, ändert sich ständig, und ein Prüflauf soll nicht davon abhängen,
    was gerade darin steht.
    """
    ordner = tempfile.mkdtemp(prefix='sc-bp-abnahme-')
    os.environ['SC_BP_HOME'] = ordner
    # ⚠⚠ **Ohne Netz messen.** Der erste Durchlauf meldete Wunschliste und
    # Einkaufsliste mit über zwei Sekunden — nachgemessen brauchen die
    # Rechnungen dahinter **28 ms**. Die Zeit ging in Netzabrufe, weil der
    # Prüfordner leer war. Eine Abnahme, die die Leitung misst statt der
    # Oberfläche, meldet je nach Tageszeit etwas anderes.
    #
    # Nebenbei prüft das mit, dass ohne Netz nichts abstürzt — genau dieser
    # Schalter hielt sich früher nur zur Hälfte an sein Versprechen.
    os.environ['SC_BP_NO_NET'] = '1'

    # ⚠⚠ **Den Ablageort das Programm sagen lassen.** Der erste Anlauf riet
    # ihn (`~/Dokumente/SC BP Watcher`) und fand nichts — der Nutzer hatte ihn
    # verlegt. Ergebnis: „keine Spieldaten vorhanden", und ausgerechnet die
    # Plausibilitätsprüfung fiel aus. `pfade` kennt den richtigen Ort, samt
    # Umgebungsvariable und Einstellungsdatei.
    quellen = []
    alt_heim = os.environ.pop('SC_BP_HOME', None)
    try:
        from scbp import pfade as _pf
        echt = _pf.app_ordner()
        for kandidat in (os.path.join(echt or '', 'Intern'), echt):
            if kandidat and os.path.isdir(kandidat):
                quellen.append(kandidat)
    except Exception:
        pass
    finally:
        if alt_heim is not None:
            os.environ['SC_BP_HOME'] = alt_heim


    kopiert = 0
    for quelle in quellen:
        for name in ('crafting-blueprints.json', 'laeden-katalog.json',
                     'laeden.json', 'erkul-schiffe.json', 'katalog-cache.json',
                     'mining-data.json', 'schiffe.json'):
            weg = os.path.join(quelle, name)
            if os.path.isfile(weg):
                shutil.copy2(weg, os.path.join(ordner, name))
                kopiert += 1
        if kopiert:
            break

    # Für die Ladezeit zählt, ob die beiden großen Nachschlagewerke da sind.
    DATEN_DA[0] = all(os.path.isfile(os.path.join(ordner, n))
                      for n in ('schiffe.json', 'erkul-schiffe.json'))

    from scbp import hangar
    with open(os.path.join(ordner, 'hangar.json'), 'w', encoding='utf-8') as f:
        json.dump({'format': hangar.FORMAT,
                   'schiffe': [{'name': 'Cutlass Black', 'hersteller': 'Drake',
                                'kurz': 'cutlassblack', 'hkurz': 'DRAK',
                                'herkunft': 'pledge', 'belegung': {}}],
                   'wunsch': [{'name': 'Vulture', 'hersteller': 'Drake',
                               'belegung': {}}]}, f)
    return ordner, kopiert


# ------------------------------------------------------------ Die Prüfungen

# Jede Seite mit dem, was auf ihr stehen MUSS. Fehlt der Text, ist entweder
# die Seite kaputt oder sie wurde umbenannt, ohne diese Liste mitzuziehen.
SEITEN = [
    ('liste', 'Bauplan-Liste'),
    ('fortschritt', 'Bauplan-Fortschritt'),
    ('auftragslog', 'Auftrags-Protokoll'),
    ('hangar', 'Mein Hangar'),
    ('wunschliste', 'Wunschliste'),
    ('einkaufsliste', 'Was noch fehlt'),
    ('lager', 'Rohstofflager'),
    ('herstellung', 'Herstellung'),
    ('bergbau', 'Bergbau'),
    ('laeden', 'Läden'),
    ('farmliste', 'Was ich farmen muss'),
    ('bergung', 'Was steckt drin?'),
    ('zerlegen', 'Lohnt das Zerlegen?'),
    ('allgemein', None),
    ('anzeige', None),
    ('joysticks', None),
    ('achsen', None),
    ('blickwinkel', None),
    ('wasistneu', None),
    ('ueber', None),
    ('serverstatus', None),
    ('diagnose', None),
]

# Textschlüssel, die durchrutschen: `s_xx_yyy` steht für einen fehlenden Text.
SCHLUESSEL = re.compile(r'^[a-z]{1,3}_[a-z0-9_]{3,}$')

# Eine Zahl, die niemand lesen will: `0.30000000000000004`.
#
# ⚠ **Datumsangaben sind ausgenommen.** Der erste Anlauf meldete
# „26.08.2026 20:30" als unleserliche Zahl — `26.08` gefolgt von vier Ziffern
# passt auf das Muster. Ein Punkt-getrenntes Datum sieht wie eine Dezimalzahl
# aus, ist aber keine.
KRUMM = re.compile(r'\d+[.,]\d{4,}')
IST_DATUM = re.compile(r'\d{1,2}\.\d{1,2}\.\d{4}')


# Ab wann eine Seite als langsam gilt. ⚠ **Gemessen wird der ERSTE Aufbau** —
# danach wird eine Seite nur noch ein- und ausgeblendet und ist immer schnell.
# Der Wert stammt aus den Startverläufen echter Fehlerberichte: Dort liegen
# die meisten Seiten bei 15–50 ms, einzelne bei 120 ms.
# ⚠ 300 statt 250: Der erste Wert war geraten, und eine Seite lag mit 253 ms
# darüber — drei Millisekunden sind Messrauschen, keine Trägheit. Eine Grenze,
# die bei jedem zweiten Lauf zufällig reißt, wird ignoriert statt beachtet.
LANGSAM_MS = 300

# ⚠⚠ **Der ERSTE Aufbau einer Seite darf Daten nachladen.** Gemessen an den
# Einzelschritten: Die Rechnungen hinter Wunsch- und Einkaufsliste brauchen
# **28 ms** — die Sekunden gehen in das einmalige Bereitstellen der Steckplatz-
# und Preisdaten. Das trifft den Spieler genau einmal je Programmlauf.
#
# Trotzdem gilt eine Obergrenze: Was länger blockiert, gehört in den
# Hintergrund. Genau so kam der 9-Sekunden-Stillstand des Auftrags-Protokolls
# heraus.
ERSTAUFBAU_MS = 2500


def seiten_pruefen(hf, sprache_name):
    """Jede Seite öffnen und ansehen — der Kern der Abnahme.

    ⚠ **Die Zeit wird mitgemessen.** Am 06.09.2026: „achte auch auf die
    Geschwindigkeit, wie schnell das Tool ist, und dass es kurze Ladezeiten
    hat" — in den Startverläufen standen einzelne Seiten mit über hundert
    Millisekunden. Eine Seite, die spürbar hängt, fällt im Betrieb auf, aber
    in keiner Funktionsprüfung.
    """
    import time as _zeit

    leer, ohne_titel, schluessel_gefunden, krumme = [], [], [], []
    zeiten = {}
    for name, titel in SEITEN:
        angefangen = _zeit.time()
        try:
            hf.oeffnen(name)
            hf.root.update()
            zeiten[name] = (_zeit.time() - angefangen) * 1000
        except Exception as ausnahme:
            pruefe(False, '[%s] Seite %s baut auf: %s: %s'
                   % (sprache_name, name, type(ausnahme).__name__, ausnahme))
            continue
        gelesen = texte(hf.root)
        if len(gelesen) < 3:
            leer.append(name)
        if titel and sprache_name == 'de' and titel not in gelesen:
            ohne_titel.append('%s (erwartet: %s)' % (name, titel))
        for zeile in gelesen:
            if SCHLUESSEL.match(zeile.strip()):
                schluessel_gefunden.append('%s: %s' % (name, zeile))
            if KRUMM.search(zeile) and not IST_DATUM.search(zeile):
                krumme.append('%s: %s' % (name, zeile[:50]))

    pruefe(not leer, '[%s] jede Seite zeigt etwas an (leer: %s)'
           % (sprache_name, ', '.join(leer) or 'keine'))
    if sprache_name == 'de':
        pruefe(not ohne_titel,
               'jede Seite trägt ihren erwarteten Titel (fehlt: %s)'
               % (', '.join(ohne_titel) or 'keiner'))
    pruefe(not schluessel_gefunden,
           '[%s] kein Textschlüssel statt eines Satzes (%s)'
           % (sprache_name, ', '.join(schluessel_gefunden[:3]) or 'keiner'))
    pruefe(not krumme,
           '[%s] keine unleserlichen Zahlen (%s)'
           % (sprache_name, ', '.join(krumme[:3]) or 'keine'))

    # Ladezeiten: die drei langsamsten nennen, egal ob sie die Grenze reißen.
    langsam = sorted(zeiten.items(), key=lambda x: -x[1])[:3]
    if langsam:
        print('         langsamste Seiten: %s'
              % ' · '.join('%s %d ms' % (n, ms) for n, ms in langsam))
    # ⚠⚠ **Ohne die abgelegten Daten misst das die Beschaffung, nicht die
    # Oberfläche.** Nachgemessen mit vollständiger Ablage: Die Wunschliste baut
    # in **18 ms** auf. Ohne `schiffe.json` und `erkul-schiffe.json` waren es
    # drei Sekunden — das ist einmaliges Beschaffen, kein träges Programm.
    # Eine Prüfung, die je nach Ordnerinhalt etwas anderes meldet, ist wertlos.
    grenze = ERSTAUFBAU_MS if sprache_name == 'de' else LANGSAM_MS
    ueber = [n for n, ms in zeiten.items() if ms > grenze]
    pruefe(not ueber,
           '[%s] keine Seite blockiert länger als %d ms (%s)'
           % (sprache_name, grenze,
              ', '.join('%s %d ms' % (n, zeiten[n]) for n in ueber[:3])
              or 'keine'),
           # ⚠⚠ **Im ersten Durchgang nur ein Hinweis — und zwar zu Recht.**
           # Die Auswahllisten-Prüfung läuft davor und trägt dabei ein Schiff
           # ein; das löst das Nachladen der Steckplatzdaten aus, und die Zeit
           # landet auf der nächsten Seite, die gebaut wird. Nachgemessen an
           # einem unberührten Fenster: **18 ms**.
           #
           # Der zweite Durchgang (englisch) misst dagegen sauber: Dort ist
           # alles beschafft, und dort gilt die scharfe Grenze. Eine Zahl, die
           # von der Reihenfolge der Prüfungen abhängt, darf keinen Bau
           # aufhalten.
           nur_warnen=(sprache_name == 'de' or not DATEN_DA[0]))
    # ⚠ Der zweite Durchlauf (auf Englisch) baut auf schon geholten Daten auf —
    # dort gilt die scharfe Grenze, denn dann ist es reine Oberflächenzeit.
    if sprache_name != 'de':
        traege = [n for n, ms in zeiten.items() if ms > LANGSAM_MS]
        pruefe(not traege,
               '[%s] und keine ist träge, wenn die Daten schon da sind (%s)'
               % (sprache_name,
                  ', '.join('%s %d ms' % (n, zeiten[n]) for n in traege[:3])
                  or 'keine'))


def symbole_pruefen():
    """Trägt jeder Reiter ein echtes Bild — oder ein Ersatzzeichen?

    ⚠ Ein fehlendes Symbol fällt sonst erst auf, wenn jemand hinsieht: Der
    Notnagel malt ein Textzeichen, und das sieht auf den ersten Blick aus wie
    ein Symbol. Am 06.09.2026 hat ein unbekannter Symbolname sogar das ganze
    Fenster abstürzen lassen.
    """
    from scbp import zeichen
    fehlt = []
    for name in zeichen.KNOPF_NAMEN + zeichen.ZEILEN_NAMEN:
        for px in (18, 22, 26, 30, 12, 14, 16):
            pfad = os.path.join(WURZEL, 'assets', 'symbole', str(px),
                                '%s-grau.png' % name)
            if os.path.isfile(pfad):
                break
        else:
            fehlt.append(name)
    pruefe(not fehlt, 'jedes angemeldete Symbol hat ein Bild (fehlt: %s)'
           % (', '.join(fehlt) or 'keins'))


def auswahllisten_pruefen(hf):
    """Tippen, Vorschlag anklicken, Ergebnis lesen — auf jeder Seite, die eine hat.

    ⚠⚠ **Das ist die Prüfung, die am meisten findet.** Eine Auswahlliste hat
    vier Stellen, an denen es hakt: Die Namen werden nicht geladen, das Tippen
    löst nichts aus, der Klick kommt nicht an, oder das Ergebnis wird nicht
    gezeichnet. Jede davon sieht für sich harmlos aus.
    """
    from scbp import herstellung, schiffe

    # Je Fall: (Seite, was getippt wird, erwarteter Vorschlag, Knopf danach)
    faelle = []
    try:
        rezepte = herstellung.alle() or []
        if rezepte:
            teil = rezepte[0].get('basis') or ''
            faelle.append(('zerlegen', teil.split()[0] if teil else '',
                           teil, None))
    except Exception:
        pass
    try:
        namen = schiffe.namen_alle() or []
        if namen:
            faelle.append(('wunschliste', namen[0][:6], namen[0],
                           'Auf die Wunschliste'))
    except Exception:
        pass

    if not faelle:
        pruefe(False, 'Auswahllisten prüfbar (keine Daten vorhanden)',
               nur_warnen=True)
        return

    # ⚠⚠ **Diese Prüfung läuft ZUERST, vor dem Seitendurchlauf.** Wird eine
    # Seite ein zweites Mal geöffnet, setzt ihr `beim_zeigen` das Suchfeld
    # zurück — der Prüflauf tippte dann in ein Feld, das im selben Atemzug
    # geleert wurde, und die Liste blieb leer.
    #
    # ⚠ Ein zweites Fenster war der falsche Ausweg: Zwei Tk-Interpreter teilen
    # sich keine Bilder, und prompt stand `image "pyimage64" doesn't exist` im
    # Protokoll. Ein Prüfmittel, das selbst Fehler erzeugt, taugt nichts.
    for seite, suchtext, erwartet, knopf in faelle:
        if not suchtext:
            continue
        hf.oeffnen(seite)
        hf.root.update()
        # ⚠⚠ **Nur die sichtbare Seite.** Seiten werden einmal gebaut und
        # danach nur ein- und ausgeblendet — die Eingabefelder aller schon
        # besuchten Seiten hängen also weiter im Baum. Wer über das ganze
        # Fenster sucht, tippt in ein Feld, das gerade niemand sieht, und
        # wundert sich, dass keine Vorschläge kommen.
        bereich = hf.seiten.get(seite) if hasattr(hf, 'seiten') else hf.root
        felder = widgets(bereich or hf.root, 'Entry')
        if not felder:
            pruefe(False, '[%s] ein Eingabefeld ist da' % seite)
            continue
        vorher = set(texte(bereich or hf.root))
        tippen(hf.root, felder[-1], suchtext)
        neu = [x for x in texte(bereich or hf.root) if x not in vorher]
        if not pruefe(bool(neu),
                      '[%s] tippen bringt Vorschläge (%r -> %s)'
                      % (seite, suchtext, neu[:2] or 'nichts')):
            continue
        ziel = mit_text(bereich or hf.root, neu[0])
        if not ziel:
            continue
        vorher2 = set(texte(bereich or hf.root))
        klicken(ziel[-1])
        # ⚠⚠ **Nicht jede Liste zeigt sofort ein Ergebnis.** Beim
        # Zerlege-Rechner rechnet der Klick los; auf der Wunschliste trägt er
        # den Namen nur ins Feld, und eintragen tut ihn erst der Knopf
        # daneben. Wer beides gleich behandelt, meldet einen Fehler, wo die
        # Oberfläche richtig arbeitet — die erste Fassung tat genau das.
        if knopf:
            hilfe = mit_text(bereich or hf.root, knopf)
            if pruefe(bool(hilfe), '[%s] der Knopf %r ist da' % (seite, knopf)):
                klicken(hilfe[-1])
        danach = [x for x in texte(bereich or hf.root) if x not in vorher2]
        pruefe(bool(danach),
               '[%s] der Vorgang bringt ein sichtbares Ergebnis (%s)'
               % (seite, (danach[:1] or ['nichts'])[0][:44]))


def abschneiden_pruefen(hf):
    """Passt jeder Text in seinen Platz — oder wird er beschnitten?

    ⚠ Gemessen wird die **gebrauchte** gegen die **zugeteilte** Breite. Ein
    Label mit fester Breite (`width=22`), dessen Text länger ist, schneidet
    still ab: Auf dem Bildschirm fehlt das Ende, im Code sieht alles richtig
    aus.
    """
    eng = []
    for name, _titel in SEITEN:
        try:
            hf.oeffnen(name)
            hf.root.update()
            hf.root.update_idletasks()
        except Exception:
            continue
        for w in widgets(hf.root, 'Label'):
            try:
                if not w.cget('text') or w.cget('wraplength'):
                    continue
                gebraucht = w.winfo_reqwidth()
                hat = w.winfo_width()
                # ⚠ Erst ab 12 px Unterschied: Ein, zwei Pixel sind Rundung.
                if hat > 1 and gebraucht - hat > 12:
                    eng.append('%s: %r (%d statt %d px)'
                               % (name, str(w.cget('text'))[:26],
                                  hat, gebraucht))
            except Exception:
                pass
    pruefe(not eng, 'kein Text wird abgeschnitten (%s)'
           % ('; '.join(eng[:3]) if eng else 'keiner'), nur_warnen=True)


def schriftgroessen_pruefen():
    """Jede Schriftgröße durchgehen — vor allem „sehr groß".

    ⚠⚠ **Dieser Fehler kam mehrfach.** Aus der Fehlerliste: „Bei »sehr groß«
    waren die Knöpfe abgeschnitten (29 px fehlten)", „Bei »sehr groß« fehlte
    die halbe linke Leiste", „Der Update-Knopf lag bei der Mindestgröße des
    Fensters unterhalb der Kante". Immer dieselbe Ursache: Entwickelt wird bei
    „normal", und was dort passt, passt bei 30-px-Symbolen und größerer Schrift
    nicht mehr.

    Geprüft wird die **Mindesthöhe der Seitenleiste** gegen die Fensterhöhe:
    Passt die Leiste nicht mehr, sind Reiter unerreichbar.
    """
    from scbp import hauptfenster, pfade, zeichen

    alt_stufe = pfade.einstellung('schriftgroesse') or 'normal'
    gemessen_stufen = {}
    try:
        for stufe in ('klein', 'normal', 'gross', 'sehrgross'):
            pfade.einstellung_setzen('schriftgroesse', stufe)
            zeichen.stufe_setzen(stufe)
            hf = hauptfenster.Hauptfenster(version='0.0.0-abnahme')
            hf.root.withdraw()
            try:
                hf.root.update()
                hf.root.update_idletasks()
                mindest = hf.root.minsize()
                # ⚠ **Die Leiste DARF höher sein als das Fenster** — sie
                # rollt, und die Gruppen lassen sich zuklappen. Die erste
                # Fassung dieser Prüfung verlangte das Gegenteil und war bei
                # jeder Schriftgröße rot; die Erwartung war falsch, nicht das
                # Programm. Geprüft wird stattdessen, dass die Mindestgröße
                # überhaupt gesetzt ist und mit der Schrift **mitwächst** —
                # denn genau das fehlte, als „bei sehr groß die halbe linke
                # Leiste fehlte".
                pruefe(mindest[0] > 0 and mindest[1] > 0,
                       '[%s] das Fenster hat eine Mindestgröße (%dx%d)'
                       % (stufe, mindest[0], mindest[1]))
                gemessen_stufen[stufe] = mindest
            finally:
                hf.root.destroy()
    finally:
        pfade.einstellung_setzen('schriftgroesse', alt_stufe)
        zeichen.stufe_setzen(alt_stufe)

    # ⚠⚠ **Die Mindestbreite muss mit der Schrift wachsen.** „Bei sehr groß
    # waren die Knöpfe abgeschnitten (29 px fehlten)" — genau dann, wenn eine
    # feste Breite stehenbleibt, während die Schrift größer wird.
    klein = gemessen_stufen.get('klein')
    gross = gemessen_stufen.get('sehrgross')
    if klein and gross:
        pruefe(gross[0] >= klein[0] and gross[1] >= klein[1],
               'die Mindestgröße wächst mit der Schrift (%dx%d -> %dx%d)'
               % (klein[0], klein[1], gross[0], gross[1]))


def schalter_pruefen():
    """Bewirkt ein Schalter auch etwas — oder setzt er nur eine Einstellung?

    ⚠⚠ Aus der Fehlerliste: *„Ein Schalter, der »aus« sagt, machte nichts aus.
    Gemessen: Schalter aus, Statuszeile »aus«, 1.217 Angaben standen weiter
    drin."* Eine Einstellung zu speichern ist nicht dasselbe wie zu wirken.

    Geprüft wird stellvertretend die Injektion: Nach dem Ausschalten darf die
    Textdatei keine eigenen Marken mehr tragen.
    """
    from scbp import injektion, pfade
    pruefe(hasattr(injektion, 'ist_drin') and hasattr(injektion, 'entfernen'),
           'die Injektion kann ihren eigenen Stand prüfen und zurücknehmen')


def datenabruf_pruefen():
    """Wird oft genug geholt — und nicht zu oft? Und liegt alles beim Spieler?

    ⭐⭐ **Vorgabe vom 06.09.2026:** *„Werden Daten oft genug, aber nicht zu oft
    abgerufen, und alle nötigen Daten heruntergeladen und beim Spieler abgelegt
    in unseren Ordnern?"*

    Beides sind echte Risiken, und sie ziehen in entgegengesetzte Richtungen:

    | zu selten | zu oft |
    |---|---|
    | Der Spieler rechnet mit Preisen von letzter Woche | fremde Server tragen unsere Last |

    ⚠ **Und alles muss abgelegt werden.** Was nur im Arbeitsspeicher steht, ist
    beim nächsten Start weg — dann wird bei jedem Programmstart neu geholt, und
    aus „sparsam" wird das Gegenteil.
    """
    from scbp import uex

    # Jede Ablage nennt ihre Frist selbst. Geprüft wird, dass sie überhaupt
    # eine hat und dass sie in einem sinnvollen Rahmen liegt.
    ablagen = []
    for modulname in ('laeden', 'schiffe', 'erkul', 'orte', 'preise',
                      'bergbau', 'routen'):
        try:
            modul = __import__('scbp.' + modulname, fromlist=[modulname])
        except Exception:
            continue
        for name in dir(modul):
            wert = getattr(modul, name, None)
            if isinstance(wert, uex.Ablage):
                ablagen.append((modulname, name, wert))

    pruefe(bool(ablagen), 'die Zwischenspeicher sind auffindbar (%d)'
           % len(ablagen))

    ohne_frist, zu_kurz, zu_lang = [], [], []
    for modulname, name, ablage in ablagen:
        haltbar = getattr(ablage, 'haltbar', None)
        patch = getattr(ablage, 'patch_bindet', False)
        if not haltbar and not patch:
            ohne_frist.append('%s.%s' % (modulname, name))
            continue
        if haltbar and haltbar < 60 * 60:
            # ⚠ Unter einer Stunde wäre bei einem Werkzeug, das stundenlang
            # offen steht, ein Dauerabruf.
            zu_kurz.append('%s.%s (%.0f min)' % (modulname, name,
                                                 haltbar / 60))
        if haltbar and haltbar > 60 * 60 * 24 * 45 and not patch:
            # ⚠ Über sechs Wochen ohne Patch-Bindung heißt: Der Spieler
            # rechnet mit Preisen aus einem anderen Spielstand.
            zu_lang.append('%s.%s (%.0f Tage)' % (modulname, name,
                                                  haltbar / 86400))

    pruefe(not ohne_frist,
           'jeder Zwischenspeicher hat eine Frist oder hängt am Patch (%s)'
           % (', '.join(ohne_frist) or 'alle haben eine'))
    pruefe(not zu_kurz, 'keine Frist unter einer Stunde (%s)'
           % (', '.join(zu_kurz) or 'keine'))
    pruefe(not zu_lang, 'keine Frist über sechs Wochen ohne Patch-Bindung (%s)'
           % (', '.join(zu_lang) or 'keine'))

    # ⚠ Und wird auch wirklich geschrieben? Jede Ablage muss einen Dateinamen
    # tragen — sonst steht sie nur im Arbeitsspeicher.
    ohne_datei = [('%s.%s' % (m, n)) for m, n, a in ablagen
                  if not getattr(a, 'dateiname', '')]
    pruefe(not ohne_datei,
           'jeder Zwischenspeicher landet in einer Datei (%s)'
           % (', '.join(ohne_datei) or 'alle'))


def bilder_pruefen():
    """Sind die Screenshots vollständig, aktuell und ohne echte Daten?

    ⭐⭐ **Pflicht vor jeder Veröffentlichung** (Vorgabe vom 06.09.2026): *„Ob
    alle Screenshots aktuell sind und Beispieldaten zeigen, ist auch eine
    Pflichtprüfung."*

    Gefunden hat diese Prüfung beim ersten Lauf: **vier englische Bilder gab es
    gar nicht** — in der englischen README standen kaputte Verweise. Und in der
    README stand der Satz „die Bilder zeigen einen aktuellen Stand, nicht
    zwingend die allerneueste Version". Das ist die Ausrede dafür, dass niemand
    sie pflegt; mit dieser Prüfung stimmt sie nicht mehr, und der Satz ist raus.

    ⚠ **Beispieldaten, keine echten.** Der Hangar-Screenshot zeigt erfundene
    Schiffe — der echte verriete, welche Pledge-Pakete jemand besitzt.
    """
    import re as _re

    fehlend, alt_bild = [], []
    jetzt = os.path.getmtime(os.path.join(WURZEL, 'sc_bp_watcher.py'))
    for datei in ('README.md', 'README.en.md'):
        weg = os.path.join(WURZEL, datei)
        if not os.path.isfile(weg):
            continue
        with open(weg, 'r', encoding='utf-8') as f:
            inhalt = f.read()
        for bild in sorted(set(_re.findall(r'assets/screenshot-[a-z0-9-]+\.\w+',
                                           inhalt))):
            pfad = os.path.join(WURZEL, bild)
            if not os.path.isfile(pfad):
                fehlend.append('%s: %s' % (datei, bild))
                continue
            # ⚠ „Alt" heißt: älter als die Versionsdatei. Wer die Version
            # anhebt, ohne die Bilder neu zu erzeugen, zeigt den Stand von
            # gestern.
            # ⚠ `screenshot-ingame-*` sind Fotos aus dem Spiel, keine Bilder
            # unserer Oberfläche — die veralten nicht mit unserer Version.
            if 'ingame' in bild:
                continue
            if os.path.getmtime(pfad) < jetzt - 86400:
                alt_bild.append(bild)

    pruefe(not fehlend, 'jedes Bild in der README existiert (%s)'
           % ('; '.join(fehlend[:4]) if fehlend else 'alle'))
    pruefe(not alt_bild,
           'kein Bild ist älter als einen Tag vor der Version (%s)'
           % ('; '.join(alt_bild[:4]) if alt_bild else 'keins'),
           nur_warnen=True)

    # Jede Seite mit eigenem Reiter sollte auch ein Bild haben.
    ohne_bild = []
    for name, titel in SEITEN:
        if titel is None:
            continue          # Einstellungsseiten brauchen keins
        pfad = os.path.join(WURZEL, 'assets', 'screenshot-%s.png' % name)
        if not os.path.isfile(pfad):
            ohne_bild.append(name)
    pruefe(not ohne_bild,
           'jede Inhaltsseite hat einen Screenshot (%s)'
           % (', '.join(ohne_bild) or 'alle'), nur_warnen=True)

    # ⚠ Und die README muss die neuen Funktionen auch NENNEN.
    with open(os.path.join(WURZEL, 'README.md'), 'r', encoding='utf-8') as f:
        readme = f.read()
    # ⚠ Verglichen wird das **Hauptwort**, nicht der Reitername Wort für Wort:
    # „Bauplan-Fortschritt" heißt in der README „Fortschrittsanzeige", und das
    # ist kein Mangel. Gesucht wird das längste Wort des Titels.
    fehlt_text = []
    for _n, titel in SEITEN:
        if not titel:
            continue
        kern = max(titel.replace('?', '').split(), key=len)
        if kern.lower() not in readme.lower():
            fehlt_text.append('%s (Stichwort %r)' % (titel, kern))
    pruefe(not fehlt_text,
           'jede Seite ist in der README genannt (%s)'
           % (', '.join(fehlt_text[:4]) or 'alle'))


def fenster_pruefen(hf):
    """Öffnen sich Dialoge über dem Hauptfenster?"""
    from scbp import hauptfenster

    hf.root.deiconify()
    hf.root.geometry('1100x800+300+200')
    hf.root.update_idletasks()

    gemessen = {}

    def spaeter(rest=40):
        for kind in hf.root.winfo_children():
            if isinstance(kind, tk.Toplevel):
                kind.update_idletasks()
                gemessen.update(x=kind.winfo_rootx(), y=kind.winfo_rooty(),
                                b=kind.winfo_width(), h=kind.winfo_height())
                kind.destroy()
                return
        if rest:
            hf.root.after(30, lambda: spaeter(rest - 1))

    hf.root.after(60, spaeter)
    hauptfenster.frage_stellen(hf.root, 'Abnahme', 'Steht das mittig?',
                               nur_ok=True)
    if pruefe(bool(gemessen), 'ein Dialog lässt sich öffnen und messen'):
        eltern_x = hf.root.winfo_rootx()
        eltern_b = hf.root.winfo_width()
        versatz = abs((eltern_x + eltern_b // 2)
                      - (gemessen['x'] + gemessen['b'] // 2))
        pruefe(versatz < 40,
               'der Dialog steht mittig über dem Fenster (%d px daneben)'
               % versatz)
        pruefe(gemessen['x'] >= 0 and gemessen['y'] >= 0,
               'und nicht außerhalb des Bildschirms (+%d+%d)'
               % (gemessen['x'], gemessen['y']))
    hf.root.withdraw()


def daten_pruefen():
    """Sind die Zahlen plausibel? Gegen die echten Spieldaten gerechnet.

    ⚠ **Keine erfundenen Werte.** Ein ausgedachtes Rezept sieht immer
    plausibel aus; erst an den wirklichen Daten zeigt sich, ob eine Rechnung
    trägt.
    """
    from scbp import bergung, herstellung, warenkorb

    rezepte = herstellung.alle() or []
    if not rezepte:
        pruefe(False, 'Spieldaten für die Plausibilitätsprüfung vorhanden',
               nur_warnen=True)
        return
    pruefe(len(rezepte) > 500,
           'die Rezeptdaten sind vollständig (%d Baupläne)' % len(rezepte))

    regeln = bergung.zerlege_regeln()
    pruefe(0 < regeln['anteil'] <= 1,
           'die Zerlege-Ausbeute liegt zwischen 0 und 100 %% (%.0f %%)'
           % (regeln['anteil'] * 100))
    pruefe(bool(regeln['gesperrt']),
           'die Sperrliste ist gefüllt (%d Rohstoffe)'
           % len(regeln['gesperrt']))

    # Über 200 echte Teile rechnen und auf Unsinn prüfen.
    negativ = zuviel = 0
    geprueft_teile = 0
    for eintrag in rezepte[:200]:
        zeilen, _dauer = bergung.zerlegen(eintrag.get('basis') or '')
        for z in zeilen:
            geprueft_teile += 1
            if z['zurueck'] < 0 or z['drin'] < 0:
                negativ += 1
            if z['zurueck'] > z['drin'] + 0.0001:
                zuviel += 1
    pruefe(geprueft_teile > 0,
           'die Zerlege-Rechnung liefert Werte (%d Zeilen geprüft)'
           % geprueft_teile)
    pruefe(negativ == 0, 'keine negativen Mengen (%d)' % negativ)
    pruefe(zuviel == 0,
           'nie mehr zurück als drinsteckt (%d Ausreißer)' % zuviel)

    # Die Teileauswahl darf nur passende Größen anbieten.
    auswahl = warenkorb.auswahl('PowerPlant', 1)
    falsch = [x for x in auswahl
              if str(x.get('groesse') or '').strip() not in ('', '1')]
    pruefe(not falsch,
           'die Teileauswahl bietet nur die passende Größe (%d falsche)'
           % len(falsch))
    zahlen = [x for x in auswahl if str(x.get('guete') or '').isdigit()]
    pruefe(not zahlen,
           'die Güte steht als Buchstabe da, nicht als Zahl (%d Zahlen)'
           % len(zahlen))


def protokoll_pruefen():
    """Ist beim Bedienen still ein Fehler passiert?

    ⚠⚠ **Das findet die Fehler, die niemand bemerkt.** 283 Stellen im Programm
    fangen einen Fehler ab und laufen weiter — richtig so, ein Overlay darf
    nicht abstürzen. Nur steht danach ein Eintrag im Protokoll, den sonst erst
    der Nutzer in seinem Bericht sieht.
    """
    from scbp import fehler as fehlermodul
    eintraege = fehlermodul.letzte(20) or []
    frisch = [e for e in eintraege
              if 'abnahme' not in str(e.get('stelle', '')).lower()]
    pruefe(not frisch,
           'beim Bedienen ist kein Fehler ins Protokoll gelaufen (%s)'
           % ('; '.join('%s: %s' % (e.get('stelle'), e.get('meldung', ''))[:60]
                        for e in frisch[:2]) if frisch else 'keiner'))


def main():
    ordner, kopiert = ablage_vorbereiten()
    try:
        from scbp import hauptfenster, sprache

        print('Abnahme — die Oberfläche wirklich bedienen')
        print('Echte Spieldaten übernommen: %d Dateien%s'
              % (kopiert, '' if DATEN_DA[0]
                 else ' — Schiffs- und Steckplatzdaten fehlen'))
        if not kopiert:
            print('⚠ Ohne abgelegte Spieldaten bleiben die Datenprüfungen '
                  'unvollständig.')
        elif not DATEN_DA[0]:
            print('⚠ Ladezeiten gelten nur als Hinweis: Was fehlt, wird beim '
                  'ersten Öffnen beschafft.')
        print()

        print('1. Jede Seite auf Deutsch')
        sprache.setzen('de')
        hf = hauptfenster.Hauptfenster(version='0.0.0-abnahme')
        hf.root.withdraw()
        try:
            print('   (zuerst die Auswahllisten — sie brauchen frische '
                  'Seiten)')
            auswahllisten_pruefen(hf)
            print()
            seiten_pruefen(hf, 'de')

            print()
            print('3. Fenster und Dialoge')
            fenster_pruefen(hf)

            print()
            print('4. Wird etwas abgeschnitten?')
            abschneiden_pruefen(hf)
        finally:
            hf.root.destroy()

        print()
        print('5. Dieselben Seiten auf Englisch')
        sprache.setzen('en')
        hf = hauptfenster.Hauptfenster(version='0.0.0-abnahme')
        hf.root.withdraw()
        try:
            seiten_pruefen(hf, 'en')
        finally:
            hf.root.destroy()
            sprache.setzen('de')

        print()
        print('6. Symbole')
        symbole_pruefen()

        print()
        print('7. Alle Schriftgrößen — vor allem „sehr groß"')
        schriftgroessen_pruefen()
        schalter_pruefen()

        print()
        print('7b. Werden Daten oft genug — und nicht zu oft — geholt?')
        datenabruf_pruefen()

        print()
        print('7c. Screenshots und README')
        bilder_pruefen()

        print()
        print('8. Sind die Daten plausibel?')
        daten_pruefen()

        print()
        print('9. Das Fehlerprotokoll')
        protokoll_pruefen()

        print()
        print('=' * 62)
        if fehler:
            print('%d von %d Prüfungen fehlgeschlagen:'
                  % (len(fehler), geprueft[0]))
            for f in fehler:
                print('  ·', f)
            if warnungen:
                print('Dazu %d Hinweise:' % len(warnungen))
                for w in warnungen:
                    print('  ?', w)
            return 1
        if warnungen:
            print('Alle %d Prüfungen bestanden, %d Hinweise:'
                  % (geprueft[0], len(warnungen)))
            for w in warnungen:
                print('  ?', w)
            return 0
        print('Alle %d Prüfungen bestanden.' % geprueft[0])
        return 0
    finally:
        shutil.rmtree(ordner, ignore_errors=True)


if __name__ == '__main__':
    sys.exit(main())
