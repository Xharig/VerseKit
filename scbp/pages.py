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

Die großen Seiten leihen sich die vorhandenen Fenster: `collection_window` und
`settings_window` können auch in einen übergebenen Rahmen
zeichnen, statt ein eigenes Fenster aufzumachen.
"""
import os
import re
import sys
import threading
import time
import tkinter as tk

from . import report, collection as bestand_datei, errors, catalog as katalog_modul
from . import paths, icons, dpi
from .language import t, pa_field
from . import theme

BG      = theme.BG
SURFACE = theme.SURFACE
BAR     = theme.BAR
FG      = theme.FG
SUB     = theme.SUB
ACCENT  = theme.ACCENT
LINE   = theme.LINE
GOLD    = theme.GOLD

# Zustimmung zum Absenden von Bericht und Scan-Bildern — wird gemerkt.
BERICHT_ZUSTIMMUNG = 'bericht_senden_bestaetigt'
RED     = theme.RED
# Fuer Zustaende, die schiefgingen, ohne eine Stoerung zu sein (abgebrochen,
# fehlgeschlagen). Gedaempft gegenueber `ROT`, das den echten Fehlern gehoert.
RED_PALE = theme.RED_PALE

# Wie viele Zeilen das Auftrags-Protokoll zuerst zeigt — der Rest kommt auf
# Klick nach.
#
# ⚠ In tkinter kostet jede Zeile echte Bedienelemente — sie im Voraus zu
# bauen ist die eigentliche Arbeit, nicht das Lesen der Datei. Mit allen
# Zeilen braucht die Seite beim ersten Öffnen weit über eine Sekunde.
#
# ⚠ 40, wie `ZEILEN_ZUERST` in der Bauplan-Liste — dieselbe Sache, dieselbe
# Zahl.
LOG_ROWS_FIRST = 40

# ⚠ Die Klassen heißen in den Daten englisch; angezeigt werden sie übersetzt.
#
# ⚠⚠ **Auf Modulebene**, weil Läden-Seite und Herstellung dieselbe
# Übersetzung brauchen — eine zweite Kopie liefe mit der Zeit auseinander.
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
# ⭐ Die empfohlene Erweiterung für den Hangar-Import: ein gepflegter Fork
# des XPLORer (MIT, AlyxOne), für Chrome, Firefox und Edge. Der
# Hub-Beitrag nennt alle drei Stores; Quellcode-Seite gibt es keine.
HANGAR_EXT_PAGE = ('https://robertsspaceindustries.com/community-hub/post/'
                   'star-citizen-hangar-extension-browser-add-on-7xzYDnJDV6c2W')
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
    # ⚠ Hier geladen, nicht oben im Modul: `stats_pages` holt sich seine
    # Bausteine aus diesem Modul und wäre sonst ein Zirkelbezug.
    from . import stats_pages, settings_pages, basetool_page
    return {
        **stats_pages.builders(),
        **settings_pages.builders(),
        **basetool_page.builders(),
        'darstellung': _appearance,
        'uebersetzung': _translation_page,
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


# Seiten, die es gibt, die aber keinen Reiter haben — Verweise, deren
# Kennung bleibt, weil Nutzungsmeldungen anderer Fassungen sie nennen.
PAGES_WITHOUT_TAB = ('asop',)


def build(fenster, kennung, rahmen):
    """Eine Seite füllen. `fenster` ist das Hauptfenster (Schriften, Meldungen)."""
    _start_tk_poller(fenster.root)
    bauer = _builders().get(kennung)
    if bauer:
        bauer(fenster, rahmen)


# ------------------------------------------------------------------ Bausteine
def style_headings(window, root_widget):
    """Schema „KRT": jede Zwischenüberschrift orange und in Großbuchstaben.

    ⭐ Zentral statt an jeder der rund 17 Stellen: Nach dem Aufbau einer
    Seite wird jede Beschriftung in Titelschrift, die auf dem Seitengrund in
    der normalen Textfarbe steht, umgefärbt. Kachelwerte (andere Fläche,
    Akzentfarbe) und die Seitenüberschrift (schon orange) bleiben, wie sie
    sind. Im Original-Schema tut die Funktion nichts.

    ⚠ Seiten, die ihren Inhalt später neu zeichnen (Statistik), rufen sie
    nach jedem Neuzeichnen noch einmal."""
    if not theme.ACCENT_HEADINGS:
        return
    title_font = str(window.f_title)
    stack = [root_widget]
    while stack:
        widget = stack.pop()
        try:
            stack.extend(widget.winfo_children())
            if (isinstance(widget, tk.Label)
                    and str(widget.cget('font')) == title_font
                    and str(widget.cget('bg')).lower() == BG.lower()
                    and str(widget.cget('fg')).lower() == FG.lower()):
                # Erst in eine Variable: `text=…cget('text')` hielte
                # `texte_pruefen` fuer festen Oberflaechentext.
                gross = theme.heading(widget.cget('text'))
                widget.configure(fg=ACCENT, text=gross)
        except tk.TclError:
            continue


def _heading(window, frame, title, lead=''):
    # Schema „KRT": Überschriften orange und in Großbuchstaben.
    tk.Label(frame, text=theme.heading(title), bg=BG,
             fg=ACCENT if theme.ACCENT_HEADINGS else FG, font=window.f_title,
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
    dehnen — oder **still abgeschnitten** zu werden (etwa Spalten der
    Raffinerien-Tabelle bei „sehr groß"). Tk meckert dabei nicht.

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
    # so eine Liste irgendwann die ganze Seite: Eine Patch-Zeile braucht 30 px, und die eigene Patch-Sammlung wächst absichtlich
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
    # ⚠⚠ **`width=1`, bis die Leinwand ihre echte Breite kennt.**
    # Ohne Breite bekommt der Inhalt einer Leinwand seine WUNSCHbreite — und
    # die ist, solange noch nichts umgebrochen ist, die der längsten
    # Textzeile: 2340 px auf „Allgemein". Alle Umbrüche (`_wrap`) rechneten
    # gegen diese Fantasiebreite und schrumpften die Fläche dann in rund 70
    # Runden herunter — **7874 von rund 10 000 Tk-Aufrufen** beim ersten
    # Öffnen des Hauptfensters. Mit 1 px warten die
    # Umbrüche (sie setzen erst ab 40 px ein) und rechnen einmal, sobald das
    # `<Configure>` der Leinwand die richtige Breite bringt.
    fenster_id = leinwand.create_window((0, 0), window=innen, anchor='nw',
                                        width=1)
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

    Gemessen auf der Joystick-Seite, warmer Wechsel:

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
            if float(werte[1]) > 0.85 and _canvas_shown(leinwand):
                nachlegen()
        except (IndexError, TypeError, ValueError):
            pass

    leinwand.configure(yscrollcommand=beim_rollen)


def _canvas_shown(leinwand):
    """Steht die Rollfläche schon auf dem Bildschirm, mit echter Höhe?

    ⚠⚠ Beim ersten Aufbau meldet Tk die Sicht `(0.0, 1.0)`, solange die
    Leinwand noch 1×1 Pixel groß und nicht gezeigt ist — also alles sichtbar.
    Ohne diese Frage legte das sofort die zweite Portion nach; gemessen auf
    der Steuerung 90 statt 45 Zeilen und fast eine Sekunde länger. Sobald die
    Fläche steht, meldet Tk die Sicht erneut, und dann zählt sie.
    """
    try:
        return bool(leinwand.winfo_ismapped()) and leinwand.winfo_height() > 1
    except tk.TclError:
        return False


def _build_on_demand(leinwand, anzahl, bauer, sofort=ROWS_FIRST,
                       schritt=ROWS_MORE):
    """Nur die sichtbaren Einträge **bauen**, den Rest beim Rollen nachlegen.

    ⚠⚠ **Unterschied zu `_pack_on_demand`:** Dort sind die Zeilen längst
    gebaut und es geht nur ums Layout. Hier werden sie gar nicht erst
    erzeugt — weil nicht das Anzeigen teuer ist, sondern das **Wegwerfen**
    beim nächsten Tastendruck.

    Gemessen auf der Seite „Zerlegen": Von 1,24 s je Anzeigen
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
            if float(werte[1]) > 0.85 and _canvas_shown(leinwand):
                nachlegen()
        except (IndexError, TypeError, ValueError):
            pass

    leinwand.configure(yscrollcommand=beim_rollen)


def after_typing(widget, action):
    """`main_window.after_typing` — hier, weil `main_window` erst innerhalb
    der Funktionen geholt wird."""
    from .main_window import after_typing as _after_typing
    return _after_typing(widget, action)


def _scroll_to_top(widget):
    """Die Rollfläche wieder an den Anfang setzen.

    ⚠⚠ **Nicht dasselbe wie `_rollstelle_halten`.** Der Helfer dort merkt sich
    den **Anteil** und stellt ihn wieder her — richtig, solange der Inhalt
    ungefähr gleich lang bleibt. Schrumpft er stark (eine aufgeklappte
    Vorschlagsliste verschwindet), führt derselbe Anteil hinter das Ende: Oben
    steht dann eine leere Fläche, und der Inhalt fehlt scheinbar.
    """
    # ⛔⛔ Das Attribut heisst `canvas`, nicht `leinwand` — `_scroll_area`
    # setzt es so (`innen_ziel.canvas = leinwand`). Mit falschem Namen
    # liefert `getattr(..., None)` brav `None`,
    # und die Funktion steigt **stillschweigend** aus: kein Fehler,
    # keine Meldung, nur eine Seite, die beim Neuzeichnen nach oben
    # springt.
    leinwand = None
    lauf = widget
    while lauf is not None and leinwand is None:
        # ⚠ **BEIDE Namen.** Die Seiten setzen `canvas`
        # (`_scroll_area`), die beiden eigenstaendigen Fenster
        # `collection_window.py` und `settings_window.py` weiterhin
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


def _files_stamp(*files):
    """Ein Fingerabdruck mehrerer Dateien: Pfad, Änderungszeit, Größe.

    ⭐ Für Seiten, die sich beim Anzeigen neu aufbauen: Stimmt der
    Fingerabdruck mit dem vom letzten Aufbau überein, hat sich keine Quelle
    geändert, und der Neuaufbau kann entfallen. Eine fehlende Datei zählt als
    „fehlt" — taucht sie auf, ändert sich der Abdruck."""
    stamp = []
    for path in files:
        try:
            info = os.stat(path) if path else None
            stamp.append((path, info.st_mtime_ns, info.st_size) if info
                         else (path, None, None))
        except OSError:
            stamp.append((path, None, None))
    return tuple(stamp)


def _keep_scroll(widget, action):
    """Etwas neu zeichnen, ohne dass die Seite nach oben springt.

    ⚠⚠ **Wer eine Liste neu aufbaut, verliert die Rollposition.** Beim Löschen
    eines Postens wird die ganze Tabelle verworfen und neu gezeichnet; die
    Leinwand steht danach wieder bei null, und wer unten am zwölften Eintrag
    war, landet oben.

    Die Stelle wird **vorher** gelesen und **nach** dem Neuzeichnen gesetzt —
    dazwischen ändert sich die Höhe des Inhalts, deshalb erst nach einem
    Leerlauf, wenn Tk den neuen Rollbereich kennt.
    """
    # ⛔⛔ Das Attribut heisst `canvas`, nicht `leinwand` — `_scroll_area`
    # setzt es so (`innen_ziel.canvas = leinwand`). Mit falschem Namen
    # liefert `getattr(..., None)` brav `None`,
    # und die Funktion steigt **stillschweigend** aus: kein Fehler,
    # keine Meldung, nur eine Seite, die beim Neuzeichnen nach oben
    # springt.
    leinwand = None
    lauf = widget
    while lauf is not None and leinwand is None:
        # ⚠ **BEIDE Namen.** Die Seiten setzen `canvas`
        # (`_scroll_area`), die beiden eigenstaendigen Fenster
        # `collection_window.py` und `settings_window.py` weiterhin
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


def _filter_bar(window, parent, fields, on_change, state):
    """Eine Reihe Auswahlfelder plus „Auswahl zurücksetzen" — für jede Seite gleich.

    ⚠⚠ **Ein Bedienkonzept für das ganze Programm.** Wer die
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
    # Reihe wortlos rechts ab — bei fünf Menüs im Laden-Reiter wäre das
    # fünfte sonst nicht mehr bedienbar.
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


def _ensure_size(c, beschriftung, flaeche, hoehe, fuellung, rand, lead=0):
    """Sorgt dafür, dass eine Knopf-Leinwand ihren Text wirklich fasst.

    ⚠ **Einmal beim Bauen zu messen reicht nicht.** `schrift.measure()` sagt,
    wie breit Tk den Text glaubt; gezeichnet wird er mit der Schrift, die das
    System hergibt — und unter Wayland steht die erst fest, wenn das Fenster
    angezeigt wird. Ohne Nachmessen schneidet die Leinwand den Text dann
    beidseitig ab.

    Deshalb wird dreimal nachgesehen: sofort, beim ersten `<Configure>` und
    einmal im Leerlauf. Vergrössert wird nur, wenn es nötig ist — dadurch kommt
    es zur Ruhe, statt sich gegenseitig neu auszulösen.

    `flaeche` ist eine **Liste** mit der Kennung des Rahmens. Wächst die
    Leinwand, wird der Rahmen neu gezeichnet, sonst endet er mitten im Wort;
    die Liste hält die neue Kennung fest, damit die Farbwechsel weiter greifen.

    `lead` ist der Platz links VOR dem Text (etwa eine Flagge): Er
    zählt zur Breite, und der Text wird rechts davon mittig gesetzt — sonst
    schöbe das Nachmessen ihn über das Bild.
    """
    from .main_window import _round_rect

    def nachmessen(_=None):
        try:
            kasten = c.bbox(beschriftung)
        except tk.TclError:
            return
        if not kasten:
            return
        noetig = (kasten[2] - kasten[0]) + 30 + lead
        if noetig <= int(c['width']):
            return
        c.configure(width=noetig)
        c.coords(beschriftung, lead + (noetig - lead) / 2.0, hoehe / 2.0)
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
    # Schema „KRT": starke Knöpfe in Großbuchstaben (Design-System `.btn`).
    if strong and not danger and theme.FILLED_BUTTONS:
        text = text.upper()
    schrift = window.f_small
    hoehe = schrift.metrics('linespace') + 16
    breite = schrift.measure(text) + 30
    # ⚠ `gefahr` faerbt **dauerhaft**, nicht erst beim Überfahren. Ein Knopf,
    # der erst rot wird, wenn die Maus schon darauf steht, warnt niemanden —
    # gesehen hat man ihn dann längst.
    farbe = RED if danger else (ACCENT if strong else FG)
    rand = RED if danger else (ACCENT if strong else LINE)
    # Schema „KRT": starke Knöpfe orange gefüllt, Schrift dunkel.
    gefuellt = strong and not danger and theme.FILLED_BUTTONS
    if gefuellt:
        farbe = BG
    c = tk.Canvas(parent, width=breite, height=hoehe, bg=BG,
                  highlightthickness=0, bd=0, cursor='hand2')
    # ⚠ Erst der Text, dann der Rahmen — und dazwischen wird **nachgemessen**.
    # `schrift.measure()` sagt, wie breit Tk den Text glaubt; gezeichnet wird
    # er mit der Schrift, die das System wirklich hergibt. Weichen die ab, ist
    # die Leinwand zu schmal und schneidet beidseitig ab.
    # `bbox()` liefert die tatsaechliche Ausdehnung, ohne dass das Fenster
    # sichtbar sein muss.
    beschriftung = c.create_text(breite / 2.0, hoehe / 2.0, text=text,
                                 fill=farbe, font=schrift, anchor='center')
    fuellung = (theme.DANGER_FILL if danger
                else (ACCENT if gefuellt
                      else (theme.ACCENT_DARK if strong else SURFACE)))
    flaeche = [_round_rect(c, 1, 1, breite - 1, hoehe - 1, radius=5,
                                fill=fuellung, outline=rand, width=1)]
    c.tag_lower(flaeche[0], beschriftung)

    _ensure_size(c, beschriftung, flaeche, hoehe, fuellung, rand)

    def rein(_=None):
        if gefuellt:
            # Gefüllt bleibt die Schrift dunkel — orange auf Orange wäre weg.
            c.itemconfigure(flaeche[0], outline=FG)
            return
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
        Knopf sah dann aus, als füllte er nur die halbe Kastenbreite.

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
    for eintrag in entries:
        # ⭐ Ein drittes Feld ist ein Bild vor dem Text (die Flaggen der
        # Sprachwahl) — `None` oder weggelassen heißt: nur Text.
        kennung, text = eintrag[0], eintrag[1]
        bild = eintrag[2] if len(eintrag) > 2 else None
        vorlauf = (bild.width() + 6) if bild else 0
        an = (kennung == active)
        hoehe = schrift.metrics('linespace') + 14
        breite = schrift.measure(text) + 26 + vorlauf
        c = tk.Canvas(reihe, width=breite, height=hoehe, bg=BG,
                      highlightthickness=0, bd=0, cursor='hand2')
        c.pack(side='left', padx=(0, 6))
        flaeche = [_round_rect(c, 1, 1, breite - 1, hoehe - 1, radius=5,
                                    fill=SURFACE, outline=ACCENT if an else LINE,
                                    width=1)]
        beschr = c.create_text(vorlauf + (breite - vorlauf) / 2.0,
                               hoehe / 2.0, text=text,
                               fill=ACCENT if an else SUB, font=schrift)
        if bild:
            c.create_image(12, hoehe / 2.0, image=bild, anchor='w')
            c.bild = bild          # Tk räumt Bilder ohne Verweis weg
        # Dieselbe Falle wie beim gewoehnlichen Knopf — siehe `_nachmessen`.
        _ensure_size(c, beschr, flaeche, hoehe, SURFACE,
                      ACCENT if an else LINE, lead=vorlauf)
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
    # ⚠⚠ **`select_quiet` muss es geben, auch wenn es hier dasselbe tut.**
    #
    # Fünf Stellen rufen es auf einer `_choice`-Reihe auf (Overlay-Ecke,
    # Bestandsfenster, Systemmenü). Fehlt es, endet der Aufruf in
    # `AttributeError: 'Frame' object has no attribute 'select_quiet'`.
    #
    # ⚠ **Vier der fünf Stellen stehen in `try/except` und würden den Fehler
    # verschlucken**: Die Auswahl frischte sich dann still nie auf.
    #
    # **Warum dasselbe `setzen`:** Der Unterschied zwischen laut und leise
    # entsteht in `main_window.choice`, wo `select` den Rückruf mit auslöst.
    # Hier hängt der Rückruf am Klick (`c.bind`), `setzen` beschriftet also
    # ohnehin nur. Beide Namen zeigen deshalb auf dieselbe Funktion — der
    # Name bleibt trotzdem nötig, damit die Aufrufer nicht wissen müssen,
    # welche der beiden `choice`-Fassungen sie gerade vor sich haben.
    reihe.select_quiet = setzen
    return reihe


def _status(window, parent, symbol, bold, rest, color=None):
    """Ein Statuskasten mit farbigem Balken links — wie in der Vorschau.

    ⚠ `symbol` ist ein Name aus `scbp/icons.py` („haken", „offen"), kein
    Schriftzeichen.
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
    feld = round_entry(reihe, wert, fenster.f_small, theme.FIELD, LINE, ACCENT, FG)
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
    **TypeError** — und der wird von `except (TclError, ValueError)` nicht
    gefangen, weil ein TypeError keins von beiden ist.

    Unter Linux (Tk 8.6) tritt das regelmäßig auf. Die Folge ist nicht nur
    ein volles Protokoll — `_umbruch` bricht ab, *bevor* es `wraplength`
    setzen kann. Der Text bleibt einzeilig und breit, wird am Fensterrand
    abgeschnitten und drückt die Schalter rechts hinaus.

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
    """
    def nachziehen(_=None):
        try:
            if not label.winfo_exists():
                return
            breite = label.winfo_width()
            if breite <= 40:
                return
            # ⚠ `wraplength` gilt für den Text, `winfo_width` für das ganze
            # Label — die Polsterung links und rechts steckt mit drin. Ohne
            # sie abzuziehen, ist der Text bei `padx=8` ein paar Pixel zu
            # breit (tools/randpruefung.py).
            #
            # ⚠⚠ Genau die Polsterung abziehen, nicht mehr: Ein Label ohne
            # `expand` bekommt seine Wunschbreite, und jeder Pixel Abschlag
            # macht es beim nächsten `<Configure>` wieder schmaler — bis es
            # ein Wort je Zeile zeigt. So passt der Text genau hinein, und die
            # Breite bleibt stehen.
            rand = 2 * sum(label.winfo_pixels(label.cget(option) or 0)
                           for option in ('padx', 'borderwidth',
                                          'highlightthickness'))
            neu = breite - rand
            # ⚠ `_pixels`, nicht `int()` — `cget('wraplength')` liefert unter
            # Linux/Tk 8.6 ein `_tkinter.Tcl_Obj`, und `int()` wirft darauf
            # einen **TypeError**, den `except tk.TclError` nicht fängt.
            # Siehe den Kasten an `_pixels`.
            if _pixels(label, label.cget('wraplength')) != neu:
                label.configure(wraplength=neu, justify='left')
        except tk.TclError:
            pass

    # Gebündelt — siehe `_bundled_idle`.
    label.bind('<Configure>', _bundled_idle(label, nachziehen), add='+')
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
        # Der Fehler stürzt nichts ab (der Haken in `errors.py` fängt ihn), er
        # füllt nur das Protokoll.
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
            # ab (gemessen: rund 5 px).
            #
            # Erfragt statt geschätzt, damit es auch bei anderer Darstellung
            # stimmt.
            try:
                rand = 2 * (_pixels(label, label.cget('borderwidth'))
                            + _pixels(label, label.cget('padx'))
                            + _pixels(label, label.cget('highlightthickness')))
            except tk.TclError:
                rand = 4
            neu = max(160, int(breite * share) - inset - rand)
            try:
                # ⚠⚠ **Nur schreiben, wenn sich der Wert ändert.** Jedes
                # `configure` lässt Tk das Label neu vermessen, das meldet
                # dem Elternrahmen ein `<Configure>` — und der ruft wieder
                # hierher. Ohne diese Bremse laufen beim ersten Öffnen des
                # Hauptfensters Hunderte Nachberechnungen (wie in
                # `_wrap_self`).
                # ⚠ `_pixels`, nicht `int()` — siehe `_wrap_self`: auf einem
                # `_tkinter.Tcl_Obj` wirft `int()` einen TypeError, und der
                # kommt an `except tk.TclError` vorbei.
                if _pixels(label, label.cget('wraplength')) != neu:
                    label.configure(wraplength=neu)
            except tk.TclError:
                pass          # zwischen Prüfung und Zugriff zerstört

    gebuendelt = _bundled_idle(label, nachziehen)
    ziel.bind('<Configure>', gebuendelt, add='+')
    # ⚠ `<Configure>` allein reicht nicht. Seiten werden gebaut, während sie
    # noch versteckt sind — dort meldet Tk Breite 1, und wenn beim späteren
    # Einblenden die Fenstergröße zufällig gleich bleibt, kommt nie ein
    # `<Configure>` mehr. Der Umbruch bliebe dann auf dem Notwert stehen.
    # `<Map>` feuert genau dann, wenn das Element wirklich sichtbar wird.
    label.bind('<Map>', gebuendelt, add='+')
    label.after(0, nachziehen)
    return label


def _bundled_idle(widget, action):
    """Einen Rückruf bündeln: viele Ereignisse, EIN Aufruf, sobald Tk ruht.

    ⚠⚠ **Warum.** Die Umbrüche hängen am `<Configure>` des
    Elternrahmens. Beim ersten Öffnen des Hauptfensters ändert sich dessen
    Größe dutzendfach, bis alles steht — und jedes Mal rechnen alle Labels
    neu: Ungebündelt kommen rund drei Viertel aller Tk-Aufrufe beim Öffnen
    aus `nachziehen`. Gebündelt rechnet jedes Label einmal,
    wenn die Größe feststeht."""
    pending = [None]

    def run():
        pending[0] = None
        action()

    def schedule(_=None):
        if pending[0] is not None:
            return
        try:
            pending[0] = widget.after_idle(run)
        except (tk.TclError, RuntimeError):
            pending[0] = None
    return schedule


def _button_row(parent, buttons, gap=8):
    """Knöpfe nebeneinander — und untereinander, sobald der Platz nicht reicht.

    ⚠ Tk bricht eine Knopfreihe nicht um. Passt sie nicht, schneidet es den
    letzten Knopf einfach ab. ⚠ Die Randprüfung sieht das nur, wenn sie
    Knöpfe nicht als Rollflächen ausnimmt — jeder Knopf hier ist ein `Canvas`.
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
        # Knöpfe sehen aus wie ein Fehler. Bevor umgebrochen wird, fordert
        # die Reihe deshalb die Breite an, die sie braucht.
        #
        # Eine feste Mindestbreite genügt dafür nicht: Wie breit ein Knopf
        # wirklich wird, steht erst fest, wenn er gezeichnet ist — unter
        # Wayland fällt das messbar anders aus.
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
                    # der Knopfreihe nichts zu tun. Mit `oben.winfo_height()`,
                    # also der gerade aktuellen Höhe, säße ein einmal hoch
                    # gezogenes Fenster in seiner eigenen Größe fest und ließe
                    # sich nie wieder niedriger ziehen.
                    #
                    # Wer die Fenstergröße anfasst, fasst genau die Maße an,
                    # um die es geht — und keine weiteren.
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

    # ⚠⚠ **Bei unbekannter Breite NICHT einfach aussteigen.**
    # Das wäre ein Teufelskreis: Ohne Breite würden die Knöpfe nirgends
    # platziert, ohne platzierte Kinder bliebe der Rahmen 1 px breit, und ohne
    # Breitenänderung feuerte nie ein `<Configure>`, das es richten könnte —
    # die Knöpfe blieben dauerhaft unsichtbar.
    #
    # Eine Bereichsauswahl fest über der Rollfläche ist beim ersten Ordnen
    # noch nicht vermessen.
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
    # vorbei.**
    #
    # Die Breiten der Knöpfe **einer Zeile** zu addieren genügt nicht: `grid`
    # richtet nach der breitesten Zelle **je Spalte** aus, über alle Zeilen
    # hinweg. Steht in Spalte 4 irgendeiner Zeile ein breiter Knopf, wird
    # Spalte 4 in JEDER Zeile so breit — und die tatsächlichen Positionen
    # laufen der Rechnung davon. Tk schneidet den Überstand wortlos ab — ohne
    # Rollbalken, ohne Hinweis.
    #
    # Deshalb bekommt jede Spalte dieselbe Breite (die des breitesten
    # Knopfes). Dann ist die Spaltenzahl eine simple Division, und was gerechnet
    # wurde, steht auch so da. Es kostet etwas Leerraum hinter den kurzen
    # Beschriftungen — dafür ist nichts abgeschnitten, und die Knöpfe
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
    # Der Rahmen forderte dann deutlich mehr Breite an, als die Spalten
    # brauchen. Sichtbar abgeschnitten ist nichts, aber die überhöhte
    # Wunschbreite reicht nach oben durch, und die Randprüfung meldet einen
    # Überstand, den es gar nicht gibt.
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

    Tk schneidet eine zu breite Reihe wortlos ab — was rechts
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
    nicht. Die Sternchen in `language.py` markieren die Betonung fuer den
    Leser der Sprachdatei; auf dem Bildschirm haben sie nichts zu suchen.

    Damit das nicht bei jedem neuen Text einzeln bedacht werden muss, geht
    es durch diese eine Stelle.

    ⚠ Dasselbe gilt fuer die Rueckstriche um Befehle und Werte. Sie kommen aus
    dem Änderungsprotokoll, das die Seite „Was ist neu" anzeigt.
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
        # Englisch sind die Wörter länger, und rechts würde der letzte Knopf
        # abgeschnitten.
        rechts = tk.Frame(links, bg=BG)
        rechts.pack(fill='x', anchor='w', pady=(8, 0))
        # ⚠ Auch hier braucht es einen Abzug. Ohne ihn bekommt der Text die
        # **volle** Breite der Zeile — die Ränder der Rollfläche darum sind
        # damit nicht eingerechnet, und die letzten Pixel fallen weg
        # (gemessen: 5, tools/randpruefung.py).
        #
        # Die Beschriftung braucht denselben Umbruch: Auf Englisch sind die
        # Wörter länger.
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
    from . import collection_window
    # ⭐ Rückweg zum Hauptfenster — die Liste braucht ihn, um auf andere Seiten
    # zu springen (andersherum geht der Weg über `stock_page`).
    #
    # ⚠⚠ **Als Argument, nicht danach zugewiesen.** Der Konstruktor zeichnet
    # die Liste bereits; wer den Rückweg erst hinterher setzt, hat beim ersten
    # Zeichnen keinen — und dann ist kein Name anklickbar, bis zufällig neu
    # gezeichnet wird.
    fenster.stock_page = collection_window.Bestandsfenster(rahmen=rahmen,
                                                            hauptfenster=fenster)

    # ⚠ Beim erneuten Aufrufen ohne Filter anfangen. Die Seite wird nur ein-
    # und ausgeblendet, sonst stünde die Auswahl von vorhin noch da — und wer
    # den alten Filter vergessen hat, sieht „Nichts gefunden" und hält den
    # Bestand für leer.
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

    ⚠ Alle 25 Kategorien in einer einzigen langen Liste wären bei über 700
    Bauplänen kaum zu durchsuchen. Deshalb zuerst die vier Bereiche mit ihrem
    Gesamtstand — und die Einzelheiten erst auf Klick. Eingeklappt starten:
    Der Überblick zeigt, wie weit man ist; die Kategorien zeigen, wo genau.

    ⭐ **„Nur Merkliste"** — zeigt den Fortschritt nur für die Baupläne der
    Merkliste. Gezählt werden
    nur **angeklickte** Baupläne; eigene Beobachtungen mit Suchmuster stehen
    für kein bestimmtes Teil und haben deshalb keinen Fortschritt. Die Wahl
    bleibt über den Neustart erhalten (`fortschritt_merkliste`).
    """
    _heading(fenster, rahmen, t('hf_fortschritt'), t('s_fo_lead'))
    inner = _scroll_area(rahmen)
    try:
        stock = bestand_datei.load()
        catalog = katalog_modul.load()
    except Exception as exc:
        errors.record('pages.fortschritt', exc)
        return

    # ⚠⚠⚠ **Sind es weniger Baupläne als je zuvor?** Dann steht das hier —
    # oben, wo die Zahl steht, über die man stolpert. Typischer Fall: Die
    # Zeiger-Datei auf den Datenordner ist weg, der Watcher nimmt still den
    # Standardordner, und zurück bleibt nur eine kleinere Zahl.
    #
    # ⚠ `shrinkage_state()` und nicht `check_shrinkage()`: Letzteres würde beim
    # Hinsehen den kleineren Stand als neuen Höchstwert festschreiben, und die
    # Meldung wäre nach einmal Ansehen für immer weg.
    shrinkage = bestand_datei.shrinkage_state()
    if shrinkage:
        present_now, highest, earlier = shrinkage
        box = tk.Frame(inner, bg=SURFACE, highlightthickness=1,
                       highlightbackground=GOLD)
        box.pack(fill='x', pady=(0, 10))
        tk.Label(box, text=t('s_schwund_titel'), bg=SURFACE, fg=GOLD,
                 font=fenster.f_bold).pack(anchor='w', padx=12, pady=(10, 2))
        tk.Label(box, text=t('s_schwund_text') % (present_now, highest),
                 bg=SURFACE, fg=FG, font=fenster.f_small, justify='left',
                 wraplength=720).pack(anchor='w', padx=12)
        # ⚠ Beide Pfade im Klartext — die Frage ist ja gerade „welcher Ordner
        # denn nun". Ohne sie ist die Meldung eine Feststellung ohne Ausweg.
        for caption, place in ((t('s_schwund_wo'), earlier),
                               (t('s_schwund_jetzt'), paths.app_folder())):
            if not place:
                continue
            tk.Label(box, text=caption, bg=SURFACE, fg=SUB,
                     font=fenster.f_small).pack(anchor='w', padx=12,
                                                pady=(6, 0))
            tk.Label(box, text=place, bg=SURFACE, fg=ACCENT,
                     font=fenster.f_small, justify='left',
                     wraplength=720).pack(anchor='w', padx=24)
        tk.Label(box, text=t('s_schwund_tipp'), bg=SURFACE, fg=SUB,
                 font=fenster.f_small, justify='left',
                 wraplength=720).pack(anchor='w', padx=12, pady=(8, 10))

    blueprints = catalog.get('bauplaene') or {}
    owned = set(stock.get('bauplaene') or {})
    mode = ('merk' if paths.setting_bool(PROGRESS_WATCHLIST_SETTING, False)
            else 'alle')

    # Die Auswahl steht fest oben, der Inhalt darunter wird bei jedem Wechsel
    # neu gebaut — die Seite selbst wird nur einmal gebaut, ein Neubau beim
    # Umschalten erreicht sie also nie.
    choice_row = _choice(fenster, inner,
                         [('alle', t('s_fo_alle')), ('merk', t('s_fo_merk'))],
                         mode, lambda chosen: show(chosen))
    choice_row.pack(anchor='w', pady=(0, 12))
    content = tk.Frame(inner, bg=BG)
    content.pack(fill='x')

    def show(chosen):
        choice_row.select_quiet(chosen)
        paths.set_setting(PROGRESS_WATCHLIST_SETTING, chosen == 'merk')
        for child in content.winfo_children():
            child.destroy()
        _progress_content(fenster, content, catalog, blueprints, owned,
                          chosen == 'merk')

    _progress_content(fenster, content, catalog, blueprints, owned,
                      mode == 'merk')


# Einstellung: Zeigt „Bauplan-Fortschritt" nur die Merkliste?
PROGRESS_WATCHLIST_SETTING = 'fortschritt_merkliste'


def progress_counts(blueprints, owned, only_keys=None):
    """Je Bereich und Kategorie: `(gesamt, meine)`.

    `only_keys` schränkt auf diese Katalogschlüssel ein (die Merkliste) —
    `None` heißt alle. Frei von Tk, damit es sich prüfen lässt.
    """
    by_area = {}
    for key, entry in blueprints.items():
        if only_keys is not None and key not in only_keys:
            continue
        raw = katalog_modul.kind_id(entry)
        area = katalog_modul.top_group(raw)
        kind = katalog_modul.kind_readable(raw) if raw else '—'
        counter = by_area.setdefault(area, {})
        total, mine = counter.get(kind, (0, 0))
        counter[kind] = (total + 1, mine + (1 if key in owned else 0))
    return by_area


def _progress_content(fenster, parent, catalog, blueprints, owned, watchlist_only):
    """Gesamtzahl, Balken, Bereiche — für alle Baupläne oder nur die Merkliste."""
    only_keys = None
    if watchlist_only:
        from . import watchlist
        watched = watchlist.names()
        only_keys = {k for k in blueprints if k in watched}
        if not only_keys:
            _body_text(parent, t('s_fo_merk_leer'), fenster.f_small, fill='x')
            return

    by_area = progress_counts(blueprints, owned, only_keys)
    total_all = sum(g for z in by_area.values() for g, _ in z.values()) or 1
    mine_all = sum(m for z in by_area.values() for _, m in z.values())

    head = tk.Frame(parent, bg=BG)
    head.pack(fill='x', pady=(0, 4))
    tk.Label(head, text=str(mine_all), bg=BG, fg=ACCENT,
             font=fenster.f_title).pack(side='left')
    tk.Label(head, text=t('s_fo_von') % (total_all, 100.0 * mine_all / total_all),
             bg=BG, fg=SUB, font=fenster.f_small).pack(side='left')

    from .main_window import round_bar
    round_bar(parent, 9, mine_all / float(total_all), BG, theme.HOVER,
              ACCENT).pack(fill='x', pady=(6, 18))

    for area in katalog_modul.TOP_GROUPS:
        counter = by_area.get(area)
        if not counter:
            continue
        total = sum(g for g, _ in counter.values())
        mine = sum(m for _, m in counter.values())
        _progress_section(fenster, parent, t('gruppe_' + area), total, mine,
                          counter)

    # ⚠ „Was bringt am meisten?" nur bei ALLEN Bauplänen: Es zählt fehlende
    # Baupläne über den ganzen Katalog. Unter „Nur Merkliste" würde es Aufträge
    # für Teile empfehlen, die man gar nicht will.
    if not watchlist_only:
        _best_contracts(fenster, parent, catalog, owned)


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
        errors.record('pages.lohnende_auftraege', ausnahme)
        return

    # ⭐ Auf- und zuklappbar wie die Bereiche darüber. Zehn Aufträge mit je
    # vier Zeilen sind der mit Abstand längste Block der Seite; wer nur
    # wissen will, wie weit er ist, scrollt sonst an ihm vorbei. **Startet
    # zugeklappt wie die Bereiche.**
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
    # ⚠ Nur die ersten zehn. Es sind 170 — eine vollständige Liste wäre
    # keine Empfehlung mehr, sondern die nächste Suchaufgabe.
    # ⚠ **Der Annahmeort gehört an die Zeile** — `lohnende_auftraege` liefert
    # ihn als sechsten Wert, `ort_text()` macht daraus den Text wie im
    # Bauplan-Fenster.
    from .collection_window import ort_text
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
        # Der Annahmeort steht direkt in der Zeile.
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
    balken = round_bar(eltern, 9, anteil, BG, theme.HOVER, ACCENT)
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
            balken_zeile = round_bar(zeile, 7, teil, BG, theme.HOVER, ACCENT,
                                      width=260)
            balken_zeile.pack(side='left', padx=8)
            zahl = tk.Label(zeile, text='%d / %d' % (art_meine, art_gesamt),
                            bg=BG, fg=SUB, font=fenster.f_small, width=10,
                            anchor='e')
            zahl.pack(side='right')

            # ⭐ „38 / 70" sagt nicht, WELCHE 32 fehlen. Der Klick führt in
            # die Bauplan-Liste, gefiltert auf genau diese Kategorie.
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
            # eines Bereichs landeten ohne Anker ganz unten, hinter dieser
            # Liste, statt unter ihrem eigenen Balken: Man klappt oben etwas
            # auf, und das Aufgeklappte liegt mehrere Bildschirmhöhen tiefer.
            #
            # ⚠ Das droht nur, weil `koerper` erst beim ERSTEN
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
        from . import settings_window
        leer = tk.Frame(window.root, bg=BG)     # nur als Halter, wird nie gepackt
        window._settings_window = settings_window.SettingsWindow(rahmen=leer)
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
    from . import autostart, paths
    from .main_window import toggle_switch
    _heading(fenster, rahmen, t('hf_allgemein'),
                  t('s_allg_lead'))
    innen = _scroll_area(rahmen)

    # ⚠ Die Sprache steht unter „Darstellung": Sie ändert, wie das Programm
    # aussieht, nicht, wie es
    # sich verhält. „Allgemein" ist das Programmverhalten.
    ziel = _setting_row(fenster, innen, t('e_ton'),
                 t('s_ton_h'))

    def ton_um():
        neu_wert = not paths.setting_bool('signalton', True)
        paths.set_setting('signalton', neu_wert)
        fenster.say('%s: %s' % (t('e_ton'), t('e_an') if neu_wert else t('e_aus')))
        return neu_wert

    toggle_switch(ziel, paths.setting_bool('signalton', True),
                    ton_um).pack()

    # ⚠ Standardmaessig AUS. Gezaehlt wird trotzdem von
    # Anfang an — sonst begaenne die Zaehlung erst beim Einschalten, und die
    # Protokolle davor haette Star Citizen dann laengst weggeraeumt. Was nichts
    # kostet und sich nicht nachholen laesst, sammelt man besser mit.
    ziel = _setting_row(fenster, innen, t('s_zeit'), t('s_zeit_h'))

    def zeit_um():
        neu_wert = not paths.setting_bool('spielzeit_zeigen', False)
        paths.set_setting('spielzeit_zeigen', neu_wert)
        # ⚠ Die Kopfzeile wird beim Fensterbau EINMAL zusammengesetzt. Ohne
        # Neuaufbau bliebe der Schalter wirkungslos, bis das Programm neu
        # startet — und das sieht aus, als tue er nichts.
        fenster.root.after(60, fenster.rebuild)
        return neu_wert

    toggle_switch(ziel, paths.setting_bool('spielzeit_zeigen', False),
                    zeit_um).pack()

    ziel = _setting_row(fenster, innen,
                 t('autostart_win') if sys.platform.startswith('win')
                 else t('autostart_linux'),
                 t('s_autostart_h'))
    if autostart.possible():
        def autostart_um():
            neu_wert = not autostart.is_on()
            autostart.set_on(neu_wert)
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
            neu_wert = not paths.setting_bool('tray', True)
            paths.set_setting('tray', neu_wert)
            return neu_wert

        toggle_switch(ziel, paths.setting_bool('tray', True),
                        tray_um).pack()
    else:
        tk.Label(ziel, text=t('s_nur_win'), bg=BG, fg=SUB,
                 font=fenster.f_small).pack()

    # Dieselbe Einstellung wie auf „Statistik → Auswertung" — hier, weil sie
    # das Programmverhalten beim Start betrifft.
    from . import play_stats
    ziel = _setting_row(fenster, innen, t('s_sa_auto'), t('s_sa_auto_h'))

    def stats_auto_flip():
        new_value = not play_stats.auto_enabled()
        paths.set_setting(play_stats.AUTO_SETTING, new_value)
        fenster.say('%s: %s' % (t('s_sa_auto'),
                                t('e_an') if new_value else t('e_aus')))
        return new_value

    toggle_switch(ziel, play_stats.auto_enabled(), stats_auto_flip).pack()

    # ⭐ Tägliche Nutzungsmeldung — ab Werk an, hier abschaltbar.
    # Was hinausgeht, sagt der Hilfetext wörtlich (Version und System).
    from . import usage_ping
    ziel = _setting_row(fenster, innen, t('s_nutzung'), t('s_nutzung_h'))

    def usage_flip():
        new_value = not usage_ping.enabled()
        paths.set_setting(usage_ping.SETTING, new_value)
        fenster.say('%s: %s' % (t('s_nutzung'),
                                t('e_an') if new_value else t('e_aus')))
        return new_value

    toggle_switch(ziel, usage_ping.enabled(), usage_flip).pack()

    # Verbindung zu scmdb.net (Log-Watcher-Protokoll) — ab Werk aus.
    from . import scmdb_bridge
    ziel = _setting_row(fenster, innen, t('s_scmdb'), t('s_scmdb_h'))

    def scmdb_flip():
        new_value = not scmdb_bridge.enabled()
        paths.set_setting(scmdb_bridge.SETTING, new_value)
        scmdb_bridge.apply_setting()
        if not new_value:
            state = t('e_aus')
        elif scmdb_bridge.STATUS['state'] == 'busy':
            state = t('s_scmdb_belegt')
        else:
            state = t('s_scmdb_bereit')
        fenster.say('%s: %s' % (t('s_scmdb'), state))
        return new_value

    toggle_switch(ziel, scmdb_bridge.enabled(), scmdb_flip).pack()


def _flag(code, master=None):
    """Eine Flagge in Zeilengröße — oder None, wenn das Bild fehlt.

    ⚠ **Die einzige Ausnahme neben Discord und Ko-fi** von der Regel „Symbole
    nur aus dem Lucide-Satz": Lucide führt keine
    Flaggen, Emoji sind verboten. Die Bilder stammen aus „flag-icons"
    (MIT-Lizenz, Vorlage und Lizenz unter `tools/flaggen-vorlagen/`) und
    liegen je Zeilengröße fertig unter `assets/flaggen/`."""
    size = icons.LINE.get(icons.level(), 14)
    if dpi.active():
        size = min(sorted(set(icons.LINE.values())),
                   key=lambda s: (abs(s - dpi.px(size)), -s))
    from .main_window import _bundled
    # Der Name in einer eigenen Zeile: Die Bau-Prüfung liest Dateinamen aus
    # `_bundled(…)` und hielte das Muster sonst für eine Datei.
    name = '%s-%d.png' % (code, size)
    path = _bundled(os.path.join('assets', 'flaggen', name))
    try:
        # ⚠ `master` bei Fenstern mit eigenem Tk (Assistent): Sonst landet das
        # Bild im ersten Tk des Prozesses, und das Label meldet
        # „image pyimage… doesn't exist".
        return (tk.PhotoImage(file=path, master=master)
                if os.path.isfile(path) else None)
    except tk.TclError:
        return None


def _language_row(window, inner):
    """Sprache der Oberfläche — auf „Darstellung"."""
    from . import paths
    parts = _settings_parts(window)
    target = _setting_row(window, inner, t('e_sprache'), t('s_sprache_h'),
                          wide=True)
    choice = _choice(window, target,
                     [('auto', t('sprache_auto')),
                      ('de', 'Deutsch', _flag('de')),
                      ('en', 'English', _flag('gb'))],
                     paths.settings().get('sprache') or 'auto',
                     lambda k: (choice.select(k), parts._choose_language(k)))
    choice.pack()


def _appearance(window, frame):
    """Darstellung: Sprache, Größe und Farbschemata."""
    _heading(window, frame, t('hf_darstellung'), t('s_da_lead'))
    inner = _scroll_area(frame)
    _scheme_row(window, inner)
    _language_row(window, inner)
    _font_size_row(window, inner)
    _sharp_row(window, inner)


def _sharp_row(window, inner):
    """Scharfe Darstellung bei Windows-Skalierung über 100 % (`dpi`) — wirkt
    nach einem Neustart, ab Werk aus."""
    from .main_window import toggle_switch
    target = _setting_row(window, inner, t('s_da_scharf'), t('s_da_scharf_h'))
    if not sys.platform.startswith('win'):
        tk.Label(target, text=t('s_nur_win'), bg=BG, fg=SUB,
                 font=window.f_small).pack()
        return
    restart = tk.Frame(inner, bg=BG)

    def flip():
        new_value = not dpi.wanted()
        paths.set_setting(dpi.SETTING, new_value)
        for child in restart.winfo_children():
            child.destroy()
        if new_value != dpi.active():
            _body_text(restart, t('s_da_scharf_neustart'), window.f_small,
                       fill='x', pady=(8, 4))
            _button(window, restart, t('s_da_neustart_knopf'),
                    lambda: restart_and_hand_over(window),
                    strong=True).pack(anchor='w')
        window.say('%s: %s' % (t('s_da_scharf'),
                               t('e_an') if new_value else t('e_aus')))
        return new_value

    toggle_switch(target, dpi.wanted(), flip).pack()
    restart.pack(fill='x')


def _scheme_row(window, inner):
    """Das Farbschema — wirkt nach einem Neustart.

    ⚠ Nicht sofort: Die Farben werden beim Laden der Module gelesen, und
    jedes Fenster hält sie als Konstanten (siehe `theme`). Ein halb
    umgefärbtes Programm wäre schlimmer als ein ehrlicher Neustart."""
    target = _setting_row(window, inner, t('s_da_schema'), t('s_da_schema_h'),
                          wide=True)
    chosen = paths.settings().get(theme.SETTING) or theme.DEFAULT
    restart = tk.Frame(target, bg=BG)

    def pick(name):
        choice.select(name)
        theme.choose(name)
        for child in restart.winfo_children():
            child.destroy()
        if name != theme.NAME:
            _body_text(restart, t('s_da_neustart'), window.f_small,
                       fill='x', pady=(8, 4))
            _button(window, restart, t('s_da_neustart_knopf'),
                    lambda: restart_and_hand_over(window),
                    strong=True).pack(anchor='w')
        window.say(t(theme.SCHEMES[name]['label']))

    choice = _scheme_cards(window, target,
                           chosen if chosen in theme.SCHEMES else theme.DEFAULT,
                           pick)
    choice.pack(anchor='w')
    restart.pack(fill='x')


def _scheme_cards(window, parent, active, action, compact=False):
    """Je Schema eine kleine Vorschau in SEINEN Farben.

    Gezeichnet werden Kopfleiste, zwei Zeilen und ein Akzentbalken — keine
    Symbole, also keine Ausnahme von der Regel „nichts selbst malen": Es ist
    eine Abbildung des Fensters, kein Bedienzeichen. Der Haken der gewählten
    Karte kommt aus dem Symbolsatz.

    ⭐ **`compact` für den Einrichtungsassistenten.** Ein Schema wählt man
    nach dem Aussehen, deshalb steht die Vorschau auch dort und nicht nur in
    den Einstellungen.

    Kompakt heißt kleiner und alle sechs in **einer** Reihe: Das Fenster des
    Assistenten ist 640 px breit, und zwei Reihen à 78 px hätten die Seite
    über den Rand geschoben."""
    row = tk.Frame(parent, bg=BG)
    cards = {}
    # ⚠ Drei je Reihe, nicht alle nebeneinander: Es gibt sechs
    # Schemata — nebeneinander wären das über 1000 px, mehr als die Seite hat.
    width, height = (88, 46) if compact else (170, 78)
    per_row = len(theme.SCHEMES) if compact else 3
    for index, (name, scheme) in enumerate(theme.SCHEMES.items()):
        box = tk.Frame(row, bg=BG)
        box.grid(row=index // per_row, column=index % per_row, sticky='w',
                 padx=(0, 12), pady=(0, 10))
        c = tk.Canvas(box, width=width, height=height, bg=BG,
                      highlightthickness=0, bd=0, cursor='hand2')
        c.pack()
        # ⚠ Alle Innenmaße als Bruchteil der Höhe — sonst zeichnet die
        # kompakte Karte dieselben Balken wie die große und läuft über.
        f = height / 78.0

        def _y(wert, _f=f):
            return int(round(wert * _f))

        c.create_rectangle(2, 2, width - 2, height - 2, fill=scheme['bg'],
                           outline=scheme['line'], width=2, tags='rand')
        c.create_rectangle(4, 4, width - 4, _y(18), fill=scheme['bar'],
                           outline='')
        c.create_oval(_y(10), _y(8), _y(16), _y(14), fill=scheme['accent'],
                      outline='')
        c.create_rectangle(_y(12), _y(28), width - _y(12), _y(34),
                           fill=scheme['surface'], outline='')
        c.create_rectangle(_y(12), _y(40), width - _y(50) if not compact
                           else width - _y(34), _y(46),
                           fill=scheme['surface'], outline='')
        c.create_rectangle(_y(12), _y(56), width - _y(30), _y(64),
                           fill=scheme['accent'], outline='')
        if scheme['square']:
            c.create_line(6, _y(30), 6, _y(24), _y(12), _y(24),
                          fill=scheme['accent'], width=2)
        label_row = tk.Frame(box, bg=BG)
        label_row.pack(pady=(4, 0))
        mark = icons.line(label_row, 'haken', color=icons.GREEN,
                          background=BG, font=window.f_small)
        text = tk.Label(label_row, text=t(scheme['label']), bg=BG, fg=SUB,
                        font=window.f_small, cursor='hand2')
        text.pack(side='left')
        for part in (c, text):
            part.bind('<Button-1>', lambda _e, k=name: action(k))
        cards[name] = (c, mark, text)

    def select(chosen):
        for name, (c, mark, text) in cards.items():
            on = name == chosen
            c.itemconfigure('rand', outline=ACCENT if on
                            else theme.SCHEMES[name]['line'])
            text.configure(fg=ACCENT if on else SUB)
            if on:
                mark.pack(side='left', padx=(0, 4), before=text)
            else:
                mark.pack_forget()

    select(active)
    row.select = select
    return row


def _font_size_row(window, inner):
    """Größe der Oberfläche: Regler stufenlos, darunter Voreinstellungen.

    ⭐ Wird der Regler benutzt, fällt die Auswahl der Voreinstellungen weg.

    ⚠ Angewendet wird beim **Loslassen**, nicht beim Ziehen: Jede neue Größe
    baut das ganze Fenster neu auf (siehe `set_font_size`) — bei jedem
    Zwischenschritt wäre das ein Flackern. Während des Ziehens wandert nur die
    Prozentzahl mit.

    Die größte Stufe des Reglers liegt über „Sehr groß"."""
    # ⚠ „Sehr groß" ist unbedenklich: Die Mindestgrösse liegt dort bei rund
    # **1215 × 380 px**, auch wenn alle Seiten gebaut sind — die Mindesthöhe
    # hängt nicht an der Schriftstufe (`minsize()`). Würde sie es, könnte die
    # Stufe das Fenster grösser als den Bildschirm machen.
    from .main_window import (slider, font_points, font_percent, auto_points,
                              FONT_POINTS_RANGE, FONT_PRESETS)
    target = _setting_row(window, inner, t('hf_schrift'), t('hf_schrift_hilfe'),
                          wide=True)
    points = font_points()
    row = tk.Frame(target, bg=BG)
    row.pack(fill='x', anchor='w')
    state = {'points': points}
    shown = tk.Label(row, text='%d %%' % font_percent(points), bg=BG, fg=FG,
                     font=window.f_bold, width=6, anchor='e')

    def drag(value):
        state['points'] = value
        shown.configure(text='%d %%' % font_percent(value))

    bar = slider(row, FONT_POINTS_RANGE[0], FONT_POINTS_RANGE[1], points, drag,
                 width=320)
    bar.pack(side='left')
    shown.pack(side='left', padx=(12, 0))

    def release(_event=None):
        # Der Regler hebt die Voreinstellung auf — die Wahl darunter fällt weg.
        paths.set_setting('schrift_voreinstellung', '')
        if state['points'] != font_points():
            window.set_font_size(state['points'])

    bar.bind('<ButtonRelease-1>', release)

    def preset(name):
        value = dict(FONT_PRESETS).get(name)
        if value is None:
            value = auto_points(window.root.winfo_screenheight())
        paths.set_setting('schrift_voreinstellung', name)
        window.set_font_size(value)

    chosen = paths.settings().get('schrift_voreinstellung') or ''
    choice = _choice(window, target,
                     [(name, t('s_gr_' + name)) for name, _v in FONT_PRESETS],
                     chosen, preset)
    choice.pack(anchor='w', pady=(8, 0))


def _display(fenster, rahmen):
    from . import paths
    from .main_window import toggle_switch
    _heading(fenster, rahmen, t('hf_overlay'),
                  t('s_anz_lead'))
    innen = _scroll_area(rahmen)
    e = _settings_parts(fenster)

    # --- Wie sich das Overlay im Spiel verhält -------------------------------
    # Ein dauerhaft sichtbares Overlay, das Klicks nicht durchreicht, stört im
    # Kampf, sobald die Maus hineinkommt — deshalb wählbar.
    ziel = _setting_row(fenster, innen, t('s_ov_modus'), t('s_ov_modus_h'), wide=True)
    modus = _choice(fenster, ziel,
                  [('immer', t('s_ov_immer')), ('popup', t('s_ov_popup'))],
                  paths.setting('overlay_modus') or 'immer',
                  lambda k: _overlay_mode(fenster, modus, k))
    modus.pack()

    # ⚠⚠ **Die Tastenkombination.** Star Citizen laeuft im Vollbild und blendet
    # den Mauszeiger aus: Wer nachsehen will, ob er einen Bauplan schon hat,
    # muss heraustabben und das Fenster dann BLIND suchen und anklicken.
    # ⚠⚠ **Die Ecke — im Pop-up-Betrieb der einzige Weg.** Dort reicht das
    # Overlay Mausklicks durch und laesst sich deshalb nicht ziehen. Ohne
    # diese Einstellung koennen diese Nutzer es ueberhaupt nicht
    # positionieren.
    ziel = _setting_row(fenster, innen, t('s_ov_ecke'), t('s_ov_ecke_h'), wide=True)
    ecke = _choice(fenster, ziel,
                 [('frei', t('s_ov_ecke_frei')),
                  ('oben-links', t('s_ov_ecke_ol')),
                  ('oben-rechts', t('s_ov_ecke_or')),
                  ('unten-links', t('s_ov_ecke_ul')),
                  ('unten-rechts', t('s_ov_ecke_ur'))],
                 paths.setting('overlay_ecke') or 'frei',
                 lambda k: _overlay_corner(fenster, ecke, k))
    ecke.pack()
    # Für „Fensterlage zurücksetzen": Die Auswahl muss den Rückweg auf „frei"
    # zeigen.
    fenster._overlay_corner_choice = ecke
    # ⭐ Zieht jemand das Overlay mit der Hand woandershin, hebt es die Ecke
    # selbst auf (`Overlay._verschoben`) — diese Liste muss das sehen, sonst
    # steht hier weiter „unten links", waehrend das Fenster woanders sitzt.
    # ⚠ `select_quiet`: Die Auswahl soll sich nur neu beschriften, nicht den
    # Rueckruf ausloesen — der wuerde die Ecke gleich wieder anwenden.
    from . import overlay as _ov_anzeige
    _ov_anzeige.CORNER_DISPLAY[0] = lambda k: ecke.select_quiet(k)

    # ⭐ **Wo die Leiste sitzt, ist eine eigene Einstellung.** Hinge sie an
    # der Ecke, wäre sie nach jedem Verschieben (Ecke „frei") immer oben.
    ziel = _setting_row(fenster, innen, t('s_ov_leiste'), t('s_ov_leiste_h'),
                 wide=True)
    leiste = _choice(fenster, ziel,
                   [('oben', t('s_ov_leiste_oben')),
                    ('unten', t('s_ov_leiste_unten'))],
                   paths.setting('overlay_leiste') or 'oben',
                   lambda k: _overlay_bar(fenster, leiste, k))
    leiste.pack()
    # Die Ecke setzt die Leiste mit — die Auswahl hier muss das zeigen.
    fenster._overlay_bar_choice = leiste

    _hotkey_field(fenster, innen)

    ziel = _setting_row(fenster, innen, t('s_ov_dauer'), t('s_ov_dauer_h'))
    from .main_window import round_entry as _zahlfeld
    dauer = _zahlfeld(ziel, None, fenster.f_small, theme.FIELD, LINE, ACCENT, FG,
                      width=6, justify='right')
    dauer.insert(0, str(paths.setting_int('popup_sekunden', 6, 2, 60)))
    dauer.holder.pack()

    def dauer_merken(_=None):
        try:
            wert = max(2, min(60, int(dauer.get())))
            paths.set_setting('popup_sekunden', wert)
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
            ziel, paths.setting_bool('durchklickbar', False),
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
            e._preview_opacity(w)
        except Exception:
            pass
        paths.set_setting('deckkraft_prozent', w)

    schieberegler(reihe, 30, 100, e.deckkraft.get(),
                  deckkraft_setzen).pack(side='left')
    wertlabel.pack(side='left', padx=(8, 0))

    ziel = _setting_row(fenster, innen, t('s_klapp'),
                 t('s_klapp_h'))

    def klapp_um():
        neu_wert = not paths.setting_bool('eingeklappt', False)
        paths.set_setting('eingeklappt', neu_wert)
        return neu_wert

    toggle_switch(ziel, paths.setting_bool('eingeklappt', False),
                    klapp_um).pack()

    ziel = _setting_row(fenster, innen, t('s_vorne'),
                 t('s_vorne_h'))

    def vorne_um():
        neu_wert = not paths.setting_bool('immer_vorne', True)
        paths.set_setting('immer_vorne', neu_wert)
        fenster.say(t('s_an_vorne')
                      % (t('e_an') if neu_wert else t('e_aus')))
        return neu_wert

    toggle_switch(ziel, paths.setting_bool('immer_vorne', True),
                    vorne_um).pack()

    ziel = _setting_row(fenster, innen, t('s_zeilen'),
                 t('s_zeilen_h'))
    from .main_window import round_entry
    zahl = round_entry(ziel, None, fenster.f_small, theme.FIELD, LINE, ACCENT, FG,
                       width=6, justify='right')
    zahl.insert(0, str(paths.setting_int('max_zeilen', 20, 5, 100)))
    zahl.holder.pack()

    def zahl_merken(_=None):
        try:
            paths.set_setting('max_zeilen',
                                     max(5, min(100, int(zahl.get()))))
            fenster.say(t('s_an_zeilen') % zahl.get())
        except ValueError:
            pass

    zahl.bind('<FocusOut>', zahl_merken)
    zahl.bind('<Return>', zahl_merken)

    ziel = _setting_row(fenster, innen, t('s_lage'),
                 t('s_lage_h'))

    def reset_position():
        # Die gemerkte Lage wegwerfen reicht nicht: Ohne Positionsangabe stellt Tk
        # das Fenster nach `+0+0`, und bei einem hochkant stehenden Monitor links
        # außen liegt dort gar kein Bild — der Knopf hätte das Overlay also wieder
        # dorthin geschickt, wo man es sucht. Deshalb wird aktiv die Standardlage
        # gesetzt: mittig auf dem Hauptbildschirm. Wie viele Bildschirme jemand hat,
        # wissen wir nicht; die Mitte des Hauptbildschirms passt überall.
        #
        # ⚠⚠ **Zurücksetzen heißt ALLES zurück** — auch Ecke und Leiste.
        # Blieben sie stehen, spränge ein Overlay mit fester Ecke wieder
        # dorthin, wo es unerreichbar war. Die Größe wird begrenzt, sonst wäre
        # sie auf einem Laptop höher als der Bildschirm (siehe
        # `groesse_begrenzen`).
        from . import screen
        from . import overlay as overlay_module
        try:
            os.remove(paths.app_file('watcher.json'))
        except OSError:
            pass
        paths.set_setting('overlay_ecke', 'frei')
        paths.set_setting('overlay_leiste', 'oben')
        for name, value in (('_overlay_corner_choice', 'frei'),
                            ('_overlay_bar_choice', 'oben')):
            choice = getattr(fenster, name, None)
            if choice is not None:
                try:
                    choice.select_quiet(value)
                except Exception:
                    pass
        # ⚠ Über das Overlay selbst, NICHT über `import sc_bp_watcher`: In der
        # `.exe` heißt das Hauptprogramm `__main__`, ein Import lüde es ein
        # zweites Mal.
        control = overlay_module.OVERLAY_CONTROL[0]
        window = screen.OVERLAY[0]
        try:
            if control is not None and hasattr(control, 'reset_position'):
                control.reset_position()
            elif window is not None:
                width, height = 440, 1000
                _sx, _sy, sb, sh = screen.work_area(window, 0, 0)
                window.geometry(screen.centered(window, min(width, sb - 16),
                                                min(height, sh - 16)))
        except Exception as exc:
            errors.record('pages.reset_position', exc)
        fenster.say(t('s_an_lage_weg'))

    _button(fenster, ziel, t('s_zuruecksetzen'), reset_position).pack()


def _folders(fenster, rahmen):
    from . import paths
    _heading(fenster, rahmen, t('hf_ordner'),
                  t('s_ordner_lead'))
    innen = _scroll_area(rahmen)
    e = _settings_parts(fenster)

    gefunden = None
    try:
        gefunden = paths.game_folder()
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
        # ⚠ Nicht über `e._choose(...)`: Das übergibt `parent=self.root` —
        # eingebettet ist das ein Rahmen, der nie gepackt wird, und der Dialog
        # erschiene nicht.
        gewaehlt = choose_folder(t('e_spiel'), e.spiel.get())
        if gewaehlt:
            e.spiel.set(gewaehlt)
            e._save()
            fenster.say(t('e_neustart_noetig'))

    _path_field(fenster, innen, e.spiel, spiel_waehlen,
              oeffnen=lambda: fenster.say(
                  t('s_or_geoeffnet') if _show_folder(e.spiel.get())
                  else t('s_or_nicht_auf')))

    tk.Label(innen, text=t('s_eigene'), bg=BG, fg=FG, font=fenster.f_bold,
             anchor='w').pack(fill='x', pady=(20, 0))
    _body_text(innen, t('s_eigene_h'), fenster.f_small, fill='x')
    ablage = tk.StringVar(rahmen, value=paths.app_folder())

    def ablage_oeffnen():
        # Nur melden, was auch stimmt: „Ordner geöffnet" zu sagen, während gar
        # nichts aufgeht, ist schlimmer als eine ehrliche Fehlanzeige.
        fenster.say(t('s_or_geoeffnet') if _show_folder(paths.app_folder())
                      else t('s_or_nicht_auf'))

    def ablage_waehlen():
        # ⚠ Wirklich wählen lassen, nicht nur auf die Einstellungen verweisen —
        # dies IST die Seite mit der Einstellung, ein Hinweis sähe aus, als
        # täte der Knopf nichts.
        gewaehlt = choose_folder(t('s_eigene'), ablage.get())
        if not gewaehlt:
            return
        _move_storage(fenster, ablage, gewaehlt)

    _path_field(fenster, innen, ablage, ablage_waehlen, oeffnen=ablage_oeffnen)

    _start_command_field(fenster, innen)
    _channel_states(fenster, innen)


# Blase je Kanal: Befund -> (Textschlüssel, Farbe).
CHANNEL_STATES = {'ok': ('s_uc_ok', ACCENT),
                  'ohne_sprache': ('s_uc_ohne_sprache', GOLD),
                  'ohne_datei': ('s_uc_ohne_datei', GOLD),
                  'keine': ('s_uc_keine', SUB)}


def _channel_states(window, inner):
    """Spielkanäle mit Zustand der `user.cfg` (Vorbild SC Deutsch Launcher).

    ⚠ Die Blasen stehen untereinander, nicht in einer Reihe: Mit fünf
    Kanälen und englischen Texten liefe eine Reihe aus dem Fenster — genau
    die abgeschnittenen Kästen, die beim Vorbild stören."""
    from . import usercfg
    from .main_window import badge
    tk.Label(inner, text=t('s_uc_kanaele'), bg=BG, fg=FG, font=window.f_bold,
             anchor='w').pack(fill='x', pady=(20, 0))
    _body_text(inner, t('s_uc_kanaele_h'), window.f_small, fill='x')
    channels = usercfg.installed_channels()
    if not channels:
        _body_text(inner, t('s_uc_keine_kanaele'), window.f_small,
                   pady=(6, 0))
        return
    for name, folder, _installed in channels:
        key, color = CHANNEL_STATES[usercfg.status(folder)]
        row = tk.Frame(inner, bg=BG)
        row.pack(fill='x', pady=(6, 0))
        badge(row, '%s: %s' % (name, t(key)), color, window.f_small,
              bg=BG).pack(side='left')


def _move_storage(fenster, ablage, ziel):
    """Den Ablage-Ordner umstellen — **und die Daten mitnehmen**.

    ⚠⚠ **Nur die Einstellung umzustellen reicht nicht.** Bestand, Merkliste
    und Auftrags-Protokoll lägen dann noch im alten Ordner, und nach dem
    Neustart sähe das Programm leer aus — für den Nutzer sieht das nach
    Datenverlust aus.

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
    alt = paths.app_folder()
    if os.path.abspath(alt) == os.path.abspath(ziel):
        return

    schreibbar, fremde, grund = paths.storage_status(ziel)
    if not schreibbar:
        # ⚠ Genau hier landet eine nur lesend eingehängte Windows-Platte. Ohne
        # diese Prüfung stünde der neue Pfad in den Einstellungen, und beim
        # nächsten Start wäre der Ordner unbrauchbar.
        fenster.say(t('s_ab_nicht_schreibbar') % paths.redact(grund))
        return

    eigene = len(paths._storage_files(alt))

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

    kopiert, uebersprungen, misslungen = paths.move_storage(alt, ziel)
    if misslungen:
        # ⚠⚠ **Bei einem Fehler wird NICHT umgestellt.** Sonst zeigt die
        # Einstellung auf einen Ordner mit lückenhaftem Bestand, und der
        # vollständige liegt am alten Ort, den niemand mehr ansieht.
        fenster.say(t('s_ab_misslungen') % (misslungen, kopiert))
        return
    _set_storage(fenster, ablage, ziel)
    # ⭐ Den alten Ordner räumen — Datei für Datei nur, was am neuen
    # Ort nachweislich gleich liegt (siehe `paths.remove_old_storage`).
    _entfernt, behalten = paths.remove_old_storage(alt, ziel)
    if behalten:
        fenster.say(t('s_ab_fertig_rest') % (kopiert, len(behalten),
                                            paths.redact(alt)))
    else:
        fenster.say(t('s_ab_fertig') % kopiert)


def _set_storage(fenster, ablage, ziel):
    """Die Einstellung schreiben und das Feld nachziehen."""
    paths.set_setting('ablage_ordner', ziel)
    ablage.set(ziel)


def _overlay_corner(fenster, wahl, kennung):
    """Die Ecke merken und sofort anwenden.

    ⚠ Sofort, nicht erst beim naechsten Start: Wer eine Ecke waehlt, will
    sehen, ob sie die richtige ist — und im Pop-up-Betrieb kann er das Fenster
    danach nicht selbst hinschieben.
    """
    paths.set_setting('overlay_ecke', kennung)
    try:
        wahl.select(kennung)
    except Exception:
        pass
    # ⭐ Die Ecke nimmt die Leiste mit: Eine untere Ecke hängt die Leiste nach
    # unten, eine obere nach
    # oben — danach lässt sie sich weiter von Hand umstellen.
    if kennung.startswith(('oben', 'unten')):
        seite = 'unten' if kennung.startswith('unten') else 'oben'
        paths.set_setting('overlay_leiste', seite)
        leiste_wahl = getattr(fenster, '_overlay_bar_choice', None)
        if leiste_wahl is not None:
            try:
                leiste_wahl.select_quiet(seite)
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
    paths.set_setting('overlay_leiste', kennung)
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
    feld = round_entry(reihe, None, fenster.f_small, theme.FIELD, LINE, ACCENT,
                       FG, width=18)
    feld.insert(0, paths.setting('hotkey') or hk.DEFAULT)
    feld.holder.pack(side='left')

    def merken(_=None):
        wunsch = feld.get().strip()
        mods, taste = hk.parse(wunsch)
        if not mods:
            fenster.say(t('s_hk_falsch'))
            return
        paths.set_setting('hotkey', wunsch)
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
    from . import paths

    tk.Label(innen, text=t('s_or_start'), bg=BG, fg=FG, font=fenster.f_bold,
             anchor='w').pack(fill='x', pady=(20, 0))
    _body_text(innen, t('s_or_start_h'), fenster.f_small, fill='x')
    _body_text(innen, t('s_or_start_bsp'), fenster.f_small, color=SUB,
                fill='x', pady=(2, 0))

    wert = tk.StringVar(innen, value=paths.setting('spielstarter') or '')

    def uebernehmen():
        text = (wert.get() or '').strip()
        paths.set_setting('spielstarter', text)
        fenster.say(t('s_or_start_ok') if text else t('s_or_start_weg'))
        # Der Startknopf hängt daran — die Leiste muss ihn neu bewerten.
        try:
            fenster.rebuild()
        except Exception as ausnahme:
            errors.record('pages.startbefehl.aufbauen', ausnahme)

    reihe = tk.Frame(innen, bg=BG)
    reihe.pack(fill='x', pady=(8, 0))
    from .main_window import round_entry
    feld = round_entry(reihe, wert, fenster.f_small, theme.FIELD, LINE, ACCENT, FG)
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
    from . import overlay, paths
    neu_wert = not paths.setting_bool('durchklickbar', False)
    paths.set_setting('durchklickbar', neu_wert)
    geklappt = True
    wurzel = overlay.OVERLAY_WINDOW[0] if overlay.OVERLAY_WINDOW else None
    if wurzel is not None:
        try:
            geklappt = overlay.set_click_through(wurzel, neu_wert)
        except Exception as ausnahme:
            errors.record('pages.durchklick', ausnahme)
            geklappt = False
    if neu_wert and not geklappt:
        fenster.say(t('ov_durchklick_geht_nicht'))
        paths.set_setting('durchklickbar', False)
        return False
    fenster.say(t('s_ov_durch_sagen')
                  % (t('e_an') if neu_wert else t('e_aus')))
    return neu_wert


def _overlay_mode(fenster, wahl, kennung):
    """Zwischen „immer sichtbar" und „nur bei Neuzugang" umstellen."""
    from . import overlay, paths
    wahl.select(kennung)
    paths.set_setting('overlay_modus', kennung)
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
            errors.record('pages.overlay_modus', ausnahme)
    if kennung == 'popup':
        fenster.say(t('s_ov_popup_gleich'))
    else:
        fenster.say(t('s_ov_modus_sagen') % t('s_ov_immer'))


def clean_environment():
    """Weiterleitung — die Wahrheit steht in `file_picker`.

    ⚠ Beide brauchen dieselbe Wäsche, und zwei Versionen davon wären eine zu
    viel. Die Weiterleitung gibt es, weil `_show_folder` und der Spielstart
    sie hier aufrufen.
    """
    from . import paths as paths_module
    return paths_module.clean_environment()


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
        errors.record('pages.ordner_zeigen', ausnahme, pfad)
        return False


def _game(fenster, rahmen):
    """Auftragstexte — Textquelle wählen und die Bauplan-Angaben eintragen."""
    from . import paths
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
            lage = e.inj_state()
        except Exception as ausnahme:
            errors.record('pages.spiel.lage', ausnahme)
            return
        if not paths.setting_bool('inj_an', True):
            # ⚠ „Ausgeschaltet“ allein ist die halbe Wahrheit. Bleibt etwas in der
            # Datei stehen (Entfernen scheiterte, oder es wurde von Hand
            # abgeschaltet), sieht der Spieler seine Angaben weiter im Spiel —
            # und der Kasten behauptete, es sei nichts da.
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
                from . import translation as _tr
                zusatz.append(t('s_sp_quelle_ist')
                              % _tr.display_name(lage['quelle']))
            if lage['stand']:
                zusatz.append(str(lage['stand']))
            _status(fenster, kasten, 'haken', t('s_sp_steht'), ' · '.join(zusatz))
        else:
            _status(fenster, kasten, 'offen', t('s_sp_nichts'), t('s_sp_nichts_h'),
                    color=SUB)

    # Damit auch Aktionen im Einstellungsobjekt den Kasten auffrischen.
    e.lage_melder = lage_zeigen
    lage_zeigen()
    # Der Hintergrund sieht alle paar Stunden nach neuen Übersetzungen — beim
    # erneuten Öffnen soll „nachgesehen um …" den letzten Blick zeigen.
    fenster.on_show['spiel'] = lage_zeigen

    # --- Textquelle ----------------------------------------------------------
    # ⭐ Die Auswahl steht auf dem eigenen Reiter „Übersetzung" — elf Quellen
    # und eine Karte je Kanal passen nicht in eine Zeile dieser Seite. Hier
    # steht nur, was gewählt ist, und der Weg dorthin.
    from . import translation as _tr
    ziel = _setting_row(fenster, innen, t('s_sp_quelle'),
                        t('s_sp_quelle_jetzt') % (
                            _tr.display_name(paths.setting('inj_quelle'))
                            if paths.setting('inj_quelle') else '—'))
    _button(fenster, ziel, t('s_sp_zur_uebersetzung'),
            lambda: fenster.jump_to('uebersetzung')).pack()

    ziel = _setting_row(fenster, innen, t('s_sp_auto'), t('s_sp_auto_h'))

    def inj_auto_um():
        neu_wert = not paths.setting_bool('inj_auto', True)
        paths.set_setting('inj_auto', neu_wert)
        fenster.say(t('s_sp_auto_sagen')
                      % (t('e_an') if neu_wert else t('e_aus')))
        return neu_wert

    toggle_switch(ziel, paths.setting_bool('inj_auto', True),
                    inj_auto_um).pack()

    # --- An oder aus ---------------------------------------------------------
    # ⚠ Für alle, die auf PTU spielen oder die Textdatei in Ruhe lassen
    # wollen: „Wieder entfernen" allein hilft nicht, beim nächsten Start
    # schriebe das Werkzeug wieder hinein.
    ziel = _setting_row(fenster, innen, t('s_sp_an'), t('s_sp_an_h'))

    def inj_an_um():
        neu_wert = not paths.setting_bool('inj_an', True)
        paths.set_setting('inj_an', neu_wert)
        fenster.say(t('s_sp_an_sagen')
                      % (t('e_an') if neu_wert else t('e_aus')))
        # ⚠ **Aus heißt weg, an heißt da.** Setzte der Schalter nur die
        # Einstellung, sähe man seine Angaben weiter im Spiel und müsste erst
        # unten „Wieder entfernen“ finden.
        #
        # Gefahrlos, weil verlustfrei: Der Urtext ist gemerkt
        # (`injection.ORIGTEXT_FILE`), das Entfernen stellt den Wortlaut auf den
        # Buchstaben genau wieder her, und Einschalten trägt neu ein.
        try:
            from . import injection as inj_modul
            drin = bool(inj_modul.status().get('drin'))
            if neu_wert and not drin:
                e._inj_refresh()
            elif not neu_wert and drin:
                e._inj_remove()
        except Exception as ausnahme:
            errors.record('pages.inj_an_um', ausnahme)
        lage_zeigen()
        return neu_wert

    toggle_switch(ziel, paths.setting_bool('inj_an', True),
                    inj_an_um).pack()

    # --- Angaben am Gegenstand ----------------------------------------------
    # Klasse, Größe und Gütegrad direkt am Namen — bei Raketen der Suchkopf.
    # Abschaltbar, weil es die Gegenstandsnamen im Spiel verändert: Wer das
    # nicht will, soll die Bauplan-Angaben trotzdem behalten können.
    ziel = _setting_row(fenster, innen, t('s_sp_angaben'), t('s_sp_angaben_h'))

    def angaben_um():
        from . import injection as inj_modul
        neu_wert = not paths.setting_bool(inj_modul.SETTING_DETAILS,
                                                  True)
        paths.set_setting(inj_modul.SETTING_DETAILS, neu_wert)
        fenster.say(t('s_sp_angaben_sagen')
                      % (t('e_an') if neu_wert else t('e_aus')))
        # ⚠ **Umlegen muss sofort wirken.** Setzte dieser Schalter nur die
        # Einstellung, bliebe die `global.ini` unangetastet, bis jemand unten
        # auf „Jetzt eintragen“ drückt — und wer die Angaben abschaltet und sie
        # nach dem Neustart weiter sieht, hält das Werkzeug für kaputt. Der
        # Kasten darüber sagt „Änderungen wirken beim nächsten Spielstart“.
        #
        # ⚠ Nur wenn wirklich etwas drinsteht und das Schreiben überhaupt
        # eingeschaltet ist. Sonst würde ein Formatschalter ungefragt eine
        # Einfügung anstoßen, die der Nutzer gar nicht wollte — der obere
        # Schalter lässt Vorhandenes mit Absicht stehen (PTU-Fall).
        try:
            if (paths.setting_bool('inj_an', True)
                    and inj_modul.status().get('drin')):
                e._inj_refresh()
                lage_zeigen()
        except Exception as ausnahme:
            errors.record('pages.angaben_um', ausnahme)
        return neu_wert

    from . import injection as _inj
    toggle_switch(ziel,
                    paths.setting_bool(_inj.SETTING_DETAILS, True),
                    angaben_um).pack()

    # --- Ruf-Stufen an den Rangnamen -------------------------------------------
    # „Gildenmitglied [ab 10.000]" im Reputationsmenü. Gleich gebaut wie der
    # Schalter darüber: Umlegen schreibt sofort neu, wenn etwas drinsteht.
    row = _setting_row(fenster, innen, t('s_sp_rang'), t('s_sp_rang_h'))

    def toggle_ranks():
        from . import injection as injection_module, rank_thresholds
        new_value = not paths.setting_bool(rank_thresholds.SETTING, True)
        paths.set_setting(rank_thresholds.SETTING, new_value)
        fenster.say(t('s_sp_rang_sagen')
                    % (t('e_an') if new_value else t('e_aus')))
        try:
            if (paths.setting_bool('inj_an', True)
                    and injection_module.status().get('drin')):
                e._inj_refresh()
                lage_zeigen()
        except Exception as exc:
            errors.record('pages.toggle_ranks', exc)
        return new_value

    from . import rank_thresholds as _ranks
    toggle_switch(row, paths.setting_bool(_ranks.SETTING, True),
                  toggle_ranks).pack()

    ziel = _setting_row(fenster, innen, t('s_sp_hand'), t('s_sp_hand_h'), wide=True)
    reihe = tk.Frame(ziel, bg=BG)
    reihe.pack()
    _button(fenster, reihe, t('s_sp_jetzt'),
           lambda: (e._inj_refresh(), lage_zeigen()),
           strong=True).pack(side='left')
    _button(fenster, reihe, t('s_sp_pruefen'),
           lambda: (e._inj_check(), lage_zeigen())).pack(side='left', padx=8)
    _button(fenster, reihe, t('s_sp_weg'),
           lambda: (e._inj_remove(), lage_zeigen()),
           danger=True).pack(side='left')

    _status(fenster, innen, '!', t('s_sp_warn'), t('s_sp_warn_h'), color=GOLD)
    _user_cfg_section(fenster, innen)


def _user_cfg_section(window, inner):
    """Die `user.cfg` je Kanal: eigene Zeilen bearbeiten, ganze Datei sehen.

    ⭐ **Untereinander statt nebeneinander**, und die Kästen wachsen mit der Zeilenzahl bis zu einer
    Grenze — erst darüber rollen sie."""
    from . import usercfg
    channels = [c for c in usercfg.installed_channels() if c[2]]
    tk.Label(inner, text=t('s_uc_titel'), bg=BG, fg=FG, font=window.f_title,
             anchor='w').pack(fill='x', pady=(24, 2))
    _body_text(inner, t('s_uc_lead'), window.f_small, fill='x')
    if not channels:
        _body_text(inner, t('s_uc_keine_kanaele'), window.f_small,
                   pady=(6, 0))
        return
    state = {'folder': channels[0][1]}
    area = tk.Frame(inner, bg=BG)

    def text_box(parent, editable):
        box = tk.Frame(parent, bg=LINE, padx=1, pady=1)
        box.pack(fill='x', pady=(4, 0))
        field = tk.Text(box, bg=theme.FIELD, fg=FG if editable else SUB,
                        insertbackground=FG, relief='flat', wrap='none',
                        font=('Consolas', 10) if sys.platform.startswith('win')
                        else ('DejaVu Sans Mono', 10), height=6,
                        padx=8, pady=6, highlightthickness=0)
        # ⚠ Der Rollbalken des Programms, nicht `tk.Scrollbar`. Er blendet sich selbst
        # aus, wenn alles hineinpasst — Prüfung 272 verbietet `tk.Scrollbar`.
        from .main_window import round_scrollbar
        roll = round_scrollbar(box, field, bg=theme.FIELD)
        roll.pack(side='right', fill='y')
        field.configure(yscrollcommand=roll.set)
        field.pack(side='left', fill='both', expand=True)
        return field, roll

    def fill(field, roll, text):
        field.configure(state='normal')
        field.delete('1.0', 'end')
        field.insert('1.0', text)
        lines = max(1, text.count('\n') + 1)
        # Mitwachsen bis 18 Zeilen, erst darüber rollen — der Rollbalken
        # erscheint dann von selbst.
        field.configure(height=max(4, min(18, lines)))

    def show():
        for child in area.winfo_children():
            child.destroy()
        folder = state['folder']
        text = usercfg.read(folder)
        tk.Label(area, text=t('s_uc_eigene'), bg=BG, fg=FG,
                 font=window.f_bold, anchor='w').pack(fill='x', pady=(12, 0))
        _body_text(area, t('s_uc_eigene_h'), window.f_small, fill='x')
        own, own_roll = text_box(area, True)
        fill(own, own_roll, '\n'.join(usercfg.own_lines(text)))
        buttons = tk.Frame(area, bg=BG)
        buttons.pack(fill='x', pady=(6, 0))

        def save():
            lines = own.get('1.0', 'end').splitlines()
            ok, bad = usercfg.write_own(folder, lines)
            if bad:
                window.say(t('s_uc_fehler') % ', '.join(
                    str(number) for number, _line in bad[:5]))
                return
            window.say(t('s_uc_gespeichert') if ok else t('s_uc_nicht'))
            show()

        _button(window, buttons, t('s_uc_speichern'), save,
                strong=True).pack(side='left')
        _button(window, buttons, t('s_uc_neu'), show).pack(
            side='left', padx=(8, 0))
        tk.Label(area, text=t('s_uc_ganz'), bg=BG, fg=FG, font=window.f_bold,
                 anchor='w').pack(fill='x', pady=(16, 0))
        _body_text(area, t('s_uc_ganz_h') % os.path.join(folder, usercfg.FILE),
                   window.f_small, fill='x')
        whole, whole_roll = text_box(area, False)
        fill(whole, whole_roll,
             text if text is not None else t('s_uc_keine_datei'))
        whole.configure(state='disabled')

    if len(channels) > 1:
        target = _setting_row(window, inner, t('s_uc_kanal'), '', wide=True)
        choice = _choice(window, target,
                         [(folder, name) for name, folder, _i in channels],
                         state['folder'],
                         lambda k: (choice.select(k),
                                    state.update(folder=k), show()))
        choice.pack()
    area.pack(fill='x')
    show()


def _choose_source(fenster, e, wahl, kennung, danach):
    """Eine Textquelle einrichten — das dauert, also erst ansagen.

    ⚠ Ohne Ansage sieht es aus, als sei nichts passiert: Das Herunterladen und
    Einsetzen braucht mehrere Sekunden.
    """
    from . import paths
    wahl.select(kennung)
    # ⚠ Die Wahl wird **vor** dem Einrichten gemerkt. Sie stand vorher dahinter,
    # und wenn das Herunterladen schiefging (kein Netz, Zertifikat, Server weg),
    # blieb die alte Quelle eingetragen — das Feld zeigte die neue, der Rest des
    # Programms rechnete mit der alten. Erst gilt, was gewählt wurde; ob es auch
    # eingerichtet werden konnte, sagt der Kasten darüber.
    paths.set_setting('inj_quelle', kennung)
    from . import translation
    fenster.say(t('s_sp_hole') % translation.display_name(kennung))
    try:
        e._inj_switch(kennung)
    except Exception as ausnahme:
        errors.record('pages.spiel.quelle', ausnahme)
        fenster.say(t('inj_fehler', ausnahme))
    danach()


def _choice_rows(window, parent, entries, active, action, per_row=3):
    """Viele Möglichkeiten als `_choice`, in Reihen zu `per_row` — elf
    Textquellen nebeneinander passten in kein Fenster. Die Reihen teilen sich
    eine Auswahl: `select(k)` hebt genau einen Knopf über alle Reihen hervor.

    ⭐ **Ein `None` in der Liste bricht die Reihe um.** Damit
    lassen sich Gruppen bilden, die zusammengehören — bei den Textquellen
    stehen so alle deutschen in der ersten Reihe, die englischen in der
    zweiten und die übrigen Sprachen danach.

    Eine Gruppe mit mehr als `per_row` Einträgen wird innerhalb weiter
    umgebrochen — die Gruppe bleibt trotzdem zusammen.
    """
    holder = tk.Frame(parent, bg=BG)
    rows = []
    gruppen = [[]]
    for eintrag in entries:
        if eintrag is None:
            if gruppen[-1]:
                gruppen.append([])
            continue
        gruppen[-1].append(eintrag)
    for gruppe in gruppen:
        for start in range(0, len(gruppe), per_row):
            row = _choice(window, holder, gruppe[start:start + per_row],
                          active, action)
            row.pack(anchor='w', pady=(0, 6))
            rows.append(row)

    def select(key):
        for row in rows:
            row.select(key)

    holder.select = select
    holder.select_quiet = select
    return holder


def _translation_page(window, frame):
    """Übersetzung — Textquelle je Kanal, mehrere Sprachen, eigene Adresse.

    Eine Karte
    je installiertem Kanal: Die Hauptinstallation (der Spielordner aus den
    Einstellungen) bekommt die Textdatei **samt Bauplan-Angaben**, jeder
    weitere Kanal (PTU …) nur die Textdatei — der Bauplan-Bestand gilt dort
    nicht (`paths.SHARED_STOCK_CHANNELS`)."""
    from . import paths, translation, usercfg
    _heading(window, frame, t('hf_uebersetzung'), t('s_tq_lead'))
    inner = _scroll_area(frame)
    parts = _settings_parts(window)
    area = tk.Frame(inner, bg=BG)
    area.pack(fill='x')

    def say(text):
        window.say(text)
        try:
            window.root.update_idletasks()
        except tk.TclError:
            pass

    def entries_for(channel):
        """Welche Quellen es für den Kanal gibt — mit Flagge, nach Sprache
        gruppiert.

        ⭐ **Erst alle deutschen, dann alle englischen, dann die übrigen.**
        Ein `None` trennt die Gruppen (`_choice_rows`), sonst bräche die
        Liste stur alle drei Knöpfe um und mischte die Sprachen.

        ⚠ Die Reihenfolge **innerhalb** einer Gruppe bleibt die aus
        `SOURCES` — dort steht die gepflegteste Quelle je Sprache vorn.
        """
        # ⭐ Die Reihenfolge kommt aus `translation.grouped_sources()` — der
        # **einen** Stelle, die sie festlegt. Der Einrichtungsassistent liest
        # dieselbe Funktion; so kann keine Quelle mehr nur an einer der beiden
        # Stellen auftauchen (siehe den Kasten dort).
        result = []
        for gruppe in translation.grouped_sources(channel):
            for key in gruppe:
                flagge = (translation.SOURCES.get(key) or {}).get('flagge')
                if key == 'original':
                    flagge = 'gb'
                result.append((key, translation.display_name(key),
                               _flag(flagge) if flagge else None))
            result.append(None)              # Reihenumbruch je Sprache

        result.append((translation.CUSTOM, t('s_sp_q_eigen'), None))
        # ⚠⚠ **„Nicht anfassen" gibt es für JEDEN Kanal, auch die
        # Hauptinstallation** — der sichtbare Weg zum Zustand „noch keine
        # Textquelle gewählt" (`s_tq_nichts_gewaehlt`).
        #
        # ⚠ An der Hauptinstallation hat die Wahl eine Folge, die man kennen
        # muss: **Ohne Textdatei keine Bauplan-Angaben im Spiel.** Deshalb
        # fragt `act()` dort nach, statt es still zu tun.
        result.append(('', t('s_tq_nichts'), None))
        return result

    def fetch_side(channel, folder, source):
        """Einen Nebenkanal (PTU …) auf eine Quelle stellen und holen."""
        translation.set_channel_source(channel, source)
        if not source:
            say(t('s_tq_unberuehrt') % channel)
            return
        if source != 'original' and not parts._source_confirmed(source):
            return
        say(t('s_sp_hole') % translation.display_name(source))
        try:
            if source == 'original':
                from . import gametext
                ok, message = gametext.fetch('english', spielordner=folder,
                                             fortschritt=say)
                if ok:
                    translation._note_set(translation._key('original', channel),
                                          'Data.p4k', 'english')
            else:
                ok, message = translation.fetch(source, progress=say,
                                                game_dir=folder,
                                                channel=channel)
        except Exception as exc:
            errors.record('pages.uebersetzung.kanal', exc)
            ok, message = False, str(exc)
        say(t('s_tq_fertig') % (channel, message) if ok
            else t('inj_fehler', message))

    def card(channel_name, folder, main):
        box = tk.Frame(area, bg=BG)
        box.pack(fill='x', pady=(0, 18))
        tk.Label(box, text=channel_name, bg=BG, fg=FG, font=window.f_title,
                 anchor='w').pack(fill='x')
        _body_text(box, t('s_tq_haupt') if main else t('s_tq_neben'),
                   window.f_small, fill='x', pady=(0, 8))
        channel = None if main else channel_name
        if main:
            chosen = paths.setting('inj_quelle') or ''
        else:
            chosen = translation.channel_sources().get(channel_name, '')
        custom_box = tk.Frame(box, bg=BG)

        def act(key):
            choice.select(key)
            if key == translation.CUSTOM:
                # Erst die Adresse — geholt wird mit „Übernehmen".
                show_custom(True)
                return
            show_custom(False)
            if main and not key:
                # ⚠⚠ **Ohne Textdatei keine Bauplan-Angaben.** Der Watcher
                # schreibt sie in die `global.ini`; gibt es keine, gibt es
                # nichts zu beschriften. Das muss vorher klar sein — sonst
                # sucht jemand den Fehler später im Spiel.
                from .main_window import ask_yes_no
                if not ask_yes_no(window.root, t('s_tq_nichts'),
                                  t('s_tq_nichts_warnung')):
                    choice.select(paths.setting('inj_quelle') or '')
                    return
                paths.set_setting('inj_quelle', '')
                say(t('s_tq_nichts_ok'))
            elif main:
                _choose_source(window, parts, choice, key, lambda: None)
            else:
                fetch_side(channel_name, folder, key)
            status()

        choice = _choice_rows(window, box, entries_for(channel), chosen, act)
        choice.pack(fill='x')
        state_label = tk.Label(box, text='', bg=BG, fg=SUB,
                               font=window.f_small, anchor='w',
                               justify='left')
        _wrap_self(state_label)

        def status():
            current = (paths.setting('inj_quelle') if main
                       else translation.channel_sources().get(channel_name))
            text = ''
            if current:
                text = translation.status_text(current, channel=channel)
            state_label.configure(
                text=('%s · %s' % (translation.display_name(current), text)
                      if current and text
                      else translation.display_name(current) if current
                      else t('s_tq_nichts_gewaehlt') if main
                      else t('s_tq_unberuehrt') % channel_name))

        # --- eigene Adresse ---------------------------------------------
        from .main_window import round_entry
        custom = translation._custom_settings(channel)
        _body_text(custom_box, t('s_tq_url_h'), window.f_small, fill='x',
                   pady=(4, 4))
        # ⚠ Mit eigener Variable: Ein Feld mit grauem Hinweis liest sonst den
        # Hinweis als Eingabe (`round_entry` verweigert das deshalb).
        url_value = tk.StringVar(custom_box, value=custom.get('url') or '')
        url_field = round_entry(custom_box, url_value, window.f_small,
                                theme.FIELD, LINE, ACCENT, FG,
                                placeholder=t('s_tq_url_platz'))
        url_field.holder.pack(fill='x')
        _body_text(custom_box, t('s_tq_sprache_h'), window.f_small, fill='x',
                   pady=(8, 4))
        language_state = {'value': custom.get('sprache') or 'english'}
        language_choice = _choice_rows(
            window, custom_box,
            [(folder_name, folder_name) for folder_name in translation.GAME_LANGUAGES],
            language_state['value'],
            lambda k: (language_choice.select(k),
                       language_state.update(value=k)), per_row=4)
        language_choice.pack(fill='x')
        buttons = tk.Frame(custom_box, bg=BG)
        buttons.pack(fill='x', pady=(6, 0))

        def take_over():
            url = translation.set_custom(url_value.get(),
                                         language_state['value'], channel)
            if not url:
                say(t('s_tq_url_falsch'))
                return
            if main:
                _choose_source(window, parts, choice, translation.CUSTOM,
                               lambda: None)
            else:
                fetch_side(channel_name, folder, translation.CUSTOM)
            status()

        _button(window, buttons, t('s_tq_uebernehmen'), take_over,
                strong=True).pack(side='left')

        def show_custom(visible):
            if visible:
                custom_box.pack(fill='x', pady=(4, 0), before=state_label)
            else:
                custom_box.pack_forget()

        state_label.pack(fill='x', pady=(6, 0))
        show_custom(chosen == translation.CUSTOM)
        row = tk.Frame(box, bg=BG)
        row.pack(fill='x', pady=(8, 0))

        def check():
            current = (paths.setting('inj_quelle') if main
                       else translation.channel_sources().get(channel_name))
            if not current or current == 'original':
                say(t('s_tq_nichts_zu_pruefen'))
                return
            say(t('inj_laeuft'))
            newer, ident = translation.update_available(current, channel)
            if newer:
                if main:
                    _choose_source(window, parts, choice, current, lambda: None)
                else:
                    fetch_side(channel_name, folder, current)
            else:
                say(t('inj_aktuell'))
            status()

        _button(window, row, t('s_tq_pruefen'), check).pack(side='left')
        status()

    main_folder = os.path.normcase(os.path.normpath(paths.game_folder() or ''))
    channels = [(name, folder) for name, folder, present
                in usercfg.installed_channels()
                if present and name in translation.CHANNELS]
    main_name = next((name for name, folder in channels
                      if os.path.normcase(os.path.normpath(folder))
                      == main_folder), None)
    if not channels and not main_folder:
        _body_text(area, t('s_tq_kein_spiel'), window.f_small, fill='x')
        return
    card(main_name or os.path.basename(main_folder) or 'LIVE',
         paths.game_folder(), True)
    for name, folder in channels:
        if name != main_name:
            card(name, folder, False)
    _status(window, inner, '!', t('s_tq_warn'), t('s_tq_warn_h'), color=GOLD)


def _backup_section(window, inner):
    """Sicherung und Einrichtung — oben auf „Sichern & Zurücksetzen".

    Dieselben Wege wie der Knopf „Sicherung" in der Titelleiste und
    „Einrichtung starten" — hier stehen sie dort, wo man sie beim
    Rechnerwechsel sucht."""
    from . import file_picker, backup
    tk.Label(inner, text=t('s_si_sicherung'), bg=BG, fg=FG,
             font=window.f_title, anchor='w').pack(fill='x', pady=(0, 2))
    _body_text(inner, t('sich_lead'), window.f_small, fill='x',
               pady=(0, 8))
    row = tk.Frame(inner, bg=BG)
    row.pack(fill='x')
    _button(window, row, t('sich_schreiben'),
            lambda: window._backup_write(file_picker, backup),
            strong=True).pack(side='left')
    _button(window, row, t('sich_lesen'),
            lambda: window._backup_read(file_picker, backup)).pack(
                side='left', padx=(8, 0))
    target = _setting_row(window, inner, t('s_si_einrichtung'),
                          t('s_si_einrichtung_h'))
    _button(window, target, t('hf_einrichtung'), window._open_wizard).pack()
    tk.Frame(inner, bg=LINE, height=1).pack(fill='x', pady=(20, 16))


def show_export(window):
    """Nach „Sichern & Zurücksetzen" springen, „Bestand ausgeben" oben.

    Der Weg aus der Bauplan-Liste: Hier stehen alle Formate mit Namen,
    statt dass ein einzelner Knopf stillschweigend eines wählt. Die Seite wird beim ersten Mal erst gebaut; gerollt wird deshalb
    erst, wenn Tk die Höhen kennt."""
    window.jump_to('bestand')
    anchor = getattr(window, 'export_anchor', None)
    if anchor is None:
        return
    canvas = None
    step = anchor
    while step is not None and canvas is None:
        canvas = getattr(step, 'canvas', None)
        step = getattr(step, 'master', None)
    if canvas is None:
        return

    def scroll():
        try:
            if not anchor.winfo_exists():
                return
            canvas.update_idletasks()
            region = canvas.bbox('all')
            total = (region[3] - region[1]) if region else 0
            if total <= 0:
                return
            top = anchor.winfo_rooty() - canvas.winfo_rooty() + canvas.canvasy(0)
            canvas.yview_moveto(max(0.0, (top - 8) / total))
        except Exception as exception:
            errors.record('pages.show_export', exception)

    canvas.after_idle(scroll)


def _collection(fenster, rahmen):
    """Sichern & Zurücksetzen.

    ⚠ Die Kennung ist `bestand` — sie steckt in „Neu"-Marken und
    Sprüngen. Das Zurücksetzen steht ganz unten, rot und mit Rückfrage,
    damit es nicht im Vorbeigehen angeklickt wird."""
    from . import export, importer
    _heading(fenster, rahmen, t('hf_sichern'), t('s_si_lead'))
    innen = _scroll_area(rahmen)
    _backup_section(fenster, innen)

    anzahl = _count_collection()
    # ⚠ Die Überschrift ist das Sprungziel von „Datei speichern …" in der
    # Bauplan-Liste (`show_export`).
    fenster.export_anchor = tk.Label(innen, text=t('s_be_aus'), bg=BG, fg=FG,
                                     font=fenster.f_title, anchor='w')
    fenster.export_anchor.pack(fill='x', pady=(0, 2))
    _body_text(innen, t('s_be_aus_h'), fenster.f_small,
                fill='x', pady=(0, 12))

    # ⚠ Ein Speichern-Knopf **je Version**, direkt an der Version — so ist
    # jedes Format einzeln erreichbar.
    karte = _card(innen)
    for art, name, wofuer in (('basetool', 'KRT Profit Basetool',
                               t('s_be_n_bp') % anzahl),
                              ('scmdb', 'scmdb.net', t('s_be_n_bp') % anzahl),
                              ('voll', t('s_be_voll'), t('s_be_voll_h'))):
        z = tk.Frame(karte, bg=SURFACE)
        z.pack(fill='x', padx=16, pady=5)
        # ⚠ `a=art` als Vorgabewert, nicht `art` direkt. Ein Lambda merkt sich
        # die **Variable**, nicht ihren Wert — ohne diese Zeile hätten alle drei
        # Knöpfe am Ende der Schleife auf „voll" gezeigt und dreimal dasselbe
        # gespeichert.
        # ⚠ Der Knopf zuerst: `pack` quetscht, was zuletzt kommt — bei der
        # größten Schrift fehlten ihm sonst 15 px (tools/randpruefung.py).
        _button(fenster, z, t('s_be_speichern_kurz'),
               lambda a=art: einzeln(a)).pack(side='right')
        # ⚠ Die Breite trägt den LÄNGSTEN Namen — auch einen übersetzten.
        # Ein Label mit zu kleiner `width` wächst über sie hinaus
        # und schiebt die Spalte daneben nach rechts: Dann steht „413
        # Baupläne" in jeder Zeile woanders.
        tk.Label(z, text=name, bg=SURFACE, fg=FG, font=fenster.f_small,
                 width=34, anchor='w').pack(side='left')
        _wofuer_lbl = tk.Label(z, text=wofuer, bg=SURFACE, fg=SUB,
                               font=fenster.f_small, anchor='w',
                               justify='left')
        _wofuer_lbl.pack(side='left', fill='x', expand=True)
        _wrap_self(_wofuer_lbl)

    reihe = tk.Frame(innen, bg=BG)
    reihe.pack(fill='x', pady=(12, 0))

    def in_ablage():
        try:
            ergebnis = export.archive()
            wieviele = ergebnis[1] if isinstance(ergebnis, tuple) else ergebnis
            fenster.say(t('s_be_geschrieben') % wieviele)
            _show_folder(export.archive_folder())
        except Exception as ausnahme:
            errors.record('pages.bestand.ablegen', ausnahme)
            fenster.say(t('s_be_schiefging'))

    def einzeln(art):
        """Eine einzelne Version speichern — die, an deren Zeile der Knopf steht.

        ⚠ `art` kommt vom Knopf der Zeile — nie fest verdrahtet, sonst
        bekäme man wortlos immer dasselbe Format.
        """
        from . import file_picker
        ziel = file_picker.save_file(
            t('s_be_speichern'), suggestion=export.suggestion(art),
            extension='.json', start=export.archive_folder())
        if not ziel:
            return
        try:
            export.write(ziel, kind=art)
            fenster.say(t('s_be_gespeichert') % os.path.basename(ziel))
        except Exception as ausnahme:
            errors.record('pages.bestand.einzeln', ausnahme)

    _button(fenster, reihe, t('s_be_alle'), in_ablage,
           strong=True).pack(side='left')
    _button(fenster, reihe, t('s_be_ablage'),
           lambda: _show_folder(export.archive_folder())).pack(side='left',
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
        # ohne Fehler: Die Baupläne DB kann die **vorgemerkten**
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

    # ⚠⚠ **Nicht rot — der Knopf kann nichts kaputt machen.** Er stösst zwar
    # einen Lauf über hunderte Protokolle an, tut aber nur eines —
    # `bestand.hinzufuegen`, und das **legt an**. Es nimmt nichts weg, überschreibt nichts, und
    # doppelt kann nichts werden. Der schlimmste Fall ist „dauert kurz".
    #
    # ⚠⚠ **Zwei Bedeutungen für dieselbe Farbe heissen: die Farbe warnt nicht
    # mehr.** Direkt darunter steht „Bestand zurücksetzen" — das loescht
    # wirklich. Wären beide rot, sagte Rot nur noch „irgendwas Wichtiges".
    #
    # Rot bleibt fuer das, was weg ist, wenn man es drueckt.
    _button(fenster, ziel, t('s_be_neu'), neu_einlesen).pack()

    # ⚠ **Bestand zurücksetzen — hier und nicht unter „Fehler melden".** Wer
    # seinen Bauplan-Stand neu aufbauen will, geht auf die Seite, die seinen
    # Bauplan-Stand verwaltet.
    #
    # Der Platz direkt unter „Protokolle erneut einlesen" ist Absicht — die
    # beiden gehören zusammen und der Unterschied wird erst nebeneinander
    # sichtbar: Einlesen **ergänzt**, was fehlt. Zurücksetzen **wirft weg** und
    # baut neu auf. Wer das falsche nimmt, verliert seinen Stand; getrennt auf
    # zwei Seiten sieht man diesen Unterschied nie.
    ziel = _setting_row(fenster, innen, t('s_be_reset'), t('s_be_reset_h'))

    def zuruecksetzen():
        from .main_window import ask_yes_no

        # ⚠⚠ **Die Zahlen NENNEN, nicht nur warnen.** Ein allgemeiner Satz
        # sagt nicht, wie wenig beim Neuaufbau zurückkommt. Wer „232 → 3"
        # liest, bricht ab; wer nur einen Satz liest, klickt weiter.
        #
        # ⚠ Gerechnet wird aus dem Bestand selbst: Was aus `log`, `nachlese`
        # oder `start` stammt, kommt beim Neuaufbau zurück — alles andere
        # (Launcher, Import, von Hand) nicht. Siehe `collection.restorable`. Kein Durchlauf über 221
        # Protokolle nötig, die Auskunft liegt schon da.
        frage = t('s_be_reset_frage')
        try:
            daten = bestand_datei.load()
            gesamt = len(daten.get('bauplaene') or {})
            bleibt = bestand_datei.restorable(daten)
            if gesamt:
                frage = '%s\n\n%s' % (
                    t('s_be_reset_zahlen') % (gesamt, bleibt, gesamt - bleibt),
                    frage)
        except Exception as ausnahme:
            errors.record('pages.bestand.reset_zahlen', ausnahme)

        if not ask_yes_no(fenster.root, t('s_be_reset'), frage):
            return
        # ⚠⚠ **Jeder Ausgang sagt etwas.** Ein Knopf, der nach der
        # Warnfrage schweigt, ist von einem kaputten nicht zu unterscheiden.
        # Was „geschafft" heisst, entscheidet `collection.reset()` — dort
        # steht auch, warum „war schon weg" dazugehoert.
        stoerung = bestand_datei.reset()
        if stoerung is not None:
            errors.record('pages.bestand.zuruecksetzen', stoerung)
            fenster.say(t('s_be_reset_fehler', stoerung))
            return
        fenster.say(t('s_be_reset_ok'))

    _button(fenster, ziel, t('s_zuruecksetzen'), zuruecksetzen, danger=True).pack()

    _status(fenster, innen, '!', t('s_be_reset_warn'), t('s_be_reset_warn_h'),
            color=GOLD)

    _shader_cache_section(fenster, innen)


def _shader_cache_section(window, parent):
    """Shader-Zwischenspeicher von Star Citizen und den Grafiktreibern leeren —
    nur, solange das Spiel nicht läuft. Suchen und Löschen laufen im
    Hintergrund, beides kann bei vielen tausend Dateien Sekunden dauern."""
    from . import shader_cache
    from .language import current

    tk.Label(parent, text=t('s_sc_titel'), bg=BG, fg=FG,
             font=window.f_title, anchor='w').pack(fill='x', pady=(28, 2))
    _body_text(parent, t('s_sc_lead'), window.f_small, fill='x', pady=(0, 12))
    slot = _setting_row(window, parent, t('s_sc_knopf'), t('s_sc_knopf_h'))
    busy = [False]

    def size_text(size):
        return shader_cache.format_size(size, ',' if current() == 'de' else '.')

    def game_blocks():
        from . import auto_update
        from .main_window import show_result
        if auto_update.game_running():
            show_result(window.root, t('s_sc_titel'), t('s_sc_spiel'))
            return True
        return False

    def finished(freed, skipped):
        from .main_window import show_result
        busy[0] = False
        text = t('s_sc_fertig') % size_text(freed)
        if skipped:
            text += '\n\n' + t('s_sc_rest') % skipped
        show_result(window.root, t('s_sc_titel'), text)

    def confirm(targets):
        from .main_window import ask_yes_no
        totals = shader_cache.total_by_kind(targets)
        if not targets or not sum(totals.values()):
            busy[0] = False
            window.say(t('s_sc_leer'))
            return
        lines = []
        for kind, key in ((shader_cache.KIND_SC, 's_sc_art_sc'),
                          (shader_cache.KIND_GPU, 's_sc_art_gpu'),
                          (shader_cache.KIND_CRASH, 's_sc_art_crash')):
            if totals.get(kind):
                lines.append(t(key) % size_text(totals[kind]))
        question = '%s\n\n%s\n\n%s' % (t('s_sc_frage_kopf'), '\n'.join(lines),
                                       t('s_sc_frage'))
        if not ask_yes_no(window.root, t('s_sc_titel'), question) or game_blocks():
            busy[0] = False
            return
        window.say(t('s_sc_loescht'))

        def work():
            freed, skipped = 0, 0
            try:
                freed, skipped = shader_cache.clear(targets)
            except Exception as error:
                errors.record('pages.shader_cache.clear', error)
            _from_thread(slot, lambda: finished(freed, skipped))

        threading.Thread(target=work, daemon=True).start()

    def start():
        if busy[0] or game_blocks():
            return
        busy[0] = True
        window.say(t('s_sc_sucht'))

        def work():
            targets = []
            try:
                targets = shader_cache.find()
            except Exception as error:
                errors.record('pages.shader_cache.find', error)
            _from_thread(slot, lambda: confirm(targets))

        threading.Thread(target=work, daemon=True).start()

    _button(window, slot, t('s_sc_knopf'), start, danger=True).pack()


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
    from . import mission_log

    _heading(fenster, rahmen, t('hf_auftragslog'), t('s_al_lead'))
    innen = _scroll_area(rahmen)
    _body_text(innen, t('s_al_hinweis'), fenster.f_small, fill='x')
    from . import issue_council
    _source_link(fenster, innen, t('s_al_spectrum'),
                 issue_council.SPECTRUM_URL)
    # Der Käfer an jeder Zeile braucht die englischen Titel — schon jetzt im
    # Hintergrund laden, damit der erste Klick nicht wartet.
    issue_council.warm_up()

    # ⚠⚠ **Die Daten werden bei JEDEM Zeigen neu geholt, nicht nur beim Bauen.**
    # Die Nachlese der alten Protokolle läuft kurz nach dem Start in einem
    # eigenen Faden. Wer die Seite in dieser Sekunde öffnet, sähe sonst
    # „Noch kein Auftrag aufgezeichnet" — und danach nie wieder etwas
    # anderes, weil die Seite gebaut bleibt.
    daten = {'alle': []}
    stand = {'art': 'alle'}
    # ⭐ **Wie viele Zeilen zuerst.** Die Liste wird begrenzt und unten
    # nachgeladen, statt alles im Voraus zu bauen — diese Seite ist mit allen
    # Zeilen die mit Abstand teuerste im Programm (siehe `LOG_ROWS_FIRST`).
    #
    # ⚠ Kein hartes Abschneiden **ohne Hinweis**: Wer 392 Aufträge gespielt
    # hat, muss erfahren, dass nicht alle gezeigt werden. Weggelassen heißt
    # nicht verschwiegen.
    gezeigt = {'anzahl': LOG_ROWS_FIRST}
    # Der Fingerabdruck der zuletzt gezeichneten Liste — siehe `zeichnen()`.
    zuletzt = {'stand': None}

    suche = tk.StringVar(rahmen)
    liste_rahmen = tk.Frame(innen, bg=BG)

    block = tk.Frame(innen, bg=BG)
    block.pack(fill='x', padx=24, pady=(14, 0))
    tk.Label(block, text=t('s_al_suche'), bg=BG, fg=FG, font=fenster.f_bold,
             anchor='w').pack(fill='x')
    from .main_window import round_entry
    feld = round_entry(block, suche, fenster.f_small, theme.FIELD, LINE,
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

    # ⚠ **Drei Aussagen, drei Farben:** Gruen fuer die Leistung,
    # blasses Rot fuer das, was schiefging, Grau fuer das, worueber wir nichts
    # behaupten. In Grau waeren abgebrochen und fehlgeschlagen von „nicht
    # mehr offen" nicht zu unterscheiden.
    #
    # ⚠ Blass, nicht `ROT`: Das kraeftige Rot gehoert den echten Fehlern
    # („Fehler melden"). Ein aufgegebener Auftrag ist eine Notiz, keine
    # Stoerung — er soll auffallen, ohne wie ein Alarm auszusehen.
    farben = {mission_log.COMPLETED: ACCENT,
              mission_log.ABORTED: RED_PALE,
              mission_log.FAILED: RED_PALE,
              mission_log.EXPIRED: SUB,
              mission_log.RUNNING: GOLD}
    worte = {mission_log.COMPLETED: 's_al_fertig',
             mission_log.ABORTED: 's_al_abbruch',
             mission_log.FAILED: 's_al_fehl',
             # Zurueckhaltend in Grau: Es ist keine Leistung und kein Abbruch,
             # nur das Ende der Spur.
             mission_log.EXPIRED: 's_al_verfallen',
             mission_log.RUNNING: 's_al_laeuft'}

    def _wort_laufend():
        """„läuft" nur, solange das Spiel wirklich schreibt.

        ⚠⚠ Ausloggen beendet keinen Auftrag — das Spiel schreibt dafür nichts ins
        Protokoll. Aufgeräumt wird so ein Fall erst, wenn eine **spätere**
        Sitzung ihn nicht mehr nennt (`_close_expired`); beim letzten
        Auftrag vor dem Ausloggen gibt es die noch nicht. Gemessen an 381
        Aufträgen: 68 waren so bereits aufgelöst, genau einer blieb übrig —
        der jüngste.

        ⚠ **Der Zustand bleibt, nur das Wort hängt am laufenden Spiel.** Der
        Auftrag ist im Spiel weiter angenommen; beim nächsten Einloggen meldet
        Star Citizen ihn erneut. Ihn zu beenden wäre gelogen. „läuft"
        behauptet aber „jetzt gerade" — und das stimmt bei geschlossenem Spiel
        nicht. „Noch offen" stimmt in beiden Fällen.
        """
        return 's_al_laeuft' if paths.game_running() else 's_al_offen'

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
                daten['alle'] = mission_log.load()
            except Exception:
                daten['alle'] = []

        # ⭐⭐ **Nur neu zeichnen, wenn sich etwas geändert hat.**
        #
        # Ein kompletter Neubau bei JEDEM Anzeigen (und nach der Nachlese
        # gleich noch einmal) kostet rund 400 ms, fast alles in Tk-Aufrufen.
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
        # ⚠ Einmal je Durchlauf, nicht je Zeile: `game_running()` sieht auf
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
                    if (e.get('zustand') or mission_log.RUNNING) == stand['art']]
        treffer = mission_log.search(alle, suche.get())
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
            zustand = eintrag.get('zustand') or mission_log.RUNNING
            zeile = tk.Frame(liste_rahmen, bg=SURFACE)
            zeilen_log.append(zeile)

            tk.Label(zeile, text=(eintrag.get('wann') or '')[:10],
                     bg=SURFACE, fg=SUB, font=fenster.f_small, width=11,
                     anchor='w', padx=10, pady=7).pack(side='left')
            # ⚠ Breit genug fuer den laengsten Zustand — „nicht mehr offen"
            # hat 16 Zeichen.
            # ⚠ Der laufende Zustand heisst je nach Lage anders — siehe
            # `_wort_laufend`. Einmal je Durchlauf gefragt, nicht je Zeile:
            # Das ist ein Dateizugriff, und die Liste hat hunderte Zeilen.
            schluessel = (wort_laufend if zustand == mission_log.RUNNING
                          else worte.get(zustand, 's_al_laeuft'))
            tk.Label(zeile, text=t(schluessel),
                     bg=SURFACE, fg=farben.get(zustand, SUB),
                     font=fenster.f_small, width=17,
                     anchor='w').pack(side='left')

            # ⭐ Rechts, vor der Mitte gepackt: Die Mitte nimmt sich mit
            # `expand` den Rest, ein danach gepacktes Element fiele heraus.
            # Der Suchbegriff entsteht erst beim Klick — das Nachschlagen des
            # englischen Titels liest die Sprachdateien.
            def im_issue_council(_ereignis=None,
                                 titel=eintrag.get('name') or ''):
                _open_issue_council(fenster, titel)

            ic_knopf = icons.tappable(zeile, 'issue_council',
                                      background=SURFACE,
                                      text=t('s_al_ic'),
                                      font=fenster.f_small)
            ic_knopf.configure(cursor='hand2', padx=10)
            ic_knopf.pack(side='right')
            ic_knopf.bind('<Button-1>', im_issue_council)
            icons.hover_group(ic_knopf)
            from . import notice
            notice.attach(ic_knopf, lambda: t('s_al_ic_tipp'))

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

            # ⭐ **Der Auftragsname führt zu seinen Bauplänen**
            # (Drei-Klick-Regel). Der Klick stellt die Bauplan-Liste auf ihn
            # ein.
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
            notice.attach(name_lab, lambda: t('s_al_klick'))
            # Der Stand gehoert nur an einen laufenden Auftrag. Bei einem
            # beendeten waere er Ballast — er ist ja fertig.
            if (zustand == mission_log.RUNNING
                    and eintrag.get('ziele_gesamt')):
                tk.Label(mitte,
                         text=t('s_al_ziele', eintrag.get('ziele_fertig') or 0,
                                eintrag['ziele_gesamt']),
                         bg=SURFACE, fg=SUB, font=fenster.f_small,
                         anchor='w').pack(fill='x')

            # ⭐ Was dabei herauskam — meist der Grund, warum jemand nach einem
            # Auftrag sucht. In der Markenfarbe, damit es beim Ueberfliegen auffaellt.
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

    # ⚠⚠ **Die Filterknoepfe tragen die Farbe ihres Zustands:**
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
                (mission_log.RUNNING, 's_al_laeuft', GOLD),
                (mission_log.COMPLETED, 's_al_fertig', ACCENT),
                (mission_log.ABORTED, 's_al_abbruch', RED_PALE),
                (mission_log.FAILED, 's_al_fehl', RED_PALE),
                (mission_log.EXPIRED, 's_al_verfallen', SUB)]

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

    suche.trace_add('write', after_typing(rahmen, zeichnen))
    zeichnen()

    def _auffrischen():
        """Erst die Logs nachlesen, dann anzeigen.

        ⚠⚠ **Die Datei allein neu zu laden genügt nicht.** Beim Programmstart
        füllt `_nachlese` im Hauptprogramm das Protokoll; wer den Watcher
        morgens startet und mittags einen Auftrag abgibt, fände ihn sonst hier
        nicht — in der Datei stünde seit dem Start nichts Neues.

        ⚠ Das ist billig: Der Lesestand merkt sich Name und Groesse jeder
        Logdatei, also wird nur die laufende `Game.log` erneut gelesen.
        Gemessen an 183 Sicherungen: **20 ms**, wenn sie gewachsen ist, und
        3 ms, wenn sich nichts getan hat.
        """
        # ⚠⚠⚠ **Im Hintergrund, nicht im Seitenaufbau.** Der Kommentar oben
        # nennt 20 ms — das gilt, wenn der Lesestand gefüllt ist und nur die
        # laufende `Game.log` neu gelesen wird. Beim **ersten** Mal ist er
        # leer, und dann werden alle Sicherungen durchgegangen: gemessen rund
        # **9 s** für diese eine Seite, in denen das Fenster steht und auf
        # keinen Klick reagiert.
        #
        # Gezeigt wird sofort der gespeicherte Stand; was dazukommt, kommt
        # nach. Ein Protokoll, das eine Sekunde später vollständig wird, ist
        # besser als eines, für das man neun Sekunden wartet.
        zeichnen(neu_laden=True)

        def nachlesen():
            try:
                mission_log.scan_backlog()
            except Exception as ausnahme:
                # Ein fehlgeschlagenes Nachlesen darf die Seite nicht leer
                # lassen — der gespeicherte Stand ist besser als nichts.
                errors.record('pages.auftragslog_nachlese', ausnahme)
                return
            # ⚠ Zurück in den Oberflächen-Faden; und nur zeichnen, wenn es die
            # Seite noch gibt.
            _from_thread(rahmen, lambda: zeichnen(neu_laden=True))

        threading.Thread(target=nachlesen, daemon=True).start()

    # ⚠ Bei jedem Öffnen frisch laden — siehe oben. Ohne das bleibt eine Seite,
    # die während der Nachlese gebaut wurde, für immer leer.
    fenster.on_show['auftragslog'] = _auffrischen

    # ⚠⚠ **Und beim ERSTEN Öffnen auch.** `on_show` feuert nur, wenn die
    # Seite bereits gebaut war (`if kennung in self.gezeichnet`) — beim ersten
    # Besuch also nicht. Wer den Watcher morgens startet, mittags einen Auftrag
    # abgibt und dann zum ersten Mal hierher wechselt, sähe sonst den Stand
    # vom Programmstart.
    #
    # ⚠ Der Aufruf steht am Ende, nachdem alles gebaut ist — vorher gäbe es
    # nichts zu zeichnen.
    _auffrischen()


def _system_path_label(path):
    """Der Systempfad eines Geräts so, wie er in eine Zeile passt.

    Unter Linux ist er kurz (`/dev/input/js0`) und bleibt, wie er ist. Unter
    Windows ist er der volle Gerätepfad (`\\\\?\\HID#VID_3344&PID_43F5&…#{…}`),
    über 100 Zeichen lang — ungekürzt schiebt er den Zustand „bereit" am
    Zeilenende aus dem Bild (`randpruefung`) und überlagert den Gerätenamen. Wiedererkennbar sind daran nur Hersteller- und
    Gerätenummer; der Rest ist für den Spieler Rauschen.
    """
    if not path:
        return '—'
    import re
    m = re.search(r'VID_([0-9A-F]{4}).*?PID_([0-9A-F]{4})', path, re.I)
    if m:
        return t('s_gh_vidpid').format(m.group(1).upper(), m.group(2).upper())
    return path if len(path) <= 24 else path[:22] + '…'


def _device_hub(fenster, eltern):
    """Alle Eingabegeräte an einem Ort — mit laufender Überwachung.

    ## Warum das oben auf der Steuerungsseite steht

    Die Seite darunter beantwortet „was liegt auf welcher Taste". Davor steht
    aber eine Frage, die sonst niemand beantwortet: **welches Gerät ist
    überhaupt welches?** Über einen Stick gibt es drei Aussagen — was das
    System sieht, was das Spiel zuletzt sah, und was in der Belegung steht —
    und ihre Nummern stimmen nicht überein: Derselbe Stick kann am System
    `js0` und im Spiel `js2` sein.

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
            # ⚠ Reihenfolge = Vorrang. Tk nimmt bei Platzmangel dem ZULETZT
            # gepackten Element den Platz weg. Also erst, was nie fehlen darf
            # (Nummer, Zustand), dann der Pfad, zuletzt der dehnbare Name.
            # Hinter dem Pfad würde „bereit" bei langen Windows-Pfaden
            # abgeschnitten.
            tk.Label(zeile, text=nummer, bg=SURFACE,
                     fg=ACCENT if geraet['nummer'] else SUB,
                     font=fenster.f_bold, width=5,
                     anchor='w', padx=8).pack(side='left', pady=6)
            tk.Label(zeile, text=t('s_gh_' + geraet['zustand']), bg=SURFACE,
                     fg=farben.get(geraet['zustand'], SUB),
                     font=fenster.f_small, anchor='e',
                     padx=10).pack(side='right')
            tk.Label(zeile, text=_system_path_label(geraet['systempfad']),
                     bg=SURFACE, fg=SUB, font=fenster.f_small,
                     anchor='e', padx=10).pack(side='right')
            tk.Label(zeile, text=geraet['name'] or geraet['kurz'], bg=SURFACE,
                     fg=FG, font=fenster.f_small,
                     anchor='w').pack(side='left', fill='x', expand=True)

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
                zeilen = len(joysticks.bindings() or {})
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
    suche = tk.StringVar(rahmen)
    nur = {'geraet': '', 'sicht': joysticks.ALL}

    # ⚠⚠ **Das Suchfeld wird EINMAL gebaut und danach nie wieder angefasst.**
    #
    # Würde bei jedem Tastendruck die ganze Seite neu gebaut — also auch das
    # Feld, in das der Spieler gerade tippt —, wäre nach jedem Buchstaben der
    # Eingabezeiger weg.
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
    feld = round_entry(werkzeug, suche, fenster.f_small, theme.FIELD, LINE,
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
        if kennzeichen == joysticks.FREE:
            return t('s_js_ohne_eingabe')
        art = joysticks.kind_of(kennzeichen)
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
        zustand = v.get('zustand') or joysticks.EMPTY

        if not geraete:
            tk.Label(oben, text=t('s_js_leer'), bg=BG, fg=SUB,
                     font=fenster.f_small, anchor='w', justify='left',
                     wraplength=560).pack(fill='x', pady=8)
            return

        # --- Zustandszeile: die eine Aussage, wegen der man hier nachsieht ---
        farbe = {joysticks.MATCHES: ACCENT, joysticks.REPLACED: GOLD,
                 joysticks.MISSING: RED}.get(zustand, SUB)
        if zustand == joysticks.MATCHES:
            satz = t('s_js_passt')
        elif zustand == joysticks.REPLACED:
            satz = t('s_js_ersetzt')
        elif zustand == joysticks.MISSING:
            satz = t('s_js_fehlt', len(v.get('fehlende') or []))
        else:
            satz = t('s_js_keine_datei')
        tk.Label(oben, text=satz, bg=BG, fg=farbe, font=fenster.f_bold,
                 anchor='w', justify='left', wraplength=560).pack(fill='x')

        # --- Der eine reparierbare Fall: Gerät unter neuer Kennung ---
        if zustand == joysticks.REPLACED and v.get('ersatz'):
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
        betroffenen Belegungen — „alles zurücksetzen?" allein ist zu
        abstrakt.
        """
        eigene = 0
        for liste in (joysticks.view(joysticks.MINE) or {}).values():
            eigene += len(liste)
        if not _ask(fenster, t('s_js_zurueck'),
                                   t('s_js_zurueck_frage', eigene)):
            return
        erfolg, meldung, _ = joysticks.reset()
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
        from .language import current
        endung = '.csv' if als_csv else '.xml'
        ziel = file_picker.save_file(
            t('s_js_ausgeben'),
            suggestion='actionmaps' + endung, extension=endung)
        if not ziel:
            return
        erfolg, meldung = joysticks.export_file(ziel, current())
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
        vorhandene = joysticks.profiles()
        # ⚠ **Nicht `simpledialog.askstring`.** Der Systemdialog kommt grau, in
        # der Systemschrift und mit englischem „Cancel" — auf dem dunklen Grund
        # ein Fremdkörper. `text_stellen()` ist derselbe Dialog im Programmstil.
        name = ask_text(
            fenster.root, t('s_js_profil'), t('s_js_profil_frage'),
            choices=vorhandene,
            choices_title=t('s_js_profil_liste') if vorhandene else '')
        if name is None:
            return                       # abgebrochen, nicht leer bestätigt
        ok, meldung = joysticks.check_name(name)
        if not ok:
            _notice(fenster, t('hf_joysticks'), t(meldung))
            return
        name = name.strip()
        erfolg, meldung = joysticks.save_profile(name)
        # Ein vorhandenes Profil wird nicht stillschweigend überschrieben —
        # dahinter kann die Belegung eines ganzen Abends stecken.
        if not erfolg and meldung == 's_js_f_name_belegt':
            if not _ask(fenster, t('s_js_profil'),
                                       t('s_js_profil_ersetzen', name)):
                return
            erfolg, meldung = joysticks.save_profile(
                name, overwrite=True)
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
        vorhandene = joysticks.profiles()
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
                quelle = joysticks.profile_file(name)
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
        erfolg, meldung, anzahl = joysticks.import_file(quelle)
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
        # ⚠⚠ **Der gefährliche Knopf steht allein, in einer eigenen Zeile** —
        # nicht als fünfter in einer Reihe neben vier harmlosen, wo er
        # abgeschnitten und aus Versehen gedrückt würde. Ein Knopf, der die
        # ganze Belegung wegwirft, gehört nicht dorthin, wo die Hand ohnehin
        # gerade ist.
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
        for schluessel, welche in ((('s_js_s_meine'), joysticks.MINE),
                                   ('s_js_s_alles', joysticks.ALL),
                                   ('s_js_s_standard', joysticks.DEFAULT),
                                   # ⭐ Ohne diese Sicht käme man an 411
                                   # Aktionen gar nicht heran: Was nirgends
                                   # belegt ist, steht in keiner Liste — und
                                   # was in keiner Liste steht, kann man auch
                                   # nicht anklicken, um es zu belegen.
                                   ('s_js_s_frei', joysticks.FREE)):
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
        # Sticks stehen — die Reihenfolge kommt aus `joysticks.KINDS`.
        for art, kuerzel in joysticks.KINDS:
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
        arten = list(joysticks.KINDS) + [('frei', joysticks.FREE)]
        for _art, kuerzel in arten:
            for kennzeichen in sorted(k for k in alle
                                      if k.startswith(kuerzel)):
                if nur['geraet'] and kennzeichen != nur['geraet']:
                    continue
                for e in alle[kennzeichen]:
                    klar, hinweis, echt = (namen.get(e['aktion'])
                                           or ('', '', False))
                    lesbar = joysticks.input_readable(e['eingabe'],
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
            t('s_js_frei_hinweis') if nur['sicht'] == joysticks.FREE
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
            # `side='right'`-Beschriftung aus dem Fenster. Die Abnahme prüft
            # die Textbreiten.
            if e.get('quelle') == joysticks.MINE:
                tk.Label(zeile, text=t('s_js_q_meine'), bg=SURFACE, fg=ACCENT,
                         font=fenster.f_small, anchor='e', padx=10).pack(
                             side='right')
            # ⚠ Grau heißt „das ist keine Bezeichnung des Spiels, sondern der
            # aufbereitete technische Name" — 382 Aktionen haben keine.
            #
            # ⚠ `expand=True`: Der Name darf schrumpfen, die Marke daneben
            # nicht.
            #
            # ⭐ **Was nicht passt, wird UMGEBROCHEN statt abgeschnitten.**
            # Bei „sehr groß" fehlte einem langen Aktionsname sonst ein
            # Drittel. Eine Zeile mehr kostet nichts, ein abgeschnittener Name
            # kostet die Auskunft.
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
        erfolg, meldung, _ = joysticks.swap_id(alt['kennung'],
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
                   or joysticks.group_of(eintrag['aktion'])
                   or _bereich_suchen(eintrag['aktion']))
        if kennzeichen == joysticks.FREE:
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
            errors.record('pages.joysticks_belegen', ausnahme)

    def _bereich_suchen(aktion):
        """Zu welcher Gruppe gehört eine Aktion?

        Die Werkseinstellung nennt die Gruppe nicht mit — sie steht nur in
        der eigenen `actionmaps.xml`. Findet sich dort nichts, bleibt die
        Gruppe leer, und `joysticks.bind_action()` legt sie unter der
        gebräuchlichsten an.
        """
        for liste in (joysticks.bindings() or {}).values():
            for e in liste:
                if e['aktion'] == aktion and e.get('bereich'):
                    return e['bereich']
        return ''

    def _laden():
        """Die Belegungen in der gewählten Sicht holen."""
        try:
            daten['belegungen'] = joysticks.view(nur['sicht'])
        except Exception as ausnahme:
            errors.record('pages.joysticks_sicht', ausnahme)
            daten['belegungen'] = {}

    # ⚠⚠ Der Fingerabdruck der zuletzt gezeichneten Lage — siehe unten.
    zuletzt = {'stand': None}

    def _auffrischen(erzwingen=False):
        from .language import current
        try:
            daten['vergleich'] = joysticks.compare()
            # Nummer → Produktname, damit in der Liste nicht `js1` steht.
            daten['geraetenamen'] = {
                'js%d' % z['nummer']: z['name']
                for z in (daten['vergleich'].get('zuordnung') or [])
                if z.get('name')}
        except Exception as ausnahme:
            errors.record('pages.joysticks', ausnahme)
            daten['vergleich'] = {}
            daten['geraetenamen'] = {}

        try:
            # ⚠ Die Klarnamen richten sich nach der **Programmsprache**, nicht
            # nach der Spielsprache: Wer den englischen Client fährt, aber die
            # Oberfläche auf Deutsch hat, will deutsche Aktionsnamen.
            #
            # ⚠⚠ **Hier steht bewusst KEIN `forget()`.** Bei JEDEM Anzeigen
            # kostet es rund 96 ms; an den Sprachwechsel gebunden, griffe es
            # zu selten — die Namen kommen aus **Dateien im Spielordner**, und
            # wer den Ordner umstellt oder das Spiel aktualisiert, bekäme
            # weiter die alten Namen.
            #
            # ⭐ Die Gültigkeit gehört dorthin, wo die Daten herkommen:
            # `joysticks.labels()` schlüsselt seinen Merker selbst nach
            # Sprache **und** Zustand der Quelldateien. Diese Seite muss gar
            # nichts darüber wissen — und bekommt trotzdem immer den richtigen
            # Stand.
            daten['namen'] = joysticks.labels(current())
        except Exception as ausnahme:
            errors.record('pages.joysticks_namen', ausnahme)
            daten['namen'] = {}
        _laden()

        # ⭐⭐ **Nichts neu zeichnen, wenn sich nichts geändert hat.**
        #
        # Dieser Rückruf läuft bei JEDEM Anzeigen der Seite — auch wenn man nur
        # kurz woanders war. Ein Neubau kostet fast eine Sekunde je Klick,
        # beinahe alles in `liste_zeichnen`.
        #
        # ⚠⚠ **Der Vergleich steht HIER — nach `_laden()`, nicht davor.**
        # `compare()` allein enthält Geräte, Zuordnung und Dateipfad, **nicht
        # die Belegungen**. Die kommen erst über `_laden()` →
        # `joysticks.view()`: `js1_x` auf `js1_y` umlegen lässt `compare()`
        # gleich, die Belegung ändert sich — die Seite zeigte den alten Stand.
        #
        # ⭐ Ein Fingerabdruck muss aus **genau den Daten** bestehen, aus denen
        # gezeichnet wird. Deshalb stehen alle vier Quellen drin, die
        # `kopf_zeichnen()` und `liste_zeichnen()` benutzen.
        #
        # ⚠ `erzwingen=True` für Aufrufer, die selbst etwas geändert haben.
        stand = (current(),
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
        if daten.get('belegungen') or nur['sicht'] != joysticks.ALL:
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
    suche.trace_add('write', after_typing(rahmen, liste_zeichnen))
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
        errors.record('pages.wasistneu', ausnahme)
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

    def fuellen():
        """Den Inhalt der Version bauen — beim ersten Aufklappen.

        ⭐ Zugeklappte Versionen bekommen keinen Inhalt, bis man
        sie öffnet. Alle 25 voll aufzubauen kostete beim ersten
        Öffnen der Seite rund 1.700 Tk-Aufrufe und eine halbe
        Sekunde — gezeigt wird davon nur die neueste.
        """
        if gebaut[0]:
            return
        gebaut[0] = True
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
            # Ein geschätzter Abzug („Fensterbreite minus …" für Seitenleiste,
            # Ränder und Art-Blase) geht schief, sobald sich eines davon ändert.
            #
            # Deshalb wird nicht gerechnet, sondern genommen, was das Label
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


    gebaut = [False]
    if offen:
        fuellen()

    def umschalten(*_):
        zustand['offen'] = not zustand['offen']
        pfeil.swap_symbol('zuklappen' if zustand['offen']
                             else 'aufklappen')
        if zustand['offen']:
            fuellen()
            # ⚠ `after=kopf` ist der ganze Witz. Ohne das packt Tk den Inhalt ans
            # **Ende** der Fläche — also unter alle anderen Versionen; wer nicht
            # weit genug rollt, hält die Version für leer. Beim ersten Zeichnen
            # fällt das nicht auf, weil dort Kopf und Inhalt ohnehin
            # nacheinander gepackt werden.
            koerper.pack(fill='x', after=kopf)
        else:
            koerper.pack_forget()

    # ⚠ **Alle** Teile des Kopfes binden, nicht nur Rahmen und Pfeil. Die
    # Versionsnummer, das Datum und die Anzahl sind eigene Labels — ein Klick
    # darauf erreichte den Rahmen sonst nie. Genau dorthin zielt man aber.
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

    ⚠ Nur zu **melden**, dass gesucht wird, reicht nicht: Der Knopf ist der
    einzige Weg, den Zwischenspeicher aufzufrischen.

    ⚠ Nachsehen ist nachsehen: Dieser Knopf lädt nichts — das Holen
    (herunterladen, einspielen, abtreten) gehört nicht hierher.

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
            errors.record('pages.jetzt_nachsehen', ausnahme)
            _from_thread(fenster.root,
                         lambda: fenster.say(t('s_ub_sucht_fehler')))
            return

        def melden():
            # ⚠ **Erst neu aufbauen, dann sagen.** `rebuild()` zerstoert
            # saemtliche Kinder des Fensters und baut sie neu — auch die
            # Fusszeile, in der `say()` schreibt. Stuende das `say()` davor,
            # existierte die Antwort ein paar Millisekunden und waere dann weg.
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
                # voneinander — etwa wenn der Abruf an GitHubs Stundengrenze
                # scheitert.
                fenster.say(t('s_ub_grenze') if updater.rate_limited()
                              else t('s_ub_sucht_fehler'))
            else:
                fenster.say(t('s_ub_aktuell'))

        _from_thread(fenster.root, melden)

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
            # die Beschriftung auf dem Stand von heute früh stehen — und bietet
            # womöglich eine ältere Version an; wer draufdrückt, geht
            # **zurück**. Einmal je Seitenaufbau nachfragen
            # ist der Preis dafür, dass draufsteht, was drin ist.
            updater.check(fenster.version or '0.0.0', force=True)
        except Exception as ausnahme:
            errors.record('pages.kanaele_auffrischen', ausnahme)
            return

        def nachziehen():
            try:
                if not kaesten.winfo_exists():
                    return
                # ⚠ **Und dann in Ruhe wieder nachsehen.** Nur einmal je
                # Seitenaufbau gefragt, sähe wer die Seite offen hat, während
                # draußen eine neue Version erscheint, weiter die alte Nummer
                # auf dem Knopf und hielte sich für aktuell.
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

        _from_thread(fenster.root, nachziehen)

    threading.Thread(target=arbeit, daemon=True).start()


def _can_fetch(mit_vorab, eigene=''):
    """Steckt hinter dem Knopf ueberhaupt eine Tat? Sonst ist er keiner.

    ⚠ `_fetch_label` liefert **nur** die Beschriftung, und zwei ihrer Ergebnisse
    sind gar keine Aufforderung, sondern eine Zustandsmeldung: „v3.0.0-rc41 ist
    schon da" und „Erst oben auf ‚Jetzt nachsehen' druecken". Bliebe der Knopf
    trotzdem ein Knopf, bekaeme wer auf „ist schon da" drueckt die laufende
    Version noch einmal installiert.

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
    v3.0.0-rc15 sieht aus wie ein Update und ist ein Rückschritt.
    Ist die angebotene Version älter, steht das dabei; ist es dieselbe,
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

# ⭐⭐ **Aufträge aus Nebenfäden laufen über diese Warteschlange.**
#
# `root.after()` aus einem Nebenfaden geht nur, solange der Hauptfaden
# gerade IN `mainloop()` steckt — nicht während einer Rückfrage, einer
# `update()`-Schleife oder beim Abbau, und genau da liegt ein Neustart nach
# dem Farbwechsel. Sonst steht `main thread is not in main loop` im
# Fehlerprotokoll und das alte Overlay bleibt stehen (Python 3.12 und 3.14
# gleich).
#
# Deshalb fasst ein Nebenfaden Tk gar nicht mehr an: Er legt die Tat hier
# ab, und der Tk-Faden holt sie alle 100 ms ab (`_start_tk_poller`, gestartet
# beim Seitenaufbau — der läuft immer im Tk-Faden).
import queue as _queue
_TK_CALLS = _queue.Queue()
_TK_POLLER = [None]


def _start_tk_poller(root):
    """Die Warteschlange im Tk-Faden abarbeiten — je Fenster einmal starten."""
    # ⚠ An der Tk-Wurzel, nicht am Hauptfenster: Das ist ein `Toplevel` über
    # dem Overlay. Ginge es zu, stünde der Abholer still, und was Nebenfäden
    # danach ablegen (der Signatur-Lauscher etwa), sammelte sich an.
    try:
        root = root._root()
    except AttributeError:
        pass
    if _TK_POLLER[0] is root:
        return

    def poll():
        try:
            while True:
                tat = _TK_CALLS.get_nowait()
                try:
                    tat()
                except Exception as ausnahme:
                    errors.record('pages.im_tk.tat', ausnahme)
        except _queue.Empty:
            pass
        try:
            root.after(100, poll)
        except (tk.TclError, RuntimeError):
            if _TK_POLLER[0] is root:
                _TK_POLLER[0] = None

    try:
        root.after(100, poll)
        _TK_POLLER[0] = root
    except (tk.TclError, RuntimeError) as ausnahme:
        errors.record('pages.tk_poller', ausnahme)


def _from_thread(widget, action):
    """Aus einem Nebenfaden: `action` im Tk-Faden ausführen, solange `widget` lebt.

    ⚠⚠ **Nie `widget.after(0, …)` aus einem Nebenfaden.** Das
    wirft `RuntimeError: main thread is not in main loop`, sobald der
    Hauptfaden gerade nicht in `mainloop()` steckt — und dann kommt das
    Ergebnis nie an. Der Weg geht über `_TK_CALLS`; Prüfung 292 bewacht das.

    Ist `widget` inzwischen weg (Seite gewechselt, Fenster zu), passiert
    nichts.
    """
    def run():
        try:
            if not widget.winfo_exists():
                return
        except tk.TclError:
            return
        action()
    _TK_CALLS.put(run)


def _in_tk(fenster, tat):
    """Etwas im Tk-Faden erledigen — und daran nicht scheitern.

    ⚠ **Zeichnen ist Beiwerk, die Arbeit ist der Zweck.** `root.after()` aus
    einem Nebenfaden kann werfen (`RuntimeError: main thread is not in main
    loop`), etwa wenn das Fenster gerade zugeht. Ungefangen risse so eine
    Ausnahme den ganzen Update-Faden mit: Der Download bräche beim ersten
    Fortschritt ab, und der Nutzer sähe gar nichts.

    ⚠ **Gemerkt wird nur der erste.** Beim Herunterladen kommt der Fortschritt
    im Sekundentakt; geht dabei das Fenster zu, wirft jeder einzelne Aufruf —
    binnen Sekunden wären alle 50 Plätze des Protokolls mit derselben Meldung
    belegt und jeder echte Fehler verdrängt. Ein erwarteter Fehler, der die
    Diagnose unbrauchbar macht, ist schlimmer als keiner.
    """
    if threading.current_thread() is not threading.main_thread():
        # Aus einem Nebenfaden nie Tk anfassen — siehe `_TK_CALLS`.
        _TK_CALLS.put(tat)
        return True
    try:
        fenster.root.after(0, tat)
        return True
    except Exception as ausnahme:
        if not _TK_REPORTED[0]:
            _TK_REPORTED[0] = True
            errors.record('pages.im_tk', ausnahme)
        return False


def restart_and_hand_over(fenster):
    """Neu starten UND die alte Fassung beenden — der ganze Weg in einem.

    ⚠⚠ `updater.restart()` startet nur die neue Fassung; gehen muss die alte
    selbst. `restart` allein ließe **zwei** VerseKit laufen, und das alte
    Overlay bliebe unter dem neuen stehen (Farbschema, Einspielen einer
    Sicherung). Gibt True zurück, wenn der
    Neustart angestoßen wurde."""
    from . import updater
    fenster.say(t('s_ub_startet_neu'))
    if not updater.restart():
        fenster.say(t('s_ub_neustart_nein'))
        return False
    _hand_over_after_restart(fenster)
    return True


def _hand_over_after_restart(fenster):
    """Erst nachsehen, ob die neue Version lebt — dann erst selbst gehen.

    ⚠ Tritt die alte Version **sofort** ab und ist die neue schon tot (siehe
    `updater.neue_fassung_laeuft`), steht der Rechner ohne Watcher da, und
    niemand erfährt den Grund.

    Die Prüfung wartet ein paar Sekunden und gehört deshalb in einen eigenen
    Faden. Gezeichnet wird nur im Tk-Faden.
    """
    import threading
    from . import updater

    # Im Tk-Faden starten, solange wir hier sind — der Prüf-Faden unten darf
    # Tk nicht anfassen.
    _start_tk_poller(fenster.root)

    def pruefen():
        lebt = updater.new_version_alive()
        if lebt:
            # ⚠⚠ **Das Sicherheitsnetz hängt an keinem Tk.** Läuft die neue
            # Fassung, geht die alte in jedem Fall — auch wenn die Oberfläche
            # gerade hängt oder das Fenster schon zu ist. `_hand_over` beendet
            # vorher sauber; dieser Zeitgeber greift nur, wenn das ausbleibt.
            threading.Timer(6.0, lambda: os._exit(0)).start()

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
        # Über die Warteschlange, nie `root.after` aus diesem Faden.
        _TK_CALLS.put(melden)

    threading.Thread(target=pruefen, daemon=True).start()


def _hand_over(fenster, notausgang=2.0, gleich=400):
    """Den Prozess beenden — verlaesslich, auch wenn Tk schon haengt.

    ⚠ `quit()` allein reicht nicht: Es beendet die Ereignisschleife, nicht den
    Prozess — die neue Version liefe, die alte stünde daneben. Also der Reihe
    nach: Fenster zu, Schleife beenden, und wenn nach zwei Sekunden immer noch
    etwas haengt (ein Faden, ein Overlay), hart raus.

    ⚠ Der Notausgang wird **sofort** scharf gestellt, nicht erst im
    `after`-Rueckruf. Dort hinge er an Tk: Feuert der Rueckruf nicht, weil
    die Ereignisschleife schon endete oder das Fenster weg ist, wird der
    Faden nie gestartet und der Prozess laeuft weiter — mit einem halb
    aufgeraeumten Rest, der danach „No such file or directory:
    ...base_library.zip" meldet.

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
    # da ist.
    try:
        updater.check(fenster.version or '0.0.0')
    except Exception as ausnahme:
        errors.record('pages.fassung_holen.nachsehen', ausnahme)
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
        # Minuten für Installer und AppImage. „Bitte hol die neue Version
        # selbst von der Releases-Seite" wäre in dieser Lücke falsch — dort
        # ist sie dann auch nicht.
        #
        # Sind Dateien da, aber keine passende, stimmt diese Meldung.
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

            # ⚠ Kein Hinweisfenster zum Quittieren: Der Watcher kommt nach
            # dem Setup von selbst wieder, und aus einem Klick sollen nicht
            # zwei werden. Die Ansage steht in der Fußzeile.
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
            # Der Installer **laeuft schon** — und wartet darauf, dass wir
            # endlich gehen. Bleiben wir, steht die Pause im Inno-Protokoll
            # auf die Millisekunde:
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
            # hoch faehrt uns der Helfer aus `update_run`. Die Sperre
            # gehoert ab hier dem Helfer; er gibt sie am Ende frei.
            if art == 'exe':
                uebergeben = True
                _in_tk(fenster, lambda: fenster.say(t('up_wird_eingespielt')))
                _hand_over(fenster)
                return

            # Linux: Das AppImage ist getauscht, die alte Fassung gesichert.
            # ⚠ Es geht gleich weiter, ohne zweiten Klick. Der Knopf „Jetzt
            # neu starten" ist nur der Rückfall, wenn schon der Start
            # scheitert.
            def _neustart():
                fenster.say(t('s_ub_startet_neu'))
                if updater.restart():
                    _hand_over_after_restart(fenster)
                    return
                # ⚠ Erst zeichnen, dann melden: Der Neuaufbau macht aus
                # „holen" ein „Jetzt neu starten" und zerstoert dabei die
                # Fusszeile. Stuende das `say()` zuerst, waere die Meldung nach
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
            errors.record('pages.fassung_holen', ausnahme)
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
    die Gesamtbreite: Wer mehr Text hat, bleibt breiter, und die beiden
    Kästen stünden sichtbar ungleich nebeneinander. `columnconfigure(…,
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
    # ⚠ **Kein Punkt vor dem Titel.** Er zeigte dasselbe wie der Rahmen des
    # Kastens, der bei der gewählten Fassung grün wird — zweimal dieselbe
    # Auskunft an derselben Stelle.
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
        # keine Verbindung **aber** ein alter Stand. Der Rat, auf „Jetzt
        # nachsehen" zu klicken, führt ohne Internet zu nichts.
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
            # dann.
            try:
                lage = serverstatus.state(force=erzwingen)
            except Exception as ausnahme:
                errors.record('pages.serverstatus', ausnahme)
                lage = None
            if lage is None:
                _from_thread(behaelter, lambda: fenster.say(t('s_st_fehler')))
            else:
                _from_thread(behaelter, lambda: zeichnen(lage))

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
            if veraendert and lage:
                _from_thread(behaelter, lambda: zeichnen(lage))
            _from_thread(behaelter, lambda: fenster.root.after(POLL_MS, takt))

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
    wenn der Inhalt nicht mitgemessen wird — der Streifen erschiene als leerer
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
            errors.record('pages.serverstatus_meldungen', ausnahme)
            liste = []
        _from_thread(fenster.root, lambda: einsetzen(liste))

    threading.Thread(target=arbeit, daemon=True).start()


def _source_link(fenster, eltern, text, adresse):
    """Ein anklickbarer Verweis als eigene Zeile."""
    link = tk.Label(eltern, text=text, bg=BG, fg=ACCENT, font=fenster.f_small,
                    anchor='w', cursor='hand2')
    link.pack(fill='x', pady=(10, 4))

    def oeffnen(_=None):
        # ⚠ Über `paths.open_in_browser` — nie `webbrowser.open()` direkt. Warum:
        # siehe die Begründung dort (im AppImage öffnet es nichts und meldet
        # trotzdem Erfolg).
        if not paths.open_in_browser(adresse):
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
        if not paths.open_in_browser(adresse):
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
    # Vorab entnommen, damit die Textprüfung die Schlüssel nicht für Sätze hält.
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
    # stünde mittig statt links.
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

    # ⚠ Auch hier durch `_ohne_marken`: Die Sternchen sind für den Leser der
    # Sprachdatei gedacht, nicht für den Spieler
    # (`tools/oberflaeche_pruefen.py`).
    text = tk.Label(kasten, text=_strip_markup(was), bg=SURFACE, fg=SUB,
                    font=fenster.f_small, anchor='w', justify='left')
    text.pack(fill='x', padx=16, pady=(0, 10))
    # ⚠⚠ `inset` MUSS die Polsterung aus `pack` nennen — hier 2 × 16.
    # `_wrap` misst den Elternrahmen; was das Label per `padx` abgibt, sieht
    # es nicht. Ohne die 32 steht `wraplength` zu hoch, und die letzte Zeile
    # jeder Quellenbeschreibung wird still abgeschnitten
    # (`tools/randpruefung.py`).
    _wrap(text, inset=32)

    if adresse:
        # ⚠ Nicht `_source_row`: die reserviert 24 Zeichen für eine
        # Beschriftung, und ohne Beschriftung stünde der Verweis eingerückt
        # mitten in der Karte statt am linken Rand wie der Text darüber.
        link = tk.Label(kasten, text=adresse, bg=SURFACE, fg=ACCENT,
                        font=fenster.f_small, anchor='w', cursor='hand2')
        link.pack(fill='x', padx=16, pady=(0, 12))
        # Lange Adressen (der Hub-Beitrag der Hangar-Erweiterung) brechen um,
        # statt das Fenster aufzudrücken (tools/randpruefung.py).
        _wrap(link, inset=32)

        def oeffnen(_=None):
            if not paths.open_in_browser(adresse):
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

    ⚠ Bei einem GPL-Programm gehört die eigene Lizenz (GPL-3.0) sichtbar hin,
    und die ISC-Lizenz von Lucide verlangt, dass ihr Hinweis mitgeliefert
    wird — eine Datei tief in der entpackten `.exe` erfüllt das formal, findet
    aber niemand.

    Ein **eigener Reiter** statt eines Abschnitts auf „Update & Über": Die Seite
    dort ist mit Version, Katalogzahlen, Update-Kanal und Holen-Knopf schon voll,
    und wem was gehört, hat mit Updates nichts zu tun.
    """
    _heading(fenster, rahmen, t('hf_danke'), t('s_dk_lead'))
    innen = _scroll_area(rahmen)

    # --- Wer das gebaut hat ---
    # ⚠ Ganz oben und mit Avatar, nicht als eine Zeile unter vielen. Diese Seite
    # nennt fremde Arbeit, und genau deshalb muss die eigene zuerst stehen —
    # sonst schmälert die Aufzählung das, worum es hier eigentlich geht.
    # Nur hier, nicht zusätzlich auf „Update & Über" — dieselben Angaben
    # gehören nicht an zwei Stellen.
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
            teiler = max(1, voll.width() // dpi.px(64))
            fenster._author_logo = voll.subsample(teiler, teiler)
            tk.Label(zeile, image=fenster._author_logo, bg=SURFACE).pack(
                side='left', padx=(0, 16))
        except Exception as ausnahme:
            errors.record('pages.danke.logo', ausnahme)
    rechts = tk.Frame(zeile, bg=SURFACE)
    rechts.pack(side='left', fill='x', expand=True)
    tk.Label(rechts, text='Xharig', bg=SURFACE, fg=ACCENT, font=fenster.f_title,
             anchor='w').pack(fill='x')
    tk.Label(rechts, text='%s %s · GPL-3.0-only'
             % (t('hf_titel'), fenster.version or ''), bg=SURFACE, fg=SUB,
             font=fenster.f_small, anchor='w').pack(fill='x')
    # Die Projektseite zuerst: Spieler finden sich dort schneller zurecht als
    # auf GitHub. GitHub bleibt darunter — dort liegt der Quellcode, den die
    # GPL-Zeile oben verspricht.
    _link(fenster, rechts, 'versekit.xharig.com',
             'https://versekit.xharig.com/')
    _link(fenster, rechts, 'github.com/Xharig/VerseKit',
             'https://github.com/Xharig/VerseKit')
    _body_text(innen, t('s_dk_selbst_h'), fenster.f_small, fill='x',
                pady=(10, 0))

    # --- Mitgeliefert ---
    tk.Label(innen, text=t('s_dk_dabei'), bg=BG, fg=FG, font=fenster.f_title,
             anchor='w').pack(fill='x', pady=(18, 2))
    _body_text(innen, t('s_dk_dabei_h'), fenster.f_small, fill='x',
                pady=(0, 10))
    _credit_box(fenster, innen, 'Lucide', 'ISC', t('s_dk_symbole'),
               'https://lucide.dev')
    # Die zwei Flaggen der Sprachwahl — die einzigen Bilder
    # neben Lucide (Lizenztext unter `tools/flaggen-vorlagen/LICENSE`).
    _credit_box(fenster, innen, 'flag-icons', 'MIT', t('s_dk_flaggen'),
               'https://flagicons.lipis.dev')
    # Die DPoP-Referenz des Basetools für dessen Anmeldung
    # (`scbp/dpop_reference/`, Lizenztext dort in `LICENSE`).
    _credit_box(fenster, innen, 'Basetool DPoP reference', 'MIT',
               t('s_dk_dpop'), 'https://krt-profit.github.io/basetool/')

    # --- Wird geladen, nicht mitgeliefert ---
    tk.Label(innen, text=t('s_dk_extern'), bg=BG, fg=FG, font=fenster.f_title,
             anchor='w').pack(fill='x', pady=(18, 2))
    _body_text(innen, t('s_dk_extern_h'), fenster.f_small, fill='x',
                pady=(0, 10))
    _credit_box(fenster, innen, 'Star Citizen Mission DataBase',
               'CC BY-NC-ND 4.0', t('s_dk_scmdb'), 'https://scmdb.net')
    # ⚠ Die Rohstoffpreise kommen von hier. Wer eine Quelle benutzt, nennt
    # sie.
    _credit_box(fenster, innen, 'UEX Corp',
               t('s_dk_keine_lizenz'), t('s_dk_uex'), 'https://uexcorp.space')
    # ⚠ Die Ruf-Stufen der Ränge kommen von hier.
    _credit_box(fenster, innen, 'scunpacked-data (Star Citizen Wiki)',
               t('s_dk_keine_lizenz'), t('s_dk_scunpacked'),
               'https://github.com/StarCitizenWiki/scunpacked-data')
    # ⚠ Die Steckplätze der Schiffe kommen von hier. Wer eine
    # Quelle benutzt, nennt sie — und zwar bevor jemand danach fragt.
    _credit_box(fenster, innen, 'erkul.games',
               t('s_dk_keine_lizenz'), t('s_dk_erkul'), 'https://erkul.games')
    # ⚠ Kein Datenlieferant, sondern fremdes **Werkzeug**: Ohne diese
    # Erweiterung müsste jeder seine vierzig Schiffe von Hand eintippen. Sie
    # steht hier, weil der Import ohne sie nichts wäre — und damit man sie
    # findet, ohne im Netz danach suchen zu müssen.
    _credit_box(fenster, innen, 'Star Citizen: Hangar Extension (AlyxOne)',
               'MIT', t('s_dk_hangarext'), HANGAR_EXT_PAGE)
    _credit_box(fenster, innen, 'Star Citizen Hangar XPLORer (dolkensp)',
               'MIT', t('s_dk_xplorer'), XPLORER_PAGE)
    # ⭐ Der Bauplan-Export folgt dem Format des Basetools, und die Zuordnung
    # der Protokolle zum Account stammt aus dessen
    # SC-Extractor. Verlinkt wird das öffentliche Repository — die Seite
    # selbst verlangt eine Anmeldung und hilft niemandem von außen.
    _credit_box(fenster, innen, 'Profit Basetool (greluc)', 'GPL-3.0',
               t('s_dk_basetool'), 'https://github.com/krt-profit/basetool')
    # StarStrings hat KEINE Lizenzangabe - kein LICENSE im Repo, nichts in
    # der readme, GitHub meldet keine. Deshalb hier keine Lizenz nennen;
    # eine geratene schriebe MrKraken etwas zu, das er nie vergeben hat.
    _credit_box(fenster, innen, 'StarStrings (MrKraken)',
               t('s_dk_keine_lizenz'),
               t('s_dk_ss'), 'https://starstrings.app')
    # ⭐ Wählbar unter „Übersetzung". Beide Projekte nennen keine Lizenz —
    # deshalb nichts mitgeliefert, nur bei Auswahl von ihrer Adresse geladen,
    # und hier genannt.
    _credit_box(fenster, innen, 'StarCitizen-Localization (Dymerz)',
               t('s_dk_keine_lizenz'), t('s_dk_dymerz'),
               'https://github.com/Dymerz/StarCitizen-Localization')
    _credit_box(fenster, innen, 'Star_citizen_ES (Thord82)',
               t('s_dk_keine_lizenz'), t('s_dk_thord82'),
               'https://github.com/Thord82/Star_citizen_ES')
    # ⭐ Die Übersetzungen des SC Launch Configurator sind ausdrücklich
    # freigegeben. Das Werkzeug steht
    # weiter unten unter den Werkzeugen anderer — hier zählt die Sprachquelle.
    # ⚠ Der Titel ist der **Eigenname**, kein beschreibender Satz — sonst steht
    # dort auf Englisch ein deutsches Wort („Luftwerft-Übersetzungen"), und
    # `oberflaeche_pruefen` schlägt zu Recht an.
    _credit_box(fenster, innen, 'SC Launch Configurator (Luftwerft)',
               t('s_dk_keine_lizenz'), t('s_dk_luftwerft'),
               'https://www.luftwerft.com/')
    # ⚠⚠ Die Übersetzung selbst hat einen eigenen Urheber und eine eigene
    # Lizenz (CC BY-NC-SA 4.0). Die verlangt ausdrücklich Name UND Repository —
    # der Verteiler allein genügt nicht.
    _credit_box(fenster, innen, 'StarCitizen-Deutsch-INI (rjcncpt)',
               'CC BY-NC-SA 4.0', t('s_dk_ini'),
               'https://github.com/rjcncpt/StarCitizen-Deutsch-INI')

    # --- Menschen ---
    # ⚠ Aufklappbar, weil die Liste wächst. Sichtbar bleibt immer nur der
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
            # ⚠ Zwei Textbausteine hintereinander. `_contributor` nimmt einen
            # Text — also hier zusammensetzen, statt die Funktion für einen
            # Sonderfall umzubauen.
            #
            # ⚠⚠ **Eine Person, ein Eintrag** — die Seite zählt Beiträge je
            # Person.
            ('rurudotorg', 'SC4M', t('s_dk_rurudotorg_idee'),
             t('s_dk_rurudotorg_bugs') + '\n\n' + t('s_dk_rurudotorg_bugs2')),
            ('Morkhan', 'KRT', t('s_dk_morkhan_idee'),
             t('s_dk_morkhan_bugs')),
            ('Horthy', 'KRT', t('s_dk_horthy_idee'), ''),
            ('Bushwick4712', 'KRT',
             t('s_dk_bushwick_idee') + '\n\n' + t('s_dk_bushwick_idee2')
             + '\n\n' + t('s_dk_bushwick_idee3')
             + '\n\n' + t('s_dk_bushwick_idee4'),
             t('s_dk_bushwick_bugs') + '\n\n' + t('s_dk_bushwick_bugs2')
             + '\n\n' + t('s_dk_bushwick_bugs3')),
            ('Choopa', '', t('s_dk_choopa_idee'), ''),
            ('YoshimitsuDE', 'KRT', t('s_dk_yoshimitsu_idee'), ''),
            ('Zwaersch', 'KRT', t('s_dk_zwaersch_idee'),
             t('s_dk_zwaersch_bugs') + '\n\n' + t('s_dk_zwaersch_bugs2')),
            ('Blackd0g84', 'KRT', t('s_dk_blackdog_idee') + '\n\n'
             + t('s_dk_blackdog_idee2') + '\n\n'
             + t('s_dk_blackdog_idee3'), ''),
            ('Aeternitas26', 'KRT', t('s_dk_aeternitas_idee') + '\n\n'
             + t('s_dk_aeternitas_idee2'), t('s_dk_aeternitas_bugs')),
            ('KynoTnis', 'ADI', t('s_dk_kynotnis_idee'),
             t('s_dk_kynotnis_bugs')),
            ('ryze', 'KRT', t('s_dk_ryze_idee'), ''),
            ('F_i_r_e', 'KRT', '', t('s_dk_fire_bugs')),
            ('greluc', 'KRT', t('s_dk_greluc_idee') + '\n\n'
             + t('s_dk_greluc_idee2'), t('s_dk_greluc_bugs')),
            ('Parsul', 'KRT', '', t('s_dk_parsul_bugs') + '\n\n'
             + t('s_dk_parsul_bugs2') + '\n\n' + t('s_dk_parsul_bugs3')),
            # ⚠ Ohne Gruppenblase — er tritt ohne Gruppe auf. Die Erweiterung
            # selbst steht oben unter den fremden Werkzeugen; hier zählt sein
            # Beitrag zum Werkzeug.
            ('AlyxOne', '', t('s_dk_alyxone_idee'), '')):
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
    from . import paths
    _heading(fenster, rahmen, t('hf_ueber'), t('s_ub_lead'))
    innen = _scroll_area(rahmen)

    # --- Zustand ---
    # ⚠ Mit dem Programmsymbol daneben — der Block mit dem Avatar steht auf
    # „Danke & Lizenzen", ohne Symbol hätte diese Seite gar kein Bild.
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
            teiler = max(1, voll.width() // dpi.px(48))
            fenster._about_logo = voll.subsample(teiler, teiler)
            tk.Label(kopf, image=fenster._about_logo, bg=SURFACE).pack(
                side='left', padx=(0, 14))
        except Exception as ausnahme:
            errors.record('pages.ueber.symbol', ausnahme)
    titel = tk.Frame(kopf, bg=SURFACE)
    titel.pack(side='left', fill='x', expand=True)
    # ⚠ Produktname aus `language.py` — nie fest hier, sonst bliebe er bei
    # einer Umbenennung teils alt.
    tk.Label(titel, text=t('hf_titel'), bg=SURFACE, fg=FG,
             font=fenster.f_title, anchor='w').pack(fill='x')
    tk.Label(titel, text=fenster.version or '—', bg=SURFACE, fg=ACCENT,
             font=fenster.f_bold, anchor='w').pack(fill='x')

    tk.Frame(karte, bg=SURFACE, height=8).pack()
    _value_row(fenster, karte, t('s_ub_bekannt'), _count_catalog())
    _value_row(fenster, karte, t('s_ub_davon'), _count_collection())
    uebersicht = {}
    try:
        uebersicht = paths.overview() or {}
    except Exception:
        pass
    _value_row(fenster, karte, t('b_ordner'),
               uebersicht.get('app_ordner') or '—')
    tk.Frame(karte, bg=SURFACE, height=10).pack()

    # --- Einmal holen, ohne etwas umzustellen ---
    #
    # ⚠ Die häufigste Absicht ist die einfachste: die neueste Version holen,
    # egal welche — ohne erst verstehen zu müssen, was ein Kanal ist, und den
    # richtigen Kasten zu finden.
    #
    # ⚠ **Und er steht ganz oben, direkt unter der Versionskarte.** Nach der
    # Knopfreihe und dem Tagesschalter läge er bei der Mindestgröße des
    # Fensters **unterhalb der Kante**.
    # Das Fenster größer zu machen wäre die falsche Antwort: Auf einem
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
        # ⚠ Nicht "stark": Der hervorgehobene Knopf der Seite ist der
        # Hol-Knopf darüber. Zwei starke Knöpfe nebeneinander heben sich
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
        paths.set_setting('vorabversionen', wert)
        fenster.say(t('e_vorab') + ': ' + (t('e_an') if wert else t('e_aus')))
        for kind in kaesten.winfo_children():
            kind.destroy()
        kanal_zeichnen()

    # Unterhalb dieser Breite stehen die beiden Kästen untereinander. 620 ist
    # gemessen, nicht geschätzt: Darunter reicht der Platz nicht mehr für zwei
    # nebeneinander, und Tk quetscht den zweiten zusammen, statt umzubrechen.
    SCHMAL = 620

    def kanal_zeichnen():
        an = paths.setting_bool('vorabversionen', False)
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
        # sofort steht. Der frischt sich aber nur einmal am Tag auf — der Knopf
        # böte dann eine veraltete Version an. Der Knopf holt zwar die
        # richtige Version (er sieht vorher nach), aber was draufsteht, führt in
        # die Irre. Deshalb einmal im Hintergrund nachsehen und die Kästen neu
        # zeichnen, wenn sich etwas geändert hat.
        _refresh_channels(fenster, kaesten, kanal_zeichnen)

    # ⚠ Der Tagesschalter steht **hinter** den Kanal-Kästen, nicht davor. Davor
    # drückte er die Kästen bei der Mindestgröße des Fensters unter die Kante —
    # und in ihnen sitzt der Knopf, mit dem man die stabile Version holt.
    # Der Schalter ist eine Nebeneinstellung, die Kästen sind der Zweck der
    # Seite; also gehören sie nach oben.
    ziel = _setting_row(fenster, innen, t('s_ub_taeglich'), t('s_ub_taeglich_h'))
    _switch(fenster, ziel, 'update_pruefen', True)
    # ⭐ Direkt darunter, weil es davon abhängt: Wer nicht nachsehen lässt,
    # bekommt auch nichts eingespielt (`auto_update.enabled`).
    from . import auto_update as _au
    ziel = _setting_row(fenster, innen, t('s_ub_auto'), t('s_ub_auto_h'))
    _switch(fenster, ziel, _au.SETTING, True)

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

    ⚠ Ein gewöhnliches Label in der Akzentfarbe **sähe aus wie ein Link** und
    täte nichts — schlimmer als schwarzer Text, weil es zum Klicken einlädt.
    Deshalb ist der Mauszeiger eine Hand, die Adresse unterstreicht sich beim
    Überfahren, und ein Klick öffnet sie.
    """
    grund = grund or SURFACE
    lbl = tk.Label(eltern, text=text, bg=grund, fg=ACCENT, font=fenster.f_small,
                   anchor='w', cursor='hand2')
    lbl.pack(fill='x', pady=(4, 0))

    def oeffnen(_=None):
        # Die saubere Umgebung und der Rückfall auf `xdg-open` stecken in
        # `paths.open_in_browser` — an EINER Stelle, damit nicht die Hälfte der
        # Verweise sie hat und die andere nicht.
        try:
            geklappt = paths.open_in_browser(ziel)
        except Exception as ausnahme:
            errors.record('pages.adresse', ausnahme, ziel)
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


def _switch(window, parent, key, default):
    """Ein An/Aus-Schalter, der sofort schreibt — es gibt keinen Speichern-Knopf.

    ⚠ Derselbe Schiebeschalter wie überall (`toggle_switch`) — ein Wort
    „an"/„aus" auf einem Kasten sähe nicht wie ein Schalter aus."""
    from . import paths
    from .main_window import toggle_switch

    def flip():
        new_value = not paths.setting_bool(key, default)
        paths.set_setting(key, new_value)
        window.say(t('e_an') if new_value else t('e_aus'))
        return new_value

    switch = toggle_switch(parent, paths.setting_bool(key, default), flip)
    switch.pack()
    return switch


def _detection(fenster, rahmen):
    from . import catalog as katalog_modul, paths, phrases
    from .main_window import toggle_switch
    _heading(fenster, rahmen, t('hf_erkennung'), t('s_er_lead'))
    innen = _scroll_area(rahmen)

    ziel = _setting_row(fenster, innen, t('s_er_takt'), t('s_er_takt_h'))
    reihe = tk.Frame(ziel, bg=BG)
    reihe.pack()
    from .main_window import round_entry
    zahl = round_entry(reihe, None, fenster.f_small, theme.FIELD, LINE, ACCENT, FG,
                       width=5, justify='right')
    zahl.insert(0, str(paths.setting_int('pruefintervall_sekunden', 3, 1, 60)))
    zahl.holder.pack(side='left')
    tk.Label(reihe, text=t('s_er_sek'), bg=BG, fg=SUB,
             font=fenster.f_small).pack(side='left')

    def takt_merken(_=None):
        try:
            paths.set_setting('pruefintervall_sekunden',
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
        saetze, woher = phrases.collect()
        gefunden = ' · '.join(str(x) for x in (saetze or [])) or '—'
    except Exception as ausnahme:
        errors.record('pages.erkennung.phrases', ausnahme)
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
            errors.record('pages.erkennung.katalog', ausnahme)
            fenster.say(t('s_er_kat_weg'))

    _button(fenster, ziel, t('s_er_kat_jetzt'), katalog_neu).pack()

    # ⭐ Baupläne ohne bekannten Weg.
    #
    # ⛔ Standard **aus**: Der Fortschritt bleibt damit die Zahl, die
    # Bestandsnutzer kennen. Wer umschaltet, sieht statt 738 alle 1591 — und
    # sein Prozentsatz fällt entsprechend. Deshalb steht in der Beschreibung,
    # was mit der Zahl passiert, bevor jemand klickt.
    #
    # ⚠ Der Schalter wirkt beim nächsten Laden des Katalogs (`catalog.load()`),
    # nicht mitten in einer offenen Seite — deshalb die Ansage in der Statuszeile.
    ziel = _setting_row(fenster, innen, t('s_er_alle'), t('s_er_alle_h'))

    # ⚠⚠ **`toggle_switch` ruft OHNE Argument und will den neuen Zustand
    # zurück** (siehe seinen Docstring). Eine Funktion mit Parameter läuft in
    # einen TypeError, den die Fehler-Haken schlucken — der Schalter sieht
    # dann aus wie kaputt und schreibt nichts.
    def alle_um():
        neu = not paths.setting_bool(katalog_modul.SETTING_ALL, False)
        paths.set_setting(katalog_modul.SETTING_ALL, neu)
        fenster.say(t('s_er_alle_hin') if neu else t('s_er_alle_weg'))
        # ⚠ Der Katalog wird beim **Laden** gefiltert. Ohne Neuaufbau bliebe
        # die Seite stehen, und Liste wie Fortschritt zeigten weiter die alte
        # Zahl — dieselbe Falle wie bei „Spielzeit zeigen".
        fenster.root.after(60, fenster.rebuild)
        return neu

    toggle_switch(ziel,
                  paths.setting_bool(katalog_modul.SETTING_ALL, False),
                  alle_um).pack()

    _account_rows(fenster, innen)

    # ⚠⚠ **Kein eigener „Protokolle neu lesen"-Knopf hier.** Unter „Bestand"
    # gibt es „Protokolle erneut einlesen" — der ignoriert den Lesestand,
    # geht jede Sicherung UND die laufende `Game.log` durch, wirkt **sofort**
    # und sagt hinterher, was dabei herauskam. Zwei Knoepfe fuer eine Sache
    # sind schlimmer als einer: Wer den schwaecheren erwischt, glaubt, das
    # Werkzeug koenne es nicht.


def _account_rows(window, inner):
    """Eigener Account und das Aufräumen fremder Baupläne.

    ⭐ Wer zwei Accounts auf einem Rechner spielt, hat beider Protokolle in
    `logbackups/`. Gezählt wird nur der eigene (`logsource.own_account`);
    hier lässt er sich umstellen, und was vorher schon hereingerutscht ist,
    lässt sich entfernen — nach Rückfrage, weil ein Rest Unsicherheit bleibt
    (siehe `logsource.foreign_only_blueprints`).
    """
    from . import collection as stock_file, logsource, overlay, paths
    from .main_window import round_select
    target = _setting_row(window, inner, t('s_er_acc'), t('s_er_acc_h'))
    counts, own = {}, None
    try:
        counts = logsource.accounts_in_logs()
        own = logsource.own_account()
    except Exception as error:
        errors.record('pages.erkennung.accounts', error)

    foreign_target = _setting_row(window, inner, t('s_er_fremd'),
                                  t('s_er_fremd_h'), wide=True)
    info = tk.Label(foreign_target, text=t('s_er_fremd_sucht'), bg=BG, fg=SUB,
                    font=window.f_small, anchor='w', justify='left',
                    wraplength=560)
    info.pack(fill='x', pady=(6, 0))
    button_slot = tk.Frame(foreign_target, bg=BG)
    button_slot.pack(anchor='w', pady=(6, 0))

    def show(affected):
        for child in button_slot.winfo_children():
            child.destroy()
        if not affected:
            info.configure(text=t('s_er_fremd_keine'))
            return
        preview = ', '.join(affected[:8]) + (' …' if len(affected) > 8 else '')
        info.configure(text=t('s_er_fremd_n') % (len(affected), preview))

        def remove():
            listing = '\n'.join('· ' + n for n in affected[:25])
            if len(affected) > 25:
                listing += '\n…'
            if not _ask(window, t('s_er_fremd'),
                        t('s_er_fremd_frage') % (len(affected), listing)):
                return
            if overlay.request_remove(affected):
                for child in button_slot.winfo_children():
                    child.destroy()
                info.configure(text=t('s_er_fremd_sucht'))
                # Der Watcher arbeitet im nächsten Takt — danach neu zählen.
                info.after(8000, search)

        _button(window, button_slot, t('s_er_fremd_weg'), remove,
                danger=True).pack(side='left')

    def search():
        """Im Hintergrund: 180 Protokolle ganz zu lesen dauert Sekunden."""
        try:
            if not info.winfo_exists():
                return
        except Exception:
            return
        info.configure(text=t('s_er_fremd_sucht'))

        def work():
            affected = []
            try:
                names = logsource.foreign_only_blueprints()
                entries = stock_file.load().get('bauplaene') or {}
                # Nur, was aus einem Protokoll kam — dasselbe Maß wie beim
                # Entfernen im Watcher (`_remove_foreign_now`).
                affected = [n for n in names
                            if (entries.get(stock_file.norm(n)) or {})
                            .get('quelle') in ('log', 'nachlese')]
            except Exception as error:
                errors.record('pages.erkennung.fremd', error)
            _from_thread(info, lambda: show(affected))

        threading.Thread(target=work, daemon=True).start()

    if counts or own:
        chosen = own or logsource.ALL_ACCOUNTS
        names = set(counts)
        if own and own != logsource.ALL_ACCOUNTS:
            names.add(own)
        entries = [(logsource.ALL_ACCOUNTS, t('s_er_acc_alle'))]
        entries += [(a, t('s_er_acc_n') % (a, counts.get(a, 0)))
                    for a in sorted(names, key=str.lower)]

        def choose(value):
            if value == (paths.setting(logsource.ACCOUNT_SETTING) or ''):
                return
            paths.set_setting(logsource.ACCOUNT_SETTING, value)
            label = (t('s_er_acc_alle') if value == logsource.ALL_ACCOUNTS
                     else value)
            window.say(t('s_er_acc_sagen') % label)
            overlay.account_changed()
            search()

        round_select(target, entries, chosen, choose, window.f_small).pack()
    else:
        tk.Label(target, text=t('s_er_acc_keiner'), bg=BG, fg=SUB,
                 font=window.f_small, wraplength=260,
                 justify='left').pack()
    search()


def _diagnostics(fenster, rahmen):
    from . import paths
    from .main_window import toggle_switch
    _heading(fenster, rahmen, t('hf_diagnose'), t('s_di_lead'))
    innen = _scroll_area(rahmen)

    # ⭐ Wer meldet? Steht ÜBER dem Bericht, damit man sieht, was mitgeht.
    # Ohne Absender lässt sich ein Bericht niemandem zuordnen, und
    # Rückfragen laufen ins Leere.
    #
    # ⚠ **Freiwillig und nie vorausgefüllt** — auch nicht mit dem
    # Benutzernamen des Systems. Das Werkzeug sammelt sonst nichts über den
    # Nutzer („no telemetry"); ein heimlich mitgeschickter Name wäre ein
    # Wortbruch.
    melder_var = tk.StringVar(rahmen, value=(paths.setting('melder_name') or ''))
    ziel_melder = _setting_row(fenster, innen, t('s_melder'), t('s_melder_h'))
    from .main_window import round_entry
    melder_feld = round_entry(ziel_melder, melder_var, fenster.f_small,
                              theme.FIELD, LINE, ACCENT, FG,
                              placeholder=t('s_pl_melder'))
    melder_feld.holder.pack(fill='x', pady=(8, 0))

    # ⭐⭐ **Ein Feld für die Meldung selbst — direkt unter dem Namen.** Ohne
    # eigenes Feld landet die Meldung im **Namensfeld**, weil es das einzige
    # ist, in das man etwas tippen kann.
    #
    # ⚠⚠ **Mehrzeilig und über die volle Breite, unter der Erklärung.** In
    # einem einzeiligen Feld neben dem Text sähe man bei zwei Sätzen nur das
    # Ende und könnte vor dem Absenden nicht mehr nachlesen, was man meldet.
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
                                   theme.FIELD, LINE, ACCENT, FG, rows=4)
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
        text = report.build(version=fenster.version, root=fenster.root)
    except Exception as ausnahme:
        errors.record('pages.diagnose', ausnahme)

    from .main_window import round_frame
    kasten = round_frame(innen, theme.FIELD, LINE, radius=8, base_color=BG)
    kasten.holder.pack(fill='both', expand=True)
    # ⚠ `highlightthickness` steht bei Text und Entry auf 1 und wird auf dem
    # Mac als helle Linie gezeichnet — im runden Kasten sah das aus wie ein
    # zweiter, eckiger Rahmen. `relief='flat'` und `bd=0` schalten das NICHT ab.
    feld = tk.Text(kasten, bg=theme.FIELD, fg=FG, font=('Consolas', 10),
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
            frisch = report.build(version=fenster.version,
                                   root=fenster.root,
                                   message=meldung_text())
        except Exception as ausnahme:
            errors.record('pages.diagnose_melder', ausnahme)
            return
        feld.configure(state='normal')
        feld.delete('1.0', 'end')
        feld.insert('1.0', frisch)
        feld.configure(state='disabled')

    def melder_uebernehmen(*_):
        """Den Namen sichern — er gilt dauerhaft, anders als die Meldung."""
        neu_wert = melder_var.get().strip()
        if neu_wert != (paths.setting('melder_name') or ''):
            paths.set_setting('melder_name', neu_wert)
        _bericht_neu()

    melder_feld.bind('<FocusOut>', melder_uebernehmen)
    melder_feld.bind('<Return>', melder_uebernehmen)
    # ⚠ Die Meldung wird **nicht** gespeichert — sie gehört zu diesem einen
    # Bericht. Deshalb nur den Text neu bauen, nichts ablegen.
    # ⚠ **Kein `<Return>`.** In einem mehrzeiligen Feld ist die Eingabetaste
    # der Zeilenumbruch, nicht das Bestätigen. Wer
    # hier bindet, nimmt dem Melder die Absätze weg — bei einer
    # Fehlerbeschreibung genau das Falsche.
    #
    # Gebraucht wird sie auch nicht: `<FocusOut>` feuert, sobald man irgendwo
    # hin klickt, und jeder der drei Knöpfe holt sich den Text ohnehin frisch
    # über `aktueller_bericht()`.
    meldung_feld.bind('<FocusOut>', lambda _=None: _bericht_neu())

    # ⚠⚠ **Beim erneuten Öffnen den Bericht neu bauen.** Er enthält die eigene
    # Bauplan-Zahl, und die ändert sich beim Spielen. Ohne das stünde in einem
    # Bericht vom Abend der Bestand vom Morgen — eine Zahl, die längst anders
    # ist.
    #
    # ⚠ **Diese Seite wird bewusst NICHT verworfen** wie die übrigen Seiten mit
    # Bestandszahlen (`main_window.STOCK_PAGES`). Ein Neubau würde das
    # Meldungsfeld leeren — jemand tippt seine Fehlerbeschreibung, wechselt
    # kurz auf eine andere Seite, um etwas nachzusehen, und der Text ist weg.
    # Genau auf dieser Seite darf das am wenigsten passieren.
    fenster.on_show['diagnose'] = _bericht_neu

    # ⭐ **Die Zusicherung steht zwischen Bericht und Knöpfen** — genau dort,
    # wo man sich fürs Absenden entscheidet. *Unter* der Knopfreihe stünde
    # sie hinter dem Klick und könnte am unteren Rand wegfallen.
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

        ⚠⚠ **Nicht der Bericht vom Öffnen der Seite.** Wer seinen Namen
        eintippt, sieht ihn sofort im Kasten (`melder_uebernehmen` zeichnet
        ihn neu) — der beim Öffnen gebaute `text` enthielte ihn nicht, und
        hinaus ginge „Von: nicht angegeben".

        Deshalb kommt der Text **aus dem Kasten**. Der Satz darunter
        verspricht „Du siehst vorher genau, was du verschickst" — dann muss
        auch genau das verschickt werden. Und der Name wird vorher übernommen,
        falls das Feld noch den Tastaturfokus hat.
        """
        melder_uebernehmen()
        return feld.get('1.0', 'end-1c')

    def _meldung_verbraucht():
        """Das Feld „Was ist passiert?" leeren — der Satz ist raus.

        ⚠⚠ **Alle drei Knöpfe, nicht nur „Absenden".** „Angaben kopieren"
        und „Melden" geben den Bericht genauso weiter — beim einen in die Zwischenablage, beim anderen
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
                errors.record('pages.diagnose_meldung_leeren', ausnahme)

    def melden():
        if report.open_issue(aktueller_bericht()):
            fenster.say(t('s_di_browser_ok'))
            _meldung_verbraucht()
        else:
            fenster.say(t('s_di_browser_weg'))

    def kopieren():
        if report.to_archive(aktueller_bericht(), fenster.root):
            fenster.say(t('s_di_kopiert'))
            _meldung_verbraucht()

    # ⚠ Gemerkt, sobald der Spieler den Haken einmal setzt — wer
    # Bilder und Berichte schicken will, soll das nicht jedes Mal neu bestätigen.
    bestaetigt = {'an': paths.setting_bool(BERICHT_ZUSTIMMUNG, False)}

    def absenden():
        """Auf Knopfdruck an den Entwickler — nach einem Haken, ohne Rückfrage.

        ⚠ Der Weg für alle, die nicht basteln wollen. Kopieren und in Discord
        einfügen scheitert daran, dass der Bericht zu lang ist und man wissen
        muss, wohin damit.

        ⚠⚠ **Zustimmung per Haken, nicht per Rückfrage-Fenster** — ein
        Fenster für den Bericht und ein zweites für die Scan-Bilder wären
        umständlich. Im Bericht
        und in den Bildern steckt nichts Heikles (Namen und Pfade sind
        herausgenommen, die Bilder zeigen nur die Zahl). Ohne Haken wird nichts
        gesendet; der Knopf sagt dann, was fehlt.
        """
        if not bestaetigt['an']:
            fenster.say(t('s_di_erst_bestaetigen'))
            return
        anzahl = 0
        try:
            from . import signature_scan
            anzahl = len(signature_scan.samples())
        except Exception as ausnahme:
            errors.record('pages.diagnose_scanbilder', ausnahme)
        anhaenge = []
        if anzahl:
            try:
                archiv = signature_scan.sample_archive()
                if archiv:
                    # ⭐ Der Dateiname trägt den Melder, sonst heißen im
                    # Download-Ordner alle Archive gleich.
                    anhaenge.append((signature_scan.archive_name(), archiv,
                                     'application/zip'))
            except Exception as ausnahme:
                errors.record('pages.diagnose_scanbilder', ausnahme)
        fenster.say(t('s_di_ab_laeuft'))
        fenster.root.update_idletasks()
        geklappt, grund = report.submit(aktueller_bericht(), fenster.version,
                                        anhaenge)
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
    # ⚠ **Immer zeigen, auch ohne eingebautes Ziel** (das trifft nur den
    # Start aus dem Quellcode). Ein Knopf, der fehlt, sieht aus wie ein
    # Fehler; einer, der beim Drücken sagt, was ihm fehlt, erklärt sich.
    # ⚠ **Drei Knöpfe, kein „Als Datei speichern".** Eine Datei erzeugt Arbeit
    # statt sie abzunehmen — wer den Bericht abschickt oder kopiert, ist
    # fertig; wer ihn als Datei ablegt, muss ihn danach noch irgendwohin
    # bringen.
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

    # ⭐ EIN Haken als Zustimmung, direkt unter „Absenden" — statt zweier
    # Rückfrage-Fenster an verschiedenen Stellen. Gibt es
    # angelernte Scan-Bilder, nennt der Haken sie mit, und sie gehen mit.
    from .main_window import toggle_switch as _toggle
    try:
        from . import signature_scan as _scan_module
        _anzahl_scan = len(_scan_module.samples())
    except Exception:
        _anzahl_scan = 0
    zustimmung = tk.Frame(reihe.master, bg=BG)
    zustimmung.pack(fill='x', pady=(6, 0))

    def zustimmung_umlegen():
        bestaetigt['an'] = not bestaetigt['an']
        paths.set_setting(BERICHT_ZUSTIMMUNG, bestaetigt['an'])
        return bestaetigt['an']

    _toggle(zustimmung, bestaetigt['an'], zustimmung_umlegen).pack(side='left')
    tk.Label(zustimmung,
             text=(t('s_di_zustimmung_bilder') % _anzahl_scan if _anzahl_scan
                   else t('s_di_zustimmung')),
             bg=BG, fg=FG, font=fenster.f_small, anchor='w', justify='left'
             ).pack(side='left', padx=8)

    # ⚠⚠ **Kein vierter Knopf** — siehe die Begründung oben. Stattdessen ein
    # Satz, der auf den Discord-Knopf verweist, den die Seitenleiste ohnehin
    # führt.
    #
    # **Warum er hier steht:** Auf GitHub ist der Knopf „New issue" ohne
    # Anmeldung ausgegraut — von außen sieht das wie eine Sperre aus, und
    # wer melden will, hört an dieser Stelle auf. Der Weg über Discord braucht kein Konto bei
    # GitHub, und genau das muss **auf dieser Seite** stehen, nicht nur in der
    # Anleitung.
    _body_text(innen, t('s_di_ohne_github'), fenster.f_small, color=SUB,
               fill='x', pady=(8, 0))

    ziel = _setting_row(fenster, innen, t('s_di_mit'), t('s_di_mit_h'))

    def mitschreiben_um():
        neu_wert = not paths.setting_bool('fehler_mitschreiben', True)
        paths.set_setting('fehler_mitschreiben', neu_wert)
        return neu_wert

    toggle_switch(ziel, paths.setting_bool('fehler_mitschreiben', True),
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
        errors.record('pages.herstellung', ausnahme)
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
    # `counts()` gibt sie zurück: Ein Bauplan, dessen Name mehrere Gegenstände meint (Idris- und
    # Reclaimer-Kraftwerk, BroadSpec in zwei Größen), zählt bewusst nicht als
    # „sicher" — richtig so, ein falsch zugeordneter Bauplan wäre schlimmer.
    #
    # Ohne sie stünde oben eine Zahl, die **kleiner ist als der eigene
    # Bestand**, und nichts sagte warum — der Hinweis (`s_he_unklar`) steht
    # sonst erst am aufgeklappten Eintrag, wo man ihn nur findet, wenn man
    # schon weiß, wonach man sucht.
    if unklar:
        tk.Label(kopf, text=t('s_he_dazu_unklar') % unklar, bg=BG, fg=SUB,
                 font=fenster.f_small).pack(side='left')

    from .main_window import round_bar, round_entry
    round_bar(innen, 9, sicher / float(gesamt or 1), BG, theme.HOVER,
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
    suche_var = tk.StringVar(rahmen, value=gesprungen)
    fenster.crafting_search = ''
    ziel_suche = _setting_row(fenster, innen, t('s_he_suche'), '')
    suchfeld = round_entry(ziel_suche, suche_var, fenster.f_small, theme.FIELD,
                           LINE, ACCENT, FG, placeholder=t('s_pl_herstellung'))
    suchfeld.holder.pack(fill='x', pady=(4, 12))
    # ⚠ Gleiches Bedienelement wie beim Bergbau. Zwei Suchfelder, die sich
    # unterschiedlich verhalten, sind schlimmer als eines ohne Kreuz.
    def _herst_frisch():
        """Beim erneuten Aufrufen ohne Filter anfangen.

        ⚠⚠ **Nur wenn wirklich etwas gesetzt war.** Sonst baut jeder Wechsel
        auf die Herstellungs-Seite die 1597 Zeilen neu auf, ohne dass sich
        etwas ändert — dieselbe Bremse wie in der Bauplan-Liste
        (`collection_window._fein_leeren`).
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
            suche_var.set(neuer_sprung)
            # Sofort zeichnen — `trace_add` wartet aufs Ende des Tippens.
            zeichnen()
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
    # ⚠ Ein Bedienkonzept für das ganze Programm (siehe `_filter_bar`).
    #
    # ⚠ Die Werte kommen aus den **vorhandenen** Einträgen, nicht aus einer
    # festen Liste. Bringt ein Patch eine neue Waffenart, steht sie am nächsten
    # Tag im Feld, ohne dass jemand etwas nachträgt.
    wahl = {'art': '', 'unterart': '', 'hersteller': '', 'zustand': '',
            'material': ''}

    # ⚠⚠ **Dieselbe Gliederung wie in der Bauplan-Liste** — es sind dieselben
    # Baupläne. Beide Seiten fragen dasselbe Modul — wer hier eine
    # eigene Einteilung baute, hätte zwei Wahrheiten über dieselben Daten.
    from . import categories as kat_modul
    from . import catalog as kat_daten

    _kat_arten = {}
    try:
        for _k, _v in (kat_daten.load().get('bauplaene') or {}).items():
            _kat_arten[herst_modul._key(_v.get('n') or '')] = _v.get('a') or ''
    except Exception as ausnahme:
        errors.record('pages.crafting.katalog', ausnahme)

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
        „Magazin" — die lange Liste, die zwei Ebenen vermeiden sollen."""
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
        # ein Vergleich gegen die rohe Liste träfe nie zu, und jede gewählte
        # Unterart würde sofort wieder geleert.
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
    # nicht erst noch einmal klicken (Drei-Klick-Regel).
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

        # ⭐ **Ohne Eingabe steht hier keine Liste.** 1597 Baupläne
        # untereinander scrollt niemand durch; dafür sind die Auswahlfelder
        # darüber da.
        #
        # ⚠ Der Hinweis darf nicht fehlen: Eine Seite, die leer aufgeht und
        # nichts sagt, sieht kaputt aus.
        if not text and not any(wahl.values()):
            _body_text(liste_rahmen, t('s_he_erst_waehlen'), fenster.f_small,
                        fill='x')
            return

        # ⭐⭐ **Auch nach der ZUTAT suchen.** Nur auf Bauplan-Namen bekäme
        # wer „ric" tippt, um zu sehen, was aus Riccite wird, Zufallstreffer
        # wie „Lo*ric*a" — und nie die Baupläne, die Riccite wirklich
        # brauchen.
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

    suche_var.trace_add('write', after_typing(rahmen, zeichnen))
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

    ⚠⚠ **Eine nackte Zahl ist keine Auskunft.** In einer Zeile mit
    SCU-Mengen und Entfernungen sagt eine blanke Zahl gar nichts.
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

    ⚠⚠ **Erst nachsehen, DANN die Seite wechseln.** Stünde `open_page` ganz
    oben, landete wer einen Auftrag ohne Baupläne anklickt trotzdem in der
    Liste — mit der alten Ansicht und einer Meldung darunter. Im
    Auftrags-Protokoll bringen rund 40 % der Aufträge keinen Bauplan — dort
    ist das der Normalfall.
    """
    try:
        titel = (titel or '').strip()
        if not titel:
            return
        # Gegen den Katalog fragen, ohne die Seite anzufassen.
        #
        # ⛔⛔ **Über `catalog.blueprints_for_contract`, nicht mit einem eigenen
        # Vergleich.** Ein wörtlicher Titelvergleich gegen `q['auftrag']`
        # trifft alles aus der eigenen Liste und **nichts** aus dem Spiel: In
        # den Herkunftsdaten steht `'Stop Rival Attack at [LOCATION]'`, im
        # Spiel `'Stop Rival Attack at Asteroiden Bergbaubasis'`.
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
        errors.record('pages.zum_auftrag', ausnahme)


def _open_issue_council(window, title):
    """Die Issue-Council-Suche nach diesem Auftrag im Browser öffnen.

    ⚠ Über `paths.open_in_browser`, nie `webbrowser.open()` direkt — siehe die
    Begründung dort.
    """
    try:
        from . import issue_council
        address = issue_council.url_for(title)
        if not paths.open_in_browser(address):
            window.say(t('s_ub_auf_nein') % address)
    except Exception as exception:
        errors.record('pages.issue_council', exception)


def _to_kind(fenster, art):
    """Vom Bauplan-Fortschritt zur Liste, gefiltert auf diese Kategorie."""
    try:
        fenster.jump_to('liste')
        seite = getattr(fenster, 'stock_page', None)
        if seite is not None and seite.zur_art(art):
            return
        fenster.say(t('s_fo_art_nichts') % art)
    except Exception as ausnahme:
        errors.record('pages.zur_art', ausnahme)


def _to_blueprint(fenster, name):
    """Von der Herstellung zur Bauplan-Liste — mit aufgeschlagener Herkunft."""
    try:
        fenster.jump_to('liste')
        seite = getattr(fenster, 'stock_page', None)
        if seite is not None and seite.zum_bauplan(name):
            return
        fenster.say(t('s_he_woher_nichts'))
    except Exception as ausnahme:
        errors.record('pages.zum_bauplan', ausnahme)
        fenster.say(t('s_he_woher_nichts'))


def _routes(fenster, rahmen):
    """Der Reiter „Routen": Wo stehe ich, wieviel passt rein — was lohnt sich?

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
    # ⚠ 120: Das ist der Laderaum der Freelancer MAX. Ein Standardwert soll einem echten Schiff entsprechen und
    # nicht geraten sein.
    scu_var = tk.StringVar(rahmen, value='120')
    geld_var = tk.StringVar(rahmen, value='500000')
    ortsuche = tk.StringVar(rahmen)

    # ⚠⚠ **Die Eingaben bleiben stehen, nur das Ergebnis rollt.** Lägen sie
    # **in** der Rollfläche, verlöre wer zu den Fahrten hinunterrollt
    # Startort, Frachtraum und Schiff aus dem Bild.
    #
    # ⚠ Wie im Laden-Reiter: Erst alles Feste packen, **danach** die
    # rollende Fläche.
    kopf = tk.Frame(rahmen, bg=BG)
    kopf.pack(fill='x', side='top', padx=24, pady=(4, 0))

    # ---------------------------------------------------- Wo stehe ich?
    tk.Label(kopf, text=t('s_rt_wo'), bg=BG, fg=SUB,
             font=fenster.f_small, anchor='w').pack(fill='x')

    # ⭐ **Dropdown UND Suchfeld — wie überall im Werkzeug.** Wer den Namen
    # weiß, tippt; wer ihn nicht weiß, klappt das System auf und sieht alle
    # Handelsposten darin.
    ortzeile = tk.Frame(kopf, bg=BG)
    ortzeile.pack(fill='x')
    # ⚠ Über `round_entry` — damit auch hier das X im Feld sitzt (Standard).
    from .main_window import round_entry as _feld_rund
    ortfeld = _feld_rund(ortzeile, ortsuche, fenster.f_base, SURFACE, LINE,
                         ACCENT, FG, placeholder=t('s_rt_wo_platz'))
    ortfeld.holder.pack(side='left', fill='x', expand=True)
    # ⚠⚠ **Nicht gepackt, solange leer.** Ein geleerter Rahmen behält seine
    # Höhe — gemessen 920 px bei null Kindern, oben klafft dann ein großer
    # Leerraum. Gilt für diesen Rahmen und den für die Schiffe.
    ortvorschlag = tk.Frame(kopf, bg=BG)

    def _ortliste_leeren():
        for kind in ortvorschlag.winfo_children():
            kind.destroy()
        ortvorschlag.pack_forget()

    def _ortliste_zeigen():
        # ⚠⚠ **`after=` — sonst landet die Liste ganz unten.** Ein `pack()`
        # ohne Angabe hängt sich ans Ende des Rahmens; weil die Liste beim
        # Leeren ausgepackt wird, stünde sie sonst unter den Schaltern, weit
        # weg vom Suchfeld.
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
        # ⚠ Der Hinweis nennt ein BEISPIEL, nicht die Beschriftung darüber —
        # „Frachtraum (SCU)" zweimal zu sagen hilft niemandem.
        zahlfeld = _feld_rund(spalte, var, fenster.f_base, SURFACE, LINE,
                              ACCENT, FG, width=breite, placeholder=beispiel)
        zahlfeld.holder.pack(anchor='w')

    # ⭐⭐ **Schiff wählen statt Zahl tippen — als Suchfeld, nicht als Fenster.**
    #
    # ⚠ Ein eigenes Auswahl-Fenster passt nicht zum Rest. Suchfeld mit
    # Vorschlagsliste ab zwei Buchstaben — dasselbe Muster wie beim Ortsfeld
    # darüber und in den anderen Menüs des Werkzeugs.
    schiff_rahmen = tk.Frame(kopf, bg=BG)
    schiff_rahmen.pack(fill='x', pady=(10, 0))
    tk.Label(schiff_rahmen, text=t('s_rt_schiff'), bg=BG, fg=SUB,
             font=fenster.f_small, anchor='w').pack(fill='x')
    schiffsuche = tk.StringVar(rahmen)
    schifffeld = _feld_rund(schiff_rahmen, schiffsuche, fenster.f_base,
                            SURFACE, LINE, ACCENT, FG,
                            placeholder=t('s_rt_schiff_platz'))
    schifffeld.holder.pack(fill='x')
    # ⭐⭐ **Eine Werft-Auswahl neben dem Suchfeld.** Ein Suchfeld, das
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

    schiffsuche.trace_add('write', after_typing(rahmen,
                                                _schiffvorschlaege))

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
            # ⚠⚠ **Das Suchfeld wird mit geleert.** Steht im Feld noch
            # „Drake Ironclad" und die neue Werft ist Argo, ergibt beides
            # zusammen nichts, und die Liste bliebe leer.
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

    # ⭐⭐ **Die beste Route überhaupt, egal von wo nach wo.** Das braucht die
    # Fahrten **aller** 184 Handelsposten — rund
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

    # ⭐ **Zurücksetzen.** Nach ein paar Versuchen stehen hier Startort, System,
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
        # ⚠ **Auch die Null wird genannt.** Bliebe die Zeile leer, solange
        # noch nichts gesammelt ist, stünde neben dem Knopf gar nichts, wo
        # bei anderen „184 von 184 Handelsposten" steht. Wer
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
                _TK_CALLS.put(zeigen)
            try:
                routen_modul.fetch_all(progress=melden)
            except Exception as ausnahme:
                errors.record('pages.routes.alle_holen', ausnahme)

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
            _TK_CALLS.put(fertig)

        threading.Thread(target=arbeit, daemon=True).start()

    ueberall_knopf.bind('<Button-1>', _ueberall_suchen)
    ueberall_knopf.bind('<Enter>',
                        lambda _=None: ueberall_knopf.configure(fg=ACCENT))
    ueberall_knopf.bind('<Leave>',
                        lambda _=None: ueberall_knopf.configure(fg=SUB))

    # ⚠⚠ **Die Umschalter stehen oben, nicht im Ergebnis.** Unter der
    # Ortswahl erschienen sie erst, wenn schon ein Ort gewählt ist — wer die
    # Seite zum ersten Mal öffnet, sähe sie nie.
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
        # ⚠ Wieviele Stationen — Routen von A nach B, von B weiter nach C,
        # von C nach A. Genau das sind diese beiden Reihen.
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

        # ⚠⚠ **Die globale Bestenliste.** Ohne gewählten Ort bräche
        # `_zeichnen()` sonst ab, und nach dem Sammeln aller Handelsposten
        # stünde weiter „Tippe oben ein, wo du gerade bist" da.
        if zustand.get('modus') == 'ueberall':
            tk.Label(ergebnis, text=t('s_rt_ueberall_titel'), bg=BG, fg=FG,
                     font=fenster.f_bold, anchor='w').pack(fill='x',
                                                           pady=(0, 6))
            # ⚠⚠ **Die Schalter gelten auch hier** — sonst färbten sie sich
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
        gewählten Ort und die besten Ketten überall.
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
        # Ohne ihn stünde da „120 SCU Copper → Rat's Nest", und niemand sähe,
        # wo man dafür einkauft.
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
        # ⚠⚠ **Der gewählte Ort bleibt im Feld stehen.** Geleert stünde oben
        # „Wo stehst du gerade?" über einem leeren Kasten, und es sähe aus,
        # als sei nichts ausgewählt.
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
                errors.record('pages.routes.holen', ausnahme)

            def fertig():
                zustand['laeuft'] = False
                try:
                    if ergebnis.winfo_exists():
                        _zeichnen()
                except tk.TclError:
                    pass
            _TK_CALLS.put(fertig)

        threading.Thread(target=arbeit, daemon=True).start()

    def _ortvorschlaege(*_a):
        if zustand.get('stumm'):
            return
        _ortliste_leeren()
        text = ortsuche.get().strip().lower()
        # ⚠⚠ **Ohne Eingabe reicht ein Klick ins Feld** — auch ohne rechts
        # gewähltes System. Wer ein Auswahlfeld anklickt, erwartet eine
        # Auswahl, keine Vorbedingung.
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
        # Sonst stünde achtmal „Seraphim Station · Stanton" untereinander, und
        # niemand könnte sagen, welche Zeile welche ist.
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

    ortsuche.trace_add('write', after_typing(rahmen, _ortvorschlaege))
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
                errors.record('pages.routes.schiffe', ausnahme)

            def fertig():
                try:
                    if werft_rahmen.winfo_exists():
                        _werft_bauen()
                except tk.TclError:
                    pass
            _TK_CALLS.put(fertig)

        threading.Thread(target=arbeit, daemon=True).start()

    _schiffe_sicherstellen()


def _route_header(fenster, eltern):
    """Die Spaltenüberschrift über der ersten Fahrt.

    ⚠⚠ **Ohne sie ist die größte Zahl mehrdeutig.** Bei
    „1.917.234 aUEC · 69 SCU Atlasium" ist nicht klar, ob das Einsatz oder
    Gewinn ist. Eine Zahl ohne Spaltennamen lädt zum Falschlesen ein.

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
    # die Zeile für sich liest (und das tut man in einer Tabelle), sähe einen
    # Pfeil ins Nichts.
    if fahrt.get('startname'):
        tk.Label(zeile, text='  ' + t('s_rt_ab') % fahrt['startname'],
                 bg=SURFACE, fg=FG, font=fenster.f_small,
                 anchor='w').pack(side='left')
    tk.Label(zeile, text='  →  ' + t('s_rt_nach')
             % (fahrt.get('zielname') or '?'), bg=SURFACE,
             fg=SUB, font=fenster.f_small, anchor='w').pack(side='left')

    # ⭐⭐ **Der Einsatz gehört dazu — ohne ihn ist der Gewinn eine Behauptung.**
    # Bei „586.500 aUEC · 1 SCU Osoian Hides" stimmt die Zahl (Einkauf
    # 283.500, Verkauf 870.000 je SCU) — aber dass man dafür erst
    # **283.500 aUEC hinlegen** muss, stünde sonst nirgends.
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
# Gemessen: Schiffswaffen 96, FPS-Waffen 168, dazu 6 ohne
# Unterart. Die sechs bleiben schlicht „Waffen" — geraten wird nicht.
SHIP_WEAPONS = frozenset(('laser', 'ballistic', 'distortion', 'neutron',
                           'tachyon'))
FPS_WEAPONS = frozenset(('pistol', 'rifle', 'sniper', 'smg', 'shotgun', 'lmg'))

# ⚠ Notdeckel für eine einzelne Warengruppe. Sie wird sonst **vollständig**
# gezeigt — die größte echte Gruppe hat 201 Einträge. Die Zahl schützt nur
# davor, dass ein Ausreißer in fremden Daten Tausende Zeilen baut.
EMERGENCY_CAP = 400

# ⚠⚠ **Wie viele Zeilen je Warengruppe, wenn mehrere nebeneinanderstehen.**
# Bei 176 Treffern in vier Gruppen füllte sonst allein die erste Gruppe die
# ganze Liste — die anderen drei sähe man nie. Ein Deckel **je Gruppe** zeigt
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
    liefert etwa `Golden Medmon` als Goldpreis.
    """
    from . import crafting as herst_modul, shops as laden_modul

    _heading(fenster, rahmen, t('hf_laeden'), t('s_ld_lead'))

    suche = tk.StringVar(rahmen)
    gewaehlt = {'name': '', 'kennung': ''}
    laeuft = {'ja': False}
    # ⚠⚠ **Die Reihenfolge ist die Kaskade.** Jedes Menü zeigt nur, was zur
    # Auswahl in den Menüs **davor** passt — und beim Wechsel fällt weg, was
    # danach nicht mehr passt.
    # ⚠⚠ **Der Hersteller hat kein Menü.** Wer Teile sucht, die in sein
    # Schiff passen, sucht nicht nach Hersteller — was hineinpasst,
    # entscheidet die Größe. Der Hersteller ist eine Angabe zum Lesen, kein Suchweg — er steht
    # an der Zeile und wird von der Suche mit gefunden, hat aber kein Menü.
    FILTER_FOLGE = ('bereich', 'gruppe', 'groesse', 'klasse', 'guete')
    # ⚠ Muss vor `_teile()` stehen — die Funktion fragt ihn ab, um während des
    # Katalog-Abrufs nicht die falsche Liste zu zeigen.
    zustand_katalog = {'laeuft': False}
    wahl = dict((f, '') for f in FILTER_FOLGE)

    # ⚠⚠ **Suchfeld und Auswahl bleiben stehen, nur die Liste rollt.**
    # Wer bei Zeile 30 merkt, dass er die Auswahl ändern
    # will, muss sonst erst wieder hochrollen — und weiß beim Rollen nicht
    # mehr, wonach er überhaupt gefiltert hat.
    #
    # Genau die Regel aus dem Projekt-Handbuch: Erst alles Feste packen,
    # **danach** die rollende Fläche mit `expand=True`.
    kopf = tk.Frame(rahmen, bg=BG)
    kopf.pack(fill='x', side='top')

    such_rahmen = tk.Frame(kopf, bg=BG)
    such_rahmen.pack(fill='x', padx=24, pady=(4, 0))
    from .main_window import round_entry as _feld_rund
    feld = _feld_rund(such_rahmen, suche, fenster.f_base, SURFACE, LINE,
                      ACCENT, FG, placeholder=t('s_ld_suche_platz'))
    feld.holder.pack(fill='x')

    # Ort: tippen oder aufklappen, wie der Lagerort im Lager. Gefiltert wird,
    # sobald ein bekannter Ort im Feld steht.
    ort = tk.StringVar(rahmen)
    ort_wahl = {'ort': ''}
    ort_rahmen = tk.Frame(kopf, bg=BG)
    ort_rahmen.pack(fill='x', padx=24, pady=(8, 0))
    tk.Label(ort_rahmen, text=t('s_ld_ort'), bg=BG, fg=FG,
             font=fenster.f_bold, anchor='w').pack(fill='x')
    ort_zeile, ort_liste, _ort_zeichnen = _combo_box(
        fenster, ort_rahmen, ort, laden_modul.catalog_places, scrollable=200,
        empty_text=t('s_ld_ort_unbekannt'))
    ort_zeile.pack(fill='x', pady=(4, 0))
    ort_liste.pack(fill='x')

    def _am_ort(b):
        return not ort_wahl['ort'] or ort_wahl['ort'] in (b.get('orte') or ())

    # ⭐⭐ **Dieselbe Filterleiste wie in der Bauplan-Liste** (`_filter_bar`).
    # Mit nur einem leeren Suchfeld sähe, wer nicht weiß, wonach er suchen
    # soll, eine leere Seite.
    filter_rahmen = tk.Frame(kopf, bg=BG)
    filter_rahmen.pack(fill='x', padx=24, pady=(8, 0))

    # Die Standzeile gehört zum festen Kopf — sie sagt, worauf sich die Liste
    # darunter bezieht. Rechts daneben der Reset: Bei fünf Auswahlmenüs plus
    # Suchfeld ist „alles wieder offen" sonst ein halbes Dutzend Klicks.
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

        ⚠⚠ **Die Quelle ist der UEX-Katalog, nicht die Bauplanliste.**
        `crafting.all_items()` zeigt nur, was man auch **bauen** kann — der
        Boomtube Rocket Launcher etwa ist nicht craftbar, obwohl UEX seine
        Läden kennt. Gemessen: **1.528 kaufbare Teile** statt 893
        craftbaren, darunter Raketen, Bomben, Torpedorohre und
        Railgun-Munition.

        ⚠⚠ **Der Rückfall auf die Baupläne gilt nur ohne laufenden Abruf.**
        Er zeigt eine andere Gliederung (Bauplan-Arten statt UEX-Bereiche) und
        kennt keine Schiffe — wer ihn während der ersten Minute sieht, hält
        ihn für das Ergebnis, während im Hintergrund noch der Katalog geholt
        wird.

        Läuft der Abruf, bleibt die Liste deshalb leer und der Hinweis darüber
        stehen. Nur wenn gar nichts geht (kein Netz), sind die Baupläne besser
        als eine leere Seite.
        """
        if zustand_katalog['laeuft']:
            return []
        try:
            katalog = laden_modul.catalog_items()
        except Exception as ausnahme:
            errors.record('pages.shops.catalog_items', ausnahme)
            katalog = []
        if katalog:
            raus = [{'name': x['name'], 'kennung': x['kennung'],
                     'bereich': x['abschnitt'], 'gruppe': x['kategorie'],
                     'hersteller': x.get('hersteller') or '',
                     'groesse': x.get('groesse') or '',
                     'klasse': x.get('klasse') or '',
                     'guete': x.get('guete') or '',
                     'orte': x.get('orte') or []}
                    for x in katalog if x['name'] and x['kennung']]
            # ⭐ **Schiffe gehören dazu.** Ihre Kauf- und Mietpreise liegen
            # ohnehin vor (Frachtraum im Routenplaner), und ein Reiter „wo
            # bekomme ich das" ist der Ort dafür. Warengruppe ist die Werft: Wer ein
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
                errors.record('pages.shops.schiffe', ausnahme)
            return raus
        try:
            alle = [b for b in herst_modul.all_items() if b.get('entity')]
        except Exception as ausnahme:
            errors.record('pages.shops.teile', ausnahme)
            return []
        return [{'name': b.get('name') or '', 'kennung': b.get('entity') or '',
                 'bereich': '', 'gruppe': _art_von(b)} for b in alle]

    def _art_von(b):
        """Die Art eines Bauplans — Waffen aufgeteilt nach Schiff und Mann.

        ⚠⚠ **„Waffen" ist keine brauchbare Gruppe.** 270 Stück, und darin
        steckt Grundverschiedenes: Schiffsgeschütze und Handfeuerwaffen. Wer
        einen Kühler fürs Schiff sucht, sucht nicht dieselbe Liste wie jemand,
        der ein Gewehr braucht.

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

        ⚠⚠ **Sortiert nach Anzahl, nicht alphabetisch.** Alphabetisch
        stünden Schiffswaffen und FPS-Waffen an Position 11 und 14, die
        aufgeklappte Liste zeigt rund zehn Zeilen — und die Rollleiste, ein
        dunkler Streifen auf dunklem Grund, fällt nicht auf.

        Ein Auswahlmenü, dessen zwei größte Gruppen man erst erscrollen muss,
        ist **falsch sortiert** — nicht zu kurz.
        """
        # ⚠⚠ **Jedes Menü richtet sich nach den Menüs davor.** Sonst lassen
        # sich Dinge zusammenstellen, die es nicht gibt — etwa „Rüstung" und
        # „Geschütze" nebeneinander, Ergebnis null. Eine unmögliche
        # Kombination gehört gar nicht erst angeboten.
        #
        # ⚠ Das **erste** Menü bleibt immer vollständig — sonst käme man aus
        # einer engen Auswahl nicht mehr heraus.
        vorher = FILTER_FOLGE[:FILTER_FOLGE.index(feld)]
        zaehler = {}
        for b in _teile():
            if not _am_ort(b):
                continue
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

        ⚠⚠ **Ein geleerter Rahmen behält seine Höhe.** Gemessen: Nach dem
        Klick auf ein Teil hatte dieser Rahmen **null Kinder und weiterhin
        920 px**. Die Läden darunter landeten damit bei y=1135 in einem
        1000 px hohen Fenster — gezeichnet, aber außerhalb der Sicht.

        `pack_forget()` ist der einzige Weg, der die Höhe zuverlässig abgibt.
        """
        _leeren(vorschlag_rahmen)
        vorschlag_rahmen.pack_forget()

    def _liste_zeigen():
        """Die Vorschlagsliste wieder einhängen — immer über dem Ergebnis.

        ⚠ **Sie bekommt einen Rand.** Ohne den klebt sie als flache Fläche am
        Text darüber und sieht nach Beschriftung aus, nicht nach Auswahl.
        Ein Kasten sagt „hier steht
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
        if ort_wahl['ort']:
            am_ort = [z for z in liste if z.get('ort') == ort_wahl['ort']]
            if not am_ort:
                _body_text(ergebnis_rahmen,
                           t('s_ld_ort_nicht_hier') % ort_wahl['ort'],
                           fenster.f_small, fill='x')
            liste = am_ort or liste

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
                errors.record('pages.shops.fetch', ausnahme)

            def fertig():
                laeuft['ja'] = False
                try:
                    if ergebnis_rahmen.winfo_exists():
                        _ergebnis_zeichnen()
                except tk.TclError:
                    pass
            _TK_CALLS.put(fertig)

        threading.Thread(target=arbeit, daemon=True).start()

    def _gruppe_aufklappen(gruppe):
        """Auf „… 38 weitere" geklickt: genau diese Warengruppe filtern."""
        wahl['gruppe'] = gruppe
        _filter_gewechselt()

    def _vorschlaege(*_a):
        _liste_leeren()
        text = suche.get().strip().lower()
        # Ist ein Teil gewählt und nichts getippt, gehört die Fläche seinen
        # Läden — keine Vorschlagsliste darüber.
        if not text and gewaehlt['kennung']:
            return
        # ⚠⚠ **Wer tippt, sucht etwas Neues — die alte Antwort muss weg.**
        # Sonst stehen unter dem neuen Vorschlag weiter die Läden des alten
        # Teils und füllen den Bildschirm. Was sich nicht sichtbar ändert,
        # gilt als kaputt.
        #
        # `_waehlen` leert das Feld (`suche.set('')`), bevor es zeichnet —
        # deshalb greift das hier nur beim echten Tippen.
        if text:
            gewaehlt['name'], gewaehlt['kennung'] = '', ''
            _leeren(ergebnis_rahmen)
        # ⚠ **Ohne Suchtext gilt der Filter.** Wer nur klickt statt zu
        # tippen, soll auch etwas sehen: Die Auswahl oben füllt die Liste.
        if len(text) < 2 and not any(wahl[f] for f in FILTER_FOLGE) \
                and not ort_wahl['ort']:
            return
        # ⚠ **Teiltext, nicht nur Wortanfang** — wer „chill" tippt, meint
        # `BlastChill`. Dieselbe Überlegung wie bei den Lagerorten.
        # ⭐⭐ **Gesucht wird auch in Bereich und Warengruppe.** Wo etwas
        # einsortiert ist, muss man nicht wissen: Wer „Radar" tippt, bekommt die Gruppe Radar, egal wo
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
            if not _am_ort(b):
                continue
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
            Wer nicht alle Quantenantriebe kennt, findet einen passenden über
            die Größe. Die nützt aber nur, wenn sie dasteht: Eine Liste aus 44 Fantasienamen (Erebos, Flash, Goliath)
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

        # ⚠⚠ **Eine Gruppe → flache Liste. Mehrere → gegliedert**, mit
        # Zwischenüberschriften je Art wie in der Bauplan-Liste. Ohne sie ist
        # „Systeme (176)" eine Namensreihe, aus der
        # niemand ablesen kann, was überhaupt dazugehört.
        #
        # Der Deckel greift deshalb **je Gruppe**, nicht auf die ganze Liste:
        # Sonst füllte die erste Gruppe alle 40 Zeilen und die übrigen
        # blieben unsichtbar.
        if len(gruppiert) == 1:
            # ⚠⚠ **Eine Gruppe wird VOLLSTÄNDIG gezeigt — kein Deckel.** Wer
            # sich bis auf eine Warengruppe durchgeklickt hat, will sie ganz
            # sehen. Auf „tipp genauer" zu verweisen, hilft niemandem, der
            # die Namen nicht kennt.
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
                # ⚠⚠ **Anklickbar, nicht nur eine Feststellung.** Ein Klick
                # auf die Zeile, die den Rest ankündigt, ist kürzer als der
                # Weg übers Menü oben: Er setzt
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
        anzahl = len([b for b in _teile() if _am_ort(b)])
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
        # Rollfläche. Bei 168 Zeilen darüber sähe den Hinweis sonst niemand,
        # und das Werkzeug wirkte schlicht kaputt.
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
                # ⚠⚠ Nie `after` aus diesem Faden: Tk wirft dann `RuntimeError:
                # main thread is not in main loop`. Siehe `_from_thread`.
                _TK_CALLS.put(zeigen)
            try:
                laden_modul.fetch_catalog(progress=melden)
            except Exception as ausnahme:
                errors.record('pages.shops.katalog', ausnahme)
            # ⚠ Die Schiffsdaten gehören zum selben Aufwasch — ohne sie
            # fehlte der Bereich „Schiffe" in der Liste.
            try:
                from . import ships as schiff_modul
                schiff_modul.update()
            except Exception as ausnahme:
                errors.record('pages.shops.schiffe_holen', ausnahme)

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
            _from_thread(stand_zeile, fertig)

        threading.Thread(target=arbeit, daemon=True).start()

    suche.trace_add('write', after_typing(rahmen, _vorschlaege))

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
                       if _am_ort(b)
                       and all(not wahl[f] or b.get(f) == wahl[f]
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
        ort_wahl['ort'] = ''
        ort.set('')
        suche.set('')
        _liste_leeren()
        _leeren(ergebnis_rahmen)
        _filter_bauen()
        _stand_melden()
        _scroll_to_top(innen)

    ld_reset.bind('<Button-1>', _ld_zuruecksetzen)
    ld_reset.bind('<Enter>', lambda _=None: ld_reset.configure(fg=RED))
    ld_reset.bind('<Leave>', lambda _=None: ld_reset.configure(fg=SUB))

    def _ort_geaendert(*_a):
        """Steht ein bekannter Ort im Feld (oder ist es leer), gilt er als
        Filter — Groß- und Kleinschreibung zählen nicht."""
        _ort_zeichnen()
        text = ort.get().strip().lower()
        treffer = '' if not text else next(
            (o for o in laden_modul.catalog_places() if o.lower() == text),
            None)
        if treffer is None or treffer == ort_wahl['ort']:
            return
        ort_wahl['ort'] = treffer
        _filter_gewechselt()
        _stand_melden()

    ort.trace_add('write', _ort_geaendert)

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
        errors.record('pages.laden_zeile.kennung', ausnahme)
        return
    if not kennung:
        return

    lbl = tk.Label(eltern, text='', bg=theme.FIELD, fg=SUB,
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
            errors.record('pages.laden_zeile.holen', ausnahme)
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
        _TK_CALLS.put(nachtragen)

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
    from .collection_window import GRAD_BUCHSTABE
    eintrag = (kat_daten.load().get('bauplaene') or {}).get(
        paths.name_key(bauplan or ''))
    if not eintrag:
        return ''
    # ⚠⚠ **Bei Rüstung und FPS-Waffen wird NICHTS gezeigt.** In den Rohdaten
    # trägt jeder Helm brav eine Größe und eine Güte — sie bedeuten dort aber
    # nichts: Das „A03 Sniper Rifle" käme als „Größe 3 · Güte A" heraus, was
    # frei erfunden ist. Dieselbe Falle steht in `catalog._values`: Werte nur
    # zeigen,
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

    ⚠⚠ **`erzwingen=True`, wenn gerade ein Schiff dazugekommen ist.** Hat
    der Hangar beim Programmstart schon einmal nachgezogen, steht die
    Sperre — und ein frisch eingetragenes Wunschschiff bekäme nie seine
    Daten; seine Ausstattung bliebe leer.

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
            errors.record('pages.steckplaetze_nachziehen', ausnahme)
        if geholt and danach is not None:
            # ⚠ Zurück in den Oberflächen-Faden — Tk aus einem Thread heraus
            # anzufassen führt zu Abstürzen, die sich nicht nachstellen lassen.
            _from_thread(widget, danach)

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
        paths.name_key(bauplan or ''))
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
        tk.Label(eltern, text=t('s_hg_passt_leer'), bg=theme.FIELD, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(fill='x', padx=12,
                                                        pady=(6, 0))
        return

    # ⚠⚠ **Schiffe im Hangar heißen nicht, dass ihre Steckplätze da sind.**
    # Ist die Steckplatz-Datei etwa nach einem Formatwechsel verworfen,
    # meldete die Herstellung sonst bei **jedem** Bauplan „passt in keines
    # deiner Schiffe".
    #
    # „Keine Daten" und „passt nicht" sehen im Code gleich aus (eine leere
    # Liste) und bedeuten das Gegenteil voneinander. Wer sie zusammenwirft,
    # behauptet etwas, das er nicht weiß — und das ist schlimmer, als nichts
    # zu sagen.
    if not (erkul.load().get('schiffe') or {}):
        lbl = tk.Label(eltern, text=t('s_hg_passt_unbekannt'), bg=theme.FIELD,
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
    # ⚠⚠ **Fett und farbig — Grau wird nicht gelesen.** In `SUB` (dem Grau
    # für Nebensächliches) unter einem langen Rezeptblock wird die Zeile
    # übersehen.
    #
    # Beide Fälle sind Antworten und beide gehören gesehen: Grün „passt in",
    # Gold „passt nirgends". `SUB` bleibt dem vorbehalten, was man überlesen
    # darf.
    lbl = tk.Label(eltern, text=text, bg=theme.FIELD, fg=farbe,
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
    # ⚠⚠ **Die aufgeklappte Zeile muss sich abheben.** Sähe sie aus wie jede
    # andere, wäre nach ein Stueck Rollen nicht mehr klar, welcher Bauplan
    # ausgewaehlt ist — der Name ist dann oben aus dem Bild.
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
    block = tk.Frame(eltern, bg=theme.FIELD)
    block.pack(fill='x', padx=(24, 0), pady=(2, 8))

    # ⚠⚠ **Der Name noch einmal, ueber dem Rezept.** Der Kasten ist lang —
    # Zutaten, Herstellzeit, Qualitaetsregler, Werte. Wer bis dorthin gerollt
    # hat, sieht die Zeile mit dem Namen nicht mehr und weiss nicht, wovon er
    # gerade die Zutaten liest. Der Hersteller steht daneben, weil „5SA
    # 'Rhada'" allein niemandem sagt, worum es geht.
    _kopf = tk.Frame(block, bg=theme.FIELD)
    _kopf.pack(fill='x', padx=12, pady=(10, 0))
    tk.Label(_kopf, text=eintrag['name'], bg=theme.FIELD, fg=ACCENT,
             font=fenster.f_bold, anchor='w').pack(side='left')
    if eintrag['hersteller']:
        tk.Label(_kopf, text='  ·  %s' % eintrag['hersteller'], bg=theme.FIELD,
                 fg=SUB, font=fenster.f_small, anchor='w').pack(side='left')
    # ⭐⭐ **Klasse, Größe und Güte gehören hierher.** Die Bauplan-Liste zeigt
    # sie als Kürzel („M/1/A") — und hier ist die Stelle, an der jemand
    # entscheidet, ob er das Teil überhaupt bauen will.
    #
    # ⚠ **Ausgeschrieben, nicht als Kürzel.** In der Liste ist „M/1/A" richtig,
    # weil dort 738 Zeilen untereinander stehen und jede Spalte zählt. Hier
    # steht eine einzige Zeile über einem langen Kasten — da hilft „Militär ·
    # Größe 4 · Güte A" mehr als drei Buchstaben, die man erst übersetzen muss.
    angaben = _blueprint_specs(eintrag.get('basis'))
    if angaben:
        tk.Label(_kopf, text='  ·  %s' % angaben, bg=theme.FIELD,
                 fg=FG, font=fenster.f_small, anchor='w').pack(side='left')

    if eintrag['habe'] is None:
        _body_text(block, t('s_he_unklar'), fenster.f_small, fill='x')

    # ⚠⚠ **Woher bekomme ich den Bauplan?** Die Antwort steht auf einer
    # anderen Seite; der Knopf führt direkt hin, statt dass man den Namen von
    # Hand hinuebertippen muss.
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
    # unten Stück für Stück da — dies ist die andere Hälfte: Was kostet
    # dasselbe Teil fertig im Regal?
    #
    # ⚠ Zugeordnet wird über die **Entitäts-Kennung**, nie über den Namen
    # (`Gold` lieferte sonst `Golden Medmon` mit). Siehe `scbp/shops.py`.
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
        # ⭐ Was davon liegt im eigenen Lager?
        # ⚠ Gezeigt wird „hast du" bzw. „dir fehlt" — **nie** „du kannst nicht
        # bauen". Das Lager wird von Hand gepflegt und ist irgendwann
        # lückenhaft; ein Hinweis darf danebenliegen, eine Behauptung nicht.
        # ⚠ Die Lage wird bei JEDER Änderung der Stückzahl neu gerechnet
        # (siehe `mengen_setzen` weiter unten) — deshalb hier nur der
        # Startwert für ein Stück.

        # ⭐ **Der Knopf steht GANZ OBEN** — nicht unter Zutaten, Herstellzeit
        # und dem Block „Mit deinem Material", gut zehn Zeilen tiefer. Eine
        # Funktion, die man suchen muss, ist für den Nutzer nicht vorhanden.
        reihe = tk.Frame(block, bg=theme.FIELD)
        reihe.pack(fill='x', padx=12, pady=(8, 2))
        rueck = tk.Label(reihe, text='', bg=theme.FIELD, fg=SUB,
                         font=fenster.f_small, anchor='w')

        # ⭐ Stückzahl daneben. Wer zehn Stück am Stück baut, soll einmal
        # klicken statt zehnmal — beim elften Klick stimmt der Bestand sonst
        # nicht mehr, und niemand merkt es.
        anzahl_var = tk.StringVar(eltern, value='1')

        # ⚠⚠ **Die eingestellte Qualität gilt auch fürs Lager.** Zählte
        # „hast du" jeden Posten ab der Mindestgüte des Rezepts, läse wer den
        # Regler auf Q 685 schiebt weiter die volle Menge, auch wenn kein
        # Posten Q 685 erreicht.
        # `gewaehlt` hält je Material den Reglerwert; die Regler weiter unten
        # schreiben hinein, Lagerzeile und Abzug lesen daraus.
        gewaehlt = {}

        def _zutaten_jetzt(zutaten=stufe['zutaten'], wahl=gewaehlt):
            return [(s, r, m, max(float(g or 0), float(wahl.get(r, 0))))
                    for s, r, m, g in zutaten]

        def hergestellt(_e=None, jetzt=_zutaten_jetzt, lbl=rueck,
                        var=anzahl_var):
            zutaten = jetzt()
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
        # danach — Regler, Wirkungen, Hinweise — fehlte ersatzlos. Nie einen
        # lokalen Namen vergeben, den es in
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
        tk.Label(reihe, text=t('s_lg_anzahl'), bg=theme.FIELD, fg=SUB,
                 font=fenster.f_small).pack(side='left', padx=(12, 6))
        from .main_window import round_entry as _rf_anzahl
        _anzahl_feld = _rf_anzahl(reihe, anzahl_var, fenster.f_small,
                                  theme.FIELD, LINE, ACCENT, FG)
        _anzahl_feld.holder.configure(width=70)
        _anzahl_feld.holder.pack(side='left')
        rueck.pack(side='left', padx=(10, 0))

        # ⭐⭐ **Vormerken — der kurze Weg zur Materialliste**, ohne Umweg
        # über Wunschliste und Steckplätze eines Schiffs. Auch für Helme,
        # Rüstungen und FPS-Waffen, die genauso Rohstoffe brauchen. Hier, wo
        # das Rezept steht, nimmt man es sich vor.
        #
        # ⚠ Die Stückzahl daneben wird mitgenommen: Wer drei Helme bauen will,
        # braucht dreifaches Material.
        from . import fleet as _mz_hangar

        # ⚠ Der Name kommt aus dem Eintrag der Herstellungsliste — `bauplan`
        # gibt es in dieser Funktion nicht, das ist die Nachbarfunktion
        # `_fits_row`.
        _mz_name = eintrag.get('name') or ''
        # ⚠⚠⚠ **NUR eine echte Entitäts-Kennung, niemals der Name.** `basis`
        # ist der Bauplanname; als `uuid` in der Preisabfrage ergäbe er eine
        # kaputte Adresse:
        #
        #     /2.0/items_prices?uuid=CF-447 Rhino Repeater
        #     InvalidURL: URL can't contain control characters
        #
        # Die Seite „Was noch fehlt" bliebe dann leer und versuchte es
        # endlos weiter.
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
            reihe, bg=theme.FIELD, fg=ACCENT, font=fenster.f_small,
            text=(t('s_mz_drauf') if _mz_hangar.notepad_contains(
                _mz_hangar.load(), _mz_name) else ''))
        merk_stand.pack(side='left', padx=(8, 0))

        # Eine Zeile, die sagt, was der Knopf tut — sonst rät man.
        _body_text(block, t('s_lg_bauen_hilfe'), fenster.f_small, fill='x')
        _body_text(block, t('s_mz_hilfe'), fenster.f_small, fill='x')

        # ⚠⚠ **Die Zutatenzeilen werden EINMAL gebaut, danach nur neu
        # beschriftet.** Sie hängen an der Stückzahl, und die ändert sich beim
        # Tippen. Würde bei jedem Tastendruck die Seite neu aufgebaut, verlöre
        # das Stückzahl-Feld den Cursor (wie beim Lager-Suchfeld). Also: Widgets stehen lassen, nur `configure(text=…)`.
        #
        # Aus demselben Grund werden ALLE Etiketten angelegt, auch die für
        # „dir fehlt" und „zu schlechte Qualität". Sie werden je nach Lage
        # ein- und ausgeblendet statt neu erzeugt — sonst springt die Höhe.
        zutat_widgets = []
        for slot, rohstoff, menge, guete in stufe['zutaten']:
            z = tk.Frame(block, bg=theme.FIELD)
            z.pack(fill='x', padx=12, pady=1)
            tk.Label(z, text=slot, bg=theme.FIELD, fg=SUB, font=fenster.f_small,
                     width=18, anchor='w').pack(side='left')
            # ⭐ Der Sprung: Klick auf den Rohstoff öffnet den Bergbau mit
            # diesem Namen in der Suche. Das ist der Grund, warum die
            # Detailfläche kurz bleiben darf — man springt, statt zu stapeln.
            roh_lbl = tk.Label(z, text=rohstoff, bg=theme.FIELD, fg=ACCENT,
                               font=fenster.f_base, anchor='w',
                               cursor='hand2')
            roh_lbl.pack(side='left')

            def zum_bergbau(_e=None, name=rohstoff):
                fenster.mining_search = name
                fenster.jump_to('bergbau')

            roh_lbl.bind('<Button-1>', zum_bergbau)
            menge_lbl = tk.Label(z, text='', bg=theme.FIELD, fg=SUB,
                                 font=fenster.f_small, anchor='e')
            menge_lbl.pack(side='right', padx=12)
            lage_lbl = tk.Label(z, text='', bg=theme.FIELD, fg=GOLD,
                                font=fenster.f_small, anchor='e')
            guete_lbl = tk.Label(z, text='', bg=theme.FIELD, fg=SUB,
                                 font=fenster.f_small, anchor='e')
            # ⭐ „kaufen oder abbauen?" — die Frage, die nach „dir fehlt X"
            # kommt. Sieben der 26 Rohstoffe lassen sich NIRGENDS kaufen; fünf
            # davon stehen zusätzlich auf der Zerlege-Sperrliste. Wer das nicht
            # weiß, sucht am Terminal nach etwas, das es dort nie gibt.
            preis_lbl = tk.Label(z, text='', bg=theme.FIELD, fg=SUB,
                                 font=fenster.f_small, anchor='e')
            zutat_widgets.append((rohstoff, menge, menge_lbl, lage_lbl,
                                  guete_lbl, preis_lbl))

        def mengen_setzen(*_):
            """Mengen und Lage neu beschriften — für die aktuelle Stückzahl.

            ⚠ Ohne sie zeigte die Zutatenliste immer den Bedarf für EIN
            Stück: Wer 10 eintippt, sähe weiter „1.16 SCU" und „dir fehlt
            1.16", obwohl 11,6 gebraucht werden.
            """
            wie_viele = lager.parse_number(anzahl_var.get())
            wie_viele = 1 if not wie_viele or wie_viele < 1 else int(wie_viele)
            neue_lage = {m: (br, da, f, zug, mq) for m, br, da, f, zug, mq
                         in lager.check(_zutaten_jetzt(), wie_viele)}
            for (rohstoff, menge, menge_lbl, lage_lbl, guete_lbl,
                 preis_lbl) in zutat_widgets:
                noetig = (menge or 0) * wie_viele
                # ⚠⚠ **Nicht alles wird in SCU gemessen.** Elf Materialien
                # sind Stückware (Hadanite, Dolivine, Sadaryx …) — dort steht
                # „× 75", nicht „75 SCU". Welches wie zählt, steht in den
                # Rezeptdaten; siehe `crafting.is_piece()`.
                _stk = herst_modul.is_piece(rohstoff)
                menge_lbl.configure(
                    text=(t('s_he_menge_stk' if _stk else 's_he_menge') % noetig
                          if wie_viele == 1
                          else t('s_he_menge_stk_n' if _stk else 's_he_menge_n')
                          % (noetig, menge, wie_viele)))
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
                    guete_lbl.configure(
                        text=t('s_lg_zu_schlecht_stk' if _stk
                               else 's_lg_zu_schlecht')
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
                        errors.record('pages.preis', ausnahme)
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
            z = tk.Frame(block, bg=theme.FIELD)
            z.pack(fill='x', padx=12, pady=(4, 8))
            tk.Label(z, text=t('s_he_zeit'), bg=theme.FIELD, fg=SUB,
                     font=fenster.f_small, width=18, anchor='w').pack(side='left')
            tk.Label(z, text=_duration(stufe['zeit']), bg=theme.FIELD,
                     fg=FG, font=fenster.f_small).pack(side='left')

        # ⭐ Was käme mit DEINEM Material heraus?
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
            werte_kopf = tk.Label(block, text=t('s_he_werte'), bg=theme.FIELD,
                                  fg=FG, font=fenster.f_base, anchor='w')
            werte_kopf.pack(fill='x', padx=12, pady=(10, 2))

            # `stand` hält die aktuelle Qualität je Material. Startwert ist
            # der eigene Lagerstand, sonst die Mitte.
            #
            # ⚠ `stand` IST `gewaehlt` — dasselbe Wörterbuch, kein Abbild.
            # Die Lagerzeile oben liest daraus; eine Kopie hieße, dass der
            # Regler das Lager nie erreicht.
            stand = gewaehlt
            stand.update({m: float(qualitaeten.get(m, 500.0))
                          for m in alle_materialien})
            aus_lager = {m: (m in qualitaeten) for m in alle_materialien}
            # ⚠ Über den Tag, nicht den Namen: „Main Powerplant" gibt es für
            # Idris und Reclaimer, der Name fände immer nur das erste Rezept.
            _bp_kennung = eintrag.get('tag') or eintrag['basis']

            # ⭐⭐ **Die Produkt-Tabelle — Grundwert, gebaut, Änderung.**
            #
            # Nur Faktoren je Material hießen: zwei Materialien auf dieselbe
            # Eigenschaft ergeben zwei Zeilen „× 1.024", und Summe und
            # Ergebnis (DPS, Schildstärke, Kühlleistung) müsste man selbst
            # ausrechnen. Oben die Tabelle, darunter die Regler — gerechnet
            # wird wie auf scmdb.net, siehe `scbp/product_stats.py`.
            #
            # ⚠ Gebaut wird EINMAL, danach nur beschriftet (Regler-Ruckeln,
            # siehe unten). Welche Zeilen es gibt, hängt nur an den
            # Grundwerten — die ändert kein Regler, also bleibt die Zahl der
            # Zeilen fest.
            from . import product_stats as _ps

            def _q_von(mat):
                return stand.get(mat)

            tabellen_rahmen = tk.Frame(block, bg=theme.FIELD)
            tabellen_rahmen.pack(fill='x', padx=12, pady=(2, 4))
            tabelle = herst_modul.product_table(_bp_kennung, _q_von)
            zellen = []
            if any(z[0] == 'row' for z in tabelle):
                for spalte, breite in ((1, 120), (2, 120), (3, 90)):
                    tabellen_rahmen.grid_columnconfigure(spalte, minsize=breite)
                tabellen_rahmen.grid_columnconfigure(0, weight=1)
                zeile_nr = 0
                for spalte, schluessel in enumerate(('s_ps_sp_wert',
                                                     's_ps_sp_grund',
                                                     's_ps_sp_gebaut',
                                                     's_ps_sp_diff')):
                    tk.Label(tabellen_rahmen, text=t(schluessel), bg=theme.FIELD,
                             fg=SUB, font=fenster.f_small,
                             anchor='w' if spalte == 0 else 'e').grid(
                                 row=zeile_nr, column=spalte, sticky='ew')
                zeile_nr += 1
                for index, z in enumerate(tabelle):
                    if z[0] == 'info':
                        if z[1]:
                            tk.Label(tabellen_rahmen, text=z[1], bg=theme.FIELD,
                                     fg=GOLD, font=fenster.f_small,
                                     anchor='w').grid(row=zeile_nr, column=0,
                                                      columnspan=4, sticky='ew',
                                                      pady=(2, 2))
                            zeile_nr += 1
                        continue
                    if z[0] == 'section':
                        tk.Label(tabellen_rahmen, text=z[1], bg=theme.FIELD,
                                 fg=GOLD, font=fenster.f_small,
                                 anchor='w').grid(row=zeile_nr, column=0,
                                                  columnspan=4, sticky='ew',
                                                  pady=(6, 0))
                        zeile_nr += 1
                        continue
                    # ⭐ DPS hervorgehoben — die Zahl, nach der gefragt wurde.
                    hervor = z[6] == 'dps'
                    schrift = fenster.f_base if hervor else fenster.f_small
                    tk.Label(tabellen_rahmen, text=z[1], bg=theme.FIELD,
                             fg=FG if hervor else SUB, font=schrift,
                             anchor='w').grid(row=zeile_nr, column=0,
                                              sticky='ew')
                    grund_lbl = tk.Label(tabellen_rahmen, text='', bg=theme.FIELD,
                                         fg=FG, font=schrift, anchor='e')
                    grund_lbl.grid(row=zeile_nr, column=1, sticky='ew')
                    gebaut_lbl = tk.Label(tabellen_rahmen, text='', bg=theme.FIELD,
                                          fg=SUB, font=schrift, anchor='e')
                    gebaut_lbl.grid(row=zeile_nr, column=2, sticky='ew')
                    diff_lbl = tk.Label(tabellen_rahmen, text='', bg=theme.FIELD,
                                        fg=SUB, font=fenster.f_small, anchor='e')
                    diff_lbl.grid(row=zeile_nr, column=3, sticky='ew')
                    zellen.append((index, grund_lbl, gebaut_lbl, diff_lbl))
                    zeile_nr += 1
            else:
                # ⚠ Zwei verschiedene Gründe, zwei Sätze: Fehlen die Daten
                # ganz, kommen sie mit dem nächsten Auffrischen. Fehlen sie
                # nur für diesen Gegenstand (Magazine, manche Kleidung), gibt
                # es schlicht keine — dann hilft kein Warten.
                _hat_daten = bool(herst_modul.load().get('products'))
                _body_text(tabellen_rahmen,
                           t('s_ps_keine' if _hat_daten else 's_ps_nachladen'),
                           fenster.f_small, fill='x', padx=0)

            _bewertung_farbe = {'good': ACCENT, 'bad': RED, 'neutral': SUB}

            def tabelle_zeichnen():
                """Nur die Zahlen der Tabelle austauschen."""
                if not zellen:
                    return
                neu = herst_modul.product_table(_bp_kennung, _q_von)
                for index, grund_lbl, gebaut_lbl, diff_lbl in zellen:
                    z = neu[index] if index < len(neu) else None
                    if not z or z[0] != 'row':
                        continue
                    grund, gebaut, diff, bewertung = _ps.formatted(z)
                    farbe = _bewertung_farbe[bewertung]
                    grund_lbl.configure(text=grund)
                    gebaut_lbl.configure(text=gebaut, fg=farbe)
                    diff_lbl.configure(text=diff, fg=farbe)

            grundliste = herst_modul.values_with_stock(
                _bp_kennung, {m: 500.0 for m in alle_materialien})

            # --- Ein Regler je Material, die Wirkung rechts daneben ---
            # Dieselbe Frage, die man sonst auf scmdb.net von Hand stellt:
            # „Und mit besserem Erz?" Nur dass hier der eigene Lagerstand der
            # Ausgangspunkt ist — je Material einzeln.
            #
            # ⚠⚠ **Je Material ein eigener Wert.** Ein Regler für alle
            # Zutaten entspräche praktisch nie der Wirklichkeit — man hat die
            # Materialien so gut wie nie in gleicher Qualität da.
            #
            # ⭐ **Die Wirkung steht rechts neben ihrem Regler.** So sieht man
            # beim Ziehen, was sich ändert, ohne zwischen Regler und einer
            # eigenen Liste hin und her zu lesen.
            from .main_window import slider as schieberegler
            tk.Label(block, text=t('s_he_regler_kopf'), bg=theme.FIELD, fg=FG,
                     font=fenster.f_base, anchor='w').pack(
                         fill='x', padx=12, pady=(10, 2))
            # ⭐ Der Satz, der die Regler erst einordnet: Wer kauft, landet
            # immer bei 500 — dem Nullpunkt. Alles darüber muss man selbst
            # abbauen.
            _body_text(block, t('s_he_kauf_q') % preis_modul.BUY_QUALITY,
                        fenster.f_small, fill='x')

            # ⚠⚠ **589 Rezept-Slots haben ein Material ohne jede
            # Qualitaetswirkung** — Titanium in der BUL-H4 Armor etwa. Der
            # Regler bleibt trotzdem bedienbar (scmdb.net haelt es genauso),
            # daneben steht, warum sich nichts tut.
            _wirksam = set()
            try:
                for _s in (herst_modul.slots(_bp_kennung) or []):
                    if _s.get('material') and _s.get('wirkungen'):
                        _wirksam.add(_s['material'])
            except Exception as ausnahme:
                errors.record('pages.wirksam', ausnahme)
                _wirksam = set(alle_materialien)

            regler_zeilen = {}
            zeilen_widgets = []
            for _mat in alle_materialien:
                reihe_r = tk.Frame(block, bg=theme.FIELD)
                reihe_r.pack(fill='x', padx=12, pady=3)
                tk.Label(reihe_r, text=_mat, bg=theme.FIELD, fg=ACCENT,
                         font=fenster.f_small, width=16, anchor='w').pack(
                             side='left', anchor='n')

                # ⚠ Der Wert MUSS neben dem Regler stehen. Ohne ihn zieht man
                # blind und weiß nicht, welche Qualität man gerade
                # durchspielt — genau der Wert, um den es geht.
                _wert_lbl = tk.Label(reihe_r, text=t('s_lg_q_wert')
                                     % int(stand[_mat]),
                                     bg=theme.FIELD, fg=ACCENT,
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
                                          gezogen, width=200, bg=theme.FIELD)
                _schieber.pack(side='left', anchor='n')
                _wert_lbl.pack(side='left', anchor='n', padx=(10, 0))
                _quelle_lbl = tk.Label(
                    reihe_r,
                    text=(t('s_he_ohne_wirkung') if _mat not in _wirksam
                          else t('s_he_regler_lager') if aus_lager[_mat]
                          else t('s_he_regler_ohne')),
                    bg=theme.FIELD, fg=SUB, font=fenster.f_small, anchor='w')
                _quelle_lbl.pack(side='left', anchor='n', padx=(10, 0))
                regler_zeilen[_mat] = (_wert_lbl, _quelle_lbl, _schieber)

                # Rechts: jede Eigenschaft, die dieses Material verändert.
                # ⚠ Ein Material kann in mehreren Slots stecken und mehrere
                # Eigenschaften treffen — jede bekommt ihre eigene Zeile.
                wirk_rahmen = tk.Frame(reihe_r, bg=theme.FIELD)
                wirk_rahmen.pack(side='right', anchor='n')
                _nr = 0
                for w in grundliste:
                    if w['material'] != _mat:
                        continue
                    # ⚠ Übersetzt über den sprachneutralen Schlüssel, nicht
                    # über den englischen Namen — siehe `crafting.property_name`.
                    tk.Label(wirk_rahmen,
                             text=herst_modul.property_name(w['eigenschaft'],
                                                            w.get('key')),
                             bg=theme.FIELD, fg=SUB, font=fenster.f_small,
                             anchor='e').grid(row=_nr, column=0, sticky='e',
                                              padx=(0, 10))
                    # ⚠⚠ Faktor und Prozent in EIGENEN Etiketten mit fester
                    # Breite — ein gemeinsames Etikett schnitt „+4,70 %" zu
                    # „+4.(" ab.
                    faktor_lbl = tk.Label(wirk_rahmen, text='', bg=theme.FIELD,
                                          fg=ACCENT, font=fenster.f_base,
                                          width=9, anchor='e')
                    faktor_lbl.grid(row=_nr, column=1, sticky='e')
                    prozent_lbl = tk.Label(wirk_rahmen, text='', bg=theme.FIELD,
                                           fg=ACCENT, font=fenster.f_base,
                                           width=10, anchor='e')
                    prozent_lbl.grid(row=_nr, column=2, sticky='e')
                    # Darunter die Spanne: Ein Faktor allein ist nicht
                    # einzuordnen — erst „×0.9–1.1" zeigt, wie viel noch geht.
                    spanne_lbl = tk.Label(wirk_rahmen, text='', bg=theme.FIELD,
                                          fg=SUB, font=fenster.f_small,
                                          anchor='e')
                    spanne_lbl.grid(row=_nr + 1, column=0, columnspan=3,
                                    sticky='e')
                    zeilen_widgets.append((w, faktor_lbl, prozent_lbl,
                                           spanne_lbl))
                    _nr += 2

            def werte_zeichnen():
                """Nur die Zahlen austauschen — keine Widgets neu bauen.

                ⚠⚠ Bei jeder Reglerbewegung alles zu zerstören und neu
                aufzubauen hieße: bei jedem Pixel. Das ruckelt so stark, dass
                der Regler nicht bedienbar ist.
                """
                aktuell = {(w['eigenschaft'], w['material'], w['slot']): w
                           for w in herst_modul.values_with_stock(
                               _bp_kennung, stand)}
                for w0, faktor_lbl, prozent_lbl, spanne_lbl in zeilen_widgets:
                    w = aktuell.get((w0['eigenschaft'], w0['material'],
                                     w0['slot']))
                    if not w:
                        faktor_lbl.configure(text='')
                        prozent_lbl.configure(text='')
                        spanne_lbl.configure(text='')
                        continue
                    # ⚠⚠ **Die Farbe darf nicht an der Zahl haengen.** Bei
                    # Rueckstoss und Quantum-Treibstoff ist WENIGER besser.
                    if w.get('absolut'):
                        # ⚠ Power Pips: eine Stueckzahl, kein Faktor.
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
                    prozent_lbl.configure(
                        text=('' if w.get('absolut')
                              else t('s_he_prozent') % ((w['faktor'] - 1) * 100)),
                        fg=farbe)
                    sp = w.get('spanne')
                    text_spanne = ''
                    if sp:
                        q_von, q_bis, f_von, f_bis, basis = sp
                        text_spanne = (t('s_he_spanne')
                                       % (q_von, q_bis, f_von, f_bis, round(basis))
                                       if basis is not None else
                                       t('s_he_spanne_ohne')
                                       % (q_von, q_bis, f_von, f_bis))
                    if not w.get('besser_hoch', True):
                        text_spanne = '%s · %s' % (t('s_he_weniger_gut'),
                                                   text_spanne)
                    spanne_lbl.configure(text=text_spanne)

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
                tabelle_zeichnen()
                # Die Lagerzeile oben hängt an denselben Qualitäten.
                mengen_setzen()

            # Alles wieder auf den eigenen Lagerstand zurückstellen.
            zurueck = tk.Label(block, text=t('s_he_zurueck_lager'),
                               bg=theme.FIELD, fg=ACCENT, font=fenster.f_small,
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
    # überein (etwa Beryl auf Daymar, 0,4 %).
    prozent = round(anteil * 100)
    text = (t('s_bg_anteil_wenig') if prozent < 1
            else t('s_bg_anteil') % prozent)
    tk.Label(zeile, text=t('s_bg_st_%d' % stufe), bg=grund, fg=SUB,
             font=fenster.f_small, anchor='e').pack(side='right', padx=(6, 12))
    tk.Label(zeile, text=text, bg=grund,
             fg=farbe, font=fenster.f_base, anchor='e').pack(side='right')


def _salvage(fenster, rahmen):
    """Was in einem Wrack steckt — und ob sich das Aussteigen lohnt.

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

    schiff = tk.StringVar(rahmen)
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
        # Schiffsteile führt kaum ein Händler (gemessen: drei von vier
        # Werksteilen ohne jedes Ankaufgebot).
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
        # fremden Fäden — zurück geht es über `_TK_CALLS`.
        def arbeit():
            try:
                teile, gefunden = _load_salvage(name)
            except Exception as ausnahme:
                errors.record('pages.bergung.holen', ausnahme)
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
            _TK_CALLS.put(fertig)

        threading.Thread(target=arbeit, daemon=True).start()

    def _vergessen():
        """Die gemerkten Wracks verwerfen — mit Rückfrage, die die Zahl nennt.

        ⚠ Ohne diesen Knopf müsste jemand `bergung.json` von Hand löschen. Ein
        Zwischenspeicher, den der Spieler nicht leeren kann, ist keiner.
        """
        # ⚠⚠ **`ask_yes_no`, nicht `messagebox.askyesno`.** Der
        # System-Dialog sieht auf jedem Schreibtisch anders aus — unter Linux
        # weißer Kasten mit fetter Schrift und englischen Knöpfen („Yes"/„No")
        # mitten in einem deutschen, dunklen Programm. Der eigene Dialog
        # wird überall sonst benutzt.
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
    # nicht" für ein Schiff, das jeder kennt.
    #
    # Die Werft steht in den UEX-Schiffsdaten, die Kürzel-Tabelle liefert
    # erkul selbst mit (152 Hersteller).
    # ⚠ **Kein Herstellerkürzel aus der Tabelle.** Erkuls Herstellerliste führt
    # fünf verschiedene Kürzel unter dem Namen „Aegis Dynamics", und `aegs`,
    # das die Schiffe benutzen, ist nicht darunter. Die Zuordnung erkennt
    # Zusammenziehungen selbst (`_ist_kuerzel`); die Werft kommt
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
                errors.record('pages.bergung.preis', ausnahme)
    return teile, treffer


def _signature_scanner(window, parent, signature_var):
    """Schalter und Scan-Bereich für das Ablesen der Signatur (nur Windows).

    ⚠ Der Schalter ist zugleich die Zustimmung zum Bildabgriff — Standard aus.
    ⚠ Gestartet wird die Wache NICHT hier, sondern beim Programmstart
    (`Overlay.run`) — eine Seite wird erst beim ersten Besuch gebaut.
    """
    from . import scan_window, signature_scan, signature_watch
    from .main_window import toggle_switch
    if not scan_window.available():
        return
    row = _setting_row(window, parent, t('s_bg_scan_kopf'), t('s_bg_scan_h'))

    def toggle():
        # ⚠ Über `set_enabled` — derselbe Weg wie das Auge in der Overlay-Leiste,
        # damit beide Anzeigen denselben Stand zeigen.
        new_value = signature_watch.set_enabled(
            not paths.setting_bool(signature_watch.SETTING, False))
        window.say(t('s_bg_scan_sagen') % (t('e_an') if new_value else t('e_aus')))
        return new_value

    switch = toggle_switch(row, paths.setting_bool(signature_watch.SETTING, False),
                           toggle)
    switch.pack()

    def follow(on):
        # Schaltet das Auge um, zieht der Schalter hier nach. Ist die Seite
        # zu, wirft `draw` — dann meldet `set_enabled` die Anzeige ab.
        if not switch.winfo_exists():
            raise tk.TclError('Seite geschlossen')
        switch.draw(on)

    signature_watch.on_switch(follow)

    line = tk.Frame(parent, bg=BG)
    line.pack(fill='x', pady=(0, 10))
    area = tk.Label(line, text='', bg=BG, fg=SUB, font=window.f_small,
                    anchor='w', justify='left')

    # ⚠ Kein Scan-Bereich: Die Pille wandert mit dem gescannten Brocken,
    # VerseKit sucht sie selbst. Der Knopf öffnet nur das Anlern-Fenster.
    def show_area():
        count = len(signature_scan.samples())
        area.configure(text=t('s_bg_scan_bilder') % count if count else '', fg=SUB)

    _button(window, line, t('s_bg_scan_anlernen'),
            lambda: scan_window.open_window(window.root)).pack(side='left')
    _button(window, line, t('scan_ordner'),
            scan_window.open_sample_folder).pack(side='left', padx=(8, 0))
    area.pack(side='left', fill='x', expand=True, padx=10)
    show_area()

    def read_value(value):
        # ⚠ Kommt aus dem Wach-Faden — Tk nur über `_TK_CALLS` anfassen.
        if value is None:
            return

        def put():
            # ⚠⚠ Die Seite gehört zu EINEM Hauptfenster. Wurde es geschlossen,
            # schriebe die Wache weiter in die tote Seite („bad window path
            # name"). Dann abmelden.
            try:
                if not parent.winfo_exists():
                    signature_watch.unlisten(read_value)
                    return
                signature_var.set('{:,}'.format(value).replace(',', '.'))
            except tk.TclError:
                signature_watch.unlisten(read_value)
        _TK_CALLS.put(put)

    signature_watch.listen(read_value)


def _mining_missing(window, page, inner):
    """Die Bergbau-Seite ohne Bergbau-Daten — mit Weg heraus.

    ⚠⚠ Zwei Fallen:

    1. **Eine Seite wird EINMAL gebaut.** Wer sie öffnet, bevor die Daten da
       sind, sähe den Satz sonst bis zum Neustart. Deshalb baut sich die
       Seite beim nächsten Öffnen neu, sobald die Daten da sind — und sofort
       nach „Jetzt holen".
    2. **Der Scanner-Schalter steht vor dem Abbruch.** Der Scanner braucht
       die Bergbau-Daten nicht.
    """
    from . import mining

    _body_text(inner, t('s_bg_keine_daten'), window.f_small, fill='x')

    def rebuild():
        window.on_show.pop('bergbau', None)
        for child in page.winfo_children():
            child.destroy()
        _mining(window, page)

    def fetch():
        window.say(t('s_bg_holt'))

        def work():
            try:
                from . import catalog
                ok, message = mining.update(catalog.current_version())
                if not ok:
                    errors.record('pages.bergbau.holen', RuntimeError(message))
            except Exception as error:
                errors.record('pages.bergbau.holen', error)
                ok, message = False, t('s_bg_holen_weg')

            def done():
                window.say(message)
                if ok and mining.locations():
                    rebuild()
            _from_thread(page, done)

        threading.Thread(target=work, daemon=True).start()

    line = tk.Frame(inner, bg=BG)
    line.pack(fill='x', pady=(6, 16))
    _button(window, line, t('s_bg_jetzt_holen'), fetch).pack(side='left')

    # Der Scanner liest die Zahl aus dem Bild — dafür braucht er keine
    # Fundorte. Nur der Rechner daneben braucht sie.
    _signature_scanner(window, inner, tk.StringVar(inner, value=''))

    def on_show():
        try:
            if mining.locations():
                rebuild()
        except Exception as error:
            errors.record('pages.bergbau.neu', error)
    window.on_show['bergbau'] = on_show


def _mining(fenster, rahmen):
    """Wo welches Erz abzubauen ist — **beide** Richtungen in einer Suche.

    Ohne Eingabe stehen die Orte da (man ist meistens irgendwo). Tippt man
    einen Rohstoff, kommen dessen Fundorte; tippt man einen Ort, kommt, was es
    dort gibt. Das sind nicht zwei Ansichten, sondern eine Tabelle mit zwei
    Eingängen — beides sind echte Fragen, je nachdem ob man gerade fliegen mag
    oder nicht.
    """
    from . import mining as berg_modul
    # ⚠ **`s_bg_lead`, nicht `s_wr_lead`** — Letzteres ist der Text der
    # Bergungs-Seite.
    _heading(fenster, rahmen, t('hf_bergbau'), t('s_bg_lead'))
    innen = _scroll_area(rahmen)

    try:
        orte = berg_modul.locations()
        erze = berg_modul.ores()
    except Exception as ausnahme:
        errors.record('pages.bergbau', ausnahme)
        orte, erze = [], []

    # ⚠⚠ **Vor der Abbruchbedingung.** Die Methodenempfehlung braucht KEINE
    # Bergbaudaten — sie steht fest im Programm. Stünde sie weiter unten, wäre
    # sie ausgerechnet für den weg, der ohne Netz unterwegs ist: Der Abbruch
    # darunter beendet die Seite, sobald die Daten fehlen.
    _method_box(fenster, innen)

    if not orte:
        _mining_missing(fenster, rahmen, innen)
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
    # ⚠ Mit ausdrücklichem Elternteil: Ohne hängt Tk die Variable an das
    # zuerst geöffnete Fenster. Lebt davon noch ein anderes, beobachtet die
    # Suche eine Variable, in die das Feld nie schreibt.
    suche_var = tk.StringVar(innen, value=getattr(fenster, 'mining_search', '')
                             or '')
    fenster.mining_search = ''
    ziel_suche = _setting_row(fenster, innen, t('s_bg_suche'), '')
    feld = round_entry(ziel_suche, suche_var, fenster.f_small, theme.FIELD,
                       LINE, ACCENT, FG)
    feld.holder.pack(fill='x', pady=(4, 12))

    # ⚠ Dieselben Auswahlfelder wie auf den anderen Seiten. Tippen bleibt
    # möglich — aber wer die 38 Rohstoffe oder 48 Orte nicht auswendig kann,
    # soll sie aufklappen können, statt zu raten. Ein Bedienkonzept für das
    # ganze Programm (siehe `_filter_bar`).
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
            # ⭐ **Gewählt heißt aufgeklappt.** Wer einen Namen aus dem
            # Auswahlfeld nimmt, hat ihn schon gewählt — die Kopfzeile danach
            # noch einmal anzuklicken wäre ein überflüssiger Klick.
            #
            # ⚠ Nur bei der Auswahl, nicht beim Tippen: Nach zwei Buchstaben
            # stehen dort noch zwölf Treffer, und einer davon spränge auf.
            if berg_wahl['erz']:
                offen['name'] = 'erz:' + berg_wahl['erz']
            elif berg_wahl['ort']:
                offen['name'] = 'ort:' + berg_wahl['ort']
            else:
                offen['name'] = None
            suche_var.set(neu)
            # Sofort zeichnen — `trace_add` wartet aufs Ende des Tippens.
            zeichnen()
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

    # ⭐⭐ **Scan-Signatur — das Werkzeug, das im Spiel wirklich fehlt.**
    # Der Bergbau-Scanner zeigt eine Zahl und verrät nicht, was dahintersteckt.
    # Die Zahl ist die Signatur des Rohstoffs mal der Zahl der Brocken im
    # Vorkommen; wie viele es höchstens sein können, sagt die Seltenheit
    # (legendär 2, verbreitet 6). Beides steht in den Bergbaudaten, die der
    # Watcher ohnehin lädt.
    #
    # ⚠ Das Feld wird **hier** gebaut, nicht in `zeichnen()`. Läge es darin,
    # verlöre es bei jedem Tastendruck den Cursor (wie beim Suchfeld im
    # Lager).
    sig_var = tk.StringVar(innen, value='')
    ziel_sig = _setting_row(fenster, innen, t('s_bg_sig_feld'), '')
    sig_feld = round_entry(ziel_sig, sig_var, fenster.f_small, theme.FIELD,
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
            errors.record('pages.signatur', ausnahme)
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
    _signature_scanner(fenster, innen, sig_var)

    liste_rahmen = tk.Frame(innen, bg=BG)
    liste_rahmen.pack(fill='both', expand=True)
    offen = {'name': None}

    def aufklappen(*_):
        """Neu zeichnen, **ohne** die Rollstelle zu verlieren.

        ⛔⛔ Ein Klick auf ein Erz baut die ganze Liste neu — ohne diesen Weg
        spränge die Seite dabei nach oben, und man müsste neu hinunterrollen.

        ⚠ **Nur beim Aufklappen, nicht beim Suchen.** Wer etwas Neues eintippt,
        will das erste Ergebnis sehen — dort ist der Sprung nach oben richtig.
        Deshalb zwei Wege auf dieselbe Zeichenfunktion statt eines.
        """
        _keep_scroll(innen, zeichnen)

    def zeichnen(*_):
        for w in liste_rahmen.winfo_children():
            w.destroy()
        text = suche_var.get().strip().lower()

        # ⚠ **Rohstoffe zuerst, auch ohne Suche.** Gesucht wird mit „wo finde
        # ich Titanium?", nicht mit „wo bin ich?".
        geraet = berg_wahl['geraet']
        for e in erze:
            if geraet and not _has_tool(e, geraet):
                continue
            if not text or text in e['name'].lower():
                _mining_ore(fenster, liste_rahmen, e, offen, aufklappen,
                          geraet)
        # Orte danach — sie beantworten die zweite Frage („was gibt es hier?").
        #
        # ⚠ **Ohne Eingabe stehen sie NICHT da.** 48 Ortszeilen unter den
        # Rohstoffen liest niemand: Wer einen Ort sucht, tippt ihn oder
        # klappt ihn im Auswahlfeld auf.
        #
        # Die Auswahl schreibt in dasselbe Suchfeld — ein gewählter Ort füllt
        # `text` also und erscheint dadurch von selbst.
        if text:
            # ⚠ Die Stationen heißen `CRU-L4 …`, die Punkte `CRU L4` — wer
            # mit Strich tippt, soll den Punkt trotzdem finden.
            ohne_strich = text.replace('-', ' ')
            for o in orte:
                if geraet and not (o.get('je_geraet') or {}).get(geraet):
                    continue
                if (text in o['name'].lower()
                        or ohne_strich in o['name'].lower()
                        or text in (o.get('vorlage') or '').lower()
                        or text in (o['system'] or '').lower()):
                    _mining_place(fenster, liste_rahmen, o, offen,
                              aufklappen, geraet)

        if not liste_rahmen.winfo_children():
            _body_text(liste_rahmen, t('s_he_nichts'), fenster.f_small,
                        fill='x')

    def jump_to_ore(name):
        """Den Rohstoff suchen und gleich aufgeklappt zeigen — wie eine Wahl
        im Auswahlfeld. Ohne Namen wird die Suche geleert."""
        wanted = berg_modul.material_key(name) if name else ''
        hit = next((e['name'] for e in erze
                    if wanted and berg_modul.material_key(e['name']) == wanted),
                   '')
        offen['name'] = ('erz:' + hit) if hit else None
        suche_var.set(hit or name or '')
        # Sofort zeichnen — `trace_add` wartet aufs Ende des Tippens.
        zeichnen()
        _scroll_to_top(liste_rahmen)

    def on_show():
        """Beim erneuten Aufrufen: ein Sprung aus Rezept oder Farmliste setzt
        den Rohstoff, sonst beginnt die Seite leer.

        ⚠ Die Seite wird nur ein- und ausgeblendet, nicht neu gebaut —
        `mining_search` muss deshalb HIER gelesen werden, nicht nur beim Bau.
        """
        name = getattr(fenster, 'mining_search', '') or ''
        fenster.mining_search = ''
        jump_to_ore(name)

    fenster.on_show['bergbau'] = on_show
    suche_var.trace_add('write', after_typing(rahmen, zeichnen))
    if suche_var.get():
        jump_to_ore(suche_var.get())
    else:
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
    lässt den Rest hängen — aus „Checkmate" würde „Checkm" / „ate". Zwei
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
    Programm.

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

    ⭐ Eine Tabelle, aus der man je Material die beste Raffinerie abliest —
    **mit den Nachteilen**.

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
        errors.record('pages.raffinerien', ausnahme)
        spalten, zeilen = [], []

    if not spalten or not zeilen:
        # ⚠ Kein leerer Bildschirm: Ohne Bergbaudaten ist die Seite nicht
        # kaputt, sie hat nur noch nichts. Das gehört dagestanden.
        _body_text(innen, t('s_rf_keine'), fenster.f_base, fill='x')
        return

    # ⛔⛔ **Jede Spalte kostet Platz, und Tk schneidet still ab.**
    # Gemessen bei 1100×842 (verfügbar: 852 px): Mit 20 Zeichen für das
    # Material und 8 je Wert bräuchte die Zeile 890 px bei normaler Schrift
    # und **1354 px** bei „sehr groß" — dort fehlten fünf Spalten ersatzlos.
    #
    # Ein Wert ist höchstens vier Zeichen breit (`+11`, `-9`). Die Überschrift
    # passt zweizeilig darüber.
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
    # Gehört eine Spalte zu mehreren Orten, stünde dort „Nyx, Pyro, Stanton",
    # und die Zeile wäre unlesbar. Jede Spalte trägt genau ein System, und
    # das System steht als **Überschrift** darüber. Eine Ortsspalte
    # wiederholte nur, was schon oben steht.
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
        # Umbrechen: Bei der größten Schrift ist die Pyro-Liste sonst zu
        # breit (tools/randpruefung.py).
        _namen_lbl = tk.Label(z, text=', '.join(namen), bg=BG, fg=SUB,
                              font=fenster.f_small, anchor='w',
                              justify='left')
        _namen_lbl.pack(side='left', fill='x', expand=True)
        _wrap_self(_namen_lbl)

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

    # ⚠ Ein eigener Textschlüssel, kein `t(...) % (a, b).split('·')[0]` —
    # dort liefe `.split()` auf dem **Tupel**, und die ganze Liste bliebe
    # leer. Der Selbsttest baut die Seite ohne Suchbegriff und erreicht
    # diesen Zweig nicht.
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
    block = tk.Frame(eltern, bg=theme.FIELD)
    block.pack(fill='x', padx=(24, 0), pady=(2, 8))
    # Mit Gerätewahl gilt dessen eigener Anteil — und damit auch dessen
    # Reihenfolge. Ohne Wahl bleibt es bei der aus `mining.ores()`.
    if geraet:
        fundorte.sort(key=lambda e: (-((e[5] or {}).get(geraet, (0.0,))[0]
                                       if len(e) > 5 else 0.0),
                                     e[0].lower()))
    for eintrag in fundorte:
        ort, system, arten = eintrag[0], eintrag[1], eintrag[2]
        # ⚠ Nachgiebig: Ältere Ablagen und Selbsttest-Listen haben nur drei
        # Felder je Fundort.
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
        z = tk.Frame(block, bg=theme.FIELD)
        z.pack(fill='x', padx=12, pady=1)
        tk.Label(z, text=ort, bg=theme.FIELD, fg=FG, font=fenster.f_base,
                 anchor='w').pack(side='left')
        tk.Label(z, text=system, bg=theme.FIELD, fg=SUB, font=fenster.f_small,
                 anchor='w').pack(side='left', padx=(10, 0))
        tk.Label(z, text=_kind_text(arten), bg=theme.FIELD, fg=SUB,
                 font=fenster.f_small, anchor='e').pack(side='right', padx=12)
        _mining_share(fenster, z, anteil, stufe, theme.FIELD, allein)

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
        errors.record('pages.raffinerie', ausnahme)
        raff = []
    if raff:
        tk.Label(block, text=t('s_bg_raff_kopf'), bg=theme.FIELD, fg=FG,
                 font=fenster.f_base, anchor='w').pack(
                     fill='x', padx=12, pady=(10, 2))
        spanne = raff[0][2] - raff[-1][2]
        if not spanne:
            _body_text(block, t('s_bg_raff_egal'), fenster.f_small, fill='x')
        else:
            for namen, system, bonus in raff:
                z = tk.Frame(block, bg=theme.FIELD)
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
                tk.Label(z, text=kurz, bg=theme.FIELD, fg=FG,
                         font=fenster.f_base, anchor='w').pack(side='left')
                tk.Label(z, text=system or '', bg=theme.FIELD, fg=SUB,
                         font=fenster.f_small, anchor='w').pack(
                             side='left', padx=(10, 0))
                tk.Label(z, text=t('s_bg_raff_zeile') % bonus, bg=theme.FIELD,
                         fg=(ACCENT if bonus > 0 else GOLD if bonus < 0 else SUB),
                         font=fenster.f_base, anchor='e').pack(
                             side='right', padx=12)
            _body_text(block, t('s_bg_raff_spanne') % spanne,
                        fenster.f_small, fill='x')
        # ⭐ **Von hier zur ganzen Tabelle.** Der Kasten beantwortet „welche
        # Raffinerie für DIESES Erz"; wer mehrere Erze im Laderaum hat, will
        # die Gegenrichtung.
        #
        # ⚠ Über `jump_to`, nicht `open_page` — nur so steht der Rückweg über
        # der Zielseite.
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

        # ⭐ **Der Vergleich klappt zu und startet zugeklappt.** Neun Methoden
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

    ⚠⚠ **Nach Gerät gruppiert, nicht in einer Liste.** „59 % Aphorite (FPS)",
    „48 % Beradom (Fahrzeug)" und „33 % Silicon (Schiff)" absteigend
    untereinander behaupteten, das eine sei ergiebiger als das andere. Das
    ist falsch: Die drei Zahlen sind
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
    block = tk.Frame(eltern, bg=theme.FIELD)
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
            tk.Label(block, text=t('s_bg_art_' + kennung), bg=theme.FIELD,
                     fg=SUB, font=fenster.f_small, anchor='w').pack(
                         fill='x', padx=12, pady=(8, 2))
        # ⚠ **Nach Konzentration, nicht alphabetisch.** „Was gibt es hier?"
        # heisst in Wahrheit „was lohnt sich hier?" — eine Liste von A bis Z
        # beantwortet das nicht. Bei gleichem Anteil entscheidet der Name,
        # damit die Reihenfolge zwischen zwei Aufrufen dieselbe bleibt.
        namen = sorted(werte, key=lambda n: (-werte[n][0], n.lower()))
        for name in namen:
            anteil, stufe = werte[name]
            z = tk.Frame(block, bg=theme.FIELD)
            z.pack(fill='x', padx=12, pady=1)
            tk.Label(z, text=name, bg=theme.FIELD, fg=FG, font=fenster.f_base,
                     anchor='w').pack(side='left')
            # ⚠ **Keine Art-Spalte hier.** Die Überschrift des Blocks sagt
            # bereits „Fahrzeug"; daneben in jeder Zeile noch einmal
            # „Fahrzeug" ist Rauschen — und bei einem Erz, das zu zwei Geräten
            # gehört (Carinite), stünde in beiden Blöcken dasselbe Paar und
            # damit zweimal etwas Falsches.
            _mining_share(fenster, z, anteil, stufe, theme.FIELD,
                         len(werte) <= 1)


# ------------------------------------------------------------------- Lager
#
# Rohstoffe selbst eintragen, beim Herstellen abziehen lassen.
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
        # Aus: leerer Kreis wie in der Bauplan-Liste, an: grüner Haken.
        symbol = icons.line(rahmen, 'haken' if an_jetzt else 'offen',
                            background=BG,
                            color=icons.GREEN if an_jetzt else icons.GREY)
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
    **nicht** in der `Game.log` — über 22 Protokolle nachgemessen:
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
    einheit = tk.StringVar(eltern, value='cscu')
    zeile = tk.Frame(ziel, bg=BG)
    zeile.pack(fill='x', pady=(6, 4))
    tk.Label(zeile, text=t('s_rf_einheit'), bg=BG, fg=SUB,
             font=fenster.f_small).pack(side='left', padx=(0, 8))
    from .main_window import round_select
    # ⚠ Reihenfolge: (eltern, eintraege, gewaehlt, beim_waehlen, schrift).
    round_select(zeile, [('cscu', 'cSCU'), ('scu', 'SCU')], 'cscu',
             lambda k: (einheit.set(k), pruefen()),
             fenster.f_small).pack(side='left')

    # ⭐ **Eigenes Lagerort-Feld.** Das Formular ganz oben steht weit weg;
    # wer den Ort für eine Ausbeute ändern will, müsste sonst hochrollen und
    # danach zurück.
    #
    # Vorbelegt mit dem Ort von oben, für alle, die immer am selben Ort
    # einlagern.
    ort_raff = tk.StringVar(eltern, value=(ort_var.get() or '').strip())
    ortblock = tk.Frame(ziel, bg=BG)
    ortblock.pack(fill='x', pady=(4, 0))
    tk.Label(ortblock, text=t('s_rf_ort'), bg=BG, fg=FG,
             font=fenster.f_bold, anchor='w').pack(fill='x')
    _ozeile, _oliste, _ozeichnen = _combo_box(fenster, ortblock, ort_raff,
                                                _orte_modul.all_places)
    _ozeile.pack(fill='x', pady=(4, 0))
    _oliste.pack(fill='x')

    kasten = round_frame(ziel, theme.FIELD, LINE, radius=8, base_color=BG)
    kasten.holder.pack(fill='x', pady=(4, 6))
    feld = tk.Text(kasten, bg=theme.FIELD, fg=FG, font=('Consolas', 10),
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
        # `storable()`. Ein Ortsname steht dort nie drin, also käme immer
        # `None` zurück, und `or ''` machte daraus einen **leeren Lagerort**:
        # Die ganze Ausbeute würde ohne Ort eingebucht.
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
            paths.set_setting('lager_raffinerie_offen', False)
        else:
            # ⚠ `after=kopf` — sonst haengt der Koerper beim zweiten Aufklappen
            # unter allem, was inzwischen dazugekommen ist, statt unter seiner
            # eigenen Kopfzeile.
            ziel.pack(fill='x', after=kopf)
            pfeil.swap_symbol('zuklappen')
            paths.set_setting('lager_raffinerie_offen', True)

    # Die ganze Kopfzeile ist die Schaltflaeche, nicht nur das Symbol: Ein
    # Pfeil von zwoelf Pixeln ist kein Ziel, das man treffen will.
    for teil in (kopf, pfeil) + tuple(kopf.winfo_children()):
        teil.bind('<Button-1>', _umschalten)
    # ⭐ Der Pfeil hebt sich ab, wenn die Maus die Kopfzeile
    # trifft — anklickbar ist hier die Zeile, nicht der Pfeil.
    icons.hover_group(kopf, pfeil)

    # ⚠⚠ **`setting_bool`, nicht `setting`.** Letztere liefert
    # einen PFAD und ruft dafür `.strip()` auf dem Wert auf. Hier steht aber
    # ein Ja/Nein: Sobald der Block einmal aufgeklappt war, liegt `True` in
    # der Datei — und `True.strip()` wirft einen AttributeError. Der träfe
    # nicht nur diesen Block, sondern risse den Aufbau der GANZEN Lager-Seite
    # ab: Die Liste der eingetragenen Posten fehlte, obwohl die Daten
    # unversehrt sind.
    #
    # ⚠ Und es bliebe kaputt bis zum Neustart — eine Seite wird nur EINMAL
    # gebaut (siehe `open_page()`).
    if paths.setting_bool('lager_raffinerie_offen', False):
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
    schiff = tk.StringVar(rahmen)
    # Die eigenen Namen im Abrufterminal — an jedem Schiff ein Feld.
    namen = _ship_names(fenster, rahmen)

    def neu_zeichnen():
        _liste_fuellen()

    # ------------------------------------------------------------- Import
    tk.Label(innen, text=t('s_hg_import_titel'), bg=BG, fg=FG,
             font=fenster.f_bold, anchor='w').pack(fill='x', padx=24,
                                                   pady=(18, 2))
    _body_text(innen, t('s_hg_import_text'), fenster.f_small, fill='x',
                padx=24, inset=48)
    # ⚠ Bei der Hangar Extension ergänzen sich beide Formate: JSON
    # bringt Kürzel und Paketbeziehung, CSV die Versicherungsdauer.
    _body_text(innen, t('s_hg_import_json'), fenster.f_small, color=GOLD,
                fill='x', padx=24, inset=48)
    _body_text(innen, t('s_hg_import_listen'), fenster.f_small, fill='x',
                padx=24, inset=48)

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
            added, existing, removed = meine.import_entries(eintraege,
                                                           daten['stand'])
            meldung['text'] = t('s_hg_import_ok').format(neu=added, alt=existing)
            if removed:
                meldung['text'] += ' ' + t('s_hg_import_weg').format(
                    anzahl=len(removed), namen=', '.join(removed))
            meldung['farbe'] = ACCENT
            _steckplaetze_holen(still=True)
        neu_zeichnen()

    # ⚠⚠ **Jede Knopfreihe braucht ihren EIGENEN Rahmen** — und die Knöpfe
    # müssen darin sitzen, nicht in der Rollfläche. `_knopfreihe` packt mit
    # `side='left'` in ihr `eltern` und merkt sich dort ihren Umbruch-Zustand
    # (`zuletzt_nebeneinander`). Bekommt sie zweimal dieselbe Fläche, tritt die
    # zweite Reihe der ersten den Zustand weg — dann sind **alle vier Knöpfe
    # unsichtbar**, ohne Fehlermeldung.
    reihe_import = tk.Frame(innen, bg=BG)
    reihe_import.pack(fill='x', padx=24, pady=(10, 0))
    _button_row(reihe_import, [
        _button(fenster, reihe_import, t('s_hg_import_knopf'), importieren,
               strong=True),
        _button(fenster, reihe_import, t('s_hg_erweiterung'),
               lambda: paths.open_in_browser(HANGAR_EXT_PAGE)),
    ])

    # -------------------------------------------------------- Von Hand
    tk.Label(innen, text=t('s_hg_hand_titel'), bg=BG, fg=FG,
             font=fenster.f_bold, anchor='w').pack(fill='x', padx=24,
                                                   pady=(18, 2))
    _body_text(innen, t('s_hg_hand_text'), fenster.f_small, fill='x',
                padx=24, inset=48)
    # ⚠ Sagt, wie das Feld benutzt wird. Ohne diesen Satz haelt man die
    # sichtbare Liste fuer das ganze Angebot.
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
    # Anzuege gar nicht eintragen: Arrow, Gladius, A.T.L.S. IKTI.
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
        # ⚠⚠ **Die Herkunft ist wählbar, nicht geraten** — auch per Echtgeld
        # gekaufte Schiffe lassen sich ohne Hangar-Import eintragen.
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

        ⚠⚠ **Dafür gibt es keinen Knopf.** Ein Knopf, der eine Arbeit
        anbietet, die das Programm längst erledigt, erklärt sich nie — er wirft
        nur die Frage auf, was er soll.

        Geholt wird an drei Stellen von selbst: nach dem Import, nach
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
        alle = (stand.get('schiffe') or [])
        try:
            zuordnung = namen['zuordnen'](alle)
        except Exception as ausnahme:
            errors.record('pages.hangar.namen', ausnahme)
            zuordnung = {}
        # Wie viele Schiffe sich einen Namen teilen — gleicher Schlüssel,
        # gleicher Name im Spiel.
        je_schluessel = {}
        for z in zuordnung.values():
            if z.get('schluessel'):
                je_schluessel[z['schluessel']] = (
                    je_schluessel.get(z['schluessel'], 0) + 1)
        # ⭐ **Das Suchfeld oben filtert auch die eigene Liste**, nicht nur
        # das Angebot zum Eintragen — sonst sähe wer „Ikti" tippt darunter
        # weiter alle Schiffe und hielte die Suche für kaputt. Gesucht wird
        # auch nach dem selbst vergebenen Namen im Spiel.
        suche = (schiff.get() or '').strip()

        def _passt(s):
            if _hangar_matches(s, suche):
                return True
            z = zuordnung.get(s.get('name') or '') or {}
            eigen = namen['eigen'](z.get('schluessel'))
            return bool(suche) and suche.lower() in eigen.lower()

        schiffsliste = [s for s in alle if _passt(s)]
        titel = (t('s_hg_meine_gefiltert').format(n=len(schiffsliste),
                                                  alle=len(alle))
                 if suche and alle else t('s_hg_meine').format(n=len(alle)))
        tk.Label(liste_rahmen, text=titel,
                 bg=BG, fg=FG, font=fenster.f_bold,
                 anchor='w').pack(fill='x', pady=(0, 6))

        if not alle:
            _body_text(liste_rahmen, t('s_hg_leer'), fenster.f_small, fill='x')
            return
        if not schiffsliste:
            _body_text(liste_rahmen, t('s_hg_filter_leer').format(text=suche),
                       fenster.f_small, fill='x')
            return

        version = erkul.game_version()
        _body_text(liste_rahmen,
                    t('s_hg_quelle').format(version=version) if version
                    else t('s_hg_keine_daten'),
                    fenster.f_small, fill='x', pady=(0, 8))

        def name_zeile(karte, eintrag):
            z = zuordnung.get(eintrag.get('name') or '')
            if not z:
                return
            namen['feld'](karte, z,
                          je_schluessel.get(z.get('schluessel') or '', 1))

        ohne = 0
        for eintrag in sorted(schiffsliste,
                              key=lambda s: (s.get('name') or '').lower()):
            ohne += _hangar_row(fenster, liste_rahmen, eintrag, daten,
                                  meldung, neu_zeichnen,
                                  name_line=name_zeile)
        if ohne:
            _body_text(liste_rahmen,
                        t('s_hg_ohne_erklaert').format(n=ohne),
                        fenster.f_small, fill='x', pady=(10, 0))

    _liste_fuellen()

    # Unter der Liste: Stand der Namen im Spiel, Grenzen, Knopf zum Übernehmen.
    namen_fuss = tk.Frame(innen, bg=BG)
    namen_fuss.pack(fill='x', padx=24, pady=(0, 24))
    namen['fuss'](namen_fuss)

    # Beim Tippen neu filtern — gebündelt: Jede Zeile der Liste ist ein
    # ganzer Block mit aufklappbarer Ausstattung, und Tk rechnet für jedes
    # gepackte Kind. Je Tastendruck neu bauen hieße bei 40 Schiffen spürbares
    # Stocken.
    filter_warte = {'id': None}

    def _filter_bald(*_args):
        if filter_warte['id']:
            try:
                liste_rahmen.after_cancel(filter_warte['id'])
            except tk.TclError:
                pass
        try:
            filter_warte['id'] = liste_rahmen.after(250, _filter_jetzt)
        except tk.TclError:
            pass

    def _filter_jetzt():
        filter_warte['id'] = None
        try:
            if liste_rahmen.winfo_exists():
                _liste_fuellen()
        except tk.TclError:
            pass

    schiff.trace_add('write', _filter_bald)

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
                errors.record('pages.hangar.nachziehen', ausnahme)
                return
            if not geholt:
                return
            def zeigen():
                try:
                    if liste_rahmen.winfo_exists():
                        neu_zeichnen()
                except tk.TclError:
                    pass
            _TK_CALLS.put(zeigen)
        threading.Thread(target=arbeit, daemon=True).start()

    _nachziehen_im_hintergrund()


def _wishlist(fenster, rahmen):
    """Schiffe, die man sich vornimmt — mit Preis, Ort und planbarer Ausstattung.

    ⚠⚠ **Eigener Reiter, nicht unten am Hangar.** Dort stünde die
    Wunschliste hinter einer Liste, die bei vierzig Schiffen über mehrere
    Bildschirmhöhen geht, und jedes davon klappt seine Ausstattung auf. Was
    hinter etwas Wachsendem steht, ist irgendwann nicht mehr da.

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
    wunsch = tk.StringVar(rahmen)

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
            # „keine Steckplatz-Daten".
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
    """Die Seite `asop` — ein Verweis auf den Hangar, ohne eigenen Reiter.

    Die eigenen Schiffsnamen stehen direkt an jedem Schiff im Hangar
    (`_ship_names`). Die Kennung `asop` bleibt bestehen: Der Nutzungszähler
    anderer Fassungen meldet sie weiter, und der Empfänger lehnt einen Bericht
    mit unbekannter Seite ganz ab.
    """
    _heading(fenster, rahmen, t('hf_asop'), t('s_as_umgezogen'))
    reihe = tk.Frame(rahmen, bg=BG)
    reihe.pack(fill='x', padx=24, pady=(12, 0))
    _button(fenster, reihe, t('s_as_zum_hangar'),
            lambda: fenster.jump_to('hangar'), strong=True).pack(side='left')


def _ship_names(fenster, rahmen):
    """Eigene Namen für die Schiffe im Fleet Manager (ASOP) — für den Hangar.

    Im Abrufterminal stehen die Werksnamen. Wer drei Abwandlungen derselben
    Reihe hat, sucht dort jedes Mal — und zwar in dem Moment, in dem er sich
    entscheiden muss.

    ⚠⚠ **Ein Name gehört zum Muster, nicht zum einzelnen Schiff.** Zwei
    *gleiche* Hornets bekommen denselben Namen; das Spiel kennt an dieser
    Stelle keinen Unterschied. Im Hangar steht das an jedem Schiff, von dem
    es mehrere gibt — wer es erst im Spiel merkt, hält das Werkzeug für kaputt.

    Gibt ein Wörterbuch zurück:

    | Schlüssel | Inhalt |
    |---|---|
    | `zuordnen(schiffe)` | `{Hangar-Name: Zuordnung}` aus `asop.match_ships` |
    | `feld(karte, zuordnung, anzahl)` | baut Feld und Stern in eine Schiffskarte |
    | `eigen(schluessel)` | der selbst vergebene Name |
    | `fuss(eltern)` | Stand, Hinweise und der Knopf zum Übernehmen |
    """
    from . import asop as asop_modul, fleet as meine, injection

    daten = {'stand': asop_modul.load()}
    alles = {'stempel': None, 'tabelle': {}}
    # Je Schlüssel die Felder, die gerade dastehen — zwei gleiche Schiffe
    # zeigen denselben Namen, also zieht das eine das andere mit.
    felder = {}
    anzeige = {'stand': None}

    def zeilen_der_ini():
        """Die Zeilen der Sprachdatei, die das Spiel gerade lädt — oder nichts."""
        try:
            pfad, _sprachordner, _quelle = injection.ini_file()
            if pfad and os.path.isfile(pfad):
                with open(pfad, encoding='utf-8', errors='ignore') as f:
                    lines = f.read().splitlines()
                # ⚠ Dieselben ergänzten Schiffsnamen wie beim Einspielen:
                # Kennt die Übersetzung ein Schiff noch nicht,
                # steht es hier trotzdem mit seinem genauen Schlüssel da —
                # sonst kürzte die Zuordnung den Namen und traf ein ANDERES
                # Fahrzeug (Sabre Raven EX → Sabre Raven).
                extra = injection._added_ship_names(
                    pfad, lines, injection.load_origtext())
                return lines + ['%s=%s' % kv for kv in extra.items()]
        except Exception as ausnahme:
            errors.record('pages._ship_names.ini', ausnahme)
        return []

    def _stand_der_quellen():
        try:
            pfad = injection.ini_file()[0]
        except Exception:
            pfad = None
        return _files_stamp(pfad, paths.app_file(injection.ORIGTEXT_FILE),
                            paths.app_file(meine.FILE),
                            paths.app_file(asop_modul.FILE))

    def tabelle():
        """Schlüssel → Werksname — nur neu gelesen, wenn sich eine Quelle
        geändert hat. Die Sprachdatei hat zwölf Megabyte."""
        stempel = _stand_der_quellen()
        if stempel != alles['stempel']:
            alles['stempel'] = stempel
            zeilen = zeilen_der_ini()
            alles['tabelle'] = asop_modul.read_keys(zeilen) if zeilen else {}
            daten['stand'] = asop_modul.load()
        return alles['tabelle']

    def zuordnen(schiffe):
        # Ein neuer Aufbau der Liste — die Felder des vorigen sind weg.
        felder.clear()
        tab = tabelle()
        if not tab or not schiffe:
            return {}
        return {e['name']: e for e in asop_modul.match_ships(schiffe, tab)}

    def eigen(schluessel):
        if not schluessel:
            return ''
        return asop_modul.entry(daten['stand'], schluessel)[0]

    def _sagen(text, farbe):
        """Den Stand anzeigen — und stillhalten, wenn die Seite schon weg ist.

        ⚠⚠ Der verzögerte Lauf kann auf ein Fenster treffen, das gerade
        geschlossen wird. Beschriften scheitert dann, und **das darf das
        Schreiben nicht verhindern**: Sonst steht der Name in `asop.json`,
        aber nie in der `global.ini` — und im Spiel bleibt der Werksname
        stehen, ohne jeden Hinweis.
        """
        try:
            if anzeige['stand'] is not None:
                anzeige['stand'].configure(text=text, fg=farbe)
        except Exception:
            pass

    def einspielen():
        """Die Namen in die Sprachdatei schreiben — über den üblichen Weg."""
        warte['id'] = None
        _sagen(t('s_as_laeuft'), SUB)
        try:
            if anzeige['stand'] is not None:
                anzeige['stand'].update_idletasks()
        except Exception:
            pass
        try:
            pfad, sprachordner, _quelle = injection.ini_file()
            if not pfad:
                _sagen(t('s_as_keine_ini'), RED)
                return
            ok, _anzahl, text = injection.refresh(pfad, sprachordner)
        except Exception as ausnahme:
            errors.record('pages._ship_names.einspielen', ausnahme)
            ok, text = False, str(ausnahme)
        _sagen(t('s_as_steht') if ok else (t('s_as_schief') % text),
               ACCENT if ok else RED)

    # ⚠⚠ **Kein Knopf zum Schreiben nötig.** Wer einen Namen eintippt, hat ihn
    # vergeben — und erwartet ihn im Spiel. Die Seite schreibt deshalb von
    # selbst (gemessen 0,28 s für die ganze 12-MB-Datei), gesammelt über
    # `after`: Wer fünf Schiffe hintereinander benennt, löst einen Lauf aus.
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

    def sichern():
        """Merken — und gleich dafür sorgen, dass es auch im Spiel ankommt."""
        if not asop_modul.save(daten['stand']):
            _sagen(t('s_as_nicht_gespeichert'), RED)
            return False
        spaeter_einspielen()
        return True

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

    def feld(karte, zuordnung, anzahl):
        """Feld und Stern in eine Schiffskarte des Hangars."""
        felder.setdefault(zuordnung.get('schluessel') or '', [])
        _asop_row(fenster, karte, zuordnung, daten, asop_modul, sichern,
                  in_card=True, count=anzahl, siblings=felder)

    def fuss(eltern):
        """Stand, Grenzen und der Knopf — unter der Schiffsliste."""
        tk.Label(eltern, text=t('s_hg_namen_titel'), bg=BG, fg=FG,
                 font=fenster.f_bold, anchor='w').pack(fill='x', pady=(18, 2))
        _body_text(eltern, t('s_as_lead'), fenster.f_small, fill='x')
        anzeige['stand'] = tk.Label(eltern, text='', bg=BG, fg=SUB,
                                    font=fenster.f_small, anchor='w',
                                    justify='left')
        anzeige['stand'].pack(fill='x', pady=(4, 0))
        if not tabelle():
            # ⚠ Ehrlich statt leer: Ohne Sprachdatei gibt es nichts zu
            # benennen, und das ist kein Fehler des Nutzers.
            _sagen(t('s_as_keine_ini'), SUB)
        _body_text(eltern, t('s_as_grenze'), fenster.f_small, fill='x',
                   pady=(6, 0))
        _body_text(eltern, t('s_as_patch_hinweis'), fenster.f_small, fill='x',
                   pady=(6, 0))
        # ⚠ Der Knopf bleibt trotz Selbstschreiben: Nach einem Spiel-Patch
        # hat sich an den Namen nichts geändert, die Sprachdatei ist aber
        # neu — dann gibt es nichts, was ein Selbstschreiben auslösen könnte.
        reihe = tk.Frame(eltern, bg=BG)
        reihe.pack(fill='x', pady=(10, 0))
        _button(fenster, reihe, t('s_as_einspielen'), einspielen).pack(
            side='left')

    return {'zuordnen': zuordnen, 'feld': feld, 'eigen': eigen,
            'fuss': fuss, 'einspielen': einspielen}


def _asop_row(fenster, eltern, e, daten, asop_modul, sichern, in_card=False,
              count=1, siblings=None):
    """Eine Schiffszeile: Werksname, Eingabefeld, Stern.

    `in_card=True` baut nur Beschriftung, Feld und Stern in eine schon
    stehende Schiffskarte des Hangars. `count` ist die Zahl gleicher Schiffe
    im Hangar; `siblings` sammelt je Schlüssel die Felder, damit gleiche
    Schiffe denselben Namen zeigen.

    ⚠ Die Reihenfolge ist überall dieselbe — Beschriftung links, Bedienelement
    rechts. Ein Schalter, der auf einer Seite mittig steht und auf der nächsten
    rechts, sieht nach Zufall aus.
    """
    if in_card:
        kasten = tk.Frame(eltern, bg=SURFACE)
        kasten.pack(fill='x', padx=16, pady=(0, 10))
        tk.Label(kasten, text=t('s_hg_name_feld'), bg=SURFACE, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(fill='x')
        innen_pad = 0
    else:
        kasten = tk.Frame(eltern, bg=SURFACE)
        kasten.pack(fill='x', pady=(0, 6))
        innen_pad = 12
        kopf = tk.Frame(kasten, bg=SURFACE)
        kopf.pack(fill='x', padx=12, pady=(8, 2))
        tk.Label(kopf, text=e['name'], bg=SURFACE, fg=FG, font=fenster.f_bold,
                 anchor='w').pack(side='left')
        if e['werksname']:
            tk.Label(kopf, text=e['werksname'], bg=SURFACE, fg=SUB,
                     font=fenster.f_small, anchor='w').pack(side='left',
                                                            padx=(10, 0))

    if not e['schluessel']:
        tk.Label(kasten, text=t('s_as_zeile_ohne'), bg=SURFACE, fg=SUB,
                 font=fenster.f_small, anchor='w').pack(fill='x',
                                                        padx=innen_pad,
                                                        pady=(0, 8))
        return

    eigen, stern = asop_modul.entry(daten['stand'], e['schluessel'])
    wert = tk.StringVar(eltern, value=eigen)
    stern_an = tk.BooleanVar(eltern, value=stern)
    if siblings is not None:
        siblings.setdefault(e['schluessel'], []).append(wert)

    reihe = tk.Frame(kasten, bg=SURFACE)
    reihe.pack(fill='x', padx=innen_pad, pady=(2 if in_card else 0, 10))

    from .main_window import round_entry
    feld = round_entry(reihe, wert, fenster.f_small, theme.FIELD, LINE, ACCENT, FG)
    feld.holder.pack(side='left', fill='x', expand=True)

    if count > 1:
        # ⚠⚠ Das Spiel unterscheidet gleiche Schiffe nicht — der Name gilt
        # für alle. Das steht hier, nicht erst im Spiel.
        alle_lbl = tk.Label(kasten, text=t('s_hg_name_alle') % (
            count, e.get('werksname') or e['name']), bg=SURFACE, fg=GOLD,
            font=fenster.f_small, anchor='w', justify='left')
        alle_lbl.pack(fill='x', padx=innen_pad, pady=(0, 6))
        _wrap(alle_lbl, inset=64)

    def uebernehmen(*_):
        asop_modul.set_name(daten['stand'], e['schluessel'], wert.get(),
                          stern_an.get())
        # Gleiche Schiffe zeigen denselben Namen — sofort, nicht erst beim
        # nächsten Aufbau.
        for andere in (siblings or {}).get(e['schluessel'], []):
            if andere is not wert:
                try:
                    if andere.get() != wert.get():
                        andere.set(wert.get())
                except tk.TclError:
                    pass
        sichern()

    # ⚠⚠ **Kein `tk.Checkbutton`** — zu klein und anders als der Rest:
    # Tk malt sein Kästchen im Systemstil, also hell, winzig und
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

    ⭐⭐ **Der Reiter, der die ganze Ausstattungs-Arbeit zusammenführt.** Wer
    wissen will, was sein nächster Ausflug zum Händler insgesamt kostet,
    müsste sonst vierzig Karten aufklappen und im Kopf addieren. Komponenten
    und Schiffe, mit Gesamtpreis und Einzelaufstellung wie auf einer
    Rechnung.

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
        # das.** Abgehakte Zeilen unter „0 Positionen" wären Unfug.
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
        # geplant war.** Abgehakte Posten zählen nicht mit. Eine Überschrift,
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
    ansetzt:** Wie viel Material gibt eine Waffe oder Komponente zurück, wenn
    man sie im Fabricator zerlegt — lohnt sich das Ausbauen?

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

    gewaehlt = tk.StringVar(rahmen)
    block = tk.Frame(innen, bg=BG)
    block.pack(fill='x', padx=24, pady=(14, 0))

    ergebnis = tk.Frame(innen, bg=BG)
    ergebnis.pack(fill='x', padx=24, pady=(14, 20))

    def namen():
        try:
            return sorted((e.get('basis') or '') for e in crafting.all_items()
                          if e.get('basis'))
        except Exception as ausnahme:
            errors.record('pages.zerlegen.namen', ausnahme)
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
    brauchen und was es kostet; hier steht, was davon noch **im Boden** liegt
    — gerechnet gegen das eigene Lager.

    ⚠⚠ **Hier in der Werkstatt und nicht bei den Schiffen.** Die
    Werkstatt-Kette lautet „welches Material ist da →
    was wird gebaut → wo gibt es das", und eine Liste fehlender Rohstoffe ist die
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

        ⚠ **Auch bei leerem Merkzettel wird der Satz gezeigt** — er sagt, wo
        der Knopf sitzt. Ein leerer Bereich ohne Erklärung lässt einen ratlos
        zurück.
        """
        eintraege = meine.notepad()
        rahmen_mz = tk.Frame(koerper, bg=BG)
        rahmen_mz.pack(fill='x', pady=(18, 0))
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

        def _set_count(name, count):
            # Die Summe oben rechnet mit der Stückzahl — nach jeder Änderung
            # wird die ganze Seite neu gerechnet.
            stand = meine.load()
            if meine.notepad_set_count(stand, name, count):
                meine.save(stand)
            neu_zeichnen()

        # ⚠⚠ **Das Material gehört an den Eintrag, nicht nur in die Summe
        # darüber.** Die Summe beantwortet
        # „wie viel Erz brauche ich insgesamt", hier steht „und wofür".
        from . import crafting as _mz_herst
        # ⚠⚠⚠ **Die Fehlmengen kommen aus DERSELBEN Rechnung wie die Summe
        # darüber.** Das Lager direkt zu fragen ergäbe hier den vollen
        # Lagerbestand und in der Summe den für diesen Bedarf zugeteilten
        # Anteil — zwei Zahlen mit derselben Beschriftung auf einer Seite.
        #
        # Der Einzelposten sagt nur, **was er braucht**; ob es reicht, sagt
        # die Farbe, und die stammt aus der Gesamtrechnung. Eine Seite, eine
        # Wahrheit.
        # ⚠⚠⚠ **Die Rechnung wird ÜBERGEBEN, nicht neu angestellt.** Ein
        # zweiter Aufruf von `cart.farm_list()` rechnete dasselbe über alle
        # Schiffe und Rezepte noch einmal — gemessen **3937 ms** statt 60.
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
            _count_stepper(fenster, zeile, menge,
                           lambda neu, n=e.get('name'): _set_count(n, neu))
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
                    # derselben Rechnung wie die Summe darüber.
                    #
                    # ⚠ `_menge_text()` statt `%.2f`: Deutsche Zahlen haben
                    # ein Komma. Sonst stünde hier „4.64" und in der Summe
                    # „8,8" auf derselben Seite.
                    tk.Label(zutat,
                             text=t('s_mz_braucht') % _amount_text(braucht),
                             bg=BG, fg=GOLD if knapp else ACCENT,
                             font=fenster.f_small,
                             anchor='w').pack(side='left')

    def _aufbauen():
        values = cart.farm_list(meine.load())
        missing = values.get('fehlt') or []
        sufficient = values.get('vollstaendig') or []
        count = values.get('posten') or 0

        # ⭐ **Erst die Summe, dann die Posten.** Die Summe beantwortet die
        # Frage der Seite — was muss ich farmen. Die vorgemerkten Bauteile
        # darunter sagen nur, wofür.
        #
        # ⚠⚠ **Drei Lagen, drei Sätze** — dieselbe Falle wie überall hier:
        # „nichts geplant", „alles da" und „nichts zu tun" sehen im Code gleich
        # aus. Wer sie zusammenwirft, sagt jemandem ohne Plan, er sei fertig.
        if not count:
            _body_text(koerper, t('s_fl_nichts_geplant'), fenster.f_small,
                        fill='x')
            _merkzettel_block(values)
            return

        tk.Label(koerper, text=t('s_fl_kopf').format(n=count), bg=BG, fg=FG,
                 font=fenster.f_bold, anchor='w').pack(fill='x', pady=(0, 8))

        if not missing:
            _body_text(koerper, t('s_fl_alles_da'), fenster.f_small,
                        color=ACCENT, fill='x')
        else:
            # ⭐ Wo es das Fehlende gibt — einmal für alle Zeilen gerechnet.
            per_material, gathering = farm_locations(
                [entry.get('rohstoff') or '' for entry in missing])
            for entry in missing:
                _farm_row(fenster, koerper, entry, fehlend=True,
                          spots=per_material.get(entry.get('rohstoff') or ''))
            _farm_gathering(fenster, koerper, gathering)

        if sufficient:
            tk.Label(koerper, text=t('s_fl_reicht_kopf').format(n=len(sufficient)),
                     bg=BG, fg=SUB, font=fenster.f_small,
                     anchor='w').pack(fill='x', pady=(14, 4))
            for entry in sufficient:
                _farm_row(fenster, koerper, entry, fehlend=False)

        # ⚠ Was gar nicht gerechnet werden konnte, wird genannt — eine Liste,
        # der stillschweigend Posten fehlen, ist schlimmer als eine kurze.
        without_recipe = values.get('ohne_rezept') or []
        if without_recipe:
            _body_text(koerper,
                        t('s_fl_ohne_rezept').format(n=len(without_recipe),
                                                     teile=', '.join(without_recipe)),
                        fenster.f_small, color=GOLD, fill='x', pady=(12, 0))

        _merkzettel_block(values)

    fenster.on_show['farmliste'] = neu_zeichnen
    _aufbauen()


def _count_stepper(window, row, count, on_change):
    """Minus, Stückzahl, Plus — rechts in einer Zeile, vor dem Streichen-Knopf.

    ⚠ Bei eins ist das Minus grau und tut nichts. Weg kommt ein Posten nur
    über den Streichen-Knopf — ein Klick zu viel auf Minus soll ihn nicht
    löschen.
    ⚠ Von rechts nach links gepackt: Plus, Zahl, Minus. So steht es da als
    Minus, Stückzahl, Plus.
    """
    from . import notice
    plus = icons.tappable(row, 'stueck_mehr', color=icons.GREEN, background=BG)
    plus.configure(cursor='hand2', padx=4)
    plus.pack(side='right')
    plus.bind('<Button-1>', lambda _e: on_change(count + 1))
    icons.hover_group(plus)
    notice.attach(plus, lambda: t('s_mz_mehr'))

    tk.Label(row, text='%d %s' % (count, t('s_mz_stueck')), bg=BG, fg=FG,
             font=window.f_small, anchor='center').pack(side='right', padx=2)

    can_lower = count > 1
    minus = icons.tappable(row, 'stueck_weniger',
                           color=icons.GREEN if can_lower else icons.GREY,
                           background=BG)
    minus.configure(padx=4)
    minus.pack(side='right', padx=(8, 0))
    if can_lower:
        minus.configure(cursor='hand2')
        minus.bind('<Button-1>', lambda _e: on_change(count - 1))
        icons.hover_group(minus)
    notice.attach(minus, lambda: t('s_mz_weniger' if can_lower
                                   else 's_mz_weniger_aus'))
    return minus, plus


def _farm_row(fenster, eltern, eintrag, fehlend, spots=None):
    """Ein Rohstoff: was gebraucht wird, was da ist, was fehlt — und wo es liegt."""
    block = tk.Frame(eltern, bg=SURFACE)
    block.pack(fill='x', pady=(0, 2))
    row = tk.Frame(block, bg=SURFACE)
    row.pack(fill='x')

    def to_mining(_event=None, name=eintrag.get('rohstoff') or ''):
        fenster.mining_search = name
        fenster.jump_to('bergbau')

    # ⭐ Der Name selbst führt in den Bergbau — der Rohstoff steht dort
    # aufgeklappt, mit allen Fundorten.
    name_farbe = FG if fehlend else SUB
    name_label = tk.Label(row, text=eintrag.get('rohstoff') or '', bg=SURFACE,
                          fg=name_farbe, font=fenster.f_small, anchor='w',
                          width=24, cursor='hand2')
    name_label.pack(side='left', padx=(12, 0), pady=4)
    name_label.bind('<Button-1>', to_mining)
    name_label.bind('<Enter>', lambda e: name_label.configure(fg=ACCENT))
    name_label.bind('<Leave>',
                    lambda e: name_label.configure(fg=name_farbe))
    from . import notice
    notice.attach(name_label, lambda: t('s_fl_zum_bergbau'))

    # ⚠ **Die Fehlmenge steht rechts und in Farbe** — das ist die Zahl, mit der
    # man losfliegt. „Brauchst 4,4 · hast 0" daneben sagt, wie sie zustande
    # kommt; ohne sie wäre die Zahl eine Behauptung.
    if fehlend:
        tk.Label(row, text=t('s_fl_fehlt').format(
            menge=_number(eintrag.get('differenz'))),
            bg=SURFACE, fg=GOLD, font=fenster.f_small,
            anchor='e').pack(side='right', padx=(0, 12))
    else:
        tk.Label(row, text=t('s_fl_genug'), bg=SURFACE, fg=ACCENT,
                 font=fenster.f_small, anchor='e').pack(side='right',
                                                        padx=(0, 12))

    tk.Label(row, text=t('s_fl_stand').format(
        braucht=_number(eintrag.get('benoetigt')),
        hat=_number(eintrag.get('vorhanden'))),
        bg=SURFACE, fg=SUB, font=fenster.f_small,
        anchor='w').pack(side='left')

    # ⚠ **Zu geringe Güte ist kein Bestand, aber auch kein Nichts.** Wer 20
    # Stileron mit Q 100 im Lager hat und Q 500 braucht, soll das erfahren —
    # sonst sucht er im Lager nach etwas, das dort sichtbar liegt, und
    # versteht die Meldung nicht.
    too_low = eintrag.get('zu_gering') or 0
    if too_low:
        tk.Label(row, text=t('s_fl_zu_gering').format(
            menge=_number(too_low),
            guete=_number(eintrag.get('mindestguete'))),
            bg=SURFACE, fg=SUB, font=fenster.f_small,
            anchor='w').pack(side='left', padx=(10, 0))

    # ⭐ **Wo es liegt** — die ergiebigsten Fundorte, ein Klick öffnet den
    # Bergbau mit diesem Rohstoff (derselbe Sprung wie aus dem Rezept).
    if spots:
        spot_label = tk.Label(block, text=t('s_fl_fundorte') % _farm_spot_text(spots),
                              bg=SURFACE, fg=ACCENT, font=fenster.f_small,
                              anchor='w', justify='left', cursor='hand2',
                              wraplength=700)
        spot_label.pack(fill='x', padx=(24, 12), pady=(0, 4))
        spot_label.bind('<Button-1>', to_mining)


def _farm_gathering(fenster, parent, gathering):
    """„Wo du das meiste auf einmal findest" — Orte mit mehreren fehlenden Erzen.

    ⭐ Die Antwort auf die Routenfrage: Wer drei Erze braucht, will wissen, wo
    er zwei davon an einem Ort bekommt. Ein Klick öffnet den Bergbau mit dem
    Ort in der Suche — dort steht, was es dort sonst noch gibt.
    """
    if not gathering:
        return
    tk.Label(parent, text=t('s_fl_sammel_kopf'), bg=BG, fg=FG,
             font=fenster.f_bold, anchor='w').pack(fill='x', pady=(16, 2))
    _body_text(parent, t('s_fl_sammel_hilfe'), fenster.f_small, fill='x')
    for place, system, materials in gathering:
        row = tk.Frame(parent, bg=SURFACE, cursor='hand2')
        row.pack(fill='x', pady=(4, 0))
        title = tk.Label(row, text='%s (%s)' % (place, system) if system else place,
                         bg=SURFACE, fg=FG, font=fenster.f_small, anchor='w',
                         cursor='hand2')
        title.pack(side='left', padx=(12, 0), pady=4)
        found = tk.Label(row, text=t('s_fl_sammel_zeile').format(
                             n=len(materials), erze=', '.join(materials)),
                         bg=SURFACE, fg=SUB, font=fenster.f_small, anchor='w',
                         cursor='hand2')
        found.pack(side='left', padx=(10, 12))

        def to_place(_event=None, name=place):
            fenster.mining_search = name
            fenster.jump_to('bergbau')

        for widget in (row, title, found):
            widget.bind('<Button-1>', to_place)


def farm_locations(needed, ores=None, per_ore=3, places=5):
    """Wo die fehlenden Rohstoffe liegen — je Rohstoff und als Sammelorte.

    ⭐⭐ Was man farmen muss **und wo** man es am besten findet — die
    Fundorte kommen aus denselben Daten wie auf der Bergbau-Seite.

    `needed` sind Rohstoffnamen aus den Rezepten (`Titanium`), `ores` die Liste
    aus `mining.ores()` (`Titanium (Ore)`); verglichen wird über
    `mining.norm_material`, sonst findet sich nichts (siehe `locations_for`).

    Gibt `(je_rohstoff, sammelorte)` zurück:

    * `je_rohstoff`: `{Rohstoff: [(Ort, System, Anteil), …]}`, die ergiebigsten
      zuerst, höchstens `per_ore`. Ein Rohstoff, der nirgends abzubauen ist,
      fehlt darin — er ist dann nicht „unbekannt", sondern schlicht kein Erz.
    * `sammelorte`: `[(Ort, System, [Rohstoffe])]` — nur Orte mit **mindestens
      zwei** der fehlenden Rohstoffe, die mit den meisten zuerst, bei
      Gleichstand der mit dem höheren Anteil. Ein Ort mit nur einem Erz ist
      keine Route, das steht schon an der Zeile.

    Frei von Tk, damit es sich prüfen lässt.
    """
    from . import mining
    if ores is None:
        try:
            ores = mining.ores()
        except Exception as exc:
            errors.record('pages.farm_locations', exc)
            ores = []
    by_norm = {mining.material_key(o.get('name') or ''): o for o in ores}
    per_material = {}
    collected = {}
    for material in needed:
        ore = by_norm.get(mining.material_key(material))
        if not ore:
            continue
        spots = ore.get('orte') or []
        per_material[material] = [(s[0], s[1], s[3] if len(s) > 3 else 0.0)
                                  for s in spots[:per_ore]]
        for spot in spots:
            share = spot[3] if len(spot) > 3 else 0.0
            entry = collected.setdefault((spot[0], spot[1]), {})
            entry[material] = share
    gathering = [(place, system, sorted(found, key=str.lower))
                 for (place, system), found in collected.items()
                 if len(found) >= 2]
    gathering.sort(key=lambda g: (-len(g[2]),
                                  -sum(collected[(g[0], g[1])].values()),
                                  g[0].lower()))
    return per_material, gathering[:places]


def _farm_spot_text(spots):
    """„Ort (System) 25 % · Ort 12 %" — kurz genug für eine Zeile."""
    parts = []
    for place, system, share in spots:
        percent = round((share or 0) * 100)
        amount = (t('s_bg_anteil_wenig') if percent < 1
                  else t('s_bg_anteil') % percent)
        parts.append('%s (%s) %s' % (place, system, amount) if system
                     else '%s %s' % (place, amount))
    return ' · '.join(parts)


def _number(value):
    """Eine Menge lesbar — ganze Zahlen ohne Komma, kleine Mengen genauer.

    ⚠ `4.4000000000000004` ist dieselbe Zahl wie `4,4`, sieht aber nach einem
    Fehler aus. Und `4,0` neben `4` in derselben Spalte liest sich, als wären
    es zwei verschiedene Angaben.

    ⚠⚠ **Unter 1 braucht es zwei Stellen.** Mit einer würde aus 0,64 ein
    „0,6" und aus 0,32 ein „0,3". Bei Rohstoffmengen, die
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
    # mit vierzig Positionen ist der Unterschied zwischen „im Hangar" und
    # „auf der Wunschliste" die wichtigste Angabe überhaupt.
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

    # ⭐⭐ **Der Haken ist die einzige Möglichkeit, das zu wissen.** Ob etwas
    # gekauft und eingebaut ist, erfährt die Einkaufsliste sonst nicht —
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
                errors.record('pages.einkauf.preis', ausnahme)
        if geholt:
            _from_thread(widget, neu_zeichnen)

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

    # ⚠⚠ **Auch ein Wunschschiff lässt sich ausstatten.** Genau hier ist die
    # Planung am meisten wert — vor
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


def _hangar_matches(eintrag, suche):
    """Passt ein eigenes Schiff zum Suchtext?

    ⚠ **Jedes Wort für sich, über Name, Hersteller und Kurzname.** Die
    Auswahlliste darüber führt die UEX-Schreibweise mit Hersteller
    (`Argo ATLS IKTI`), der Hangar oft den Pledge-Namen ohne ihn (`ATLS IKTI
    Akuma`, Hersteller `ARGO`). Ein Teiltext über den ganzen Namen fände das
    eigene Schiff nach dem Auswählen nicht mehr — genau in dem Moment, in dem
    man es sucht. Punkte und Bindestriche zählen nicht (`ATLS` = `A.T.L.S.`).
    """
    worte = [_squashed(w) for w in (suche or '').split()]
    worte = [w for w in worte if w]
    if not worte:
        return True
    # ⚠ Das Pledge-PAKET nicht: Es nennt alles, was mit im Kauf war — die
    # F7C-M erschien sonst bei „ATLS", weil ihr Paket einen ATLS enthielt.
    teile = [str(eintrag.get(f) or '') for f in _HANGAR_SUCHFELDER]
    heuhaufen = _squashed(' '.join(teile))
    return all(w in heuhaufen for w in worte)


# Datenschluessel der Hangar-Eintraege, in denen `_hangar_matches` sucht —
# keine Oberflaechentexte.
_HANGAR_SUCHFELDER = ('name', 'hersteller', 'hkurz', 'kurz')


def _hangar_row(fenster, eltern, eintrag, daten, meldung, neu_zeichnen,
                name_line=None):
    """Eine Schiffszeile. Gibt `1` zurück, wenn Steckplätze fehlen.

    `name_line(karte, eintrag)` setzt das Feld für den Namen im Spiel in die
    Karte, unter die Angaben und vor die Ausstattung.
    """
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
    teile = [{meine.PLEDGE: t('s_hg_pledge'),
              meine.BASETOOL: t('s_hg_basetool'),
              meine.LIST: t('s_hg_liste', eintrag.get('quelle') or '?')}.get(
                  eintrag.get('herkunft'), t('s_hg_ingame'))]
    if eintrag.get('lti'):
        teile.append(t('s_hg_lti'))
    else:
        # Versicherungsdauer aus dem CSV der Hangar Extension: „120 Month
        # Insurance" → „10 Jahre Versicherung", „6 Month" → „6 Monate".
        monate = int(eintrag.get('versicherung') or 0)
        if monate >= 12 and monate % 12 == 0:
            teile.append(t('s_hg_vers_jahre').format(n=monate // 12))
        elif monate > 0:
            teile.append(t('s_hg_vers_monate').format(n=monate))
    # Kam das Schiff mit einem anderen aus dem Hangar (URSA der Carrack)?
    # Steht nur da, wenn das Paket ein eigenes Schiff nennt — Pledge-Namen
    # des XPLORer („Standalone Ship") bleiben weg.
    beilage = meine.bundled_with(daten['stand'], eintrag)
    if beilage:
        teile.append(t('s_hg_beilage').format(schiff=beilage))

    # ⚠⚠ **Drei Zustände, nicht zwei** — und der Unterschied ist der ganze
    # Punkt. Fehlende Steckplatz-Daten heißen nicht „noch nicht im Spiel":
    # Ein Schiff kann längst fliegen und nur nicht zugeordnet sein.
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

    # ⭐⭐ **Offene Posten sieht man, ohne aufzuklappen.** Bei vierzig
    # Schiffen klappt niemand alle auf, und was man aufklappen muss, um es zu
    # finden, findet man nicht.
    #
    # ⚠ In der Markenfarbe, nicht in Grau — wie bei „passt in dein Schiff":
    # Grau wird übersehen.
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
    if name_line is not None:
        try:
            name_line(karte, eintrag)
        except Exception as ausnahme:
            errors.record('pages.hangar.namensfeld', ausnahme)
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
        die Zeile behauptete weiter, es seien vier offen.
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
    # seinem Schiff sei alles in Ordnung.
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

    # ⚠ **Zählt, was noch zu tun ist** — wie der Kopf der Sammelliste und
    # die Marke in der Zeile. Abgehakte Posten zählen nicht.
    noch_offen = [x for x in liste if not x.get('erledigt')]
    fertig = [x for x in liste if x.get('erledigt')]

    # ⚠⚠ **Eingebautes steht nicht mehr im Warenkorb** — das wäre
    # verschenkter Platz. Ein Korb zeigt, was noch hineingehört.
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
    `NICHT_GEPRUEFT` heißt wörtlich „noch niemand hat nachgesehen" — und
    ohne diesen Abruf sähe auch niemand nach. Der Satz wäre wahr und
    trotzdem eine Sackgasse. Die Bergungs-Seite holt ihre Preise genauso
    selbst nach.

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
                errors.record('pages.cart.preis', ausnahme)
        if geholt:
            # ⚠ Zurück in den Oberflächen-Faden. Tk aus einem Thread heraus
            # anzufassen ist der Weg in Abstürze, die sich nicht nachstellen
            # lassen.
            _from_thread(widget, neu_zeichnen)

    threading.Thread(target=arbeit, daemon=True).start()


def _notice(window, title, text):
    """Ein Hinweis im Programmstil — **nie** der System-Dialog von Tk.

    ⚠⚠ **Warum das keine Stilfrage ist.** Ein `messagebox.showinfo` bringt
    zwei Fehler auf einmal mit:

    1. **Er sieht fremd aus** — heller Kasten mit Systemschrift mitten in einem
       dunklen Programm, und seine Knöpfe kommen in der Systemsprache, nicht in
       der eingestellten.
    2. **Er erscheint irgendwo** — Tk setzt ihn nicht über das Elternfenster.
       Landet er unten am Bildschirmrand, wo niemand hinsieht, nimmt er als
       **modales** Fenster die ganze Oberfläche mit: Die Gruppen in der
       Seitenleiste lassen sich nicht mehr auf- und zuklappen. Das sieht nach
       einem kaputten Menü aus und ist ein unsichtbares Fenster.

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
    Namensliste nicht, was er da anklickt.

    ⚠ **Die Klasse wird nachgeschlagen, wenn sie fehlt** — UEX führt sie
    neben der Güte. Kommt sie mit, greift der direkte Weg und der Nachschlag
    entfällt von selbst — er steht hier, damit nicht zwei Stellen dieselbe
    Filterung doppeln.

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
            errors.record('pages.teil_nachschlagen', ausnahme)
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

    gewaehlt = cart.loadout(eintrag)
    # ⚠ Mit der eigenen Auslegung: Nach einem Rack-Tausch stehen die Plätze
    # des neuen Racks da (20 × Bombe S3 statt 3 × Torpedo S9).
    plaetze = erkul.hardpoints(eintrag.get('name') or '',
                                 eintrag.get('hersteller') or '',
                                 eintrag.get('kurz') or '',
                                 eintrag.get('hkurz') or '', chosen=gewaehlt)

    tk.Label(eltern, text=t('s_wk_auslegung'), bg=SURFACE, fg=FG,
             font=fenster.f_bold, anchor='w').pack(fill='x', padx=(46, 16),
                                                   pady=(0, 4))

    # ⚠⚠ **Gleiche Plätze werden gebündelt.** Eine Cutlass Black hat sechzehn
    # Raketenplätze, alle mit derselben Ignite II ab Werk — sechzehn identische
    # Zeilen untereinander sind keine Liste, sondern eine Wand.
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
    # Klick eine Teileauswahl auf — ohne Pfeil wäre das nur am Mauszeiger
    # erkennbar, wenn man zufällig darüberfährt.
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
        # Kommentar mit dem Suchmuster darin löst sie genauso aus.
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
        # ⚠ Der Platz selbst geht mit: Racks, Raketen, Bomben und
        # Lackierungen passen nach seinen Kennzeichnungen, nicht nach der
        # Größe allein (etwa die Bomben-Racks der Eclipse).
        moeglich = cart.choices(platz.get('art'), platz.get('groesse'), platz)
        if not moeglich:
            # ⚠ Ehrlich statt hübsch: Wenn zu diesem Platz keine kaufbaren
            # Teile bekannt sind, wird das gesagt — nicht der halbe Katalog
            # angeboten, aus dem nichts passt.
            _body_text(auswahl_rahmen, t('s_wk_kein_preis'), fenster.f_small,
                        bg=SURFACE, fill='x', padx=(22, 0), inset=90)
            return
        nach_name = dict((m['name'], m) for m in moeglich)
        gewaehlt_var = tk.StringVar(auswahl_rahmen)

        def uebernehmen(*args):
            # ⚠⚠ **Der Name kommt als Argument, nicht aus dem Feld.** Wer
            # `beim_waehlen` an `_auswahlfeld` übergibt, bekommt den Namen
            # gereicht — das Eingabefeld wird dann absichtlich **nicht**
            # befüllt (siehe `waehlen()` dort, der Verkaufs-Reiter braucht das
            # so). Wer trotzdem aus dem Feld liest, liest eine leere
            # Zeichenkette: Der Klick auf ein Teil täte schlicht nichts, ohne
            # Fehler und ohne Meldung.
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

    ⚠⚠⚠ **Nie nur die Schiffsliste schreiben.** `{'format': …, 'schiffe': …}`
    **ohne `wunsch`** löschte bei jeder Änderung an der Ausstattung eines
    Schiffs die komplette Wunschliste. Und ein **Wunsch**-Eintrag, der nur in
    `schiffe` gesucht wird, wird dort nie gefunden — seine Änderung ginge
    ebenfalls verloren.

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

    karte = tk.Frame(eltern, bg=theme.FIELD)
    karte.pack(fill='x', padx=((46, 16) if eingerueckt else (0, 0)),
               pady=(0, 6))

    fertig = bool(posten.get('erledigt'))
    kopf = tk.Frame(karte, bg=theme.FIELD)
    kopf.pack(fill='x', padx=12, pady=(8, 2))

    # ⭐⭐ **Der Haken sitzt am Posten selbst, nicht nur auf der Sammelliste.**
    # Genau hier steht man vor dem Schiff und sieht seine Plätze — hier fällt
    # einem ein, dass das Teil längst drin ist, nicht zwei Reiter weiter.
    def abhaken(_e=None):
        if cart.set_done(eintrag, posten['pfad'], not fertig):
            _save_entry(eintrag)
            neu_zeichnen()

    haken = icons.line(kopf, 'haken', background=theme.FIELD,
                          color=icons.GREEN if fertig else icons.GREY,
                          font=fenster.f_small)
    haken.configure(cursor='hand2')
    haken.pack(side='left', padx=(0, 8))
    haken.bind('<Button-1>', abhaken)

    # ⚠ Erledigtes tritt zurück, bleibt aber lesbar — wer versehentlich
    # abhakt, muss die Zeile wiederfinden.
    tk.Label(kopf, text=posten.get('name') or '', bg=theme.FIELD,
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
    tk.Label(kopf, text=hinweis, bg=theme.FIELD,
             fg=ACCENT if fertig else SUB, font=fenster.f_small,
             anchor='w').pack(side='left', padx=(8, 0))
    for teil in kopf.winfo_children():
        teil.bind('<Button-1>', abhaken)

    # ⚠ Ist der Posten erledigt, stehen die beiden Wege nicht mehr da: Die
    # Frage „kaufen oder bauen" ist beantwortet, sobald das Teil drin ist.
    if fertig:
        tk.Frame(karte, bg=theme.FIELD, height=6).pack()
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
        zeile = tk.Frame(karte, bg=theme.FIELD)
        zeile.pack(fill='x', padx=12, pady=(0, 4))

        # ⚠ Der gewählte Weg ist in der Markenfarbe hervorgehoben — nicht durch
        # einen Schiebeschalter, der bei zwölf Posten zwölfmal dastünde.
        aktiv = posten.get('weg') == weg
        tk.Label(zeile, text=t(schluessel), bg=theme.FIELD,
                 fg=ACCENT if aktiv else SUB,
                 font=fenster.f_bold if aktiv else fenster.f_small,
                 anchor='w', width=18).pack(side='left')

        if angabe.get('zustand') == cart.KNOWN:
            if weg == cart.BUY:
                # ⚠⚠ **Der Ort steht oft schon im Ladennamen.** UEX schreibt
                # ihn dort mit hinein: Laden „Ship Weapons - Pyro Gateway
                # (Stanton)", Ort „Pyro Gateway (Stanton)". Beides
                # aneinandergehängt ergäbe denselben Ort zweimal, und die Zeile
                # würde so lang, dass sie den Knopf daneben aus dem Fenster
                # schiebt.
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
        # ⚠⚠ **Ein fehlender Preis ist kein fehlender Weg.** Stünde der Knopf
        # nur bei `BEKANNT`, käme wer einmal auf „Selbst herstellen"
        # gewechselt hat nicht mehr zurück, solange UEX keinen Preis führt.
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
        # ⚠⚠ **Der Knopf wird VOR dem Text gepackt — das ist kein Stil.** In
        # `tkinter` bekommt das zuerst gepackte Element seinen Platz; ein
        # langer Text mit `side='left'` schiebt einen später gepackten
        # `side='right'`-Knopf schlicht aus dem Fenster. Bei einem Laden mit
        # langem Namen wäre der Knopf „Kaufen" sonst unsichtbar.
        if not aktiv and angabe.get('zustand') != cart.NO_RECIPE:
            _button(fenster, zeile, t(schluessel),
                   waehlen(weg)).pack(side='right', padx=(8, 0))

        # ⚠ Der Text kommt zuletzt und darf schrumpfen: Was nicht passt, wird
        # abgeschnitten — der Knopf daneben bleibt.
        tk.Label(zeile, text=text, bg=theme.FIELD, fg=farbe,
                 font=fenster.f_small, anchor='w').pack(side='left',
                                                        fill='x', expand=True)

        # ⚠⚠ Ein Rohstoff ohne Kaufpreis ist nicht kostenlos, sondern nicht
        # kaufbar. Ohne diesen Satz sieht Selberbauen billiger aus, als es ist.
        if weg == cart.CRAFT and angabe.get('ohne_preis'):
            _body_text(karte,
                        t('s_wk_ohne_preis').format(
                            rohstoffe=', '.join(angabe['ohne_preis'])),
                        fenster.f_small, color=GOLD, bg=theme.FIELD,
                        fill='x', padx=12, pady=(0, 6), inset=90)

    tk.Frame(karte, bg=theme.FIELD, height=4).pack()


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
    # sind „2 Läden · 1 Stopp" — ein gemeinsamer Mehrzahl-Satz schriebe
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
    material = tk.StringVar(rahmen)
    menge = tk.StringVar(rahmen)
    guete = tk.StringVar(rahmen)
    # Der zuletzt benutzte Lagerort steht schon drin — siehe unten beim
    # Eintragen, warum.
    ort = tk.StringVar(rahmen, value=paths.setting('lager_ort') or '')
    # ⭐ In welcher Einheit das Mengenfeld rechnet. Das Raffinerie-Terminal im
    # Spiel zeigt **cSCU**, die Gegenstands-Anzeige im Lager **SCU** — und vom
    # Terminal abzutippen ist bequemer, weil man dort nicht jeden Stapel
    # einzeln mit der Maus anfahren muss. Das Kästchen
    # neben dem Feld schaltet um; die Beschriftung sagt immer, was gerade gilt.
    cscu = [paths.setting('lager_einheit') == 'cscu']

    def _einheit(name=None):
        """„Stück" oder „SCU" für die Meldungen — zum gewählten oder
        zum genannten Material."""
        try:
            from . import crafting as _h_einheit
            stueck = _h_einheit.is_piece(name if name is not None
                                         else material.get())
        except Exception:
            stueck = False
        return t('s_lg_stueck') if stueck else 'SCU'

    def _stueckware():
        """Zählt das gerade gewählte Material in Stück statt in SCU?

        ⚠⚠ Elf Materialien tun das (Hadanite, Dolivine, Sadaryx …). Für sie
        gibt es kein cSCU — siehe `s_lg_menge_stueck`.
        """
        try:
            from . import crafting as _h_einheit
            return _h_einheit.is_piece(material.get())
        except Exception:
            return False

    def _faktor():
        # ⚠ Stückware nie umrechnen: 75 Hadanite sind 75, nicht 0,75. Das
        # Kästchen ist dort ausgeblendet, sein gemerkter Wert bleibt aber
        # stehen — ohne diese Abfrage schlüge er trotzdem durch.
        if _stueckware():
            return 1.0
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
    # ⚠⚠ **Beschriftung ÜBER dem Feld** — dasselbe Bild wie im Handelslager.
    # Die Zeilenform (Bezeichnung links, Feld rechts) verträgt sich nicht
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
            f = round_entry(_mengenzeile, var, fenster.f_small, theme.FIELD,
                            LINE, ACCENT, FG)
            mengen_beschriftung = kopf_label

            def einheit_um(an):
                mengen_beschriftung.configure(
                    text=t('s_lg_menge_cscu') if an else t('s_lg_menge'))
                paths.set_setting('lager_einheit',
                                         'cscu' if an else 'scu')
                mengen_vorschau_zeigen()

            # ⚠⚠ **Erst das Kästchen packen, dann das Feld.** In `tkinter`
            # bekommt das zuletzt gepackte Element den übrigen Platz — und ein
            # Feld mit `expand=True` nimmt sich alles. Andersherum gepackt
            # schob es das Kästchen aus dem Fenster.
            _cscu_kasten = _checkbox(_mengenzeile, t('s_lg_cscu'), cscu,
                                     einheit_um, fenster.f_small)
            _cscu_kasten.pack(side='right', padx=(10, 0))
            f.holder.pack(side='left', fill='both', expand=True)
            if cscu[0]:
                kopf_label.configure(text=t('s_lg_menge_cscu'))

            def einheit_nachziehen(*_):
                """Beschriftung und cSCU-Kästchen zum gewählten Material.

                ⚠ Bei Stückware gibt es kein cSCU. Das Kästchen stehen zu
                lassen hiesse: Man hakt es an, tippt 75 — und hat 0,75 im
                Lager. Deshalb verschwindet es, statt wirkungslos dazustehen.

                ⚠ Beim Wiedereinblenden zählt `before=`: Ohne die Angabe
                landet es hinter dem Feld, das mit `expand=True` allen Platz
                nimmt — und wäre aus dem Fenster geschoben (siehe oben).
                """
                if _stueckware():
                    _cscu_kasten.pack_forget()
                    mengen_beschriftung.configure(text=t('s_lg_menge_stueck'))
                else:
                    if not _cscu_kasten.winfo_manager():
                        _cscu_kasten.pack(side='right', padx=(10, 0),
                                          before=f.holder)
                    mengen_beschriftung.configure(
                        text=t('s_lg_menge_cscu') if cscu[0]
                        else t('s_lg_menge'))
                mengen_vorschau_zeigen()

            material.trace_add('write', einheit_nachziehen)
            # ⭐⭐ **Die Vorschau ist die eigentliche Erklärung.** Wer beim
            # Tippen von „1.04+3" daneben „ergibt 4,04 SCU" liest, braucht
            # keinen Satz über Auf- und Abbuchen mehr.
            mengen_vorschau = tk.Label(block, text='', bg=BG, fg=ACCENT,
                                       font=fenster.f_small, anchor='w')
            mengen_vorschau.pack(fill='x')
        else:
            f = round_entry(block, var, fenster.f_small, theme.FIELD, LINE,
                            ACCENT, FG)
            f.holder.pack(fill='x', pady=(4, 0))

    # ℹ Keine „Meintest du:"-Zeilen für Rohstoff und Lagerort: Das
    # Auswahlfeld filtert beim Tippen selbst und zeigt auf Knopfdruck die
    # ganze Liste. Zwei Wege für dieselbe Hilfe nebeneinander wären eine
    # Bedienung zu viel.

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
            mengen_vorschau.configure(text=t('s_lg_ergibt_minus')
                                      % (vorher, _einheit()),
                                      fg=GOLD)
        elif wert == 0:
            mengen_vorschau.configure(text=t('s_lg_ergibt_null'), fg=GOLD)
        else:
            mengen_vorschau.configure(text=t('s_lg_ergibt')
                                      % (round(wert, 3), _einheit()),
                                      fg=ACCENT)
        mengen_vorschau.pack(fill='x', pady=(4, 0))

    menge.trace_add('write', mengen_vorschau_zeigen)

    liste_rahmen = tk.Frame(innen, bg=BG)
    meldung = tk.Label(innen, text='', bg=BG, fg=SUB, font=fenster.f_small,
                       anchor='w')

    # ⚠ Als **Tabelle mit Spalten**, nicht als Fließtext: Bei 26 Materialien
    # an mehreren Orten wird die Liste lang, und dann sucht man einen Posten,
    # statt ihn zu sehen. Spaltenköpfe sortieren auf Klick, das Feld darüber
    # filtert.
    sortier = {'nach': 'material', 'ab': False}
    filter_var = tk.StringVar(rahmen)

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
                           font=fenster.f_small,
                           width=0 if schluessel == SPALTEN[-1][0] else breite,
                           anchor=anker_, cursor='hand2')
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
            # Umständen nichts — es stünde „chen" statt „Löschen" da. Zuerst
            # gepackt, reserviert es
            # seinen Platz, und die Spalten teilen sich den Rest.
            weg = tk.Label(z, text=t('s_lg_weg'), bg=z_bg, fg=SUB,
                           font=fenster.f_small, cursor='hand2', anchor='e')
            weg.pack(side='right', padx=(8, 4))
            # ⚠ Rollstelle halten — sonst springt die Seite beim Löschen nach
            # ganz oben, und wer beim zwölften Posten war, sucht sich neu
            # zurecht.
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
                letzte = _k == SPALTEN[-1][0]
                lbl = tk.Label(z, text=wert, bg=z_bg, fg=farbe, font=schrift,
                               width=0 if letzte else breite, anchor=anker_,
                               justify='left', cursor='hand2')
                if letzte:
                    # ⚠ Die letzte Spalte (Ort) nimmt den Rest und bricht um,
                    # statt feste Zeichen zu fordern: Bei der größten Schrift
                    # bekäme „Levski" sonst nur rund die Hälfte seiner Breite
                    # (tools/randpruefung.py).
                    lbl.pack(side='left', padx=(0, 8), fill='x', expand=True)
                    _wrap_self(lbl)
                else:
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

    filter_var.trace_add('write', after_typing(rahmen, zeichnen))

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
                                                       float(p_.get('menge') or 0),
                                                       _einheit(p_.get('material') or ''))):
            return
        _keep_scroll(innen, lambda: (lager.remove(nummer),
                                           verwerfen(), zeichnen()))

    def verwerfen(*_):
        """Zurueck zum Eintragen — Felder leeren, nichts speichern."""
        if bearbeitung['nummer'] is None:
            return
        bearbeitung['nummer'] = None
        material.set(''); menge.set(''); guete.set('')
        ort.set(paths.setting('lager_ort') or '')
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
            # Ein Knopf „Trotzdem eintragen" machte das Feld faktisch frei —
            # und ein freies Textfeld heisst, dass jemand Schimpfwoerter,
            # Religioeses oder Politisches eintraegt, ein Bildschirmfoto macht
            # und es verbreitet. Es steht dann in diesem Werkzeug.
            #
            # Speicherbar ist NUR, was in der Rohstoff-Liste steht.
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
        # Nur ein FUEHRENDES Vorzeichen zu zaehlen, lehnte genau diese
        # natuerliche Eingabe ab. `lager.calculate()` kann beides und liefert
        # direkt die **neue Menge**.
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
                # läse sich dort wie ein Buchhaltungsfehler, dabei ist die
                # Eingabe („-2" in ein leeres Formular) schlicht sinnlos.
                meldung.configure(text=t('s_lg_nicht_negativ'), fg=GOLD)
                return
            if neu_wert < 0:
                # ⚠ Nicht stillschweigend auf 0 setzen. Wer sich um eine Ziffer
                # vertippt, soll den Bestand sehen, nicht ihn verlieren.
                meldung.configure(text=t('s_lg_zu_wenig')
                                  % (vorher, _einheit()), fg=GOLD)
                return
            if neu_wert == 0 and nr is not None:
                # Alles abgegeben — dann hat der Posten keinen Zweck mehr.
                lager.remove(nr)
                bearbeitung['nummer'] = None
                material.set(''); menge.set(''); guete.set('')
                ort.set(paths.setting('lager_ort') or '')
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
            hinweis = t('s_lg_eingetragen') % (name, wert, _einheit(name))
        else:
            lager.change(bearbeitung['nummer'], name, wert, q, ort.get())
            hinweis = t('s_lg_geaendert') % (name, wert, _einheit(name))
            bearbeitung['nummer'] = None
        # ⚠ **Der Lagerort bleibt stehen.** Wer eine Raffinerie-Ausbeute
        # einträgt, trägt sechs Posten am selben Ort ein — ihn jedes Mal neu
        # zu wählen ist reine Tipparbeit. Material, Menge und Qualität werden
        # geleert, der Ort nicht; er wird zusätzlich gemerkt, damit er auch
        # beim nächsten Programmstart noch dasteht.
        material.set(''); menge.set(''); guete.set('')
        paths.set_setting('lager_ort', ort.get().strip())
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
    # ⚠⚠ **Das Suchfeld wird EINMAL gebaut — nicht in `zeichnen()`.**
    # `zeichnen()` räumt bei jeder Änderung den ganzen Listenbereich leer:
    # Mit jedem getippten Buchstaben zerstörte sich das Feld selbst, und der
    # Tastaturfokus ginge verloren.
    #
    # Alles, woran ein Cursor stehen kann, gehört ausserhalb der Zeichenroutine.
    from .main_window import round_entry as _rf_suche
    _such_zeile = tk.Frame(innen, bg=BG)
    _such_zeile.pack(fill='x', pady=(6, 0))
    tk.Label(_such_zeile, text=t('s_lg_filter'), bg=BG, fg=SUB,
             font=fenster.f_small).pack(side='left', padx=(0, 10))
    _such_feld = _rf_suche(_such_zeile, filter_var, fenster.f_small,
                           theme.FIELD, LINE, ACCENT, FG)
    _such_feld.holder.pack(side='left', fill='x', expand=True)

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
            errors.record('pages.lager.ausgeben', ausnahme)

    def _einlesen():
        from . import file_picker
        quelle = file_picker.open_file(t('s_lg_einlesen'))
        if not quelle:
            return
        try:
            with open(quelle, encoding='utf-8') as f:
                posten = lager.from_json(f.read())
        except Exception as ausnahme:
            errors.record('pages.lager.einlesen', ausnahme)
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
    hin, und nicht jeder kennt alle Komponenten auswendig.

    ⚠ **Der Zusatz wird mitgesucht.** Wer `stealth` tippt, meint die
    Tarn-Komponenten und nicht ein Teil namens Stealth — eine Liste, die den
    Text zeigt, aber nicht darauf reagiert, wirkt kaputt.
    """
    from .main_window import round_entry

    zeile = tk.Frame(parent, bg=BG)
    liste = tk.Frame(parent, bg=BG)
    offen = {'ja': False}

    # ⭐ `clearable`: ein X im Feld leert die Suche — in jedem Auswahlfeld
    # gleich.
    # ⚠⚠ **Feste Regel: Der Pfeil sitzt IM Feld und sieht aus wie bei
    # `round_select`** (dasselbe ▾), kein eigenes Bauteil daneben — sonst
    # stünden zwei Sorten Auswahlfeld untereinander. Siehe
    # `round_entry(dropdown=True)`.
    feld = round_entry(zeile, var, window.f_small, theme.FIELD, LINE, ACCENT,
                       FG, clearable=True, dropdown=True)
    pfeil = feld.trailing

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
            return
        if not text and not offen['ja']:
            liste.pack_forget()
            return
        # ⚠⚠ **Punkte und Bindestriche zaehlen beim Suchen nicht.** Wer „ATLS"
        # tippt, meint „A.T.L.S." — und umgekehrt. Ohne diese Zeile findet das
        # Feld sein eigenes Schiff nicht: Der Pledge-Store schreibt „A.T.L.S.",
        # UEX schreibt „Argo ATLS IKTI", und beide sind dasselbe Ding.
        schlank = _squashed(text)

        def _passt(name):
            if schlank in _squashed(name):
                return True
            # Der Zusatz zaehlt mit: „stealth" findet die Tarn-Teile, „a"
            # allein nicht (ein einzelner Buchstabe traefe jede Guete).
            bei = _zusatz_text(name)
            return len(schlank) > 1 and bei and schlank in _squashed(bei)

        treffer = [e for e in alle if _passt(e)] if text else list(alle)
        if not treffer:
            liste.pack(fill='x', pady=(4, 0))
            tk.Label(liste, text=empty_text or t('s_vk_nichts_gefunden'),
                     bg=BG, fg=SUB,
                     font=window.f_small, anchor='w').pack(fill='x', pady=3)
            return
        liste.pack(fill='x', pady=(4, 0))
        # ⭐ **`rollbar`: alle Treffer, in einer Flaeche fester Hoehe.**
        # Ohne das zeigt die Liste zehn Namen und sagt „und 124 weitere" — wer
        # nicht weiterliest, haelt die zehn fuer das ganze Angebot.
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
        # gibt: `TclError: bad window path name`.
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

    feld.holder.pack(side='left', fill='both', expand=True)

    # ⭐⭐ **Ein Klick ins Feld klappt die Liste auf.** Der Pfeil ist ein
    # 16-Pixel-Symbol am rechten Rand, und wer ein Auswahlfeld anklickt,
    # erwartet eine Auswahl. Der Pfeil bleibt trotzdem — er zeigt an, dass da
    # etwas zum Aufklappen ist, und schliesst die Liste wieder.
    def beim_hineinklicken(_=None):
        if not offen['ja']:
            offen['ja'] = True
            zeichnen()

    feld.bind('<FocusIn>', beim_hineinklicken, add='+')
    feld.bind('<Button-1>', beim_hineinklicken, add='+')

    # ⚠⚠ **Und sie geht wieder zu, wenn man woanders hinklickt.** Eine
    # Liste, die nur aufgeht, ist eine halbe Bedienung.
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
    # Die Fensterregel `_bind_click_on_empty` greift dort: Die Leiste ist
    # eine Leinwand, kein Eingabefeld, also bekommt das Fenster den Fokus,
    # das Feld meldet `<FocusOut>`, und 200 ms später klappte `_zumachen`
    # die Liste zu — obwohl `_klick_im_fenster` den Klick richtig als
    # „drinnen" erkennt.
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
    # und die Liste bliebe offen.
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

    # ⭐⭐ **Enter übernimmt, was dasteht.** Wer den Namen kennt und ihn zu Ende tippt, hat die Liste nicht
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
    zeichnen_bald = after_typing(zeile, zeichnen)

    def beim_tippen(*_):
        offen['ja'] = not (var.get() or '').strip()
        zeichnen_bald()

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
    suche = tk.StringVar(rahmen)
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
        Rückkehr über `_TK_CALLS`.
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
                errors.record('pages.verkauf_abruf', ausnahme)
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

            _TK_CALLS.put(melden)

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
        # ⭐⭐ **Menge gleich mitnehmen, ohne Umweg übers Lager.** Wer gerade 120 SCU Gold im Laderaum hat und
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

    # ⚠ Kein einzelnes Mengenfeld für „die zuletzt gewählte Ware": Die Menge
    # steht an der Marke der Ware selbst (siehe `_chips`) — zwei Wege für
    # dieselbe Sache überschrieben sich gegenseitig.
    _menge_lbl = tk.Label(suchzeile, text=t('s_vk_menge_hinweis'), bg=BG,
                          fg=SUB, font=fenster.f_small, anchor='w',
                          justify='left')
    _menge_lbl.pack(fill='x', pady=(6, 0))
    _wrap(_menge_lbl)

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

        ⚠⚠ **Die Menge gehört an die Ware, nicht an ein Feld daneben.** Ein
        einziges Mengenfeld für „die zuletzt gewählte Ware" muss raten — und
        rät falsch, sobald jemand die Zahl eintippt, **bevor** er die Ware
        wählt: Die Zahl landet bei der vorigen Ware.

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

            var = tk.StringVar(marke, value=str(eigene_mengen.get(name) or ''))
            # ⚠ Nur ein Wort: Das Feld ist fünf Zeichen breit. Ein
            # abgeschnittener Hinweis wäre schlimmer als keiner — und die
            # Einheit steht ohnehin als Etikett daneben.
            from .main_window import round_entry as _feld_rund
            feld = _feld_rund(marke, var, fenster.f_small, theme.FIELD, LINE,
                              ACCENT, FG, width=5, justify='right',
                              placeholder=t('s_pl_menge'))
            feld.holder.pack(side='left', pady=3)
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
            #
            # Die Ablage kennt 114 Waren mit
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
                # ⚠ `s_vk_je_scu` nutzt `{preis}`, nicht `%s`. Ein zweiter
                # Eintrag desselben Namens verdrängt den ersten still, und
                # `% _geld(...)` flöge auf die Nase. Ein doppelter Schlüssel
                # fällt in einem Wörterbuch nicht auf — deshalb prüft der
                # Selbsttest das.
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
            # In jedem wiederholt, stünde sie bei 40 Orten vierzigmal da und
            # machte die Liste unruhiger, statt sie zu erklären.
            _selling_row(fenster, ergebnis_rahmen, ort, len(auswahl),
                           lagermengen, mit_kopf=(nummer == 0))

    def neu_zeichnen():
        _chips()
        such_zeichnen()
        _ergebnis()

    # ⚠⚠ **Ein Klick ins Leere nimmt auch den Schreibcursor mit.** Ein
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

    ware = tk.StringVar(rahmen)
    menge = tk.StringVar(rahmen)
    ort = tk.StringVar(rahmen, value=paths.setting('handel_ort') or '')
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
    beschriftungen = {}
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
        beschriftungen[str(var)] = tk.Label(block, text=beschriftung, bg=BG,
                                            fg=FG, font=fenster.f_bold,
                                            anchor='w')
        beschriftungen[str(var)].pack(fill='x')
        if var is menge:
            feld = round_entry(block, var, fenster.f_small, theme.FIELD, LINE,
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
        # foto davon macht die Runde. Beim Ändern darf der Name stehen bleiben,
        # den der Posten schon trägt — ein Item aus dem Basetool steht nicht
        # in der UEX-Liste.
        if not preisdaten.known(name) and name != _name_vorher():
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
            paths.set_setting('handel_ort', ort.get() or '')
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

    def _name_vorher():
        nr = bearbeitung['nummer']
        posten = lager.load() if nr is not None else []
        return ((posten[nr].get('ware') or '').strip()
                if nr is not None and 0 <= nr < len(posten) else None)

    def _mengen_beschriftung(stueck):
        beschriftungen[str(menge)].configure(
            text=t('s_hl_menge_stueck') if stueck else t('s_hl_menge'))

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
        _mengen_beschriftung(False)
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
        _mengen_beschriftung(lager.is_pieces(p))
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
            errors.record('pages.handelslager.ausgeben', ausnahme)
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
            errors.record('pages.handelslager.einlesen', ausnahme)
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

        ⚠⚠ **Der Name ist mit Absicht lang.** `_leeren` heisst schon der
        Helfer weiter oben, der die Kinder eines Rahmens wegraeumt und **mit**
        Argument gerufen wird. Die spaetere Definition gewinnt in Python: Jeder
        Aufbau der Liste scheiterte dann mit „_leeren() takes 0 positional
        arguments but 1 was given".

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

    ⚠⚠ **Ein gemeinsames Raster, nicht ein Rahmen je Zeile.** Mit einem
    eigenen Kasten je Zeile (`pack`) richtet sich nichts aneinander aus, und
    man muss raten, welche Zahl wozu gehört. Mit `grid` in **einem** Rahmen
    legt Tk die Spalten über alle Zeilen gleich breit an, und die Zuordnung
    ist zu sehen statt zu erraten.

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
            # Ein Item zählt in Stück, nicht in der SCU der Überschrift.
            ((_amount_text(p.get('menge') or 0) + ' ' + t('s_lg_stueck'))
             if p.get('stueck') else _amount_text(p.get('menge') or 0),
             vorne, 'e'),
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

    An einem Aufbau mit drei Bildschirmen gemessen: Tk meldet
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

    zustand = {'abstand': tk.StringVar(rahmen, value='')}

    def _gespeichertes():
        daten = fov_modul.stored()
        return (daten.get('fov_mm_je_pixel'), daten.get('fov_pixelbreite'),
                daten.get('fov_abstand_mm'))

    def _fertig_gemessen(karte_px, bildschirm_px, vollbild=True):
        mm_je_pixel = fov_modul.mm_per_pixel(karte_px)
        if not mm_je_pixel:
            return
        # ⚠ Ohne echtes Vollbild ist `bildschirm_px` die FENSTERbreite, nicht
        # die des Bildschirms — die ganze Rechnung wäre falsch. Dann lieber
        # nichts speichern und es sagen.
        if not vollbild:
            _notice(fenster, t('hf_blickwinkel'),
                                   t('s_fv_kein_vollbild'))
            return
        fov_modul.remember(mm_per_px=mm_je_pixel, width_px=bildschirm_px)
        breite = fov_modul.screen_width_mm(bildschirm_px, mm_je_pixel)
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
            start = fov_modul.CARD_WIDTH_MM / mm_je_pixel
        # ⛔⛔ **Die Schlüsselwörter heißen `font`, `small`, `start_width`** —
        # so benennt `calibrate()` seine Parameter. Ein falsches wirft erst
        # beim Klick auf „Neu ausmessen" einen `TypeError`, und es passiert
        # schlicht nichts. Ein Schlüsselwort gehört der **empfangenden**
        # Funktion.
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
            fov_modul.remember(distance_mm=zentimeter * 10.0)
        _auffrischen()

    def _auffrischen():
        # Dieselbe Begründung wie auf der Achsen-Seite: Ein Neuaufbau darf
        # die Rollstelle nicht verlieren.
        _keep_scroll(inhalt, _neu_bauen)

    def _neu_bauen():
        for kind in list(inhalt.winfo_children()):
            kind.destroy()

        mm_je_pixel, pixelbreite, abstand_mm = _gespeichertes()
        breite_mm = (fov_modul.screen_width_mm(pixelbreite, mm_je_pixel)
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
        # ⚠ An diesem Feld hängt schon ein `<FocusOut>`, das den Wert
        # speichert. Das verträgt sich: `_abstand_merken` steigt bei leerem
        # Text aus (`float('')` wirft), und genau leer ist die Variable,
        # solange der Hinweis steht.
        from .main_window import round_entry as _feld_rund
        feld = _feld_rund(zeile, zustand['abstand'], fenster.f_base, SURFACE,
                          LINE, ACCENT, FG, width=6, justify='right',
                          placeholder=t('s_pl_abstand'))
        feld.holder.pack(side='right', padx=(16, 0))
        feld.bind('<Return>', _abstand_merken)
        # ⚠⚠ `add='+'` ist hier PFLICHT. Ohne das ersetzt diese Bindung die,
        # die `fields.hint()` gerade gesetzt hat — und der Hinweis kommt
        # nach dem ersten Verlassen des Feldes nie wieder (leeres Feld →
        # Return → Fokus weg, Hinweis bleibt verschwunden).
        #
        # Die Falle gilt für JEDES Feld mit eigener Bindung: `bind()` ohne
        # `add='+'` wirft die vorhandene weg, ohne sich zu beschweren.
        feld.bind('<FocusOut>', _abstand_merken, add='+')
        tk.Frame(inhalt, bg=LINE, height=1).pack(fill='x', pady=(12, 0))

        if not abstand_mm:
            return

        # --- 3. Das Ergebnis -------------------------------------------
        neutral = fov_modul.field_of_view(breite_mm, abstand_mm)
        if neutral is None:
            return
        _wert_zeile(inhalt, t('s_fv_neutral'), '%.1f°' % neutral,
                    hilfe=t('s_fv_neutral_hilfe'), farbe=ACCENT)

        spiel = fov_modul.game_setting()
        if spiel.get('fov') is None:
            _body_text(inhalt, t('s_fv_kein_spielwert'), fenster.f_small,
                        fill='x')
            return

        _wert_zeile(inhalt, t('s_fv_im_spiel'), '%.1f°' % spiel['fov'])

        # ⭐ Der Optimalpunkt: nicht „stell dein Spiel um", sondern „so weit
        # müsstest du sitzen". Wer sein FOV kennt und mag, will seinen Stuhl
        # rücken — nicht seine Einstellung.
        optimal_mm = fov_modul.distance_for(breite_mm, spiel['fov'])
        if optimal_mm is None:
            return
        _wert_zeile(inhalt, t('s_fv_optimalpunkt'),
                    '%.0f cm' % (optimal_mm / 10.0),
                    hilfe=t('s_fv_optimalpunkt_hilfe'))

        # --- 4. Rot / Gelb / Grün --------------------------------------
        note, abweichung = fov_modul.rating(abstand_mm, optimal_mm)
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
        # ⚠ Der Wert zuerst: `pack` quetscht, was zuletzt kommt. Stünde die
        # Erklärung vorn, bekäme der Wert bei der größten Schrift kaum Platz
        # (tools/randpruefung.py). So bricht die Erklärung um.
        tk.Label(zeile, text=wert, bg=BG, fg=farbe, font=fenster.f_bold,
                 anchor='e').pack(side='right', padx=(16, 0))
        links = tk.Frame(zeile, bg=BG)
        links.pack(side='left', fill='x', expand=True)
        tk.Label(links, text=bezeichnung, bg=BG, fg=FG, font=fenster.f_bold,
                 anchor='w').pack(fill='x')
        if hilfe:
            _hilfe_lbl = tk.Label(links, text=hilfe, bg=BG, fg=SUB,
                                  font=fenster.f_small, anchor='w',
                                  justify='left')
            _hilfe_lbl.pack(fill='x')
            _wrap_self(_hilfe_lbl)
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

    An einem echten Aufbau gemessen: drei solcher Fälle, darunter
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

    # ⚠⚠⚠ **Liegen mehrere Belegungsdateien da, muss man das sehen.** Liest
    # der Watcher die falsche, zeigt er andere Werte als das Spiel — und weil
    # in einer alten Datei die Geräte anders durchnummeriert sein können, die
    # Werte des falschen Sticks dazu.
    #
    # Gelesen wird die zuletzt geänderte (`joysticks.all_actionmaps`).
    # Das allein reicht aber nicht: Solange die Karteileiche danebenliegt,
    # kann sie beim nächsten Kopiervorgang wieder die jüngere sein. Deshalb
    # steht hier, was gefunden wurde — und welche davon zählt.
    from . import joysticks as _js_dateien
    _dateien = _js_dateien.all_actionmaps()
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
        Ergebnis: gemessen **313 ms bei jedem Klick**.

        ⭐ Der Fingerabdruck ist die **Datenlage selbst**, nicht ein
        Zeitstempel: Sind die Zahlen gleich, wäre auch das Bild gleich. Und
        wenn eine Quelle einmal etwas Unsortiertes liefert, ist der schlimmste
        Fall ein **überflüssiger** Neuaufbau — nie ein veraltetes Bild. Das ist
        die richtige Richtung für einen Irrtum.

        ⚠⚠ **Er braucht ALLE Quellen, aus denen gezeichnet wird — nicht nur
        die naheliegendste.** In `curves.summary()` stehen die
        Achseneinstellungen, aber **nicht**, welche Funktion auf welcher Achse
        liegt: `v_pitch` von `js1_x` auf `js1_y` schieben lässt die
        Zusammenfassung gleich, und Beschriftung, CurvePlot und Regler blieben
        auf dem alten Stand.

        ⭐ **Statt die einzelnen Abfragen nachzubauen, steht hier die QUELLE.**
        `zusammenfassung()`, `funktionen_je_achse()` und `spielachsen_auf()`
        lesen alle dieselbe Datei — die `actionmaps.xml` des Spielers. Ein
        Abdruck über ihren Inhalt deckt damit **alles** ab, was diese Seite von
        dort zeigt, auch das, woran hier gerade niemand denkt.

        Einzelne Abfragen aufzuzählen wäre die zweite Wahrheit, die irgendwann
        unvollständig wird.

        ⚠ Dazu `device_set.sets()`: eine **eigene** Quelle (die Gerätesätze
        stehen woanders), und sie wird auf derselben Seite angezeigt.
        """
        stand = _achsen_stand()
        if stand is not None and stand == zuletzt_achsen['stand']:
            return
        _auffrischen()

    def _achsen_stand():
        """Der Fingerabdruck der Lage — siehe `_beim_zeigen`."""
        import hashlib
        from . import device_set as _gs
        from . import joysticks as _js
        try:
            weg = _js._actionmaps_path()
            roh = b''
            if weg and os.path.exists(weg):
                with open(weg, 'rb') as f:
                    roh = f.read()
            # ⚠⚠ Der XML-Hash allein reicht NICHT. `zusammenfassung()` hängt
            # zusätzlich an `gueltige_kennungen()` → `joysticks.devices()`,
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
            errors.record('pages.achsen_stand', ausnahme)
            stand = None
        return stand

    def _auffrischen():
        """Neu zeichnen, ohne dass die Seite nach oben springt.

        ⚠ Der Inhalt wird bei jedem Geräte- und Achsenwechsel komplett neu
        gebaut — danach steht die Rollfläche wieder bei null, und wer unten
        bei den Reglern war, landet oben. `_rollstelle_halten` gibt es im
        Projekt genau dafür.
        """
        _keep_scroll(inhalt, _neu_bauen)
        # ⚠ Den Abdruck NACH dem Aufbau nehmen: `_neu_bauen` setzt beim
        # ersten Mal die Vorauswahl in `wahl`, und Klicks auf der Seite ändern
        # sie. Genommen vorher, passte der Abdruck beim nächsten Besuch nicht,
        # und die Seite baute sich umsonst neu (0,47 s, gemessen mit
        # `tools/tempo_messen.py`).
        zuletzt_achsen['stand'] = _achsen_stand()

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
            var = tk.DoubleVar(zeile, value=(ruhe if ist is None else ist))
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
            # 0.098999992 wird beim Aufbau 0.099 — und die Seite hielte das
            # für eine Änderung, die niemand gemacht hat: „Ungespeicherte
            # Änderung" stünde sofort beim Öffnen da, und beim Speichern
            # landeten Werte in der Datei, die der Spieler nie angefasst hat.
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

        # ⚠ Kein lokaler Import des Tk-Dialogs hier: Er verdeckte den Dialog
        # im Programmstil, der weiter oben in dieser Funktion definiert ist,
        # und holte den weißen System-Kasten zurück.
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
                    erfolg, meldung, _ = joysticks.swap_bindings(
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

        ⚠⚠ **Das ist eine Auskunft, kein Auftrag.** In Gold ganz oben mit
        Knöpfen darunter wird sie als Fehlermeldung gelesen, die man
        wegklicken muss. Wer alle Knöpfe reihum drückt, überschreibt seine
        funktionierenden Werte mit alten, die sich untereinander
        widersprechen.

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
        name = tk.StringVar(eltern)
        from .main_window import round_entry as _feld_rund
        feld = _feld_rund(neu, name, fenster.f_small, SURFACE, LINE, ACCENT,
                          # 18 statt 22 Zeichen: Mit 22 ragte „Speichern"
                          # bei der größten Schrift 15 px aus der Spalte.
                          FG, width=18, placeholder=t('s_pl_satzname'))
        feld.holder.pack(side='left', padx=(0, 8))

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

        # ⚠ Die Knöpfe zuerst, der Text danach: `pack` verteilt knappen Platz
        # in Packreihenfolge, und was zuletzt kommt, wird gequetscht. Stünden
        # Name und Stand vorn, würde „Löschen" abgeschnitten
        # (tools/randpruefung.py). So bricht der Text um, und die Knöpfe
        # bleiben ganz.
        links_teil = tk.Frame(zeile, bg=SURFACE)
        links_teil.pack(side='left', fill='x', expand=True, padx=10, pady=8)
        for text, schrift, farbe in (
                (satz['name'], fenster.f_bold, FG),
                ('%s  ·  %s' % (
                    t('s_gs_geraete').format(len(satz.get('geraete') or {})),
                    t('s_gs_stand').format(satz.get('stand', '—'))),
                 fenster.f_small, SUB)):
            _zeilen_lbl = tk.Label(links_teil, text=text, bg=SURFACE, fg=farbe,
                                   font=schrift, anchor='w', justify='left')
            _zeilen_lbl.pack(fill='x')
            _wrap_self(_zeilen_lbl)

    def _exponent_fuer(ueberblick, block, achse):
        """Der Exponent, der auf **dieser** physischen Achse gilt.

        ⚠⚠⚠ **Nicht den Exponenten des ganzen GERÄTS nehmen.** Sonst stünde
        auf einer Achse ohne Flugfunktion eine gebogene Kurve — geerbt von
        anderen Achsen desselben Geräts —, während direkt darunter „Auf dieser
        Achse liegt keine Flugfunktion" steht.

        **Gefragt wird die Belegung, nicht das Gerät.** Liegt auf der
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

        ⚠⚠ **Kein eigener Speichern-Knopf je Zeile.** Totzone und Sättigung
        sammelt man und schreibt sie mit **einem** Knopf — je Zeile einzeln
        zu speichern wäre ein Bruch. Und ein Knopf je Zeile fräße so viel
        Platz, dass vom Regler zwei kurze Balken übrig blieben, die niemand
        als Regler erkennt.
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
        var = tk.DoubleVar(eltern, value=(ruhe if ist is None else ist))
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
        # unerklärt.** Totzone und Sättigung hängen an der **physischen**
        # Achse, die Empfindlichkeit an der **Flugfunktion**, die darauf
        # liegt. Wo keine Funktion belegt ist, gibt es auch nichts
        # einzustellen — das steht in der Zeile.
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

        ⚠ **Nicht jede Achse einzeln.** An einem echten Aufbau wären das elf
        Zeilen für einen einzigen Fall und über zwanzig insgesamt — die
        eigentliche Bedienung stünde damit unter der Falzkante.
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


# ------------------------------------------------- Was der Patch geändert hat
# ⚠ **Höchstens so viele Posten auf einmal.** Ein Patch ändert allein bei den
# Schiffen schon mal 184 Einträge, viele davon mit mehreren Feldern —
# gezeichnet wären das über tausend Etiketten in einem Rutsch. Tk zeichnet
# einsträngig, das Fenster stünde sekundenlang. Der Rest steht als „… und N
# weitere" darunter; wer mehr sehen will, schränkt den Bereich ein.
_PATCH_MAX = 60

# ⚠ **Wie hoch die Patch-Liste höchstens wird.** Sie steht fest über dem
# Inhalt, damit man beim Durchsehen der Werte nicht nach oben zurück muss — und
# genau deshalb darf sie nicht mitwachsen. Gemessen: 30 px je Zeile. 150 px
# sind genau fünf Zeilen; alles darüber rollt in der Liste selbst.
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
        # ⚠⚠ **Keine Exponentialschreibweise.** `%g` kippt unterhalb von
        # 0,0001 auf `1.25e-06` um, und das liest niemand, der nicht täglich
        # mit Zehnerpotenzen umgeht. Richtig, aber unverständlich ist fast
        # dasselbe wie unsichtbar.
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

    ⚠⚠ **Das ist keine Kosmetik — ohne das lügt die Seite.** Fest auf vier
    Nachkommastellen gerundet, werden Treibstoff-Brennraten um `1.25e-06` zu
    `0`, alt und neu sehen gleich aus, und die Zeile meldet „unverändert" —
    bei echten Änderungen, oft um den Faktor drei bis fünf:

        precomputed.fuel.burnRate.main       1.25e-06 → 2.67e-07
        precomputed.fuel.burnRate.maneuver   1e-05    → 2.136e-06

    Deshalb wird die Genauigkeit so weit erhöht, bis der Unterschied
    dasteht. Normale Werte bleiben kurz (`110 → 120`), nur die winzigen werden
    länger — und das ist der Preis dafür, dass sie überhaupt zu sehen sind.
    """
    def _verschluckt(roh, gezeigt):
        """Wird hier eine Zahl zu „0", die in Wirklichkeit keine ist?"""
        return gezeigt == '0' and isinstance(roh, (int, float)) \
            and not isinstance(roh, bool) and roh != 0

    for stellen in (4, 6, 8, 10, 12, 14):
        vorher, nachher = _patch_number(alt, stellen), _patch_number(neu, stellen)
        # ⚠ **Unterschiedlich zu sein reicht nicht.** Bei sechs Stellen wird
        # aus `1.25e-06 → 2.67e-07` die Zeile „1e-06 → 0" — verschieden, ja,
        # aber die zweite Zahl ist gelogen: Der Wert ist nicht null, er ist
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

    ⚠⚠ **Das ist bei kleinen Zahlen die einzige lesbare Aussage.**
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
    lesbar = pa_field(pfad)
    if lesbar != pfad:
        return lesbar
    letzte = pfad.split('.')[-1] if pfad else pfad
    return letzte or pfad


def _pc_struct(alt, neu, hoechstens=3):
    """Was sich in zwei verschachtelten Werten wirklich unterscheidet.

    ⚠⚠ **Warum es diese Funktion gibt:** `_patch_number` gibt für alles
    Verschachtelte ein `…` zurück. In der Kategorie `blades` ist **jeder**
    geänderte Wert eine verschachtelte Struktur (`resource.states[0].flows`)
    — ohne diese Funktion stünden dort nur Zeilen `… → …` untereinander.

    Statt den ganzen Wert zu zeigen (der wäre unlesbar lang), wird der
    **Unterschied** gebildet: beide Seiten in Blattwerte zerlegt, verglichen,
    und nur die abweichenden benannt.

    Und der ehrlichste Fall zuerst: Unterscheiden sich die Blätter gar nicht,
    hat der Patch an dieser Stelle nichts geändert — dann sagt das Ergebnis
    genau das, statt einen Unterschied vorzutäuschen. Gemessen trifft das auf
    rund ein Fünftel der Posten eines Patches zu.
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
    tk.Label(zeile, text=pa_field(feld['pfad']), bg=BG, fg=SUB,
             font=fenster.f_small, anchor='w').pack(side='left')
    # ⚠ Drei Fälle, nicht einer. Fehlt `newValue`, hat der Patch das Feld
    # **weggenommen**; fehlt `oldValue`, ist es **dazugekommen**. Wer stumpf
    # „alt → neu" schreibt, macht daraus „1090 → None" und behauptet einen
    # Wert, den es nicht gibt (etwa bei der C-788 Cannon).
    if feld['hat_alt'] and feld['hat_neu']:
        # ⚠ Verschachtelte Werte NICHT als „… → …" abtun — dann steht dort
        # nichts. Bei ihnen zeigt `_pc_struct` den echten Unterschied.
        if isinstance(feld['alt'], (list, dict)) \
                or isinstance(feld['neu'], (list, dict)):
            text = _pc_struct(feld['alt'], feld['neu'])
            farbe = FG
        else:
            # ⚠⚠ **Maßgeblich ist der ROHWERT, nicht eine grobe Anzeige.**
            # Lässt die Anzeige zwei verschiedene Werte gleich aussehen, ist
            # die ANZEIGE das Problem — dann muss sie genauer werden, nicht
            # die Änderung verschwinden. `_pc_pair` erhöht die Genauigkeit so
            # weit, bis der Unterschied dasteht.
            vorher, nachher = _pc_pair(feld['alt'], feld['neu'])
            richtung = _pc_direction(feld['alt'], feld['neu'])
            # ⚠ `0 → 0` ist keine Auskunft, sondern Lärm — und davon steht
            # reichlich in den Daten: Erkul führt ein Feld auch dann im Diff,
            # wenn der Wert derselbe geblieben ist. Wer zwischen echten
            # Änderungen zehnmal „0 → 0" liest, hält den ganzen Reiter für
            # kaputt.
            #
            # Verglichen werden die Ausgaben von `_pc_pair` — die sind genau
            # genug, dass ein echter Unterschied darin sichtbar bleibt.
            if vorher == nachher:
                # ⚠⚠ **Der WERT selbst, grau — nicht „0" und nicht ein Satz.**
                #
                # Ein Satz wäre dreimal so lang wie die Zeile daneben.
                #
                # Eine `0` LÜGT: Bei „Verbrauch Haupttriebwerk 0" liest man,
                # der Verbrauch sei null — nicht, dass er gleich geblieben ist.
                # Ein Feld, dessen echter Wert 0 ist, wäre davon nicht mehr zu
                # unterscheiden.
                #
                # Ein `→ X` liest sich wie „ist weggefallen" — und dafür gibt
                # es bereits
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
                # ⚠ Mit der genauen Anzeige aus `_pc_pair` ist der Fall
                # selten. Der Zweig bleibt trotzdem — ein Patch kann ein Feld
                # anfassen, ohne den Wert zu ändern.
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
    from . import patch_changes as pa

    _heading(fenster, rahmen, t('hf_patchaenderungen'), t('s_pa_lead'))

    # ⚠⚠ **Patch-Liste und Bereichsknöpfe bleiben STEHEN, nur die Werte
    # rollen.** Läge alles in EINER Rollfläche, müsste wer bei „ships" durch
    # 60 Posten gescrollt ist und dann „weapons" ansehen will, den ganzen Weg
    # zurück nach oben — bei jedem Wechsel.
    #
    # ⚠ Die Reihenfolge ist dabei die halbe Miete: Erst alles Feste packen,
    # **danach** die Rollfläche mit `expand=True`. Wird der feste Teil nach dem
    # wachsenden gepackt, schiebt der Inhalt ihn aus dem Fenster.
    _body_text(rahmen, t('s_pa_sammlung'), fenster.f_small, fill='x',
                padx=24, inset=48)

    kopf = tk.Frame(rahmen, bg=BG)
    kopf.pack(fill='x', padx=24, pady=(12, 0))
    stand = tk.Label(kopf, text='', bg=BG, fg=SUB, font=fenster.f_small,
                     anchor='w')

    # ⚠⚠ **Die Patch-Liste rollt in sich selbst, mit fester Höhe.**
    #
    # Die Liste steht fest über dem Inhalt, und sie wächst — die eigene Sammlung geht absichtlich über die zehn Patches
    # hinaus, die die Quelle vorhält. Gemessen: 30 px je Zeile, also blieben
    # bei 30 Patches noch 41 px für die Werte und bei 50 gar nichts mehr.
    #
    # Fünf Zeilen sind sichtbar, der Rest wird gerollt: Ein fester Bereich mit veränderlichem Inhalt
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
            if pa.stored():
                _body_text(ergebnis, t('s_pa_waehlen'), fenster.f_small,
                            fill='x')
            else:
                _body_text(ergebnis,
                            t('s_pa_erst_holen').format(knopf=t('s_pa_suchen')),
                            fenster.f_small, fill='x')
            return
        posten = pa.changes(version, zustand['art'])
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
        if not version or not pa.load(version):
            return
        # ⚠⚠ **`_knopfgitter` will fertige KNÖPFE, keine Beschriftungspaare.**
        # `_reflow_grid` ruft `winfo_exists()` auf jedem Eintrag, ein
        # `(Text, Rückruf)`-Tupel hat das nicht — es flöge `AttributeError:
        # 'tuple' object has no attribute 'winfo_exists'`, und zwar aus einem
        # `after`-Rückruf heraus. Der Reiter bliebe leer, ohne sichtbare
        # Fehlermeldung, und der Selbsttest klickt ihn nicht an.
        knoepfe = [_button(fenster, bereiche, t('s_pa_alle'),
                          lambda: _art_waehlen(None),
                          strong=(zustand['art'] is None))]
        for art, anzahl in pa.categories(version):
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
        for eintrag in pa.overview():
            if eintrag['version'] == version:
                return eintrag
        return None

    def _patch_waehlen(version):
        zustand['patch'] = version
        # ⚠⚠ **Nicht „Alle" vorwählen.** Die Anzeige bricht nach 60 Posten ab, und über alle
        # Bereiche hinweg stehen die alphabetisch vorn — bei 4.10.0 sind das
        # 60 Zeilen `blades`, während die 184 Schiffe dahinter nie zu sehen
        # sind. Man sucht sich also erst durch das Uninteressanteste.
        #
        # Vorgewählt wird der GRÖSSTE Bereich, nicht fest `ships`. Bei 4.10.0
        # ist das dasselbe (ships 184), aber nicht immer: In 4.9.0 sind es
        # `weapons` mit 135, ships kommt dort nur auf 70. Fest `ships` würde
        # bei einem Waffen-Patch also wieder am Wesentlichen vorbeizeigen —
        # und `categories()` liefert ohnehin schon nach Größe sortiert.
        zustand['art'] = None
        if not pa.load(version):
            # ⚠⚠ **Zwei völlig verschiedene Gründe, warum hier nichts liegt** —
            # und sie brauchen zwei verschiedene Sätze:
            #
            #   1. Der Patch hat wirklich nichts geändert (die meisten Patches)
            #   2. Der Patch hat Änderungen — sie sind nur noch nicht geholt
            #
            # Derselbe Satz für beide behauptete im zweiten Fall, es gebe
            # nichts, während der Knopf daneben genau das geholt hätte.
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
        bereiche_da = pa.categories(version)
        zustand['art'] = bereiche_da[0][0] if bereiche_da else None
        _bereiche_zeigen()
        _posten_zeigen()

    def _liste_zeigen():
        _leeren(liste)

        # ⚠⚠ **Patches ohne Werteänderung kommen NICHT in die Liste.** Meist
        # sind es Hotfixes — von zehn Patches sind typischerweise **acht**
        # leer. Als volle Zeilen drängten sie die interessanten nach unten,
        # und wer der Reihe nach von oben klickt, landete zuerst auf ihnen.
        #
        # ⚠ Aber nicht spurlos: Darunter steht, **wie viele** weggelassen
        # wurden. Ohne diese Zeile sähe es aus, als fehlten Patches oder als
        # sei der Abruf unvollständig. Weglassen ja, verschweigen nein.
        alle = pa.overview()
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
                neu = pa.sync()
            except Exception as ausnahme:
                errors.record('pages.patch_changes.sync', ausnahme)
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
            _TK_CALLS.put(nachtragen)

        threading.Thread(target=arbeit, daemon=True).start()

    _button(fenster, kopf, t('s_pa_suchen'), _suchen).pack(side='left')

    _liste_zeigen()
    # ⚠ Über `_posten_zeigen()`, nicht mit einem fest hingeschriebenen Text:
    # `s_pa_waehlen` direkt („Wähl links einen Patch aus") stünde auch dann
    # da, wenn links noch gar nichts Abgelegtes liegt. Die Fallunterscheidung
    # steckt in
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


