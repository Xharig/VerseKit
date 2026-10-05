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
"""Texterkennung von Windows (`Windows.Media.Ocr`) über ctypes.

Ohne Skript-Interpreter und ohne Zusatzpakete: Die WinRT-Klassen werden
über `combase.dll` aktiviert und ihre Methoden über die Funktionstabellen
der Schnittstellen gerufen. Vergrößern und Ausschneiden macht GDI
(`StretchBlt`), die Graustufe `SoftwareBitmap.Convert`.

Die Nummern hinter den Methoden sind die Plätze in der Funktionstabelle:
0–2 IUnknown, 3–5 IInspectable, ab 6 die Methoden der Schnittstelle in der
Reihenfolge ihrer Metadaten.
"""
import ctypes
import struct
import sys
import threading
import time
import uuid
import zlib
from ctypes import wintypes

HRESULT = ctypes.c_long


class GUID(ctypes.Structure):
    _fields_ = [('Data1', ctypes.c_uint32), ('Data2', ctypes.c_uint16),
                ('Data3', ctypes.c_uint16), ('Data4', ctypes.c_ubyte * 8)]


def _guid(text):
    """Eine GUID aus ihrer Schreibweise `XXXXXXXX-XXXX-…`."""
    return GUID.from_buffer_copy(uuid.UUID(text).bytes_le)


IID_OcrEngineStatics = _guid('5BFFA85A-3384-3540-9940-699120D428A8')
IID_SoftwareBitmapFactory = _guid('C99FEB69-2D62-4D47-A6B3-4FDB6A07FDF8')
IID_SoftwareBitmapStatics = _guid('DF0385DB-672F-4A9D-806E-C2442F343E86')
IID_MemoryBuffer = _guid('FBC4DD2A-245B-11E4-AF98-689423260CF8')
IID_MemoryBufferByteAccess = _guid('5B0D3235-4DBA-4D44-865E-8F1D0E4FD04D')
IID_Closable = _guid('30D5A829-7FA4-4026-83BB-D75BAE4EA99E')
IID_AsyncInfo = _guid('00000036-0000-0000-C000-000000000046')

BGRA8 = 87
GRAY8 = 62
ALPHA_IGNORE = 2
ACCESS_READ_WRITE = 1


class OcrError(Exception):
    pass


class _Rect(ctypes.Structure):
    _fields_ = [('x', ctypes.c_float), ('y', ctypes.c_float),
                ('w', ctypes.c_float), ('h', ctypes.c_float)]


class _Plane(ctypes.Structure):
    _fields_ = [('start', ctypes.c_int32), ('width', ctypes.c_int32),
                ('height', ctypes.c_int32), ('stride', ctypes.c_int32)]


def _check(hr, what):
    if hr < 0:
        raise OcrError('%s: 0x%08X' % (what, hr & 0xFFFFFFFF))


def _call(obj, index, *args, argtypes=()):
    """Methode `index` der Schnittstelle `obj` rufen; gibt das HRESULT."""
    vtable = ctypes.cast(obj, ctypes.POINTER(ctypes.POINTER(ctypes.c_void_p)))[0]
    proto = ctypes.WINFUNCTYPE(HRESULT, ctypes.c_void_p, *argtypes)
    return proto(vtable[index])(obj, *args)


def _release(obj):
    if obj:
        vtable = ctypes.cast(obj, ctypes.POINTER(ctypes.POINTER(ctypes.c_void_p)))[0]
        ctypes.WINFUNCTYPE(ctypes.c_ulong, ctypes.c_void_p)(vtable[2])(obj)


def _query(obj, iid):
    out = ctypes.c_void_p()
    _check(_call(obj, 0, ctypes.byref(iid), ctypes.byref(out),
                 argtypes=(ctypes.POINTER(GUID), ctypes.POINTER(ctypes.c_void_p))),
           'QueryInterface')
    return out


def _get_ptr(obj, index, what):
    out = ctypes.c_void_p()
    _check(_call(obj, index, ctypes.byref(out),
                 argtypes=(ctypes.POINTER(ctypes.c_void_p),)), what)
    return out


class _Runtime:
    def __init__(self):
        self.combase = ctypes.WinDLL('combase')
        self.combase.WindowsCreateString.argtypes = (
            wintypes.LPCWSTR, ctypes.c_uint32, ctypes.POINTER(ctypes.c_void_p))
        self.combase.WindowsDeleteString.argtypes = (ctypes.c_void_p,)
        self.combase.WindowsGetStringRawBuffer.argtypes = (
            ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32))
        self.combase.WindowsGetStringRawBuffer.restype = ctypes.c_void_p
        self.combase.RoGetActivationFactory.argtypes = (
            ctypes.c_void_p, ctypes.POINTER(GUID), ctypes.POINTER(ctypes.c_void_p))
        self.threads = threading.local()

    def enter(self):
        """WinRT im laufenden Faden einrichten — je Faden einmal."""
        if getattr(self.threads, 'ready', False):
            return
        hr = self.combase.RoInitialize(1)
        # S_FALSE und RPC_E_CHANGED_MODE: schon eingerichtet, reicht auch.
        if hr < 0 and (hr & 0xFFFFFFFF) != 0x80010106:
            _check(hr, 'RoInitialize')
        self.threads.ready = True

    def hstring(self, text):
        h = ctypes.c_void_p()
        _check(self.combase.WindowsCreateString(text, len(text), ctypes.byref(h)),
               'WindowsCreateString')
        return h

    def text(self, h):
        if not h:
            return ''
        n = ctypes.c_uint32()
        p = self.combase.WindowsGetStringRawBuffer(h, ctypes.byref(n))
        value = ctypes.wstring_at(p, n.value) if p else ''
        self.combase.WindowsDeleteString(h)
        return value

    def factory(self, name, iid):
        h = self.hstring(name)
        out = ctypes.c_void_p()
        try:
            _check(self.combase.RoGetActivationFactory(h, ctypes.byref(iid),
                                                       ctypes.byref(out)),
                   'Factory ' + name)
        finally:
            self.combase.WindowsDeleteString(h)
        return out


_RT = []


def _runtime():
    if not _RT:
        _RT.append(_Runtime())
    _RT[0].enter()
    return _RT[0]


# ----------------------------------------------------------- Bild lesen
def read_bmp(path):
    """(breite, höhe, BGRA-Bytes oben beginnend) aus einer 24/32-Bit-BMP."""
    with open(path, 'rb') as handle:
        data = handle.read()
    if data[:2] != b'BM':
        raise OcrError('not a BMP')
    offset = struct.unpack('<I', data[10:14])[0]
    width, height = struct.unpack('<ii', data[18:26])
    bits = struct.unpack('<H', data[28:30])[0]
    top_down = height < 0
    height = abs(height)
    if bits not in (24, 32):
        raise OcrError('BMP with %d bits' % bits)
    bpp = bits // 8
    row = (width * bpp + 3) & ~3
    out = bytearray(width * height * 4)
    for y in range(height):
        src = y if top_down else height - 1 - y
        line = data[offset + src * row: offset + src * row + width * bpp]
        start = y * width * 4
        if bpp == 4:
            out[start:start + width * 4] = line
        else:
            seg = out[start:start + width * 4]
            seg[0::4] = line[0::3]
            seg[1::4] = line[1::3]
            seg[2::4] = line[2::3]
            out[start:start + width * 4] = seg
    return width, height, out


# ------------------------------------------------------------- GDI
class _BitmapInfo(ctypes.Structure):
    _fields_ = [('biSize', ctypes.c_uint32), ('biWidth', ctypes.c_int32),
                ('biHeight', ctypes.c_int32), ('biPlanes', ctypes.c_uint16),
                ('biBitCount', ctypes.c_uint16), ('biCompression', ctypes.c_uint32),
                ('biSizeImage', ctypes.c_uint32), ('biXPelsPerMeter', ctypes.c_int32),
                ('biYPelsPerMeter', ctypes.c_int32), ('biClrUsed', ctypes.c_uint32),
                ('biClrImportant', ctypes.c_uint32)]


def _gdi():
    gdi = ctypes.WinDLL('gdi32')
    gdi.CreateCompatibleDC.argtypes = (ctypes.c_void_p,)
    gdi.CreateCompatibleDC.restype = ctypes.c_void_p
    gdi.CreateDIBSection.argtypes = (ctypes.c_void_p, ctypes.c_void_p, ctypes.c_uint,
                                     ctypes.POINTER(ctypes.c_void_p),
                                     ctypes.c_void_p, ctypes.c_uint32)
    gdi.CreateDIBSection.restype = ctypes.c_void_p
    gdi.SelectObject.argtypes = (ctypes.c_void_p, ctypes.c_void_p)
    gdi.SelectObject.restype = ctypes.c_void_p
    gdi.DeleteObject.argtypes = (ctypes.c_void_p,)
    gdi.DeleteDC.argtypes = (ctypes.c_void_p,)
    gdi.SetStretchBltMode.argtypes = (ctypes.c_void_p, ctypes.c_int)
    gdi.SetBrushOrgEx.argtypes = (ctypes.c_void_p, ctypes.c_int, ctypes.c_int,
                                  ctypes.c_void_p)
    gdi.StretchBlt.argtypes = (ctypes.c_void_p, ctypes.c_int, ctypes.c_int,
                               ctypes.c_int, ctypes.c_int, ctypes.c_void_p,
                               ctypes.c_int, ctypes.c_int, ctypes.c_int,
                               ctypes.c_int, ctypes.c_uint32)
    gdi.GdiFlush.argtypes = ()
    return gdi


def _dib(gdi, width, height):
    info = _BitmapInfo(ctypes.sizeof(_BitmapInfo), width, -height, 1, 32, 0,
                       0, 0, 0, 0, 0)
    bits = ctypes.c_void_p()
    handle = gdi.CreateDIBSection(None, ctypes.byref(info), 0, ctypes.byref(bits),
                                  None, 0)
    if not handle or not bits:
        raise OcrError('CreateDIBSection')
    return handle, bits


def scaled(src, box, scale):
    """Ausschnitt `box` von `src` (breite, höhe, BGRA) um `scale` vergrößert.

    Gibt (breite, höhe, BGRA-Bytes)."""
    width, height, raw = src
    x, y, w, h = box
    tw, th = max(1, int(w * scale)), max(1, int(h * scale))
    gdi = _gdi()
    src_dc = gdi.CreateCompatibleDC(None)
    dst_dc = gdi.CreateCompatibleDC(None)
    src_bmp, src_bits = _dib(gdi, width, height)
    dst_bmp, dst_bits = _dib(gdi, tw, th)
    try:
        ctypes.memmove(src_bits, bytes(raw), len(raw))
        old_src = gdi.SelectObject(src_dc, src_bmp)
        old_dst = gdi.SelectObject(dst_dc, dst_bmp)
        gdi.SetStretchBltMode(dst_dc, 4)        # HALFTONE
        gdi.SetBrushOrgEx(dst_dc, 0, 0, None)
        if not gdi.StretchBlt(dst_dc, 0, 0, tw, th, src_dc, x, y, w, h,
                              0x00CC0020):          # SRCCOPY
            raise OcrError('StretchBlt')
        gdi.GdiFlush()
        out = ctypes.string_at(dst_bits, tw * th * 4)
        gdi.SelectObject(src_dc, old_src)
        gdi.SelectObject(dst_dc, old_dst)
    finally:
        gdi.DeleteObject(src_bmp)
        gdi.DeleteObject(dst_bmp)
        gdi.DeleteDC(src_dc)
        gdi.DeleteDC(dst_dc)
    return tw, th, out


# --------------------------------------------------------- SoftwareBitmap
def _lock(bitmap):
    """(Puffer-Referenz, Zeiger, Länge, Ebene) eines SoftwareBitmap."""
    buffer = ctypes.c_void_p()
    _check(_call(bitmap, 15, ACCESS_READ_WRITE, ctypes.byref(buffer),
                 argtypes=(ctypes.c_int, ctypes.POINTER(ctypes.c_void_p))),
           'LockBuffer')
    plane = _Plane()
    _check(_call(buffer, 7, 0, ctypes.byref(plane),
                 argtypes=(ctypes.c_int32, ctypes.POINTER(_Plane))),
           'GetPlaneDescription')
    memory = _query(buffer, IID_MemoryBuffer)
    reference = _get_ptr(memory, 6, 'CreateReference')
    access = _query(reference, IID_MemoryBufferByteAccess)
    pointer = ctypes.c_void_p()
    size = ctypes.c_uint32()
    _check(_call(access, 3, ctypes.byref(pointer), ctypes.byref(size),
                 argtypes=(ctypes.POINTER(ctypes.c_void_p),
                           ctypes.POINTER(ctypes.c_uint32))), 'GetBuffer')
    _release(access)
    _release(memory)
    return (buffer, reference), pointer, size.value, plane


def _unlock(handles):
    for obj in handles:
        try:
            closable = _query(obj, IID_Closable)
            _call(closable, 6)
            _release(closable)
        except OcrError:
            pass
        _release(obj)


def _bitmap_bgra(width, height, raw):
    rt = _runtime()
    factory = rt.factory('Windows.Graphics.Imaging.SoftwareBitmap',
                         IID_SoftwareBitmapFactory)
    bitmap = ctypes.c_void_p()
    try:
        _check(_call(factory, 7, BGRA8, width, height, ALPHA_IGNORE,
                     ctypes.byref(bitmap),
                     argtypes=(ctypes.c_int, ctypes.c_int32, ctypes.c_int32,
                               ctypes.c_int, ctypes.POINTER(ctypes.c_void_p))),
               'SoftwareBitmap.CreateWithAlpha')
    finally:
        _release(factory)
    handles, pointer, size, plane = _lock(bitmap)
    try:
        row = width * 4
        if plane.stride == row:
            ctypes.memmove(pointer.value + plane.start, bytes(raw), len(raw))
        else:
            for y in range(height):
                ctypes.memmove(pointer.value + plane.start + y * plane.stride,
                               bytes(raw[y * row:(y + 1) * row]), row)
    finally:
        _unlock(handles)
    return bitmap


def _to_gray(bitmap, table):
    """Graustufe aus einem BGRA-Bitmap, jedes Byte über `table` umgesetzt."""
    rt = _runtime()
    statics = rt.factory('Windows.Graphics.Imaging.SoftwareBitmap',
                         IID_SoftwareBitmapStatics)
    gray = ctypes.c_void_p()
    try:
        _check(_call(statics, 7, bitmap, GRAY8, ctypes.byref(gray),
                     argtypes=(ctypes.c_void_p, ctypes.c_int,
                               ctypes.POINTER(ctypes.c_void_p))),
               'SoftwareBitmap.Convert')
    finally:
        _release(statics)
    handles, pointer, size, plane = _lock(gray)
    try:
        data = ctypes.string_at(pointer.value, size)
        ctypes.memmove(pointer.value, data.translate(table), size)
    finally:
        _unlock(handles)
    return gray


# Graustufe mit gespreiztem Kontrast (helle Schrift bleibt hell) und
# umgekehrte Graustufe (dunkle Schrift auf hellem Grund).
_GRAY = bytes(max(0, min(255, int(round(v * 1.8 - 0.25 * 255)))) for v in range(256))
_DARK = bytes(max(0, min(255, int(round((255 - v) / 0.7)))) for v in range(256))


# ------------------------------------------------------------------ OCR
def _wait(operation, timeout):
    info = _query(operation, IID_AsyncInfo)
    try:
        end = time.monotonic() + timeout
        status = ctypes.c_int()
        while True:
            _check(_call(info, 7, ctypes.byref(status),
                         argtypes=(ctypes.POINTER(ctypes.c_int),)), 'Status')
            if status.value == 1:
                return
            if status.value in (2, 3):
                raise OcrError('recognition stopped (%d)' % status.value)
            if time.monotonic() > end:
                raise OcrError('timeout')
            time.sleep(0.01)
    finally:
        _release(info)


def _engine():
    rt = _runtime()
    statics = rt.factory('Windows.Media.Ocr.OcrEngine', IID_OcrEngineStatics)
    try:
        limit = ctypes.c_uint32()
        _check(_call(statics, 6, ctypes.byref(limit),
                     argtypes=(ctypes.POINTER(ctypes.c_uint32),)),
               'MaxImageDimension')
        engine = _get_ptr(statics, 10, 'TryCreateFromUserProfileLanguages')
        if not engine:
            languages = _get_ptr(statics, 7, 'AvailableRecognizerLanguages')
            try:
                count = ctypes.c_uint32()
                _check(_call(languages, 7, ctypes.byref(count),
                             argtypes=(ctypes.POINTER(ctypes.c_uint32),)), 'Size')
                if count.value:
                    first = ctypes.c_void_p()
                    _check(_call(languages, 6, 0, ctypes.byref(first),
                                 argtypes=(ctypes.c_uint32,
                                           ctypes.POINTER(ctypes.c_void_p))),
                           'GetAt')
                    engine = ctypes.c_void_p()
                    _check(_call(statics, 9, first, ctypes.byref(engine),
                                 argtypes=(ctypes.c_void_p,
                                           ctypes.POINTER(ctypes.c_void_p))),
                           'TryCreateFromLanguage')
                    _release(first)
            finally:
                _release(languages)
        return engine, limit.value
    finally:
        _release(statics)


def _language(engine):
    rt = _runtime()
    language = _get_ptr(engine, 7, 'RecognizerLanguage')
    try:
        tag = ctypes.c_void_p()
        _check(_call(language, 6, ctypes.byref(tag),
                     argtypes=(ctypes.POINTER(ctypes.c_void_p),)), 'LanguageTag')
        return rt.text(tag)
    finally:
        _release(language)


def _vector(view):
    count = ctypes.c_uint32()
    _check(_call(view, 7, ctypes.byref(count),
                 argtypes=(ctypes.POINTER(ctypes.c_uint32),)), 'Size')
    items = []
    for i in range(count.value):
        item = ctypes.c_void_p()
        _check(_call(view, 6, i, ctypes.byref(item),
                     argtypes=(ctypes.c_uint32, ctypes.POINTER(ctypes.c_void_p))),
               'GetAt')
        items.append(item)
    return items


def recognize(engine, bitmap, timeout=60):
    """Wörter `[(text, x, y, w, h), …]` eines SoftwareBitmap."""
    rt = _runtime()
    operation = ctypes.c_void_p()
    _check(_call(engine, 6, bitmap, ctypes.byref(operation),
                 argtypes=(ctypes.c_void_p, ctypes.POINTER(ctypes.c_void_p))),
           'RecognizeAsync')
    try:
        _wait(operation, timeout)
        result = _get_ptr(operation, 8, 'GetResults')
    finally:
        _release(operation)
    words = []
    try:
        lines_view = _get_ptr(result, 6, 'Lines')
        try:
            for line in _vector(lines_view):
                words_view = _get_ptr(line, 6, 'Words')
                try:
                    for word in _vector(words_view):
                        rect = _Rect()
                        _check(_call(word, 6, ctypes.byref(rect),
                                     argtypes=(ctypes.POINTER(_Rect),)),
                               'BoundingRect')
                        text = ctypes.c_void_p()
                        _check(_call(word, 7, ctypes.byref(text),
                                     argtypes=(ctypes.POINTER(ctypes.c_void_p),)),
                               'Text')
                        words.append((rt.text(text), rect.x, rect.y, rect.w, rect.h))
                        _release(word)
                finally:
                    _release(words_view)
                _release(line)
        finally:
            _release(lines_view)
    finally:
        _release(result)
    return words


def ocr_image(path, jobs, save=None, save_crop=None, keep_width=None,
              timeout=180):
    """Ein BMP lesen: je Auftrag ((x, y, b, h), vergrößerung, art) die
    Wörter mit Rahmen in Bildpunkten des Originals.

    Art `plain` (unverändert), `gray` (Graustufe, Kontrast gespreizt) oder
    `dark` (Graustufe umgekehrt). `save` legt das ganze Bild als PNG ab
    (höchstens `keep_width` breit), `save_crop` = ((x, y, b, h), pfad) einen
    Ausschnitt in voller Auflösung.

    Gibt `{'width', 'height', 'lang', 'passes': [{'box', 'scale', 'mode',
    'words'}, …]}`.
    """
    src = read_bmp(path)
    width, height, _raw = src
    if save:
        save_png(src, (0, 0, width, height), save, keep_width)
    if save_crop:
        (cx, cy, cw, ch), crop_path = save_crop
        cx, cy = max(0, int(cx)), max(0, int(cy))
        save_png(src, (cx, cy, min(int(cw), width - cx),
                       min(int(ch), height - cy)), crop_path)
    engine, limit = _engine()
    if not engine:
        raise OcrError('keine_sprache')
    try:
        passes = []
        for box, scale, mode in jobs:
            x, y, w, h = (int(v) for v in box)
            x, y = max(0, x), max(0, y)
            w, h = min(w, width - x), min(h, height - y)
            if w <= 0 or h <= 0:
                continue
            scale = float(scale) if scale > 0 else 1.0
            if max(w, h) * scale > limit:
                scale = limit / float(max(w, h))
            tw, th, raw = scaled(src, (x, y, w, h), scale)
            bitmap = _bitmap_bgra(tw, th, raw)
            try:
                if mode in ('gray', 'dark'):
                    gray = _to_gray(bitmap, _GRAY if mode == 'gray' else _DARK)
                    _release(bitmap)
                    bitmap = gray
                found = recognize(engine, bitmap, timeout)
            finally:
                _release(bitmap)
            sx, sy = tw / float(w), th / float(h)
            passes.append({'box': [x, y, w, h], 'scale': scale, 'mode': mode,
                           'words': [{'t': t, 'x': int(x + rx / sx),
                                      'y': int(y + ry / sy),
                                      'w': int(rw / sx), 'h': int(rh / sy)}
                                     for t, rx, ry, rw, rh in found]})
        return {'width': width, 'height': height, 'lang': _language(engine),
                'passes': passes}
    finally:
        _release(engine)


# ------------------------------------------------------------ Ablage
def png_bytes(width, height, bgra):
    """BGRA-Bytes (oben beginnend) als RGB-PNG."""
    rgb = bytearray(width * height * 3)
    rgb[0::3] = bgra[2::4]
    rgb[1::3] = bgra[1::4]
    rgb[2::3] = bgra[0::4]
    row = width * 3
    raw = b''.join(b'\0' + bytes(rgb[y * row:(y + 1) * row]) for y in range(height))

    def chunk(kind, body):
        block = kind + body
        return (struct.pack('>I', len(body)) + block
                + struct.pack('>I', zlib.crc32(block) & 0xFFFFFFFF))

    return (b'\x89PNG\r\n\x1a\n'
            + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(raw, 6))
            + chunk(b'IEND', b''))


def save_png(src, box, path, max_width=None):
    """Ausschnitt `box` von `src` als PNG ablegen, höchstens `max_width` breit."""
    x, y, w, h = box
    scale = 1.0
    if max_width and w > max_width:
        scale = max_width / float(w)
    tw, th, raw = scaled(src, (x, y, w, h), scale)
    with open(path, 'wb') as handle:
        handle.write(png_bytes(tw, th, raw))


def supported():
    """Gibt es die Texterkennung von Windows auf diesem System?"""
    return sys.platform == 'win32'
