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
Messungen für das Ein-Klick-Update unter Windows (Paket P1b-1).

⚠⚠ **Über Inno Setup wird nichts geglaubt, was nicht gemessen wurde.** Fünf
Anläufe am Windows-Update sind daran gescheitert, dass eine plausible
Erklärung ungeprüft übernommen wurde. Dieses Werkzeug baut nichts am Programm
— es stellt Fragen an einen echten Installer und schreibt die Antworten auf.

**Wo es läuft:** im Bau-Ablauf `messung-p1b.yml` auf `windows-latest`. Dort
verhält sich Inno Setup genauso wie bei jedem Nutzer. **Nicht** messbar ist
dort, was an einem echten Arbeitsplatz hängt: der Echtzeitschutz von Defender
(auf den Bau-Rechnern abgeschaltet), ein Klick auf „Abbrechen", eine Anmeldung
mitten im Update. Diese Fragen bleiben für den Windows-Rechner des Autors.

**Was gemessen wird**

  M1  Rückgabewert bei Erfolg und bei einem echten Fehler
  M2  Lebt der gestartete Setup-Prozess bis zum Ende — oder übergibt er?
  M3  Stört ein lebender, ein sofort sterbender oder ein mit Kompatibilitäts-
      Kennung versehener Elternprozess?
  M4  Schließt `CloseApplications=force` den laufenden Watcher, wenn das
      Setup aus einem Helfer in %TEMP% startet?
  M5  Ist der Port der Einzelinstanz-Sperre nach hartem Beenden sofort frei?
  M6  Verhindert die Sperre unter Windows überhaupt eine zweite Bindung?
      (Sie bindet mit SO_REUSEADDR — das bedeutet unter Windows etwas
      anderes als unter Linux. Gemessen, nicht angenommen.)
  M7  Überleben Pfade mit `& ^ % ! ( ) '`, Umlauten und großer Länge den
      Aufruf über `cmd`?

**Aufruf**

    python tools/messung_p1b.py --setup PFAD --exe PFAD --out ORDNER
    python tools/messung_p1b.py --trocken      # nur Aufbau und Drift prüfen

⚠ **Der Installer-Aufruf ist hier nachgebildet**, weil er im Programm mitten
in `aktualisierung.einspielen()` steht und nicht einzeln aufrufbar ist. Damit
er nicht still wegdriftet, prüft `drift_check()` vor jeder Messung, ob die
Aufrufzeilen im Programm noch wörtlich so lauten. Tun sie es nicht, gilt die
Messung nicht mehr — und das steht dann im Bericht.
"""
import argparse
import json
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
GUARD_PORT = 47913          # scbp/overlay.py: WAECHTER_PORT
TIMEOUT = 180               # Sekunden je Installer-Lauf

# Die Zeilen aus `aktualisierung.einspielen()`, die hier nachgebildet sind.
# Stehen sie dort nicht mehr wörtlich, misst dieses Werkzeug etwas anderes als
# das, was beim Nutzer läuft.
EXPECTED_IN_PROGRAM = (
    "schalter = '/SILENT /NORESTART /CLOSEAPPLICATIONS'",
    "schalter += ' /DIR=\"%s\"' % eigener_ordner",
    "schalter += ' /LOG=\"%s\"' % protokoll_datei",
    "befehl = 'cmd /c \"\"%s\" %s\"' % (neue_datei, schalter)",
    "umgebung.pop('__COMPAT_LAYER', None)",
)

# Pfadnamen, an denen `cmd` scheitern könnte. ⚠ Nicht nur Leerzeichen: `&`
# trennt Befehle, `^` ist das Fluchtzeichen, `%` und `!` leiten Variablen ein,
# Klammern gliedern Blöcke.
AWKWARD_NAMES = (
    'mit leer zeichen',
    'a&b',
    'a^b',
    'hundert 100%',
    'a!b',
    'a(b)c',
    "o'reilly",
    'umlaut äöü',
    'lang_' + 'x' * 110,
)

INNO_STAMP = re.compile(r'^(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d\.\d{3})')

# Eine Ablage, in der das Werkzeug schon eingerichtet ist.
# ⚠⚠ Ohne sie bleibt der gebaute Watcher im Einrichtungsassistenten stehen und
# wartet auf einen Klick, den im Bau-Ablauf niemand macht. Der Instanz-Wächter
# startet aber erst danach (`Overlay.run()`, direkt vor `mainloop()`) — der
# Port wird nie geöffnet. Genau so im ersten Lauf am 11.09.2026: M4, M5 und M6
# liefen grün durch und hatten nichts gemessen.
SEEDED_SETTINGS = {'einrichtung_fertig': True, 'einrichtung_ohne_spiel': True}
WATCHER_WAIT = 60           # Sekunden, bis der Watcher lauschen muss


# ------------------------------------------------------------------ Grundlagen

def drift_check():
    """Stehen die nachgebildeten Aufrufzeilen noch wörtlich im Programm?"""
    path = os.path.join(ROOT, 'scbp', 'aktualisierung.py')
    text = open(path, encoding='utf-8').read()
    missing = [line for line in EXPECTED_IN_PROGRAM if line not in text]
    return {'ok': not missing, 'missing': missing}


def program_style_command(setup, target_dir, log_file):
    """Der Aufruf, den das Programm beim Update absetzt — Zeichen für Zeichen."""
    switches = '/SILENT /NORESTART /CLOSEAPPLICATIONS'
    switches += ' /DIR="%s"' % target_dir
    switches += ' /LOG="%s"' % log_file
    return 'cmd /c ""%s" %s"' % (setup, switches)


def program_style_env(compat_layer=None):
    """Die Umgebung, wie das Programm sie säubert — auf Wunsch mit Kennung."""
    env = dict(os.environ)
    for name in ('_MEIPASS', '_MEIPASS2', 'TCL_LIBRARY', 'TK_LIBRARY',
                 'TIX_LIBRARY', 'MATPLOTLIBDATA', '__COMPAT_LAYER'):
        env.pop(name, None)
    if compat_layer:
        env['__COMPAT_LAYER'] = compat_layer
    return env


def detach_flags():
    flags = getattr(subprocess, 'CREATE_NO_WINDOW', 0)
    flags |= getattr(subprocess, 'DETACHED_PROCESS', 0)
    flags |= getattr(subprocess, 'CREATE_NEW_PROCESS_GROUP', 0)
    return flags


def run_timed(command, env=None, cwd=None, shell=False):
    """Starten, warten, messen. Hängt es, wird es beendet — und das notiert.

    ⚠ Ein Meldungsfenster im stillen Modus wartet auf einen Klick, den es auf
    einem Bau-Rechner nie gibt. Ein Hänger ist deshalb ein **Ergebnis**:
    Beim Nutzer stünde dann ein Fenster, das niemand erwartet.
    """
    started = time.time()
    proc = subprocess.Popen(command, env=env, cwd=cwd or tempfile.gettempdir(),
                            creationflags=detach_flags(), shell=shell)
    try:
        code = proc.wait(timeout=TIMEOUT)
        hung = False
    except subprocess.TimeoutExpired:
        subprocess.run(['taskkill', '/F', '/T', '/PID', str(proc.pid)],
                       capture_output=True)
        code, hung = None, True
    return {'exit_code': code, 'hung': hung,
            'seconds': round(time.time() - started, 2),
            'ended_at': time.time()}


def inno_log_facts(log_file):
    """Was im Setup-Protokoll steht — und wann die letzte Zeile geschrieben wurde."""
    if not os.path.isfile(log_file):
        return {'exists': False}
    text = open(log_file, encoding='utf-8', errors='replace').read()
    stamps = [m.group(1) for m in (INNO_STAMP.match(z) for z in text.splitlines())
              if m]
    last = None
    if stamps:
        last = time.mktime(time.strptime(stamps[-1][:19], '%Y-%m-%d %H:%M:%S'))
    return {
        'exists': True,
        'security_validation_failure': 'Security validation failure' in text,
        'compatibility_mode_line': [z.strip() for z in text.splitlines()
                                    if 'Compatibility mode' in z][:2],
        'restart_manager_lines': [z.strip()[:160] for z in text.splitlines()
                                  if 'RestartManager' in z or 'Restart Manager' in z
                                  or 'application' in z.lower()][:8],
        'last_line_epoch': last,
        'log_closed': 'Log closed' in text,
        'tail': [z.strip()[:160] for z in text.splitlines()[-4:]],
    }


def tmp_processes():
    """Laufen noch Prozesse, deren Name auf `.tmp` endet? So heißt Innos Hälfte."""
    out = subprocess.run(['tasklist', '/FO', 'CSV', '/NH'], capture_output=True,
                         text=True, errors='replace').stdout
    return sorted({z.split('","')[0].strip('"') for z in out.splitlines()
                   if z.lower().split('","')[0].strip('"').endswith('.tmp')})


def process_alive(pid):
    out = subprocess.run(['tasklist', '/FI', 'PID eq %d' % pid, '/FO', 'CSV',
                          '/NH'], capture_output=True, text=True,
                         errors='replace').stdout
    return str(pid) in out


def port_listener():
    """Welcher Prozess hört gerade auf dem Sperr-Port? Ohne ihn anzustoßen.

    ⚠ Kein `connect()`: Der Watcher deutet jede Verbindung als „bitte nach
    vorn". `netstat` fragt nur, wer lauscht.
    """
    out = subprocess.run(['netstat', '-ano', '-p', 'TCP'], capture_output=True,
                         text=True, errors='replace').stdout
    for z in out.splitlines():
        if ('127.0.0.1:%d' % GUARD_PORT) in z and 'LISTEN' in z.upper():
            return z.split()[-1]
    return None


def try_bind(reuse):
    """Bekommt ein zweiter Prozess den Port? `reuse` wie im Watcher."""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        if reuse:
            s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        s.bind(('127.0.0.1', GUARD_PORT))
        s.listen(1)
        return True
    except OSError:
        return False
    finally:
        s.close()


def start_watcher(exe, home):
    """Die gebaute Fassung starten — mit eigener, schon eingerichteter Ablage.

    Gibt (Prozess, lauscht er, Startspur) zurück.

    ⚠ Lauscht er nach `WATCHER_WAIT` Sekunden nicht, ist jede Messung, die auf
    ihm aufbaut, **ungültig** — nicht „bestanden". Dann kommt die Startspur
    des Programms (`start-spur.txt`) mit in den Bericht, damit man sieht, wo er
    stehen blieb, statt zu raten.
    """
    env = dict(os.environ)
    env['SC_BP_HOME'] = home
    env['SC_BP_NO_NET'] = '1'
    os.makedirs(home, exist_ok=True)
    with open(os.path.join(home, 'einstellungen.json'), 'w',
              encoding='utf-8') as f:
        json.dump(SEEDED_SETTINGS, f)
    proc = subprocess.Popen([exe], env=env, cwd=os.path.dirname(exe),
                            creationflags=detach_flags())
    listening = False
    for _ in range(WATCHER_WAIT * 2):
        if port_listener():
            listening = True
            break
        if proc.poll() is not None:
            break                       # schon beendet — da kommt nichts mehr
        time.sleep(0.5)
    trace = None
    if not listening:
        try:
            with open(os.path.join(home, 'start-spur.txt'), encoding='utf-8',
                      errors='replace') as f:
                trace = f.read().splitlines()[-30:]
        except OSError:
            trace = ['keine start-spur.txt in der Ablage']
    return proc, listening, trace


def stop_watcher(proc):
    """Hart beenden, samt Kindprozess (die gepackte .exe entpackt sich in einen)."""
    subprocess.run(['taskkill', '/F', '/T', '/PID', str(proc.pid)],
                   capture_output=True)


def not_measured(result, trace):
    """Eine Messung als ungültig kennzeichnen — laut, nicht als Erfolg."""
    result['invalid'] = 'Watcher hat seinen Port nie geöffnet — nichts gemessen'
    result['start_trace'] = trace
    return result


# ------------------------------------------------------------------ Messungen

def m1_m2_m3a_success(setup, work, out):
    """M1 Erfolg · M2 Lebensdauer · M3a lebender cmd-Elternprozess."""
    target = os.path.join(work, 'm1_ziel')
    log = os.path.join(out, 'm1_erfolg_setup.log')
    run = run_timed(program_style_command(setup, target, log),
                    env=program_style_env())
    leftovers = tmp_processes()          # direkt nach dem Ende von cmd
    exe = os.path.join(target, 'SC-BP-Watcher.exe')
    facts = inno_log_facts(log)
    exe_mtime = os.path.getmtime(exe) if os.path.isfile(exe) else None
    return {
        'id': 'M1+M2+M3a',
        'question': 'Rückgabewert bei Erfolg; lebt der Prozess bis zum Ende; '
                    'stört ein lebender cmd-Vater?',
        'run': run, 'installed_exe': bool(exe_mtime), 'log': facts,
        'tmp_processes_after_exit': leftovers,
        'exe_written_after_process_end': (exe_mtime is not None
                                          and exe_mtime > run['ended_at']),
        'log_line_after_process_end': (facts.get('last_line_epoch') is not None
                                       and facts['last_line_epoch']
                                       > run['ended_at'] + 1),
    }


def m1_direct(setup, work, out):
    """M1 ohne cmd — liefert der Installer selbst denselben Wert?"""
    target = os.path.join(work, 'm1_direkt')
    log = os.path.join(out, 'm1_direkt_setup.log')
    run = run_timed([setup, '/SILENT', '/NORESTART', '/CLOSEAPPLICATIONS',
                     '/DIR=%s' % target, '/LOG=%s' % log],
                    env=program_style_env())
    return {'id': 'M1-direkt', 'question': 'Rückgabewert ohne cmd dazwischen',
            'run': run, 'log': inno_log_facts(log)}


def m1_error(setup, work, out):
    """M1 Fehler: ein Zielordner auf einem Laufwerk, das es nicht gibt."""
    letter = next((c for c in 'QRSTUVWXYZ' if not os.path.exists(c + ':\\')),
                  None)
    if not letter:
        return {'id': 'M1-Fehler', 'skipped': 'kein freier Laufwerksbuchstabe'}
    target = letter + ':\\gibt_es_nicht\\SC-BP-Watcher'
    results = {}
    for variant, extra in (('wie_im_programm', ''),
                           ('mit_SUPPRESSMSGBOXES', ' /SUPPRESSMSGBOXES')):
        log = os.path.join(out, 'm1_fehler_%s_setup.log' % variant)
        cmd = program_style_command(setup, target, log)
        if extra:
            cmd = cmd[:-1] + extra + '"'
        results[variant] = {'run': run_timed(cmd, env=program_style_env()),
                            'log': inno_log_facts(log)}
    return {'id': 'M1-Fehler',
            'question': 'Rückgabewert bei echtem Fehler — und hängt ein '
                        'Meldungsfenster im stillen Modus?',
            'target': target, 'results': results}


def m3b_dying_parent(setup, work, out):
    """M3b: Der Elternprozess stirbt sofort (cmd /c start)."""
    target = os.path.join(work, 'm3b_ziel')
    log = os.path.join(out, 'm3b_sterbender_vater_setup.log')
    cmd = 'cmd /c start "" "%s" /SILENT /NORESTART /CLOSEAPPLICATIONS ' \
          '/DIR="%s" /LOG="%s"' % (setup, target, log)
    parent = run_timed(cmd, env=program_style_env())
    exe = os.path.join(target, 'SC-BP-Watcher.exe')
    for _ in range(TIMEOUT):             # auf das eigentliche Setup warten
        if inno_log_facts(log).get('log_closed'):
            break
        time.sleep(1)
    return {'id': 'M3b', 'question': 'Stört ein sofort sterbender Vater?',
            'parent': parent, 'installed_exe': os.path.isfile(exe),
            'log': inno_log_facts(log)}


def m3c_compat_layer(setup, work, out):
    """M3c: Versuch, den alten Fehler nachzustellen — mit Kompatibilitäts-Kennung.

    ⚠ Ausdrücklich ein **Versuch**. Welchen Wert Windows damals gesetzt hat,
    stand nur als „DetectorsAppHealth" im Protokoll; ob der Wert hier dieselbe
    Wirkung hat, ist genau die Frage.
    """
    target = os.path.join(work, 'm3c_ziel')
    log = os.path.join(out, 'm3c_kompat_setup.log')
    run = run_timed(program_style_command(setup, target, log),
                    env=program_style_env(compat_layer='DetectorsAppHealth'))
    return {'id': 'M3c', 'question': 'Stellt __COMPAT_LAYER den alten Fehler nach?',
            'run': run, 'log': inno_log_facts(log)}


def m6_m5_guard(exe, work):
    """M6 Sperre verhindert zweite Bindung? · M5 Port nach hartem Beenden frei?"""
    home = os.path.join(work, 'm6_ablage')
    proc, listening, trace = start_watcher(exe, home)
    result = {'id': 'M6+M5',
              'question': 'Verhindert die Sperre unter Windows eine zweite '
                          'Bindung — und ist der Port nach hartem Beenden frei?',
              'watcher_pid': proc.pid, 'listener_pid': port_listener(),
              'watcher_listening': listening}
    if not listening:
        stop_watcher(proc)
        return not_measured(result, trace)
    result['second_bind_with_reuse'] = try_bind(reuse=True)
    result['second_bind_without_reuse'] = try_bind(reuse=False)
    stop_watcher(proc)
    series = []
    for wait in (0, 0.5, 1, 2, 5):
        time.sleep(wait if not series else wait - series[-1]['t'])
        series.append({'t': wait, 'listener': port_listener(),
                       'bind_like_watcher': try_bind(reuse=True),
                       'bind_strict': try_bind(reuse=False)})
    result['after_hard_kill'] = series
    return result


def m4_close_from_temp_helper(setup, exe_installed, work, out):
    """M4: Setup aus einem Helfer in %TEMP%, Watcher läuft aus dem Zielordner."""
    target = os.path.dirname(exe_installed)
    home = os.path.join(work, 'm4_ablage')
    proc, listening, trace = start_watcher(exe_installed, home)
    result = {'id': 'M4',
              'question': 'Schließt CloseApplications=force den Watcher, wenn '
                          'das Setup aus einem Helfer in %TEMP% startet?',
              'watcher_listening_before': listening}
    if not listening:
        stop_watcher(proc)
        return not_measured(result, trace)
    log = os.path.join(out, 'm4_helfer_setup.log')
    helper = os.path.join(tempfile.gettempdir(), 'scbp_messung_helfer.cmd')
    with open(helper, 'w', encoding='ascii', errors='replace') as f:
        f.write('@echo off\r\n%s\r\nexit /b %%errorlevel%%\r\n'
                % program_style_command(setup, target, log))
    run = run_timed(['cmd', '/c', helper], env=program_style_env())
    time.sleep(2)
    result.update({'watcher_alive_after': process_alive(proc.pid),
                   'run': run, 'log': inno_log_facts(log),
                   'port_after': {'listener': port_listener(),
                                  'bind_like_watcher': try_bind(reuse=True)}})
    if result['watcher_alive_after']:
        stop_watcher(proc)              # aufräumen, der Befund steht schon oben
    return result


def m7_awkward_paths(setup, work, out):
    """M7: Pfade mit Zeichen, die cmd mitverarbeiten könnte."""
    results = []
    for number, name in enumerate(AWKWARD_NAMES, 1):
        base = os.path.join(work, 'pfade', name)
        try:
            os.makedirs(base, exist_ok=True)
            setup_copy = os.path.join(base, 'SC-BP-Watcher-Setup.exe')
            shutil.copy2(setup, setup_copy)
        except OSError as exc:
            results.append({'name': name, 'prepare_error': str(exc)})
            continue
        target = os.path.join(base, 'ziel')
        log = os.path.join(base, 'setup.log')
        run = run_timed(program_style_command(setup_copy, target, log),
                        env=program_style_env())
        facts = inno_log_facts(log)
        if facts.get('exists'):
            # ⚠ Mit laufender Nummer: `a&b`, `a^b` und `a!b` werden beim
            #   Ersetzen alle zu `a_b` und überschrieben sich sonst gegenseitig.
            shutil.copy2(log, os.path.join(out, 'm7_%d_%s_setup.log' % (
                number, re.sub(r'[^A-Za-z0-9_]', '_', name)[:40])))
        results.append({'name': name, 'path_length': len(target),
                        'run': run,
                        'installed_exe': os.path.isfile(
                            os.path.join(target, 'SC-BP-Watcher.exe')),
                        'log_written': facts.get('exists', False)})
    return {'id': 'M7', 'question': 'Überleben ungewöhnliche Pfade den cmd-Aufruf?',
            'results': results}


# ------------------------------------------------------------------ Bericht

def invalid_ids(report):
    """Welche Messungen haben nichts gemessen (oder sind abgestürzt)?"""
    return [str(m.get('id')) for m in report['measurements']
            if m.get('invalid') or m.get('crashed')]


def summary_lines(report):
    """Eine lesbare Kurzfassung. Die Einzelheiten stehen im JSON daneben."""
    z = ['# Messung P1b-1', '',
         'Drift-Prüfung: %s' % ('Aufruf im Programm unverändert'
                               if report['drift']['ok'] else
                               'ACHTUNG — Aufruf im Programm geändert, '
                               'Messung gilt nicht: %s'
                               % report['drift']['missing']), '']
    invalid = invalid_ids(report)
    z += [('UNGÜLTIG, nichts gemessen: %s' % ', '.join(invalid)) if invalid
          else 'Alle Messungen gültig', '']
    for m in report['measurements']:
        z.append('## %s — %s' % (m.get('id'), m.get('question', '')))
        z.append('```')
        z.append(json.dumps(m, ensure_ascii=False, indent=1, default=str)[:3000])
        z.append('```')
        z.append('')
    return z


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[1])
    parser.add_argument('--setup')
    parser.add_argument('--exe')
    parser.add_argument('--out', default='messung')
    parser.add_argument('--trocken', action='store_true',
                        help='nur Aufbau und Drift prüfen, nichts ausführen')
    args = parser.parse_args(argv[1:])

    drift = drift_check()
    if args.trocken:
        print('Drift-Prüfung:', 'ok' if drift['ok'] else drift['missing'])
        print('Beispielaufruf:', program_style_command(
            r'C:\Temp\SC-BP-Watcher-Setup.exe', r'C:\Ziel mit Leerzeichen',
            r'C:\Temp\setup.log'))
        print('Pfade für M7:', len(AWKWARD_NAMES))
        return 0 if drift['ok'] else 1

    if not sys.platform.startswith('win'):
        print('Diese Messungen gibt es nur unter Windows (sonst --trocken).')
        return 2
    os.makedirs(args.out, exist_ok=True)
    out = os.path.abspath(args.out)
    work = tempfile.mkdtemp(prefix='scbp_messung_')
    setup = os.path.abspath(args.setup)
    exe = os.path.abspath(args.exe)

    report = {'drift': drift, 'python': sys.version, 'measurements': []}

    def step(fn, *a):
        # ⚠ Eine Messung, die abstürzt, darf die übrigen nicht mitreißen —
        #   und ihr Absturz ist selbst ein Befund.
        try:
            result = fn(*a)
        except Exception as exc:
            result = {'id': fn.__name__, 'crashed': repr(exc)}
        report['measurements'].append(result)
        print('fertig:', result.get('id'))
        return result

    first = step(m1_m2_m3a_success, setup, work, out)
    step(m1_direct, setup, work, out)
    step(m1_error, setup, work, out)
    step(m3b_dying_parent, setup, work, out)
    step(m3c_compat_layer, setup, work, out)
    step(m6_m5_guard, exe, work)
    installed = os.path.join(work, 'm1_ziel', 'SC-BP-Watcher.exe')
    if first.get('installed_exe') and os.path.isfile(installed):
        step(m4_close_from_temp_helper, setup, installed, work, out)
    else:
        report['measurements'].append(
            {'id': 'M4', 'skipped': 'M1 hat nichts installiert'})
    step(m7_awkward_paths, setup, work, out)

    with open(os.path.join(out, 'messung.json'), 'w', encoding='utf-8') as f:
        json.dump(report, f, ensure_ascii=False, indent=1, default=str)
    with open(os.path.join(out, 'messung.md'), 'w', encoding='utf-8') as f:
        f.write('\n'.join(summary_lines(report)) + '\n')
    print('\n'.join(summary_lines(report)))
    # ⚠ Ein Befund ist ein Ergebnis, kein Fehlschlag — deshalb bleibt der
    #   Lauf auch bei „Installer scheitert" grün. Rot wird er nur, wenn das
    #   Werkzeug selbst nichts gemessen hat: Drift im Programm, oder eine
    #   Messung ungültig bzw. abgestürzt. Der erste Lauf war trotz drei leerer
    #   Messungen grün — genau das soll nicht wieder durchrutschen.
    return 0 if drift['ok'] and not invalid_ids(report) else 1


if __name__ == '__main__':
    sys.exit(main(sys.argv))
