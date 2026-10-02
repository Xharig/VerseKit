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
> wird zur Laufzeit, von der Original-Adresse, nur wenn der Nutzer es wählt — dasselbe
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
        # ⚠⚠ **Die Datei im Repo, nicht das Release.** rjcncpt veröffentlicht
        # neue Stände nicht zuverlässig als Release; `live/global.ini` im Repo
        # ist aktueller. Wer nur nach Releases fragt, sieht nichts Neues — und
        # im Flottenmanager stehen Platzhalter wie
        # `@vehicle_NameAEGS_Sabre_Raven_EX`, weil der alten Datei die neuen
        # Schiffe fehlen. Das Release bleibt als Rückfall (`datei`).
        'repo_datei': 'live/global.ini',
        'datei':    'StarCitizen.Deutsch.LIVE.zip',
        'sprache':  'german_(germany)',
        'ton':      'english',
        'name':     'Deutsche Übersetzung (rjcncpt)',
        'lizenz':   'CC-BY-NC-SA-4.0',
        'seite':    'https://github.com/rjcncpt/StarCitizen-Deutsch-INI',
        'label':    's_sp_q_de',
        'flagge':   'de',
    },
    'starstrings': {
        'repo':     'MrKraken/StarStrings',
        'datei':    'StarStrings-LIVE.zip',
        'sprache':  'english',
        'ton':      None,
        'name':     'StarStrings (aufgeräumte englische Texte)',
        'lizenz':   'siehe Projektseite',
        'seite':    'https://github.com/MrKraken/StarStrings',
        'label':    's_sp_q_ss',
        'flagge':   'gb',
        # Die Testfassung hängt an einem eigenen Release (Vorabversion) —
        # `releases/latest` liefert sie nie, deshalb der feste Tag.
        'ptu':      {'tag': 'latest-ptu', 'datei': 'StarStrings-PTU.zip'},
    },
    # ⭐ Weitere gepflegte Übersetzungen. Dymerz liefert je Sprache eine Zip
    # mit einer blanken `global.ini`; seine PTU-Fassung ist veraltet und wird
    # deshalb nicht angeboten.
    # ⚠ Keine Lizenz angegeben — wie bei StarStrings: nichts mitliefern, nur
    # nach Wahl des Nutzers von der Adresse des Projekts laden, und auf
    # „Danke & Lizenzen" nennen.
    'dymerz_de': {
        'repo': 'Dymerz/StarCitizen-Localization', 'datei': 'german_.germany.zip',
        'sprache': 'german_(germany)', 'ton': 'english',
        'name': 'Deutsch (Dymerz)', 'lizenz': 'siehe Projektseite',
        'seite': 'https://github.com/Dymerz/StarCitizen-Localization',
        'label': 's_sp_q_dymerz_de', 'flagge': 'de',
    },
    'dymerz_fr': {
        'repo': 'Dymerz/StarCitizen-Localization', 'datei': 'french_.france.zip',
        'sprache': 'french_(france)', 'ton': 'english',
        'name': 'Français (Dymerz)', 'lizenz': 'siehe Projektseite',
        'seite': 'https://github.com/Dymerz/StarCitizen-Localization',
        'label': 's_sp_q_fr', 'flagge': 'fr',
    },
    'dymerz_it': {
        'repo': 'Dymerz/StarCitizen-Localization', 'datei': 'italian_.italy.zip',
        'sprache': 'italian_(italy)', 'ton': 'english',
        'name': 'Italiano (Dymerz)', 'lizenz': 'siehe Projektseite',
        'seite': 'https://github.com/Dymerz/StarCitizen-Localization',
        'label': 's_sp_q_it', 'flagge': 'it',
    },
    'dymerz_es': {
        'repo': 'Dymerz/StarCitizen-Localization', 'datei': 'spanish_.spain.zip',
        'sprache': 'spanish_(spain)', 'ton': 'english',
        'name': 'Español (Dymerz)', 'lizenz': 'siehe Projektseite',
        'seite': 'https://github.com/Dymerz/StarCitizen-Localization',
        'label': 's_sp_q_es', 'flagge': 'es',
    },
    # Eigenes Projekt, bringt LIVE **und** PTU in einer Zip mit (`LIVE/…`,
    # `PTU/…`) — `teil` wählt den Ordner darin.
    'thord82_es': {
        'repo': 'Thord82/Star_citizen_ES', 'datei': 'Star_citizen_ES.zip',
        'teil': 'LIVE/', 'sprache': 'spanish_(spain)', 'ton': 'english',
        'name': 'Español (Thord82)', 'lizenz': 'siehe Projektseite',
        'seite': 'https://github.com/Thord82/Star_citizen_ES',
        'label': 's_sp_q_es2', 'flagge': 'es',
        'ptu': {'teil': 'PTU/'},
    },
    'dymerz_pt': {
        'repo': 'Dymerz/StarCitizen-Localization', 'datei': 'portuguese_.brazil.zip',
        'sprache': 'portuguese_(brazil)', 'ton': 'english',
        'name': 'Português (Dymerz)', 'lizenz': 'siehe Projektseite',
        'seite': 'https://github.com/Dymerz/StarCitizen-Localization',
        'label': 's_sp_q_pt', 'flagge': 'br',
    },
    'dymerz_tr': {
        'repo': 'Dymerz/StarCitizen-Localization', 'datei': 'turkish_.turkey.zip',
        'sprache': 'turkish_(turkey)', 'ton': 'english',
        'name': 'Türkçe (Dymerz)', 'lizenz': 'siehe Projektseite',
        'seite': 'https://github.com/Dymerz/StarCitizen-Localization',
        'label': 's_sp_q_tr', 'flagge': 'tr',
    },
}

# ---------------------------------------------------------------- Luftwerft
#
# ⭐ **Die Übersetzungen des SC Launch Configurator** (Luftwerft), von ihren
# Entwicklern zur Einbindung freigegeben.
#
# ⚠ **Der Hinweis der Quelle gehört dazu:** Die Übersetzungen sind noch nicht
# ausgereift. Deshalb trägt jede dieser Quellen `hinweis`; die Rückfrage vor
# dem Umstellen zeigt ihn an. Wer eine fremde Übersetzung einsetzt, soll
# vorher wissen, worauf er sich einlässt.
#
# ⚠ Hier steht **kein Repo und kein Release**, sondern die Datei direkt: Die
# Dateien liegen nicht auf GitHub, sondern auf dem eigenen Server. Sie bauen
# sich dort aus den CIG-Patches automatisch neu auf — es gibt also keine
# Fassungsnummer zum Vergleichen, die Kennung kommt über HEAD/ETag wie bei
# einer eigenen Adresse.
_LUFTWERFT = 'https://www.luftwerft.com/download/resources/localizations'

for _kennung, _ordner, _anzeige, _flagge, _textschluessel in (
        ('luftwerft_de', 'german_(germany)',  'Deutsch (Luftwerft)',  'de',
         's_sp_q_lw_de'),
        ('luftwerft_en', 'english',           'English (Luftwerft)',  'gb',
         's_sp_q_lw_en'),
        ('luftwerft_fr', 'french_(france)',   'Français (Luftwerft)', 'fr',
         's_sp_q_lw_fr'),
        ('luftwerft_it', 'italian_(italy)',   'Italiano (Luftwerft)', 'it',
         's_sp_q_lw_it'),
        ('luftwerft_es', 'spanish_(spain)',   'Español (Luftwerft)',  'es',
         's_sp_q_lw_es')):
    SOURCES[_kennung] = {
        'url':      '%s/LIVE/%s/global.ini' % (_LUFTWERFT, _ordner),
        'sprache':  _ordner,
        # Kein eigener Ton — die Sprachausgabe bleibt, wie sie ist.
        'ton':      'english' if _ordner != 'english' else None,
        'name':     _anzeige,
        'lizenz':   'siehe Projektseite',
        'seite':    'https://www.luftwerft.com/',
        'label':    _textschluessel,
        'flagge':   _flagge,
        'hinweis':  's_sp_hinweis_luftwerft',
        'ptu': {'url': '%s/PTU/%s/global.ini' % (_LUFTWERFT, _ordner)},
    }
del _kennung, _ordner, _anzeige, _flagge, _textschluessel

# ⭐ Die eigene Adresse (v3.59.0, wie SC Deutsch Launcher und SCLC): Der
# Spieler trägt selbst ein, wo eine Übersetzung liegt. Keine Zeile in
# `SOURCES` — Adresse und Sprachordner stehen je Kanal in den Einstellungen.
CUSTOM = 'eigene'
SETTING_CUSTOM = 'eigene_quelle'
# Welche Quelle ein NICHT-Haupt-Kanal (PTU …) benutzt: {Kanal: Quelle}.
SETTING_CHANNELS = 'kanal_quellen'
# Kanäle, für die eine Übersetzung wählbar ist.
CHANNELS = ('LIVE', 'HOTFIX', 'PTU', 'EPTU')
# Kanäle, die die Testfassung einer Quelle bekommen. HOTFIX läuft wie LIVE.
TEST_CHANNELS = ('PTU', 'EPTU')
# Die Sprachordner, die Star Citizen kennt — Auswahl für die eigene Adresse.
GAME_LANGUAGES = ('english', 'german_(germany)', 'french_(france)',
                  'italian_(italy)', 'spanish_(spain)', 'portuguese_(brazil)',
                  'turkish_(turkey)', 'chinese_(simplified)',
                  'korean_(south_korea)', 'japanese_(japan)', 'polish_(poland)')
# Mehr als das lädt niemand als Textdatei (die echte hat rund 16 MB).
CUSTOM_MAX_BYTES = 80 * 1048576


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
# Ohne diese Zeile stünde bei einem Zertifikatsproblem nur, dass die Version
# nicht gefunden wurde — das sieht aus, als gäbe es das Release nicht, und
# führt in die völlig falsche Richtung, obwohl die Ausnahme den Grund kennt.
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


# ------------------------------------------------------ Kanäle und Quellen
# ⭐ Jeder Kanal (LIVE, PTU …) kann eine eigene Textquelle haben.
# `channel=None` heißt überall: die Hauptinstallation (der Spielordner aus
# den Einstellungen) — dort stehen die Vermerke unter dem blanken
# Quellennamen, damit bestehende Installationen nichts merken.
# Ein Nebenkanal bekommt seine Vermerke unter `quelle@KANAL`.

def _key(source, channel=None):
    return source if not channel else '%s@%s' % (source, channel)


def _custom_settings(channel=None):
    """Adresse und Sprachordner der eigenen Quelle für einen Kanal.

    ⚠ `paths.settings()`, nicht `paths.setting()`: Das Letzte liefert nur
    Text und hält ein Wörterbuch für nicht gesetzt."""
    all_settings = paths.settings().get(SETTING_CUSTOM)
    if not isinstance(all_settings, dict):
        return {}
    entry = all_settings.get(channel or 'MAIN')
    return dict(entry) if isinstance(entry, dict) else {}


def set_custom(url, language_folder, channel=None):
    """Die eigene Adresse für einen Kanal merken. Gibt die bereinigte Adresse
    zurück — oder None, wenn sie nicht taugt (nur `https://`)."""
    url = normalize_url(url)
    if not url:
        return None
    all_settings = paths.settings().get(SETTING_CUSTOM)
    all_settings = dict(all_settings) if isinstance(all_settings, dict) else {}
    all_settings[channel or 'MAIN'] = {
        'url': url,
        'sprache': language_folder if language_folder in GAME_LANGUAGES
        else 'english'}
    paths.set_setting(SETTING_CUSTOM, all_settings)
    return url


def normalize_url(url):
    """Aus einer eingefügten Adresse die zum Herunterladen machen.

    Eine Datei-Ansicht auf GitHub (`github.com/…/blob/…`) liefert eine
    Webseite, keine Datei — daraus wird die Rohfassung. ⚠ Nur `https://`:
    Über eine offene Verbindung könnte unterwegs jemand eine andere Datei
    unterschieben, und die landet im Spiel."""
    url = (url or '').strip()
    if not url.lower().startswith('https://'):
        return None
    marker = 'https://github.com/'
    if url.startswith(marker) and '/blob/' in url:
        owner_repo, _, rest = url[len(marker):].partition('/blob/')
        url = 'https://raw.githubusercontent.com/%s/%s' % (owner_repo, rest)
    return url


def _spec(source, channel=None):
    """Die Angaben einer Quelle für einen Kanal — oder None, wenn es für
    diesen Kanal keine Fassung gibt (rjcncpt kennt kein PTU)."""
    if source == CUSTOM:
        custom = _custom_settings(channel)
        if not custom.get('url'):
            return None
        return {'url': custom['url'], 'sprache': custom.get('sprache') or 'english',
                'ton': None, 'name': t('s_sp_q_eigen'), 'label': 's_sp_q_eigen'}
    q = SOURCES.get(source)
    if not q:
        return None
    if channel in TEST_CHANNELS:
        if not q.get('ptu'):
            return None
        merged = dict(q)
        merged.update(q['ptu'])
        return merged
    return q


def available(source, channel=None):
    """Gibt es von dieser Quelle eine Fassung für den Kanal?"""
    if source in ('original', CUSTOM):
        return True
    return _spec(source, channel) is not None


def language_folder(source, channel=None):
    """In welchen Sprachordner die Quelle schreibt (`french_(france)` …)."""
    if source == 'original':
        return 'english'
    # Bei der eigenen Adresse gilt, wohin wirklich geschrieben wurde — das
    # Archiv kann einen anderen Ordner vorgeben als die Auswahl im Fenster.
    if source == CUSTOM and info(source, channel).get('sprache'):
        return info(source, channel)['sprache']
    spec = _spec(source, channel)
    if spec:
        return spec.get('sprache') or 'english'
    # Die eigene Quelle ohne Adresse: was beim Einsetzen vermerkt wurde.
    return info(source, channel).get('sprache') or 'english'


def grouped_sources(channel=None, with_original=True):
    """Die wählbaren Quellen, **nach Sprache gruppiert** — eine Liste je Sprache.

    ⚠⚠ **Die eine Stelle, die die Reihenfolge festlegt** — für den Reiter
    „Übersetzung" und den Einrichtungsassistenten. Getrennte Listen laufen
    still auseinander: Eine neue Quelle erschiene dann nur an einer Stelle,
    und niemandem fiele es auf, weil nichts kaputtgeht.

    Wer eine Quelle ergänzt, ergänzt **eine** Zeile in `SOURCES`. Beide
    Ansichten ziehen von hier.

    Reihenfolge: erst Deutsch, dann Englisch, danach die übrigen Sprachen
    alphabetisch. Innerhalb einer Sprache: erst Luftwerft, dann rjcncpt und
    StarStrings, danach die übrigen in der Reihenfolge aus `SOURCES`.
    """
    by_language = {}
    for key, spec in SOURCES.items():
        if not available(key, channel):
            continue
        by_language.setdefault(spec.get('sprache') or '', []).append(key)
    if with_original:
        # Das Originalenglisch aus dem Spiel gehört zu den englischen.
        by_language.setdefault('english', []).append('original')

    def rank(language):
        if language.startswith('german'):
            return (0, language)
        if language.startswith('english'):
            return (1, language)
        return (2, language)

    def source_rank(source):
        if source.startswith('luftwerft_'):
            return 0
        if source in ('deutsch', 'starstrings'):
            return 1
        return 2

    return [sorted(by_language[language], key=source_rank)
            for language in sorted(by_language, key=rank)]


def display_name(source):
    """Der Name einer Quelle, wie der Spieler ihn liest."""
    if source == CUSTOM:
        return t('s_sp_q_eigen')
    if source == 'original':
        return t('s_sp_q_or')
    q = SOURCES.get(source) or {}
    return t(q['label']) if q.get('label') else q.get('name', source)


def channel_sources():
    """{Kanal: Quelle} der Nebenkanäle (PTU …), wie gewählt."""
    value = paths.settings().get(SETTING_CHANNELS)
    return dict(value) if isinstance(value, dict) else {}


def set_channel_source(channel, source):
    value = channel_sources()
    if source:
        value[channel] = source
    else:
        value.pop(channel, None)
    paths.set_setting(SETTING_CHANNELS, value)


def _custom_latest(spec):
    """Kennung der Datei hinter der eigenen Adresse — ohne sie zu laden.

    Gefragt wird nur nach dem Kopf (`HEAD`): ETag, Änderungsdatum oder Größe.
    Liefert der Server nichts davon, gibt es keine Kennung — dann holt der
    Hintergrund nicht alle sechs Stunden 16 MB auf Verdacht."""
    from .catalog import OFF
    if OFF:
        raise OSError('Netzabrufe sind abgeschaltet (SC_BP_NO_NET)')
    req = urllib.request.Request(spec['url'], method='HEAD',
                                 headers={'User-Agent': USER_AGENT})
    with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
        head = r.headers
    for field, prefix in (('ETag', 'etag'), ('Last-Modified', 'datum'),
                          ('Content-Length', 'groesse')):
        value = (head.get(field) or '').strip().strip('"')
        if value:
            return '%s:%s' % (prefix, value)
    return None


def _direct(source, spec):
    """Liegt die Datei direkt unter einer Adresse statt in einem GitHub-Repo?
    (eigene Adresse, Luftwerft)"""
    return source == CUSTOM or (bool(spec.get('url')) and not spec.get('repo'))


def latest(source, channel=None):
    """Die neueste Version einer Quelle: (Kennung, Adresse, Größe) oder None.

    Die Kennung ist der Release-Tag. Bei StarStrings heißt der Tag immer
    `latest` — dort taugt er nicht zum Vergleichen, deshalb wird zusätzlich
    das Veröffentlichungsdatum genommen."""
    q = _spec(source, channel)
    if not q:
        last_error[0] = t('m_keine_kanalfassung') % (channel or 'LIVE')
        return None
    if _direct(source, q):
        try:
            ident = _custom_latest(q)
            last_error[0] = None
        except Exception as e:
            _net_error(e)
            return None
        return ident, q['url'], 0
    if q.get('repo_datei'):
        # Kennung = der letzte Commit, der genau diese Datei geändert hat.
        # Die Adresse zeigt auf DIESEN Commit, nicht auf `main` — sonst könnte
        # zwischen Nachsehen und Laden eine andere Fassung kommen, als vermerkt.
        try:
            commits = _fetch('https://api.github.com/repos/%s/commits?path=%s'
                             '&per_page=1' % (q['repo'], q['repo_datei']))
        except Exception as e:
            # ⚠⚠ **Kein Rückfall aufs Release, wenn die Abfrage scheitert.**
            # Das Release trägt eine andere Kennung als die eingesetzte
            # Repo-Datei — ein abgewiesener Abruf (GitHub-Limit) gälte sonst
            # als neue Fassung und schriebe die veraltete Datei aus dem
            # Release über die aktuelle.
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
        # Ein fester Tag (StarStrings-PTU hängt an `latest-ptu`, einer
        # Vorabversion) — `releases/latest` liefert Vorabversionen nie.
        if q.get('tag'):
            r = _fetch('https://api.github.com/repos/%s/releases/tags/%s'
                       % (q['repo'], q['tag']))
        else:
            r = _fetch('https://api.github.com/repos/%s/releases/latest'
                       % q['repo'])
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
    # `latest-ptu` genauso: ein fester Name, der nie wechselt.
    if ident.lower() in ('latest', '') or ident.lower().startswith('latest-'):
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


def _note_set(source, ident, language_folder=None):
    """`source` ist der Vermerk-Schlüssel (`_key`), nicht nur der Name."""
    d = _note()
    entry = {'kennung': ident, 'stand': time.strftime('%Y-%m-%d %H:%M')}
    if language_folder:
        entry['sprache'] = language_folder
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
    neueste, wird ihr Datum nachgetragen — Vermerke ohne Datum bekommen es
    so nachträglich."""
    d = _note()
    entry = d.get(source)
    if not isinstance(entry, dict):
        return
    entry['geprueft'] = time.strftime('%Y-%m-%d %H:%M')
    entry['pruefung'] = result
    if result == 'aktuell' and _version_dates.get(ident):
        entry['datum'] = _version_dates[ident]
    _note_write(d)


def info(source, channel=None):
    """Der Vermerk einer Quelle als dict (leer, wenn keiner da ist)."""
    entry = _note().get(_key(source, channel))
    return dict(entry) if isinstance(entry, dict) else {}


def _day(stamp, lang=None):
    """`JJJJ-MM-TT` → `TT.MM.JJJJ` (deutsch) bzw. unverändert (englisch)."""
    from . import language
    lang = lang or language.current()
    try:
        y, m, d = stamp[:10].split('-')
    except (ValueError, TypeError, AttributeError):
        return ''
    return '%s.%s.%s' % (d, m, y) if lang == 'de' else '%s-%s-%s' % (y, m, d)


def status_text(source, lang=None, today=None, channel=None):
    """Für Spieler lesbar: Stand der Übersetzung und ob sie aktuell ist.

    Beispiel: `Stand TT.MM.JJJJ · aktuell, nachgesehen heute 04:52`.
    ⚠ Nie die Kennung (`git-082b11db5e73`) — daran erkennt niemand etwas."""
    entry = info(source, channel)
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

    Auch für den Weg mit den Originaltexten aus dem Spiel nötig, obwohl dort nichts
    heruntergeladen wird: Ohne Vermerk weiß der Watcher beim nächsten Start
    nicht, dass der Spieler die Bauplan-Angaben überhaupt eingerichtet hat —
    und würde sie nach einem Spiel-Patch nicht wieder eintragen."""
    _note_set(source, ident)


def installed(source, channel=None):
    """Welche Version liegt hier? Kennung oder None."""
    return (_note().get(_key(source, channel)) or {}).get('kennung')


def update_available(source, channel=None):
    """(True, neue_Kennung), wenn es etwas Neueres gibt. Wirft nie."""
    try:
        fresh = latest(source, channel)
    except Exception as e:
        _net_error(e)
        fresh = None
    key = _key(source, channel)
    # ⚠ Die eigene Adresse ohne Kennung (der Server nennt weder ETag noch
    # Datum noch Größe): Dann gibt es nichts zu vergleichen — nicht jedes Mal
    # neu laden, sondern als nicht geprüft melden.
    if not fresh or not fresh[0]:
        _record_check(key, 'fehler')
        return False, None
    newer = fresh[0] != installed(source, channel)
    _record_check(key, 'neu' if newer else 'aktuell', fresh[0])
    return newer, fresh[0]


# ------------------------------------------------------------ Installieren
def _ini_from_zip(content, sprache, part=None):
    """Die `global.ini` aus dem Archiv holen — egal wie der Ordner geschrieben ist.

    StarStrings packt nach `Data/…`, die deutsche Übersetzung nach `data/…`.
    Unter Windows ist das dasselbe, **unter Linux nicht** — dort wäre ein
    falsch geschriebener Ordner schlicht unsichtbar für das Spiel. Deshalb wird
    hier nur auf den Dateinamen geachtet und der Zielpfad später selbst gebaut.

    `part` beschränkt auf einen Ordner im Archiv (Thord82 packt `LIVE/…` und
    `PTU/…` in dieselbe Zip). Gibt (Inhalt, Sprachordner laut Archiv) zurück;
    der Sprachordner ist None, wenn die Datei ohne Ordner im Archiv liegt
    (Dymerz)."""
    with zipfile.ZipFile(io.BytesIO(content)) as z:
        names = [n for n in z.namelist()
                 if n.replace('\\', '/').lower().split('/')[-1] == 'global.ini']
        if part:
            names = [n for n in names
                     if n.replace('\\', '/').lower().startswith(part.lower())]
        for name in names:
            parts = name.replace('\\', '/').lower().split('/')
            if sprache and sprache.lower() in parts:
                return z.read(name), sprache
        if len(names) == 1:
            parts = names[0].replace('\\', '/').split('/')
            found = None
            lowered = [x.lower() for x in parts]
            if 'localization' in lowered:
                i = lowered.index('localization')
                if i + 1 < len(parts) - 1:
                    found = parts[i + 1].lower()
            return z.read(names[0]), found
    return None, None


def looks_like_global_ini(content):
    """Ist das wirklich eine Textdatei des Spiels? (für die eigene Adresse)

    ⚠ Wer eine Adresse einträgt, kann sich vertun — eine Webseite statt der
    Datei, eine falsche Datei. Die landet sonst im Spiel und das Spiel zeigt
    nur noch Schlüssel. Geprüft wird, was jede echte `global.ini` hat: viele
    `schlüssel=text`-Zeilen und die Fahrzeugnamen."""
    if not content or len(content) > CUSTOM_MAX_BYTES:
        return False
    head = content[:20000000]
    if b'<html' in head[:2000].lower():
        return False
    return b'vehicle_' in head and head.count(b'=') > 5000


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

    ⚠⚠ **Warum es das braucht.** Das Werkzeug **schreibt** `g_language`
    (`user_cfg_setzen`). Ohne die Zeile zu lesen, nähme `injection.ini_file()`
    bei der Textquelle „Original" eine feste Reihenfolge — erst `english`,
    dann `german_(germany)` — und beide Dateien gibt es fast immer. Dann
    landet die Auszeichnung in der englischen, während das Spiel die deutsche
    liest: Eingetragen korrekt, angekommen nichts, und die Statuszeile meldet
    trotzdem Erfolg.
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


def fetch(source, progress=None, game_dir=None, channel=None):
    """Eine Quelle herunterladen und einsetzen. Gibt (Erfolg, Meldung) zurück.

    `channel` und `game_dir` gehören zusammen: ein Nebenkanal (PTU …) und sein
    Ordner. Ohne beides: die Hauptinstallation."""
    def report(text):
        if progress:
            progress(text)

    q = _spec(source, channel)
    if not q:
        return False, t('m_keine_kanalfassung') % (channel or 'LIVE')
    fresh = latest(source, channel)
    if not fresh:
        return False, last_error[0] or t('m_keine_fassung')
    ident, address, byte_size = fresh

    report(t('z_laedt') % (q['name'], byte_size / 1048576.0))
    try:
        content = _fetch(address, raw=True)
    except Exception as e:
        return False, 'Download fehlgeschlagen: %s' % e

    sprache = q['sprache']
    # Die Repo-Datei kommt als blanke `global.ini`, das Release als Archiv.
    if content[:2] == b'PK':
        try:
            ini, from_archive = _ini_from_zip(content, sprache, q.get('teil'))
        except zipfile.BadZipFile:
            ini, from_archive = None, None
        # Bei der eigenen Adresse sagt das Archiv selbst, welche Sprache es
        # ist (`Data/Localization/french_(france)/global.ini`).
        if source == CUSTOM and from_archive:
            sprache = from_archive
    else:
        ini = content if b'\nvehicle_' in content[:20000000] else None
    if _direct(source, q) and not (ini and looks_like_global_ini(ini)):
        return False, t('m_keine_spieldatei')
    if not ini:
        return False, t('m_keine_ini_archiv')
    if _direct(source, q) and not ident:
        # Der Server nennt keine Kennung — dann ist der Inhalt die Kennung.
        import hashlib
        ident = 'inhalt:%s' % hashlib.sha256(ini).hexdigest()[:16]

    target = target_ini(sprache, game_dir)
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

    set_user_cfg(sprache, q.get('ton'), game_dir)
    _note_set(_key(source, channel), ident, sprache)
    if channel:
        # Ein Nebenkanal bekommt keine Bauplan-Angaben — die gemerkten
        # Originaltexte gehören zur Hauptinstallation und bleiben.
        return True, '%s (%s), %.1f MB' % (q['name'], channel,
                                          len(ini) / 1048576.0)
    # ⚠ Hier liegt jetzt eine **fremde, unberührte** Datei. Die gemerkten
    # Originaltexte gehören zur alten und würden auf einen überholten Stand
    # zurückschreiben; zugleich wird vermerkt, dass in dieser Datei noch nie
    # injiziert wurde. Ohne diesen Vermerk schneidet der Formen-Notnagel beim
    # ersten Lauf fremde Kennzeichnungen heraus — bei StarStrings 17 Stück,
    # und wegen des dann falsch gemerkten „Urtextes" für immer.
    from . import injection
    injection.discard_origtext()
    return True, '%s (%s), %.1f MB' % (q['name'], ident, len(ini) / 1048576.0)
