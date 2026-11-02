# -*- coding: utf-8 -*-
"""Prüft, ob der Git-Tag und die Version des Programms zusammenpassen.

Die Version steht in `Cargo.toml` unter `[workspace.package]`. Ein Tag `v5.0.0-rc1`
gehört zur Version `5.0.0-rc1`.

Aufruf:  python3 .github/scripts/version_pruefen.py v5.0.0
         python3 .github/scripts/version_pruefen.py --nur-version
"""
import re
import sys


def version_im_code(pfad='Cargo.toml'):
    with open(pfad, encoding='utf-8') as f:
        text = f.read()
    abschnitt = re.search(r'^\[workspace\.package\]\s*$(.*?)(?=^\[|\Z)', text, re.M | re.S)
    if not abschnitt:
        return ''
    m = re.search(r'^version\s*=\s*"([^"]+)"', abschnitt.group(1), re.M)
    return m.group(1) if m else ''


def main():
    if sys.argv[1:] == ['--nur-version']:
        print(version_im_code())
        return 0 if version_im_code() else 1
    tag = (sys.argv[1] if len(sys.argv) > 1 else '').lstrip('v')
    code = version_im_code()
    print('Tag: %s · im Code: %s' % (tag or '(keiner)', code))
    if not code:
        print('::error::Keine Version unter [workspace.package] in Cargo.toml gefunden.')
        return 1
    if tag and tag != code:
        print('::error::Tag (%s) und die Version in Cargo.toml (%s) stimmen nicht überein. '
              'Erst die Version anheben, dann taggen.' % (tag, code))
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
