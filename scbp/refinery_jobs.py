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
Laufende Raffinerie-Aufträge mit ihrer Endzeit.

Das Spiel schreibt dazu nichts ins Log. Die Endzeit kommt aus dem Scan des
Terminals (`refinery_scan`): Restzeit der Karte plus Zeitpunkt des Scans.
Danach läuft die Zeit ohne weiteren Scan weiter, auch über einen Neustart.

Ein Auftrag ist an seinen Rohstoffen und seiner Ausbeute zu erkennen. Ein
neuer Scan desselben Auftrags ersetzt die Endzeit; liest er ihn als fertig,
gilt er ab sofort als fertig. Fertige Aufträge fallen nach `KEEP_DONE_SEC`
aus der Liste. Das Overlay meldet jeden abgelaufenen Auftrag einmal
(`announce_due`, Merkmal `gemeldet`).
"""
import json
import os
import time

from . import paths

FILE = 'raffinerie-auftraege.json'
KEEP_DONE_SEC = 24 * 3600


def _file():
    return paths.app_file(FILE)


def load():
    """Alle gemerkten Aufträge: `[{'materials', 'total', 'ends'}]`."""
    try:
        with open(_file(), encoding='utf-8') as f:
            data = json.load(f)
    except (OSError, ValueError):
        return []
    jobs = data.get('auftraege') if isinstance(data, dict) else None
    return [j for j in jobs or () if isinstance(j, dict)
            and isinstance(j.get('ends'), (int, float))]


def _save(jobs):
    target = _file()
    tmp = target + '.neu'
    try:
        with open(tmp, 'w', encoding='utf-8') as f:
            json.dump({'version': 1, 'auftraege': jobs}, f,
                      ensure_ascii=False, indent=1)
        os.replace(tmp, target)
        return True
    except OSError as exc:
        from . import errors
        errors.record('refinery_jobs.save', exc)
        return False


def _key(job):
    return (tuple(m.lower() for m in job.get('materials') or ()),
            job.get('total') or '')


def merge(stored, scanned, now):
    """Gemerkte Aufträge mit einem Scan zusammenführen — ohne Platte.

    Laufende mit gelesener Restzeit bekommen `ends = now + Restzeit`, als
    fertig gelesene `ends = now` (wenn sie schon gemerkt waren). Was länger
    als `KEEP_DONE_SEC` fertig ist, fällt weg."""
    by_key = {_key(j): dict(j) for j in stored or ()}
    for job in scanned or ():
        key = _key(job)
        if not key[0]:
            continue
        if job.get('remaining') is not None and job.get('state') != 'fertig':
            by_key[key] = {'materials': list(job.get('materials') or ()),
                           'total': job.get('total') or '',
                           'ends': now + job['remaining']}
        elif job.get('state') == 'fertig' and key in by_key:
            by_key[key]['ends'] = min(by_key[key]['ends'], now)
            # Wer ihn am Terminal fertig sieht, braucht keine Meldung mehr.
            by_key[key]['gemeldet'] = True
    kept = [j for j in by_key.values() if now - j['ends'] <= KEEP_DONE_SEC]
    kept.sort(key=lambda j: j['ends'])
    return kept


def remember(scanned, now=None):
    """Einen Scan übernehmen und speichern. Gibt die neue Liste zurück."""
    now = time.time() if now is None else now
    if not any(j.get('remaining') is not None or j.get('state') == 'fertig'
               for j in scanned or ()):
        return load()
    jobs = merge(load(), scanned, now)
    _save(jobs)
    return jobs


def due(stored, now):
    """`(fällig, alle)` — fällig sind abgelaufene Aufträge ohne `gemeldet`;
    in `alle` tragen sie danach `gemeldet: True`. Ohne Platte."""
    found, jobs = [], []
    for job in stored or ():
        job = dict(job)
        if job['ends'] <= now and not job.get('gemeldet'):
            job['gemeldet'] = True
            found.append(job)
        jobs.append(job)
    return found, jobs


def announce_due(now=None):
    """Die seit dem letzten Aufruf fertig gewordenen Aufträge — jeder genau
    einmal, auch über einen Neustart hinweg."""
    now = time.time() if now is None else now
    found, jobs = due(load(), now)
    if found:
        _save(jobs)
    return found


def current(now=None):
    """`(laufend, fertig)` — je Liste von Aufträgen, laufende nach Endzeit."""
    now = time.time() if now is None else now
    jobs = merge(load(), (), now)
    return ([j for j in jobs if j['ends'] > now],
            [j for j in jobs if j['ends'] <= now])
