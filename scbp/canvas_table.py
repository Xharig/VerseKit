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
Eine Tabelle als Elemente auf der Rollleinwand einer Seite.

`Area` ersetzt `pages._scroll_area` für Seiten mit einer langen Tabelle: Kopf
(`top`) und Fuß (`bottom`) sind gewöhnliche Rahmen, dazwischen zeichnet
`Table` ihre Zeilen als Text-, Bild- und Rechteck-Elemente direkt auf die
Leinwand. Eine Zeile ist damit kein Satz eigener Fenster; Auf- und Zuklappen
verschiebt Elemente und den Fuß, kein Fenster ändert seine Größe.

Eine Zeile (`Row`) beschreibt ihre Zellen; gezeichnet wird sie erst, wenn sie
in der Tabelle steht. Formulare unter einer Zeile bleiben echte Rahmen
(`Table.insert_widget`).
"""
import tkinter as tk
import tkinter.font as tkfont

from . import icons, notice
from .pages import BG, SUB, ACCENT

INDENT = 28
PAD = 8
IPAD = 3
ROW_TAG = 'tbl'
CLICK_TAG = 'act'


class Area:
    """Ein Rollbereich aus Kopf, Tabelle und Fuß auf einer Leinwand.

    `top` und `bottom` sind Rahmen zum Packen wie das Ergebnis von
    `pages._scroll_area` und tragen wie dieses die Leinwand als `canvas`.

    Gerechnet wird mit der angeforderten Höhe von Kopf und Fuß: Ein
    Fenster-Element außerhalb des sichtbaren Teils bekommt von Tk keine
    Größe. Wer Kopf oder Fuß ändert, während sie nicht zu sehen sind, ruft
    `refresh()`."""

    def __init__(self, frame, inset=24, gap=8):
        from .main_window import bind_wheel, round_scrollbar
        outer = tk.Frame(frame, bg=BG)
        outer.pack(fill='both', expand=True)
        canvas = tk.Canvas(outer, bg=BG, highlightthickness=0)
        bar = round_scrollbar(outer, canvas, bg=BG)
        canvas.configure(yscrollcommand=bar.set)
        bar.pack(side='right', fill='y')
        canvas.pack(side='left', fill='both', expand=True)
        self.canvas = canvas
        self.inset = inset
        self.gap = gap
        self.table = None
        self.width = 1
        self.ids = {}
        self.holders = {}
        self.fonts = {}
        self.pending = None
        self.top = self._part('top')
        self.bottom = self._part('bottom')
        canvas.bind('<Configure>', self._resized)
        bind_wheel(canvas)

    def _part(self, name):
        holder = tk.Frame(self.canvas, bg=BG)
        self.holders[name] = holder
        self.ids[name] = self.canvas.create_window(
            (0, 0), window=holder, anchor='nw', width=1)
        holder.bind('<Configure>', lambda _e: self.layout())
        target = holder
        if self.inset:
            target = tk.Frame(holder, bg=BG)
            target.pack(fill='both', expand=True, padx=self.inset)
        target.canvas = self.canvas
        return target

    def font(self, font):
        """`font` als Schrift im Interpreter dieser Leinwand. Messen und
        Zeichnen laufen dann durch denselben Interpreter, auch wenn die
        übergebene Schrift an einer anderen Tk-Wurzel hängt."""
        if getattr(font, '_tk', None) is self.canvas.tk:
            return font
        key = str(font)
        if key not in self.fonts:
            try:
                spec = font.actual()
            except (tk.TclError, AttributeError):
                return font
            self.fonts[key] = tkfont.Font(root=self.canvas, **spec)
        return self.fonts[key]

    def part_height(self, name):
        """Die angeforderte Höhe von Kopf (`top`) oder Fuß (`bottom`)."""
        try:
            return self.holders[name].winfo_reqheight()
        except tk.TclError:
            return 0

    def _resized(self, event):
        for item in self.ids.values():
            self.canvas.itemconfigure(item, width=event.width)
        if event.width != self.width:
            self.width = event.width
            if self.table is not None:
                self.table.set_width(self.table_width())
        self.layout()

    def table_width(self):
        """Breite der Tabelle: die Leinwand ohne die Ränder."""
        return max(1, self.width - 2 * self.inset)

    def table_top(self):
        """Wo die Tabelle beginnt (Leinwand-Koordinate)."""
        return self.part_height('top') + self.gap

    def set_table(self, table):
        """Die gezeigte Tabelle tauschen; die alte verschwindet ganz."""
        if self.table is not None and self.table is not table:
            self.table.discard()
        self.table = table
        if table is not None:
            table.set_width(self.table_width())
        self.layout()
        self.refresh()

    def refresh(self):
        """Neu anordnen, sobald Tk die neuen Größen von Kopf und Fuß kennt."""
        if self.pending is not None:
            return

        def run():
            self.pending = None
            self.layout()
        try:
            self.pending = self.canvas.after(20, run)
        except tk.TclError:
            self.pending = None

    def layout(self):
        """Tabelle unter den Kopf, Fuß unter die Tabelle, Rollbereich neu."""
        top = self.table_top()
        height = 0
        if self.table is not None:
            self.table.move_to(self.inset, top)
            height = self.table.height()
        try:
            self.canvas.coords(self.ids['bottom'], 0, top + height)
            self.canvas.configure(scrollregion=(
                0, 0, self.width,
                top + height + self.part_height('bottom')))
        except tk.TclError:
            pass

    def see(self, y, margin=40):
        """So rollen, dass die Leinwand-Höhe `y` oben im Bild steht."""
        try:
            total = float(self.canvas.cget('scrollregion').split()[3])
        except (tk.TclError, IndexError, ValueError):
            return
        if total > 0:
            self.canvas.yview_moveto(max(0.0, (y - margin) / total))


class Column:
    """Eine Spalte: Schlüssel, Überschrift, Ausrichtung (`w`/`e`), auf Klick
    `on_click` (sortieren), `active` für die gerade sortierende."""

    def __init__(self, key, title='', anchor='w', on_click=None,
                 active=False):
        self.key = key
        self.title = title
        self.anchor = anchor
        self.on_click = on_click
        self.active = active


class Name:
    """Die Namenszelle: Name, Kürzel, Abzeichen `(Text, Farbe)` und klein
    dahinter `extra`. `indent` rückt eine Unterzeile ein."""

    def __init__(self, text, badges=(), tag=None, indent=False, extra='',
                 color=None):
        self.text = text
        self.badges = list(badges)
        self.tag = tag
        self.indent = indent
        self.extra = extra
        self.color = color


class Icons:
    """Symbole am rechten Rand einer Zelle, von rechts nach links:
    `(Symbol, Aktion, Erklärtext)`, wahlweise ergänzt um die Farbe (ein
    Bildsatz aus `icons`) und den Schlüssel in `Row.action_icons` (ab Werk
    der Symbolname). Ohne Aktion ist es ein Zeichen in der Zeile
    (`icons.LINE`), mit Aktion ein Knopf (`icons.TAPPABLE`)."""

    def __init__(self, entries, right=PAD, between=4):
        self.entries = list(entries)
        self.right = right
        self.between = between


class Row:
    """Eine Tabellenzeile. `cells` je Spaltenschlüssel `(Inhalt, Farbe)`;
    ein Inhalt ist Text, `Name` oder `Icons`. `on_click` gilt der ganzen
    Zeile außer ihren Symbolen.

    Gezeichnet trägt sie `y`, `height`, `bg`, `action_icons` (Symbol-Name ->
    `icons.CanvasSymbol`) und `arrow` (das erste Symbol ohne Aktion)."""

    _next = [0]

    def __init__(self, kind, what, cells, on_click=None):
        Row._next[0] += 1
        self.id = Row._next[0]
        self.tag = 'r%d' % self.id
        self.kind = kind
        self.what = what
        self.cells = cells
        self.on_click = on_click
        self.table = None
        self.y = 0
        self.height = 0
        self.bg = BG
        self.items = {}
        self.action_icons = {}
        self.arrow = None
        self.drawn = False

    def texts(self):
        """Alle Texte, die die Zeile gerade zeigt."""
        if not self.drawn or self.table is None:
            return []
        canvas = self.table.canvas
        out = []
        for item in canvas.find_withtag(self.tag):
            if canvas.type(item) == 'text':
                out.append(str(canvas.itemcget(item, 'text')))
        return out

    def exists(self):
        """Steht die Zeile noch auf der Leinwand?"""
        return self.drawn and self.table is not None and self.table.alive


class Slot:
    """Ein echter Rahmen zwischen zwei Zeilen (etwa ein Formular)."""

    def __init__(self, widget, pady=(0, 0), padx=(0, 0)):
        self.widget = widget
        self.pady = pady
        self.padx = padx
        self.kind = 'widget'
        self.what = None
        self.tag = None
        self.item = None
        self.y = 0
        self.height = 0
        self.drawn = False
        self.table = None


class Table:
    """Eine Tabelle auf der Leinwand eines `Area`.

    `measure` legt die Spaltenbreiten aus allen übergebenen Zeilen fest — auch
    aus denen, die erst beim Aufklappen erscheinen, damit Auf- und Zuklappen
    keine Spalte verschiebt. `color_of(row, n)` liefert den Grund der n-ten
    Datenzeile."""

    def __init__(self, area, window, columns, color_of=None):
        self.area = area
        self.canvas = area.canvas
        self.window = window
        self.columns = list(columns)
        self.keys = [c.key for c in self.columns]
        self.color_of = color_of or (lambda _row, _n: BG)
        self.base = area.font(window.f_base)
        self.small = area.font(window.f_small)
        self.rows = []
        self.widths = {}
        self.natural = {}
        self.width = 1
        self.x = 0
        self.y = 0
        self.alive = True
        self.head_items = []
        self.head_height = (self.small.metrics('linespace') + 2 * IPAD
                            + 2)
        self.badge_height = self.small.metrics('linespace') + 8

    def data_rows(self):
        """Die Zeilen in Tabellenreihenfolge, ohne Formulare."""
        return [r for r in self.rows if isinstance(r, Row)]

    def height(self):
        """Höhe der Tabelle in Pixeln; ohne Zeilen 0."""
        if not self.rows:
            return 0
        return self.head_height + sum(r.height for r in self.rows)

    def _cell_width(self, key, content):
        if isinstance(content, Name):
            return self._name_layout(content, 10 ** 6)[1]
        if isinstance(content, Icons):
            return self._icons_width(content)
        return self.small.measure(str(content)) + 2 * PAD + 4

    def _icons_width(self, spec):
        width = spec.right
        for entry in spec.entries:
            action = entry[1]
            sizes = icons.TAPPABLE if action else icons.LINE
            width += icons.box_px(sizes, action) + spec.between
        return width

    def measure(self, rows):
        """Spaltenbreiten aus Überschriften und `rows` bestimmen."""
        natural = {}
        for column in self.columns:
            natural[column.key] = (self.small.measure(column.title.upper())
                                   + 2 * PAD + 4 if column.title else 0)
        for row in rows:
            for key, (content, _color) in row.cells.items():
                if key in natural:
                    natural[key] = max(natural[key],
                                       self._cell_width(key, content))
        self.natural = natural
        self._spread()

    def _spread(self):
        others = sum(w for k, w in self.natural.items() if k != self.keys[0])
        widths = dict(self.natural)
        widths[self.keys[0]] = max(self.natural.get(self.keys[0], 0),
                                   self.width - others)
        if self.width - others < self.natural.get(self.keys[0], 0):
            widths[self.keys[0]] = max(60, self.width - others)
        self.widths = widths

    def _column_x(self, key):
        x = self.x
        for k in self.keys:
            if k == key:
                return x
            x += self.widths.get(k, 0)
        return x

    def set_width(self, width):
        """Neue Breite: Spalten neu verteilen und alles neu zeichnen."""
        if width == self.width:
            return
        self.width = width
        self._spread()
        self.redraw()

    def move_to(self, x, y):
        """Die Tabelle an `(x, y)` rücken."""
        dx, dy = x - self.x, y - self.y
        if not (dx or dy):
            return
        self.x, self.y = x, y
        if not self.alive:
            return
        self.canvas.move(ROW_TAG, dx, dy)
        for row in self.rows:
            row.y += dy

    def discard(self):
        """Alle Elemente und Formular-Fenster der Tabelle entfernen."""
        for row in self.rows:
            self._undraw(row)
        self._undraw_head()
        self.alive = False

    def show(self, rows):
        """Die Zeilen `rows` in dieser Reihenfolge zeigen."""
        for row in self.rows:
            self._undraw(row)
        self.rows = list(rows)
        for row in self.rows:
            row.table = self
        self.redraw()

    def redraw(self):
        """Alles neu zeichnen — nach einer Breitenänderung."""
        if not self.alive:
            return
        self._undraw_head()
        for row in self.rows:
            self._undraw(row)
        if self.width <= 1 or not self.rows:
            for row in self.rows:
                row.y = self.y
            return
        self._draw_head()
        y = self.y + self.head_height
        for row in self.rows:
            self._draw(row, y)
            y += row.height
        self.restripe()

    def _undraw_head(self):
        for item in self.head_items:
            self.canvas.delete(item)
        self.head_items = []

    def _draw_head(self):
        canvas = self.canvas
        center = self.y + (self.head_height - 2) / 2.0
        for column in self.columns:
            if not column.title:
                continue
            x0 = self._column_x(column.key)
            width = self.widths.get(column.key, 0)
            x = x0 + PAD + 2 if column.anchor == 'w' else x0 + width - PAD - 2
            item = canvas.create_text(
                x, center, text=column.title.upper(), anchor=column.anchor,
                font=self.small, fill=ACCENT if column.active else SUB,
                tags=(ROW_TAG, 'head'))
            self.head_items.append(item)
            if column.on_click is not None:
                canvas.tag_bind(item, '<Button-1>',
                                lambda _e, run=column.on_click: run())
                canvas.tag_bind(item, '<Enter>', lambda _e: self._cursor(
                    'hand2'))
                canvas.tag_bind(item, '<Leave>', lambda _e: self._cursor(''))

    def _cursor(self, name):
        try:
            self.canvas.configure(cursor=name)
        except tk.TclError:
            pass

    def _row_height(self, row):
        heights = []
        for key, (content, _color) in row.cells.items():
            if isinstance(content, Name):
                font = self.small if content.indent else self.base
                heights.append(font.metrics('linespace') + 6)
                if content.badges:
                    heights.append(self.badge_height)
            elif isinstance(content, Icons):
                for entry in content.entries:
                    action = entry[1]
                    sizes = icons.TAPPABLE if action else icons.LINE
                    heights.append(icons.box_px(sizes, action))
            else:
                heights.append(self.small.metrics('linespace') + 6)
        return max(heights or [0]) + 2 * IPAD

    def _draw(self, row, y):
        if isinstance(row, Slot):
            self._draw_slot(row, y)
            return
        canvas = self.canvas
        row.table = self
        row.y = y
        row.height = self._row_height(row)
        row.items = {'fill': []}
        row.action_icons = {}
        row.arrow = None
        tags = (ROW_TAG, row.tag)
        row.items['bg'] = canvas.create_rectangle(
            self.x, y, self.x + self.width, y + row.height, fill=row.bg,
            width=0, tags=tags)
        center = y + row.height / 2.0
        for key in self.keys:
            if key not in row.cells:
                continue
            content, color = row.cells[key]
            x0 = self._column_x(key)
            width = self.widths.get(key, 0)
            column = self.columns[self.keys.index(key)]
            if isinstance(content, Name):
                self._draw_name(row, content, color, x0, width, center, tags)
            elif isinstance(content, Icons):
                self._draw_icons(row, content, x0 + width, center, tags)
            elif content != '':
                text = _fit(self.small, str(content), width - 2 * PAD - 4)
                x = x0 + PAD + 2 if column.anchor == 'w' \
                    else x0 + width - PAD - 2
                canvas.create_text(x, center, text=text, anchor=column.anchor,
                                   font=self.small, fill=color, tags=tags)
        row.drawn = True
        if row.on_click is not None:
            canvas.tag_bind(row.tag, '<Button-1>',
                            lambda e, r=row: self._clicked(r))
            canvas.tag_bind(row.tag, '<Enter>',
                            lambda _e: self._cursor('hand2'))
            canvas.tag_bind(row.tag, '<Leave>', lambda _e: self._cursor(''))

    def _clicked(self, row):
        current = self.canvas.find_withtag('current')
        if current and CLICK_TAG in self.canvas.gettags(current[0]):
            return
        if row.on_click is not None:
            row.on_click()

    def _name_layout(self, spec, width):
        """Wie die Namenszelle in `width` Platz findet: Teile mit ihrer Lage
        und die natürliche Breite."""
        font = self.small if spec.indent else self.base
        parts = []
        x = INDENT if spec.indent else 0
        pad = 0 if spec.indent else PAD
        name = spec.text or ''
        extra = spec.extra or ''
        fixed = x
        if name:
            fixed += 2 * pad + 4
        if spec.tag:
            fixed += 2 + self.small.measure(spec.tag) + 6
        for text, _color in spec.badges:
            fixed += 6 + self.small.measure(text) + 20
        natural = fixed + (font.measure(name) if name else 0)
        if extra:
            natural += 8 + self.small.measure(extra) + 6
        if natural > width and extra:
            room = width - (fixed + (font.measure(name) if name else 0)) \
                - 8 - 6
            extra = _fit(self.small, extra, room) if room > 20 else ''
        if name and fixed + font.measure(name) > width:
            name = _fit(font, name, max(20, width - fixed))
        if name:
            parts.append(('name', x + pad + 2, name, font))
            x += font.measure(name) + 2 * pad + 4
        if spec.tag:
            x += 2
            parts.append(('tag', x + 3, spec.tag, self.small))
            x += self.small.measure(spec.tag) + 6
        for text, color in spec.badges:
            x += 6
            w = self.small.measure(text) + 20
            parts.append(('badge', x, (text, color, w), self.small))
            x += w
        if extra:
            x += 8
            parts.append(('extra', x + 3, extra, self.small))
            x += self.small.measure(extra) + 6
        return parts, natural

    def _draw_name(self, row, spec, color, x0, width, center, tags):
        from .main_window import _round_rect
        canvas = self.canvas
        parts, _natural = self._name_layout(spec, width)
        for kind, x, value, font in parts:
            if kind == 'name':
                canvas.create_text(x0 + x, center, text=value, anchor='w',
                                   font=font, fill=spec.color or color,
                                   tags=tags)
            elif kind in ('tag', 'extra'):
                canvas.create_text(x0 + x, center, text=value, anchor='w',
                                   font=font, fill=SUB, tags=tags)
            else:
                text, badge_color, w = value
                h = self.badge_height
                top = int(round(center - h / 2.0))
                left = int(round(x0 + x))
                bubble = _round_rect(
                    canvas, left + 1, top + 1, left + w - 1, top + h - 1,
                    radius=max(4, h // 3), fill=row.bg, outline=badge_color,
                    width=1, tags=tags)
                row.items['fill'].append(bubble)
                canvas.create_text(left + w / 2.0, top + h / 2.0, text=text,
                                   anchor='center', font=font,
                                   fill=badge_color, tags=tags)

    def _draw_icons(self, row, spec, right, center, tags):
        x = right - spec.right
        for entry in spec.entries:
            symbol, action, tip = entry[:3]
            color = entry[3] if len(entry) > 3 else icons.GREY
            sizes = icons.TAPPABLE if action else icons.LINE
            box = icons.box_px(sizes, action)
            item_tags = tags + ((CLICK_TAG,) if action else ())
            icon = icons.CanvasSymbol(self.canvas, x - box / 2.0, center,
                                      symbol, sizes, action, color=color,
                                      tags=item_tags)
            if tip is not None:
                notice.attach_item(self.canvas, icon.item, tip)
            if action is not None:
                row.action_icons[entry[4] if len(entry) > 4 else symbol] = icon
            elif row.arrow is None:
                row.arrow = icon
            x -= box + spec.between

    def _draw_slot(self, slot, y):
        slot.table = self
        slot.y = y
        try:
            slot.widget.update_idletasks()
            inner = slot.widget.winfo_reqheight()
        except tk.TclError:
            inner = 0
        slot.height = inner + slot.pady[0] + slot.pady[1]
        slot.item = self.canvas.create_window(
            self.x + slot.padx[0], y + slot.pady[0], window=slot.widget,
            anchor='nw', width=max(1, self.width - slot.padx[0]
                                   - slot.padx[1]), tags=(ROW_TAG,))
        slot.drawn = True

    def _undraw(self, row):
        if not row.drawn:
            return
        canvas = self.canvas
        if isinstance(row, Slot):
            try:
                canvas.delete(row.item)
            except tk.TclError:
                pass
            row.drawn = False
            return
        try:
            canvas.delete(row.tag)
            for sequence in ('<Button-1>', '<Enter>', '<Leave>'):
                canvas.tag_unbind(row.tag, sequence)
        except tk.TclError:
            pass
        row.drawn = False
        row.action_icons = {}
        row.arrow = None

    def _shift(self, start, dy, end):
        """Alles zwischen den Leinwand-Höhen `start` und `end` um `dy`
        verschieben."""
        if not dy or start >= end:
            return
        canvas = self.canvas
        canvas.addtag_overlapping('mv', self.x - 2, start + 1,
                                  self.x + self.width + 2, end - 1)
        canvas.move('mv&&' + ROW_TAG, 0, dy)
        canvas.dtag('mv', 'mv')

    def insert(self, after, rows):
        """`rows` hinter `after` (eine Zeile, `None` = ganz oben) zeigen."""
        at = 0 if after is None else self.rows.index(after) + 1
        if self.width <= 1 or not self.alive:
            self.rows[at:at] = rows
            for row in rows:
                row.table = self
            return
        if not self.rows:
            self.rows = list(rows)
            self.redraw()
            self.area.layout()
            return
        start = (self.rows[at].y if at < len(self.rows)
                 else self.y + self.height())
        for row in rows:
            row.height = (self._row_height(row) if isinstance(row, Row)
                          else 0)
        if any(isinstance(r, Slot) for r in rows):
            for row in rows:
                if isinstance(row, Slot):
                    try:
                        row.widget.update_idletasks()
                        row.height = (row.widget.winfo_reqheight()
                                      + row.pady[0] + row.pady[1])
                    except tk.TclError:
                        row.height = 0
        dy = sum(r.height for r in rows)
        self._shift(start, dy, self.y + self.height())
        for row in self.rows[at:]:
            row.y += dy
        y = start
        for row in rows:
            self._draw(row, y)
            y += row.height
        self.rows[at:at] = rows
        self.restripe()
        self.area.layout()

    def remove(self, rows):
        """Diese Zeilen (am Stück) aus der Tabelle nehmen."""
        rows = [r for r in rows if r in self.rows]
        if not rows:
            return
        if self.width <= 1 or not self.alive:
            for row in rows:
                self.rows.remove(row)
            return
        rows.sort(key=self.rows.index)
        start = rows[0].y
        dy = sum(r.height for r in rows)
        end = self.y + self.height()
        for row in rows:
            self._undraw(row)
            self.rows.remove(row)
        after = [r for r in self.rows if r.y >= start + dy]
        if not self.rows:
            self._undraw_head()
        else:
            self._shift(start + dy, -dy, end)
        for row in after:
            row.y -= dy
        self.restripe()
        self.area.layout()

    def insert_widget(self, after, widget, pady=(0, 0), padx=(0, 0)):
        """Einen echten Rahmen (Kind der Leinwand) hinter `after` zeigen."""
        slot = Slot(widget, pady, padx)
        self.insert(after, [slot])
        return slot

    def restripe(self):
        """Jeder Datenzeile ihren Grund aus `color_of` geben."""
        if not self.alive:
            return
        canvas = self.canvas
        for n, row in enumerate(self.data_rows()):
            want = self.color_of(row, n)
            if want == row.bg and row.drawn:
                continue
            row.bg = want
            if not row.drawn:
                continue
            try:
                canvas.itemconfigure(row.items['bg'], fill=want)
                for item in row.items['fill']:
                    canvas.itemconfigure(item, fill=want)
            except tk.TclError:
                pass

    def point(self, row):
        """Ein Punkt in der Zeile in Fensterkoordinaten der Leinwand, nachdem
        sie ins Bild gerollt ist — für Klicks im Selbsttest."""
        self.area.see(row.y, margin=0)
        self.canvas.update_idletasks()
        return (int(self.x + 4 - self.canvas.canvasx(0)),
                int(row.y + row.height / 2.0 - self.canvas.canvasy(0)))


def _fit(font, text, width):
    """`text` mit Auslassungszeichen so kürzen, dass es in `width` Pixel passt."""
    if width <= 0:
        return ''
    if font.measure(text) <= width:
        return text
    low, high = 0, len(text)
    while low < high:
        middle = (low + high + 1) // 2
        if font.measure(text[:middle] + '…') <= width:
            low = middle
        else:
            high = middle - 1
    return text[:low] + '…' if low else ''
