# SPDX-License-Identifier: GPL-3.0-only
#
# VerseKit — zeigt live neue Star-Citizen-Baupläne an.
# Copyright (C) 2026 Xharig
#
# This program is free software: you can redistribute it and/or modify it under
# the terms of the GNU General Public License version 3 as published by the
# Free Software Foundation.
#
# This program is distributed in the hope that it will be useful, but WITHOUT
# ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
# FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
# details.
#
# You should have received a copy of the GNU General Public License along with
# this program.  If not, see <https://www.gnu.org/licenses/>.
"""
Hinweistexte in Eingabefeldern — **jedes** Feld sagt, was hineingehört.

## Warum es diese Datei gibt

Ein leeres Kästchen sagt nichts. Wer davorsitzt, weiß nicht, ob er einen
Bauplan, einen Auftrag, ein Schiff oder einen Ort eintippen soll — und
probiert. Deshalb gilt im ganzen Werkzeug: **In jedem Eingabefeld steht,
was man dort eingibt.**

## ⚠⚠ Warum der Hinweis KEIN eigenes Bauteil sein darf

Die erste Fassung legte ein `tk.Label` über das Feld. Das sieht gleich aus und
ist trotzdem falsch: Ein Label **fängt die Mausklicks ab**. Wer auf den
Hinweistext klickt, klickt auf das Label — nicht ins Feld. Die Schreibmarke
wandert nicht mit, und das Feld fühlt sich tot an.

> Gemeldet am 12.09.2026: *„wenn ich ins Feld klicken muss, muss ich zwingend
> neben den bestehenden Text klicken, da das Eingabefeld es sonst nicht
> erkennt — das ist alles andere als intuitiv."*

Ein Klick-Weiterreichen (`bind('<Button-1>', …focus_set)`) hilft nur halb: Es
holt den Tastaturfokus, aber der Klick selbst erreicht das Feld nie. Die
Schreibmarke bleibt, wo sie war.

## Wie es hier gelöst ist

Der Hinweis steht **im Feld selbst**. Es gibt kein zweites Bauteil, also auch
nichts, was einen Klick abfangen könnte.

⚠⚠ **Die Falle dabei — und der Grund, warum es früher ein Label war:** Ein
Suchfeld hängt an einer `StringVar`, und daran hängt der Filter. Stünde der
Hinweis als Wert darin, würde die Liste danach filtern und wäre beim Start
**leer**.

Deshalb wird die Variable währenddessen **abgehängt**
(`configure(textvariable='')`). Der Hinweis steht dann nur im Widget; die
Variable bleibt leer und wird kein einziges Mal beschrieben. Beim ersten
Tastendruck wird sie wieder angehängt.

Gemessen am 12.09.2026 mit Gegenprobe: Ohne das Abhängen sieht die Variable
den Hinweis sehr wohl — der Trick ist also nicht Zierde, sondern das Einzige,
was den Filter heil lässt.

## Warum er beim Fokus stehen bleibt

Er verschwindet erst beim **ersten Tastendruck**, nicht schon beim Hineinklicken.
Sonst wäre er genau dann weg, wenn man ihn braucht — und in einem Feld, das
beim Aufbau den Fokus bekommt (die Bauplan-Liste tut das), hätte man ihn nie
gesehen.
"""
import tkinter as tk

# Die Standardfarben der Oberfläche. Sie stehen absichtlich als Vorgabewerte
# hier und nicht fest im Code: Neun Dateien führen ihre eigene Kopie von
# `FG`/`SUB`, und dieser Baustein soll aus jeder davon benutzbar sein.
NORMAL = '#e6edf3'
GRAU = '#8b98a5'

# Tasten, die den Hinweis NICHT vertreiben — sie ändern den Inhalt nicht.
# ⚠ Ohne diese Liste verschwände der Hinweis schon beim Tabben oder beim
# Drücken von Umschalt, und das Feld stünde grundlos leer da.
_STILLE_TASTEN = frozenset((
    'Tab', 'ISO_Left_Tab', 'Shift_L', 'Shift_R', 'Control_L', 'Control_R',
    'Alt_L', 'Alt_R', 'Caps_Lock', 'Escape', 'Up', 'Down', 'Left', 'Right',
    'Home', 'End', 'Prior', 'Next', 'Win_L', 'Win_R', 'Super_L', 'Super_R',
))


def hinweis(feld, variable, text, normal=NORMAL, grau=GRAU):
    """Einen Hinweistext in ein Eingabefeld legen.

    `feld` ist das `tk.Entry`, `variable` seine `StringVar`, `text` der
    Hinweis. Rueckgabe ist eine Funktion, die sagt, ob der Hinweis gerade
    steht — der Aufrufer braucht sie selten, der Selbsttest schon.

    ⚠ Der Aufrufer muss nichts weiter tun: Solange er den Wert ueber die
    **Variable** liest (und nicht ueber `feld.get()`), bekommt er nie den
    Hinweis zu sehen.
    """
    # ⚠⚠ `sperre` ist nicht Zierde, sondern verhindert einen Kreis.
    #
    # `verstecken()` haengt die Variable wieder an. Genau das laesst Tk die
    # Beobachtung feuern — die sieht „Variable leer" und zeigt den Hinweis
    # sofort wieder an. Ergebnis: Der erste Tastendruck raeumte ihn weg und
    # holte ihn im selben Atemzug zurueck.
    #
    # Gefunden vom Selbsttest am 12.09.2026, nicht beim Lesen des Codes.
    zustand = {'an': False, 'sperre': False}

    def zeigen():
        if zustand['an'] or variable.get():
            return
        try:
            zustand['sperre'] = True
            feld.configure(textvariable='')
            feld.delete(0, 'end')
            feld.insert(0, text)
            feld.configure(fg=grau)
            zustand['an'] = True
        except tk.TclError:
            pass                      # Feld schon zerstoert (Seitenwechsel)
        finally:
            zustand['sperre'] = False

    def verstecken(_ereignis=None):
        if not zustand['an']:
            return
        try:
            zustand['sperre'] = True
            feld.delete(0, 'end')
            feld.configure(fg=normal, textvariable=variable)
            zustand['an'] = False
        except tk.TclError:
            pass
        finally:
            zustand['sperre'] = False

    def bei_taste(ereignis):
        # ⚠ Erst pruefen, DANN durchlassen: Diese Bindung laeuft vor der
        # Klassenbindung, die das Zeichen einsetzt. Wer hier nicht raeumt,
        # bekommt das Zeichen mitten in den Hinweistext geschrieben.
        if ereignis.keysym in _STILLE_TASTEN:
            return None
        verstecken()
        return None

    def bei_variable(*_args):
        """Das Programm setzt die Suche selbst — dann muss der Hinweis weg.

        ⚠⚠ **Ohne das bleibt der Hinweis stehen, waehrend die Liste schon
        filtert.** Denn solange er angezeigt wird, ist die Variable ja
        *abgehaengt*: Das Feld bekommt von `variable.set(...)` nichts mit.

        Genau so passiert beim Sprung zu einem Bauplan („Woher?" → Liste):
        `zum_bauplan()` setzt die Suche, die Liste zeigt einen Treffer — und
        im Feld stuende weiter „Bauplan oder Auftrag suchen".

        Gefunden vom Selbsttest (Pruefung 97) am 12.09.2026, nicht von Hand.

        ⚠ Und der Rueckweg gehoert dazu: Leert das Programm die Suche (das ✕
        im Feld tut genau das), muss der Hinweis **sofort** wieder da sein —
        nicht erst beim naechsten Fokuswechsel. Sonst steht der Nutzer vor
        einem leeren Kasten, und das war der Ausgangszustand, den diese ganze
        Datei behebt.
        """
        if zustand['sperre']:
            return                    # wir selbst schalten gerade um
        if variable.get():
            verstecken()
        else:
            zeigen()

    feld.bind('<Key>', bei_taste, add='+')
    feld.bind('<FocusOut>', lambda _e: zeigen(), add='+')
    variable.trace_add('write', bei_variable)
    zeigen()
    return lambda: zustand['an']
