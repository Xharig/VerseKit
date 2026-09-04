# SPDX-License-Identifier: GPL-3.0-only
#
# SC BP Watcher — CryXmlB lesen
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
Binaeres CryEngine-XML lesen — mit Bordmitteln.

## Wozu

Viele Dateien im `Data.p4k` sehen aus wie XML, sind aber **keins**. Sie
beginnen mit `CryXmlB\\0` und sind ein binaeres Format, das die CryEngine
selbst schreibt. `xml.etree` steigt dort sofort aus.

Gebraucht wird es fuer `Data/Libs/Config/defaultProfile.xml`: Dort steht, wie
die Aktionen des Spiels **heissen** — `v_eject` traegt das Etikett
`@ui_CIEject`, das wiederum in der `global.ini` zu „Aussteigen" bzw. „Eject"
wird. Ohne diese Datei blieben in der Belegungsliste die technischen Namen
stehen.

## Das Format — an der echten Datei vermessen (04.09.2026)

Der Kopf ist 44 Bytes: acht Bytes Signatur, danach neun 32-Bit-Zahlen.

| Ab | Was |
|---|---|
| 0 | `CryXmlB\\0` |
| 8 | Gesamtlaenge, dann je Tabelle **Offset und Anzahl** |

Daraus ergeben sich vier Tabellen mit fester Satzlaenge:

| Tabelle | Satz | Inhalt |
|---|---|---|
| Knoten | **28 Bytes** | Name, Inhalt, Zahl der Attribute und Kinder, Elternteil, erstes Attribut, erstes Kind, ungenutzt |
| Kinder | **4 Bytes** | Verweis auf einen Knoten |
| Attribute | **8 Bytes** | zwei Verweise: Schluessel und Wert |
| Texte | — | mit Null-Byte getrennte Zeichenketten, alle Verweise oben sind Abstaende hierin |

⚠ **Die Satzlaengen werden nicht geraten, sondern nachgerechnet**: Der Abstand
zwischen zwei Tabellenanfaengen geteilt durch die Anzahl muss aufgehen. Passt
es nicht, bricht der Leser ab, statt Unsinn zu liefern — ein Format, das sich
mit einem Patch aendert, faellt so sofort auf und nicht erst in der Anzeige.

⚠ **Die Tabellen stehen nicht in der Reihenfolge, in der sie im Kopf genannt
werden.** In der gemessenen Datei kam die Kinder- vor der Attributtabelle.
Deshalb wird nach Offset sortiert, bevor Satzlaengen gerechnet werden.

## Was dieser Leser bewusst NICHT ist

Kein vollstaendiger XML-Ersatz. Er liefert eine schlichte Baumstruktur aus
Woerterbuechern — genug, um Knoten eines Namens zu finden und ihre Attribute
zu lesen. Verschachtelte Textinhalte, Namensraeume, Kommentare: nicht
vorgesehen, weil nicht gebraucht.
"""
import struct

SIGNATUR = b'CryXmlB\0'


class NichtCryXml(Exception):
    """Die Daten sind kein CryXmlB — der Aufrufer soll es als XML versuchen."""


class Kaputt(Exception):
    """Die Struktur passt nicht zusammen — lieber abbrechen als raten."""


def ist_cryxml(daten):
    """Faengt die Datei mit der Signatur an?"""
    return bool(daten) and daten[:8] == SIGNATUR


def lesen(daten):
    """Den Baum aufmachen. Liefert den Wurzelknoten.

    Ein Knoten ist ein Woerterbuch:

        {'name': 'action', 'attribute': {…}, 'kinder': [ … ], 'inhalt': ''}
    """
    # ⚠ Die Meldungen der Ausnahmen sind knapp und technisch gehalten: Sie
    # landen im Fehlerprotokoll, nie auf dem Bildschirm. Ganze deutsche Saetze
    # wuerden hier die Zweisprachigkeits-Wache (Pruefung 17) ausloesen, die
    # nicht unterscheiden kann, was sichtbar wird und was nicht.
    if not ist_cryxml(daten):
        raise NichtCryXml('signature')
    if len(daten) < 44:
        raise Kaputt('header')

    (laenge, n_off, n_zahl, a_off, a_zahl, k_off, k_zahl,
     s_off, _s_gr) = struct.unpack('<9I', daten[8:44])
    if laenge != len(daten):
        # Nur ein Hinweis, kein Abbruch: Ein Block darf hinten aufgefuellt sein.
        pass
    if not n_zahl or s_off >= len(daten):
        raise Kaputt('tables')

    # ⚠ Satzlaengen nachrechnen statt annehmen — siehe Kopf des Moduls.
    grenzen = sorted([(n_off, 'knoten', n_zahl),
                      (k_off, 'kinder', k_zahl),
                      (a_off, 'attribute', a_zahl),
                      (s_off, 'texte', 0)])
    breite = {}
    for i, (off, name, zahl) in enumerate(grenzen):
        if name == 'texte':
            continue
        ende = grenzen[i + 1][0]
        if zahl <= 0 or ende <= off:
            raise Kaputt('empty table: %s' % name)
        rest = (ende - off) % zahl
        if rest:
            raise Kaputt('record size: %s' % name)
        breite[name] = (ende - off) // zahl
    if breite['knoten'] < 28 or breite['attribute'] < 8 or breite['kinder'] < 4:
        raise Kaputt('record sizes %r' % (breite,))

    def text(abstand):
        """Eine Zeichenkette aus der Texttabelle."""
        anfang = s_off + abstand
        if anfang >= len(daten):
            return ''
        ende = daten.find(b'\0', anfang)
        if ende < 0:
            ende = len(daten)
        return daten[anfang:ende].decode('utf-8', 'replace')

    # Erst alle Knoten flach einlesen, dann verketten. Andersherum muesste man
    # rekursiv springen — bei 1531 Knoten unnoetig und anfaellig fuer Zyklen.
    roh = []
    for i in range(n_zahl):
        b = n_off + i * breite['knoten']
        (nm, inh, a_anz, k_anz, _eltern,
         a_erst, k_erst, _res) = struct.unpack('<IIHHIIII', daten[b:b + 28])
        attribute = {}
        for j in range(a_anz):
            ab = a_off + (a_erst + j) * breite['attribute']
            if ab + 8 > len(daten):
                break
            sk, sv = struct.unpack('<II', daten[ab:ab + 8])
            attribute[text(sk)] = text(sv)
        roh.append({'name': text(nm), 'inhalt': text(inh),
                    'attribute': attribute, 'kinder': [],
                    '_k_anz': k_anz, '_k_erst': k_erst})

    for knoten in roh:
        for j in range(knoten.pop('_k_anz')):
            kb = k_off + (knoten['_k_erst'] + j) * breite['kinder']
            if kb + 4 > len(daten):
                break
            (ziel,) = struct.unpack('<I', daten[kb:kb + 4])
            if 0 <= ziel < len(roh):
                knoten['kinder'].append(roh[ziel])
        knoten.pop('_k_erst')

    return roh[0]


def alle(knoten, name):
    """Jeden Knoten dieses Namens im Baum, in Dokumentreihenfolge.

    Bewusst iterativ: Die Spieldateien sind flach genug, aber eine kaputte
    Datei mit Zyklus wuerde eine rekursive Suche in den Abgrund schicken.
    """
    stapel, heraus, gesehen = [knoten], [], set()
    while stapel:
        k = stapel.pop()
        if id(k) in gesehen:
            continue
        gesehen.add(id(k))
        if k.get('name') == name:
            heraus.append(k)
        stapel.extend(reversed(k.get('kinder') or []))
    return heraus
