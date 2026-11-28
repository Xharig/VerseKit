# -*- coding: utf-8 -*-
# VerseKit — companion for Star Citizen.
# Copyright (C) 2026 Xharig
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
Checks that Setup goes on when it may not make the shortcuts or the entry in "Apps & Features" (access denied, as
an antivirus or a lock on exactly that key does), on an invisible desktop of its own:

  1. the folder of the shortcuts (/VKVERKNUEPFUNGEN) lies below a file, so no shortcut can be saved there (a real
     failure of that step, without changing any rights), and the check of the entry writes where Windows refuses
     (/VKEINTRAGGESPERRT=1, check setups only): Setup installs silently all the same (exit code 0), the program
     file and the uninstaller are there, no entry, one message only (the summary, not one error box per shortcut),
     and the log names what was left out and what to use instead; the uninstaller in the folder removes the
     installation,
  2. without either: the shortcuts and the entry are made, the log names nothing left out; uninstalled again.

Only the check setup of this script (its own id, name, program, start value and key) and a throwaway folder: the
real installation, the real start menu and the real entry are never touched, and no rights are changed. The setup:

    ISCC /DTestRun=1 "/DAppName=VerseKit Setupzugriff" /DAppVersion=5.6.4-zugriff
         /DAppIdGuid=3E8A61C2-5D07-4B9F-A2C4-71E0D93B58F6 /DProgramFile=VerseKit-Setupzugriff.exe
         /DRunValue=VerseKit-Setupzugriff "/DProgramKey=Software\\Xharig\\VerseKit-Setupzugriff"
         /DSourceExe=<any small program> /DExtensionDir=<a folder with a file>
         /DOutputName=VerseKit-Setup-Setupzugriff packaging\\installer.iss

    python -I tools/setup_zugriff_pruefen.py dist/VerseKit-Setup-Setupzugriff.exe

Prints one line per check and `ERGEBNIS gruen` or `ERGEBNIS rot`. A setup built from an installer that stops on a
denied shortcut or entry must give `ERGEBNIS rot` (the counter-check).
"""
import os
import shutil
import subprocess
import sys
import tempfile
import time
import winreg

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import eigener_desktop  # noqa: E402

GUID = '3E8A61C2-5D07-4B9F-A2C4-71E0D93B58F6'
PROGRAM = 'VerseKit-Setupzugriff.exe'
PROGRAM_KEY = r'Software\Xharig\VerseKit-Setupzugriff'
UNINSTALL_KEY = r'Software\Microsoft\Windows\CurrentVersion\Uninstall\{%s}_is1' % GUID
NO_WINDOW = 0x08000000 | 0x00000200
LEFT_OUT = 'Left out for lack of access'

RESULTS = []


def check(ok, what):
    RESULTS.append(bool(ok))
    print('%s  %s' % ('ok  ' if ok else 'FEHLT', what), flush=True)
    return ok


def key_values(path):
    """The names of the values of a key of the current user; None when the key is not there."""
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, path) as key:
            names, i = [], 0
            while True:
                try:
                    names.append(winreg.EnumValue(key, i)[0])
                except OSError:
                    return names
                i += 1
    except OSError:
        return None


def log_text(log):
    try:
        with open(log, encoding='utf-8', errors='replace') as f:
            return f.read()
    except OSError:
        return ''


def install(setup, target, shortcuts, log, extra=()):
    return subprocess.run([setup, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/DIR=%s' % target,
                           '/VKVERKNUEPFUNGEN=%s' % shortcuts, '/LOG=%s' % log] + list(extra), timeout=300).returncode


def uninstall(target, log):
    uninstaller = os.path.join(target, 'unins000.exe')
    if not os.path.isfile(uninstaller):
        return None
    rc = subprocess.run([uninstaller, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/LOG=%s' % log],
                        timeout=300).returncode
    end = time.time() + 30
    while os.path.exists(os.path.join(target, PROGRAM)) and time.time() < end:
        time.sleep(0.5)
    return rc


def links(folder):
    return sorted(n for n in os.listdir(folder) if n.lower().endswith('.lnk')) if os.path.isdir(folder) else []


def denied(setup, work):
    target = os.path.join(work, 'Programme gesperrt')
    blocker = os.path.join(work, 'keine-ordner.txt')
    with open(blocker, 'w', encoding='utf-8') as f:
        f.write('a file, so no folder can be made below it')
    shortcuts = os.path.join(blocker, 'Startmenue')
    log = os.path.join(work, 'setup-gesperrt.txt')
    rc = install(setup, target, shortcuts, log, ['/VKEINTRAGGESPERRT=1'])
    text = log_text(log)
    check(rc == 0, 'Setup goes on and ends well while both are denied (exit code %s)' % rc)
    check(os.path.isfile(os.path.join(target, PROGRAM)), 'the program file is installed')
    check(os.path.isfile(os.path.join(target, 'unins000.exe')), 'the uninstaller is in the folder')
    check(key_values(UNINSTALL_KEY) is None, 'no entry in "Apps & Features"')
    # Without the guard Windows' refusal of each shortcut comes as an error message of its own (three boxes the
    # player must answer); with it the only message is the summary at the end.
    boxes = text.count('suppressed message box')
    check(boxes == 1, 'one message only, the summary at the end (%s message boxes)' % boxes)
    check(LEFT_OUT in text, 'the log names what was left out')
    check('Apps & Features' in text and 'unins000.exe' in text and PROGRAM in text,
          'with the uninstaller and the program file to use instead')
    if rc != 0:
        lines = [line.rstrip() for line in text.splitlines() if 'error' in line.lower() or 'code' in line.lower()
                 or 'fail' in line.lower()]
        print('     setup log:\n' + '\n'.join('       ' + line for line in lines[-12:]), flush=True)
    rc = uninstall(target, os.path.join(work, 'uninstall-gesperrt.txt'))
    check(rc == 0 and not os.path.exists(os.path.join(target, PROGRAM)),
          'the uninstaller in the folder removes the installation (exit code %s)' % rc)


def free(setup, work):
    target = os.path.join(work, 'Programme frei')
    shortcuts = os.path.join(work, 'Startmenue frei')
    log = os.path.join(work, 'setup-frei.txt')
    rc = install(setup, target, shortcuts, log)
    check(rc == 0, 'Setup without a lock ends well (exit code %s)' % rc)
    check(len(links(shortcuts)) == 2, 'both shortcuts are made: %s' % links(shortcuts))
    values = key_values(UNINSTALL_KEY) or []
    check('DisplayName' in values and 'VerseKitSchreibprobe' not in values,
          'the entry is made, without the value of the check')
    text = log_text(log)
    check(LEFT_OUT not in text and 'suppressed message box' not in text, 'the log names nothing left out, no message')
    rc = uninstall(target, os.path.join(work, 'uninstall-frei.txt'))
    check(rc == 0 and key_values(UNINSTALL_KEY) is None, 'uninstalled, entry gone (exit code %s)' % rc)


def inside(setup):
    work = tempfile.mkdtemp(prefix='versekit-setupzugriff-')
    try:
        if not check(key_values(UNINSTALL_KEY) is None and key_values(PROGRAM_KEY) is None,
                     'no check installation left from before'):
            return
        denied(setup, work)
        free(setup, work)
    except Exception as error:
        check(False, 'the check broke off: %s: %s' % (type(error).__name__, error))
    finally:
        subprocess.run(['taskkill', '/F', '/IM', PROGRAM], capture_output=True, creationflags=NO_WINDOW)
        clean_up(work)


def clean_up(work):
    """Whatever a run broke off left: the keys of the check setup and the throwaway folder."""
    for path in (UNINSTALL_KEY, PROGRAM_KEY):
        if key_values(path) is not None:
            try:
                winreg.DeleteKey(winreg.HKEY_CURRENT_USER, path)
                print('     removed what the run left: HKCU\\%s' % path, flush=True)
            except OSError as error:
                print('     could not remove HKCU\\%s: %s' % (path, error), flush=True)
    for _ in range(20):
        shutil.rmtree(work, ignore_errors=True)
        if not os.path.exists(work):
            return
        time.sleep(0.5)
    print('     could not remove %s' % work, flush=True)


def main(argv):
    if len(argv) == 2 and argv[0] == '--innen':
        inside(os.path.abspath(argv[1]))
        ok = all(RESULTS) and RESULTS
        print('ERGEBNIS %s' % ('gruen' if ok else 'rot'), flush=True)
        return 0 if ok else 1
    if len(argv) != 1:
        print(__doc__)
        return 2
    return eigener_desktop.starten([sys.executable, '-I', os.path.abspath(__file__), '--innen'] + argv)


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
