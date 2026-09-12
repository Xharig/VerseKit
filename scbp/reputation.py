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

CACHE_FILE = 'auftragsruf.json'
FORMAT = 1

# ⚠⚠ **Geholt wird vom GitHub-Spiegel, nicht von scmdb.net.** Krovax hat ihn
# eigens für Programme angelegt und dazu gesagt: „Ich habs gemirrored, keine
# Lust was bei CF falsch einzustellen und dann passieren unerwartete Dinge."
# Wer eine fremde Quelle benutzt, benutzt den Weg, den ihr Betreiber dafür
# vorgesehen hat — sonst hängt man an einer Einstellung, die jederzeit anders
# gemeint sein kann.
BASE = ('https://raw.githubusercontent.com/KrovaxCode/SCMDB_DATA/main/data')
# ⚠ Beim Spiegel heißt die Übersicht `game-versions.json`, auf der Webseite
# `versions.json`. Inhalt und Aufbau sind gleich.
VERSION_FILE = 'game-versions.json'

# ⚠ **Nur als Rückfall.** Krovax hat die Nutzung seiner Webseite ausdrücklich
# erlaubt, für den Fall, dass am Spiegel etwas fehlt. Der Spiegel bleibt
# trotzdem der erste Weg: Er ist der, den er dafür gebaut hat.
FALLBACK = 'https://scmdb.net/data'
FALLBACK_VERSION_FILE = 'versions.json'

TIME_LIMIT = 30

# ⚠ Wer den Netzzugriff abschaltet, meint auch diesen. Der Selbsttest haelt
# fest, dass JEDES Modul mit Netzabruf den Schalter kennt — und hat dieses
# hier beim ersten Lauf prompt erwischt.
#
# ⚠ Abgeschaltet wird das **Holen**, nicht das Wissen: Eine bereits geladene
# Tabelle bleibt nutzbar. Sonst verlöre man mit dem Netz auch das, was längst
# auf der Platte liegt.
OFF = os.environ.get('SC_BP_NO_NET', '') not in ('', '0')

# Der Auftragsschluessel steht bei scmdb mit fuehrendem `@` und in anderer
# Gross-/Kleinschreibung als in den Vertragsdaten:
#
#     unser:  shubin_industrial_shipmining_nyx_m_solar_title_001
#     scmdb: @Shubin_Industrial_ShipMining_Nyx_M_Solar_Title_001
#
# ⚠ Gemessen: Ohne diese Angleichung gibt es **null** Treffer, mit ihr passen
# die Listen zusammen. Der Vergleich laeuft deshalb immer ueber `_schluessel`.
def _key(raw):
    return (raw or '').lstrip('@').lower()


def path():
    return pfade.app_datei(CACHE_FILE)


def load():
    """Die aufbereitete Tabelle — `{'version':…, 'auftraege': {…}}`."""
    try:
        with open(path(), encoding='utf-8') as f:
            data = json.load(f)
        if (isinstance(data, dict) and data.get('format') == FORMAT
                and isinstance(data.get('auftraege'), dict)):
            return data
    except (OSError, ValueError):
        pass
    except Exception as error:
        fehler.merken('reputation.load', error)
    return {'format': FORMAT, 'version': '', 'auftraege': {}}


def save(data):
    try:
        data['format'] = FORMAT
        target = path()
        folder = os.path.dirname(target)
        if folder and not os.path.isdir(folder):
            os.makedirs(folder)
        tentative = target + '.neu'
        with open(tentative, 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False)
        os.replace(tentative, target)
        return True
    except Exception as error:
        fehler.merken('reputation.save', error)
        return False


def _fetch(address):
    import urllib.request
    request = urllib.request.Request(
        address, headers={'User-Agent': 'SC-BP-Watcher'})
    with urllib.request.urlopen(request, timeout=TIME_LIMIT) as reply:
        return json.loads(reply.read().decode('utf-8'))


def prepare(raw):
    """Aus der 12,5-MB-Datei die Tabelle bauen, die wir brauchen.

    Gibt `{schluessel: [{'wer':…, 'was':…, 'wieviel':…}, …]}` zurueck.

    ⚠ Ein Auftrag kann MEHREREN Parteien Ruf bringen — deshalb eine Liste.
    Genau das war der Anlass: „headhunters ist ne gute quelle da gibt es
    beides, oder citizen for prosperity."
    """
    contracts = raw.get('contracts') or []
    pools = raw.get('factionRewardsPools') or []
    factions = raw.get('factions') or {}
    scopes = raw.get('scopes') or {}

    out = {}
    for c in contracts:
        key = _key(c.get('titleLocKey') or c.get('titleKey'))
        idx = c.get('factionRewardsIndex')
        if not key or not isinstance(idx, int):
            continue
        if idx < 0 or idx >= len(pools):
            continue
        entries = []
        for part in (pools[idx] or []):
            if not isinstance(part, dict):
                continue
            amount_ = part.get('amount')
            if not amount_:
                continue
            faction = (factions.get(part.get('factionGuid')) or {}).get('name')
            kind = (scopes.get(part.get('scopeGuid')) or {}).get('displayName')
            if not faction and not kind:
                continue
            entries.append({'wer': faction or '', 'was': kind or '',
                              'wieviel': amount_})
        if entries:
            out[key] = entries
    return out


def refresh(game_version=''):
    """Die Tabelle holen, wenn sie fehlt oder zum Patch nicht mehr passt.

    Gibt die Zahl der Auftraege zurueck. Bei Netzfehlern bleibt der alte
    Stand stehen — eine veraltete Angabe ist besser als keine.
    """
    old = load()
    if old['auftraege'] and (not game_version
                             or old.get('version') == game_version):
        return len(old['auftraege'])
    if OFF:
        return len(old['auftraege'])

    try:
        # ⚠ Spiegel zuerst, Webseite nur wenn dort etwas fehlt. Beide Wege
        # sind erlaubt — der Spiegel ist der vorgesehene.
        base_url = BASE
        try:
            versions_ = _fetch('%s/%s' % (BASE, VERSION_FILE))
        except Exception:
            base_url = FALLBACK
            versions_ = _fetch('%s/%s' % (FALLBACK,
                                          FALLBACK_VERSION_FILE))
        file_name = None
        # ⚠ Die zum Spielstand passende Fassung, nicht blind die erste: Die
        # Liste beginnt mit der PTU, und wer auf LIVE spielt, bekaeme sonst
        # Auftragsdaten einer Version, die er gar nicht hat.
        for entry in (versions_ or []):
            if game_version and entry.get('version') == game_version:
                file_name = entry.get('file')
                break
        if not file_name:
            for entry in (versions_ or []):
                if 'live' in (entry.get('version') or ''):
                    file_name = entry.get('file')
                    break
        if not file_name and versions_:
            file_name = versions_[0].get('file')
        if not file_name:
            return len(old['auftraege'])

        try:
            raw = _fetch('%s/%s' % (base_url, file_name))
        except Exception:
            # Die Übersicht kam durch, die Datei selbst nicht — dann den
            # anderen Weg versuchen, statt ganz aufzugeben.
            other = FALLBACK if base_url == BASE else BASE
            raw = _fetch('%s/%s' % (other, file_name))
        contract_map = prepare(raw)
        if not contract_map:
            return len(old['auftraege'])
        save({'format': FORMAT,
                 'version': raw.get('version') or game_version or '',
                 'auftraege': contract_map})
        return len(contract_map)
    except Exception as error:
        fehler.merken('reputation.refresh', error)
        return len(old['auftraege'])


def entries_for(key, data=None):
    """Die Rufeintraege eines Auftrags — leere Liste, wenn nichts bekannt."""
    data = data if data is not None else load()
    return data['auftraege'].get(_key(key)) or []


def line(key, word='Ruf', data=None):
    """Eine fertige Zeile fuer den Auftragstext — oder `''`.

    Sieht so aus: `# Ruf: Headhunters +50 Standing`, bei mehreren Parteien
    durch Komma getrennt.

    ⚠ Ohne Fraktionsnamen wird die Art allein genannt statt „ +50" ins Leere
    zu schreiben; ohne Art umgekehrt. Eine halbe Angabe ist immer noch eine
    Auskunft, eine erfundene waere keine.
    """
    parts = []
    for e in entries_for(key, data):
        amount_ = e.get('wieviel')
        who, what = (e.get('wer') or '').strip(), (e.get('was') or '').strip()
        if who and what:
            parts.append('%s +%s %s' % (who, amount_, what))
        elif who:
            parts.append('%s +%s' % (who, amount_))
        elif what:
            parts.append('+%s %s' % (amount_, what))
    if not parts:
        return ''
    return '# %s: %s' % (word, ', '.join(parts))
