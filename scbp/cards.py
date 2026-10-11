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
Die Kartenansicht: jeder Bereich als Karte, jeder Unterpunkt als Karte mit
kurzer Erklärung — ein zweiter Weg zu denselben Seiten wie die Reiterliste.

Aufbau:

| Stufe | Seite | Inhalt |
|---|---|---|
| 1 | Übersicht | Hauptkarten (`AREAS`) |
| 2 | `bereich_<kennung>` | Unterpunkt-Karten des Bereichs |
| 3 | die bisherige Seite | unverändert |

`AREAS` und `PAGES` müssen zur Reiterliste in `main_window._body` passen
(Symbol und Name je Seite); eine Prüfung vergleicht beides. Die Bereiche
„Basetool" und „Info" gibt es nur hier: Basetool bündelt Seiten aus
verschiedenen Gruppen, Info enthält nur die Seiten, die nicht über Kopf- und
Fußleiste erreichbar sind.

Bausteine für alle Kartenseiten: `card()` (eine Karte) und `CardGrid` (so
viele Spalten, wie mit Mindestbreite nebeneinander passen; Karten einer Reihe
gleich hoch).
"""
import tkinter as tk

from . import errors, theme
from .language import t

# Seite -> (Symbol, Textschlüssel des Namens) — wie in der Reiterliste.
PAGES = {
    'liste': ('liste', 'hf_liste'),
    'fortschritt': ('fortschritt', 'hf_fortschritt'),
    'auftragslog': ('eigenbuch', 'hf_auftragslog'),
    'hangar': ('hangar', 'hf_hangar'),
    'wunschliste': ('wunschliste', 'hf_wunschliste'),
    'einkaufsliste': ('einkaufsliste', 'hf_einkaufsliste'),
    'herstellung': ('blitz', 'hf_herstellung'),
    'farmliste': ('farmliste', 'hf_farmliste'),
    'bedarf': ('basetool', 'hf_bedarf'),
    'lager': ('bestand', 'hf_lager'),
    'bergbau': ('herkunft', 'hf_bergbau'),
    'raffinerien': ('raffinerie', 'hf_raffinerien'),
    'bergung': ('sicherung', 'hf_bergung'),
    'zerlegen': ('zerlegen', 'hf_zerlegen'),
    'laeden': ('laeden', 'hf_laeden'),
    'handelslager': ('handelslager', 'hf_handelslager'),
    'verkauf': ('verkauf', 'hf_verkauf'),
    'routen': ('routen', 'hf_routen'),
    'statistik_auswertung': ('st_auswertung', 'hf_st_auswertung'),
    'statistik': ('statistik', 'hf_st_uebersicht'),
    'statistik_schiffe': ('st_schiffe', 'hf_st_schiffe'),
    'statistik_auftraege': ('st_auftraege', 'hf_st_auftraege'),
    'statistik_quantum': ('st_quantum', 'hf_st_quantum'),
    'statistik_stabil': ('st_stabil', 'hf_st_stabil'),
    'allgemein': ('einstellungen', 'hf_allgemein'),
    'anzeige': ('overlay', 'hf_overlay'),
    'darstellung': ('darstellung', 'hf_darstellung'),
    'ordner': ('ordner', 'hf_ordner'),
    'module': ('module', 'hf_module'),
    'basetool': ('basetool', 'hf_basetool'),
    'bestand': ('sichern', 'hf_sichern'),
    'erkennung': ('erkennung', 'hf_erkennung'),
    'startprogramme': ('startprogramme', 'hf_startprogramme'),
    'spiel': ('auftragstexte', 'hf_spiel'),
    'uebersetzung': ('uebersetzung', 'hf_uebersetzung'),
    'grafik': ('anzeige', 'hf_grafik'),
    'blickwinkel': ('blickwinkel', 'hf_blickwinkel'),
    'joysticks': ('joysticks', 'hf_joysticks'),
    'achsen': ('achsen', 'hf_achsen'),
    'shader': ('leeren', 'hf_shader'),
    'patchaenderungen': ('zeit', 'hf_patchaenderungen'),
    'serverstatus': ('serverstatus', 'hf_serverstatus'),
    'danke': ('quellen', 'hf_danke'),
}

# Die Hauptbereiche in ihrer Reihenfolge: (Kennung, Symbol, Namensschlüssel,
# Unterseiten). Die Kennung ist die der Gruppe in der Reiterliste (für das
# Ausblenden über `modules`), „basetool" ist eigen.
AREAS = (
    ('bauplaene', 'liste', 'hf_gruppe_bp',
     ('liste', 'fortschritt', 'auftragslog')),
    ('schiffe', 'hangar', 'hf_gruppe_schiffe',
     ('hangar', 'wunschliste', 'einkaufsliste')),
    ('werkstatt', 'blitz', 'hf_gruppe_herst',
     ('herstellung', 'farmliste', 'bedarf', 'lager', 'bergbau', 'raffinerien')),
    ('bergung', 'zerlegen', 'hf_gruppe_bergung', ('bergung', 'zerlegen')),
    ('handel', 'laeden', 'hf_gruppe_handel',
     ('laeden', 'handelslager', 'verkauf', 'routen')),
    ('statistiken', 'statistik', 'hf_gruppe_statistik',
     ('statistik_auswertung', 'statistik', 'statistik_schiffe',
      'statistik_auftraege', 'statistik_quantum', 'statistik_stabil')),
    ('spiel_einst', 'joysticks', 'hf_gruppe_spiel',
     ('spiel', 'uebersetzung', 'grafik', 'blickwinkel', 'joysticks', 'achsen',
      'shader')),
    ('einstellungen', 'einstellungen', 'hf_gruppe_einst',
     ('allgemein', 'anzeige', 'darstellung', 'ordner', 'module', 'basetool',
      'bestand', 'erkennung', 'startprogramme')),
    ('basetool', 'basetool', 'k_basetool_titel',
     ('basetool', 'bedarf', 'lager', 'handelslager', 'hangar', 'liste')),
    ('info', 'ueber', 'hf_gruppe_info',
     ('patchaenderungen', 'serverstatus', 'danke')),
)

# Unterseiten, die im Bereich „Fortgeschritten" stehen — auf der Karte als
# Zusatz am Namen.
ADVANCED = ('erkennung', 'startprogramme')

# Basetool lässt sich wie die Module ausblenden; es hat keine Gruppe in der
# Reiterliste und deshalb einen eigenen Schalter.
BASETOOL_SETTING = 'modul_basetool'

# Die angehefteten Unterseiten (Liste von Seitenkennungen, Reihenfolge wie
# gezeigt).
PINNED_SETTING = 'karten_angeheftet'

# Mindestbreiten bei Schriftstufe 0; sie wachsen mit der Schrift.
MAIN_MIN_WIDTH = 220
SUB_MIN_WIDTH = 300
GAP = 12
RADIUS = 12


def area(kennung):
    """Der Eintrag aus `AREAS` — oder None."""
    return next((a for a in AREAS if a[0] == kennung), None)


def area_page_id(kennung):
    return 'bereich_' + kennung


def area_enabled(kennung):
    """Ist der Bereich eingeblendet? Folgt den Modul-Schaltern; Basetool hat
    seinen eigenen, Baupläne, Einstellungen, Spiel-Einstellungen und Info sind
    immer da."""
    from . import modules, paths
    if kennung == 'basetool':
        return paths.setting_bool(BASETOOL_SETTING, True)
    return modules.enabled(kennung)


def visible_areas():
    return [a for a in AREAS if area_enabled(a[0])]


def area_of(page, preferred=None):
    """Zu welchem Bereich eine Seite gehört. `preferred` gewinnt, wenn die
    Seite auch dort steht (Basetool zeigt Seiten aus anderen Bereichen)."""
    if preferred:
        a = area(preferred)
        if a and page in a[3]:
            return preferred
    for kennung, _symbol, _name, pages in AREAS:
        if kennung != 'basetool' and page in pages:
            return kennung
    return None


def description_key(page, in_area=None):
    """Textschlüssel der Kurzerklärung — im Basetool-Bereich eigene Texte."""
    if in_area == 'basetool':
        return 'kb_' + page
    return 'k_' + page


def pinned():
    """Die angehefteten Seiten, nur bekannte, ohne Doppelte."""
    from . import paths
    saved = (paths.settings() or {}).get(PINNED_SETTING)
    result = []
    for page in saved if isinstance(saved, list) else []:
        if page in PAGES and page not in result:
            result.append(page)
    return result


def set_pinned(page, on):
    """Anheften oder lösen; gibt die neue Liste zurück."""
    from . import paths
    current = pinned()
    if on and page not in current:
        current.append(page)
    elif not on and page in current:
        current.remove(page)
    paths.set_setting(PINNED_SETTING, current)
    return current


def swap_pinned(first, second):
    """Zwei angeheftete Karten tauschen die Plätze."""
    from . import paths
    order = pinned()
    if first in order and second in order and first != second:
        i, j = order.index(first), order.index(second)
        order[i], order[j] = order[j], order[i]
        paths.set_setting(PINNED_SETTING, order)
    return order


def columns_for(width, min_width, gap=GAP):
    """Wie viele Karten mit `min_width` nebeneinander passen — mindestens 1."""
    return max(1, int((width + gap) // (min_width + gap)))


def scaled(window, width):
    """Mindestbreite passend zur eingestellten Schrift."""
    try:
        size = abs(int(window.f_base.cget('size')))
    except (AttributeError, tk.TclError, ValueError):
        size = 10
    return int(width * max(size, 8) / 10.0)


# ------------------------------------------------------------------ Bausteine
class CardGrid:
    """Ein Raster, das so viele Spalten zeigt, wie Karten mit Mindestbreite
    nebeneinander passen. Die Karten einer Reihe teilen sich die Breite
    gleichmäßig und sind gleich hoch (`sticky='nsew'`, Rahmen mit
    `stretch=True`).

    `frame` ist der Rahmen zum Packen; `add(build)` legt eine Zelle an und
    ruft `build(zelle)`."""

    def __init__(self, parent, min_width, bg=None):
        self.frame = tk.Frame(parent, bg=bg or theme.BG)
        self.min_width = min_width
        self.cells = []
        self.columns = 0
        self.frame.bind('<Configure>', self._resized)

    def add(self, build):
        cell = tk.Frame(self.frame, bg=self.frame.cget('bg'))
        self.cells.append(cell)
        try:
            build(cell)
        except Exception as exc:
            errors.record('cards.grid', exc)
        self._place(max(self.columns, 1))
        return cell

    def clear(self):
        for cell in self.cells:
            cell.destroy()
        self.cells = []

    def _resized(self, event):
        columns = columns_for(event.width, self.min_width)
        if columns != self.columns:
            self._place(columns)

    def _place(self, columns):
        old = self.columns
        self.columns = columns
        for column in range(max(old, columns)):
            if column < columns:
                self.frame.columnconfigure(column, weight=1, uniform='karte')
            else:
                self.frame.columnconfigure(column, weight=0, uniform='')
        half = GAP // 2
        for index, cell in enumerate(self.cells):
            column = index % columns
            cell.grid(row=index // columns, column=column, sticky='nsew',
                      padx=(0 if column == 0 else half,
                            0 if column == columns - 1 else half),
                      pady=(0, GAP))


def card(window, cell, symbol, title, text, action, pin=None,
         button_text=None):
    """Eine Karte: Symbol und Name, Kurzerklärung, Knopf „Öffnen".

    Die ganze Karte ist klickbar; beim Überfahren leuchtet der Rand. `pin` ist
    ein Paar `(angeheftet, umschalten)` — dann steht oben rechts die
    Stecknadel; `umschalten()` gibt den neuen Zustand zurück."""
    from . import icons
    from .main_window import round_button, round_frame
    from .pages import _wrap
    box = round_frame(cell, theme.SURFACE, theme.LINE, radius=RADIUS,
                      base_color=theme.BG, stretch=True)
    box.holder.pack(fill='both', expand=True)
    head = tk.Frame(box, bg=theme.SURFACE)
    head.pack(fill='x', padx=16, pady=(14, 6))
    mark = icons.button(head, symbol, color=icons.LIGHT,
                        background=theme.SURFACE, font=window.f_icon)
    mark.pack(side='left')
    name = tk.Label(head, text=title, bg=theme.SURFACE, fg=theme.FG,
                    font=window.f_bold, anchor='w', justify='left')
    name.pack(side='left', fill='x', expand=True, padx=(8, 0))
    skip = []
    if pin is not None:
        state = {'on': bool(pin[0])}
        needle = icons.button(head, 'loesen' if state['on'] else 'anheften',
                              color=icons.GREEN if state['on'] else icons.GREY,
                              background=theme.SURFACE, font=window.f_icon)
        needle.pack(side='right')

        def flip(_e=None):
            try:
                state['on'] = bool(pin[1]())
            except Exception as exc:
                errors.record('cards.pin', exc)
                return 'break'
            needle.swap_symbol('loesen' if state['on'] else 'anheften')
            needle.recolor(icons.GREEN if state['on'] else icons.GREY)
            return 'break'
        needle.bind('<Button-1>', flip)
        needle.configure(cursor='hand2')
        from . import notice
        notice.attach(needle, lambda: t('k_nadel_loesen' if state['on']
                                        else 'k_nadel_anheften'))
        box.needle = needle
        # Die Nadel ist kein Griff zum Verschieben.
        needle.tile_link = True
        skip.append(needle)
    body = tk.Label(box, text=text, bg=theme.SURFACE, fg=theme.SUB,
                    font=window.f_small, anchor='nw', justify='left')
    body.pack(fill='both', expand=True, padx=16)
    _wrap(body, inset=32)
    foot = tk.Frame(box, bg=theme.SURFACE)
    foot.pack(fill='x', padx=16, pady=(8, 14))
    knopf = round_button(foot, button_text or t('k_oeffnen'), action,
                         window.f_small, theme.SURFACE, theme.SURFACE,
                         theme.ACCENT, theme.ACCENT, radius=8,
                         padding=(14, 5))
    knopf.pack(side='right')
    skip.append(knopf)

    def outline(color):
        try:
            box.canvas.itemconfigure(box.shape, outline=color)
        except tk.TclError:
            pass

    def leave(_e=None):
        # Beim Wechsel auf ein Kind kommt auch ein `<Leave>` — nur färben,
        # wenn der Zeiger wirklich außerhalb der Karte ist.
        try:
            x, y = box.holder.winfo_pointerxy()
            inside = box.holder.winfo_containing(x, y)
        except (tk.TclError, KeyError):
            inside = None
        while inside is not None and inside is not box.holder:
            inside = getattr(inside, 'master', None)
        if inside is None:
            outline(theme.LINE)

    # Öffnen beim Loslassen, nicht beim Drücken — sonst ließe sich eine Karte
    # nicht an der Überschrift greifen und verschieben. Wer dabei mehr als ein
    # paar Pixel zieht, öffnet nichts.
    press = {'x': None, 'y': None}

    def pressed(event):
        press['x'], press['y'] = event.x_root, event.y_root

    def released(event):
        if press['x'] is None:
            return
        moved = (abs(event.x_root - press['x'])
                 + abs(event.y_root - press['y']))
        press['x'] = None
        if moved <= 6:
            action()

    def bind_all(widget):
        if widget in skip:
            return
        widget.bind('<Button-1>', pressed, add='+')
        widget.bind('<ButtonRelease-1>', released, add='+')
        widget.bind('<Enter>', lambda _e: outline(theme.ACCENT), add='+')
        widget.bind('<Leave>', leave, add='+')
        try:
            widget.configure(cursor='hand2')
        except tk.TclError:
            pass
        for child in widget.winfo_children():
            bind_all(child)
    bind_all(box.holder)
    box.open_button = knopf
    box.open = action
    box.title_label = name
    cell.tile_head = head
    box.text_label = body
    cell.card = box
    return box


# ------------------------------------------------------------------ Seiten
def builders():
    """Je Bereich eine Seite `bereich_<kennung>`."""
    return {area_page_id(a[0]): (lambda window, frame, k=a[0]:
                                  area_page(window, frame, k))
            for a in AREAS}


def page_title(page, in_area=None):
    if in_area == 'basetool' and page == 'basetool':
        return t('k_basetool_verbindung')
    title = t(PAGES[page][1])
    if page in ADVANCED:
        title += ' · ' + t('k_fortgeschritten')
    return title


def area_page(window, frame, kennung):
    """Die Bereichsseite: Überschrift, Erklärung, eine Karte je Unterpunkt."""
    from .pages import _heading, _scroll_area
    entry = area(kennung)
    if entry is None:
        return
    _kennung, _symbol, name_key, pages = entry
    _heading(window, frame, t(name_key), t('ka_' + kennung))
    parent = _scroll_area(frame)
    grid = CardGrid(parent, scaled(window, SUB_MIN_WIDTH))
    grid.frame.pack(fill='x', pady=(0, 16))
    for page in pages:
        symbol = PAGES[page][0]

        def build(cell, page=page, symbol=symbol):
            card(window, cell, symbol, page_title(page, kennung),
                 t(description_key(page, kennung)),
                 lambda: window.open_page(page, via='karte', area=kennung),
                 pin=(page in pinned(),
                      lambda: page in set_pinned(page, page not in pinned())))
        cell = grid.add(build)
        cell.page_key = page
    frame.card_grid = grid
