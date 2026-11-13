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
Checks the Windows delivery end to end, on an invisible desktop of its own:

  1. installs the first check setup silently into a throwaway folder with awkward characters,
  2. starts the installed program on a throwaway data folder (SC_BP_HOME),
  3. starts the second check setup as the program starts an update: the setup itself, from the folder of
     VerseKit under %LOCALAPPDATA%, with the switches of the update run (/VKUPDATE and the files it reports
     to, in a data folder with a space and an umlaut); no copy of the program file, nothing started from the
     temporary folder. Once the setup has written its sign of life the program is ended as it ends itself
     before an update. Checks: the setup waited for the program, exit code 0 written, the lock removed, the
     version in "Apps & Features" is the second one, the setup started the program again exactly once;
     then the same update as at the end of the program (/VERYSILENT, /VKKEINSTART): exit code 0, the lock
     removed, and no program runs afterwards,
  4. runs the setup as the update helper of 5.2.2 and earlier does (without /VKUPDATE): the setup starts
     nothing, the file named under its key is started and runs once,
  5. uninstalls and checks that the entries are gone.

Only check setups (built with /DTestRun=1 and their own id, program name, start value and key; see
packaging/installer.iss): the real installation, its start menu, its start with Windows and a running real
program are never touched.

    python -I tools/ausliefern_pruefen.py dist/VerseKit-Setup-Pruefung1.exe dist/VerseKit-Setup-Pruefung2.exe

Prints one line per check and `ERGEBNIS gruen` or `ERGEBNIS rot`.
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

GUID = '2462E8F8-8D10-4C5A-84E6-7C7749A83D71'
PROGRAM = 'VerseKit-Pruefung.exe'
RUN_VALUE = 'VerseKit-Pruefung'
PROGRAM_KEY = r'Software\Xharig\VerseKit-Pruefung'
UNINSTALL_KEY = r'Software\Microsoft\Windows\CurrentVersion\Uninstall\{%s}_is1' % GUID
RUN_KEY = r'Software\Microsoft\Windows\CurrentVersion\Run'
NO_WINDOW = 0x08000000 | 0x00000200
# The command line of an update as `update_run::setup_arguments` writes it, and the sign of life of the setup
# (`update_run::LIFE_SIGN`). ⚠ Both sides change together: the Rust test
# `the_check_of_the_delivery_starts_the_setup_as_the_program_does` compares these lines with the program.
UPDATE_ARGUMENTS = ('/SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /DIR="{ziel}" /LOG="{setuplog}" '
                    '/VKUPDATE /VKPIDS={pids} /VKSPERRE="{sperre}" /VKERGEBNIS="{ergebnis}" '
                    '/VKPROTOKOLL="{protokoll}"')
LIFE_SIGN = 'Installer started'
# The switch of an update at the end of the program (`update_run::NO_START_SWITCH`): the setup starts nothing again.
NO_START = '/VKKEINSTART'

RESULTS = []


def check(ok, what):
    RESULTS.append(bool(ok))
    print('%s  %s' % ('ok  ' if ok else 'FEHLT', what), flush=True)
    return ok


def value(path, name):
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, path) as key:
            return winreg.QueryValueEx(key, name)[0]
    except OSError:
        return None


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
        print('     process %s %s' % (pid, path), flush=True)
        if path and os.path.normcase(os.path.dirname(path)) == os.path.normcase(folder):
            found.append(int(pid))
    return found


def end_program():
    subprocess.run(['taskkill', '/F', '/IM', PROGRAM], capture_output=True, creationflags=NO_WINDOW)


def inside(setup1, setup2):
    work = tempfile.mkdtemp(prefix='versekit-ausliefern-')
    target = os.path.join(work, "Programme & ^ % ! ( ) ' Ümlaut")
    # A data folder with a space and an umlaut: the setup reads the files of the update from its command line.
    data = os.path.join(work, 'Dokumente', 'Verse-Kit Ü')
    temp = os.path.join(work, 'temp')
    appdata = os.path.join(work, 'appdata')
    local = os.path.join(work, 'localappdata')
    for folder in (data, temp, appdata, local):
        os.makedirs(folder)
    # The program keeps files under %APPDATA% also with its own data folder: both point into the throwaway folder.
    env = dict(os.environ, SC_BP_HOME=data, SC_BP_NO_NET='1', TEMP=temp, TMP=temp, APPDATA=appdata,
               LOCALAPPDATA=local)
    try:
        if not check(not key_exists(UNINSTALL_KEY), 'no check installation left from before'):
            return
        # 1. Install.
        rc = subprocess.run([setup1, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART',
                             '/DIR=%s' % target, '/LOG=%s' % os.path.join(work, 'setup1.txt')],
                            env=env).returncode
        check(rc == 0, 'first setup installs silently (exit code %s)' % rc)
        program = os.path.join(target, PROGRAM)
        check(os.path.isfile(program), 'the program file has the name of the setup: %s' % PROGRAM)
        check(os.path.isfile(os.path.join(target, 'LICENSE.txt')), 'the licence lies next to it')
        extension = os.path.join(target, 'erweiterung')
        check(os.path.isfile(os.path.join(extension, 'hangar-exporter-0.5.0.xpi'))
              and os.path.isfile(os.path.join(extension, 'LICENSE.txt'))
              and os.path.isfile(os.path.join(extension, 'COPYRIGHT.txt'))
              and os.path.isfile(os.path.join(extension, 'QUELLE.txt')),
              'the Hangar-Exporter with licence, copyright and source lies in erweiterung')
        check(value(UNINSTALL_KEY, 'DisplayVersion') == '5.0.0-pruefung1', '"Apps & Features" shows the first version')
        check(value(UNINSTALL_KEY, 'DisplayName') == 'VerseKit Pruefung', 'and the name')
        check(value(PROGRAM_KEY, 'Programm') == program, 'the setup names its program file for the helper of 5.2.2 and earlier')
        check(value(RUN_KEY, RUN_VALUE) is None, 'no start with Windows without the task')

        # 2. The installed program runs (as it does when an update is handed over).
        running = subprocess.Popen([program], env=env, cwd=target)
        time.sleep(6)
        check(running.poll() is None, 'the installed program runs (on its own data folder)')

        # 3. The update as the program starts it (`update_run::setup_arguments`): the setup itself, from the
        #    folder of VerseKit under %LOCALAPPDATA%.
        fetched = os.path.join(local, 'VerseKit', 'Update')
        os.makedirs(fetched)
        setup = os.path.join(fetched, 'VerseKit-Setup.exe')
        shutil.copy(setup2, setup)
        lock = os.path.join(data, 'update-sperre.json')
        with open(lock, 'w', encoding='utf-8') as f:
            json.dump({'pid': 0, 'zeit': time.time()}, f)
        log_file = os.path.join(data, 'update-helfer.txt')
        result_file = os.path.join(data, 'update-ergebnis.txt')
        arguments = UPDATE_ARGUMENTS.format(
            ziel=target, setuplog=os.path.join(data, 'update-setup.txt'), pids=running.pid, sperre=lock,
            ergebnis=result_file, protokoll=log_file)
        started = time.time()
        updating = subprocess.Popen('"%s" %s' % (setup, arguments), env=env, cwd=fetched, creationflags=NO_WINDOW)
        # The program steps down only once the setup's own process has written its sign (`update_run::LIFE_SIGN`).
        signed = False
        while not signed and time.time() - started < 30 and updating.poll() is None:
            time.sleep(0.25)
            signed = os.path.exists(log_file) and LIFE_SIGN in open(log_file, encoding='utf-8', errors='replace').read()
        check(signed, 'the setup wrote its sign of life (after %.1f s)' % (time.time() - started))
        time.sleep(2)
        check(updating.poll() is None, 'the setup waits while the program runs')
        check(running.poll() is None, 'and does not close it before its time')
        # The program ends itself before an update; here it is ended.
        end_program()
        rc = updating.wait(timeout=300)
        check(rc == 0, 'the setup of the update ends with 0 (exit code %s, after %.0f s)' % (rc, time.time() - started))
        log = open(log_file, encoding='utf-8', errors='replace').read() if os.path.exists(log_file) else ''
        print('     update log:\n' + '\n'.join('       ' + line for line in log.splitlines()))
        check('Old version has exited' in log, 'the setup waited for the program to exit')
        result = open(result_file, encoding='ascii').read().strip() if os.path.exists(result_file) else ''
        check(result == '0', 'the setup wrote the exit code 0 (written: %r)' % result)
        check(not os.path.exists(lock), 'the lock is removed')
        check(value(UNINSTALL_KEY, 'DisplayVersion') == '5.0.0-pruefung2', '"Apps & Features" shows the second version')
        check(not any(name.lower().startswith('versekit') and name.lower().endswith('.exe') for name in os.listdir(temp)),
              'no program file of VerseKit in the temporary folder')
        time.sleep(4)
        again = processes(target)
        check(len(again) == 1, 'the setup started the program again from the installation folder, once (%s)' % again)
        check('Starting the program %s' % program in log, 'from the installation folder')

        # 3b. The update at the end of the program (`update_run::setup_arguments` with `at_exit`): the player ended
        #     the program, the setup runs without a window and does not start it again.
        shutil.copy(setup2, setup)
        with open(lock, 'w', encoding='utf-8') as f:
            json.dump({'pid': 0, 'zeit': time.time()}, f)
        offset = len(open(log_file, encoding='utf-8', errors='replace').read()) if os.path.exists(log_file) else 0
        waiting_for = processes(target) or [0]
        arguments = (UPDATE_ARGUMENTS.replace('/SILENT', '/VERYSILENT', 1) + ' ' + NO_START).format(
            ziel=target, setuplog=os.path.join(data, 'update-setup-ende.txt'),
            pids=','.join(str(pid) for pid in waiting_for), sperre=lock, ergebnis=result_file, protokoll=log_file)
        started = time.time()
        updating = subprocess.Popen('"%s" %s' % (setup, arguments), env=env, cwd=fetched, creationflags=NO_WINDOW)
        signed = False
        while not signed and time.time() - started < 30 and updating.poll() is None:
            time.sleep(0.25)
            text = open(log_file, encoding='utf-8', errors='replace').read() if os.path.exists(log_file) else ''
            signed = LIFE_SIGN in text[offset:]
        check(signed, 'at the end: the setup wrote its sign of life (after %.1f s)' % (time.time() - started))
        end_program()
        rc = updating.wait(timeout=300)
        check(rc == 0, 'at the end: the setup ends with 0 (exit code %s)' % rc)
        log = open(log_file, encoding='utf-8', errors='replace').read()[offset:] if os.path.exists(log_file) else ''
        print('     update log at the end:\n' + '\n'.join('       ' + line for line in log.splitlines()))
        check('not started again' in log, 'at the end: the setup says it starts nothing again')
        check(not os.path.exists(lock), 'at the end: the lock is removed')
        time.sleep(4)
        check(processes(target) == [], 'at the end: no program runs afterwards')

        # 4. The way of the helper of 5.2.2 and earlier: it waits until the program has ended, runs the setup
        #    without /VKUPDATE, then starts the file the setup names under its key. The setup itself starts nothing.
        end_program()
        time.sleep(1)
        rc = subprocess.run('"%s" /SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS /DIR="%s" /LOG="%s"'
                            % (setup2, target, os.path.join(work, 'setup-alt.txt')),
                            env=env, cwd=work, creationflags=NO_WINDOW).returncode
        check(rc == 0, 'the setup run as the old helper runs it ends with 0 (exit code %s)' % rc)
        time.sleep(4)
        check(processes(target) == [], 'without /VKUPDATE the setup starts no program')
        named = value(PROGRAM_KEY, 'Programm')
        check(named == program, 'the setup names its program file for the old helper (%s)' % named)
        if named:
            subprocess.Popen([named], env=env, cwd=target)
        time.sleep(4)
        again = processes(target)
        check(len(again) == 1, 'the old helper starts it, and it runs once (%s)' % again)
        end_program()
        time.sleep(1)

        # 5. Uninstall (the second setup is still installed).
        uninstaller = os.path.join(target, 'unins000.exe')
        rc = subprocess.run([uninstaller, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART'], env=env).returncode
        check(rc == 0, 'uninstalling runs silently (exit code %s)' % rc)
        for _ in range(30):
            if not os.path.exists(program):
                break
            time.sleep(0.5)
        check(not os.path.exists(program), 'the program file is gone')
        check(not key_exists(UNINSTALL_KEY), 'the entry in "Apps & Features" is gone')
        check(not key_exists(PROGRAM_KEY), 'the key of the program file is gone')
        check(value(RUN_KEY, RUN_VALUE) is None, 'no start with Windows is left')
        check(os.path.isdir(data), 'the data folder of the player is not touched')
    except Exception as error:
        check(False, 'the check broke off: %s: %s' % (type(error).__name__, error))
    finally:
        end_program()
        clean_up(work, env)


def clean_up(work, env):
    """Whatever a run broke off left: the check installation (by its uninstaller, else its two keys, which only
    a check setup writes) and the throwaway folder."""
    if key_exists(UNINSTALL_KEY):
        command = value(UNINSTALL_KEY, 'UninstallString') or ''
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
    remove_folder(work)


def remove_folder(folder):
    """The throwaway folder; files a process still holds for a moment get a few more tries."""
    for _ in range(20):
        shutil.rmtree(folder, ignore_errors=True)
        if not os.path.exists(folder):
            return
        time.sleep(0.5)
    print('     could not remove %s' % folder, flush=True)


def main(argv):
    if len(argv) >= 3 and argv[0] == '--innen':
        # The throwaway folders of earlier runs that could not be removed then.
        for name in os.listdir(tempfile.gettempdir()):
            if name.startswith('versekit-ausliefern-'):
                remove_folder(os.path.join(tempfile.gettempdir(), name))
        inside(os.path.abspath(argv[1]), os.path.abspath(argv[2]))
        ok = all(RESULTS) and RESULTS
        print('ERGEBNIS %s' % ('gruen' if ok else 'rot'), flush=True)
        return 0 if ok else 1
    if len(argv) != 2:
        print(__doc__)
        return 2
    return eigener_desktop.starten([sys.executable, '-I', os.path.abspath(__file__), '--innen'] + argv)


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
