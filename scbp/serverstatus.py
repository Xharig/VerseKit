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
Läuft Star Citizen gerade? — der Serverstatus von CIG.

**Wozu.** Wer craften will und das Spiel lässt ihn nicht hinein, sucht den Fehler
zuerst bei sich: Neu starten, Ordner leeren, Anmeldung prüfen. Ein Blick ins
Werkzeug beantwortet die Frage vorher.

⚠️ **Das hier ist eine Meldung von CIG, keine Messung.** Die Statusseite wird von
Hand gepflegt. Vergisst jemand, eine Meldung zu schließen, steht dort weiter
„Wartung"; stürzen die Server ab, bevor jemand schreibt, steht dort „läuft".
Deshalb gehört an die Anzeige **immer** die Uhrzeit des Abrufs und die Quelle —
und sie darf nie als eigene Feststellung auftreten.

⚠️ **Die Zustände bleiben im Wortlaut von CIG** (`operational`, `maintenance`,
`major_outage`). Eine Übersetzung wäre eine Aussage, die RSI nie gemacht hat —
und im Zweifel eine falsche.

Die Quelle (geprüft 26.08.2026):

    index.json                       Lage aller Systeme, rund 3 KB
    issues/<datei>/index.json        ein Vorfall im Volltext
    issues/index.json                die Historie, rund 145 KB

Es ist eine **statische Seite** (cState auf S3) — kein Schlüssel, keine Anmeldung.

⚠️ **Sackgasse, die Zeit kostet:** Die üblichen Atlassian-Pfade
(`/api/v2/status.json`, `/api/v2/summary.json`) antworten mit **403**. Wer dort
sucht, hält die Seite für unlesbar und gibt auf, obwohl die Daten offen liegen.

**Sparsam fragen.** Die Seite liefert `ETag` und `Last-Modified`. Gefragt wird mit
`If-None-Match`; hat sich nichts geändert, antwortet der Server mit **304** und
ohne Inhalt. Das kostet fast nichts und darf deshalb oft passieren.
"""
import calendar
import html
import json
import os
import re
import time
import urllib.error
import urllib.request

from . import pfade

BASE = 'https://status.robertsspaceindustries.com'
CACHE = 'serverstatus.json'
TIMEOUT = 15
OFF = os.environ.get('SC_BP_NO_NET', '') not in ('', '0')

# Wie alt die gespeicherte Lage werden darf, bevor erneut gefragt wird.
FRESH_SEC = 300

# Die Ampelfarben stehen in der Seite selbst (`colorOk` und Geschwister). Sie
# hier noch einmal zu führen wäre doppelt — geholt werden sie beim Abruf und
# mit gespeichert. Diese hier greifen nur, wenn noch nie etwas geholt wurde.
COLORS = {'ok': '#008000', 'gestoert': '#cc4400',
          'aus': '#e60000', 'hinweis': '#24478f'}

# Welcher Zustand welche Ampel bekommt. cState kennt mehr Namen als die drei
# Farben; alles Unbekannte gilt als Störung — lieber einmal zu viel gewarnt.
LIGHTS = {
    'operational': 'ok',
    'monitoring': 'hinweis',
    'maintenance': 'hinweis',
    'degraded_performance': 'gestoert',
    'partial_outage': 'gestoert',
    'major_outage': 'aus',
}


def _ident():
    from sc_bp_watcher import __version__ as v
    return 'SC-BP-Watcher/%s (+https://github.com/Xharig/SC-BP-Watcher)' % v


def _fetch(path, etag=None):
    """Eine Datei der Statusseite holen.

    Gibt `(daten, etag)` zurück. Bei **304** (nichts geändert) kommt
    `(None, etag)` — das ist kein Fehler, sondern der Normalfall."""
    if OFF:
        return None, etag
    header = {'User-Agent': _ident(), 'Accept': 'application/json'}
    if etag:
        header['If-None-Match'] = etag
    request = urllib.request.Request(BASE + path, headers=header)
    try:
        with urllib.request.urlopen(request, timeout=TIMEOUT) as reply:
            raw = reply.read().decode('utf-8', 'replace')
            return json.loads(raw), reply.headers.get('ETag') or etag
    except urllib.error.HTTPError as e:
        if e.code == 304:
            return None, etag
        raise


def _text_from_html(raw):
    """Aus dem Meldungstext lesbare Zeilen machen — **mit** der Hervorhebung.

    Rückgabe: `[(text, fett), …]`.

    Der Text kommt als HTML, jede Zeile ein Absatz. CIG hebt darin genau das
    hervor, was man tun soll: „**Players are strongly advised to safely stow
    their vehicles**". Wer das Fett wegwirft, macht aus einer Warnung einen
    Satz unter vielen — deshalb wird `<strong>`/`<b>` mitgenommen und in der
    Anzeige wieder fett gesetzt.

    Bewusst kein HTML-Parser: Es geht um Absätze, Zeilenumbrüche, Fettung und
    Entities, nicht um verschachtelte Auszeichnung. `<!-- raw HTML omitted -->`
    steht als Kommentar drin und fällt beim Entfernen der Tags von selbst weg.
    """
    if not raw:
        return []
    text = re.sub(r'(?i)<!--.*?-->', '', raw, flags=re.S)
    text = re.sub(r'(?i)<br\s*/?>', '\n', text)
    text = re.sub(r'(?i)</p\s*>', '\n\n', text)

    lines = []
    for piece in text.split('\n'):
        if not piece.strip():
            continue
        # Fett ist die Zeile, wenn ihr sichtbarer Text vollständig in einer
        # Hervorhebung steckt. Ein einzelnes fettes Wort mitten im Satz
        # bekäme sonst die ganze Zeile fett — falsch gewichtet.
        without_tags = re.sub(r'<[^>]+>', '', piece).strip()
        # Verglichen wird der Text **innerhalb** der Hervorhebung mit dem
        # gesamten sichtbaren Text. Nur wenn beide gleich sind, ist die ganze
        # Zeile hervorgehoben.
        highlight = ' '.join(
            re.sub(r'<[^>]+>', '', hits)
            for hits in re.findall(r'(?is)<(?:strong|b)\s*>(.*?)</(?:strong|b)\s*>',
                                      piece)).strip()
        bold = bool(highlight) and highlight == without_tags
        clean = html.unescape(without_tags)
        if clean:
            lines.append((clean, bold))
    return lines


def _timestamp(raw):
    """'2026-08-26 14:15:00 +0000 UTC' -> Sekunden seit 1970, oder None.

    ⚠ **Die Seite rechnet in UTC**, auch wo keine Zone dabeisteht (`buildTimezone`
    sagt es für die ganze Datei). Wer das mit `time.mktime` liest, verschiebt
    jede Angabe um den eigenen Zeitunterschied — in Deutschland um ein bis zwei
    Stunden. Ein Vorfall von vor zehn Minuten stünde dann als „in zwei Stunden"
    da. Deshalb `calendar.timegm`, das ausdrücklich UTC liest.

    cState schreibt außerdem mehrere Formate in dieselbe Datei: mit Zone
    (`+0000 UTC`), mit doppelter Zone, ganz ohne — und `buildTime` sogar **ohne
    Sekunden** (`18:30`). Deshalb sind die Sekunden im Muster wahlfrei.
    Scheitert das Lesen, gibt es lieber **keine** Zeit als eine falsche."""
    if not raw:
        return None
    m = re.match(r'(\d{4}-\d{2}-\d{2})[T ](\d{2}:\d{2})(?::(\d{2}))?',
                 str(raw).strip())
    if not m:
        return None
    day, hour, second = m.group(1), m.group(2), m.group(3) or '00'
    try:
        return calendar.timegm(
            time.strptime('%s %s:%s' % (day, hour, second), '%Y-%m-%d %H:%M:%S'))
    except Exception:
        return None


# ------------------------------------------------------------------- Abruf
def _cache_read():
    try:
        with open(pfade.app_datei(CACHE), encoding='utf-8') as f:
            return json.load(f)
    except Exception:
        return {}


def _cache_write(data):
    target = pfade.app_datei(CACHE)
    try:
        os.makedirs(os.path.dirname(target), exist_ok=True)
        temp = target + '.tmp'
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump(data, f, ensure_ascii=False)
        os.replace(temp, target)
    except Exception:
        pass


def stored_state():
    """Der zuletzt geholte Stand — **ohne** ins Netz zu gehen.

    Damit steht beim Öffnen der Seite sofort etwas da, während der frische
    Abruf noch läuft. Gab es nie einen Abruf, kommt `{}` zurück."""
    return (_cache_read().get('lage') or {})


def state(force=False, deadline=None):
    """Die Lage aller Systeme — aus dem Netz oder aus dem Zwischenspeicher.

    Rückgabe:

        {'gesamt': 'operational',
         'systeme': [{'name': 'Platform', 'status': 'operational',
                      'ampel': 'ok', 'farbe': '#008000', 'meldungen': [...]}, …],
         'geholt': 1756240000.0,      # wann zuletzt gefragt wurde
         'stand': 1756238000.0,       # wann CIG die Seite zuletzt geändert hat
         'quelle': 'https://status.robertsspaceindustries.com/'}

    Wirft nie. Ohne Netz gilt der letzte Stand; gab es nie einen, kommt
    `{}` zurück — dann zeigt die Oberfläche nichts an, statt zu raten."""
    stored = _cache_read()
    old = stored.get('lage') or {}
    # `frist` sagt, wie alt der gespeicherte Stand sein darf, bevor überhaupt
    # gefragt wird. Der Live-Takt setzt sie auf 0: Er fragt jede Minute, aber
    # **mit** ETag — unverändert antwortet der Server mit 304 und ohne Inhalt.
    # Das ist der billige Fall und darf deshalb oft passieren.
    limit = FRESH_SEC if deadline is None else deadline
    is_fresh = (time.time() - (old.get('geholt') or 0)) < limit
    if old and is_fresh and not force:
        return old

    try:
        # ⚠ Beim erzwungenen Abruf **ohne** ETag fragen. Sonst antwortet der
        # Server mit 304, und „jetzt nachsehen" liefert genau die Daten zurück,
        # die schon dastanden — der Knopf wirkt kaputt, obwohl alles läuft.
        data, etag = _fetch('/index.json',
                            None if force else stored.get('etag'))
    except Exception:
        # ⚠ **Sagen, dass es am Netz lag.** Vorher kam hier nur der alte Stand
        # zurück — oder `{}`, wenn es nie einen gab. Die Seite konnte „noch nie
        # abgerufen" und „gerade keine Verbindung" nicht auseinanderhalten und
        # bat, auf „Jetzt nachsehen" zu klicken. Ohne Internet führt dieser
        # Klick zu nichts, und der Nutzer sucht den Fehler bei sich.
        old = dict(old) if old else {}
        old['kein_netz'] = True
        return old

    if data is None:                   # 304 — unverändert, nur die Uhr stellen
        if old:
            old['geholt'] = time.time()
            stored['lage'] = old
            _cache_write(stored)
        return old

    colors = {
        'ok': data.get('colorOk') or COLORS['ok'],
        'gestoert': data.get('colorDisrupted') or COLORS['gestoert'],
        'aus': data.get('colorDown') or COLORS['aus'],
        'hinweis': data.get('colorNotice') or COLORS['hinweis'],
    }
    sys_list = []
    for s in data.get('systems') or []:
        condition = (s.get('status') or '').strip()
        light = LIGHTS.get(condition, 'gestoert')
        sys_list.append({
            'name': s.get('name') or '?',
            'status': condition,          # im Wortlaut von CIG, nie übersetzt
            'ampel': light,
            'farbe': colors[light],
            'meldungen': [_incident_short(i) for i in (s.get('unresolvedIssues') or [])],
        })

    fresh = {
        'gesamt': (data.get('summaryStatus') or '').strip(),
        'systeme': sys_list,
        'geholt': time.time(),
        'stand': _timestamp('%s %s' % (data.get('buildDate') or '',
                                         data.get('buildTime') or '')),
        'quelle': BASE + '/',
    }
    _cache_write({'etag': etag, 'lage': fresh})
    return fresh


def ask():
    """Ein Blick, ob sich etwas geändert hat — für den laufenden Takt.

    Fragt **mit** ETag. Hat CIG nichts angefasst, kommt ein 304 ohne Inhalt
    zurück; das kostet kaum etwas und darf deshalb jede Minute passieren. Nur
    wenn sich wirklich etwas geändert hat, wird gelesen und gespeichert.

    Gibt `(lage, veraendert)` zurück — `veraendert` sagt der Oberfläche, ob sie
    überhaupt neu zeichnen muss. Ohne das würde die Anzeige jede Minute
    zerlegt und neu aufgebaut, obwohl sich nichts getan hat: Wer gerade eine
    Meldung liest, verlöre dabei seine Rollposition."""
    before = stored_state()
    fresh = state(deadline=0)
    return fresh, _core(fresh) != _core(before)


def _core(state_):
    """Woran man erkennt, ob sich inhaltlich etwas geändert hat.

    Bewusst **ohne** `geholt` — das ändert sich bei jedem Blick und würde jede
    Nachfrage als Änderung ausgeben."""
    if not state_:
        return None
    return (state_.get('gesamt'),
            tuple((s.get('name'), s.get('status'),
                   tuple(sorted(m.get('titel') or '' for m in s.get('meldungen') or [])))
                  for s in state_.get('systeme') or []))


def _incident_short(raw):
    """Die Angaben zu einem Vorfall, wie sie in der Systemliste mitkommen."""
    return {
        'titel': raw.get('title') or '',
        'schwere': (raw.get('severity') or '').strip(),
        'betroffen': list(raw.get('affected') or []),
        'begonnen': _timestamp(raw.get('createdAt')),
        'erledigt': _timestamp(raw.get('resolvedAt')),
        'datei': raw.get('filename') or '',
        'adresse': raw.get('permalink') or '',
    }


def incident(file_name):
    """Ein Vorfall im Volltext — Meldung samt Update-Zeilen.

    `datei` ist der Dateiname aus der Übersicht (`2026-08-26_live-deployment.md`).
    Die Endung fällt weg, der Rest ist der Ordner unter `/issues/`."""
    name = re.sub(r'\.md$', '', (file_name or '').strip())
    if not name:
        return {}
    try:
        data, _ = _fetch('/issues/%s/index.json' % name)
    except Exception:
        return {}
    if not data:
        return {}
    e = _incident_short(data)
    e['zeilen'] = _text_from_html(data.get('body'))
    return e


def messages(months=2, at_most=12):
    """Die Meldungen der letzten Monate — im Volltext, wie auf der Statusseite.

    Die Seite zeigt unter „Latest incidents" **auch erledigte** Vorfälle. Das ist
    der eigentliche Nutzen: Wer abends nicht ins Spiel kommt, will sehen, ob es
    heute Nachmittag eine Wartung gab — nicht nur, ob gerade eine läuft.

    ⚠ **Jeder Volltext ist ein eigener Abruf.** Deshalb werden sie
    zwischengespeichert und nur einmal geholt: Eine erledigte Meldung von
    vorletzter Woche ändert sich nicht mehr. Ohne den Zwischenspeicher liefen
    bei jedem Öffnen des Reiters ein Dutzend Abrufe los.

    Zwei Monate sind Absicht, nicht die ganze Historie: Sie liegt vollständig
    unter der verlinkten Adresse, und 265 Vorfälle im Fenster hülfen niemandem."""
    limit = time.time() - months * 30 * 86400
    between = _cache_read()
    full_texts = between.get('volltexte') or {}
    result, freshly_fetched = [], False

    for short in history(60):
        when = short.get('begonnen') or 0
        if when and when < limit:
            break                      # die Liste ist nach Datum sortiert
        file_name = short.get('datei') or ''
        if file_name in full_texts:
            full = full_texts[file_name]
        else:
            full = incident(file_name)
            if full:
                # Nur Erledigtes darf dauerhaft liegen bleiben. Eine offene
                # Meldung bekommt weitere Update-Zeilen — die würden wir sonst
                # nie wieder sehen.
                if full.get('erledigt'):
                    full_texts[file_name] = full
                    freshly_fetched = True
        if full:
            result.append(full)
        if len(result) >= at_most:
            break

    if freshly_fetched:
        between['volltexte'] = full_texts
        _cache_write(between)
    return result


def history(at_most=20):
    """Die letzten Vorfälle — neueste zuerst.

    Gedacht zum Nachsehen („war gestern etwas?") und als Prüfstoff: Solange
    alles läuft, gibt es keine offene Meldung, mit der sich die Anzeige testen
    ließe. Ein alter Vorfall füllt diese Lücke."""
    try:
        data, _ = _fetch('/issues/index.json')
    except Exception:
        return []
    page_list = (data or {}).get('pages') or []
    return [_incident_short(s) for s in page_list[:at_most]]
