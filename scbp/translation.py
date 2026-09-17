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
(`scbp/injection.py`): welche Missionen einen Bauplan geben und welche davon
man **schon hat**. Das kann keine der Fremdquellen leisten — den eigenen
Bestand kennt nur dieses Werkzeug.
"""
import io
import json
import os
import time
import urllib.request
import zipfile

from . import paths
from .language import t

NOTE_FILE = 'uebersetzung.json'
USER_AGENT = 'SC-BP-Watcher (+https://github.com/Xharig/VerseKit)'
TIMEOUT = 60

# Die Fremdquellen. `sprache` ist der Ordnername, unter dem Star Citizen die
# Datei erwartet — er entscheidet zugleich, was in die `user.cfg` muss.
SOURCES = {
    'deutsch': {
        'repo':     'rjcncpt/StarCitizen-Deutsch-INI',
        # ⚠⚠ **Die Datei im Repo, nicht das Release** (17.09.2026). rjcncpt
        # veröffentlicht neue Stände nicht mehr als Release: Das neueste war
        # `2026.09.08-LIVE` mit Textstand 29.08., während `live/global.ini` im
        # Repo am 16.09. für Patch 4.10.1 nachgezogen war. VerseKit fragte nur
        # nach Releases, sah „nichts Neues" — und im Flottenmanager stand
        # `@vehicle_NameAEGS_Sabre_Raven_EX`, weil der alten Datei die neuen
        # Schiffe fehlten. Das Release bleibt als Rückfall (`datei`).
        'repo_datei': 'live/global.ini',
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

# Kennung → Tag der Fassung (`JJJJ-MM-TT`), wie ihn die letzte Abfrage nannte.
# Die Kennung selbst (`git-082b11db5e73`) sagt einem Spieler nichts; ob seine
# Übersetzung aktuell ist, erkennt er am Datum.
_version_dates = {}


def _net_error(e):
    """Den Netzfehler im Klartext merken."""
    text = str(e)
    if 'CERTIFICATE' in text.upper() or 'SSL' in text.upper():
        last_error[0] = t('m_kein_zertifikat')
    elif getattr(e, 'code', None) == 403 or '403' in text:
        last_error[0] = t('m_abgewiesen')
    else:
        last_error[0] = text


def latest(source):
    """Die neueste Version einer Quelle: (Kennung, Adresse, Größe) oder None.

    Die Kennung ist der Release-Tag. Bei StarStrings heißt der Tag immer
    `latest` — dort taugt er nicht zum Vergleichen, deshalb wird zusätzlich
    das Veröffentlichungsdatum genommen."""
    q = SOURCES.get(source)
    if not q:
        return None
    if q.get('repo_datei'):
        # Kennung = der letzte Commit, der genau diese Datei geändert hat.
        # Die Adresse zeigt auf DIESEN Commit, nicht auf `main` — sonst könnte
        # zwischen Nachsehen und Laden eine andere Fassung kommen, als vermerkt.
        try:
            commits = _fetch('https://api.github.com/repos/%s/commits?path=%s'
                             '&per_page=1' % (q['repo'], q['repo_datei']))
        except Exception as e:
            # ⚠⚠ **Kein Rückfall aufs Release, wenn die Abfrage scheitert**
            # (17.09.2026). Das Release trägt eine andere Kennung als die
            # eingesetzte Repo-Datei — ein abgewiesener Abruf (GitHub-Limit)
            # hätte als „neue Fassung" gegolten und die veraltete Datei aus dem
            # Release über die aktuelle geschrieben.
            _net_error(e)
            return None
        first = (commits[0] or {}) if isinstance(commits, list) and commits else {}
        sha = first.get('sha') or ''
        if sha:
            last_error[0] = None
            ident = 'git-%s' % sha[:12]
            stamp = ((first.get('commit') or {}).get('committer') or {}).get('date')
            if stamp:
                _version_dates[ident] = stamp[:10]
            return (ident, 'https://raw.githubusercontent.com/%s/%s/%s'
                    % (q['repo'], sha, q['repo_datei']), 0)
        # Datei im Repo nicht (mehr) gefunden: dann das Release, wie früher.
    try:
        r = _fetch('https://api.github.com/repos/%s/releases/latest' % q['repo'])
        last_error[0] = None
    except Exception as e:
        # Zertifikatsfehler eigens benennen — die Meldung von OpenSSL ist für
        # Nichttechniker unlesbar, die Ursache aber immer dieselbe.
        # ⚠ Auch 403 eigens: eine Absage, kein Netzfehler. Bei GitHub ist es
        # meist das Abruflimit. Ohne eigene Meldung sucht man beim eigenen
        # Anschluss.
        _net_error(e)
        return None
    ident = r.get('tag_name') or ''
    if ident.lower() in ('latest', ''):
        ident = (r.get('published_at') or '')[:19]
    if r.get('published_at'):
        _version_dates[ident] = r['published_at'][:10]
    for a in r.get('assets') or []:
        if a.get('name') == q['datei']:
            return ident, a.get('browser_download_url'), a.get('size') or 0
    return None


def _note():
    try:
        with open(paths.app_file(NOTE_FILE), encoding='utf-8') as f:
            d = json.load(f)
        return d if isinstance(d, dict) else {}
    except Exception:
        return {}


def _note_write(d):
    target = paths.app_file(NOTE_FILE)
    try:
        with open(target + '.tmp', 'w', encoding='utf-8') as f:
            json.dump(d, f, ensure_ascii=False, indent=1)
        os.replace(target + '.tmp', target)
    except OSError:
        pass


def _note_set(source, ident):
    d = _note()
    entry = {'kennung': ident, 'stand': time.strftime('%Y-%m-%d %H:%M')}
    if _version_dates.get(ident):
        entry['datum'] = _version_dates[ident]
        # Frisch geholt heißt: eben nachgesehen, und es ist die neueste.
        entry['geprueft'] = entry['stand']
        entry['pruefung'] = 'aktuell'
    d[source] = entry
    _note_write(d)


def _record_check(source, result, ident=None):
    """Festhalten, wann zuletzt nachgesehen wurde und mit welchem Ergebnis.

    `result`: `aktuell`, `neu` oder `fehler`. Ist die eingesetzte Fassung die
    neueste, wird ihr Datum nachgetragen — Installationen von vor v3.48.5
    kennen es sonst nicht."""
    d = _note()
    entry = d.get(source)
    if not isinstance(entry, dict):
        return
    entry['geprueft'] = time.strftime('%Y-%m-%d %H:%M')
    entry['pruefung'] = result
    if result == 'aktuell' and _version_dates.get(ident):
        entry['datum'] = _version_dates[ident]
    _note_write(d)


def info(source):
    """Der Vermerk einer Quelle als dict (leer, wenn keiner da ist)."""
    entry = _note().get(source)
    return dict(entry) if isinstance(entry, dict) else {}


def _day(stamp, lang=None):
    """`2026-09-16` → `16.09.2026` (deutsch) bzw. unverändert (englisch)."""
    from . import language
    lang = lang or language.current()
    try:
        y, m, d = stamp[:10].split('-')
    except (ValueError, TypeError, AttributeError):
        return ''
    return '%s.%s.%s' % (d, m, y) if lang == 'de' else '%s-%s-%s' % (y, m, d)


def status_text(source, lang=None, today=None):
    """Für Spieler lesbar: Stand der Übersetzung und ob sie aktuell ist.

    Beispiel: „Stand 16.09.2026 · aktuell, nachgesehen heute 04:52".
    ⚠ Nie die Kennung (`git-082b11db5e73`) — daran erkennt niemand etwas."""
    entry = info(source)
    parts = []
    if entry.get('datum'):
        parts.append(t('s_sp_stand_vom') % _day(entry['datum'], lang))
    checked = entry.get('geprueft') or ''
    if checked:
        today = today or time.strftime('%Y-%m-%d')
        clock = checked[11:16]
        when = (t('s_sp_heute') % clock if checked[:10] == today
                else '%s %s' % (_day(checked, lang), clock))
        result = entry.get('pruefung')
        key = {'aktuell': 's_sp_ist_aktuell', 'neu': 's_sp_neuere_da',
               'fehler': 's_sp_nicht_geprueft'}.get(result)
        if key:
            parts.append(t(key) % when)
    return ' · '.join(parts)


def forget_note(source):
    """Den Vermerk einer Quelle löschen — ihre Datei wurde ersetzt.

    Gebraucht von `gametext.fetch()`: Ersetzt „Original" eine StarStrings-Datei,
    die das Werkzeug selbst eingesetzt hatte, darf die Quelle nicht weiter als
    eingerichtet gelten. Sonst meldet die Lage „StarStrings", obwohl das
    Original drinsteht — und der nächste Wechsel auf Original würde die frische
    Originaldatei gleich noch einmal ersetzen."""
    d = _note()
    if source not in d:
        return
    del d[source]
    _note_write(d)


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
        _record_check(source, 'fehler')
        return False, None
    newer = fresh[0] != installed(source)
    _record_check(source, 'neu' if newer else 'aktuell', fresh[0])
    return newer, fresh[0]


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
    root = game_dir or paths.game_folder()
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
    „Original" nahm `injection.ini_file()` deshalb eine feste Reihenfolge —
    erst `english`, dann `german_(germany)` — und beide Dateien gibt es fast
    immer. Ergebnis: Wir schrieben in die englische, das Spiel las die deutsche.
    Eingetragen wurde also korrekt, angekommen ist nie etwas, und die Statuszeile
    meldete trotzdem Erfolg. Am 29.08.2026 gemeldet; es erklärt vermutlich
    monatelang nicht ankommende Auftragstexte.
    """
    root = game_dir or paths.game_folder()
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
    root = game_dir or paths.game_folder()
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

    # Die Repo-Datei kommt als blanke `global.ini`, das Release als Archiv.
    if content[:2] == b'PK':
        ini = _ini_from_zip(content, q['sprache'])
    else:
        ini = content if b'\nvehicle_' in content[:20000000] else None
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
    from . import injection
    injection.discard_origtext()
    return True, '%s (%s), %.1f MB' % (q['name'], ident, len(ini) / 1048576.0)
