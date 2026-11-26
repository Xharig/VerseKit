# -*- coding: utf-8 -*-
"""Maskiert die persönlichen Beta-Codes einer Testerliste im Protokoll eines Laufs und prüft die Liste.

Aufruf:  python3 .github/scripts/beta_masken.py <tester.json>

Die Liste ist `[{"tester": "…", "code": "XXXX-XXXX-XXXX-XXXX-XXXX"}]`. Für jeden Code und jeden Namen steht danach
`::add-mask::` im Protokoll, damit er dort nie lesbar wird, auch in der Form ohne Striche. Ausgegeben wird sonst nichts
(nie ein Code, nie ein Name); Fehler nennen nur die Stelle in der Liste.
"""
import json
import sys

ZEICHEN = set('0123456789ABCDEFGHJKMNPQRSTVWXYZ')


def normal(text):
    """Der Code in Grundform wie im Programm (`Code::parse`): Striche und Leerraum weg, O = 0, I und L = 1."""
    rein = []
    for c in str(text):
        if c == '-' or c.isspace():
            continue
        c = c.upper()
        c = {'O': '0', 'I': '1', 'L': '1'}.get(c, c)
        rein.append(c)
    rein = ''.join(rein)
    return rein if len(rein) == 20 and set(rein) <= ZEICHEN else None


def masken(eintraege):
    """(Zeilen für das Protokoll, Fehler). Ein Eintrag ohne gültigen Code ist ein Fehler, ebenso doppelte Kennungen."""
    zeilen, fehler, kennungen = [], [], set()
    if not isinstance(eintraege, list) or not eintraege:
        return [], ['die Liste ist leer oder keine Liste']
    for i, eintrag in enumerate(eintraege, 1):
        code = normal(eintrag.get('code', '')) if isinstance(eintrag, dict) else None
        if code is None:
            fehler.append('Eintrag %d: kein Code' % i)
            continue
        if code[:4] in kennungen:
            fehler.append('Eintrag %d: die ersten vier Zeichen sind schon vergeben' % i)
        kennungen.add(code[:4])
        gezeigt = '-'.join(code[k:k + 4] for k in range(0, 20, 4))
        for text in (code, gezeigt, str(eintrag.get('tester', '')).strip()):
            if text:
                zeilen.append('::add-mask::' + text)
    return zeilen, fehler


def main():
    if len(sys.argv) != 2:
        print('Aufruf: beta_masken.py <tester.json>', file=sys.stderr)
        return 2
    try:
        with open(sys.argv[1], encoding='utf-8') as f:
            eintraege = json.load(f)
    except (OSError, ValueError):
        print('::error::Die Testerliste ist kein JSON.')
        return 1
    zeilen, fehler = masken(eintraege)
    for zeile in zeilen:
        print(zeile)
    for text in fehler:
        print('::error::' + text)
    return 1 if fehler else 0


if __name__ == '__main__':
    sys.exit(main())
