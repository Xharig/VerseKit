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
Neue Versionen bemerken, nachlesen und holen.

Niemand geht regelmäßig auf GitHub nachsehen, ob es etwas Neues gibt. Also
schaut das Programm selbst nach, sagt Bescheid und holt die neue Version auf
Knopfdruck. Und weil „es gibt eine neue Version" allein nichts wert ist, kann
man **nachlesen, was sich geändert hat** — auch bei älteren Versionen.

Drei Teile:

  **Nachsehen.** Einmal am Tag gegen die GitHub-API, im Hintergrund. Ohne Netz
  passiert nichts und es wird auch nichts gemeldet.
  **Nachlesen.** Das Änderungsprotokoll liegt als `CHANGELOG.en.md` bei; neuere
  Einträge kommen aus den Release-Texten. Beides zusammen ergibt die Historie.
  **Holen.** Die zur eigenen Verpackung passende Datei (`.exe` oder AppImage)
  wird geladen und ersetzt die laufende.

> ⚠️ **Das Ersetzen ist heikel und je System verschieden.** Unter Windows kann
> sich eine laufende `.exe` nicht selbst überschreiben — die neue Datei wird
> daneben abgelegt und ein winziges Hilfsskript tauscht sie nach dem Beenden.
> Ein AppImage darf sich ersetzen, solange man die Datei austauscht statt in sie
> hineinzuschreiben. Wer aus dem Quellcode startet, bekommt keinen Selbstersatz
> angeboten — dort ist `git pull` der richtige Weg, und alles andere würde
> lokale Änderungen überfahren.

Geladen wird ausschließlich von `github.com`; eine Datei von woanders wird
abgelehnt, selbst wenn die API sie nennen würde.
"""
import hashlib
import json
import errno
import os
import re
import sys
import tempfile
import time
import urllib.request

from . import fehler
from . import pfade

REPO = 'Xharig/SC-BP-Watcher'
API = 'https://api.github.com/repos/%s/releases' % REPO
# ⚠ **Kein `/releases/latest` mehr (28.08.2026).** Der Link führte auf
# **v2.0.0**: GitHub blendet dort Vorabversionen aus, und alle rc-Fassungen
# sind welche. Wer ihn weitergab, schickte Leute auf einen Stand von vor
# Monaten — und bekam prompt Fehler gemeldet, die längst behoben waren.
# Die Übersicht zeigt alles, auch die Vorabversionen.
PAGE = 'https://github.com/%s/releases' % REPO
# ⚠ Der User-Agent geht an FREMDE Server (GitHub, scmdb, UEX) — ihre Betreiber
# sehen ihn. Deshalb nennt er nach der Umbenennung (12.09.2026) **beide** Namen:
# Wer den alten in einer Freigabeliste stehen hat, erkennt uns weiter, und wer
# nur den neuen kennt, auch. Die Adresse bleibt ohnehin dieselbe, weil `REPO`
# nicht umbenannt wird.
USER_AGENT = 'VerseKit (ehemals SC-BP-Watcher) (+https://github.com/%s)' % REPO
CACHE = 'versionen.json'
# Wie lange ein Blick auf GitHub gilt. Früher standen hier 24 Stunden — „einmal
# am Tag reicht". Tut es nicht: Wer das Programm mehrmals startet, bekam beim
# zweiten Mal nichts mehr zu sehen, obwohl inzwischen eine neue Version
# vorlag. Gemeldet am 24.08.2026, an einem Tag mit zehn Vorabversionen.
#
# Eine Stunde ist der Kompromiss: Beim Starten wird praktisch immer nachgesehen,
# im Dauerbetrieb bleibt es bei ein paar Abfragen am Tag.
MIN_INTERVAL = 3600
OFF = os.environ.get('SC_BP_NO_NET', '') not in ('', '0')
ALLOWED_HOSTS = ('github.com', 'objects.githubusercontent.com')


# ------------------------------------------------------------ Versionsvergleich
def _parts(version):
    """'v2.0.1-fork.3' -> (2, 0, 1). Vorspann und Zusatz werden ignoriert."""
    m = re.search(r'(\d+)\.(\d+)\.(\d+)', str(version or ''))
    return tuple(int(x) for x in m.groups()) if m else (0, 0, 0)


def _is_prerelease(version):
    """Trägt die Version einen Vorab-Zusatz (-dev, -rc1, -beta, -alpha)?"""
    return bool(re.search(r'-(rc|beta|alpha|dev)', str(version or '')))


def _prerelease_number(version):
    """Die Zahl hinter dem Zusatz: aus '2.0.0-rc2' wird 2, aus '-dev' wird 0."""
    m = re.search(r'-(?:rc|beta|alpha|dev)\.?(\d*)', str(version or ''))
    if not m:
        return 0
    return int(m.group(1)) if m.group(1) else 0


def _version_key(version):
    """Eindeutig **je Fassung** — `-rc1` und `-rc13` sind nicht dasselbe.

    ⚠⚠ **Nicht `_teile` benutzen, wo Fassungen unterschieden werden müssen.**
    `_teile('v3.15.0-rc13')` ergibt `(3, 15, 0)` — genau wie `-rc1`. Im
    Änderungsverlauf galten dadurch alle dreizehn Testfassungen als dieselbe
    Version, und zwölf davon flogen als Doppelung heraus. Am 05.09.2026
    gemeldet: „Dann müsste man auch jeden rc anzeigen, nicht nur den letzten."

    Die fertige Version steht über ihren Vorabfassungen — deshalb `9999`.
    """
    major, minor, patch = _parts(version)
    step = _prerelease_number(version) if _is_prerelease(version) else 9999
    return (major, minor, patch, step)


def is_newer(other, own):
    """Ist `fremd` eine höhere Version als `eigen`?

    Ein `-dev`-Zusatz gilt als **älter** als dieselbe Zahl ohne Zusatz: Wer eine
    Entwicklerfassung von 1.6.0 fährt, soll das fertige 1.6.0 angeboten bekommen."""
    a, b = _parts(other), _parts(own)
    if a != b:
        return a > b
    # Gleiche Zahl: Eine Version mit Zusatz (-dev, -rc1, -beta) gilt als älter
    # als dieselbe Zahl ohne. Wer 2.0.0-rc1 fährt, soll 2.0.0 angeboten bekommen.
    va, vb = _is_prerelease(other), _is_prerelease(own)
    if va != vb:
        return vb           # nur die fertige Version ist neuer
    if not va:
        return False        # beide fertig und gleiche Zahl
    # Beide Vorabversionen: nach der Nummer dahinter (rc1 < rc2 < rc10)
    return _prerelease_number(other) > _prerelease_number(own)


# ------------------------------------------------------------------- Nachsehen
def _fetch(url, timeout=20):
    req = urllib.request.Request(url, headers={
        'User-Agent': USER_AGENT, 'Accept': 'application/vnd.github+json'})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read().decode('utf-8'))


def _cache_read():
    try:
        with open(pfade.app_datei(CACHE), encoding='utf-8') as f:
            return json.load(f)
    except Exception:
        return {}


def _cache_write(data):
    try:
        with open(pfade.app_datei(CACHE), 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False)
    except OSError:
        pass


# Hat der letzte erzwungene Blick zu GitHub geklappt? `None` = noch nicht
# versucht, `False` = Abruf gescheitert (Netz weg, Grenze erreicht).
_FETCH = {'ok': None, 'grenze': False}


def fetch_succeeded():
    """Hat der letzte Blick zu GitHub wirklich stattgefunden?

    ⚠ **Ohne das kann „nichts Neues" zweierlei heißen** — und die zwei sind das
    Gegenteil voneinander: entweder „du bist aktuell" oder „ich konnte gar nicht
    nachsehen". Der Prüfknopf meldete bisher in beiden Fällen Entwarnung.

    Aufgefallen am 27.08.2026: Bomb20 drückte „Auf Aktualität prüfen", bekam „du
    hast die neueste rc67" — und rc68 war seit zwei Minuten draußen. GitHub
    erlaubt anonym nur **60 Abfragen pro Stunde und Adresse**; wer an einem
    Vormittag viel klickt, läuft dagegen. Der Abruf scheiterte, der Code fing das
    still ab und rechnete mit dem alten Stand weiter.

    Ein Prüfknopf, der fälschlich Entwarnung gibt, ist schlimmer als keiner.
    """
    return _FETCH['ok']


def rate_limited():
    """War der letzte Fehlschlag die Stundengrenze von GitHub?"""
    return _FETCH['grenze']


def check(own_version, force=False):
    """Gibt es etwas Neues? Rückgabe: dict mit Angaben oder None.

    Gefragt wird höchstens einmal je `ABSTAND` (eine Stunde); dazwischen gilt
    der gemerkte Stand.

    ⚠ Der Abstand allein macht noch keine Wiederholung: Bis v3.0.1 rief diese
    Funktion **niemand** ein zweites Mal, und ein laufender Watcher erfuhr nie
    von einer neuen Fassung. Wer den Takt ändert, ändert ihn an **zwei** Stellen
    — hier und in `Overlay.VERSION_TAKT`.
    Fehler sind kein Drama — ohne Netz meldet sich das Programm einfach nicht."""
    cached = _cache_read()
    # `SC_BP_NO_NET` verbietet das **Abfragen**, nicht das Wissen: Was schon
    # bekannt ist, darf weiter gemeldet werden — das ist keine Netzverbindung.
    old_enough = time.time() - cached.get('geprueft', 0) > MIN_INTERVAL
    if not OFF and (force or old_enough):
        try:
            # ⚠ **20 reicht längst nicht.** Bei 83 Freigaben und einer
            # Testversion nach der anderen war unter den letzten 20 **keine
            # einzige stabile** — `neueste(False)` fand nichts, und im Kasten
            # „Stabile Version" stand statt eines Knopfes „Erst oben auf ‚Jetzt
            # nachsehen' drücken". Eine Sackgasse: Wer die stabile Version wollte,
            # sah keinen Weg, sondern eine Hausaufgabe. Gemessen am 27.08.2026:
            # 20 Freigaben → 0 stabile, 100 Freigaben → 3.
            #
            # 100 ist das Höchste, was GitHub in einer Abfrage hergibt, und es
            # bleibt **eine** Abfrage — die Stundengrenze zählt Anfragen, nicht
            # Einträge.
            releases = _fetch(API + '?per_page=100')
            cached = {
                'geprueft': time.time(),
                'freigaben': [{
                    'version': f.get('tag_name'),
                    'name': f.get('name'),
                    'datum': (f.get('published_at') or '')[:10],
                    'text': f.get('body') or '',
                    'vorab': bool(f.get('prerelease')),
                    'dateien': [{'name': a.get('name'), 'url':
                                 a.get('browser_download_url'),
                                 'groesse': a.get('size')}
                                for a in (f.get('assets') or [])],
                } for f in releases if not f.get('draft')],
            }
            _cache_write(cached)
            _FETCH['ok'] = True
            _FETCH['grenze'] = False
        except Exception as ausnahme:
            # ⚠ Nicht mehr stillschweigend: Ob der Blick stattgefunden hat, ist
            # eine andere Auskunft als „es gibt nichts Neues". Siehe
            # `abruf_geglueckt()`.
            _FETCH['ok'] = False
            _FETCH['grenze'] = '403' in str(ausnahme) or 'rate limit' in str(
                ausnahme).lower()
            fehler.merken('updater.nachsehen', ausnahme)
            # Der letzte bekannte Stand gilt weiter — ohne Netz ist das besser
            # als gar nichts.

    # Vorabversionen bekommt **niemand ungefragt** angeboten — eine Vorabfassung
    # ist zum Prüfen da, nicht zum Verteilen. Angeboten werden sie in drei Fällen:
    #
    #   1. Der Spieler hat es in den Einstellungen ausdrücklich verlangt
    #      (`vorabversionen`, Standard aus). Das ist der Testkanal: Wer mithelfen
    #      will, bekommt die Versionen vor allen anderen — wer Ruhe will, merkt
    #      von ihnen nichts und bleibt auf den fertigen Versionen.
    #   2. Er fährt selbst schon eine Vorabfassung; dann wäre es unsinnig, ihm
    #      die nächste zu verschweigen.
    #   3. Die fertige Version zur selben Nummer erscheint — die ist ohnehin
    #      "neuer" als jede Vorabfassung (siehe `ist_neuer`), also endet der
    #      Testkanal nie in einer Sackgasse.
    own_is_prerelease = (_is_prerelease(own_version)
                        or pfade.einstellung_wahrheit('vorabversionen', False))
    # ⚠ **Nicht** den ersten Treffer nehmen, sondern den höchsten.
    # GitHub gibt die Freigaben nach Erstellungszeit des Tags zurück, nicht nach
    # Versionsnummer — und das ist nicht dasselbe: In der Liste stand `rc10`
    # hinter `rc9`, weshalb Nutzern die **vorletzte** Version als „neu" gemeldet
    # wurde. Gemeldet am 24.08.2026.
    best = None
    for f in cached.get('freigaben') or []:
        if not f.get('version'):
            continue
        if f.get('vorab') and not own_is_prerelease:
            continue
        if not is_newer(f['version'], own_version):
            continue
        if best is None or is_newer(f['version'], best['version']):
            best = f
    return best


def latest(with_prerelease):
    """Die neueste bekannte Freigabe eines Kanals — unabhängig von der eigenen Version.

    ⚠ Nicht dasselbe wie `nachsehen()`. Das meldet nur, was **neuer** ist als die
    laufende Version — richtig für eine Update-Meldung, unbrauchbar für einen
    Knopf „hol mir die letzte fertige Version". Wer eine Testfassung fährt, will
    ja gerade zurück auf die fertige können.
    """
    best = None
    for f in releases():
        if not f.get('version'):
            continue
        if f.get('vorab') and not with_prerelease:
            continue
        if best is None or is_newer(f['version'], best['version']):
            best = f
    return best


def releases():
    """Alle bekannten Freigaben, neueste zuerst — für das Änderungsprotokoll."""
    return _cache_read().get('freigaben') or []


# --------------------------------------------------------- Änderungsprotokoll
def _changelog_file():
    """Die mitgelieferte Änderungsliste in der Sprache des Nutzers finden.

    Es gibt zwei: `CHANGELOG.en.md` (englisch) und `CHANGELOG.md` (deutsch).
    Wer die Oberfläche auf Deutsch stehen hat, soll auch die Einträge auf
    Deutsch lesen — sonst wäre die Zweisprachigkeit an der Stelle nur behauptet.
    Fehlt die eigene Sprache, gilt die andere: eine fremdsprachige Auskunft ist
    besser als gar keine."""
    from . import sprache
    names = (['CHANGELOG.md', 'CHANGELOG.en.md'] if sprache.aktuelle() == 'de'
             else ['CHANGELOG.en.md', 'CHANGELOG.md'])
    folder = []
    if getattr(sys, 'frozen', False):        # PyInstaller legt Beigaben hierhin
        folder.append(getattr(sys, '_MEIPASS', ''))
        folder.append(os.path.dirname(sys.executable))
    folder.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    for name in names:
        for folder_path in folder:
            if not folder_path:
                continue
            full_path = os.path.join(folder_path, name)
            if os.path.isfile(full_path):
                return full_path
    return None


# Welche Überschrift im CHANGELOG bedeutet was. Die Zuordnung ist bewusst
# großzügig — englische wie deutsche Version, und wer eine neue Überschrift
# erfindet, landet unter „neu" statt im Nichts.
_KINDS = (
    ('fix',  ('behoben', 'fixed', 'korrigiert', 'bugfix')),
    ('bess', ('geändert', 'changed', 'verbessert', 'improved', 'entfernt',
              'removed', 'bedienung')),
    ('neu',  ('hinzugefügt', 'added', 'neu', 'new')),
)


def intro(text):
    """Der Satz, mit dem eine Version vorgestellt wird — das Zitat im Changelog.

    Im Markdown steht er als `> …`-Block über den Aufzählungen: „Ein Fenster für
    alles." Er sagt in einem Satz, worum es in der Version ging, und war bisher
    nirgends zu sehen — `punkte_nach_art` wirft alles weg, was keine Aufzählung
    ist. Genau dieser Satz gehört aber unter die Version, wenn man sie aufklappt.
    """
    lines = []
    for line in (text or '').split('\n'):
        stripped = line.strip()
        if stripped.startswith('>'):
            lines.append(stripped.lstrip('>').strip())
        elif lines and not stripped:
            break                      # der Block ist zu Ende
        elif lines:
            break
    sentence = ' '.join(z for z in lines if z)
    # Die Auszeichnungen sind im Fenster nur Zeichen — sie stören mehr, als sie
    # helfen.
    return sentence.replace('**', '').replace('`', '').strip()


def points_by_kind(text):
    """Zerlegt einen Änderungstext in (art, zeile) — für Filter und Marken.

    Der Text ist Markdown mit Zwischenüberschriften (`### Behoben`). Alles
    darunter gilt als diese Art, bis die nächste Überschrift kommt. Ohne
    Überschrift gilt „neu": Lieber falsch einsortiert als unsichtbar.
    """
    kind = 'neu'
    out_list = []
    in_block = False
    for line in (text or '').split('\n'):
        # ⚠ Eingezäunte Codeblöcke (```) überspringen. Sie zeigen im
        # Änderungsprotokoll, wie etwas auf dem Bildschirm aussieht — als
        # Fortsetzungszeile an einen Punkt geklebt ergibt das Kauderwelsch,
        # und die Zaunzeichen selbst standen bis rc42 sichtbar im Fenster.
        if line.strip().startswith('```'):
            in_block = not in_block
            continue
        if in_block:
            continue
        # ⚠ Die Einrückung muss VOR dem Abschneiden geprüft werden — sonst sind
        # Unterpunkte nicht mehr von Hauptpunkten zu unterscheiden.
        indented = line[:1].isspace()
        stripped = line.strip()
        if stripped.startswith('#'):
            # ⚠ Hiess bis zum 12.09.2026 `klein` — derselbe Name, den zwei
            # andere Funktionen fuer die **Nebenversionsnummer** benutzen.
            # Eine maschinelle Umbenennung haette daraus `minor` gemacht.
            # Mehrdeutigkeit gehoert VOR dem Umbenennen aufgeloest.
            kleinschrift = stripped.lstrip('#').strip().lower()
            for tag, words in _KINDS:
                if any(w in kleinschrift for w in words):
                    kind = tag
                    break
            continue
        if not indented and stripped.startswith(('- ', '* ')):
            out_list.append([kind, stripped[2:].strip()])
        elif indented and stripped and not stripped.startswith(('- ', '* ')) and out_list:
            # Eine eingerückte Zeile **ohne** Aufzählungszeichen ist die
            # Fortsetzung des Punktes darüber — im Markdown umgebrochen, im
            # Fenster gehört sie an denselben Satz. Wer sie verwirft, zeigt
            # abgeschnittene Sätze („… ganz unten") und merkt es nicht, weil
            # es wie ein Zeilenumbruch aussieht.
            out_list[-1][1] = (out_list[-1][1] + ' ' + stripped).strip()
    return [(a, z) for a, z in out_list]


def history():
    """Die Versionsgeschichte als Liste, neueste zuerst.

    Zusammengesetzt aus zwei Quellen: der **mitgelieferten** `CHANGELOG.md`
    bzw. `CHANGELOG.en.md` — je nach eingestellter Sprache — und den Release-Texten
    von GitHub, die auch Versionen kennen, die neuer sind als die eigene.

    ⚠ Der mitgelieferte Changelog hat Vorrang, **weil nur er die Sprache kennt**.
    Der Release-Text auf GitHub ist bewusst zweisprachig aufgebaut: Englisch oben,
    Deutsch in einem aufklappbaren Block darunter. Auf der Release-Seite ist das
    richtig — im Fenster wurde daraus eine englische Liste für jemanden, der die
    Oberfläche auf Deutsch stehen hat. Genau so gemeldet.

    GitHub springt nur dort ein, wo der Changelog nichts hat: bei Versionen, die
    neuer sind als die eigene.
    """
    entries, seen = [], {}
    asset = _changelog_file()
    if asset:
        try:
            with open(asset, encoding='utf-8') as f:
                raw = f.read()
        except OSError:
            raw = ''
        for block in re.split(r'^## ', raw, flags=re.M)[1:]:
            heading, _, rest = block.partition('\n')
            version = heading.split('—')[0].split(' - ')[0].strip()
            if _parts(version) == (0, 0, 0):
                continue        # „Unveröffentlicht" ist nichts für Nutzer
            # ⚠ Je **Fassung**, nicht je Versionsnummer — sonst gilt rc1 als
            # dasselbe wie rc13. Siehe `_fassungsschluessel`.
            key = _version_key(version)
            date = ''
            m = re.search(r'(\d{4}-\d{2}-\d{2})', heading)
            if m:
                date = m.group(1)
            if key in seen:
                continue
            entry = {'version': version, 'datum': date,
                       'text': rest.strip(), 'quelle': 'changelog'}
            seen[key] = entry
            entries.append(entry)

    # Und nun alles, was der mitgelieferte Changelog noch nicht kennt — das sind
    # die Versionen, die nach dieser hier erschienen sind.
    for f in releases():
        key = _version_key(f.get('version'))
        if key in seen or not f.get('version'):
            continue
        entry = {'version': f.get('version'),
                   'datum': f.get('datum') or '',
                   'text': (f.get('text') or '').strip(),
                   'quelle': 'github'}
        seen[key] = entry
        entries.append(entry)

    # ⚠ Nach der **Fassung** sortieren, sonst stünden rc1 bis rc13 in
    # zufälliger Reihenfolge — sie tragen alle dasselbe Datum.
    entries.sort(key=lambda e: (_version_key(e['version']),
                                  e['datum']), reverse=True)
    return entries


def history_grouped():
    """Dasselbe wie `protokoll()`, aber Patch-Versionen unter ihrer Reihe.

    ⭐⭐ **Aus v3.13.0, .1, .2 und .3 wird ein Eintrag „v3.13".** Am 05.09.2026
    gefragt: „Ist es nicht sinnvoller, 3.13.x, 3.14.x zusammenzufassen?" Ja —
    die vier standen alle vom selben Tag untereinander, zusammen neun Punkte.
    Vier Zeilen für das, was eine Sache ist, ist Buchführung, kein
    Änderungsprotokoll; und niemand denkt in „3.13.2", sondern in „3.13".

    ⚠ **Vorabversionen bleiben einzeln.** Wer eine Testfassung fährt, will
    sehen, was **diese** gebracht hat — gebündelt stünden dreizehn rc als ein
    Klumpen da, und die Rückmeldung „was ist seit gestern anders?" wäre nicht
    mehr zu beantworten. Sobald die fertige Version erscheint, verschwinden
    sie ohnehin aus der Liste.

    ⚠ Der Vorspann kommt von der **neuesten** Fassung der Reihe; die Punkte
    aller Fassungen stehen darunter, in derselben Reihenfolge wie bisher.
    """
    all_items = history()
    # ⚠⚠ **Eine Testfassung verschwindet, sobald ihre fertige Version da ist.**
    # Sonst stünden 91 rc-Blöcke im Verlauf und verdrängten alles andere. Am
    # 05.09.2026: „Sobald es live ist, kommen die eh weg." Genau so — und
    # solange es die fertige noch nicht gibt, ist jede einzelne sichtbar, denn
    # dann testet gerade jemand und will wissen, was seine Fassung gebracht hat.
    final_versions = set(_parts(e['version']) for e in all_items
                    if not _is_prerelease(e.get('version') or ''))
    out, by_series = [], {}
    for e in all_items:
        version = e.get('version') or ''
        if _is_prerelease(version):
            if _parts(version) in final_versions:
                continue
            out.append(e)
            continue
        major, minor, _patch = _parts(version)
        series = (major, minor)
        present = by_series.get(series)
        if present is None:
            grouped = dict(e)
            grouped['version'] = 'v%d.%d' % (major, minor)
            by_series[series] = grouped
            out.append(grouped)
            continue
        # Ältere Fassung derselben Reihe: nur ihre Punkte anhängen. Vorspann
        # und Datum bleiben die der neuesten — `protokoll()` liefert absteigend.
        present['text'] = (present.get('text') or '') + '\n\n' \
            + (e.get('text') or '')
    return out


# ------------------------------------------------------------------- Holen
# ⚠⚠ Welche Dateien uns gehoeren, steht in `pfade.EIGENE_DATEINAMEN` —
# an EINER Stelle fuer alle drei Pruefungen (hier zweimal, dazu
# `update_run._aufraeumen()`). Zwei Listen waeren nach dem ersten
# Namenswechsel auseinander.


def own_appimage():
    """Der Pfad **unseres** AppImage — oder None.

    ⚠ `APPIMAGE` allein genügt nicht. Die Variable steht in der Umgebung
    **jedes** Programms, das aus einem AppImage heraus gestartet wurde — auch in
    einem Terminal, das man daraus öffnet, und in allem, was von dort aus läuft.
    Wer nur auf sie schaut, hält jedes beliebige Programm für sich selbst.

    Das ist am 25.08.2026 teuer geworden: Ein Testlauf des Selbst-Updates lief in
    einer Umgebung, in der `APPIMAGE` auf eine **fremde** Anwendung zeigte — und
    das Update hat prompt diese fremde Datei überschrieben (234 MB durch 12 MB
    ersetzt). Zurückzuholen war sie nur, weil das fremde Programm noch lief und
    die alte Inode über `/proc/<pid>/exe` offen hielt.

    Verlässlich ist erst der zweite Teil: Zu einem AppImage gehört `APPDIR`, der
    Ort, an dem es entpackt eingehängt ist. Nur wenn **unser eigener Code** von
    dort kommt, laufen wir wirklich in diesem AppImage.
    """
    path = os.environ.get('APPIMAGE')
    if not path or not os.path.isfile(path):
        return None
    # ⚠ Der erste Anlauf verglich den eigenen Code mit `APPDIR`. Das ging schief:
    # PyInstaller entpackt sich in ein **eigenes** Verzeichnis (`sys._MEIPASS`,
    # etwa `/tmp/_MEIabc123`), nicht in den AppImage-Einhängepunkt. Der Vergleich
    # schlug also **immer** fehl — das Programm hielt sich für eine `.exe`, ging in
    # den Windows-Zweig und meldete „[Errno 2] No such file or directory: 'cmd'".
    #
    # Maßgeblich ist stattdessen der Dateiname: Zeigt `APPIMAGE` auf eine Datei,
    # die nach diesem Programm heißt, ist es unsere. Ein fremdes AppImage — der
    # Unfall, um den es hier geht — heißt anders und fällt durch.
    #
    # ⚠⚠ **BEIDE Namen, dauerhaft** (Umbenennung zu VerseKit, 12.09.2026).
    # Zwei Fälle, die gleichzeitig gelten:
    #   * Bestandsnutzer: Beim Update wird die VORHANDENE Datei an ihrem Platz
    #     ersetzt. Sie heißt danach weiter `SC-BP-Watcher-x86_64.AppImage` und
    #     enthält VerseKit.
    #   * Neuinstallationen: Die Release-Datei heißt `VerseKit-x86_64.AppImage`.
    # Wer hier einen der beiden Namen streicht, sorgt dafür, dass sich die eine
    # Hälfte der Nutzer nicht mehr selbst erkennt: `verpackung()` fällt auf
    # `'exe'` zurück, das Programm geht in den Windows-Zweig und stirbt mit
    # `[Errno 2] No such file or directory: 'cmd'`.
    if not pfade.gehoert_uns(path):
        return None
    return path


def packaging():
    """Wie läuft dieses Programm gerade? 'exe', 'appimage' oder 'quellcode'."""
    if own_appimage():
        return 'appimage'
    if getattr(sys, 'frozen', False):
        return 'exe'
    return 'quellcode'


# Welcher Anhang unter Windows geholt wird — und warum es der Installer ist.
#
# Seit v3.0.0 hängen nur noch **zwei** Dateien an einer Freigabe:
#
#     SC-BP-Watcher-Setup.exe          der Installer  ← der einzige Windows-Weg
#     SC-BP-Watcher-x86_64.AppImage    Linux
#
# ⚠ Die nackte `SC-BP-Watcher.exe` ist bewusst weg (bewusste Entscheidung,
# 27.08.2026: „ich will die exe ohne install loswerden … sie belastet mich
# nur"). Sie war eine Maßnahme aus der Anfangszeit — ein unsigniertes Programm
# ohne Installer wirkt harmloser, und es ging darum, Vertrauen aufzubauen. Das
# ist erreicht; zwei Auslieferungswege heißen ab jetzt nur noch zwei
# Fehlerquellen und doppelte Unterstützung. „Nun wollen wir es funktionierend
# und einfach."
#
# **Und v2.0.0, die es nur als nackte .exe gab?** Deren Update-Logik nimmt die
# erste Datei auf `.exe` — jetzt also den Installer — und ihr Hilfsskript
# **startet** die getauschte Datei anschließend (`start "" "<ziel>"`). Der
# Installer läuft damit von selbst und richtet alles ordentlich ein. Was früher
# der Fehler war (der Installer landete unter dem Namen des Programms), ist
# damit genau der Weg hinaus.
#
# ⚠ Bis rc39 wurde hier nach der **ersten** Datei auf `.exe` gesucht. GitHub
# liefert sie alphabetisch, ein `-` (0x2D) steht vor einem `.` (0x2E), also kam
# `-Setup.exe` zuerst — und die alte `einspielen()` schob diesen Fund roh über
# die laufende `SC-BP-Watcher.exe`, ohne ihn je auszuführen. Am 26.08.2026 im
# Test bestätigt: geladen wurden 14.812.324 Bytes statt 13.015.189.
#
# Seitdem ist es **Absicht**, den Installer zu holen — er wird gestartet statt
# kopiert. Inno beendet das laufende Programm selbst
# (`CloseApplications=force`), ersetzt die Datei, pflegt den Eintrag in
# „Apps & Features" und startet den Watcher danach wieder. Denselben Weg geht
# der SC-Deutsch-Launcher.
#
# Unter Linux bleibt es beim Tausch des AppImage — dort gibt es keinen
# Installer, und ein laufendes AppImage darf ersetzt werden.
#
# ⚠ `-setup.exe` steht vorn und bleibt: Genau danach suchen die Testfassungen
# rc39–rc75. Wird der Installer je umbenannt, bekommen sie nie wieder ein
# Update angeboten.
WINDOWS_INSTALLER = ('-setup.exe', '-installer.exe', '_setup.exe')


def matching_asset(release, kind=None):
    """Die zur eigenen Verpackung passende Datei aus einer Freigabe — oder None."""
    kind = kind or packaging()
    if kind == 'quellcode':
        return None                      # dort ist `git pull` der richtige Weg
    if kind == 'appimage':
        for asset in release.get('dateien') or []:
            name = (asset.get('name') or '').lower()
            if name.endswith('.appimage') and _url_ok(asset.get('url') or ''):
                return asset
        return None
    # Windows: **nur** der Installer. Findet sich keiner, gibt es lieber gar
    # kein Update als das falsche — die nackte .exe wäre hier wertlos, weil
    # niemand mehr da ist, der sie an ihren Platz legt.
    for asset in release.get('dateien') or []:
        name = (asset.get('name') or '').lower()
        if name.endswith(WINDOWS_INSTALLER) and _url_ok(asset.get('url') or ''):
            return asset
    return None


def _url_ok(url):
    """Nur Dateien von GitHub — egal, was die Antwort sonst behauptet."""
    try:
        from urllib.parse import urlparse
        parts = urlparse(url)
        return parts.scheme == 'https' and (
            parts.hostname in ALLOWED_HOSTS
            or (parts.hostname or '').endswith('.github.com'))
    except Exception:
        return False


# ------------------------------------------------------------ Prüfsummen
#
# ⚠⚠ **Herkunft ist nicht Inhalt.** `_url_ok()` stellt sicher, dass die Datei
# von GitHub kommt — nicht, dass es **die richtige** Datei ist. Bis v3.28.x
# wurde alles eingespielt, was durch diesen Filter kam.
#
# Seit P1 gilt: **Keine gültige Prüfsumme, keine Installation.** Kein Schalter,
# kein „trotzdem installieren", kein stilles Durchwinken. Wer die Prüfung nicht
# bestehen kann, bekommt den Weg von Hand über die Release-Seite genannt.
#
# ⚠ Ältere ausgelieferte Fassungen lassen sich nicht nachrüsten — ihr Updater
# ist längst beim Nutzer. Das ist eine Tatsache, kein Schlupfloch für Neues.
CHECKSUM_FILE = 'SHA256SUMS.txt'

# Was der Updater ueberhaupt anfassen darf. Alles andere wird abgelehnt, auch
# wenn GitHub es anbietet — ein Asset-Name kommt vom Server, nicht von uns.
ALLOWED_SUFFIXES = ('.appimage', '.exe')


def safe_filename(name, fallback='update.bin', verified=False):
    """Aus einem Asset-Namen einen harmlosen Dateinamen machen.

    ⚠⚠ **Mit `geprueft=True` gibt es KEINEN Rückfall — dann kommt `None`.**
    Das ist der Unterschied zwischen „irgendwohin schreiben" und „darf das
    hier überhaupt sein": Ein Rückfallname ist für Notpfade recht, aber eine
    Sicherheitsentscheidung darf er nicht tragen. Sonst hinge alles daran,
    dass in keiner Summen-Datei je ein Eintrag `update.bin` steht.

    ⚠⚠ Der Name kommt aus der Antwort des Servers und wurde bisher **roh** als
    Pfadbestandteil benutzt. Ein Name wie `../../autostart/boese.exe` hätte die
    Datei damit an einen ganz anderen Ort gelegt. Dass GitHub so etwas nicht
    vergibt, ist kein Argument: Der Wert ist trotzdem Fremdeingabe.

    Deshalb: nur der reine Dateiname, keine Pfadtrenner, keine Punkte-Namen —
    und nur die Endungen, die dieses Programm überhaupt einspielt.
    """
    raw = (name or '').strip()
    # Beide Trennzeichen entfernen: Ein unter Linux geladener Name kann einen
    # Backslash tragen und umgekehrt.
    raw = os.path.basename(raw.replace('\\', '/').rstrip('/'))
    if raw in ('', '.', '..') or not raw.lower().endswith(ALLOWED_SUFFIXES):
        return None if verified else fallback
    return raw


def compute_checksum(path):
    """Die SHA-256-Summe einer Datei — blockweise, nicht am Stück.

    ⚠ Blockweise, weil ein AppImage rund 100 MB hat. `f.read()` am Stück wäre
    auf einem knappen Rechner ein vermeidbarer Ausschlag im Speicher.
    """
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for block in iter(lambda: f.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def parse_checksums(text):
    """`SHA256SUMS.txt` in `{Dateiname: Summe}` übersetzen.

    Das Format ist das von `sha256sum`: `<64 Zeichen hex>  <Dateiname>`. Ein
    Stern vor dem Namen (Binärkennzeichen) wird abgeschnitten.

    ⚠⚠ **Zwei Dinge machen die ganze Datei ungültig, statt still zu gewinnen:**

    1. **Ein Pfadanteil im Namen.** Der Bau erzeugt die Datei ausdrücklich mit
       reinen Dateinamen (`cd dateien/linux && sha256sum …`). Stünde dort
       `dateien/linux/…`, wäre etwas anders als gedacht — das per `basename()`
       geradezubiegen hiesse, den Fehler zu verstecken.
    2. **Ein Name zweimal.** Dann entschied vorher schlicht die letzte Zeile.
       Welche Summe gilt, darf nicht von der Reihenfolge abhängen.

    In beiden Fällen kommt eine **leere** Tabelle zurück — und leer heisst
    weiter oben: keine Installation.
    """
    table = {}
    for line in (text or '').splitlines():
        parts = line.strip().split(None, 1)
        if len(parts) != 2:
            continue
        checksum, name = parts[0].lower(), parts[1].strip().lstrip('*')
        if len(checksum) != 64 or not all(c in '0123456789abcdef' for c in checksum):
            continue
        if '/' in name or '\\' in name:
            return {}
        if name in table:
            return {}
        table[name] = checksum
    return table


def fetch_checksums(release):
    """Die Prüfsummen einer Freigabe. Gibt `(tabelle, grund)`.

    `grund` ist `''`, wenn alles gut ging, sonst sagt es **warum nicht** — und
    das ist der Punkt: „Die Datei gibt es nicht" ist etwas anderes als „das
    Netz war weg". Die zweite Lage darf nicht wie ein manipuliertes Update
    aussehen, sonst erschrickt jemand grundlos.
    """
    for asset in release.get('dateien') or []:
        if (asset.get('name') or '') != CHECKSUM_FILE:
            continue
        url = asset.get('url') or ''
        if not _url_ok(url):
            return {}, 'fremd'
        try:
            req = urllib.request.Request(url, headers={'User-Agent': USER_AGENT})
            with urllib.request.urlopen(req, timeout=30) as r:
                # Die Datei ist ein paar hundert Byte gross; die Grenze ist
                # nur da, damit eine falsche Antwort nicht den Speicher frisst.
                text = r.read(64 * 1024).decode('utf-8', 'replace')
            return parse_checksums(text), ''
        except Exception as ausnahme:
            fehler.merken('updater.pruefsummen_holen', ausnahme)
            return {}, 'netz'
    return {}, 'fehlt'


def _download_target(name):
    """Wohin die neue Version geladen wird.

    **Windows** — in den Temp-Ordner. Geholt wird dort ein Installer, der nur
    einmal gestartet und danach nie wieder gebraucht wird; Windows räumt den
    Ordner von selbst auf. Früher lag er neben dem Programm, und wenn das Update
    scheiterte, blieben dort 14 MB liegen, die niemand zuordnen konnte.

    **Linux** — **neben** das laufende AppImage.

    ⚠ Hier lag ein Fehler, der jedes Selbst-Update unter Linux scheitern ließ:
    Geladen wurde nach `/tmp`, eingespielt mit `os.replace()`. Auf so gut wie
    jedem Linux ist `/tmp` ein eigenes Dateisystem (tmpfs), und `os.replace` kann
    nicht über Dateisystemgrenzen verschieben — es endet mit
    „[Errno 18] Invalid cross-device link". Gemeldet am 25.08.2026 direkt aus dem
    Fenster.

    Nebenbei ist das Einspielen dadurch **atomar**: Innerhalb eines Dateisystems
    ist `os.replace` unteilbar, es gibt keinen Moment, in dem die Datei halb da
    ist. Ist der Zielordner nicht beschreibbar, bleibt `/tmp` als Rückfall — dann
    greift beim Einspielen der Umweg über `shutil.move`.

    ⚠ Der Name wird **entschärft**, bevor er in einen Pfad kommt — siehe
    `sicherer_dateiname()`. Er stammt aus der Antwort des Servers.
    """
    clean = safe_filename(name)
    if sys.platform.startswith('win'):
        return os.path.join(tempfile.gettempdir(), clean)
    running = own_appimage() or sys.executable
    folder = os.path.dirname(os.path.abspath(running))
    if os.access(folder, os.W_OK):
        return os.path.join(folder, '.' + clean + '.neu')
    return os.path.join(tempfile.gettempdir(), clean)


def download(asset, progress=None, release=None):
    """Lädt die neue Version in eine Nebendatei. Gibt deren Pfad zurück.

    Geladen wird **neben** das laufende Programm, nicht darüber: Bricht die
    Leitung ab, ist die alte Version noch vollständig da.

    ⚠⚠ **`freigabe` ist Pflicht, nicht Beiwerk.** Aus ihr kommt die
    veröffentlichte Prüfsumme, und ohne die wird nicht eingespielt. Wer diesen
    Aufruf ohne `freigabe` baut, hätte den Schutz wieder abgeschaltet —
    deshalb fliegt das hier auf, statt still durchzugehen.

    Die Prüfung sitzt bewusst **hier** und nicht in `einspielen()`: So bekommt
    `einspielen()` niemals eine ungeprüfte Datei zu sehen, und es gibt keinen
    zweiten Weg, an dem man sie vorbeischleusen könnte.
    """
    from . import sprache
    url = asset.get('url')
    if not _url_ok(url):
        # ⚠ Der Text dieser Ausnahme landet über `str(fehler)` sichtbar
        # beim Nutzer (siehe `return False, str(fehler)` weiter unten).
        raise ValueError(sprache.t('up_fremde_quelle'))

    # ⚠ Die Prüfsummen kommen **vor** dem Herunterladen. Fehlen sie, ist der
    # Download umsonst — und 100 MB umsonst zu laden, um sie danach
    # wegzuwerfen, wäre unhöflich gegenüber jeder Leitung.
    if release is None:
        raise ValueError(sprache.t('up_ohne_pruefung'))
    # ⚠⚠ **Kein Rückfallname für eine Sicherheitsentscheidung.** `geprueft=True`
    # gibt bei einem unbrauchbaren Asset-Namen `None` statt `update.bin`
    # zurück. Sonst könnte ein Anhang mit fremder Endung durchrutschen, sobald
    # in der Summen-Datei zufällig ein Eintrag `update.bin` stünde — der
    # Rückfall gehört an Anzeige- und Notpfade, nicht hierher.
    name = safe_filename(asset.get('name'), verified=True)
    if not name:
        raise ValueError(sprache.t('up_fremde_datei') % (asset.get('name') or '?'))

    table, reason = fetch_checksums(release)
    expected = table.get(name)
    if not expected:
        # Vier Lagen, vier Sätze — „kein Netz" darf nicht wie „manipuliert"
        # klingen, „diese Fassung hat noch keine Prüfsummen" auch nicht, und
        # eine Summen-Datei von fremdem Server ist etwas anderes als gar keine.
        raise ValueError(sprache.t({
            'netz': 'up_summen_netz',
            'fremd': 'up_summen_fremd',
        }.get(reason, 'up_keine_summen')))

    target = _download_target(asset.get('name'))
    # ⚠⚠ **Ab hier liegt eine Datei auf der Platte — und sie ist ungeprüft.**
    #
    # Bricht die Leitung mitten im Schreiben ab, wird die Summe nie gerechnet,
    # und ein Bruchstück bleibt liegen: unter Linux **neben** dem laufenden
    # AppImage. Genau das soll P1 verhindern, und der erste Anlauf räumte nur
    # bei falscher Summe auf. Deshalb umschliesst der Fehlerpfad jetzt das
    # ganze Stück von der ersten geschriebenen Zeile bis zur bestandenen
    # Prüfung.
    try:
        _fetch_and_verify(url, target, expected, progress)
    except Exception:
        _discard(target)
        raise
    # ⚠ Die geprüfte Summe wandert mit. Unter Windows startet nicht mehr der
    # Watcher den Installer, sondern ein Helfer — ein anderer Prozess, Sekunden
    # später. Bis dahin kann die Datei in `%TEMP%` ersetzt worden sein. Der
    # Helfer gleicht deshalb unmittelbar vor dem Start noch einmal ab, und zwar
    # gegen **diese** Summe aus der Veröffentlichung, nicht gegen eine, die er
    # selbst aus der Datei rechnet.
    _CHECKED[os.path.abspath(target)] = expected
    return target


def _discard(target):
    """Eine ungeprüfte Update-Datei entfernen. Scheitert nie laut."""
    try:
        os.remove(target)
    except FileNotFoundError:
        pass
    except OSError as ausnahme:
        fehler.merken('updater.summe_verwerfen', ausnahme)


def _fetch_and_verify(url, target, expected, progress=None):
    """Herunterladen und die Summe abgleichen. Wirft bei jedem Zweifel."""
    from . import sprache
    req = urllib.request.Request(url, headers={'User-Agent': USER_AGENT})
    with urllib.request.urlopen(req, timeout=120) as r, open(target, 'wb') as f:
        total = int(r.headers.get('Content-Length') or 0)
        loaded = 0
        while True:
            block = r.read(256 * 1024)
            if not block:
                break
            f.write(block)
            loaded += len(block)
            if progress and total:
                # ⚠ **Die Anzeige darf den Download nicht umbringen.** Sie ist
                # Beiwerk, das Herunterladen ist der Zweck — und genau
                # andersherum lief es: Der Rückruf zeichnet ins Fenster, und
                # wenn das aus einem Nebenfaden schiefgeht (`RuntimeError: main
                # thread is not in main loop`), riss die Ausnahme den ganzen
                # Faden mit. Der Nutzer sah: nichts. Kein Fortschritt, kein
                # Update, keine Meldung.
                #
                # Bomb20 am 27.08.2026, dreimal in Folge im Diagnosebericht:
                # „und ich habe auf get 68 geklickt, aber da kam nix mit restart
                # oder install." Es wurde nie etwas geladen.
                try:
                    progress(round(100 * loaded / total))
                except Exception:
                    progress = None      # einmal daneben, nie wieder fragen

    # ⚠ Das Aufräumen macht der Aufrufer — er fängt **jede** Ausnahme von hier
    # ab, nicht nur die falsche Summe.
    if compute_checksum(target) != expected:
        raise ValueError(sprache.t('up_summe_falsch'))


# ⚠ Hier stand einmal ein Hilfsskript, das die laufende `.exe` selbst tauschte —
# und das ist am 26.08.2026 im Test auf eine Verklemmung gelaufen, die niemand
# vorhergesehen hatte:
#
#   * Die App beendet sich, aber der PyInstaller-Bootloader lebt weiter: Er
#     räumt seinen Ordner unter `%TEMP%` auf. Zwei Laufzeit-Bibliotheken
#     (`VCRUNTIME140.dll`, `VCRUNTIME140_1.dll`) blieben gesperrt, und er stand
#     im Fenster "Failed to remove temporary directory" still.
#   * Solange dieses Fenster steht, hält der Bootloader die `.exe`.
#   * Das Skript wartete also auf eine Freigabe, die erst kam, wenn der Nutzer
#     eine Warnung wegklickte, von der er nicht wusste, dass sie zum Update
#     gehört. Nach zwei Minuten gab es auf — und im Programmordner blieb eine
#     verwaiste 14-MB-Datei liegen. **Bei jedem Versuch aufs Neue.**
#
# Der Eigenbau ist deshalb weg. Unter Windows startet jetzt der Installer, und
# der kann all das, was hier mühsam nachgebaut war: Er beendet das laufende
# Programm über den Restart Manager (`CloseApplications=force` in
# `installer.iss`), ersetzt die Datei und pflegt den Eintrag in
# „Apps & Features“.
#
# ⚠ **Zwei Angaben standen hier falsch** und haben am 28.08.2026 die Fehlersuche
# in die Irre geschickt. Wer hier nachliest, soll den Stand aus `installer.iss`
# bekommen, nicht den von vorgestern:
#
#   * „erkannt über `AppMutex`“ — **nein.** Der Restart Manager erkennt ein
#     laufendes Programm an den Dateien, die es offen hält; einen Mutex braucht
#     er dafür nicht. `AppMutex` stand einmal in `installer.iss` und hat den
#     Update-Weg am 26.08.2026 vollständig blockiert — die Begründung steht
#     ausführlich dort.
#   * „startet den Watcher danach wieder (`RestartApplications=yes`)“ —
#     **nein.** Dort steht `RestartApplications=no`, mit Absicht: Der Restart
#     Manager fährt nur wieder hoch, was er selbst *sanft* geschlossen hat, und
#     `force` schliesst hart. Nach einem stillen Update startet **niemand** den
#     Watcher — der Nutzer macht das selbst, angesagt über
#     `s_ub_hinweis_neustart`.
#
# ⚠ Der Installer hält das Programm auch nicht *unten*: Ein Autostart-Eintrag
# kann es mitten in der Installation wieder hochfahren — gemessen am 28.08.2026,
# `DeleteFile ... in use (5)`. Dagegen steht `PrepareToInstall` im
# `[Code]`-Abschnitt von `installer.iss`.


# ⚠ Unter Windows übernimmt der Installer auch den **Neustart**. `neu_starten()`
# darf dann NICHT auch noch starten — sonst kommen zwei Versionen hoch, und die
# zweite legt sich über die Arbeit der ersten. Unter Linux bleibt es beim
# Tausch, dort startet das Programm sich selbst neu.
_SWAP_RUNNING = [False]

# Welche Datei gegen welche veröffentlichte Summe geprüft wurde — gefüllt von
# `herunterladen()`, gelesen von `einspielen()`. Unter Windows geht die Summe
# an den Helfer weiter; ohne Eintrag hier wird dort nichts eingespielt.
_CHECKED = {}

# Die Sicherung der bisherigen Fassung unter Linux, gleich neben der Datei.
BACKUP_SUFFIX = '.vorher'


def _backup(target):
    """Die laufende Datei neben sich kopieren. True nur bei vollständiger Kopie."""
    import shutil
    before = target + BACKUP_SUFFIX
    try:
        shutil.copy2(target, before)
        return os.path.getsize(before) == os.path.getsize(target)
    except OSError as ausnahme:
        fehler.merken('updater.sichern', ausnahme)
        return False


def roll_back():
    """Die Sicherung von vor dem Tausch zurücklegen (Linux). True, wenn es klappte.

    Gebraucht, wenn die neue Fassung beim Start stirbt: Dann ist das AppImage
    schon getauscht, und die alte liefe nur noch aus ihrer offenen Inode
    weiter — wer sie schließt, stünde ohne Watcher da.
    """
    target = own_appimage()
    if not target:
        return False
    before = target + BACKUP_SUFFIX
    if not os.path.isfile(before):
        return False
    try:
        os.replace(before, target)
        os.chmod(target, 0o755)
        return True
    except OSError as ausnahme:
        fehler.merken('updater.zurueckrollen', ausnahme)
        return False


def install(new_file, target_version='', previous_version=''):
    """Die laufende Version durch die neue ersetzen.

    Zwei Wege, je nach Verpackung:

    * **Linux** — erst wird die laufende Datei gesichert, dann das AppImage
      getauscht. Den Neustart macht der Aufrufer, gleich danach.
    * **Windows** — ein Helfer übernimmt (`update_run`): Er wartet, bis dieser
      Watcher weg ist, prüft die Summe erneut, startet den Installer und fährt
      den Watcher danach wieder hoch.

    `ziel_version` und `alte_version` landen in der Laufmarke — daran sieht der
    nächste Start, was aus dem Update geworden ist.

    Gibt (True, '') zurück, wenn der Weg angetreten ist. Bei (False, Grund) muss
    der Nutzer selbst ran.

    ⚠ In dieser Funktion **kein** `fehler.merken`: Das `except … as fehler`
    unten macht `fehler` hier zur lokalen Variable, ein Aufruf davor fiele mit
    `UnboundLocalError` um."""
    kind = packaging()
    if kind == 'quellcode':
        return False, 'quellcode'
    target = own_appimage() or sys.executable

    # ⚠ Letzter Riegel vor dem Überschreiben: Der Dateiname muss zu uns gehören.
    # Selbst wenn die Erkennung oben irgendwann wieder danebenliegt, wird dadurch
    # keine fremde Datei ersetzt. Genau dieser Riegel hätte den Unfall vom
    # 25.08.2026 verhindert, bei dem ein fremdes AppImage überschrieben wurde,
    # weil `APPIMAGE` auf ein anderes Programm zeigte.
    # ⚠⚠ Auch hier beide Namen — siehe `pfade.gehoert_uns()`.
    if not pfade.gehoert_uns(target):
        from . import sprache
        return False, sprache.t('up_fremde_datei', os.path.basename(target))
    try:
        if kind == 'appimage':
            # Unter Linux darf die laufende Datei ersetzt werden, solange man sie
            # austauscht statt hineinzuschreiben: Der laufende Prozess hält die
            # alte Inode, die neue liegt sofort am Platz.
            #
            # ⚠ **Erst sichern, dann tauschen.** Stirbt die neue Fassung beim
            # Start, ist der Rückweg damit ein Umbenennen (`zurueckrollen()`),
            # genau wie bei `tools/testfassung_holen.sh`. Lässt sich die
            # Sicherung nicht anlegen, wird auch nicht getauscht — ein Update
            # ohne Rückweg ist keins.
            if not _backup(target):
                from . import sprache
                _discard(new_file)
                return False, sprache.t('up_sicherung_nein')
            #
            # ⚠ `os.replace` schafft das nur **innerhalb eines Dateisystems**.
            # Deshalb wird gleich daneben geladen (siehe `_ablageort_fuer_update`).
            # Liegt die Datei doch woanders — Zielordner schreibgeschützt, eigener
            # Pfad über `SC_BP_APPIMAGE` —, tut es `shutil.move`: Das kopiert bei
            # Bedarf und räumt danach auf. Langsamer, aber es funktioniert.
            try:
                os.replace(new_file, target)
            except OSError as reason:
                if getattr(reason, 'errno', None) != errno.EXDEV:
                    raise
                import shutil
                shutil.move(new_file, target)
            os.chmod(target, 0o755)
            return True, ''
        # Windows: den Installer starten. Er bringt alles mit, was hier frueher
        # von Hand nachgebaut war — siehe die Erklaerung oben.
        #
        # `/SILENT` zeigt nur einen Fortschrittsbalken statt des ganzen
        # Assistenten, `/NORESTART` verbietet ihm, den Rechner neu zu starten,
        # und `/CLOSEAPPLICATIONS` laesst ihn den laufenden Watcher schliessen.
        #
        # ⚠ **Kein `/RESTARTAPPLICATIONS`.** Das war ein Fehler und hat am
        # 26.08.2026 den Selbststart zerschossen. Der Schalter uebersteuert
        # `RestartApplications=no` aus `installer.iss` — und dann starten
        # **zwei** Wege den Watcher: der Restart Manager und der
        # `[Run]`-Abschnitt. Im Protokoll steht beides direkt untereinander:
        #
        #     Attempting to restart applications.
        #     -- Run entry --   Filename: ...\SC-BP-Watcher.exe
        #
        # ⚠⚠ **„Security validation failure: parent process has different
        # executable!" kam NICHT von Inno** — richtiggestellt am 11.09.2026.
        # Alle zehn Meldungen dieser Reihe stehen im PyInstaller-Bootloader
        # der Watcher-`.exe` (nachgesehen), im Inno-Installer keine. Es ist die
        # Prüfung, mit der eine gepackte `.exe` kontrolliert, ob ihr Vater ihr
        # eigener Bootloader ist.
        #
        # Was am 26.08.2026 sehr wahrscheinlich geschah: Inno startete den
        # Watcher über `[Run]` (bzw. den Restart Manager) neu, und der neue
        # erbte über das Setup die `_PYI_*`-Variablen des alten. Er hielt sich
        # für das Kind eines Bootloaders, fand als Vater aber Innos Setup —
        # „parent process has different executable". Aus einer PowerShell
        # heraus war das nie nachstellbar, weil es dort kein `_PYI_*` gibt.
        # Am 11.09.2026 kam dieselbe Reihe wieder („failed to obtain
        # executable path for parent proces", der Vater war schon beendet) —
        # diesmal mit „Installation process succeeded" im Setup-Protokoll eine
        # Sekunde davor. Die Installation war also nie das Problem, der
        # Neustart war es.
        #
        # Den Neustart macht seitdem der Helfer (`update_run`) mit einer
        # Umgebung ohne `_PYI_*`. `/RESTARTAPPLICATIONS` bleibt trotzdem weg:
        # Sonst startet ein zweiter Weg den Watcher.
        _SWAP_RUNNING[0] = True

        # ⚠ Die Umgebung MUSS gesaeubert werden, das Arbeitsverzeichnis ebenso.
        # Sie geht über den Helfer an den neu gestarteten Watcher, und alles,
        # was vom laufenden PyInstaller stammt, zeigt entweder in dessen Ordner
        # unter `%TEMP%`, den er gleich aufraeumt (siehe `neu_starten()`), oder
        # führt den neuen Bootloader in die Irre (siehe oben). Deshalb über
        # `saubere_umgebung()`, nicht `dict(os.environ)`: Dort fliegen auch die
        # `_PYI_*`-Variablen raus.
        env = pfade.saubere_umgebung()
        for name in ('_MEIPASS', '_MEIPASS2', 'TCL_LIBRARY', 'TK_LIBRARY',
                     'TIX_LIBRARY', 'MATPLOTLIBDATA'):
            env.pop(name, None)

        # `__COMPAT_LAYER` fliegt weiterhin raus — als Vorsicht, nicht als
        # Heilmittel. Am 26.08.2026 galt die Variable als DIE Ursache: Mit ihr
        # stand „Compatibility mode: Yes (DetectorsAppHealth)" im
        # Setup-Protokoll, ohne sie lief das Update durch. Die Messung vom
        # 11.09.2026 konnte den Fehler mit ihr allein aber nicht nachstellen —
        # Inno installiert damit anstandslos. Wahrscheinlich fiel das Entfernen
        # damals mit einem Lauf zusammen, in dem der Neustart anders verlief.
        # Schaden tut es nicht: Kein Programm, das der Helfer startet, braucht
        # einen Kompatibilitäts-Shim.
        env.pop('__COMPAT_LAYER', None)
        # ⚠ **Das Setup schreibt ein Protokoll**, und zwar immer — nicht nur im
        # Fehlerfall. Am 26.08.2026 lagen **drei** Erklärungsversuche daneben
        # (vererbtes Arbeitsverzeichnis, `/RESTARTAPPLICATIONS`, sterbender
        # Elternprozess), und die vierte — `__COMPAT_LAYER` — sehr
        # wahrscheinlich auch: Gesucht wurde im Installer, gemeldet hatte der
        # neu gestartete Watcher. Erst das Protokoll vom 11.09.2026 trennte
        # beides.
        #
        # Ohne Protokoll bleibt in so einem Fall nur Raten — und Raten hat hier
        # drei Versionen gekostet. Mit Protokoll beantwortet der nächste
        # Fehlerfall die Frage selbst, auch wenn er bei einem Nutzer auftritt,
        # dessen Rechner niemand ansehen kann.
        #
        # Es landet neben dem Fehlerbericht, wird also vom Diagnose-Bericht
        # miterfasst. Eine Datei pro Lauf, die alte wird überschrieben — es geht
        # um den letzten Versuch, nicht um ein Tagebuch.
        log_file = ''
        try:
            # ⚠ Kein `from . import pfade` hier: Ein Import in der Funktion
            # macht `pfade` für die GANZE Funktion lokal — und
            # `pfade.saubere_umgebung()` weiter oben fiele mit
            # `UnboundLocalError` um. Prüfung 185 hat genau das gefangen.
            log_file = pfade.app_datei('update-setup.txt')
        except Exception:
            pass                     # ohne Protokoll ist der Weg derselbe

        # ⚠⚠ **Seit dem Ein-Klick-Update startet nicht mehr der Watcher den
        # Installer, sondern ein Helfer** (`update_run`). Bis v3.29.0 lief der
        # Installer still, startete bei `/SILENT` absichtlich nichts, und der
        # Watcher blieb unten. Jetzt wartet eine `.cmd` in `%TEMP%` auf unser
        # Ende, prüft die Summe erneut, startet den Installer mit
        # `/SUPPRESSMSGBOXES` und fährt uns danach wieder hoch.
        #
        # Gemessen am 11.09.2026, bevor gebaut wurde:
        #   * Ein `cmd` als Elternprozess stört Inno nicht — lebend wie sterbend.
        #   * Der Restart Manager meldete den laufenden Watcher **nicht**
        #     („found no applications"). Deshalb wartet der Helfer selbst auf
        #     unsere PID, statt sich auf `CloseApplications` zu verlassen.
        #   * Ohne `/SUPPRESSMSGBOXES` hängt ein echter Fehler an einem
        #     unsichtbaren OK-Fenster; mit endet das Setup nach 0,4 s mit 3.
        #   * Pfade mit `& ^ % ! ( ) '`, Umlauten und 185 Zeichen gelingen.
        #
        # ⚠ Dorthin installieren, wo das laufende Programm liegt — sonst gibt es
        # zwei Kopien.
        #
        # v2.0.0 wurde **nur** als nackte `SC-BP-Watcher.exe` ausgeliefert; alle
        # ihre Nutzer laufen zwangsläufig „portabel", ohne es gewollt zu haben.
        # Gemeldet am 27.08.2026: „niemand nutzt sowas portabel … niemand
        # schiebt es auf nen Stick, um an nem anderen PC SC zu spielen."
        #
        # Ohne `/DIR` nimmt Inno seinen Standardordner
        # (`%LOCALAPPDATA%\Programs\…`) — die alte Datei bliebe daneben liegen,
        # und wer sie per Verknüpfung startet, benutzt für immer die alte
        # Fassung. Mit `/DIR` wird ersetzt statt danebengelegt.
        #
        # Läuft das Programm bereits installiert, zeigt `sys.executable` auf den
        # Installationsordner — dann ist es derselbe Wert, den Inno ohnehin
        # gewählt hätte. Ein Fall, zwei Wege, dieselbe Zeile.
        own_dir = os.path.dirname(os.path.abspath(sys.executable))

        # ⚠⚠ **Ohne geprüfte Summe keine Übergabe.** `herunterladen()` legt sie
        # in `_GEPRUEFT` ab; fehlt sie, kam die Datei nicht über den geprüften
        # Weg — und dann wird hier auch nichts gestartet.
        checksum = _CHECKED.get(os.path.abspath(new_file))
        if not checksum:
            from . import sprache
            return False, sprache.t('up_ohne_pruefung')

        # ⚠⚠ Wie der Helfer gestartet wird, steht an EINER Stelle
        # (`update_run.helfer_flags`) — der Selbsttest startet ihn genauso.
        # Hier stand `DETACHED_PROCESS`: Der Helfer lief ohne Konsole, jedes
        # Konsolenprogramm darin bekam eine eigene, sichtbare, und ignorierte
        # die Umleitungen. Im ersten Echttest am 11.09.2026 hing `find` in
        # einem Fenster, und `certutil` schrieb seine Summe ins Leere.
        from . import update_run
        flags = update_run.helper_flags()

        # Erst die Laufmarke, dann der Helfer: Stirbt irgendetwas danach, weiß
        # der nächste Start, dass ein Update begonnen hatte.
        update_run.begin_run(target_version, previous_version, new_file, checksum)
        update_run.start_helper(new_file, checksum, own_dir,
                                   log_file, env, flags,
                                   exe=sys.executable)
        return True, ''
    except Exception as fehler:
        return False, str(fehler)


if __name__ == '__main__':
    own_value = '1.6.0-dev'
    print('Verpackung:', packaging())
    fresh = check(own_value, force='--jetzt' in sys.argv)
    print('Neuere Version:', (fresh or {}).get('version') or 'keine')
    print('\nÄnderungsprotokoll:')
    for e in history()[:6]:
        heading = '%s %s' % (e['version'], ('(%s)' % e['datum']) if e['datum'] else '')
        print('  %-28s %s  %d Zeichen' % (heading, e['quelle'], len(e['text'])))


# --------------------------------------------------- Eintrag in "Apps & Features"
# Der Installer trägt die Version dort ein. Ersetzt sich das Programm danach
# selbst, bleibt der Eintrag auf dem alten Stand stehen — Windows zeigt dann
# eine Nummer, die es gar nicht mehr gibt. Dazu: „Die Versionsanzeige
# wäre schon wichtig, da User ja sonst nicht sehen ob sie aktuell sind."
#
# Der Schlüssel liegt unter HKCU (der Installer schreibt in den Benutzerzweig),
# deshalb braucht das **keine Administratorrechte**.
INNO_KEY = '{7C4B1E93-2A6F-4D58-B0E1-9F3A5C8D2461}_is1'


# Die frisch gestartete Version. Gebraucht wird sie nur, um **nachzusehen, ob
# sie noch lebt** — siehe `neue_fassung_laeuft()`.
_STARTED = [None]

# Wohin die Fehlerausgabe der frisch gestarteten Fassung läuft, und die offene
# Datei dazu. Ohne das ist ein gescheiterter Neustart nicht aufzuklären.
RESTART_OUTPUT_FILE = 'neustart-ausgabe.txt'
_OUTPUT = [None]


def new_version_alive(wait=3.0):
    """Lebt die eben gestartete Version noch? Erst danach darf die alte gehen.

    ⚠ **Ein Programm zu starten heißt nicht, dass es läuft.** `Popen` meldet
    Erfolg, sobald der Prozess angelegt ist; ob er eine Sekunde später an einer
    fehlenden Bibliothek stirbt, erfährt niemand — `stdout` und `stderr` gehen
    nach `/dev/null`, und auf den Rückgabewert wartete bisher keiner.

    Genau so ist der Neustart unter Linux monatelang **stumm** gescheitert: Die
    alte Version trat pflichtschuldig ab, die neue war da schon tot, und übrig
    blieb ein Rechner ohne Watcher. Der Nutzer sieht nur „es geht aus und kommt
    nicht wieder" und kann nicht einmal sagen, woran es lag.

    Diese Prüfung kostet ein paar Sekunden Warten — sie gehört deshalb in einen
    eigenen Faden, nicht in den Tk-Faden.

    Gibt `True` zurück, wenn die neue Version die Wartezeit überlebt hat oder es
    gar keinen eigenen Prozess gibt (Windows: dort startet der Installer neu).
    """
    process = _STARTED[0]
    if process is None:
        return True
    import time
    end = time.monotonic() + wait
    while time.monotonic() < end:
        if process.poll() is not None:
            _report_death(process.returncode)
            return False
        time.sleep(0.15)
    return True


def _report_death(exit_code):
    """Warum die neue Fassung gestorben ist — ins Fehlerprotokoll damit.

    ⚠ Ohne das steht im Diagnosebericht **gar nichts**: Bis rc69 wurde nur die
    Meldung ins Fenster geschrieben, und wer den Bericht schickte, hatte keinen
    einzigen Eintrag dazu. Am 27.08.2026: Neustart klappte nicht,
    Protokoll leer, Ursache im Dunkeln.
    """
    text = ''
    asset = _OUTPUT[0]
    if asset is not None:
        try:
            asset.flush()
            asset.seek(0)
            text = (asset.read() or '').strip()[-800:]
        except Exception:
            text = ''
    fehler.merken('updater.neustart_tot',
                  RuntimeError('Rückgabewert %s%s' % (
                      exit_code, (' — ' + text) if text else
                      ' — keine Ausgabe')))


def restart():
    """Das Programm durch die frisch eingespielte Version ersetzen.

    Nach einem Update läuft weiter die alte Version — der Prozess hält seine alte
    Inode. „Beim nächsten Start läuft die neue" stimmt zwar, heißt aber: selbst
    beenden und selbst wieder starten. Das nimmt dieser Weg ab.

    ⚠ Reihenfolge: erst den Einzelinstanz-Wächter schließen, dann starten. Sonst
    sieht die neue Version den belegten Port, hält sich für die zweite Instanz und
    beendet sich sofort wieder.
    """
    import subprocess
    from . import overlay as overlay_modul
    target = own_appimage() or sys.executable
    if packaging() == 'quellcode':
        return False
    try:
        overlay_modul.waechter_stoppen()

        # Wartet ein Hilfsskript darauf, die Datei zu tauschen, ist unser Teil
        # hier **erledigt** — es startet die neue Version selbst. Siehe die
        # Erklärung über `_TAUSCH_LAEUFT`.
        if _SWAP_RUNNING[0]:
            return True

        # ⚠ **Hier stand `dict(os.environ)`** — und genau daran ist der Neustart
        # unter Linux gescheitert. Entfernt wurden nur `APPIMAGE` und Freunde;
        # `LD_LIBRARY_PATH`, `PYTHONHOME` und `PYTHONPATH` blieben stehen, und
        # die zeigen im AppImage in den **entpackten Mount der alten Version**.
        # Zwei Sekunden später beendet sich die alte, ihr Mount verschwindet —
        # und die neue sucht ihre Bibliotheken in einem Verzeichnis, das es nicht
        # mehr gibt. Sie stirbt, bevor ein Fenster kommt.
        #
        # Für den Nutzer sah das so aus: „es geht dann aus aber startet nicht"
        # (Bomb20, 27.08.2026), am selben Tag nachgestellt.
        #
        # `pfade.saubere_umgebung()` macht genau diese Wäsche — sie war längst da,
        # nur benutzte der Neustart eine eigene, unvollständige Version davon.
        # Zwei Wäschen sind eine zu viel.
        from . import pfade as pfade_modul
        env = pfade_modul.saubere_umgebung()
        # Die Variablen des laufenden AppImage gehören der **alten** Version.
        for name in ('APPIMAGE', 'APPDIR', 'OWD', 'ARGV0'):
            env.pop(name, None)

        # ⚠ Dasselbe gilt unter Windows für die Variablen von PyInstaller — und
        # dort ist es schlimmer, weil das Programm gar nicht mehr startet.
        #
        # Eine mit PyInstaller gebaute `.exe` entpackt sich beim Start nach
        # `%TEMP%\_MEIxxxxxx` und zeigt mit `TCL_LIBRARY` und `TK_LIBRARY`
        # dorthin. Erbt die neue Version diese Variablen, sucht sie ihre
        # Tcl-Dateien im Ordner der **alten** — den die alte beim Beenden
        # gerade aufräumt. Ergebnis, so beim Testen gemeldet (Haldjas,
        # 25.08.2026):
        #
        #     Failed to execute script 'sc_bp_watcher' due to unhandled
        #     exception: Can't find a usable init.tcl in the following
        #     directories: C:\Users\…\AppData\Local\Temp\_MEI000067b42…
        #
        # Und gleich hinterher die Gegenseite: „Failed to remove temporary
        # directory" — die alte Version kommt an ihren eigenen Ordner nicht
        # mehr heran, weil die neue darin liest.
        for name in ('_MEIPASS', '_MEIPASS2', 'TCL_LIBRARY', 'TK_LIBRARY',
                     'TIX_LIBRARY', 'MATPLOTLIBDATA'):
            env.pop(name, None)
        # ⚠ **`stderr` NICHT wegwerfen.** Hier stand `DEVNULL` — und genau
        # deshalb war der gescheiterte Neustart unter Linux monatelang nicht
        # aufzuklären: Die neue Fassung schrieb ihren Grund brav auf die
        # Fehlerausgabe, und wir haben ihn ins Nichts geleitet. Übrig blieb
        # „geht aus, kommt nicht wieder" und Raten.
        #
        # Jetzt läuft die Ausgabe in eine Datei neben den Diagnosebericht. Kommt
        # die neue Fassung nicht hoch, steht dort, woran es lag — und
        # `neue_fassung_laeuft()` hängt es ins Fehlerprotokoll, wo es im Bericht
        # auftaucht.
        try:
            _OUTPUT[0] = open(pfade_modul.app_datei(RESTART_OUTPUT_FILE), 'w+',
                               encoding='utf-8', errors='replace')
        except Exception:
            _OUTPUT[0] = None
        _STARTED[0] = subprocess.Popen(
            [target], env=env, start_new_session=True,
            stdout=subprocess.DEVNULL,
            stderr=_OUTPUT[0] or subprocess.DEVNULL)
        return True
    except Exception as ausnahme:
        fehler.merken('updater.neu_starten', ausnahme)
        return False


def update_windows_entry(own_version):
    """Die angezeigte Version in Windows nachziehen. True, wenn geändert.

    ⚠ Der Schlüssel liegt **nicht** immer unter HKCU. Der Kommentar über
    `INNO_KENNUNG` behauptete das jahrelang, und die Funktion suchte nur dort —
    also fand sie am 26.08.2026 auf dem Testrechner gar nichts, obwohl der
    Eintrag existierte. Er lag unter **HKLM**.

    Grund: `installer.iss` hat zwar `PrivilegesRequired=lowest`, dazu aber
    `PrivilegesRequiredOverridesAllowed=dialog`. Inno fragt damit beim
    Installieren nach, und wer "für alle Nutzer" wählt, bekommt seinen Eintrag
    im Maschinenzweig. Gesucht wird deshalb in beiden.

    Schreiben klappt in HKLM ohne Administratorrechte nicht — das ist in Ordnung
    und **kein Fehler**: Dann bleibt die angezeigte Nummer eben stehen, statt
    dass eine Ausnahme das Update aufhält.
    """
    if not sys.platform.startswith('win') or not own_version:
        return False
    try:
        import winreg
    except ImportError:
        return False
    path = (r'Software\Microsoft\Windows\CurrentVersion\Uninstall\%s'
            % INNO_KEY)
    for hive in (winreg.HKEY_CURRENT_USER, winreg.HKEY_LOCAL_MACHINE):
        try:
            with winreg.OpenKey(hive, path, 0,
                                winreg.KEY_READ | winreg.KEY_WRITE) as key:
                try:
                    shown = winreg.QueryValueEx(key, 'DisplayVersion')[0]
                except FileNotFoundError:
                    shown = ''
                if str(shown) == str(own_version):
                    return False
                winreg.SetValueEx(key, 'DisplayVersion', 0, winreg.REG_SZ,
                                  str(own_version))
                return True
        except FileNotFoundError:
            continue          # in diesem Zweig nicht installiert
        except PermissionError:
            continue          # HKLM ohne Administratorrechte — hinnehmen
        except Exception as ausnahme:
            from . import fehler
            fehler.merken('updater.windows_eintrag', ausnahme)
            return False
    return False
