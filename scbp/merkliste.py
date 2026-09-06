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
Die Merkliste — Baupläne, auf die man wartet.

Trägt man einen Bauplan hier ein, meldet der Watcher ihn **auffällig**, sobald
er auftaucht: gold statt grün, mit Stern. Danach fliegt er von selbst wieder
raus, denn eine Merkliste voller längst erledigter Wünsche ist keine.

Gepflegt wird sie **im Fenster mit einem Klick** — niemand soll dafür eine
Datei bearbeiten müssen. Die Datei (`watchlist.json`) bleibt trotzdem lesbar
und von Hand änderbar, denn ein eigenes Werkzeug des Autors schreibt dort Teile der
Ausrüstungsliste hinein.

Zwei Arten von Einträgen leben nebeneinander:

  **Namen** — was im Fenster angeklickt wurde. Genauer Abgleich.
  **Muster** — Teilstücke eines Namens, von außen eingetragen (der Skill kennt
  die endgültigen Namen künftiger Gegenstände ja noch nicht). Trifft ein Muster,
  gilt der Eintrag als erfüllt.

Format:

    {
      "namen": ["Attrition-5 Repeater"],
      "eintraege": [{"titel": "Helm meiner Wahl", "muster": ["adp-mk4", "woodland"]}]
    }
"""
import re
import json
import os

from . import pfade

DATEI = 'watchlist.json'


def _norm(s):
    """Vergleichsform eines Namens — siehe `pfade.namensform`."""
    return pfade.namensform(s)


def pfad():
    return pfade.app_datei(DATEI)


def laden():
    """Die Merkliste. Fehlt die Datei, ist sie leer — das ist kein Fehler."""
    try:
        with open(pfad(), encoding='utf-8') as f:
            d = json.load(f)
    except Exception:
        return {'namen': [], 'eintraege': []}
    if not isinstance(d, dict):
        return {'namen': [], 'eintraege': []}
    d.setdefault('namen', [])
    d.setdefault('eintraege', [])
    if not isinstance(d['namen'], list):
        d['namen'] = []
    if not isinstance(d['eintraege'], list):
        d['eintraege'] = []
    return d


def speichern(daten):
    """Schreibt über eine Nebendatei, damit ein Absturz nichts zerreißt."""
    ziel = pfad()
    temp = ziel + '.tmp'
    try:
        os.makedirs(os.path.dirname(ziel), exist_ok=True)
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump(daten, f, ensure_ascii=False, indent=1)
        os.replace(temp, ziel)
        return True
    except OSError as ausnahme:
        try:
            from . import fehler
            fehler.merken('merkliste.speichern', ausnahme)
        except Exception:
            pass
        try:
            os.remove(temp)
        except OSError:
            pass
        return False


# ---------------------------------------------------------------- Nach außen
def namen(daten=None):
    """Die angeklickten Namen in Vergleichsform."""
    return {_norm(n) for n in (daten or laden())['namen']}


def enthaelt(name, daten=None):
    return _norm(name) in namen(daten)


def hinzufuegen(name, daten=None):
    """Aufnehmen. Gibt die geänderten Daten zurück (noch nicht gespeichert)."""
    daten = daten or laden()
    if not enthaelt(name, daten):
        daten['namen'].append(name.strip())
    return daten


def entfernen(name, daten=None):
    """Herausnehmen — auch aus den Muster-Einträgen, falls einer greift."""
    daten = daten or laden()
    k = _norm(name)
    daten['namen'] = [n for n in daten['namen'] if _norm(n) != k]
    daten['eintraege'] = [e for e in daten['eintraege']
                          if not _muster_trifft(e, k)]
    return daten


def eintrag_entfernen(titel, daten=None):
    """Eine **eigene Beobachtung** herausnehmen — über ihren Titel.

    ⚠ Nicht dasselbe wie `entfernen()`. Das nimmt einen Bauplan-Namen heraus
    und wirft dabei jede Muster-Beobachtung mit weg, die auf ihn passt. Hier
    geht es um die Beobachtung selbst: „Helm meiner Wahl" abwählen,
    weil die Staffel ein anderes Teil nimmt — die Baupläne, die das Muster
    zufällig trifft, gehen niemanden etwas an.

    Gibt die geänderten Daten zurück (noch nicht gespeichert).
    """
    daten = daten or laden()
    gesucht = (titel or '').strip().lower()
    daten['eintraege'] = [e for e in daten['eintraege']
                          if (e.get('titel') or '').strip().lower() != gesucht]
    return daten


def umschalten(name):
    """Klick im Fenster: rein oder raus. Gibt zurück, ob er jetzt drin ist."""
    daten = laden()
    drin = enthaelt(name, daten)
    daten = entfernen(name, daten) if drin else hinzufuegen(name, daten)
    speichern(daten)
    return not drin


def _muster_trifft(eintrag, name_norm):
    """Passt eine Beobachtung auf diesen Namen?

    ⚠ **An Wortgrenzen, nicht mitten im Wort.** Ein blosses „steckt drin"
    liefert falsche Treffer, die niemand als solche erkennt: Das Muster
    `arden backpack` traf am 29.08.2026 auf *W**arden** Backpack Purgatory
    Camo* — und der Watcher meldete ein Rüstungsteil als verfügbar, das mit
    der gesuchten Ausrüstung nichts zu tun hat. Wer sich darauf verlässt,
    fliegt umsonst los.

    Vor und hinter dem Muster darf deshalb kein weiterer Buchstabe und keine
    Ziffer stehen. Bindestriche und Leerzeichen zählen als Grenze, damit
    `abc-mk4 legs grey` weiter passt.
    """
    muster = [str(m).lower().strip() for m in (eintrag.get('muster') or [])]
    for m in muster:
        if not m:
            continue
        if re.search(r'(?<![a-z0-9])%s(?![a-z0-9])' % re.escape(m), name_norm):
            return True
    return False


def treffer(name, daten=None):
    """Wird auf diesen Bauplan gewartet? Rückgabe: Titel des Eintrags oder None.

    Bei einem angeklickten Namen ist der Titel der Name selbst, bei einem
    Muster-Eintrag dessen Titel („Helm meiner Wahl")."""
    daten = daten or laden()
    k = _norm(name)
    for n in daten['namen']:
        if _norm(n) == k:
            return n
    for e in daten['eintraege']:
        if _muster_trifft(e, k):
            return e.get('titel') or name
    return None


def erledigen(name):
    """Einen erfüllten Wunsch austragen. Gibt den Titel zurück, wenn einer weg ist.

    Wird aufgerufen, sobald ein Bauplan im eigenen Bestand landet: Worauf man
    gewartet hat und was man jetzt hat, gehört nicht mehr auf die Liste."""
    daten = laden()
    titel = treffer(name, daten)
    if not titel:
        return None
    speichern(entfernen(name, daten))
    return titel


def aufraeumen(bestand_namen):
    """Merkposten austragen, die schon im Bestand stehen. Gibt die Anzahl zurück.

    ⚠⚠⚠ **`erledigen()` greift nur beim FUND.** Wer einen Bauplan merkt, den er
    längst hat — oder ihn zwischen zwei Programmstarts über eine andere Quelle
    bekommt —, behält den Merkposten für immer. Am 06.09.2026 stand
    `H4-PBF Ammo Carrier` unter „beobachtet", obwohl er in derselben Liste ein
    Häkchen trug: *„da wird einer beobachtet, den ich schon habe."*

    Eine Beobachtungsliste, auf der Erledigtes stehen bleibt, wird mit jeder
    Woche unbrauchbarer — genau wie eine Aufgabenliste ohne Abhaken.

    ⚠ Nur `namen` werden aufgeräumt, **nicht** die Muster-Einträge: Ein Muster
    wie „Morozov" steht für mehrere Teile, von denen erst eines da sein kann.
    """
    daten = laden()
    habe = {_norm(n) for n in (bestand_namen or ())}
    if not habe:
        return 0
    bleibt = [n for n in daten['namen'] if _norm(n) not in habe]
    weg = len(daten['namen']) - len(bleibt)
    if weg:
        daten['namen'] = bleibt
        speichern(daten)
    return weg


def alle(daten=None):
    """Alles, worauf gewartet wird — für die Anzeige. Namen zuerst."""
    daten = daten or laden()
    liste = [{'titel': n, 'art': 'name'} for n in sorted(daten['namen'])]
    liste += [{'titel': e.get('titel') or '?', 'art': 'muster',
               'muster': e.get('muster') or []}
              for e in daten['eintraege']]
    return liste


def anzahl(daten=None):
    daten = daten or laden()
    return len(daten['namen']) + len(daten['eintraege'])


if __name__ == '__main__':
    print('Datei:', pfad())
    for e in alle():
        print('  %-8s %s %s' % (e['art'], e['titel'], e.get('muster') or ''))
    print('Gesamt:', anzahl())
