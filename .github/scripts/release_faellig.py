# -*- coding: utf-8 -*-
"""
Prueft, ob heute eine Veroeffentlichung faellig ist.

Gedacht fuer den Zeitplan-Ablauf `geplant.yml`, der mittwochs nachsieht. Er darf
**nicht** blind jede Woche taggen — sonst geht irgendwann eine halbfertige Version
raus, nur weil Mittwoch ist. Faellig ist eine Version genau dann, wenn alle vier
Punkte stimmen:

  1. `__version__` in `sc_bp_watcher.py` steht auf einer Nummer,
  2. zu der es in **beiden** CHANGELOG-Dateien einen Abschnitt gibt,
  3. dessen Datum heute oder frueher ist (das ist die bewusste Freigabe),
  4. und fuer die es noch **keinen** Tag gibt.

Fehlt eines davon, passiert nichts — und im Protokoll steht, woran es lag.

Ausgabe fuer GitHub Actions (GITHUB_OUTPUT): faellig, tag, grund
"""
import datetime
import os
import re
import subprocess
import sys

WURZEL = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CHANGELOGS = ('CHANGELOG.en.md', 'CHANGELOG.md')


def version():
    """Die Nummer aus der Versionskonstante."""
    pfad = os.path.join(WURZEL, 'sc_bp_watcher.py')
    with open(pfad, encoding='utf-8') as f:
        treffer = re.search(r"^__version__\s*=\s*['\"]([^'\"]+)['\"]", f.read(), re.M)
    return treffer.group(1) if treffer else ''


def abschnitt_datum(pfad, nummer):
    """Das Datum aus der Ueberschrift des Versions-Abschnitts — oder None.

    Erwartet `## v2.1.0 - 2026-08-26`. Ohne Datum gilt der Abschnitt als
    unfertig: Fehlt das Datum, ist der Abschnitt noch nicht zur
    Veroeffentlichung freigegeben.
    """
    try:
        with open(os.path.join(WURZEL, pfad), encoding='utf-8') as f:
            text = f.read()
    except OSError:
        return None
    for block in re.split(r'^## ', text, flags=re.M)[1:]:
        kopf = block.partition('\n')[0]
        if nummer not in kopf:
            continue
        tag = re.search(r'(\d{4})-(\d{2})-(\d{2})', kopf)
        if not tag:
            return None
        return datetime.date(int(tag.group(1)), int(tag.group(2)), int(tag.group(3)))
    return None


def tag_existiert(tag):
    """Gibt es den Tag schon? Dann ist die Version laengst draussen."""
    fertig = subprocess.run(['git', 'tag', '--list', tag],
                            cwd=WURZEL, capture_output=True, text=True)
    return bool(fertig.stdout.strip())


def melden(faellig, tag, grund):
    print(('FAELLIG: %s' % tag) if faellig else ('NICHTS ZU TUN — %s' % grund))
    ziel = os.environ.get('GITHUB_OUTPUT')
    if ziel:
        with open(ziel, 'a', encoding='utf-8') as f:
            f.write('faellig=%s\n' % ('true' if faellig else 'false'))
            f.write('tag=%s\n' % tag)
            f.write('grund=%s\n' % grund)
    return 0


def main():
    nummer = version()
    if not nummer:
        return melden(False, '', 'keine Versionskonstante gefunden')
    tag = 'v' + nummer

    if tag_existiert(tag):
        return melden(False, tag, '%s ist bereits veroeffentlicht' % tag)

    heute = datetime.date.today()
    for pfad in CHANGELOGS:
        datum = abschnitt_datum(pfad, nummer)
        if datum is None:
            return melden(False, tag,
                          '%s hat keinen datierten Abschnitt fuer %s' % (pfad, nummer))
        if datum > heute:
            return melden(False, tag,
                          '%s ist auf %s datiert, das ist noch nicht so weit'
                          % (pfad, datum.isoformat()))

    return melden(True, tag, 'Version, Changelog und Datum stimmen')


if __name__ == '__main__':
    sys.exit(main())
