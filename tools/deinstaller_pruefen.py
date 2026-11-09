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
Checks the uninstaller end to end, on an invisible desktop of its own:

  1. installs the check setup silently into a throwaway folder; Setup with a process list it cannot read
     (/VKLISTEFEHLT) installs as always; Setup with a program it cannot end (/VKLAEUFTNOCH) stops and changes
     nothing; the uninstaller (silent and very silent) stops and changes nothing with either, and with a program
     that came back after its first check (/VKLAEUFTWIEDER; all three check setups only); then starts the
     installed program and uninstalls silently while it runs: the uninstaller ends it, its file and the folder are
     gone, the copy of the data is untouched (a silent uninstall never removes data),
  2. installs again into the same folder (the new installation after an uninstall), starts the program, and
     uninstalls with the answer "yes" to the question about the data (/VKDATENLOESCHEN, check setups only) on
     the copy of the data named by /VKDATEN: the own files of the copy are gone, the foreign files stay.

Only the check setup of this script (its own id, program name, start value and key) and a copy of the data in
the throwaway folder: the real installation and the real data are never touched. The setup:

    ISCC /DTestRun=1 "/DAppName=VerseKit Deinstallpruefung" /DAppVersion=5.0.0-deinstall
         /DAppIdGuid=5B0D3C7E-91A4-4F62-8E15-3C9A7D2B6F40 /DProgramFile=VerseKit-Deinstallpruefung.exe
         /DRunValue=VerseKit-Deinstallpruefung "/DProgramKey=Software\\Xharig\\VerseKit-Deinstallpruefung"
         /DOutputName=VerseKit-Setup-Deinstallpruefung packaging\\installer.iss

    python -I tools/deinstaller_pruefen.py dist/VerseKit-Setup-Deinstallpruefung.exe

Prints one line per check and `ERGEBNIS gruen` or `ERGEBNIS rot`. A setup built from an installer that does not
end the program or does not remove the data must give `ERGEBNIS rot` (the counter-check).
"""
import json
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

GUID = '5B0D3C7E-91A4-4F62-8E15-3C9A7D2B6F40'
PROGRAM = 'VerseKit-Deinstallpruefung.exe'
PROGRAM_KEY = r'Software\Xharig\VerseKit-Deinstallpruefung'
UNINSTALL_KEY = r'Software\Microsoft\Windows\CurrentVersion\Uninstall\{%s}_is1' % GUID
NO_WINDOW = 0x08000000 | 0x00000200

# The copy of the data: own files of the program, and foreign ones beside them that must stay.
OWN = ['VerseKit-Daten/Bauplaene/bestand.json', 'VerseKit-Daten/Einstellungen/einstellungen.json',
       'VerseKit-Daten/Intern/katalog-cache.json', 'VerseKit-Daten/Diagnose/fehler.json',
       'VerseKit-Daten/export/scmdb-import.json', 'AppData/sc-bp-watcher/hochwasser.json']
FOREIGN = ['VerseKit-Daten/Urlaub.jpg', 'VerseKit-Daten/export/meine-liste.xlsx', 'VerseKit-Daten/Intern/eigene-notiz.txt',
           'Brief.docx']

RESULTS = []


def check(ok, what):
    RESULTS.append(bool(ok))
    print('%s  %s' % ('ok  ' if ok else 'FEHLT', what), flush=True)
    return ok


def key_exists(path):
    try:
        winreg.OpenKey(winreg.HKEY_CURRENT_USER, path).Close()
        return True
    except OSError:
        return False


def processes(folder):
    """Process ids of the check program file in `folder` (by its image path)."""
    out = subprocess.run(
        ['powershell', '-NoProfile', '-Command',
         "[Console]::OutputEncoding = [Text.Encoding]::UTF8; "
         "Get-Process -Name '%s' -ErrorAction SilentlyContinue | ForEach-Object { \"$($_.Id)|$($_.Path)\" }"
         % PROGRAM[:-4]],
        capture_output=True, encoding='utf-8', errors='replace', creationflags=NO_WINDOW).stdout
    found = []
    for line in out.splitlines():
        pid, _, path = line.strip().partition('|')
        if path and os.path.normcase(os.path.dirname(path)) == os.path.normcase(folder):
            found.append(int(pid))
    return found


def end_program():
    subprocess.run(['taskkill', '/F', '/IM', PROGRAM], capture_output=True, creationflags=NO_WINDOW)


def wait_until(condition, seconds=30):
    end = time.time() + seconds
    while time.time() < end:
        if condition():
            return True
        time.sleep(0.5)
    return condition()


def make_copy(copy):
    """The copy of the data, with both pointers pointing at its data folder."""
    data = os.path.join(copy, 'VerseKit-Daten')
    for rel in OWN + FOREIGN:
        path = os.path.join(copy, *rel.split('/'))
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, 'w', encoding='utf-8') as f:
            if rel.endswith('/scmdb-import.json'):
                # An export file goes only with the program's mark in it.
                f.write(json.dumps({'exportSchemaVersion': 1, 'productName': 'VerseKit'}))
            else:
                f.write('{}' if rel.endswith('.json') else 'fremd')
    pointer = json.dumps({'ablage_ordner': data})
    for rel in ('AppData/sc-bp-watcher/ablage.json', 'Verse-Kit/Einstellungen/einstellungen.json'):
        path = os.path.join(copy, *rel.split('/'))
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, 'w', encoding='utf-8') as f:
            f.write(pointer)


def listing(root):
    out = []
    for folder, dirs, files in os.walk(root):
        for name in dirs + files:
            out.append(os.path.relpath(os.path.join(folder, name), root).replace('\\', '/'))
    return sorted(out)


def install(setup, target, env, log):
    rc = subprocess.run([setup, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/DIR=%s' % target,
                         '/LOG=%s' % log], env=env).returncode
    program = os.path.join(target, PROGRAM)
    check(rc == 0 and os.path.isfile(program), 'the setup installs silently (exit code %s)' % rc)
    return program


def run_program(program, target, env):
    subprocess.Popen([program], env=env, cwd=os.path.dirname(target))
    return check(wait_until(lambda: len(processes(target)) >= 1, 15), 'the installed program runs')


def uninstall(target, env, extra, log):
    uninstaller = os.path.join(target, 'unins000.exe')
    rc = subprocess.run([uninstaller, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/LOG=%s' % log] + extra,
                        env=env, timeout=300).returncode
    check(rc == 0, 'uninstalling runs silently while the program runs (exit code %s)' % rc)
    check(wait_until(lambda: not processes(target)), 'the uninstaller ended the program')
    gone = check(wait_until(lambda: not os.path.exists(os.path.join(target, PROGRAM))), 'the program file is gone')
    if not gone and os.path.exists(log):
        with open(log, encoding='utf-8', errors='replace') as f:
            lines = [line.rstrip() for line in f if 'fail' in line.lower() or 'code' in line.lower()]
        print('     uninstall log:\n' + '\n'.join('       ' + line for line in lines[-12:]), flush=True)
    check(wait_until(lambda: not os.path.exists(target)), 'the installation folder is gone')
    check(wait_until(lambda: not key_exists(UNINSTALL_KEY)), 'the entry in "Apps & Features" is gone')
    check(not key_exists(PROGRAM_KEY), 'the key of the program file is gone')


def installed_files(target):
    """The files of the installation with their size and time, to see that a stopped run changed nothing."""
    out = {}
    for folder, _, files in os.walk(target):
        for name in files:
            path = os.path.join(folder, name)
            info = os.stat(path)
            out[os.path.relpath(path, target)] = (info.st_size, info.st_mtime_ns)
    return out


def stopped_runs(setup, target, program_env, uninstall_env, work):
    """A program that cannot be ended (`/VKLAEUFTNOCH`, check setups only): Setup and the uninstaller stop before
    anything is changed, silent and very silent."""
    def setup_run(flag):
        return subprocess.run([setup, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/DIR=%s' % target,
                               '/LOG=%s' % os.path.join(work, 'setup%s.txt' % flag[1:]), flag],
                              env=program_env, timeout=300).returncode

    # A process list that cannot be read does not stop Setup (nor the update): it installs as always.
    rc = setup_run('/VKLISTEFEHLT')
    check(rc == 0, 'Setup goes on while the process list cannot be read (exit code %s)' % rc)
    before = installed_files(target)
    rc = setup_run('/VKLAEUFTNOCH')
    check(rc != 0, 'Setup stops while the program cannot be ended (exit code %s)' % rc)
    check(installed_files(target) == before, 'and changed no file of the installation')
    cases = [('/VKLAEUFTNOCH', 'the program cannot be ended'), ('/VKLISTEFEHLT', 'the process list cannot be read'),
             ('/VKLAEUFTWIEDER', 'the program came back after the first check and cannot be ended')]
    for flag, why in cases:
        for mode in ('/VERYSILENT', '/SILENT'):
            rc = subprocess.run([os.path.join(target, 'unins000.exe'), mode, '/SUPPRESSMSGBOXES', '/NORESTART',
                                 '/LOG=%s' % os.path.join(work, 'uninstall-steht%s%s.txt' % (mode[1:], flag[1:])),
                                 flag], env=uninstall_env, timeout=300).returncode
            time.sleep(3)
            check(rc != 0, 'the uninstaller (%s) stops while %s (exit code %s)' % (mode, why, rc))
            check(installed_files(target) == before, 'and removed no file')
            check(key_exists(UNINSTALL_KEY) and key_exists(PROGRAM_KEY), 'and no entry')


def inside(setup):
    work = tempfile.mkdtemp(prefix='versekit-deinstaller-')
    target = os.path.join(work, "Programme Ü")
    copy = os.path.join(work, 'Kopie der Daten')
    running = os.path.join(work, 'laufdaten')
    machine = os.path.join(work, 'maschine')
    temp = os.path.join(work, 'temp')
    for folder in (copy, running, machine, temp):
        os.makedirs(folder)
    make_copy(copy)
    before = listing(copy)
    # The running program on a data folder of its own (no start with Windows, no report); the uninstaller and the
    # removal it starts on a machine of their own below the throwaway folder, without the registry.
    base = dict(os.environ, SC_BP_NO_NET='1', TEMP=temp, TMP=temp)
    base.pop('SC_BP_HOME', None)
    program_env = dict(base, SC_BP_HOME=running)
    uninstall_env = dict(base, VERSEKIT_TESTUMGEBUNG=machine)
    try:
        if not check(not key_exists(UNINSTALL_KEY), 'no check installation left from before'):
            return
        # 1. Uninstall silently while the program runs: no question, so no data removed.
        program = install(setup, target, program_env, os.path.join(work, 'setup1.txt'))
        stopped_runs(setup, target, program_env, uninstall_env, work)
        if run_program(program, target, program_env):
            uninstall(target, uninstall_env, ['/VKDATEN=%s' % copy], os.path.join(work, 'uninstall1.txt'))
        check(listing(copy) == before, 'a silent uninstall leaves the data as they were')

        # 2. Install again into the same folder, and uninstall with the answer "yes" about the data.
        program = install(setup, target, program_env, os.path.join(work, 'setup2.txt'))
        if run_program(program, target, program_env):
            uninstall(target, uninstall_env, ['/VKDATENLOESCHEN', '/VKDATEN=%s' % copy],
                      os.path.join(work, 'uninstall2.txt'))
        left = listing(copy)
        print('     left in the copy: %s' % left, flush=True)
        check(all(rel not in left for rel in OWN), 'the own files of the copy are gone')
        check(all(rel in left for rel in FOREIGN), 'the foreign files of the copy stay')
        check('Verse-Kit' not in left and 'AppData/sc-bp-watcher' not in left,
              'the pointer and the configuration folder of the copy are gone')
        check(listing(machine) == [], 'nothing written outside the copy: %s' % listing(machine))
    except Exception as error:
        check(False, 'the check broke off: %s: %s' % (type(error).__name__, error))
    finally:
        end_program()
        clean_up(work, uninstall_env)


def clean_up(work, env):
    """Whatever a run broke off left: the check installation and the throwaway folder."""
    if key_exists(UNINSTALL_KEY):
        command = ''
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, UNINSTALL_KEY) as key:
                command = winreg.QueryValueEx(key, 'UninstallString')[0]
        except OSError:
            pass
        uninstaller = command.strip('"')
        if os.path.isfile(uninstaller):
            subprocess.run([uninstaller, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART'], env=env)
            time.sleep(2)
    for path in (UNINSTALL_KEY, PROGRAM_KEY):
        if key_exists(path):
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
