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

from . import updater, pfade, sprache
from .sprache import t, fenstertitel

BG      = '#10141c'
FLAECHE = '#161c28'
BAR     = '#1b2230'
FG      = '#e6edf3'
SUB     = '#8b98a5'
ACCENT  = '#9ce430'
GELB    = '#d8a03a'


def schrift(size, bold=False):
    fam = 'Segoe UI' if pfade.WINDOWS else 'Helvetica'
    return (fam, size, 'bold' if bold else 'normal')


def language_part(text):
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
    german = m.group(1).strip()
    english = text[:m.start()].strip().rstrip('-').strip()
    if sprache.aktuelle() == 'de':
        return german or english
    return english or german


def prepare(text):
    """Markdown so weit entschärfen, dass es sich als schlichter Text liest.

    Ein vollwertiger Markdown-Anzeiger wäre ein eigenes Projekt und bräuchte
    Pakete, die dieses Programm nicht haben will. Sternchen und Rauten weg,
    Listenpunkte vereinheitlichen — mehr braucht es für Release-Texte nicht."""
    zeilen = []
    for roh in (text or '').splitlines():
        row = roh.rstrip()
        row = row.replace('**', '').replace('`', '')
        if row.startswith('### '):
            row = row[4:].upper()
        elif row.startswith('## '):
            row = row[3:].upper()
        elif row.lstrip().startswith('- '):
            indent = len(row) - len(row.lstrip())
            row = ' ' * indent + '•' + row.lstrip()[1:]
        zeilen.append(row)
    return '\n'.join(zeilen).strip()


class VersionWindow:
    def __init__(self, parent=None, own_version='', on_close=None):
        self.own_value = own_version
        self.on_close = on_close
        self.newer = updater.check(own_version)

        self.root = tk.Toplevel(parent) if parent else tk.Tk()
        self.root.title(fenstertitel(t('hf_titel') + ' — ' + t('was_ist_neu')))
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
        if parent is None or not center_over(self.root, parent, 700, 740):
            self.root.geometry('700x740')
        self.root.protocol('WM_DELETE_WINDOW', self.close)

        head = tk.Frame(self.root, bg=BAR)
        head.pack(fill='x')
        tk.Label(head, text=t('was_ist_neu'), bg=BAR, fg=FG,
                 font=schrift(12, True)).pack(side='left', padx=16, pady=11)
        # Von Hand nachsehen — nötig, weil die Abfrage sonst höchstens
        # stündlich läuft und man sonst nicht weiß, ob gerade geprüft wurde.
        self.check_label = tk.Label(head, text=' %s ' % t('inj_pruefen'), bg=FLAECHE,
                                  fg=FG, font=schrift(10), cursor='hand2',
                                  padx=10, pady=5)
        self.check_label.pack(side='right', padx=16)
        self.check_label.bind('<Button-1>', lambda e: self._check_now())
        # Kein eigenes ✕ — das Fenster hat eine Systemtitelleiste, und die hat
        # schon eins. Zwei Kreuze übereinander sehen aus wie ein Fehler.

        self._banner()
        self._history()

    def _check_now(self):
        """Sofort bei GitHub nachfragen — der Knopf wird selbst zur Antwort.

        Bewusst **kein** Neuaufbau des Fensters: Wer nachsieht, will eine
        Auskunft, kein Flackern. Steht etwas Neues an, sagt der Knopf es; sonst
        steht dort, dass alles aktuell ist."""
        self.check_label.configure(text='  …  ', fg=SUB)
        self.root.update()
        fresh = updater.check(self.own_value, force=True)
        if fresh and fresh.get('version'):
            self.newer = fresh
            self.check_label.configure(
                text='  %s  ' % t('neue_version_da', fresh['version']), fg=ACCENT)
        else:
            self.check_label.configure(text='  %s  ' % t('inj_aktuell'), fg=SUB)

    # ------------------------------------------------------------------ Banner
    def _banner(self):
        """Der Hinweis auf die neue Version — nur wenn es eine gibt."""
        self.banner = tk.Frame(self.root, bg=FLAECHE)
        self.banner.pack(fill='x', padx=14, pady=(12, 0))
        if not self.newer:
            tk.Label(self.banner, text=t('aktuelle_fassung'), bg=FLAECHE, fg=SUB,
                     font=schrift(10), anchor='w', padx=14,
                     pady=10).pack(fill='x')
            return

        top = tk.Frame(self.banner, bg=FLAECHE)
        top.pack(fill='x', padx=14, pady=(12, 4))
        tk.Label(top, text=t('neue_version_da', self.newer['version']),
                 bg=FLAECHE, fg=ACCENT, font=schrift(13, True),
                 anchor='w').pack(side='left')
        tk.Label(top, text=t('du_hast', self.own_value), bg=FLAECHE, fg=SUB,
                 font=schrift(9), anchor='e').pack(side='right')

        self.message = tk.Label(self.banner, text='', bg=FLAECHE, fg=SUB,
                                font=schrift(10), anchor='w', justify='left',
                                wraplength=620)
        self.message.pack(fill='x', padx=14)

        buttons = tk.Frame(self.banner, bg=FLAECHE)
        buttons.pack(fill='x', padx=14, pady=(8, 12))
        kind = updater.packaging()
        asset = updater.matching_asset(self.newer)
        if kind == 'quellcode':
            self.message.configure(text=t('update_quellcode'))
        elif not asset:
            self.message.configure(text=t('selbst_holen'))
        else:
            self.fetch = tk.Label(buttons, text='  %s  ' % t('jetzt_holen'),
                                  bg=ACCENT, fg=BG, font=schrift(10, True),
                                  cursor='hand2', padx=10, pady=6)
            self.fetch.pack(side='left')
            self.fetch.bind('<Button-1>', lambda e, d=asset: self._fetch(d))
            self._knopfleiste = buttons

    def _fetch(self, asset):
        """Herunterladen und einspielen — im Nebenläufer, damit nichts einfriert."""
        self.fetch.configure(bg=BAR, fg=SUB, cursor='')
        self.fetch.unbind('<Button-1>')

        def arbeit():
            try:
                ziel = updater.download(
                    asset, progress=lambda p: self.root.after(
                        0, lambda: self.message.configure(
                            text=t('wird_geladen', p))),
                    release=self.newer)
                collapsed, bg_colour = updater.install(ziel)
                self.root.after(0, lambda: self._outcome(collapsed, bg_colour))
            except Exception as fehler:
                notice_text = str(fehler)
                self.root.after(0, lambda: self._outcome(False, notice_text))

        threading.Thread(target=arbeit, daemon=True).start()

    def _outcome(self, collapsed, bg_colour):
        if not collapsed:
            self.message.configure(text=t('update_fehler', bg_colour) + '\n'
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
        self.message.configure(text=t('neustart_noetig'), fg=ACCENT)
        try:
            self._neustart_knopf()
        except Exception as ausnahme:
            from . import fehler
            fehler.merken('version_window.restart_button', ausnahme)

    def _neustart_knopf(self):
        """Aus „geladen" wird ein Knopf, der den Neustart auch ausführt."""
        bar = getattr(self, '_knopfleiste', None)
        if bar is None:
            return
        button = tk.Label(bar, text='  %s  ' % t('s_ub_neustart'),
                         bg=ACCENT, fg=BG, font=schrift(10, True),
                         cursor='hand2', padx=10, pady=6)
        button.pack(side='left', padx=(8, 0))
        button.bind('<Button-1>', lambda e: self._restart())

    def _restart(self):
        """Die frisch geladene Version übernehmen.

        ⚠ Derselbe Ablauf wie auf der Einstellungsseite: Der Notausgang wird
        **sofort** scharf gestellt, nicht erst in einem Tk-Rückruf — feuert der
        nicht, liefe der Prozess weiter, während sein Arbeitsordner schon
        abgeräumt wird.
        """
        import os
        if not updater.restart():
            self.message.configure(text=t('s_ub_neustart_nein'), fg=GELB)
            return

        # ⚠ **Erst nachsehen, ob die neue Version lebt.** Vorher wurde der
        # Notausgang hier sofort scharf gestellt — war die neue Version schon
        # tot (unter Linux monatelang der Regelfall), stand der Rechner ohne
        # Watcher da, und niemand erfuhr den Grund. Siehe
        # `updater.neue_fassung_laeuft`.
        def check():
            alive = updater.new_version_alive()

            def weiter():
                if not alive:
                    self.message.configure(text=t('s_ub_neustart_tot'), fg=GELB)
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

        threading.Thread(target=check, daemon=True).start()

    # ------------------------------------------------------------- Geschichte
    def _history(self):
        frame = tk.Frame(self.root, bg=BG)
        frame.pack(fill='both', expand=True, padx=14, pady=12)
        canvas = tk.Canvas(frame, bg=BG, highlightthickness=0)
        from .main_window import round_scrollbar
        rolle = round_scrollbar(frame, canvas, bg=BG)
        body = tk.Frame(canvas, bg=BG)
        body.bind('<Configure>', lambda e: canvas.configure(
            scrollregion=canvas.bbox('all')))
        window = canvas.create_window((0, 0), window=body, anchor='nw')
        canvas.bind('<Configure>',
                      lambda e: canvas.itemconfigure(window, width=e.width))
        canvas.configure(yscrollcommand=rolle.set)
        canvas.pack(side='left', fill='both', expand=True)
        rolle.pack(side='right', fill='y')
        from .main_window import bind_wheel
        bind_wheel(canvas)

        entries = updater.history()
        if not entries:
            tk.Label(body, text=t('keine_versionen'), bg=BG, fg=SUB,
                     font=schrift(11), pady=20).pack()
            return
        for e in entries:
            self._eintrag(body, e)

    def _eintrag(self, parent, e):
        block = tk.Frame(parent, bg=BG)
        block.pack(fill='x', pady=(0, 18))
        head = tk.Frame(block, bg=BG)
        head.pack(fill='x')
        # Die eigene Version hervorheben — dann sieht man auf einen Blick,
        # wie weit man zurückliegt.
        own = (updater._parts(e['version'])
                 == updater._parts(self.own_value))
        tk.Label(head, text=e['version'], bg=BG, fg=ACCENT if own else FG,
                 font=schrift(12, True), anchor='w').pack(side='left')
        rechts = e['datum']
        if own:
            rechts = (rechts + '  ·  ' if rechts else '') + t('du_hast', '').strip(' %s')
        if rechts:
            tk.Label(head, text=rechts, bg=BG, fg=SUB, font=schrift(9),
                     anchor='e').pack(side='right')
        tk.Frame(block, bg=FLAECHE, height=1).pack(fill='x', pady=(4, 8))
        tk.Label(block, text=prepare(language_part(e['text'])) or '—', bg=BG, fg=SUB,
                 font=schrift(10), anchor='w', justify='left',
                 wraplength=630).pack(fill='x')

    def close(self):
        if self.on_close:
            self.on_close()
        self.root.destroy()

    def run(self):
        self.root.mainloop()


if __name__ == '__main__':
    VersionWindow(own_version='1.0.3').run()
