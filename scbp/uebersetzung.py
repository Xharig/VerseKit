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

MERKDATEI = 'uebersetzung.json'
USER_AGENT = 'SC-BP-Watcher (+https://github.com/Xharig/SC-BP-Watcher)'
ZEITLIMIT = 60

# Die Fremdquellen. `sprache` ist der Ordnername, unter dem Star Citizen die
# Datei erwartet — er entscheidet zugleich, was in die `user.cfg` muss.
QUELLEN = {
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


def _hole(url, roh=False):
    # ⚠ `SC_BP_NO_NET` gilt hier genauso. Die Anleitung verspricht, dass sich
    # die Netzabrufe abschalten lassen — bis rc42 galt das für den Katalog,
    # die Preise, die Orte, den Serverstatus und die Update-Frage, aber nicht
    # für die Übersetzungsquellen und die Auftragsdaten. Ein Versprechen, das
    # nur zum Teil eingehalten wird, ist keines.
    from .catalog import OFF
    if OFF:
        raise OSError('Netzabrufe sind abgeschaltet (SC_BP_NO_NET)')
    req = urllib.request.Request(url, headers={'User-Agent': USER_AGENT})
    with urllib.request.urlopen(req, timeout=ZEITLIMIT) as r:
        daten = r.read()
    return daten if roh else json.loads(daten.decode('utf-8'))


# --------------------------------------------------------------- Was ist neu?
# Der zuletzt aufgetretene Netzfehler — im Klartext, für die Anzeige.
# Ohne diese Zeile stand bei einem Zertifikatsproblem nur „Version nicht
# gefunden" im Fenster, was nach „das Release existiert nicht" aussieht und in
# die völlig falsche Richtung führt. Die Diagnose kostete am 24.08.2026 eine
# halbe Stunde, obwohl die Ausnahme den Grund kannte.
letzter_fehler = [None]


def neueste(quelle):
    """Die neueste Version einer Quelle: (Kennung, Adresse, Größe) oder None.

    Die Kennung ist der Release-Tag. Bei StarStrings heißt der Tag immer
    `latest` — dort taugt er nicht zum Vergleichen, deshalb wird zusätzlich
    das Veröffentlichungsdatum genommen."""
    q = QUELLEN.get(quelle)
    if not q:
        return None
    try:
        r = _hole('https://api.github.com/repos/%s/releases/latest' % q['repo'])
        letzter_fehler[0] = None
    except Exception as e:
        # Zertifikatsfehler eigens benennen — die Meldung von OpenSSL ist für
        # Nichttechniker unlesbar, die Ursache aber immer dieselbe.
        text = str(e)
        if 'CERTIFICATE' in text.upper() or 'SSL' in text.upper():
            letzter_fehler[0] = t('m_kein_zertifikat')
        elif getattr(e, 'code', None) == 403 or '403' in text:
            # ⚠ Auch hier gilt: 403 ist eine Absage, kein Netzfehler. Bei
            # GitHub ist es meist das Abruflimit, bei Cloudflare-Seiten der
            # Bot-Schutz. Ohne eigene Meldung sucht man beim eigenen Anschluss.
            letzter_fehler[0] = t('m_abgewiesen')
        else:
            letzter_fehler[0] = text
        return None
    kennung = r.get('tag_name') or ''
    if kennung.lower() in ('latest', ''):
        kennung = (r.get('published_at') or '')[:19]
    for a in r.get('assets') or []:
        if a.get('name') == q['datei']:
            return kennung, a.get('browser_download_url'), a.get('size') or 0
    return None


def _merk():
    try:
        with open(pfade.app_datei(MERKDATEI), encoding='utf-8') as f:
            d = json.load(f)
        return d if isinstance(d, dict) else {}
    except Exception:
        return {}


def _merk_setzen(quelle, kennung):
    d = _merk()
    d[quelle] = {'kennung': kennung, 'stand': time.strftime('%Y-%m-%d %H:%M')}
    ziel = pfade.app_datei(MERKDATEI)
    try:
        with open(ziel + '.tmp', 'w', encoding='utf-8') as f:
            json.dump(d, f, ensure_ascii=False, indent=1)
        os.replace(ziel + '.tmp', ziel)
    except OSError:
        pass


def vermerken(quelle, kennung):
    """Eine Quelle als eingerichtet festhalten.

    Auch für den Weg „Originaltexte aus dem Spiel" nötig, obwohl dort nichts
    heruntergeladen wird: Ohne Vermerk weiß der Watcher beim nächsten Start
    nicht, dass der Spieler die Bauplan-Angaben überhaupt eingerichtet hat —
    und würde sie nach einem Spiel-Patch nicht wieder eintragen."""
    _merk_setzen(quelle, kennung)


def installiert(quelle):
    """Welche Version liegt hier? Kennung oder None."""
    return (_merk().get(quelle) or {}).get('kennung')


def update_da(quelle):
    """(True, neue_Kennung), wenn es etwas Neueres gibt. Wirft nie."""
    neu = neueste(quelle)
    if not neu:
        return False, None
    return (neu[0] != installiert(quelle)), neu[0]


# ------------------------------------------------------------ Installieren
def _ini_aus_zip(inhalt, sprache):
    """Die `global.ini` aus dem Archiv holen — egal wie der Ordner geschrieben ist.

    StarStrings packt nach `Data/…`, die deutsche Übersetzung nach `data/…`.
    Unter Windows ist das dasselbe, **unter Linux nicht** — dort wäre ein
    falsch geschriebener Ordner schlicht unsichtbar für das Spiel. Deshalb wird
    hier nur auf den Dateinamen geachtet und der Zielpfad später selbst gebaut."""
    with zipfile.ZipFile(io.BytesIO(inhalt)) as z:
        for name in z.namelist():
            teile = name.replace('\\', '/').lower().split('/')
            if teile[-1] == 'global.ini' and sprache.lower() in teile:
                return z.read(name)
        for name in z.namelist():          # Rückfall: die einzige global.ini
            if name.replace('\\', '/').lower().endswith('/global.ini'):
                return z.read(name)
    return None


def ziel_ini(sprache, spielordner=None):
    """Wohin die Datei gehört. Ein vorhandener `Data`-Ordner wird beibehalten,
    sonst wird `data` angelegt — Linux unterscheidet die beiden."""
    wurzel = spielordner or pfade.spiel_ordner()
    if not wurzel:
        return None
    for schreibweise in ('data', 'Data'):
        p = os.path.join(wurzel, schreibweise)
        if os.path.isdir(p):
            return os.path.join(p, 'Localization', sprache, 'global.ini')
    return os.path.join(wurzel, 'data', 'Localization', sprache, 'global.ini')


def spielsprache(spielordner=None):
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
    wurzel = spielordner or pfade.spiel_ordner()
    if not wurzel:
        return None
    pfad = os.path.join(wurzel, 'user.cfg')
    try:
        with open(pfad, encoding='utf-8', errors='ignore') as f:
            zeilen = f.read().splitlines()
    except OSError:
        return None
    for z in zeilen:
        if z.split('=', 1)[0].strip() != 'g_language':
            continue
        wert = z.split('=', 1)[1].strip() if '=' in z else ''
        # Kommentare hinter dem Wert abschneiden — die `user.cfg` erlaubt sie.
        wert = wert.split(';', 1)[0].split('--', 1)[0].strip().strip('"\'')
        if wert:
            return wert
    return None


def user_cfg_setzen(sprache, ton=None, spielordner=None):
    """`g_language` in der `user.cfg` setzen — **ergänzend**, nicht ersetzend.

    In dieser Datei stehen die Grafikeinstellungen des Spielers. Sie zu
    überschreiben, weil man eine Zeile ändern will, wäre ein handfester
    Schaden — deshalb wird zeilenweise gelesen und nur die betroffene Zeile
    ausgetauscht."""
    wurzel = spielordner or pfade.spiel_ordner()
    if not wurzel:
        return False
    pfad = os.path.join(wurzel, 'user.cfg')
    zeilen = []
    if os.path.isfile(pfad):
        try:
            with open(pfad, encoding='utf-8', errors='ignore') as f:
                zeilen = f.read().splitlines()
        except OSError:
            return False
    gesetzt = {'g_language': False, 'g_languageAudio': ton is None}
    neu = []
    for z in zeilen:
        schluessel = z.split('=', 1)[0].strip()
        if schluessel == 'g_language':
            neu.append('g_language = %s' % sprache)
            gesetzt['g_language'] = True
        elif schluessel == 'g_languageAudio' and ton:
            neu.append('g_languageAudio = %s' % ton)
            gesetzt['g_languageAudio'] = True
        else:
            neu.append(z)
    if not gesetzt['g_language']:
        neu.append('g_language = %s' % sprache)
    if ton and not gesetzt['g_languageAudio']:
        neu.append('g_languageAudio = %s' % ton)
    try:
        with open(pfad + '.tmp', 'w', encoding='utf-8') as f:
            f.write('\n'.join(neu) + '\n')
        os.replace(pfad + '.tmp', pfad)
        return True
    except OSError:
        return False


def holen(quelle, fortschritt=None, spielordner=None):
    """Eine Quelle herunterladen und einsetzen. Gibt (Erfolg, Meldung) zurück."""
    def melde(text):
        if fortschritt:
            fortschritt(text)

    q = QUELLEN.get(quelle)
    if not q:
        return False, 'unbekannte Quelle'
    neu = neueste(quelle)
    if not neu:
        return False, letzter_fehler[0] or t('m_keine_fassung')
    kennung, adresse, groesse = neu

    melde(t('z_laedt') % (q['name'], groesse / 1048576.0))
    try:
        inhalt = _hole(adresse, roh=True)
    except Exception as e:
        return False, 'Download fehlgeschlagen: %s' % e

    ini = _ini_aus_zip(inhalt, q['sprache'])
    if not ini:
        return False, t('m_keine_ini_archiv')

    ziel = ziel_ini(q['sprache'], spielordner)
    if not ziel:
        return False, 'Star-Citizen-Ordner unbekannt'

    melde(t('z_einsetzen'))
    try:
        os.makedirs(os.path.dirname(ziel), exist_ok=True)
        with open(ziel + '.tmp', 'wb') as f:
            f.write(ini)
        os.replace(ziel + '.tmp', ziel)
    except OSError as e:
        return False, 'Schreiben fehlgeschlagen: %s' % e

    user_cfg_setzen(q['sprache'], q['ton'], spielordner)
    _merk_setzen(quelle, kennung)
    # ⚠ Hier liegt jetzt eine **fremde, unberührte** Datei. Die gemerkten
    # Originaltexte gehören zur alten und würden auf einen überholten Stand
    # zurückschreiben; zugleich wird vermerkt, dass in dieser Datei noch nie
    # injiziert wurde. Ohne diesen Vermerk schneidet der Formen-Notnagel beim
    # ersten Lauf fremde Kennzeichnungen heraus — bei StarStrings 17 Stück,
    # und wegen des dann falsch gemerkten „Urtextes" für immer.
    from . import injektion
    injektion.urtext_verwerfen()
    return True, '%s (%s), %.1f MB' % (q['name'], kennung, len(ini) / 1048576.0)
