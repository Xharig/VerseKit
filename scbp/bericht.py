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

from . import fehler, pfade
from .sprache import t


def _sicher(f, standard='—'):
    """Eine Angabe holen und dabei nichts riskieren.

    ⚠ Ein leerer Wert ist normal (kein Spiel installiert, keine Merkliste) —
    eine **Ausnahme** ist es nicht. Die wurde hier bisher stillschweigend
    verschluckt, und im Bericht stand nur ein Strich. So blieb der Fehler bei
    der Spielsprache drei Übergaben lang unentdeckt: Er sah aus wie „nichts
    gefunden", war aber ein TypeError.
    """
    try:
        wert = f()
        if wert is None or wert == '':
            return standard
        return wert
    except Exception as ausnahme:
        try:
            from . import fehler
            fehler.merken('bericht.angabe', ausnahme)
        except Exception:
            pass              # das Melden darf den Bericht nie umwerfen
        return standard


def _gedraengt(eintraege):
    """Gleiche Zeilen hintereinander zu einer zusammenfassen.

    ⚠ Wozu: Der Bericht zeigt je Topf nur zwölf Zeilen. Eine Liste, die sich
    beim Tippen immer wieder neu zeichnet, schreibt in Sekunden zwölf gleiche
    Zeilen — und der Ausschnitt sagt danach nichts mehr aus. Genau so kam der
    rc42-Bericht an (30.08.2026): zwölfmal „Liste: zeichnen beginnt".

    Zusammengefasst wird nur, was **direkt hintereinander** gleich ist, und die
    Uhrzeit der ersten Zeile bleibt stehen — sonst ginge die Reihenfolge oder
    der Zeitpunkt verloren.
    """
    heraus = []
    for eintrag in eintraege:
        text = eintrag.split('  ', 1)[-1].strip()
        if heraus and heraus[-1][1] == text:
            heraus[-1][2] += 1
            continue
        heraus.append([eintrag, text, 1])
    return [zeile if zahl == 1 else '%s  (%d×)' % (zeile, zahl)
            for zeile, _text, zahl in heraus]


def _protokollzeile():
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
    from . import bestand as bestand_modul
    from . import logquelle, pfade as pfade_modul

    # ⚠ **Jeder Schritt fuer sich abgesichert, auch der erste.** Diese Zeile
    # steht in einem Bericht, den jemand abschickt, WEIL schon etwas kaputt
    # ist — eine ausgehaengte Platte darf ihn nicht um den Rest bringen. Beim
    # Bauen lag der erste Aufruf zunaechst ausserhalb; Selbsttest 94 hat es
    # sofort gemeldet.
    sicherungen = []
    try:
        sicherungen = pfade_modul.log_sicherungen()
    except Exception:
        pass
    # ⚠ Einzahl beachten: „1 Protokolle" stand so im Bericht (02.09.2026).
    teile = [t('b_protokolle_1' if len(sicherungen) == 1 else 'b_protokolle')
             % len(sicherungen)]
    try:
        stand = logquelle.Lesestand()
        gelesen = sum(1 for p in sicherungen if stand.kennt(p))
        teile.append(t('b_logs_gelesen') % gelesen)
    except Exception:
        pass
    try:
        quellen = bestand_modul.nach_quelle(bestand_modul.laden())
        aus_logs = quellen.get('log', 0) + quellen.get('nachlese', 0)
        teile.append(t('b_logs_funde_1' if aus_logs == 1 else 'b_logs_funde')
                     % aus_logs)
    except Exception:
        pass
    return ' · '.join(teile)


def _bestandzeile():
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
    from . import bestand as bestand_modul
    from . import katalog as katalog_modul
    daten = bestand_modul.laden()
    gesamt = bestand_modul.anzahl(daten)
    try:
        bekannt = set(katalog_modul.laden().get('bauplaene') or {})
    except Exception:
        bekannt = set()
    if not bekannt:
        return t('b_n_bauplaene') % gesamt
    im_katalog = len(bestand_modul.schluessel(daten) & bekannt)
    if im_katalog == gesamt:
        return t('b_n_bauplaene') % gesamt
    return t('b_n_bp_katalog') % (gesamt, im_katalog, gesamt - im_katalog)


# Wie viele Namen der Bericht höchstens aufzählt. Mehr macht ihn unlesbar,
# und für die Frage „woran liegt es" reicht eine Handvoll Beispiele.
UNBEKANNT_MAX = 12


def _unbekannte_bauplaene():
    """Die Baupläne im eigenen Bestand, die der Katalog nicht kennt.

    ⚠ Die Zahl allein („23 unbekannt") sagt nur, dass etwas nicht zusammenpasst.
    Die Namen sagen, **was** — und meistens auch gleich, warum: ein ganzes
    Rüstungsset, das der Katalog noch nicht führt, oder eine abweichende
    Schreibweise. Ohne sie muss jemand die Datei von Hand mit dem Katalog
    vergleichen; damit ist die Angabe im Bericht wertlos.
    """
    from . import bestand as bestand_modul
    from . import katalog as katalog_modul
    try:
        bekannt = set(katalog_modul.laden().get('bauplaene') or {})
    except Exception:
        return ''
    if not bekannt:
        return ''
    daten = bestand_modul.laden()
    fehlend = sorted((e.get('name') or k)
                     for k, e in daten['bauplaene'].items() if k not in bekannt)
    if not fehlend:
        return ''
    gezeigt = fehlend[:UNBEKANNT_MAX]
    text = ' · '.join(gezeigt)
    if len(fehlend) > UNBEKANNT_MAX:
        text += '  ' + t('b_und_weitere') % (len(fehlend) - UNBEKANNT_MAX)
    return text


def _spielsprache():
    """Wonach im Log gesucht wird — und woher **jede** Formulierung stammt.

    ⚠ Hier stand eine einzige Herkunft hinter der **ganzen** Liste. Die Liste
    ist aber gemischt: belegte Formulierungen (eigene Angabe, `global.ini`) und
    die eingebaute Rückfalltabelle. Der Bericht las sich dadurch so, als stünden
    alle sieben in der `global.ini` — dort steht genau eine. Am 01.09.2026
    kostete das drei Suchläufe in einer 12-MB-Datei, bis klar war, dass „Bauplan
    überchoo" (Schweizerdeutsch) aus der Tabelle kommt und dort gar nicht stehen
    kann. Ein Bericht, der eine falsche Herkunft behauptet, schickt die
    Fehlersuche in die Irre — genau das, was er verhindern soll."""
    from . import phrasen as phrasen_modul
    gefunden, _herkunft = phrasen_modul.sammeln()
    if not gefunden:
        return None
    eigene, aus_ini = phrasen_modul.gemessene()
    belegt = eigene + aus_ini
    rueckfall = [p for p in gefunden if p not in belegt]
    teile = []
    if eigene:
        teile.append('%s (%s)' % (', '.join(eigene), t('b_woher_eigen')))
    if aus_ini:
        teile.append('%s (%s)' % (', '.join(aus_ini), t('b_woher_ini')))
    if rueckfall:
        teile.append('%s (%s)' % (', '.join(rueckfall), t('b_woher_tabelle')))
    return ' · '.join(teile)


def _patchhistorie():
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
    from . import patchhistorie
    liste = patchhistorie.patches()
    if not liste:
        return None
    liste = liste[:5]
    kurzformen = [kurz for _voll, kurz, _anzahl in liste]
    return ', '.join(
        '%s (%d)' % (kurz if kurzformen.count(kurz) == 1 else voll, anzahl)
        for voll, kurz, anzahl in liste)


def _json_groesse(pfad_, schluessel):
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

        bericht.angabe  FileNotFoundError: .../Bauplaene/watchlist.json

    Wer einen Fehler sucht, soll in dieser Liste keine Zeilen finden, die gar
    keine sind. Der Docstring von `_sicher` sagt es schon: „Ein leerer Wert ist
    normal (kein Spiel installiert, keine Merkliste) — eine Ausnahme ist es
    nicht."
    """
    if not os.path.exists(pfad_):
        return '—'
    with open(pfad_, encoding='utf-8') as f:
        daten = json.load(f)
    if schluessel not in daten:
        return '—'
    wert = daten[schluessel]
    return len(wert) if hasattr(wert, '__len__') else '—'


def _system():
    name = platform.system()
    if name == 'Linux':
        kennung = _sicher(lambda: platform.freedesktop_os_release().get('PRETTY_NAME'), '')
        sitzung = os.environ.get('XDG_SESSION_TYPE', '')
        return ' · '.join(x for x in ('Linux', kennung, platform.release(), sitzung) if x)
    if name == 'Windows':
        return 'Windows %s · Build %s' % (platform.release(), platform.version())
    return '%s %s' % (name, platform.release())


def _tk_fassung():
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
        wurzel = tkinter._default_root
        if wurzel is not None:
            return str(wurzel.tk.call('info', 'patchlevel'))
    except Exception:
        pass
    return str(tkinter.TkVersion)


def _verpackung_lesbar():
    """Die Kennung aus `aktualisierung` in einen lesbaren Namen übersetzen.

    ⚠ Nur hier, nur für die Anzeige: Die Kennung selbst wird anderswo
    verglichen (`art == 'quellcode'`) und bleibt deshalb, wie sie ist.
    """
    art = __import__('scbp.aktualisierung',
                     fromlist=['verpackung']).verpackung()
    return {'quellcode': t('b_v_quellcode'),
            'exe': t('b_v_exe'),
            'appimage': t('b_v_appimage')}.get(art, art)


def _bildschirme(wurzel):
    """Größe und Skalierung — hier lagen schon zwei Fehler begraben."""
    if wurzel is None:
        return '—'
    breite = wurzel.winfo_screenwidth()
    hoehe = wurzel.winfo_screenheight()
    # 72 Punkte je Zoll ist Tks Bezug; daraus wird die Skalierung lesbar.
    skalierung = round(float(wurzel.tk.call('tk', 'scaling')) * 72 / 96 * 100)
    zeile = t('b_skalierung') % (breite, hoehe, skalierung)
    # ⭐ **Fenstermaße dazu.** Am 30.08.2026 meldete ein Nutzer, das Fenster sei
    # zu groß und er komme „nicht mehr an alles ran" — im Bericht stand dazu
    # keine einzige Zahl. Sichtbar war nur der Bildschirm, nicht das Fenster
    # darauf und schon gar nicht das `minsize`, das den Fehler ausmachte:
    # Ist die Mindesthöhe größer als der Bildschirm, hält Tk sie gegen jedes
    # Verkleinern. Genau diese drei Zahlen nebeneinander beantworten die Frage
    # in einer Zeile.
    try:
        fb, fh = wurzel.winfo_width(), wurzel.winfo_height()
        mb, mh = wurzel.minsize()
        if fb > 50 and fh > 50:
            zeile += t('b_fenstermass') % (fb, fh, mb, mh)
            if mh > hoehe:
                zeile += t('b_fenster_zu_hoch')
    except Exception:
        pass
    return zeile


def _spielstarter():
    """Der Weg, auf dem Star Citizen gestartet würde — gekürzt und eingeordnet.

    Drei Auskünfte in einer Zeile: **ob** etwas gefunden wurde, **was**, und ob
    es der selbst eingetragene Startbefehl ist. Genau diese drei Fragen standen
    am 27.08.2026 zwei Stunden lang im Raum.
    """
    from . import pfade as pfade_modul
    from . import sprache as sprache_modul
    starter = pfade_modul.spielstarter()
    if not starter:
        return sprache_modul.t('b_starter_kein')
    kurz = pfade_modul.kuerzen(str(starter))
    eigen = (pfade_modul.einstellung('spielstarter') or '').strip()
    if eigen:
        return sprache_modul.t('b_starter_eigen', kurz)
    return kurz


def _injektionslage():
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
    teile = [t('b_inj_drin') if lage['drin'] else t('b_inj_weg')]
    # ⚠ Beide Schalter stehen auf „an", solange niemand sie anfasst — dann
    # tauchen sie in `selbst_gesetzt` NICHT auf. Ohne diese zwei Angaben liest
    # man „nicht eingetragen" und weiß nicht, ob das Absicht ist.
    if not pfade.einstellung_wahrheit('inj_an', True):
        teile.append(t('b_inj_aus'))
    teile.append(t('b_inj_auto')
                 if pfade.einstellung_wahrheit('inj_auto', True)
                 else t('b_inj_hand'))
    if lage['quelle']:
        teile.append('%s %s' % (lage['quelle'], lage['stand'] or ''))
    return ' · '.join(x for x in teile if x)


def bauen(version='', wurzel=None, fehleranzahl=8):
    """Den Bericht als Text zusammensetzen."""
    zeilen = []

    def zeile(bez, wert):
        zeilen.append('%-18s%s' % (bez, pfade.kuerzen(wert)))

    zeilen.append(t('b_kopf')
                  % (version or '—', datetime.now().strftime(t('b_datum'))))
    zeilen.append('')

    # ⭐ Wer meldet das? Steht bewusst ganz oben — mit vielen Nutzern ist ein
    # Bericht ohne Absender kaum zuzuordnen, und Rückfragen laufen ins Leere.
    # **Freiwillig**: Ist nichts eingetragen, steht hier „nicht angegeben"; der
    # Watcher füllt das Feld nie von selbst.
    melder = (pfade.einstellung('melder_name') or '').strip()
    # ⚠ **Ohne `kuerzen()`.** Jede andere Zeile läuft durch die Anonymisierung,
    # die Benutzernamen durch `<benutzer>` ersetzt — und genau das traf den
    # Melder-Namen, wenn er dem Systemkonto gleicht („Xharig"). Ausgerechnet
    # die einzige Angabe, die der Nutzer BEWUSST macht, verschwand dadurch.
    # Aufgefallen am 29.08.2026 auf einem Bildschirmfoto, nicht im Test.
    zeilen.append('%-18s%s' % (t('b_melder'), melder or t('s_melder_leer')))
    zeilen.append('')

    uebersicht = _sicher(pfade.uebersicht, {})
    if not isinstance(uebersicht, dict):
        uebersicht = {}

    zeile(t('b_system'), _sicher(_system))
    zeile(t('b_verpackung'), _sicher(_verpackung_lesbar))
    zeile(t('b_python'), '%s / %s' % (platform.python_version(),
                                      _sicher(_tk_fassung)))
    zeile(t('b_bildschirm'), _sicher(lambda: _bildschirme(wurzel)))
    zeilen.append('')

    zeile(t('b_spiel'), _sicher(lambda: uebersicht.get('spiel_ordner')
                                 or t('b_nicht_gefunden')))
    zeile(t('b_gamelog'), _sicher(lambda: uebersicht.get('game_log')
                                   or t('b_nicht_gefunden')))
    zeile(t('b_sicherungen'), _sicher(_protokollzeile))
    zeile(t('b_launcher'), _sicher(lambda: uebersicht.get('launcher')
                                    or t('b_nicht_da')))
    # ⚠ **Womit sich das Spiel starten ließe — und ob das jemand von Hand
    # eingetragen hat.** Ohne diese Zeile ist „der Startknopf tut nichts" nicht
    # zu beantworten, ohne den Nutzer auszufragen. Siehe die Regel: Was einen
    # Fehler erklären würde, gehört in den Bericht, bevor er das nächste Mal
    # gemeldet wird.
    zeile(t('b_starter'), _sicher(_spielstarter))
    # ⚠ `sammeln()` gibt ein **Tupel** zurück — (phrasen, herkunft). Hier stand
    # `', '.join(sammeln())`, was eine Liste mit einem String zusammenfügen
    # wollte und mit einem TypeError abbrach. `_sicher()` verschluckte den, und
    # im Bericht stand nur ein Strich. Drei Übergaben lang galt das als
    # ungeklärter Punkt; in Wahrheit war es diese eine fehlende `[0]`.
    #
    # Die Herkunft wird gleich mit ausgegeben: Sie sagt, ob die Formulierung aus
    # der echten `global.ini` des Spielers stammt oder nur aus unserer Tabelle
    # geraten ist — genau die Auskunft, die man bei „er erkennt meine Baupläne
    # nicht" als Erstes braucht.
    zeile(t('b_spielsprache'), _sicher(_spielsprache))
    zeile(t('b_inj'), _sicher(_injektionslage))
    zeile(t('b_inj_datei'), _sicher(
        lambda: __import__('scbp.injektion', fromlist=['ini_datei'])
        .ini_datei()[0] or t('b_inj_keine')))
    zeilen.append('')

    zeile(t('b_bestand'), _sicher(_bestandzeile))
    _unbekannt = _sicher(_unbekannte_bauplaene, '')
    if _unbekannt and _unbekannt != '—':
        zeile(t('b_unbekannt'), _unbekannt)
    zeile(t('b_merkliste'), _sicher(lambda: t('b_n_eintraege') % _json_groesse(
        __import__('scbp.merkliste', fromlist=['pfad']).pfad(), 'eintraege')))
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
    zeile(t('b_katalog'), _sicher(lambda: (__import__(
        'scbp.katalog', fromlist=['laden']).laden().get('version') or None)))
    zeile(t('b_historie'), _sicher(_patchhistorie))

    # ⚠ Die drei Werkstatt-Seiten (ab v3.3.0). Ohne sie liesse sich eine
    # Meldung wie „bei mir bleibt die Herstellung leer" nicht beurteilen —
    # man saehe nicht, ob die Daten ueberhaupt geladen sind.
    def _lagerzeile():
        from . import rohstoffe
        posten = rohstoffe.laden()
        arten = {(p_.get('material') or '').strip().lower() for p_ in posten}
        return t('b_n_posten') % (len(posten), len(arten - {''}))

    def _rezeptzeile():
        from . import herstellung
        stand = herstellung.stand()
        if not stand:
            return t('b_nicht_geladen')
        return t('b_n_bauplaene_kurz') % (len(herstellung.alle()), stand)

    def _bergbauzeile():
        from . import bergbau
        stand = bergbau.stand()
        if not stand:
            return t('b_nicht_geladen')
        return t('b_n_orte') % (len(bergbau.orte()), stand)

    zeile(t('b_lager'), _sicher(_lagerzeile))
    zeile(t('b_rezepte'), _sicher(_rezeptzeile))
    zeile(t('b_bergbaudaten'), _sicher(_bergbauzeile))
    zeilen.append('')

    zeile(t('b_ordner'), _sicher(lambda: uebersicht.get('app_ordner')))
    zeile(t('b_einstellungen'), _sicher(lambda: ', '.join(
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
    start, seiten = _sicher(fehler.spur_geteilt, ([], []))
    # ⚠ Erst zusammenfassen, dann die letzten zwölf nehmen — andersherum wäre
    # der Ausschnitt schon leergeräumt, bevor das Zusammenfassen greift.
    start = _gedraengt(start)
    if start:
        zeilen.append('')
        zeilen.append(t('b_spur'))
        for eintrag in start[-12:]:
            zeilen.append('  ' + eintrag)
    # Die Diagnose-Seite selbst gehört nicht in die Liste: Der Bericht entsteht,
    # **während** sie gebaut wird, und stünde sonst in jedem Bericht als letzte,
    # unfertige Zeile — es sähe jedes Mal so aus, als wäre genau dort Schluss.
    while seiten and 'Seite diagnose' in seiten[-1]:
        seiten.pop()
    seiten = _gedraengt(seiten)
    if seiten:
        zeilen.append('')
        zeilen.append(t('b_spur_seiten'))
        for eintrag in seiten[-12:]:
            zeilen.append('  ' + eintrag)

    # ⚠ Und danach der harte Abbruch, falls es einen gab. Er steht **vor** den
    # Fehlern, weil er der schwerere Befund ist: Ein Eintrag in der Fehlerliste
    # heißt, das Programm hat weitergelebt; hier war es mitten im Befehl weg.
    # Nur die erste Handvoll Zeilen — der volle Aufrufweg aller Fäden füllt
    # Seiten, und der Melder soll den Bericht noch verschicken können.
    absturz = _sicher(fehler.letzter_absturz, [])
    if absturz:
        zeilen.append('')
        zeilen.append(t('b_absturz'))
        for eintrag in absturz[:14]:
            zeilen.append('  ' + eintrag)
        if len(absturz) > 14:
            zeilen.append('  … (%d)' % (len(absturz) - 14))

    letzte = _sicher(lambda: fehler.letzte(fehleranzahl), [])
    gesamt = _sicher(fehler.anzahl, 0)
    zeilen.append('')
    if letzte:
        zeilen.append(t('b_fehler') % (len(letzte), gesamt))
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
        gebuendelt = []
        for e in letzte:
            kennung = (e.get('fassung'), e.get('stelle'), e.get('art'),
                       e.get('meldung'))
            if gebuendelt and gebuendelt[-1][0] == kennung:
                gebuendelt[-1][2] += 1
                gebuendelt[-1][3] = e.get('zeit', '—')
            else:
                gebuendelt.append([kennung, e, 1, e.get('zeit', '—')])

        for _kennung, e, wieoft, bis in gebuendelt:
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
            fassung = e.get('fassung') or '?'
            alt_marke = ''
            if version and fassung not in ('?', '') and fassung != version:
                alt_marke = '  ' + t('b_fehler_alt')
            zeilen.append('  %s  %-10s %-24s %s: %s%s'
                          % (e.get('zeit', '—'), fassung, e.get('stelle', '—'),
                             e.get('art', '—'), e.get('meldung', '—'), alt_marke))
            if wieoft > 1:
                zeilen.append(t('b_fehler_mehrfach') % (wieoft, bis))
    else:
        zeilen.append(t('b_fehler_keine'))

    zeilen.append('')
    zeilen.append(t('b_fuss'))
    return '\n'.join(zeilen)


def absenden(text, version=''):
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
    from . import berichtziel
    ziel = berichtziel.ziel()
    if not berichtziel.moeglich():
        return False, t('m_bericht_kein_ziel')

    import urllib.request
    import uuid
    grenze = uuid.uuid4().hex
    name = 'bericht-%s.txt' % datetime.now().strftime('%Y-%m-%d-%H%M')
    kopf = ('**Fehlerbericht** · %s' % (version or '?'))[:1900]

    teile = []
    for feld, wert in (('content', kopf),):
        teile.append('--%s\r\nContent-Disposition: form-data; name="%s"\r\n\r\n%s\r\n'
                     % (grenze, feld, wert))
    teile.append('--%s\r\nContent-Disposition: form-data; name="files[0]"; '
                 'filename="%s"\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n%s\r\n'
                 % (grenze, name, text))
    teile.append('--%s--\r\n' % grenze)
    leib = ''.join(teile).encode('utf-8')

    try:
        anfrage = urllib.request.Request(
            ziel, data=leib, method='POST',
            headers={'Content-Type': 'multipart/form-data; boundary=%s' % grenze,
                     'User-Agent': 'SC-BP-Watcher'})
        with urllib.request.urlopen(anfrage, timeout=30) as antwort:
            if 200 <= antwort.status < 300:
                return True, ''
            return False, 'HTTP %s' % antwort.status
    except Exception as ausnahme:
        fehler.merken('bericht.absenden', ausnahme)
        # ⚠ Den Grund NICHT durchreichen: In der Fehlermeldung einer
        # fehlgeschlagenen Anfrage steht die Adresse, und die ist geheim.
        return False, t('m_bericht_weg')


def in_die_ablage(text, wurzel=None):
    """Den Bericht in die Zwischenablage legen. True, wenn es geklappt hat."""
    try:
        if wurzel is None:
            return False
        wurzel.clipboard_clear()
        wurzel.clipboard_append(text)
        wurzel.update()          # ohne das ist die Ablage nach dem Beenden leer
        return True
    except Exception:
        return False


# GitHub schneidet sehr lange Adressen ab. Der Bericht ist normalerweise gut
# 1 KB groß; die Grenze greift erst, wenn jemand mit 50 Fehlern im Gepäck meldet.
URL_GRENZE = 6000
ISSUE_ADRESSE = 'https://github.com/Xharig/SC-BP-Watcher/issues/new'


def _vorlage_zur_sprache():
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


def issue_adresse(text, titel='', vorlage=None):
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

    koerper = text or ''
    if len(koerper) > URL_GRENZE:
        koerper = koerper[:URL_GRENZE] + t('m_bericht_gekuerzt')

    werte = {'template': vorlage or _vorlage_zur_sprache(), 'bericht': koerper}
    if titel:
        werte['title'] = titel
    return ISSUE_ADRESSE + '?' + urlencode(werte)


def issue_oeffnen(text, titel=''):
    """Das vorausgefüllte Formular im Browser öffnen. True, wenn es startete.

    ⚠ Über `pfade.im_browser`, nicht über `webbrowser.open()` — im AppImage
    öffnet das nichts und meldet trotzdem Erfolg (Begründung dort).
    """
    try:
        return pfade.im_browser(issue_adresse(text, titel))
    except Exception:
        return False


def speichern(text, pfad_=None):
    """Den Bericht als Datei ablegen; gibt den Pfad zurück oder None."""
    try:
        pfad_ = pfad_ or pfade.app_datei('bericht.txt')
        with open(pfad_, 'w', encoding='utf-8') as f:
            f.write(text)
        return pfad_
    except Exception as ausnahme:
        try:
            from . import fehler
            fehler.merken('bericht.speichern', ausnahme)
        except Exception:
            pass
        return None


if __name__ == '__main__':
    sys.stdout.write(bauen(version='3.0.0-dev') + '\n')
