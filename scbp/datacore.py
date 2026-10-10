# -*- coding: utf-8 -*-
#
# SC BP Watcher — zeigt live neue Star-Citizen-Bauplaene an.
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
Leser für die Spieldatenbank von Star Citizen (DataCore, `Data/Game2.dcb`
in der `Data.p4k`) — nur Standardbibliothek.

⭐⭐ **Der Aufbau wird ERKANNT, nicht angenommen.** Das Format ändert sich
ohne Ankündigung. Durchprobiert werden:

  * Kopfgröße (28–33 Felder, also 1–6 Textlängen am Ende)
  * Datensatzgröße (32, 36, 40, 44 Byte)
  * Reihenfolge der Wert-Tabellen (zwei bekannte)

Gültig ist nur eine Variante, bei der **die berechnete Länge aufs Byte der
Dateilänge entspricht**, die Wahrheitswerte nur 0 und 1 sind und die Namen
lesbar sind. Passt keine, wirft der Leser `UnknownFormat` — lieber keine
Aussage als eine falsche.
"""
import struct
import uuid

ARCHIVE_PATH = 'Data/Game2.dcb'


class UnknownFormat(Exception):
    pass


# Name, Bytes je Eintrag — Reihenfolge in der DATEI
ORDER_A = ['int8', 'int16', 'int32', 'int64', 'uint8', 'uint16', 'uint32', 'uint64',
           'bool', 'float', 'double', 'guid', 'string', 'locale', 'enum', 'strong',
           'weak', 'ref', 'enumopt']
ORDER_B = ['bool', 'int8', 'int16', 'int32', 'int64', 'uint8', 'uint16', 'uint32',
           'uint64', 'float', 'double', 'guid', 'string', 'locale', 'enum', 'strong',
           'weak', 'ref', 'enumopt']
SIZE = {'int8': 1, 'int16': 2, 'int32': 4, 'int64': 8, 'uint8': 1, 'uint16': 2,
        'uint32': 4, 'uint64': 8, 'bool': 1, 'float': 4, 'double': 8, 'guid': 16,
        'string': 4, 'locale': 4, 'enum': 4, 'strong': 8, 'weak': 8, 'ref': 20,
        'enumopt': 4}
# Reihenfolge der ZÄHLER im Kopf
COUNTS = ['bool', 'int8', 'int16', 'int32', 'int64', 'uint8', 'uint16', 'uint32',
          'uint64', 'float', 'double', 'guid', 'string', 'locale', 'enum', 'strong',
          'weak', 'ref', 'enumopt']
INLINE = {0x1: 1, 0x2: 1, 0x3: 2, 0x4: 4, 0x5: 8, 0x6: 1, 0x7: 2, 0x8: 4, 0x9: 8,
          0xA: 4, 0xB: 4, 0xC: 8, 0xD: 4, 0xE: 16, 0xF: 4, 0x110: 8, 0x210: 8,
          0x310: 20}
ARRAY = {0x1: 'bool', 0x2: 'int8', 0x3: 'int16', 0x4: 'int32', 0x5: 'int64',
         0x6: 'uint8', 0x7: 'uint16', 0x8: 'uint32', 0x9: 'uint64', 0xA: 'string',
         0xB: 'float', 0xC: 'double', 0xD: 'locale', 0xE: 'guid', 0xF: 'enum',
         0x110: 'strong', 0x210: 'weak', 0x310: 'ref'}
NONE = 0xFFFFFFFF


def guid_str(raw):
    return str(uuid.UUID(bytes_le=bytes(raw)))


class DataCore:
    """Die entpackte `Game2.dcb`. `records()` zählt die Datensätze auf,
    `read()` liest einen davon als verschachteltes dict."""

    def __init__(self, data):
        self.d = data
        tried = []
        for hdr in (30, 29, 31, 32, 33, 28):
            try:
                if not self._defs(hdr):
                    continue
            except Exception as exc:
                tried.append('header %d: %r' % (hdr, exc))
                continue
            for rec in (36, 32, 40, 44):
                if self._fits(rec):
                    order = self._order()
                    if order and self._readable():
                        self.layout = {'kopf_felder': hdr, 'datensatz_bytes': rec,
                                       'reihenfolge': order}
                        return
            tried.append('header %d: no record size fits' % hdr)
        raise UnknownFormat('no known layout fits (version %d): %s'
                            % (struct.unpack_from('<I', data, 4)[0],
                               '; '.join(tried)))

    # ------------------------------------------------------------ Aufbau
    def _defs(self, hdr):
        """Kopf und Definitionstabellen lesen. Hängt nur an der Kopfgröße."""
        d = self.d
        h = struct.unpack_from('<%dI' % hdr, d, 0)
        self.version = h[1]
        ns, npd, ne, nm, nr = h[4:9]
        if not (0 < ns < 10 ** 6 and 0 < npd < 10 ** 7 and 0 < nr < 10 ** 7
                and 0 < nm <= ns):
            return False
        self.cnt = dict(zip(COUNTS, h[9:28]))
        self.texts = list(h[28:hdr])
        if not self.texts:
            return False
        o = 4 * hdr
        self.structs = [struct.unpack_from('<IIHHI', d, o + 16 * i)
                        for i in range(ns)]
        o += 16 * ns
        self.props = [struct.unpack_from('<IHHHH', d, o + 12 * i)
                      for i in range(npd)]
        o += 12 * npd
        o += 8 * ne                                   # Aufzählungen: nicht gebraucht
        self.maps = [struct.unpack_from('<II', d, o + 8 * i) for i in range(nm)]
        o += 8 * nm
        self.rec_off, self.nrec = o, nr
        for s in self.structs:
            if s[1] != NONE and s[1] >= ns:
                return False
        for p in self.props:
            if p[2] != 0x10 and p[3] == 0 and p[2] not in INLINE:
                return False
        self._ssize = {}
        self._props_cache = {}
        self.data_size = 0
        for cnt, si in self.maps:
            if si >= ns:
                return False
            self.data_size += cnt * self.structsize(si)
        return True

    def _fits(self, recsize):
        """Geht die Rechnung mit dieser Datensatzgröße aufs Byte auf?"""
        o = self.rec_off + recsize * self.nrec
        arrays = sum(SIZE[n] * self.cnt[n] for n in COUNTS)
        end = o + arrays + sum(self.texts) + self.data_size
        if end != len(self.d):
            return False
        self.recsize = recsize
        self.values_off = o
        return True

    def _order(self):
        """Reihenfolge der Wert-Tabellen — erkannt an den Wahrheitswerten:
        in der richtigen Lage stehen dort nur Nullen und Einsen."""
        for name, order in (('A', ORDER_A), ('B', ORDER_B)):
            o = self.values_off
            arr = {}
            for n in order:
                arr[n] = (o, SIZE[n])
                o += SIZE[n] * self.cnt[n]
            bo = arr['bool'][0]
            sample = self.d[bo:bo + min(self.cnt['bool'], 200000)]
            if bytes(sample).strip(b'\x00\x01') == b'':
                self.arr = arr
                self.text = []
                for tl in self.texts:
                    self.text.append(o)
                    o += tl
                self.t1 = self.text[0]
                self.t2 = self.text[1] if len(self.text) > 1 else self.text[0]
                self.inst = {}
                for cnt, si in self.maps:
                    self.inst[si] = (o, cnt)
                    o += cnt * self.structsize(si)
                return name
        return None

    def _readable(self):
        """Die ersten Strukturnamen müssen reiner Text sein."""
        for i in range(min(20, len(self.structs))):
            try:
                n = self.n2(self.structs[i][0])
            except ValueError:
                return False
            if not n or not all(32 <= ord(c) < 127 for c in n):
                return False
        return True

    # ------------------------------------------------------------ Texte
    def _cstr(self, base, off):
        e = self.d.index(b'\0', base + off)
        return bytes(self.d[base + off:e]).decode('utf-8', 'replace')

    def n1(self, off):
        return self._cstr(self.t1, off)

    def n2(self, off):
        return self._cstr(self.t2, off)

    def type_name(self, si):
        return self.n2(self.structs[si][0])

    # ------------------------------------------------------------ Strukturen
    def allprops(self, si):
        if si in self._props_cache:
            return self._props_cache[si]
        chain = []
        s = si
        while s != NONE:
            chain.append(s)
            s = self.structs[s][1]
        out = []
        for c in reversed(chain):
            _, _, ac, fa, _ = self.structs[c]
            out += list(range(fa, fa + ac))
        self._props_cache[si] = out
        return out

    def structsize(self, si):
        if si in self._ssize:
            return self._ssize[si]
        tot = 0
        for p in self.allprops(si):
            _, psi, dt, ct, _ = self.props[p]
            if ct != 0:
                tot += 8
            elif dt == 0x10:
                tot += self.structsize(psi)
            else:
                tot += INLINE[dt]
        self._ssize[si] = tot
        return tot

    # ------------------------------------------------------------ Datensätze
    def records(self):
        """(name, dateiname, strukturnummer, guid, instanz) je Datensatz.

        Aufbau je nach Größe: name, datei, [etikett], struktur, guid, instanz, größe."""
        extra = (self.recsize - 32) // 4
        fmt = '<II' + 'I' * extra + 'I16sHH'
        for i in range(self.nrec):
            v = struct.unpack_from(fmt, self.d, self.rec_off + self.recsize * i)
            nm, fn, si, gid, ii = (v[0], v[1], v[2 + extra], v[3 + extra],
                                   v[4 + extra])
            yield self.n2(nm), self.n1(fn), si, guid_str(gid), ii

    # ------------------------------------------------------------ Lesen
    def read(self, si, ii, maxd=30):
        o = self.inst[si][0] + ii * self.structsize(si)
        return self._read(si, o, 0, maxd)

    def _aval(self, name, i):
        o, sz = self.arr[name]
        return self.d[o + sz * i:o + sz * (i + 1)]

    def _scalar(self, dt, b):
        if dt == 0x1:
            return b[0] != 0
        if dt in (0x2, 0x3, 0x4, 0x5):
            return int.from_bytes(b, 'little', signed=True)
        if dt in (0x6, 0x7, 0x8, 0x9):
            return int.from_bytes(b, 'little')
        if dt in (0xA, 0xD, 0xF):
            return self.n1(struct.unpack('<I', b)[0])
        if dt == 0xB:
            return struct.unpack('<f', b)[0]
        if dt == 0xC:
            return struct.unpack('<d', b)[0]
        if dt == 0xE:
            return guid_str(b)
        if dt in (0x110, 0x210):
            return ('ptr', dt) + struct.unpack('<II', b)
        if dt == 0x310:
            return 'REF:' + guid_str(b[4:20])
        return None

    def _res(self, v, depth, maxd):
        if isinstance(v, tuple):
            _, dt, si2, ii2 = v
            if si2 == NONE:
                return None
            if dt == 0x210 or depth >= maxd:   # schwache Zeiger nicht verfolgen
                return '<%s>' % self.type_name(si2)
            return self._read(si2, self.inst[si2][0] + ii2 * self.structsize(si2),
                              depth + 1, maxd)
        return v

    def _read(self, si, o, depth, maxd):
        out = {'_type': self.type_name(si)}
        for p in self.allprops(si):
            nm, psi, dt, ct, _ = self.props[p]
            name = self.n2(nm)
            if ct != 0:
                c, f = struct.unpack_from('<II', self.d, o)
                o += 8
                if dt == 0x10:
                    sz = self.structsize(psi)
                    base = self.inst[psi][0]
                    out[name] = [self._read(psi, base + (f + k) * sz, depth + 1, maxd)
                                 if depth < maxd else '...' for k in range(c)]
                else:
                    out[name] = [self._res(self._scalar(dt, self._aval(ARRAY[dt], f + k)),
                                           depth, maxd) for k in range(c)]
            elif dt == 0x10:
                out[name] = self._read(psi, o, depth + 1, maxd)
                o += self.structsize(psi)
            else:
                sz = INLINE[dt]
                out[name] = self._res(self._scalar(dt, self.d[o:o + sz]), depth, maxd)
                o += sz
        return out
