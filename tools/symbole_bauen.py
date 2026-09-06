# -*- coding: utf-8 -*-
"""Erzeugt die Symbol-Bilder der Oberfläche aus den Lucide-Vorlagen.

**Wozu das gut ist.** Bis v3.0.0-rc55 waren die Symbole der Melde-Leiste
Schriftzeichen (`✕ 🗑 ⚙ ⟳ …`). Das hatte drei Nachteile, die alle denselben
Kern haben — *die Schrift entscheidet, nicht wir*:

1. **Ungleiche Größen.** Ein Schriftzeichen füllt seine Box nur zu 50–70 %, und
   jedes Zeichen anders. Neben den zwei selbstgemalten Symbolen (Glocke,
   Klemmbrett), die ihre Fläche voll nutzten, wirkte alles andere geschrumpft.
2. **Ungleicher Stil.** `🗑` und `▶` sind gefüllte Flächen, `⚙ ⟳ ⏻ ✕` dünne
   Striche. Die stammen von verschiedenen Schriftdesignern und passen nicht
   zusammen.
3. **Je System ein anderes Bild.** Windows greift zu `Segoe UI Symbol`, macOS
   und Linux zu etwas ganz anderem. Entwickelt wird auf allen dreien — er
   sah am Mac buchstäblich andere Zeichen als seine Nutzer unter Windows.

Ein PNG kennt diese Probleme nicht: feste Pixel, überall gleich.

**Warum Lucide.** Alle Symbole sind von denselben Leuten auf einem 24×24-Raster
mit 2 px Strichstärke gezeichnet. Einheitlichkeit ist damit eingebaut und muss
nicht von Hand hergestellt werden — der Punkt, an dem die selbstgemalte Glocke
zwei Anläufe gebraucht hat. Lizenz ISC, Mitliefern erlaubt (siehe
`assets/symbole/LIZENZ.txt`).

**Warum ein eigener Zeichner statt `cairosvg`.** Die üblichen SVG-Wandler hängen
an der System-Bibliothek *cairo*. Die ist unter Windows ein Ärgernis (fehlende
DLLs) — und der Autor soll dieses Skript auf **jedem** seiner drei Rechner selbst
laufen lassen können, ohne vorher etwas zu installieren. Deshalb liest es die
Pfade selbst und malt mit Pillow, das ohnehin schon Bau-Werkzeug ist
(`make_icon.py`).

**Wie gemalt wird.** Jeder Strich wird in eine dichte Punktkette aufgelöst und
mit einem Kreis je Punkt gestempelt. Das ergibt runde Enden und runde Ecken
(`stroke-linecap/-linejoin: round`) ohne Sonderbehandlung. Gemalt wird in
achtfacher Größe und danach mit LANCZOS verkleinert — daher die weichen Kanten,
die ein Tk-Canvas von sich aus nicht hinbekommt.

**Aufruf** (aus dem Projektordner):

    python tools/symbole_bauen.py

Die fertigen Bilder landen in `assets/symbole/` und gehören ins Repo — der
Release-Bau lädt hier nichts nach.
"""

import math
import os
import re
import sys
import xml.etree.ElementTree as ET

try:
    from PIL import Image, ImageDraw
except ImportError:                                   # pragma: no cover
    sys.exit('Pillow fehlt. Erst installieren:  pip install pillow')


HIER = os.path.dirname(os.path.abspath(__file__))
PROJEKT = os.path.dirname(HIER)
# ⚠ Die SVG-Vorlagen liegen unter `tools/`, **nicht** unter `assets/`: Sie
# sind Bau-Material, und `assets/symbole` wandert vollständig in die
# fertige `.exe`. Dort haben Dateien nichts verloren, die das Programm nie
# liest.
VORLAGEN = os.path.join(HIER, 'symbol-vorlagen')
ZIEL = os.path.join(PROJEKT, 'assets', 'symbole')

# Achtfach malen, dann verkleinern — das erzeugt die weichen Kanten.
UEBER = 8
# Lucide zeichnet auf einem 24×24-Raster mit 2 px Strich.
RASTER = 24.0
STRICH = 2.0

# ⚠ Die Größen hängen **nicht** mehr an einer Schriftmetrik. Vorher stand da
# `f_zeichen.metrics('linespace') + 2` — und Schriftmetriken sind je System
# verschieden, womit die Symbole am Mac andere Maße hatten als unter Windows.
# Feste Pixelwerte sind überall gleich.
#
# Die Ordner heißen nach ihrer Pixelzahl (`assets/symbole/22/…`), nicht nach der
# Einstellungs-Stufe. Grund: Dieselbe Zahl wird für verschiedene Stufen
# gebraucht, je nachdem wofür das Symbol steht — ein Knopf in der Stufe „klein"
# ist so groß wie ein Zeilenpunkt in „sehrgross". Nach Pixeln benannt, gibt es
# jede Größe genau einmal.
#
# KNOPF: die Symbole der Leisten. `normal` = 22 px ist so gewählt, dass die
#        Melde-Leiste ihre bisherige Höhe von 26 px behält (22 + 2 px Luft).
# ZEILE: die kleinen Zeichen **in** einer Textzeile — Statuspunkte vor einem
#        Bauplan, Haken, Aufklapp-Pfeile. Die müssen zur Textgröße passen, nicht
#        zur Leiste.
KNOPF = {'klein': 18, 'normal': 22, 'gross': 26, 'sehrgross': 30}
ZEILE = {'klein': 12, 'normal': 14, 'gross': 16, 'sehrgross': 18}
# Muss zu `scbp/zeichen.py` passen — sonst fehlt die 22er-Version und das
# Zeichen verschwindet bei „sehr gross" stillschweigend (`bild()` gibt `None`).
ANTIPPBAR = {'klein': 14, 'normal': 16, 'gross': 18, 'sehrgross': 22}

# ⚠ **Kein 2×-Satz für hochauflösende Bildschirme.** Der Gedanke lag nahe —
# Retina-Macs und Windows mit 125 % Skalierung blasen ein kleines Bild auf, eine
# scharfe Version daneben müsste helfen. Am 27.08.2026 nachgemessen: **Tk kann
# das nicht.** Es zeichnet einen Bildpunkt als einen Punkt, ohne umzurechnen —
# ein 44-px-Symbol erscheint schlicht doppelt so groß, nicht doppelt so scharf.
# Verkleinern ginge nur mit `PhotoImage.subsample()`, und das wirft jeden zweiten
# Pixel weg, statt zu glätten: Das Ergebnis wäre schlechter als das direkt in der
# Zielgröße gemalte Bild.
#
# Wer mehr Schärfe will, muss also die Symbole **größer** machen (die Zahlen
# oben), nicht feiner auflösen.

# Die Farben stammen aus `scbp/hauptfenster.py`. Bleibt eine Farbe dort nicht
# gleich, muss sie hier mitgezogen und das Skript neu gestartet werden.
FARBEN = {
    'grau':  '#8b98a5',      # SUB    — der Normalzustand
    'gruen': '#9ce430',      # ACCENT — Update da, Schalter an, Reiter gewählt
    'hell':  '#e6edf3',      # FG     — Mauszeiger darüber
    # Die beiden Zustandsfarben der Bauplanzeilen. Ohne sie müsste ein gelber
    # Punkt grün gemalt werden, und die Zeile verlöre ihre Aussage.
    'gelb':  '#d8a03a',      # PROV   — aus der Game.log, noch nicht bestätigt
    'blau':  '#4aa3d8',      # CATA   — neu im Spiel craftbar, kein eigener Fund
    'rot':   '#e05252',      # ROT    — „hier meldest du, wenn etwas klemmt“
}

# Welche Lucide-Vorlage wofür steht. Der Schlüssel ist der Name, unter dem das
# Programm das Symbol anfordert — er sagt, was es *bedeutet*, nicht wie es
# aussieht. So kann eine Vorlage getauscht werden, ohne den Code anzufassen.
#
# Zwei Tabellen, weil die Größe vom Einsatzort abhängt (siehe KNOPF/ZEILE oben).

KNOPF_SYMBOLE = {
    # --- Melde-Leiste (Overlay) ---
    'starten':      'rocket',            # ⚠ nicht `play`: ein Abspielpfeil heißt
                                         # „Video ab", nicht „Programm starten".
                                         # Eine Rakete sagt beides — starten und
                                         # Weltraum. Gemeldet am 27.08.2026:
                                         # „SC Starten ist das symbol nicht
                                         # eindeutig genug".
    'glocke':       'bell',              # neue Version verfügbar
    'liste':        'clipboard-list',    # die Bauplan-Liste
    'einstellungen': 'settings',
    'einklappen':   'chevron-down',    # Zustand „offen" — Klick klappt zu
    'aufklappen':   'chevron-right',   # Zustand „zusammengeklappt"
    'ausklappen':   'chevron-up',
    'leeren':       'eraser',            # ⚠ nicht `trash`: der Knopf löscht
                                         # nichts, er räumt nur die angezeigten
                                         # Meldungen weg — die Baupläne bleiben.
                                         # Ein Mülleimer verspricht Vernichtung
                                         # und schreckt vom Klicken ab.
    'schliessen':   'x',
    # ⚠ Für „diesen Auftrag ausblenden" — **nicht** `x`. Das Kreuz steht im
    # Programm für „Fenster schliessen"; hier wird eine einzelne Zeile
    # weggenommen. Der durchgestrichene Kreis sagt „gilt nicht mehr", und er
    # ist deutlich zu sehen: „nen Blinder findet das sonst nicht mehr."
    'ausblenden':   'ban',
    # Klicks werden ins Spiel durchgereicht (zu) oder abgefangen (offen). Das
    # Schloss bleibt als einziges Element klickbar — sonst käme man aus dem
    # durchlässigen Zustand nur heraus, indem man aus dem Spiel heraustabbt.
    'schloss_zu':   'lock',
    'schloss_auf':  'lock-open',
    'ziehgriff':    'grip',              # die Ecke zum Größerziehen
    # ⚠ Vier Richtungen statt einer. Der Griff sitzt an der FREIEN Ecke des
    # Fensters — bei einer unteren Bildschirmecke also oben, bei einer rechten
    # links. Der Pfeil zeigt dorthin, wohin sich das Fenster ziehen laesst.
    # Vorher stand dort fest das Schriftzeichen „◢" und wies in drei von vier
    # Ecken gegen den Bildschirmrand, wo kein Platz ist. Gemeldet 02.09.2026.
    'ziehen_ol':    'arrow-up-left',
    'ziehen_or':    'arrow-up-right',
    'ziehen_ul':    'arrow-down-left',
    'ziehen_ur':    'arrow-down-right',
    # --- Seitenleiste und Titelknöpfe des großen Fensters ---
    'fortschritt':  'chart-column',
    'anzeige':      'monitor',
    'auftragstexte': 'message-square-text',
    'bestand':      'package',
    'wasistneu':    'sparkles',
    'ueber':        'info',
    'serverstatus': 'server',
    'ordner':       'folder',
    'erkennung':    'scan-search',
    # ⚠ `gamepad-2` und nicht `joystick`: Lucide fuehrt keinen Joystick, und
    # der Reiter meint ohnehin alle Eingabegeraete — Sticks, Pedale, Gamepads.
    'joysticks':    'gamepad-2',
    # ⚠ `chart-spline` und nicht `chart-column`: Das Säulendiagramm gehört
    # schon dem Fortschritt, und die Seite zeigt eine Kurve, keine Balken.
    # Die Vorlage ist ein Achsenkreuz mit geschwungener Linie — genau das,
    # was die Seite tut.
    'achsen':       'chart-spline',
    # Der Blickwinkel — was das Auge vom Bildschirm sieht.
    'blickwinkel':  'eye',
    'diagnose':     'stethoscope',
    'quellen':      'heart-handshake',   # fremde Arbeit + Lizenzen
    # --- nur für die Anleitung (README-Merkmalstabelle) ---
    # ⚠ Die standen dort als Emoji (⚡ 🧭 🏷️ 🔔 …). Emoji sehen auf jedem System
    # anders aus und haben mit dem Symbolsatz des Programms nichts zu tun — in
    # einer Anleitung für genau dieses Programm ist das ein Bruch.
    'blitz':        'zap',
    'herkunft':     'compass',
    'kuerzel':      'tag',
    'ton':          'volume-2',
    'vordergrund':  'pin',
    'verschieben':  'move',
    'sprachen':     'languages',
    'nurlesend':    'shield-check',
    'eigenbuch':    'notebook-pen',
    'abhaken':      'check-check',
    'einrichtung':  'wand-sparkles',     # der Assistent — ein Zauberstab ist das
                                         # übliche Bild für „führt dich durch".
    'neustart':     'rotate-cw',
    'herunterladen': 'download',
    # ⚠ Nicht `download` mitbenutzen: Der Pfeil steht im Programm schon für
    # „neue Version holen". Ein Schild sagt „in Sicherheit gebracht", und das
    # ist es, was der Knopf tut. Dieselbe Vorlage steht bereits unter
    # `nurlesend` — die taucht aber nur in der Anleitung auf, nie in der
    # Leiste, also gibt es nichts zu verwechseln.
    'sicherung':    'shield-check',
    # Die Spielzeit in der Kopfzeile (05.09.2026).
    'zeit':         'clock',
    'zurueck':      'undo-2',            # auf eine ältere Version zurück
    # --- Gruppe „Handel" (v3.4.0) ---
    # Münzen für den Verkauf, Lagerhalle für den Handelsbestand. Bewusst
    # **nicht** dasselbe `package` wie das Werkstatt-Lager: Zwei Reiter mit
    # demselben Bild sind in einer Leiste nicht auseinanderzuhalten.
    'verkauf':      'coins',
    'handelslager': 'warehouse',
    # Der eigene Hangar. Bewusst **nicht** `rocket` — das ist seit v3.0.0 der
    # Startknopf für das Spiel, und zwei verschiedene Dinge dürfen nicht
    # dasselbe Bild tragen. Auch nicht `warehouse`: Die Halle steht schon für
    # den Handelsbestand. Ein Fluggerät ist das, was hier drinsteht.
    'hangar':       'plane',
    # Die Wunschliste. Der Stern ist das Bild, das überall für „vorgemerkt"
    # steht — und er ist im Programm noch frei: Die Beobachtungsliste bei den
    # Bauplänen malt kein Symbol, sie färbt ihre Zeile golden.
    #
    # ⚠ Bewusst **nicht** `heart`: Ein Herz heißt „gefällt mir", ein Stern
    # „will ich haben". Auf einer Liste, die Kaufpreise trägt, ist der
    # Unterschied nicht bloß Geschmack.
    'wunschliste':  'star',
    # Die Farmliste: was noch im Boden liegt. Ein Kristall ist das Nächste am
    # Erz, was der Satz hergibt.
    #
    # ⚠ `diamond` trägt bereits das **Zeilen**-Symbol `standard` (die kleine
    # Raute in einer Bestandszeile). Das ist hier unbedenklich: Die beiden
    # Sätze sind getrennt, und ein 14-px-Zeichen mitten in einer Textzeile
    # steht nie neben einem 22-px-Reitersymbol. Die Regel „zwei verschiedene
    # Dinge, zwei verschiedene Bilder" zielt auf **eine** Leiste — dort kommt
    # die Raute genau einmal vor.
    'farmliste':    'diamond',
    # Der Laden, in dem ein fertiges Teil im Regal steht — die Gegenrichtung
    # zu `verkauf` (Münzen, Ware loswerden) und zu `herkunft` (Kompass, wo
    # kommt der Rohstoff her). Drei verschiedene Fragen, drei Bilder.
    'laeden':       'store',
    # Die Route: zwei Punkte, ein Weg dazwischen. Nicht `compass` (das trägt
    # `herkunft`) — dort geht es um „wo kommt es her", hier um „in welcher
    # Reihenfolge fahre ich".
    'routen':       'route',
    # Die Einkaufsliste über alle Schiffe — Teile und Wunschschiffe mit
    # Gesamtpreis und Einzelaufstellung.
    #
    # ⚠ Bewusst **nicht** `clipboard-list`: Das trägt schon `liste` (die
    # Bauplan-Liste). Zwei verschiedene Dinge dürfen nicht dasselbe Bild
    # haben — dieselbe Überlegung wie bei `hangar` gegen `rocket` weiter oben.
    # Auch nicht `coins` (`verkauf`, Geld einnehmen) oder `store` (`laeden`,
    # wo ein Teil im Regal steht).
    #
    # Ein Kassenbon ist das Bild, das die Sache trifft: eine Aufstellung mit
    # Summe darunter. Der Wunsch war wörtlich „eine Einzelaufstellung, so wie
    # jede Rechnung die man bekommen würde".
    'einkaufsliste': 'receipt',
}

ZEILEN_SYMBOLE = {
    # ⚠ Diese vier lösen die farbigen Emoji `🟢 🟡 🔵 ⭐` ab, die vor jeder
    # Bauplanzeile standen. Emoji liegen über `U+FFFF`; Windows malt sie über
    # die Farb-Emoji-Schrift als bunte Klötzchen, die die eingestellte Farbe
    # **ignorieren**. Das fiel lange nicht auf, weil es die am häufigsten
    # gesehene Stelle des Programms ist und man sich daran gewöhnt.
    'bestaetigt':   'circle-check',      # war 🟢 — Fund steht fest
    'vorlaeufig':   'circle-dashed',     # war 🟡 — noch nicht bestätigt
    'punkt':        'circle',            # war 🔵 — gewöhnlicher Eintrag
    'gemerkt':      'star',              # war ⭐ — vorgemerkt
    # --- Haken, Aufklapper, Statuszeichen ---
    'haken':        'check',             # war ✔ / ✓
    'offen':        'circle',            # war ○
    'standard':     'diamond',           # war ◆
    'aufklappen':   'chevron-right',     # war ▶ (Liste zu)
    'zuklappen':    'chevron-down',      # war ▼ (Liste offen)
    'hinweiszeile': 'info',              # war ⓘ / ℹ in einer Zeile
    'kaffee':       'coffee',            # die Tasse im großen Fenster
    # ⚠ Steht **auch** unter KNOPF_SYMBOLE. Das Kreuz schließt nicht nur
    # Fenster (groß, in der Leiste), sondern auch Kästen mitten auf einer
    # Seite (klein, in der Zeile) — etwa den Herkunftskasten der Bauplan-
    # Liste. Fehlte die kleine Version, blieb dort eine leere Lücke statt
    # eines Kreuzes; gemeldet von am 27.08.2026 gemeldet.
    'schliessen':   'x',
    # Steht ebenfalls in beiden Tabellen: Im Overlay sitzt es in einer Zeile
    # (klein), im grossen Fenster koennte es als Knopf gebraucht werden.
    'ausblenden':   'ban',
}

# Alles zusammen, mit dem passenden Größensatz.
# ⚠ Der Wert ist die Menge der **Pixelgrößen**, nicht der Größensatz selbst.
# Ein Name darf in beiden Tabellen stehen (`schliessen` tut es) und braucht dann
# beide Reihen. Vorher überschrieb die zweite Schleife die erste — der Name
# verlor stillschweigend seine Knopfgrößen, und in der Melde-Leiste wäre statt
# des Kreuzes eine Lücke geblieben.
# Zeichen, die in einer Zeile sitzen, aber angeklickt werden — sie brauchen
# zusaetzlich die Reihe aus `ANTIPPBAR`.
ANTIPPBAR_SYMBOLE = {n: ZEILEN_SYMBOLE[n]
                     for n in ('hinweiszeile', 'zuklappen')}

SYMBOLE = {}
for _tabelle, _satz in ((KNOPF_SYMBOLE, KNOPF), (ZEILEN_SYMBOLE, ZEILE),
                        (ANTIPPBAR_SYMBOLE, ANTIPPBAR)):
    for _n, _v in _tabelle.items():
        _vorlage, _groessen = SYMBOLE.get(_n, (_v, set()))
        SYMBOLE[_n] = (_vorlage, _groessen | set(_satz.values()))


# ------------------------------------------------------------------ SVG lesen

def _zahlen(text):
    """Alle Zahlen einer Pfadangabe der Reihe nach."""
    return [float(z) for z in
            re.findall(r'-?\d*\.?\d+(?:[eE][-+]?\d+)?', text)]


def _bezier(p0, p1, p2, p3, schritte):
    """Eine kubische Bézier-Kurve als Punktkette."""
    punkte = []
    for i in range(1, schritte + 1):
        t = i / float(schritte)
        g = 1.0 - t
        punkte.append((
            g*g*g*p0[0] + 3*g*g*t*p1[0] + 3*g*t*t*p2[0] + t*t*t*p3[0],
            g*g*g*p0[1] + 3*g*g*t*p1[1] + 3*g*t*t*p2[1] + t*t*t*p3[1]))
    return punkte


def _bogen(start, rx, ry, dreh, gross, richtung, ende, schritte=24):
    """Einen SVG-Bogen (`A`) als Punktkette.

    Das ist der einzige wirklich sperrige Teil eines SVG-Pfads: Die Angabe
    beschreibt den Bogen über *Zielpunkt und Radien*, gezeichnet wird aber über
    *Mittelpunkt und Winkel*. Die Umrechnung steht so in der SVG-Spezifikation
    (Abschnitt „Elliptical arc implementation notes") und ist hier nur
    nachgebaut.
    """
    x1, y1 = start
    x2, y2 = ende
    if rx == 0 or ry == 0 or (abs(x1 - x2) < 1e-9 and abs(y1 - y2) < 1e-9):
        return [ende]
    rx, ry = abs(rx), abs(ry)
    phi = math.radians(dreh)
    cos_p, sin_p = math.cos(phi), math.sin(phi)

    # In das Koordinatensystem der (ungedrehten) Ellipse wechseln.
    dx, dy = (x1 - x2) / 2.0, (y1 - y2) / 2.0
    x1s = cos_p * dx + sin_p * dy
    y1s = -sin_p * dx + cos_p * dy

    # Zu kleine Radien aufblasen, sonst gibt es keine Lösung.
    lam = (x1s*x1s) / (rx*rx) + (y1s*y1s) / (ry*ry)
    if lam > 1:
        rx *= math.sqrt(lam)
        ry *= math.sqrt(lam)

    zaehler = rx*rx * ry*ry - rx*rx * y1s*y1s - ry*ry * x1s*x1s
    nenner = rx*rx * y1s*y1s + ry*ry * x1s*x1s
    faktor = math.sqrt(max(0.0, zaehler / nenner)) if nenner else 0.0
    if gross == richtung:
        faktor = -faktor
    cxs = faktor * rx * y1s / ry
    cys = -faktor * ry * x1s / rx

    # Zurück ins Bild.
    cx = cos_p * cxs - sin_p * cys + (x1 + x2) / 2.0
    cy = sin_p * cxs + cos_p * cys + (y1 + y2) / 2.0

    def winkel(ux, uy, vx, vy):
        punkt = ux*vx + uy*vy
        laenge = math.hypot(ux, uy) * math.hypot(vx, vy)
        w = math.acos(max(-1.0, min(1.0, punkt / laenge))) if laenge else 0.0
        return -w if ux*vy - uy*vx < 0 else w

    start_w = winkel(1, 0, (x1s - cxs) / rx, (y1s - cys) / ry)
    delta = winkel((x1s - cxs) / rx, (y1s - cys) / ry,
                   (-x1s - cxs) / rx, (-y1s - cys) / ry)
    if not richtung and delta > 0:
        delta -= 2 * math.pi
    elif richtung and delta < 0:
        delta += 2 * math.pi

    punkte = []
    for i in range(1, schritte + 1):
        w = start_w + delta * i / float(schritte)
        ex, ey = rx * math.cos(w), ry * math.sin(w)
        punkte.append((cos_p * ex - sin_p * ey + cx,
                       sin_p * ex + cos_p * ey + cy))
    return punkte


def _pfad_zerlegen(d):
    """Eine `d`-Angabe in einzelne Striche (Punktketten) zerlegen."""
    teile = re.findall(r'([MmLlHhVvCcSsAaZz])([^MmLlHhVvCcSsAaZz]*)', d)
    striche, aktuell = [], []
    x = y = 0.0
    start_x = start_y = 0.0
    letzter_griff = None                  # für `S` — der gespiegelte Griff

    def ablegen():
        if len(aktuell) > 1:
            striche.append(list(aktuell))

    for befehl, rest in teile:
        werte = _zahlen(rest)
        klein = befehl.islower()          # Kleinbuchstabe = relativ
        gross_b = befehl.upper()

        if gross_b == 'M':
            for i in range(0, len(werte) - 1, 2):
                nx, ny = werte[i], werte[i+1]
                if klein:
                    nx, ny = x + nx, y + ny
                if i == 0:
                    ablegen()
                    aktuell = [(nx, ny)]
                    start_x, start_y = nx, ny
                else:
                    aktuell.append((nx, ny))   # weitere Paare gelten als `L`
                x, y = nx, ny
            letzter_griff = None

        elif gross_b == 'L':
            for i in range(0, len(werte) - 1, 2):
                nx, ny = werte[i], werte[i+1]
                if klein:
                    nx, ny = x + nx, y + ny
                aktuell.append((nx, ny))
                x, y = nx, ny
            letzter_griff = None

        elif gross_b == 'H':
            for w in werte:
                x = x + w if klein else w
                aktuell.append((x, y))
            letzter_griff = None

        elif gross_b == 'V':
            for w in werte:
                y = y + w if klein else w
                aktuell.append((x, y))
            letzter_griff = None

        elif gross_b == 'C':
            for i in range(0, len(werte) - 5, 6):
                p = werte[i:i+6]
                if klein:
                    p = [p[0]+x, p[1]+y, p[2]+x, p[3]+y, p[4]+x, p[5]+y]
                aktuell += _bezier((x, y), (p[0], p[1]), (p[2], p[3]),
                                   (p[4], p[5]), 16)
                letzter_griff = (p[2], p[3])
                x, y = p[4], p[5]

        elif gross_b == 'S':
            # Der erste Griff ist die Spiegelung des vorigen am Startpunkt.
            for i in range(0, len(werte) - 3, 4):
                p = werte[i:i+4]
                if klein:
                    p = [p[0]+x, p[1]+y, p[2]+x, p[3]+y]
                if letzter_griff:
                    g1 = (2*x - letzter_griff[0], 2*y - letzter_griff[1])
                else:
                    g1 = (x, y)
                aktuell += _bezier((x, y), g1, (p[0], p[1]), (p[2], p[3]), 16)
                letzter_griff = (p[0], p[1])
                x, y = p[2], p[3]

        elif gross_b == 'A':
            for i in range(0, len(werte) - 6, 7):
                rx, ry, dreh, gr, ri, nx, ny = werte[i:i+7]
                if klein:
                    nx, ny = x + nx, y + ny
                aktuell += _bogen((x, y), rx, ry, dreh, int(gr), int(ri),
                                  (nx, ny))
                x, y = nx, ny
            letzter_griff = None

        elif gross_b == 'Z':
            if aktuell:
                aktuell.append((start_x, start_y))
                ablegen()
                aktuell = [(start_x, start_y)]
            x, y = start_x, start_y
            letzter_griff = None

    ablegen()
    return striche


def _rechteck(x, y, breite, hoehe, radius):
    """Ein Rechteck mit runden Ecken als geschlossene Punktkette."""
    r = min(radius, breite / 2.0, hoehe / 2.0)
    punkte = []
    ecken = ((x + breite - r, y + r, -90, 0),        # rechts oben
             (x + breite - r, y + hoehe - r, 0, 90),  # rechts unten
             (x + r, y + hoehe - r, 90, 180),         # links unten
             (x + r, y + r, 180, 270))               # links oben
    for cx, cy, von, bis in ecken:
        for i in range(9):
            w = math.radians(von + (bis - von) * i / 8.0)
            punkte.append((cx + r * math.cos(w), cy + r * math.sin(w)))
    punkte.append(punkte[0])
    return punkte


def _kreis(cx, cy, r):
    """Ein Kreis als geschlossene Punktkette."""
    punkte = [(cx + r * math.cos(math.radians(g)),
               cy + r * math.sin(math.radians(g)))
              for g in range(0, 361, 8)]
    return punkte


def striche_lesen(pfad):
    """Alle Striche einer Lucide-Vorlage in 24er-Koordinaten."""
    baum = ET.parse(pfad).getroot()
    striche = []
    for knoten in baum.iter():
        art = knoten.tag.split('}')[-1]
        h = knoten.attrib
        if art == 'path':
            striche += _pfad_zerlegen(h.get('d', ''))
        elif art == 'circle':
            striche.append(_kreis(float(h['cx']), float(h['cy']),
                                  float(h['r'])))
        elif art == 'rect':
            striche.append(_rechteck(float(h['x']), float(h['y']),
                                     float(h['width']), float(h['height']),
                                     float(h.get('rx', 0))))
        elif art == 'line':
            striche.append([(float(h['x1']), float(h['y1'])),
                            (float(h['x2']), float(h['y2']))])
        elif art == 'polyline':
            zahlen = _zahlen(h.get('points', ''))
            striche.append([(zahlen[i], zahlen[i+1])
                            for i in range(0, len(zahlen) - 1, 2)])
    return striche


# ------------------------------------------------------------------ malen

def maske_malen(striche, px):
    """Die Striche als Graustufen-Maske in achtfacher Größe malen.

    Gestempelt wird mit einem Kreis je Punkt. Das klingt umständlich, spart aber
    jede Sonderbehandlung für runde Enden und runde Ecken — beides ergibt sich
    von selbst, wenn die Punkte dicht genug liegen.
    """
    gross = px * UEBER
    faktor = gross / RASTER
    dicke = STRICH * faktor
    r = dicke / 2.0

    bild = Image.new('L', (gross, gross), 0)
    stift = ImageDraw.Draw(bild)

    for strich in striche:
        # Die Kette verdichten, damit die Stempel lückenlos überlappen.
        dicht = []
        for i, (px_, py_) in enumerate(strich):
            x, y = px_ * faktor, py_ * faktor
            if i:
                vx, vy = dicht[-1]
                weite = math.hypot(x - vx, y - vy)
                for s in range(1, int(weite) + 1):
                    t = s / float(int(weite) + 1)
                    dicht.append((vx + (x - vx) * t, vy + (y - vy) * t))
            dicht.append((x, y))
        for x, y in dicht:
            stift.ellipse((x - r, y - r, x + r, y + r), fill=255)

    return bild.resize((px, px), Image.LANCZOS)


def einfaerben(maske, farbe):
    """Aus der Maske ein farbiges Bild mit durchsichtigem Grund machen."""
    r = int(farbe[1:3], 16)
    g = int(farbe[3:5], 16)
    b = int(farbe[5:7], 16)
    bild = Image.new('RGBA', maske.size, (r, g, b, 0))
    bild.putalpha(maske)
    return bild


def main():
    if not os.path.isdir(VORLAGEN):
        sys.exit('Vorlagen fehlen: %s' % VORLAGEN)

    gebaut = 0
    fehlend = []
    for name, (vorlage, groessen) in sorted(SYMBOLE.items()):
        quelle = os.path.join(VORLAGEN, vorlage + '.svg')
        if not os.path.exists(quelle):
            fehlend.append(vorlage)
            continue
        striche = striche_lesen(quelle)
        # Jede Pixelgröße nur **einmal** malen, auch wenn zwei Stufen dieselbe
        # Zahl haben — und die Farben aus derselben Maske schöpfen. Das spart
        # den teuersten Teil (das Malen in achtfacher Größe).
        noetig = set(groessen)
        for px in sorted(noetig):
            maske = maske_malen(striche, px)
            ordner = os.path.join(ZIEL, str(px))
            if not os.path.isdir(ordner):
                os.makedirs(ordner)
            for farbname, wert in FARBEN.items():
                ziel = os.path.join(ordner, '%s-%s.png' % (name, farbname))
                einfaerben(maske, wert).save(ziel)
                gebaut += 1
        print('  %-14s <- %-22s %s px' % (name, vorlage,
                                          '/'.join(str(g) for g in
                                                   sorted(noetig))))

    if fehlend:
        print('\n  ! Vorlagen fehlen: %s' % ', '.join(fehlend))
    print('\n%d Bilder in %s' % (gebaut, ZIEL))


if __name__ == '__main__':
    main()
