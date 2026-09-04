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
Was in den einzelnen Reitern des Hauptfensters steht.

Getrennt von `hauptfenster.py`, weil das zwei verschiedene Fragen sind: Dort
geht es um den **Rahmen** (Reiterleiste, Umschalten, Größe), hier um den
**Inhalt**. So bleibt jede Datei überschaubar, und eine neue Seite ist eine
Funktion, kein Eingriff in den Rahmen.

Die großen Seiten leihen sich die vorhandenen Fenster: `bestandsfenster` und
`einstellungsfenster` können seit v3.0.0 auch in einen übergebenen Rahmen
zeichnen, statt ein eigenes Fenster aufzumachen.
"""
import os
import sys
import time
import tkinter as tk

from . import bericht, bestand as bestand_datei, fehler, katalog as katalog_modul
from . import pfade, zeichen
from .sprache import t

BG      = '#10141c'
FLAECHE = '#161c28'
BAR     = '#1b2230'
FG      = '#e6edf3'
SUB     = '#8b98a5'
ACCENT  = '#9ce430'
LINIE   = '#232c3d'
GOLD    = '#e8c353'
ROT     = '#e05252'


def _bauer_tabelle():
    """Welche Kennung von welcher Funktion gebaut wird.

    ⚠ Als eigene Funktion, damit das Hauptfenster die Kennungen abfragen kann,
    ohne eine Seite zu bauen — nötig fürs Vorbauen im Leerlauf
    (`_seiten_vorbauen`). Die Namen stehen erst hier unten im Modul zur
    Verfügung, deshalb eine Funktion und keine Konstante ganz oben.
    """
    return {
        'liste':       _liste,
        'fortschritt': _fortschritt,
        'auftragslog': _auftragslog,
        'allgemein':   _allgemein,
        'anzeige':     _anzeige,
        'ordner':      _ordner,
        'spiel':       _spiel,
        'bestand':     _bestand,
        'wasistneu':   _wasistneu,
        'ueber':       _ueber,
        'serverstatus': _serverstatus,
        'danke':       _danke,
        'erkennung':   _erkennung,
        'joysticks':   _joysticks,
        'diagnose':    _diagnose,
        'herstellung': _herstellung,
        'bergbau':     _bergbau,
        'lager':       _lager,
        'verkauf':     _verkauf,
        'handelslager': _handelslager,
    }


def kennungen():
    """Alle Seiten-Kennungen — für das Vorbauen im Leerlauf."""
    return tuple(_bauer_tabelle())


def bauen(fenster, kennung, rahmen):
    """Eine Seite füllen. `fenster` ist das Hauptfenster (Schriften, Meldungen)."""
    bauer = _bauer_tabelle().get(kennung)
    if bauer:
        bauer(fenster, rahmen)


# ------------------------------------------------------------------ Bausteine
def _ueberschrift(fenster, rahmen, titel, lead=''):
    tk.Label(rahmen, text=titel, bg=BG, fg=FG, font=fenster.f_titel,
             anchor='w').pack(fill='x', padx=24, pady=(20, 2))
    if lead:
        # `abzug=48` sind die beiden Ränder von je 24 — ohne sie rechnet der
        # Umbruch mit Platz, den es nicht gibt, und die letzten Wörter fallen
        # trotzdem heraus.
        einleitung = tk.Label(rahmen, text=lead, bg=BG, fg=SUB,
                              font=fenster.f_klein, anchor='w', justify='left')
        einleitung.pack(fill='x', padx=24, pady=(0, 14))
        _umbruch(einleitung, abzug=48)


def _rollflaeche(rahmen, rand=24):
    """Ein Bereich, der rollt. Leinwand + Balken, wie im Einstellungsfenster.

    ⚠ Die Fläche wird **zuletzt** gepackt und bekommt `expand=True` — alles,
    was fest bleiben soll, muss vorher gepackt sein.

    `rand` rückt den Inhalt ein. Das gehört hierher und nicht in jeden Baustein:
    Die Bausteine stammen aus dem alten Einstellungsfenster und packen sich ohne
    Rand — neben eingerückten Überschriften sahen sie aus, als wären sie
    verrutscht.
    """
    aussen = tk.Frame(rahmen, bg=BG)
    aussen.pack(fill='both', expand=True)
    leinwand = tk.Canvas(aussen, bg=BG, highlightthickness=0)
    from .hauptfenster import rundleiste
    balken = rundleiste(aussen, leinwand, grund=BG)
    innen = tk.Frame(leinwand, bg=BG)
    innen.bind('<Configure>',
               lambda e: leinwand.configure(scrollregion=leinwand.bbox('all')))
    fenster_id = leinwand.create_window((0, 0), window=innen, anchor='nw')
    leinwand.bind('<Configure>',
                  lambda e: leinwand.itemconfigure(fenster_id, width=e.width))
    leinwand.configure(yscrollcommand=balken.set)
    balken.pack(side='right', fill='y')
    leinwand.pack(side='left', fill='both', expand=True)

    if rand:
        polster = tk.Frame(innen, bg=BG)
        polster.pack(fill='both', expand=True, padx=rand)
        innen_ziel = polster
    else:
        innen_ziel = innen

    from .hauptfenster import rad_anschliessen
    rad_anschliessen(leinwand)
    # ⭐ Die Leinwand mitgeben. Seiten, die ihre Liste neu zeichnen (Lager,
    # Handelslager), brauchen sie, um die Rollposition zu halten — siehe
    # `_rollstelle_halten`.
    innen_ziel.leinwand = leinwand
    return innen_ziel


def _rollstelle_halten(widget, tat):
    """Etwas neu zeichnen, ohne dass die Seite nach oben springt.

    ⚠⚠ **Wer eine Liste neu aufbaut, verliert die Rollposition.** Beim Löschen
    eines Postens wird die ganze Tabelle verworfen und neu gezeichnet; die
    Leinwand steht danach wieder bei null, und wer unten am zwölften Eintrag
    war, landet oben. Am 30.08.2026 gemeldet: „beim Löschen von Einträgen
    springt das Fenster immer wieder nach ganz oben."

    Die Stelle wird **vorher** gelesen und **nach** dem Neuzeichnen gesetzt —
    dazwischen ändert sich die Höhe des Inhalts, deshalb erst nach einem
    Leerlauf, wenn Tk den neuen Rollbereich kennt.
    """
    leinwand = None
    lauf = widget
    while lauf is not None and leinwand is None:
        leinwand = getattr(lauf, 'leinwand', None)
        lauf = getattr(lauf, 'master', None)
    if leinwand is None:
        tat()
        return
    try:
        stelle = leinwand.yview()[0]
    except Exception:
        stelle = None
    tat()
    if stelle is None:
        return

    def zurueck():
        try:
            if leinwand.winfo_exists():
                leinwand.yview_moveto(stelle)
        except Exception:
            pass

    try:
        leinwand.after_idle(zurueck)
    except Exception:
        pass


def _suche_leeren_kreuz(fenster, halter, var):
    """Ein × neben dem Suchfeld, das den Text wegnimmt.

    ⚠ Es erscheint nur, wenn wirklich etwas im Feld steht. Ein Kreuz an einem
    leeren Feld sieht aus wie ein Knopf, der nichts tut.

    Warum es das braucht: Wer „titan" gesucht hat und danach die ganze Liste
    sehen will, musste den Text von Hand markieren und löschen — und wer den
    Suchbegriff übersieht, hält die kurze Liste für den ganzen Bestand.
    """
    from . import hinweis
    kreuz = tk.Label(halter, text='\u00d7', bg=BG, fg=SUB,
                     font=fenster.f_grund, cursor='hand2')
    hinweis.anhaengen(kreuz, lambda: t('s_suche_leeren'))
    kreuz.bind('<Button-1>', lambda _e: var.set(''))
    kreuz.bind('<Enter>', lambda _e: kreuz.configure(fg=ACCENT))
    kreuz.bind('<Leave>', lambda _e: kreuz.configure(fg=SUB))

    def nachziehen(*_):
        if var.get().strip():
            kreuz.pack(side='right', padx=(6, 2))
        else:
            kreuz.pack_forget()

    var.trace_add('write', nachziehen)
    nachziehen()
    return kreuz


def _filterleiste(fenster, eltern, felder, beim_wechsel, zustand):
    """Eine Reihe Auswahlfelder plus „Auswahl zurücksetzen" — für jede Seite gleich.

    ⚠⚠ **Ein Bedienkonzept für das ganze Programm.** Xharig am 29.08.2026:
    *„egal wo, sollte das Bedienkonzept nicht jedes Mal ändern — die Leute
    wollen es nutzen und nicht erst lernen, wie sie es nutzen."* Wer die
    Bauplan-Liste bedienen kann, muss Herstellung und Bergbau ohne Umlernen
    bedienen können. Deshalb dasselbe `rundwahl` wie dort, derselbe
    Zurücksetzen-Knopf an derselben Stelle.

    `felder` ist eine Liste aus `(schluessel, beschriftung, eintraege)`:
    `eintraege` sind Paare `(wert, text)`; ein leerer Wert ist der „alle"-Fall.
    **Ein Feld ohne echte Auswahl wird weggelassen** — ein Auswahlfeld, das nur
    „alle" anbietet, ist Ballast und lässt einen suchen, was es filtern soll.

    `zustand` ist ein Wörterbuch, in dem die Wahl landet; `beim_wechsel` wird
    nach jeder Änderung gerufen.

    Gibt eine Funktion zurück, die alles zurücksetzt.
    """
    from .hauptfenster import rundwahl
    reihe = tk.Frame(eltern, bg=BG)
    reihe.pack(fill='x', pady=(0, 8))
    links = tk.Frame(reihe, bg=BG)
    links.pack(side='left', fill='x', expand=True)

    gebaut = {}
    for schluessel, beschriftung, eintraege in felder:
        if len(eintraege) <= 1:
            continue
        w = rundwahl(links, [('', beschriftung)] + list(eintraege),
                     zustand.get(schluessel, ''),
                     lambda wert, s=schluessel: (zustand.__setitem__(s, wert),
                                                 beim_wechsel()),
                     fenster.f_klein)
        w.pack(side='left', padx=(0, 8), pady=2)
        gebaut[schluessel] = w

    def zuruecksetzen():
        for schluessel, w in gebaut.items():
            zustand[schluessel] = ''
            try:
                w.stumm_setzen('')
            except Exception:
                pass
        beim_wechsel()

    return zuruecksetzen, reihe, gebaut


def _mass_sichern(c, beschriftung, flaeche, hoehe, fuellung, rand):
    """Sorgt dafür, dass eine Knopf-Leinwand ihren Text wirklich fasst.

    ⚠ **Einmal beim Bauen zu messen reicht nicht.** `schrift.measure()` sagt,
    wie breit Tk den Text glaubt; gezeichnet wird er mit der Schrift, die das
    System hergibt — und unter Wayland steht die erst fest, wenn das Fenster
    angezeigt wird. Auf einem anderen Rechner stand deshalb „erung speichern" auf
    einem Knopf, während derselbe Knopf hier sauber aussah.

    Deshalb wird dreimal nachgesehen: sofort, beim ersten `<Configure>` und
    einmal im Leerlauf. Vergrössert wird nur, wenn es nötig ist — dadurch kommt
    es zur Ruhe, statt sich gegenseitig neu auszulösen.

    `flaeche` ist eine **Liste** mit der Kennung des Rahmens. Wächst die
    Leinwand, wird der Rahmen neu gezeichnet, sonst endet er mitten im Wort;
    die Liste hält die neue Kennung fest, damit die Farbwechsel weiter greifen.
    """
    from .hauptfenster import _rundes_rechteck

    def nachmessen(_=None):
        try:
            kasten = c.bbox(beschriftung)
        except tk.TclError:
            return
        if not kasten:
            return
        noetig = (kasten[2] - kasten[0]) + 30
        if noetig <= int(c['width']):
            return
        c.configure(width=noetig)
        c.coords(beschriftung, noetig / 2.0, hoehe / 2.0)
        c.delete(flaeche[0])
        flaeche[0] = _rundes_rechteck(c, 1, 1, noetig - 1, hoehe - 1, radius=5,
                                      fill=fuellung, outline=rand, width=1)
        c.tag_lower(flaeche[0], beschriftung)

    nachmessen()
    c.bind('<Configure>', nachmessen, add='+')
    c.after_idle(nachmessen)
    return nachmessen


def _knopf(fenster, eltern, text, tat, stark=False, gefahr=False):
    """Ein Knopf im Stil der Vorschau — Rand, Farbe beim Überfahren."""
    from .hauptfenster import _rundes_rechteck
    schrift = fenster.f_klein
    hoehe = schrift.metrics('linespace') + 16
    breite = schrift.measure(text) + 30
    # ⚠ `gefahr` faerbt **dauerhaft**, nicht erst beim Überfahren. Ein Knopf,
    # der erst rot wird, wenn die Maus schon darauf steht, warnt niemanden —
    # gesehen hat man ihn dann längst. am 28.08.2026 gemeldet zum
    # Absende-Knopf: „der Button wird erst beim Überfahren rot."
    farbe = ROT if gefahr else (ACCENT if stark else FG)
    rand = ROT if gefahr else (ACCENT if stark else LINIE)
    c = tk.Canvas(eltern, width=breite, height=hoehe, bg=BG,
                  highlightthickness=0, bd=0, cursor='hand2')
    # ⚠ Erst der Text, dann der Rahmen — und dazwischen wird **nachgemessen**.
    # `schrift.measure()` sagt, wie breit Tk den Text glaubt; gezeichnet wird
    # er mit der Schrift, die das System wirklich hergibt. Weichen die ab, ist
    # die Leinwand zu schmal und schneidet beidseitig ab: Am 29.08.2026 stand
    # auf einem Knopf „erung speichern" statt „Änderung speichern".
    # `bbox()` liefert die tatsaechliche Ausdehnung, ohne dass das Fenster
    # sichtbar sein muss.
    beschriftung = c.create_text(breite / 2.0, hoehe / 2.0, text=text,
                                 fill=farbe, font=schrift, anchor='center')
    fuellung = ('#2a1414' if gefahr
                else ('#1d2a14' if stark else FLAECHE))
    flaeche = [_rundes_rechteck(c, 1, 1, breite - 1, hoehe - 1, radius=5,
                                fill=fuellung, outline=rand, width=1)]
    c.tag_lower(flaeche[0], beschriftung)

    _mass_sichern(c, beschriftung, flaeche, hoehe, fuellung, rand)

    def rein(_=None):
        c.itemconfigure(flaeche[0], outline=ROT if gefahr else ACCENT)
        c.itemconfigure(beschriftung, fill=ROT if gefahr else ACCENT)

    def raus(_=None):
        c.itemconfigure(flaeche[0], outline=rand)
        # ⚠ `farben['ruhe']`, nicht `farbe`: Wurde der Knopf zwischendurch
        # umbeschriftet (Verkaufs-Reiter, Restzeit), holte die feste Farbe die
        # alte zurück, sobald die Maus den Knopf einmal verlassen hatte.
        c.itemconfigure(beschriftung, fill=farben['ruhe'])

    def mitwachsen(_=None):
        """Wird der Knopf gestreckt, muss das gezeichnete Rechteck mit.

        ⚠ Ein Canvas hat eine feste Wunschbreite. `pack(fill='x')` streckt zwar
        die Leinwand, aber das darauf gezeichnete Rechteck bleibt schmal — der
        Knopf sah dann aus, als füllte er nur die halbe Kastenbreite. Genau so
        am 26.08.2026 gemeldet: „der Button füllt nur die hälfte unter den
        versionen".

        Deshalb bei jeder Größenänderung Rechteck und Text neu setzen. Knöpfe,
        die nicht gestreckt werden, behalten ihre Breite von selbst.
        """
        neue_breite = c.winfo_width()
        if neue_breite <= 10 or abs(neue_breite - breite) < 2:
            return
        punkte = _rundes_rechteck(c, 1, 1, neue_breite - 1, hoehe - 1,
                                  radius=5, fill='', outline='')
        c.coords(flaeche, *c.coords(punkte))
        c.delete(punkte)
        c.coords(beschriftung, neue_breite / 2.0, hoehe / 2.0)

    def beschriften(neuer_text, neue_farbe=None):
        """Text und Farbe im laufenden Betrieb ändern.

        Gebraucht für den herunterzählenden Knopf im Verkaufs-Reiter: Solange
        die Stundensperre läuft, steht dort die Restzeit statt der Aufschrift.

        ⚠ **Die Breite bleibt, wie sie war.** Sie wurde beim Bauen aus dem
        längsten Text berechnet — ein kürzerer Text macht den Knopf also nicht
        schmaler, und beim Herunterzählen springt nichts. Ein *längerer* Text
        als der ursprüngliche würde abgeschnitten; wer das braucht, baut den
        Knopf mit dem längeren Text und setzt danach den kurzen.
        """
        c.itemconfigure(beschriftung, text=neuer_text,
                        fill=neue_farbe if neue_farbe else farbe)
        # Damit `raus()` nach dem Überfahren nicht die alte Farbe zurückholt.
        farben['ruhe'] = neue_farbe if neue_farbe else farbe

    farben = {'ruhe': farbe}

    c.bind('<Configure>', mitwachsen, add='+')
    c.bind('<Enter>', rein)
    c.bind('<Leave>', raus)
    c.bind('<Button-1>', lambda e: tat())
    c.beschriften = beschriften
    c.ist_knopf = True          # damit tools/randpruefung.py ihn prüft
    return c


def _wahl(fenster, eltern, eintraege, aktiv, tat):
    """Mehrere Möglichkeiten nebeneinander — die gewählte trägt den Akzentrand."""
    from .hauptfenster import _rundes_rechteck
    reihe = tk.Frame(eltern, bg=BG)
    knoepfe = {}
    schrift = fenster.f_klein
    for kennung, text in eintraege:
        an = (kennung == aktiv)
        hoehe = schrift.metrics('linespace') + 14
        breite = schrift.measure(text) + 26
        c = tk.Canvas(reihe, width=breite, height=hoehe, bg=BG,
                      highlightthickness=0, bd=0, cursor='hand2')
        c.pack(side='left', padx=(0, 6))
        flaeche = [_rundes_rechteck(c, 1, 1, breite - 1, hoehe - 1, radius=5,
                                    fill=FLAECHE, outline=ACCENT if an else LINIE,
                                    width=1)]
        beschr = c.create_text(breite / 2.0, hoehe / 2.0, text=text,
                               fill=ACCENT if an else SUB, font=schrift)
        # Dieselbe Falle wie beim gewoehnlichen Knopf — siehe `_nachmessen`.
        _mass_sichern(c, beschr, flaeche, hoehe, FLAECHE,
                      ACCENT if an else LINIE)
        c.teile = (flaeche, beschr)
        c.bind('<Button-1>', lambda e, k=kennung: tat(k))
        c.ist_knopf = True      # damit tools/randpruefung.py ihn prüft
        knoepfe[kennung] = c

    def setzen(gewaehlt):
        for kennung, c in knoepfe.items():
            an = (kennung == gewaehlt)
            flaeche, beschr = c.teile
            c.itemconfigure(flaeche[0], outline=ACCENT if an else LINIE)
            c.itemconfigure(beschr, fill=ACCENT if an else SUB)

    reihe.setzen = setzen
    return reihe


def _status(fenster, eltern, symbol, fett, rest, farbe=None):
    """Ein Statuskasten mit farbigem Balken links — wie in der Vorschau.

    ⚠ `symbol` ist ein Name aus `scbp/zeichen.py` („haken", „offen"), kein
    Schriftzeichen mehr. Der Parameter hieß bis v3.0.0-rc55 `zeichen` und hätte
    das gleichnamige Modul verdeckt.
    """
    farbe = farbe or ACCENT
    innen = _karte(eltern, rand=farbe, pady=(0, 14))
    zeile = tk.Frame(innen, bg=FLAECHE)
    zeile.pack(fill='x', padx=14, pady=12)
    zeichen.zeile(zeile, symbol, grund=FLAECHE, schrift=fenster.f_grund,
                  farbe=zeichen.GRAU if farbe == SUB else zeichen.GRUEN
                  ).pack(side='left', padx=(0, 10), anchor='n')
    text = tk.Frame(zeile, bg=FLAECHE)
    text.pack(side='left', fill='x', expand=True)
    oben = tk.Label(text, text=fett, bg=FLAECHE, fg=FG, font=fenster.f_fett,
                    anchor='w', justify='left')
    oben.pack(fill='x')
    _umbruch(oben)
    if rest:
        unten = tk.Label(text, text=rest, bg=FLAECHE, fg=SUB,
                         font=fenster.f_klein, anchor='w', justify='left')
        unten.pack(fill='x')
        _umbruch(unten)
    return innen


def _pfadfeld(fenster, eltern, wert, waehlen, oeffnen=None, platzhalter=''):
    """Ein Pfad mit Knopf daneben."""
    reihe = tk.Frame(eltern, bg=BG)
    reihe.pack(fill='x', pady=(8, 0))
    from .hauptfenster import rundes_feld
    feld = rundes_feld(reihe, wert, fenster.f_klein, '#0c1017', LINIE, ACCENT, FG)
    feld.halter.pack(side='left', fill='x', expand=True, padx=(0, 8))
    if platzhalter and not wert.get():
        feld.configure(fg=SUB)
    _knopf(fenster, reihe, t('s_durchsuchen'), waehlen).pack(side='left')
    if oeffnen:
        _knopf(fenster, reihe, t('s_oeffnen'), oeffnen).pack(side='left', padx=(8, 0))
    return reihe



def _masszahl(widget, wert, ersatz=0):
    """Eine Tk-Massangabe als ganze Zahl lesen.

    ⚠ `cget()` liefert je nach Widget und Option mal ein `int`, mal einen
    String, mal ein `_tkinter.Tcl_Obj`. Auf Letzteres wirft `int()` einen
    **TypeError** — und der wurde von `except (TclError, ValueError)` nicht
    gefangen, weil ein TypeError keins von beiden ist.

    Gemessen am 28.08.2026 unter Linux (AppImage, Python 3.14.6, Tk 8.6):
    **50 von 50** aufgehobenen Fehlern kamen aus dieser einen Stelle. Die Folge
    war nicht nur ein volles Protokoll — `_umbruch` brach ab, *bevor* es
    `wraplength` setzen konnte. Der Text blieb einzeilig und breit, wurde am
    Fensterrand abgeschnitten und drückte die Schalter rechts hinaus. Auf
    "Texte im Spiel" und "Bestand" war das bei kleiner Fenstergröße sichtbar.

    `tk.getint()` ist Tks eigener Umwandler und versteht alle drei Formen.
    """
    try:
        return widget.tk.getint(wert)
    except (tk.TclError, ValueError, TypeError):
        return ersatz


def _umbruch(label, anteil=1.0, abzug=0, bezug=None, neben=None):
    """Den Zeilenumbruch an die tatsächliche Breite hängen.

    ⚠ Feste Werte wie `wraplength=560` sind der Grund, warum Text bei kleinem
    Fenster abgeschnitten statt umgebrochen wurde: Sie stimmen genau für die
    eine Fenstergröße, bei der sie eingetragen wurden. Wer das Fenster auf die
    Mindestgröße zieht oder auf Englisch umstellt, sieht Stümpfe.

    `anteil` ist für nebeneinanderliegende Kästen (zwei Spalten → 0.5).

    `bezug` ist der Rahmen, an dem gemessen wird — normalerweise der eigene
    Elternrahmen. ⚠ Er taugt nicht immer: Steht rechts noch ein Bedienelement,
    hat der linke Rahmen bereits die **zu große** Breite, die den Überlauf
    überhaupt erst verursacht. Dann wird am gemeinsamen Elternrahmen gemessen.

    `neben` ist genau dieses Bedienelement. Seine gebrauchte Breite wird
    abgezogen, denn diesen Platz gibt es für den Text nicht.
    """
    ziel = bezug if bezug is not None else label.master

    def nachziehen(_=None):
        # ⚠ Erst nachsehen, ob es die Widgets noch gibt. `label.after(0, …)`
        # unten plant einen Rückruf ein, und beim Seitenwechsel wird das Label
        # zerstört, bevor er drankommt — dann meldet Tk `invalid command name
        # .!...!label`. Dasselbe beim `<Configure>` des Elternrahmens: Der lebt
        # noch, das Label darin nicht mehr.
        #
        # Der Fehler stürzte nichts ab (der Haken in `fehler.py` fängt ihn), er
        # füllte nur das Protokoll: acht Einträge in einem Bericht vom
        # 27.08.2026, alle aus demselben Augenblick.
        try:
            if not (label.winfo_exists() and ziel.winfo_exists()):
                return
        except tk.TclError:
            return
        breite = ziel.winfo_width()
        if neben is not None:
            try:
                breite -= neben.winfo_reqwidth()
            except tk.TclError:
                pass
        if breite > 40:
            # ⚠ Der eigene Rahmen des Labels zählt mit. `wraplength` begrenzt
            # nur den TEXT; was das Label am Ende belegt, ist Text + Rand +
            # Innenabstand. Stand `wraplength` auf der vollen Breite, brauchte
            # es also ein paar Pixel mehr, als es bekam — und Tk schnitt still
            # ab. Genau so gemessen am 28.08.2026: die englische Warnzeile auf
            # der Spiel-Seite ragte um 5 px heraus, bei 1100×842.
            #
            # Erfragt statt geschätzt, damit es auch bei anderer Darstellung
            # stimmt.
            try:
                rand = 2 * (_masszahl(label, label.cget('borderwidth'))
                            + _masszahl(label, label.cget('padx'))
                            + _masszahl(label, label.cget('highlightthickness')))
            except tk.TclError:
                rand = 4
            try:
                label.configure(wraplength=max(160, int(breite * anteil)
                                               - abzug - rand))
            except tk.TclError:
                pass          # zwischen Prüfung und Zugriff zerstört

    ziel.bind('<Configure>', nachziehen, add='+')
    # ⚠ `<Configure>` allein reicht nicht. Seiten werden gebaut, während sie
    # noch versteckt sind — dort meldet Tk Breite 1, und wenn beim späteren
    # Einblenden die Fenstergröße zufällig gleich bleibt, kommt nie ein
    # `<Configure>` mehr. Der Umbruch bliebe dann auf dem Notwert stehen.
    # `<Map>` feuert genau dann, wenn das Element wirklich sichtbar wird.
    label.bind('<Map>', nachziehen, add='+')
    label.after(0, nachziehen)
    return label


def _knopfreihe(eltern, knoepfe, abstand=8):
    """Knöpfe nebeneinander — und untereinander, sobald der Platz nicht reicht.

    ⚠ Tk bricht eine Knopfreihe nicht um. Passt sie nicht, schneidet es den
    letzten Knopf einfach ab: Auf der Über-Seite stand bei Mindestbreite
    sichtbar „Einrichtung wiederho…". Aufgefallen ist das erst auf einem
    Bildschirmfoto — die Randprüfung hatte Knöpfe als Rollflächen ausgenommen,
    weil jeder Knopf hier ein `Canvas` ist.
    """
    def ordnen(_=None):
        # Wie bei `_umbruch`: Der Rückruf kann nach dem Seitenwechsel drankommen,
        # wenn die Knöpfe längst zerstört sind.
        try:
            if not eltern.winfo_exists():
                return
            if not all(k.winfo_exists() for k in knoepfe):
                return
        except tk.TclError:
            return
        platz = eltern.winfo_width()
        gebraucht = sum(k.winfo_reqwidth() for k in knoepfe) \
            + abstand * (len(knoepfe) - 1)

        # ⚠⚠ **Erst Platz schaffen, dann umbrechen.** Untereinander stehende
        # Knöpfe sehen aus wie ein Fehler — Xharig: „das sieht schrecklich
        # aus." Bevor umgebrochen wird, fordert die Reihe deshalb die Breite
        # an, die sie braucht.
        #
        # Eine feste Mindestbreite genügt dafür nicht: Wie breit ein Knopf
        # wirklich wird, steht erst fest, wenn er gezeichnet ist — unter
        # Wayland fällt das messbar anders aus als hier. Zwei Anläufe mit
        # geschätzten Zahlen (1100, dann 1160) reichten beide nicht.
        try:
            oben = eltern.winfo_toplevel()
            fehlend = gebraucht - platz
            if platz > 1 and fehlend > 0:
                noetig = oben.winfo_width() + fehlend + 4
                # Nicht breiter als der Bildschirm — sonst schiebt sich das
                # Fenster aus dem Bild, und das ist schlimmer als ein Umbruch.
                grenze = oben.winfo_screenwidth() - 40
                if noetig <= grenze:
                    # ⚠⚠ **Die Mindesthöhe bleibt, wie sie ist.**
                    #
                    # `minsize()` setzt immer beide Werte. Hier geht es aber nur
                    # um die BREITE — wie hoch das Fenster sein muss, hat mit
                    # der Knopfreihe nichts zu tun. Bis 3.9.5 stand an dieser
                    # Stelle `oben.winfo_height()`, also die gerade aktuelle
                    # Höhe: Wer sein Fenster einmal hoch gezogen hatte und dann
                    # eine Seite mit breiter Knopfreihe öffnete, konnte es nie
                    # wieder niedriger ziehen. Gemeldet mit `Fenster 1770×899,
                    # mindestens 1770×899` — beide Maße gleich, das Fenster saß
                    # in seiner eigenen Größe fest, obwohl `MIN_HOEHE` 380 ist.
                    #
                    # Es ist derselbe Fehler wie in Falle 4 der Projektnotiz,
                    # nur andersherum: Dort blieb `minsize` beim Verkleinern
                    # stehen, hier wächst es beim Vergrößern mit. Beide Male
                    # gilt: Wer die Fenstergröße anfasst, fasst genau die
                    # Maße an, um die es geht — und keine weiteren.
                    _, min_hoch = oben.minsize()
                    oben.minsize(noetig, min_hoch)
                    if oben.winfo_width() < noetig:
                        oben.geometry('%dx%d' % (noetig, oben.winfo_height()))
                    return          # `<Configure>` kommt gleich mit mehr Platz
        except tk.TclError:
            pass

        nebeneinander = platz <= 1 or gebraucht <= platz
        if nebeneinander == getattr(eltern, 'zuletzt_nebeneinander', None):
            return
        eltern.zuletzt_nebeneinander = nebeneinander
        for nummer, knopf in enumerate(knoepfe):
            knopf.pack_forget()
            if nebeneinander:
                knopf.pack(side='left', padx=(0 if nummer == 0 else abstand, 0))
            else:
                knopf.pack(side='top', anchor='w',
                           pady=(0 if nummer == 0 else 6, 0))

    eltern.bind('<Configure>', ordnen, add='+')
    eltern.after(0, ordnen)
    return eltern


def _fliesstext(eltern, text, schrift, farbe=SUB, grund=BG, abzug=0, **pack):
    """Ein Absatz, der mit dem Fenster mitgeht.

    Der Regelweg für jeden mehrzeiligen Text. Wer stattdessen `wraplength=600`
    schreibt, baut den Fehler wieder ein, den diese Funktion behebt: Der Wert
    passt für die eine Fenstergröße, bei der er entstanden ist.

    `abzug` ist der waagerechte Rand, den der Text nicht benutzen darf —
    üblicherweise das Doppelte des `padx` beim Packen.
    """
    label = tk.Label(eltern, text=text, bg=grund, fg=farbe, font=schrift,
                     anchor='w', justify='left')
    label.pack(**pack)
    return _umbruch(label, abzug=abzug)


def _ohne_marken(text):
    """Die Auszeichnung aus einem Text nehmen — `**fett**` und Rueckstriche.

    ⚠ Tk-Labels können kein Mischformat — ein Label ist ganz fett oder gar
    nicht. Die Sternchen in `sprache.py` markieren die Betonung fuer den
    Leser der Sprachdatei; auf dem Bildschirm haben sie nichts zu suchen.

    Die Danke-Seite entfernte sie schon, die Einstellungszeilen nicht: Auf
    "Texte im Spiel" stand dadurch woertlich `**ganze Spiel**` auf dem
    Bildschirm (gefunden von am 28.08.2026 gemeldet unter rc85). Damit das
    nicht bei jedem neuen Text wieder passiert, geht es jetzt durch diese
    eine Stelle.

    ⚠ Dasselbe gilt fuer die Rueckstriche um Befehle und Werte. Sie kommen aus
    dem Änderungsprotokoll, das die Seite „Was ist neu" anzeigt, und standen
    dort bis rc42 mit auf dem Bildschirm.
    """
    return text.replace('**', '').replace('`', '') if text else text


def _feld(fenster, eltern, bezeichnung, hilfe, breit=False, oben=False):
    """Eine Einstellungszeile: Bezeichnung, Erklärung, Platz für das Bedienelement.

    `oben=True` heftet die Beschriftung an die **Oberkante** statt sie
    mittig zu setzen. ⚠ Gebraucht bei Feldern, die im Betrieb wachsen: Klappt
    ein Auswahlfeld seine Liste auf, wird die Zeile plötzlich zehn Zeilen hoch
    — und die Beschriftung stand dann auf halber Höhe irgendwo neben der Liste
    statt neben ihrem Feld.
    """
    zeile = tk.Frame(eltern, bg=BG)
    zeile.pack(fill='x', pady=(12, 0))
    links = tk.Frame(zeile, bg=BG)
    links.pack(side='left', fill='x', expand=True,
               **({'anchor': 'n'} if oben else {}))
    beschriftung = tk.Label(links, text=bezeichnung, bg=BG, fg=FG,
                            font=fenster.f_fett, anchor='w')
    beschriftung.pack(fill='x')
    erklaerung = None
    if hilfe:
        erklaerung = tk.Label(links, text=_ohne_marken(hilfe), bg=BG, fg=SUB,
                              font=fenster.f_klein, anchor='w', justify='left')
        erklaerung.pack(fill='x')
    if breit:
        # Breite Bedienelemente unter die Beschreibung statt daneben: Auf
        # Englisch sind die Wörter länger, und rechts wurde der letzte Knopf
        # abgeschnitten („Ve…" statt „Very large").
        rechts = tk.Frame(links, bg=BG)
        rechts.pack(fill='x', anchor='w', pady=(8, 0))
        # ⚠ Auch hier braucht es einen Abzug. Ohne ihn bekommt der Text die
        # **volle** Breite der Zeile — die Ränder der Rollfläche darum sind
        # damit nicht eingerechnet, und die letzten Pixel fallen weg
        # (gemessen: 5, tools/randpruefung.py).
        #
        # Die Beschriftung braucht denselben Umbruch: Auf Englisch sind die
        # Wörter länger, und bisher hatte sie in diesem Zweig gar keinen.
        if erklaerung is not None:
            _umbruch(erklaerung, bezug=zeile, abzug=10)
        _umbruch(beschriftung, bezug=zeile, abzug=10)
    else:
        rechts = tk.Frame(zeile, bg=BG)
        rechts.pack(side='right', padx=(16, 0),
                    **({'anchor': 'n'} if oben else {}))
        # ⚠ Hier NICHT an `links` messen: Der Rahmen ist in genau dem Moment
        # zu breit, in dem der Text überläuft — er würde den Fehler bestätigen
        # statt ihn zu beheben. Gemessen wird am gemeinsamen Elternrahmen
        # abzüglich des Bedienelements, das rechts steht.
        # ⚠ `abzug` deckt mehr ab als nur `padx=(16, 0)`: `winfo_reqwidth()`
        # liefert die **gewünschte** Breite des Bedienelements, nicht die
        # tatsächliche. Bei Schiebeschaltern und Zahlenfeldern liegen ein paar
        # Pixel dazwischen — gemessen fehlten 5 (tools/randpruefung.py). Mit
        # Luft bricht der Text minimal früher um, statt abgeschnitten zu werden.
        if erklaerung is not None:
            _umbruch(erklaerung, bezug=zeile, neben=rechts, abzug=26)
        _umbruch(beschriftung, bezug=zeile, neben=rechts, abzug=26)
    tk.Frame(eltern, bg=LINIE, height=1).pack(fill='x', pady=(12, 0))
    # ⚠ Die linke Spalte haengt am Rueckgabewert. Manche Zeilen wollen dort
    # etwas unterbringen — der Namensvorschlag im Lager etwa gehoert neben das
    # Eingabefeld, nicht ans Seitenende. Ohne diesen Griff muesste der Aufrufer
    # sich durch `winfo_children()` hangeln, und das bricht beim naechsten
    # Umbau still.
    rechts.links = links
    # ⚠ Auch die Beschriftung durchreichen. Eine Zeile, deren Einheit sich
    # umschalten laesst (Menge im Lager: SCU ↔ cSCU), muss ihren eigenen Text
    # aendern koennen — sonst steht dort „Menge (SCU)", waehrend cSCU gemeint
    # ist, und die eingetragene Menge ist um den Faktor 100 daneben.
    rechts.beschriftung = beschriftung
    return rechts


# --------------------------------------------------------------------- Seiten
def _liste(fenster, rahmen):
    """Die Bauplan-Liste — das vorhandene Fenster, eingebettet."""
    from . import bestandsfenster
    fenster.bestandsseite = bestandsfenster.Bestandsfenster(rahmen=rahmen)

    # ⚠ Beim erneuten Aufrufen ohne Filter anfangen. Die Seite wird nur ein-
    # und ausgeblendet, sonst stünde die Auswahl von vorhin noch da — und wer
    # „Andockkragen, Größe 2, Grad A" vergessen hat, sieht „Nichts gefunden"
    # und hält den Bestand für leer. Am 29.08.2026 gemeldet.
    def _frisch():
        seite = getattr(fenster, 'bestandsseite', None)
        if seite is None:
            return
        seite._fein_leeren()
        seite._suche_leeren()

    fenster.beim_zeigen['liste'] = _frisch


def _fortschritt(fenster, rahmen):
    """Wie weit bin ich? — nach Bereichen gegliedert, jeder Bereich aufklappbar.

    ⚠ Vorher standen hier alle 25 Kategorien in einer einzigen langen Liste. Bei
    722 Bauplänen sucht man darin ewig, und der eine Wert, der einen gerade
    interessiert, steht irgendwo in der Mitte. Jetzt zuerst die vier Bereiche mit
    ihrem Gesamtstand — und die Einzelheiten erst auf Klick. Eingeklappt zu
    starten ist Absicht: Der Überblick ist die Antwort auf „wie weit bin ich",
    die Kategorien sind die Antwort auf „und wo genau".
    """
    _ueberschrift(fenster, rahmen, t('hf_fortschritt'), t('s_fo_lead'))
    innen = _rollflaeche(rahmen)
    try:
        bestand = bestand_datei.laden()
        katalog = katalog_modul.laden()
    except Exception as ausnahme:
        fehler.merken('seiten.fortschritt', ausnahme)
        return

    bp = katalog.get('bauplaene') or {}
    habe = set(bestand.get('bauplaene') or {})
    # Je Bereich: Liste von (Kategorie, gesamt, meine)
    nach_bereich = {}
    for schluessel, e in bp.items():
        roh = katalog_modul.art_kennung(e)
        bereich = katalog_modul.obergruppe(roh)
        art = katalog_modul.art_lesbar(roh) if roh else '—'
        zaehler = nach_bereich.setdefault(bereich, {})
        gesamt, meine = zaehler.get(art, (0, 0))
        zaehler[art] = (gesamt + 1, meine + (1 if schluessel in habe else 0))

    gesamt_alle = sum(g for z in nach_bereich.values() for g, _ in z.values()) or 1
    meine_alle = sum(m for z in nach_bereich.values() for _, m in z.values())

    kopf = tk.Frame(innen, bg=BG)
    kopf.pack(fill='x', pady=(0, 4))
    tk.Label(kopf, text=str(meine_alle), bg=BG, fg=ACCENT,
             font=fenster.f_titel).pack(side='left')
    tk.Label(kopf, text=t('s_fo_von')
             % (gesamt_alle, 100.0 * meine_alle / gesamt_alle),
             bg=BG, fg=SUB, font=fenster.f_klein).pack(side='left')

    from .hauptfenster import rundbalken
    rundbalken(innen, 9, meine_alle / float(gesamt_alle), BG, '#222b3b',
               ACCENT).pack(fill='x', pady=(6, 18))

    for bereich in katalog_modul.OBERGRUPPEN:
        zaehler = nach_bereich.get(bereich)
        if not zaehler:
            continue
        gesamt = sum(g for g, _ in zaehler.values())
        meine = sum(m for _, m in zaehler.values())
        _fortschritt_bereich(fenster, innen, t('gruppe_' + bereich), gesamt,
                             meine, zaehler)

    _lohnende_auftraege(fenster, innen, katalog, habe)


def _lohnende_auftraege(fenster, eltern, katalog, habe):
    """„Was bringt am meisten?" — die Aufträge mit den meisten fehlenden BPs.

    ⚠ Die Frage nach dem Fortschritt endet sonst bei „55 Prozent" und lässt
    einen damit allein. Hier steht, **was als Nächstes den größten Schritt
    macht**: ein einziger Auftrag bringt bis zu 44 fehlende Baupläne auf einmal.
    Gerechnet wird auf Daten, die ohnehin geladen sind — kein Netz, kein
    weiterer Datenweg.
    """
    try:
        lohnend = katalog_modul.lohnende_auftraege(katalog, habe)
    except Exception as ausnahme:
        fehler.merken('seiten.lohnende_auftraege', ausnahme)
        return

    tk.Label(eltern, text=t('s_fo_lohnt'), bg=BG, fg=FG,
             font=fenster.f_grund, anchor='w').pack(fill='x', pady=(22, 2))
    _fliesstext(eltern, t('s_fo_lohnt_hilfe'), fenster.f_klein, fill='x')

    if not lohnend:
        _fliesstext(eltern, t('s_fo_lohnt_leer'), fenster.f_klein,
                    fill='x', pady=(8, 0))
        return

    from .hauptfenster import rundrahmen
    kasten = rundrahmen(eltern, FLAECHE, LINIE, radius=8, grundfarbe=BG)
    kasten.halter.pack(fill='x', pady=(10, 0))
    # ⚠ Nur die ersten zehn. Es sind 170 — eine vollständige Liste wäre keine
    # Antwort auf „was mache ich als Nächstes", sondern die nächste Suchaufgabe.
    # ⚠ **Der Annahmeort gehört an die Zeile.** Er lag von Anfang an vor —
    # `lohnende_auftraege` liefert ihn als sechsten Wert — und wurde hier
    # weggeworfen. Damit beantwortete die Seite „welcher Auftrag lohnt sich"
    # und ließ die Anschlussfrage „und wo nehme ich den an" offen; genau
    # dieselbe Lücke war im Bauplan-Fenster schon einmal gemeldet worden,
    # weshalb es `ort_text()` überhaupt gibt. Sie wurde dort geschlossen und
    # hier nicht.
    from .bestandsfenster import ort_text
    for titel, fraktion, anzahl, uec, rang, wo in lohnend[:10]:
        zeile = tk.Frame(kasten, bg=FLAECHE)
        zeile.pack(fill='x', padx=14, pady=3)
        tk.Label(zeile, text=str(anzahl), bg=FLAECHE, fg=ACCENT,
                 font=fenster.f_grund, width=3, anchor='e').pack(side='left')
        rechts = tk.Frame(zeile, bg=FLAECHE)
        rechts.pack(side='left', fill='x', expand=True, padx=(10, 0))
        tk.Label(rechts, text=titel, bg=FLAECHE, fg=FG, font=fenster.f_klein,
                 anchor='w').pack(fill='x')
        teile = [fraktion] if fraktion else []
        if uec:
            teile.append('%s aUEC' % '{:,}'.format(uec).replace(',', '.'))
        if rang:
            teile.append(rang)
        teile.append(t('s_fo_lohnt_topf', anzahl))
        tk.Label(rechts, text=' · '.join(teile), bg=FLAECHE, fg=SUB,
                 font=fenster.f_klein, anchor='w').pack(fill='x')
        # Der Annahmeort steht direkt da — er beantwortet „wo finde ich den".
        ort = ort_text(wo)
        beschriftungen = []
        if ort:
            ort_label = tk.Label(rechts, text=ort, bg=FLAECHE, fg=SUB,
                                 font=fenster.f_klein, anchor='w',
                                 justify='left')
            ort_label.pack(fill='x')
            beschriftungen.append(ort_label)
        tk.Label(rechts, text=t('s_fo_lohnt_klick'), bg=FLAECHE, fg=SUB,
                 font=fenster.f_klein, anchor='w').pack(fill='x')

        # ⚠⚠ **Die Zahl ist keine Antwort, sie ist eine Frage.** „44" sagt
        # nicht, WELCHE 44 — und danach fragt man als Nächstes. Der Klick
        # führt deshalb in die Bauplan-Liste, gefiltert auf diesen Auftrag;
        # dort steht jeder einzelne, mit Haken für das, was man schon hat.
        # Die Liste kann das längst (`self.auftrag` als Filter), sie war von
        # hier aus nur nicht erreichbar: Man musste den Auftragsnamen von Hand
        # ins Suchfeld tippen und dann die Auftragszeile anklicken.
        def hinspringen(_ereignis=None, titel=titel):
            _zum_auftrag(fenster, titel)

        for teil in [zeile, rechts] + beschriftungen:
            teil.config(cursor='hand2')
            teil.bind('<Button-1>', hinspringen)
        for kind in rechts.winfo_children():
            kind.config(cursor='hand2')
            kind.bind('<Button-1>', hinspringen)
    tk.Label(kasten, text='', bg=FLAECHE).pack(pady=2)


def _fortschritt_bereich(fenster, eltern, titel, gesamt, meine, kategorien):
    """Ein Bereich mit Gesamtbalken — die Kategorien darin klappen auf."""
    from .hauptfenster import rundbalken
    zustand = {'offen': False}

    kopf = tk.Frame(eltern, bg=BG, cursor='hand2')
    kopf.pack(fill='x', pady=(10, 2))
    pfeil = zeichen.zeile(kopf, 'aufklappen', grund=BG,
                          schrift=fenster.f_klein)
    pfeil.pack(side='left')
    tk.Label(kopf, text=titel, bg=BG, fg=FG, font=fenster.f_fett,
             anchor='w').pack(side='left')
    tk.Label(kopf, text='  %d / %d' % (meine, gesamt), bg=BG, fg=SUB,
             font=fenster.f_klein, anchor='w').pack(side='left')

    anteil = max(0.0, min(1.0, meine / float(gesamt or 1)))
    rundbalken(eltern, 9, anteil, BG, '#222b3b', ACCENT).pack(fill='x',
                                                              pady=(2, 0))

    koerper = tk.Frame(eltern, bg=BG)

    def zeichnen():
        if koerper.winfo_children():
            return
        for art, (art_gesamt, art_meine) in sorted(kategorien.items(),
                                                   key=lambda x: -x[1][0]):
            zeile = tk.Frame(koerper, bg=BG)
            zeile.pack(fill='x', pady=3)
            tk.Label(zeile, text=art, bg=BG, fg=SUB, font=fenster.f_klein,
                     width=22, anchor='w').pack(side='left')
            teil = max(0.0, min(1.0, art_meine / float(art_gesamt or 1)))
            rundbalken(zeile, 7, teil, BG, '#222b3b', ACCENT,
                       breite=260).pack(side='left', padx=8)
            tk.Label(zeile, text='%d / %d' % (art_meine, art_gesamt), bg=BG,
                     fg=SUB, font=fenster.f_klein, width=10,
                     anchor='e').pack(side='right')

    def umschalten(*_):
        zustand['offen'] = not zustand['offen']
        pfeil.symbol_tauschen('zuklappen' if zustand['offen']
                             else 'aufklappen')
        if zustand['offen']:
            zeichnen()
            koerper.pack(fill='x', padx=(18, 0), pady=(6, 0))
        else:
            koerper.pack_forget()

    # Der ganze Kopf ist die Schaltfläche, nicht nur der Pfeil.
    for teil in [kopf] + list(kopf.winfo_children()):
        teil.bind('<Button-1>', umschalten)
        try:
            teil.configure(cursor='hand2')
        except tk.TclError:
            pass


def _einstellungen(fenster):
    """Die Bausteine des Einstellungsfensters — einmal erzeugt, mehrfach genutzt."""
    if getattr(fenster, '_einst', None) is None:
        from . import einstellungsfenster
        leer = tk.Frame(fenster.root, bg=BG)     # nur als Halter, wird nie gepackt
        fenster._einst = einstellungsfenster.Einstellungsfenster(rahmen=leer)
        # Ohne diesen Rückruf öffnet ein Sprachwechsel ein zweites Fenster.
        fenster._einst.beim_sprachwechsel = fenster.neu_aufbauen
        # ⚠ Und ohne diesen laufen alle Rückmeldungen ins Leere: Eingebettet gibt
        # es den Fuß des Einstellungsfensters nicht, also auch sein Meldungs-Label
        # nicht. Jeder Klick auf „Jetzt auffrischen", „Übersetzung prüfen" oder eine
        # Textquelle brach deshalb mit `AttributeError` ab, **bevor** überhaupt
        # etwas passierte — die Seite sah fertig aus und tat nichts.
        fenster._einst.melder = fenster.sagen
    return fenster._einst


def _allgemein(fenster, rahmen):
    from . import autostart, pfade
    from .hauptfenster import schiebeschalter
    _ueberschrift(fenster, rahmen, t('hf_allgemein'),
                  t('s_allg_lead'))
    innen = _rollflaeche(rahmen)
    e = _einstellungen(fenster)

    ziel = _feld(fenster, innen, t('e_sprache'), t('s_sprache_h'),
                 breit=True)
    wahl = _wahl(fenster, ziel,
                 [('auto', t('sprache_auto')), ('de', 'Deutsch'), ('en', 'English')],
                 pfade.einstellungen().get('sprache') or 'auto',
                 lambda k: (wahl.setzen(k), e._sprache_waehlen(k)))
    wahl.pack()

    ziel = _feld(fenster, innen, t('e_ton'),
                 t('s_ton_h'))

    def ton_um():
        neu_wert = not pfade.einstellung_wahrheit('signalton', True)
        pfade.einstellung_setzen('signalton', neu_wert)
        fenster.sagen('%s: %s' % (t('e_ton'), t('e_an') if neu_wert else t('e_aus')))
        return neu_wert

    schiebeschalter(ziel, pfade.einstellung_wahrheit('signalton', True),
                    ton_um).pack()

    ziel = _feld(fenster, innen,
                 t('autostart_win') if sys.platform.startswith('win')
                 else t('autostart_linux'),
                 t('s_autostart_h'))
    if autostart.moeglich():
        def autostart_um():
            neu_wert = not autostart.ist_an()
            autostart.setzen(neu_wert)
            fenster.sagen(t('s_al_autostart')
                          % (t('e_an') if neu_wert else t('e_aus')))
            return autostart.ist_an()

        schalter = schiebeschalter(ziel, autostart.ist_an(), autostart_um)
        schalter.pack()
        # Mitschalten, wenn der Autostart woanders umgestellt wird — etwa am
        # Symbol im Overlay, das ja gleichzeitig sichtbar ist.
        autostart.anzeige_anmelden(
            lambda: schalter.zeichnen(autostart.ist_an()))
    else:
        tk.Label(ziel, text=t('s_nicht_moegl'), bg=BG, fg=SUB,
                 font=fenster.f_klein).pack()

    _menueeintrag_feld(fenster, innen)

    ziel = _feld(fenster, innen, t('s_tray'),
                 t('s_tray_h'))
    if sys.platform.startswith('win'):
        def tray_um():
            neu_wert = not pfade.einstellung_wahrheit('tray', True)
            pfade.einstellung_setzen('tray', neu_wert)
            return neu_wert

        schiebeschalter(ziel, pfade.einstellung_wahrheit('tray', True),
                        tray_um).pack()
    else:
        tk.Label(ziel, text=t('s_nur_win'), bg=BG, fg=SUB,
                 font=fenster.f_klein).pack()


def _anzeige(fenster, rahmen):
    from . import pfade
    from .hauptfenster import schiebeschalter
    _ueberschrift(fenster, rahmen, t('hf_anzeige'),
                  t('s_anz_lead'))
    innen = _rollflaeche(rahmen)
    e = _einstellungen(fenster)

    # --- Wie sich das Overlay im Spiel verhält -------------------------------
    # Angestoßen von einer Rückmeldung von Haldjas (pr0): „Das Overlay ist permanent
    # zu sehen und nicht durchklickbar. Wenn ich im Kampf mit der Maus
    # hineinkomme, wird das unangenehm."
    ziel = _feld(fenster, innen, t('s_ov_modus'), t('s_ov_modus_h'), breit=True)
    modus = _wahl(fenster, ziel,
                  [('immer', t('s_ov_immer')), ('popup', t('s_ov_popup'))],
                  pfade.einstellung('overlay_modus') or 'immer',
                  lambda k: _overlay_modus(fenster, modus, k))
    modus.pack()

    # ⚠⚠ **Die Tastenkombination.** Star Citizen laeuft im Vollbild und blendet
    # den Mauszeiger aus: Wer nachsehen will, ob er einen Bauplan schon hat,
    # muss heraustabben und das Fenster dann BLIND suchen und anklicken. Am
    # 31.08.2026 als Nutzerwunsch gemeldet.
    # ⚠⚠ **Die Ecke — im Pop-up-Betrieb der einzige Weg.** Dort reicht das
    # Overlay Mausklicks durch und laesst sich deshalb nicht ziehen. Ohne
    # diese Einstellung koennen diese Nutzer es ueberhaupt nicht
    # positionieren. Am 31.08.2026 gemeldet.
    ziel = _feld(fenster, innen, t('s_ov_ecke'), t('s_ov_ecke_h'), breit=True)
    ecke = _wahl(fenster, ziel,
                 [('frei', t('s_ov_ecke_frei')),
                  ('oben-links', t('s_ov_ecke_ol')),
                  ('oben-rechts', t('s_ov_ecke_or')),
                  ('unten-links', t('s_ov_ecke_ul')),
                  ('unten-rechts', t('s_ov_ecke_ur'))],
                 pfade.einstellung('overlay_ecke') or 'frei',
                 lambda k: _overlay_ecke(fenster, ecke, k))
    ecke.pack()

    _hotkey_feld(fenster, innen)

    ziel = _feld(fenster, innen, t('s_ov_dauer'), t('s_ov_dauer_h'))
    from .hauptfenster import rundes_feld as _zahlfeld
    dauer = _zahlfeld(ziel, None, fenster.f_klein, '#0c1017', LINIE, ACCENT, FG,
                      breite=6, justify='right')
    dauer.insert(0, str(pfade.einstellung_zahl('popup_sekunden', 6, 2, 60)))
    dauer.halter.pack()

    def dauer_merken(_=None):
        try:
            wert = max(2, min(60, int(dauer.get())))
            pfade.einstellung_setzen('popup_sekunden', wert)
            fenster.sagen(t('s_ov_dauer_sagen') % wert)
        except ValueError:
            pass

    dauer.bind('<FocusOut>', dauer_merken)
    dauer.bind('<Return>', dauer_merken)

    ziel = _feld(fenster, innen, t('s_ov_durch'), t('s_ov_durch_h'))
    if _durchklick_moeglich():
        # ⚠ Nicht `_schalter` nennen — so heisst in dieser Datei bereits eine
        # Funktion, und ein lokaler Name wuerde sie verdecken (Selbsttest 67).
        _durch_schalter = schiebeschalter(
            ziel, pfade.einstellung_wahrheit('durchklickbar', False),
            lambda: _durchklick_um(fenster))
        _durch_schalter.pack()

        # ⚠ Das Durchreichen laesst sich auch am Schloss des Overlays umlegen.
        # Ohne diesen Draht zeigte der Schalter dann weiter den alten Zustand,
        # solange die Seite offen war.
        def _nachziehen(zustand):
            try:
                if _durch_schalter.winfo_exists():
                    _durch_schalter.zeichnen(bool(zustand))
            except tk.TclError:
                pass                     # Seite ist weg - nichts nachzuziehen

        # ⚠ `overlay` wird hier lokal geholt wie ueberall in dieser Datei:
        # Auf Modulebene waere es ein Zirkelbezug.
        from . import overlay as _ov
        _ov.DURCHKLICK_ANZEIGE[0] = _nachziehen
    else:
        # Ehrlich statt still: Unter nativem Wayland kann ein gewöhnliches Fenster
        # keine Klicks weiterreichen. Ein Schalter, der nichts bewirkt, wäre
        # schlimmer als gar keiner.
        tk.Label(ziel, text=t('s_ov_durch_nein'), bg=BG, fg=SUB,
                 font=fenster.f_klein, anchor='w', justify='left').pack(fill='x')

    ziel = _feld(fenster, innen, t('hf_schrift'), t('hf_schrift_hilfe'),
                 breit=True)
    # ⚠⚠ **„Sehr groß" ist bewusst NICHT mehr dabei.** Die Stufe vergrösserte
    # Schrift, Symbole und Knöpfe so weit, dass die daraus folgende
    # Mindesthöhe grösser wurde als ein Bildschirm — bei zwei übereinander
    # stehenden Monitoren lief das Fenster in den zweiten hinein (30.08.2026
    # gemeldet). Das Fenster wird jetzt zwar auf seinem Monitor gehalten
    # (`_auf_den_schirm_holen`), aber dann wäre es randvoll und der Inhalt
    # trotzdem beschnitten. Eine Einstellung, die das Fenster unbrauchbar
    # macht, gehört nicht angeboten.
    #
    # ⚠ Der Wert bleibt im Programm gültig (`STUFEN`, `zeichen.py`): Wer ihn
    # gespeichert hat, verliert nichts — er kann ihn nur nicht neu wählen.
    wahl = _wahl(fenster, ziel,
                 [(s, t('hf_s_' + s))
                  for s in ('klein', 'normal', 'gross')],
                 pfade.einstellung('schriftgroesse') or 'normal',
                 # ⚠ Nur noch der eine Aufruf. `schriftgroesse_setzen()` baut
                 # das Fenster neu auf — damit zeichnet sich die Wahl selbst
                 # richtig, und die Rückmeldung kommt von dort, nach dem
                 # Aufbau. Das frühere `wahl.setzen(k)` und `sagen()` hier
                 # liefen beide ins Leere, sobald neu gezeichnet wurde.
                 lambda k: fenster.schriftgroesse_setzen(k))
    wahl.pack()

    ziel = _feld(fenster, innen, t('e_deckkraft'),
                 t('s_deck_h'))
    from .hauptfenster import regler as schieberegler
    reihe = tk.Frame(ziel, bg=BG)
    reihe.pack()
    wertlabel = tk.Label(reihe, text='%d %%' % e.deckkraft.get(), bg=BG,
                         fg=ACCENT, font=fenster.f_klein, width=6, anchor='e')

    def deckkraft_setzen(w):
        e.deckkraft.set(w)
        wertlabel.configure(text='%d %%' % w)
        try:
            e._deckkraft_vorfuehren(w)
        except Exception:
            pass
        pfade.einstellung_setzen('deckkraft_prozent', w)

    schieberegler(reihe, 30, 100, e.deckkraft.get(),
                  deckkraft_setzen).pack(side='left')
    wertlabel.pack(side='left', padx=(8, 0))

    ziel = _feld(fenster, innen, t('s_klapp'),
                 t('s_klapp_h'))

    def klapp_um():
        neu_wert = not pfade.einstellung_wahrheit('eingeklappt', False)
        pfade.einstellung_setzen('eingeklappt', neu_wert)
        return neu_wert

    schiebeschalter(ziel, pfade.einstellung_wahrheit('eingeklappt', False),
                    klapp_um).pack()

    ziel = _feld(fenster, innen, t('s_vorne'),
                 t('s_vorne_h'))

    def vorne_um():
        neu_wert = not pfade.einstellung_wahrheit('immer_vorne', True)
        pfade.einstellung_setzen('immer_vorne', neu_wert)
        fenster.sagen(t('s_an_vorne')
                      % (t('e_an') if neu_wert else t('e_aus')))
        return neu_wert

    schiebeschalter(ziel, pfade.einstellung_wahrheit('immer_vorne', True),
                    vorne_um).pack()

    ziel = _feld(fenster, innen, t('s_zeilen'),
                 t('s_zeilen_h'))
    from .hauptfenster import rundes_feld
    zahl = rundes_feld(ziel, None, fenster.f_klein, '#0c1017', LINIE, ACCENT, FG,
                       breite=6, justify='right')
    zahl.insert(0, str(pfade.einstellung_zahl('max_zeilen', 20, 5, 100)))
    zahl.halter.pack()

    def zahl_merken(_=None):
        try:
            pfade.einstellung_setzen('max_zeilen',
                                     max(5, min(100, int(zahl.get()))))
            fenster.sagen(t('s_an_zeilen') % zahl.get())
        except ValueError:
            pass

    zahl.bind('<FocusOut>', zahl_merken)
    zahl.bind('<Return>', zahl_merken)

    ziel = _feld(fenster, innen, t('s_lage'),
                 t('s_lage_h'))

    def lage_weg():
        # Die gemerkte Lage wegwerfen reicht nicht: Ohne Positionsangabe stellt Tk
        # das Fenster nach `+0+0`, und bei einem hochkant stehenden Monitor links
        # außen liegt dort gar kein Bild — der Knopf hätte das Overlay also wieder
        # dorthin geschickt, wo man es sucht. Deshalb wird aktiv die Standardlage
        # gesetzt: mittig auf dem Hauptbildschirm. Wie viele Bildschirme jemand hat,
        # wissen wir nicht; die Mitte des Hauptbildschirms passt überall.
        from . import bildschirm
        try:
            os.remove(pfade.app_datei('watcher.json'))
        except OSError:
            pass
        overlay = bildschirm.OVERLAY[0]
        if overlay is not None:
            try:
                overlay.geometry(bildschirm.mittig(overlay, 440, 1000))
            except Exception as ausnahme:
                fehler.merken('seiten.lage_weg', ausnahme)
        fenster.sagen(t('s_an_lage_weg'))

    _knopf(fenster, ziel, t('s_zuruecksetzen'), lage_weg).pack()


def _ordner(fenster, rahmen):
    from . import pfade
    _ueberschrift(fenster, rahmen, t('hf_ordner'),
                  t('s_ordner_lead'))
    innen = _rollflaeche(rahmen)
    e = _einstellungen(fenster)

    gefunden = None
    try:
        gefunden = pfade.spiel_ordner()
    except Exception:
        pass
    if gefunden:
        _status(fenster, innen, 'haken', t('s_sc_da'),
                t('s_or_mitlesen') % gefunden)
    else:
        _status(fenster, innen, '!', t('s_sc_weg'),
                t('s_sc_weg_h'), farbe=GOLD)

    tk.Label(innen, text=t('e_spiel'), bg=BG, fg=FG, font=fenster.f_fett,
             anchor='w').pack(fill='x', pady=(6, 0))
    _fliesstext(innen, t('e_spiel_hilfe'), fenster.f_klein, fill='x')

    def spiel_waehlen():
        # ⚠ Vorher lief das über `e._waehlen(...)`, und das übergibt
        # `parent=self.root` — eingebettet ist das ein Rahmen, der nie gepackt
        # wird. Der Dialog erschien deshalb nicht: „beim Klick passiert nichts".
        gewaehlt = ordner_waehlen(t('e_spiel'), e.spiel.get())
        if gewaehlt:
            e.spiel.set(gewaehlt)
            e._speichern()
            fenster.sagen(t('e_neustart_noetig'))

    _pfadfeld(fenster, innen, e.spiel, spiel_waehlen,
              oeffnen=lambda: fenster.sagen(
                  t('s_or_geoeffnet') if _ordner_zeigen(e.spiel.get())
                  else t('s_or_nicht_auf')))

    tk.Label(innen, text=t('s_eigene'), bg=BG, fg=FG, font=fenster.f_fett,
             anchor='w').pack(fill='x', pady=(20, 0))
    _fliesstext(innen, t('s_eigene_h'), fenster.f_klein, fill='x')
    ablage = tk.StringVar(value=pfade.app_ordner())

    def ablage_oeffnen():
        # Nur melden, was auch stimmt: „Ordner geöffnet" zu sagen, während gar
        # nichts aufgeht, ist schlimmer als eine ehrliche Fehlanzeige.
        fenster.sagen(t('s_or_geoeffnet') if _ordner_zeigen(pfade.app_ordner())
                      else t('s_or_nicht_auf'))

    def ablage_waehlen():
        # ⚠ Hier stand nur ein Hinweis in der Fußzeile („lässt sich in den
        # Einstellungen hinterlegen") — auf der Seite, die genau diese Einstellung
        # IST. Für den Nutzer sah es aus, als täte der Knopf nichts.
        gewaehlt = ordner_waehlen(t('s_eigene'), ablage.get())
        if not gewaehlt:
            return
        pfade.einstellung_setzen('ablage_ordner', gewaehlt)
        ablage.set(gewaehlt)
        fenster.sagen(t('e_neustart_noetig'))

    _pfadfeld(fenster, innen, ablage, ablage_waehlen, oeffnen=ablage_oeffnen)

    tk.Label(innen, text='%s  —  %s' % (t('e_launcher'), t('s_optional')), bg=BG, fg=FG,
             font=fenster.f_fett, anchor='w').pack(fill='x', pady=(20, 0))
    _fliesstext(innen, t('e_launcher_hilfe'), fenster.f_klein, fill='x')
    def launcher_waehlen():
        gewaehlt = ordner_waehlen(t('e_launcher'), e.launcher.get())
        if gewaehlt:
            e.launcher.set(gewaehlt)
            e._speichern()
            fenster.sagen(t('e_neustart_noetig'))

    _pfadfeld(fenster, innen, e.launcher, launcher_waehlen,
              platzhalter=t('s_or_leer'))

    _startbefehl_feld(fenster, innen)


def _overlay_ecke(fenster, wahl, kennung):
    """Die Ecke merken und sofort anwenden.

    ⚠ Sofort, nicht erst beim naechsten Start: Wer eine Ecke waehlt, will
    sehen, ob sie die richtige ist — und im Pop-up-Betrieb kann er das Fenster
    danach nicht selbst hinschieben.
    """
    pfade.einstellung_setzen('overlay_ecke', kennung)
    try:
        wahl.setzen(kennung)
    except Exception:
        pass
    from . import overlay as ov
    steuerung = ov.OVERLAY_STEUERUNG[0]
    if steuerung is not None and hasattr(steuerung, 'ecke_anwenden'):
        steuerung.ecke_anwenden()


def _hotkey_feld(fenster, innen):
    """Die Tastenkombination einstellen — oder ehrlich sagen, warum nicht.

    ⚠⚠ **Unter Wayland steht hier keine Eingabe, sondern die Erklaerung.** Ein
    leeres Feld, das nichts bewirkt, waere schlimmer als gar keins: Der Nutzer
    tippt etwas ein, nichts passiert, und er sucht den Fehler bei sich. Das
    System laesst es nicht zu — also sagen wir das und stellen den fertigen Weg
    daneben.
    """
    from . import hotkey as hk
    geht, grund = hk.moeglich()
    if not geht and grund == 'wayland':
        _feld(fenster, innen, t('s_hk'), t('s_hk_wayland'), breit=True)
        return
    if not geht:
        return                       # kein Bildschirm, kein Windows — still

    ziel = _feld(fenster, innen, t('s_hk'), t('s_hk_h'), breit=True)
    reihe = tk.Frame(ziel, bg=BG)
    reihe.pack(anchor='w')

    from .hauptfenster import rundes_feld
    feld = rundes_feld(reihe, None, fenster.f_klein, '#0c1017', LINIE, ACCENT,
                       FG, breite=18)
    feld.insert(0, pfade.einstellung('hotkey') or hk.STANDARD)
    feld.halter.pack(side='left')

    def merken(_=None):
        wunsch = feld.get().strip()
        mods, taste = hk.zerlegen(wunsch)
        if not mods:
            fenster.sagen(t('s_hk_falsch'))
            return
        pfade.einstellung_setzen('hotkey', wunsch)
        # ⚠ Sofort ausprobieren, nicht erst beim naechsten Start: „belegt"
        # erfaehrt man sonst zu einem Zeitpunkt, an dem niemand mehr weiss,
        # dass er etwas eingestellt hat.
        from . import overlay as ov
        wache = getattr(ov.OVERLAY_STEUERUNG[0], 'hotkey', None)
        if wache is None:
            fenster.sagen(t('e_neustart_noetig'))
            return
        ok, warum = wache.anmelden(wunsch)
        # ⚠ Getrennte Zweige statt eines Ausdrucks: Pruefung 10 liest, was in
        # `sagen()` steht, und haelt einen Vergleichswert sonst fuer einen
        # sichtbaren Text. Sie hat recht, so herum ist es ohnehin lesbarer.
        if ok:
            fenster.sagen(t('s_hk_ok', wunsch))
        elif warum == 'belegt':
            fenster.sagen(t('s_hk_belegt', wunsch))
        else:
            fenster.sagen(t('s_hk_falsch'))

    feld.bind('<Return>', merken)
    _knopf(fenster, reihe, t('s_or_uebernehmen'), merken).pack(side='left',
                                                            padx=(8, 0))


def _startbefehl_feld(fenster, innen):
    """Ein eigener Startbefehl für Star Citizen — für alle ohne LUG Helper.

    ⚠ Diese Einstellung gab es schon lange (`spielstarter`), nur **nirgends in
    der Oberfläche**: Sie stand allein in der `einstellungen.json`. Wer über
    Lutris oder Heroic spielt, sah deshalb gar keinen Startknopf und hatte keine
    Möglichkeit, das zu ändern — der Ausweg war vorhanden und unerreichbar.

    Ein Weg, den man nur kennt, wenn man den Quelltext gelesen hat, ist kein Weg.
    """
    from . import pfade

    tk.Label(innen, text=t('s_or_start'), bg=BG, fg=FG, font=fenster.f_fett,
             anchor='w').pack(fill='x', pady=(20, 0))
    _fliesstext(innen, t('s_or_start_h'), fenster.f_klein, fill='x')
    _fliesstext(innen, t('s_or_start_bsp'), fenster.f_klein, farbe=SUB,
                fill='x', pady=(2, 0))

    wert = tk.StringVar(value=pfade.einstellung('spielstarter') or '')

    def uebernehmen():
        text = (wert.get() or '').strip()
        pfade.einstellung_setzen('spielstarter', text)
        fenster.sagen(t('s_or_start_ok') if text else t('s_or_start_weg'))
        # Der Startknopf hängt daran — die Leiste muss ihn neu bewerten.
        try:
            fenster.neu_aufbauen()
        except Exception as ausnahme:
            fehler.merken('seiten.startbefehl.aufbauen', ausnahme)

    reihe = tk.Frame(innen, bg=BG)
    reihe.pack(fill='x', pady=(8, 0))
    from .hauptfenster import rundes_feld
    feld = rundes_feld(reihe, wert, fenster.f_klein, '#0c1017', LINIE, ACCENT, FG)
    feld.halter.pack(side='left', fill='x', expand=True, padx=(0, 8))
    _knopf(fenster, reihe, t('s_or_uebernehmen'), uebernehmen).pack(side='left')


def _menueeintrag_feld(fenster, innen):
    """Startmenü-Eintrag anlegen oder entfernen — nur unter Linux sinnvoll.

    Unter Windows erledigt das der Installer; dort wäre der Punkt nur Ballast.
    """
    from . import verknuepfung
    if not verknuepfung.moeglich():
        return
    ziel = _feld(fenster, innen, t('s_menue'), t('s_menue_h'), breit=True)
    reihe = tk.Frame(ziel, bg=BG)
    reihe.pack()
    stand = tk.Label(reihe, text='', bg=BG, fg=SUB, font=fenster.f_klein)

    def zeigen():
        stand.configure(text=t('s_menue_steht') if verknuepfung.vorhanden() else '')

    def anlegen():
        geklappt, wohin = verknuepfung.anlegen()
        fenster.sagen((t('as_menue_da') % wohin) if geklappt
                      else t('as_menue_nein') % wohin)
        zeigen()

    def weg():
        verknuepfung.entfernen()
        fenster.sagen(t('s_menue_weg_ok'))
        zeigen()

    _knopf(fenster, reihe, t('s_menue_anlegen'), anlegen).pack(side='left')
    _knopf(fenster, reihe, t('s_menue_weg'), weg).pack(side='left', padx=8)
    stand.pack(side='left', padx=(10, 0))
    zeigen()


def _durchklick_moeglich():
    from . import overlay
    try:
        return overlay.durchklickbar_moeglich()
    except Exception:
        return False


def _durchklick_um(fenster):
    """Klicks durchreichen ein- oder ausschalten — und sofort anwenden."""
    from . import overlay, pfade
    neu_wert = not pfade.einstellung_wahrheit('durchklickbar', False)
    pfade.einstellung_setzen('durchklickbar', neu_wert)
    geklappt = True
    wurzel = overlay.OVERLAY_FENSTER[0] if overlay.OVERLAY_FENSTER else None
    if wurzel is not None:
        try:
            geklappt = overlay.durchklickbar_setzen(wurzel, neu_wert)
        except Exception as ausnahme:
            fehler.merken('seiten.durchklick', ausnahme)
            geklappt = False
    if neu_wert and not geklappt:
        fenster.sagen(t('ov_durchklick_geht_nicht'))
        pfade.einstellung_setzen('durchklickbar', False)
        return False
    fenster.sagen(t('s_ov_durch_sagen')
                  % (t('e_an') if neu_wert else t('e_aus')))
    return neu_wert


def _overlay_modus(fenster, wahl, kennung):
    """Zwischen „immer sichtbar" und „nur bei Neuzugang" umstellen."""
    from . import overlay, pfade
    wahl.setzen(kennung)
    pfade.einstellung_setzen('overlay_modus', kennung)
    wurzel = overlay.OVERLAY_FENSTER[0] if overlay.OVERLAY_FENSTER else None
    if wurzel is not None:
        try:
            if kennung == 'popup':
                # Nicht sofort verstecken — sonst ist das Fenster weg, während
                # man noch in den Einstellungen steht. Es verschwindet beim
                # nächsten Mal von selbst.
                pass
            else:
                wurzel.deiconify()
        except Exception as ausnahme:
            fehler.merken('seiten.overlay_modus', ausnahme)
    if kennung == 'popup':
        fenster.sagen(t('s_ov_popup_gleich'))
    else:
        fenster.sagen(t('s_ov_modus_sagen') % t('s_ov_immer'))


def saubere_umgebung():
    """Weiterleitung — die Wahrheit steht in `dateiwahl`.

    ⚠ Sie stand jahrelang hier, weil sie hier zuerst gebraucht wurde. Seit die
    Dateiauswahl ein eigenes Modul hat, gehört sie dorthin: Beide brauchen
    dieselbe Wäsche, und zwei Versionen davon wären eine zu viel. Die
    Weiterleitung bleibt, weil `_ordner_zeigen` und der Spielstart sie hier
    aufrufen.
    """
    from . import pfade as pfade_modul
    return pfade_modul.saubere_umgebung()


def ordner_waehlen(titel, start=None):
    """Weiterleitung — siehe `dateiwahl.ordner_waehlen`."""
    from . import dateiwahl
    return dateiwahl.ordner_waehlen(titel, start)


def _im_pfad(name):
    """Gibt es dieses Programm auf dem Rechner?"""
    import shutil
    return bool(shutil.which(name))


def _ordner_zeigen(pfad):
    """Den Ordner im Dateiverwalter öffnen — auf jedem System anders.

    ⚠ Auch hier die saubere Umgebung: Im AppImage würde `xdg-open` sonst unsere
    mitgelieferten Bibliotheken laden und sofort sterben — der Dateiverwalter
    ginge nicht auf, ohne dass irgendetwas darauf hinweist.
    """
    import subprocess
    if not pfad or not os.path.isdir(pfad):
        return False
    try:
        if sys.platform.startswith('win'):
            os.startfile(pfad)                      # noqa: S606
        elif sys.platform == 'darwin':
            subprocess.Popen(['open', pfad])
        else:
            subprocess.Popen(['xdg-open', pfad], env=saubere_umgebung())
        return True
    except Exception as ausnahme:
        fehler.merken('seiten.ordner_zeigen', ausnahme, pfad)
        return False


def _spiel(fenster, rahmen):
    """Auftragstexte — Textquelle wählen und die Bauplan-Angaben eintragen."""
    from . import pfade
    from .hauptfenster import schiebeschalter
    _ueberschrift(fenster, rahmen, t('hf_spiel'), t('s_sp_lead'))
    innen = _rollflaeche(rahmen)
    e = _einstellungen(fenster)

    # Der Zustandskasten sitzt in einem eigenen Rahmen, damit er nach jeder
    # Aktion neu gefüllt werden kann, ohne die ganze Seite anzufassen.
    kasten = tk.Frame(innen, bg=BG)
    kasten.pack(fill='x')

    def lage_zeigen():
        for kind in kasten.winfo_children():
            kind.destroy()
        try:
            lage = e.inj_lage()
        except Exception as ausnahme:
            fehler.merken('seiten.spiel.lage', ausnahme)
            return
        if not pfade.einstellung_wahrheit('inj_an', True):
            # ⚠ „Ausgeschaltet“ allein ist die halbe Wahrheit. Bleibt etwas in der
            # Datei stehen (Entfernen scheiterte, oder es wurde von Hand
            # abgeschaltet), sieht der Spieler seine Angaben weiter im Spiel —
            # und der Kasten behauptet, es sei nichts da. Genau daran ist
            # am 28.08.2026 gemeldet im Test hängengeblieben.
            if lage['drin']:
                _status(fenster, kasten, 'offen', t('s_sp_aus_rest'),
                        t('s_sp_aus_rest_h'), farbe=SUB)
            else:
                _status(fenster, kasten, 'offen', t('s_sp_aus_hinweis'), '',
                        farbe=SUB)
            return
        if lage['drin']:
            zusatz = []
            if lage['quelle']:
                zusatz.append(t('s_sp_quelle_ist')
                              % t(_QUELLTEXT.get(lage['quelle'], 's_sp_q_or')))
            if lage['stand']:
                zusatz.append(str(lage['stand']))
            _status(fenster, kasten, 'haken', t('s_sp_steht'), ' · '.join(zusatz))
        else:
            _status(fenster, kasten, 'offen', t('s_sp_nichts'), t('s_sp_nichts_h'),
                    farbe=SUB)

    # Damit auch Aktionen im Einstellungsobjekt den Kasten auffrischen.
    e.lage_melder = lage_zeigen
    lage_zeigen()

    # --- Textquelle ----------------------------------------------------------
    ziel = _feld(fenster, innen, t('s_sp_quelle'), t('s_sp_quelle_h'),
                 breit=True)
    wahl = _wahl(fenster, ziel,
                 [('deutsch', t('s_sp_q_de')), ('starstrings', t('s_sp_q_ss')),
                  ('original', t('s_sp_q_or'))],
                 pfade.einstellung('inj_quelle') or '',
                 lambda k: _quelle_waehlen(fenster, e, wahl, k, lage_zeigen))
    wahl.pack()

    ziel = _feld(fenster, innen, t('s_sp_auto'), t('s_sp_auto_h'))

    def inj_auto_um():
        neu_wert = not pfade.einstellung_wahrheit('inj_auto', True)
        pfade.einstellung_setzen('inj_auto', neu_wert)
        fenster.sagen(t('s_sp_auto_sagen')
                      % (t('e_an') if neu_wert else t('e_aus')))
        return neu_wert

    schiebeschalter(ziel, pfade.einstellung_wahrheit('inj_auto', True),
                    inj_auto_um).pack()

    # --- An oder aus ---------------------------------------------------------
    # ⚠ Der Schalter fehlte ganz. Wer auf PTU spielt oder die Textdatei in Ruhe
    # lassen will, hatte keine Möglichkeit außer „Wieder entfernen" — und beim
    # nächsten Start schrieb das Werkzeug wieder hinein.
    ziel = _feld(fenster, innen, t('s_sp_an'), t('s_sp_an_h'))

    def inj_an_um():
        neu_wert = not pfade.einstellung_wahrheit('inj_an', True)
        pfade.einstellung_setzen('inj_an', neu_wert)
        fenster.sagen(t('s_sp_an_sagen')
                      % (t('e_an') if neu_wert else t('e_aus')))
        # ⚠ **Aus heißt weg, an heißt da.** Bis rc83 setzte der Schalter nur die
        # Einstellung: Wer abschaltete, sah seine Angaben weiter im Spiel und
        # musste erst unten „Wieder entfernen“ finden. Der Hinweis darauf stand
        # im Kleingedruckten — und genau das liest niemand.
        #
        # Am 28.08.2026 fiel auf, nachdem er im eigenen Test darauf hereinfiel:
        # „ich schalte es auf aus, also ist es weg.“
        #
        # Gefahrlos, weil verlustfrei: Der Urtext ist gemerkt
        # (`injektion.URTEXT_DATEI`), das Entfernen stellt den Wortlaut auf den
        # Buchstaben genau wieder her, und Einschalten trägt neu ein.
        try:
            from . import injektion as inj_modul
            drin = bool(inj_modul.lage().get('drin'))
            if neu_wert and not drin:
                e._inj_erneuern()
            elif not neu_wert and drin:
                e._inj_entfernen()
        except Exception as ausnahme:
            fehler.merken('seiten.inj_an_um', ausnahme)
        lage_zeigen()
        return neu_wert

    schiebeschalter(ziel, pfade.einstellung_wahrheit('inj_an', True),
                    inj_an_um).pack()

    # --- Angaben am Gegenstand ----------------------------------------------
    # Klasse, Größe und Gütegrad direkt am Namen — bei Raketen der Suchkopf.
    # Abschaltbar, weil es die Gegenstandsnamen im Spiel verändert: Wer das
    # nicht will, soll die Bauplan-Angaben trotzdem behalten können.
    ziel = _feld(fenster, innen, t('s_sp_angaben'), t('s_sp_angaben_h'))

    def angaben_um():
        from . import injektion as inj_modul
        neu_wert = not pfade.einstellung_wahrheit(inj_modul.EINSTELLUNG_ANGABEN,
                                                  True)
        pfade.einstellung_setzen(inj_modul.EINSTELLUNG_ANGABEN, neu_wert)
        fenster.sagen(t('s_sp_angaben_sagen')
                      % (t('e_an') if neu_wert else t('e_aus')))
        # ⚠ **Umlegen muss sofort wirken.** Bis rc83 setzte dieser Schalter nur
        # die Einstellung — die `global.ini` blieb unangetastet, bis jemand unten
        # auf „Jetzt eintragen“ drückte. Wer die Angaben abschaltete, das Spiel
        # neu startete und sie weiter sah, hielt das Werkzeug für kaputt.
        #
        # Verschlimmert durch den Kasten darüber: Der sagt „Änderungen wirken beim
        # nächsten Spielstart“ — also genau das, was hier eben NICHT stimmte.
        # Gemessen am 28.08.2026 (gemessen): Schalter aus, Datei unverändert,
        # 1.217 Angaben standen weiter drin.
        #
        # Dazu: „ein user erwartet das was er liest und sieht, ist es aus
        # angaben weg also muss das auch so sein.“
        #
        # ⚠ Nur wenn wirklich etwas drinsteht und das Schreiben überhaupt
        # eingeschaltet ist. Sonst würde ein Formatschalter ungefragt eine
        # Einfügung anstoßen, die der Nutzer gar nicht wollte — der obere
        # Schalter lässt Vorhandenes mit Absicht stehen (PTU-Fall).
        try:
            if (pfade.einstellung_wahrheit('inj_an', True)
                    and inj_modul.lage().get('drin')):
                e._inj_erneuern()
                lage_zeigen()
        except Exception as ausnahme:
            fehler.merken('seiten.angaben_um', ausnahme)
        return neu_wert

    from . import injektion as _inj
    schiebeschalter(ziel,
                    pfade.einstellung_wahrheit(_inj.EINSTELLUNG_ANGABEN, True),
                    angaben_um).pack()

    ziel = _feld(fenster, innen, t('s_sp_hand'), t('s_sp_hand_h'), breit=True)
    reihe = tk.Frame(ziel, bg=BG)
    reihe.pack()
    _knopf(fenster, reihe, t('s_sp_jetzt'),
           lambda: (e._inj_erneuern(), lage_zeigen()),
           stark=True).pack(side='left')
    _knopf(fenster, reihe, t('s_sp_pruefen'),
           lambda: (e._inj_pruefen(), lage_zeigen())).pack(side='left', padx=8)
    _knopf(fenster, reihe, t('s_sp_weg'),
           lambda: (e._inj_entfernen(), lage_zeigen()),
           gefahr=True).pack(side='left')

    _status(fenster, innen, '!', t('s_sp_warn'), t('s_sp_warn_h'), farbe=GOLD)


# Welche Beschriftung zu welcher Quelle gehört — für den Zustandskasten.
_QUELLTEXT = {'deutsch': 's_sp_q_de', 'starstrings': 's_sp_q_ss',
              'original': 's_sp_q_or'}


def _quelle_waehlen(fenster, e, wahl, kennung, danach):
    """Eine Textquelle einrichten — das dauert, also erst ansagen.

    ⚠ Ohne Ansage sieht es aus, als sei nichts passiert: Das Herunterladen und
    Einsetzen braucht mehrere Sekunden, und in dieser Zeit stand vorher nichts
    im Fenster.
    """
    from . import pfade
    wahl.setzen(kennung)
    # ⚠ Die Wahl wird **vor** dem Einrichten gemerkt. Sie stand vorher dahinter,
    # und wenn das Herunterladen schiefging (kein Netz, Zertifikat, Server weg),
    # blieb die alte Quelle eingetragen — das Feld zeigte die neue, der Rest des
    # Programms rechnete mit der alten. Erst gilt, was gewählt wurde; ob es auch
    # eingerichtet werden konnte, sagt der Kasten darüber.
    pfade.einstellung_setzen('inj_quelle', kennung)
    fenster.sagen(t('s_sp_hole') % t(_QUELLTEXT.get(kennung, 's_sp_q_or')))
    try:
        e._inj_wechseln(kennung)
    except Exception as ausnahme:
        fehler.merken('seiten.spiel.quelle', ausnahme)
        fenster.sagen(t('inj_fehler', ausnahme))
    danach()


def _bestand(fenster, rahmen):
    from . import export, importieren
    _ueberschrift(fenster, rahmen, t('hf_bestand'), t('s_be_lead'))
    innen = _rollflaeche(rahmen)

    anzahl = _zahl_bestand()
    tk.Label(innen, text=t('s_be_aus'), bg=BG, fg=FG,
             font=fenster.f_titel, anchor='w').pack(fill='x', pady=(0, 2))
    _fliesstext(innen, t('s_be_aus_h'), fenster.f_klein,
                fill='x', pady=(0, 12))

    # ⚠ Ein Speichern-Knopf **je Version**, direkt an der Version. Vorher gab es
    # nur einen gemeinsamen Knopf „Einzeln speichern …", und der schrieb immer
    # die Basetool-Version. Wer beim Vorführen scmdb einzeln speichern wollte,
    # suchte vergeblich — es gab den Weg schlicht nicht.
    karte = _karte(innen)
    for art, name, wofuer in (('basetool', 'KRT Profit Basetool',
                               t('s_be_n_bp') % anzahl),
                              ('scmdb', 'scmdb.net', t('s_be_n_bp') % anzahl),
                              ('voll', t('s_be_voll'), t('s_be_voll_h'))):
        z = tk.Frame(karte, bg=FLAECHE)
        z.pack(fill='x', padx=16, pady=5)
        tk.Label(z, text=name, bg=FLAECHE, fg=FG, font=fenster.f_klein,
                 width=26, anchor='w').pack(side='left')
        tk.Label(z, text=wofuer, bg=FLAECHE, fg=SUB,
                 font=fenster.f_klein).pack(side='left')
        # ⚠ `a=art` als Vorgabewert, nicht `art` direkt. Ein Lambda merkt sich
        # die **Variable**, nicht ihren Wert — ohne diese Zeile hätten alle drei
        # Knöpfe am Ende der Schleife auf „voll" gezeigt und dreimal dasselbe
        # gespeichert.
        _knopf(fenster, z, t('s_be_speichern_kurz'),
               lambda a=art: einzeln(a)).pack(side='right')

    reihe = tk.Frame(innen, bg=BG)
    reihe.pack(fill='x', pady=(12, 0))

    def in_ablage():
        try:
            ergebnis = export.ablegen()
            wieviele = ergebnis[1] if isinstance(ergebnis, tuple) else ergebnis
            fenster.sagen(t('s_be_geschrieben') % wieviele)
            _ordner_zeigen(export.ablage_ordner())
        except Exception as ausnahme:
            fehler.merken('seiten.bestand.ablegen', ausnahme)
            fenster.sagen(t('s_be_schiefging'))

    def einzeln(art):
        """Eine einzelne Version speichern — die, an deren Zeile der Knopf steht.

        ⚠ Hier stand `art='basetool'` **fest verdrahtet**, während der Knopf
        „Einzeln speichern …" hieß. Wer scmdb oder die Vollsicherung einzeln
        wollte, bekam wortlos die Basetool-Version; über den Dialog waren die
        anderen beiden gar nicht erreichbar. Gemeldet am
        27.08.2026 („bei einzeln speichern speichert er nur basetool").
        """
        from . import dateiwahl
        ziel = dateiwahl.datei_speichern(
            t('s_be_speichern'), vorschlag=export.vorschlag(art),
            endung='.json', start=export.ablage_ordner())
        if not ziel:
            return
        try:
            export.schreiben(ziel, art=art)
            fenster.sagen(t('s_be_gespeichert') % os.path.basename(ziel))
        except Exception as ausnahme:
            fehler.merken('seiten.bestand.einzeln', ausnahme)

    _knopf(fenster, reihe, t('s_be_alle_drei'), in_ablage,
           stark=True).pack(side='left')
    _knopf(fenster, reihe, t('s_be_ablage'),
           lambda: _ordner_zeigen(export.ablage_ordner())).pack(side='left',
                                                                padx=8)

    # Der Satz nimmt die häufigste Frage vorweg: „Muss ich das jedes Mal von
    # Hand machen?" Nein — seit die Ablage bei jedem neuen Bauplan mitgeschrieben
    # wird, sind die drei Dateien von allein aktuell.
    _fliesstext(innen, t('s_be_fort'), fenster.f_klein, fill='x', pady=(8, 0))

    tk.Label(innen, text=t('s_be_ein'), bg=BG, fg=FG,
             font=fenster.f_titel, anchor='w').pack(fill='x', pady=(28, 2))
    _fliesstext(innen, t('s_be_ein_h'), fenster.f_klein,
                fill='x', pady=(0, 12))

    vorschau_platz = tk.Frame(innen, bg=BG)

    def einlesen():
        from . import dateiwahl
        pfad = dateiwahl.datei_oeffnen(
            t('s_be_ein'),
            muster=(('JSON', '*.json'), (t('alle_dateien'), '*.*')))
        if not pfad:
            return
        art, eintraege = importieren.lesen(pfad)
        for kind in vorschau_platz.winfo_children():
            kind.destroy()
        if not art:
            _status(fenster, vorschau_platz, '!', t('s_be_unbekannt'),
                    t('s_be_unbekannt_h'), farbe=ROT)
            return
        v = importieren.vorschau(eintraege)
        _vorschau_zeigen(fenster, vorschau_platz, art, eintraege, v)

    _knopf(fenster, innen, t('s_be_waehlen'), einlesen,
           stark=True).pack(anchor='w')
    _fliesstext(innen, t('s_be_erkannt'), fenster.f_klein,
                fill='x', pady=(10, 0))
    vorschau_platz.pack(fill='x', pady=(14, 20))
    # Der Kasten steht von Anfang an da — sonst wirkt die Seite unfertig, und
    # niemand weiß, dass vor dem Übernehmen noch eine Vorschau kommt.
    _leere_vorschau(fenster, vorschau_platz)

    # ⚠ **Protokolle erneut einlesen.** Steht hier unten und nicht oben: Es ist
    # kein Weg, den man täglich geht, sondern einer für den Fall, dass etwas
    # fehlt. Denselben Knopf gibt es am Overlay — dort ist er näher an dem
    # Moment, in dem jemand merkt, dass ein Bauplan nicht angekommen ist.
    #
    # Die Arbeit macht der Watcher-Faden (`overlay.neu_einlesen_anstossen`),
    # nicht diese Seite: Der Bestand wird an genau einer Stelle geschrieben,
    # sonst überschreibt der Faden das Ergebnis beim nächsten Fund.
    ziel = _feld(fenster, innen, t('s_be_neu'), t('s_be_neu_h'))

    def neu_einlesen():
        from . import overlay as ov
        fenster.sagen(t('s_be_neu_los') if ov.neu_einlesen_anstossen()
                      else t('s_be_neu_kein'))

    # ⚠⚠ **Nicht rot — der Knopf kann nichts kaputt machen.** Bis v3.5.1 war er
    # es, weil er „etwas anrichtet": Er stösst einen Lauf über hunderte
    # Protokolle an. Nachgesehen tut er aber nur eines — `bestand.hinzufuegen`,
    # und das **legt an**. Es nimmt nichts weg, überschreibt nichts, und
    # doppelt kann nichts werden. Der schlimmste Fall ist „dauert kurz".
    #
    # ⚠⚠ **Zwei Bedeutungen für dieselbe Farbe heissen: die Farbe warnt nicht
    # mehr.** Direkt darunter steht „Bestand zurücksetzen" — das loescht
    # wirklich. Waren beide rot, sagte Rot nur noch „irgendwas Wichtiges".
    # Am 31.08.2026 genau so passiert: Haldjas drueckte den harmlosen, und es
    # brauchte einen Zuruf „nicht druecken, der ist nicht ohne Grund rot" —
    # bei einem Knopf, der gar nichts anrichten kann.
    #
    # Rot bleibt fuer das, was weg ist, wenn man es drueckt.
    _knopf(fenster, ziel, t('s_be_neu'), neu_einlesen).pack()

    # ⚠ **Bestand zurücksetzen — hier und nicht unter „Fehler melden".** Dort
    # stand es bis rc42, und dort sucht es niemand: Wer seinen Bauplan-Stand
    # neu aufbauen will, geht auf die Seite, die seinen Bauplan-Stand verwaltet.
    #
    # Der Platz direkt unter „Protokolle erneut einlesen" ist Absicht — die
    # beiden gehören zusammen und der Unterschied wird erst nebeneinander
    # sichtbar: Einlesen **ergänzt**, was fehlt. Zurücksetzen **wirft weg** und
    # baut neu auf. Wer das falsche nimmt, verliert seinen Stand; getrennt auf
    # zwei Seiten sieht man diesen Unterschied nie.
    ziel = _feld(fenster, innen, t('s_be_reset'), t('s_be_reset_h'))

    def zuruecksetzen():
        from .hauptfenster import frage_stellen
        if not frage_stellen(fenster.root, t('s_be_reset'),
                             t('s_be_reset_frage')):
            return
        # ⚠⚠ **Jeder Ausgang sagt etwas.** Ein Knopf, der nach der
        # Warnfrage schweigt, ist von einem kaputten nicht zu unterscheiden.
        # Was „geschafft" heisst, entscheidet `bestand.zuruecksetzen()` — dort
        # steht auch, warum „war schon weg" dazugehoert.
        stoerung = bestand_datei.zuruecksetzen()
        if stoerung is not None:
            fehler.merken('seiten.bestand.zuruecksetzen', stoerung)
            fenster.sagen(t('s_be_reset_fehler', stoerung))
            return
        fenster.sagen(t('s_be_reset_ok'))

    _knopf(fenster, ziel, t('s_zuruecksetzen'), zuruecksetzen, gefahr=True).pack()

    _status(fenster, innen, '!', t('s_be_reset_warn'), t('s_be_reset_warn_h'),
            farbe=GOLD)


def _leere_vorschau(fenster, eltern):
    """Der Vorschau-Kasten, bevor eine Datei gewählt wurde."""
    innen = _karte(eltern, rand=SUB)
    tk.Label(innen, text=t('s_vorschau_leer'), bg=FLAECHE, fg=FG,
             font=fenster.f_fett, anchor='w').pack(fill='x', padx=16,
                                                   pady=(12, 2))
    _fliesstext(innen, t('s_vorschau_leer_h'), fenster.f_klein,
                grund=FLAECHE, abzug=32, fill='x', padx=16, pady=(0, 12))
    return innen


def _vorschau_zeigen(fenster, eltern, art, eintraege, v):
    """Was der Import täte — erst nach dem Knopf passiert wirklich etwas."""
    from . import importieren
    from .hauptfenster import marke as blase
    innen = _karte(eltern, rand=ACCENT)

    kopf = tk.Frame(innen, bg=FLAECHE)
    kopf.pack(fill='x', padx=16, pady=(12, 10))
    tk.Label(kopf, text=t('s_be_vorschau'), bg=FLAECHE,
             fg=FG, font=fenster.f_fett).pack(side='left')
    blase(kopf, {'eigen': t('s_be_eigen'), 'basetool': 'KRT Profit Basetool',
                 'scmdb': 'scmdb.net',
                 'launcher': 'SC Deutsch Launcher'}.get(art, art),
          ACCENT, fenster.f_klein).pack(side='right')

    zahlen = tk.Frame(innen, bg=FLAECHE)
    zahlen.pack(fill='x', padx=16, pady=(0, 10))
    for wert, wofuer, farbe in ((len(v['neu']), t('s_be_dazu'), ACCENT),
                                (len(v['schon_da']), t('s_be_schon'), FG),
                                (len(v['unbekannt']), t('s_be_nicht_kat'), GOLD)):
        s = tk.Frame(zahlen, bg=FLAECHE)
        s.pack(side='left', padx=(0, 30))
        tk.Label(s, text=str(wert), bg=FLAECHE, fg=farbe,
                 font=fenster.f_titel).pack(anchor='w')
        tk.Label(s, text=wofuer, bg=FLAECHE, fg=SUB,
                 font=fenster.f_klein).pack(anchor='w')

    if v['unbekannt']:
        tk.Label(innen, text=t('s_be_nicht_kat_h')
                             + ' · '.join(v['unbekannt'][:6])
                             + (' …' if len(v['unbekannt']) > 6 else ''),
                 bg=FLAECHE, fg=SUB, font=fenster.f_klein, anchor='w',
                 justify='left', wraplength=560).pack(fill='x', padx=16,
                                                      pady=(0, 8))

    _fliesstext(innen, t('s_be_merge'), fenster.f_klein,
                grund=FLAECHE, abzug=32, fill='x', padx=16, pady=(0, 10))

    reihe = tk.Frame(innen, bg=FLAECHE)
    reihe.pack(fill='x', padx=16, pady=(0, 14))

    def uebernehmen():
        dazu = importieren.uebernehmen(eintraege)
        fenster.sagen(t('s_be_genommen') % dazu)
        innen.halter.destroy()

    k = _knopf(fenster, reihe, t('s_be_nimm') % len(v['neu']),
               uebernehmen, stark=True)
    k.configure(bg=FLAECHE)
    k.pack(side='left')
    k2 = _knopf(fenster, reihe, t('abbrechen'), innen.halter.destroy)
    k2.configure(bg=FLAECHE)
    k2.pack(side='left', padx=8)


def _auftragslog(fenster, rahmen):
    """Welche Aufträge wann gespielt wurden — die Rückschau.

    ⚠ **Bewusst eine Liste und sonst nichts.** Keine Auswertung, keine
    Belohnungen, keine Kategorie — das steht schlicht nicht in den Protokollen
    des Spiels. Was drinsteht, ist: welcher Auftrag, wann, wie oft.

    Zwei Zeilenformen, wie beim Bauplan-Bestand:

    * **Läuft noch** → mit Stand, wenn er sich eindeutig zuordnen lässt.
    * **Beendet** → eine Zeile, mehr braucht es nicht.
    """
    from . import missionslog

    _ueberschrift(fenster, rahmen, t('hf_auftragslog'), t('s_al_lead'))
    innen = _rollflaeche(rahmen)
    _fliesstext(innen, t('s_al_hinweis'), fenster.f_klein, fill='x')

    # ⚠⚠ **Die Daten werden bei JEDEM Zeigen neu geholt, nicht nur beim Bauen.**
    # Die Nachlese der alten Protokolle läuft kurz nach dem Start in einem
    # eigenen Faden. Wer die Seite in dieser Sekunde öffnet, sah bis
    # 04.09.2026 „Noch kein Auftrag aufgezeichnet" — und danach nie wieder
    # etwas anderes, weil die Seite gebaut blieb. Genau so gemeldet, mit acht
    # Aufträgen in der Datei.
    daten = {'alle': []}

    suche = tk.StringVar()
    liste_rahmen = tk.Frame(innen, bg=BG)

    block = tk.Frame(innen, bg=BG)
    block.pack(fill='x', padx=24, pady=(14, 0))
    tk.Label(block, text=t('s_al_suche'), bg=BG, fg=FG, font=fenster.f_fett,
             anchor='w').pack(fill='x')
    from .hauptfenster import rundes_feld
    feld = rundes_feld(block, suche, fenster.f_klein, '#0c1017', LINIE,
                       ACCENT, FG)
    feld.halter.pack(fill='x', pady=(4, 0))

    kopf = tk.Label(innen, text='', bg=BG, fg=SUB, font=fenster.f_klein,
                    anchor='w')
    kopf.pack(fill='x', padx=24, pady=(12, 0))
    liste_rahmen.pack(fill='both', expand=True, padx=24, pady=(4, 12))

    farben = {missionslog.ABGESCHLOSSEN: ACCENT,
              missionslog.ABGEBROCHEN: SUB,
              missionslog.VERFALLEN: SUB,
              missionslog.LAEUFT: GOLD}
    worte = {missionslog.ABGESCHLOSSEN: 's_al_fertig',
             missionslog.ABGEBROCHEN: 's_al_abbruch',
             # Zurueckhaltend in Grau: Es ist keine Leistung und kein Abbruch,
             # nur das Ende der Spur.
             missionslog.VERFALLEN: 's_al_verfallen',
             missionslog.LAEUFT: 's_al_laeuft'}

    def zeichnen(*_, neu_laden=False):
        if neu_laden or not daten['alle']:
            try:
                daten['alle'] = missionslog.laden()
            except Exception:
                daten['alle'] = []
        alle = daten['alle']
        for kind in liste_rahmen.winfo_children():
            kind.destroy()
        if not alle:
            # Noch gar nichts aufgezeichnet — das ist etwas anderes als „die
            # Suche findet nichts" und braucht deshalb einen eigenen Satz.
            kopf.configure(text='')
            tk.Label(liste_rahmen, text=t('s_al_leer'), bg=BG, fg=SUB,
                     font=fenster.f_klein, anchor='w', justify='left',
                     wraplength=560).pack(fill='x', pady=8)
            return
        treffer = missionslog.suchen(alle, suche.get())
        kopf.configure(text=t('s_al_anzahl', len(treffer)))
        if not treffer:
            tk.Label(liste_rahmen, text=t('s_al_nichts'), bg=BG, fg=SUB,
                     font=fenster.f_klein, anchor='w').pack(fill='x', pady=8)
            return
        # ⚠ Nicht alles auf einmal: Wer hundert Auftraege gespielt hat, wartet
        # sonst beim Oeffnen. Dieselbe Grenze wie in der Bauplan-Liste.
        for eintrag in treffer[:200]:
            zustand = eintrag.get('zustand') or missionslog.LAEUFT
            zeile = tk.Frame(liste_rahmen, bg=FLAECHE)
            zeile.pack(fill='x', pady=1)

            tk.Label(zeile, text=(eintrag.get('wann') or '')[:10],
                     bg=FLAECHE, fg=SUB, font=fenster.f_klein, width=11,
                     anchor='w', padx=10, pady=7).pack(side='left')
            # ⚠ Breit genug fuer den laengsten Zustand — „nicht mehr offen"
            # hat 16 Zeichen, bei 14 stand dort ein Stumpf.
            tk.Label(zeile, text=t(worte.get(zustand, 's_al_laeuft')),
                     bg=FLAECHE, fg=farben.get(zustand, SUB),
                     font=fenster.f_klein, width=17,
                     anchor='w').pack(side='left')

            mitte = tk.Frame(zeile, bg=FLAECHE)
            mitte.pack(side='left', fill='x', expand=True)
            # ⚠ Lange Namen brechen um, statt rechts abgeschnitten zu werden.
            # Das Spiel liefert bis zu 109 Zeichen („Verified Bounty: … | HRT
            # (Großes Mehrbesatzungsschiff, mittlere Unterstützung)").
            name_lab = tk.Label(mitte, text=eintrag.get('name') or '',
                                bg=FLAECHE, fg=FG, font=fenster.f_klein,
                                anchor='w', justify='left')
            name_lab.pack(fill='x')
            _umbruch(name_lab)
            # Der Stand gehoert nur an einen laufenden Auftrag. Bei einem
            # beendeten waere er Ballast — er ist ja fertig.
            if (zustand == missionslog.LAEUFT
                    and eintrag.get('ziele_gesamt')):
                tk.Label(mitte,
                         text=t('s_al_ziele', eintrag.get('ziele_fertig') or 0,
                                eintrag['ziele_gesamt']),
                         bg=FLAECHE, fg=SUB, font=fenster.f_klein,
                         anchor='w').pack(fill='x')

            # ⭐ Was dabei herauskam. Das ist der Grund, warum jemand nach einem
            # Auftrag sucht — „welcher war das nochmal, bei dem der Helm kam?".
            # In der Markenfarbe, damit es beim Ueberfliegen auffaellt.
            bps = eintrag.get('bauplaene') or []
            if bps:
                bp_lab = tk.Label(
                    mitte,
                    text=t('s_al_bp' if len(bps) == 1 else 's_al_bp_mehr',
                           ' · '.join(bps)),
                    bg=FLAECHE, fg=ACCENT, font=fenster.f_klein,
                    anchor='w', justify='left')
                bp_lab.pack(fill='x')
                # Bei mehreren Funden wird diese Zeile laenger als der Name.
                _umbruch(bp_lab)

        # Wie oft welcher Auftrag lief — die Antwort auf „mache ich den
        # ständig?". Steht mit in `zeichnen()`, damit es beim Auffrischen
        # mitwächst statt einen alten Stand zu zeigen.
        zaehler = missionslog.zusammenfassen(alle)
        if len(zaehler) < len(alle):    # nur wenn sich etwas wiederholt hat
            tk.Label(liste_rahmen, text=t('s_al_oft_kopf'), bg=BG, fg=FG,
                     font=fenster.f_fett, anchor='w').pack(fill='x',
                                                           pady=(16, 4))
            for name, (gesamt, fertig) in sorted(zaehler.items(),
                                                 key=lambda x: -x[1][0]):
                if gesamt < 2:
                    continue
                reihe = tk.Frame(liste_rahmen, bg=BG)
                reihe.pack(fill='x')
                tk.Label(reihe, text=name, bg=BG, fg=FG, font=fenster.f_klein,
                         anchor='w').pack(side='left')
                tk.Label(reihe, text='  ' + t('s_al_oft', gesamt, fertig),
                         bg=BG, fg=SUB, font=fenster.f_klein,
                         anchor='w').pack(side='left')

    suche.trace_add('write', zeichnen)
    zeichnen()

    def _auffrischen():
        """Erst die Logs nachlesen, dann anzeigen.

        ⚠⚠ **Die Datei allein neu zu laden genügt nicht.** Das Protokoll wurde
        bis zum 04.09.2026 ausschliesslich beim Programmstart gefuellt
        (`_nachlese` im Hauptprogramm). Wer den Watcher morgens startet und
        mittags einen Auftrag abgibt, fand ihn hier nicht — die Seite lud brav
        eine Datei, in der seit dem Start nichts Neues stand. Gemeldet mit
        einem Auftrag, der eine halbe Stunde zuvor beendet worden war.

        ⚠ Das ist billig: Der Lesestand merkt sich Name und Groesse jeder
        Logdatei, also wird nur die laufende `Game.log` erneut gelesen.
        Gemessen an 183 Sicherungen: **20 ms**, wenn sie gewachsen ist, und
        3 ms, wenn sich nichts getan hat.
        """
        try:
            missionslog.nachlese()
        except Exception as ausnahme:
            # Ein fehlgeschlagenes Nachlesen darf die Seite nicht leer lassen —
            # der gespeicherte Stand ist immer noch besser als nichts.
            fehler.merken('seiten.auftragslog_nachlese', ausnahme)
        zeichnen(neu_laden=True)

    # ⚠ Bei jedem Öffnen frisch laden — siehe oben. Ohne das bleibt eine Seite,
    # die während der Nachlese gebaut wurde, für immer leer.
    fenster.beim_zeigen['auftragslog'] = _auffrischen


def _joysticks(fenster, rahmen):
    """Welcher Stick welche Nummer hat — und was darauf liegt.

    Zwei Fragen, zwei Blöcke:

    1. **Stimmt die Zuordnung noch?** Star Citizen hängt Belegungen an einer
       Nummer (`js1`), nicht am Gerät. Ändert sich die Kennung eines Geräts,
       zeigt die alte Belegung ins Leere.
    2. **Was liegt eigentlich drauf?** Die `actionmaps.xml` weiß das für jeden
       Stick — ganz ohne Gerätevorlagen.

    ⚠ **Umgeschrieben wird nur auf Knopfdruck.** Ein Automatismus, der die
    Belegungsdatei des Spielers im Hintergrund anfasst, ist das Letzte, was
    ein Overlay tun sollte — zumal der Fall, in dem er hülfe, selten ist.
    """
    from . import joysticks

    _ueberschrift(fenster, rahmen, t('hf_joysticks'), t('s_js_lead'))
    innen = _rollflaeche(rahmen)
    _fliesstext(innen, t('s_js_hinweis'), fenster.f_klein, fill='x')

    daten = {}
    suche = tk.StringVar()
    nur = {'geraet': '', 'sicht': joysticks.ALLES}

    # ⚠⚠ **Das Suchfeld wird EINMAL gebaut und danach nie wieder angefasst.**
    #
    # Die erste Fassung baute bei jedem Tastendruck die ganze Seite neu — also
    # auch das Feld, in das der Spieler gerade tippte. Ergebnis: Nach jedem
    # Buchstaben war der Eingabezeiger weg und man musste neu hineinklicken.
    # Genau so gemeldet, und es ist im Projekt nicht das erste Mal passiert.
    #
    # Die Aufteilung dagegen:
    #
    # | Rahmen | Wird neu gebaut |
    # |---|---|
    # | `oben` (Zustand, Geräteliste) | nur bei `_auffrischen()` |
    # | `werkzeug` (Suchfeld) | **nie** — es hält den Eingabezeiger |
    # | `filter_rahmen` (Geräteknöpfe) | bei `_auffrischen()` |
    # | `liste_rahmen` (die Treffer) | bei jedem Tastendruck, nur die Kinder |
    #
    # **Regel für jede weitere Seite mit Suchfeld:** Was Eingaben entgegennimmt,
    # steht ausserhalb dessen, was die Suche neu zeichnet.
    oben = tk.Frame(innen, bg=BG)
    oben.pack(fill='x', padx=24, pady=(14, 0))
    unten = tk.Frame(innen, bg=BG)
    unten.pack(fill='both', expand=True, padx=24, pady=(4, 12))

    kopfzeile = tk.Label(unten, text=t('s_js_belegt'), bg=BG, fg=FG,
                         font=fenster.f_fett, anchor='w')
    werkzeugleiste = tk.Frame(unten, bg=BG)
    sicht_rahmen = tk.Frame(unten, bg=BG)
    filter_rahmen = tk.Frame(unten, bg=BG)
    werkzeug = tk.Frame(unten, bg=BG)
    zaehler = tk.Label(unten, text='', bg=BG, fg=SUB, font=fenster.f_klein,
                       anchor='w')
    liste_rahmen = tk.Frame(unten, bg=BG)

    from .hauptfenster import rundes_feld
    feld = rundes_feld(werkzeug, suche, fenster.f_klein, '#0c1017', LINIE,
                       ACCENT, FG)
    feld.halter.pack(fill='x')

    def _kennung_kurz(k):
        """Nur der vordere, unterscheidende Teil der Kennung.

        Der Rest (`-0000-0000-0000-504944564944`) ist bei allen Geräten
        gleich — er sagt nur „das ist ein DirectInput-Gerät" und macht die
        Zeile doppelt so lang.
        """
        return (k or '').split('-')[0]

    def _geraetename(kennzeichen, lang=False):
        """Der Name, den ein Mensch versteht — nicht `js1`.

        ⚠⚠ **`js1` sagt niemandem etwas.** Es ist die Nummer, unter der das
        Spiel das Gerät führt, und sie steht so auch in der Belegungsdatei —
        aber wer vor der Liste sitzt, will wissen, ob das der linke oder der
        rechte Stick ist. Deshalb steht hier der **Produktname**, so wie ihn
        das Spiel selbst kennt; die Nummer wandert in Klammern dahinter, wo
        sie beim Vergleich mit anderen Werkzeugen noch nützlich ist.

        Ist das Gerät unbekannt (kommt in keiner Belegung vor), bleibt die
        Nummer stehen — falsch raten wäre schlechter als technisch wirken.
        """
        # In der Sicht „noch nicht belegt" gibt es kein Gerät — die Aktion
        # gehört noch zu keinem. Ein Strich sagt das; „frei" sähe aus wie ein
        # Gerätename.
        if kennzeichen == joysticks.FREI:
            return t('s_js_ohne_eingabe')
        art = joysticks.art_von(kennzeichen)
        if art in ('tastatur', 'maus', 'gamepad'):
            return t('s_js_a_' + art)
        name = (daten.get('geraetenamen') or {}).get(kennzeichen, '')
        if not name:
            return kennzeichen
        if lang:
            return '%s (%s)' % (name, kennzeichen)
        # In der Liste ist die Spalte schmal; der Anfang des Namens trägt die
        # Unterscheidung („LEFT …" / „RIGHT …").
        return name if len(name) <= 20 else (name[:19] + '…')

    def kopf_zeichnen():
        """Zustand und Geräteliste — nur beim Auffrischen, nicht beim Tippen."""
        for kind in oben.winfo_children():
            kind.destroy()
        v = daten.get('vergleich') or {}
        geraete = v.get('geraete') or []
        belegt = v.get('zuordnung') or []
        zustand = v.get('zustand') or joysticks.LEER

        if not geraete:
            tk.Label(oben, text=t('s_js_leer'), bg=BG, fg=SUB,
                     font=fenster.f_klein, anchor='w', justify='left',
                     wraplength=560).pack(fill='x', pady=8)
            return

        # --- Zustandszeile: die eine Aussage, wegen der man hier nachsieht ---
        farbe = {joysticks.PASST: ACCENT, joysticks.ERSETZT: GOLD,
                 joysticks.FEHLT: ROT}.get(zustand, SUB)
        if zustand == joysticks.PASST:
            satz = t('s_js_passt')
        elif zustand == joysticks.ERSETZT:
            satz = t('s_js_ersetzt')
        elif zustand == joysticks.FEHLT:
            satz = t('s_js_fehlt', len(v.get('fehlende') or []))
        else:
            satz = t('s_js_keine_datei')
        tk.Label(oben, text=satz, bg=BG, fg=farbe, font=fenster.f_fett,
                 anchor='w', justify='left', wraplength=560).pack(fill='x')

        # --- Der eine reparierbare Fall: Gerät unter neuer Kennung ---
        if zustand == joysticks.ERSETZT and v.get('ersatz'):
            alt, neu = v['ersatz'][0]
            tk.Label(oben, text=t('s_js_ersatz_frage', alt['name'],
                                  neu['name']),
                     bg=BG, fg=SUB, font=fenster.f_klein, anchor='w',
                     justify='left', wraplength=560).pack(fill='x', pady=(6, 0))
            tk.Label(oben, text=t('s_js_spiel_zu'), bg=BG, fg=GOLD,
                     font=fenster.f_klein, anchor='w', justify='left',
                     wraplength=560).pack(fill='x', pady=(4, 0))
            _knopf(fenster, oben, t('s_js_uebernehmen'),
                   lambda: _uebernehmen(alt, neu), stark=True).pack(
                       anchor='w', pady=(8, 0))

        # --- Block 1: was verbunden ist ---
        tk.Label(oben, text=t('s_js_geraete'), bg=BG, fg=FG,
                 font=fenster.f_fett, anchor='w').pack(fill='x', pady=(16, 4))
        nach_kennung = {z['kennung']: z for z in belegt if z['kennung']}
        for g in geraete:
            zeile = tk.Frame(oben, bg=FLAECHE)
            zeile.pack(fill='x', pady=1)
            tk.Label(zeile, text=t('s_js_platz', g['platz']), bg=FLAECHE,
                     fg=SUB, font=fenster.f_klein, width=9, anchor='w',
                     padx=10, pady=7).pack(side='left')
            zu = nach_kennung.get(g['kennung'])
            tk.Label(zeile, text=('js%d' % zu['nummer']) if zu
                     else t('s_js_ohne'),
                     bg=FLAECHE, fg=ACCENT if zu else GOLD,
                     font=fenster.f_klein, width=13, anchor='w').pack(
                         side='left')
            tk.Label(zeile, text=g['name'], bg=FLAECHE, fg=FG,
                     font=fenster.f_klein, anchor='w').pack(side='left')
            tk.Label(zeile, text=_kennung_kurz(g['kennung']), bg=FLAECHE,
                     fg=SUB, font=fenster.f_klein, anchor='e',
                     padx=10).pack(side='right')

        # Belegte Geräte, die gerade fehlen — sonst sieht man nur, was da ist.
        for z in (v.get('fehlende') or []):
            zeile = tk.Frame(oben, bg=FLAECHE)
            zeile.pack(fill='x', pady=1)
            tk.Label(zeile, text='—', bg=FLAECHE, fg=ROT,
                     font=fenster.f_klein, width=9, anchor='w', padx=10,
                     pady=7).pack(side='left')
            tk.Label(zeile, text='js%d' % z['nummer'], bg=FLAECHE, fg=ROT,
                     font=fenster.f_klein, width=13, anchor='w').pack(
                         side='left')
            tk.Label(zeile, text=z['name'], bg=FLAECHE, fg=SUB,
                     font=fenster.f_klein, anchor='w').pack(side='left')

    def _zuruecksetzen():
        """Alle eigenen Belegungen verwerfen — mit ausdrücklicher Rückfrage.

        ⚠⚠ Der gefährlichste Knopf auf der Seite: Er wirft weg, woran jemand
        einen Abend gesessen hat. Deshalb nennt die Frage die **Anzahl** der
        betroffenen Belegungen — „alles zurücksetzen?" ist zu abstrakt, um
        eine Entscheidung darauf zu stützen.
        """
        from tkinter import messagebox
        eigene = 0
        for liste in (joysticks.sicht(joysticks.MEINE) or {}).values():
            eigene += len(liste)
        if not messagebox.askyesno(t('s_js_zurueck'),
                                   t('s_js_zurueck_frage', eigene),
                                   icon='warning', default='no'):
            return
        erfolg, meldung, _ = joysticks.zuruecksetzen()
        if erfolg:
            messagebox.showinfo(t('hf_joysticks'),
                                t('s_js_zurueck_ok', meldung))
        else:
            messagebox.showwarning(t('hf_joysticks'),
                                   t('s_js_schief', t(meldung)))
        _auffrischen()

    def _ausgeben(als_csv=False):
        """Die Belegung als Datei sichern — ohne Umweg über die Spielkonsole."""
        from tkinter import messagebox
        from . import dateiwahl
        from .sprache import aktuelle
        endung = '.csv' if als_csv else '.xml'
        ziel = dateiwahl.datei_speichern(
            t('s_js_ausgeben'),
            vorschlag='actionmaps' + endung, endung=endung)
        if not ziel:
            return
        erfolg, meldung = joysticks.ausgeben(ziel, aktuelle())
        if erfolg:
            messagebox.showinfo(t('hf_joysticks'),
                                t('s_js_ausgabe_ok', meldung))
        else:
            messagebox.showwarning(t('hf_joysticks'),
                                   t('s_js_schief', t(meldung)))

    def _einlesen():
        from tkinter import messagebox
        from . import dateiwahl
        quelle = dateiwahl.datei_oeffnen(t('s_js_einlesen'),
                                         muster=(('XML', '*.xml'),))
        if not quelle:
            return
        if not messagebox.askyesno(t('s_js_einlesen'),
                                   t('s_js_einlesen_frage'),
                                   icon='warning', default='no'):
            return
        erfolg, meldung, anzahl = joysticks.einlesen(quelle)
        if erfolg:
            messagebox.showinfo(t('hf_joysticks'),
                                t('s_js_einlesen_ok', anzahl, meldung))
        else:
            messagebox.showwarning(t('hf_joysticks'),
                                   t('s_js_schief', t(meldung)))
        _auffrischen()

    def werkzeug_zeichnen():
        """Sichern, Einspielen, Zurücksetzen — einmal gebaut, bleibt stehen."""
        for kind in werkzeugleiste.winfo_children():
            kind.destroy()
        _knopf(fenster, werkzeugleiste, t('s_js_ausgeben'),
               lambda: _ausgeben(False)).pack(side='left', padx=(0, 6))
        _knopf(fenster, werkzeugleiste, t('s_js_ausgeben_csv'),
               lambda: _ausgeben(True)).pack(side='left', padx=(0, 6))
        _knopf(fenster, werkzeugleiste, t('s_js_einlesen'),
               _einlesen).pack(side='left', padx=(0, 6))
        # ⚠ `gefahr=True` färbt dauerhaft rot, nicht erst beim Überfahren —
        # ein Knopf, der erst warnt, wenn die Maus schon darauf steht, warnt
        # niemanden.
        _knopf(fenster, werkzeugleiste, t('s_js_zurueck'), _zuruecksetzen,
               gefahr=True).pack(side='right')

    def sicht_zeichnen():
        """Die drei Sichten: was ich geändert habe · alles · Werkseinstellung.

        Dieselbe Einteilung, die das Spiel in seinen Optionen benutzt — und
        die Antwort auf zwei verschiedene Fragen: „was habe ich umgestellt"
        und „was tut diese Taste eigentlich".
        """
        for kind in sicht_rahmen.winfo_children():
            kind.destroy()
        tk.Label(sicht_rahmen, text=t('s_js_sicht'), bg=BG, fg=SUB,
                 font=fenster.f_klein, anchor='w').pack(side='left',
                                                        padx=(0, 8))

        def _waehlen(welche):
            nur['sicht'] = welche
            nur['geraet'] = ''         # die Geräte unterscheiden sich je Sicht
            _laden()
            sicht_zeichnen()
            filter_zeichnen()
            liste_zeichnen()

        for schluessel, welche in ((('s_js_s_meine'), joysticks.MEINE),
                                   ('s_js_s_alles', joysticks.ALLES),
                                   ('s_js_s_standard', joysticks.STANDARD),
                                   # ⭐ Ohne diese Sicht käme man an 411
                                   # Aktionen gar nicht heran: Was nirgends
                                   # belegt ist, steht in keiner Liste — und
                                   # was in keiner Liste steht, kann man auch
                                   # nicht anklicken, um es zu belegen.
                                   ('s_js_s_frei', joysticks.FREI)):
            _knopf(fenster, sicht_rahmen, t(schluessel),
                   (lambda w=welche: _waehlen(w)),
                   stark=(nur['sicht'] == welche)).pack(side='left',
                                                        padx=(0, 6))

    def filter_zeichnen():
        """Die Geräteknöpfe — ein Knopf je Gerät plus „Alle".

        Kein Aufklappmenü: Es sind selten mehr als fünf Geräte, und ein Klick
        ist weniger als zwei. Wird beim Auffrischen neu bestückt, weil ein
        Gerät dazukommen oder wegfallen kann — das Suchfeld bleibt davon
        unberührt, es liegt in einem eigenen Rahmen.
        """
        for kind in filter_rahmen.winfo_children():
            kind.destroy()
        alle = daten.get('belegungen') or {}

        def _waehlen(kennzeichen):
            nur['geraet'] = kennzeichen
            filter_zeichnen()
            liste_zeichnen()

        knoepfe = [(t('s_js_alle'), '')]
        # Nach Art gruppiert, damit Tastatur und Maus nicht zwischen den
        # Sticks stehen — die Reihenfolge kommt aus `joysticks.ARTEN`.
        for art, kuerzel in joysticks.ARTEN:
            for kennzeichen in sorted(k for k in alle
                                      if k.startswith(kuerzel)):
                knoepfe.append((_geraetename(kennzeichen), kennzeichen))
        for text, kennzeichen in knoepfe:
            _knopf(fenster, filter_rahmen, text,
                   (lambda k=kennzeichen: _waehlen(k)),
                   stark=(nur['geraet'] == kennzeichen)).pack(side='left',
                                                              padx=(0, 6))

    def liste_zeichnen(*_):
        """Die Trefferliste — das Einzige, was beim Tippen neu entsteht."""
        for kind in liste_rahmen.winfo_children():
            kind.destroy()
        alle = daten.get('belegungen') or {}
        if not alle:
            zaehler.configure(text='')
            return

        begriff = (suche.get() or '').strip().lower()
        namen = daten.get('namen') or {}
        gezeigt = []
        gesehen = set()
        arten = list(joysticks.ARTEN) + [('frei', joysticks.FREI)]
        for _art, kuerzel in arten:
            for kennzeichen in sorted(k for k in alle
                                      if k.startswith(kuerzel)):
                if nur['geraet'] and kennzeichen != nur['geraet']:
                    continue
                for e in alle[kennzeichen]:
                    klar, hinweis, echt = (namen.get(e['aktion'])
                                           or ('', '', False))
                    lesbar = joysticks.eingabe_lesbar(e['eingabe'],
                                                      e.get('art', ''))
                    # ⚠ Dieselbe Aktion steht in mehreren Gruppen der
                    # Spieldatei. Ohne Entdoppelung erschien sie doppelt in
                    # der Liste — mit identischer Zeile, was wie ein Fehler
                    # aussieht.
                    marke = (kennzeichen, e['eingabe'], e['aktion'])
                    if marke in gesehen:
                        continue
                    gesehen.add(marke)
                    # Gesucht wird über **alles**, was der Spieler sehen
                    # kann — den lesbaren Namen zuerst. Wer „Aussteigen"
                    # tippt, denkt nicht an `v_eject`; wer aus einer Anleitung
                    # `v_eject` kopiert, soll es trotzdem finden.
                    if begriff and begriff not in ('%s %s %s %s %s %s' % (
                            lesbar, e['eingabe'], klar, hinweis, e['aktion'],
                            e['bereich'])).lower():
                        continue
                    gezeigt.append((kennzeichen, e, klar, lesbar, echt))

        zaehler.configure(text='%s   ·   %s' % (
            t('s_js_bindungen', len(gezeigt)),
            t('s_js_frei_hinweis') if nur['sicht'] == joysticks.FREI
            else t('s_js_b_hinweis')))
        if not gezeigt:
            tk.Label(liste_rahmen, text=t('s_js_nichts'), bg=BG, fg=SUB,
                     font=fenster.f_klein, anchor='w').pack(fill='x', pady=8)
            return
        # ⚠ Dieselbe Grenze wie in der Bauplan-Liste: Wer alle Geräte auf
        # einmal zeigt, hat schnell dreihundert Zeilen und wartet beim Öffnen.
        for kennzeichen, e, klar, lesbar, echt in gezeigt[:200]:
            zeile = tk.Frame(liste_rahmen, bg=FLAECHE)
            zeile.pack(fill='x', pady=1)

            # ⚠ Die Bindung muss auf **jedes** Kind gelegt werden, nicht nur
            # auf den Rahmen: Ein Klick landet auf der Beschriftung, die
            # darüberliegt, und der Rahmen bekommt ihn nie zu sehen.
            def _klick(_ereignis=None, kz=kennzeichen, eintrag=e, name=klar):
                _neu_belegen(kz, eintrag, name)

            def _anfassen(kind):
                kind.bind('<Button-1>', _klick)
                kind.configure(cursor='hand2')

            _anfassen(zeile)
            tk.Label(zeile, text=_geraetename(kennzeichen), bg=FLAECHE,
                     fg=SUB, font=fenster.f_klein, width=21, anchor='w',
                     padx=10, pady=6).pack(side='left')
            tk.Label(zeile, text=(lesbar or t('s_js_ohne_eingabe')),
                     bg=FLAECHE, fg=(ACCENT if lesbar else SUB),
                     font=fenster.f_klein, width=17, anchor='w').pack(
                         side='left')
            # ⚠ Grau heißt „das ist keine Bezeichnung des Spiels, sondern der
            # aufbereitete technische Name" — 382 Aktionen haben keine.
            tk.Label(zeile, text=(klar or e['aktion']), bg=FLAECHE,
                     fg=(FG if echt else SUB), font=fenster.f_klein,
                     anchor='w').pack(side='left')
            # Was der Spieler selbst geändert hat, wird gekennzeichnet —
            # sonst sieht man in der Gesamtsicht nicht, was von einem selbst
            # stammt und was ab Werk so ist.
            if e.get('quelle') == joysticks.MEINE:
                tk.Label(zeile, text=t('s_js_q_meine'), bg=FLAECHE, fg=ACCENT,
                         font=fenster.f_klein, anchor='e', padx=10).pack(
                             side='right')
            for kind in zeile.winfo_children():
                _anfassen(kind)

    def _uebernehmen(alt, neu):
        from tkinter import messagebox
        erfolg, meldung, _ = joysticks.kennung_tauschen(alt['kennung'],
                                                        neu['kennung'],
                                                        neu['name'])
        if erfolg:
            messagebox.showinfo(t('hf_joysticks'), t('s_js_fertig', meldung))
        else:
            # ⚠ `meldung` ist ein Sprachschlüssel, kein fertiger Satz — sonst
            # stünde in der englischen Oberfläche deutscher Text.
            messagebox.showwarning(t('hf_joysticks'),
                                   t('s_js_schief', t(meldung)))
        _auffrischen()

    def _neu_belegen(kennzeichen, eintrag, klar):
        """Das Fenster zum Neubelegen öffnen — Klick auf eine Zeile.

        ⚠ Der `bereich` muss mit: Star Citizen sortiert Aktionen in Gruppen
        (`spaceship_movement`, `player`), und dieselbe Aktion kann in
        mehreren stecken. Ohne die Gruppe landet die Belegung woanders als
        gedacht. Aus der Werkseinstellung kommt sie nicht mit — dann wird die
        Gruppe aus der eigenen Datei gesucht.
        """
        from .belegenfenster import Belegenfenster
        bereich = (eintrag.get('bereich')
                   or joysticks.gruppe_von(eintrag['aktion'])
                   or _bereich_suchen(eintrag['aktion']))
        if kennzeichen == joysticks.FREI:
            # Eine unbelegte Aktion gehört noch zu keinem Gerät — welches es
            # wird, entscheidet der Knopf, den der Spieler gleich drückt.
            kennzeichen = ''
        try:
            Belegenfenster(fenster.root, eintrag['aktion'], bereich,
                           kennzeichen, klarname=klar,
                           bisher=eintrag.get('eingabe', ''),
                           fertig=_auffrischen)
        except Exception as ausnahme:
            fehler.merken('seiten.joysticks_belegen', ausnahme)

    def _bereich_suchen(aktion):
        """Zu welcher Gruppe gehört eine Aktion?

        Die Werkseinstellung nennt die Gruppe nicht mit — sie steht nur in
        der eigenen `actionmaps.xml`. Findet sich dort nichts, bleibt die
        Gruppe leer, und `joysticks.belegen()` legt sie unter der
        gebräuchlichsten an.
        """
        for liste in (joysticks.belegungen() or {}).values():
            for e in liste:
                if e['aktion'] == aktion and e.get('bereich'):
                    return e['bereich']
        return ''

    def _laden():
        """Die Belegungen in der gewählten Sicht holen."""
        try:
            daten['belegungen'] = joysticks.sicht(nur['sicht'])
        except Exception as ausnahme:
            fehler.merken('seiten.joysticks_sicht', ausnahme)
            daten['belegungen'] = {}

    def _auffrischen():
        from .sprache import aktuelle
        try:
            daten['vergleich'] = joysticks.vergleich()
            # Nummer → Produktname, damit in der Liste nicht `js1` steht.
            daten['geraetenamen'] = {
                'js%d' % z['nummer']: z['name']
                for z in (daten['vergleich'].get('zuordnung') or [])
                if z.get('name')}
        except Exception as ausnahme:
            fehler.merken('seiten.joysticks', ausnahme)
            daten['vergleich'] = {}
            daten['geraetenamen'] = {}
        try:
            # ⚠ Die Klarnamen richten sich nach der **Programmsprache**, nicht
            # nach der Spielsprache: Wer den englischen Client fährt, aber die
            # Oberfläche auf Deutsch hat, will deutsche Aktionsnamen.
            joysticks.vergessen()
            daten['namen'] = joysticks.klarnamen(aktuelle())
        except Exception as ausnahme:
            fehler.merken('seiten.joysticks_namen', ausnahme)
            daten['namen'] = {}
        _laden()
        kopf_zeichnen()
        # Die feste Hülle erst zeigen, wenn es wirklich Belegungen gibt —
        # ein Suchfeld über einer leeren Liste ist nur Ballast.
        if daten.get('belegungen') or nur['sicht'] != joysticks.ALLES:
            kopfzeile.pack(fill='x', pady=(16, 4))
            werkzeugleiste.pack(fill='x', pady=(0, 10))
            sicht_rahmen.pack(fill='x', pady=(0, 6))
            filter_rahmen.pack(fill='x', pady=(0, 6))
            werkzeug.pack(fill='x', pady=(2, 6))
            zaehler.pack(fill='x', pady=(0, 4))
            liste_rahmen.pack(fill='both', expand=True)
        else:
            for teil in (kopfzeile, werkzeugleiste, sicht_rahmen,
                         filter_rahmen, werkzeug, zaehler, liste_rahmen):
                teil.pack_forget()
        werkzeug_zeichnen()
        sicht_zeichnen()
        filter_zeichnen()
        liste_zeichnen()

    # ⚠ Hier hängt die Suche NUR an der Liste, nicht am Seitenaufbau — sonst
    # verschwände mit jedem Buchstaben das Feld, in das getippt wird.
    suche.trace_add('write', liste_zeichnen)
    _auffrischen()
    # ⚠ Bei jedem Öffnen frisch: Zwischen zwei Besuchen kann ein Gerät
    # abgezogen oder das Spiel gelaufen sein. Eine Seite, die einmal gebaut
    # wird und dann steht, zeigt bei genau der Frage „ist noch alles da?"
    # eine veraltete Antwort — das wäre schlimmer als keine.
    fenster.beim_zeigen['joysticks'] = _auffrischen


def _wasistneu(fenster, rahmen):
    """Die Änderungen — als Reiter, nicht als Fenster über dem Fenster.

    Zwei Dinge halten die Seite kurz, auch wenn zwanzig Versionen zusammenkommen:
    Nur die **neueste** ist aufgeklappt, und ein Filter zeigt bei Bedarf nur
    Behobenes. Wer einen Fehler gemeldet hat, sucht genau danach.
    """
    _ueberschrift(fenster, rahmen, t('hf_wasistneu'), t('s_wn_lead'))

    from . import aktualisierung
    try:
        eintraege = aktualisierung.protokoll()
    except Exception as ausnahme:
        fehler.merken('seiten.wasistneu', ausnahme)
        eintraege = []

    stand = {'art': 'alle'}
    chips = {}
    leiste = tk.Frame(rahmen, bg=BG)
    leiste.pack(fill='x', pady=(0, 10))
    innen = _rollflaeche(rahmen)
    behaelter = tk.Frame(innen, bg=BG)
    behaelter.pack(fill='both', expand=True)

    def zeichnen():
        for kind in behaelter.winfo_children():
            kind.destroy()
        gezeigt = 0
        for nummer, e in enumerate(eintraege[:15]):
            punkte = aktualisierung.punkte_nach_art(e.get('text') or '')
            if stand['art'] != 'alle':
                punkte = [p for p in punkte if p[0] == stand['art']]
            if not punkte:
                continue
            gezeigt += 1
            # Nur die neueste Version offen; ältere sind einen Klick entfernt.
            offen = (nummer == 0) or stand['art'] != 'alle'
            _fassung(fenster, behaelter, e, punkte, offen)
        if not gezeigt:
            tk.Label(behaelter, text=t('s_wn_nichts'), bg=BG, fg=SUB,
                     font=fenster.f_klein).pack(anchor='w', pady=12)

    def waehlen(art):
        stand['art'] = art
        for kennung, k in chips.items():
            k.setzen(kennung == art)
        zeichnen()

    for kennung, text in (('alle', t('s_wn_f_alle')), ('neu', t('s_wn_f_neu')),
                          ('bess', t('s_wn_f_bess')), ('fix', t('s_wn_f_fix'))):
        an = (kennung == 'alle')
        k = _chip(fenster, leiste, text, an)
        k.pack(side='left', padx=(0, 6))
        k.bind('<Button-1>', lambda ev, s=kennung: waehlen(s))
        chips[kennung] = k

    zeichnen()


def _chip(fenster, eltern, text, an):
    """Ein anklickbarer Filter — abgerundet, Rand in Akzentfarbe wenn gewählt."""
    from .hauptfenster import _rundes_rechteck
    schrift = fenster.f_klein
    hoehe = schrift.metrics('linespace') + 12
    breite = schrift.measure(text) + 26
    c = tk.Canvas(eltern, width=breite, height=hoehe, bg=BG,
                  highlightthickness=0, bd=0, cursor='hand2')
    blase = _rundes_rechteck(c, 1, 1, breite - 1, hoehe - 1,
                             radius=max(5, hoehe // 3),
                             fill=FLAECHE, outline=ACCENT if an else LINIE, width=1)
    beschriftung = c.create_text(breite / 2.0, hoehe / 2.0 + 1, text=text,
                                 fill=ACCENT if an else SUB, font=schrift)

    def setzen(gewaehlt):
        c.itemconfigure(blase, outline=ACCENT if gewaehlt else LINIE)
        c.itemconfigure(beschriftung, fill=ACCENT if gewaehlt else SUB)

    c.setzen = setzen
    return c


_ART_FARBE = {'neu': ACCENT, 'bess': '#7db8e8', 'fix': GOLD}
# ⚠ Eine Funktion, keine Konstante: Ein Wörterbuch auf Modulebene wird **einmal**
# beim Import gefüllt — und behielte damit die Sprache, die beim Start galt. Wer
# danach umschaltet, sähe die Marken weiter in der alten Sprache.
def _art_wort(art):
    return {'neu': t('s_wn_f_neu'), 'bess': t('s_wn_f_bess'),
            'fix': t('s_wn_f_fix')}.get(art, '')


def _fassung(fenster, eltern, eintrag, punkte, offen):
    """Eine Version mit Kopfzeile zum Auf- und Zuklappen."""
    zustand = {'offen': offen}
    kopf = tk.Frame(eltern, bg=BG, cursor='hand2')
    kopf.pack(fill='x', padx=24, pady=(12, 2))
    pfeil = zeichen.zeile(kopf, 'zuklappen' if offen else 'aufklappen',
                          grund=BG, schrift=fenster.f_klein)
    pfeil.pack(side='left', padx=(0, 8))
    tk.Label(kopf, text=eintrag.get('version') or '—', bg=BG, fg=ACCENT,
             font=fenster.f_fett).pack(side='left')
    if eintrag.get('datum'):
        tk.Label(kopf, text='  ' + eintrag['datum'], bg=BG, fg=SUB,
                 font=fenster.f_klein).pack(side='left')
    tk.Label(kopf, text=t('s_wn_aenderungen') % len(punkte), bg=BG, fg=SUB,
             font=fenster.f_klein).pack(side='right')

    koerper = tk.Frame(eltern, bg=BG)
    if offen:
        koerper.pack(fill='x')

    from .hauptfenster import marke
    # Der Vorstellungssatz der Version steht **hier**, unter ihrer Überschrift —
    # nicht irgendwo am Seitenende. Wer eine Version aufklappt, will zuerst wissen,
    # worum es ging, und dann die Einzelheiten.
    from . import aktualisierung as _akt
    lead = _akt.einleitung(eintrag.get('text') or '')
    if lead:
        satz = tk.Label(koerper, text=lead, bg=BG, fg=SUB, font=fenster.f_klein,
                        anchor='w', justify='left', wraplength=600)
        satz.pack(fill='x', padx=24, pady=(2, 8))

        # Denselben Weg wie bei den Punkten: nicht rechnen, sondern nehmen, was
        # das Label wirklich bekommt — sonst ragt der Satz bei jeder
        # Fenstergröße heraus.
        def lead_umbruch(ereignis, lab=satz):
            passend = max(200, ereignis.width - 8)
            try:
                if abs(_masszahl(lab, lab.cget('wraplength'))
                       - passend) > 4:
                    lab.configure(wraplength=passend)
            except tk.TclError:
                pass

        satz.bind('<Configure>', lead_umbruch)

    # Alle Blasen so breit wie die längste Beschriftung — sonst flattern sie
    # und die Texte daneben fangen an unterschiedlichen Stellen an.
    breiteste = max(fenster.f_klein.measure(_art_wort(a))
                    for a in ('neu', 'bess', 'fix')) + 20
    for art, zeile in punkte:
        z = tk.Frame(koerper, bg=BG)
        z.pack(fill='x', pady=3)
        marke(z, _art_wort(art), _ART_FARBE.get(art, SUB),
              fenster.f_klein, grund=BG,
              mindestbreite=breiteste).pack(side='left', anchor='n', padx=(0, 14))
        # ⚠ `wraplength` muss zur wirklichen Breite passen. Steht er zu hoch, bricht
        # der Text zu spät um und der Rest wird stumm abgeschnitten.
        #
        # Vorher stand hier „Fensterbreite minus 340" — ein geschätzter Abzug für
        # Seitenleiste, Ränder und die Art-Blase davor. Die Schätzung ging schief,
        # sobald sich eines davon änderte: Seit die Seitenleiste ihre Breite selbst
        # misst, fehlten rund 50 Pixel, und `tools/randpruefung.py` meldete die
        # Zeilen bei **jeder** Fenstergröße als beschnitten.
        #
        # Jetzt wird nicht mehr gerechnet, sondern genommen, was das Label
        # tatsächlich bekommt — und bei jeder Größenänderung neu. Damit stimmt es
        # auch, wenn jemand das Fenster zieht.
        etikett = tk.Label(z, text=_saubere_zeile(zeile), bg=BG, fg=FG,
                           font=fenster.f_klein, anchor='w', justify='left',
                           wraplength=max(360, (fenster.root.winfo_width()
                                                or 980) - 340))
        etikett.pack(side='left', fill='x', expand=True)

        def umbruch_anpassen(ereignis, lab=etikett):
            # Die Abfrage verhindert eine Schleife: Ein neuer Umbruch ändert die
            # Höhe, das löst wieder ein <Configure> aus.
            passend = max(200, ereignis.width - 8)
            try:
                if abs(_masszahl(lab, lab.cget('wraplength'))
                       - passend) > 4:
                    lab.configure(wraplength=passend)
            except tk.TclError:
                pass

        etikett.bind('<Configure>', umbruch_anpassen)

    def umschalten(*_):
        zustand['offen'] = not zustand['offen']
        pfeil.symbol_tauschen('zuklappen' if zustand['offen']
                             else 'aufklappen')
        if zustand['offen']:
            # ⚠ `after=kopf` ist der ganze Witz. Ohne das packt Tk den Inhalt ans
            # **Ende** der Fläche — also unter alle anderen Versionen. Bei elf
            # Versionen klappte man v3.0.0 auf und der Text erschien unterhalb von
            # v1.0.0; wer nicht weit genug rollt, hält die Version für leer. Beim
            # ersten Zeichnen fiel das nicht auf, weil dort Kopf und Inhalt
            # ohnehin nacheinander gepackt werden.
            koerper.pack(fill='x', after=kopf)
        else:
            koerper.pack_forget()

    # ⚠ **Alle** Teile des Kopfes binden, nicht nur Rahmen und Pfeil. Die
    # Versionsnummer, das Datum und die Anzahl sind eigene Labels — ein Klick
    # darauf erreichte den Rahmen nie. Genau dorthin zielt man aber: Gemeldet als
    # „die alten Versionen sind gar nicht aufklappbar".
    for teil in [kopf, pfeil] + list(kopf.winfo_children()):
        teil.bind('<Button-1>', umschalten)
        try:
            teil.configure(cursor='hand2')
        except tk.TclError:
            pass


def _saubere_zeile(zeile):
    """Markdown-Auszeichnung raus — Tk zeigt sie sonst als Sternchen."""
    import re
    zeile = re.sub(r'\*\*(.+?)\*\*', r'\1', zeile)
    zeile = re.sub(r'`([^`]+)`', r'\1', zeile)
    zeile = re.sub(r'\[([^\]]+)\]\([^)]+\)', r'\1', zeile)
    # ⚠ Und was danach noch an Rückstrichen übrig ist, fliegt raus. Der
    # Ausdruck oben nimmt nur **Paare**; ein einzelner Strich — aus einem
    # halbierten Codeblock etwa — blieb stehen und stand im Fenster.
    return zeile.replace('`', '').strip()


def _karte(eltern, rand=None, **kw):
    """Ein abgesetzter Kasten mit runden Ecken (siehe `hauptfenster.rundrahmen`)."""
    from .hauptfenster import rundrahmen
    innen = rundrahmen(eltern, FLAECHE, rand or LINIE, radius=8, grundfarbe=BG)
    innen.halter.pack(fill='x', **kw)
    return innen


def _wertzeile(fenster, eltern, bez, wert, farbe=None):
    z = tk.Frame(eltern, bg=FLAECHE)
    z.pack(fill='x', padx=16, pady=3)
    tk.Label(z, text=bez, bg=FLAECHE, fg=SUB, font=fenster.f_klein,
             width=24, anchor='w').pack(side='left')
    tk.Label(z, text=str(wert), bg=FLAECHE, fg=farbe or FG,
             font=fenster.f_klein, anchor='w').pack(side='left')


def _jetzt_nachsehen(fenster):
    """Wirklich bei GitHub nachfragen und sagen, was dabei herauskam.

    ⚠ Hier stand nur `fenster.sagen(t('s_ub_sucht'))` — der Knopf **meldete**, dass
    er sucht, und suchte nicht. Ein Nutzer (Bomb20, 25.08.2026) hatte deshalb auf
    rc18 weiterhin rc12 als neueste Version angeboten bekommen: Sein
    Zwischenspeicher blieb auf dem alten Stand, und der einzige Knopf, der ihn
    hätte auffrischen können, tat nichts.

    ⚠ Und danach stand hier zeitweise der **Holen**-Ablauf: herunterladen,
    einspielen, abtreten — mit `datei` und `freigabe`, die es in dieser Funktion
    nie gab. Der Knopf antwortete deshalb mit `name 'datei' is not defined`,
    egal ob eine neue Version da war oder nicht. Gemeldet am
    27.08.2026. Nachsehen ist nachsehen: Dieser Knopf lädt nichts.

    Läuft im eigenen Faden — die Abfrage geht ins Netz. Gezeichnet wird nur im
    Tk-Faden.
    """
    import threading
    from . import aktualisierung
    fenster.sagen(t('s_ub_sucht'))

    def arbeit():
        try:
            neuere = aktualisierung.nachsehen(fenster.version or '0.0.0',
                                              erzwingen=True)
        except Exception as ausnahme:
            fehler.merken('seiten.jetzt_nachsehen', ausnahme)
            fenster.root.after(0, lambda: fenster.sagen(t('s_ub_sucht_fehler')))
            return

        def melden():
            # ⚠ **Erst neu aufbauen, dann sagen.** `neu_aufbauen()` zerstoert
            # saemtliche Kinder des Fensters und baut sie neu — auch die
            # Fusszeile, in der `sagen()` schreibt. Stand das `sagen()` davor,
            # existierte die Antwort ein paar Millisekunden und war dann weg:
            # Der Knopf blieb bei „Suche nach einer neuen Version …" stehen und
            # meldete nie ein Ergebnis. Genau so gemeldet von der Autor am
            # 27.08.2026, direkt nach der Reparatur des `datei`-Fehlers.
            #
            # Der Neuaufbau muss trotzdem sein: Die Kanal-Kaesten tragen die
            # Versionsnummern und muessen mitziehen.
            try:
                fenster.neu_aufbauen()
            except Exception:
                pass
            if neuere:
                fenster.sagen(t('s_ub_gefunden') % neuere.get('version'))
            elif aktualisierung.abruf_geglueckt() is False:
                # ⚠ **Nicht „du bist aktuell" sagen, wenn gar nicht nachgesehen
                # werden konnte.** Die beiden Auskünfte sind das Gegenteil
                # voneinander. Bomb20 bekam am 27.08.2026 „du hast die neueste
                # rc67" gemeldet, während rc68 seit zwei Minuten draußen war —
                # der Abruf war an GitHubs Stundengrenze gescheitert und wurde
                # still verschluckt.
                fenster.sagen(t('s_ub_grenze') if aktualisierung.grenze_erreicht()
                              else t('s_ub_sucht_fehler'))
            else:
                fenster.sagen(t('s_ub_aktuell'))

        try:
            fenster.root.after(0, melden)
        except Exception:
            pass

    threading.Thread(target=arbeit, daemon=True).start()


# Wie oft die Update-Seite von allein bei GitHub nachsieht, solange sie offen
# ist. Fünf Minuten — siehe die Begründung in `_kanaele_auffrischen`.
AUFFRISCH_MS = 5 * 60 * 1000


def _kanaele_auffrischen(fenster, kaesten, neu_zeichnen):
    """Im Hintergrund nachsehen und die Knöpfe nachziehen — höchstens einmal.

    Läuft in einem eigenen Faden: Die Abfrage geht ins Netz und darf die Seite
    nicht aufhalten. Gezeichnet wird ausschließlich im Tk-Faden (`after`) — alles
    andere endet früher oder später in einem Absturz.
    """
    import threading
    from . import aktualisierung
    if getattr(kaesten, 'schon_gefragt', False):
        return
    kaesten.schon_gefragt = True
    vorher = (_holen_text(False, fenster.version),
              _holen_text(True, fenster.version))

    def arbeit():
        try:
            # ⚠ `erzwingen=True` ist hier nötig. Ohne das fragt `nachsehen()` nur,
            # wenn der letzte Blick länger als einen Tag her ist — und dann bleibt
            # die Beschriftung auf dem Stand von heute früh stehen. Genau so ist es
            # passiert: Der Knopf bot „v3.0.0-rc13 holen" an, während rc15 lief.
            # Wer draufdrückt, geht **zurück**. Einmal je Seitenaufbau nachfragen
            # ist der Preis dafür, dass draufsteht, was drin ist.
            aktualisierung.nachsehen(fenster.version or '0.0.0', erzwingen=True)
        except Exception as ausnahme:
            fehler.merken('seiten.kanaele_auffrischen', ausnahme)
            return

        def nachziehen():
            try:
                if not kaesten.winfo_exists():
                    return
                # ⚠ **Und dann in Ruhe wieder nachsehen.** Bis rc71 wurde
                # **einmal je Seitenaufbau** gefragt. Wer die Seite offen hatte,
                # während draußen eine neue Version erschien, sah weiter die alte
                # Nummer auf dem Knopf und hielt sich für aktuell. Bomb20 am
                # 27.08.2026: „ich krieg noch 67 angezeigt" — rc68 war seit
                # Minuten da.
                #
                # Fünf Minuten sind der Kompromiss: oft genug, dass niemand eine
                # Version verpasst, und selten genug für GitHubs Grenze von 60
                # Abfragen pro Stunde (macht zwölf, wenn jemand die Seite den
                # ganzen Tag offen lässt).
                try:
                    kaesten.schon_gefragt = False
                    kaesten.after(AUFFRISCH_MS, lambda: _kanaele_auffrischen(
                        fenster, kaesten, neu_zeichnen))
                except tk.TclError:
                    pass
                if (_holen_text(False, fenster.version),
                        _holen_text(True, fenster.version)) == vorher:
                    return              # nichts Neues, kein Flackern
                for kind in kaesten.winfo_children():
                    kind.destroy()
                neu_zeichnen()
            except tk.TclError:
                pass

        try:
            fenster.root.after(0, nachziehen)
        except Exception:
            pass

    threading.Thread(target=arbeit, daemon=True).start()


def _holen_moeglich(mit_vorab, eigene=''):
    """Steckt hinter dem Knopf ueberhaupt eine Tat? Sonst ist er keiner.

    ⚠ `_holen_text` liefert **nur** die Beschriftung, und zwei ihrer Ergebnisse
    sind gar keine Aufforderung, sondern eine Zustandsmeldung: „v3.0.0-rc41 ist
    schon da" und „Erst oben auf ‚Jetzt nachsehen' druecken". Der Knopf blieb
    trotzdem ein Knopf — wer auf „ist schon da" drueckte, bekam die laufende
    Version noch einmal installiert. Gemeldet am 26.08.2026: „in dem gruenen
    kasten steht aber rc41 ist schon da, wenn man klickt will er auch direkt
    installieren."

    Was aussieht wie ein Knopf, muss etwas tun. Sonst wird daraus ein ruhiger
    Hinweis (siehe `_kanalkasten`).
    """
    from . import aktualisierung
    try:
        freigabe = aktualisierung.neueste(mit_vorab)
    except Exception:
        return False
    if not freigabe:
        return False                     # „Erst oben auf ... druecken"
    fassung = freigabe.get('version') or ''
    # Nach einem geglueckten Update steht dort „Jetzt neu starten" — das ist
    # sehr wohl eine Tat.
    if _BEREIT[0] and _BEREIT[0] == fassung:
        return True
    if eigene and fassung.lstrip('v') == eigene.lstrip('v'):
        return False                     # laeuft schon, nichts zu holen
    return True



def _holen_text(mit_vorab, eigene=''):
    """Die Beschriftung des Knopfes — mit der Version, die dahinter steckt.

    Aus dem Zwischenspeicher, ohne ins Netz zu gehen: Die Seite soll sofort
    stehen. Kurz darauf frischt `_kanaele_auffrischen` sie auf.

    ⚠ Und sie sagt, **wohin** es geht. „v2.0.0 holen" neben einer laufenden
    v3.0.0-rc15 sieht aus wie ein Update und ist ein Rückschritt — der Autor ist
    am 25.08.2026 genau darauf hereingefallen und stand danach wieder auf rc13.
    Ist die angebotene Version älter, steht das jetzt dabei; ist es dieselbe,
    steht das auch da.
    """
    from . import aktualisierung
    try:
        freigabe = aktualisierung.neueste(mit_vorab)
    except Exception:
        freigabe = None
    if not freigabe:
        return t('s_ub_holen_keine')
    fassung = freigabe.get('version') or ''
    # Nach einem geglückten Update ist die neue Version auf der Platte, aber der
    # laufende Prozess hält noch die alte. Statt „beim nächsten Start" zu sagen,
    # wird der Knopf zum Neustart-Knopf.
    if _BEREIT[0] and _BEREIT[0] == fassung:
        return t('s_ub_neustart')
    if eigene:
        sauber = fassung.lstrip('v')
        if sauber == eigene.lstrip('v'):
            return t('s_ub_holen_gleich') % fassung
        if aktualisierung.ist_neuer(eigene, fassung):
            return t('s_ub_holen_zurueck') % fassung
    return t('s_ub_holen') % fassung


# Welche Version geholt und eingespielt wurde und nur noch einen Neustart braucht.
_BEREIT = [None]


_IM_TK_GEMELDET = [False]      # siehe unten: nur der erste wird gemerkt


def _im_tk(fenster, tat):
    """Etwas im Tk-Faden erledigen — und daran nicht scheitern.

    ⚠ **Zeichnen ist Beiwerk, die Arbeit ist der Zweck.** `root.after()` aus
    einem Nebenfaden kann werfen (`RuntimeError: main thread is not in main
    loop`), etwa wenn das Fenster gerade zugeht. Bis rc68 riss so eine Ausnahme
    den ganzen Update-Faden mit: Der Download brach beim ersten Fortschritt ab,
    es wurde nie etwas geholt, und der Nutzer sah gar nichts.

    Bomb20 am 27.08.2026: „ich habe auf get 68 geklickt, aber da kam nix mit
    restart oder install." In seinem Bericht stand der Fehler dreimal, bei jedem
    Klick einmal.

    ⚠ **Gemerkt wird nur der erste.** Beim Herunterladen kommt der Fortschritt
    im Sekundentakt; geht dabei das Fenster zu, wirft jeder einzelne Aufruf.
    Ein Bericht vom 28.08.2026 zeigte **50 von 50** Plätzen mit derselben
    Meldung belegt, alle innerhalb von acht Sekunden — und damit war jeder
    echte Fehler aus dem Protokoll verdrängt. Ein erwarteter Fehler, der die
    Diagnose unbrauchbar macht, ist schlimmer als keiner.
    """
    try:
        fenster.root.after(0, tat)
        return True
    except Exception as ausnahme:
        if not _IM_TK_GEMELDET[0]:
            _IM_TK_GEMELDET[0] = True
            fehler.merken('seiten.im_tk', ausnahme)
        return False


def _nach_neustart_abtreten(fenster):
    """Erst nachsehen, ob die neue Version lebt — dann erst selbst gehen.

    ⚠ Vorher trat die alte Version **sofort** ab. War die neue schon tot (unter
    Linux monatelang der Regelfall, siehe `aktualisierung.neue_fassung_laeuft`),
    stand der Rechner ohne Watcher da, und niemand erfuhr den Grund.

    Die Prüfung wartet ein paar Sekunden und gehört deshalb in einen eigenen
    Faden. Gezeichnet wird nur im Tk-Faden.
    """
    import threading
    from . import aktualisierung

    def pruefen():
        lebt = aktualisierung.neue_fassung_laeuft()
        def melden():
            if lebt:
                _abtreten(fenster)
            else:
                fenster.sagen(t('s_ub_neustart_tot'))
        try:
            fenster.root.after(0, melden)
        except Exception as ausnahme:
            fehler.merken('seiten.nach_neustart', ausnahme)

    threading.Thread(target=pruefen, daemon=True).start()


def _abtreten(fenster, notausgang=2.0, gleich=400):
    """Den Prozess beenden — verlaesslich, auch wenn Tk schon haengt.

    ⚠ `quit()` allein reicht nicht: Es beendet die Ereignisschleife, nicht den
    Prozess. Gemeldet als „er schliesst das fenster nicht selbst" (Haldjas,
    25.08.2026) — die neue Version lief, die alte stand daneben. Also der Reihe
    nach: Fenster zu, Schleife beenden, und wenn nach zwei Sekunden immer noch
    etwas haengt (ein Faden, ein Overlay), hart raus.

    ⚠ Der Notausgang wird **sofort** scharf gestellt, nicht erst im
    `after`-Rueckruf. Stand er dort, hing er an Tk: Feuert der Rueckruf nicht,
    weil die Ereignisschleife schon endete oder das Fenster weg ist, wurde der
    Faden nie gestartet und der Prozess lief weiter — „als haette er nur das
    symbol von der taskleiste gekillt", mit einem halb aufgeraeumten Rest, der
    danach „No such file or directory: ...base_library.zip" meldete.

    Ein eigener Faden haengt an nichts und laeuft in jedem Fall ab.
    """
    import threading
    threading.Timer(notausgang, lambda: os._exit(0)).start()

    def _weg():
        try:
            fenster.root.quit()
            fenster.root.destroy()
        except Exception:
            pass

    try:
        fenster.root.after(gleich, _weg)
    except Exception:
        pass



def _fassung_holen(fenster, mit_vorab):
    """Die neueste Version dieses Kanals holen und einspielen.

    ⚠ Nicht `nachsehen()` benutzen: Das meldet nur, was **neuer** ist als die
    laufende Version. Wer eine Testfassung fährt und zurück auf die letzte
    fertige will, bekäme damit nichts. `neueste()` fragt den Kanal, nicht den
    Abstand zur eigenen Version.

    Heruntergeladen wird in einem eigenen Faden — es sind zwölf Megabyte, und
    das Fenster darf so lange nicht einfrieren.
    """
    import threading
    from . import aktualisierung
    # ⚠ Erst nachsehen, dann greifen. Die Liste der Freigaben steht im
    # Zwischenspeicher und frischt sich nur einmal am Tag auf — ohne diesen
    # Schritt holt der Knopf die Version von gestern, obwohl heute eine neuere
    # da ist. Gemessen: Der Knopf bot v3.0.0-rc2 an, während rc7 längst
    # veröffentlicht war.
    try:
        aktualisierung.nachsehen(fenster.version or '0.0.0')
    except Exception as ausnahme:
        fehler.merken('seiten.fassung_holen.nachsehen', ausnahme)
    freigabe = aktualisierung.neueste(mit_vorab)
    if not freigabe:
        fenster.sagen(t('s_ub_holen_keine'))
        return
    # Schon geholt? Dann ist der Knopf jetzt der Neustart-Knopf.
    if _BEREIT[0] and _BEREIT[0] == (freigabe.get('version') or ''):
        fenster.sagen(t('s_ub_startet_neu'))
        if not aktualisierung.neu_starten():
            fenster.sagen(t('s_ub_neustart_nein'))
            return
        _nach_neustart_abtreten(fenster)
        return
    art = aktualisierung.verpackung()
    if art == 'quellcode':
        fenster.sagen(t('update_quellcode'))
        return
    datei = aktualisierung.passende_datei(freigabe)
    if not datei:
        # ⚠⚠ **Zwei verschiedene Lagen, zwei verschiedene Antworten.**
        #
        # Hängt an der Freigabe **gar keine** Datei, wird sie gerade noch
        # gebaut: Der Tag ist da, GitHub Actions braucht danach ein bis zwei
        # Minuten für Installer und AppImage. Wer in dieser Lücke klickt, bekam
        # bisher „Bitte hol die neue Version selbst von der Releases-Seite" —
        # und dort ist sie dann auch nicht. Am 30.08.2026 gemeldet: „wieso
        # steht das da?"; nach einem Neustart lief es von allein.
        #
        # Sind Dateien da, aber keine passende, stimmt die alte Meldung.
        if not (freigabe.get('dateien') or []):
            fenster.sagen(t('s_ub_wird_gebaut'))
        else:
            fenster.sagen(t('selbst_holen'))
        return

    fenster.sagen(t('s_ub_holen_laeuft') % freigabe.get('version'))

    def arbeit():
        try:
            ziel = aktualisierung.herunterladen(
                datei, fortschritt=lambda p: _im_tk(
                    fenster, lambda: fenster.sagen(t('wird_geladen', p))))

            # ⚠ Sagen, was gleich passiert — **vor** dem Einspielen.
            #
            # Das war die eigentliche Neuerung von rc52: Ein Programm, das sich
            # wortlos schliesst und nicht wiederkommt, sieht aus wie ein
            # Absturz. Der Hinweis nennt das Schliessen, das Einspielen und den
            # noetigen Neustart, und beruhigt wegen des Bestands.
            #
            # ⚠ Nur stand er bis rc62 in `_jetzt_nachsehen` — einer Funktion,
            # die gar nichts einspielt und deren Block ohnehin an einem
            # `NameError` starb. Beim echten Update kam er also **nie**.
            # Gefunden am 27.08.2026 beim Nachgehen des Nachsehen-Fehlers.
            #
            # ⚠ `messagebox` gehoert in den Tk-Faden, nicht hierher. Deshalb
            # `after(0, …)` und das Warten auf die Quittung: Erst wenn der
            # Nutzer gelesen hat, laeuft das Setup los. Sonst zaehlte der
            # Restart Manager schon seine dreissig Sekunden, waehrend der
            # Dialog noch offen steht.
            if art == 'exe':
                gelesen = threading.Event()

                def bescheid_geben():
                    try:
                        from tkinter import messagebox
                        messagebox.showinfo(t('s_ub_hinweis_titel'),
                                            t('s_ub_hinweis_neustart'))
                    finally:
                        gelesen.set()

                _im_tk(fenster, bescheid_geben)
                gelesen.wait(120)      # ⚠ nicht ewig: ein Fenster kann zugehen

            geklappt, grund = aktualisierung.einspielen(ziel)
            if not geklappt:
                _im_tk(fenster, lambda: fenster.sagen(
                    t('update_fehler', grund)))
                return
            _BEREIT[0] = freigabe.get('version') or ''

            # ⚠ Unter Windows ist hier **Schluss** — kein zweiter Klick mehr.
            #
            # Der Ablauf mit „erst holen, dann auf ‚Jetzt neu starten' druecken"
            # stammt aus der Zeit des Dateitauschs: Damals lag die neue Datei
            # nur bereit, und getauscht wurde beim Beenden. Der Installer
            # dagegen **laeuft schon** — und wartet darauf, dass wir endlich
            # gehen.
            #
            # Genau das hat am 26.08.2026 die lange Pause verursacht, die
            # der Autor gemeldet hat („wieso es solange dauert bis er alles
            # geschlossen hat, das wirkt komisch auf user"). Im Inno-Protokoll
            # steht sie auf die Millisekunde:
            #
            #     09:50:07.869  Shutting down applications using our files.
            #     09:50:39.243  Directory for uninstall files: ...
            #
            # 31,4 Sekunden — der Standard-Timeout des Restart Managers. Er
            # bittet erst hoeflich ums Schliessen und raeumt erst nach Ablauf
            # hart ab. Wer waehrenddessen auf den Knopf schaut, sieht ein
            # Programm, das nichts tut.
            #
            # Treten wir gleich ab, entfaellt das Warten vollstaendig, und der
            # `[Run]`-Abschnitt des Installers faehrt uns danach wieder hoch.
            if art == 'exe':
                _im_tk(fenster, lambda: fenster.sagen(t('s_ub_startet_neu')))
                _abtreten(fenster)
                return

            # Linux: Das AppImage ist getauscht, laufen tut aber noch die alte
            # Version. Hier bleibt der zweite Klick sinnvoll — er beendet und
            # startet neu.
            # ⚠ Dieselbe Reihenfolge wie oben: erst zeichnen, dann melden.
            # Der Neuaufbau macht aus „holen" ein „Jetzt neu starten" — er
            # zerstoert dabei aber die Fusszeile. Stand das `sagen()` zuerst
            # (after 0) und der Aufbau danach (after 50), war die Meldung nach
            # einer zwanzigstel Sekunde wieder weg.
            _im_tk(fenster, fenster.neu_aufbauen)
            try:
                fenster.root.after(50, lambda: fenster.sagen(t('s_ub_bereit')))
            except Exception:
                pass
        except Exception as ausnahme:
            grund = str(ausnahme)
            fehler.merken('seiten.fassung_holen', ausnahme)
            _im_tk(fenster, lambda: fenster.sagen(t('update_fehler', grund)))

    threading.Thread(target=arbeit, daemon=True).start()


def _kanalkasten(fenster, eltern, titel, text, gewaehlt, tat, marke_text='',
                 untereinander=False, holen=None, holen_text='',
                 holen_aktiv=True, platz=0):
    """Eine Wahlmöglichkeit als Kasten — wie in der Vorschau.

    Ein Schalter mit „an/aus" beantwortet die Frage nicht, die der Spieler hat:
    *Was bedeutet das für mich?* Zwei Kästen mit je zwei Sätzen tun das.

    ⚠ `untereinander` ist kein Schönheitsgriff. Nebeneinander brauchen die
    beiden Kästen mehr Platz, als die Mindestfensterbreite hergibt — Tk
    verteilt dann nicht etwa gerecht, sondern gibt dem ersten seine volle
    Wunschbreite und quetscht den zweiten auf 49 Pixel zusammen. Gemessen bei
    720×520: 329 Pixel fehlten.

    ⚠ **`grid` statt `pack`, und zwar wegen `uniform`.** Mit
    `pack(expand=True)` verteilt Tk nur den **Überschuss** gleichmäßig, nicht
    die Gesamtbreite: Wer mehr Text hat, bleibt breiter. Die beiden Kästen
    standen deshalb sichtbar ungleich nebeneinander — gemeldet von der Autor am
    27.08.2026 („die müssen aber gleich sein"). `columnconfigure(…,
    uniform=…)` ist die einzige Zusage in Tk, die zwei Spalten wirklich gleich
    breit macht; bei `pack` gibt es nichts Vergleichbares.
    """
    from .hauptfenster import marke as blase
    from .hauptfenster import rundrahmen
    innen = rundrahmen(eltern, FLAECHE, ACCENT if gewaehlt else LINIE,
                       radius=8, grundfarbe=BG)
    rand = innen.halter
    if untereinander:
        eltern.grid_columnconfigure(0, weight=1, uniform='')
        rand.grid(row=platz, column=0, sticky='ew', pady=(0, 10))
    else:
        # `uniform` bindet die Spalten aneinander: gleiche Breite, egal wie
        # lang der Text ist. `sticky='nsew'` zieht beide auf dieselbe Höhe.
        eltern.grid_columnconfigure(platz, weight=1, uniform='kanal')
        eltern.grid_rowconfigure(0, weight=1)
        rand.grid(row=0, column=platz, sticky='nsew',
                  padx=(0, 5) if platz == 0 else (5, 0))
    rand.configure(cursor='hand2')
    innen.configure(cursor='hand2')
    innen.leinwand.configure(cursor='hand2')
    leinwand = innen.leinwand

    kopf = tk.Frame(innen, bg=FLAECHE)
    kopf.pack(fill='x', padx=14, pady=(12, 2))
    zeichen.zeile(kopf, 'punkt',
                  farbe=zeichen.GRUEN if gewaehlt else zeichen.GRAU,
                  grund=FLAECHE, schrift=fenster.f_klein
                  ).pack(side='left', padx=(0, 7))
    tk.Label(kopf, text=titel, bg=FLAECHE, fg=FG,
             font=fenster.f_fett).pack(side='left')
    if marke_text:
        blase(kopf, marke_text, GOLD, fenster.f_klein).pack(side='left', padx=8)

    beschreibung = tk.Label(innen, text=text, bg=FLAECHE, fg=SUB,
                            font=fenster.f_klein, anchor='w', justify='left')
    beschreibung.pack(fill='x', padx=14, pady=(0, 12))
    # ⚠ 28 gleicht nur `padx=14` links und rechts aus. Rahmen und Leinwand
    # brauchen darüber hinaus ein paar Pixel, die niemand mitgerechnet hat —
    # gemessen fehlten 5 (tools/randpruefung.py). Mit etwas Luft bricht der Text
    # ein paar Pixel früher um, was niemand sieht, statt abgeschnitten zu
    # werden, was jeder sieht.
    _umbruch(beschreibung, abzug=36)

    for teil in (rand, leinwand, innen, kopf):
        teil.bind('<Button-1>', lambda e: tat())
    for kind in innen.winfo_children() + kopf.winfo_children():
        try:
            kind.bind('<Button-1>', lambda e: tat())
        except Exception:
            pass

    # Der Holen-Knopf ganz unten im Kasten, über die volle Breite. ⚠ **Nach** den
    # Bindungen oben angelegt: Sonst würde ihn die Schleife mit „Kanal wählen"
    # belegen, und ein Klick darauf täte etwas anderes als draufsteht.
    if holen is not None and holen_aktiv:
        knopf = _knopf(fenster, innen, holen_text, holen, stark=gewaehlt)
        knopf.pack(fill='x', padx=14, pady=(0, 12))
    elif holen is not None:
        # Kein Knopf, sondern eine Auskunft: Es gibt gerade nichts zu holen.
        # Gleiche Stelle, gleiche Breite, nur ohne Rahmen und ohne Handzeiger —
        # damit niemand darauf drueckt und sich fragt, warum nichts passiert.
        auskunft = tk.Label(innen, text=holen_text, bg=FLAECHE, fg=SUB,
                            font=fenster.f_klein, anchor='center')
        auskunft.pack(fill='x', padx=14, pady=(4, 16))
        # Ein Klick darauf soll dasselbe tun wie ein Klick auf den Kasten:
        # den Kanal waehlen. Sonst waere hier ein totes Loch im Kasten.
        auskunft.bind('<Button-1>', lambda e: tat())
        auskunft.configure(cursor='hand2')
    return rand


def _serverstatus(fenster, rahmen):
    """Läuft Star Citizen gerade? — was CIG auf seiner Statusseite meldet.

    ⚠ **Erst zeigen, dann holen.** Beim Öffnen steht sofort der letzte bekannte
    Stand da, das Auffrischen läuft im Hintergrund. Wer die Seite öffnet und
    fünfzehn Sekunden auf eine leere Fläche sieht, hält sie für kaputt — und
    genau so lange darf ein Abruf dauern, bevor er aufgibt.

    ⚠ **Der Abruf läuft nie im Tk-Faden.** Ein hängendes Netz würde sonst das
    ganze Fenster einfrieren, Overlay eingeschlossen.
    """
    import threading
    from . import serverstatus

    _ueberschrift(fenster, rahmen, t('hf_serverstatus'), t('s_st_lead'))
    innen = _rollflaeche(rahmen)

    # ⚠ Der Behälter wird hier nur **erzeugt**, gepackt wird er weiter unten —
    # nach dem Knopf. Über `before` einzufügen ging schief: Die Knöpfe sind
    # Leinwände, die ihre Höhe erst über ein Ereignis nachziehen, und der Knopf
    # blieb als leerer grüner Streifen stehen. Die Packreihenfolge einzuhalten
    # ist der ruhigere Weg als sie nachträglich zu drehen.
    behaelter = tk.Frame(innen, bg=BG)

    def zeichnen(lage):
        for kind in behaelter.winfo_children():
            kind.destroy()
        # ⚠ Drei Fälle, drei Meldungen: nie abgerufen, keine Verbindung, oder
        # keine Verbindung **aber** ein alter Stand. Vorher gab es nur „noch
        # nichts abgerufen" — und den Rat, auf „Jetzt nachsehen" zu klicken,
        # was ohne Internet zu nichts führt.
        ohne_netz = bool(lage.get('kein_netz'))
        if not lage.get('systeme'):
            _fliesstext(behaelter,
                        t('s_st_kein_netz') if ohne_netz else t('s_st_leer'),
                        fenster.f_klein, pady=(4, 8))
            return
        if ohne_netz:
            _fliesstext(behaelter, t('s_st_alt_ohne_netz'), fenster.f_klein,
                        farbe=GOLD, pady=(0, 8))

        # --- Kopfzeile, wie oben auf der Statusseite ---
        # Links „Zuletzt aktualisiert vor …", rechts die Zusammenfassung. Die
        # Seite hinterlegt diesen Streifen in der Ampelfarbe; das ist ihr
        # auffälligstes Element und die Antwort auf die eigentliche Frage.
        _kopfstreifen(fenster, behaelter, lage)

        karte = _karte(behaelter, pady=(0, 6))
        tk.Frame(karte, bg=FLAECHE, height=8).pack()
        for sys_ in lage.get('systeme') or []:
            _systemzeile(fenster, karte, sys_)
        tk.Frame(karte, bg=FLAECHE, height=10).pack()

        fuss = _karte(behaelter, pady=(8, 6))
        tk.Frame(fuss, bg=FLAECHE, height=8).pack()
        if lage.get('stand'):
            _wertzeile(fenster, fuss, t('s_st_stand'), _uhrzeit(lage['stand']))
        _wertzeile(fenster, fuss, t('s_st_geholt'), _uhrzeit(lage.get('geholt')))
        _quellzeile(fenster, fuss, t('s_st_quelle'), lage.get('quelle') or '')
        tk.Frame(fuss, bg=FLAECHE, height=10).pack()

        _fliesstext(behaelter, t('s_st_hinweis'), fenster.f_klein, pady=(10, 4))

        # --- „Letzte Meldungen", wie unten auf der Statusseite ---
        # Auch **erledigte**: Wer abends nicht ins Spiel kommt, will sehen, ob
        # es nachmittags eine Wartung gab — nicht nur, ob gerade eine läuft.
        tk.Label(behaelter, text=t('s_st_letzte'), bg=BG, fg=FG,
                 font=fenster.f_titel, anchor='w').pack(fill='x', pady=(22, 6))
        meldungsraum = tk.Frame(behaelter, bg=BG)
        meldungsraum.pack(fill='x')
        _fliesstext(meldungsraum, t('s_st_laedt'), fenster.f_klein, pady=(2, 4))
        _meldungen_laden(fenster, meldungsraum, lage.get('quelle') or '')

    def auffrischen(erzwingen=False):
        if erzwingen:
            fenster.sagen(t('s_st_laedt'))

        def arbeit():
            # ⚠⚠ **Jeder Rückweg ins Fenster muss abgesichert sein.** Der Faden
            # läuft weiter, auch wenn der Nutzer die Seite wechselt oder das
            # Fenster schliesst — `after()` wirft dann
            # `RuntimeError: main thread is not in main loop`, und der Fehler
            # landet in keinem Haken, weil er in einem eigenen Faden passiert.
            # Ohne Internet dauert der Abruf am längsten, also trifft es genau
            # dann: „Einstellungsmenü stürzt ab, wenn der User kein Internet
            # mehr hat und man auf Serverstatus geht" (30.08.2026).
            try:
                lage = serverstatus.lage(erzwingen=erzwingen)
            except Exception as ausnahme:
                fehler.merken('seiten.serverstatus', ausnahme)
                lage = None
            try:
                if lage is None:
                    fenster.root.after(
                        0, lambda: behaelter.winfo_exists()
                        and fenster.sagen(t('s_st_fehler')))
                else:
                    fenster.root.after(
                        0, lambda: behaelter.winfo_exists() and zeichnen(lage))
            except (RuntimeError, tk.TclError):
                pass          # Fenster ist weg — dann gibt es nichts zu zeigen

        threading.Thread(target=arbeit, daemon=True).start()

    # ⚠ Der Knopf gehört **über** den Inhalt, nicht darunter. Unter der
    # Meldungsliste läge er nach mehreren Bildschirmhöhen Text — niemand rollt
    # nach unten, um eine Schaltfläche zu suchen, die er sofort erwartet.
    # `before` setzt ihn vor den Behälter, obwohl er später erzeugt wird.
    # Der Knopf gehört über den Inhalt: Unter der Meldungsliste läge er nach
    # mehreren Bildschirmhöhen Text, und niemand rollt nach unten, um eine
    # Schaltfläche zu suchen, die er sofort erwartet.
    _knopf(fenster, innen, t('s_st_nachsehen'),
           lambda: auffrischen(True), stark=True).pack(fill='x', pady=(0, 12))
    behaelter.pack(fill='x')

    # --- Der laufende Takt ---
    #
    # Jede Minute ein Blick, ob sich etwas geändert hat. Das ist billig, weil
    # mit ETag gefragt wird: Hat CIG nichts angefasst, kommt ein 304 ohne
    # Inhalt zurück.
    #
    # ⚠ **Neu gezeichnet wird nur, wenn sich wirklich etwas geändert hat.**
    # Sonst würde die Anzeige jede Minute zerlegt und neu aufgebaut — wer
    # gerade eine Meldung liest, verlöre dabei seine Rollposition.
    #
    # ⚠ Der Takt hört auf, sobald die Seite weg ist. Ohne die Prüfung auf
    # `winfo_exists` liefe er weiter, wenn der Nutzer längst woanders ist, und
    # jeder Seitenwechsel legte einen weiteren Takt obendrauf.
    def takt():
        if not behaelter.winfo_exists():
            return

        def arbeit():
            try:
                lage, veraendert = serverstatus.nachfragen()
            except Exception:
                lage, veraendert = None, False
            # Dieselbe Absicherung wie oben: Der Takt läuft, während der Nutzer
            # das Fenster schliessen kann.
            try:
                if veraendert and lage:
                    fenster.root.after(0, lambda: behaelter.winfo_exists()
                                       and zeichnen(lage))
                fenster.root.after(TAKT_MS, takt)
            except (RuntimeError, tk.TclError):
                pass

        threading.Thread(target=arbeit, daemon=True).start()

    zeichnen(serverstatus.gespeicherte_lage())   # sofort, ohne Netz
    auffrischen()                                # und im Hintergrund nachziehen
    fenster.root.after(TAKT_MS, takt)            # danach im Takt weiter


# Wie oft nachgefragt wird, solange die Seite offen ist. Eine Minute ist
# vertretbar, weil mit ETag gefragt wird und der unveränderte Fall den Server
# fast nichts kostet.
TAKT_MS = 60_000


def _relative_zeit(stempel):
    """„gerade eben", „vor 7 Std.", „vor 2 Monaten" — wie auf der Statusseite.

    Die Seite schreibt das Alter, nicht das Datum („Last updated just now",
    „7h ago"). Das ist die Angabe, die man beim Überfliegen wirklich braucht:
    Ob eine Wartung heute Nachmittag war oder im Juli, sieht man so sofort.
    Das genaue Datum steht daneben in der Fußzeile."""
    import time as _t
    if not stempel:
        return '—'
    alter = max(0, _t.time() - stempel)
    if alter < 90:
        return t('s_st_gerade')
    def form(anzahl, schluessel):
        """Einzahl und Mehrzahl auseinanderhalten — „vor 1 Tagen" ist falsch."""
        if anzahl == 1:
            return t(schluessel + '_1')
        return t(schluessel) % anzahl

    if alter < 3600:
        return form(int(alter // 60), 's_st_vor_min')
    if alter < 86400:
        return form(int(alter // 3600), 's_st_vor_std')
    if alter < 60 * 86400:
        return form(int(alter // 86400), 's_st_vor_tag')
    return form(max(1, int(alter // (30 * 86400))), 's_st_vor_monat')


def _kopfstreifen(fenster, eltern, lage):
    """Der Streifen ganz oben: links das Alter, rechts die Zusammenfassung.

    Bildet nach, was die Statusseite dort zeigt („Last updated just now" /
    „No issues detected"). Es ist die Antwort auf die Frage, wegen der jemand
    die Seite überhaupt öffnet — deshalb steht sie oben und nicht in einer
    Werteliste.

    ⚠ **Kein `rundrahmen`.** Dessen Leinwand bleibt auf ihrer Anfangshöhe,
    wenn der Inhalt nicht mitgemessen wird — der Streifen erschien als leerer
    grüner Rahmen. Ein schlichter Frame mit farbigem Balken am linken Rand
    trägt dieselbe Aussage und kann nicht einklappen.
    """
    farbe = _ampelfarbe(lage)
    streifen = tk.Frame(eltern, bg=FLAECHE)
    streifen.pack(fill='x', pady=(0, 10))
    tk.Frame(streifen, bg=farbe, width=4).pack(side='left', fill='y')

    inhalt = tk.Frame(streifen, bg=FLAECHE)
    inhalt.pack(side='left', fill='x', expand=True, padx=12, pady=10)
    alter = _relative_zeit(lage.get('geholt'))
    tk.Label(inhalt, text=t('s_st_zuletzt') % alter,
             bg=FLAECHE, fg=SUB, font=fenster.f_klein,
             anchor='w').pack(side='left')
    alles_gut = (lage.get('gesamt') or '').lower() == 'operational'
    tk.Label(inhalt, text=t('s_st_ok') if alles_gut else t('s_st_stoerung'),
             bg=FLAECHE, fg=farbe, font=fenster.f_fett,
             anchor='e').pack(side='right')


def _meldungen_laden(fenster, raum, quelle):
    """Die letzten Meldungen holen und einsetzen — im eigenen Faden.

    ⚠ Jeder Volltext ist ein eigener Abruf; beim ersten Mal dauert das ein paar
    Sekunden. Deshalb steht solange „wird geholt" da, statt das Fenster
    festzuhalten."""
    import threading
    from . import serverstatus

    def einsetzen(liste):
        if not raum.winfo_exists():
            return
        for kind in raum.winfo_children():
            kind.destroy()
        if not liste:
            _fliesstext(raum, t('s_st_keine'), fenster.f_klein, pady=(2, 4))
        else:
            for meldung in liste:
                _meldungskarte(fenster, raum, meldung)
        if quelle:
            _quellink(fenster, raum, t('s_st_alle_zeigen'), quelle)

    def arbeit():
        try:
            liste = serverstatus.meldungen(2)
        except Exception as ausnahme:
            fehler.merken('seiten.serverstatus_meldungen', ausnahme)
            liste = []
        fenster.root.after(0, lambda: einsetzen(liste))

    threading.Thread(target=arbeit, daemon=True).start()


def _quellink(fenster, eltern, text, adresse):
    """Ein anklickbarer Verweis als eigene Zeile."""
    link = tk.Label(eltern, text=text, bg=BG, fg=ACCENT, font=fenster.f_klein,
                    anchor='w', cursor='hand2')
    link.pack(fill='x', pady=(10, 4))

    def oeffnen(_=None):
        # ⚠ Über `pfade.im_browser` — nie `webbrowser.open()` direkt. Warum:
        # siehe die Begründung dort (im AppImage öffnet es nichts und meldet
        # trotzdem Erfolg).
        if not pfade.im_browser(adresse):
            fenster.sagen(t('s_ub_auf_nein') % adresse)

    link.bind('<Button-1>', oeffnen)
    link.bind('<Enter>', lambda e: link.configure(fg=FG))
    link.bind('<Leave>', lambda e: link.configure(fg=ACCENT))


def _quellzeile(fenster, eltern, bez, adresse):
    """Wie `_wertzeile`, aber die Adresse lässt sich anklicken.

    Eine Quelle, die man nur ablesen und abtippen kann, ist keine Quelle —
    besonders bei einer Angabe, die man im Zweifel selbst nachprüfen soll."""
    z = tk.Frame(eltern, bg=FLAECHE)
    z.pack(fill='x', padx=16, pady=3)
    tk.Label(z, text=bez, bg=FLAECHE, fg=SUB, font=fenster.f_klein,
             width=24, anchor='w').pack(side='left')
    if not adresse:
        tk.Label(z, text='—', bg=FLAECHE, fg=FG, font=fenster.f_klein,
                 anchor='w').pack(side='left')
        return
    link = tk.Label(z, text=adresse, bg=FLAECHE, fg=ACCENT,
                    font=fenster.f_klein, anchor='w', cursor='hand2')
    link.pack(side='left')

    def oeffnen(_=None):
        if not pfade.im_browser(adresse):
            fenster.sagen(t('s_ub_auf_nein') % adresse)

    link.bind('<Button-1>', oeffnen)

    # Rückmeldung beim Darüberfahren über die **Farbe**, nicht über die Schrift.
    #
    # ⚠ `fenster.f_klein` ist ein `tkfont.Font`-Objekt, kein Tupel — `font[0]`
    # wirft. Und ein eigenes, unterstrichenes Font-Objekt anzulegen wäre die
    # zweite Falle: „Schrift größer" stellt zentral genau diese gemeinsamen
    # Objekte um, ein eigenes bliebe stehen und der Link wäre der einzige
    # Text, der nicht mitwächst.
    link.bind('<Enter>', lambda e: link.configure(fg=FG))
    link.bind('<Leave>', lambda e: link.configure(fg=ACCENT))


def _ampelfarbe(lage):
    """Die Farbe der Gesamtlage — die schlechteste, die vorkommt.

    „Alles grün außer einem" ist nicht grün. Wer nur die Zusammenfassung liest,
    soll denselben Eindruck bekommen wie jemand, der die Liste durchgeht."""
    rang = {'ok': 0, 'hinweis': 1, 'gestoert': 2, 'aus': 3}
    schlimmste = None
    for s_ in lage.get('systeme') or []:
        if schlimmste is None or rang.get(s_.get('ampel'), 2) > rang.get(schlimmste.get('ampel'), 2):
            schlimmste = s_
    return (schlimmste or {}).get('farbe') or FG


def _systemzeile(fenster, eltern, sys_):
    """Ein System: Farbbalken, Name, Zustand im Wortlaut von CIG."""
    z = tk.Frame(eltern, bg=FLAECHE)
    z.pack(fill='x', padx=16, pady=3)
    # Der Balken trägt die Aussage für alle, die Farben schlecht unterscheiden,
    # zusammen mit dem ausgeschriebenen Zustand daneben — nie die Farbe allein.
    tk.Frame(z, bg=sys_.get('farbe') or FG, width=4, height=18).pack(
        side='left', padx=(0, 10))
    # ⚠ Beides sind **Daten von CIG**, kein Oberflächentext: Systemname und
    # Zustand stehen so auf der Statusseite und dürfen nicht übersetzt werden.
    # Vorher entnommen, damit die Textprüfung die Schlüssel nicht für Sätze hält.
    name = sys_.get('name') or '?'
    zustand = sys_.get('status') or '—'
    tk.Label(z, text=name, bg=FLAECHE, fg=FG,
             font=fenster.f_klein, width=22, anchor='w').pack(side='left')
    tk.Label(z, text=zustand, bg=FLAECHE,
             fg=sys_.get('farbe') or FG, font=fenster.f_klein,
             anchor='w').pack(side='left')


def _meldungskarte(fenster, eltern, meldung):
    """Eine Meldung im Aufbau der Statusseite.

        Live Deployment                              ✔ Erledigt
        vor 7 Std.
        maintenance          Persistent Universe · Arena Commander
        <Meldungstext, Update-Zeilen im Original>

    ⚠ Der Text bleibt im **Wortlaut von CIG**, auch die Update-Zeilen
    (`1415 UTC - Initial Notice, Matchmaking disabled.`). Übersetzt wäre es eine
    Aussage, die RSI nie gemacht hat — und bei einer Störungsmeldung ist genau
    das gefährlich."""
    karte = _karte(eltern, pady=(6, 2))
    tk.Frame(karte, bg=FLAECHE, height=10).pack()

    # Kopf: Titel links, Zustand rechts
    kopf = tk.Frame(karte, bg=FLAECHE)
    kopf.pack(fill='x', padx=16)
    # Der Titel kommt von CIG und bleibt, wie er dort steht.
    titel = meldung.get('titel') or '—'
    tk.Label(kopf, text=titel, bg=FLAECHE, fg=FG,
             font=fenster.f_fett, anchor='w').pack(side='left')
    erledigt = bool(meldung.get('erledigt'))
    tk.Label(kopf, text=(t('s_st_erledigt_kurz')) if erledigt
             else t('s_st_offen'),
             bg=FLAECHE,
             # Erledigt grün wie auf der Statusseite; offen in Gold, damit es
             # auffällt — eine laufende Störung ist der Grund, warum jemand
             # überhaupt hier nachsieht.
             fg=(ACCENT if erledigt else GOLD),
             font=fenster.f_klein, anchor='e').pack(side='right')

    # Alter — wie auf der Seite („7h ago"), nicht das Datum
    wann = _relative_zeit(meldung.get('begonnen'))
    tk.Label(karte, text=wann, bg=FLAECHE,
             fg=SUB, font=fenster.f_klein, anchor='w').pack(
                 fill='x', padx=16, pady=(2, 6))

    # Etiketten: Schweregrad links, betroffene Systeme rechts — wie auf der Seite
    _etikettenreihe(fenster, karte, meldung.get('schwere'),
                    meldung.get('betroffen') or [])

    # ⚠ `fill='x'` ist Pflicht. Das Label ist zwar linksbündig gesetzt, aber
    # ohne Füllung zentriert Tk es als Ganzes im Kasten — der Meldungstext
    # stand mittig statt links und sah dadurch nicht aus wie auf der Seite.
    # ⚠ `fill='x'` ist Pflicht. Das Label ist zwar linksbündig gesetzt, aber
    # ohne Füllung zentriert Tk es als Ganzes im Kasten — der Meldungstext
    # stand mittig statt links.
    #
    # Die Hervorhebung kommt aus dem Quelltext von CIG mit: Dort steht fett,
    # was man tun soll („Fahrzeuge sichern"). Ältere Zwischenspeicher führen
    # noch reine Zeichenketten — die werden weiter vertragen, statt beim ersten
    # Start nach dem Update eine Ausnahme zu werfen.
    for eintrag in (meldung.get('zeilen') or []):
        if isinstance(eintrag, (list, tuple)):
            zeile, fett = eintrag[0], bool(eintrag[1])
        else:
            zeile, fett = eintrag, False
        _fliesstext(karte, zeile,
                    fenster.f_fett if fett else fenster.f_klein,
                    farbe=FG if fett else SUB, grund=FLAECHE,
                    fill='x', padx=16, pady=(0, 3), abzug=48)
    tk.Frame(karte, bg=FLAECHE, height=10).pack()


def _etikett(fenster, eltern, text):
    """Ein kleines graues Schild, wie die Marken auf der Statusseite.

    ⚠ **Bewusst kein `rundrahmen`.** Der setzt seinen Inhalt per
    `create_window` auf eine Leinwand — dadurch trägt der Inhalt nicht zur
    Wunschgröße bei, das Schild hat keine eigene Breite und dehnt sich über
    die halbe Karte. Bei großen Kästen fällt das nicht auf, hier schon: Aus
    kompakten Marken wurden Balken. Ein schlichtes Label kennt seine Größe.
    """
    return tk.Label(eltern, text=text, bg=LINIE, fg=FG, font=fenster.f_klein,
                    padx=8, pady=3)


def _etikettenreihe(fenster, eltern, schwere, betroffen):
    """Die Etiketten einer Meldung — **warum** links, **was betroffen ist** rechts.

    Die Trennung ist keine Kosmetik, sie trägt die Aussage: Links steht der
    Grund (`Maintenance`, `Degraded Performance`), rechts stehen die Systeme,
    die es trifft. Stehen alle drei gleichrangig nebeneinander, ist es
    Einheitsbrei und man muss raten, was wovon abhängt.

    ⚠ **Zwei Fallen, beide schon zugeschnappt:**

      Nebeneinander gepackt fällt heraus, wofür der Platz nicht reicht — auf
      der Seite standen drei Marken, im Werkzeug nur zwei. Tk warnt dabei nicht.

      Und ein reiner Fließumbruch behebt zwar das, ebnet aber die Trennung ein.

    Deshalb zwei Ebenen: Grund und Systemblock rücken untereinander, sobald sie
    nicht mehr nebeneinander passen — die Trennung bleibt dann als *oben und
    unten* erhalten. Innerhalb des Systemblocks bricht `grid` die Systeme
    weiter um, sodass auch bei sehr schmalem Fenster keines verschwindet.
    """
    reihe = tk.Frame(eltern, bg=FLAECHE)
    reihe.pack(fill='x', padx=16, pady=(0, 6))

    ABSTAND = 6

    grund = _etikett(fenster, reihe, schwere) if schwere else None

    systeme = tk.Frame(reihe, bg=FLAECHE)
    schilder = [_etikett(fenster, systeme, name) for name in (betroffen or [])]
    if not grund and not schilder:
        return

    def systeme_ordnen(platz):
        """Die Systeme fließend umbrechen — keines darf herausfallen."""
        zeile, spalte, belegt = 0, 0, 0
        for schild in schilder:
            breite = schild.winfo_reqwidth() + ABSTAND
            # Das erste Schild einer Zeile bleibt immer stehen, auch wenn es
            # allein schon zu breit ist. Abschneiden wäre genau der Fehler,
            # den diese Funktion behebt.
            if spalte and belegt + breite > max(platz, 1):
                zeile, spalte, belegt = zeile + 1, 0, 0
            schild.grid(row=zeile, column=spalte, sticky='w',
                        padx=(0, ABSTAND), pady=2)
            spalte += 1
            belegt += breite

    def ordnen(_=None):
        platz = reihe.winfo_width()
        if platz <= 1:
            platz = reihe.winfo_toplevel().winfo_width()
        breite_grund = (grund.winfo_reqwidth() + ABSTAND * 2) if grund else 0
        breite_systeme = sum(s.winfo_reqwidth() + ABSTAND for s in schilder)
        nebeneinander = breite_grund + breite_systeme <= platz

        if nebeneinander == getattr(reihe, 'zuletzt_nebeneinander', None):
            return
        reihe.zuletzt_nebeneinander = nebeneinander

        if grund:
            grund.pack_forget()
        systeme.pack_forget()

        if nebeneinander:
            if grund:
                grund.pack(side='left', padx=(0, ABSTAND))
            systeme.pack(side='right')
            systeme_ordnen(breite_systeme)          # alles in eine Zeile
        else:
            if grund:
                grund.pack(side='top', anchor='w', pady=(0, 4))
            systeme.pack(side='top', anchor='w', fill='x')
            systeme_ordnen(platz)

    reihe.bind('<Configure>', ordnen, add='+')
    reihe.after(0, ordnen)


def _uhrzeit(stempel):
    """Ein Zeitpunkt als Ortszeit. Die Quelle rechnet in UTC — hier steht,
    was die Uhr des Nutzers zeigt, sonst rechnet jeder selbst um."""
    import time as _t
    if not stempel:
        return '—'
    return _t.strftime('%d.%m.%Y %H:%M', _t.localtime(stempel))


def _dankblock(fenster, eltern, name, lizenz, was, adresse=None):
    """Ein Beitrag: wer, unter welcher Lizenz, wofür — und wo er zu finden ist."""
    kasten = tk.Frame(eltern, bg=FLAECHE)
    kasten.pack(fill='x', pady=(0, 8))

    from .hauptfenster import marke as blase
    kopf = tk.Frame(kasten, bg=FLAECHE)
    kopf.pack(fill='x', padx=16, pady=(12, 2))
    tk.Label(kopf, text=name, bg=FLAECHE, fg=FG, font=fenster.f_fett,
             anchor='w').pack(side='left')
    # Die Lizenz als Blase daneben — sie gehört zum Namen, nicht in den Fließtext.
    blase(kopf, lizenz, ACCENT, fenster.f_klein).pack(side='left', padx=8)

    text = tk.Label(kasten, text=was, bg=FLAECHE, fg=SUB, font=fenster.f_klein,
                    anchor='w', justify='left')
    text.pack(fill='x', padx=16, pady=(0, 10))
    _umbruch(text)

    if adresse:
        # ⚠ Nicht `_quellzeile`: die reserviert 24 Zeichen für eine
        # Beschriftung, und ohne Beschriftung stünde der Verweis eingerückt
        # mitten in der Karte statt am linken Rand wie der Text darüber.
        link = tk.Label(kasten, text=adresse, bg=FLAECHE, fg=ACCENT,
                        font=fenster.f_klein, anchor='w', cursor='hand2')
        link.pack(fill='x', padx=16, pady=(0, 12))

        def oeffnen(_=None):
            if not pfade.im_browser(adresse):
                fenster.sagen(t('s_ub_auf_nein') % adresse)

        link.bind('<Button-1>', oeffnen)
        link.bind('<Enter>', lambda e: link.configure(fg=FG))
        link.bind('<Leave>', lambda e: link.configure(fg=ACCENT))


def _person(fenster, eltern, name, gruppe, idee, funde):
    """Ein Name in der Dankliste — aufklappbar.

    Sichtbar ist immer nur die Kopfzeile (Name + Gruppe). Was die Person
    beigetragen hat, steht darunter und erscheint erst auf Klick. Grund: Die
    Liste soll vollständig bleiben, auch wenn irgendwann fünfzig Leute
    daraufstehen — vollständig **und** überschaubar geht nur so.
    """
    from .hauptfenster import marke as blase
    kasten = tk.Frame(eltern, bg=FLAECHE)
    kasten.pack(fill='x', pady=(0, 6))

    kopf = tk.Frame(kasten, bg=FLAECHE, cursor='hand2')
    kopf.pack(fill='x', padx=16, pady=10)
    pfeil = zeichen.zeile(kopf, 'aufklappen', grund=FLAECHE,
                          schrift=fenster.f_klein)
    pfeil.pack(side='left', padx=(0, 8))
    tk.Label(kopf, text=name, bg=FLAECHE, fg=FG, font=fenster.f_fett,
             anchor='w').pack(side='left')
    if gruppe:
        blase(kopf, gruppe, ACCENT, fenster.f_klein).pack(side='left', padx=8)

    koerper = tk.Frame(kasten, bg=FLAECHE)

    gebaut = []

    def zeichnen():
        if gebaut:
            return
        for text, farbe in ((idee, FG), (funde, SUB)):
            if not text:
                continue
            lab = tk.Label(koerper, text=_ohne_marken(text), bg=FLAECHE,
                           fg=farbe, font=fenster.f_klein, anchor='w',
                           justify='left')
            lab.pack(fill='x', padx=(46, 16), pady=(0, 8))
            _umbruch(lab, abzug=62)
        gebaut.append(True)

    def umschalten(_=None):
        if koerper.winfo_ismapped():
            koerper.pack_forget()
            pfeil.symbol_tauschen('aufklappen')
        else:
            zeichnen()
            koerper.pack(fill='x', after=kopf)
            pfeil.symbol_tauschen('zuklappen')

    for teil in (kopf, pfeil) + tuple(kopf.winfo_children()):
        teil.bind('<Button-1>', umschalten)


def _danke(fenster, rahmen):
    """Wem was gehört — und Dank an die, ohne die es das Werkzeug nicht gäbe.

    ⚠ Diese Seite gibt es seit v3.0.0-rc58. Vorher stand im ganzen Programm
    **keine** Lizenzangabe: weder die eigene (GPL-3.0) noch die der Symbole. Bei
    einem GPL-Programm gehört die eigene Lizenz sichtbar hin, und die
    ISC-Lizenz von Lucide verlangt, dass ihr Hinweis mitgeliefert wird — eine
    Datei tief in der entpackten `.exe` erfüllt das formal, findet aber niemand.

    Ein **eigener Reiter** statt eines Abschnitts auf „Update & Über": Die Seite
    dort ist mit Version, Katalogzahlen, Update-Kanal und Holen-Knopf schon voll,
    und wem was gehört, hat mit Updates nichts zu tun. Gemeldet am 27.08.2026:
    „fremdleistungen gehören doch als eigener tab ehr in info oder?"
    """
    _ueberschrift(fenster, rahmen, t('hf_danke'), t('s_dk_lead'))
    innen = _rollflaeche(rahmen)

    # --- Wer das gebaut hat ---
    # ⚠ Ganz oben und mit Avatar, nicht als eine Zeile unter vielen. Diese Seite
    # nennt fremde Arbeit, und genau deshalb muss die eigene zuerst stehen —
    # sonst schmälert die Aufzählung das, worum es hier eigentlich geht.
    # Gemeldet am 27.08.2026: „ich bin zwar dankbar, aber so dankbar nun auch
    # wieder nicht, zudem wieso sollte ich meine Leistung dadurch schmälern."
    #
    # Der Block stand bis dahin auf „Update & Über" und ist von dort hierher
    # gewandert — dieselben Angaben an zwei Stellen waren die eigentliche Klage.
    tk.Label(innen, text=t('hf_wer'), bg=BG, fg=FG, font=fenster.f_titel,
             anchor='w').pack(fill='x', pady=(0, 2))
    tk.Label(innen, text=t('s_ub_wer_h'), bg=BG, fg=SUB, font=fenster.f_klein,
             anchor='w').pack(fill='x', pady=(0, 12))

    autor = _karte(innen)
    zeile = tk.Frame(autor, bg=FLAECHE)
    zeile.pack(fill='x', padx=16, pady=14)
    from .hauptfenster import _mitgeliefert
    logo = _mitgeliefert(os.path.join('assets', 'xharig.png'))
    if logo and os.path.exists(logo):
        try:
            voll = tk.PhotoImage(file=logo)
            teiler = max(1, voll.width() // 64)
            fenster._autorlogo = voll.subsample(teiler, teiler)
            tk.Label(zeile, image=fenster._autorlogo, bg=FLAECHE).pack(
                side='left', padx=(0, 16))
        except Exception as ausnahme:
            fehler.merken('seiten.danke.logo', ausnahme)
    rechts = tk.Frame(zeile, bg=FLAECHE)
    rechts.pack(side='left', fill='x', expand=True)
    tk.Label(rechts, text='Xharig', bg=FLAECHE, fg=ACCENT, font=fenster.f_titel,
             anchor='w').pack(fill='x')
    tk.Label(rechts, text='SC BP Watcher %s · GPL-3.0-only'
             % (fenster.version or ''), bg=FLAECHE, fg=SUB,
             font=fenster.f_klein, anchor='w').pack(fill='x')
    _adresse(fenster, rechts, 'github.com/Xharig/SC-BP-Watcher',
             'https://github.com/Xharig/SC-BP-Watcher')
    _fliesstext(innen, t('s_dk_selbst_h'), fenster.f_klein, fill='x',
                pady=(10, 0))

    # --- Mitgeliefert ---
    tk.Label(innen, text=t('s_dk_dabei'), bg=BG, fg=FG, font=fenster.f_titel,
             anchor='w').pack(fill='x', pady=(18, 2))
    _fliesstext(innen, t('s_dk_dabei_h'), fenster.f_klein, fill='x',
                pady=(0, 10))
    _dankblock(fenster, innen, 'Lucide', 'ISC', t('s_dk_symbole'),
               'https://lucide.dev')

    # --- Wird geladen, nicht mitgeliefert ---
    tk.Label(innen, text=t('s_dk_extern'), bg=BG, fg=FG, font=fenster.f_titel,
             anchor='w').pack(fill='x', pady=(18, 2))
    _fliesstext(innen, t('s_dk_extern_h'), fenster.f_klein, fill='x',
                pady=(0, 10))
    _dankblock(fenster, innen, 'Star Citizen Mission DataBase',
               'CC BY-NC-ND 4.0', t('s_dk_scmdb'), 'https://scmdb.net')
    # ⚠ Seit v3.3.0-rc39 kommen die Rohstoffpreise von hier. Wer eine Quelle
    # benutzt, nennt sie — sie stand bis rc40 nirgends.
    _dankblock(fenster, innen, 'UEX Corp',
               t('s_dk_keine_lizenz'), t('s_dk_uex'), 'https://uexcorp.space')
    # StarStrings hat KEINE Lizenzangabe - kein LICENSE im Repo, nichts in
    # der readme, GitHub meldet keine (geprueft 29.08.2026). Hier stand
    # 'CC BY-NC-SA 4.0'. Das war geraten, vermutlich von scmdb uebernommen,
    # und es schrieb MrKraken eine Lizenz zu, die er nie vergeben hat.
    _dankblock(fenster, innen, 'StarStrings (MrKraken)',
               t('s_dk_keine_lizenz'),
               t('s_dk_ss'), 'https://starstrings.app')
    _dankblock(fenster, innen, 'SC Deutsch Launcher', t('s_dk_freiwillig'),
               t('s_dk_scdl'), 'https://www.sc-deutsch-launcher.de/')
    # ⚠⚠ Die Übersetzung selbst hat einen eigenen Urheber und eine eigene
    # Lizenz (CC BY-NC-SA 4.0). Die verlangt ausdrücklich Name UND Repository —
    # der Verteiler allein genügt nicht.
    _dankblock(fenster, innen, 'StarCitizen-Deutsch-INI (rjcncpt)',
               'CC BY-NC-SA 4.0', t('s_dk_ini'),
               'https://github.com/rjcncpt/StarCitizen-Deutsch-INI')

    # --- Menschen ---
    # ⚠ Aufklappbar, und zwar mit Absicht: Die Liste wird wachsen. der Autor am
    # 27.08.2026: „das werden später ja mal richtig viele, ich möchte schon alle
    # drauf haben aber nichts überladen." Sichtbar bleibt darum immer nur der
    # Name mit seiner Gruppe — was daraus geworden ist, steht eine Zeile tiefer
    # und nur auf Klick. So trägt die Seite auch fünfzig Namen noch.
    tk.Label(innen, text=t('s_dk_leute'), bg=BG, fg=FG, font=fenster.f_titel,
             anchor='w').pack(fill='x', pady=(18, 2))
    _fliesstext(innen, t('s_dk_leute_h'), fenster.f_klein, fill='x',
                pady=(0, 2))
    _fliesstext(innen, t('s_dk_aufklappen'), fenster.f_klein, farbe=SUB,
                fill='x', pady=(0, 10))

    for name, gruppe, idee, funde in (
            ('Haldjas', 'pr0', t('s_dk_haldjas_idee'),
             t('s_dk_haldjas_bugs')),
            # ⚠ Zwei Bausteine hintereinander: die Funde und der Nitro-Dank.
            # `_person` nimmt einen Text — also hier zusammensetzen, statt die
            # Funktion für einen Sonderfall umzubauen.
            ('Bomb20', 'pr0', t('s_dk_bomb_idee'), t('s_dk_bomb_bugs')),
            ('Morkhan', 'KRT', t('s_dk_morkhan_idee'),
             t('s_dk_morkhan_bugs')),
            ('Horthy', 'KRT', t('s_dk_horthy_idee'), ''),
            ('Bushwick4712', 'KRT', t('s_dk_bushwick_idee'), '')):
        _person(fenster, innen, name, gruppe, idee, funde)

    # --- Marken ---
    _fliesstext(innen, t('s_dk_marken'), fenster.f_klein, fill='x',
                pady=(18, 6))

    # --- Star Citizen Fan Content ---
    # ⚠ Gehoert ins Programm, nicht nur in die README: Wer ein Werkzeug
    # benutzt, liest die README meist nie. Der Wortlaut folgt dem Fankit
    # Agreement und dem UGC-Abschnitt der RSI-Nutzungsbedingungen.
    tk.Label(innen, text=t('s_dk_fankit_kopf'), bg=BG, fg=FG,
             font=fenster.f_grund, anchor='w').pack(fill='x', pady=(12, 2))
    _fliesstext(innen, t('s_dk_fankit'), fenster.f_klein, fill='x',
                pady=(0, 20))


def _ueber(fenster, rahmen):
    from . import pfade
    _ueberschrift(fenster, rahmen, t('hf_ueber'), t('s_ub_lead'))
    innen = _rollflaeche(rahmen)

    # --- Zustand ---
    # ⚠ Mit dem Programmsymbol daneben. Es stand hier nie — und seit der
    # Autor-Block mit dem Avatar auf „Danke & Lizenzen" gewandert ist, hatte die
    # Seite gar kein Bild mehr und wirkte nackt. Gemeldet am 27.08.2026: „bei
    # über muss oben zur Version noch das Watcher Logo (icon)".
    karte = _karte(innen, pady=(0, 6))
    kopf = tk.Frame(karte, bg=FLAECHE)
    kopf.pack(fill='x', padx=16, pady=(14, 6))
    from .hauptfenster import _mitgeliefert
    symbol = _mitgeliefert(os.path.join('assets', 'icon.png'))
    if symbol and os.path.exists(symbol):
        try:
            voll = tk.PhotoImage(file=symbol)
            # `subsample` verkleinert nur ganzzahlig — 48 px ist die Größe, die
            # neben zwei Textzeilen sitzt, ohne die Karte auseinanderzuziehen.
            teiler = max(1, voll.width() // 48)
            fenster._ueberlogo = voll.subsample(teiler, teiler)
            tk.Label(kopf, image=fenster._ueberlogo, bg=FLAECHE).pack(
                side='left', padx=(0, 14))
        except Exception as ausnahme:
            fehler.merken('seiten.ueber.symbol', ausnahme)
    titel = tk.Frame(kopf, bg=FLAECHE)
    titel.pack(side='left', fill='x', expand=True)
    tk.Label(titel, text='SC BP Watcher', bg=FLAECHE, fg=FG,
             font=fenster.f_titel, anchor='w').pack(fill='x')
    tk.Label(titel, text=fenster.version or '—', bg=FLAECHE, fg=ACCENT,
             font=fenster.f_fett, anchor='w').pack(fill='x')

    tk.Frame(karte, bg=FLAECHE, height=8).pack()
    _wertzeile(fenster, karte, t('s_ub_bekannt'), _zahl_katalog())
    _wertzeile(fenster, karte, t('s_ub_davon'), _zahl_bestand())
    uebersicht = {}
    try:
        uebersicht = pfade.uebersicht() or {}
    except Exception:
        pass
    _wertzeile(fenster, karte, t('b_ordner'),
               uebersicht.get('app_ordner') or '—')
    tk.Frame(karte, bg=FLAECHE, height=10).pack()

    # --- Einmal holen, ohne etwas umzustellen ---
    #
    # ⚠ Der häufigste Wunsch ist der einfachste: „gib mir die neueste, egal
    # welche". Bisher musste man dafür erst verstehen, was ein Kanal ist, und
    # den richtigen Kasten anklicken. Morkhan am 26.08.2026 dazu: „das ist
    # verwirrend" — er hatte den falschen gewählt und bekam gar nichts.
    #
    # ⚠ **Und er steht ganz oben, direkt unter der Versionskarte.** Vorher kam er
    # erst nach der Knopfreihe und dem Tagesschalter — bei der Mindestgröße des
    # Fensters lag er damit **unterhalb der Kante**. Gemeldet am 27.08.2026:
    # „das nervt User, weil die den Button zum Updaten nicht sofort finden."
    # Das Fenster größer zu machen wäre die falsche Antwort gewesen: Auf einem
    # 1366×768-Laptop passt es dann gar nicht mehr. Der wichtigste Knopf gehört
    # nach oben, nicht das Fenster in die Höhe.
    _fliesstext(innen, t('s_up_sofort_h'), fenster.f_klein,
                pady=(10, 6))
    _knopf(fenster, innen, t('s_up_sofort'),
           lambda: _fassung_holen(fenster, True),
           stark=True).pack(fill='x', pady=(0, 10))

    reihe = tk.Frame(innen, bg=BG)
    reihe.pack(fill='x', pady=(4, 4))
    _knopfreihe(reihe, [
        # ⚠ Nicht mehr "stark": Der hervorgehobene Knopf der Seite ist jetzt
        # der Hol-Knopf darüber. Zwei starke Knöpfe nebeneinander heben sich
        # gegenseitig auf — dann sticht keiner mehr hervor.
        _knopf(fenster, reihe, t('s_ub_nachsehen'),
               lambda: _jetzt_nachsehen(fenster)),
        _knopf(fenster, reihe, t('hf_wasistneu'),
               lambda: fenster.oeffnen('wasistneu')),
        _knopf(fenster, reihe, t('s_ub_einrichtung'), fenster._einrichtung),
    ])

    # --- Testkanal: zwei Kästen statt eines Schalters ---
    tk.Label(innen, text=t('s_ub_kanal'), bg=BG, fg=FG,
             font=fenster.f_titel, anchor='w').pack(fill='x', pady=(24, 2))
    _fliesstext(innen, t('s_ub_kanal_h'), fenster.f_klein,
                fill='x', pady=(0, 12))

    kaesten = tk.Frame(innen, bg=BG)
    kaesten.pack(fill='x')

    def kanal_setzen(wert):
        pfade.einstellung_setzen('vorabversionen', wert)
        fenster.sagen(t('e_vorab') + ': ' + (t('e_an') if wert else t('e_aus')))
        for kind in kaesten.winfo_children():
            kind.destroy()
        kanal_zeichnen()

    # Unterhalb dieser Breite stehen die beiden Kästen untereinander. 620 ist
    # gemessen, nicht geschätzt: Darunter reicht der Platz nicht mehr für zwei
    # nebeneinander, und Tk quetscht den zweiten zusammen, statt umzubrechen.
    SCHMAL = 620

    def kanal_zeichnen():
        an = pfade.einstellung_wahrheit('vorabversionen', False)
        breite = kaesten.winfo_width()
        # Vor dem ersten Zeichnen meldet Tk eine 1 — dann entscheidet das
        # Fenster, nicht der Platzhalter.
        if breite <= 1:
            breite = kaesten.winfo_toplevel().winfo_width()
        eng = breite < SCHMAL
        kaesten.zuletzt_eng = eng
        _kanalkasten(fenster, kaesten, t('s_ub_fertig'), t('s_ub_fertig_h'),
                     not an, lambda: kanal_setzen(False), untereinander=eng,
                     platz=0,
                     holen=lambda: _fassung_holen(fenster, False),
                     holen_text=_holen_text(False, fenster.version),
                     holen_aktiv=_holen_moeglich(False, fenster.version))
        _kanalkasten(fenster, kaesten, t('s_ub_test'), t('s_ub_test_h'),
                     an, lambda: kanal_setzen(True), marke_text='rc',
                     untereinander=eng, platz=1,
                     holen=lambda: _fassung_holen(fenster, True),
                     holen_text=_holen_text(True, fenster.version),
                     holen_aktiv=_holen_moeglich(True, fenster.version))
        # ⚠ Die Beschriftungen kommen aus dem Zwischenspeicher, damit die Seite
        # sofort steht. Der frischt sich aber nur einmal am Tag auf — auf einem
        # Bildschirmfoto vom 25.08.2026 bot der Knopf „v3.0.0-rc9 holen" an,
        # während rc12 lief und rc13 schon draußen war. Der Knopf holt zwar die
        # richtige Version (er sieht vorher nach), aber was draufsteht, führt in
        # die Irre. Deshalb einmal im Hintergrund nachsehen und die Kästen neu
        # zeichnen, wenn sich etwas geändert hat.
        _kanaele_auffrischen(fenster, kaesten, kanal_zeichnen)

    # ⚠ Der Tagesschalter steht **hinter** den Kanal-Kästen, nicht davor. Davor
    # drückte er die Kästen bei der Mindestgröße des Fensters unter die Kante —
    # und in ihnen sitzt der Knopf, mit dem man die stabile Version holt.
    # Gemeldet am 27.08.2026: „bei der stable version dann bitte auch."
    # Der Schalter ist eine Nebeneinstellung, die Kästen sind der Zweck der
    # Seite; also gehören sie nach oben.
    ziel = _feld(fenster, innen, t('s_ub_taeglich'), t('s_ub_taeglich_h'))
    _schalter(fenster, ziel, 'update_pruefen', True)

    def kanal_pruefen(_=None):
        """Nur neu bauen, wenn die Anordnung wirklich kippt — sonst flackert es."""
        eng = kaesten.winfo_width() < SCHMAL
        if eng != getattr(kaesten, 'zuletzt_eng', None):
            for kind in kaesten.winfo_children():
                kind.destroy()
            kanal_zeichnen()

    kanal_zeichnen()
    kaesten.bind('<Configure>', kanal_pruefen, add='+')



def _adresse(fenster, eltern, text, ziel, grund=None):
    """Eine anklickbare Adresse — öffnet den Browser.

    ⚠ Vorher war das ein gewöhnliches Label in der Akzentfarbe: Es **sah aus wie
    ein Link** und tat nichts. Das ist schlimmer als schwarzer Text, weil es zum
    Klicken einlädt. Jetzt ist der Mauszeiger eine Hand, die Adresse unterstreicht
    sich beim Überfahren, und ein Klick öffnet sie.
    """
    grund = grund or FLAECHE
    lbl = tk.Label(eltern, text=text, bg=grund, fg=ACCENT, font=fenster.f_klein,
                   anchor='w', cursor='hand2')
    lbl.pack(fill='x', pady=(4, 0))

    def oeffnen(_=None):
        # Die saubere Umgebung und der Rückfall auf `xdg-open` stecken jetzt in
        # `pfade.im_browser` — an EINER Stelle, damit nicht die Hälfte der
        # Verweise sie hat und die andere nicht. Genau daran hingen „Kaffee
        # spendieren" und „Discord" (30.08.2026 gemeldet).
        try:
            geklappt = pfade.im_browser(ziel)
        except Exception as ausnahme:
            fehler.merken('seiten.adresse', ausnahme, ziel)
            geklappt = False
        fenster.sagen(t('s_ub_auf') % ziel if geklappt else t('s_ub_auf_nein') % ziel)

    def rein(_=None):
        lbl.configure(font=_unterstrichen(fenster.f_klein))

    def raus(_=None):
        lbl.configure(font=fenster.f_klein)

    lbl.bind('<Button-1>', oeffnen)
    lbl.bind('<Enter>', rein)
    lbl.bind('<Leave>', raus)
    return lbl


def _unterstrichen(schrift):
    """Dieselbe Schrift, nur unterstrichen — für die Maus-über-Anzeige."""
    import tkinter.font as tkfont
    try:
        kopie = tkfont.Font(font=schrift)
        kopie.configure(underline=True)
        return kopie
    except tk.TclError:
        return schrift


def _schalter(fenster, eltern, schluessel, standard):
    """Ein An/Aus-Schalter, der sofort schreibt — es gibt keinen Speichern-Knopf."""
    from . import pfade
    k = tk.Label(eltern, text='', bg=FLAECHE, font=fenster.f_klein,
                 cursor='hand2', padx=10, pady=4)
    k.pack()

    def zeichnen():
        an = pfade.einstellung_wahrheit(schluessel, standard)
        k.configure(text=' %s ' % (t('e_an') if an else t('e_aus')),
                    fg=ACCENT if an else SUB)

    def umschalten():
        neu = not pfade.einstellung_wahrheit(schluessel, standard)
        pfade.einstellung_setzen(schluessel, neu)
        zeichnen()
        fenster.sagen(t('e_an') if neu else t('e_aus'))

    k.bind('<Button-1>', lambda e: umschalten())
    zeichnen()
    return k


def _erkennung(fenster, rahmen):
    from . import katalog as katalog_modul, pfade, phrasen
    _ueberschrift(fenster, rahmen, t('hf_erkennung'), t('s_er_lead'))
    innen = _rollflaeche(rahmen)

    ziel = _feld(fenster, innen, t('s_er_takt'), t('s_er_takt_h'))
    reihe = tk.Frame(ziel, bg=BG)
    reihe.pack()
    from .hauptfenster import rundes_feld
    zahl = rundes_feld(reihe, None, fenster.f_klein, '#0c1017', LINIE, ACCENT, FG,
                       breite=5, justify='right')
    zahl.insert(0, str(pfade.einstellung_zahl('pruefintervall_sekunden', 3, 1, 60)))
    zahl.halter.pack(side='left')
    tk.Label(reihe, text=t('s_er_sek'), bg=BG, fg=SUB,
             font=fenster.f_klein).pack(side='left')

    def takt_merken(_=None):
        try:
            pfade.einstellung_setzen('pruefintervall_sekunden',
                                     max(1, min(60, int(zahl.get()))))
            fenster.sagen(t('s_er_takt_sagen') % zahl.get())
        except ValueError:
            pass

    zahl.bind('<FocusOut>', takt_merken)
    zahl.bind('<Return>', takt_merken)

    # ⚠ `breit=True`: Die gefundenen Sätze sind lang. Rechts neben der
    # Beschreibung lief der Kasten über die Fensterkante hinaus und war an
    # beiden Enden abgeschnitten — lesbar war weder Anfang noch Ende.
    ziel = _feld(fenster, innen, t('s_er_satz'), t('s_er_satz_h'),
                 breit=True)
    # ⚠ `sammeln()` gibt ein Paar zurueck: die Liste der Saetze und woher sie
    # stammt. Wer das Paar einfach zusammenschreibt, bekommt rohe
    # Python-Schreibweise ins Fenster — eckige Klammern, Anfuehrungszeichen,
    # am Ende ein loses „tabelle". Genau so stand es dort.
    gefunden = '—'
    try:
        saetze, woher = phrasen.sammeln()
        gefunden = ' · '.join(str(x) for x in (saetze or [])) or '—'
    except Exception as ausnahme:
        fehler.merken('seiten.erkennung.phrasen', ausnahme)
    kasten = _karte(ziel)
    _fliesstext(kasten, gefunden, fenster.f_klein, farbe=FG,
                grund=FLAECHE, abzug=24, fill='x', padx=12, pady=8)

    ziel = _feld(fenster, innen, t('s_er_kat'), t('s_er_kat_h'))

    def katalog_neu():
        fenster.sagen(t('s_er_kat_holt'))
        try:
            katalog_modul.aktualisieren()
            fenster.sagen(t('s_er_kat_da') % _zahl_katalog())
        except Exception as ausnahme:
            fehler.merken('seiten.erkennung.katalog', ausnahme)
            fenster.sagen(t('s_er_kat_weg'))

    _knopf(fenster, ziel, t('s_er_kat_jetzt'), katalog_neu).pack()

    # ⚠⚠ **Hier stand bis v3.5.1 ein zweiter „Protokolle neu lesen"-Knopf.**
    # Er loeschte `logstand.json` und wirkte erst **beim naechsten Start**.
    # Unter „Bestand" gibt es denselben Auftrag als „Protokolle erneut
    # einlesen" — der ignoriert den Lesestand ebenfalls, geht jede Sicherung
    # UND die laufende `Game.log` durch, wirkt **sofort** und sagt hinterher,
    # was dabei herauskam.
    #
    # Der eine konnte also strikt weniger als der andere. Gemeldet am
    # 31.08.2026 von Haldjas: „unter detection macht es das nach dem naechsten
    # start, unter BP inventory sofort — ersteres ist wahrscheinlich dann nicht
    # mehr so sinnvoll?" Er hatte recht.
    #
    # ⚠ Zwei Knoepfe fuer eine Sache sind schlimmer als einer: Wer den
    # schwaecheren erwischt, glaubt, das Werkzeug koenne es nicht.


def _diagnose(fenster, rahmen):
    from . import pfade
    from .hauptfenster import schiebeschalter
    _ueberschrift(fenster, rahmen, t('hf_diagnose'), t('s_di_lead'))
    innen = _rollflaeche(rahmen)

    # ⭐ Wer meldet? Steht ÜBER dem Bericht, damit man sieht, was mitgeht.
    #
    # Anlass (29.08.2026): Das Werkzeug wurde im SCMDB-Discord vorgestellt
    # (620 Mitglieder). Ohne Absender lässt sich ein Bericht niemandem
    # zuordnen, und Rückfragen laufen ins Leere.
    #
    # ⚠ **Freiwillig und nie vorausgefüllt** — auch nicht mit dem
    # Benutzernamen des Systems. Das Werkzeug sammelt sonst nichts über den
    # Nutzer, und in der Ankündigung steht „no telemetry". Ein heimlich
    # mitgeschickter Name wäre ein Wortbruch.
    melder_var = tk.StringVar(value=(pfade.einstellung('melder_name') or ''))
    ziel_melder = _feld(fenster, innen, t('s_melder'), t('s_melder_h'))
    from .hauptfenster import rundes_feld
    melder_feld = rundes_feld(ziel_melder, melder_var, fenster.f_klein,
                              '#0c1017', LINIE, ACCENT, FG)
    melder_feld.halter.pack(fill='x', pady=(8, 0))

    text = ''
    try:
        text = bericht.bauen(version=fenster.version, wurzel=fenster.root)
    except Exception as ausnahme:
        fehler.merken('seiten.diagnose', ausnahme)

    from .hauptfenster import rundrahmen
    kasten = rundrahmen(innen, '#0c1017', LINIE, radius=8, grundfarbe=BG)
    kasten.halter.pack(fill='both', expand=True)
    # ⚠ `highlightthickness` steht bei Text und Entry auf 1 und wird auf dem
    # Mac als helle Linie gezeichnet — im runden Kasten sah das aus wie ein
    # zweiter, eckiger Rahmen. `relief='flat'` und `bd=0` schalten das NICHT ab.
    feld = tk.Text(kasten, bg='#0c1017', fg=FG, font=('Consolas', 10),
                   height=16, wrap='none', relief='flat', bd=0,
                   highlightthickness=0, insertbackground=FG, padx=14, pady=12)
    feld.pack(fill='both', expand=True)
    feld.insert('1.0', text)
    feld.configure(state='disabled')

    def melder_uebernehmen(*_):
        """Namen sichern und den Bericht neu aufbauen.

        ⚠ Der Bericht wird beim Öffnen der Seite EINMAL gebaut. Ohne dieses
        Auffrischen stünde der eben eingetippte Name nicht darin — man sähe
        „nicht angegeben" und hielte das Feld für kaputt."""
        neu_wert = melder_var.get().strip()
        if neu_wert == (pfade.einstellung('melder_name') or ''):
            return
        pfade.einstellung_setzen('melder_name', neu_wert)
        try:
            frisch = bericht.bauen(version=fenster.version, wurzel=fenster.root)
        except Exception as ausnahme:
            fehler.merken('seiten.diagnose_melder', ausnahme)
            return
        feld.configure(state='normal')
        feld.delete('1.0', 'end')
        feld.insert('1.0', frisch)
        feld.configure(state='disabled')

    melder_feld.bind('<FocusOut>', melder_uebernehmen)
    melder_feld.bind('<Return>', melder_uebernehmen)

    reihe = tk.Frame(innen, bg=BG)
    reihe.pack(fill='x', pady=(12, 0))

    def aktueller_bericht():
        """Genau das, was im Kasten steht — und vorher den Namen übernehmen.

        ⚠⚠ **Nicht die Fassung von vorhin.** Bis v3.3.0 arbeiteten alle vier
        Knöpfe mit `text`, dem Bericht, der beim **Öffnen der Seite** gebaut
        wurde. Wer seinen Namen eintippte, sah ihn zwar sofort im Kasten
        (`melder_uebernehmen` zeichnet ihn neu) — kopiert, gespeichert und
        gesendet wurde trotzdem die alte Fassung, also „Von: nicht angegeben".
        Genau so am 30.08.2026 passiert: Der Melder hatte seinen Namen
        eingetragen, im Bericht stand er nicht, und niemand konnte sich
        erklären, warum.

        Deshalb kommt der Text jetzt **aus dem Kasten**. Der Satz darunter
        verspricht „Du siehst vorher genau, was du verschickst" — dann muss
        auch genau das verschickt werden. Und der Name wird vorher übernommen,
        falls das Feld noch den Tastaturfokus hat.
        """
        melder_uebernehmen()
        return feld.get('1.0', 'end-1c')

    def melden():
        if bericht.issue_oeffnen(aktueller_bericht()):
            fenster.sagen(t('s_di_browser_ok'))
        else:
            fenster.sagen(t('s_di_browser_weg'))

    def kopieren():
        if bericht.in_die_ablage(aktueller_bericht(), fenster.root):
            fenster.sagen(t('s_di_kopiert'))

    def speichern():
        ziel_datei = bericht.speichern(aktueller_bericht())
        fenster.sagen(t('s_di_gespeichert') % os.path.basename(ziel_datei)
                      if ziel_datei else t('s_di_speich_weg'))

    def absenden():
        """Auf Knopfdruck an den Entwickler — mit vorheriger Rückfrage.

        ⚠ Der Weg für alle, die nicht basteln wollen. Kopieren und in Discord
        einfügen scheitert daran, dass der Bericht zu lang ist und man wissen
        muss, wohin damit. Gemeldet am 28.08.2026: „ich will nicht jedem eine
        Stunde erklären, wie ich zu dem Bericht komme."

        Gefragt wird trotzdem: Etwas ins Netz zu schicken, ohne dass jemand
        zugestimmt hat, macht dieses Werkzeug nicht.
        """
        from .hauptfenster import frage_stellen
        if not frage_stellen(fenster.root, t('s_di_ab_frage_t'),
                             t('s_di_ab_frage')):
            return
        fenster.sagen(t('s_di_ab_laeuft'))
        fenster.root.update_idletasks()
        geklappt, grund = bericht.absenden(aktueller_bericht(), fenster.version)
        fenster.sagen(t('s_di_ab_ok') if geklappt
                      else t('s_di_ab_weg') % grund)

    # ⚠ Ganz vorn und in Rot: Wer hier landet, hat ein Problem und sucht den
    # kürzesten Weg.
    #
    # ⚠ **Immer zeigen, auch ohne eingebautes Ziel.** Der erste Anlauf blendete
    # ihn aus, wenn nicht gesendet werden kann — gedacht als „ein Knopf, der
    # nichts tut, ist schlimmer als keiner". In der Praxis trifft das nur den
    # Quellcode, also den Entwickler selbst. am 28.08.2026 gemeldet vor der
    # Diagnose-Seite: „nicht mal ICH finde den." Ein Knopf, der fehlt, sieht aus
    # wie ein Fehler; einer, der beim Drücken sagt, was ihm fehlt, erklärt sich.
    _knopfreihe(reihe, [
        _knopf(fenster, reihe, t('s_di_absenden'), absenden, gefahr=True),
        _knopf(fenster, reihe, t('s_di_melden'), melden, stark=True),
        _knopf(fenster, reihe, t('s_di_kopieren'), kopieren),
        _knopf(fenster, reihe, t('s_di_speichern'), speichern),
        _knopf(fenster, reihe, t('s_di_ordner'),
               lambda: _ordner_zeigen(pfade.app_ordner())),
    ])

    _status(fenster, innen, 'haken', t('s_di_sicher'), t('s_di_sicher_h'))

    ziel = _feld(fenster, innen, t('s_di_mit'), t('s_di_mit_h'))

    def mitschreiben_um():
        neu_wert = not pfade.einstellung_wahrheit('fehler_mitschreiben', True)
        pfade.einstellung_setzen('fehler_mitschreiben', neu_wert)
        return neu_wert

    schiebeschalter(ziel, pfade.einstellung_wahrheit('fehler_mitschreiben', True),
                    mitschreiben_um).pack()



def _zahl_katalog():
    try:
        return len((katalog_modul.laden().get('bauplaene') or {}))
    except Exception:
        return '—'


def _zahl_bestand():
    try:
        return len((bestand_datei.laden().get('bauplaene') or {}))
    except Exception:
        return '—'


# --------------------------------------------------------------- Herstellung
#
# ⚠ **Nicht scmdb nachbauen.** Die Seite beantwortet genau eine Frage: „Ich will
# das bauen — was brauche ich?" Keine Wahrscheinlichkeits-Balken, kein
# Refinery-Vergleich; wer das braucht, ist auf scmdb.net besser aufgehoben.
# Was diese Seite dagegen kann und die Webseite nicht: Sie **weiß**, welche
# Baupläne der Spieler hat.

HERST_MAX = 150          # so viele Zeilen auf einmal — mehr macht Tk zäh


def _dauer(sekunden):
    """Herstellzeit lesbar: 45 s · 16 min · 2 h 30 min."""
    sekunden = int(sekunden or 0)
    if sekunden < 60:
        return t('s_he_sekunden') % sekunden
    if sekunden < 3600:
        return t('s_he_minuten') % round(sekunden / 60.0)
    return t('s_he_std_min') % (sekunden // 3600, (sekunden % 3600) // 60)


def _herstellung(fenster, rahmen):
    """Alle herstellbaren Gegenstände, mit Rezept auf Klick."""
    from . import herstellung as herst_modul
    _ueberschrift(fenster, rahmen, t('hf_herstellung'), t('s_he_lead'))
    innen = _rollflaeche(rahmen)

    try:
        habe = bestand_datei.schluessel(bestand_datei.laden())
        eintraege = herst_modul.mit_bestand(habe)
        sicher, gesamt, unklar = herst_modul.zaehlung(habe)
    except Exception as ausnahme:
        fehler.merken('seiten.herstellung', ausnahme)
        eintraege, sicher, gesamt, unklar = [], 0, 0, 0

    if not eintraege:
        _fliesstext(innen, t('s_he_keine_daten'), fenster.f_klein, fill='x')
        return

    # Kopfzahl im selben Aufbau wie der Bauplan-Fortschritt — wer die eine
    # Seite kennt, liest die andere sofort.
    kopf = tk.Frame(innen, bg=BG)
    kopf.pack(fill='x', pady=(0, 4))
    tk.Label(kopf, text=str(sicher), bg=BG, fg=ACCENT,
             font=fenster.f_titel).pack(side='left')
    tk.Label(kopf, text=t('s_he_von') % gesamt, bg=BG, fg=SUB,
             font=fenster.f_klein).pack(side='left')
    # ⚠⚠ **Die unklaren gehören dazu, sonst fehlt eine Zahl ohne Erklärung.**
    # `zaehlung()` gibt sie längst zurück, angezeigt wurden sie nie: Ein
    # Bauplan, dessen Name mehrere Gegenstände meint (Idris- und
    # Reclaimer-Kraftwerk, BroadSpec in zwei Größen), zählt bewusst nicht als
    # „sicher" — richtig so, ein falsch zugeordneter Bauplan wäre schlimmer.
    #
    # Nur stand oben dann eine Zahl, die **kleiner ist als der eigene
    # Bestand**, und nichts sagte warum. Gemeldet als „404 von 1597" bei 405
    # Bauplänen; der Hinweis dazu (`s_he_unklar`) steht bisher erst am
    # aufgeklappten Eintrag — also genau dort, wo man ihn nur findet, wenn man
    # schon weiß, wonach man sucht.
    if unklar:
        tk.Label(kopf, text=t('s_he_dazu_unklar') % unklar, bg=BG, fg=SUB,
                 font=fenster.f_klein).pack(side='left')

    from .hauptfenster import rundbalken, rundes_feld
    rundbalken(innen, 9, sicher / float(gesamt or 1), BG, '#222b3b',
               ACCENT).pack(fill='x', pady=(6, 14))

    # ⚠ Beschriftetes Feld wie auf den anderen Seiten (siehe „Dein Name" auf
    # der Diagnose-Seite) — **nicht** das nackte Suchfeld aus der Werkzeugleiste
    # der Bauplan-Liste. Dort gibt die Leiste den Kontext, hier gäbe ein leeres
    # Kästchen mitten auf der Seite keinen Hinweis, wofür es da ist.
    suche_var = tk.StringVar()
    ziel_suche = _feld(fenster, innen, t('s_he_suche'), '')
    suchfeld = rundes_feld(ziel_suche, suche_var, fenster.f_klein, '#0c1017',
                           LINIE, ACCENT, FG)
    suchfeld.halter.pack(fill='x', pady=(4, 12))
    # ⚠ Gleiches Bedienelement wie beim Bergbau. Zwei Suchfelder, die sich
    # unterschiedlich verhalten, sind schlimmer als eines ohne Kreuz.
    _suche_leeren_kreuz(fenster, ziel_suche, suche_var)
    def _herst_frisch():
        """Beim erneuten Aufrufen ohne Filter anfangen.

        ⚠⚠ **Nur wenn wirklich etwas gesetzt war.** Sonst baut jeder Wechsel
        auf die Herstellungs-Seite die 1597 Zeilen neu auf, ohne dass sich
        etwas ändert — dieselbe Bremse wie in der Bauplan-Liste
        (`bestandsfenster._fein_leeren`), am 31.08.2026 gemessen und gemeldet.
        """
        etwas_gesetzt = bool(suche_var.get() or any(wahl.values())
                             or _material_merker)
        if not etwas_gesetzt:
            # ⚠ Auch `set('')` nicht: Das loest den `trace` aus und zeichnet
            # damit die ganze Liste neu — genau das, was hier vermieden wird.
            return
        suche_var.set('')
        for schluessel in wahl:
            wahl[schluessel] = ''
        _material_merker.clear()
        filter_bauen()
        zeichnen()

    fenster.beim_zeigen['herstellung'] = _herst_frisch

    # --- Filter, dieselben Bedienelemente wie in der Bauplan-Liste ----------
    # ⚠ „egal wo, sollte das Bedienkonzept nicht jedes Mal ändern — die Leute
    # wollen es nutzen und nicht erst lernen, wie sie es nutzen." (29.08.2026)
    #
    # ⚠ Die Werte kommen aus den **vorhandenen** Einträgen, nicht aus einer
    # festen Liste. Bringt ein Patch eine neue Waffenart, steht sie am nächsten
    # Tag im Feld, ohne dass jemand etwas nachträgt.
    wahl = {'art': '', 'unterart': '', 'hersteller': '', 'zustand': '',
            'material': ''}

    # ⚠⚠ **Dieselbe Gliederung wie in der Bauplan-Liste.** Xharig:
    # „BP und Herstellung sind ja die gleichen BP, also muss man auf die
    # gleiche Art suchen." Beide Seiten fragen dasselbe Modul — wer hier eine
    # eigene Einteilung baute, hätte zwei Wahrheiten über dieselben Daten.
    from . import kategorien as kat_modul
    from . import katalog as kat_daten

    _kat_arten = {}
    try:
        for _k, _v in (kat_daten.laden().get('bauplaene') or {}).items():
            _kat_arten[herst_modul._schluessel(_v.get('n') or '')] = _v.get('a') or ''
    except Exception as ausnahme:
        fehler.merken('seiten.herstellung.katalog', ausnahme)

    _kat_merker = {}

    def _kategorie(e):
        name = e.get('basis') or e.get('name') or ''
        if name in _kat_merker:
            return _kat_merker[name]
        b = herst_modul.rezept_roh(name) or {}
        wert = kat_modul.einordnen(
            art=_kat_arten.get(herst_modul._schluessel(name), ''),
            tag=b.get('tag') or '',
            unterart=e.get('unterart') or '',
            rezeptart=e.get('art') or '')
        _kat_merker[name] = wert
        return wert

    def _werte(feld):
        return sorted({(e.get(feld) or '') for e in eintraege} - {''},
                      key=str.lower)

    def _oberkategorien():
        """Die Oberkategorien mit Anzahl — Gruppen zuerst, Einzelgänger danach."""
        zaehler = {}
        for e in eintraege:
            o, _u = _kategorie(e)
            if o:
                zaehler[o] = zaehler.get(o, 0) + 1
        raus = []
        for o, n in zaehler.items():
            name = kat_modul.obername(o)
            if not kat_modul.ist_gruppe(o):
                name = kat_daten.art_lesbar(kat_modul.rohe_art(o)) or name
            raus.append((o, '%s (%d)' % (name, n), kat_modul.ist_gruppe(o), name))
        raus.sort(key=lambda p: (not p[2], p[3].lower()))
        return [(o, b) for o, b, _g, _n in raus]

    def _unterarten_zur_art(ober):
        """Nur die Unterarten der gewählten Oberkategorie.

        Ohne die Einschränkung stünde „Laserkanone" neben „Helm" neben
        „Magazin" — wieder die lange Liste, die zwei Ebenen gerade abschaffen."""
        if not ober:
            return []
        zaehler = {}
        for e in eintraege:
            o, u = _kategorie(e)
            if o != ober or not u:
                continue
            zaehler[u] = zaehler.get(u, 0) + 1
        return [(u, '%s (%d)' % (kat_modul.untername(u), n))
                for u, n in sorted(zaehler.items(),
                                   key=lambda q: kat_modul.untername(q[0]).lower())]

    filter_rahmen = tk.Frame(innen, bg=BG)
    filter_rahmen.pack(fill='x')

    def filter_bauen():
        for w in filter_rahmen.winfo_children():
            w.destroy()
        unterarten = _unterarten_zur_art(wahl['art'])
        # Das leere Feld nennt die Zahl — sonst findet niemand, dass es hier
        # weitergeht.
        unter_text = (t('ff_unterart_waehlen') % len(unterarten) if unterarten
                      else t('ff_alle_unterarten'))
        felder = [
            ('art', t('ff_alle_arten'), _oberkategorien()),
            ('unterart', unter_text, unterarten),
            ('hersteller', t('ff_alle_hersteller'),
             [(h_, h_) for h_ in _werte('hersteller')]),
            ('zustand', t('ff_alle_zustaende'),
             [('habe', t('ff_zustand_habe')), ('fehlt', t('ff_zustand_fehlt'))]),
            # ⭐ „Was kann ich gerade wirklich bauen?" — gerechnet gegen das
            # eigene Lager. ⚠ Der Watcher kennt den Frachtraum nicht; er
            # rechnet mit der von Hand gepflegten Liste, und das steht auch
            # oben auf der Seite.
            ('material', t('ff_alle_material'),
             [('reicht', t('ff_material_reicht')),
              ('fehlt', t('ff_material_fehlt'))]),
        ]
        _filterleiste(fenster, filter_rahmen, felder, gewechselt, wahl)

    def gewechselt():
        # Eine Unterart, die zur neuen Art nicht passt, muss weg — sonst
        # filtert man auf etwas, das es in dieser Art gar nicht gibt.
        # ⚠ `_unterarten_zur_art()` liefert **Paare** (Wert, Beschriftung) —
        # der Vergleich gegen die rohe Liste traf deshalb nie zu, und jede
        # gewählte Unterart wurde sofort wieder geleert: „klicke ich sie an,
        # ist nichts ausgewählt" (29.08.2026).
        gueltig = [u for u, _b in _unterarten_zur_art(wahl['art'])]
        if wahl['unterart'] and wahl['unterart'] not in gueltig:
            wahl['unterart'] = ''
        filter_bauen()
        zeichnen()

    filter_bauen()

    liste_rahmen = tk.Frame(innen, bg=BG)
    liste_rahmen.pack(fill='both', expand=True)

    # Welche Zeile ist gerade aufgeklappt? Eine reicht — zwei offene Rezepte
    # untereinander sind schon wieder die Zettelwirtschaft, die der Umschalter
    # vermeiden soll.
    offen = {'name': None}

    _material_merker = {}

    def _material_reicht(e):
        """Reicht das Lager für die erste Stufe dieses Bauplans?

        ⚠ Einmal je Bauplan gerechnet und gemerkt. Ohne das würde bei jedem
        Filterklick für 1597 Einträge das Lager durchgegangen.
        """
        name = e.get('basis') or e.get('name') or ''
        if name in _material_merker:
            return _material_merker[name]
        wert = False
        try:
            rez = herst_modul.rezept(name) or {}
            stufen = rez.get('stufen') or []
            if stufen:
                from . import rohstoffe as lager_modul
                fehlt = [z for z in lager_modul.pruefen(stufen[0]['zutaten'])
                         if z[3]]          # z[3] = „fehlt"
                wert = not fehlt
        except Exception:
            wert = False
        _material_merker[name] = wert
        return wert

    def passt(e):
        if wahl['art'] or wahl['unterart']:
            ober, unter = _kategorie(e)
            if wahl['art'] and ober != wahl['art']:
                return False
            if wahl['unterart'] and unter != wahl['unterart']:
                return False
        if wahl['hersteller'] and (e.get('hersteller') or '') != wahl['hersteller']:
            return False
        # ⚠ `habe` kann None sein („unklar", drei mehrdeutige Namen). Unklares
        # gilt weder als vorhanden noch als fehlend — sonst behaupten wir etwas.
        if wahl['zustand'] == 'habe' and e.get('habe') is not True:
            return False
        if wahl['zustand'] == 'fehlt' and e.get('habe') is not False:
            return False
        if wahl['material']:
            reicht = _material_reicht(e)
            if wahl['material'] == 'reicht' and not reicht:
                return False
            if wahl['material'] == 'fehlt' and reicht:
                return False
        return True

    def zeichnen(*_):
        for w in liste_rahmen.winfo_children():
            w.destroy()
        text = suche_var.get().strip().lower()

        # ⭐⭐ **Auch nach der ZUTAT suchen.** Bis v3.3.0-rc40 sah die Suche
        # nur auf Bauplan-Namen. Wer „ric" tippte, um zu sehen, was aus Riccite
        # wird, bekam „Lo*ric*a" und „Fab*ric*ation" — Zufallstreffer — und nie
        # die 84 Baupläne, die Riccite wirklich brauchen. Am 30.08.2026
        # gemeldet: „Was kann ich aus Sadaryx herstellen? Meine User werden es
        # nie erfahren."
        material_treffer, aus_material = [], set()
        if text:
            for name_ in herst_modul.einlagerbar():
                if text in name_.lower():
                    material_treffer.append(name_)
                    aus_material.update(herst_modul.bauplaene_mit(name_))

        treffer = [e for e in eintraege
                   if passt(e)
                   and (not text
                        or text in e['name'].lower()
                        or text in (e['hersteller'] or '').lower()
                        or text in (e['art'] or '').lower()
                        or e['name'] in aus_material)]

        # ⚠ Eine **leere Liste ist auch eine Antwort** — und oft die richtige:
        # 26 der 52 einlagerbaren Namen kommen in keinem Rezept vor, alle 13
        # Pflanzen darunter. Das muss dastehen, sonst sucht jemand weiter.
        for name_ in material_treffer[:3]:
            anzahl = len(herst_modul.bauplaene_mit(name_))
            _fliesstext(liste_rahmen,
                        (t('s_he_aus') % (name_, anzahl) if anzahl
                         else t('s_he_aus_keine') % name_),
                        fenster.f_klein, fill='x')

        if not treffer:
            if not material_treffer:
                _fliesstext(liste_rahmen, t('s_he_nichts'), fenster.f_klein,
                            fill='x')
            return
        for e in treffer[:HERST_MAX]:
            _herstellung_zeile(fenster, liste_rahmen, e, offen, zeichnen)
        if len(treffer) > HERST_MAX:
            _fliesstext(liste_rahmen, t('s_he_mehr') % (len(treffer) - HERST_MAX),
                        fenster.f_klein, fill='x')

    suche_var.trace_add('write', zeichnen)
    zeichnen()


def _geld(betrag):
    """Ein Geldbetrag mit Tausenderpunkten — 22700 wird zu „22.700".

    ⚠ Ohne Trennung liest niemand fünfstellige Zahlen richtig: „145789" und
    „14578" sehen im Vorbeigehen gleich aus. Punkt statt Komma, weil das Spiel
    es so schreibt.
    """
    return '{:,.0f}'.format(float(betrag or 0)).replace(',', '.')


def _hat_herkunft(name):
    """Kennt der Katalog diesen Bauplan — und weiss er, woher es ihn gibt?

    ⚠ Beides zusammen. Ein Katalogeintrag ohne `q` hat keine Bezugsquelle
    (59 Bauplaene, ueberwiegend Event-Belohnungen); ein Knopf dorthin fuehrte
    zu einer Seite, die nichts zu sagen hat.
    """
    if not name:
        return False
    try:
        from . import katalog as kat
        gesucht = bestand_datei.norm(name)
        for e in (kat.laden().get('bauplaene') or {}).values():
            if bestand_datei.norm(e.get('n') or '') == gesucht:
                return bool(e.get('q'))
    except Exception:
        pass
    return False


def _zum_auftrag(fenster, titel):
    """Von „Was bringt am meisten?" zur Bauplan-Liste, auf diesen Auftrag."""
    try:
        fenster.oeffnen('liste')
        seite = getattr(fenster, 'bestandsseite', None)
        if seite is not None and seite.zum_auftrag(titel):
            return
        fenster.sagen(t('s_fo_lohnt_nichts'))
    except Exception as ausnahme:
        fehler.merken('seiten.zum_auftrag', ausnahme)


def _zum_bauplan(fenster, name):
    """Von der Herstellung zur Bauplan-Liste — mit aufgeschlagener Herkunft."""
    try:
        fenster.oeffnen('liste')
        seite = getattr(fenster, 'bestandsseite', None)
        if seite is not None and seite.zum_bauplan(name):
            return
        fenster.sagen(t('s_he_woher_nichts'))
    except Exception as ausnahme:
        fehler.merken('seiten.zum_bauplan', ausnahme)
        fenster.sagen(t('s_he_woher_nichts'))


def _herstellung_zeile(fenster, eltern, eintrag, offen, neu_zeichnen):
    """Eine Zeile der Herstellungs-Liste, auf Klick klappt das Rezept auf."""
    from . import herstellung as herst_modul
    zeile = tk.Frame(eltern, bg=BG, cursor='hand2')
    zeile.pack(fill='x', pady=1)

    # Drei Zustände, nicht zwei: habe / fehlt / **unklar**.
    if eintrag['habe'] is True:
        zeichen_text, farbe = '✓', ACCENT
    elif eintrag['habe'] is None:
        zeichen_text, farbe = '?', GOLD
    else:
        zeichen_text, farbe = '·', SUB
    tk.Label(zeile, text=zeichen_text, bg=BG, fg=farbe, font=fenster.f_grund,
             width=2).pack(side='left')
    # ⚠⚠ **Die aufgeklappte Zeile muss sich abheben.** Am 31.08.2026 gemeldet:
    # „nicht klar genug, welcher Bauplan bei Herstellung ausgewaehlt ist,
    # steht auch nirgends." Sie sah aus wie jede andere — und sobald man ein
    # Stueck gerollt hatte, war der Name oben aus dem Bild.
    _offen = offen['name'] == eintrag['name']
    tk.Label(zeile, text=eintrag['name'], bg=BG,
             fg=ACCENT if _offen else FG,
             font=fenster.f_fett if _offen else fenster.f_grund,
             anchor='w').pack(side='left', fill='x', expand=True)
    if eintrag['hersteller']:
        tk.Label(zeile, text=eintrag['hersteller'], bg=BG, fg=SUB,
                 font=fenster.f_klein, anchor='e').pack(side='right', padx=(8, 0))

    def umschalten(*_):
        offen['name'] = None if offen['name'] == eintrag['name'] else eintrag['name']
        neu_zeichnen()

    for w in (zeile,) + tuple(zeile.winfo_children()):
        w.bind('<Button-1>', umschalten)

    if offen['name'] != eintrag['name']:
        return

    # --- aufgeklappt: das Rezept ---
    block = tk.Frame(eltern, bg='#0c1017')
    block.pack(fill='x', padx=(24, 0), pady=(2, 8))

    # ⚠⚠ **Der Name noch einmal, ueber dem Rezept.** Der Kasten ist lang —
    # Zutaten, Herstellzeit, Qualitaetsregler, Werte. Wer bis dorthin gerollt
    # hat, sieht die Zeile mit dem Namen nicht mehr und weiss nicht, wovon er
    # gerade die Zutaten liest. Der Hersteller steht daneben, weil „5SA
    # 'Rhada'" allein niemandem sagt, worum es geht.
    _kopf = tk.Frame(block, bg='#0c1017')
    _kopf.pack(fill='x', padx=12, pady=(10, 0))
    tk.Label(_kopf, text=eintrag['name'], bg='#0c1017', fg=ACCENT,
             font=fenster.f_fett, anchor='w').pack(side='left')
    if eintrag['hersteller']:
        tk.Label(_kopf, text='  ·  %s' % eintrag['hersteller'], bg='#0c1017',
                 fg=SUB, font=fenster.f_klein, anchor='w').pack(side='left')

    if eintrag['habe'] is None:
        _fliesstext(block, t('s_he_unklar'), fenster.f_klein, fill='x')

    # ⚠⚠ **„Ich kann das nicht bauen — woher bekomme ich den Bauplan?"**
    # Gewuenscht von Bushwick4712 (KRT) am 31.08.2026. Die Antwort stand schon
    # im Werkzeug, aber auf einer anderen Seite und hinter einem Symbol: Man
    # musste wissen, dass es sie gibt, und den Namen von Hand hinuebertippen.
    #
    # ⚠ **Nur wenn der Bauplan fehlt.** Wer ihn hat, will hier bauen und nicht
    # wissen, wo es ihn gaebe — der Knopf waere dann nur eine Zeile mehr.
    # ⚠ **Und nur, wenn dahinter wirklich etwas steht.** Der Katalog kennt 738
    # Bauplaene, die Rezepte sind 1607; ein Knopf, der auf eine leere Liste
    # fuehrt, ist schlimmer als keiner.
    if eintrag['habe'] is not True and _hat_herkunft(eintrag.get('basis')):
        _knopf(fenster, block, t('s_he_woher_bp'),
               lambda n=eintrag.get('basis'): _zum_bauplan(fenster, n)).pack(
                   anchor='w', padx=12, pady=(8, 0))
    rez = herst_modul.rezept(eintrag['basis'])
    from . import rohstoffe as lager
    from . import preise as preis_modul
    for stufe in (rez or {}).get('stufen') or []:
        # ⭐ Was davon liegt im eigenen Lager? (Vorschlag von Horthy (KRT))
        # ⚠ Gezeigt wird „hast du" bzw. „dir fehlt" — **nie** „du kannst nicht
        # bauen". Das Lager wird von Hand gepflegt und ist irgendwann
        # lückenhaft; ein Hinweis darf danebenliegen, eine Behauptung nicht.
        # ⚠ Die Lage wird jetzt bei JEDER Änderung der Stückzahl neu gerechnet
        # (siehe `mengen_setzen` weiter unten) — deshalb hier nur der
        # Startwert für ein Stück.

        # ⭐ **Der Knopf steht GANZ OBEN.** Er stand bis zum 29.08.2026 unter
        # den Zutaten, der Herstellzeit UND dem Block „Mit deinem Material" —
        # bei drei Zutaten also gut zehn Zeilen tiefer. Xharig hat ihn selbst
        # nicht gefunden: „wenn selbst ich es nicht verstehe". Eine Funktion,
        # die man suchen muss, ist für den Nutzer nicht vorhanden.
        reihe = tk.Frame(block, bg='#0c1017')
        reihe.pack(fill='x', padx=12, pady=(8, 2))
        rueck = tk.Label(reihe, text='', bg='#0c1017', fg=SUB,
                         font=fenster.f_klein, anchor='w')

        # ⭐ Stückzahl daneben. Wer zehn Stück am Stück baut, soll einmal
        # klicken statt zehnmal — beim elften Klick stimmt der Bestand sonst
        # nicht mehr, und niemand merkt es.
        anzahl_var = tk.StringVar(value='1')

        def hergestellt(_e=None, zutaten=stufe['zutaten'], lbl=rueck,
                        var=anzahl_var):
            wie_oft = lager.zahl_lesen(var.get())
            # Unsinn im Feld heisst 1 — lieber einmal abziehen als gar nichts
            # tun und den Nutzer raten lassen, warum nichts passiert.
            wie_oft = 1 if not wie_oft or wie_oft < 1 else int(wie_oft)
            ok, fehlt = lager.abziehen(zutaten, wie_oft)
            if ok:
                text = (t('s_lg_abgezogen') if wie_oft == 1
                        else t('s_lg_abgezogen_n') % wie_oft)
            else:
                # ⚠ Mit der Fehlmenge, nicht nur dem Namen. „Es fehlt: Iron"
                # lässt einen raten, ob 0,1 oder 10 fehlen — und genau danach
                # richtet sich, ob man losfliegt.
                text = t('s_lg_teilweise') % ', '.join(
                    t('s_lg_fehlt_paar') % (name, round(menge, 3))
                    for name, menge in fehlt)
            lbl.configure(text=text, fg=ACCENT if ok else GOLD)
            # ⚠ Die Stückzahl bleibt NUR stehen, wenn nichts abgezogen wurde —
            # dann will man sie berichtigen, nicht neu tippen. Nach einem
            # erfolgreichen Abzug zurück auf 1, damit der nächste Klick nicht
            # unbemerkt wieder zehn nimmt.
            if ok:
                var.set('1')
                neu_zeichnen()
            else:
                mengen_setzen()

        # ⚠ **Was beim Zerlegen NICHT zurueckkommt.** Sechs Rohstoffe stehen
        # auf CIGs Sperrliste (Lindinium, Quantainium, Riccite, Ouratite,
        # Stileron, Savrilium) — wer daraus baut, bekommt sie nie wieder
        # heraus. Das aendert die Rechnung und gehoert deshalb ans Rezept,
        # nicht in eine Fussnote. Steht in denselben Rezeptdaten
        # (`dismantle.blacklistedResources`), kostet also keinen Abruf.
        # ⚠⚠ **Der dritte Rückgabewert hieß hier `_dauer` — und überschrieb
        # damit die Funktion `_dauer()` weiter oben in dieser Datei.** Ab da
        # war `_dauer` eine Zahl, und `_dauer(stufe['zeit'])` ein paar Zeilen
        # später warf `TypeError: 'int' object is not callable`. Sichtbar wurde
        # das als **verschwundener Qualitäts-Block**: Die Ausnahme brach den
        # Aufbau mitten drin ab, die Herstellzeit blieb ohne Wert und alles
        # danach — Regler, Wirkungen, Hinweise — fehlte ersatzlos. In rc37 und
        # rc38 ausgeliefert. Nie einen lokalen Namen vergeben, den es in
        # dieser Datei schon als Funktion gibt.
        try:
            _sperre, _wirkung, _zerlege_sekunden = herst_modul.zerlege_sperre()
        except Exception:
            _sperre, _wirkung = set(), 0.5
        _betroffen = [r for _s, r, _m, _g in stufe['zutaten']
                      if r and lager.norm_rohstoff(r) in
                      {lager.norm_rohstoff(x) for x in _sperre}]
        if _betroffen:
            _fliesstext(block, t('s_he_zerlegen') % (_wirkung * 100,
                                                     ', '.join(dict.fromkeys(_betroffen))),
                        fenster.f_klein, fill='x')

        _knopf(fenster, reihe, t('s_lg_bauen'), hergestellt).pack(side='left')
        tk.Label(reihe, text=t('s_lg_anzahl'), bg='#0c1017', fg=SUB,
                 font=fenster.f_klein).pack(side='left', padx=(12, 6))
        from .hauptfenster import rundes_feld as _rf_anzahl
        _anzahl_feld = _rf_anzahl(reihe, anzahl_var, fenster.f_klein,
                                  '#0c1017', LINIE, ACCENT, FG)
        _anzahl_feld.halter.configure(width=70)
        _anzahl_feld.halter.pack(side='left')
        rueck.pack(side='left', padx=(10, 0))
        # Eine Zeile, die sagt, was der Knopf tut — sonst rät man.
        _fliesstext(block, t('s_lg_bauen_hilfe'), fenster.f_klein, fill='x')

        # ⚠⚠ **Die Zutatenzeilen werden EINMAL gebaut, danach nur neu
        # beschriftet.** Sie hängen an der Stückzahl, und die ändert sich beim
        # Tippen. Würde bei jedem Tastendruck die Seite neu aufgebaut, verlöre
        # das Stückzahl-Feld den Cursor — derselbe Fehler wie im Lager-Suchfeld
        # (v3.3.0-rc21). Also: Widgets stehen lassen, nur `configure(text=…)`.
        #
        # Aus demselben Grund werden ALLE Etiketten angelegt, auch die für
        # „dir fehlt" und „zu schlechte Qualität". Sie werden je nach Lage
        # ein- und ausgeblendet statt neu erzeugt — sonst springt die Höhe.
        zutat_widgets = []
        for slot, rohstoff, menge, guete in stufe['zutaten']:
            z = tk.Frame(block, bg='#0c1017')
            z.pack(fill='x', padx=12, pady=1)
            tk.Label(z, text=slot, bg='#0c1017', fg=SUB, font=fenster.f_klein,
                     width=18, anchor='w').pack(side='left')
            # ⭐ Der Sprung: Klick auf den Rohstoff öffnet den Bergbau mit
            # diesem Namen in der Suche. Das ist der Grund, warum die
            # Detailfläche kurz bleiben darf — man springt, statt zu stapeln.
            roh_lbl = tk.Label(z, text=rohstoff, bg='#0c1017', fg=ACCENT,
                               font=fenster.f_grund, anchor='w',
                               cursor='hand2')
            roh_lbl.pack(side='left')

            def zum_bergbau(_e=None, name=rohstoff):
                fenster.bergbau_suche = name
                fenster.oeffnen('bergbau')

            roh_lbl.bind('<Button-1>', zum_bergbau)
            menge_lbl = tk.Label(z, text='', bg='#0c1017', fg=SUB,
                                 font=fenster.f_klein, anchor='e')
            menge_lbl.pack(side='right', padx=12)
            lage_lbl = tk.Label(z, text='', bg='#0c1017', fg=GOLD,
                                font=fenster.f_klein, anchor='e')
            guete_lbl = tk.Label(z, text='', bg='#0c1017', fg=SUB,
                                 font=fenster.f_klein, anchor='e')
            # ⭐ „kaufen oder abbauen?" — die Frage, die nach „dir fehlt X"
            # kommt. Sieben der 26 Rohstoffe lassen sich NIRGENDS kaufen; fünf
            # davon stehen zusätzlich auf der Zerlege-Sperrliste. Wer das nicht
            # weiß, sucht am Terminal nach etwas, das es dort nie gibt.
            preis_lbl = tk.Label(z, text='', bg='#0c1017', fg=SUB,
                                 font=fenster.f_klein, anchor='e')
            zutat_widgets.append((rohstoff, menge, menge_lbl, lage_lbl,
                                  guete_lbl, preis_lbl))

        def mengen_setzen(*_):
            """Mengen und Lage neu beschriften — für die aktuelle Stückzahl.

            ⚠ **Hier steckt der Grund, warum es die Funktion gibt.** Bis
            v3.3.0-rc35 zeigte die Zutatenliste immer den Bedarf für EIN
            Stück. Wer 10 eintippte, sah weiter „1.16 SCU" und „dir fehlt
            1.16" — obwohl 11,6 gebraucht wurden. Der Abzug rechnete richtig,
            die Anzeige log. Am 30.08.2026 gemeldet.
            """
            wie_viele = lager.zahl_lesen(anzahl_var.get())
            wie_viele = 1 if not wie_viele or wie_viele < 1 else int(wie_viele)
            neue_lage = {m: (br, da, f, zug, mq) for m, br, da, f, zug, mq
                         in lager.pruefen(stufe['zutaten'], wie_viele)}
            for (rohstoff, menge, menge_lbl, lage_lbl, guete_lbl,
                 preis_lbl) in zutat_widgets:
                noetig = (menge or 0) * wie_viele
                menge_lbl.configure(
                    text=(t('s_he_menge') % noetig if wie_viele == 1
                          else t('s_he_menge_n') % (noetig, menge, wie_viele)))
                _br, _da, _fehlt, _zu_gering, _mindestq = neue_lage.get(
                    rohstoff, (0, 0, 0, 0, 0))
                if _fehlt > 0:
                    # Liegt schon etwas da, gehört das dazu — sonst fliegt
                    # jemand los, um 0,09 zu holen, obwohl ihm nur 0,07 fehlen.
                    txt = (t('s_lg_teil') % (round(_da, 3), round(noetig, 3),
                                             round(_fehlt, 3))
                           if _da > 0 else t('s_lg_fehlt') % round(_fehlt, 3))
                    lage_lbl.configure(text=txt, fg=GOLD)
                    lage_lbl.pack(side='right', padx=(0, 8))
                elif _da > 0:
                    lage_lbl.configure(text=t('s_lg_da') % round(_da, 3),
                                       fg=ACCENT)
                    lage_lbl.pack(side='right', padx=(0, 8))
                else:
                    lage_lbl.pack_forget()
                # ⚠ Eigener Hinweis, wenn Material zwar daliegt, aber die
                # geforderte Qualität nicht erreicht. Ohne ihn stünde „dir
                # fehlt 0,3" da, obwohl 12 SCU im Lager liegen — und niemand
                # käme auf den Grund.
                if _zu_gering > 0:
                    guete_lbl.configure(text=t('s_lg_zu_schlecht')
                                        % (round(_zu_gering, 3), _mindestq))
                    guete_lbl.pack(side='right', padx=(0, 8))
                else:
                    guete_lbl.pack_forget()

                # Was das Schliessen der Lücke kostet — oder dass es gar nicht
                # geht. ⚠ Nur zeigen, wenn wirklich etwas fehlt: Bei vollem
                # Lager ist die Frage „kaufen?" gegenstandslos.
                #
                # ⚠ Ohne Preisdaten (kein Netz, erster Start) bleibt die Zeile
                # leer. Kein Hinweis, keine Entschuldigung — die Seite sah
                # vorher genauso aus.
                _p = None
                if _fehlt > 0:
                    try:
                        _p = preis_modul.preis(rohstoff)
                    except Exception as ausnahme:
                        fehler.merken('seiten.preis', ausnahme)
                if not _p:
                    preis_lbl.pack_forget()
                else:
                    _kauf, _verk, _form = _p
                    if _kauf > 0:
                        # ⚠ Die Qualitaet MUSS dabeistehen. Ohne sie liest sich
                        # „kaufen: 22.730 aUEC" wie ein gleichwertiger Weg, der
                        # nur Geld statt Zeit kostet — und das stimmt nicht.
                        preis_lbl.configure(
                            text=t('s_he_kaufen') % (_geld(_kauf * _fehlt),
                                                     preis_modul.KAUF_QUALITAET),
                            fg=SUB)
                    else:
                        # ⚠ NICHT „0 aUEC" — das liest sich wie geschenkt.
                        preis_lbl.configure(text=t('s_he_nur_abbau'), fg=GOLD)
                    preis_lbl.pack(side='right', padx=(0, 8))

        anzahl_var.trace_add('write', mengen_setzen)
        mengen_setzen()
        if stufe['zeit']:
            z = tk.Frame(block, bg='#0c1017')
            z.pack(fill='x', padx=12, pady=(4, 8))
            tk.Label(z, text=t('s_he_zeit'), bg='#0c1017', fg=SUB,
                     font=fenster.f_klein, width=18, anchor='w').pack(side='left')
            tk.Label(z, text=_dauer(stufe['zeit']), bg='#0c1017',
                     fg=FG, font=fenster.f_klein).pack(side='left')

        # ⭐ Was käme mit DEINEM Material heraus? (Idee von Xharig, 29.08.2026)
        #
        # Die Rezepte tragen die Qualitätswirkung mit: mieses Erz macht ein
        # schlechteres Stück, gutes ein besseres. Das steht in keiner Webseite,
        # weil dort niemand weiß, was im eigenen Frachtraum liegt.
        #
        # ⚠ Nur zeigen, wenn das Lager etwas dazu hergibt — geraten wird nicht.
        qualitaeten = {}
        for _slot, _roh, _mg, _gt in stufe['zutaten']:
            beste = lager.beste_qualitaet(_roh, _gt)
            if beste is not None:
                qualitaeten[_roh] = beste
        # ⚠ **Auch ohne Lager anzeigen.** Die Frage „was bringt mir Erz mit
        # Qualität X?" stellt man, BEVOR man es hat — genau dafür ist der
        # Regler unten da. Ohne Lagerstand wird mit Q 500 (Mitte) begonnen.
        alle_materialien = [r for _s, r, _m, _g in stufe['zutaten'] if r]
        if alle_materialien:
            # ⚠ Die Ueberschrift ist NICHT fest. Liegt nichts von den Zutaten
            # im Lager, waere „Mit deinem Material" eine Behauptung ueber
            # Material, das es nicht gibt — gerechnet wird dann mit dem
            # Reglerwert. `werte_zeichnen()` setzt sie passend.
            werte_kopf = tk.Label(block, text=t('s_he_werte'), bg='#0c1017',
                                  fg=FG, font=fenster.f_grund, anchor='w')
            werte_kopf.pack(fill='x', padx=12, pady=(10, 2))
            werte_rahmen = tk.Frame(block, bg='#0c1017')
            werte_rahmen.pack(fill='x')
            regler_lbl = tk.Label(block, text='', bg='#0c1017', fg=SUB,
                                  font=fenster.f_klein, anchor='w')

            # ⚠⚠ **Die Zeilen werden EINMAL gebaut, danach nur beschriftet.**
            #
            # Vorher wurde bei jeder Reglerbewegung alles zerstört und neu
            # aufgebaut — bei einem Regler heißt das: bei jedem Pixel. Das
            # ruckelte und flackerte so stark, dass er nicht bedienbar war
            # (gemeldet 29.08.2026). Tk-Widgets zu erzeugen ist teuer,
            # `configure(text=…)` ist billig.
            #
            # Damit die Zeilenzahl feststeht, wird die Liste **immer** mit
            # einer vollständigen Qualitätsvorgabe gebaut; welche Werte darin
            # stehen, entscheidet erst `werte_setzen()`.
            grundliste = herst_modul.werte_mit_lager(
                eintrag['basis'], {m: 500.0 for m in alle_materialien})
            zeilen_widgets = []
            for w in grundliste:
                wz = tk.Frame(werte_rahmen, bg='#0c1017')
                wz.pack(fill='x', padx=12, pady=1)
                # ⚠ Uebersetzt ueber den sprachneutralen Schluessel, nicht
                # ueber den englischen Namen — siehe `herstellung.eigenschaft`.
                tk.Label(wz, text=herst_modul.eigenschaft(w['eigenschaft'],
                                                          w.get('key')),
                         bg='#0c1017', fg=SUB,
                         font=fenster.f_klein, width=22,
                         anchor='w').pack(side='left')
                # ⚠⚠ **Die feste Breite gilt nur für den Faktor.** Als die
                # Prozentzahl in v3.3.0-rc37 dazukam, wurde sie in dasselbe
                # Etikett geschrieben — und `width=9` schnitt sie ab: Auf dem
                # Bildschirm stand „× 1.047  +4.(" statt „+4,70 %". Eine feste
                # Breite ist eine Zusage über den Inhalt; wer Inhalt dazutut,
                # muss sie anfassen.
                faktor_lbl = tk.Label(wz, text='', bg='#0c1017', fg=ACCENT,
                                      font=fenster.f_grund, width=9,
                                      anchor='w')
                faktor_lbl.pack(side='left')
                # Eigene Spalte fürs Prozent — so bleiben beide untereinander
                # bündig, statt sich gegenseitig zu verschieben.
                prozent_lbl = tk.Label(wz, text='', bg='#0c1017', fg=ACCENT,
                                       font=fenster.f_grund, width=10,
                                       anchor='w')
                prozent_lbl.pack(side='left', padx=(6, 0))
                herkunft_lbl = tk.Label(wz, text='', bg='#0c1017', fg=SUB,
                                        font=fenster.f_klein, anchor='e')
                herkunft_lbl.pack(side='right', padx=12)
                # ⚠ Zweite Zeile darunter: die Spanne. Ein Faktor allein ist
                # nicht einzuordnen — „× 0.867" sagt nicht, ob noch viel geht.
                # Erst „×1.2–0.8" daneben macht klar, dass es schon zwei
                # Drittel des Wegs sind. scmdb zeigt es aus demselben Grund.
                #
                # ⚠⚠ **In `werte_rahmen`, direkt hinter die eigene Zeile.**
                # Bis rc42 stand hier `block` — der Behälter eine Ebene höher.
                # Dadurch rutschten *alle* Spannen ans Ende des Blocks und
                # standen dort als gleich aussehende Zeilen untereinander,
                # während die Werte, zu denen sie gehören, weiter oben blieben.
                # Auf dem Bildschirm war nicht mehr zu erkennen, welche Spanne
                # zu welchem Wert gehört. Der Elternteil bestimmt hier die
                # Zuordnung — nicht nur den Ort.
                spanne_lbl = tk.Label(werte_rahmen, text='', bg='#0c1017',
                                      fg=SUB, font=fenster.f_klein, anchor='w')
                spanne_lbl.pack(fill='x', padx=(46, 12))
                zeilen_widgets.append((w, faktor_lbl, prozent_lbl,
                                       herkunft_lbl, spanne_lbl))

            leer_lbl = tk.Label(werte_rahmen, text='', bg='#0c1017', fg=SUB,
                                font=fenster.f_klein, anchor='w')

            # ⚠⚠ **Je Material ein eigener Wert.** Bis v3.3.0-rc35 gab es
            # EINEN Regler, der allen Zutaten dieselbe Qualität gab. Das ist
            # praktisch nie die Wirklichkeit: „jedes Material hat man so gut
            # wie nie in der gleichen Qualität da" (30.08.2026). Und weil jede
            # Zutat eine ANDERE Eigenschaft anhebt, ist die eigentliche Frage
            # ohnehin eine andere: „ich habe 500er Iron — was kommt raus, wenn
            # ich 900er nähme, und was ändert sich dadurch am Riccite-Wert?"
            # Mit einem gemeinsamen Regler liess sie sich gar nicht stellen.
            #
            # `stand` hält die aktuelle Qualität je Material. Startwert ist
            # der eigene Lagerstand, sonst die Mitte.
            stand = {m: float(qualitaeten.get(m, 500.0))
                     for m in alle_materialien}
            aus_lager = {m: (m in qualitaeten) for m in alle_materialien}

            def werte_zeichnen():
                """Nur die Zahlen austauschen — keine Widgets neu bauen."""
                aktuell = {(w['eigenschaft'], w['material'], w['slot']): w
                           for w in herst_modul.werte_mit_lager(
                               eintrag['basis'], stand)}
                gezeigt = 0
                for (w0, faktor_lbl, prozent_lbl, herkunft_lbl,
                     spanne_lbl) in zeilen_widgets:
                    w = aktuell.get((w0['eigenschaft'], w0['material'],
                                     w0['slot']))
                    if not w:
                        faktor_lbl.configure(text='')
                        prozent_lbl.configure(text='')
                        herkunft_lbl.configure(text='')
                        spanne_lbl.configure(text='')
                        continue
                    gezeigt += 1
                    # ⚠⚠ **Die Farbe darf nicht an der Zahl haengen.** Bis
                    # v3.3.0-rc35 galt „>= 1 ist gut". Bei Rueckstoss und
                    # Quantum-Treibstoff ist WENIGER besser — dort stand der
                    # bestmoegliche Wert (× 0.800) in der Warnfarbe und der
                    # schlechteste (× 1.200) in Gruen. 852 von 6524
                    # Modifikatoren im Spielstand 4.10.0 laufen so.
                    if w.get('absolut'):
                        # ⚠ Power Pips: eine Stueckzahl, kein Faktor. Bis
                        # v3.3.0-rc35 stand hier „× -1.000" — ein
                        # Multiplikator, den es nicht geben kann. 598 der 6524
                        # Modifikatoren sind so gebaut (alle Kraftwerke).
                        gut = w['faktor'] > 0
                        text_wert = (t('s_he_absolut_null') if not w['faktor']
                                     else t('s_he_absolut') % w['faktor'])
                        farbe = (ACCENT if w['faktor'] > 0
                                 else GOLD if w['faktor'] < 0 else SUB)
                    else:
                        gut = (w['faktor'] >= 1 if w.get('besser_hoch', True)
                               else w['faktor'] <= 1)
                        text_wert = t('s_he_faktor') % w['faktor']
                        farbe = ACCENT if gut else GOLD
                    faktor_lbl.configure(text=text_wert, fg=farbe)
                    # ⚠ Prozent in die eigene Spalte. „× 0.867" muss man im
                    # Kopf umrechnen, „−13,28 %" nicht — und genau das ist die
                    # Zahl, die man mit anderem Material vergleicht.
                    prozent_lbl.configure(
                        text=('' if w.get('absolut')
                              else t('s_he_prozent') % ((w['faktor'] - 1) * 100)),
                        fg=farbe)
                    herkunft = t('s_he_woher') % (w['material'], w['qualitaet'])
                    if not w.get('besser_hoch', True):
                        herkunft = '%s · %s' % (t('s_he_weniger_gut'), herkunft)
                    herkunft_lbl.configure(text=herkunft)
                    sp = w.get('spanne')
                    if sp:
                        q_von, q_bis, f_von, f_bis, basis = sp
                        spanne_lbl.configure(
                            text=(t('s_he_spanne')
                                  % (q_von, q_bis, f_von, f_bis, round(basis))
                                  if basis is not None else
                                  t('s_he_spanne_ohne') % (q_von, q_bis, f_von, f_bis)))
                    else:
                        spanne_lbl.configure(text='')
                if not gezeigt:
                    leer_lbl.configure(text=t('s_he_kein_lager'))
                    leer_lbl.pack(fill='x', padx=12)
                else:
                    leer_lbl.pack_forget()

                # Überschrift: „mit deinem Material" nur, solange nichts
                # verstellt wurde. Sobald ein Regler von seinem Lagerwert
                # abweicht, ist es ein Durchspielen und keine Aussage mehr.
                verstellt = any(
                    abs(stand[m] - float(qualitaeten.get(m, 500.0))) > 0.5
                    or not aus_lager[m] for m in alle_materialien)
                if not verstellt and qualitaeten:
                    werte_kopf.configure(text=t('s_he_werte'))
                else:
                    werte_kopf.configure(text=t('s_he_werte_probe_je'))

            # --- Ein Regler je Material ---
            # Dieselbe Frage, die man sonst auf scmdb.net von Hand stellt:
            # „Und mit besserem Erz?" Nur dass hier der eigene Lagerstand der
            # Ausgangspunkt ist — je Material einzeln.
            from .hauptfenster import regler as schieberegler
            tk.Label(block, text=t('s_he_regler_kopf'), bg='#0c1017', fg=FG,
                     font=fenster.f_grund, anchor='w').pack(
                         fill='x', padx=12, pady=(10, 2))
            # ⭐ Der Satz, der die Regler erst einordnet: Wer kauft, landet
            # immer bei 500 — dem Nullpunkt. Alles darüber muss man selbst
            # abbauen. Ohne diesen Hinweis sieht der Regler nach einer freien
            # Wahl aus, die man am Terminal treffen könnte.
            _fliesstext(block, t('s_he_kauf_q') % preis_modul.KAUF_QUALITAET,
                        fenster.f_klein, fill='x')

            # ⚠⚠ **589 Rezept-Slots haben ein Material ohne jede
            # Qualitaetswirkung** — Titanium in der BUL-H4 Armor etwa. Man
            # zieht dort am Regler, und es passiert nichts, weil es keine Zeile
            # dazu gibt. Am 30.08.2026 beim Testen aufgefallen.
            #
            # ⚠ Der Regler bleibt trotzdem, und zwar bedienbar — scmdb.net
            # haelt es genauso: „so sieht der User, egal was er nimmt, es hat
            # keine Auswirkung." Selbst ausprobieren ueberzeugt mehr als ein
            # fehlendes Bedienelement, das wie ein Versehen aussieht. Dazu
            # kommt nur der Hinweis, damit niemand den Fehler bei sich sucht.
            _wirksam = set()
            try:
                for _s in (herst_modul.slots(eintrag['basis']) or []):
                    if _s.get('material') and _s.get('wirkungen'):
                        _wirksam.add(_s['material'])
            except Exception as ausnahme:
                fehler.merken('seiten.wirksam', ausnahme)
                _wirksam = set(alle_materialien)

            regler_zeilen = {}
            for _mat in alle_materialien:
                reihe_r = tk.Frame(block, bg='#0c1017')
                reihe_r.pack(fill='x', padx=12, pady=2)
                tk.Label(reihe_r, text=_mat, bg='#0c1017', fg=ACCENT,
                         font=fenster.f_klein, width=16, anchor='w').pack(
                             side='left')

                # ⚠ Der Wert MUSS neben dem Regler stehen. Ohne ihn zieht man
                # blind und weiß nicht, welche Qualität man gerade
                # durchspielt — genau der Wert, um den es geht.
                _wert_lbl = tk.Label(reihe_r, text=t('s_lg_q_wert')
                                     % int(stand[_mat]),
                                     bg='#0c1017', fg=ACCENT,
                                     font=fenster.f_grund, width=7, anchor='w')

                def gezogen(wert, mat=_mat):
                    stand[mat] = float(wert)
                    regler_zeilen[mat][0].configure(
                        text=t('s_lg_q_wert') % int(wert))
                    regler_zeilen[mat][1].configure(
                        text=(t('s_he_regler_lager')
                              if (aus_lager[mat]
                                  and abs(float(wert)
                                          - float(qualitaeten.get(mat, -1))) < 0.5)
                              else ''))
                    werte_zeichnen()

                _schieber = schieberegler(reihe_r, 0, 1000, int(stand[_mat]),
                                          gezogen, breite=200, grund='#0c1017')
                _schieber.pack(side='left')
                _wert_lbl.pack(side='left', padx=(10, 0))
                # Woher der Startwert kommt: eigener Lagerstand oder Mitte.
                # ⚠ Bei einem Material ohne Wirkung ist die Herkunft der
                # Qualitaet gleichgueltig — dort steht der Grund, warum sich
                # beim Ziehen nichts tut.
                _quelle_lbl = tk.Label(
                    reihe_r,
                    text=(t('s_he_ohne_wirkung') if _mat not in _wirksam
                          else t('s_he_regler_lager') if aus_lager[_mat]
                          else t('s_he_regler_ohne')),
                    bg='#0c1017', fg=SUB, font=fenster.f_klein, anchor='w')
                _quelle_lbl.pack(side='left', padx=(10, 0))
                regler_zeilen[_mat] = (_wert_lbl, _quelle_lbl, _schieber)

            # Alles wieder auf den eigenen Lagerstand zurückstellen.
            zurueck = tk.Label(block, text=t('s_he_zurueck_lager'),
                               bg='#0c1017', fg=ACCENT, font=fenster.f_klein,
                               cursor='hand2')

            def zurueck_zum_lager(_e=None):
                for _m in alle_materialien:
                    stand[_m] = float(qualitaeten.get(_m, 500.0))
                    _w, _q, _s = regler_zeilen[_m]
                    _w.configure(text=t('s_lg_q_wert') % int(stand[_m]))
                    _q.configure(text=(t('s_he_regler_lager') if aus_lager[_m]
                                       else t('s_he_regler_ohne')))
                    # `regler()` gibt seine Zeichenfunktion mit heraus —
                    # damit steht der Knopf wieder an der richtigen Stelle.
                    try:
                        _s.zeichnen(stand[_m])
                    except Exception:
                        pass
                werte_zeichnen()

            if qualitaeten:
                zurueck.pack(anchor='w', padx=12, pady=(2, 0))
                zurueck.bind('<Button-1>', zurueck_zum_lager)
            werte_zeichnen()
            _fliesstext(block, t('s_he_werte_hinweis'), fenster.f_klein,
                        fill='x')



# ------------------------------------------------------------------- Bergbau

BERG_MAX = 60


def _art_text(arten):
    """Die Abbauarten lesbar: „FPS · Schiff"."""
    reihenfolge = ('fps', 'fahrzeug', 'schiff', 'schiff_selten')
    return ' · '.join(t('s_bg_art_' + a) for a in reihenfolge if a in arten)


def _bergbau(fenster, rahmen):
    """Wo welches Erz abzubauen ist — **beide** Richtungen in einer Suche.

    Ohne Eingabe stehen die Orte da (man ist meistens irgendwo). Tippt man
    einen Rohstoff, kommen dessen Fundorte; tippt man einen Ort, kommt, was es
    dort gibt. Das sind nicht zwei Ansichten, sondern eine Tabelle mit zwei
    Eingängen — beides sind echte Fragen, je nachdem ob man gerade fliegen mag
    oder nicht.
    """
    from . import bergbau as berg_modul
    _ueberschrift(fenster, rahmen, t('hf_bergbau'), t('s_bg_lead'))
    innen = _rollflaeche(rahmen)

    try:
        orte = berg_modul.orte()
        erze = berg_modul.erze()
    except Exception as ausnahme:
        fehler.merken('seiten.bergbau', ausnahme)
        orte, erze = [], []

    # ⚠⚠ **Vor der Abbruchbedingung.** Die Methodenempfehlung braucht KEINE
    # Bergbaudaten — sie steht fest im Programm. Stünde sie weiter unten, wäre
    # sie ausgerechnet für den weg, der ohne Netz unterwegs ist: Der Abbruch
    # darunter beendet die Seite, sobald die Daten fehlen.
    _methodenblock(fenster, innen)

    if not orte:
        _fliesstext(innen, t('s_bg_keine_daten'), fenster.f_klein, fill='x')
        return

    kopf = tk.Frame(innen, bg=BG)
    kopf.pack(fill='x', pady=(0, 10))
    tk.Label(kopf, text=t('s_bg_orte') % (len(orte), len(erze)), bg=BG, fg=SUB,
             font=fenster.f_klein, anchor='w').pack(fill='x')

    from .hauptfenster import rundes_feld
    # Der Sprung aus einem Rezept setzt hier den Rohstoff hinein.
    suche_var = tk.StringVar(value=getattr(fenster, 'bergbau_suche', '') or '')
    fenster.bergbau_suche = ''
    ziel_suche = _feld(fenster, innen, t('s_bg_suche'), '')
    feld = rundes_feld(ziel_suche, suche_var, fenster.f_klein, '#0c1017',
                       LINIE, ACCENT, FG)
    feld.halter.pack(fill='x', pady=(4, 12))
    _suche_leeren_kreuz(fenster, ziel_suche, suche_var)

    # ⚠ Dieselben Auswahlfelder wie auf den anderen Seiten. Tippen bleibt
    # möglich — aber wer die 38 Rohstoffe oder 48 Orte nicht auswendig kann,
    # soll sie aufklappen können, statt zu raten. „egal wo, sollte das
    # Bedienkonzept nicht jedes Mal ändern." (29.08.2026)
    berg_wahl = {'erz': '', 'ort': ''}

    def berg_gewechselt():
        # Die Auswahl schreibt in dasselbe Suchfeld — es gibt nur **einen**
        # Filter, nicht zwei, die sich gegenseitig widersprechen könnten.
        suche_var.set(berg_wahl['erz'] or berg_wahl['ort'] or '')

    # ⚠ `erze()` und `orte()` liefern **Objekte**, keine Namen — mit ihnen
    # direkt bestückt bliebe das Feld leer.
    _erznamen = sorted({(e_.get('name') or '') for e_ in erze} - {''},
                       key=str.lower)
    _ortnamen = sorted({(o_.get('name') or '') for o_ in orte} - {''},
                       key=str.lower)
    _filterleiste(fenster, innen,
                  [('erz', t('s_bg_alle_erze'), [(x, x) for x in _erznamen]),
                   ('ort', t('s_bg_alle_orte'), [(x, x) for x in _ortnamen])],
                  berg_gewechselt, berg_wahl)
    # Beim erneuten Aufrufen des Reiters wieder leer — die Seite wird nur
    # ein- und ausgeblendet, nicht neu gebaut.
    fenster.beim_zeigen['bergbau'] = lambda: suche_var.set('')

    # ⭐⭐ **Scan-Signatur — das Werkzeug, das im Spiel wirklich fehlt.**
    # Der Bergbau-Scanner zeigt eine Zahl und verrät nicht, was dahintersteckt.
    # Die Zahl ist die Signatur des Rohstoffs mal der Zahl der Brocken im
    # Vorkommen; wie viele es höchstens sein können, sagt die Seltenheit
    # (legendär 2, verbreitet 6). Beides steht in den Bergbaudaten, die der
    # Watcher ohnehin lädt.
    #
    # ⚠ Das Feld wird **hier** gebaut, nicht in `zeichnen()`. Läge es darin,
    # verlöre es bei jedem Tastendruck den Cursor — derselbe Fehler wie beim
    # Suchfeld im Lager (v3.3.0-rc21).
    sig_var = tk.StringVar(value='')
    ziel_sig = _feld(fenster, innen, t('s_bg_sig_feld'), '')
    sig_feld = rundes_feld(ziel_sig, sig_var, fenster.f_klein, '#0c1017',
                           LINIE, ACCENT, FG)
    sig_feld.halter.pack(fill='x', pady=(4, 2))
    _fliesstext(innen, t('s_bg_sig_hilfe'), fenster.f_klein, fill='x')
    sig_rahmen = tk.Frame(innen, bg=BG)
    sig_rahmen.pack(fill='x', pady=(2, 10))

    def sig_zeichnen(*_):
        for w in sig_rahmen.winfo_children():
            w.destroy()
        eingabe = sig_var.get().strip()
        if not eingabe:
            return
        try:
            treffer = berg_modul.signatur_suchen(eingabe)
        except Exception as ausnahme:
            fehler.merken('seiten.signatur', ausnahme)
            return
        if not treffer:
            _fliesstext(sig_rahmen, t('s_bg_sig_nichts'), fenster.f_klein,
                        fill='x')
            return
        tk.Label(sig_rahmen, text=t('s_bg_sig_anzahl') % len(treffer), bg=BG,
                 fg=SUB, font=fenster.f_klein, anchor='w').pack(fill='x')
        # ⚠ Höchstens zehn. Eine Bereichssuche kann dutzende Treffer haben,
        # und die Liste darunter soll nicht aus dem Bild geschoben werden.
        for name, anzahl, gesamt, ab in treffer[:10]:
            z = tk.Frame(sig_rahmen, bg=BG)
            z.pack(fill='x', pady=1)
            tk.Label(z, text=t('s_bg_sig_treffer') % (anzahl, name), bg=BG,
                     fg=ACCENT, font=fenster.f_grund, anchor='w').pack(
                         side='left', padx=(4, 0))
            tk.Label(z, text='%d' % gesamt, bg=BG, fg=FG,
                     font=fenster.f_klein, anchor='e').pack(
                         side='right', padx=(8, 4))
            # Die Abweichung nur, wenn es eine gibt — „+0,0 %" ist Rauschen.
            if abs(ab) >= 0.05:
                tk.Label(z, text='%+.1f %%' % ab, bg=BG, fg=SUB,
                         font=fenster.f_klein, anchor='e').pack(
                             side='right', padx=(8, 0))
            else:
                tk.Label(z, text=t('s_bg_sig_genau'), bg=BG, fg=SUB,
                         font=fenster.f_klein, anchor='e').pack(
                             side='right', padx=(8, 0))

    sig_var.trace_add('write', sig_zeichnen)
    _suche_leeren_kreuz(fenster, ziel_sig, sig_var)

    liste_rahmen = tk.Frame(innen, bg=BG)
    liste_rahmen.pack(fill='both', expand=True)
    offen = {'name': None}

    def zeichnen(*_):
        for w in liste_rahmen.winfo_children():
            w.destroy()
        text = suche_var.get().strip().lower()

        # ⚠ **Rohstoffe zuerst, auch ohne Suche.** Die Seite zeigte im
        # Grundzustand die 48 Orte — man kam also mit „wo bin ich?" herein,
        # gesucht wird aber mit „wo finde ich Titanium?". Am 29.08.2026:
        # „in der Liste sollten auch nicht die Orte, sondern erst das Mineral
        # stehen, da sucht man als Erstes nach."
        for e in erze:
            if not text or text in e['name'].lower():
                _berg_erz(fenster, liste_rahmen, e, offen, zeichnen)
        # Orte danach — sie beantworten die zweite Frage („was gibt es hier?").
        # Ohne Suche stehen sie unter den Rohstoffen, nicht davor.
        for o in orte:
            passt = (not text
                     or text in o['name'].lower()
                     or text in (o['system'] or '').lower())
            if passt:
                _berg_ort(fenster, liste_rahmen, o, offen, zeichnen)

        if not liste_rahmen.winfo_children():
            _fliesstext(liste_rahmen, t('s_he_nichts'), fenster.f_klein,
                        fill='x')

    suche_var.trace_add('write', zeichnen)
    zeichnen()
    _fliesstext(innen, t('s_bg_mehr_info'), fenster.f_klein, fill='x')


def _berg_kopfzeile(fenster, eltern, links, rechts, farbe, aufklappen):
    zeile = tk.Frame(eltern, bg=BG, cursor='hand2')
    zeile.pack(fill='x', pady=1)
    tk.Label(zeile, text=links, bg=BG, fg=farbe, font=fenster.f_grund,
             anchor='w').pack(side='left', padx=(4, 0))
    if rechts:
        tk.Label(zeile, text=rechts, bg=BG, fg=SUB, font=fenster.f_klein,
                 anchor='e').pack(side='right', padx=(8, 4))
    for w in (zeile,) + tuple(zeile.winfo_children()):
        w.bind('<Button-1>', aufklappen)
    return zeile


def _berg_erz(fenster, eltern, erz, offen, neu_zeichnen):
    """Ein Rohstoff — aufgeklappt stehen seine Fundorte darunter."""
    schluessel = 'erz:' + erz['name']

    def umschalten(*_):
        offen['name'] = None if offen['name'] == schluessel else schluessel
        neu_zeichnen()

    # ⚠ Hier stand `t('s_bg_orte') % (a, b).split('·')[0]` — das `.split()` lief
    # auf dem **Tupel**, nicht auf dem Text. Ergebnis: Ausnahme in `zeichnen()`,
    # und die ganze Liste blieb leer. Der Selbsttest sah es nicht, weil er die
    # Seite ohne Suchbegriff baut und dieser Zweig nie lief. Gefunden auf einem
    # Bildschirmfoto (29.08.2026). Jetzt ein eigener Textschlüssel.
    _berg_kopfzeile(fenster, eltern, erz['name'],
                    t('s_bg_nur_orte') % len(erz['orte']),
                    ACCENT, umschalten)
    if offen['name'] != schluessel:
        return
    block = tk.Frame(eltern, bg='#0c1017')
    block.pack(fill='x', padx=(24, 0), pady=(2, 8))
    for ort, system, arten in erz['orte']:
        z = tk.Frame(block, bg='#0c1017')
        z.pack(fill='x', padx=12, pady=1)
        tk.Label(z, text=ort, bg='#0c1017', fg=FG, font=fenster.f_grund,
                 anchor='w').pack(side='left')
        tk.Label(z, text=system, bg='#0c1017', fg=SUB, font=fenster.f_klein,
                 anchor='w').pack(side='left', padx=(10, 0))
        tk.Label(z, text=_art_text(arten), bg='#0c1017', fg=SUB,
                 font=fenster.f_klein, anchor='e').pack(side='right', padx=12)

    # ⭐ **Wohin damit?** Die Frage nach dem Fundort ist nur die halbe. Zwanzig
    # Raffinerien teilen sich zehn Profile, und der Unterschied ist kein
    # Rundungsfehler: Bei Bexalite liegen 18 Prozentpunkte zwischen der besten
    # und der schlechtesten Wahl, bei Quartz 16. Wer das nicht weiß, verschenkt
    # jeden Flug ein Stück Ausbeute.
    #
    # ⚠ Die Daten stehen in denselben Bergbaudaten (`refineries` +
    # `refineryProfiles`) und kosten keinen zusätzlichen Abruf. Gegengerechnet
    # gegen die Tabelle auf scmdb.net: alle zehn ARC-L1-Werte identisch.
    from . import bergbau as berg_modul
    try:
        raff = berg_modul.raffinerien_fuer(erz['name'])
    except Exception as ausnahme:
        fehler.merken('seiten.raffinerie', ausnahme)
        raff = []
    if raff:
        tk.Label(block, text=t('s_bg_raff_kopf'), bg='#0c1017', fg=FG,
                 font=fenster.f_grund, anchor='w').pack(
                     fill='x', padx=12, pady=(10, 2))
        spanne = raff[0][2] - raff[-1][2]
        if not spanne:
            _fliesstext(block, t('s_bg_raff_egal'), fenster.f_klein, fill='x')
        else:
            for namen, system, bonus in raff:
                z = tk.Frame(block, bg='#0c1017')
                z.pack(fill='x', padx=12, pady=1)
                # Nur das Kürzel — „ARC-L1 Wide Forest Station" dreimal
                # untereinander ist eine Wand aus Text. Und bei mehreren
                # Stationen mit demselben Profil nur die erste plus Zähler:
                # Ein Profil deckt acht Stationen ab, ausgeschrieben sprengt
                # das jede Zeile.
                _kuerzel = list(dict.fromkeys(n.split(' ')[0] for n in namen))
                kurz = (_kuerzel[0] if len(_kuerzel) == 1
                        else t('s_bg_raff_weitere') % (_kuerzel[0],
                                                       len(_kuerzel) - 1))
                tk.Label(z, text=kurz, bg='#0c1017', fg=FG,
                         font=fenster.f_grund, anchor='w').pack(side='left')
                tk.Label(z, text=system or '', bg='#0c1017', fg=SUB,
                         font=fenster.f_klein, anchor='w').pack(
                             side='left', padx=(10, 0))
                tk.Label(z, text=t('s_bg_raff_zeile') % bonus, bg='#0c1017',
                         fg=(ACCENT if bonus > 0 else GOLD if bonus < 0 else SUB),
                         font=fenster.f_grund, anchor='e').pack(
                             side='right', padx=12)
            _fliesstext(block, t('s_bg_raff_spanne') % spanne,
                        fenster.f_klein, fill='x')


def _methodenblock(fenster, eltern):
    """„Welche Verarbeitungsmethode?" — die dritte Frage der Kette.

    Wo baue ich ab → wohin bringe ich es → **wie lasse ich es verarbeiten.**
    Die ersten beiden beantwortet die Liste darunter je Erz; diese hier ist
    erz-unabhängig und steht deshalb darüber, nicht neunmal in jeder
    Aufklappung.

    ⚠ Die Auswahl steht in derselben `_filterleiste` wie überall sonst — ein
    Bedienkonzept fürs ganze Programm, kein Sonderweg für eine Seite.
    """
    from . import raffinerie as raff
    block = tk.Frame(eltern, bg=BG)
    block.pack(fill='x', pady=(0, 12))
    tk.Label(block, text=t('s_rm_kopf'), bg=BG, fg=FG, font=fenster.f_grund,
             anchor='w').pack(fill='x')
    _fliesstext(block, t('s_rm_lead'), fenster.f_klein, fill='x')

    achsentext = {'ertrag': t('s_rm_ertrag'), 'kosten': t('s_rm_kosten'),
                  'tempo': t('s_rm_tempo')}
    auswahl = [(a, achsentext[a]) for a in raff.ACHSEN]
    wahl = {'erste': '', 'zweite': ''}
    ergebnis = tk.Frame(block, bg=BG)

    def stufentext(kennung):
        ertrag = {1: 's_rm_s_gering', 2: 's_rm_s_moderat',
                  3: 's_rm_s_hoch'}[raff.stufe(kennung, 'ertrag')]
        tempo = {0: 's_rm_t_sehr', 1: 's_rm_t_langsam', 2: 's_rm_t_mittel',
                 3: 's_rm_t_schnell'}[raff.stufe(kennung, 'tempo')]
        # ⚠ Umdrehen: Im Modul ist 3 der **Kostenvorteil**, auf dem Bildschirm
        # steht „geringe Kosten". Ohne diese Zeile stünde dort das Gegenteil.
        kosten = {3: 's_rm_s_gering', 2: 's_rm_s_moderat',
                  1: 's_rm_s_hoch'}[raff.stufe(kennung, 'kosten')]
        return t('s_rm_zeile') % (t(ertrag), t(tempo), t(kosten))

    def zeichnen():
        for w in ergebnis.winfo_children():
            w.destroy()
        beste, alle = raff.empfehlung(wahl['erste'] or None,
                                      wahl['zweite'] or None)
        tk.Label(ergebnis, text=t('s_rm_nimm') % raff.NAMEN[beste], bg=BG,
                 fg=ACCENT, font=fenster.f_grund, anchor='w').pack(
                     fill='x', pady=(6, 0))
        _fliesstext(ergebnis, stufentext(beste), fenster.f_klein, fill='x')
        # Der Satz gehört genau dann dazu, wenn die Empfehlung mit Zeit
        # bezahlt wird — sonst wäre er ein Allgemeinplatz.
        if raff.stufe(beste, 'tempo') <= 1:
            _fliesstext(ergebnis, t('s_rm_zeit_laeuft'), fenster.f_klein,
                        fill='x')

        tk.Label(ergebnis, text=t('s_rm_alle'), bg=BG, fg=SUB,
                 font=fenster.f_klein, anchor='w').pack(fill='x', pady=(10, 2))
        for kennung in alle:
            z = tk.Frame(ergebnis, bg=BG)
            z.pack(fill='x', pady=1)
            tk.Label(z, text=raff.NAMEN[kennung], bg=BG,
                     fg=(ACCENT if kennung == beste else FG),
                     font=fenster.f_grund, anchor='w').pack(side='left',
                                                            padx=(4, 0))
            tk.Label(z, text=stufentext(kennung), bg=BG, fg=SUB,
                     font=fenster.f_klein, anchor='e').pack(side='right',
                                                            padx=(8, 4))

        # Methoden, die nichts können, was eine andere nicht besser kann.
        for schlecht, besser in sorted(raff.unterlegen().items()):
            _fliesstext(ergebnis,
                        t('s_rm_unterlegen') % (raff.NAMEN[schlecht],
                                                raff.NAMEN[besser]),
                        fenster.f_klein, fill='x')
        _fliesstext(ergebnis, t('s_rm_stand') % (raff.PATCH, raff.ABGELESEN),
                    fenster.f_klein, fill='x')

    _filterleiste(fenster, block,
                  [('erste', t('s_rm_erste'), auswahl),
                   ('zweite', t('s_rm_zweite'), auswahl)],
                  zeichnen, wahl)
    ergebnis.pack(fill='x')
    zeichnen()


def _berg_ort(fenster, eltern, ort, offen, neu_zeichnen):
    """Ein Ort — aufgeklappt steht darunter, was es dort gibt."""
    schluessel = 'ort:' + ort['name']

    def umschalten(*_):
        offen['name'] = None if offen['name'] == schluessel else schluessel
        neu_zeichnen()

    _berg_kopfzeile(fenster, eltern, ort['name'],
                    '%s · %s' % (ort['system'], ort['typ']), FG, umschalten)
    if offen['name'] != schluessel:
        return
    block = tk.Frame(eltern, bg='#0c1017')
    block.pack(fill='x', padx=(24, 0), pady=(2, 8))
    for name in sorted(ort['erze']):
        z = tk.Frame(block, bg='#0c1017')
        z.pack(fill='x', padx=12, pady=1)
        tk.Label(z, text=name, bg='#0c1017', fg=FG, font=fenster.f_grund,
                 anchor='w').pack(side='left')
        tk.Label(z, text=_art_text(ort['erze'][name]), bg='#0c1017', fg=SUB,
                 font=fenster.f_klein, anchor='e').pack(side='right', padx=12)


# ------------------------------------------------------------------- Lager
#
# Vorschlag von **Horthy (KRT)** (29.08.2026): Rohstoffe selbst
# eintragen, beim Herstellen abziehen lassen.
#
# ⚠ **Von Hand, weil es nicht anders geht.** Die `Game.log` sagt nichts über
# Rohstoffe — in 17 MB Protokollen kommt weder `resource` noch `cargo` vor.
# Deshalb steht der Hinweis oben auf der Seite: Diese Liste gehört dem Spieler,
# nicht dem Spiel.


def _kaestchen(eltern, text, an, umschalten, schrift_klein):
    """Ein anklickbares Kästchen mit Haken — für „ja/nein" neben einem Feld.

    ⚠ Warum kein `tk.Checkbutton`: Der ist ein Systemelement und sieht auf
    jedem Betriebssystem anders aus. Das Programm hat eine Formensprache; ein
    graues Aqua-Kästchen mitten in einer sonst dunklen Zeile fällt auf wie ein
    Fremdkörper. Gezeichnet wird nichts von Hand — der Haken ist das
    Symbol `abhaken` aus dem Satz.
    """
    rahmen = tk.Frame(eltern, bg=BG, cursor='hand2')
    # ⚠ Nur Symbole aus dem festgelegten Satz — `haken` steht in
    # `zeichen.ZEILEN_NAMEN`. Ein frei erfundener Name (`abhaken` gibt es nur
    # als Knopf-Symbol) faellt still auf den Ersatztext zurueck, und die Zeile
    # sieht dann anders aus als der Rest des Programms.
    # ⚠⚠ **Nur die festgelegten Farben.** Die Symbole liegen als fertige Bilder
    # je Farbe im Satz (`grau`, `gruen`, `hell`, `gelb`, `blau`, `rot`) — ein
    # eigener Farbwert findet kein Bild, und das Symbol fehlt dann **still**.
    # Genau so passiert: Mit `#2a3446` stand neben „cSCU" gar kein Haken.
    def _bauen(an_jetzt):
        for kind in rahmen.winfo_children():
            kind.destroy()
        symbol = zeichen.zeile(rahmen, 'haken', grund=BG,
                               farbe=zeichen.GRUEN if an_jetzt
                               else zeichen.GRAU)
        symbol.pack(side='left')
        lbl = tk.Label(rahmen, text=text, bg=BG,
                       fg=ACCENT if an_jetzt else SUB, font=schrift_klein)
        lbl.pack(side='left', padx=(4, 0))
        for teil in (symbol, lbl):
            teil.bind('<Button-1>', klick)

    def klick(_=None):
        an[0] = not an[0]
        _bauen(an[0])
        umschalten(an[0])

    _bauen(an[0])
    rahmen.bind('<Button-1>', klick)
    return rahmen


def _raffinerie_block(fenster, eltern, lager, ort_var, neu_zeichnen, meldung):
    """Eine ganze Raffinerie-Ausbeute auf einmal eintragen.

    ⚠⚠ **Warum das nicht automatisch geht.** Der Raffinerie-Auftrag steht
    **nicht** in der `Game.log` — am 30.08.2026 über 22 Protokolle nachgemessen:
    `Refinery` kommt dort 58-mal vor, ausschliesslich als Ladezeile für die
    3D-Modelle des Decks; `Aslarite`, `Agricium` und `cSCU` **kein einziges
    Mal**. Das Spiel hält diese Aufträge serverseitig.

    Bilderkennung wäre der andere Weg und ist bewusst keiner: Sie bräuchte
    Zusatzpakete, und dieses Werkzeug kommt mit der Standardbibliothek aus.

    Bleibt: das Abtippen erträglich machen. Sechs Posten sind über das Formular
    oben **24 Eingaben**; hier sind es sechs Zeilen, so wie sie im Terminal
    stehen.
    """
    from . import herstellung as herst_lager
    from . import orte as _orte_modul
    from .hauptfenster import rundrahmen

    # ⭐ **Zugeklappt, bis er gebraucht wird.** Der Block ist der laengste auf
    # der Seite — Einheitenwahl, Lagerort mit Auswahlliste, ein sieben Zeilen
    # hohes Tippfeld, Vorschau und Knopf. Wer nur schnell einen Posten von Hand
    # eintraegt (der haeufigere Fall), rollte an alldem vorbei, und die Liste
    # des eigenen Lagers lag darunter ausser Sicht.
    #
    # ⚠ Der Zustand wird **gemerkt** (`lager_raffinerie_offen`): Wer nach jedem
    # Raffinerie-Lauf abtippt, will den Block offen vorfinden, und wer ihn nie
    # benutzt, will ihn nicht bei jedem Start wieder zuklappen. Standard ist
    # zu — fuer den, der ihn noch nie gebraucht hat, ist das die richtige Lage.
    #
    # Gebaut wie die Klappbloecke auf der Danke-Seite: Kopfzeile mit dem
    # Klapp-Symbol links, Koerper darunter. Kein Textpfeil — das Symbol kommt
    # aus dem Satz, wie ueberall sonst.
    kasten = tk.Frame(eltern, bg=BG)
    kasten.pack(fill='x', pady=(12, 0))
    kopf = tk.Frame(kasten, bg=BG, cursor='hand2')
    kopf.pack(fill='x')
    pfeil = zeichen.zeile(kopf, 'aufklappen', grund=BG,
                          schrift=fenster.f_klein)
    pfeil.pack(side='left', padx=(0, 8))
    tk.Label(kopf, text=t('s_rf_titel'), bg=BG, fg=FG, font=fenster.f_fett,
             anchor='w', cursor='hand2').pack(side='left')

    ziel = tk.Frame(kasten, bg=BG)
    # Die Erklaerung gehoert in den Koerper, nicht in die Kopfzeile: Sonst
    # steht zugeklappt ein Absatz da, der etwas erklaert, das man nicht sieht.
    _rf_hilfe = tk.Label(ziel, text=_ohne_marken(t('s_rf_hilfe')), bg=BG,
                         fg=SUB, font=fenster.f_klein, anchor='w',
                         justify='left')
    _rf_hilfe.pack(fill='x', pady=(2, 0))
    _umbruch(_rf_hilfe, bezug=kasten, abzug=10)
    einheit = tk.StringVar(value='cscu')
    zeile = tk.Frame(ziel, bg=BG)
    zeile.pack(fill='x', pady=(6, 4))
    tk.Label(zeile, text=t('s_rf_einheit'), bg=BG, fg=SUB,
             font=fenster.f_klein).pack(side='left', padx=(0, 8))
    from .hauptfenster import rundwahl
    # ⚠ Reihenfolge: (eltern, eintraege, gewaehlt, beim_waehlen, schrift).
    rundwahl(zeile, [('cscu', 'cSCU'), ('scu', 'SCU')], 'cscu',
             lambda k: (einheit.set(k), pruefen()),
             fenster.f_klein).pack(side='left')

    # ⭐ **Eigenes Lagerort-Feld.** Vorher galt stillschweigend der Ort aus dem
    # Formular ganz oben — der steht seit dem Umbau weit weg, und wer ihn für
    # eine Ausbeute ändern wollte, musste hochrollen und danach zurück. Am
    # 30.08.2026 gemeldet: „man kann für Raffinerie-Ausbeute keinen Lagerort
    # angeben."
    #
    # Vorbelegt mit dem Ort von oben, damit sich für alle, die immer am selben
    # Ort einlagern, nichts ändert.
    ort_raff = tk.StringVar(value=(ort_var.get() or '').strip())
    ortblock = tk.Frame(ziel, bg=BG)
    ortblock.pack(fill='x', pady=(4, 0))
    tk.Label(ortblock, text=t('s_rf_ort'), bg=BG, fg=FG,
             font=fenster.f_fett, anchor='w').pack(fill='x')
    _ozeile, _oliste, _ozeichnen = _auswahlfeld(fenster, ortblock, ort_raff,
                                                _orte_modul.alle)
    _ozeile.pack(fill='x', pady=(4, 0))
    _oliste.pack(fill='x')

    kasten = rundrahmen(ziel, '#0c1017', LINIE, radius=8, grundfarbe=BG)
    kasten.halter.pack(fill='x', pady=(4, 6))
    feld = tk.Text(kasten, bg='#0c1017', fg=FG, font=('Consolas', 10),
                   height=7, wrap='none', relief='flat', bd=0,
                   insertbackground=FG, highlightthickness=0)
    feld.pack(fill='both', expand=True, padx=12, pady=10)

    vorschau = tk.Label(ziel, text=t('s_rf_nichts'), bg=BG, fg=SUB,
                        font=fenster.f_klein, anchor='w', justify='left')
    vorschau.pack(fill='x')
    knopf_platz = tk.Frame(ziel, bg=BG)
    knopf_platz.pack(anchor='w', pady=(6, 0))
    stand = {'posten': []}

    def pruefen(*_):
        """Beim Tippen mitrechnen — man sieht sofort, was hineinginge."""
        posten, fehlerhaft = lager.raffinerie_zeilen(
            feld.get('1.0', 'end-1c'), einheit.get())
        stand['posten'] = posten
        teile = []
        for name, menge, guete in posten[:8]:
            teile.append('%s %s · Q %d' % (name, _menge_text(menge), guete))
        if len(posten) > 8:
            teile.append('…')
        for roh, grund in fehlerhaft[:4]:
            teile.append('⚠ %s — %s' % (roh, grund))
        vorschau.configure(
            text='\n'.join(teile) if teile else t('s_rf_nichts'),
            fg=GOLD if fehlerhaft else SUB)
        for w in knopf_platz.winfo_children():
            w.destroy()
        if posten:
            _knopf(fenster, knopf_platz, t('s_rf_knopf') % len(posten),
                   uebernehmen, stark=True).pack(side='left')

    def uebernehmen():
        # ⚠ Geschlossene Liste wie überall: Was UEX nicht kennt, kommt nicht
        # ins Lager. Leer ist erlaubt — der Lagerort ist freiwillig.
        if not _orte_modul.kennt((ort_raff.get() or '').strip()):
            meldung.configure(text=t('s_rf_ort_unbekannt'), fg=ROT)
            return
        # ⚠⚠ **Der Ort läuft NICHT durch `lager_name()`.** Die Funktion zieht
        # eine Eingabe auf einen bekannten **Rohstoff** — sie vergleicht gegen
        # `einlagerbar()`. Ein Ortsname steht dort nie drin, also kam immer
        # `None` zurück, und `or ''` machte daraus einen **leeren Lagerort**:
        # Wer „Levski" gewählt hatte, bekam seine ganze Ausbeute ohne Ort
        # eingebucht. Am 30.08.2026 gemeldet.
        #
        # Der Ort wird gegen die Ortsliste geprüft, so wie im Formular oben.
        ziel_ort = (ort_raff.get() or '').strip()
        for name, menge, guete in stand['posten']:
            lager.eintragen(name, menge, guete, ziel_ort)
        anzahl = len(stand['posten'])
        feld.delete('1.0', 'end')
        pruefen()
        neu_zeichnen()
        meldung.configure(text=t('s_rf_fertig') % anzahl, fg=ACCENT)

    feld.bind('<KeyRelease>', pruefen)

    def _umschalten(_=None):
        if ziel.winfo_ismapped():
            ziel.pack_forget()
            pfeil.symbol_tauschen('aufklappen')
            pfade.einstellung_setzen('lager_raffinerie_offen', False)
        else:
            # ⚠ `after=kopf` — sonst haengt der Koerper beim zweiten Aufklappen
            # unter allem, was inzwischen dazugekommen ist, statt unter seiner
            # eigenen Kopfzeile.
            ziel.pack(fill='x', after=kopf)
            pfeil.symbol_tauschen('zuklappen')
            pfade.einstellung_setzen('lager_raffinerie_offen', True)

    # Die ganze Kopfzeile ist die Schaltflaeche, nicht nur das Symbol: Ein
    # Pfeil von zwoelf Pixeln ist kein Ziel, das man treffen will.
    for teil in (kopf, pfeil) + tuple(kopf.winfo_children()):
        teil.bind('<Button-1>', _umschalten)
    # ⚠⚠ **`einstellung_wahrheit`, nicht `einstellung`.** Letztere liefert
    # einen PFAD und ruft dafür `.strip()` auf dem Wert auf. Hier steht aber
    # ein Ja/Nein: Sobald der Block einmal aufgeklappt war, lag `True` in der
    # Datei — und `True.strip()` warf einen AttributeError. Der traf nicht nur
    # diesen Block, sondern riss den Aufbau der GANZEN Lager-Seite ab: Die
    # Liste der eingetragenen Posten fehlte danach völlig, obwohl die Daten
    # unversehrt waren. Gemeldet am 03.09.2026, drin seit v3.4.1.
    #
    # ⚠ Und es blieb kaputt, bis das Programm neu startete — eine Seite wird
    # nur EINMAL gebaut (siehe `oeffnen()`). Zuklappen half also nicht.
    if pfade.einstellung_wahrheit('lager_raffinerie_offen', False):
        _umschalten()
    return feld


def _menge_text(menge):
    """Eine Menge kurz und ohne Nullen am Ende — 1,88 statt 1,8800."""
    return ('%g' % round(menge, 4)).replace('.', ',')


def _lager(fenster, rahmen):
    """Das eigene Rohstoff-Lager: eintragen, ansehen, löschen."""
    from . import rohstoffe as lager
    _ueberschrift(fenster, rahmen, t('hf_lager'), t('s_lg_lead'))
    innen = _rollflaeche(rahmen)

    _fliesstext(innen, t('s_lg_hinweis'), fenster.f_klein, fill='x')

    from .hauptfenster import rundes_feld
    material = tk.StringVar()
    menge = tk.StringVar()
    guete = tk.StringVar()
    # Der zuletzt benutzte Lagerort steht schon drin — siehe unten beim
    # Eintragen, warum.
    ort = tk.StringVar(value=pfade.einstellung('lager_ort') or '')
    # ⭐ In welcher Einheit das Mengenfeld rechnet. Das Raffinerie-Terminal im
    # Spiel zeigt **cSCU**, die Gegenstands-Anzeige im Lager **SCU** — und vom
    # Terminal abzutippen ist bequemer, weil man dort nicht jeden Stapel
    # einzeln mit der Maus anfahren muss (Wunsch vom 30.08.2026). Das Kästchen
    # neben dem Feld schaltet um; die Beschriftung sagt immer, was gerade gilt.
    cscu = [pfade.einstellung('lager_einheit') == 'cscu']

    def _faktor():
        return lager.CSCU if cscu[0] else 1.0

    # Welche Zeile gerade zum Ändern offen ist. `None` heisst: neuer Posten.
    # ⚠ Die Nummer ist die Position in der ungefilterten Liste — nicht die
    # Position in der Anzeige. Sortieren und Filtern duerfen sie nicht
    # verschieben, sonst berichtigt man den falschen Posten.
    bearbeitung = {'nummer': None}

    # Ein Name, den der Nutzer trotz Warnung eintragen will. Steht er hier,
    # laesst ihn der naechste Klick durch — einmal, fuer genau diesen Namen.
    frei = {'name': None}

    mengen_vorschau = None
    # ⚠⚠ **Beschriftung ÜBER dem Feld** — dasselbe Bild wie im Handelslager
    # (Wunsch vom 30.08.2026: „damit wir überall das gleiche Bild haben").
    # Die alte Zeilenform (Bezeichnung links, Feld rechts) verträgt sich nicht
    # mit einem Feld, das im Betrieb wächst: Klappt die Auswahlliste auf, wird
    # die Zeile zehn Zeilen hoch und Tk setzt die Beschriftung auf halbe Höhe.
    ware_zeichnen = ort_zeichnen = lambda: None
    mengen_beschriftung = None
    for beschriftung, var in ((t('s_lg_material'), material),
                              (t('s_lg_menge'), menge),
                              (t('s_lg_qualitaet'), guete),
                              (t('s_lg_ort'), ort)):
        block = tk.Frame(innen, bg=BG)
        block.pack(fill='x', padx=24, pady=(12, 0))
        kopf_label = tk.Label(block, text=beschriftung, bg=BG, fg=FG,
                              font=fenster.f_fett, anchor='w')
        kopf_label.pack(fill='x')

        if var is material or var is ort:
            # ⭐ Auswahlfeld: tippen **oder** den Pfeil anklicken und aussuchen.
            # Ohne Vorschläge tippt jemand „Aslerite", bekommt nie einen
            # Treffer und sucht den Fehler bei sich.
            if var is material:
                from . import herstellung as _h_lg

                def _quelle_material():
                    try:
                        return sorted(_h_lg.einlagerbar())
                    except Exception:
                        return []
                quelle = _quelle_material
            else:
                from . import orte as _o_lg
                quelle = _o_lg.alle
            zeile_, liste_, zeichnen_ = _auswahlfeld(fenster, block, var,
                                                     quelle)
            zeile_.pack(fill='x', pady=(4, 0))
            liste_.pack(fill='x')
            if var is material:
                ware_zeichnen = zeichnen_
            else:
                ort_zeichnen = zeichnen_
            continue

        if var is menge:
            # Feld und Kästchen in einer Zeile — das Kästchen rechts daneben,
            # damit die Einheit dort steht, wo die Zahl entsteht.
            _mengenzeile = tk.Frame(block, bg=BG)
            _mengenzeile.pack(fill='x', pady=(4, 0))
            f = rundes_feld(_mengenzeile, var, fenster.f_klein, '#0c1017',
                            LINIE, ACCENT, FG)
            mengen_beschriftung = kopf_label

            def einheit_um(an):
                mengen_beschriftung.configure(
                    text=t('s_lg_menge_cscu') if an else t('s_lg_menge'))
                pfade.einstellung_setzen('lager_einheit',
                                         'cscu' if an else 'scu')
                mengen_vorschau_zeigen()

            # ⚠⚠ **Erst das Kästchen packen, dann das Feld.** In `tkinter`
            # bekommt das zuletzt gepackte Element den übrigen Platz — und ein
            # Feld mit `expand=True` nimmt sich alles. Andersherum gepackt
            # schob es das Kästchen aus dem Fenster.
            _kaestchen(_mengenzeile, t('s_lg_cscu'), cscu, einheit_um,
                       fenster.f_klein).pack(side='right', padx=(10, 0))
            f.halter.pack(side='left', fill='both', expand=True)
            if cscu[0]:
                kopf_label.configure(text=t('s_lg_menge_cscu'))
            # ⭐⭐ **Die Vorschau ist die eigentliche Erklärung.** Wer beim
            # Tippen von „1.04+3" daneben „ergibt 4,04 SCU" liest, braucht
            # keinen Satz über Auf- und Abbuchen mehr.
            mengen_vorschau = tk.Label(block, text='', bg=BG, fg=ACCENT,
                                       font=fenster.f_klein, anchor='w')
            mengen_vorschau.pack(fill='x')
        else:
            f = rundes_feld(block, var, fenster.f_klein, '#0c1017', LINIE,
                            ACCENT, FG)
            f.halter.pack(fill='x', pady=(4, 0))

    # ℹ Die früheren „Meintest du:"-Zeilen für Rohstoff und Lagerort sind
    # entfallen: Das Auswahlfeld filtert beim Tippen selbst und zeigt auf
    # Knopfdruck die ganze Liste. Zwei Wege für dieselbe Hilfe nebeneinander
    # wären eine Bedienung zu viel.

    def _bestand_vorher():
        """Wie viel im gerade bearbeiteten Posten liegt — sonst 0."""
        nr = bearbeitung['nummer']
        if nr is None:
            return 0.0
        posten = lager.laden()
        return float(posten[nr].get('menge') or 0) if 0 <= nr < len(posten) else 0.0

    def mengen_vorschau_zeigen(*_):
        """Zeigt beim Tippen, was herauskommt.

        ⚠ Nur bei einer **Rechnung**, nicht bei einer blossen Zahl: Wer „4,5"
        tippt, weiss, dass 4,5 herauskommt — „ergibt 4,5 SCU" wäre Rauschen.
        """
        if mengen_vorschau is None:
            return
        roh = (menge.get() or '').strip()
        rechnung = any(z in roh[1:] for z in '+-−') or roh[:1] in '+-−'
        if not roh or not rechnung:
            mengen_vorschau.pack_forget()
            return
        # ⚠ `vorher` kommt aus dem Lager und ist SCU — das Feld rechnet aber
        # in der gewählten Einheit. Ohne Umrechnung addiert „+3" auf einen
        # hundertfach zu grossen Ausgangswert.
        vorher = _bestand_vorher() / _faktor()
        wert = lager.rechnen(roh, vorher)
        if wert is None:
            mengen_vorschau.pack_forget()
            return
        if wert < 0:
            mengen_vorschau.configure(text=t('s_lg_ergibt_minus') % vorher,
                                      fg=GOLD)
        elif wert == 0:
            mengen_vorschau.configure(text=t('s_lg_ergibt_null'), fg=GOLD)
        else:
            mengen_vorschau.configure(text=t('s_lg_ergibt') % round(wert, 3),
                                      fg=ACCENT)
        mengen_vorschau.pack(fill='x', pady=(4, 0))

    menge.trace_add('write', mengen_vorschau_zeigen)

    liste_rahmen = tk.Frame(innen, bg=BG)
    meldung = tk.Label(innen, text='', bg=BG, fg=SUB, font=fenster.f_klein,
                       anchor='w')

    # ⚠ Als **Tabelle mit Spalten**, nicht als Fließtext: Bei 26 Materialien
    # an mehreren Orten wird die Liste lang, und dann sucht man einen Posten,
    # statt ihn zu sehen. Spaltenköpfe sortieren auf Klick, das Feld darüber
    # filtert. (Wunsch von Xharig, 29.08.2026.)
    sortier = {'nach': 'material', 'ab': False}
    filter_var = tk.StringVar()

    SPALTEN = (('material', 's_lg_sp_material', 22, 'w'),
               ('menge',    's_lg_sp_menge',     9, 'e'),
               ('qualitaet', 's_lg_sp_q',        9, 'e'),
               # ⭐ Womit man das holt — Hand, Fahrzeug oder Schiff. Steht in
               # den Bergbaudaten und beantwortet die Frage, die nach „habe
               # ich genug?" kommt: „und wie komme ich an mehr?"
               ('abbau',    's_lg_sp_abbau',    10, 'w'),
               ('ort',      's_lg_sp_ort',      16, 'w'))

    def _abbau_text(material):
        """Hand / Fahrzeug / Schiff — oder leer, wenn die Daten fehlen."""
        try:
            from . import bergbau as berg
            arten = berg.abbauart(material)
        except Exception:
            return ''
        namen = []
        for schluessel, text_ in (('fps', 's_lg_abbau_fps'),
                                  ('fahrzeug', 's_lg_abbau_fahrzeug'),
                                  ('schiff', 's_lg_abbau_schiff')):
            if schluessel in arten:
                namen.append(t(text_))
        return ' · '.join(namen)

    def zeichnen():
        for w in liste_rahmen.winfo_children():
            w.destroy()
        posten = lager.laden()
        if not posten:
            _fliesstext(liste_rahmen, t('s_lg_leer'), fenster.f_klein, fill='x')
            return

        arten = len({(p.get('material') or '').lower() for p in posten})
        summe_txt = (t('s_lg_summe_eins') % len(posten) if arten == 1
                     else t('s_lg_summe') % (len(posten), arten))
        tk.Label(liste_rahmen, text=summe_txt, bg=BG, fg=SUB,
                 font=fenster.f_klein, anchor='w').pack(fill='x', pady=(0, 6))

        # (Das Suchfeld steht ausserhalb dieser Funktion — siehe dort.)

        # --- Kopfzeile ---
        kopf = tk.Frame(liste_rahmen, bg=BG)
        kopf.pack(fill='x', pady=(0, 2))

        def sortieren(nach):
            if sortier['nach'] == nach:
                sortier['ab'] = not sortier['ab']
            else:
                sortier['nach'], sortier['ab'] = nach, False
            zeichnen()

        for schluessel, textkey, breite, anker_ in SPALTEN:
            pfeil = ''
            if sortier['nach'] == schluessel:
                pfeil = ' ▾' if sortier['ab'] else ' ▴'
            lbl = tk.Label(kopf, text=t(textkey) + pfeil, bg=BG,
                           fg=(ACCENT if sortier['nach'] == schluessel else SUB),
                           font=fenster.f_klein, width=breite, anchor=anker_,
                           cursor='hand2')
            lbl.pack(side='left', padx=(0, 8))
            lbl.bind('<Button-1>', lambda _e, k=schluessel: sortieren(k))

        # --- Zeilen ---
        text = filter_var.get().strip().lower()
        sichtbar = [(i, p) for i, p in enumerate(posten)
                    if not text
                    or text in (p.get('material') or '').lower()
                    or text in (p.get('ort') or '').lower()]

        def schluessel_von(paar):
            p = paar[1]
            wert = p.get(sortier['nach'])
            if sortier['nach'] in ('menge', 'qualitaet'):
                return float(wert or 0)
            return str(wert or '').lower()

        sichtbar.sort(key=schluessel_von, reverse=sortier['ab'])

        if not sichtbar:
            _fliesstext(liste_rahmen, t('s_lg_nichts_da'), fenster.f_klein,
                        fill='x')
            return

        for nummer, p in sichtbar:
            offen = bearbeitung['nummer'] == nummer
            # Die offene Zeile bekommt Flaeche unter sich, damit man sieht,
            # welchen Posten die Felder oben gerade zeigen.
            z_bg = FLAECHE if offen else BG
            z = tk.Frame(liste_rahmen, bg=z_bg)
            z.pack(fill='x', pady=1)
            # ⚠ Erst in Variablen holen. `text=p.get('material')` liest
            # `texte_pruefen.py` als festen Oberflächentext „material" und
            # meldet ihn — ein Fehlalarm, der die Prüfung rot färbt.
            name_txt = p.get('material') or '?'
            menge_txt = '%g' % float(p.get('menge') or 0)
            q_txt = ('%g' % float(p['qualitaet'])) if p.get('qualitaet') else '—'
            abbau_txt = _abbau_text(name_txt) or '—'
            ort_txt = p.get('ort') or '—'
            # ⚠⚠ **„Löschen" MUSS vor den Spalten gepackt werden.** Tk gibt
            # den Platz in der Reihenfolge des Packens: Was links zuerst
            # kommt, nimmt sich seine Breite, und der rechte Rest bekommt, was
            # übrig ist — bei fünf Spalten mit fester Breite also unter
            # Umständen nichts. Auf dem Bildschirm stand deshalb „chen" statt
            # „Löschen" (30.08.2026 gemeldet). Zuerst gepackt, reserviert es
            # seinen Platz, und die Spalten teilen sich den Rest.
            weg = tk.Label(z, text=t('s_lg_weg'), bg=z_bg, fg=SUB,
                           font=fenster.f_klein, cursor='hand2', anchor='e')
            weg.pack(side='right', padx=(8, 4))
            # ⚠ Rollstelle halten — sonst springt die Seite beim Löschen nach
            # ganz oben, und wer beim zwölften Posten war, sucht sich neu
            # zurecht (30.08.2026 gemeldet).
            weg.bind('<Button-1>',
                     lambda _e, n=nummer: _rollstelle_halten(
                         weg, lambda: (lager.entfernen(n),
                                       verwerfen(), zeichnen())))

            spalten_labels = []
            for wert, (_k, _tk, breite, anker_), farbe, schrift in (
                    (name_txt, SPALTEN[0], FG, fenster.f_grund),
                    (menge_txt, SPALTEN[1], ACCENT, fenster.f_grund),
                    (q_txt, SPALTEN[2], SUB, fenster.f_klein),
                    (abbau_txt, SPALTEN[3], SUB, fenster.f_klein),
                    (ort_txt, SPALTEN[4], SUB, fenster.f_klein)):
                lbl = tk.Label(z, text=wert, bg=z_bg, fg=farbe, font=schrift,
                               width=breite, anchor=anker_, cursor='hand2')
                lbl.pack(side='left', padx=(0, 8))
                spalten_labels.append(lbl)

            # Die ganze Zeile oeffnet den Posten zum Berichtigen. ⚠ Auch jedes
            # Label einzeln binden — ein Label verschluckt den Klick, sonst
            # trifft man nur die Luecken dazwischen.
            #
            # ⚠ **Nur die Spalten**, nicht „Löschen": Das hat seine eigene
            # Aufgabe. Frueher ergab sich das von selbst, weil es nach dieser
            # Schleife entstand — jetzt wird es ausdruecklich ausgelassen.
            z.bind('<Button-1>', lambda _e, n=nummer: bearbeiten(n))
            for kind in spalten_labels:
                kind.bind('<Button-1>', lambda _e, n=nummer: bearbeiten(n))

    filter_var.trace_add('write', lambda *_: zeichnen())

    def bearbeiten(nummer):
        """Einen vorhandenen Posten in die Felder oben holen.

        Bewusst dieselben Felder wie beim Eintragen: eine zweite Eingabemaske
        an anderer Stelle waere ein zweiter Ort zum Suchen.
        """
        posten = lager.laden()
        if not (0 <= nummer < len(posten)):
            return
        p = posten[nummer]
        bearbeitung['nummer'] = nummer
        material.set(p.get('material') or '')
        # ⚠ In der Einheit vorlegen, in der das Feld gerade rechnet — sonst
        # steht beim Bearbeiten eine SCU-Zahl in einem cSCU-Feld und wird beim
        # Speichern durch 100 geteilt.
        menge.set('%g' % round(float(p.get('menge') or 0) / _faktor(), 4))
        guete.set('%g' % float(p['qualitaet']) if p.get('qualitaet') else '')
        ort.set(p.get('ort') or '')
        # ⚠ Erst in eine Variable. Steht `p.get('material')` direkt im
        # `text=`-Ausdruck, meldet `texte_pruefen.py` „material" als festen
        # Oberflächentext — ein Fehlalarm, der die Prüfung rot färbt.
        offen_txt = p.get('material') or '?'
        meldung.configure(text=t('s_lg_bearbeite') % offen_txt, fg=ACCENT)
        # Das Auf- und Abbuchen sieht man dem Feld nicht an — also hinschreiben,
        # und zwar erst dann, wenn es auch gilt.
        rechenhinweis.configure(text=t('s_lg_rechnen'))
        knoepfe_setzen()
        zeichnen()

    def posten_weg(*_):
        """Den gerade offenen Posten löschen — mit Rückfrage."""
        from .hauptfenster import frage_stellen
        nummer = bearbeitung['nummer']
        if nummer is None:
            return
        alle = lager.laden()
        if not (0 <= nummer < len(alle)):
            return
        p_ = alle[nummer]
        if not frage_stellen(fenster.root, t('s_lg_posten_frage_t'),
                             t('s_lg_posten_frage') % (p_.get('material') or '?',
                                                       float(p_.get('menge') or 0))):
            return
        _rollstelle_halten(innen, lambda: (lager.entfernen(nummer),
                                           verwerfen(), zeichnen()))

    def verwerfen(*_):
        """Zurueck zum Eintragen — Felder leeren, nichts speichern."""
        if bearbeitung['nummer'] is None:
            return
        bearbeitung['nummer'] = None
        material.set(''); menge.set(''); guete.set('')
        ort.set(pfade.einstellung('lager_ort') or '')
        meldung.configure(text='', fg=SUB)
        rechenhinweis.configure(text='')
        knoepfe_setzen()
        zeichnen()

    def eintragen(*_):
        name = material.get().strip()
        if not name:
            # ⚠ Nicht stumm zurückkehren. Wer den Knopf drückt und nichts
            # passieren sieht, hält das Feld für kaputt.
            meldung.configure(text=t('s_lg_kein_material'), fg=GOLD)
            return

        # --- Namensabgleich ------------------------------------------------
        # Ein freies Textfeld für einen Namen, der exakt passen muss, ist eine
        # stille Fehlerquelle: „Aslerite" sieht in der Liste richtig aus, wird
        # aber von keinem Rezept gefunden. Vorschlaege allein reichen nicht —
        # sie lassen sich uebergehen.
        from . import herstellung as h_modul
        richtig = h_modul.lager_name(name)
        if richtig is None:
            # ⚠⚠ **HIER endet es. Es gibt keinen Ausweg, und das ist Absicht.**
            #
            # Bis v3.3.0-rc40 stand daneben ein Knopf „Trotzdem eintragen".
            # Damit war das Feld faktisch frei — und ein freies Textfeld heisst,
            # dass jemand Schimpfwoerter, Religioeses oder Politisches
            # eintraegt, ein Bildschirmfoto macht und es verbreitet. Am Ende
            # fragt niemand, wer das getippt hat: Es steht in diesem Werkzeug,
            # also kommt es scheinbar von dessen Autor.
            #
            # Am 30.08.2026 unmissverstaendlich festgelegt: „NUR was auch in
            # der Rohstoff-Liste ist darf speicherbar sein, sonst nichts."
            #
            # Die Liste umfasst alle 39 Mineralien und 13 Pflanzen aus den
            # Spieldaten (`herstellung.einlagerbar()`). Fehlt etwas, wird die
            # LISTE ergaenzt — nicht die Sperre gelockert.
            meldung.configure(text=t('s_lg_name_fremd') % name, fg=GOLD)
            ware_zeichnen()
            return
        if richtig != name:
            # Berichtigung nicht verschweigen. Wer „Aslerite" tippt und
            # „Aslarite" in der Liste findet, soll wissen, warum.
            if richtig.lower() != name.lower():
                meldung.configure(text=t('s_lg_berichtigt') % (name, richtig),
                                  fg=SUB)
            name = richtig
        # Auf- und Abbuchen: „+5" legt dazu, „-2" nimmt weg. Nur sinnvoll,
        # solange ein Posten offen ist — bei einem neuen gibt es nichts, worauf
        # sich das Vorzeichen beziehen koennte, dort zaehlt schlicht die Zahl.
        # ⚠⚠ **Auch „1.04+3" muss gehen.** Beim Bearbeiten steht die aktuelle
        # Menge schon im Feld — wer drei dazulegen will, tippt hinten „+3" an.
        # Bis v3.3.0-rc39 zaehlte nur ein FUEHRENDES Vorzeichen, und genau die
        # natuerliche Eingabe wurde abgelehnt. `lager.rechnen()` kann jetzt
        # beides und liefert direkt die **neue Menge**.
        roh = (menge.get() or '0').strip()
        vorher_menge = _bestand_vorher()
        rechnend = bool(roh) and (roh[:1] in '+-−'
                                  or any(z in roh[1:] for z in '+-−'))
        wert = lager.rechnen(roh, vorher_menge)
        if wert is None:
            # ⚠ Keine Zahl? Dann nichts tun statt abstürzen — jemand tippt
            # „12 SCU" statt „12", und das darf das Fenster nicht kosten.
            # Und die Meldung muss erklären, nicht die Feldbeschriftung
            # wiederholen.
            meldung.configure(text=t('s_lg_keine_menge'), fg=GOLD)
            return
        if rechnend:
            nr = bearbeitung['nummer']
            vorher = vorher_menge
            neu_wert = wert
            if neu_wert < 0 and nr is None:
                # ⚠ Beim ANLEGEN gibt es keinen Bestand, von dem etwas
                # abgehen könnte. „So viel ist nicht da. Vorhanden: 0 SCU"
                # las sich dort wie ein Buchhaltungsfehler, dabei ist die
                # Eingabe schlicht sinnlos. Am 30.08.2026 aufgefallen: „-2"
                # in ein leeres Formular.
                meldung.configure(text=t('s_lg_nicht_negativ'), fg=GOLD)
                return
            if neu_wert < 0:
                # ⚠ Nicht stillschweigend auf 0 setzen. Wer sich um eine Ziffer
                # vertippt, soll den Bestand sehen, nicht ihn verlieren.
                meldung.configure(text=t('s_lg_zu_wenig') % vorher, fg=GOLD)
                return
            if neu_wert == 0 and nr is not None:
                # Alles abgegeben — dann hat der Posten keinen Zweck mehr.
                lager.entfernen(nr)
                bearbeitung['nummer'] = None
                material.set(''); menge.set(''); guete.set('')
                ort.set(pfade.einstellung('lager_ort') or '')
                meldung.configure(text=t('s_lg_alles_weg') % name, fg=SUB)
                knoepfe_setzen()
                zeichnen()
                return
            wert = neu_wert
        elif wert <= 0:
            # Ein Posten mit 0 SCU ist Ballast in der Liste.
            meldung.configure(text=t('s_lg_keine_menge'), fg=GOLD)
            return
        # --- Lagerort ------------------------------------------------------
        # ⚠⚠ Geschlossene Liste, wie beim Rohstoffnamen — und aus demselben
        # Grund: Ein freies Textfeld lässt sich mit allem füllen, was man
        # danach als Bildschirmfoto verbreiten kann. Leer bleiben darf es, das
        # Feld ist freiwillig.
        from . import orte as orte_modul
        ort_richtig = orte_modul.offizieller_name(ort.get())
        if ort_richtig is None:
            meldung.configure(text=t('s_lg_ort_fremd') % ort.get().strip(),
                              fg=GOLD)
            ort_zeichnen()
            return
        ort.set(ort_richtig)

        # Qualität ist Pflicht. Ohne sie kann die Herstellung nicht sagen, was
        # das Material aus dem Produkt macht — und genau dafür ist das Lager da.
        # Der Lagerort bleibt freiwillig: Wer alles an einem Ort hat, soll das
        # nicht 40-mal tippen müssen.
        q_zahl = lager.zahl_lesen(guete.get())
        if q_zahl is None:
            meldung.configure(text=t('s_lg_keine_guete'), fg=GOLD)
            return
        q = int(round(q_zahl))
        if not (0 <= q <= 1000):
            # ⚠ Die Skala der Rezepte ist 0–1000, nicht 0–100. Eine 720 ist
            # gültig, eine 7200 ist ein Vertipper — und würde die
            # Wirkungsrechnung still verzerren.
            meldung.configure(text=t('s_lg_keine_guete'), fg=GOLD)
            return
        # ⚠ Das Feld rechnet in der Einheit, die daneben steht — das Lager
        # immer in SCU. Umgerechnet wird erst hier, nach dem Rechnen: Wer in
        # cSCU „+3" tippt, meint drei cSCU, nicht drei SCU.
        wert = round(wert * _faktor(), 4)
        if bearbeitung['nummer'] is None:
            lager.eintragen(name, wert, q, ort.get())
            hinweis = t('s_lg_eingetragen') % (name, wert)
        else:
            lager.aendern(bearbeitung['nummer'], name, wert, q, ort.get())
            hinweis = t('s_lg_geaendert') % (name, wert)
            bearbeitung['nummer'] = None
        # ⚠ **Der Lagerort bleibt stehen.** Wer eine Raffinerie-Ausbeute
        # einträgt, trägt sechs Posten am selben Ort ein — ihn jedes Mal neu
        # zu wählen ist reine Tipparbeit. Material, Menge und Qualität werden
        # geleert, der Ort nicht; er wird zusätzlich gemerkt, damit er auch
        # beim nächsten Programmstart noch dasteht.
        material.set(''); menge.set(''); guete.set('')
        pfade.einstellung_setzen('lager_ort', ort.get().strip())
        frei['name'] = None
        # Bestätigen: Man soll sehen, dass es angekommen ist.
        meldung.configure(text=hinweis, fg=SUB)
        rechenhinweis.configure(text='')
        knoepfe_setzen()
        zeichnen()

    # ⚠ Die Knopfreihe wird neu gebaut, nicht umbeschriftet. Ein Knopf ist ein
    # Canvas mit fester Breite — „Änderung speichern" passt nicht in die
    # Breite von „Eintragen" und wuerde abgeschnitten.
    rechenhinweis = tk.Label(innen, text='', bg=BG, fg=SUB,
                             font=fenster.f_klein, anchor='w', justify='left')
    rechenhinweis.pack(fill='x', pady=(0, 4))

    knopf_rahmen = tk.Frame(innen, bg=BG)

    def knoepfe_setzen():
        for w in knopf_rahmen.winfo_children():
            w.destroy()
        if bearbeitung['nummer'] is None:
            _knopf(fenster, knopf_rahmen, t('s_lg_eintragen'),
                   eintragen).pack(side='left')
        else:
            _knopf(fenster, knopf_rahmen, t('s_lg_speichern'), eintragen,
                   stark=True).pack(side='left')
            _knopf(fenster, knopf_rahmen, t('s_lg_abbrechen'),
                   verwerfen).pack(side='left', padx=(8, 0))
            # ⭐ Löschen genau dieses Postens — man hat ihn ja gerade offen.
            # Das „Löschen" an der Zeile bleibt daneben bestehen; hier ist es
            # der Weg für den, der schon in der Bearbeitung steckt.
            _knopf(fenster, knopf_rahmen, t('s_lg_posten_weg'), posten_weg,
                   gefahr=True).pack(side='left', padx=(24, 0))

    knoepfe_setzen()
    knopf_rahmen.pack(anchor='w', pady=(4, 10))
    meldung.pack(fill='x')

    _raffinerie_block(fenster, innen, lager, ort, zeichnen, meldung)
    # ⚠⚠ **Das Suchfeld wird EINMAL gebaut — nicht in `zeichnen()`.** Dort
    # stand es bis rc28, und `zeichnen()` räumt bei jeder Änderung den ganzen
    # Listenbereich leer: Mit jedem getippten Buchstaben zerstörte sich das
    # Feld selbst, der Tastaturfokus ging verloren, und man musste für den
    # nächsten Buchstaben neu hineinklicken. Am 30.08.2026 gemeldet: „im Lager
    # bei Eingabe im Suchfeld tabt man automatisch raus".
    #
    # Alles, woran ein Cursor stehen kann, gehört ausserhalb der Zeichenroutine.
    from .hauptfenster import rundes_feld as _rf_suche
    _such_zeile = tk.Frame(innen, bg=BG)
    _such_zeile.pack(fill='x', pady=(6, 0))
    tk.Label(_such_zeile, text=t('s_lg_filter'), bg=BG, fg=SUB,
             font=fenster.f_klein).pack(side='left', padx=(0, 10))
    _such_feld = _rf_suche(_such_zeile, filter_var, fenster.f_klein,
                           '#0c1017', LINIE, ACCENT, FG)
    _such_feld.halter.pack(side='left', fill='x', expand=True)
    _suche_leeren_kreuz(fenster, _such_zeile, filter_var)

    liste_rahmen.pack(fill='both', expand=True, pady=(6, 0))

    # --- Sichern und zurueckholen --------------------------------------
    # ⚠ Das Lager wird von Hand gepflegt — es ist Arbeit, die sonst nirgends
    # liegt. Ohne Ausgabe ist sie beim naechsten Rechnerwechsel weg.
    def _ausgeben(art):
        from . import dateiwahl
        endung = '.csv' if art == 'csv' else '.json'
        ziel = dateiwahl.datei_speichern(
            t('s_lg_ausgeben'),
            vorschlag='lager-%s%s' % (time.strftime('%Y-%m-%d'), endung),
            endung=endung, start=None)
        if not ziel:
            return
        try:
            inhalt = (lager.als_csv() if art == 'csv' else lager.als_json())
            with open(ziel, 'w', encoding='utf-8') as f:
                f.write(inhalt)
            meldung.configure(text=t('s_lg_gespeichert') % os.path.basename(ziel),
                              fg=SUB)
        except Exception as ausnahme:
            fehler.merken('seiten.lager.ausgeben', ausnahme)

    def _einlesen():
        from . import dateiwahl
        quelle = dateiwahl.datei_oeffnen(t('s_lg_einlesen'))
        if not quelle:
            return
        try:
            with open(quelle, encoding='utf-8') as f:
                posten = lager.aus_json(f.read())
        except Exception as ausnahme:
            fehler.merken('seiten.lager.einlesen', ausnahme)
            posten = None
        if posten is None:
            # ⚠ Nicht schweigen. Wer eine falsche Datei waehlt und nichts
            # passieren sieht, haelt das Einlesen fuer kaputt.
            meldung.configure(text=t('s_lg_datei_falsch'), fg=GOLD)
            return
        lager.sichern(posten)
        verwerfen()
        meldung.configure(text=t('s_lg_eingelesen') % len(posten), fg=SUB)
        zeichnen()

    _reihe_aus = tk.Frame(innen, bg=BG)
    _reihe_aus.pack(fill='x', pady=(14, 0))
    _knopf(fenster, _reihe_aus, t('s_lg_aus_json'),
           lambda: _ausgeben('json')).pack(side='left')
    _knopf(fenster, _reihe_aus, t('s_lg_aus_csv'),
           lambda: _ausgeben('csv')).pack(side='left', padx=(8, 0))
    _knopf(fenster, _reihe_aus, t('s_lg_einlesen'),
           lambda: _einlesen()).pack(side='left', padx=(8, 0))

    def _leeren():
        """Das ganze Lager verwerfen — nach Rückfrage.

        ⚠ Rot **und** mit Frage. Das Lager ist Handarbeit, die sonst nirgends
        liegt: kein Log, keine Datenquelle, nur die eigenen Eingaben. Ein
        versehentlicher Klick wäre unwiederbringlich, deshalb steht in der
        Frage auch die Zahl der Posten — „4 Posten werden entfernt" wiegt
        anders als „wirklich löschen?".
        """
        from .hauptfenster import frage_stellen
        anzahl = len(lager.laden())
        if not anzahl:
            return
        if not frage_stellen(fenster.root, t('s_lg_leeren_frage_t'),
                             t('s_lg_leeren_frage') % anzahl):
            return
        lager.sichern([])
        verwerfen()
        meldung.configure(text=t('s_lg_geleert') % anzahl, fg=GOLD)
        zeichnen()

    _knopf(fenster, _reihe_aus, t('s_lg_leeren'), _leeren,
           gefahr=True).pack(side='left', padx=(24, 0))
    _fliesstext(innen, t('s_lg_aus_hilfe'), fenster.f_klein, fill='x')

    zeichnen()



def _wartetext(rest):
    """Die Restzeit der Sperre als `43:12` — oder `''`, wenn sie abgelaufen ist."""
    if rest <= 0:
        return ''
    return '%d:%02d' % (rest // 60, rest % 60)


def _warteton(rest):
    """Welche Farbe die Restzeit hat.

    ⭐ **Kein Rot.** Der Knopf ist gesperrt, *weil* der Abruf eben geklappt hat —
    Rot ist in diesem Programm die Fehlerfarbe und würde nach einem Erfolg das
    Gegenteil melden. Stattdessen „reift" der Knopf von grau nach grün:

    | Restzeit | Farbe | liest sich als |
    |---|---|---|
    | über 30 Min | grau | weit weg, egal |
    | 30 – 5 Min | gold | tut sich was |
    | unter 5 Min | grün | gleich wieder da |

    Gelb und Orange getrennt anzubieten hätte nichts gebracht: Die Palette hat
    dafür nur `GOLD`, zwei Töne davon unterscheidet im Betrieb niemand.
    """
    if rest > 30 * 60:
        return SUB
    if rest > 5 * 60:
        return GOLD
    return ACCENT


def _alterstext(sekunden):
    """Wie alt eine Meldung ist, in Worten — `None` ergibt `''`."""
    if sekunden is None:
        return ''
    stunden = sekunden / 3600.0
    if stunden < 1:
        return t('s_vk_alter_frisch')
    if stunden < 24:
        return t('s_vk_alter_stunden').format(n=int(stunden))
    return t('s_vk_alter_tage').format(n=int(stunden / 24))


def _auswahlfeld(fenster, eltern, var, eintraege_holen, hoechstens=10,
                 beim_waehlen=None):
    """Ein Eingabefeld mit Aufklappliste — tippen **oder** aussuchen.

    Gibt `(rahmen, listen_rahmen, neu_zeichnen)` zurück. Der Aufrufer packt
    beide Rahmen selbst, damit die Liste dort landet, wo sie hingehört.

    ⚠⚠ **Kein `ttk.Combobox`.** Die ist ein Systemelement und sieht auf jedem
    Betriebssystem anders aus — dieselbe Überlegung wie bei `_kaestchen`, das
    aus genau diesem Grund kein `tk.Checkbutton` ist. Das Programm hat eine
    Formensprache; ein Kasten, der unter Windows grau und unter KDE blau ist,
    fällt sofort als Fremdkörper auf.

    **Zwei Wege zum selben Ziel**, und man muss nicht wissen, welchen das
    Programm meint:

    | Weg | was passiert |
    |---|---|
    | Pfeil anklicken | die ganze Liste klappt auf |
    | lostippen | dieselbe Liste, auf die Treffer eingedampft |

    ⚠ **Teiltexte, nicht nur Wortanfänge** — dieselbe Erfahrung wie bei den
    Lagerorten in `orte.py`: Wer `Ore` tippt, sucht `Copper (Ore)`.

    ⚠ Es werden **höchstens zehn** Einträge gezeigt, sonst schiebt eine Liste
    mit 114 Waren alles andere aus dem Bild. Darunter steht, wie viele noch
    kommen — verschwiegen wäre schlimmer als abgeschnitten.
    """
    from .hauptfenster import rundes_feld

    zeile = tk.Frame(eltern, bg=BG)
    liste = tk.Frame(eltern, bg=BG)
    offen = {'ja': False}

    feld = rundes_feld(zeile, var, fenster.f_klein, '#0c1017', LINIE, ACCENT,
                       FG)

    # ⚠ Dasselbe Klapp-Symbol wie überall sonst — nicht ein Textpfeil, der je
    # nach Systemschrift anders aussieht als die gezeichneten Symbole daneben.
    pfeil = zeichen.zeile(zeile, 'aufklappen', grund=BG,
                          schrift=fenster.f_klein)
    pfeil.configure(cursor='hand2')

    def _leeren():
        for w in liste.winfo_children():
            w.destroy()

    def zeichnen():
        _leeren()
        text = (var.get() or '').strip().lower()
        alle = eintraege_holen()
        # Steht genau der gewählte Eintrag im Feld, ist nichts mehr zu suchen.
        if text and any(text == e.lower() for e in alle) and not offen['ja']:
            liste.pack_forget()
            pfeil.symbol_tauschen('aufklappen')
            return
        if not text and not offen['ja']:
            liste.pack_forget()
            pfeil.symbol_tauschen('aufklappen')
            return
        treffer = [e for e in alle if text in e.lower()] if text else list(alle)
        pfeil.symbol_tauschen('zuklappen' if offen['ja'] else 'aufklappen')
        if not treffer:
            liste.pack(fill='x', pady=(4, 0))
            tk.Label(liste, text=t('s_vk_nichts_gefunden'), bg=BG, fg=SUB,
                     font=fenster.f_klein, anchor='w').pack(fill='x', pady=3)
            return
        liste.pack(fill='x', pady=(4, 0))
        for name in treffer[:hoechstens]:
            eintrag = tk.Label(liste, text=name, bg=BG, fg=FG,
                               font=fenster.f_klein, anchor='w',
                               cursor='hand2', padx=8, pady=3)
            eintrag.pack(fill='x')
            eintrag.bind('<Button-1>', lambda _e, n=name: waehlen(n))
            eintrag.bind('<Enter>', lambda _e, w=eintrag: w.configure(bg=FLAECHE))
            eintrag.bind('<Leave>', lambda _e, w=eintrag: w.configure(bg=BG))
        rest = len(treffer) - hoechstens
        if rest > 0:
            tk.Label(liste, text=t('s_af_weitere').format(n=rest), bg=BG,
                     fg=SUB, font=fenster.f_klein, anchor='w',
                     padx=8).pack(fill='x', pady=(2, 0))

    def waehlen(name):
        offen['ja'] = False
        if beim_waehlen is not None:
            # Der Verkaufs-Reiter sammelt mehrere Waren: Dort landet der Name
            # in der Auswahl, und das Feld wird wieder leer. Ohne diesen Weg
            # müsste der Aufrufer den Eintrag aus dem Feld zurücklesen.
            beim_waehlen(name)
        else:
            var.set(name)
        zeichnen()

    def umschalten(_=None):
        offen['ja'] = not offen['ja']
        zeichnen()

    pfeil.bind('<Button-1>', umschalten)

    # ⚠ **Erst den Pfeil packen, dann das Feld.** In `tkinter` bekommt das
    # zuletzt gepackte Element den übrigen Platz, und ein Feld mit
    # `expand=True` nimmt sich alles — andersherum schöbe es den Pfeil aus dem
    # Fenster. Genau der Fehler, der im Werkstatt-Lager beim cSCU-Kästchen
    # schon einmal auftrat.
    pfeil.pack(side='right')
    feld.halter.pack(side='left', fill='both', expand=True)

    # Tippen schliesst das Aufklappen wieder — sonst bliebe die volle Liste
    # stehen, während schon gefiltert wird.
    def beim_tippen(*_):
        offen['ja'] = False
        zeichnen()

    var.trace_add('write', beim_tippen)
    return zeile, liste, zeichnen


def _verkauf(fenster, rahmen):
    """Wo man seine Ware los wird — die beste Stelle zuerst."""
    import threading

    from . import handelslager, verkauf as preisdaten
    from .hauptfenster import rundes_feld

    _ueberschrift(fenster, rahmen, t('hf_verkauf'), t('s_vk_lead'))
    innen = _rollflaeche(rahmen)

    # Die ausgewählten Waren. Liste statt Menge, damit die Reihenfolge der
    # Auswahl erhalten bleibt — wer zuerst Gold eintippt, sieht Gold zuerst.
    auswahl = []
    suche = tk.StringVar()
    nur_nqa = [False]
    meldung = {'text': '', 'farbe': SUB}

    ergebnis_rahmen = tk.Frame(innen, bg=BG)
    chip_rahmen = tk.Frame(innen, bg=BG)

    # ------------------------------------------------ Kopf: Abruf und Stand
    kopf = tk.Frame(innen, bg=BG)
    kopf.pack(fill='x', padx=24, pady=(4, 0))

    stand_label = tk.Label(kopf, text='', bg=BG, fg=SUB,
                           font=fenster.f_klein, anchor='w')

    laeuft = {'ja': False}

    def abrufen():
        """Der Knopf. Holt die Preise — **im Hintergrund**.

        ⚠⚠ **Nicht im Oberflächen-Thread abrufen.** Der Abruf darf bis zu 30
        Sekunden dauern (`ZEITLIMIT`), und solange stünde das ganze Fenster
        still: kein Rollen, kein Umschalten, nichts. Für jemanden, der nebenbei
        spielt, sieht ein eingefrorenes Fenster nach Absturz aus.

        Dasselbe Muster wie beim Update-Knopf weiter oben: Arbeit im Thread,
        Rückkehr über `root.after(0, …)`.
        """
        if laeuft['ja']:
            return
        rest = preisdaten.wartezeit()
        if rest:
            meldung['text'], meldung['farbe'] = t('s_vk_gesperrt'), GOLD
            neu_zeichnen()
            return
        laeuft['ja'] = True
        knopf.beschriften(t('s_vk_holt'), SUB)

        def arbeit():
            try:
                ok, grund = preisdaten.aktualisieren(erzwingen=True)
            except Exception as ausnahme:
                fehler.merken('seiten.verkauf_abruf', ausnahme)
                ok, grund = False, 'netz'

            def melden():
                laeuft['ja'] = False
                if ok:
                    meldung['text'], meldung['farbe'] = t('s_vk_geholt'), ACCENT
                elif grund == 'gesperrt':
                    meldung['text'], meldung['farbe'] = t('s_vk_gesperrt'), GOLD
                elif grund == 'aus':
                    meldung['text'], meldung['farbe'] = (t('s_vk_kein_netz_aus'),
                                                         SUB)
                else:
                    meldung['text'], meldung['farbe'] = t('s_vk_fehler'), ROT
                try:
                    if ergebnis_rahmen.winfo_exists():
                        neu_zeichnen()
                except Exception:
                    pass

            try:
                fenster.root.after(0, melden)
            except Exception:
                laeuft['ja'] = False

        threading.Thread(target=arbeit, daemon=True).start()

    knopf = _knopf(fenster, kopf, t('s_vk_holen'), abrufen)
    knopf.pack(side='left')
    stand_label.pack(side='left', padx=(12, 0))

    def _ticker():
        """Zählt die Sperre im Knopf herunter — einmal pro Sekunde.

        ⚠ **Prüft, ob es den Knopf noch gibt.** Beim Seitenwechsel wird der
        Rahmen zerstört, der `after`-Auftrag läuft aber weiter. Ohne diese
        Prüfung greift er auf ein totes Widget zu und das Programm stürzt beim
        Umschalten ab — der Grund, warum hier kein Aufräum-Register nötig ist.
        """
        try:
            if not knopf.winfo_exists():
                return
        except Exception:
            return
        rest = preisdaten.wartezeit()
        if rest:
            knopf.beschriften(_wartetext(rest), _warteton(rest))
        else:
            knopf.beschriften(t('s_vk_holen'), None)
        alter = preisdaten.alter()
        stand_label.configure(
            text=(t('s_vk_stand').format(alter=_alterstext(alter))
                  if alter is not None else t('s_vk_kein_stand')))
        knopf.after(1000, _ticker)

    # ------------------------------------------------------- Warenauswahl
    def waehlen(name):
        if name not in auswahl:
            auswahl.append(name)
        suche.set('')
        neu_zeichnen()

    def entfernen(name):
        if name in auswahl:
            auswahl.remove(name)
        neu_zeichnen()

    def aus_lager():
        """Die Waren aus dem Handelslager übernehmen — die Brücke zum Lager."""
        genommen = 0
        for ware in handelslager.waren_im_lager():
            # ⚠ Nur übernehmen, was die Preisdaten auch kennen. Sonst steht ein
            # Name in der Auswahl, zu dem es nie ein Ergebnis geben kann, und
            # der Nutzer sucht den Fehler bei sich.
            if preisdaten.bekannt(ware) and ware not in auswahl:
                auswahl.append(ware)
                genommen += 1
        if handelslager.hat_gestohlenes():
            nur_nqa[0] = True
        if not genommen:
            meldung['text'] = t('s_vk_lager_leer')
            meldung['farbe'] = SUB
        neu_zeichnen()

    # ------------------------------------------------------- Suchfeld
    suchzeile = tk.Frame(innen, bg=BG)
    suchzeile.pack(fill='x', padx=24, pady=(14, 0))
    tk.Label(suchzeile, text=t('s_vk_ware'), bg=BG, fg=SUB,
             font=fenster.f_klein, anchor='w').pack(fill='x')
    # Auswahlfeld statt blossem Suchfeld: Wer nicht weiss, wie die Ware bei UEX
    # heisst, klappt die Liste auf und sucht sie aus.
    feldzeile, feldliste, such_zeichnen = _auswahlfeld(
        fenster, suchzeile, suche, preisdaten.waren,
        beim_waehlen=lambda name: waehlen(name))
    feldzeile.pack(fill='x', pady=(4, 0))
    feldliste.pack(fill='x')

    def kaestchen_um(an):
        nur_nqa[0] = an
        neu_zeichnen()

    schalterzeile = tk.Frame(innen, bg=BG)
    schalterzeile.pack(fill='x', padx=24, pady=(10, 0))
    _kaestchen(schalterzeile, t('s_vk_nur_nqa'), nur_nqa, kaestchen_um,
               fenster.f_klein).pack(side='left')
    _knopf(fenster, schalterzeile, t('s_vk_aus_lager'),
           aus_lager).pack(side='right')

    chip_rahmen.pack(fill='x', padx=24, pady=(10, 0))
    ergebnis_rahmen.pack(fill='both', expand=True, padx=24, pady=(6, 20))

    def _leeren(halter):
        for kind in halter.winfo_children():
            kind.destroy()

    def _chips():
        """Die gewählten Waren als anklickbare Marken — Klick entfernt sie."""
        _leeren(chip_rahmen)
        if not auswahl:
            return
        reihe = tk.Frame(chip_rahmen, bg=BG)
        reihe.pack(fill='x')
        for name in auswahl:
            marke = tk.Label(reihe, text=name + '  ×', bg=FLAECHE, fg=FG,
                             font=fenster.f_klein, padx=8, pady=3,
                             cursor='hand2')
            marke.pack(side='left', padx=(0, 6), pady=2)
            marke.bind('<Button-1>', lambda e, n=name: entfernen(n))

    def _ergebnis():
        _leeren(ergebnis_rahmen)
        if meldung['text']:
            tk.Label(ergebnis_rahmen, text=meldung['text'], bg=BG,
                     fg=meldung['farbe'], font=fenster.f_klein,
                     anchor='w').pack(fill='x', pady=(0, 8))
            meldung['text'] = ''
        if not auswahl:
            _fliesstext(ergebnis_rahmen, t('s_vk_leer_hinweis'),
                        fenster.f_klein, fill='x')
            return
        orte = preisdaten.orte_fuer(auswahl, nur_nqa=nur_nqa[0])
        if not orte:
            _fliesstext(ergebnis_rahmen, t('s_vk_keine_orte'),
                        fenster.f_klein, fill='x')
            return
        # Mengen aus dem Handelslager — nur dann wird ein echter Erlös gezeigt.
        # ⚠ Ohne Mengen **keine Summe**: Sie wäre eine Behauptung über eine
        # Ladung, die das Werkzeug nicht kennt (siehe `orte_fuer`).
        lagermengen = handelslager.mengen()
        for nummer, ort in enumerate(orte[:40]):
            # ⚠ Die Spaltenüberschrift steht **nur über dem ersten Kasten**.
            # In jedem zu wiederholen war der erste Bau: Bei 40 Orten stand
            # „Ware · SCU · Preis 1 SCU · Gesamtpreis" vierzigmal da und machte
            # die Liste unruhiger, statt sie zu erklären.
            _verkauf_zeile(fenster, ergebnis_rahmen, ort, len(auswahl),
                           lagermengen, mit_kopf=(nummer == 0))

    def neu_zeichnen():
        _chips()
        such_zeichnen()
        _ergebnis()

    neu_zeichnen()
    _ticker()

    # ℹ Der stille Abruf steht **nicht** hier, sondern beim Programmstart
    # (`sc_bp_watcher.py`, neben `preise` und `orte`). Wer die Seite öffnet,
    # soll Daten vorfinden und nicht auf einen Abruf warten — und eine Seite,
    # die beim Betreten von sich aus ins Netz greift, tut das bei jedem
    # Umschalten erneut.


def _verkauf_zeile(fenster, eltern, ort, gesucht, lagermengen,
                   mit_kopf=False):
    """Ein Ankaufsort: wie viele Waren er nimmt, was er zahlt, wie alt das ist."""
    kasten = tk.Frame(eltern, bg=FLAECHE, highlightthickness=1,
                      highlightbackground=LINIE)
    kasten.pack(fill='x', pady=(0, 6))
    innen = tk.Frame(kasten, bg=FLAECHE)
    innen.pack(fill='x', padx=12, pady=8)

    kopf = tk.Frame(innen, bg=FLAECHE)
    kopf.pack(fill='x')

    # ⭐ Die Trefferzahl steht **vorn und farbig**. Sie ist die eigentliche
    # Antwort des Reiters: Ein Ort, der die ganze Ladung nimmt, spart einen
    # Anflug — und das ist mehr wert als ein paar Prozent Aufpreis anderswo
    # (gemessen: 2 % Mehrerlös für zwei zusätzliche Stopps).
    voll = ort['anzahl'] >= gesucht
    tk.Label(kopf, text='%d/%d' % (ort['anzahl'], gesucht), bg=FLAECHE,
             fg=ACCENT if voll else SUB, font=fenster.f_klein,
             width=5, anchor='w').pack(side='left')

    name = ort['terminal']
    beiwerk = ' · '.join(x for x in (ort.get('ort'), ort.get('system')) if x)
    tk.Label(kopf, text=name, bg=FLAECHE, fg=FG, font=fenster.f_klein,
             anchor='w').pack(side='left')
    if beiwerk:
        tk.Label(kopf, text='  ' + beiwerk, bg=FLAECHE, fg=SUB,
                 font=fenster.f_klein, anchor='w').pack(side='left')
    if ort.get('nqa'):
        # Keine Wertung, nur eine Auskunft: Hier wird nicht nach der Herkunft
        # gefragt. Für saubere Ware ist das weder gut noch schlecht.
        tk.Label(kopf, text='  ' + t('s_vk_nqa_marke'), bg=FLAECHE, fg=GOLD,
                 font=fenster.f_klein, anchor='w').pack(side='left')

    alterstext = _alterstext(ort.get('alter'))
    if alterstext:
        # ⚠ Alte Meldungen werden **abgesetzt, nicht versteckt**. Wer eine 10
        # Tage alte Angabe sieht, kann selbst entscheiden, ob ihm das reicht;
        # eine Zeile, die stillschweigend fehlt, lässt ihn das Werkzeug für
        # kaputt halten.
        zu_alt = (ort.get('alter') or 0) > 7 * 24 * 3600
        tk.Label(kopf, text=alterstext, bg=FLAECHE, fg=GOLD if zu_alt else SUB,
                 font=fenster.f_klein, anchor='e').pack(side='right')

    # ⚠ Dasselbe Raster wie im Handelslager: Ware | SCU | Preis 1 SCU |
    # Gesamtpreis. Zwei Ansichten desselben Werkzeugs dürfen ihre Zahlen nicht
    # verschieden anordnen — wer die eine lesen kann, muss die andere blind
    # lesen können.
    zeilen = tk.Frame(innen, bg=FLAECHE)
    zeilen.pack(fill='x', pady=(4, 0))
    # ⚠⚠ **Feste Mindestbreiten, sonst steht jeder Kasten anders.** Jeder Ort
    # ist ein eigenes Raster, und Tk richtet Spalten nur **innerhalb** eines
    # Rasters aus. Ohne feste Breiten war der erste Kasten schmaler als die
    # folgenden — allein, weil über ihm die Spaltenüberschrift steht und die
    # breiter ist als die Zahlen darunter. Untereinander standen die Beträge
    # dann versetzt.
    #
    # Die Werte sind grosszuegig gewaehlt: Ein Betrag, der breiter wird als
    # seine Spalte, schiebt die Ausrichtung wieder auseinander.
    zeilen.grid_columnconfigure(0, weight=1, minsize=140)
    for _spalte, _breite in ((1, 80), (2, 120), (3, 150)):
        zeilen.grid_columnconfigure(_spalte, weight=0, minsize=_breite)

    if mit_kopf:
        hat_mengen = any(lagermengen.get(tr['ware']) for tr in ort['treffer'])
        kopf_zeile = ('', t('s_hl_sp_menge') if hat_mengen else '',
                      t('s_hl_sp_je_scu'),
                      t('s_hl_sp_gesamt') if hat_mengen else '')
        for spalte, titel in enumerate(kopf_zeile):
            tk.Label(zeilen, text=titel, bg=FLAECHE, fg=SUB, padx=8,
                     font=fenster.f_klein,
                     anchor='w' if spalte == 0 else 'e').grid(
                row=0, column=spalte, sticky='ew')

    erloes = 0.0
    for reihe, treffer in enumerate(ort['treffer'], start=1):
        menge = lagermengen.get(treffer['ware'])
        summe = (menge or 0) * treffer['preis']
        erloes += summe
        felder = (
            (treffer['ware'], SUB, 'w'),
            (_menge_text(menge) if menge else '', FG, 'e'),
            (_geld(treffer['preis']), FG, 'e'),
            (_geld(summe) if menge else '', FG, 'e'),
        )
        for spalte, (text, farbe, seite) in enumerate(felder):
            tk.Label(zeilen, text=text, bg=FLAECHE, fg=farbe, padx=8,
                     font=fenster.f_klein, anchor=seite).grid(
                row=reihe, column=spalte, sticky='ew')

    # ⚠⚠ **Eine Summe gibt es nur mit Mengen aus dem Handelslager.** Ohne sie
    # wäre jede Zahl hier eine Behauptung über eine Ladung, die das Werkzeug
    # nicht kennt — siehe `verkauf.orte_fuer`.
    if erloes:
        tk.Label(innen, text=t('s_vk_erloes').format(summe=_geld(erloes)),
                 bg=FLAECHE, fg=ACCENT, font=fenster.f_klein,
                 anchor='e').pack(fill='x', pady=(4, 0))


def _handelslager(fenster, rahmen):
    """Was zum Verkauf im Laderaum liegt — eintragen, ansehen, löschen."""
    from . import handelslager as lager, orte as ortsliste
    from . import verkauf as preisdaten
    from .hauptfenster import rundes_feld

    _ueberschrift(fenster, rahmen, t('hf_handelslager'), t('s_hl_lead'))
    innen = _rollflaeche(rahmen)
    _fliesstext(innen, t('s_hl_hinweis'), fenster.f_klein, fill='x')

    ware = tk.StringVar()
    menge = tk.StringVar()
    ort = tk.StringVar(value=pfade.einstellung('handel_ort') or '')
    gestohlen = [False]
    meldung = {'text': '', 'farbe': SUB}
    # Welche Zeile gerade zum Ändern offen ist. `None` heisst: neuer Posten.
    # ⚠ Die Nummer ist die Position in `lager.laden()` — dieselbe Vorsicht wie
    # im Werkstatt-Lager: Sortieren darf sie nicht verschieben, sonst
    # berichtigt man den falschen Posten.
    bearbeitung = {'nummer': None}

    liste_rahmen = tk.Frame(innen, bg=BG)

    # ⭐ Ware und Ort sind **Auswahlfelder**: tippen oder den Pfeil anklicken
    # und aussuchen. Beide Listen sind geschlossen (siehe `eintragen`), also
    # soll man sie auch sehen können, statt raten zu müssen, was drinsteht.
    ware_zeichnen = ort_zeichnen = lambda: None
    for beschriftung, var in ((t('s_hl_ware'), ware),
                              (t('s_hl_menge'), menge),
                              (t('s_hl_ort'), ort)):
        # ⚠⚠ **Beschriftung ÜBER dem Feld, nicht daneben.** Die Zeilenform aus
        # `_feld` (Bezeichnung links, Bedienelement rechts) vertraegt sich
        # nicht mit einem Feld, das im Betrieb waechst: Klappt die Warenliste
        # auf, wird die Zeile zehn Zeilen hoch, und Tk setzt die Beschriftung
        # auf halbe Hoehe — „Ware" stand dann mitten neben der Liste statt
        # neben seinem Feld. Ein `anchor='n'` half nicht (zweimal versucht).
        #
        # Der Verkaufs-Reiter macht es ohnehin so („Ware suchen" ueber dem
        # Feld). Damit sehen beide Seiten des Handels-Bereichs gleich aus.
        block = tk.Frame(innen, bg=BG)
        block.pack(fill='x', padx=24, pady=(12, 0))
        tk.Label(block, text=beschriftung, bg=BG, fg=FG,
                 font=fenster.f_fett, anchor='w').pack(fill='x')
        if var is menge:
            feld = rundes_feld(block, var, fenster.f_klein, '#0c1017', LINIE,
                               ACCENT, FG)
            feld.halter.pack(fill='x', pady=(4, 0))
            continue
        quelle = (preisdaten.waren if var is ware else ortsliste.alle)
        zeile, liste, zeichnen_ = _auswahlfeld(fenster, block, var, quelle)
        zeile.pack(fill='x', pady=(4, 0))
        # Die Liste sitzt **unter** dem Feld und ist genauso breit — sie gehoert
        # sichtbar zu ihm.
        liste.pack(fill='x')
        if var is ware:
            ware_zeichnen = zeichnen_
        else:
            ort_zeichnen = zeichnen_

    schalter = tk.Frame(innen, bg=BG)
    schalter.pack(fill='x', padx=24, pady=(12, 4))

    def marke_um(an):
        gestohlen[0] = an

    # ⭐ Das Kästchen steht dort, wo im Werkstatt-Lager die Güte steht. Es ist
    # ihr Ersatz: Der Ankaufpreis hängt nicht an der Qualität (`quality` ist im
    # ganzen UEX-Abzug 0), und erbeutete Ware hat ohnehin immer Q 0 — die
    # Frage, die beim Verkauf wirklich zählt, ist eine andere.
    _kaestchen(schalter, t('s_hl_gestohlen'), gestohlen, marke_um,
               fenster.f_klein).pack(side='left')

    def _leeren(halter):
        for kind in halter.winfo_children():
            kind.destroy()

    def eintragen():
        name = (ware.get() or '').strip()
        # ⚠⚠ **Geschlossene Liste, kein Freitext** — dieselbe Regel wie beim
        # Lagerort. Angenommen wird nur, was UEX kennt; sonst steht am Ende ein
        # ausgedachter oder beleidigender Name im Werkzeug, und ein Bildschirm-
        # foto davon macht die Runde.
        if not preisdaten.bekannt(name):
            meldung['text'], meldung['farbe'] = t('s_hl_unbekannt'), ROT
            neu_zeichnen()
            return
        # ⚠⚠ **Der Ort wird genauso gesperrt wie die Ware.** Sonst steht in
        # dem einen Feld eine geschlossene Liste und im anderen darf jeder
        # tippen, was er will — und der Missbrauchsfall (etwas Beleidigendes
        # eintragen, Bildschirmfoto machen, verbreiten) steht wieder offen.
        #
        # ℹ `orte.kennt()` lässt Leeres durch und meldet ohne Ortsliste alles
        # als gültig — der Lagerort ist freiwillig, und beim ersten Start ohne
        # Netz darf das Feld nicht blockieren.
        if not ortsliste.kennt(ort.get()):
            meldung['text'], meldung['farbe'] = t('s_hl_ort_unbekannt'), ROT
            neu_zeichnen()
            return
        if bearbeitung['nummer'] is None:
            ok, grund = lager.eintragen(name, menge.get(), ort.get(),
                                        gestohlen[0])
        else:
            ok, grund = lager.aendern(bearbeitung['nummer'], name,
                                      menge.get(), ort.get(), gestohlen[0])
        if ok:
            bearbeitung['nummer'] = None
            # Der Lagerort bleibt stehen: Wer eine Ladung bucht, bucht meist
            # mehrere Posten am selben Ort. Dasselbe Verhalten wie im
            # Werkstatt-Lager.
            pfade.einstellung_setzen('handel_ort', ort.get() or '')
            ware.set('')
            menge.set('')
            meldung['text'], meldung['farbe'] = t('s_hl_gebucht'), ACCENT
        else:
            meldung['text'] = {'ware': t('s_hl_fehlt_ware'),
                               'menge': t('s_hl_fehlt_menge')}.get(
                                   grund, t('s_hl_fehler'))
            meldung['farbe'] = ROT
        neu_zeichnen()

    # Zeigt beim Tippen, was aus einer Rechnung herauskommt.
    # ⚠ Nur bei einer **Rechnung**, nicht bei einer blossen Zahl: Wer „40"
    # tippt, weiss, dass 40 herauskommt — „ergibt 40 SCU" wäre Rauschen.
    # Dieselbe Regel wie im Werkstatt-Lager.
    vorschau = tk.Label(innen, text='', bg=BG, fg=SUB, font=fenster.f_klein,
                        anchor='w')
    vorschau.pack(fill='x', padx=24)

    def _bestand_vorher():
        nr = bearbeitung['nummer']
        if nr is None:
            return 0.0
        posten = lager.laden()
        return (float(posten[nr].get('menge') or 0)
                if 0 <= nr < len(posten) else 0.0)

    def vorschau_zeigen(*_):
        roh = (menge.get() or '').strip()
        rechnung = any(z in roh[1:] for z in '+-−') or roh[:1] in '+-−'
        if not roh or not rechnung:
            vorschau.configure(text='')
            return
        wert = lager.rechnen(roh, _bestand_vorher())
        if wert is None:
            vorschau.configure(text=t('s_hl_rechnung_kaputt'), fg=GOLD)
        elif wert <= 0:
            # ⚠ Sagen, was **passieren würde** — nicht nur „geht nicht". Wer
            # `100-200` tippt, soll sehen, warum das abgelehnt wird.
            vorschau.configure(text=t('s_hl_unter_null'), fg=ROT)
        else:
            vorschau.configure(text=t('s_hl_ergibt').format(
                menge=_menge_text(wert)), fg=SUB)

    menge.trace_add('write', vorschau_zeigen)

    knopf_rahmen = tk.Frame(innen, bg=BG)
    knopf_rahmen.pack(fill='x', padx=24, pady=(8, 0))

    def abbrechen():
        bearbeitung['nummer'] = None
        ware.set(''); menge.set(''); gestohlen[0] = False
        neu_zeichnen()

    def knoepfe_setzen():
        # ⚠ Die Knopfreihe wird **neu gebaut, nicht umbeschriftet**: Ein Knopf
        # ist eine Leinwand fester Breite, und „Änderung speichern" passt nicht
        # in die Breite von „Eintragen". Genau wie im Werkstatt-Lager.
        for w in knopf_rahmen.winfo_children():
            w.destroy()
        if bearbeitung['nummer'] is None:
            _knopf(fenster, knopf_rahmen, t('s_hl_buchen'), eintragen,
                   stark=True).pack(side='left')
        else:
            _knopf(fenster, knopf_rahmen, t('s_hl_speichern'), eintragen,
                   stark=True).pack(side='left')
            _knopf(fenster, knopf_rahmen, t('s_hl_abbrechen'),
                   abbrechen).pack(side='left', padx=(8, 0))

    def bearbeiten(nummer):
        """Einen Posten ins Formular holen — dann lässt er sich mit `+5`
        oder `-20` nachjustieren."""
        posten = lager.laden()
        if not 0 <= nummer < len(posten):
            return
        p = posten[nummer]
        bearbeitung['nummer'] = nummer
        ware.set(p.get('ware') or '')
        # ⭐ Die aktuelle Menge steht **im Feld**. Wer fünf dazubuchen will,
        # tippt hinten `+5` an und hat `40+5` dastehen — die natürlichste
        # Schreibweise, und `rechnen()` versteht sie.
        menge.set(_menge_text(p.get('menge') or 0))
        ort.set(p.get('ort') or '')
        gestohlen[0] = bool(p.get('gestohlen'))
        neu_zeichnen()

    liste_rahmen.pack(fill='both', expand=True, padx=24, pady=(12, 20))

    def _liste():
        _leeren(liste_rahmen)
        if meldung['text']:
            tk.Label(liste_rahmen, text=meldung['text'], bg=BG,
                     fg=meldung['farbe'], font=fenster.f_klein,
                     anchor='w').pack(fill='x', pady=(0, 8))
            meldung['text'] = ''
        posten = lager.laden()
        if not posten:
            _fliesstext(liste_rahmen, t('s_hl_leer'), fenster.f_klein,
                        fill='x')
            return
        # ⚠ Erst zeigen, wenn etwas dasteht: Ein Hinweis „Zeile anklicken", wo
        # keine Zeile ist, erklärt etwas, das man gar nicht tun kann.
        _fliesstext(liste_rahmen, t('s_hl_aendern_hinweis'), fenster.f_klein,
                    fill='x', pady=(0, 8))
        gesamt = _handelslager_tabelle(
            fenster, liste_rahmen, posten, preisdaten.bester_preis,
            lambda n: _rollstelle_halten(
                liste_rahmen, lambda: (lager.entfernen(n), abbrechen())),
            bearbeiten, bearbeitung['nummer'])
        if gesamt:
            # ⚠ „höchstens" ist wörtlich gemeint: der beste bekannte Ankauf je
            # Ware, ohne Rücksicht darauf, ob ein einzelner Ort alles nimmt.
            # Der Verkaufs-Reiter rechnet die belastbare Zahl je Ort.
            tk.Label(liste_rahmen,
                     text=t('s_hl_gesamt').format(summe=_geld(gesamt)),
                     bg=BG, fg=ACCENT, font=fenster.f_klein,
                     anchor='e').pack(fill='x', pady=(8, 0))

    def neu_zeichnen():
        ware_zeichnen()
        ort_zeichnen()
        vorschau_zeigen()
        knoepfe_setzen()
        _liste()

    # --- Sichern, zurueckholen, leeren ---------------------------------
    # ⭐ **Dieselbe Reihe wie im Werkstatt-Lager**, in derselben Reihenfolge und
    # mit denselben Beschriftungen: Sicherung, Tabelle, Einlesen — Abstand —
    # Löschen in Rot. Zwei Lager, die dasselbe koennen, muessen es an derselben
    # Stelle und mit denselben Worten koennen; sonst sucht man auf der zweiten
    # Seite, was man auf der ersten blind findet.
    #
    # ⚠ Warum es das hier ueberhaupt braucht: Das Handelslager ist Handarbeit
    # wie das andere — das Spiel gibt nichts her. Und beim Patch-Wisch ist der
    # Laderaum leer, waehrend Posten fuer Posten von Hand zu loeschen genau die
    # Fleissarbeit ist, die niemand macht (also bleibt ein falsches Lager
    # stehen und die Verkaufsrechnung luegt).
    def _ausgeben(art):
        from . import dateiwahl
        endung = '.csv' if art == 'csv' else '.json'
        ziel = dateiwahl.datei_speichern(
            t('s_hl_ausgeben'),
            vorschlag='handelslager-%s%s' % (time.strftime('%Y-%m-%d'), endung),
            endung=endung, start=None)
        if not ziel:
            return
        try:
            inhalt = (lager.als_csv() if art == 'csv' else lager.als_json())
            with open(ziel, 'w', encoding='utf-8') as f:
                f.write(inhalt)
            meldung['text'] = t('s_lg_gespeichert') % os.path.basename(ziel)
            meldung['farbe'] = SUB
        except Exception as ausnahme:
            fehler.merken('seiten.handelslager.ausgeben', ausnahme)
            meldung['text'], meldung['farbe'] = t('s_hl_fehler'), ROT
        neu_zeichnen()

    def _einlesen():
        from . import dateiwahl
        quelle = dateiwahl.datei_oeffnen(t('s_lg_einlesen'))
        if not quelle:
            return
        try:
            with open(quelle, encoding='utf-8') as f:
                posten = lager.aus_json(f.read())
        except Exception as ausnahme:
            fehler.merken('seiten.handelslager.einlesen', ausnahme)
            posten = None
        if posten is None:
            # ⚠ Nicht schweigen. Wer eine falsche Datei waehlt und nichts
            # passieren sieht, haelt das Einlesen fuer kaputt.
            meldung['text'], meldung['farbe'] = t('s_hl_datei_falsch'), GOLD
            neu_zeichnen()
            return
        lager.sichern(posten)
        # ⚠ Erst die Bearbeitung schliessen, dann melden: `abbrechen()` zeichnet
        # neu, und die Meldung wird beim Zeichnen verbraucht — andersherum waere
        # sie weg, bevor sie jemand sieht.
        abbrechen()
        meldung['text'] = t('s_hl_eingelesen') % len(posten)
        meldung['farbe'] = SUB
        neu_zeichnen()

    def _lager_leeren():
        """Das ganze Handelslager verwerfen — nach Rückfrage.

        ⚠⚠ **Der Name ist mit Absicht lang.** Sie hiess bis 31.08.2026
        `_leeren` — genau wie der Helfer weiter oben, der die Kinder eines
        Rahmens wegraeumt und **mit** Argument gerufen wird. Die spaetere
        Definition gewinnt in Python: Ab dem Einbau des Loeschen-Knopfes
        scheiterte jeder Aufbau der Liste mit „_leeren() takes 0 positional
        arguments but 1 was given" — die Seite blieb ohne ihre Tabelle. Ging
        so in v3.4.2 an die Nutzer.

        ⚠ Rot **und** mit Frage, wie im Werkstatt-Lager. In der Frage steht die
        Zahl der Posten: „12 Posten werden entfernt" wiegt anders als „wirklich
        löschen?" — und nach einem Patch-Wisch ist genau das der Griff, der
        gemeint ist.
        """
        from .hauptfenster import frage_stellen
        anzahl = len(lager.laden())
        if not anzahl:
            return
        if not frage_stellen(fenster.root, t('s_hl_leeren_frage_t'),
                             t('s_hl_leeren_frage') % anzahl):
            return
        lager.leeren()
        abbrechen()
        meldung['text'], meldung['farbe'] = t('s_hl_geleert') % anzahl, GOLD
        neu_zeichnen()

    _reihe_aus = tk.Frame(innen, bg=BG)
    _reihe_aus.pack(fill='x', padx=24, pady=(0, 4))
    _knopf(fenster, _reihe_aus, t('s_lg_aus_json'),
           lambda: _ausgeben('json')).pack(side='left')
    _knopf(fenster, _reihe_aus, t('s_lg_aus_csv'),
           lambda: _ausgeben('csv')).pack(side='left', padx=(8, 0))
    _knopf(fenster, _reihe_aus, t('s_lg_einlesen'),
           _einlesen).pack(side='left', padx=(8, 0))
    _knopf(fenster, _reihe_aus, t('s_lg_leeren'), _lager_leeren,
           gefahr=True).pack(side='left', padx=(24, 0))
    _fliesstext(innen, t('s_hl_aus_hilfe'), fenster.f_klein, abzug=48,
                fill='x', padx=24, pady=(0, 20))

    neu_zeichnen()


def _handelslager_tabelle(fenster, eltern, posten, preis_von, loeschen,
                          bearbeiten, offen_nr):
    """Das Lager als echte Tabelle. Gibt den Gesamtwert zurück.

    ⚠⚠ **Ein gemeinsames Raster, nicht ein Rahmen je Zeile.** Vorher war jede
    Zeile ein eigener Kasten mit `pack` — dabei richtet sich nichts aneinander
    aus: Die Beträge standen rechtsbündig irgendwo, und man musste raten,
    welche Zahl wozu gehört. Mit `grid` in **einem** Rahmen legt Tk die Spalten
    über alle Zeilen gleich breit an, und die Zuordnung ist zu sehen statt zu
    erraten. Am 30.08.2026 so gewünscht: „wie ne Tabelle aufgebaut bitte, damit
    man die zuordnung zahlen text erkennt".

    Die Zahlenspalten stehen rechtsbündig (`sticky='e'`) — so steht Tausender
    unter Tausender, und ungleich lange Beträge bleiben vergleichbar.
    """
    tabelle = tk.Frame(eltern, bg=BG)
    tabelle.pack(fill='x')

    # Die Ware dehnt sich, alles andere bleibt so breit wie sein Inhalt.
    tabelle.grid_columnconfigure(0, weight=1, minsize=140)
    for spalte, breite in ((1, 120), (2, 80), (3, 120), (4, 150), (5, 0)):
        tabelle.grid_columnconfigure(spalte, weight=0, minsize=breite)

    # Reihenfolge: Ware | Ort | SCU | Preis 1 SCU | Gesamtpreis. Die beiden
    # Texte stehen links beieinander, die drei Zahlen rechts — so muss das Auge
    # nicht zwischen Wort und Zahl hin und her springen.
    kopf = (t('s_hl_sp_ware'), t('s_hl_sp_ort'), t('s_hl_sp_menge'),
            t('s_hl_sp_je_scu'), t('s_hl_sp_gesamt'), '')
    for spalte, titel in enumerate(kopf):
        tk.Label(tabelle, text=titel, bg=BG, fg=SUB, font=fenster.f_klein,
                 padx=8, anchor='e' if spalte in (2, 3, 4) else 'w').grid(
            row=0, column=spalte, sticky='ew', pady=(0, 4))

    gesamt = 0.0
    for nummer, p in enumerate(posten):
        preis = preis_von(p['ware'])
        wert = preis * float(p.get('menge') or 0)
        gesamt += wert
        # Zebra-Streifen statt Kästen: Die Zeilen bleiben unterscheidbar, ohne
        # dass jede ihren eigenen Rahmen und damit ihre eigene Breite bekommt.
        offen = (nummer == offen_nr)
        grund = ACCENT if offen else (FLAECHE if nummer % 2 == 0 else BG)
        vorne = BG if offen else FG
        blass = BG if offen else SUB

        felder = (
            (p['ware'], vorne, 'w'),
            (p.get('ort') or '—', blass, 'w'),
            # ⚠ Nur die Zahl — die Einheit steht in der Spaltenüberschrift.
            # „100 SCU" in jeder Zeile wiederholt, was darüber schon steht,
            # und schiebt die Zahlen auseinander.
            (_menge_text(p.get('menge') or 0), vorne, 'e'),
            (_geld(preis) if preis else '—', blass, 'e'),
            (_geld(wert) if wert else '—', vorne, 'e'),
        )
        zellen = []
        for spalte, (text, farbe, seite) in enumerate(felder):
            # ⚠⚠ **Der Spaltenabstand gehört ins Label (`padx=`), nicht ins
            # Raster.** Mit `grid(padx=…)` liegt zwischen den Zellen eine Lücke
            # in der Seitenfarbe — der Zebra-Streifen zerfällt dann in einzelne
            # Flecken statt einer durchgehenden Zeile. So gesehen im ersten
            # Bau der Tabelle.
            lbl = tk.Label(tabelle, text=text, bg=grund, fg=farbe, padx=8,
                           font=fenster.f_klein, anchor=seite, cursor='hand2')
            lbl.grid(row=nummer + 1, column=spalte, sticky='ew', ipady=3)
            zellen.append(lbl)

        if p.get('gestohlen'):
            # Die Marke hängt an der Ware, nicht in einer eigenen Spalte — sonst
            # bliebe bei sauberer Ladung eine leere Spalte über die ganze Breite.
            zellen[0].configure(text=p['ware'] + '  · ' + t('s_hl_marke'),
                                fg=BG if offen else GOLD)

        for lbl in zellen:
            lbl.bind('<Button-1>', lambda _e, n=nummer: bearbeiten(n))

        kreuz = tk.Label(tabelle, text='×', bg=grund, fg=blass,
                         font=fenster.f_klein, cursor='hand2', padx=8)
        kreuz.grid(row=nummer + 1, column=5, sticky='ew', ipady=3)
        kreuz.bind('<Button-1>', lambda _e, n=nummer: loeschen(n))
        kreuz.bind('<Enter>', lambda _e, w=kreuz: w.configure(fg=ROT))
        kreuz.bind('<Leave>', lambda _e, w=kreuz, f=blass: w.configure(fg=f))

    return gesamt
