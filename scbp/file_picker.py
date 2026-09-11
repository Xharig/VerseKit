# -*- coding: utf-8 -*-
"""Datei- und Ordnerauswahl, die nach dem System aussieht, auf dem sie läuft.

⚠ **Warum es dieses Modul gibt.** `tkinter.filedialog` zeichnet unter Linux
seinen **eigenen** Dialog — den alten Motif-Kasten: eine Spaltenliste mit jedem
versteckten Ordner (`.cache`, `.pki`, `.var`), kein Sortieren, keine Vorschau,
kein „zuletzt benutzt". Unter **Windows und macOS** reicht Tk dagegen den echten
Systemdialog durch; dort ist alles in Ordnung und dieses Modul greift nicht ein.

Unter Linux gibt es zwei verbreitete Helfer, die den **echten** Dialog des
Schreibtischs öffnen: `kdialog` (KDE Plasma) und `zenity` (GNOME, liegt aber auf
fast jedem System). Ist keiner da, bleibt der Tk-Dialog als Rückfall — hässlich,
aber funktionierend ist besser als gar nichts. **Nichts hängt davon ab.**

⚠ **Warum dieses Modul überhaupt entstand, obwohl es die Ordnerwahl schon gab:**
Für Ordner stand der ganze Ablauf bereits in `seiten.py` — für das Öffnen und
Speichern von **Dateien** aber nicht, dort lief weiterhin `filedialog`. Beim
Vorführen des Werkzeugs fiel es auf (gemeldet 27.08.2026: „bei Datei wählen
kommt auch diese hässliche unübersichtliche Ordner-Auswahl"). Die drei Wege
gehören zusammen und stehen deshalb jetzt an **einer** Stelle statt an dreien.

⚠ Bis zum 11.09.2026 hieß dieses Modul `dateiwahl` (Sprachumstellung P3):
`ordner_waehlen` → `choose_folder`, `datei_oeffnen` → `open_file`,
`datei_speichern` → `save_file`. **Auch die Schlüsselwörter sind umbenannt**
(`vorschlag` → `suggestion`, `endung` → `extension`, `muster` → `patterns`,
`titel` → `title`) — und genau das ist die Stelle, an der ein vergessener
Aufrufer erst beim Klick auf „Speichern" auffällt, weil kein Prüflauf diese
Dialoge öffnet. Die Aufrufe wurden deshalb beim Umbenennen maschinell gegen
die neuen Signaturen geprüft.
"""

import os
import shutil
import subprocess
import sys

from . import fehler

# Wie lange ein Dialog offen stehen darf, bevor aufgegeben wird. Großzügig: Es
# sitzt ein Mensch davor, der sucht.
PATIENCE = 600

# Auf diesen Systemen ruft Tk den echten Systemdialog auf — Finger weg.
TK_IS_NATIVE = sys.platform.startswith(('win', 'darwin'))


def clean_environment():
    """Weiterleitung — die Wahrheit steht in `pfade`.

    ⚠ Sie stand hier, weil die Dateiauswahl sie zuerst brauchte. Am 27.08.2026
    stellte sich heraus, dass der **Neustart nach einem Update** dieselbe Wäsche
    braucht und eine eigene, unvollständige Version mitführte — mit dem Ergebnis,
    dass sich das Werkzeug unter Linux nicht selbst neu starten konnte. Eine
    Wäsche an einer Stelle, benutzt von allen.
    """
    from . import pfade
    return pfade.saubere_umgebung()


def _on_path(name):
    """Gibt es dieses Programm auf dem Rechner?"""
    return bool(shutil.which(name))


def _try_helpers(commands, origin):
    """Die Helfer der Reihe nach fragen. Gibt den Pfad, '' oder None zurück.

    * **Pfad** — der Nutzer hat etwas gewählt.
    * **`''`** — er hat bewusst abgebrochen; damit ist die Sache erledigt.
    * **`None`** — kein Helfer kam durch; der Aufrufer nimmt den Tk-Dialog.

    ⚠ Rückgabecodes auseinanderhalten: **1 heißt „abgebrochen"** und ist eine
    gültige Antwort. Jeder andere Code heißt, das Werkzeug selbst ist
    gescheitert — dann wird der nächste versucht. Vorher galt beides als
    Abbruch, und ein im AppImage abgestürztes `zenity` sah aus wie ein Knopf
    ohne Funktion.
    """
    if TK_IS_NATIVE:
        return None
    environment = clean_environment()
    for command in commands:
        if not _on_path(command[0]):
            continue
        try:
            done = subprocess.run(command, capture_output=True, text=True,
                                  timeout=PATIENCE, env=environment)
        except Exception as exc:
            fehler.merken('%s:%s' % (origin, command[0]), exc)
            continue
        chosen = (done.stdout or '').strip()
        if done.returncode == 0 and chosen:
            return chosen
        if done.returncode == 1:
            return ''                      # bewusst abgebrochen
        fehler.merken('%s:%s' % (origin, command[0]),
                      RuntimeError('Code %s: %s' % (done.returncode,
                                                    (done.stderr or '')[:200])))
    return None


def _kdialog_filter(patterns):
    """Tk-Muster `(('JSON', '*.json'), …)` in die Schreibweise von kdialog."""
    return ' '.join(m for _n, m in patterns) + '|' + \
           ' '.join(n for n, _m in patterns)


def choose_folder(title, start=None):
    """Einen Ordner auswählen lassen. Gibt den Pfad oder '' zurück."""
    answer = _try_helpers([
        ['kdialog', '--getexistingdirectory',
         start or os.path.expanduser('~'), '--title', title],
        ['zenity', '--file-selection', '--directory', '--title', title]
        + (['--filename', start.rstrip('/') + '/'] if start else []),
    ], 'file_picker.folder')
    if answer is not None:
        return answer
    from tkinter import filedialog
    return filedialog.askdirectory(title=title, initialdir=start or None) or ''


def open_file(title, patterns=(('JSON', '*.json'),), start=None):
    """Eine vorhandene Datei auswählen lassen. Gibt den Pfad oder '' zurück."""
    zenity = ['zenity', '--file-selection', '--title', title]
    for name, m in patterns:
        zenity.append('--file-filter=%s | %s' % (name, m))
    if start:
        zenity += ['--filename', start.rstrip('/') + '/']
    answer = _try_helpers([
        ['kdialog', '--getopenfilename', start or os.path.expanduser('~'),
         _kdialog_filter(patterns), '--title', title],
        zenity,
    ], 'file_picker.open')
    if answer is not None:
        return answer
    from tkinter import filedialog
    return filedialog.askopenfilename(title=title,
                                      filetypes=list(patterns)) or ''


def save_file(title, suggestion='', extension='.json', start=None,
              patterns=(('JSON', '*.json'),)):
    """Einen Speicherort auswählen lassen. Gibt den Pfad oder '' zurück.

    ⚠ Die Endung wird **nachgetragen**, wenn der Nutzer keine tippt. Tk erledigt
    das über `defaultextension` von allein, `kdialog` und `zenity` nicht — ohne
    diesen Schritt entstünde eine Datei ohne Endung, die hinterher kein Programm
    mehr als JSON erkennt.
    """
    place = os.path.join(start or os.path.expanduser('~'), suggestion) \
        if suggestion else (start or os.path.expanduser('~'))
    answer = _try_helpers([
        ['kdialog', '--getsavefilename', place, _kdialog_filter(patterns),
         '--title', title],
        ['zenity', '--file-selection', '--save', '--confirm-overwrite',
         '--title', title, '--filename', place],
    ], 'file_picker.save')
    if answer is None:
        from tkinter import filedialog
        answer = filedialog.asksaveasfilename(
            title=title, initialfile=suggestion, defaultextension=extension,
            filetypes=list(patterns)) or ''
    if answer and extension and not answer.lower().endswith(extension.lower()):
        answer += extension
    return answer
