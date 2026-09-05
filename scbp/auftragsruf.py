# SPDX-License-Identifier: GPL-3.0-only
#
# SC BP Watcher — welcher Auftrag welchen Ruf bringt
# Copyright (C) 2026 Xharig
#
# This program is free software: you can redistribute it and/or modify it under
# the terms of the GNU General Public License version 3 as published by the
# Free Software Foundation.
#
# This program is distributed in the hope that it will be useful, but WITHOUT
# ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
# FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
# details.
#
# You should have received a copy of the GNU General Public License along with
# this program.  If not, see <https://www.gnu.org/licenses/>.
"""
Wem ein Auftrag Ruf gutschreibt — und welcher Art.

## ⚠⚠ Warum das eine eigene Quelle braucht

Die Vertragsdaten, aus denen die Injektion sonst schoepft, kennen die
Rufpunkte als blosse Zahl: „# Zu erwartende Rufpunkte: 150 XP". Bei WEM sie
anfallen und ob es **Standing**, **Affinity** oder **Bounty Hunting** ist,
steht dort nicht — gemessen am 05.09.2026 an allen 818 Eintraegen: kein
einziges Feld dafuer.

Gewuenscht wurde genau das: „auf SCMDB sieht man auch ob es Standing oder Rep
bekommt, das muss auf jeden fall mit in den Questtext." Ein Auftrag kann
dabei **mehreren** Parteien Ruf bringen (Headhunters und Citizens For
Prosperity im selben Auftrag) — deshalb je Auftrag eine Liste, kein Einzelwert.

## Die Kette

    contract.factionRewardsIndex
      -> factionRewardsPools[i]   ->  {factionGuid, scopeGuid, amount}
           -> factions[guid].name        = „Headhunters"
           -> scopes[guid].displayName   = „Standing"

Daraus wird die Zeile `Headhunters: +50 Standing`.

## ⚠ Gespeichert wird nur das Ergebnis

Die Quelldatei ist **12,5 MB**; aufbereitet bleiben rund 1.300 Zeilen. Beim
Spieler liegt nur die kleine Fassung — dieselbe Regel wie beim
Gegenstands-Zwischenspeicher.

## ⚠ An den Spielstand gebunden, nicht an die Uhr

Nach einem Patch aendern sich Auftraege und Rufhoehen. Der Zwischenspeicher
traegt die Spielversion, mit der er geholt wurde; passt sie nicht mehr, wird
neu geladen statt auf einen Zeitablauf zu warten.
"""
import json
import os
import re

from . import fehler, pfade

DATEI = 'auftragsruf.json'
FORMAT = 1

BASIS = 'https://scmdb.net/data'
ZEITGRENZE = 30

# ⚠ Wer den Netzzugriff abschaltet, meint auch diesen. Der Selbsttest haelt
# fest, dass JEDES Modul mit Netzabruf den Schalter kennt — und hat dieses
# hier beim ersten Lauf prompt erwischt.
#
# ⚠ Abgeschaltet wird das **Holen**, nicht das Wissen: Eine bereits geladene
# Tabelle bleibt nutzbar. Sonst verlöre man mit dem Netz auch das, was längst
# auf der Platte liegt.
AUS = os.environ.get('SC_BP_NO_NET', '') not in ('', '0')

# Der Auftragsschluessel steht bei scmdb mit fuehrendem `@` und in anderer
# Gross-/Kleinschreibung als in den Vertragsdaten:
#
#     unser:  shubin_industrial_shipmining_nyx_m_solar_title_001
#     scmdb: @Shubin_Industrial_ShipMining_Nyx_M_Solar_Title_001
#
# ⚠ Gemessen: Ohne diese Angleichung gibt es **null** Treffer, mit ihr passen
# die Listen zusammen. Der Vergleich laeuft deshalb immer ueber `_schluessel`.
def _schluessel(roh):
    return (roh or '').lstrip('@').lower()


def pfad():
    return pfade.app_datei(DATEI)


def laden():
    """Die aufbereitete Tabelle — `{'version':…, 'auftraege': {…}}`."""
    try:
        with open(pfad(), encoding='utf-8') as f:
            daten = json.load(f)
        if (isinstance(daten, dict) and daten.get('format') == FORMAT
                and isinstance(daten.get('auftraege'), dict)):
            return daten
    except (OSError, ValueError):
        pass
    except Exception as ausnahme:
        fehler.merken('auftragsruf.laden', ausnahme)
    return {'format': FORMAT, 'version': '', 'auftraege': {}}


def sichern(daten):
    try:
        daten['format'] = FORMAT
        ziel = pfad()
        ordner = os.path.dirname(ziel)
        if ordner and not os.path.isdir(ordner):
            os.makedirs(ordner)
        vorlaeufig = ziel + '.neu'
        with open(vorlaeufig, 'w', encoding='utf-8') as f:
            json.dump(daten, f, ensure_ascii=False)
        os.replace(vorlaeufig, ziel)
        return True
    except Exception as ausnahme:
        fehler.merken('auftragsruf.sichern', ausnahme)
        return False


def _holen(adresse):
    import urllib.request
    anfrage = urllib.request.Request(
        adresse, headers={'User-Agent': 'SC-BP-Watcher'})
    with urllib.request.urlopen(anfrage, timeout=ZEITGRENZE) as antwort:
        return json.loads(antwort.read().decode('utf-8'))


def aufbereiten(roh):
    """Aus der 12,5-MB-Datei die Tabelle bauen, die wir brauchen.

    Gibt `{schluessel: [{'wer':…, 'was':…, 'wieviel':…}, …]}` zurueck.

    ⚠ Ein Auftrag kann MEHREREN Parteien Ruf bringen — deshalb eine Liste.
    Genau das war der Anlass: „headhunters ist ne gute quelle da gibt es
    beides, oder citizen for prosperity."
    """
    contracts = roh.get('contracts') or []
    pools = roh.get('factionRewardsPools') or []
    factions = roh.get('factions') or {}
    scopes = roh.get('scopes') or {}

    raus = {}
    for c in contracts:
        schluessel = _schluessel(c.get('titleLocKey') or c.get('titleKey'))
        index = c.get('factionRewardsIndex')
        if not schluessel or not isinstance(index, int):
            continue
        if index < 0 or index >= len(pools):
            continue
        eintraege = []
        for teil in (pools[index] or []):
            if not isinstance(teil, dict):
                continue
            menge = teil.get('amount')
            if not menge:
                continue
            fraktion = (factions.get(teil.get('factionGuid')) or {}).get('name')
            art = (scopes.get(teil.get('scopeGuid')) or {}).get('displayName')
            if not fraktion and not art:
                continue
            eintraege.append({'wer': fraktion or '', 'was': art or '',
                              'wieviel': menge})
        if eintraege:
            raus[schluessel] = eintraege
    return raus


def auffrischen(spielversion=''):
    """Die Tabelle holen, wenn sie fehlt oder zum Patch nicht mehr passt.

    Gibt die Zahl der Auftraege zurueck. Bei Netzfehlern bleibt der alte
    Stand stehen — eine veraltete Angabe ist besser als keine.
    """
    alt = laden()
    if alt['auftraege'] and (not spielversion
                             or alt.get('version') == spielversion):
        return len(alt['auftraege'])
    if AUS:
        return len(alt['auftraege'])

    try:
        versionen = _holen(BASIS + '/versions.json')
        datei = None
        # ⚠ Die zum Spielstand passende Fassung, nicht blind die erste: Die
        # Liste beginnt mit der PTU, und wer auf LIVE spielt, bekaeme sonst
        # Auftragsdaten einer Version, die er gar nicht hat.
        for eintrag in (versionen or []):
            if spielversion and eintrag.get('version') == spielversion:
                datei = eintrag.get('file')
                break
        if not datei:
            for eintrag in (versionen or []):
                if 'live' in (eintrag.get('version') or ''):
                    datei = eintrag.get('file')
                    break
        if not datei and versionen:
            datei = versionen[0].get('file')
        if not datei:
            return len(alt['auftraege'])

        roh = _holen('%s/%s' % (BASIS, datei))
        auftraege = aufbereiten(roh)
        if not auftraege:
            return len(alt['auftraege'])
        sichern({'format': FORMAT,
                 'version': roh.get('version') or spielversion or '',
                 'auftraege': auftraege})
        return len(auftraege)
    except Exception as ausnahme:
        fehler.merken('auftragsruf.auffrischen', ausnahme)
        return len(alt['auftraege'])


def zu(schluessel, daten=None):
    """Die Rufeintraege eines Auftrags — leere Liste, wenn nichts bekannt."""
    daten = daten if daten is not None else laden()
    return daten['auftraege'].get(_schluessel(schluessel)) or []


def zeile(schluessel, wort='Ruf', daten=None):
    """Eine fertige Zeile fuer den Auftragstext — oder `''`.

    Sieht so aus: `# Ruf: Headhunters +50 Standing`, bei mehreren Parteien
    durch Komma getrennt.

    ⚠ Ohne Fraktionsnamen wird die Art allein genannt statt „ +50" ins Leere
    zu schreiben; ohne Art umgekehrt. Eine halbe Angabe ist immer noch eine
    Auskunft, eine erfundene waere keine.
    """
    teile = []
    for e in zu(schluessel, daten):
        menge = e.get('wieviel')
        wer, was = (e.get('wer') or '').strip(), (e.get('was') or '').strip()
        if wer and was:
            teile.append('%s +%s %s' % (wer, menge, was))
        elif wer:
            teile.append('%s +%s' % (wer, menge))
        elif was:
            teile.append('+%s %s' % (menge, was))
    if not teile:
        return ''
    return '# %s: %s' % (wort, ', '.join(teile))
