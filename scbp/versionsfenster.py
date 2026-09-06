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
„Was ist neu" — Versionsgeschichte zum Nachlesen, und der Weg zur neuen Version.

Vorbild ist das Info-Log des SC Deutsch Launcher: nicht nur die Meldung, dass es
etwas Neues gibt, sondern auch **was** neu ist — und das auch für ältere
Versionen. Wer eine Version übersprungen hat, soll nachlesen können, was
dazwischen passiert ist.

Oben steht, falls vorhanden, die neue Version mit einem Knopf zum Holen.
Darunter die Geschichte, neueste zuerst.
"""
import threading
import tkinter as tk

import re

from . import aktualisierung, pfade, sprache
from .sprache import t, fenstertitel

BG      = '#10141c'
FLAECHE = '#161c28'
BAR     = '#1b2230'
FG      = '#e6edf3'
SUB     = '#8b98a5'
ACCENT  = '#9ce430'
GELB    = '#d8a03a'


def schrift(groesse, fett=False):
    fam = 'Segoe UI' if pfade.WINDOWS else 'Helvetica'
    return (fam, groesse, 'bold' if fett else 'normal')


def sprachteil(text):
    """Aus einem zweisprachigen Release-Text den passenden Teil holen.

    Die Release-Texte tragen Englisch oben und Deutsch in einem aufklappbaren
    Block darunter — auf GitHub ist das richtig, im Fenster wäre es doppelt.
    Hier bekommt jeder nur seine Sprache zu sehen; fehlt sie, bleibt alles
    stehen, denn eine unvollständige Auskunft ist schlechter als eine
    fremdsprachige."""
    m = re.search(r'<details>\s*<summary>.*?</summary>(.*?)</details>', text,
                  re.S | re.I)
    if not m:
        return text
    deutsch = m.group(1).strip()
    englisch = text[:m.start()].strip().rstrip('-').strip()
    if sprache.aktuelle() == 'de':
        return deutsch or englisch
    return englisch or deutsch


def aufbereiten(text):
    """Markdown so weit entschärfen, dass es sich als schlichter Text liest.

    Ein vollwertiger Markdown-Anzeiger wäre ein eigenes Projekt und bräuchte
    Pakete, die dieses Programm nicht haben will. Sternchen und Rauten weg,
    Listenpunkte vereinheitlichen — mehr braucht es für Release-Texte nicht."""
    zeilen = []
    for roh in (text or '').splitlines():
        zeile = roh.rstrip()
        zeile = zeile.replace('**', '').replace('`', '')
        if zeile.startswith('### '):
            zeile = zeile[4:].upper()
        elif zeile.startswith('## '):
            zeile = zeile[3:].upper()
        elif zeile.lstrip().startswith('- '):
            einzug = len(zeile) - len(zeile.lstrip())
            zeile = ' ' * einzug + '•' + zeile.lstrip()[1:]
        zeilen.append(zeile)
    return '\n'.join(zeilen).strip()


class Versionsfenster:
    def __init__(self, eltern=None, eigene_version='', beim_schliessen=None):
        self.eigene = eigene_version
        self.beim_schliessen = beim_schliessen
        self.neue = aktualisierung.nachsehen(eigene_version)

        self.root = tk.Toplevel(eltern) if eltern else tk.Tk()
        self.root.title(fenstertitel('SC BP Watcher — ' + t('was_ist_neu')))
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
        if eltern is None or not mittig_ueber(self.root, eltern, 700, 740):
            self.root.geometry('700x740')
        self.root.protocol('WM_DELETE_WINDOW', self.schliessen)

        kopf = tk.Frame(self.root, bg=BAR)
        kopf.pack(fill='x')
        tk.Label(kopf, text=t('was_ist_neu'), bg=BAR, fg=FG,
                 font=schrift(12, True)).pack(side='left', padx=16, pady=11)
        # Von Hand nachsehen — nötig, weil die Abfrage sonst höchstens
        # stündlich läuft und man sonst nicht weiß, ob gerade geprüft wurde.
        self.pruef_lbl = tk.Label(kopf, text=' %s ' % t('inj_pruefen'), bg=FLAECHE,
                                  fg=FG, font=schrift(10), cursor='hand2',
                                  padx=10, pady=5)
        self.pruef_lbl.pack(side='right', padx=16)
        self.pruef_lbl.bind('<Button-1>', lambda e: self._jetzt_pruefen())
        # Kein eigenes ✕ — das Fenster hat eine Systemtitelleiste, und die hat
        # schon eins. Zwei Kreuze übereinander sehen aus wie ein Fehler.

        self._banner()
        self._geschichte()

    def _jetzt_pruefen(self):
        """Sofort bei GitHub nachfragen — der Knopf wird selbst zur Antwort.

        Bewusst **kein** Neuaufbau des Fensters: Wer nachsieht, will eine
        Auskunft, kein Flackern. Steht etwas Neues an, sagt der Knopf es; sonst
        steht dort, dass alles aktuell ist."""
        self.pruef_lbl.configure(text='  …  ', fg=SUB)
        self.root.update()
        neu = aktualisierung.nachsehen(self.eigene, erzwingen=True)
        if neu and neu.get('version'):
            self.neue = neu
            self.pruef_lbl.configure(
                text='  %s  ' % t('neue_version_da', neu['version']), fg=ACCENT)
        else:
            self.pruef_lbl.configure(text='  %s  ' % t('inj_aktuell'), fg=SUB)

    # ------------------------------------------------------------------ Banner
    def _banner(self):
        """Der Hinweis auf die neue Version — nur wenn es eine gibt."""
        self.banner = tk.Frame(self.root, bg=FLAECHE)
        self.banner.pack(fill='x', padx=14, pady=(12, 0))
        if not self.neue:
            tk.Label(self.banner, text=t('aktuelle_fassung'), bg=FLAECHE, fg=SUB,
                     font=schrift(10), anchor='w', padx=14,
                     pady=10).pack(fill='x')
            return

        oben = tk.Frame(self.banner, bg=FLAECHE)
        oben.pack(fill='x', padx=14, pady=(12, 4))
        tk.Label(oben, text=t('neue_version_da', self.neue['version']),
                 bg=FLAECHE, fg=ACCENT, font=schrift(13, True),
                 anchor='w').pack(side='left')
        tk.Label(oben, text=t('du_hast', self.eigene), bg=FLAECHE, fg=SUB,
                 font=schrift(9), anchor='e').pack(side='right')

        self.meldung = tk.Label(self.banner, text='', bg=FLAECHE, fg=SUB,
                                font=schrift(10), anchor='w', justify='left',
                                wraplength=620)
        self.meldung.pack(fill='x', padx=14)

        knoepfe = tk.Frame(self.banner, bg=FLAECHE)
        knoepfe.pack(fill='x', padx=14, pady=(8, 12))
        art = aktualisierung.verpackung()
        datei = aktualisierung.passende_datei(self.neue)
        if art == 'quellcode':
            self.meldung.configure(text=t('update_quellcode'))
        elif not datei:
            self.meldung.configure(text=t('selbst_holen'))
        else:
            self.holen = tk.Label(knoepfe, text='  %s  ' % t('jetzt_holen'),
                                  bg=ACCENT, fg=BG, font=schrift(10, True),
                                  cursor='hand2', padx=10, pady=6)
            self.holen.pack(side='left')
            self.holen.bind('<Button-1>', lambda e, d=datei: self._holen(d))
            self._knopfleiste = knoepfe

    def _holen(self, datei):
        """Herunterladen und einspielen — im Nebenläufer, damit nichts einfriert."""
        self.holen.configure(bg=BAR, fg=SUB, cursor='')
        self.holen.unbind('<Button-1>')

        def arbeit():
            try:
                ziel = aktualisierung.herunterladen(
                    datei, fortschritt=lambda p: self.root.after(
                        0, lambda: self.meldung.configure(
                            text=t('wird_geladen', p))))
                geklappt, grund = aktualisierung.einspielen(ziel)
                self.root.after(0, lambda: self._ergebnis(geklappt, grund))
            except Exception as fehler:
                nachricht = str(fehler)
                self.root.after(0, lambda: self._ergebnis(False, nachricht))

        threading.Thread(target=arbeit, daemon=True).start()

    def _ergebnis(self, geklappt, grund):
        if not geklappt:
            self.meldung.configure(text=t('update_fehler', grund) + '\n'
                                   + t('selbst_holen'), fg=GELB)
            return

        # ⚠ Hier stand nur „Beim nächsten Start läuft die neue Version" — und
        # genau das stimmt unter Windows **nicht**. Dort tauscht ein Hilfsskript
        # die Datei erst, wenn das Programm beendet ist; wer einfach weiterspielt,
        # bei dem gibt es nach zwei Minuten auf, und aktualisiert ist nichts.
        #
        # Morkhan am 26.08.2026: „dann klicke ich auf jetzt holen, dann läuft
        # des durch … und dann passiert nix mehr." Er hatte alles richtig
        # gemacht — es fehlte schlicht der zweite Schritt, und niemand sagte ihm
        # das. In den Einstellungen gibt es den Neustart-Knopf längst; hier war
        # er nie eingebaut.
        self.meldung.configure(text=t('neustart_noetig'), fg=ACCENT)
        try:
            self._neustart_knopf()
        except Exception as ausnahme:
            from . import fehler
            fehler.merken('versionsfenster.neustart_knopf', ausnahme)

    def _neustart_knopf(self):
        """Aus „geladen" wird ein Knopf, der den Neustart auch ausführt."""
        leiste = getattr(self, '_knopfleiste', None)
        if leiste is None:
            return
        knopf = tk.Label(leiste, text='  %s  ' % t('s_ub_neustart'),
                         bg=ACCENT, fg=BG, font=schrift(10, True),
                         cursor='hand2', padx=10, pady=6)
        knopf.pack(side='left', padx=(8, 0))
        knopf.bind('<Button-1>', lambda e: self._neu_starten())

    def _neu_starten(self):
        """Die frisch geladene Version übernehmen.

        ⚠ Derselbe Ablauf wie auf der Einstellungsseite: Der Notausgang wird
        **sofort** scharf gestellt, nicht erst in einem Tk-Rückruf — feuert der
        nicht, liefe der Prozess weiter, während sein Arbeitsordner schon
        abgeräumt wird.
        """
        import os
        if not aktualisierung.neu_starten():
            self.meldung.configure(text=t('s_ub_neustart_nein'), fg=GELB)
            return

        # ⚠ **Erst nachsehen, ob die neue Version lebt.** Vorher wurde der
        # Notausgang hier sofort scharf gestellt — war die neue Version schon
        # tot (unter Linux monatelang der Regelfall), stand der Rechner ohne
        # Watcher da, und niemand erfuhr den Grund. Siehe
        # `aktualisierung.neue_fassung_laeuft`.
        def pruefen():
            lebt = aktualisierung.neue_fassung_laeuft()

            def weiter():
                if not lebt:
                    self.meldung.configure(text=t('s_ub_neustart_tot'), fg=GELB)
                    return
                threading.Timer(2.0, lambda: os._exit(0)).start()
                try:
                    self.root.quit()
                    self.root.destroy()
                except Exception:
                    pass
            try:
                self.root.after(0, weiter)
            except Exception:
                pass

        threading.Thread(target=pruefen, daemon=True).start()

    # ------------------------------------------------------------- Geschichte
    def _geschichte(self):
        rahmen = tk.Frame(self.root, bg=BG)
        rahmen.pack(fill='both', expand=True, padx=14, pady=12)
        leinwand = tk.Canvas(rahmen, bg=BG, highlightthickness=0)
        from .hauptfenster import rundleiste
        rolle = rundleiste(rahmen, leinwand, grund=BG)
        inhalt = tk.Frame(leinwand, bg=BG)
        inhalt.bind('<Configure>', lambda e: leinwand.configure(
            scrollregion=leinwand.bbox('all')))
        fenster = leinwand.create_window((0, 0), window=inhalt, anchor='nw')
        leinwand.bind('<Configure>',
                      lambda e: leinwand.itemconfigure(fenster, width=e.width))
        leinwand.configure(yscrollcommand=rolle.set)
        leinwand.pack(side='left', fill='both', expand=True)
        rolle.pack(side='right', fill='y')
        from .hauptfenster import rad_anschliessen
        rad_anschliessen(leinwand)

        eintraege = aktualisierung.protokoll()
        if not eintraege:
            tk.Label(inhalt, text=t('keine_versionen'), bg=BG, fg=SUB,
                     font=schrift(11), pady=20).pack()
            return
        for e in eintraege:
            self._eintrag(inhalt, e)

    def _eintrag(self, eltern, e):
        block = tk.Frame(eltern, bg=BG)
        block.pack(fill='x', pady=(0, 18))
        kopf = tk.Frame(block, bg=BG)
        kopf.pack(fill='x')
        # Die eigene Version hervorheben — dann sieht man auf einen Blick,
        # wie weit man zurückliegt.
        eigen = (aktualisierung._teile(e['version'])
                 == aktualisierung._teile(self.eigene))
        tk.Label(kopf, text=e['version'], bg=BG, fg=ACCENT if eigen else FG,
                 font=schrift(12, True), anchor='w').pack(side='left')
        rechts = e['datum']
        if eigen:
            rechts = (rechts + '  ·  ' if rechts else '') + t('du_hast', '').strip(' %s')
        if rechts:
            tk.Label(kopf, text=rechts, bg=BG, fg=SUB, font=schrift(9),
                     anchor='e').pack(side='right')
        tk.Frame(block, bg=FLAECHE, height=1).pack(fill='x', pady=(4, 8))
        tk.Label(block, text=aufbereiten(sprachteil(e['text'])) or '—', bg=BG, fg=SUB,
                 font=schrift(10), anchor='w', justify='left',
                 wraplength=630).pack(fill='x')

    def schliessen(self):
        if self.beim_schliessen:
            self.beim_schliessen()
        self.root.destroy()

    def run(self):
        self.root.mainloop()


if __name__ == '__main__':
    Versionsfenster(eigene_version='1.0.3').run()
