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
Scharfe Darstellung bei Windows-Skalierung über 100 %.

Ist die Einstellung `scharfe_darstellung` an, meldet sich der Prozess bei
Windows als DPI-bewusst (systemweit), und Tk zeichnet in physischen
Bildpunkten. Das übrige Programm rechnet weiter in logischen Punkten
(96 dpi). Die Umrechnung geschieht an genau einer Stelle — an der Grenze zu
Tkinter:

| Richtung | Was | Rechnung |
|---|---|---|
| hinein | Abstände, Rahmen, Fenster- und Leinwandmaße, Koordinaten | × Faktor |
| heraus | `winfo_*`, Ereignis-Lagen, `cget`, `pack_info`, Schriftmaße | ÷ Faktor |
| Schrift | Größen in Punkt | über `tk scaling` |

Dadurch bleiben gespeicherte Fensterlagen, das Overlay und der Bildschirm-
abgriff (`screen_grab.dpi_scale`) ohne Umrechnung gültig.

`width`/`height` sind bei Label und Knopf **Zeichen**, nicht Bildpunkte — sie
werden nur umgerechnet, wenn das Element ein Bild zeigt.

Ist die Einstellung aus, unter Linux oder bei 100 %, wird nichts verändert.
"""
import os
import re
import sys

SETTING = 'scharfe_darstellung'
FORCE_VARIABLE = 'VERSEKIT_DPI_FAKTOR'

# Kennungen aus winuser.h / shellscalingapi.h.
_SYSTEM_AWARE_CONTEXT = -2
_PROCESS_SYSTEM_DPI_AWARE = 1
_LOGPIXELSX = 88

_FACTOR = [1.0]
_INSTALLED = [False]


def factor():
    """Physische Bildpunkte je logischem Punkt (1,0 = keine Umrechnung)."""
    return _FACTOR[0]


def active():
    """Rechnet dieses Modul gerade um?"""
    return _FACTOR[0] != 1.0


def wanted():
    """Ist die Einstellung an (ungeachtet von System und Skalierung)?"""
    try:
        from . import paths
        return paths.setting_bool(SETTING, False)
    except Exception:
        return False


def px(value):
    """Ein logisches Maß in physische Bildpunkte (ganzzahlig)."""
    if not active():
        return value
    return int(round(value * _FACTOR[0]))


def logical(value):
    """Ein physisches Maß in logische Punkte (ganzzahlig)."""
    if not active():
        return value
    return int(round(value / _FACTOR[0]))


def logical_rect(rect):
    """(links, oben, breite, höhe) von physisch nach logisch."""
    if not active() or rect is None:
        return rect
    return tuple(logical(v) for v in rect)


# --- Einschalten -------------------------------------------------------------

def _make_aware():
    """Den Prozess DPI-bewusst machen und die System-Skalierung liefern."""
    import ctypes
    user32 = ctypes.windll.user32
    done = False
    try:
        fn = user32.SetProcessDpiAwarenessContext
        fn.restype = ctypes.c_int
        fn.argtypes = [ctypes.c_void_p]
        done = bool(fn(ctypes.c_void_p(_SYSTEM_AWARE_CONTEXT)))
    except Exception:
        done = False
    if not done:
        try:
            done = ctypes.windll.shcore.SetProcessDpiAwareness(
                _PROCESS_SYSTEM_DPI_AWARE) == 0
        except Exception:
            done = False
    if not done:
        try:
            user32.SetProcessDPIAware()
        except Exception:
            pass
    dpi = 0
    try:
        dpi = int(user32.GetDpiForSystem())
    except Exception:
        dpi = 0
    if dpi <= 0:
        try:
            hdc = user32.GetDC(0)
            dpi = int(ctypes.windll.gdi32.GetDeviceCaps(hdc, _LOGPIXELSX))
            user32.ReleaseDC(0, hdc)
        except Exception:
            dpi = 96
    return max(1.0, dpi / 96.0)


def _forced_factor():
    """Faktor aus `VERSEKIT_DPI_FAKTOR` (Prüfen ohne Umstellen der
    Windows-Skalierung; ohne DPI-Kennung, auf jedem System) — oder None."""
    raw = os.environ.get(FORCE_VARIABLE, '').strip().replace(',', '.')
    try:
        value = float(raw)
    except ValueError:
        return None
    return value if 0.5 <= value <= 4.0 else None


def install():
    """Vor dem ersten Tk-Fenster aufrufen. Gibt True zurück, wenn umgerechnet wird."""
    if _INSTALLED[0]:
        return active()
    _INSTALLED[0] = True
    forced = _forced_factor()
    if forced:
        scale = forced
    elif sys.platform != 'win32' or not wanted():
        return False
    else:
        try:
            scale = _make_aware()
        except Exception:
            return False
    if abs(scale - 1.0) < 0.01:
        return False
    _FACTOR[0] = scale
    _patch_tkinter()
    try:
        from . import screen_grab
        screen_grab._scale_cache[:] = [scale]
    except Exception:
        pass
    return True


# --- Umrechnen ---------------------------------------------------------------

_NUMBER = re.compile(r'^-?\d+(\.\d+)?$')


def _up(value):
    """Logisch → physisch für Zahl, Zahlentext, Tupel oder Liste."""
    f = _FACTOR[0]
    if isinstance(value, bool) or value is None:
        return value
    if isinstance(value, int):
        return int(round(value * f))
    if isinstance(value, float):
        return value * f
    if isinstance(value, str):
        parts = value.split()
        if parts and all(_NUMBER.match(p) for p in parts):
            return ' '.join(str(_up(int(p) if '.' not in p else float(p)))
                            for p in parts)
        return value
    if isinstance(value, (tuple, list)):
        return type(value)(_up(v) for v in value)
    return value


def _up_coord(value):
    """Leinwand-Koordinate logisch → physisch, ohne zu runden."""
    if isinstance(value, bool):
        return value
    if isinstance(value, (int, float)):
        return float(value) * _FACTOR[0]
    if isinstance(value, str) and _NUMBER.match(value.strip()):
        return float(value) * _FACTOR[0]
    return value


def _down(value):
    """Physisch → logisch für Zahl, Zahlentext, Tupel oder Liste."""
    f = _FACTOR[0]
    if isinstance(value, bool) or value is None:
        return value
    if isinstance(value, int):
        return int(round(value / f))
    if isinstance(value, float):
        return value / f
    if isinstance(value, (tuple, list)):
        return type(value)(_down(v) for v in value)
    text = value if isinstance(value, str) else None
    if text is None:
        try:
            text = str(value)
        except Exception:
            return value
    parts = text.split()
    if parts and all(_NUMBER.match(p) for p in parts):
        nums = [_down(int(p) if '.' not in p else float(p)) for p in parts]
        if len(nums) == 1:
            return nums[0]
        return ' '.join(str(n) for n in nums)
    return value


# Widget-Optionen, die immer Bildpunkte sind.
_PIXEL_OPTIONS = frozenset((
    'padx', 'pady', 'bd', 'borderwidth', 'highlightthickness', 'wraplength',
    'insertwidth', 'insertborderwidth', 'selectborderwidth',
    'activeborderwidth', 'spacing1', 'spacing2', 'spacing3', 'sashwidth',
    'sashpad', 'sliderlength', 'length', 'xscrollincrement',
    'yscrollincrement', 'scrollregion'))
# Widgets, bei denen `width`/`height` Bildpunkte sind.
_SIZED_WIDGETS = frozenset(('frame', 'toplevel', 'canvas', 'labelframe',
                            'panedwindow', 'scrollbar', 'scale', 'message'))
# Widgets, bei denen `width`/`height` nur mit Bild Bildpunkte sind.
_IMAGE_WIDGETS = frozenset(('label', 'button', 'checkbutton', 'radiobutton',
                            'menubutton'))
# Optionen von Leinwand-Elementen.
_ITEM_OPTIONS = frozenset(('width', 'activewidth', 'disabledwidth', 'height',
                           'arrowshape'))
# Optionen von Text-Markierungen und eingebetteten Fenstern/Bildern.
_TAG_OPTIONS = frozenset(('lmargin1', 'lmargin2', 'rmargin', 'spacing1',
                          'spacing2', 'spacing3', 'offset', 'borderwidth',
                          'padx', 'pady'))
_PACK_OPTIONS = frozenset(('padx', 'pady', 'ipadx', 'ipady'))
_PLACE_OPTIONS = frozenset(('x', 'y', 'width', 'height'))
_GRID_ROWCOL_OPTIONS = frozenset(('minsize', 'pad'))


def _kind(widget):
    import tkinter as tk
    name = getattr(widget, 'widgetName', None)
    if name is None and isinstance(widget, tk.Tk):
        return 'toplevel'
    return name or ''


def _shows_image(widget, options, cget):
    if options.get('image') or options.get('bitmap'):
        return True
    if widget is None:
        return False
    try:
        return bool(str(cget(widget, 'image')) or str(cget(widget, 'bitmap')))
    except Exception:
        return False


def _is_pixel_option(kind, key, widget, options, cget):
    if key in _PIXEL_OPTIONS:
        return True
    if key in ('width', 'height'):
        if kind in _SIZED_WIDGETS:
            return True
        if kind in _IMAGE_WIDGETS:
            return _shows_image(widget, options, cget)
    return False


def _scale_dict(options, keys, convert):
    out = {}
    for key, value in options.items():
        if isinstance(key, str) and key.rstrip('_') in keys \
                and not callable(value):
            value = convert(value)
        out[key] = value
    return out


def _widget_options(kind, options, widget, cget):
    out = {}
    for key, value in options.items():
        if isinstance(key, str) and not callable(value) \
                and _is_pixel_option(kind, key.rstrip('_'), widget, options,
                                     cget):
            value = _up(value)
        out[key] = value
    return out


_GEOMETRY = re.compile(r'^(=?)(?:(\d+)x(\d+))?(?:([+-])(-?\d+)([+-])(-?\d+))?$')


def _geometry(text, convert):
    match = _GEOMETRY.match(str(text).strip())
    if not match:
        return text
    eq, width, height, sx, x, sy, y = match.groups()
    out = eq
    if width is not None:
        out += '%dx%d' % (convert(int(width)), convert(int(height)))
    if x is not None:
        out += '%s%d%s%d' % (sx, convert(int(x)), sy, convert(int(y)))
    return out


def _patch_tkinter():
    import tkinter as tk
    import tkinter.font as tkfont

    misc_cget = tk.Misc.cget

    # Jede Tk-Wurzel: Schriften in Punkt an die Skalierung binden.
    tk_init = tk.Tk.__init__

    def tk_root_init(self, *args, **kw):
        tk_init(self, *args, **kw)
        self.tk.call('tk', 'scaling', _FACTOR[0] * 96.0 / 72.0)

    tk.Tk.__init__ = tk_root_init

    # Widgets anlegen.
    base_init = tk.BaseWidget.__init__

    def base_widget_init(self, master, widgetName, cnf={}, kw={}, extra=()):
        options = tk._cnfmerge((cnf, kw)) if kw else dict(cnf or {})
        options = _widget_options(widgetName, options, None, misc_cget)
        base_init(self, master, widgetName, options, {}, extra)

    tk.BaseWidget.__init__ = base_widget_init

    # configure / itemconfigure / tag_configure / window_configure.
    misc_configure = tk.Misc._configure

    def dpi_configure(self, cmd, cnf, kw):
        if kw:
            cnf = tk._cnfmerge((cnf, kw))
        elif isinstance(cnf, (tuple, list)) and cnf:
            cnf = tk._cnfmerge(cnf)
        if isinstance(cnf, dict) and cnf:
            if cmd == 'configure' or cmd == 'config':
                cnf = _widget_options(_kind(self), cnf, self, misc_cget)
            elif isinstance(cmd, tuple) and cmd and cmd[0] == 'itemconfigure':
                cnf = _scale_dict(cnf, _ITEM_OPTIONS, _up)
            elif isinstance(cmd, tuple) and len(cmd) > 1 \
                    and cmd[0] in ('tag', 'window', 'image') \
                    and cmd[1] == 'configure':
                cnf = _scale_dict(cnf, _TAG_OPTIONS, _up)
        return misc_configure(self, cmd, cnf, None)

    tk.Misc._configure = dpi_configure

    def dpi_cget(self, key):
        value = misc_cget(self, key)
        name = str(key).lstrip('-')
        if _is_pixel_option(_kind(self), name, self, {}, misc_cget):
            return _down(value)
        return value

    tk.Misc.cget = dpi_cget
    tk.Misc.__getitem__ = dpi_cget

    # Pack / Grid / Place.
    tk.Pack.pack_configure = tk.Pack.pack = _layout_wrapper(
        tk, tk.Pack.pack_configure, _PACK_OPTIONS)
    tk.Grid.grid_configure = tk.Grid.grid = _layout_wrapper(
        tk, tk.Grid.grid_configure, _PACK_OPTIONS)
    tk.Place.place_configure = tk.Place.place = _layout_wrapper(
        tk, tk.Place.place_configure, _PLACE_OPTIONS)
    tk.Pack.pack_info = tk.Pack.info = _info_wrapper(
        tk.Pack.pack_info, _PACK_OPTIONS)
    tk.Grid.grid_info = _info_wrapper(tk.Grid.grid_info, _PACK_OPTIONS)
    tk.Place.place_info = _info_wrapper(tk.Place.place_info, _PLACE_OPTIONS)

    grid_rowcol = tk.Misc._grid_configure

    def dpi_grid_rowcol(self, command, index, cnf, kw):
        if kw:
            cnf = tk._cnfmerge((cnf, kw))
        if isinstance(cnf, dict) and cnf:
            cnf = _scale_dict(cnf, _GRID_ROWCOL_OPTIONS, _up)
        return grid_rowcol(self, command, index, cnf, {})

    tk.Misc._grid_configure = dpi_grid_rowcol

    # Fenster.
    wm_geometry = tk.Wm.wm_geometry

    def dpi_geometry(self, newGeometry=None):
        if newGeometry is None:
            return _geometry(wm_geometry(self), lambda v: int(round(_down(v))))
        return wm_geometry(self, _geometry(newGeometry, _up))

    tk.Wm.wm_geometry = tk.Wm.geometry = dpi_geometry
    tk.Wm.wm_minsize = tk.Wm.minsize = _wm_size_wrapper(tk.Wm.wm_minsize)
    tk.Wm.wm_maxsize = tk.Wm.maxsize = _wm_size_wrapper(tk.Wm.wm_maxsize)

    # Abfragen.
    tk.Misc.winfo_width = _logical_result(tk.Misc.winfo_width)
    tk.Misc.winfo_height = _logical_result(tk.Misc.winfo_height)
    tk.Misc.winfo_reqwidth = _logical_result(tk.Misc.winfo_reqwidth)
    tk.Misc.winfo_reqheight = _logical_result(tk.Misc.winfo_reqheight)
    tk.Misc.winfo_x = _logical_result(tk.Misc.winfo_x)
    tk.Misc.winfo_y = _logical_result(tk.Misc.winfo_y)
    tk.Misc.winfo_rootx = _logical_result(tk.Misc.winfo_rootx)
    tk.Misc.winfo_rooty = _logical_result(tk.Misc.winfo_rooty)
    tk.Misc.winfo_pointerx = _logical_result(tk.Misc.winfo_pointerx)
    tk.Misc.winfo_pointery = _logical_result(tk.Misc.winfo_pointery)
    tk.Misc.winfo_pointerxy = _logical_result(tk.Misc.winfo_pointerxy)
    tk.Misc.winfo_screenwidth = _logical_result(tk.Misc.winfo_screenwidth)
    tk.Misc.winfo_screenheight = _logical_result(tk.Misc.winfo_screenheight)

    winfo_geometry = tk.Misc.winfo_geometry
    tk.Misc.winfo_geometry = lambda self: _geometry(
        winfo_geometry(self), lambda v: int(round(_down(v))))

    winfo_containing = tk.Misc.winfo_containing

    def dpi_containing(self, rootX, rootY, displayof=0):
        return winfo_containing(self, _up(rootX), _up(rootY), displayof)

    tk.Misc.winfo_containing = dpi_containing
    tk.Misc.winfo_pixels = _pixels_wrapper(tk.Misc.winfo_pixels)
    tk.Misc.winfo_fpixels = _pixels_wrapper(tk.Misc.winfo_fpixels)

    # Ereignisse.
    substitute = tk.Misc._substitute

    def substitute_scaled(self, *args):
        result = substitute(self, *args)
        if len(result) == 1:
            event = result[0]
            for attr in ('x', 'y', 'x_root', 'y_root', 'width', 'height'):
                value = getattr(event, attr, None)
                if isinstance(value, int) and not isinstance(value, bool):
                    setattr(event, attr, _down(value))
        return result

    tk.Misc._substitute = substitute_scaled

    # Leinwand.
    canvas_create = tk.Canvas._create

    def dpi_canvas_create(self, itemType, args, kw):
        flat = tk._flatten(args)
        cnf = {}
        if flat and isinstance(flat[-1], (dict, tuple)):
            cnf = flat[-1]
            flat = flat[:-1]
        options = tk._cnfmerge((cnf, kw)) if kw else dict(cnf or {})
        options = _scale_dict(options, _ITEM_OPTIONS, _up)
        return canvas_create(self, itemType,
                             tuple(_up_coord(v) for v in flat) + (options,),
                             {})

    tk.Canvas._create = dpi_canvas_create

    canvas_coords = tk.Canvas.coords

    def dpi_canvas_coords(self, *args):
        flat = tk._flatten(args)
        if len(flat) > 1:
            flat = (flat[0],) + tuple(_up_coord(v) for v in flat[1:])
        result = canvas_coords(self, *flat)
        return [_down(v) for v in result] if isinstance(result, list) \
            else result

    tk.Canvas.coords = dpi_canvas_coords

    canvas_bbox = tk.Canvas.bbox
    tk.Canvas.bbox = lambda self, *args: _down(canvas_bbox(self, *args))

    canvas_move = tk.Canvas.move

    def dpi_canvas_move(self, *args):
        if len(args) == 3:
            args = (args[0], _up_coord(args[1]), _up_coord(args[2]))
        return canvas_move(self, *args)

    tk.Canvas.move = dpi_canvas_move

    canvas_moveto = tk.Canvas.moveto

    def dpi_canvas_moveto(self, tagOrId, x='', y=''):
        return canvas_moveto(self, tagOrId,
                             _up_coord(x) if x != '' else x,
                             _up_coord(y) if y != '' else y)

    tk.Canvas.moveto = dpi_canvas_moveto

    canvas_scale = tk.Canvas.scale

    def dpi_canvas_scale(self, *args):
        if len(args) >= 3:
            args = (args[0], _up_coord(args[1]), _up_coord(args[2])) \
                + tuple(args[3:])
        return canvas_scale(self, *args)

    tk.Canvas.scale = dpi_canvas_scale
    tk.Canvas.canvasx = _canvas_xy_wrapper(tk.Canvas.canvasx)
    tk.Canvas.canvasy = _canvas_xy_wrapper(tk.Canvas.canvasy)
    tk.Canvas.find_overlapping = _find_area_wrapper(tk.Canvas.find_overlapping)
    tk.Canvas.find_enclosed = _find_area_wrapper(tk.Canvas.find_enclosed)

    find_closest = tk.Canvas.find_closest

    def dpi_find_closest(self, x, y, halo=None, start=None):
        return find_closest(self, _up(x), _up(y), _up(halo), start)

    tk.Canvas.find_closest = dpi_find_closest

    # Text.
    text_bbox = tk.Text.bbox
    tk.Text.bbox = lambda self, index: _down(text_bbox(self, index))
    text_dlineinfo = tk.Text.dlineinfo
    tk.Text.dlineinfo = lambda self, index: _down(text_dlineinfo(self, index))
    tk.Text.window_create = _embed_wrapper(tk, tk.Text.window_create)
    tk.Text.image_create = _embed_wrapper(tk, tk.Text.image_create)

    # Schriftmaße.
    font_measure = tkfont.Font.measure
    tkfont.Font.measure = lambda self, text, displayof=None: _down(
        font_measure(self, text, displayof))

    font_metrics = tkfont.Font.metrics

    def dpi_font_metrics(self, *options, **kw):
        result = font_metrics(self, *options, **kw)
        if isinstance(result, dict):
            return {k: (_down(v) if k in ('ascent', 'descent', 'linespace')
                        else v) for k, v in result.items()}
        if options and str(options[0]).lstrip('-') in ('ascent', 'descent',
                                                       'linespace'):
            return _down(result)
        return result

    tkfont.Font.metrics = dpi_font_metrics


# --- Hüllen für `_patch_tkinter` ------------------------------------------------

def _layout_wrapper(tk, original, keys):
    """pack/grid/place: Abstände bzw. Lage hinein umrechnen."""
    def dpi_layout(self, cnf={}, **kw):
        options = tk._cnfmerge((cnf, kw)) if kw else dict(cnf or {})
        return original(self, _scale_dict(options, keys, _up))
    return dpi_layout


def _info_wrapper(original, keys):
    """pack_info/grid_info/place_info: Abstände bzw. Lage heraus umrechnen."""
    def dpi_layout_info(self):
        result = original(self)
        if isinstance(result, dict):
            return _scale_dict(result, keys, _down)
        return result
    return dpi_layout_info


def _wm_size_wrapper(original):
    """minsize/maxsize in beide Richtungen umrechnen."""
    def dpi_wm_size(self, width=None, height=None):
        if width is None and height is None:
            return _down(original(self))
        return original(self, _up(width), _up(height))
    return dpi_wm_size


def _logical_result(original):
    """Eine `winfo_*`-Abfrage, deren Ergebnis heraus umgerechnet wird."""
    def dpi_winfo(self, *args, **kw):
        return _down(original(self, *args, **kw))
    return dpi_winfo


def _pixels_wrapper(original):
    """winfo_pixels/fpixels: reine Zahlen sind schon logisch, Maße mit
    Einheit (`1i`, `2c`) rechnet Tk physisch und werden heraus umgerechnet."""
    def dpi_winfo_pixels(self, number):
        if isinstance(number, (int, float)) or (
                isinstance(number, str) and _NUMBER.match(number.strip())):
            return original(self, number)
        return _down(original(self, number))
    return dpi_winfo_pixels


def _canvas_xy_wrapper(original):
    """canvasx/canvasy in beide Richtungen umrechnen."""
    def dpi_canvas_xy(self, screen, gridspacing=None):
        return _down(original(self, _up(screen), _up(gridspacing)))
    return dpi_canvas_xy


def _find_area_wrapper(original):
    """find_overlapping/find_enclosed: Rechteck hinein umrechnen."""
    def dpi_find_area(self, x1, y1, x2, y2):
        return original(self, _up(x1), _up(y1), _up(x2), _up(y2))
    return dpi_find_area


def _embed_wrapper(tk, original):
    """Text.window_create/image_create: Abstände hinein umrechnen."""
    def dpi_text_embed(self, index, cnf={}, **kw):
        options = tk._cnfmerge((cnf, kw)) if kw else dict(cnf or {})
        return original(self, index, _scale_dict(options, _TAG_OPTIONS, _up))
    return dpi_text_embed
