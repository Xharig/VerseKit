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

Getrennt von `main_window.py`, weil das zwei verschiedene Fragen sind: Dort
geht es um den **Rahmen** (Reiterleiste, Umschalten, Größe), hier um den
**Inhalt**. So bleibt jede Datei überschaubar, und eine neue Seite ist eine
Funktion, kein Eingriff in den Rahmen.

Die großen Seiten leihen sich die vorhandenen Fenster: `bestandsfenster` und
`einstellungsfenster` können seit v3.0.0 auch in einen übergebenen Rahmen
zeichnen, statt ein eigenes Fenster aufzumachen.
"""
import os
import re
import sys
import threading
import time
import tkinter as tk

from . import bericht, collection as bestand_datei, fehler, catalog as katalog_modul
from . import pfade, icons, fields
from .sprache import t, pa_feld

BG      = '#10141c'
SURFACE = '#161c28'
BAR     = '#1b2230'
FG      = '#e6edf3'
SUB     = '#8b98a5'
ACCENT  = '#9ce430'
LINE   = '#232c3d'
GOLD    = '#e8c353'
RED     = '#e05252'
# Fuer Zustaende, die schiefgingen, ohne eine Stoerung zu sein (abgebrochen,
# fehlgeschlagen). Gedaempft gegenueber `ROT`, das den echten Fehlern gehoert.
RED_PALE = '#c98a8a'

# Wie viele Zeilen das Auftrags-Protokoll zuerst zeigt — der Rest kommt auf
# Klick nach.
#
# ⚠ **Die Zahl ist gemessen, nicht geraten.** Am 07.09.2026 brauchte die Seite
# beim ersten Öffnen **1479 ms**; sie war damit die mit Abstand teuerste im
# Programm, alle übrigen 29 Seiten standen unter 130 ms. In tkinter kostet
# jede Zeile echte Bedienelemente — sie im Voraus zu bauen ist die eigentliche
# Arbeit, nicht das Lesen der Datei.
#
# ⚠ 40, wie `ZEILEN_ZUERST` in der Bauplan-Liste. Zwei verschiedene Zahlen für
# dieselbe Sache wären genau die Art Unterschied, die niemand begründen kann.
LOG_ROWS_FIRST = 40

# ⚠ Die Klassen heißen in den Daten englisch; angezeigt werden sie übersetzt.
#
# ⚠⚠ **Hier auf Modulebene und nicht in der Funktion, die sie zuerst
# brauchte.** Sie stand bis zum 06.09.2026 lokal in der Läden-Seite; als die
# Herstellung dieselbe Übersetzung brauchte, wäre die naheliegende Lösung eine
# zweite Kopie gewesen. Genau so ist `namensform()` einmal dreifach im Programm
# gelandet und auseinandergelaufen.
CLASS_LABELS = {'Civilian': 's_ld_kl_civilian',
                 'Military': 's_ld_kl_military',
                 'Industrial': 's_ld_kl_industrial',
                 'Stealth': 's_ld_kl_stealth',
                 'Competition': 's_ld_kl_competition',
                 'Medical': 's_ld_kl_medical',
                 'Mining': 's_ld_kl_mining',
                 'Salvage and Repair': 's_ld_kl_salvage'}

# Die Browser-Erweiterung, aus der der Hangar-Import kommt. Fremde Arbeit,
# freiwillig gepflegt — sie wird genannt und verlinkt, nicht stillschweigend
# vorausgesetzt. Ein Import, dessen Quelle man nicht findet, ist kein Angebot.
#
# ⚠ Die Projektseite und **nicht** ein einzelner Store: Welchen Browser der
# Spieler benutzt, weiß das Werkzeug nicht, und dort stehen alle drei
# nebeneinander (Chrome, Firefox, Opera).
XPLORER_PAGE = 'https://github.com/dolkensp/HangarXPLOR'
XPLORER_FIREFOX = ('https://addons.mozilla.org/en-US/firefox/addon/'
                   'star-citizen-hangar-xplorer/')
XPLORER_CHROME = ('https://chrome.google.com/webstore/detail/hangarxplor/'
                  'bhkgemjdepodofcnmekdobmmbifemhkc/')


def _builders():
    """Welche Kennung von welcher Funktion gebaut wird.

    ⚠ Als eigene Funktion, damit das Hauptfenster die Kennungen abfragen kann,
    ohne eine Seite zu bauen — nötig fürs Vorbauen im Leerlauf
    (`_prebuild_pages`). Die Namen stehen erst hier unten im Modul zur
    Verfügung, deshalb eine Funktion und keine Konstante ganz oben.
    """
    return {
        'liste':       _blueprint_list,
        'fortschritt': _progress,
        'auftragslog': _contract_log,
        'allgemein':   _general,
        'anzeige':     _display,
        'ordner':      _folders,
        'spiel':       _game,
        'bestand':     _collection,
        'wasistneu':   _whats_new,
        'patchaenderungen': _patch_changes,
        'ueber':       _about,
        'serverstatus': _server_status,
        'danke':       _thanks,
        'erkennung':   _detection,
        'joysticks':   _joysticks,
        'achsen':      _axes,
        'blickwinkel': _view_angle,
        'diagnose':    _diagnostics,
        'hangar':      _hangar,
        'wunschliste': _wishlist,
        'asop':        _asop,
        'einkaufsliste': _shopping_list,
        'farmliste':   _farm_list,
        'bergung':     _salvage,
        'zerlegen':    _dismantle,
        'herstellung': _crafting,
        'bergbau':     _mining,
        'raffinerien': _refineries,
        'lager':       _storage,
        'verkauf':     _selling,
        'handelslager': _trade_storage,
        'laeden':      _shops,
        'routen':      _routes,
    }


def page_ids():
    """Alle Seiten-Kennungen — für das Vorbauen im Leerlauf."""
    return tuple(_builders())


def build(fenster, kennung, rahmen):
    """Eine Seite füllen. `fenster` ist das Hauptfenster (Schriften, Meldungen)."""
    bauer = _builders().get(kennung)
    if bauer:
        bauer(fenster, rahmen)


# ------------------------------------------------------------------ Bausteine
def _heading(window, frame, title, lead=''):
    tk.Label(frame, text=title, bg=BG, fg=FG, font=window.f_title,
             anchor='w').pack(fill='x', padx=24, pady=(20, 2))
    if lead:
        # `abzug=48` sind die beiden Ränder von je 24 — ohne sie rechnet der
        # Umbruch mit Platz, den es nicht gibt, und die letzten Wörter fallen
        # trotzdem heraus.
        einleitung = tk.Label(frame, text=lead, bg=BG, fg=SUB,
                              font=window.f_small, anchor='w', justify='left')
        einleitung.pack(fill='x', padx=24, pady=(0, 14))
        _wrap(einleitung, inset=48)


def _wide_area(frame, bg=None):
    """Ein Bereich, der **waagerecht** rollt — für breite Tabellen.

    ⛔⛔ **Der Seitenkörper darf nie waagerecht rollen** (Projektregel). Zu
    breiter Inhalt bekommt deshalb seine eigene Fläche, statt die Seite zu
    dehnen — oder, was bis zum 14.09.2026 geschah, **still abgeschnitten** zu
    werden: Auf der Raffinerien-Seite fehlten bei „sehr groß" drei Spalten,
    und zwar in einer ausgelieferten Fassung. Tk meckert dabei nicht.

    ⚠ Im Quelltext stand seit dem ersten Tag ein Kommentar, die Tabelle habe
    so eine Fläche. Sie hatte sie nie. **Ein Kommentar ist kein Bauteil.**

    ⚠ Anders als `_scroll_area()` wird die Breite des inneren Rahmens **nicht**
    auf die Leinwand gezwungen — genau die soll er ja überschreiten dürfen.

    ⚠ Der Balken erscheint nur, wenn es etwas zu rollen gibt. Eine Bahn ohne
    Aufgabe sagt „hier lässt sich schieben", und das stimmt dann nicht.
    """
    bg = bg or SURFACE
    aussen = tk.Frame(frame, bg=bg)
    aussen.pack(fill='x')
    leinwand = tk.Canvas(aussen, bg=bg, highlightthickness=0)
    from .main_window import round_scrollbar
    balken = round_scrollbar(aussen, leinwand, bg=bg, orient='horizontal')
    innen = tk.Frame(leinwand, bg=bg)
    leinwand.create_window((0, 0), window=innen, anchor='nw')
    leinwand.configure(xscrollcommand=balken.set)

    def _nachmessen(_e=None):
        leinwand.configure(scrollregion=leinwand.bbox('all'))
        # ⚠ Die Höhe muss mitwachsen: Eine Leinwand ist von sich aus 100 px
        # hoch, egal was darin steht — die Tabelle wäre unten abgeschnitten.
        leinwand.configure(height=innen.winfo_reqheight())
        if innen.winfo_reqwidth() <= leinwand.winfo_width():
            balken.pack_forget()
        else:
            balken.pack(side='bottom', fill='x', pady=(4, 0))

    innen.bind('<Configure>', _nachmessen)
    leinwand.bind('<Configure>', _nachmessen)
    leinwand.pack(side='top', fill='x', expand=True)
    return innen


def _scroll_area(frame, inset=24, height=None):
    """Ein Bereich, der rollt. Leinwand + Balken, wie im Einstellungsfenster.

    ⚠ Die Fläche wird **zuletzt** gepackt und bekommt `expand=True` — alles,
    was fest bleiben soll, muss vorher gepackt sein.

    `rand` rückt den Inhalt ein. Das gehört hierher und nicht in jeden Baustein:
    Die Bausteine stammen aus dem alten Einstellungsfenster und packen sich ohne
    Rand — neben eingerückten Überschriften sahen sie aus, als wären sie
    verrutscht.
    """
    # ⚠ `hoehe` macht daraus einen Bereich mit **fester** Höhe, der nicht
    # mitwächst — für Listen, die mit der Zeit länger werden. Ohne das frisst
    # so eine Liste irgendwann die ganze Seite: Gemessen am 07.09.2026 braucht
    # eine Patch-Zeile 30 px, und die eigene Patch-Sammlung wächst absichtlich
    # über die zehn hinaus, die die Quelle vorhält. Bei 30 Patches blieben für
    # den eigentlichen Inhalt noch 41 px, bei 50 gar nichts mehr.
    aussen = tk.Frame(frame, bg=BG)
    aussen.pack(fill='both' if height is None else 'x',
                expand=(height is None))
    if height:
        # ⚠ `height` am Canvas allein genügt NICHT: Er ist mit `expand=True`
        # gepackt und wird von seinem Inhalt gedehnt — gemessen 265 px bei
        # gesetzten 150, weil der Inhalt 300 px hoch war. Erst wenn der
        # umgebende Rahmen seine Größe nicht mehr vom Inhalt nimmt, hält die
        # Grenze.
        aussen.pack_propagate(False)
        aussen.configure(height=height)
    leinwand = tk.Canvas(aussen, bg=BG, highlightthickness=0,
                         **({'height': height} if height else {}))
    from .main_window import round_scrollbar
    balken = round_scrollbar(aussen, leinwand, bg=BG)
    innen = tk.Frame(leinwand, bg=BG)
    innen.bind('<Configure>',
               lambda e: leinwand.configure(scrollregion=leinwand.bbox('all')))
    fenster_id = leinwand.create_window((0, 0), window=innen, anchor='nw')
    leinwand.bind('<Configure>',
                  lambda e: leinwand.itemconfigure(fenster_id, width=e.width))
    leinwand.configure(yscrollcommand=balken.set)
    balken.pack(side='right', fill='y')
    leinwand.pack(side='left', fill='both', expand=True)

    if inset:
        polster = tk.Frame(innen, bg=BG)
        polster.pack(fill='both', expand=True, padx=inset)
        innen_ziel = polster
    else:
        innen_ziel = innen

    from .main_window import bind_wheel
    bind_wheel(leinwand)
    # ⭐ Die Leinwand mitgeben. Seiten, die ihre Liste neu zeichnen (Lager,
    # Handelslager), brauchen sie, um die Rollposition zu halten — siehe
    # `_rollstelle_halten`.
    innen_ziel.canvas = leinwand
    return innen_ziel


# Wie viele Zeilen sofort dastehen, und wie viele beim Rollen dazukommen.
# ⚠ Der erste Wert muss die **sichtbare** Fläche sicher füllen — sonst sieht
# man unten Leerraum und hält die Liste für zu Ende. 45 Zeilen decken auch
# ein hohes Fenster ab.
ROWS_FIRST = 45
ROWS_MORE = 45


def _pack_on_demand(leinwand, zeilen, sofort=ROWS_FIRST,
                        schritt=ROWS_MORE):
    """Nur die sichtbaren Zeilen packen, den Rest beim Rollen nachlegen.

    ⚠⚠ **Warum das etwas bringt, obwohl die Zeilen längst gebaut sind:**
    Tk rechnet die Geometrie für **jedes gepackte** Kind, auch für die, die
    weit unter dem Fensterrand liegen. Bei 200 Zeilen ist das die eigentliche
    Wartezeit beim Seitenwechsel — nicht das Bauen.

    Gemessen am 13.09.2026 auf der Joystick-Seite, warmer Wechsel:

    | | |
    |---|---|
    | alle 200 Zeilen gepackt | **635 ms** |
    | nur 30 gepackt | **192 ms** |

    ⚠ Und die Reihenfolge bleibt, weil **nur nach hinten** angehängt wird.
    Ein `pack()` nach `pack_forget()` würde die Zeile ans Ende setzen — mit
    Herausnehmen und Wiedereinsetzen wäre die Liste nach dem ersten Rollen
    durcheinander.

    ⚠ Die Zeilen sind **gebaut**, nur nicht gepackt. Sie sind damit sofort
    da, wenn gerollt wird — kein Nachladen, kein Flackern.
    """
    stand = {'gepackt': 0}

    def nachlegen():
        if stand['gepackt'] >= len(zeilen):
            return
        bis = min(len(zeilen), stand['gepackt'] + schritt)
        for zeile in zeilen[stand['gepackt']:bis]:
            try:
                zeile.pack(fill='x', pady=1)
            except tk.TclError:
                return            # Liste wurde inzwischen neu gezeichnet
        stand['gepackt'] = bis

    nachlegen()
    if len(zeilen) <= sofort:
        return

    # ⭐ An `yscrollcommand` andocken statt an Mausrad und Rollbalken einzeln:
    # Tk ruft es bei **jeder** Sichtänderung, egal wodurch sie entstand.
    # ⚠ `cget` liefert einen Tcl-Befehlsnamen, keine Python-Funktion — der
    # bisherige Empfänger (der Rollbalken) wird deshalb über `tk.call`
    # weiterbedient. Ohne das bliebe der Balken stehen.
    vorher = leinwand.cget('yscrollcommand')

    def beim_rollen(*werte):
        if vorher:
            try:
                leinwand.tk.call(vorher, *werte)
            except tk.TclError:
                pass
        try:
            # Nahe am Ende? Dann die nächste Portion anhängen.
            if float(werte[1]) > 0.85:
                nachlegen()
        except (IndexError, TypeError, ValueError):
            pass

    leinwand.configure(yscrollcommand=beim_rollen)


def _build_on_demand(leinwand, anzahl, bauer, sofort=ROWS_FIRST,
                       schritt=ROWS_MORE):
    """Nur die sichtbaren Einträge **bauen**, den Rest beim Rollen nachlegen.

    ⚠⚠ **Unterschied zu `_pack_on_demand`:** Dort sind die Zeilen längst
    gebaut und es geht nur ums Layout. Hier werden sie gar nicht erst
    erzeugt — weil nicht das Anzeigen teuer ist, sondern das **Wegwerfen**
    beim nächsten Tastendruck.

    Gemessen am 13.09.2026 auf der Seite „Zerlegen": Von 1,24 s je Anzeigen
    gingen **1,03 s** allein für `destroy()` von 1600 Bauteilen drauf. Die
    Auswahlliste baut für jedes der rund 400 Teile eine Zeile aus bis zu drei
    Bauteilen — und wirft alles weg, sobald jemand einen Buchstaben tippt.

    `bauer(i)` erzeugt den Eintrag mit der Nummer `i` und packt ihn selbst.
    """
    stand = {'gebaut': 0}

    def nachlegen():
        if stand['gebaut'] >= anzahl:
            return
        bis = min(anzahl, stand['gebaut'] + schritt)
        for i in range(stand['gebaut'], bis):
            try:
                bauer(i)
            except tk.TclError:
                return            # Liste wurde inzwischen neu gezeichnet
        stand['gebaut'] = bis

    nachlegen()
    if anzahl <= sofort or leinwand is None:
        return

    vorher = leinwand.cget('yscrollcommand')

    def beim_rollen(*werte):
        if vorher:
            try:
                leinwand.tk.call(vorher, *werte)
            except tk.TclError:
                pass
        try:
            if float(werte[1]) > 0.85:
                nachlegen()
        except (IndexError, TypeError, ValueError):
            pass

    leinwand.configure(yscrollcommand=beim_rollen)


def _scroll_to_top(widget):
    """Die Rollfläche wieder an den Anfang setzen.

    ⚠⚠ **Nicht dasselbe wie `_rollstelle_halten`.** Der Helfer dort merkt sich
    den **Anteil** und stellt ihn wieder her — richtig, solange der Inhalt
    ungefähr gleich lang bleibt. Schrumpft er stark (eine aufgeklappte
    Vorschlagsliste verschwindet), führt derselbe Anteil hinter das Ende: Oben
    steht dann eine leere Fläche, und der Inhalt fehlt scheinbar.

    Am 04.09.2026 gemeldet: „Wieso ist da so viel leerer Raum … verschenkter
    Platz." Genau dieser Fall.
    """
    # ⛔⛔ Das Attribut heisst `canvas`, nicht `leinwand` — `_scroll_area`
    # setzt es so (`innen_ziel.canvas = leinwand`). Bis zum 14.09.2026
    # stand hier der alte deutsche Name, ein Rueckstand der
    # Sprachumstellung. `getattr(..., None)` liefert dann brav `None`,
    # und die Funktion steigt **stillschweigend** aus: kein Fehler,
    # keine Meldung, nur eine Seite, die beim Neuzeichnen nach oben
    # springt.
    leinwand = None
    lauf = widget
    while lauf is not None and leinwand is None:
        # ⚠ **BEIDE Namen.** Die Seiten setzen `canvas`
        # (`_scroll_area`), die beiden eigenstaendigen Fenster
        # `bestandsfenster.py` und `einstellungsfenster.py` weiterhin
        # `leinwand`. Wer nur einen sucht, legt die Haelfte still —
        # und zwar lautlos, weil `getattr(..., None)` brav `None`
        # liefert und die Funktion einfach aussteigt.
        leinwand = (getattr(lauf, 'canvas', None)
                    or getattr(lauf, 'leinwand', None))
        lauf = getattr(lauf, 'master', None)
    if leinwand is None:
        return

    def hoch():
        try:
            if leinwand.winfo_exists():
                leinwand.yview_moveto(0.0)
        except Exception:
            pass

    try:
        leinwand.after_idle(hoch)
    except Exception:
        pass


def _keep_scroll(widget, action):
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
    # ⛔⛔ Das Attribut heisst `canvas`, nicht `leinwand` — `_scroll_area`
    # setzt es so (`innen_ziel.canvas = leinwand`). Bis zum 14.09.2026
    # stand hier der alte deutsche Name, ein Rueckstand der
    # Sprachumstellung. `getattr(..., None)` liefert dann brav `None`,
    # und die Funktion steigt **stillschweigend** aus: kein Fehler,
    # keine Meldung, nur eine Seite, die beim Neuzeichnen nach oben
    # springt.
    leinwand = None
    lauf = widget
    while lauf is not None and leinwand is None:
        # ⚠ **BEIDE Namen.** Die Seiten setzen `canvas`
        # (`_scroll_area`), die beiden eigenstaendigen Fenster
        # `bestandsfenster.py` und `einstellungsfenster.py` weiterhin
        # `leinwand`. Wer nur einen sucht, legt die Haelfte still —
        # und zwar lautlos, weil `getattr(..., None)` brav `None`
        # liefert und die Funktion einfach aussteigt.
        leinwand = (getattr(lauf, 'canvas', None)
                    or getattr(lauf, 'leinwand', None))
        lauf = getattr(lauf, 'master', None)
    if leinwand is None:
        action()
        return
    try:
        stelle = leinwand.yview()[0]
    except Exception:
        stelle = None
    action()
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


def _search_clear(window, holder, var):
    """Ein × neben dem Suchfeld, das den Text wegnimmt.

    ⚠ Es erscheint nur, wenn wirklich etwas im Feld steht. Ein Kreuz an einem
    leeren Feld sieht aus wie ein Knopf, der nichts tut.

    Warum es das braucht: Wer „titan" gesucht hat und danach die ganze Liste
    sehen will, musste den Text von Hand markieren und löschen — und wer den
    Suchbegriff übersieht, hält die kurze Liste für den ganzen Bestand.
    """
    from . import notice
    kreuz = tk.Label(holder, text='\u00d7', bg=BG, fg=SUB,
                     font=window.f_base, cursor='hand2')
    notice.attach(kreuz, lambda: t('s_suche_leeren'))
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


def _filter_bar(window, parent, fields, on_change, state):
    """Eine Reihe Auswahlfelder plus „Auswahl zurücksetzen" — für jede Seite gleich.

    ⚠⚠ **Ein Bedienkonzept für das ganze Programm.** Xharig am 29.08.2026:
    *„egal wo, sollte das Bedienkonzept nicht jedes Mal ändern — die Leute
    wollen es nutzen und nicht erst lernen, wie sie es nutzen."* Wer die
    Bauplan-Liste bedienen kann, muss Herstellung und Bergbau ohne Umlernen
    bedienen können. Deshalb dasselbe `round_select` wie dort, derselbe
    Zurücksetzen-Knopf an derselben Stelle.

    `felder` ist eine Liste aus `(schluessel, beschriftung, eintraege)`:
    `eintraege` sind Paare `(wert, text)`; ein leerer Wert ist der „alle"-Fall.
    **Ein Feld ohne echte Auswahl wird weggelassen** — ein Auswahlfeld, das nur
    „alle" anbietet, ist Ballast und lässt einen suchen, was es filtern soll.

    `zustand` ist ein Wörterbuch, in dem die Wahl landet; `beim_wechsel` wird
    nach jeder Änderung gerufen.

    Gibt eine Funktion zurück, die alles zurücksetzt.
    """
    from .main_window import round_select
    reihe = tk.Frame(parent, bg=BG)
    reihe.pack(fill='x', pady=(0, 8))
    links = tk.Frame(reihe, bg=BG)
    links.pack(side='left', fill='x', expand=True)

    gebaut = {}
    reihenfolge = []
    for schluessel, beschriftung, eintraege in fields:
        if len(eintraege) <= 1:
            continue
        w = round_select(links, [('', beschriftung)] + list(eintraege),
                     state.get(schluessel, ''),
                     lambda wert, s=schluessel: (state.__setitem__(s, wert),
                                                 on_change()),
                     window.f_small)
        gebaut[schluessel] = w
        reihenfolge.append(w)
    # ⚠⚠ **Umbrechend, nicht abgeschnitten.** Tk schneidet eine zu breite
    # Reihe wortlos rechts ab — bei fünf Menüs im Laden-Reiter stand dort
    # „Alle Gü…", und das fünfte war nicht mehr bedienbar. Am 05.09.2026
    # gemeldet: „Kein Umbruch bei den Dropdowns."
    #
    # Derselbe Helfer wie bei den Geräteknöpfen der Steuerung. Er wirkt hier
    # auf **alle** Seiten mit Filterleiste — die Bauplan-Liste hat sechs
    # Felder und lief in dieselbe Grenze.
    if reihenfolge:
        _button_grid(links, reihenfolge, gap=8)

    def zuruecksetzen():
        for schluessel, w in gebaut.items():
            state[schluessel] = ''
            try:
                w.select_quiet('')
            except Exception:
                pass
        on_change()

    return zuruecksetzen, reihe, gebaut


def _ensure_size(c, beschriftung, flaeche, hoehe, fuellung, rand):
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
    from .main_window import _round_rect

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
        flaeche[0] = _round_rect(c, 1, 1, noetig - 1, hoehe - 1, radius=5,
                                      fill=fuellung, outline=rand, width=1)
        c.tag_lower(flaeche[0], beschriftung)

    nachmessen()
    c.bind('<Configure>', nachmessen, add='+')
    c.after_idle(nachmessen)
    return nachmessen


def _button(window, parent, text, action, strong=False, danger=False):
    """Ein Knopf im Stil der Vorschau — Rand, Farbe beim Überfahren."""
    from .main_window import _round_rect
    schrift = window.f_small
    hoehe = schrift.metrics('linespace') + 16
    breite = schrift.measure(text) + 30
    # ⚠ `gefahr` faerbt **dauerhaft**, nicht erst beim Überfahren. Ein Knopf,
    # der erst rot wird, wenn die Maus schon darauf steht, warnt niemanden —
    # gesehen hat man ihn dann längst. am 28.08.2026 gemeldet zum
    # Absende-Knopf: „der Button wird erst beim Überfahren rot."
    farbe = RED if danger else (ACCENT if strong else FG)
    rand = RED if danger else (ACCENT if strong else LINE)
    c = tk.Canvas(parent, width=breite, height=hoehe, bg=BG,
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
    fuellung = ('#2a1414' if danger
                else ('#1d2a14' if strong else SURFACE))
    flaeche = [_round_rect(c, 1, 1, breite - 1, hoehe - 1, radius=5,
                                fill=fuellung, outline=rand, width=1)]
    c.tag_lower(flaeche[0], beschriftung)

    _ensure_size(c, beschriftung, flaeche, hoehe, fuellung, rand)

    def rein(_=None):
        c.itemconfigure(flaeche[0], outline=RED if danger else ACCENT)
        c.itemconfigure(beschriftung, fill=RED if danger else ACCENT)

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
        punkte = _round_rect(c, 1, 1, neue_breite - 1, hoehe - 1,
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
    c.bind('<Button-1>', lambda e: action())
    c.beschriften = beschriften
    c.is_button = True          # damit tools/randpruefung.py ihn prüft
    return c


def _choice(window, parent, entries, active, action):
    """Mehrere Möglichkeiten nebeneinander — die gewählte trägt den Akzentrand."""
    from .main_window import _round_rect
    reihe = tk.Frame(parent, bg=BG)
    knoepfe = {}
    schrift = window.f_small
    for kennung, text in entries:
        an = (kennung == active)
        hoehe = schrift.metrics('linespace') + 14
        breite = schrift.measure(text) + 26
        c = tk.Canvas(reihe, width=breite, height=hoehe, bg=BG,
                      highlightthickness=0, bd=0, cursor='hand2')
        c.pack(side='left', padx=(0, 6))
        flaeche = [_round_rect(c, 1, 1, breite - 1, hoehe - 1, radius=5,
                                    fill=SURFACE, outline=ACCENT if an else LINE,
                                    width=1)]
        beschr = c.create_text(breite / 2.0, hoehe / 2.0, text=text,
                               fill=ACCENT if an else SUB, font=schrift)
        # Dieselbe Falle wie beim gewoehnlichen Knopf — siehe `_nachmessen`.
        _ensure_size(c, beschr, flaeche, hoehe, SURFACE,
                      ACCENT if an else LINE)
        c.teile = (flaeche, beschr)
        c.bind('<Button-1>', lambda e, k=kennung: action(k))
        c.is_button = True      # damit tools/randpruefung.py ihn prüft
        knoepfe[kennung] = c

    def setzen(gewaehlt):
        for kennung, c in knoepfe.items():
            an = (kennung == gewaehlt)
            flaeche, beschr = c.teile
            c.itemconfigure(flaeche[0], outline=ACCENT if an else LINE)
            c.itemconfigure(beschr, fill=ACCENT if an else SUB)

    reihe.select = setzen
    return reihe


def _status(window, parent, symbol, bold, rest, color=None):
    """Ein Statuskasten mit farbigem Balken links — wie in der Vorschau.

    ⚠ `symbol` ist ein Name aus `scbp/icons.py` („haken", „offen"), kein
    Schriftzeichen mehr. Der Parameter hieß bis v3.0.0-rc55 `zeichen` und hätte
    das gleichnamige Modul verdeckt.
    """
    color = color or ACCENT
    innen = _card(parent, border_color=color, pady=(0, 14))
    zeile = tk.Frame(innen, bg=SURFACE)
    zeile.pack(fill='x', padx=14, pady=12)
    icons.line(zeile, symbol, background=SURFACE, font=window.f_base,
                  color=icons.GREY if color == SUB else icons.GREEN
                  ).pack(side='left', padx=(0, 10), anchor='n')
    text = tk.Frame(zeile, bg=SURFACE)
    text.pack(side='left', fill='x', expand=True)
    oben = tk.Label(text, text=bold, bg=SURFACE, fg=FG, font=window.f_bold,
                    anchor='w', justify='left')
    oben.pack(fill='x')
    _wrap(oben)
    if rest:
        unten = tk.Label(text, text=rest, bg=SURFACE, fg=SUB,
                         font=window.f_small, anchor='w', justify='left')
        unten.pack(fill='x')
        _wrap(unten)
    return innen


def _path_field(fenster, eltern, wert, waehlen, oeffnen=None, platzhalter=''):
    """Ein Pfad mit Knopf daneben."""
    reihe = tk.Frame(eltern, bg=BG)
    reihe.pack(fill='x', pady=(8, 0))
    from .main_window import round_entry
    feld = round_entry(reihe, wert, fenster.f_small, '#0c1017', LINE, ACCENT, FG)
    feld.holder.pack(side='left', fill='x', expand=True, padx=(0, 8))
    if platzhalter and not wert.get():
        feld.configure(fg=SUB)
    _button(fenster, reihe, t('s_durchsuchen'), waehlen).pack(side='left')
    if oeffnen:
        _button(fenster, reihe, t('s_oeffnen'), oeffnen).pack(side='left', padx=(8, 0))
    return reihe



def _pixels(widget, wert, ersatz=0):
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


def _wrap_self(label):
    """Umbrechen auf die Breite, die das Label **selbst** bekommen hat.

    ⭐ Der Fall, für den `_wrap()` nicht taugt: ein Label, das mit
    `fill='x', expand=True` in einer Zeile sitzt, in der rechts noch mehrere
    Dinge stehen. Der Elternrahmen ist dann deutlich breiter als das, was das
    Label wirklich abbekommt — `_wrap()` würde zu großzügig rechnen, und Tk
    schneidet weiter ab. `beside=` hilft nur bei **einem** Nachbarn.

    Hier wird gar nicht gerechnet: Das Label fragt nach jedem `<Configure>`
    seine eigene Breite ab. Die kennt es genau.

    ⚠ **Nur setzen, wenn sich der Wert ändert.** Ein neues `wraplength` ändert
    die Höhe, das löst wieder ein `<Configure>` aus — ohne diese Bremse dreht
    sich das im Kreis.

    > **Woher:** 14.09.2026. In der Belegungsliste stand ausdrücklich, das
    > Abschneiden sei Absicht („der Name steht immerhin am Anfang lesbar da").
    > Bei „sehr groß" brauchte ein Aktionsname 446 px und bekam 296 — ein
    > Drittel fehlte. Und ausgerechnet wer diese Stufe wählt, tut das, **weil**
    > er lesen können will.
    """
    def nachziehen(_=None):
        try:
            if not label.winfo_exists():
                return
            breite = label.winfo_width()
            if breite <= 40:
                return
            neu = breite - 4
            if int(label.cget('wraplength') or 0) != neu:
                label.configure(wraplength=neu, justify='left')
        except tk.TclError:
            pass

    label.bind('<Configure>', nachziehen, add='+')
    label.after(0, nachziehen)
    return label


def _wrap(label, share=1.0, inset=0, reference=None, beside=None):
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
    ziel = reference if reference is not None else label.master

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
        if beside is not None:
            try:
                breite -= beside.winfo_reqwidth()
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
                rand = 2 * (_pixels(label, label.cget('borderwidth'))
                            + _pixels(label, label.cget('padx'))
                            + _pixels(label, label.cget('highlightthickness')))
            except tk.TclError:
                rand = 4
            try:
                label.configure(wraplength=max(160, int(breite * share)
                                               - inset - rand))
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


def _button_row(parent, buttons, gap=8):
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
            if not parent.winfo_exists():
                return
            if not all(k.winfo_exists() for k in buttons):
                return
        except tk.TclError:
            return
        platz = parent.winfo_width()
        gebraucht = sum(k.winfo_reqwidth() for k in buttons) \
            + gap * (len(buttons) - 1)

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
            oben = parent.winfo_toplevel()
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
                    # in seiner eigenen Größe fest, obwohl `MIN_HEIGHT` 380 ist.
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
        if nebeneinander == getattr(parent, 'zuletzt_nebeneinander', None):
            return
        parent.zuletzt_nebeneinander = nebeneinander
        for nummer, knopf in enumerate(buttons):
            knopf.pack_forget()
            if nebeneinander:
                knopf.pack(side='left', padx=(0 if nummer == 0 else gap, 0))
            else:
                knopf.pack(side='top', anchor='w',
                           pady=(0 if nummer == 0 else 6, 0))

    parent.bind('<Configure>', ordnen, add='+')
    parent.after(0, ordnen)
    return parent


def _reflow_grid(eltern):
    """Die Knöpfe eines `_knopfgitter` auf so viele Zeilen verteilen wie nötig."""
    zustand = getattr(eltern, '_gitter', None)
    if zustand is None:
        return
    knoepfe = zustand['knoepfe']
    try:
        if not eltern.winfo_exists() or \
                not all(k.winfo_exists() for k in knoepfe):
            return
    except tk.TclError:
        return
    platz = eltern.winfo_width()

    # ⚠⚠ **Bei unbekannter Breite NICHT einfach aussteigen** (07.09.2026).
    # Das war ein Teufelskreis: Ohne Breite wurden die Knöpfe nirgends
    # platziert, ohne platzierte Kinder blieb der Rahmen 1 px breit, und ohne
    # Breitenänderung feuerte nie ein `<Configure>`, das es hätte richten
    # können. Neun Bereichsknöpfe blieben so dauerhaft unsichtbar.
    #
    # Vorher fiel das nicht auf, weil dieser Rahmen in einer Rollfläche lag und
    # deren Breite sofort feststand. Seit die Bereichsauswahl fest über der
    # Rollfläche sitzt, ist sie beim ersten Ordnen noch nicht vermessen.
    #
    # Also: einspaltig setzen — damit der Rahmen überhaupt eine Größe bekommt —
    # und gleich noch einmal nachfassen, wenn die Breite steht.
    if platz <= 1:
        for nummer, knopf in enumerate(knoepfe):
            knopf.grid(row=nummer, column=0, sticky='w', pady=(0, 4))
        eltern.after(50, lambda: _reflow_grid(eltern))
        return
    if platz == zustand['breite']:
        return
    zustand['breite'] = platz
    abstand = zustand['abstand']

    # ⚠⚠ **Alle Spalten gleich breit — sonst rechnet der Umbruch am Layout
    # vorbei** (07.09.2026, gemeldet: „ab mounts ist es auch abgeschnitten bei
    # den auswahlen").
    #
    # Vorher addierte diese Schleife die Breiten der Knöpfe **einer Zeile** und
    # brach um, wenn die Summe den Platz überschritt. `grid` richtet aber nach
    # der breitesten Zelle **je Spalte** aus, über alle Zeilen hinweg: Steht in
    # Spalte 4 irgendeiner Zeile ein breiter Knopf („mininglasers (13)",
    # 131 px), wird Spalte 4 in JEDER Zeile so breit — und die tatsächlichen
    # Positionen laufen der Rechnung davon.
    #
    # Gemessen bei 892 px Rahmenbreite und 16 Bereichen: „missileracks (5)"
    # saß bei x=864 und war 120 px breit, ragte also 92 px hinaus. Tk schneidet
    # das wortlos ab — ohne Rollbalken, ohne Hinweis. Genau die Falle, gegen
    # die `_knopfgitter` einmal gebaut wurde.
    #
    # Die Lösung ist nicht, genauer zu rechnen, sondern dem Layout die Freiheit
    # zu nehmen: Jede Spalte bekommt dieselbe Breite (die des breitesten
    # Knopfes). Dann ist die Spaltenzahl eine simple Division, und was gerechnet
    # wurde, steht auch so da. Es kostet etwas Leerraum hinter den kurzen
    # Beschriftungen — dafür ist nichts mehr abgeschnitten, und die Knöpfe
    # stehen sauber untereinander statt in ausgefransten Zeilen.
    breiteste = max((k.winfo_reqwidth() for k in knoepfe), default=0)
    spaltenbreite = breiteste + abstand
    spalten = max(1, platz // spaltenbreite) if spaltenbreite else 1

    for nummer, knopf in enumerate(knoepfe):
        knopf.grid(row=nummer // spalten, column=nummer % spalten,
                   sticky='w', padx=(0, abstand), pady=(0, 4))
    # ⚠ `uniform` ist der Teil, der die Gleichbreite wirklich durchsetzt —
    # `minsize` allein ließe eine Spalte weiter wachsen.
    for spalte in range(spalten):
        eltern.grid_columnconfigure(spalte, uniform='knopf',
                                    minsize=spaltenbreite)
    # ⚠⚠ **Spalten von früher wieder freigeben.** `grid_columnconfigure` bleibt
    # stehen, auch wenn in der Spalte nichts mehr steht: Eine Spalte mit
    # `minsize` fordert ihren Platz weiter an. Beim ersten Ordnen liegen alle
    # Knöpfe einspaltig, bei einem breiten Fenster in acht Spalten — wird das
    # Fenster danach schmaler, blieben die alten acht `minsize` erhalten.
    #
    # Gemessen am 14.09.2026 auf der Joystick-Seite: Der Rahmen forderte
    # **1344 px** an, obwohl fünf Spalten à 168 px nur 840 brauchen. Sichtbar
    # abgeschnitten war nichts — die Knöpfe saßen alle richtig —, aber die
    # überhöhte Wunschbreite reicht nach oben durch, und die Randprüfung
    # meldete dreimal einen Überstand, den es gar nicht gab.
    spalte = spalten
    while eltern.grid_columnconfigure(spalte).get('minsize'):
        eltern.grid_columnconfigure(spalte, uniform='', minsize=0)
        spalte += 1


def _button_grid(parent, buttons, gap=6):
    """Knöpfe nebeneinander — und in einer zweiten Zeile, wenn es eng wird.

    ⚠⚠ **Nicht dasselbe wie `_knopfreihe`.** Die kennt nur zwei Zustände,
    alle nebeneinander oder alle untereinander, und fordert vorher Platz beim
    Fenster an. Für vier Knöpfe ist das richtig; für sieben Geräteknöpfe wäre
    „alle untereinander" eine Wand, und Platz anzufordern geht an der Sache
    vorbei, wenn jemand sein Fenster **absichtlich** klein zieht.

    Am 05.09.2026 gemeldet: „Werkseinstellung zurücksetzen wird abgeschnitten"
    und „Maus, Tastatur und Gamepad werden auch abgeschnitten beim
    Kleinerziehen". Tk schneidet eine zu breite Reihe wortlos ab — was rechts
    nicht mehr hinpasst, ist einfach weg, ohne Rollbalken und ohne Hinweis.

    ⚠ Der Rahmen gehört **allein** dem Gitter: `grid` und `pack` vertragen
    sich im selben Behälter nicht.
    """
    zustand = getattr(parent, '_gitter', None)
    if zustand is None:
        parent._gitter = {'knoepfe': list(buttons), 'breite': 0,
                          'abstand': gap}
        # ⚠ Nur EINMAL binden. Diese Reihen werden neu bestückt, sobald sich
        # ein Gerät ändert — bei jedem Mal neu zu binden häufte die Rückrufe
        # an, bis dasselbe Ordnen zwanzigfach liefe.
        parent.bind('<Configure>', lambda _=None: _reflow_grid(parent),
                    add='+')
    else:
        zustand['knoepfe'] = list(buttons)
        zustand['breite'] = 0

    # ⚠⚠ **Die Höhe MUSS hier schon stehen, nicht erst in `_reflow_grid`.**
    # Seit die Knöpfe per `place` sitzen, hat der Rahmen keine Kinder mehr, aus
    # denen er seine Größe ableiten könnte — er bliebe 1 px hoch. Und
    # `_reflow_grid` steigt aus, solange die Breite noch nicht steht
    # (`platz <= 1`), setzt die Höhe also unter Umständen nie. Ergebnis:
    # neun Bereichsknöpfe, alle unsichtbar.
    #
    # Deshalb hier eine Höhe aus der Wunschgröße der Knöpfe, bevor überhaupt
    # etwas gemessen wird. `_reflow_grid` korrigiert sie später auf die
    # tatsächliche Zeilenzahl.
    parent.after(0, lambda: _reflow_grid(parent))
    return parent


def _body_text(parent, text, font, color=SUB, bg=BG, inset=0, **pack):
    """Ein Absatz, der mit dem Fenster mitgeht.

    Der Regelweg für jeden mehrzeiligen Text. Wer stattdessen `wraplength=600`
    schreibt, baut den Fehler wieder ein, den diese Funktion behebt: Der Wert
    passt für die eine Fenstergröße, bei der er entstanden ist.

    `abzug` ist der waagerechte Rand, den der Text nicht benutzen darf —
    üblicherweise das Doppelte des `padx` beim Packen.
    """
    # ⚠ Der Regelweg für Absätze ist auch der Regelweg für die Auszeichnung:
    # Wer hier einen Text mit `**fett**` hineingibt, soll ihn nicht mit
    # Sternchen auf dem Bildschirm wiederfinden. Doppelt entschärfen schadet
    # nicht — `_ohne_marken` auf einem sauberen Text ändert nichts.
    label = tk.Label(parent, text=_strip_markup(text), bg=bg, fg=color,
                     font=font, anchor='w', justify='left')
    label.pack(**pack)
    return _wrap(label, inset=inset)


def _strip_markup(text):
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


def _setting_row(window, parent, caption, help_text, wide=False, top=False):
    """Eine Einstellungszeile: Bezeichnung, Erklärung, Platz für das Bedienelement.

    `oben=True` heftet die Beschriftung an die **Oberkante** statt sie
    mittig zu setzen. ⚠ Gebraucht bei Feldern, die im Betrieb wachsen: Klappt
    ein Auswahlfeld seine Liste auf, wird die Zeile plötzlich zehn Zeilen hoch
    — und die Beschriftung stand dann auf halber Höhe irgendwo neben der Liste
    statt neben ihrem Feld.
    """
    zeile = tk.Frame(parent, bg=BG)
    zeile.pack(fill='x', pady=(12, 0))
    links = tk.Frame(zeile, bg=BG)
    links.pack(side='left', fill='x', expand=True,
               **({'anchor': 'n'} if top else {}))
    beschriftung = tk.Label(links, text=caption, bg=BG, fg=FG,
                            font=window.f_bold, anchor='w')
    beschriftung.pack(fill='x')
    erklaerung = None
    if help_text:
        erklaerung = tk.Label(links, text=_strip_markup(help_text), bg=BG, fg=SUB,
                              font=window.f_small, anchor='w', justify='left')
        erklaerung.pack(fill='x')
    if wide:
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
            _wrap(erklaerung, reference=zeile, inset=10)
        _wrap(beschriftung, reference=zeile, inset=10)
    else:
        rechts = tk.Frame(zeile, bg=BG)
        rechts.pack(side='right', padx=(16, 0),
                    **({'anchor': 'n'} if top else {}))
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
            _wrap(erklaerung, reference=zeile, beside=rechts, inset=26)
        _wrap(beschriftung, reference=zeile, beside=rechts, inset=26)
    tk.Frame(parent, bg=LINE, height=1).pack(fill='x', pady=(12, 0))
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
    rechts.caption = beschriftung
    return rechts


# --------------------------------------------------------------------- Seiten
def _blueprint_list(fenster, rahmen):
    """Die Bauplan-Liste — das vorhandene Fenster, eingebettet."""
    from . import bestandsfenster
    # ⭐ Rückweg zum Hauptfenster — die Liste braucht ihn, um auf andere Seiten
    # zu springen (bisher ging der Weg nur andersherum, über `stock_page`).
    #
    # ⚠⚠ **Als Argument, nicht danach zugewiesen.** Der Konstruktor zeichnet
    # die Liste bereits; wer den Rückweg erst hinterher setzt, hat beim ersten
    # Zeichnen keinen — und dann ist kein Name anklickbar, bis zufällig neu
    # gezeichnet wird. Genau das war der Fehler in v3.26.0 bis rc3.
    fenster.stock_page = bestandsfenster.Bestandsfenster(rahmen=rahmen,
                                                            hauptfenster=fenster)

    # ⚠ Beim erneuten Aufrufen ohne Filter anfangen. Die Seite wird nur ein-
    # und ausgeblendet, sonst stünde die Auswahl von vorhin noch da — und wer
    # „Andockkragen, Größe 2, Grad A" vergessen hat, sieht „Nichts gefunden"
    # und hält den Bestand für leer. Am 29.08.2026 gemeldet.
    def _frisch():
        seite = getattr(fenster, 'stock_page', None)
        if seite is None:
            return
        seite._fein_leeren()
        seite._suche_leeren()
        # ⚠⚠ **Und die Daten selbst.** Filter zu leeren nützt nichts, wenn
        # darunter der Bestand von vorhin liegt: Ein Bauplan, der seit dem
        # ersten Öffnen dazukam, fehlte in der Anzahl und hatte keinen Haken.
        # Der Katalog kommt hier mit — ein Patch kann zwischendurch neue
        # Baupläne gebracht haben, und beim Seitenwechsel ist Zeit dafür.
        seite.neu_laden(auch_katalog=True)

    fenster.on_show['liste'] = _frisch


def _progress(fenster, rahmen):
    """Wie weit bin ich? — nach Bereichen gegliedert, jeder Bereich aufklappbar.

    ⚠ Vorher standen hier alle 25 Kategorien in einer einzigen langen Liste. Bei
    722 Bauplänen sucht man darin ewig, und der eine Wert, der einen gerade
    interessiert, steht irgendwo in der Mitte. Jetzt zuerst die vier Bereiche mit
    ihrem Gesamtstand — und die Einzelheiten erst auf Klick. Eingeklappt zu
    starten ist Absicht: Der Überblick ist die Antwort auf „wie weit bin ich",
    die Kategorien sind die Antwort auf „und wo genau".
    """
    _heading(fenster, rahmen, t('hf_fortschritt'), t('s_fo_lead'))
    innen = _scroll_area(rahmen)
    try:
        bestand = bestand_datei.load()
        katalog = katalog_modul.load()
    except Exception as ausnahme:
        fehler.merken('seiten.fortschritt', ausnahme)
        return

    # ⚠⚠⚠ **Sind es weniger Baupläne als je zuvor?** Dann steht das hier —
    # oben, wo die Zahl steht, über die man stolpert. Am 06.09.2026 zeigte
    # der Watcher nach einem Neustart 406 statt 413, weil die Zeiger-Datei
    # auf den Datenordner beim Aufräumen im Dateimanager mit weggeworfen
    # worden war. Er nahm still den Standardordner. Zurück blieb eine
    # kleinere Zahl und die Frage „wieso ändert sich immer wieder der
    # Ordner, die ganze Zeit hat es doch geklappt?"
    #
    # ⚠ `shrinkage_state()` und nicht `check_shrinkage()`: Letzteres würde beim
    # Hinsehen den kleineren Stand als neuen Höchstwert festschreiben, und die
    # Meldung wäre nach einmal Ansehen für immer weg.
    schwund = bestand_datei.shrinkage_state()
    if schwund:
        jetzt_da, hoechst, frueher = schwund
        kasten = tk.Frame(innen, bg=SURFACE, highlightthickness=1,
                          highlightbackground=GOLD)
        kasten.pack(fill='x', pady=(0, 10))
        tk.Label(kasten, text=t('s_schwund_titel'), bg=SURFACE, fg=GOLD,
                 font=fenster.f_bold).pack(anchor='w', padx=12, pady=(10, 2))
        tk.Label(kasten, text=t('s_schwund_text') % (jetzt_da, hoechst),
                 bg=SURFACE, fg=FG, font=fenster.f_small, justify='left',
                 wraplength=720).pack(anchor='w', padx=12)
        # ⚠ Beide Pfade im Klartext — die Frage ist ja gerade „welcher Ordner
        # denn nun". Ohne sie ist die Meldung eine Feststellung ohne Ausweg.
        for beschriftung, ort in ((t('s_schwund_wo'), frueher),
                                  (t('s_schwund_jetzt'), pfade.app_ordner())):
            if not ort:
                continue
            tk.Label(kasten, text=beschriftung, bg=SURFACE, fg=SUB,
                     font=fenster.f_small).pack(anchor='w', padx=12,
                                                pady=(6, 0))
            tk.Label(kasten, text=ort, bg=SURFACE, fg=ACCENT,
                     font=fenster.f_small, justify='left',
                     wraplength=720).pack(anchor='w', padx=24)
        tk.Label(kasten, text=t('s_schwund_tipp'), bg=SURFACE, fg=SUB,
                 font=fenster.f_small, justify='left',
                 wraplength=720).pack(anchor='w', padx=12, pady=(8, 10))

    bp = katalog.get('bauplaene') or {}
    habe = set(bestand.get('bauplaene') or {})
    # Je Bereich: Liste von (Kategorie, gesamt, meine)
    nach_bereich = {}
    for schluessel, e in bp.items():
        roh = katalog_modul.kind_id(e)
        bereich = katalog_modul.top_group(roh)
        art = katalog_modul.kind_readable(roh) if roh else '—'
        zaehler = nach_bereich.setdefault(bereich, {})
        gesamt, meine = zaehler.get(art, (0, 0))
        zaehler[art] = (gesamt + 1, meine + (1 if schluessel in habe else 0))

    gesamt_alle = sum(g for z in nach_bereich.values() for g, _ in z.values()) or 1
    meine_alle = sum(m for z in nach_bereich.values() for _, m in z.values())

    kopf = tk.Frame(innen, bg=BG)
    kopf.pack(fill='x', pady=(0, 4))
    tk.Label(kopf, text=str(meine_alle), bg=BG, fg=ACCENT,
             font=fenster.f_title).pack(side='left')
    tk.Label(kopf, text=t('s_fo_von')
             % (gesamt_alle, 100.0 * meine_alle / gesamt_alle),
             bg=BG, fg=SUB, font=fenster.f_small).pack(side='left')

    from .main_window import round_bar
    round_bar(innen, 9, meine_alle / float(gesamt_alle), BG, '#222b3b',
               ACCENT).pack(fill='x', pady=(6, 18))

    for bereich in katalog_modul.TOP_GROUPS:
        zaehler = nach_bereich.get(bereich)
        if not zaehler:
            continue
        gesamt = sum(g for g, _ in zaehler.values())
        meine = sum(m for _, m in zaehler.values())
        _progress_section(fenster, innen, t('gruppe_' + bereich), gesamt,
                             meine, zaehler)

    _best_contracts(fenster, innen, katalog, habe)


def _best_contracts(fenster, eltern, katalog, habe):
    """„Was bringt am meisten?" — die Aufträge mit den meisten fehlenden BPs.

    ⚠ Die Frage nach dem Fortschritt endet sonst bei „55 Prozent" und lässt
    einen damit allein. Hier steht, **was als Nächstes den größten Schritt
    macht**: ein einziger Auftrag bringt bis zu 44 fehlende Baupläne auf einmal.
    Gerechnet wird auf Daten, die ohnehin geladen sind — kein Netz, kein
    weiterer Datenweg.
    """
    try:
        lohnend = katalog_modul.worthwhile_contracts(katalog, habe)
    except Exception as ausnahme:
        fehler.merken('seiten.lohnende_auftraege', ausnahme)
        return

    # ⭐ Auf- und zuklappbar wie die Bereiche darüber — gewünscht am
    # 07.09.2026. Zehn Aufträge mit je vier Zeilen sind der mit Abstand
    # längste Block der Seite; wer nur wissen will, wie weit er ist, scrollt
    # sonst an ihm vorbei. **Zugeklappt zu starten ist dieselbe Entscheidung
    # wie bei den Bereichen:** Der Überblick ist die Antwort auf „wie weit bin
    # ich", die Aufträge sind die Antwort auf „und was mache ich als
    # Nächstes".
    #
    # ⚠ Kopfaufbau bewusst Zeichen für Zeichen wie in `_progress_section`:
    # Pfeil, Titel, Zahl — gleiche Dinge stehen an der gleichen Stelle.
    zustand = {'offen': False}
    kopf = tk.Frame(eltern, bg=BG, cursor='hand2')
    kopf.pack(fill='x', pady=(22, 2))
    pfeil = icons.line(kopf, 'aufklappen', background=BG,
                          font=fenster.f_small)
    pfeil.pack(side='left')
    tk.Label(kopf, text=t('s_fo_lohnt'), bg=BG, fg=FG,
             font=fenster.f_base, anchor='w').pack(side='left')
    if lohnend:
        tk.Label(kopf, text='  %d' % len(lohnend[:10]), bg=BG, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(side='left')

    koerper = tk.Frame(eltern, bg=BG)

    def umschalten(*_):
        zustand['offen'] = not zustand['offen']
        pfeil.swap_symbol('zuklappen' if zustand['offen']
                              else 'aufklappen')
        if zustand['offen']:
            # ⚠ `after=kopf`: nachträglich gepackt heißt sonst „ans Ende" —
            # dieselbe Falle, die die Kategorien hinter diese Liste geschoben
            # hat.
            koerper.pack(fill='x', after=kopf)
        else:
            koerper.pack_forget()

    for teil in [kopf] + list(kopf.winfo_children()):
        teil.bind('<Button-1>', umschalten)
        try:
            teil.configure(cursor='hand2')
        except tk.TclError:
            pass

    _body_text(koerper, t('s_fo_lohnt_hilfe'), fenster.f_small, fill='x')

    if not lohnend:
        _body_text(koerper, t('s_fo_lohnt_leer'), fenster.f_small,
                    fill='x', pady=(8, 0))
        return

    from .main_window import round_frame
    kasten = round_frame(koerper, SURFACE, LINE, radius=8, base_color=BG)
    kasten.holder.pack(fill='x', pady=(10, 0))
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
        zeile = tk.Frame(kasten, bg=SURFACE)
        zeile.pack(fill='x', padx=14, pady=3)
        tk.Label(zeile, text=str(anzahl), bg=SURFACE, fg=ACCENT,
                 font=fenster.f_base, width=3, anchor='e').pack(side='left')
        rechts = tk.Frame(zeile, bg=SURFACE)
        rechts.pack(side='left', fill='x', expand=True, padx=(10, 0))
        tk.Label(rechts, text=titel, bg=SURFACE, fg=FG, font=fenster.f_small,
                 anchor='w').pack(fill='x')
        teile = [fraktion] if fraktion else []
        if uec:
            teile.append('%s aUEC' % '{:,}'.format(uec).replace(',', '.'))
        if rang:
            teile.append(rang)
        teile.append(t('s_fo_lohnt_topf', anzahl))
        tk.Label(rechts, text=' · '.join(teile), bg=SURFACE, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(fill='x')
        # Der Annahmeort steht direkt da — er beantwortet „wo finde ich den".
        ort = ort_text(wo)
        beschriftungen = []
        if ort:
            ort_label = tk.Label(rechts, text=ort, bg=SURFACE, fg=SUB,
                                 font=fenster.f_small, anchor='w',
                                 justify='left')
            ort_label.pack(fill='x')
            beschriftungen.append(ort_label)
        tk.Label(rechts, text=t('s_fo_lohnt_klick'), bg=SURFACE, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(fill='x')

        # ⚠⚠ **Die Zahl ist keine Antwort, sie ist eine Frage.** „44" sagt
        # nicht, WELCHE 44 — und danach fragt man als Nächstes. Der Klick
        # führt deshalb in die Bauplan-Liste, gefiltert auf diesen Auftrag;
        # dort steht jeder einzelne, mit Haken für das, was man schon hat.
        # Die Liste kann das längst (`self.auftrag` als Filter), sie war von
        # hier aus nur nicht erreichbar: Man musste den Auftragsnamen von Hand
        # ins Suchfeld tippen und dann die Auftragszeile anklicken.
        def hinspringen(_ereignis=None, titel=titel):
            _to_contract(fenster, titel)

        for teil in [zeile, rechts] + beschriftungen:
            teil.config(cursor='hand2')
            teil.bind('<Button-1>', hinspringen)
        for kind in rechts.winfo_children():
            kind.config(cursor='hand2')
            kind.bind('<Button-1>', hinspringen)
    tk.Label(kasten, text='', bg=SURFACE).pack(pady=2)


def _progress_section(fenster, eltern, titel, gesamt, meine, kategorien):
    """Ein Bereich mit Gesamtbalken — die Kategorien darin klappen auf."""
    from .main_window import round_bar
    zustand = {'offen': False}

    kopf = tk.Frame(eltern, bg=BG, cursor='hand2')
    kopf.pack(fill='x', pady=(10, 2))
    pfeil = icons.line(kopf, 'aufklappen', background=BG,
                          font=fenster.f_small)
    pfeil.pack(side='left')
    tk.Label(kopf, text=titel, bg=BG, fg=FG, font=fenster.f_bold,
             anchor='w').pack(side='left')
    tk.Label(kopf, text='  %d / %d' % (meine, gesamt), bg=BG, fg=SUB,
             font=fenster.f_small, anchor='w').pack(side='left')

    anteil = max(0.0, min(1.0, meine / float(gesamt or 1)))
    balken = round_bar(eltern, 9, anteil, BG, '#222b3b', ACCENT)
    balken.pack(fill='x', pady=(2, 0))

    koerper = tk.Frame(eltern, bg=BG)

    def zeichnen():
        if koerper.winfo_children():
            return
        for art, (art_gesamt, art_meine) in sorted(kategorien.items(),
                                                   key=lambda x: -x[1][0]):
            zeile = tk.Frame(koerper, bg=BG)
            zeile.pack(fill='x', pady=3)
            beschriftung = tk.Label(zeile, text=art, bg=BG, fg=SUB,
                                    font=fenster.f_small, width=22,
                                    anchor='w')
            beschriftung.pack(side='left')
            teil = max(0.0, min(1.0, art_meine / float(art_gesamt or 1)))
            balken_zeile = round_bar(zeile, 7, teil, BG, '#222b3b', ACCENT,
                                      width=260)
            balken_zeile.pack(side='left', padx=8)
            zahl = tk.Label(zeile, text='%d / %d' % (art_meine, art_gesamt),
                            bg=BG, fg=SUB, font=fenster.f_small, width=10,
                            anchor='e')
            zahl.pack(side='right')

            # ⭐ **Die Zahl ist keine Antwort, sie ist eine Frage** — dieselbe
            # Überlegung wie bei „Was bringt am meisten?": „38 / 70" sagt
            # nicht, WELCHE 32 fehlen. Der Klick führt in die Bauplan-Liste,
            # gefiltert auf genau diese Kategorie. Gewünscht am 07.09.2026
            # („bei einem Klick auf Cooler, Schild, Radar wird man auf die
            # Baupläne geschickt und die Vorauswahl getroffen").
            def hinspringen(_ereignis=None, art=art):
                _to_kind(fenster, art)

            for teil_widget in (zeile, beschriftung, balken_zeile, zahl):
                teil_widget.bind('<Button-1>', hinspringen)
                try:
                    teil_widget.configure(cursor='hand2')
                except tk.TclError:
                    pass

    def umschalten(*_):
        zustand['offen'] = not zustand['offen']
        pfeil.swap_symbol('zuklappen' if zustand['offen']
                             else 'aufklappen')
        if zustand['offen']:
            zeichnen()
            # ⚠⚠ **`after=balken` ist Pflicht, nicht Kosmetik.** `pack()` ohne
            # Anker hängt ans ENDE der Elternfläche — und dort steht längst
            # alles, was nach den Bereichen kommt (die Liste „Was lohnt sich
            # am meisten", zehn Aufträge mit je vier Zeilen). Die Kategorien
            # eines Bereichs landeten dadurch ganz unten, hinter dieser Liste,
            # statt unter ihrem eigenen Balken: Man klappte oben etwas auf und
            # es erschien nichts — das Aufgeklappte lag mehrere
            # Bildschirmhöhen tiefer. Gemeldet am 07.09.2026 („die Infos da
            # ganz unten müssen doch oben hin").
            #
            # ⚠ Der Fehler tritt nur auf, weil `koerper` erst beim ERSTEN
            # Aufklappen gepackt wird. Wer so etwas nachträglich packt, muss
            # immer sagen, wohin — sonst entscheidet die Reihenfolge der
            # Klicks über das Layout.
            koerper.pack(fill='x', padx=(18, 0), pady=(6, 0), after=balken)
        else:
            koerper.pack_forget()

    # Der ganze Kopf ist die Schaltfläche, nicht nur der Pfeil.
    for teil in [kopf] + list(kopf.winfo_children()):
        teil.bind('<Button-1>', umschalten)
        try:
            teil.configure(cursor='hand2')
        except tk.TclError:
            pass


def _settings_parts(window):
    """Die Bausteine des Einstellungsfensters — einmal erzeugt, mehrfach genutzt."""
    if getattr(window, '_settings_window', None) is None:
        from . import einstellungsfenster
        leer = tk.Frame(window.root, bg=BG)     # nur als Halter, wird nie gepackt
        window._settings_window = einstellungsfenster.Einstellungsfenster(rahmen=leer)
        # Ohne diesen Rückruf öffnet ein Sprachwechsel ein zweites Fenster.
        window._settings_window.beim_sprachwechsel = window.rebuild
        # ⚠ Und ohne diesen laufen alle Rückmeldungen ins Leere: Eingebettet gibt
        # es den Fuß des Einstellungsfensters nicht, also auch sein Meldungs-Label
        # nicht. Jeder Klick auf „Jetzt auffrischen", „Übersetzung prüfen" oder eine
        # Textquelle brach deshalb mit `AttributeError` ab, **bevor** überhaupt
        # etwas passierte — die Seite sah fertig aus und tat nichts.
        window._settings_window.melder = window.say
    return window._settings_window


def _general(fenster, rahmen):
    from . import autostart, pfade
    from .main_window import toggle_switch
    _heading(fenster, rahmen, t('hf_allgemein'),
                  t('s_allg_lead'))
    innen = _scroll_area(rahmen)
    e = _settings_parts(fenster)

    ziel = _setting_row(fenster, innen, t('e_sprache'), t('s_sprache_h'),
                 wide=True)
    wahl = _choice(fenster, ziel,
                 [('auto', t('sprache_auto')), ('de', 'Deutsch'), ('en', 'English')],
                 pfade.einstellungen().get('sprache') or 'auto',
                 lambda k: (wahl.select(k), e._sprache_waehlen(k)))
    wahl.pack()

    ziel = _setting_row(fenster, innen, t('e_ton'),
                 t('s_ton_h'))

    def ton_um():
        neu_wert = not pfade.einstellung_wahrheit('signalton', True)
        pfade.einstellung_setzen('signalton', neu_wert)
        fenster.say('%s: %s' % (t('e_ton'), t('e_an') if neu_wert else t('e_aus')))
        return neu_wert

    toggle_switch(ziel, pfade.einstellung_wahrheit('signalton', True),
                    ton_um).pack()

    # ⚠ Standardmaessig AUS (Wunsch 05.09.2026). Gezaehlt wird trotzdem von
    # Anfang an — sonst begaenne die Zaehlung erst beim Einschalten, und die
    # Protokolle davor haette Star Citizen dann laengst weggeraeumt. Was nichts
    # kostet und sich nicht nachholen laesst, sammelt man besser mit.
    ziel = _setting_row(fenster, innen, t('s_zeit'), t('s_zeit_h'))

    def zeit_um():
        neu_wert = not pfade.einstellung_wahrheit('spielzeit_zeigen', False)
        pfade.einstellung_setzen('spielzeit_zeigen', neu_wert)
        # ⚠ Die Kopfzeile wird beim Fensterbau EINMAL zusammengesetzt. Ohne
        # Neuaufbau bliebe der Schalter wirkungslos, bis das Programm neu
        # startet — und das sieht aus, als tue er nichts.
        fenster.root.after(60, fenster.rebuild)
        return neu_wert

    toggle_switch(ziel, pfade.einstellung_wahrheit('spielzeit_zeigen', False),
                    zeit_um).pack()

    ziel = _setting_row(fenster, innen,
                 t('autostart_win') if sys.platform.startswith('win')
                 else t('autostart_linux'),
                 t('s_autostart_h'))
    if autostart.possible():
        def autostart_um():
            neu_wert = not autostart.is_on()
            autostart.set(neu_wert)
            fenster.say(t('s_al_autostart')
                          % (t('e_an') if neu_wert else t('e_aus')))
            return autostart.is_on()

        schalter = toggle_switch(ziel, autostart.is_on(), autostart_um)
        schalter.pack()
        # Mitschalten, wenn der Autostart woanders umgestellt wird — etwa am
        # Symbol im Overlay, das ja gleichzeitig sichtbar ist.
        autostart.register_display(
            lambda: schalter.draw(autostart.is_on()))
    else:
        tk.Label(ziel, text=t('s_nicht_moegl'), bg=BG, fg=SUB,
                 font=fenster.f_small).pack()

    _menu_entry_field(fenster, innen)

    ziel = _setting_row(fenster, innen, t('s_tray'),
                 t('s_tray_h'))
    if sys.platform.startswith('win'):
        def tray_um():
            neu_wert = not pfade.einstellung_wahrheit('tray', True)
            pfade.einstellung_setzen('tray', neu_wert)
            return neu_wert

        toggle_switch(ziel, pfade.einstellung_wahrheit('tray', True),
                        tray_um).pack()
    else:
        tk.Label(ziel, text=t('s_nur_win'), bg=BG, fg=SUB,
                 font=fenster.f_small).pack()


def _display(fenster, rahmen):
    from . import pfade
    from .main_window import toggle_switch
    _heading(fenster, rahmen, t('hf_anzeige'),
                  t('s_anz_lead'))
    innen = _scroll_area(rahmen)
    e = _settings_parts(fenster)

    # --- Wie sich das Overlay im Spiel verhält -------------------------------
    # Angestoßen von einer Rückmeldung von Haldjas (pr0): „Das Overlay ist permanent
    # zu sehen und nicht durchklickbar. Wenn ich im Kampf mit der Maus
    # hineinkomme, wird das unangenehm."
    ziel = _setting_row(fenster, innen, t('s_ov_modus'), t('s_ov_modus_h'), wide=True)
    modus = _choice(fenster, ziel,
                  [('immer', t('s_ov_immer')), ('popup', t('s_ov_popup'))],
                  pfade.einstellung('overlay_modus') or 'immer',
                  lambda k: _overlay_mode(fenster, modus, k))
    modus.pack()

    # ⚠⚠ **Die Tastenkombination.** Star Citizen laeuft im Vollbild und blendet
    # den Mauszeiger aus: Wer nachsehen will, ob er einen Bauplan schon hat,
    # muss heraustabben und das Fenster dann BLIND suchen und anklicken. Am
    # 31.08.2026 als Nutzerwunsch gemeldet.
    # ⚠⚠ **Die Ecke — im Pop-up-Betrieb der einzige Weg.** Dort reicht das
    # Overlay Mausklicks durch und laesst sich deshalb nicht ziehen. Ohne
    # diese Einstellung koennen diese Nutzer es ueberhaupt nicht
    # positionieren. Am 31.08.2026 gemeldet.
    ziel = _setting_row(fenster, innen, t('s_ov_ecke'), t('s_ov_ecke_h'), wide=True)
    ecke = _choice(fenster, ziel,
                 [('frei', t('s_ov_ecke_frei')),
                  ('oben-links', t('s_ov_ecke_ol')),
                  ('oben-rechts', t('s_ov_ecke_or')),
                  ('unten-links', t('s_ov_ecke_ul')),
                  ('unten-rechts', t('s_ov_ecke_ur'))],
                 pfade.einstellung('overlay_ecke') or 'frei',
                 lambda k: _overlay_corner(fenster, ecke, k))
    ecke.pack()
    # ⭐ Zieht jemand das Overlay mit der Hand woandershin, hebt es die Ecke
    # selbst auf (`Overlay._verschoben`) — diese Liste muss das sehen, sonst
    # steht hier weiter „unten links", waehrend das Fenster woanders sitzt.
    # ⚠ `select_quiet`: Die Auswahl soll sich nur neu beschriften, nicht den
    # Rueckruf ausloesen — der wuerde die Ecke gleich wieder anwenden.
    from . import overlay as _ov_anzeige
    _ov_anzeige.CORNER_DISPLAY[0] = lambda k: ecke.select_quiet(k)

    # ⭐ **Wo die Leiste sitzt, entscheidet der Nutzer** (13.09.2026). Bisher
    # hing das an der Ecke: untere Ecke = Leiste unten, sonst oben. Seit ein
    # Verschieben die Ecke auf „frei" stellt, waere sie damit immer oben — wer
    # sie unten hatte, haette sie verloren. Also eine eigene Einstellung.
    ziel = _setting_row(fenster, innen, t('s_ov_leiste'), t('s_ov_leiste_h'),
                 wide=True)
    leiste = _choice(fenster, ziel,
                   [('oben', t('s_ov_leiste_oben')),
                    ('unten', t('s_ov_leiste_unten'))],
                   pfade.einstellung('overlay_leiste') or 'oben',
                   lambda k: _overlay_bar(fenster, leiste, k))
    leiste.pack()

    _hotkey_field(fenster, innen)

    ziel = _setting_row(fenster, innen, t('s_ov_dauer'), t('s_ov_dauer_h'))
    from .main_window import round_entry as _zahlfeld
    dauer = _zahlfeld(ziel, None, fenster.f_small, '#0c1017', LINE, ACCENT, FG,
                      width=6, justify='right')
    dauer.insert(0, str(pfade.einstellung_zahl('popup_sekunden', 6, 2, 60)))
    dauer.holder.pack()

    def dauer_merken(_=None):
        try:
            wert = max(2, min(60, int(dauer.get())))
            pfade.einstellung_setzen('popup_sekunden', wert)
            fenster.say(t('s_ov_dauer_sagen') % wert)
        except ValueError:
            pass

    dauer.bind('<FocusOut>', dauer_merken)
    dauer.bind('<Return>', dauer_merken)

    ziel = _setting_row(fenster, innen, t('s_ov_durch'), t('s_ov_durch_h'))
    if _click_through_possible():
        # ⚠ Nicht `_switch` nennen — so heisst in dieser Datei bereits eine
        # Funktion, und ein lokaler Name wuerde sie verdecken (Selbsttest 67).
        # (Bis P4 Stufe 7d hiess sie `_schalter`.)
        _durch_schalter = toggle_switch(
            ziel, pfade.einstellung_wahrheit('durchklickbar', False),
            lambda: _click_through_toggle(fenster))
        _durch_schalter.pack()

        # ⚠ Das Durchreichen laesst sich auch am Schloss des Overlays umlegen.
        # Ohne diesen Draht zeigte der Schalter dann weiter den alten Zustand,
        # solange die Seite offen war.
        def _nachziehen(zustand):
            try:
                if _durch_schalter.winfo_exists():
                    _durch_schalter.draw(bool(zustand))
            except tk.TclError:
                pass                     # Seite ist weg - nichts nachzuziehen

        # ⚠ `overlay` wird hier lokal geholt wie ueberall in dieser Datei:
        # Auf Modulebene waere es ein Zirkelbezug.
        from . import overlay as _ov
        _ov.CLICK_THROUGH_DISPLAY[0] = _nachziehen
    else:
        # Ehrlich statt still: Unter nativem Wayland kann ein gewöhnliches Fenster
        # keine Klicks weiterreichen. Ein Schalter, der nichts bewirkt, wäre
        # schlimmer als gar keiner.
        tk.Label(ziel, text=t('s_ov_durch_nein'), bg=BG, fg=SUB,
                 font=fenster.f_small, anchor='w', justify='left').pack(fill='x')

    ziel = _setting_row(fenster, innen, t('hf_schrift'), t('hf_schrift_hilfe'),
                 wide=True)
    # ⭐⭐ **„Sehr groß" ist seit 14.09.2026 wieder dabei** — und die Geschichte
    # dazu gehört hierher, weil sie zeigt, wann ein festgeschriebener Rückbau
    # überprüft werden muss.
    #
    # **Warum es raus war (30.08.2026):** Die Stufe vergrösserte Schrift,
    # Symbole und Knöpfe so weit, dass die daraus folgende **Mindesthöhe
    # grösser wurde als ein Bildschirm** — bei zwei übereinander stehenden
    # Monitoren lief das Fenster in den zweiten hinein. Eine Einstellung, die
    # das Fenster unbrauchbar macht, gehört nicht angeboten. Das war richtig.
    #
    # **Warum es zurück ist:** Nachgemessen am 14.09.2026 beträgt die
    # Mindestgrösse dort **1215 × 380 px** — auch nachdem alle 33 Seiten
    # gebaut sind. Sie passt damit auf jeden üblichen Bildschirm. Der Grund
    # für den Rückbau hat sich erledigt, ohne dass es jemandem aufgefallen
    # wäre: Die Mindesthöhe hängt seit der `minsize()`-Reparatur nicht mehr
    # an der Schriftstufe.
    #
    # ⚠⚠ **Ein festgeschriebener Rückbau ist ein Zeitstempel, keine
    # Wahrheit.** Genau dieselbe Lehre wie beim Titelleisten-Umbau, der nach
    # vier Anläufen als unmöglich galt und danach im ersten gelang.
    #
    # **Und der Anlass war kein technischer:** Bomb20 und Haldjas wollten die
    # Stufe zurück — sie lesen den Text sonst schlecht. Eine Einstellung, die
    # niemandem schadet und zwei Leuten das Lesen ermöglicht, wird angeboten.
    wahl = _choice(fenster, ziel,
                 [(s, t('hf_s_' + s))
                  for s in ('klein', 'normal', 'gross', 'sehrgross')],
                 pfade.einstellung('schriftgroesse') or 'normal',
                 # ⚠ Nur noch der eine Aufruf. `set_font_size()` baut
                 # das Fenster neu auf — damit zeichnet sich die Wahl selbst
                 # richtig, und die Rückmeldung kommt von dort, nach dem
                 # Aufbau. Das frühere `wahl.select(k)` und `say()` hier
                 # liefen beide ins Leere, sobald neu gezeichnet wurde.
                 lambda k: fenster.set_font_size(k))
    wahl.pack()

    ziel = _setting_row(fenster, innen, t('e_deckkraft'),
                 t('s_deck_h'))
    from .main_window import slider as schieberegler
    reihe = tk.Frame(ziel, bg=BG)
    reihe.pack()
    wertlabel = tk.Label(reihe, text='%d %%' % e.deckkraft.get(), bg=BG,
                         fg=ACCENT, font=fenster.f_small, width=6, anchor='e')

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

    ziel = _setting_row(fenster, innen, t('s_klapp'),
                 t('s_klapp_h'))

    def klapp_um():
        neu_wert = not pfade.einstellung_wahrheit('eingeklappt', False)
        pfade.einstellung_setzen('eingeklappt', neu_wert)
        return neu_wert

    toggle_switch(ziel, pfade.einstellung_wahrheit('eingeklappt', False),
                    klapp_um).pack()

    ziel = _setting_row(fenster, innen, t('s_vorne'),
                 t('s_vorne_h'))

    def vorne_um():
        neu_wert = not pfade.einstellung_wahrheit('immer_vorne', True)
        pfade.einstellung_setzen('immer_vorne', neu_wert)
        fenster.say(t('s_an_vorne')
                      % (t('e_an') if neu_wert else t('e_aus')))
        return neu_wert

    toggle_switch(ziel, pfade.einstellung_wahrheit('immer_vorne', True),
                    vorne_um).pack()

    ziel = _setting_row(fenster, innen, t('s_zeilen'),
                 t('s_zeilen_h'))
    from .main_window import round_entry
    zahl = round_entry(ziel, None, fenster.f_small, '#0c1017', LINE, ACCENT, FG,
                       width=6, justify='right')
    zahl.insert(0, str(pfade.einstellung_zahl('max_zeilen', 20, 5, 100)))
    zahl.holder.pack()

    def zahl_merken(_=None):
        try:
            pfade.einstellung_setzen('max_zeilen',
                                     max(5, min(100, int(zahl.get()))))
            fenster.say(t('s_an_zeilen') % zahl.get())
        except ValueError:
            pass

    zahl.bind('<FocusOut>', zahl_merken)
    zahl.bind('<Return>', zahl_merken)

    ziel = _setting_row(fenster, innen, t('s_lage'),
                 t('s_lage_h'))

    def lage_weg():
        # Die gemerkte Lage wegwerfen reicht nicht: Ohne Positionsangabe stellt Tk
        # das Fenster nach `+0+0`, und bei einem hochkant stehenden Monitor links
        # außen liegt dort gar kein Bild — der Knopf hätte das Overlay also wieder
        # dorthin geschickt, wo man es sucht. Deshalb wird aktiv die Standardlage
        # gesetzt: mittig auf dem Hauptbildschirm. Wie viele Bildschirme jemand hat,
        # wissen wir nicht; die Mitte des Hauptbildschirms passt überall.
        from . import screen
        try:
            os.remove(pfade.app_datei('watcher.json'))
        except OSError:
            pass
        overlay = screen.OVERLAY[0]
        if overlay is not None:
            try:
                overlay.geometry(screen.centered(overlay, 440, 1000))
            except Exception as ausnahme:
                fehler.merken('seiten.lage_weg', ausnahme)
        fenster.say(t('s_an_lage_weg'))

    _button(fenster, ziel, t('s_zuruecksetzen'), lage_weg).pack()


def _folders(fenster, rahmen):
    from . import pfade
    _heading(fenster, rahmen, t('hf_ordner'),
                  t('s_ordner_lead'))
    innen = _scroll_area(rahmen)
    e = _settings_parts(fenster)

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
                t('s_sc_weg_h'), color=GOLD)

    tk.Label(innen, text=t('e_spiel'), bg=BG, fg=FG, font=fenster.f_bold,
             anchor='w').pack(fill='x', pady=(6, 0))
    _body_text(innen, t('e_spiel_hilfe'), fenster.f_small, fill='x')

    def spiel_waehlen():
        # ⚠ Vorher lief das über `e._waehlen(...)`, und das übergibt
        # `parent=self.root` — eingebettet ist das ein Rahmen, der nie gepackt
        # wird. Der Dialog erschien deshalb nicht: „beim Klick passiert nichts".
        gewaehlt = choose_folder(t('e_spiel'), e.spiel.get())
        if gewaehlt:
            e.spiel.set(gewaehlt)
            e._speichern()
            fenster.say(t('e_neustart_noetig'))

    _path_field(fenster, innen, e.spiel, spiel_waehlen,
              oeffnen=lambda: fenster.say(
                  t('s_or_geoeffnet') if _show_folder(e.spiel.get())
                  else t('s_or_nicht_auf')))

    tk.Label(innen, text=t('s_eigene'), bg=BG, fg=FG, font=fenster.f_bold,
             anchor='w').pack(fill='x', pady=(20, 0))
    _body_text(innen, t('s_eigene_h'), fenster.f_small, fill='x')
    ablage = tk.StringVar(value=pfade.app_ordner())

    def ablage_oeffnen():
        # Nur melden, was auch stimmt: „Ordner geöffnet" zu sagen, während gar
        # nichts aufgeht, ist schlimmer als eine ehrliche Fehlanzeige.
        fenster.say(t('s_or_geoeffnet') if _show_folder(pfade.app_ordner())
                      else t('s_or_nicht_auf'))

    def ablage_waehlen():
        # ⚠ Hier stand nur ein Hinweis in der Fußzeile („lässt sich in den
        # Einstellungen hinterlegen") — auf der Seite, die genau diese Einstellung
        # IST. Für den Nutzer sah es aus, als täte der Knopf nichts.
        gewaehlt = choose_folder(t('s_eigene'), ablage.get())
        if not gewaehlt:
            return
        _move_storage(fenster, ablage, gewaehlt)

    _path_field(fenster, innen, ablage, ablage_waehlen, oeffnen=ablage_oeffnen)

    tk.Label(innen, text='%s  —  %s' % (t('e_launcher'), t('s_optional')), bg=BG, fg=FG,
             font=fenster.f_bold, anchor='w').pack(fill='x', pady=(20, 0))
    _body_text(innen, t('e_launcher_hilfe'), fenster.f_small, fill='x')
    def launcher_waehlen():
        gewaehlt = choose_folder(t('e_launcher'), e.launcher.get())
        if gewaehlt:
            e.launcher.set(gewaehlt)
            e._speichern()
            fenster.say(t('e_neustart_noetig'))

    _path_field(fenster, innen, e.launcher, launcher_waehlen,
              platzhalter=t('s_or_leer'))

    _start_command_field(fenster, innen)


def _move_storage(fenster, ablage, ziel):
    """Den Ablage-Ordner umstellen — **und die Daten mitnehmen**.

    ⚠⚠ **Bis v3.19.0 setzte der Knopf nur die Einstellung.** Verschoben wurde
    nichts; gemeldet wurde „Neustart nötig". Wer umstellte, startete neu und
    sah ein leeres Programm — Bestand, Merkliste, Auftrags-Protokoll lagen noch
    im alten Ordner, aber das sagte ihm niemand. Für den Nutzer sieht das nicht
    nach einem halben Umzug aus, sondern nach Datenverlust.

    Das war der Grund, warum der eigentlich beste Rat für Doppelstart-Nutzer
    nicht gegeben werden konnte: „Leg die Ablage auf eine Platte, die beide
    Systeme sehen" wäre mit diesem Knopf eine Falle gewesen.

    **Vier Lagen, vier Antworten** — sie unterscheiden sich, und keine darf
    stillschweigend passieren:

    | Lage | was geschieht |
    |---|---|
    | Ziel nicht beschreibbar | abbrechen, Grund nennen — nichts wird gesetzt |
    | Ziel leer, altes voll | fragen „mitnehmen?", dann kopieren |
    | Ziel hat schon Dateien | fragen „die dort benutzen?" — nichts überschreiben |
    | nichts zu kopieren | still umstellen, es gibt nichts zu erzählen |
    """
    from .main_window import ask_yes_no
    alt = pfade.app_ordner()
    if os.path.abspath(alt) == os.path.abspath(ziel):
        return

    schreibbar, fremde, grund = pfade.ablage_lage(ziel)
    if not schreibbar:
        # ⚠ Genau hier landet eine nur lesend eingehängte Windows-Platte. Ohne
        # diese Prüfung stünde der neue Pfad in den Einstellungen, und beim
        # nächsten Start wäre der Ordner unbrauchbar.
        fenster.say(t('s_ab_nicht_schreibbar') % pfade.kuerzen(grund))
        return

    eigene = len(pfade._dateien_der_ablage(alt))

    if fremde:
        # Am Ziel liegt schon eine Ablage — der zweite Rechner beim
        # Doppelstart. Seine Daten gehören ihm; wir fassen sie nicht an.
        if not ask_yes_no(fenster.root, t('s_ab_titel'),
                             t('s_ab_belegt') % fremde,
                             yes_text=t('s_ab_belegt_ja'), no_text=t('e_abbrechen')):
            return
        _set_storage(fenster, ablage, ziel)
        fenster.say(t('s_ab_uebernommen'))
        return

    if not eigene:
        _set_storage(fenster, ablage, ziel)
        fenster.say(t('e_neustart_noetig'))
        return

    if not ask_yes_no(fenster.root, t('s_ab_titel'),
                         t('s_ab_mitnehmen') % eigene,
                         yes_text=t('s_ab_mitnehmen_ja'), no_text=t('s_ab_ohne')):
        # Bewusst ohne Daten umstellen — auch das ist eine gültige Wahl.
        _set_storage(fenster, ablage, ziel)
        fenster.say(t('e_neustart_noetig'))
        return

    kopiert, uebersprungen, misslungen = pfade.ablage_umziehen(alt, ziel)
    if misslungen:
        # ⚠⚠ **Bei einem Fehler wird NICHT umgestellt.** Sonst zeigt die
        # Einstellung auf einen Ordner mit lückenhaftem Bestand, und der
        # vollständige liegt am alten Ort, den niemand mehr ansieht.
        fenster.say(t('s_ab_misslungen') % (misslungen, kopiert))
        return
    _set_storage(fenster, ablage, ziel)
    fenster.say(t('s_ab_fertig') % (kopiert, pfade.kuerzen(alt)))


def _set_storage(fenster, ablage, ziel):
    """Die Einstellung schreiben und das Feld nachziehen."""
    pfade.einstellung_setzen('ablage_ordner', ziel)
    ablage.set(ziel)


def _overlay_corner(fenster, wahl, kennung):
    """Die Ecke merken und sofort anwenden.

    ⚠ Sofort, nicht erst beim naechsten Start: Wer eine Ecke waehlt, will
    sehen, ob sie die richtige ist — und im Pop-up-Betrieb kann er das Fenster
    danach nicht selbst hinschieben.
    """
    pfade.einstellung_setzen('overlay_ecke', kennung)
    try:
        wahl.select(kennung)
    except Exception:
        pass
    from . import overlay as ov
    steuerung = ov.OVERLAY_CONTROL[0]
    if steuerung is not None and hasattr(steuerung, 'ecke_anwenden'):
        steuerung.ecke_anwenden()


def _overlay_bar(fenster, wahl, kennung):
    """Die Leiste nach oben oder unten haengen — sofort.

    ⚠ Sofort und nicht erst beim naechsten Start: Wer eine Seite waehlt, will
    sehen, ob sie die richtige ist. Dieselbe Begruendung wie bei der Ecke.
    """
    pfade.einstellung_setzen('overlay_leiste', kennung)
    try:
        wahl.select(kennung)
    except Exception:
        pass
    from . import overlay as ov
    steuerung = ov.OVERLAY_CONTROL[0]
    if steuerung is not None and hasattr(steuerung, 'leiste_anwenden'):
        steuerung.leiste_anwenden()


def _hotkey_field(fenster, innen):
    """Die Tastenkombination einstellen — oder ehrlich sagen, warum nicht.

    ⚠⚠ **Unter Wayland steht hier keine Eingabe, sondern die Erklaerung.** Ein
    leeres Feld, das nichts bewirkt, waere schlimmer als gar keins: Der Nutzer
    tippt etwas ein, nichts passiert, und er sucht den Fehler bei sich. Das
    System laesst es nicht zu — also sagen wir das und stellen den fertigen Weg
    daneben.
    """
    from . import hotkey as hk
    geht, grund = hk.possible()
    if not geht and grund == 'wayland':
        _setting_row(fenster, innen, t('s_hk'), t('s_hk_wayland'), wide=True)
        return
    if not geht:
        return                       # kein Bildschirm, kein Windows — still

    ziel = _setting_row(fenster, innen, t('s_hk'), t('s_hk_h'), wide=True)
    reihe = tk.Frame(ziel, bg=BG)
    reihe.pack(anchor='w')

    from .main_window import round_entry
    feld = round_entry(reihe, None, fenster.f_small, '#0c1017', LINE, ACCENT,
                       FG, width=18)
    feld.insert(0, pfade.einstellung('hotkey') or hk.DEFAULT)
    feld.holder.pack(side='left')

    def merken(_=None):
        wunsch = feld.get().strip()
        mods, taste = hk.parse(wunsch)
        if not mods:
            fenster.say(t('s_hk_falsch'))
            return
        pfade.einstellung_setzen('hotkey', wunsch)
        # ⚠ Sofort ausprobieren, nicht erst beim naechsten Start: „belegt"
        # erfaehrt man sonst zu einem Zeitpunkt, an dem niemand mehr weiss,
        # dass er etwas eingestellt hat.
        from . import overlay as ov
        wache = getattr(ov.OVERLAY_CONTROL[0], 'hotkey', None)
        if wache is None:
            fenster.say(t('e_neustart_noetig'))
            return
        ok, warum = wache.register(wunsch)
        # ⚠ Getrennte Zweige statt eines Ausdrucks: Pruefung 10 liest, was in
        # `say()` steht, und haelt einen Vergleichswert sonst fuer einen
        # sichtbaren Text. Sie hat recht, so herum ist es ohnehin lesbarer.
        if ok:
            fenster.say(t('s_hk_ok', wunsch))
        elif warum == 'belegt':
            fenster.say(t('s_hk_belegt', wunsch))
        else:
            fenster.say(t('s_hk_falsch'))

    feld.bind('<Return>', merken)
    _button(fenster, reihe, t('s_or_uebernehmen'), merken).pack(side='left',
                                                            padx=(8, 0))


def _start_command_field(fenster, innen):
    """Ein eigener Startbefehl für Star Citizen — für alle ohne LUG Helper.

    ⚠ Diese Einstellung gab es schon lange (`spielstarter`), nur **nirgends in
    der Oberfläche**: Sie stand allein in der `einstellungen.json`. Wer über
    Lutris oder Heroic spielt, sah deshalb gar keinen Startknopf und hatte keine
    Möglichkeit, das zu ändern — der Ausweg war vorhanden und unerreichbar.

    Ein Weg, den man nur kennt, wenn man den Quelltext gelesen hat, ist kein Weg.
    """
    from . import pfade

    tk.Label(innen, text=t('s_or_start'), bg=BG, fg=FG, font=fenster.f_bold,
             anchor='w').pack(fill='x', pady=(20, 0))
    _body_text(innen, t('s_or_start_h'), fenster.f_small, fill='x')
    _body_text(innen, t('s_or_start_bsp'), fenster.f_small, color=SUB,
                fill='x', pady=(2, 0))

    wert = tk.StringVar(value=pfade.einstellung('spielstarter') or '')

    def uebernehmen():
        text = (wert.get() or '').strip()
        pfade.einstellung_setzen('spielstarter', text)
        fenster.say(t('s_or_start_ok') if text else t('s_or_start_weg'))
        # Der Startknopf hängt daran — die Leiste muss ihn neu bewerten.
        try:
            fenster.rebuild()
        except Exception as ausnahme:
            fehler.merken('seiten.startbefehl.aufbauen', ausnahme)

    reihe = tk.Frame(innen, bg=BG)
    reihe.pack(fill='x', pady=(8, 0))
    from .main_window import round_entry
    feld = round_entry(reihe, wert, fenster.f_small, '#0c1017', LINE, ACCENT, FG)
    feld.holder.pack(side='left', fill='x', expand=True, padx=(0, 8))
    _button(fenster, reihe, t('s_or_uebernehmen'), uebernehmen).pack(side='left')


def _menu_entry_field(fenster, innen):
    """Startmenü-Eintrag anlegen oder entfernen — nur unter Linux sinnvoll.

    Unter Windows erledigt das der Installer; dort wäre der Punkt nur Ballast.
    """
    from . import desktop_entry
    if not desktop_entry.available():
        return
    ziel = _setting_row(fenster, innen, t('s_menue'), t('s_menue_h'), wide=True)
    reihe = tk.Frame(ziel, bg=BG)
    reihe.pack()
    stand = tk.Label(reihe, text='', bg=BG, fg=SUB, font=fenster.f_small)

    def zeigen():
        stand.configure(text=t('s_menue_steht') if desktop_entry.exists() else '')

    def anlegen():
        geklappt, wohin = desktop_entry.create()
        fenster.say((t('as_menue_da') % wohin) if geklappt
                      else t('as_menue_nein') % wohin)
        zeigen()

    def weg():
        desktop_entry.remove()
        fenster.say(t('s_menue_weg_ok'))
        zeigen()

    _button(fenster, reihe, t('s_menue_anlegen'), anlegen).pack(side='left')
    _button(fenster, reihe, t('s_menue_weg'), weg).pack(side='left', padx=8)
    stand.pack(side='left', padx=(10, 0))
    zeigen()


def _click_through_possible():
    from . import overlay
    try:
        return overlay.click_through_possible()
    except Exception:
        return False


def _click_through_toggle(fenster):
    """Klicks durchreichen ein- oder ausschalten — und sofort anwenden."""
    from . import overlay, pfade
    neu_wert = not pfade.einstellung_wahrheit('durchklickbar', False)
    pfade.einstellung_setzen('durchklickbar', neu_wert)
    geklappt = True
    wurzel = overlay.OVERLAY_WINDOW[0] if overlay.OVERLAY_WINDOW else None
    if wurzel is not None:
        try:
            geklappt = overlay.set_click_through(wurzel, neu_wert)
        except Exception as ausnahme:
            fehler.merken('seiten.durchklick', ausnahme)
            geklappt = False
    if neu_wert and not geklappt:
        fenster.say(t('ov_durchklick_geht_nicht'))
        pfade.einstellung_setzen('durchklickbar', False)
        return False
    fenster.say(t('s_ov_durch_sagen')
                  % (t('e_an') if neu_wert else t('e_aus')))
    return neu_wert


def _overlay_mode(fenster, wahl, kennung):
    """Zwischen „immer sichtbar" und „nur bei Neuzugang" umstellen."""
    from . import overlay, pfade
    wahl.select(kennung)
    pfade.einstellung_setzen('overlay_modus', kennung)
    wurzel = overlay.OVERLAY_WINDOW[0] if overlay.OVERLAY_WINDOW else None
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
        fenster.say(t('s_ov_popup_gleich'))
    else:
        fenster.say(t('s_ov_modus_sagen') % t('s_ov_immer'))


def clean_environment():
    """Weiterleitung — die Wahrheit steht in `file_picker` (bis 11.09.2026 `dateiwahl`).

    ⚠ Sie stand jahrelang hier, weil sie hier zuerst gebraucht wurde. Seit die
    Dateiauswahl ein eigenes Modul hat, gehört sie dorthin: Beide brauchen
    dieselbe Wäsche, und zwei Versionen davon wären eine zu viel. Die
    Weiterleitung bleibt, weil `_show_folder` und der Spielstart sie hier
    aufrufen.
    """
    from . import pfade as pfade_modul
    return pfade_modul.saubere_umgebung()


def choose_folder(titel, start=None):
    """Weiterleitung — siehe `file_picker.choose_folder`."""
    from . import file_picker
    return file_picker.choose_folder(titel, start)


def _in_path(name):
    """Gibt es dieses Programm auf dem Rechner?"""
    import shutil
    return bool(shutil.which(name))


def _show_folder(pfad):
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
            subprocess.Popen(['xdg-open', pfad], env=clean_environment())
        return True
    except Exception as ausnahme:
        fehler.merken('seiten.ordner_zeigen', ausnahme, pfad)
        return False


def _game(fenster, rahmen):
    """Auftragstexte — Textquelle wählen und die Bauplan-Angaben eintragen."""
    from . import pfade
    from .main_window import toggle_switch
    _heading(fenster, rahmen, t('hf_spiel'), t('s_sp_lead'))
    innen = _scroll_area(rahmen)
    e = _settings_parts(fenster)

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
                        t('s_sp_aus_rest_h'), color=SUB)
            else:
                _status(fenster, kasten, 'offen', t('s_sp_aus_hinweis'), '',
                        color=SUB)
            return
        if lage['drin']:
            zusatz = []
            if lage['quelle']:
                zusatz.append(t('s_sp_quelle_ist')
                              % t(_SOURCE_LABELS.get(lage['quelle'], 's_sp_q_or')))
            if lage['stand']:
                zusatz.append(str(lage['stand']))
            _status(fenster, kasten, 'haken', t('s_sp_steht'), ' · '.join(zusatz))
        else:
            _status(fenster, kasten, 'offen', t('s_sp_nichts'), t('s_sp_nichts_h'),
                    color=SUB)

    # Damit auch Aktionen im Einstellungsobjekt den Kasten auffrischen.
    e.lage_melder = lage_zeigen
    lage_zeigen()

    # --- Textquelle ----------------------------------------------------------
    ziel = _setting_row(fenster, innen, t('s_sp_quelle'), t('s_sp_quelle_h'),
                 wide=True)
    wahl = _choice(fenster, ziel,
                 [('deutsch', t('s_sp_q_de')), ('starstrings', t('s_sp_q_ss')),
                  ('original', t('s_sp_q_or'))],
                 pfade.einstellung('inj_quelle') or '',
                 lambda k: _choose_source(fenster, e, wahl, k, lage_zeigen))
    wahl.pack()

    ziel = _setting_row(fenster, innen, t('s_sp_auto'), t('s_sp_auto_h'))

    def inj_auto_um():
        neu_wert = not pfade.einstellung_wahrheit('inj_auto', True)
        pfade.einstellung_setzen('inj_auto', neu_wert)
        fenster.say(t('s_sp_auto_sagen')
                      % (t('e_an') if neu_wert else t('e_aus')))
        return neu_wert

    toggle_switch(ziel, pfade.einstellung_wahrheit('inj_auto', True),
                    inj_auto_um).pack()

    # --- An oder aus ---------------------------------------------------------
    # ⚠ Der Schalter fehlte ganz. Wer auf PTU spielt oder die Textdatei in Ruhe
    # lassen will, hatte keine Möglichkeit außer „Wieder entfernen" — und beim
    # nächsten Start schrieb das Werkzeug wieder hinein.
    ziel = _setting_row(fenster, innen, t('s_sp_an'), t('s_sp_an_h'))

    def inj_an_um():
        neu_wert = not pfade.einstellung_wahrheit('inj_an', True)
        pfade.einstellung_setzen('inj_an', neu_wert)
        fenster.say(t('s_sp_an_sagen')
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

    toggle_switch(ziel, pfade.einstellung_wahrheit('inj_an', True),
                    inj_an_um).pack()

    # --- Angaben am Gegenstand ----------------------------------------------
    # Klasse, Größe und Gütegrad direkt am Namen — bei Raketen der Suchkopf.
    # Abschaltbar, weil es die Gegenstandsnamen im Spiel verändert: Wer das
    # nicht will, soll die Bauplan-Angaben trotzdem behalten können.
    ziel = _setting_row(fenster, innen, t('s_sp_angaben'), t('s_sp_angaben_h'))

    def angaben_um():
        from . import injektion as inj_modul
        neu_wert = not pfade.einstellung_wahrheit(inj_modul.EINSTELLUNG_ANGABEN,
                                                  True)
        pfade.einstellung_setzen(inj_modul.EINSTELLUNG_ANGABEN, neu_wert)
        fenster.say(t('s_sp_angaben_sagen')
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
    toggle_switch(ziel,
                    pfade.einstellung_wahrheit(_inj.EINSTELLUNG_ANGABEN, True),
                    angaben_um).pack()

    ziel = _setting_row(fenster, innen, t('s_sp_hand'), t('s_sp_hand_h'), wide=True)
    reihe = tk.Frame(ziel, bg=BG)
    reihe.pack()
    _button(fenster, reihe, t('s_sp_jetzt'),
           lambda: (e._inj_erneuern(), lage_zeigen()),
           strong=True).pack(side='left')
    _button(fenster, reihe, t('s_sp_pruefen'),
           lambda: (e._inj_pruefen(), lage_zeigen())).pack(side='left', padx=8)
    _button(fenster, reihe, t('s_sp_weg'),
           lambda: (e._inj_entfernen(), lage_zeigen()),
           danger=True).pack(side='left')

    _status(fenster, innen, '!', t('s_sp_warn'), t('s_sp_warn_h'), color=GOLD)


# Welche Beschriftung zu welcher Quelle gehört — für den Zustandskasten.
_SOURCE_LABELS = {'deutsch': 's_sp_q_de', 'starstrings': 's_sp_q_ss',
              'original': 's_sp_q_or'}


def _choose_source(fenster, e, wahl, kennung, danach):
    """Eine Textquelle einrichten — das dauert, also erst ansagen.

    ⚠ Ohne Ansage sieht es aus, als sei nichts passiert: Das Herunterladen und
    Einsetzen braucht mehrere Sekunden, und in dieser Zeit stand vorher nichts
    im Fenster.
    """
    from . import pfade
    wahl.select(kennung)
    # ⚠ Die Wahl wird **vor** dem Einrichten gemerkt. Sie stand vorher dahinter,
    # und wenn das Herunterladen schiefging (kein Netz, Zertifikat, Server weg),
    # blieb die alte Quelle eingetragen — das Feld zeigte die neue, der Rest des
    # Programms rechnete mit der alten. Erst gilt, was gewählt wurde; ob es auch
    # eingerichtet werden konnte, sagt der Kasten darüber.
    pfade.einstellung_setzen('inj_quelle', kennung)
    fenster.say(t('s_sp_hole') % t(_SOURCE_LABELS.get(kennung, 's_sp_q_or')))
    try:
        e._inj_wechseln(kennung)
    except Exception as ausnahme:
        fehler.merken('seiten.spiel.quelle', ausnahme)
        fenster.say(t('inj_fehler', ausnahme))
    danach()


def _collection(fenster, rahmen):
    from . import export, importer
    _heading(fenster, rahmen, t('hf_bestand'), t('s_be_lead'))
    innen = _scroll_area(rahmen)

    anzahl = _count_collection()
    tk.Label(innen, text=t('s_be_aus'), bg=BG, fg=FG,
             font=fenster.f_title, anchor='w').pack(fill='x', pady=(0, 2))
    _body_text(innen, t('s_be_aus_h'), fenster.f_small,
                fill='x', pady=(0, 12))

    # ⚠ Ein Speichern-Knopf **je Version**, direkt an der Version. Vorher gab es
    # nur einen gemeinsamen Knopf „Einzeln speichern …", und der schrieb immer
    # die Basetool-Version. Wer beim Vorführen scmdb einzeln speichern wollte,
    # suchte vergeblich — es gab den Weg schlicht nicht.
    karte = _card(innen)
    for art, name, wofuer in (('basetool', 'KRT Profit Basetool',
                               t('s_be_n_bp') % anzahl),
                              ('scmdb', 'scmdb.net', t('s_be_n_bp') % anzahl),
                              # ⚠ Die Baupläne DB nimmt **nur** ihr eigenes
                              # Format an (`blueprints[].key` + `isDone`). Die
                              # drei anderen Versionen weist sie mit
                              # „Ungültiges Dateiformat" ab — ohne diese Zeile
                              # kommt der eigene Bestand dort nicht hinein.
                              #
                              # ⛔ Sie heißt **nicht** „SC Deutsch Launcher".
                              # Der Launcher ist das Programm für die
                              # Übersetzung; die Baupläne DB ist die
                              # Bauplan-Übersicht im Browser. Am 13.09.2026
                              # stand hier einen Tag lang der falsche Name.
                              ('bpdb', 'Baupläne DB · Star Citizen Deutsch',
                               t('s_be_n_bp') % anzahl),
                              ('voll', t('s_be_voll'), t('s_be_voll_h'))):
        z = tk.Frame(karte, bg=SURFACE)
        z.pack(fill='x', padx=16, pady=5)
        # ⚠ Die Breite trägt den LÄNGSTEN Namen — „Baupläne DB · Star Citizen
        # Deutsch". Ein Label mit zu kleiner `width` wächst über sie hinaus
        # und schiebt die Spalte daneben nach rechts: Dann steht „413
        # Baupläne" in jeder Zeile woanders.
        tk.Label(z, text=name, bg=SURFACE, fg=FG, font=fenster.f_small,
                 width=34, anchor='w').pack(side='left')
        tk.Label(z, text=wofuer, bg=SURFACE, fg=SUB,
                 font=fenster.f_small).pack(side='left')
        # ⚠ `a=art` als Vorgabewert, nicht `art` direkt. Ein Lambda merkt sich
        # die **Variable**, nicht ihren Wert — ohne diese Zeile hätten alle drei
        # Knöpfe am Ende der Schleife auf „voll" gezeigt und dreimal dasselbe
        # gespeichert.
        _button(fenster, z, t('s_be_speichern_kurz'),
               lambda a=art: einzeln(a)).pack(side='right')

    reihe = tk.Frame(innen, bg=BG)
    reihe.pack(fill='x', pady=(12, 0))

    def in_ablage():
        try:
            ergebnis = export.ablegen()
            wieviele = ergebnis[1] if isinstance(ergebnis, tuple) else ergebnis
            fenster.say(t('s_be_geschrieben') % wieviele)
            _show_folder(export.ablage_ordner())
        except Exception as ausnahme:
            fehler.merken('seiten.bestand.ablegen', ausnahme)
            fenster.say(t('s_be_schiefging'))

    def einzeln(art):
        """Eine einzelne Version speichern — die, an deren Zeile der Knopf steht.

        ⚠ Hier stand `art='basetool'` **fest verdrahtet**, während der Knopf
        „Einzeln speichern …" hieß. Wer scmdb oder die Vollsicherung einzeln
        wollte, bekam wortlos die Basetool-Version; über den Dialog waren die
        anderen beiden gar nicht erreichbar. Gemeldet am
        27.08.2026 („bei einzeln speichern speichert er nur basetool").
        """
        from . import file_picker
        ziel = file_picker.save_file(
            t('s_be_speichern'), suggestion=export.vorschlag(art),
            extension='.json', start=export.ablage_ordner())
        if not ziel:
            return
        try:
            export.schreiben(ziel, art=art)
            fenster.say(t('s_be_gespeichert') % os.path.basename(ziel))
        except Exception as ausnahme:
            fehler.merken('seiten.bestand.einzeln', ausnahme)

    _button(fenster, reihe, t('s_be_alle'), in_ablage,
           strong=True).pack(side='left')
    _button(fenster, reihe, t('s_be_ablage'),
           lambda: _show_folder(export.ablage_ordner())).pack(side='left',
                                                                padx=8)

    # Der Satz nimmt die häufigste Frage vorweg: „Muss ich das jedes Mal von
    # Hand machen?" Nein — seit die Ablage bei jedem neuen Bauplan mitgeschrieben
    # wird, sind die drei Dateien von allein aktuell.
    _body_text(innen, t('s_be_fort'), fenster.f_small, fill='x', pady=(8, 0))

    tk.Label(innen, text=t('s_be_ein'), bg=BG, fg=FG,
             font=fenster.f_title, anchor='w').pack(fill='x', pady=(28, 2))
    _body_text(innen, t('s_be_ein_h'), fenster.f_small,
                fill='x', pady=(0, 12))

    vorschau_platz = tk.Frame(innen, bg=BG)

    def einlesen():
        from . import file_picker
        pfad = file_picker.open_file(
            t('s_be_ein'),
            patterns=(('JSON', '*.json'), (t('alle_dateien'), '*.*')))
        if not pfad:
            return
        art, eintraege = importer.read(pfad)
        for kind in vorschau_platz.winfo_children():
            kind.destroy()
        if not art:
            _status(fenster, vorschau_platz, '!', t('s_be_unbekannt'),
                    t('s_be_unbekannt_h'), color=RED)
            return
        # ⚠⚠ **Erkannt und trotzdem leer** — das gibt es wirklich, und zwar
        # ohne Fehler: Die Baupläne DB gibt auf Wunsch die **vorgemerkten**
        # Baupläne aus, scmdb die nur beobachteten. Beides
        # sind Wunschzettel, keine erspielten Baupläne; wir übernehmen daraus
        # nichts. Ohne diesen Hinweis stünde dort eine Vorschau „0 kommen
        # dazu" mit einem Knopf „0 Baupläne übernehmen" — das sieht kaputt
        # aus, obwohl alles richtig lief.
        if not eintraege:
            _status(fenster, vorschau_platz, '!', t('s_be_leer'),
                    t('s_be_leer_h'), color=GOLD)
            return
        v = importer.preview(eintraege)
        _show_preview(fenster, vorschau_platz, art, eintraege, v)

    _button(fenster, innen, t('s_be_waehlen'), einlesen,
           strong=True).pack(anchor='w')
    _body_text(innen, t('s_be_erkannt'), fenster.f_small,
                fill='x', pady=(10, 0))
    vorschau_platz.pack(fill='x', pady=(14, 20))
    # Der Kasten steht von Anfang an da — sonst wirkt die Seite unfertig, und
    # niemand weiß, dass vor dem Übernehmen noch eine Vorschau kommt.
    _empty_preview(fenster, vorschau_platz)

    # ⚠ **Protokolle erneut einlesen.** Steht hier unten und nicht oben: Es ist
    # kein Weg, den man täglich geht, sondern einer für den Fall, dass etwas
    # fehlt. Denselben Knopf gibt es am Overlay — dort ist er näher an dem
    # Moment, in dem jemand merkt, dass ein Bauplan nicht angekommen ist.
    #
    # Die Arbeit macht der Watcher-Faden (`overlay.request_rescan`),
    # nicht diese Seite: Der Bestand wird an genau einer Stelle geschrieben,
    # sonst überschreibt der Faden das Ergebnis beim nächsten Fund.
    ziel = _setting_row(fenster, innen, t('s_be_neu'), t('s_be_neu_h'))

    def neu_einlesen():
        from . import overlay as ov
        fenster.say(t('s_be_neu_los') if ov.request_rescan()
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
    _button(fenster, ziel, t('s_be_neu'), neu_einlesen).pack()

    # ⚠ **Bestand zurücksetzen — hier und nicht unter „Fehler melden".** Dort
    # stand es bis rc42, und dort sucht es niemand: Wer seinen Bauplan-Stand
    # neu aufbauen will, geht auf die Seite, die seinen Bauplan-Stand verwaltet.
    #
    # Der Platz direkt unter „Protokolle erneut einlesen" ist Absicht — die
    # beiden gehören zusammen und der Unterschied wird erst nebeneinander
    # sichtbar: Einlesen **ergänzt**, was fehlt. Zurücksetzen **wirft weg** und
    # baut neu auf. Wer das falsche nimmt, verliert seinen Stand; getrennt auf
    # zwei Seiten sieht man diesen Unterschied nie.
    ziel = _setting_row(fenster, innen, t('s_be_reset'), t('s_be_reset_h'))

    def zuruecksetzen():
        from .main_window import ask_yes_no

        # ⚠⚠ **Die Zahlen NENNEN, nicht nur warnen.** Am 05.09.2026 hat ein
        # Melder seinen Bestand von **232 auf 3** zurückgesetzt — die Warnung
        # sagte zwar „was älter ist als deine Protokolle, kommt nicht zurück",
        # aber nicht, wie wenig das bei ihm war. Wer „232 → 3" liest, bricht
        # ab; wer nur einen Satz liest, klickt weiter.
        #
        # ⚠ Gerechnet wird aus dem Bestand selbst: Was aus `log` oder
        # `nachlese` stammt, kommt beim Neuaufbau zurück — alles andere
        # (Launcher, Import, von Hand) nicht. Kein Durchlauf über 221
        # Protokolle nötig, die Auskunft liegt schon da.
        frage = t('s_be_reset_frage')
        try:
            daten = bestand_datei.load()
            gesamt = len(daten.get('bauplaene') or {})
            quellen = bestand_datei.by_source(daten)
            bleibt = quellen.get('log', 0) + quellen.get('nachlese', 0)
            if gesamt:
                frage = '%s\n\n%s' % (
                    t('s_be_reset_zahlen') % (gesamt, bleibt, gesamt - bleibt),
                    frage)
        except Exception as ausnahme:
            fehler.merken('seiten.bestand.reset_zahlen', ausnahme)

        if not ask_yes_no(fenster.root, t('s_be_reset'), frage):
            return
        # ⚠⚠ **Jeder Ausgang sagt etwas.** Ein Knopf, der nach der
        # Warnfrage schweigt, ist von einem kaputten nicht zu unterscheiden.
        # Was „geschafft" heisst, entscheidet `collection.reset()` — dort
        # steht auch, warum „war schon weg" dazugehoert.
        stoerung = bestand_datei.reset()
        if stoerung is not None:
            fehler.merken('seiten.bestand.zuruecksetzen', stoerung)
            fenster.say(t('s_be_reset_fehler', stoerung))
            return
        fenster.say(t('s_be_reset_ok'))

    _button(fenster, ziel, t('s_zuruecksetzen'), zuruecksetzen, danger=True).pack()

    _status(fenster, innen, '!', t('s_be_reset_warn'), t('s_be_reset_warn_h'),
            color=GOLD)


def _empty_preview(fenster, eltern):
    """Der Vorschau-Kasten, bevor eine Datei gewählt wurde."""
    innen = _card(eltern, border_color=SUB)
    tk.Label(innen, text=t('s_vorschau_leer'), bg=SURFACE, fg=FG,
             font=fenster.f_bold, anchor='w').pack(fill='x', padx=16,
                                                   pady=(12, 2))
    _body_text(innen, t('s_vorschau_leer_h'), fenster.f_small,
                bg=SURFACE, inset=32, fill='x', padx=16, pady=(0, 12))
    return innen


def _show_preview(fenster, eltern, art, eintraege, v):
    """Was der Import täte — erst nach dem Knopf passiert wirklich etwas."""
    from . import importer
    from .main_window import badge as blase
    innen = _card(eltern, border_color=ACCENT)

    kopf = tk.Frame(innen, bg=SURFACE)
    kopf.pack(fill='x', padx=16, pady=(12, 10))
    tk.Label(kopf, text=t('s_be_vorschau'), bg=SURFACE,
             fg=FG, font=fenster.f_bold).pack(side='left')
    blase(kopf, {'eigen': t('s_be_eigen'), 'basetool': 'KRT Profit Basetool',
                 'scmdb': 'scmdb.net',
                 # ⚠ Beide scmdb-Formate heißen für den Nutzer gleich — ihn
                 # geht nicht an, welche Fassung der Ausfuhr er erwischt hat.
                 'scmdb2': 'scmdb.net',
                 'launcher': 'SC Deutsch Launcher',
                 'bpdb': 'Baupläne DB · Star Citizen Deutsch'}.get(art, art),
          ACCENT, fenster.f_small).pack(side='right')

    zahlen = tk.Frame(innen, bg=SURFACE)
    zahlen.pack(fill='x', padx=16, pady=(0, 10))
    for wert, wofuer, farbe in ((len(v['neu']), t('s_be_dazu'), ACCENT),
                                (len(v['schon_da']), t('s_be_schon'), FG),
                                (len(v['unbekannt']), t('s_be_nicht_kat'), GOLD)):
        s = tk.Frame(zahlen, bg=SURFACE)
        s.pack(side='left', padx=(0, 30))
        tk.Label(s, text=str(wert), bg=SURFACE, fg=farbe,
                 font=fenster.f_title).pack(anchor='w')
        tk.Label(s, text=wofuer, bg=SURFACE, fg=SUB,
                 font=fenster.f_small).pack(anchor='w')

    if v['unbekannt']:
        tk.Label(innen, text=t('s_be_nicht_kat_h')
                             + ' · '.join(v['unbekannt'][:6])
                             + (' …' if len(v['unbekannt']) > 6 else ''),
                 bg=SURFACE, fg=SUB, font=fenster.f_small, anchor='w',
                 justify='left', wraplength=560).pack(fill='x', padx=16,
                                                      pady=(0, 8))

    _body_text(innen, t('s_be_merge'), fenster.f_small,
                bg=SURFACE, inset=32, fill='x', padx=16, pady=(0, 10))

    reihe = tk.Frame(innen, bg=SURFACE)
    reihe.pack(fill='x', padx=16, pady=(0, 14))

    def uebernehmen():
        dazu = importer.merge(eintraege)
        fenster.say(t('s_be_genommen') % dazu)
        innen.holder.destroy()

    k = _button(fenster, reihe, t('s_be_nimm') % len(v['neu']),
               uebernehmen, strong=True)
    k.configure(bg=SURFACE)
    k.pack(side='left')
    k2 = _button(fenster, reihe, t('abbrechen'), innen.holder.destroy)
    k2.configure(bg=SURFACE)
    k2.pack(side='left', padx=8)


def _contract_log(fenster, rahmen):
    """Welche Aufträge wann gespielt wurden — die Rückschau.

    ⚠ **Bewusst eine Liste und sonst nichts.** Keine Auswertung, keine
    Belohnungen, keine Kategorie — das steht schlicht nicht in den Protokollen
    des Spiels. Was drinsteht, ist: welcher Auftrag, wann, wie oft.

    Zwei Zeilenformen, wie beim Bauplan-Bestand:

    * **Läuft noch** → mit Stand, wenn er sich eindeutig zuordnen lässt.
    * **Beendet** → eine Zeile, mehr braucht es nicht.
    """
    from . import missionslog

    _heading(fenster, rahmen, t('hf_auftragslog'), t('s_al_lead'))
    innen = _scroll_area(rahmen)
    _body_text(innen, t('s_al_hinweis'), fenster.f_small, fill='x')

    # ⚠⚠ **Die Daten werden bei JEDEM Zeigen neu geholt, nicht nur beim Bauen.**
    # Die Nachlese der alten Protokolle läuft kurz nach dem Start in einem
    # eigenen Faden. Wer die Seite in dieser Sekunde öffnet, sah bis
    # 04.09.2026 „Noch kein Auftrag aufgezeichnet" — und danach nie wieder
    # etwas anderes, weil die Seite gebaut blieb. Genau so gemeldet, mit acht
    # Aufträgen in der Datei.
    daten = {'alle': []}
    stand = {'art': 'alle'}
    # ⭐ **Wie viele Zeilen zuerst.** Vorgeschlagen von Zwaersch am 07.09.2026:
    # lange Listen begrenzen und unten nachladen lassen, statt alles im Voraus
    # zu bauen. Gemessen wurde vorher, wo sich das lohnt — diese Seite war mit
    # **1479 ms** beim ersten Öffnen die mit Abstand teuerste, alle übrigen
    # standen unter 130 ms. Eine Grenze überall einzubauen hätte nichts
    # gebracht.
    #
    # ⚠ Vorher stand hier ein hartes `treffer[:200]` — **ohne jeden Hinweis**.
    # Wer 392 Aufträge gespielt hat, sah 200 und erfuhr nirgends, dass 192
    # fehlen. Dieselbe Falle wie bei der Patch-Liste: Weggelassen heißt nicht
    # verschwiegen.
    gezeigt = {'anzahl': LOG_ROWS_FIRST}
    # Der Fingerabdruck der zuletzt gezeichneten Liste — siehe `zeichnen()`.
    zuletzt = {'stand': None}

    suche = tk.StringVar()
    liste_rahmen = tk.Frame(innen, bg=BG)

    block = tk.Frame(innen, bg=BG)
    block.pack(fill='x', padx=24, pady=(14, 0))
    tk.Label(block, text=t('s_al_suche'), bg=BG, fg=FG, font=fenster.f_bold,
             anchor='w').pack(fill='x')
    from .main_window import round_entry
    feld = round_entry(block, suche, fenster.f_small, '#0c1017', LINE,
                       ACCENT, FG, placeholder=t('s_pl_auftrag'))
    feld.holder.pack(fill='x', pady=(4, 0))

    # ⚠ Die Filterleiste wird weiter unten befuellt — die Farben und Woerter
    # dazu stehen erst danach. Gepackt wird sie hier, damit sie zwischen
    # Suchfeld und Liste sitzt.
    filterleiste = tk.Frame(innen, bg=BG)
    filterleiste.pack(fill='x', padx=24, pady=(10, 0))

    kopf = tk.Label(innen, text='', bg=BG, fg=SUB, font=fenster.f_small,
                    anchor='w')
    kopf.pack(fill='x', padx=24, pady=(12, 0))
    liste_rahmen.pack(fill='both', expand=True, padx=24, pady=(4, 12))

    # ⚠ **Drei Aussagen, drei Farben** (06.09.2026): Gruen fuer die Leistung,
    # blasses Rot fuer das, was schiefging, Grau fuer das, worueber wir nichts
    # behaupten. Abgebrochen und fehlgeschlagen standen vorher beide in Grau
    # und waren dadurch von „nicht mehr offen" nicht zu unterscheiden.
    #
    # ⚠ Blass, nicht `ROT`: Das kraeftige Rot gehoert den echten Fehlern
    # („Fehler melden"). Ein aufgegebener Auftrag ist eine Notiz, keine
    # Stoerung — er soll auffallen, ohne wie ein Alarm auszusehen.
    farben = {missionslog.ABGESCHLOSSEN: ACCENT,
              missionslog.ABGEBROCHEN: RED_PALE,
              missionslog.FEHLGESCHLAGEN: RED_PALE,
              missionslog.VERFALLEN: SUB,
              missionslog.LAEUFT: GOLD}
    worte = {missionslog.ABGESCHLOSSEN: 's_al_fertig',
             missionslog.ABGEBROCHEN: 's_al_abbruch',
             missionslog.FEHLGESCHLAGEN: 's_al_fehl',
             # Zurueckhaltend in Grau: Es ist keine Leistung und kein Abbruch,
             # nur das Ende der Spur.
             missionslog.VERFALLEN: 's_al_verfallen',
             missionslog.LAEUFT: 's_al_laeuft'}

    def _wort_laufend():
        """„läuft" nur, solange das Spiel wirklich schreibt.

        ⚠⚠ **Gemeldet am 05.09.2026:** „Spiel ist aus, und die Quest die da
        auf läuft steht ist von gestern nacht, da bin ich ohne ab zu brechen
        ausgeloggt weil ich zu müde war."

        Ausloggen beendet keinen Auftrag — das Spiel schreibt dafür nichts ins
        Protokoll. Aufgeräumt wird so ein Fall erst, wenn eine **spätere**
        Sitzung ihn nicht mehr nennt (`_verfallene_schliessen`); beim letzten
        Auftrag vor dem Ausloggen gibt es die noch nicht. Gemessen an 381
        Aufträgen: 68 waren so bereits aufgelöst, genau einer blieb übrig —
        der jüngste.

        ⚠ **Der Zustand bleibt richtig, nur das Wort war es nicht.** Der
        Auftrag ist im Spiel weiter angenommen; beim nächsten Einloggen meldet
        Star Citizen ihn erneut. Ihn zu beenden wäre gelogen. „läuft"
        behauptet aber „jetzt gerade" — und das stimmt bei geschlossenem Spiel
        nicht. „Noch offen" stimmt in beiden Fällen.
        """
        return 's_al_laeuft' if pfade.spiel_laeuft() else 's_al_offen'

    def _anzeige_stand():
        """Der Fingerabdruck dessen, was die Liste zeigen WÜRDE.

        ⚠⚠ Er muss **jede** Quelle enthalten, aus der `zeichnen()` liest —
        Daten **und** Anzeigezustand. Fehlt eine, bleibt die Liste still auf
        dem alten Stand. Dieselbe Regel wie in der Bauplan-Liste, dort in
        vier Prüfrunden teuer gelernt.
        """
        return (repr(daten['alle']), stand['art'], suche.get(),
                gezeigt['anzahl'], _wort_laufend())

    def zeichnen(*_, neu_laden=False, mehr=False):
        # ⚠ Jede neue Auswahl fängt wieder oben an. Ohne das stünde nach einem
        # Filterwechsel „… 12 weitere anzeigen" über einer Liste, die längst
        # vollständig ist.
        if not mehr:
            gezeigt['anzahl'] = LOG_ROWS_FIRST
        if neu_laden or not daten['alle']:
            try:
                daten['alle'] = missionslog.laden()
            except Exception:
                daten['alle'] = []

        # ⭐⭐ **Nur neu zeichnen, wenn sich etwas geändert hat.**
        #
        # Diese Seite wurde bei JEDEM Anzeigen komplett neu gebaut — und nach
        # der Nachlese im Hintergrund gleich noch einmal. Gemessen am
        # 13.09.2026: 388 ms je Anzeigen, davon 332 ms in 1.859 Tk-Aufrufen.
        # Wer nur kurz auf eine andere Seite und zurück klickt, wartete jedes
        # Mal darauf, dass dieselbe Liste neu entsteht.
        #
        # ⚠ Der Abdruck wird am ENDE geschrieben, nicht hier — und nur auf
        # einem erfolgreichen Weg. Bricht das Zeichnen ab, bleibt er ungültig
        # und die Liste wird beim nächsten Mal neu gebaut. (Regel 5.12:
        # „Wer zeichnet, schreibt den Abdruck", und: erst ungültig machen,
        # dann zerstören.)
        jetzt = _anzeige_stand()
        if jetzt == zuletzt.get('stand'):
            return
        zuletzt['stand'] = None

        alle = daten['alle']
        # ⚠ Einmal je Durchlauf, nicht je Zeile: `spiel_laeuft()` sieht auf
        # die Datei, und die Liste hat hunderte Zeilen.
        wort_laufend = _wort_laufend()
        for kind in liste_rahmen.winfo_children():
            kind.destroy()
        if not alle:
            # Noch gar nichts aufgezeichnet — das ist etwas anderes als „die
            # Suche findet nichts" und braucht deshalb einen eigenen Satz.
            # ⚠ Gegen `daten['alle']` gefragt, VOR dem Filtern: Ein Filter,
            # der nichts findet, ist kein leeres Protokoll. Sonst stuende bei
            # „fehlgeschlagen" auf einem sauberen Konto „Noch kein Auftrag
            # aufgezeichnet" — und das waere schlicht gelogen.
            kopf.configure(text='')
            tk.Label(liste_rahmen, text=t('s_al_leer'), bg=BG, fg=SUB,
                     font=fenster.f_small, anchor='w', justify='left',
                     wraplength=560).pack(fill='x', pady=8)
            zuletzt['stand'] = _anzeige_stand()   # auch das ist ein Bild
            return
        # ⚠ **Erst filtern, dann suchen.** Der Kopf zaehlt, was am Ende
        # dasteht — sonst meldet er 386 und zeigt 62.
        if stand['art'] != 'alle':
            alle = [e for e in alle
                    if (e.get('zustand') or missionslog.LAEUFT) == stand['art']]
        treffer = missionslog.suchen(alle, suche.get())
        kopf.configure(text=t('s_al_anzahl', len(treffer)))
        if not treffer:
            tk.Label(liste_rahmen, text=t('s_al_nichts'), bg=BG, fg=SUB,
                     font=fenster.f_small, anchor='w').pack(fill='x', pady=8)
            zuletzt['stand'] = _anzeige_stand()
            return
        # ⚠ Nicht alles auf einmal: Wer hundert Auftraege gespielt hat, wartet
        # sonst beim Oeffnen. Dieselbe Grenze wie in der Bauplan-Liste.
        # ⭐ Gebaut werden alle, gepackt nur die sichtbaren — Tk rechnet sonst
        # die Geometrie auch für die Zeilen unter dem Fensterrand.
        zeilen_log = []
        for eintrag in treffer[:gezeigt['anzahl']]:
            zustand = eintrag.get('zustand') or missionslog.LAEUFT
            zeile = tk.Frame(liste_rahmen, bg=SURFACE)
            zeilen_log.append(zeile)

            tk.Label(zeile, text=(eintrag.get('wann') or '')[:10],
                     bg=SURFACE, fg=SUB, font=fenster.f_small, width=11,
                     anchor='w', padx=10, pady=7).pack(side='left')
            # ⚠ Breit genug fuer den laengsten Zustand — „nicht mehr offen"
            # hat 16 Zeichen, bei 14 stand dort ein Stumpf.
            # ⚠ Der laufende Zustand heisst je nach Lage anders — siehe
            # `_wort_laufend`. Einmal je Durchlauf gefragt, nicht je Zeile:
            # Das ist ein Dateizugriff, und die Liste hat hunderte Zeilen.
            schluessel = (wort_laufend if zustand == missionslog.LAEUFT
                          else worte.get(zustand, 's_al_laeuft'))
            tk.Label(zeile, text=t(schluessel),
                     bg=SURFACE, fg=farben.get(zustand, SUB),
                     font=fenster.f_small, width=17,
                     anchor='w').pack(side='left')

            mitte = tk.Frame(zeile, bg=SURFACE)
            mitte.pack(side='left', fill='x', expand=True)
            # ⚠ Lange Namen brechen um, statt rechts abgeschnitten zu werden.
            # Das Spiel liefert bis zu 109 Zeichen („Verified Bounty: … | HRT
            # (Großes Mehrbesatzungsschiff, mittlere Unterstützung)").
            name_lab = tk.Label(mitte, text=eintrag.get('name') or '',
                                bg=SURFACE, fg=FG, font=fenster.f_small,
                                anchor='w', justify='left')
            name_lab.pack(fill='x')
            _wrap(name_lab)

            # ⭐ **Der Auftragsname führt zu seinen Bauplänen** (08.09.2026,
            # Drei-Klick-Regel). Das Protokoll sagt bisher nur „diesen Auftrag
            # hast du gespielt" — die Anschlussfrage ist „und was bringt der
            # eigentlich?". Der Klick stellt die Bauplan-Liste auf ihn ein.
            #
            # ⚠ `_to_contract` sieht vorher nach: Kennt kein Bauplan diesen
            # Auftrag als Quelle, wird NICHT gesprungen, sondern gemeldet. Das
            # ist hier der Normalfall — die meisten Aufträge im Protokoll
            # bringen keinen Bauplan.
            def zur_bauplanliste(_ereignis=None, titel=eintrag.get('name') or ''):
                _to_contract(fenster, titel)

            for teil in (name_lab, mitte):
                teil.bind('<Button-1>', zur_bauplanliste)
                try:
                    teil.configure(cursor='hand2')
                except tk.TclError:
                    pass
            name_lab.bind('<Enter>', lambda e, w=name_lab: w.configure(fg=ACCENT))
            name_lab.bind('<Leave>', lambda e, w=name_lab: w.configure(fg=FG))
            from . import notice
            notice.attach(name_lab, lambda: t('s_al_klick'))
            # Der Stand gehoert nur an einen laufenden Auftrag. Bei einem
            # beendeten waere er Ballast — er ist ja fertig.
            if (zustand == missionslog.LAEUFT
                    and eintrag.get('ziele_gesamt')):
                tk.Label(mitte,
                         text=t('s_al_ziele', eintrag.get('ziele_fertig') or 0,
                                eintrag['ziele_gesamt']),
                         bg=SURFACE, fg=SUB, font=fenster.f_small,
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
                    bg=SURFACE, fg=ACCENT, font=fenster.f_small,
                    anchor='w', justify='left')
                bp_lab.pack(fill='x')
                # Bei mehreren Funden wird diese Zeile laenger als der Name.
                _wrap(bp_lab)

        _pack_on_demand(innen.canvas, zeilen_log)

        # ⭐ **Was noch fehlt, steht darunter — und lädt auf Klick nach.**
        # Wortlaut und Verhalten wie in der Bauplan-Liste: gleiche Dinge an der
        # gleichen Stelle.
        rest = len(treffer) - gezeigt['anzahl']
        if rest > 0:
            def mehr_zeigen(_ereignis=None):
                gezeigt['anzahl'] += LOG_ROWS_FIRST
                zeichnen(mehr=True)

            mehr = tk.Label(liste_rahmen, text=t('weitere_anzeigen', rest),
                            bg=BG, fg=ACCENT, font=fenster.f_small,
                            cursor='hand2', pady=10)
            mehr.pack(fill='x')
            mehr.bind('<Button-1>', mehr_zeigen)

        # ⭐ **Wer zeichnet, schreibt den Abdruck** — am Ende, nach dem Aufbau.
        # Bricht etwas ab, bleibt er auf `None` und die Liste entsteht beim
        # nächsten Anzeigen neu.
        zuletzt['stand'] = _anzeige_stand()

        # ⚠ **„Mehrfach gespielt" ist am 07.09.2026 entfernt worden.** Der
        # Block zählte unter der Liste auf, welcher Auftrag wie oft lief.
        # Begründung: „schaut sich niemand an und bringt einem eh keinen
        # Mehrwert." Er kostete bei jedem Zeichnen einen Durchlauf über alle
        # Einträge plus zwei Bedienelemente je Wiederholung — auf einer Seite,
        # die ohnehin die teuerste im Programm war.
        #
        # `missionslog.zusammenfassen()` bleibt bestehen: Die Funktion ist
        # geprüft und harmlos, sie wird hier nur nicht mehr angezeigt.

    # ⚠⚠ **Die Filterknoepfe tragen die Farbe ihres Zustands** (06.09.2026):
    # Wer „abgebrochen" sucht, drueckt einen Knopf im selben blassen Rot, in
    # dem die Zeilen danach dastehen. Ohne diese Kopplung waeren es sechs
    # gleich aussehende Knoepfe, und die Farben in der Liste haetten keine
    # Entsprechung in der Bedienung.
    #
    # ⚠ Die Reihenfolge folgt dem Ausgang, nicht dem Alphabet: erst alles,
    # dann was laeuft, dann was geschafft ist, dann was schiefging, zuletzt
    # das Ungewisse.
    chips = {}
    schalter = [('alle', 's_al_f_alle', ACCENT),
                (missionslog.LAEUFT, 's_al_laeuft', GOLD),
                (missionslog.ABGESCHLOSSEN, 's_al_fertig', ACCENT),
                (missionslog.ABGEBROCHEN, 's_al_abbruch', RED_PALE),
                (missionslog.FEHLGESCHLAGEN, 's_al_fehl', RED_PALE),
                (missionslog.VERFALLEN, 's_al_verfallen', SUB)]

    def waehlen(art):
        stand['art'] = art
        for kennung, k in chips.items():
            k.setzen(kennung == art)
        zeichnen()

    for kennung, schluessel, farbe in schalter:
        knopf = _chip(fenster, filterleiste, t(schluessel),
                      kennung == 'alle', farbe)
        knopf.pack(side='left', padx=(0, 6))
        knopf.bind('<Button-1>', lambda ev, s=kennung: waehlen(s))
        chips[kennung] = knopf

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
        # ⚠⚠⚠ **Im Hintergrund, nicht im Seitenaufbau.** Der Kommentar oben
        # nennt 20 ms — das gilt, wenn der Lesestand gefüllt ist und nur die
        # laufende `Game.log` neu gelesen wird. Beim **ersten** Mal ist er
        # leer, und dann werden alle Sicherungen durchgegangen: Beim
        # Abnahme-Durchlauf am 06.09.2026 gemessen **9.003 ms** für diese eine
        # Seite. Neun Sekunden, in denen das Fenster steht und auf keinen
        # Klick reagiert — genau das, was in den Startverläufen als lange
        # Ladezeit auffiel.
        #
        # Gezeigt wird sofort der gespeicherte Stand; was dazukommt, kommt
        # nach. Ein Protokoll, das eine Sekunde später vollständig wird, ist
        # besser als eines, für das man neun Sekunden wartet.
        zeichnen(neu_laden=True)

        def nachlesen():
            try:
                missionslog.nachlese()
            except Exception as ausnahme:
                # Ein fehlgeschlagenes Nachlesen darf die Seite nicht leer
                # lassen — der gespeicherte Stand ist besser als nichts.
                fehler.merken('seiten.auftragslog_nachlese', ausnahme)
                return
            # ⚠ Zurück in den Oberflächen-Faden; und nur zeichnen, wenn es die
            # Seite noch gibt.
            try:
                if rahmen.winfo_exists():
                    rahmen.after(0, lambda: zeichnen(neu_laden=True))
            except Exception:
                pass

        threading.Thread(target=nachlesen, daemon=True).start()

    # ⚠ Bei jedem Öffnen frisch laden — siehe oben. Ohne das bleibt eine Seite,
    # die während der Nachlese gebaut wurde, für immer leer.
    fenster.on_show['auftragslog'] = _auffrischen

    # ⚠⚠ **Und beim ERSTEN Öffnen auch.** `on_show` feuert nur, wenn die
    # Seite bereits gebaut war (`if kennung in self.gezeichnet`) — beim ersten
    # Besuch also nicht. Wer den Watcher morgens startet, mittags einen Auftrag
    # abgibt und dann zum ersten Mal hierher wechselt, sah den Stand vom
    # Programmstart. Am 05.09.2026 gemeldet von **Bushwick4712**: „mission log
    # updated nicht."
    #
    # ⚠ Der Aufruf steht am Ende, nachdem alles gebaut ist — vorher gäbe es
    # nichts zu zeichnen.
    _auffrischen()


def _device_hub(fenster, eltern):
    """Alle Eingabegeräte an einem Ort — mit laufender Überwachung.

    ## Warum das oben auf der Steuerungsseite steht

    Die Seite darunter beantwortet „was liegt auf welcher Taste". Davor steht
    aber eine Frage, die sonst niemand beantwortet: **welches Gerät ist
    überhaupt welches?** Über einen Stick gibt es drei Aussagen — was das
    System sieht, was das Spiel zuletzt sah, und was in der Belegung steht —
    und ihre Nummern stimmen nicht überein. Gemessen am 06.09.2026: Derselbe
    Stick war am System `js0` und im Spiel `js2`.

    ## ⚠ Die Überwachung fragt ab, sie horcht nicht

    Alle drei Sekunden wird die Geräteliste gelesen und mit der vorigen
    verglichen — kein Systemdienst, kein Fremdpaket, auf beiden Systemen
    derselbe Weg. Das kostet unter Linux ein `listdir`.

    ⚠ Der Takt haengt am Fenster (`after`), nicht an einem Faden: Wird die
    Seite zerstört, hört er von selbst auf. Ein Faden müsste dafür eigens
    beendet werden, und genau das wird beim nächsten Umbau vergessen.
    """
    from . import device_hub

    # ⚠⚠ **Kein `messagebox`** — der Dialog des Betriebssystems ist ein
    # weißer Kasten mit grauen Knöpfen mitten im dunklen Fenster. Dieselbe
    # Kapsel wie auf den Achsen- und Blickwinkel-Seiten; `frage_stellen()`
    # gibt es im Projekt genau dafür.
    class messagebox:
        """Dieselben Aufrufe wie `tkinter.messagebox`, nur im Programmstil."""

        @staticmethod
        def showinfo(titel, text):
            from .main_window import ask_yes_no
            ask_yes_no(eltern.winfo_toplevel(), titel, text, only_ok=True)

        @staticmethod
        def showwarning(titel, text):
            from .main_window import ask_yes_no
            ask_yes_no(eltern.winfo_toplevel(), titel, text, only_ok=True)

        @staticmethod
        def askyesno(titel, text):
            from .main_window import ask_yes_no
            return ask_yes_no(eltern.winfo_toplevel(), titel, text)

    kopf = tk.Label(eltern, text=t('s_gh_titel'), bg=BG, fg=FG,
                    font=fenster.f_bold, anchor='w')
    kopf.pack(fill='x', pady=(6, 0))
    _body_text(eltern, t('s_gh_lead'), fenster.f_small, fill='x')

    tafel = tk.Frame(eltern, bg=BG)
    tafel.pack(fill='x', pady=(8, 0))
    meldung = tk.Label(eltern, text='', bg=BG, fg=GOLD,
                       font=fenster.f_small, anchor='w')
    meldung.pack(fill='x')

    wache = device_hub.Watchdog()
    farben = {device_hub.READY: ACCENT, device_hub.NO_NUMBER: GOLD,
              device_hub.UNPLUGGED: RED, device_hub.UNKNOWN: GOLD}

    def _zeichnen():
        for kind in list(tafel.winfo_children()):
            kind.destroy()
        ueberblick = device_hub.summary()
        if not ueberblick['geraete']:
            tk.Label(tafel, text=t('s_gh_kein_geraet'), bg=BG, fg=SUB,
                     font=fenster.f_small, anchor='w').pack(fill='x')
            return

        for geraet in ueberblick['geraete']:
            zeile = tk.Frame(tafel, bg=SURFACE)
            zeile.pack(fill='x', pady=(0, 2))
            streifen = tk.Frame(zeile, bg=farben.get(geraet['zustand'], SUB),
                                width=3)
            streifen.pack(side='left', fill='y')

            # ⚠ Die Spiel-Nummer zuerst und in der Markenfarbe: Das ist die
            # Zahl, mit der der Spieler es im Spiel und in Anleitungen zu tun
            # hat. Der Systempfad steht daneben, damit man beide auseinander
            # halten kann — genau daran scheitern sonst alle Anleitungen.
            nummer = ('js%d' % geraet['nummer']) if geraet['nummer'] else '—'
            tk.Label(zeile, text=nummer, bg=SURFACE,
                     fg=ACCENT if geraet['nummer'] else SUB,
                     font=fenster.f_bold, width=5,
                     anchor='w', padx=8).pack(side='left', pady=6)
            tk.Label(zeile, text=geraet['name'] or geraet['kurz'], bg=SURFACE,
                     fg=FG, font=fenster.f_small,
                     anchor='w').pack(side='left', fill='x', expand=True)
            tk.Label(zeile, text=geraet['systempfad'] or '—', bg=SURFACE,
                     fg=SUB, font=fenster.f_small,
                     anchor='e', padx=10).pack(side='right')
            tk.Label(zeile, text=t('s_gh_' + geraet['zustand']), bg=SURFACE,
                     fg=farben.get(geraet['zustand'], SUB),
                     font=fenster.f_small, anchor='e',
                     padx=10).pack(side='right')

        # Ein Satz zur Lage — und nur dann einer, wenn er etwas sagt.
        if ueberblick['alles_gut']:
            tk.Label(tafel, text=t('s_gh_alles_gut'), bg=BG, fg=ACCENT,
                     font=fenster.f_small, anchor='w').pack(fill='x',
                                                            pady=(6, 0))
            return

        # ⭐ **Der Assistent: nicht was los ist, sondern was zu tun ist.**
        #
        # Ein Zustand allein hilft niemandem — „ohne Nummer" sagt nicht, wie
        # man zu einer kommt. Der häufigste Fall sieht sogar nach zwei
        # Problemen aus und ist eines: Ein Stick mit neuer Kennung steht
        # zweimal da, einmal als fehlend und einmal als unbekannt. Ein
        # Handgriff behebt das.
        tk.Label(tafel, text=t('s_gh_was_tun'), bg=BG, fg=FG,
                 font=fenster.f_bold, anchor='w').pack(fill='x', pady=(12, 0))

        for vorschlag in device_hub.suggestions():
            _vorschlag_zeigen(vorschlag)

    def _vorschlag_zeigen(vorschlag):
        """Ein Schritt, in einem Satz — und wo möglich mit Knopf."""
        from . import device_hub

        geraet = vorschlag['geraet']
        kasten = tk.Frame(tafel, bg=SURFACE)
        kasten.pack(fill='x', pady=(6, 0))

        if vorschlag['art'] == device_hub.SWAP:
            alt = vorschlag['alt']
            tk.Label(kasten,
                     text=t('s_gh_tausch').format(
                         geraet['name'] or geraet['kurz'],
                         alt['name'] or alt['kurz']),
                     bg=SURFACE, fg=ACCENT, font=fenster.f_bold, anchor='w',
                     padx=12).pack(fill='x', pady=(10, 0))
            _body_text(kasten, t('s_gh_tausch_lang'), fenster.f_small,
                        bg=SURFACE, fill='x', inset=24)

            def _umhaengen(neu=geraet, vorher=alt):
                from . import joysticks
                zeilen = len(joysticks.belegungen() or {})
                if not _ask(fenster, 
                        t('hf_joysticks'),
                        t('s_gh_tausch_frage').format(
                            vorher['name'] or vorher['kurz'],
                            neu['name'] or neu['kurz'], zeilen)
                        + '\n\n' + t('s_ac_spiel_zu')):
                    return
                erfolg, meldung, _ = device_hub.reassign(
                    vorher['kennung'], neu['kennung'])
                if not erfolg:
                    _notice(fenster, t('hf_joysticks'), t(meldung))
                    return
                _notice(fenster, t('hf_joysticks'), t('s_gh_umgehaengt'))
                _zeichnen()

            # ⚠ Der Knopf steht UNTER dem Kasten: `_knopf` zeichnet fest auf
            # `BG`, in einem `FLAECHE`-Kasten wäre das ein Farbklotz.
            tk.Frame(kasten, bg=SURFACE, height=10).pack(fill='x')
            reihe = tk.Frame(tafel, bg=BG)
            reihe.pack(fill='x', pady=(6, 0))
            _button(fenster, reihe, t('s_gh_tausch_knopf'), _umhaengen,
                   strong=True).pack(side='left')
            return

        # Die beiden Fälle ohne Knopf: Da hilft kein Schreibvorgang, sondern
        # ein Handgriff am Rechner. Ein Knopf, der nur einen Rat wiederholt,
        # wäre eine Attrappe.
        rat = (t('s_gh_starten_rat') if vorschlag['art'] == device_hub.START
               else t('s_gh_anstecken_rat'))
        _body_text(kasten, rat.format(geraet['name'] or geraet['kurz']),
                    fenster.f_small, bg=SURFACE, fill='x', inset=24)
        tk.Frame(kasten, bg=SURFACE, height=8).pack(fill='x')

    def _takt():
        # ⚠ `winfo_exists()` prüfen: Der Takt läuft weiter, während die Seite
        # längst zerstört sein kann (Sprachwechsel, Neuaufbau). Ohne die
        # Prüfung wirft der nächste Zugriff eine Ausnahme in einem Rückruf —
        # und die sieht in einer `.exe` niemand.
        if not tafel.winfo_exists():
            return
        dazu, weg = wache.check()
        if dazu or weg:
            namen = [g.get('name', '?') for g in (dazu or weg)]
            meldung.configure(
                text=(t('s_gh_neu') if dazu else t('s_gh_weg')).format(
                    ', '.join(namen)))
            _zeichnen()
        tafel.after(3000, _takt)

    wache.check()          # Grundlage setzen, ohne zu melden
    _zeichnen()
    tafel.after(3000, _takt)
    return _zeichnen


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

    _heading(fenster, rahmen, t('hf_joysticks'), t('s_js_lead'))
    innen = _scroll_area(rahmen)
    _body_text(innen, t('s_js_hinweis'), fenster.f_small, fill='x')

    # ⭐ Der Geräte-Hub steht GANZ OBEN, vor allem anderen. Bevor jemand
    # fragt „was liegt auf welcher Taste", muss klar sein, welches Gerät
    # überhaupt welches ist — und dass System und Spiel verschieden zählen.
    _device_hub(fenster, innen)

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
                         font=fenster.f_bold, anchor='w')
    werkzeugleiste = tk.Frame(unten, bg=BG)
    # Eigene Zeile nur für das Zurücksetzen — siehe `werkzeug_zeichnen`.
    gefahrleiste = tk.Frame(unten, bg=BG)
    sicht_rahmen = tk.Frame(unten, bg=BG)
    filter_rahmen = tk.Frame(unten, bg=BG)
    werkzeug = tk.Frame(unten, bg=BG)
    zaehler = tk.Label(unten, text='', bg=BG, fg=SUB, font=fenster.f_small,
                       anchor='w')
    liste_rahmen = tk.Frame(unten, bg=BG)

    from .main_window import round_entry
    feld = round_entry(werkzeug, suche, fenster.f_small, '#0c1017', LINE,
                       ACCENT, FG, placeholder=t('s_pl_belegung'))
    feld.holder.pack(fill='x')

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
                     font=fenster.f_small, anchor='w', justify='left',
                     wraplength=560).pack(fill='x', pady=8)
            return

        # --- Zustandszeile: die eine Aussage, wegen der man hier nachsieht ---
        farbe = {joysticks.PASST: ACCENT, joysticks.ERSETZT: GOLD,
                 joysticks.FEHLT: RED}.get(zustand, SUB)
        if zustand == joysticks.PASST:
            satz = t('s_js_passt')
        elif zustand == joysticks.ERSETZT:
            satz = t('s_js_ersetzt')
        elif zustand == joysticks.FEHLT:
            satz = t('s_js_fehlt', len(v.get('fehlende') or []))
        else:
            satz = t('s_js_keine_datei')
        tk.Label(oben, text=satz, bg=BG, fg=farbe, font=fenster.f_bold,
                 anchor='w', justify='left', wraplength=560).pack(fill='x')

        # --- Der eine reparierbare Fall: Gerät unter neuer Kennung ---
        if zustand == joysticks.ERSETZT and v.get('ersatz'):
            alt, neu = v['ersatz'][0]
            tk.Label(oben, text=t('s_js_ersatz_frage', alt['name'],
                                  neu['name']),
                     bg=BG, fg=SUB, font=fenster.f_small, anchor='w',
                     justify='left', wraplength=560).pack(fill='x', pady=(6, 0))
            tk.Label(oben, text=t('s_js_spiel_zu'), bg=BG, fg=GOLD,
                     font=fenster.f_small, anchor='w', justify='left',
                     wraplength=560).pack(fill='x', pady=(4, 0))
            _button(fenster, oben, t('s_js_uebernehmen'),
                   lambda: _uebernehmen(alt, neu), strong=True).pack(
                       anchor='w', pady=(8, 0))

        # --- Block 1: was verbunden ist ---
        tk.Label(oben, text=t('s_js_geraete'), bg=BG, fg=FG,
                 font=fenster.f_bold, anchor='w').pack(fill='x', pady=(16, 4))
        nach_kennung = {z['kennung']: z for z in belegt if z['kennung']}
        for g in geraete:
            zeile = tk.Frame(oben, bg=SURFACE)
            zeile.pack(fill='x', pady=1)
            tk.Label(zeile, text=t('s_js_platz', g['platz']), bg=SURFACE,
                     fg=SUB, font=fenster.f_small, width=9, anchor='w',
                     padx=10, pady=7).pack(side='left')
            zu = nach_kennung.get(g['kennung'])
            tk.Label(zeile, text=('js%d' % zu['nummer']) if zu
                     else t('s_js_ohne'),
                     bg=SURFACE, fg=ACCENT if zu else GOLD,
                     font=fenster.f_small, width=13, anchor='w').pack(
                         side='left')
            tk.Label(zeile, text=g['name'], bg=SURFACE, fg=FG,
                     font=fenster.f_small, anchor='w').pack(side='left')
            tk.Label(zeile, text=_kennung_kurz(g['kennung']), bg=SURFACE,
                     fg=SUB, font=fenster.f_small, anchor='e',
                     padx=10).pack(side='right')

        # Belegte Geräte, die gerade fehlen — sonst sieht man nur, was da ist.
        for z in (v.get('fehlende') or []):
            zeile = tk.Frame(oben, bg=SURFACE)
            zeile.pack(fill='x', pady=1)
            tk.Label(zeile, text='—', bg=SURFACE, fg=RED,
                     font=fenster.f_small, width=9, anchor='w', padx=10,
                     pady=7).pack(side='left')
            tk.Label(zeile, text='js%d' % z['nummer'], bg=SURFACE, fg=RED,
                     font=fenster.f_small, width=13, anchor='w').pack(
                         side='left')
            tk.Label(zeile, text=z['name'], bg=SURFACE, fg=SUB,
                     font=fenster.f_small, anchor='w').pack(side='left')

    def _zuruecksetzen():
        """Alle eigenen Belegungen verwerfen — mit ausdrücklicher Rückfrage.

        ⚠⚠ Der gefährlichste Knopf auf der Seite: Er wirft weg, woran jemand
        einen Abend gesessen hat. Deshalb nennt die Frage die **Anzahl** der
        betroffenen Belegungen — „alles zurücksetzen?" ist zu abstrakt, um
        eine Entscheidung darauf zu stützen.
        """
        eigene = 0
        for liste in (joysticks.sicht(joysticks.MEINE) or {}).values():
            eigene += len(liste)
        if not _ask(fenster, t('s_js_zurueck'),
                                   t('s_js_zurueck_frage', eigene)):
            return
        erfolg, meldung, _ = joysticks.zuruecksetzen()
        if erfolg:
            _notice(fenster, t('hf_joysticks'),
                                t('s_js_zurueck_ok', meldung))
        else:
            _notice(fenster, t('hf_joysticks'),
                                   t('s_js_schief', t(meldung)))
        _auffrischen()

    def _ausgeben(als_csv=False):
        """Die Belegung als Datei sichern — ohne Umweg über die Spielkonsole."""
        from . import file_picker
        from .sprache import aktuelle
        endung = '.csv' if als_csv else '.xml'
        ziel = file_picker.save_file(
            t('s_js_ausgeben'),
            suggestion='actionmaps' + endung, extension=endung)
        if not ziel:
            return
        erfolg, meldung = joysticks.ausgeben(ziel, aktuelle())
        if erfolg:
            _notice(fenster, t('hf_joysticks'),
                                t('s_js_ausgabe_ok', meldung))
        else:
            _notice(fenster, t('hf_joysticks'),
                                   t('s_js_schief', t(meldung)))

    def _profil_speichern():
        """Die Belegung als Profil ablegen, das Star Citizen selbst kennt.

        ⚠ Der Unterschied zu „Belegung sichern": Eine Sicherung ist eine Datei
        irgendwo, ein Profil liegt **im Mappings-Ordner des Spiels** und lässt
        sich dort unter seinem Namen laden. Das ist der Weg, den man sonst nur
        über die Spielkonsole hat.
        """
        from .main_window import ask_text
        vorhandene = joysticks.profile()
        # ⚠ **Nicht `simpledialog.askstring`.** Der Systemdialog kommt grau, in
        # der Systemschrift und mit englischem „Cancel" — auf dem dunklen Grund
        # ein Fremdkörper. `text_stellen()` ist derselbe Dialog im Programmstil.
        name = ask_text(
            fenster.root, t('s_js_profil'), t('s_js_profil_frage'),
            choices=vorhandene,
            choices_title=t('s_js_profil_liste') if vorhandene else '')
        if name is None:
            return                       # abgebrochen, nicht leer bestätigt
        ok, meldung = joysticks.name_pruefen(name)
        if not ok:
            _notice(fenster, t('hf_joysticks'), t(meldung))
            return
        name = name.strip()
        erfolg, meldung = joysticks.profil_speichern(name)
        # Ein vorhandenes Profil wird nicht stillschweigend überschrieben —
        # dahinter kann die Belegung eines ganzen Abends stecken.
        if not erfolg and meldung == 's_js_f_name_belegt':
            if not _ask(fenster, t('s_js_profil'),
                                       t('s_js_profil_ersetzen', name)):
                return
            erfolg, meldung = joysticks.profil_speichern(
                name, ueberschreiben=True)
        if erfolg:
            _notice(fenster, t('hf_joysticks'),
                                t('s_js_profil_ok', name, name))
        else:
            _notice(fenster, t('hf_joysticks'), t(meldung))

    def _einlesen():
        from . import file_picker
        from .main_window import ask_pick, ask_choice
        # ⚠ Erst die eigenen Profile anbieten, dann den Dateiwähler. Der
        # Spieler kennt seine Belegung am **Namen**, nicht am Pfad — und der
        # Mappings-Ordner liegt auf jedem Rechner woanders. Wer eine
        # zugeschickte Datei hat, nimmt weiter den zweiten Weg.
        quelle = None
        vorhandene = joysticks.profile()
        if vorhandene:
            wahl = ask_choice(fenster.root, t('s_js_einlesen'),
                                t('s_js_einlesen_woher'),
                                t('s_js_einlesen_profil'),
                                t('s_js_einlesen_datei'))
            if wahl == 'a':
                name = ask_pick(fenster.root, t('s_js_einlesen'),
                                       t('s_js_einlesen_waehlen'), vorhandene)
                if not name:
                    return
                quelle = joysticks.profil_datei(name)
                if not quelle:
                    _notice(fenster, t('hf_joysticks'),
                                           t('s_js_f_datei'))
                    return
            elif wahl != 'b':
                return                   # abgebrochen
        if not quelle:
            quelle = file_picker.open_file(t('s_js_einlesen'),
                                           patterns=(('XML', '*.xml'),))
        if not quelle:
            return
        if not _ask(fenster, t('s_js_einlesen'),
                                   t('s_js_einlesen_frage')):
            return
        erfolg, meldung, anzahl = joysticks.einlesen(quelle)
        if erfolg:
            _notice(fenster, t('hf_joysticks'),
                                t('s_js_einlesen_ok', anzahl, meldung))
        else:
            _notice(fenster, t('hf_joysticks'),
                                   t('s_js_schief', t(meldung)))
        _auffrischen()

    def werkzeug_zeichnen():
        """Sichern, Einspielen, Zurücksetzen — einmal gebaut, bleibt stehen."""
        for kind in werkzeugleiste.winfo_children():
            kind.destroy()
        for kind in gefahrleiste.winfo_children():
            kind.destroy()
        # ⚠ Steht **vor** „Belegung sichern": Das Profil ist der Weg, den die
        # meisten wollen — es liegt im Spiel und ist dort ladbar. Die Sicherung
        # als lose Datei ist der Sonderfall (weitergeben, aufheben).
        _button_grid(werkzeugleiste, [
            _button(fenster, werkzeugleiste, t('s_js_profil'),
                   _profil_speichern),
            _button(fenster, werkzeugleiste, t('s_js_ausgeben'),
                   lambda: _ausgeben(False)),
            _button(fenster, werkzeugleiste, t('s_js_ausgeben_csv'),
                   lambda: _ausgeben(True)),
            _button(fenster, werkzeugleiste, t('s_js_einlesen'), _einlesen),
        ])
        # ⚠⚠ **Der gefährliche Knopf steht allein, in einer eigenen Zeile.**
        # Am 05.09.2026 gemeldet: „Wird abgeschnitten. Willst du den Knopf
        # nicht besser platzieren, wo er nicht abgeschnitten wird und nicht
        # aus Versehen gedrückt wird?" Beides richtig — er stand als fünfter
        # in einer Reihe, die nicht mehr hinpasste, direkt neben vier
        # harmlosen. Ein Knopf, der die ganze Belegung wegwirft, gehört nicht
        # dorthin, wo die Hand ohnehin gerade ist.
        #
        # `gefahr=True` färbt ihn dauerhaft rot, nicht erst beim Überfahren —
        # ein Knopf, der erst warnt, wenn die Maus schon darauf steht, warnt
        # niemanden.
        _button(fenster, gefahrleiste, t('s_js_zurueck'), _zuruecksetzen,
               danger=True).pack(side='right')

    def sicht_zeichnen():
        """Die drei Sichten: was ich geändert habe · alles · Werkseinstellung.

        Dieselbe Einteilung, die das Spiel in seinen Optionen benutzt — und
        die Antwort auf zwei verschiedene Fragen: „was habe ich umgestellt"
        und „was tut diese Taste eigentlich".
        """
        for kind in sicht_rahmen.winfo_children():
            kind.destroy()
        beschriftung = tk.Label(sicht_rahmen, text=t('s_js_sicht'), bg=BG,
                                fg=SUB, font=fenster.f_small, anchor='w')

        def _waehlen(welche):
            nur['sicht'] = welche
            nur['geraet'] = ''         # die Geräte unterscheiden sich je Sicht
            _laden()
            sicht_zeichnen()
            filter_zeichnen()
            liste_zeichnen()

        knoepfe = [beschriftung]
        for schluessel, welche in ((('s_js_s_meine'), joysticks.MEINE),
                                   ('s_js_s_alles', joysticks.ALLES),
                                   ('s_js_s_standard', joysticks.STANDARD),
                                   # ⭐ Ohne diese Sicht käme man an 411
                                   # Aktionen gar nicht heran: Was nirgends
                                   # belegt ist, steht in keiner Liste — und
                                   # was in keiner Liste steht, kann man auch
                                   # nicht anklicken, um es zu belegen.
                                   ('s_js_s_frei', joysticks.FREI)):
            knoepfe.append(_button(fenster, sicht_rahmen, t(schluessel),
                                  (lambda w=welche: _waehlen(w)),
                                  strong=(nur['sicht'] == welche)))
        _button_grid(sicht_rahmen, knoepfe)

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
        # ⚠ Umbrechend statt abgeschnitten — bei sieben Geräten passt die
        # Reihe in ein kleiner gezogenes Fenster sonst nicht mehr, und Tk
        # schneidet den Rest wortlos ab (Maus, Tastatur, Gamepad waren weg).
        _button_grid(filter_rahmen,
                     [_button(fenster, filter_rahmen, text,
                             (lambda k=kennzeichen: _waehlen(k)),
                             strong=(nur['geraet'] == kennzeichen))
                      for text, kennzeichen in knoepfe])

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
                     font=fenster.f_small, anchor='w').pack(fill='x', pady=8)
            return
        # ⚠ Dieselbe Grenze wie in der Bauplan-Liste: Wer alle Geräte auf
        # einmal zeigt, hat schnell dreihundert Zeilen und wartet beim Öffnen.
        # ⭐⭐ **Gebaut werden alle, gepackt nur die sichtbaren.**
        # Tk rechnet die Geometrie für jedes gepackte Kind — auch für die 170
        # Zeilen unter dem Fensterrand. Gemessen: 635 ms gegen 192 ms beim
        # Wechsel auf diese Seite. Siehe `_pack_on_demand`.
        gepackt = []
        for kennzeichen, e, klar, lesbar, echt in gezeigt[:200]:
            zeile = tk.Frame(liste_rahmen, bg=SURFACE)
            gepackt.append(zeile)

            # ⚠ Die Bindung muss auf **jedes** Kind gelegt werden, nicht nur
            # auf den Rahmen: Ein Klick landet auf der Beschriftung, die
            # darüberliegt, und der Rahmen bekommt ihn nie zu sehen.
            def _klick(_ereignis=None, kz=kennzeichen, eintrag=e, name=klar):
                _neu_belegen(kz, eintrag, name)

            def _anfassen(kind):
                kind.bind('<Button-1>', _klick)
                kind.configure(cursor='hand2')

            _anfassen(zeile)
            tk.Label(zeile, text=_geraetename(kennzeichen), bg=SURFACE,
                     fg=SUB, font=fenster.f_small, width=21, anchor='w',
                     padx=10, pady=6).pack(side='left')
            tk.Label(zeile, text=(lesbar or t('s_js_ohne_eingabe')),
                     bg=SURFACE, fg=(ACCENT if lesbar else SUB),
                     font=fenster.f_small, width=17, anchor='w').pack(
                         side='left')
            # ⚠⚠ **Die Marke rechts wird ZUERST gepackt.** In `tkinter`
            # bekommt das zuerst gepackte Element seinen Platz; ein langer
            # Aktionsname mit `side='left'` schiebt eine später gepackte
            # `side='right'`-Beschriftung aus dem Fenster. Beim
            # Abnahme-Durchlauf am 06.09.2026 gemessen: „geändert" brauchte
            # 82 px und bekam 43 — es stand also „geänd…" da, bei manchen
            # Zeilen nur 6 px.
            #
            # Derselbe Fehler steckte am selben Tag im Warenkorb, wo er den
            # Knopf „Kaufen" unerreichbar machte. Die Regel steht seit
            # Langem in den Projektnotizen — sie greift nur, wenn man beim
            # Schreiben daran denkt, deshalb prüft die Abnahme jetzt die
            # Textbreiten.
            if e.get('quelle') == joysticks.MEINE:
                tk.Label(zeile, text=t('s_js_q_meine'), bg=SURFACE, fg=ACCENT,
                         font=fenster.f_small, anchor='e', padx=10).pack(
                             side='right')
            # ⚠ Grau heißt „das ist keine Bezeichnung des Spiels, sondern der
            # aufbereitete technische Name" — 382 Aktionen haben keine.
            #
            # ⚠ `expand=True`: Der Name darf schrumpfen, die Marke daneben
            # nicht.
            #
            # ⭐ **Was nicht passt, wird UMGEBROCHEN statt abgeschnitten**
            # (14.09.2026). Hier stand vorher, das Abschneiden sei Absicht —
            # „der Name steht immerhin am Anfang lesbar da". Bei „sehr groß"
            # brauchte ein Aktionsname 446 px und bekam 296: Ein Drittel
            # fehlte. Und wer diese Schriftstufe wählt, tut das, **weil** er
            # lesen können will. Eine Zeile mehr kostet nichts, ein
            # abgeschnittener Name kostet die Auskunft.
            _name_lbl = tk.Label(zeile, text=(klar or e['aktion']),
                                 bg=SURFACE,
                                 fg=(FG if echt else SUB),
                                 font=fenster.f_small, anchor='w')
            _name_lbl.pack(side='left', fill='x', expand=True)
            _wrap_self(_name_lbl)
            for kind in zeile.winfo_children():
                _anfassen(kind)

        # ⭐ Jetzt erst packen — und nur so viele, wie hineinpassen.
        _pack_on_demand(innen.canvas, gepackt)

    def _uebernehmen(alt, neu):
        erfolg, meldung, _ = joysticks.kennung_tauschen(alt['kennung'],
                                                        neu['kennung'],
                                                        neu['name'])
        if erfolg:
            _notice(fenster, t('hf_joysticks'), t('s_js_fertig', meldung))
        else:
            # ⚠ `meldung` ist ein Sprachschlüssel, kein fertiger Satz — sonst
            # stünde in der englischen Oberfläche deutscher Text.
            _notice(fenster, t('hf_joysticks'),
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
        from .binding_window import BindingWindow
        bereich = (eintrag.get('bereich')
                   or joysticks.gruppe_von(eintrag['aktion'])
                   or _bereich_suchen(eintrag['aktion']))
        if kennzeichen == joysticks.FREI:
            # Eine unbelegte Aktion gehört noch zu keinem Gerät — welches es
            # wird, entscheidet der Knopf, den der Spieler gleich drückt.
            kennzeichen = ''
        try:
            # ⛔ Rückstand der Sprachumstellung, wie bei `calibrate()` weiter
            # unten: `BindingWindow` heißt seine Parameter `plain_name`,
            # `previous`, `done`. Mit den alten deutschen Namen warf der
            # Aufruf `TypeError` — das Fenster ging schlicht nicht auf.
            BindingWindow(fenster.root, eintrag['aktion'], bereich,
                          kennzeichen, plain_name=klar,
                          previous=eintrag.get('eingabe', ''),
                          done=_auffrischen)
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

    # ⚠⚠ Der Fingerabdruck der zuletzt gezeichneten Lage — siehe unten.
    zuletzt = {'stand': None}

    def _auffrischen(erzwingen=False):
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
            #
            # ⚠⚠ **Hier steht bewusst KEIN `vergessen()` mehr.**
            #
            # Die Geschichte dazu in drei Schritten, weil sie lehrreich ist:
            #
            # 1. Es stand hier und lief bei JEDEM Anzeigen — gemessen 96 ms.
            # 2. Ich entfernte es, maß 894 statt 886 ms und schloss daraus
            #    „wirkungslos". ⛔ Diese **Einzelmessung war zu verrauscht**:
            #    Die Zahl schwankt zwischen 978 und 3966 ms.
            # 3. Dann band ich es an den Sprachwechsel. Auch das war falsch —
            #    der Prüfer zeigte, dass die Namen aus **Dateien im
            #    Spielordner** kommen. Wer den Ordner umstellt oder das Spiel
            #    aktualisiert, bekam weiter die alten Namen.
            #
            # ⭐ Die Gültigkeit gehört dorthin, wo die Daten herkommen:
            # `joysticks.klarnamen()` schlüsselt seinen Merker jetzt selbst
            # nach Sprache **und** Zustand der Quelldateien. Diese Seite muss
            # gar nichts mehr darüber wissen — und bekommt trotzdem immer den
            # richtigen Stand.
            daten['namen'] = joysticks.klarnamen(aktuelle())
        except Exception as ausnahme:
            fehler.merken('seiten.joysticks_namen', ausnahme)
            daten['namen'] = {}
        _laden()

        # ⭐⭐ **Nichts neu zeichnen, wenn sich nichts geändert hat.**
        #
        # Dieser Rückruf läuft bei JEDEM Anzeigen der Seite — auch wenn man nur
        # kurz woanders war. Gemessen am 12.09.2026: **894 ms je Klick**, davon
        # 92 % in `liste_zeichnen`. Gemeldet als „wirkt lahm … als hätte ein
        # Anfänger das gebaut".
        #
        # ⚠⚠ **Der Vergleich steht HIER — nach `_laden()`, nicht davor.**
        # Die erste Fassung verglich nur `vergleich()`, und das enthält Geräte,
        # Zuordnung und Dateipfad, **nicht die Belegungen**. Die kommen erst
        # über `_laden()` → `joysticks.sicht()`. Der Prüfer hat es nachgestellt:
        # `js1_x` auf `js1_y` umlegen — `vergleich()` bleibt gleich, die
        # Belegung ändert sich, und die Seite hätte den alten Stand gezeigt.
        # Importierte und im Werkzeug neu gesetzte Belegungen wären unsichtbar
        # geblieben.
        #
        # ⭐ Die Lehre daraus: Ein Fingerabdruck muss aus **genau den Daten**
        # bestehen, aus denen gezeichnet wird — nicht aus denen, die man
        # zuerst zur Hand hat. Deshalb stehen jetzt alle vier Quellen drin,
        # die `kopf_zeichnen()` und `liste_zeichnen()` benutzen.
        #
        # ⚠ `erzwingen=True` für Aufrufer, die selbst etwas geändert haben.
        stand = (aktuelle(),
                 repr(daten.get('vergleich')),
                 repr(daten.get('belegungen')),
                 repr(daten.get('namen')),
                 repr(daten.get('geraetenamen')),
                 repr(nur.get('sicht')))
        if not erzwingen and stand == zuletzt['stand']:
            return
        zuletzt['stand'] = stand

        kopf_zeichnen()
        # Die feste Hülle erst zeigen, wenn es wirklich Belegungen gibt —
        # ein Suchfeld über einer leeren Liste ist nur Ballast.
        if daten.get('belegungen') or nur['sicht'] != joysticks.ALLES:
            kopfzeile.pack(fill='x', pady=(16, 4))
            werkzeugleiste.pack(fill='x', pady=(0, 2))
            gefahrleiste.pack(fill='x', pady=(0, 10))
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
    fenster.on_show['joysticks'] = _auffrischen


def _whats_new(fenster, rahmen):
    """Die Änderungen — als Reiter, nicht als Fenster über dem Fenster.

    Zwei Dinge halten die Seite kurz, auch wenn zwanzig Versionen zusammenkommen:
    Nur die **neueste** ist aufgeklappt, und ein Filter zeigt bei Bedarf nur
    Behobenes. Wer einen Fehler gemeldet hat, sucht genau danach.
    """
    _heading(fenster, rahmen, t('hf_wasistneu'), t('s_wn_lead'))

    from . import updater
    try:
        # ⚠ Gebündelt: v3.13.0 bis .3 stehen als **eine** Reihe „v3.13" da.
        # Siehe `updater.protokoll_gebuendelt`.
        eintraege = updater.history_grouped()
    except Exception as ausnahme:
        fehler.merken('seiten.wasistneu', ausnahme)
        eintraege = []

    stand = {'art': 'alle'}
    chips = {}
    leiste = tk.Frame(rahmen, bg=BG)
    leiste.pack(fill='x', pady=(0, 10))
    innen = _scroll_area(rahmen)
    behaelter = tk.Frame(innen, bg=BG)
    behaelter.pack(fill='both', expand=True)

    def zeichnen():
        for kind in behaelter.winfo_children():
            kind.destroy()
        gezeigt = 0
        # ⚠ 25 statt 15, seit die Fassungen gebündelt sind: Eine laufende
        # Testreihe kann ein Dutzend Einträge stellen, und die verdrängten die
        # fertigen Versionen darunter aus der Liste.
        for nummer, e in enumerate(eintraege[:25]):
            punkte = updater.points_by_kind(e.get('text') or '')
            if stand['art'] != 'alle':
                punkte = [p for p in punkte if p[0] == stand['art']]
            if not punkte:
                continue
            gezeigt += 1
            # Nur die neueste Version offen; ältere sind einen Klick entfernt.
            offen = (nummer == 0) or stand['art'] != 'alle'
            _version_box(fenster, behaelter, e, punkte, offen)
        if not gezeigt:
            tk.Label(behaelter, text=t('s_wn_nichts'), bg=BG, fg=SUB,
                     font=fenster.f_small).pack(anchor='w', pady=12)

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


def _chip(fenster, eltern, text, an, farbe=None):
    """Ein anklickbarer Filter — abgerundet, Rand in Akzentfarbe wenn gewählt.

    ⚠ `farbe` gibt dem Knopf die Farbe des Zustands, den er filtert — im
    Auftrags-Protokoll steht „abgebrochen" damit im selben blassen Rot wie die
    Zeilen, die er zeigt. Ohne Angabe bleibt es beim Grün, so wie auf der Seite
    „Was ist neu", wo alle Filter gleichrangig sind.
    """
    from .main_window import _round_rect
    farbe = farbe or ACCENT
    schrift = fenster.f_small
    hoehe = schrift.metrics('linespace') + 12
    breite = schrift.measure(text) + 26
    c = tk.Canvas(eltern, width=breite, height=hoehe, bg=BG,
                  highlightthickness=0, bd=0, cursor='hand2')
    blase = _round_rect(c, 1, 1, breite - 1, hoehe - 1,
                             radius=max(5, hoehe // 3),
                             fill=SURFACE, outline=farbe if an else LINE, width=1)
    beschriftung = c.create_text(breite / 2.0, hoehe / 2.0 + 1, text=text,
                                 fill=farbe if an else SUB, font=schrift)

    def setzen(gewaehlt):
        c.itemconfigure(blase, outline=farbe if gewaehlt else LINE)
        c.itemconfigure(beschriftung, fill=farbe if gewaehlt else SUB)

    c.setzen = setzen
    return c


_KIND_COLOR = {'neu': ACCENT, 'bess': '#7db8e8', 'fix': GOLD}
# ⚠ Eine Funktion, keine Konstante: Ein Wörterbuch auf Modulebene wird **einmal**
# beim Import gefüllt — und behielte damit die Sprache, die beim Start galt. Wer
# danach umschaltet, sähe die Marken weiter in der alten Sprache.
def _kind_word(art):
    return {'neu': t('s_wn_f_neu'), 'bess': t('s_wn_f_bess'),
            'fix': t('s_wn_f_fix')}.get(art, '')


def _version_box(fenster, eltern, eintrag, punkte, offen):
    """Eine Version mit Kopfzeile zum Auf- und Zuklappen."""
    zustand = {'offen': offen}
    kopf = tk.Frame(eltern, bg=BG, cursor='hand2')
    kopf.pack(fill='x', padx=24, pady=(12, 2))
    pfeil = icons.line(kopf, 'zuklappen' if offen else 'aufklappen',
                          background=BG, font=fenster.f_small)
    pfeil.pack(side='left', padx=(0, 8))
    tk.Label(kopf, text=eintrag.get('version') or '—', bg=BG, fg=ACCENT,
             font=fenster.f_bold).pack(side='left')
    if eintrag.get('datum'):
        tk.Label(kopf, text='  ' + eintrag['datum'], bg=BG, fg=SUB,
                 font=fenster.f_small).pack(side='left')
    tk.Label(kopf, text=t('s_wn_aenderungen') % len(punkte), bg=BG, fg=SUB,
             font=fenster.f_small).pack(side='right')

    koerper = tk.Frame(eltern, bg=BG)
    if offen:
        koerper.pack(fill='x')

    from .main_window import badge
    # Der Vorstellungssatz der Version steht **hier**, unter ihrer Überschrift —
    # nicht irgendwo am Seitenende. Wer eine Version aufklappt, will zuerst wissen,
    # worum es ging, und dann die Einzelheiten.
    from . import updater as _akt
    lead = _akt.intro(eintrag.get('text') or '')
    if lead:
        satz = tk.Label(koerper, text=lead, bg=BG, fg=SUB, font=fenster.f_small,
                        anchor='w', justify='left', wraplength=600)
        satz.pack(fill='x', padx=24, pady=(2, 8))

        # Denselben Weg wie bei den Punkten: nicht rechnen, sondern nehmen, was
        # das Label wirklich bekommt — sonst ragt der Satz bei jeder
        # Fenstergröße heraus.
        def lead_umbruch(ereignis, lab=satz):
            passend = max(200, ereignis.width - 8)
            try:
                if abs(_pixels(lab, lab.cget('wraplength'))
                       - passend) > 4:
                    lab.configure(wraplength=passend)
            except tk.TclError:
                pass

        satz.bind('<Configure>', lead_umbruch)

    # Alle Blasen so breit wie die längste Beschriftung — sonst flattern sie
    # und die Texte daneben fangen an unterschiedlichen Stellen an.
    breiteste = max(fenster.f_small.measure(_kind_word(a))
                    for a in ('neu', 'bess', 'fix')) + 20
    for art, zeile in punkte:
        z = tk.Frame(koerper, bg=BG)
        z.pack(fill='x', pady=3)
        badge(z, _kind_word(art), _KIND_COLOR.get(art, SUB),
              fenster.f_small, bg=BG,
              min_width=breiteste).pack(side='left', anchor='n', padx=(0, 14))
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
        etikett = tk.Label(z, text=_clean_row(zeile), bg=BG, fg=FG,
                           font=fenster.f_small, anchor='w', justify='left',
                           wraplength=max(360, (fenster.root.winfo_width()
                                                or 980) - 340))
        etikett.pack(side='left', fill='x', expand=True)

        def umbruch_anpassen(ereignis, lab=etikett):
            # Die Abfrage verhindert eine Schleife: Ein neuer Umbruch ändert die
            # Höhe, das löst wieder ein <Configure> aus.
            passend = max(200, ereignis.width - 8)
            try:
                if abs(_pixels(lab, lab.cget('wraplength'))
                       - passend) > 4:
                    lab.configure(wraplength=passend)
            except tk.TclError:
                pass

        etikett.bind('<Configure>', umbruch_anpassen)

    def umschalten(*_):
        zustand['offen'] = not zustand['offen']
        pfeil.swap_symbol('zuklappen' if zustand['offen']
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


def _clean_row(zeile):
    """Markdown-Auszeichnung raus — Tk zeigt sie sonst als Sternchen."""
    import re
    zeile = re.sub(r'\*\*(.+?)\*\*', r'\1', zeile)
    zeile = re.sub(r'`([^`]+)`', r'\1', zeile)
    zeile = re.sub(r'\[([^\]]+)\]\([^)]+\)', r'\1', zeile)
    # ⚠ Und was danach noch an Rückstrichen übrig ist, fliegt raus. Der
    # Ausdruck oben nimmt nur **Paare**; ein einzelner Strich — aus einem
    # halbierten Codeblock etwa — blieb stehen und stand im Fenster.
    return zeile.replace('`', '').strip()


def _card(parent, border_color=None, **kw):
    """Ein abgesetzter Kasten mit runden Ecken (siehe `main_window.rundrahmen`)."""
    from .main_window import round_frame
    innen = round_frame(parent, SURFACE, border_color or LINE, radius=8, base_color=BG)
    innen.holder.pack(fill='x', **kw)
    return innen


def _value_row(fenster, eltern, bez, wert, farbe=None):
    z = tk.Frame(eltern, bg=SURFACE)
    z.pack(fill='x', padx=16, pady=3)
    tk.Label(z, text=bez, bg=SURFACE, fg=SUB, font=fenster.f_small,
             width=24, anchor='w').pack(side='left')
    tk.Label(z, text=str(wert), bg=SURFACE, fg=farbe or FG,
             font=fenster.f_small, anchor='w').pack(side='left')


def _check_now(fenster):
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
    from . import updater
    fenster.say(t('s_ub_sucht'))

    def arbeit():
        try:
            neuere = updater.check(fenster.version or '0.0.0',
                                              force=True)
        except Exception as ausnahme:
            fehler.merken('seiten.jetzt_nachsehen', ausnahme)
            fenster.root.after(0, lambda: fenster.say(t('s_ub_sucht_fehler')))
            return

        def melden():
            # ⚠ **Erst neu aufbauen, dann sagen.** `rebuild()` zerstoert
            # saemtliche Kinder des Fensters und baut sie neu — auch die
            # Fusszeile, in der `say()` schreibt. Stand das `say()` davor,
            # existierte die Antwort ein paar Millisekunden und war dann weg:
            # Der Knopf blieb bei „Suche nach einer neuen Version …" stehen und
            # meldete nie ein Ergebnis. Genau so gemeldet von der Autor am
            # 27.08.2026, direkt nach der Reparatur des `datei`-Fehlers.
            #
            # Der Neuaufbau muss trotzdem sein: Die Kanal-Kaesten tragen die
            # Versionsnummern und muessen mitziehen.
            try:
                fenster.rebuild()
            except Exception:
                pass
            if neuere:
                fenster.say(t('s_ub_gefunden') % neuere.get('version'))
            elif updater.fetch_succeeded() is False:
                # ⚠ **Nicht „du bist aktuell" sagen, wenn gar nicht nachgesehen
                # werden konnte.** Die beiden Auskünfte sind das Gegenteil
                # voneinander. Bomb20 bekam am 27.08.2026 „du hast die neueste
                # rc67" gemeldet, während rc68 seit zwei Minuten draußen war —
                # der Abruf war an GitHubs Stundengrenze gescheitert und wurde
                # still verschluckt.
                fenster.say(t('s_ub_grenze') if updater.rate_limited()
                              else t('s_ub_sucht_fehler'))
            else:
                fenster.say(t('s_ub_aktuell'))

        try:
            fenster.root.after(0, melden)
        except Exception:
            pass

    threading.Thread(target=arbeit, daemon=True).start()


# Wie oft die Update-Seite von allein bei GitHub nachsieht, solange sie offen
# ist. Fünf Minuten — siehe die Begründung in `_refresh_channels`.
REFRESH_MS = 5 * 60 * 1000


def _refresh_channels(fenster, kaesten, neu_zeichnen):
    """Im Hintergrund nachsehen und die Knöpfe nachziehen — höchstens einmal.

    Läuft in einem eigenen Faden: Die Abfrage geht ins Netz und darf die Seite
    nicht aufhalten. Gezeichnet wird ausschließlich im Tk-Faden (`after`) — alles
    andere endet früher oder später in einem Absturz.
    """
    import threading
    from . import updater
    if getattr(kaesten, 'schon_gefragt', False):
        return
    kaesten.schon_gefragt = True
    vorher = (_fetch_label(False, fenster.version),
              _fetch_label(True, fenster.version))

    def arbeit():
        try:
            # ⚠ `erzwingen=True` ist hier nötig. Ohne das fragt `nachsehen()` nur,
            # wenn der letzte Blick länger als einen Tag her ist — und dann bleibt
            # die Beschriftung auf dem Stand von heute früh stehen. Genau so ist es
            # passiert: Der Knopf bot „v3.0.0-rc13 holen" an, während rc15 lief.
            # Wer draufdrückt, geht **zurück**. Einmal je Seitenaufbau nachfragen
            # ist der Preis dafür, dass draufsteht, was drin ist.
            updater.check(fenster.version or '0.0.0', force=True)
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
                    kaesten.after(REFRESH_MS, lambda: _refresh_channels(
                        fenster, kaesten, neu_zeichnen))
                except tk.TclError:
                    pass
                if (_fetch_label(False, fenster.version),
                        _fetch_label(True, fenster.version)) == vorher:
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


def _can_fetch(mit_vorab, eigene=''):
    """Steckt hinter dem Knopf ueberhaupt eine Tat? Sonst ist er keiner.

    ⚠ `_fetch_label` liefert **nur** die Beschriftung, und zwei ihrer Ergebnisse
    sind gar keine Aufforderung, sondern eine Zustandsmeldung: „v3.0.0-rc41 ist
    schon da" und „Erst oben auf ‚Jetzt nachsehen' druecken". Der Knopf blieb
    trotzdem ein Knopf — wer auf „ist schon da" drueckte, bekam die laufende
    Version noch einmal installiert. Gemeldet am 26.08.2026: „in dem gruenen
    kasten steht aber rc41 ist schon da, wenn man klickt will er auch direkt
    installieren."

    Was aussieht wie ein Knopf, muss etwas tun. Sonst wird daraus ein ruhiger
    Hinweis (siehe `_channel_box`).
    """
    from . import updater
    try:
        freigabe = updater.latest(mit_vorab)
    except Exception:
        return False
    if not freigabe:
        return False                     # „Erst oben auf ... druecken"
    fassung = freigabe.get('version') or ''
    # Nach einem geglueckten Update steht dort „Jetzt neu starten" — das ist
    # sehr wohl eine Tat.
    if _READY[0] and _READY[0] == fassung:
        return True
    if eigene and fassung.lstrip('v') == eigene.lstrip('v'):
        return False                     # laeuft schon, nichts zu holen
    return True



def _fetch_label(mit_vorab, eigene=''):
    """Die Beschriftung des Knopfes — mit der Version, die dahinter steckt.

    Aus dem Zwischenspeicher, ohne ins Netz zu gehen: Die Seite soll sofort
    stehen. Kurz darauf frischt `_refresh_channels` sie auf.

    ⚠ Und sie sagt, **wohin** es geht. „v2.0.0 holen" neben einer laufenden
    v3.0.0-rc15 sieht aus wie ein Update und ist ein Rückschritt — der Autor ist
    am 25.08.2026 genau darauf hereingefallen und stand danach wieder auf rc13.
    Ist die angebotene Version älter, steht das jetzt dabei; ist es dieselbe,
    steht das auch da.
    """
    from . import updater
    try:
        freigabe = updater.latest(mit_vorab)
    except Exception:
        freigabe = None
    if not freigabe:
        return t('s_ub_holen_keine')
    fassung = freigabe.get('version') or ''
    # Nach einem geglückten Update ist die neue Version auf der Platte, aber der
    # laufende Prozess hält noch die alte. Statt „beim nächsten Start" zu sagen,
    # wird der Knopf zum Neustart-Knopf.
    if _READY[0] and _READY[0] == fassung:
        return t('s_ub_neustart')
    if eigene:
        sauber = fassung.lstrip('v')
        if sauber == eigene.lstrip('v'):
            return t('s_ub_holen_gleich') % fassung
        if updater.is_newer(eigene, fassung):
            return t('s_ub_holen_zurueck') % fassung
    return t('s_ub_holen') % fassung


# Welche Version geholt und eingespielt wurde und nur noch einen Neustart braucht.
_READY = [None]


_TK_REPORTED = [False]      # siehe unten: nur der erste wird gemerkt


def _in_tk(fenster, tat):
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
        if not _TK_REPORTED[0]:
            _TK_REPORTED[0] = True
            fehler.merken('seiten.im_tk', ausnahme)
        return False


def _hand_over_after_restart(fenster):
    """Erst nachsehen, ob die neue Version lebt — dann erst selbst gehen.

    ⚠ Vorher trat die alte Version **sofort** ab. War die neue schon tot (unter
    Linux monatelang der Regelfall, siehe `updater.neue_fassung_laeuft`),
    stand der Rechner ohne Watcher da, und niemand erfuhr den Grund.

    Die Prüfung wartet ein paar Sekunden und gehört deshalb in einen eigenen
    Faden. Gezeichnet wird nur im Tk-Faden.
    """
    import threading
    from . import updater

    def pruefen():
        lebt = updater.new_version_alive()
        def melden():
            if lebt:
                _hand_over(fenster)
            elif updater.roll_back():
                # ⚠ Ohne Rückweg liefe die alte Fassung nur noch aus ihrer
                # offenen Inode weiter — wer sie schließt, stünde ohne Watcher
                # da. Die Sicherung von vor dem Tausch macht daraus ein
                # Umbenennen.
                fenster.say(t('up_zurueckgerollt'))
            else:
                fenster.say(t('s_ub_neustart_tot'))
        try:
            fenster.root.after(0, melden)
        except Exception as ausnahme:
            fehler.merken('seiten.nach_neustart', ausnahme)

    threading.Thread(target=pruefen, daemon=True).start()


def _hand_over(fenster, notausgang=2.0, gleich=400):
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



def _fetch_version(fenster, mit_vorab):
    """Die neueste Version dieses Kanals holen und einspielen.

    ⚠ Nicht `nachsehen()` benutzen: Das meldet nur, was **neuer** ist als die
    laufende Version. Wer eine Testfassung fährt und zurück auf die letzte
    fertige will, bekäme damit nichts. `neueste()` fragt den Kanal, nicht den
    Abstand zur eigenen Version.

    Heruntergeladen wird in einem eigenen Faden — es sind zwölf Megabyte, und
    das Fenster darf so lange nicht einfrieren.
    """
    import threading
    from . import updater
    # ⚠ Erst nachsehen, dann greifen. Die Liste der Freigaben steht im
    # Zwischenspeicher und frischt sich nur einmal am Tag auf — ohne diesen
    # Schritt holt der Knopf die Version von gestern, obwohl heute eine neuere
    # da ist. Gemessen: Der Knopf bot v3.0.0-rc2 an, während rc7 längst
    # veröffentlicht war.
    try:
        updater.check(fenster.version or '0.0.0')
    except Exception as ausnahme:
        fehler.merken('seiten.fassung_holen.nachsehen', ausnahme)
    freigabe = updater.latest(mit_vorab)
    if not freigabe:
        fenster.say(t('s_ub_holen_keine'))
        return
    # Schon geholt? Dann ist der Knopf jetzt der Neustart-Knopf.
    if _READY[0] and _READY[0] == (freigabe.get('version') or ''):
        fenster.say(t('s_ub_startet_neu'))
        if not updater.restart():
            fenster.say(t('s_ub_neustart_nein'))
            return
        _hand_over_after_restart(fenster)
        return
    art = updater.packaging()
    if art == 'quellcode':
        fenster.say(t('update_quellcode'))
        return
    datei = updater.matching_asset(freigabe)
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
            fenster.say(t('s_ub_wird_gebaut'))
        else:
            fenster.say(t('selbst_holen'))
        return

    fenster.say(t('s_ub_holen_laeuft') % freigabe.get('version'))

    # ⚠ **Die Sperre kommt vor dem Herunterladen.** Zwei Klicks kurz
    # hintereinander — oder zwei Instanzen, die im Startfenster beide
    # hochkamen, bevor der Einzelstart greift — ließen sonst zwei Installer
    # los. Freigegeben wird sie bei jedem Ausgang, außer beim Übergeben an den
    # Helfer: Dann gehört sie ihm, und er räumt sie weg.
    from . import update_run
    if not update_run.take_lock():
        fenster.say(t('up_laeuft_schon'))
        return

    def arbeit():
        uebergeben = False
        try:
            ziel = updater.download(
                datei, progress=lambda p: _in_tk(
                    fenster, lambda: fenster.say(t('wird_geladen', p))),
                release=freigabe)

            # ⚠ Hier stand bis v3.29.0 ein Hinweisfenster, das quittiert
            # werden musste, bevor das Setup loslief (seit rc52). Es war
            # richtig, solange der Watcher danach **nicht** wiederkam: Ein
            # Programm, das sich wortlos schließt, sieht aus wie ein Absturz.
            # Jetzt kommt er von selbst wieder — und aus einem Klick sollen
            # nicht zwei werden. Die Ansage steht in der Fußzeile.
            geklappt, grund = updater.install(
                ziel, target_version=freigabe.get('version') or '',
                previous_version=fenster.version or '')
            if not geklappt:
                _in_tk(fenster, lambda: fenster.say(
                    t('update_fehler', grund)))
                return
            _READY[0] = freigabe.get('version') or ''

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
            # Treten wir gleich ab, entfaellt das Warten vollstaendig. Wieder
            # hoch faehrt uns der Helfer aus `update_run` — bis v3.29.0 tat
            # das niemand, und der Nutzer musste selbst starten. Die Sperre
            # gehoert ab hier dem Helfer; er gibt sie am Ende frei.
            if art == 'exe':
                uebergeben = True
                _in_tk(fenster, lambda: fenster.say(t('up_wird_eingespielt')))
                _hand_over(fenster)
                return

            # Linux: Das AppImage ist getauscht, die alte Fassung gesichert.
            # ⚠ Bis v3.29.0 wurde hier die Seite umgebaut, und es brauchte
            # einen zweiten Klick auf „Jetzt neu starten" — ein Rest aus der
            # Zeit des Dateitauschs beim Beenden. Jetzt geht es gleich weiter.
            # Der alte Knopf bleibt nur als Rückfall, wenn schon der Start
            # scheitert.
            def _neustart():
                fenster.say(t('s_ub_startet_neu'))
                if updater.restart():
                    _hand_over_after_restart(fenster)
                    return
                # ⚠ Erst zeichnen, dann melden: Der Neuaufbau macht aus
                # „holen" ein „Jetzt neu starten" und zerstoert dabei die
                # Fusszeile. Stand das `say()` zuerst, war die Meldung nach
                # einer zwanzigstel Sekunde wieder weg.
                fenster.rebuild()
                try:
                    fenster.root.after(50, lambda: fenster.say(
                        t('s_ub_neustart_nein')))
                except Exception:
                    pass

            _in_tk(fenster, _neustart)
        except Exception as ausnahme:
            grund = str(ausnahme)
            fehler.merken('seiten.fassung_holen', ausnahme)
            _in_tk(fenster, lambda: fenster.say(t('update_fehler', grund)))
        finally:
            if not uebergeben:
                update_run.release_lock()

    threading.Thread(target=arbeit, daemon=True).start()


def _channel_box(fenster, eltern, titel, text, gewaehlt, tat, marke_text='',
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
    from .main_window import badge as blase
    from .main_window import round_frame
    innen = round_frame(eltern, SURFACE, ACCENT if gewaehlt else LINE,
                       radius=8, base_color=BG)
    rand = innen.holder
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
    innen.canvas.configure(cursor='hand2')
    leinwand = innen.canvas

    kopf = tk.Frame(innen, bg=SURFACE)
    kopf.pack(fill='x', padx=14, pady=(12, 2))
    # ⚠ **Kein Punkt vor dem Titel mehr** (07.09.2026). Er zeigte dasselbe wie
    # der Rahmen des Kastens, der bei der gewählten Fassung grün wird —
    # zweimal dieselbe Auskunft an derselben Stelle. Gemeldet mit „ist
    # unnötig, da der Kasten ja schon grün wird".
    tk.Label(kopf, text=titel, bg=SURFACE, fg=FG,
             font=fenster.f_bold).pack(side='left')
    if marke_text:
        blase(kopf, marke_text, GOLD, fenster.f_small).pack(side='left', padx=8)

    beschreibung = tk.Label(innen, text=text, bg=SURFACE, fg=SUB,
                            font=fenster.f_small, anchor='w', justify='left')
    beschreibung.pack(fill='x', padx=14, pady=(0, 12))
    # ⚠ 28 gleicht nur `padx=14` links und rechts aus. Rahmen und Leinwand
    # brauchen darüber hinaus ein paar Pixel, die niemand mitgerechnet hat —
    # gemessen fehlten 5 (tools/randpruefung.py). Mit etwas Luft bricht der Text
    # ein paar Pixel früher um, was niemand sieht, statt abgeschnitten zu
    # werden, was jeder sieht.
    _wrap(beschreibung, inset=36)

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
        knopf = _button(fenster, innen, holen_text, holen, strong=gewaehlt)
        knopf.pack(fill='x', padx=14, pady=(0, 12))
    elif holen is not None:
        # Kein Knopf, sondern eine Auskunft: Es gibt gerade nichts zu holen.
        # Gleiche Stelle, gleiche Breite, nur ohne Rahmen und ohne Handzeiger —
        # damit niemand darauf drueckt und sich fragt, warum nichts passiert.
        auskunft = tk.Label(innen, text=holen_text, bg=SURFACE, fg=SUB,
                            font=fenster.f_small, anchor='center')
        auskunft.pack(fill='x', padx=14, pady=(4, 16))
        # Ein Klick darauf soll dasselbe tun wie ein Klick auf den Kasten:
        # den Kanal waehlen. Sonst waere hier ein totes Loch im Kasten.
        auskunft.bind('<Button-1>', lambda e: tat())
        auskunft.configure(cursor='hand2')
    return rand


def _server_status(fenster, rahmen):
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

    _heading(fenster, rahmen, t('hf_serverstatus'), t('s_st_lead'))
    innen = _scroll_area(rahmen)

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
            _body_text(behaelter,
                        t('s_st_kein_netz') if ohne_netz else t('s_st_leer'),
                        fenster.f_small, pady=(4, 8))
            return
        if ohne_netz:
            _body_text(behaelter, t('s_st_alt_ohne_netz'), fenster.f_small,
                        color=GOLD, pady=(0, 8))

        # --- Kopfzeile, wie oben auf der Statusseite ---
        # Links „Zuletzt aktualisiert vor …", rechts die Zusammenfassung. Die
        # Seite hinterlegt diesen Streifen in der Ampelfarbe; das ist ihr
        # auffälligstes Element und die Antwort auf die eigentliche Frage.
        _status_banner(fenster, behaelter, lage)

        karte = _card(behaelter, pady=(0, 6))
        tk.Frame(karte, bg=SURFACE, height=8).pack()
        for sys_ in lage.get('systeme') or []:
            _system_row(fenster, karte, sys_)
        tk.Frame(karte, bg=SURFACE, height=10).pack()

        fuss = _card(behaelter, pady=(8, 6))
        tk.Frame(fuss, bg=SURFACE, height=8).pack()
        if lage.get('stand'):
            _value_row(fenster, fuss, t('s_st_stand'), _clock(lage['stand']))
        _value_row(fenster, fuss, t('s_st_geholt'), _clock(lage.get('geholt')))
        _source_row(fenster, fuss, t('s_st_quelle'), lage.get('quelle') or '')
        tk.Frame(fuss, bg=SURFACE, height=10).pack()

        _body_text(behaelter, t('s_st_hinweis'), fenster.f_small, pady=(10, 4))

        # --- „Letzte Meldungen", wie unten auf der Statusseite ---
        # Auch **erledigte**: Wer abends nicht ins Spiel kommt, will sehen, ob
        # es nachmittags eine Wartung gab — nicht nur, ob gerade eine läuft.
        tk.Label(behaelter, text=t('s_st_letzte'), bg=BG, fg=FG,
                 font=fenster.f_title, anchor='w').pack(fill='x', pady=(22, 6))
        meldungsraum = tk.Frame(behaelter, bg=BG)
        meldungsraum.pack(fill='x')
        _body_text(meldungsraum, t('s_st_laedt'), fenster.f_small, pady=(2, 4))
        _load_notices(fenster, meldungsraum, lage.get('quelle') or '')

    def auffrischen(erzwingen=False):
        if erzwingen:
            fenster.say(t('s_st_laedt'))

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
                lage = serverstatus.state(force=erzwingen)
            except Exception as ausnahme:
                fehler.merken('seiten.serverstatus', ausnahme)
                lage = None
            try:
                if lage is None:
                    fenster.root.after(
                        0, lambda: behaelter.winfo_exists()
                        and fenster.say(t('s_st_fehler')))
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
    _button(fenster, innen, t('s_st_nachsehen'),
           lambda: auffrischen(True), strong=True).pack(fill='x', pady=(0, 12))
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
                lage, veraendert = serverstatus.ask()
            except Exception:
                lage, veraendert = None, False
            # Dieselbe Absicherung wie oben: Der Takt läuft, während der Nutzer
            # das Fenster schliessen kann.
            try:
                if veraendert and lage:
                    fenster.root.after(0, lambda: behaelter.winfo_exists()
                                       and zeichnen(lage))
                fenster.root.after(POLL_MS, takt)
            except (RuntimeError, tk.TclError):
                pass

        threading.Thread(target=arbeit, daemon=True).start()

    zeichnen(serverstatus.stored_state())   # sofort, ohne Netz
    auffrischen()                                # und im Hintergrund nachziehen
    fenster.root.after(POLL_MS, takt)            # danach im Takt weiter


# Wie oft nachgefragt wird, solange die Seite offen ist. Eine Minute ist
# vertretbar, weil mit ETag gefragt wird und der unveränderte Fall den Server
# fast nichts kostet.
POLL_MS = 60_000


def _relative_time(stempel):
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


def _status_banner(fenster, eltern, lage):
    """Der Streifen ganz oben: links das Alter, rechts die Zusammenfassung.

    Bildet nach, was die Statusseite dort zeigt („Last updated just now" /
    „No issues detected"). Es ist die Antwort auf die Frage, wegen der jemand
    die Seite überhaupt öffnet — deshalb steht sie oben und nicht in einer
    Werteliste.

    ⚠ **Kein `round_frame`.** Dessen Leinwand bleibt auf ihrer Anfangshöhe,
    wenn der Inhalt nicht mitgemessen wird — der Streifen erschien als leerer
    grüner Rahmen. Ein schlichter Frame mit farbigem Balken am linken Rand
    trägt dieselbe Aussage und kann nicht einklappen.
    """
    farbe = _status_color(lage)
    streifen = tk.Frame(eltern, bg=SURFACE)
    streifen.pack(fill='x', pady=(0, 10))
    tk.Frame(streifen, bg=farbe, width=4).pack(side='left', fill='y')

    inhalt = tk.Frame(streifen, bg=SURFACE)
    inhalt.pack(side='left', fill='x', expand=True, padx=12, pady=10)
    alter = _relative_time(lage.get('geholt'))
    tk.Label(inhalt, text=t('s_st_zuletzt') % alter,
             bg=SURFACE, fg=SUB, font=fenster.f_small,
             anchor='w').pack(side='left')
    alles_gut = (lage.get('gesamt') or '').lower() == 'operational'
    tk.Label(inhalt, text=t('s_st_ok') if alles_gut else t('s_st_stoerung'),
             bg=SURFACE, fg=farbe, font=fenster.f_bold,
             anchor='e').pack(side='right')


def _load_notices(fenster, raum, quelle):
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
            _body_text(raum, t('s_st_keine'), fenster.f_small, pady=(2, 4))
        else:
            for meldung in liste:
                _notice_card(fenster, raum, meldung)
        if quelle:
            _source_link(fenster, raum, t('s_st_alle_zeigen'), quelle)

    def arbeit():
        try:
            liste = serverstatus.messages(2)
        except Exception as ausnahme:
            fehler.merken('seiten.serverstatus_meldungen', ausnahme)
            liste = []
        fenster.root.after(0, lambda: einsetzen(liste))

    threading.Thread(target=arbeit, daemon=True).start()


def _source_link(fenster, eltern, text, adresse):
    """Ein anklickbarer Verweis als eigene Zeile."""
    link = tk.Label(eltern, text=text, bg=BG, fg=ACCENT, font=fenster.f_small,
                    anchor='w', cursor='hand2')
    link.pack(fill='x', pady=(10, 4))

    def oeffnen(_=None):
        # ⚠ Über `pfade.im_browser` — nie `webbrowser.open()` direkt. Warum:
        # siehe die Begründung dort (im AppImage öffnet es nichts und meldet
        # trotzdem Erfolg).
        if not pfade.im_browser(adresse):
            fenster.say(t('s_ub_auf_nein') % adresse)

    link.bind('<Button-1>', oeffnen)
    link.bind('<Enter>', lambda e: link.configure(fg=FG))
    link.bind('<Leave>', lambda e: link.configure(fg=ACCENT))


def _source_row(fenster, eltern, bez, adresse):
    """Wie `_value_row`, aber die Adresse lässt sich anklicken.

    Eine Quelle, die man nur ablesen und abtippen kann, ist keine Quelle —
    besonders bei einer Angabe, die man im Zweifel selbst nachprüfen soll."""
    z = tk.Frame(eltern, bg=SURFACE)
    z.pack(fill='x', padx=16, pady=3)
    tk.Label(z, text=bez, bg=SURFACE, fg=SUB, font=fenster.f_small,
             width=24, anchor='w').pack(side='left')
    if not adresse:
        tk.Label(z, text='—', bg=SURFACE, fg=FG, font=fenster.f_small,
                 anchor='w').pack(side='left')
        return
    link = tk.Label(z, text=adresse, bg=SURFACE, fg=ACCENT,
                    font=fenster.f_small, anchor='w', cursor='hand2')
    link.pack(side='left')

    def oeffnen(_=None):
        if not pfade.im_browser(adresse):
            fenster.say(t('s_ub_auf_nein') % adresse)

    link.bind('<Button-1>', oeffnen)

    # Rückmeldung beim Darüberfahren über die **Farbe**, nicht über die Schrift.
    #
    # ⚠ `fenster.f_small` ist ein `tkfont.Font`-Objekt, kein Tupel — `font[0]`
    # wirft. Und ein eigenes, unterstrichenes Font-Objekt anzulegen wäre die
    # zweite Falle: „Schrift größer" stellt zentral genau diese gemeinsamen
    # Objekte um, ein eigenes bliebe stehen und der Link wäre der einzige
    # Text, der nicht mitwächst.
    link.bind('<Enter>', lambda e: link.configure(fg=FG))
    link.bind('<Leave>', lambda e: link.configure(fg=ACCENT))


def _status_color(lage):
    """Die Farbe der Gesamtlage — die schlechteste, die vorkommt.

    „Alles grün außer einem" ist nicht grün. Wer nur die Zusammenfassung liest,
    soll denselben Eindruck bekommen wie jemand, der die Liste durchgeht."""
    rang = {'ok': 0, 'hinweis': 1, 'gestoert': 2, 'aus': 3}
    schlimmste = None
    for s_ in lage.get('systeme') or []:
        if schlimmste is None or rang.get(s_.get('ampel'), 2) > rang.get(schlimmste.get('ampel'), 2):
            schlimmste = s_
    return (schlimmste or {}).get('farbe') or FG


def _system_row(fenster, eltern, sys_):
    """Ein System: Farbbalken, Name, Zustand im Wortlaut von CIG."""
    z = tk.Frame(eltern, bg=SURFACE)
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
    tk.Label(z, text=name, bg=SURFACE, fg=FG,
             font=fenster.f_small, width=22, anchor='w').pack(side='left')
    tk.Label(z, text=zustand, bg=SURFACE,
             fg=sys_.get('farbe') or FG, font=fenster.f_small,
             anchor='w').pack(side='left')


def _notice_card(fenster, eltern, meldung):
    """Eine Meldung im Aufbau der Statusseite.

        Live Deployment                              ✔ Erledigt
        vor 7 Std.
        maintenance          Persistent Universe · Arena Commander
        <Meldungstext, Update-Zeilen im Original>

    ⚠ Der Text bleibt im **Wortlaut von CIG**, auch die Update-Zeilen
    (`1415 UTC - Initial Notice, Matchmaking disabled.`). Übersetzt wäre es eine
    Aussage, die RSI nie gemacht hat — und bei einer Störungsmeldung ist genau
    das gefährlich."""
    karte = _card(eltern, pady=(6, 2))
    tk.Frame(karte, bg=SURFACE, height=10).pack()

    # Kopf: Titel links, Zustand rechts
    kopf = tk.Frame(karte, bg=SURFACE)
    kopf.pack(fill='x', padx=16)
    # Der Titel kommt von CIG und bleibt, wie er dort steht.
    titel = meldung.get('titel') or '—'
    tk.Label(kopf, text=titel, bg=SURFACE, fg=FG,
             font=fenster.f_bold, anchor='w').pack(side='left')
    erledigt = bool(meldung.get('erledigt'))
    tk.Label(kopf, text=(t('s_st_erledigt_kurz')) if erledigt
             else t('s_st_offen'),
             bg=SURFACE,
             # Erledigt grün wie auf der Statusseite; offen in Gold, damit es
             # auffällt — eine laufende Störung ist der Grund, warum jemand
             # überhaupt hier nachsieht.
             fg=(ACCENT if erledigt else GOLD),
             font=fenster.f_small, anchor='e').pack(side='right')

    # Alter — wie auf der Seite („7h ago"), nicht das Datum
    wann = _relative_time(meldung.get('begonnen'))
    tk.Label(karte, text=wann, bg=SURFACE,
             fg=SUB, font=fenster.f_small, anchor='w').pack(
                 fill='x', padx=16, pady=(2, 6))

    # Etiketten: Schweregrad links, betroffene Systeme rechts — wie auf der Seite
    _tag_row(fenster, karte, meldung.get('schwere'),
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
        _body_text(karte, zeile,
                    fenster.f_bold if fett else fenster.f_small,
                    color=FG if fett else SUB, bg=SURFACE,
                    fill='x', padx=16, pady=(0, 3), inset=48)
    tk.Frame(karte, bg=SURFACE, height=10).pack()


def _tag(fenster, eltern, text):
    """Ein kleines graues Schild, wie die Marken auf der Statusseite.

    ⚠ **Bewusst kein `round_frame`.** Der setzt seinen Inhalt per
    `create_window` auf eine Leinwand — dadurch trägt der Inhalt nicht zur
    Wunschgröße bei, das Schild hat keine eigene Breite und dehnt sich über
    die halbe Karte. Bei großen Kästen fällt das nicht auf, hier schon: Aus
    kompakten Marken wurden Balken. Ein schlichtes Label kennt seine Größe.
    """
    return tk.Label(eltern, text=text, bg=LINE, fg=FG, font=fenster.f_small,
                    padx=8, pady=3)


def _tag_row(fenster, eltern, schwere, betroffen):
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
    reihe = tk.Frame(eltern, bg=SURFACE)
    reihe.pack(fill='x', padx=16, pady=(0, 6))

    ABSTAND = 6

    grund = _tag(fenster, reihe, schwere) if schwere else None

    systeme = tk.Frame(reihe, bg=SURFACE)
    schilder = [_tag(fenster, systeme, name) for name in (betroffen or [])]
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


def _clock(stempel):
    """Ein Zeitpunkt als Ortszeit. Die Quelle rechnet in UTC — hier steht,
    was die Uhr des Nutzers zeigt, sonst rechnet jeder selbst um."""
    import time as _t
    if not stempel:
        return '—'
    return _t.strftime('%d.%m.%Y %H:%M', _t.localtime(stempel))


def _credit_box(fenster, eltern, name, lizenz, was, adresse=None):
    """Ein Beitrag: wer, unter welcher Lizenz, wofür — und wo er zu finden ist."""
    kasten = tk.Frame(eltern, bg=SURFACE)
    kasten.pack(fill='x', pady=(0, 8))

    from .main_window import badge as blase
    kopf = tk.Frame(kasten, bg=SURFACE)
    kopf.pack(fill='x', padx=16, pady=(12, 2))
    tk.Label(kopf, text=name, bg=SURFACE, fg=FG, font=fenster.f_bold,
             anchor='w').pack(side='left')
    # Die Lizenz als Blase daneben — sie gehört zum Namen, nicht in den Fließtext.
    blase(kopf, lizenz, ACCENT, fenster.f_small).pack(side='left', padx=8)

    # ⚠ Auch hier durch `_ohne_marken`: Auf der Danke-Seite stand wörtlich
    # `**Krovax**` auf dem Bildschirm — die Sternchen sind für den Leser der
    # Sprachdatei gedacht, nicht für den Spieler. Gefunden am 09.09.2026 von
    # `tools/oberflaeche_pruefen.py`, drin seit die Quelle genannt wird.
    text = tk.Label(kasten, text=_strip_markup(was), bg=SURFACE, fg=SUB,
                    font=fenster.f_small, anchor='w', justify='left')
    text.pack(fill='x', padx=16, pady=(0, 10))
    # ⚠⚠ `inset` MUSS die Polsterung aus `pack` nennen — hier 2 × 16.
    # `_wrap` misst den Elternrahmen; was das Label per `padx` abgibt, sieht
    # es nicht. Ohne die 32 stand `wraplength` auf 886, verfügbar waren 860,
    # und die letzte Zeile jeder Quellenbeschreibung wurde still abgeschnitten
    # (gemessen 14.09.2026: +8 bis +27 px auf der Danke-Seite, in beiden
    # Sprachen). Gefunden hat es `tools/randpruefung.py` — aber erst, seit es
    # nicht mehr nur elf von 33 Seiten anschaut.
    _wrap(text, inset=32)

    if adresse:
        # ⚠ Nicht `_source_row`: die reserviert 24 Zeichen für eine
        # Beschriftung, und ohne Beschriftung stünde der Verweis eingerückt
        # mitten in der Karte statt am linken Rand wie der Text darüber.
        link = tk.Label(kasten, text=adresse, bg=SURFACE, fg=ACCENT,
                        font=fenster.f_small, anchor='w', cursor='hand2')
        link.pack(fill='x', padx=16, pady=(0, 12))

        def oeffnen(_=None):
            if not pfade.im_browser(adresse):
                fenster.say(t('s_ub_auf_nein') % adresse)

        link.bind('<Button-1>', oeffnen)
        link.bind('<Enter>', lambda e: link.configure(fg=FG))
        link.bind('<Leave>', lambda e: link.configure(fg=ACCENT))


def _contributor(fenster, eltern, name, gruppe, idee, funde):
    """Ein Name in der Dankliste — aufklappbar.

    Sichtbar ist immer nur die Kopfzeile (Name + Gruppe). Was die Person
    beigetragen hat, steht darunter und erscheint erst auf Klick. Grund: Die
    Liste soll vollständig bleiben, auch wenn irgendwann fünfzig Leute
    daraufstehen — vollständig **und** überschaubar geht nur so.
    """
    from .main_window import badge as blase
    kasten = tk.Frame(eltern, bg=SURFACE)
    kasten.pack(fill='x', pady=(0, 6))

    kopf = tk.Frame(kasten, bg=SURFACE, cursor='hand2')
    kopf.pack(fill='x', padx=16, pady=10)
    pfeil = icons.line(kopf, 'aufklappen', background=SURFACE,
                          font=fenster.f_small)
    pfeil.pack(side='left', padx=(0, 8))
    tk.Label(kopf, text=name, bg=SURFACE, fg=FG, font=fenster.f_bold,
             anchor='w').pack(side='left')
    if gruppe:
        blase(kopf, gruppe, ACCENT, fenster.f_small).pack(side='left', padx=8)

    koerper = tk.Frame(kasten, bg=SURFACE)

    gebaut = []

    def zeichnen():
        if gebaut:
            return
        for text, farbe in ((idee, FG), (funde, SUB)):
            if not text:
                continue
            lab = tk.Label(koerper, text=_strip_markup(text), bg=SURFACE,
                           fg=farbe, font=fenster.f_small, anchor='w',
                           justify='left')
            lab.pack(fill='x', padx=(46, 16), pady=(0, 8))
            _wrap(lab, inset=62)
        gebaut.append(True)

    def umschalten(_=None):
        if koerper.winfo_ismapped():
            koerper.pack_forget()
            pfeil.swap_symbol('aufklappen')
        else:
            zeichnen()
            koerper.pack(fill='x', after=kopf)
            pfeil.swap_symbol('zuklappen')

    for teil in (kopf, pfeil) + tuple(kopf.winfo_children()):
        teil.bind('<Button-1>', umschalten)


    # ⭐ Der Pfeil hebt sich ab, wenn die Maus die Kopfzeile
    # trifft — anklickbar ist hier die Zeile, nicht der Pfeil.
    icons.hover_group(kopf, pfeil)

def _thanks(fenster, rahmen):
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
    _heading(fenster, rahmen, t('hf_danke'), t('s_dk_lead'))
    innen = _scroll_area(rahmen)

    # --- Wer das gebaut hat ---
    # ⚠ Ganz oben und mit Avatar, nicht als eine Zeile unter vielen. Diese Seite
    # nennt fremde Arbeit, und genau deshalb muss die eigene zuerst stehen —
    # sonst schmälert die Aufzählung das, worum es hier eigentlich geht.
    # Gemeldet am 27.08.2026: „ich bin zwar dankbar, aber so dankbar nun auch
    # wieder nicht, zudem wieso sollte ich meine Leistung dadurch schmälern."
    #
    # Der Block stand bis dahin auf „Update & Über" und ist von dort hierher
    # gewandert — dieselben Angaben an zwei Stellen waren die eigentliche Klage.
    tk.Label(innen, text=t('hf_wer'), bg=BG, fg=FG, font=fenster.f_title,
             anchor='w').pack(fill='x', pady=(0, 2))
    tk.Label(innen, text=t('s_ub_wer_h'), bg=BG, fg=SUB, font=fenster.f_small,
             anchor='w').pack(fill='x', pady=(0, 12))

    autor = _card(innen)
    zeile = tk.Frame(autor, bg=SURFACE)
    zeile.pack(fill='x', padx=16, pady=14)
    from .main_window import _bundled
    logo = _bundled(os.path.join('assets', 'xharig.png'))
    if logo and os.path.exists(logo):
        try:
            voll = tk.PhotoImage(file=logo)
            teiler = max(1, voll.width() // 64)
            fenster._author_logo = voll.subsample(teiler, teiler)
            tk.Label(zeile, image=fenster._author_logo, bg=SURFACE).pack(
                side='left', padx=(0, 16))
        except Exception as ausnahme:
            fehler.merken('seiten.danke.logo', ausnahme)
    rechts = tk.Frame(zeile, bg=SURFACE)
    rechts.pack(side='left', fill='x', expand=True)
    tk.Label(rechts, text='Xharig', bg=SURFACE, fg=ACCENT, font=fenster.f_title,
             anchor='w').pack(fill='x')
    tk.Label(rechts, text='%s %s · GPL-3.0-only'
             % (t('hf_titel'), fenster.version or ''), bg=SURFACE, fg=SUB,
             font=fenster.f_small, anchor='w').pack(fill='x')
    _link(fenster, rechts, 'github.com/Xharig/SC-BP-Watcher',
             'https://github.com/Xharig/SC-BP-Watcher')
    _body_text(innen, t('s_dk_selbst_h'), fenster.f_small, fill='x',
                pady=(10, 0))

    # --- Mitgeliefert ---
    tk.Label(innen, text=t('s_dk_dabei'), bg=BG, fg=FG, font=fenster.f_title,
             anchor='w').pack(fill='x', pady=(18, 2))
    _body_text(innen, t('s_dk_dabei_h'), fenster.f_small, fill='x',
                pady=(0, 10))
    _credit_box(fenster, innen, 'Lucide', 'ISC', t('s_dk_symbole'),
               'https://lucide.dev')

    # --- Wird geladen, nicht mitgeliefert ---
    tk.Label(innen, text=t('s_dk_extern'), bg=BG, fg=FG, font=fenster.f_title,
             anchor='w').pack(fill='x', pady=(18, 2))
    _body_text(innen, t('s_dk_extern_h'), fenster.f_small, fill='x',
                pady=(0, 10))
    _credit_box(fenster, innen, 'Star Citizen Mission DataBase',
               'CC BY-NC-ND 4.0', t('s_dk_scmdb'), 'https://scmdb.net')
    # ⚠ Seit v3.3.0-rc39 kommen die Rohstoffpreise von hier. Wer eine Quelle
    # benutzt, nennt sie — sie stand bis rc40 nirgends.
    _credit_box(fenster, innen, 'UEX Corp',
               t('s_dk_keine_lizenz'), t('s_dk_uex'), 'https://uexcorp.space')
    # ⚠ Seit v3.19.0 kommen die Steckplätze der Schiffe von hier. Wer eine
    # Quelle benutzt, nennt sie — und zwar bevor jemand danach fragt.
    _credit_box(fenster, innen, 'erkul.games',
               t('s_dk_keine_lizenz'), t('s_dk_erkul'), 'https://erkul.games')
    # ⚠ Kein Datenlieferant, sondern fremdes **Werkzeug**: Ohne diese
    # Erweiterung müsste jeder seine vierzig Schiffe von Hand eintippen. Sie
    # steht hier, weil der Import ohne sie nichts wäre — und damit man sie
    # findet, ohne im Netz danach suchen zu müssen.
    _credit_box(fenster, innen, 'Star Citizen Hangar XPLORer (dolkensp)',
               'MIT', t('s_dk_xplorer'), XPLORER_PAGE)
    # StarStrings hat KEINE Lizenzangabe - kein LICENSE im Repo, nichts in
    # der readme, GitHub meldet keine (geprueft 29.08.2026). Hier stand
    # 'CC BY-NC-SA 4.0'. Das war geraten, vermutlich von scmdb uebernommen,
    # und es schrieb MrKraken eine Lizenz zu, die er nie vergeben hat.
    _credit_box(fenster, innen, 'StarStrings (MrKraken)',
               t('s_dk_keine_lizenz'),
               t('s_dk_ss'), 'https://starstrings.app')
    _credit_box(fenster, innen, 'SC Deutsch Launcher', t('s_dk_freiwillig'),
               t('s_dk_scdl'), 'https://www.sc-deutsch-launcher.de/')
    # ⚠⚠ Die Übersetzung selbst hat einen eigenen Urheber und eine eigene
    # Lizenz (CC BY-NC-SA 4.0). Die verlangt ausdrücklich Name UND Repository —
    # der Verteiler allein genügt nicht.
    _credit_box(fenster, innen, 'StarCitizen-Deutsch-INI (rjcncpt)',
               'CC BY-NC-SA 4.0', t('s_dk_ini'),
               'https://github.com/rjcncpt/StarCitizen-Deutsch-INI')

    # --- Menschen ---
    # ⚠ Aufklappbar, und zwar mit Absicht: Die Liste wird wachsen. der Autor am
    # 27.08.2026: „das werden später ja mal richtig viele, ich möchte schon alle
    # drauf haben aber nichts überladen." Sichtbar bleibt darum immer nur der
    # Name mit seiner Gruppe — was daraus geworden ist, steht eine Zeile tiefer
    # und nur auf Klick. So trägt die Seite auch fünfzig Namen noch.
    tk.Label(innen, text=t('s_dk_leute'), bg=BG, fg=FG, font=fenster.f_title,
             anchor='w').pack(fill='x', pady=(18, 2))
    _body_text(innen, t('s_dk_leute_h'), fenster.f_small, fill='x',
                pady=(0, 2))
    _body_text(innen, t('s_dk_aufklappen'), fenster.f_small, color=SUB,
                fill='x', pady=(0, 10))

    for name, gruppe, idee, funde in (
            ('Haldjas', 'pr0', t('s_dk_haldjas_idee'),
             t('s_dk_haldjas_bugs')),
            # ⚠ Zwei Bausteine hintereinander: die frühen Funde samt
            # Nitro-Dank und der Fund vom 11.09. `_contributor` nimmt einen Text —
            # also hier zusammensetzen, statt die Funktion für einen
            # Sonderfall umzubauen.
            #
            # ⚠⚠ **Eine Person, ein Eintrag.** Er stand hier bis zum
            # 12.09.2026 zweimal, weil sein Anzeigename gewechselt hat — die
            # Seite zählt Beiträge je Person, seine waren dadurch geteilt.
            # Genannt wird der Name, unter dem er heute auftritt.
            ('rurudotorg', 'SC4M', t('s_dk_rurudotorg_idee'),
             t('s_dk_rurudotorg_bugs') + '\n\n' + t('s_dk_rurudotorg_bugs2')),
            ('Morkhan', 'KRT', t('s_dk_morkhan_idee'),
             t('s_dk_morkhan_bugs')),
            ('Horthy', 'KRT', t('s_dk_horthy_idee'), ''),
            ('Bushwick4712', 'KRT',
             t('s_dk_bushwick_idee') + '\n\n' + t('s_dk_bushwick_idee2')
             + '\n\n' + t('s_dk_bushwick_idee3'),
             t('s_dk_bushwick_bugs')),
            ('YoshimitsuDE', 'KRT', t('s_dk_yoshimitsu_idee'), ''),
            ('Zwaersch', 'KRT', t('s_dk_zwaersch_idee'),
             t('s_dk_zwaersch_bugs') + '\n\n' + t('s_dk_zwaersch_bugs2')),
            ('Blackd0g84', 'KRT', t('s_dk_blackdog_idee'), '')):
        _contributor(fenster, innen, name, gruppe, idee, funde)

    # --- Marken ---
    _body_text(innen, t('s_dk_marken'), fenster.f_small, fill='x',
                pady=(18, 6))

    # --- Star Citizen Fan Content ---
    # ⚠ Gehoert ins Programm, nicht nur in die README: Wer ein Werkzeug
    # benutzt, liest die README meist nie. Der Wortlaut folgt dem Fankit
    # Agreement und dem UGC-Abschnitt der RSI-Nutzungsbedingungen.
    tk.Label(innen, text=t('s_dk_fankit_kopf'), bg=BG, fg=FG,
             font=fenster.f_base, anchor='w').pack(fill='x', pady=(12, 2))
    _body_text(innen, t('s_dk_fankit'), fenster.f_small, fill='x',
                pady=(0, 20))


def _about(fenster, rahmen):
    from . import pfade
    _heading(fenster, rahmen, t('hf_ueber'), t('s_ub_lead'))
    innen = _scroll_area(rahmen)

    # --- Zustand ---
    # ⚠ Mit dem Programmsymbol daneben. Es stand hier nie — und seit der
    # Autor-Block mit dem Avatar auf „Danke & Lizenzen" gewandert ist, hatte die
    # Seite gar kein Bild mehr und wirkte nackt. Gemeldet am 27.08.2026: „bei
    # über muss oben zur Version noch das Watcher Logo (icon)".
    karte = _card(innen, pady=(0, 6))
    kopf = tk.Frame(karte, bg=SURFACE)
    kopf.pack(fill='x', padx=16, pady=(14, 6))
    from .main_window import _bundled
    symbol = _bundled(os.path.join('assets', 'icon.png'))
    if symbol and os.path.exists(symbol):
        try:
            voll = tk.PhotoImage(file=symbol)
            # `subsample` verkleinert nur ganzzahlig — 48 px ist die Größe, die
            # neben zwei Textzeilen sitzt, ohne die Karte auseinanderzuziehen.
            teiler = max(1, voll.width() // 48)
            fenster._about_logo = voll.subsample(teiler, teiler)
            tk.Label(kopf, image=fenster._about_logo, bg=SURFACE).pack(
                side='left', padx=(0, 14))
        except Exception as ausnahme:
            fehler.merken('seiten.ueber.symbol', ausnahme)
    titel = tk.Frame(kopf, bg=SURFACE)
    titel.pack(side='left', fill='x', expand=True)
    # ⚠ Produktname aus `sprache.py` — nie fest hier. Bei der Umbenennung zu
    # VerseKit (12.09.2026) stand er an vier Stellen hart im Code und wäre
    # teils alt geblieben.
    tk.Label(titel, text=t('hf_titel'), bg=SURFACE, fg=FG,
             font=fenster.f_title, anchor='w').pack(fill='x')
    tk.Label(titel, text=fenster.version or '—', bg=SURFACE, fg=ACCENT,
             font=fenster.f_bold, anchor='w').pack(fill='x')

    tk.Frame(karte, bg=SURFACE, height=8).pack()
    _value_row(fenster, karte, t('s_ub_bekannt'), _count_catalog())
    _value_row(fenster, karte, t('s_ub_davon'), _count_collection())
    uebersicht = {}
    try:
        uebersicht = pfade.uebersicht() or {}
    except Exception:
        pass
    _value_row(fenster, karte, t('b_ordner'),
               uebersicht.get('app_ordner') or '—')
    tk.Frame(karte, bg=SURFACE, height=10).pack()

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
    _body_text(innen, t('s_up_sofort_h'), fenster.f_small,
                pady=(10, 6))
    _button(fenster, innen, t('s_up_sofort'),
           lambda: _fetch_version(fenster, True),
           strong=True).pack(fill='x', pady=(0, 10))

    reihe = tk.Frame(innen, bg=BG)
    reihe.pack(fill='x', pady=(4, 4))
    _button_row(reihe, [
        # ⚠ Nicht mehr "stark": Der hervorgehobene Knopf der Seite ist jetzt
        # der Hol-Knopf darüber. Zwei starke Knöpfe nebeneinander heben sich
        # gegenseitig auf — dann sticht keiner mehr hervor.
        _button(fenster, reihe, t('s_ub_nachsehen'),
               lambda: _check_now(fenster)),
        _button(fenster, reihe, t('hf_wasistneu'),
               lambda: fenster.jump_to('wasistneu')),
        _button(fenster, reihe, t('s_ub_einrichtung'), fenster._open_wizard),
    ])

    # --- Testkanal: zwei Kästen statt eines Schalters ---
    tk.Label(innen, text=t('s_ub_kanal'), bg=BG, fg=FG,
             font=fenster.f_title, anchor='w').pack(fill='x', pady=(24, 2))
    _body_text(innen, t('s_ub_kanal_h'), fenster.f_small,
                fill='x', pady=(0, 12))

    kaesten = tk.Frame(innen, bg=BG)
    kaesten.pack(fill='x')

    def kanal_setzen(wert):
        pfade.einstellung_setzen('vorabversionen', wert)
        fenster.say(t('e_vorab') + ': ' + (t('e_an') if wert else t('e_aus')))
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
        _channel_box(fenster, kaesten, t('s_ub_fertig'), t('s_ub_fertig_h'),
                     not an, lambda: kanal_setzen(False), untereinander=eng,
                     platz=0,
                     holen=lambda: _fetch_version(fenster, False),
                     holen_text=_fetch_label(False, fenster.version),
                     holen_aktiv=_can_fetch(False, fenster.version))
        _channel_box(fenster, kaesten, t('s_ub_test'), t('s_ub_test_h'),
                     an, lambda: kanal_setzen(True), marke_text='rc',
                     untereinander=eng, platz=1,
                     holen=lambda: _fetch_version(fenster, True),
                     holen_text=_fetch_label(True, fenster.version),
                     holen_aktiv=_can_fetch(True, fenster.version))
        # ⚠ Die Beschriftungen kommen aus dem Zwischenspeicher, damit die Seite
        # sofort steht. Der frischt sich aber nur einmal am Tag auf — auf einem
        # Bildschirmfoto vom 25.08.2026 bot der Knopf „v3.0.0-rc9 holen" an,
        # während rc12 lief und rc13 schon draußen war. Der Knopf holt zwar die
        # richtige Version (er sieht vorher nach), aber was draufsteht, führt in
        # die Irre. Deshalb einmal im Hintergrund nachsehen und die Kästen neu
        # zeichnen, wenn sich etwas geändert hat.
        _refresh_channels(fenster, kaesten, kanal_zeichnen)

    # ⚠ Der Tagesschalter steht **hinter** den Kanal-Kästen, nicht davor. Davor
    # drückte er die Kästen bei der Mindestgröße des Fensters unter die Kante —
    # und in ihnen sitzt der Knopf, mit dem man die stabile Version holt.
    # Gemeldet am 27.08.2026: „bei der stable version dann bitte auch."
    # Der Schalter ist eine Nebeneinstellung, die Kästen sind der Zweck der
    # Seite; also gehören sie nach oben.
    ziel = _setting_row(fenster, innen, t('s_ub_taeglich'), t('s_ub_taeglich_h'))
    _switch(fenster, ziel, 'update_pruefen', True)

    def kanal_pruefen(_=None):
        """Nur neu bauen, wenn die Anordnung wirklich kippt — sonst flackert es."""
        eng = kaesten.winfo_width() < SCHMAL
        if eng != getattr(kaesten, 'zuletzt_eng', None):
            for kind in kaesten.winfo_children():
                kind.destroy()
            kanal_zeichnen()

    kanal_zeichnen()
    kaesten.bind('<Configure>', kanal_pruefen, add='+')



def _link(fenster, eltern, text, ziel, grund=None):
    """Eine anklickbare Adresse — öffnet den Browser.

    ⚠ Vorher war das ein gewöhnliches Label in der Akzentfarbe: Es **sah aus wie
    ein Link** und tat nichts. Das ist schlimmer als schwarzer Text, weil es zum
    Klicken einlädt. Jetzt ist der Mauszeiger eine Hand, die Adresse unterstreicht
    sich beim Überfahren, und ein Klick öffnet sie.
    """
    grund = grund or SURFACE
    lbl = tk.Label(eltern, text=text, bg=grund, fg=ACCENT, font=fenster.f_small,
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
        fenster.say(t('s_ub_auf') % ziel if geklappt else t('s_ub_auf_nein') % ziel)

    def rein(_=None):
        lbl.configure(font=_underlined(fenster.f_small))

    def raus(_=None):
        lbl.configure(font=fenster.f_small)

    lbl.bind('<Button-1>', oeffnen)
    lbl.bind('<Enter>', rein)
    lbl.bind('<Leave>', raus)
    return lbl


def _underlined(schrift):
    """Dieselbe Schrift, nur unterstrichen — für die Maus-über-Anzeige."""
    import tkinter.font as tkfont
    try:
        kopie = tkfont.Font(font=schrift)
        kopie.configure(underline=True)
        return kopie
    except tk.TclError:
        return schrift


def _switch(fenster, eltern, schluessel, standard):
    """Ein An/Aus-Schalter, der sofort schreibt — es gibt keinen Speichern-Knopf."""
    from . import pfade
    k = tk.Label(eltern, text='', bg=SURFACE, font=fenster.f_small,
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
        fenster.say(t('e_an') if neu else t('e_aus'))

    k.bind('<Button-1>', lambda e: umschalten())
    zeichnen()
    return k


def _detection(fenster, rahmen):
    from . import catalog as katalog_modul, pfade, phrasen
    _heading(fenster, rahmen, t('hf_erkennung'), t('s_er_lead'))
    innen = _scroll_area(rahmen)

    ziel = _setting_row(fenster, innen, t('s_er_takt'), t('s_er_takt_h'))
    reihe = tk.Frame(ziel, bg=BG)
    reihe.pack()
    from .main_window import round_entry
    zahl = round_entry(reihe, None, fenster.f_small, '#0c1017', LINE, ACCENT, FG,
                       width=5, justify='right')
    zahl.insert(0, str(pfade.einstellung_zahl('pruefintervall_sekunden', 3, 1, 60)))
    zahl.holder.pack(side='left')
    tk.Label(reihe, text=t('s_er_sek'), bg=BG, fg=SUB,
             font=fenster.f_small).pack(side='left')

    def takt_merken(_=None):
        try:
            pfade.einstellung_setzen('pruefintervall_sekunden',
                                     max(1, min(60, int(zahl.get()))))
            fenster.say(t('s_er_takt_sagen') % zahl.get())
        except ValueError:
            pass

    zahl.bind('<FocusOut>', takt_merken)
    zahl.bind('<Return>', takt_merken)

    # ⚠ `breit=True`: Die gefundenen Sätze sind lang. Rechts neben der
    # Beschreibung lief der Kasten über die Fensterkante hinaus und war an
    # beiden Enden abgeschnitten — lesbar war weder Anfang noch Ende.
    ziel = _setting_row(fenster, innen, t('s_er_satz'), t('s_er_satz_h'),
                 wide=True)
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
    kasten = _card(ziel)
    _body_text(kasten, gefunden, fenster.f_small, color=FG,
                bg=SURFACE, inset=24, fill='x', padx=12, pady=8)

    ziel = _setting_row(fenster, innen, t('s_er_kat'), t('s_er_kat_h'))

    def katalog_neu():
        fenster.say(t('s_er_kat_holt'))
        try:
            katalog_modul.update()
            fenster.say(t('s_er_kat_da') % _count_catalog())
        except Exception as ausnahme:
            fehler.merken('seiten.erkennung.katalog', ausnahme)
            fenster.say(t('s_er_kat_weg'))

    _button(fenster, ziel, t('s_er_kat_jetzt'), katalog_neu).pack()

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


def _diagnostics(fenster, rahmen):
    from . import pfade
    from .main_window import toggle_switch
    _heading(fenster, rahmen, t('hf_diagnose'), t('s_di_lead'))
    innen = _scroll_area(rahmen)

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
    ziel_melder = _setting_row(fenster, innen, t('s_melder'), t('s_melder_h'))
    from .main_window import round_entry
    melder_feld = round_entry(ziel_melder, melder_var, fenster.f_small,
                              '#0c1017', LINE, ACCENT, FG,
                              placeholder=t('s_pl_melder'))
    melder_feld.holder.pack(fill='x', pady=(8, 0))

    # ⭐⭐ **Ein Feld für die Meldung selbst — direkt unter dem Namen.**
    # Am 05.09.2026 schrieb Bushwick4712 seine Meldung („mission log updated
    # nicht") in das **Namensfeld**, weil es das einzige war, in das man etwas
    # tippen konnte. Der Bericht kam damit als „BUSHWICK mission log updated
    # niocht" an — der Hinweis war da, aber an der falschen Stelle, und wäre
    # bei einem längeren Satz abgeschnitten worden.
    #
    # ⚠ **Nicht gespeichert.** Anders als der Name gehört ein Satz zu *einem*
    # Bericht; beim nächsten Öffnen stünde er sonst noch da und würde
    # versehentlich zu einer zweiten Meldung mitgeschickt.
    # ⚠⚠ **Mehrzeilig und über die volle Breite, unter der Erklärung.** Der
    # erste Anlauf setzte ein einzeiliges Feld rechts neben den Text — halb so
    # breit wie die Seite, eine Zeile hoch. Wer zwei Sätze tippte, sah nur das
    # Ende und konnte vor dem Absenden nicht mehr nachlesen, was er meldet.
    # Am 05.09.2026 gemeldet: „Wir haben da ja noch ne Menge Platz, macht es
    # nicht Sinn das Fenster … größer und unter den Text zu machen, das der
    # Melder das was er eintippt auch noch selber lesen kann?"
    #
    # Genau dafür ist `breit=True` da (siehe `_feld`): unter die Beschreibung
    # statt daneben, volle Breite. Der Name bleibt bewusst einzeilig rechts —
    # ein Name ist kurz, und ein vierzeiliges Feld dafür sähe albern aus.
    #
    # ⚠ **Nicht gespeichert.** Anders als der Name gehört ein Satz zu *einem*
    # Bericht; beim nächsten Öffnen stünde er sonst noch da und würde
    # versehentlich zu einer zweiten Meldung mitgeschickt.
    from .main_window import round_textarea
    ziel_meldung = _setting_row(fenster, innen, t('s_meldung'), t('s_meldung_h'),
                         wide=True)
    meldung_feld = round_textarea(ziel_meldung, fenster.f_small,
                                   '#0c1017', LINE, ACCENT, FG, rows=4)
    meldung_feld.holder.pack(fill='x', pady=(8, 0))

    def meldung_text():
        """Was gerade im Feld steht — ohne den Zeilenumbruch am Ende.

        ⚠ Ein `Text` hängt immer ein `\\n` an. Ohne das Abschneiden stünde im
        Bericht eine Leerzeile mehr, und `strip()` an fünf Aufrufstellen zu
        wiederholen ist genau die Sorte Wissen, die beim sechsten vergessen
        wird.
        """
        try:
            return meldung_feld.get('1.0', 'end-1c').strip()
        except Exception:
            return ''

    text = ''
    try:
        text = bericht.bauen(version=fenster.version, wurzel=fenster.root)
    except Exception as ausnahme:
        fehler.merken('seiten.diagnose', ausnahme)

    from .main_window import round_frame
    kasten = round_frame(innen, '#0c1017', LINE, radius=8, base_color=BG)
    kasten.holder.pack(fill='both', expand=True)
    # ⚠ `highlightthickness` steht bei Text und Entry auf 1 und wird auf dem
    # Mac als helle Linie gezeichnet — im runden Kasten sah das aus wie ein
    # zweiter, eckiger Rahmen. `relief='flat'` und `bd=0` schalten das NICHT ab.
    feld = tk.Text(kasten, bg='#0c1017', fg=FG, font=('Consolas', 10),
                   height=16, wrap='none', relief='flat', bd=0,
                   highlightthickness=0, insertbackground=FG, padx=14, pady=12)
    feld.pack(fill='both', expand=True)
    feld.insert('1.0', text)
    feld.configure(state='disabled')

    def _bericht_neu():
        """Den Bericht neu aufbauen — mit dem, was gerade in den Feldern steht.

        ⚠ Der Bericht wird beim Öffnen der Seite EINMAL gebaut. Ohne dieses
        Auffrischen stünde der eben eingetippte Text nicht darin — man sähe
        „nicht angegeben" und hielte das Feld für kaputt."""
        try:
            frisch = bericht.bauen(version=fenster.version,
                                   wurzel=fenster.root,
                                   meldung=meldung_text())
        except Exception as ausnahme:
            fehler.merken('seiten.diagnose_melder', ausnahme)
            return
        feld.configure(state='normal')
        feld.delete('1.0', 'end')
        feld.insert('1.0', frisch)
        feld.configure(state='disabled')

    def melder_uebernehmen(*_):
        """Den Namen sichern — er gilt dauerhaft, anders als die Meldung."""
        neu_wert = melder_var.get().strip()
        if neu_wert != (pfade.einstellung('melder_name') or ''):
            pfade.einstellung_setzen('melder_name', neu_wert)
        _bericht_neu()

    melder_feld.bind('<FocusOut>', melder_uebernehmen)
    melder_feld.bind('<Return>', melder_uebernehmen)
    # ⚠ Die Meldung wird **nicht** gespeichert — sie gehört zu diesem einen
    # Bericht. Deshalb nur den Text neu bauen, nichts ablegen.
    # ⚠ **Kein `<Return>` mehr.** Im einzeiligen Feld war die Eingabetaste das
    # Bestätigen; in einem mehrzeiligen Feld ist sie der Zeilenumbruch. Wer
    # hier bindet, nimmt dem Melder die Absätze weg — bei einer
    # Fehlerbeschreibung genau das Falsche.
    #
    # Gebraucht wird sie auch nicht: `<FocusOut>` feuert, sobald man irgendwo
    # hin klickt, und jeder der drei Knöpfe holt sich den Text ohnehin frisch
    # über `aktueller_bericht()`.
    meldung_feld.bind('<FocusOut>', lambda _=None: _bericht_neu())

    # ⚠⚠ **Beim erneuten Öffnen den Bericht neu bauen.** Er enthält die eigene
    # Bauplan-Zahl, und die ändert sich beim Spielen. Ohne das stünde in einem
    # Bericht vom Abend der Bestand vom Morgen — und der Entwickler sucht einen
    # Fehler an einer Zahl, die längst anders ist.
    #
    # ⚠ **Diese Seite wird bewusst NICHT verworfen** wie die übrigen Seiten mit
    # Bestandszahlen (`main_window.STOCK_PAGES`). Ein Neubau würde das
    # Meldungsfeld leeren — jemand tippt seine Fehlerbeschreibung, wechselt
    # kurz auf eine andere Seite, um etwas nachzusehen, und der Text ist weg.
    # Genau auf dieser Seite darf das am wenigsten passieren.
    fenster.on_show['diagnose'] = _bericht_neu

    # ⭐ **Die Zusicherung steht zwischen Bericht und Knöpfen** — genau dort,
    # wo die Entscheidung fällt. Sie stand bis zum 05.09.2026 *unter* der
    # Knopfreihe, also hinter dem Klick, und konnte am unteren Rand
    # wegfallen. Gemeldet mit der Frage, ob sie nicht besser nach oben
    # gehöre, „damit sie nicht aus Versehen abgeschnitten wird".
    #
    # ⚠ **Nach oben aber nicht.** Ihr eigener Text lautet „Der Block **oben**
    # ist der ganze Inhalt" — über dem Bericht stimmte der Bezug nicht mehr.
    # Und die Knöpfe gehören hinter den Bericht: Erst sehen, was rausgeht,
    # dann der Knopf. Ein Absende-Knopf über dem Inhalt lädt zum blinden
    # Klicken ein, und das ist bei einem Knopf, der etwas ins Netz schickt,
    # das Letzte, was man will.
    #
    # Hier kann sie auch nicht mehr abgeschnitten werden, ohne dass die
    # Knöpfe gleich mit verschwinden — und die sucht jeder.
    _status(fenster, innen, 'haken', t('s_di_sicher'), t('s_di_sicher_h'))

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

    def _meldung_verbraucht():
        """Das Feld „Was ist passiert?" leeren — der Satz ist raus.

        ⚠⚠ **Alle drei Knöpfe, nicht nur „Absenden".** Beim ersten Anlauf hing
        das nur am Absenden. „Angaben kopieren" und „Melden" geben den Bericht
        aber genauso weiter — beim einen in die Zwischenablage, beim anderen
        ins Meldeformular. Wer so meldet, hätte seinen Satz eine Woche später
        unbemerkt am nächsten Bericht hängen.

        ⚠ **Der Kasten bleibt stehen, wie er ist.** Nur das Eingabefeld wird
        geleert. Wer zweimal kopiert — erst in den Chat, dann ins Formular —
        bekommt beide Male denselben vollständigen Bericht; würde der Kasten
        mitgeleert, fehlte beim zweiten Mal genau der Satz, um den es geht.
        Beim nächsten Öffnen der Seite baut `beim_zeigen['diagnose']` ihn
        ohnehin frisch auf, dann ist er auch dort weg.
        """
        if meldung_text():
            try:
                meldung_feld.delete('1.0', 'end')
            except Exception as ausnahme:
                fehler.merken('seiten.diagnose_meldung_leeren', ausnahme)

    def melden():
        if bericht.issue_oeffnen(aktueller_bericht()):
            fenster.say(t('s_di_browser_ok'))
            _meldung_verbraucht()
        else:
            fenster.say(t('s_di_browser_weg'))

    def kopieren():
        if bericht.in_die_ablage(aktueller_bericht(), fenster.root):
            fenster.say(t('s_di_kopiert'))
            _meldung_verbraucht()

    def absenden():
        """Auf Knopfdruck an den Entwickler — mit vorheriger Rückfrage.

        ⚠ Der Weg für alle, die nicht basteln wollen. Kopieren und in Discord
        einfügen scheitert daran, dass der Bericht zu lang ist und man wissen
        muss, wohin damit. Gemeldet am 28.08.2026: „ich will nicht jedem eine
        Stunde erklären, wie ich zu dem Bericht komme."

        Gefragt wird trotzdem: Etwas ins Netz zu schicken, ohne dass jemand
        zugestimmt hat, macht dieses Werkzeug nicht.
        """
        from .main_window import ask_yes_no
        if not ask_yes_no(fenster.root, t('s_di_ab_frage_t'),
                             t('s_di_ab_frage')):
            return
        fenster.say(t('s_di_ab_laeuft'))
        fenster.root.update_idletasks()
        geklappt, grund = bericht.absenden(aktueller_bericht(), fenster.version)
        fenster.say(t('s_di_ab_ok') if geklappt
                      else t('s_di_ab_weg') % grund)
        # ⚠ **Nur bei Erfolg.** Scheitert das Senden — kein Netz, Dienst weg —,
        # bleibt der Text stehen. Ihn dann zu löschen hieße, dem Melder seine
        # Arbeit wegzunehmen, genau in dem Moment, in dem er es noch einmal
        # versuchen will.
        if geklappt:
            _meldung_verbraucht()

    # ⚠ Ganz vorn und in Rot: Wer hier landet, hat ein Problem und sucht den
    # kürzesten Weg.
    #
    # ⚠ **Immer zeigen, auch ohne eingebautes Ziel.** Der erste Anlauf blendete
    # ihn aus, wenn nicht gesendet werden kann — gedacht als „ein Knopf, der
    # nichts tut, ist schlimmer als keiner". In der Praxis trifft das nur den
    # Quellcode, also den Entwickler selbst. am 28.08.2026 gemeldet vor der
    # Diagnose-Seite: „nicht mal ICH finde den." Ein Knopf, der fehlt, sieht aus
    # wie ein Fehler; einer, der beim Drücken sagt, was ihm fehlt, erklärt sich.
    # ⚠ **Drei Knöpfe, nicht fünf.** „Als Datei speichern" und „Eigenen Ordner
    # öffnen" sind am 05.09.2026 gestrichen worden: In über einem Jahr hat sie
    # niemand benutzt. Beide erzeugen Arbeit statt sie abzunehmen — wer den
    # Bericht abschickt oder kopiert, ist fertig; wer ihn als Datei ablegt,
    # muss ihn danach noch irgendwohin bringen.
    #
    # ⚠ Der Bericht ist damit **nicht** unerreichbar: „In die Ablage kopieren"
    # gibt denselben Text, und wer ihn wirklich als Datei braucht, fügt ihn ein
    # und speichert dort. Ein Knopf für einen Zwischenschritt, den niemand geht,
    # ist Ballast auf einer Seite, auf der man ohnehin schon Ärger hat.
    _button_row(reihe, [
        _button(fenster, reihe, t('s_di_absenden'), absenden, danger=True),
        _button(fenster, reihe, t('s_di_melden'), melden, strong=True),
        _button(fenster, reihe, t('s_di_kopieren'), kopieren),
    ])

    ziel = _setting_row(fenster, innen, t('s_di_mit'), t('s_di_mit_h'))

    def mitschreiben_um():
        neu_wert = not pfade.einstellung_wahrheit('fehler_mitschreiben', True)
        pfade.einstellung_setzen('fehler_mitschreiben', neu_wert)
        return neu_wert

    toggle_switch(ziel, pfade.einstellung_wahrheit('fehler_mitschreiben', True),
                    mitschreiben_um).pack()



def _count_catalog():
    try:
        return len((katalog_modul.load().get('bauplaene') or {}))
    except Exception:
        return '—'


def _count_collection():
    try:
        return len((bestand_datei.load().get('bauplaene') or {}))
    except Exception:
        return '—'


# --------------------------------------------------------------- Herstellung
#
# ⚠ **Nicht scmdb nachbauen.** Die Seite beantwortet genau eine Frage: „Ich will
# das bauen — was brauche ich?" Keine Wahrscheinlichkeits-Balken, kein
# Refinery-Vergleich; wer das braucht, ist auf scmdb.net besser aufgehoben.
# Was diese Seite dagegen kann und die Webseite nicht: Sie **weiß**, welche
# Baupläne der Spieler hat.

CRAFT_MAX = 150          # so viele Zeilen auf einmal — mehr macht Tk zäh


def _duration(seconds):
    """Herstellzeit lesbar: 45 s · 16 min · 2 h 30 min."""
    seconds = int(seconds or 0)
    if seconds < 60:
        return t('s_he_sekunden') % seconds
    if seconds < 3600:
        return t('s_he_minuten') % round(seconds / 60.0)
    return t('s_he_std_min') % (seconds // 3600, (seconds % 3600) // 60)


def _crafting(fenster, rahmen):
    """Alle herstellbaren Gegenstände, mit Rezept auf Klick."""
    from . import crafting as herst_modul
    _heading(fenster, rahmen, t('hf_herstellung'), t('s_he_lead'))
    innen = _scroll_area(rahmen)

    try:
        habe = bestand_datei.keys(bestand_datei.load())
        eintraege = herst_modul.with_collection(habe)
        sicher, gesamt, unklar = herst_modul.counts(habe)
    except Exception as ausnahme:
        fehler.merken('seiten.herstellung', ausnahme)
        eintraege, sicher, gesamt, unklar = [], 0, 0, 0

    if not eintraege:
        _body_text(innen, t('s_he_keine_daten'), fenster.f_small, fill='x')
        return

    # Kopfzahl im selben Aufbau wie der Bauplan-Fortschritt — wer die eine
    # Seite kennt, liest die andere sofort.
    kopf = tk.Frame(innen, bg=BG)
    kopf.pack(fill='x', pady=(0, 4))
    tk.Label(kopf, text=str(sicher), bg=BG, fg=ACCENT,
             font=fenster.f_title).pack(side='left')
    tk.Label(kopf, text=t('s_he_von') % gesamt, bg=BG, fg=SUB,
             font=fenster.f_small).pack(side='left')
    # ⚠⚠ **Die unklaren gehören dazu, sonst fehlt eine Zahl ohne Erklärung.**
    # `counts()` gibt sie längst zurück, angezeigt wurden sie nie: Ein
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
                 font=fenster.f_small).pack(side='left')

    from .main_window import round_bar, round_entry
    round_bar(innen, 9, sicher / float(gesamt or 1), BG, '#222b3b',
               ACCENT).pack(fill='x', pady=(6, 14))

    # ⚠ Beschriftetes Feld wie auf den anderen Seiten (siehe „Dein Name" auf
    # der Diagnose-Seite) — **nicht** das nackte Suchfeld aus der Werkzeugleiste
    # der Bauplan-Liste. Dort gibt die Leiste den Kontext, hier gäbe ein leeres
    # Kästchen mitten auf der Seite keinen Hinweis, wofür es da ist.
    # ⭐ Der Sprung aus der Bauplan-Liste setzt hier den Namen hinein — genau
    # wie `mining_search` beim Rohstoff-Sprung. Danach wieder leeren, sonst
    # stünde der Begriff beim nächsten Öffnen erneut da.
    #
    # ⚠ `gesprungen` wird weiter unten gebraucht, um die Zeile gleich
    # aufgeklappt zu zeigen — deshalb hier gemerkt und nicht nur ins Suchfeld
    # geschrieben.
    gesprungen = getattr(fenster, 'crafting_search', '') or ''
    suche_var = tk.StringVar(value=gesprungen)
    fenster.crafting_search = ''
    ziel_suche = _setting_row(fenster, innen, t('s_he_suche'), '')
    suchfeld = round_entry(ziel_suche, suche_var, fenster.f_small, '#0c1017',
                           LINE, ACCENT, FG, placeholder=t('s_pl_herstellung'))
    suchfeld.holder.pack(fill='x', pady=(4, 12))
    # ⚠ Gleiches Bedienelement wie beim Bergbau. Zwei Suchfelder, die sich
    # unterschiedlich verhalten, sind schlimmer als eines ohne Kreuz.
    _search_clear(fenster, ziel_suche, suche_var)
    def _herst_frisch():
        """Beim erneuten Aufrufen ohne Filter anfangen.

        ⚠⚠ **Nur wenn wirklich etwas gesetzt war.** Sonst baut jeder Wechsel
        auf die Herstellungs-Seite die 1597 Zeilen neu auf, ohne dass sich
        etwas ändert — dieselbe Bremse wie in der Bauplan-Liste
        (`bestandsfenster._fein_leeren`), am 31.08.2026 gemessen und gemeldet.
        """
        # ⚠⚠ **Ein Sprung aus der Bauplan-Liste darf hier NICHT geleert
        # werden.** Beim ersten Mal wird die Seite frisch gebaut und nimmt den
        # Namen im Aufbau entgegen — beim zweiten Mal existiert sie schon, und
        # dann läuft nur noch dieser Rückruf. Ohne die Abfrage hätte der
        # Sprung genau einmal funktioniert und danach nie wieder.
        neuer_sprung = getattr(fenster, 'crafting_search', '') or ''
        if neuer_sprung:
            fenster.crafting_search = ''
            for schluessel in wahl:
                wahl[schluessel] = ''
            _material_merker.clear()
            # Gleich aufgeklappt zeigen — der Grund des Sprungs sind ja die
            # Zutaten.
            offen['name'] = neuer_sprung
            filter_bauen()
            suche_var.set(neuer_sprung)      # löst `zeichnen()` über den trace aus
            return

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
        offen['name'] = None
        filter_bauen()
        zeichnen()

    fenster.on_show['herstellung'] = _herst_frisch

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
    from . import categories as kat_modul
    from . import catalog as kat_daten

    _kat_arten = {}
    try:
        for _k, _v in (kat_daten.load().get('bauplaene') or {}).items():
            _kat_arten[herst_modul._key(_v.get('n') or '')] = _v.get('a') or ''
    except Exception as ausnahme:
        fehler.merken('seiten.crafting.katalog', ausnahme)

    _kat_merker = {}

    def _kategorie(e):
        name = e.get('basis') or e.get('name') or ''
        if name in _kat_merker:
            return _kat_merker[name]
        b = herst_modul.recipe_raw(name) or {}
        wert = kat_modul.classify(
            art=_kat_arten.get(herst_modul._key(name), ''),
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
            name = kat_modul.top_name(o)
            if not kat_modul.is_group(o):
                name = kat_daten.kind_readable(kat_modul.raw_kind(o)) or name
            raus.append((o, '%s (%d)' % (name, n), kat_modul.is_group(o), name))
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
        return [(u, '%s (%d)' % (kat_modul.sub_name(u), n))
                for u, n in sorted(zaehler.items(),
                                   key=lambda q: kat_modul.sub_name(q[0]).lower())]

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
        _filter_bar(fenster, filter_rahmen, felder, gewechselt, wahl)

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
    # ⭐ **Wer aus der Bauplan-Liste herspringt, will die Zutaten SEHEN** —
    # nicht erst noch einmal klicken. Gemeldet am 07.09.2026 nach dem Test von
    # v3.26.0: „das Teil ist zugeklappt, geht es dass das direkt aufgeklappt
    # ist, damit man nicht einen extra Klick hat?" — mit Verweis auf die
    # Drei-Klick-Regel.
    #
    # ⚠ Nur beim Sprung, nicht beim gewöhnlichen Öffnen der Seite: Dort weiß
    # niemand, welche der 1597 Zeilen aufgehen sollte.
    offen = {'name': gesprungen or None}

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
            rez = herst_modul.recipe(name) or {}
            stufen = rez.get('stufen') or []
            if stufen:
                from . import materials as lager_modul
                fehlt = [z for z in lager_modul.check(stufen[0]['zutaten'])
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

        # ⭐ **Ohne Eingabe steht hier keine Liste** (07.09.2026). Vorher
        # standen 1597 Baupläne untereinander — mit Auswahlfeldern darüber,
        # die genau dafür da sind. Gemeldet mit „wir haben nen Dropdown, da
        # kann der Spieler ja schon auswählen, und die Liste scrollt eh
        # niemand durch".
        #
        # ⚠ Der Hinweis darf nicht fehlen: Eine Seite, die leer aufgeht und
        # nichts sagt, sieht kaputt aus.
        if not text and not any(wahl.values()):
            _body_text(liste_rahmen, t('s_he_erst_waehlen'), fenster.f_small,
                        fill='x')
            return

        # ⭐⭐ **Auch nach der ZUTAT suchen.** Bis v3.3.0-rc40 sah die Suche
        # nur auf Bauplan-Namen. Wer „ric" tippte, um zu sehen, was aus Riccite
        # wird, bekam „Lo*ric*a" und „Fab*ric*ation" — Zufallstreffer — und nie
        # die 84 Baupläne, die Riccite wirklich brauchen. Am 30.08.2026
        # gemeldet: „Was kann ich aus Sadaryx herstellen? Meine User werden es
        # nie erfahren."
        material_treffer, aus_material = [], set()
        if text:
            for name_ in herst_modul.storable():
                if text in name_.lower():
                    material_treffer.append(name_)
                    aus_material.update(herst_modul.blueprints_with(name_))

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
            anzahl = len(herst_modul.blueprints_with(name_))
            _body_text(liste_rahmen,
                        (t('s_he_aus') % (name_, anzahl) if anzahl
                         else t('s_he_aus_keine') % name_),
                        fenster.f_small, fill='x')

        if not treffer:
            if not material_treffer:
                _body_text(liste_rahmen, t('s_he_nichts'), fenster.f_small,
                            fill='x')
            return
        for e in treffer[:CRAFT_MAX]:
            _crafting_row(fenster, liste_rahmen, e, offen, zeichnen)
        if len(treffer) > CRAFT_MAX:
            _body_text(liste_rahmen, t('s_he_mehr') % (len(treffer) - CRAFT_MAX),
                        fenster.f_small, fill='x')

    suche_var.trace_add('write', zeichnen)
    zeichnen()


def _money(amount):
    """Ein Geldbetrag mit Tausenderpunkten — 22700 wird zu „22.700".

    ⚠ Ohne Trennung liest niemand fünfstellige Zahlen richtig: „145789" und
    „14578" sehen im Vorbeigehen gleich aus. Punkt statt Komma, weil das Spiel
    es so schreibt.
    """
    return '{:,.0f}'.format(float(amount or 0)).replace(',', '.')


def _auec(amount):
    """Ein Geldbetrag **mit Einheit** — „59.345 aUEC".

    ⚠⚠ **Eine nackte Zahl ist keine Auskunft.** Am 04.09.2026 stand im
    Routen-Reiter „59.345" ohne alles, und die Frage kam prompt: „was sind die
    59.345, Eier, Pfannkuchen?" Berechtigt — in einer Zeile mit SCU-Mengen und
    Entfernungen sagt eine blanke Zahl gar nichts.
    """
    return t('s_auec') % _money(amount)


def _has_source(name):
    """Kennt der Katalog diesen Bauplan — und weiss er, woher es ihn gibt?

    ⚠ Beides zusammen. Ein Katalogeintrag ohne `q` hat keine Bezugsquelle
    (59 Bauplaene, ueberwiegend Event-Belohnungen); ein Knopf dorthin fuehrte
    zu einer Seite, die nichts zu sagen hat.
    """
    if not name:
        return False
    try:
        from . import catalog as kat
        gesucht = bestand_datei.norm(name)
        for e in (kat.load().get('bauplaene') or {}).values():
            if bestand_datei.norm(e.get('n') or '') == gesucht:
                return bool(e.get('q'))
    except Exception:
        pass
    return False


def _to_contract(fenster, titel):
    """Zur Bauplan-Liste, gefiltert auf diesen Auftrag.

    Gerufen von „Was bringt am meisten?" und vom Auftrags-Protokoll.

    ⚠⚠ **Erst nachsehen, DANN die Seite wechseln.** Vorher stand `open_page`
    ganz oben: Wer einen Auftrag ohne Baupläne anklickte, landete trotzdem in
    der Liste — mit der alten Ansicht und einer Meldung darunter. Aus „Was
    bringt am meisten?" fiel das nie auf, dort stehen nur Aufträge MIT
    Bauplänen. Im Auftrags-Protokoll (ab 08.09.2026 anklickbar) sind es
    **178 von 419**, die keinen bringen — dort wäre es der Normalfall
    gewesen.
    """
    try:
        titel = (titel or '').strip()
        if not titel:
            return
        # Gegen den Katalog fragen, ohne die Seite anzufassen.
        #
        # ⛔⛔ **Über `catalog.blueprints_for_contract`, nicht mit einem eigenen
        # Vergleich.** Hier stand ein wörtlicher Titelvergleich gegen
        # `q['auftrag']` — und derselbe noch einmal in
        # `bestandsfenster.zum_auftrag()`. Beide trafen alles aus der eigenen
        # Liste und **nichts** aus dem Spiel: In den Herkunftsdaten steht
        # `'Stop Rival Attack at [LOCATION]'`, im Spiel
        # `'Stop Rival Attack at Asteroiden Bergbaubasis'`. 55 Baupläne, und
        # die Zeile war tot (gemeldet 13.09.2026).
        from . import catalog as kat_modul
        if not kat_modul.blueprints_for_contract(kat_modul.load(), titel):
            fenster.say(t('s_fo_lohnt_nichts'))
            return

        fenster.jump_to('liste')
        seite = getattr(fenster, 'stock_page', None)
        if seite is not None and seite.zum_auftrag(titel):
            return
        fenster.say(t('s_fo_lohnt_nichts'))
    except Exception as ausnahme:
        fehler.merken('seiten.zum_auftrag', ausnahme)


def _to_kind(fenster, art):
    """Vom Bauplan-Fortschritt zur Liste, gefiltert auf diese Kategorie."""
    try:
        fenster.jump_to('liste')
        seite = getattr(fenster, 'stock_page', None)
        if seite is not None and seite.zur_art(art):
            return
        fenster.say(t('s_fo_art_nichts') % art)
    except Exception as ausnahme:
        fehler.merken('seiten.zur_art', ausnahme)


def _to_blueprint(fenster, name):
    """Von der Herstellung zur Bauplan-Liste — mit aufgeschlagener Herkunft."""
    try:
        fenster.jump_to('liste')
        seite = getattr(fenster, 'stock_page', None)
        if seite is not None and seite.zum_bauplan(name):
            return
        fenster.say(t('s_he_woher_nichts'))
    except Exception as ausnahme:
        fehler.merken('seiten.zum_bauplan', ausnahme)
        fenster.say(t('s_he_woher_nichts'))


def _routes(fenster, rahmen):
    """Der Reiter „Routen": Wo stehe ich, wieviel passt rein — was lohnt sich?

    Gewünscht von **YoshimitsuDE** (04.09.2026).

    ⚠⚠ **Drei Eingaben, keine Automatik.** Das Spiel verrät nicht, wo der
    Spieler steht, was in seinem Laderaum liegt oder wieviel Geld er hat —
    nichts davon steht in der `Game.log`. Also wird gefragt, statt geraten.
    """
    from . import routes as routen_modul, ships as schiff_modul
    from . import selling as preisdaten

    _heading(fenster, rahmen, t('hf_routen'), t('s_rt_lead'))

    zustand = {'start': '', 'startname': '', 'laeuft': False, 'kurz': False,
               'schiff': '', 'stumm': False, 'stopps': 2, 'rund': False,
               'stumm_schiff': False, 'modus': 'ab_hier', 'ort_offen': False}
    # ⚠ 120 statt 96: Das ist der Laderaum der Freelancer MAX, gemessen am
    # 04.09.2026. Ein Standardwert soll einem echten Schiff entsprechen und
    # nicht geraten sein.
    scu_var = tk.StringVar(value='120')
    geld_var = tk.StringVar(value='500000')
    ortsuche = tk.StringVar()

    # ⚠⚠ **Die Eingaben bleiben stehen, nur das Ergebnis rollt.** Sie lagen
    # bis v3.15.1 **in** der Rollfläche — wer zu den Fahrten hinunterrollte,
    # verlor Startort, Frachtraum und Schiff aus dem Bild. Auf einem kleineren
    # Fenster fällt das sofort auf: Morkhan schickte am 05.09.2026 ein Bild,
    # auf dem die ganze Zeile „Wo stehst du gerade?" fehlte.
    #
    # ⚠ Derselbe Fehler war im Laden-Reiter schon behoben — und hier
    # übersehen. Wo eine Seite eine feste Kopfzone hat, brauchen die anderen
    # sie auch: Erst alles Feste packen, **danach** die rollende Fläche.
    kopf = tk.Frame(rahmen, bg=BG)
    kopf.pack(fill='x', side='top', padx=24, pady=(4, 0))

    # ---------------------------------------------------- Wo stehe ich?
    tk.Label(kopf, text=t('s_rt_wo'), bg=BG, fg=SUB,
             font=fenster.f_small, anchor='w').pack(fill='x')

    # ⭐ **Dropdown UND Suchfeld — wie überall im Werkzeug.** Wer den Namen
    # weiß, tippt; wer ihn nicht weiß, klappt das System auf und sieht alle
    # Handelsposten darin. Gewünscht am 04.09.2026: „mach noch Dropdown zum
    # Auswahlfeld bei ‚Wo stehst du gerade' bei Routen."
    ortzeile = tk.Frame(kopf, bg=BG)
    ortzeile.pack(fill='x')
    ortfeld = tk.Entry(ortzeile, textvariable=ortsuche, font=fenster.f_base,
                       bg=SURFACE, fg=FG, insertbackground=FG, relief='flat',
                       highlightthickness=1, highlightbackground=LINE,
                       highlightcolor=ACCENT)
    ortfeld.pack(side='left', fill='x', expand=True, ipady=5)
    fields.hint(ortfeld, ortsuche, t('s_rt_wo_platz'), normal=FG, grey=SUB)
    # ⚠⚠ **Nicht gepackt, solange leer.** Ein geleerter Rahmen behält seine
    # Höhe — gemessen 920 px bei null Kindern. Am 05.09.2026 im Routen-Reiter
    # gemeldet: „Oben entsteht mega viel Leerraum, ich scrolle, um nichts zu
    # sehen wegzubekommen." Genau dieser Rahmen und der für die Schiffe.
    ortvorschlag = tk.Frame(kopf, bg=BG)

    def _ortliste_leeren():
        for kind in ortvorschlag.winfo_children():
            kind.destroy()
        ortvorschlag.pack_forget()

    def _ortliste_zeigen():
        # ⚠⚠ **`after=` — sonst landet die Liste ganz unten.** Ein `pack()`
        # ohne Angabe hängt sich ans Ende des Rahmens; seit die Liste beim
        # Leeren ausgepackt wird, ist das nicht mehr ihr alter Platz. Am
        # 05.09.2026 gemeldet: „Oben kommt keine Auswahl mehr, wo ich Seraphim
        # Station auswählen könnte" — sie stand unter den Schaltern, weit weg
        # vom Suchfeld.
        ortvorschlag.pack(fill='x', after=ortzeile)

    def _systeme():
        """Die Systeme, in denen es Handelsposten gibt — mit Anzahl."""
        stellen = (preisdaten.load() or {}).get('terminals') or {}
        zaehler = {}
        for stelle in stellen.values():
            art = stelle.get('t')
            if art is not None and art not in preisdaten.TRADE_TYPES:
                continue
            system = (stelle.get('s') or '').strip()
            if system:
                zaehler[system] = zaehler.get(system, 0) + 1
        return [(s, '%s (%d)' % (s, n))
                for s, n in sorted(zaehler.items(),
                                   key=lambda p: (-p[1], p[0]))]

    def _system_gewechselt(wert):
        zustand['system'] = wert
        # ⚠ Das Feld leeren: Sonst filtert der alte Suchtext gegen das neue
        # System und die Liste bliebe leer, ohne dass man wüsste warum.
        zustand['stumm'] = True
        ortsuche.set('')
        zustand['stumm'] = False
        _ortvorschlaege()

    from .main_window import round_select
    systeme = _systeme()
    # ⚠ Gemerkt, damit „Zurücksetzen" die Anzeige mitnehmen kann — ohne das
    # stünde dort weiter „Stanton (128)", während intern schon alles offen ist.
    system_menue = [None]
    if systeme:
        system_menue[0] = round_select(
            ortzeile, [('', t('s_rt_alle_systeme'))] + systeme,
            zustand.get('system', ''), _system_gewechselt, fenster.f_small)
        system_menue[0].pack(side='left', padx=(8, 0))

    # ------------------------------------------- Frachtraum und Kapital
    zahlen = tk.Frame(kopf, bg=BG)
    zahlen.pack(fill='x', pady=(10, 0))
    for beschriftung, var, breite, beispiel in (
            (t('s_rt_scu'), scu_var, 8, t('s_pl_scu')),
            (t('s_rt_geld'), geld_var, 14, t('s_pl_geld'))):
        spalte = tk.Frame(zahlen, bg=BG)
        spalte.pack(side='left', padx=(0, 18))
        tk.Label(spalte, text=beschriftung, bg=BG, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(fill='x')
        zahlfeld = tk.Entry(spalte, textvariable=var, font=fenster.f_base,
                            width=breite, bg=SURFACE, fg=FG,
                            insertbackground=FG, relief='flat',
                            highlightthickness=1, highlightbackground=LINE,
                            highlightcolor=ACCENT)
        zahlfeld.pack(ipady=4)
        # ⚠ Der Hinweis nennt ein BEISPIEL, nicht die Beschriftung darüber —
        # „Frachtraum (SCU)" zweimal zu sagen hilft niemandem.
        fields.hint(zahlfeld, var, beispiel, normal=FG, grey=SUB)

    # ⭐⭐ **Schiff wählen statt Zahl tippen — als Suchfeld, nicht als Fenster.**
    #
    # ⚠ Der erste Anlauf war ein Knopf, der einen Auswahl-Dialog öffnete. Zwei
    # Fehler auf einmal, am 04.09.2026 gemeldet: Der Dialog zeigte nur sieben
    # von 134 Schiffen („leer und nur wenige auswählbar"), und ein eigenes
    # Fenster passt nicht zum Rest.
    #
    # Xharig dazu: „Suchfeld immer mit Auswahl-Dropdown bei 2 Buchstaben oder
    # so, so machen wir es auch in anderen Menüs im Tool." Genau so ist es
    # jetzt — dasselbe Muster wie beim Ortsfeld darüber.
    schiff_rahmen = tk.Frame(kopf, bg=BG)
    schiff_rahmen.pack(fill='x', pady=(10, 0))
    tk.Label(schiff_rahmen, text=t('s_rt_schiff'), bg=BG, fg=SUB,
             font=fenster.f_small, anchor='w').pack(fill='x')
    schiffsuche = tk.StringVar()
    schifffeld = tk.Entry(schiff_rahmen, textvariable=schiffsuche,
                          font=fenster.f_base, bg=SURFACE, fg=FG,
                          insertbackground=FG, relief='flat',
                          highlightthickness=1, highlightbackground=LINE,
                          highlightcolor=ACCENT)
    schifffeld.pack(fill='x', ipady=5)
    fields.hint(schifffeld, schiffsuche, t('s_rt_schiff_platz'),
                   normal=FG, grey=SUB)
    # ⭐⭐ **Eine Werft-Auswahl neben dem Suchfeld.** Am 05.09.2026: „Dropdown
    # hast du mir für Schiffe unter Routen versprochen — Spieler kennen ja
    # nicht alle Schiffe und deren SCU-Kapazität." Richtig: Ein Suchfeld, das
    # erst ab zwei Zeichen etwas zeigt, setzt voraus, dass man den Namen schon
    # kennt. Über die Werft kommt man auch ohne hin — dasselbe Muster wie im
    # Laden-Reiter, wo die Schiffe ebenfalls nach Werft stehen.
    werft_rahmen = tk.Frame(schiff_rahmen, bg=BG)
    werft_rahmen.pack(fill='x', pady=(6, 0))
    # Ebenfalls nur gepackt, wenn etwas darin steht — siehe `ortvorschlag`.
    schiffvorschlag = tk.Frame(schiff_rahmen, bg=BG)

    # Wo es das gewählte Schiff gibt — steht unter den Eingaben, nicht im
    # Ergebnis: Es gehört zur Ausrüstung, nicht zur Route.
    schiff_info = tk.Frame(kopf, bg=BG)
    schiff_info.pack(fill='x', pady=(8, 0))

    def _schiffliste_leeren():
        for kind in schiffvorschlag.winfo_children():
            kind.destroy()
        schiffvorschlag.pack_forget()

    def _schiffliste_zeigen():
        # ⚠ Direkt unter das Hersteller-Menü — siehe `_ortliste_zeigen`.
        schiffvorschlag.pack(fill='x', after=werft_rahmen)

    def _schiff_waehlen(name):
        zustand['schiff'] = name
        zustand['stumm_schiff'] = True
        schiffsuche.set(name)
        zustand['stumm_schiff'] = False
        _schiffliste_leeren()
        scu_var.set(str(schiff_modul.scu(name)))
        _schiff_zeigen()

    werft_wahl = {'werft': ''}

    def _schiffvorschlaege(*_a):
        if zustand.get('stumm_schiff'):
            return
        _schiffliste_leeren()
        text = schiffsuche.get().strip().lower()
        # ⚠ Ohne Suchtext gilt die Werft — sonst passiert beim Klicken nichts.
        if len(text) < 2 and not werft_wahl['werft']:
            return
        if text and text == (zustand.get('schiff') or '').lower():
            return
        flotte = schiff_modul.with_cargo()
        if not flotte:
            _schiffliste_zeigen()
            _body_text(schiffvorschlag, t('s_rt_keine_schiffe'),
                        fenster.f_small, fill='x')
            return
        treffer = [s for s in flotte
                   if (not werft_wahl['werft']
                       or s['werft'] == werft_wahl['werft'])
                   and (not text or text in s['name'].lower())]
        # ⚠ Wer eine Werft gewählt hat, sieht sie ganz — höchstens 25 Schiffe.
        # Beim Tippen bleibt es bei zehn, sonst schiebt die Liste die
        # Ergebnisse darunter aus dem Bild.
        treffer = treffer[:25 if werft_wahl['werft'] else 10]
        if not treffer:
            _schiffliste_zeigen()
            _body_text(schiffvorschlag, t('s_ld_nichts_gefunden'),
                        fenster.f_small, fill='x')
            return
        _schiffliste_zeigen()
        # ⚠ Der Frachtraum steht in der Zeile — sonst wählt man nach dem Namen
        # und weiß hinterher nicht, was man bekommen hat.
        for s in treffer:
            zeile = tk.Label(schiffvorschlag,
                             text='  %s  ·  %s' % (s['name'],
                                                   t('s_rt_scu_menge')
                                                   % s['scu']),
                             bg=SURFACE, fg=FG, font=fenster.f_small,
                             anchor='w', cursor='hand2')
            zeile.pack(fill='x', pady=1)
            zeile.bind('<Button-1>',
                       lambda _=None, x=s['name']: _schiff_waehlen(x))
            zeile.bind('<Enter>',
                       lambda _=None, w=zeile: w.configure(fg=ACCENT))
            zeile.bind('<Leave>', lambda _=None, w=zeile: w.configure(fg=FG))

    schiffsuche.trace_add('write', _schiffvorschlaege)

    def _werft_bauen():
        """Das Werft-Menü — größte Werft oben, mit Anzahl."""
        for kind in werft_rahmen.winfo_children():
            kind.destroy()
        zaehler = {}
        for s in schiff_modul.with_cargo():
            if s['werft']:
                zaehler[s['werft']] = zaehler.get(s['werft'], 0) + 1
        eintraege = [(w, '%s (%d)' % (w, n)) for w, n in
                     sorted(zaehler.items(), key=lambda p: (-p[1],
                                                            p[0].lower()))]
        if not eintraege:
            return

        def _gewechselt():
            # ⚠⚠ **Das Suchfeld wird mit geleert.** Am 05.09.2026 gemeldet:
            # „Wenn ich einen anderen Hersteller auswähle, muss sich das Feld
            # oben leeren — Spieler wissen es nicht, löschen es nicht und
            # sehen nun keine Auswahl mehr." Genau so: Im Feld stand noch
            # „Drake Ironclad", die neue Werft war Argo — beides zusammen
            # ergibt nichts, und die Liste blieb leer.
            zustand['schiff'] = ''
            zustand['stumm_schiff'] = True
            schiffsuche.set('')
            zustand['stumm_schiff'] = False
            _schiffvorschlaege()

        _filter_bar(fenster, werft_rahmen,
                      [('werft', t('s_rt_alle_werften'), eintraege)],
                      _gewechselt, werft_wahl)

    _werft_bauen()

    def _schiff_zeigen():
        for kind in schiff_info.winfo_children():
            kind.destroy()
        name = zustand['schiff']
        if not name:
            return
        for schluessel, stellen in ((t('s_rt_kaufen'),
                                     schiff_modul.buy_at(name)),
                                    (t('s_rt_mieten'),
                                     schiff_modul.rent_at(name))):
            if not stellen:
                continue
            billig = stellen[0]
            wo = ' · '.join(x for x in (billig.get('stelle'),
                                        billig.get('system')) if x)
            tk.Label(schiff_info,
                     text='%s  %s  ·  %s' % (schluessel,
                                             _auec(billig['preis']), wo),
                     bg=BG, fg=SUB, font=fenster.f_small,
                     anchor='w').pack(fill='x')

    # ⭐⭐ **„Die beste Route überhaupt, egal von wo nach wo."** Gewünscht am
    # 04.09.2026. Das braucht die Fahrten **aller** 184 Handelsposten — rund
    # 92 Sekunden. Deshalb ein Knopf und kein Automatismus: Beim Öffnen der
    # Seite ungefragt anderthalb Minuten ins Netz zu greifen wäre unhöflich
    # gegenüber der fremden Schnittstelle und dem Spieler.
    ueberall = tk.Frame(kopf, bg=BG)
    ueberall.pack(fill='x', pady=(10, 0))
    ueberall_knopf = tk.Label(ueberall, text=t('s_rt_ueberall_suchen'),
                              bg=SURFACE, fg=SUB, font=fenster.f_small,
                              cursor='hand2', padx=10)
    ueberall_knopf.pack(side='left', ipady=5)
    ueberall_stand = tk.Label(ueberall, text='', bg=BG, fg=SUB,
                              font=fenster.f_small, anchor='w')
    ueberall_stand.pack(side='left', padx=(10, 0))

    # ⭐ **Zurücksetzen.** Gewünscht am 05.09.2026: „Routen braucht auch einen
    # Reset-Button." Nach ein paar Versuchen stehen hier Startort, System,
    # Schiff, Hersteller, Frachtraum, Geld und drei Umschalter — von Hand
    # zurückzustellen ist das ein halbes Dutzend Klicks.
    #
    # ⚠ Er steht **rechts** und abgesetzt, nicht neben der Suche: Ein Knopf,
    # der alles wegwirft, gehört nicht dorthin, wo die Hand ohnehin ist.
    reset_knopf = tk.Label(ueberall, text=t('s_zuruecksetzen'), bg=BG, fg=SUB,
                           font=fenster.f_small, cursor='hand2', padx=10)
    reset_knopf.pack(side='right')

    def _stand_zeigen():
        bekannt = routen_modul.known_starts()
        gesamt = len(routen_modul.trade_posts())
        # ⚠ **Auch die Null wird genannt.** Vorher blieb die Zeile leer,
        # solange noch nichts gesammelt war — und damit stand neben dem Knopf
        # gar nichts, wo bei anderen „184 von 184 Handelsposten" steht. Wer
        # zwei Bildschirmfotos vergleicht, hält das für einen Fehler; wer
        # allein davor sitzt, weiß nicht, dass der Knopf etwas sammelt.
        if gesamt:
            ueberall_stand.configure(
                text=t('s_rt_ueberall_stand') % (bekannt, gesamt))
        else:
            ueberall_stand.configure(text='')

    def _ueberall_suchen(_=None):
        if zustand['laeuft']:
            return
        zustand['laeuft'] = True
        zustand['modus'] = 'ueberall'
        ueberall_knopf.configure(text=t('s_rt_ueberall_laeuft'), fg=GOLD)

        def arbeit():
            def melden(fertig, gesamt):
                def zeigen():
                    try:
                        if ueberall_stand.winfo_exists():
                            ueberall_stand.configure(
                                text=t('s_rt_ueberall_stand')
                                % (fertig, gesamt))
                    except tk.TclError:
                        pass
                try:
                    ueberall_stand.after(0, zeigen)
                except tk.TclError:
                    pass
            try:
                routen_modul.fetch_all(progress=melden)
            except Exception as ausnahme:
                fehler.merken('seiten.routes.alle_holen', ausnahme)

            def fertig():
                zustand['laeuft'] = False
                try:
                    if ueberall_knopf.winfo_exists():
                        ueberall_knopf.configure(
                            text=t('s_rt_ueberall_suchen'), fg=SUB)
                    if ergebnis.winfo_exists():
                        _stand_zeigen()
                        _zeichnen()
                except tk.TclError:
                    pass
            try:
                ergebnis.after(0, fertig)
            except tk.TclError:
                zustand['laeuft'] = False

        threading.Thread(target=arbeit, daemon=True).start()

    ueberall_knopf.bind('<Button-1>', _ueberall_suchen)
    ueberall_knopf.bind('<Enter>',
                        lambda _=None: ueberall_knopf.configure(fg=ACCENT))
    ueberall_knopf.bind('<Leave>',
                        lambda _=None: ueberall_knopf.configure(fg=SUB))

    # ⚠⚠ **Die Umschalter stehen oben, nicht im Ergebnis.** Sie lagen zuerst
    # unter der Ortswahl und erschienen deshalb erst, wenn schon ein Ort
    # gewählt war — wer die Seite zum ersten Mal öffnete, sah sie nie und
    # wusste nicht, dass es sie gibt. Am 04.09.2026 gesucht und nicht
    # gefunden: „Da wolltest du doch was bauen, beste Route, schnellste Route
    # oder so."
    #
    # Es sind **Einstellungen**, keine Ergebnisse. Sie gehören dorthin, wo man
    # etwas einstellt.
    schalter_rahmen = tk.Frame(kopf, bg=BG)
    schalter_rahmen.pack(fill='x', pady=(10, 0))

    # Ab hier rollt es — der Kopf darüber steht fest.
    innen = _scroll_area(rahmen, inset=0)
    ergebnis = tk.Frame(innen, bg=BG)
    ergebnis.pack(fill='both', expand=True, padx=24, pady=(12, 20))

    def _as_int(var, ersatz):
        """Eine Eingabe als Zahl — Unsinn wird zum Standard, nicht zum Absturz."""
        try:
            return max(0, int(float((var.get() or '').replace('.', '')
                                    .replace(' ', ''))))
        except (TypeError, ValueError):
            return ersatz

    def _leeren(wo):
        for kind in wo.winfo_children():
            kind.destroy()

    def _schalter_zeichnen():
        """Die drei Umschalter-Reihen — sichtbar ab dem ersten Öffnen."""
        _leeren(schalter_rahmen)

        def reihe_bauen(eintraege, schluessel):
            reihe = tk.Frame(schalter_rahmen, bg=BG)
            reihe.pack(fill='x', pady=(0, 4))
            for wert, beschriftung in eintraege:
                aktiv = zustand[schluessel] == wert
                k = tk.Label(reihe, text='  %s  ' % beschriftung, bg=SURFACE,
                             fg=ACCENT if aktiv else SUB,
                             font=fenster.f_small, cursor='hand2')
                k.pack(side='left', padx=(0, 6), ipady=3)

                def um(_=None, s=schluessel, w=wert):
                    zustand[s] = w
                    _schalter_zeichnen()
                    _zeichnen()
                k.bind('<Button-1>', um)

        reihe_bauen(((False, t('s_rt_nach_gewinn')),
                     (True, t('s_rt_nach_strecke'))), 'kurz')
        # ⚠ Wieviele Stationen — gewünscht am 04.09.2026: „bei Tools im
        # Internet bekommt man Routen von A nach B, von B weiter nach C, von C
        # nach A". Genau das sind diese beiden Reihen.
        reihe_bauen(((2, t('s_rt_stopps') % 2),
                     (3, t('s_rt_stopps') % 3),
                     (4, t('s_rt_stopps') % 4)), 'stopps')
        reihe_bauen(((False, t('s_rt_offen')),
                     (True, t('s_rt_rund'))), 'rund')

    def _zeichnen():
        _leeren(ergebnis)
        if zustand['laeuft']:
            _body_text(ergebnis, t('s_rt_rechnet'), fenster.f_small, fill='x')
            return
        scu, geld = _as_int(scu_var, 96), _as_int(geld_var, 500000)

        # ⚠⚠ **Die globale Bestenliste.** Sie fehlte: Der Knopf sammelte alle
        # 184 Handelsposten ein — und danach stand weiter „Tippe oben ein, wo
        # du gerade bist" da, weil `_zeichnen()` ohne gewählten Ort abbrach.
        # Anderthalb Minuten Abruf für nichts. Am 04.09.2026 gemeldet: „Beste
        # Route lädt 187 Handelsposten, zeigt dann aber nichts an."
        if zustand.get('modus') == 'ueberall':
            tk.Label(ergebnis, text=t('s_rt_ueberall_titel'), bg=BG, fg=FG,
                     font=fenster.f_bold, anchor='w').pack(fill='x',
                                                           pady=(0, 6))
            # ⚠⚠ **Die Schalter gelten auch hier.** Bis v3.15.0-rc6 zeigte
            # dieser Zweig immer nur Einzelfahrten: „Ich möchte eine Rundreise
            # über 3 Stationen, kurze Strecken — die Anzeige bleibt aber so
            # wie am Anfang geladen" (05.09.2026). Die Schalter färbten sich
            # und bewirkten nichts, und das ist schlimmer als kein Schalter.
            #
            # Gerechnet wird nur mit dem, was der Rundumlauf gesammelt hat —
            # nach einem vollen Lauf gemessen unter einer Sekunde.
            stopps = int(zustand.get('stopps') or 1)
            if stopps > 1:
                ketten = routen_modul.best_chains_anywhere(
                    scu, geld, short=bool(zustand.get('kurz')), stops=stopps,
                    round_trip=bool(zustand.get('rund')), most=8)
                if not ketten:
                    _body_text(ergebnis, t('s_rt_keine_kette'),
                                fenster.f_small, fill='x')
                    return
                for gewinn, startname, weg in ketten:
                    _kette_zeichnen(ergebnis, gewinn, weg, startname)
                return
            beste = routen_modul.best_anywhere(scu, geld, most=15)
            if not beste:
                _body_text(ergebnis, t('s_rt_ueberall_leer'),
                            fenster.f_small, fill='x')
                return
            for nummer, e in enumerate(beste):
                if nummer == 0:
                    _route_header(fenster, ergebnis)
                _route_row(fenster, ergebnis, e, hervor=(nummer == 0),
                              mit_start=True)
            return

        if not zustand['start']:
            _body_text(ergebnis, t('s_rt_kein_ort'), fenster.f_small,
                        fill='x')
            return

        kopfzeile = tk.Frame(ergebnis, bg=BG)
        kopfzeile.pack(fill='x', pady=(0, 8))
        tk.Label(kopfzeile, text=zustand['startname'], bg=BG, fg=FG,
                 font=fenster.f_bold, anchor='w').pack(side='left')
        a = routen_modul.age(zustand['start'])
        if a is not None:
            tk.Label(kopfzeile,
                     text=t('s_vk_stand').format(alter=_age_text(a)),
                     bg=BG, fg=SUB, font=fenster.f_small,
                     anchor='e').pack(side='right')

        einzeln = routen_modul.single_trips(zustand['start'], scu, geld,
                                             most=8)
        if not einzeln:
            _body_text(ergebnis, t('s_rt_nichts'), fenster.f_small, fill='x')
            return

        tk.Label(ergebnis, text=t('s_rt_einzeln'), bg=BG, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(fill='x', pady=(4, 4))
        for nummer, e in enumerate(einzeln[:6]):
            if nummer == 0:
                _route_header(fenster, ergebnis)
            # ⚠ Der Startort steht sonst nur in der Überschrift — in der Zeile
            # bliebe ein Pfeil ohne Anfang. `einzelfahrten` kennt ihn nicht,
            # er kommt aus der Auswahl darüber.
            e = dict(e)
            e['startname'] = zustand['startname']
            _route_row(fenster, ergebnis, e, hervor=(nummer == 0))

        ketten = routen_modul.chain(zustand['start'], scu, geld,
                                    short=zustand['kurz'], most=3,
                                    stops=zustand['stopps'],
                                    round_trip=zustand['rund'])
        ueberschrift = (t('s_rt_rundreise_titel') if zustand['rund']
                        else t('s_rt_ketten') % zustand['stopps'])
        tk.Label(ergebnis, text=ueberschrift, bg=BG, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(fill='x',
                                                        pady=(12, 4))
        if not ketten:
            # ⚠ Eine Rundreise findet sich nicht immer — sagen statt schweigen,
            # sonst hält der Nutzer die Seite für kaputt.
            _body_text(ergebnis, t('s_rt_keine_kette'), fenster.f_small,
                        fill='x')
        for nummer, (gesamt, weg) in enumerate(ketten):
            _kette_zeichnen(ergebnis, gesamt, weg, zustand['startname'],
                            hervor=(nummer == 0),
                            rund=bool(zustand['rund']))

    def _kette_zeichnen(eltern, gesamt, weg, startname, hervor=False,
                        rund=False):
        """Eine Fahrtenkette als Kasten — Gewinn, Weg, Schritte.

        ⚠ Eigene Funktion, weil beide Modi sie brauchen: die Ketten ab einem
        gewählten Ort und die besten Ketten überall. Vorher stand der Code nur
        im ersten Zweig, und der zweite konnte deshalb gar keine Ketten
        zeigen.
        """
        kasten = tk.Frame(eltern, bg=SURFACE, highlightthickness=1,
                          highlightbackground=LINE)
        kasten.pack(fill='x', pady=(0, 6))
        oben = tk.Frame(kasten, bg=SURFACE)
        oben.pack(fill='x', padx=12, pady=(6, 2))
        tk.Label(oben, text=t('s_rt_kette_gewinn') % _money(gesamt),
                 bg=SURFACE, fg=ACCENT if hervor else FG,
                 font=fenster.f_small, anchor='w').pack(side='left')
        strecke = sum(f.get('strecke') or 0 for f in weg)
        if strecke:
            tk.Label(oben, text=t('s_rt_strecke') % int(strecke),
                     bg=SURFACE, fg=SUB, font=fenster.f_small,
                     anchor='e').pack(side='right')
        # ⭐⭐ **Was man vorstrecken muss.** Nur die erste Fahrt wird aus
        # eigener Tasche bezahlt; ab der zweiten kauft man vom Erlös der
        # vorigen. Diese eine Zahl entscheidet, ob die Tour für einen selbst
        # überhaupt in Frage kommt.
        anfangs = (weg[0].get('ek') or 0) * (weg[0].get('menge') or 0) \
            if weg else 0
        if anfangs:
            tk.Label(kasten, text='   ' + t('s_rt_kette_einsatz')
                     % _money(anfangs), bg=SURFACE, fg=SUB,
                     font=fenster.f_small, anchor='w').pack(fill='x', padx=12)
        # ⭐⭐ **Der ganze Weg auf einen Blick, vor den Einzelschritten.**
        # Ohne ihn stand da „120 SCU Copper → Rat's Nest", und niemand sah,
        # wo man dafür einkauft. Am 04.09.2026: „Wie fliegt man hier? Ich
        # versteh es nicht, User also auch nicht."
        weg_orte = [startname] + [f['zielname'] for f in weg]
        tk.Label(kasten, text='   ' + '  →  '.join(weg_orte), bg=SURFACE,
                 fg=FG, font=fenster.f_small, anchor='w',
                 wraplength=760, justify='left').pack(
                     fill='x', padx=12, pady=(0, 6))

        for schritt, f in enumerate(weg, start=1):
            # ⚠ **Jeder Schritt nennt beide Orte.** „Kaufe X in A, verkaufe
            # in B" ist eine Anweisung; „X → B" war ein Rätsel. Der
            # Einkaufsort ist das Ziel des vorigen Schritts — beim ersten
            # der gewählte Startort.
            woher = weg_orte[schritt - 1]
            wohin = f['zielname']
            if rund and schritt == len(weg):
                wohin = t('s_rt_zurueck') % wohin
            text = t('s_rt_schritt') % (schritt, woher, f['menge'],
                                        f['ware'], wohin)
            # ⚠ Der Einkauf dieses Schritts gehört dazu — sonst sieht man nur
            # den Gesamtgewinn und weiß nicht, welcher Schritt das Geld bindet.
            ek = (f.get('ek') or 0) * (f.get('menge') or 0)
            if ek:
                text += '  ·  ' + t('s_rt_schritt_ek') % _auec(ek)
            tk.Label(kasten, text=text,
                     bg=SURFACE, fg=SUB, font=fenster.f_small,
                     anchor='w', wraplength=760, justify='left').pack(
                         fill='x', padx=12, pady=(0, 4))

    def _start_waehlen(kennung, name):
        zustand['start'], zustand['startname'] = kennung, name
        # ⚠ Wer einen Ort wählt, will Fahrten **von dort** — nicht weiter die
        # globale Liste. Sonst klickt man einen Ort an und nichts ändert sich.
        zustand['modus'] = 'ab_hier'
        # ⚠⚠ **Der gewählte Ort bleibt im Feld stehen.** Vorher wurde es
        # geleert — dann stand oben „Wo stehst du gerade?" über einem leeren
        # Kasten, und es sah aus, als sei nichts ausgewählt. Am 04.09.2026
        # gemeldet: „Ort verschwindet nach Eingabe oben."
        #
        # `stumm` verhindert dabei, dass das Setzen sofort wieder die
        # Vorschlagsliste aufklappt.
        zustand['stumm'] = True
        ortsuche.set(name)
        zustand['stumm'] = False
        # ⚠ Aufgeklappt bleibt sie nur, bis etwas gewählt ist.
        zustand['ort_offen'] = False
        _ortliste_leeren()
        # ⚠ Die Vorschlagsliste war eben noch acht Zeilen hoch; ohne das hier
        # bleibt die Rollfläche stehen, wo sie war, und oben klafft eine Lücke.
        _scroll_to_top(ergebnis)
        if routen_modul.trips(kennung) is not None:
            _zeichnen()
            return
        zustand['laeuft'] = True
        _zeichnen()

        def arbeit():
            try:
                routen_modul.fetch(kennung)
            except Exception as ausnahme:
                fehler.merken('seiten.routes.holen', ausnahme)

            def fertig():
                zustand['laeuft'] = False
                try:
                    if ergebnis.winfo_exists():
                        _zeichnen()
                except tk.TclError:
                    pass
            try:
                ergebnis.after(0, fertig)
            except tk.TclError:
                zustand['laeuft'] = False

        threading.Thread(target=arbeit, daemon=True).start()

    def _ortvorschlaege(*_a):
        if zustand.get('stumm'):
            return
        _ortliste_leeren()
        text = ortsuche.get().strip().lower()
        # ⚠⚠ **Ohne Eingabe reicht ein Klick ins Feld.** Vorher passierte
        # unter zwei Zeichen nur etwas, wenn rechts ein System gewählt war —
        # am 05.09.2026 gemeldet: „Muss rechts System auswählen, dann klappt
        # die Eingrenzung … das erwartet so kein User." Stimmt: Wer ein
        # Auswahlfeld anklickt, erwartet eine Auswahl, kein Vorbedingung.
        if len(text) < 2 and not zustand.get('system') \
                and not zustand.get('ort_offen'):
            return
        # Steht im Feld genau der schon gewählte Ort, gibt es nichts
        # vorzuschlagen — sonst klappt die Liste beim Zurückkommen wieder auf.
        if text and text == (zustand.get('startname') or '').lower():
            return
        # ⚠ Die Terminal-Liste liegt bereits in der Verkaufs-Ablage — 826
        # Stück mit Namen und System. Kein eigener Abruf nötig.
        stellen = (preisdaten.load() or {}).get('terminals') or {}
        treffer = []
        for kennung, stelle in stellen.items():
            # ⚠⚠ **Nur Terminals, die mit Ware handeln.** Von 826 sind es 184;
            # der Rest sind Läden, Tankstellen und Mietstationen. Wer die
            # mitanbietet, gibt keine Routen, sondern eine Ladenliste — und
            # Seraphim Station stand mit 16 Zeilen da, von denen 15 nichts
            # taugten. Ältere Ablagen kennen die Art noch nicht; dort bleibt
            # alles stehen, statt die Liste leer zu lassen.
            art = stelle.get('t')
            if art is not None and art not in preisdaten.TRADE_TYPES:
                continue
            ort = stelle.get('o') or ''
            # ⚠⚠ **Gesucht wird in BEIDEN Namen.** Eine Station hat viele
            # Terminals; wer „Seraphim" tippt, meint die Station, wer „TDD"
            # tippt, das Terminal. Beides muss finden.
            terminal = stelle.get('n') or ''
            if not (ort or terminal):
                continue
            if zustand.get('system') and \
                    (stelle.get('s') or '') != zustand['system']:
                continue
            if text and text not in ort.lower() \
                    and text not in terminal.lower():
                continue
            treffer.append((kennung, terminal or ort, ort,
                            stelle.get('s') or ''))
            # ⚠ Mehr Zeilen, wenn nur nach System gefiltert wird — Stanton hat
            # 128 Handelsposten, acht davon zu zeigen wäre eine Andeutung.
            if len(treffer) >= (25 if not text else 8):
                break
        # ⚠ Angezeigt wird der **Terminalname**, dahinter Station und System.
        # Vorher stand achtmal „Seraphim Station · Stanton" untereinander und
        # niemand konnte sagen, welche Zeile welche ist.
        if treffer:
            _ortliste_zeigen()
        for kennung, name, ort, system in sorted(treffer,
                                                 key=lambda x: x[1].lower()):
            beiwerk = ' · '.join(x for x in (ort if ort != name else '',
                                             system) if x)
            beschriftung = ('  %s  ·  %s' % (name, beiwerk) if beiwerk
                            else '  ' + name)
            zeile = tk.Label(ortvorschlag, text=beschriftung, bg=SURFACE,
                             fg=FG, font=fenster.f_small, anchor='w',
                             cursor='hand2')
            zeile.pack(fill='x', pady=1)
            zeile.bind('<Button-1>',
                       lambda _=None, k=kennung, n=name: _start_waehlen(k, n))
            zeile.bind('<Enter>',
                       lambda _=None, w=zeile: w.configure(fg=ACCENT))
            zeile.bind('<Leave>', lambda _=None, w=zeile: w.configure(fg=FG))

    # ⭐ Ein Klick ins Ortsfeld zeigt die Handelsposten — ohne dass vorher ein
    # System gewählt sein muss. Dasselbe Verhalten wie bei den Auswahlfeldern
    # im Handelslager.
    def _ort_aufklappen(_=None):
        if not zustand.get('ort_offen'):
            zustand['ort_offen'] = True
            _ortvorschlaege()

    ortfeld.bind('<FocusIn>', _ort_aufklappen, add='+')
    ortfeld.bind('<Button-1>', _ort_aufklappen, add='+')

    # ⚠ Wieder zu, wenn man woanders hinklickt — verzögert, weil der Klick auf
    # eine Zeile erst nach dem `<FocusOut>` ankommt. Siehe `_auswahlfeld`.
    def _ort_zumachen():
        if zustand.get('ort_offen'):
            zustand['ort_offen'] = False
            try:
                if ortvorschlag.winfo_exists():
                    _ortvorschlaege()
            except tk.TclError:
                pass

    ortfeld.bind('<FocusOut>',
                 lambda _=None: ortfeld.after(200, _ort_zumachen), add='+')
    ortfeld.bind('<Escape>', lambda _=None: _ort_zumachen(), add='+')

    # ⚠ Und jeder Klick woandershin — `<FocusOut>` allein greift nicht, wenn
    # das Ziel gar nicht fokussierbar ist. Siehe `_auswahlfeld`.
    def _ort_klick_im_fenster(ereignis):
        if not zustand.get('ort_offen'):
            return
        w = getattr(ereignis, 'widget', None)
        while w is not None:
            if w in (ortfeld, ortvorschlag, ortzeile):
                return
            w = getattr(w, 'master', None)
        _ort_zumachen()

    try:
        rahmen.winfo_toplevel().bind('<Button-1>', _ort_klick_im_fenster,
                                     add='+')
    except tk.TclError:
        pass

    def _alles_zuruecksetzen(_=None):
        """Die Seite auf den Anfangszustand — alle Eingaben und Schalter.

        ⚠ **Der gesammelte Rundumlauf bleibt.** Zurückgesetzt wird, was man
        eingestellt hat, nicht was man geholt hat: Die 184 Handelsposten
        wegzuwerfen hieße anderthalb Minuten Abruf für nichts.
        """
        zustand['start'] = zustand['startname'] = ''
        zustand['system'] = ''
        zustand['schiff'] = ''
        zustand['modus'] = 'ab_hier'
        zustand['kurz'] = False
        zustand['stopps'] = 2
        zustand['rund'] = False
        zustand['ort_offen'] = False
        werft_wahl['werft'] = ''
        zustand['stumm'] = zustand['stumm_schiff'] = True
        ortsuche.set('')
        schiffsuche.set('')
        zustand['stumm'] = zustand['stumm_schiff'] = False
        scu_var.set('120')
        geld_var.set('500000')
        if system_menue[0] is not None:
            try:
                system_menue[0].select_quiet('')
            except Exception:
                pass
        _ortliste_leeren()
        _schiffliste_leeren()
        _werft_bauen()
        _schiff_zeigen()
        _schalter_zeichnen()
        _zeichnen()
        _scroll_to_top(innen)

    reset_knopf.bind('<Button-1>', _alles_zuruecksetzen)
    reset_knopf.bind('<Enter>',
                     lambda _=None: reset_knopf.configure(fg=RED))
    reset_knopf.bind('<Leave>',
                     lambda _=None: reset_knopf.configure(fg=SUB))

    ortsuche.trace_add('write', _ortvorschlaege)
    for var in (scu_var, geld_var):
        var.trace_add('write', lambda *_a: _zeichnen())
    _schalter_zeichnen()
    _stand_zeigen()
    _zeichnen()

    def _beim_zeigen():
        # ⚠ Der gewählte Ort bleibt — wer zurückkommt, will weiterarbeiten
        # und nicht neu tippen. Nur die offene Vorschlagsliste wird geräumt.
        _ortliste_leeren()
        _schiffe_sicherstellen()
    fenster.on_show['routen'] = _beim_zeigen

    def _schiffe_sicherstellen():
        """Die Schiffsdaten holen, falls sie fehlen — und das Werft-Menü füllen.

        ⚠ Beim Aufbau der Seite liegen sie oft noch nicht vor; dann bliebe das
        Menü leer und der Reiter fragte nach einem Namen, den niemand kennt.
        """
        if schiff_modul.with_cargo():
            return

        def arbeit():
            try:
                schiff_modul.update()
            except Exception as ausnahme:
                fehler.merken('seiten.routes.schiffe', ausnahme)

            def fertig():
                try:
                    if werft_rahmen.winfo_exists():
                        _werft_bauen()
                except tk.TclError:
                    pass
            try:
                werft_rahmen.after(0, fertig)
            except tk.TclError:
                pass

        threading.Thread(target=arbeit, daemon=True).start()

    _schiffe_sicherstellen()


def _route_header(fenster, eltern):
    """Die Spaltenüberschrift über der ersten Fahrt.

    ⚠⚠ **Ohne sie ist die größte Zahl mehrdeutig.** Am 04.09.2026 stand da
    „1.917.234 aUEC · 69 SCU Atlasium" — und die Frage kam: „EK sind fast 2
    Mio, aber wieviel Gewinn macht man?" Es *war* der Gewinn (der Einsatz lag
    bei 4.982.766). Eine Zahl ohne Spaltennamen lädt zum Falschlesen ein.

    ⚠ Nur **über der ersten** Fahrt — in jeder Zeile wiederholt wäre sie
    Lärm. Dieselbe Regel wie im Verkaufs-Reiter.
    """
    kopf = tk.Frame(eltern, bg=BG)
    kopf.pack(fill='x', pady=(0, 3))
    tk.Label(kopf, text=t('s_rt_sp_gewinn'), bg=BG, fg=SUB,
             font=fenster.f_small, width=16, anchor='w').pack(side='left')
    tk.Label(kopf, text=t('s_rt_sp_menge'), bg=BG, fg=SUB,
             font=fenster.f_small, width=8, anchor='w').pack(side='left')
    tk.Label(kopf, text=t('s_rt_sp_weg'), bg=BG, fg=SUB,
             font=fenster.f_small, anchor='w').pack(side='left')


def _route_row(fenster, eltern, fahrt, hervor=False, mit_start=False):
    """Eine Einzelfahrt: Gewinn, Menge, Ware, Ziel, Strecke.

    `mit_start=True` nennt zusätzlich den **Einkaufsort** — nötig in der
    globalen Bestenliste, wo jede Zeile woanders beginnt. Ohne ihn stünde da
    ein Gewinn ohne Angabe, wo man ihn holt.
    """
    kasten = tk.Frame(eltern, bg=SURFACE, highlightthickness=1,
                      highlightbackground=LINE)
    kasten.pack(fill='x', pady=(0, 4))
    zeile = tk.Frame(kasten, bg=SURFACE)
    zeile.pack(fill='x', padx=12, pady=6)
    tk.Label(zeile, text=_auec(fahrt['gewinn']), bg=SURFACE,
             fg=ACCENT if hervor else FG, font=fenster.f_small,
             width=16, anchor='w').pack(side='left')
    tk.Label(zeile, text=t('s_rt_scu_menge') % fahrt['menge'], bg=SURFACE,
             fg=SUB, font=fenster.f_small, width=8, anchor='w').pack(
                 side='left')
    tk.Label(zeile, text=fahrt['ware'], bg=SURFACE, fg=FG,
             font=fenster.f_small, anchor='w').pack(side='left')
    # ⚠⚠ **Beide Orte, beide beschriftet.** „→ Terra Gateway" allein sagt
    # nicht, wo man einkauft — das stand nur in der Überschrift darüber. Wer
    # die Zeile für sich liest (und das tut man in einer Tabelle), sah einen
    # Pfeil ins Nichts.
    if fahrt.get('startname'):
        tk.Label(zeile, text='  ' + t('s_rt_ab') % fahrt['startname'],
                 bg=SURFACE, fg=FG, font=fenster.f_small,
                 anchor='w').pack(side='left')
    tk.Label(zeile, text='  →  ' + t('s_rt_nach')
             % (fahrt.get('zielname') or '?'), bg=SURFACE,
             fg=SUB, font=fenster.f_small, anchor='w').pack(side='left')

    # ⭐⭐ **Der Einsatz gehört dazu — ohne ihn ist der Gewinn eine Behauptung.**
    # Am 04.09.2026 stand da „586.500 aUEC · 1 SCU Osoian Hides", und die
    # Frage kam sofort: „Bringt da 1 SCU wirklich über 500k?" Die Zahl stimmte
    # (Einkauf 283.500, Verkauf 870.000 je SCU) — aber dass man dafür erst
    # **283.500 aUEC hinlegen** muss, stand nirgends.
    #
    # Genau das trennt eine Auskunft von einer Zahl: Ein Gewinn ohne Einsatz
    # sagt nicht, ob man ihn sich leisten kann.
    einsatz = (fahrt.get('ek') or 0) * fahrt['menge']
    if einsatz:
        zweite = tk.Frame(kasten, bg=SURFACE)
        zweite.pack(fill='x', padx=12, pady=(0, 6))
        tk.Label(zweite, text=t('s_rt_einsatz') % _money(einsatz), bg=SURFACE,
                 fg=SUB, font=fenster.f_small, anchor='w').pack(side='left')
        # Wieviel dort überhaupt liegt — die häufigste Enttäuschung: Man fliegt
        # hin und es sind zwei Kisten.
        if fahrt.get('vorrat'):
            tk.Label(zweite,
                     text='   ' + t('s_rt_vorrat') % fahrt['vorrat'],
                     bg=SURFACE, fg=SUB, font=fenster.f_small,
                     anchor='w').pack(side='left')
        # ⭐ Woran die Menge hängt. „69 von 120 SCU" sagt noch nicht, ob ein
        # größeres Schiff hilft — „begrenzt durch dein Geld" schon.
        if fahrt.get('grenze'):
            tk.Label(zweite, text='   ' + t(fahrt['grenze']), bg=SURFACE,
                     fg=GOLD, font=fenster.f_small,
                     anchor='w').pack(side='left')
    if fahrt.get('strecke'):
        tk.Label(zeile, text=t('s_rt_strecke') % int(fahrt['strecke']),
                 bg=SURFACE, fg=SUB, font=fenster.f_small,
                 anchor='e').pack(side='right')


# ⚠⚠ Die Unterarten, die eine **Schiffswaffe** ausmachen, und die einer
# **Handfeuerwaffe**. Beides steckt in den Rezeptdaten unter derselben Art
# `weapons` — 270 Stück, die niemand zusammen durchsucht.
#
# Gemessen am 04.09.2026: Schiffswaffen 96, FPS-Waffen 168, dazu 6 ohne
# Unterart. Die sechs bleiben schlicht „Waffen" — geraten wird nicht.
SHIP_WEAPONS = frozenset(('laser', 'ballistic', 'distortion', 'neutron',
                           'tachyon'))
FPS_WEAPONS = frozenset(('pistol', 'rifle', 'sniper', 'smg', 'shotgun', 'lmg'))

# ⚠ Notdeckel für eine einzelne Warengruppe. Sie wird sonst **vollständig**
# gezeigt — die größte echte Gruppe hat 201 Einträge. Die Zahl schützt nur
# davor, dass ein Ausreißer in fremden Daten Tausende Zeilen baut.
EMERGENCY_CAP = 400

# ⚠⚠ **Wie viele Zeilen je Warengruppe, wenn mehrere nebeneinanderstehen.**
# Am 04.09.2026 gemeldet: „Was gehört alles zu Systemen? Blicke da nicht
# durch." Bei 176 Treffern in vier Gruppen füllte allein die erste Gruppe die
# ganze Liste — die anderen drei sah man nie. Ein Deckel **je Gruppe** zeigt
# stattdessen von jeder etwas, und darunter steht, wie viele noch folgen.
PER_GROUP = 12

# Kennzeichen für Einträge, die kein Ladenteil sind. Sie müssen sich von jeder
# echten Kennung unterscheiden — deshalb ein Doppelpunkt, den UUIDs nicht haben.
SHIP_PREFIX = 'schiff:'
SHIPYARD_PREFIX = 'werft:'
AREA_SHIPS = '__schiffe'


def _shops(fenster, rahmen):
    """Der Reiter „Läden": Wo bekomme ich ein fertiges Teil, und was kostet es?

    ⚠ **Die Gegenrichtung zum Verkaufs-Reiter.** Dort geht es um Ware, die man
    loswerden will; hier um ein Teil, das man haben will. Und die Ergänzung zur
    Herstellung: Dort steht „lohnt Bauen?", hier „wo krieg ich's fertig?".

    ⚠⚠ **Gesucht wird über den Bauplan-Namen, zugeordnet über die
    Entitäts-Kennung.** Der Name ist das, was der Spieler kennt; die Kennung
    ist das, worüber es keine Verwechslung gibt. Ein Namensvergleich gegen UEX
    hat hier schon einmal `Golden Medmon` als Goldpreis geliefert.
    """
    from . import crafting as herst_modul, shops as laden_modul

    _heading(fenster, rahmen, t('hf_laeden'), t('s_ld_lead'))

    suche = tk.StringVar()
    gewaehlt = {'name': '', 'kennung': ''}
    laeuft = {'ja': False}
    # ⚠⚠ **Die Reihenfolge ist die Kaskade.** Jedes Menü zeigt nur, was zur
    # Auswahl in den Menüs **davor** passt — und beim Wechsel fällt weg, was
    # danach nicht mehr passt. Am 04.09.2026 verlangt: „Wenn mehr Filter nötig
    # sind, damit man findet was man sucht, dann musst du die bauen." Vier
    # Ebenen decken die Frage „Radar — Schiffskomponenten oder Schiffswaffen,
    # welche Größe, welcher Hersteller?" vollständig ab.
    # ⚠⚠ **Drei Menüs, und der Hersteller ist keins davon.** Am 05.09.2026:
    # „Nach Hersteller sucht eigentlich niemand, wenn er Teile sucht, die in
    # sein Schiff passen" — und: „Hersteller muss grau hinter dem Namen
    # stehen, das reicht." Stimmt beides: Was hineinpasst, entscheidet die
    # Größe. Der Hersteller ist eine Angabe zum Lesen, kein Suchweg — er steht
    # an der Zeile und wird von der Suche mit gefunden, hat aber kein Menü.
    FILTER_FOLGE = ('bereich', 'gruppe', 'groesse', 'klasse', 'guete')
    # ⚠ Muss vor `_teile()` stehen — die Funktion fragt ihn ab, um während des
    # Katalog-Abrufs nicht die falsche Liste zu zeigen.
    zustand_katalog = {'laeuft': False}
    wahl = dict((f, '') for f in FILTER_FOLGE)

    # ⚠⚠ **Suchfeld und Auswahl bleiben stehen, nur die Liste rollt.**
    # Am 04.09.2026 gemeldet: „Auswahlfelder verschwinden oben beim
    # Runterscrollen." Wer bei Zeile 30 merkt, dass er die Auswahl ändern
    # will, muss sonst erst wieder hochrollen — und weiß beim Rollen nicht
    # mehr, wonach er überhaupt gefiltert hat.
    #
    # Genau die Regel aus dem Projekt-Handbuch: Erst alles Feste packen,
    # **danach** die rollende Fläche mit `expand=True`.
    kopf = tk.Frame(rahmen, bg=BG)
    kopf.pack(fill='x', side='top')

    such_rahmen = tk.Frame(kopf, bg=BG)
    such_rahmen.pack(fill='x', padx=24, pady=(4, 0))
    feld = tk.Entry(such_rahmen, textvariable=suche, font=fenster.f_base,
                    bg=SURFACE, fg=FG, insertbackground=FG, relief='flat',
                    highlightthickness=1, highlightbackground=LINE,
                    highlightcolor=ACCENT)
    feld.pack(fill='x', ipady=5)
    fields.hint(feld, suche, t('s_ld_suche_platz'), normal=FG, grey=SUB)

    # ⭐⭐ **Dieselbe Filterleiste wie in der Bauplan-Liste.** Vorher stand hier
    # nur ein leeres Suchfeld — wer nicht wusste, wonach er suchen soll, sah
    # eine leere Seite. Xharig am 04.09.2026: „bei Läden gähnende Leere, kann
    # man da nicht ein Menü mit Dropdowns bauen … gleiches Bild und Bedienung
    # wie im restlichen Tool?"
    #
    # Genau dafür gibt es `_filterleiste` — sie trägt in ihrem eigenen Kopf
    # den Satz „das Bedienkonzept sollte nicht jedes Mal ändern".
    filter_rahmen = tk.Frame(kopf, bg=BG)
    filter_rahmen.pack(fill='x', padx=24, pady=(8, 0))

    # Die Standzeile gehört zum festen Kopf — sie sagt, worauf sich die Liste
    # darunter bezieht. Rechts daneben der Reset: Bei fünf Auswahlmenüs plus
    # Suchfeld ist „alles wieder offen" sonst ein halbes Dutzend Klicks.
    # Gewünscht am 05.09.2026: „Läden braucht auch einen Reset-Button."
    stand_rahmen = tk.Frame(kopf, bg=BG)
    stand_rahmen.pack(fill='x', padx=24, pady=(6, 0))
    stand_zeile = tk.Label(stand_rahmen, text='', bg=BG, fg=GOLD,
                           font=fenster.f_small, anchor='w')
    # ⭐ Umbrechen statt abschneiden: Die Standmeldung ist ein ganzer
    # Satz und stand bei „sehr groß" 81 px über den Rand hinaus.
    _wrap_self(stand_zeile)
    ld_reset = tk.Label(stand_rahmen, text=t('s_zuruecksetzen'), bg=BG,
                        fg=SUB, font=fenster.f_small, cursor='hand2',
                        padx=10)
    ld_reset.pack(side='right')

    # Ab hier rollt es.
    innen = _scroll_area(rahmen, inset=0)

    vorschlag_rahmen = tk.Frame(innen, bg=SURFACE, highlightthickness=1,
                                highlightbackground=LINE)
    vorschlag_rahmen.pack(fill='x', padx=24)
    ergebnis_rahmen = tk.Frame(innen, bg=BG)
    ergebnis_rahmen.pack(fill='both', expand=True, padx=24, pady=(10, 20))

    def _teile():
        """Was es **wirklich zu kaufen gibt** — einheitlich aufbereitet.

        Je Eintrag `name`, `kennung`, `bereich` und `gruppe`. Bereich und
        Gruppe sind die **englischen** Werte aus der Quelle; übersetzt wird
        erst beim Anzeigen (`_gruppenname`). So bleibt eine getroffene Auswahl
        gültig, auch wenn jemand die Sprache umstellt.

        ⚠⚠ **Die Quelle ist der UEX-Katalog, nicht die Bauplanliste.** Bis
        v3.14.0 kam die Liste aus `crafting.all_items()` — sie zeigte also nur,
        was man auch **bauen** kann. Am 04.09.2026 gefragt: „Wie soll man da
        wissen, wo es Boomtube-Raketen gibt?" Gar nicht: Der Boomtube Rocket
        Launcher ist nicht craftbar und stand deshalb nirgends, obwohl UEX
        seine Läden kennt. Gemessen: **1.528 kaufbare Teile** statt 893
        craftbaren, darunter Raketen, Bomben, Torpedorohre und
        Railgun-Munition.

        ⚠⚠ **Der Rückfall auf die Baupläne gilt nur ohne laufenden Abruf.**
        Er zeigt eine andere Gliederung (Bauplan-Arten statt UEX-Bereiche) und
        kennt keine Schiffe — wer ihn während der ersten Minute sieht, hält
        ihn für das Ergebnis. Am 05.09.2026 genau so gemeldet: „Finde Schiffe
        nicht mehr in der Auswahl bei Läden", während im Hintergrund noch der
        Katalog geholt wurde.

        Läuft der Abruf, bleibt die Liste deshalb leer und der Hinweis darüber
        stehen. Nur wenn gar nichts geht (kein Netz), sind die Baupläne besser
        als eine leere Seite.
        """
        if zustand_katalog['laeuft']:
            return []
        try:
            katalog = laden_modul.catalog_items()
        except Exception as ausnahme:
            fehler.merken('seiten.shops.catalog_items', ausnahme)
            katalog = []
        if katalog:
            raus = [{'name': x['name'], 'kennung': x['kennung'],
                     'bereich': x['abschnitt'], 'gruppe': x['kategorie'],
                     'hersteller': x.get('hersteller') or '',
                     'groesse': x.get('groesse') or '',
                     'klasse': x.get('klasse') or '',
                     'guete': x.get('guete') or ''}
                    for x in katalog if x['name'] and x['kennung']]
            # ⭐ **Schiffe gehören dazu.** Die Kauf- und Mietpreise lagen seit
            # v3.14.0 vor, wurden aber nur für den Frachtraum im Routenplaner
            # benutzt — angezeigt hat sie nie jemand. Ein Reiter „wo bekomme
            # ich das" ist der Ort dafür. Warengruppe ist die Werft: Wer ein
            # Schiff sucht, sucht meistens „die Drakes".
            try:
                from . import ships as schiff_modul
                for s in schiff_modul.catalog():
                    raus.append({'name': s['name'],
                                 'kennung': SHIP_PREFIX + s['name'],
                                 'bereich': AREA_SHIPS,
                                 'gruppe': SHIPYARD_PREFIX + (s['werft'] or '?'),
                                 'hersteller': '', 'groesse': '',
                                 'klasse': '', 'guete': ''})
            except Exception as ausnahme:
                fehler.merken('seiten.shops.schiffe', ausnahme)
            return raus
        try:
            alle = [b for b in herst_modul.all_items() if b.get('entity')]
        except Exception as ausnahme:
            fehler.merken('seiten.shops.teile', ausnahme)
            return []
        return [{'name': b.get('name') or '', 'kennung': b.get('entity') or '',
                 'bereich': '', 'gruppe': _art_von(b)} for b in alle]

    def _art_von(b):
        """Die Art eines Bauplans — Waffen aufgeteilt nach Schiff und Mann.

        ⚠⚠ **„Waffen" ist keine brauchbare Gruppe.** 270 Stück, und darin
        steckt Grundverschiedenes: Schiffsgeschütze und Handfeuerwaffen. Wer
        einen Kühler fürs Schiff sucht, sucht nicht dieselbe Liste wie jemand,
        der ein Gewehr braucht. Am 04.09.2026: „Aber Waffen und Schiffswaffen
        gehört trotzdem getrennt."

        Die Trennung steckt schon in den Daten — in der **Unterart**:

        | | Unterarten | Anzahl |
        |---|---|---|
        | Schiffswaffen | laser, ballistic, distortion, neutron, tachyon | 96 |
        | FPS-Waffen | pistol, rifle, sniper, smg, shotgun, lmg | 168 |

        ⚠ Was in keine der beiden Listen fällt (6 ohne Unterart), bleibt
        schlicht „Waffen" — geraten wird nicht.
        """
        art = (b.get('art') or '').strip()
        if art != 'weapons':
            return art
        unterart = (b.get('unterart') or '').strip().lower()
        if unterart in SHIP_WEAPONS:
            return 'weapons_schiff'
        if unterart in FPS_WEAPONS:
            return 'weapons_fps'
        return art

    def _artname(wert):
        """Der lesbare Name einer Art — auch für die beiden neuen."""
        if wert == 'weapons_schiff':
            return t('s_ld_art_schiffswaffen')
        if wert == 'weapons_fps':
            return t('s_ld_art_fpswaffen')
        from . import crafting as hm
        return hm.kind_name(wert)

    def _gruppenname(wert):
        """Warengruppe lesbar — aus dem Katalog oder aus den Bauplan-Arten.

        ⚠ Kennt die Tabelle den Namen nicht (UEX legt eine Gruppe nach),
        bleibt der englische stehen. Ein geratenes deutsches Wort wäre
        schlechter als ein ehrliches englisches.
        """
        if wert.startswith(SHIPYARD_PREFIX):
            # Eine Werft heißt überall gleich — Drake bleibt Drake.
            return wert[len(SHIPYARD_PREFIX):]
        schluessel = laden_modul.GROUP_KEYS.get(wert)
        if schluessel:
            return t(schluessel)
        return _artname(wert) if wert else wert

    def _bereichsname(wert):
        """Bereich lesbar — sonst wie `_gruppenname`."""
        if wert == AREA_SHIPS:
            return t('s_ld_ber_schiffe')
        schluessel = laden_modul.SECTION_KEYS.get(wert)
        return t(schluessel) if schluessel else wert

    def _wertname(feld, wert):
        """Ein Filterwert, wie er im Menü steht."""
        if feld == 'bereich':
            return _bereichsname(wert)
        if feld == 'gruppe':
            return _gruppenname(wert)
        if feld == 'groesse':
            return t('s_ld_groesse') % wert
        if feld == 'guete':
            return t('s_ld_guete') % wert
        if feld == 'klasse':
            schluessel = CLASS_LABELS.get(wert)
            return t(schluessel) if schluessel else wert
        return wert

    def _mit_zahl(feld):
        """Werte eines Feldes mit ihrer Anzahl — „Kühler (74)".

        ⚠⚠ **Sortiert nach Anzahl, nicht alphabetisch.** Am 04.09.2026
        gemeldet: „Da fehlen noch Schiffswaffen und FPS-Waffen." Sie fehlten
        nicht — sie standen alphabetisch an Position 11 und 14, und die
        aufgeklappte Liste zeigt rund zehn Zeilen. Die Rollleiste ist da und
        bekommt ihre 10 px (nachgemessen, in beiden Pack-Reihenfolgen), aber
        ein dunkler Streifen auf dunklem Grund fällt nicht auf: „sieht aber
        keine Sau, weil kein Balken da ist."

        Ein Auswahlmenü, dessen zwei größte Gruppen man erst erscrollen muss,
        ist **falsch sortiert** — nicht zu kurz.
        """
        # ⚠⚠ **Jedes Menü richtet sich nach den Menüs davor.** Sonst lassen
        # sich Dinge zusammenstellen, die es nicht gibt — am 04.09.2026
        # gemeldet: „Rüstung (710)" und „Geschütze (87)" nebeneinander,
        # Ergebnis null. „Geschütze gehören doch nicht zu Rüstungen." Eine
        # unmögliche Kombination gehört gar nicht erst angeboten.
        #
        # ⚠ Das **erste** Menü bleibt immer vollständig — sonst käme man aus
        # einer engen Auswahl nicht mehr heraus.
        vorher = FILTER_FOLGE[:FILTER_FOLGE.index(feld)]
        zaehler = {}
        for b in _teile():
            if any(wahl[f] and b.get(f) != wahl[f] for f in vorher):
                continue
            wert = (b.get(feld) or '').strip()
            if wert:
                zaehler[wert] = zaehler.get(wert, 0) + 1
        raus = []
        for wert, anzahl in zaehler.items():
            lesbar = _wertname(feld, wert)
            raus.append((anzahl, wert, '%s (%d)' % (lesbar or wert, anzahl),
                         (lesbar or wert)))
        if feld == 'groesse':
            # ⚠ Größen gehören in ihre eigene Ordnung, nicht nach Häufigkeit:
            # 1, 2, 3 … 10. Nach Anzahl sortiert stünde Größe 4 vor Größe 1,
            # und niemand fände die Zeile, die er sucht.
            def _sort_key(paar):
                try:
                    return (0, float(paar[1]))
                except ValueError:
                    return (1, 0.0)
            raus.sort(key=_sort_key)
        else:
            # Größte zuerst; bei Gleichstand alphabetisch, damit die
            # Reihenfolge nicht von Lauf zu Lauf springt.
            raus.sort(key=lambda p: (-p[0], p[3].lower()))
        return [(w, b) for _n, w, b, _s in raus]

    def _leeren(wo):
        for kind in wo.winfo_children():
            kind.destroy()

    def _liste_leeren():
        """Die Vorschlagsliste wegräumen — **ausgepackt**, nicht nur geleert.

        ⚠⚠ **Ein geleerter Rahmen behält seine Höhe.** Gemessen am 04.09.2026:
        Nach dem Klick auf ein Teil hatte dieser Rahmen **null Kinder und
        weiterhin 920 px**. Die Läden darunter landeten damit bei y=1135 in
        einem 1000 px hohen Fenster — gezeichnet, aber außerhalb der Sicht.
        Der Reiter wirkte leer, obwohl elf Läden bereitstanden: „klickt man
        diese an, sieht man keinen Verkaufsort."

        `pack_forget()` ist der einzige Weg, der die Höhe zuverlässig abgibt.
        """
        _leeren(vorschlag_rahmen)
        vorschlag_rahmen.pack_forget()

    def _liste_zeigen():
        """Die Vorschlagsliste wieder einhängen — immer über dem Ergebnis.

        ⚠ **Sie bekommt einen Rand.** Ohne den klebt sie als flache Fläche am
        Text darüber und sieht nach Beschriftung aus, nicht nach Auswahl —
        am 04.09.2026 genau so missverstanden. Ein Kasten sagt „hier steht
        etwas zum Anklicken", bevor jemand den Mauszeiger darüber hält.
        """
        vorschlag_rahmen.pack(fill='x', padx=24, pady=(6, 0),
                              before=ergebnis_rahmen)

    def _schiff_zeichnen(schiffsname):
        """Kauf- und Mietstellen eines Schiffs — zwei Blöcke untereinander.

        ⚠ **Mieten steht dabei, nicht nur Kaufen.** Für die meisten Schiffe
        ist die Miete die Zahl, die zählt: Wer einmal Fracht fahren will,
        mietet für einen Tag, statt Millionen auszugeben.
        """
        from . import ships as schiff_modul
        kopf = tk.Frame(ergebnis_rahmen, bg=BG)
        kopf.pack(fill='x', pady=(0, 6))
        tk.Label(kopf, text=schiffsname, bg=BG, fg=FG, font=fenster.f_bold,
                 anchor='w').pack(side='left')
        laderaum = schiff_modul.scu(schiffsname)
        if laderaum:
            tk.Label(kopf, text=t('s_ld_scu') % laderaum, bg=BG, fg=SUB,
                     font=fenster.f_small, anchor='e').pack(side='right')

        etwas = False
        for schluessel, holen in (('s_ld_kaufen', schiff_modul.buy_at),
                                  ('s_ld_mieten', schiff_modul.rent_at)):
            stellen = holen(schiffsname)
            if not stellen:
                continue
            etwas = True
            tk.Label(ergebnis_rahmen, text=t(schluessel), bg=BG, fg=SUB,
                     font=fenster.f_small, anchor='w').pack(fill='x',
                                                            pady=(6, 2))
            for nummer, z in enumerate(stellen):
                kasten = tk.Frame(ergebnis_rahmen, bg=SURFACE,
                                  highlightthickness=1,
                                  highlightbackground=LINE)
                kasten.pack(fill='x', pady=(0, 4))
                zeile = tk.Frame(kasten, bg=SURFACE)
                zeile.pack(fill='x', padx=12, pady=6)
                tk.Label(zeile, text=_auec(z['preis']), bg=SURFACE,
                         fg=ACCENT if nummer == 0 else FG,
                         font=fenster.f_small, width=16,
                         anchor='w').pack(side='left')
                tk.Label(zeile, text=z.get('stelle') or '?', bg=SURFACE,
                         fg=FG, font=fenster.f_small,
                         anchor='w').pack(side='left')
                beiwerk = ' · '.join(x for x in (z.get('ort'), z.get('system'))
                                     if x)
                if beiwerk:
                    tk.Label(zeile, text='  ' + beiwerk, bg=SURFACE, fg=SUB,
                             font=fenster.f_small,
                             anchor='w').pack(side='left')
        if not etwas:
            _body_text(ergebnis_rahmen, t('s_ld_schiff_nichts'),
                        fenster.f_small, fill='x')

    def _ergebnis_zeichnen():
        _leeren(ergebnis_rahmen)
        if not gewaehlt['kennung']:
            return
        if gewaehlt['kennung'].startswith(SHIP_PREFIX):
            _schiff_zeichnen(gewaehlt['kennung'][len(SHIP_PREFIX):])
            return
        liste = laden_modul.shops_for(gewaehlt['kennung'])
        if liste is None:
            _body_text(ergebnis_rahmen, t('s_ld_sucht'), fenster.f_small,
                        fill='x')
            return
        if not liste:
            # ⚠ **Nicht „gibt es nirgends".** UEX hat Lücken (gemessen: 435 von
            # 1.604 Bauplänen). Eine Lücke in fremden Daten ist keine Aussage
            # über das Spiel — das wäre eine Behauptung, die wir nicht belegen
            # können.
            _body_text(ergebnis_rahmen, t('s_ld_unbekannt'), fenster.f_small,
                        fill='x')
            return

        kopf = tk.Frame(ergebnis_rahmen, bg=BG)
        kopf.pack(fill='x', pady=(0, 6))
        tk.Label(kopf, text=gewaehlt['name'], bg=BG, fg=FG,
                 font=fenster.f_bold, anchor='w').pack(side='left')
        a = laden_modul.age(gewaehlt['kennung'])
        if a is not None:
            tk.Label(kopf, text=t('s_vk_stand').format(alter=_age_text(a)),
                     bg=BG, fg=SUB, font=fenster.f_small,
                     anchor='e').pack(side='right')

        for nummer, z in enumerate(liste):
            kasten = tk.Frame(ergebnis_rahmen, bg=SURFACE,
                              highlightthickness=1, highlightbackground=LINE)
            kasten.pack(fill='x', pady=(0, 4))
            zeile = tk.Frame(kasten, bg=SURFACE)
            zeile.pack(fill='x', padx=12, pady=6)
            # ⭐ Der billigste steht oben und wird als einziger hervorgehoben.
            # Zwei grüne Zeilen wären keine Empfehlung mehr.
            tk.Label(zeile, text=_auec(z['preis']), bg=SURFACE,
                     fg=ACCENT if nummer == 0 else FG, font=fenster.f_small,
                     width=16, anchor='w').pack(side='left')
            tk.Label(zeile, text=z.get('laden') or '?', bg=SURFACE, fg=FG,
                     font=fenster.f_small, anchor='w').pack(side='left')
            beiwerk = ' · '.join(x for x in (z.get('ort'), z.get('system'))
                                 if x)
            if beiwerk:
                tk.Label(zeile, text='  ' + beiwerk, bg=SURFACE, fg=SUB,
                         font=fenster.f_small, anchor='w').pack(side='left')
            # ⚠ Der Zustand gehört dazu: Gebrauchte Ware ist billiger **und**
            # weniger wert. Ein Preis ohne diese Zahl wäre die halbe Wahrheit.
            if z.get('zustand') and z['zustand'] < 100:
                tk.Label(zeile, text=t('s_ld_zustand') % z['zustand'],
                         bg=SURFACE, fg=GOLD, font=fenster.f_small,
                         anchor='e').pack(side='right')

    def _waehlen(name, kennung):
        gewaehlt['name'], gewaehlt['kennung'] = name, kennung
        suche.set('')
        _liste_leeren()
        _ergebnis_zeichnen()
        # ⚠ Wer aus einer langen Liste auswählt, steht weit unten. Die Antwort
        # erscheint oben — ohne diesen Sprung sieht er weiter die Stelle, an
        # der eben noch seine Auswahl stand.
        _scroll_to_top(innen)
        # ⚠ Schiffe liegen schon vollständig vor — ihre Preise kommen aus den
        # drei Wochenlisten, nicht aus einem Abruf je Gegenstand.
        if kennung.startswith(SHIP_PREFIX):
            return
        if laden_modul.known(kennung) or laeuft['ja']:
            return
        laeuft['ja'] = True

        def arbeit():
            try:
                # ⚠ Der Name ist der Rückfall, falls die Kennung leer ausgeht
                # — siehe Kopf von `shops.py`. Er hat dort 375 Teile mehr
                # zugeordnet.
                laden_modul.fetch(kennung, name)
            except Exception as ausnahme:
                fehler.merken('seiten.shops.fetch', ausnahme)

            def fertig():
                laeuft['ja'] = False
                try:
                    if ergebnis_rahmen.winfo_exists():
                        _ergebnis_zeichnen()
                except tk.TclError:
                    pass
            try:
                ergebnis_rahmen.after(0, fertig)
            except tk.TclError:
                laeuft['ja'] = False

        threading.Thread(target=arbeit, daemon=True).start()

    def _gruppe_aufklappen(gruppe):
        """Auf „… 38 weitere" geklickt: genau diese Warengruppe filtern."""
        wahl['gruppe'] = gruppe
        _filter_gewechselt()

    def _vorschlaege(*_a):
        _liste_leeren()
        text = suche.get().strip().lower()
        # ⚠⚠ **Wer tippt, sucht etwas Neues — die alte Antwort muss weg.**
        # Am 04.09.2026 gemeldet: „Nachdem ich boomtube eingegeben habe,
        # bleibt das Eingabefeld ohne Funktion, ich kann nach keinem zweiten
        # Artikel suchen." Es reagierte sehr wohl — nur standen unter dem
        # neuen Vorschlag weiter die 19 Läden des alten Teils, und die
        # füllten den Bildschirm. Was sich nicht sichtbar ändert, gilt als
        # kaputt, und zwar zu Recht.
        #
        # `_waehlen` leert das Feld (`suche.set('')`), bevor es zeichnet —
        # deshalb greift das hier nur beim echten Tippen.
        if text:
            gewaehlt['name'], gewaehlt['kennung'] = '', ''
            _leeren(ergebnis_rahmen)
        # ⚠ **Ohne Suchtext gilt der Filter.** Vorher passierte unter zwei
        # Zeichen gar nichts — und wer nur klickte statt zu tippen, sah nie
        # etwas. Jetzt füllt die Auswahl oben die Liste.
        if len(text) < 2 and not any(wahl[f] for f in FILTER_FOLGE):
            return
        # ⚠ **Teiltext, nicht nur Wortanfang** — wer „chill" tippt, meint
        # `BlastChill`. Dieselbe Überlegung wie bei den Lagerorten.
        # ⭐⭐ **Gesucht wird auch in Bereich und Warengruppe.** Am 04.09.2026
        # gefragt: „Wenn ich Radar suche — sind es Schiffskomponenten,
        # Untergruppe Radar, oder Schiffswaffen?" Genau das muss man nicht
        # wissen müssen: Wer „Radar" tippt, bekommt die Gruppe Radar, egal wo
        # sie einsortiert ist. Gesucht wird in der deutschen **und** der
        # englischen Bezeichnung — die Quelle ist englisch, und viele kennen
        # die Teile nur so.
        namen_memo = {}

        def _heuhaufen(b):
            schluessel = (b.get('bereich'), b.get('gruppe'))
            if schluessel not in namen_memo:
                namen_memo[schluessel] = (' '.join(
                    (_bereichsname(schluessel[0] or ''),
                     _gruppenname(schluessel[1] or ''),
                     schluessel[0] or '', schluessel[1] or ''))).lower()
            return ((b.get('name') or '') + ' '
                    + (b.get('hersteller') or '')).lower() + ' ' \
                + namen_memo[schluessel]

        # Nach Warengruppe gebündelt — die Gliederung steht unten.
        gruppiert = {}
        gesamt = 0
        for b in _teile():
            if any(wahl[f] and b.get(f) != wahl[f] for f in FILTER_FOLGE):
                continue
            if text and text not in _heuhaufen(b):
                continue
            gruppiert.setdefault(b.get('gruppe') or '', []).append(b)
            gesamt += 1
        if not gesamt:
            _liste_zeigen()
            _body_text(vorschlag_rahmen, t('s_ld_nichts_gefunden'),
                        fenster.f_small, fill='x')
            return
        _liste_zeigen()

        def _zeile_bauen(b):
            """Eine Zeile: Name links, Größe und Hersteller rechts.

            ⚠⚠ **Die Größe gehört an die Zeile, nicht nur ins Filtermenü.**
            Am 05.09.2026 gefragt: „Ein Spieler, der nicht alle
            Quantenantriebe kennt — wie findet er einen für sein Schiff
            passenden?" Über die Größe. Die nützt ihm aber nur, wenn sie
            dasteht: Eine Liste aus 44 Fantasienamen (Erebos, Flash, Goliath)
            sagt ihm nichts, dieselbe Liste mit „Größe 1 · Wen-Cassel" schon.
            """
            name, kennung = b['name'], b['kennung']
            kasten = tk.Frame(vorschlag_rahmen, bg=SURFACE, cursor='hand2')
            kasten.pack(fill='x')
            zeile = tk.Label(kasten, text='   ' + name, bg=SURFACE, fg=FG,
                             font=fenster.f_small, anchor='w',
                             cursor='hand2')
            zeile.pack(side='left', ipady=4)
            # ⭐ Klasse · Größe · Güte · Hersteller — dieselben vier Angaben,
            # nach denen auch die Quelle selbst filtert. Was fehlt, fällt weg.
            beiwerk = ' · '.join(x for x in (
                _wertname('klasse', b['klasse']) if b.get('klasse') else '',
                t('s_ld_groesse') % b['groesse'] if b.get('groesse') else '',
                t('s_ld_guete') % b['guete'] if b.get('guete') else '',
                b.get('hersteller') or '') if x)
            teile_der_zeile = [kasten, zeile]
            if beiwerk:
                rechts = tk.Label(kasten, text=beiwerk + '   ', bg=SURFACE,
                                  fg=SUB, font=fenster.f_small, anchor='e',
                                  cursor='hand2')
                rechts.pack(side='right', ipady=4)
                teile_der_zeile.append(rechts)
            # ⚠ Jedes Stück der Zeile hört auf denselben Klick — sonst trifft
            # man neben dem Namen ins Leere.
            for stueck in teile_der_zeile:
                stueck.bind('<Button-1>',
                            lambda _=None, n=name, k=kennung: _waehlen(n, k))
                stueck.bind('<Enter>',
                            lambda _=None: zeile.configure(fg=ACCENT))
                stueck.bind('<Leave>',
                            lambda _=None: zeile.configure(fg=FG))

        # ⚠⚠ **Eine Gruppe → flache Liste. Mehrere → gegliedert.**
        # Am 04.09.2026 gemeldet: „Was gehört alles zu Systemen? Blicke da
        # nicht durch — die Auswahl bei der Bauplan-Liste ist deutlich feiner
        # und besser untergliedert." Stimmt: Dort stehen Zwischenüberschriften
        # je Art. Ohne sie ist „Systeme (176)" eine Namensreihe, aus der
        # niemand ablesen kann, was überhaupt dazugehört.
        #
        # Der Deckel greift deshalb **je Gruppe**, nicht auf die ganze Liste:
        # Sonst füllte die erste Gruppe alle 40 Zeilen und die übrigen drei
        # blieben unsichtbar — genau die, nach denen gefragt wurde.
        if len(gruppiert) == 1:
            # ⚠⚠ **Eine Gruppe wird VOLLSTÄNDIG gezeigt — kein Deckel.**
            # Am 05.09.2026: „Entweder komplette Liste, und Size dabei, oder
            # der User braucht keine Liste — er findet eh nichts, und wenn er
            # keine Namen kennt, bringt ihm die Liste nichts." Genau so: Wer
            # sich bis auf eine Warengruppe durchgeklickt hat, will sie ganz
            # sehen. Bei 87 Geschützen 40 zu zeigen und auf „tipp genauer" zu
            # verweisen, hilft niemandem, der die Namen nicht kennt.
            #
            # ⚠ Der Notdeckel bleibt, damit ein Ausreißer in fremden Daten
            # nicht Tausende Zeilen baut. Die größte echte Gruppe hat 201.
            (_nur_gruppe, eintraege), = gruppiert.items()
            gezeigt = eintraege[:EMERGENCY_CAP]
            for b in gezeigt:
                _zeile_bauen(b)
            if len(eintraege) > len(gezeigt):
                tk.Label(vorschlag_rahmen,
                         text='   ' + t('s_ld_mehr_da') % (len(gezeigt),
                                                           len(eintraege)),
                         bg=SURFACE, fg=SUB, font=fenster.f_small,
                         anchor='w').pack(fill='x', ipady=5)
            return

        for gruppe, eintraege in sorted(gruppiert.items(),
                                        key=lambda p: (-len(p[1]),
                                                       p[0].lower())):
            tk.Label(vorschlag_rahmen,
                     text='  %s (%d)' % (_gruppenname(gruppe), len(eintraege)),
                     bg=SURFACE, fg=ACCENT, font=fenster.f_bold,
                     anchor='w').pack(fill='x', ipady=5)
            for b in eintraege[:PER_GROUP]:
                _zeile_bauen(b)
            rest = len(eintraege) - PER_GROUP
            if rest > 0:
                # ⚠⚠ **Anklickbar, nicht nur eine Feststellung.** Am
                # 05.09.2026: „Gruppen zeigen zu wenig." Der Weg zum Rest war
                # da — man musste ihn nur oben im Menü suchen. Ein Klick auf
                # die Zeile, die den Rest ankündigt, ist der kürzere: Er setzt
                # genau diese Warengruppe als Filter.
                mehr = tk.Label(vorschlag_rahmen,
                                text='   ' + t('s_ld_weitere') % rest,
                                bg=SURFACE, fg=SUB, font=fenster.f_small,
                                anchor='w', cursor='hand2')
                mehr.pack(fill='x', ipady=4)
                mehr.bind('<Button-1>',
                          lambda _=None, g=gruppe: _gruppe_aufklappen(g))
                mehr.bind('<Enter>',
                          lambda _=None, w=mehr: w.configure(fg=ACCENT))
                mehr.bind('<Leave>',
                          lambda _=None, w=mehr: w.configure(fg=SUB))

    # ⚠⚠ **Der Katalog wird beim Öffnen der Seite geholt, nicht beim Start.**
    # Er kostet rund 76 Abrufe und eine knappe Minute (gemessen: 57 s) — das
    # gehört nicht in den Programmstart, wo es niemand angefordert hat. Wer
    # diese Seite öffnet, will genau diese Auskunft; solange sie fehlt, steht
    # die ungefilterte Liste da und ein Hinweis darüber.

    def _stand_melden():
        """Sagen, wie viele Teile bereitstehen — statt einer leeren Fläche."""
        anzahl = len(_teile())
        if not anzahl:
            stand_zeile.pack_forget()
            return
        stand_zeile.configure(text=t('s_ld_nur_kaufbar') % anzahl, fg=SUB)
        stand_zeile.pack(side='left', fill='x', expand=True)

    def _katalog_anstossen():
        if laden_modul.catalog_ready() or zustand_katalog['laeuft']:
            _stand_melden()
            return
        zustand_katalog['laeuft'] = True
        stand_zeile.configure(text=t('s_ld_katalog_laeuft'), fg=GOLD)
        # ⚠ Die Zeile sitzt im festen Kopf, neben dem Reset — nicht in der
        # Rollfläche. Bei 168 Zeilen darüber sähe den Hinweis sonst niemand;
        # am 04.09.2026 genau so passiert: Die Liste war ungefiltert, der
        # Grund stand außer Sicht, und das Werkzeug wirkte schlicht kaputt.
        stand_zeile.pack(side='left', fill='x', expand=True)

        def arbeit():
            def melden(fertig, gesamt):
                def zeigen():
                    try:
                        if stand_zeile.winfo_exists():
                            stand_zeile.configure(
                                text=t('s_ld_katalog_stand') % (fertig, gesamt))
                    except tk.TclError:
                        pass
                try:
                    stand_zeile.after(0, zeigen)
                except Exception:
                    # ⚠⚠ **`Exception`, nicht nur `tk.TclError`.** Wird das
                    # Fenster geschlossen, während dieser Faden noch lädt,
                    # wirft Tk `RuntimeError: main thread is not in main
                    # loop` — eine andere Ausnahme, die hier durchrutschte
                    # und im Fehlerprotokoll des Nutzers landete. Beim
                    # Abnahme-Durchlauf am 06.09.2026 gefunden.
                    pass
            try:
                laden_modul.fetch_catalog(progress=melden)
            except Exception as ausnahme:
                fehler.merken('seiten.shops.katalog', ausnahme)
            # ⚠ Die Schiffsdaten gehören zum selben Aufwasch — ohne sie
            # fehlte der Bereich „Schiffe" in der Liste.
            try:
                from . import ships as schiff_modul
                schiff_modul.update()
            except Exception as ausnahme:
                fehler.merken('seiten.shops.schiffe_holen', ausnahme)

            def fertig():
                zustand_katalog['laeuft'] = False
                try:
                    # ⭐ Die Zeile verschwindet nicht, sie sagt jetzt etwas
                    # anderes: wie viele Teile bereitstehen. Eine Seite, auf
                    # der man erst etwas auswählen muss, sieht sonst leer aus
                    # — und leer wirkt kaputt.
                    _stand_melden()
                    # ⚠ Auch die Menüs neu bauen — ihre Zahlen stammen von
                    # vor dem Abruf. Siehe `_filter_bauen`.
                    _filter_bauen()
                    _vorschlaege()
                except tk.TclError:
                    pass
            try:
                stand_zeile.after(0, fertig)
            except tk.TclError:
                zustand_katalog['laeuft'] = False

        threading.Thread(target=arbeit, daemon=True).start()

    suche.trace_add('write', _vorschlaege)

    def _filter_gewechselt():
        gewaehlt['name'], gewaehlt['kennung'] = '', ''
        _leeren(ergebnis_rahmen)
        # ⚠ Was nach der Änderung nicht mehr passt, fällt weg — sonst bliebe
        # eine Auswahl stehen, die null Treffer hat. Von vorn nach hinten
        # durch die Kaskade, damit sich die Prüfungen aufeinander stützen.
        for stelle, feld in enumerate(FILTER_FOLGE):
            if not wahl[feld]:
                continue
            vorher = FILTER_FOLGE[:stelle]
            passend = {b.get(feld) for b in _teile()
                       if all(not wahl[f] or b.get(f) == wahl[f]
                              for f in vorher)}
            if wahl[feld] not in passend:
                wahl[feld] = ''
        _vorschlaege()
        # ⚠ **`after_idle`, nicht sofort.** `_filter_bauen` zerstört genau
        # die Menüs, aus deren Klick wir gerade kommen — mitten im eigenen
        # Rückruf ist das ein Griff ins Leere.
        try:
            filter_rahmen.after_idle(_filter_bauen)
        except tk.TclError:
            pass

    def _filter_bauen():
        """Die Auswahlmenüs (neu) aufbauen.

        ⚠⚠ **Muss nach dem Katalog-Abruf noch einmal laufen.** Die Zahlen in
        den Menüs entstehen aus der gefilterten Liste — vor dem Abruf steht
        dort „FPS-Waffen (168)", danach sind es 60. Ohne diesen zweiten
        Aufbau bleibt die alte Zahl stehen, und das Werkzeug behauptet etwas,
        das es selbst schon besser weiß.

        Am 04.09.2026 genau so aufgefallen: Seite geöffnet, während der Abruf
        noch lief, ungefilterte Liste gesehen — und der Changelog behauptete
        das Gegenteil.
        """
        _leeren(filter_rahmen)
        # ⚠ Ein Menü ohne echte Auswahl lässt `_filterleiste` selbst weg — so
        # verschwindet „Größe" bei Rüstung von allein, wo die Angabe fehlt.
        _filter_bar(fenster, filter_rahmen,
                      [('bereich', t('s_ld_alle_bereiche'),
                        _mit_zahl('bereich')),
                       ('gruppe', t('s_ld_alle_arten'), _mit_zahl('gruppe')),
                       ('groesse', t('s_ld_alle_groessen'),
                        _mit_zahl('groesse')),
                       ('klasse', t('s_ld_alle_klassen'),
                        _mit_zahl('klasse')),
                       ('guete', t('s_ld_alle_gueten'), _mit_zahl('guete'))],
                      _filter_gewechselt, wahl)

    _filter_bauen()

    def _ld_zuruecksetzen(_=None):
        """Alle fünf Menüs, das Suchfeld und das gewählte Teil zurück.

        ⚠ **Der Katalog bleibt.** Zurückgesetzt wird, was man eingestellt hat
        — nicht, was das Werkzeug geholt hat. Ihn wegzuwerfen hieße eine
        Minute Abruf für nichts.
        """
        for feld in FILTER_FOLGE:
            wahl[feld] = ''
        gewaehlt['name'], gewaehlt['kennung'] = '', ''
        suche.set('')
        _liste_leeren()
        _leeren(ergebnis_rahmen)
        _filter_bauen()
        _stand_melden()
        _scroll_to_top(innen)

    ld_reset.bind('<Button-1>', _ld_zuruecksetzen)
    ld_reset.bind('<Enter>', lambda _=None: ld_reset.configure(fg=RED))
    ld_reset.bind('<Leave>', lambda _=None: ld_reset.configure(fg=SUB))

    # ⚠ Beim erneuten Betreten der Seite steht sonst der alte Suchbegriff noch
    # da — eine Seite wird nur EINMAL gebaut.
    def _beim_zeigen():
        suche.set('')
        _liste_leeren()
        _katalog_anstossen()
    fenster.on_show['laeden'] = _beim_zeigen
    _katalog_anstossen()


def _shop_row(fenster, eltern, bauplan):
    """„Fertig kaufen: X aUEC bei Y" — oder gar nichts.

    ⚠⚠ **Der Abruf läuft im Hintergrund, nicht im Klick.** Wer einen Bauplan
    aufklappt, wartet sonst auf eine fremde Schnittstelle — und bei
    ausgefallenem Netz volle 30 Sekunden auf ein Zeitlimit. Die Zeile erscheint
    einfach nach, wenn die Antwort da ist.

    ⚠ **Drei verschiedene Zustände, drei verschiedene Anzeigen:**

    | Lage | was steht da |
    |---|---|
    | noch nicht nachgesehen | nichts (die Zeile kommt nach) |
    | UEX kennt das Teil nicht | nichts — es gibt nichts zu sagen |
    | Läden bekannt | der billigste, mit Ort |

    „Nirgends im Handel" wird **nicht** behauptet: UEX hat Lücken (gemessen:
    435 von 1.604 Bauplänen), und eine Lücke in fremden Daten ist keine
    Aussage über das Spiel.
    """
    from . import crafting as herst_modul, shops
    try:
        kennung = herst_modul.entity_of(bauplan)
    except Exception as ausnahme:
        fehler.merken('seiten.laden_zeile.kennung', ausnahme)
        return
    if not kennung:
        return

    lbl = tk.Label(eltern, text='', bg='#0c1017', fg=SUB,
                   font=fenster.f_small, anchor='w')

    def zeigen():
        bester = shops.cheapest(kennung)
        if not bester:
            return
        preis, laden, ort = bester
        wo = ' · '.join(x for x in (laden, ort) if x)
        lbl.configure(text=t('s_he_fertig_kaufen') % (_money(preis), wo))
        lbl.pack(fill='x', padx=12, pady=(6, 0))

    if shops.known(kennung):
        zeigen()
        return

    # Noch nichts da — im Hintergrund nachschlagen und dann nachtragen.
    def arbeit():
        try:
            # ⚠ Der Name kommt als Rückfall mit: UEX führt manche Teile unter
            # einer anderen Kennung als das Spiel (gemessen bei den
            # CF-Repeatern). Siehe `scbp/shops.py`.
            shops.fetch(kennung, name=bauplan)
        except Exception as ausnahme:
            fehler.merken('seiten.laden_zeile.holen', ausnahme)
            return
        # ⚠ Zurück in den Oberflächen-Faden: Tk verträgt keine Zugriffe aus
        # einem fremden Thread. Und das Etikett kann inzwischen zerstört sein,
        # wenn jemand die Seite gewechselt hat.
        def nachtragen():
            try:
                if lbl.winfo_exists():
                    zeigen()
            except tk.TclError:
                pass
        try:
            lbl.after(0, nachtragen)
        except tk.TclError:
            pass

    threading.Thread(target=arbeit, daemon=True).start()


def _blueprint_specs(bauplan):
    """Klasse, Größe und Güte eines Bauplans, ausgeschrieben — oder `''`.

    Beispiel: `Militär · Größe 4 · Güte A`

    ⚠ **Was fehlt, fällt weg — es wird nichts erfunden.** Bei Rüstung und
    FPS-Waffen stehen Größe und Güte zwar in den Rohdaten, bedeuten dort aber
    nichts (siehe `catalog._values`). Wo der Katalog keine Klasse führt, gibt es
    auch keine; ein „–" an dieser Stelle wäre eine Angabe, die keine ist.
    """
    from . import catalog as kat_daten
    from .bestandsfenster import GRAD_BUCHSTABE
    eintrag = (kat_daten.load().get('bauplaene') or {}).get(
        pfade.namensform(bauplan or ''))
    if not eintrag:
        return ''
    # ⚠⚠ **Bei Rüstung und FPS-Waffen wird NICHTS gezeigt.** In den Rohdaten
    # trägt jeder Helm brav eine Größe und eine Güte — sie bedeuten dort aber
    # nichts. Gemessen am 06.09.2026: Das „A03 Sniper Rifle" kam als „Größe 3 ·
    # Güte A" heraus, was frei erfunden ist. Dieselbe Falle steht in
    # `reference_scmdb_craftdaten` und in `catalog._values`: Werte nur zeigen,
    # wo sie eine Bedeutung haben.
    art = eintrag.get('a') or ''
    if art.startswith('Char_') or art in ('WeaponPersonal', 'WeaponAttachment'):
        return ''
    teile = []
    schluessel = CLASS_LABELS.get(eintrag.get('c'))
    if schluessel:
        teile.append(t(schluessel))
    if eintrag.get('s'):
        teile.append(t('s_ld_groesse') % eintrag['s'])
    grad = GRAD_BUCHSTABE.get(eintrag.get('g'))
    if grad:
        teile.append(t('s_ld_guete') % grad.upper())
    return '  ·  '.join(teile)


def _fetch_slots(widget, erzwingen=False, danach=None):
    """Fehlende Steckplatz-Daten im Hintergrund holen.

    ⚠ Nicht nur auf der Hangar-Seite: Wer nach einem Update zuerst die
    Herstellung öffnet, bekäme sonst bei jedem Bauplan „noch keine Daten" und
    müsste erst erraten, dass ein Besuch im Hangar hilft. Die Daten fehlen —
    also werden sie geholt, egal von wo aus jemand fragt.

    ⚠ Höchstens **einmal je Programmlauf**: Bei Konzeptschiffen gibt es dauerhaft
    nichts zu holen, und ein Bauplan-Klick soll keinen Abruf auslösen, der beim
    letzten Mal schon nichts brachte.

    ⚠⚠ **`erzwingen=True`, wenn gerade ein Schiff dazugekommen ist.** Genau
    daran scheiterte die Wunschliste: Der Hangar hatte beim Programmstart schon
    einmal nachgezogen, die Sperre stand — und ein frisch eingetragenes
    Wunschschiff bekam nie seine Daten. Auf der Karte stand dann „Für dieses
    Schiff liegen keine Steckplatz-Daten vor", und die Ausstattung blieb leer.
    Am 06.09.2026 gemeldet mit „kann da nichts auswählen".

    Die Sperre bleibt für den Normalfall richtig — sie verhindert einen Abruf
    bei **jedem** Bauplan-Klick. Sie darf nur nicht gelten, wenn sich die Liste
    der Schiffe wirklich geändert hat.

    `danach` wird nach dem Holen im Oberflächen-Faden aufgerufen, damit die
    Seite die frischen Daten auch zeigt.
    """
    if _REFRESHED[0] and not erzwingen:
        return
    _REFRESHED[0] = True

    def arbeit():
        geholt = 0
        try:
            from . import fleet as meine
            geholt = meine.fetch_missing() or 0
        except Exception as ausnahme:
            fehler.merken('seiten.steckplaetze_nachziehen', ausnahme)
        if geholt and danach is not None:
            # ⚠ Zurück in den Oberflächen-Faden — Tk aus einem Thread heraus
            # anzufassen führt zu Abstürzen, die sich nicht nachstellen lassen.
            try:
                widget.after(0, danach)
            except Exception:
                pass

    threading.Thread(target=arbeit, daemon=True).start()


# Ob in diesem Programmlauf schon einmal nachgezogen wurde.
_REFRESHED = [False]


def _fits_row(fenster, eltern, bauplan):
    """„Passt in dein Schiff" — die Antwort auf die Frage nach dem Bauplan.

    ⭐⭐ Das ist die Auskunft, die **keine fremde Seite geben kann**: Erkul
    kennt die Steckplätze jedes Schiffs, aber nicht deinen Hangar; der Watcher
    kennt deinen Hangar und deine Baupläne, aber nicht die Steckplätze. Erst
    zusammen wird daraus „der Kühler passt in die Cutlass, nicht in den Arrow".

    ⚠⚠ **Drei Fälle, die auseinandergehalten werden müssen** — sie sehen im
    Code gleich aus (immer eine leere Liste) und bedeuten Verschiedenes:

    | Lage | was gesagt wird |
    |---|---|
    | Hangar leer | „trag deine Schiffe ein" — **kein** „passt nirgends" |
    | Teil hat keine Größe (Rüstung, FPS-Waffen) | gar nichts |
    | Hangar voll, nichts passt | „passt in keines deiner Schiffe" |

    Wer den ersten Fall wie den dritten behandelt, sagt jedem Neuling, sein
    frisch freigeschalteter Bauplan sei nutzlos.
    """
    from . import erkul, fleet as meine, catalog as kat_daten

    eintrag = (kat_daten.load().get('bauplaene') or {}).get(
        pfade.namensform(bauplan or ''))
    if not eintrag:
        return
    art, groesse = eintrag.get('a'), eintrag.get('s')
    # ⚠ Ohne Größe keine Aussage. Rüstung, Helme und FPS-Waffen tragen in den
    # Daten zwar eine — sie bedeutet dort aber nichts, und erkul führt sie
    # ohnehin nicht. Lieber nichts sagen als etwas Erfundenes.
    if not art or not groesse:
        return

    schiffe = (meine.load().get('schiffe') or [])
    if not schiffe:
        tk.Label(eltern, text=t('s_hg_passt_leer'), bg='#0c1017', fg=SUB,
                 font=fenster.f_small, anchor='w').pack(fill='x', padx=12,
                                                        pady=(6, 0))
        return

    # ⚠⚠ **Schiffe im Hangar heißen nicht, dass ihre Steckplätze da sind.**
    # Genau diese Verwechslung hat am 06.09.2026 eine falsche Auskunft erzeugt:
    # Nach dem Umstieg auf ein neues Ablage-Format war die Steckplatz-Datei
    # ungültig und wurde verworfen — die Herstellung meldete daraufhin bei
    # **jedem** Bauplan „passt in keines deiner Schiffe", obwohl eine S4-Waffe
    # in 48 Plätze gepasst hätte.
    #
    # „Keine Daten" und „passt nicht" sehen im Code gleich aus (eine leere
    # Liste) und bedeuten das Gegenteil voneinander. Wer sie zusammenwirft,
    # behauptet etwas, das er nicht weiß — und das ist schlimmer, als nichts
    # zu sagen.
    if not (erkul.load().get('schiffe') or {}):
        lbl = tk.Label(eltern, text=t('s_hg_passt_unbekannt'), bg='#0c1017',
                       fg=GOLD, font=fenster.f_small, anchor='w',
                       justify='left')
        lbl.pack(fill='x', padx=12, pady=(6, 0))
        _wrap(lbl, inset=36)
        _fetch_slots(lbl)
        return

    treffer = erkul.matching_ships(art, groesse, schiffe)
    if treffer:
        namen = ', '.join(
            t('s_hg_passt_mehrfach').format(name=n, n=z) if z > 1 else n
            for n, z in treffer)
        text, farbe = t('s_hg_passt_in').format(schiffe=namen), ACCENT
    else:
        text, farbe = t('s_hg_passt_nirgends'), GOLD
    # ⚠⚠ **Fett und farbig — Grau wird nicht gelesen.** Die Zeile stand hier
    # zuerst in `SUB` (dem Grau für Nebensächliches) unter einem langen
    # Rezeptblock. Rückmeldung dazu am 06.09.2026: *„in Grau nimmt es keiner
    # wahr und fragt sich dann, wo er die Info findet"* — von jemandem, der
    # wusste, dass es die Auskunft gibt, und sie trotzdem übersah.
    #
    # Beide Fälle sind Antworten und beide gehören gesehen: Grün „passt in",
    # Gold „passt nirgends". `SUB` bleibt dem vorbehalten, was man überlesen
    # darf.
    lbl = tk.Label(eltern, text=text, bg='#0c1017', fg=farbe,
                   font=fenster.f_bold, anchor='w', justify='left')
    lbl.pack(fill='x', padx=12, pady=(8, 2))
    _wrap(lbl, inset=36)


def _crafting_row(fenster, eltern, eintrag, offen, neu_zeichnen):
    """Eine Zeile der Herstellungs-Liste, auf Klick klappt das Rezept auf."""
    from . import crafting as herst_modul
    zeile = tk.Frame(eltern, bg=BG, cursor='hand2')
    zeile.pack(fill='x', pady=1)

    # Drei Zustände, nicht zwei: habe / fehlt / **unklar**.
    if eintrag['habe'] is True:
        zeichen_text, farbe = '✓', ACCENT
    elif eintrag['habe'] is None:
        zeichen_text, farbe = '?', GOLD
    else:
        zeichen_text, farbe = '·', SUB
    tk.Label(zeile, text=zeichen_text, bg=BG, fg=farbe, font=fenster.f_base,
             width=2).pack(side='left')
    # ⚠⚠ **Die aufgeklappte Zeile muss sich abheben.** Am 31.08.2026 gemeldet:
    # „nicht klar genug, welcher Bauplan bei Herstellung ausgewaehlt ist,
    # steht auch nirgends." Sie sah aus wie jede andere — und sobald man ein
    # Stueck gerollt hatte, war der Name oben aus dem Bild.
    _offen = offen['name'] == eintrag['name']
    tk.Label(zeile, text=eintrag['name'], bg=BG,
             fg=ACCENT if _offen else FG,
             font=fenster.f_bold if _offen else fenster.f_base,
             anchor='w').pack(side='left', fill='x', expand=True)
    if eintrag['hersteller']:
        tk.Label(zeile, text=eintrag['hersteller'], bg=BG, fg=SUB,
                 font=fenster.f_small, anchor='e').pack(side='right', padx=(8, 0))

    def umschalten(*_):
        offen['name'] = None if offen['name'] == eintrag['name'] else eintrag['name']
        neu_zeichnen()

    for w in (zeile,) + tuple(zeile.winfo_children()):
        w.bind('<Button-1>', umschalten)
    icons.hover_row(zeile, BG, SURFACE)

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
             font=fenster.f_bold, anchor='w').pack(side='left')
    if eintrag['hersteller']:
        tk.Label(_kopf, text='  ·  %s' % eintrag['hersteller'], bg='#0c1017',
                 fg=SUB, font=fenster.f_small, anchor='w').pack(side='left')
    # ⭐⭐ **Klasse, Größe und Güte gehören hierher.** Die Bauplan-Liste zeigt
    # sie als Kürzel („M/1/A"), die Herstellung zeigte sie gar nicht — dabei
    # ist genau hier die Stelle, an der jemand entscheidet, ob er das Teil
    # überhaupt bauen will. Gemeldet am 06.09.2026: *„in Herstellung finde ich
    # auch nicht raus, welche Size etwas hat oder ob es Military ist."*
    #
    # ⚠ **Ausgeschrieben, nicht als Kürzel.** In der Liste ist „M/1/A" richtig,
    # weil dort 738 Zeilen untereinander stehen und jede Spalte zählt. Hier
    # steht eine einzige Zeile über einem langen Kasten — da hilft „Militär ·
    # Größe 4 · Güte A" mehr als drei Buchstaben, die man erst übersetzen muss.
    angaben = _blueprint_specs(eintrag.get('basis'))
    if angaben:
        tk.Label(_kopf, text='  ·  %s' % angaben, bg='#0c1017',
                 fg=FG, font=fenster.f_small, anchor='w').pack(side='left')

    if eintrag['habe'] is None:
        _body_text(block, t('s_he_unklar'), fenster.f_small, fill='x')

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
    if eintrag['habe'] is not True and _has_source(eintrag.get('basis')):
        _button(fenster, block, t('s_he_woher_bp'),
               lambda n=eintrag.get('basis'): _to_blueprint(fenster, n)).pack(
                   anchor='w', padx=12, pady=(8, 0))
    # ⭐⭐ **„Lohnt sich das Bauen überhaupt?"** Die Zutatenkosten stehen
    # unten schon Stück für Stück da — was fehlte, war die andere Hälfte:
    # Was kostet dasselbe Teil fertig im Regal?
    #
    # ⚠ Zugeordnet wird über die **Entitäts-Kennung**, nie über den Namen.
    # Über Namen ist es hier schon einmal schiefgegangen (`Gold` lieferte
    # `Golden Medmon` mit). Siehe `scbp/shops.py`.
    _shop_row(fenster, block, eintrag.get('basis'))
    # ⭐⭐ **„Und passt das überhaupt in mein Schiff?"** Die Frage, die auf
    # jeden neuen Bauplan folgt. Steht direkt unter dem Ladenpreis, weil beide
    # dieselbe Entscheidung tragen: bauen, kaufen — oder gar nicht, weil es
    # nirgends hineinpasst.
    _fits_row(fenster, block, eintrag.get('basis'))

    rez = herst_modul.recipe(eintrag['basis'])
    from . import materials as lager
    from . import prices as preis_modul
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
                         font=fenster.f_small, anchor='w')

        # ⭐ Stückzahl daneben. Wer zehn Stück am Stück baut, soll einmal
        # klicken statt zehnmal — beim elften Klick stimmt der Bestand sonst
        # nicht mehr, und niemand merkt es.
        anzahl_var = tk.StringVar(value='1')

        def hergestellt(_e=None, zutaten=stufe['zutaten'], lbl=rueck,
                        var=anzahl_var):
            wie_oft = lager.parse_number(var.get())
            # Unsinn im Feld heisst 1 — lieber einmal abziehen als gar nichts
            # tun und den Nutzer raten lassen, warum nichts passiert.
            wie_oft = 1 if not wie_oft or wie_oft < 1 else int(wie_oft)
            ok, fehlt = lager.deduct(zutaten, wie_oft)
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
            _sperre, _wirkung, _zerlege_sekunden = herst_modul.dismantle_block()
        except Exception:
            _sperre, _wirkung = set(), 0.5
        _betroffen = [r for _s, r, _m, _g in stufe['zutaten']
                      if r and lager.norm_material(r) in
                      {lager.norm_material(x) for x in _sperre}]
        if _betroffen:
            _body_text(block, t('s_he_zerlegen') % (_wirkung * 100,
                                                     ', '.join(dict.fromkeys(_betroffen))),
                        fenster.f_small, fill='x')

        _button(fenster, reihe, t('s_lg_bauen'), hergestellt).pack(side='left')
        tk.Label(reihe, text=t('s_lg_anzahl'), bg='#0c1017', fg=SUB,
                 font=fenster.f_small).pack(side='left', padx=(12, 6))
        from .main_window import round_entry as _rf_anzahl
        _anzahl_feld = _rf_anzahl(reihe, anzahl_var, fenster.f_small,
                                  '#0c1017', LINE, ACCENT, FG)
        _anzahl_feld.holder.configure(width=70)
        _anzahl_feld.holder.pack(side='left')
        rueck.pack(side='left', padx=(10, 0))

        # ⭐⭐ **Vormerken — der kurze Weg zur Materialliste.** Bis v3.20.0
        # führte er nur über ein Schiff: erst auf die Wunschliste, dann
        # Steckplätze belegen, dann stand das Material da. Für einen Helm,
        # eine Rüstung oder eine FPS-Waffe gab es ihn **gar nicht**, obwohl
        # das genauso Baupläne mit Rohstoffbedarf sind.
        #
        # Gemeldet von Haldjas am 06.09.2026: „‚What to farm' ist irgendwie
        # bisschen unnötig komplex — man geht da rein, wird dann zu ‚still
        # missing' geschickt und weiß dann aber nicht so genau, was man machen
        # soll." Genau hier, wo das Rezept steht, ist die Stelle, an der man es
        # sich vornimmt.
        #
        # ⚠ Die Stückzahl daneben wird mitgenommen: Wer drei Helme bauen will,
        # braucht dreifaches Material.
        from . import fleet as _mz_hangar

        # ⚠ Der Name kommt aus dem Eintrag der Herstellungsliste — `bauplan`
        # gibt es in dieser Funktion nicht, das ist die Nachbarfunktion
        # `_fits_row`.
        _mz_name = eintrag.get('name') or ''
        # ⚠⚠⚠ **NUR eine echte Entitäts-Kennung, niemals der Name.** Hier
        # stand `eintrag.get('ref') or eintrag.get('basis')` — und `basis` ist
        # der Bauplanname. Der landete als `uuid` in der Preisabfrage und
        # erzeugte eine kaputte Adresse:
        #
        #     /2.0/items_prices?uuid=CF-447 Rhino Repeater
        #     InvalidURL: URL can't contain control characters
        #
        # Die Seite „Was noch fehlt" blieb daraufhin leer und versuchte es
        # endlos weiter. Gemeldet am 06.09.2026: „der sucht als was und will
        # was laden, hört aber nicht auf."
        #
        # Ohne Kennung ist der Posten trotzdem vollständig: Das Rezept findet
        # `bauweg()` über den Namen, und ein Ladenpreis steht eben nicht dabei.
        # Lieber keine Angabe als eine erfundene Abfrage.
        _mz_ref = (eintrag.get('ref') or '').strip()

        def _vormerken():
            stand = _mz_hangar.load()
            try:
                wieviel = max(1, int(anzahl_var.get() or 1))
            except (TypeError, ValueError):
                wieviel = 1
            _mz_hangar.notepad_add(
                stand, _mz_name, ref=_mz_ref, count=wieviel)
            # ⚠ Der Gesamtstand wird gespeichert, nicht eine Teilmenge —
            # dieselbe Falle, die einmal die komplette Wunschliste gelöscht
            # hat (siehe `_save_entry`).
            _mz_hangar.save(stand)
            # ⚠ Der Knopf ist eine Leinwand mit gezeichnetem Text und lässt
            # sich nicht umbeschriften. Die Rückmeldung steht deshalb daneben —
            # sichtbar, ohne den Knopf neu zu bauen.
            merk_stand.configure(text=t('s_mz_drauf'), fg=ACCENT)

        _button(fenster, reihe, t('s_mz_knopf'),
               _vormerken).pack(side='left', padx=(12, 0))
        merk_stand = tk.Label(
            reihe, bg='#0c1017', fg=ACCENT, font=fenster.f_small,
            text=(t('s_mz_drauf') if _mz_hangar.notepad_contains(
                _mz_hangar.load(), _mz_name) else ''))
        merk_stand.pack(side='left', padx=(8, 0))

        # Eine Zeile, die sagt, was der Knopf tut — sonst rät man.
        _body_text(block, t('s_lg_bauen_hilfe'), fenster.f_small, fill='x')
        _body_text(block, t('s_mz_hilfe'), fenster.f_small, fill='x')

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
            tk.Label(z, text=slot, bg='#0c1017', fg=SUB, font=fenster.f_small,
                     width=18, anchor='w').pack(side='left')
            # ⭐ Der Sprung: Klick auf den Rohstoff öffnet den Bergbau mit
            # diesem Namen in der Suche. Das ist der Grund, warum die
            # Detailfläche kurz bleiben darf — man springt, statt zu stapeln.
            roh_lbl = tk.Label(z, text=rohstoff, bg='#0c1017', fg=ACCENT,
                               font=fenster.f_base, anchor='w',
                               cursor='hand2')
            roh_lbl.pack(side='left')

            def zum_bergbau(_e=None, name=rohstoff):
                fenster.mining_search = name
                fenster.jump_to('bergbau')

            roh_lbl.bind('<Button-1>', zum_bergbau)
            menge_lbl = tk.Label(z, text='', bg='#0c1017', fg=SUB,
                                 font=fenster.f_small, anchor='e')
            menge_lbl.pack(side='right', padx=12)
            lage_lbl = tk.Label(z, text='', bg='#0c1017', fg=GOLD,
                                font=fenster.f_small, anchor='e')
            guete_lbl = tk.Label(z, text='', bg='#0c1017', fg=SUB,
                                 font=fenster.f_small, anchor='e')
            # ⭐ „kaufen oder abbauen?" — die Frage, die nach „dir fehlt X"
            # kommt. Sieben der 26 Rohstoffe lassen sich NIRGENDS kaufen; fünf
            # davon stehen zusätzlich auf der Zerlege-Sperrliste. Wer das nicht
            # weiß, sucht am Terminal nach etwas, das es dort nie gibt.
            preis_lbl = tk.Label(z, text='', bg='#0c1017', fg=SUB,
                                 font=fenster.f_small, anchor='e')
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
            wie_viele = lager.parse_number(anzahl_var.get())
            wie_viele = 1 if not wie_viele or wie_viele < 1 else int(wie_viele)
            neue_lage = {m: (br, da, f, zug, mq) for m, br, da, f, zug, mq
                         in lager.check(stufe['zutaten'], wie_viele)}
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
                        _p = preis_modul.price(rohstoff)
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
                            text=t('s_he_kaufen') % (_money(_kauf * _fehlt),
                                                     preis_modul.BUY_QUALITY),
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
                     font=fenster.f_small, width=18, anchor='w').pack(side='left')
            tk.Label(z, text=_duration(stufe['zeit']), bg='#0c1017',
                     fg=FG, font=fenster.f_small).pack(side='left')

        # ⭐ Was käme mit DEINEM Material heraus? (Idee von Xharig, 29.08.2026)
        #
        # Die Rezepte tragen die Qualitätswirkung mit: mieses Erz macht ein
        # schlechteres Stück, gutes ein besseres. Das steht in keiner Webseite,
        # weil dort niemand weiß, was im eigenen Frachtraum liegt.
        #
        # ⚠ Nur zeigen, wenn das Lager etwas dazu hergibt — geraten wird nicht.
        qualitaeten = {}
        for _slot, _roh, _mg, _gt in stufe['zutaten']:
            beste = lager.best_quality(_roh, _gt)
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
                                  fg=FG, font=fenster.f_base, anchor='w')
            werte_kopf.pack(fill='x', padx=12, pady=(10, 2))
            werte_rahmen = tk.Frame(block, bg='#0c1017')
            werte_rahmen.pack(fill='x')
            regler_lbl = tk.Label(block, text='', bg='#0c1017', fg=SUB,
                                  font=fenster.f_small, anchor='w')

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
            grundliste = herst_modul.values_with_stock(
                eintrag['basis'], {m: 500.0 for m in alle_materialien})
            zeilen_widgets = []
            for w in grundliste:
                wz = tk.Frame(werte_rahmen, bg='#0c1017')
                wz.pack(fill='x', padx=12, pady=1)
                # ⚠ Uebersetzt ueber den sprachneutralen Schluessel, nicht
                # ueber den englischen Namen — siehe `crafting.property_name`.
                tk.Label(wz, text=herst_modul.property_name(w['eigenschaft'],
                                                          w.get('key')),
                         bg='#0c1017', fg=SUB,
                         font=fenster.f_small, width=22,
                         anchor='w').pack(side='left')
                # ⚠⚠ **Die feste Breite gilt nur für den Faktor.** Als die
                # Prozentzahl in v3.3.0-rc37 dazukam, wurde sie in dasselbe
                # Etikett geschrieben — und `width=9` schnitt sie ab: Auf dem
                # Bildschirm stand „× 1.047  +4.(" statt „+4,70 %". Eine feste
                # Breite ist eine Zusage über den Inhalt; wer Inhalt dazutut,
                # muss sie anfassen.
                faktor_lbl = tk.Label(wz, text='', bg='#0c1017', fg=ACCENT,
                                      font=fenster.f_base, width=9,
                                      anchor='w')
                faktor_lbl.pack(side='left')
                # Eigene Spalte fürs Prozent — so bleiben beide untereinander
                # bündig, statt sich gegenseitig zu verschieben.
                prozent_lbl = tk.Label(wz, text='', bg='#0c1017', fg=ACCENT,
                                       font=fenster.f_base, width=10,
                                       anchor='w')
                prozent_lbl.pack(side='left', padx=(6, 0))
                herkunft_lbl = tk.Label(wz, text='', bg='#0c1017', fg=SUB,
                                        font=fenster.f_small, anchor='e')
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
                                      fg=SUB, font=fenster.f_small, anchor='w')
                spanne_lbl.pack(fill='x', padx=(46, 12))
                zeilen_widgets.append((w, faktor_lbl, prozent_lbl,
                                       herkunft_lbl, spanne_lbl))

            leer_lbl = tk.Label(werte_rahmen, text='', bg='#0c1017', fg=SUB,
                                font=fenster.f_small, anchor='w')

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
                           for w in herst_modul.values_with_stock(
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
            from .main_window import slider as schieberegler
            tk.Label(block, text=t('s_he_regler_kopf'), bg='#0c1017', fg=FG,
                     font=fenster.f_base, anchor='w').pack(
                         fill='x', padx=12, pady=(10, 2))
            # ⭐ Der Satz, der die Regler erst einordnet: Wer kauft, landet
            # immer bei 500 — dem Nullpunkt. Alles darüber muss man selbst
            # abbauen. Ohne diesen Hinweis sieht der Regler nach einer freien
            # Wahl aus, die man am Terminal treffen könnte.
            _body_text(block, t('s_he_kauf_q') % preis_modul.BUY_QUALITY,
                        fenster.f_small, fill='x')

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
                         font=fenster.f_small, width=16, anchor='w').pack(
                             side='left')

                # ⚠ Der Wert MUSS neben dem Regler stehen. Ohne ihn zieht man
                # blind und weiß nicht, welche Qualität man gerade
                # durchspielt — genau der Wert, um den es geht.
                _wert_lbl = tk.Label(reihe_r, text=t('s_lg_q_wert')
                                     % int(stand[_mat]),
                                     bg='#0c1017', fg=ACCENT,
                                     font=fenster.f_base, width=7, anchor='w')

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
                                          gezogen, width=200, bg='#0c1017')
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
                    bg='#0c1017', fg=SUB, font=fenster.f_small, anchor='w')
                _quelle_lbl.pack(side='left', padx=(10, 0))
                regler_zeilen[_mat] = (_wert_lbl, _quelle_lbl, _schieber)

            # Alles wieder auf den eigenen Lagerstand zurückstellen.
            zurueck = tk.Label(block, text=t('s_he_zurueck_lager'),
                               bg='#0c1017', fg=ACCENT, font=fenster.f_small,
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
                        _s.draw(stand[_m])
                    except Exception:
                        pass
                werte_zeichnen()

            if qualitaeten:
                zurueck.pack(anchor='w', padx=12, pady=(2, 0))
                zurueck.bind('<Button-1>', zurueck_zum_lager)
            werte_zeichnen()
            _body_text(block, t('s_he_werte_hinweis'), fenster.f_small,
                        fill='x')



# ------------------------------------------------------------------- Bergbau



def _kind_text(arten):
    """Die Abbauarten lesbar: „FPS · Schiff"."""
    reihenfolge = ('fps', 'fahrzeug', 'schiff', 'schiff_selten')
    return ' · '.join(t('s_bg_art_' + a) for a in reihenfolge if a in arten)


def _has_tool(erz, geraet):
    """Lässt sich dieses Erz mit dem gewählten Gerät abbauen?"""
    from .mining import _pot
    for eintrag in erz.get('orte') or []:
        for art in (eintrag[2] if len(eintrag) > 2 else ()):
            if _pot(art) == geraet:
                return True
    return False


def _mining_share(fenster, zeile, anteil, stufe, grund, allein=False):
    """Rechts an eine Bergbau-Zeile: „18 % · viel".

    ⚠ **Erst die Abbauart packen, dann das hier** — bei `side='right'` sitzt
    das zuerst Gepackte ganz aussen. Andersherum stünde die Prozentzahl mal
    links und mal rechts von der Art, je nachdem welche Zeile man ansieht.

    Ohne Anteil (alte Ablage, Erz ohne Zusammensetzung) bleibt die Zeile leer
    statt „0 %" — eine Null ist eine Aussage, und die hätten wir nicht.
    """
    if not anteil:
        return
    # ⚠⚠ **„100 % · fast nur das" ist an 11 von 26 Fahrzeug-Orten keine
    # Aussage, sondern ein Rechenartefakt**: Dort kennt der ROC schlicht nur
    # dieses eine Mineral, also sind es zwangsläufig 100 %. Die Zahl klingt
    # nach einem Spitzenfundort und ist doch nur eine Feststellung über die
    # Länge der Liste. Deshalb steht dort, was wirklich gemeint ist.
    if allein:
        tk.Label(zeile, text=t('s_bg_einziges'), bg=grund, fg=SUB,
                 font=fenster.f_small, anchor='e').pack(side='right',
                                                        padx=(6, 12))
        return
    # Die Stufe färbt mit: Was sich lohnt, soll man sehen, ohne zu rechnen.
    farbe = ACCENT if stufe >= 5 else FG if stufe >= 3 else SUB
    # ⚠ **Unter einem halben Prozent steht „<1 %", nicht „0 %".** Eine Null
    # neben einem Erz, das in der Liste steht, liest sich wie „gibt es hier
    # nicht" — und dann stimmt die Zeile darüber nicht mehr mit sich selbst
    # überein. Genau so stand Beryl auf Daymar da (0,4 %).
    prozent = round(anteil * 100)
    text = (t('s_bg_anteil_wenig') if prozent < 1
            else t('s_bg_anteil') % prozent)
    tk.Label(zeile, text=t('s_bg_st_%d' % stufe), bg=grund, fg=SUB,
             font=fenster.f_small, anchor='e').pack(side='right', padx=(6, 12))
    tk.Label(zeile, text=text, bg=grund,
             fg=farbe, font=fenster.f_base, anchor='e').pack(side='right')


def _salvage(fenster, rahmen):
    """Was in einem Wrack steckt — und ob sich das Aussteigen lohnt.

    Der Wunsch kam von **Zwaersch (KRT)**; er ist der Grund, warum das
    Werkzeug überhaupt an die Schiffsdaten angeschlossen wurde.

    ⚠ **Eigene Schiffsauswahl, nicht der Hangar.** Ein Wrack ist nicht das
    eigene Schiff — hier geht es um jeden Rumpf, der einem begegnet. Deshalb
    sitzt die Seite auch nicht bei „Mein Hangar".
    """
    from . import salvage as bg, erkul, shops, ships as alle_schiffe

    _heading(fenster, rahmen, t('hf_bergung'), t('s_wr_lead'))
    innen = _scroll_area(rahmen)

    # ⚠⚠ **Die NPC-Warnung steht GANZ OBEN**, nicht unter der Liste. Wer vor
    # einem Spielerwrack steht, muss das lesen, **bevor** er eine Zahl sieht —
    # danach ist die Zahl schon im Kopf. Ein Spielerschiff wird unbrauchbar,
    # sobald die Versicherung beansprucht wird; ausgebaute Teile sind dann
    # wertlos, und nur das Abkratzen der Hülle lohnt.
    warnung = _card(innen, border_color=GOLD, pady=(0, 14))
    _body_text(warnung, _strip_markup(t('s_wr_npc_warnung')), fenster.f_small,
                color=GOLD, bg=SURFACE, fill='x', padx=16, pady=12,
                inset=56)

    schiff = tk.StringVar()
    ergebnis = tk.Frame(innen, bg=BG)
    meldung = {'text': '', 'farbe': SUB}

    tk.Label(innen, text=t('s_wr_schiff'), bg=BG, fg=FG, font=fenster.f_bold,
             anchor='w').pack(fill='x', padx=24)
    _body_text(innen, t('s_wr_such_hilfe'), fenster.f_small, fill='x',
                padx=24, inset=48)

    block = tk.Frame(innen, bg=BG)
    block.pack(fill='x', padx=24, pady=(6, 0))
    zeile, auswahl, _ = _combo_box(fenster, block, schiff,
                                     alle_schiffe.all_names,
                                     empty_text=t('s_hg_nichts_gefunden'),
                                     scrollable=200)
    zeile.pack(fill='x')
    auswahl.pack(fill='x')

    hinweis = tk.Label(innen, text='', bg=BG, fg=SUB, font=fenster.f_small,
                       anchor='w')

    def _zeigen(name, teile, stand=''):
        for kind in ergebnis.winfo_children():
            kind.destroy()
        if not teile:
            # ⚠ „Konzept" nur, wenn UEX es sagt — sonst der neutrale Satz.
            schluessel = ('s_wr_konzept' if alle_schiffe.is_concept(name)
                          else 's_wr_unbekannt')
            _body_text(ergebnis, t(schluessel), fenster.f_small,
                        color=GOLD, fill='x')
            return
        tk.Label(ergebnis, text=t('s_wr_ueberschrift') % name, bg=BG, fg=FG,
                 font=fenster.f_bold, anchor='w').pack(fill='x', pady=(0, 2))
        _body_text(ergebnis, _strip_markup(t('s_wr_werk_hinweis')),
                    fenster.f_small, fill='x', pady=(0, 8))

        def preis_von(ref):
            if not shops.known(ref):
                return None            # nichts nachladen beim Zeichnen
            bester = shops.cheapest(ref)
            return bester[0] if bester else None

        summe, _mit, ohne = bg.value(teile, preis_von)
        tk.Label(ergebnis, text=t('s_wr_wert') % _money(summe), bg=BG,
                 fg=ACCENT, font=fenster.f_bold,
                 anchor='w').pack(fill='x', pady=(0, 2))
        # ⚠ Die Einordnung steht **an** der Zahl, nicht in einer Fußnote:
        # Es ist der Ladenwert, kein Verkaufserlös. Verkaufspreise für
        # Schiffsteile führt kaum ein Händler (gemessen 06.09.2026: drei von
        # vier Werksteilen ohne jedes Ankaufgebot).
        _body_text(ergebnis, _strip_markup(t('s_wr_wert_hinweis')),
                    fenster.f_small, fill='x', pady=(0, 6))
        if ohne:
            _body_text(ergebnis, t('s_wr_ohne_preis') % ohne,
                        fenster.f_small, color=GOLD, fill='x', pady=(0, 6))

        for teil in teile:
            karte = _card(ergebnis, pady=(0, 4))
            kopf = tk.Frame(karte, bg=SURFACE)
            kopf.pack(fill='x', padx=16, pady=(8, 8))
            tk.Label(kopf, text=t('s_wr_stueck') % teil['anzahl'], bg=SURFACE,
                     fg=ACCENT, font=fenster.f_bold, width=4,
                     anchor='w').pack(side='left')
            tk.Label(kopf, text=teil['name'], bg=SURFACE, fg=FG,
                     font=fenster.f_small, anchor='w').pack(side='left')
            merkmale = [teil['art']]
            if teil.get('groesse'):
                merkmale.append('S%s%s' % (teil['groesse'], teil.get('guete') or ''))
            preis = preis_von(teil['ref'])
            rechts = (_money(preis * teil['anzahl']) + ' aUEC' if preis
                      else t('s_wr_kein_preis'))
            tk.Label(kopf, text=rechts, bg=SURFACE,
                     fg=FG if preis else SUB, font=fenster.f_small,
                     anchor='e').pack(side='right')
            tk.Label(kopf, text='  ·  '.join(merkmale), bg=SURFACE, fg=SUB,
                     font=fenster.f_small, anchor='w').pack(side='left',
                                                            padx=(12, 0))
        if stand:
            _body_text(ergebnis, t('s_wr_stand') % stand, fenster.f_small,
                        fill='x', pady=(8, 0))

    def nachsehen():
        name = (schiff.get() or '').strip()
        if not alle_schiffe.knows(name):
            hinweis.configure(text=t('s_wr_kein_schiff'), fg=RED)
            return
        kennung = erkul.ident(name, '', '', '')
        gespeichert = bg.remembered(kennung) if kennung else None
        if gespeichert:
            hinweis.configure(text='', fg=SUB)
            _zeigen(name, gespeichert['teile'])
            return

        hinweis.configure(text=t('s_wr_hole'), fg=SUB)

        # ⚠ In einem eigenen Faden: Ein Schiff holen sind mehrere Abrufe, und
        # die Ladenpreise kommen einzeln nach. Tk verträgt keine Zugriffe aus
        # fremden Fäden — zurück geht es über `after(0, …)`.
        def arbeit():
            try:
                teile, gefunden = _load_salvage(name)
            except Exception as ausnahme:
                fehler.merken('seiten.bergung.holen', ausnahme)
                teile, gefunden = [], ''
            def fertig():
                try:
                    if not ergebnis.winfo_exists():
                        return
                except tk.TclError:
                    return
                hinweis.configure(text='', fg=SUB)
                if gefunden:
                    bg.remember_ship(gefunden, name, teile)
                _zeigen(name, teile)
            try:
                ergebnis.after(0, fertig)
            except tk.TclError:
                pass

        threading.Thread(target=arbeit, daemon=True).start()

    def _vergessen():
        """Die gemerkten Wracks verwerfen — mit Rückfrage, die die Zahl nennt.

        ⚠ Ohne diesen Knopf müsste jemand `bergung.json` von Hand löschen. Ein
        Zwischenspeicher, den nur der Entwickler leeren kann, ist keiner.
        """
        # ⚠⚠ **`ask_yes_no`, nicht `messagebox.askyesno`.** Der
        # System-Dialog sieht auf jedem Schreibtisch anders aus — unter Linux
        # weißer Kasten mit fetter Schrift und englischen Knöpfen („Yes"/„No")
        # mitten in einem deutschen, dunklen Programm. Rückmeldung dazu am
        # 06.09.2026 in zwei Worten: „sieht kacke aus." Der eigene Dialog
        # steht seit v3.0.0 bereit und wird überall sonst benutzt.
        from .main_window import ask_yes_no
        anzahl = len(bg.load().get('schiffe') or {})
        if not anzahl:
            hinweis.configure(text=t('s_wr_nichts_gemerkt'), fg=SUB)
            return
        if not ask_yes_no(fenster.root, t('s_wr_vergessen'),
                             t('s_wr_vergessen_frage') % anzahl,
                             yes_text=t('s_wr_vergessen_ja'), no_text=t('e_abbrechen')):
            return
        weg = bg.forget()
        for kind in ergebnis.winfo_children():
            kind.destroy()
        hinweis.configure(text=t('s_wr_vergessen_ok') % weg, fg=ACCENT)

    reihe = tk.Frame(innen, bg=BG)
    reihe.pack(fill='x', padx=24, pady=(10, 0))
    def _feld_leeren():
        """Suchfeld und Ergebnis zurücksetzen.

        ⚠ Ein Feld, das man nur mit der Rücktaste leerbekommt, ist bei einem
        Schiffsnamen wie „Anvil F7C-M Super Hornet Mk II" eine Zumutung — und
        das Ergebnis darunter bleibt sonst zu einem Schiff stehen, das gar
        nicht mehr im Feld steht.
        """
        schiff.set('')
        hinweis.configure(text='', fg=SUB)
        for kind in ergebnis.winfo_children():
            kind.destroy()

    _button_row(reihe, [
        _button(fenster, reihe, t('s_wr_nachsehen'), nachsehen, strong=True),
        _button(fenster, reihe, t('s_wr_leeren'), _feld_leeren),
        _button(fenster, reihe, t('s_wr_vergessen'), _vergessen),
    ])
    hinweis.pack(fill='x', padx=24, pady=(8, 0))
    ergebnis.pack(fill='x', padx=24, pady=(14, 20))


def _load_salvage(name):
    """Die Werksausstattung eines Schiffs holen — samt Ladenpreisen.

    Läuft **außerhalb** des Oberflächen-Fadens. Gibt `(teile, kennung)` zurück.
    """
    from . import salvage as bg, erkul, shops
    kat = erkul.ship_catalog()
    if not isinstance(kat, dict):
        return [], ''
    verzeichnis = {}
    for gruppe in (kat.get('groups') or []):
        pfad = gruppe.get('indexPath')
        if not pfad:
            continue
        index = erkul._fetch('%s/%s' % (erkul.BRANCH, pfad), 'bergung.index')
        for eintrag in ((index or {}).get('blobs') or []):
            if eintrag.get('id') and eintrag.get('path'):
                verzeichnis[eintrag['id']] = eintrag['path']

    # ⚠⚠ **Das Herstellerkürzel muss mit.** Erkul führt „Aegis Dynamics" als
    # `aegs` und „Anvil Aerospace" als `anvl` — Zusammenziehungen, keine
    # Wortanfänge. Ohne die Übersetzung findet „Aegis Gladius Valiant" sein
    # `aegs_gladius_valiant` nicht, und die Seite meldet „fliegt im Spiel noch
    # nicht" für ein Schiff, das jeder kennt. Genau so am 06.09.2026 gemeldet.
    #
    # Die Werft steht in den UEX-Schiffsdaten, die Kürzel-Tabelle liefert
    # erkul selbst mit (152 Hersteller).
    # ⚠ **Kein Herstellerkürzel aus der Tabelle.** Erkuls Herstellerliste führt
    # fünf verschiedene Kürzel unter dem Namen „Aegis Dynamics", und `aegs`,
    # das die Schiffe benutzen, ist nicht darunter. Die Zuordnung erkennt
    # Zusammenziehungen inzwischen selbst (`_ist_kuerzel`); die Werft kommt
    # trotzdem mit, weil sie bei manchen Namen das entscheidende Wort liefert.
    from . import ships as alle_schiffe
    eintrag = alle_schiffe._find(name) or {}
    werft = eintrag.get('werft') or ''

    treffer = erkul._search_wordwise(verzeichnis, name, werft, '', '')
    if not treffer:
        return [], ''
    teile = bg.factory_loadout(treffer, verzeichnis[treffer])
    # Preise nachladen, damit die Anzeige sie schon hat.
    for teil in teile:
        if not shops.known(teil['ref']):
            try:
                shops.fetch(teil['ref'], name=teil['name'])
            except Exception as ausnahme:
                fehler.merken('seiten.bergung.preis', ausnahme)
    return teile, treffer


def _mining(fenster, rahmen):
    """Wo welches Erz abzubauen ist — **beide** Richtungen in einer Suche.

    Ohne Eingabe stehen die Orte da (man ist meistens irgendwo). Tippt man
    einen Rohstoff, kommen dessen Fundorte; tippt man einen Ort, kommt, was es
    dort gibt. Das sind nicht zwei Ansichten, sondern eine Tabelle mit zwei
    Eingängen — beides sind echte Fragen, je nachdem ob man gerade fliegen mag
    oder nicht.
    """
    from . import mining as berg_modul
    # ⚠ **`s_bg_lead`, nicht `s_wr_lead`.** Hier stand der Text der
    # Bergungs-Seite — „Vor dir treibt ein Wrack…" über der Erzsuche. Der
    # eigene Satz war die ganze Zeit da und wurde von niemandem gerufen.
    _heading(fenster, rahmen, t('hf_bergbau'), t('s_bg_lead'))
    innen = _scroll_area(rahmen)

    try:
        orte = berg_modul.locations()
        erze = berg_modul.ores()
    except Exception as ausnahme:
        fehler.merken('seiten.bergbau', ausnahme)
        orte, erze = [], []

    # ⚠⚠ **Vor der Abbruchbedingung.** Die Methodenempfehlung braucht KEINE
    # Bergbaudaten — sie steht fest im Programm. Stünde sie weiter unten, wäre
    # sie ausgerechnet für den weg, der ohne Netz unterwegs ist: Der Abbruch
    # darunter beendet die Seite, sobald die Daten fehlen.
    _method_box(fenster, innen)

    if not orte:
        _body_text(innen, t('s_bg_keine_daten'), fenster.f_small, fill='x')
        return

    kopf = tk.Frame(innen, bg=BG)
    kopf.pack(fill='x', pady=(0, 10))
    tk.Label(kopf, text=t('s_bg_orte') % (len(orte), len(erze)), bg=BG, fg=SUB,
             font=fenster.f_small, anchor='w').pack(fill='x')
    # ⚠ Ein Satz, kein Absatz. Die Prozentzahl in den Zeilen ist ein **Anteil**
    # und keine Menge — ohne diesen Hinweis liest sie jeder als „so viel liegt
    # hier", und dann ist ein winziger Fleck plötzlich die beste Adresse.
    _body_text(kopf, t('s_bg_anteil_hilfe'), fenster.f_small, fill='x')

    from .main_window import round_entry
    # Der Sprung aus einem Rezept setzt hier den Rohstoff hinein.
    suche_var = tk.StringVar(value=getattr(fenster, 'mining_search', '') or '')
    fenster.mining_search = ''
    ziel_suche = _setting_row(fenster, innen, t('s_bg_suche'), '')
    feld = round_entry(ziel_suche, suche_var, fenster.f_small, '#0c1017',
                       LINE, ACCENT, FG)
    feld.holder.pack(fill='x', pady=(4, 12))
    _search_clear(fenster, ziel_suche, suche_var)

    # ⚠ Dieselben Auswahlfelder wie auf den anderen Seiten. Tippen bleibt
    # möglich — aber wer die 38 Rohstoffe oder 48 Orte nicht auswendig kann,
    # soll sie aufklappen können, statt zu raten. „egal wo, sollte das
    # Bedienkonzept nicht jedes Mal ändern." (29.08.2026)
    berg_wahl = {'erz': '', 'ort': '', 'geraet': ''}
    # Was zuletzt über Erz oder Ort ins Suchfeld geschrieben wurde. Ohne das
    # löschte ein Wechsel des Geräte-Feldes die getippte Suche mit.
    berg_letzte = {'wert': ''}

    def berg_gewechselt():
        # Erz und Ort schreiben in dasselbe Suchfeld — es gibt nur **einen**
        # Filter, nicht zwei, die sich gegenseitig widersprechen könnten.
        neu = berg_wahl['erz'] or berg_wahl['ort'] or ''
        if neu != berg_letzte['wert']:
            berg_letzte['wert'] = neu
            # ⭐ **Gewählt heißt aufgeklappt** (08.09.2026): „wenn man eine
            # Auswahl trifft, machs doch so, dass das betreffende direkt
            # aufgeklappt ist, und man nicht nochmal extra klicken muss."
            # Wer einen Namen aus dem Auswahlfeld nimmt, hat sich schon
            # entschieden — die Kopfzeile danach noch einmal anzuklicken ist
            # ein Klick, der nichts entscheidet.
            #
            # ⚠ Nur bei der Auswahl, nicht beim Tippen: Nach zwei Buchstaben
            # stehen dort noch zwölf Treffer, und einer davon spränge auf.
            if berg_wahl['erz']:
                offen['name'] = 'erz:' + berg_wahl['erz']
            elif berg_wahl['ort']:
                offen['name'] = 'ort:' + berg_wahl['ort']
            else:
                offen['name'] = None
            suche_var.set(neu)   # zeichnet über `trace_add` von selbst neu
            return
        # ⚠ **Das Gerät ist kein Suchbegriff, sondern ein zweiter Filter.**
        # Es steht neben der Suche, nicht darin — deshalb hier nur neu
        # zeichnen, statt das Feld zu überschreiben.
        zeichnen()

    # ⚠ `erze()` und `orte()` liefern **Objekte**, keine Namen — mit ihnen
    # direkt bestückt bliebe das Feld leer.
    _erznamen = sorted({(e_.get('name') or '') for e_ in erze} - {''},
                       key=str.lower)
    _ortnamen = sorted({(o_.get('name') or '') for o_ in orte} - {''},
                       key=str.lower)
    # ⭐ **„Womit farmst du?"** — der Filter, der die Prozentzahlen erst
    # ehrlich macht. Handabbau, Fahrzeug und Schiff werden getrennt gerechnet;
    # untereinander gemischt behauptet die Sortierung eine Vergleichbarkeit,
    # die es nicht gibt (59 % Aphorite mit dem Multi-Tool sagen nichts über
    # 33 % Silicon mit dem Prospector). Wer sein Gerät wählt, sieht nur noch
    # Zahlen, die zueinander passen.
    _geraete = [('schiff', t('s_bg_art_schiff')),
                ('fahrzeug', t('s_bg_art_fahrzeug')),
                ('fps', t('s_bg_art_fps'))]
    _filter_bar(fenster, innen,
                  [('erz', t('s_bg_alle_erze'), [(x, x) for x in _erznamen]),
                   ('ort', t('s_bg_alle_orte'), [(x, x) for x in _ortnamen]),
                   ('geraet', t('s_bg_alle_geraete'), _geraete)],
                  berg_gewechselt, berg_wahl)
    # Beim erneuten Aufrufen des Reiters wieder leer — die Seite wird nur
    # ein- und ausgeblendet, nicht neu gebaut.
    fenster.on_show['bergbau'] = lambda: suche_var.set('')

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
    ziel_sig = _setting_row(fenster, innen, t('s_bg_sig_feld'), '')
    sig_feld = round_entry(ziel_sig, sig_var, fenster.f_small, '#0c1017',
                           LINE, ACCENT, FG, placeholder=t('s_pl_signatur'))
    sig_feld.holder.pack(fill='x', pady=(4, 2))
    _body_text(innen, t('s_bg_sig_hilfe'), fenster.f_small, fill='x')
    sig_rahmen = tk.Frame(innen, bg=BG)
    sig_rahmen.pack(fill='x', pady=(2, 10))

    def sig_zeichnen(*_):
        for w in sig_rahmen.winfo_children():
            w.destroy()
        input_device = sig_var.get().strip()
        if not input_device:
            return
        try:
            treffer = berg_modul.find_signature(input_device)
        except Exception as ausnahme:
            fehler.merken('seiten.signatur', ausnahme)
            return
        if not treffer:
            _body_text(sig_rahmen, t('s_bg_sig_nichts'), fenster.f_small,
                        fill='x')
            return
        tk.Label(sig_rahmen, text=t('s_bg_sig_anzahl') % len(treffer), bg=BG,
                 fg=SUB, font=fenster.f_small, anchor='w').pack(fill='x')
        # ⚠ Höchstens zehn. Eine Bereichssuche kann dutzende Treffer haben,
        # und die Liste darunter soll nicht aus dem Bild geschoben werden.
        for name, anzahl, gesamt, ab in treffer[:10]:
            z = tk.Frame(sig_rahmen, bg=BG)
            z.pack(fill='x', pady=1)
            tk.Label(z, text=t('s_bg_sig_treffer') % (anzahl, name), bg=BG,
                     fg=ACCENT, font=fenster.f_base, anchor='w').pack(
                         side='left', padx=(4, 0))
            tk.Label(z, text='%d' % gesamt, bg=BG, fg=FG,
                     font=fenster.f_small, anchor='e').pack(
                         side='right', padx=(8, 4))
            # Die Abweichung nur, wenn es eine gibt — „+0,0 %" ist Rauschen.
            if abs(ab) >= 0.05:
                tk.Label(z, text='%+.1f %%' % ab, bg=BG, fg=SUB,
                         font=fenster.f_small, anchor='e').pack(
                             side='right', padx=(8, 0))
            else:
                tk.Label(z, text=t('s_bg_sig_genau'), bg=BG, fg=SUB,
                         font=fenster.f_small, anchor='e').pack(
                             side='right', padx=(8, 0))

    sig_var.trace_add('write', sig_zeichnen)
    _search_clear(fenster, ziel_sig, sig_var)

    liste_rahmen = tk.Frame(innen, bg=BG)
    liste_rahmen.pack(fill='both', expand=True)
    offen = {'name': None}

    def aufklappen(*_):
        """Neu zeichnen, **ohne** die Rollstelle zu verlieren.

        ⛔⛔ Ein Klick auf ein Erz baut die ganze Liste neu — und die Seite
        sprang dabei nach oben. Gemeldet am 14.09.2026: „klickt man ein Erz an,
        um die Infos zu sehen, rollt das Fenster nach oben, und man muss neu
        runterscrollen … der User denkt, da sei was defekt, und meldet mir
        Fehler."

        ⚠ **Nur beim Aufklappen, nicht beim Suchen.** Wer etwas Neues eintippt,
        will das erste Ergebnis sehen — dort ist der Sprung nach oben richtig.
        Deshalb zwei Wege auf dieselbe Zeichenfunktion statt eines.
        """
        _keep_scroll(innen, zeichnen)

    def zeichnen(*_):
        for w in liste_rahmen.winfo_children():
            w.destroy()
        text = suche_var.get().strip().lower()

        # ⚠ **Rohstoffe zuerst, auch ohne Suche.** Die Seite zeigte im
        # Grundzustand die 48 Orte — man kam also mit „wo bin ich?" herein,
        # gesucht wird aber mit „wo finde ich Titanium?". Am 29.08.2026:
        # „in der Liste sollten auch nicht die Orte, sondern erst das Mineral
        # stehen, da sucht man als Erstes nach."
        geraet = berg_wahl['geraet']
        for e in erze:
            if geraet and not _has_tool(e, geraet):
                continue
            if not text or text in e['name'].lower():
                _mining_ore(fenster, liste_rahmen, e, offen, aufklappen,
                          geraet)
        # Orte danach — sie beantworten die zweite Frage („was gibt es hier?").
        #
        # ⚠ **Ohne Eingabe stehen sie NICHT da** (07.09.2026). Vorher hingen
        # 48 Ortszeilen unter den Rohstoffen, durch die niemand liest: Wer
        # einen Ort sucht, tippt ihn oder klappt ihn im Auswahlfeld auf.
        # Gemeldet mit „Erze sollten wir da anzeigen, Orte reicht wenn der
        # User das per Textfeld suchen kann oder aus dem Dropdown".
        #
        # Die Auswahl schreibt in dasselbe Suchfeld — ein gewählter Ort füllt
        # `text` also und erscheint dadurch von selbst.
        if text:
            for o in orte:
                if geraet and not (o.get('je_geraet') or {}).get(geraet):
                    continue
                if (text in o['name'].lower()
                        or text in (o['system'] or '').lower()):
                    _mining_place(fenster, liste_rahmen, o, offen,
                              aufklappen, geraet)

        if not liste_rahmen.winfo_children():
            _body_text(liste_rahmen, t('s_he_nichts'), fenster.f_small,
                        fill='x')

    suche_var.trace_add('write', zeichnen)
    zeichnen()
    _body_text(innen, t('s_bg_mehr_info'), fenster.f_small, fill='x')


# Spaltenbreiten der Raffinerien-Tafel, in Zeichen. ⚠ Sie stehen hier oben,
# weil die **Systemleiste** sich über `len(gruppe) * COLUMN_VALUE` legt: Band,
# Überschrift und Werte müssen dieselbe Zahl benutzen, sonst verrutscht die
# Leiste gegenüber ihren Spalten. Zwei Stellen mit derselben Zahl sind eine
# Stelle zu viel.
COLUMN_MATERIAL = 18
COLUMN_VALUE = 6


def _refinery_head(kuerzel):
    """Die Spaltenüberschrift — zweizeilig, statt abgeschnitten.

    ⛔ „Checkmate" braucht bei normaler Schrift **63 px**, eine Spalte hat
    46. Tk kürzt das ohne Meldung auf „Checkm". Umbrechen kostet dagegen nur
    Höhe, und die ist hier reichlich da: Die Tafel ist breit, nicht hoch.

    ⚠ Nicht mit `wraplength` lösen — das rechnet in Pixeln und müsste je
    Schriftgröße nachgezogen werden. Hier wird nach **Zeichen** umgebrochen,
    genau wie die Spaltenbreite in Zeichen angegeben ist.

    ⚠ Und nicht `textwrap`: Das füllt die erste Zeile bis zum Anschlag und
    lässt den Rest hängen — aus „Checkmate" wurde „Checkm" / „ate". Zwei
    möglichst gleich lange Hälften lesen sich besser („Check" / „mate"), und
    wo ein Trennzeichen nahe der Mitte steht, wird dort getrennt
    („Pyro-" / „Gate").

    ⚠ **Ab 13 Zeichen bricht es unschön** („Stanton-Gate" → „Stanto" /
    „n-Gate"), weil keine Hälfte länger sein darf als die Spalte. In den
    heutigen Daten kommt das nicht vor — die längste Überschrift hat neun
    Zeichen —, und die Legende darunter schreibt ohnehin jede Station aus.
    Wird es einmal gebraucht, ist die Spaltenbreite die Stellschraube, nicht
    diese Funktion.
    """
    if len(kuerzel) <= COLUMN_VALUE:
        return kuerzel
    mitte = (len(kuerzel) + 1) // 2
    schnitt = mitte
    for versatz in range(0, COLUMN_VALUE):
        for stelle in (mitte + versatz, mitte - versatz):
            if 0 < stelle < len(kuerzel) and kuerzel[stelle - 1] in '- ':
                schnitt = stelle
                break
        else:
            continue
        break
    oben, unten = kuerzel[:schnitt].rstrip(), kuerzel[schnitt:]
    if max(len(oben), len(unten)) > COLUMN_VALUE:   # Notnagel: hart in der Mitte
        oben, unten = kuerzel[:mitte], kuerzel[mitte:]
    return oben + '\n' + unten


def _refinery_groups(spalten):
    """Die Spalten nach System zusammengefasst: `[(system, [spalten])]`.

    ⚠ Die Reihenfolge kommt aus `refinery_matrix()` und ist bereits nach
    System sortiert — hier wird nur zusammengefasst, **nicht neu sortiert**.
    Wer hier sortierte, könnte die Leiste gegen die Spalten verschieben.
    """
    gruppen = []
    for eintrag in spalten:
        system = eintrag[1]
        if gruppen and gruppen[-1][0] == system:
            gruppen[-1][1].append(eintrag)
        else:
            gruppen.append((system, [eintrag]))
    return gruppen


def _refinery_short(namen):
    """Aus „ARC-L1 Wide Forest Station" wird „ARC-L1".

    ⚠ Dieselbe Regel wie im Raffinerie-Kasten der Bergbau-Seite. Zwei
    Schreibweisen für dieselbe Station wären ein Widerspruch im eigenen
    Programm — und genau die sind heute dreimal teuer geworden.

    ⚠⚠ **Bei Gateways reicht das erste Wort nicht.** „Pyro Gateway (Nyx)"
    steht in **Nyx** und hieße gekürzt „Pyro" — in einer Spalte, über der
    „Nyx" steht. Das Wort davor benennt das Ziel des Sprungpunkts, nicht den
    Ort. Deshalb bleibt „Gateway" dran (gekürzt), sonst widerspricht die
    Überschrift der Ortsangabe daneben.
    """
    kuerzel = []
    for n in namen:
        if not n:
            continue
        erstes = n.split(' ')[0]
        kuerzel.append(erstes + '-Gate' if 'Gateway' in n else erstes)
    kuerzel = list(dict.fromkeys(kuerzel))
    return kuerzel[0] if kuerzel else '—'


def _refineries(fenster, rahmen):
    """Alle Raffinerien nebeneinander — Boni **und** Nachteile.

    ⭐ Gewünscht am 13.09.2026: „eine Seite, wo er sehen kann, welche Boni alle
    Raffinerien geben, in einer Tabelle, damit er entscheiden kann, welche die
    beste ist, wo er für unterschiedliche Materialien das beste Ergebnis
    bekommt" — und ausdrücklich **mit den Nachteilen**.

    ⚠⚠ **Die Nachteile sind der Kern, nicht die Zugabe.** 40 der 108 Werte
    sind negativ (−9 % bis +13 %). Eine Tabelle, die nur die Boni zeigt,
    empfiehlt eine Station, die beim nächsten Erz draufzahlt — und das ist
    schlechter als gar keine Empfehlung.

    ⚠ **Materialien als Zeilen, Raffinerien als Spalten.** So liest man eine
    Zeile für „welche Station für dieses Erz" und eine Spalte für „was taugt
    diese Station überhaupt". Andersherum stünden 24 Zahlenspalten
    nebeneinander, und niemand fände die eigene Zeile wieder.

    ⚠ Der Spaltenkopf trägt nur das Kürzel (`ARC-L1`) — ausgeschrieben sind es
    bis zu vier Stationen je Spalte. Die vollen Namen stehen unter der Tabelle.
    """
    from . import mining as berg_modul

    _heading(fenster, rahmen, t('hf_raffinerien'), t('s_rf_lead'))
    innen = _scroll_area(rahmen)

    try:
        spalten, zeilen = berg_modul.refinery_matrix()
    except Exception as ausnahme:
        fehler.merken('seiten.raffinerien', ausnahme)
        spalten, zeilen = [], []

    if not spalten or not zeilen:
        # ⚠ Kein leerer Bildschirm: Ohne Bergbaudaten ist die Seite nicht
        # kaputt, sie hat nur noch nichts. Das gehört dagestanden.
        _body_text(innen, t('s_rf_keine'), fenster.f_base, fill='x')
        return

    # ⛔⛔ **Jede Spalte kostet Platz, und Tk schneidet still ab.**
    # Gemessen am 14.09.2026 bei 1100×842 (verfügbar: 852 px): Mit 20 Zeichen
    # für das Material und 8 je Wert brauchte die Zeile 890 px bei normaler
    # Schrift und **1354 px** bei „sehr groß" — dort fehlten fünf Spalten
    # ersatzlos. Schon die ausgelieferte v3.34.0 verlor bei „sehr groß" drei.
    #
    # Ein Wert ist höchstens vier Zeichen breit (`+11`, `-9`). Die acht waren
    # nur für die Überschrift da — und die passt jetzt zweizeilig.
    karte = _card(innen, pady=(0, 12))
    # ⭐ Ab hier rollt die Tafel waagerecht in ihrer eigenen Fläche. Schmalere
    # Spalten und zweizeilige Überschriften (siehe unten) holen genug heraus,
    # dass bei normaler und großer Schrift gar nicht gerollt werden muss —
    # der Balken erscheint nur bei „sehr groß".
    karte = _wide_area(karte)

    # ⭐ Eine Leiste mit dem System über den Spalten. Sie beantwortet die
    # Frage, die jemand wirklich hat („wohin fliege ich?"), ohne dass man in
    # die Legende springen muss — und sie kostet nichts an Breite, weil sie
    # sich über die Spalten ihres Systems legt.
    band = tk.Frame(karte, bg=SURFACE)
    band.pack(fill='x', padx=12, pady=(10, 0))
    tk.Label(band, text='', bg=SURFACE, font=fenster.f_small,
             width=COLUMN_MATERIAL).pack(side='left')
    for system, gruppe in _refinery_groups(spalten):
        tk.Label(band, text=system or '—', bg=SURFACE, fg=ACCENT,
                 font=fenster.f_small, anchor='w',
                 width=len(gruppe) * COLUMN_VALUE).pack(side='left')

    kopf = tk.Frame(karte, bg=SURFACE)
    kopf.pack(fill='x', padx=12, pady=(0, 4))
    tk.Label(kopf, text=t('s_rf_material'), bg=SURFACE, fg=SUB,
             font=fenster.f_small, anchor='w',
             width=COLUMN_MATERIAL).pack(side='left')
    for namen, _system in spalten:
        tk.Label(kopf, text=_refinery_head(_refinery_short(namen)), bg=SURFACE, fg=SUB,
                 font=fenster.f_small, width=COLUMN_VALUE,
                 anchor='se', justify='right').pack(side='left', fill='y')

    for material, werte, bester in zeilen:
        z = tk.Frame(karte, bg=SURFACE)
        z.pack(fill='x', padx=12, pady=1)
        tk.Label(z, text=material, bg=SURFACE, fg=FG, font=fenster.f_small,
                 anchor='w', width=COLUMN_MATERIAL).pack(side='left')
        for i, wert in enumerate(werte):
            # ⚠ Drei Zustände, drei Farben: Gewinn, Verlust, weder noch.
            # Eine 0 grau zu lassen ist wichtig — sie ist keine Empfehlung.
            if wert > 0:
                farbe = ACCENT if i == bester else FG
            elif wert < 0:
                farbe = RED_PALE
            else:
                farbe = SUB
            tk.Label(z, text=('%+d' % wert) if wert else '·',
                     bg=SURFACE, fg=farbe, font=fenster.f_small,
                     width=COLUMN_VALUE, anchor='e').pack(side='left')

    # ⛔⛔ **Nach System gegliedert, nicht als Liste mit Ortsspalte.**
    # Die erste Fassung schrieb je Zeile „Kürzel · System · Stationen". Sobald
    # eine Spalte zu mehreren Orten gehörte, stand dort „Nyx, Pyro, Stanton" —
    # und damit war die Zeile unlesbar. Jetzt trägt jede Spalte genau ein
    # System, und das System steht als **Überschrift** darüber. Die Ortsspalte
    # entfällt ersatzlos: Sie wiederholte nur, was schon oben steht.
    _body_text(innen, t('s_rf_legende'), fenster.f_small, fill='x')
    _letztes = None
    for namen, system in spalten:
        if system != _letztes:
            tk.Label(innen, text=system or '—', bg=BG, fg=ACCENT,
                     font=fenster.f_base, anchor='w').pack(
                         fill='x', pady=(8, 2))
            _letztes = system
        z = tk.Frame(innen, bg=BG)
        z.pack(fill='x', pady=1)
        # ⚠ Die Zahl dahinter ist wichtig: „Checkmate" allein sieht aus wie
        # **eine** Station, tatsächlich stehen fünf in dieser Spalte — und die
        # Überschrift nennt die alphabetisch erste, nicht die einzige.
        _kurz = _refinery_short(namen)
        if len(namen) > 1:
            _kurz = t('s_bg_raff_weitere') % (_kurz, len(namen) - 1)
        tk.Label(z, text=_kurz, bg=BG, fg=FG, font=fenster.f_small,
                 anchor='w', width=16).pack(side='left', padx=(12, 0))
        tk.Label(z, text=', '.join(namen), bg=BG, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(side='left',
                                                        fill='x', expand=True)

    _body_text(innen, t('s_rf_quelle'), fenster.f_small, fill='x',
               pady=(12, 0))


def _mining_header(fenster, eltern, links, rechts, farbe, aufklappen):
    zeile = tk.Frame(eltern, bg=BG, cursor='hand2')
    zeile.pack(fill='x', pady=1)
    tk.Label(zeile, text=links, bg=BG, fg=farbe, font=fenster.f_base,
             anchor='w').pack(side='left', padx=(4, 0))
    if rechts:
        tk.Label(zeile, text=rechts, bg=BG, fg=SUB, font=fenster.f_small,
                 anchor='e').pack(side='right', padx=(8, 4))
    for w in (zeile,) + tuple(zeile.winfo_children()):
        w.bind('<Button-1>', aufklappen)
    # ⭐ Diese Zeile hat kein Symbol — sie hebt sich ueber den Hintergrund
    # ab, sonst wirkt die ganze Bergbau-Seite tot.
    icons.hover_row(zeile, BG, SURFACE)
    return zeile


def _mining_ore(fenster, eltern, erz, offen, neu_zeichnen, geraet=''):
    """Ein Rohstoff — aufgeklappt stehen seine Fundorte darunter.

    `geraet` ist die Wahl aus „Womit?" (`''` = alle). Sie entscheidet nicht
    nur, welche Fundorte gezeigt werden, sondern auch **welche Prozentzahl**:
    Ein Erz, das man sowohl mit der Hand als auch vom Fahrzeug bekommt, hat
    je Gerät einen eigenen Anteil.
    """
    schluessel = 'erz:' + erz['name']

    def umschalten(*_):
        offen['name'] = None if offen['name'] == schluessel else schluessel
        neu_zeichnen()

    # ⚠ Hier stand `t('s_bg_orte') % (a, b).split('·')[0]` — das `.split()` lief
    # auf dem **Tupel**, nicht auf dem Text. Ergebnis: Ausnahme in `zeichnen()`,
    # und die ganze Liste blieb leer. Der Selbsttest sah es nicht, weil er die
    # Seite ohne Suchbegriff baut und dieser Zweig nie lief. Gefunden auf einem
    # Bildschirmfoto (29.08.2026). Jetzt ein eigener Textschlüssel.
    # ⚠ Bei gewähltem Gerät zählt die Kopfzeile nur die Orte, die dann auch
    # darunter stehen — sonst verspricht sie 18 Orte und zeigt drei.
    from .mining import _pot
    fundorte = [e for e in erz['orte']
                if not geraet
                or any(_pot(a) == geraet for a in (e[2] if len(e) > 2 else ()))]
    _mining_header(fenster, eltern, erz['name'],
                    t('s_bg_nur_orte') % len(fundorte),
                    ACCENT, umschalten)
    if offen['name'] != schluessel:
        return
    block = tk.Frame(eltern, bg='#0c1017')
    block.pack(fill='x', padx=(24, 0), pady=(2, 8))
    # Mit Gerätewahl gilt dessen eigener Anteil — und damit auch dessen
    # Reihenfolge. Ohne Wahl bleibt es bei der aus `mining.ores()`.
    if geraet:
        fundorte.sort(key=lambda e: (-((e[5] or {}).get(geraet, (0.0,))[0]
                                       if len(e) > 5 else 0.0),
                                     e[0].lower()))
    for eintrag in fundorte:
        ort, system, arten = eintrag[0], eintrag[1], eintrag[2]
        # ⚠ Nachgiebig: Ablagen und Selbsttest-Listen von vor v3.27 haben nur
        # drei Felder je Fundort.
        anteil = eintrag[3] if len(eintrag) > 3 else 0.0
        stufe = eintrag[4] if len(eintrag) > 4 else 1
        allein = False
        fein = (eintrag[5] if len(eintrag) > 5 else None) or {}
        if geraet and geraet in fein:
            anteil, stufe, wieviele = fein[geraet]
            allein = wieviele <= 1
        elif len(fein) == 1:
            # Ohne Gerätewahl: Steht das Erz an diesem Ort für sein Gerät
            # allein da, gilt derselbe Hinweis.
            allein = list(fein.values())[0][2] <= 1
        z = tk.Frame(block, bg='#0c1017')
        z.pack(fill='x', padx=12, pady=1)
        tk.Label(z, text=ort, bg='#0c1017', fg=FG, font=fenster.f_base,
                 anchor='w').pack(side='left')
        tk.Label(z, text=system, bg='#0c1017', fg=SUB, font=fenster.f_small,
                 anchor='w').pack(side='left', padx=(10, 0))
        tk.Label(z, text=_kind_text(arten), bg='#0c1017', fg=SUB,
                 font=fenster.f_small, anchor='e').pack(side='right', padx=12)
        _mining_share(fenster, z, anteil, stufe, '#0c1017', allein)

    # ⭐ **Wohin damit?** Die Frage nach dem Fundort ist nur die halbe. Zwanzig
    # Raffinerien teilen sich zehn Profile, und der Unterschied ist kein
    # Rundungsfehler: Bei Bexalite liegen 18 Prozentpunkte zwischen der besten
    # und der schlechtesten Wahl, bei Quartz 16. Wer das nicht weiß, verschenkt
    # jeden Flug ein Stück Ausbeute.
    #
    # ⚠ Die Daten stehen in denselben Bergbaudaten (`refineries` +
    # `refineryProfiles`) und kosten keinen zusätzlichen Abruf. Gegengerechnet
    # gegen die Tabelle auf scmdb.net: alle zehn ARC-L1-Werte identisch.
    from . import mining as berg_modul
    try:
        raff = berg_modul.refineries_for(erz['name'])
    except Exception as ausnahme:
        fehler.merken('seiten.raffinerie', ausnahme)
        raff = []
    if raff:
        tk.Label(block, text=t('s_bg_raff_kopf'), bg='#0c1017', fg=FG,
                 font=fenster.f_base, anchor='w').pack(
                     fill='x', padx=12, pady=(10, 2))
        spanne = raff[0][2] - raff[-1][2]
        if not spanne:
            _body_text(block, t('s_bg_raff_egal'), fenster.f_small, fill='x')
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
                         font=fenster.f_base, anchor='w').pack(side='left')
                tk.Label(z, text=system or '', bg='#0c1017', fg=SUB,
                         font=fenster.f_small, anchor='w').pack(
                             side='left', padx=(10, 0))
                tk.Label(z, text=t('s_bg_raff_zeile') % bonus, bg='#0c1017',
                         fg=(ACCENT if bonus > 0 else GOLD if bonus < 0 else SUB),
                         font=fenster.f_base, anchor='e').pack(
                             side='right', padx=12)
            _body_text(block, t('s_bg_raff_spanne') % spanne,
                        fenster.f_small, fill='x')
        # ⭐ **Von hier zur ganzen Tabelle.** Der Kasten beantwortet „welche
        # Raffinerie für DIESES Erz"; wer mehrere Erze im Laderaum hat, will
        # die Gegenrichtung. Ausdrücklich so gewünscht am 13.09.2026:
        # „mit Verlinkung von den Raffinerien von der Bergbau-Seite".
        #
        # ⚠ Über `jump_to`, nicht `open_page` — nur so steht der Rückweg über
        # der Zielseite (seit v3.32.0).
        _button(fenster, block, t('s_bg_raff_alle'),
                lambda: fenster.jump_to('raffinerien')).pack(
                    anchor='w', padx=12, pady=(6, 10))


def _method_box(fenster, eltern):
    """„Welche Verarbeitungsmethode?" — die dritte Frage der Kette.

    Wo baue ich ab → wohin bringe ich es → **wie lasse ich es verarbeiten.**
    Die ersten beiden beantwortet die Liste darunter je Erz; diese hier ist
    erz-unabhängig und steht deshalb darüber, nicht neunmal in jeder
    Aufklappung.

    ⚠ Die Auswahl steht in derselben `_filterleiste` wie überall sonst — ein
    Bedienkonzept fürs ganze Programm, kein Sonderweg für eine Seite.
    """
    from . import refinery as raff
    block = tk.Frame(eltern, bg=BG)
    block.pack(fill='x', pady=(0, 12))
    tk.Label(block, text=t('s_rm_kopf'), bg=BG, fg=FG, font=fenster.f_base,
             anchor='w').pack(fill='x')
    _body_text(block, t('s_rm_lead'), fenster.f_small, fill='x')

    achsentext = {'ertrag': t('s_rm_ertrag'), 'kosten': t('s_rm_kosten'),
                  'tempo': t('s_rm_tempo')}
    auswahl = [(a, achsentext[a]) for a in raff.AXES]
    wahl = {'erste': '', 'zweite': ''}
    # Der Klappzustand des Vergleichs — überlebt das Neuzeichnen, siehe unten.
    klapp = {'offen': False}
    ergebnis = tk.Frame(block, bg=BG)

    def stufentext(kennung):
        ertrag = {1: 's_rm_s_gering', 2: 's_rm_s_moderat',
                  3: 's_rm_s_hoch'}[raff.level(kennung, 'ertrag')]
        tempo = {0: 's_rm_t_sehr', 1: 's_rm_t_langsam', 2: 's_rm_t_mittel',
                 3: 's_rm_t_schnell'}[raff.level(kennung, 'tempo')]
        # ⚠ Umdrehen: Im Modul ist 3 der **Kostenvorteil**, auf dem Bildschirm
        # steht „geringe Kosten". Ohne diese Zeile stünde dort das Gegenteil.
        kosten = {3: 's_rm_s_gering', 2: 's_rm_s_moderat',
                  1: 's_rm_s_hoch'}[raff.level(kennung, 'kosten')]
        return t('s_rm_zeile') % (t(ertrag), t(tempo), t(kosten))

    def zeichnen():
        for w in ergebnis.winfo_children():
            w.destroy()
        beste, alle = raff.recommend(wahl['erste'] or None,
                                      wahl['zweite'] or None)
        tk.Label(ergebnis, text=t('s_rm_nimm') % raff.NAMES[beste], bg=BG,
                 fg=ACCENT, font=fenster.f_base, anchor='w').pack(
                     fill='x', pady=(6, 0))
        _body_text(ergebnis, stufentext(beste), fenster.f_small, fill='x')
        # Der Satz gehört genau dann dazu, wenn die Empfehlung mit Zeit
        # bezahlt wird — sonst wäre er ein Allgemeinplatz.
        if raff.level(beste, 'tempo') <= 1:
            _body_text(ergebnis, t('s_rm_zeit_laeuft'), fenster.f_small,
                        fill='x')

        # ⭐ **Der Vergleich klappt zu und startet zugeklappt** (08.09.2026):
        # „die Info braucht man nur, wenn man sie sehen will." Neun Methoden
        # mit Bewertung sind zwölf Zeilen über der eigentlichen Liste — die
        # Empfehlung darüber beantwortet die Frage schon, der Rest ist zum
        # Nachschlagen. Gleiches Muster wie „Was bringt am meisten?" auf der
        # Fortschritts-Seite: Pfeil, Titel, Zahl.
        #
        # ⚠ Der Klappzustand liegt **außerhalb** von `zeichnen()` — die
        # Funktion räumt `ergebnis` bei jeder Auswahländerung leer und baut
        # neu; ein Zustand darin wäre bei jedem Wechsel wieder zu.
        kopf = tk.Frame(ergebnis, bg=BG, cursor='hand2')
        kopf.pack(fill='x', pady=(10, 2))
        pfeil = icons.line(kopf, 'zuklappen' if klapp['offen']
                              else 'aufklappen', background=BG,
                              font=fenster.f_small)
        pfeil.pack(side='left')
        tk.Label(kopf, text=t('s_rm_alle'), bg=BG, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(side='left')
        tk.Label(kopf, text='  %d' % len(alle), bg=BG, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(side='left')

        koerper = tk.Frame(ergebnis, bg=BG)
        if klapp['offen']:
            koerper.pack(fill='x', after=kopf)

        def umschalten(*_):
            klapp['offen'] = not klapp['offen']
            pfeil.swap_symbol('zuklappen' if klapp['offen']
                                  else 'aufklappen')
            if klapp['offen']:
                # ⚠ `after=kopf` — sonst landet der Block ganz unten.
                koerper.pack(fill='x', after=kopf)
            else:
                koerper.pack_forget()

        for teil in [kopf] + list(kopf.winfo_children()):
            teil.bind('<Button-1>', umschalten)
            try:
                teil.configure(cursor='hand2')
            except tk.TclError:
                pass

        for kennung in alle:
            z = tk.Frame(koerper, bg=BG)
            z.pack(fill='x', pady=1)
            tk.Label(z, text=raff.NAMES[kennung], bg=BG,
                     fg=(ACCENT if kennung == beste else FG),
                     font=fenster.f_base, anchor='w').pack(side='left',
                                                            padx=(4, 0))
            tk.Label(z, text=stufentext(kennung), bg=BG, fg=SUB,
                     font=fenster.f_small, anchor='e').pack(side='right',
                                                            padx=(8, 4))

        # Methoden, die nichts können, was eine andere nicht besser kann.
        for schlecht, besser in sorted(raff.dominated().items()):
            _body_text(koerper,
                        t('s_rm_unterlegen') % (raff.NAMES[schlecht],
                                                raff.NAMES[besser]),
                        fenster.f_small, fill='x')
        _body_text(koerper, t('s_rm_stand') % (raff.PATCH, raff.READ_ON),
                    fenster.f_small, fill='x')

    _filter_bar(fenster, block,
                  [('erste', t('s_rm_erste'), auswahl),
                   ('zweite', t('s_rm_zweite'), auswahl)],
                  zeichnen, wahl)
    ergebnis.pack(fill='x')
    zeichnen()


def _mining_place(fenster, eltern, ort, offen, neu_zeichnen, geraet=''):
    """Ein Ort — aufgeklappt steht darunter, was es dort gibt.

    ⚠⚠ **Nach Gerät gruppiert, nicht in einer Liste.** Vorher standen auf
    Daymar „59 % Aphorite (FPS)", „48 % Beradom (Fahrzeug)" und „33 % Silicon
    (Schiff)" untereinander, absteigend sortiert — und behaupteten damit, das
    eine sei ergiebiger als das andere. Das ist falsch: Die drei Zahlen sind
    je Gerät auf 100 % gerechnet, ein Vergleich über die Blöcke hinweg ergibt
    keinen Sinn. Wer oben ein Gerät wählt, bekommt nur dessen Block.
    """
    schluessel = 'ort:' + ort['name']

    def umschalten(*_):
        offen['name'] = None if offen['name'] == schluessel else schluessel
        neu_zeichnen()

    _mining_header(fenster, eltern, ort['name'],
                    '%s · %s' % (ort['system'], ort['typ']), FG, umschalten)
    if offen['name'] != schluessel:
        return
    block = tk.Frame(eltern, bg='#0c1017')
    block.pack(fill='x', padx=(24, 0), pady=(2, 8))

    je_geraet = ort.get('je_geraet') or {}
    # Immer dieselbe Reihenfolge der Blöcke — Schiff zuerst, weil die meisten
    # Orte damit angeflogen werden.
    bloecke = [g for g in ('schiff', 'fahrzeug', 'fps')
               if je_geraet.get(g) and (not geraet or g == geraet)]
    if not bloecke:
        return
    for kennung in bloecke:
        werte = je_geraet[kennung]
        # Die Überschrift nur, wenn wirklich mehrere Blöcke dastehen — bei
        # gewähltem Gerät sagt sie nichts, was oben nicht schon steht.
        if len(bloecke) > 1:
            tk.Label(block, text=t('s_bg_art_' + kennung), bg='#0c1017',
                     fg=SUB, font=fenster.f_small, anchor='w').pack(
                         fill='x', padx=12, pady=(8, 2))
        # ⚠ **Nach Konzentration, nicht alphabetisch.** „Was gibt es hier?"
        # heisst in Wahrheit „was lohnt sich hier?" — eine Liste von A bis Z
        # beantwortet das nicht. Bei gleichem Anteil entscheidet der Name,
        # damit die Reihenfolge zwischen zwei Aufrufen dieselbe bleibt.
        namen = sorted(werte, key=lambda n: (-werte[n][0], n.lower()))
        for name in namen:
            anteil, stufe = werte[name]
            z = tk.Frame(block, bg='#0c1017')
            z.pack(fill='x', padx=12, pady=1)
            tk.Label(z, text=name, bg='#0c1017', fg=FG, font=fenster.f_base,
                     anchor='w').pack(side='left')
            # ⚠ **Keine Art-Spalte hier.** Die Überschrift des Blocks sagt
            # bereits „Fahrzeug"; daneben in jeder Zeile noch einmal
            # „Fahrzeug" ist Rauschen — und bei einem Erz, das zu zwei Geräten
            # gehört (Carinite), stünde in beiden Blöcken dasselbe Paar und
            # damit zweimal etwas Falsches.
            _mining_share(fenster, z, anteil, stufe, '#0c1017',
                         len(werte) <= 1)


# ------------------------------------------------------------------- Lager
#
# Vorschlag von **Horthy (KRT)** (29.08.2026): Rohstoffe selbst
# eintragen, beim Herstellen abziehen lassen.
#
# ⚠ **Von Hand, weil es nicht anders geht.** Die `Game.log` sagt nichts über
# Rohstoffe — in 17 MB Protokollen kommt weder `resource` noch `cargo` vor.
# Deshalb steht der Hinweis oben auf der Seite: Diese Liste gehört dem Spieler,
# nicht dem Spiel.


def _checkbox(parent, text, on, toggle, small_font):
    """Ein anklickbares Kästchen mit Haken — für „ja/nein" neben einem Feld.

    ⚠ Warum kein `tk.Checkbutton`: Der ist ein Systemelement und sieht auf
    jedem Betriebssystem anders aus. Das Programm hat eine Formensprache; ein
    graues Aqua-Kästchen mitten in einer sonst dunklen Zeile fällt auf wie ein
    Fremdkörper. Gezeichnet wird nichts von Hand — der Haken ist das
    Symbol `abhaken` aus dem Satz.
    """
    rahmen = tk.Frame(parent, bg=BG, cursor='hand2')
    # ⚠ Nur Symbole aus dem festgelegten Satz — `haken` steht in
    # `icons.LINE_NAMES`. Ein frei erfundener Name (`abhaken` gibt es nur
    # als Knopf-Symbol) faellt still auf den Ersatztext zurueck, und die Zeile
    # sieht dann anders aus als der Rest des Programms.
    # ⚠⚠ **Nur die festgelegten Farben.** Die Symbole liegen als fertige Bilder
    # je Farbe im Satz (`grau`, `gruen`, `hell`, `gelb`, `blau`, `rot`) — ein
    # eigener Farbwert findet kein Bild, und das Symbol fehlt dann **still**.
    # Genau so passiert: Mit `#2a3446` stand neben „cSCU" gar kein Haken.
    def _bauen(an_jetzt):
        for kind in rahmen.winfo_children():
            kind.destroy()
        symbol = icons.line(rahmen, 'haken', background=BG,
                               color=icons.GREEN if an_jetzt
                               else icons.GREY)
        symbol.pack(side='left')
        lbl = tk.Label(rahmen, text=text, bg=BG,
                       fg=ACCENT if an_jetzt else SUB, font=small_font)
        lbl.pack(side='left', padx=(4, 0))
        for teil in (symbol, lbl):
            teil.bind('<Button-1>', klick)

    def klick(_=None):
        on[0] = not on[0]
        _bauen(on[0])
        toggle(on[0])

    _bauen(on[0])
    rahmen.bind('<Button-1>', klick)
    return rahmen


def _refinery_box(fenster, eltern, lager, ort_var, neu_zeichnen, meldung):
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
    from . import crafting as herst_lager
    from . import places as _orte_modul
    from .main_window import round_frame

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
    pfeil = icons.line(kopf, 'aufklappen', background=BG,
                          font=fenster.f_small)
    pfeil.pack(side='left', padx=(0, 8))
    tk.Label(kopf, text=t('s_rf_titel'), bg=BG, fg=FG, font=fenster.f_bold,
             anchor='w', cursor='hand2').pack(side='left')

    ziel = tk.Frame(kasten, bg=BG)
    # Die Erklaerung gehoert in den Koerper, nicht in die Kopfzeile: Sonst
    # steht zugeklappt ein Absatz da, der etwas erklaert, das man nicht sieht.
    _rf_hilfe = tk.Label(ziel, text=_strip_markup(t('s_rf_hilfe')), bg=BG,
                         fg=SUB, font=fenster.f_small, anchor='w',
                         justify='left')
    _rf_hilfe.pack(fill='x', pady=(2, 0))
    _wrap(_rf_hilfe, reference=kasten, inset=10)
    einheit = tk.StringVar(value='cscu')
    zeile = tk.Frame(ziel, bg=BG)
    zeile.pack(fill='x', pady=(6, 4))
    tk.Label(zeile, text=t('s_rf_einheit'), bg=BG, fg=SUB,
             font=fenster.f_small).pack(side='left', padx=(0, 8))
    from .main_window import round_select
    # ⚠ Reihenfolge: (eltern, eintraege, gewaehlt, beim_waehlen, schrift).
    round_select(zeile, [('cscu', 'cSCU'), ('scu', 'SCU')], 'cscu',
             lambda k: (einheit.set(k), pruefen()),
             fenster.f_small).pack(side='left')

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
             font=fenster.f_bold, anchor='w').pack(fill='x')
    _ozeile, _oliste, _ozeichnen = _combo_box(fenster, ortblock, ort_raff,
                                                _orte_modul.all_places)
    _ozeile.pack(fill='x', pady=(4, 0))
    _oliste.pack(fill='x')

    kasten = round_frame(ziel, '#0c1017', LINE, radius=8, base_color=BG)
    kasten.holder.pack(fill='x', pady=(4, 6))
    feld = tk.Text(kasten, bg='#0c1017', fg=FG, font=('Consolas', 10),
                   height=7, wrap='none', relief='flat', bd=0,
                   insertbackground=FG, highlightthickness=0)
    feld.pack(fill='both', expand=True, padx=12, pady=10)

    vorschau = tk.Label(ziel, text=t('s_rf_nichts'), bg=BG, fg=SUB,
                        font=fenster.f_small, anchor='w', justify='left')
    vorschau.pack(fill='x')
    knopf_platz = tk.Frame(ziel, bg=BG)
    knopf_platz.pack(anchor='w', pady=(6, 0))
    stand = {'posten': []}

    def pruefen(*_):
        """Beim Tippen mitrechnen — man sieht sofort, was hineinginge."""
        posten, fehlerhaft = lager.refinery_lines(
            feld.get('1.0', 'end-1c'), einheit.get())
        stand['posten'] = posten
        teile = []
        for name, menge, guete in posten[:8]:
            teile.append('%s %s · Q %d' % (name, _amount_text(menge), guete))
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
            _button(fenster, knopf_platz, t('s_rf_knopf') % len(posten),
                   uebernehmen, strong=True).pack(side='left')

    def uebernehmen():
        # ⚠ Geschlossene Liste wie überall: Was UEX nicht kennt, kommt nicht
        # ins Lager. Leer ist erlaubt — der Lagerort ist freiwillig.
        if not _orte_modul.knows((ort_raff.get() or '').strip()):
            meldung.configure(text=t('s_rf_ort_unbekannt'), fg=RED)
            return
        # ⚠⚠ **Der Ort läuft NICHT durch `storage_name()`.** Die Funktion zieht
        # eine Eingabe auf einen bekannten **Rohstoff** — sie vergleicht gegen
        # `storable()`. Ein Ortsname steht dort nie drin, also kam immer
        # `None` zurück, und `or ''` machte daraus einen **leeren Lagerort**:
        # Wer „Levski" gewählt hatte, bekam seine ganze Ausbeute ohne Ort
        # eingebucht. Am 30.08.2026 gemeldet.
        #
        # Der Ort wird gegen die Ortsliste geprüft, so wie im Formular oben.
        ziel_ort = (ort_raff.get() or '').strip()
        for name, menge, guete in stand['posten']:
            lager.add(name, menge, guete, ziel_ort)
        anzahl = len(stand['posten'])
        feld.delete('1.0', 'end')
        pruefen()
        neu_zeichnen()
        meldung.configure(text=t('s_rf_fertig') % anzahl, fg=ACCENT)

    feld.bind('<KeyRelease>', pruefen)

    def _umschalten(_=None):
        if ziel.winfo_ismapped():
            ziel.pack_forget()
            pfeil.swap_symbol('aufklappen')
            pfade.einstellung_setzen('lager_raffinerie_offen', False)
        else:
            # ⚠ `after=kopf` — sonst haengt der Koerper beim zweiten Aufklappen
            # unter allem, was inzwischen dazugekommen ist, statt unter seiner
            # eigenen Kopfzeile.
            ziel.pack(fill='x', after=kopf)
            pfeil.swap_symbol('zuklappen')
            pfade.einstellung_setzen('lager_raffinerie_offen', True)

    # Die ganze Kopfzeile ist die Schaltflaeche, nicht nur das Symbol: Ein
    # Pfeil von zwoelf Pixeln ist kein Ziel, das man treffen will.
    for teil in (kopf, pfeil) + tuple(kopf.winfo_children()):
        teil.bind('<Button-1>', _umschalten)
    # ⭐ Der Pfeil hebt sich ab, wenn die Maus die Kopfzeile
    # trifft — anklickbar ist hier die Zeile, nicht der Pfeil.
    icons.hover_group(kopf, pfeil)

    # ⚠⚠ **`einstellung_wahrheit`, nicht `einstellung`.** Letztere liefert
    # einen PFAD und ruft dafür `.strip()` auf dem Wert auf. Hier steht aber
    # ein Ja/Nein: Sobald der Block einmal aufgeklappt war, lag `True` in der
    # Datei — und `True.strip()` warf einen AttributeError. Der traf nicht nur
    # diesen Block, sondern riss den Aufbau der GANZEN Lager-Seite ab: Die
    # Liste der eingetragenen Posten fehlte danach völlig, obwohl die Daten
    # unversehrt waren. Gemeldet am 03.09.2026, drin seit v3.4.1.
    #
    # ⚠ Und es blieb kaputt, bis das Programm neu startete — eine Seite wird
    # nur EINMAL gebaut (siehe `open_page()`). Zuklappen half also nicht.
    if pfade.einstellung_wahrheit('lager_raffinerie_offen', False):
        _umschalten()
    return feld


def _amount_text(quantity):
    """Eine Menge kurz und ohne Nullen am Ende — 1,88 statt 1,8800."""
    return ('%g' % round(quantity, 4)).replace('.', ',')


def _hangar(fenster, rahmen):
    """Meine Schiffe: importieren, von Hand eintragen, ansehen, austragen.

    ⚠ Die Reihenfolge auf der Seite ist Absicht: **erst der Import**, dann der
    Handeintrag. Wer vierzig Pledges hat, soll nicht vierzig Mal tippen — und
    wer keinen Export hat, findet den Handeintrag direkt darunter. Umgekehrt
    wäre der bequeme Weg der versteckte.
    """
    from . import fleet as meine, erkul, ships as alle_schiffe, file_picker

    _heading(fenster, rahmen, t('hf_hangar'), t('s_hg_lead'))
    innen = _scroll_area(rahmen)
    _body_text(innen, t('s_hg_hinweis'), fenster.f_small, fill='x')

    daten = {'stand': meine.load()}
    meldung = {'text': '', 'farbe': SUB}
    liste_rahmen = tk.Frame(innen, bg=BG)
    schiff = tk.StringVar()

    def neu_zeichnen():
        _liste_fuellen()

    # ------------------------------------------------------------- Import
    tk.Label(innen, text=t('s_hg_import_titel'), bg=BG, fg=FG,
             font=fenster.f_bold, anchor='w').pack(fill='x', padx=24,
                                                   pady=(18, 2))
    _body_text(innen, t('s_hg_import_text'), fenster.f_small, fill='x',
                padx=24, inset=48)
    # ⚠ Der Hinweis auf JSON ist kein Geschmack: Bei einem echten Export vom
    # 06.09.2026 fehlten der CSV drei Schiffe, die in der JSON standen.
    _body_text(innen, t('s_hg_import_json'), fenster.f_small, color=GOLD,
                fill='x', padx=24, inset=48)

    def importieren():
        pfad = file_picker.open_file(
            t('s_hg_import_knopf'),
            # JSON zuerst — das ist der empfohlene Weg. CSV bleibt wählbar.
            patterns=(('JSON', '*.json'), ('CSV', '*.csv')))
        if not pfad:
            return
        eintraege, fehlertext = meine.read(pfad)
        if fehlertext:
            meldung['text'], meldung['farbe'] = t('s_hg_import_fehler'), RED
        elif not eintraege:
            meldung['text'], meldung['farbe'] = t('s_hg_import_leer'), RED
        else:
            neu, alt = meine.import_entries(eintraege, daten['stand'])
            meldung['text'] = t('s_hg_import_ok').format(neu=neu, alt=alt)
            meldung['farbe'] = ACCENT
            _steckplaetze_holen(still=True)
        neu_zeichnen()

    # ⚠⚠ **Jede Knopfreihe braucht ihren EIGENEN Rahmen** — und die Knöpfe
    # müssen darin sitzen, nicht in der Rollfläche. `_knopfreihe` packt mit
    # `side='left'` in ihr `eltern` und merkt sich dort ihren Umbruch-Zustand
    # (`zuletzt_nebeneinander`). Bekommt sie zweimal dieselbe Fläche, tritt die
    # zweite Reihe der ersten den Zustand weg — beim ersten Anlauf hier waren
    # daraufhin **alle vier Knöpfe unsichtbar**, ohne Fehlermeldung.
    reihe_import = tk.Frame(innen, bg=BG)
    reihe_import.pack(fill='x', padx=24, pady=(10, 0))
    _button_row(reihe_import, [
        _button(fenster, reihe_import, t('s_hg_import_knopf'), importieren,
               strong=True),
        _button(fenster, reihe_import, t('s_hg_erweiterung'),
               lambda: pfade.im_browser(XPLORER_PAGE)),
    ])

    # -------------------------------------------------------- Von Hand
    tk.Label(innen, text=t('s_hg_hand_titel'), bg=BG, fg=FG,
             font=fenster.f_bold, anchor='w').pack(fill='x', padx=24,
                                                   pady=(18, 2))
    _body_text(innen, t('s_hg_hand_text'), fenster.f_small, fill='x',
                padx=24, inset=48)
    # ⚠ Sagt, wie das Feld benutzt wird. Ohne diesen Satz haelt man die
    # sichtbare Liste fuer das ganze Angebot — genau das war die Rueckmeldung
    # vom 06.09.2026.
    _body_text(innen, t('s_hg_such_hilfe'), fenster.f_small, fill='x',
                padx=24, inset=48)

    block = tk.Frame(innen, bg=BG)
    block.pack(fill='x', padx=24, pady=(8, 0))
    # ⚠⚠ **Geschlossene Liste, kein Freitext** — dieselbe Regel wie beim
    # Handelslager und beim Lagerort. Angenommen wird nur, was die Schiffsliste
    # kennt; sonst steht am Ende ein ausgedachter oder beleidigender Name im
    # Werkzeug, und ein Bildschirmfoto davon macht die Runde.
    # ⚠⚠ **`namen_alle`, nicht `alle`** — das dort liefert nur Schiffe **mit
    # Laderaum** (134 von 280). Damit liessen sich Jaeger, Renner und Exo-
    # Anzuege gar nicht eintragen: Arrow, Gladius, A.T.L.S. IKTI. Gemeldet am
    # 06.09.2026.
    zeile, auswahl, _ = _combo_box(fenster, block, schiff,
                                     alle_schiffe.all_names,
                                     empty_text=t('s_hg_nichts_gefunden'),
                                     scrollable=200)
    zeile.pack(fill='x')
    auswahl.pack(fill='x')

    def von_hand():
        name = (schiff.get() or '').strip()
        # ⚠⚠ **Geschlossene Liste** — dieselbe Regel wie beim Lagerort und
        # beim Handelslager: Angenommen wird nur, was UEX kennt. Sonst steht am
        # Ende ein ausgedachter oder beleidigender Name im Werkzeug, und ein
        # Bildschirmfoto davon macht die Runde.
        if not alle_schiffe.knows(name):
            meldung['text'], meldung['farbe'] = t('s_hg_kein_name'), RED
            neu_zeichnen()
            return
        # ⚠⚠ **Die Herkunft ist wählbar, nicht geraten.** Bis zum 06.09.2026
        # bekam jedes von Hand eingetragene Schiff „im Spiel gekauft" —
        # verpflichtend, ohne Wahl. Gefragt wurde: „wie tragen User per
        # Echtgeld gekaufte Schiffe ein, die nicht den Hangar XPLORer nutzen
        # wollen?" Die ehrliche Antwort war: gar nicht.
        #
        # Der Unterschied ist keine Nebensache: An der Herkunft hängt, ob ein
        # Schiff dauerhaft versichert ist (LTI kommt nur mit Echtgeld-Käufen)
        # — und genau das entscheidet beim Claimen, ob die eingebauten Teile
        # überleben.
        if meine.add(daten['stand'], name,
                             origin=(meine.PLEDGE if echtgeld[0]
                                     else meine.INGAME),
                             lti=echtgeld[0] and lti[0]):
            meine.save(daten['stand'])
            meldung['text'] = t('s_hg_getragen').format(name=name)
            meldung['farbe'] = ACCENT
            schiff.set('')
            _steckplaetze_holen(still=True)
        else:
            meldung['text'], meldung['farbe'] = t('s_hg_schon_da'), RED
        neu_zeichnen()

    def _steckplaetze_holen(still=True):
        """Die Steckplätze der Hangar-Schiffe holen, soweit sie fehlen.

        ⚠⚠ **Dafür gab es einmal einen Knopf — er ist raus.** „Steckplätze
        holen" stand neben „Eintragen", und die Rückmeldung dazu lautete: *„was
        macht Steckplätze holen denn? ich verstehe den Knopf nicht."* Zu Recht:
        Er holte nach, was nach Import und Handeintrag ohnehin schon geholt
        wird. Ein Knopf, der eine Arbeit anbietet, die das Programm längst
        erledigt hat, erklärt sich nie — er wirft nur die Frage auf, was er
        soll.

        Geholt wird jetzt an drei Stellen von selbst: nach dem Import, nach
        einem Handeintrag, und **beim Öffnen der Seite**, wenn etwas fehlt.
        """
        geholt = meine.fetch_missing(daten['stand'])
        if not still and geholt:
            meldung['text'] = t('s_hg_geholt').format(n=geholt)
            meldung['farbe'] = ACCENT
        if geholt:
            neu_zeichnen()
        return geholt

    # ⚠ Die Wahl steht **über** dem Knopf, nicht daneben: Wer eintragen will,
    # liest von oben nach unten und soll die Frage vorher sehen, nicht danach.
    echtgeld = [False]
    lti = [False]
    wahl_reihe = tk.Frame(innen, bg=BG)
    wahl_reihe.pack(fill='x', padx=24, pady=(8, 0))

    def _lti_zeigen():
        # LTI gibt es nur bei Echtgeld-Käufen — die Frage ergibt sonst keinen
        # Sinn und würde nur verwirren.
        if echtgeld[0]:
            lti_kasten.pack(side='left', padx=(20, 0))
        else:
            lti[0] = False
            lti_kasten.pack_forget()

    def _echtgeld_um(an):
        echtgeld[0] = an
        _lti_zeigen()

    _checkbox(wahl_reihe, t('s_hg_mit_echtgeld'), echtgeld, _echtgeld_um,
               fenster.f_small).pack(side='left')
    lti_kasten = _checkbox(wahl_reihe, t('s_hg_hat_lti'), lti,
                            lambda an: lti.__setitem__(0, an), fenster.f_small)

    reihe_hand = tk.Frame(innen, bg=BG)
    reihe_hand.pack(fill='x', padx=24, pady=(10, 0))
    _button_row(reihe_hand, [
        _button(fenster, reihe_hand, t('s_hg_eintragen'), von_hand),
    ])

    hinweis = tk.Label(innen, text='', bg=BG, fg=SUB, font=fenster.f_small,
                       anchor='w')
    hinweis.pack(fill='x', padx=24, pady=(10, 0))

    # ----------------------------------------------------------- Die Liste
    liste_rahmen.pack(fill='x', padx=24, pady=(16, 20))

    def _liste_fuellen():
        for kind in liste_rahmen.winfo_children():
            kind.destroy()
        hinweis.configure(text=meldung['text'], fg=meldung['farbe'])

        stand = daten['stand']
        schiffsliste = (stand.get('schiffe') or [])
        tk.Label(liste_rahmen, text=t('s_hg_meine').format(n=len(schiffsliste)),
                 bg=BG, fg=FG, font=fenster.f_bold,
                 anchor='w').pack(fill='x', pady=(0, 6))

        if not schiffsliste:
            _body_text(liste_rahmen, t('s_hg_leer'), fenster.f_small, fill='x')
            return

        version = erkul.game_version()
        _body_text(liste_rahmen,
                    t('s_hg_quelle').format(version=version) if version
                    else t('s_hg_keine_daten'),
                    fenster.f_small, fill='x', pady=(0, 8))

        ohne = 0
        for eintrag in sorted(schiffsliste,
                              key=lambda s: (s.get('name') or '').lower()):
            ohne += _hangar_row(fenster, liste_rahmen, eintrag, daten,
                                  meldung, neu_zeichnen)
        if ohne:
            _body_text(liste_rahmen,
                        t('s_hg_ohne_erklaert').format(n=ohne),
                        fenster.f_small, fill='x', pady=(10, 0))

    _liste_fuellen()

    # ⭐ **Fehlendes wird beim Öffnen nachgeholt, ohne dass jemand etwas
    # drücken muss.** Der Regelfall ist, dass nichts fehlt — dann kostet es
    # einen Abruf von 2,7 KB (den Katalog), und der sagt sofort, dass sich
    # nichts geändert hat.
    #
    # ⚠ In einem eigenen Faden: Bei dreißig fehlenden Schiffen wären das
    # dreißig Abrufe, und das Fenster stünde so lange. Tk verträgt keine
    # Zugriffe aus fremden Fäden — deshalb kommt die Anzeige über `after(0, …)`
    # zurück in den Oberflächen-Faden.
    def _nachziehen_im_hintergrund():
        try:
            if not meine.unknown(daten['stand']):
                return
        except Exception:
            return
        def arbeit():
            try:
                geholt = meine.fetch_missing(daten['stand'])
            except Exception as ausnahme:
                fehler.merken('seiten.hangar.nachziehen', ausnahme)
                return
            if not geholt:
                return
            def zeigen():
                try:
                    if liste_rahmen.winfo_exists():
                        neu_zeichnen()
                except tk.TclError:
                    pass
            try:
                liste_rahmen.after(0, zeigen)
            except tk.TclError:
                pass
        threading.Thread(target=arbeit, daemon=True).start()

    _nachziehen_im_hintergrund()


def _wishlist(fenster, rahmen):
    """Schiffe, die man sich vornimmt — mit Preis, Ort und planbarer Ausstattung.

    ⭐ Vorschlag von Zwaersch (KRT) am 06.09.2026: „Also Unterpunkt könnte man
    noch ne Wishlist-Option anbieten. Für, ich nenn's mal allgemein Vehikel,
    die man sich erspielen/kaufen möchte."

    ⚠⚠ **Eigener Reiter, nicht mehr unten am Hangar.** Am selben Tag gemeldet:
    „wird sonst unübersichtlich und niemand findet es auf Anhieb." Das stimmt —
    die Wunschliste stand hinter einer Liste, die bei vierzig Schiffen über
    mehrere Bildschirmhöhen ging, und jedes davon klappt seine Ausstattung auf.
    Was hinter etwas Wachsendem steht, ist irgendwann nicht mehr da.

    ⚠ **Getrennt vom Hangar geführt.** Ein Wunsch ist kein Besitz — was hier
    steht, darf nie in „passt in dein Schiff" auftauchen. Sonst beantwortet das
    Werkzeug eine Frage über ein Schiff, das niemand hat. Gespeichert wird
    trotzdem in derselben Datei: Es ist dieselbe Sammlung, nur ein anderes Fach.
    """
    from . import fleet as meine, ships as alle_schiffe

    _heading(fenster, rahmen, t('hf_wunschliste'), t('s_wl_lead'))
    innen = _scroll_area(rahmen)
    _body_text(innen, t('s_hg_wunsch_text'), fenster.f_small, fill='x')

    daten = {'stand': meine.load()}
    meldung = {'text': '', 'farbe': SUB}
    wunsch = tk.StringVar()

    block = tk.Frame(innen, bg=BG)
    block.pack(fill='x', padx=24, pady=(14, 0))
    w_zeile, w_auswahl, _ = _combo_box(fenster, block, wunsch,
                                         alle_schiffe.all_names,
                                         empty_text=t('s_hg_nichts_gefunden'),
                                         scrollable=200)
    w_zeile.pack(fill='x')
    w_auswahl.pack(fill='x')

    hinweis = tk.Label(innen, text='', bg=BG, fg=SUB, font=fenster.f_small,
                       anchor='w', justify='left')
    liste_rahmen = tk.Frame(innen, bg=BG)

    def neu_zeichnen():
        _fuellen()

    def eintragen():
        name = (wunsch.get() or '').strip()
        if not alle_schiffe.knows(name):
            meldung['text'], meldung['farbe'] = t('s_hg_kein_name'), RED
            neu_zeichnen()
            return
        # ⚠ Was man hat, wünscht man sich nicht — und das wird gesagt, nicht
        # stillschweigend verschluckt.
        if meine.contains(daten['stand'], name):
            meldung['text'], meldung['farbe'] = t('s_hg_wunsch_schon_da'), RED
            neu_zeichnen()
            return
        # ⚠ **Den Hersteller gleich mitspeichern.** Ohne ihn findet `erkul`
        # einen Teil der Schiffe nicht (bei der Prospector gemessen) — und dann
        # ließe sich ein Wunschschiff nicht ausstatten. Auf der Wunschliste
        # gibt es keinen Pledge-Export, aus dem er käme; UEX führt ihn im
        # Namen mit, also wird er dort geholt.
        if meine.wishlist_add(daten['stand'], name,
                                    alle_schiffe.manufacturer(name)):
            meine.save(daten['stand'])
            meldung['text'] = t('s_hg_wunsch_notiert').format(name=name)
            meldung['farbe'] = ACCENT
            wunsch.set('')
            # ⚠⚠ **Mit `erzwingen`, sonst passiert gar nichts.** Der Hangar
            # hat beim Programmstart schon einmal nachgezogen und die Sperre
            # gesetzt; ohne dieses Kennzeichen käme das frisch eingetragene
            # Schiff nie an seine Daten, und auf der Karte stünde dauerhaft
            # „keine Steckplatz-Daten". Genau das war am 06.09.2026 der Fall.
            #
            # `danach` zeichnet neu, sobald wirklich etwas geholt wurde — sonst
            # müsste man den Reiter wechseln, damit die Ausstattung erscheint.
            _fetch_slots(innen, erzwingen=True,
                                     danach=neu_zeichnen)
        else:
            meldung['text'], meldung['farbe'] = t('s_hg_wunsch_doppelt'), RED
        neu_zeichnen()

    reihe = tk.Frame(innen, bg=BG)
    reihe.pack(fill='x', padx=24, pady=(10, 0))
    _button_row(reihe, [
        _button(fenster, reihe, t('s_hg_wunsch_eintragen'), eintragen),
    ])
    hinweis.pack(fill='x', padx=24, pady=(10, 0))
    liste_rahmen.pack(fill='x', padx=24, pady=(16, 20))

    def _fuellen():
        for kind in liste_rahmen.winfo_children():
            kind.destroy()
        hinweis.configure(text=meldung['text'], fg=meldung['farbe'])
        liste = meine.wishlist(daten['stand'])
        tk.Label(liste_rahmen, text=t('s_hg_wunsch_meine').format(n=len(liste)),
                 bg=BG, fg=FG, font=fenster.f_bold,
                 anchor='w').pack(fill='x', pady=(0, 6))
        if not liste:
            _body_text(liste_rahmen, t('s_wl_leer'), fenster.f_small,
                        fill='x')
            return
        for eintrag in liste:
            _wish_row(fenster, liste_rahmen, eintrag, daten, meldung,
                          neu_zeichnen)

    # ⚠ Beim erneuten Öffnen frisch laden: Die Seite wird nur **einmal** gebaut
    # (siehe `open_page()`), und wer inzwischen im Hangar ein Wunschschiff
    # gekauft hat, fände hier sonst den alten Stand.
    def _beim_zeigen():
        daten['stand'] = meine.load()
        meldung['text'] = ''
        wunsch.set('')
        _fuellen()

    fenster.on_show['wunschliste'] = _beim_zeigen
    _fetch_slots(innen)
    _fuellen()


def _asop(fenster, rahmen):
    """Eigene Namen für die Schiffe im Fleet Manager (ASOP).

    Im Abrufterminal stehen die Werksnamen. Wer drei Abwandlungen derselben
    Reihe hat, sucht dort jedes Mal — und zwar in dem Moment, in dem er sich
    entscheiden muss.

    ⚠⚠ **Ein Name gehört zum Muster, nicht zum einzelnen Schiff.** Zwei
    *gleiche* Hornets bekommen denselben Namen; das Spiel kennt an dieser
    Stelle keinen Unterschied. Das steht auch auf der Seite, nicht nur hier —
    wer es erst im Spiel merkt, hält das Werkzeug für kaputt.

    ⚠ Die Liste kommt aus dem eigenen Hangar, nicht aus allen 655 Fahrzeugen
    des Spiels. Niemand benennt ein Schiff um, das er nicht hat, und eine
    Liste, die man erst durchsuchen muss, ist ein Bausatz.
    """
    from . import asop as asop_modul, fleet as meine, injektion

    # ⚠⚠ **Reihenfolge ist hier alles.** Erst alles Feste packen (Kopf oben,
    # Fuß unten), **danach** die rollende Fläche mit `expand=True`. Wer die
    # Liste zuerst packt, schiebt den Fuß aus dem Fenster — bei 41 Schiffen
    # ist der Knopf dann unerreichbar. Genau so gemeldet zu v3.28.0:
    # „der Button verschwindet, wenn man runterscrollt, in der ewig langen
    # Liste."
    _heading(fenster, rahmen, t('hf_asop'), t('s_as_lead'))

    daten = {'stand': asop_modul.laden()}
    alles = {'zuordnung': []}

    # --- fester Kopf: Suche und Stand -------------------------------------
    kopf = tk.Frame(rahmen, bg=BG)
    kopf.pack(side='top', fill='x')

    # ⚠ Gesucht wird über **beides** — den Werksnamen und den eigenen. Wer
    # „Hornet" tippt, meint das Schiff; wer „Leitschiff" tippt, meint den
    # Namen, den er selbst vergeben hat. Nur nach einem von beiden zu suchen
    # wäre auf der Hälfte der Fälle nutzlos.
    suche = tk.StringVar()
    such_zeile = tk.Frame(kopf, bg=BG)
    such_zeile.pack(fill='x', padx=24, pady=(0, 6))
    from .main_window import round_entry as _rundes_feld_such
    such_feld = _rundes_feld_such(such_zeile, suche, fenster.f_small,
                                  '#0c1017', LINE, ACCENT, FG)
    such_feld.holder.pack(side='left', fill='x', expand=True)
    _search_clear(fenster, such_zeile, suche)

    such_platz = tk.Label(such_feld, text=t('s_as_suche'), bg='#0c1017',
                          fg=SUB, font=fenster.f_small, anchor='w')

    def such_platz_zeigen(*_):
        try:
            if suche.get():
                such_platz.place_forget()
            else:
                such_platz.place(x=1, rely=0.5, anchor='w')
        except tk.TclError:
            pass

    such_platz.bind('<Button-1>', lambda _e: such_feld.focus_set())
    such_platz_zeigen()

    meldung = tk.Label(kopf, text='', bg=BG, fg=SUB, font=fenster.f_small,
                       anchor='w', justify='left')
    # ⚠ Eigene Zeile für „steht im Spiel". Sie sagt, ob die Namen wirklich
    # angekommen sind — die Zeile darüber zählt nur, wie viele sich benennen
    # lassen. Zwei verschiedene Auskünfte gehören nicht in dasselbe Feld.
    stand = tk.Label(kopf, text='', bg=BG, fg=SUB, font=fenster.f_small,
                     anchor='w', justify='left')
    stand.pack(fill='x', padx=24, pady=(2, 0))

    def zeilen_der_ini():
        """Die Zeilen der Sprachdatei, die das Spiel gerade lädt — oder nichts."""
        try:
            pfad, _sprachordner, _quelle = injektion.ini_datei()
            if pfad and os.path.isfile(pfad):
                with open(pfad, encoding='utf-8', errors='ignore') as f:
                    return f.read().splitlines()
        except Exception as ausnahme:
            fehler.merken('seiten._asop.ini', ausnahme)
        return []

    def sichern():
        """Merken — und gleich dafür sorgen, dass es auch im Spiel ankommt."""
        if not asop_modul.speichern(daten['stand']):
            stand.configure(text=t('s_as_nicht_gespeichert'), fg=RED)
            return False
        spaeter_einspielen()
        return True

    def _fuellen():
        """Die Daten holen — Sprachdatei und Hangar. Das Zeichnen macht `_zeichnen`."""
        alles['zuordnung'] = []
        zeilen = zeilen_der_ini()
        tabelle = asop_modul.schluessel_lesen(zeilen) if zeilen else {}
        if not tabelle:
            # ⚠ Ehrlich statt leer: Ohne Sprachdatei gibt es nichts zu
            # benennen, und das ist kein Fehler des Nutzers.
            meldung.configure(text=t('s_as_keine_ini'), fg=SUB)
            meldung.pack(fill='x', padx=24, pady=(8, 0))
            _zeichnen()
            return
        schiffe = (meine.load().get('schiffe') or [])
        if not schiffe:
            meldung.configure(text=t('s_as_kein_hangar'), fg=SUB)
            meldung.pack(fill='x', padx=24, pady=(8, 0))
            _zeichnen()
            return
        alles['zuordnung'] = asop_modul.zuordnen(schiffe, tabelle)
        ohne = [e for e in alles['zuordnung'] if not e['schluessel']]
        meldung.configure(
            text=t('s_as_stand') % (len(alles['zuordnung']) - len(ohne),
                                    len(alles['zuordnung'])),
            fg=SUB)
        meldung.pack(fill='x', padx=24, pady=(8, 0))
        _zeichnen()

    def _passt(e, text):
        """Trifft der Suchbegriff dieses Schiff?

        ⚠ Gesucht wird über **drei** Schreibweisen: den Namen aus dem Hangar,
        den Werksnamen aus dem Spiel und den selbst vergebenen. Wer „Hornet"
        tippt, meint das Schiff; wer „Leitschiff" tippt, meint seinen eigenen
        Namen. Nur eines davon zu durchsuchen wäre in der Hälfte der Fälle
        nutzlos.
        """
        if not text:
            return True
        eigen = ''
        if e['schluessel']:
            eigen = asop_modul.eintrag(daten['stand'], e['schluessel'])[0]
        return any(text in (x or '').lower()
                   for x in (e['name'], e['werksname'], eigen))

    def _zeichnen():
        for kind in liste.winfo_children():
            kind.destroy()
        zuordnung = alles['zuordnung']
        if not zuordnung:
            liste.pack_forget()
            return
        liste.pack(fill='x', padx=24, pady=(10, 0))
        text = (suche.get() or '').strip().lower()
        zeigen = [e for e in zuordnung if _passt(e, text)]
        if not zeigen:
            # ⚠ Nicht einfach leer bleiben — eine leere Liste sieht aus wie ein
            # kaputtes Werkzeug, nicht wie „nichts gefunden".
            tk.Label(liste, text=t('s_as_nichts_gefunden'), bg=BG, fg=SUB,
                     font=fenster.f_small, anchor='w').pack(fill='x', pady=(6, 0))
            return
        for e in zeigen:
            _asop_row(fenster, liste, e, daten, asop_modul, sichern)
        ohne = [e for e in zeigen if not e['schluessel']]
        if ohne:
            hinweis_lbl = tk.Label(
                liste, text=t('s_as_ohne') % ', '.join(x['name'] for x in ohne),
                bg=BG, fg=SUB, font=fenster.f_small, anchor='w', justify='left')
            hinweis_lbl.pack(fill='x', pady=(10, 0))
            _wrap(hinweis_lbl, inset=48)

    def _sagen(text, farbe):
        """Den Stand anzeigen — und stillhalten, wenn die Seite schon weg ist.

        ⚠⚠ Der verzögerte Lauf kann auf ein Fenster treffen, das gerade
        geschlossen wird. Beschriften scheitert dann, und **das darf das
        Schreiben nicht verhindern**: Sonst steht der Name in `asop.json`,
        aber nie in der `global.ini` — und im Spiel bleibt der Werksname
        stehen, ohne jeden Hinweis. Genau dieses Bild wurde schon zweimal
        gemeldet, damals aus einem anderen Grund.
        """
        try:
            stand.configure(text=text, fg=farbe)
        except Exception:
            pass

    def einspielen():
        """Die Namen in die Sprachdatei schreiben — über den üblichen Weg."""
        warte['id'] = None
        _sagen(t('s_as_laeuft'), SUB)
        try:
            stand.update_idletasks()
        except Exception:
            pass
        try:
            pfad, sprachordner, _quelle = injektion.ini_datei()
            if not pfad:
                _sagen(t('s_as_keine_ini'), RED)
                return
            ok, _anzahl, text = injektion.aktualisieren(pfad, sprachordner)
        except Exception as ausnahme:
            fehler.merken('seiten._asop.einspielen', ausnahme)
            ok, text = False, str(ausnahme)
        _sagen(t('s_as_steht') if ok else (t('s_as_schief') % text),
               ACCENT if ok else RED)

    # ⚠⚠ **Der Knopf war der Fehler von v3.28.0.** Wer einen Namen eintippt,
    # hat ihn vergeben — und erwartet ihn im Spiel. Stattdessen musste er unter
    # einer Liste von 41 Schiffen einen Knopf finden, der weit außerhalb des
    # Bildes lag. Gemeldet mit Bildschirmfoto: Name eingetragen, Haken gesetzt,
    # im Flottenmanager stand weiter der Werksname. Die Datei war nie
    # geschrieben worden.
    #
    # ⚠ Deshalb schreibt die Seite jetzt **von selbst** — gemessen 0,28 s für
    # die ganze 12-MB-Datei, also nichts, wofür man jemanden klicken lässt.
    # Gesammelt wird über `after`: Wer fünf Schiffe hintereinander benennt,
    # löst einen Lauf aus, nicht fünf.
    #
    # ⚠ Die Wartezeit hängt am **Fenster**, nicht an einem Element der Seite.
    # Ein `after` auf einem Bauteil, das inzwischen zerstört ist, läuft ins
    # Leere; das Fenster lebt dagegen so lange wie das Programm.
    warte = {'id': None}

    def spaeter_einspielen():
        try:
            if warte['id']:
                fenster.root.after_cancel(warte['id'])
        except Exception:
            pass
        _sagen(t('s_as_gemerkt'), SUB)
        warte['id'] = fenster.root.after(900, einspielen)

    def _offenes_nachholen():
        """Beim Zumachen: einen noch wartenden Schreiblauf jetzt ausführen.

        Wer einen Namen eintippt und sofort das Fenster schließt, ist
        schneller als die Drossel — ohne diese Stelle bliebe sein Name
        ungeschrieben liegen.
        """
        if not warte['id']:
            return
        try:
            fenster.root.after_cancel(warte['id'])
        except Exception:
            pass
        warte['id'] = None
        einspielen()

    try:
        fenster.before_close.append(_offenes_nachholen)
    except AttributeError:
        pass          # Prüfstände bauen die Seite auch ohne ganzes Fenster

    # --- fester Fuß: der Knopf, der immer erreichbar bleiben muss ----------
    # ⚠ `side='bottom'`, und **vor** der Rollfläche gepackt. Der Knopf bleibt
    # trotz Selbstschreiben: Nach einem Spiel-Patch hat sich an den Namen
    # nichts geändert, die Sprachdatei ist aber neu — dann gibt es nichts, was
    # ein Selbstschreiben auslösen könnte.
    fuss = tk.Frame(rahmen, bg=BG)
    fuss.pack(side='bottom', fill='x', padx=24, pady=(10, 12))
    _button(fenster, fuss, t('s_as_einspielen'), einspielen).pack(side='left')

    # --- und zuletzt die rollende Liste ------------------------------------
    innen = _scroll_area(rahmen)
    _body_text(innen, t('s_as_grenze'), fenster.f_small, fill='x', inset=48)
    _body_text(innen, t('s_as_patch_hinweis'), fenster.f_small, fill='x',
                inset=48, pady=(6, 0))
    liste = tk.Frame(innen, bg=BG)

    suche.trace_add('write', lambda *_: (such_platz_zeigen(), _zeichnen()))
    fenster.on_show['asop'] = _fuellen
    _fuellen()


def _asop_row(fenster, eltern, e, daten, asop_modul, sichern):
    """Eine Schiffszeile: Werksname, Eingabefeld, Stern.

    ⚠ Die Reihenfolge ist überall dieselbe — Beschriftung links, Bedienelement
    rechts. Ein Schalter, der auf einer Seite mittig steht und auf der nächsten
    rechts, sieht nach Zufall aus.
    """
    kasten = tk.Frame(eltern, bg=SURFACE)
    kasten.pack(fill='x', pady=(0, 6))

    kopf = tk.Frame(kasten, bg=SURFACE)
    kopf.pack(fill='x', padx=12, pady=(8, 2))
    tk.Label(kopf, text=e['name'], bg=SURFACE, fg=FG, font=fenster.f_bold,
             anchor='w').pack(side='left')
    if e['werksname']:
        tk.Label(kopf, text=e['werksname'], bg=SURFACE, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(side='left', padx=(10, 0))

    if not e['schluessel']:
        tk.Label(kasten, text=t('s_as_zeile_ohne'), bg=SURFACE, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(fill='x', padx=12,
                                                        pady=(0, 8))
        return

    eigen, stern = asop_modul.eintrag(daten['stand'], e['schluessel'])
    wert = tk.StringVar(value=eigen)
    stern_an = tk.BooleanVar(value=stern)

    reihe = tk.Frame(kasten, bg=SURFACE)
    reihe.pack(fill='x', padx=12, pady=(0, 10))

    from .main_window import round_entry
    feld = round_entry(reihe, wert, fenster.f_small, '#0c1017', LINE, ACCENT, FG)
    feld.holder.pack(side='left', fill='x', expand=True)

    def uebernehmen(*_):
        asop_modul.setzen(daten['stand'], e['schluessel'], wert.get(),
                          stern_an.get())
        sichern()

    # ⚠⚠ **Kein `tk.Checkbutton`.** In v3.28.0 stand hier eines — gemeldet:
    # „zu klein, sieht niemand, und sieht anders aus als der Rest im Projekt".
    # Beides stimmt: Tk malt sein Kästchen im Systemstil, also hell, winzig und
    # in einer anderen Handschrift als jede andere Seite. Das Projekt hat einen
    # eigenen Schalter (`_switch`) — der hängt an einer Einstellung und passt
    # hier nicht, sein **Aussehen** aber schon: eine Schaltfläche, die im
    # Akzentgrün steht, wenn sie an ist.
    #
    # ⚠ Und er sitzt **rechts** neben dem Feld, wie jedes Bedienelement seiner
    # Art im Programm. Symmetrie: Gleiches steht überall an der gleichen Stelle.
    schalter = tk.Label(reihe, text='', bg=SURFACE, font=fenster.f_small,
                        cursor='hand2', padx=12, pady=4)
    schalter.pack(side='right', padx=(10, 0))

    def schalter_zeichnen():
        an = stern_an.get()
        schalter.configure(text=' %s ' % t('s_as_stern'),
                           fg=BG if an else SUB,
                           bg=ACCENT if an else SURFACE)

    def stern_umschalten(_=None):
        stern_an.set(not stern_an.get())
        schalter_zeichnen()
        uebernehmen()

    schalter.bind('<Button-1>', stern_umschalten)
    # ⚠ Der Griff für Prüfungen — wie `neu_zeichnen` bei `_schalter`. Ein
    # erzeugter `<Button-1>` braucht ein **gemapptes** Fenster; im Bau-Lauf
    # unter Windows gibt es keins, und die Prüfung wäre dort rot, obwohl der
    # Schalter tut, was er soll. Genau daran ist Prüfung 155 schon einmal
    # gescheitert: gemessen wurde die Pixellage statt der Wirkung.
    schalter.umschalten = stern_umschalten
    schalter_zeichnen()

    # ⚠ Beim Verlassen des Feldes sichern, nicht bei jedem Tastendruck: Sonst
    # schreibt das Werkzeug bei „Mamba-Leitschiff" siebzehn Dateien.
    feld.bind('<FocusOut>', uebernehmen)
    feld.bind('<Return>', uebernehmen)


def _shopping_list(fenster, rahmen):
    """Alles, was noch zu besorgen ist — über alle Schiffe, wie eine Rechnung.

    ⭐⭐ **Der Reiter, der die ganze Ausstattungs-Arbeit zusammenführt.** Bis
    v3.19.0 hing der Warenkorb unter jeder einzelnen Schiffszeile: Wer wissen
    wollte, was sein nächster Ausflug zum Händler insgesamt kostet, musste
    vierzig Karten aufklappen und im Kopf addieren. Am 06.09.2026 gefragt:
    „der Warenkorb braucht auch einen extra Reiter, Einkaufsliste, da können
    Komponenten oder Schiffe drin sein, wenn Schiffe mit Fitting, dann muss man
    am Ende auch den Gesamtpreis sehen und eine Einzelaufstellung, so wie jede
    Rechnung die man bekommen würde."

    ⚠ **Nach Schiff gegliedert, nicht nach Teileart.** Eine Rechnung ordnet
    nach Position, nicht nach Warengruppe — man will sehen, was *dieses* Schiff
    kostet, und nicht alle Kühler des Hangars in einer Reihe.

    ⚠ **Das Schiff selbst steht nur drauf, wenn man es noch nicht hat.** Was im
    Hangar steht, ist bezahlt; dort zählen nur die fehlenden Teile. Ein
    Wunschschiff kostet erst sich selbst und dann seine Ausstattung — genau die
    Frage, die vor dem Kauf im Kopf steht.
    """
    from . import cart, fleet as meine

    _heading(fenster, rahmen, t('hf_einkaufsliste'), t('s_ek_lead'))
    innen = _scroll_area(rahmen)
    daten = {'stand': None}

    koerper = tk.Frame(innen, bg=BG)
    koerper.pack(fill='x', padx=24, pady=(10, 20))

    def neu_zeichnen():
        for kind in koerper.winfo_children():
            kind.destroy()
        _aufbauen()

    def _aufbauen():
        stand = meine.load()
        werte = cart.invoice(stand)
        daten['stand'] = stand
        posten = werte.get('posten') or []

        # ⚠⚠ **Drei Lagen, drei Sätze** — dieselbe Falle wie überall in diesem
        # Bereich: „nichts eingetragen", „nichts zu besorgen" und „keine Daten"
        # sehen im Code gleich aus und bedeuten Verschiedenes. Wer sie
        # zusammenwirft, sagt jemandem mit leerem Hangar, er sei fertig.
        if not (stand.get('schiffe') or stand.get('wunsch')):
            _body_text(koerper, t('s_ek_kein_schiff'), fenster.f_small,
                        fill='x')
            return
        if not posten:
            _body_text(koerper, t('s_ek_nichts_offen'), fenster.f_small,
                        fill='x')
            _no_data_note(fenster, koerper, werte)
            return
        # ⚠⚠ **Die Seite heißt „Was noch fehlt" — dann steht hier auch nur
        # das.** Am 06.09.2026: „hier kann, was eingebaut ist, auch raus, ist
        # ja Unfug." Stimmt: Unter der Überschrift „0 Positionen" standen
        # trotzdem vier abgehakte Zeilen.
        #
        # ⚠ Alles abgehakt ist etwas anderes als „nie etwas geplant" — und der
        # Unterschied gehört gesagt, sonst wirkt eine fertige Liste wie eine
        # leere.
        fertige = [x for x in posten if x.get('erledigt')]
        posten = [x for x in posten if not x.get('erledigt')]
        if not posten:
            _body_text(koerper,
                        t('s_ek_alles_erledigt').format(n=len(fertige)),
                        fenster.f_small, color=ACCENT, fill='x', pady=(0, 8))
            _no_data_note(fenster, koerper, werte)
            return

        _fetch_buy_prices(posten, koerper, neu_zeichnen)

        # ⚠⚠ **Der Kopf zählt, was noch zu tun ist — nicht, was einmal
        # geplant war.** Bis zum 06.09.2026 stand „8 Positionen aus 2
        # Schiffen" da, obwohl zwei davon abgehakt waren. Eine Überschrift,
        # die sich beim Abarbeiten nicht ändert, ist keine Auskunft.
        tk.Label(koerper,
                 text=t('s_ek_kopf_1' if len(posten) == 1
                        else 's_ek_kopf').format(
                            n=len(posten), schiffe=werte.get('schiffe') or 0),
                 bg=BG, fg=FG, font=fenster.f_bold,
                 anchor='w').pack(fill='x', pady=(0, 8))

        # Nach Schiff gruppieren — die Reihenfolge kommt schon sortiert an.
        def abhaken(posten_eintrag, ja):
            """Einen Posten als erledigt markieren — und sofort speichern.

            ⚠ Gesucht wird das Schiff über Name **und** Herkunft: Ein Schiff
            kann gleichzeitig im Hangar und auf der Wunschliste stehen (etwa
            ein zweites Exemplar), und der Haken gehört genau an eines davon.
            """
            quelle = (stand.get('wunsch') if posten_eintrag.get('quelle')
                      == cart.WISHLIST else stand.get('schiffe')) or []
            for schiff in quelle:
                if (schiff.get('name') or '') == posten_eintrag.get('schiff'):
                    if cart.set_done(schiff,
                                                 posten_eintrag.get('pfad'),
                                                 ja):
                        meine.save(stand)
                    break
            neu_zeichnen()

        aktuelles = None
        for eintrag in posten:
            if eintrag.get('schiff') != aktuelles:
                aktuelles = eintrag.get('schiff')
                _buy_ship_head(fenster, koerper, eintrag)
            _buy_row(fenster, koerper, eintrag, abhaken=abhaken)

        # Wie viel schon erledigt ist — sonst sieht eine halb abgearbeitete
        # Liste aus wie eine unangetastete.
        # ⚠ Nicht verschweigen, nur nicht auflisten: Wer weiß, dass vier
        # Posten erledigt sind, versteht auch, warum die Summe niedriger ist
        # als erwartet.
        if fertige:
            _body_text(koerper, t('s_ek_abgehakt').format(n=len(fertige)),
                        fenster.f_small, color=ACCENT, fill='x', pady=(6, 0))

        _cart_total(fenster, koerper, posten)
        _no_data_note(fenster, koerper, werte)
        _cart_route(fenster, koerper, posten)

    # ⚠ Beim erneuten Öffnen frisch rechnen: Die Seite wird nur einmal gebaut,
    # und zwischen zwei Besuchen ändert sich im Hangar fast immer etwas.
    fenster.on_show['einkaufsliste'] = neu_zeichnen
    _aufbauen()


def _dismantle(fenster, rahmen):
    """Was ein Fabricator aus einem Teil zurückgibt — vor dem Ausbauen wissen.

    ⭐⭐ **Die Frage eines Bergungsspielers, bevor er den Schneidbrenner
    ansetzt.** Vorschlag vom 06.09.2026: *„unter Bergung ein Extra-Fenster, wo
    man Waffen, Komponenten etc. prüfen kann, wie viel Material man rausbekommt,
    wenn man es im Fabricator zerlegt … so kann ein Salvager gleich entscheiden,
    brauche ich die Komponente evtl. zum Zerlegen."*

    ⚠⚠ **Die halbe Wahrheit wäre hier die gefährlichere.** „Man bekommt 50 %
    zurück" stimmt — aber sechs Rohstoffe stehen auf der Sperrliste des
    Fabricators und kommen **gar nicht** wieder, darunter Quantainium und
    Stileron. Gemessen: Bei **258 von 400** Teilen ist mindestens einer davon
    dabei. Ein Rechner, der stumpf halbiert, schickt zwei Drittel der Spieler
    mit falschen Erwartungen los.
    """
    from . import salvage as bg, crafting

    _heading(fenster, rahmen, t('hf_zerlegen'), t('s_zl_lead'))
    innen = _scroll_area(rahmen)

    regeln = bg.dismantle_rules()
    _body_text(innen,
                t('s_zl_regel').format(prozent=int(regeln['anteil'] * 100),
                                       dauer=regeln['dauer'],
                                       n=len(regeln['gesperrt'])),
                fenster.f_small, fill='x')

    gewaehlt = tk.StringVar()
    block = tk.Frame(innen, bg=BG)
    block.pack(fill='x', padx=24, pady=(14, 0))

    ergebnis = tk.Frame(innen, bg=BG)
    ergebnis.pack(fill='x', padx=24, pady=(14, 20))

    def namen():
        try:
            return sorted((e.get('basis') or '') for e in crafting.all_items()
                          if e.get('basis'))
        except Exception as ausnahme:
            fehler.merken('seiten.zerlegen.namen', ausnahme)
            return []

    def zeigen(name=None):
        for kind in ergebnis.winfo_children():
            kind.destroy()
        gesucht = (name or gewaehlt.get() or '').strip()
        if not gesucht:
            return
        zeilen, dauer = bg.dismantle(gesucht)
        if not zeilen:
            # ⚠ Kein Rezept heisst nicht „gibt nichts zurück" — es heisst, dass
            # wir es nicht wissen. Der Unterschied gehört gesagt.
            _body_text(ergebnis, t('s_zl_kein_rezept').format(name=gesucht),
                        fenster.f_small, color=GOLD, fill='x')
            return

        tk.Label(ergebnis, text=gesucht, bg=BG, fg=FG, font=fenster.f_bold,
                 anchor='w').pack(fill='x', pady=(0, 2))
        tk.Label(ergebnis, text=t('s_zl_dauer').format(dauer=dauer), bg=BG,
                 fg=SUB, font=fenster.f_small, anchor='w').pack(fill='x',
                                                               pady=(0, 8))

        for zeile in zeilen:
            _dismantle_row(fenster, ergebnis, zeile)

        # ⭐ Die Kurzfassung unter dem Strich: Lohnt es sich überhaupt?
        verloren = [z for z in zeilen if z['verloren']]
        if verloren:
            # ⚠ Eigene Fassung für die Einzahl — „1 Rohstoffe" liest sich
            # nach einem Fehler.
            _body_text(ergebnis,
                        t('s_zl_verloren_1' if len(verloren) == 1
                          else 's_zl_verloren').format(
                            n=len(verloren),
                            stoffe=', '.join(z['rohstoff'] for z in verloren)),
                        fenster.f_small, color=GOLD, fill='x', pady=(10, 0))
        else:
            _body_text(ergebnis, t('s_zl_alles_zurueck'), fenster.f_small,
                        color=ACCENT, fill='x', pady=(10, 0))

    feld, liste, _ = _combo_box(fenster, block, gewaehlt, namen,
                                  on_pick=zeigen,
                                  on_confirm=lambda *_a: zeigen(),
                                  empty_text=t('s_hg_nichts_gefunden'),
                                  scrollable=200)
    feld.pack(fill='x')
    liste.pack(fill='x')

    # ⚠ Beim erneuten Öffnen leer anfangen: Die Seite wird nur einmal gebaut,
    # und ein Ergebnis von vorhin gehört nicht zur nächsten Frage.
    def _beim_zeigen():
        gewaehlt.set('')
        for kind in ergebnis.winfo_children():
            kind.destroy()

    fenster.on_show['zerlegen'] = _beim_zeigen


def _dismantle_row(fenster, eltern, zeile):
    """Ein Rohstoff: was drinsteckt, was zurückkommt."""
    rahmen = tk.Frame(eltern, bg=SURFACE)
    rahmen.pack(fill='x', pady=(0, 2))

    tk.Label(rahmen, text=zeile.get('rohstoff') or '', bg=SURFACE,
             fg=SUB if zeile.get('verloren') else FG, font=fenster.f_small,
             anchor='w', width=22).pack(side='left', padx=(12, 0), pady=4)

    # ⚠ **Der Rückgabewert steht rechts und in Farbe** — das ist die Zahl, wegen
    # der jemand diese Seite öffnet. „steckt drin" daneben erklärt sie.
    if zeile.get('verloren'):
        tk.Label(rahmen, text=t('s_zl_nichts'), bg=SURFACE, fg=GOLD,
                 font=fenster.f_small, anchor='e').pack(side='right',
                                                        padx=(0, 12))
    else:
        tk.Label(rahmen, text=t('s_zl_zurueck').format(
            menge=_number(zeile.get('zurueck'))),
            bg=SURFACE, fg=ACCENT, font=fenster.f_small,
            anchor='e').pack(side='right', padx=(0, 12))

    tk.Label(rahmen, text=t('s_zl_drin').format(menge=_number(zeile.get('drin'))),
             bg=SURFACE, fg=SUB, font=fenster.f_small,
             anchor='w').pack(side='left')


def _farm_list(fenster, rahmen):
    """Was an Rohstoffen fehlt, um das Geplante selbst zu bauen.

    ⭐⭐ **Die Gegenrichtung zu „Was noch fehlt".** Dort steht, was die Schiffe
    brauchen und was es kostet; hier steht, was davon noch **im Boden** liegt.
    Der Vorschlag am 06.09.2026: *„Herstellungswarteliste fällt mir da ein,
    wäre dann sinnvoll, dann würde man anhand seines Lagers auch sehen was man
    noch farmen muss."*

    ⚠⚠ **Hier in der Werkstatt und nicht bei den Schiffen** — ebenfalls seine
    Einordnung: *„schiebt man Herstellungsliste nicht eher unten in die
    Werkstatt?"* Stimmt. Die Werkstatt-Kette lautet „was habe ich an Material →
    was baue ich → wo hole ich es", und eine Liste fehlender Rohstoffe ist die
    Antwort auf genau die erste Frage. Bei den Schiffen ginge es um Geld, hier
    um Erz.

    ⚠ **Gerechnet wird über ALLE Posten zusammen, nicht Rezept für Rezept.**
    Zwei Bauteile, die je 2 Iron brauchen, bei 3 Iron im Lager: Einzeln
    geprüft meldet jedes „reicht", zusammen fehlt eines. Wer die Fehlmengen
    einzeln addiert, schickt den Spieler mit zu wenig Material los.
    """
    from . import cart, fleet as meine

    _heading(fenster, rahmen, t('hf_farmliste'), t('s_fl_lead'))
    innen = _scroll_area(rahmen)

    koerper = tk.Frame(innen, bg=BG)
    koerper.pack(fill='x', padx=24, pady=(10, 20))

    def neu_zeichnen():
        for kind in koerper.winfo_children():
            kind.destroy()
        _aufbauen()

    def _merkzettel_block(werte):
        """Was von Hand vorgemerkt wurde — mit Stückzahl und Streichen.

        ⭐⭐ **Der einzige Weg hierher, der ohne Schiff auskommt.** Alles andere
        auf dieser Seite kommt aus den Schiffen: Hangar oder Wunschliste, über
        belegte Steckplätze. Ein Helm, eine Rüstung, eine FPS-Waffe hat keinen
        Steckplatz — vorgemerkt wird sie in der Herstellung, und hier steht
        sie dann.

        Gemeldet von Haldjas am 06.09.2026: *„‚What to farm' ist irgendwie
        bisschen unnötig komplex — man geht da rein, wird dann zu ‚still
        missing' geschickt und weiß dann aber nicht so genau, was man machen
        soll."* Der Umweg über die Wunschliste war nirgends erklärt, und für
        FPS-Ausrüstung gab es ihn gar nicht.

        ⚠ **Auch bei leerem Merkzettel wird der Satz gezeigt** — er sagt, wo
        der Knopf sitzt. Ein leerer Bereich ohne Erklärung wirft genau die
        Frage auf, die diese Rückmeldung ausgelöst hat.
        """
        eintraege = meine.notepad()
        rahmen_mz = tk.Frame(koerper, bg=BG)
        rahmen_mz.pack(fill='x', pady=(0, 14))
        tk.Label(rahmen_mz, text=t('s_mz_titel'), bg=BG, fg=FG,
                 font=fenster.f_bold, anchor='w').pack(fill='x')
        if not eintraege:
            _body_text(rahmen_mz, t('s_mz_leer'), fenster.f_small, fill='x')
            return

        def _streichen(name):
            stand = meine.load()
            meine.notepad_remove(stand, name)
            meine.save(stand)
            neu_zeichnen()

        # ⚠⚠ **Das Material gehört an den Eintrag, nicht nur in die Summe
        # unten.** Am 06.09.2026 gemeldet: „Man sieht da aber kein Material,
        # was man farmen muss — unter den Waffen würde es Sinn machen, dass man
        # das zu farmende Material sieht." Genau so: Die Summe unten beantwortet
        # „wie viel Erz brauche ich insgesamt", hier steht „und wofür".
        from . import crafting as _mz_herst
        # ⚠⚠⚠ **Die Fehlmengen kommen aus DERSELBEN Rechnung wie die Summe
        # darunter.** Der erste Anlauf fragte hier das Lager direkt und zeigte
        # „hast 8,01", während zehn Zeilen tiefer „hast 3,44" stand. Beide
        # Zahlen waren richtig gerechnet — oben der volle Lagerbestand, unten
        # der Anteil, der für diesen Bedarf zugeteilt wurde — und genau das ist
        # der Fehler: Zwei Zahlen mit derselben Beschriftung auf einer Seite.
        #
        # Der Einzelposten sagt jetzt nur noch, **was er braucht**; ob es
        # reicht, sagt die Farbe, und die stammt aus der Gesamtrechnung. Eine
        # Seite, eine Wahrheit.
        # ⚠⚠⚠ **Die Rechnung wird ÜBERGEBEN, nicht neu angestellt.** Der erste
        # Anlauf rief hier `cart.farm_list()` ein zweites Mal — dieselbe
        # Rechnung über alle Schiffe und Rezepte, nur damit die Farbe stimmt.
        # Gemessen: Die Seite brauchte dadurch **3937 ms** statt 60.
        #
        # Eine teure Rechnung gehört einmal gemacht und weitergereicht.
        _fehlt_gesamt = set()
        for _e in ((werte or {}).get('fehlt') or []):
            _fehlt_gesamt.add((_e.get('rohstoff') or '').strip().lower())

        for e in eintraege:
            zeile = tk.Frame(rahmen_mz, bg=BG)
            zeile.pack(fill='x', pady=(6, 0))
            # ⚠ Der Streichen-Knopf zuerst, dann der Name mit `expand` —
            # sonst schiebt ein langer Bauplanname ihn aus dem Fenster.
            # Dieselbe Packreihenfolge-Falle, die hier schon zweimal zugeschlagen
            # hat (Kaufen-Knopf, „geändert"-Marke).
            _button(fenster, zeile, t('s_mz_weg'),
                   lambda n=e.get('name'): _streichen(n)).pack(side='right',
                                                               padx=(8, 0))
            menge = int(e.get('anzahl') or 1)
            if menge > 1:
                tk.Label(zeile, text='%d× %s' % (menge, t('s_mz_stueck')),
                         bg=BG, fg=SUB, font=fenster.f_small,
                         anchor='e').pack(side='right', padx=(8, 0))
            tk.Label(zeile, text=e.get('name') or '', bg=BG, fg=FG,
                     font=fenster.f_base, anchor='w').pack(side='left',
                                                            fill='x',
                                                            expand=True)

            # Die Zutaten darunter — mit der Stückzahl multipliziert und
            # gegen das Lager gehalten.
            try:
                rez = _mz_herst.recipe(e.get('name') or '')
            except Exception:
                rez = None
            if not rez or not rez.get('stufen'):
                # ⚠ Kein Rezept ist eine Aussage, kein Grund zu schweigen —
                # sonst steht der Eintrag ohne Erklärung nackt da.
                tk.Label(rahmen_mz, text=t('s_mz_kein_rezept'), bg=BG, fg=SUB,
                         font=fenster.f_small,
                         anchor='w').pack(fill='x', padx=(18, 0))
                continue
            for stufe in rez['stufen']:
                for _slot, rohstoff, einzeln, guete in (stufe.get('zutaten')
                                                        or []):
                    if not rohstoff:
                        continue
                    braucht = float(einzeln or 0) * menge
                    knapp = (rohstoff or '').strip().lower() in _fehlt_gesamt
                    zutat = tk.Frame(rahmen_mz, bg=BG)
                    zutat.pack(fill='x', padx=(18, 0))
                    tk.Label(zutat, text=rohstoff, bg=BG, fg=SUB,
                             font=fenster.f_small, anchor='w',
                             width=20).pack(side='left')
                    # ⚠ Grün heißt „reicht", Gold „fehlt" — dieselbe Sprache
                    # wie überall im Werkzeug, und die Farbe stammt aus
                    # derselben Rechnung wie die Summe darunter.
                    #
                    # ⚠ `_menge_text()` statt `%.2f`: Deutsche Zahlen haben
                    # ein Komma. Sonst stand oben „4.64" und unten „8,8" auf
                    # derselben Seite.
                    tk.Label(zutat,
                             text=t('s_mz_braucht') % _amount_text(braucht),
                             bg=BG, fg=GOLD if knapp else ACCENT,
                             font=fenster.f_small,
                             anchor='w').pack(side='left')

    def _aufbauen():
        werte = cart.farm_list(meine.load())
        fehlt = werte.get('fehlt') or []
        reicht = werte.get('vollstaendig') or []
        anzahl = werte.get('posten') or 0

        _merkzettel_block(werte)

        # ⚠⚠ **Drei Lagen, drei Sätze** — dieselbe Falle wie überall hier:
        # „nichts geplant", „alles da" und „nichts zu tun" sehen im Code gleich
        # aus. Wer sie zusammenwirft, sagt jemandem ohne Plan, er sei fertig.
        if not anzahl:
            _body_text(koerper, t('s_fl_nichts_geplant'), fenster.f_small,
                        fill='x')
            return

        tk.Label(koerper, text=t('s_fl_kopf').format(n=anzahl), bg=BG, fg=FG,
                 font=fenster.f_bold, anchor='w').pack(fill='x', pady=(0, 8))

        if not fehlt:
            _body_text(koerper, t('s_fl_alles_da'), fenster.f_small,
                        color=ACCENT, fill='x')
        else:
            for eintrag in fehlt:
                _farm_row(fenster, koerper, eintrag, fehlend=True)

        if reicht:
            tk.Label(koerper, text=t('s_fl_reicht_kopf').format(n=len(reicht)),
                     bg=BG, fg=SUB, font=fenster.f_small,
                     anchor='w').pack(fill='x', pady=(14, 4))
            for eintrag in reicht:
                _farm_row(fenster, koerper, eintrag, fehlend=False)

        # ⚠ Was gar nicht gerechnet werden konnte, wird genannt — eine Liste,
        # der stillschweigend Posten fehlen, ist schlimmer als eine kurze.
        ohne = werte.get('ohne_rezept') or []
        if ohne:
            _body_text(koerper,
                        t('s_fl_ohne_rezept').format(n=len(ohne),
                                                     teile=', '.join(ohne)),
                        fenster.f_small, color=GOLD, fill='x', pady=(12, 0))

    fenster.on_show['farmliste'] = neu_zeichnen
    _aufbauen()


def _farm_row(fenster, eltern, eintrag, fehlend):
    """Ein Rohstoff: was gebraucht wird, was da ist, was fehlt."""
    zeile = tk.Frame(eltern, bg=SURFACE)
    zeile.pack(fill='x', pady=(0, 2))

    tk.Label(zeile, text=eintrag.get('rohstoff') or '', bg=SURFACE,
             fg=FG if fehlend else SUB, font=fenster.f_small, anchor='w',
             width=24).pack(side='left', padx=(12, 0), pady=4)

    # ⚠ **Die Fehlmenge steht rechts und in Farbe** — das ist die Zahl, mit der
    # man losfliegt. „Brauchst 4,4 · hast 0" daneben sagt, wie sie zustande
    # kommt; ohne sie wäre die Zahl eine Behauptung.
    if fehlend:
        tk.Label(zeile, text=t('s_fl_fehlt').format(
            menge=_number(eintrag.get('differenz'))),
            bg=SURFACE, fg=GOLD, font=fenster.f_small,
            anchor='e').pack(side='right', padx=(0, 12))
    else:
        tk.Label(zeile, text=t('s_fl_genug'), bg=SURFACE, fg=ACCENT,
                 font=fenster.f_small, anchor='e').pack(side='right',
                                                        padx=(0, 12))

    tk.Label(zeile, text=t('s_fl_stand').format(
        braucht=_number(eintrag.get('benoetigt')),
        hat=_number(eintrag.get('vorhanden'))),
        bg=SURFACE, fg=SUB, font=fenster.f_small,
        anchor='w').pack(side='left')

    # ⚠ **Zu geringe Güte ist kein Bestand, aber auch kein Nichts.** Wer 20
    # Stileron mit Q 100 im Lager hat und Q 500 braucht, soll das erfahren —
    # sonst sucht er im Lager nach etwas, das dort sichtbar liegt, und
    # versteht die Meldung nicht.
    zu_gering = eintrag.get('zu_gering') or 0
    if zu_gering:
        tk.Label(zeile, text=t('s_fl_zu_gering').format(
            menge=_number(zu_gering),
            guete=_number(eintrag.get('mindestguete'))),
            bg=SURFACE, fg=SUB, font=fenster.f_small,
            anchor='w').pack(side='left', padx=(10, 0))


def _number(value):
    """Eine Menge lesbar — ganze Zahlen ohne Komma, kleine Mengen genauer.

    ⚠ `4.4000000000000004` ist dieselbe Zahl wie `4,4`, sieht aber nach einem
    Fehler aus. Und `4,0` neben `4` in derselben Spalte liest sich, als wären
    es zwei verschiedene Angaben.

    ⚠⚠ **Unter 1 braucht es zwei Stellen.** Die erste Fassung rundete immer
    auf eine — beim Durchklicken des Zerlege-Rechners am 06.09.2026 wurde
    daraus aus 0,64 ein „0,6" und aus 0,32 ein „0,3". Bei Rohstoffmengen, die
    fast alle unter eins liegen, ist das keine Rundung mehr, sondern eine
    andere Zahl: Wer 0,32 zurückbekommt, hat nicht 0,3.
    """
    try:
        z = float(value or 0)
    except (TypeError, ValueError):
        return '0'
    if abs(z - round(z)) < 0.005:
        return str(int(round(z)))
    if abs(z) < 10:
        return ('%.2f' % z).rstrip('0').rstrip('.').replace('.', ',')
    return ('%.1f' % z).replace('.', ',')


def _no_data_note(fenster, eltern, werte):
    """Welche Schiffe gar nicht mitgerechnet werden konnten.

    ⚠ **Verschwiegen wäre schlimmer als unvollständig.** Ohne diesen Satz
    stünde eine Summe da, die stillschweigend ein paar Schiffe auslässt — und
    wer danach einkaufen geht, steht mit zu wenig Geld am Terminal.
    """
    fehlen = werte.get('ohne_steckplatzdaten') or []
    if not fehlen:
        return
    _body_text(eltern,
                t('s_ek_ohne_daten').format(n=len(fehlen),
                                            schiffe=', '.join(fehlen)),
                fenster.f_small, color=GOLD, fill='x', pady=(8, 0))


def _buy_ship_head(fenster, eltern, eintrag):
    """Die Zwischenüberschrift je Schiff — mit Herkunft."""
    from . import cart

    zeile = tk.Frame(eltern, bg=BG)
    zeile.pack(fill='x', pady=(12, 4))
    tk.Label(zeile, text=eintrag.get('schiff') or '', bg=BG, fg=FG,
             font=fenster.f_bold, anchor='w').pack(side='left')
    # ⚠ Woher das Schiff kommt, gehört an die Überschrift: Auf einer Rechnung
    # mit vierzig Positionen ist der Unterschied zwischen „habe ich" und
    # „will ich haben" die wichtigste Angabe überhaupt.
    marke = (t('s_ek_aus_wunsch')
             if eintrag.get('quelle') == cart.WISHLIST
             else t('s_ek_aus_hangar'))
    tk.Label(zeile, text=marke, bg=BG, fg=SUB, font=fenster.f_small,
             anchor='w').pack(side='left', padx=(10, 0))


def _buy_row(fenster, eltern, eintrag, abhaken=None):
    """Eine Rechnungsposition: wo, was, wie, wie viel — und ein Haken davor."""
    from . import cart

    fertig = bool(eintrag.get('erledigt'))
    zeile = tk.Frame(eltern, bg=SURFACE)
    zeile.pack(fill='x', pady=(0, 2))

    # ⭐⭐ **Der Haken ist die einzige Möglichkeit, das zu wissen.** Am
    # 06.09.2026 gefragt: „wenn etwas von der Liste gekauft wurde, und im
    # Schiff eingebaut ist, wie erfährt die Einkaufsliste davon?" Gar nicht —
    # das Spiel schreibt nicht in die Game.log, was in einem Schiff steckt.
    # Also wird nichts erraten, sondern abgehakt wie auf jedem Einkaufszettel.
    # Beim Selbstherstellen genauso: ein Haken für beide Wege.
    if abhaken is not None and eintrag.get('sorte') == cart.PART:
        haken = icons.line(zeile, 'haken', background=SURFACE,
                              color=icons.GREEN if fertig else icons.GREY,
                              font=fenster.f_small)
        haken.configure(cursor='hand2')
        haken.pack(side='left', padx=(12, 8), pady=4)
        haken.bind('<Button-1>', lambda _e: abhaken(eintrag, not fertig))
        rand = (0, 0)
    else:
        rand = (12, 0)

    # ⚠ Erledigtes bleibt lesbar, tritt aber zurück: Es ist erledigt, nicht
    # ungültig. Wer den Haken versehentlich setzt, muss die Zeile wiederfinden.
    haupt = SUB if fertig else FG
    neben = LINE if fertig else SUB

    # Position (Steckplatz) — bei einem Schiff steht dort, dass es das Schiff
    # selbst ist, damit die Spalte nie leer bleibt.
    pos = eintrag.get('position') or ''
    if eintrag.get('sorte') == cart.SHIP:
        pos = t('s_ek_das_schiff')
    tk.Label(zeile, text=pos, bg=SURFACE, fg=neben, font=fenster.f_small,
             anchor='w', width=22).pack(side='left', padx=rand, pady=4)

    tk.Label(zeile, text=eintrag.get('name') or '', bg=SURFACE, fg=haupt,
             font=fenster.f_small, anchor='w').pack(side='left')

    # Güte und Klasse — dieselbe Angabe wie in der Teileauswahl. Auf einer
    # Rechnung sagt „Fortitude" wenig, „Fortitude · C · Industrie" viel.
    kennzeichen = (_part_label({'kennung': eintrag.get('ref')})
                   if eintrag.get('sorte') == cart.PART else '')
    if kennzeichen:
        tk.Label(zeile, text=kennzeichen, bg=SURFACE, fg=neben,
                 font=fenster.f_small, anchor='w').pack(side='left',
                                                        padx=(10, 0))

    # Rechts der Betrag, daneben der gewählte Weg.
    weg = eintrag.get('weg')
    angabe = (eintrag.get('kauf') if weg == cart.BUY
              else eintrag.get('bau')) or {}
    if fertig:
        # ⚠ Kein Betrag mehr, sondern das Wort: Ein abgehakter Posten kostet
        # nichts mehr, und eine durchgestrichene Zahl waere nur Ballast.
        betrag, farbe = t('s_ek_erledigt'), ACCENT
    elif angabe.get('zustand') == cart.KNOWN:
        betrag = _money(angabe.get('preis') if weg == cart.BUY
                       else angabe.get('material'))
        farbe = FG
    elif angabe.get('zustand') == cart.NOT_CHECKED:
        betrag, farbe = t('s_wk_nicht_geprueft'), SUB
    else:
        betrag, farbe = t('s_ek_kein_betrag'), GOLD
    tk.Label(zeile, text=betrag, bg=SURFACE, fg=farbe, font=fenster.f_small,
             anchor='e').pack(side='right', padx=(0, 12))
    tk.Label(zeile, text=t('s_wk_kaufen') if weg == cart.BUY
             else t('s_wk_bauen'),
             bg=SURFACE, fg=neben, font=fenster.f_small,
             anchor='e').pack(side='right', padx=(0, 16))


def _fetch_buy_prices(posten, widget, neu_zeichnen):
    """Fehlende Ladenpreise der Rechnung im Hintergrund nachholen.

    ⚠ Dieselbe Begründung wie bei `_fetch_cart_prices`: `rechnung()`
    fasst bewusst kein Netz an — zwölf Posten wären zwölf Netzrunden, während
    die Oberfläche steht. Das Holen gehört hierher.
    """
    from . import cart, shops

    offen = cart.missing_prices(posten)
    if not offen:
        return

    def arbeit():
        geholt = False
        for kennung, name in offen:
            try:
                if not shops.known(kennung):
                    shops.fetch(kennung, name=name or '')
                    geholt = True
            except Exception as ausnahme:
                fehler.merken('seiten.einkauf.preis', ausnahme)
        if geholt:
            try:
                widget.after(0, neu_zeichnen)
            except Exception:
                pass

    threading.Thread(target=arbeit, daemon=True).start()


def _wish_row(fenster, eltern, eintrag, daten, meldung, neu_zeichnen):
    """Ein Schiff auf der Wunschliste — mit dem, was es kostet und wo es steht.

    ⚠ **Die Preise stehen schon im Werkzeug**, es wird nichts nachgeladen:
    `ships.buy_at()` und `ships.rent_at()` kommen aus derselben UEX-Ablage,
    die der Routenplaner ohnehin füllt. Ein Wunsch ohne Preis wäre eine
    Merkliste; mit Preis ist es eine Entscheidungshilfe.
    """
    from . import fleet as meine, ships as alle_schiffe

    name = eintrag.get('name') or ''
    karte = _card(eltern, pady=(0, 6))
    kopf = tk.Frame(karte, bg=SURFACE)
    kopf.pack(fill='x', padx=16, pady=(10, 2))
    tk.Label(kopf, text=name, bg=SURFACE, fg=FG, font=fenster.f_bold,
             anchor='w').pack(side='left')

    def streichen():
        if meine.wishlist_remove(daten['stand'], name):
            meine.save(daten['stand'])
            meldung['text'], meldung['farbe'] = '', SUB
        neu_zeichnen()

    _button(fenster, kopf, t('s_hg_wunsch_streichen'), streichen).pack(
        side='right')

    unten = tk.Frame(karte, bg=SURFACE)
    unten.pack(fill='x', padx=16, pady=(0, 10))
    kauf = alle_schiffe.buy_at(name)
    miete = alle_schiffe.rent_at(name)
    teile = []
    if kauf:
        bester = kauf[0]
        teile.append(t('s_hg_wunsch_kauf') % (_money(bester.get('preis') or 0),
                                              bester.get('ort') or '?'))
    if miete:
        teile.append(t('s_hg_wunsch_miete')
                     % _money(miete[0].get('preis') or 0))
    # ⚠ **Kein Preis heißt nicht „gibt es nicht".** Viele Schiffe sind im Spiel
    # gar nicht käuflich, sondern nur über Echtgeld zu haben — das ist eine
    # Auskunft, keine Lücke.
    tk.Label(unten, text='  ·  '.join(teile) if teile
             else t('s_hg_wunsch_kein_preis'),
             bg=SURFACE, fg=SUB if teile else GOLD, font=fenster.f_small,
             anchor='w').pack(side='left')

    # Dieselbe Marke wie im Hangar — ein geplantes Wunschschiff hat oft mehr
    # offene Posten als ein fertiges.
    from . import cart as _wk_marke
    offen = _wk_marke.open_count(eintrag)
    if offen:
        tk.Label(unten, text=t('s_hg_offen').format(n=offen), bg=SURFACE,
                 fg=ACCENT, font=fenster.f_small,
                 anchor='w').pack(side='left', padx=(12, 0))

    # ⚠⚠ **Auch ein Wunschschiff lässt sich ausstatten.** Am 06.09.2026
    # gefragt: „was ist, wenn jemand ein Schiff und dazu ein besseres Fitting
    # bauen oder kaufen will?" Genau hier ist die Planung am meisten wert — vor
    # dem Kauf, wenn die Summe noch eine Entscheidung ist und keine Quittung.
    #
    # Der Block ist derselbe wie im Hangar: `warenkorb` arbeitet auf einem
    # beliebigen Eintrag und legt seine Wahl in dessen Feld `belegung` ab. Ein
    # Wunsch-Eintrag ist dafür so gut wie ein Hangar-Eintrag; gespeichert wird
    # ohnehin die ganze Datei.
    #
    # ⚠ Was hier geplant wird, bleibt **Planung**: Ein Wunschschiff taucht
    # nirgends in „passt in dein Schiff" auf. Sonst würde das Werkzeug über
    # ein Schiff Auskunft geben, das dem Spieler gar nicht gehört.
    _cart_box(fenster, karte, eintrag, daten)


def _hangar_row(fenster, eltern, eintrag, daten, meldung, neu_zeichnen):
    """Eine Schiffszeile. Gibt `1` zurück, wenn Steckplätze fehlen."""
    from . import fleet as meine, erkul

    name = eintrag.get('name') or ''
    plaetze = erkul.slot_counts(name, eintrag.get('hersteller', ''),
                            eintrag.get('kurz', ''), eintrag.get('hkurz', ''))
    karte = _card(eltern, pady=(0, 6))

    kopf = tk.Frame(karte, bg=SURFACE)
    kopf.pack(fill='x', padx=16, pady=(10, 2))
    tk.Label(kopf, text=name, bg=SURFACE, fg=FG, font=fenster.f_bold,
             anchor='w').pack(side='left')

    def austragen():
        if meine.remove(daten['stand'], name, eintrag.get('hersteller', '')):
            meine.save(daten['stand'])
            meldung['text'], meldung['farbe'] = '', SUB
        neu_zeichnen()

    _button(fenster, kopf, t('s_hg_entfernen'), austragen).pack(side='right')

    # Zweite Zeile: Herkunft, LTI, Steckplätze — die drei Angaben, die den
    # Unterschied machen.
    teile = [t('s_hg_pledge') if eintrag.get('herkunft') == meine.PLEDGE
             else t('s_hg_ingame')]
    if eintrag.get('lti'):
        teile.append(t('s_hg_lti'))

    # ⚠⚠ **Drei Zustände, nicht zwei** — und der Unterschied ist der ganze
    # Punkt. Bis zum 06.09.2026 stand bei jedem Schiff ohne Steckplatz-Daten
    # „noch nicht im Spiel". Das war schlicht **falsch**: Die Ironclad Assault
    # und die Super Hornet Mk II fliegen längst, sie waren nur nicht zugeordnet.
    # Aus einer fehlenden Zuordnung eine Aussage über das Spiel zu machen, ist
    # genau die Sorte Behauptung, die dieses Werkzeug nicht aufstellt.
    #
    # | Lage | was dasteht |
    # |---|---|
    # | Steckplätze vorhanden | „N Steckplätze" |
    # | UEX sagt `is_concept` | „Konzept — noch nicht im Spiel" |
    # | sonst nichts bekannt | „keine Steckplatz-Daten" |
    #
    # Der mittlere Fall stützt sich auf eine **Fremdangabe** (UEX pflegt das
    # Feld), nicht auf unser eigenes Nichtwissen.
    from . import ships as alle_schiffe
    if plaetze:
        teile.append(t('s_hg_plaetze').format(
            n=sum(int(p.get('anzahl') or 0) for p in plaetze)))
        farbe = SUB
    elif alle_schiffe.is_concept(name):
        teile.append(t('s_hg_konzept'))
        farbe = GOLD
    else:
        teile.append(t('s_hg_ohne_daten'))
        farbe = SUB
    unten = tk.Frame(karte, bg=SURFACE)
    unten.pack(fill='x', padx=16, pady=(0, 10))
    tk.Label(unten, text='  ·  '.join(teile), bg=SURFACE,
             fg=farbe, font=fenster.f_small,
             anchor='w').pack(side='left')

    # ⭐⭐ **Offene Posten sieht man, ohne aufzuklappen.** Am 06.09.2026
    # gefragt: „wie sehe ich ohne Aufklappen, dass ich dort noch nicht
    # besorgte Komponenten habe?" Gar nicht — bei vierzig Schiffen klappt
    # niemand alle auf, und was man aufklappen muss, um es zu finden, findet
    # man nicht.
    #
    # ⚠ In der Markenfarbe, nicht in Grau: Dieselbe Rückmeldung wie zu „passt
    # in dein Schiff" — „in Grau nimmt es keiner wahr und fragt sich dann, wo
    # er die Info findet".
    from . import cart as _wk_marke
    # ⚠ Ein eigener Rahmen, damit sich die Marke nachziehen lässt, ohne die
    # ganze Zeile neu zu bauen — beim Abhaken darf die aufgeklappte
    # Ausstattung nicht zuklappen.
    marke_rahmen = tk.Frame(unten, bg=SURFACE)
    marke_rahmen.pack(side='left')

    def marke_setzen():
        for kind in marke_rahmen.winfo_children():
            kind.destroy()
        _draw_badge(fenster, marke_rahmen, eintrag)

    marke_setzen()
    offen = _wk_marke.open_count(eintrag)
    # Ausstattung und Warenkorb — aufklappbar, damit ein Hangar mit vierzig
    # Schiffen eine Liste bleibt und keine Bleiwüste wird.
    _cart_box(fenster, karte, eintrag, daten, beim_aendern=marke_setzen)
    return 0 if plaetze else 1


def _draw_badge(fenster, eltern, eintrag):
    """Die Marke an einer Schiffszeile: offene Posten oder „fertig gefittet".

    ⚠⚠ **Die Fertig-Marke ist eine Warnung, keine Auszeichnung.** Ein neu
    geclaimtes Schiff kommt in seiner Werksausstattung zurück: Wer ein
    aufgerüstetes Schiff ohne die passende Versicherung claimt, verliert alles
    Eingebaute. Deshalb steht sie in **Gold** wie die übrigen
    Vorsichtshinweise, nicht in der Markenfarbe wie eine Erfolgsmeldung.
    """
    from . import cart

    offen = cart.open_count(eintrag)
    if offen:
        tk.Label(eltern, text=t('s_hg_offen').format(n=offen), bg=SURFACE,
                 fg=ACCENT, font=fenster.f_small,
                 anchor='w').pack(side='left', padx=(12, 0))
    elif cart.fully_fitted(eintrag):
        haken = icons.line(eltern, 'haken', background=SURFACE,
                              color=icons.YELLOW, font=fenster.f_small)
        haken.pack(side='left', padx=(12, 4))
        tk.Label(eltern, text=t('s_hg_fertig'), bg=SURFACE, fg=GOLD,
                 font=fenster.f_small, anchor='w').pack(side='left')


def _cart_box(fenster, karte, eintrag, daten, beim_aendern=None):
    """„Ausstattung & Warenkorb" unter einer Schiffszeile — erst auf Klick.

    ⚠ Gebaut wird der Inhalt **beim ersten Aufklappen**, nicht beim Zeichnen
    der Liste. Sonst kostet jede Hangar-Seite so viele Steckplatz-Durchläufe,
    wie der Spieler Schiffe hat — bei vierzig Schiffen wartet er auf etwas,
    das er gar nicht sehen wollte.
    """
    from . import cart, fleet as meine

    kasten = tk.Frame(karte, bg=SURFACE)
    kasten.pack(fill='x')

    kopf = tk.Frame(kasten, bg=SURFACE, cursor='hand2')
    kopf.pack(fill='x', padx=16, pady=(0, 10))
    pfeil = icons.line(kopf, 'aufklappen', background=SURFACE,
                          font=fenster.f_small)
    pfeil.pack(side='left', padx=(0, 8))
    tk.Label(kopf, text=t('s_wk_titel'), bg=SURFACE, fg=FG,
             font=fenster.f_small, anchor='w').pack(side='left')

    koerper = tk.Frame(kasten, bg=SURFACE)

    def neu():
        """Den Block neu aufbauen — nach jeder Änderung an der Ausstattung.

        ⚠⚠ **Und die Zeile darüber mitziehen.** Der Block kennt nur sich
        selbst; die Marke „4 noch zu besorgen" steht aber in der Schiffszeile,
        eine Ebene höher. Ohne diesen Rückruf hakte man alle vier Posten ab und
        die Zeile behauptete weiter, es seien vier offen. Am 06.09.2026
        gemeldet: „habe die angehakt, aber steht immer noch 4 zu besorgen."
        """
        for kind in koerper.winfo_children():
            kind.destroy()
        _cart_content(fenster, koerper, eintrag, daten, neu)
        if beim_aendern is not None:
            beim_aendern()

    def umschalten(_=None):
        if koerper.winfo_ismapped():
            koerper.pack_forget()
            pfeil.swap_symbol('aufklappen')
        else:
            neu()
            koerper.pack(fill='x', after=kopf)
            pfeil.swap_symbol('zuklappen')

    for teil in (kopf, pfeil) + tuple(kopf.winfo_children()):
        teil.bind('<Button-1>', umschalten)


    # ⭐ Der Pfeil hebt sich ab, wenn die Maus die Kopfzeile
    # trifft — anklickbar ist hier die Zeile, nicht der Pfeil.
    icons.hover_group(kopf, pfeil)

def _cart_content(fenster, eltern, eintrag, daten, neu_zeichnen):
    """Der Inhalt: Steckplätze, Warenkorb, Summe, Kaufroute."""
    from . import cart, fleet as meine

    zustand, liste = cart.line_items(eintrag)

    # ⚠⚠ **Drei Zustände, drei verschiedene Sätze.** „Keine Daten" und „nichts
    # zu besorgen" sehen im Code gleich aus — beides ist eine leere Liste. Wer
    # sie gleich behandelt, sagt jemandem mit fehlenden Steckplatz-Daten, an
    # seinem Schiff sei alles in Ordnung. Genau diese Verwechslung stand am
    # 06.09.2026 bei jedem Bauplan.
    if zustand == cart.NO_DATA:
        _body_text(eltern, t('s_wk_keine_daten'), fenster.f_small,
                    bg=SURFACE, fill='x', padx=(46, 16), pady=(0, 10),
                    inset=78)
        return

    _slot_list(fenster, eltern, eintrag, daten, neu_zeichnen)

    if zustand == cart.NOTHING_OPEN:
        # ⚠⚠ **Zwei Gründe für „nichts offen", zwei verschiedene Sätze.**
        # Entweder war nie etwas geplant (dann steckt die Werksausstattung
        # drin), oder alles Geplante ist eingebaut — und dann hängt an diesem
        # Schiff ein echtes Risiko: Ein neu geclaimtes Schiff kommt in der
        # Werksausstattung zurück, ohne passende Versicherung sind die
        # eingebauten Teile weg. Beides gleich zu behandeln hieße, die
        # teuerste Auskunft dieser Seite zu verschweigen.
        if cart.fully_fitted(eintrag):
            _body_text(eltern, t('s_hg_fertig_hilfe'), fenster.f_small,
                        color=GOLD, bg=SURFACE, fill='x', padx=(46, 16),
                        pady=(4, 10), inset=78)
        else:
            _body_text(eltern, t('s_wk_nichts_offen'), fenster.f_small,
                        bg=SURFACE, fill='x', padx=(46, 16), pady=(4, 10),
                        inset=78)
        return

    cart.enrich(liste)
    _fetch_cart_prices(liste, eltern, neu_zeichnen)

    # ⚠ **Zählt, was noch zu tun ist.** Derselbe Fehler wie im Kopf der
    # Sammelliste: „Warenkorb (2)" blieb bei zwei stehen, obwohl beide Posten
    # abgehakt waren. Beim Durchklicken am 06.09.2026 aufgefallen — die Marke
    # in der Zeile zog schon mit, diese Überschrift nicht.
    noch_offen = [x for x in liste if not x.get('erledigt')]
    fertig = [x for x in liste if x.get('erledigt')]

    # ⚠⚠ **Eingebautes steht nicht mehr im Warenkorb.** Am 06.09.2026: „wenn
    # etwas eingebaut ist, muss es auch nicht mehr im Warenkorb unter dem
    # Schiff stehen, das ist verschenkter Platz." Stimmt — unter der
    # Überschrift „Warenkorb (0)" standen trotzdem vier Karten, jede mit dem
    # Vermerk, dass sie erledigt ist. Ein Korb zeigt, was noch hineingehört.
    #
    # ⚠ Aber **nicht spurlos**: Eine Zeile nennt die Zahl, und ein Klick holt
    # sie zurück. Wer einen Haken versehentlich setzt, muss ihn wiederfinden —
    # „Zurücksetzen" wäre der falsche Weg zurück, das wirft auch die getroffene
    # Auswahl weg und stellt die Werksausstattung wieder her.
    tk.Label(eltern, text=t('s_wk_posten').format(n=len(noch_offen)),
             bg=SURFACE, fg=FG, font=fenster.f_bold, anchor='w').pack(
                 fill='x', padx=(46, 16), pady=(8, 4))

    for posten in noch_offen:
        _cart_item(fenster, eltern, eintrag, posten, neu_zeichnen)

    if fertig:
        _installed_items(fenster, eltern, eintrag, fertig, neu_zeichnen)

    _cart_total(fenster, eltern, liste)
    _cart_route(fenster, eltern, liste)


def _fetch_cart_prices(liste, widget, neu_zeichnen):
    """Fehlende Ladenpreise im Hintergrund nachholen, dann neu zeichnen.

    ⚠⚠ **Ohne das steht „wird nachgeschlagen …" für immer da.** Der Zustand
    `NICHT_GEPRUEFT` heißt wörtlich „noch niemand hat nachgesehen" — aber
    nachgesehen hat auch niemand, weil der Warenkorb den Abruf nie ausgelöst
    hat. Der Satz war also wahr und trotzdem eine Sackgasse: Am 06.09.2026
    gemeldet mit „kaufen lädt keinen Preis". Die Bergungs-Seite holt ihre
    Preise seit jeher selbst nach; hier fehlte genau dieser Schritt.

    ⚠ Im **Hintergrund**, nicht im Zeichnen. Jeder unbekannte Posten ist ein
    Netzabruf; bei zwölf Posten stünde die Oberfläche sonst sekundenlang. Erst
    wenn wirklich etwas dazukam, wird neu gezeichnet — sonst flackert die
    Liste bei jedem Aufklappen ohne Grund.
    """
    from . import cart, shops

    offen = [p for p in liste
             if (p.get('kauf') or {}).get('zustand') == cart.NOT_CHECKED
             and p.get('ref')]
    if not offen:
        return

    def arbeit():
        geholt = False
        for posten in offen:
            try:
                if not shops.known(posten['ref']):
                    shops.fetch(posten['ref'], name=posten.get('name') or '')
                    geholt = True
            except Exception as ausnahme:
                fehler.merken('seiten.cart.preis', ausnahme)
        if geholt:
            # ⚠ Zurück in den Oberflächen-Faden. Tk aus einem Thread heraus
            # anzufassen ist der Weg in Abstürze, die sich nicht nachstellen
            # lassen.
            try:
                widget.after(0, neu_zeichnen)
            except Exception:
                pass

    threading.Thread(target=arbeit, daemon=True).start()


def _notice(window, title, text):
    """Ein Hinweis im Programmstil — **nie** der System-Dialog von Tk.

    ⚠⚠ **Warum das keine Stilfrage ist.** Ein `messagebox.showinfo` bringt
    zwei Fehler auf einmal mit, und beide sind am 06.09.2026 aufgetreten:

    1. **Er sieht fremd aus** — heller Kasten mit Systemschrift mitten in einem
       dunklen Programm, und seine Knöpfe kommen in der Systemsprache, nicht in
       der eingestellten. („sieht kacke aus", „der Dialog zeigt im Deutschen
       englische Wörter")
    2. **Er erscheint irgendwo** — Tk setzt ihn nicht über das Elternfenster.
       Er landete unten am Bildschirmrand, wo niemand hinsieht, und weil er
       **modal** ist, nahm er die ganze Oberfläche mit: Die Gruppen in der
       Seitenleiste ließen sich nicht mehr auf- und zuklappen. Der Fehler sah
       nach einem kaputten Menü aus und war ein unsichtbares Fenster.
       („Fenster spawnt irgendwo unten, wo niemand hinschaut, was genau zu
       diesem Fehler geführt hat")

    `ask_yes_no` steht mittig über dem Elternfenster, trägt die Farben des
    Programms und benutzt die eingestellte Sprache.
    """
    from .main_window import ask_yes_no
    ask_yes_no(window.root, title, text, only_ok=True)


def _ask(window, title, text):
    """Eine Ja/Nein-Frage im Programmstil — siehe `_hinweis`."""
    from .main_window import ask_yes_no
    return ask_yes_no(window.root, title, text)


def _part_label(teil):
    """„C · Industrie" — Güte und Klasse eines Teils, kurz.

    ⭐⭐ **Das ist die Angabe, nach der ausgesucht wird.** Ein Schiff wird auf
    einen Zweck hin gebaut: Tarnung, Kampf, Bergbau. Wer die Namen nicht
    auswendig kennt — und das tut fast niemand —, sieht in einer reinen
    Namensliste nicht, was er da anklickt. Am 06.09.2026 gemeldet: „man sollte
    in der Liste sehen ob es grade A B oder C ist und ob Military oder was
    anderes."

    ⚠ **Die Klasse wird nachgeschlagen, wenn sie fehlt.** `cart.choices()`
    reichte sie bis v3.19.0 nicht durch; UEX führt sie neben der Güte. Sobald
    sie mitkommt, greift der direkte Weg und der Nachschlag entfällt von
    selbst — er steht hier, damit nicht zwei Stellen dieselbe Filterung
    doppeln.

    ⚠ Übersetzt wird über `KLASSEN_TEXTE`, wie überall sonst. Die **Güte**
    bleibt `A`/`B`/`C`/`D` — das ist im Spiel ein Buchstabe, kein Wort.
    """
    if not teil:
        return ''
    guete = (teil.get('guete') or '').strip()
    klasse = (teil.get('klasse') or '').strip()
    if (not guete or not klasse) and teil.get('kennung'):
        nach = _lookup_part(teil['kennung'])
        guete = guete or nach.get('guete') or ''
        klasse = klasse or nach.get('klasse') or ''
    teile = [guete] if guete else []
    if klasse:
        schluessel = CLASS_LABELS.get(klasse)
        teile.append(t(schluessel) if schluessel else klasse)
    # ⚠⚠ **Eine fehlende Klasse wird NICHT geraten.** Der Bauplan-Katalog
    # führt sie nur bei 240 von 738 Einträgen — der Militär-Antrieb Crossfield
    # steht ganz ohne da. „Civilian" als Standardwert wäre bei einem
    # Militärteil schlicht falsch, und niemand könnte es merken. Also lieber
    # eine Angabe weniger.
    #
    # ⭐ Dafür sagt die **Herkunft** an dieser Stelle oft mehr: Dass ein Teil
    # nur über einen Bauplan zu bekommen ist, ist genau die Auskunft, wegen
    # der jemand hier hinsieht — es steht in keinem Laden, egal wie lange man
    # sucht. Umgekehrt braucht „auch kaufbar" keinen Hinweis: Das ist der
    # Normalfall, und an jedem zweiten Teil stünde dasselbe Wort.
    from . import cart as _wk
    if (teil.get('herkunft') or '') == _wk.CRAFTABLE:
        teile.append(t('s_wk_nur_bauplan'))
    return ' · '.join(teile)


def _lookup_part(kennung):
    """Güte und Klasse eines Teils aus dem Laden-Katalog — über die Kennung.

    ⚠ Einmal je Programmlauf gebaut, nicht je Zeile: Der Katalog hat über
    1.600 Einträge, und die Auswahlliste zeichnet sich bei jedem Tastendruck
    neu. Ohne das Verzeichnis wäre jeder Buchstabe ein Durchlauf über alles.

    ⚠ **Beide Werte, nicht nur die Klasse.** In der Steckplatz-Zeile ist von
    einem Teil nur die Kennung bekannt (`werk.ref`) — dort fehlte sonst genau
    die Güte, die den Vergleich mit der Auswahl trägt.
    """
    if _PART_INDEX[0] is None:
        from . import shops
        try:
            _PART_INDEX[0] = dict(
                (x.get('kennung'), {'guete': x.get('guete') or '',
                                    'klasse': x.get('klasse') or ''})
                for x in shops.catalog_items() if x.get('kennung'))
        except Exception as ausnahme:
            fehler.merken('seiten.teil_nachschlagen', ausnahme)
            _PART_INDEX[0] = {}
    return _PART_INDEX[0].get(kennung) or {}


# Kennung -> {guete, klasse}, einmal je Programmlauf aufgebaut.
_PART_INDEX = [None]


def _slot_list(fenster, eltern, eintrag, daten, neu_zeichnen):
    """Die Steckplätze des Schiffs, jeder mit dem, was darin sitzt.

    ⚠ Gezeigt wird immer die **Werksausstattung** als Ausgangspunkt, auch wenn
    nichts geändert wurde. Ohne sie wäre nicht zu sehen, wogegen der Spieler
    tauscht — und „non-stock" wäre eine Behauptung ohne Bezugsgröße.
    """
    from . import cart, erkul, fleet as meine

    plaetze = erkul.hardpoints(eintrag.get('name') or '',
                                 eintrag.get('hersteller') or '',
                                 eintrag.get('kurz') or '',
                                 eintrag.get('hkurz') or '')
    gewaehlt = cart.loadout(eintrag)

    tk.Label(eltern, text=t('s_wk_auslegung'), bg=SURFACE, fg=FG,
             font=fenster.f_bold, anchor='w').pack(fill='x', padx=(46, 16),
                                                   pady=(0, 4))

    # ⚠⚠ **Gleiche Plätze werden gebündelt.** Eine Cutlass Black hat sechzehn
    # Raketenplätze, alle mit derselben Ignite II ab Werk — sechzehn identische
    # Zeilen untereinander sind keine Liste, sondern eine Wand. Rückmeldung am
    # 06.09.2026: „gleiche Raketen kann man zusammenfassen, sonst wird die
    # Liste zu lang."
    #
    # Gebündelt wird nur, was wirklich gleich ist: gleiche Art, gleiche Größe,
    # gleiches Teil ab Werk **und** dieselbe eigene Wahl. Sobald jemand einen
    # einzelnen Platz anders belegt, löst er sich aus der Gruppe und steht für
    # sich — sonst verschwände seine Abweichung hinter einem „16x".
    for gruppe in _bundle_slots(plaetze, gewaehlt):
        _slot_row(fenster, eltern, eintrag, gruppe[0], gewaehlt,
                          neu_zeichnen, gruppe=gruppe)


def _bundle_slots(plaetze, gewaehlt):
    """Gleichartige Steckplätze zusammenfassen; gibt Gruppen zurück.

    Die Reihenfolge bleibt erhalten: Die Gruppe steht dort, wo ihr erster Platz
    stand. Wer die Liste zweimal öffnet, findet dieselbe Anordnung.
    """
    gruppen = []
    nach_schluessel = {}
    for platz in plaetze:
        werk = platz.get('werk') or {}
        eigen = gewaehlt.get(platz.get('pfad') or '') or {}
        schluessel = (platz.get('art'), platz.get('groesse'),
                      werk.get('ref'), eigen.get('ref'))
        if schluessel in nach_schluessel:
            nach_schluessel[schluessel].append(platz)
        else:
            neue = [platz]
            nach_schluessel[schluessel] = neue
            gruppen.append(neue)
    return gruppen


def _slot_row(fenster, eltern, eintrag, platz, gewaehlt,
                      neu_zeichnen, gruppe=None):
    """Ein Steckplatz — anklickbar, die Teileauswahl klappt darunter auf.

    ⚠ Kein eigenes Fenster für die Auswahl: Wer vier Plätze nacheinander
    belegt, müsste sonst viermal ein Fenster öffnen und schließen. Aufgeklappt
    wird dieselbe Formensprache benutzt wie beim Handeintrag darüber.
    """
    from . import cart, fleet as meine

    pfad = platz.get('pfad') or ''
    # ⚠⚠ **Eine Wahl gilt für die ganze Gruppe.** Die Zeile vertritt bei
    # gebündelten Plätzen mehrere — wer bei „16 x Missile S2" ein Teil wählt,
    # meint alle sechzehn. Würde nur der erste belegt, zerfiele die Gruppe beim
    # nächsten Zeichnen in „1 x geändert" und „15 x ab Werk", und niemand
    # verstünde, warum.
    alle_pfade = [(g.get('pfad') or '') for g in (gruppe or [platz])]
    zeile = tk.Frame(eltern, bg=SURFACE, cursor='hand2')
    zeile.pack(fill='x', padx=(46, 16), pady=1)

    # Links: was für ein Platz das ist. Der Pfad wird **nicht** angezeigt — er
    # ist eine Kennung, keine Beschriftung, und `hardpoint_Left_Pylon_03` sagt
    # niemandem etwas.
    # ⚠⚠ **Ein Pfeil, sonst findet niemand die Funktion.** Die Zeile klappt auf
    # Klick eine Teileauswahl auf — erkennbar war das nur am Mauszeiger, wenn
    # man zufällig darüberfuhr. Am 06.09.2026 gefragt: „wo würde man die Plätze
    # eigentlich belegen?" Wenn der Entwickler die eigene Funktion nicht
    # findet, findet sie niemand.
    #
    # Überall sonst im Programm steht an aufklappbaren Zeilen dieser Pfeil.
    pfeil = icons.line(zeile, 'aufklappen', background=SURFACE,
                          font=fenster.f_small)
    pfeil.pack(side='left', padx=(0, 6))

    art_text = platz.get('art') or ''
    if platz.get('groesse') is not None:
        art_text = '%s S%s' % (art_text, platz['groesse'])
    # Die Anzahl steht vorn, nicht hinten: So sieht man beim Überfliegen der
    # linken Spalte sofort, wie viele Plätze eine Zeile vertritt.
    anzahl = len(gruppe) if gruppe else 1
    if anzahl > 1:
        art_text = '%d x %s' % (anzahl, art_text)
    tk.Label(zeile, text=art_text, bg=SURFACE, fg=SUB, font=fenster.f_small,
             anchor='w', width=22).pack(side='left')

    eigenes = gewaehlt.get(pfad) or {}
    werk = platz.get('werk') or {}
    if eigenes.get('ref') and eigenes['ref'] != werk.get('ref'):
        text, farbe, kennung = eigenes.get('name') or '', ACCENT, eigenes['ref']
    elif werk.get('name'):
        text, farbe, kennung = werk['name'], SUB, werk.get('ref') or ''
    else:
        text, farbe, kennung = t('s_wk_ab_werk_leer'), SUB, ''
    tk.Label(zeile, text=text, bg=SURFACE, fg=farbe, font=fenster.f_small,
             anchor='w').pack(side='left')

    # ⚠ **Auch hier Güte und Klasse** — nicht nur in der Auswahlliste. Sonst
    # sieht man zwar, dass die Auswahl ein „A · Tarnung" anbietet, aber nicht,
    # dass ab Werk längst ein A drinsteckt. Ohne den Vergleich ist die eine
    # Angabe die Hälfte einer Auskunft.
    kennzeichen = _part_label({'kennung': kennung}) if kennung else ''
    if kennzeichen:
        tk.Label(zeile, text=kennzeichen, bg=SURFACE, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(side='left',
                                                        padx=(10, 0))

    def speichern():
        _save_entry(eintrag)

    if eigenes.get('ref'):
        # ⚠⚠ **Diese Funktion darf nicht schlicht „zurücksetzen" heißen.**
        # Prüfung 93 sucht in dieser Datei die **letzte** so benannte Funktion
        # und erwartet dahinter den Bestand-Knopf mit seinen beiden Meldungen.
        # Eine gleichnamige Funktion weiter unten übernimmt diese Rolle
        # stillschweigend, und die Prüfung meldet dann einen Fehler an einer
        # Stelle, an der niemand etwas geändert hat.
        #
        # ⚠ Und der Name darf hier auch nicht als Beispiel ausgeschrieben
        # stehen: Die Prüfung liest den Quelltext, nicht den Code — ein
        # Kommentar mit dem Suchmuster darin löst sie genauso aus. Beim ersten
        # Anlauf hat genau die Warnung vor der Falle die Falle ausgelöst.
        def platz_zuruecksetzen():
            geaendert = False
            for einer in alle_pfade:
                if cart.clear_part(eintrag, einer):
                    geaendert = True
            if geaendert:
                speichern()
                neu_zeichnen()
        _button(fenster, zeile, t('s_wk_zuruecksetzen'),
               platz_zuruecksetzen).pack(side='right')

    auswahl_rahmen = tk.Frame(eltern, bg=SURFACE)
    gebaut = []

    def aufbauen():
        if gebaut:
            return
        gebaut.append(True)
        moeglich = cart.choices(platz.get('art'), platz.get('groesse'))
        if not moeglich:
            # ⚠ Ehrlich statt hübsch: Wenn zu diesem Platz keine kaufbaren
            # Teile bekannt sind, wird das gesagt — nicht der halbe Katalog
            # angeboten, aus dem nichts passt.
            _body_text(auswahl_rahmen, t('s_wk_kein_preis'), fenster.f_small,
                        bg=SURFACE, fill='x', padx=(22, 0), inset=90)
            return
        nach_name = dict((m['name'], m) for m in moeglich)
        gewaehlt_var = tk.StringVar()

        def uebernehmen(*args):
            # ⚠⚠ **Der Name kommt als Argument, nicht aus dem Feld.** Wer
            # `beim_waehlen` an `_auswahlfeld` übergibt, bekommt den Namen
            # gereicht — das Eingabefeld wird dann absichtlich **nicht**
            # befüllt (siehe `waehlen()` dort, der Verkaufs-Reiter braucht das
            # so). Wer trotzdem aus dem Feld liest, liest eine leere
            # Zeichenkette: Der Klick auf ein Teil tat schlicht nichts, ohne
            # Fehler und ohne Meldung. Am 06.09.2026 gemeldet mit „klick ich
            # was an, wird es nicht eingefügt".
            #
            # `args` ist leer, wenn die Eingabetaste bestätigt — dann gilt das
            # Feld.
            gewaehlt_name = (args[0] if args and isinstance(args[0], str)
                             else gewaehlt_var.get() or '')
            m = nach_name.get(gewaehlt_name.strip())
            if not m:
                return
            geaendert = False
            for einer in alle_pfade:
                if cart.set_part(eintrag, einer, m['kennung'], m['name']):
                    geaendert = True
            if geaendert:
                speichern()
                neu_zeichnen()

        feld, liste, _ = _combo_box(
            fenster, auswahl_rahmen, gewaehlt_var,
            lambda: sorted(nach_name),
            on_pick=uebernehmen, on_confirm=uebernehmen,
            empty_text=t('s_hg_nichts_gefunden'), scrollable=200,
            extra=lambda n: _part_label(nach_name.get(n)))
        feld.pack(fill='x', padx=(22, 0))
        liste.pack(fill='x', padx=(22, 0))

    def umschalten(_=None):
        if auswahl_rahmen.winfo_ismapped():
            auswahl_rahmen.pack_forget()
            pfeil.swap_symbol('aufklappen')
        else:
            aufbauen()
            auswahl_rahmen.pack(fill='x', padx=(46, 16), pady=(0, 6),
                                after=zeile)
            # ⚠ Der Pfeil zeigt den **Zustand**, nicht die Tat: nach unten
            # heißt „ist offen", nach rechts „ist zu". Andersherum gelesen
            # wäre er eine Aufforderung und damit immer verkehrt herum.
            pfeil.swap_symbol('zuklappen')

    for teil in (zeile,) + tuple(zeile.winfo_children()):
        if isinstance(teil, tk.Label):
            teil.bind('<Button-1>', umschalten)
    zeile.bind('<Button-1>', umschalten)


def _save_entry(eintrag):
    """Diesen Eintrag in die Datei zurückschreiben — Hangar **oder** Wunsch.

    ⚠⚠⚠ **Der Vorgänger hat Daten vernichtet.** `_hangar_liste()` gab nur die
    Schiffsliste zurück, und gespeichert wurde damit
    `{'format': …, 'schiffe': …}` — **ohne `wunsch`**. Jedes Mal, wenn jemand
    an der Ausstattung eines Schiffs etwas änderte, war die komplette
    Wunschliste weg. Am 06.09.2026 gemeldet: „gebe ich ein Schiff auf der
    Wunschliste ein, bleibt es nur so lange stehen, bis ich Komponenten dazu
    eintrage."

    Dazu kam der zweite Teil desselben Fehlers: Ein **Wunsch**-Eintrag wurde in
    `schiffe` gesucht, dort nie gefunden — seine Änderung ging also ebenfalls
    verloren.

    ⚠ Gespeichert wird deshalb der **geladene Gesamtstand** mit dem
    ausgetauschten Eintrag. Wer eine Teilmenge schreibt, löscht den Rest; das
    ist bei einer Datei, in der zwei Listen stehen, keine Frage des Stils.
    """
    from . import fleet as meine
    stand = meine.load()
    name = eintrag.get('name')
    hersteller = eintrag.get('hersteller')
    gefunden = False
    for schluessel in ('schiffe', 'wunsch'):
        for i, s in enumerate(stand.get(schluessel) or []):
            if (s.get('name') == name
                    and s.get('hersteller') == hersteller):
                stand[schluessel][i] = eintrag
                gefunden = True
                break
        if gefunden:
            break
    meine.save(stand)
    return gefunden


def _installed_items(fenster, eltern, eintrag, fertig, neu_zeichnen):
    """Was eingebaut ist: eine Zeile, auf Klick die Liste.

    ⚠ Zugeklappt, weil es die häufigere Lage ist — wer am Schiff arbeitet,
    will sehen, was noch fehlt. Aufgeklappt steht jeder Posten mit seinem
    Haken da und lässt sich wieder öffnen.
    """
    kasten = tk.Frame(eltern, bg=SURFACE)
    kasten.pack(fill='x', padx=(46, 16), pady=(6, 0))

    kopf = tk.Frame(kasten, bg=SURFACE, cursor='hand2')
    kopf.pack(fill='x')
    pfeil = icons.line(kopf, 'aufklappen', background=SURFACE,
                          font=fenster.f_small)
    pfeil.pack(side='left', padx=(0, 8))
    tk.Label(kopf, text=t('s_wk_eingebaut_n').format(n=len(fertig)),
             bg=SURFACE, fg=ACCENT, font=fenster.f_small,
             anchor='w').pack(side='left')

    koerper = tk.Frame(kasten, bg=SURFACE)

    def umschalten(_=None):
        if koerper.winfo_ismapped():
            koerper.pack_forget()
            pfeil.swap_symbol('aufklappen')
        else:
            for kind in koerper.winfo_children():
                kind.destroy()
            for posten in fertig:
                _cart_item(fenster, koerper, eintrag, posten,
                                  neu_zeichnen, eingerueckt=False)
            koerper.pack(fill='x', after=kopf)
            pfeil.swap_symbol('zuklappen')

    for teil in (kopf, pfeil) + tuple(kopf.winfo_children()):
        teil.bind('<Button-1>', umschalten)


    # ⭐ Der Pfeil hebt sich ab, wenn die Maus die Kopfzeile
    # trifft — anklickbar ist hier die Zeile, nicht der Pfeil.
    icons.hover_group(kopf, pfeil)

def _cart_item(fenster, eltern, eintrag, posten, neu_zeichnen,
                      eingerueckt=True):
    """Ein Posten mit **beiden** Wegen nebeneinander — kaufen und bauen.

    ⭐⭐ **Das ist der Punkt, an dem dieses Werkzeug mehr kann als jede
    Ausstattungs-Seite im Netz:** Es kennt die Baupläne des Spielers. Also steht
    hier nicht ein Preis, sondern beide Wege — und die Wahl trifft der Spieler,
    Posten für Posten. Wer gerade kein Erz hat, kauft trotz des besseren
    Preises; wer Zeit hat, baut.
    """
    from . import cart, fleet as meine

    karte = tk.Frame(eltern, bg='#0c1017')
    karte.pack(fill='x', padx=((46, 16) if eingerueckt else (0, 0)),
               pady=(0, 6))

    fertig = bool(posten.get('erledigt'))
    kopf = tk.Frame(karte, bg='#0c1017')
    kopf.pack(fill='x', padx=12, pady=(8, 2))

    # ⭐⭐ **Der Haken sitzt am Posten selbst, nicht nur auf der Sammelliste.**
    # Am 06.09.2026 dazu: „es muss auch anklickbar sein, ob eine Komponente
    # schon eingebaut ist oder noch gekauft oder hergestellt werden muss."
    # Genau hier steht man vor dem Schiff und sieht seine Plätze — hier fällt
    # einem ein, dass das Teil längst drin ist, nicht zwei Reiter weiter.
    def abhaken(_e=None):
        if cart.set_done(eintrag, posten['pfad'], not fertig):
            _save_entry(eintrag)
            neu_zeichnen()

    haken = icons.line(kopf, 'haken', background='#0c1017',
                          color=icons.GREEN if fertig else icons.GREY,
                          font=fenster.f_small)
    haken.configure(cursor='hand2')
    haken.pack(side='left', padx=(0, 8))
    haken.bind('<Button-1>', abhaken)

    # ⚠ Erledigtes tritt zurück, bleibt aber lesbar — wer versehentlich
    # abhakt, muss die Zeile wiederfinden.
    tk.Label(kopf, text=posten.get('name') or '', bg='#0c1017',
             fg=SUB if fertig else FG,
             font=fenster.f_bold, anchor='w', cursor='hand2').pack(side='left')
    # Wogegen getauscht wird — ohne diese Angabe ist „non-stock" eine
    # Behauptung ohne Bezugsgröße.
    if fertig:
        hinweis = t('s_wk_eingebaut')
    elif posten.get('werk_name'):
        hinweis = t('s_wk_statt').format(name=posten['werk_name'])
    else:
        hinweis = t('s_wk_zusaetzlich')
    tk.Label(kopf, text=hinweis, bg='#0c1017',
             fg=ACCENT if fertig else SUB, font=fenster.f_small,
             anchor='w').pack(side='left', padx=(8, 0))
    for teil in kopf.winfo_children():
        teil.bind('<Button-1>', abhaken)

    # ⚠ Ist der Posten erledigt, stehen die beiden Wege nicht mehr da: Die
    # Frage „kaufen oder bauen" ist beantwortet, sobald das Teil drin ist.
    if fertig:
        tk.Frame(karte, bg='#0c1017', height=6).pack()
        return

    def waehlen(weg):
        def tat():
            if cart.set_method(eintrag, posten['pfad'], weg):
                _save_entry(eintrag)
                neu_zeichnen()
        return tat

    for weg, schluessel, zustandsfeld in ((cart.BUY, 's_wk_kaufen',
                                           'kauf'),
                                          (cart.CRAFT, 's_wk_bauen',
                                           'bau')):
        angabe = posten.get(zustandsfeld) or {}
        zeile = tk.Frame(karte, bg='#0c1017')
        zeile.pack(fill='x', padx=12, pady=(0, 4))

        # ⚠ Der gewählte Weg ist in der Markenfarbe hervorgehoben — nicht durch
        # einen Schiebeschalter, der bei zwölf Posten zwölfmal dastünde.
        aktiv = posten.get('weg') == weg
        tk.Label(zeile, text=t(schluessel), bg='#0c1017',
                 fg=ACCENT if aktiv else SUB,
                 font=fenster.f_bold if aktiv else fenster.f_small,
                 anchor='w', width=18).pack(side='left')

        if angabe.get('zustand') == cart.KNOWN:
            if weg == cart.BUY:
                # ⚠⚠ **Der Ort steht oft schon im Ladennamen.** UEX schreibt
                # ihn dort mit hinein: Laden „Ship Weapons - Pyro Gateway
                # (Stanton)", Ort „Pyro Gateway (Stanton)". Beides
                # aneinandergehängt ergab „… bei Ship Weapons - Pyro Gateway
                # (Stanton) · Pyro Gateway (Stanton)" — derselbe Ort zweimal,
                # und die Zeile so lang, dass sie den Knopf daneben aus dem
                # Fenster geschoben hat.
                laden = (angabe.get('laden') or '').strip()
                ort = (angabe.get('ort') or '').strip()
                stellen = [laden] if laden else []
                if ort and ort.lower() not in laden.lower():
                    stellen.append(ort)
                text = t('s_wk_kauf_preis').format(
                    preis=_money(angabe.get('preis')),
                    laden=' · '.join(stellen) or '?', ort='')
                text = text.rstrip(' ·')
            else:
                text = t('s_wk_bau_kosten').format(
                    preis=_money(angabe.get('material')),
                    dauer=_duration(angabe.get('dauer')))
            farbe = FG
        elif angabe.get('zustand') == cart.NO_RECIPE:
            text, farbe = t('s_wk_kein_rezept'), SUB
        elif angabe.get('zustand') == cart.NO_PRICE:
            text, farbe = t('s_wk_kein_preis'), SUB
        else:
            text, farbe = t('s_wk_nicht_geprueft'), SUB
        # ⚠⚠ **Ein fehlender Preis ist kein fehlender Weg.** Bis zum 06.09.2026
        # stand der Knopf nur bei `BEKANNT` — wer einmal auf „Selbst
        # herstellen" gewechselt hatte, kam nicht mehr zurück, solange UEX
        # keinen Preis führte. Gefragt wurde: „wie wähle ich Kaufen
        # überhaupt aus?" Die richtige Antwort war: gar nicht.
        #
        # | Zustand | heißt | Knopf |
        # |---|---|---|
        # | `KEIN_REZEPT` | du hast den Bauplan nicht | **nein** — harte Tatsache über dein Lager |
        # | `KEIN_PREIS` | UEX führt keinen Preis | ja — im Spiel kaufbar bleibt es trotzdem |
        # | `NICHT_GEPRUEFT` | niemand hat nachgesehen | ja — Nichtwissen verbietet nichts |
        #
        # Der Unterschied ist derselbe wie überall hier: Eine Aussage über
        # **fremde Daten** darf nie zu einer Aussage über das Spiel werden.
        #
        # ⚠⚠ **Der Knopf wird VOR dem Text gepackt — das ist kein Stil, sondern
        # der Fehler selbst.** In `tkinter` bekommt das zuerst gepackte Element
        # seinen Platz; ein langer Text mit `side='left'` schiebt einen später
        # gepackten `side='right'`-Knopf schlicht aus dem Fenster. Genau das ist
        # passiert: Bei einem Laden mit langem Namen war der Knopf „Kaufen"
        # unsichtbar, und wer einmal auf „Selbst herstellen" gewechselt hatte,
        # kam nicht zurück. Am 06.09.2026 gemeldet — und dieselbe Falle steht
        # schon zweimal in den Projektregeln.
        if not aktiv and angabe.get('zustand') != cart.NO_RECIPE:
            _button(fenster, zeile, t(schluessel),
                   waehlen(weg)).pack(side='right', padx=(8, 0))

        # ⚠ Der Text kommt zuletzt und darf schrumpfen: Was nicht passt, wird
        # abgeschnitten — der Knopf daneben bleibt.
        tk.Label(zeile, text=text, bg='#0c1017', fg=farbe,
                 font=fenster.f_small, anchor='w').pack(side='left',
                                                        fill='x', expand=True)

        # ⚠⚠ Ein Rohstoff ohne Kaufpreis ist nicht kostenlos, sondern nicht
        # kaufbar. Ohne diesen Satz sieht Selberbauen billiger aus, als es ist.
        if weg == cart.CRAFT and angabe.get('ohne_preis'):
            _body_text(karte,
                        t('s_wk_ohne_preis').format(
                            rohstoffe=', '.join(angabe['ohne_preis'])),
                        fenster.f_small, color=GOLD, bg='#0c1017',
                        fill='x', padx=12, pady=(0, 6), inset=90)

    tk.Frame(karte, bg='#0c1017', height=4).pack()


def _cart_total(fenster, eltern, liste):
    """Was der Warenkorb kostet — nach der getroffenen Wahl."""
    from . import cart
    zahlen = cart.total(liste)

    kasten = tk.Frame(eltern, bg=SURFACE)
    kasten.pack(fill='x', padx=(46, 16), pady=(4, 0))
    tk.Label(kasten, text=t('s_wk_summe').format(
        preis=_money(zahlen['gesamt'])), bg=SURFACE, fg=ACCENT,
        font=fenster.f_bold, anchor='w').pack(fill='x')

    if zahlen['kaufen'] and zahlen['bauen']:
        tk.Label(kasten, text=t('s_wk_summe_teil').format(
            kaufen=_money(zahlen['kaufen']), bauen=_money(zahlen['bauen'])),
            bg=SURFACE, fg=SUB, font=fenster.f_small,
            anchor='w').pack(fill='x')
    if zahlen['dauer']:
        tk.Label(kasten, text=t('s_wk_bauzeit').format(
            dauer=_duration(zahlen['dauer'])), bg=SURFACE, fg=SUB,
            font=fenster.f_small, anchor='w').pack(fill='x')
    # ⚠ Eine Summe, der Posten fehlen, sieht aus wie eine vollständige.
    if zahlen['offen']:
        tk.Label(kasten, text=t('s_wk_summe_offen').format(n=zahlen['offen']),
                 bg=SURFACE, fg=GOLD, font=fenster.f_small,
                 anchor='w').pack(fill='x')


def _cart_route(fenster, eltern, liste):
    """Die Einkaufsroute für alles, was gekauft wird."""
    from . import cart
    stopps, ohne = cart.route(liste)

    tk.Label(eltern, text=t('s_wk_route'), bg=SURFACE, fg=FG,
             font=fenster.f_bold, anchor='w').pack(fill='x', padx=(46, 16),
                                                   pady=(10, 2))
    if not stopps:
        _body_text(eltern, t('s_wk_route_leer'), fenster.f_small,
                    bg=SURFACE, fill='x', padx=(46, 16), inset=78)
        return

    zahlen = cart.route_total(stopps)
    # ⚠⚠ **„Auf dieser Route", nicht „Summe".** Die Zahl oben rechnet mit dem
    # billigsten Laden im ganzen Verse, diese mit den Läden, die auf der Route
    # wirklich liegen. Beide sind richtig und meinen Verschiedenes —
    # unbeschriftet nebeneinander sähe es aus, als rechne das Werkzeug falsch.
    # ⚠ Läden und Stopps werden **einzeln** gebeugt. Zwei Läden an einem Ort
    # sind „2 Läden · 1 Stopp" — ein gemeinsamer Mehrzahl-Satz schrieb hier
    # „1 Stopps".
    zahl_text = '%s · %s' % (
        t('s_wk_laden' if zahlen['laeden'] == 1
          else 's_wk_laeden').format(n=zahlen['laeden']),
        t('s_wk_stopp' if zahlen['stopps'] == 1
          else 's_wk_stopps').format(n=zahlen['stopps']))
    tk.Label(eltern, text='%s  ·  %s' % (
        t('s_wk_route_summe').format(preis=_money(zahlen['gesamt'])),
        zahl_text),
        bg=SURFACE, fg=SUB, font=fenster.f_small, anchor='w').pack(
            fill='x', padx=(46, 16))

    for stopp in stopps:
        zeile = tk.Frame(eltern, bg=SURFACE)
        zeile.pack(fill='x', padx=(58, 16), pady=(4, 0))
        wo = ' · '.join(x for x in (stopp['system'], stopp['ort']) if x)
        tk.Label(zeile, text=wo, bg=SURFACE, fg=FG, font=fenster.f_small,
                 anchor='w').pack(side='left')
        tk.Label(zeile, text=_auec(stopp['summe']), bg=SURFACE, fg=SUB,
                 font=fenster.f_small, anchor='e').pack(side='right')
        for eintrag_posten in stopp['posten']:
            unter = tk.Frame(eltern, bg=SURFACE)
            unter.pack(fill='x', padx=(70, 16))
            tk.Label(unter, text='%s — %s' % (eintrag_posten['name'],
                                              eintrag_posten['laden']),
                     bg=SURFACE, fg=SUB, font=fenster.f_small,
                     anchor='w').pack(side='left')
            tk.Label(unter, text=_money(eintrag_posten['preis']), bg=SURFACE,
                     fg=SUB, font=fenster.f_small,
                     anchor='e').pack(side='right')

    if ohne:
        _body_text(eltern, t('s_wk_route_ohne').format(n=len(ohne)),
                    fenster.f_small, color=GOLD, bg=SURFACE, fill='x',
                    padx=(46, 16), pady=(6, 0), inset=78)


def _storage(fenster, rahmen):
    """Das eigene Rohstoff-Lager: eintragen, ansehen, löschen."""
    from . import materials as lager
    _heading(fenster, rahmen, t('hf_lager'), t('s_lg_lead'))
    innen = _scroll_area(rahmen)

    _body_text(innen, t('s_lg_hinweis'), fenster.f_small, fill='x')

    from .main_window import round_entry
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
                              font=fenster.f_bold, anchor='w')
        kopf_label.pack(fill='x')

        if var is material or var is ort:
            # ⭐ Auswahlfeld: tippen **oder** den Pfeil anklicken und aussuchen.
            # Ohne Vorschläge tippt jemand „Aslerite", bekommt nie einen
            # Treffer und sucht den Fehler bei sich.
            if var is material:
                from . import crafting as _h_lg

                def _quelle_material():
                    try:
                        return sorted(_h_lg.storable())
                    except Exception:
                        return []
                quelle = _quelle_material
            else:
                from . import places as _o_lg
                quelle = _o_lg.all_places
            zeile_, liste_, zeichnen_ = _combo_box(fenster, block, var,
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
            f = round_entry(_mengenzeile, var, fenster.f_small, '#0c1017',
                            LINE, ACCENT, FG)
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
            _checkbox(_mengenzeile, t('s_lg_cscu'), cscu, einheit_um,
                       fenster.f_small).pack(side='right', padx=(10, 0))
            f.holder.pack(side='left', fill='both', expand=True)
            if cscu[0]:
                kopf_label.configure(text=t('s_lg_menge_cscu'))
            # ⭐⭐ **Die Vorschau ist die eigentliche Erklärung.** Wer beim
            # Tippen von „1.04+3" daneben „ergibt 4,04 SCU" liest, braucht
            # keinen Satz über Auf- und Abbuchen mehr.
            mengen_vorschau = tk.Label(block, text='', bg=BG, fg=ACCENT,
                                       font=fenster.f_small, anchor='w')
            mengen_vorschau.pack(fill='x')
        else:
            f = round_entry(block, var, fenster.f_small, '#0c1017', LINE,
                            ACCENT, FG)
            f.holder.pack(fill='x', pady=(4, 0))

    # ℹ Die früheren „Meintest du:"-Zeilen für Rohstoff und Lagerort sind
    # entfallen: Das Auswahlfeld filtert beim Tippen selbst und zeigt auf
    # Knopfdruck die ganze Liste. Zwei Wege für dieselbe Hilfe nebeneinander
    # wären eine Bedienung zu viel.

    def _bestand_vorher():
        """Wie viel im gerade bearbeiteten Posten liegt — sonst 0."""
        nr = bearbeitung['nummer']
        if nr is None:
            return 0.0
        posten = lager.load()
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
        wert = lager.calculate(roh, vorher)
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
    meldung = tk.Label(innen, text='', bg=BG, fg=SUB, font=fenster.f_small,
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
            from . import mining as berg
            arten = berg.mining_kinds(material)
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
        posten = lager.load()
        if not posten:
            _body_text(liste_rahmen, t('s_lg_leer'), fenster.f_small, fill='x')
            return

        arten = len({(p.get('material') or '').lower() for p in posten})
        summe_txt = (t('s_lg_summe_eins') % len(posten) if arten == 1
                     else t('s_lg_summe') % (len(posten), arten))
        tk.Label(liste_rahmen, text=summe_txt, bg=BG, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(fill='x', pady=(0, 6))

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
                           font=fenster.f_small, width=breite, anchor=anker_,
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
            _body_text(liste_rahmen, t('s_lg_nichts_da'), fenster.f_small,
                        fill='x')
            return

        for nummer, p in sichtbar:
            offen = bearbeitung['nummer'] == nummer
            # Die offene Zeile bekommt Flaeche unter sich, damit man sieht,
            # welchen Posten die Felder oben gerade zeigen.
            z_bg = SURFACE if offen else BG
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
                           font=fenster.f_small, cursor='hand2', anchor='e')
            weg.pack(side='right', padx=(8, 4))
            # ⚠ Rollstelle halten — sonst springt die Seite beim Löschen nach
            # ganz oben, und wer beim zwölften Posten war, sucht sich neu
            # zurecht (30.08.2026 gemeldet).
            weg.bind('<Button-1>',
                     lambda _e, n=nummer: _keep_scroll(
                         weg, lambda: (lager.remove(n),
                                       verwerfen(), zeichnen())))
            # ⭐ „Löschen" ist ein Wort, kein Symbol — es hebt sich über
            # die Schriftfarbe ab. ⚠ Rot, nicht Markenfarbe: Die
            # Warnung gehört zur Aussage.
            icons.hover_fg(weg, SUB, RED_PALE)

            spalten_labels = []
            for wert, (_k, _tk, breite, anker_), farbe, schrift in (
                    (name_txt, SPALTEN[0], FG, fenster.f_base),
                    (menge_txt, SPALTEN[1], ACCENT, fenster.f_base),
                    (q_txt, SPALTEN[2], SUB, fenster.f_small),
                    (abbau_txt, SPALTEN[3], SUB, fenster.f_small),
                    (ort_txt, SPALTEN[4], SUB, fenster.f_small)):
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
        posten = lager.load()
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
        from .main_window import ask_yes_no
        nummer = bearbeitung['nummer']
        if nummer is None:
            return
        alle = lager.load()
        if not (0 <= nummer < len(alle)):
            return
        p_ = alle[nummer]
        if not ask_yes_no(fenster.root, t('s_lg_posten_frage_t'),
                             t('s_lg_posten_frage') % (p_.get('material') or '?',
                                                       float(p_.get('menge') or 0))):
            return
        _keep_scroll(innen, lambda: (lager.remove(nummer),
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
        from . import crafting as h_modul
        richtig = h_modul.storage_name(name)
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
            # Spieldaten (`crafting.storable()`). Fehlt etwas, wird die
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
        # natuerliche Eingabe wurde abgelehnt. `lager.calculate()` kann jetzt
        # beides und liefert direkt die **neue Menge**.
        roh = (menge.get() or '0').strip()
        vorher_menge = _bestand_vorher()
        rechnend = bool(roh) and (roh[:1] in '+-−'
                                  or any(z in roh[1:] for z in '+-−'))
        wert = lager.calculate(roh, vorher_menge)
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
                lager.remove(nr)
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
        from . import places as orte_modul
        ort_richtig = orte_modul.official_name(ort.get())
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
        q_zahl = lager.parse_number(guete.get())
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
            lager.add(name, wert, q, ort.get())
            hinweis = t('s_lg_eingetragen') % (name, wert)
        else:
            lager.change(bearbeitung['nummer'], name, wert, q, ort.get())
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
                             font=fenster.f_small, anchor='w', justify='left')
    rechenhinweis.pack(fill='x', pady=(0, 4))

    knopf_rahmen = tk.Frame(innen, bg=BG)

    def knoepfe_setzen():
        for w in knopf_rahmen.winfo_children():
            w.destroy()
        if bearbeitung['nummer'] is None:
            _button(fenster, knopf_rahmen, t('s_lg_eintragen'),
                   eintragen).pack(side='left')
        else:
            _button(fenster, knopf_rahmen, t('s_lg_speichern'), eintragen,
                   strong=True).pack(side='left')
            _button(fenster, knopf_rahmen, t('s_lg_abbrechen'),
                   verwerfen).pack(side='left', padx=(8, 0))
            # ⭐ Löschen genau dieses Postens — man hat ihn ja gerade offen.
            # Das „Löschen" an der Zeile bleibt daneben bestehen; hier ist es
            # der Weg für den, der schon in der Bearbeitung steckt.
            _button(fenster, knopf_rahmen, t('s_lg_posten_weg'), posten_weg,
                   danger=True).pack(side='left', padx=(24, 0))

    knoepfe_setzen()
    knopf_rahmen.pack(anchor='w', pady=(4, 10))
    meldung.pack(fill='x')

    _refinery_box(fenster, innen, lager, ort, zeichnen, meldung)
    # ⚠⚠ **Das Suchfeld wird EINMAL gebaut — nicht in `zeichnen()`.** Dort
    # stand es bis rc28, und `zeichnen()` räumt bei jeder Änderung den ganzen
    # Listenbereich leer: Mit jedem getippten Buchstaben zerstörte sich das
    # Feld selbst, der Tastaturfokus ging verloren, und man musste für den
    # nächsten Buchstaben neu hineinklicken. Am 30.08.2026 gemeldet: „im Lager
    # bei Eingabe im Suchfeld tabt man automatisch raus".
    #
    # Alles, woran ein Cursor stehen kann, gehört ausserhalb der Zeichenroutine.
    from .main_window import round_entry as _rf_suche
    _such_zeile = tk.Frame(innen, bg=BG)
    _such_zeile.pack(fill='x', pady=(6, 0))
    tk.Label(_such_zeile, text=t('s_lg_filter'), bg=BG, fg=SUB,
             font=fenster.f_small).pack(side='left', padx=(0, 10))
    _such_feld = _rf_suche(_such_zeile, filter_var, fenster.f_small,
                           '#0c1017', LINE, ACCENT, FG)
    _such_feld.holder.pack(side='left', fill='x', expand=True)
    _search_clear(fenster, _such_zeile, filter_var)

    liste_rahmen.pack(fill='both', expand=True, pady=(6, 0))

    # --- Sichern und zurueckholen --------------------------------------
    # ⚠ Das Lager wird von Hand gepflegt — es ist Arbeit, die sonst nirgends
    # liegt. Ohne Ausgabe ist sie beim naechsten Rechnerwechsel weg.
    def _ausgeben(art):
        from . import file_picker
        endung = '.csv' if art == 'csv' else '.json'
        ziel = file_picker.save_file(
            t('s_lg_ausgeben'),
            suggestion='lager-%s%s' % (time.strftime('%Y-%m-%d'), endung),
            extension=endung, start=None)
        if not ziel:
            return
        try:
            inhalt = (lager.as_csv() if art == 'csv' else lager.as_json())
            with open(ziel, 'w', encoding='utf-8') as f:
                f.write(inhalt)
            meldung.configure(text=t('s_lg_gespeichert') % os.path.basename(ziel),
                              fg=SUB)
        except Exception as ausnahme:
            fehler.merken('seiten.lager.ausgeben', ausnahme)

    def _einlesen():
        from . import file_picker
        quelle = file_picker.open_file(t('s_lg_einlesen'))
        if not quelle:
            return
        try:
            with open(quelle, encoding='utf-8') as f:
                posten = lager.from_json(f.read())
        except Exception as ausnahme:
            fehler.merken('seiten.lager.einlesen', ausnahme)
            posten = None
        if posten is None:
            # ⚠ Nicht schweigen. Wer eine falsche Datei waehlt und nichts
            # passieren sieht, haelt das Einlesen fuer kaputt.
            meldung.configure(text=t('s_lg_datei_falsch'), fg=GOLD)
            return
        lager.save(posten)
        verwerfen()
        meldung.configure(text=t('s_lg_eingelesen') % len(posten), fg=SUB)
        zeichnen()

    _reihe_aus = tk.Frame(innen, bg=BG)
    _reihe_aus.pack(fill='x', pady=(14, 0))
    _button(fenster, _reihe_aus, t('s_lg_aus_json'),
           lambda: _ausgeben('json')).pack(side='left')
    _button(fenster, _reihe_aus, t('s_lg_aus_csv'),
           lambda: _ausgeben('csv')).pack(side='left', padx=(8, 0))
    _button(fenster, _reihe_aus, t('s_lg_einlesen'),
           lambda: _einlesen()).pack(side='left', padx=(8, 0))

    def _leeren():
        """Das ganze Lager verwerfen — nach Rückfrage.

        ⚠ Rot **und** mit Frage. Das Lager ist Handarbeit, die sonst nirgends
        liegt: kein Log, keine Datenquelle, nur die eigenen Eingaben. Ein
        versehentlicher Klick wäre unwiederbringlich, deshalb steht in der
        Frage auch die Zahl der Posten — „4 Posten werden entfernt" wiegt
        anders als „wirklich löschen?".
        """
        from .main_window import ask_yes_no
        anzahl = len(lager.load())
        if not anzahl:
            return
        if not ask_yes_no(fenster.root, t('s_lg_leeren_frage_t'),
                             t('s_lg_leeren_frage') % anzahl):
            return
        lager.save([])
        verwerfen()
        meldung.configure(text=t('s_lg_geleert') % anzahl, fg=GOLD)
        zeichnen()

    _button(fenster, _reihe_aus, t('s_lg_leeren'), _leeren,
           danger=True).pack(side='left', padx=(24, 0))
    _body_text(innen, t('s_lg_aus_hilfe'), fenster.f_small, fill='x')

    zeichnen()



def _cooldown_text(rest):
    """Die Restzeit der Sperre als `43:12` — oder `''`, wenn sie abgelaufen ist."""
    if rest <= 0:
        return ''
    return '%d:%02d' % (rest // 60, rest % 60)


def _cooldown_color(rest):
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


def _age_text(seconds):
    """Wie alt eine Meldung ist, in Worten — `None` ergibt `''`."""
    if seconds is None:
        return ''
    stunden = seconds / 3600.0
    if stunden < 1:
        return t('s_vk_alter_frisch')
    if stunden < 24:
        return t('s_vk_alter_stunden').format(n=int(stunden))
    return t('s_vk_alter_tage').format(n=int(stunden / 24))


def _squashed(text):
    """Kleinschreibung ohne Punkte, Bindestriche und Leerzeichen.

    Damit findet „ATLS" auch „A.T.L.S.", und „F7C-M" auch „F7CM". Dieselbe
    Schreibweise, verschiedene Quellen — das ist bei Schiffsnamen der Normalfall
    und kein Sonderfall.
    """
    return re.sub(r'[^a-z0-9]', '', (text or '').lower())


def _combo_box(window, parent, var, get_entries, at_most=10,
                 on_pick=None, on_confirm=None, empty_text=None,
                 scrollable=0, extra=None):
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
    Lagerorten in `places.py`: Wer `Ore` tippt, sucht `Copper (Ore)`.

    ⚠ Es werden **höchstens zehn** Einträge gezeigt, sonst schiebt eine Liste
    mit 114 Waren alles andere aus dem Bild. Darunter steht, wie viele noch
    kommen — verschwiegen wäre schlimmer als abgeschnitten.

    ⭐⭐ **`zusatz`: was rechts neben dem Namen steht** — eine Funktion
    `name -> text` oder ein Wörterbuch. Bei der Teileauswahl sind das Güte und
    Klasse, und die entscheiden dort alles: Man baut ein Schiff auf einen Zweck
    hin. Am 06.09.2026 dazu: „man sollte in der Liste sehen ob es grade A B
    oder C ist und ob Military oder was anderes … nicht jeder weiß alle
    Komponenten auswendig."

    ⚠ **Der Zusatz wird mitgesucht.** Wer `stealth` tippt, meint die
    Tarn-Komponenten und nicht ein Teil namens Stealth — eine Liste, die den
    Text zeigt, aber nicht darauf reagiert, wirkt kaputt.
    """
    from .main_window import round_entry

    zeile = tk.Frame(parent, bg=BG)
    liste = tk.Frame(parent, bg=BG)
    offen = {'ja': False}

    feld = round_entry(zeile, var, window.f_small, '#0c1017', LINE, ACCENT,
                       FG)

    # ⚠ Dasselbe Klapp-Symbol wie überall sonst — nicht ein Textpfeil, der je
    # nach Systemschrift anders aussieht als die gezeichneten Symbole daneben.
    pfeil = icons.line(zeile, 'aufklappen', background=BG,
                          font=window.f_small)
    pfeil.configure(cursor='hand2')

    def _leeren():
        for w in liste.winfo_children():
            w.destroy()

    def _zusatz_text(name):
        """Was rechts neben diesem Namen steht — oder ''."""
        if extra is None:
            return ''
        try:
            return (extra(name) if callable(extra)
                    else extra.get(name) or '')
        except Exception:
            return ''

    def zeichnen():
        _leeren()
        text = (var.get() or '').strip().lower()
        alle = get_entries()
        # Steht genau der gewählte Eintrag im Feld, ist nichts mehr zu suchen.
        if text and any(text == e.lower() for e in alle) and not offen['ja']:
            liste.pack_forget()
            pfeil.swap_symbol('aufklappen')
            return
        if not text and not offen['ja']:
            liste.pack_forget()
            pfeil.swap_symbol('aufklappen')
            return
        # ⚠⚠ **Punkte und Bindestriche zaehlen beim Suchen nicht.** Wer „ATLS"
        # tippt, meint „A.T.L.S." — und umgekehrt. Ohne diese Zeile findet das
        # Feld sein eigenes Schiff nicht: Der Pledge-Store schreibt „A.T.L.S.",
        # UEX schreibt „Argo ATLS IKTI", und beide sind dasselbe Ding.
        # Gemeldet am 06.09.2026 beim Handeintrag im Hangar.
        schlank = _squashed(text)

        def _passt(name):
            if schlank in _squashed(name):
                return True
            # Der Zusatz zaehlt mit: „stealth" findet die Tarn-Teile, „a"
            # allein nicht (ein einzelner Buchstabe traefe jede Guete).
            bei = _zusatz_text(name)
            return len(schlank) > 1 and bei and schlank in _squashed(bei)

        treffer = [e for e in alle if _passt(e)] if text else list(alle)
        pfeil.swap_symbol('zuklappen' if offen['ja'] else 'aufklappen')
        if not treffer:
            liste.pack(fill='x', pady=(4, 0))
            tk.Label(liste, text=empty_text or t('s_vk_nichts_gefunden'),
                     bg=BG, fg=SUB,
                     font=window.f_small, anchor='w').pack(fill='x', pady=3)
            return
        liste.pack(fill='x', pady=(4, 0))
        # ⭐ **`rollbar`: alle Treffer, in einer Flaeche fester Hoehe.**
        # Ohne das zeigt die Liste zehn Namen und sagt „und 124 weitere" — wer
        # nicht weiterliest, haelt die zehn fuer das ganze Angebot. Gemeldet am
        # 06.09.2026 zum Hangar: „sonst sieht er die Liste und denkt sich, dass
        # nicht alle Schiffe eintragbar sind."
        #
        # ⚠ Die Hoehe ist **fest**, nicht mitwachsend: 280 Schiffe untereinander
        # wuerden die ganze Seite zuschuetten und den Knopf darunter
        # unerreichbar machen.
        halter = liste
        if scrollable:
            leinwand = tk.Canvas(liste, bg=BG, highlightthickness=0,
                                 height=scrollable)
            from .main_window import round_scrollbar, bind_wheel
            balken = round_scrollbar(liste, leinwand, bg=BG)
            halter = tk.Frame(leinwand, bg=BG)
            halter.bind('<Configure>', lambda _e: leinwand.configure(
                scrollregion=leinwand.bbox('all')))
            fenster_id = leinwand.create_window((0, 0), window=halter,
                                                anchor='nw')
            leinwand.bind('<Configure>', lambda e: leinwand.itemconfigure(
                fenster_id, width=e.width))
            leinwand.configure(yscrollcommand=balken.set)
            balken.pack(side='right', fill='y')
            leinwand.pack(side='left', fill='both', expand=True)
            # ⚠ Ueber die vorhandene Stelle, nie selbst gebaut — sie kennt die
            # Fallen (Trackpad, macOS, `bind_all` ohne `add='+'`).
            bind_wheel(leinwand)

        zeigen = treffer if scrollable else treffer[:at_most]

        def _eintrag_bauen(nummer):
            name = zeigen[nummer]
            bei = _zusatz_text(name)
            if not bei:
                eintrag = tk.Label(halter, text=name, bg=BG, fg=FG,
                                   font=window.f_small, anchor='w',
                                   cursor='hand2', padx=8, pady=3)
                mitfaerben = (eintrag,)
            else:
                # ⚠ Zwei Beschriftungen in einer Zeile, **nicht** ein Text mit
                # Trennzeichen: Der Zusatz steht rechts und in gedaempfter
                # Farbe, damit die Namensspalte beim Ueberfliegen eine Spalte
                # bleibt. Zusammengeklebt („Fortitude · C · Industrial")
                # franst die linke Kante bei jeder Zeile anders aus.
                eintrag = tk.Frame(halter, bg=BG, cursor='hand2')
                links = tk.Label(eintrag, text=name, bg=BG, fg=FG,
                                 font=window.f_small, anchor='w',
                                 cursor='hand2', padx=8, pady=3)
                links.pack(side='left')
                rechts = tk.Label(eintrag, text=bei, bg=BG, fg=SUB,
                                  font=window.f_small, anchor='e',
                                  cursor='hand2', padx=8, pady=3)
                rechts.pack(side='right')
                mitfaerben = (eintrag, links, rechts)
            eintrag.pack(fill='x')
            for teil in mitfaerben:
                teil.bind('<Button-1>', lambda _e, n=name: waehlen(n))
                teil.bind('<Enter>', lambda _e, w=mitfaerben: [
                    x.configure(bg=SURFACE) for x in w])
                teil.bind('<Leave>', lambda _e, w=mitfaerben: [
                    x.configure(bg=BG) for x in w])

        # ⭐⭐ **Nur die sichtbaren Einträge bauen.** Bei 400 Teilen entstanden
        # sonst über 1.600 Bauteile — und beim nächsten Tastendruck wurden sie
        # alle wieder zerstört. Gemessen: 1,03 von 1,24 s gingen allein für
        # das `destroy()` drauf. Siehe `_build_on_demand`.
        #
        # ⚠ Ohne Rollfläche (`rollbar=None`) bleibt alles wie bisher: Dort
        # deckelt `hoechstens` die Liste ohnehin auf wenige Zeilen.
        if scrollable:
            _build_on_demand(leinwand, len(zeigen), _eintrag_bauen)
        else:
            for nummer in range(len(zeigen)):
                _eintrag_bauen(nummer)
        rest = 0 if scrollable else len(treffer) - at_most
        if rest > 0:
            tk.Label(halter, text=t('s_af_weitere').format(n=rest), bg=BG,
                     fg=SUB, font=window.f_small, anchor='w',
                     padx=8).pack(fill='x', pady=(2, 0))

    def waehlen(name):
        offen['ja'] = False
        if on_pick is not None:
            # Der Verkaufs-Reiter sammelt mehrere Waren: Dort landet der Name
            # in der Auswahl, und das Feld wird wieder leer. Ohne diesen Weg
            # müsste der Aufrufer den Eintrag aus dem Feld zurücklesen.
            on_pick(name)
        else:
            var.set(name)
        # ⚠⚠ **Der Rückruf kann dieses Feld zerstört haben.** Bei der
        # Teileauswahl im Warenkorb zeichnet `beim_waehlen` die ganze
        # Steckplatz-Liste neu — und die Auswahlliste hängt darin. Danach
        # arbeitete `zeichnen()` auf einem Widget weiter, das es nicht mehr
        # gibt: `TclError: bad window path name`, acht Stück in einem
        # Fehlerbericht vom 06.09.2026.
        #
        # ⚠ Und **warum das erst jetzt auffiel**: Solange der Klick gar nichts
        # bewirkte (der Name kam nie an, siehe `uebernehmen`), wurde auch
        # nichts neu gezeichnet. Ein behobener Fehler hat den zweiten
        # freigelegt — nicht verursacht.
        try:
            if not liste.winfo_exists():
                return
        except tk.TclError:
            return
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
    feld.holder.pack(side='left', fill='both', expand=True)

    # ⭐⭐ **Ein Klick ins Feld klappt die Liste auf.** Am 05.09.2026 gemeldet:
    # „Erwarte, dass ich ins Feld klicke, was eingeben kann und auch vor der
    # Eingabe schon das Dropdown aufgeht — das Dropdown ist aber rechts
    # versteckt, da sucht es niemand." Beides stimmt: Der Pfeil ist ein
    # 16-Pixel-Symbol am rechten Rand, und wer ein Auswahlfeld anklickt,
    # erwartet eine Auswahl. Der Pfeil bleibt trotzdem — er zeigt an, dass da
    # etwas zum Aufklappen ist, und schliesst die Liste wieder.
    def beim_hineinklicken(_=None):
        if not offen['ja']:
            offen['ja'] = True
            zeichnen()

    feld.bind('<FocusIn>', beim_hineinklicken, add='+')
    feld.bind('<Button-1>', beim_hineinklicken, add='+')

    # ⚠⚠ **Und sie geht wieder zu, wenn man woanders hinklickt.** Am
    # 05.09.2026 gemeldet: „Wenn man dann doch nichts auswählt, bleibt die
    # einfach offen." Eine Liste, die nur aufgeht, ist eine halbe Bedienung.
    #
    # ⚠ **Verzögert um 200 ms** — und das ist kein Schönheitsfehler: Ein Klick
    # auf einen Listeneintrag löst zuerst `<FocusOut>` am Feld aus und erst
    # danach den Klick auf die Zeile. Wer sofort zumacht, zerstört die Zeile,
    # bevor ihr Klick ankommt; die Auswahl ginge nie.
    def _zumachen():
        try:
            if not liste.winfo_exists():
                return
        except tk.TclError:
            return
        if offen['ja']:
            offen['ja'] = False
            zeichnen()

    # ⚠⚠ **Ein Klick auf die Rollleiste ist ein Klick IN die Liste.**
    # Gemeldet am 11.09.2026 zu „Was steckt drin?": Mit dem Mausrad ließ sich
    # die Schiffsliste rollen — wer aber die Leiste rechts anfasste, dem
    # verschwand die ganze Auswahl. Ursache ist die Fensterregel
    # `_bind_click_on_empty`: Die Leiste ist eine Leinwand, kein
    # Eingabefeld, also bekommt das Fenster den Fokus, das Feld meldet
    # `<FocusOut>`, und 200 ms später klappte `_zumachen` die Liste zu.
    # `_klick_im_fenster` erkannte den Klick zwar richtig als „drinnen" —
    # nur fragte das `<FocusOut>` gar nicht erst nach.
    #
    # Deshalb merkt sich `_klick_im_fenster`, wann zuletzt innen geklickt
    # wurde. Fällt der Fokusverlust auf denselben Augenblick, stammt er von
    # diesem Klick, und die Liste bleibt offen. Verglichen wird mit dem
    # Zeitpunkt des `<FocusOut>`, **nicht** mit dem des verzögerten Aufrufs:
    # Kommt der Zeitgeber unter Last zu spät, darf das nicht wieder zuklappen.
    # Selbsttest 189 geht genau diesen Weg.
    drinnen = {'zeit': -1.0}

    def beim_verlassen(_=None):
        verlassen = time.monotonic()

        def _spaeter():
            if abs(verlassen - drinnen['zeit']) < 0.15:
                return
            _zumachen()

        try:
            feld.after(200, _spaeter)
        except tk.TclError:
            pass

    feld.bind('<FocusOut>', beim_verlassen, add='+')
    feld.bind('<Escape>', lambda _=None: _zumachen(), add='+')

    # ⚠⚠ **Ein Klick irgendwohin schließt die Liste — auch ins Leere.**
    # `<FocusOut>` allein reicht **nicht**: Es feuert nur, wenn ein anderes
    # **fokussierbares** Element den Fokus übernimmt. Wer auf eine
    # Beschriftung oder freie Fläche klickt, lässt den Fokus im Eingabefeld —
    # und die Liste blieb offen. Am 05.09.2026: „Klicken im selben Fenster ins
    # Leere blendet das Dropdown auch nicht aus, ist in jedem Programm so, nur
    # bei mir nicht."
    #
    # Deshalb hängt die Prüfung am Fenster, nicht am Feld: Jeder Klick wird
    # daraufhin angesehen, ob er innerhalb von Feld, Pfeil oder Liste lag.
    def _klick_im_fenster(ereignis):
        if not offen['ja']:
            return
        w = getattr(ereignis, 'widget', None)
        # Nach oben durchhangeln: Ein Klick auf eine Zeile IN der Liste ist
        # ein Klick auf die Liste.
        while w is not None:
            if w in (feld, liste, pfeil, zeile):
                # Für `beim_verlassen`: Ein Fokusverlust in diesem Augenblick
                # kommt von hier (Rollleiste) und klappt nicht zu.
                drinnen['zeit'] = time.monotonic()
                return
            w = getattr(w, 'master', None)
        _zumachen()

    try:
        parent.winfo_toplevel().bind('<Button-1>', _klick_im_fenster, add='+')
    except tk.TclError:
        pass

    # ⭐⭐ **Enter übernimmt, was dasteht.** Am 05.09.2026 gemeldet: „Gebe ich
    # Gold fertig ein und wähle es nicht aus der Liste, übernimmt er Gold auch
    # nicht." Wer den Namen kennt und ihn zu Ende tippt, hat die Liste nicht
    # nötig — und drückt Enter.
    #
    # ⚠ Der Aufrufer entscheidet, was gültig ist (`beim_bestaetigen`): Im
    # Verkauf muss die Ware in den Preisdaten stehen, sonst käme ein Name in
    # die Auswahl, zu dem es nie ein Ergebnis geben kann.
    def _bestaetigen(_=None):
        if on_confirm is None:
            return
        text = (var.get() or '').strip()
        if not text:
            return
        # Genau ein passender Eintrag? Dann ist die Sache eindeutig, auch
        # wenn nur ein Teil getippt wurde.
        alle = get_entries()
        genau = [e for e in alle if e.lower() == text.lower()]
        teil = [e for e in alle if text.lower() in e.lower()]
        ziel = genau[0] if genau else (teil[0] if len(teil) == 1 else '')
        if ziel:
            offen['ja'] = False
            on_confirm(ziel)

    feld.bind('<Return>', _bestaetigen, add='+')
    feld.bind('<KP_Enter>', _bestaetigen, add='+')

    # Tippen dampft die Liste auf die Treffer ein — sonst bliebe die volle
    # Liste stehen, während schon gefiltert wird. Ist das Feld wieder leer,
    # bleibt sie offen: Der Spieler sucht dann ja noch.
    def beim_tippen(*_):
        offen['ja'] = not (var.get() or '').strip()
        zeichnen()

    var.trace_add('write', beim_tippen)
    return zeile, liste, zeichnen


def _selling(fenster, rahmen):
    """Wo man seine Ware los wird — die beste Stelle zuerst."""
    import threading

    from . import trade_cargo, selling as preisdaten
    from .main_window import round_entry

    _heading(fenster, rahmen, t('hf_verkauf'), t('s_vk_lead'))
    innen = _scroll_area(rahmen)

    # Die ausgewählten Waren. Liste statt Menge, damit die Reihenfolge der
    # Auswahl erhalten bleibt — wer zuerst Gold eintippt, sieht Gold zuerst.
    auswahl = []
    suche = tk.StringVar()
    # Von Hand eingetragene Mengen — sie ergänzen das Handelslager, ohne dass
    # etwas eingelagert werden muss. Siehe `waehlen`.
    # Von Hand eingetragene Mengen je Ware — sie ergänzen das Handelslager,
    # ohne dass etwas eingelagert werden muss. Eingetippt wird an der Marke
    # der jeweiligen Ware (siehe `_chips`).
    eigene_mengen = {}
    nur_nqa = [False]
    meldung = {'text': '', 'farbe': SUB}

    ergebnis_rahmen = tk.Frame(innen, bg=BG)
    chip_rahmen = tk.Frame(innen, bg=BG)

    # ------------------------------------------------ Kopf: Abruf und Stand
    kopf = tk.Frame(innen, bg=BG)
    kopf.pack(fill='x', padx=24, pady=(4, 0))

    stand_label = tk.Label(kopf, text='', bg=BG, fg=SUB,
                           font=fenster.f_small, anchor='w')

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
        rest = preisdaten.wait_time()
        if rest:
            meldung['text'], meldung['farbe'] = t('s_vk_gesperrt'), GOLD
            neu_zeichnen()
            return
        laeuft['ja'] = True
        knopf.beschriften(t('s_vk_holt'), SUB)

        def arbeit():
            try:
                ok, grund = preisdaten.update(force=True)
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
                    meldung['text'], meldung['farbe'] = t('s_vk_fehler'), RED
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

    knopf = _button(fenster, kopf, t('s_vk_holen'), abrufen)
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
        rest = preisdaten.wait_time()
        if rest:
            knopf.beschriften(_cooldown_text(rest), _cooldown_color(rest))
        else:
            knopf.beschriften(t('s_vk_holen'), None)
        alter = preisdaten.age()
        # ⚠⚠ **Ein Patch zählt mehr als das Alter.** Die Zahlen können eine
        # Stunde alt und trotzdem überholt sein, wenn dazwischen ein Patch lag —
        # CIG wirft dabei regelmäßig Preise um. Wer das nicht sagt, behauptet
        # etwas Falsches mit derselben Bestimmtheit wie etwas Richtiges.
        from . import gamebuild
        veraltet, damals, jetzt = gamebuild.outdated(preisdaten._store)
        if veraltet:
            stand_label.configure(text=t('s_vk_patch').format(
                alt=damals, neu=jetzt), fg=GOLD)
        elif alter is not None:
            stand_label.configure(
                text=t('s_vk_stand').format(alter=_age_text(alter)), fg=SUB)
        else:
            stand_label.configure(text=t('s_vk_kein_stand'), fg=SUB)
        knopf.after(1000, _ticker)

    # ------------------------------------------------------- Warenauswahl
    def waehlen(name):
        if name not in auswahl:
            auswahl.append(name)
        # ⭐⭐ **Menge gleich mitnehmen, ohne Umweg übers Lager.** Am
        # 05.09.2026: „Eine Menge, die man hat, kann man nur über das Lager
        # eingeben — manchmal will man Ware sofort verkaufen, ohne erst
        # einzulagern." Richtig: Wer gerade 120 SCU Gold im Laderaum hat und
        # wissen will, was sie bringen, will sie nicht erst eintragen.
        suche.set('')
        neu_zeichnen()

    def entfernen(name):
        if name in auswahl:
            auswahl.remove(name)
        eigene_mengen.pop(name, None)
        neu_zeichnen()

    def aus_lager():
        """Die Waren aus dem Handelslager übernehmen — die Brücke zum Lager."""
        genommen = 0
        for ware in trade_cargo.goods_in_stock():
            # ⚠ Nur übernehmen, was die Preisdaten auch kennen. Sonst steht ein
            # Name in der Auswahl, zu dem es nie ein Ergebnis geben kann, und
            # der Nutzer sucht den Fehler bei sich.
            if preisdaten.known(ware) and ware not in auswahl:
                auswahl.append(ware)
                genommen += 1
        if trade_cargo.has_stolen():
            nur_nqa[0] = True
        if not genommen:
            meldung['text'] = t('s_vk_lager_leer')
            meldung['farbe'] = SUB
        neu_zeichnen()

    # ------------------------------------------------------- Suchfeld
    suchzeile = tk.Frame(innen, bg=BG)
    suchzeile.pack(fill='x', padx=24, pady=(14, 0))
    tk.Label(suchzeile, text=t('s_vk_ware'), bg=BG, fg=SUB,
             font=fenster.f_small, anchor='w').pack(fill='x')
    # Auswahlfeld statt blossem Suchfeld: Wer nicht weiss, wie die Ware bei UEX
    # heisst, klappt die Liste auf und sucht sie aus.
    feldzeile, feldliste, such_zeichnen = _combo_box(
        fenster, suchzeile, suche, preisdaten.goods,
        on_pick=lambda name: waehlen(name),
        on_confirm=lambda name: waehlen(name))
    feldzeile.pack(fill='x', pady=(4, 0))
    feldliste.pack(fill='x')

    # ⚠ Hier stand bis v3.15.0-rc12 ein einzelnes Mengenfeld für „die zuletzt
    # gewählte Ware". Es ist weg: Die Menge steht jetzt an der Marke der Ware
    # selbst (siehe `_chips`), und zwei Wege für dieselbe Sache waren genau
    # der Grund, warum sich die Zahlen gegenseitig überschrieben.
    tk.Label(suchzeile, text=t('s_vk_menge_hinweis'), bg=BG, fg=SUB,
             font=fenster.f_small, anchor='w').pack(fill='x', pady=(6, 0))

    def kaestchen_um(an):
        nur_nqa[0] = an
        neu_zeichnen()

    schalterzeile = tk.Frame(innen, bg=BG)
    schalterzeile.pack(fill='x', padx=24, pady=(10, 0))
    _checkbox(schalterzeile, t('s_vk_nur_nqa'), nur_nqa, kaestchen_um,
               fenster.f_small).pack(side='left')
    _button(fenster, schalterzeile, t('s_vk_aus_lager'),
           aus_lager).pack(side='right')

    chip_rahmen.pack(fill='x', padx=24, pady=(10, 0))
    ergebnis_rahmen.pack(fill='both', expand=True, padx=24, pady=(6, 20))

    def _leeren(halter):
        for kind in halter.winfo_children():
            kind.destroy()

    def _chips():
        """Die gewählten Waren — je eine Marke mit **eigenem** Mengenfeld.

        ⚠⚠ **Die Menge gehört an die Ware, nicht an ein Feld daneben.**
        Zuerst gab es ein einziges Mengenfeld, das für „die zuletzt gewählte
        Ware" galt. Das muss raten — und rät falsch, sobald jemand die Zahl
        eintippt, **bevor** er die Ware wählt. Am 05.09.2026 gemeldet: „Wenn
        ich eine Menge eingebe, wird die nicht übernommen, erst wenn ich den
        zweiten Artikel eingebe — und dann ändert sich die Zahl wieder."
        Genau das: Die Zahl landete bei der vorigen Ware und wanderte dann mit.

        Mit einem Feld je Marke gibt es nichts mehr zu raten.
        """
        _leeren(chip_rahmen)
        if not auswahl:
            return
        reihe = tk.Frame(chip_rahmen, bg=BG)
        reihe.pack(fill='x')
        for name in auswahl:
            marke = tk.Frame(reihe, bg=SURFACE)
            marke.pack(side='left', padx=(0, 6), pady=2)
            tk.Label(marke, text=name, bg=SURFACE, fg=FG,
                     font=fenster.f_small, padx=8, pady=3).pack(side='left')

            var = tk.StringVar(value=str(eigene_mengen.get(name) or ''))
            feld = tk.Entry(marke, textvariable=var, width=5,
                            font=fenster.f_small, bg='#0c1017', fg=FG,
                            insertbackground=FG, relief='flat',
                            highlightthickness=1, highlightbackground=LINE,
                            highlightcolor=ACCENT, justify='right')
            feld.pack(side='left', pady=3)
            # ⚠ Nur ein Wort: Das Feld ist fünf Zeichen breit. Ein
            # abgeschnittener Hinweis wäre schlimmer als keiner — und die
            # Einheit steht ohnehin als Etikett daneben.
            fields.hint(feld, var, t('s_pl_menge'), normal=FG, grey=SUB)
            tk.Label(marke, text=t('s_vk_scu_kurz'), bg=SURFACE, fg=SUB,
                     font=fenster.f_small, padx=4).pack(side='left')

            def _getippt(*_a, _n=name, _v=var):
                # ⚠ **Nur das Ergebnis neu zeichnen, nicht die Marken.** Wer
                # `_chips()` mitzieht, zerstört genau das Feld, in das gerade
                # getippt wird — nach jedem Zeichen wäre der Fokus weg.
                try:
                    scu = int(float((_v.get() or '0').replace(',', '.')))
                except (TypeError, ValueError):
                    return
                if scu > 0:
                    eigene_mengen[_n] = scu
                else:
                    eigene_mengen.pop(_n, None)
                _ergebnis()

            var.trace_add('write', _getippt)

            weg = tk.Label(marke, text='×', bg=SURFACE, fg=SUB,
                           font=fenster.f_small, padx=8, pady=3,
                           cursor='hand2')
            weg.pack(side='left')
            weg.bind('<Button-1>', lambda e, n=name: entfernen(n))
            weg.bind('<Enter>', lambda e, w=weg: w.configure(fg=RED))
            weg.bind('<Leave>', lambda e, w=weg: w.configure(fg=SUB))

    def _ergebnis():
        _leeren(ergebnis_rahmen)
        if meldung['text']:
            tk.Label(ergebnis_rahmen, text=meldung['text'], bg=BG,
                     fg=meldung['farbe'], font=fenster.f_small,
                     anchor='w').pack(fill='x', pady=(0, 8))
            meldung['text'] = ''
        if not auswahl:
            _body_text(ergebnis_rahmen, t('s_vk_leer_hinweis'),
                        fenster.f_small, fill='x')
            # ⭐⭐ **Statt einer leeren Seite: Was zahlt gerade am besten?**
            # Xharig am 04.09.2026: „ist einfach nur leer, und ein Suchfeld,
            # ist langweilig — was kann man da hinbauen?"
            #
            # Die Antwort lag schon da: Die Ablage kennt 114 Waren mit
            # Ankaufgeboten. Daraus eine Bestenliste zu bauen kostet **keinen
            # einzigen Abruf** — und beantwortet die Frage, die man vor dem
            # Suchfeld überhaupt hat: „Wonach soll ich suchen?"
            #
            # ⚠ Anklickbar, nicht nur zum Anschauen. Eine Liste, aus der man
            # nichts übernehmen kann, ist eine Tapete.
            # ⚠ Event-Geschenke bleiben draußen, und ein einzelnes absurdes
            # Gebot wird verworfen — beides steckt in `selling.py`, damit es
            # an einer Stelle steht und nicht in der Anzeige verstreut.
            spitze = []
            for ware in preisdaten.goods():
                if not preisdaten.in_top_list(ware):
                    continue
                preis = preisdaten.best_price(ware)
                if preis:
                    spitze.append((preis, ware))
            spitze.sort(reverse=True)
            if not spitze:
                return
            tk.Label(ergebnis_rahmen, text=t('s_vk_spitze'), bg=BG, fg=SUB,
                     font=fenster.f_small, anchor='w').pack(fill='x',
                                                            pady=(14, 4))
            for preis, ware in spitze[:12]:
                kasten = tk.Frame(ergebnis_rahmen, bg=SURFACE,
                                  highlightthickness=1,
                                  highlightbackground=LINE)
                kasten.pack(fill='x', pady=(0, 3))
                zeile = tk.Frame(kasten, bg=SURFACE)
                zeile.pack(fill='x', padx=12, pady=5)
                for w in (kasten, zeile):
                    w.configure(cursor='hand2')
                # ⚠ `s_vk_je_scu` gab es **schon** — mit `{preis}` statt `%s`.
                # Ein zweiter Eintrag desselben Namens hat den alten still
                # verdrängt, und `% _geld(...)` flog auf die Nase: „Text da,
                # aber zeigt nichts an" (04.09.2026). Ein doppelter Schlüssel
                # fällt in einem Wörterbuch nicht auf — deshalb prüft der
                # Selbsttest das jetzt.
                p = tk.Label(zeile,
                             text=t('s_vk_je_scu').format(
                                 preis=_auec(preis)),
                             bg=SURFACE, fg=ACCENT, font=fenster.f_small,
                             width=20, anchor='w')
                p.pack(side='left')
                n = tk.Label(zeile, text=ware, bg=SURFACE, fg=FG,
                             font=fenster.f_small, anchor='w', cursor='hand2')
                n.pack(side='left')
                for w in (kasten, zeile, p, n):
                    w.bind('<Button-1>',
                           lambda _=None, x=ware: waehlen(x))
            return
        orte = preisdaten.places_for(auswahl, nqa_only=nur_nqa[0])
        if not orte:
            _body_text(ergebnis_rahmen, t('s_vk_keine_orte'),
                        fenster.f_small, fill='x')
            return
        # Mengen aus dem Handelslager — nur dann wird ein echter Erlös gezeigt.
        # ⚠ Ohne Mengen **keine Summe**: Sie wäre eine Behauptung über eine
        # Ladung, die das Werkzeug nicht kennt (siehe `orte_fuer`).
        # ⚠ Von Hand eingetragene Mengen **überschreiben** das Lager: Wer hier
        # 120 SCU eingibt, meint die Ladung, die er gerade dabei hat — nicht
        # das, was vor drei Tagen im Lager stand.
        lagermengen = dict(trade_cargo.amounts())
        lagermengen.update(eigene_mengen)
        for nummer, ort in enumerate(orte[:40]):
            # ⚠ Die Spaltenüberschrift steht **nur über dem ersten Kasten**.
            # In jedem zu wiederholen war der erste Bau: Bei 40 Orten stand
            # „Ware · SCU · Preis 1 SCU · Gesamtpreis" vierzigmal da und machte
            # die Liste unruhiger, statt sie zu erklären.
            _selling_row(fenster, ergebnis_rahmen, ort, len(auswahl),
                           lagermengen, mit_kopf=(nummer == 0))

    def neu_zeichnen():
        _chips()
        such_zeichnen()
        _ergebnis()

    # ⚠⚠ **Ein Klick ins Leere nimmt auch den Schreibcursor mit.** Am
    # 05.09.2026 gemeldet: „Bei Eingabe der Menge muss der Marker aus dem Feld
    # auch wieder verschwinden, wenn man ins Leere klickt." Stimmt — ein
    # blinkender Cursor in einem Feld, das man gar nicht mehr bearbeitet,
    # behauptet, man sei noch dabei.
    #
    # ⚠ Nur, wenn wirklich daneben geklickt wurde: Ein Klick in ein anderes
    # Eingabefeld gehört dorthin, und ein Klick auf einen Knopf soll dessen
    # eigene Wirkung behalten.
    def _fokus_loslassen(ereignis):
        ziel = getattr(ereignis, 'widget', None)
        if isinstance(ziel, (tk.Entry, tk.Text)):
            return
        try:
            if rahmen.winfo_exists() and rahmen.focus_get() is not None:
                rahmen.focus_set()
        except (tk.TclError, KeyError):
            pass

    try:
        rahmen.winfo_toplevel().bind('<Button-1>', _fokus_loslassen, add='+')
    except tk.TclError:
        pass

    neu_zeichnen()
    _ticker()

    # ℹ Der stille Abruf steht **nicht** hier, sondern beim Programmstart
    # (`sc_bp_watcher.py`, neben `preise` und `orte`). Wer die Seite öffnet,
    # soll Daten vorfinden und nicht auf einen Abruf warten — und eine Seite,
    # die beim Betreten von sich aus ins Netz greift, tut das bei jedem
    # Umschalten erneut.


def _selling_row(fenster, eltern, ort, gesucht, lagermengen,
                   mit_kopf=False):
    """Ein Ankaufsort: wie viele Waren er nimmt, was er zahlt, wie alt das ist."""
    kasten = tk.Frame(eltern, bg=SURFACE, highlightthickness=1,
                      highlightbackground=LINE)
    kasten.pack(fill='x', pady=(0, 6))
    innen = tk.Frame(kasten, bg=SURFACE)
    innen.pack(fill='x', padx=12, pady=8)

    kopf = tk.Frame(innen, bg=SURFACE)
    kopf.pack(fill='x')

    # ⭐ Die Trefferzahl steht **vorn und farbig**. Sie ist die eigentliche
    # Antwort des Reiters: Ein Ort, der die ganze Ladung nimmt, spart einen
    # Anflug — und das ist mehr wert als ein paar Prozent Aufpreis anderswo
    # (gemessen: 2 % Mehrerlös für zwei zusätzliche Stopps).
    voll = ort['anzahl'] >= gesucht
    tk.Label(kopf, text='%d/%d' % (ort['anzahl'], gesucht), bg=SURFACE,
             fg=ACCENT if voll else SUB, font=fenster.f_small,
             width=5, anchor='w').pack(side='left')

    name = ort['terminal']
    beiwerk = ' · '.join(x for x in (ort.get('ort'), ort.get('system')) if x)
    tk.Label(kopf, text=name, bg=SURFACE, fg=FG, font=fenster.f_small,
             anchor='w').pack(side='left')
    if beiwerk:
        tk.Label(kopf, text='  ' + beiwerk, bg=SURFACE, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(side='left')
    if ort.get('nqa'):
        # Keine Wertung, nur eine Auskunft: Hier wird nicht nach der Herkunft
        # gefragt. Für saubere Ware ist das weder gut noch schlecht.
        tk.Label(kopf, text='  ' + t('s_vk_nqa_marke'), bg=SURFACE, fg=GOLD,
                 font=fenster.f_small, anchor='w').pack(side='left')

    alterstext = _age_text(ort.get('alter'))
    if alterstext:
        # ⚠ Alte Meldungen werden **abgesetzt, nicht versteckt**. Wer eine 10
        # Tage alte Angabe sieht, kann selbst entscheiden, ob ihm das reicht;
        # eine Zeile, die stillschweigend fehlt, lässt ihn das Werkzeug für
        # kaputt halten.
        zu_alt = (ort.get('alter') or 0) > 7 * 24 * 3600
        tk.Label(kopf, text=alterstext, bg=SURFACE, fg=GOLD if zu_alt else SUB,
                 font=fenster.f_small, anchor='e').pack(side='right')

    # ⚠ Dasselbe Raster wie im Handelslager: Ware | SCU | Preis 1 SCU |
    # Gesamtpreis. Zwei Ansichten desselben Werkzeugs dürfen ihre Zahlen nicht
    # verschieden anordnen — wer die eine lesen kann, muss die andere blind
    # lesen können.
    zeilen = tk.Frame(innen, bg=SURFACE)
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
            tk.Label(zeilen, text=titel, bg=SURFACE, fg=SUB, padx=8,
                     font=fenster.f_small,
                     anchor='w' if spalte == 0 else 'e').grid(
                row=0, column=spalte, sticky='ew')

    erloes = 0.0
    for reihe, treffer in enumerate(ort['treffer'], start=1):
        menge = lagermengen.get(treffer['ware'])
        summe = (menge or 0) * treffer['preis']
        erloes += summe
        # ⭐ Der Füllstand steht **am Warennamen**, weil er dorthin gehört:
        # Dasselbe Terminal kann bei einer Ware randvoll und bei der nächsten
        # leer sein. Und er erscheint nur, wenn er etwas ändert — der
        # Normalfall (über 90 %) bleibt stumm, sonst liest ihn niemand mehr.
        stand = treffer.get('fuellstand')
        warentext = treffer['ware']
        warenfarbe = SUB
        if stand:
            schluessel, ist_warnung = stand
            warentext = '%s  %s' % (treffer['ware'], t(schluessel))
            warenfarbe = RED if ist_warnung else GOLD
        felder = (
            (warentext, warenfarbe, 'w'),
            (_amount_text(menge) if menge else '', FG, 'e'),
            (_money(treffer['preis']), FG, 'e'),
            (_money(summe) if menge else '', FG, 'e'),
        )
        for spalte, (text, farbe, seite) in enumerate(felder):
            tk.Label(zeilen, text=text, bg=SURFACE, fg=farbe, padx=8,
                     font=fenster.f_small, anchor=seite).grid(
                row=reihe, column=spalte, sticky='ew')

    # ⚠⚠ **Eine Summe gibt es nur mit Mengen aus dem Handelslager.** Ohne sie
    # wäre jede Zahl hier eine Behauptung über eine Ladung, die das Werkzeug
    # nicht kennt — siehe `selling.places_for`.
    if erloes:
        tk.Label(innen, text=t('s_vk_erloes').format(summe=_money(erloes)),
                 bg=SURFACE, fg=ACCENT, font=fenster.f_small,
                 anchor='e').pack(fill='x', pady=(4, 0))


def _trade_storage(fenster, rahmen):
    """Was zum Verkauf im Laderaum liegt — eintragen, ansehen, löschen."""
    from . import trade_cargo as lager, places as ortsliste
    from . import selling as preisdaten
    from .main_window import round_entry

    _heading(fenster, rahmen, t('hf_handelslager'), t('s_hl_lead'))
    innen = _scroll_area(rahmen)
    _body_text(innen, t('s_hl_hinweis'), fenster.f_small, fill='x')

    ware = tk.StringVar()
    menge = tk.StringVar()
    ort = tk.StringVar(value=pfade.einstellung('handel_ort') or '')
    gestohlen = [False]
    meldung = {'text': '', 'farbe': SUB}
    # Welche Zeile gerade zum Ändern offen ist. `None` heisst: neuer Posten.
    # ⚠ Die Nummer ist die Position in `lager.load()` — dieselbe Vorsicht wie
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
                 font=fenster.f_bold, anchor='w').pack(fill='x')
        if var is menge:
            feld = round_entry(block, var, fenster.f_small, '#0c1017', LINE,
                               ACCENT, FG)
            feld.holder.pack(fill='x', pady=(4, 0))
            continue
        quelle = (preisdaten.goods if var is ware else ortsliste.all_places)
        zeile, liste, zeichnen_ = _combo_box(fenster, block, var, quelle)
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
    _checkbox(schalter, t('s_hl_gestohlen'), gestohlen, marke_um,
               fenster.f_small).pack(side='left')

    def _leeren(halter):
        for kind in halter.winfo_children():
            kind.destroy()

    def eintragen():
        name = (ware.get() or '').strip()
        # ⚠⚠ **Geschlossene Liste, kein Freitext** — dieselbe Regel wie beim
        # Lagerort. Angenommen wird nur, was UEX kennt; sonst steht am Ende ein
        # ausgedachter oder beleidigender Name im Werkzeug, und ein Bildschirm-
        # foto davon macht die Runde.
        if not preisdaten.known(name):
            meldung['text'], meldung['farbe'] = t('s_hl_unbekannt'), RED
            neu_zeichnen()
            return
        # ⚠⚠ **Der Ort wird genauso gesperrt wie die Ware.** Sonst steht in
        # dem einen Feld eine geschlossene Liste und im anderen darf jeder
        # tippen, was er will — und der Missbrauchsfall (etwas Beleidigendes
        # eintragen, Bildschirmfoto machen, verbreiten) steht wieder offen.
        #
        # ℹ `places.knows()` lässt Leeres durch und meldet ohne Ortsliste alles
        # als gültig — der Lagerort ist freiwillig, und beim ersten Start ohne
        # Netz darf das Feld nicht blockieren.
        if not ortsliste.knows(ort.get()):
            meldung['text'], meldung['farbe'] = t('s_hl_ort_unbekannt'), RED
            neu_zeichnen()
            return
        if bearbeitung['nummer'] is None:
            ok, grund = lager.add(name, menge.get(), ort.get(),
                                        gestohlen[0])
        else:
            ok, grund = lager.change(bearbeitung['nummer'], name,
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
            meldung['farbe'] = RED
        neu_zeichnen()

    # Zeigt beim Tippen, was aus einer Rechnung herauskommt.
    # ⚠ Nur bei einer **Rechnung**, nicht bei einer blossen Zahl: Wer „40"
    # tippt, weiss, dass 40 herauskommt — „ergibt 40 SCU" wäre Rauschen.
    # Dieselbe Regel wie im Werkstatt-Lager.
    vorschau = tk.Label(innen, text='', bg=BG, fg=SUB, font=fenster.f_small,
                        anchor='w')
    vorschau.pack(fill='x', padx=24)

    def _bestand_vorher():
        nr = bearbeitung['nummer']
        if nr is None:
            return 0.0
        posten = lager.load()
        return (float(posten[nr].get('menge') or 0)
                if 0 <= nr < len(posten) else 0.0)

    def vorschau_zeigen(*_):
        roh = (menge.get() or '').strip()
        rechnung = any(z in roh[1:] for z in '+-−') or roh[:1] in '+-−'
        if not roh or not rechnung:
            vorschau.configure(text='')
            return
        wert = lager.calculate(roh, _bestand_vorher())
        if wert is None:
            vorschau.configure(text=t('s_hl_rechnung_kaputt'), fg=GOLD)
        elif wert <= 0:
            # ⚠ Sagen, was **passieren würde** — nicht nur „geht nicht". Wer
            # `100-200` tippt, soll sehen, warum das abgelehnt wird.
            vorschau.configure(text=t('s_hl_unter_null'), fg=RED)
        else:
            vorschau.configure(text=t('s_hl_ergibt').format(
                menge=_amount_text(wert)), fg=SUB)

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
            _button(fenster, knopf_rahmen, t('s_hl_buchen'), eintragen,
                   strong=True).pack(side='left')
        else:
            _button(fenster, knopf_rahmen, t('s_hl_speichern'), eintragen,
                   strong=True).pack(side='left')
            _button(fenster, knopf_rahmen, t('s_hl_abbrechen'),
                   abbrechen).pack(side='left', padx=(8, 0))

    def bearbeiten(nummer):
        """Einen Posten ins Formular holen — dann lässt er sich mit `+5`
        oder `-20` nachjustieren."""
        posten = lager.load()
        if not 0 <= nummer < len(posten):
            return
        p = posten[nummer]
        bearbeitung['nummer'] = nummer
        ware.set(p.get('ware') or '')
        # ⭐ Die aktuelle Menge steht **im Feld**. Wer fünf dazubuchen will,
        # tippt hinten `+5` an und hat `40+5` dastehen — die natürlichste
        # Schreibweise, und `rechnen()` versteht sie.
        menge.set(_amount_text(p.get('menge') or 0))
        ort.set(p.get('ort') or '')
        gestohlen[0] = bool(p.get('gestohlen'))
        neu_zeichnen()

    liste_rahmen.pack(fill='both', expand=True, padx=24, pady=(12, 20))

    def _liste():
        _leeren(liste_rahmen)
        if meldung['text']:
            tk.Label(liste_rahmen, text=meldung['text'], bg=BG,
                     fg=meldung['farbe'], font=fenster.f_small,
                     anchor='w').pack(fill='x', pady=(0, 8))
            meldung['text'] = ''
        posten = lager.load()
        if not posten:
            _body_text(liste_rahmen, t('s_hl_leer'), fenster.f_small,
                        fill='x')
            return
        # ⚠ Erst zeigen, wenn etwas dasteht: Ein Hinweis „Zeile anklicken", wo
        # keine Zeile ist, erklärt etwas, das man gar nicht tun kann.
        _body_text(liste_rahmen, t('s_hl_aendern_hinweis'), fenster.f_small,
                    fill='x', pady=(0, 8))
        gesamt = _trade_table(
            fenster, liste_rahmen, posten, preisdaten.best_price,
            lambda n: _keep_scroll(
                liste_rahmen, lambda: (lager.remove(n), abbrechen())),
            bearbeiten, bearbeitung['nummer'])
        if gesamt:
            # ⚠ „höchstens" ist wörtlich gemeint: der beste bekannte Ankauf je
            # Ware, ohne Rücksicht darauf, ob ein einzelner Ort alles nimmt.
            # Der Verkaufs-Reiter rechnet die belastbare Zahl je Ort.
            tk.Label(liste_rahmen,
                     text=t('s_hl_gesamt').format(summe=_money(gesamt)),
                     bg=BG, fg=ACCENT, font=fenster.f_small,
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
        from . import file_picker
        endung = '.csv' if art == 'csv' else '.json'
        ziel = file_picker.save_file(
            t('s_hl_ausgeben'),
            suggestion='handelslager-%s%s' % (time.strftime('%Y-%m-%d'), endung),
            extension=endung, start=None)
        if not ziel:
            return
        try:
            inhalt = (lager.as_csv() if art == 'csv' else lager.as_json())
            with open(ziel, 'w', encoding='utf-8') as f:
                f.write(inhalt)
            meldung['text'] = t('s_lg_gespeichert') % os.path.basename(ziel)
            meldung['farbe'] = SUB
        except Exception as ausnahme:
            fehler.merken('seiten.handelslager.ausgeben', ausnahme)
            meldung['text'], meldung['farbe'] = t('s_hl_fehler'), RED
        neu_zeichnen()

    def _einlesen():
        from . import file_picker
        quelle = file_picker.open_file(t('s_lg_einlesen'))
        if not quelle:
            return
        try:
            with open(quelle, encoding='utf-8') as f:
                posten = lager.from_json(f.read())
        except Exception as ausnahme:
            fehler.merken('seiten.handelslager.einlesen', ausnahme)
            posten = None
        if posten is None:
            # ⚠ Nicht schweigen. Wer eine falsche Datei waehlt und nichts
            # passieren sieht, haelt das Einlesen fuer kaputt.
            meldung['text'], meldung['farbe'] = t('s_hl_datei_falsch'), GOLD
            neu_zeichnen()
            return
        lager.save(posten)
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
        from .main_window import ask_yes_no
        anzahl = len(lager.load())
        if not anzahl:
            return
        if not ask_yes_no(fenster.root, t('s_hl_leeren_frage_t'),
                             t('s_hl_leeren_frage') % anzahl):
            return
        lager.clear()
        abbrechen()
        meldung['text'], meldung['farbe'] = t('s_hl_geleert') % anzahl, GOLD
        neu_zeichnen()

    _reihe_aus = tk.Frame(innen, bg=BG)
    _reihe_aus.pack(fill='x', padx=24, pady=(0, 4))
    _button(fenster, _reihe_aus, t('s_lg_aus_json'),
           lambda: _ausgeben('json')).pack(side='left')
    _button(fenster, _reihe_aus, t('s_lg_aus_csv'),
           lambda: _ausgeben('csv')).pack(side='left', padx=(8, 0))
    _button(fenster, _reihe_aus, t('s_lg_einlesen'),
           _einlesen).pack(side='left', padx=(8, 0))
    _button(fenster, _reihe_aus, t('s_lg_leeren'), _lager_leeren,
           danger=True).pack(side='left', padx=(24, 0))
    _body_text(innen, t('s_hl_aus_hilfe'), fenster.f_small, inset=48,
                fill='x', padx=24, pady=(0, 20))

    neu_zeichnen()


def _trade_table(fenster, eltern, posten, preis_von, loeschen,
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
        tk.Label(tabelle, text=titel, bg=BG, fg=SUB, font=fenster.f_small,
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
        grund = ACCENT if offen else (SURFACE if nummer % 2 == 0 else BG)
        vorne = BG if offen else FG
        blass = BG if offen else SUB

        felder = (
            (p['ware'], vorne, 'w'),
            (p.get('ort') or '—', blass, 'w'),
            # ⚠ Nur die Zahl — die Einheit steht in der Spaltenüberschrift.
            # „100 SCU" in jeder Zeile wiederholt, was darüber schon steht,
            # und schiebt die Zahlen auseinander.
            (_amount_text(p.get('menge') or 0), vorne, 'e'),
            (_money(preis) if preis else '—', blass, 'e'),
            (_money(wert) if wert else '—', vorne, 'e'),
        )
        zellen = []
        for spalte, (text, farbe, seite) in enumerate(felder):
            # ⚠⚠ **Der Spaltenabstand gehört ins Label (`padx=`), nicht ins
            # Raster.** Mit `grid(padx=…)` liegt zwischen den Zellen eine Lücke
            # in der Seitenfarbe — der Zebra-Streifen zerfällt dann in einzelne
            # Flecken statt einer durchgehenden Zeile. So gesehen im ersten
            # Bau der Tabelle.
            lbl = tk.Label(tabelle, text=text, bg=grund, fg=farbe, padx=8,
                           font=fenster.f_small, anchor=seite, cursor='hand2')
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
                         font=fenster.f_small, cursor='hand2', padx=8)
        kreuz.grid(row=nummer + 1, column=5, sticky='ew', ipady=3)
        kreuz.bind('<Button-1>', lambda _e, n=nummer: loeschen(n))
        kreuz.bind('<Enter>', lambda _e, w=kreuz: w.configure(fg=RED))
        kreuz.bind('<Leave>', lambda _e, w=kreuz, f=blass: w.configure(fg=f))

    return gesamt


def _view_angle(fenster, rahmen):
    """Welcher Blickwinkel passt — und wo müsste man dafür sitzen?

    ## Die Rechnung

    Es gibt genau einen Blickwinkel, bei dem das Bild so groß erscheint wie
    das Gezeigte in Wirklichkeit wäre — wenn der Bildschirm im Auge denselben
    Winkel einnimmt wie das, was er darstellt:

        Winkel = 2 · arctan( Bildschirmbreite / (2 · Sitzabstand) )

    ## ⚠ Warum von Hand ausgemessen wird

    Am 06.09.2026 an einem Aufbau mit drei Bildschirmen gemessen: Tk meldet
    für `winfo_screenmmwidth()` **1640 mm** — das ist der gesamte Desktop über
    alle drei Geräte, nicht der Bildschirm, auf dem gespielt wird. Die
    EDID-Angabe des Geräts (1193 mm) wäre richtig, ist aber nur unter X11
    erreichbar und bei manchen Geräten schlicht falsch.

    Eine Bankkarte anzuhalten dauert zehn Sekunden, stimmt überall und
    braucht keine einzige systemabhängige Zeile.
    """
    from . import fov as fov_modul

    # ⚠⚠ **Kein `messagebox`.** Der Dialog des Betriebssystems ist ein weißer
    # Kasten mit grauen Knöpfen mitten in einem dunklen Fenster — er sieht
    # nicht nach diesem Programm aus. `frage_stellen()` gibt es genau dafür,
    # und es war schon da: Für die Bergung wurde derselbe Fehler bereits
    # einmal behoben. Wer ein neues Fenster baut, benutzt die Vorgabe.
    class messagebox:
        """Dieselben Aufrufe wie `tkinter.messagebox`, nur im Programmstil."""

        @staticmethod
        def showinfo(titel, text):
            from .main_window import ask_yes_no
            # ⚠ Das TOPLEVEL übergeben, nicht den Seitenrahmen. `ask_yes_no`
            # setzt den Dialog mittig über sein Elternteil — der Seitenrahmen
            # beginnt aber erst rechts neben der Reiterleiste, und der Dialog
            # landete dadurch unten rechts statt in der Fenstermitte.
            ask_yes_no(rahmen.winfo_toplevel(), titel, text, only_ok=True)

        @staticmethod
        def showwarning(titel, text):
            from .main_window import ask_yes_no
            # ⚠ Das TOPLEVEL übergeben, nicht den Seitenrahmen. `ask_yes_no`
            # setzt den Dialog mittig über sein Elternteil — der Seitenrahmen
            # beginnt aber erst rechts neben der Reiterleiste, und der Dialog
            # landete dadurch unten rechts statt in der Fenstermitte.
            ask_yes_no(rahmen.winfo_toplevel(), titel, text, only_ok=True)

        @staticmethod
        def askyesno(titel, text):
            from .main_window import ask_yes_no
            return ask_yes_no(rahmen.winfo_toplevel(), titel, text)

    _heading(fenster, rahmen, t('hf_blickwinkel'), t('s_fv_lead'))
    innen = _scroll_area(rahmen)
    _body_text(innen, t('s_fv_erklaerung'), fenster.f_small, fill='x')

    inhalt = tk.Frame(innen, bg=BG)
    inhalt.pack(fill='both', expand=True, padx=24, pady=(4, 12))

    zustand = {'abstand': tk.StringVar(value='')}

    def _gespeichertes():
        daten = fov_modul.gespeichert()
        return (daten.get('fov_mm_je_pixel'), daten.get('fov_pixelbreite'),
                daten.get('fov_abstand_mm'))

    def _fertig_gemessen(karte_px, bildschirm_px, vollbild=True):
        mm_je_pixel = fov_modul.mm_pro_pixel(karte_px)
        if not mm_je_pixel:
            return
        # ⚠ Ohne echtes Vollbild ist `bildschirm_px` die FENSTERbreite, nicht
        # die des Bildschirms — die ganze Rechnung wäre falsch. Dann lieber
        # nichts speichern und es sagen.
        if not vollbild:
            _notice(fenster, t('hf_blickwinkel'),
                                   t('s_fv_kein_vollbild'))
            return
        fov_modul.merken(mm_je_pixel=mm_je_pixel, pixelbreite=bildschirm_px)
        breite = fov_modul.bildschirmbreite_mm(bildschirm_px, mm_je_pixel)
        _notice(fenster, 
            t('hf_blickwinkel'),
            t('s_fv_gespeichert').format((breite or 0) / 10.0))
        _auffrischen()

    def _messen():
        from . import fov_window
        mm_je_pixel, pixelbreite, _abstand = _gespeichertes()
        start = None
        if mm_je_pixel:
            # Dort weitermachen, wo zuletzt aufgehört wurde — wer nachjustiert,
            # soll nicht wieder bei einer beliebigen Größe anfangen.
            start = fov_modul.KARTE_BREITE_MM / mm_je_pixel
        # ⛔⛔ **Rückstand der Sprachumstellung.** Hier standen bis zum
        # 14.09.2026 noch die deutschen Schlüsselwörter `schrift=`, `klein=`
        # und `startbreite=`; `calibrate()` heißt seine Parameter längst
        # `font`, `small`, `start_width`. Der Klick auf „Neu ausmessen" warf
        # deshalb `TypeError: calibrate() got an unexpected keyword argument
        # 'schrift'` — es passierte schlicht nichts. Gefunden von Blackd0g84
        # (KRT). Dieselbe Bauart wie der `CurvePlot(breite=…)`-Fehler auf der
        # Achsen-Seite: Ein Schlüsselwort gehört der **empfangenden**
        # Funktion, und ein falsches fällt erst beim Aufruf auf.
        fov_window.calibrate(rahmen, _fertig_gemessen,
                             font=fenster.f_base, small=fenster.f_small,
                             start_width=start)

    def _abstand_merken(*_e):
        text = (zustand['abstand'].get() or '').replace(',', '.').strip()
        try:
            zentimeter = float(text)
        except ValueError:
            return
        if zentimeter > 0:
            fov_modul.merken(abstand_mm=zentimeter * 10.0)
        _auffrischen()

    def _auffrischen():
        # Dieselbe Begründung wie auf der Achsen-Seite: Ein Neuaufbau darf
        # die Rollstelle nicht verlieren.
        _keep_scroll(inhalt, _neu_bauen)

    def _neu_bauen():
        for kind in list(inhalt.winfo_children()):
            kind.destroy()

        mm_je_pixel, pixelbreite, abstand_mm = _gespeichertes()
        breite_mm = (fov_modul.bildschirmbreite_mm(pixelbreite, mm_je_pixel)
                     if (mm_je_pixel and pixelbreite) else None)

        # --- 1. Ausmessen ----------------------------------------------
        reihe = tk.Frame(inhalt, bg=BG)
        reihe.pack(fill='x', pady=(10, 0))
        _button(fenster, reihe,
               t('s_fv_neu_messen') if breite_mm else t('s_fv_messen'),
               _messen, strong=not breite_mm).pack(side='left')

        if not breite_mm:
            _body_text(inhalt, t('s_fv_nicht_gemessen'), fenster.f_small,
                        fill='x')
            return

        _wert_zeile(inhalt, t('s_fv_breite'), '%.1f cm' % (breite_mm / 10.0))

        # --- 2. Sitzabstand --------------------------------------------
        zeile = tk.Frame(inhalt, bg=BG)
        zeile.pack(fill='x', pady=(12, 0))
        links = tk.Frame(zeile, bg=BG)
        links.pack(side='left', fill='x', expand=True)
        tk.Label(links, text=t('s_fv_abstand'), bg=BG, fg=FG,
                 font=fenster.f_bold, anchor='w').pack(fill='x')
        tk.Label(links, text=t('s_fv_abstand_hilfe'), bg=BG, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(fill='x')
        if abstand_mm:
            zustand['abstand'].set('%g' % round(abstand_mm / 10.0, 1))
        feld = tk.Entry(zeile, textvariable=zustand['abstand'], width=6,
                        bg=SURFACE, fg=FG, insertbackground=FG,
                        font=fenster.f_base, relief='flat', justify='right')
        feld.pack(side='right', padx=(16, 0), ipady=3)
        # ⚠ An diesem Feld hängt schon ein `<FocusOut>`, das den Wert
        # speichert. Das verträgt sich: `_abstand_merken` steigt bei leerem
        # Text aus (`float('')` wirft), und genau leer ist die Variable,
        # solange der Hinweis steht.
        fields.hint(feld, zustand['abstand'], t('s_pl_abstand'),
                       normal=FG, grey=SUB)
        feld.bind('<Return>', _abstand_merken)
        # ⚠⚠ `add='+'` ist hier PFLICHT. Ohne das ersetzt diese Bindung die,
        # die `fields.hint()` gerade gesetzt hat — und der Hinweis kommt
        # nach dem ersten Verlassen des Feldes nie wieder. Vom Prüfer
        # nachgestellt (12.09.2026): leeres Feld → Return → Fokus weg, Hinweis
        # bleibt verschwunden.
        #
        # Die Falle gilt für JEDES Feld mit eigener Bindung: `bind()` ohne
        # `add='+'` wirft die vorhandene weg, ohne sich zu beschweren.
        feld.bind('<FocusOut>', _abstand_merken, add='+')
        tk.Frame(inhalt, bg=LINE, height=1).pack(fill='x', pady=(12, 0))

        if not abstand_mm:
            return

        # --- 3. Das Ergebnis -------------------------------------------
        neutral = fov_modul.blickwinkel(breite_mm, abstand_mm)
        if neutral is None:
            return
        _wert_zeile(inhalt, t('s_fv_neutral'), '%.1f°' % neutral,
                    hilfe=t('s_fv_neutral_hilfe'), farbe=ACCENT)

        spiel = fov_modul.spiel_einstellung()
        if spiel.get('fov') is None:
            _body_text(inhalt, t('s_fv_kein_spielwert'), fenster.f_small,
                        fill='x')
            return

        _wert_zeile(inhalt, t('s_fv_im_spiel'), '%.1f°' % spiel['fov'])

        # ⭐ Der Optimalpunkt: nicht „stell dein Spiel um", sondern „so weit
        # müsstest du sitzen". Wer sein FOV kennt und mag, will seinen Stuhl
        # rücken — nicht seine Einstellung.
        optimal_mm = fov_modul.abstand_fuer(breite_mm, spiel['fov'])
        if optimal_mm is None:
            return
        _wert_zeile(inhalt, t('s_fv_optimalpunkt'),
                    '%.0f cm' % (optimal_mm / 10.0),
                    hilfe=t('s_fv_optimalpunkt_hilfe'))

        # --- 4. Rot / Gelb / Grün --------------------------------------
        note, abweichung = fov_modul.bewertung(abstand_mm, optimal_mm)
        farbe = {'gruen': ACCENT, 'gelb': GOLD}.get(note, RED)
        unterschied_cm = abs(abstand_mm - optimal_mm) / 10.0
        if note == 'gruen':
            text = t('s_fv_passt_gut')
        elif abweichung > 0:
            text = t('s_fv_zu_weit').format(unterschied_cm)
        else:
            text = t('s_fv_zu_nah').format(unterschied_cm)

        kasten = tk.Frame(inhalt, bg=SURFACE)
        kasten.pack(fill='x', pady=(16, 0))
        streifen = tk.Frame(kasten, bg=farbe, width=4)
        streifen.pack(side='left', fill='y')
        tk.Label(kasten, text=text, bg=SURFACE, fg=farbe,
                 font=fenster.f_bold, anchor='w',
                 padx=12, pady=10).pack(side='left', fill='x', expand=True)

        _body_text(inhalt, t('s_fv_hinweis_deutung'), fenster.f_small,
                    fill='x')

    def _wert_zeile(eltern, bezeichnung, wert, hilfe='', farbe=FG):
        zeile = tk.Frame(eltern, bg=BG)
        zeile.pack(fill='x', pady=(12, 0))
        links = tk.Frame(zeile, bg=BG)
        links.pack(side='left', fill='x', expand=True)
        tk.Label(links, text=bezeichnung, bg=BG, fg=FG, font=fenster.f_bold,
                 anchor='w').pack(fill='x')
        if hilfe:
            tk.Label(links, text=hilfe, bg=BG, fg=SUB, font=fenster.f_small,
                     anchor='w', justify='left').pack(fill='x')
        tk.Label(zeile, text=wert, bg=BG, fg=farbe, font=fenster.f_bold,
                 anchor='e').pack(side='right', padx=(16, 0))
        tk.Frame(eltern, bg=LINE, height=1).pack(fill='x', pady=(12, 0))

    _auffrischen()
    fenster.on_show['blickwinkel'] = _auffrischen


def _axes(fenster, rahmen):
    """Totzone, Sättigung und Kurve — und was davon überhaupt noch gilt.

    ## Warum diese Seite neben „Joysticks" steht

    Der Nachbarreiter beantwortet „welcher Stick ist js1 und was liegt
    darauf". Hier geht es um etwas anderes: **wie** die Achse reagiert. Das
    sind zwei Fragen, die im Spiel auch an zwei verschiedenen Stellen stehen.

    ## ⭐ Der Befund zuerst, die Einstellung danach

    Ganz oben steht, was **nicht mehr wirkt**. Das ist die Auskunft, die es
    sonst nirgends gibt: Star Citizen hängt Totzone und Sättigung an die
    Kennung des Geräts. Bekommt ein Stick eine neue (anderer USB-Anschluss,
    neue Firmware), legt das Spiel ihn als neues Gerät an — die alten Werte
    bleiben in der Datei stehen und tun nichts. Im Spiel ist das nicht zu
    sehen, weil dort nur das aktuelle Gerät auftaucht.

    An einem echten Aufbau gemessen (06.09.2026): drei solcher Fälle, darunter
    eine Sättigung, die der Spieler eingestellt hatte und die seit einem
    Kennungswechsel wirkungslos war. Beide Sticks liefen dadurch unterschiedlich
    scharf, bei identischer Beschriftung.

    ⚠ **Geschrieben wird nur auf Knopfdruck** — wie im ganzen Joystick-Teil.
    """
    from . import curves
    from .curve_plot import CurvePlot

    # ⚠⚠ **Kein `messagebox`** — siehe die Begründung auf der Seite
    # „Blickwinkel". Jedes Fenster sieht aus wie das Programm, auch ein neues.
    class messagebox:
        """Dieselben Aufrufe wie `tkinter.messagebox`, nur im Programmstil."""

        @staticmethod
        def showinfo(titel, text):
            from .main_window import ask_yes_no
            # ⚠ Das TOPLEVEL übergeben, nicht den Seitenrahmen. `ask_yes_no`
            # setzt den Dialog mittig über sein Elternteil — der Seitenrahmen
            # beginnt aber erst rechts neben der Reiterleiste, und der Dialog
            # landete dadurch unten rechts statt in der Fenstermitte.
            ask_yes_no(rahmen.winfo_toplevel(), titel, text, only_ok=True)

        @staticmethod
        def showwarning(titel, text):
            from .main_window import ask_yes_no
            # ⚠ Das TOPLEVEL übergeben, nicht den Seitenrahmen. `ask_yes_no`
            # setzt den Dialog mittig über sein Elternteil — der Seitenrahmen
            # beginnt aber erst rechts neben der Reiterleiste, und der Dialog
            # landete dadurch unten rechts statt in der Fenstermitte.
            ask_yes_no(rahmen.winfo_toplevel(), titel, text, only_ok=True)

        @staticmethod
        def askyesno(titel, text):
            from .main_window import ask_yes_no
            return ask_yes_no(rahmen.winfo_toplevel(), titel, text)

    _heading(fenster, rahmen, t('hf_achsen'), t('s_ac_lead'))
    innen = _scroll_area(rahmen)
    _body_text(innen, t('s_ac_hinweis'), fenster.f_small, fill='x')

    # ⚠⚠⚠ **Liegen mehrere Belegungsdateien da, muss man das sehen.** Am
    # 06.09.2026 las der Watcher die falsche von zweien und zeigte eine
    # Empfindlichkeit von 2, während im Spiel überall 1,00 stand — und weil
    # in der alten Datei die Geräte anders durchnummeriert waren, die Werte
    # des falschen Sticks dazu. Nichts davon war zu sehen.
    #
    # Gelesen wird jetzt die zuletzt geänderte (`joysticks.alle_actionmaps`).
    # Das allein reicht aber nicht: Solange die Karteileiche danebenliegt,
    # kann sie beim nächsten Kopiervorgang wieder die jüngere sein. Deshalb
    # steht hier, was gefunden wurde — und welche davon zählt.
    from . import joysticks as _js_dateien
    _dateien = _js_dateien.alle_actionmaps()
    if len(_dateien) > 1:
        _kasten = tk.Frame(innen, bg=SURFACE, highlightthickness=1,
                           highlightbackground=GOLD)
        _kasten.pack(fill='x', padx=24, pady=(8, 4))
        tk.Label(_kasten, text=t('s_ac_doppelt'), bg=SURFACE, fg=FG,
                 font=fenster.f_small, justify='left',
                 wraplength=760).pack(anchor='w', padx=12, pady=(10, 6))
        for _nr, _weg in enumerate(_dateien):
            try:
                _zeit = time.strftime(
                    '%d.%m.%Y %H:%M', time.localtime(os.path.getmtime(_weg)))
            except OSError:
                _zeit = '—'
            # ⚠ Der volle Pfad, nicht nur der Dateiname — die Dateien heißen
            # alle gleich, sie unterscheiden sich nur im Ordner davor. Genau
            # diese Unterscheidung ist der ganze Punkt.
            tk.Label(_kasten,
                     text='%s  ·  %s  ·  %s'
                          % (t('s_ac_gelesen') if _nr == 0
                             else t('s_ac_liegt'), _zeit, _weg),
                     bg=SURFACE, fg=ACCENT if _nr == 0 else SUB,
                     font=fenster.f_small, justify='left',
                     wraplength=760).pack(anchor='w', padx=12, pady=(0, 6))

    # ⚠ Wie bei den Joysticks: Was neu gezeichnet wird, steht in einem eigenen
    # Rahmen. Der Kopftext darüber bleibt stehen.
    inhalt = tk.Frame(innen, bg=BG)
    inhalt.pack(fill='both', expand=True, padx=24, pady=(4, 12))

    # Die gewählte Achse — als Wörterbuch, damit die Rückrufe sie ändern
    # können, ohne `nonlocal` durch drei Ebenen zu reichen.
    wahl = {'kennung': '', 'achse': '', 'ganz': False}

    def _two_decimals(wert):
        """Eine Zahl fürs Auge: zwei Stellen, ohne Nullenschwanz."""
        if wert is None:
            return '—'
        return ('%.2f' % wert).rstrip('0').rstrip('.') or '0'

    # Der Fingerabdruck der zuletzt gezeichneten Lage — siehe `_beim_zeigen`.
    zuletzt_achsen = {'stand': None}

    def _beim_zeigen():
        """Beim Anzeigen der Seite: nur neu zeichnen, wenn nötig.

        ⚠⚠ `_auffrischen()` baut den ganzen Inhalt neu auf — alle Kinder
        zerstören, alles wieder hinstellen. Beim Ändern einer Einstellung ist
        das richtig. Bei einem **Seitenwechsel** ist es reine Arbeit ohne
        Ergebnis: Gemessen am 12.09.2026 **313 ms bei jedem Klick**, und das
        ist Teil dessen, was als „wirkt lahm" ankam.

        ⭐ Der Fingerabdruck ist die **Datenlage selbst**, nicht ein
        Zeitstempel: Sind die Zahlen gleich, wäre auch das Bild gleich. Und
        wenn eine Quelle einmal etwas Unsortiertes liefert, ist der schlimmste
        Fall ein **überflüssiger** Neuaufbau — nie ein veraltetes Bild. Das ist
        die richtige Richtung für einen Irrtum.

        ⚠⚠ **Er braucht ALLE Quellen, aus denen gezeichnet wird — nicht nur
        die naheliegendste.** Die erste Fassung nahm allein
        `curves.summary()`. Darin stehen die Achseneinstellungen, aber
        **nicht**, welche Funktion auf welcher Achse liegt. Vom Prüfer
        nachgestellt (12.09.2026): `v_pitch` von `js1_x` auf `js1_y` schieben,
        Exponent unverändert — die Zusammenfassung bleibt gleich, die Funktion
        gehört danach zu einer anderen Achse, und Beschriftung, CurvePlot und
        Regler wären auf dem alten Stand geblieben.

        ⭐ **Statt die einzelnen Abfragen nachzubauen, steht hier die QUELLE.**
        `zusammenfassung()`, `funktionen_je_achse()` und `spielachsen_auf()`
        lesen alle dieselbe Datei — die `actionmaps.xml` des Spielers. Ein
        Abdruck über ihren Inhalt deckt damit **alles** ab, was diese Seite von
        dort zeigt, auch das, woran hier gerade niemand denkt.

        Einzelne Abfragen aufzuzählen wäre die zweite Wahrheit, die irgendwann
        unvollständig wird — genau der Fehler, den die erste Fassung hatte.

        ⚠ Dazu `device_set.sets()`: eine **eigene** Quelle (die Gerätesätze
        stehen woanders), und sie wird auf derselben Seite angezeigt.
        """
        import hashlib
        from . import device_set as _gs
        from . import joysticks as _js
        try:
            weg = _js._pfad_actionmaps()
            roh = b''
            if weg and os.path.exists(weg):
                with open(weg, 'rb') as f:
                    roh = f.read()
            # ⚠⚠ Der XML-Hash allein reicht NICHT. `zusammenfassung()` hängt
            # zusätzlich an `gueltige_kennungen()` → `joysticks.geraete()`,
            # und die kommen aus dem **Spielprotokoll**, nicht aus der XML.
            # Vom Prüfer nachgestellt: dieselbe XML, eine Gerätekennung im
            # Protokoll ergänzt — ein Geräteblock wechselt von `aktiv=False`
            # auf `True`, der Hash bleibt gleich. Geräteauswahl und Bewertung
            # wären veraltet geblieben.
            #
            # Deshalb steht die Zusammenfassung **zusätzlich** drin: Sie
            # deckt die Protokoll-Seite ab, der Hash die Datei-Seite.
            stand = (hashlib.sha1(roh).hexdigest(),
                     repr(curves.summary()),
                     repr(_gs.sets()),
                     repr(wahl))
        except Exception as ausnahme:
            fehler.merken('seiten.achsen_stand', ausnahme)
            stand = None
        if stand is not None and stand == zuletzt_achsen['stand']:
            return
        zuletzt_achsen['stand'] = stand
        _auffrischen()

    def _auffrischen():
        """Neu zeichnen, ohne dass die Seite nach oben springt.

        ⚠ Der Inhalt wird bei jedem Geräte- und Achsenwechsel komplett neu
        gebaut — danach steht die Rollfläche wieder bei null, und wer unten
        bei den Reglern war, landet oben. Gemeldet 06.09.2026: „beim
        Anklicken einer Option springt das Fenster immer nach oben."
        `_rollstelle_halten` gibt es im Projekt genau dafür.
        """
        _keep_scroll(inhalt, _neu_bauen)

    def _neu_bauen():
        for kind in list(inhalt.winfo_children()):
            kind.destroy()

        ueberblick = curves.summary()
        aktive = [b for b in ueberblick['bloecke']
                  if b['aktiv'] and b['kennung']]

        if not ueberblick['bloecke']:
            _body_text(inhalt, t('s_ac_keine'), fenster.f_base, fill='x')
            return

        # --- 1. Geräteauswahl ------------------------------------------
        if not aktive:
            _body_text(inhalt, t('s_ac_keine'), fenster.f_base, fill='x')
            return
        if not wahl['kennung'] or not any(b['kennung'] == wahl['kennung']
                                          for b in aktive):
            wahl['kennung'] = aktive[0]['kennung']

        leiste = tk.Frame(inhalt, bg=BG)
        leiste.pack(fill='x', pady=(16, 0))
        for block in aktive:
            gewaehlt = block['kennung'] == wahl['kennung']

            def _waehlen(k=block['kennung']):
                wahl['kennung'] = k
                wahl['achse'] = ''
                _auffrischen()

            knopf = tk.Label(leiste, text=block['name'], bg=BAR if gewaehlt
                             else SURFACE, fg=BG if gewaehlt else FG,
                             font=fenster.f_small, padx=10, pady=4,
                             cursor='hand2')
            if gewaehlt:
                knopf.configure(bg=ACCENT)
            knopf.pack(side='left', padx=(0, 6))
            knopf.bind('<Button-1>', lambda _e, f=_waehlen: f())

        gewaehlter = [b for b in aktive if b['kennung'] == wahl['kennung']][0]

        # --- 3. Die Achsen des gewählten Geräts ------------------------
        vorhandene = [a for a in curves.AXES if a in gewaehlter['achsen']]
        if not vorhandene:
            _body_text(inhalt, t('s_ac_keine_werte'), fenster.f_small,
                        fill='x')
            return
        if wahl['achse'] not in vorhandene:
            wahl['achse'] = vorhandene[0]

        unten = tk.Frame(inhalt, bg=BG)
        unten.pack(fill='both', expand=True, pady=(14, 0))

        # ⚠ Erst das Feste (die Kurve rechts), dann die wachsende Liste —
        # sonst schiebt die Liste das Bild aus dem Fenster. Dieselbe Falle
        # wie beim Speichern-Knopf im Einstellungsfenster.
        rechts = tk.Frame(unten, bg=BG)
        rechts.pack(side='right', anchor='n', padx=(18, 0))
        links = tk.Frame(unten, bg=BG)
        links.pack(side='left', fill='both', expand=True)

        werte = gewaehlter['achsen'].get(wahl['achse']) or {}
        plot = CurvePlot(rechts, width=240, height=240, whole=wahl['ganz'],
                          font=fenster.f_base, small=fenster.f_small)
        plot.pack()
        plot.show(totzone=werte.get('deadzone'),
                    saturation=werte.get('saturation'),
                    exponent=_exponent_fuer(ueberblick, gewaehlter,
                                            wahl['achse']))

        schalter = tk.Frame(rechts, bg=BG)
        schalter.pack(fill='x', pady=(8, 0))

        def _umschalten():
            wahl['ganz'] = not wahl['ganz']
            _auffrischen()

        def _gross():
            from .curve_plot import show_large
            show_large(rahmen, '%s — %s %s' % (t('s_ac_titel_gross'),
                                                 gewaehlter['name'],
                                                 wahl['achse']),
                         totzone=werte.get('deadzone'),
                         saturation=werte.get('saturation'),
                         exponent=_exponent_fuer(ueberblick, gewaehlter,
                                                 wahl['achse']),
                         # ⛔ Rückstand der Sprachumstellung: `show_large`
                         # heißt seine Parameter `whole`, `font`, `small`.
                         whole=wahl['ganz'],
                         font=fenster.f_base, small=fenster.f_small)

        _button(fenster, schalter,
               t('s_kv_quadrant') if wahl['ganz'] else t('s_kv_ganz'),
               _umschalten).pack(side='left')
        _button(fenster, schalter, t('s_ac_gross'),
               _gross).pack(side='left', padx=(8, 0))

        # ⚠⚠ **Einmal lesen, nicht je Zeile.** Welche Flugfunktion auf welcher
        # Achse liegt, steht in der Belegungsdatei; sie für jede der acht
        # Zeilen neu zu lesen hieße, eine 20-KB-Datei achtmal je Zeichnen
        # anzufassen.
        nummer_fuer_liste = None
        for _spiel in curves.game_axes():
            if (_spiel['art'] == 'joystick'
                    and _spiel['kennung'] == gewaehlter['kennung']):
                nummer_fuer_liste = _spiel['nummer']
                break
        belegt = ({} if nummer_fuer_liste is None
                  else curves.functions_per_axis(nummer_fuer_liste,
                                                  vorhandene))

        for achse in vorhandene:
            _achsenzeile(links, gewaehlter, achse, belegt.get(achse) or [])

        # --- 4. Die Regler ---------------------------------------------
        #
        # ⚠⚠ **Der Regler schreibt NICHT.** Er ändert nur die Vorschau; erst
        # der Knopf darunter fasst die Datei an. Dieselbe Linie wie im
        # Belegungsfenster: Zwischen „eingestellt" und „geschrieben" gehört
        # ein Mensch — an dieser Datei hängt die komplette Steuerung, und ein
        # Regler löst beim Ziehen dutzende Ereignisse aus.
        regler = tk.Frame(links, bg=BG)
        regler.pack(fill='x', pady=(16, 0))

        anzeigen = {}

        def _vorschau(_wert=None):
            """Kurve neu zeichnen — ohne die Seite neu zu bauen.

            ⚠ Hier **nicht** `_auffrischen()` rufen: Das baut die Regler neu,
            und der Griff, den die Maus gerade hält, wäre weg. Dieselbe Falle
            wie beim Suchfeld auf der Joystick-Seite.
            """
            for name, teil in anzeigen.items():
                teil['anzeige'].configure(text=_two_decimals(teil['var'].get()))
            plot.show(totzone=anzeigen['deadzone']['var'].get(),
                        saturation=anzeigen['saturation']['var'].get(),
                        exponent=_exponent_fuer(ueberblick, gewaehlter,
                                                wahl['achse']))
            _stand_zeigen()

        def _speichern_alles():
            """Totzone, Sättigung und jede Empfindlichkeit in einem Zug.

            ⚠ Zwei verschiedene Ziele in derselben Schleife: Totzone und
            Sättigung gehören zur **physischen** Achse und laufen über
            `curves.apply()`, die Empfindlichkeit zur **Spielachse** und über
            `kurven.spiel_setzen()`. Woran ein Eintrag hängt, sagt sein Feld
            `spiel` — steht dort etwas, ist es eine Spielachse.
            """
            aenderungen = _offen()
            if not aenderungen:
                _notice(fenster, t('hf_achsen'), t('s_ac_nichts_offen'))
                return
            for schluessel, neu in aenderungen.items():
                teil = anzeigen[schluessel]
                if teil.get('spiel'):
                    nummer, achse = teil['spiel']
                    erfolg, meldung, _ = curves.apply_to_game(
                        nummer, achse, 'exponent', neu)
                else:
                    erfolg, meldung, _ = curves.apply(
                        gewaehlter['kennung'], wahl['achse'], schluessel, neu)
                if not erfolg:
                    _notice(fenster, t('hf_achsen'), t(meldung))
                    return
            _notice(fenster, t('hf_achsen'), t('s_ac_gespeichert'))
            _auffrischen()

        def _reglerzeile(eigenschaft, beschriftung, ist):
            """Ein Regler für eine Eigenschaft.

            ⚠⚠ **„Nicht gesetzt" ist nicht dasselbe wie 0.** Fehlt die
            Sättigung in der Datei, gilt im Spiel **1,0** — der volle Weg.
            Ein Regler, der in diesem Fall links auf 0 stünde, wäre eine
            Falle: Wer ihn anfasst und speichert, schriebe Sättigung 0 und
            hätte danach praktisch keine Kontrolle mehr über den Stick.
            Deshalb hat jede Eigenschaft ihren eigenen Ruhewert.
            """
            ruhe = curves.DEFAULT.get(eigenschaft, 0.0)
            zeile = tk.Frame(regler, bg=BG)
            zeile.pack(fill='x', pady=(6, 0))
            tk.Label(zeile, text=beschriftung, bg=BG, fg=FG,
                     font=fenster.f_small, anchor='w',
                     width=14).pack(side='left')
            var = tk.DoubleVar(value=(ruhe if ist is None else ist))
            # ⚠ Die Zahl steht RECHTS vom Regler und wird zuerst gepackt —
            # sonst nimmt der Regler ihr den Platz weg, sobald das Fenster
            # schmal wird, und der Wert ist nicht mehr zu lesen.
            anzeige = tk.Label(zeile, text=_two_decimals(ist), bg=BG, fg=ACCENT,
                               font=fenster.f_small, width=5, anchor='e')
            anzeige.pack(side='right', padx=(8, 0))
            schieber = tk.Scale(zeile, from_=0.0, to=1.0, resolution=0.001,
                                orient='horizontal', variable=var,
                                command=_vorschau, showvalue=False,
                                bg=BG, fg=FG, troughcolor=SURFACE,
                                activebackground=ACCENT, highlightthickness=0,
                                bd=0, sliderrelief='flat', length=200)
            schieber.pack(side='left', fill='x', expand=True)
            # ⚠⚠ **Den Ausgangswert NACH dem Rastern lesen.**
            #
            # `tk.Scale` zieht jeden Wert auf sein Raster: Aus dem echten
            # 0.098999992 wurde beim Aufbau 0.099 — und die Seite hielt das
            # für eine Änderung, die niemand gemacht hatte. Die Folge war
            # sichtbar: „Ungespeicherte Änderung" stand sofort beim Öffnen da,
            # und beim Speichern landeten Werte in der Datei, die der Spieler
            # nie angefasst hatte (gemeldet 06.09.2026: „die Werte sind immer
            # wieder die alten").
            #
            # Verglichen wird deshalb gegen den **gerasterten** Startwert.
            # Fasst niemand den Regler an, gibt es keine Änderung — und der
            # krumme Originalwert bleibt unangetastet in der Datei stehen.
            anzeigen[eigenschaft] = {'var': var, 'anzeige': anzeige,
                                     'ist': ist, 'ruhe': ruhe,
                                     'start': var.get()}

        _reglerzeile('deadzone', t('s_kv_totzone'), werte.get('deadzone'))
        _reglerzeile('saturation', t('s_kv_saettigung'),
                     werte.get('saturation'))

        stand = tk.Label(links, text='', bg=BG, fg=SUB,
                         font=fenster.f_small, anchor='w')
        stand.pack(fill='x', pady=(10, 0))

        # --- Empfindlichkeit je Spielachse -----------------------------
        #
        # ⚠⚠ **Sie hängt NICHT an der physischen Achse.** Totzone und
        # Sättigung gelten für den Stickweg `y`; die Empfindlichkeit gilt für
        # die Funktion, die darauf liegt — und das sind oft **mehrere**.
        # Gemessen lagen auf `js2_y` gleichzeitig „Nicken" und „Schub
        # hoch/runter", jede mit eigenem Wert. Ein Regler je Stickachse wäre
        # falsch; deshalb bekommt jede Funktion ihren eigenen.
        empf_rahmen = tk.Frame(links, bg=BG)
        empf_rahmen.pack(fill='x')

        def _offen():
            """Was hat der Spieler geändert, ohne zu speichern?

            ⚠ Steht ein Regler auf seinem Ruhewert und war die Eigenschaft
            vorher gar nicht gesetzt, ist das **keine** Änderung. Sonst
            meldete die Seite bei jedem Öffnen „ungespeichert" und schriebe
            beim Speichern Werte in die Datei, die der Spieler nie angefasst
            hat — genau die Sorte stiller Eingriff, die dieses Werkzeug
            vermeiden soll.
            """
            heraus = {}
            for eigenschaft, teil in anzeigen.items():
                neu = round(teil['var'].get(), 4)
                # ⚠ Gegen den GERASTERTEN Startwert vergleichen, nicht gegen
                # den Wert aus der Datei — sonst gilt der Regler als bewegt,
                # sobald der echte Wert nicht auf sein Raster passt.
                if abs(teil['start'] - neu) > 1e-6:
                    heraus[eigenschaft] = neu
            return heraus

        def _stand_zeigen():
            stand.configure(text=t('s_ac_geaendert') if _offen() else '',
                            fg=GOLD)

        # ⚠ **Erst HIER, nicht weiter oben.** Die Regler der Empfindlichkeit
        # tragen sich in dasselbe `anzeigen` ein und rufen `_stand_zeigen`
        # beim Ziehen — beides muss also schon dastehen. Der Aufruf stand
        # zuerst oben beim Rahmen und lief damit gegen eine Funktion, die es
        # zu dem Zeitpunkt noch nicht gab.
        _empfindlichkeit(empf_rahmen, gewaehlter, wahl['achse'],
                         plot, anzeigen, _stand_zeigen)

        # ⚠ Der frühere lokale Import des Tk-Dialogs ist hier weggefallen: Er
        # verdeckte den Dialog im Programmstil, der weiter oben in dieser
        # Funktion definiert ist, und holte den weißen System-Kasten zurück.
        # (Der Aufruf steht bewusst nicht ausgeschrieben — die Wache im
        # Selbsttest sucht nach dem Wortlaut und schlüge sonst an.)
        _speichern = _speichern_alles

        knopfreihe = tk.Frame(links, bg=BG)
        knopfreihe.pack(fill='x', pady=(10, 0))
        _button(fenster, knopfreihe, t('s_ac_speichern'), _speichern,
               strong=True).pack(side='left')
        _button(fenster, knopfreihe, t('s_ac_verwerfen'),
               _auffrischen).pack(side='left', padx=(8, 0))
        _body_text(links, t('s_ac_spiel_zu'), fenster.f_small, fill='x')

        # --- 5. Zwei Sticks gleich einstellen --------------------------
        #
        # ⭐ Wer HOSAS fliegt, will auf beiden Seiten dasselbe Gefühl. Von
        # Hand sind das zwölf Mal dieselbe Zahl — und einmal vertippt fällt
        # es erst im Gefecht auf. Übertragen wird **alles auf einmal**, aber
        # nur für Achsen, die es auf beiden Geräten gibt: Ein Pedalsatz hat
        # kein `rotx`, und ein erfundener Wert wäre schlimmer als keiner.
        # ⚠⚠ **Gitter, keine Reihe.** Zwei Knöpfe mit vollem Gerätenamen sind
        # breiter als das Fenster, und Tk schneidet eine zu breite Reihe
        # **wortlos** ab — auf einem Bildschirmfoto stand rechts „RIGHT VPC
        # Stick WarBRD-D« übert…" und lief aus dem Fenster. `_knopfgitter`
        # bricht stattdessen um; es gibt es im Projekt genau dafür.
        andere = [b for b in aktive if b['kennung'] != gewaehlter['kennung']]
        if andere:
            tk.Frame(links, bg=LINE, height=1).pack(fill='x', pady=(18, 0))
            tk.Label(links, text=t('s_ac_kopf_angleichen'), bg=BG, fg=FG,
                     font=fenster.f_bold, anchor='w').pack(fill='x',
                                                           pady=(14, 0))
            _body_text(links, t('s_ac_lead_angleichen'), fenster.f_small,
                        fill='x')
            reihe2 = tk.Frame(links, bg=BG)
            reihe2.pack(fill='x', pady=(8, 0))
            gitter2 = []
            for ziel in andere:
                def _angleichen(z=ziel):
                    # ⚠ Die Arbeit macht `kurven.angleichen()` — hier steht
                    # nur die Rückfrage. Logik in einem Rückruf der Oberfläche
                    # lässt sich nicht prüfen; im Modul hat sie eine Prüfung.
                    if not _ask(fenster, 
                            t('hf_achsen'),
                            t('s_ac_angleichen_frage').format(
                                gewaehlter['name'], z['name'])
                            + '\n\n' + t('s_ac_spiel_zu')):
                        return
                    erfolg, meldung, anzahl = curves.align(
                        gewaehlter['kennung'], z['kennung'])
                    if not erfolg:
                        _notice(fenster, t('hf_achsen'), t(meldung))
                        return
                    _notice(fenster, 
                        t('hf_achsen'),
                        t('s_ac_angeglichen').format(anzahl))
                    _auffrischen()

                gitter2.append(_button(
                    fenster, reihe2,
                    t('s_ac_angleichen').format(ziel['name']), _angleichen))
            _button_grid(reihe2, gitter2)

        # --- 6. Belegungen über Kreuz tauschen -------------------------
        #
        # ⭐ Der Fall: Nach einem Neustart hat das Spiel die Nummern anders
        # vergeben, und die komplette Belegung sitzt auf der falschen Hand.
        # Getauscht wird nur, welche Kennung welche Nummer hat — die 400
        # Belegungszeilen bleiben unangetastet.
        if andere:
            tk.Label(links, text=t('s_ac_kopf_tauschen'), bg=BG, fg=FG,
                     font=fenster.f_bold, anchor='w').pack(fill='x',
                                                           pady=(16, 0))
            _body_text(links, t('s_ac_lead_tauschen'), fenster.f_small,
                        fill='x')
            reihe3 = tk.Frame(links, bg=BG)
            reihe3.pack(fill='x', pady=(8, 0))
            gitter3 = []
            for ziel in andere:
                def _tauschen(z=ziel):
                    if not _ask(fenster, 
                            t('hf_achsen'),
                            t('s_ac_tausch_frage').format(gewaehlter['name'],
                                                          z['name'])
                            + '\n\n' + t('s_ac_spiel_zu')):
                        return
                    from . import joysticks
                    erfolg, meldung, _ = joysticks.belegungen_tauschen(
                        gewaehlter['kennung'], z['kennung'])
                    if not erfolg:
                        _notice(fenster, t('hf_achsen'), t(meldung))
                        return
                    _notice(fenster, t('hf_achsen'), t('s_ac_getauscht'))
                    _auffrischen()

                gitter3.append(_button(
                    fenster, reihe3,
                    t('s_ac_tauschen').format(ziel['name']), _tauschen))
            _button_grid(reihe3, gitter3)

        # --- 7. Gerätesätze --------------------------------------------
        _saetze_block(links)

        # --- 8. Altbestand — ganz unten und zugeklappt ------------------
        _altbestand(links, ueberblick)

    def _altbestand(eltern, ueberblick):
        """Alte Einträge früherer Gerätenummern — bewusst unauffällig.

        ⚠⚠ **Das ist eine Auskunft, kein Auftrag.** Die erste Fassung stand
        in Gold ganz oben, hieß „Diese Einstellungen wirken nicht mehr" und
        hatte drei Knöpfe darunter — und wurde prompt als Fehlermeldung
        gelesen, die man wegklicken muss. Wer alle Knöpfe reihum drückt,
        überschreibt seine funktionierenden Werte mit alten, die sich
        untereinander widersprechen. Genau so ist es passiert.

        Deshalb: unten statt oben, zugeklappt statt offen, sachlicher Titel
        statt Warnfarbe, und der erste Satz sagt, dass nichts zu tun ist.
        """
        faelle = ueberblick['uebernehmbar']
        ok, _m, tote = curves.clean_up(count_only=True)
        anzahl = tote if ok else 0
        if not faelle and not anzahl:
            return

        tk.Frame(eltern, bg=LINE, height=1).pack(fill='x', pady=(18, 0))
        kopf = tk.Frame(eltern, bg=BG, cursor='hand2')
        kopf.pack(fill='x', pady=(12, 0))
        pfeil = icons.line(kopf, 'aufklappen', background=BG,
                              font=fenster.f_small)
        pfeil.pack(side='left', padx=(0, 6))
        tk.Label(kopf, text=t('s_ac_befund'), bg=BG, fg=SUB,
                 font=fenster.f_bold, anchor='w').pack(side='left')
        # ⭐ Umbrechen statt abschneiden: Bei „sehr groß" braucht diese
        # Zeile 394 px und bekommt 201 — die Hälfte fiele weg.
        _befund_lbl = tk.Label(
            kopf, text='  ' + t('s_ac_befund_kopf').format(anzahl),
            bg=BG, fg=SUB, font=fenster.f_small, anchor='w')
        _befund_lbl.pack(side='left', fill='x', expand=True)
        _wrap_self(_befund_lbl)

        koerper = tk.Frame(eltern, bg=BG)
        zustand = {'offen': False}

        def _klappen(_e=None):
            zustand['offen'] = not zustand['offen']
            pfeil.swap_symbol('zuklappen' if zustand['offen']
                                  else 'aufklappen')
            if zustand['offen']:
                koerper.pack(fill='x')
            else:
                koerper.pack_forget()

        for teil in (kopf,) + tuple(kopf.winfo_children()):
            teil.bind('<Button-1>', _klappen)

        # ⭐ Der beruhigende Satz zuerst, die Erklärung danach — und der
        # harmlose Knopf (aufräumen) vor dem gefährlichen (übernehmen).
        tk.Label(koerper, text=t('s_ac_befund_ruhig'), bg=BG, fg=FG,
                 font=fenster.f_small, anchor='w').pack(fill='x',
                                                        pady=(10, 0))
        _body_text(koerper, t('s_ac_befund_lead'), fenster.f_small, fill='x')

        if anzahl:
            _body_text(koerper, t('s_ac_aufraeumen_lead'), fenster.f_small,
                        fill='x')

            def _aufraeumen():
                ok2, meldung, wieviele = curves.clean_up(count_only=True)
                if not ok2:
                    _notice(fenster, t('hf_achsen'), t(meldung))
                    return
                if not _ask(fenster, 
                        t('hf_achsen'),
                        t('s_ac_aufraeumen_frage').format(wieviele)):
                    return
                ok2, meldung, wieviele = curves.clean_up()
                if not ok2:
                    _notice(fenster, t('hf_achsen'), t(meldung))
                    return
                _notice(fenster, t('hf_achsen'),
                                    t('s_ac_aufgeraeumt').format(wieviele))
                _auffrischen()

            reihe_auf = tk.Frame(koerper, bg=BG)
            reihe_auf.pack(fill='x', pady=(8, 0))
            _button(fenster, reihe_auf, t('s_ac_aufraeumen'), _aufraeumen,
                   strong=True).pack(side='left')

        if faelle:
            _body_text(koerper, t('s_ac_befund_warnung'), fenster.f_small,
                        fill='x')
            for fall in faelle:
                _befund_block(fall, koerper)

    def _saetze_block(eltern):
        """Ganze Einrichtungen unter einem Namen — „mit/ohne Pedale"."""
        from . import device_set

        tk.Frame(eltern, bg=LINE, height=1).pack(fill='x', pady=(18, 0))
        tk.Label(eltern, text=t('s_gs_titel'), bg=BG, fg=FG,
                 font=fenster.f_bold, anchor='w').pack(fill='x', pady=(14, 0))
        _body_text(eltern, t('s_gs_lead'), fenster.f_small, fill='x')

        vorhandene = device_set.sets()
        if not vorhandene:
            tk.Label(eltern, text=t('s_gs_keine'), bg=BG, fg=SUB,
                     font=fenster.f_small, anchor='w').pack(fill='x',
                                                            pady=(6, 0))
        for satz in vorhandene:
            _satz_zeile(eltern, satz, device_set)

        # Neuen Satz anlegen: Feld und Knopf in einer Zeile.
        neu = tk.Frame(eltern, bg=BG)
        neu.pack(fill='x', pady=(12, 0))
        name = tk.StringVar()
        feld = tk.Entry(neu, textvariable=name, bg=SURFACE, fg=FG,
                        insertbackground=FG, font=fenster.f_small,
                        relief='flat', width=22)
        fields.hint(feld, name, t('s_pl_satzname'), normal=FG, grey=SUB)
        feld.pack(side='left', ipady=4, padx=(0, 8))

        def _sichern():
            ok, meldung, wieviele = device_set.save(name.get())
            if not ok and meldung == 's_gs_f_name_belegt':
                if not _ask(fenster, t('hf_achsen'), t(meldung)):
                    return
                ok, meldung, wieviele = device_set.save(
                    name.get(), overwrite=True)
            if not ok:
                _notice(fenster, t('hf_achsen'), t(meldung))
                return
            _auffrischen()

        _button(fenster, neu, t('s_gs_speichern'), _sichern).pack(side='left')

    def _satz_zeile(eltern, satz, device_set):
        zeile = tk.Frame(eltern, bg=SURFACE)
        zeile.pack(fill='x', pady=(6, 0))
        links_teil = tk.Frame(zeile, bg=SURFACE)
        links_teil.pack(side='left', fill='x', expand=True, padx=10, pady=8)
        tk.Label(links_teil, text=satz['name'], bg=SURFACE, fg=FG,
                 font=fenster.f_bold, anchor='w').pack(fill='x')
        tk.Label(links_teil,
                 text='%s  ·  %s' % (
                     t('s_gs_geraete').format(len(satz.get('geraete') or {})),
                     t('s_gs_stand').format(satz.get('stand', '—'))),
                 bg=SURFACE, fg=SUB, font=fenster.f_small,
                 anchor='w').pack(fill='x')

        def _anwenden(n=satz['name']):
            schreibt, fehlt = device_set.preview(n)
            frage = t('s_gs_frage').format(n, len(schreibt))
            if fehlt:
                frage += '\n\n' + t('s_gs_fehlt').format(', '.join(fehlt))
            frage += '\n\n' + t('s_ac_spiel_zu')
            if not _ask(fenster, t('hf_achsen'), frage):
                return
            erfolg, meldung, anzahl = device_set.apply(n)
            if not erfolg:
                _notice(fenster, t('hf_achsen'), t(meldung))
                return
            _notice(fenster, t('hf_achsen'),
                                t('s_gs_angewendet').format(anzahl))
            _auffrischen()

        def _weg(n=satz['name']):
            if not _ask(fenster, t('hf_achsen'),
                                       t('s_gs_loeschen_frage').format(n)):
                return
            device_set.delete(n)
            _auffrischen()

        # ⚠ Die Knöpfe stehen außerhalb des `FLAECHE`-Kastens nicht zur
        # Verfügung — hier sind es einfache Beschriftungen im Kastenton,
        # damit kein Farbklotz entsteht (`_knopf` zeichnet fest auf `BG`).
        for text, tat, farbe in ((t('s_gs_anwenden'), _anwenden, ACCENT),
                                 (t('s_gs_loeschen'), _weg, SUB)):
            knopf = tk.Label(zeile, text=text, bg=SURFACE, fg=farbe,
                             font=fenster.f_small, padx=12, pady=6,
                             cursor='hand2')
            knopf.pack(side='right', padx=(0, 8))
            knopf.bind('<Button-1>', lambda _e, f=tat: f())

    def _exponent_fuer(ueberblick, block, achse):
        """Der Exponent, der auf **dieser** physischen Achse gilt.

        ⚠⚠⚠ **Die erste Fassung nahm den Exponenten des ganzen GERÄTS** — wenn
        es dort genau einen gab, galt er für jede Achse. Sie nannte sich selbst
        „eine Näherung" und schrieb „lieber nichts anzeigen als das Falsche" —
        und zeigte dann genau das Falsche:

        Am 06.09.2026 stand am **rechten** Stick auf der Achse `x` eine deutlich
        gebogene Kurve, während direkt darunter „Auf dieser Achse liegt keine
        Flugfunktion" stand. Beides auf einer Seite, beides über dieselbe Achse.
        Die Kurve kam von `z`, `rotx` und `roty` desselben Geräts, die alle auf
        2 stehen.

        Dazu die Frage, die keine gute Antwort hatte: *„Wie soll ich einem User
        erklären, dass er da was sieht, was gar nicht stimmt?"*

        **Jetzt wird die Belegung gefragt, nicht das Gerät.** Liegt auf der
        Achse keine Flugfunktion, ist der Exponent **1** — die Kurve gerade,
        und das stimmt dann auch: Ohne Funktion wirkt keine Empfindlichkeit.

        ⚠ Liegen **mehrere** Funktionen darauf (bei `x` des linken Sticks sind
        es zwei) und haben sie **verschiedene** Exponenten, gilt weiter „lieber
        nichts": Dann steht die gerade Linie da, weil es die eine Wahrheit
        nicht gibt. Sind sie sich einig, wird ihr Wert gezeigt.
        """
        nummer = None
        for spiel in ueberblick['spiel']:
            if (spiel.get('art') == 'joystick'
                    and spiel.get('kennung') == block.get('kennung')):
                nummer = spiel.get('nummer')
                break
        if nummer is None:
            return 1.0
        exponenten = set()
        for eintrag in curves.game_axes_of(nummer, achse):
            wert = eintrag.get('exponent')
            if wert is not None:
                exponenten.add(wert)
        return exponenten.pop() if len(exponenten) == 1 else 1.0

    def _empfindlichkeit(eltern, block, achse, plot, anzeigen, stand_zeigen):
        """Je Flugfunktion auf dieser Stickachse ein Regler.

        ⚠ Geschrieben wird auch hier erst auf Knopfdruck — jeder Regler hat
        seinen eigenen, weil jede Funktion einzeln in der Datei steht.
        """
        nummer = None
        for spiel in curves.game_axes():
            if (spiel['art'] == 'joystick'
                    and spiel['kennung'] == block['kennung']):
                nummer = spiel['nummer']
                break
        if nummer is None:
            return

        funktionen = curves.game_axes_of(nummer, achse)

        tk.Frame(eltern, bg=LINE, height=1).pack(fill='x', pady=(18, 0))
        tk.Label(eltern, text=t('s_ac_kopf_empf'), bg=BG, fg=FG,
                 font=fenster.f_bold, anchor='w').pack(fill='x', pady=(14, 0))
        _body_text(eltern, t('s_ac_lead_empf'), fenster.f_small, fill='x')

        if not funktionen:
            tk.Label(eltern, text=t('s_ac_keine_funktion'), bg=BG, fg=SUB,
                     font=fenster.f_small, anchor='w').pack(fill='x',
                                                            pady=(6, 0))
            return

        for eintrag in funktionen:
            _empf_zeile(eltern, nummer, eintrag, plot, anzeigen,
                        stand_zeigen)

    def _empf_zeile(eltern, nummer, eintrag, plot, anzeigen, stand_zeigen):
        """Ein Regler je Flugfunktion — im selben Sammelbecken wie die anderen.

        ⚠⚠ **Kein eigener Speichern-Knopf je Zeile.** Die erste Fassung hatte
        einen, und das war gleich doppelt falsch: Totzone und Sättigung
        sammelt man und schreibt sie mit **einem** Knopf — plötzlich je Zeile
        einzeln zu speichern ist ein Bruch. Und der Knopf fraß so viel Platz,
        dass vom Regler zwei kurze Balken übrig blieben, die niemand als
        Regler erkennt. Gemeldet am 06.09.2026: „Empfindlichkeit kann ich
        nicht einstellen oder ich checks einfach nicht."
        """
        # ⚠ Ein fehlender Klarname wird NICHT verschwiegen: Dann steht der
        # technische Name da. Lieber „flight_move_pitch" als eine leere Zeile,
        # bei der niemand weiß, was er gerade verstellt.
        klar = t('s_ax_' + eintrag['achse'])
        if klar == 's_ax_' + eintrag['achse']:
            klar = eintrag['achse']

        ruhe = curves.DEFAULT['exponent']
        ist = eintrag['exponent']
        zeile = tk.Frame(eltern, bg=BG)
        zeile.pack(fill='x', pady=(8, 0))
        tk.Label(zeile, text=klar, bg=BG, fg=FG, font=fenster.f_small,
                 anchor='w', width=20).pack(side='left')
        var = tk.DoubleVar(value=(ruhe if ist is None else ist))
        anzeige = tk.Label(zeile, text='', bg=BG, fg=ACCENT,
                           font=fenster.f_small, width=5, anchor='e')
        anzeige.pack(side='right', padx=(8, 0))

        def _gezogen(_w=None):
            anzeige.configure(text=_two_decimals(var.get()))
            # Die Kurve mitziehen, damit man sieht, was man tut.
            plot.show(totzone=anzeigen['deadzone']['var'].get(),
                        saturation=anzeigen['saturation']['var'].get(),
                        exponent=var.get())
            stand_zeigen()

        # ⚠ Der Bereich ist NICHT 0..1 wie bei Totzone und Sättigung: Ein
        # Exponent unter 1 macht die Mitte gröber, über 1 feiner. Gemessen
        # kommen 1, 1.1, 1.5 und 3 vor; 0.2 bis 4 deckt das mit Luft ab.
        schieber = tk.Scale(zeile, from_=0.2, to=4.0, resolution=0.05,
                            orient='horizontal', variable=var,
                            command=_gezogen, showvalue=False,
                            bg=BG, fg=FG, troughcolor=SURFACE,
                            activebackground=ACCENT, highlightthickness=0,
                            bd=0, sliderrelief='flat', length=200)
        schieber.pack(side='left', fill='x', expand=True)
        anzeige.configure(text=_two_decimals(var.get()))

        # ⚠ In DASSELBE Sammelbecken wie Totzone und Sättigung. Der Schlüssel
        # trägt die Nummer und den Achsennamen, weil auf einer Stickachse
        # mehrere Funktionen liegen können — sonst überschriebe „Gieren" den
        # Eintrag von „Schub seitlich".
        anzeigen['exp:%s:%s' % (nummer, eintrag['achse'])] = {
            'var': var, 'anzeige': anzeige, 'ist': ist, 'ruhe': ruhe,
            'start': var.get(), 'spiel': (nummer, eintrag['achse'])}

    def _achsenzeile(eltern, block, achse, funktionen=()):
        gewaehlt = achse == wahl['achse']
        werte = block['achsen'].get(achse) or {}
        zeile = tk.Frame(eltern, bg=BAR if gewaehlt else BG, cursor='hand2')
        zeile.pack(fill='x', pady=(0, 2))

        def _waehlen(_e=None, a=achse):
            wahl['achse'] = a
            _auffrischen()

        name = tk.Label(zeile, text=achse, bg=zeile['bg'],
                        fg=ACCENT if gewaehlt else FG, font=fenster.f_bold,
                        anchor='w', width=9, padx=8)
        name.pack(side='left', pady=5)
        text = '%s %s   ·   %s %s' % (
            t('s_kv_totzone'), _two_decimals(werte.get('deadzone')),
            t('s_kv_saettigung'), _two_decimals(werte.get('saturation')))
        wert = tk.Label(zeile, text=text, bg=zeile['bg'], fg=SUB,
                        font=fenster.f_small, anchor='w')
        wert.pack(side='left', fill='x', expand=True)

        # ⭐⭐ **Was auf dieser Achse liegt — sonst bleibt die Empfindlichkeit
        # unerklärt.** Am 06.09.2026 gefragt: „Sättigung und Totzone stehen in
        # ner Art Tabelle, Empfindlichkeit aber nicht dabei, an welchen Achsen
        # kann man Empfindlichkeit eigentlich einstellen?"
        #
        # Die Antwort ließ sich aus der Tabelle nicht ablesen: Totzone und
        # Sättigung hängen an der **physischen** Achse, die Empfindlichkeit an
        # der **Flugfunktion**, die darauf liegt. Wo keine Funktion belegt ist,
        # gibt es auch nichts einzustellen — das steht jetzt in der Zeile.
        #
        # ⚠ Bei mehreren Funktionen nur die Zahl: Auf `js2_slider1` liegen
        # `flight_move_strafe_back` und `flight_move_move_back` gleichzeitig,
        # ausgeschrieben sprengt das die Zeile.
        if funktionen:
            if len(funktionen) == 1:
                schluessel = 's_ax_' + (funktionen[0].get('achse') or '')
                klar = t(schluessel)
                if klar == schluessel:
                    klar = funktionen[0].get('achse') or ''
            else:
                klar = t('s_ac_mehrere_funktionen').format(n=len(funktionen))
            # ⭐ Umbrechen statt abschneiden: „Umsehen links/rechts"
            # braucht bei „sehr groß" 200 px und bekommt 148 — die
            # Zeile ist voll, und Tk kürzt wortlos.
            _fn_lbl = tk.Label(zeile, text=klar, bg=zeile['bg'],
                               fg=ACCENT if gewaehlt else SUB,
                               font=fenster.f_small, anchor='e', padx=8)
            _fn_lbl.pack(side='right')
            _wrap_self(_fn_lbl)

        if achse in block['mehrfach']:
            marke = tk.Label(zeile, text='⚠', bg=zeile['bg'], fg=GOLD,
                             font=fenster.f_small, padx=8)
            marke.pack(side='right')
        for teil in (zeile, name, wert):
            teil.bind('<Button-1>', _waehlen)

    def _befund_block(fall, wohin=None):
        """Ein verlorener Stand — nach Wert zusammengefasst, nicht je Achse.

        ⚠ **Die erste Fassung listete jede Achse einzeln.** An einem echten
        Aufbau waren das elf Zeilen für einen einzigen Fall und über zwanzig
        insgesamt — die eigentliche Bedienung stand damit unter der Falzkante.
        Dabei steht auf allen sechs Achsen fast immer **derselbe** Wert: Wer
        eine Sättigung einstellt, stellt sie für den ganzen Stick ein. Eine
        Zeile je Wert sagt dasselbe und passt auf den Bildschirm.
        """
        ziel_rahmen = wohin if wohin is not None else inhalt
        kasten = tk.Frame(ziel_rahmen, bg=SURFACE)
        kasten.pack(fill='x', pady=(8, 0))
        tk.Label(kasten, text=fall['name'], bg=SURFACE, fg=FG,
                 font=fenster.f_bold, anchor='w', padx=10).pack(fill='x',
                                                                pady=(8, 0))

        # Nach (Eigenschaft, alter Wert, jetziger Wert) bündeln — die Achsen
        # sammeln sich als Aufzählung dahinter.
        buendel = {}
        for achse, eigenschaft, alt, jetzt in fall['werte']:
            buendel.setdefault((eigenschaft, alt, jetzt), []).append(achse)

        for (eigenschaft, alt, jetzt), achsen in buendel.items():
            name = (t('s_kv_totzone') if eigenschaft == 'deadzone'
                    else t('s_kv_saettigung'))
            wie = (t('s_ac_fehlt') if jetzt is None
                   else '%s %s' % (t('s_ac_jetzt'), _two_decimals(jetzt)))
            tk.Label(kasten,
                     text='%s: %s %s → %s' % (name, t('s_ac_war'),
                                              _two_decimals(alt), wie),
                     bg=SURFACE, fg=FG, font=fenster.f_small, anchor='w',
                     padx=10).pack(fill='x')
            tk.Label(kasten, text='   ' + ', '.join(achsen), bg=SURFACE,
                     fg=SUB, font=fenster.f_small, anchor='w',
                     padx=10).pack(fill='x')

        def _uebernehmen(f=fall):
            """Die alten Werte auf die neue Kennung schreiben.

            ⚠ **Erst fragen.** Das schreibt in die Datei, an der die komplette
            Steuerung hängt — und zwar mehrere Werte auf einmal. Der Spieler
            sieht vorher, was passiert; die Sicherung entsteht ohnehin bei
            jedem Schreibvorgang.
            """
            if not _ask(fenster, 
                    t('hf_achsen'),
                    '%s\n\n%s' % (t('s_ac_uebernehmen'), t('s_ac_spiel_zu'))):
                return
            for achse, eigenschaft, alt, _jetzt in f['werte']:
                erfolg, meldung, _ = curves.apply(
                    f['neu']['kennung'], achse, eigenschaft, alt)
                if not erfolg:
                    _notice(fenster, t('hf_achsen'), t(meldung))
                    return
            _notice(fenster, t('hf_achsen'), t('s_ac_gespeichert'))
            _auffrischen()

        tk.Frame(kasten, bg=SURFACE, height=8).pack(fill='x')
        # ⚠ Der Knopf steht UNTER dem Kasten, nicht darin: `_knopf` zeichnet
        # seine Leinwand fest auf `BG`, und in einem `FLAECHE`-Kasten wäre das
        # ein sichtbarer Farbklotz. Die gemeinsame Funktion dafür umzubauen
        # wäre der größere Eingriff — an ihr hängen alle anderen Seiten.
        reihe = tk.Frame(ziel_rahmen, bg=BG)
        reihe.pack(fill='x', pady=(6, 0))
        _button(fenster, reihe, t('s_ac_uebernehmen'),
               _uebernehmen).pack(side='left')

    _auffrischen()
    # ⚠ Nicht `_auffrischen` selbst: Der baut IMMER neu. Beim Seitenwechsel
    # soll nur neu gebaut werden, wenn sich wirklich etwas geändert hat.
    fenster.on_show['achsen'] = _beim_zeigen


# ------------------------------------------- Was der Patch geändert hat (v3.24)
# ⚠ **Höchstens so viele Posten auf einmal.** Der Patch vom 26.08.2026 ändert
# allein bei den Schiffen 184 Einträge, viele davon mit mehreren Feldern —
# gezeichnet wären das über tausend Etiketten in einem Rutsch. Tk zeichnet
# einsträngig, das Fenster stünde sekundenlang. Der Rest steht als „… und N
# weitere" darunter; wer mehr sehen will, schränkt den Bereich ein.
_PATCH_MAX = 60

# ⚠ **Wie hoch die Patch-Liste höchstens wird.** Sie steht fest über dem
# Inhalt, damit man beim Durchsehen der Werte nicht nach oben zurück muss — und
# genau deshalb darf sie nicht mitwachsen. Gemessen am 07.09.2026: 30 px je
# Zeile. 150 px sind genau fünf Zeilen — „5 patches zu sehen reicht"; alles
# darüber rollt in der Liste selbst.
#
# Die Zahl ist bewusst eine Höhe und keine Zeilenzahl: Bei größerer Schrift
# werden die Zeilen höher, und dann sollen eben vier statt fünf hineinpassen —
# nicht fünf, die unten abgeschnitten sind.
_PATCH_LIST_HEIGHT = 150


def _patch_number(value, digits=4):
    """Einen Wert aus den Patch-Daten anzeigbar machen.

    ⚠ Es kommen Zahlen, Texte, Wahrheitswerte und verschachtelte Gebilde
    durcheinander — `weapon.ammo.damage.physical` ist eine Zahl, `i18n.name`
    ein Text. Und Kommazahlen kommen mit voller Genauigkeit („0.30000000004"),
    was in einer Liste nur Lärm ist."""
    if isinstance(value, bool):
        return 'ja' if value else 'nein'
    if isinstance(value, float):
        gerundet = round(value, digits)
        # ⚠⚠ **Keine Exponentialschreibweise** (07.09.2026, gefragt mit
        # „1.25e-06 was soll das für nen wert sein?"). Zu Recht: `%g` kippt
        # unterhalb von 0,0001 auf `1.25e-06` um, und das liest niemand, der
        # nicht täglich mit Zehnerpotenzen umgeht. Die Zahl war zwar richtig
        # und sichtbar — nur eben unverständlich, was fast dasselbe ist wie
        # unsichtbar.
        #
        # Also ausgeschrieben: `0.00000125`. Länger, aber jeder sieht sofort,
        # dass es eine sehr kleine Zahl ist — und wie klein.
        if gerundet and abs(gerundet) < 1e-4:
            return ('%.*f' % (digits, gerundet)).rstrip('0').rstrip('.')
        # `%g` wirft die Nullen weg und macht aus 975.0 wieder 975.
        return '%g' % gerundet
    if value is None:
        return '—'
    if isinstance(value, (list, dict)):
        return '…'
    return str(value)


def _pc_pair(alt, neu):
    """Alt und neu so darstellen, dass ein Unterschied auch SICHTBAR ist.

    ⚠⚠ **Das ist keine Kosmetik — ohne das log die Seite** (07.09.2026).
    `_pa_zahl` rundete fest auf vier Nachkommastellen. Die Treibstoff-Brennraten
    aus 4.10.0 liegen aber bei `1.25e-06`; alles darunter wurde damit zu `0`,
    und alt wie neu sahen gleich aus. Die Zeile meldete „unverändert".

    Nachgemessen über beide Patches: **557 Zeilen** standen so da — und
    **keine einzige** davon war wirklich unverändert. Es waren durchweg echte
    Änderungen, oft um den Faktor drei bis fünf:

        precomputed.fuel.burnRate.main       1.25e-06 → 2.67e-07
        precomputed.fuel.burnRate.maneuver   1e-05    → 2.136e-06

    Also genau das Gegenteil dessen, was der Reiter soll: Er versteckte die
    größten Änderungen des Patches hinter dem Wort „unverändert".

    Deshalb wird die Genauigkeit jetzt so weit erhöht, bis der Unterschied
    dasteht. Normale Werte bleiben kurz (`110 → 120`), nur die winzigen werden
    länger — und das ist der Preis dafür, dass sie überhaupt zu sehen sind.
    """
    def _verschluckt(roh, gezeigt):
        """Wird hier eine Zahl zu „0", die in Wirklichkeit keine ist?"""
        return gezeigt == '0' and isinstance(roh, (int, float)) \
            and not isinstance(roh, bool) and roh != 0

    for stellen in (4, 6, 8, 10, 12, 14):
        vorher, nachher = _patch_number(alt, stellen), _patch_number(neu, stellen)
        # ⚠ **Unterschiedlich zu sein reicht nicht.** Bei sechs Stellen wurde
        # aus `1.25e-06 → 2.67e-07` die Zeile „1e-06 → 0" — verschieden, ja,
        # aber die zweite Zahl war gelogen: Der Wert ist nicht null, er ist
        # nur kleiner als die Anzeige fassen konnte. Eine falsche 0 an dieser
        # Stelle ist schlimmer als eine lange Zahl, denn sie liest sich wie
        # „abgeschaltet".
        if vorher != nachher \
                and not _verschluckt(alt, vorher) \
                and not _verschluckt(neu, nachher):
            return vorher, nachher
    # Wirklich gleich — oder so winzig, dass auch vierzehn Stellen nicht reichen.
    return _patch_number(alt), _patch_number(neu)


def _pc_percent(alt, neu):
    """Um wie viel Prozent hat sich der Wert verändert — als fertiger Zusatz.

    ⚠⚠ **Das ist bei kleinen Zahlen die einzige lesbare Aussage** (07.09.2026).
    `0.00000125 → 0.00000036` ist zwar richtig und ausgeschrieben, aber wer das
    erfassen will, muss Nullen zählen. Was der Spieler wissen will, ist nicht
    die Zahl, sondern **wie stark** sich etwas geändert hat: „auf ein Drittel
    gefallen".

    Der Zusatz steht bei jeder Zeile, nicht nur bei den winzigen — auch
    `110 → 120` gewinnt durch `(+9 %)`, weil man den Anteil sonst im Kopf
    ausrechnet.

    ⚠ Kein Prozent ohne Bezugsgröße: Ist der alte Wert 0, wäre jede Steigerung
    unendlich. Dann bleibt der Zusatz weg.
    """
    if isinstance(alt, bool) or isinstance(neu, bool):
        return ''
    if not isinstance(alt, (int, float)) or not isinstance(neu, (int, float)):
        return ''
    if not alt or alt == neu:
        return ''
    anteil = (neu - alt) / abs(alt) * 100.0
    if abs(anteil) < 0.5:                  # unter einem halben Prozent
        return ''
    # Grosse Sprünge in Prozent sind unhandlich („+400 %") — ab dem Dreifachen
    # sagt ein Faktor mehr aus.
    if anteil >= 200:
        return '  (×%s)' % _patch_number(round(neu / abs(alt), 1))
    return '  (%+d %%)' % round(anteil)


def _pc_direction(alt, neu):
    """Ist der Wert gestiegen (1), gefallen (-1) oder keins von beidem (0)?

    ⚠ **Die Farbe zeigt die RICHTUNG, nicht ob es besser wurde.** Grün heißt
    „mehr geworden", nicht „gut": Beim Verbrauch je Sekunde ist ein grüner
    Pfeil nach oben für den Spieler eine Verschlechterung. Eine Bewertung
    würde voraussetzen, dass für jedes der 60 Felder feststeht, in welche
    Richtung „besser" liegt — das steht nirgends und wäre geraten. Die
    Richtung dagegen ist ablesbar und stimmt immer.

    ⚠ Nur echte Zahlen vergleichen. `True`/`False` sind in Python zwar Zahlen,
    aber „von nein auf ja" ist kein Anstieg; Texte erst recht nicht. In beiden
    Fällen kommt 0 zurück, und die Zeile bleibt in der Grundfarbe.
    """
    if isinstance(alt, bool) or isinstance(neu, bool):
        return 0
    if not isinstance(alt, (int, float)) or not isinstance(neu, (int, float)):
        return 0
    if neu > alt:
        return 1
    if neu < alt:
        return -1
    return 0


def _pc_leaves(wert, pfad='', aus=None):
    """Alle Einzelwerte einer verschachtelten Struktur, mit ihrem Pfad.

    Aus `[{'consumes': [{'resource': 'Power', 'units': 4}]}]` wird
    `{'[0].consumes[0].resource': 'Power', '[0].consumes[0].units': 4}`.
    """
    aus = {} if aus is None else aus
    if isinstance(wert, dict):
        for schluessel, unterwert in wert.items():
            _pc_leaves(unterwert,
                         '%s.%s' % (pfad, schluessel) if pfad else schluessel,
                         aus)
    elif isinstance(wert, list):
        for nummer, unterwert in enumerate(wert):
            _pc_leaves(unterwert, '%s[%d]' % (pfad, nummer), aus)
    else:
        aus[pfad] = wert
    return aus


def _pc_short_path(pfad):
    """Nur der sprechende Rest eines Blattpfads — `consumes[0].units` → `units`.

    ⚠ Vorher noch in der Feldtabelle nachsehen: Steht der volle Pfad dort, ist
    der deutsche Name besser als das abgeschnittene englische Ende.
    """
    lesbar = pa_feld(pfad)
    if lesbar != pfad:
        return lesbar
    letzte = pfad.split('.')[-1] if pfad else pfad
    return letzte or pfad


def _pc_struct(alt, neu, hoechstens=3):
    """Was sich in zwei verschachtelten Werten wirklich unterscheidet.

    ⚠⚠ **Warum es diese Funktion gibt** (07.09.2026, gemeldet als „da sind gar
    keine Infos drin"): `_pa_zahl` gibt für alles Verschachtelte ein `…`
    zurück. In der Kategorie `blades` ist **jeder** geänderte Wert eine
    verschachtelte Struktur (`resource.states[0].flows`) — die Seite zeigte
    dort also 69 Zeilen `… → …` untereinander. Buchstäblich keine Auskunft,
    und weil `blades` alphabetisch weit vorn steht, war es das Erste, was man
    sah.

    Statt den ganzen Wert zu zeigen (der wäre unlesbar lang), wird der
    **Unterschied** gebildet: beide Seiten in Blattwerte zerlegt, verglichen,
    und nur die abweichenden benannt.

    Und der ehrlichste Fall zuerst: Unterscheiden sich die Blätter gar nicht,
    hat der Patch an dieser Stelle nichts geändert — dann sagt das Ergebnis
    genau das, statt einen Unterschied vorzutäuschen. Gemessen am 07.09.2026
    trifft das auf rund ein Fünftel der Posten in 4.10.0 zu.
    """
    a, b = _pc_leaves(alt), _pc_leaves(neu)
    teile = []
    for schluessel in sorted(set(a) | set(b)):
        if a.get(schluessel) == b.get(schluessel):
            continue
        kurz = _pc_short_path(schluessel)
        if schluessel not in b:
            teile.append(t('s_pa_feld_weg').format(feld=kurz))
        elif schluessel not in a:
            teile.append(t('s_pa_feld_neu').format(
                feld=kurz, wert=_patch_number(b[schluessel])))
        else:
            teile.append('%s %s → %s' % (kurz, _patch_number(a[schluessel]),
                                         _patch_number(b[schluessel])))
    if not teile:
        return t('s_pa_gleich')
    if len(teile) > hoechstens:
        rest = len(teile) - hoechstens
        return '%s, %s' % (', '.join(teile[:hoechstens]),
                           t('s_pa_mehr').format(n=rest))
    return ', '.join(teile)


def _pc_field_row(fenster, eltern, feld):
    """Eine einzelne Feldänderung: Pfad und was aus dem Wert wurde."""
    zeile = tk.Frame(eltern, bg=BG)
    zeile.pack(fill='x', padx=(18, 0))
    # ⚠ Über `sprache.pa_feld`, nicht der rohe Pfad. `precomputed.fuel.
    # hydrogenCapacity` ist ein CIG-Bezeichner und sagt einem Spieler nichts;
    # was nicht in der Tabelle steht, bleibt bewusst roh stehen.
    tk.Label(zeile, text=pa_feld(feld['pfad']), bg=BG, fg=SUB,
             font=fenster.f_small, anchor='w').pack(side='left')
    # ⚠ Drei Fälle, nicht einer. Fehlt `newValue`, hat der Patch das Feld
    # **weggenommen**; fehlt `oldValue`, ist es **dazugekommen**. Wer stumpf
    # „alt → neu" schreibt, macht daraus „1090 → None" und behauptet einen
    # Wert, den es nicht gibt. Gemessen an der C-788 Cannon (07.09.2026).
    if feld['hat_alt'] and feld['hat_neu']:
        # ⚠ Verschachtelte Werte NICHT als „… → …" abtun — dann steht dort
        # nichts. Bei ihnen zeigt `_pc_struct` den echten Unterschied.
        if isinstance(feld['alt'], (list, dict)) \
                or isinstance(feld['neu'], (list, dict)):
            text = _pc_struct(feld['alt'], feld['neu'])
            farbe = FG
        else:
            # ⚠⚠ **Verglichen wird der ROHWERT, nicht die Anzeige.**
            # Hier stand bis zum 07.09.2026 das Gegenteil, mit der Begründung
            # „was für den Leser gleich aussieht, ist für ihn auch gleich".
            # Das klang vernünftig und war falsch: Wenn die Anzeige zwei
            # verschiedene Werte gleich aussehen lässt, ist die ANZEIGE das
            # Problem — dann muss sie genauer werden, nicht die Änderung
            # verschwinden. `_pc_pair` erhöht die Genauigkeit so weit, bis der
            # Unterschied dasteht.
            vorher, nachher = _pc_pair(feld['alt'], feld['neu'])
            richtung = _pc_direction(feld['alt'], feld['neu'])
            # ⚠ `0 → 0` ist keine Auskunft, sondern Lärm — und davon steht
            # reichlich in den Daten: Erkul führt ein Feld auch dann im Diff,
            # wenn der Wert derselbe geblieben ist. Gemessen am 07.09.2026:
            # rund ein Fünftel der Posten in 4.10.0 ändert keinen einzigen
            # Wert. Wer zwischen echten Änderungen zehnmal „0 → 0" liest,
            # hält den ganzen Reiter für kaputt.
            #
            # Verglichen wird die ANZEIGE, nicht der Rohwert: Was für den
            # Leser gleich aussieht, ist für ihn auch gleich — ob dahinter
            # eine gerundete Winzigkeit steckt, ändert daran nichts.
            if vorher == nachher:
                # ⚠⚠ **Der WERT selbst, grau — nicht „0" und nicht ein Satz.**
                #
                # Vorher stand hier „umgebaut, aber kein Wert anders": richtig,
                # aber dreimal so lang wie die Zeile daneben, und bei der
                # Avenger Stalker füllt das 3 von 8 Zeilen.
                #
                # Vorgeschlagen war, stattdessen `0` einzutragen. Das wäre
                # kürzer, aber es LÜGT: Bei „Verbrauch Haupttriebwerk 0" liest
                # man, der Verbrauch sei null — nicht, dass er gleich geblieben
                # ist. Ein Feld, dessen echter Wert 0 ist, wäre davon nicht
                # mehr zu unterscheiden.
                #
                # Vorgeschlagen war weiter ein `→ X`. Dagegen spricht, dass es
                # sich wie „ist weggefallen" liest — und dafür gibt es bereits
                # eine eigene Zeile (`1090 → fällt weg`, gold). Zwei Zeichen
                # für zwei verschiedene Aussagen, die gleich aussehen, sind
                # schlimmer als eine Zeile mehr Text.
                #
                # Beide Werte mit Pfeil, in Grau: Jede Zeile der Liste ist
                # gleich aufgebaut (`alt → neu`), man sieht die Werte selbst,
                # und dass beide dieselben sind, ist auf einen Blick klar.
                # Grau heißt hier ohnehin schon „unverändert" — dieselbe
                # Regel wie bei den grünen und roten Zeilen.
                #
                # ⚠ In den heutigen Daten kommt dieser Fall **nicht** vor:
                # Nach dem Rundungsfix vom 07.09.2026 sind alle 2652 Posten
                # sichtbar geändert. Der Zweig bleibt trotzdem — der nächste
                # Patch kann ein Feld anfassen, ohne den Wert zu ändern.
                text, farbe = '%s → %s' % (vorher, nachher), SUB
            else:
                text = '%s → %s%s' % (vorher, nachher,
                                      _pc_percent(feld['alt'], feld['neu']))
                farbe = {1: ACCENT, -1: RED}.get(richtung, FG)
    elif feld['hat_alt']:
        text = t('s_pa_weggefallen').format(alt=_patch_number(feld['alt']))
        farbe = GOLD
    else:
        text = t('s_pa_dazugekommen').format(neu=_patch_number(feld['neu']))
        farbe = ACCENT
    tk.Label(zeile, text=text, bg=BG, fg=farbe, font=fenster.f_small,
             anchor='w').pack(side='left', padx=(10, 0))


def _patch_changes(fenster, rahmen):
    """Was ein Spiel-Patch an Werten verändert hat — Stufe 4 der Erkul-Reihe.

    ⭐ **Der Gewinn steckt in der Ablage, nicht im Abruf.** Erkul hält nur die
    letzten zehn Patches vor. Was hier einmal liegt, bleibt — nach einem Jahr
    hat der Spieler eine Sammlung, die es sonst nirgends gibt. Deshalb steht
    der Satz dazu auch auf der Seite und nicht nur im Quelltext.

    ⚠ **Der Abruf läuft im Hintergrund.** Genau an dieser Stelle hing der
    Vormerken-Knopf: Ein Netzabruf im Oberflächen-Faden hält ohne Netz das
    ganze Fenster fest, bis das Zeitlimit greift.
    """
    from . import patchaenderungen as pa

    _heading(fenster, rahmen, t('hf_patchaenderungen'), t('s_pa_lead'))

    # ⚠⚠ **Patch-Liste und Bereichsknöpfe bleiben STEHEN, nur die Werte rollen**
    # (07.09.2026): „ab den auswahlen nach unten scrollbar … wenn man durch die
    # werte schaut ist es mega nervig erst nach oben zu müssen um ne neue
    # auswahl zu treffen."
    #
    # Vorher lag alles in EINER Rollfläche. Wer bei „ships" durch 60 Posten
    # gescrollt war und dann „weapons" ansehen wollte, musste den ganzen Weg
    # zurück nach oben — bei jedem Wechsel.
    #
    # ⚠ Die Reihenfolge ist dabei die halbe Miete: Erst alles Feste packen,
    # **danach** die Rollfläche mit `expand=True`. Wird der feste Teil nach dem
    # wachsenden gepackt, schiebt der Inhalt ihn aus dem Fenster — genau die
    # Falle, die in diesem Projekt schon zweimal zugeschlagen hat.
    _body_text(rahmen, t('s_pa_sammlung'), fenster.f_small, fill='x',
                padx=24, inset=48)

    kopf = tk.Frame(rahmen, bg=BG)
    kopf.pack(fill='x', padx=24, pady=(12, 0))
    stand = tk.Label(kopf, text='', bg=BG, fg=SUB, font=fenster.f_small,
                     anchor='w')

    # ⚠⚠ **Die Patch-Liste rollt in sich selbst, mit fester Höhe** (07.09.2026):
    # „wenn viele weitere patches hinzukommen, ist unten dann überhaupt noch was
    # lesbar?"
    #
    # Nein, wäre die Antwort gewesen. Die Liste steht fest über dem Inhalt, und
    # sie wächst — die eigene Sammlung geht absichtlich über die zehn Patches
    # hinaus, die die Quelle vorhält. Gemessen: 30 px je Zeile, also blieben
    # bei 30 Patches noch 41 px für die Werte und bei 50 gar nichts mehr.
    #
    # Fünf Zeilen sind sichtbar, der Rest wird gerollt. Das ist genau die Regel
    # aus der Projekt-Anleitung: Ein fester Bereich mit veränderlichem Inhalt
    # braucht eine Höhengrenze **und** eine eigene Rollfläche — sonst wird er
    # irgendwann abgeschnitten, ohne dass es jemand merkt.
    liste = _scroll_area(rahmen, height=_PATCH_LIST_HEIGHT)
    bereiche = tk.Frame(rahmen, bg=BG)
    bereiche.pack(fill='x', padx=24, pady=(10, 0))

    # Ab hier rollt es — und nur das.
    innen = _scroll_area(rahmen)
    ergebnis = tk.Frame(innen, bg=BG)
    ergebnis.pack(fill='x', padx=24, pady=(8, 20))

    zustand = {'patch': '', 'art': None}

    def _leeren(*rahmen_liste):
        for r in rahmen_liste:
            for kind in r.winfo_children():
                kind.destroy()

    # ---------------------------------------------------------- Die Anzeige
    def _posten_zeigen():
        _leeren(ergebnis)
        version = zustand['patch']
        if not version:
            # ⚠ „Wähl links einen Patch aus" ist nur richtig, wenn links auch
            # etwas Auswählbares liegt. Solange nichts abgelegt ist, führt der
            # Satz in die Irre: Man wählt, bekommt nichts, und hält die Seite
            # für leer — dabei fehlt bloß ein Druck auf den Knopf darüber.
            if pa.gespeicherte():
                _body_text(ergebnis, t('s_pa_waehlen'), fenster.f_small,
                            fill='x')
            else:
                _body_text(ergebnis,
                            t('s_pa_erst_holen').format(knopf=t('s_pa_suchen')),
                            fenster.f_small, fill='x')
            return
        posten = pa.aenderungen(version, zustand['art'])
        if not posten:
            _body_text(ergebnis, t('s_pa_nichts_hier'), fenster.f_small,
                        fill='x')
            return
        for eintrag in posten[:_PATCH_MAX]:
            kasten = tk.Frame(ergebnis, bg=BG)
            kasten.pack(fill='x', pady=(0, 8))
            zeile = tk.Frame(kasten, bg=BG)
            zeile.pack(fill='x')
            marke = {'neu': (t('s_pa_zustand_neu'), ACCENT),
                     'weg': (t('s_pa_zustand_weg'), GOLD),
                     'geaendert': (t('s_pa_zustand_geae'), SUB)}[eintrag['zustand']]
            tk.Label(zeile, text=marke[0], bg=BG, fg=marke[1],
                     font=fenster.f_small, anchor='w').pack(side='left')
            tk.Label(zeile, text=eintrag['name'], bg=BG, fg=FG,
                     font=fenster.f_bold, anchor='w').pack(side='left',
                                                           padx=(10, 0))
            if eintrag.get('groesse') is not None:
                tk.Label(zeile,
                         text=t('s_pa_groesse').format(n=eintrag['groesse']),
                         bg=BG, fg=SUB, font=fenster.f_small,
                         anchor='w').pack(side='left', padx=(8, 0))
            for feld in eintrag['felder']:
                _pc_field_row(fenster, kasten, feld)
        rest = len(posten) - _PATCH_MAX
        if rest > 0:
            _body_text(ergebnis, t('s_pa_mehr').format(n=rest),
                        fenster.f_small, fill='x', pady=(6, 0))

    def _bereiche_zeigen():
        _leeren(bereiche)
        version = zustand['patch']
        if not version or not pa.laden(version):
            return
        # ⚠⚠ **`_knopfgitter` will fertige KNÖPFE, keine Beschriftungspaare.**
        # Hier standen bis zum 07.09.2026 `(Text, Rückruf)`-Tupel, und das ist
        # kein Schönheitsfehler: `_reflow_grid` ruft `winfo_exists()` auf
        # jedem Eintrag, ein Tupel hat das nicht — beim Klick auf einen
        # abgelegten Patch flog `AttributeError: 'tuple' object has no
        # attribute 'winfo_exists'`, und zwar aus einem `after`-Rückruf heraus.
        # Der Reiter blieb dabei leer, ohne sichtbare Fehlermeldung.
        #
        # Aufgefallen ist es erst am 07.09.2026 beim ersten echten Durchklicken:
        # Der Reiter ist am Mac entstanden, wo Fenster gebaut, aber nicht
        # gezeigt werden — geklickt hatte ihn vorher niemand. Genau die Lücke,
        # die auch der Selbsttest nicht schließt.
        knoepfe = [_button(fenster, bereiche, t('s_pa_alle'),
                          lambda: _art_waehlen(None),
                          strong=(zustand['art'] is None))]
        for art, anzahl in pa.kategorien(version):
            knoepfe.append(_button(fenster, bereiche,
                                  '%s (%d)' % (art, anzahl),
                                  (lambda a=art: _art_waehlen(a)),
                                  strong=(zustand['art'] == art)))
        _button_grid(bereiche, knoepfe)

    def _art_waehlen(art):
        zustand['art'] = art
        # ⚠ Die Knopfreihe mit neu bauen, sonst bleibt die Hervorhebung auf
        # dem zuvor gewählten Bereich stehen — man sieht dann Posten aus
        # „ships", während „Alle" hervorgehoben ist.
        _bereiche_zeigen()
        _posten_zeigen()

    def _eintrag_zu(version):
        """Der Übersichtseintrag zu einer Version — oder None."""
        for eintrag in pa.uebersicht():
            if eintrag['version'] == version:
                return eintrag
        return None

    def _patch_waehlen(version):
        zustand['patch'] = version
        # ⚠⚠ **Nicht „Alle" vorwählen** (07.09.2026): „Alle ist irgendwie doof,
        # da sieht man eh nichts mehr."
        #
        # Zu Recht. Die Anzeige bricht nach 60 Posten ab, und über alle
        # Bereiche hinweg stehen die alphabetisch vorn — bei 4.10.0 sind das
        # 60 Zeilen `blades`, während die 184 Schiffe dahinter nie zu sehen
        # sind. Man sucht sich also erst durch das Uninteressanteste.
        #
        # Vorgewählt wird der GRÖSSTE Bereich, nicht fest `ships`. Bei 4.10.0
        # ist das dasselbe (ships 184), aber nicht immer: In 4.9.0 sind es
        # `weapons` mit 135, ships kommt dort nur auf 70. Fest `ships` würde
        # bei einem Waffen-Patch also wieder am Wesentlichen vorbeizeigen —
        # und `kategorien()` liefert ohnehin schon nach Größe sortiert.
        zustand['art'] = None
        if not pa.laden(version):
            # ⚠⚠ **Zwei völlig verschiedene Gründe, warum hier nichts liegt** —
            # und bis zum 07.09.2026 bekamen beide denselben Satz zu sehen:
            # „nicht abgelegt, die Quelle meldet keine Änderungen".
            #
            #   1. Der Patch hat wirklich nichts geändert (8 von 10 Patches!)
            #   2. Der Patch hat 352 Änderungen — sie sind nur noch nicht geholt
            #
            # Im zweiten Fall log die Seite den Nutzer an: Sie behauptete, es
            # gebe nichts, während der Knopf daneben genau das geholt hätte.
            # Wer zuerst auf die zwei jüngsten Patches klickt — und die sind
            # leer — hält den ganzen Reiter für kaputt oder nutzlos. Genau so
            # gemeldet am 07.09.2026: „da sind gar keine Infos drin".
            _leeren(bereiche, ergebnis)
            eintrag = _eintrag_zu(version)
            if eintrag is not None and not eintrag['leer']:
                z = eintrag['summary']
                _body_text(ergebnis, t('s_pa_nicht_geholt').format(
                    plus=z.get('added', 0), minus=z.get('removed', 0),
                    tilde=z.get('modified', 0), knopf=t('s_pa_suchen')),
                    fenster.f_small, fill='x')
            else:
                _body_text(ergebnis, t('s_pa_leer_klick'), fenster.f_small,
                            fill='x')
            return
        bereiche_da = pa.kategorien(version)
        zustand['art'] = bereiche_da[0][0] if bereiche_da else None
        _bereiche_zeigen()
        _posten_zeigen()

    def _liste_zeigen():
        _leeren(liste)

        # ⚠⚠ **Patches ohne Werteänderung kommen NICHT in die Liste**
        # (07.09.2026): „das sind ja nur hotfixes wo nie ne änderung drin ist."
        #
        # Stimmt, und die Zahlen geben ihm recht: Von zehn Patches sind **acht**
        # leer. Sie standen bisher als volle Zeilen dazwischen, drängten die
        # zwei interessanten nach unten — und wer der Reihe nach von oben
        # klickte, landete zuerst auf ihnen und hielt den Reiter für kaputt.
        #
        # ⚠ Aber nicht spurlos: Darunter steht, **wie viele** weggelassen
        # wurden. Ohne diese Zeile sähe es aus, als fehlten Patches oder als
        # sei der Abruf unvollständig — und genau dieser Verdacht („da sind gar
        # keine Infos drin") war der Anlass, den Reiter zu überarbeiten.
        # Weglassen ja, verschweigen nein.
        alle = pa.uebersicht()
        gezeigt = [e for e in alle if not e['leer']]
        weggelassen = len(alle) - len(gezeigt)

        for eintrag in gezeigt:
            zeile = tk.Frame(liste, bg=BG, cursor='hand2')
            zeile.pack(fill='x', pady=(0, 4))
            z = eintrag['summary']
            if eintrag['leer']:
                rechts, farbe = t('s_pa_leer'), SUB
            else:
                rechts = t('s_pa_zaehler').format(
                    plus=z.get('added', 0), minus=z.get('removed', 0),
                    tilde=z.get('modified', 0))
                farbe = FG
            # ⚠ **Auch die Versionsnummer zurücknehmen, nicht nur die Zahl
            # rechts.** Von zehn Patches sind acht leer; standen alle zehn in
            # voller Schriftfarbe da, sahen die zwei mit Inhalt aus wie die
            # anderen — und man klickt der Reihe nach von oben, wo genau die
            # leeren stehen. Die Liste soll auf einen Blick zeigen, wo etwas
            # zu holen ist. Weggelassen wird nichts: Ein leerer Patch ist eine
            # gültige Auskunft („in dieser Woche hat sich nichts geändert").
            name_farbe = SUB if eintrag['leer'] else FG
            for text, fg, breit in ((eintrag['version'], name_farbe, True),
                                    (eintrag['datum'], SUB, False),
                                    (rechts, farbe, False)):
                tk.Label(zeile, text=text, bg=BG, fg=fg,
                         font=fenster.f_bold if breit else fenster.f_small,
                         anchor='w').pack(side='left', padx=(0, 14))
            # ⭐ Genau der Fall, für den lokal abgelegt wird: Die Quelle führt
            # ihn nicht mehr, der Spieler hat ihn trotzdem.
            if not eintrag['bei_erkul']:
                tk.Label(zeile, text=t('s_pa_nur_hier'), bg=BG, fg=ACCENT,
                         font=fenster.f_small, anchor='w').pack(side='left')
            # ⚠ Die Bindung muss auf die Zeile UND ihre Etiketten — ein Klick
            # landet auf dem Etikett unter dem Zeiger, nicht auf dem Rahmen
            # darunter. Ohne die Schleife reagiert nur der schmale Rand.
            for teil in [zeile] + list(zeile.winfo_children()):
                teil.bind('<Button-1>',
                          lambda _e, v=eintrag['version']: _patch_waehlen(v))

        if weggelassen:
            tk.Label(liste, text=t('s_pa_leere_weg').format(n=weggelassen),
                     bg=BG, fg=SUB, font=fenster.f_small,
                     anchor='w').pack(fill='x', pady=(6, 0))

    # ----------------------------------------------------------- Der Abruf
    def _suchen():
        stand.configure(text=t('s_pa_laeuft'))
        stand.pack(side='left', padx=(12, 0))

        def arbeit():
            try:
                neu = pa.abgleichen()
            except Exception as ausnahme:
                fehler.merken('seiten.patchaenderungen.abgleich', ausnahme)
                neu = None

            # ⚠ Zurück in den Oberflächen-Faden — Tk verträgt keine Zugriffe
            # aus einem fremden Strang. Und die Seite kann inzwischen weg
            # sein, wenn jemand weitergeklickt hat.
            def nachtragen():
                try:
                    if not stand.winfo_exists():
                        return
                    if neu is None:
                        stand.configure(text=t('s_pa_kein_netz'))
                    elif neu:
                        stand.configure(text=t('s_pa_neu').format(n=len(neu)))
                        _liste_zeigen()
                    else:
                        stand.configure(text=t('s_pa_keine_neuen'))
                except tk.TclError:
                    pass
            try:
                stand.after(0, nachtragen)
            except tk.TclError:
                pass

        threading.Thread(target=arbeit, daemon=True).start()

    _button(fenster, kopf, t('s_pa_suchen'), _suchen).pack(side='left')

    _liste_zeigen()
    # ⚠ Über `_posten_zeigen()`, nicht mit einem fest hingeschriebenen Text.
    # Genau daran ging der Startzustand vorbei: Hier stand `s_pa_waehlen`
    # direkt, also „Wähl links einen Patch aus" — auch dann, wenn links noch
    # gar nichts Abgelegtes liegt. Die Fallunterscheidung steckt in
    # `_posten_zeigen()`; steht sie an zwei Stellen, läuft sie auseinander.
    _posten_zeigen()

    # ⚠ Beim erneuten Öffnen die Liste auffrischen, aber **nicht** von selbst
    # ins Netz greifen: Die Seite wird beim Start im Leerlauf vorgebaut (siehe
    # `_prebuild_pages`) — ein Abruf von allein wäre ein Netzzugriff, den
    # niemand angestoßen hat. Der Knopf ist dafür da.
    def _beim_zeigen():
        _liste_zeigen()
        if zustand['patch']:
            _bereiche_zeigen()
            _posten_zeigen()

    fenster.on_show['patchaenderungen'] = _beim_zeigen
