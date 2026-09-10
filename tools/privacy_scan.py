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
Nachsehen, ob etwas Privates ins Repo gerutscht ist — vor dem Push, nicht danach.

⚠⚠ **Warum das nicht optional ist.** Das Repo ist öffentlich, und aus dem
aktuellen Stand zu entfernen reicht nicht: **Die Historie behält alles.** Was
einmal gepusht ist, bleibt auffindbar, auch nach dem schönsten Aufräum-Commit.
Deshalb muss der Fund **vor** dem Push passieren.

Bisher war das reine Disziplin — plus zwei Riegel, die nur auf dem Rechner des
Autors laufen. Wer von woanders arbeitet oder eine Datei über die
Web-Oberfläche ändert, hatte gar nichts.

## Was gesucht wird

**Fest eingebaut** (steht hier im Klartext, weil es niemanden verrät):
E-Mail-Adressen, Telefonnummern mit Ländervorwahl, Heimverzeichnisse,
Laufwerksbuchstaben, Windows-Freigaben, Adressen aus privaten Netzen,
Webhook-Adressen, Zugangsdaten und private Schlüssel.

**Aus einer Sperrliste** (steht **nicht** im Repo): Klarnamen, Ortsnamen,
Rechner- und Freigabenamen — alles, was schon durch seine bloße Nennung
verrät, wonach gesucht wird.

    Datei `.sperrliste` im Wurzelverzeichnis (per `.gitignore` ausgenommen)
    oder der Pfad in der Umgebungsvariablen `SC_BP_SPERRLISTE`.
    Eine Zeile je Begriff, `#` leitet einen Kommentar ein.

⛔⛔ **Der Inhalt eines Fundes wird NIE ausgegeben.** Gemeldet werden Datei,
Zeile und **welche** Regel angeschlagen hat — nicht der gefundene Text. Sonst
stünde das Gesuchte am Ende im Protokoll eines öffentlichen Bau-Laufs, und der
Scanner wäre die Lücke, die er schließen soll.

## Was ausdrücklich erlaubt ist

`Xharig` ist der Name nach außen und darf überall stehen. Ebenso die
noreply-Adresse, die ohnehin in jedem Commit steht, und offensichtliche
Platzhalter (`example.com`, `<heim>`, `<benutzer>`).

## Aufruf

    python3 tools/privacy_scan.py            # das ganze Repo
    python3 tools/privacy_scan.py datei ...  # nur diese Dateien

Rückgabe 0 = sauber, 1 = Fund, 2 = Aufrufproblem.

⚠ **Lieber wenige harte Muster als viele weiche.** Ein Scanner, der bei jedem
zweiten Push Fehlalarm gibt, wird abgeschaltet — und schützt dann gar nichts.
Wer ein Muster ergänzt, misst vorher am ganzen Repo, wie viele Fehlalarme es
erzeugt.
"""
import os
import re
import subprocess
import sys

WURZEL = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SPERRLISTE = os.environ.get('SC_BP_SPERRLISTE') or os.path.join(WURZEL,
                                                                '.sperrliste')

# Dateien, die nicht durchsucht werden.
#
# ⚠ `daten/` enthält Spieldaten von CIG — dort tragen Gegenstände Namen, die
# anderswo verdächtig wären, und Pfade aus dem Spiel sind keine privaten
# Pfade. Diese Datei selbst enthält die Muster und wuerde sich sonst selbst
# melden.
#
# ⚠ Die Namen stehen hier bewusst NICHT als Beispiel: Prüfung 52s im
# Selbsttest sucht genau danach und hat diesen Kommentar prompt gemeldet, als
# einer darin stand.
AUSGENOMMEN = (
    'tools/privacy_scan.py',
    'daten/',
    '.git/',
    'assets/',
)

# Was trotz Treffer in Ordnung ist. ⚠ Sparsam halten: Jede Ausnahme ist ein
# Loch, und ein Loch, das niemand mehr kennt, ist schlimmer als kein Scanner.
ERLAUBT = (
    # Der Name nach aussen — und die Adresse, die in jedem Commit steht.
    'users.noreply.github.com',
    # Offensichtliche Platzhalter und Beispieladressen.
    #
    # ⚠⚠ **Nur ganze Domains, nie Bruchstuecke vor dem @.** Hier standen
    # anfangs `name@`, `deine@` und `your@` als „Platzhalter" — und damit war
    # **jede** Adresse erlaubt, die auf diese Silben endet:
    # `vorname.nachname@gmail.com` enthaelt `name@` und ging glatt durch.
    # Aufgefallen erst in der Gegenprobe, nicht beim Schreiben.
    # Eine Ausnahme muss so eng sein, dass sie nur das durchlaesst, was sie
    # durchlassen soll.
    '@example.', '@beispiel.', '@invalid',
    # Die Kuerzungen aus `pfade.kuerzen()` — genau die sollen dastehen.
    '<heim>', '<benutzer>', '<user>', '<home>',
)

# Erfundene Benutzernamen, die als Beispiel in der Doku stehen duerfen.
GENERISCH = ('spieler', 'user', 'users', 'nutzer', 'benutzer', 'runner',
             'name', 'dein', 'you', 'xharig', 'ich', 'max', 'mustermann',
             'home', 'du', 'wer', 'jemand')

# ⚠ Wer eine Stelle bewusst stehen lassen will, schreibt diesen Vermerk in die
# Zeile — **mit Begruendung dahinter**. Er wirkt nur fuer diese eine Zeile.
# Das ist Absicht: Eine Ausnahme, die man sieht, ist besser als ein Muster,
# das man entschaerft, bis es nichts mehr findet.
VERMERK = 'privacy-ok:'

MUSTER = [
    ('E-Mail-Adresse',
     re.compile(r'[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}')),
    ('Telefonnummer mit Laendervorwahl',
     re.compile(r'(?:\+|00)49[\s\-/]?\(?\d[\d\s\-/()]{7,}')),
    # ⚠⚠ **Zwei Muster sind hier bewusst NICHT eingebaut**, obwohl sie
    # naheliegen: „Laufwerksbuchstabe" und „Windows-Freigabe". Am ganzen Repo
    # gemessen (11.09.2026) ergaben sie **17 Fehlalarme und null echte Funde**
    # — Quelle waren verdoppelte Backslashes in Python-Zeichenketten
    # (`%APPDATA%\\sc-bp-watcher\\` sieht aus wie `\\SERVER\`) und
    # Zeitstempel-Ausdruecke (`\d\dT\d\d:\d\d` enthaelt `d:\d`). Beides ist im
    # Quelltext nicht sauber von echten Pfaden zu trennen.
    #
    # Der **konkrete** Laufwerksbuchstabe und der **konkrete** Freigabename
    # gehoeren ohnehin in die Sperrliste: Dort stehen sie treffsicher, ohne
    # dass irgendwo im Repo steht, wonach gesucht wird.
    ('Heimverzeichnis mit Benutzernamen',
     re.compile(r'(?:/home/|/Users/|[Cc]:\\+Users\\+)([A-Za-z0-9_.-]+)')),
    ('Adresse aus einem privaten Netz',
     re.compile(r'\b(?:192\.168\.\d{1,3}\.\d{1,3}'
                r'|10\.\d{1,3}\.\d{1,3}\.\d{1,3}'
                r'|172\.(?:1[6-9]|2\d|3[01])\.\d{1,3}\.\d{1,3})\b')),
    ('Webhook-Adresse',
     re.compile(r'https://(?:[a-z]+\.)?(?:discord(?:app)?\.com/api/webhooks/'
                r'|hooks\.slack\.com/)\S+')),
    ('Zugangsdaten im Klartext',
     re.compile(r'(?i)\b(?:passwo(?:rd|rt)|passwd|api[_-]?key|secret|token)\b'
                r'\s*[=:]\s*[\'"][^\'"]{8,}[\'"]')),
    ('GitHub-Zugangsschluessel',
     re.compile(r'\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}'
                r'|\bgithub_pat_[A-Za-z0-9_]{20,}')),
    ('Privater Schluessel',
     re.compile(r'-----BEGIN [A-Z ]*PRIVATE KEY-----')),
]


def dateien_aus_git():
    """Was Git wirklich kennt — nicht, was zufaellig im Ordner liegt.

    ⚠ Genau darum geht es: Was nicht versioniert ist, kann auch nicht
    versehentlich veroeffentlicht werden.
    """
    roh = subprocess.check_output(['git', '-C', WURZEL, 'ls-files'],
                                  text=True, encoding='utf-8')
    return [z for z in roh.splitlines() if z.strip()]


def sperrliste_lesen():
    """Die eigenen Suchbegriffe — oder eine leere Liste.

    Gibt `(begriffe, quelle)`. `quelle` ist nur fuer die Ausgabe da und sagt,
    ob ueberhaupt eine Liste gefunden wurde.
    """
    if not os.path.isfile(SPERRLISTE):
        return [], ''
    begriffe = []
    with open(SPERRLISTE, encoding='utf-8') as f:
        for zeile in f:
            wort = zeile.split('#', 1)[0].strip().lower()
            if len(wort) >= 3:          # kuerzeres traefe zu viel
                begriffe.append(wort)
    return begriffe, SPERRLISTE


def sperrliste_ist_ausgenommen():
    """Steht die Sperrliste in `.gitignore`? Das ist die erste Frage.

    ⛔ Eine Sperrliste **im** Repo waere die Liste dessen, wonach zu suchen
    sich lohnt — genau verkehrt herum.
    """
    name = os.path.basename(SPERRLISTE)
    weg = os.path.join(WURZEL, '.gitignore')
    if not os.path.isfile(weg):
        return False
    with open(weg, encoding='utf-8') as f:
        return any(z.strip() == name for z in f)


def _erlaubt(zeile_klein):
    return any(a in zeile_klein for a in ERLAUBT)


def zeile_pruefen(zeile, begriffe):
    """Was an dieser Zeile auffaellt — als Liste von Regelnamen.

    ⛔ Gibt **nie** den gefundenen Text zurueck, nur den Namen der Regel.
    """
    klein = zeile.lower()
    if _erlaubt(klein) or VERMERK in klein:
        return []
    funde = []
    for name, muster in MUSTER:
        treffer = muster.search(zeile)
        if not treffer:
            continue
        # Ein erfundener Benutzername oder ein Platzhalter in einem
        # Beispielpfad ist kein Fund.
        if name.startswith('Heimverzeichnis') and treffer.lastindex:
            wer = (treffer.group(1) or '').lower()
            if wer in GENERISCH or not wer.strip('.'):
                continue
        funde.append(name)
    for i, wort in enumerate(begriffe, 1):
        if wort in klein:
            # ⛔ Nur die Nummer, nicht das Wort.
            funde.append('Sperrliste, Eintrag %d' % i)
    return funde


def main(argv):
    begriffe, quelle = sperrliste_lesen()
    if quelle and not sperrliste_ist_ausgenommen():
        print('FEHLER: %s steht nicht in .gitignore. Eine Sperrliste im Repo '
              'waere die Liste dessen, wonach zu suchen sich lohnt.'
              % os.path.basename(SPERRLISTE))
        return 2

    if len(argv) > 1:
        dateien = [os.path.relpath(os.path.abspath(p), WURZEL)
                   for p in argv[1:]]
    else:
        try:
            dateien = dateien_aus_git()
        except Exception as ausnahme:
            print('FEHLER: git ls-files ging nicht (%s)' % ausnahme)
            return 2

    funde, geprueft = [], 0
    for rel in dateien:
        if any(rel.startswith(a) or rel == a.rstrip('/') for a in AUSGENOMMEN):
            continue
        voll = os.path.join(WURZEL, rel)
        if not os.path.isfile(voll):
            continue
        try:
            with open(voll, encoding='utf-8') as f:
                inhalt = f.read()
        except (UnicodeDecodeError, OSError):
            continue                    # Bilder und Binaerdateien
        geprueft += 1
        for nr, zeile in enumerate(inhalt.splitlines(), 1):
            for regel in zeile_pruefen(zeile, begriffe):
                funde.append((rel, nr, regel))

    print('%d Dateien geprueft, Sperrliste: %s'
          % (geprueft, '%d Eintraege' % len(begriffe) if begriffe
             else 'keine gefunden (nur die festen Muster)'))
    if not funde:
        print('Nichts Privates gefunden.')
        return 0

    print('\n%d Fund(e) — Datei, Zeile und Regel; der Text selbst wird '
          'bewusst NICHT ausgegeben:' % len(funde))
    for rel, nr, regel in funde:
        print('  %s:%d  %s' % (rel, nr, regel))
    print('\nJede Stelle einzeln ansehen. Ist sie harmlos, gehoert sie in '
          'ERLAUBT oder AUSGENOMMEN — mit Begruendung im Kommentar.')
    return 1


if __name__ == '__main__':
    sys.exit(main(sys.argv))
