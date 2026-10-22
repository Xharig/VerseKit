# -*- coding: utf-8 -*-
#
# Copyright (C) 2026 Xharig
#
# SPDX-License-Identifier: GPL-3.0-only
"""Latin subset of a TrueType font as WOFF2, with the standard library and the brotli CLI.

Builds assets/schrift/xharig-sans-400.woff2 and xharig-sans-700.woff2 for the
web pages from Lato-Regular.ttf and Lato-Bold.ttf (SIL OFL, licence in
assets/schrift/). A subset is a modified version under the OFL, so the font is
renamed to "Xharig Sans": the Reserved Font Name "Lato" may not be used for it.
GPOS keeps only the kern feature, cut down to the kept glyphs.

Glyph outlines outside the kept set are emptied (glyph ids stay, so GPOS
kerning and GSUB stay valid); the kept set is the cmap of the wanted code
points, closed over composite glyphs and every GSUB substitution. The font is
then wrapped as WOFF2 with null transforms and one brotli stream.

    python3 tools/schrift_woff2.py Lato-Regular.ttf assets/schrift/xharig-sans-400.woff2
"""
import struct
import subprocess
import sys
import tempfile
import os

BEREICHE = [(0x20, 0x7E), (0xA0, 0xFF), (0x131, 0x131), (0x152, 0x153),
            (0x2C6, 0x2C6), (0x2DA, 0x2DA), (0x2DC, 0x2DC),
            (0x2010, 0x2014), (0x2018, 0x201E), (0x2022, 0x2022),
            (0x2026, 0x2026), (0x2039, 0x203A), (0x20AC, 0x20AC),
            (0x2122, 0x2122), (0x2190, 0x2193), (0x2212, 0x2212),
            (0xFB01, 0xFB02)]


def tabellen(d):
    n = struct.unpack('>H', d[4:6])[0]
    t = {}
    for i in range(n):
        tag, _cs, off, ln = struct.unpack('>4sIII', d[12 + 16 * i:28 + 16 * i])
        t[tag.decode('latin-1')] = d[off:off + ln]
    return t


def cmap_lesen(c):
    karte = {}
    n = struct.unpack('>H', c[2:4])[0]
    for i in range(n):
        pid, eid, off = struct.unpack('>HHI', c[4 + 8 * i:12 + 8 * i])
        if pid != 3:
            continue
        fmt = struct.unpack('>H', c[off:off + 2])[0]
        if fmt == 4:
            segx2 = struct.unpack('>H', c[off + 6:off + 8])[0]
            seg = segx2 // 2
            ende = struct.unpack('>%dH' % seg, c[off + 14:off + 14 + segx2])
            p = off + 16 + segx2
            anfang = struct.unpack('>%dH' % seg, c[p:p + segx2])
            delta = struct.unpack('>%dh' % seg, c[p + segx2:p + 2 * segx2])
            ro_pos = p + 2 * segx2
            ro = struct.unpack('>%dH' % seg, c[ro_pos:ro_pos + segx2])
            for s in range(seg):
                for cp in range(anfang[s], ende[s] + 1):
                    if cp == 0xFFFF:
                        continue
                    if ro[s] == 0:
                        g = (cp + delta[s]) & 0xFFFF
                    else:
                        a = ro_pos + 2 * s + ro[s] + 2 * (cp - anfang[s])
                        g = struct.unpack('>H', c[a:a + 2])[0]
                        if g:
                            g = (g + delta[s]) & 0xFFFF
                    if g:
                        karte.setdefault(cp, g)
        elif fmt == 12:
            ng = struct.unpack('>I', c[off + 12:off + 16])[0]
            for k in range(ng):
                a, b, g = struct.unpack('>III', c[off + 16 + 12 * k:off + 28 + 12 * k])
                for cp in range(a, b + 1):
                    karte.setdefault(cp, g + cp - a)
    return karte


def abdeckung(d, off):
    fmt = struct.unpack('>H', d[off:off + 2])[0]
    if fmt == 1:
        n = struct.unpack('>H', d[off + 2:off + 4])[0]
        return list(struct.unpack('>%dH' % n, d[off + 4:off + 4 + 2 * n]))
    n = struct.unpack('>H', d[off + 2:off + 4])[0]
    paare = []
    for k in range(n):
        a, b, idx = struct.unpack('>HHH', d[off + 4 + 6 * k:off + 10 + 6 * k])
        for g in range(a, b + 1):
            paare.append((idx + g - a, g))
    paare.sort()
    liste = [g for _i, g in paare]
    return liste


def gsub_huelle(gsub, behalten):
    if not gsub:
        return False
    lookup_list = struct.unpack('>H', gsub[8:10])[0]
    # Only features a browser turns on by itself; the page uses no others.
    fl = struct.unpack('>H', gsub[6:8])[0]
    an = set()
    for i in range(struct.unpack('>H', gsub[fl:fl + 2])[0]):
        tag = gsub[fl + 2 + 6 * i:fl + 6 + 6 * i]
        if tag in (b'ccmp', b'locl', b'liga', b'clig', b'calt', b'rlig'):
            fo = fl + struct.unpack('>H', gsub[fl + 6 + 6 * i:fl + 8 + 6 * i])[0]
            for j in range(struct.unpack('>H', gsub[fo + 2:fo + 4])[0]):
                an.add(struct.unpack('>H', gsub[fo + 4 + 2 * j:fo + 6 + 2 * j])[0])
    neu = False
    for i in sorted(an):
        lo = lookup_list + struct.unpack('>H', gsub[lookup_list + 2 + 2 * i:lookup_list + 4 + 2 * i])[0]
        typ, _flag, nsub = struct.unpack('>HHH', gsub[lo:lo + 6])
        for s in range(nsub):
            so = lo + struct.unpack('>H', gsub[lo + 6 + 2 * s:lo + 8 + 2 * s])[0]
            t = typ
            if t == 7:
                t = struct.unpack('>H', gsub[so + 2:so + 4])[0]
                so = so + struct.unpack('>I', gsub[so + 4:so + 8])[0]
            ausgabe = []
            fmt = struct.unpack('>H', gsub[so:so + 2])[0]
            if t in (1, 2, 3, 4):
                cov = abdeckung(gsub, so + struct.unpack('>H', gsub[so + 2:so + 4])[0])
            if t == 1:
                if fmt == 1:
                    delta = struct.unpack('>h', gsub[so + 4:so + 6])[0]
                    ausgabe = [((g + delta) & 0xFFFF) for g in cov if g in behalten]
                else:
                    cnt = struct.unpack('>H', gsub[so + 4:so + 6])[0]
                    ziel = struct.unpack('>%dH' % cnt, gsub[so + 6:so + 6 + 2 * cnt])
                    ausgabe = [ziel[k] for k, g in enumerate(cov) if g in behalten]
            elif t in (2, 3):
                cnt = struct.unpack('>H', gsub[so + 4:so + 6])[0]
                for k, g in enumerate(cov[:cnt]):
                    if g not in behalten:
                        continue
                    sq = so + struct.unpack('>H', gsub[so + 6 + 2 * k:so + 8 + 2 * k])[0]
                    m = struct.unpack('>H', gsub[sq:sq + 2])[0]
                    ausgabe += list(struct.unpack('>%dH' % m, gsub[sq + 2:sq + 2 + 2 * m]))
            elif t == 4:
                cnt = struct.unpack('>H', gsub[so + 4:so + 6])[0]
                for k, g in enumerate(cov[:cnt]):
                    if g not in behalten:
                        continue
                    ls = so + struct.unpack('>H', gsub[so + 6 + 2 * k:so + 8 + 2 * k])[0]
                    m = struct.unpack('>H', gsub[ls:ls + 2])[0]
                    for j in range(m):
                        lg = ls + struct.unpack('>H', gsub[ls + 2 + 2 * j:ls + 4 + 2 * j])[0]
                        lig, cc = struct.unpack('>HH', gsub[lg:lg + 4])
                        teile = struct.unpack('>%dH' % (cc - 1), gsub[lg + 4:lg + 4 + 2 * (cc - 1)])
                        if all(x in behalten for x in teile):
                            ausgabe.append(lig)
            for g in ausgabe:
                if g not in behalten:
                    behalten.add(g)
                    neu = True
    return neu


def glyphen(t):
    head = t['head']
    lang = struct.unpack('>h', head[50:52])[0] == 1
    loca = t['loca']
    n = struct.unpack('>H', t['maxp'][4:6])[0]
    if lang:
        offs = struct.unpack('>%dI' % (n + 1), loca[:4 * (n + 1)])
    else:
        offs = [o * 2 for o in struct.unpack('>%dH' % (n + 1), loca[:2 * (n + 1)])]
    return [t['glyf'][offs[i]:offs[i + 1]] for i in range(n)], lang


def bestandteile(g):
    if len(g) < 10 or struct.unpack('>h', g[0:2])[0] >= 0:
        return []
    p, teile = 10, []
    while True:
        flags, idx = struct.unpack('>HH', g[p:p + 4])
        teile.append(idx)
        p += 4 + (4 if flags & 1 else 2)
        if flags & 0x8:
            p += 2
        elif flags & 0x40:
            p += 4
        elif flags & 0x80:
            p += 8
        if not flags & 0x20:
            return teile


def wertgroesse(fmt):
    if fmt & 0xF0:
        raise ValueError('device tables in value records are not handled')
    return 2 * bin(fmt).count('1')


def u16(d, p):
    return struct.unpack('>H', d[p:p + 2])[0]


def klassen(d, off):
    fmt = u16(d, off)
    k = {}
    if fmt == 1:
        start, n = u16(d, off + 2), u16(d, off + 4)
        for i in range(n):
            k[start + i] = u16(d, off + 6 + 2 * i)
    else:
        for i in range(u16(d, off + 2)):
            a, b, c = struct.unpack('>HHH', d[off + 4 + 6 * i:off + 10 + 6 * i])
            for g in range(a, b + 1):
                k[g] = c
    return k


def abdeckung_schreiben(glyphs):
    return struct.pack('>HH%dH' % len(glyphs), 1, len(glyphs), *glyphs)


def klassen_schreiben(k):
    bereiche = []
    for g in sorted(k):
        if k[g] == 0:
            continue
        if bereiche and bereiche[-1][1] == g - 1 and bereiche[-1][2] == k[g]:
            bereiche[-1][1] = g
        else:
            bereiche.append([g, g, k[g]])
    out = struct.pack('>HH', 2, len(bereiche))
    for a, b, c in bereiche:
        out += struct.pack('>HHH', a, b, c)
    return out


def paar_neu(d, so, behalten):
    """One PairPos subtable, cut down to the kept glyphs."""
    fmt, cov_off, vf1, vf2 = struct.unpack('>HHHH', d[so:so + 8])
    cov = abdeckung(d, so + cov_off)
    g1, g2 = wertgroesse(vf1), wertgroesse(vf2)
    if fmt == 1:
        n = u16(d, so + 8)
        erste, saetze = [], []
        for k, g in enumerate(cov[:n]):
            if g not in behalten:
                continue
            ps = so + u16(d, so + 10 + 2 * k)
            m = u16(d, ps)
            satz = b''
            anzahl = 0
            for j in range(m):
                r = ps + 2 + j * (2 + g1 + g2)
                if u16(d, r) in behalten:
                    satz += d[r:r + 2 + g1 + g2]
                    anzahl += 1
            if anzahl:
                erste.append(g)
                saetze.append(struct.pack('>H', anzahl) + satz)
        kopf = 10 + 2 * len(saetze)
        cov_neu = abdeckung_schreiben(erste)
        offs, body, pos = [], b'', kopf + len(cov_neu)
        for s in saetze:
            offs.append(pos)
            body += s
            pos += len(s)
        return (struct.pack('>HHHHH', 1, kopf, vf1, vf2, len(saetze))
                + struct.pack('>%dH' % len(offs), *offs) + cov_neu + body)
    cd1, cd2, c1, c2 = struct.unpack('>HHHH', d[so + 8:so + 16])
    k1 = {g: c for g, c in klassen(d, so + cd1).items() if g in behalten}
    k2 = {g: c for g, c in klassen(d, so + cd2).items() if g in behalten}
    matrix = d[so + 16:so + 16 + c1 * c2 * (g1 + g2)]
    cov_neu = abdeckung_schreiben([g for g in cov if g in behalten])
    kd1, kd2 = klassen_schreiben(k1), klassen_schreiben(k2)
    kopf = 16 + len(matrix)
    return (struct.pack('>HHHHHHHH', 2, kopf, vf1, vf2, kopf + len(cov_neu),
                        kopf + len(cov_neu) + len(kd1), c1, c2)
            + matrix + cov_neu + kd1 + kd2)


def gpos_neu(gpos, behalten):
    """A new GPOS with only the kern lookup, cut down to the kept glyphs."""
    fl = u16(gpos, 6)
    ll = u16(gpos, 8)
    kern = None
    for i in range(u16(gpos, fl)):
        tag = gpos[fl + 2 + 6 * i:fl + 6 + 6 * i]
        if tag == b'kern':
            fo = fl + u16(gpos, fl + 6 + 6 * i)
            kern = [u16(gpos, fo + 4 + 2 * j) for j in range(u16(gpos, fo + 2))]
            break
    teile = []
    flag = 0
    for li in kern or []:
        lo = ll + u16(gpos, ll + 2 + 2 * li)
        typ, flag, ns = struct.unpack('>HHH', gpos[lo:lo + 6])
        for s in range(ns):
            so = lo + u16(gpos, lo + 6 + 2 * s)
            tt = typ
            if typ == 9:
                tt = u16(gpos, so + 2)
                so = so + struct.unpack('>I', gpos[so + 4:so + 8])[0]
            if tt == 2:
                teile.append(paar_neu(gpos, so, behalten))
    langsys = struct.pack('>HHHH', 0, 0xFFFF, 1, 0)
    script = struct.pack('>HH', 4, 0) + langsys
    scriptlist = struct.pack('>H', 2)
    scriptlist += b'DFLT' + struct.pack('>H', 14) + b'latn' + struct.pack('>H', 14 + len(script))
    scriptlist += script + script
    feature = struct.pack('>HHH', 0, 1, 0)
    featurelist = struct.pack('>H', 1) + b'kern' + struct.pack('>H', 8) + feature
    n = len(teile)
    lookup_kopf = 6 + 2 * n
    ext_offs = [lookup_kopf + 8 * i for i in range(n)]
    lookup = struct.pack('>HHH', 9, flag, n) + struct.pack('>%dH' % n, *ext_offs)
    pos = lookup_kopf + 8 * n
    exts, body = b'', b''
    for i, teil in enumerate(teile):
        exts += struct.pack('>HHI', 1, 2, pos - ext_offs[i])
        body += teil
        pos += len(teil)
    lookuplist = struct.pack('>HH', 1, 4) + lookup + exts + body
    kopf = 10
    return (struct.pack('>HHHHH', 1, 0, kopf, kopf + len(scriptlist),
                        kopf + len(scriptlist) + len(featurelist))
            + scriptlist + featurelist + lookuplist)


# The SIL OFL forbids the Reserved Font Name ("Lato") on a modified version,
# and a subset is one. These name records carry the font's name; copyright,
# licence and designer records stay as they are.
NAME_IDS = {1, 3, 4, 6, 16, 18, 21}
NEUER_NAME = 'Xharig Sans'


def name_umbenennen(name, alt='Lato', neu=NEUER_NAME):
    fmt, anzahl, start = struct.unpack('>HHH', name[:6])
    if fmt != 0:
        raise ValueError('name table format %d is not handled' % fmt)
    saetze, daten = [], b''
    for i in range(anzahl):
        pid, eid, lid, nid, laenge, off = struct.unpack('>HHHHHH', name[6 + 12 * i:18 + 12 * i])
        roh = name[start + off:start + off + laenge]
        if nid in NAME_IDS:
            kodierung = 'mac_roman' if pid == 1 else 'utf-16-be'
            text = roh.decode(kodierung)
            ersatz = neu.replace(' ', '') if nid == 6 else neu
            roh = text.replace(alt, ersatz).encode(kodierung)
        saetze.append((pid, eid, lid, nid, len(roh), len(daten)))
        daten += roh
    kopf = struct.pack('>HHH', 0, anzahl, 6 + 12 * anzahl)
    return kopf + b''.join(struct.pack('>HHHHHH', *s) for s in saetze) + daten


def namen_lesen(name, ids=(1, 4, 6)):
    _fmt, anzahl, start = struct.unpack('>HHH', name[:6])
    gefunden = []
    for i in range(anzahl):
        pid, _eid, _lid, nid, laenge, off = struct.unpack('>HHHHHH', name[6 + 12 * i:18 + 12 * i])
        if nid in ids and pid == 3:
            gefunden.append((nid, name[start + off:start + off + laenge].decode('utf-16-be')))
    return gefunden


def ub128(n):
    teile = [n & 0x7F]
    n >>= 7
    while n:
        teile.insert(0, (n & 0x7F) | 0x80)
        n >>= 7
    return bytes(teile)


def main(quelle, ziel):
    d = open(quelle, 'rb').read()
    t = tabellen(d)
    karte = cmap_lesen(t['cmap'])
    behalten = {0}
    for a, b in BEREICHE:
        for cp in range(a, b + 1):
            if cp in karte:
                behalten.add(karte[cp])
    glyf, lang = glyphen(t)
    while True:
        neu = gsub_huelle(t.get('GSUB'), behalten)
        for g in list(behalten):
            for k in bestandteile(glyf[g]):
                if k not in behalten:
                    behalten.add(k)
                    neu = True
        if not neu:
            break

    daten, offs = b'', [0]
    for i, g in enumerate(glyf):
        if i in behalten and g:
            g = g + b'\0' * (-len(g) % 4)
            daten += g
        offs.append(len(daten))
    t['glyf'] = daten
    if lang:
        t['loca'] = struct.pack('>%dI' % len(offs), *offs)
    else:
        t['loca'] = struct.pack('>%dH' % len(offs), *[o // 2 for o in offs])
    if 'GPOS' in t:
        t['GPOS'] = gpos_neu(t['GPOS'], behalten)
    # Metrics of emptied glyphs are never used; zeros compress to nothing.
    lm = struct.unpack('>H', t['hhea'][34:36])[0]
    hmtx = bytearray(t['hmtx'])
    for i in range(len(glyf)):
        if i not in behalten:
            if i < lm:
                hmtx[4 * i:4 * i + 4] = b'\0\0\0\0'
            else:
                p = 4 * lm + 2 * (i - lm)
                hmtx[p:p + 2] = b'\0\0'
    t['hmtx'] = bytes(hmtx)
    t['name'] = name_umbenennen(t['name'])
    for nid, text in namen_lesen(t['name']):
        print('  name %d: %s' % (nid, text))
    if '--groessen' in sys.argv:
        for tag in sorted(t):
            print(tag, len(t[tag]))
    # Kept glyph data shrank; drop the stale checksum adjustment.
    t['head'] = t['head'][:8] + b'\0\0\0\0' + t['head'][12:]

    tags = sorted(t)
    bekannt = {'cmap': 0, 'head': 1, 'hhea': 2, 'hmtx': 3, 'maxp': 4, 'name': 5,
               'OS/2': 6, 'post': 7, 'cvt ': 8, 'fpgm': 9, 'glyf': 10, 'loca': 11,
               'prep': 12}
    verzeichnis = b''
    strom = b''
    sfnt = 12 + 16 * len(tags)
    for tag in tags:
        inhalt = t[tag]
        idx = bekannt.get(tag, 63)
        flags = idx | ((3 << 6) if tag in ('glyf', 'loca') else 0)
        verzeichnis += bytes([flags])
        if idx == 63:
            verzeichnis += tag.encode('latin-1')
        verzeichnis += ub128(len(inhalt))
        strom += inhalt
        sfnt += len(inhalt) + (-len(inhalt) % 4)

    with tempfile.TemporaryDirectory() as tmp:
        roh = os.path.join(tmp, 'roh')
        gepackt = os.path.join(tmp, 'roh.br')
        open(roh, 'wb').write(strom)
        subprocess.run(['brotli', '-q', '11', '-f', '-o', gepackt, roh], check=True)
        br = open(gepackt, 'rb').read()

    koerper = verzeichnis + br
    laenge = 48 + len(koerper)
    laenge += -laenge % 4
    kopf = struct.pack('>4sIIHHIIHHIIIII', b'wOF2', 0x00010000, laenge, len(tags), 0,
                       sfnt, len(br), 1, 0, 0, 0, 0, 0, 0)
    datei = kopf + koerper
    datei += b'\0' * (laenge - len(datei))
    open(ziel, 'wb').write(datei)
    print('%s: %d glyphs kept of %d, %d bytes' % (ziel, len(behalten), len(glyf), len(datei)))


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2])
