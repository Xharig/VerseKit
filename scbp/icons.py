# -*- coding: utf-8 -*-
"""Die Symbole der Oberfläche — fertige Bilder statt Schriftzeichen.

**Warum überhaupt Bilder?** Bis v3.0.0-rc55 waren die Symbole Schriftzeichen
(`✕ 🗑 ⚙ ⟳ ▶ …`), zwei davon von Hand auf eine Leinwand gemalt. Auslöser der
Umstellung war ein Satz von Gemeldet am 27.08.2026: „die sollen alle gleich groß
sein, sind aber unterschiedlich groß, und die glocke ist sogar die Größte."

Der Grund stand im Code: Die gemalte Glocke füllte ihr Feld randlos aus, ein
Schriftzeichen füllt seine Box aber nur zu 50–70 % — und jedes anders, weil das
der Schriftdesigner so entschieden hat. Dazu zwei Probleme, die sich durch
bloßes Größerstellen nicht lösen ließen:

* **Der Stil passte nicht.** `🗑` und `▶` sind gefüllte Flächen, `⚙ ⟳ ⏻ ✕` dünne
  Striche, die gemalten wieder gefüllt. Drei Handschriften in einer Leiste.
* **Jedes System zeigte etwas anderes.** Windows greift zu `Segoe UI Symbol`,
  macOS und Linux zu etwas ganz anderem. Entwickelt wird auf allen dreien
  und sah am Mac buchstäblich andere Zeichen als seine Nutzer unter Windows — er
  konnte am eigenen Rechner nicht beurteilen, was draußen ankommt.

Am schlimmsten waren die farbigen Emoji (`🟢 🟡 🔵 ⭐`) vor jeder Bauplanzeile:
Die liegen über `U+FFFF`, Windows malt sie über die Farb-Emoji-Schrift als bunte
Klötzchen — und die **ignorieren die eingestellte Farbe**. Ausgerechnet an der
Stelle, die man am häufigsten sieht.

**Kein Zusatzpaket.** `tk.PhotoImage` liest PNG seit Tk 8.6 von sich aus; Pillow
wird nur im Bau-Werkzeug `tools/symbole_bauen.py` gebraucht, nie zur Laufzeit.
Die eiserne Projektregel „reine Standardbibliothek" bleibt unangetastet, und die
fertige `.exe` ist dadurch nicht dicker geworden.

**Ein Symbol ändern:** nicht hier — in `tools/symbole_bauen.py`. Dort steht die
Zuordnung „Bedeutung → Lucide-Vorlage". Dieses Modul lädt nur, was dort
herauskam. Übersicht aller Symbole: siehe Projektnotizen.
"""

import os
import sys
import tkinter as tk


# Die drei Farben, in denen jedes Symbol vorliegt (siehe `symbole_bauen.py`).
# Namen statt Farbwerten, damit der Code sagt, **was** gemeint ist:
# `recolor(GREEN)` heißt „hervorheben", nicht „nimm #9ce430".
GREY, GREEN, LIGHT = 'grau', 'gruen', 'hell'
# Die beiden Zustandsfarben der Bauplanzeilen — Gelb heißt „aus der Game.log,
# noch nicht vom Launcher bestätigt", Blau „neu im Spiel craftbar".
YELLOW, BLUE = 'gelb', 'blau'
# Die Schriftfarbe für Wörter, die **neben** einem Symbol stehen. Tk faerbt
# Text sonst schwarz — auf dunklem Grund ist er damit unlesbar.
TEXT_COLOR = '#8b98a5'

# ⚠ **Nur für den Notnagel:** Was jeder Bildsatz als **echte** Farbe bedeutet.
# Steht kein Bild zur Verfügung, wird ein Zeichen gezeichnet — und das braucht
# eine Farbe, die Tk kennt. Die Namen oben sind Satznamen (`grau`, `gruen`),
# keine Farbwerte; sie ungeprüft an Tk zu reichen, hat das Programm beim
# Aufbau der Reiterleiste abstürzen lassen.
_SET_COLORS = {
    'grau':  TEXT_COLOR,
    'gruen': '#9ce430',   # die Markenfarbe
    'hell':  '#e6edf3',
    'gelb':  '#e3b341',
    'blau':  '#4a9eff',
}
# Rot ist keine Zustandsfarbe, sondern ein Wegweiser: Der Reiter „Fehler
# melden“ traegt sie, damit ihn niemand sucht, wenn gerade etwas klemmt.
RED = 'rot'

# ⚠ Muss zu `KNOPF`/`ZEILE` in `tools/symbole_bauen.py` passen. Zwei Skalen,
# weil es auf den Einsatzort ankommt: ein Knopf in der Leiste ist etwas anderes
# als ein Statuspunkt **in** einer Textzeile.
#
# Die Zahlen sind bewusst fest und stammen **nicht** mehr aus
# `font.metrics('linespace')` — Schriftmetriken sind je System verschieden, und
# genau daher kamen die abweichenden Maße zwischen Mac und Windows.
BUTTON = {'klein': 18, 'normal': 22, 'gross': 26, 'sehrgross': 30}
LINE = {'klein': 12, 'normal': 14, 'gross': 16, 'sehrgross': 18}
# ⚠ Eine Stufe groesser als `LINE` — fuer Zeichen, die man **treffen** muss.
# Das ⓘ am rechten Rand der Bauplan-Liste oeffnet den Herkunftskasten; in
# Zeilengroesse (14 px bei „normal") war es zu klein, um es als Schaltflaeche zu
# erkennen und sicher zu treffen. Gemeldet am 27.08.2026. Ein
# eigener Satz statt eines groesseren `LINE`, damit die Statuspunkte im Overlay
# unveraendert bleiben — die will niemand anklicken.
TAPPABLE = {'klein': 14, 'normal': 16, 'gross': 18, 'sehrgross': 22}

# Tk räumt Bilder weg, sobald keine Python-Variable mehr auf sie zeigt — auch
# dann, wenn sie gerade angezeigt werden; das Widget allein hält sie nicht. Ohne
# diesen Halter verschwinden die Symbole, sobald der Aufräumer läuft. Ein
# bekannter Stolperstein in tkinter, und er fällt immer erst im laufenden
# Programm auf.
_CACHE = {}
_MISSING = set()

# Alle angelegten Symbol-Widgets, damit sie beim Umstellen der Schriftgröße
# mitziehen können — dasselbe Muster wie `sprache.anmelden()`.
_WIDGETS = []
_LEVEL = ['normal']


def _bundled(*parts):
    """Pfad zu einer mitgelieferten Datei — im Quellcode wie im fertigen Paket.

    PyInstaller entpackt alles nach `sys._MEIPASS`; daneben zu suchen geht dort
    ins Leere.
    """
    base = getattr(sys, '_MEIPASS', None) or os.path.dirname(
        os.path.dirname(os.path.abspath(__file__)))
    return os.path.join(base, 'assets', 'symbole', *parts)


def set_level(level):
    """Die eingestellte Schriftgröße übernehmen und alle Symbole nachziehen.

    Wird aus `schriftgroesse_anwenden()` gerufen, damit die Symbole sofort
    mitwachsen — ohne Neustart, so wie die Schriften auch.
    """
    if level not in BUTTON:
        level = 'normal'
    _LEVEL[0] = level
    alive = []
    for w in _WIDGETS:
        try:
            w.resize()
            alive.append(w)
        except Exception:
            pass                       # Fenster war schon zu — Eintrag fällt weg
    _WIDGETS[:] = alive


def level():
    """Die gerade gültige Stufe."""
    return _LEVEL[0]


def width(sizes=None):
    """Kantenlänge in Pixeln für die aktuelle Stufe."""
    return (sizes or BUTTON).get(_LEVEL[0], 22)


def photo(name, px, color=GREY, master=None):
    """Ein Symbol als `tk.PhotoImage` — beim zweiten Mal aus dem Speicher.

    ⚠ **`master` ist Pflicht, sobald es mehr als einen Tk-Interpreter gibt.**
    Ein `PhotoImage` gehört immer zu genau einem — ohne Angabe nimmt Tk den
    zuerst erzeugten. Wird der geschlossen und ein neuer aufgemacht (im
    Selbsttest passiert das mehrfach), zeigt der Speicher auf Bilder eines toten
    Interpreters, und Tk meldet `image "pyimageN" does not exist`. Deshalb hängt
    der Interpreter im Schlüssel mit drin.

    Gibt `None` zurück, wenn die Datei fehlt. Der aufrufende Code fällt dann auf
    Text zurück, statt abzubrechen: Ein fehlendes Symbol ist ein
    Schönheitsfehler, kein Grund, das Programm anzuhalten.
    """
    interp = id(master.tk) if master is not None else 0
    key = (interp, name, px, color)
    if key in _CACHE:
        return _CACHE[key]
    if (name, px, color) in _MISSING:
        return None
    try:
        _CACHE[key] = tk.PhotoImage(
            file=_bundled(str(px), '%s-%s.png' % (name, color)),
            master=master)
        return _CACHE[key]
    except Exception:
        _MISSING.add((name, px, color))
        return None


def _build(parent, name, sizes, action, color, background, fallback, text,
           font):
    """Gemeinsamer Kern von `button()` und `line()`."""
    background = background if background is not None else parent['bg']
    px = sizes.get(_LEVEL[0], 22)
    b = photo(name, px, color, parent)

    common = dict(bg=background, bd=0, highlightthickness=0)
    if action:
        common['cursor'] = 'hand2'

    if b is not None:
        w = tk.Label(parent, image=b, **common)
        w.image = b                    # zusätzlicher Halter am Widget selbst
    else:
        # Notnagel: Fehlt die Bilddatei, steht wenigstens ein Zeichen da, statt
        # einer leeren Lücke, die niemand als Knopf erkennt.
        w = tk.Label(parent, text=fallback, fg=TEXT_COLOR, **common)
        if font is not None:
            w.configure(font=font)

    if text:
        # Bild **und** Wort — ein Symbol allein erklärt sich nur dem, der es
        # gebaut hat. Tk kann beides in einem Label, das spart einen Rahmen.
        #
        # ⚠ `fg` gehört hierher, nicht nur in den Notnagel oben. Lädt das Bild
        # normal, bekam das Label bis 04.09.2026 **nie** eine Vordergrundfarbe —
        # Tk nahm seinen Standard, und der ist Schwarz. Auf dem dunklen Grund
        # war „n weitere Wege zu diesem Bauplan" dadurch kaum zu lesen.
        w.configure(text=text, compound='left', padx=4, fg=TEXT_COLOR)
        if font is not None:
            w.configure(font=font)

    w.symbol = name
    w.symbol_sizes = sizes
    w.symbol_color = color

    def show():
        n = photo(w.symbol, w.symbol_sizes.get(_LEVEL[0], 22),
                 w.symbol_color, w)
        if n is not None:
            w.configure(image=n)
            w.image = n

    def recolor(new_color):
        """Statt `configure(fg=…)` — ein Bild nimmt keine Vordergrundfarbe an,
        es muss gegen eine andersfarbige Version getauscht werden.

        ⚠⚠ **Der Notnagel darf nicht schlimmer sein als die Lücke.** Die Namen
        hier (`grau`, `gruen`, `hell`) benennen **Bildsätze**, keine Farben —
        Tk kennt sie nicht. Fehlt die Bilddatei, ging genau dieser Name als
        `fg` an Tk, und das ganze Programm brach beim Aufbau der Reiterleiste
        ab: `TclError: unknown color name "grau"`.

        Damit war der Fall „Symbol noch nicht gebaut" kein fehlendes Bild,
        sondern ein Programm, das gar nicht erst startet — und der Notnagel
        zwei Funktionen weiter oben lief nie. Am 06.09.2026 aufgefallen, als
        ein neuer Reiter angelegt wurde, bevor sein Bild da war.
        """
        w.symbol_color = new_color
        if b is not None:
            w.configure(fg=w.cget('fg'))
        else:
            w.configure(fg=_SET_COLORS.get(new_color, TEXT_COLOR))
        show()

    def swap(new_name):
        """Ein anderes Motiv zeigen — etwa Pfeil auf/zu beim Umklappen."""
        w.symbol = new_name
        show()

    w.recolor = recolor
    w.swap_symbol = swap
    w.resize = show
    _WIDGETS.append(w)

    if action:
        w.bind('<Button-1>', lambda e: action())
    return w


def button(parent, name, action=None, color=GREY, background=None,
           fallback='', text='', font=None):
    """Ein anklickbares Symbol in einer Leiste (Melde-Leiste, Reiter, Titel).

    Am Rückgabewert hängen drei Zusätze, die `configure()` hier nicht leisten
    kann: `.recolor(color)`, `.swap_symbol(name)` und `.resize()`.
    """
    return _build(parent, name, BUTTON, action, color, background, fallback,
                  text, font)


def tappable(parent, name, action=None, color=GREY, background=None,
             fallback='', text='', font=None):
    """Wie `line()`, nur eine Stufe groesser — fuer Zeichen zum Anklicken.

    Zwischen `line()` (blosse Anzeige) und `button()` (eigene Schaltflaeche in
    einer Leiste): sitzt in einer Textzeile, ist aber ein Bedienelement und
    muss deshalb getroffen werden koennen."""
    return _build(parent, name, TAPPABLE, action, color, background, fallback,
                  text, font)


def line(parent, name, action=None, color=GREY, background=None, fallback='',
         text='', font=None):
    """Ein kleines Symbol **in** einer Textzeile — Statuspunkt, Haken, Pfeil.

    Kleiner als `button()`, damit es zur Textgröße passt und die Zeilenhöhe
    nicht aufbläht.
    """
    return _build(parent, name, LINE, action, color, background, fallback,
                  text, font)


# Welche Symbole es gibt — für den Selbsttest. Die Zuordnung zu den
# Lucide-Vorlagen steht in `tools/symbole_bauen.py`.
BUTTON_NAMES = (
    'starten', 'glocke', 'liste', 'einstellungen', 'einklappen', 'ausklappen',
    'leeren', 'schliessen', 'ziehgriff', 'fortschritt', 'anzeige',
    'auftragstexte', 'bestand', 'wasistneu', 'ueber', 'serverstatus', 'ordner',
    'erkennung', 'joysticks', 'achsen', 'blickwinkel', 'diagnose',
    'einrichtung', 'neustart',
    'herunterladen',
    'zurueck', 'ausblenden', 'sicherung', 'laeden', 'routen', 'zeit', 'hangar',
    'wunschliste', 'farmliste', 'zerlegen', 'einkaufsliste', 'raffinerie',
    # Der Ziehgriff in vier Richtungen — er zeigt dorthin, wohin sich das
    # Fenster ziehen laesst (siehe `Overlay.GRIFF_SYMBOLE`).
    'ziehen_ol', 'ziehen_or', 'ziehen_ul', 'ziehen_ur',
)
LINE_NAMES = (
    'bestaetigt', 'vorlaeufig', 'punkt', 'gemerkt', 'haken', 'offen',
    'standard', 'aufklappen', 'zuklappen', 'hinweiszeile', 'kaffee',
    'ausblenden',
)
ALL_NAMES = BUTTON_NAMES + LINE_NAMES
