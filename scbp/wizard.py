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
Der Einrichtungsassistent — jederzeit wiederholbar.

Läuft beim ersten Start von allein und ist danach über einen Knopf erreichbar.
Das ist Absicht: Wer sich mit Rechnern nicht auskennt, soll etwas nachstellen
können, ohne zu wissen, in welchem Menü es steckt. Ein Assistent führt; ein
Einstellungsfenster setzt voraus, dass man weiß, wonach man sucht.

    1. Sprache      zuerst, damit der Rest lesbar ist
    2. Star Citizen die eine Angabe, ohne die nichts geht
    3. Nachlesen    hier bekommt der Spieler seinen Bestand geschenkt
    4. Anzeige      Overlay-Verhalten, Schrift, Durchsichtigkeit, Spielzeit
    5. Start        mit dem System starten, Symbol neben der Uhr
    6. Angaben      was in die Texte des Spiels geschrieben wird
    7. Texte        Übersetzung holen und die Angaben eintragen
    8. Fertig       was jetzt passiert, und wo die Liste steckt

⭐ **Die wichtigsten Einstellungen gehören in den Assistenten** (17.09.2026).
Wer sie nur unter *Einstellungen* findet, lebt mit den Voreinstellungen — und
erfährt nie, dass das Overlay auch nur bei einem Neuzugang aufblenden kann.
Thematisch auf drei Karten verteilt, damit keine Seite eine Liste wird.

⚠ **„Angaben" steht VOR „Texte".** Der Texte-Schritt trägt die Angaben gleich
ein — mit den Schaltern, die davor gewählt wurden. Umgekehrt stünde nach dem
Ausschalten schon etwas in der Datei.

Ohne Spielordner fallen Nachlesen, Angaben und Texte weg; Anzeige und Start
gelten auch dann.

**Erst arbeitet das Programm, dann der Mensch.** Schritt 3 läuft von selbst und
holt aus den aufgehobenen Logs alles, was noch da ist. Von Hand nachtragen soll
nur, wer muss — und nur das, was wirklich keine Logdatei mehr hergibt.
"""
import os
import tkinter as tk

from . import errors
from . import collection as bestand_datei
from . import logsource, paths, language
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

# ⛔ **Hier standen die Schriftstufen `klein/normal/gross/sehrgross`** — seit
# dem 28.09.2026 nicht mehr. Der Assistent bietet dieselben Voreinstellungen an
# wie die Seite *Darstellung* (`main_window.FONT_PRESETS`: Auto, Full HD, WQHD,
# UHD 125 %, UHD 150 %). Zwei Namen für dieselbe Sache waren genau das, was die
# Symmetrie-Regel verhindern soll. Die Stufen selbst gibt es weiter — sie sind
# nur kein Auswahlmenü mehr, sondern das Ergebnis der Punktzahl
# (`main_window.level_for_points`).


def font(groesse, fett=False, unterstrichen=False):
    """Die Schrift des Assistenten.

    `unterstrichen` ist für Textlinks — im Haus die Auszeichnung dafür, dass
    man auf etwas klicken kann. Ohne sie sieht ein Textlink aus wie ein
    Hinweis und wird übersehen.
    """
    fam = 'Segoe UI' if paths.WINDOWS else 'Helvetica'
    teile = [fam, groesse]
    stil = ' '.join(x for x, an in (('bold', fett),
                                    ('underline', unterstrichen)) if an)
    teile.append(stil or 'normal')
    return tuple(teile)


def mono(groesse):
    return ('Consolas' if paths.WINDOWS else 'Menlo', groesse)


class Wizard:
    def __init__(self, eltern=None, nur_wenn_noetig=False):
        self.abgebrochen = False
        self.liste_zeigen = False
        self.nachlese_gelaufen = False
        self.schritt = 1
        self.gedeutet = None
        self.ohne_spielordner = False
        # Welche Einstellungen hier umgestellt wurden — `start()` zieht sie
        # danach im laufenden Programm nach (`apply_changes`).
        self.changed = set()
        # Einstellungsschlüssel → Handgriff. Für den Selbsttest: So lässt sich
        # jede Zeile bedienen, ohne ein Fenster anzuklicken.
        self.controls = {}
        # `pages._scheme_cards` beschriftet seine Vorschau mit `f_small` —
        # dieselbe Schrift wie die Hinweise unter jeder Zeile.
        self.f_small = font(9)

        self.root =tk.Toplevel(eltern) if eltern else tk.Tk()
        self.root.title(window_title(t('hf_titel') + ' — ' + t('assistent')))
        self.root.configure(bg=BG)
        # ⚠⚠ **Mit Position, nicht nur mit Größe.** Ein `geometry` ohne
        # `+x+y` überlässt die Platzierung dem Fenstermanager — und der
        # weiß nichts vom Hauptfenster. Auf mehreren Bildschirmen landete
        # so am 06.09.2026 ein Fenster außerhalb des sichtbaren Bereichs;
        # weil es modal war, ließ sich das Programm nicht einmal beenden.
        #
        # `center_over` setzt beides und fällt auf die reine Größe
        # zurück, wenn es kein Elternfenster gibt (eigenständiger Start).
        from .main_window import center_over
        if eltern is None or not center_over(self.root, eltern, 640, 600):
            self.root.geometry('640x600')
        self.root.protocol('WM_DELETE_WINDOW', self._cancel)

        self.kopf = tk.Frame(self.root, bg=BAR)
        self.kopf.pack(fill='x')
        self.titel = tk.Label(self.kopf, text='', bg=BAR, fg=FG,
                              font=font(13, True))
        self.titel.pack(side='left', padx=18, pady=12)
        self.zaehler = tk.Label(self.kopf, text='', bg=BAR, fg=SUB,
                                font=font(9))
        self.zaehler.pack(side='right', padx=18)

        self.buehne = tk.Frame(self.root, bg=BG)
        self.buehne.pack(fill='both', expand=True)

        fuss = tk.Frame(self.root, bg=BAR)
        fuss.pack(fill='x', side='bottom')
        self.fuss = fuss            # `_fit_window` braucht seine Wunschhöhe
        self.zurueck = tk.Label(fuss, text=t('zurueck'), bg=BAR, fg=SUB,
                                font=font(10), cursor='hand2', padx=16, pady=13)
        self.zurueck.pack(side='left')
        self.zurueck.bind('<Button-1>', lambda e: self._back())
        self.weiter = tk.Label(fuss, text='', bg=ACCENT, fg=BG,
                               font=font(10, True), cursor='hand2',
                               padx=14, pady=7)
        self.weiter.pack(side='right', padx=16, pady=6)
        self.weiter.bind('<Button-1>', lambda e: self._next())
        self.root.bind('<Return>', lambda e: self._next())

        self._draw()

    # ------------------------------------------------------------ Gerüst
    def _clear(self):
        for kind in self.buehne.winfo_children():
            kind.destroy()

    def _area(self):
        f = tk.Frame(self.buehne, bg=BG, padx=24, pady=20)
        f.pack(fill='both', expand=True)
        return f

    def _paragraph(self, eltern, text, farbe=SUB, groesse=10, oben=0, fett=False):
        tk.Label(eltern, text=text, bg=BG, fg=farbe, font=font(groesse, fett),
                 anchor='w', justify='left', wraplength=560).pack(
                     fill='x', pady=(oben, 0))

    def _order(self):
        """Die Schritte, die gerade gelten — in dieser Reihenfolge.

        ⚠ Eine Liste statt fester Nummern: Ohne Spielordner fallen drei
        Schritte weg, und wo es weder Autostart noch ein Ablagesymbol gibt,
        entfällt die Start-Karte. Mit festen Nummern stünde dort eine leere
        Seite, und der Zähler „Schritt 5 von 8" löge.
        """
        steps = ['sprache', 'ablage', 'spiel']
        if not self.ohne_spielordner:
            steps.append('lesen')
        steps.append('anzeige')
        if self._start_possible():
            steps.append('start')
        if not self.ohne_spielordner:
            steps += ['angaben', 'texte']
        steps.append('fertig')
        return steps

    def _current(self):
        steps = self._order()
        return steps[min(self.schritt, len(steps)) - 1]

    def _draw(self):
        self._clear()
        anzahl = len(self._order())
        self.zaehler.configure(text=t('schritt_von', self.schritt, anzahl))
        self.zurueck.configure(fg=SUB if self.schritt > 1 else BG,
                               cursor='hand2' if self.schritt > 1 else '')
        self.weiter.configure(text='  %s  ' % (t('fertig') if self.schritt >= anzahl
                                               else t('weiter')),
                              bg=ACCENT, fg=BG, cursor='hand2')
        {'sprache': self._step_language, 'ablage': self._step_storage,
         'spiel': self._step_game,
         'lesen': self._step_read, 'anzeige': self._step_display,
         'start': self._step_startup, 'angaben': self._step_details,
         'texte': self._step_texts, 'fertig': self._step_done}[self._current()]()
        self._fit_window()

    # ------------------------------------------------------------ Größe
    # Kleiner wird das Fenster nie — darunter wirkt es zusammengedrückt, und
    # die kurzen Schritte (Sprache, Fertig) sollen nicht winzig aufspringen.
    MIN_SIZE = (640, 600)

    def _fit_window(self):
        """Das Fenster auf den Schritt einstellen, der gerade gezeichnet wurde.

        ⚠⚠ **Feste 640 × 600 reichen nicht mehr** (28.09.2026). Der Schritt
        „Bauplan-Angaben" zeigt seit dieser Fassung vierzehn Übersetzungen und
        darunter das Feld für die eigene Adresse — das Feld lag **unterhalb des
        Fensterrands**, und man kam nur daran, indem man das Fenster größer
        zog. Gemeldet mit dem einzig richtigen Maßstab: *„das findet niemand und
        wird denken, es sei kaputt, und holt sich ein anderes Tool, was nicht
        kaputt ist."*

        Ein Assistent, bei dem man am Fensterrahmen ziehen muss, um an ein
        Eingabefeld zu kommen, ist kaputt — auch wenn jede einzelne Zeile
        funktioniert.

        Deshalb wird nach jedem Zeichnen **gemessen**, nicht geraten: Kopf, Fuß
        und der Platzbedarf der Bühne ergeben die Höhe. So wächst das Fenster
        mit jedem künftigen Schritt von selbst mit, und niemand muss daran
        denken.

        ⚠ Nach oben begrenzt der Bildschirm — ein Fenster, das darüber
        hinausragt, hat denselben Fehler nur andersherum. Passt es dann immer
        noch nicht, bleibt es beim Bildschirmmaß; das ist selten und immer noch
        besser als ein abgeschnittener Rand ohne Grenze.

        ⚠ `minsize()` zieht mit — Tk setzt eine kleinere `geometry()` sonst
        schlicht nicht durch. Dieselbe Falle hat das Overlay schon einmal
        außerhalb des Bildschirms landen lassen.
        """
        try:
            self.root.update_idletasks()
            breite = max(self.MIN_SIZE[0],
                         self.buehne.winfo_reqwidth(),
                         self.kopf.winfo_reqwidth())
            # ⚠⚠ **Nur Wunschmaße addieren, nie mit der aktuellen Größe
            # mischen.** Der erste Anlauf rechnete die Fußhöhe als
            # `root.winfo_height() - buehne - kopf` — und die Bühne trug beim
            # Messen noch die Größe des **vorigen** Schritts. Nach einem hohen
            # Schritt blieb das Fenster hoch, und „Fertig" stand in einer
            # halbleeren Fläche. Drei Wunschmaße, mehr braucht es nicht.
            hoehe = max(self.MIN_SIZE[1],
                        self.kopf.winfo_reqheight()
                        + self.buehne.winfo_reqheight()
                        + self.fuss.winfo_reqheight())
            # Platz für Fensterrahmen und Leisten lassen, nicht bis an die Kante.
            breite = min(breite, self.root.winfo_screenwidth() - 80)
            hoehe = min(hoehe, self.root.winfo_screenheight() - 120)
            if (breite, hoehe) != (self.root.winfo_width(),
                                   self.root.winfo_height()):
                self.root.minsize(min(breite, self.MIN_SIZE[0]),
                                  min(hoehe, self.MIN_SIZE[1]))
                self.root.geometry('%dx%d' % (breite, hoehe))
        except tk.TclError:
            pass                    # Fenster schon zu

    # ------------------------------------------------- Einstellungszeilen
    def _row(self, parent, title, hint, below=False):
        """Eine Einstellung: Name und kurzer Hinweis links, Bedienelement rechts.

        ⚠ Schalter und Regler stehen **rechts**, wie auf den Einstellungsseiten
        (Symmetrie). Nur Auswahlreihen (`below=True`) kommen darunter — sie sind
        auf Englisch zu breit für den Platz daneben. Auch dann linksbündig: Ein
        `pack()` ohne Anker säße mittig.
        """
        row = tk.Frame(parent, bg=BG)
        row.pack(fill='x', pady=(16, 0))
        if not below:
            # Fest zuerst packen — sonst schiebt ein langer Hinweis den
            # Schalter aus dem Fenster.
            control = tk.Frame(row, bg=BG)
            control.pack(side='right', padx=(16, 0))
        text = tk.Frame(row, bg=BG)
        text.pack(side='left', fill='x', expand=True)
        tk.Label(text, text=title, bg=BG, fg=FG, font=font(11), anchor='w',
                 justify='left').pack(fill='x')
        tk.Label(text, text=hint, bg=BG, fg=SUB, font=font(9), anchor='w',
                 justify='left', wraplength=400 if not below else 560).pack(fill='x')
        if below:
            # ⚠⚠ **Im Textblock anlegen, nicht nur hineinpacken.** Bis v3.50.0
            # war die Auswahlreihe ein Geschwister des Textblocks und wurde mit
            # `pack(in_=text)` hineingesetzt. Tk zeichnet aber nach der
            # Reihenfolge des Anlegens: Der später angelegte Textblock lag
            # darüber, und Overlay-Modus und Schriftgröße standen als leere
            # Lücke da — auswählen ließ sich nichts (gemeldet 17.09.2026).
            control = tk.Frame(text, bg=BG)
            control.pack(anchor='w', pady=(6, 0))
        return control

    def _choices(self, parent, key, options, active, on_choice):
        """Eine Reihe zum Auswählen — die gewählte leuchtet in der Markenfarbe."""
        reihe = tk.Frame(parent, bg=BG)
        reihe.pack(anchor='w')
        knoepfe = {}

        def zeichnen(wert):
            for w, k in knoepfe.items():
                an = w == wert
                k.configure(bg=ACCENT if an else FLAECHE, fg=BG if an else FG)

        def waehlen(wert):
            on_choice(wert)
            zeichnen(wert)

        for wert, text in options:
            k = tk.Label(reihe, text=' %s ' % text, font=font(10),
                         cursor='hand2', padx=10, pady=5)
            k.pack(side='left', padx=(0, 6))
            k.bind('<Button-1>', lambda e, w=wert: waehlen(w))
            knoepfe[wert] = k
        zeichnen(active)
        self.controls[key] = waehlen

    def _switch(self, parent, key, default):
        """Ein Schiebeschalter für eine Ja/Nein-Einstellung."""
        from .main_window import toggle_switch

        def umlegen():
            return self._flip(key, default)

        toggle_switch(parent, paths.setting_bool(key, default), umlegen).pack()
        self.controls[key] = umlegen

    def _set(self, key, value):
        paths.set_setting(key, value)
        self.changed.add(key)

    def _flip(self, key, default):
        new_value = not paths.setting_bool(key, default)
        self._set(key, new_value)
        return new_value

    # ---------------------------------------------------------- 4. Anzeige
    def _step_display(self):
        self.titel.configure(text=t('schritt_anzeige'))
        f = self._area()
        self._paragraph(f, t('as_spaeter'), SUB, 10)

        ziel = self._row(f, t('s_ov_modus'), t('as_modus_h'), below=True)
        self._choices(ziel, 'overlay_modus',
                      [('immer', t('s_ov_immer')), ('popup', t('s_ov_popup'))],
                      paths.setting('overlay_modus') or 'immer',
                      lambda k: self._set('overlay_modus', k))

        # ⚠⚠ **Dieselben Voreinstellungen wie unter „Darstellung"** (28.09.2026).
        # Hier standen noch die alten vier Stufen „Klein / Normal / Groß / Sehr
        # groß" (`FONT_CHOICES`), während die Einstellungsseite seit v3.58.0
        # **Auto / Full HD / WQHD / UHD 125 % / UHD 150 %** anbietet und einen
        # stufenlosen Regler dazu. Zwei Namen für dieselbe Sache, je nachdem wo
        # man hinsieht — genau das, was die Symmetrie-Regel verhindern soll.
        #
        # ⚠ `set_font_size` wirkt **sofort** auf das ganze Fenster, also auch
        # auf den Assistenten selbst. Das ist hier erwünscht: Man sieht beim
        # Klicken, was man wählt.
        from .main_window import FONT_PRESETS, auto_points
        ziel = self._row(f, t('hf_schrift'), t('as_schrift_h'), below=True)

        def groesse(name):
            punkte = dict(FONT_PRESETS).get(name)
            if punkte is None:
                punkte = auto_points(self.root.winfo_screenheight())
            paths.set_setting('schrift_voreinstellung', name)
            fenster = getattr(self, 'hauptfenster', None)
            if fenster is not None and hasattr(fenster, 'set_font_size'):
                fenster.set_font_size(punkte)
            else:
                # Noch kein Hauptfenster (allererster Start) — dann nur merken;
                # gebaut wird es gleich danach ohnehin mit dieser Größe.
                from .main_window import FONT_POINTS, level_for_points
                paths.set_setting(FONT_POINTS, punkte)
                paths.set_setting('schriftgroesse', level_for_points(punkte))
            # ⚠ `schriftgroesse` ist der Schluessel, den `start()` nachzieht
            # (`apply_changes`) — nicht die Voreinstellung. Ohne diese Zeile
            # bliebe die Wahl bis zum naechsten Programmstart wirkungslos.
            self.changed.add('schriftgroesse')

        self._choices(ziel, 'schrift_voreinstellung',
                      [(name, t('s_gr_' + name)) for name, _wert in FONT_PRESETS],
                      paths.settings().get('schrift_voreinstellung') or '',
                      groesse)

        ziel = self._row(f, t('e_deckkraft'), t('as_deckkraft_h'))
        from .main_window import slider
        wert = paths.setting_int('deckkraft_prozent', 93, 30, 100)
        anzeige = tk.Label(ziel, text='%d %%' % wert, bg=BG, fg=ACCENT,
                           font=font(9), width=5, anchor='e')

        def deckkraft(w):
            anzeige.configure(text='%d %%' % w)
            self._set('deckkraft_prozent', int(w))
            # Gleich am Overlay zeigen — man soll sehen, was man einstellt.
            _apply_opacity()

        slider(ziel, 30, 100, wert, deckkraft, width=150).pack(side='left')
        anzeige.pack(side='left', padx=(6, 0))
        self.controls['deckkraft_prozent'] = deckkraft

        ziel = self._row(f, t('s_zeit'), t('as_zeit_h'))
        self._switch(ziel, 'spielzeit_zeigen', False)

        # ⭐ Farbschema (28.09.2026). Es gibt sechs seit v3.58.0 — wer den
        # Assistenten durchläuft, hat sie bis dahin nie gesehen und findet sie
        # erst, wenn er die Einstellungen durchsucht.
        #
        # ⚠ Nur merken, nicht umfärben: Jedes Fenster hält die Farben als
        # Konstanten; ein halb umgefärbtes Programm wäre schlimmer als ein
        # ehrlicher Neustart (siehe `pages._appearance_page`). Beim ersten
        # Start stört das nicht — danach wird ohnehin neu gestartet.
        #
        # ⚠⚠ **Mit Vorschau, nicht als Textknöpfe** (28.09.2026, v3.60.0-rc5).
        # rc4 hat `pages._scheme_cards(compact=True)` genau dafür gebaut — und
        # hier stand weiter `_choices` mit den bloßen Namen. Der Baustein war
        # da, nur nicht eingehängt. Gemeldet mit der Frage, ob ein Neuling
        # überhaupt sieht, wie es aussehen würde: Nein, sah er nicht.
        from . import theme as theme_modul
        from .pages import _scheme_cards
        ziel = self._row(f, t('s_da_schema'), t('as_schema_h'), below=True)
        chosen = paths.setting(theme_modul.SETTING) or theme_modul.DEFAULT
        if chosen not in theme_modul.SCHEMES:
            chosen = theme_modul.DEFAULT

        def pick_scheme(name):
            theme_modul.choose(name)
            cards.select(name)

        cards = _scheme_cards(self, ziel, chosen, pick_scheme, compact=True)
        cards.pack(anchor='w')
        self.scheme_cards = cards
        self.controls[theme_modul.SETTING] = pick_scheme

        # ⭐ Welche Baupläne zählen (28.09.2026, Wunsch: „Abfrage welche BP man
        # sehen will, alle oder nur erspielbare?"). Die Einstellung gibt es
        # seit v3.60.0 unter *Erkennung* — hier wird sie einmal bewusst
        # entschieden, statt sie zu finden.
        #
        # ⚠ Als zwei benannte Knöpfe statt als Schalter: „Nur erspielbare" und
        # „Alle herstellbaren" sagen beide, was sie bedeuten. Ein Schalter
        # hieße „an/aus" von etwas, das man erst lesen muss.
        from . import catalog as katalog_modul
        ziel = self._row(f, t('as_umfang'), t('as_umfang_h'), below=True)
        self._choices(
            ziel, katalog_modul.SETTING_ALL,
            [('nur', t('as_umfang_nur')), ('alle', t('as_umfang_alle'))],
            'alle' if paths.setting_bool(katalog_modul.SETTING_ALL, False)
            else 'nur',
            lambda k: self._set(katalog_modul.SETTING_ALL, k == 'alle'))

    # ------------------------------------------------------------ 5. Start
    @staticmethod
    def _start_possible():
        from . import autostart
        return autostart.possible() or paths.WINDOWS

    def _step_startup(self):
        from . import autostart
        self.titel.configure(text=t('schritt_start'))
        f = self._area()
        self._paragraph(f, t('as_spaeter'), SUB, 10)

        if autostart.possible():
            ziel = self._row(f, t('autostart_win') if paths.WINDOWS
                             else t('autostart_linux'), t('as_autostart_h'))
            from .main_window import toggle_switch

            def autostart_um():
                autostart.set_on(not autostart.is_on())
                return autostart.is_on()

            toggle_switch(ziel, autostart.is_on(), autostart_um).pack()
            self.controls['autostart'] = autostart_um

        # Das Ablagesymbol gibt es nur unter Windows. Unter Linux gar nicht
        # erst zeigen — ein Schalter mit „nur Windows" daneben ist im
        # Assistenten nur Rauschen.
        if paths.WINDOWS:
            ziel = self._row(f, t('s_tray'), t('as_tray_h'))
            self._switch(ziel, 'tray', True)

    # ---------------------------------------------------------- 6. Angaben
    def _step_details(self):
        from . import injection, rank_thresholds
        self.titel.configure(text=t('schritt_angaben'))
        f = self._area()
        self._paragraph(f, t('as_angaben_text'), SUB, 10)
        for key, title, hint in (
                ('inj_an', 's_sp_an', 'as_inj_an_h'),
                ('inj_auto', 's_sp_auto', 'as_inj_auto_h'),
                (injection.SETTING_DETAILS, 's_sp_angaben', 'as_angaben_h'),
                (rank_thresholds.SETTING, 's_sp_rang', 'as_rang_h')):
            ziel = self._row(f, t(title), t(hint))
            self._switch(ziel, key, True)

    # ------------------------------------------------------- 1. Sprache
    def _step_language(self):
        self.titel.configure(text=t('schritt_sprache'))
        f = self._area()
        self._paragraph(f, t('schritt_sprache_text'), FG, 11)
        reihe = tk.Frame(f, bg=BG)
        reihe.pack(fill='x', pady=(20, 0))
        aktiv = language.chosen()
        for wert, text in (('auto', t('sprache_auto')), ('de', 'Deutsch'),
                           ('en', 'English')):
            an = wert == aktiv
            k = tk.Label(reihe, text=' %s ' % text, bg=ACCENT if an else FLAECHE,
                         fg=BG if an else FG, font=font(10), cursor='hand2',
                         padx=12, pady=8)
            k.pack(side='left', padx=(0, 8))
            k.bind('<Button-1>', lambda e, w=wert: self._language(w))

    def _language(self, wert):
        paths.set_setting('sprache', wert)
        language.set_language(wert)
        self.root.title(window_title(t('hf_titel') + ' — ' + t('assistent')))
        self._draw()

    # -------------------------------------------------- 2. Datenordner
    def _step_storage(self):
        """Wo die Daten liegen — gefragt, nicht still genommen.

        ⚠⚠ Bis v3.62.1 nahm Verse-Kit ungefragt den Dokumente-Ordner. Bei
        Parsul (29.09.2026) lag der in OneDrive und war für das Programm
        gesperrt: Einstellungen, Bestand, Statistik, Bergbau-Daten — nichts
        ließ sich speichern, und nicht einmal das Fehlerprotokoll sagte es.
        Deshalb wird hier gefragt und **vor** dem Weiter geprüft, ob sich dort
        schreiben lässt.
        """
        self.titel.configure(text=t('schritt_ablage'))
        f = self._area()
        self._paragraph(f, t('schritt_ablage_text'), FG, 11)
        if paths.WINDOWS:
            self._paragraph(f, t('schritt_ablage_hilfe'), SUB, 10, oben=10)

        self.ablage = tk.StringVar(value=paths.app_folder())
        zeile = tk.Frame(f, bg=BG)
        zeile.pack(fill='x', pady=(18, 0))
        from .main_window import round_entry
        feld = round_entry(zeile, self.ablage, mono(10), FLAECHE, LINIE, ACCENT, FG)
        feld.holder.pack(side='left', fill='x', expand=True, padx=(0, 8))
        knopf = tk.Label(zeile, text=' %s ' % t('durchsuchen'), bg=BAR, fg=FG,
                         font=font(10), cursor='hand2', padx=8, pady=6)
        knopf.pack(side='right')
        knopf.bind('<Button-1>', lambda e: self._choose_storage())

        self.ablage_meldung = tk.Label(f, text='', bg=BG, fg=SUB, font=font(10),
                                       anchor='w', justify='left', wraplength=560)
        self.ablage_meldung.pack(fill='x', pady=(10, 0))
        # ⚠ Geprüft wird beim Öffnen, nach „Durchsuchen" und beim Weiter — NICHT
        # bei jedem Tastendruck: Die Probe legt den Ordner an, und aus
        # „D:\Ver" würde sonst ein echter Ordner „D:\Ver".
        self._check_storage()

    def _choose_storage(self):
        from . import file_picker
        ordner = file_picker.choose_folder(t('schritt_ablage'))
        if ordner:
            self.ablage.set(ordner)
            self._check_storage()

    def _check_storage(self):
        """Lässt sich dort schreiben? Zeigt es an und gibt (ok, ziel) zurück."""
        eingabe = self.ablage.get().strip()
        if not eingabe:
            self.ablage_meldung.configure(text='', fg=SUB)
            return False, ''
        ziel = os.path.abspath(os.path.expanduser(eingabe))
        ok, _eigene, grund = paths.storage_status(ziel)
        if ok:
            self.ablage_meldung.configure(text=t('ablage_ok'), fg=ACCENT)
        else:
            self.ablage_meldung.configure(
                text=t('ablage_gesperrt') % paths.redact(grund), fg=GELB)
        return ok, ziel

    def _apply_storage(self):
        """Beim Weiter: den gewählten Ordner übernehmen. False = hierbleiben."""
        ok, ziel = self._check_storage()
        if not ok:
            return False
        if os.environ.get('SC_BP_HOME'):
            # Selbsttest und Sonderfälle: Der Ort steht fest, gewählt wird nicht.
            return True
        alt = paths.app_folder()
        if os.path.normcase(os.path.abspath(alt)) == os.path.normcase(ziel):
            return True
        # Was schon da ist (etwa die Sprache aus Schritt 1), kommt mit.
        try:
            paths.move_storage(alt, ziel)
        except Exception as ausnahme:
            errors.record('wizard.ablage_mitnehmen', ausnahme)
        if not paths.set_setting('ablage_ordner', ziel):
            self.ablage_meldung.configure(text=t('ablage_umzug_weg'), fg=GELB)
            return False
        return True

    # -------------------------------------------------- 3. Star Citizen
    def _step_game(self):
        self.titel.configure(text=t('schritt_spiel'))
        f = self._area()
        gefunden = paths.game_folder()
        self._paragraph(f, t('schritt_spiel_text'), FG, 11)
        self._paragraph(f, t('schritt_spiel_hilfe'), SUB, 10, oben=10)

        self.pfad = tk.StringVar(value=gefunden or '')
        self.pfad.trace_add('write', lambda *_: self._check_path())
        zeile = tk.Frame(f, bg=BG)
        zeile.pack(fill='x', pady=(18, 0))
        from .main_window import round_entry
        feld = round_entry(zeile, self.pfad, mono(10), FLAECHE, LINIE, ACCENT, FG,
                           placeholder=t('s_pl_spielordner'))
        feld.holder.pack(side='left', fill='x', expand=True, padx=(0, 8))
        knopf = tk.Label(zeile, text=' %s ' % t('durchsuchen'), bg=BAR, fg=FG,
                         font=font(10), cursor='hand2', padx=8, pady=6)
        knopf.pack(side='right')
        knopf.bind('<Button-1>', lambda e: self._choose())

        self.rueckmeldung = tk.Label(f, text='', bg=BG, fg=SUB, font=font(10),
                                     anchor='w', justify='left', wraplength=560)
        self.rueckmeldung.pack(fill='x', pady=(10, 0))

        if not gefunden:
            self._paragraph(f, t('gesucht_wurde_hier'), SUB, 9, oben=16)
            for ort in paths.searched_game_locations(4):
                tk.Label(f, text=ort, bg=BG, fg=SUB, font=mono(8), anchor='w',
                         justify='left', wraplength=560).pack(fill='x')

            # ⚠ Ohne diesen Ausweg sitzt fest, wer Star Citizen nicht auf
            # diesem Rechner hat: Der Weiter-Knopf bleibt grau, und weil
            # `needed()` am fehlenden Spielordner hängt, kommt der Assistent
            # bei jedem Start wieder. Genau so ging es beim Ansehen auf einem
            # Zweitrechner — man kam nie über diese Seite hinaus.
            # ⚠ Als schlichter grauer Text sieht das aus wie ein Hinweis, nicht
            # wie etwas zum Anklicken — genau so wurde er beim Ausprobieren
            # übersehen. Deshalb unterstrichen und in der Akzentfarbe: Das ist
            # im Haus die Auszeichnung für „hier kann man klicken".
            ohne = tk.Label(f, text='→  ' + t('ohne_spiel'), bg=BG, fg=ACCENT,
                            font=font(10, unterstrichen=True),
                            cursor='hand2')
            ohne.pack(anchor='w', pady=(18, 0))
            ohne.bind('<Button-1>', lambda e: self._without_game())
            ohne.bind('<Enter>', lambda e: ohne.configure(fg=FG))
            ohne.bind('<Leave>', lambda e: ohne.configure(fg=ACCENT))
        self._check_path()

    def _without_game(self):
        """Weiter ohne Spielordner — bewusst und einmalig gemerkt."""
        self.ohne_spielordner = True
        paths.set_setting('einrichtung_ohne_spiel', True)
        # ⚠ Nicht gleich zum Ende: Anzeige und Start gelten auch ohne Spiel.
        self.schritt = self._order().index('anzeige') + 1
        self._draw()

    def _choose(self):
        # ⚠ Siehe `file_picker`: Der Tk-Dialog wäre hier besonders unglücklich —
        # das ist der allererste Bildschirm, den ein neuer Nutzer sieht.
        from . import file_picker
        ordner = file_picker.choose_folder(t('spielordner'))
        if ordner:
            self.pfad.set(ordner)

    def _check_path(self):
        input_device = self.pfad.get().strip()
        self.gedeutet = paths.resolve_game_folder(input_device) if input_device else None
        if not input_device:
            self.rueckmeldung.configure(text='', fg=SUB)
        elif self.gedeutet:
            text = t('log_gefunden')
            if self.gedeutet.rstrip('/\\') != input_device.rstrip('/\\'):
                text += '\n' + t('ordner_gedeutet', self.gedeutet)
            self.rueckmeldung.configure(text=text, fg=ACCENT)
        else:
            self.rueckmeldung.configure(text=t('keine_log_darin'), fg=GELB)
        an = bool(self.gedeutet)
        self.weiter.configure(bg=ACCENT if an else BAR, fg=BG if an else SUB,
                              cursor='hand2' if an else '')

    # ----------------------------------------------------- 3. Nachlesen
    def _step_read(self):
        self.titel.configure(text=t('schritt_lesen'))
        f = self._area()
        self._paragraph(f, t('schritt_lesen_text'), FG, 11)
        self.ergebnis = tk.Label(f, text=t('lese_logs'), bg=BG, fg=SUB,
                                 font=font(11), anchor='w', justify='left',
                                 wraplength=560)
        self.ergebnis.pack(fill='x', pady=(20, 0))
        self.luecke = tk.Label(f, text='', bg=BG, fg=GELB, font=font(10),
                               anchor='w', justify='left', wraplength=560)
        self.luecke.pack(fill='x', pady=(12, 0))
        self.root.update()
        if not self.nachlese_gelaufen:
            self._reread()

    def _reread(self):
        """Läuft von selbst — hier muss niemand etwas tun."""
        self.nachlese_gelaufen = True
        errors.trail('Assistent: Logs nachlesen beginnt')
        try:
            anzahl_dateien = len(paths.log_backups())
            if anzahl_dateien:
                self.ergebnis.configure(text=t('lese_logs_n', anzahl_dateien))
                self.root.update()
            funde, bericht = logsource.read_backlog(logsource.ReadState())
            b = bestand_datei.load()
            neu = 0
            for name, _zusatz in funde:
                if bestand_datei.add(b, name, 'nachlese'):
                    neu += 1
            if neu:
                bestand_datei.save(b)
            errors.trail('Assistent: nachgelesen (%d neu)' % neu)
            self.ergebnis.configure(
                text=t('nachgelesen_gross', neu, bericht.get('dateien', 0)),
                fg=FG, font=font(12))
            if bericht.get('luecke') and bericht.get('grund'):
                self.luecke.configure(text=bericht['grund'] + '\n\n'
                                      + t('nachtragen_hinweis'))
        except Exception:
            # Ein Fehler hier darf die Einrichtung nicht abbrechen — der Watcher
            # läuft auch ohne Vorgeschichte weiter.
            self.ergebnis.configure(text=t('nachgelesen_gross', 0, 0), fg=SUB)

    # ------------------------------------------- 4. Bauplan-Angaben im Spiel
    def _step_texts(self):
        """Die einzige Stelle, an der dieses Werkzeug etwas am Spiel verändert —
        deshalb wird hier **gefragt**, nicht stillschweigend gemacht.

        Drei Wege plus „jetzt nicht". Voreingestellt ist nichts: Wer weiterklickt,
        ohne etwas zu wählen, behält seine Installation unverändert."""
        self.titel.configure(text=t('schritt_spiel_texte'))
        f = self._area()
        self._paragraph(f, t('inj_text'), FG, 11)
        self._paragraph(f, t('inj_wie'), SUB, 10, oben=10)

        self.inj_meldung = tk.Label(f, text='', bg=BG, fg=SUB, font=font(10),
                                    anchor='w', justify='left', wraplength=560)

        # ⚠⚠ **Die Quellen kommen aus `translation.grouped_sources()`** — hier
        # standen bis zum 28.09.2026 **drei fest verdrahtete Zeilen**
        # (`deutsch`, `starstrings`, `original`). Der Reiter „Übersetzung" baute
        # seine Liste dagegen aus `SOURCES`, und so lief beides auseinander:
        # Bei v3.60.0 kannte der Assistent **2 von 14** Quellen. Aufgefallen ist
        # es nur, weil jemand hinsah — kaputt war nichts, es fehlte bloß.
        #
        # Eine neue Übersetzung braucht jetzt genau **eine** Zeile in `SOURCES`.
        #
        # ⚠ Eine Reihe je Sprache, nicht alles untereinander: Vierzehn Knöpfe
        # in einer Spalte sprengen das Fenster des Assistenten.
        from . import translation
        from .pages import _flag
        # ⚠⚠ **Die Wahl muss man sehen** (28.09.2026): *„beim Anklicken wird das
        # Ausgewählte nicht hervorgehoben, so weiß niemand, was er gewählt
        # hat."* Der Reiter „Übersetzung" hebt die aktive Quelle seit jeher
        # hervor — hier fiel es erst auf, als aus drei Knöpfen vierzehn wurden.
        # Bei dreien ahnt man noch, was man angeklickt hat; bei vierzehn nicht.
        self._quellknoepfe = {}
        erste = True
        for gruppe in translation.grouped_sources():
            reihe = tk.Frame(f, bg=BG)
            reihe.pack(anchor='w', pady=(14 if erste else 6, 0))
            erste = False
            for quelle in gruppe:
                land = ('gb' if quelle == 'original'
                        else (translation.SOURCES.get(quelle) or {}).get('flagge'))
                flagge = _flag(land, master=f) if land else None
                k = tk.Label(reihe, text='  %s  ' % translation.display_name(quelle),
                             bg=FLAECHE, fg=FG, font=font(11), cursor='hand2',
                             padx=10, pady=8, image=flagge or '',
                             compound='left')
                k.image = flagge    # sonst räumt Python das Bild weg
                k.pack(side='left', padx=(0, 6))
                k.bind('<Button-1>', lambda e, q=quelle: self._fetch_texts(q))
                self._quellknoepfe[quelle] = k
        # Was schon gewählt ist, steht beim Öffnen hervorgehoben da — wer den
        # Assistenten ein zweites Mal durchläuft, sieht seine eigene Wahl.
        self._quelle_hervorheben(paths.setting('inj_quelle') or '')

        # ⭐ **Die eigene Adresse gehört auch hierher** (28.09.2026). Sie gibt es
        # seit v3.59.0 unter „Übersetzung" — im Assistenten fehlte sie, und wer
        # eine andere Übersetzung nutzt, hatte hier keinen Weg außer „Nicht
        # anfassen" und später selbst suchen.
        #
        # ⚠ Das Feld liegt eingeklappt darunter: Ein Eingabefeld, das immer
        # offen steht, sieht aus wie eine Pflichtangabe.
        eigene_kasten = tk.Frame(f, bg=BG)
        self.eigene_url = tk.StringVar(
            value=(translation._custom_settings(None).get('url') or ''))

        def eigene_zeigen(_=None):
            if eigene_kasten.winfo_ismapped():
                eigene_kasten.pack_forget()
            else:
                eigene_kasten.pack(fill='x', pady=(8, 0))
                self._quelle_hervorheben(translation.CUSTOM)
            # ⚠ Das Fenster muss **mitwachsen**: Klappt der Kasten auf, liegt
            # das Eingabefeld sonst unter dem Fensterrand — genau der Fall, für
            # den es `_fit_window` gibt.
            self._fit_window()

        eigene_knopf = tk.Label(f, text='  %s  ' % t('s_sp_q_eigen'),
                                bg=FLAECHE, fg=FG, font=font(11),
                                cursor='hand2', padx=10, pady=8)
        eigene_knopf.pack(anchor='w', pady=(10, 0))
        eigene_knopf.bind('<Button-1>', eigene_zeigen)
        self._quellknoepfe[translation.CUSTOM] = eigene_knopf

        from .main_window import round_entry
        self._paragraph(eigene_kasten, t('s_tq_url_h'), SUB, 9)
        zeile_url = tk.Frame(eigene_kasten, bg=BG)
        zeile_url.pack(fill='x', pady=(4, 0))
        feld_url = round_entry(zeile_url, self.eigene_url, mono(10), FLAECHE,
                               LINIE, ACCENT, FG,
                               placeholder=t('s_tq_url_platz'))
        feld_url.holder.pack(side='left', fill='x', expand=True, padx=(0, 8))

        def eigene_uebernehmen(_=None):
            # ⚠ Die Sprache steht hier nicht zur Wahl — im Assistenten wäre
            # eine zweite Auswahl zu viel. `set_custom` nimmt Englisch als
            # Standard; unter „Übersetzung" lässt sich beides ändern.
            adresse = translation.set_custom(self.eigene_url.get(), 'english')
            if not adresse:
                self.inj_meldung.configure(text=t('s_tq_url_falsch'), fg=GELB)
                return
            self._fetch_texts(translation.CUSTOM)

        knopf_url = tk.Label(zeile_url, text=' %s ' % t('s_tq_uebernehmen'),
                             bg=BAR, fg=FG, font=font(10), cursor='hand2',
                             padx=8, pady=6)
        knopf_url.pack(side='right')
        knopf_url.bind('<Button-1>', eigene_uebernehmen)

        # ⚠⚠ Der vierte Weg braucht einen Knopf, sonst gibt es ihn nicht.
        # Der Docstring oben nennt ihn seit jeher („Drei Wege plus ‚jetzt
        # nicht'"), und weiterklicken ohne Wahl tat auch genau das — nur stand
        # im Fenster nichts davon. Gemeldet von Choopa (28.09.2026), der die
        # Übersetzungen des SCLC nutzt: „Was wenn ich das nicht direkt will?"
        # Ein Weg, den man nicht sieht, ist für den Nutzer keiner.
        #
        # Ohne Flagge: Das hier ist keine Sprache, sondern die Entscheidung,
        # keine zu nehmen. Abgesetzt durch den größeren Abstand darüber.
        nichts = tk.Label(f, text='  %s  ' % t('inj_quelle_nichts'), bg=FLAECHE,
                          fg=SUB, font=font(11), cursor='hand2',
                          padx=10, pady=8)
        nichts.pack(anchor='w', pady=(14, 0))
        nichts.bind('<Button-1>', lambda e: self._skip_texts())
        self._quellknoepfe[''] = nichts

        self._paragraph(f, t('inj_fremd'), SUB, 9, oben=16)
        self.inj_meldung.pack(fill='x', pady=(14, 0))

    def _skip_texts(self):
        """„Nicht anfassen" — bestätigen, dass nichts geschieht, und nichts tun.

        ⛔ Schreibt **keine** Einstellung. `inj_quelle` zu setzen hieße, auf der
        Seite „Angaben im Spiel" stünde hinterher eine Quelle angewählt, die nie
        geholt wurde — derselbe Fehler, den Haldjas am 25.08.2026 andersherum
        gemeldet hat. Hier ist das Nichtstun die Wahrheit, und die bleibt
        unverändert stehen.
        """
        self._quelle_hervorheben('')
        self.inj_meldung.configure(text=t('inj_nichts_ok'), fg=ACCENT)

    def _quelle_hervorheben(self, quelle):
        """Die gewählte Textquelle sichtbar machen — wie im Reiter.

        ⚠ Auch die **eigene Adresse** und „Nicht anfassen" gehören dazu: Sie
        sind Wahlmöglichkeiten wie die anderen, nur ohne Flagge.
        """
        for name, knopf in (getattr(self, '_quellknoepfe', None) or {}).items():
            an = name == quelle
            try:
                knopf.configure(bg=ACCENT if an else FLAECHE,
                                fg=BG if an else FG)
            except tk.TclError:
                pass                # Seite schon abgebaut

    def _fetch_texts(self, quelle):
        """Herunterladen, einsetzen, Bauplan-Angaben eintragen — in einem Zug."""
        self._quelle_hervorheben(quelle)
        from . import injection, gametext, translation
        # ⚠ Die Wahl **vor** dem Einrichten merken — genau wie auf der
        # Einstellungsseite. Fehlte das hier, holte der Assistent zwar die Texte,
        # aber unter „Angaben im Spiel" stand danach keine der drei Quellen
        # angewählt: Der Assistent schrieb `inj_quelle` nie. Gemeldet von
        # Haldjas, 25.08.2026 — „alle 3 Buttons sind nicht ausgewählt".
        paths.set_setting('inj_quelle', quelle)
        self.inj_meldung.configure(text=t('inj_laeuft'), fg=SUB)
        self.root.update()
        try:
            if quelle == 'original':
                # Kein Download nötig: Die englische Version liegt im Data.p4k
                # des Spielers und wird von dort geholt (0,2 s). Eine fremde
                # vorhandene Datei wird nicht ersetzt — eine vom Werkzeug
                # eingesetzte (StarStrings) schon, siehe `gametext._placed_by_us`.
                sprache_ordner = 'english'
                ok, meldung = gametext.fetch(
                    sprache_ordner,
                    fortschritt=lambda x: (self.inj_meldung.configure(text=x),
                                           self.root.update()))
                if not ok:
                    self.inj_meldung.configure(text=t('inj_fehler', meldung),
                                               fg=GELB)
                    return
                # `g_language` setzt `gametext.fetch()` selbst — dort
                # gehört es hin, damit kein Weg es vergessen kann.
                ziel = translation.target_ini(sprache_ordner)
                translation.note('original', 'Data.p4k')
            else:
                ok, meldung = translation.fetch(
                    quelle, progress=lambda x: (
                        self.inj_meldung.configure(text=x), self.root.update()))
                if not ok:
                    self.inj_meldung.configure(text=t('inj_fehler', meldung),
                                               fg=GELB)
                    return
                sprache_ordner = translation.SOURCES[quelle]['sprache']
                ziel = translation.target_ini(sprache_ordner)

            # ⚠ Wer eine Karte vorher „Angaben in die Auftragstexte schreiben"
            # ausgeschaltet hat, bekommt nur die Übersetzung. Sonst stünde
            # gleich nach dem Ausschalten doch etwas in der Datei.
            if not paths.setting_bool('inj_an', True):
                self.inj_meldung.configure(text=t('as_nur_uebersetzung'),
                                           fg=ACCENT)
                return

            ok, anzahl, meldung = injection.setup(
                ziel, sprache_ordner,
                progress=lambda x: (self.inj_meldung.configure(text=x),
                                       self.root.update()))
            if ok:
                self.inj_meldung.configure(text=t('inj_aktiv', anzahl), fg=ACCENT)
            else:
                self.inj_meldung.configure(text=t('inj_fehler', meldung), fg=GELB)
        except Exception as e:
            self.inj_meldung.configure(text=t('inj_fehler', e), fg=GELB)

    # -------------------------------------------------------- 5. Fertig
    def _step_done(self):
        if self.ohne_spielordner:
            self._step_done_no_game()
            return
        self.titel.configure(text=t('schritt_fertig'))
        f = self._area()
        b = bestand_datei.load()
        self._paragraph(f, t('bauplaene') + ': %d' % bestand_datei.count(b),
                     ACCENT, 15, fett=True)
        self._paragraph(f, t('schritt_fertig_text'), FG, 11, oben=14)
        # ⚠ Ohne führendes Zeichen. Hier stand `☰`, das es seit rc55 gar nicht
        # mehr gibt (durch das Klemmbrett ersetzt) — der Tipp zeigte also auf
        # ein Zeichen, das im Programm nicht vorkam. Die Texte benennen die
        # Symbole jetzt in Worten.
        self._paragraph(f, t('tipp_liste'), SUB, 10, oben=18)
        self._paragraph(f, t('tipp_erneut'), SUB, 10, oben=8)

        self._offer_menu_entry(f)

        knopf = tk.Label(f, text=' %s ' % t('liste_oeffnen'), bg=FLAECHE, fg=FG,
                         font=font(10), cursor='hand2', padx=12, pady=7)
        knopf.pack(anchor='w', pady=(22, 0))
        knopf.bind('<Button-1>', lambda e: self._with_list())

    def _offer_menu_entry(self, flaeche):
        """Unter Linux einen Startmenü-Eintrag anbieten.

        ⚠ Warum überhaupt: Unter Windows legt der Installer alles an. Unter Linux
        lädt man ein AppImage herunter — das liegt dann im Download-Ordner, steht
        in keinem Menü und ist nach einem Neustart erst einmal verschwunden. Wer
        es nicht selbst einträgt, sucht es jedes Mal.

        Der Eintrag ist zugleich die Stelle, auf die sich ein Tastenkürzel legen
        lässt; zusammen mit dem Einzelinstanz-Wächter holt das im Pop-up-Betrieb
        das Overlay zurück.
        """
        from . import desktop_entry
        if not desktop_entry.available() or desktop_entry.exists():
            return
        self._paragraph(flaeche, t('as_menue_frage'), FG, 11, oben=18)
        meldung = tk.Label(flaeche, text='', bg=BG, fg=SUB, font=font(9),
                           anchor='w', justify='left')

        def anlegen(_=None):
            geklappt, wohin = desktop_entry.create()
            meldung.configure(text=(t('as_menue_da') % wohin) if geklappt
                              else t('as_menue_nein') % wohin,
                              fg=ACCENT if geklappt else SUB)

        knopf = tk.Label(flaeche, text=' %s ' % t('as_menue_knopf'), bg=FLAECHE,
                         fg=FG, font=font(10), cursor='hand2', padx=12, pady=6)
        knopf.pack(anchor='w', pady=(8, 0))
        knopf.bind('<Button-1>', anlegen)
        meldung.pack(anchor='w', pady=(6, 0), fill='x')

    def _step_done_no_game(self):
        """Der Abschluss, wenn kein Spielordner eingetragen wurde.

        Ehrlich sagen, was jetzt nicht geht — und was sehr wohl. Ein
        „fertig eingerichtet" wäre gelogen, ein Abbruch wäre unnötig.
        """
        self.titel.configure(text=t('ohne_spiel_titel'))
        f = self._area()
        self._paragraph(f, t('ohne_spiel_text'), FG, 11)
        self._paragraph(f, t('ohne_spiel_wo'), SUB, 10, oben=14)

        knopf = tk.Label(f, text=' %s ' % t('liste_oeffnen'), bg=FLAECHE, fg=FG,
                         font=font(10), cursor='hand2', padx=12, pady=7)
        knopf.pack(anchor='w', pady=(22, 0))
        knopf.bind('<Button-1>', lambda e: self._with_list())

    def _with_list(self):
        self.liste_zeigen = True
        self._close()

    # ------------------------------------------------------------ Steuerung
    def _next(self):
        if self._current() == 'ablage' and not self._apply_storage():
            return                          # ohne beschreibbaren Ordner geht nichts
        if self._current() == 'spiel':
            if not self.gedeutet:
                return                              # ohne Spielordner geht nichts
            paths.set_setting('spiel_ordner', self.gedeutet)
            # Wer erst „ohne Spiel" wählte und dann zurückkam, hat jetzt eins.
            if self.ohne_spielordner:
                self.ohne_spielordner = False
                paths.set_setting('einrichtung_ohne_spiel', False)
        if self.schritt >= len(self._order()):
            # ⚠ Hier wird festgehalten, dass die Einrichtung durch ist — und
            # zwar in einer eigenen Einstellung. Vorher galt die Datei
            # `logstand.json` als Beleg dafür; die ist aber der **Lesestand im
            # Spielprotokoll**, kein Einrichtungsmerkmal, und ein Knopf im
            # Programm löscht sie absichtlich („alte Protokolle neu einlesen").
            # Wer den drückte, bekam beim nächsten Start den ganzen Assistenten
            # vorgesetzt (30.08.2026 gemeldet).
            paths.set_setting('einrichtung_fertig', True)
            self._close()
            return
        self.schritt += 1
        self._draw()

    def _back(self):
        if self.schritt > 1:
            self.schritt -= 1
            self._draw()

    def _cancel(self):
        self.abgebrochen = True
        self._close()

    def _close(self):
        try:
            self.root.quit()
        except Exception:
            pass
        self.root.destroy()

    def run(self):
        self.root.mainloop()
        return not self.abgebrochen


def is_configured():
    """Ist dieses Werkzeug hier schon einmal eingerichtet worden?

    ⚠⚠ **Nicht am Lesestand festmachen.** Bis rc44 galt: keine `logstand.json`,
    also erster Start. Das ist der Lesestand im Spielprotokoll — und unter
    *Erkennung* gibt es einen Knopf, der ihn **mit Absicht** löscht, damit die
    alten Protokolle noch einmal durchgegangen werden. Wer ihn drückte, bekam
    beim nächsten Start den kompletten Einrichtungsassistenten vorgesetzt,
    obwohl nichts fehlte (30.08.2026 gemeldet).

    Der Beleg ist jetzt die Einstellung `einrichtung_fertig`. Wer schon vorher
    eingerichtet war, hat sie noch nicht — deshalb zählt zusätzlich ein
    **eingetragener** Spielordner. Der steht nur in der Einstellungsdatei, wenn
    ihn jemand bestätigt hat (Assistent oder die Seite *Erkennung*); ein bloß
    automatisch gefundener zählt nicht, sonst bekäme ein neuer Nutzer mit
    installiertem Spiel den Assistenten nie zu sehen.
    """
    if paths.setting_bool('einrichtung_fertig', False):
        return True
    return bool(paths.setting('spiel_ordner'))


def needed():
    """Muss der Assistent laufen? Beim ersten Mal, oder wenn das Spiel fehlt.

    ⚠ Wer bewusst ohne Spielordner weitergemacht hat, bekommt ihn nicht bei
    jedem Start erneut vorgesetzt. Vorher hing die Frage allein am gefundenen
    Spiel — auf einem Rechner ohne Star Citizen hieß das: jedes Mal wieder von
    vorn, und über die zweite Seite kam man nie hinaus.
    """
    # ⚠⚠ **Zuerst: Lässt sich überhaupt speichern?** Sonst merkt sich das
    # Programm nichts — auch nicht, dass es eingerichtet ist — und der Nutzer
    # erfährt es nie (Parsul, 29.09.2026: Datenordner in OneDrive, von Windows
    # gesperrt). Dann führt die Einrichtung zum Schritt „Datenordner".
    if not storage_writable():
        return True
    if paths.setting_bool('einrichtung_ohne_spiel', False):
        return False
    return not is_configured() or not paths.game_folder()


def storage_writable():
    """Kann Verse-Kit in seinen Datenordner schreiben?"""
    try:
        return paths.storage_status(paths.app_folder())[0]
    except Exception as exc:
        errors.record('wizard.ablage_pruefen', exc)
        return False


def _overlay():
    from . import overlay
    return overlay.OVERLAY_CONTROL[0]


def _apply_opacity():
    """Die gespeicherte Durchsichtigkeit ans laufende Overlay geben."""
    control = _overlay()
    if control is None:
        return
    try:
        control.root.attributes(
            '-alpha', paths.setting_int('deckkraft_prozent', 93, 30, 100) / 100.0)
    except Exception as exc:
        errors.record('wizard.opacity', exc)


def apply_changes(changed):
    """Was im Assistenten umgestellt wurde, im laufenden Programm nachziehen.

    ⚠ **Erst nach dem Schließen.** Eine neue Schriftgröße baut das große
    Fenster neu auf (`MainWindow.rebuild` zerstört alle Kinder seiner Wurzel) —
    und der Assistent ist eines davon, wenn er von dort geöffnet wurde.

    Beim allerersten Start gibt es noch kein Overlay; es liest beim Aufbau
    ohnehin, was gespeichert ist.
    """
    control = _overlay()
    if not changed or control is None:
        return
    window = getattr(control, '_fenster', None)
    if 'schriftgroesse' in changed:
        stufe = paths.setting('schriftgroesse') or 'normal'
        try:
            if window is not None:
                # Zieht über `on_font_change` auch das Overlay mit.
                window.set_font_size(stufe)
            else:
                control.schriftgroesse_anwenden(stufe)
        except Exception as exc:
            errors.record('wizard.font_size', exc)
    elif 'spielzeit_zeigen' in changed and window is not None:
        # Die Kopfzeile wird einmal zusammengesetzt — ohne Neuaufbau bliebe
        # die Wahl bis zum Neustart unsichtbar.
        try:
            window.root.after(60, window.rebuild)
        except Exception as exc:
            errors.record('wizard.play_time', exc)
    if 'deckkraft_prozent' in changed:
        _apply_opacity()
    # Bei offenem Fenster greift der Modus beim Schließen (`_liste_zu`) —
    # sonst verschwände das Overlay, während man noch davorsteht.
    if 'overlay_modus' in changed and window is None:
        try:
            control.verhalten_anwenden()
        except Exception as exc:
            errors.record('wizard.overlay_mode', exc)


def start(eltern=None):
    """Assistent durchlaufen. Gibt (fertig, liste_zeigen) zurück.

    Auch abgebrochen wird nachgezogen: Jede Wahl ist beim Klick gespeichert.
    """
    a = Wizard(eltern)
    fertig = a.run()
    apply_changes(a.changed)
    return fertig, a.liste_zeigen


if __name__ == '__main__':
    print('nötig:', needed())
    print('Ergebnis:', start())
