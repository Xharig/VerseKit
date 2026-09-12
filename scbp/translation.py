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
Die Textdatei des Spiels holen und aktuell halten.

Star Citizen liest seine Texte aus `Data/Localization/<sprache>/global.ini`.
Liegt dort eine Datei, hat sie **Vorrang** vor der Version im `Data.p4k` —
genau darauf beruhen die Community-Übersetzungen, und CIG gibt das
ausdrücklich frei.

Drei mögliche Grundlagen:

  1. **Deutsche Übersetzung** — `rjcncpt/StarCitizen-Deutsch-INI`, das Projekt,
     aus dem auch der SC Deutsch Launcher schöpft (CC-BY-NC-SA-4.0)
  2. **StarStrings** — `MrKraken/StarStrings`, aufgeräumte englische Texte
  3. **Original** — die englische Version direkt aus dem `Data.p4k` des Spielers
     (kein Download nötig, siehe `tools/extract_global_ini.py`)

> **Nichts davon wird mitgeliefert.** Beide Fremdprojekte behalten ihre Rechte,
> und ihre Lizenzen vertragen sich nicht mit der GPL dieses Werkzeugs. Geholt
> wird zur Laufzeit, von der Original-Adresse, auf Wunsch des Nutzers — dasselbe
> Vorgehen wie beim Bauplan-Katalog von scmdb.

Was der Watcher **selbst** beisteuert, ist die Bauplan-Auszeichnung obendrauf
(`scbp/injektion.py`): welche Missionen einen Bauplan geben und welche davon
man **schon hat**. Das kann keine der Fremdquellen leisten — den eigenen
Bestand kennt nur dieses Werkzeug.
"""
import io
import json
import os
import time
import urllib.request
import zipfile

from . import pfade
from .sprache import t

NOTE_FILE = 'uebersetzung.json'
USER_AGENT = 'SC-BP-Watcher (+https://github.com/Xharig/SC-BP-Watcher)'
TIMEOUT = 60

# Die Fremdquellen. `sprache` ist der Ordnername, unter dem Star Citizen die
# Datei erwartet — er entscheidet zugleich, was in die `user.cfg` muss.
SOURCES = {
    'deutsch': {
        'repo':     'rjcncpt/StarCitizen-Deutsch-INI',
        'datei':    'StarCitizen.Deutsch.LIVE.zip',
        'sprache':  'german_(germany)',
        'ton':      'english',
        'name':     'Deutsche Übersetzung (rjcncpt)',
        'lizenz':   'CC-BY-NC-SA-4.0',
        'seite':    'https://github.com/rjcncpt/StarCitizen-Deutsch-INI',
    },
    'starstrings': {
        'repo':     'MrKraken/StarStrings',
        'datei':    'StarStrings-LIVE.zip',
        'sprache':  'english',
        'ton':      None,
        'name':     'StarStrings (aufgeräumte englische Texte)',
        'lizenz':   'siehe Projektseite',
        'seite':    'https://github.com/MrKraken/StarStrings',
    },
}


def _fetch(url, raw=False):
    # ⚠ `SC_BP_NO_NET` gilt hier genauso. Die Anleitung verspricht, dass sich
    # die Netzabrufe abschalten lassen — bis rc42 galt das für den Katalog,
    # die Preise, die Orte, den Serverstatus und die Update-Frage, aber nicht
    # für die Übersetzungsquellen und die Auftragsdaten. Ein Versprechen, das
    # nur zum Teil eingehalten wird, ist keines.
    from .catalog import OFF
    if OFF:
        raise OSError('Netzabrufe sind abgeschaltet (SC_BP_NO_NET)')
    req = urllib.request.Request(url, headers={'User-Agent': USER_AGENT})
    with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
        data = r.read()
    return data if raw else json.loads(data.decode('utf-8'))


# --------------------------------------------------------------- Was ist neu?
# Der zuletzt aufgetretene Netzfehler — im Klartext, für die Anzeige.
# Ohne diese Zeile stand bei einem Zertifikatsproblem nur „Version nicht
# gefunden" im Fenster, was nach „das Release existiert nicht" aussieht und in
# die völlig falsche Richtung führt. Die Diagnose kostete am 24.08.2026 eine
# halbe Stunde, obwohl die Ausnahme den Grund kannte.
last_error = [None]


def latest(source):
    """Die neueste Version einer Quelle: (Kennung, Adresse, Größe) oder None.

    Die Kennung ist der Release-Tag. Bei StarStrings heißt der Tag immer
    `latest` — dort taugt er nicht zum Vergleichen, deshalb wird zusätzlich
    das Veröffentlichungsdatum genommen."""
    q = SOURCES.get(source)
    if not q:
        return None
    try:
        r = _fetch('https://api.github.com/repos/%s/releases/latest' % q['repo'])
        last_error[0] = None
    except Exception as e:
        # Zertifikatsfehler eigens benennen — die Meldung von OpenSSL ist für
        # Nichttechniker unlesbar, die Ursache aber immer dieselbe.
        text = str(e)
        if 'CERTIFICATE' in text.upper() or 'SSL' in text.upper():
            last_error[0] = t('m_kein_zertifikat')
        elif getattr(e, 'code', None) == 403 or '403' in text:
            # ⚠ Auch hier gilt: 403 ist eine Absage, kein Netzfehler. Bei
            # GitHub ist es meist das Abruflimit, bei Cloudflare-Seiten der
            # Bot-Schutz. Ohne eigene Meldung sucht man beim eigenen Anschluss.
            last_error[0] = t('m_abgewiesen')
        else:
            last_error[0] = text
        return None
    ident = r.get('tag_name') or ''
    if ident.lower() in ('latest', ''):
        ident = (r.get('published_at') or '')[:19]
    for a in r.get('assets') or []:
        if a.get('name') == q['datei']:
            return ident, a.get('browser_download_url'), a.get('size') or 0
    return None


def _note():
    try:
        with open(pfade.app_datei(NOTE_FILE), encoding='utf-8') as f:
            d = json.load(f)
        return d if isinstance(d, dict) else {}
    except Exception:
        return {}


def _note_set(source, ident):
    d = _note()
    d[source] = {'kennung': ident, 'stand': time.strftime('%Y-%m-%d %H:%M')}
    target = pfade.app_datei(NOTE_FILE)
    try:
        with open(target + '.tmp', 'w', encoding='utf-8') as f:
            json.dump(d, f, ensure_ascii=False, indent=1)
        os.replace(target + '.tmp', target)
    except OSError:
        pass


def note(source, ident):
    """Eine Quelle als eingerichtet festhalten.

    Auch für den Weg „Originaltexte aus dem Spiel" nötig, obwohl dort nichts
    heruntergeladen wird: Ohne Vermerk weiß der Watcher beim nächsten Start
    nicht, dass der Spieler die Bauplan-Angaben überhaupt eingerichtet hat —
    und würde sie nach einem Spiel-Patch nicht wieder eintragen."""
    _note_set(source, ident)


def installed(source):
    """Welche Version liegt hier? Kennung oder None."""
    return (_note().get(source) or {}).get('kennung')


def update_available(source):
    """(True, neue_Kennung), wenn es etwas Neueres gibt. Wirft nie."""
    fresh = latest(source)
    if not fresh:
        return False, None
    return (fresh[0] != installed(source)), fresh[0]


# ------------------------------------------------------------ Installieren
def _ini_from_zip(content, sprache):
    """Die `global.ini` aus dem Archiv holen — egal wie der Ordner geschrieben ist.

    StarStrings packt nach `Data/…`, die deutsche Übersetzung nach `data/…`.
    Unter Windows ist das dasselbe, **unter Linux nicht** — dort wäre ein
    falsch geschriebener Ordner schlicht unsichtbar für das Spiel. Deshalb wird
    hier nur auf den Dateinamen geachtet und der Zielpfad später selbst gebaut."""
    with zipfile.ZipFile(io.BytesIO(content)) as z:
        for name in z.namelist():
            parts = name.replace('\\', '/').lower().split('/')
            if parts[-1] == 'global.ini' and sprache.lower() in parts:
                return z.read(name)
        for name in z.namelist():          # Rückfall: die einzige global.ini
            if name.replace('\\', '/').lower().endswith('/global.ini'):
                return z.read(name)
    return None


def target_ini(sprache, game_dir=None):
    """Wohin die Datei gehört. Ein vorhandener `Data`-Ordner wird beibehalten,
    sonst wird `data` angelegt — Linux unterscheidet die beiden."""
    root = game_dir or pfade.spiel_ordner()
    if not root:
        return None
    for spelling in ('data', 'Data'):
        p = os.path.join(root, spelling)
        if os.path.isdir(p):
            return os.path.join(p, 'Localization', sprache, 'global.ini')
    return os.path.join(root, 'data', 'Localization', sprache, 'global.ini')


def game_language(game_dir=None):
    """Welche Sprache das Spiel wirklich liest — aus der `user.cfg`.

    Gibt den Sprachordner zurück (`german_(germany)`, `english`, …) oder `None`,
    wenn nichts eingetragen ist. Ohne Eintrag startet Star Citizen auf Englisch.

    ⚠⚠ **Warum es das braucht.** Das Werkzeug **schrieb** `g_language` schon
    lange (`user_cfg_setzen`), gelesen hat es die Zeile nie. Bei der Textquelle
    „Original" nahm `injektion.ini_datei()` deshalb eine feste Reihenfolge —
    erst `english`, dann `german_(germany)` — und beide Dateien gibt es fast
    immer. Ergebnis: Wir schrieben in die englische, das Spiel las die deutsche.
    Eingetragen wurde also korrekt, angekommen ist nie etwas, und die Statuszeile
    meldete trotzdem Erfolg. Am 29.08.2026 gemeldet; es erklärt vermutlich
    monatelang nicht ankommende Auftragstexte.
    """
    root = game_dir or pfade.spiel_ordner()
    if not root:
        return None
    path = os.path.join(root, 'user.cfg')
    try:
        with open(path, encoding='utf-8', errors='ignore') as f:
            lines = f.read().splitlines()
    except OSError:
        return None
    for z in lines:
        if z.split('=', 1)[0].strip() != 'g_language':
            continue
        value = z.split('=', 1)[1].strip() if '=' in z else ''
        # Kommentare hinter dem Wert abschneiden — die `user.cfg` erlaubt sie.
        value = value.split(';', 1)[0].split('--', 1)[0].strip().strip('"\'')
        if value:
            return value
    return None


def set_user_cfg(sprache, audio=None, game_dir=None):
    """`g_language` in der `user.cfg` setzen — **ergänzend**, nicht ersetzend.

    In dieser Datei stehen die Grafikeinstellungen des Spielers. Sie zu
    überschreiben, weil man eine Zeile ändern will, wäre ein handfester
    Schaden — deshalb wird zeilenweise gelesen und nur die betroffene Zeile
    ausgetauscht."""
    root = game_dir or pfade.spiel_ordner()
    if not root:
        return False
    path = os.path.join(root, 'user.cfg')
    lines = []
    if os.path.isfile(path):
        try:
            with open(path, encoding='utf-8', errors='ignore') as f:
                lines = f.read().splitlines()
        except OSError:
            return False
    was_set = {'g_language': False, 'g_languageAudio': audio is None}
    fresh = []
    for z in lines:
        key = z.split('=', 1)[0].strip()
        if key == 'g_language':
            fresh.append('g_language = %s' % sprache)
            was_set['g_language'] = True
        elif key == 'g_languageAudio' and audio:
            fresh.append('g_languageAudio = %s' % audio)
            was_set['g_languageAudio'] = True
        else:
            fresh.append(z)
    if not was_set['g_language']:
        fresh.append('g_language = %s' % sprache)
    if audio and not was_set['g_languageAudio']:
        fresh.append('g_languageAudio = %s' % audio)
    try:
        with open(path + '.tmp', 'w', encoding='utf-8') as f:
            f.write('\n'.join(fresh) + '\n')
        os.replace(path + '.tmp', path)
        return True
    except OSError:
        return False


def fetch(source, progress=None, game_dir=None):
    """Eine Quelle herunterladen und einsetzen. Gibt (Erfolg, Meldung) zurück."""
    def report(text):
        if progress:
            progress(text)

    q = SOURCES.get(source)
    if not q:
        return False, 'unbekannte Quelle'
    fresh = latest(source)
    if not fresh:
        return False, last_error[0] or t('m_keine_fassung')
    ident, address, byte_size = fresh

    report(t('z_laedt') % (q['name'], byte_size / 1048576.0))
    try:
        content = _fetch(address, raw=True)
    except Exception as e:
        return False, 'Download fehlgeschlagen: %s' % e

    ini = _ini_from_zip(content, q['sprache'])
    if not ini:
        return False, t('m_keine_ini_archiv')

    target = target_ini(q['sprache'], game_dir)
    if not target:
        return False, 'Star-Citizen-Ordner unbekannt'

    report(t('z_einsetzen'))
    try:
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(target + '.tmp', 'wb') as f:
            f.write(ini)
        os.replace(target + '.tmp', target)
    except OSError as e:
        return False, 'Schreiben fehlgeschlagen: %s' % e

    set_user_cfg(q['sprache'], q['ton'], game_dir)
    _note_set(source, ident)
    # ⚠ Hier liegt jetzt eine **fremde, unberührte** Datei. Die gemerkten
    # Originaltexte gehören zur alten und würden auf einen überholten Stand
    # zurückschreiben; zugleich wird vermerkt, dass in dieser Datei noch nie
    # injiziert wurde. Ohne diesen Vermerk schneidet der Formen-Notnagel beim
    # ersten Lauf fremde Kennzeichnungen heraus — bei StarStrings 17 Stück,
    # und wegen des dann falsch gemerkten „Urtextes" für immer.
    from . import injektion
    injektion.urtext_verwerfen()
    return True, '%s (%s), %.1f MB' % (q['name'], ident, len(ini) / 1048576.0)
