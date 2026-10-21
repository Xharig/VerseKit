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
Ein Fehlerbericht, mit dem sich arbeiten lässt.

Dieses Modul baut einen Textblock, den der Spieler in ein Issue einfügt — und der die Fragen schon
beantwortet, die man sonst einzeln stellen müsste: Welches System, welche
Verpackung, welche Tk-Version, welcher Bildschirmaufbau, ist das Spiel
gefunden, welche Sprache wurde erkannt, wie weit ist das Protokoll gelesen,
was steht im Katalog — und was ist zuletzt schiefgegangen.

Drei Regeln, die nicht verhandelbar sind:

  1. **Kein Benutzername, keine Kennungen.** Jeder Wert läuft durch
     `paths.redact()`; aus `/home/spieler/…` wird `<heim>/…`. Ein Name steht
     nur oben als Melder, so wie im Feld der Seite (`reporter_name`). Ein
     Bericht kann in einem **öffentlichen** Issue landen.
  2. **Nichts wird verschickt.** Das Modul gibt Text zurück, mehr nicht. Ob er
     jemanden erreicht, entscheidet allein der Spieler.
  3. **Der Bericht darf nie scheitern.** Jede Angabe wird einzeln geholt; was
     nicht zu ermitteln ist, steht als `—` da. Ein Bericht, der abbricht, weil
     eine Kleinigkeit fehlt, ist genau dann nutzlos, wenn man ihn braucht.
"""
import json
import os
import platform
import re
import sys
from datetime import datetime

from . import errors, overlay, paths
from .language import t


def _safe(f, default='—'):
    """Eine Angabe holen und dabei nichts riskieren.

    ⚠ Ein leerer Wert ist normal (kein Spiel installiert, keine Merkliste) —
    eine **Ausnahme** ist es nicht. Die wurde hier bisher stillschweigend
    verschluckt, und im Bericht stand nur ein Strich. So blieb der Fehler bei
    der Spielsprache drei Übergaben lang unentdeckt: Er sah aus wie „nichts
    gefunden", war aber ein TypeError.
    """
    try:
        value = f()
        if value is None or value == '':
            return default
        return value
    except Exception as exception:
        try:
            from . import errors
            errors.record('report.angabe', exception)
        except Exception:
            pass              # das Melden darf den Bericht nie umwerfen
        return default


def _wrap(text, width):
    """Einen Satz auf mehrere Zeilen verteilen, ohne Wörter zu zerschneiden.

    ⚠ Kein `textwrap`: Der Bericht soll auch dann noch stehen, wenn jemand
    eine einzelne Zeile ohne Leerzeichen einträgt (ein Pfad zum Beispiel) —
    die bleibt hier ganz, statt hart getrennt zu werden. Lieber eine zu lange
    Zeile als ein zerschnittener Dateiname.

    ⚠⚠ **Eigene Zeilenumbrüche bleiben stehen.** Ein einzelnes `.split()`
    über den ganzen Text zerlegte ihn an jedem Leerraum — auch an `\\n`. Das
    Eingabefeld ist mehrzeilig, und Leute tippen Aufzählungen:

        1. Läden öffnen
        2. Auf einen Radar klicken
        3. nichts passiert

    Daraus würde eine einzige Zeile — und genau die Abfolge macht eine
    Meldung brauchbar. Umbrochen wird deshalb **je Zeile**; leere Zeilen
    bleiben als Absatztrenner.
    """
    result = []
    for paragraph in (text or '').split('\n'):
        if not paragraph.strip():
            # Eine Leerzeile trennt Absätze. Nicht mehrere hintereinander —
            # wer dreimal Enter drückt, soll den Bericht nicht auseinander
            # ziehen.
            if result and result[-1] != '':
                result.append('')
            continue
        running = ''
        for word in paragraph.split():
            if running and len(running) + 1 + len(word) > width:
                result.append(running)
                running = word
            else:
                running = (running + ' ' + word) if running else word
        if running:
            result.append(running)
    # Ein Absatztrenner ganz am Ende wäre nur eine Leerzeile im Bericht.
    while result and result[-1] == '':
        result.pop()
    return result or ['']


def _dense(entries):
    """Gleiche Zeilen hintereinander zu einer zusammenfassen.

    ⚠ Wozu: Der Bericht zeigt je Topf nur zwölf Zeilen. Eine Liste, die sich
    beim Tippen immer wieder neu zeichnet, schreibt in Sekunden zwölf gleiche
    Zeilen — und der Ausschnitt sagt danach nichts mehr aus (zwölfmal
    `Liste: zeichnen beginnt`).

    Zusammengefasst wird nur, was **direkt hintereinander** gleich ist, und die
    Uhrzeit der ersten Zeile bleibt stehen — sonst ginge die Reihenfolge oder
    der Zeitpunkt verloren.
    """
    out = []
    for entry in entries:
        text = entry.split('  ', 1)[-1].strip()
        if out and out[-1][1] == text:
            out[-1][2] += 1
            continue
        out.append([entry, text, 1])
    return [line if number == 1 else '%s  (%d×)' % (line, number)
            for line, _text, number in out]


def _log_line():
    """Wie viele Protokolle da sind, wie viele gelesen wurden — und was dabei
    herauskam.

    ⚠⚠ **Diese Zeile ersetzt eine Rueckfrage, die oft nicht moeglich ist.**
    Viele Protokolle und null Bauplaene koennen heissen, dass die Erkennung
    versagt — oder dass jemand neu im Spiel ist. Der Bericht unterscheidet
    das selbst:

    | Was dasteht | Was es heisst |
    |---|---|
    | 462 · 462 durchgesehen · 0 Bauplaene daraus | die Erkennung findet nichts |
    | 462 · 0 durchgesehen · 0 Bauplaene daraus | die Nachlese lief nie |
    | 462 · 462 durchgesehen · 380 Bauplaene daraus | alles in Ordnung |

    ⚠ Gezaehlt werden nur die Bauplaene aus `log` und `nachlese`. Was vom
    Launcher, von Hand oder aus den Startbauplaenen kam, sagt ueber die
    Log-Erkennung nichts aus — und genau die steht hier zur Frage.
    """
    from . import collection as collection_module
    from . import logsource, paths as paths_module

    # ⚠ **Jeder Schritt fuer sich abgesichert, auch der erste.** Diese Zeile
    # steht in einem Bericht, den jemand abschickt, WEIL schon etwas kaputt
    # ist — eine ausgehaengte Platte darf ihn nicht um den Rest bringen.
    # Selbsttest 94 prueft das.
    backups = []
    try:
        backups = paths_module.log_backups()
    except Exception:
        pass
    # ⚠ Einzahl beachten, sonst steht `1 Protokolle` im Bericht.
    parts = [t('b_protokolle_1' if len(backups) == 1 else 'b_protokolle')
             % len(backups)]
    try:
        state_value = logsource.ReadState()
        read_count = sum(1 for p in backups if state_value.knows(p))
        parts.append(t('b_logs_gelesen') % read_count)
    except Exception:
        pass
    try:
        sources = collection_module.by_source(collection_module.load())
        from_logs = sources.get('log', 0) + sources.get('nachlese', 0)
        parts.append(t('b_logs_funde_1' if from_logs == 1 else 'b_logs_funde')
                     % from_logs)
    except Exception:
        pass
    return ' · '.join(parts)


def _collection_line():
    """Wie viele Baupläne — und wie viele davon die Bauplan-Liste zeigt.

    ⚠⚠ **Warum zwei Zahlen.** Der Bericht zählt die Einträge in `bestand.json`,
    die Bauplan-Liste geht den **Katalog** durch und hakt ab, was man davon hat.
    Ein Bauplan, den der Katalog nicht kennt, steht also in der einen Zahl und
    fehlt in der anderen — beide Zahlen stimmen, und wer nur sie sieht, hält
    eine davon für kaputt.

    Deshalb steht die Differenz im Bericht, statt dass sie jemand suchen
    muss. Sie ist auch die interessantere Angabe: Sie sagt, wie weit Katalog und
    eigener Stand auseinanderlaufen.
    """
    from . import collection as collection_module
    from . import catalog as catalog_module
    data = collection_module.load()
    total = collection_module.count(data)
    try:
        known = set(catalog_module.load().get('bauplaene') or {})
    except Exception:
        known = set()
    if not known:
        return t('b_n_bauplaene') % total
    in_catalog = len(collection_module.keys(data) & known)
    if in_catalog == total:
        return t('b_n_bauplaene') % total
    return t('b_n_bp_katalog') % (total, in_catalog, total - in_catalog)


# Wie viele Namen der Bericht höchstens aufzählt. Mehr macht ihn unlesbar,
# und um die Ursache zu erkennen, reicht eine Handvoll Beispiele.
UNBEKANNT_MAX = 12


def _unknown_blueprints():
    """Die Baupläne im eigenen Bestand, die der Katalog nicht kennt.

    ⚠ Die Zahl allein (`23 unbekannt`) sagt nur, dass etwas nicht zusammenpasst.
    Die Namen sagen, **was** — und meistens auch gleich, warum: ein ganzes
    Rüstungsset, das der Katalog noch nicht führt, oder eine abweichende
    Schreibweise. Ohne sie muss jemand die Datei von Hand mit dem Katalog
    vergleichen; damit ist die Angabe im Bericht wertlos.
    """
    from . import collection as collection_module
    from . import catalog as catalog_module
    try:
        known = set(catalog_module.load().get('bauplaene') or {})
    except Exception:
        return ''
    if not known:
        return ''
    data = collection_module.load()
    missing = sorted((e.get('name') or k)
                     for k, e in data['bauplaene'].items() if k not in known)
    if not missing:
        return ''
    shown = missing[:UNBEKANNT_MAX]
    text = ' · '.join(shown)
    if len(missing) > UNBEKANNT_MAX:
        text += '  ' + t('b_und_weitere') % (len(missing) - UNBEKANNT_MAX)
    return text


def _game_language():
    """Wonach im Log gesucht wird — und woher **jede** Formulierung stammt.

    ⚠ Die Herkunft steht **je Formulierung**, nicht einmal für die ganze
    Liste. Die Liste ist gemischt: belegte Formulierungen (eigene Angabe,
    `global.ini`) und die eingebaute Rückfalltabelle. Eine gemeinsame Herkunft
    ließe den Bericht so lesen, als stünden alle in der `global.ini` — und
    schickte die Fehlersuche in die Irre."""
    from . import phrases
    found, _origin = phrases.collect()
    if not found:
        return None
    own_ones, from_ini = phrases.measured()
    used = own_ones + from_ini
    fallback = [p for p in found if p not in used]
    parts = []
    if own_ones:
        parts.append('%s (%s)' % (', '.join(own_ones), t('b_woher_eigen')))
    if from_ini:
        parts.append('%s (%s)' % (', '.join(from_ini), t('b_woher_ini')))
    if fallback:
        parts.append('%s (%s)' % (', '.join(fallback), t('b_woher_tabelle')))
    return ' · '.join(parts)


def _patch_history():
    """Was die Historie je Spielversion führt — mit Anzahl.

    ⚠ Der Katalogstand allein sagt nichts über die Historie darunter: Ein
    eigener Fund, der die mitgelieferte Liste derselben Version überschreibt,
    lässt aus vielen Bauplänen einer Version wenige werden. Zeigt der
    Patch-Filter fast nichts, stehen die Zahlen hier, ohne dass jemand erst
    eine JSON-Datei aufmacht.

    ⚠ Die Kurzform allein reicht nicht. `4.10.0-live.12519617` und
    `4.10.0-live.12545750` kürzen beide auf `4.10.0`; zweimal dieselbe
    Kurzform mit verschiedenen Zahlen ließe sich nicht zuordnen. Darum: Kurzform nur, solange sie eindeutig ist, sonst
    die volle Version."""
    from . import patchhistory
    listing = patchhistory.patches()
    if not listing:
        return None
    listing = listing[:5]
    short_forms = [short for _full, short, _count in listing]
    return ', '.join(
        '%s (%d)' % (short if short_forms.count(short) == 1 else full, anzahl)
        for full, short, anzahl in listing)


def _json_size(path_, key):
    """Wie viele Einträge stehen in einer unserer JSON-Dateien?

    ⚠ Kein `daten.get(schluessel, daten)` — fehlt der Schlüssel, würde sonst
    das **ganze** Wörterbuch gezählt: drei Felder oben (version, stand,
    bauplaene) statt der Baupläne darin. Eine falsche Zahl, die völlig
    plausibel aussieht — genau die Sorte, die niemand nachprüft.

    Fehlt der Schlüssel, steht `—` da. Lieber keine Angabe als eine
    erfundene, gerade in einem Bericht, mit dem jemand einen Fehler sucht.
    ⚠ Und: Eine Datei, die **es gar nicht gibt**, ist hier kein Fehler, sondern
    der Normalfall. Wer noch nichts auf die Merkliste gesetzt hat, hat keine
    `watchlist.json`. Ein `FileNotFoundError` würde von `_sicher` zwar
    aufgefangen, aber als Fehler in den Bericht geschrieben:

        report.angabe  FileNotFoundError: .../Bauplaene/watchlist.json

    Wer einen Fehler sucht, soll in dieser Liste keine Zeilen finden, die gar
    keine sind — ein leerer Wert ist normal, keine Ausnahme (siehe
    `_sicher`).
    """
    if not os.path.exists(path_):
        return '—'
    with open(path_, encoding='utf-8') as f:
        data = json.load(f)
    if key not in data:
        return '—'
    value = data[key]
    return len(value) if hasattr(value, '__len__') else '—'


def _system():
    name = platform.system()
    if name == 'Linux':
        ident = _safe(lambda: platform.freedesktop_os_release().get('PRETTY_NAME'), '')
        session = os.environ.get('XDG_SESSION_TYPE', '')
        return ' · '.join(x for x in ('Linux', ident, platform.release(), session) if x)
    if name == 'Windows':
        return 'Windows %s · Build %s' % (platform.release(), platform.version())
    return '%s %s' % (name, platform.release())


def _tk_version():
    """Die Tk-Fassung — so genau, wie sie zu bekommen ist.

    ⚠ `tkinter.TkVersion` ist eine Fliesskommazahl und meldet nur „9.0", auch
    bei 9.0.3. Zwischen Tk 8.6 und 9.0 liegt ein Hauptversionssprung, und
    Unterschiede im Aufbau der Oberflaeche sind genau dort zu erwarten (etwa
    ein traeger Fensteraufbau unter 9.0, der unter 8.6 zuegig laeuft). Ohne
    die genaue Nummer im Bericht laesst sich so etwas nicht zuordnen.
    """
    import tkinter
    try:
        root = tkinter._default_root
        if root is not None:
            return str(root.tk.call('info', 'patchlevel'))
    except Exception:
        pass
    return str(tkinter.TkVersion)


def _packaging_readable():
    """Die Kennung aus `updater` in einen lesbaren Namen übersetzen.

    ⚠ Nur hier, nur für die Anzeige: Die Kennung selbst wird anderswo
    verglichen (`art == 'quellcode'`) und bleibt deshalb, wie sie ist.
    """
    # ⚠⚠ **Ein dynamischer Import — kein Umbenennungswerkzeug findet ihn.**
    # Der Modulname steht in einer Zeichenkette, der Funktionsname auch.
    # Weder ein Syntaxbaum-Scanner noch der Selbsttest finden diese Stelle
    # bei einer Umbenennung: Sie bricht erst, wenn jemand einen Fehlerbericht
    # baut. Nur eine Textsuche über das ganze Repo trifft sie.
    kind = __import__('scbp.updater',
                     fromlist=['packaging']).packaging()
    return {'quellcode': t('b_v_quellcode'),
            'exe': t('b_v_exe'),
            'appimage': t('b_v_appimage')}.get(kind, kind)


def _screens(root):
    """Größe und Skalierung — hier lagen schon zwei Fehler begraben."""
    if root is None:
        return '—'
    width = root.winfo_screenwidth()
    height = root.winfo_screenheight()
    # 72 Punkte je Zoll ist Tks Bezug; daraus wird die Skalierung lesbar.
    scaling = round(float(root.tk.call('tk', 'scaling')) * 72 / 96 * 100)
    line = t('b_skalierung') % (width, height, scaling)
    # ⭐ **Fenstermaße dazu.** Neben dem Bildschirm stehen das Fenster und
    # sein `minsize`: Ist die Mindesthöhe größer als der Bildschirm, hält Tk
    # sie gegen jedes Verkleinern, und Teile des Fensters sind unerreichbar.
    # Diese drei Zahlen nebeneinander zeigen das in einer Zeile.
    try:
        fb, fh = root.winfo_width(), root.winfo_height()
        mb, mh = root.minsize()
        if fb > 50 and fh > 50:
            line += t('b_fenstermass') % (fb, fh, mb, mh)
            if mh > height:
                line += t('b_fenster_zu_hoch')
    except Exception:
        pass
    from . import dpi
    if dpi.active():
        line += t('b_scharf') % dpi.factor()
    elif dpi.wanted():
        line += t('b_scharf_ohne')
    return line


def _game_launcher():
    """Der Weg, auf dem Star Citizen gestartet würde — gekürzt und eingeordnet.

    Drei Auskünfte in einer Zeile: **ob** etwas gefunden wurde, **was**, und ob
    es der selbst eingetragene Startbefehl ist.
    """
    from . import paths as paths_module
    from . import language as language_module
    launcher = paths_module.game_starter()
    if not launcher:
        return language_module.t('b_starter_kein')
    short = paths_module.redact(str(launcher))
    own_flag = (paths_module.setting('spielstarter') or '').strip()
    if own_flag:
        return language_module.t('b_starter_eigen', short)
    return short


def _injection_state():
    """Stehen die Bauplan-Angaben im Spiel? Eine Zeile, die einen Anruf spart.

    ⚠ Der häufigste Support-Fall: Die Angaben fehlen im Spiel. Ursache ist
    fast immer, dass ein Übersetzungs-Update oder ein Spiel-Patch die
    `global.ini` neu geschrieben und die Angaben dabei stillschweigend
    entfernt hat — das Werkzeug merkt davon nichts.

    Die Einstellung (`inj_quelle`) allein sagt nicht, ob überhaupt etwas
    eingetragen ist; diese Zeile sagt es.

    Die Auskunft kommt aus `injection.status()` — derselben Stelle, die auch das
    Einstellungsfenster anzeigt. Kosten: rund 20 ms für eine 9-MB-Datei,
    gemessen; das fällt neben dem Rest nicht auf.
    """
    from . import injection
    lage = injection.status()
    # ⚠ Keine Datei ist NICHT dasselbe wie nicht eingetragen. Wer unter
    # Linux ohne Übersetzung spielt, hat schlicht keine `global.ini` — dort
    # wäre ein fettes NICHT eingetragen eine Warnung vor dem Normalzustand.
    if not lage['datei']:
        return t('b_inj_keine')
    parts = [t('b_inj_drin') if lage['drin'] else t('b_inj_weg')]
    # ⚠ Beide Schalter stehen auf „an", solange niemand sie anfasst — dann
    # tauchen sie in `selbst_gesetzt` NICHT auf. Ohne diese zwei Angaben liest
    # man nicht eingetragen und weiß nicht, ob das Absicht ist.
    if not paths.setting_bool('inj_an', True):
        parts.append(t('b_inj_aus'))
    parts.append(t('b_inj_auto')
                 if paths.setting_bool('inj_auto', True)
                 else t('b_inj_hand'))
    if lage['quelle']:
        # Für den Bericht beides: die Kennung (eindeutig) und den lesbaren Stand.
        from . import translation
        parts.append('%s %s %s' % (lage['quelle'],
                                   translation.installed(lage['quelle']) or '',
                                   lage['stand'] or ''))
    # ⭐⭐ **Welche Sprachdatei — und welche das Spiel wirklich lädt.**
    #
    # Es gibt `english/global.ini` und `german_(germany)/global.ini`, und der
    # Watcher pflegt genau eine davon. Steht die Angabe in der anderen, sieht
    # der Spieler **nichts** — ohne dass irgendetwas kaputt wäre. Welche
    # Datei gegen welche Sprache steht, muss deshalb im Bericht stehen.
    #
    # ⚠ Die Spielsprache steht als `g_language` in der `user.cfg`. Fehlt sie,
    # startet Star Citizen auf Englisch; dann steht hier „—" und das ist die
    # Auskunft, nicht ein fehlender Wert.
    try:
        folder = os.path.basename(os.path.dirname(lage['datei'] or '')) or '?'
        from . import translation
        played = translation.game_language() or '—'
        parts.append('%s / Spiel: %s' % (folder, played))
    except Exception as exception:
        errors.record('report.injektionssprache', exception)
    return ' · '.join(x for x in parts if x)


def _crash_brief(lines, limit):
    """Aus einem harten Abbruch die Zeilen, die etwas sagen.

    ⛔⛔ **Der schuldige Faden zuerst — sonst steht im Bericht Beiwerk.**
    Python schreibt bei einem harten Abbruch **alle** Fäden weg und markiert
    den, in dem es knallte, mit `Current thread`. Wo der in der Liste steht,
    ist Zufall.

    Die ersten 14 Zeilen zeigen oft nur Fäden, die **warten** —
    `GetMessageW` im Tastenkürzel-Faden, `socket.accept` im Overlay, die
    Tray-Schleife —, während der abgestürzte Faden weiter unten in der
    Kürzung verschwindet.

    > **Ein Bericht, der die Ursache abschneidet und die Zuschauer zeigt, ist
    > schlimmer als keiner — er schickt den Leser in die falsche Richtung.**

    Deshalb: Der Block ab `Current thread` kommt nach vorn, der Kopf (die
    Fehlermeldung selbst) bleibt darüber. Was dann noch Platz hat, folgt in
    ursprünglicher Reihenfolge.
    """
    # ⚠ Der Kopf ist alles VOR dem ersten Faden — nicht „die ersten drei
    # Zeilen". Sonst rutscht eine Zeile aus einem wartenden Faden mit nach
    # oben und sieht aus, als gehöre sie zur Fehlermeldung.
    first = next((i for i, z in enumerate(lines)
                   if z.lower().startswith(('thread ', 'current thread'))),
                  len(lines))
    head = lines[:first]
    place = next((i for i, z in enumerate(lines)
                   if z.lower().startswith('current thread')), None)
    if place is None:
        return lines[:limit]
    # Der schuldige Block reicht bis zum nächsten Faden.
    end = next((i for i in range(place + 1, len(lines))
                 if lines[i].lower().startswith(('thread ', 'current thread'))),
                len(lines))
    blame = lines[place:end]
    rest = [z for i, z in enumerate(lines)
            if i >= first and not (place <= i < end)]
    return (head + blame + rest)[:limit]


def _stamp(path):
    """Änderungszeit einer Datei im Datumsformat des Berichts, oder `—`."""
    try:
        return datetime.fromtimestamp(os.path.getmtime(path)).strftime(
            t('b_datum'))
    except OSError:
        return '—'


def _version_check_line():
    """Wann zuletzt bei GitHub nachgesehen wurde und was dabei herauskam.

    ⚠ Der gespeicherte Abgleich speist die Update-Seite. Liegt sein Zeitpunkt
    weit zurück, obwohl der Abruf in diesem Lauf geklappt hat, wurde er nicht
    gespeichert — dann zeigt die Seite eine veraltete Freigabeliste.
    """
    from . import updater
    checked = updater._cache_read().get('geprueft') or 0
    newest = updater.latest(paths.setting_bool('vorabversionen', False))
    when = (datetime.fromtimestamp(checked).strftime(t('b_datum'))
            if checked else t('b_va_nie'))
    state = {None: 'b_va_offen', True: 'b_va_ok', False: 'b_va_fehl'}.get(
        updater.fetch_succeeded(), 'b_va_offen')
    return t('b_va_wert') % (when, (newest or {}).get('version') or '—',
                             t(state))


# Dateien, die bei jedem Start oder Abgleich neu geschrieben werden.
_REWRITTEN_FILES = ('start-spur.txt', 'versionen.json', 'fehler.json',
                    'einstellungen.json', 'bestand.json')

_ATTR_READONLY = 0x1
_ATTR_HIDDEN = 0x2
_ATTR_SYSTEM = 0x4
_ATTR_REPARSE = 0x400
_ATTR_OFFLINE = 0x1000
_ATTR_RECALL = 0x400000


def _file_flags_line():
    """Schreibschutz, versteckt, System oder Cloud-Platzhalter an den eigenen Dateien.

    ⚠ Unter Windows scheitert das Überschreiben (`open(…, 'w')`) einer
    versteckten oder System-Datei mit `PermissionError`, Anhängen dagegen
    klappt. Der Ordner gilt dabei weiter als beschreibbar.
    """
    found = []
    for name in _REWRITTEN_FILES:
        path = paths.app_file(name)
        try:
            info = os.stat(path)
            attrs = info.st_file_attributes if os.name == 'nt' else 0
        except OSError:
            continue
        marks = []
        if attrs & _ATTR_READONLY or not os.access(path, os.W_OK):
            marks.append(t('b_ds_schreibschutz'))
        if attrs & _ATTR_HIDDEN:
            marks.append(t('b_ds_versteckt'))
        if attrs & _ATTR_SYSTEM:
            marks.append(t('b_ds_system'))
        if attrs & (_ATTR_REPARSE | _ATTR_OFFLINE | _ATTR_RECALL):
            marks.append(t('b_ds_cloud'))
        if marks:
            found.append('%s: %s' % (name, ', '.join(marks)))
    return ' · '.join(found) or t('b_ds_ok')


def _write_failures_line():
    """Gescheiterte Schreibversuche dieses Laufs, die sonst niemand meldet."""
    from . import updater
    found = [x for x in (errors.TRAIL_WRITE_ERROR[0],
                         updater.CACHE_WRITE_ERROR[0]) if x]
    return ' · '.join(found)


# Protokollzeilen des Setups, die etwas über den Ausgang sagen.
_SETUP_KEYWORDS = ('error', 'exception', 'fail', 'succeeded', 'rolling back',
                   'exit code', 'abort', 'code 32', 'denied', 'cannot',
                   'log closed')


def _tail(path, count, keywords=None):
    """Die letzten `count` nichtleeren Zeilen einer Datei, wahlweise gefiltert."""
    try:
        with open(path, encoding='utf-8', errors='replace') as f:
            rows = [z.rstrip() for z in f if z.strip()]
    except OSError:
        return []
    if keywords:
        rows = [z for z in rows if any(k in z.lower() for k in keywords)]
    return rows[-count:]


_TEMP_SETUP_PREFIX = 'scbp-update-setup'

_TEMP_SETUP_LOGS = [None, []]


def forget_temp_setup_logs():
    """Die gemerkte Liste der Setup-Protokolle unter `%TEMP%` verwerfen."""
    _TEMP_SETUP_LOGS[:] = [None, []]


def _temp_setup_logs(folder):
    """Die Setup-Protokolle `scbp-update-setup*.txt` direkt in `folder`.

    Gesucht wird einmal je Ordner, bis `forget_temp_setup_logs()` die Liste
    verwirft; nur `updater` legt dort solche Dateien an."""
    if _TEMP_SETUP_LOGS[0] == folder:
        return list(_TEMP_SETUP_LOGS[1])
    hits = []
    try:
        with os.scandir(folder) as entries:
            for entry in entries:
                name = entry.name.lower()
                if name.startswith(_TEMP_SETUP_PREFIX) and name.endswith('.txt'):
                    hits.append(entry.path)
    except OSError:
        return []
    _TEMP_SETUP_LOGS[:] = [folder, hits]
    return list(hits)


def _newest_setup_log():
    """Das jüngste Setup-Protokoll — im Datenordner oder, wenn es dort nicht
    angelegt werden konnte, unter `%TEMP%` (siehe `updater._free_setup_log`)."""
    import tempfile
    found = [paths.app_file('update-setup.txt')] + _temp_setup_logs(
        tempfile.gettempdir())
    found = [p for p in found if os.path.isfile(p)]
    if not found:
        return ''
    return max(found, key=os.path.getmtime)


def _update_attempt_lines():
    """Was der letzte Update-Versuch hinterlassen hat — Helfer, Setup, Laufmarke.

    ⚠ Startet ein Update und geht das Programm danach gleich wieder zu, steht
    der Hergang nur in diesen Dateien. Die Laufmarke bleibt liegen, solange
    kein Start sie ausgewertet hat.
    """
    from . import update_run
    out = []
    run = update_run.read_run()
    if run:
        try:
            began = datetime.fromtimestamp(float(run.get('start') or 0)) \
                .strftime(t('b_datum'))
        except (TypeError, ValueError, OSError):
            began = '—'
        out.append(t('b_up_marke') % (run.get('alt') or '?',
                                      run.get('ziel') or '?', began))
    code = update_run.read_result()
    if code is not None:
        out.append(t('b_up_ergebnis') % code)
    for name in (update_run.LOG_FILE_OLD, update_run.LOG_FILE):
        path = paths.app_file(name)
        rows = _tail(path, 8)
        if rows:
            out.append(t('b_up_helfer') % (name, _stamp(path)))
            out.extend('  ' + z for z in rows)
    setup = _newest_setup_log()
    rows = _tail(setup, 8, _SETUP_KEYWORDS)
    if rows:
        out.append(t('b_up_setup') % _stamp(setup))
        out.extend('  ' + z for z in rows)
    return out


TRACE_SHOWN = 4      # so viele Aufrufstellen je Fehler der laufenden Fassung
_TRACE_CODE = 90     # so viele Zeichen der Codezeile
_FRAME_RE = re.compile(r'^\s*File "(.+)", line (\d+), in (.+)$')


def _trace_frames(trace):
    """Die Aufrufstellen einer gespeicherten Rückverfolgung, je eine Zeile.

    Aus `File ".../scbp/pages.py", line 12, in zeigen` und der Codezeile
    darunter wird `scbp/pages.py:12 in zeigen: code`. Vom Pfad bleiben die
    letzten zwei Teile. Stellen in `tkinter` selbst fallen weg, solange eine
    andere übrig bleibt — sie stehen bei jedem Oberflächenfehler gleich da.
    Die Fehlermeldung am Ende fehlt, sie steht schon in der Zeile darüber.
    Jede Zeile läuft noch einmal durch `paths.redact()`."""
    frames = []
    for row in str(trace or '').splitlines():
        found = _FRAME_RE.match(row)
        if found:
            parts = [p for p in re.split(r'[\\/]', found.group(1)) if p]
            frames.append(['/'.join(parts[-2:]), found.group(2),
                           found.group(3).strip(), ''])
        elif (frames and row.startswith('    ') and not frames[-1][3]
              and row.strip(' ^~')):
            frames[-1][3] = row.strip()
    own = [f for f in frames if f[0].split('/')[0].lower() != 'tkinter']
    result = []
    for file_name, number, function, code in (own or frames):
        text = '%s:%s in %s' % (file_name, number, function)
        if code:
            text += ': ' + (code if len(code) <= _TRACE_CODE
                            else code[:_TRACE_CODE - 1] + '…')
        result.append(paths.redact(text))
    return result


REPORTER_SETTING = 'melder_name'
# Ja, sobald jemand das vorausgefüllte Feld geleert hat. Ein leerer
# `melder_name` ohne diese Marke gilt als nie eingetragen.
REPORTER_CLEARED = 'melder_leer_gewollt'


def own_reporter():
    """Der selbst eingetragene Melder-Name — oder None, wenn keiner gilt.

    Ein leerer Eintrag gilt nur mit `REPORTER_CLEARED`; sonst greift der
    Rückfall auf den Spiel-Account (`reporter_name`)."""
    raw = paths.settings().get(REPORTER_SETTING)
    if not isinstance(raw, str):
        return None
    name = raw.strip()
    if name or paths.setting_bool(REPORTER_CLEARED, False):
        return name
    return None


def reporter_name():
    """Wer den Bericht schickt.

    Selbst eingetragen → dieser Name, auch ein bewusst geleerter (dann
    keiner). Sonst der gewählte Spiel-Account der Seite Erkennung. Das Feld
    auf der Seite Fehler melden zeigt genau diesen Wert, er geht also nie
    ungesehen hinaus."""
    own = own_reporter()
    if own is not None:
        return own
    from . import logsource
    account = (paths.setting(logsource.ACCOUNT_SETTING) or '').strip()
    return '' if account == logsource.ALL_ACCOUNTS else account


# Einstellungen, die Kennungen aus dem Basetool (Einheiten), den
# Spiel-Account oder den Melder-Namen tragen: im Bericht steht statt des
# Werts nur •••, denn der Bericht kann öffentlich landen. Der Name steht,
# wenn gewollt, einmal oben als Melder (`reporter_name`).
# `eigener_account` ist `logsource.ACCOUNT_SETTING` (Prüfung 469 hält das fest).
MASKED_SETTINGS = frozenset({'basetool_einheit_neu', 'staffellager_einheit',
                             'eigener_account', REPORTER_SETTING})
# Einstellungen, die in der Einstellungszeile gar nicht erscheinen.
HIDDEN_SETTINGS = frozenset({REPORTER_CLEARED})


# Breite der Beschriftungsspalte. Längere Beschriftungen schieben den Wert
# nach rechts, behalten aber immer ein Leerzeichen davor.
LABEL_WIDTH = 18


def _row(label, value):
    """Eine Zeile `Beschriftung  Wert` in Spaltenform."""
    return '%s %s' % (label.ljust(LABEL_WIDTH - 1), value)


def build(version='', root=None, error_count=8, message='', reporter=None):
    """Den Bericht als Text zusammensetzen.

    `reporter` ist der Name, der oben als Melder steht — die Seite Fehler
    melden gibt den Wert aus ihrem Feld mit; ohne Angabe gilt
    `reporter_name()`."""
    lines = []

    def line(label, value):
        lines.append(_row(label, paths.redact(value)))

    lines.append(t('b_kopf')
                  % (version or '—', datetime.now().strftime(t('b_datum'))))
    lines.append('')

    # ⭐ Wer meldet das? Steht bewusst ganz oben — mit vielen Nutzern ist ein
    # Bericht ohne Absender kaum zuzuordnen, und Rückfragen laufen ins Leere.
    # **Freiwillig**: Ohne eigenen Eintrag steht hier der gewählte Spiel-Account,
    # sichtbar vorausgefüllt auf der Seite Fehler melden; bewusst geleert
    # steht hier `nicht angegeben` (`reporter_name`).
    if reporter is None:
        reporter = reporter_name()
    # ⚠ **Ohne `paths.redact()`.** Jede andere Zeile läuft durch die Anonymisierung,
    # die Benutzernamen durch `<benutzer>` ersetzt — und das träfe den
    # Absender-Namen, wenn er dem Systemkonto gleicht. Ausgerechnet die
    # einzige Angabe, die der Nutzer BEWUSST macht, verschwände dadurch.
    lines.append(_row(t('b_melder'), reporter or t('s_melder_leer')))
    # ⭐⭐ **Was ist passiert — in eigenen Worten, ganz oben.** Ohne dieses Feld
    # landet die Beschreibung im Namensfeld.
    #
    # ⚠ Steht **vor** allen technischen Angaben. Was der Mensch schreibt, ist
    # der Anfang jeder Diagnose — die Zahlen darunter belegen oder widerlegen
    # es. Umgekehrt sucht man erst in 60 Zeilen Technik nach dem Grund, warum
    # jemand überhaupt geschrieben hat.
    #
    # ⚠ Mehrzeilig eingerückt, damit ein langer Satz die Spaltenform nicht
    # sprengt — und durch `paths.redact()`, denn hier kann ein Pfad drinstehen.
    note = (message or '').strip()
    if note:
        lines.append('')
        lines.append(t('b_meldung'))
        for piece in _wrap(paths.redact(note), 76):
            lines.append('  ' + piece)
    lines.append('')

    summary = _safe(paths.overview, {})
    if not isinstance(summary, dict):
        summary = {}

    line(t('b_system'), _safe(_system))
    line(t('b_verpackung'), _safe(_packaging_readable))
    line(t('b_python'), '%s / %s' % (platform.python_version(),
                                      _safe(_tk_version)))
    line(t('b_bildschirm'), _safe(lambda: _screens(root)))
    # ⭐ **Wie das Overlay gerade steht:** Fenstergroesse, Klappzustand,
    # Mindestbreite, Leistengroesse, Versatz — eine Zeile beantwortet, wofuer
    # sonst nachgefragt oder nachgemessen werden muesste.
    #
    # ⚠ Nur Zahlen und Zustaende; der Bericht landet in einem oeffentlichen
    # Issue. Steht kein Overlay (Pruefstand, reines Fensterprogramm), bleibt
    # die Zeile weg statt „unbekannt" zu melden.
    _state = _safe(lambda: (overlay.STATE_REPORT[0] or (lambda: ''))())
    if _state:
        line(t('b_overlay'), _state)
    lines.append('')

    line(t('b_spiel'), _safe(lambda: summary.get('spiel_ordner')
                                 or t('b_nicht_gefunden')))
    line(t('b_gamelog'), _safe(lambda: summary.get('game_log')
                                   or t('b_nicht_gefunden')))
    line(t('b_sicherungen'), _safe(_log_line))
    # ⚠ **Womit sich das Spiel starten ließe — und ob das jemand von Hand
    # eingetragen hat.** Tut der Startknopf nichts, lässt sich das ohne diese
    # Zeile nicht klären, ohne den Nutzer auszufragen.
    line(t('b_starter'), _safe(_game_launcher))
    # ⚠ `collect()` gibt ein **Tupel** zurück — (phrases, herkunft). Ein
    # `', '.join(...)` direkt darauf bricht mit TypeError ab; `_safe()`
    # verschluckt den, und im Bericht stünde nur ein Strich.
    #
    # Die Herkunft wird gleich mit ausgegeben: Sie sagt, ob die Formulierung aus
    # der echten `global.ini` des Spielers stammt oder nur aus unserer Tabelle
    # geraten ist — die erste Auskunft, wenn Baupläne nicht erkannt werden.
    line(t('b_spielsprache'), _safe(_game_language))
    line(t('b_inj'), _safe(_injection_state))
    line(t('b_inj_datei'), _safe(
        lambda: __import__('scbp.injection', fromlist=['ini_file'])
        .ini_file()[0] or t('b_inj_keine')))
    lines.append('')

    line(t('b_bestand'), _safe(_collection_line))
    _unknown = _safe(_unknown_blueprints, '')
    if _unknown and _unknown != '—':
        line(t('b_unbekannt'), _unknown)
    line(t('b_merkliste'), _safe(lambda: t('b_n_eintraege') % _json_size(
        __import__('scbp.watchlist', fromlist=['path']).path(), 'eintraege')))
    # ⚠⚠ **Der gespeicherte Stand, kein Netzabruf.** `catalog.current_version()`
    # fragt scmdb.net. Ohne Internet wartete der Bericht auf den Timeout, und
    # weil er im Hauptfaden gebaut wird, wäre das ganze Fenster so lange
    # starr — ausgerechnet die Seite, die man bei Störungen braucht.
    #
    # Der Bericht soll ohnehin den **Ist-Zustand auf diesem Rechner** zeigen,
    # nicht den im Netz: Interessant ist, welchen Katalog der Nutzer hat.
    line(t('b_katalog'), _safe(lambda: (__import__(
        'scbp.catalog', fromlist=['load']).load().get('version') or None)))
    line(t('b_historie'), _safe(_patch_history))

    # ⚠ Mein Lager, Rezepte und Bergbaudaten. Ohne sie liesse sich eine leere
    # Herstellungs-Seite nicht beurteilen — man saehe nicht, ob die Daten
    # ueberhaupt geladen sind.
    def _storage_line():
        # Alle Posten von Mein Lager: Rohstoffe aus dem Werkstatt-Lager,
        # Waren und Items aus dem Handelslager — gezählt wie in der Übersicht.
        from . import materials, overview, trade_cargo
        posts = materials.load()
        goods = trade_cargo.load()
        return t('b_n_lager') % (
            (len(posts) + len(goods),)
            + overview.storage_counts(posts, goods))

    def _recipe_line():
        from . import crafting
        state_value = crafting.current_build()
        if not state_value:
            return t('b_nicht_geladen')
        return t('b_n_bauplaene_kurz') % (len(crafting.all_items()), state_value)

    def _mining_line():
        from . import mining
        state_value = mining.current_build()
        if not state_value:
            return t('b_nicht_geladen')
        return t('b_n_orte') % (len(mining.locations()), state_value)

    line(t('b_lager'), _safe(_storage_line))
    line(t('b_rezepte'), _safe(_recipe_line))
    line(t('b_bergbaudaten'), _safe(_mining_line))

    def _scanner_line():
        # ⭐ Ohne diese Zeile sagt ein Bericht zum Signatur-Scanner nichts:
        # an/aus, wie viel angelernt ist, was zuletzt gelesen wurde. (Einen
        # festen Scan-Bereich gibt es nicht — die Pille wird gesucht.)
        #
        # ⚠⚠ **Der Schalter ist nicht der Betrieb.** Die Einstellung allein
        # sagt nicht, ob der Wach-Faden überhaupt lebt; zeigt der Scanner
        # nichts mehr an, ist genau das die entscheidende Auskunft.
        from . import paths as paths_module, screen_grab, signature_scan, signature_watch
        if not screen_grab.supported():
            return t('b_scan_nicht')
        own = signature_scan._read_templates(
            paths_module.app_file(signature_scan.OWN_TEMPLATE_FILE))
        if not paths_module.setting_bool(signature_watch.SETTING, False):
            state = t('e_aus')
        else:
            state = t('b_scan_laeuft' if signature_watch.running()
                      else 'b_scan_tot')
        return t('b_scan') % (
            state,
            sum(len(v) for v in own.values()), len(own),
            len(signature_scan.samples()),
            signature_watch.shown() or '—',
            '%.2f' % screen_grab.dpi_scale())

    line(t('b_scanner'), _safe(_scanner_line))

    def _anti_afk_line():
        # Der letzte Abbruch steht hier auch dann, wenn der Schalter danach
        # wieder an ist — `last_error()` gilt nur bis zum Einschalten.
        from . import anti_afk
        if not anti_afk.supported():
            return t('b_aafk_nicht')
        failure = anti_afk.last_failure()
        return t('b_aafk_wert') % (
            t('e_an') if anti_afk.enabled() else t('e_aus'),
            anti_afk.sent_count(),
            t('e_an') if anti_afk.joy_running() else t('e_aus'),
            '%s (%s)' % failure if failure else '—')

    line(t('b_aafk'), _safe(_anti_afk_line))
    lines.append('')

    def _folder_line():
        """Der Datenordner — und ob sich dort überhaupt schreiben lässt.

        ⚠⚠ Lässt sich im Datenordner nichts speichern, zeigt der Bericht
        sonst Standard-Einstellungen und keine Fehler — auch das
        Fehlerprotokoll wird dann nicht geschrieben. Ohne diese Angabe liest
        sich so ein Bericht, als sei alles in Ordnung."""
        from . import paths as paths_module
        folder = summary.get('app_ordner')
        ok, _own, reason = paths_module.storage_status(paths_module.app_folder())
        if ok:
            text = '%s · %s' % (folder, t('b_ordner_ok'))
        else:
            text = '%s · %s' % (folder, t('b_ordner_gesperrt') % reason)
        if paths_module.first_pointer_blocked():
            text += ' · ' + t('b_zeiger_gesperrt')
        return text

    line(t('b_ordner'), _safe(_folder_line))
    def _settings_line():
        # Eine gesperrte Datei liest sich leer — dann nicht „Standard"
        # melden, sondern dass sie nicht lesbar war.
        if not paths._settings_read()[1]:
            return t('b_einst_gesperrt')
        return ', '.join(
            '%s=%s' % (k, ('•••' if v else '') if k in MASKED_SETTINGS else v)
            for k, v in sorted(
                (summary.get('selbst_gesetzt') or {}).items())
            if k not in HIDDEN_SETTINGS) or t('b_standard')

    line(t('b_einstellungen'), _safe(_settings_line))
    line(t('b_dateischutz'), _safe(_file_flags_line))
    _failures = _safe(_write_failures_line, '')
    if _failures:
        line(t('b_schreibfehler'), _failures)
    line(t('b_versionsabgleich'), _safe(_version_check_line))

    def _basetool_line():
        """Verbindung zum KRT Profit Basetool — ohne Token, ohne Kennungen.

        ⚠ Zeigt die Seite eine Ablehnung oder fehlende Erlaubnis, entscheidet
        sich der Fall daran, welche Rechte das Basetool bei der Anmeldung
        wirklich erteilt hat — die stehen hier."""
        from . import basetool, basetool_sync
        status = dict(basetool_sync.STATUS)
        rights = (basetool.CONNECTION.granted
                  or status.get('capabilities') or ())
        return t('b_basetool_wert') % (
            t('e_an') if basetool.CONNECTION.connected() else t('e_aus'),
            ', '.join(sorted(r.replace('exchange.', '') for r in rights))
            or '—',
            status.get('last_sync') or '—',
            status.get('code') or '—')

    # Nur wer das Basetool überhaupt nutzt, bekommt die Zeile.
    from . import basetool_sync as _basetool_sync
    if _basetool_sync.enabled():
        line(t('b_basetool'), _safe(_basetool_line))

    # ⚠ Die Startspur zuerst — bei einem Absturz ist sie das Einzige, was bleibt.
    # Ein `SIGSEGV` beendet den Prozess sofort: kein `except`, kein Fehlerbericht,
    # nur der Absturz. Die letzte Zeile hier sagt, wie weit der Start kam.
    # ⚠ Start und Bedienung **getrennt** deckeln. In einem gemeinsamen Topf mit
    # den letzten zwölf Zeilen verdrängen fünf Klicks den kompletten
    # Startverlauf — ausgerechnet den Teil, für den die Spur gebaut ist.
    start, page_lines = _safe(errors.split_trail, ([], []))
    # ⚠ Erst zusammenfassen, dann die letzten zwölf nehmen — andersherum wäre
    # der Ausschnitt schon leergeräumt, bevor das Zusammenfassen greift.
    start = _dense(start)
    if start:
        lines.append('')
        lines.append(t('b_spur'))
        for entry in start[-12:]:
            lines.append('  ' + entry)
    # Die Diagnose-Seite selbst gehört nicht in die Liste: Der Bericht entsteht,
    # **während** sie gebaut wird, und stünde sonst in jedem Bericht als letzte,
    # unfertige Zeile — es sähe jedes Mal so aus, als wäre genau dort Schluss.
    while page_lines and 'Seite diagnose' in page_lines[-1]:
        page_lines.pop()
    page_lines = _dense(page_lines)
    if page_lines:
        lines.append('')
        lines.append(t('b_spur_seiten'))
        # ⚠⚠ **24 Zeilen, nicht 12 — der Weg zum Bericht frisst die Spur.**
        # Jede Seite belegt zwei Zeilen; zwölf zeigten also sechs Seiten. Wer
        # den Bericht holt, klickt sich aber erst durch die Info-Seiten dorthin
        # — und schöbe damit genau die Seite hinaus, um die es ging.
        #
        # Ein Bericht, dessen Beschaffung die Beobachtung zerstört, ist kein
        # Bericht. `TRAIL_KEEP` hebt 60 Zeilen auf, der Platz war also da.
        for entry in page_lines[-24:]:
            lines.append('  ' + entry)

    attempt = _safe(_update_attempt_lines, [])
    if attempt:
        lines.append('')
        lines.append(t('b_up_block'))
        for entry in attempt:
            lines.append('  ' + paths.redact(entry))

    # ⚠ Und danach der harte Abbruch, falls es einen gab. Er steht **vor** den
    # Fehlern, weil er der schwerere Befund ist: Ein Eintrag in der Fehlerliste
    # heißt, das Programm hat weitergelebt; hier war es mitten im Befehl weg.
    # Nur die erste Handvoll Zeilen — der volle Aufrufweg aller Fäden füllt
    # Seiten, und der Melder soll den Bericht noch verschicken können.
    crash = _safe(errors.last_crash, [])
    if crash:
        lines.append('')
        # ⚠ Mit Datum: Die Datei überlebt beliebig viele saubere Läufe. Der
        # Vermerk stellt fest, wann — er urteilt nicht, ob es noch zutrifft.
        crash_at = _safe(errors.crash_time, None)
        # ⚠ Und mit Fassung, genau wie die Fehlerliste darunter — derselbe
        # Vermerk, dieselbe Regel: feststellen, nicht urteilen. Ohne sie
        # stünde ein Abbruch aus einer alten Version in einem neuen Bericht,
        # und nichts im Kopf verriete das.
        crash_version = _safe(errors.crash_version, '')
        if not crash_version:
            version_note = t('b_absturz_ohne_fassung')
        else:
            version_note = t('b_absturz_fassung') % crash_version
            if version and crash_version != version:
                version_note += '  ' + t('b_fehler_alt')
        lines.append(t('b_absturz') % (
            datetime.fromtimestamp(crash_at).strftime(t('b_datum')) if crash_at else '—')
            + ' · ' + version_note)
        for entry in _crash_brief(crash, 14):
            lines.append('  ' + entry)
        if len(crash) > 14:
            lines.append('  … (%d)' % (len(crash) - 14))

    latest = _safe(lambda: errors.latest(error_count), [])
    total = _safe(errors.count, 0)
    lines.append('')
    if latest:
        lines.append(t('b_fehler') % (len(latest), total))
        # ⚠ **Gleichartige Fehler zusammenfassen.** Ein einziger Vorfall kann
        # den ganzen Speicher belegen: Ein Fehler in einer Schleife (etwa ein
        # Fortschritt im Sekundentakt bei zugehendem Fenster) füllt alle 50
        # Plätze mit derselben Zeile — und die angezeigten Zeilen sagen alle
        # dasselbe, ohne Platz für das, was sonst noch passiert ist.
        #
        # Deshalb wird hier
        # gebündelt, was sich nur in der Uhrzeit unterscheidet — dieselbe Stelle,
        # dieselbe Art, dieselbe Meldung, dieselbe Fassung.
        # Seit `errors._merge` trägt ein Eintrag seine Wiederholungen selbst
        # (`anzahl`, `zuletzt`); ältere Dateien haben noch Einzelzeilen.
        bundled = []
        for e in latest:
            ident = (e.get('fassung'), e.get('stelle'), e.get('art'),
                       e.get('meldung'))
            times = int(e.get('anzahl') or 1)
            last = e.get('zuletzt') or e.get('zeit', '—')
            if bundled and bundled[-1][0] == ident:
                bundled[-1][2] += times
                bundled[-1][3] = last
            else:
                bundled.append([ident, e, times, last])

        traced = set()
        for _ident, e, how_often, until in bundled:
            # ⚠ Die Version dazuschreiben und kennzeichnen, was **nicht** aus
            # der laufenden Fassung stammt. Ohne die Angabe sucht der Nächste
            # nach einem Fehler, den es vielleicht nicht mehr gibt.
            #
            # ⚠⚠ **Die Kennzeichnung stellt fest, sie urteilt nicht.** Ein
            # Urteil wie `vermutlich behoben` stimmt nur, wenn der Absender
            # seine Fassung selbst überholt hat. Wer lange kein
            # Update gemacht hat, schickt aus einer alten Version einen Fehler,
            # den wir noch nie gesehen haben. Eine Bemerkung, die zum Abhaken
            # einlädt, ist dann genau das Gegenteil einer Hilfe.
            version_text = e.get('fassung') or '?'
            old_mark = ''
            if version and version_text not in ('?', '') and version_text != version:
                old_mark = '  ' + t('b_fehler_alt')
            lines.append('  %s  %-10s %-24s %s: %s%s'
                          % (e.get('zeit', '—'), version_text, e.get('stelle', '—'),
                             e.get('art', '—'), e.get('meldung', '—'), old_mark))
            if how_often > 1:
                lines.append(t('b_fehler_mehrfach') % (how_often, until))
            # Die Aufrufstellen zum jüngsten Eintrag je Stelle: aus der
            # laufenden Fassung `TRACE_SHOWN`, aus einer anderen nur die
            # letzte, damit alte Einträge den Bericht nicht strecken.
            if e.get('stelle') in traced:
                continue
            traced.add(e.get('stelle'))
            current = not version or version_text == version
            frames = _safe(lambda: _trace_frames(e.get('spur')), [])
            for frame in frames[-(TRACE_SHOWN if current else 1):]:
                lines.append(t('b_fehler_spur') % frame)
    else:
        lines.append(t('b_fehler_keine'))

    lines.append('')
    lines.append(t('b_fuss'))
    return '\n'.join(lines)


def submit(text, version='', attachments=None):
    """Den Bericht an den eingebauten Kanal schicken. (Erfolg, Meldung).

    ⚠ **Der einzige Weg, der bei Nicht-Bastlern ankommt.** Kopieren und in
    Discord einfügen scheitert dreifach: Der Bericht steckt unter
    Fortgeschritten, er ist zu lang für eine Nachricht, und man muss wissen,
    wohin damit.

    Verschickt wird **nur auf Knopfdruck** und erst, nachdem der Nutzer den
    vollen Wortlaut gesehen hat. Der Text ist derselbe, der auf der Seite steht
    — durch `paths.redact()` von Namen und Pfaden befreit.

    Als **Datei**, nicht als Nachricht: Discord nimmt höchstens 2000 Zeichen je
    Nachricht, ein Bericht ist regelmäßig länger. Eine angehängte `.txt` ist
    zudem das, was man lesen und aufheben kann.

    `attachments`: weitere Dateien als (Name, Bytes, MIME-Typ) — etwa die
    angelernten Scan-Bilder oder vom Nutzer gewählte Bilder (`image_files`).
    ⚠ Nur mit ausdrücklicher Zustimmung übergeben.
    """
    from . import report_target
    target = report_target.target()
    if not report_target.available():
        return False, t('m_bericht_kein_ziel')

    import urllib.request
    import uuid
    limit = uuid.uuid4().hex
    name = 'bericht-%s.txt' % datetime.now().strftime('%Y-%m-%d-%H%M')
    head = ('**Fehlerbericht** · %s' % (version or '?'))[:1900]

    torso = multipart(limit, head, name, text, attachments)
    # Die Weiterleitung lehnt größere Anfragen ab — geprüft wird vor dem
    # Hochladen.
    if len(torso) > SEND_LIMIT:
        return False, t('m_bericht_zugross')

    try:
        request = urllib.request.Request(
            target, data=torso, method='POST',
            headers={'Content-Type': 'multipart/form-data; boundary=%s' % limit,
                     'User-Agent': 'VerseKit (ehemals SC-BP-Watcher)'})
        with urllib.request.urlopen(request, timeout=30) as answer:
            if 200 <= answer.status < 300:
                return True, ''
            return False, 'HTTP %s' % answer.status
    except Exception as exception:
        errors.record('report.submit', exception)
        # ⭐ Die Weiterleitung bremst die Menge je Absender.
        # Wer zweimal schnell hintereinander drückt, soll hören, dass er nur
        # kurz warten muss — nicht, dass keine Verbindung besteht.
        if getattr(exception, 'code', None) == 429:
            return False, t('m_bericht_zuviel')
        if getattr(exception, 'code', None) == 413:
            return False, t('m_bericht_zugross')
        # ⚠ Den Grund sonst NICHT durchreichen: In der Fehlermeldung einer
        # fehlgeschlagenen Anfrage kann die Adresse stehen.
        return False, t('m_bericht_weg')


def multipart(limit, head, name, text, attachments=None):
    """Den Körper der Anfrage bauen — Bytes, damit auch Bilder hineinpassen."""
    parts = [('--%s\r\nContent-Disposition: form-data; name="content"\r\n\r\n%s\r\n'
              % (limit, head)).encode('utf-8'),
             ('--%s\r\nContent-Disposition: form-data; name="files[0]"; '
              'filename="%s"\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n'
              % (limit, name)).encode('utf-8') + text.encode('utf-8') + b'\r\n']
    # Discord nimmt höchstens zehn Dateien je Nachricht.
    for index, (file_name, data, mime) in enumerate((attachments or [])[:9], 1):
        parts.append(('--%s\r\nContent-Disposition: form-data; name="files[%d]"; '
                      'filename="%s"\r\nContent-Type: %s\r\n\r\n'
                      % (limit, index, file_name, mime)).encode('utf-8')
                     + data + b'\r\n')
    parts.append(('--%s--\r\n' % limit).encode('utf-8'))
    return b''.join(parts)


# Gleiche Grenzen wie in der Weiterleitung (`tools/bericht-worker/worker.js`).
SEND_LIMIT = 8 * 1024 * 1024
IMAGE_LIMIT = 4
IMAGE_BUDGET = 7 * 1024 * 1024
IMAGE_PATTERNS = (('PNG / JPG', '*.png *.jpg *.jpeg'),)

PNG_SIGNATURE = b'\x89PNG\r\n\x1a\n'
JPEG_START = b'\xff\xd8\xff'
# Textfelder, Exif und Zeitstempel — für die Anzeige des Bildes nicht nötig.
PNG_METADATA = frozenset((b'tEXt', b'zTXt', b'iTXt', b'eXIf', b'tIME'))
# APP0 (JFIF), APP2 (ICC-Farbprofil) und APP14 (Adobe-Farbumwandlung) braucht
# der Decoder; die übrigen APPn (Exif, XMP, IPTC …) und Kommentare nicht.
JPEG_KEEP_APP = frozenset((0xE0, 0xE2, 0xEE))


def image_kind(data):
    """'png', 'jpg' oder None — nach den ersten Bytes, nicht nach der Endung."""
    if data.startswith(PNG_SIGNATURE):
        return 'png'
    if data.startswith(JPEG_START):
        return 'jpg'
    return None


def _png_without_metadata(data):
    """Die PNG ohne Text- und Exif-Blöcke; None, wenn sie kaputt ist.

    Alles nach `IEND` fällt weg — dort kann beliebiges angehängt sein."""
    out = [PNG_SIGNATURE]
    pos = len(PNG_SIGNATURE)
    while pos + 12 <= len(data):
        length = int.from_bytes(data[pos:pos + 4], 'big')
        chunk = data[pos + 4:pos + 8]
        end = pos + 12 + length
        if end > len(data):
            return None
        if chunk not in PNG_METADATA:
            out.append(data[pos:end])
        pos = end
        if chunk == b'IEND':
            return b''.join(out)
    return None


def _jpeg_without_metadata(data):
    """Die JPEG ohne Exif/XMP/IPTC und Kommentare; None, wenn sie kaputt ist.

    Die Bilddaten nach jedem SOS werden bis zum nächsten echten Marker
    übernommen (`FF 00` und `FF D0`–`D7` gehören zu den Daten). Alles nach
    `EOI` fällt weg — dort hängen manche Kameras ganze Videos an."""
    out = [data[:2]]
    pos = 2
    size = len(data)
    while pos + 2 <= size:
        if data[pos] != 0xFF:
            return None
        marker = data[pos + 1]
        if marker == 0xFF:
            pos += 1
            continue
        if marker == 0xD9:
            out.append(data[pos:pos + 2])
            return b''.join(out)
        if 0xD0 <= marker <= 0xD7 or marker == 0x01:
            out.append(data[pos:pos + 2])
            pos += 2
            continue
        if pos + 4 > size:
            return None
        length = int.from_bytes(data[pos + 2:pos + 4], 'big')
        end = pos + 2 + length
        if length < 2 or end > size:
            return None
        if marker == 0xDA:
            scan = end
            while True:
                found = data.find(b'\xff', scan)
                if found < 0 or found + 1 >= size:
                    return None
                follow = data[found + 1]
                if follow == 0x00 or 0xD0 <= follow <= 0xD7:
                    scan = found + 2
                elif follow == 0xFF:
                    scan = found + 1
                else:
                    break
            out.append(data[pos:found])
            pos = found
            continue
        drop = marker == 0xFE or (0xE0 <= marker <= 0xEF
                                  and marker not in JPEG_KEEP_APP)
        if not drop:
            out.append(data[pos:end])
        pos = end
    return None


def strip_metadata(data, kind):
    """Bildinhalt unverändert, versteckte Zusatzangaben entfernt (oder None)."""
    if kind == 'png':
        return _png_without_metadata(data)
    if kind == 'jpg':
        return _jpeg_without_metadata(data)
    return None


def load_image(path):
    """Ein vom Nutzer gewähltes Bild für den Bericht einlesen.

    Gibt (Bytes, Art, None) oder (None, None, Textschlüssel) zurück. Geprüft
    wird der Inhalt, nicht die Endung: Nur echte PNG und JPEG gehen durch.
    """
    try:
        with open(path, 'rb') as handle:
            data = handle.read(IMAGE_BUDGET + 1)
    except OSError as exception:
        errors.record('report.load_image', exception)
        return None, None, 's_di_bild_unlesbar'
    if len(data) > IMAGE_BUDGET:
        return None, None, 's_di_bild_zugross'
    kind = image_kind(data)
    clean = strip_metadata(data, kind) if kind else None
    if clean is None:
        return None, None, 's_di_bild_art'
    return clean, kind, None


def image_files(images):
    """Die gewählten Bilder als Anhänge (Name, Bytes, MIME-Typ).

    `images`: Liste von (Bytes, Art). Die Dateinamen werden neu vergeben —
    der ursprüngliche Name kann den Benutzernamen oder einen Pfad tragen.
    """
    mime = {'png': 'image/png', 'jpg': 'image/jpeg'}
    return [('bild-%d.%s' % (number, kind), data, mime[kind])
            for number, (data, kind) in enumerate(images[:IMAGE_LIMIT], 1)]


def to_archive(text, root=None):
    """Den Bericht in die Zwischenablage legen. True, wenn es geklappt hat."""
    try:
        if root is None:
            return False
        root.clipboard_clear()
        root.clipboard_append(text)
        root.update()          # ohne das ist die Ablage nach dem Beenden leer
        return True
    except Exception:
        return False


# GitHub schneidet sehr lange Adressen ab. Der Bericht ist normalerweise gut
# 1 KB groß; die Grenze greift erst, wenn jemand mit 50 Fehlern im Gepäck meldet.
URL_GRENZE = 6000
ISSUE_ADRESSE = 'https://github.com/Xharig/VerseKit/issues/new'


def _template_for_language():
    """Deutsche Oberfläche → deutsches Formular, sonst das englische.

    ⚠ **Der Rückfall ist das deutsche Formular** — Deutsch ist
    die Hauptsprache des Projekts. Er greift nur, wenn sich die eingestellte
    Sprache nicht ermitteln lässt; das ist ein Ausnahmefall, und dann ist die
    Hauptsprache die bessere Wahl als die zweite.

    ⚠⚠ **Die beiden Dateinamen sind festgenagelt.** Jede ausgelieferte Fassung
    schickt `template=bug.yml` bzw. `template=fehler.yml` mit — wer sie
    umbenennt (etwa um die deutsche in der GitHub-Auswahl nach oben zu
    sortieren), lässt bei allen älteren Fassungen den vorausgefüllten Bericht
    ins Leere laufen.
    """
    try:
        from . import language
        return 'bug.yml' if language.current() == 'en' else 'fehler.yml'
    except Exception:
        return 'fehler.yml'


def issue_url(text, title='', template=None):
    """Eine Adresse, die bei GitHub ein **vorausgefülltes** Formular öffnet.

    Warum dieser Weg und kein Absenden aus dem Programm heraus: Ein Issue
    anzulegen verlangt einen Zugangsschlüssel. Einen eigenen mitzuliefern hieße,
    ihn zu verschenken — in einer `.exe` ist nichts geheim, und jeder könnte
    damit im Namen des Projekts schreiben. Den Spieler nach seinem zu fragen ist
    ihm nicht zuzumuten.

    Über die Adresse ist beides gelöst: Der Browser öffnet das Formular fertig
    ausgefüllt, der Spieler liest es und drückt selbst auf Abschicken. Er sieht
    also genau, was er weitergibt — und eingeloggt ist er dort ohnehin.
    """
    from urllib.parse import urlencode

    body = text or ''
    if len(body) > URL_GRENZE:
        body = body[:URL_GRENZE] + t('m_bericht_gekuerzt')

    values = {'template': template or _template_for_language(), 'bericht': body}
    if title:
        values['title'] = title
    return ISSUE_ADRESSE + '?' + urlencode(values)


def open_issue(text, title=''):
    """Das vorausgefüllte Formular im Browser öffnen. True, wenn es startete.

    ⚠ Über `paths.open_in_browser`, nicht über `webbrowser.open()` — im AppImage
    öffnet das nichts und meldet trotzdem Erfolg (Begründung dort).
    """
    try:
        return paths.open_in_browser(issue_url(text, title))
    except Exception:
        return False


def save(text, path_=None):
    """Den Bericht als Datei ablegen; gibt den Pfad zurück oder None."""
    try:
        path_ = path_ or paths.app_file('bericht.txt')
        with open(path_, 'w', encoding='utf-8') as f:
            f.write(text)
        return path_
    except Exception as exception:
        try:
            from . import errors
            errors.record('report.save', exception)
        except Exception:
            pass
        return None


if __name__ == '__main__':
    sys.stdout.write(build(version='3.0.0-dev') + '\n')
