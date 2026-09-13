"""Sucht Bedienelemente, die über den Fensterrand hinausragen.

Warum das ein eigenes Werkzeug ist: Abgeschnittene Knöpfe fallen beim Bauen
nicht auf, weil die deutsche Beschriftung noch passt — auf Englisch sind die
Wörter länger, und dann steht dort „Ve…" statt „Very large". Genau das ist
mehrfach passiert und jedes Mal erst beim Ansehen eines Bildschirmfotos
aufgefallen. Eine Maschine sieht das zuverlässiger als ein müdes Auge.

Geprüft wird jede Seite in beiden Sprachen und in zwei Fenstergrößen: der
Mindestgröße, die ein Nutzer einstellen kann, und einer üblichen. Was rechts
über den Inhaltsbereich hinausragt, wird gemeldet.

Aufruf:  python3 tools/randpruefung.py
Rückgabe 0 = nichts ragt heraus, 1 = Fundstellen (Liste steht darüber).
"""
import os
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# Prueflaeufe bauen echte Fenster. Ohne diese Umleitung blitzen sie ueber
# einem laufenden Spiel auf und reissen den Fokus mit — siehe unsichtbar.py.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import unsichtbar                                              # noqa: E402
unsichtbar.sicherstellen(messend=True)
# ⛔ Vor der ersten Ausgabe: Die Windows-Konsole kann kein `⚠` — siehe ausgabe.py.
import ausgabe                                                 # noqa: E402
ausgabe.utf8()


os.environ.setdefault('SC_BP_HOME', tempfile.mkdtemp(prefix='randpruefung-'))
os.environ['SC_BP_NO_NET'] = '1'

import tkinter as tk                                          # noqa: E402
from scbp import main_window, seiten, sprache                # noqa: E402

# ⛔⛔ **Keine feste Liste.** Hier standen bis zum 14.09.2026 elf Kennungen von
# Hand — das Programm hatte längst **33**. Zweiundzwanzig Seiten wurden also
# nie auf abgeschnittene Beschriftungen geprüft, darunter alle Werkstatt- und
# Handelsseiten.
#
# Aufgefallen ist es, weil auf der Raffinerien-Seite bei „sehr groß" **drei
# Spalten gar nicht angezeigt** wurden — in einer ausgelieferten Fassung. Genau
# dafür gibt es dieses Werkzeug; es hat nur nie hingeschaut.
#
# Eine Liste von Hand ist am Tag ihrer Erweiterung still veraltet. Gefragt wird
# deshalb das Programm selbst.
SEITEN = list(seiten.kennungen())

GROESSEN = ('1100x842', '1440x900')

LUFT = 4          # Pixel Toleranz: ein Rahmen darf bündig abschließen.


def _beschriftung(w, tiefe=0):
    """Der Text eines Elements — bei Rahmen der Text der Kinder.

    Ein „Frame" als Fundstelle hilft niemandem beim Suchen. Also nachsehen,
    was darin steht, und die ersten Beschriftungen zusammenziehen.
    """
    try:
        text = w.cget('text')
        if text:
            return str(text)[:38]
    except Exception:
        pass
    if tiefe < 6:      # 3 reichte nicht: verschachtelte Rahmen blieben „Frame"
        woerter = []
        for kind in w.winfo_children():
            wort = _beschriftung(kind, tiefe + 1)
            if wort and not wort.endswith(('Frame', 'Canvas')):
                woerter.append(wort)
            if len(woerter) >= 3:
                break
        if woerter:
            return ' / '.join(woerter)[:44]
    return w.winfo_class()


def _durchgehen(w, gefunden):
    """Rekursiv durch alle sichtbaren Kinder und nach Beschnitt suchen.

    ⚠ Über die Position ist ein abgeschnittener Knopf NICHT zu finden: Tk
    beschneidet ein zu breites Kind still am Elternrahmen, und danach sehen
    die Koordinaten sauber aus — genau deshalb fällt so etwas beim Bauen nie
    auf. Messbar ist es nur im Vergleich „gebraucht gegen bekommen":
    `winfo_reqwidth` sagt, wie breit das Element sein müsste, `winfo_width`,
    wie breit es sein darf. Klafft das auseinander, fehlt sichtbar Text.

    Ein beschnittenes Element wird gemeldet und nicht weiter aufgeklappt —
    seine Kinder sind zwangsläufig mitbeschnitten, und zwanzig Meldungen für
    einen Knopf helfen niemandem.
    """
    for kind in w.winfo_children():
        try:
            if not kind.winfo_ismapped():
                continue
            gebraucht, bekommen = kind.winfo_reqwidth(), kind.winfo_width()
            # Rollflächen, Text- und Eingabefelder dürfen absichtlich
            # schmaler sein als ihr Inhalt: Dort rollt man oder tippt weiter.
            # Gesucht sind Beschriftungen, die stumm verstümmelt werden.
            #
            # ⚠ `Canvas` darf NICHT pauschal durchgehen. Jeder Knopf des
            # Hauses ist ein Canvas mit fest gerechneter Breite — genau die
            # Elemente, für die diese Prüfung gebaut wurde. Solange sie
            # ausgenommen waren, meldete sie nichts, während auf der
            # Über-Seite sichtbar „Einrichtung wiederho…" stand. Knöpfe
            # tragen deshalb die Markierung `is_button` und werden geprüft.
            knopf = getattr(kind, 'is_button', False)
            rollend = ((kind.winfo_class() in ('Canvas', 'Text', 'Listbox',
                                               'Entry', 'Scrollbar')
                        and not knopf)
                       or getattr(kind, 'sized', False))
            if gebraucht - bekommen > LUFT and not rollend:
                gefunden.append((_beschriftung(kind), gebraucht - bekommen,
                                 kind.winfo_class()))
            else:
                _durchgehen(kind, gefunden)
        except tk.TclError:
            continue


def _eine_runde(groesse, kuerzel, treffer):
    sprache.setzen(kuerzel)
    fenster = main_window.MainWindow()
    fenster.root.geometry(groesse)
    fenster.root.update_idletasks()
    for seite in SEITEN:
        try:
            fenster.open_page(seite)
        except Exception as ausnahme:
            treffer.append((groesse, kuerzel, seite,
                            'Seite baut nicht: %s' % ausnahme, 0))
            continue
        fenster.root.update_idletasks()
        fenster.root.update()
        gefunden = []
        _durchgehen(fenster.content, gefunden)
        for text, fehlt, art in gefunden:
            treffer.append((groesse, kuerzel, seite,
                            '%s (%s)' % (text, art), fehlt))
    fenster.root.destroy()


def pruefen(groessen=GROESSEN):
    treffer = []
    for groesse in groessen:
        for kuerzel in ('de', 'en'):
            _eine_runde(groesse, kuerzel, treffer)
    return treffer


if __name__ == '__main__':
    alle = pruefen()
    if not alle:
        print('Nichts wird abgeschnitten (%d Seiten × 2 Sprachen × '
              '%d Größen).' % (len(SEITEN), len(GROESSEN)))
        raise SystemExit(0)
    print('Wird abgeschnitten — so viele Pixel fehlen:\n')
    for groesse, kuerzel, seite, was, fehlt in alle:
        print('  [%s %s] %-12s %-42s +%d px'
              % (groesse, kuerzel, seite, was, fehlt))
    print('\n%d Fundstellen. Meist hilft _feld(..., breit=True): Das setzt '
          'breite\nBedienelemente unter die Beschreibung statt daneben.'
          % len(alle))
    raise SystemExit(1)
