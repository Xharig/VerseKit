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
Ein Fenster für alles — Reiter links, Inhalt rechts.

**Warum der Umbau:** Bisher gab es zwei getrennte Fenster (Bauplan-Liste und
Einstellungen), und man musste wissen, in welchem etwas steckt. Jetzt liegen
beide zusammen: oben die Baupläne, darunter die Einstellungen, ganz unten
eingeklappt, was nur Fortgeschrittene brauchen.

**Aufbau des Rahmens** — die Reihenfolge beim Packen ist entscheidend:

    1. Titelleiste      oben, fest
    2. Fußzeile         unten, fest
    3. Reiterleiste     links, fest
    4. Inhaltsbereich   zuletzt, `expand=True` → bekommt den Rest

Wer den Inhalt vor der Fußzeile packt, schiebt sie aus dem Fenster — das ist
hier schon einmal passiert (der unsichtbare Speichern-Knopf). Steht auch in der
`CLAUDE.md` des Projekts.

**Seiten werden erst gezeichnet, wenn man sie öffnet.** Der Katalog hat über 700
Einträge; alles beim Start aufzubauen kostet Sekunden, die niemand hergibt, um
danach eine einzige Seite anzusehen.

**Kein Speichern-Knopf.** Jede Änderung greift sofort und wird sofort geschrieben.
Die Begründung: „Vergisst ein Nutzer das Speichern, ist der Ärger größer als
der Nutzen des Knopfes."
"""
import os
import sys
import time
import tkinter as tk
import tkinter.font as tkfont

from . import bildschirm, fehler, hinweis, neuheiten, pfade, zeichen
from .sprache import t, fenstertitel

BG      = '#10141c'
FLAECHE = '#161c28'
BAR     = '#1b2230'
FG      = '#e6edf3'
SUB     = '#8b98a5'
ACCENT  = '#9ce430'
LINIE   = '#232c3d'
GOLD    = '#e8c353'
# Rot ist hier kein Zustand, sondern ein Wegweiser: Der Reiter „Fehler
# melden“ traegt es, damit ihn niemand suchen muss.
ROT     = '#e05252'

# Mindestgröße: Darunter bricht die Bedienung, und keine Layout-Regel hilft mehr.
# Kleinste Größe, auf die sich das Fenster ziehen lässt — zugleich die Startgröße.
#
# **Breite:** `tools/randpruefung.py` zeigt, dass unterhalb von 1060 Pixel
# Bedienelemente auf den Seiten „Ordner", „Angaben im Spiel", „Bestand" und „Über"
# rechts herausragen — auf Englisch früher als auf Deutsch, weil die Wörter länger
# sind. 1100 gibt etwas Luft.
#
# **Höhe:** Der Wert hier ist nur die Untergrenze. Die wirkliche Mindesthöhe wird
# **gemessen** (siehe `_mindesthoehe_nachziehen`), denn wie viel Platz die
# Seitenleiste braucht, hängt an Schriftgröße und Anzeige-Skalierung: bei 100 %
# rund 674 Pixel, bei 125 % schon 842. Eine feste Zahl wäre auf dem einen
# System zu klein — dann ist unten „Diagnose" abgeschnitten — und auf dem anderen
# unnötig groß.
#
# Rücksicht auf kleine Laptop-Bildschirme braucht es nicht: Wer Star Citizen
# spielt, sitzt nicht an einem 1366×768-Gerät. Ein Fenster, das sich nicht beliebig
# klein ziehen lässt, macht weniger Ärger als abgeschnittene Knöpfe.
# ⚠ **1160, nicht 1100.** Die Knopfreihe auf „Fehler melden" braucht auf
# Deutsch 869 px (fünf Knöpfe, gemessen 29.08.2026); dazu die Seitenleiste mit
# 210 und rund 60 für Ränder und Rollleiste. Bei 1100 brach sie um und die
# Knöpfe standen untereinander — Xharig: „das sieht schrecklich aus."
# Englisch käme mit 710 aus; massgeblich ist die längere Sprache.
# ⭐ **Mindesthöhe 380 statt 760** (30.08.2026). Sie hing vorher am Platzbedarf
# der Seitenleiste — bei 1020 px passte das Fenster auf keinen 1080er
# Bildschirm mehr, und selbst auf grossen Schirmen liess es sich nicht kleiner
# ziehen als 1028. Seit die Leiste rollt und ihre Gruppen klappbar sind, geht
# nichts verloren, wenn das Fenster kürzer ist: Was nicht hinpasst, rollt.
MIN_BREITE, MIN_HOEHE = 1160, 380

# ⚠⚠ **Der Seiten-Vorbau ist ABGESCHALTET (02.09.2026).**
#
# Er baute alle uebrigen Seiten im Hintergrund vor, damit sie beim Anklicken
# sofort dastehen. Die Absicht war richtig, die Wirkung nicht: Tk zeichnet
# einstraengig, und der Vorbau hielt die Oberflaeche **1,7 Sekunden** am Stueck
# fest (gemessen ueber 17 Seiten: `wasistneu` 181 ms, `diagnose` 162 ms,
# `liste` 87 ms). Getroffen wurde jeweils das, was der Nutzer gerade anfasste —
# gemeldet mal als „linke leiste laed langsamer", mal als „bauplan liste
# weiterhin langsam". **Wechselnde Symptome, eine Ursache.**
#
# Zwei Anlaeufe, ihn zu baendigen, reichten nicht: erst nach Ruhe bauen, dann
# auch den Seitenwechsel als Aktion zaehlen. Er lief weiter, waehrend die
# frisch geoeffnete Seite noch gezeichnet wurde — sie meldete `steht (3 ms)`
# und war trotzdem nicht zu sehen.
#
# ⚠ **Er hat nie etwas beschleunigt.** Das stand von Anfang an in seiner
# eigenen Beschreibung: „Das macht nichts schneller — dieselbe Arbeit faellt
# weiter an, nur eben bevor jemand darauf wartet." Ohne ihn kostet die erste
# Anzeige einer Seite genau ihre Bauzeit, und die ist gemessen vertretbar:
# Bauplan-Liste 87 ms, teuerste Seite 178 ms, alle uebrigen unter 40 ms.
#
# Wer ihn wieder einschaltet, muss zuerst das Zeichnen der angeklickten Seite
# sicherstellen — sonst kehrt genau dieses Bild zurueck.
VORBAU_AN = False

# Die zuletzt eingestellte Fenstergroesse. Nur die **Groesse**, keine Lage:
# Eine gemerkte Position zeigt auf einem anderen Rechner ins Nichts (siehe die
# Regel dazu in der Projekt-CLAUDE.md und `geometrie_pruefen` beim Overlay) —
# das Fenster geht deshalb weiter mittig auf.
GROESSE_SCHLUESSEL = 'fenster_groesse'


def gemerkte_groesse(root):
    """Die gemerkte Fenstergroesse als `(Breite, Hoehe)`.

    Faellt auf die Mindestgroesse zurueck, wenn nichts gemerkt ist oder der
    Eintrag unbrauchbar ist.

    ⚠ **Zweimal begrenzt, und beides ist noetig.** Nach unten auf
    `MIN_BREITE`/`MIN_HOEHE` — sonst koennte ein alter Eintrag das Fenster
    kleiner machen, als seine Mindestgroesse zulaesst, und Tk zoege es beim
    ersten Zeichnen ruckartig wieder auf. Nach oben auf den Bildschirm: Wer
    seine Groesse am 4K-Schirm gemerkt hat und spaeter am Laptop startet,
    haette sonst ein Fenster, dessen rechte Haelfte nicht erreichbar ist.
    """
    roh = (pfade.einstellung(GROESSE_SCHLUESSEL) or '').strip().lower()
    breite = hoehe = 0
    if 'x' in roh:
        teile = roh.split('x', 1)
        if teile[0].isdigit() and teile[1].isdigit():
            breite, hoehe = int(teile[0]), int(teile[1])
    try:
        breite = min(breite, root.winfo_screenwidth())
        hoehe = min(hoehe, root.winfo_screenheight())
    except Exception:
        pass
    # ⚠⚠ **Die Mindestgroesse hat das letzte Wort — nach der Deckelung.**
    # Andersherum gewinnt auf einem Bildschirm, der kleiner ist als die
    # Mindestgroesse, der Bildschirm: Das Fenster kaeme mit 1024x768 heraus,
    # obwohl `minsize` 1160x380 verlangt, und Tk zoege es beim ersten Zeichnen
    # ruckartig wieder auf. Gefunden hat das der Bau-Lauf von v3.4.2 — der
    # Windows-Rechner dort hat einen kleineren Schirm als jeder echte Nutzer.
    breite = max(MIN_BREITE, breite)
    hoehe = max(MIN_HOEHE, hoehe)
    return breite, hoehe


# Startbreite der Seitenleiste. Auch sie ist nur eine Untergrenze: Wie breit
# „Angaben im Spiel" oder das englische „In-game details" wirklich wird, hängt
# wieder an Schrift und Skalierung — bei 125 % ragte der Text aus der Leiste
# heraus und war abgeschnitten.
LEISTE_BREITE = 210

# Wie viel Streichweg auf dem Trackpad eine Zeile ergibt. Ein Trackpad meldet
# viele kleine Schritte statt Rasten; ohne Teiler säuselt die Liste am Finger
# vorbei. Der Wert ist ein Startwert zum Nachjustieren — größer heißt ruhiger.
TRACKPAD_TEILER = 12

# Schriftgrößen als **eine** Stellschraube. Anlass: Das ⟳ in der Titelleiste war
# mit Brille kaum zu erkennen. Alle Widgets teilen sich diese Font-Objekte —
# `configure(size=…)` zieht damit die ganze Oberfläche mit, statt dass jede
# Stelle einzeln angefasst werden müsste.
STUFEN = {'klein': 0, 'normal': 1, 'gross': 3, 'sehrgross': 5}


def _rundes_rechteck(leinwand, x1, y1, x2, y2, radius, **kw):
    """Ein Rechteck mit runden Ecken.

    Tk kennt so etwas nicht — aber ein Vieleck mit `smooth=True` rundet genau
    dort ab, wo Punkte dicht beieinander liegen. Deshalb sitzt an jeder Ecke ein
    Punktepaar im Abstand des Radius.
    """
    punkte = [
        x1 + radius, y1, x2 - radius, y1, x2, y1,
        x2, y1 + radius, x2, y2 - radius, x2, y2,
        x2 - radius, y2, x1 + radius, y2, x1, y2,
        x1, y2 - radius, x1, y1 + radius, x1, y1,
    ]
    return leinwand.create_polygon(punkte, smooth=True, **kw)


def schiebeschalter(eltern, an, umschalten, grund=None):
    """Ein runder Schiebeschalter — an oder aus, auf einen Blick.

    Tk kennt nur Kästchen zum Ankreuzen, und die sehen auf jedem System anders
    aus. Auf einer kleinen Leinwand lässt sich dagegen genau das zeichnen, was
    heute jeder erwartet: eine Kapsel, in der ein Punkt nach rechts wandert.

    `umschalten()` wird beim Klick aufgerufen und muss den neuen Zustand
    zurückgeben — gezeichnet wird erst danach, damit nichts leuchtet, was gar
    nicht gespeichert wurde.
    """
    grund = grund or BG
    breite, hoehe = 44, 24
    c = tk.Canvas(eltern, width=breite, height=hoehe, bg=grund,
                  highlightthickness=0, bd=0, cursor='hand2')
    kapsel = _rundes_rechteck(c, 2, 3, breite - 2, hoehe - 3, radius=9,
                              fill='#2b3547', outline='')
    punkt = c.create_oval(5, 6, 19, 20, fill=SUB, outline='')

    def zeichnen(zustand):
        c.itemconfigure(kapsel, fill='#2a3a1c' if zustand else '#2b3547')
        c.itemconfigure(punkt, fill=ACCENT if zustand else SUB)
        x = (breite - 24) if zustand else 0
        c.coords(punkt, 5 + x, 6, 19 + x, 20)

    def klick(_=None):
        zeichnen(bool(umschalten()))

    c.bind('<Button-1>', klick)
    zeichnen(bool(an))
    c.zeichnen = zeichnen
    return c


def nach_vorn(fenster, fokus=False):
    """Ein vorhandenes Fenster nach vorn holen — **ohne aus dem Spiel zu werfen**.

    ⚠ **`lift()` allein genügt nicht.** Unter Wayland — und je nach
    Fensterverwaltung auch unter X11 — darf sich ein Fenster nicht selbst in
    den Vordergrund setzen; `lift()` wirkt dann nur innerhalb der eigenen
    Anwendung und `focus_force()` wird schlicht ignoriert. Gemeldet am
    29.08.2026: Ein Klick auf das Overlay öffnete zwar die Seite, aber das
    Fenster blieb hinter dem Spiel.

    Was zuverlässig wirkt, ist `-topmost` **kurz** zu setzen und gleich wieder
    abzuschalten: Der Compositor holt das Fenster dabei nach vorn, und danach
    klebt es nicht dauerhaft über allem.

    `deiconify()` gehört dazu — sonst bleibt ein **minimiertes** Fenster
    minimiert, und der Klick scheint gar nichts zu tun.

    ⚠⚠ **`focus_force()` nur auf ausdrücklichen Wunsch** (`fokus=True`).
    Es zieht den Tastaturfokus — und wer gerade Star Citizen spielt, fliegt
    damit aus dem Spiel. Genau das darf ein Overlay-Werkzeug nicht:

        „ich bin mitten im spiel, du kannst die fenster aufrufen aber
         bekommst du es hin mich nicht als raus zu tabben?"  (29.08.2026)

    Ohne Fokus kommt das Fenster **sichtbar** nach vorn, die Tastatur bleibt
    aber beim Spiel. Wer darin tippen will, klickt hinein — dann bekommt es den
    Fokus vom Fenstermanager, und das ist eine bewusste Entscheidung des
    Spielers statt eines Überfalls.

    Mit `fokus=True` wird es nur beim **Programmstart** gerufen: Dort hat der
    Nutzer das Werkzeug gerade selbst gestartet und will hin.
    """
    try:
        # 1. Der sanfte Weg — reicht unter X11 und den meisten Oberflächen.
        fenster.deiconify()
        fenster.lift()
        fenster.attributes('-topmost', True)
        fenster.after(400, lambda: fenster.attributes('-topmost', False))
        if fokus:
            fenster.focus_force()

        # 2. Wayland lässt das oft ins Leere laufen: Dort entscheidet der
        #    Compositor, wer vorne steht, und ein Fenster darf sich nicht
        #    selbst vordrängen. Was er annimmt, ist ein Fenster, das sich
        #    **neu anmeldet** — also einmal ab- und wieder aufmelden.
        #
        #    ⚠ Nur unter Wayland, und nur wenn das Fenster wirklich verdeckt
        #    ist. Es kostet ein kurzes Flackern; das ist der Preis dafür, dass
        #    der Klick überhaupt etwas tut. Ohne diesen Schritt blieb nur
        #    „Programm neu starten", und dazu Xharig am 29.08.2026:
        #    „nen user findet das nervig und wers nicht nervig findet rafft es
        #    nicht."
        if _wayland() and not fenster.focus_displayof():
            fenster.withdraw()
            fenster.update_idletasks()
            fenster.deiconify()
            fenster.lift()
            fenster.attributes('-topmost', True)
            fenster.after(400, lambda: fenster.attributes('-topmost', False))
            if fokus:
                fenster.focus_force()
        return True
    except tk.TclError:
        return False                 # ohne Fenstermanager nicht möglich


def _wayland():
    """Läuft die Sitzung unter Wayland?

    Beide Kennzeichen gelten: `WAYLAND_DISPLAY` setzt der Compositor,
    `XDG_SESSION_TYPE` die Anmeldung. Unter X11 ist keines davon gesetzt —
    dann bleibt es beim sanften Weg, der dort zuverlässig wirkt.
    """
    return bool(os.environ.get('WAYLAND_DISPLAY')
                or os.environ.get('XDG_SESSION_TYPE') == 'wayland')


def regler(eltern, von, bis, wert, beim_ziehen, breite=190, grund=None):
    """Ein Schieberegler in der Machart des Fensters.

    Tk bringt zwar `Scale` mit, aber das ist ein Systemelement: Auf dem Mac ein
    graues Kästchen, unter Windows ein anderes, unter Linux je nach Oberfläche
    wieder anders. Selbst gezeichnet sieht es überall gleich aus — und passt zu
    den Schaltern daneben.
    """
    grund = grund or BG
    hoehe = 26
    c = tk.Canvas(eltern, width=breite, height=hoehe, bg=grund,
                  highlightthickness=0, bd=0, cursor='hand2')
    y = hoehe // 2
    _rundes_rechteck(c, 0, y - 3, breite, y + 3, radius=3,
                     fill='#2b3547', outline='')
    gefuellt = _rundes_rechteck(c, 0, y - 3, 10, y + 3, radius=3,
                                fill=ACCENT, outline='')
    knopf = c.create_oval(0, y - 8, 16, y + 8, fill=ACCENT, outline='')

    spanne = float(max(1, bis - von))

    def zeichnen(w):
        anteil = max(0.0, min(1.0, (w - von) / spanne))
        x = 8 + anteil * (breite - 16)
        c.coords(gefuellt, *([0, y - 3, x, y - 3, x, y - 3, x, y + 3,
                              x, y + 3, 0, y + 3, 0, y + 3, 0, y - 3]))
        c.coords(knopf, x - 8, y - 8, x + 8, y + 8)

    def aus_x(ereignis):
        anteil = max(0.0, min(1.0, (ereignis.x - 8) / float(breite - 16)))
        return int(round(von + anteil * spanne))

    def ziehen(ereignis):
        neuer = aus_x(ereignis)
        zeichnen(neuer)
        beim_ziehen(neuer)

    c.bind('<Button-1>', ziehen)
    c.bind('<B1-Motion>', ziehen)
    zeichnen(wert)
    c.zeichnen = zeichnen
    return c



def ecken(x1, y1, x2, y2, r):
    """Die Punktfolge eines abgerundeten Rechtecks — für `coords`.

    Wird gebraucht, wenn ein schon gezeichnetes Rechteck seine Größe ändert:
    `create_polygon` legt die Punkte einmal fest, `coords` schiebt sie nach.
    """
    return [x1 + r, y1, x2 - r, y1, x2, y1, x2, y1 + r, x2, y2 - r, x2, y2,
            x2 - r, y2, x1 + r, y2, x1, y2, x1, y2 - r, x1, y1 + r, x1, y1]


def rundrahmen(eltern, grund, rand, radius=8, grundfarbe=None):
    """Ein Kasten mit runden Ecken, in den beliebiger Inhalt kommt.

    Tk kann Rahmen nur eckig — deshalb liegt hinter dem Inhalt eine Leinwand
    mit einem gemalten Rechteck, und der Inhalt sitzt als Fenster darauf. Die
    Leinwand zieht ihre Höhe nach, sobald der Inhalt steht; sonst bliebe sie
    auf ihrer Anfangsgröße und schnitte alles ab.

    Zurück kommt der innere Rahmen — dort hinein wird gepackt wie gewohnt.
    Am Rückgabewert hängen `.leinwand` und `.form`, falls die Randfarbe später
    wechseln soll (etwa bei einer Auswahl).

    ⚠⚠ **Nur für Kästen, die ohnehin die volle Breite bekommen — nie für kleine
    Elemente.**

    Der Inhalt sitzt per `create_window` auf der Leinwand und zählt damit
    **nicht** zur Wunschgröße des Kastens. Ein `rundrahmen` weiß also nicht,
    wie groß er sein müsste: Er dehnt sich auf den verfügbaren Platz und bleibt
    in der Höhe auf seinem Anfangswert, bis ihn jemand nachzieht.

    Bei einer Karte über die volle Breite fällt das nicht auf — genau dafür ist
    er gebaut. Bei allem, was seine eigene Größe haben soll, ist es der falsche
    Baustein: Aus kompakten Etiketten wurden Balken über die halbe Karte, ein
    Statusstreifen erschien als leerer Rahmen ohne Inhalt. Beides am 26.08.2026
    im Serverstatus, und beides schon am Vormittag desselben Tages an anderer
    Stelle.

    **Für kleine Elemente ein schlichtes `tk.Label` mit `bg` und `padx/pady`
    nehmen.** Eckig, aber richtig bemessen.
    """
    grundfarbe = grundfarbe or eltern.cget('bg')
    halter = tk.Frame(eltern, bg=grundfarbe)
    leinwand = tk.Canvas(halter, bg=grundfarbe, highlightthickness=0, bd=0,
                         height=10)
    leinwand.pack(fill='both', expand=True)
    innen = tk.Frame(leinwand, bg=grund)
    form = _rundes_rechteck(leinwand, 1, 1, 100, 100, radius=radius,
                            fill=grund, outline=rand, width=1)
    # ⚠ Ein per `create_window` eingesetztes Widget liegt in Tk IMMER über
    # allem Gemalten — die Zeichenreihenfolge gilt dafür nicht. Säße der Inhalt
    # bündig in der Ecke, deckte sein rechteckiger Hintergrund die Rundung ab,
    # und der Kasten sähe trotz gemaltem Bogen eckig aus. Deshalb rückt der
    # Inhalt um die halbe Rundung ein; dort bleibt der Bogen frei.
    # Der ganze Radius, nicht die Hälfte: Bei halbem Einzug deckt der Inhalt
    # die obere Hälfte des Bogens ab, und im Kasten sitzt sichtbar eine zweite,
    # eckige Kante — das sah nach doppeltem Rahmen aus.
    einzug = radius
    fenster_id = leinwand.create_window(einzug, einzug, window=innen,
                                        anchor='nw')

    def nachziehen(_=None):
        # ⚠ Solange nichts gezeichnet ist, meldet `winfo_width` eine 1. Dann
        # die gewünschte Breite nehmen — sonst bliebe das Rechteck auf seinen
        # Anfangskoordinaten stehen, seine Rundungen lägen außerhalb der
        # Leinwand, und der Kasten sähe wieder eckig aus. Genau das ist bei den
        # schmalen Zahlenfeldern passiert.
        breite = leinwand.winfo_width()
        if breite < 10:
            breite = leinwand.winfo_reqwidth()
        hoehe = innen.winfo_reqheight()
        if breite < 10:
            return
        leinwand.configure(height=hoehe + einzug * 2)
        leinwand.itemconfigure(fenster_id, width=breite - einzug * 2)
        leinwand.coords(form, *ecken(1, 1, breite - 1,
                                     hoehe + einzug * 2 - 1, radius))

    innen.bind('<Configure>', nachziehen)
    leinwand.bind('<Configure>', nachziehen)
    leinwand.bind('<Map>', nachziehen)
    # Merkmal für die Randprüfung: Dieser Rahmen wird bewusst auf die
    # Kastenbreite gezwungen — sein Wunsch nach mehr Platz ist kein Fehler,
    # der Text darin bricht um. Ohne die Markierung meldet jede Karte einen
    # Fehlalarm.
    innen.auf_mass_gesetzt = True
    innen.nachziehen = nachziehen
    innen.halter = halter
    innen.leinwand = leinwand
    innen.form = form
    return innen


def rundes_feld(eltern, textvariable, schrift, grund, rand, akzent, fg,
                breite=None, **kw):
    """Ein Eingabefeld mit runden Ecken — überall im Programm dasselbe.

    Das Feld selbst bleibt ein gewöhnliches `Entry` (nur so lässt sich tippen),
    aber ohne eigenen Rand; den runden Rand malt die Leinwand darunter. Beim
    Hineinklicken wechselt der Rand auf die Akzentfarbe, damit man sieht, wo
    man schreibt.
    """
    schrift = _als_schrift(schrift)
    radius = 8
    polster = 6
    hoehe = schrift.metrics('linespace') + polster * 2
    leinwand = tk.Canvas(eltern, height=hoehe, bg=eltern.cget('bg'),
                         highlightthickness=0, bd=0)
    form = _rundes_rechteck(leinwand, 1, 1, 100, hoehe - 1, radius=radius,
                            fill=grund, outline=rand, width=1)
    if textvariable is not None:
        kw['textvariable'] = textvariable
    feld = tk.Entry(leinwand, bg=grund, fg=fg, font=schrift, relief='flat',
                    bd=0, highlightthickness=0, insertbackground=fg, **kw)
    fenster_id = leinwand.create_window(polster + 2, hoehe / 2.0, window=feld,
                                        anchor='w')

    def nachziehen(_=None):
        # ⚠ Der Rückruf aus `after(0, …)` kann drankommen, wenn die Leinwand
        # längst zerstört ist — beim Seitenwechsel passiert genau das.
        try:
            if not leinwand.winfo_exists():
                return
        except tk.TclError:
            return
        b = leinwand.winfo_width()
        if b < 10:
            b = leinwand.winfo_reqwidth()
        if b < 10:
            return
        try:
            leinwand.coords(form, *ecken(1, 1, b - 1, hoehe - 1, radius))
            leinwand.itemconfigure(fenster_id, width=b - (polster + 2) * 2)
        except tk.TclError:
            pass

    leinwand.bind('<Configure>', nachziehen)
    leinwand.bind('<Map>', nachziehen)
    if breite:
        # Feste Breite: so viele Ziffern plus Luft. Ohne das zieht `fill='x'`
        # der Zeile das Feld über die halbe Seite.
        leinwand.configure(width=schrift.measure('0') * breite + polster * 4)
    feld.halter = leinwand
    leinwand.after(0, nachziehen)
    feld.bind('<FocusIn>',
              lambda e: leinwand.itemconfigure(form, outline=akzent), add='+')
    feld.bind('<FocusOut>',
              lambda e: leinwand.itemconfigure(form, outline=rand), add='+')
    return feld



def rundes_textfeld(eltern, schrift, grund, rand, akzent, fg, zeilen=4, **kw):
    """Das mehrzeilige Gegenstück zu `rundes_feld` — gleiche Optik.

    ⚠⚠ **Wofür.** Ein `Entry` zeigt immer nur einen Ausschnitt: Wer zwei Sätze
    tippt, sieht das Ende und nicht mehr, was er geschrieben hat. Für eine
    Fehlerbeschreibung ist genau das falsch — man will beim Absenden noch
    einmal lesen können, was man da meldet. Am 05.09.2026 gemeldet:
    „macht es nicht Sinn das Fenster … größer und unter den Text zu machen,
    das der Melder das was er eintippt auch noch selber lesen kann?"

    ⚠ **Warum eine eigene Funktion und kein `Text` an Ort und Stelle.**
    Gleiche Dinge sehen im Programm gleich aus (Projektregel „Symmetrie"). Ein
    nacktes `Text` hätte eckige Ecken und einen anderen Rand als jedes andere
    Eingabefeld — auf derselben Seite, direkt unter einem runden Namensfeld.
    Der Rand wird deshalb genauso gemalt und wechselt beim Hineinklicken
    ebenso auf die Akzentfarbe.

    Rückgabe ist das `Text` selbst; die Leinwand hängt als `.halter` daran —
    dieselbe Verabredung wie bei `rundes_feld`, damit beide sich gleich
    einbauen lassen.
    """
    schrift = _als_schrift(schrift)
    radius = 8
    polster = 8
    hoehe = schrift.metrics('linespace') * zeilen + polster * 2
    leinwand = tk.Canvas(eltern, height=hoehe, bg=eltern.cget('bg'),
                         highlightthickness=0, bd=0)
    form = _rundes_rechteck(leinwand, 1, 1, 100, hoehe - 1, radius=radius,
                            fill=grund, outline=rand, width=1)
    feld = tk.Text(leinwand, bg=grund, fg=fg, font=schrift, relief='flat',
                   bd=0, highlightthickness=0, insertbackground=fg,
                   height=zeilen, wrap='word', padx=0, pady=0, **kw)
    fenster_id = leinwand.create_window(polster + 2, polster, window=feld,
                                        anchor='nw')

    def nachziehen(_=None):
        # ⚠ Wie bei `rundes_feld`: Der Rückruf aus `after(0, …)` kann
        # drankommen, wenn die Leinwand beim Seitenwechsel längst weg ist.
        try:
            if not leinwand.winfo_exists():
                return
        except tk.TclError:
            return
        b = leinwand.winfo_width()
        if b < 10:
            b = leinwand.winfo_reqwidth()
        if b < 10:
            return
        try:
            leinwand.coords(form, *ecken(1, 1, b - 1, hoehe - 1, radius))
            leinwand.itemconfigure(fenster_id, width=b - (polster + 2) * 2,
                                   height=hoehe - polster * 2)
        except tk.TclError:
            pass

    leinwand.bind('<Configure>', nachziehen)
    leinwand.bind('<Map>', nachziehen)
    feld.halter = leinwand
    leinwand.after(0, nachziehen)
    feld.bind('<FocusIn>',
              lambda e: leinwand.itemconfigure(form, outline=akzent), add='+')
    feld.bind('<FocusOut>',
              lambda e: leinwand.itemconfigure(form, outline=rand), add='+')
    return feld


def _als_schrift(schrift):
    """Eine Schrift als messbares Objekt.

    Die älteren Fenster geben ihre Schrift als Tupel `('Helvetica', 10)`
    weiter — damit lässt sich zeichnen, aber nicht messen. Ein gemalter Knopf
    braucht aber die Breite des Wortes, sonst schneidet er es ab. Also hier
    einmal umwandeln, statt an jeder Stelle daran zu denken.
    """
    if isinstance(schrift, (tuple, list)):
        return tkfont.Font(family=schrift[0], size=schrift[1],
                           weight=schrift[2] if len(schrift) > 2 else 'normal')
    return schrift



def rundbalken(eltern, hoehe, anteil, grund, leer, voll, breite=None):
    """Ein Fortschrittsbalken mit runden Enden.

    Zwei ineinandergeschobene Rahmen wären einfacher, hätten aber scharfe
    Kanten — im Rest des Programms ist nichts scharfkantig. Also wieder eine
    Leinwand: eine Rille in ganzer Länge, darüber der gefüllte Teil.

    Der gefüllte Teil zieht mit, wenn sich die Breite ändert (Fenster größer,
    Seitenleiste ein- oder ausgeklappt) — deshalb `<Configure>` statt einer
    einmal ausgerechneten Pixelzahl.
    """
    r = hoehe / 2.0
    c = tk.Canvas(eltern, height=hoehe, bg=grund, highlightthickness=0, bd=0)
    if breite:
        c.configure(width=breite)
    rille = _rundes_rechteck(c, 0, 0, 100, hoehe, radius=r, fill=leer,
                             outline='')
    fuellung = _rundes_rechteck(c, 0, 0, 10, hoehe, radius=r, fill=voll,
                                outline='')

    def nachziehen(_=None):
        b = c.winfo_width()
        if b < 4:
            return
        c.coords(rille, *ecken(0, 0, b, hoehe, r))
        if anteil <= 0:
            c.itemconfigure(fuellung, state='hidden')
            return
        c.itemconfigure(fuellung, state='normal')
        # Mindestens so breit wie hoch: Ein Balken bei 1 % wäre sonst ein
        # Strich, den man für einen Zeichenfehler hält.
        voll_breite = max(hoehe, b * anteil)
        c.coords(fuellung, *ecken(0, 0, voll_breite, hoehe, r))

    c.bind('<Configure>', nachziehen)
    return c


def _eigenes_rollen(vom, bis):
    """Ein Textfeld zwischen `vom` und `bis`, das selbst rollen kann — oder None.

    Geprüft wird, ob überhaupt etwas zu rollen **ist**: Ein Feld, dessen Inhalt
    hineinpasst, meldet `(0.0, 1.0)`. Dort soll weiter die Seite rollen, sonst
    bliebe der Zeiger über einem kurzen Feld hängen und nichts bewegte sich.
    """
    knoten = vom
    while knoten is not None and knoten is not bis:
        if isinstance(knoten, tk.Text):
            try:
                oben, unten = knoten.yview()
                if (unten - oben) < 0.999:
                    return knoten
            except tk.TclError:
                pass
        knoten = getattr(knoten, 'master', None)
    return None


def rad_anschliessen(leinwand):
    """Das Mausrad an eine Rollfläche hängen — für das ganze Fenster.

    ⚠ Zwei Fehler steckten hier, und beide zusammen ließen das Rad wirkungslos
    aussehen, während der Rollbalken von Hand funktionierte:

    1. **Die Rechnung.** Vorher stand hier `int(-1 * e.delta / 120)`. Windows
       meldet ±120, Linux meldet sich über Button-4/5 — beides ging auf.
       macOS meldet aber **±1**, und `int(-1/120)` ist **0**: kein Ausschlag.
       Deshalb zählt jetzt nur die Richtung, nie der Betrag.

    2. **Die Bindung.** Vorher hingen die Ereignisse an drei Widgets
       (Leinwand, Innenrahmen, Polster). Tk schickt das Rad aber an das
       Element **unter dem Zeiger**, und das ist fast immer eine Beschriftung
       oder ein Kasten darin — dort war nichts gebunden. Also greift die
       Bindung jetzt am ganzen Fenster, und der Griff sucht sich die
       Rollfläche unter dem Zeiger.

    3. **Und der Grund, warum das trotzdem nicht wirkte:** Die Bauplan-Liste
       rief `bind_all` **ohne** `add='+'` auf. Das ersetzt jede vorher
       gesetzte Bindung im ganzen Fenster — und weil die Liste die Startseite
       ist, war die Bindung der Seiten sofort wieder weg. Danach rollte das
       Rad überall nur noch die Liste, auch wenn die gar nicht zu sehen war.
       Deshalb hängen jetzt **alle** Rollflächen an dieser einen Stelle.

    4. **Trackpad.** Gemessen mit `tools/rad_messen.py`: Vom Trackpad kommt
       **kein einziges** `<MouseWheel>` an — nicht etwa ein zu kleiner Wert,
       sondern gar nichts. Seit Tk 8.7 gibt es dafür ein eigenes Ereignis,
       `<TouchpadScroll>`, und erst das liefert die Streichgesten. Es feuert
       viel häufiger als eine Radraste und trägt beide Richtungen in **einer**
       Zahl: untere 16 Bit waagerecht, obere 16 Bit senkrecht.

       Ältere Tk-Versionen (8.6, verbreitet unter Linux) kennen das Ereignis
       nicht — dort wirft das Binden einen Fehler, der abgefangen wird. Dort
       melden sich Trackpads ohnehin als Button-4/5.

       Weil beide Wege kleine Beträge liefern, werden sie **aufaddiert**, bis
       eine ganze Zeile zusammenkommt; der Rest bleibt für das nächste
       Ereignis stehen.
    """
    wurzel = leinwand.winfo_toplevel()
    if not hasattr(wurzel, 'rollflaechen'):
        wurzel.rollflaechen = []
        # Was noch keine ganze Zeile ergeben hat, wartet hier auf den Rest.
        angesammelt = {'wert': 0.0}

        def schritte_aus(e):
            """Wie viele Zeilen sollen es sein? Negativ heißt nach oben."""
            nummer = getattr(e, 'num', 0)
            if nummer == 4:                      # Linux: Rad nach oben
                return -1
            if nummer == 5:                      # Linux: Rad nach unten
                return 1
            betrag = float(getattr(e, 'delta', 0) or 0)
            if betrag == 0:
                return 0
            if abs(betrag) >= 120:               # Windows: eine Raste = 120
                betrag /= 120.0
            angesammelt['wert'] += betrag
            ganze = int(angesammelt['wert'])     # schneidet Richtung null ab
            angesammelt['wert'] -= ganze
            return -ganze                        # nach oben = negativ

        def flaeche_unter(e):
            """Was unter dem Mauszeiger gerollt werden soll — oder nichts.

            ⚠ **Ein Textfeld rollt sich selbst.** Vorher zählten nur die
            registrierten Rollflächen; ein `tk.Text` ist keine, also ging das
            Rad an die Seite dahinter. Auf der Diagnose-Seite hieß das: Erst
            die ganze Seite nach unten schieben, und **dann** erst ließ sich im
            Bericht rollen. Am 28.08.2026 fiel auf, nachdem sein Bruder
            dasselbe gemeldet hatte: „in dem Fehlerbericht-Fenster kann man
            erst scrollen, nachdem die Diagnose-Seite nach unten gescrollt
            ist."

            Wie im Browser: Was unter dem Zeiger liegt und rollen kann, rollt
            — die Seite bewegt man daneben.
            """
            unter = wurzel.winfo_containing(e.x_root, e.y_root)
            erstes = unter
            while unter is not None:
                if unter in wurzel.rollflaechen:
                    # Liegt auf dem Weg dorthin ein Textfeld, das ueberlaeuft,
                    # gehoert ihm das Rad.
                    eigenes = _eigenes_rollen(erstes, unter)
                    return eigenes if eigenes is not None else unter
                unter = getattr(unter, 'master', None)
            return None

        def rollen(e):
            ziel = flaeche_unter(e)
            if ziel is None:
                return
            schritte = schritte_aus(e)
            if not schritte:
                return
            try:
                ziel.yview_scroll(schritte, 'units')
            except tk.TclError:
                pass

        def streichen(e):
            """Trackpad: beide Richtungen stecken gepackt in einer Zahl."""
            ziel = flaeche_unter(e)
            if ziel is None:
                return
            roh = int(getattr(e, 'delta', 0) or 0)
            senkrecht = (roh >> 16) & 0xFFFF
            if senkrecht >= 0x8000:          # als vorzeichenbehaftet lesen
                senkrecht -= 0x10000
            if not senkrecht:
                return
            # Ein Streich meldet viele kleine Schritte. `TEILER` bestimmt, wie
            # weit eine Geste trägt — kleiner heißt schneller.
            angesammelt['wert'] += senkrecht / float(TRACKPAD_TEILER)
            ganze = int(angesammelt['wert'])
            angesammelt['wert'] -= ganze
            if not ganze:
                return
            # ⚠ Kein Minus wie beim Rad: `<TouchpadScroll>` zählt andersherum
            # als `<MouseWheel>`. Mit dem Vorzeichen des Rades rollte die Liste
            # genau falsch herum. Die vom Nutzer eingestellte Richtung
            # („natürliches Scrollen") hat das System da schon eingerechnet.
            try:
                ziel.yview_scroll(ganze, 'units')
            except tk.TclError:
                pass

        for ereignis in ('<MouseWheel>', '<Button-4>', '<Button-5>'):
            wurzel.bind_all(ereignis, rollen, add='+')
        try:
            wurzel.bind_all('<TouchpadScroll>', streichen, add='+')
        except tk.TclError:
            # Tk 8.6 und älter kennen das Ereignis nicht. Dort melden sich
            # Trackpads als Button-4/5, also fehlt nichts.
            pass

    if leinwand not in wurzel.rollflaechen:
        wurzel.rollflaechen.append(leinwand)



def rundleiste(eltern, leinwand, grund=None, breite=10):
    """Eine Rollleiste mit runden Enden — statt der des Betriebssystems.

    ⚠ `tk.Scrollbar` ist das einzige Bedienelement, das sich nicht einfärben
    lässt: Tk reicht es an das System durch. Unter Linux ist sie grau, auf dem
    Mac hellweiß — und damit der einzige Fleck im Fenster, der aus dem Bild
    fällt. Die Entwurfsvorschau hatte dort eine schmale, abgerundete Leiste in
    `#2b3547`; genau die wird hier nachgebaut.

    Bedienung wie gewohnt: ziehen, und ein Klick daneben springt eine Seite
    weiter. `leinwand` ist die Rollfläche, an der sie hängt.
    """
    grund = grund or BG
    # ⚠⚠ **Ein Rollbalken, den man nicht sieht, ist keiner.** Der Griff war
    # `#2b3547` — auf der Fläche einer aufgeklappten Auswahlliste (`#161c28`)
    # ergibt das einen Kontrast von **1,6 : 1**. Am 30.08.2026 gemeldet: „ah es
    # ist scrollbar und funktioniert, aber ich sehe keinen Rollbalken, und da
    # ich mich grad mal dumm stelle wie normale User — wenn ich es nicht sehe,
    # wie sollen es andere dann checken."
    #
    # Jetzt 2,9 : 1 auf der Liste und 3,6 : 1 auf einer Seite, unter der Maus
    # 3,8 : 1. Dazu eine sichtbare Rille: Erst sie zeigt, dass es überhaupt
    # eine Bahn gibt, an der etwas entlangläuft — der Griff allein sieht aus
    # wie ein Strich.
    rille_farbe, griff_farbe, griff_hell = '#0b0e14', '#5a6b85', '#7d90ad'
    r = breite / 2.0

    c = tk.Canvas(eltern, width=breite, bg=grund, highlightthickness=0, bd=0)
    rille = c.create_rectangle(0, 0, breite, 10, fill=rille_farbe, outline='')
    griff = _rundes_rechteck(c, 0, 0, breite, 30, radius=r,
                             fill=griff_farbe, outline='')
    c.auf_mass_gesetzt = True        # die Randprüfung soll sie nicht melden

    lage = {'anfang': 0.0, 'ende': 1.0, 'griff_ab': 0, 'zieht': False}

    def griff_lage():
        """Wo der Griff **wirklich** gezeichnet ist — als (oben, unten, hoehe).

        ⚠ Diese Rechnung muss an EINER Stelle stehen. Vorher zeichnete
        `nachziehen` den Griff mit einer Mindesthöhe, während `springen` mit der
        rechnerischen Höhe prüfte, ob man ihn getroffen hat. Bei 722 Bauplänen ist
        der Griff rechnerisch rund zwölf Pixel hoch und gezeichnet vierundzwanzig:
        Wer die untere Hälfte des sichtbaren Griffs anfasste, galt als „daneben“ —
        die Leiste sprang, statt sich ziehen zu lassen. Sie sah also greifbar aus
        und war es nicht.
        """
        hoehe = c.winfo_height() or 1
        anfang, ende = lage['anfang'], lage['ende']
        oben = anfang * hoehe
        # Der Griff bleibt greifbar, auch wenn 700 Baupläne in der Liste
        # stehen und er rechnerisch drei Pixel hoch wäre.
        unten = max(oben + breite * 2.4, ende * hoehe)
        if unten > hoehe:                 # am unteren Ende nach oben schieben
            oben, unten = max(0.0, hoehe - (unten - oben)), hoehe
        return oben, unten, hoehe

    def nachziehen(*_):
        hoehe = c.winfo_height()
        if hoehe < 4:
            return
        c.coords(rille, 0, 0, breite, hoehe)
        if lage['ende'] - lage['anfang'] >= 0.999:   # nichts zu rollen
            c.itemconfigure(griff, state='hidden')
            return
        c.itemconfigure(griff, state='normal')
        oben, unten, _ = griff_lage()
        c.coords(griff, *ecken(0, oben, breite, unten, r))

    def setzen(anfang, ende):
        """Ruft Tk auf, wenn sich der sichtbare Ausschnitt ändert."""
        lage['anfang'], lage['ende'] = float(anfang), float(ende)
        nachziehen()

    def springen(e):
        oben, unten, hoehe = griff_lage()
        spanne = lage['ende'] - lage['anfang']
        if oben <= e.y <= unten:                  # auf dem Griff: ziehen
            lage['zieht'] = True
            lage['griff_ab'] = e.y - oben
            return
        ziel = max(0.0, min(1.0, (e.y / hoehe) - spanne / 2.0))
        leinwand.yview_moveto(ziel)

    def ziehen(e):
        """Den Griff mitnehmen.

        Gerechnet wird über den **Weg, den der Griff zurücklegen kann** — also die
        Leistenhöhe minus Griffhöhe. Vorher wurde durch die volle Leistenhöhe
        geteilt; weil der Griff eine Mindesthöhe hat, blieb das letzte Stück der
        Liste unerreichbar: Man zog bis ganz nach unten und war trotzdem nicht am
        Ende.
        """
        if not lage['zieht']:
            return
        oben, unten, hoehe = griff_lage()
        weg = max(1.0, hoehe - (unten - oben))
        spanne = lage['ende'] - lage['anfang']
        anteil = (e.y - lage['griff_ab']) / weg
        leinwand.yview_moveto(max(0.0, min(1.0, anteil * max(0.0, 1.0 - spanne))))

    def loslassen(_=None):
        lage['zieht'] = False

    def rein(_=None):
        c.itemconfigure(griff, fill=griff_hell)

    def raus(_=None):
        if not lage['zieht']:
            c.itemconfigure(griff, fill=griff_farbe)

    c.bind('<Configure>', nachziehen)
    c.bind('<Button-1>', springen)
    c.bind('<B1-Motion>', ziehen)
    c.bind('<ButtonRelease-1>', loslassen)
    c.bind('<Enter>', rein)
    c.bind('<Leave>', raus)
    # ⚠ `set` heißt hier englisch, weil Tk selbst diesen Namen aufruft:
    # `leinwand.configure(yscrollcommand=leiste.set)`. `setzen` steht daneben,
    # damit der Rest des Programms bei seiner Sprache bleiben kann.
    c.set = setzen
    c.setzen = setzen
    return c


# --- Discord-Zeichen ------------------------------------------------------
# Die Umrisse stammen aus dem offiziellen SVG (svgrepo, viewBox 0 -28.5 256 256)
# und sind auf 0..1 normiert. Die Bezier-Kurven des Pfades wurden dabei in
# Strecken aufgelöst — Tk kennt keine Kurven, zeichnet aber Streckenzüge in
# jeder Größe sauber.
#
# ⚠ **Warum kein Bild und kein Schriftzeichen:**
#
#   * Als Zeichen geht es nicht. Ein Discord-Logo gibt es in Unicode nicht, und
#     die naheliegenden Sprechblasen (`U+1F4AC`, `U+1F5E8`) liegen außerhalb der
#     Grundebene — genau die Falle, vor der weiter oben gewarnt wird: Im Fenster
#     stünde ein Fragezeichen, und auffallen würde es erst im laufenden
#     Programm.
#   * Als PNG geht es schlecht. Tk lädt PNG zwar mit Bordmitteln, kann sie ohne
#     Fremdpakete aber nur **ganzzahlig** skalieren (`subsample`, `zoom`). Bei
#     vier Schriftstufen käme genau eine Größe sauber heraus und der Rest
#     ausgefranst. Dazu käme eine Datei, die ins Paket muss.
#
# Als Vektor ist es in jeder Größe scharf, braucht keine Datei und hängt an
# keiner Schriftart. Ein erster Versuch hatte die Form nur **angedeutet**;
# am 26.08.2026 gemeldet dazu: „und wieso nimmst du nicht das original logo, was
# schärfer wäre und nicht so pixelig?" — zu Recht.
_DC_UMRISS = (

    (0.8471, 0.1762), (0.8144, 0.1617), (0.7809, 0.1487), (0.7468, 0.1371),
    (0.7121, 0.1270), (0.6767, 0.1184), (0.6408, 0.1113), (0.6362, 0.1198),
    (0.6316, 0.1289), (0.6269, 0.1383), (0.6224, 0.1479), (0.6182, 0.1573),
    (0.6144, 0.1662), (0.5760, 0.1614), (0.5377, 0.1585), (0.4995, 0.1575),
    (0.4615, 0.1585), (0.4235, 0.1614), (0.3857, 0.1662), (0.3819, 0.1573),
    (0.3776, 0.1479), (0.3730, 0.1383), (0.3683, 0.1289), (0.3636, 0.1198),
    (0.3590, 0.1113), (0.3230, 0.1184), (0.2876, 0.1270), (0.2529, 0.1371),
    (0.2187, 0.1488), (0.1852, 0.1618), (0.1525, 0.1763), (0.0950, 0.2746),
    (0.0521, 0.3721), (0.0228, 0.4689), (0.0058, 0.5650), (0.0000, 0.6606),
    (0.0042, 0.7557), (0.0473, 0.7860), (0.0900, 0.8123), (0.1323, 0.8351),
    (0.1742, 0.8547), (0.2159, 0.8713), (0.2573, 0.8853), (0.2673, 0.8712),
    (0.2769, 0.8567), (0.2861, 0.8420), (0.2950, 0.8270), (0.3034, 0.8117),
    (0.3115, 0.7961), (0.2967, 0.7902), (0.2821, 0.7839), (0.2677, 0.7772),
    (0.2536, 0.7700), (0.2397, 0.7625), (0.2261, 0.7546), (0.2297, 0.7519),
    (0.2332, 0.7492), (0.2367, 0.7464), (0.2402, 0.7437), (0.2436, 0.7409),
    (0.2470, 0.7380), (0.3304, 0.7701), (0.4152, 0.7893), (0.5007, 0.7957),
    (0.5861, 0.7893), (0.6704, 0.7701), (0.7529, 0.7380), (0.7564, 0.7409),
    (0.7598, 0.7437), (0.7633, 0.7464), (0.7668, 0.7492), (0.7703, 0.7519),
    (0.7739, 0.7546), (0.7602, 0.7625), (0.7463, 0.7701), (0.7322, 0.7772),
    (0.7178, 0.7840), (0.7032, 0.7903), (0.6884, 0.7962), (0.6964, 0.8117),
    (0.7048, 0.8270), (0.7137, 0.8420), (0.7229, 0.8568), (0.7325, 0.8713),
    (0.7426, 0.8854), (0.7840, 0.8714), (0.8257, 0.8548), (0.8677, 0.8352),
    (0.9100, 0.8124), (0.9527, 0.7860), (0.9957, 0.7557), (0.9998, 0.6482),
    (0.9916, 0.5453), (0.9717, 0.4469), (0.9406, 0.3527), (0.8988, 0.2625),
    (0.8471, 0.1762),
)

_DC_AUGE_LINKS = (

    (0.3339, 0.6390), (0.3101, 0.6354), (0.2886, 0.6250), (0.2704, 0.6090),
    (0.2563, 0.5882), (0.2472, 0.5638), (0.2440, 0.5368), (0.2472, 0.5097),
    (0.2561, 0.4853), (0.2701, 0.4645), (0.2882, 0.4485), (0.3098, 0.4381),
    (0.3339, 0.4344), (0.3581, 0.4381), (0.3797, 0.4485), (0.3980, 0.4645),
    (0.4120, 0.4853), (0.4209, 0.5097), (0.4238, 0.5368), (0.4206, 0.5638),
    (0.4117, 0.5882), (0.3977, 0.6090), (0.3795, 0.6250), (0.3580, 0.6354),
    (0.3339, 0.6390),
)

_DC_AUGE_RECHTS = (

    (0.6661, 0.6390), (0.6423, 0.6354), (0.6209, 0.6250), (0.6026, 0.6090),
    (0.5885, 0.5882), (0.5794, 0.5638), (0.5762, 0.5368), (0.5794, 0.5097),
    (0.5884, 0.4853), (0.6023, 0.4645), (0.6205, 0.4485), (0.6420, 0.4381),
    (0.6661, 0.4344), (0.6903, 0.4381), (0.7120, 0.4485), (0.7302, 0.4645),
    (0.7443, 0.4853), (0.7531, 0.5097), (0.7560, 0.5368), (0.7528, 0.5638),
    (0.7439, 0.5882), (0.7299, 0.6090), (0.7118, 0.6250), (0.6902, 0.6354),
    (0.6661, 0.6390),
)


def discord_zeichen(leinwand, x, mitte, hoehe, farbe):
    """Das Discord-Zeichen, gezeichnet aus den Originalumrissen.

    `x` ist die linke Kante, `mitte` die senkrechte Mitte, `hoehe` der
    verfügbare Platz. Alle Punkte sind Anteile davon, das Zeichen wächst also
    mit der Schriftgröße mit.
    """
    h = max(9.0, hoehe * 0.82)
    b = h                      # der viewBox ist quadratisch
    lx = x
    oy = mitte - h / 2.0

    def strecke(punkte):
        flach = []
        for ax, ay in punkte:
            flach.append(lx + ax * b)
            flach.append(oy + ay * h)
        return flach

    leinwand.create_polygon(strecke(_DC_UMRISS), fill=farbe, outline=farbe)
    # Die Augen sind im SVG Aussparungen derselben Fläche. Tk kennt keine
    # Löcher, deshalb werden sie in der Farbe des Untergrunds darübergelegt.
    grund = leinwand['bg']
    for auge in (_DC_AUGE_LINKS, _DC_AUGE_RECHTS):
        leinwand.create_polygon(strecke(auge), fill=grund, outline=grund)


# Von am 26.08.2026 gemeldet bestätigt. ⚠ Wer sie ändert, prüft vorher, dass die
# Seite wirklich erreichbar ist: Ein Knopf, der ins Leere führt, ist schlimmer
# als keiner — wer ihn drückt, hält das Werkzeug für kaputt.
KOFI_ADRESSE = 'https://ko-fi.com/xharig'


def kaffee_zeichen(leinwand, x, mitte, hoehe, farbe):
    """Eine Kaffeetasse — für den Ko-fi-Knopf.

    ⚠ Gezeichnet, nicht getippt. Die Tassen-Zeichen in Unicode (`U+2615` ☕,
    `U+1F375`) liegen entweder außerhalb der Grundebene oder werden von der
    Oberflächenschrift als **farbiges Emoji** gerendert — beides passt nicht: Das
    eine erscheint als Fragezeichen, das andere sprengt die einfarbige Leiste.
    Dieselbe Überlegung wie beim Discord-Zeichen, siehe dort.

    ⚠ **Der erste Entwurf hatte einen Dampffaden**, und der war bei Knopfgröße
    nicht mehr zu sehen — ein Strich von einem Pixel Breite verschwindet. Für
    kleine Zeichen gilt: **wenige, kräftige Formen.** Was man wegkürzen kann,
    ohne dass das Motiv unklar wird, gehört weg. Bei einer Tasse tragen Becher
    und Henkel, der Dampf ist Zierde.
    """
    h = max(9.0, hoehe * 0.72)
    b = h * 1.05
    lx = x
    oy = mitte - h / 2.0

    # Der Henkel — zuerst, damit der Becher ihn sauber überdeckt. Kräftig
    # genug, dass er auch bei zwölf Pixeln noch trägt.
    leinwand.create_arc(lx + b * 0.60, oy + h * 0.28,
                        lx + b * 1.02, oy + h * 0.74,
                        start=270, extent=180, style='arc',
                        outline=farbe, width=max(2, int(h * 0.14)))

    # Der Becher: oben breit, nach unten leicht zulaufend.
    leinwand.create_polygon(
        lx + b * 0.06, oy + h * 0.24,
        lx + b * 0.74, oy + h * 0.24,
        lx + b * 0.64, oy + h * 0.92,
        lx + b * 0.16, oy + h * 0.92,
        fill=farbe, outline=farbe)

    # Ein abgesetzter Streifen als Kaffeespiegel — das macht aus dem Umriss
    # erst eine gefüllte Tasse.
    grund = leinwand['bg']
    leinwand.create_rectangle(lx + b * 0.14, oy + h * 0.33,
                              lx + b * 0.66, oy + h * 0.41,
                              fill=grund, outline=grund)

    # Die Untertasse — ein flacher Balken, der die Tasse auf den Boden stellt.
    leinwand.create_rectangle(lx + b * 0.02, oy + h * 0.92,
                              lx + b * 0.78, oy + h * 1.00,
                              fill=farbe, outline=farbe)


def rundknopf(eltern, text, tat, schrift, grund, fuellung, rand, fg,
              radius=6, polster=(10, 5), cursor='hand2', malen=None):
    """Ein klickbarer Knopf mit runden Ecken — der Standard im ganzen Programm.

    Ein `Label` mit Hintergrundfarbe wäre einfacher, sähe aber überall eckig
    aus; das Programm hat aber genau eine Formensprache. Deshalb wieder eine
    kleine Leinwand mit gemaltem Rechteck.

    Am Rückgabewert hängt `.setzen(fuellung, rand, fg)` — damit lässt sich der
    Knopf später umfärben (an/aus, ausgewählt/nicht), ohne ihn neu zu bauen.

    ⚠ **Das Rechteck wird bei jeder Größenänderung neu gemalt.** Vorher entstand
    es genau einmal in Textbreite und blieb so. Wer den Knopf mit `fill='x'`
    streckte, bekam ein breiteres Canvas mit einem schmalen Rechteck darin — der
    Knopf sah je nach Textlänge unterschiedlich breit aus, obwohl beide dieselbe
    Anweisung hatten. Aufgefallen an zwei Knöpfen untereinander (gemeldet,
    26.08.2026): „Discord Button sollte die Gleiche Breite haben wie SC Starten".

    `malen` ist eine Funktion `(leinwand, x, mitte, hoehe, farbe)`, die links im
    Knopf ein Symbol zeichnet — für alles, wofür es kein brauchbares Zeichen in
    der Grundebene gibt (siehe die Warnung weiter oben zu `🗀` und `⇅`). Sie wird
    bei jedem Neuzeichnen erneut gerufen und muss ihre eigenen Formen anlegen;
    aufgeräumt wird vorher.
    """
    schrift = _als_schrift(schrift)
    symbolbreite = 0
    if malen:
        # Platz für das Symbol plus Abstand — an der Schrifthöhe bemessen,
        # damit es mit der Schriftgröße mitwächst.
        symbolbreite = schrift.metrics('linespace') + 8
    breite = schrift.measure(text) + polster[0] * 2 + symbolbreite
    hoehe = schrift.metrics('linespace') + polster[1] * 2
    c = tk.Canvas(eltern, width=breite, height=hoehe, bg=grund,
                  highlightthickness=0, bd=0, cursor=cursor)

    zustand = {'fuellung': fuellung, 'rand': rand, 'fg': fg}

    def zeichnen(b=None, h=None):
        b = b or int(c['width'])
        h = h or int(c['height'])
        c.delete('all')
        c.form = _rundes_rechteck(c, 1, 1, b - 1, h - 1, radius=radius,
                                  fill=zustand['fuellung'],
                                  outline=zustand['rand'], width=1)
        if malen:
            malen(c, polster[0], h / 2.0, h - polster[1] * 2, zustand['fg'])
        # ⚠ Der Text sitzt in der Mitte des **Restes**, nicht der ganzen
        # Fläche. Sonst rückt ein Symbol links die Beschriftung nach rechts aus
        # der Mitte, und zwei Knöpfe untereinander stehen krumm.
        mitte_x = (b + symbolbreite) / 2.0 if malen else b / 2.0
        c.beschriftung = c.create_text(mitte_x, h / 2.0, text=text,
                                       fill=zustand['fg'], font=schrift,
                                       anchor='center')

    zeichnen(breite, hoehe)

    # ⚠ Nur bei echter Änderung neu malen. Tk schickt `<Configure>` auch dann,
    # wenn sich nichts an der Größe geändert hat — ein bedingungsloses
    # Neuzeichnen darin läuft im Kreis.
    letzte = {'b': breite, 'h': hoehe}

    def _gewachsen(e):
        if e.width == letzte['b'] and e.height == letzte['h']:
            return
        letzte['b'], letzte['h'] = e.width, e.height
        zeichnen(e.width, e.height)

    c.bind('<Configure>', _gewachsen)

    def setzen(fuellung=None, neuer_rand=None, neues_fg=None):
        if fuellung:
            zustand['fuellung'] = fuellung
            c.itemconfigure(c.form, fill=fuellung)
        if neuer_rand:
            zustand['rand'] = neuer_rand
            c.itemconfigure(c.form, outline=neuer_rand)
        if neues_fg:
            zustand['fg'] = neues_fg
            c.itemconfigure(c.beschriftung, fill=neues_fg)
            if malen:
                # Das Symbol traegt dieselbe Farbe wie die Schrift — neu malen
                # ist einfacher, als sich jede Einzelform zu merken.
                zeichnen()

    c.setzen = setzen
    c.ist_knopf = True          # damit tools/randpruefung.py ihn prüft
    if tat:
        c.bind('<Button-1>', lambda e: tat())
    return c


# So viele Zeilen zeigt eine aufgeklappte Auswahlliste hoechstens; alles
# darueber wird gerollt. Nicht aus Platzgruenden — der Bildschirm hat Platz —,
# sondern damit die Liste ueberschaubar bleibt und die Rollleiste sichtbar
# wird. Ohne diese Grenze reichte die Ortsliste im Bergbau (48 Eintraege) vom
# Auswahlfeld bis weit unter das Fenster.
MAX_WAHLZEILEN = 15

# So breit darf ein geschlossenes Auswahlfeld höchstens werden, in Zeichen.
# Die aufgeklappte Liste ist davon nicht betroffen — siehe `rundwahl`.
MAX_FELDZEICHEN = 18


def rundwahl(eltern, eintraege, gewaehlt, beim_waehlen, schrift, grund=None,
             breite=None):
    """Ein Auswahlfeld im Hausstil — Knopf mit ▾, der eine Liste aufklappt.

    ⚠ Warum selbst gebaut: Tk bringt `OptionMenu` und `ttk.Combobox` mit, und
    beide sind Systemelemente — auf dem Mac ein graues Aqua-Feld, unter Windows
    ein anderes, unter Linux je nach Oberfläche wieder anders. Das Programm hat
    aber genau eine Formensprache, und ein Auswahlfeld ist zu auffällig, um
    davon ausgenommen zu werden.

    `eintraege` sind Paare `(wert, beschriftung)`. Ein Eintrag mit dem Wert `''`
    ist der „alle"-Fall; ist etwas anderes gewählt, färbt sich das Feld in der
    Akzentfarbe — so sieht man auf einen Blick, dass ein Filter greift, ohne
    jede Liste aufklappen zu müssen.

    Die aufgeklappte Liste ist ein rahmenloses Fenster: Nur so kann sie über
    den Rand ihres Elternrahmens hinausragen, und genau das muss sie, wenn
    zwanzig Arten zur Wahl stehen.
    """
    grund = grund or BG
    s = _als_schrift(schrift)
    zustand = {'wert': gewaehlt, 'liste': None, 'zu_seit': 0.0, 'wachen': []}

    def beschriftung_zu(wert):
        for w, text in eintraege:
            if w == wert:
                return text
        return eintraege[0][1] if eintraege else ''

    # ⚠⚠ **Das geschlossene Feld muss NICHT so breit sein wie der längste
    # Eintrag.** Bis v3.3.0-rc40 war es das — und weil unter den 64
    # Herstellern „Musashi Industrial & Starflight Concern" steht (39 Zeichen),
    # war das Feld über 300 Pixel breit. In der Herstellung passte die vierte
    # Auswahl dadurch nicht mehr in die Zeile und wurde rechts abgeschnitten
    # („Materia…"). Am 30.08.2026 gemeldet: „obwohl der breiteste Eintrag bei
    # Hersteller ca. die Hälfte des Dropdowns benötigt."
    #
    # Die **aufgeklappte Liste** bleibt so breit, wie ihr längster Eintrag es
    # verlangt — dort ist der Platz da. Nur das Feld wird gedeckelt; ein zu
    # langer gewählter Wert bekommt am Ende ein „…".
    if breite is None:
        noetig = max(s.measure(text) for _, text in eintraege) + 42
        breite = min(noetig, s.measure('M' * MAX_FELDZEICHEN) + 42)
    hoehe = s.metrics('linespace') + 14

    def _passend(text):
        """Text so kürzen, dass er ins geschlossene Feld passt."""
        platz = breite - 34          # Rand links, Pfeil rechts
        if s.measure(text) <= platz:
            return text
        gekuerzt = text
        while gekuerzt and s.measure(gekuerzt + '…') > platz:
            gekuerzt = gekuerzt[:-1]
        return (gekuerzt + '…') if gekuerzt else text

    c = tk.Canvas(eltern, width=breite, height=hoehe, bg=grund,
                  highlightthickness=0, bd=0, cursor='hand2')
    form = _rundes_rechteck(c, 1, 1, breite - 1, hoehe - 1, radius=5,
                            fill='#0c1017', outline=LINIE, width=1)
    text_id = c.create_text(11, hoehe / 2.0,
                            text=_passend(beschriftung_zu(gewaehlt)),
                            fill=FG, font=s, anchor='w')
    pfeil = c.create_text(breite - 12, hoehe / 2.0, text='▾', fill=SUB,
                          font=s, anchor='e')
    c.ist_knopf = True          # die Randprüfung soll ihn messen

    def faerben():
        gesetzt = bool(zustand['wert'])
        c.itemconfigure(form, outline=ACCENT if gesetzt else LINIE)
        c.itemconfigure(text_id, fill=ACCENT if gesetzt else FG)
        c.itemconfigure(pfeil, fill=ACCENT if gesetzt else SUB)

    def zuklappen(_=None):
        # ⚠ Zuerst die Wachen lösen. Die aufgeklappte Liste ist ein **eigenes
        # Fenster**; bleiben ihre Bindungen am Hauptfenster hängen, feuern sie
        # später ins Leere.
        for ereignis, marke in zustand.get('wachen') or []:
            try:
                c.winfo_toplevel().unbind(ereignis, marke)
            except tk.TclError:
                pass
        zustand['wachen'] = []
        if zustand['liste'] is not None:
            try:
                zustand['liste'].destroy()
            except tk.TclError:
                pass
            zustand['liste'] = None
            zustand['zu_seit'] = time.time()

    def waehlen(wert):
        zuklappen()
        zustand['wert'] = wert
        c.itemconfigure(text_id, text=_passend(beschriftung_zu(wert)))
        faerben()
        beim_waehlen(wert)

    def aufklappen(_=None):
        if zustand['liste'] is not None:
            zuklappen()
            return
        # ⚠ Ein Klick, der die Liste gerade eben geschlossen hat, darf sie nicht
        # sofort wieder öffnen. Schließt das Fenster über `<FocusOut>`, kommt der
        # Klick anschließend hier an — man sah die Liste aufblitzen und sofort
        # wieder verschwinden, und erst der zweite Klick hielt sie offen.
        if time.time() - zustand['zu_seit'] < 0.25:
            return
        fenster = tk.Toplevel(c)
        fenster.overrideredirect(True)        # kein Titelbalken, kein Rahmen
        fenster.configure(bg=LINIE)
        zustand['liste'] = fenster

        # ⚠ Die Liste muss rollen können. Bei 25 Arten ist sie höher als der
        # Platz unter dem Feld — steht das Fenster weit unten, waren die
        # letzten Einträge unerreichbar. Also: Höhe begrenzen, eigene
        # Rollfläche, und wenn unten kein Platz ist, klappt sie nach oben.
        aussen = tk.Frame(fenster, bg=FLAECHE)
        aussen.pack(fill='both', expand=True, padx=1, pady=1)
        leinwand = tk.Canvas(aussen, bg=FLAECHE, highlightthickness=0, bd=0)
        innen = tk.Frame(leinwand, bg=FLAECHE)
        fenster_id = leinwand.create_window((0, 0), window=innen, anchor='nw')

        for wert, text in eintraege:
            an = (wert == zustand['wert'])
            zeile = tk.Label(innen, text=text, bg=FLAECHE,
                             fg=ACCENT if an else FG, font=s, anchor='w',
                             padx=11, pady=4, cursor='hand2')
            zeile.pack(fill='x')
            zeile.bind('<Button-1>', lambda e, w=wert: waehlen(w))
            zeile.bind('<Enter>', lambda e, z=zeile: z.configure(bg=BAR))
            zeile.bind('<Leave>', lambda e, z=zeile: z.configure(bg=FLAECHE))

        c.update_idletasks()
        innen.update_idletasks()
        gebraucht_hoehe = innen.winfo_reqheight()
        gebraucht_breite = max(breite, innen.winfo_reqwidth() + 2)

        x = c.winfo_rootx()
        unten = c.winfo_rooty() + hoehe + 2
        # ⚠ Nicht `winfo_screenheight()`: Tk meldet damit die Höhe **aller**
        # Bildschirme zusammen. Bei zwei übereinander stehenden Monitoren passt
        # eine lange Liste rechnerisch immer nach unten — und klappt in
        # Wirklichkeit unterhalb des Bildes auf. Gemeldet als „Alle Arten und
        # Alle Quellen sind nicht auswählbar", also genau die beiden längsten
        # Listen. Maßgeblich ist der Bildschirm, auf dem das Feld steht.
        _sx, schirm_oben, _sb, schirm_hoch = bildschirm.schirm_fuer(
            c, c.winfo_rootx(), c.winfo_rooty())
        schirm_unten = schirm_oben + schirm_hoch
        # So viel Platz ist nach unten bzw. nach oben — mit etwas Luft zum Rand.
        platz_unten = schirm_unten - unten - 20
        platz_oben = c.winfo_rooty() - schirm_oben - 20
        nach_oben = gebraucht_hoehe > platz_unten and platz_oben > platz_unten
        sicht = min(gebraucht_hoehe, max(platz_oben if nach_oben
                                         else platz_unten, 120))
        # ⚠ **Zweite Grenze: das Fenster selbst.** Der Bildschirm allein
        # genügt nicht — steht das Fenster weit unten im Bild, passt eine lange
        # Liste rechnerisch noch auf den Schirm, ragt aber weit darunter hinaus
        # und wird am Bildrand abgeschnitten. Bei 38 Rohstoffen im Bergbau war
        # sie länger als das Fenster hoch ist: „ist die Liste über dem
        # Einstellungsfenster hinaus, wird die abgeschnitten, wenn man das
        # Fenster zu weit unten im Bild hat" (30.08.2026).
        #
        # Wird sie dadurch kürzer als ihr Inhalt, bekommt sie unten eine
        # Rollleiste — der Code dafür steht bereits da.
        # ⚠ Maßstab ist die **kleinstmögliche** Fensterhöhe, nicht die
        # aktuelle. Wer sein Fenster gross zieht, bekaeme sonst eine Liste, die
        # nach dem Verkleinern nicht mehr hineinpasst — und beim naechsten
        # Aufklappen unten abgeschnitten waere. Gefordert am 30.08.2026:
        # „Auswahlfenster duerfen die minimale Fensterhoehe NIE
        # ueberschreiten." Also gilt immer `MIN_HOEHE`, auch im Vollbild.
        try:
            fenster_hoch = c.winfo_toplevel().winfo_height()
            if fenster_hoch > 200:
                sicht = min(sicht, min(fenster_hoch, MIN_HOEHE) - 40)
        except tk.TclError:
            pass
        # ⚠ **Dritte Grenze, und die entscheidende: eine feste Zeilenzahl.**
        # Die beiden Grenzen darueber messen den Platz — der ist auf einem
        # grossen Bildschirm riesig. Bei 48 Orten im Bergbau hing die Liste
        # daraufhin ueber die ganze Fensterhoehe herunter und weit darueber
        # hinaus ins Bild, ohne dass man ihr ansah, dass sie rollt (30.08.2026).
        #
        # Eine Auswahlliste soll man ueberblicken koennen. Mehr als rund
        # fuenfzehn Zeilen liest ohnehin niemand am Stueck — der Rest gehoert
        # gerollt, und dann ist auch die Rollleiste sichtbar und sagt, dass da
        # noch mehr kommt.
        if len(eintraege) > MAX_WAHLZEILEN:
            try:
                kinder = innen.winfo_children()
                if kinder:
                    zeilenhoehe = kinder[0].winfo_reqheight()
                    if zeilenhoehe > 0:
                        sicht = min(sicht, zeilenhoehe * MAX_WAHLZEILEN)
            except tk.TclError:
                pass
        y = (c.winfo_rooty() - sicht - 2) if nach_oben else unten

        leinwand.configure(width=gebraucht_breite - 2, height=sicht)
        leinwand.pack(side='left', fill='both', expand=True)
        if gebraucht_hoehe > sicht:
            # ⚠ Gleiche Breite wie auf den Seiten. Acht Pixel waren schmaler
            # als alles andere im Fenster und dadurch noch schwerer zu sehen.
            #
            # ⚠⚠ **Und sie reicht trotzdem nicht.** Am 04.09.2026: „sieht aber
            # keine Sau, weil kein Balken da ist … ah, man muss scrollen." Die
            # Leiste ist da und **bekommt ihre 10 px** (nachgemessen, in beiden
            # Pack-Reihenfolgen) — nur sieht man einen dunklen Streifen auf
            # dunklem Grund nicht.
            #
            # Die Lehre daraus gehört nicht hierher, sondern in die Listen
            # selbst: **Was wichtig ist, gehört nach oben.** Standen die zwei
            # größten Gruppen (Rüstung 910, Waffen 270) alphabetisch an Position
            # 11 und 14, half auch der beste Balken nicht.
            leiste = rundleiste(aussen, leinwand, grund=FLAECHE, breite=10)
            leiste.pack(side='right', fill='y')
            leinwand.configure(yscrollcommand=leiste.set)
        leinwand.configure(scrollregion=(0, 0, gebraucht_breite,
                                         gebraucht_hoehe))
        leinwand.itemconfigure(fenster_id, width=gebraucht_breite - 2)

        # ⚠⚠ **Das Rad muss HIER abgefangen werden, sonst rollt die Seite
        # dahinter.** `rad_anschliessen` haengt global am Programm (`bind_all`)
        # und sucht sich die Rollflaeche, indem es vom Element unter dem Zeiger
        # nach oben durch die Elternkette geht. Die aufgeklappte Liste ist zwar
        # ein eigenes Fenster, ihr Elternteil ist aber das Auswahlfeld — die
        # Kette laeuft also aus der Liste heraus und findet die Rollflaeche der
        # Seite darunter. Ergebnis: Man dreht am Rad, die Liste steht still und
        # die Seite dahinter wandert. Die unteren Eintraege waren so gar nicht
        # erreichbar. Am 30.08.2026 gemeldet.
        #
        # Eine Bindung am Listenfenster selbst greift fuer alle Zeilen darin
        # (Tk geht Widget → Klasse → Fenster → „all") und laeuft VOR der
        # globalen. `return 'break'` beendet die Kette — die Seite dahinter
        # bekommt das Rad gar nicht erst zu sehen.
        def rad_in_liste(e):
            if gebraucht_hoehe > sicht:
                nummer = getattr(e, 'num', 0)
                if nummer == 4:
                    schritte = -1
                elif nummer == 5:
                    schritte = 1
                else:
                    # Nur die Richtung zaehlt, nie der Betrag: Windows meldet
                    # ±120, macOS ±1. Eine Division durch 120 ergaebe dort 0.
                    betrag = float(getattr(e, 'delta', 0) or 0)
                    if not betrag:
                        return 'break'
                    schritte = -1 if betrag > 0 else 1
                try:
                    leinwand.yview_scroll(schritte, 'units')
                except tk.TclError:
                    pass
            # Auch wenn nichts zu rollen ist: Das Rad darf nicht durchfallen.
            return 'break'

        def streich_in_liste(e):
            """Trackpad — beide Richtungen stecken gepackt in einer Zahl."""
            roh = int(getattr(e, 'delta', 0) or 0)
            senkrecht = (roh >> 16) & 0xFFFF
            if senkrecht >= 0x8000:
                senkrecht -= 0x10000
            if senkrecht and gebraucht_hoehe > sicht:
                try:
                    leinwand.yview_scroll(1 if senkrecht > 0 else -1, 'units')
                except tk.TclError:
                    pass
            return 'break'

        for ereignis in ('<MouseWheel>', '<Button-4>', '<Button-5>'):
            fenster.bind(ereignis, rad_in_liste)
        try:
            fenster.bind('<TouchpadScroll>', streich_in_liste)
        except tk.TclError:
            # Tk 8.6 kennt das Ereignis nicht; dort melden sich Trackpads
            # ohnehin als Button-4/5.
            pass

        fenster.geometry('%dx%d+%d+%d' % (gebraucht_breite, sicht + 2, x, y))
        # ⚠⚠ **Ohne das landet die Liste links oben am Bildschirmrand.**
        # Ein `overrideredirect`-Fenster wird angezeigt, sobald Tk dazu kommt —
        # und wenn das VOR dem Verarbeiten der Geometrie geschieht, sitzt es auf
        # der Voreinstellung (0,0) statt unter dem Feld. Ein `update_idletasks()`
        # laesst Tk die Anweisung erst anwenden.
        #
        # Am 02.09.2026 gemeldet, mit Bild: „Alle Klassen" klappte ganz links
        # oben auf. Die Rechnung war dabei die ganze Zeit richtig — die Spur
        # zeigte `Feld bei (2160,347) gemappt=1 -> Liste bei (2160,380)`. Genau
        # deshalb war es so schwer zu finden: Es sah nach einem Rechenfehler
        # aus und war ein Zeitpunktfehler. Gefunden wurde es, weil ein
        # Messpunkt mit `update_idletasks()` den Fehler versehentlich behob.
        #
        # ⚠ `rundwahl` selbst ist seit v3.9.1 unveraendert (344 Zeilen,
        # geprueft). Ausgeloest hat es vermutlich der Seiten-Vorbau, der die
        # Ereignisschleife staerker belegt und damit das Zeitfenster
        # verschiebt — das erklaert, warum derselbe Code vorher richtig lag.
        fenster.update_idletasks()
        fenster.lift()
        fenster.focus_set()

        # Ein Klick irgendwo anders schließt die Liste — sonst bleibt sie stehen,
        # sobald man es sich anders überlegt.
        #
        # ⚠ Die Bindung wird **verzögert** gesetzt. Direkt nach einer Auswahl baut
        # die Bauplan-Liste sich neu auf (bis zu 670 Zeilen), und dabei wandert der
        # Fokus noch einmal. Hing `<FocusOut>` sofort am frischen Fenster, fing es
        # genau dieses Nachzucken ab und schloss sich von selbst: Wer nach einer
        # Auswahl gleich das nächste Feld anklickte, sah die Liste aufblitzen und
        # verschwinden — erst der zweite Klick hielt. Genau so gemeldet.
        def wache_setzen():
            try:
                fenster.bind('<FocusOut>', zuklappen)
                fenster.bind('<Escape>', zuklappen)
                # ⚠⚠ **Scrollen schliesst die Liste auch.** Sie schwebt als
                # eigenes Fenster an einer festen Stelle des Bildschirms —
                # rollt man die Seite darunter weg, bleibt sie stehen und legt
                # sich über fremde Zeilen. Am 29.08.2026 gemeldet: „alles
                # scrollt mit, wenn ich durch die Liste scrolle." Ein
                # Fokuswechsel findet dabei nicht statt, `<FocusOut>` greift
                # also nicht.
                #
                # Dasselbe gilt, wenn das Fenster verschoben oder in der Grösse
                # geändert wird: Die Liste stünde dann neben ihrem Feld.
                wurzel = c.winfo_toplevel()
                wachen = []
                for ereignis in ('<MouseWheel>', '<Button-4>', '<Button-5>',
                                 '<Configure>'):
                    wachen.append((ereignis,
                                   wurzel.bind(ereignis, zuklappen, add='+')))
                zustand['wachen'] = wachen
            except tk.TclError:
                pass

        fenster.after(250, wache_setzen)

    def stumm_setzen(wert):
        """Anzeige umstellen, ohne den Rückruf auszulösen.

        Gebraucht beim Zurücksetzen mehrerer Felder auf einmal: Sonst löst
        jedes einzelne einen vollen Neuaufbau der Liste aus.
        """
        zuklappen()
        zustand['wert'] = wert
        c.itemconfigure(text_id, text=beschriftung_zu(wert))
        faerben()

    c.bind('<Button-1>', aufklappen)
    # ⚠⚠ **Beim Seitenwechsel muss die Liste mitgehen.** Sie ist ein eigenes,
    # rahmenloses Fenster und hängt an keiner Seite: Wer sie in der Herstellung
    # aufklappt und dann links auf „Mein Lager" klickt, hatte sie bis
    # v3.3.0-rc40 weiter über dem Lager schweben. Am 30.08.2026 gemeldet.
    #
    # Die vorhandenen Wachen greifen dort nicht: Ein Seitenwechsel ist kein
    # Fokuswechsel (`<FocusOut>`), kein Rollen und keine Größenänderung des
    # Fensters — es wird nur eine Fläche gegen eine andere getauscht.
    #
    # Also am Feld selbst horchen: Wird es ausgeblendet (`<Unmap>`) oder
    # abgeräumt (`<Destroy>`), ist die Liste dazu gegenstandslos.
    def _weg_mit_der_liste(ereignis=None):
        # ⚠ Nur auf das Feld selbst hören. `<Destroy>` kommt auch für jedes
        # Kind und würde sonst beim Abräumen der Liste selbst wieder feuern.
        if ereignis is not None and ereignis.widget is not c:
            return
        zuklappen()

    c.bind('<Unmap>', _weg_mit_der_liste)
    c.bind('<Destroy>', _weg_mit_der_liste)
    faerben()
    c.setzen = waehlen
    c.stumm_setzen = stumm_setzen
    c.wert = lambda: zustand['wert']
    return c


def marke(eltern, text, farbe, schrift, grund=None, mindestbreite=0):
    """Eine abgerundete Blase mit farbigem Rand — „neu", „behoben" und Verwandte.

    Ein farbiges Wort geht in einer Liste unter; eine umrandete Blase liest man
    als Auszeichnung.

    ⚠ Mit einem Label geht das nicht: `highlightthickness` zeichnet Tk je nach
    System nur bei Fokus (auf dem Mac blieb der Rand unsichtbar), und
    `relief='solid'` malt eine Systemlinie statt einer eigenen Farbe. Runde
    Ecken kann ein Label ohnehin nicht. Deshalb eine kleine Leinwand: Sie kostet
    ein paar Zeilen mehr und sieht auf allen drei Systemen gleich aus.
    """
    grund = grund or FLAECHE
    hoehe = schrift.metrics('linespace') + 8
    # ⚠ Genug Luft, und alle Blasen einer Gruppe gleich breit: Sonst wird die
    # längste abgeschnitten, sobald der Platz feststeht — und die Wörter
    # flattern, weil jede Blase anders breit ist.
    breite = max(mindestbreite, schrift.measure(text) + 20)
    c = tk.Canvas(eltern, width=breite, height=hoehe, bg=grund,
                  highlightthickness=0, bd=0)
    c.blase = _rundes_rechteck(c, 1, 1, breite - 1, hoehe - 1,
                               radius=max(4, hoehe // 3),
                               fill=grund, outline=farbe, width=1)
    c.create_text(breite / 2.0, hoehe / 2.0, text=text, fill=farbe,
                  font=schrift, anchor='center')

    def hintergrund(neuer):
        """Beim Einfärben der Zeile mitziehen — Leinwand und Blasenfüllung."""
        c.configure(bg=neuer)
        c.itemconfigure(c.blase, fill=neuer)

    c.hintergrund = hintergrund
    return c


class Hauptfenster:
    """Der Rahmen mit der Reiterleiste. Die Seiten liefern andere Module."""

    def __init__(self, eltern=None, beim_schliessen=None, version='',
                 beim_schriftwechsel=None, startseite='liste'):
        self.beim_schliessen = beim_schliessen
        self.version = version
        self.root = tk.Toplevel(eltern) if eltern else tk.Tk()
        # ⚠⚠ **Erst bauen, dann zeigen.** Ein `Toplevel` steht ab der Erzeugung
        # auf dem Bildschirm — Reiterleiste, Fusszeile und die erste Seite
        # entstehen also VOR den Augen des Nutzers. Gemeldet als „braucht eine
        # Weile, bis Symbole und Text links geladen werden" (Haldjas, pr0, und
        # am 02.09.2026 erneut). Die Zeit war dabei nie das eigentliche Problem
        # — man sah nur beim Bauen zu. Das `deiconify()` am Ende von `__init__`
        # gehoert untrennbar hierher.
        self.root.withdraw()
        self.root.title(fenstertitel(t('hf_titel')))
        self.root.configure(bg=BG)
        # Start = die zuletzt eingestellte Groesse, sonst die Mindestgroesse —
        # mittig auf dem Hauptbildschirm. Mittig, damit das Fenster bei
        # mehreren Monitoren nicht auf einer Kante landet.
        #
        # ⭐ **Wer sein Fenster groesser zieht, findet es so wieder.** Vorher
        # ging es bei jedem Start wieder auf 1160x380 zurueck, und wer mit
        # langen Listen arbeitet, zog es jedes Mal von Hand auf.
        _b_start, _h_start = gemerkte_groesse(self.root)
        self.root.geometry(bildschirm.mittig(self.root, _b_start, _h_start))
        self.root.minsize(MIN_BREITE, MIN_HOEHE)
        # Merker fuer die Drossel unten — solange etwas darin steht, ist ein
        # Speichern schon vorgemerkt.
        self._groesse_wartet = None
        self.root.bind('<Configure>', self._groesse_beobachten, add='+')

        self._schriften_anlegen()

        self.seiten = {}          # kennung -> Frame
        self.gezeichnet = set()   # welche Seiten schon Inhalt haben
        self.knoepfe = {}         # kennung -> Reiter-Label
        # Die klappbaren Gruppen der Seitenleiste — siehe `_gruppe`.
        self.gruppen = {}
        self.aktuell = None
        self.fortgeschritten_offen = False
        # Wer die Schriftgröße ändert, meint das ganze Programm — auch das
        # Overlay. Das Fenster kennt es nicht, deshalb ein Rückruf.
        self.beim_schriftwechsel = beim_schriftwechsel

        self._titelleiste()
        self._fusszeile()         # ⚠ vor dem Inhalt — sonst rutscht sie hinaus
        self._korpus()

        # ⚠⚠ **Die gewuenschte Seite, nicht fest die Liste.** Bis zum 02.09.2026
        # stand hier `self.oeffnen('liste')`, und der Aufrufer oeffnete die
        # eigentlich gewollte Seite erst DANACH. Wer die Einstellungen aufmachte,
        # wartete damit auf den Aufbau der kompletten Bauplan-Liste, die sofort
        # wieder ausgeblendet wurde. Im Fehlerbericht stand es zweimal woertlich
        # untereinander: `Seite liste: steht (205 ms)` gefolgt von
        # `Seite allgemein: steht (7 ms)` — 205 der 212 ms waren fuer nichts.
        self.oeffnen(startseite)
        # Die Mindesthöhe hängt an Schriftgröße und Skalierung — einmal messen,
        # sobald Tk die Seitenleiste gezeichnet hat.
        self.root.after(50, self._mindesthoehe_nachziehen)
        self.root.protocol('WM_DELETE_WINDOW', self.schliessen)
        # ⚠ Jetzt ist alles gebaut — ab hier darf es gesehen werden. Steht
        # bewusst als letzte Zeile: Was danach noch dazukaeme, saehe der Nutzer
        # wieder entstehen. (Im Pruefbetrieb legt `tools/unsichtbar.py`
        # `deiconify` stumm, dort bleibt das Fenster also verborgen.)
        self.root.deiconify()

    # ------------------------------------------------------------- Schriften
    def _schriften_anlegen(self):
        stufe = STUFEN.get(pfade.einstellung('schriftgroesse') or 'normal', 1)
        self.f_grund  = tkfont.Font(family='Segoe UI', size=10 + stufe)
        self.f_fett   = tkfont.Font(family='Segoe UI', size=10 + stufe, weight='bold')
        self.f_klein  = tkfont.Font(family='Segoe UI', size=9 + stufe)
        self.f_titel  = tkfont.Font(family='Segoe UI', size=12 + stufe, weight='bold')
        # Siehe `Overlay.ZEICHEN_SCHRIFT`: `Segoe UI` enthält die Symbole nicht,
        # Windows fällt sonst auf die **farbige** Segoe UI Emoji zurück.
        self.f_zeichen = tkfont.Font(
            family='Segoe UI Symbol' if pfade.WINDOWS else 'Segoe UI',
            size=13 + stufe)

    def schriftgroesse_setzen(self, stufe):
        """Die ganze Oberfläche wächst oder schrumpft — sofort, ohne Neustart.

        ⚠ **Die Schriften umzustellen reicht nicht.** Ein benanntes Tk-Font
        wirkt sofort auf jedes Widget, das es benutzt — aber nur auf den
        *Text*. Alles, was seine Größe beim Bauen **einmal gemessen** hat,
        bleibt stehen: die gezeichneten Rundknöpfe (`_wahl`, `rundknopf`,
        `rundes_feld`) legen ihre Leinwand auf `schrift.measure(text)` fest.
        Bei „sehr groß" ragte der Text deshalb aus dem Kasten heraus und war
        abgeschnitten — gemeldet von am 27.08.2026 gemeldet an der
        Overlay-Wahl („immer sichtbar" / „nur bei einem Neuzugang").

        Deshalb dasselbe wie beim Sprachwechsel: einmal neu aufbauen. Jede
        Leinwand misst dann mit der neuen Schrift. Einzeln nachzuziehen wären
        vier Bausteine an Dutzenden Stellen — eine Liste, die man nicht
        pflegen kann.

        ⚠ Die Rückmeldung kommt **nach** dem Neuaufbau. Vorher gesagt, wäre sie
        sofort wieder weg: `neu_aufbauen()` zerstört auch die Fußzeile.
        """
        n = STUFEN.get(stufe, 1)
        for schrift, grund in ((self.f_grund, 10), (self.f_fett, 10),
                               (self.f_klein, 9), (self.f_titel, 12),
                               (self.f_zeichen, 13)):
            schrift.configure(size=grund + n)
        pfade.einstellung_setzen('schriftgroesse', stufe)
        if self.beim_schriftwechsel:
            try:
                self.beim_schriftwechsel(stufe)
            except Exception as ausnahme:
                fehler.merken('hauptfenster.schriftwechsel', ausnahme)

        # ⚠ Über `after`, nicht sofort: Wir stecken im Klick-Rückruf des
        # Knopfes, der gleich zerstört wird. Tk meldete sonst
        # „invalid command name“.
        def nachziehen():
            try:
                self.neu_aufbauen()
                self.sagen('%s: %s' % (t('hf_schrift'), t('hf_s_' + stufe)))
            except Exception as ausnahme:
                fehler.merken('hauptfenster.schriftgroesse_nachziehen',
                              ausnahme)

        self.root.after(0, nachziehen)

    # ------------------------------------------------------------ Titelleiste
    def _titelleiste(self):
        bar = tk.Frame(self.root, bg=BAR)
        bar.pack(side='top', fill='x')

        # Das Programm-Icon gehört hierhin — dort sucht man es.
        self._icon_bild = None
        png = _mitgeliefert(os.path.join('assets', 'icon.png'))
        if png and os.path.exists(png):
            try:
                voll = tk.PhotoImage(file=png)
                teiler = max(1, voll.width() // 22)
                self._icon_bild = voll.subsample(teiler, teiler)
                tk.Label(bar, image=self._icon_bild, bg=BAR).pack(side='left',
                                                                 padx=(12, 8), pady=8)
            except Exception as ausnahme:
                fehler.merken('hauptfenster.icon', ausnahme)

        tk.Label(bar, text=t('hf_titel'), bg=BAR, fg=FG,
                 font=self.f_fett).pack(side='left')
        tk.Label(bar, text='v%s' % (self.version or '—'), bg=BAR, fg=SUB,
                 font=self.f_klein).pack(side='left', padx=(6, 0))

        # Symbol UND Wort: Ein Symbol allein erklärt sich nur dem, der es gebaut
        # hat — hier war selbst der Entwickler unsicher, was `⟳` bedeutet. Genau
        # deshalb steht der Zauberstab jetzt neben dem Wort „Einrichtung
        # starten": ein Verb sagt, dass etwas losgeht; „Einrichtung" allein
        # klang nach einem Ort, an dem man etwas nachschlägt.
        self.knopf_neu = self._titelknopf(bar, 'wasistneu', t('hf_wasistneu'),
                                          t('hf_hinweis_neu'), self._was_ist_neu)
        self._titelknopf(bar, 'einrichtung', t('hf_einrichtung'),
                         t('hf_hinweis_einr'), self._einrichtung)
        # ⚠ Gehoert hier oben hin, nicht in die Einstellungen: Wer den Rechner
        # wechselt, sucht nicht erst in Untermenues — und wer eine Sicherung
        # nie gesehen hat, macht auch keine. Ein sichtbarer Knopf ist der
        # Unterschied zwischen „gibt es" und „wird benutzt".
        self._titelknopf(bar, 'sicherung', t('hf_sicherung'),
                         t('hf_hinweis_sich'), self._sicherung)

    def _titelknopf(self, eltern, symbol, wort, erklaerung, tat):
        rahmen = tk.Frame(eltern, bg=BAR, cursor='hand2')
        rahmen.pack(side='right', padx=(0, 10), pady=6)
        z = zeichen.knopf(rahmen, symbol, grund=BAR, schrift=self.f_zeichen)
        z.pack(side='left')
        w = tk.Label(rahmen, text=' ' + wort, bg=BAR, fg=SUB, font=self.f_klein)
        w.pack(side='left')
        for teil in (rahmen, z, w):
            teil.bind('<Button-1>', lambda e, f=tat: f())
        hinweis.anhaengen(rahmen, lambda: erklaerung)
        rahmen.teile = (z, w)
        return rahmen

    # --------------------------------------------------------------- Fußzeile
    def _fusszeile(self):
        fuss = tk.Frame(self.root, bg=BAR)
        fuss.pack(side='bottom', fill='x')
        self.meldung = tk.Label(fuss, text=t('hf_sofort'), bg=BAR, fg=SUB,
                                font=self.f_klein)
        self.meldung.pack(side='left', padx=14, pady=9)
        k = tk.Label(fuss, text=' %s ' % t('hf_schliessen'), bg=FLAECHE, fg=FG,
                     font=self.f_klein, cursor='hand2', padx=10, pady=4)
        k.pack(side='right', padx=12)
        k.bind('<Button-1>', lambda e: self.schliessen())

    def sagen(self, text):
        """Kurze Rückmeldung in der Fußzeile — statt eines Speichern-Knopfes."""
        try:
            self.meldung.configure(text=text, fg=ACCENT)
            self.root.after(4000, lambda: self.meldung.configure(
                text=t('hf_sofort'), fg=SUB))
        except Exception:
            pass

    # ----------------------------------------------------------------- Korpus
    def _korpus(self):
        # 210 ist nur der Startwert — die wirkliche Breite wird gemessen, sobald
        # die Einträge stehen (siehe `_leistenbreite_nachziehen`).
        # ⚠⚠ **Die Leiste rollt, wenn sie nicht ganz auf den Bildschirm passt.**
        #
        # Vorher war sie ein fester Rahmen, und ihre Höhe bestimmte die
        # Mindesthöhe des Fensters (`_mindestmass_nachziehen`). Mit jedem neuen
        # Reiter wuchs das Fenster mit — bei der Gruppe „Handel" (v3.4.0)
        # brauchte sie 1020 px und das Fenster passte auf einem 1080er
        # Bildschirm nicht mehr: unten stand es über der Taskleiste hinaus, und
        # man kam an den Inhalt darunter nicht mehr heran. Am 30.08.2026 so
        # gemeldet: „das Einstellungsfenster ist zu groß, er kommt nicht mehr
        # an alles ran."
        #
        # Ein `minsize`, das größer ist als der Bildschirm, lässt sich auch
        # nicht wegdeckeln — Tk hält es gegen jedes `geometry()`. Also muss die
        # Leiste kleiner werden dürfen, ohne dass Einträge unerreichbar werden.
        # ⚠⚠ **Die Knöpfe unten rollen NICHT mit.** „Star Citizen starten",
        # Kaffee und Discord sitzen in einem eigenen Fuß unter der Rollfläche —
        # ein Startknopf, den man erst herunterrollen muss, ist keiner. Deshalb
        # eine Spalte mit zwei Teilen: unten der feste Fuß, darüber die
        # rollende Leiste, die sich den Rest nimmt.
        self.leisten_spalte = tk.Frame(self.root, bg=FLAECHE,
                                       width=LEISTE_BREITE)
        self.leisten_spalte.pack(side='left', fill='y')
        self.leisten_spalte.pack_propagate(False)

        self.leisten_fuss = tk.Frame(self.leisten_spalte, bg=FLAECHE)
        self.leisten_fuss.pack(side='bottom', fill='x')

        # Zwischenrahmen, damit Rollbalken und Fläche nebeneinander liegen und
        # der Fuß darunter unberührt bleibt.
        rollbereich = tk.Frame(self.leisten_spalte, bg=FLAECHE)
        rollbereich.pack(side='top', fill='both', expand=True)
        self.leisten_flaeche = tk.Canvas(rollbereich, bg=FLAECHE,
                                         width=LEISTE_BREITE,
                                         highlightthickness=0, bd=0)
        # ⚠ **Ohne sichtbaren Balken sieht die Leiste kaputt aus.** Passt sie
        # nicht ganz, sind die unteren Einträge einfach weg — eine offene
        # Gruppe wirkt dann leer, und niemand kommt auf die Idee zu rollen.
        # Genau so stand „Info" beim ersten Bau da: aufgeklappt und trotzdem
        # ohne einen einzigen Eintrag.
        self.leisten_balken = rundleiste(rollbereich, self.leisten_flaeche,
                                         grund=FLAECHE)
        self.leisten_flaeche.configure(
            yscrollcommand=self.leisten_balken.set)
        self.leisten_balken.pack(side='right', fill='y')
        self.leisten_flaeche.pack(side='left', fill='both', expand=True)
        self.leiste = tk.Frame(self.leisten_flaeche, bg=FLAECHE)
        self._leisten_fenster = self.leisten_flaeche.create_window(
            0, 0, window=self.leiste, anchor='nw', width=LEISTE_BREITE)

        def _leiste_nachmessen(_=None):
            """Rollbereich auf den Inhalt setzen — und nur rollen, wenn nötig."""
            try:
                hoch = self.leiste.winfo_reqheight()
                self.leisten_flaeche.configure(scrollregion=(0, 0, 0, hoch))
                # Passt alles, steht die Leiste still — sonst würde ein
                # Mausrad-Dreh die Einträge grundlos verschieben.
                if hoch <= self.leisten_flaeche.winfo_height():
                    self.leisten_flaeche.yview_moveto(0)
            except tk.TclError:
                pass

        self.leiste.bind('<Configure>', _leiste_nachmessen)
        self.leisten_flaeche.bind('<Configure>', _leiste_nachmessen)

        # ⭐ **Mausrad über die vorhandene Stelle**, nicht selbst gebaut:
        # `rad_anschliessen` kennt bereits alle Fallen, die hier schon einmal
        # Arbeit gekostet haben — Trackpad-Streichgesten, macOS mit ±1 statt
        # ±120, und vor allem `bind_all` **ohne** `add='+'`, das jede andere
        # Bindung im Fenster stillschweigend ersetzt. Ein zweiter Eigenbau
        # daneben hätte genau das wieder aufgerissen.
        rad_anschliessen(self.leisten_flaeche)
        self._leiste_nachmessen = _leiste_nachmessen

        self.inhalt = tk.Frame(self.root, bg=BG)
        self.inhalt.pack(side='right', fill='both', expand=True)

        g_bp = self._gruppe(t('hf_gruppe_bp'), 'bauplaene')
        self._reiter('liste', 'liste', t('hf_liste'), g_bp)
        self._reiter('fortschritt', 'fortschritt', t('hf_fortschritt'), g_bp)
        # ⚠ Hier und nicht in einer eigenen Gruppe: Auftraege sind die Quelle
        # der Baupläne — wer wissen will, woher seine kommen, sucht sie neben
        # der Bauplan-Liste, nicht in einem eigenen Bereich.
        self._reiter('auftragslog', 'eigenbuch', t('hf_auftragslog'), g_bp)

        # Eigene Gruppe, kein Anhängsel unter „Baupläne": Die beiden Seiten
        # beantworten eine andere Frage („was brauche ich / wo hole ich es")
        # als der eigene Bestand („habe ich das schon"). Die Gruppenüberschrift
        # gibt zugleich den Kontext, deshalb reichen darunter kurze Namen.
        # ⚠ Die Reihenfolge ist die **Kette, wie man sie im Spiel erlebt**:
        # Was habe ich → was brauche ich dafür → wo hole ich das. So hat es
        # Xharig am 29.08.2026 selbst beschrieben, und so liest sich die
        # Leiste von oben nach unten wie ein Ablauf statt wie eine Sammlung.
        g_werk = self._gruppe(t('hf_gruppe_herst'), 'werkstatt')
        self._reiter('lager', 'bestand', t('hf_lager'), g_werk)
        self._reiter('herstellung', 'blitz', t('hf_herstellung'), g_werk)
        self._reiter('bergbau', 'herkunft', t('hf_bergbau'), g_werk)
        # ⚠ **Hier und nicht bei „Handel".** Die Kette der Werkstatt endet bei
        # „wo hole ich das" — und ein fertig gekauftes Teil ist die Antwort auf
        # dieselbe Frage, nur der andere Weg: bauen oder kaufen. Bei „Handel"
        # ginge es um Ware, die man **loswerden** will; das ist etwas anderes.
        self._reiter('laeden', 'laeden', t('hf_laeden'), g_werk)

        # ⚠ **Eigene Gruppe, nicht an „Werkstatt" angehängt.** Die Kette dort
        # endet beim Bauen („was habe ich → was brauche ich → wo hole ich es").
        # Handel ist die Gegenrichtung: Ware, die man **loswerden** will. Wer
        # verkauft, denkt nicht an Rezepte — und wer baut, will sein Baumaterial
        # nicht in einer Verkaufsliste sehen.
        #
        # Reihenfolge wie in der Werkstatt-Gruppe: erst der Bestand, dann was
        # man damit tut.
        g_handel = self._gruppe(t('hf_gruppe_handel'), 'handel')
        self._reiter('handelslager', 'handelslager', t('hf_handelslager'),
                     g_handel)
        self._reiter('verkauf', 'verkauf', t('hf_verkauf'), g_handel)
        # ⚠ Nach „Verkauf", weil es die größere Frage ist: Dort geht es um
        # Ware, die man **schon hat**; hier um die Fahrt, die man erst plant.
        # Wer den Laderaum voll hat, will „wohin damit" — wer ihn leer hat,
        # „was soll ich überhaupt laden".
        self._reiter('routen', 'routen', t('hf_routen'), g_handel)

        g_einst = self._gruppe(t('hf_gruppe_einst'), 'einstellungen')
        self._reiter('allgemein', 'einstellungen', t('hf_allgemein'), g_einst)
        self._reiter('anzeige', 'anzeige', t('hf_anzeige'), g_einst)
        self._reiter('spiel', 'auftragstexte', t('hf_spiel'), g_einst)
        # ⚠ Unter „Einstellungen" und nicht bei den Bauplänen: Die Seite sagt,
        # wie der eigene Aufbau aussieht — welcher Stick welche Nummer hat und
        # was darauf liegt. Das ist dieselbe Sorte Frage wie „welcher Ordner,
        # welche Sprache", nur für die Steuerung.
        self._reiter('joysticks', 'joysticks', t('hf_joysticks'), g_einst)

        # „Was ist neu" und „Über" stellen nichts ein — sie erzählen etwas.
        # Unter der Überschrift „Einstellungen" waren sie falsch einsortiert.
        g_info = self._gruppe(t('hf_gruppe_info'), 'info')
        self._reiter('wasistneu', 'wasistneu', t('hf_wasistneu'), g_info)
        self._reiter('ueber', 'ueber', t('hf_ueber'), g_info)
        # Direkt unter „Update & Über": Wer nicht ins Spiel kommt, sucht den
        # Fehler zuerst bei sich. Ein eigener Reiter beantwortet das, statt die
        # Auskunft unten an eine andere Seite zu hängen, wo niemand sie sucht.
        self._reiter('serverstatus', 'serverstatus', t('hf_serverstatus'),
                     g_info)
        # ⚠ **Diagnose gehört hierher, nicht unter „Fortgeschritten".** Wer die
        # Seite braucht, hat ein Problem — und sucht sie dann in einem Menü, das
        # zugeklappt ist und „Fortgeschritten" heißt, also nach „nichts für
        # mich" aussieht. Am 28.08.2026 fiel auf, nachdem sein Bruder den
        # Bericht nicht fand: „ich will nicht jedem eine Stunde erklären, wie
        # ich zu dem Bericht komme."
        #
        # Seit dem roten Knopf „Fehlerbericht absenden" ist die Seite außerdem
        # der Weg, auf dem Meldungen überhaupt ankommen. Ein Weg, den man
        # erklären muss, wird nicht benutzt.
        self._reiter('diagnose', 'diagnose', t('hf_diagnose'), g_info)
        # ⚠ Eigener Reiter, kein Abschnitt auf „Update & Über": Die Seite dort
        # ist mit Version, Katalogzahlen, Update-Kanal und Holen-Knopf schon
        # voll, und wem was gehört, hat mit Updates nichts zu tun.
        self._reiter('danke', 'quellen', t('hf_danke'), g_info)

        # Fortgeschrittenes ist zugeklappt — sichtbar, aber nicht im Weg. Wer
        # es sucht, findet es; wer es nicht kennt, wird nicht erschlagen.
        #
        # ⚠ **Es sitzt in der Gruppe „Einstellungen", nicht mehr am unteren
        # Rand.** Dort klebte es früher zwischen den Knöpfen und war das
        # einzige Element der Leiste ohne Gruppe — ein Bruch, sobald die
        # Gruppen klappbar wurden (30.08.2026).
        #
        # ⚠ Und zwar **Einstellungen**, nicht „Info": Dahinter liegen Pfade,
        # Erkennung und der Bauplan-Bestand, also Dinge, die man **einstellt**.
        # „Info" erzählt etwas (Was ist neu, Über, Serverstatus, Danke) — dort
        # wäre es thematisch falsch einsortiert, auch wenn es optisch passte.
        self.klapp = tk.Frame(g_einst, bg=FLAECHE)
        self.klapp.pack(fill='x', pady=(6, 4))
        # ⚠ Aufbau wie eine Gruppenüberschrift: Beschriftung links, Pfeil
        # rechts, dasselbe Symbol. Es ist dieselbe Handlung — etwas auf- und
        # zuklappen —, also muss es gleich aussehen (Wunsch vom 30.08.2026:
        # „gleiches Bild im gesamten Projekt").
        self.klappkopf = tk.Frame(self.klapp, bg=FLAECHE, cursor='hand2')
        self.klappkopf.pack(fill='x')
        self.klapppfeil = zeichen.zeile(self.klappkopf, 'aufklappen',
                                        grund=FLAECHE, schrift=self.f_klein)
        self.klapppfeil.pack(side='right', padx=(0, 12))
        self.klappknopf = tk.Label(self.klappkopf, text=t('hf_fortgeschritten'),
                                   bg=FLAECHE, fg=SUB, font=self.f_klein,
                                   cursor='hand2', anchor='w', padx=16, pady=8)
        self.klappknopf.pack(side='left', fill='x', expand=True)
        for _teil in (self.klappkopf, self.klappknopf, self.klapppfeil):
            _teil.bind('<Button-1>', lambda e: self._klapp_umschalten())
        self.klappinhalt = tk.Frame(self.klapp, bg=FLAECHE)

        # --- Discord -----------------------------------------------------
        # Wunsch von am 26.08.2026 gemeldet, nach dem Vorbild des
        # SC-Deutsch-Launchers: „discord Button wäre tatsächlich auch sinnvoll."
        #
        # ⚠ Bewusst **ruhiger** als der Knopf darüber. Star Citizen zu starten
        # ist die Handlung, für die jemand dieses Fenster offen hat; der Weg zum
        # Discord ist ein Angebot. Zwei gleich laute Knöpfe nebeneinander nehmen
        # sich gegenseitig die Wirkung — das markante Grün trägt nur, solange es
        # an genau einer Stelle steht.
        rahmen_dc = tk.Frame(self.leisten_fuss, bg=FLAECHE)
        rahmen_dc.pack(side='bottom', fill='x', padx=12, pady=(0, 6))
        self.discordknopf = rundknopf(
            rahmen_dc, t('hf_discord'), self._discord_oeffnen, self.f_klein,
            FLAECHE, FLAECHE, LINIE, SUB, radius=8, polster=(12, 6),
            malen=discord_zeichen)
        self.discordknopf.pack(fill='x')

        # --- Ko-fi -------------------------------------------------------
        # ⚠ Die Rechtslage dazu ist **zweigeteilt** und am 26.08.2026 geprüft:
        #
        #   * Die Fandom-FAQ von RSI führt „donations" wörtlich in der Liste
        #     verbotener kommerzieller Nutzung.
        #   * Die **Terms of Service** — das Dokument, das jeder Spieler mit
        #     seinem Konto annimmt — verbieten für Fan-Seiten nur
        #     Zugangsgebühren und Werbe- bzw. Sponsoreneinnahmen. Spenden kommen
        #     dort nicht vor.
        #
        # der Autor hat sich nach beiden Fundstellen dafür entschieden, weil das
        # Projekt echte Kosten verursacht und die ToS es nicht untersagen. Was in
        # **beiden** Dokumenten verboten bleibt und deshalb hier nie entstehen
        # darf: eine Bezahlschranke, ein Abo, Werbung. Der Knopf führt zu einer
        # freiwilligen Seite, das Werkzeug bleibt vollständig und kostenlos.
        rahmen_kofi = tk.Frame(self.leisten_fuss, bg=FLAECHE)
        rahmen_kofi.pack(side='bottom', fill='x', padx=12, pady=(0, 2))
        self.kofiknopf = rundknopf(
            rahmen_kofi, t('hf_kofi'), self._kofi_oeffnen, self.f_klein,
            FLAECHE, FLAECHE, LINIE, SUB, radius=8, polster=(12, 6),
            malen=kaffee_zeichen)
        self.kofiknopf.pack(fill='x')

        # --- Star Citizen starten ---------------------------------------
        # ⚠ Der Knopf stand erst auf der Seite „Auftragstexte", also dort, wo es
        # um Bauplan-Angaben im Spiel geht — selbst der Autor fand ihn nicht
        # wieder. Danach zog er ins Overlay; sichtbar war er dort nur, solange
        # das Overlay eingeblendet ist.
        #
        # Gemeldet am 26.08.2026: „den SC Starten Button sollten wir über für
        # Fortgeschrittene packen in dem markanten grün wie jetzt auch, da sieht
        # man ihn sofort." Hier ist er auf **jeder** Seite zu sehen.
        #
        # ⚠ `side='bottom'` staffelt von unten nach oben: Was **später** gepackt
        # wird, sitzt weiter oben. Dieser Knopf kommt deshalb nach dem
        # Klappbereich und landet dadurch **über** ihm.
        #
        # Nur bauen, wenn wirklich ein Startweg gefunden wurde — unter Windows
        # der RSI Launcher, unter Linux der lug-helper. Ein Knopf, der nichts
        # tut, wäre schlimmer als keiner.
        try:
            from . import pfade as pfade_start
            hat_starter = bool(pfade_start.spielstarter())
        except Exception:
            hat_starter = False
        if hat_starter:
            rahmen_start = tk.Frame(self.leisten_fuss, bg=FLAECHE)
            rahmen_start.pack(side='bottom', fill='x', padx=12, pady=(8, 2))
            self.spielknopf = rundknopf(
                rahmen_start, t('s_sp_start_knopf'),
                self._spiel_starten, self.f_klein,
                FLAECHE, ACCENT, ACCENT, BG, radius=8, polster=(12, 7))
            self.spielknopf.pack(fill='x')

    def _kofi_oeffnen(self):
        """Die Ko-fi-Seite im Browser aufmachen."""
        self._adresse_auf(KOFI_ADRESSE, t('hf_kofi_auf'), 'hauptfenster.kofi')

    def _discord_oeffnen(self):
        """Die Einladung im Browser aufmachen.

        ⚠ Die Adresse steht **fest** im Code und ist die dauerhafte Einladung
        (`CODE_OF_CONDUCT.md` nennt dieselbe). Ein Link, der irgendwann abläuft,
        führt Leute auf eine Fehlerseite und niemand merkt es.
        """
        self._adresse_auf('https://discord.gg/g2E7e6XxZC',
                          t('hf_discord_auf'), 'hauptfenster.discord')

    def _adresse_auf(self, adresse, meldung, stelle):
        """Eine Adresse aufmachen — und **sagen**, wenn es nicht geklappt hat.

        ⚠ Beide Knöpfe riefen bis rc43 `webbrowser.open()` direkt auf. Im
        AppImage öffnet das nichts (siehe `pfade.im_browser`), meldet aber auch
        keinen Fehler: Die Statuszeile sagte „wird geöffnet", und dann passierte
        nie etwas. Ein Knopf, der schweigend nichts tut, ist schlimmer als einer,
        der sagt, dass er nicht kann — dann steht wenigstens die Adresse da.
        """
        from . import pfade as pfade_browser
        self.sagen(meldung)
        self.root.update_idletasks()
        try:
            geklappt = pfade_browser.im_browser(adresse)
        except Exception as ausnahme:
            from . import fehler
            fehler.merken(stelle, ausnahme, adresse)
            geklappt = False
        if not geklappt:
            self.sagen(t('s_ub_auf_nein') % adresse)

    def _spiel_starten(self):
        """Star Citizen aus dem Werkzeug heraus hochfahren."""
        from . import pfade as pfade_start
        self.sagen(t('s_sp_start_lauft'))
        try:
            ok, grund = pfade_start.spiel_starten()
        except Exception as ausnahme:
            ok, grund = False, str(ausnahme)
        if not ok:
            self.sagen(t('s_sp_start_nein', grund))

        # --- Star Citizen starten ---------------------------------------
        # ⚠ Der Knopf stand vorher auf der Seite „Auftragstexte", also dort, wo
        # es um Bauplan-Angaben im Spiel geht. Selbst der Autor fand ihn nicht
        # wieder. Danach zog er ins Overlay; sichtbar war er dort nur, solange
        # das Overlay eingeblendet ist.
        #
        # Gemeldet am 26.08.2026: „den SC Starten Button sollten wir über für
        # Fortgeschrittene packen in dem markanten grün wie jetzt auch, da sieht
        # man ihn sofort." Genau hier ist er auf **jeder** Seite zu sehen, ohne
        # dass man ihn suchen muss.
        #
        # ⚠ `side='bottom'` staffelt von unten nach oben: Was **spaeter**
        # gepackt wird, sitzt weiter oben. Dieser Knopf kommt also nach dem
        # Klappbereich und landet dadurch **ueber** ihm.
        #
        # Nur bauen, wenn wirklich ein Startweg gefunden wurde — unter Windows
        # der RSI Launcher, unter Linux der lug-helper. Ein Knopf, der nichts
        # tut, waere schlimmer als keiner.
        try:
            hat_starter = bool(pfade.spielstarter())
        except Exception:
            hat_starter = False
        if hat_starter:
            rahmen_start = tk.Frame(self.leisten_fuss, bg=FLAECHE)
            rahmen_start.pack(side='bottom', fill='x', padx=12, pady=(8, 2))
            self.spielknopf = rundknopf(
                rahmen_start, t('s_sp_start_knopf'),
                self._spiel_starten, self.f_klein,
                FLAECHE, ACCENT, ACCENT, BG, radius=8, polster=(12, 7))
            self.spielknopf.pack(fill='x')

    def _spiel_starten(self):
        """Star Citizen aus dem Werkzeug heraus hochfahren."""
        from . import pfade as pfade_start
        self.sagen(t('s_sp_start_lauft'))
        try:
            ok, grund = pfade_start.spiel_starten()
        except Exception as ausnahme:
            ok, grund = False, str(ausnahme)
        if not ok:
            self.sagen(t('s_sp_start_nein', grund))

    # ⚠⚠ **Diese Gruppen lassen sich NICHT zuklappen** (05.09.2026).
    # In „Info" steht „Fehler melden". Wer die Gruppe zuklappt, blendet damit
    # den Weg aus, auf dem er ein Problem loswird — und sucht ihn genau dann,
    # wenn etwas klemmt und die Geduld ohnehin am Ende ist. Gemeldet mit dem
    # Satz: „Info sollte auch nicht einklappbar sein, sonst blendet jemand
    # Fehler melden aus, und findet es nicht mehr."
    #
    # ⚠ Warum nicht einfach alle festnageln: Das Zuklappen gibt es aus einem
    # guten Grund — die Seitenleiste bestimmt die Mindesthöhe des Fensters,
    # und zugeklappte Gruppen sparen rund 400 px. Festgenagelt wird deshalb
    # nur, was im Notfall auffindbar bleiben muss.
    IMMER_OFFEN = ('info',)

    def _gruppe(self, text, kennung=None):
        """Eine Gruppenüberschrift — anklicken klappt ihre Reiter weg.

        Gibt den Rahmen zurück, in den die Reiter der Gruppe gehören.

        ⭐ **Warum klappbar** (Wunsch vom 30.08.2026): Die Seitenleiste
        bestimmt mit, wie hoch das Fenster mindestens sein muss. Bei 36 px je
        Reiter waren es mit der Gruppe „Handel" 1020 px — mehr, als auf einen
        1080er Bildschirm passt. Wer Werkstatt, Handel und Einstellungen
        zuklappt, spart rund 400 px, und die Mindesthöhe geht mit.

        ⚠ **Das ersetzt weder die Deckelung noch die rollende Leiste.** Klappt
        jemand alles auf, ist der Bedarf wieder da; ohne die beiden anderen
        Maßnahmen wäre derselbe Fehler zurück.

        ⚠ Der Zustand wird gemerkt, aber **die Gruppe des offenen Reiters
        bleibt offen** (siehe `oeffnen`) — sonst verschwindet die Seite, auf
        der man gerade steht, aus der Leiste, und das sieht nach kaputt aus.
        """
        kennung = kennung or text
        fest = kennung in self.IMMER_OFFEN
        # ⚠ Eine festgenagelte Gruppe steht offen, auch wenn in den
        # Einstellungen noch ein „zu" von früher liegt. Sonst bliebe sie bei
        # allen zu, die sie einmal zugeklappt hatten — also genau bei denen,
        # um die es hier geht.
        offen = fest or not pfade.einstellung_wahrheit(
            'gruppe_zu_%s' % kennung, False)

        # ⚠ Kein Zeigefinger-Zeiger, wo es nichts zu klicken gibt: Ein Kopf,
        # der wie ein Knopf aussieht und nicht reagiert, wirkt kaputt.
        kopf = tk.Frame(self.leiste, bg=FLAECHE,
                        cursor='' if fest else 'hand2')
        kopf.pack(fill='x', pady=(10, 0))
        # ⚠ **Dasselbe Symbol wie überall sonst im Programm.** Zuerst standen
        # hier Textpfeile (`⌄`/`⌃`) — die sehen je nach Systemschrift anders aus
        # als die gezeichneten Symbole, mit denen sich der Bauplan-Fortschritt
        # und der Bestand aufklappen. Ein Werkzeug, das dieselbe Handlung an
        # zwei Stellen verschieden abbildet, muss zweimal gelernt werden.
        pfeil = zeichen.zeile(kopf, 'zuklappen' if offen else 'aufklappen',
                              grund=FLAECHE, schrift=self.f_klein)
        # ⚠ Bei einer festgenagelten Gruppe gar kein Pfeil. Ein Pfeil ist ein
        # Versprechen („hier lässt sich klappen"); eines, das nicht eingelöst
        # wird, ist schlimmer als keines.
        if not fest:
            pfeil.pack(side='right', padx=(0, 12))
        beschriftung = tk.Label(kopf, text=text.upper(), bg=FLAECHE, fg=SUB,
                                font=self.f_klein, anchor='w', padx=16, pady=6)
        beschriftung.pack(side='left', fill='x', expand=True)

        inhalt = tk.Frame(self.leiste, bg=FLAECHE)
        if offen:
            inhalt.pack(fill='x')

        self.gruppen[kennung] = {'kopf': kopf, 'inhalt': inhalt,
                                 'pfeil': pfeil, 'offen': offen,
                                 'reiter': []}

        if not fest:
            for teil in (kopf, beschriftung, pfeil):
                teil.bind('<Button-1>',
                          lambda _e, k=kennung: self._gruppe_um(k))
        return inhalt

    def _gruppe_um(self, kennung, auf=None):
        """Eine Gruppe auf- oder zuklappen. `auf=True` erzwingt das Aufklappen."""
        g = self.gruppen.get(kennung)
        if not g:
            return
        # ⚠ **Der Riegel gehört hierher, nicht nur an den Mausklick.** Diese
        # Funktion wird auch von `_gruppe_von_reiter_oeffnen` gerufen. Wer die
        # Sperre allein an die Bindung hängt, hat sie beim nächsten Aufrufer
        # nicht mehr — und der kommt bestimmt.
        if kennung in self.IMMER_OFFEN and not (auf is True or auf is None):
            return
        neu_offen = (not g['offen']) if auf is None else bool(auf)
        if kennung in self.IMMER_OFFEN and not neu_offen:
            return
        if neu_offen == g['offen']:
            return
        g['offen'] = neu_offen
        try:
            if neu_offen:
                # ⚠ **Vor dem Klappteil einordnen, nicht ans Ende.** Ohne
                # `before` landet eine wieder aufgeklappte Gruppe unter allem,
                # was von unten gepackt ist — die Reihenfolge der Leiste wäre
                # nach dem ersten Zuklappen dauerhaft durcheinander.
                g['inhalt'].pack(fill='x', after=g['kopf'])
            else:
                g['inhalt'].pack_forget()
            g['pfeil'].symbol_tauschen('zuklappen' if neu_offen
                                       else 'aufklappen')
            pfade.einstellung_setzen('gruppe_zu_%s' % kennung,
                                     'nein' if neu_offen else 'ja')
        except tk.TclError:
            pass
        self.root.after(30, self._mindesthoehe_nachziehen)

    # ⚠ Nur Zeichen aus der Grundebene benutzen. `🗀` und `⇅` liegen darüber und
    # fehlen in der Oberflächenschrift — im Fenster stand statt des Symbols ein
    # Fragezeichen. Auffallen tut das erst im laufenden Fenster, nicht im Code.
    # Prüfen lässt es sich mit `tkfont.Font.measure`: Ein fehlendes Zeichen ist
    # genauso breit wie das amtliche Ersatzzeichen `￿`.
    def _reiter(self, kennung, symbol, text, wohin=None):
        ziel = wohin if wohin is not None else self.leiste
        zeile = tk.Frame(ziel, bg=FLAECHE, cursor='hand2')
        zeile.pack(fill='x')
        strich = tk.Frame(zeile, bg=FLAECHE, width=3)
        strich.pack(side='left', fill='y')
        # ⚠ `symbol` heißt der Parameter, nicht `zeichen` — sonst verdeckt er
        # das gleichnamige Modul, aus dem das Bild kommt.
        z = zeichen.knopf(zeile, symbol, grund=FLAECHE, schrift=self.f_zeichen)
        z.pack(side='left', padx=(10, 4), pady=7)
        b = tk.Label(zeile, text=text, bg=FLAECHE, fg=SUB, font=self.f_grund,
                     anchor='w')
        b.pack(side='left', fill='x', expand=True)

        marke_widget = None
        if neuheiten.ist_neu(kennung, self.version):
            marke_widget = marke(zeile, t('hf_neu'), ACCENT, self.f_klein)
            marke_widget.pack(side='right', padx=10)

        for teil in (zeile, z, b):
            teil.bind('<Button-1>', lambda e, k=kennung: self.oeffnen(k))
        self.knoepfe[kennung] = (zeile, strich, z, b, marke_widget)

    def _seitenleiste_bedarf(self):
        """Wie viele Pixel Höhe die Seitenleiste für all ihre Einträge braucht.

        Gerechnet wird über die Kinder, nicht über den Rahmen selbst: Die Leiste
        hat eine feste Breite (`pack_propagate(False)`), und dann meldet Tk für den
        Rahmen die gesetzte Größe statt der des Inhalts.
        """
        hoch = 0
        for kind in self.leiste.winfo_children():
            try:
                # ⚠⚠ **Was nicht gepackt ist, zählt nicht.** `winfo_reqheight()`
                # meldet auch für einen weggeklappten Rahmen weiter die volle
                # Höhe seines Inhalts — der Rahmen ist ja noch da, nur nicht
                # sichtbar. Ohne diese Abfrage brachte das Zuklappen einer
                # Gruppe **null** Ersparnis: 1020 px vorher, 1020 px nachher.
                # `pack_info()` wirft bei einem nicht gepackten Widget, und
                # genau das ist hier die Auskunft.
                polster = kind.pack_info().get('pady', 0)
            except Exception:
                continue
            if isinstance(polster, str):
                polster = sum(int(teil) for teil in polster.split())
            elif isinstance(polster, (tuple, list)):
                polster = sum(int(teil) for teil in polster)
            hoch += kind.winfo_reqheight() + 2 * int(polster or 0)
        return hoch

    def _leistenbreite_nachziehen(self):
        """Die Seitenleiste so breit machen, dass der längste Eintrag hineinpasst.

        ⚠ Die Leiste hat eine feste Breite (`pack_propagate(False)`) — sonst würde
        sie mit dem Inhalt wandern. Feste Breite heißt aber auch: Was nicht
        hineinpasst, wird **abgeschnitten**, ohne Hinweis. Bei 125 % Anzeige-
        Skalierung traf das „Angaben im Spiel"; auf Englisch sind mehrere Einträge
        noch länger. Deshalb wird die Breite aus den Einträgen gemessen.
        """
        try:
            breiten = []
            for eintrag in self.knoepfe.values():
                if not eintrag or not eintrag[0]:
                    continue
                zeile, _strich, _z, beschriftung, _marke = eintrag
                # ⚠ Der **aktive** Reiter wird fett gezeichnet, und fett ist breiter.
                # Gemessen wird aber der Zustand, in dem die Zeile gerade ist — wer
                # nur `winfo_reqwidth()` nimmt, misst bei allen anderen die schmale
                # Version und macht die Leiste zu knapp. Genau deshalb war „Angaben
                # im Spiel" abgeschnitten, sobald die Seite offen war.
                zusatz = 0
                try:
                    text = beschriftung.cget('text')
                    zusatz = max(0, self.f_fett.measure(text)
                                 - self.f_grund.measure(text))
                except tk.TclError:
                    pass
                breiten.append(zeile.winfo_reqwidth() + zusatz)
            # ⚠ Den **Kopf** messen, nicht nur die Beschriftung: Seit der
            # Pfeil daneben sitzt, ist die Zeile breiter als ihr Text.
            breiten.append(self.klappkopf.winfo_reqwidth())
            noetig = max(LEISTE_BREITE, max(breiten) + 12)
            if noetig != self.leisten_spalte.winfo_width():
                self.leisten_spalte.configure(width=noetig)
                self.leisten_flaeche.configure(width=noetig)
                self.leisten_flaeche.itemconfigure(self._leisten_fenster,
                                                   width=noetig)
            return noetig
        except (tk.TclError, ValueError):
            return LEISTE_BREITE

    def _mindesthoehe_nachziehen(self, versuch=0):
        """Die Mindesthöhe an das anpassen, was die Seitenleiste braucht.

        ⚠ Gerechnet wird immer für den **aufgeklappten** Zustand — auch solange
        „Für Fortgeschrittene" noch zu ist. Sonst passte das Fenster genau, und beim
        Aufklappen war „Diagnose" unten abgeschnitten: Die Reiter werden von oben
        gepackt, der Klappteil von unten, und was dazwischen nicht hineinpasst,
        fällt heraus. Genau so gemeldet. Ein Fenster, das beim Aufklappen von selbst
        wächst, wäre die zweitbeste Lösung — es springt dann unter den Händen.

        ⚠ Gemessen wird erst, wenn Tk die Leiste wirklich gezeichnet hat. Vorher ist
        ihre Höhe 1 Pixel, und die Rechnung „Fenster minus Leiste" ergibt Unsinn —
        im ersten Anlauf kam so eine Mindesthöhe von 1418 Pixeln heraus. Ist sie noch
        nicht so weit, wird es kurz darauf noch einmal versucht.
        """
        try:
            if self.leisten_flaeche.winfo_height() < 50:
                if versuch < 10:
                    self.root.after(60, lambda: self._mindesthoehe_nachziehen(
                        versuch + 1))
                return
            # ⚠⚠ **Der Leistenbedarf bestimmt die Mindesthöhe NICHT mehr.**
            #
            # Er tat es, solange die Leiste ein fester Rahmen war: Was nicht
            # ins Fenster passte, war unerreichbar, also musste das Fenster
            # mitwachsen. Mit jedem neuen Reiter wuchs es weiter — bei der
            # Gruppe „Handel" auf über 1000 px. Auf einem 1080er Bildschirm
            # passte es dann gar nicht mehr, und selbst auf grossen Schirmen
            # liess es sich nicht kleiner ziehen als 1028 px („das fenster ist
            # zu hoch, kann es nicht kleiner ziehen", 30.08.2026).
            #
            # Seit die Leiste rollt (`_korpus`) und ihre Gruppen klappbar sind,
            # geht bei einem kürzeren Fenster nichts verloren: Was nicht
            # hinpasst, rollt. Die Mindesthöhe ist deshalb wieder eine feste
            # Zahl — `_seitenleiste_bedarf()` wird nur noch für den Rollbereich
            # gebraucht, nicht mehr für die Fenstergrösse.
            noetig = MIN_HOEHE
            # ⚠⚠ **Die Mindesthöhe darf den Bildschirm nie überschreiten.**
            #
            # Ein `minsize`, das höher ist als der Monitor, lässt sich nicht
            # mehr wegdeckeln: Tk hält es gegen jedes `geometry()`, auch gegen
            # `_auf_den_schirm_holen()` weiter unten. Das Fenster stand dann
            # über die Taskleiste hinaus, und an alles darunter kam man nicht
            # mehr heran (30.08.2026 gemeldet, nachdem die Gruppe „Handel" die
            # Leiste auf 1020 px gebracht hatte).
            #
            # Seit die Seitenleiste rollt (siehe `_korpus`), ist ein Fenster,
            # das kürzer ist als ihr Bedarf, auch kein Verlust mehr — man
            # kommt weiterhin an jeden Eintrag.
            from . import bildschirm as _bs
            try:
                _, _, _, schirm_hoch = _bs.schirm_fuer(
                    self.root, self.root.winfo_x(), self.root.winfo_y())
                if schirm_hoch and schirm_hoch > 200:
                    noetig = min(noetig, schirm_hoch)
            except Exception as ausnahme:
                # Lieber die alte, womöglich zu große Höhe als gar kein Fenster.
                fehler.merken('hauptfenster.schirmhoehe', ausnahme)
            # Wird die Leiste breiter, braucht auch das Fenster mehr — sonst geht
            # der Platz auf Kosten des Inhalts daneben.
            leiste_breit = self._leistenbreite_nachziehen()
            breit = MIN_BREITE + max(0, leiste_breit - LEISTE_BREITE)
            self.root.minsize(breit, noetig)
            if self.root.winfo_height() < noetig or self.root.winfo_width() < breit:
                self.root.geometry('%dx%d' % (max(breit, self.root.winfo_width()),
                                              max(noetig, self.root.winfo_height())))
            self._auf_den_schirm_holen()
        except tk.TclError:
            pass

    def _auf_den_schirm_holen(self):
        """Das Fenster auf dem Bildschirm halten, auf dem es gerade steht.

        ⚠⚠ **Bei „Sehr groß" wuchs das Fenster über den Monitor hinaus.** Die
        Schriftgröße vergrößert Schrift, Symbole und Knöpfe; daraus folgt eine
        größere Mindesthöhe, und die wurde gesetzt, ohne zu fragen, ob sie
        überhaupt auf den Bildschirm passt. Bei zwei übereinander stehenden
        49-Zoll-Monitoren lief das Fenster in den zweiten hinein. Am 30.08.2026
        gemeldet.

        ⚠ Tk hilft hier nicht: `winfo_screenheight()` meldet die Höhe **aller**
        Bildschirme zusammen — bei zwei übereinander also das Doppelte. Für
        „passt das?" ist das die falsche Zahl. `bildschirm.schirm_fuer()`
        liefert den Monitor, auf dem das Fenster wirklich steht.

        Verschoben wird nur, was muss: Wer sein Fenster selbst irgendwohin
        zieht, soll es dort wiederfinden.
        """
        from . import bildschirm as _bs
        try:
            x, y = self.root.winfo_x(), self.root.winfo_y()
            b, h = self.root.winfo_width(), self.root.winfo_height()
            if b < 50 or h < 50:              # noch nicht angezeigt
                return
            sx, sy, sb, sh = _bs.schirm_fuer(self.root, x, y)
            # ⚠ Erst die Größe deckeln, dann die Lage — sonst schiebt man ein
            # zu großes Fenster hin und her und es ragt trotzdem heraus.
            neu_b, neu_h = min(b, sb), min(h, sh)
            neu_x = max(sx, min(x, sx + sb - neu_b))
            neu_y = max(sy, min(y, sy + sh - neu_h))
            if (neu_b, neu_h, neu_x, neu_y) != (b, h, x, y):
                self.root.geometry('%dx%d+%d+%d' % (neu_b, neu_h, neu_x, neu_y))
        except (tk.TclError, ValueError, TypeError) as ausnahme:
            fehler.merken('hauptfenster.schirm', ausnahme)

    def _klapp_umschalten(self):
        self.fortgeschritten_offen = not self.fortgeschritten_offen
        # Der Pfeil zeigt, was ein Klick tut — wie bei den Gruppenüberschriften.
        try:
            self.klapppfeil.symbol_tauschen(
                'zuklappen' if self.fortgeschritten_offen else 'aufklappen')
        except (AttributeError, tk.TclError):
            pass
        if self.fortgeschritten_offen:
            self.klappinhalt.pack(fill='x')
            if not self.klappinhalt.winfo_children():
                # Pfade liegen hier unten, seit die Erkennung sie selbst
                # findet: Spielordner und Launcher werden gesucht, und wer doch
                # nachhelfen muss, wird vom Einrichtungsassistenten geführt —
                # der erklärt, was die Seite nur als Felder zeigt. Ein Reiter,
                # den fast niemand braucht, steht oben nur im Weg.
                self._reiter('ordner', 'ordner', t('hf_ordner'), self.klappinhalt)
                self._reiter('erkennung', 'erkennung', t('hf_erkennung'), self.klappinhalt)
                # ⚠ **Bauplan-Bestand gehört hierher, nicht in die offene
                # Liste.** Die Seite schreibt am eigenen Bestand — einlesen,
                # überschreiben, zurücksetzen. Am 30.08.2026 hat sie genau
                # deshalb schon einen Fehler ausgelöst: Sie stand zwischen
                # „Anzeige" und „Texte im Spiel", also zwischen lauter
                # harmlosen Seiten, und wurde nebenbei angeklickt.
                #
                # Hinter dem zugeklappten „Für Fortgeschrittene" ist sie
                # weiterhin erreichbar, aber nicht mehr im Vorbeigehen.
                self._reiter('bestand', 'bestand', t('hf_bestand'),
                             self.klappinhalt)
            self.klappknopf.configure(text=t('hf_fortgeschritten'))
        else:
            self.klappinhalt.pack_forget()
            self.klappknopf.configure(text=t('hf_fortgeschritten'))
        # Kurz warten, statt `after_idle`: Vorher hat Tk die neuen Einträge noch
        # nicht vermessen — und `after_idle` kommt hier nicht zuverlässig dran,
        # weil die Bauplan-Liste selbst Leerlauf-Aufgaben nachlegt.
        self.root.after(30, self._mindesthoehe_nachziehen)

    # ------------------------------------------------------------ Seitenwahl
    def _gruppe_von_reiter_oeffnen(self, kennung):
        """Die Gruppe aufklappen, in der dieser Reiter sitzt."""
        eintrag = self.knoepfe.get(kennung)
        if not eintrag or not eintrag[0]:
            return
        elternrahmen = eintrag[0].master
        for name, g in self.gruppen.items():
            if g['inhalt'] is elternrahmen and not g['offen']:
                self._gruppe_um(name, auf=True)
                return

    def oeffnen(self, kennung):
        """Eine Seite zeigen — und beim ersten Mal ihren Inhalt bauen."""
        # ⚠⚠ **Ein Seitenwechsel ist die deutlichste Nutzeraktion überhaupt.**
        # Ohne diese Zeile lief der Vorbau munter weiter, während die gerade
        # angeklickte Seite noch gezeichnet wurde: Sie meldete `steht (3 ms)`,
        # war aber sekundenlang nicht zu sehen, weil Tk mit dem Vorbau der
        # nächsten Seite beschäftigt war. Gemeldet am 02.09.2026 als „bauplan
        # langsam", nachdem die linke Leiste bereits schnell war.
        self._aktion_merken()
        # ⚠ **Die Gruppe des Reiters muss offen sein.** Sonst steht man auf
        # einer Seite, deren Eintrag in der Leiste gar nicht zu sehen ist — das
        # sieht nach einem Fehler aus, und der Weg zurück ist nicht zu finden.
        # Betrifft vor allem den Programmstart: Die zuletzt benutzte Seite kann
        # in einer zugeklappten Gruppe liegen.
        self._gruppe_von_reiter_oeffnen(kennung)
        if not hasattr(self, 'beim_zeigen'):
            # kennung -> Funktion, die beim erneuten Anzeigen laeuft
            self.beim_zeigen = {}
        if kennung not in self.seiten:
            self.seiten[kennung] = tk.Frame(self.inhalt, bg=BG)
        # ⚠ Beim **zweiten** Besuch wurde bisher nur „steht" geschrieben, weil
        # die Seite schon gebaut war. Knallte es dabei, fehlte die Zeile ganz
        # statt nur zur Hälfte — und die Überschrift des Berichts verspricht
        # „die letzte Zeile ohne ‚steht' ist die, an der es hing". Das stimmte
        # dann nicht mehr. Aufgefallen im rc75-Bericht, notiert für dieses
        # Release.
        #
        # Deshalb auch hier eine Zeile, aber eine eigene: „zeigen" statt
        # „bauen beginnt". Wer den Bericht liest, sieht damit den Unterschied
        # zwischen „beim Aufbauen gestorben" und „beim Einblenden gestorben".
        # ⚠⚠ **Millisekunden, nicht nur Sekunden.** Gemeldet am 02.09.2026
        # (Haldjas, pr0): „Er braucht eben recht lang, um die Icons und co zu
        # laden, wenn man die Einstellungen oeffnet." Im Bericht standen nur
        # sekundengenaue Zeitstempel — damit liess sich nicht unterscheiden,
        # ob eine Seite 50 ms oder 900 ms braucht. Zwei Erklaerungen wurden
        # dadurch gejagt und beide widerlegt (Schriftgroesse, Zahl der
        # Symbolbilder: gemessen 36 Bilder in 4 ms). Ohne Zahl im Bericht
        # bleibt es beim Raten.
        _beginn = time.perf_counter()
        if kennung in self.gezeichnet:
            fehler.spur('Seite %s: zeigen' % kennung)
            # ⚠ Eine Seite wird **einmal** gebaut und danach nur noch ein- und
            # ausgeblendet. Alles, was beim erneuten Aufrufen frisch sein soll,
            # muss sich deshalb hier melden — sonst steht der Suchbegriff von
            # vorhin noch da. Am 29.08.2026 gemeldet: „da sollte man den
            # Titan-Eintrag im Suchfeld nicht speichern."
            ruf = self.beim_zeigen.get(kennung)
            if ruf:
                try:
                    ruf()
                except Exception as ausnahme:
                    fehler.merken('hauptfenster.zeigen:%s' % kennung, ausnahme)
        else:
            self.gezeichnet.add(kennung)
            # ⚠ Die Spur führt jetzt auch über die Bedienung, nicht nur über den
            # Start. Grund: Bomb20 meldete am 27.08.2026 einen reproduzierbaren
            # Absturz beim Öffnen von „Was ist neu" — und sein Bericht wusste
            # nichts davon. Die Fehlerhaken greifen nur bei Python-Ausnahmen,
            # und die Spur endete beim letzten Startschritt. Fehlt die zweite
            # Zeile hier, hat es beim Bauen genau dieser Seite geknallt.
            fehler.spur('Seite %s: bauen beginnt' % kennung)
            try:
                self._seite_fuellen(kennung, self.seiten[kennung])
            except Exception as ausnahme:
                fehler.merken('hauptfenster.seite:%s' % kennung, ausnahme)
                tk.Label(self.seiten[kennung], text='—', bg=BG, fg=SUB,
                         font=self.f_grund).pack(padx=20, pady=20)

        if self.aktuell:
            self.seiten[self.aktuell].pack_forget()
        self.seiten[kennung].pack(fill='both', expand=True)
        self.aktuell = kennung
        fehler.spur('Seite %s: steht (%.0f ms)'
                    % (kennung, (time.perf_counter() - _beginn) * 1000))
        # ⚠ Erst JETZT die restlichen Seiten im Leerlauf vorbauen — nachdem die
        # angeklickte steht. Vorher gestartet, wuerde der Vorbau genau die
        # Seite verzoegern, die der Mensch gerade sehen will.
        # `_vorbau_laeuft` sorgt dafuer, dass das nur einmal je Fenster
        # anlaeuft; bei jedem Reiterwechsel neu anzustossen haette mehrere
        # Ketten parallel erzeugt.
        if not getattr(self, '_vorbau_laeuft', False):
            self._vorbau_laeuft = True
            if VORBAU_AN:
                self._letzte_aktion = time.monotonic()
                # ⚠ Jede Eingabe verschiebt den Vorbau nach hinten — siehe
                # `_seiten_vorbauen`. `add='+'` ist Pflicht, sonst verdraengt
                # das die Haken, die andere Bausteine global gesetzt haben.
                for ereignis in ('<Button>', '<Key>', '<MouseWheel>'):
                    try:
                        self.root.bind_all(ereignis, self._aktion_merken,
                                           add='+')
                    except Exception:
                        pass
                self.root.after(400, self._seiten_vorbauen)
        self._reiter_faerben()
        # Der aktive Reiter wird fett — und fett ist breiter. Die Leiste muss
        # deshalb bei jedem Wechsel nachmessen, sonst wird der längste Eintrag
        # genau dann abgeschnitten, wenn man auf ihm steht.
        self._leistenbreite_nachziehen()

        # Die „neu"-Marke hat ihren Zweck erfüllt, sobald man drin war.
        if neuheiten.ist_neu(kennung, self.version):
            neuheiten.gesehen(kennung, self.version)
            eintrag = self.knoepfe.get(kennung)
            if eintrag and eintrag[4] is not None:
                eintrag[4].destroy()
                # ⚠ Und aus der Liste nehmen! Ein zerstörtes Widget bleibt sonst
                # im Tupel stehen, und das nächste Einfärben greift ins Leere
                # (`invalid command name`). Das schlug beim zweiten Reiterwechsel
                # zu — also bei jedem Nutzer sofort.
                zeile, strich, z, b, _ = eintrag
                self.knoepfe[kennung] = (zeile, strich, z, b, None)

    def _fehler_liegen_an(self):
        """Wurde seit dem Start etwas mitgeschrieben? Faerbt das Reiter-Symbol.

        Gefragt wird bei jedem Neuzeichnen der Leiste — also bei jedem
        Seitenwechsel. Das genuegt: Wer gerade auf einen Fehler laeuft, klickt
        ohnehin weiter, und dann steht die Farbe.
        """
        try:
            from . import fehler as fehler_modul
            return fehler_modul.anzahl() > 0
        except Exception:
            return False

    def _reiter_faerben(self):
        for kennung, (zeile, strich, z, b, marke) in self.knoepfe.items():
            an = (kennung == self.aktuell)
            grund = '#1d2634' if an else FLAECHE
            for teil in (zeile, z, b):
                teil.configure(bg=grund)
            if marke is not None:
                marke.hintergrund(grund)
            # ⚠ Ein Bild nimmt kein `fg` an — die passend eingefärbte Version
            # muss eingehängt werden.
            # ⚠ „Fehler melden“ traegt Rot — aber in zwei Stufen, damit die
            # Farbe etwas bedeutet und nicht nur schmueckt:
            #
            #   * **Das Wort ist immer rot.** Wer ein Problem hat, soll den
            #     Reiter finden, ohne ein Menue zu durchsuchen. der Autor am
            #     28.08.2026: „damit wirklich niemand uebersieht“.
            #   * **Das Symbol wird nur rot, wenn wirklich etwas passiert ist**
            #     — wenn also Fehler mitgeschrieben wurden. Sonst stuende der
            #     Reiter dauerhaft auf Alarm, obwohl alles laeuft, und niemand
            #     naehme ihn noch ernst.
            #
            # Der Strich darunter bleibt gruen, wenn die Seite offen ist —
            # sonst saehe die gewaehlte Seite aus wie eine Warnung.
            rot = (kennung == 'diagnose')
            z.faerben(zeichen.ROT if (rot and self._fehler_liegen_an())
                      else (zeichen.HELL if an else zeichen.GRAU))
            b.configure(fg=ROT if rot else (FG if an else SUB),
                        font=self.f_fett if (an or rot) else self.f_grund)
            strich.configure(bg=ACCENT if an else FLAECHE)

    # Die Seiten, auf denen der eigene Bauplan-Bestand steht. Ändert er sich,
    # sind ihre Zahlen falsch — und zwar still, ohne dass irgendetwas darauf
    # hinweist.
    # ⚠ Nachgeprüft, nicht geraten: Genau diese vier lesen `bestand_datei` beim
    # Bauen — „Wie weit bin ich", „Herstellung", „Sichern & Übertragen" (die
    # Zahl über den Ausgabe-Knöpfen) und „Über". `allgemein` und `erkennung`
    # stehen bewusst NICHT hier: Sie zeigen nur Katalogzahlen, und die ändern
    # sich durch einen eigenen Fund nicht.
    BESTANDSSEITEN = ('fortschritt', 'herstellung', 'bestand', 'ueber')

    def bestand_geaendert(self):
        """Sagt allen Seiten Bescheid, die den eigenen Bestand anzeigen.

        ⚠⚠ **Gemeldet von Bushwick4712 am 05.09.2026** für die Bauplan-Liste.
        Beim Nachsehen hatten vier weitere Seiten denselben Fehler: Sie lesen
        den Bestand beim Bauen, werden danach nur ein- und ausgeblendet und
        zeigen deshalb für den Rest der Sitzung den Stand von damals. Wer
        einen Bauplan bekommt, sieht auf „Wie weit bin ich" weiter die alte
        Zahl — auch nach dem Wechseln auf eine andere Seite und zurück.

        **Zwei verschiedene Wege, mit Absicht:**

        - Die **Liste** frischt sich sofort auf. Sie kann das verlustfrei:
          Suche, Filter und Ausklapp-Zustände bleiben, nur die Daten sind neu.
          Genau dort schaut man hin, wenn ein Bauplan fällt.
        - Die **übrigen** werden nur verworfen und beim nächsten Öffnen neu
          gebaut. Sie unter den Händen des Nutzers neu aufzubauen würde
          aufgeklappte Bereiche zuklappen und die Rollposition verlieren — für
          eine Zahl, die er in dem Moment gar nicht ansieht.

        ⚠ Die gerade sichtbare Seite bleibt deshalb stehen, wie sie ist. Sie
        ist beim nächsten Öffnen frisch, und das ist der Moment, in dem
        jemand hinschaut.
        """
        seite = getattr(self, 'bestandsseite', None)
        if seite is not None:
            try:
                seite.neu_laden()
            except Exception as ausnahme:
                from . import fehler
                fehler.merken('hauptfenster.bestand_liste', ausnahme)

        for kennung in self.BESTANDSSEITEN:
            if kennung == self.aktuell or kennung not in self.gezeichnet:
                continue
            try:
                rahmen = self.seiten.get(kennung)
                if rahmen is None:
                    continue
                for kind in rahmen.winfo_children():
                    kind.destroy()
                self.gezeichnet.discard(kennung)
            except Exception as ausnahme:
                from . import fehler
                fehler.merken('hauptfenster.bestand_verwerfen:%s' % kennung,
                              ausnahme)

    def neu_aufbauen(self):
        """Alles neu zeichnen — nach einem Sprachwechsel.

        Texte stehen in der Reiterleiste, in der Titelleiste, in der Fußzeile
        und auf jeder Seite. Einzeln nachzuziehen wäre zwanzig Stellen, die man
        vergessen kann; einmal neu aufbauen ist verlässlicher.
        """
        merker = self.aktuell
        offen = self.fortgeschritten_offen
        for kind in self.root.winfo_children():
            kind.destroy()
        self.seiten, self.gezeichnet, self.knoepfe = {}, set(), {}
        # ⚠ Mit zuruecksetzen: Sonst liefe der Vorbau nach einem Neuaufbau
        # (Sprache, Schriftgroesse) nie wieder an — die Seiten sind ja alle
        # weg, aber die Sperre stuende noch.
        self._vorbau_laeuft = False
        self.aktuell = None
        self._einst = None            # das geliehene Einstellungsfenster ist weg
        self.fortgeschritten_offen = False

        self._titelleiste()
        self._fusszeile()
        self._korpus()
        if offen:
            self._klapp_umschalten()
        self.oeffnen(merker or 'liste')

        # ⚠ Die Mindestgroesse muss mitwachsen. Sie haengt an der Hoehe der
        # Seitenleiste, und die haengt an der Schrift: Bei „sehr gross" braucht
        # sie mehr Platz, als das Fenster hoch ist — dann fallen „Star Citizen
        # starten", „Kaffee spendieren" und „Discord" unten heraus, weil sie von
        # unten gepackt werden. Genau so gemeldet von Gemeldet am 27.08.2026:
        # „wenn jemand so schlecht sehen sollte, was ja moeglich ist, dann muss
        # die minimale groesse eben im verhaeltnis mitwachsen."
        #
        # Gerechnet hat das `_mindesthoehe_nachziehen()` schon immer richtig —
        # es lief nur beim Start und beim Aufklappen, nie nach einem Schrift-
        # oder Sprachwechsel. Hier ist der richtige Ort: Wer neu aufbaut, hat
        # neue Masse. Ueber `after`, weil Tk die Leiste erst zeichnen muss —
        # vorher meldet sie 1 Pixel Hoehe (die Funktion faengt das ab und
        # versucht es erneut).
        self.root.after(50, self._mindesthoehe_nachziehen)

    def _seite_fuellen(self, kennung, rahmen):
        """Hier hängen die Seiten ein — geliefert von `seiten.py`."""
        from . import seiten
        seiten.bauen(self, kennung, rahmen)

    # ⚠⚠ **So lange muss Ruhe sein, bevor im Hintergrund gebaut wird.**
    # Tk zeichnet einstraengig: Jede vorgebaute Seite haelt die Oberflaeche
    # fest. Am 02.09.2026 gemessen, waehrend jemand das Fenster bediente —
    # `wasistneu` 181 ms, `diagnose` 153 ms, in Summe **1,7 Sekunden** ueber
    # 17 Seiten. Gemeldet wurde das als „linke leiste laed langsamer" bzw.
    # „bauplan liste weiterhin langsam", je nachdem, was gerade angefasst
    # wurde — dasselbe Stocken, nur an wechselnder Stelle.
    VORBAU_RUHE_S = 1.2
    VORBAU_NACHFRAGE_MS = 400

    def _aktion_merken(self, _ereignis=None):
        """Zeitpunkt der letzten Eingabe — der Vorbau richtet sich danach."""
        self._letzte_aktion = time.monotonic()

    def _seiten_vorbauen(self, rest=None):
        """Die noch leeren Seiten nacheinander im Leerlauf bauen.

        ⚠ **Warum das nötig ist.** Jede Seite entsteht erst beim ersten
        Aufruf. Gemessen am 02.09.2026 im eigenen Startverlauf:

            00:51:48  Seite wasistneu: bauen beginnt
            00:51:49  Seite wasistneu: steht

        Eine volle Sekunde, in der das Fenster keine Klicks annimmt — und das
        einmal je Seite. Aus dem Testlauf gemeldet: „ich muss das
        Einstellungsfenster einmal zu und erneut aufmachen, ehe ich etwas
        auswählen kann."

        ⚠ Das macht nichts **schneller** — dieselbe Arbeit fällt weiter an, nur
        eben bevor jemand darauf wartet. Ehrlich bleiben: verlagert, nicht
        beschleunigt.

        ⚠⚠ **Eine Seite je Durchlauf, nicht alle am Stück.** Tk zeichnet im
        selben Strang; neunzehn Seiten hintereinander würden das Fenster
        sekundenlang festhalten — also genau der Fehler, den das hier beheben
        soll, nur schlimmer. `after()` gibt die Ereignisschleife zwischendurch
        frei, sodass Klicks sofort ankommen.

        ⚠ Klickt jemand währenddessen auf eine noch nicht vorgebaute Seite,
        baut `oeffnen()` sie sofort selbst und trägt sie in `gezeichnet` ein —
        hier wird sie dann übersprungen. Doppelt gebaut wird nie.
        """
        try:
            if rest is None:
                from . import seiten
                rest = [k for k in seiten.kennungen()
                        if k not in self.gezeichnet]
            if not rest:
                return
            # ⚠⚠ **Erst bauen, wenn der Nutzer eine Weile nichts getan hat.**
            # Ohne das lief der Vorbau mitten in die Bedienung hinein und hielt
            # bei jeder Seite die Oberflaeche fest. Er hat es nicht eilig — die
            # Seiten werden gebraucht, wenn jemand sie anklickt, und bis dahin
            # ist meistens laengst Ruhe gewesen.
            still = time.monotonic() - getattr(self, '_letzte_aktion', 0.0)
            if still < self.VORBAU_RUHE_S:
                self.root.after(self.VORBAU_NACHFRAGE_MS,
                                lambda r=rest: self._seiten_vorbauen(r))
                return
            kennung, rest = rest[0], rest[1:]
            if kennung not in self.gezeichnet:
                self.gezeichnet.add(kennung)
                if kennung not in self.seiten:
                    self.seiten[kennung] = tk.Frame(self.inhalt, bg=BG)
                # ⚠⚠ **Den Eingabefokus retten.** Manche Seiten setzen ihn beim
                # Bauen selbst — das Suchfeld der Bauplan-Liste ruft
                # `feld.focus_set()`, damit man sofort tippen kann. Beim
                # ANZEIGEN ist das richtig; beim Vorbauen im Hintergrund
                # klaut eine unsichtbare Seite damit den Fokus, und die
                # Eingabe im sichtbaren Feld kommt nicht mehr an.
                #
                # Gemeldet am 02.09.2026, unmittelbar nach rc4: „kann bei BP
                # Suche nichts mehr eingeben … marker das ich nun text
                # eingeben kann fehlt ebenso." Ein selbst eingebauter Fehler,
                # entstanden aus einer Verbesserung.
                vorher = None
                try:
                    vorher = self.root.focus_get()
                except Exception:
                    pass
                _t_vor = time.perf_counter()
                try:
                    self._seite_fuellen(kennung, self.seiten[kennung])
                    # ⚠ Diagnose (02.09.2026): Der Vorbau laeuft 400 ms nach
                    # dem Oeffnen los und haelt Tk je Seite fest — waehrend
                    # dieser Zeit reagiert das Fenster traege. Gemeldet als
                    # „bauplan liste weiterhin langsam", obwohl der Aufbau
                    # selbst nur noch 88 ms braucht. Ohne Zahlen bleibt es
                    # beim Raten, welche Seite wie lange blockiert.
                    fehler.spur('Vorbau %s: %d ms'
                                % (kennung,
                                   round((time.perf_counter() - _t_vor) * 1000)))
                except Exception as ausnahme:
                    # Eine Seite, die sich nicht bauen laesst, darf die
                    # anderen nicht aufhalten — beim Anklicken zeigt
                    # `oeffnen()` denselben Platzhalter.
                    fehler.merken('hauptfenster.vorbau:%s' % kennung, ausnahme)
                # Und zurueckgeben, was die vorgebaute Seite sich genommen hat.
                # ⚠ Nur, wenn es das Widget noch gibt: Beim Neuaufbau des
                # Fensters ist der alte Fokus-Halter schon zerstoert, und
                # `focus_set()` darauf wirft einen TclError.
                try:
                    if vorher is not None and vorher.winfo_exists():
                        vorher.focus_set()
                except Exception:
                    pass
            if rest:
                self.root.after(60, lambda: self._seiten_vorbauen(rest))
        except Exception as ausnahme:
            fehler.merken('hauptfenster.seiten_vorbauen', ausnahme)

    # ------------------------------------------------------------------ Tat
    def _was_ist_neu(self):
        """Kein eigenes Fenster mehr — die Änderungen sind ein Reiter.

        Ein Fenster über dem Fenster verdeckt genau das, was man gerade
        vergleichen will, und es gibt keinen Grund dafür: Der Platz ist da.
        """
        self.oeffnen('wasistneu')

    def _einrichtung(self):
        from . import assistent
        try:
            assistent.starten(self.root)
        except Exception as ausnahme:
            fehler.merken('hauptfenster.assistent', ausnahme)

    def _sicherung(self):
        """Alles Eigene in eine Datei — oder eine solche Datei einspielen.

        ⚠ **Zwei Wege hinter einem Knopf.** Sichern ist der haeufige Fall,
        Einspielen der seltene (Rechnerwechsel, neu aufgesetzt) — aber genau
        dafuer sichert man ueberhaupt. Ein Knopf, der nur schreiben kann,
        loest das halbe Problem und laesst den Spieler beim anderen allein.
        """
        from . import dateiwahl, sicherung
        try:
            wahl = wahl_stellen(
                self.root, t('sich_titel'),
                t('sich_lead') + '\n\n' + t('sich_was'),
                t('sich_schreiben'), t('sich_lesen'))
            if wahl == 'a':
                self._sicherung_schreiben(dateiwahl, sicherung)
            elif wahl == 'b':
                self._sicherung_lesen(dateiwahl, sicherung)
        except Exception as ausnahme:
            fehler.merken('hauptfenster.sicherung', ausnahme)

    def _sicherung_schreiben(self, dateiwahl, sicherung):
        ziel = dateiwahl.datei_speichern(
            t('sich_schreiben'), vorschlag=sicherung.vorschlag(),
            endung='.zip', muster=(('ZIP', '*.zip'),))
        if not ziel:
            return
        ok, meldung, anzahl = sicherung.schreiben(ziel, self.version)
        if ok:
            self.sagen(t('sich_fertig', anzahl, os.path.basename(meldung)))
        elif meldung == 'leer':
            self.sagen(t('sich_leer'))
        else:
            self.sagen(t('sich_fehler', meldung))

    def _belegung_anbieten(self, quelle, sicherung):
        """Die gesicherte Steuerung anbieten — zwei Fragen, nicht eine.

        Die erste betrifft die **Profile**: Sie kommen nur dazu, es geht nichts
        verloren. Die zweite betrifft die **aktive** Belegung, und die ist eine
        andere Größenordnung — deshalb wird sie einzeln gestellt und steht
        voreingestellt auf Nein.
        """
        try:
            aktiv_dabei, profile = sicherung.belegung_im_archiv(quelle)
            if not (aktiv_dabei or profile):
                return
            was = ', '.join(profile) if profile else t('sich_belegung_keine')
            if not frage_stellen(self.root, t('sich_titel'),
                                 t('sich_belegung_frage', was)):
                return
            mit_aktiver = aktiv_dabei and frage_stellen(
                self.root, t('sich_titel'), t('sich_belegung_aktiv'))
            ok, _meldung, geschrieben = sicherung.belegung_zurueckholen(
                quelle, mit_aktiver=mit_aktiver)
            if ok:
                self.sagen(t('sich_belegung_ok', geschrieben))
        except Exception as ausnahme:
            # ⚠ Ein Fehler hier darf das Einspielen nicht mitreißen — der
            # Bestand ist zu diesem Zeitpunkt bereits zurück.
            fehler.merken('hauptfenster.belegung_anbieten', ausnahme)

    def _sicherung_lesen(self, dateiwahl, sicherung):
        quelle = dateiwahl.datei_oeffnen(t('sich_lesen'),
                                         muster=(('ZIP', '*.zip'),))
        if not quelle:
            return
        # ⚠ Erst nachsehen, dann fragen, dann erst schreiben. Wer sich in der
        # Datei vergreift, soll das erfahren, BEVOR sein Bestand weg ist.
        gueltig, anzahl, wann = sicherung.pruefen(quelle)
        if not gueltig:
            self.sagen(t('sich_ungueltig'))
            return
        if not frage_stellen(self.root, t('sich_titel'),
                             t('sich_frage', wann or '?', anzahl)):
            return
        ok, meldung, anzahl = sicherung.zurueckholen(quelle)
        if not ok:
            self.sagen(t('sich_fehler', meldung))
            return
        # ⚠⚠ **Die Steuerung kommt getrennt und nur auf Nachfrage.** Sie liegt
        # im Spielordner, nicht in unserer Ablage — und wer die aktive Belegung
        # aus einer fremden Sicherung bekommt, sitzt vor einem Schiff, das auf
        # nichts mehr reagiert. Profile dazuzulegen ist dagegen harmlos: Sie
        # liegen nur herum, bis jemand eines lädt.
        self._belegung_anbieten(quelle, sicherung)
        self.sagen(t('sich_zurueck_ok', anzahl))
        # ⚠⚠ Neustart ist Pflicht, keine Hoeflichkeit: Bestand, Lager und
        # Protokoll liegen im Arbeitsspeicher und wuerden beim naechsten
        # Speichern ueber die gerade eingespielten Dateien geschrieben.
        #
        # ⚠ Aus dem Quellcode heraus kann sich das Programm nicht selbst
        # ersetzen (`neu_starten` meldet dann False) — dann bleibt nur der
        # ehrliche Hinweis. Stillschweigend weiterlaufen waere das Schlimmste:
        # Der Spieler sieht seinen alten Stand und haelt das Einspielen fuer
        # gescheitert, waehrend die Dateien laengst da sind.
        def _neustart():
            from . import aktualisierung
            try:
                if not aktualisierung.neu_starten():
                    self.sagen(t('sich_neustart_selbst'))
            except Exception as ausnahme:
                fehler.merken('hauptfenster.sicherung_neustart', ausnahme)
                self.sagen(t('sich_neustart_selbst'))

        self.root.after(1200, _neustart)

    def _groesse_beobachten(self, ereignis):
        """Auf Groessenaenderungen horchen — aber nicht bei jedem Pixel schreiben.

        ⚠ **Gedrosselt.** `<Configure>` feuert waehrend des Ziehens
        ununterbrochen; ungebremst schriebe das Werkzeug die Einstellungsdatei
        hundertfach in der Sekunde. Gespeichert wird erst, wenn eine halbe
        Sekunde lang nichts mehr passiert ist.

        ⚠ **Nur das Fenster selbst.** Jedes Widget bekommt sein eigenes
        `<Configure>`; ohne diese Abfrage zaehlte auch jede Listenzeile mit.
        """
        if ereignis.widget is not self.root:
            return
        if self._groesse_wartet:
            try:
                self.root.after_cancel(self._groesse_wartet)
            except Exception:
                pass
        self._groesse_wartet = self.root.after(500, self._groesse_merken)

    def _groesse_merken(self):
        """Die eingestellte Groesse sichern.

        ⚠ **Nur im normalen Zustand.** Ein maximiertes Fenster meldet die
        volle Bildschirmgroesse; gemerkt wuerde damit eine Groesse, die beim
        naechsten Start als *nicht* maximiertes Fenster bis unter die
        Taskleiste reicht. Wer maximiert, findet beim naechsten Start seine
        letzte selbst gezogene Groesse vor — das ist die ehrlichere Antwort.
        """
        self._groesse_wartet = None
        try:
            if self.root.state() != 'normal':
                return
            breite, hoehe = self.root.winfo_width(), self.root.winfo_height()
        except Exception:
            return
        # Solange nichts gezeichnet ist, meldet Tk eine 1 — das ist keine Groesse.
        if breite < MIN_BREITE or hoehe < MIN_HOEHE:
            return
        wert = '%dx%d' % (breite, hoehe)
        if wert == (pfade.einstellung(GROESSE_SCHLUESSEL) or ''):
            return
        pfade.einstellung_setzen(GROESSE_SCHLUESSEL, wert)

    def schliessen(self):
        # Beim Zumachen noch einmal sichern: Wer das Fenster kurz nach dem
        # Ziehen schliesst, waere sonst schneller als die Drossel.
        try:
            self._groesse_merken()
        except Exception as ausnahme:
            fehler.merken('hauptfenster.groesse_merken', ausnahme)
        try:
            if self.beim_schliessen:
                self.beim_schliessen()
        finally:
            self.root.destroy()

    def run(self):
        """Das Fenster zeigen und auf Eingaben warten.

        ⚠ Erst nach vorn holen. Ein frisch gestartetes Fenster liegt sonst
        hinter dem, was gerade offen war — gemeldet als „es startet, aber ich
        sehe nichts", während das Fenster nachweislich gebaut und sichtbar
        war (1040×760, Zustand „normal"), nur eben verdeckt. Besonders auf
        dem Mac: Wird das Programm aus einem Terminal gestartet, behält das
        Terminal den Vordergrund.

        `-topmost` wird gleich wieder abgeschaltet — es soll nach vorn
        kommen, aber nicht dauerhaft über allem kleben.
        """
        # Beim Start ausdrücklich MIT Fokus: Der Nutzer hat das Werkzeug
        # gerade selbst gestartet und will hin.
        nach_vorn(self.root, fokus=True)
        self.root.mainloop()


def _mitgeliefert(name):
    """Pfad zu einer mitgelieferten Datei — im Quellcode wie im fertigen Paket."""
    try:
        basis = getattr(sys, '_MEIPASS', None) or os.path.dirname(
            os.path.dirname(os.path.abspath(__file__)))
        return os.path.join(basis, name)
    except Exception:
        return None


# --------------------------------------------------------------- Frage-Dialog
#
# ⚠ **Warum nicht `messagebox.askyesno`.** Der System-Dialog von Tk ist auf
# einem dunklen Programm ein Fremdkörper: heller Kasten, fremde Schrift — und
# seine Knöpfe holt er aus Tks eigener Sprachtabelle, die auf vielen
# Linux-Systemen unvollständig ist. Ergebnis war deutscher Text über den
# Knöpfen **Yes / No** (gemeldet, 28.08.2026). Die Sprache liesse sich über
# `msgcat` flicken, das Aussehen nicht: Farben und Breite gibt der Dialog nicht
# her, und er wird **hoch statt breit** — bei einem längeren Satz eine schmale
# Säule.
#
# Deshalb ein eigener. Er kostet wenig, sieht aus wie das Programm und ist in
# beiden Sprachen richtig beschriftet.
FRAGE_BREITE = 620          # bewusst breit: Am 28.08.: "eher breiter statt hoch"

# Wieviele Eintraege eine Auswahlliste im Dialog zeigt, bevor sie abkuerzt.
# ⚠ Sieben, damit der Dialog nicht ueber den Bildschirmrand waechst und die
# Knoepfe unten erreichbar bleiben — dieselbe Falle wie beim Overlay.
LISTE_SICHTBAR = 7


def _dialog_knopf(eltern, text, tat, schrift, stark=False):
    """Knopf im Programmstil — dieselbe Machart wie `seiten._knopf`.

    Bewusst hier nachgebaut statt importiert: `seiten` importiert aus diesem
    Modul, andersherum gäbe es einen Ringschluss.
    """
    hoehe = schrift.metrics('linespace') + 16
    breite = schrift.measure(text) + 40
    farbe = ACCENT if stark else FG
    rand = ACCENT if stark else LINIE
    c = tk.Canvas(eltern, width=breite, height=hoehe, bg=BG,
                  highlightthickness=0, bd=0, cursor='hand2')
    flaeche = _rundes_rechteck(c, 1, 1, breite - 1, hoehe - 1, radius=5,
                               fill='#1d2a14' if stark else FLAECHE,
                               outline=rand, width=1)
    beschriftung = c.create_text(breite / 2.0, hoehe / 2.0, text=text,
                                 fill=farbe, font=schrift, anchor='center')
    c.bind('<Enter>', lambda _=None: (c.itemconfigure(flaeche, outline=ACCENT),
                                      c.itemconfigure(beschriftung, fill=ACCENT)))
    c.bind('<Leave>', lambda _=None: (c.itemconfigure(flaeche, outline=rand),
                                      c.itemconfigure(beschriftung, fill=farbe)))
    c.bind('<Button-1>', lambda _=None: tat())
    return c


def wahl_stellen(eltern, titel, text, knopf_a, knopf_b):
    """Zwei Wege zur Auswahl stellen. Gibt `'a'`, `'b'` oder `''` zurueck.

    ⚠ **Warum nicht `frage_stellen`.** Dort bedeutet Escape „nein" — bei einer
    Ja/Nein-Frage ist das richtig. Stehen aber zwei gleichwertige Handlungen zur
    Wahl, waere „nein" die zweite davon: Wer den Dialog wegklickt, haette
    ungewollt eine Sicherung eingespielt. Hier bricht Escape deshalb ab, ohne
    etwas zu tun, und beide Knoepfe muessen bewusst getroffen werden.
    """
    antwort = {'wert': ''}
    try:
        top = tk.Toplevel(eltern)
        top.title(titel)
        top.configure(bg=BG)
        top.resizable(False, False)
        top.transient(eltern)
        top.configure(highlightthickness=1, highlightbackground=LINIE,
                      highlightcolor=LINIE)

        schrift_titel = tkfont.Font(family='Segoe UI', size=12, weight='bold')
        schrift_text = tkfont.Font(family='Segoe UI', size=10)
        schrift_knopf = tkfont.Font(family='Segoe UI', size=9)

        rahmen = tk.Frame(top, bg=BG, padx=26, pady=22)
        rahmen.pack(fill='both', expand=True)
        tk.Label(rahmen, text=titel, bg=BG, fg=ACCENT, font=schrift_titel,
                 anchor='w', justify='left').pack(fill='x')
        tk.Label(rahmen, text=text, bg=BG, fg=FG, font=schrift_text,
                 anchor='w', justify='left',
                 wraplength=FRAGE_BREITE - 52).pack(fill='x', pady=(10, 0))

        def schliessen(wert):
            antwort['wert'] = wert
            try:
                top.grab_release()
            except tk.TclError:
                pass
            top.destroy()

        reihe = tk.Frame(rahmen, bg=BG)
        reihe.pack(anchor='e', pady=(20, 0))
        _dialog_knopf(reihe, knopf_b, lambda: schliessen('b'),
                      schrift_knopf).pack(side='right', padx=(8, 0))
        _dialog_knopf(reihe, knopf_a, lambda: schliessen('a'),
                      schrift_knopf, stark=True).pack(side='right')

        top.bind('<Escape>', lambda _=None: schliessen(''))
        top.protocol('WM_DELETE_WINDOW', lambda: schliessen(''))

        top.update_idletasks()
        breite = max(FRAGE_BREITE, top.winfo_reqwidth())
        hoehe = top.winfo_reqheight()
        try:
            x = eltern.winfo_rootx() + (eltern.winfo_width() - breite) // 2
            y = eltern.winfo_rooty() + (eltern.winfo_height() - hoehe) // 3
        except tk.TclError:
            x = y = 200
        top.geometry('%dx%d+%d+%d' % (breite, hoehe, max(0, x), max(0, y)))

        top.grab_set()
        top.focus_set()
        eltern.wait_window(top)
    except Exception as ausnahme:
        fehler.merken('hauptfenster.wahl_stellen', ausnahme)
    return antwort['wert']


def text_stellen(eltern, titel, text, vorgabe='', ja=None, nein=None,
                 liste=(), listentitel=''):
    """Nach einem kurzen Text fragen — im Programmstil. Gibt den Text oder `None`.

    `None` heisst **abgebrochen**, `''` heisst „nichts eingetippt". Der
    Unterschied zaehlt: Beim Abbruch soll gar nichts passieren, bei einer
    leeren Eingabe eine Meldung kommen.

    `liste` sind vorhandene Eintraege, die zur Auswahl stehen — **untereinander**,
    nicht als Aufzaehlung im Fliesstext.

    ⚠ Warum untereinander: In den Fliesstext gequetscht laufen sechs Namen
    rechts aus dem Fenster, und abgeschnitten ist ein Name unbrauchbar — man
    kann ihn weder lesen noch abtippen. Untereinander ist jeder ganz da, die
    Liste bleibt ueberschaubar, und ein Klick uebernimmt den Namen ins Feld
    (dann ersetzt man ein Profil, statt sich am Namen zu vertippen).

    ⚠⚠ **Ersatz fuer `simpledialog.askstring`, und zwar aus gutem Grund.** Der
    Systemdialog kommt grau, in der Systemschrift und mit einem englischen
    „Cancel" daher — auf dem dunklen Grund des Programms ein Fremdkoerper, und
    ein Regelverstoss gleich doppelt: Jeder sichtbare Text gehoert nach
    `sprache.py`, und gleiche Dinge sehen ueberall gleich aus.

    Aufgebaut wie `frage_stellen()` — dieselben Farben, dieselbe Kante,
    dieselben Tasten (Eingabe = uebernehmen, Escape = abbrechen).
    """
    try:
        ja = ja or t('e_speichern')
        nein = nein or t('e_abbrechen')
        top = tk.Toplevel(eltern)
        top.title(titel)
        top.configure(bg=BG)
        top.resizable(False, False)
        top.transient(eltern)
        top.configure(highlightthickness=1, highlightbackground=LINIE,
                      highlightcolor=LINIE)

        schrift_titel = tkfont.Font(family='Segoe UI', size=12, weight='bold')
        schrift_text = tkfont.Font(family='Segoe UI', size=10)
        schrift_knopf = tkfont.Font(family='Segoe UI', size=9)
        schrift_klein = tkfont.Font(family='Segoe UI', size=9)
        # ⚠ Feste Breite fuer das Eingabefeld: Ein Profilname ist kurz, aber
        # der Hinweistext darueber ist breit. Ohne eigene Schrift erbt das
        # Feld die des Systems und faellt aus dem Bild.
        schrift_feld = tkfont.Font(family='Segoe UI', size=11)

        rahmen = tk.Frame(top, bg=BG, padx=26, pady=22)
        rahmen.pack(fill='both', expand=True)
        tk.Label(rahmen, text=titel, bg=BG, fg=ACCENT, font=schrift_titel,
                 anchor='w', justify='left').pack(fill='x')
        tk.Label(rahmen, text=text, bg=BG, fg=FG, font=schrift_text,
                 anchor='w', justify='left',
                 wraplength=FRAGE_BREITE - 52).pack(fill='x', pady=(10, 0))

        wert = tk.StringVar(value=vorgabe)

        if liste:
            if listentitel:
                tk.Label(rahmen, text=listentitel, bg=BG, fg=SUB,
                         font=schrift_klein, anchor='w').pack(
                             fill='x', pady=(14, 4))
            kasten = tk.Frame(rahmen, bg=FLAECHE, highlightthickness=1,
                              highlightbackground=LINIE)
            kasten.pack(fill='x')
            # ⚠ Ab sieben Eintraegen rollt der Kasten, statt den Dialog ueber
            # den Bildschirmrand wachsen zu lassen. Wer zwanzig Profile hat,
            # soll trotzdem an die Knoepfe kommen.
            for eintrag in liste[:LISTE_SICHTBAR]:
                zeile = tk.Label(kasten, text='  ' + eintrag, bg=FLAECHE,
                                 fg=FG, font=schrift_text, anchor='w',
                                 cursor='hand2')
                zeile.pack(fill='x', pady=1)

                def uebernehmen(_=None, name=eintrag):
                    wert.set(name)
                zeile.bind('<Button-1>', uebernehmen)
                # Ein Klickziel muss sich als solches zeigen — sonst probiert
                # es niemand aus.
                zeile.bind('<Enter>',
                           lambda _=None, w=zeile: w.configure(fg=ACCENT))
                zeile.bind('<Leave>',
                           lambda _=None, w=zeile: w.configure(fg=FG))
            wenn_mehr = len(liste) - LISTE_SICHTBAR
            if wenn_mehr > 0:
                tk.Label(kasten, text=t('e_liste_mehr', wenn_mehr), bg=FLAECHE,
                         fg=SUB, font=schrift_klein, anchor='w').pack(
                             fill='x', pady=(2, 3))

        feld = tk.Entry(rahmen, textvariable=wert, font=schrift_feld,
                        bg=FLAECHE, fg=FG, insertbackground=FG,
                        relief='flat', highlightthickness=1,
                        highlightbackground=LINIE, highlightcolor=ACCENT)
        feld.pack(fill='x', pady=(14, 0), ipady=5)

        antwort = {'wert': None}

        def schliessen(uebernehmen):
            antwort['wert'] = wert.get().strip() if uebernehmen else None
            try:
                top.grab_release()
            except tk.TclError:
                pass
            top.destroy()

        reihe = tk.Frame(rahmen, bg=BG)
        reihe.pack(anchor='e', pady=(20, 0))
        _dialog_knopf(reihe, nein, lambda: schliessen(False),
                      schrift_knopf).pack(side='right', padx=(8, 0))
        _dialog_knopf(reihe, ja, lambda: schliessen(True),
                      schrift_knopf, stark=True).pack(side='right')

        top.bind('<Return>', lambda _=None: schliessen(True))
        top.bind('<Escape>', lambda _=None: schliessen(False))
        top.protocol('WM_DELETE_WINDOW', lambda: schliessen(False))

        top.update_idletasks()
        breite = max(FRAGE_BREITE, top.winfo_reqwidth())
        hoehe = top.winfo_reqheight()
        try:
            x = eltern.winfo_rootx() + (eltern.winfo_width() - breite) // 2
            y = eltern.winfo_rooty() + (eltern.winfo_height() - hoehe) // 3
        except tk.TclError:
            x = y = 200
        top.geometry('%dx%d+%d+%d' % (breite, hoehe, max(0, x), max(0, y)))

        top.grab_set()
        # Der Mauszeiger steht schon im Feld — wer tippen soll, soll tippen
        # koennen, ohne vorher zu klicken.
        feld.focus_set()
        feld.selection_range(0, 'end')
        eltern.wait_window(top)
        return antwort['wert']
    except Exception as ausnahme:
        fehler.merken('hauptfenster.text_stellen', ausnahme)
        from tkinter import simpledialog
        return simpledialog.askstring(titel, text, initialvalue=vorgabe)


def auswahl_stellen(eltern, titel, text, eintraege, nein=None):
    """Einen Eintrag aus einer Liste waehlen. Gibt den Eintrag oder `None`.

    ⚠ **Wozu, wenn es doch einen Dateiwaehler gibt:** Weil der Spieler seine
    Profile am **Namen** kennt, nicht am Pfad. Wer „Virpil_Kampf" einspielen
    will, soll ihn anklicken — und nicht erst durch `USER/client/0/controls/
    mappings/` navigieren, einen Ordner, den er nie selbst angelegt hat und
    der auf jedem Rechner woanders liegt.

    Der Dateiwaehler bleibt daneben bestehen: Fuer eine Datei, die jemand
    zugeschickt bekommen hat, ist er der richtige Weg.
    """
    try:
        nein = nein or t('e_abbrechen')
        top = tk.Toplevel(eltern)
        top.title(titel)
        top.configure(bg=BG)
        top.resizable(False, False)
        top.transient(eltern)
        top.configure(highlightthickness=1, highlightbackground=LINIE,
                      highlightcolor=LINIE)

        schrift_titel = tkfont.Font(family='Segoe UI', size=12, weight='bold')
        schrift_text = tkfont.Font(family='Segoe UI', size=10)
        schrift_knopf = tkfont.Font(family='Segoe UI', size=9)
        schrift_klein = tkfont.Font(family='Segoe UI', size=9)

        rahmen = tk.Frame(top, bg=BG, padx=26, pady=22)
        rahmen.pack(fill='both', expand=True)
        tk.Label(rahmen, text=titel, bg=BG, fg=ACCENT, font=schrift_titel,
                 anchor='w', justify='left').pack(fill='x')
        if text:
            tk.Label(rahmen, text=text, bg=BG, fg=FG, font=schrift_text,
                     anchor='w', justify='left',
                     wraplength=FRAGE_BREITE - 52).pack(fill='x', pady=(10, 0))

        antwort = {'wert': None}

        def schliessen(wert):
            antwort['wert'] = wert
            try:
                top.grab_release()
            except tk.TclError:
                pass
            top.destroy()

        kasten = tk.Frame(rahmen, bg=FLAECHE, highlightthickness=1,
                          highlightbackground=LINIE)
        kasten.pack(fill='x', pady=(14, 0))
        for eintrag in list(eintraege)[:LISTE_SICHTBAR]:
            zeile = tk.Label(kasten, text='  ' + eintrag, bg=FLAECHE, fg=FG,
                             font=schrift_text, anchor='w', cursor='hand2')
            zeile.pack(fill='x', pady=2)
            zeile.bind('<Button-1>',
                       lambda _=None, n=eintrag: schliessen(n))
            zeile.bind('<Enter>',
                       lambda _=None, w=zeile: w.configure(fg=ACCENT))
            zeile.bind('<Leave>', lambda _=None, w=zeile: w.configure(fg=FG))
        mehr = len(list(eintraege)) - LISTE_SICHTBAR
        if mehr > 0:
            tk.Label(kasten, text=t('e_liste_mehr', mehr), bg=FLAECHE, fg=SUB,
                     font=schrift_klein, anchor='w').pack(fill='x', pady=(2, 3))

        reihe = tk.Frame(rahmen, bg=BG)
        reihe.pack(anchor='e', pady=(20, 0))
        _dialog_knopf(reihe, nein, lambda: schliessen(None),
                      schrift_knopf).pack(side='right')

        top.bind('<Escape>', lambda _=None: schliessen(None))
        top.protocol('WM_DELETE_WINDOW', lambda: schliessen(None))

        top.update_idletasks()
        breite = max(FRAGE_BREITE, top.winfo_reqwidth())
        hoehe = top.winfo_reqheight()
        try:
            x = eltern.winfo_rootx() + (eltern.winfo_width() - breite) // 2
            y = eltern.winfo_rooty() + (eltern.winfo_height() - hoehe) // 3
        except tk.TclError:
            x = y = 200
        top.geometry('%dx%d+%d+%d' % (breite, hoehe, max(0, x), max(0, y)))

        top.grab_set()
        top.focus_set()
        eltern.wait_window(top)
        return antwort['wert']
    except Exception as ausnahme:
        fehler.merken('hauptfenster.auswahl_stellen', ausnahme)
        return None


def frage_stellen(eltern, titel, text, ja=None, nein=None,
                  nur_ok=False):
    """Ja/Nein-Frage im Programmstil. Gibt True zurück, wenn bejaht wurde.

    Ersatz für `messagebox.askyesno`. Modal, mittig über dem Elternfenster,
    Eingabetaste = ja, Escape = nein.

    Fällt bei einem Fehler auf den System-Dialog zurück: Eine Frage, die sich
    nicht stellen lässt, wäre schlimmer als eine hässliche.
    """
    try:
        ja = ja or t('e_ja')
        nein = nein or t('e_nein')
        top = tk.Toplevel(eltern)
        top.title(titel)
        top.configure(bg=BG)
        top.resizable(False, False)
        top.transient(eltern)
        # ⚠ Eigene Kante. Ohne sie ist der Dialog eine dunkle Flaeche auf
        # dunklem Grund — jeder andere Kasten im Programm (Zustandskasten,
        # Karten) hat eine sichtbare Linie, und ohne sie wirkt er nicht
        # dazugehoerig. Der Fensterrahmen des Systems ersetzt das nicht: Er
        # sieht auf jedem Schreibtisch anders aus, innen bleibt es randlos.
        top.configure(highlightthickness=1, highlightbackground=LINIE,
                      highlightcolor=LINIE)

        schrift_titel = tkfont.Font(family='Segoe UI', size=12, weight='bold')
        schrift_text = tkfont.Font(family='Segoe UI', size=10)
        schrift_knopf = tkfont.Font(family='Segoe UI', size=9)

        rahmen = tk.Frame(top, bg=BG, padx=26, pady=22)
        rahmen.pack(fill='both', expand=True)
        tk.Label(rahmen, text=titel, bg=BG, fg=ACCENT, font=schrift_titel,
                 anchor='w', justify='left').pack(fill='x')
        tk.Label(rahmen, text=text, bg=BG, fg=FG, font=schrift_text,
                 anchor='w', justify='left',
                 wraplength=FRAGE_BREITE - 52).pack(fill='x', pady=(10, 0))

        antwort = {'wert': False}

        def schliessen(wert):
            antwort['wert'] = wert
            try:
                top.grab_release()
            except tk.TclError:
                pass
            top.destroy()

        reihe = tk.Frame(rahmen, bg=BG)
        reihe.pack(anchor='e', pady=(20, 0))
        # ⚠ Beim blossen Bescheid gibt es nichts zu entscheiden — dann waere
        # ein zweiter Knopf eine Frage, die keine ist.
        if not nur_ok:
            _dialog_knopf(reihe, nein, lambda: schliessen(False),
                          schrift_knopf).pack(side='right', padx=(8, 0))
        _dialog_knopf(reihe, ja, lambda: schliessen(True),
                      schrift_knopf, stark=True).pack(side='right')

        top.bind('<Return>', lambda _=None: schliessen(True))
        top.bind('<Escape>', lambda _=None: schliessen(False))
        top.protocol('WM_DELETE_WINDOW', lambda: schliessen(False))

        # Mittig über das Elternfenster setzen — erst messen, dann schieben.
        top.update_idletasks()
        breite = max(FRAGE_BREITE, top.winfo_reqwidth())
        hoehe = top.winfo_reqheight()
        try:
            x = eltern.winfo_rootx() + (eltern.winfo_width() - breite) // 2
            y = eltern.winfo_rooty() + (eltern.winfo_height() - hoehe) // 3
        except tk.TclError:
            x = y = 200
        top.geometry('%dx%d+%d+%d' % (breite, hoehe, max(0, x), max(0, y)))

        top.grab_set()
        top.focus_set()
        eltern.wait_window(top)
        return antwort['wert']
    except Exception as ausnahme:
        fehler.merken('hauptfenster.frage_stellen', ausnahme)
        from tkinter import messagebox
        if nur_ok:
            messagebox.showinfo(titel, text, parent=eltern)
            return True
        return bool(messagebox.askyesno(titel, text, parent=eltern))


def kanal_waehlen(eltern, eingetragen, kanaele):
    """Fragen, aus welchem Spielordner ab jetzt gelesen wird. Gibt ihn zurueck.

    ⚠⚠ **Wofuer das da ist.** Legt CIG eine ausgebesserte Fassung neben LIVE,
    laedt kaum jemand 100 GB neu — man benennt den LIVE-Ordner in HOTFIX um,
    damit der Launcher nur die Unterschiede holt. Der eingetragene Ordner ist
    damit weg, und der Watcher las entweder stillschweigend woanders oder meldete
    „Star Citizen nicht gefunden", obwohl in den Einstellungen ein Pfad steht.
    Beides sieht nach einem kaputten Programm aus. Gemeldet von Haldjas am
    03.09.2026, dem genau das passiert war.

    ⚠ **Gefragt wird, nicht stillschweigend umgestellt.** Welcher Kanal gemeint
    ist, weiss nur der Spieler — wer PTU testet und daneben LIVE liegen hat,
    haette sonst ploetzlich die falschen Baupläne gezaehlt.

    `kanaele` sind Tupel `(Name, Ordner, Zeitstempel)` aus
    `pfade.kanaele_vorhanden()`, neueste zuerst. Rueckgabe: der gewaehlte Ordner
    oder None, wenn der Spieler es beim Alten lassen will.
    """
    if not kanaele:
        return None

    # Der Normalfall ist genau ein Nachbarkanal (LIVE wurde zu HOTFIX). Dafuer
    # braucht es keine Liste — eine Frage mit Ja und Nein ist kuerzer und
    # beantwortet sich schneller.
    if len(kanaele) == 1:
        name, ordner, _stempel = kanaele[0]
        text = '%s\n%s\n\n%s' % (t('s_kn_weg') % os.path.basename(eingetragen),
                                 t('s_kn_da') % name,
                                 t('s_kn_frage'))
        if frage_stellen(eltern, t('s_kn_titel'), text,
                         nein=t('s_kn_spaeter')):
            return ordner
        return None

    try:
        top = tk.Toplevel(eltern)
        top.title(t('s_kn_titel'))
        top.configure(bg=BG, highlightthickness=1, highlightbackground=LINIE,
                      highlightcolor=LINIE)
        top.resizable(False, False)
        top.transient(eltern)

        schrift_titel = tkfont.Font(family='Segoe UI', size=12, weight='bold')
        schrift_text = tkfont.Font(family='Segoe UI', size=10)
        schrift_knopf = tkfont.Font(family='Segoe UI', size=9)

        rahmen = tk.Frame(top, bg=BG, padx=26, pady=22)
        rahmen.pack(fill='both', expand=True)
        tk.Label(rahmen, text=t('s_kn_titel'), bg=BG, fg=ACCENT,
                 font=schrift_titel, anchor='w', justify='left').pack(fill='x')
        tk.Label(rahmen,
                 text='%s\n\n%s' % (t('s_kn_weg') % os.path.basename(eingetragen),
                                    t('s_kn_mehrere')),
                 bg=BG, fg=FG, font=schrift_text, anchor='w', justify='left',
                 wraplength=FRAGE_BREITE - 52).pack(fill='x', pady=(10, 0))

        gewaehlt = {'wert': None}

        def schliessen(wert):
            gewaehlt['wert'] = wert
            try:
                top.grab_release()
            except tk.TclError:
                pass
            top.destroy()

        for name, ordner, stempel in kanaele:
            wann = time.strftime('%d.%m.%Y %H:%M', time.localtime(stempel))
            kasten = tk.Frame(rahmen, bg=FLAECHE, cursor='hand2',
                              highlightthickness=1, highlightbackground=LINIE)
            kasten.pack(fill='x', pady=(10, 0))
            tk.Label(kasten, text=name, bg=FLAECHE, fg=FG, font=schrift_text,
                     anchor='w', padx=12, pady=(8)).pack(fill='x')
            tk.Label(kasten, text=t('s_kn_zuletzt') % wann, bg=FLAECHE, fg=SUB,
                     font=schrift_knopf, anchor='w',
                     padx=12).pack(fill='x', pady=(0, 8))
            # Auch auf den Beschriftungen — ein Klick daneben darf nicht ins
            # Leere gehen, sonst haelt man den Kasten fuer nicht anklickbar.
            for teil in (kasten,) + tuple(kasten.winfo_children()):
                teil.bind('<Button-1>',
                          lambda _e, o=ordner: schliessen(o))

        reihe = tk.Frame(rahmen, bg=BG)
        reihe.pack(anchor='e', pady=(20, 0))
        _dialog_knopf(reihe, t('s_kn_spaeter'), lambda: schliessen(None),
                      schrift_knopf).pack(side='right')

        top.bind('<Escape>', lambda _=None: schliessen(None))
        top.protocol('WM_DELETE_WINDOW', lambda: schliessen(None))

        top.update_idletasks()
        breite = max(FRAGE_BREITE, top.winfo_reqwidth())
        hoehe = top.winfo_reqheight()
        try:
            x = eltern.winfo_rootx() + (eltern.winfo_width() - breite) // 2
            y = eltern.winfo_rooty() + (eltern.winfo_height() - hoehe) // 3
        except tk.TclError:
            x = y = 200
        top.geometry('%dx%d+%d+%d' % (breite, hoehe, max(0, x), max(0, y)))

        top.grab_set()
        top.focus_set()
        eltern.wait_window(top)
        return gewaehlt['wert']
    except Exception as ausnahme:
        fehler.merken('hauptfenster.kanal_waehlen', ausnahme)
        # Lieber die kurze Frage auf den neuesten Kanal als gar keine.
        name, ordner, _s = kanaele[0]
        if frage_stellen(eltern, t('s_kn_titel'),
                         '%s\n%s\n\n%s' % (
                             t('s_kn_weg') % os.path.basename(eingetragen),
                             t('s_kn_da') % name, t('s_kn_frage')),
                         nein=t('s_kn_spaeter')):
            return ordner
        return None


def bescheid_geben(eltern, titel, text):
    """Ein Ergebnis zeigen, das nicht uebersehen werden darf.

    ⚠⚠ **Die Fusszeile reicht dafuer nicht.** Sie zeigt vier Sekunden lang
    eine Zeile und ist dann wieder leer — wer in der Zeit woanders hinsieht,
    erfaehrt das Ergebnis nie. Bei einem Lauf ueber hunderte Protokolle sieht
    man aber genau dorthin nicht: Man hat den Knopf gedrueckt und wartet.
    Am 31.08.2026 gemeldet: „in der Leiste steht es zu kurz oder gar nicht."

    ⚠ Ein Fenster nur fuer ein ERGEBNIS, nicht fuer jede Meldung. Ein Werkzeug,
    das staendig Fenster aufreisst, wird weggeklickt, ohne gelesen zu werden.
    """
    frage_stellen(eltern, titel, text, ja=t('e_ok'), nur_ok=True)
