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
Der Bauplan-Katalog: welche Baupläne es gibt — und woher sie kommen.

Zwei Fragen beantwortet dieses Modul:

  **Was gibt es überhaupt?** 714 Baupläne (Stand 4.9.0). Das ist etwas anderes
  als „was ist craftbar": Die Datei `crafting_items` zählt 1573 Gegenstände,
  aber für die meisten davon droppt nie ein Bauplan. Für eine Liste zum Abhaken
  wäre die große Zahl irreführend.

  **Woher bekomme ich einen bestimmten?** Fraktion, Auftrag, nötiger Ruf,
  Belohnung. Für 655 der 714 (92 %) ist das auflösbar. **Genau das kann der
  SC Deutsch Launcher nicht** — „mir fehlt X" ist die halbe Information,
  „X droppt bei Fraktion Y ab Rang Z" ist die ganze.

Die Kette durch die Daten von scmdb.net:

    contracts[].blueprintRewards[].blueprintPool   (GUID)
        -> blueprintPools[GUID].blueprints[].name  = der Bauplan
    contracts[].factionGuid  -> factions[GUID].name
    contracts[].minStanding  -> Rang und nötige Rufpunkte
    contracts[].factionRewardsIndex -> factionRewardsPools[i] = Ruf-Gewinn

> **Die Daten werden NICHT mitgeliefert.** scmdb steht unter CC BY-NC-ND 4.0;
> eine Kopie im Repo wäre eine Weitergabe und verstieße gegen diese Lizenz wie
> gegen die GPL dieses Projekts. Geholt wird auf dem Rechner des Nutzers, so wie
> es ein Browser täte, mit ehrlicher Kennung — und nur einmal je Spielversion.
> `SC_BP_NO_NET=1` schaltet es ab.

Der Sammel-Dump ist rund 12 MB. Deshalb wird er **nicht** aufgehoben, sondern
sofort zu einer kleinen eigenen Datei eingedampft (`katalog-cache.json`, etwa
ein Zwanzigstel davon).
"""
import json
import os
import re
import time
import urllib.error
import urllib.request

from . import fehler, patchhistorie, pfade, sprache
from .sprache import t


class Abgewiesen(Exception):
    """Die Seite hat den Abruf abgelehnt (HTTP 403).

    Eigene Ausnahme, damit der Fall von einem echten Netzfehler zu
    unterscheiden ist: Hier ist die Leitung in Ordnung, die Gegenseite mag
    nur nicht. Wiederholen hilft nicht, und die Meldung muss eine andere
    sein — sonst sucht man den Fehler bei sich."""


# ⭐ **Zwei Adressen für dieselben Daten** (seit 29.08.2026).
#
# Krovax (scmdb) hat auf Anfrage einen **öffentlichen Spiegel** eingerichtet,
# ausdrücklich für Programme: „Public mirror of SCMDB's static data JSONs for
# programmatic consumers." Der ist die erste Wahl, denn:
#
#   * **kein Bot-Schutz davor.** scmdb.net steht hinter Cloudflare; dessen
#     Regeln können sich ändern, ohne dass hier jemand etwas tut — und dann
#     stünde die Datenversorgung bei allen Nutzern still.
#   * GitHub Raw ist für genau diesen Zweck gedacht und stabil.
#
# scmdb.net bleibt als **Rückfall** stehen: Fällt der Spiegel aus oder hängt er
# hinterher, holt der Watcher weiter von der Originaladresse. Zwei Wege sind
# hier billig zu haben — und der Ausfall einer einzelnen Quelle legt sonst das
# ganze Werkzeug lahm.
SPIEGEL = 'https://raw.githubusercontent.com/KrovaxCode/SCMDB_DATA/main/data'
SCMDB = 'https://scmdb.net/data'
BASIS = SPIEGEL
CACHE = 'katalog-cache.json'

# Aufbau-Nummer des Katalogs — **nicht** die Spielversion.
#
# ⚠ Der Katalog wird sonst nur erneuert, wenn Star Citizen eine neue Version
# bringt. Ändert sich sein *Aufbau*, weil das Programm etwas Neues hineinlegt,
# reicht das nicht: Wer schon einen Katalog auf der Platte hat, behielte den
# alten bis zum nächsten Patch — der Umbau wäre für ihn unsichtbar.
#
# Am 28.08.2026 genau so aufgefallen: `_missionen()` führt seither die
# Preisstufen eines Auftrags zusammen (797 Baupläne, die vorher niemand sah).
# Ohne diese Nummer hätte kein einziger Tester den Fix zu Gesicht bekommen.
#
# **Hochzählen, sobald `_missionen()` oder `_herkunft()` etwas anders ablegen.**
FORMAT = 2
KENNUNG = 'SC-BP-Watcher/2.0 (+https://github.com/Xharig/SC-BP-Watcher)'
ZEITLIMIT = 120
AUS = os.environ.get('SC_BP_NO_NET', '') not in ('', '0')

# Die Bezeichnungen der Arten stehen im Sprachmodul — sie sind Oberflächentext
# und müssen mit umschalten. „Char_Armor_Helmet" ist nichts, was ein Mensch
# lesen sollte, und „Helm" nichts, was in einer englischen Liste stehen darf.
def ruf_deckel(katalog_daten, bauplan_schluessel):
    """Kann man sich diesen Bauplan durch **zu hohen Ruf** aussperren?

    Gibt `(rep_max, rang_max)` des großzügigsten Auftrags zurück — oder `None`,
    wenn mindestens ein Weg ohne Obergrenze bleibt.

    ⚠⚠ **Warum das jemand wissen will.** 280 der 353 Aufträge haben eine
    Ruf-OBERGRENZE (`maxStanding`): Steigt der Ruf bei der Fraktion darüber,
    wird der Auftrag **nicht mehr angeboten** — und seine Baupläne sind für
    diesen Spielstand endgültig weg. Wer fleißig Aufträge läuft, steigt im Rang
    und verliert dabei still den Zugang zu Bauplänen, die er noch nicht hat.
    Im Spiel steht das nirgends.

    ⚠ **Maßgeblich ist der großzügigste Weg.** Führen fünf Aufträge zu einem
    Bauplan und einer davon hat keine Obergrenze, ist nichts in Gefahr — dann
    gibt es hier `None`. Nur wenn **alle** Wege gedeckelt sind, zählt der
    höchste; bis dahin ist Zeit.

    ⚠ Der **eigene** Ruf-Stand steht nicht in der `Game.log` — nachgemessen am
    30.08.2026 über 22 Protokolle: Dort taucht `reputation` ausschließlich als
    Verbindungszeile zu CIGs `ReputationService` auf, nie ein Wert. Deshalb
    sagt diese Auskunft „ab wann zu", nicht „dir bleiben noch 4.200".
    """
    wege = []
    for mission in (katalog_daten.get('missionen') or {}).values():
        for name in mission.get('bp') or []:
            if _norm(name) == bauplan_schluessel:
                wege.append(mission)
                break
    if not wege:
        return None
    if any(not w.get('rep_max') for w in wege):
        return None                      # ein offener Weg genügt
    hoechster = max(wege, key=lambda w: w.get('rep_max') or 0)
    return hoechster.get('rep_max'), hoechster.get('rang_max')


def auftragsmerkmale(katalog_daten, bauplan_schluessel):
    """Was die Aufträge zu diesem Bauplan noch hergeben.

    Gibt `{'teilbar': bool|None, 'sperre': Minuten|None}` zurück — beides aus
    CIGs eigenen Vertragsdaten (`canBeShared`, `personalCooldownTime`).

    ⚠ **Teilbar nur, wenn ALLE Wege es sind.** „Den könnt ihr zu fünft laufen"
    darf nicht dastehen, wenn es nur für einen von vier Aufträgen gilt — dann
    steht die Staffel am falschen Auftrag. 334 der 353 sind teilbar, die
    Ausnahme ist also selten und genau deshalb wichtig.

    ⚠ **Die Sperre ist die kürzeste.** Sie sagt, wie schnell man es erneut
    versuchen kann; der langsamste Weg interessiert dabei niemanden.
    """
    wege = []
    for mission in (katalog_daten.get('missionen') or {}).values():
        for name in mission.get('bp') or []:
            if _norm(name) == bauplan_schluessel:
                wege.append(mission)
                break
    if not wege:
        return {'teilbar': None, 'sperre': None}
    sperren = [w['cooldown'] for w in wege if w.get('cooldown')]
    return {'teilbar': all(w.get('teilbar') for w in wege),
            'sperre': min(sperren) if sperren else None}


def lohnende_auftraege(katalog_daten, habe):
    """Welcher Auftrag bringt die meisten **fehlenden** Baupläne?

    Liste aus `(titel, fraktion, anzahl, uec, rang, wo)`, absteigend nach der
    Zahl der fehlenden Baupläne.

    ⚠ **Gezählt wird über die Bezugsquellen der Baupläne, nicht über
    `missionen`.** Die Missionsliste führt ihre Aufträge nur unter dem
    Textschlüssel (`headhunters_eliminateall_cfp_M_title_001`) — daraus wird
    ohne die Sprachdatei des Spiels kein lesbarer Titel. Die Bezugsquellen am
    Bauplan tragen den **aufgelösten** Titel bereits, samt Fraktion, Rang,
    Belohnung und Annahmeort. Gleiche Auskunft, kein zusätzlicher Datenweg.
    """
    auftraege = {}
    for schluessel, eintrag in (katalog_daten.get('bauplaene') or {}).items():
        if schluessel in habe:
            continue                      # was man hat, lohnt nicht mehr
        for quelle in eintrag.get('q') or []:
            titel = quelle.get('auftrag')
            if not titel:
                continue
            kennung = (titel, quelle.get('fraktion') or '')
            eintrag_a = auftraege.setdefault(kennung, {
                'anzahl': 0, 'uec': 0, 'rang': quelle.get('rang'),
                'wo': quelle.get('wo')})
            eintrag_a['anzahl'] += 1
            eintrag_a['uec'] = max(eintrag_a['uec'], quelle.get('uec') or 0)
    raus = [(titel, fraktion, w['anzahl'], w['uec'], w['rang'], w['wo'])
            for (titel, fraktion), w in auftraege.items()]
    raus.sort(key=lambda x: (-x[2], -x[3], x[0]))
    return raus


def art_lesbar(roh):
    """Aus 'Char_Armor_Helmet' wird 'Helm' bzw. 'Helmet'."""
    return sprache.art(ART_ZUSAMMEN.get(roh, roh))


# Arten, die dasselbe meinen und deshalb eine Gruppe bilden.
#
# ⚠ scmdb führt Magazine unter zwei Kennungen: 32 als `WeaponAttachment`
# (alle mit Subtyp „Magazine", nichts anderes steckt darin) und die beiden
# Start-Magazine als `ammo`. Für den Spieler ist das ein und dieselbe Sache —
# gemeldet als „Magazin für Waffen fehlen quasi alle Waffen bis auf 2": Der
# Filter „Magazin" zeigte die zwei, die 32 anderen standen unter
# „Waffenaufsatz".
#
# ⚠ Derselbe Fall bei den Handfeuerwaffen: 87 stehen als `WeaponPersonal` da, die
# S-38 Pistol und das P4-AR Rifle als `weapons`. Im Fenster ergab das zwei
# Gruppen — „FPS-Waffe (87)" und „Handfeuerwaffe (2)" — für ein und dieselbe
# Sache. Wer nach FPS-Waffen filtert, sucht auch die beiden.
ART_ZUSAMMEN = {'ammo': 'WeaponAttachment', 'weapons': 'WeaponPersonal'}


def art_kennung(eintrag_oder_roh):
    """Die Art, unter der ein Bauplan einsortiert und gefiltert wird.

    Nimmt einen Katalogeintrag oder die rohe Kennung. Zusammengehörende Arten
    (siehe `ART_ZUSAMMEN`) werden auf eine gezogen.
    """
    roh = (eintrag_oder_roh.get('a')
           if isinstance(eintrag_oder_roh, dict) else eintrag_oder_roh)
    return ART_ZUSAMMEN.get(roh, roh)


# So viele Bezugsquellen je Bauplan werden behalten.
#
# Stand 24.08.2026 **gemessen** am Dump 4.9.0-live.12344265 (655 Baupläne mit
# Quelle): Median 4 Wege, Mittelwert 5,8, Höchstwert 73. Die frühere Grenze von
# 3 hat damit **54 %** aller Baupläne Wege abgeschnitten — die Annahme „einer
# reicht, man nimmt ohnehin den leichtesten" war falsch. Sie stimmt nur, solange
# man den leichtesten auch fliegen *will*: Wer gerade bei einer anderen Fraktion
# Ruf sammelt, braucht den zweiten oder dritten Weg.
#
# Bei 12 verliert genau **ein** Bauplan etwas (der mit 73 Vorkommen). Angezeigt
# wird trotzdem nur der leichteste; der Rest steht hinter „weitere Wege".
QUELLEN_JE_BP = 12


# ------------------------------------------------------------------ Netz
VERSUCHE = 3


def _hole(url, zeitlimit=ZEITLIMIT, versuche=VERSUCHE):
    """Eine JSON-Datei holen — mit Wiederholung.

    Der Sammel-Dump ist rund 12 MB, und genau bei der Größe reißt die Leitung
    gern mitten drin ab (hier beim Bauen zweimal passiert). Ein einzelner
    Fehlversuch darf deshalb nicht heißen, dass es den Katalog nicht gibt."""
    letzter = None
    for versuch in range(versuche):
        try:
            req = urllib.request.Request(url, headers={'User-Agent': KENNUNG})
            with urllib.request.urlopen(req, timeout=zeitlimit) as r:
                return json.loads(r.read().decode('utf-8'))
        except urllib.error.HTTPError as fehler:
            # ⚠ 403 ist eine Absage, kein Wackelkontakt. Cloudflare weist
            # Abrufe ohne eigene Kennung ab; noch zweimal zu fragen kostet nur
            # Zeit und aendert nichts. Sofort raus, mit klarer Meldung.
            if fehler.code == 403:
                raise Abgewiesen(t('m_abgewiesen')) from fehler
            letzter = fehler
            if versuch + 1 < versuche:
                time.sleep(2 * (versuch + 1))
        except Exception as fehler:
            letzter = fehler
            if versuch + 1 < versuche:
                time.sleep(2 * (versuch + 1))
    raise letzter


def hole_datei(name, zeitlimit=ZEITLIMIT, versuche=3):
    """Eine Datendatei holen — erst vom Spiegel, dann von scmdb.net.

    **Die eine Stelle**, über die alle drei Datenmodule holen (Katalog,
    Herstellung, Bergbau). Wer eine Quelle ändert, ändert sie hier — nicht an
    drei Orten mit drei Schreibweisen.
    """
    letzter = None
    for basis in (SPIEGEL, SCMDB):
        try:
            return _hole('%s/%s' % (basis, name), zeitlimit=zeitlimit,
                         versuche=versuche)
        except Abgewiesen as ausnahme:
            # 403 heißt: Diese Quelle mag nicht. Die andere darf es versuchen.
            letzter = ausnahme
        except Exception as ausnahme:
            letzter = ausnahme
    raise letzter


def aktuelle_version():
    """Die laufende Spielversion laut scmdb (PTU wird übersprungen)."""
    for v in _hole(BASIS + '/versions.json', zeitlimit=15):
        name = (v.get('version') or '')
        if name and 'ptu' not in name.lower():
            return name
    return None


# ------------------------------------------------------------ Aufbereitung
# Alle Anführungszeichen, die in Bauplan-Namen vorkommen — gerade, typografische
# und die französischen. Sie werden beim Vergleichen auf ein einfaches `'`
# gezogen.
#
# ⚠ Warum das nötig ist: Der SC Deutsch Launcher exportiert `7MA "Lorica"` mit
# geraden doppelten Anführungszeichen, scmdb führt denselben Bauplan als
# `7MA 'Lorica'` mit einfachen. Ohne Angleichung sind das zwei verschiedene
# Schlüssel — der Bauplan galt als „fehlt", obwohl er im eigenen Bestand stand.
# Gefunden an einem echten Bestand mit 392 Bauplänen: 391 wurden zugeordnet,
# genau dieser eine nicht. An erfundenen Testdaten wäre das nie aufgefallen.
ANFUEHRUNG = str.maketrans({
    '"': "'", '„': "'", '“': "'", '”': "'",   # " „ " "
    '‘': "'", '’': "'", '«': "'", '»': "'",  # ' ' « »
})


def _norm(s):
    """Vergleichsform eines Namens — siehe `pfade.namensform`."""
    return pfade.namensform(s)


def _werte(rohe_items):
    """Name -> Art, Größe, Gütegrad, Klasse, Hersteller."""
    werte = {}
    for e in rohe_items.get('items', []):
        name = e.get('name')
        if name:
            werte.setdefault(_norm(name), {
                'a': e.get('attachType') or e.get('cgItemType'),
                'sub': e.get('attachSubType'),
                's': e.get('size'),
                'g': e.get('grade'),
                'c': e.get('componentClass'),
                'm': e.get('manufacturer'),
            })
    return werte


# So viele Annahmeorte werden genannt. Mehr hilft niemandem: Wer den Auftrag
# sucht, fliegt den nächsten an — eine Aufzählung von fünfzehn Lagrange-Punkten
# beantwortet die Frage „wo hole ich den?" schlechter als drei Planeten.
ORTE_JE_AUFTRAG = 4


def _annahmeorte(vertrag, orte_pool):
    """Wo sich der Auftrag annehmen lässt — System und die größeren Orte.

    Von der Orga gemeldet: „Man sieht, wo es den Bauplan gibt, aber nicht, wo
    man die Mission annehmen muss." Die Angabe steckt in `locations` als
    Kennungen; aufgelöst werden sie über `locationPools`.

    Planeten zuerst, danach der Rest — ein Planetenname ist die Auskunft, mit
    der ein Spieler etwas anfangen kann; „HUR L2" hilft nur, wenn man ohnehin
    schon weiß, wo man ist."""
    kennungen = vertrag.get('locations') or []
    planeten, sonstige = [], []
    for kennung in kennungen:
        ort = orte_pool.get(kennung)
        if not isinstance(ort, dict):
            continue
        name = ort.get('name')
        if not name:
            continue
        art = (ort.get('type') or '').lower()
        if art == 'star':
            continue                       # das System steht ohnehin dabei
        (planeten if art in ('planet', 'moon') else sonstige).append(name)

    def sauber(liste):
        gesehen, raus = set(), []
        for n in liste:
            if n not in gesehen:
                gesehen.add(n)
                raus.append(n)
        return raus

    namen = sauber(planeten) or sauber(sonstige)
    systeme = vertrag.get('availableSystems') or vertrag.get('systems') or []
    if not namen and not systeme:
        return None
    return {'system': ', '.join(systeme) or None,
            'orte': namen[:ORTE_JE_AUFTRAG],
            'mehr': max(0, len(namen) - ORTE_JE_AUFTRAG)}


# Vorsätze, die jeder Belohnungstopf trägt — sie sagen nichts aus.
_TOPF_VORSATZ = re.compile(r'^BP_(?:MISSION)?REWARDS?_', re.I)

# Töpfe, deren Namen ein Mensch nicht deuten muss. Alles andere wird nur
# aufgeräumt, nicht gedeutet — lieber „aus: RedWind" als eine erfundene Erklärung.
_TOPF_KLARTEXT = (
    (re.compile(r'^xenothreat', re.I), 'XenoThreat'),
    (re.compile(r'^rdc[_ ]?boss', re.I), 'RDC-Boss'),
    (re.compile(r'^superheavy', re.I), 'Super-Heavy-Mission'),
    (re.compile(r'^cds[_ ]', re.I), 'CDS-Rüstung'),
)


def topf_lesbar(roh):
    """Aus `BP_REWARDS_Xenothreat2_15_06` wird `XenoThreat`.

    Die 59 Baupläne ohne Auftrag liegen **nicht im Nichts** — sie stehen in
    benannten Belohnungstöpfen. Vorher stand bei ihnen nur ein `?`, und der
    Spieler wusste nicht, ob es ihn nie gibt oder ob nur die Daten fehlen. Der
    Topf-Name sagt ihm wenigstens, wonach er suchen muss.

    Gedeutet wird nur, was eindeutig ist. Der Rest wird lediglich lesbar
    gemacht: Vorsatz weg, Unterstriche zu Leerzeichen, die durchnummerierten
    Endungen (`_15_06`) abgeschnitten — sie sind Stufen desselben Topfes.
    """
    roh = (roh or '').strip()
    if not roh:
        return ''
    kern = _TOPF_VORSATZ.sub('', roh)
    for muster, klar in _TOPF_KLARTEXT:
        if muster.search(kern):
            return klar
    # Manche Töpfe heißen nach dem **Gegenstand**, nicht nach der Quelle:
    # `behr_rifle_ballistic_01_mr01` ist ein Behring-Gewehr, kein Ort und kein
    # Ereignis. Solche Namen zu zeigen wäre schlechter als nichts — der Spieler
    # läse eine Herkunft, die keine ist. Erkennbar sind sie daran, dass sie
    # durchgehend klein geschrieben sind; die echten Quellen (`RedWind`,
    # `Xenothreat2`) tragen Großbuchstaben.
    if kern and kern == kern.lower():
        return ''
    kern = re.sub(r'(_\d+)+$', '', kern)          # `_15_06` und Verwandte weg
    kern = re.sub(r'[_\-]+', ' ', kern).strip()
    # Zweite Schranke: Eine Quelle heißt kurz („RedWind", „RDC Boss"). Wo eine
    # ganze Gegenstandsbeschreibung steht („Carryable 2H FL MissionItem
    # Microsatellite a"), ist es wieder keine Herkunft, sondern das Ding selbst.
    if len(kern.split()) > 2:
        return ''
    return kern or ''


def _herkunft(merged):
    """Bauplan-Name -> Liste von Bezugsquellen, leichteste zuerst."""
    pools = {}
    for guid, pool in (merged.get('blueprintPools') or {}).items():
        pools[guid] = [b.get('name') for b in (pool.get('blueprints') or [])
                       if b.get('name')]
    factions = merged.get('factions') or {}
    belohnungen = merged.get('factionRewardsPools') or []
    orte_pool = merged.get('locationPools') or {}

    quellen = {}
    for vertrag in ((merged.get('contracts') or [])
                    + (merged.get('legacyContracts') or [])):
        ziele = [r.get('blueprintPool')
                 for r in (vertrag.get('blueprintRewards') or [])]
        if not ziele:
            continue
        fraktion = factions.get(vertrag.get('factionGuid')) or {}
        i = vertrag.get('factionRewardsIndex')
        rufgewinn = None
        if isinstance(i, int) and 0 <= i < len(belohnungen):
            rufgewinn = sum(e.get('amount', 0) for e in belohnungen[i])
        rang = vertrag.get('minStanding') or {}
        eintrag = {
            'auftrag': vertrag.get('title'),
            'typ': vertrag.get('missionType'),
            'fraktion': fraktion.get('name') if isinstance(fraktion, dict) else None,
            'uec': vertrag.get('rewardUEC'),
            'ruf': rufgewinn,
            'rang': rang.get('name'),
            'rep': rang.get('minReputation'),
            'wo': _annahmeorte(vertrag, orte_pool),
        }
        for guid in ziele:
            for name in pools.get(guid, []):
                quellen.setdefault(_norm(name), []).append(eintrag)

    # Leichtesten Weg zuerst: niedrigste Ruf-Anforderung, bei Gleichstand die
    # höhere Bezahlung. Dubletten (derselbe Auftrag über mehrere Pools) raus.
    for name, liste in quellen.items():
        gesehen, sauber = set(), []
        for e in sorted(liste, key=lambda x: ((x['rep'] if x['rep'] is not None
                                               else 10 ** 9),
                                              -(x['uec'] or 0))):
            schluessel = (e['auftrag'], e['fraktion'])
            if schluessel in gesehen:
                continue
            gesehen.add(schluessel)
            sauber.append(e)
        quellen[name] = sauber[:QUELLEN_JE_BP]
    return pools, quellen


def _startbauplaene(version):
    """Die Baupläne, die jeder Spieler von Anfang an hat.

    **Warum die extra geholt werden müssen:** Der Katalog entsteht aus den
    `blueprintPools` — also aus dem, was Missionen ausschütten. Startbaupläne
    stehen in **keinem** Pool, weil man sie nie als Belohnung bekommt. Sie
    fehlten dadurch vollständig: nicht in der Liste, nicht im Bestand, und wer
    danach suchte, fand nichts.

    Zu erkennen sind sie am Feld `isDefault` in `crafting_blueprints-<version>.json`
    — **nicht** in `crafting_items`, dort gibt es das Feld nicht. Es sind acht:
    P4-AR Rifle und S-38 Pistol samt Magazinen, dazu der Field Recon Suit
    (vier Teile).

    Die Datei ist mit 4,2 MB die größte der drei, wird aber nur beim Neubau des
    Katalogs geholt — also einmal je Spielversion."""
    try:
        roh = _hole('%s/crafting_blueprints-%s.json' % (BASIS, version))
    except Exception:
        return []
    liste = roh.get('blueprints') if isinstance(roh, dict) else roh
    ergebnis = []
    for e in liste or []:
        if not e.get('isDefault'):
            continue
        name = e.get('productName')
        if name:
            ergebnis.append({'n': name, 'a': e.get('type'), 'start': True})
    return ergebnis


def _missionen(merged):
    """Missionen, die Baupläne ausschütten — für die Auszeichnung im Spiel.

    Das ist die **Gegenrichtung** zu `_herkunft()`: dort „welcher Bauplan kommt
    woher", hier „welche Baupläne gibt diese Mission". Gebraucht wird sie von
    `scbp/injektion.py`, das die Angaben in die Textdatei des Spiels schreibt.

    Angehängt wird an den **Textschlüssel** (`descriptionLocKey`,
    `titleLocKey`), nicht an den Missionsnamen: Der Schlüssel ist in jeder
    Sprache derselbe, der Name nicht. Dadurch funktioniert dieselbe Zuordnung
    für die deutsche Übersetzung wie für die englische Version — und für die
    neun weiteren Sprachen im Spiel gleich mit."""
    pools = {}
    for guid, pool in (merged.get('blueprintPools') or {}).items():
        pools[guid] = [b.get('name') for b in (pool.get('blueprints') or [])
                       if b.get('name')]
    belohnungen_pools = merged.get('factionRewardsPools') or []

    # ⚠ **Ein Schlüssel, viele Verträge — der Fehler vom 28.08.2026.**
    # Hier stand am Ende `ergebnis[titel_key or text_key] = eintrag`. Verträge,
    # die sich einen Textschlüssel teilen, haben sich damit gegenseitig
    # überschrieben: Der zuletzt eingelesene gewann, alle anderen fielen still
    # weg. Gemessen am Dump 4.10.0-live.12519617:
    #
    #     353 Schlüssel, davon 123 mehrfach belegt
    #     319 Verträge fielen weg
    #     797 Bauplan-Einträge wurden dadurch nie angezeigt
    #
    # Gemeldet hat es Morkhan: „ich bekomme nicht angezeigt welche baupläne ich
    # beim neulingsauftrag bekommen kann, sondern NUR die auf der höchsten
    # stufe." Es war nicht die höchste Stufe — es war die zuletzt gelesene.
    #
    # Jetzt werden alle Varianten **zusammengeführt**: Die Liste zeigt, was der
    # Auftragstyp überhaupt hergibt, das Kästchen sagt, was man davon hat.
    # Dazu am 28.08.2026: „wenn es den da nicht gibt ist das egal, er
    # hat ihn, also gehört er abgehakt PUNKT."
    roh = {}
    for vertrag in ((merged.get('contracts') or [])
                    + (merged.get('legacyContracts') or [])):
        schluessel = vertrag.get('titleLocKey') or vertrag.get('descriptionLocKey')
        if schluessel:
            roh.setdefault(schluessel, []).append(vertrag)

    ergebnis = {}
    for schluessel, varianten in roh.items():
        # ⚠ Varianten **ohne** Baupläne bleiben in `gesehen` mit dabei. Vorher
        # flogen sie sofort raus — dadurch war nicht mehr erkennbar, dass es
        # Stufen dieses Auftrags gibt, die leer ausgehen. 14 Auftragstexte sind
        # so gebaut, darunter Morkhans Beispiel: Die Trainee-Stufe schüttet
        # nichts aus, hängt aber am selben Text wie die beiden höheren.
        gesehen = []
        for v in varianten:
            namen = []
            for r in (v.get('blueprintRewards') or []):
                namen.extend(pools.get(r.get('blueprintPool'), []))
            rang = v.get('minStanding') or {}
            gesehen.append({'namen': set(namen), 'vertrag': v,
                            'rep': rang.get('minReputation'),
                            'rang': rang.get('name'),
                            'uec': v.get('rewardUEC')})
        mit_bp = [g for g in gesehen if g['namen']]
        if not mit_bp:
            continue

        alle_namen = set()
        for g in mit_bp:
            alle_namen |= g['namen']

        # Der **leichteste** Weg liefert die Kopfdaten: niedrigster Ruf, bei
        # Gleichstand die höhere Bezahlung — dieselbe Regel wie in `_herkunft()`.
        def _leichtest(menge):
            return sorted(menge, key=lambda g: ((g['rep'] if g['rep'] is not None
                                                 else 10 ** 9),
                                                -(g['uec'] or 0)))[0]

        # Ab welchem Rang ein Bauplan überhaupt zu haben ist. Nur dort, wo sich
        # die Stufen wirklich unterscheiden — sonst stünde an jedem dasselbe.
        # Das ist reine Zusatzinfo: Sie ändert weder Kästchen noch Auswahl,
        # sondern beantwortet „warum finde ich den hier gerade nicht".
        ab_rang = {}
        if len(set(g['rep'] for g in mit_bp)) > 1:
            for name in alle_namen:
                q = _leichtest([g for g in mit_bp if name in g['namen']])
                if q['rang'] and q['rep']:
                    ab_rang[name] = {'rang': q['rang'], 'rep': q['rep']}
            # ⚠ **Nur wenn es die Baupläne unterscheidet.** Beim ersten Anlauf
            # stand die Angabe an *jedem* Bauplan derselbe Auftrags — zwölfmal
            # „erst ab Sr. Contractor" untereinander. Das ist kein Wissen,
            # sondern Lärm: Gilt für alle derselbe Rang, steht er ohnehin oben
            # unter „Min. Reputation". Die Zeile lohnt sich erst, wenn ein
            # Bauplan einen *höheren* Rang braucht als ein anderer.
            if len(set((b['rang'], b['rep']) for b in ab_rang.values())) < 2:
                ab_rang = {}

        leicht = _leichtest(mit_bp)
        vertrag = leicht['vertrag']
        belohnungen = vertrag.get('blueprintRewards') or []
        # „Sicher" und „Chance" über **alle** Varianten: Wenn irgendeine den
        # Bauplan garantiert gibt, ist er erreichbar.
        sicher = any(r.get('chance') == 1 for g in mit_bp
                     for r in (g['vertrag'].get('blueprintRewards') or []))
        chance = max((r.get('chance') or 0) for g in mit_bp
                     for r in (g['vertrag'].get('blueprintRewards') or []))
        rang = vertrag.get('minStanding') or {}
        hoechst = vertrag.get('maxStanding') or {}
        i = vertrag.get('factionRewardsIndex')
        rufgewinn = None
        if isinstance(i, int) and 0 <= i < len(belohnungen_pools):
            rufgewinn = sum(e.get('amount', 0) for e in belohnungen_pools[i])
        eintrag = {
            'bp': sorted(alle_namen),
            'sicher': sicher,
            'chance': chance,
            'rep': rang.get('minReputation'),
            'rang': rang.get('name'),
            'rep_max': hoechst.get('minReputation'),
            'rang_max': hoechst.get('name'),
            'uec': vertrag.get('rewardUEC'),
            'ruf': rufgewinn,
            'teilbar': vertrag.get('canBeShared'),
            'cooldown': (vertrag.get('personalCooldownTime')
                         if vertrag.get('hasPersonalCooldown') else None),
        }
        eintrag = {k: v for k, v in eintrag.items() if v not in (None, '', [])}
        eintrag['bp'] = sorted(alle_namen)
        eintrag['sicher'] = sicher
        if ab_rang:
            eintrag['ab'] = ab_rang
        # Wie viele Stufen dieses Auftrags leer ausgehen — steht als Warnung
        # dran, damit niemand für eine Liste hinfliegt, die seine Stufe nicht
        # hergibt.
        leer = len(gesehen) - len(mit_bp)
        if leer:
            eintrag['leer'] = leer
            eintrag['stufen'] = len(gesehen)
        if vertrag.get('titleLocKey'):
            eintrag['titel_key'] = vertrag.get('titleLocKey')
        if vertrag.get('descriptionLocKey'):
            eintrag['text_key'] = vertrag.get('descriptionLocKey')
        ergebnis[schluessel] = eintrag
    return ergebnis


def erzeugen(version=None, fortschritt=None, aus_datei=None):
    """Holt die Daten und legt den eigenen Katalog an. Gibt (anzahl, version) zurück.

    `fortschritt` ist eine Funktion für Zwischenmeldungen — das Holen dauert
    ein paar Sekunden, und ein stummes Programm sieht dabei aus wie ein hängendes."""
    def melde(text):
        if fortschritt:
            fortschritt(text)

    version = version or aktuelle_version()
    if not version:
        return 0, ''

    melde(t('z_werte'))
    werte = _werte(_hole('%s/crafting_items-%s.json' % (BASIS, version)))

    if aus_datei:                       # nur für Entwicklung und Selbsttest
        melde(t('z_herkunft_datei') % os.path.basename(aus_datei))
        with open(aus_datei, encoding='utf-8') as f:
            merged = json.load(f)
    else:
        melde(t('z_herkunft_netz'))
        merged = _hole('%s/merged-%s.json' % (BASIS, version))

    melde(t('z_auswerten'))
    pools, quellen = _herkunft(merged)
    topf_namen = {}
    for topf in (merged.get('blueprintPools') or {}).values():
        wie = topf.get('name') or ''
        for b in (topf.get('blueprints') or []):
            if b.get('name'):
                topf_namen.setdefault(_norm(b['name']), []).append(wie)
    namen = {n for liste in pools.values() for n in liste}

    bauplaene = {}
    for name in sorted(namen):
        k = _norm(name)
        eintrag = {'n': name}
        eintrag.update({s: w for s, w in (werte.get(k) or {}).items() if w})
        q = quellen.get(k)
        if q:
            eintrag['q'] = q
        else:
            # Kein Auftrag schüttet ihn aus — aber der Topf hat einen Namen.
            toepfe = sorted({topf_lesbar(t) for t in (topf_namen.get(k) or []) if t})
            if toepfe:
                eintrag['topf'] = ' · '.join(toepfe[:2])
        bauplaene[k] = eintrag

    # Startbaupläne dazu — sie stehen in keinem Belohnungs-Pool und würden
    # sonst fehlen. Vorhandene Einträge werden nicht überschrieben.
    melde(t('z_startbp'))
    for e in _startbauplaene(version):
        k = _norm(e['n'])
        if k in bauplaene:
            bauplaene[k]['start'] = True
        else:
            eintrag = {'n': e['n'], 'start': True}
            if e.get('a'):
                eintrag['a'] = e['a']
            bauplaene[k] = eintrag

    # ---- Was hat dieser Patch gebracht? ----
    #
    # Verglichen wird gegen **alle je gesehenen** Baupläne, nicht gegen den
    # Katalog von letzter Woche. Der Unterschied ist der ganze Grund für
    # `patchhistorie`: Am 26.08.2026 meldete der Vergleich gegen den letzten
    # Katalog 74 Zugänge, von denen 53 längst im Spiel waren — die Quelle hatte
    # sie zwischendurch schlicht nicht geführt.
    #
    # ⚠ Ist noch nichts gesehen worden (erster Katalogbau überhaupt), wird
    # NICHTS als Zugang gewertet — sonst stünden alle 730 Baupläne als „neu" da.
    # Nur die Vergleichsgrundlage wird gesetzt.
    #
    # ⚠ Wer den Watcher schon vor v3.0.0-rc55 benutzt hat, hat einen Katalog,
    # aber noch keine Vergleichsgrundlage — die Datei kam erst mit der
    # Patch-Historie dazu. Ohne diesen Nachzug griffe bei ihm die Regel oben
    # fälschlich, und der **nächste** Patch bliebe stumm: alles gälte als
    # „schon immer da". Der alte Katalog ist die richtige Grundlage — was darin
    # steht, war vor diesem Lauf im Spiel.
    bekannt = _vergleichsgrundlage()
    if bekannt:
        zugang = [e['n'] for k, e in bauplaene.items() if k not in bekannt]
        if zugang:
            patchhistorie.eintragen(version, zugang)
    patchhistorie.gesehen_setzen(bekannt | set(bauplaene))

    # Der Stempel kommt aus der Historie, nicht aus diesem Lauf. Dadurch trägt
    # auch ein frisch gebauter Katalog die Herkunft aller früheren Patches —
    # die mitgelieferte Historie reicht weiter zurück als das eigene Zusehen.
    herkunft = patchhistorie.version_je_bauplan()
    for k, eintrag in bauplaene.items():
        if k in herkunft:
            eintrag['seit'] = herkunft[k]

    daten = {'version': version, 'format': FORMAT,
             'geholt': time.strftime('%Y-%m-%d %H:%M'),
             'bauplaene': bauplaene, 'missionen': _missionen(merged)}
    ziel = pfade.app_datei(CACHE)
    temp = ziel + '.tmp'
    with open(temp, 'w', encoding='utf-8') as f:
        json.dump(daten, f, ensure_ascii=False)
    os.replace(temp, ziel)
    return len(bauplaene), version


# ------------------------------------------------------------------ Lesen
def laden():
    """Der eigene Katalog von der Platte. Fehlt er, ist er leer.

    ⚠ **Die Schlüssel werden beim Laden neu gebildet.** Ein Katalog auf der
    Platte kann Monate alt sein und mit einer früheren Fassung von
    `namensform()` geschrieben worden sein. Genau das lag am 29.08.2026 vor:
    Dort standen Magazine noch als `… magazine (15 cap)`, während der Bestand
    sie längst als `… magazine (15)` führt — die Angleichung der Mengenangabe
    kam erst später dazu.

    Die Folge war eine Zahl, die niemand erklären konnte: Das Overlay meldete
    **405** Baupläne, der Fortschritt **382 von 738**. 23 Magazine und
    Batterien galten überall als fehlend, obwohl sie im Bestand standen.

    Deshalb wird hier nicht mehr darauf vertraut, wie die Schlüssel einmal
    geschrieben wurden — sie werden aus dem Namen neu gebildet.
    """
    try:
        with open(pfade.app_datei(CACHE), encoding='utf-8') as f:
            d = json.load(f)
        if isinstance(d.get('bauplaene'), dict):
            d.setdefault('missionen', {})    # Kataloge vor v2.0.0-rc5
            d['bauplaene'] = _schluessel_angleichen(d['bauplaene'])
            return d
    except Exception:
        pass
    return {'version': '', 'geholt': '', 'bauplaene': {}, 'missionen': {}}


def _schluessel_angleichen(bauplaene):
    """Schlüssel aus dem Namen neu bilden — für Kataloge älterer Fassungen.

    Passt schon alles, wird nichts angefasst: Bei einem frischen Katalog kostet
    das einen Vergleich je Bauplan und keine neue Verzeichnisstruktur.
    """
    umzubauen = False
    for schluessel, e in bauplaene.items():
        name = e.get('n')
        if name and _norm(name) != schluessel:
            umzubauen = True
            break
    if not umzubauen:
        return bauplaene
    neu = {}
    for schluessel, e in bauplaene.items():
        name = e.get('n')
        neu[_norm(name) if name else schluessel] = e
    return neu


def _vergleichsgrundlage():
    """Wogegen dieser Lauf vergleicht, um Zugänge zu erkennen.

    Normalerweise die Liste aller je gesehenen Baupläne. Fehlt sie — jeder, der
    den Watcher vor v3.0.0-rc55 benutzt hat, hat sie nicht —, gilt ersatzweise
    der Katalog, der schon auf der Platte liegt: Was darin steht, war vor diesem
    Lauf im Spiel. Ohne diesen Ersatz griffe beim nächsten Patch die Regel
    „erster Katalogbau überhaupt", und er meldete **keinen einzigen** Zugang.

    Leer ist das Ergebnis nur beim allerersten Katalogbau — dann ist es richtig
    so, sonst stünden alle 738 Baupläne als „neu" da."""
    return patchhistorie.gesehen() or set(laden().get('bauplaene') or {})


def stempel_nachziehen():
    """Fehlende `seit`-Stempel im vorhandenen Katalog nachtragen.

    ⚠ **Warum das nötig ist.** Gestempelt wurde bisher nur beim Neubau des
    Katalogs — und neu gebaut wird nur, wenn eine neue Spielversion kommt. Wer
    seinen Katalog vor v3.0.0-rc55 geholt hat, sitzt deshalb auf 738 Einträgen
    ohne Herkunft, und daran ändert sich bis zum nächsten Patch nichts: Das
    Auswahlfeld zeigte „4.10.0 (21)" — es liest die Historie direkt —, die
    Liste blieb aber leer, weil der Filter gegen den Stempel im Katalog prüft.
    Dasselbe traf „neu im Spiel".

    Der Abgleich kostet nichts und braucht kein Netz: Die Historie liegt beim
    Programm, der Katalog auf der Platte. Geschrieben wird nur, wenn sich
    wirklich etwas ändert.

    Gibt die Zahl der nachgetragenen Stempel zurück."""
    try:
        d = laden()
        bauplaene = d.get('bauplaene') or {}
        if not bauplaene:
            return 0
        herkunft = patchhistorie.version_je_bauplan()
        geaendert = 0
        for k, eintrag in bauplaene.items():
            seit = herkunft.get(k)
            if seit and eintrag.get('seit') != seit:
                eintrag['seit'] = seit
                geaendert += 1
        if not geaendert:
            return 0
        ziel = pfade.app_datei(CACHE)
        temp = ziel + '.tmp'
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump(d, f, ensure_ascii=False)
        os.replace(temp, ziel)
        return geaendert
    except Exception:
        return 0


def aktualisieren(fortschritt=None):
    """Erneuert den Katalog, falls eine neue Spielversion vorliegt.

    Gibt (True, Anzahl, Version) zurück, wenn etwas passiert ist. Wirft nie —
    ohne Netz gilt der letzte Stand, und der Watcher läuft ohne Katalog weiter
    (dann fehlt nur die Liste, nicht die Erkennung)."""
    # ⚠ Ganz am Anfang — vor der Netzsperre und vor dem Netz-Zugriff. Bringt
    # eine neue Programmfassung Historie mit, die der Katalog auf der Platte
    # noch nicht kennt, muss der Stempel auch dann nachkommen, wenn gar keine
    # neue Spielversion ansteht. `AUS` verbietet das **Netz**, nicht die Arbeit:
    # Historie und Katalog liegen beide auf der Platte. Stand die Zeile hinter
    # dem Riegel, blieb die Liste bei abgeschaltetem Netz für immer leer.
    stempel_nachziehen()
    if AUS:
        return False, 0, ''
    try:
        version = aktuelle_version()
        da = laden()
        # Neu gebaut wird bei neuer Spielversion **oder** bei neuem Aufbau
        # (siehe `FORMAT`). Das zweite ist der Grund, warum ein Katalog-Umbau
        # überhaupt bei jemandem ankommt, der schon einen liegen hat.
        # ⚠⚠ **Die Werkstatt-Daten VOR der Abbruchbedingung holen.**
        #
        # Sie standen ursprünglich hinter `erzeugen()` — und wurden damit bei
        # niemandem je geholt, der schon einen aktuellen Katalog hatte. Also
        # bei **allen bisherigen Nutzern**: Der Katalog stimmte, die Funktion
        # kehrte hier um, und Herstellung, Bergbau und Lager blieben dauerhaft
        # leer, bis CIG das nächste Mal patcht.
        #
        # Beide Module bringen ihre **eigene** Prüfung mit (`schon aktuell?`)
        # und laden nichts doppelt. Sie hängen nicht am Katalog-Format, sondern
        # nur an der Spielversion.
        #
        # Jeweils **eigenes `try`**: Scheitert eines, soll der Katalog trotzdem
        # durchlaufen — eine leere Seite, die das sagt, ist besser als ein
        # verlorener Abruf.
        try:
            from . import herstellung
            herstellung.aktualisieren(version, fortschritt)
        except Exception as ausnahme:
            fehler.merken('katalog.aktualisieren.herstellung', ausnahme)
        try:
            from . import mining
            mining.update(version, fortschritt)
        except Exception as ausnahme:
            fehler.merken('katalog.aktualisieren.bergbau', ausnahme)

        if not version or (version == da.get('version')
                           and da.get('format') == FORMAT):
            return False, 0, version or ''
        anzahl, version = erzeugen(version, fortschritt)
        return bool(anzahl), anzahl, version
    except Exception:
        return False, 0, ''


def version_kurz(version):
    """Aus '4.10.0-live.12519617' wird '4.10.0'.

    Die volle Kennung ist eindeutig und wird deshalb gespeichert; im Auswahlfeld
    hat sie nichts verloren — dort will man „4.10.0" lesen, nicht die Buildnummer."""
    return (version or '').split('-')[0] or (version or '')


def patches(daten=None):
    """[(volle Version, kurze Version, Anzahl), …] — neueste zuerst.

    Alle Spielversionen, aus denen im Katalog Baupläne stammen. Grundlage ist
    derselbe Stempel `seit`, den auch `neue()` benutzt: Kommt ein Patch dazu,
    taucht seine Version hier von allein auf — es gibt keine gepflegte Liste,
    die man vergessen könnte.

    ⚠ **Gezählt wird der Katalog, nicht die Historie.** Hier stand ein
    `return patchhistorie.patches()`, und damit versprach das Auswahlfeld etwas,
    das die Liste darunter nicht einlösen konnte: Am 28.08.2026 stand im Feld
    „4.10.0 (24)" und darunter drei Zeilen. Das Feld las die Historie, der
    Filter prüft den Stempel im Katalog — zwei Quellen für dieselbe Frage, und
    die Zahl in Klammern ist genau die Zusage, wie viele Zeilen kommen.

    Dass die Historie mehr weiß, hilft dabei nicht: Was nicht gestempelt ist,
    kann die Liste nicht zeigen. Dafür sorgt `stempel_nachziehen()`, und zwar
    bevor das Fenster den Katalog liest.

    Vor dem zweiten Katalogbau ist die Liste leer: Ohne Vorgänger wird nichts
    gestempelt, und ein Feld mit einem einzigen Eintrag hilft niemandem."""
    zaehler = {}
    for e in ((daten or laden()).get('bauplaene') or {}).values():
        seit = e.get('seit')
        if seit:
            zaehler[seit] = zaehler.get(seit, 0) + 1
    return [(v, version_kurz(v), zaehler[v])
            for v in sorted(zaehler, key=patchhistorie.rang, reverse=True)]


def neue(daten=None):
    """Die Baupläne, die mit der **zuletzt geholten** Spielversion dazukamen.

    Grundlage ist der Stempel `seit`, den `erzeugen()` setzt. Gezeigt wird nur,
    was zur aktuellen Katalogversion passt — ältere Stempel bleiben zwar in der
    Datei stehen (sie sagen, mit welchem Patch es einen Bauplan gibt), gehören
    aber nicht mehr unter „neu im Spiel".

    Leer ist das Ergebnis, solange der Katalog erst einmal gebaut wurde: Ohne
    Vorgänger gibt es keine Differenz."""
    d = daten or laden()
    version = d.get('version') or ''
    if not version:
        return set()
    return {k for k, e in d['bauplaene'].items() if e.get('seit') == version}


def startbauplaene(daten=None):
    """Die Vergleichsformen der Baupläne, die jeder von Anfang an hat."""
    return {k for k, e in (daten or laden())['bauplaene'].items() if e.get('start')}


def namen(daten=None):
    """Alle Bauplan-Namen in Vergleichsform."""
    return set((daten or laden())['bauplaene'])


def nach_art(daten=None):
    """Baupläne nach Art gruppiert: {'Cooler': [Eintrag, …], …}."""
    gruppen = {}
    for k, e in (daten or laden())['bauplaene'].items():
        gruppen.setdefault(art_lesbar(e.get('a')), []).append(e)
    for liste in gruppen.values():
        liste.sort(key=lambda e: e['n'].lower())
    return gruppen


# ------------------------------------------------------- Obergruppen
# Alphabetisch sortiert stand „Andockkragen" vor „Schiffswaffe" und die Rüstung
# mittendrin — 25 Kategorien in Buchstabenreihenfolge sind kein Überblick.
# Wer sucht, denkt in Bereichen: erst das Schiff, dann was man am Mann trägt.
OBERGRUPPEN = ('schiff', 'fps', 'ruestung', 'sonstiges')

ART_GRUPPE = {
    # Alles, was am Schiff verbaut wird
    'Cooler': 'schiff', 'PowerPlant': 'schiff', 'QuantumDrive': 'schiff',
    'Shield': 'schiff', 'Radar': 'schiff', 'WeaponGun': 'schiff',
    'WeaponMining': 'schiff', 'SalvageModifier': 'schiff',
    'SalvageHead': 'schiff', 'TractorBeam': 'schiff',
    'DockingCollar': 'schiff', 'Cargo': 'schiff',
    # Was man in die Hand nimmt
    'WeaponPersonal': 'fps', 'WeaponAttachment': 'fps',
    # ⚠ scmdb führt einige Einträge unter kleingeschriebenen Sammelbegriffen
    # statt unter der sonst üblichen Kennung. Ohne diese vier Zeilen landeten
    # die S-38 Pistol und das P4-AR Rifle unter „Sonstiges", während der
    # Filter „nur FPS-Waffen" nichts anzeigte — dasselbe für den Field Recon
    # Suit unter „Rüstung". Betroffen sind 10 der 722 Baupläne; wer nur auf
    # die Gesamtzahl sieht, merkt davon nichts.
    # Beide fallen inzwischen über `ART_ZUSAMMEN` mit ihrer richtigen Kennung
    # zusammen; die Zeile bleibt für den Fall, dass `obergruppe()` einmal eine
    # rohe Kennung bekommt, die nicht durch `art_kennung()` gelaufen ist.
    'weapons': 'fps', 'ammo': 'fps',
    # Was man am Körper trägt
    'Char_Armor_Helmet': 'ruestung', 'Char_Armor_Torso': 'ruestung',
    'Char_Armor_Legs': 'ruestung', 'Char_Armor_Arms': 'ruestung',
    'Char_Armor_Backpack': 'ruestung', 'Char_Armor_Undersuit': 'ruestung',
    'Char_Clothing_Torso_0': 'ruestung', 'Char_Clothing_Torso_1': 'ruestung',
    'Char_Clothing_Legs': 'ruestung', 'Char_Clothing_Feet': 'ruestung',
    'armour': 'ruestung',
}


# Zusätzliche Suchwörter je Art. Vier Kategorien heißen bewusst englisch, weil
# das Spiel sie so nennt — „Cooler", „Power Plant", „Quantum Drive", „Radar".
# Wer deutsch denkt, tippt trotzdem „Kühler" und findet dann nichts. Beides
# soll gehen, ohne die im Spiel gebräuchliche Beschriftung zu ändern.
ART_SUCHWORTE = {
    'Cooler':        ('kühler', 'kuehler', 'kuhler'),
    'PowerPlant':    ('generator', 'kraftwerk', 'energie'),
    'QuantumDrive':  ('sprungantrieb', 'quantenantrieb', 'qd'),
    'Radar':         ('scanner', 'ortung'),
    'Shield':        ('schild', 'schilde'),
    'WeaponGun':     ('kanone', 'geschütz', 'geschuetz'),
    'WeaponPersonal': ('gewehr', 'pistole', 'fps'),
    'TractorBeam':   ('traktor', 'schlepper'),
    'Cargo':         ('fracht', 'ladung'),
}


def suchworte(roh):
    """Weitere Begriffe, unter denen diese Art gefunden werden soll."""
    return ART_SUCHWORTE.get(roh, ())


def obergruppe(roh):
    """Zu welchem Bereich gehört diese Art? Unbekanntes landet bei 'sonstiges'."""
    return ART_GRUPPE.get(roh, 'sonstiges')


def gruppen_geordnet(daten=None):
    """[(obergruppe, art_lesbar, [Einträge]), …] — in der Reihenfolge, in der
    sie angezeigt werden sollen.

    Innerhalb eines Bereichs alphabetisch: Das ist vorhersagbar, und eine
    „sinnvolle" Reihenfolge innerhalb der Schiffsteile hätte jeder anders
    im Kopf."""
    gruppen = {}
    for e in (daten or laden())['bauplaene'].values():
        roh = art_kennung(e)
        gruppen.setdefault((obergruppe(roh), art_lesbar(roh)), []).append(e)
    for liste in gruppen.values():
        liste.sort(key=lambda e: e['n'].lower())
    return [(og, art, gruppen[(og, art)])
            for og, art in sorted(gruppen,
                                  key=lambda p: (OBERGRUPPEN.index(p[0]),
                                                 p[1].lower()))]


if __name__ == '__main__':
    import sys
    d = laden()
    if '--holen' in sys.argv or not d['bauplaene']:
        datei = next((a.split('=', 1)[1] for a in sys.argv
                      if a.startswith('--datei=')), None)
        n, v = erzeugen(fortschritt=lambda t: print(' ', t), aus_datei=datei)
        print('Katalog angelegt: %d Baupläne, Version %s' % (n, v))
        d = laden()
    mit = sum(1 for e in d['bauplaene'].values() if e.get('q'))
    print('Version %s · %d Baupläne · %d mit Herkunft'
          % (d['version'], len(d['bauplaene']), mit))
    print('Datei  :', pfade.app_datei(CACHE),
          '(%.0f KB)' % (os.path.getsize(pfade.app_datei(CACHE)) / 1024))
    for art, liste in sorted(nach_art(d).items(), key=lambda x: -len(x[1]))[:8]:
        print('   %4d  %s' % (len(liste), art))
