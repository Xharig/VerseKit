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

„Bei mir geht es nicht" ist keine Fehlermeldung. Dieses Modul baut daraus einen
Textblock, den der Spieler in ein Issue einfügt — und der die Fragen schon
beantwortet, die man sonst einzeln stellen müsste: Welches System, welche
Verpackung, welche Tk-Version, welcher Bildschirmaufbau, ist das Spiel
gefunden, welche Sprache wurde erkannt, wie weit ist das Protokoll gelesen,
was steht im Katalog — und was ist zuletzt schiefgegangen.

Drei Regeln, die nicht verhandelbar sind:

  1. **Keine Namen.** Jeder Wert läuft durch `pfade.kuerzen()`; aus
     `/home/spieler/…` wird `<heim>/…`. Ein Bericht landet in einem
     **öffentlichen** Issue.
  2. **Nichts wird verschickt.** Das Modul gibt Text zurück, mehr nicht. Ob er
     jemanden erreicht, entscheidet allein der Spieler.
  3. **Der Bericht darf nie scheitern.** Jede Angabe wird einzeln geholt; was
     nicht zu ermitteln ist, steht als `—` da. Ein Bericht, der abbricht, weil
     eine Kleinigkeit fehlt, ist genau dann nutzlos, wenn man ihn braucht.
"""
import json
import os
import platform
import sys
from datetime import datetime

from . import fehler, overlay, pfade
from .sprache import t


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
            from . import fehler
            fehler.merken('report.angabe', exception)
        except Exception:
            pass              # das Melden darf den Bericht nie umwerfen
        return default


def _wrap(text, width):
    """Einen Satz auf mehrere Zeilen verteilen, ohne Wörter zu zerschneiden.

    ⚠ Kein `textwrap`: Der Bericht soll auch dann noch stehen, wenn jemand
    eine einzelne Zeile ohne Leerzeichen einträgt (ein Pfad zum Beispiel) —
    die bleibt hier ganz, statt hart getrennt zu werden. Lieber eine zu lange
    Zeile als ein zerschnittener Dateiname.

    ⚠⚠ **Eigene Zeilenumbrüche bleiben stehen.** Bis zum 05.09.2026 zerlegte
    ein einzelnes `.split()` den ganzen Text an jedem Leerraum — auch an
    `\\n`. Solange das Eingabefeld einzeilig war, fiel das nicht auf; seit es
    vier Zeilen hat, tippen Leute Aufzählungen:

        1. Läden öffnen
        2. Auf einen Radar klicken
        3. nichts passiert

    Daraus wurde eine einzige Zeile — aus drei Schritten ein Klumpen, und
    genau die Abfolge ist das, was eine Meldung brauchbar macht. Umbrochen
    wird deshalb **je Zeile**; leere Zeilen bleiben als Absatztrenner.
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
    Zeilen — und der Ausschnitt sagt danach nichts mehr aus. Genau so kam der
    rc42-Bericht an (30.08.2026): zwölfmal „Liste: zeichnen beginnt".

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

    ⚠⚠ **Diese Zeile ersetzt eine Rueckfrage, die oft nicht moeglich ist.** Am
    31.08.2026 kam ein Bericht mit „462 Protokolle" und „0 Baupläne", ohne
    Absender und ohne Nachricht. Daraus war nicht zu erkennen, ob die Erkennung
    bei dem Menschen versagt oder ob er einfach neu im Spiel ist — und genau
    das ist der Unterschied zwischen „alles in Ordnung" und „das Werkzeug ist
    fuer ihn wertlos".

    Jetzt beantwortet der Bericht es selbst:

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
    from . import logsource, pfade as paths_module

    # ⚠ **Jeder Schritt fuer sich abgesichert, auch der erste.** Diese Zeile
    # steht in einem Bericht, den jemand abschickt, WEIL schon etwas kaputt
    # ist — eine ausgehaengte Platte darf ihn nicht um den Rest bringen. Beim
    # Bauen lag der erste Aufruf zunaechst ausserhalb; Selbsttest 94 hat es
    # sofort gemeldet.
    backups = []
    try:
        backups = paths_module.log_sicherungen()
    except Exception:
        pass
    # ⚠ Einzahl beachten: „1 Protokolle" stand so im Bericht (02.09.2026).
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
    fehlt in der anderen. Am 30.08.2026 gemeldet: Bericht 315, Liste 292 — und
    beide Zahlen stimmten. Wer das sieht, hält eine davon für kaputt.

    Deshalb steht die Differenz jetzt im Bericht, statt dass sie jemand suchen
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
# und für die Frage „woran liegt es" reicht eine Handvoll Beispiele.
UNBEKANNT_MAX = 12


def _unknown_blueprints():
    """Die Baupläne im eigenen Bestand, die der Katalog nicht kennt.

    ⚠ Die Zahl allein („23 unbekannt") sagt nur, dass etwas nicht zusammenpasst.
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

    ⚠ Hier stand eine einzige Herkunft hinter der **ganzen** Liste. Die Liste
    ist aber gemischt: belegte Formulierungen (eigene Angabe, `global.ini`) und
    die eingebaute Rückfalltabelle. Der Bericht las sich dadurch so, als stünden
    alle sieben in der `global.ini` — dort steht genau eine. Am 01.09.2026
    kostete das drei Suchläufe in einer 12-MB-Datei, bis klar war, dass „Bauplan
    überchoo" (Schweizerdeutsch) aus der Tabelle kommt und dort gar nicht stehen
    kann. Ein Bericht, der eine falsche Herkunft behauptet, schickt die
    Fehlersuche in die Irre — genau das, was er verhindern soll."""
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

    ⚠ Diese Zeile gibt es, weil ein Fehler sich hier drei Wochen lang verstecken
    konnte: Ein eigener Fund überschrieb die mitgelieferte Liste derselben
    Version, und aus 24 Bauplänen in 4.10.0 wurden 3. Im Bericht stand nur der
    Katalogstand — der war völlig in Ordnung, die Historie darunter nicht. Wer
    „der Patch-Filter zeigt fast nichts" meldet, soll die Zahlen sehen können,
    ohne dass jemand erst eine JSON-Datei aufmacht.

    ⚠ Die Kurzform allein reicht nicht. `4.10.0-live.12519617` und
    `4.10.0-live.12545750` kürzen beide auf „4.10.0"; im Bericht stand dann
    zweimal „4.10.0" mit verschiedenen Zahlen, und niemand konnte zuordnen,
    welcher Patch welche Zugänge brachte — ausgerechnet in der Zeile, die es
    zum Zuordnen gibt. Darum: Kurzform nur, solange sie eindeutig ist, sonst
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

    ⚠ Hier stand `daten.get(schluessel, daten)` — fehlte der Schlüssel, wurde
    also das **ganze** Wörterbuch gezählt. Der Bericht meldete damit „3
    Baupläne", weil die Datei drei Felder oben hat (version, stand, bauplaene),
    während darin 394 Baupläne standen. Eine falsche Zahl, die völlig plausibel
    aussieht — genau die Sorte, die niemand nachprüft.

    Fehlt der Schlüssel, steht jetzt `—` da. Lieber keine Angabe als eine
    erfundene, gerade in einem Bericht, mit dem jemand einen Fehler sucht.
    ⚠ Und: Eine Datei, die **es gar nicht gibt**, ist hier kein Fehler, sondern
    der Normalfall. Wer noch nichts auf die Merkliste gesetzt hat, hat keine
    `watchlist.json` — bis rc42 flog dabei ein `FileNotFoundError`, den `_sicher`
    zwar auffing, aber als Fehler in den Bericht schrieb. Im Bericht vom
    26.08.2026 stand er ganz oben, direkt über den echten Altlasten:

        report.angabe  FileNotFoundError: .../Bauplaene/watchlist.json

    Wer einen Fehler sucht, soll in dieser Liste keine Zeilen finden, die gar
    keine sind. Der Docstring von `_sicher` sagt es schon: „Ein leerer Wert ist
    normal (kein Spiel installiert, keine Merkliste) — eine Ausnahme ist es
    nicht."
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
    Unterschiede im Aufbau der Oberflaeche sind genau dort zu erwarten: Am
    02.09.2026 kam eine Meldung ueber traegen Fensteraufbau von einem Rechner
    mit Tk 9.0, waehrend dieselbe Fassung unter 8.6 zuegig lief. Ohne die
    genaue Nummer im Bericht laesst sich so etwas nicht zuordnen.
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
    # Bei der Bezeichner-Migration am 12.09.2026 war das die EINZIGE Stelle
    # im Projekt, die weder der Syntaxbaum-Scanner noch der Selbsttest
    # gemeldet hätte: Sie bricht erst, wenn jemand einen Fehlerbericht baut.
    # Gefunden per Textsuche über das ganze Repo, nachdem alles grün war.
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
    # ⭐ **Fenstermaße dazu.** Am 30.08.2026 meldete ein Nutzer, das Fenster sei
    # zu groß und er komme „nicht mehr an alles ran" — im Bericht stand dazu
    # keine einzige Zahl. Sichtbar war nur der Bildschirm, nicht das Fenster
    # darauf und schon gar nicht das `minsize`, das den Fehler ausmachte:
    # Ist die Mindesthöhe größer als der Bildschirm, hält Tk sie gegen jedes
    # Verkleinern. Genau diese drei Zahlen nebeneinander beantworten die Frage
    # in einer Zeile.
    try:
        fb, fh = root.winfo_width(), root.winfo_height()
        mb, mh = root.minsize()
        if fb > 50 and fh > 50:
            line += t('b_fenstermass') % (fb, fh, mb, mh)
            if mh > height:
                line += t('b_fenster_zu_hoch')
    except Exception:
        pass
    return line


def _game_launcher():
    """Der Weg, auf dem Star Citizen gestartet würde — gekürzt und eingeordnet.

    Drei Auskünfte in einer Zeile: **ob** etwas gefunden wurde, **was**, und ob
    es der selbst eingetragene Startbefehl ist. Genau diese drei Fragen standen
    am 27.08.2026 zwei Stunden lang im Raum.
    """
    from . import pfade as paths_module
    from . import sprache as language_module
    launcher = paths_module.spielstarter()
    if not launcher:
        return language_module.t('b_starter_kein')
    short = paths_module.kuerzen(str(launcher))
    own_flag = (paths_module.einstellung('spielstarter') or '').strip()
    if own_flag:
        return language_module.t('b_starter_eigen', short)
    return short


def _injection_state():
    """Stehen die Bauplan-Angaben im Spiel? Eine Zeile, die einen Anruf spart.

    ⚠ Der häufigste Support-Fall lautet „ich sehe deine Angaben im Spiel nicht
    mehr". Ursache ist fast immer, dass ein Übersetzungs-Update oder ein
    Spiel-Patch die `global.ini` neu geschrieben und die Angaben dabei
    stillschweigend entfernt hat — das Werkzeug merkt davon nichts.

    Am 28.08.2026 stand in Morkhans Bericht nur `inj_quelle=deutsch`. Ob
    überhaupt etwas eingetragen war, ließ sich daraus nicht ablesen; es musste
    erschlossen werden. Genau dafür gibt es den Bericht.

    Die Auskunft kommt aus `injektion.lage()` — derselben Stelle, die auch das
    Einstellungsfenster anzeigt. Kosten: rund 20 ms für eine 9-MB-Datei,
    gemessen; das fällt neben dem Rest nicht auf.
    """
    from . import injektion
    lage = injektion.lage()
    # ⚠ „Keine Datei" ist NICHT dasselbe wie „nicht eingetragen". Wer unter
    # Linux ohne Übersetzung spielt, hat schlicht keine `global.ini` — dort
    # wäre ein fettes „NICHT eingetragen" eine Warnung vor dem Normalzustand.
    if not lage['datei']:
        return t('b_inj_keine')
    parts = [t('b_inj_drin') if lage['drin'] else t('b_inj_weg')]
    # ⚠ Beide Schalter stehen auf „an", solange niemand sie anfasst — dann
    # tauchen sie in `selbst_gesetzt` NICHT auf. Ohne diese zwei Angaben liest
    # man „nicht eingetragen" und weiß nicht, ob das Absicht ist.
    if not pfade.einstellung_wahrheit('inj_an', True):
        parts.append(t('b_inj_aus'))
    parts.append(t('b_inj_auto')
                 if pfade.einstellung_wahrheit('inj_auto', True)
                 else t('b_inj_hand'))
    if lage['quelle']:
        parts.append('%s %s' % (lage['quelle'], lage['stand'] or ''))
    # ⭐⭐ **Welche Sprachdatei — und welche das Spiel wirklich lädt.**
    #
    # Es gibt `english/global.ini` und `german_(germany)/global.ini`, und der
    # Watcher pflegt genau eine davon. Steht die Angabe in der anderen, sieht
    # der Spieler **nichts** und meldet „funktioniert nicht" — ohne dass
    # irgendetwas kaputt wäre.
    #
    # ⚠ Am 29.08.2026 kostete genau das einen Abend, und am 13.09.2026 kam
    # dieselbe Meldung von einem zweiten Nutzer („bei Original habe ich keine
    # Kästen gehabt"). Beide Male war die Frage „welche Datei gegen welche
    # Sprache" nicht aus dem Bericht zu beantworten — sie musste erschlossen
    # werden. Genau dafür ist der Bericht da.
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
        fehler.merken('report.injektionssprache', exception)
    return ' · '.join(x for x in parts if x)


def _crash_brief(lines, limit):
    """Aus einem harten Abbruch die Zeilen, die etwas sagen.

    ⛔⛔ **Der schuldige Faden zuerst — sonst steht im Bericht Beiwerk.**
    Python schreibt bei einem harten Abbruch **alle** Fäden weg und markiert
    den, in dem es knallte, mit `Current thread`. Wo der in der Liste steht,
    ist Zufall.

    Bis zum 14.09.2026 nahm der Bericht schlicht die ersten 14 Zeilen. In einem
    echten Bericht dieses Tages (Heap-Korruption `0xc0000374`) waren das drei
    Fäden, die alle nur **warteten**: `GetMessageW` im Tastenkürzel-Faden,
    `socket.accept` im Overlay, die Tray-Schleife. Der Faden, der abgestürzt
    ist, stand weiter unten und wurde von „… (19)" verschluckt.

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


def build(version='', root=None, fehleranzahl=8, message=''):
    """Den Bericht als Text zusammensetzen."""
    lines = []

    def line(label, value):
        lines.append('%-18s%s' % (label, pfade.kuerzen(value)))

    lines.append(t('b_kopf')
                  % (version or '—', datetime.now().strftime(t('b_datum'))))
    lines.append('')

    # ⭐ Wer meldet das? Steht bewusst ganz oben — mit vielen Nutzern ist ein
    # Bericht ohne Absender kaum zuzuordnen, und Rückfragen laufen ins Leere.
    # **Freiwillig**: Ist nichts eingetragen, steht hier „nicht angegeben"; der
    # Watcher füllt das Feld nie von selbst.
    reporter = (pfade.einstellung('melder_name') or '').strip()
    # ⚠ **Ohne `kuerzen()`.** Jede andere Zeile läuft durch die Anonymisierung,
    # die Benutzernamen durch `<benutzer>` ersetzt — und genau das traf den
    # Melder-Namen, wenn er dem Systemkonto gleicht („Xharig"). Ausgerechnet
    # die einzige Angabe, die der Nutzer BEWUSST macht, verschwand dadurch.
    # Aufgefallen am 29.08.2026 auf einem Bildschirmfoto, nicht im Test.
    lines.append('%-18s%s' % (t('b_melder'), reporter or t('s_melder_leer')))
    # ⭐⭐ **Was ist passiert — in eigenen Worten, ganz oben.** Ohne dieses Feld
    # landete die Meldung im Namen: „BUSHWICK mission log updated niocht"
    # (05.09.2026). Der Hinweis war da, nur an der falschen Stelle.
    #
    # ⚠ Steht **vor** allen technischen Angaben. Was der Mensch schreibt, ist
    # der Anfang jeder Diagnose — die Zahlen darunter belegen oder widerlegen
    # es. Umgekehrt sucht man erst in 60 Zeilen Technik nach dem Grund, warum
    # jemand überhaupt geschrieben hat.
    #
    # ⚠ Mehrzeilig eingerückt, damit ein langer Satz die Spaltenform nicht
    # sprengt — und durch `kuerzen()`, denn hier kann ein Pfad drinstehen.
    note = (message or '').strip()
    if note:
        lines.append('')
        lines.append(t('b_meldung'))
        for piece in _wrap(pfade.kuerzen(note), 76):
            lines.append('  ' + piece)
    lines.append('')

    uebersicht = _safe(pfade.uebersicht, {})
    if not isinstance(uebersicht, dict):
        uebersicht = {}

    line(t('b_system'), _safe(_system))
    line(t('b_verpackung'), _safe(_packaging_readable))
    line(t('b_python'), '%s / %s' % (platform.python_version(),
                                      _safe(_tk_version)))
    line(t('b_bildschirm'), _safe(lambda: _screens(root)))
    # ⭐ **Wie das Overlay gerade steht.** Am 13.09.2026 kostete eine Meldung
    # ueber das schwebende Schloss einen ganzen Abend Messungen, weil hier
    # nichts davon stand: keine Fenstergroesse, kein Klappzustand, keine
    # Mindestbreite, keine Leistengroesse, kein Versatz. Der Bericht waechst
    # deshalb mit — eine Zeile beantwortet, wofuer sonst nachgefragt wird.
    #
    # ⚠ Nur Zahlen und Zustaende; der Bericht landet in einem oeffentlichen
    # Issue. Steht kein Overlay (Pruefstand, reines Fensterprogramm), bleibt
    # die Zeile weg statt „unbekannt" zu melden.
    _state = _safe(lambda: (overlay.STATE_REPORT[0] or (lambda: ''))())
    if _state:
        line(t('b_overlay'), _state)
    lines.append('')

    line(t('b_spiel'), _safe(lambda: uebersicht.get('spiel_ordner')
                                 or t('b_nicht_gefunden')))
    line(t('b_gamelog'), _safe(lambda: uebersicht.get('game_log')
                                   or t('b_nicht_gefunden')))
    line(t('b_sicherungen'), _safe(_log_line))
    line(t('b_launcher'), _safe(lambda: uebersicht.get('launcher')
                                    or t('b_nicht_da')))
    # ⚠ **Womit sich das Spiel starten ließe — und ob das jemand von Hand
    # eingetragen hat.** Ohne diese Zeile ist „der Startknopf tut nichts" nicht
    # zu beantworten, ohne den Nutzer auszufragen. Siehe die Regel: Was einen
    # Fehler erklären würde, gehört in den Bericht, bevor er das nächste Mal
    # gemeldet wird.
    line(t('b_starter'), _safe(_game_launcher))
    # ⚠ `collect()` gibt ein **Tupel** zurück — (phrases, herkunft). Hier stand
    # `', '.join(sammeln())`, was eine Liste mit einem String zusammenfügen
    # wollte und mit einem TypeError abbrach. `_sicher()` verschluckte den, und
    # im Bericht stand nur ein Strich. Drei Übergaben lang galt das als
    # ungeklärter Punkt; in Wahrheit war es diese eine fehlende `[0]`.
    #
    # Die Herkunft wird gleich mit ausgegeben: Sie sagt, ob die Formulierung aus
    # der echten `global.ini` des Spielers stammt oder nur aus unserer Tabelle
    # geraten ist — genau die Auskunft, die man bei „er erkennt meine Baupläne
    # nicht" als Erstes braucht.
    line(t('b_spielsprache'), _safe(_game_language))
    line(t('b_inj'), _safe(_injection_state))
    line(t('b_inj_datei'), _safe(
        lambda: __import__('scbp.injektion', fromlist=['ini_datei'])
        .ini_datei()[0] or t('b_inj_keine')))
    lines.append('')

    line(t('b_bestand'), _safe(_collection_line))
    _unknown = _safe(_unknown_blueprints, '')
    if _unknown and _unknown != '—':
        line(t('b_unbekannt'), _unknown)
    line(t('b_merkliste'), _safe(lambda: t('b_n_eintraege') % _json_size(
        __import__('scbp.watchlist', fromlist=['path']).path(), 'eintraege')))
    # ⚠⚠ **Der gespeicherte Stand, kein Netzabruf.** Hier stand
    # `aktuelle_version()` — und die fragt scmdb.net. Ohne Internet wartete der
    # Bericht auf den Timeout, und weil er im Hauptfaden gebaut wird, war das
    # ganze Fenster so lange starr: „ohne Internetverbindung geht auch Fehler
    # melden nicht aufzurufen, Einstellungsfenster ist auch da nicht mehr
    # bedienbar" (30.08.2026). Ausgerechnet die Seite, die man bei Störungen
    # braucht.
    #
    # Der Bericht soll ohnehin den **Ist-Zustand auf diesem Rechner** zeigen,
    # nicht den im Netz: Interessant ist, welchen Katalog der Nutzer hat.
    line(t('b_katalog'), _safe(lambda: (__import__(
        'scbp.catalog', fromlist=['load']).load().get('version') or None)))
    line(t('b_historie'), _safe(_patch_history))

    # ⚠ Die drei Werkstatt-Seiten (ab v3.3.0). Ohne sie liesse sich eine
    # Meldung wie „bei mir bleibt die Herstellung leer" nicht beurteilen —
    # man saehe nicht, ob die Daten ueberhaupt geladen sind.
    def _storage_line():
        from . import materials
        items = materials.load()
        kinds = {(p_.get('material') or '').strip().lower() for p_ in items}
        return t('b_n_posten') % (len(items), len(kinds - {''}))

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
    lines.append('')

    line(t('b_ordner'), _safe(lambda: uebersicht.get('app_ordner')))
    line(t('b_einstellungen'), _safe(lambda: ', '.join(
        '%s=%s' % (k, v) for k, v in sorted(
            (uebersicht.get('selbst_gesetzt') or {}).items()))
        or t('b_standard')))

    # ⚠ Die Startspur zuerst — bei einem Absturz ist sie das Einzige, was bleibt.
    # Ein `SIGSEGV` beendet den Prozess sofort: kein `except`, kein Fehlerbericht,
    # nur „es stürzt ab". Die letzte Zeile hier sagt, wie weit der Start kam.
    # ⚠ Start und Bedienung **getrennt** deckeln. Beides in einen Topf zu werfen
    # und die letzten zwölf Zeilen zu nehmen, war der Fehler in rc74: Fünf Klicks
    # genügten, und der komplette Startverlauf war aus dem Bericht verdrängt —
    # ausgerechnet der Teil, für den die Spur gebaut wurde.
    start, seiten = _safe(fehler.spur_geteilt, ([], []))
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
    while seiten and 'Seite diagnose' in seiten[-1]:
        seiten.pop()
    seiten = _dense(seiten)
    if seiten:
        lines.append('')
        lines.append(t('b_spur_seiten'))
        # ⚠⚠ **24 Zeilen, nicht 12 — der Weg zum Bericht frisst die Spur.**
        # Jede Seite belegt zwei Zeilen; zwölf zeigten also sechs Seiten. Wer
        # den Bericht holt, klickt sich aber erst durch die Info-Seiten dorthin
        # — und schob damit genau die Seite hinaus, um die es ging. Am
        # 05.09.2026 aufgefallen: „Hab Läden geöffnet und auch mal was
        # gesucht", und im Bericht stand keine Zeile davon.
        #
        # Ein Bericht, dessen Beschaffung die Beobachtung zerstört, ist kein
        # Bericht. `SPUR_REST` hebt 60 Zeilen auf, der Platz war also da.
        for entry in seiten[-24:]:
            lines.append('  ' + entry)

    # ⚠ Und danach der harte Abbruch, falls es einen gab. Er steht **vor** den
    # Fehlern, weil er der schwerere Befund ist: Ein Eintrag in der Fehlerliste
    # heißt, das Programm hat weitergelebt; hier war es mitten im Befehl weg.
    # Nur die erste Handvoll Zeilen — der volle Aufrufweg aller Fäden füllt
    # Seiten, und der Melder soll den Bericht noch verschicken können.
    crash = _safe(fehler.letzter_absturz, [])
    if crash:
        lines.append('')
        lines.append(t('b_absturz'))
        for entry in _crash_brief(crash, 14):
            lines.append('  ' + entry)
        if len(crash) > 14:
            lines.append('  … (%d)' % (len(crash) - 14))

    letzte = _safe(lambda: fehler.letzte(fehleranzahl), [])
    total = _safe(fehler.anzahl, 0)
    lines.append('')
    if letzte:
        lines.append(t('b_fehler') % (len(letzte), total))
        # ⚠ **Gleichartige Fehler zusammenfassen.** Ein einziger Vorfall kann
        # den ganzen Speicher belegen: Am 28.08.2026 stand in einem Bericht
        # **50 von 50** Plätzen dieselbe Zeile, alle innerhalb von acht Sekunden
        # (ein Fortschritt im Sekundentakt bei zugehendem Fenster). Acht davon
        # wurden angezeigt — acht Zeilen, die dasselbe sagen, und kein Platz für
        # das, was sonst noch passiert ist.
        #
        # Die Ursache dafür ist behoben, aber das Muster kann jederzeit
        # wiederkommen: Jeder Fehler in einer Schleife tut das. Deshalb wird hier
        # gebündelt, was sich nur in der Uhrzeit unterscheidet — dieselbe Stelle,
        # dieselbe Art, dieselbe Meldung, dieselbe Fassung.
        bundled = []
        for e in letzte:
            ident = (e.get('fassung'), e.get('stelle'), e.get('art'),
                       e.get('meldung'))
            if bundled and bundled[-1][0] == ident:
                bundled[-1][2] += 1
                bundled[-1][3] = e.get('zeit', '—')
            else:
                bundled.append([ident, e, 1, e.get('zeit', '—')])

        for _ident, e, how_often, bis in bundled:
            # ⚠ Die Version dazuschreiben und kennzeichnen, was **nicht** aus
            # der laufenden Fassung stammt. Ohne die Angabe sucht der Nächste
            # nach einem Fehler, den es vielleicht nicht mehr gibt.
            #
            # ⚠⚠ **Die Kennzeichnung stellt fest, sie urteilt nicht.** Sie hieß
            # bis 05.09.2026 „vermutlich längst behoben" — das stimmt aber nur,
            # wenn der Melder seine Fassung selbst überholt hat. Wer lange kein
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
                lines.append(t('b_fehler_mehrfach') % (how_often, bis))
    else:
        lines.append(t('b_fehler_keine'))

    lines.append('')
    lines.append(t('b_fuss'))
    return '\n'.join(lines)


def submit(text, version=''):
    """Den Bericht an den eingebauten Kanal schicken. (Erfolg, Meldung).

    ⚠ **Der einzige Weg, der bei Nicht-Bastlern ankommt.** Kopieren und in
    Discord einfügen scheitert dreifach: Der Bericht steckt unter
    „Fortgeschritten", er ist zu lang für eine Nachricht, und man muss wissen,
    wohin damit. Gemeldet am 28.08.2026: „ich will nicht jedem eine Stunde
    erklären, wie ich zu dem Bericht komme."

    Verschickt wird **nur auf Knopfdruck** und erst, nachdem der Nutzer den
    vollen Wortlaut gesehen hat. Der Text ist derselbe, der auf der Seite steht
    — durch `pfade.kuerzen()` von Namen und Pfaden befreit.

    Als **Datei**, nicht als Nachricht: Discord nimmt höchstens 2000 Zeichen je
    Nachricht, ein Bericht ist regelmäßig länger. Eine angehängte `.txt` ist
    zudem das, was man lesen und aufheben kann.
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

    parts = []
    for field, value in (('content', head),):
        parts.append('--%s\r\nContent-Disposition: form-data; name="%s"\r\n\r\n%s\r\n'
                     % (limit, field, value))
    parts.append('--%s\r\nContent-Disposition: form-data; name="files[0]"; '
                 'filename="%s"\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n%s\r\n'
                 % (limit, name, text))
    parts.append('--%s--\r\n' % limit)
    torso = ''.join(parts).encode('utf-8')

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
        fehler.merken('report.submit', exception)
        # ⚠ Den Grund NICHT durchreichen: In der Fehlermeldung einer
        # fehlgeschlagenen Anfrage steht die Adresse, und die ist geheim.
        return False, t('m_bericht_weg')


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

    ⚠ **Der Rückfall ist seit 31.08.2026 das deutsche Formular** — Deutsch ist
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
        from . import sprache
        return 'bug.yml' if sprache.aktuelle() == 'en' else 'fehler.yml'
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
    also genau, was er weitergibt — und angemeldet ist er dort ohnehin.
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

    ⚠ Über `pfade.im_browser`, nicht über `webbrowser.open()` — im AppImage
    öffnet das nichts und meldet trotzdem Erfolg (Begründung dort).
    """
    try:
        return pfade.im_browser(issue_url(text, title))
    except Exception:
        return False


def save(text, path_=None):
    """Den Bericht als Datei ablegen; gibt den Pfad zurück oder None."""
    try:
        path_ = path_ or pfade.app_datei('bericht.txt')
        with open(path_, 'w', encoding='utf-8') as f:
            f.write(text)
        return path_
    except Exception as exception:
        try:
            from . import fehler
            fehler.merken('report.save', exception)
        except Exception:
            pass
        return None


if __name__ == '__main__':
    sys.stdout.write(build(version='3.0.0-dev') + '\n')
