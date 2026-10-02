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
Das Verwaltungsfenster — der eigene Bauplan-Bestand zum Nachschlagen und Abhaken.

Die Melde-Leiste zeigt, was gerade hereinkommt. Dieses Fenster zeigt den Stand:
was es gibt, was man hat, was fehlt — und **woher man das Fehlende bekommt**.

Drei Dinge, für die es da ist:

  **Nachschlagen.** Ob ein Bauplan schon da ist, ohne im Spiel nachzusehen.
  **Nachtragen.** Was keine Log-Sicherung mehr hergibt, hakt man hier von Hand
  ab — das Gegenstück zum Lückenhinweis beim Start.
  **Finden.** Bei jedem fehlenden Bauplan steht, welche Fraktion ihn auslobt,
  in welchem Auftrag, ab welchem Rang und was er einbringt.

Bedienung: tippen filtert, Klick auf eine Zeile setzt oder entfernt das Häkchen,
Klick auf ⓘ klappt die Bezugsquellen aus.
"""
import time
import tkinter as tk

from . import errors
from . import collection as bestand_datei
from . import export as export_modul
from . import notice
from . import catalog as katalog_modul
from . import watchlist as merk
from . import icons
from . import paths
from . import fields
from .language import t, window_title
from . import theme

BG      = theme.BG
FLAECHE = theme.SURFACE
BAR     = theme.BAR
LINIE   = theme.LINE2   # Rand runder Kästen und Felder — überall dieselbe Linie
FG      = theme.FG
SUB     = theme.SUB
ACCENT  = theme.ACCENT
GELB    = theme.YELLOW

# Ab wie vielen Zeilen nur noch der Anfang gezeigt wird. 714 Zeilen einzeln zu
# bauen dauert in tkinter spürbar lange, und niemand scrollt durch 714 Zeilen —
# wer etwas sucht, tippt. Der Rest kommt auf Knopfdruck.
#
# ⚠ Der Wert ist **gemessen**, nicht geschätzt. Das erste Zeichnen wächst
# überproportional, weil Tk alle Widgets auf einmal darstellen muss (gemessen
# auf dem Mac, Tk 9.0, echter Katalog mit 722 Bauplänen):
#
#     10 Zeilen   0,45 s        60 Zeilen    5,05 s
#     30 Zeilen   1,14 s       120 Zeilen   30,36 s
#
# Bei 120 waren es 801 Widgets und eine halbe Minute, in der das Fenster steht.
# Jedes weitere Zeichnen danach dauert nur 0,4 s; teuer ist ausschließlich der
# erste Aufbau.
#
# 40 Zeilen sind gut zwei Bildschirmhöhen bei unter zwei Sekunden Wartezeit.
# Mehr gibt es über die Suche oder den Knopf „weitere anzeigen".
ZEILEN_ZUERST = 40

# Ab welcher Inhaltshöhe die Liste in Blöcken gezeigt werden muss.
#
# ⚠ Das ist keine Geschmacksfrage, sondern eine harte Grenze des Fenstersystems:
# X11 rechnet Fensterkoordinaten in **16 Bit**, es gibt also keine Position
# jenseits von 32767 Pixeln. Ein Frame in einer Leinwand, der höher wird, sitzt
# ab dort nicht mehr dort, wo Tk ihn hinrechnet — die Zeilen am Ende der Liste
# **überlappen einander**. Gemessen mit einem echten Katalog bei 125 % Anzeige-
# Skalierung: Inhalt 33452 px, davon 16 Elemente jenseits der Grenze — und genau
# die überlagerten sich.
#
# Ein Sicherheitsabstand bis 32000 px. Wie viele Zeilen das sind, hängt von
# Schriftgröße und Skalierung ab und wird gemessen, nicht geraten (siehe
# `_zeilen_deckel`). Wird es mehr, übernimmt der Blockmodus — abgeschnitten wird
# nichts, siehe Abschnitt „Lange Liste in Blöcken".
HOECHSTE_INHALTSHOEHE = 32000

# Wie viele Reihen in einen Block kommen, wenn die Liste in Blöcken gezeigt
# wird (siehe „Lange Liste in Blöcken"). 120 Reihen sind rund 5000 Pixel —
# klein genug, dass immer nur wenige Blöcke gleichzeitig gebaut sein müssen,
# und groß genug, dass beim Rollen nicht dauernd neu gebaut wird.
BLOCK_REIHEN = 120
# Die Programmversion wird vom Hauptprogramm gesetzt; sie landet im
# scmdb-Export als Kennung des erzeugenden Werkzeugs.
VERSION = ['']


def schrift(groesse, fett=False, unterstrichen=False):
    """Die Schrift dieses Fensters.

    `unterstrichen` ist für Textlinks — im Haus die Auszeichnung dafür, dass
    man klicken kann. Ohne sie sieht ein Textlink aus wie ein Hinweis.
    """
    fam = 'Segoe UI' if paths.WINDOWS else 'Helvetica'
    stil = ' '.join(x for x, an in (('bold', fett),
                                    ('underline', unterstrichen)) if an)
    return (fam, groesse, stil or 'normal')


def mono(groesse):
    return ('Consolas' if paths.WINDOWS else 'Menlo', groesse)


def kuerzel(eintrag):
    """Klasse/Größe/Grad als „M/1/A" — leer, wo es nichts zu zeigen gibt.

    ⚠ Die Reihenfolge ist **Klasse, Größe, Grad**, nicht Klasse, Grad, Größe.
    So liest es sich wie im Spiel (Size 1, Grade A), und die Größe ist beim
    Suchen das Wichtigere: Ein Cooler der falschen Größe passt gar nicht,
    einer mit anderem Grad passt schlechter.
    """
    klasse, grad, groesse = eintrag.get('c'), eintrag.get('g'), eintrag.get('s')
    if not (klasse or grad or groesse):
        return ''
    buchstabe = KLASSE_BUCHSTABE.get(klasse, '–')
    grad_b = GRAD_BUCHSTABE.get(grad, '–').upper()
    return '%s/%s/%s' % (buchstabe, groesse if groesse else '–', grad_b)


def quelle_text(q):
    """Eine Bezugsquelle in einem Satz."""
    teile = []
    if q.get('fraktion'):
        teile.append(q['fraktion'])
    if q.get('typ'):
        teile.append(q['typ'])
    kopf = ' · '.join(teile)
    unten = []
    if q.get('rang'):
        unten.append(t('ab_rang', q['rang'])
                     + (' ' + t('ruf_punkte', f"{q['rep']:,}".replace(',', '.'))
                        if q.get('rep') else ''))
    if q.get('uec'):
        unten.append('%s aUEC' % f"{q['uec']:,}".replace(',', '.'))
    if q.get('ruf'):
        unten.append(t('ruf_gewinn', q['ruf']))
    return kopf, (q.get('auftrag') or ''), ' · '.join(unten), ort_text(q.get('wo'))


def ort_text(wo):
    """Wo der Auftrag angenommen wird, etwa `Stanton: Hurston, Crusader, …`.

    Ergänzt zur Herkunft (*woher* ein Bauplan kommt) den Ort (*wo* man den
    Auftrag findet), damit man den Missionsnamen nicht anderswo nachschlagen
    muss."""
    if not wo:
        return ''
    orte = ', '.join(wo.get('orte') or [])
    if wo.get('mehr'):
        orte += t('und_weitere', wo['mehr'])
    system = wo.get('system')
    if system and orte:
        return '%s %s: %s' % (t('annehmen_in'), system, orte)
    return '%s %s' % (t('annehmen_in'), system or orte)


# Was die Suche außer dem Namen noch durchsucht: die Klasse (military,
# civilian, stealth …), die in jeder Zeile steht, dazu Hersteller und
# Gütegrad.
#
# Tatsächlich vorhandene Klassen (gemessen am Katalog 4.9.0): Civilian 72,
# Energy 45, Military 38, Ballistic 30, Industrial 25, Stealth 22, Electron 6,
# Laser 2. „Competition" kommt in den Daten nicht vor — es steht trotzdem in
# der Kürzel-Tabelle des Overlays, schadet aber nicht.
#
# Der Gütegrad steht als **Zahl** (1–4), angezeigt wird ein Buchstabe. Wer
# „Grade A" sucht, tippt den Buchstaben — also muss hier umgerechnet werden,
# sonst findet die Suche nie etwas.
GRAD_BUCHSTABE = {1: 'a', 2: 'b', 3: 'c', 4: 'd'}

# Die Klassen, wie das Spiel sie kennt — mit ihrem Kürzel in der Zeile.
KLASSE_BUCHSTABE = {'Military': 'M', 'Stealth': 'S', 'Industrial': 'I',
                    'Civilian': 'C', 'Competition': 'K'}

# ⚠ Klassen, Größen und Grade stehen hier **fest**, nicht aus dem Katalog
# abgeleitet. Grund: Was gerade kein Bauplan hat, fehlte sonst in der Auswahl —
# etwa „Competition" (kommt im Katalog 4.9.0 nicht vor), die Größen 4 bis 6
# oder die Grade B bis D. Ein Auswahlfeld, dessen Inhalt sich mit
# jedem Spiel-Patch ändert, ist keins: Man sucht etwas und findet den Eintrag
# nicht, ohne zu erfahren warum. Was der Katalog darüber hinaus hergibt, wird
# unten trotzdem ergänzt — verlieren soll man nichts.
KLASSEN_FEST = ('Military', 'Stealth', 'Industrial', 'Civilian', 'Competition')
GROESSEN_FEST = ('1', '2', '3', '4', '5', '6')
GRADE_FEST = ('1', '2', '3', '4')


def _passt(eintrag, text):
    """Trifft der Suchbegriff diesen Bauplan — Name, Klasse, Hersteller, Grad?"""
    if text in eintrag['n'].lower():
        return True
    klasse = (eintrag.get('c') or '').lower()
    if klasse and text in klasse:
        return True
    hersteller = (eintrag.get('m') or '').lower()
    if hersteller and text in hersteller:
        return True
    # `grade a` und `size 2` — so, wie es in der Zeile steht
    grad = GRAD_BUCHSTABE.get(eintrag.get('g'))
    if grad and text in ('grade %s' % grad, 'grad %s' % grad, grad):
        return True
    if eintrag.get('s') and text in ('size %s' % eintrag['s']).lower():
        return True
    # ⭐ **Auch nach dem Auftrag suchen.** „Retake" fand bis rc21 nichts,
    # obwohl sechs Baupläne aus Aufträgen mit diesem Wort stammen. Wer einen
    # Auftrag fliegt, will wissen, was dabei herausspringt — und wer einen
    # Bauplan sucht, sucht oft über den Auftrag, aus dem er kommt.
    for q in eintrag.get('q') or []:
        for feld in ('auftrag', 'fraktion', 'typ', 'wo'):
            wert = q.get(feld)
            # ⚠ `wo` ist **kein Text**, sondern ein Objekt (Ort samt System).
            # Ohne diese Prüfung stürzt die Suche bei jedem Tastendruck ab —
            # und weil das im Zeichnen passiert, hängt das Fenster.
            if isinstance(wert, str) and wert and text in wert.lower():
                return True
    return False


def auftraege_zu(text, katalog):
    """Welche Aufträge passen zum Suchbegriff — und wie viele Baupläne je Auftrag.

    Gibt Paare `(Auftragsname, Anzahl)` zurück, die häufigsten zuerst. Damit
    beantwortet die Liste die Frage hinter der Suche: „Was gibt es in dieser
    Quest überhaupt?"
    """
    if not text:
        return []
    zaehler = {}
    for e in (katalog.get('bauplaene') or {}).values():
        gesehen = set()
        for q in e.get('q') or []:
            name = (q.get('auftrag') or '').strip()
            if not name or name in gesehen:
                continue
            if text in name.lower():
                gesehen.add(name)
                zaehler[name] = zaehler.get(name, 0) + 1
    return sorted(zaehler.items(), key=lambda p: (-p[1], p[0].lower()))


def _dauer_text(minuten):
    """Minuten lesbar machen — 15 Min, 2 Std 30 Min, 7 Tagen.

    ⚠ Die Werte reichen von 1 bis 10.080 (eine Woche). „10080 Min" liest
    niemand; deshalb ab 24 Stunden in Tagen.
    """
    minuten = int(minuten)
    if minuten < 60:
        return t('zeit_min') % minuten
    if minuten < 24 * 60:
        std, rest = divmod(minuten, 60)
        return (t('zeit_std') % std if not rest
                else t('zeit_std_min') % (std, rest))
    tage = round(minuten / (24 * 60))
    return (t('zeit_tag') if tage == 1 else t('zeit_tage')) % tage


class Bestandsfenster:
    """Eigenständiges Fenster. Wird von der Melde-Leiste aus geöffnet."""

    def __init__(self, eltern=None, beim_schliessen=None, rahmen=None,
                 hauptfenster=None):
        """Ohne `rahmen` ein eigenes Fenster, mit `rahmen` eine Seite im Hauptfenster.

        Der eigenständige Modus lässt sich einzeln starten und prüfen.

        ⚠⚠ **`hauptfenster` gehört hier hinein und NICHT nachträglich
        zugewiesen.** Der Rückweg zum Hauptfenster bestimmt, ob ein
        Bauplan-Name anklickbar ist (Sprung zu den Zutaten) — und das wird
        beim **ersten** Zeichnen festgelegt, das noch im Konstruktor
        passiert. Wer ihn erst danach setzt, bekommt eine Liste, in der nichts
        geht, bis sie ein zweites Mal gezeichnet wird.
        """
        self.hauptfenster = hauptfenster
        self.beim_schliessen = beim_schliessen
        self.eingebettet = rahmen is not None
        if self.eingebettet:
            self.root = rahmen
            self.root.configure(bg=BG)
        else:
            self.root = tk.Toplevel(eltern) if eltern else tk.Tk()
            self.root.title(window_title(t('titel_bauplaene')))
            self.root.configure(bg=BG)
            # ⚠⚠ **Mit Position, nicht nur mit Größe.** Ein `geometry` ohne
            # `+x+y` überlässt die Platzierung dem Fenstermanager — und der
            # weiß nichts vom Hauptfenster. Auf mehreren Bildschirmen kann das
            # Fenster so außerhalb des sichtbaren Bereichs landen; weil es
            # modal ist, ließe sich das Programm dann nicht einmal beenden.
            #
            # `center_over` setzt beides und fällt auf die reine Größe
            # zurück, wenn es kein Elternfenster gibt (eigenständiger Start).
            from .main_window import center_over
            if eltern is None or not center_over(self.root, eltern, 720, 780):
                self.root.geometry('720x780')

        # ⚠⚠ **Messpunkte für den Bericht.** Zwischen den Berichtszeilen
        # `Seite liste: bauen beginnt` und `Liste: zeichnen beginnt` liegt
        # alles hier — Bestand lesen, Katalog stempeln, Katalog laden, Kopf
        # und Werkzeugleiste bauen. Die Zeiten hängen stark von der Größe des
        # Bestands ab, deshalb wird jeder Schritt einzeln gemessen.
        _t_bau = time.perf_counter()
        self.bestand = bestand_datei.load()
        _ms_bestand = (time.perf_counter() - _t_bau) * 1000
        # ⚠ Erst stempeln, dann laden. Der Netz-Takt (`catalog.update()`)
        # läuft irgendwann nach dem Start in einem eigenen Faden; ist das
        # Fenster vorher gebaut, hält die Liste den ungestempelten Stand fest
        # und zeigt zu wenige Zeilen, bis sie neu geöffnet wird.
        #
        # Das trifft jeden ersten Start nach einer Fassung, die neue Historie
        # mitbringt. Hier kostet es nichts: gelesen wird ohnehin, geschrieben
        # nur, wenn sich wirklich etwas ändert.
        _t_stempel = time.perf_counter()
        katalog_modul.refresh_stamp()
        _ms_stempel = (time.perf_counter() - _t_stempel) * 1000
        _t_kat = time.perf_counter()
        self.katalog = katalog_modul.load()
        _ms_katalog = (time.perf_counter() - _t_kat) * 1000
        errors.trail('Liste: Daten gelesen (Bestand %d ms, Stempel %d ms, '
                    'Katalog %d ms, %d Bauplaene)'
                    % (round(_ms_bestand), round(_ms_stempel),
                       round(_ms_katalog),
                       len(self.katalog.get('bauplaene') or {})))
        self.filter = 'alle'
        self.suche = tk.StringVar()
        from .main_window import after_typing
        self.suche.trace_add('write', after_typing(
            self.root, lambda: self._zeichnen(nach_oben=True)))
        self.offen = set()          # Namen, deren Herkunft ausgeklappt ist
        self.alle_zeigen = False
        self.bereiche_aus = set()   # ausgeblendete Bereiche (Schiff, FPS, …)

        # ⚠ Messpunkt für die vier folgenden Bauschritte — ohne ihn fehlt im
        # Bericht die Zeit zwischen Daten und Zeichnen.
        _t_rahmen = time.perf_counter()
        self._kopf()
        _ms_kopf = (time.perf_counter() - _t_rahmen) * 1000
        _t_wz = time.perf_counter()
        self._werkzeugleiste()
        self._grenze_zeigen()
        _ms_wz = (time.perf_counter() - _t_wz) * 1000
        # ⚠ Reihenfolge: erst der feste Block unten, dann die rollende Liste.
        # Wer die Liste zuerst packt, schiebt den Block aus dem Fenster.
        _t_hk = time.perf_counter()
        self._herkunftsbereich()
        _ms_hk = (time.perf_counter() - _t_hk) * 1000
        _t_li = time.perf_counter()
        self._liste()
        _ms_li = (time.perf_counter() - _t_li) * 1000
        errors.trail('Liste: Rahmen gebaut (Kopf %d ms, Leiste %d ms, '
                    'Herkunft %d ms, Rollflaeche %d ms)'
                    % (round(_ms_kopf), round(_ms_wz), round(_ms_hk),
                       round(_ms_li)))
        self._zeichnen()
        # ⚠ Der Schlussstrich unter den ganzen Aufbau: Diese Zahl ist die, die
        # der Nutzer als Wartezeit erlebt. Alles davor sind Teilstücke.
        errors.trail('Liste: Fenster fertig (%d ms gesamt)'
                    % round((time.perf_counter() - _t_bau) * 1000))

    # ------------------------------------------------------------------ Aufbau
    def _kopf(self):
        bar = tk.Frame(self.root, bg=BAR)
        bar.pack(fill='x')
        tk.Label(bar, text=t('bauplaene'), bg=BAR, fg=FG,
                 font=schrift(12, True)).pack(side='left', padx=14, pady=10)
        self.fortschritt = tk.Label(bar, text='', bg=BAR, fg=SUB, font=schrift(10))
        self.fortschritt.pack(side='left')
        # Export rechts in der Kopfzeile — dort, wo man ihn sucht, wenn man die
        # Liste vor sich hat.
        # ⚠⚠ **Der Knopf springt zur Seite „Sichern & Zurücksetzen", Abschnitt
        # „Bestand ausgeben".** Dort stehen alle Formate mit Namen
        # nebeneinander, die Ablage gleich darunter — ein zweiter Weg hier
        # wäre doppelt.
        # Im eigenständigen Fenster (ohne Hauptfenster) gibt es keine Seite
        # zum Hinspringen; dort öffnet sich der Speichern-Dialog.
        from .main_window import round_button
        k = round_button(bar, t('export_einzeln'), None, schrift(9), BAR,
                         FLAECHE, LINIE, FG)
        k.pack(side='right', padx=(0, 14))
        k.bind('<Button-1>', lambda e: self._zum_export())
        notice.attach(k, lambda: t('hinweis_export'))
        self.export_meldung = tk.Label(bar, text='', bg=BAR, fg=ACCENT,
                                       font=schrift(9))
        self.export_meldung.pack(side='right', padx=(0, 10))
        # Kein eigenes ✕: Dieses Fenster hat eine ganz normale Titelleiste vom
        # System, und die hat bereits eins. Zwei Kreuze übereinander sehen aus
        # wie ein Fehler — und man rät, welches was tut. (Betraf Windows genauso,
        # die Leiste kommt dort ebenfalls vom Fenstermanager.) Das randlose
        # Overlay ist der andere Fall: Dort gibt es keine Systemleiste, deshalb
        # behält es sein eigenes ✕.

    def _zum_export(self):
        """Zu „Sichern & Zurücksetzen" springen, auf „Bestand ausgeben".

        Ohne Hauptfenster (eigenständiger Start) bleibt der Speichern-Dialog."""
        try:
            main = getattr(self, 'hauptfenster', None)
            if main is None:
                self._exportieren('voll')
                return
            from . import pages
            pages.show_export(main)
        except Exception as exception:
            errors.record('collection_window.zum_export', exception)

    def _exportieren(self, art):
        """Bestand als Datei ausgeben — Ziel wählt der Spieler."""
        # ⚠ Siehe `file_picker` — der Systemdialog statt des Tk-Kastens.
        from . import file_picker
        pfad = file_picker.save_file(
            t('export_basetool' if art == 'basetool' else 'export_alles'),
            suggestion=export_modul.suggestion(art), extension='.json',
            start=export_modul.archive_folder(),
            patterns=(('JSON', '*.json'), (t('alle_dateien'), '*.*')))
        if not pfad:
            return
        ok, meldung = export_modul.write(pfad, art, self.bestand,
                                         self.katalog)
        self.export_meldung.configure(
            text=t('export_fertig', meldung) if ok else t('export_fehler', meldung),
            fg=ACCENT if ok else GELB)
        # Nach ein paar Sekunden wieder wegnehmen — eine Erfolgsmeldung, die
        # stehen bleibt, wird zur Beschriftung und sagt dann nichts mehr.
        self.root.after(6000, lambda: self.export_meldung.configure(text=''))

    def _grenze_zeigen(self):
        """Sagen, was das Werkzeug NICHT wissen kann.

        ⚠⚠ Der Watcher kennt nur, was seit seiner Installation im Protokoll
        stand — und Star Citizen loescht seine alten Protokolle laufend weg.
        Wer vorher gespielt hat, hat Bauplaene, von denen das Werkzeug nichts
        weiss.

        ⚠ Das ist **kein Fehler und laesst sich nicht beheben**: Gemessen an
        194 Protokollen ist jede Bauplan-Meldung, die darin stand, auch im
        Bestand gelandet. Die Luecke stammt aus der Zeit davor. Und der
        Fabricator hilft nicht weiter: Das Spiel schreibt seine Liste
        nirgends ins Protokoll, nur die Verbindung zum Dienst.

        Also bleibt der Abgleich von Hand — und wer das nicht weiss, haelt
        eine unvollstaendige Liste fuer vollstaendig.

        ⚠ Steht **unter** der Werkzeugleiste, nicht als Kasten oben: Es ist
        eine einmalige Auskunft, keine Warnung. Wer sie gelesen hat, soll
        nicht jedes Mal daran vorbeischauen muessen.
        """
        zeile = tk.Label(self.root, text=t('bp_grenze'), bg=BG, fg=SUB,
                         font=schrift(9), anchor='w', justify='left')
        zeile.pack(fill='x', padx=14, pady=(0, 6))

        # ⚠⚠ **Der Satz muss umbrechen, sonst schiebt er das Fenster breiter.**
        # Auf Englisch bei 1100 px Breite sind es +8 px (Randprüfung), dort
        # ist der Text länger. Ein `wraplength`, das mit
        # der Fensterbreite mitzieht, statt einer festen Zahl: Wer das Fenster
        # schmaler zieht, bekäme sonst denselben Fehler zurück.
        def _umbrechen(_ereignis=None):
            try:
                breite = self.root.winfo_width() - 2 * 14 - 8
                if breite > 80:
                    zeile.configure(wraplength=breite)
            except tk.TclError:
                pass

        self.root.bind('<Configure>', _umbrechen, add='+')
        self.root.after(0, _umbrechen)

    def _werkzeugleiste(self):
        leiste = tk.Frame(self.root, bg=BG)
        leiste.pack(fill='x', padx=14, pady=(12, 4))

        # Suchfeld mit Löschkreuz: Das ✕ liegt im selben Kasten wie das Feld,
        # damit es dazugehörig aussieht und nicht wie ein weiterer Knopf.
        from .main_window import round_frame
        kasten = round_frame(leiste, FLAECHE, LINIE, radius=8, base_color=BG)
        kasten.holder.pack(side='left', fill='x', expand=True, padx=(0, 10))
        feld = tk.Entry(kasten, textvariable=self.suche, bg=FLAECHE, fg=FG,
                        insertbackground=FG, relief='flat', bd=0,
                        highlightthickness=0, font=schrift(11))
        feld.pack(side='left', fill='x', expand=True, ipady=6, padx=(8, 0))
        feld.focus_set()
        self._platzhalter(feld)
        self.loeschen_lbl = icons.line(kasten, 'schliessen', background=FLAECHE,
                                          font=schrift(10))
        self.loeschen_lbl.configure(padx=8, cursor='hand2')
        self.loeschen_lbl.bind('<Button-1>', lambda e: self._suche_leeren())
        notice.attach(self.loeschen_lbl, lambda: t('hinweis_suche_leeren'))
        # Erscheint erst, wenn etwas drinsteht — ein ✕ an einem leeren Feld ist
        # nur ein Zeichen, das nichts tut.
        self._loeschkreuz_zeigen()

        # ⚠ Eigene Zeile für die Zähler-Knöpfe. In der Zeile des Suchfelds,
        # das `expand=True` hat, schneidet Tk beim Schmalerziehen den letzten
        # Knopf ab (`⭐ be…` statt `⭐ beobachtet`).
        # ⚠ Ein Halter um die Zeile. Die Knöpfe selbst werden per `grid`
        # angeordnet (Umbruch), und Tk verträgt `grid` und `pack` nicht im
        # selben Elternteil — der Zurücksetzen-Knopf rechts wird aber gepackt.
        knopfhalter = tk.Frame(self.root, bg=BG)
        knopfhalter.pack(fill='x', padx=14, pady=(0, 6))
        knopfzeile = tk.Frame(knopfhalter, bg=BG)
        knopfzeile.pack(side='left', fill='x', expand=True)
        self.knopfhalter = knopfhalter

        self.knoepfe = {}
        for schluessel, text in (('alle', t('filter_alle')),
                                 ('habe', t('filter_habe')),
                                 ('fehlt', t('filter_fehlt')),
                                 ('merk', t('filter_merk')),
                                 ('neu', t('filter_neu')),
                                 ('deckel', t('filter_deckel'))):
            from .main_window import round_button
            k = round_button(knopfzeile, text, None, schrift(10), BG, FLAECHE,
                          LINIE, SUB)
            k.bind('<Button-1>', lambda e, s=schluessel: self._filter_setzen(s))
            self.knoepfe[schluessel] = k
        # Auch diese Reihe bricht um, falls selbst die eigene Zeile nicht reicht.
        self._reihe_umbrechen(knopfzeile, list(self.knoepfe.values()))

        self._feinfilter()

    def _feinfilter(self):
        """Fünf Auswahlfelder: Art, Klasse, Größe, Quelle, Gütegrad.

        ⚠ Ausgewählt wird, was man **sehen** will — nicht, was ausgeblendet
        wird. Ausblenden hieße, für einen Bereich alle anderen wegzuklicken.

        Die Einträge kommen aus dem Katalog, nicht aus einer festen Liste: Was
        es im Spiel nicht gibt, steht auch nicht zur Wahl.
        """
        from .main_window import round_select

        reihe = tk.Frame(self.root, bg=BG)
        reihe.pack(fill='x', padx=14, pady=(0, 8))
        self.fein = {'art': '', 'unterart': '', 'klasse': '', 'groesse': '',
                     'quelle': '', 'grad': '', 'patch': ''}
        # Ein angeklickter Auftrag — dann zeigt die Liste nur, was aus ihm
        # stammt. Kein Auswahlfeld: Aufträge gibt es hunderte, sie kommen
        # über die Suche und werden dort angeklickt.
        self.auftrag = ''
        # ⚠ Die zu `auftrag` aufgelösten Bauplan-Schlüssel, oder None. Gesetzt
        # von `zum_auftrag()`, weil ein Titel aus dem Spiel sich nicht
        # wörtlich mit den Herkunftsdaten vergleichen lässt (Platzhalter).
        self.auftrag_bp = None
        # Eine angeklickte Katalog-Art (Cooler, Schild, Helm) — kommt aus
        # dem Bauplan-Fortschritt, wo jede Kategorie eine eigene Zeile hat.
        #
        # ⚠ Bewusst NICHT `fein['art']`: Das sind die Oberkategorien aus
        # `categories.classify`, der Fortschritt zählt dagegen nach
        # `catalog.kind_readable`. Zwei Zuordnungen, die sich ähneln, aber nicht
        # deckungsgleich sind — vermischt bleibt die Liste leer, während im
        # Feld eine Zahl steht. Wer aus dem
        # Fortschritt springt, muss exakt die Baupläne sehen, die dort gezählt
        # wurden.
        self.katalog_art = ''

        # ⚠ Eigener Rahmen für die Auswahlfelder. Sie werden per `grid`
        # angeordnet (damit sie umbrechen können), und Tk verträgt `grid` und
        # `pack` nicht im selben Elternteil — daneben liegen aber der
        # Trefferzähler und `zurücksetzen`, die gepackt sind.
        self.fein_rahmen = tk.Frame(reihe, bg=BG)
        self.fein_rahmen.pack(side='left', fill='x', expand=True)


        self._feinfilter_felder()

        # ⚠ Ein Knopf, kein unterstrichener Kleintext — als graue
        # 9-Punkt-Zeile wird er übersehen.
        # ⚠ Er sitzt **oben in der Zustandszeile**, ganz rechts und mit
        # Abstand — nicht an den Neu-im-Spiel-Knopf geklebt.
        self.zuruecksetzen_lbl = tk.Label(
            self.knopfhalter, text='\u00d7  ' + t('ff_zuruecksetzen'),
            bg=FLAECHE, fg=ACCENT, font=schrift(10), cursor='hand2',
            padx=12, pady=4,
            highlightthickness=1, highlightbackground=ACCENT,
            highlightcolor=ACCENT)
        self.zuruecksetzen_lbl.bind('<Button-1>', lambda e: self._alles_leeren())
        self.zuruecksetzen_lbl.bind(
            '<Enter>', lambda e: self.zuruecksetzen_lbl.configure(bg=theme.ACCENT_DARK))
        self.zuruecksetzen_lbl.bind(
            '<Leave>', lambda e: self.zuruecksetzen_lbl.configure(bg=FLAECHE))

        self.treffer_lbl = tk.Label(reihe, text='', bg=BG, fg=SUB,
                                    font=schrift(9))
        self.treffer_lbl.pack(side='right')
        # (Das Anordnen macht `_feinfilter_felder()` selbst.)

    def _reihe_umbrechen(self, rahmen, elemente, rechts_frei=None):
        """Eine Reihe von Bedienelementen umbrechen lassen, wenn es eng wird.

        ⚠ Tk bricht nicht um, es schneidet ab: In beiden Reihen der
        Bauplan-Liste stünde sonst das fünfte Auswahlfeld nur halb da, und bei
        Mindestbreite blieben von vier Zähler-Knöpfen zwei übrig. Im Entwurf
        macht das `flex-wrap: wrap`; hier ist es von Hand nachgebaut.

        `rechts_frei` ist ein Widget, für das rechts Platz bleiben soll (der
        Trefferzähler).

        ⚠ Angeordnet wird per `grid`, nicht per `pack` mit Zwischenrahmen. Den
        Elternteil eines Tk-Widgets kann man nicht nachträglich umsetzen
        (`feld.master = …`) — die Auswahlfelder verschwänden dann
        vollständig aus dem Fenster. Mit `grid` bleibt jedes Element, wo es
        gebaut wurde, und wechselt nur Zeile und Spalte.
        """
        def ordnen(_=None):
            # ⚠ Tote Elemente überspringen. Die Auswahlfelder werden neu
            # gebaut, sobald die Oberkategorie wechselt — die alte Bindung
            # hängt aber weiter am Rahmen und greift dann auf zerstörte
            # Leinwände zu: `TclError: bad window path name … !canvas14`.
            # Bricht `ordnen()` daran mittendrin ab, bleiben die Felder
            # ungesetzt und die Liste zeichnet nichts mehr (0 Treffer bei
            # gewählter Kategorie).
            try:
                if not rahmen.winfo_exists():
                    return
                platz = rahmen.winfo_width()
            except tk.TclError:
                return
            lebende = []
            for element in elemente:
                try:
                    if element.winfo_exists():
                        lebende.append(element)
                except tk.TclError:
                    pass
            if not lebende:
                return
            if platz <= 1:
                return
            if rechts_frei is not None:
                try:
                    platz -= rechts_frei.winfo_reqwidth() + 12
                except tk.TclError:
                    pass
            plaetze, zeile, spalte, breite = [], 0, 0, 0
            for element in lebende:
                try:
                    braucht = element.winfo_reqwidth() + 6
                except tk.TclError:
                    continue
                if spalte and breite + braucht > platz:
                    zeile, spalte, breite = zeile + 1, 0, 0
                plaetze.append((element, zeile, spalte))
                spalte += 1
                breite += braucht
            if plaetze == getattr(rahmen, 'zuletzt', None):
                return                  # unverändert — nicht neu setzen
            rahmen.zuletzt = plaetze
            for element, z, s in plaetze:
                try:
                    element.grid(row=z, column=s, sticky='w',
                                 padx=(0, 6), pady=(0 if z == 0 else 4, 0))
                except tk.TclError:
                    pass

        rahmen.bind('<Configure>', ordnen, add='+')
        rahmen.after_idle(ordnen)

    # --- Woraus die Auswahlfelder ihre Einträge nehmen ---
    def _kat_werte(self, feld):
        """Alle im Katalog vorkommenden Werte eines Feldes, alphabetisch."""
        werte = set()
        for e in (self.katalog.get('bauplaene') or {}).values():
            wert = e.get(feld)
            if wert:
                werte.add(wert)
        return sorted(werte, key=lambda x: str(x).lower())

    def _kategorie(self, eintrag):
        """Ober- und Unterkategorie eines Bauplans — `(ober, unter)`.

        ⚠ Einmal je Bauplan berechnet und gemerkt. Die Zuordnung fragt die
        Rezeptdaten (2 MB) nach dem Tag; das bei jedem Filterklick für 738
        Baupläne zu tun wäre dieselbe Falle wie beim Qualitätsregler.
        """
        name = eintrag.get('n') or ''
        merker = getattr(self, '_kat_merker', None)
        if merker is None:
            merker = self._kat_merker = {}
        if name in merker:
            return merker[name]
        try:
            from . import categories as kat_modul, crafting as herst
            b = herst.recipe_raw(name) or {}
            wert = kat_modul.classify(art=eintrag.get('a') or '',
                                       tag=b.get('tag') or '',
                                       unterart=b.get('subtype') or '',
                                       rezeptart=b.get('type') or '')
        except Exception:
            wert = ('', '')
        merker[name] = wert
        return wert

    def _oberkategorien(self):
        """Die Oberkategorien fürs Auswahlfeld — mit Anzahl.

        ⚠ **Zwei Ebenen statt dreissig Einträgen.** Rüstungsteile (Arme,
        Beine, Torso, Helm, Rucksack …) werden zu einer Oberkategorie
        gebündelt. Die Gliederung folgt der gepflegten Vergleichsliste.

        Was sich nicht bündeln lässt, bleibt als eigener Eintrag stehen.
        """
        from . import categories as kat_modul
        zaehler = {}
        for e in (self.katalog.get('bauplaene') or {}).values():
            ober, _u = self._kategorie(e)
            if ober:
                zaehler[ober] = zaehler.get(ober, 0) + 1
        eintraege = []
        for ober, n in zaehler.items():
            name = kat_modul.top_name(ober)
            if not kat_modul.is_group(ober):
                # Einzelgänger: den gewohnten Katalognamen zeigen.
                name = katalog_modul.kind_readable(kat_modul.raw_kind(ober)) or name
            eintraege.append((ober, '%s (%d)' % (name, n),
                              kat_modul.is_group(ober), name))
        # Gruppen zuerst, danach die Einzelgänger — beides alphabetisch.
        eintraege.sort(key=lambda p: (not p[2], p[3].lower()))
        return [(o, b) for o, b, _g, _n in eintraege]

    def _arten(self):
        """Die Arten für das Auswahlfeld — zusammengehörende nur einmal.

        ⚠ Über `art_kennung`, nicht über das rohe Feld: Sonst stehen `ammo`
        und `WeaponAttachment` als zwei Einträge in der Liste, beide mit der
        Beschriftung „Magazin" — einer mit 34 Bauplänen, einer mit null.
        """
        arten = {}
        for e in (self.katalog.get('bauplaene') or {}).values():
            roh = katalog_modul.kind_id(e)
            if roh:
                arten[roh] = katalog_modul.kind_readable(roh)
        return sorted(arten.items(), key=lambda p: p[1].lower())

    def _anzahl_je(self, feld, kennung=None):
        """Wie viele Baupläne hat jeder Wert dieses Feldes?

        Damit steht in der Auswahlliste, was einen erwartet — und eine Null
        ist erklärt statt rätselhaft. Beispiel „Competition": Die Klasse
        steht zu Recht in der Liste (das Spiel kennt sie), nur hat im Katalog
        4.9.0 kein einziger Bauplan sie.
        """
        from collections import Counter
        zaehler = Counter()
        nur_echte = feld in ('g', 's')
        for e in (self.katalog.get('bauplaene') or {}).values():
            if nur_echte and not self._feld_zaehlt(e, feld):
                continue          # dort ist die Zahl ohne Bedeutung — nicht mitzählen
            wert = kennung(e) if kennung else e.get(feld)
            if wert is not None and wert != '':
                zaehler[str(wert)] += 1
        return zaehler

    # ------------------------------- Wo Grad und Größe überhaupt etwas bedeuten
    #
    # ⚠ „Grad A" lieferte 603 von 722 Bauplänen — der Filter rechnete richtig, aber
    # die Zahl im Katalog steht nicht überall für etwas. Bei Schiffsteilen ist der
    # Gütegrad echt verteilt (Cooler: 11×A, 15×B, 12×C, 7×D); bei Rüstung steht
    # 314-mal die 1, weil das Feld ausgefüllt sein muss, nicht weil es Grad A wäre.
    # Dasselbe bei der Größe: 445-mal die 1.
    #
    # Deshalb wirken beide Filter nur auf die Arten, bei denen der Wert wirklich
    # unterschiedlich ausfällt. Das wird **aus den Daten abgeleitet**, nicht in eine
    # Liste geschrieben: Gibt CIG der Rüstung eines Tages echte Grade, greift der
    # Filter dort von selbst — und niemand muss daran denken, hier etwas
    # nachzutragen.
    #
    # Die Zehn-Prozent-Schwelle fängt Einzelfälle ab: Bei den Helmen tragen zwei von
    # 82 einen anderen Grad. Zwei Ausreißer machen aus einem bedeutungslosen Feld
    # noch kein Merkmal, nach dem man sinnvoll sucht.
    VERTEILT_MINDESTANTEIL = 0.10

    def _arten_mit_echtem(self, feld):
        """Bei welchen Arten sagt dieses Feld etwas aus? (Menge von Art-Kennungen)"""
        merker = getattr(self, '_verteilt_merker', None)
        if merker is None:
            merker = self._verteilt_merker = {}
        if feld in merker:
            return merker[feld]
        from collections import Counter
        je_art = {}
        for e in (self.katalog.get('bauplaene') or {}).values():
            wert = e.get(feld)
            if wert in (None, ''):
                continue
            je_art.setdefault(katalog_modul.kind_id(e), Counter())[str(wert)] += 1
        echt = set()
        for art, zaehler in je_art.items():
            gesamt = sum(zaehler.values())
            haeufigste = zaehler.most_common(1)[0][1]
            if gesamt and (gesamt - haeufigste) / float(gesamt) >= self.VERTEILT_MINDESTANTEIL:
                echt.add(art)
        merker[feld] = echt
        return echt

    def _feld_zaehlt(self, eintrag, feld):
        """Zählt dieser Bauplan für den Grad- bzw. Größenfilter überhaupt mit?"""
        return katalog_modul.kind_id(eintrag) in self._arten_mit_echtem(feld)

    def _mit_zahl(self, eintraege, zaehler):
        """An jede Beschriftung die Anzahl hängen — `Military (38)`."""
        return [(wert, '%s (%d)' % (text, zaehler.get(str(wert), 0)))
                for wert, text in eintraege]

    def _mit_katalog(self, fest, feld):
        """Die feste Liste, ergänzt um alles, was der Katalog sonst noch hat.

        So fehlt nichts, wenn ein Patch etwas Neues bringt — und nichts
        verschwindet, nur weil es gerade keinen Bauplan dazu gibt.
        """
        werte = list(fest)
        for wert in self._kat_werte(feld):
            if str(wert) not in werte:
                werte.append(str(wert))
        return werte

    def _klassen(self):
        return [(k, k) for k in self._mit_katalog(KLASSEN_FEST, 'c')]

    def _groessen(self):
        return [(s, t('ff_groesse') % s)
                for s in self._mit_katalog(GROESSEN_FEST, 's')]

    # (Grad und Größe kommen aus `_mit_katalog`; welche Baupläne dahinter zählen,
    #  entscheidet `_feld_zaehlt` — siehe oben.)

    def _grade(self):
        return [(g, t('ff_grad') % GRAD_BUCHSTABE.get(int(g), g).upper()
                 if g.isdigit() else g)
                for g in self._mit_katalog(GRADE_FEST, 'g')]

    def _patches(self):
        """Die Spielversionen, aus denen Baupläne stammen — neueste zuerst.

        Gewählt wird die volle Kennung (`4.10.0-live.12519617`), angezeigt die
        kurze mit Anzahl: „4.10.0 (16)". Die Liste pflegt sich selbst: Was ein
        Patch bringt, trägt seine Version als Stempel und steht damit beim
        nächsten Öffnen im Feld.

        ⚠⚠ **Die Kurzform allein reicht nicht.** `4.10.0-live.12519617` und
        `4.10.0-live.12545750` kürzen beide auf „4.10.0" — im Menü stünden
        dann **zwei gleich beschriftete Einträge** mit verschiedenen Zahlen,
        und niemand könnte sagen, welcher welcher ist.

        Dieselbe Regel gilt im Bericht (`report.py`, `_patch_history`). Wer
        sie an einer Stelle ändert, muss die andere mitnehmen: Der Bericht
        führt die Liste nur auf, das Menü lässt danach **auswählen**.

        Warum es überhaupt zwei 4.10.0 gibt: Ein Hotfix wurde in den
        Live-Kanal übernommen. Dabei ändern sich Werte an bestehenden
        Bauplänen, die Datenquelle nimmt sie neu auf — und sie tragen den
        Stempel des neuen Patches, obwohl es sie im Spiel längst gibt."""
        liste = katalog_modul.patches(self.katalog)
        kurzformen = [kurz for _voll, kurz, _anzahl in liste]
        return [(voll, '%s (%d)'
                 % (kurz if kurzformen.count(kurz) == 1 else voll, anzahl))
                for voll, kurz, anzahl in liste]

    def _quellen(self):
        """Fraktionen und Sonderquellen — beides, wonach man wirklich sucht."""
        fraktionen, sonder = set(), set()
        for e in (self.katalog.get('bauplaene') or {}).values():
            for q in e.get('q') or []:
                if q.get('fraktion'):
                    fraktionen.add(q['fraktion'])
            if e.get('topf'):
                sonder.add(e['topf'])
        eintraege = [('f:' + f, f) for f in sorted(fraktionen, key=str.lower)]
        eintraege += [('t:' + s, s) for s in sorted(sonder, key=str.lower)]
        return eintraege

    def _widerspruch_pruefen(self, gezeigt):
        """Meldet, wenn die Liste leer bleibt, obwohl das Feld Treffer verspricht.

        ⚠ **Der stumme Fehler.** Steht im Auswahlfeld eine Zahl (etwa
        `Schiffsmodule (157)`) und die Liste zeigt keinen Treffer, sieht das
        am Bildschirm aus wie ein leerer Bestand — und im Diagnosebericht
        steht nichts davon, weil nichts abgestürzt ist.

        Genau das hält diese Prüfung fest: Die Zahl **im Feld** kommt aus dem
        Katalog, die Zahl **in der Liste** aus dem Filter. Klaffen sie
        auseinander, stimmt der Filter nicht — und die Meldung steht im
        Bericht, bevor jemand ein Bildschirmfoto schicken muss.
        """
        try:
            if gezeigt or not self.fein.get('art'):
                return
            erwartet = 0
            for wert, beschriftung in self._oberkategorien():
                if wert == self.fein['art']:
                    zahl = beschriftung.rsplit('(', 1)[-1].rstrip(')')
                    erwartet = int(zahl) if zahl.isdigit() else 0
                    break
            if erwartet <= 0:
                return
            # Andere Filter dürfen sehr wohl auf null führen — dann ist es
            # kein Widerspruch, sondern eine ehrliche leere Schnittmenge.
            weitere = [s for s, w in self.fein.items() if w and s != 'art']
            wenn_nur_art = not weitere and self.filter == 'alle' \
                and not self.suche.get().strip()
            if not wenn_nur_art:
                return
            errors.record(
                'collection_window.filter_leer',
                RuntimeError('Kategorie %r verspricht %d Bauplaene, '
                             'die Liste zeigt keinen'
                             % (self.fein['art'], erwartet)))
        except Exception:
            # Eine Selbstprüfung darf nie das Zeichnen kosten.
            pass

    def _auftragsuebersicht(self):
        """Zeigt, welche Aufträge zum Suchbegriff passen — und wie viele
        Baupläne in jedem stecken.

        ⭐ Wer nach einem Auftrag sucht, will wissen, was er einbringt. Die
        Zeilen darunter sind dann die Baupläne selbst.
        """
        try:
            text = self.suche.get().strip().lower()
            if len(text) < 3:
                return                  # zu kurz — träfe zu viele Aufträge
            treffer = auftraege_zu(text, self.katalog)
            if not treffer:
                return
            tk.Label(self.inhalt, text=t('s_bp_auftrag_kopf') % text,
                     bg=BG, fg=ACCENT, font=schrift(11, fett=True),
                     anchor='w', pady=6).pack(fill='x')
            for name, anzahl in treffer[:8]:
                gewaehlt = (name == self.auftrag)
                z = tk.Label(
                    self.inhalt,
                    text=t('s_bp_auftrag_zeile') % (name, anzahl),
                    bg=theme.ACCENT_DARK if gewaehlt else FLAECHE,
                    fg=ACCENT if gewaehlt else FG,
                    font=schrift(10), anchor='w', cursor='hand2',
                    padx=10, pady=4)
                z.pack(fill='x', pady=1)
                # ⚠ Klick schaltet um: derselbe Auftrag noch einmal angeklickt
                # löst ihn wieder. Ein Filter, aus dem man nicht herauskommt,
                # ist schlimmer als keiner.
                z.bind('<Button-1>', lambda _e, a_=name: self._auftrag_waehlen(a_))
                z.bind('<Enter>', lambda _e, l=z, g=gewaehlt:
                       l.configure(bg=theme.ACCENT_DARK if g else BAR))
                z.bind('<Leave>', lambda _e, l=z, g=gewaehlt:
                       l.configure(bg=theme.ACCENT_DARK if g else FLAECHE))
            tk.Label(self.inhalt, text=t('s_bp_auftrag_klick'), bg=BG, fg=SUB,
                     font=schrift(9), anchor='w').pack(fill='x', pady=(4, 0))
            tk.Frame(self.inhalt, bg=BG, height=8).pack(fill='x')
        except Exception:
            pass

    def _auftrag_waehlen(self, name):
        """Einen Auftrag als Filter setzen — oder wieder lösen.

        ⚠ **Die aufgelöste Menge gehört mit zurückgesetzt.** Sie stammt aus
        `zum_auftrag()` und gilt nur für den Auftrag, für den sie geholt wurde.
        Bliebe sie stehen, filterte ein Klick in der Liste nach dem vorigen
        Auftrag — dieselbe Sorte wie ein Zwischenspeicher, den niemand leert.
        """
        self.auftrag_bp = None
        self.auftrag = '' if self.auftrag == name else name
        self.alle_zeigen = False
        self._zeichnen(nach_oben=True)

    def _eigene_beobachtungen(self):
        """Die Muster-Beobachtungen zeigen — sie stehen in keinem Katalog.

        Die Merkliste führt zwei Sorten: angeklickte Baupläne (die stehen im
        Katalog und erscheinen als normale Zeilen) und **eigene Regeln** mit
        Suchmustern. Für die zweite Sorte gibt es keinen Katalogeintrag, den man
        anzeigen könnte — also bekommt sie hier eigene Zeilen.

        ⚠ Nur unter „beobachtet". In der vollen Liste stünden sie zwischen
        Bauplänen, die es wirklich gibt, und sähen aus wie welche.
        """
        if self.filter != 'merk':
            return
        eintraege = merk.load().get('eintraege') or []
        if not eintraege:
            return
        tk.Label(self.inhalt, text=t('merk_eigene'), bg=BG, fg=ACCENT,
                 font=schrift(11, fett=True), anchor='w',
                 pady=6).pack(fill='x')
        tk.Label(self.inhalt, text=t('merk_eigene_h'), bg=BG, fg=SUB,
                 font=schrift(9), anchor='w', justify='left',
                 wraplength=560).pack(fill='x', pady=(0, 6))
        for e in eintraege:
            z = tk.Frame(self.inhalt, bg=FLAECHE)
            z.pack(fill='x', pady=2)
            kopfzeile = tk.Frame(z, bg=FLAECHE)
            kopfzeile.pack(fill='x', padx=10, pady=(6, 0))
            tk.Label(kopfzeile, text=e.get('titel') or '?', bg=FLAECHE, fg=FG,
                     font=schrift(10), anchor='w').pack(side='left', fill='x',
                                                        expand=True)
            # ⚠ Abwählen muss gehen — sonst wird jede Beobachtung zur Altlast.
            weg = tk.Label(kopfzeile, text='\u00d7', bg=FLAECHE, fg=SUB,
                           font=schrift(11), cursor='hand2')
            weg.pack(side='right')
            notice.attach(weg, lambda: t('merk_eigene_weg'))
            weg.bind('<Enter>', lambda _e, l=weg: l.configure(fg='#e05555'))
            weg.bind('<Leave>', lambda _e, l=weg: l.configure(fg=SUB))
            weg.bind('<Button-1>',
                     lambda _e, titel=(e.get('titel') or ''): self._eigene_weg(titel))
            muster = ', '.join(e.get('muster') or [])
            if muster:
                tk.Label(z, text=t('merk_wartet') % muster, bg=FLAECHE, fg=SUB,
                         font=schrift(9), anchor='w', justify='left',
                         wraplength=540).pack(fill='x', padx=10, pady=(0, 6))
        tk.Frame(self.inhalt, bg=BG, height=10).pack(fill='x')

    def _eigene_weg(self, titel):
        """Eine eigene Beobachtung abwählen."""
        try:
            merk.save(merk.remove_entry(titel))
        except Exception as ausnahme:
            errors.record('collection_window.eigene_weg', ausnahme)
        self._zeichnen(nach_oben=False)

    def _treffer_zeigen(self, gruppen):
        """Rechts die Zahl, links `zurücksetzen` — beides nur, wenn es zählt.

        Die Zahl zeigt, ob man gerade alles vor sich hat oder einen
        Ausschnitt. `zurücksetzen` erscheint erst, wenn wirklich etwas
        gesetzt ist — ein Knopf, der nichts tut, ist schlimmer als keiner.
        """
        if not hasattr(self, 'treffer_lbl'):
            return
        gezeigt = sum(len(treffer) for _, treffer in gruppen)
        gesamt = len(self.katalog.get('bauplaene') or {})
        eng = bool(self.suche.get().strip()) or self.filter != 'alle' \
            or any(self.fein.values()) or bool(self.auftrag) \
            or bool(self.katalog_art)
        self.treffer_lbl.configure(
            text=(t('ff_treffer') % (gezeigt, gesamt)) if eng
            else (t('ff_alle_treffer') % gesamt))

        self._widerspruch_pruefen(gezeigt)

        # Die ganze Zeile ein- oder ausblenden — sie steht unter den
        # Auswahlfeldern und nimmt sonst Platz weg, wenn nichts gefiltert ist.
        # ⚠ Auch die Suche und die Zustandswahl zählen. Wer „fehlt mir"
        # gewählt hat, will genauso zurückkönnen wie nach einem Auswahlfeld.
        if eng:
            self.zuruecksetzen_lbl.pack(side='right', padx=(24, 0))
        else:
            self.zuruecksetzen_lbl.pack_forget()

    def _fein_passt(self, e, key=None):
        """Kommt dieser Bauplan durch die fünf Auswahlfelder?

        Ein leeres Feld heißt „alle" und lässt alles durch. Die Quelle prüft
        zwei Dinge: `f:` eine Fraktion, die den Bauplan auslobt, `t:` einen
        Belohnungstopf (XenoThreat und Verwandte).

        ⚠ `key` ist der Katalog-Schlüssel der Zeile. Er wird nur für den
        Auftragsfilter gebraucht (siehe unten) und ist deshalb freiwillig —
        ohne ihn gilt der alte Weg über den Namen.
        """
        if self.auftrag:
            # Stammt dieser Bauplan aus dem angeklickten Auftrag?
            #
            # ⚠ Steht die aufgelöste Menge bereit (`zum_auftrag`), gilt sie —
            # sie kennt auch Titel mit Platzhalter. Sonst wie bisher über den
            # Namen: Ein Klick IN der Liste trägt den Namen aus den
            # Herkunftsdaten, und dort passt der Vergleich.
            aufgeloest = getattr(self, 'auftrag_bp', None)
            if aufgeloest is not None and key is not None:
                if key not in aufgeloest:
                    return False
            else:
                namen = {(q.get('auftrag') or '').strip()
                         for q in (e.get('q') or [])}
                if self.auftrag not in namen:
                    return False
        if self.katalog_art:
            # Gehört er zu der Kategorie, die im Fortschritt angeklickt wurde?
            if (katalog_modul.kind_readable(katalog_modul.kind_id(e))
                    != self.katalog_art):
                return False
        if self.fein.get('art') or self.fein.get('unterart'):
            ober, unter = self._kategorie(e)
            if self.fein.get('art') and ober != self.fein['art']:
                return False
            if self.fein.get('unterart') and unter != self.fein['unterart']:
                return False
        if self.fein['klasse'] and e.get('c') != self.fein['klasse']:
            return False
        # Aus welchem Patch stammt der Bauplan? Ohne Stempel ist er älter als
        # der erste Vergleich — er gehört dann in keinen der Patch-Einträge.
        if self.fein['patch'] and e.get('seit') != self.fein['patch']:
            return False
        # ⚠ Wer nach Größe 2 oder Grad A filtert, meint Schiffsteile. Arten, bei
        # denen die Zahl nur der Vollständigkeit halber dasteht (Rüstung, FPS-Waffen),
        # fallen deshalb heraus, statt das Ergebnis zu fluten.
        if self.fein['groesse']:
            if not self._feld_zaehlt(e, 's') or str(e.get('s')) != self.fein['groesse']:
                return False
        if self.fein['grad']:
            if not self._feld_zaehlt(e, 'g') or str(e.get('g')) != self.fein['grad']:
                return False
        quelle = self.fein['quelle']
        if quelle:
            if quelle.startswith('f:'):
                gesucht = quelle[2:]
                if not any((q.get('fraktion') or '') == gesucht
                           for q in e.get('q') or []):
                    return False
            elif quelle.startswith('t:'):
                if (e.get('topf') or '') != quelle[2:]:
                    return False
        return True

    def _feinfilter_felder(self):
        """Die Auswahlfelder bestücken — auch nach einem Wechsel der Art neu.

        ⚠ Eigene Methode, weil die **Unterarten von der Art abhängen**: Bei
        Schiffswaffen müssen dort `ballistic` und `laser` stehen, bei
        Rüstung die Körperteile. Ohne Neuaufbau bliebe die Liste der vorigen Art
        stehen, und wer daraus wählt, bekommt eine leere Trefferliste.
        """
        from .main_window import round_select

        def feld(schluessel, eintraege):
            if len(eintraege) <= 1:      # nichts zu wählen — Feld weglassen
                return
            w = round_select(self.fein_rahmen, eintraege,
                         self.fein.get(schluessel) or '',
                         lambda wert, s=schluessel: self._fein_setzen(s, wert),
                         schrift(10), bg=BG)
            self.fein_felder[schluessel] = w

        self.fein_felder = {}
        # Die Zahl hinter jedem Eintrag sagt, was einen erwartet — und erklärt
        # eine Null, statt sie rätselhaft zu lassen.
        feld('art', [('', t('ff_alle_arten'))] + self._oberkategorien())
        # ⭐ Unterart — teilt die lange Waffenliste in Ballistik, Laser,
        # Repeater, Cannon. Der Katalog kennt nur
        # `WeaponGun`; welche davon ballistisch sind, steht in den Rezeptdaten.
        # Beide werden über den Namen verbunden — 738 von 738 passen.
        #
        # ⚠ Das Feld erscheint nur, wenn die gewählte Art wirklich Unterarten
        # hat. Bei Kühlern gäbe es nichts zu wählen, und ein leeres Feld lässt
        # einen suchen, was es filtern soll.
        _unter = self._unterarten()
        # ⚠ Das leere Feld nennt die Zahl der Unterarten. Ein Feld mit „Alle
        # Unterarten" sieht aus wie eine Anzeige, eines mit Zahl und
        # Aufforderung zum Verfeinern wie eine Auswahl.
        feld('unterart',
             [('', t('ff_unterart_waehlen') % len(_unter) if _unter
                   else self._unterart_beschriftung())] + _unter)
        feld('klasse', [('', t('ff_alle_klassen'))]
             + self._mit_zahl(self._klassen(), self._anzahl_je('c')))
        feld('groesse', [('', t('ff_alle_groessen'))]
             + self._mit_zahl(self._groessen(), self._anzahl_je('s')))
        feld('quelle', [('', t('ff_alle_quellen'))] + self._quellen())
        feld('grad', [('', t('ff_alle_grade'))]
             + self._mit_zahl(self._grade(), self._anzahl_je('g')))
        # Erweitert sich von allein: Jeder Patch, der Baupläne bringt, stempelt
        # seine Version an die Neuzugänge und steht dadurch beim nächsten
        # Öffnen im Feld. Vor dem zweiten Katalogbau ist die Liste leer, dann
        # lässt `feld()` das Auswahlfeld weg.
        feld('patch', [('', t('ff_alle_patches'))] + self._patches())


        # Und neu anordnen — die Felder wechseln je nach Art.
        self._reihe_umbrechen(
            self.fein_rahmen,
            [self.fein_felder[k] for k in
             ('art', 'unterart', 'klasse', 'groesse', 'quelle', 'grad', 'patch')
             if k in self.fein_felder],
            rechts_frei=getattr(self, 'treffer_lbl', None))

    def _unterart_von(self, eintrag):
        """Die Unterart eines Katalog-Bauplans — aus den Rezeptdaten."""
        try:
            from . import crafting as herst
            return herst.subkind_of(eintrag.get('n') or '')
        except Exception:
            return ''

    def _unterart_beschriftung(self):
        """Was im leeren Unterart-Feld steht.

        ⚠ Die **Rüstungsrolle** (Kampf, Technik, Tarnung) ist keine eigene
        Auswahl. Bei Rüstung zählen die Körperteile.
        """
        return t('ff_alle_unterarten')

    def _unterarten(self):
        """Die Unterarten **der gewählten Oberkategorie** — mit Anzahl.

        ⚠ Ohne gewählte Oberkategorie bleibt die Liste leer. Alle Unterarten
        durcheinander (Laserkanone neben Helm neben Magazin) wäre wieder die
        lange Liste, die dieses Feld gerade abschaffen soll — zur Auswahl
        stehen nur die Unterarten passend zur Oberkategorie.
        """
        from . import categories as kat_modul
        ober = self.fein.get('art') or ''
        if not ober:
            return []
        zaehler = {}
        for e in (self.katalog.get('bauplaene') or {}).values():
            o, u = self._kategorie(e)
            if o != ober or not u:
                continue
            zaehler[u] = zaehler.get(u, 0) + 1
        return [(u, '%s (%d)' % (kat_modul.sub_name(u), n))
                for u, n in sorted(zaehler.items(),
                                   key=lambda p: kat_modul.sub_name(p[0]).lower())]

    def _fein_setzen(self, schluessel, wert):
        self.fein[schluessel] = wert
        # ⚠ Die Unterarten hängen an der Art. Wird die Art gewechselt, muss das
        # Feld neu bestückt werden — sonst stünden dort die Unterarten der
        # vorigen Art, und wer eine wählt, bekommt eine leere Liste.
        if schluessel == 'art':
            alt = self.fein.get('unterart') or ''
            if alt and alt not in [w for w, _b in self._unterarten()]:
                self.fein['unterart'] = ''
            self._feinfilter_neu()
        self.alle_zeigen = False
        self._zeichnen(nach_oben=True)

    def _feinfilter_neu(self):
        """Die Auswahlfelder neu aufbauen — nach einem Wechsel der Art."""
        try:
            merke = dict(self.fein)
            for kind in self.fein_rahmen.winfo_children():
                kind.destroy()
            # ⚠ Der Merker zeigt sonst auf zerstörte Widgets und `ordnen()`
            # hielte die neue Anordnung für „unverändert".
            self.fein_rahmen.zuletzt = None
            self._feinfilter_felder()
            self.fein.update(merke)
            for schluessel, w in self.fein_felder.items():
                if merke.get(schluessel):
                    try:
                        w.select_quiet(merke[schluessel])
                    except Exception:
                        pass
        except tk.TclError:
            pass

    def _alles_leeren(self):
        """Zurück auf Anfang — Auswahlfelder, Suchfeld und Zustandswahl.

        ⚠ Muss alles drei umfassen. Der Knopf erscheint, sobald **irgendetwas**
        eingegrenzt ist; nähme er nur die Auswahlfelder weg, drückte man ihn
        und die Liste bliebe gefiltert — schlimmer als kein Knopf.
        """
        self.suche.set('')
        self.filter = 'alle'
        self.auftrag = ''
        self.auftrag_bp = None
        self.katalog_art = ''
        # `_fein_leeren()` zeichnet neu — und dabei werden die Zustandsknöpfe
        # mit eingefärbt. Ein eigener Aufruf dafür wäre doppelt.
        self._fein_leeren()

    def _fein_leeren(self):
        """Alle Auswahlfelder zurück auf „alle" — und EINMAL neu zeichnen.

        ⚠ `setzen()` der Felder ruft den Rückruf mit auf. Fünf Felder
        nacheinander zurückzustellen hieße fünfmal die ganze Liste neu bauen;
        deshalb wird stumm gesetzt und am Ende einmal gezeichnet.

        ⚠⚠ **War nichts gesetzt, wird auch nichts gezeichnet.** Diese Routine
        läuft bei **jedem** Wechsel auf die Bauplan-Liste. Alle Zeilen neu zu
        zeichnen kostet rund 800 ms für eine Seite, die längst gebaut dasteht.

        Der Normalfall ist: kein Filter gesetzt — dann ist die Liste schon
        richtig, und es gibt nichts zurückzustellen.
        """
        etwas_gesetzt = (any(self.fein.values()) or self.alle_zeigen)
        for schluessel in self.fein:
            self.fein[schluessel] = ''
        for feld in self.fein_felder.values():
            feld.select_quiet('')
        self.alle_zeigen = False
        if etwas_gesetzt:
            self._zeichnen(nach_oben=True)

    def _suche_leeren(self):
        """Das Suchfeld leeren — aber nur, wenn etwas drinsteht.

        ⚠⚠ `set('')` loest den `trace` auch dann aus, wenn das Feld schon leer
        war, und der zeichnet die ganze Liste neu. Zusammen mit
        `_fein_leeren` waren das **794 ms** bei jedem Wechsel auf diese Seite,
        ohne dass sich irgendetwas geaendert haette.
        """
        if self.suche.get():
            self.suche.set('')

    def neu_laden(self, auch_katalog=False):
        """Den Bestand frisch von der Platte lesen und die Liste neu zeichnen.

        ⚠⚠ Fällt ein Bauplan, soll in der Liste sofort die neue Anzahl und
        der grüne Haken stehen. Die Seite wird aber einmal gebaut und danach
        nur ein- und ausgeblendet (dieselbe Falle wie beim Auftrags-Protokoll);
        würde `self.bestand` **nur** im `__init__` gelesen, bliebe der Stand
        vom ersten Öffnen stehen, obwohl der Fund längst in `bestand.json`
        liegt.

        ⚠ **Der Katalog bleibt standardmäßig, wie er ist.** Er ändert sich
        nicht dadurch, dass ein Bauplan freigeschaltet wird, und ihn mitzulesen
        kostet mehr als alles andere hier zusammen (gemessen: Bestand 1 ms,
        Katalog 9 ms, Stempeln 12 ms). Beim Seitenwechsel wird er trotzdem
        mitgenommen — dort ist Zeit dafür, und ein Patch kann zwischendurch
        neue Baupläne gebracht haben.

        ⚠ Suche und Filter bleiben stehen. Wer gerade nach etwas sucht und
        dabei einen Bauplan bekommt, will nicht seine Eingabe verlieren —
        er will den Haken auftauchen sehen.
        """
        try:
            self.bestand = bestand_datei.load()
            if auch_katalog:
                katalog_modul.refresh_stamp()
                self.katalog = katalog_modul.load()
            # ⭐⭐ **Nur neu zeichnen, wenn sich die Daten geändert haben.**
            #
            # `_zeichnen()` baut bis zu 200 Zeilen aus je mehreren Bauteilen
            # neu auf. Gemessen im Profillauf: **177 ms bei jedem Anzeigen
            # der Seite**, obwohl man nur kurz woanders war.
            #
            # ⭐⭐ Nur neu zeichnen, wenn sich etwas geändert hat — und der
            # Vergleich geht gegen den Abdruck, den `_zeichnen()` beim
            # **letzten tatsächlichen Aufbau** hinterlassen hat.
            if self._anzeige_stand() != getattr(self, '_letzter_stand', None):
                self._zeichnen()
        except Exception as ausnahme:
            errors.record('collection_window.neu_laden', ausnahme)

    def _platzhalter(self, feld):
        """Der graue Hinweis im leeren Suchfeld.

        ⚠⚠ **Warum es das braucht:** Die Liste findet auch *Aufträge* — ohne
        Hinweis sieht das Feld aus wie ein leeres Kästchen und verrät das
        nicht. Der Text steht genau dort, wohin man beim Tippen ohnehin sieht.

        ⚠⚠ **Kein Label ÜBER dem Feld** — das wäre ein Bauteil, das die
        Mausklicks abfängt: Wer auf den Hinweis klickt, klickt nicht ins Feld.

        Der Hinweis steht **im Feld selbst** (`fields.hint`), also gibt es
        nichts, was einen Klick abfangen könnte. Warum das die
        Textvariable nicht stört, steht ausführlich in `scbp/fields.py`.
        """
        # ⚠ Das Feld wird gemerkt, damit der Selbsttest es nicht im
        # Widget-Baum suchen muss. Solange der Hinweis steht, ist die
        # Textvariable abgehängt — ein Suchen „nach dem Feld mit dieser
        # Variable" fände es dann gar nicht und griffe ein beliebiges anderes.
        self.suchfeld = feld
        self._hinweis_steht = fields.hint(
            feld, self.suche, t('s_bp_suche_platz'), normal=FG, grey=SUB)

    def _loeschkreuz_zeigen(self):
        """Das ✕ nur zeigen, wenn es etwas zu löschen gibt."""
        if self.suche.get():
            self.loeschen_lbl.pack(side='right')
        else:
            self.loeschen_lbl.pack_forget()

    def _bereich_umschalten(self, gruppe):
        """Einen Bereich ein- oder ausblenden.

        Der letzte sichtbare lässt sich nicht auch noch ausblenden — eine leere
        Liste ohne erkennbaren Grund ist keine Einstellung, sondern ein Rätsel."""
        if gruppe in self.bereiche_aus:
            self.bereiche_aus.discard(gruppe)
        elif len(self.bereiche_aus) < len(katalog_modul.TOP_GROUPS) - 1:
            self.bereiche_aus.add(gruppe)
        self.alle_zeigen = False
        self._zeichnen(nach_oben=True)

    def _herkunftsbereich(self):
        """Der feste Block unter der Liste — Herkunft des gewählten Bauplans.

        ⚠ Muss **vor** der Liste gepackt werden. In tkinter bekommt das zuletzt
        gepackte Element mit `expand=True` den Rest des Platzes; käme dieser
        Block danach, schöbe die Liste ihn aus dem Fenster.

        Warum überhaupt fest: Ein Bauplan hat bis zu zwölf Bezugsquellen. In
        der Zeile aufgeklappt würde der Block über 700 Pixel hoch, sichtbar
        sind 465 — er schöbe die ganze Liste weg, und man wüsste nicht mehr,
        wo man war.
        """
        self.herkunft_rahmen = tk.Frame(self.root, bg=BG)
        self.herkunft_rahmen.pack(side='bottom', fill='x', padx=14,
                                  pady=(0, 10))
        self.gewaehlt = None
        self._herkunft_zeichnen()

    def _liste(self):
        # Anfangswerte für den Blockmodus. Müssen stehen, **bevor** die Leinwand
        # existiert: Ihr `yscrollcommand` feuert schon beim ersten Zeichnen.
        self._blockteile = {}
        self._reihen = []
        self._block_start, self._block_y, self._block_h = [], [], []
        self._gesamthoehe = 0
        self._pflege_laeuft = False

        rahmen = tk.Frame(self.root, bg=BG)
        rahmen.pack(fill='both', expand=True, padx=14, pady=(0, 10))
        # Anker für die Zurücksetzen-Zeile: Sie schiebt sich davor, damit sie
        # unter den Filtern steht und nicht unter der Liste.
        self.liste_traeger = rahmen
        self.leinwand = tk.Canvas(rahmen, bg=BG, highlightthickness=0)
        from .main_window import round_scrollbar
        rolle = round_scrollbar(rahmen, self.leinwand, bg=BG)
        self.inhalt = tk.Frame(self.leinwand, bg=BG)
        self.inhalt.bind('<Configure>', lambda e: self._rollbereich_anmelden())
        self.fenster = self.leinwand.create_window((0, 0), window=self.inhalt,
                                                   anchor='nw')
        self.leinwand.bind('<Configure>', lambda e: self._leinwand_breit(e.width))
        # ⚠ Zwischen Leinwand und Rollleiste gehängt: Beim Rollen müssen im
        # Blockmodus die Blöcke nachgezogen werden, die neu ins Bild kommen.
        def gerollt(anfang, ende):
            rolle.set(anfang, ende)
            self._bloecke_pflegen()

        self.leinwand.configure(yscrollcommand=gerollt)
        self.leinwand.pack(side='left', fill='both', expand=True)
        rolle.pack(side='right', fill='y')
        # ⚠ Hier stand `bind_all` OHNE `add='+'`. Das ersetzt jede vorher
        # gesetzte Bindung im ganzen Fenster — und weil diese Liste die
        # Startseite ist, war die Bindung aller anderen Seiten sofort wieder
        # weg. Danach rollte das Rad überall nur noch diese Liste, auch wenn
        # sie gar nicht zu sehen war.
        from .main_window import bind_wheel
        bind_wheel(self.leinwand)

    def _leinwand_breit(self, breite):
        """Der Inhalt ist so breit wie die Leinwand — im Blockmodus auch die Blöcke."""
        try:
            self.leinwand.itemconfigure(self.fenster, width=breite)
            for wid, _rahmen in getattr(self, '_blockteile', {}).values():
                self.leinwand.itemconfigure(wid, width=breite)
        except tk.TclError:
            pass

    # ----------------------------------------------------------------- Zeichnen
    def _filter_setzen(self, welcher):
        self.filter = welcher
        self.alle_zeigen = False
        self._zeichnen(nach_oben=True)

    def _auswahl(self):
        """Die Baupläne, die gerade angezeigt werden sollen.

        Die Reihenfolge kommt aus `catalog.groups_ordered()`: erst die
        Schiffsteile, dann die FPS-Waffen, dann Rüstung und Kleidung — nicht
        nach Alphabet, sonst stünde die Rüstung mittendrin."""
        text = self.suche.get().strip().lower()
        habe = bestand_datei.keys(self.bestand)
        beobachtet = merk.names()
        # Was mit dem letzten Patch dazukam. Einmal je Durchlauf holen — die
        # Menge ist für alle Zeilen dieselbe.
        neu_im_spiel = katalog_modul.new_ones(self.katalog)
        ergebnis = []
        for og, art, liste in katalog_modul.groups_ordered(self.katalog):
            if og in self.bereiche_aus:
                continue
            # ⚠ **Hier wird die Art NICHT vorab je Gruppe geprüft.** Die
            # Auswahl bietet **Oberkategorien** an (`schiffsmodul`); ein
            # Vergleich Katalog-Art gegen Oberkategorie trifft nie zu, und
            # **jede** Gruppe fiele heraus.
            #
            # Geprüft wird je Zeile in `_fein_passt()`. Das ist die einzige
            # Stelle, an der die Kategorie ausgewertet wird. Die Kategorie je
            # Bauplan ist gemerkt, das kostet also kaum etwas.
            # Suchwörter der Art: „Kühler" soll die Cooler finden, obwohl die
            # Kategorie im Spiel englisch heißt.
            wortliste = katalog_modul.keywords(liste[0].get('a')) if liste else ()
            art_passt = bool(text) and (text in art.lower()
                                        or any(text in w for w in wortliste))
            treffer = []
            for e in liste:
                k = katalog_modul._norm(e['n'])
                drin = k in habe
                if self.filter == 'habe' and not drin:
                    continue
                if self.filter == 'fehlt' and drin:
                    continue
                if self.filter == 'merk' and k not in beobachtet:
                    # ⚠ Auch die **Muster** zählen. Wird ein beobachtetes Teil
                    # im Spiel verfügbar, taucht es hier als ganz normale Zeile
                    # auf — mit Info-Zeichen, Abgabeort und Ruf. Vorher prüfte
                    # der Filter nur angeklickte Namen, und ein Treffer auf
                    # ein Muster-Treffer wäre unsichtbar geblieben:
                    # Man beobachtet etwas und erfährt nicht, dass es da ist.
                    if not merk.match(e['n']):
                        continue
                if self.filter == 'neu' and k not in neu_im_spiel:
                    continue
                # ⚠ Filter `deckel`: nur was **fehlt** und **nur** ueber
                # Auftraege mit Ruf-Obergrenze zu bekommen ist. Was man schon
                # hat, kann nicht mehr verloren gehen; und ein einziger Weg
                # ohne Deckel genuegt, damit nichts in Gefahr ist.
                if self.filter == 'deckel':
                    if drin or not katalog_modul.reward_cap(self.katalog, k):
                        continue
                if text and not art_passt and not _passt(e, text):
                    continue
                if not self._fein_passt(e, k):
                    continue
                treffer.append((e, drin))
            if treffer:
                ergebnis.append((art, treffer))
        return ergebnis

    def _anzeige_stand(self):
        """Der Fingerabdruck dessen, was die Liste zeigen WÜRDE.

        ⚠⚠ Er muss **jede** Quelle enthalten, aus der `_zeichnen()` liest —
        Daten **und** Anzeigezustand. Fehlt eine, bleibt die Liste still auf
        dem alten Stand, und das fällt niemandem auf.

        Dazu gehören der Anzeigezustand (Filter, Suche) und die **Merkliste**
        — `_auswahl()` liest `merk.names()` und `merk.match()`, die Zeilen
        lesen `merk.contains()`. Wer sie in einem zweiten Fenster ändert,
        sähe hier sonst weiter die alten Sterne.

        ⚠ Lieber eine Quelle zu viel als eine zu wenig: Ein Irrtum soll einen
        **überflüssigen** Neuaufbau kosten, nie ein veraltetes Bild.
        """
        try:
            merkliste = repr(merk.load())
        except Exception:
            merkliste = '?'                 # im Zweifel neu zeichnen
        return (repr(self.bestand), repr(self.katalog), merkliste,
                self.filter, self.suche.get(), repr(self.fein),
                self.auftrag, getattr(self, 'katalog_art', ''),
                self.alle_zeigen, repr(sorted(self.offen)),
                repr(getattr(self, 'bereiche_aus', None)))

    def _zeichnen(self, nach_oben=False):
        """Die Liste neu aufbauen.

        `nach_oben` springt an den Anfang. Nötig bei Suche und Filter: Die
        Ansicht behält sonst ihre alte Scrollposition, und wenn aus 714 Zeilen
        plötzlich fünf werden, steht man vor **leerer Fläche** und hält die Suche
        für kaputt — die Treffer sind da, nur weit über dem sichtbaren
        Ausschnitt.

        Beim Abhaken, Merken und Ausklappen bleibt die Position dagegen stehen —
        dort wäre ein Sprung nach oben ein Verlust, man arbeitet ja an einer
        bestimmten Stelle.

        ⚠ Stehenbleiben braucht die Pixelhöhe: Tk merkt sich den **Anteil** der
        Scrollfläche, nicht die Pixelhöhe. Klappt man die Herkunft aus, wird
        die Liste länger, derselbe Anteil zeigt plötzlich weiter oben — und die
        angeklickte Zeile ist weg. Gemessen: 0,50 sprang auf 0,43, also ein
        halbes Fenster weit. Deshalb wird hier die **Pixelhöhe** gemerkt und
        danach zurückgerechnet."""
        oben_px = None
        if not nach_oben:
            try:
                oben_px = self.leinwand.canvasy(0)
            except tk.TclError:
                oben_px = None

        # ⭐⭐ **Ab hier gibt es kein gueltiges Bild mehr.** Die naechste Zeile
        # zerstoert die Zeilen; was danach schiefgeht — ein vorzeitiges
        # `return`, eine Ausnahme —, hinterlaesst eine halbe oder leere Liste.
        # Der alte Abdruck wuerde sie weiter als Zustand A ausweisen, und
        # `neu_laden()` spraenge ab, obwohl A laengst weg ist.
        #
        # Fall: A zeichnen → leeren Katalog zeichnen (fliegt unten raus) → A
        # wiederherstellen → `neu_laden()` — die Liste bliebe leer.
        #
        # Deshalb: **erst ungueltig machen, dann bauen.** Jeder erfolgreiche
        # Ausgang schreibt den Abdruck selbst wieder.
        self._letzter_stand = None

        # ⭐⭐ **Jeder Zeichenvorgang bekommt eine Nummer.** Der Aufbau langer
        # Listen laeuft erst im Leerlauf — bis dahin kann laengst ein zweiter
        # Zeichenvorgang gelaufen sein. Ohne Nummer legt der alte Auftrag dann
        # seine Bloecke ueber das neue Bild und stempelt es auch noch als
        # gueltig. Fall: lange Ansicht einplanen → kurze Ansicht zeichnen →
        # Leerlauf abarbeiten.
        #
        # ⛔ Und die Nummer allein reicht nicht: Der Abdruck darf NICHT in
        # einem gemeinsamen Feld zwischengelagert werden, sonst greift der
        # ueberholte Auftrag nach dem Abdruck des neuen. **Zeilen und Abdruck
        # reisen zusammen mit dem Auftrag.**
        self._zeichen_lauf = getattr(self, '_zeichen_lauf', 0) + 1
        lauf = self._zeichen_lauf

        for kind in self.inhalt.winfo_children():
            kind.destroy()

        for schluessel, knopf in self.knoepfe.items():
            an = schluessel == self.filter
            knopf.restyle(fill_color=ACCENT if an else FLAECHE,
                         border_color=ACCENT if an else LINIE,
                         fg_color=BG if an else SUB)

        # (Hier standen die vier Bereichs-Knöpfe. Sie sind den fünf
        # Auswahlfeldern gewichen — die färben sich selbst, sobald etwas
        # gesetzt ist, und brauchen kein Nachziehen von außen.)
        self._loeschkreuz_zeigen()

        # ⚠ Die Warnzeile zum Filter `deckel` steht ÜBER der Liste, nicht im
        # Hilfetext: Die Mechanik dahinter (Auftraege mit Ruf-Obergrenze)
        # kennt kaum ein Spieler, und ein Hilfetext, den man erst durch
        # Draufzeigen findet, erreicht niemanden.
        if self.filter == 'deckel':
            tk.Label(self.inhalt, text=t('deckel_warnung'), bg=BG, fg=SUB,
                     font=schrift(10), justify='left', anchor='w',
                     wraplength=620).pack(fill='x', padx=24, pady=(12, 4))

        # ⚠⚠ **Messpunkte für den Bericht.** Zwischen `zeichnen beginnt` und
        # `steht` liegen Auswahl, Gruppierung und das Packen der Zeilen;
        # deshalb hier drei Zahlen statt einer: Auswahl, Zeilen, gesamt.
        _t_start = time.perf_counter()
        errors.trail('Liste: zeichnen beginnt')
        _t_auswahl = time.perf_counter()
        gruppen = self._auswahl()
        _ms_auswahl = (time.perf_counter() - _t_auswahl) * 1000
        habe = bestand_datei.keys(self.bestand)
        gesamt = len(self.katalog['bauplaene'])
        meine = sum(1 for k in self.katalog['bauplaene'] if k in habe)
        if gesamt:
            self.fortschritt.configure(
                text=t('von_gesamt', meine, gesamt,
                       round(100 * meine / gesamt)))
        elif not self.katalog['bauplaene']:
            # ⚠ Auch das ist ein **fertiges** Bild, nur ein sehr kurzes. Ohne
            # diesen Abdruck bliebe die Seite auf ewig ungueltig und wuerde bei
            # jedem Anzeigen neu gebaut — der Fall ohne Katalog trifft jede
            # frische Installation.
            self._hinweis_kein_katalog()
            self._letzter_stand = self._anzeige_stand()
            return

        # Zwei Wege, je nachdem wie lang die Liste wird:
        #
        # * **Kurz** (der Normalfall — beim Start 40 Zeilen, mit Suche oder Filter
        #   fast immer): alles in einen Rahmen packen. Erprobt und einfach.
        # * **Lang** (alle anzeigen ohne Filter, über 700 Zeilen): in Blöcken,
        #   weil ein einzelner Rahmen sonst höher würde, als X11 Fenster
        #   platzieren kann — siehe Abschnitt „Lange Liste in Blöcken".
        gesamt_zeilen = sum(len(paare) for _, paare in gruppen)
        in_bloecken = self.alle_zeigen and gesamt_zeilen > self._zeilen_deckel()
        # Bleibt None, wenn in Bloecken gebaut wird — dort laufen die Zeilen
        # erst im Leerlauf und gehoeren nicht in diese Messung.
        _ms_zeilen = None

        if in_bloecken:
            reihen = []
            for art, treffer in gruppen:
                reihen.append(('kopf', art, treffer))
                for eintrag, drin in treffer:
                    reihen.append(('zeile', eintrag, drin))
            # ⚠ `stand` wird hier schon gebildet und als Vorgabewert
            # eingefroren — er beschreibt die Daten, aus denen `reihen`
            # entstanden ist. Er gilt erst, wenn der Aufbau geglueckt ist.
            self.root.after_idle(
                lambda r=reihen, n=lauf, s=self._anzeige_stand():
                self._bloecke_aufbauen(r, n, s))
            gezeichnet = gesamt_zeilen
        else:
            self._bloecke_abraeumen()
            # ⚠ Eigene Beobachtungen zuerst. Sie stehen in keinem Katalog und
            # tauchten deshalb nirgends auf: Die Liste meldete „Du beobachtest
            # noch nichts", während neun Einträge hinterlegt waren — acht
            # Muster-Beobachtungen und ein Name, der nicht im Katalog steht.
            self._eigene_beobachtungen()
            self._auftragsuebersicht()
            deckel = self._zeilen_deckel() if self.alle_zeigen else ZEILEN_ZUERST
            gezeichnet = 0
            # ⚠ Die Zeilen getrennt messen. „gesamt" allein sagt nicht, ob die
            # Zeit in den Zeilen steckt oder in dem, was drumherum passiert —
            # und genau diese Unterscheidung entscheidet, wo man ansetzt.
            _t_zeilen = time.perf_counter()
            for art, treffer in gruppen:
                if gezeichnet >= deckel:
                    break
                self._gruppenkopf(art, treffer, habe)
                for eintrag, drin in treffer:
                    if gezeichnet >= deckel:
                        break
                    self._zeile(eintrag, drin)
                    gezeichnet += 1
            _ms_zeilen = (time.perf_counter() - _t_zeilen) * 1000

            rest = gesamt_zeilen - gezeichnet
            if rest > 0:
                mehr = tk.Label(self.inhalt, text=t('weitere_anzeigen', rest),
                                bg=BG, fg=ACCENT, font=schrift(10),
                                cursor='hand2', pady=10)
                mehr.pack(fill='x')
                mehr.bind('<Button-1>', lambda e: self._alle())
        self._treffer_zeigen(gruppen)
        if not gruppen and not (self.filter == 'merk'
                                and (merk.load().get('eintraege') or [])):
            leer = (t('merkliste_leer') if self.filter == 'merk'
                    else t('deckel_leer') if self.filter == 'deckel'
                    else t('neu_leer') if self.filter == 'neu'
                    else t('nichts_gefunden'))
            tk.Label(self.inhalt, text=leer, bg=BG, fg=SUB, font=schrift(11),
                     pady=20, wraplength=520, justify='center').pack()

        # ⚠ Der Gegenwert zu `_t_start` oben. Steht bewusst VOR den
        # `after_idle`-Sprüngen: Was danach kommt, läuft erst im Leerlauf und
        # gehört nicht mehr zum Zeichnen.
        errors.trail('Liste: gezeichnet (%d Zeilen, Auswahl %d ms, Zeilen %s, '
                    'gesamt %d ms)'
                    % (gezeichnet, round(_ms_auswahl),
                       ('%d ms' % round(_ms_zeilen)) if _ms_zeilen is not None
                       else 'in Bloecken',
                       round((time.perf_counter() - _t_start) * 1000)))

        # Die Zeilenhöhe einmal nachmessen — sie bestimmt, wie viele Zeilen in eine
        # Ansicht passen (siehe `_zeilen_deckel`).
        if not getattr(self, '_zeilenhoehe', 0):
            self.root.after_idle(self._zeilenhoehe_merken)

        if nach_oben:
            # Erst wenn Tk die neue Höhe kennt — sonst bezieht sich der Sprung
            # noch auf die Scrollfläche von vorher und landet daneben.
            self.root.after_idle(lambda: self.leinwand.yview_moveto(0))
        elif oben_px is not None:
            # ⚠ ZWEIMAL `after_idle`, und drinnen KEIN `update_idletasks()`.
            # Der erste Durchgang packt die Zeilen, der zweite läuft, wenn Tk
            # sie vermessen hat — dann stimmt `bbox('all')` von selbst.
            #
            # Mit `update_idletasks()` innerhalb des Idle-Handlers dauerte das
            # Zeichnen **29,6 Sekunden** statt einem Sechstel davon: Der Aufruf
            # arbeitet mitten im Zeichnen alle offenen Aufgaben ab und stößt
            # bei 120 Zeilen eine Kaskade an. Gemessen mit cProfile — 29,4 der
            # 29,6 Sekunden steckten in dieser einen Zeile.
            self.root.after_idle(
                lambda: self.root.after_idle(lambda: self._zurueck_zu(oben_px)))

        # ⭐⭐ **Wer zeichnet, schreibt den Abdruck — und zwar HIER, am Ende.**
        #
        # Nicht in `neu_laden()`: Das ist nur einer von **vierzehn** Wegen, die
        # `_zeichnen()` rufen. Sonst: Zustand A über `neu_laden()` zeichnen,
        # Zustand B direkt zeichnen, A auf der Platte wiederherstellen,
        # `neu_laden()` — der Vergleich findet sein altes A, springt ab, und
        # auf dem Bildschirm steht weiter B.
        #
        # ⚠ Und **nach** dem Aufbau, nicht davor: Bricht das Zeichnen ab,
        # bleibt der Abdruck ungültig (oben auf `None` gesetzt), und die Liste
        # wird beim nächsten Anzeigen neu gebaut. Vorher hätte ein Abbruch
        # einen Zustand als gezeichnet vermerkt, den niemand je gesehen hat.
        #
        # ⚠⚠ **Das Ende dieser Funktion ist nicht immer das Ende des Aufbaus.**
        # Bei langen Listen läuft `_bloecke_aufbauen()` erst im Leerlauf. Dort
        # ist der Aufbau nur **eingeplant**; der Abdruck reist mit dem Auftrag
        # mit und wird erst gültig, wenn die Blöcke wirklich stehen. Hier gilt
        # er deshalb nur für den geradlinigen Weg.
        if not in_bloecken:
            self._letzter_stand = self._anzeige_stand()

    def _rollbereich_anmelden(self):
        """Die Scrollfläche neu vermessen — aber höchstens einmal je Runde.

        ⚠ Hier lag der Grund, warum die Liste **30 Sekunden** zum Aufbau
        brauchte. Vorher hing am `<Configure>` des Inhalts direkt ein
        `bbox('all')`. Jedes gepackte Widget löst so ein Ereignis aus, und
        `bbox('all')` läuft über **alle** bisherigen — bei 801 Widgets in der
        Liste wird daraus quadratischer Aufwand.

        Gemessen mit dem echten Katalog (722 Baupläne, 400 im Bestand):
        30,0 s vorher. Mit 13 Testbauplänen fiel das nie auf — der Fehler war
        also schon lange da und wurde erst mit echten Daten sichtbar.

        Jetzt wird nur gemerkt, dass etwas zu tun ist, und einmal im Leerlauf
        gerechnet. Hundert Ereignisse ergeben eine Messung.
        """
        if getattr(self, '_rollbereich_faellig', False):
            return
        self._rollbereich_faellig = True

        def rechnen():
            self._rollbereich_faellig = False
            # ⚠ Im Blockmodus NICHT aus `bbox('all')` rechnen: Dort liegen immer
            # nur die Blöcke in der Nähe des Ausschnitts in der Leinwand, die
            # Hülle wäre also viel zu klein und das Rollen bräche zusammen.
            # Dort gilt die vorher gerechnete Gesamthöhe.
            if getattr(self, '_block_start', None):
                return
            try:
                self.leinwand.configure(scrollregion=self.leinwand.bbox('all'))
            except tk.TclError:
                pass

        self.root.after_idle(rechnen)

    def _zurueck_zu(self, oben_px):
        """Denselben Pixel wieder nach oben holen, egal wie lang die Liste ist.

        ⚠ Zweimal aufgepasst werden muss hier:

        * **Wann** gemessen wird. `after_idle` allein reicht nicht — die Zeilen
          sind dann gepackt, aber noch nicht vermessen, und `winfo_reqheight`
          meldet einen zu kleinen Wert. Damit wird aus jedem Anteil eine 1,0,
          und die Liste springt ans Ende. Genau so gemessen: aus Pixel 1700
          wurde 5202. Deshalb erst `update_idletasks`.
        * **Woran** gemessen wird: an der Scrollfläche (`bbox('all')`), denn
          auf die bezieht sich `yview_moveto`.
        """
        try:
            if getattr(self, '_block_start', None):
                gesamt = float(self._gesamthoehe)
            else:
                bereich = self.leinwand.bbox('all')
                gesamt = float(bereich[3]) if bereich else 0.0
            if gesamt > 1:
                self.leinwand.yview_moveto(max(0.0, min(1.0, oben_px / gesamt)))
        except tk.TclError:
            pass

    def _alle(self):
        self.alle_zeigen = True
        self._zeichnen()

    # ------------------------------------------------- Lange Liste in Blöcken
    #
    # Warum das nötig ist: X11 kann kein Fenster jenseits von 32767 Pixeln
    # platzieren (16-Bit-Koordinaten). Alle 722 Baupläne in **einen** Rahmen zu
    # packen ergibt bei üblicher Schrift gut 33000 Pixel — die letzten Zeilen
    # überlagerten sich. Ein Deckel wäre die einfache Antwort, kostet aber genau
    # das, wofür die Liste da ist.
    #
    # Die Lösung: Die Reihen werden in Blöcke zu je `BLOCK_REIHEN` aufgeteilt.
    # Jeder Block ist ein eigener Rahmen in der Leinwand, und **nur die Blöcke in
    # der Nähe des sichtbaren Ausschnitts liegen wirklich dort**. Was weit weg
    # ist, wird abgeräumt und beim Zurückrollen neu gebaut. Damit bleibt jede
    # Fensterkoordinate klein, egal wie lang die Liste wird.
    #
    # Die Höhen werden **vorher** gerechnet, nicht nachträglich gemessen: Nur so
    # steht jede Position von Anfang an fest und nichts springt beim Rollen.
    # Gemessen wird einmal, wie hoch ein Gruppenkopf und eine Zeile sind (mit und
    # ohne zweite Zeile darunter) — der Rest ist Rechnen.

    def _reihenhoehen_messen(self):
        """Einmal nachsehen, wie hoch Kopf und Zeilen wirklich sind.

        Die Werte hängen an Schriftgröße und Anzeige-Skalierung; raten geht
        schief. Gemessen wird an unsichtbaren Probestücken, damit nichts blinkt.
        """
        if getattr(self, '_hoehen', None):
            return self._hoehen
        probe = tk.Frame(self.leinwand, bg=BG)
        beispiel = {'n': 'Xxxxxxxxxxxxxxxx', 'q': None}
        self._gruppenkopf('PROBE', [(beispiel, False)], set(), eltern=probe)
        self._zeile(beispiel, False, eltern=probe)
        mit_zusatz = dict(beispiel)
        mit_zusatz['m'] = 'Probe'
        self._zeile(mit_zusatz, False, eltern=probe)
        probe.update_idletasks()
        kinder = probe.winfo_children()
        hoehen = [k.winfo_reqheight() + 2 for k in kinder]   # +2 für das pady
        probe.destroy()
        if len(hoehen) < 3 or min(hoehen) < 4:
            self._hoehen = (34, 45, 58)         # Notnagel, falls nichts messbar
        else:
            self._hoehen = (hoehen[0] + 16, hoehen[1], hoehen[2])
        return self._hoehen

    def _reihenhoehe(self, reihe):
        kopf_h, zeile_h, zeile_zusatz_h = self._reihenhoehen_messen()
        if reihe[0] == 'kopf':
            return kopf_h
        eintrag = reihe[1]
        hat_zusatz = bool(kuerzel(eintrag) or eintrag.get('m'))
        return zeile_zusatz_h if hat_zusatz else zeile_h

    def _bloecke_aufbauen(self, reihen, lauf=None, stand=None):
        """Das Gerüst anlegen: Wo liegt welcher Block, und wie hoch ist alles.

        `lauf` ist die Nummer des Zeichenvorgangs, aus dem `reihen` stammt,
        `stand` sein Fingerabdruck. Beide reisen **mit dem Auftrag**, nicht
        über ein gemeinsames Feld — sonst greift ein überholter Auftrag nach
        dem Abdruck des neueren.

        ⛔ **Der Abbruch steht ganz oben, VOR jeder Änderung.** Ein überholter
        Auftrag darf weder Blöcke anlegen noch welche abräumen: Er würde sonst
        seine alten Zeilen über das neue Bild legen.
        """
        if lauf is not None and lauf != getattr(self, '_zeichen_lauf', lauf):
            return                          # überholt — nichts anfassen
        self._bloecke_abraeumen()
        self._reihen = reihen
        self._block_start = list(range(0, len(reihen), BLOCK_REIHEN))
        self._block_y, self._block_h = [], []
        y = 0
        for start in self._block_start:
            hoch = sum(self._reihenhoehe(r)
                       for r in reihen[start:start + BLOCK_REIHEN])
            self._block_y.append(y)
            self._block_h.append(hoch)
            y += hoch
        self._gesamthoehe = y
        breite = max(1, self.leinwand.winfo_width())
        self.leinwand.configure(scrollregion=(0, 0, breite, y))
        geglueckt = self._bloecke_pflegen()

        # ⭐ **Jetzt erst steht das Bild — wenn es denn steht.**
        #
        # ⛔ `_bloecke_pflegen()` fängt `TclError` ab und kehrt trotzdem
        # normal zurück. Seine Rückkehr ist deshalb **kein Beleg** für einen
        # geglückten Aufbau: Ein `TclError` in `_block_bauen()` ergäbe null
        # gebaute Blöcke und trotzdem einen gültigen Abdruck. Deshalb meldet
        # es den Erfolg ausdrücklich zurück.
        #
        # ⚠ Und die Laufnummer noch einmal: Zwischen Gerüst und hier kann ein
        # Rückruf ein neues Zeichnen ausgelöst haben.
        if (geglueckt and stand is not None
                and (lauf is None
                     or lauf == getattr(self, '_zeichen_lauf', lauf))):
            self._letzter_stand = stand

    def _bloecke_abraeumen(self):
        """Alle Blöcke aus der Leinwand nehmen — beim Neuzeichnen der Liste."""
        for wid, rahmen in getattr(self, '_blockteile', {}).values():
            try:
                self.leinwand.delete(wid)
                rahmen.destroy()
            except tk.TclError:
                pass
        self._blockteile = {}
        self._reihen = []
        self._block_start, self._block_y, self._block_h = [], [], []
        self._gesamthoehe = 0

    def _block_bauen(self, nummer):
        start = self._block_start[nummer]
        rahmen = tk.Frame(self.leinwand, bg=BG)
        habe = bestand_datei.keys(self.bestand)
        for reihe in self._reihen[start:start + BLOCK_REIHEN]:
            if reihe[0] == 'kopf':
                self._gruppenkopf(reihe[1], reihe[2], habe, eltern=rahmen)
            else:
                self._zeile(reihe[1], reihe[2], eltern=rahmen)
        breite = max(1, self.leinwand.winfo_width())
        wid = self.leinwand.create_window((0, self._block_y[nummer]),
                                          window=rahmen, anchor='nw',
                                          width=breite)
        self._blockteile[nummer] = (wid, rahmen)

    def _hoehen_nachziehen(self):
        """Die echten Blockhöhen übernehmen — nach dem Auf- oder Zuklappen.

        ⚠ Die Rollhöhe entsteht aus **geschätzten** Zeilenhöhen (Kopf, Zeile,
        Zeile mit Zusatz). Das stimmt, solange jede Zeile gleich hoch ist. Klappt
        jemand die Herkunft eines Bauplans auf, wächst der Block aber um ein
        Vielfaches: Bei `Hart Scraper Module` sind es zwölf Wege. Die Schätzung
        weiß nichts davon, die Rollfläche bleibt zu kurz — und die unteren Wege
        sind nicht erreichbar.

        Deshalb hier: gebaute Blöcke ausmessen, alle Blöcke neu untereinander
        legen, Rollfläche auf die neue Gesamthöhe setzen.
        """
        if not self._block_start:
            return
        try:
            self.leinwand.update_idletasks()
            y = 0
            for nummer in range(len(self._block_start)):
                teil = self._blockteile.get(nummer)
                if teil is not None:
                    # Nur gebaute Blöcke lassen sich messen; die übrigen
                    # behalten ihre Schätzung, bis sie ins Bild kommen.
                    hoch = max(1, teil[1].winfo_reqheight())
                    self._block_h[nummer] = hoch
                    self.leinwand.coords(teil[0], 0, y)
                self._block_y[nummer] = y
                y += self._block_h[nummer]
            self._gesamthoehe = y
            breite = max(1, self.leinwand.winfo_width())
            self.leinwand.configure(scrollregion=(0, 0, breite, y))
        except tk.TclError:
            return
        self._bloecke_pflegen()

    def _bloecke_pflegen(self, *_):
        """Blöcke im Sichtfeld bauen, weit entfernte wieder abräumen.

        Gibt `True` zurück, wenn danach **jeder** gebrauchte Block wirklich
        dasteht.

        ⛔⛔ **Die normale Rückkehr allein beweist gar nichts** — hier wird
        `TclError` abgefangen, damit ein Rollvorgang nicht das Programm
        mitreißt. Wer daraus „ist gebaut" schließt, stempelt im Fingerabdruck
        ein Bild als gültig, das nie entstanden ist; deshalb der
        ausdrückliche Rückgabewert.
        """
        if not self._block_start:
            return True                     # nichts zu bauen ist auch fertig
        if getattr(self, '_pflege_laeuft', False):
            return False                    # läuft schon — Ausgang unbekannt
        self._pflege_laeuft = True
        gebraucht = set()
        geglueckt = False
        try:
            oben = self.leinwand.canvasy(0)
            unten = oben + max(1, self.leinwand.winfo_height())
            # Ein Block Vorlauf nach oben und unten: So ist beim Rollen schon
            # gezeichnet, was gleich ins Bild kommt.
            rand = max(self._block_h) if self._block_h else 0
            for nummer, y in enumerate(self._block_y):
                if y + self._block_h[nummer] >= oben - rand and y <= unten + rand:
                    gebraucht.add(nummer)
            for nummer in list(self._blockteile):
                if nummer not in gebraucht:
                    wid, rahmen = self._blockteile.pop(nummer)
                    try:
                        self.leinwand.delete(wid)
                        rahmen.destroy()
                    except tk.TclError:
                        pass
            for nummer in sorted(gebraucht):
                if nummer not in self._blockteile:
                    self._block_bauen(nummer)
            # ⭐ Nicht auf fehlende Ausnahmen verlassen, sondern **nachsehen**: Ein
            # Block, der in `_blockteile` fehlt, steht auch nicht auf dem
            # Schirm — egal warum.
            geglueckt = all(n in self._blockteile for n in gebraucht)
        except tk.TclError:
            geglueckt = False
        finally:
            self._pflege_laeuft = False
        # ⚠ Erst jetzt lässt sich prüfen, ob die Schätzung stimmt: Gebaut ist
        # gebaut, und ein aufgeklappter Bauplan ist ein Vielfaches höher als
        # eine Zeile. Ohne diese Runde bleibt die Rollfläche zu kurz und die
        # unteren Wege sind unerreichbar.
        self._hoehen_pruefen()
        return geglueckt

    def _hoehen_pruefen(self):
        """Weicht ein gebauter Block von seiner Schätzung ab? Dann nachziehen.

        Selbsttätig statt an jeder Klickstelle einzeln: Aufgeklappt wird an
        zwei Stellen (Herkunft eines Bauplans, weitere Wege darin), und beim
        nächsten Umbau käme eine dritte dazu, die jemand vergisst.

        ⚠ Der Wächter verhindert die Schleife — `_hoehen_nachziehen()` ruft
        `_bloecke_pflegen()`, und das käme sonst wieder hier heraus.
        """
        if getattr(self, '_hoehen_laeuft', False) or not self._blockteile:
            return
        abweichung = False
        try:
            for nummer, (_wid, rahmen) in self._blockteile.items():
                echt = max(1, rahmen.winfo_reqheight())
                if abs(echt - self._block_h[nummer]) > 2:
                    abweichung = True
                    break
        except (tk.TclError, IndexError, KeyError):
            return
        if not abweichung:
            return
        self._hoehen_laeuft = True
        try:
            self._hoehen_nachziehen()
        finally:
            self._hoehen_laeuft = False

    def _zeilen_deckel(self):
        """Wie viele Zeilen in eine Ansicht passen, ohne dass X11 aussteigt.

        Gerechnet wird aus der **gemessenen** Höhe einer echten Zeile: Sie hängt
        an Schriftgröße und Anzeige-Skalierung, ist also auf keinem Rechner gleich.
        Solange noch nichts gemessen wurde, gilt ein vorsichtiger Wert — er wird
        beim ersten Zeichnen sofort durch die echte Zahl ersetzt.
        """
        hoehe = getattr(self, '_zeilenhoehe', 0)
        if not hoehe:
            return 650
        return max(ZEILEN_ZUERST, int(HOECHSTE_INHALTSHOEHE / hoehe))

    def _zeilenhoehe_merken(self):
        """Die Höhe einer Zeile einmal nachmessen, wenn Tk sie gezeichnet hat."""
        try:
            kinder = [k for k in self.inhalt.winfo_children()
                      if k.winfo_height() > 1]
            if len(kinder) >= 4:
                # Der zweite bis vierte Eintrag: der erste ist ein Gruppenkopf
                # und niedriger als eine Bauplan-Zeile.
                hoehen = sorted(k.winfo_height() for k in kinder[1:4])
                self._zeilenhoehe = hoehen[len(hoehen) // 2]
        except tk.TclError:
            pass

    def _hinweis_kein_katalog(self):
        tk.Label(self.inhalt, text=t('kein_katalog'),
                 bg=BG, fg=FG, font=schrift(11), pady=14).pack()
        tk.Label(self.inhalt, bg=BG, fg=SUB, font=schrift(10), justify='left',
                 text=t('kein_katalog_hilfe')).pack()

    def _gruppenkopf(self, art, treffer, habe, eltern=None):
        drin = sum(1 for _, d in treffer if d)
        kopf = tk.Frame(eltern if eltern is not None else self.inhalt, bg=BG)
        kopf.pack(fill='x', pady=(14, 4))
        tk.Label(kopf, text=art.upper(), bg=BG, fg=ACCENT, font=schrift(9, True),
                 anchor='w').pack(side='left')
        tk.Label(kopf, text='  %d/%d' % (drin, len(treffer)), bg=BG, fg=SUB,
                 font=schrift(9), anchor='w').pack(side='left')

    def _zeile(self, eintrag, drin, eltern=None):
        name = eintrag['n']
        row = tk.Frame(eltern if eltern is not None else self.inhalt,
                         bg=FLAECHE)
        row.pack(fill='x', pady=1)

        check_icon = icons.line(row, 'haken' if drin else 'offen',
                              color=icons.GREEN if drin else icons.GREY,
                              background=FLAECHE, font=schrift(12))
        check_icon.configure(cursor='hand2', padx=10, pady=6)
        check_icon.pack(side='left')
        check_icon.bind('<Button-1>', lambda e, n=name: self._umschalten(n))
        icons.hover_group(check_icon)

        middle = tk.Frame(row, bg=FLAECHE)
        middle.pack(side='left', fill='x', expand=True)
        name_label = tk.Label(middle, text=name, bg=FLAECHE,
                            fg=FG if drin else SUB, font=schrift(11),
                            anchor='w')
        name_label.pack(fill='x')

        # ⭐ **Der Name führt zu den Zutaten.** Die Zeile zeigt, woher es den
        # Bauplan gibt; was man zum Bauen braucht, steht auf der
        # Herstellungs-Seite. Der Klick auf den Namen spart den Weg über
        # Seitenleiste plus Suchfeld (Drei-Klick-Regel).
        #
        # ⚠ Der Name war der einzige freie Platz in der Zeile — Haken, Info
        # und Stern sind längst belegt. Ein eigener Knopf daneben wäre das
        # vierte Bedienelement in einer Zeile gewesen.
        #
        # ⚠ Nur wo es wirklich ein Rezept gibt: Ein Klick, der auf einer
        # leeren Seite endet, ist schlimmer als keiner — dieselbe Regel wie
        # bei `zum_auftrag` und `zur_art`.
        # ⚠⚠ **Keine Rezept-Prüfung an dieser Stelle.** Die Rezepte werden
        # nachgeladen; eine Abfrage des Ladezustands hier trifft ihn nur
        # halb (ein zu früh gemerktes Nein, ein zu früher Zugriff auf
        # `laden()`), und beim ersten Öffnen wäre dann kein Name anklickbar.
        #
        # Die Prüfung verhindert ohnehin nichts: Gemessen haben **alle 738**
        # Baupläne ein Rezept. Führt ein
        # Sprung doch einmal ins Leere, sagt die Herstellungs-Seite das mit
        # ihrem eigenen Hinweis — besser als eine Liste, in der gar nichts
        # geht.
        if getattr(self, 'hauptfenster', None) is not None:
            base_color = FG if drin else SUB
            name_label.configure(cursor='hand2')
            name_label.bind('<Button-1>', lambda e, n=name: self._zur_herstellung(n))
            # ⚠ Ohne Rückmeldung beim Überfahren sieht man einem Namen nicht
            # an, dass er mehr ist als Text. Der Zeiger allein verrät es erst,
            # wenn man schon darauf steht.
            name_label.bind('<Enter>', lambda e, w=name_label: w.configure(fg=ACCENT))
            name_label.bind('<Leave>',
                          lambda e, w=name_label, f=base_color: w.configure(fg=f))
            notice.attach(name_label, lambda: t('hinweis_zutaten'))

        details = [t for t in (kuerzel(eintrag), eintrag.get('m')) if t]
        if details:
            tk.Label(middle, text=' · '.join(details), bg=FLAECHE, fg=SUB,
                     font=schrift(9), anchor='w').pack(fill='x')

        if eintrag.get('q'):
            symbol = 'zuklappen' if name in self.offen else 'hinweiszeile'
            # ⚠ `antippbar()` statt `zeile()`: eine Stufe groesser. Das Zeichen
            # oeffnet den Herkunftskasten — in reiner Zeilengroesse war es zu
            # klein, um es als Schaltflaeche zu erkennen und zu treffen.
            # ⚠⚠ **Wort dazu, nicht nur ein Zeichen.** Ein Symbol am rechten
            # Rand allein verrät nicht, dass hier steht, wo es den Bauplan
            # gibt.
            source_button = icons.tappable(row, symbol, background=FLAECHE,
                                     text=t('hk_knopf'), font=schrift(10))
            source_button.configure(cursor='hand2', padx=12, fg=ACCENT)
            source_button.pack(side='right')
            source_button.bind('<Button-1>', lambda e, n=name: self._herkunft_umschalten(n))
            icons.hover_group(source_button)
            notice.attach(source_button, lambda: t('hinweis_quellen'))
        elif eintrag.get('start'):
            # Startbaupläne: hat jeder von Anfang an, stehen in keinem Pool und
            # in keinem Log. Eigenes Zeichen, damit niemand nach einem Auftrag
            # sucht, den es nicht gibt.
            start_icon = icons.line(row, 'standard', color=icons.GREEN,
                                background=FLAECHE, font=schrift(10))
            start_icon.configure(padx=12)
            start_icon.pack(side='right')
            notice.attach(start_icon, lambda: t('hinweis_startbauplan'))
        else:
            # 59 Baupläne haben in den Daten keine Bezugsquelle — überwiegend
            # Event-Belohnungen (`Purgatory Camo`, `SecondWind`). Ohne Zeichen
            # sähe die Zeile aus, als hätte jemand vergessen, die Herkunft
            # einzutragen; mit ? steht da, was Sache ist: Es gibt keinen Auftrag,
            # über den man da herankommt.
            no_source_label = tk.Label(row, text='?', bg=FLAECHE, fg=SUB,
                            font=schrift(11), padx=12)
            no_source_label.pack(side='right')
            notice.attach(no_source_label, lambda: t('hinweis_ohne_quelle'))

        # Stern: worauf man wartet, wird auffällig angezeigt, sobald es auftaucht.
        # Bei schon vorhandenen Bauplänen wäre das Merken sinnlos — dort kein
        # Stern. ⚠ **Außer er ist schon gemerkt**: Erledigte bleiben auf der
        # Merkliste, und der Stern ist der einzige Weg, so einen
        # Eintrag wieder loszuwerden.
        watched = merk.contains(name)
        if not drin or watched:
            # Größer als der Rest: Der Stern ist das einzige Zeichen in der
            # Zeile, das man *trifft* statt liest — in Zeilenschrift war er zu
            # klein zum Klicken und ging neben dem Namen unter.
            star = icons.line(row, 'gemerkt',
                                  color=icons.YELLOW if watched else icons.GREY,
                                  background=FLAECHE, font=schrift(16))
            star.configure(cursor='hand2', padx=10)
            star.pack(side='right')
            star.bind('<Button-1>', lambda e, n=name: self._merken(n))
            icons.hover_group(star)
            notice.attach(star, lambda n=name: t('nicht_mehr_merken')
                              if merk.contains(n) else t('merken'))


    def _herkunft_zeichnen(self):
        """Den festen Block neu füllen — für den gerade gewählten Bauplan."""
        for kind in self.herkunft_rahmen.winfo_children():
            kind.destroy()

        eintrag = self._gewaehlter_eintrag()
        if eintrag is None:
            # Ohne Auswahl nur eine Zeile, damit die Liste den Platz behält.
            tk.Label(self.herkunft_rahmen, text=t('hk_nichts'), bg=BG, fg=SUB,
                     font=schrift(10), anchor='w').pack(fill='x', pady=(6, 2))
            return

        # ⚠ `badge` misst die Textbreite und braucht deshalb ein Font-Objekt.
        # Dieses Fenster reicht Schriften als Tupel weiter — `_as_font`
        # wandelt um, sonst gibt es `'tuple' object has no attribute 'metrics'`.
        from .main_window import badge as blase, round_frame, _as_font
        quellen = list(eintrag.get('q') or [])
        farbe = ACCENT if quellen else GELB
        kasten = round_frame(self.herkunft_rahmen, FLAECHE, farbe, radius=8,
                            base_color=BG)
        kasten.holder.pack(fill='x')

        kopf = tk.Frame(kasten, bg=FLAECHE)
        kopf.pack(fill='x', padx=14, pady=(10, 2))
        tk.Label(kopf, text=eintrag['n'], bg=FLAECHE, fg=FG, font=schrift(12, True),
                 anchor='w').pack(side='left')
        zu = icons.line(kopf, 'schliessen', background=FLAECHE,
                           font=schrift(11))
        zu.configure(cursor='hand2', padx=6)
        zu.pack(side='right')
        zu.bind('<Button-1>', lambda e: self._auswaehlen(None))
        notice.attach(zu, lambda: t('hk_zu'))
        if quellen:
            blase(kopf, t('hk_ein_weg') if len(quellen) == 1
                  else t('hk_wege') % len(quellen),
                  ACCENT, _as_font(schrift(9))).pack(side='right',
                                                        padx=8)

        # Unterzeile: Art, Klasse, Besitz — und der Hinweis auf die Sortierung.
        teile = [katalog_modul.kind_readable(eintrag.get('a'))]
        if eintrag.get('c'):
            teile.append(eintrag['c'])
        teile.append(t('hk_hast_du')
                     if bestand_datei.contains(self.bestand, eintrag['n'])
                     else t('hk_fehlt_dir'))
        if len(quellen) > 1:
            teile.append(t('hk_leichtester'))
        tk.Label(kasten, text=' · '.join(teile), bg=FLAECHE, fg=SUB,
                 font=schrift(9), anchor='w').pack(fill='x', padx=14,
                                                   pady=(0, 8))

        # ⚠⚠ Die Ruf-Obergrenze — die einzige Stelle, an der man etwas
        # **verlieren** kann, ohne es zu merken. Steht in Gold direkt unter den
        # Angaben, nicht irgendwo unten: Wer den Bauplan noch nicht hat, muss es
        # sehen, bevor er weiterliest.
        if not bestand_datei.contains(self.bestand, eintrag['n']):
            deckel = katalog_modul.reward_cap(
                self.katalog, katalog_modul._norm(eintrag['n']))
            if deckel:
                grenze, rang = deckel
                tk.Label(kasten,
                         text='⚠ ' + t('deckel_zeile') % (
                             rang or '—', '{:,}'.format(grenze).replace(',', '.')),
                         bg=FLAECHE, fg=GELB, font=schrift(9, True),
                         anchor='w').pack(fill='x', padx=14, pady=(0, 8))

        # Zwei Angaben, die ebenfalls in CIGs Vertragsdaten stehen und sonst
        # nirgends auftauchen: teilbar und Wiederholsperre.
        merkmale = katalog_modul.contract_traits(
            self.katalog, katalog_modul._norm(eintrag['n']))
        stichworte = []
        if merkmale['teilbar'] is True:
            stichworte.append('👥 ' + t('hk_teilbar'))
        elif merkmale['teilbar'] is False:
            stichworte.append('👤 ' + t('hk_nicht_teilbar'))
        if merkmale['sperre']:
            stichworte.append('⏱ ' + t('hk_sperre') % _dauer_text(
                merkmale['sperre']))
        if stichworte:
            tk.Label(kasten, text='  ·  '.join(stichworte), bg=FLAECHE, fg=SUB,
                     font=schrift(9), anchor='w').pack(fill='x', padx=14,
                                                       pady=(0, 8))

        if not quellen:
            if eintrag.get('start'):
                text = t('hk_start')
            elif eintrag.get('topf'):
                text = '%s: %s\n%s' % (t('hk_topf'), eintrag['topf'],
                                       t('hk_topf_text'))
            else:
                text = t('hk_keine')
            tk.Label(kasten, text=text, bg=FLAECHE, fg=SUB, font=schrift(10),
                     anchor='w', justify='left', wraplength=620).pack(
                         fill='x', padx=14, pady=(0, 12))
            return

        # Der einfachste Weg steht ausgeschrieben da — den braucht man zuerst.
        self._weg_zeigen(kasten, quellen[0])

        if len(quellen) > 1:
            self._weitere_wege(kasten, quellen[1:])

    def _gewaehlter_eintrag(self):
        """Der Katalogeintrag zum gewählten Namen — oder nichts."""
        if not getattr(self, 'gewaehlt', None):
            return None
        schluessel = katalog_modul._norm(self.gewaehlt)
        return (self.katalog.get('bauplaene') or {}).get(schluessel)

    def _weg_zeigen(self, eltern, q):
        """Eine Bezugsquelle als beschriftete Zeilen — Auftrag, Fraktion, …"""
        gitter = tk.Frame(eltern, bg=FLAECHE)
        gitter.pack(fill='x', padx=14, pady=(0, 10))
        gitter.columnconfigure(1, weight=1)

        rang = q.get('rang') or '—'
        if q.get('rep'):
            rang += '  (%s)' % t('ruf_punkte',
                                 f"{q['rep']:,}".replace(',', '.'))
        zeilen = ((t('hk_auftrag'), q.get('auftrag') or '—'),
                  (t('hk_fraktion'), q.get('fraktion') or '—'),
                  (t('hk_annahme'), ort_text(q.get('wo')) or '—'),
                  (t('hk_rang'), rang),
                  (t('hk_belohnung'),
                   ('%s aUEC' % f"{q['uec']:,}".replace(',', '.'))
                   if q.get('uec') else '—'))
        for nummer, (bez, wert) in enumerate(zeilen):
            tk.Label(gitter, text=bez, bg=FLAECHE, fg=SUB, font=schrift(9),
                     anchor='w').grid(row=nummer, column=0, sticky='w',
                                      padx=(0, 14), pady=1)
            tk.Label(gitter, text=wert, bg=FLAECHE, fg=FG, font=schrift(10),
                     anchor='w', justify='left', wraplength=520).grid(
                         row=nummer, column=1, sticky='w', pady=1)

    def _weitere_wege(self, eltern, weitere):
        """Die übrigen Wege — eingeklappt, damit sie den Block nicht sprengen."""
        rahmen = tk.Frame(eltern, bg=FLAECHE)
        rahmen.pack(fill='x', padx=14, pady=(0, 10))
        tk.Frame(rahmen, bg=LINIE, height=1).pack(fill='x', pady=(0, 8))

        kopf = icons.line(rahmen, 'aufklappen', background=FLAECHE,
                             font=schrift(10),
                             text='  ' + t('hk_weitere') % len(weitere))
        kopf.configure(cursor='hand2', anchor='w')
        kopf.pack(fill='x')
        # Aufgeklappt in einer eigenen Rollfläche mit begrenzter Höhe — der
        # Block steht fest unter der Liste und könnte sonst aus dem Fenster
        # ragen.
        from .main_window import bind_wheel, round_scrollbar
        bereich = tk.Frame(rahmen, bg=FLAECHE)
        leinwand = tk.Canvas(bereich, bg=FLAECHE, highlightthickness=0,
                             height=1)
        rolle = round_scrollbar(bereich, leinwand, bg=FLAECHE)
        leinwand.configure(yscrollcommand=rolle.set)
        inhalt = tk.Frame(leinwand, bg=FLAECHE)
        fenster = leinwand.create_window((0, 0), window=inhalt, anchor='nw')

        def hoehe_setzen(_=None):
            grenze = max(120, int(self.root.winfo_height() * 0.3))
            noetig = inhalt.winfo_reqheight()
            leinwand.configure(height=min(noetig, grenze),
                               scrollregion=(0, 0, 0, noetig))
            if noetig > grenze:
                rolle.pack(side='right', fill='y', padx=(6, 0))
            else:
                rolle.pack_forget()

        inhalt.bind('<Configure>', hoehe_setzen)
        leinwand.bind('<Configure>',
                      lambda e: leinwand.itemconfigure(fenster, width=e.width))
        leinwand.pack(side='left', fill='both', expand=True)
        bind_wheel(leinwand)

        def umschalten(_=None):
            if bereich.winfo_ismapped():
                bereich.pack_forget()
                kopf.swap_symbol('aufklappen')
            else:
                bereich.pack(fill='x', pady=(8, 0))
                hoehe_setzen()
                kopf.swap_symbol('zuklappen')
            # ⚠ Ohne das bleibt die Rollfläche so lang wie vorher — die
            # aufgeklappten Wege stehen dann unerreichbar unterhalb.
            self._hoehen_nachziehen()

        kopf.bind('<Button-1>', umschalten)
        for q in weitere:
            kopf_text, auftrag, unten, wo = quelle_text(q)
            zeile = tk.Frame(inhalt, bg=FLAECHE)
            zeile.pack(fill='x', pady=3)
            tk.Label(zeile, text=kopf_text, bg=FLAECHE, fg=GELB,
                     font=schrift(9), anchor='w').pack(fill='x')
            if auftrag:
                tk.Label(zeile, text='„%s"' % auftrag, bg=FLAECHE, fg=FG,
                         font=schrift(9), anchor='w', wraplength=600,
                         justify='left').pack(fill='x')
            rest = ' · '.join(x for x in (unten, wo) if x)
            if rest:
                tk.Label(zeile, text=rest, bg=FLAECHE, fg=SUB, font=schrift(9),
                         anchor='w', wraplength=600,
                         justify='left').pack(fill='x')

    # ------------------------------------------------------------------ Aktion
    def _umschalten(self, name):
        """Häkchen setzen oder entfernen — und sofort auf die Platte schreiben."""
        if bestand_datei.contains(self.bestand, name):
            bestand_datei.remove(self.bestand, name)
        else:
            bestand_datei.add(self.bestand, name, 'hand')
        bestand_datei.save(self.bestand)
        self._zeichnen()

    def _merken(self, name):
        """Stern an oder aus — sofort auf die Platte, kein Speichern-Knopf."""
        merk.toggle(name)
        self._zeichnen()

    def _herkunft_umschalten(self, name):
        """Bauplan wählen — die Herkunft erscheint im festen Block unten.

        ⚠ Hier wird die Liste **nicht** neu gebaut. Vorher tat sie das, weil
        der Herkunftsblock zwischen den Zeilen stand: Jeder Klick baute 700
        Zeilen neu auf, die Ansicht sprang, und der aufgeklappte Block schob
        alles weg. Jetzt ändert sich nur der Block unten — die Liste bleibt
        stehen, wo sie steht.
        """
        self._auswaehlen(None if name == getattr(self, 'gewaehlt', None)
                         else name)

    def _auswaehlen(self, name):
        self.gewaehlt = name
        self._herkunft_zeichnen()

    def zum_auftrag(self, name):
        """Die Liste auf diesen Auftrag stellen — alles, was er hergibt.

        Von aussen gerufen: Die Auswertung der ergiebigsten Auftraege nennt
        einen Auftrag mit einer Zahl daneben. Die Zahl allein beantwortet die naechste Frage
        nicht — **welche** Bauplaene sind das? Hier stehen sie.

        ⚠⚠ **Erst nachsehen, dann springen** — dieselbe Regel wie bei
        `zum_bauplan`: Kennt kein Bauplan diesen Auftrag als Quelle, wird die
        Liste NICHT umgestellt. Ein Sprung auf eine leere Liste sieht aus, als
        sei das Werkzeug kaputt. Rueckgabe sagt, ob es geklappt hat.

        ⚠ Gesetzt wird, nicht umgeschaltet: `_auftrag_waehlen` loest denselben
        Auftrag beim zweiten Klick wieder — richtig fuer einen Klick in der
        Liste, falsch fuer einen Sprung von aussen. Wer zweimal aus dem
        Fortschritt herspringt, will zweimal dasselbe sehen.
        """
        name = (name or '').strip()
        if not name:
            return False
        # ⛔⛔ Über `catalog.blueprints_for_contract` — siehe dort. Hier stand
        # ein wörtlicher Vergleich gegen `q['auftrag']`, der Titel aus dem
        # Spiel nie traf (Platzhalter `[LOCATION]`).
        from . import catalog as kat_modul
        treffer = kat_modul.blueprints_for_contract(self.katalog or {}, name)
        if not treffer:
            return False
        # ⚠ Die aufgelösten Schlüssel merken, nicht nur den Namen: Der Filter
        # kann sonst dieselbe Auflösung nicht machen und zeigte eine leere
        # Liste — der Sprung sähe aus, als sei das Werkzeug kaputt.
        self.auftrag_bp = treffer
        # ⚠ Der Filter muss auf `alle` — sonst versteckt `fehlt mir` genau die
        # Bauplaene, die man schon hat, und die Zahl daneben stimmt nicht mehr
        # mit dem ueberein, was dasteht.
        self.filter = 'alle'
        self.alle_zeigen = False
        self.suche.set('')
        self.auftrag = name
        self._zeichnen(nach_oben=True)
        return True

    def _zur_herstellung(self, name):
        """Zur Herstellungs-Seite springen, mit diesem Bauplan in der Suche.

        ⚠ Derselbe Weg wie beim Rohstoff-Sprung in den Bergbau: Der Name wird
        am Hauptfenster hinterlegt, die Zielseite liest ihn beim Aufbau aus.
        """
        try:
            hf = getattr(self, 'hauptfenster', None)
            if hf is None:
                return
            hf.crafting_search = name
            hf.jump_to('herstellung')
        except Exception as ausnahme:
            errors.record('collection_window.zur_herstellung', ausnahme)

    def zur_art(self, art):
        """Die Liste auf eine Katalog-Art stellen — Cooler, Schild, Helm.

        Von aussen gerufen: Im Bauplan-Fortschritt steht je Kategorie ein
        Balken mit „38 / 70" daneben. Die Zahl beantwortet die naechste Frage
        nicht — **welche** 70 sind das, und welche 32 fehlen mir? Hier stehen
        sie.

        ⚠⚠ **Erst nachsehen, dann springen** — dieselbe Regel wie bei
        `zum_auftrag` und `zum_bauplan`: Kennt der Katalog die Art nicht, wird
        die Liste NICHT umgestellt. Ein Sprung auf eine leere Liste sieht aus,
        als sei das Werkzeug kaputt.

        ⚠ Gefiltert wird ueber `catalog.kind_readable` — **dieselbe** Zuordnung,
        nach der der Fortschritt zaehlt. Wuerde hier der Feinfilter
        (`fein['art']`, aus `categories.classify`) benutzt, koennten Zahl und
        Liste stumm auseinanderlaufen.
        """
        art = (art or '').strip()
        if not art:
            return False
        bekannt = any(
            katalog_modul.kind_readable(katalog_modul.kind_id(e)) == art
            for e in ((self.katalog or {}).get('bauplaene') or {}).values())
        if not bekannt:
            return False
        # ⚠ Filter auf `alle`: Sonst versteckt `fehlt mir` genau die Bauplaene,
        # die man schon hat — und die Zahl im Fortschritt (38 / 70) zaehlt
        # beide Seiten.
        self.filter = 'alle'
        self.alle_zeigen = False
        self.suche.set('')
        self.auftrag = ''
        self.auftrag_bp = None
        self.katalog_art = art
        self._zeichnen(nach_oben=True)
        return True

    def zum_bauplan(self, name):
        """Diesen Bauplan zeigen und seine Herkunft gleich aufschlagen.

        Von aussen gerufen — die Herstellung schickt hierher, wenn jemand
        wissen will, woher er einen fehlenden Bauplan bekommt.

        ⚠⚠ **Erst nachsehen, dann springen.** Kennt der Katalog den Namen
        nicht, wird die Liste NICHT umgestellt: Ein Sprung auf eine leere
        Liste sieht aus, als sei das Werkzeug kaputt, und der Suchbegriff von
        vorhin waere obendrein weg. Rueckgabe sagt, ob es geklappt hat.
        """
        treffer = None
        for eintrag in ((self.katalog or {}).get('bauplaene') or {}).values():
            if bestand_datei.norm(eintrag.get('n') or '') == bestand_datei.norm(name):
                treffer = eintrag.get('n')
                break
        if not treffer:
            return False
        # ⚠ Filter zurueck auf `alle`: Steht er auf `fehlt mir` und der Bauplan
        # ist vorhanden (oder umgekehrt), faende die Suche ihn nicht.
        self.filter = 'alle'
        self.alle_zeigen = False
        self.suche.set(treffer)
        # Sofort zeichnen — die Suche wartet sonst aufs Ende des Tippens.
        self._zeichnen(nach_oben=True)
        self._auswaehlen(treffer)
        return True

    def schliessen(self):
        if getattr(self, 'eingebettet', False):
            return                     # eine Seite schließt sich nicht selbst
        if self.beim_schliessen:
            self.beim_schliessen()
        self.root.destroy()

    def run(self):
        self.root.mainloop()


if __name__ == '__main__':
    Bestandsfenster().run()
