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
Der Einrichtungsassistent — vier Schritte, jederzeit wiederholbar.

Läuft beim ersten Start von allein und ist danach über einen Knopf erreichbar.
Das ist Absicht: Wer sich mit Rechnern nicht auskennt, soll etwas nachstellen
können, ohne zu wissen, in welchem Menü es steckt. Ein Assistent führt; ein
Einstellungsfenster setzt voraus, dass man weiß, wonach man sucht.

    1. Sprache      zuerst, damit der Rest lesbar ist
    2. Star Citizen die eine Angabe, ohne die nichts geht
    3. Nachlesen    hier bekommt der Spieler seinen Bestand geschenkt
    4. Fertig       was jetzt passiert, und wo die Liste steckt

**Erst arbeitet das Programm, dann der Mensch.** Schritt 3 läuft von selbst und
holt aus den aufgehobenen Logs alles, was noch da ist. Von Hand nachtragen soll
nur, wer muss — und nur das, was wirklich keine Logdatei mehr hergibt.
"""
import os
import tkinter as tk

from . import fehler
from . import bestand as bestand_datei
from . import logquelle, pfade, sprache
from .sprache import t, fenstertitel

BG      = '#10141c'
FLAECHE = '#161c28'
BAR     = '#1b2230'
LINIE   = '#2a3345'   # Rand runder Kästen und Felder — überall dieselbe Linie
FG      = '#e6edf3'
SUB     = '#8b98a5'
ACCENT  = '#9ce430'
GELB    = '#d8a03a'

SCHRITTE = 5


def schrift(groesse, fett=False, unterstrichen=False):
    """Die Schrift des Assistenten.

    `unterstrichen` ist für Textlinks — im Haus die Auszeichnung dafür, dass
    man auf etwas klicken kann. Ohne sie sieht ein Textlink aus wie ein
    Hinweis und wird übersehen.
    """
    fam = 'Segoe UI' if pfade.WINDOWS else 'Helvetica'
    teile = [fam, groesse]
    stil = ' '.join(x for x, an in (('bold', fett),
                                    ('underline', unterstrichen)) if an)
    teile.append(stil or 'normal')
    return tuple(teile)


def mono(groesse):
    return ('Consolas' if pfade.WINDOWS else 'Menlo', groesse)


class Assistent:
    def __init__(self, eltern=None, nur_wenn_noetig=False):
        self.abgebrochen = False
        self.liste_zeigen = False
        self.nachlese_gelaufen = False
        self.schritt = 1
        self.gedeutet = None

        self.root = tk.Toplevel(eltern) if eltern else tk.Tk()
        self.root.title(fenstertitel('SC BP Watcher — ' + t('assistent')))
        self.root.configure(bg=BG)
        # ⚠⚠ **Mit Position, nicht nur mit Größe.** Ein `geometry` ohne
        # `+x+y` überlässt die Platzierung dem Fenstermanager — und der
        # weiß nichts vom Hauptfenster. Auf mehreren Bildschirmen landete
        # so am 06.09.2026 ein Fenster außerhalb des sichtbaren Bereichs;
        # weil es modal war, ließ sich das Programm nicht einmal beenden.
        #
        # `mittig_ueber` setzt beides und fällt auf die reine Größe
        # zurück, wenn es kein Elternfenster gibt (eigenständiger Start).
        from .hauptfenster import mittig_ueber
        if eltern is None or not mittig_ueber(self.root, eltern, 640, 520):
            self.root.geometry('640x520')
        self.root.protocol('WM_DELETE_WINDOW', self._abbrechen)

        self.kopf = tk.Frame(self.root, bg=BAR)
        self.kopf.pack(fill='x')
        self.titel = tk.Label(self.kopf, text='', bg=BAR, fg=FG,
                              font=schrift(13, True))
        self.titel.pack(side='left', padx=18, pady=12)
        self.zaehler = tk.Label(self.kopf, text='', bg=BAR, fg=SUB,
                                font=schrift(9))
        self.zaehler.pack(side='right', padx=18)

        self.buehne = tk.Frame(self.root, bg=BG)
        self.buehne.pack(fill='both', expand=True)

        fuss = tk.Frame(self.root, bg=BAR)
        fuss.pack(fill='x', side='bottom')
        self.zurueck = tk.Label(fuss, text=t('zurueck'), bg=BAR, fg=SUB,
                                font=schrift(10), cursor='hand2', padx=16, pady=13)
        self.zurueck.pack(side='left')
        self.zurueck.bind('<Button-1>', lambda e: self._zurueck())
        self.weiter = tk.Label(fuss, text='', bg=ACCENT, fg=BG,
                               font=schrift(10, True), cursor='hand2',
                               padx=14, pady=7)
        self.weiter.pack(side='right', padx=16, pady=6)
        self.weiter.bind('<Button-1>', lambda e: self._weiter())
        self.root.bind('<Return>', lambda e: self._weiter())

        self._zeichnen()

    # ------------------------------------------------------------ Gerüst
    def _leeren(self):
        for kind in self.buehne.winfo_children():
            kind.destroy()

    def _flaeche(self):
        f = tk.Frame(self.buehne, bg=BG, padx=24, pady=20)
        f.pack(fill='both', expand=True)
        return f

    def _absatz(self, eltern, text, farbe=SUB, groesse=10, oben=0, fett=False):
        tk.Label(eltern, text=text, bg=BG, fg=farbe, font=schrift(groesse, fett),
                 anchor='w', justify='left', wraplength=560).pack(
                     fill='x', pady=(oben, 0))

    def _zeichnen(self):
        self._leeren()
        self.zaehler.configure(text=t('schritt_von', self.schritt, SCHRITTE))
        self.zurueck.configure(fg=SUB if self.schritt > 1 else BG,
                               cursor='hand2' if self.schritt > 1 else '')
        self.weiter.configure(text='  %s  ' % (t('fertig') if self.schritt == SCHRITTE
                                               else t('weiter')),
                              bg=ACCENT, fg=BG, cursor='hand2')
        {1: self._schritt_sprache, 2: self._schritt_spiel,
         3: self._schritt_lesen, 4: self._schritt_texte,
         5: self._schritt_fertig}[self.schritt]()

    # ------------------------------------------------------- 1. Sprache
    def _schritt_sprache(self):
        self.titel.configure(text=t('schritt_sprache'))
        f = self._flaeche()
        self._absatz(f, t('schritt_sprache_text'), FG, 11)
        reihe = tk.Frame(f, bg=BG)
        reihe.pack(fill='x', pady=(20, 0))
        aktiv = sprache.gewaehlt()
        for wert, text in (('auto', t('sprache_auto')), ('de', 'Deutsch'),
                           ('en', 'English')):
            an = wert == aktiv
            k = tk.Label(reihe, text=' %s ' % text, bg=ACCENT if an else FLAECHE,
                         fg=BG if an else FG, font=schrift(10), cursor='hand2',
                         padx=12, pady=8)
            k.pack(side='left', padx=(0, 8))
            k.bind('<Button-1>', lambda e, w=wert: self._sprache(w))

    def _sprache(self, wert):
        pfade.einstellung_setzen('sprache', wert)
        sprache.setzen(wert)
        self.root.title(fenstertitel('SC BP Watcher — ' + t('assistent')))
        self._zeichnen()

    # -------------------------------------------------- 2. Star Citizen
    def _schritt_spiel(self):
        self.titel.configure(text=t('schritt_spiel'))
        f = self._flaeche()
        gefunden = pfade.spiel_ordner()
        self._absatz(f, t('schritt_spiel_text'), FG, 11)
        self._absatz(f, t('schritt_spiel_hilfe'), SUB, 10, oben=10)

        self.pfad = tk.StringVar(value=gefunden or '')
        self.pfad.trace_add('write', lambda *_: self._pfad_pruefen())
        zeile = tk.Frame(f, bg=BG)
        zeile.pack(fill='x', pady=(18, 0))
        from .hauptfenster import rundes_feld
        feld = rundes_feld(zeile, self.pfad, mono(10), FLAECHE, LINIE, ACCENT, FG)
        feld.halter.pack(side='left', fill='x', expand=True, padx=(0, 8))
        knopf = tk.Label(zeile, text=' %s ' % t('durchsuchen'), bg=BAR, fg=FG,
                         font=schrift(10), cursor='hand2', padx=8, pady=6)
        knopf.pack(side='right')
        knopf.bind('<Button-1>', lambda e: self._waehlen())

        self.rueckmeldung = tk.Label(f, text='', bg=BG, fg=SUB, font=schrift(10),
                                     anchor='w', justify='left', wraplength=560)
        self.rueckmeldung.pack(fill='x', pady=(10, 0))

        if not gefunden:
            self._absatz(f, t('gesucht_wurde_hier'), SUB, 9, oben=16)
            for ort in pfade.gesuchte_spielorte(4):
                tk.Label(f, text=ort, bg=BG, fg=SUB, font=mono(8), anchor='w',
                         justify='left', wraplength=560).pack(fill='x')

            # ⚠ Ohne diesen Ausweg sitzt fest, wer Star Citizen nicht auf
            # diesem Rechner hat: Der Weiter-Knopf bleibt grau, und weil
            # `noetig()` am fehlenden Spielordner hängt, kommt der Assistent
            # bei jedem Start wieder. Genau so ging es beim Ansehen auf einem
            # Zweitrechner — man kam nie über diese Seite hinaus.
            # ⚠ Als schlichter grauer Text sieht das aus wie ein Hinweis, nicht
            # wie etwas zum Anklicken — genau so wurde er beim Ausprobieren
            # übersehen. Deshalb unterstrichen und in der Akzentfarbe: Das ist
            # im Haus die Auszeichnung für „hier kann man klicken".
            ohne = tk.Label(f, text='→  ' + t('ohne_spiel'), bg=BG, fg=ACCENT,
                            font=schrift(10, unterstrichen=True),
                            cursor='hand2')
            ohne.pack(anchor='w', pady=(18, 0))
            ohne.bind('<Button-1>', lambda e: self._ohne_spiel())
            ohne.bind('<Enter>', lambda e: ohne.configure(fg=FG))
            ohne.bind('<Leave>', lambda e: ohne.configure(fg=ACCENT))
        self._pfad_pruefen()

    def _ohne_spiel(self):
        """Weiter ohne Spielordner — bewusst und einmalig gemerkt."""
        self.ohne_spielordner = True
        pfade.einstellung_setzen('einrichtung_ohne_spiel', True)
        self.schritt = SCHRITTE
        self._zeichnen()

    def _waehlen(self):
        # ⚠ Siehe `dateiwahl`: Der Tk-Dialog wäre hier besonders unglücklich —
        # das ist der allererste Bildschirm, den ein neuer Nutzer sieht.
        from . import dateiwahl
        ordner = dateiwahl.ordner_waehlen(t('spielordner'))
        if ordner:
            self.pfad.set(ordner)

    def _pfad_pruefen(self):
        eingabe = self.pfad.get().strip()
        self.gedeutet = pfade.spielordner_deuten(eingabe) if eingabe else None
        if not eingabe:
            self.rueckmeldung.configure(text='', fg=SUB)
        elif self.gedeutet:
            text = t('log_gefunden')
            if self.gedeutet.rstrip('/\\') != eingabe.rstrip('/\\'):
                text += '\n' + t('ordner_gedeutet', self.gedeutet)
            self.rueckmeldung.configure(text=text, fg=ACCENT)
        else:
            self.rueckmeldung.configure(text=t('keine_log_darin'), fg=GELB)
        an = bool(self.gedeutet)
        self.weiter.configure(bg=ACCENT if an else BAR, fg=BG if an else SUB,
                              cursor='hand2' if an else '')

    # ----------------------------------------------------- 3. Nachlesen
    def _schritt_lesen(self):
        self.titel.configure(text=t('schritt_lesen'))
        f = self._flaeche()
        self._absatz(f, t('schritt_lesen_text'), FG, 11)
        self.ergebnis = tk.Label(f, text=t('lese_logs'), bg=BG, fg=SUB,
                                 font=schrift(11), anchor='w', justify='left',
                                 wraplength=560)
        self.ergebnis.pack(fill='x', pady=(20, 0))
        self.luecke = tk.Label(f, text='', bg=BG, fg=GELB, font=schrift(10),
                               anchor='w', justify='left', wraplength=560)
        self.luecke.pack(fill='x', pady=(12, 0))
        self.root.update()
        if not self.nachlese_gelaufen:
            self._nachlesen()

    def _nachlesen(self):
        """Läuft von selbst — hier muss niemand etwas tun."""
        self.nachlese_gelaufen = True
        fehler.spur('Assistent: Logs nachlesen beginnt')
        try:
            anzahl_dateien = len(pfade.log_sicherungen())
            if anzahl_dateien:
                self.ergebnis.configure(text=t('lese_logs_n', anzahl_dateien))
                self.root.update()
            funde, bericht = logquelle.nachlesen(logquelle.Lesestand())
            b = bestand_datei.laden()
            neu = 0
            for name, _zusatz in funde:
                if bestand_datei.hinzufuegen(b, name, 'nachlese'):
                    neu += 1
            if neu:
                bestand_datei.speichern(b)
            fehler.spur('Assistent: nachgelesen (%d neu)' % neu)
            self.ergebnis.configure(
                text=t('nachgelesen_gross', neu, bericht.get('dateien', 0)),
                fg=FG, font=schrift(12))
            if bericht.get('luecke') and bericht.get('grund'):
                self.luecke.configure(text=bericht['grund'] + '\n\n'
                                      + t('nachtragen_hinweis'))
        except Exception:
            # Ein Fehler hier darf die Einrichtung nicht abbrechen — der Watcher
            # läuft auch ohne Vorgeschichte weiter.
            self.ergebnis.configure(text=t('nachgelesen_gross', 0, 0), fg=SUB)

    # ------------------------------------------- 4. Bauplan-Angaben im Spiel
    def _schritt_texte(self):
        """Die einzige Stelle, an der dieses Werkzeug etwas am Spiel verändert —
        deshalb wird hier **gefragt**, nicht stillschweigend gemacht.

        Drei Wege plus „jetzt nicht". Voreingestellt ist nichts: Wer weiterklickt,
        ohne etwas zu wählen, behält seine Installation unverändert."""
        self.titel.configure(text=t('schritt_spiel_texte'))
        f = self._flaeche()
        self._absatz(f, t('inj_text'), FG, 11)
        self._absatz(f, t('inj_wie'), SUB, 10, oben=10)

        self.inj_meldung = tk.Label(f, text='', bg=BG, fg=SUB, font=schrift(10),
                                    anchor='w', justify='left', wraplength=560)

        for schluessel, quelle in (('inj_quelle_de', 'deutsch'),
                                   ('inj_quelle_ss', 'starstrings'),
                                   ('inj_quelle_orig', 'original')):
            k = tk.Label(f, text='  %s  ' % t(schluessel), bg=FLAECHE, fg=FG,
                         font=schrift(11), cursor='hand2', padx=10, pady=8)
            k.pack(anchor='w', pady=(14 if schluessel.endswith('_de') else 6, 0))
            k.bind('<Button-1>', lambda e, q=quelle: self._texte_holen(q))

        self._absatz(f, t('inj_fremd'), SUB, 9, oben=16)
        self.inj_meldung.pack(fill='x', pady=(14, 0))

    def _texte_holen(self, quelle):
        """Herunterladen, einsetzen, Bauplan-Angaben eintragen — in einem Zug."""
        from . import injektion, spieltexte, uebersetzung
        # ⚠ Die Wahl **vor** dem Einrichten merken — genau wie auf der
        # Einstellungsseite. Fehlte das hier, holte der Assistent zwar die Texte,
        # aber unter „Angaben im Spiel" stand danach keine der drei Quellen
        # angewählt: Der Assistent schrieb `inj_quelle` nie. Gemeldet von
        # Haldjas, 25.08.2026 — „alle 3 Buttons sind nicht ausgewählt".
        pfade.einstellung_setzen('inj_quelle', quelle)
        self.inj_meldung.configure(text=t('inj_laeuft'), fg=SUB)
        self.root.update()
        try:
            if quelle == 'original':
                # Kein Download nötig: Die englische Version liegt im Data.p4k
                # des Spielers und wird von dort geholt (0,2 s). Eine bereits
                # vorhandene Datei wird nicht ersetzt.
                sprache_ordner = 'english'
                ok, meldung = spieltexte.holen(
                    sprache_ordner,
                    fortschritt=lambda x: (self.inj_meldung.configure(text=x),
                                           self.root.update()))
                if not ok:
                    self.inj_meldung.configure(text=t('inj_fehler', meldung),
                                               fg=GELB)
                    return
                # `g_language` setzt `spieltexte.holen()` selbst — dort
                # gehört es hin, damit kein Weg es vergessen kann.
                ziel = uebersetzung.ziel_ini(sprache_ordner)
                uebersetzung.vermerken('original', 'Data.p4k')
            else:
                ok, meldung = uebersetzung.holen(
                    quelle, fortschritt=lambda x: (
                        self.inj_meldung.configure(text=x), self.root.update()))
                if not ok:
                    self.inj_meldung.configure(text=t('inj_fehler', meldung),
                                               fg=GELB)
                    return
                sprache_ordner = uebersetzung.QUELLEN[quelle]['sprache']
                ziel = uebersetzung.ziel_ini(sprache_ordner)

            ok, anzahl, meldung = injektion.einrichten(
                ziel, sprache_ordner,
                fortschritt=lambda x: (self.inj_meldung.configure(text=x),
                                       self.root.update()))
            if ok:
                self.inj_meldung.configure(text=t('inj_aktiv', anzahl), fg=ACCENT)
            else:
                self.inj_meldung.configure(text=t('inj_fehler', meldung), fg=GELB)
        except Exception as e:
            self.inj_meldung.configure(text=t('inj_fehler', e), fg=GELB)

    # -------------------------------------------------------- 5. Fertig
    def _schritt_fertig(self):
        if getattr(self, 'ohne_spielordner', False):
            self._schritt_fertig_ohne_spiel()
            return
        self.titel.configure(text=t('schritt_fertig'))
        f = self._flaeche()
        b = bestand_datei.laden()
        self._absatz(f, t('bauplaene') + ': %d' % bestand_datei.anzahl(b),
                     ACCENT, 15, fett=True)
        self._absatz(f, t('schritt_fertig_text'), FG, 11, oben=14)
        # ⚠ Ohne führendes Zeichen. Hier stand `☰`, das es seit rc55 gar nicht
        # mehr gibt (durch das Klemmbrett ersetzt) — der Tipp zeigte also auf
        # ein Zeichen, das im Programm nicht vorkam. Die Texte benennen die
        # Symbole jetzt in Worten.
        self._absatz(f, t('tipp_liste'), SUB, 10, oben=18)
        self._absatz(f, t('tipp_erneut'), SUB, 10, oben=8)

        self._menueeintrag_anbieten(f)

        knopf = tk.Label(f, text=' %s ' % t('liste_oeffnen'), bg=FLAECHE, fg=FG,
                         font=schrift(10), cursor='hand2', padx=12, pady=7)
        knopf.pack(anchor='w', pady=(22, 0))
        knopf.bind('<Button-1>', lambda e: self._mit_liste())

    def _menueeintrag_anbieten(self, flaeche):
        """Unter Linux einen Startmenü-Eintrag anbieten.

        ⚠ Warum überhaupt: Unter Windows legt der Installer alles an. Unter Linux
        lädt man ein AppImage herunter — das liegt dann im Download-Ordner, steht
        in keinem Menü und ist nach einem Neustart erst einmal verschwunden. Wer
        es nicht selbst einträgt, sucht es jedes Mal.

        Der Eintrag ist zugleich die Stelle, auf die sich ein Tastenkürzel legen
        lässt; zusammen mit dem Einzelinstanz-Wächter holt das im Pop-up-Betrieb
        das Overlay zurück.
        """
        from . import verknuepfung
        if not verknuepfung.moeglich() or verknuepfung.vorhanden():
            return
        self._absatz(flaeche, t('as_menue_frage'), FG, 11, oben=18)
        meldung = tk.Label(flaeche, text='', bg=BG, fg=SUB, font=schrift(9),
                           anchor='w', justify='left')

        def anlegen(_=None):
            geklappt, wohin = verknuepfung.anlegen()
            meldung.configure(text=(t('as_menue_da') % wohin) if geklappt
                              else t('as_menue_nein') % wohin,
                              fg=ACCENT if geklappt else SUB)

        knopf = tk.Label(flaeche, text=' %s ' % t('as_menue_knopf'), bg=FLAECHE,
                         fg=FG, font=schrift(10), cursor='hand2', padx=12, pady=6)
        knopf.pack(anchor='w', pady=(8, 0))
        knopf.bind('<Button-1>', anlegen)
        meldung.pack(anchor='w', pady=(6, 0), fill='x')

    def _schritt_fertig_ohne_spiel(self):
        """Der Abschluss, wenn kein Spielordner eingetragen wurde.

        Ehrlich sagen, was jetzt nicht geht — und was sehr wohl. Ein
        „fertig eingerichtet" wäre gelogen, ein Abbruch wäre unnötig.
        """
        self.titel.configure(text=t('ohne_spiel_titel'))
        f = self._flaeche()
        self._absatz(f, t('ohne_spiel_text'), FG, 11)
        self._absatz(f, t('ohne_spiel_wo'), SUB, 10, oben=14)

        knopf = tk.Label(f, text=' %s ' % t('liste_oeffnen'), bg=FLAECHE, fg=FG,
                         font=schrift(10), cursor='hand2', padx=12, pady=7)
        knopf.pack(anchor='w', pady=(22, 0))
        knopf.bind('<Button-1>', lambda e: self._mit_liste())

    def _mit_liste(self):
        self.liste_zeigen = True
        self._beenden()

    # ------------------------------------------------------------ Steuerung
    def _weiter(self):
        if self.schritt == 2 and not self.gedeutet:
            return                                  # ohne Spielordner geht nichts
        if self.schritt == 2:
            pfade.einstellung_setzen('spiel_ordner', self.gedeutet)
        if self.schritt >= SCHRITTE:
            # ⚠ Hier wird festgehalten, dass die Einrichtung durch ist — und
            # zwar in einer eigenen Einstellung. Vorher galt die Datei
            # `logstand.json` als Beleg dafür; die ist aber der **Lesestand im
            # Spielprotokoll**, kein Einrichtungsmerkmal, und ein Knopf im
            # Programm löscht sie absichtlich („alte Protokolle neu einlesen").
            # Wer den drückte, bekam beim nächsten Start den ganzen Assistenten
            # vorgesetzt (30.08.2026 gemeldet).
            pfade.einstellung_setzen('einrichtung_fertig', True)
            self._beenden()
            return
        self.schritt += 1
        self._zeichnen()

    def _zurueck(self):
        if self.schritt > 1:
            self.schritt -= 1
            self._zeichnen()

    def _abbrechen(self):
        self.abgebrochen = True
        self._beenden()

    def _beenden(self):
        try:
            self.root.quit()
        except Exception:
            pass
        self.root.destroy()

    def durchlaufen(self):
        self.root.mainloop()
        return not self.abgebrochen


def eingerichtet():
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
    if pfade.einstellung_wahrheit('einrichtung_fertig', False):
        return True
    return bool(pfade.einstellung('spiel_ordner'))


def noetig():
    """Muss der Assistent laufen? Beim ersten Mal, oder wenn das Spiel fehlt.

    ⚠ Wer bewusst ohne Spielordner weitergemacht hat, bekommt ihn nicht bei
    jedem Start erneut vorgesetzt. Vorher hing die Frage allein am gefundenen
    Spiel — auf einem Rechner ohne Star Citizen hieß das: jedes Mal wieder von
    vorn, und über die zweite Seite kam man nie hinaus.
    """
    if pfade.einstellung_wahrheit('einrichtung_ohne_spiel', False):
        return False
    return not eingerichtet() or not pfade.spiel_ordner()


def starten(eltern=None):
    """Assistent durchlaufen. Gibt (fertig, liste_zeigen) zurück."""
    a = Assistent(eltern)
    fertig = a.durchlaufen()
    return fertig, a.liste_zeigen


if __name__ == '__main__':
    print('nötig:', noetig())
    print('Ergebnis:', starten())
