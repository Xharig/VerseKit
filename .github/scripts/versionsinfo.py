# -*- coding: utf-8 -*-
"""Schreibt die Versionsangaben, die Windows in der .exe anzeigt.

Grund: Ohne diese Angaben zeigt der Task-Manager den **Dateinamen** — und der
heißt aus Rücksicht auf das Selbst-Update weiter `SC-BP-Watcher.exe`. Nach der
Umbenennung zu Verse-Kit suchte dort niemand mehr nach dem alten Namen.
Mit `FileDescription` zeigt Windows stattdessen den Produktnamen.

⚠ Der Name kommt aus `scbp/language.py` (`hf_titel`), nicht von hier — er steht
im ganzen Programm nur an dieser einen Stelle (siehe `sc_bp_watcher.py`).

Aufruf:  python .github/scripts/versionsinfo.py build/versionsinfo.txt
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.dirname(
    os.path.abspath(__file__)))))

from scbp import language  # noqa: E402
from version_pruefen import version_im_code  # noqa: E402


def zahlen(version):
    """„3.56.0-rc1" → (3, 56, 0, 0). Windows verlangt reine Zahlen."""
    teile = version.split('-', 1)[0].split('.')
    werte = [int(t) for t in teile if t.isdigit()][:4]
    return tuple(werte + [0] * (4 - len(werte)))


def inhalt(version, name):
    z = zahlen(version)
    felder = (
        ('CompanyName', 'Xharig'),
        ('FileDescription', name),
        ('FileVersion', version),
        ('InternalName', name),
        ('LegalCopyright', 'GPL-3.0-only'),
        ('OriginalFilename', 'SC-BP-Watcher.exe'),
        ('ProductName', name),
        ('ProductVersion', version),
    )
    zeilen = ',\n'.join("          StringStruct(%r, %r)" % f for f in felder)
    return (
        "VSVersionInfo(\n"
        "  ffi=FixedFileInfo(filevers=%r, prodvers=%r, mask=0x3f, flags=0x0,\n"
        "                    OS=0x40004, fileType=0x1, subtype=0x0, date=(0, 0)),\n"
        "  kids=[\n"
        "    StringFileInfo([StringTable('040904B0', [\n%s])]),\n"
        "    VarFileInfo([VarStruct('Translation', [1033, 1200])])\n"
        "  ]\n"
        ")\n" % (z, z, zeilen))


def main():
    ziel = sys.argv[1] if len(sys.argv) > 1 else 'versionsinfo.txt'
    version = version_im_code()
    if not version:
        print('::error::__version__ nicht gefunden')
        return 1
    name = language.TEXTS['hf_titel'][0]
    os.makedirs(os.path.dirname(ziel) or '.', exist_ok=True)
    with open(ziel, 'w', encoding='utf-8') as f:
        f.write(inhalt(version, name))
    # ⚠ Nur ASCII ausgeben: Die Windows-Konsole (cp1252) bricht an einem
    #   Pfeil ab — gemessen beim ersten Lauf, das hätte den Bau gekippt.
    print('Versionsangaben: %s %s -> %s' % (name, version, ziel))
    return 0


if __name__ == '__main__':
    sys.exit(main())
