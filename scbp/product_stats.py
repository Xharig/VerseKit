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
Die Produktwerte eines gebauten Gegenstands — Grundwert gegen gebauten Wert.

Beantwortet: „Was hat meine NDB-30 mit diesem Erz am Ende für DPS?"

⚠⚠ **Warum es dieses Modul gibt.** Bis v3.42.4 zeigte die Herstellung je
Material nur einen Faktor („Aufprallwucht × 1.024"). Zwei Materialien, die
dieselbe Eigenschaft heben, standen als zwei Zeilen untereinander — die
Summe musste man selbst bilden, und DPS, Schildstärke oder Kühlleistung
standen nirgends. Am 16.09.2026 gemeldet: im Spiel werde „nur die HP
verbessert, nicht die Feuerkraft". Die Faktoren stimmten (an allen 1.607
Bauplänen gegen scmdb nachgerechnet, keine Abweichung), nur sah man nicht,
was daraus wird.

**Die Rechnung folgt scmdb.net genau**, damit beide Werkzeuge dieselbe Zahl
zeigen:

* Je Slot gilt die Qualität des Materials. Je Eigenschaft wird die Spanne
  genommen, in die die Qualität fällt — sonst die **letzte**.
* Mehrere Slots auf dieselbe Eigenschaft werden **addiert**, nicht
  multipliziert: ×1.042 und ×1.032 ergeben ×1.074 (+4,2 % + 3,2 %).
* `additive`-Wirkungen (Power Pips) sind Stückzahlen, gerundet und summiert.
* Welcher Schlüssel welchen Grundwert trifft, steht in `_apply_*`.

**Woher die Grundwerte kommen:** `crafting_items-<build>.json` von scmdb.net,
zur Laufzeit geholt, nie mitgeliefert (Lizenz siehe `crafting.py`). Abgelegt
wird nur, was zu einem Bauplan gehört, mit aufgelösten Verweisen in die
gemeinsamen Tabellen (`fireModesPools` usw.).
"""
from . import fehler
from .language import t

SOURCE = 'crafting_items-%s.json'

# Felder, die für die Anzeige nichts beitragen und die Ablage nur aufblähen.
_DROP = ('acceptedResources', 'acceptedResourceGroups', 'knownIssue', 'tags',
         'shieldResistance', 'shieldAbsorption')

# Verweis-Feld -> Tabelle in der Quelle -> Name im abgelegten Gegenstand.
_POOLS = (('fireModesIndex', 'fireModesPools', 'fireModes'),
          ('ammoIndex', 'ammoPools', 'ammo'),
          ('magazineIndex', 'magazinePools', 'magazine'),
          ('damageResistanceIndex', 'damageResistancePools', 'damageResistance'),
          ('signaturesIndex', 'signaturesPools', 'signatures'))

DAMAGE_TYPES = ('physical', 'energy', 'distortion', 'thermal', 'biochemical',
                'stun')


# ------------------------------------------------------------------ Ablage


def compact(raw_items, blueprints):
    """{Entitäts-Kennung: Gegenstand} — nur, was ein Bauplan herstellt.

    ⚠ Auch `suggestedProductEntityClass` gehört dazu: Bei fünf Bauplänen hat
    CIG das Produkt vertauscht (`cigDataError`), scmdb zeigt dort die Werte
    des gemeinten Gegenstands. Ohne die Kennung stünde dort gar nichts.
    """
    wanted = set()
    for b in blueprints or []:
        for key in ('productEntityClass', 'suggestedProductEntityClass'):
            if b.get(key):
                wanted.add(b[key])
    result = {}
    for item in (raw_items or {}).get('items') or []:
        ident = item.get('entityClass')
        if ident not in wanted:
            continue
        entry = {k: v for k, v in item.items() if k not in _DROP}
        for index_key, pool_key, target in _POOLS:
            idx = entry.pop(index_key, None)
            pool = raw_items.get(pool_key) or []
            if isinstance(idx, int) and 0 <= idx < len(pool):
                entry[target] = pool[idx]
        result[ident] = entry
    return result


def product_for(blueprint, products):
    """Der Gegenstand zu einem Rezept-Eintrag — oder None (wie scmdb `Et`)."""
    if not blueprint or not products:
        return None
    if blueprint.get('cigDataError'):
        ident = blueprint.get('suggestedProductEntityClass')
        if ident and ident in products:
            return products[ident]
    return products.get(blueprint.get('productEntityClass') or '')


# ------------------------------------------------------------ Qualitätswirkung


def _lerp(m, q):
    span = float(m.get('endQuality', 0)) - float(m.get('startQuality', 0))
    a = float(m.get('modifierAtStart', 1))
    if span <= 0:
        return a
    share = max(0.0, min(1.0, (q - float(m.get('startQuality', 0))) / span))
    return a + share * (float(m.get('modifierAtEnd', 1)) - a)


def _slot_modifiers(slot):
    """Wirkungen eines Slots: die der ersten Option und die des Slots selbst."""
    options = slot.get('options') or [{}]
    return list(options[0].get('modifiers') or []) + list(slot.get('modifiers') or [])


def _slot_material(slot):
    o = (slot.get('options') or [{}])[0]
    return o.get('resourceName') or o.get('itemName') or ''


def combined(blueprint, quality_of, default=500.0):
    """(Faktoren, Pips) über alle Slots — `quality_of(material)` liefert Q.

    ⚠ Addiert, nicht multipliziert — so rechnet scmdb, und nur so stimmen
    die Zahlen mit dessen Seite überein.
    """
    mul, pips = {}, {}
    tiers = (blueprint or {}).get('tiers') or []
    if not tiers:
        return mul, pips
    for slot in tiers[0].get('slots') or []:
        q = quality_of(_slot_material(slot))
        q = float(default if q is None else q)
        groups = {}
        order = []
        for m in _slot_modifiers(slot):
            key = m.get('propertyKey')
            if key not in groups:
                groups[key] = []
                order.append(key)
            groups[key].append(m)
        for key in order:
            ranges = groups[key]
            hit = next((r for r in ranges
                        if float(r.get('startQuality', 0)) <= q
                        <= float(r.get('endQuality', 0))), ranges[-1])
            value = _lerp(hit, q)
            if ranges[0].get('additive'):
                pips[key] = pips.get(key, 0) + int(round(value))
            else:
                mul[key] = mul.get(key, 1.0) + (value - 1.0)
    return mul, pips


# ------------------------------------------------------------ Grundwerte ändern


def _damage(ammo):
    """(Schadenstabelle, Quelle) — Aufprall oder Detonation, wie scmdb `kr`."""
    if not ammo:
        return None, 'impact'
    impact, blast = ammo.get('damage'), ammo.get('detonationDamage')
    if not impact and not blast:
        return None, 'impact'
    if impact and blast:
        if _total(impact) < 1:
            return blast, 'detonation'
        return impact, 'impact'
    if impact:
        return impact, 'impact'
    return blast, 'detonation'


def _total(damage):
    return sum(v for v in (damage or {}).values() if isinstance(v, (int, float)))


def _single_type(damage):
    hits = [k for k, v in (damage or {}).items()
            if isinstance(v, (int, float)) and v > 0]
    return hits[0] if len(hits) == 1 else None


def _mode_entry(mode):
    """Bei Sequenzen zählt der erste Eintrag — wie auf scmdb."""
    if mode.get('type') == 'sequence' and mode.get('sequenceEntries'):
        return mode['sequenceEntries'][0]
    return mode


def dps(item):
    """Schaden je Sekunde einer Waffe — oder None (scmdb `qo`)."""
    modes = (item or {}).get('fireModes') or []
    if not modes:
        return None
    damage, _src = _damage(item.get('ammo'))
    if not damage:
        return None
    entry = next((m for m in modes if (m.get('fireRate') or 0) > 0), None)
    if entry is None:
        for m in modes:
            if m.get('type') == 'sequence':
                entry = next((e for e in m.get('sequenceEntries') or []
                              if (e.get('fireRate') or 0) > 0
                              or (e.get('delay') or 0) > 0), None)
                if entry:
                    break
    if entry is None:
        return None
    rate = entry.get('fireRate') or entry.get('delay') or 0
    if not rate:
        return None
    return rate / 60.0 * _total(damage) * (entry.get('damageMultiplier') or 1)


def _scaled_damage(damage, f):
    if not damage:
        return damage
    return {k: (damage.get(k) or 0) * f for k in DAMAGE_TYPES}


def _apply_weapon(item, mul):
    """scmdb `Km`: Feuerrate, Schaden, Rückstoß-Glättung."""
    out = dict(item)
    fr = mul.get('weapon_firerate')
    if fr and item.get('fireModes'):
        modes = []
        for m in item['fireModes']:
            if m.get('type') == 'sequence' and m.get('sequenceEntries'):
                m = dict(m, sequenceEntries=[
                    dict(e, fireRate=(e['fireRate'] * fr if e.get('fireRate')
                                      else e.get('fireRate')),
                         delay=(e['delay'] * fr if e.get('delay')
                                else e.get('delay')))
                    for e in m['sequenceEntries']])
            else:
                m = dict(m, fireRate=(m.get('fireRate') or 0) * fr)
            modes.append(m)
        out['fireModes'] = modes
    dmg = mul.get('weapon_damage')
    if dmg and item.get('ammo'):
        ammo = dict(item['ammo'])
        if ammo.get('damage'):
            ammo['damage'] = _scaled_damage(ammo['damage'], dmg)
        if ammo.get('detonationDamage'):
            ammo['detonationDamage'] = _scaled_damage(ammo['detonationDamage'], dmg)
        out['ammo'] = ammo
    smooth = mul.get('weapon_recoil_smoothness') or 1
    if item.get('fireModes') and smooth != 1:
        def scale(e):
            if not e.get('recoil'):
                return e
            r = dict(e['recoil'])
            for k in ('yawMaxDeg', 'pitchMaxDeg', 'smoothTime'):
                if r.get(k) is not None:
                    r[k] = r[k] * smooth
            return dict(e, recoil=r)
        modes = []
        for m in out.get('fireModes') or item['fireModes']:
            if m.get('type') == 'sequence' and m.get('sequenceEntries'):
                modes.append(dict(m, sequenceEntries=[scale(e) for e in
                                                      m['sequenceEntries']]))
            else:
                modes.append(scale(m))
        out['fireModes'] = modes
    return out


# Schlüssel -> Feld im Gegenstand, jeweils multipliziert (scmdb `Bx`).
_COMPONENT_FIELDS = (
    ('health_maxhealth', 'health'),
    ('shield_maxhealth', 'shieldHealth'),
    ('itemresource_coolantgeneration', 'coolingRate'),
    ('quantum_fuelrequirement', 'quantumFuelRequirement'),
    ('quantum_speed', 'driveSpeed'),
    ('radar_minaimassistdistance', 'minAimAssistRange'),
    ('radar_maxaimassistdistance', 'maxAimAssistRange'),
    ('weapon_hullscraping_speed', 'salvageSpeedMultiplier'),
    ('weapon_hullscraping_radius', 'radiusMultiplier'),
    ('weapon_hullscraping_efficiency', 'extractionEfficiency'),
    ('weapon_tractor_force', 'maxForce'),
    ('weapon_tractor_fullstrengthdist', 'fullStrengthDistance'),
    ('weapon_tractor_maxdist', 'maxDistance'),
    ('weapon_tractor_maxvolume', 'maxVolume'),
)


def _apply_component(item, mul, pips):
    out = dict(item)
    for key, field in _COMPONENT_FIELDS:
        if key in mul and item.get(field) is not None:
            out[field] = item[field] * mul[key]
    if pips.get('itemresource_powergeneration') and item.get('powerOutput') is not None:
        out['powerOutput'] = item['powerOutput'] + pips['itemresource_powergeneration']
    if mul.get('weapon_damage') is not None and item.get('beams'):
        out['beams'] = [dict(b, dps=(b.get('dps') or 0) * mul['weapon_damage'])
                        for b in item['beams']]
    return out


def _apply_armor(item, mul):
    """scmdb `Wm`: Schadensminderung, Temperatur, Strahlungsabbau."""
    out = dict(item)
    mit = mul.get('armor_damagemitigation')
    if mit and item.get('damageResistance'):
        res = dict(item['damageResistance'])
        for k in DAMAGE_TYPES:
            cell = res.get(k)
            if isinstance(cell, dict) and cell.get('multiplier'):
                reduction = (1 - cell['multiplier']) * mit
                res[k] = dict(cell, multiplier=1 - reduction)
        out['damageResistance'] = res
    temp = item.get('temperatureResistance')
    if temp:
        out['temperatureResistance'] = {
            'min': (temp.get('min') or 0) * (mul.get('armor_temperaturemin') or 1),
            'max': (temp.get('max') or 0) * (mul.get('armor_temperaturemax') or 1)}
    rad = item.get('radiationResistance')
    if mul.get('armor_radiationdissipation') and rad:
        out['radiationResistance'] = dict(
            rad, dissipationRate=(rad.get('dissipationRate') or 0)
            * mul['armor_radiationdissipation'])
    return out


def crafted(item, mul, pips):
    """Der Gegenstand nach der Qualitätswirkung."""
    if not item:
        return item
    if item.get('cgItemType') == 'WeaponMining':
        return _apply_component(item, mul, pips)
    if item.get('itemType') == 'weapon':
        return _apply_weapon(item, mul)
    if item.get('itemType') == 'shipcomponent':
        return _apply_component(item, mul, pips)
    return _apply_armor(item, mul)


# ------------------------------------------------------------------ Tabelle
#
# Eine Zeile: (Beschriftung, Grund, Gebaut, Einheit, Nachkommastellen, Art)
# Art: 'high' (mehr ist besser), 'low' (weniger ist besser), 'abs' (Betrag
# zählt, Rückstoß), 'dps' (hervorgehoben). Abschnitte: ('section', Titel).


def _row(rows, label, base, made, unit='', dec=0, kind='high'):
    # ⚠ Kein Grundwert, keine Zeile — scmdb blendet sie ebenso aus.
    if base is None:
        return
    rows.append(('row', label, base, made, unit, dec, kind))


def _info(item):
    parts = []
    if item.get('manufacturer'):
        parts.append(item['manufacturer'])
    if (item.get('mass') or 0) > 0:
        parts.append('%g kg' % item['mass'])
    return parts


def rows(item, made):
    """Info-Zeile und Tabellenzeilen — `[('info', text), ('section', …), ('row', …)]`."""
    return _without_empty_sections(_rows(item, made))


def _rows(item, made):
    """Die Zeilen je Gegenstandsart — der Aufbau folgt den Panels auf scmdb."""
    if not item:
        return []
    made = made or item
    out = []
    kind = item.get('cgItemType')
    attach = (item.get('attachType') or '').lower()

    if kind == 'WeaponMining':
        info = _info(item)
        if item.get('size') is not None:
            info.append(t('s_ps_groesse') % item['size'])
        out.append(('info', ' · '.join(info)))
        out.append(('section', t('s_ps_komponente')))
        _row(out, t('s_ps_integritaet'), item.get('health'), made.get('health'), ' HP')
        _row(out, t('s_ps_strom'), item.get('powerDraw'), made.get('powerDraw'), '', 1, 'low')
        _row(out, t('s_ps_reparatur'), item.get('selfRepairTime'),
             made.get('selfRepairTime'), ' s', 1, 'low')
        for i, beam in enumerate(item.get('beams') or []):
            mb = (made.get('beams') or [])[i] if i < len(made.get('beams') or []) else {}
            out.append(('section', {'ElectricArc': t('s_ps_laserkraft'),
                                    'Extraction': t('s_ps_extraktion')}.get(
                                        beam.get('hitType'), beam.get('hitType') or '')))
            d = beam.get('dps') or 0
            _row(out, t('s_ps_dps'), beam.get('dps'), mb.get('dps'), '',
                 2 if d < 10 else 0, 'dps')
            _row(out, t('s_ps_voll_bis'), beam.get('fullDamageRange'),
                 mb.get('fullDamageRange'), ' m')
            _row(out, t('s_ps_null_ab'), beam.get('zeroDamageRange'),
                 mb.get('zeroDamageRange'), ' m', 0, 'low')
        return out

    if item.get('itemType') == 'weapon':
        info = _info(item)
        ammo = item.get('ammo') or {}
        gun = kind == 'WeaponGun'
        if gun and (ammo.get('speed') or 0) > 0 and (ammo.get('lifetime') or 0) > 0:
            info.append(t('s_ps_reichweite_m') % round(ammo['speed'] * ammo['lifetime']))
        elif not gun and (item.get('combatRange') or {}).get('category'):
            info.append(t('s_ps_reichweite') % item['combatRange']['category'])
        if ((item.get('magazine') or {}).get('ammoCount') or 0) > 0:
            info.append(t('s_ps_magazin') % item['magazine']['ammoCount'])
        out.append(('info', ' · '.join(info)))
        base_dps, made_dps = dps(item), dps(made)
        base_dmg, src = _damage(item.get('ammo'))
        made_dmg, _ = _damage(made.get('ammo'))
        dtype = _single_type(base_dmg)
        label = t('s_ps_schaden_art') % t('s_ps_dmg_' + dtype) if dtype \
            else t('s_ps_schaden')
        if src == 'detonation':
            label += ' ' + t('s_ps_detonation')
        for i, mode in enumerate(item.get('fireModes') or []):
            made_modes = made.get('fireModes') or []
            mm = made_modes[i] if i < len(made_modes) else mode
            e, me = _mode_entry(mode), _mode_entry(mm)
            # Überschrift wie scmdb: Name, sonst Art; weicht die Art vom Namen
            # ab, steht sie in Klammern dahinter.
            name, typ = mode.get('name'), mode.get('type') or ''
            title = (name or typ).capitalize()
            if name and typ and typ != name.lower():
                title += ' (%s)' % typ
            out.append(('section', title))
            _row(out, t('s_ps_feuerrate'), e.get('fireRate') or e.get('delay'),
                 me.get('fireRate') or me.get('delay'), ' RPM')
            _row(out, label, _total(base_dmg) if base_dmg else 0,
                 _total(made_dmg) if made_dmg else 0, '', 1)
            if base_dps is not None and i == 0:
                _row(out, t('s_ps_dps'), base_dps, made_dps, '', 1, 'dps')
            rec, mrec = e.get('recoil'), me.get('recoil') or {}
            if rec:
                _row(out, t('s_ps_rueck_pitch'), rec.get('pitchMaxDeg'),
                     mrec.get('pitchMaxDeg'), '°', 3, 'abs')
                _row(out, t('s_ps_rueck_yaw'), rec.get('yawMaxDeg'),
                     mrec.get('yawMaxDeg'), '°', 3, 'abs')
                _row(out, t('s_ps_rueck_glatt'), rec.get('smoothTime'),
                     mrec.get('smoothTime'), ' s', 3, 'abs')
            sp = e.get('spread') or {}
            if sp.get('max') is not None:
                _row(out, t('s_ps_streuung'), sp.get('min'), sp.get('min'),
                     '–%.2f°' % sp['max'], 2)
            if (ammo.get('speed') or 0) > 0:
                _row(out, t('s_ps_geschoss'), ammo['speed'], ammo['speed'], ' m/s')
            if (e.get('heatPerShot') or 0) > 0 and item.get('heat'):
                _row(out, t('s_ps_hitze'), e['heatPerShot'], e['heatPerShot'],
                     ' / %s' % item['heat'].get('overheatTemperature'), 1)
        return out

    if item.get('itemType') == 'shipcomponent':
        info = _info(item)
        if item.get('size') is not None:
            info.append(t('s_ps_groesse') % item['size'])
        if item.get('componentClass'):
            info.append(item['componentClass'])
        out.append(('info', ' · '.join(info)))
        out.append(('section', t('s_ps_komponente')))
        _row(out, t('s_ps_integritaet'), item.get('health'), made.get('health'), ' HP')
        _row(out, t('s_ps_strom'), item.get('powerDraw'), made.get('powerDraw'), '', 1, 'low')
        _row(out, t('s_ps_em'), item.get('emSignature'), made.get('emSignature'), '', 0, 'low')
        if (item.get('selfRepairTime') or 0) > 0:
            _row(out, t('s_ps_reparatur'), item['selfRepairTime'],
                 made.get('selfRepairTime'), ' s', 1, 'low')
        if attach == 'shield':
            out.append(('section', t('s_ps_schild')))
            _row(out, t('s_ps_schild_hp'), item.get('shieldHealth'), made.get('shieldHealth'))
            _row(out, t('s_ps_regen'), item.get('shieldRegen'), made.get('shieldRegen'), '/s', 1)
            _row(out, t('s_ps_down_delay'), item.get('downedRegenDelay'),
                 made.get('downedRegenDelay'), ' s', 1, 'low')
            _row(out, t('s_ps_dmg_delay'), item.get('damagedRegenDelay'),
                 made.get('damagedRegenDelay'), ' s', 1, 'low')
        elif attach == 'radar':
            out.append(('section', t('s_ps_radar')))
            _row(out, t('s_ps_empfindlich'), item.get('sensitivity'), made.get('sensitivity'), '', 2)
            _row(out, t('s_ps_durchdringung'), item.get('piercing'), made.get('piercing'), '', 2)
            _row(out, t('s_ps_ping'), item.get('pingCooldown'), made.get('pingCooldown'),
                 ' s', 1, 'low')
            if item.get('minAimAssistRange') is not None \
                    or item.get('maxAimAssistRange') is not None:
                _row(out, t('s_ps_hilfe_min'), item.get('minAimAssistRange'),
                     made.get('minAimAssistRange'), ' m')
                _row(out, t('s_ps_hilfe_max'), item.get('maxAimAssistRange'),
                     made.get('maxAimAssistRange'), ' m')
            else:
                _row(out, t('s_ps_hilfe'), item.get('aimAssistRange'),
                     made.get('aimAssistRange'), ' m')
        elif attach == 'powerplant':
            out.append(('section', t('s_ps_kraftwerk')))
            _row(out, t('s_ps_leistung'), item.get('powerOutput'),
                 made.get('powerOutput'), ' Pips')
        elif attach == 'cooler':
            out.append(('section', t('s_ps_kuehler')))
            _row(out, t('s_ps_kuehlrate'), item.get('coolingRate'), made.get('coolingRate'), '', 1)
            _row(out, t('s_ps_ir'), item.get('irSignature'), made.get('irSignature'), '', 0, 'low')
        elif attach == 'tractorbeam':
            out.append(('section', t('s_ps_traktor')))
            _row(out, t('s_ps_kraft_min'), item.get('minForce'), made.get('minForce'), ' N')
            _row(out, t('s_ps_kraft_max'), item.get('maxForce'), made.get('maxForce'), ' N')
            _row(out, t('s_ps_voll_bis'), item.get('fullStrengthDistance'),
                 made.get('fullStrengthDistance'), ' m', 1)
            _row(out, t('s_ps_max_dist'), item.get('maxDistance'), made.get('maxDistance'), ' m', 1)
            _row(out, t('s_ps_max_vol'), item.get('maxVolume'), made.get('maxVolume'))
        elif attach == 'salvagemodifier':
            out.append(('section', t('s_ps_bergung')))
            for label, field in (('s_ps_tempo', 'salvageSpeedMultiplier'),
                                 ('s_ps_radius', 'radiusMultiplier'),
                                 ('s_ps_effizienz', 'extractionEfficiency')):
                b, m = item.get(field), made.get(field)
                _row(out, t(label), None if b is None else b * 100,
                     None if m is None else m * 100, ' %', 1)
        if item.get('fuelRate') is not None or item.get('quantumFuelRate') is not None:
            out.append(('section', t('s_ps_duese')))
            _row(out, t('s_ps_wasserstoff'), item.get('fuelRate'), made.get('fuelRate'),
                 '', 2, 'low')
            _row(out, t('s_ps_quantum_rate'), item.get('quantumFuelRate'),
                 made.get('quantumFuelRate'), '', 2, 'low')
        if attach == 'quantumdrive':
            out.append(('section', t('s_ps_quantum')))
            b, m = item.get('driveSpeed'), made.get('driveSpeed')
            _row(out, t('s_ps_tempo_q'), None if b is None else b / 1e6,
                 None if m is None else m / 1e6, ' Mm/s', 2)
            _row(out, t('s_ps_verbrauch'), item.get('quantumFuelRequirement'),
                 made.get('quantumFuelRequirement'), '', 4, 'low')
            _row(out, t('s_ps_spool'), item.get('spoolUpTime'), made.get('spoolUpTime'),
                 ' s', 1, 'low')
            _row(out, t('s_ps_abkuehlen'), item.get('cooldownTime'),
                 made.get('cooldownTime'), ' s', 1, 'low')
        elif attach == 'container':
            out.append(('section', t('s_ps_erzkapsel')))
            _row(out, t('s_ps_fracht'), item.get('cargoCapacitySCU'),
                 made.get('cargoCapacitySCU'), ' SCU')
        elif attach == 'miningmodifier':
            out.append(('section', t('s_ps_modul')))
            _row(out, t('s_ps_ladungen'), item.get('charges'), made.get('charges'))
            for key, value in (item.get('modifiers') or {}).items():
                label = 's_ps_mm_' + key.lower()
                _row(out, t(label) if t(label) != label else key, value,
                     (made.get('modifiers') or {}).get(key), '', 1)
        return out

    # Rüstung, Kleidung, Sonstiges (scmdb `Pk`)
    info = _info(item)
    if item.get('attachSubType'):
        info.append(item['attachSubType'])
    out.append(('info', ' · '.join(info)))
    res = item.get('damageResistance') or {}
    mres = made.get('damageResistance') or {}
    cells = [k for k in DAMAGE_TYPES if isinstance(res.get(k), dict)
             and res[k].get('multiplier') is not None]
    if cells:
        out.append(('section', t('s_ps_widerstand')))
        for k in cells:
            b = (1 - res[k]['multiplier']) * 100
            m = mres.get(k, {}).get('multiplier')
            _row(out, t('s_ps_dmg_' + k), b, None if m is None else (1 - m) * 100,
                 ' %', 1)
    temp = item.get('temperatureResistance')
    if temp:
        mtemp = made.get('temperatureResistance') or {}
        out.append(('section', t('s_ps_temperatur')))
        _row(out, t('s_ps_temp_min'), temp.get('min'), mtemp.get('min'), ' °C', 1, 'low')
        _row(out, t('s_ps_temp_max'), temp.get('max'), mtemp.get('max'), ' °C', 1)
    rad = item.get('radiationResistance')
    if rad:
        mrad = made.get('radiationResistance') or {}
        out.append(('section', t('s_ps_strahlung')))
        _row(out, t('s_ps_rad_schutz'), rad.get('capacity'), mrad.get('capacity'), ' REM')
        _row(out, t('s_ps_rad_abbau'), rad.get('dissipationRate'),
             mrad.get('dissipationRate'), ' REM/s')
    return out


def _without_empty_sections(out):
    """Überschriften ohne eine einzige Zeile darunter fallen weg.

    ⚠ Ein Bergungsmodul hat keine Integrität und keinen Stromverbrauch — dort
    stand sonst „Komponente" direkt über „Bergungsmodul", wie ein Versehen.
    """
    result = []
    for i, z in enumerate(out):
        if z[0] == 'section':
            nxt = next((y for y in out[i + 1:] if y[0] != 'info'), None)
            if nxt is None or nxt[0] == 'section':
                continue
        result.append(z)
    return result


def formatted(row):
    """(Grund, Gebaut, Änderung, Bewertung) als Text — wie scmdb `Qe`.

    ⚠ Verglichen werden die **gerundeten Texte**: Was auf der Anzeige gleich
    aussieht, bekommt „—" statt „+0,00 %". Bewertung: 'good', 'bad', 'neutral'.
    """
    _tag, _label, base, made, unit, dec, kind = row
    fmt = '%.' + str(int(dec)) + 'f'
    base_txt = fmt % base
    made_txt = None if made is None else fmt % made
    if made_txt is None or made_txt == base_txt:
        return base_txt + unit, '—', '', 'neutral'
    if not base:
        return base_txt + unit, made_txt + unit, '', 'neutral'
    if kind == 'abs':
        pct = (abs(made) - abs(base)) / abs(base) * 100
    else:
        pct = (made - base) / abs(base) * 100
    lower = kind in ('low', 'abs')
    good = pct < -0.05 if lower else pct > 0.05
    bad = pct > 0.05 if lower else pct < -0.05
    return (base_txt + unit, made_txt + unit, '%+.2f %%' % pct,
            'good' if good else 'bad' if bad else 'neutral')


def table(blueprint, products, quality_of):
    """Alles für die Anzeige in einem Aufruf — leer, wenn es nichts gibt."""
    try:
        item = product_for(blueprint, products)
        if not item:
            return []
        mul, pips = combined(blueprint, quality_of)
        return rows(item, crafted(item, mul, pips))
    except Exception as exc:
        fehler.merken('product_stats.table', exc)
        return []
