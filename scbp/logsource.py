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
Die Game.log lesen — laufend und rückwirkend.

Zwei Aufgaben:

  **Mitlesen.** Wie bisher: ab dem Startzeitpunkt die laufende Game.log
  verfolgen und neue Baupläne sofort melden.

  **Nachlesen.** Beim Start werden die aufgehobenen Logs
  vergangener Sitzungen (`logbackups/`) durchgesehen. Wer ohne laufenden
  Watcher gespielt hat, verliert dadurch nichts mehr. Was schon gelesen wurde,
  merkt sich der Lesestand — beim nächsten Start wird nicht alles erneut
  durchgekaut.

**Und die ehrliche Lücke:** Star Citizen hebt nur eine begrenzte Zahl alter Logs
auf. Liegt die letzte gelesene Sitzung weiter zurück als die älteste noch
vorhandene Sicherung, fehlt dazwischen etwas — das lässt sich nicht
zurückholen, aber sagen. Genau dafür ist `bericht['luecke']` da: Der Nutzer
soll die fehlenden Baupläne im Verwaltungsfenster von Hand abhaken können,
statt sich auf eine stille Untergrenze zu verlassen.
"""
import json
import os
import re
import time

from . import auftraege, pfade, phrasen
from .sprache import t, Satz, Zeitpunkt

# Schiffskomponenten stehen im Log MIT Zusatz „(Klasse/Size/Grade)", z. B.
# „7CA 'Nargun' (Civ/3/A)" — der Launcher-Schlüssel ist aber „7CA 'Nargun'".
# Bewusst eng gefasst (nur die bekannten Kürzel), damit echte Namens-Klammern
# wie „(30 cap)" oder „Singe Cannon (S2)" unangetastet bleiben.
#
# ⚠ **Die Liste muss zu `scbp/angaben.py` passen.** Seit v3.0.0 schreibt das
# Werkzeug diese Zusätze selbst an die Gegenstandsnamen (Angaben am
# Traktorstrahl) — und das Spiel schreibt den Namen anschließend **mitsamt
# Zusatz** in die Game.log. Wird hier einer nicht erkannt, landet der Bauplan
# unter falschem Namen im Bestand und wird **nie abgehakt**.
#
# Dazu kommen zwei Formen, die es beim Launcher nicht gab:
#   * **Striche** für Unbekanntes — `Glacis (Ind/4/–)`, `V60-26 (Mil/–/B)`
#   * **Waffen ohne Größe** — `P4-AR "Warhawk" Rifle (Bal)`; FPS-Waffen haben
#     in Star Citizen weder Größe noch Gütegrad
#   * **Raketen** — `'Arrow' I Missile (IR1)`, Suchkopf statt Fraktion
_ABBREV = ('Civ|Mil|Ind|Sth|Cmp'          # Fraktion, auch CIGs eigene Schreibweise
            '|Las|Ele|Pla|Dis|Mic|Bal'     # Waffenwirkung
            '|Nah|Min|Slv|Med|Tool|Trc')   # Nahkampf, Bergbau, Bergung, Medizin
_DASH = '\u2013|-'                        # Gedankenstrich oder Bindestrich
SUFFIX_RE = re.compile(
    r'\s*\((?:(%s)/(\d+|%s)/([A-D]|%s)'      # (Mil/1/A), auch mit Strichen
    r'|(%s)'                                 # (Bal) — Waffe ohne Größe/Grad
    r'|(IR|EM|CS)(\d{1,2}))\)\s*$'           # (IR1) — Rakete
    % (_ABBREV, _DASH, _DASH, _ABBREV), re.I)

# Wie viel einer Sicherung am Stück gelesen wird. Die Dateien werden mehrere
# hundert Megabyte groß; sie komplett in den Speicher zu holen wäre unnötig.
BLOCK = 4 * 1024 * 1024


def split_names(raw):
    """('7CA \\'Nargun\\'', ('Civ', '3', 'A'))  aus  "7CA 'Nargun' (Civ/3/A)".

    Zweiter Wert ist None, wenn kein Zusatz dranhing (FPS-Waffen, Rüstung)."""
    m = SUFFIX_RE.search(raw)
    if not m:
        return raw.strip(), None
    name = raw[:m.start()].strip()
    if m.group(1):                       # (Mil/1/A) — die vollständige Form
        return name, (m.group(1).title(), m.group(2), m.group(3).upper())
    if m.group(4):                       # (Bal) — nur die Klasse
        return name, (m.group(4).title(), None, None)
    return name, (m.group(5).upper(), m.group(6), None)   # (IR1) — Rakete


def _names_from_text(text, pattern):
    """Die Bauplan-Namen aus einem Textabschnitt.

    ⚠ **Die erste gefüllte Gruppe zählt, nicht stur Gruppe 1.** Seit
    `phrasen.muster()` auch umgestellte Formulierungen erkennt („%s ist
    eingetroffen"), kann der Ausdruck mehrere Klammergruppen haben — je
    Alternative eine. `m.group(1)` wäre bei einem Treffer der zweiten
    Alternative `None`.
    """
    out = []
    for m in pattern.finditer(text):
        for value in m.groups():
            if value:
                out.append(split_names(value))
                break
    return out


# ------------------------------------------------------------------ Lesestand
class ReadState:
    """Merkt sich, was schon gelesen wurde — über Programmneustarts hinweg."""

    def __init__(self):
        self.path = pfade.app_datei('logstand.json')
        self.data = self._load()

    def _load(self):
        try:
            with open(self.path, encoding='utf-8') as f:
                d = json.load(f)
            d.setdefault('aktiv', {})
            d.setdefault('sicherungen', {})
            d.setdefault('letzte_sitzung', 0.0)
            return d
        except Exception:
            return {'aktiv': {}, 'sicherungen': {}, 'letzte_sitzung': 0.0}

    def save(self):
        try:
            os.makedirs(os.path.dirname(self.path), exist_ok=True)
            temp = self.path + '.tmp'
            with open(temp, 'w', encoding='utf-8') as f:
                json.dump(self.data, f, ensure_ascii=False, indent=1)
            os.replace(temp, self.path)
        except Exception:
            pass

    # --- Sicherungen ---
    def knows(self, filename):
        """Wurde diese Sicherung schon gelesen? Erkannt an Name, Größe und Zeit —
        wächst eine Datei doch noch, gilt sie wieder als ungelesen."""
        e = self.data['sicherungen'].get(os.path.basename(filename))
        if not e:
            return False
        try:
            return (e.get('groesse') == os.path.getsize(filename)
                    and abs(e.get('mtime', 0) - os.path.getmtime(filename)) < 1)
        except OSError:
            return False

    def remember(self, filename):
        try:
            self.data['sicherungen'][os.path.basename(filename)] = {
                'groesse': os.path.getsize(filename),
                'mtime': os.path.getmtime(filename),
            }
            self.data['letzte_sitzung'] = max(
                self.data.get('letzte_sitzung', 0.0), os.path.getmtime(filename))
        except OSError:
            pass

    def cleanup(self, existing):
        """Einträge zu Sicherungen wegwerfen, die es nicht mehr gibt — sonst
        wächst die Datei mit jeder Spielsitzung weiter."""
        da = {os.path.basename(p) for p in existing}
        self.data['sicherungen'] = {k: v for k, v
                                     in self.data['sicherungen'].items() if k in da}

    # --- laufende Log ---
    def get_active(self, path):
        e = self.data['aktiv']
        return e.get('offset', 0) if e.get('pfad') == path else None

    def set_active(self, path, offset):
        self.data['aktiv'] = {'pfad': path, 'offset': offset, 'zeit': time.time()}
        self.data['letzte_sitzung'] = max(self.data.get('letzte_sitzung', 0.0),
                                           time.time())


# ------------------------------------------------------------------- Nachlese
def read_backlog(state=None, pattern=None, only_new=True, incl_running=True):
    """Die aufgehobenen Logs durchsehen.

    Rückgabe: (namen, bericht). `namen` ist eine Liste von (Name, Zusatz) —
    dieselbe Form, die auch das Mitlesen liefert, damit beide Wege im
    Hauptprogramm gleich behandelt werden können.

    `bericht` sagt, was passiert ist: wie viele Dateien gelesen wurden, ob eine
    Lücke bleibt und warum."""
    state = state or ReadState()
    pattern = pattern or phrasen.muster()
    all_names = pfade.log_sicherungen()
    # Vergleichswert VOR dem Lauf festhalten — `stand.merke()` schreibt ihn
    # gleich fort, danach ließe sich keine Lücke mehr erkennen.
    before = state.data.get('letzte_sitzung', 0.0)
    report = {'dateien': 0, 'uebersprungen': 0, 'gefunden': 0,
               'vorhanden': len(all_names), 'luecke': False, 'grund': '',
               'laufende': False, 'unlesbar': 0}

    match, seen = [], set()
    for filename in all_names:
        if only_new and state.knows(filename):
            report['uebersprungen'] += 1
            continue
        # ⚠ Eine einzige Datei darf den ganzen Lauf nicht kippen. `_lies_datei`
        # faengt `OSError` selbst ab — alles andere (unerwartete Ausnahme beim
        # Zerlegen, Speichernot bei einer riesigen Zeile) flog bis hierher
        # durch, und weiter bis in `_nachlese()`, das sie **still** verschluckt.
        # Dann wird `stand.speichern()` unten nie erreicht: Alle in diesem Lauf
        # gemerkten Dateien gelten wieder als ungelesen, die Nachlese beginnt
        # beim naechsten Start von vorn — jedes Mal, ohne dass jemand erfaehrt
        # warum. Deshalb hier abfangen und die eine Datei ueberspringen.
        try:
            finds = _read_file(filename, pattern)
        except Exception:
            # Bewusst NICHT merken: Eine Datei, die wir nicht lesen konnten,
            # muss beim naechsten Lauf wieder drankommen.
            report['unlesbar'] += 1
            continue
        for name, extra in finds:
            key = name.lower().strip()
            if key in seen:
                continue
            seen.add(key)
            match.append((name, extra))
        state.remember(filename)
        report['dateien'] += 1

    # Die laufende Game.log gehört mit dazu, wenn sie noch nie gelesen wurde:
    # Wer den Watcher startet, während Star Citizen schon läuft, hätte sonst
    # ausgerechnet die aktuelle Sitzung als Loch im Bestand. Danach steht der
    # Lesestand auf dem Dateiende, das Mitlesen setzt dort nahtlos an.
    if incl_running:
        # ⚠ Auch dieser Teil darf den Lauf nicht kippen — er steht NACH der
        # Schleife, also haette eine Ausnahme hier ausgerechnet die eben
        # gelesenen Sicherungen um ihren Eintrag gebracht.
        active = _safe_game_log()
        # ⚠ **Immer lesen, nicht nur beim allerersten Mal.** Hier stand
        # `if aktiv and stand.aktiv_holen(aktiv) is None:` — die laufende Datei
        # wurde also übersprungen, sobald sie einmal gelesen war. Das trifft
        # genau den Fall, den jeder für abgedeckt hält:
        #
        #   Watcher zu, Star Citizen läuft weiter, Baupläne kommen, Watcher
        #   später wieder auf.
        #
        # Dann steht der Lesestand irgendwo mitten in der Datei, das Mitlesen
        # setzt **dort** an, und alles davor ist für immer weg — es landet auch
        # nicht in `logbackups/`, denn dorthin wandert die Datei erst beim
        # nächsten Spielstart. Gemessen am 28.08.2026: Bauplan bei Byte
        # 11.987.664, Lesestand 12.759.872. Er wäre nie gefunden worden.
        #
        # Die Datei ganz zu lesen kostet bei 12 MB den Bruchteil einer Sekunde —
        # die Nachlese geht ohnehin über 149 Sicherungen. Doppelte fängt der
        # Bestand ab, der prüft jeden Namen.
        if active:
            try:
                for name, extra in _read_file(active, pattern):
                    key = name.lower().strip()
                    if key in seen:
                        continue
                    seen.add(key)
                    match.append((name, extra))
                report['laufende'] = True
            except Exception:
                report['unlesbar'] += 1
            try:
                state.set_active(active, os.path.getsize(active))
            except OSError:
                pass

    report['gefunden'] = len(match)
    try:
        report.update(_check_gap(before, all_names))
    except Exception:
        pass
    # ⚠ Das Festhalten steht am Ende und muss es auch erreichen — deshalb ist
    # oben alles abgefangen, was dazwischenkommen kann. Wird hier nicht
    # gespeichert, war der ganze Lauf umsonst: Beim naechsten Start wird alles
    # noch einmal gelesen, still und ohne erkennbaren Grund.
    try:
        state.cleanup(all_names)
    except Exception:
        pass
    state.save()
    return match, report


def _safe_game_log():
    """Die laufende `Game.log` — oder None, wenn der Pfad nicht zu holen ist.

    ⚠ `pfade.game_log()` sieht auf dem Dateisystem nach. Eine ausgehaengte
    Platte oder ein Netzpfad, der gerade nicht antwortet, hat den ganzen
    Nachlese-Lauf gekippt, samt der bereits gelesenen Sicherungen."""
    try:
        return pfade.game_log()
    except Exception:
        return None


def read_all(pattern=None):
    """Alle Protokolle noch einmal einlesen, auch die schon bekannten.

    Für den Fall, dass etwas fehlt: Der Lesestand wird ignoriert, jede Datei in
    `logbackups/` und die laufende `Game.log` werden vollständig durchgesehen.
    Danach steht der Stand wieder sauber am Dateiende.

    Gebraucht wird das, wenn der Lesestand weiter ist als der Bestand — etwa
    weil beim ersten Lauf die Spielsprache noch nicht erkannt war und die
    Protokolle mit der falschen Formulierung durchsucht wurden, oder nach einem
    Zurücksetzen des Bestands.

    Rückgabe wie `nachlesen()`: (Namen, Bericht)."""
    return read_backlog(state=ReadState(), pattern=pattern,
                     only_new=False, incl_running=True)


def _read_file(filename, pattern):
    """Eine ganze Logdatei blockweise durchsuchen."""
    found = []
    try:
        with open(filename, 'rb') as f:
            rest = b''
            while True:
                block = f.read(BLOCK)
                if not block:
                    break
                block = rest + block
                cut_at = block.rfind(b'\n')
                if cut_at < 0:            # eine sehr lange Zeile — weitersammeln
                    rest = block
                    continue
                rest = block[cut_at + 1:]
                text = block[:cut_at].decode('utf-8', 'ignore')
                found.extend(_names_from_text(text, pattern))
            if rest:
                found.extend(_names_from_text(rest.decode('utf-8', 'ignore'),
                                                pattern))
    except OSError:
        pass
    return found


def _check_gap(before, all_names):
    """Bleibt trotz Nachlese etwas unbekannt?

    Zwei Fälle sagen Ja:
      * **Erster Start überhaupt** — was vor der ältesten aufgehobenen Sicherung
        liegt, hat nie jemand gelesen. Das ist der Normalfall bei der ersten
        Benutzung und der Grund, warum es die Liste zum Abhaken gibt.
      * **Zu lange nicht gelaufen** — die älteste vorhandene Sicherung ist neuer
        als die zuletzt gelesene Sitzung. Dazwischen hat Star Citizen Logs
        weggeräumt, die niemand mehr hat."""
    # ⚠ Zurück kommt ein `Satz`, **kein fertiger Text**: Diese Meldung landet in
    # der Melde-Leiste und bleibt dort stehen. Ein fertig zusammengesetzter Satz
    # wäre in der Sprache von damals eingefroren — wer später umstellt, hätte
    # eine deutsche Zeile in einem englischen Fenster. Genau so gefunden am
    # 26.08.2026. Der `Satz` merkt sich Schlüssel und Werte und lässt sich beim
    # Sprachwechsel neu auswerten.
    if not all_names:
        return {'luecke': True, 'grund': Satz('m_keine_logs')}
    oldest = min((os.path.getmtime(p) for p in all_names
                     if os.path.exists(p)), default=0.0)
    if not before:
        return {'luecke': True,
                'grund': Satz('m_erster_lauf', Zeitpunkt(oldest))}
    if oldest > before + 60:
        return {'luecke': True,
                # ⚠ Auch das Datumsformat ist sprachabhängig: Im Englischen
                # steht das Jahr vorn (`m_erster_datum`). Deshalb wandert hier
                # der rohe Zeitstempel weiter (`Zeitpunkt`) statt eines fertig
                # formatierten Datums — sonst stünde in der englischen Meldung
                # ein deutsches Datum.
                'grund': Satz('m_luecke_logs',
                              Zeitpunkt(before), Zeitpunkt(oldest))}
    return {'luecke': False, 'grund': ''}


# ------------------------------------------------------------------- Mitlesen
class LogTail:
    """Liest die laufende Game.log fortlaufend weiter.

    Neu gegenüber v1.5.0: Der Lesestand überlebt einen Programmneustart. Wer den
    Watcher neu startet, während das Spiel läuft, verliert die Baupläne dieser
    Sitzung nicht mehr."""

    def __init__(self, state=None, pattern=None):
        self.state = state or ReadState()
        self.pattern = pattern or phrasen.muster()
        self.path, self.offset = None, 0
        # Zweites Muster fuer angenommene Auftraege (ab v3.2.0). Wird von aussen
        # gesetzt; ist es None, aendert sich am Verhalten nichts.
        #
        # ⚠ Bewusst NICHT ueber den Rueckgabewert von `new_names()`: Den werten
        # mehrere Stellen aus (Watcher-Faden, Nachlese, Selbsttest). Eine zweite
        # Sorte Treffer hineinzumischen haette jede davon anfassen muessen —
        # und der Bauplan-Weg ist der Weg, der nie brechen darf.
        self.mission_pattern = None
        self.missions = []
        # Und die Gegenstuecke: abgeschlossen, zurueckgezogen, fehlgeschlagen.
        # ⚠ Ohne sie bliebe jeder Auftrag ewig stehen — nach einem Abend mit
        # zehn Auftraegen stuende eine Liste da, von der nichts mehr stimmt.
        self.mission_end_pattern = None
        self.missions_done = []
        # ⚠ Und dasselbe noch einmal **in der Reihenfolge des Logs**. Zwei
        # getrennte Listen verlieren, was zuerst kam: Steht in einem Abschnitt
        # erst die Annahme und danach der Abschluss — genau der Fall nach einem
        # Neustart des Watchers, der einen ganzen Abend nachliest —, dann nimmt
        # eine Auswertung „erst alle Enden, dann alle Annahmen" den Auftrag weg
        # und stellt ihn gleich wieder hin. Er stuende als frisch angenommen da,
        # obwohl er laengst erledigt ist. Genau so am 30.08.2026 gemessen.
        # Eintraege sind `(ist_annahme, titel, mission_id, objective_id)`
        # — die beiden Kennungen entscheiden, ob ein Ende den Auftrag
        # meint oder nur ein Zwischenziel (siehe `auftraege.ZUSATZ`).
        self.mission_events = []
        # Und die Zwischenziele desselben Abschnitts — was gerade zu tun ist.
        # ⚠ Zwei Sorten in einer Liste, roh: Zustandswechsel und Wortlaut.
        # Gewertet wird in `auftraege.Ziele`, damit Start und laufender Betrieb
        # nicht wieder eigene Rechenwege bekommen.
        self.objective_events = []

    def _locate(self):
        p = pfade.game_log()
        if p and p != self.path:
            self.path = p
            remembered = self.state.get_active(p)
            try:
                size = os.path.getsize(p)
            except OSError:
                size = 0
            # ⚠ **Drei Fälle, und der mittlere hat Baupläne verschluckt.**
            #
            #   gemerkt is None      Die Datei wurde noch nie gelesen. Dann hat
            #                        `nachlesen()` sie eben von vorn durch und
            #                        den Stand ans Ende gesetzt — hier gilt das
            #                        Ende, sonst käme alles ein zweites Mal.
            #
            #   gemerkt > groesse    Die Datei ist **kürzer** als der Stand:
            #                        Star Citizen hat beim Neustart eine frische
            #                        Game.log angelegt. Alles darin ist neu →
            #                        **von vorn**.
            #
            #   sonst                Weiterlesen, wo aufgehört wurde.
            #
            # Bis v3.0.0 stand im zweiten Fall `groesse` statt `0`, also das
            # **Ende** der neuen Datei. Damit übersprang der Watcher jeden
            # Bauplan, den die frische Sitzung schon gemeldet hatte, und merkte
            # es nie: `new_names()` hat zwar dieselbe Regel richtig
            # (`if size < self.offset: self.offset = 0`), kommt aber nicht dazu
            # — der Stand steht dann längst auf dem Dateiende und
            # `size == self.offset` steigt sofort aus.
            #
            # Gemessen am 28.08.2026: Stand 12.759.872, Datei 12.758.651 Bytes.
            # Zwei Baupläne standen in der Log, einer fehlte im Bestand.
            if remembered is None:
                self.offset = size
            elif remembered > size:
                self.offset = 0
            else:
                self.offset = remembered
        elif not p:
            self.path = None
        return self.path

    def new_names(self):
        """Neue Baupläne seit dem letzten Aufruf — Liste von (Name, Zusatz)."""
        # ⚠ **Zuerst leeren, vor jedem Ausstieg.** Die drei Auftragslisten
        # gehoeren zu *diesem* Abschnitt. Blieben sie stehen, wenn nichts Neues
        # da ist, laese der Aufrufer sie ein zweites Mal — und der Watcher
        # meldete einen Auftrag von vorhin noch einmal als eben angenommen.
        # Der Bauplan-Weg hat das Problem nie gehabt, weil er seine Funde
        # zurueckgibt statt sie abzulegen.
        self.missions = []
        self.missions_done = []
        self.mission_events = []
        self.objective_events = []
        if not self._locate():
            return []
        try:
            size = os.path.getsize(self.path)
            if size < self.offset:          # Log rotiert -> neue Spielsitzung
                self.offset = 0
            if size == self.offset:
                return []
            with open(self.path, 'rb') as f:
                f.seek(self.offset)
                chunk = f.read()
        except OSError:
            return []
        cut = chunk.rfind(b'\n')            # angefangene letzte Zeile stehen lassen
        if cut < 0:
            return []
        self.offset += cut + 1
        self.state.set_active(self.path, self.offset)
        self.state.save()
        text = chunk[:cut].decode('utf-8', 'ignore')
        # Derselbe Textabschnitt, zweiter Blick: angenommene Auftraege.
        self.missions = (self.mission_pattern.findall(text)
                          if self.mission_pattern else [])
        # Und ein dritter: was in diesem Abschnitt zu Ende gegangen ist.
        self.missions_done = (self.mission_end_pattern.findall(text)
                                  if self.mission_end_pattern else [])
        self.mission_events = self._sort_events(text)
        # ⚠ Ohne Auftragsmuster gibt es auch keine Auftragsanzeige — dann
        # braucht niemand die Ziele, und das Suchen waere reine Arbeit.
        self.objective_events = (auftraege.ziel_ereignisse_aus_text(text)
                                if self.mission_pattern else [])
        return _names_from_text(text, self.pattern)

    def _sort_events(self, text):
        """Annahmen und Enden dieses Abschnitts in der Reihenfolge des Logs.

        ⚠ Das Auslesen selbst liegt in `auftraege.ereignisse_aus_text` — eine
        Stelle für beide Wege. Der Start rechnet dort über die ganze
        `Game.log`, der laufende Betrieb hier über den neuen Abschnitt; liefen
        die beiden auseinander, verschwände ein Auftrag beim Neustart oder
        stünde doppelt da.
        """
        if not self.mission_pattern and not self.mission_end_pattern:
            return []
        return auftraege.ereignisse_aus_text(text, self.mission_pattern,
                                             self.mission_end_pattern)


if __name__ == '__main__':
    finds, b = read_backlog()
    print('Sicherungen vorhanden:', b['vorhanden'],
          '· gelesen:', b['dateien'], '· übersprungen:', b['uebersprungen'])
    print('Baupläne gefunden:', b['gefunden'])
    if b['luecke']:
        print('LÜCKE:', b['grund'])
    for n, z in finds[:20]:
        print(' ·', n, z or '')
