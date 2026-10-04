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
Rufpunkte als blosse Zahl (`# Zu erwartende Rufpunkte: 150 XP`). Bei WEM sie
anfallen und ob es **Standing**, **Affinity** oder **Bounty Hunting** ist,
steht dort nicht — gemessen an allen 818 Eintraegen: kein einziges Feld
dafuer.

Genau das gehoert aber in den Auftragstext, wie scmdb es zeigt. Ein Auftrag kann
dabei **mehreren** Parteien Ruf bringen (Headhunters und Citizens For
Prosperity im selben Auftrag) — deshalb je Auftrag eine Liste, kein Einzelwert.

## Die Kette

    contract.factionRewardsIndex
      -> factionRewardsPools[i]   ->  {factionGuid, scopeGuid, amount}
           -> factions[guid].name        = "Headhunters"
           -> scopes[guid].displayName   = "Standing"

Daraus wird die Zeile `Headhunters: +50 Standing`.

## ⚠ Gespeichert wird nur das Ergebnis

Die Quelldatei ist **12,5 MB**; aufbereitet bleiben rund 1.300 Zeilen. Beim
Spieler liegt nur die kleine Fassung — dieselbe Regel wie beim
Gegenstands-Zwischenspeicher.

## Und wo der Auftrag spielt

Aus derselben Datei kommt die **Region**: `systems`, bei Pyro `pyroRegion`
(A–D) und die Orte je Auftrag (`locations` → `locationPools[…].planet`).
Daraus wird `# Region: Stanton (Crusader, microTech)`. ⚠ Eine **Gefahrenstufe** gibt es in diesen
Daten nicht — sie wird nicht erfunden.

## ⚠ An den Spielstand gebunden, nicht an die Uhr

Nach einem Patch aendern sich Auftraege und Rufhoehen. Der Zwischenspeicher
traegt die Spielversion, mit der er geholt wurde; passt sie nicht mehr, wird
neu geladen statt auf einen Zeitablauf zu warten.
"""
import json
import os

from . import errors, paths

CACHE_FILE = 'auftragsruf.json'
# ⚠ Ab 2 mit Regionen. Ein Zwischenspeicher im Aufbau 1
# gilt als leer und wird beim nächsten Eintragen neu geholt — sonst stünde bis
# zum nächsten Patch keine Region im Text.
FORMAT = 2

# So viele Planeten werden hinter einem System höchstens genannt; bei mehr
# steht nur das System — eine lange Aufzählung liest im Auftrag niemand.
MAX_PLANETS = 4

# ⚠⚠ **Geholt wird vom GitHub-Spiegel, nicht von scmdb.net.** Der Betreiber
# hat den Spiegel eigens für Programme angelegt, damit Abrufe nicht an den
# Einstellungen der Webseite (Cloudflare) hängen. Wer eine fremde Quelle
# benutzt, benutzt den Weg, den ihr Betreiber dafür vorgesehen hat.
BASE = ('https://raw.githubusercontent.com/KrovaxCode/SCMDB_DATA/main/data')
# ⚠ Beim Spiegel heißt die Übersicht `game-versions.json`, auf der Webseite
# `versions.json`. Inhalt und Aufbau sind gleich.
VERSION_FILE = 'game-versions.json'

# ⚠ **Nur als Rückfall**, falls am Spiegel etwas fehlt — die Nutzung der
# Webseite dafür ist vom Betreiber erlaubt. Der Spiegel bleibt der erste Weg.
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
    return paths.app_file(CACHE_FILE)


def load():
    """Die aufbereitete Tabelle — `{'version':…, 'auftraege': {…}}`."""
    try:
        with open(path(), encoding='utf-8') as f:
            data = json.load(f)
        if (isinstance(data, dict) and data.get('format') == FORMAT
                and isinstance(data.get('auftraege'), dict)):
            if not isinstance(data.get('regionen'), dict):
                data['regionen'] = {}
            return data
    except (OSError, ValueError):
        pass
    except Exception as error:
        errors.record('reputation.load', error)
    return {'format': FORMAT, 'version': '', 'auftraege': {}, 'regionen': {}}


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
        errors.record('reputation.save', error)
        return False


def _fetch(address):
    import urllib.request
    request = urllib.request.Request(
        address, headers={'User-Agent': 'SC-BP-Watcher'})
    with urllib.request.urlopen(request, timeout=TIME_LIMIT) as reply:
        return json.loads(reply.read().decode('utf-8'))


def prepare(raw, issues=None):
    """Aus der 12,5-MB-Datei die Tabelle bauen, die wir brauchen.

    Gibt `{schluessel: [{'wer':…, 'was':…, 'wieviel':…}, …]}` zurueck.

    ⚠ Ein Auftrag kann MEHREREN Parteien Ruf bringen — deshalb eine Liste.

    `issues` ist die Liste bekannter CIG-Datenfehler (`cig_issues`). Nennt sie
    für einen Vertrag (debugName) eine andere Fraktion, geht der Ruf, der in
    den Rohdaten an die falsche Fraktion fällt, an die gemeinte; der Eintrag
    trägt dann `datenfehler` mit dem alten Namen.
    """
    from . import cig_issues
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
        fix = cig_issues.faction_fix(issues, c.get('debugName'))
        wrong = ((c.get('factionGuid') or '').lower()
                 if fix and (c.get('factionGuid') or '').lower() != fix[0]
                 else None)
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
            row = {'wer': faction or '', 'was': kind or '', 'wieviel': amount_}
            if wrong and (part.get('factionGuid') or '').lower() == wrong:
                right = ((factions.get(fix[0]) or {}).get('name') or fix[1])
                if right:
                    row['wer'] = right
                    row['datenfehler'] = {'statt': faction or '',
                                          'note': fix[2]}
            entries.append(row)
        if entries:
            out[key] = entries
    return out


def prepare_regions(raw):
    """Wo ein Auftrag spielt — `{schluessel: 'Stanton (Crusader, microTech)'}`.

    Mehrere Einträge mit demselben Titelschlüssel (Varianten eines Auftrags)
    werden zusammengelegt. Je System die Planeten; bei Pyro zählt die
    Pyro-Region (A–D), weil `locations` dort oft leer sind. Mehr als
    `MAX_PLANETS` Planeten oder alle vier Pyro-Regionen: nur das System.
    """
    pools = raw.get('locationPools') or {}
    pyro = {}
    for letter, region in (raw.get('pyroRegions') or {}).items():
        pyro[letter] = [loc.get('name') for loc in (region or {}).get('locations') or []
                        if isinstance(loc, dict) and loc.get('navIcon') == 'Planet'
                        and loc.get('name')]
    # Alle Planeten je System — nennt ein Auftrag sie alle, heißt das „überall
    # im System", und dann steht nur das System da.
    everywhere = {}
    for pool in pools.values():
        if isinstance(pool, dict) and pool.get('planet') and pool.get('system'):
            everywhere.setdefault(pool['system'], set()).add(pool['planet'])
    for names in pyro.values():
        everywhere.setdefault('Pyro', set()).update(names)
    gathered = {}
    for c in raw.get('contracts') or []:
        key = _key(c.get('titleLocKey') or c.get('titleKey'))
        if not key:
            continue
        slot = gathered.setdefault(key, {'systems': set(), 'pyro': set(),
                                         'planets': {}})
        slot['systems'].update(s for s in (c.get('systems') or []) if s)
        slot['pyro'].update(r for r in (c.get('pyroRegion') or []) if r)
        for loc in c.get('locations') or []:
            pool = pools.get(loc) or {}
            if pool.get('planet') and pool.get('system'):
                slot['planets'].setdefault(pool['system'], set()).add(pool['planet'])
    out = {}
    for key, slot in gathered.items():
        parts = []
        for system in sorted(slot['systems']):
            planets = set(slot['planets'].get(system) or ())
            if system == 'Pyro' and slot['pyro'] and len(slot['pyro']) < len(pyro):
                for letter in slot['pyro']:
                    planets.update(pyro.get(letter) or ())
            elif system == 'Pyro' and len(slot['pyro']) >= len(pyro) > 0:
                planets = set()
            if planets >= (everywhere.get(system) or {None}):
                planets = set()
            if planets and len(planets) <= MAX_PLANETS:
                parts.append('%s (%s)' % (system, ', '.join(sorted(planets))))
            else:
                parts.append(system)
        if parts:
            out[key] = ', '.join(parts)
    return out


def same_build(full, game_version):
    """Gehört die volle Kennung (`4.10.1-live.12660092`) zum Spielstand
    (`4.10.1`)? Das Spiel nennt nur die Versionsnummer, scmdb hängt Kanal und
    Buildnummer an — ein Vergleich auf Gleichheit träfe nie."""
    full = full or ''
    game_version = game_version or ''
    if not full or not game_version:
        return False
    return full == game_version or full.startswith(game_version + '-')


def refresh(game_version=''):
    """Die Tabelle holen, wenn sie fehlt oder zum Patch nicht mehr passt.

    Gibt die Zahl der Auftraege zurueck. Bei Netzfehlern bleibt der alte
    Stand stehen — eine veraltete Angabe ist besser als keine.
    """
    from . import cig_issues
    old = load()
    # Hat sich die Fehlerliste zum abgelegten Build geändert, wird neu
    # aufbereitet — sonst blieben Berichtigungen bis zum nächsten Patch aus.
    issues_same = ((old.get('datenfehler') or '')
                   == cig_issues.fingerprint(
                       cig_issues.issues_for(old.get('version'))))
    if old['auftraege'] and issues_same and (
            not game_version or same_build(old.get('version'), game_version)):
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
            if (game_version and same_build(entry.get('version'), game_version)
                    and 'ptu' not in (entry.get('version') or '')):
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
        raw_version = raw.get('version') or game_version or ''
        cig_issues.update(raw_version)
        issues = cig_issues.issues_for(raw_version)
        contract_map = prepare(raw, issues)
        if not contract_map:
            return len(old['auftraege'])
        save({'format': FORMAT,
                 'version': raw_version,
                 'auftraege': contract_map,
                 'regionen': prepare_regions(raw),
                 'datenfehler': cig_issues.fingerprint(issues)})
        return len(contract_map)
    except Exception as error:
        errors.record('reputation.refresh', error)
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


def region_line(key, word='Region', data=None):
    """`# Region: Pyro (Bloom)` — oder `''`, wenn nichts bekannt ist."""
    data = data if data is not None else load()
    text = (data.get('regionen') or {}).get(_key(key))
    return '# %s: %s' % (word, text) if text else ''
