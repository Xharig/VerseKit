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

from . import contracts, paths, phrases
from .language import t, Phrase, Moment

# Schiffskomponenten stehen im Log MIT Zusatz `(Klasse/Size/Grade)`, z. B.
# `7CA 'Nargun' (Civ/3/A)` — der Schlüssel ist aber `7CA 'Nargun'`.
# Bewusst eng gefasst (nur die bekannten Kürzel), damit echte Namens-Klammern
# wie `(30 cap)` oder `Singe Cannon (S2)` unangetastet bleiben.
#
# ⚠ **Die Liste muss zu `scbp/specs.py` passen.** Dort schreibt das
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
    `phrases.pattern()` auch umgestellte Formulierungen erkennt („%s ist
    eingetroffen"), kann der Ausdruck mehrere Klammergruppen haben — je
    Alternative eine. `m.group(1)` wäre bei einem Treffer der zweiten
    Alternative `None`.
    """
    out = []
    for m in pattern.finditer(text):
        for value in m.groups():
            if value:
                name, extra = split_names(value)
                # ⚠⚠ **Hier, nicht erst beim Anzeigen.** Konnte das Spiel den
                # Namen nicht übersetzen, steht in der Meldung der rohe
                # Schlüssel (`@Nozzle_…_Name`) — und der wäre im Bestand der
                # Schlüssel des Eintrags, also für immer drin. Siehe
                # `phrases.resolve_key`.
                out.append((phrases.resolve_key(name), extra))
                break
    return out


# ------------------------------------------------------------------ Account
# ⭐⭐ **Wem gehört eine Log?** (ab v3.57.0)
#
# Wer zwei Accounts auf einem Rechner spielt, hat die Protokolle BEIDER in
# `logbackups/` liegen — und bis hierher landeten die Baupläne des zweiten
# still im eigenen Bestand. Das Spiel schreibt beim Anmelden, wer es ist:
#
#   <Legacy login response> [CIG-net] User Login Success - Handle[Spieler] - …
#   <AccountLoginCharacterStatus_Character> Character: … - name Spieler - state STATE_CURRENT
#
# Beides steht in den ersten ~50 KB. Die `nickname="…"`-Zeilen werden bewusst
# NICHT genommen — in Verbindungszeilen können auch Mitspieler stehen.
#
# Der SC-Extractor des Profit Basetools (GPL-3.0) ordnet seine Protokolle
# genauso zu.
ACCOUNT_RES = (
    re.compile(r'User Login Success - Handle\[([^\]\r\n]+)\]'),
    re.compile(r' - name (\S+) - state STATE_CURRENT'),
)
# So weit wird am Dateianfang nach der Anmeldung gesucht. Sie steht nach
# ~50 KB; die Grenze ist großzügig, damit ein langsamer Start sie nicht
# verpasst, und begrenzt, damit 180 Sicherungen in Sekundenbruchteilen
# durch sind.
ACCOUNT_HEAD = 16 * 1024 * 1024
# ⭐ Die feste Kennung des Accounts. Sie steht in der Charakterzeile
#
#   <AccountLoginCharacterStatus_Character> Character: … - accountId 123 - name Spieler - state STATE_CURRENT
#
# und bleibt gleich, wenn der Spieler seinen Handle ändert. Ältere Protokolle
# haben die Zeile nicht; dort entscheidet weiter der Handle.
ACCOUNT_ID_RE = re.compile(r' - accountId (\d+) - name \S+ - state STATE_CURRENT')
# So weit hinter dem Handle wird noch nach der Kennung gesucht. Sie steht
# wenige KB davor; ohne Grenze würde jede alte Log ohne Kennung bis
# `ACCOUNT_HEAD` gelesen.
ACCOUNT_ID_REACH = 1024 * 1024
# Einstellung: der eigene Account. `*` heißt: alle Accounts zählen.
ACCOUNT_SETTING = 'eigener_account'
# Einstellung: die Kennung des eigenen Accounts. Gehört zum Handle in
# `ACCOUNT_SETTING`; wer den Account umstellt, leert sie (`set_own_account`).
ACCOUNT_ID_SETTING = 'eigene_account_kennung'
ALL_ACCOUNTS = '*'
_OWN_TRIED = [0.0]


def account_from_text(text):
    """Der Account aus einem Textabschnitt — oder None."""
    for rx in ACCOUNT_RES:
        m = rx.search(text)
        if m:
            return m.group(1).strip()
    return None


def identity_from_text(text):
    """(Handle, Kennung) aus einem Textabschnitt — fehlendes ist None."""
    m = ACCOUNT_ID_RE.search(text)
    return account_from_text(text), (m.group(1) if m else None)


def account_of_file(filename):
    """Der Account, zu dem eine Log gehört — oder None (noch kein Login,
    unlesbar). Gelesen wird nur der Anfang."""
    return identity_of_file(filename)[0]


def identity_of_file(filename):
    """(Handle, Kennung) einer Log — fehlendes ist None.

    ⭐ Gemerkt je Datei und Stand: Die Seite Erkennung und der Abgleich
    fremder Baupläne fragen das für alle rund 180 Sicherungen — bei jedem
    Besuch neu, 0,64 s beim ersten Öffnen (gemessen mit
    `tools/tempo_messen.py`). Eine Sicherung ändert sich nie; eine wachsende
    `Game.log` ändert Größe und Zeit und wird dann neu gelesen."""
    try:
        info = os.stat(filename)
        key = (filename, info.st_mtime_ns, info.st_size)
    except OSError:
        key = None
    if key is not None and key in _ACCOUNT_CACHE:
        return _ACCOUNT_CACHE[key]
    # ⭐ Auch über den Neustart hinweg (rc9): Sicherungen stehen mit ihrem
    # Account in `konten-je-log.json`. Beim ersten Öffnen nach dem Start
    # müssen dann nur neue Protokolle gelesen werden, nicht alle 180.
    # ⚠ Ein Eintrag mit nur drei Feldern hat noch keine Kennung und wird
    # einmal neu gelesen.
    stored = _stored_accounts()
    if key is not None:
        hit = stored.get(filename)
        if (hit and len(hit) >= 4 and hit[0] == key[1]
                and hit[1] == key[2]):
            found = (hit[2] or None, hit[3] or None)
            _ACCOUNT_CACHE[key] = found
            return found
    found = _identity_of_file(filename)
    if key is not None:
        if len(_ACCOUNT_CACHE) > 5000:
            _ACCOUNT_CACHE.clear()
        _ACCOUNT_CACHE[key] = found
        # Nur Sicherungen dauerhaft merken — die laufende `Game.log` ändert
        # sich ständig, ein Eintrag für sie wäre sofort veraltet.
        if ((found[0] or found[1])
                and os.path.basename(filename).lower() != 'game.log'):
            stored[filename] = [key[1], key[2], found[0], found[1]]
            _UNSAVED[0] += 1
            # Gebündelt schreiben: beim allerersten Lauf kämen sonst 180
            # Schreibvorgänge zusammen. Was zwischen zwei Sicherungen verloren
            # geht, wird beim nächsten Mal eben noch einmal gelesen.
            if _UNSAVED[0] >= 40 or time.time() - _LAST_SAVE[0] > 5:
                _save_accounts(stored)
    return found


_ACCOUNT_CACHE = {}
ACCOUNTS_FILE = 'konten-je-log.json'
_STORED = [None]


def _stored_accounts():
    """Die dauerhaft gemerkten Accounts je Sicherung — `{Pfad: [mtime_ns,
    Größe, Account]}`. Fehlt oder klemmt die Datei, ist es ein leerer Speicher,
    kein Fehler: Dann wird eben gelesen."""
    if _STORED[0] is None:
        try:
            with open(paths.app_file(ACCOUNTS_FILE), encoding='utf-8') as f:
                data = json.load(f)
            _STORED[0] = data if isinstance(data, dict) else {}
        except (OSError, ValueError):
            _STORED[0] = {}
    return _STORED[0]


_UNSAVED = [0]
_LAST_SAVE = [0.0]


def _save_accounts(stored):
    _UNSAVED[0] = 0
    _LAST_SAVE[0] = time.time()
    try:
        target = paths.app_file(ACCOUNTS_FILE)
        temp = target + '.tmp'
        with open(temp, 'w', encoding='utf-8') as f:
            json.dump(stored, f, ensure_ascii=False)
        paths.replace_file(temp, target)
    except OSError:
        pass                      # nur ein Zwischenspeicher — kein Schaden


def _identity_of_file(filename):
    """Die eigentliche Suche hinter `identity_of_file` — ohne Zwischenspeicher.

    Der Handle kommt aus dem ersten Block, in dem einer steht. Die Kennung
    wird danach höchstens `ACCOUNT_ID_REACH` weiter gesucht."""
    handle = account_id = None
    handle_at = 0
    try:
        with open(filename, 'rb') as f:
            read = 0
            rest = b''
            while read < ACCOUNT_HEAD:
                block = f.read(BLOCK)
                if not block:
                    break
                read += len(block)
                text = (rest + block).decode('utf-8', 'ignore')
                found_handle, found_id = identity_from_text(text)
                if handle is None and found_handle:
                    handle, handle_at = found_handle, read
                account_id = account_id or found_id
                if handle and (account_id or read - handle_at
                               >= ACCOUNT_ID_REACH):
                    break
                rest = block[-400:]          # eine Zeile über die Blockgrenze
    except OSError:
        pass
    return handle, account_id


def _all_log_files():
    files = list(paths.log_backups())
    active = _safe_game_log()
    if active:
        files.append(active)
    return files


def accounts_in_logs(files=None):
    """{Account: Anzahl Protokolle} über alle Sicherungen und die laufende Log.

    Handles mit derselben Kennung sind ein Account: Er steht unter dem
    Handle des neuesten Protokolls, mit den Protokollen aller zusammen."""
    counts = {}
    for count, _mtime, handle, _id in _account_groups(
            _identities(files)).values():
        if handle:
            counts[handle] = counts.get(handle, 0) + count
    return counts


def own_account(files=None):
    """Der eigene Account — `None`, wenn noch keiner zu erkennen ist,
    `ALL_ACCOUNTS`, wenn alle zählen sollen.

    ⚠ **Beim ersten Mal wird festgelegt, nicht jedes Mal neu geraten.** Genommen
    wird der Account mit den **meisten** Protokollen — nicht der der neuesten
    Log: Wer beim ersten Start gerade den Zweitaccount spielt, bekäme sonst
    dessen Bestand. Danach steht er in der Einstellung und ändert sich nur,
    wenn der Spieler ihn umstellt (Seite Erkennung)."""
    chosen = (paths.setting(ACCOUNT_SETTING) or '').strip()
    if chosen:
        return chosen
    # ⚠ Das Mitlesen fragt alle paar Sekunden. Ist noch kein Account zu
    # erkennen (frische Installation, noch nie eingeloggt), würde jede Frage alle
    # Sicherungen anfassen — deshalb höchstens alle fünf Minuten suchen.
    if time.time() - _OWN_TRIED[0] < 300:
        return None
    _OWN_TRIED[0] = time.time()
    best = _choose_own(_identities(files))
    if not best:
        return None
    paths.set_setting(ACCOUNT_SETTING, best[0])
    paths.set_setting(ACCOUNT_ID_SETTING, best[1] or '')
    return best[0]


def own_account_id():
    """Die Kennung des eigenen Accounts — oder None (noch nicht gelernt,
    alle Accounts zählen)."""
    if (paths.setting(ACCOUNT_SETTING) or '').strip() == ALL_ACCOUNTS:
        return None
    return (str(paths.setting(ACCOUNT_ID_SETTING) or '')).strip() or None


def set_own_account(value):
    """Den eigenen Account umstellen. Die Kennung gehört zum alten Handle und
    wird geleert und sofort für den neuen gelernt — sonst gälten bis zum
    nächsten Lesen die früheren Handles desselben Accounts als fremd."""
    paths.set_setting(ACCOUNT_SETTING, value)
    paths.set_setting(ACCOUNT_ID_SETTING, '')
    try:
        refresh_own_identity()
    except Exception as error:
        paths._report_error('logsource.set_own_account', error)


def _identities(files=None):
    """[(mtime, Handle, Kennung)] aller Protokolle mit Handle oder Kennung."""
    out = []
    for filename in (files if files is not None else _all_log_files()):
        handle, account_id = identity_of_file(filename)
        if not (handle or account_id):
            continue
        try:
            mtime = os.path.getmtime(filename)
        except OSError:
            mtime = 0.0
        out.append((mtime, handle, account_id))
    if _UNSAVED[0]:
        _save_accounts(_stored_accounts())     # den Rest des Durchlaufs sichern
    return out


def _choose_own(identities):
    """(Handle, Kennung) des Accounts mit den meisten Protokollen — oder None.
    Bei Gleichstand entscheidet die Sortierung, also fest."""
    groups = _account_groups(identities)
    usable = [k for k in sorted(groups) if groups[k][2]]
    if not usable:
        return None
    best = max(usable, key=lambda k: groups[k][0])
    return groups[best][2], groups[best][3]


def _account_groups(identities):
    """{Schlüssel: [Protokolle, neueste mtime, Handle, Kennung]} je Account.

    Protokolle mit derselben Kennung zählen zusammen, auch unter
    verschiedenen Handles; ein Protokoll ohne Kennung zählt zu der Kennung,
    die sein Handle anderswo trägt. Der Handle ist der des neuesten
    Protokolls."""
    handle_ids = {}
    for _mtime, handle, account_id in identities:
        if handle and account_id:
            handle_ids.setdefault(handle.lower(), account_id)
    groups = {}
    for mtime, handle, account_id in identities:
        account_id = account_id or (handle_ids.get(handle.lower())
                                    if handle else None)
        key = ('i:' + account_id) if account_id else ('h:' + handle.lower())
        group = groups.setdefault(key, [0, -1.0, None, account_id])
        group[0] += 1
        if handle and mtime >= group[1]:
            group[1], group[2] = mtime, handle
    return groups


def refresh_own_identity(files=None):
    """Kennung des eigenen Accounts lernen und den Handle nachziehen.

    Fehlt die Kennung, kommt sie aus den Protokollen mit dem gespeicherten
    Handle. Steht sie fest, gilt als Handle der des neuesten Protokolls mit
    dieser Kennung — so zählt nach einem Wechsel des Handles der neue."""
    own = (paths.setting(ACCOUNT_SETTING) or '').strip()
    if not own or own == ALL_ACCOUNTS:
        return
    identities = _identities(files)
    own_id = own_account_id()
    if not own_id:
        ids = {}
        for _mtime, handle, account_id in identities:
            if account_id and handle and handle.lower() == own.lower():
                ids[account_id] = ids.get(account_id, 0) + 1
        if not ids:
            return
        own_id = max(sorted(ids), key=lambda i: ids[i])
        paths.set_setting(ACCOUNT_ID_SETTING, own_id)
    newest = max(((mtime, handle) for mtime, handle, account_id in identities
                  if account_id == own_id and handle), default=None)
    if newest and newest[1] != own:
        paths.set_setting(ACCOUNT_SETTING, newest[1])


def note_identity(handle, account_id):
    """Was die laufende Log über ihren Account sagt, für den eigenen nutzen:
    Kennung lernen, wenn der Handle der eigene ist; Handle nachziehen, wenn
    die Kennung die eigene ist. Die laufende Log ist immer die neueste."""
    if not handle or not account_id:
        return
    own = (paths.setting(ACCOUNT_SETTING) or '').strip()
    if not own or own == ALL_ACCOUNTS:
        return
    own_id = own_account_id()
    if not own_id:
        if handle.lower() == own.lower():
            paths.set_setting(ACCOUNT_ID_SETTING, account_id)
            return
        # Fremder Handle und noch keine Kennung: aus den Sicherungen lernen,
        # höchstens alle fünf Minuten, denn das Mitlesen fragt alle paar
        # Sekunden.
        if time.time() - _REFRESH_TRIED[0] < 300:
            return
        _REFRESH_TRIED[0] = time.time()
        refresh_own_identity()
        own = (paths.setting(ACCOUNT_SETTING) or '').strip()
        own_id = own_account_id()
    if own_id and account_id == own_id and handle != own:
        paths.set_setting(ACCOUNT_SETTING, handle)


_REFRESH_TRIED = [0.0]


def _own_handles(own_id):
    """Alle Handles, unter denen die Kennung `own_id` gesehen wurde, klein."""
    out = set()
    for handle, account_id in list(_ACCOUNT_CACHE.values()):
        if handle and account_id == own_id:
            out.add(handle.lower())
    for hit in list(_stored_accounts().values()):
        if (isinstance(hit, list) and len(hit) >= 4 and hit[2]
                and hit[3] == own_id):
            out.add(str(hit[2]).lower())
    return out


def counts_for(own, account, account_id=None):
    """Zählt ein Protokoll dieses Accounts? Unbekannt zählt — lieber einen
    Bauplan zu viel als einen verlorenen.

    Kennen Protokoll und Einstellung die Kennung, entscheidet sie. Sonst der
    Handle — auch ein früherer Handle des eigenen Accounts."""
    if not own or own == ALL_ACCOUNTS or not (account or account_id):
        return True
    own_id = own_account_id()
    if own_id and account_id:
        return str(account_id) == own_id
    if not account:
        return True
    if account.lower() == own.lower():
        return True
    return bool(own_id) and account.lower() in _own_handles(own_id)


def foreign_only_blueprints(pattern=None, own=None):
    """Baupläne, die NUR in Protokollen anderer Accounts stehen.

    Grundlage für „aufräumen": Was auch der eigene Account je bekam, bleibt
    unangetastet. ⚠ Ein Rest Unsicherheit bleibt — ist die eigene Log mit
    dem Fund schon weggeräumt, sieht der Bauplan hier fremd aus. Deshalb schlägt
    diese Funktion nur vor; entfernt wird erst nach Rückfrage."""
    pattern = pattern or phrases.pattern()
    # ⚠ Vorher die Kennung lernen: Ohne sie gälten frühere Handles des
    # eigenen Accounts als fremd, und ihre Baupläne stünden zum Entfernen da.
    try:
        refresh_own_identity()
    except Exception as error:
        paths._report_error('logsource.foreign_only', error)
    own = own or own_account()
    if not own or own == ALL_ACCOUNTS:
        return []
    mine, foreign = set(), {}
    for filename in _all_log_files():
        account, account_id = identity_of_file(filename)
        try:
            finds = _read_file(filename, pattern)
        except Exception:
            continue
        for name, _extra in finds:
            key = name.lower().strip()
            if counts_for(own, account, account_id):
                mine.add(key)
            else:
                foreign.setdefault(key, name)
    return sorted((n for k, n in foreign.items() if k not in mine),
                  key=str.lower)


def foreign_summary(state=None, pattern=None):
    """Was aus Sitzungen anderer Accounts nicht gezählt wurde.

    `{'bauplaene': Anzahl, 'sitzungen': Anzahl, 'zuletzt': mtime}` — die
    Sicherungen aus dem Lesestand, dazu die laufende `Game.log`, wenn sie
    einem anderen Account gehört. Ohne Handle; zählen alle Accounts
    (`ALL_ACCOUNTS`), ist alles null."""
    out = {'bauplaene': 0, 'sitzungen': 0, 'zuletzt': 0.0}
    own = (paths.setting(ACCOUNT_SETTING) or '').strip()
    if not own or own == ALL_ACCOUNTS:
        return out
    state = state or ReadState()
    entries = state.data.get('sicherungen') or {}
    for filename in paths.log_backups():
        entry = entries.get(os.path.basename(filename)) or {}
        if not entry.get('fremd') or not state.knows(filename):
            continue
        if counts_for(own, *identity_of_file(filename)):
            continue
        out['sitzungen'] += 1
        out['bauplaene'] += int(entry.get('bauplaene') or 0)
        out['zuletzt'] = max(out['zuletzt'], float(entry.get('mtime') or 0))
    active = _safe_game_log()
    if active:
        handle, account_id = identity_of_file(active)
        if (handle or account_id) and not counts_for(own, handle, account_id):
            out['sitzungen'] += 1
            out['bauplaene'] += len(_read_file(active,
                                               pattern or phrases.pattern()))
            try:
                out['zuletzt'] = max(out['zuletzt'],
                                     os.path.getmtime(active))
            except OSError:
                pass
    return out


# ------------------------------------------------------------------ Lesestand
class ReadState:
    """Merkt sich, was schon gelesen wurde — über Programmneustarts hinweg."""

    def __init__(self):
        self.path = paths.app_file('logstand.json')
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

    def remember(self, filename, account=None, foreign=None):
        """Eine Sicherung als erledigt merken.

        `foreign` ist bei einer Sicherung eines anderen Accounts die Zahl
        ihrer Baupläne; sie werden nicht gezählt, nur gemerkt
        (`foreign_summary`). Der Handle steht dann nicht im Eintrag."""
        try:
            entry = {
                'groesse': os.path.getsize(filename),
                'mtime': os.path.getmtime(filename),
                'fremd': foreign is not None,
            }
            if foreign is not None:
                entry['bauplaene'] = int(foreign)
            elif account:
                entry['account'] = account
            self.data['sicherungen'][os.path.basename(filename)] = entry
            self.data['letzte_sitzung'] = max(
                self.data.get('letzte_sitzung', 0.0), os.path.getmtime(filename))
        except OSError:
            pass

    def needs_recheck(self, filename, own, previous=None):
        """Muss eine schon bekannte Sicherung noch einmal gelesen werden?

        Ja, wenn sie als fremd übersprungen wurde und jetzt zählt. Und bei
        einem Eintrag ohne `fremd`-Feld, dessen Handle nicht der ist, gegen
        den beim Merken verglichen wurde (`previous`, sonst `own`): Er wurde
        übersprungen, das Feld fehlt nur. So kommen nach einem Wechsel des
        Handles die verpassten Baupläne nach."""
        entry = self.data['sicherungen'].get(os.path.basename(filename)) or {}
        if entry.get('fremd'):
            handle, account_id = identity_of_file(filename)
            return counts_for(own, handle, account_id)
        compare = previous or own
        if 'fremd' in entry or not compare or compare == ALL_ACCOUNTS:
            return False
        account = entry.get('account')
        return bool(account) and str(account).lower() != compare.lower()

    def settle(self, filename):
        """Einen Eintrag ohne `fremd`-Feld als gelesen festhalten, sobald
        `needs_recheck` ihn durchgelassen hat. Sonst fiele er beim nächsten
        Start, mit dem nachgezogenen Handle, noch einmal durch."""
        entry = self.data['sicherungen'].get(os.path.basename(filename))
        if entry is not None and 'fremd' not in entry:
            entry['fremd'] = False

    def is_foreign(self, filename):
        """Wurde diese Sicherung als fremd übersprungen?"""
        entry = self.data['sicherungen'].get(os.path.basename(filename)) or {}
        return bool(entry.get('fremd'))

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
    pattern = pattern or phrases.pattern()
    all_names = paths.log_backups()
    # Vergleichswert VOR dem Lauf festhalten — `stand.merke()` schreibt ihn
    # gleich fort, danach ließe sich keine Lücke mehr erkennen.
    before = state.data.get('letzte_sitzung', 0.0)
    report = {'dateien': 0, 'uebersprungen': 0, 'gefunden': 0,
               'vorhanden': len(all_names), 'luecke': False, 'grund': '',
               'laufende': False, 'unlesbar': 0, 'fremd': 0}
    active = _safe_game_log() if incl_running else None
    files = all_names + ([active] if active else [])
    own = previous = own_account(files)
    # ⭐ Vor dem Lesen: Kennung lernen und den Handle nachziehen, sonst
    # gälten nach einem Wechsel des Handles die neuen Sitzungen als fremd.
    try:
        refresh_own_identity(files)
        own = own_account(files)
    except Exception:
        pass

    match, seen = [], set()
    for filename in all_names:
        if (only_new and state.knows(filename)
                and not state.needs_recheck(filename, own, previous)):
            state.settle(filename)
            report['uebersprungen'] += 1
            continue
        # ⭐ Protokolle eines anderen Accounts zählen nicht — siehe oben.
        # Gemerkt werden sie trotzdem, mit der Zahl ihrer Baupläne, sonst
        # würden sie bei jedem Start neu angefasst. Stellt der Spieler den
        # Account um, liest `read_all` ohnehin alles wieder.
        account, account_id = identity_of_file(filename)
        if not counts_for(own, account, account_id):
            report['fremd'] += 1
            try:
                foreign = len(_read_file(filename, pattern))
            except Exception:
                foreign = 0
            state.remember(filename, account, foreign=foreign)
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
        state.remember(filename, account)
        report['dateien'] += 1

    # Die laufende Game.log gehört mit dazu, wenn sie noch nie gelesen wurde:
    # Wer den Watcher startet, während Star Citizen schon läuft, hätte sonst
    # ausgerechnet die aktuelle Sitzung als Loch im Bestand. Danach steht der
    # Lesestand auf dem Dateiende, das Mitlesen setzt dort nahtlos an.
    if incl_running:
        # ⚠ Auch dieser Teil darf den Lauf nicht kippen — er steht NACH der
        # Schleife, also haette eine Ausnahme hier ausgerechnet die eben
        # gelesenen Sicherungen um ihren Eintrag gebracht.
        # ⚠ **Immer lesen, nicht nur beim allerersten Mal.** Mit einer
        # Bedingung wie `if aktiv and stand.aktiv_holen(aktiv) is None:` würde
        # die laufende Datei übersprungen, sobald sie einmal gelesen war. Das
        # trifft genau den Fall, den jeder für abgedeckt hält:
        #
        #   Watcher zu, Star Citizen läuft weiter, Baupläne kommen, Watcher
        #   später wieder auf.
        #
        # Dann steht der Lesestand irgendwo mitten in der Datei, das Mitlesen
        # setzt **dort** an, und alles davor ist für immer weg — es landet auch
        # nicht in `logbackups/`, denn dorthin wandert die Datei erst beim
        # nächsten Spielstart. Ein Bauplan vor dem Lesestand würde nie
        # gefunden.
        #
        # Die Datei ganz zu lesen kostet bei 12 MB den Bruchteil einer Sekunde —
        # die Nachlese geht ohnehin über 149 Sicherungen. Doppelte fängt der
        # Bestand ab, der prüft jeden Namen.
        if active and not counts_for(own, *identity_of_file(active)):
            report['fremd'] += 1
        elif active:
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
        if active:
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

    ⚠ `paths.game_log()` sieht auf dem Dateisystem nach. Eine ausgehaengte
    Platte oder ein Netzpfad, der gerade nicht antwortet, hat den ganzen
    Nachlese-Lauf gekippt, samt der bereits gelesenen Sicherungen."""
    try:
        return paths.game_log()
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
    """Eine ganze Logdatei blockweise durchsuchen.

    ⭐ Gemerkt je Datei, Stand und Suchmuster (rc9): Unveränderte Dateien
    ergeben dasselbe — gelesen wurde die laufende `Game.log` trotzdem bei
    jedem Besuch der Seite „Erkennung" ganz (siehe den Kommentar in
    `catch_up`, warum ganz). Wächst sie, ändert sich der Stand, und sie wird
    wieder ganz gelesen."""
    try:
        info = os.stat(filename)
        key = (filename, info.st_mtime_ns, info.st_size,
               getattr(pattern, 'pattern', pattern))
    except (OSError, TypeError):
        key = None
    if key is not None and key in _READ_CACHE:
        return list(_READ_CACHE[key])
    found = _read_file_uncached(filename, pattern)
    if key is not None:
        if len(_READ_CACHE) > 1000:
            _READ_CACHE.clear()
        _READ_CACHE[key] = tuple(found)
    return found


_READ_CACHE = {}


def _read_file_uncached(filename, pattern):
    """Die eigentliche Suche hinter `_read_file` — ohne Zwischenspeicher."""
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
    # wäre in der Sprache dieses Moments eingefroren — wer später umstellt, hätte
    # eine deutsche Zeile in einem englischen Fenster. Der `Satz` merkt sich
    # Schlüssel und Werte und lässt sich beim
    # Sprachwechsel neu auswerten.
    if not all_names:
        return {'luecke': True, 'grund': Phrase('m_keine_logs')}
    oldest = min((os.path.getmtime(p) for p in all_names
                     if os.path.exists(p)), default=0.0)
    if not before:
        return {'luecke': True,
                'grund': Phrase('m_erster_lauf', Moment(oldest))}
    if oldest > before + 60:
        return {'luecke': True,
                # ⚠ Auch das Datumsformat ist sprachabhängig: Im Englischen
                # steht das Jahr vorn (`m_erster_datum`). Deshalb wandert hier
                # der rohe Zeitstempel weiter (`Zeitpunkt`) statt eines fertig
                # formatierten Datums — sonst stünde in der englischen Meldung
                # ein deutsches Datum.
                'grund': Phrase('m_luecke_logs',
                              Moment(before), Moment(oldest))}
    return {'luecke': False, 'grund': ''}


# ------------------------------------------------------------------- Mitlesen
class LogTail:
    """Liest die laufende Game.log fortlaufend weiter.

    Neu gegenüber v1.5.0: Der Lesestand überlebt einen Programmneustart. Wer den
    Watcher neu startet, während das Spiel läuft, verliert die Baupläne dieser
    Sitzung nicht mehr."""

    def __init__(self, state=None, pattern=None):
        self.state = state or ReadState()
        self.pattern = pattern or phrases.pattern()
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
        # eine Auswertung, die erst alle Enden und dann alle Annahmen abarbeitet,
        # den Auftrag weg und stellt ihn gleich wieder hin. Er stuende als frisch
        # angenommen da, obwohl er laengst erledigt ist.
        # Eintraege sind `(ist_annahme, titel, mission_id, objective_id)`
        # — die beiden Kennungen entscheiden, ob ein Ende den Auftrag
        # meint oder nur ein Zwischenziel (siehe `contracts.SUFFIX`).
        self.mission_events = []
        # ⭐ `{mission_id: vertrag_id}` aus den `CreateMarker`-Zeilen desselben
        # Abschnitts. Ein leeres Woerterbuch ist der Normalfall, solange kein
        # Auftrag angenommen wurde.
        self.mission_contracts = {}
        # Und die Zwischenziele desselben Abschnitts — was gerade zu tun ist.
        # ⚠ Zwei Sorten in einer Liste, roh: Zustandswechsel und Wortlaut.
        # Gewertet wird in `contracts.Objectives`, damit Start und laufender Betrieb
        # nicht wieder eigene Rechenwege bekommen.
        self.objective_events = []
        # Der Text des letzten Abschnitts — für die Prüfung nach einem
        # Serverwechsel (`contracts.server_change_silence`).
        self.last_text = ''
        # ⭐ Wem die laufende Log gehört — None, solange sich im Spiel noch
        # niemand eingeloggt hat. Baupläne eines fremden Accounts werden nicht
        # angezeigt (siehe `account_from_text`).
        self.account = None
        # Die Kennung dazu (siehe `ACCOUNT_ID_RE`) — None, solange sie fehlt.
        self.account_id = None

    def _locate(self):
        p = paths.game_log()
        if p and p != self.path:
            self.path = p
            self.account, self.account_id = identity_of_file(p)
            remembered = self.state.get_active(p)
            try:
                size = os.path.getsize(p)
            except OSError:
                size = 0
            # ⚠ **Drei Fälle, und am mittleren hängt, ob Baupläne verloren gehen.**
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
            # Im zweiten Fall muss `0` stehen, nicht `groesse` (das **Ende** der
            # neuen Datei). Sonst übersprange der Watcher jeden Bauplan, den
            # die frische Sitzung schon geschrieben hat, und merkte es nie:
            # `new_names()` hat zwar dieselbe Regel
            # (`if size < self.offset: self.offset = 0`), kommt aber nicht dazu
            # — der Stand steht dann längst auf dem Dateiende und
            # `size == self.offset` steigt sofort aus.
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
        self.mission_contracts = {}
        self.objective_events = []
        self.last_text = ''
        if not self._locate():
            return []
        try:
            size = os.path.getsize(self.path)
            if size < self.offset:          # Log rotiert -> neue Spielsitzung
                self.offset = 0
                self.account = None         # neue Sitzung, neue Anmeldung
                self.account_id = None
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
        self.last_text = text if self.mission_pattern else ''
        # Derselbe Textabschnitt, zweiter Blick: angenommene Auftraege.
        self.missions = (self.mission_pattern.findall(text)
                          if self.mission_pattern else [])
        # Und ein dritter: was in diesem Abschnitt zu Ende gegangen ist.
        self.missions_done = (self.mission_end_pattern.findall(text)
                                  if self.mission_end_pattern else [])
        self.mission_events = self._sort_events(text)
        # ⭐ Und welcher VERTRAG hinter einer Mission steckt. Die Zeile steht im
        # selben Abschnitt (`CreateMarker`) und nennt die
        # `contractDefinitionId` — damit bekommt die Anzeige die Bauplanliste
        # genau dieser Region statt der ueber alle Regionen zusammengefassten.
        # Siehe `contracts.contracts_from_text`.
        self.mission_contracts = (contracts.contracts_from_text(text)
                                  if self.mission_pattern else {})
        # ⚠ Ohne Auftragsmuster gibt es auch keine Auftragsanzeige — dann
        # braucht niemand die Ziele, und das Suchen waere reine Arbeit.
        self.objective_events = (contracts.objective_events_from_text(text)
                                if self.mission_pattern else [])
        if self.account is None or self.account_id is None:
            handle, account_id = identity_from_text(text)
            self.account = self.account or handle
            self.account_id = self.account_id or account_id
        # ⭐ Die laufende Log ist die neueste: Nach einem Wechsel des Handles
        # zieht sie den gespeicherten nach, bevor `counts_for` unten prüft.
        if self.account_id:
            note_identity(self.account, self.account_id)
        # ⚠ Nur die BAUPLÄNE werden für einen fremden Account unterdrückt.
        # Aufträge bleiben: Wer gerade den Zweitaccount spielt, will dessen
        # laufende Aufträge im Overlay sehen — die gehören nicht in den
        # Bestand, sondern zur Sitzung.
        if not counts_for(own_account(), self.account, self.account_id):
            return []
        return _names_from_text(text, self.pattern)

    def _sort_events(self, text):
        """Annahmen und Enden dieses Abschnitts in der Reihenfolge des Logs.

        ⚠ Das Auslesen selbst liegt in `contracts.events_from_text` — eine
        Stelle für beide Wege. Der Start rechnet dort über die ganze
        `Game.log`, der laufende Betrieb hier über den neuen Abschnitt; liefen
        die beiden auseinander, verschwände ein Auftrag beim Neustart oder
        stünde doppelt da.
        """
        if not self.mission_pattern and not self.mission_end_pattern:
            return []
        return contracts.events_from_text(text, self.mission_pattern,
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
