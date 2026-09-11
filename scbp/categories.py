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
Zwei Ebenen statt einer langen Liste — Oberkategorie und Unterart.

**Das Problem, das dieses Modul löst.** Die Art-Auswahl hatte dreissig Einträge:
`Rüstung (Arme)`, `Rüstung (Beine)`, `Rüstung (Torso)`, `Helm`, `Rucksack`,
`Unteranzug`, `Kleidung (Jacke)`, `Kleidung (Schuhe)` … Wer eine ganze Rüstung
zusammenstellen will, sucht sich darin einen Wolf — und bei `Schiffswaffe (87)`
stand alles zusammen, ohne dass man sah, was davon ballistisch ist und was
Laser. Am 29.08.2026 gemeldet: *„da gibt es aber viele, und ich weiß grad nicht,
welche Ballistik sind, welche Laser, welche Repeater oder Cannon."*

**Die Gliederung ist nicht erfunden.** Sie folgt der Liste, die Xharig seit
Hand gepflegten Vergleichsliste: sieben
Oberkategorien, darunter die feinen Arten. Was sich dort bewährt hat, muss das
Werkzeug nicht neu erfinden.

**Woher die Angaben kommen** — drei Quellen, in dieser Reihenfolge:

1. **Der Tag der Rezeptdaten** ist am genauesten. `BP_CRAFT_APAR_BallisticGatling_S4`
   nennt die Waffenart direkt; so wird auch anderswo
   Ballistic Cannon von Ballistic Gatling. Gemessen: 89 Schiffswaffen und
   Werkzeuge tragen sie.
2. **Die Katalog-Art** (`Char_Armor_Helmet`, `Cooler`) für alles, was
   Körperteile oder Bauteilart meint.
3. **Der Rezept-Untertyp** (`pistol`, `sniper`, `shotgun`) für FPS-Waffen.

Was in keine Kategorie fällt, landet unter „Sonstiges" — sichtbar, nicht
verschwunden.

⚠ Bis zum 12.09.2026 hieß dieses Modul `kategorien` (Sprachumstellung P4,
Stufe 2). **Nur Bezeichner sind umbenannt, keine Zeichenketten.** Bewusst
gleich geblieben: die Kennungen der Gruppen (`'schiffswaffe'`, `'ruestung'`,
`'kleidung'` …) und der feinen Arten (`'ballistic_cannon'`, `'helm'` …) —
über sie holt die Oberfläche ihre Texte (`kat_ober_…`, `kat_unter_…`), und
der Vorsatz `art:` für Einzelgänger steht in gespeicherten Filtern.
"""
import re

from .sprache import t

# --- Die feinen Arten, wie sie im Tag der Rezeptdaten stehen ---------------
# ⚠ Reihenfolge egal, aber die Schreibweise muss zum Tag passen; verglichen
# wird ohne Rücksicht auf Gross- und Kleinschreibung.
_TAG_KINDS = (
    # ⚠ Die zusammengesetzten zuerst: `BallisticScatterGun` muss vor
    # `ScatterGun` stehen, sonst schluckt das kürzere Wort den Treffer und
    # sechs von sieben Scatterguns fielen durch — gemessen am 29.08.2026.
    'BallisticScatterGun', 'LaserScatterGun',
    'BallisticCannon', 'BallisticGatling', 'BallisticRepeater',
    'LaserCannon', 'LaserRepeater',
    'DistortionCannon', 'DistortionRepeater',
    'NeutronCannon', 'NeutronRepeater',
    'TachyonCannon', 'ScatterGun', 'MassDriver',
    'MiningLaser', 'SalvageModifier', 'SalvageHead', 'TractorBeam',
)
_TAG_PATTERN = re.compile(r'_(%s)(?:_|$)' % '|'.join(_TAG_KINDS), re.I)

# --- Oberkategorie je feiner Art ------------------------------------------
# Die sieben Gruppen aus einer erprobten Liste.
SHIP_WEAPON = 'schiffswaffe'
SHIP_MODULE = 'schiffsmodul'
SHIP_TOOL = 'schiffswerkzeug'
FPS_WEAPON = 'fpswaffe'
GEAR = 'ausruestung'
ARMOUR = 'ruestung'
CLOTHING = 'kleidung'
OTHER = 'sonstiges'

# Reihenfolge im Auswahlfeld — dieselbe wie in der Vergleichsliste.
TOP_ORDER = (SHIP_WEAPON, SHIP_MODULE, SHIP_TOOL, FPS_WEAPON,
             GEAR, ARMOUR, CLOTHING, OTHER)

_FROM_TAG = {
    'ballisticcannon': (SHIP_WEAPON, 'ballistic_cannon'),
    'ballisticgatling': (SHIP_WEAPON, 'ballistic_gatling'),
    'ballisticrepeater': (SHIP_WEAPON, 'ballistic_repeater'),
    'lasercannon': (SHIP_WEAPON, 'laser_cannon'),
    'laserrepeater': (SHIP_WEAPON, 'laser_repeater'),
    'distortioncannon': (SHIP_WEAPON, 'dist_cannon'),
    'distortionrepeater': (SHIP_WEAPON, 'dist_repeater'),
    'neutroncannon': (SHIP_WEAPON, 'neutron_cannon'),
    'neutronrepeater': (SHIP_WEAPON, 'neutron_repeater'),
    'tachyoncannon': (SHIP_WEAPON, 'tachyon_cannon'),
    'scattergun': (SHIP_WEAPON, 'scatter_gun'),
    'ballisticscattergun': (SHIP_WEAPON, 'scatter_gun'),
    'laserscattergun': (SHIP_WEAPON, 'scatter_gun'),
    'massdriver': (SHIP_WEAPON, 'mass_driver'),
    'mininglaser': (SHIP_TOOL, 'mining_laser'),
    'salvagemodifier': (SHIP_TOOL, 'salvage_modifier'),
    'salvagehead': (SHIP_TOOL, 'salvage_head'),
    'tractorbeam': (SHIP_TOOL, 'tractor_beam'),
}

# --- Aus der Katalog-Art --------------------------------------------------
_FROM_KIND = {
    'char_armor_helmet': (ARMOUR, 'helm'),
    'char_armor_torso': (ARMOUR, 'torso'),
    'char_armor_arms': (ARMOUR, 'arme'),
    'char_armor_legs': (ARMOUR, 'beine'),
    'char_armor_undersuit': (ARMOUR, 'unteranzug'),
    'char_armor_backpack': (GEAR, 'rucksack'),
    'backpack': (GEAR, 'rucksack'),
    'undersuit': (ARMOUR, 'unteranzug'),
    'char_clothing_torso': (CLOTHING, 'oberkoerper'),
    'char_clothing_legs': (CLOTHING, 'beine'),
    'char_clothing_feet': (CLOTHING, 'schuhe'),
    'char_clothing_jacket': (CLOTHING, 'jacke'),
    'cooler': (SHIP_MODULE, 'cooler'),
    'powerplant': (SHIP_MODULE, 'powerplant'),
    'quantumdrive': (SHIP_MODULE, 'quantumdrive'),
    'shield': (SHIP_MODULE, 'schild'),
    'radar': (SHIP_MODULE, 'radar'),
    'weaponattachment': (GEAR, 'aufsatz'),
    'weaponmagazine': (GEAR, 'magazin'),
    'magazine': (GEAR, 'magazin'),
    'dockingcollar': (SHIP_TOOL, 'andockkragen'),
    'fuelnozzle': (SHIP_TOOL, 'fuelnozzle'),
    'weaponmining': (SHIP_TOOL, 'mining_laser'),
    'container': (GEAR, 'behaelter'),
    'cargomodule': (SHIP_TOOL, 'frachtmodul'),
}

# --- Aus dem Rezept-Untertyp (FPS-Waffen) ---------------------------------
_FROM_SUBTYPE = {
    'pistol': (FPS_WEAPON, 'pistole'),
    'rifle': (FPS_WEAPON, 'gewehr'),
    'sniper': (FPS_WEAPON, 'sniper'),
    'smg': (FPS_WEAPON, 'smg'),
    'shotgun': (FPS_WEAPON, 'schrotflinte'),
    'lmg': (FPS_WEAPON, 'lmg'),
}


# Magazine tragen keine eigene Katalog-Art — sie stehen unter
# `WeaponAttachment` zwischen Zielfernrohren und Griffen. Ihr Tag endet aber
# immer auf `_mag` (oder `_mag_civilian`). ⚠ Ohne diese Zeile lagen 18 Magazine
# unter „Waffenaufsatz", während sie andernorts als
# eigene Gruppe führt.
_MAGAZINE = re.compile(r'_mag(?:_|$)', re.I)


def _from_tag(tag):
    tag = tag or ''
    if _MAGAZINE.search(tag):
        return (GEAR, 'magazin')
    m = _TAG_PATTERN.search(tag)
    if not m:
        return None
    return _FROM_TAG.get(m.group(1).lower())


def classify(art='', tag='', unterart='', rezeptart=''):
    """Ober- und Unterkategorie eines Bauplans — `(ober, unter)`.

    Die Reihenfolge der Quellen ist Absicht: Der Tag ist am genauesten, die
    Katalog-Art am verlässlichsten, der Untertyp am gröbsten. Wer sie anders
    herum abfragt, bekommt bei einer ballistischen Gatling nur „Waffe".

    ⚠ Die Namen der Schlüsselwörter (`art`, `tag`, `unterart`, `rezeptart`)
    bleiben vorerst deutsch: Die Aufrufer in `seiten.py` und
    `bestandsfenster.py` rufen sie so, und die Dateien sind noch nicht an der
    Reihe. Sie wandern mit, wenn ihre eigene Stufe drankommt.
    """
    hit = _from_tag(tag)
    if hit:
        return hit
    hit = _FROM_KIND.get((art or '').lower())
    if hit:
        return hit
    hit = _FROM_SUBTYPE.get((unterart or '').lower())
    if hit:
        return hit
    # Munition zählt zur Ausrüstung — sie gehört zur Waffe, nicht zum Schiff.
    if (rezeptart or '').lower() == 'ammo':
        return (GEAR, 'munition')
    if (art or '').lower().startswith('weapongun'):
        return (SHIP_WEAPON, '')
    if (art or '').lower().startswith('weapon'):
        return (FPS_WEAPON, '')
    if (art or '').lower().startswith('char_armor'):
        return (ARMOUR, '')
    if (art or '').lower().startswith('char_clothing'):
        return (CLOTHING, '')
    # ⚠ Was sich nicht bündeln lässt, bleibt **allein stehen** — mit seinem
    # eigenen Namen, nicht in einem Sammeltopf „Sonstiges". Xharig:
    # „nur was man nicht bündeln kann, sollte noch alleine stehen bleiben."
    # Ein Andockkragen gehört in keine der sieben Gruppen, ist aber eine klare
    # Sache — er verschwindet nicht, er steht für sich.
    if art:
        return ('art:' + art, '')
    return (OTHER, '')


def is_group(key):
    """Ist das eine der sieben Gruppen — oder ein Einzelgänger?"""
    return bool(key) and not str(key).startswith('art:')


def raw_kind(key):
    """Die Katalog-Art hinter einem Einzelgänger (`art:Cooler` → `Cooler`)."""
    s = str(key or '')
    return s[4:] if s.startswith('art:') else ''


def top_name(key):
    """Wie eine Oberkategorie im Fenster heisst.

    Einzelgänger tragen ihren Katalognamen — den kennt der Aufrufer besser als
    dieses Modul, deshalb gibt es hier den Rohwert zurück.
    """
    if not key:
        return ''
    if not is_group(key):
        return raw_kind(key)
    return t('kat_ober_%s' % key)


def sub_name(key):
    """Wie eine Unterart im Fenster heisst."""
    return t('kat_unter_%s' % key) if key else ''
