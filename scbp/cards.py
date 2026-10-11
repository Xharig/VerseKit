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
Basetool und Info gibt es nur hier: Basetool bündelt Seiten aus
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
     ('herstellung', 'farmliste', 'lager', 'bergbau', 'raffinerien')),
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

# Reiter einer Gruppe, die in ihrem Bereich fehlen, weil sie nur mit dem
# Basetool etwas zeigen — sie stehen im Bereich Basetool.
BASETOOL_ONLY = {'werkstatt': ('bedarf',)}

# Unterseiten, die im Bereich Fortgeschritten stehen — auf der Karte als
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
# Was eine Karte links und rechts neben ihrer Knopfreihe braucht: Einzug um
# die Rundung und Innenabstand, je Seite.
FOOT_MARGIN = 2 * (RADIUS + 16)


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
        # Eine Karte mit vielen Knöpfen braucht mehr als die Mindestbreite —
        # dann gilt ihre Knopfreihe, sonst quetscht Tk die Knöpfe zusammen.
        foot = getattr(getattr(cell, 'card', None), 'foot', None)
        if foot is not None:
            # Die nötige Breite steht erst nach dem Ausmessen fest.
            foot.update_idletasks()
            need = foot.winfo_reqwidth() + FOOT_MARGIN
            if need > self.min_width:
                self.min_width = need
                width = self.frame.winfo_width()
                if width > 1:
                    self.columns = 0
                    self._place(columns_for(width, self.min_width))
                    return cell
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
        # Karten in den geraden Spalten, der Abstand als eigene schmale Spalte
        # dazwischen — so ist jede Karte gleich breit, auch die am Rand.
        old = self.columns
        self.columns = columns
        for column in range(max(old, columns) * 2):
            if column >= columns * 2 - 1:
                self.frame.columnconfigure(column, weight=0, uniform='',
                                           minsize=0)
            elif column % 2:
                self.frame.columnconfigure(column, weight=0, uniform='',
                                           minsize=GAP)
            else:
                self.frame.columnconfigure(column, weight=1, uniform='karte',
                                           minsize=0)
        for index, cell in enumerate(self.cells):
            cell.grid(row=index // columns, column=(index % columns) * 2,
                      sticky='nsew', padx=0, pady=(0, GAP))


def card(window, cell, symbol, title, text, action, pin=None,
         button_text=None, extras=None, button_color=None):
    """Eine Karte: Symbol und Name, Kurzerklärung, Knopf „Öffnen".

    Die ganze Karte ist klickbar; beim Überfahren leuchtet der Rand. `pin` ist
    ein Paar `(angeheftet, umschalten)` — dann steht oben rechts die
    Stecknadel; `umschalten()` gibt den neuen Zustand zurück. `extras` sind
    Bausteine `build(fuß)` für Zusatzknöpfe (`page_extras`)."""
    from . import icons
    from .main_window import round_button, round_frame
    from .pages import _wrap
    box = round_frame(cell, theme.SURFACE, theme.LINE, radius=RADIUS,
                      base_color=theme.BG, stretch=True, bottom=True)
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
        icons.hover_group(needle)

        def set_pin(on):
            state['on'] = bool(on)
            needle.swap_symbol('loesen' if state['on'] else 'anheften')
            needle.recolor(icons.GREEN if state['on'] else icons.GREY)
        box.set_pin = set_pin
        box.is_pinned = lambda: state['on']
        from . import notice
        notice.attach(needle, lambda: t('k_nadel_loesen' if state['on']
                                        else 'k_nadel_anheften'))
        box.needle = needle
        # Die Nadel ist kein Griff zum Verschieben.
        needle.tile_link = True
        skip.append(needle)
    body = tk.Label(box, text=text, bg=theme.SURFACE, fg=theme.SUB,
                    font=window.f_small, anchor='nw', justify='left')
    body.pack(fill='x', padx=16)
    _wrap(body, inset=32)
    # Die Knopfzeile sitzt im Boden des Kastens — unten am Rand, in jeder
    # Karte der Reihe gleich tief.
    bottom = box.bottom
    foot = tk.Frame(bottom, bg=theme.SURFACE)
    foot.pack(fill='x', padx=16, pady=(8, 14))
    knopf = round_button(foot, button_text or t('k_oeffnen'), action,
                         window.f_small, theme.SURFACE, theme.SURFACE,
                         button_color or theme.ACCENT,
                         button_color or theme.ACCENT, radius=8,
                         padding=(14, 5))
    knopf.pack(side='right')
    skip.append(knopf)
    # Zusatzknöpfe links neben „Öffnen", der erste direkt daneben. Jeder
    # Baustein packt sich selbst (manche nur unter einer Bedingung).
    box.extra_buttons = []
    extra_bar = tk.Frame(bottom, bg=theme.SURFACE)
    for build in extras or ():
        try:
            widget = build(extra_bar)
        except Exception as exc:
            errors.record('cards.extra', exc)
            continue
        if widget is not None:
            skip.append(widget)
            box.extra_buttons.append(widget)
    box.extra_bar = extra_bar
    # Passen die Zusatzknöpfe nicht mehr neben den Öffnen-Knopf (schmale
    # Karte, große Schrift), stehen sie in einer eigenen Zeile darüber —
    # sonst quetscht Tk sie zusammen.
    layout = {'wrapped': None}

    def fit(_e=None):
        if not box.extra_buttons:
            return
        try:
            need = knopf.winfo_reqwidth() + extra_bar.winfo_reqwidth()
            wrapped = need > foot.winfo_width() > 1
            if wrapped == layout['wrapped']:
                return
            layout['wrapped'] = wrapped
            extra_bar.pack_forget()
            if wrapped:
                # Rechts 8 px weniger: Der letzte Zusatzknopf trägt seinen
                # Abstand zum Öffnen-Knopf selbst, so stehen beide Reihen
                # bündig.
                extra_bar.pack(in_=bottom, before=foot, anchor='e',
                               padx=(16, 8), pady=(8, 0))
            else:
                extra_bar.pack(in_=foot, side='right', after=knopf)
        except tk.TclError:
            pass
    extra_bar.pack(in_=foot, side='right')
    layout['wrapped'] = False
    foot.bind('<Configure>', fit, add='+')
    box.fit_extras = fit

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

    def bind_tree(widget):
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
            bind_tree(child)
    bind_tree(box.holder)
    box.open_button = knopf
    box.foot = foot
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


def _small_button(window, parent, text, action, color=None):
    """Ein Knopf im Stil von `k_oeffnen` — `color` für Rand und Schrift."""
    from .main_window import round_button
    color = color or theme.ACCENT
    return round_button(parent, text, action, window.f_small, theme.SURFACE,
                        theme.SURFACE, color, color, radius=8, padding=(14, 5))


def _place_extra(widget):
    widget.pack(side='right', padx=(0, 8))
    return widget


def _counted(key, action):
    """`action` ausführen und den Klick zählen (`page_usage.ACTIONS`)."""
    def run(*args):
        from . import page_usage
        page_usage.action(key)
        return action(*args)
    return run


def _scan_extra(window):
    def build(parent):
        from . import overview
        return _place_extra(_small_button(
            window, parent, t('k_raff_scanner'),
            _counted('karte_raffinerie',
                     lambda: overview.refinery_scan_now(window))))
    return build


# Kennung der Abgleich-Karte im Bereich Basetool — keine Seite.
SYNC_CARD = 'abgleich'


def sync_now(window):
    """Den Basetool-Abgleich sofort anstoßen und das Ergebnis in der
    Statuszeile melden. Ohne Verbindung geht es zur Verbindungsseite."""
    from . import basetool, basetool_sync, page_usage, pages
    page_usage.action('abgleich_karte')
    try:
        ready = bool(basetool.CONNECTION.connected()
                     and basetool_sync.enabled())
    except Exception:
        ready = False
    if not ready:
        window.say(t('k_sync_erst_verbinden'))
        window.open_page('basetool', via='karte', area='basetool')
        return False

    def finished():
        status = basetool_sync.STATUS
        if status.get('running') or status.get('state') == 'running':
            return
        if done in basetool_sync.LISTENERS:
            basetool_sync.LISTENERS.remove(done)
        window.say(pages._storage_sync_result()[0])

    def done():
        pages._from_thread(window.root, finished)
    basetool_sync.LISTENERS.append(done)
    basetool_sync.request_now()
    window.say(t('s_lg_sync_laeuft'))
    return True


def _sync_card(window, cell):
    card(window, cell, 'neustart', t('k_sync_titel'), t('k_sync_text'),
         lambda: sync_now(window), button_text=t('s_lg_sync'),
         button_color=theme.KRT_ORANGE)
    cell.page_key = SYNC_CARD


def _mining_scanner_extra(window):
    """Mining-Scanner an/aus — derselbe Schalter wie auf der Bergbau-Seite
    und das Auge im Overlay. Grün gefüllt, solange er an ist."""
    def build(parent):
        from . import paths, signature_watch

        def is_on():
            return paths.setting_bool(signature_watch.SETTING, False)
        button = _place_extra(_small_button(window, parent, t('k_scanner'),
                                            None, color=theme.LINE))

        def paint(on):
            if on:
                button.restyle(theme.ACCENT, theme.ACCENT, theme.BG)
            else:
                button.restyle(theme.SURFACE, theme.LINE, theme.SUB)

        def click(_e=None):
            from . import page_usage
            page_usage.action('karte_scanner')
            new_value = signature_watch.set_enabled(not is_on())
            window.say(t('s_bg_scan_sagen')
                       % (t('e_an') if new_value else t('e_aus')))
            paint(new_value)
            return 'break'
        button.bind('<Button-1>', click)

        def follow(on):
            # Ist die Karte weg, wirft das — dann meldet `set_enabled` sie ab.
            if not button.winfo_exists():
                raise tk.TclError('Karte geschlossen')
            paint(on)
        signature_watch.on_switch(follow)
        paint(is_on())
        button.toggle = click
        button.is_on = is_on
        return button
    return build


def _translation_extras(window):
    def check(parent):
        from . import overview
        button = _small_button(window, parent, t('k_tr_pruefen'), None)
        button.configure(cursor='hand2')
        button.bind('<Button-1>', _counted(
            'karte_tr_pruefen',
            lambda _e: overview.translation_check(window, button)))
        return _place_extra(button)

    def reload(parent):
        from . import overview
        return _place_extra(_small_button(
            window, parent, t('k_tr_neu'),
            _counted('karte_tr_neu',
                     lambda: overview.translation_reload(window))))
    # Gepackt von rechts: `k_tr_pruefen` direkt neben dem Öffnen-Knopf,
    # `k_tr_neu` links davon.
    return [check, reload]


def page_extras(window, page, kennung):
    """Die Zusatzknöpfe einer Unterpunkt-Karte — was man oft braucht, ohne
    erst die Seite zu öffnen."""
    builders = []
    if page == 'lager':
        try:
            from . import refinery_scan
            if refinery_scan.supported():
                builders.append(_scan_extra(window))
        except Exception as exc:
            errors.record('cards.scan_extra', exc)
    if page == 'bergbau':
        try:
            from . import scan_window
            if scan_window.available():
                builders.append(_mining_scanner_extra(window))
        except Exception as exc:
            errors.record('cards.mining_extra', exc)
    if page == 'uebersetzung':
        builders.extend(_translation_extras(window))
    return builders


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
                      lambda: page in set_pinned(page, page not in pinned())),
                 extras=page_extras(window, page, kennung))
        cell = grid.add(build)
        cell.page_key = page
        # Im Bereich Basetool steht der Abgleich als eigene Karte gleich
        # hinter der Verbindung.
        if kennung == 'basetool' and page == 'basetool':
            grid.add(lambda cell: _sync_card(window, cell))
    frame.card_grid = grid

    def refresh():
        # Angeheftet kann auch an der Seitenüberschrift werden — beim Zeigen
        # den Stand der Nadeln nachziehen.
        now = pinned()
        for cell in grid.cells:
            try:
                cell.card.set_pin(cell.page_key in now)
            except (AttributeError, tk.TclError):
                pass
    window.on_show[area_page_id(kennung)] = refresh
