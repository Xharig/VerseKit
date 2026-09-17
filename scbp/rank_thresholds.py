# SPDX-License-Identifier: GPL-3.0-only
#
# VerseKit — ab wie viel Ruf ein Rang beginnt
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
Ab wie viel Ruf ein Rang beginnt — für die Rangnamen im Reputationsmenü.

Aus `Gildenmitglied` wird `Gildenmitglied [ab 10.000]`. Der Füllstand des
Balkens selbst kommt vom Spielserver und steht nirgends geschrieben; die
Schwelle macht ihn trotzdem lesbar: Man sieht, wo man steht und was als
Nächstes kommt. Gewünscht von KynoTnis (ADI), 16.09.2026.

## Woher die Zahlen kommen

Aus den entpackten Spieldateien im Repo `StarCitizenWiki/scunpacked-data`,
`factions/<datei>.json` → `Reputation.Context.PrimaryScope.Standings`
(`Name`, `MinReputation`). Das Repo wird je Patch neu erzeugt.

⚠ **Abgerufen, nicht mitgeliefert.** Das Repo nennt keine Lizenz — dieselbe
Linie wie bei scmdb: abrufen ja, mitliefern nein. Gespeichert wird nur das
Ergebnis (ein paar Dutzend Zahlen), gebunden an die Spielversion.

## ⚠⚠ Die Zuordnung ist Handarbeit — und nur belegte Stufen stehen drin

Kein Datensatz verweist auf seinen Sprachschlüssel. Die Schlüssel heißen auch
nicht gleich: `RepStanding_Bounty_MasterBountyHunter_Name` („Erfahrenes
Gildenmitglied") ist in den Spieldaten `BountyHunter_VeteranAgent_…`. Die
Tabelle `RANKS` unten ist deshalb am 17.09.2026 Stufe für Stufe über
Rangnummer bzw. Reihenfolge **und** den englischen Text belegt worden
(43 Schlüssel).

Bewusst **weggelassen**:

| Schlüssel | Warum |
|---|---|
| `RepScope_Contractor_Rank0` | Schlüssel sagt „Applicant", Spieldaten „Neutral" — unklar, was angezeigt wird |
| `RepStanding_NotEligible`, `…_NotEligible_Name` | Schwelle −1000, teilen sich mehrere Parteien |

Eine falsche Zahl ist schlimmer als keine: Wer „ab 40.000" liest und bei
45.000 nicht aufsteigt, hält das Werkzeug für kaputt.

## ⚠ Geprüft wird bei jedem Abruf

Fehlt eine Stufe einer Gruppe in den Spieldaten, oder steigen die Schwellen in
unserer Reihenfolge nicht an, bekommt die **ganze Gruppe** keine Zahl. Dann
hat CIG etwas umgebaut, und die Handzuordnung stimmt womöglich nicht mehr.
"""
import json
import os
import re

from . import errors, paths

CACHE_FILE = 'rufstufen.json'
FORMAT = 1

# Stufen ab dieser Schwelle bekommen eine Zahl. Die Einstiegsstufe (0) und
# „nicht berechtigt" (−1000) tragen keine Auskunft.
MIN_SHOWN = 1

SETTING = 'ruf_stufen'

BASE = ('https://raw.githubusercontent.com/StarCitizenWiki/scunpacked-data/'
        'master/factions')
TIME_LIMIT = 30

# ⚠ Wer den Netzzugriff abschaltet, meint auch diesen (Prüfung 74).
OFF = os.environ.get('SC_BP_NO_NET', '') not in ('', '0')

# Gruppe -> (Datei, Scope, [(Sprachschlüssel, Stufenname in den Spieldaten), …])
# Die Liste steht in AUFSTEIGENDER Reihenfolge — daran wird geprüft.
# Eine Datei je Gruppe genügt: Alle Parteien desselben Scopes haben dieselben
# Schwellen (gemessen am 17.09.2026 über 38 Dateien).
RANKS = {
    'contractor': ('faction_reputation_lawful_foxwellenforcement.json',
                   'FactionReputation',
                   [('RepScope_Contractor_Rank%d' % n,
                     'FactionRep_Allied_Rank%d' % n) for n in range(1, 7)]),
    'technician': ('faction_lawful_aciedo.json', 'Technician',
                   [('RepScope_Technician_Rank%d' % n,
                     'Technician_Rank%d' % n) for n in range(0, 6)]),
    'bounty': ('faction_reputation_lawful_bountyhuntersguild.json',
               'BountyHunter_BountyHuntersGuild',
               [('RepStanding_Bounty_Applicant_Name',
                 'BountyHunter_Applicant_BountyHuntersGuild'),
                ('RepStanding_Bounty_Probation_Name',
                 'BountyHunter_Probation_BountyHuntersGuild'),
                ('RepStanding_Bounty_Junior_Name',
                 'BountyHunter_Junior_BountyHuntersGuild'),
                ('RepStanding_Bounty_MidLevel_Name',
                 'BountyHunter_MidLevel_BountyHuntersGuild'),
                ('RepStanding_Bounty_Senior_Name',
                 'BountyHunter_Senior_BountyHuntersGuild'),
                # ⚠ Die Namen tauschen hier: „Master" im Schlüssel ist
                # „VeteranAgent" in den Spieldaten, „Legendary" ist „MasterAgent".
                ('RepStanding_Bounty_MasterBountyHunter_Name',
                 'BountyHunter_VeteranAgent_BountyHuntersGuild'),
                ('RepStanding_Bounty_LegendaryBountyHunter_Name',
                 'BountyHunter_MasterAgent_BountyHuntersGuild')]),
    'hauling': ('faction_reputation_lawful_covalexshipping.json', 'Hauling',
                [('RepStanding_TransportGuild_Rank%d' % n,
                  'Delivery_Rank%d' % n) for n in range(0, 7)]),
    'security': ('factionreputation_lawful_northrockservicegroup.json',
                 'Security_MercenaryGuild',
                 [('RepStanding_Security_Rank%d' % n,
                   'Security_Rank%d_MercenaryGuild' % n) for n in range(0, 5)]
                 + [('RepStanding_Security_Rank5', 'Security_Rank5'),
                    ('RepStanding_Security_Rank6', 'Security_Rank6')]),
    'battaglia': ('factionreputation_battaglia.json',
                  'MissionProviderReputation_Battaglia',
                  [('mg_battaglia_RepScope_Rank%d' % n,
                    'MissionProviderRep_Battaglia_Rank%d' % n)
                   for n in range(0, 6)]),
    'wikelo': ('factionreputation_wikelo.json', 'Wikelo',
               [('RepStanding_Barter_Rank0_Name', 'Nothing'),
                ('RepStanding_Barter_Rank1_Name', 'Armour'),
                ('RepStanding_Barter_Rank2_Name', 'Wolf')]),
}

# Unser Zusatz am Ende eines Rangnamens — zum Wiedererkennen und Abschneiden.
_SUFFIX = re.compile(r'\s\[(?:ab [\d.]+|[\d,]+\+)\]$')


def path():
    return paths.app_file(CACHE_FILE)


def load():
    """Die gespeicherten Schwellen — `{'version':…, 'stufen': {schlüssel: zahl}}`."""
    try:
        with open(path(), encoding='utf-8') as f:
            data = json.load(f)
        if (isinstance(data, dict) and data.get('format') == FORMAT
                and isinstance(data.get('stufen'), dict)):
            return data
    except (OSError, ValueError):
        pass
    except Exception as error:
        errors.record('rank_thresholds.load', error)
    return {'format': FORMAT, 'version': '', 'stufen': {}}


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
        errors.record('rank_thresholds.save', error)
        return False


def _fetch(file_name):
    import urllib.request
    request = urllib.request.Request('%s/%s' % (BASE, file_name),
                                     headers={'User-Agent': 'VerseKit'})
    with urllib.request.urlopen(request, timeout=TIME_LIMIT) as reply:
        return json.loads(reply.read().decode('utf-8'))


def _standings(raw, scope_name):
    """`{Stufenname: MinReputation}` eines Scopes — Primär- und Zusatzscopes."""
    context = ((raw or {}).get('Reputation') or {}).get('Context') or {}
    scopes = [context.get('PrimaryScope')] + list(
        context.get('AdditionalScopes') or [])
    for scope in scopes:
        if isinstance(scope, dict) and scope.get('ScopeName') == scope_name:
            return {s.get('Name'): s.get('MinReputation')
                    for s in (scope.get('Standings') or [])
                    if isinstance(s, dict)}
    return {}


def prepare(raw_by_file):
    """Aus den Rohdateien die Tabelle `{Sprachschlüssel: Schwelle}` bauen.

    ⚠ Eine Gruppe kommt nur ganz oder gar nicht: fehlt eine Stufe, ist ein
    Wert keine Zahl, oder steigen die Schwellen in unserer Reihenfolge nicht
    an, fällt die Gruppe weg (siehe Modultext)."""
    out = {}
    for file_name, scope_name, ranks in RANKS.values():
        found = _standings(raw_by_file.get(file_name), scope_name)
        values = [found.get(name) for _key, name in ranks]
        if not all(isinstance(v, int) and not isinstance(v, bool)
                   for v in values):
            continue
        if any(b <= a for a, b in zip(values, values[1:])):
            continue
        for (key, _name), value in zip(ranks, values):
            out[key] = value
    return out


def refresh(game_version=''):
    """Die Schwellen holen, wenn sie fehlen oder zum Patch nicht mehr passen.

    Gibt die Zahl der Schlüssel zurück. Bei Netzfehlern bleibt der alte Stand."""
    old = load()
    if old['stufen'] and (not game_version
                          or old.get('version') == game_version):
        return len(old['stufen'])
    if OFF:
        return len(old['stufen'])
    raw_by_file = {}
    for file_name, _scope, _ranks in RANKS.values():
        try:
            raw_by_file[file_name] = _fetch(file_name)
        except Exception as error:
            errors.record('rank_thresholds.refresh', error)
    table = prepare(raw_by_file)
    if not table:
        return len(old['stufen'])
    save({'format': FORMAT, 'version': game_version or '', 'stufen': table})
    return len(table)


def suffix(value, lang_code):
    """` [ab 10.000]` auf Deutsch, ` [10,000+]` auf Englisch."""
    if lang_code == 'de':
        return ' [ab %s]' % '{:,}'.format(value).replace(',', '.')
    return ' [%s+]' % '{:,}'.format(value)


def strip_suffix(text):
    """Unseren Zusatz vom Ende eines Rangnamens abschneiden — sonst nichts."""
    return _SUFFIX.sub('', text or '')


def with_suffix(text, tail):
    """Rangname mit Schwelle. Ein alter Zusatz wird vorher entfernt, damit ein
    zweiter Lauf nicht `[ab 800] [ab 800]` schreibt."""
    return strip_suffix(text).rstrip() + tail


def build_table(lang_code, data=None):
    """`{Sprachschlüssel: Zusatz}` für die Injektion — leer, wenn abgeschaltet."""
    if not paths.setting_bool(SETTING, True):
        return {}
    data = data if data is not None else load()
    return {key: suffix(value, lang_code)
            for key, value in (data.get('stufen') or {}).items()
            if isinstance(value, int) and value >= MIN_SHOWN}
