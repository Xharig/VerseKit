# -*- coding: utf-8 -*-
"""Lässt die Release-Dateien von VirusTotal prüfen und hängt das Ergebnis an
die Release-Notiz.

**Aufruf** (aus dem Bau-Ablauf, `VT_API_KEY` als Secret):

    python3 .github/scripts/virustotal.py v4.10.0 dateien/windows/VerseKit-Setup.exe …

Ablauf je Datei: hochladen (`POST /files`, über 32 MB erst
`GET /files/upload_url`), dann die Analyse abfragen, bis sie fertig ist —
höchstens `WAIT_TOTAL` Sekunden, mit `PAUSE` Sekunden zwischen zwei Abfragen
(die öffentliche API erlaubt vier je Minute).

⚠ **Das Skript endet immer mit 0.** Ohne Schlüssel, bei einem Fehler von
VirusTotal oder bei Zeitüberschreitung steht ein Hinweis im Log, das Release
bleibt grün.

⚠ **Der Schlüssel geht nur als Kopfzeile `x-apikey` hinaus.** Er steht in
keiner Ausgabe; Fehlertexte laufen durch `_clean()`.

⚠⚠ **Die angehängte Zeile ist keine Aufzählung und hat keine Überschrift.**
`scbp/updater.py` liest den Release-Text, und `points_by_kind()` macht aus
jeder Zeile „- …" einen Punkt der zuletzt gesehenen Art — ein
VirusTotal-Ergebnis stünde in „Was ist neu" sonst als Fehlerbehebung.
"""

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import uuid

API = 'https://www.virustotal.com/api/v3'
GUI = 'https://www.virustotal.com/gui/file/'
DIRECT_LIMIT = 32 * 1024 * 1024
WAIT_TOTAL = 600
PAUSE = 31
MARK = '<sub>**VirusTotal**'


class _Api:
    """Abrufe bei VirusTotal mit Abstand zwischen zwei Anfragen."""

    def __init__(self, key, opener=urllib.request.urlopen, sleep=time.sleep):
        self.key = key
        self.opener = opener
        self.sleep = sleep
        self.calls = 0

    def call(self, url, data=None, content_type=None, method=None):
        if self.calls:
            self.sleep(PAUSE)
        self.calls += 1
        request = urllib.request.Request(url, data=data, method=method)
        request.add_header('x-apikey', self.key)
        request.add_header('accept', 'application/json')
        if content_type:
            request.add_header('Content-Type', content_type)
        with self.opener(request, timeout=300) as response:
            return json.loads(response.read().decode('utf-8'))


def _clean(exc, key):
    """Ein Fehlertext ohne Schlüssel — bei HTTP-Fehlern nur Code und Grund."""
    if isinstance(exc, urllib.error.HTTPError):
        text = 'HTTP %s %s' % (exc.code, exc.reason)
    else:
        text = '%s: %s' % (type(exc).__name__, exc)
    if key:
        text = text.replace(key, '***')
    return text


def sha256(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as f:
        for block in iter(lambda: f.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def _multipart(path):
    boundary = uuid.uuid4().hex
    head = ('--%s\r\nContent-Disposition: form-data; name="file"; '
            'filename="%s"\r\nContent-Type: application/octet-stream\r\n\r\n'
            % (boundary, os.path.basename(path))).encode('utf-8')
    tail = ('\r\n--%s--\r\n' % boundary).encode('utf-8')
    with open(path, 'rb') as f:
        body = head + f.read() + tail
    return body, 'multipart/form-data; boundary=' + boundary


def upload(api, path):
    """Datei hochladen; gibt die Kennung der Analyse."""
    url = API + '/files'
    if os.path.getsize(path) > DIRECT_LIMIT:
        url = api.call(API + '/files/upload_url')['data']
    body, content_type = _multipart(path)
    return api.call(url, data=body, content_type=content_type,
                    method='POST')['data']['id']


def wait(api, analysis_ids, clock=time.monotonic):
    """Analysen abfragen, bis alle fertig sind oder die Zeit um ist.

    Gibt {Kennung: stats} für die fertigen zurück.
    """
    done = {}
    deadline = clock() + WAIT_TOTAL
    while len(done) < len(analysis_ids) and clock() < deadline:
        for analysis_id in analysis_ids:
            if analysis_id in done or clock() >= deadline:
                continue
            attributes = api.call(API + '/analyses/' + analysis_id)[
                'data']['attributes']
            if attributes.get('status') == 'completed':
                done[analysis_id] = attributes.get('stats') or {}
    return done


def counts(stats):
    """(Treffer, geprüft): bösartig + verdächtig von allen, die geurteilt haben."""
    flagged = stats.get('malicious', 0) + stats.get('suspicious', 0)
    total = flagged + stats.get('undetected', 0) + stats.get('harmless', 0)
    return flagged, total


def summary_line(results):
    """Eine Zeile für die Release-Notiz, zweisprachig, ohne Aufzählung.

    `results`: Liste aus (Dateiname, sha256, stats oder None).
    """
    parts = []
    for name, digest, stats in results:
        if stats is None:
            verdict = 'Analyse läuft · analysis running'
        else:
            verdict = '%d/%d Treffer · hits' % counts(stats)
        parts.append('`%s` %s — [Bericht · report](%s%s)'
                     % (name, verdict, GUI, digest))
    return '%s (Virenprüfung · virus scan): %s</sub>' % (
        MARK, ' · '.join(parts))


def with_line(body, line):
    """Release-Text mit genau einer VirusTotal-Zeile am Ende."""
    kept = [z for z in (body or '').rstrip('\n').split('\n')
            if not z.startswith(MARK)]
    text = '\n'.join(kept).rstrip('\n')
    return (text + '\n\n' if text else '') + line + '\n'


def update_release(tag, line, run=subprocess.run):
    body = run(['gh', 'release', 'view', tag, '--json', 'body', '-q', '.body'],
               capture_output=True, text=True, check=True).stdout
    with tempfile.NamedTemporaryFile('w', suffix='.md', delete=False,
                                     encoding='utf-8') as f:
        f.write(with_line(body, line))
        path = f.name
    try:
        run(['gh', 'release', 'edit', tag, '--notes-file', path], check=True)
    finally:
        os.remove(path)


def main(argv, env=None, opener=urllib.request.urlopen, sleep=time.sleep,
         clock=time.monotonic, run=subprocess.run):
    env = os.environ if env is None else env
    key = (env.get('VT_API_KEY') or '').strip()
    if not key:
        print('VT_API_KEY nicht gesetzt — VirusTotal-Pruefung uebersprungen.')
        return 0
    if len(argv) < 2:
        print('VirusTotal: kein Tag oder keine Datei angegeben — uebersprungen.')
        return 0
    tag, paths = argv[0], argv[1:]
    try:
        api = _Api(key, opener=opener, sleep=sleep)
        ids = []
        for path in paths:
            ids.append(upload(api, path))
            print('VirusTotal: %s hochgeladen' % os.path.basename(path))
        done = wait(api, ids, clock=clock)
        results = [(os.path.basename(p), sha256(p), done.get(i))
                   for p, i in zip(paths, ids)]
        line = summary_line(results)
        print(line)
        update_release(tag, line, run=run)
        print('VirusTotal: Ergebnis an die Release-Notiz gehaengt.')
    except Exception as exc:
        print('VirusTotal: uebersprungen (%s)' % _clean(exc, key))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
