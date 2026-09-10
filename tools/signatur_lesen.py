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
Die Scan-Signatur vom Bildschirm lesen — als Werkzeug, noch nicht im Programm.

**Wozu.** Der Scanner im Spiel zeigt eine Zahl; welcher Brocken dahintersteckt,
sagt er nicht. `scbp.bergbau.signatur_suchen()` beantwortet das längst — nur
muss die Zahl bisher von Hand eingetippt werden. Ein Nutzer dazu: „wieso kann
ein Funk-Plugin das, mein Tool mit SC-Bezug nicht?" Das Argument sitzt.

**Warum zuerst ein Werkzeug und nicht gleich der Einbau.** Die Ziffernerkennung
ist das große Stück; wenn sie wackelt, ist alles andere umsonst gebaut. Als
eigenes Werkzeug lässt sie sich gegen echte Bilder prüfen, ohne das Programm
anzufassen — dieselbe Linie wie bei `wackelt.py` und `bilder_machen.py`.

**Kein Fremdpaket, kein Tesseract.** Der Bildabgriff läuft über `ctypes` und
`libX11` (Linux) beziehungsweise GDI (Windows, noch nicht gebaut). Otsu-Schwelle,
Flächensuche, Normierung und Vergleich sind je ein paar Dutzend Zeilen reines
Python. Tesseract wäre nur das Anlern-Werkzeug gewesen und liefert auf dieser
Schrift ohnehin Müll — angelernt wird stattdessen von Hand, einmal.

**Ablauf**

    python3 tools/signatur_lesen.py --zeigen
        Sucht das SC-Fenster, greift den Signatur-Bereich ab und legt ihn als
        PNG ab, damit man sieht, was das Werkzeug sieht.

    python3 tools/signatur_lesen.py --anlernen 11700
        Nimmt den aktuellen Bildschirminhalt, trennt die Ziffern und ordnet sie
        der getippten Zahl der Reihe nach zu. Passt die Anzahl nicht, wird
        abgelehnt statt geraten. Ergebnis landet in der Vorlagen-Datei.

    python3 tools/signatur_lesen.py
        Liest die Zahl mit den vorhandenen Vorlagen und sagt, wie sicher.

⚠ **Dieses Werkzeug öffnet kein Fenster.** Es liest nur und schreibt PNG-Dateien
— deshalb braucht es `unsichtbar.sicherstellen()` nicht. Wer hier je eine
Oberfläche einbaut, muss den Aufruf nachziehen.

⚠ **Stand: Linux/X11.** Unter nativem Wayland hat Star Citizen kein X-Fenster
und der Abgriff ist unmöglich — gemessen am 08.09.2026. Wine muss im X11-Modus
laufen (`WAYLAND_DISPLAY=` leer), und die Bildschirm-Skalierung muss glatt sein,
sonst zielt man im Spiel daneben. Der Windows-Weg über GDI fehlt noch.
"""
import argparse
import ctypes
import json
import os
import struct
import sys
import zlib

# Die Ziffern werden vor dem Vergleich auf eine feste Größe gebracht. Damit
# spielt es keine Rolle mehr, ob die Zahl auf 1080p oder auf einem Ultrawide
# steht — verglichen wird immer dasselbe Raster.
NORM_B, NORM_H = 16, 24

# ⚠ Wie erkennt man, dass gar keine Zahl da ist? Der erste Versuch nahm die
# Spanne (hellster minus dunkelster Punkt) — untauglich: Ein einzelner Stern im
# schwarzen Weltraum ergibt Spanne 191, und das Werkzeug meldete „Text da" auf
# einem leeren Bild (09.09.2026 gemessen). Gezaehlt wird deshalb, wie VIELE
# Punkte ueber der Schwelle liegen. Eine fuenfstellige Zahl bringt einige
# hundert; darunter ist nichts da, was sich zu lesen lohnt.
MINDEST_HELLE_PUNKTE = 60

VORLAGEN_DATEI = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                              'signatur-ziffern.json')


# --------------------------------------------------------------------------
# Bild holen
# --------------------------------------------------------------------------

class _XImage(ctypes.Structure):
    _fields_ = [('width', ctypes.c_int), ('height', ctypes.c_int),
                ('xoffset', ctypes.c_int), ('format', ctypes.c_int),
                ('data', ctypes.c_void_p),
                ('byte_order', ctypes.c_int), ('bitmap_unit', ctypes.c_int),
                ('bitmap_bit_order', ctypes.c_int),
                ('bitmap_pad', ctypes.c_int), ('depth', ctypes.c_int),
                ('bytes_per_line', ctypes.c_int),
                ('bits_per_pixel', ctypes.c_int),
                ('red_mask', ctypes.c_ulong), ('green_mask', ctypes.c_ulong),
                ('blue_mask', ctypes.c_ulong)]


class _XWA(ctypes.Structure):
    _fields_ = [('x', ctypes.c_int), ('y', ctypes.c_int),
                ('width', ctypes.c_int), ('height', ctypes.c_int),
                ('border_width', ctypes.c_int), ('depth', ctypes.c_int),
                ('visual', ctypes.c_void_p), ('root', ctypes.c_ulong),
                ('class_', ctypes.c_int), ('bit_gravity', ctypes.c_int),
                ('win_gravity', ctypes.c_int), ('backing_store', ctypes.c_int),
                ('backing_planes', ctypes.c_ulong),
                ('backing_pixel', ctypes.c_ulong),
                ('save_under', ctypes.c_int), ('colormap', ctypes.c_ulong),
                ('map_installed', ctypes.c_int), ('map_state', ctypes.c_int),
                ('all_event_masks', ctypes.c_long),
                ('your_event_mask', ctypes.c_long),
                ('do_not_propagate_mask', ctypes.c_long),
                ('override_redirect', ctypes.c_int),
                ('screen', ctypes.c_void_p)]


def _x11():
    """Die X-Bibliothek mit gesetzten Signaturen — oder None."""
    try:
        lib = ctypes.CDLL('libX11.so.6')
    except OSError:
        return None
    lib.XOpenDisplay.restype = ctypes.c_void_p
    lib.XDefaultRootWindow.restype = ctypes.c_ulong
    lib.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
    lib.XGetImage.restype = ctypes.POINTER(_XImage)
    lib.XGetImage.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int,
                              ctypes.c_int, ctypes.c_uint, ctypes.c_uint,
                              ctypes.c_ulong, ctypes.c_int]
    lib.XQueryTree.argtypes = [
        ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong),
        ctypes.POINTER(ctypes.c_ulong),
        ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)),
        ctypes.POINTER(ctypes.c_uint)]
    lib.XGetWindowAttributes.argtypes = [ctypes.c_void_p, ctypes.c_ulong,
                                         ctypes.POINTER(_XWA)]
    lib.XFetchName.argtypes = [ctypes.c_void_p, ctypes.c_ulong,
                               ctypes.POINTER(ctypes.c_char_p)]
    lib.XFree.argtypes = [ctypes.c_void_p]
    return lib


def spielfenster(lib, anzeige):
    """Das Star-Citizen-Fenster suchen. Gibt (Kennung, Breite, Hoehe) oder None.

    ⚠ Gesucht wird über den Fensternamen, nicht über den Prozess: Unter Wine
    gehoert das sichtbare Fenster nicht zwangslaeufig dem Prozess, den man
    erwartet. Der Name ist stabil ('Star Citizen').
    """
    wurzel = lib.XDefaultRootWindow(anzeige)

    def kinder(von, tiefe=0):
        if tiefe > 3:
            return
        a, b = ctypes.c_ulong(), ctypes.c_ulong()
        liste = ctypes.POINTER(ctypes.c_ulong)()
        anzahl = ctypes.c_uint()
        if not lib.XQueryTree(anzeige, von, ctypes.byref(a), ctypes.byref(b),
                              ctypes.byref(liste), ctypes.byref(anzahl)):
            return
        for i in range(anzahl.value):
            yield liste[i]
            for tiefer in kinder(liste[i], tiefe + 1):
                yield tiefer

    for fenster in kinder(wurzel):
        merkmale = _XWA()
        if not lib.XGetWindowAttributes(anzeige, fenster,
                                        ctypes.byref(merkmale)):
            continue
        # IsViewable = 2. Tiefe 24 schliesst das Wurzelfenster aus, das unter
        # XWayland zwar gemeldet wird, seinen Inhalt aber nie herausgibt.
        if merkmale.map_state != 2 or merkmale.depth != 24:
            continue
        if merkmale.width < 800:
            continue
        zeiger = ctypes.c_char_p()
        name = ''
        if lib.XFetchName(anzeige, fenster, ctypes.byref(zeiger)) \
                and zeiger.value:
            name = zeiger.value.decode('utf-8', 'replace')
            lib.XFree(zeiger)
        if 'star citizen' in name.lower():
            return fenster, merkmale.width, merkmale.height
    return None


def abgreifen(links, oben, breite, hoehe):
    """Einen Ausschnitt des Spielfensters als Graustufen-Raster holen.

    Gibt (raster, breite, hoehe) — raster ist eine Liste von Zeilen, jede
    Zeile eine Liste von Helligkeiten 0..255. Oder None, wenn nichts geht.
    """
    if sys.platform.startswith('win'):
        # ⚠ Noch nicht gebaut. Der Weg ist unstrittig (ctypes/GDI, wie es
        # `scbp/ablagesymbol.py` schon tut) — er fehlt hier nur.
        return None
    lib = _x11()
    if lib is None:
        return None
    anzeige = lib.XOpenDisplay(None)
    if not anzeige:
        return None
    anzeige = ctypes.c_void_p(anzeige)
    gefunden = spielfenster(lib, anzeige)
    if gefunden is None:
        return None
    fenster = gefunden[0]
    bild = lib.XGetImage(anzeige, fenster, links, oben, breite, hoehe,
                         ctypes.c_ulong(0xFFFFFFFF), 2)   # 2 = ZPixmap
    if not bild:
        return None
    b = bild.contents
    je = max(1, b.bits_per_pixel // 8)
    roh = ctypes.cast(b.data, ctypes.POINTER(
        ctypes.c_ubyte * (b.bytes_per_line * b.height))).contents
    raster = []
    for y in range(b.height):
        grund = y * b.bytes_per_line
        zeile = []
        for x in range(b.width):
            s = grund + x * je
            if s + 2 < len(roh):
                # Graustufe nach Augenempfindlichkeit — eine reine Mittelung
                # verschluckt gruenen Text auf blauem Grund, und genau so
                # sieht das SC-HUD aus.
                zeile.append((roh[s + 2] * 299 + roh[s + 1] * 587
                              + roh[s] * 114) // 1000)
            else:
                zeile.append(0)
        raster.append(zeile)
    lib.XDestroyImage(bild)
    return raster, b.width, b.height


# --------------------------------------------------------------------------
# PNG lesen
# --------------------------------------------------------------------------

def png_lesen(pfad):
    """Ein PNG als Graustufen-Raster einlesen. Reine Standardbibliothek.

    ⚠⚠ **Warum das sein muss, obwohl das Spiel die Bilder liefert.** Ohne
    Dateien als Quelle koennte der Selbsttest die Erkennung nie pruefen — er
    haette ja kein Bild. Eine Pruefung, die ein laufendes Star Citizen
    voraussetzt, laeuft in Wahrheit nie und ueberspringt sich still. Mit
    abgelegten Bildern wird aus der Erkennung etwas Pruefbares.

    Unterstuetzt Farbtyp 0 (Graustufe) und 2 (RGB) mit 8 Bit, samt aller fuenf
    Zeilenfilter — damit reicht es fuer eigene Ausschnitte und fuer
    Bildschirmfotos, die jemand beisteuert. Alles andere (Palette, 16 Bit,
    Transparenz) waere Mehraufwand ohne Anlass.

    Gibt (raster, breite, hoehe) oder None.
    """
    try:
        with open(pfad, 'rb') as f:
            roh = f.read()
    except OSError:
        return None
    if not roh.startswith(b'\x89PNG\r\n\x1a\n'):
        return None
    stelle, kopf, teile = 8, None, []
    while stelle + 8 <= len(roh):
        laenge = struct.unpack('>I', roh[stelle:stelle + 4])[0]
        art = roh[stelle + 4:stelle + 8]
        inhalt = roh[stelle + 8:stelle + 8 + laenge]
        if art == b'IHDR':
            kopf = struct.unpack('>IIBBBBB', inhalt)
        elif art == b'IDAT':
            teile.append(inhalt)
        elif art == b'IEND':
            break
        stelle += 12 + laenge
    if kopf is None or not teile:
        return None
    breite, hoehe, tiefe, farbe, _k, _f, verschraenkt = kopf
    if tiefe != 8 or verschraenkt or farbe not in (0, 2):
        return None
    je = 1 if farbe == 0 else 3
    try:
        daten = zlib.decompress(b''.join(teile))
    except zlib.error:
        return None

    zeilenlaenge = breite * je
    vorherige = bytearray(zeilenlaenge)
    raster = []
    ort = 0
    for _y in range(hoehe):
        if ort >= len(daten):
            return None
        filter_art = daten[ort]
        ort += 1
        zeile = bytearray(daten[ort:ort + zeilenlaenge])
        if len(zeile) < zeilenlaenge:
            return None
        ort += zeilenlaenge
        # Die fuenf Filter der PNG-Norm rueckgaengig machen. `links` ist der
        # Wert je Kanal einen Punkt weiter vorn, `oben` derselbe Punkt der
        # Zeile darueber.
        for i in range(zeilenlaenge):
            links = zeile[i - je] if i >= je else 0
            oben = vorherige[i]
            schraeg = vorherige[i - je] if i >= je else 0
            if filter_art == 1:
                zeile[i] = (zeile[i] + links) & 0xFF
            elif filter_art == 2:
                zeile[i] = (zeile[i] + oben) & 0xFF
            elif filter_art == 3:
                zeile[i] = (zeile[i] + (links + oben) // 2) & 0xFF
            elif filter_art == 4:
                p = links + oben - schraeg
                pa, pb, pc = abs(p - links), abs(p - oben), abs(p - schraeg)
                nah = links if (pa <= pb and pa <= pc) else (
                    oben if pb <= pc else schraeg)
                zeile[i] = (zeile[i] + nah) & 0xFF
        vorherige = zeile
        if farbe == 0:
            raster.append(list(zeile))
        else:
            raster.append([(zeile[x * 3] * 299 + zeile[x * 3 + 1] * 587
                            + zeile[x * 3 + 2] * 114) // 1000
                           for x in range(breite)])
    return raster, breite, hoehe


# --------------------------------------------------------------------------
# Bildrechnerei
# --------------------------------------------------------------------------

def otsu(raster):
    """Die Trennlinie zwischen Text und Grund selbst bestimmen.

    Nach Otsu: Diejenige Schwelle waehlen, bei der die beiden entstehenden
    Gruppen in sich am gleichfoermigsten sind. Ein fester Wert ginge nicht —
    das HUD ist mal vor Weltraum, mal vor einem beleuchteten Asteroiden.
    """
    haeufig = [0] * 256
    for zeile in raster:
        for punkt in zeile:
            haeufig[punkt] += 1
    gesamt = sum(haeufig)
    if not gesamt:
        return 128
    summe = sum(i * haeufig[i] for i in range(256))
    summe_a, gewicht_a, bestes, schwelle = 0.0, 0, -1.0, 128
    for i in range(256):
        gewicht_a += haeufig[i]
        if not gewicht_a:
            continue
        gewicht_b = gesamt - gewicht_a
        if not gewicht_b:
            break
        summe_a += i * haeufig[i]
        mittel_a = summe_a / gewicht_a
        mittel_b = (summe - summe_a) / gewicht_b
        # Streuung ZWISCHEN den Gruppen — die soll moeglichst gross sein.
        guete = gewicht_a * gewicht_b * (mittel_a - mittel_b) ** 2
        if guete > bestes:
            bestes, schwelle = guete, i
    return schwelle


def zeichen_trennen(raster, schwelle):
    """Zusammenhaengende helle Flaechen finden — das sind die Ziffern.

    Gibt sie von links nach rechts sortiert zurueck, je als
    (links, oben, rechts, unten). Flaechenfuellung ueber einen eigenen Stapel
    statt Rekursion: Bei einer breiten Ziffer reicht Pythons Rekursionstiefe
    sonst nicht.

    ⚠ Das Ortungssymbol vor der Zahl ist auch eine helle Flaeche. Es wird hier
    NICHT aussortiert — das passiert erst beim Zuordnen, wo die Anzahl der
    Flaechen gegen die Anzahl der Ziffern geprueft wird.
    """
    hoehe, breite = len(raster), len(raster[0]) if raster else 0
    gesehen = [[False] * breite for _ in range(hoehe)]
    flaechen = []
    for y in range(hoehe):
        for x in range(breite):
            if gesehen[y][x] or raster[y][x] <= schwelle:
                continue
            stapel = [(x, y)]
            gesehen[y][x] = True
            links = rechts = x
            oben = unten = y
            while stapel:
                px, py = stapel.pop()
                if px < links:
                    links = px
                if px > rechts:
                    rechts = px
                if py < oben:
                    oben = py
                if py > unten:
                    unten = py
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1),
                               (1, 1), (1, -1), (-1, 1), (-1, -1)):
                    nx, ny = px + dx, py + dy
                    if 0 <= nx < breite and 0 <= ny < hoehe \
                            and not gesehen[ny][nx] \
                            and raster[ny][nx] > schwelle:
                        gesehen[ny][nx] = True
                        stapel.append((nx, ny))
            # Einzelne helle Punkte sind Bildrauschen, keine Ziffer.
            if (rechts - links) >= 1 and (unten - oben) >= 3:
                flaechen.append((links, oben, rechts, unten))
    flaechen.sort(key=lambda f: f[0])
    return flaechen


def normieren(raster, kasten, schwelle):
    """Eine gefundene Flaeche auf das feste Raster bringen.

    Das Seitenverhaeltnis bleibt erhalten, der Rest wird mit Grund aufgefuellt
    — sonst wuerde aus einer schmalen `1` eine breite Flaeche und der Vergleich
    mit einer `0` faende faelschlich Aehnlichkeit.

    Gibt eine flache Liste aus 0 und 1, Laenge NORM_B * NORM_H.
    """
    links, oben, rechts, unten = kasten
    qb, qh = rechts - links + 1, unten - oben + 1
    faktor = min(NORM_B / float(qb), NORM_H / float(qh))
    zb, zh = max(1, int(qb * faktor)), max(1, int(qh * faktor))
    rand_x, rand_y = (NORM_B - zb) // 2, (NORM_H - zh) // 2
    ergebnis = [0] * (NORM_B * NORM_H)
    for y in range(zh):
        # Naechster Nachbar genuegt: Die Vorlage ist ohnehin nur 16x24 gross,
        # eine Mittelung braechte hier keine zusaetzliche Sicherheit.
        qy = oben + min(qh - 1, int(y / faktor))
        for x in range(zb):
            qx = links + min(qb - 1, int(x / faktor))
            if raster[qy][qx] > schwelle:
                ergebnis[(rand_y + y) * NORM_B + (rand_x + x)] = 1
    return ergebnis


def abstand(a, b):
    """Wie unaehnlich zwei normierte Zeichen sind — 0.0 heisst gleich."""
    falsch = sum(1 for i in range(len(a)) if a[i] != b[i])
    return falsch / float(len(a))


# --------------------------------------------------------------------------
# Vorlagen
# --------------------------------------------------------------------------

def vorlagen_laden():
    try:
        with open(VORLAGEN_DATEI, encoding='utf-8') as f:
            daten = json.load(f)
        if daten.get('format') == 1 and isinstance(daten.get('ziffern'), dict):
            return daten['ziffern']
    except Exception:
        pass
    return {}


def vorlagen_sichern(ziffern):
    with open(VORLAGEN_DATEI, 'w', encoding='utf-8') as f:
        json.dump({'format': 1, 'raster': [NORM_B, NORM_H],
                   'ziffern': ziffern}, f, ensure_ascii=False, indent=1)


def anlernen(zeichen, text, ziffern):
    """Gefundene Zeichen der getippten Zahl zuordnen.

    ⚠ Wird NICHT geraten: Stimmt die Anzahl der Flaechen nicht mit der Anzahl
    der Zeichen ueberein, wird abgelehnt. Eine falsch zugeordnete Vorlage
    vergiftet danach jede Erkennung, und man sieht ihr das nicht an.

    Gibt (erfolg, meldung).
    """
    erwartet = [z for z in text if z.isdigit() or z == ',']
    if len(zeichen) != len(erwartet):
        return False, ('%d Flaechen gefunden, aber %d Zeichen getippt — '
                       'passt nicht zusammen, nichts gelernt'
                       % (len(zeichen), len(erwartet)))
    for bild, zeichenname in zip(zeichen, erwartet):
        # Bis zu acht Beispiele je Ziffer: Genug, um Ausreisser abzufangen,
        # und die Datei bleibt weit unter 100 KB.
        beispiele = ziffern.setdefault(zeichenname, [])
        if len(beispiele) < 8 and bild not in beispiele:
            beispiele.append(bild)
    return True, 'gelernt: %s' % ' '.join(erwartet)


def erkennen(zeichen, ziffern, hoechstens=0.18):
    """Aus normierten Zeichen die Zahl bauen.

    Gibt (text, schlechtester_abstand). Ist ein Zeichen unsicherer als
    `hoechstens`, kommt None zurueck — lieber nichts sagen als raten.
    """
    if not ziffern:
        return None, 1.0
    text, schlechtester = [], 0.0
    for bild in zeichen:
        bester, bestes_zeichen = 1.0, None
        for name, beispiele in ziffern.items():
            for beispiel in beispiele:
                a = abstand(bild, beispiel)
                if a < bester:
                    bester, bestes_zeichen = a, name
        if bestes_zeichen is None or bester > hoechstens:
            return None, bester
        text.append(bestes_zeichen)
        if bester > schlechtester:
            schlechtester = bester
    return ''.join(text), schlechtester


# --------------------------------------------------------------------------
# PNG schreiben — nur zum Ansehen, nicht fuer den Betrieb
# --------------------------------------------------------------------------

def png_schreiben(raster, ziel):
    """Ein Graustufen-Raster als PNG ablegen. Reine Standardbibliothek."""
    hoehe = len(raster)
    breite = len(raster[0]) if hoehe else 0
    roh = bytearray()
    for zeile in raster:
        roh.append(0)                       # Filter "keiner"
        for punkt in zeile:
            roh.append(punkt)

    def brocken(art, inhalt):
        return (struct.pack('>I', len(inhalt)) + art + inhalt
                + struct.pack('>I', zlib.crc32(art + inhalt) & 0xFFFFFFFF))

    with open(ziel, 'wb') as f:
        f.write(b'\x89PNG\r\n\x1a\n'
                + brocken(b'IHDR', struct.pack('>IIBBBBB', breite, hoehe,
                                               8, 0, 0, 0, 0))
                + brocken(b'IDAT', zlib.compress(bytes(roh), 6))
                + brocken(b'IEND', b''))


# --------------------------------------------------------------------------
# Kommandozeile
# --------------------------------------------------------------------------

# Der Bereich, in dem die Pille sitzt — am 09.09.2026 auf einem
# 5120x1440-Fenster ausgemessen (2480,470 mit 200x56).
# ⚠ Diese Anteile gelten fuer 3,56:1. SC verschiebt HUD-Elemente je nach
# Bildformat; auf 16:9 sitzt die Pille woanders. Deshalb ist das hier nur der
# Startwert des WERKZEUGS — im Programm wird die Pille spaeter gesucht und
# einmal bestaetigt, nicht ueber eine Prozentzahl festgenagelt.
ANTEIL = (0.4844, 0.3264, 0.5234, 0.3653)


def bereich_holen():
    """Den Signatur-Bereich abgreifen. Gibt (raster, meldung)."""
    lib = _x11()
    if lib is None:
        return None, 'libX11 nicht ladbar — laeuft hier X11?'
    anzeige = lib.XOpenDisplay(None)
    if not anzeige:
        return None, 'kein X-Display erreichbar'
    gefunden = spielfenster(lib, ctypes.c_void_p(anzeige))
    if gefunden is None:
        return None, ('kein Star-Citizen-Fenster gefunden — laeuft das Spiel, '
                      'und laeuft Wine im X11-Modus?')
    _f, breite, hoehe = gefunden
    x0, y0 = int(breite * ANTEIL[0]), int(hoehe * ANTEIL[1])
    b = int(breite * (ANTEIL[2] - ANTEIL[0]))
    h = int(hoehe * (ANTEIL[3] - ANTEIL[1]))
    geholt = abgreifen(x0, y0, b, h)
    if geholt is None:
        return None, 'Abgriff fehlgeschlagen'
    return geholt[0], 'Fenster %dx%d, Ausschnitt %d,%d %dx%d' % (
        breite, hoehe, x0, y0, b, h)


def main():
    zerleger = argparse.ArgumentParser(
        description='Die Scan-Signatur vom Bildschirm lesen.')
    zerleger.add_argument('--anlernen', metavar='ZAHL',
                          help='die Ziffern der angezeigten Zahl anlernen')
    zerleger.add_argument('--zeigen', action='store_true',
                          help='den Ausschnitt als PNG ablegen')
    zerleger.add_argument('--aus-bild', metavar='PNG',
                          help='statt vom Spiel aus einer PNG-Datei lesen')
    wahl = zerleger.parse_args()

    if wahl.aus_bild:
        gelesen = png_lesen(wahl.aus_bild)
        if gelesen is None:
            print('PNG nicht lesbar: %s' % wahl.aus_bild)
            return 1
        raster = gelesen[0]
        print('Aus Datei: %s (%dx%d)'
              % (wahl.aus_bild, gelesen[1], gelesen[2]))
    else:
        raster, meldung = bereich_holen()
        print(meldung)
        if raster is None:
            return 1

    schwelle = otsu(raster)
    hell = max(max(z) for z in raster)
    dunkel = min(min(z) for z in raster)
    ueber = sum(1 for z in raster for p in z if p > schwelle)
    print('Schwelle %d (Bild %d..%d, %d Punkte darueber)'
          % (schwelle, dunkel, hell, ueber))
    if ueber < MINDEST_HELLE_PUNKTE:
        print('Kein Text im Bild — ist die Signatur gerade eingeblendet?')
        return 1

    if wahl.zeigen:
        # ⚠ NICHT in den Repo-Ordner: Das Bild zeigt einen Ausschnitt des
        # echten Bildschirms und hat in einem oeffentlichen Repo nichts zu
        # suchen (am 09.09.2026 waere es beinahe mitcommittet worden).
        # Es landet deshalb in der Ablage, die ohnehin privat ist.
        from scbp import pfade as _pf
        ziel = _pf.app_datei('signatur-ausschnitt.png')
        png_schreiben(raster, ziel)
        print('PNG: %s' % ziel)

    flaechen = zeichen_trennen(raster, schwelle)
    print('%d Flaeche(n) gefunden:' % len(flaechen))
    for links, oben, rechts, unten in flaechen:
        print('   x %3d-%-3d  y %2d-%-2d  (%dx%d)'
              % (links, rechts, oben, unten,
                 rechts - links + 1, unten - oben + 1))
    if not flaechen:
        return 1

    zeichen = [normieren(raster, f, schwelle) for f in flaechen]
    ziffern = vorlagen_laden()

    if wahl.anlernen:
        gut, was = anlernen(zeichen, wahl.anlernen, ziffern)
        print(was)
        if gut:
            vorlagen_sichern(ziffern)
            print('Vorlagen: %s' % VORLAGEN_DATEI)
            return 0
        return 1

    text, unsicher = erkennen(zeichen, ziffern)
    if text is None:
        print('Nicht sicher erkannt (Abstand %.2f) — keine Zahl gemeldet.'
              % unsicher)
        return 1
    print('Gelesen: %s   (schlechtester Abstand %.2f)' % (text, unsicher))
    return 0


if __name__ == '__main__':
    sys.exit(main())
