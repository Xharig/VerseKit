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
Die Einstellungen — sichtbar, statt in einer Datei.

Bis v2.0.0-rc3 gab es dieses Fenster nicht. Sprache und Spielordner ließen sich
nur über den **Einrichtungsassistenten** ändern, die übrigen drei Felder gar
nicht — für die musste man `einstellungen.json` von Hand bearbeiten und das
Programm neu starten. Gemeldet als „ich finde den Einstellungs-Button gar
nicht", und das zu Recht: Niemand kommt darauf, dass „Einrichtung wiederholen"
der Weg zur Spracheinstellung ist.

Das widersprach auch der eigenen Projektregel — *fehlt eine Angabe, wird
gefragt, nie „bearbeite diese Datei und starte neu"*.

Aufbau: fünf Felder, jedes mit **einem Satz Erklärung darunter**. Die Erklärungen
standen vorher schon in der JSON-Datei, weil man dort keine ausgegraute
Beschriftung hat — hier stehen sie da, wo sie hingehören.

Der Assistent bleibt daneben bestehen. Er führt Schritt für Schritt durch die
Ersteinrichtung; dieses Fenster ist zum gezielten Nachstellen einer Sache.
"""
import os
import tkinter as tk

from . import injektion
from . import pfade
from . import spieltexte
from . import sprache
from . import uebersetzung
from .sprache import t, fenstertitel

BG      = '#10141c'
FLAECHE = '#161c28'
BAR     = '#1b2230'
LINIE   = '#2a3345'   # Rand runder Kästen und Felder — überall dieselbe Linie
FG      = '#e6edf3'
SUB     = '#8b98a5'
ACCENT  = '#9ce430'
ROT     = '#e05252'

INTERVALL_MIN, INTERVALL_MAX = 1, 60


def schrift(groesse, fett=False):
    return ('Segoe UI', groesse, 'bold' if fett else 'normal')


class Einstellungsfenster:
    """Ein Fenster, kein Dauerzustand — beim Schließen ist es weg."""

    def __init__(self, eltern=None, rahmen=None):
        """Ohne `rahmen` ein eigenes Fenster; mit `rahmen` liefert es nur seine
        Bausteine, die das Hauptfenster auf die Reiter verteilt."""
        self.eingebettet = rahmen is not None
        self.beim_sprachwechsel = None      # setzt das Hauptfenster
        if self.eingebettet:
            self.root = rahmen
            self.root.configure(bg=BG)
        else:
            self.root = tk.Toplevel(eltern) if eltern else tk.Tk()
            self.root.title(fenstertitel(t('titel_einstellungen')))
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
            if eltern is None or not mittig_ueber(self.root, eltern, 660, 900):
                self.root.geometry('660x900')

        # Werte laden. Leere Felder heißen „selbst suchen" — das bleibt so,
        # ein leeres Feld ist hier kein Fehler.
        self.sprache_wahl = tk.StringVar(value=pfade.einstellungen().get('sprache')
                                         or 'auto')
        self.spiel = tk.StringVar(value=pfade.einstellungen().get('spiel_ordner') or '')
        self.launcher = tk.StringVar(value=pfade.einstellungen().get('launcher_ordner')
                                     or '')
        self.intervall = tk.StringVar(
            value=str(pfade.einstellung_zahl('pruefintervall_sekunden', 3,
                                             INTERVALL_MIN, INTERVALL_MAX)))
        self.ton = tk.BooleanVar(value=pfade.einstellung_wahrheit('signalton', True))
        self.deckkraft = tk.IntVar(
            value=pfade.einstellung_zahl('deckkraft_prozent', 93, 30, 100))

        if self.eingebettet:
            return                     # die Bausteine holt sich `seiten.py`

        self._kopf()
        # ⚠ Reihenfolge ist entscheidend: Der Fuß mit dem Speichern-Knopf wird
        # **vor** dem Inhalt gepackt. Sonst schiebt ein zu langer Inhalt ihn aus
        # dem Fenster — genau das ist passiert: Der Knopf war unsichtbar, bis man
        # das Fenster von Hand größer zog, und niemand kam auf die Idee, dass
        # unten noch etwas fehlt.
        self._fuss()
        flaeche = self._rollflaeche()

        self._sprachwahl(flaeche)
        self._ordnerfeld(flaeche, t('e_spiel'), t('e_spiel_hilfe'), self.spiel)
        self._ordnerfeld(flaeche, t('e_launcher'), t('e_launcher_hilfe'),
                         self.launcher)
        self._intervallfeld(flaeche)
        self._tonfeld(flaeche)
        self._deckkraftfeld(flaeche)
        self._injektionsfeld(flaeche)

    # ------------------------------------------------------------- Bausteine
    def _kopf(self):
        bar = tk.Frame(self.root, bg=BAR)
        bar.pack(fill='x')
        tk.Label(bar, text=t('einstellungen'), bg=BAR, fg=FG,
                 font=schrift(12, True)).pack(side='left', padx=18, pady=10)
        # Kein eigenes ✕ — siehe Bestandsfenster: Die Systemtitelleiste hat eins.

    def _rollflaeche(self):
        """Der scrollbare Innenbereich.

        Das Fenster ist in der Höhe begrenzt, der Inhalt wächst mit jeder neuen
        Einstellung. Ohne Rollbalken wäre die letzte Einstellung irgendwann
        unerreichbar — und man merkt es nicht, weil nichts darauf hinweist."""
        rahmen = tk.Frame(self.root, bg=BG)
        rahmen.pack(fill='both', expand=True)
        self.leinwand = tk.Canvas(rahmen, bg=BG, highlightthickness=0)
        from .hauptfenster import rundleiste
        rolle = rundleiste(rahmen, self.leinwand, grund=BG)
        innen = tk.Frame(self.leinwand, bg=BG)
        innen.bind('<Configure>', lambda e: self.leinwand.configure(
            scrollregion=self.leinwand.bbox('all')))
        self._innen_id = self.leinwand.create_window((0, 0), window=innen,
                                                     anchor='nw')
        # Die Innenfläche muss so breit sein wie das Fenster, sonst kleben die
        # Beschriftungen links und der Umbruch stimmt nicht.
        self.leinwand.bind('<Configure>', lambda e: self.leinwand.itemconfigure(
            self._innen_id, width=e.width))
        self.leinwand.configure(yscrollcommand=rolle.set)
        self.leinwand.pack(side='left', fill='both', expand=True, padx=(20, 0))
        rolle.pack(side='right', fill='y')

        for ziel in (self.root, self.leinwand, innen):
            ziel.bind_all('<Button-4>', self._rollen, add='+')
            ziel.bind_all('<Button-5>', self._rollen, add='+')
            ziel.bind_all('<MouseWheel>', self._rollen, add='+')
        return innen

    def _rollen(self, ereignis):
        """Mausrad — unter Linux kommen 4/5, unter Windows ein Delta."""
        try:
            if getattr(ereignis, 'num', None) == 4:
                richtung = -1
            elif getattr(ereignis, 'num', None) == 5:
                richtung = 1
            else:
                richtung = -1 if ereignis.delta > 0 else 1
            self.leinwand.yview_scroll(richtung, 'units')
        except tk.TclError:
            pass

    def _titel(self, eltern, text, hilfe):
        tk.Label(eltern, text=text, bg=BG, fg=FG, font=schrift(11, True),
                 anchor='w').pack(fill='x', pady=(16, 2))
        tk.Label(eltern, text=hilfe, bg=BG, fg=SUB, font=schrift(9),
                 anchor='w', justify='left', wraplength=590).pack(fill='x',
                                                                  pady=(0, 6))

    def _sprachwahl(self, eltern):
        self._titel(eltern, t('e_sprache'), t('e_sprache_hilfe'))
        reihe = tk.Frame(eltern, bg=BG)
        reihe.pack(fill='x')
        self.sprach_knoepfe = {}
        for wert, text in (('auto', t('e_sprache_auto')),
                           ('de', 'Deutsch'), ('en', 'English')):
            k = tk.Label(reihe, text=' %s ' % text, bg=FLAECHE, fg=SUB,
                         font=schrift(10), cursor='hand2', padx=10, pady=6)
            k.pack(side='left', padx=(0, 6))
            k.bind('<Button-1>', lambda e, w=wert: self._sprache_waehlen(w))
            self.sprach_knoepfe[wert] = k
        self._sprach_knoepfe_faerben()

    def _sprache_waehlen(self, wert):
        """Sofort umschalten, nicht erst beim Speichern.

        Wer eine Sprache wählt, will sehen, ob es die richtige ist. Gespeichert
        wird trotzdem erst mit dem Knopf — sonst könnte man nichts ausprobieren,
        ohne es zu übernehmen."""
        self.sprache_wahl.set(wert)
        sprache.setzen(wert)
        # ⚠ Seit v3.0.0 gibt es keinen Speichern-Knopf mehr — also muss die Wahl
        # hier festgehalten werden. Vorher wurde nur der laufende Betrieb
        # umgestellt: Die Oberfläche sprach Deutsch, die Markierung stand
        # weiter auf der alten Sprache, und nach einem Neustart war alles beim
        # Alten. Das sah aus wie ein Anzeigefehler, war aber verlorene Eingabe.
        pfade.einstellung_setzen('sprache', wert)
        self._sprach_knoepfe_faerben()
        self._neu_beschriften()

    def _sprach_knoepfe_faerben(self):
        # Im Hauptfenster zeichnet `seiten.py` die Sprachwahl selbst — dort gibt
        # es diese Knöpfe gar nicht. Ohne die Prüfung stirbt der Sprachwechsel
        # mit einem Attributfehler, und zwar mitten im Umschalten.
        for wert, knopf in getattr(self, 'sprach_knoepfe', {}).items():
            an = wert == self.sprache_wahl.get()
            knopf.configure(fg=BG if an else SUB, bg=ACCENT if an else FLAECHE)

    def _ordnerfeld(self, eltern, titel, hilfe, variable):
        self._titel(eltern, titel, hilfe)
        reihe = tk.Frame(eltern, bg=BG)
        reihe.pack(fill='x')
        from .hauptfenster import rundes_feld
        feld = rundes_feld(reihe, variable, schrift(10), FLAECHE, LINIE, ACCENT, FG)
        feld.halter.pack(side='left', fill='x', expand=True, padx=(0, 8))
        knopf = tk.Label(reihe, text=' %s ' % t('e_durchsuchen'), bg=FLAECHE,
                         fg=FG, font=schrift(10), cursor='hand2', padx=8, pady=6)
        knopf.pack(side='left')
        knopf.bind('<Button-1>', lambda e, v=variable, ti=titel: self._waehlen(v, ti))

    # ------------------------------------------------------- Rückmeldungen
    #
    # ⚠ Hier lag ein Totalausfall: Alle Rückmeldungen gingen direkt an
    # `self.meldung` — das Label im Fuß des **eigenständigen** Einstellungsfensters.
    # Eingebettet in das Hauptfenster wird dieser Fuß nie gebaut, das Label gibt es
    # also gar nicht. Jeder Klick auf „Jetzt auffrischen", „Prüfen" oder eine
    # Textquelle brach sofort mit `AttributeError` ab — noch **vor** der eigentlichen
    # Arbeit. Die Seite sah vollständig aus und tat nichts.
    #
    # Deshalb laufen alle Meldungen jetzt durch `_melden()`. Eingebettet gehen sie
    # an den Rückruf, den das Hauptfenster setzt (seine Fußzeile), sonst an das
    # eigene Label.
    melder = None                 # setzt das Hauptfenster beim Einbetten
    lage_melder = None            # dito, für den Zustandsblock der Seite

    def _melden(self, text, farbe=None):
        if getattr(self, 'melder', None):
            try:
                self.melder(text)
                return
            except Exception:
                pass
        label = getattr(self, 'meldung', None)
        if label is not None:
            try:
                label.configure(text=text, fg=farbe or SUB)
            except tk.TclError:
                pass

    def _weiterarbeiten(self):
        """Tk Gelegenheit geben, die Meldung wirklich zu zeigen."""
        try:
            self.root.update()
        except tk.TclError:
            pass

    def _waehlen(self, variable, titel):
        # ⚠ Nicht `filedialog`: Unter Linux zeichnet Tk seinen eigenen
        # Motif-Kasten. `dateiwahl` nimmt den Dialog des Systems, wo es einen
        # gibt, und fällt sonst auf Tk zurück.
        from . import dateiwahl
        ordner = dateiwahl.ordner_waehlen(titel, variable.get() or None)
        if ordner:
            variable.set(ordner)

    def _intervallfeld(self, eltern):
        self._titel(eltern, t('e_intervall'), t('e_intervall_hilfe'))
        from .hauptfenster import rundes_feld
        feld = rundes_feld(eltern, self.intervall, schrift(10), FLAECHE, LINIE,
                           ACCENT, FG, breite=8)
        feld.halter.pack(anchor='w')

    def _tonfeld(self, eltern):
        self._titel(eltern, t('e_ton'), t('e_ton_hilfe'))
        self.ton_lbl = tk.Label(eltern, bg=FLAECHE, font=schrift(10),
                                cursor='hand2', padx=12, pady=6)
        self.ton_lbl.pack(anchor='w')
        self.ton_lbl.bind('<Button-1>', lambda e: self._ton_umschalten())
        self._ton_beschriften()

    def _ton_umschalten(self):
        self.ton.set(not self.ton.get())
        self._ton_beschriften()

    def _ton_beschriften(self):
        an = self.ton.get()
        self.ton_lbl.configure(text=' %s ' % (t('e_an') if an else t('e_aus')),
                               fg=BG if an else SUB,
                               bg=ACCENT if an else FLAECHE)

    def _deckkraftfeld(self, eltern):
        """Schieberegler — und die Wirkung sofort sichtbar.

        Eine Zahl einzutippen und dann zu speichern, um zu sehen, ob es passt,
        wäre hier unbrauchbar: Durchsichtigkeit beurteilt man mit dem Auge, nicht
        mit einem Wert. Deshalb wird das Overlay beim Ziehen mitgeführt."""
        self._titel(eltern, t('e_deckkraft'), t('e_deckkraft_hilfe'))
        regler = tk.Scale(eltern, from_=30, to=100, orient='horizontal',
                          variable=self.deckkraft, bg=BG, fg=FG,
                          troughcolor=FLAECHE, highlightthickness=0,
                          activebackground=ACCENT, font=schrift(9),
                          length=300, command=self._deckkraft_vorfuehren)
        regler.pack(anchor='w')

    def _deckkraft_vorfuehren(self, _wert=None):
        """Das Overlay sofort mitziehen, damit man sieht, was man einstellt."""
        try:
            haupt = self.root.master
            while haupt is not None and not isinstance(haupt, tk.Tk):
                haupt = haupt.master
            if haupt is not None:
                haupt.attributes('-alpha', self.deckkraft.get() / 100.0)
        except tk.TclError:
            pass

    def _injektionsfeld(self, eltern):
        """Bauplan-Angaben im Spiel: Zustand, Auffrischen, Entfernen, Update.

        Auffrischen ist kein Luxus, sondern nötig: Jedes Übersetzungs-Update und
        jeder Spiel-Patch schreibt die `global.ini` neu — die Angaben sind dann
        stillschweigend weg. Deshalb steht hier immer, ob sie gerade drin sind."""
        self._titel(eltern, t('schritt_spiel_texte'), t('inj_wie'))
        self.inj_lage_lbl = tk.Label(eltern, text='', bg=BG, fg=SUB,
                                 font=schrift(10), anchor='w', justify='left',
                                 wraplength=600)
        self.inj_lage_lbl.pack(fill='x', pady=(0, 8))

        # Quelle wechseln — dieselben drei Wege wie im Einrichtungsassistenten.
        # Wer sich später umentscheidet (etwa vom deutschen auf den englischen
        # Client), soll dafür nicht den Assistenten suchen müssen.
        # Untereinander, nicht nebeneinander: Zu dritt nebeneinander passte der
        # letzte nicht mehr ins Fenster und war schlicht unsichtbar — man musste
        # das Fenster erst breiter ziehen, um zu ahnen, dass es ihn gibt.
        wahl = tk.Frame(eltern, bg=BG)
        wahl.pack(fill='x', pady=(0, 8))
        for schluessel, quelle in (('inj_quelle_de', 'deutsch'),
                                   ('inj_quelle_ss', 'starstrings'),
                                   ('inj_quelle_orig', 'original')):
            k = tk.Label(wahl, text='  %s  ' % t(schluessel), bg=FLAECHE, fg=FG,
                         font=schrift(10), cursor='hand2', padx=10, pady=7,
                         anchor='w')
            k.pack(fill='x', pady=(0, 4))
            k.bind('<Button-1>', lambda e, q=quelle: self._inj_wechseln(q))
        tk.Label(eltern, text=t('inj_fremd'), bg=BG, fg=SUB, font=schrift(9),
                 anchor='w', justify='left', wraplength=600).pack(fill='x',
                                                                  pady=(0, 10))

        reihe = tk.Frame(eltern, bg=BG)
        reihe.pack(fill='x')
        for text, tat in ((t('inj_erneuern'), self._inj_erneuern),
                          (t('inj_entfernen'), self._inj_entfernen),
                          (t('inj_pruefen'), self._inj_pruefen)):
            k = tk.Label(reihe, text=' %s ' % text, bg=FLAECHE, fg=FG,
                         font=schrift(10), cursor='hand2', padx=10, pady=6)
            k.pack(side='left', padx=(0, 6))
            k.bind('<Button-1>', lambda e, f=tat: f())
        self._inj_lage_zeigen()

    def _inj_wechseln(self, quelle):
        """Auf eine andere Textquelle umstellen — holen, einsetzen, auszeichnen.

        ⚠ Vor dem ersten Einsetzen einer **fremden** Quelle wird gefragt. Grund
        aus dem Test (Bomb20, 25.08.2026): „übrigens tauscht das tool — wenn auf
        deutsch gestellt — auch im Spiel alles englische gegen deutsches aus."
        Das ist so gewollt, aber niemand rechnet damit: Wer einen Bauplan-Melder
        installiert, erwartet keine vollständige Spielübersetzung. Eine
        Überraschung an der Spielinstallation ist genau das, was dieses
        Werkzeug nicht sein will.

        „Original" fragt nicht — das nimmt die Texte aus der eigenen
        Installation und ändert die Sprache nicht.
        """
        if quelle in ('deutsch', 'starstrings') and not self._quelle_bestaetigt(quelle):
            return
        self._inj_wechseln_jetzt(quelle)

    def _quelle_bestaetigt(self, quelle):
        """Einmal je Quelle fragen, bevor die Textdatei ersetzt wird.

        Einmal bestätigt, wird nicht wieder gefragt — wer die Quelle schon
        benutzt, weiß, was sie tut. Gemerkt wird das in den Einstellungen.
        """
        gemerkt = pfade.einstellung('inj_bestaetigt') or ''
        if quelle in gemerkt.split(','):
            return True

        from .hauptfenster import frage_stellen
        name = {'deutsch': t('s_sp_q_de'),
                'starstrings': t('s_sp_q_ss')}.get(quelle, quelle)
        if not frage_stellen(self.root, t('s_sp_warnung_titel'),
                             t('s_sp_warnung') % name):
            return False
        neu = [x for x in gemerkt.split(',') if x] + [quelle]
        pfade.einstellung_setzen('inj_bestaetigt', ','.join(neu))
        return True

    def _inj_wechseln_jetzt(self, quelle):
        """Der eigentliche Wechsel — ohne Rückfrage."""
        def melde(x):
            self._melden(x)
            self._weiterarbeiten()

        melde(t('inj_laeuft'))
        try:
            if quelle == 'original':
                # Holt die englische global.ini aus dem Data.p4k des Spielers —
                # dadurch braucht die Auszeichnung **keine** Fremdquelle.
                sprache_ordner = 'english'
                ok, meldung = spieltexte.holen(sprache_ordner, fortschritt=melde)
                if not ok:
                    self._melden(t('inj_fehler', meldung), ROT)
                    return
                # `g_language` setzt `spieltexte.holen()` selbst.
                ziel = uebersetzung.ziel_ini(sprache_ordner)
                uebersetzung.vermerken('original', 'Data.p4k')
            else:
                ok, meldung = uebersetzung.holen(quelle, fortschritt=melde)
                if not ok:
                    self._melden(t('inj_fehler', meldung), ROT)
                    return
                sprache_ordner = uebersetzung.QUELLEN[quelle]['sprache']
                ziel = uebersetzung.ziel_ini(sprache_ordner)
            ok, n, meldung = injektion.einrichten(ziel, sprache_ordner,
                                                  fortschritt=melde)
            self._melden(t('inj_aktiv', n) if ok else t('inj_fehler', meldung),
                         SUB if ok else ROT)
        except Exception as e:
            self._melden(t('inj_fehler', e), ROT)
        self._inj_lage_zeigen()

    def _inj_ini(self):
        """Die `global.ini`, um die es geht — siehe `injektion.ini_datei()`.

        ⚠ Die Logik steht bewusst NICHT mehr hier, sondern frei im Modul
        `injektion`: Der Diagnosebericht braucht dieselbe Auskunft und hat
        kein Fenster. Zwei Kopien wären zwei Wahrheiten.
        """
        return injektion.ini_datei()

    def inj_lage(self=None):
        """Steht etwas im Spiel, und aus welcher Quelle? (dict für die Seite)"""
        return injektion.lage()

    def _inj_lage_zeigen(self):
        lage = self.inj_lage()
        text = t('inj_steht') if lage['drin'] else t('inj_steht_nicht')
        if lage['stand']:
            text += ' · %s' % lage['stand']
        if not lage['datei']:
            text = '—'
        label = getattr(self, 'inj_lage_lbl', None)
        if label is not None:
            try:
                label.configure(text=text,
                                fg=ACCENT if lage['drin'] else SUB)
            except tk.TclError:
                pass
        if getattr(self, 'lage_melder', None):
            try:
                self.lage_melder()
            except Exception:
                pass

    def _inj_erneuern(self):
        pfad, sprache_ordner, _ = self._inj_ini()
        if not pfad:
            self._melden(t('inj_fehler', 'global.ini'), ROT)
            return
        self._melden(t('inj_laeuft'))
        self._weiterarbeiten()
        ok, n, meldung = injektion.aktualisieren(
            pfad, sprache_ordner,
            fortschritt=lambda x: (self._melden(x), self._weiterarbeiten()))
        self._melden(t('inj_aktiv', n) if ok else t('inj_fehler', meldung),
                     SUB if ok else ROT)
        self._inj_lage_zeigen()

    def _inj_entfernen(self):
        pfad, sprache_ordner, _ = self._inj_ini()
        if not pfad:
            return
        ok, n, meldung = injektion.entfernen(pfad, sprache_ordner)
        self._melden(meldung, SUB if ok else ROT)
        self._inj_lage_zeigen()

    def _inj_pruefen(self):
        """Gibt es bei der benutzten Quelle etwas Neues?"""
        pfad, _sprache, quelle = self._inj_ini()
        drin = bool(pfad and os.path.isfile(pfad) and injektion.ist_drin(pfad))
        if not quelle:
            # Keine Übersetzung vermerkt — dann bleibt nur die Aussage, ob die
            # Angaben gerade im Spiel stehen.
            self._melden(t('inj_steht') if drin else t('inj_steht_nicht'))
            return
        self._melden(t('inj_laeuft'))
        self._weiterarbeiten()
        neu, kennung = uebersetzung.update_da(quelle)
        stand = uebersetzung.installiert(quelle)
        teile = [t('inj_steht') if drin else t('inj_steht_nicht')]
        if stand:
            teile.append(str(stand))
        teile.append(t('inj_update_da', kennung) if neu else t('inj_aktuell'))
        self._melden(' · '.join(teile))

    def _fuss(self):
        fuss = tk.Frame(self.root, bg=BG)
        fuss.pack(fill='x', side='bottom', padx=20, pady=16)
        # Trennlinie, damit der Fuß sich vom rollenden Inhalt absetzt
        tk.Frame(self.root, bg=BAR, height=1).pack(
            fill='x', side='bottom')
        self.meldung = tk.Label(fuss, text='', bg=BG, fg=SUB, font=schrift(9),
                                anchor='w', justify='left', wraplength=420)
        self.meldung.pack(side='left', fill='x', expand=True)
        self.speichern_lbl = tk.Label(fuss, text='  %s  ' % t('e_speichern'),
                                      bg=ACCENT, fg=BG, font=schrift(11, True),
                                      cursor='hand2', padx=10, pady=8)
        self.speichern_lbl.pack(side='right')
        self.speichern_lbl.bind('<Button-1>', lambda e: self._speichern())

    def _neu_beschriften(self):
        """Nach einem Sprachwechsel alles neu aufbauen — einfacher und
        verlässlicher, als zwanzig Beschriftungen einzeln nachzuziehen.

        ⚠ Im **eingebetteten** Zustand (als Seite im Hauptfenster) darf hier
        nichts neu erzeugt werden: `self.root` ist dann ein Rahmen im großen
        Fenster, und ein neues `Einstellungsfenster` daneben ginge als eigenes
        Fenster auf. Genau das ist passiert. Stattdessen sagt das Modul dem
        Hauptfenster Bescheid, und **das** zeichnet seine Seiten neu — dort
        stehen ja ebenfalls überall Texte.
        """
        if getattr(self, 'eingebettet', False):
            if callable(getattr(self, 'beim_sprachwechsel', None)):
                self.beim_sprachwechsel()
            return

        werte = (self.sprache_wahl.get(), self.spiel.get(), self.launcher.get(),
                 self.intervall.get(), self.ton.get(), self.deckkraft.get())
        eltern = self.root.master
        self.root.destroy()
        neu = Einstellungsfenster(eltern)
        (neu.sprache_wahl.set(werte[0]), neu.spiel.set(werte[1]),
         neu.launcher.set(werte[2]), neu.intervall.set(werte[3]),
         neu.ton.set(werte[4]), neu.deckkraft.set(werte[5]))
        neu._sprach_knoepfe_faerben()
        neu._ton_beschriften()

    # -------------------------------------------------------------- Speichern
    def _speichern(self):
        """Alles auf einmal — und vorher prüfen, was prüfbar ist.

        Ein Ordner, den es nicht gibt, wird **nicht** gespeichert: Sonst sucht
        der Watcher beim nächsten Start an einem Ort, den der Spieler für
        richtig hält, und meldet nichts, ohne dass jemand den Grund sieht."""
        for variable in (self.spiel, self.launcher):
            wert = variable.get().strip()
            if wert and not os.path.isdir(os.path.expanduser(wert)):
                self._melden(t('e_pfad_fehlt'), ROT)
                return

        try:
            takt = int(self.intervall.get())
        except ValueError:
            takt = 3
        takt = max(INTERVALL_MIN, min(INTERVALL_MAX, takt))
        self.intervall.set(str(takt))

        pfade.einstellung_setzen('sprache', self.sprache_wahl.get())
        pfade.einstellung_setzen('spiel_ordner', self.spiel.get().strip())
        pfade.einstellung_setzen('launcher_ordner', self.launcher.get().strip())
        pfade.einstellung_setzen('pruefintervall_sekunden', takt)
        pfade.einstellung_setzen('signalton', bool(self.ton.get()))
        pfade.einstellung_setzen('deckkraft_prozent', int(self.deckkraft.get()))

        # Ehrlich sagen, was sofort gilt und was nicht: Die Sprache schaltet
        # dieses Fenster gerade selbst um, Ordner und Takt liest der laufende
        # Watcher-Thread aber nur beim Start.
        self._melden(t('e_neustart_noetig'))

    def schliessen(self):
        self.root.destroy()


def oeffnen(eltern=None):
    """Das Fenster zeigen — oder ein schon offenes nach vorn holen."""
    vorhanden = getattr(oeffnen, '_offen', None)
    if vorhanden is not None:
        try:
            # ⚠ `lift()` allein wird unter Wayland ignoriert, und ein
            # minimiertes Fenster bliebe minimiert — siehe `nach_vorn()`.
            from .hauptfenster import nach_vorn
            nach_vorn(vorhanden.root)
            return vorhanden
        except tk.TclError:
            pass
    fenster = Einstellungsfenster(eltern)
    oeffnen._offen = fenster
    return fenster
