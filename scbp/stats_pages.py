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
Die Seiten der Gruppe „Statistik" (seit v3.58.0-rc2).

Eigenes Modul, weil `pages.py` mit rund 18.000 Zeilen schon die teuerste Datei
im Projekt ist — sechs Seiten mehr hätten sie nur noch länger gemacht. Die
Bausteine (Überschrift, Rollfläche, Absatz, Knopf) kommen aus `pages`, damit
die Seiten aussehen wie alle anderen.

⚠ **Zeigen, dann nachlesen.** Jede Seite zeigt sofort den gespeicherten Stand
aus `statistik.json`; nur die laufende `Game.log` wird beim Öffnen im
Hintergrund nachgelesen — und nur, wenn „Automatisch auswerten" an ist.

⚠ **Nichts ist anklickbar.** Die Seiten erzählen, sie führen nirgendwohin
(Wunsch vom 27.09.2026: „ohne was anklickbares"). Balken und Tabellen sind
Labels, keine Knöpfe — ein Mauszeiger als Hand wäre ein falsches Versprechen.
"""
import calendar
import os
import threading
import time
import tkinter as tk

from . import errors, paths
from .language import t

# Die Farben wie in `pages` — eine eigene Kopie wäre die nächste, die
# auseinanderläuft. Deshalb über das Modul, nicht abgeschrieben.
from .pages import (BG, SURFACE, FG, SUB, ACCENT, LINE, GOLD, RED, RED_PALE,
                    _heading, _scroll_area, _body_text, _button, _setting_row)

# Wie hoch eine Zeile der Wärmekarte ist — und wie viel Platz die
# Wochentage links brauchen. Die Breite der Zellen rechnet sich aus der Seite.
STATS_CELL_HEIGHT = 22
STATS_DAY_WIDTH = 34

# Die Seiten der Gruppe, in der Reihenfolge der Leiste. Kennung -> Bauer.
PAGE_IDS = ('statistik_auswertung', 'statistik', 'statistik_schiffe',
            'statistik_auftraege', 'statistik_quantum', 'statistik_stabil')

# Bekannte Gründe für einen Verbindungsabbruch -> Text. Unbekannte bleiben,
# wie das Spiel sie schreibt: lieber roh als falsch übersetzt.
DISCONNECT_REASONS = {
    'Nub destroyed': 's_sb_r_nub',
    'Remote Disconnect - Player requested disconnect': 's_sb_r_selbst',
    'DisconnectCmd: disconnect light ExitToMenu': 's_sb_r_menue',
    'Remote Disconnect - player inactive': 's_sb_r_inaktiv',
    'DGS disconnecting all channels before game shutdown': 's_sb_r_server',
    'Remote Disconnect - Dup Login': 's_sb_r_doppelt',
}

SESSION_ENDS = {'sauber': ('s_sb_e_sauber', ACCENT),
                'absturz': ('s_sb_e_absturz', RED),
                'offen': ('s_sb_e_offen', GOLD)}


def builders():
    return {'statistik_auswertung': evaluation,
            'statistik': overview,
            'statistik_schiffe': ships_page,
            'statistik_auftraege': missions_page,
            'statistik_quantum': quantum_page,
            'statistik_stabil': stability_page}


# ------------------------------------------------------------------ Bausteine
def stats_count(value):
    """Ganze Zahl mit Tausenderpunkt (deutsch) bzw. -komma (englisch)."""
    from .language import current
    text = '{:,}'.format(int(value or 0))
    return text.replace(',', '.') if current() == 'de' else text


def percent(part, whole):
    return stats_count(round(100.0 * part / whole)) if whole else '0'


def local_day(seconds):
    from .translation import _day
    return _day(time.strftime('%Y-%m-%d', time.localtime(seconds)))


def local_moment(seconds):
    return '%s, %s' % (local_day(seconds),
                       time.strftime('%H:%M', time.localtime(seconds)))


def log_seconds(stamp):
    """Ein Zeitstempel aus dem Log (UTC, ohne Zone) als Sekunden."""
    try:
        return calendar.timegm(time.strptime(stamp[:19], '%Y-%m-%dT%H:%M:%S'))
    except (ValueError, TypeError):
        return None


def tiles(window, parent, items, columns=2):
    """Kennzahlen als gleich breite Kacheln. `items`: (Titel, Wert, Zeile)."""
    from .main_window import round_frame
    grid = tk.Frame(parent, bg=BG)
    grid.pack(fill='x')
    for column in range(columns):
        grid.columnconfigure(column, weight=1, uniform='kachel')
    for index, (title, value, below) in enumerate(items):
        row, column = divmod(index, columns)
        card = round_frame(grid, SURFACE, LINE, radius=8, base_color=BG)
        card.holder.grid(row=row, column=column, sticky='nsew',
                         padx=(0 if column == 0 else 5,
                               0 if column == columns - 1 else 5), pady=5)
        tk.Label(card, text=title, bg=SURFACE, fg=SUB, font=window.f_small,
                 anchor='w').pack(fill='x', padx=16, pady=(12, 0))
        tk.Label(card, text=value, bg=SURFACE, fg=ACCENT, font=window.f_title,
                 anchor='w').pack(fill='x', padx=16)
        if below:
            tk.Label(card, text=below, bg=SURFACE, fg=SUB, font=window.f_small,
                     anchor='w').pack(fill='x', padx=16)
        # Der untere Rand für sich — sonst hinge er an einer Zeile, die nicht
        # jede Kachel hat, und die Kacheln wären verschieden hoch gepolstert.
        tk.Frame(card, bg=SURFACE, height=12).pack(fill='x')
    return grid


def section(window, parent, title, right=''):
    """Überschrift eines Abschnitts, rechts wahlweise eine Summe."""
    tk.Frame(parent, bg=LINE, height=1).pack(fill='x', pady=(22, 0))
    row = tk.Frame(parent, bg=BG)
    row.pack(fill='x', pady=(12, 6))
    tk.Label(row, text=title, bg=BG, fg=FG, font=window.f_bold,
             anchor='w').pack(side='left')
    if right:
        tk.Label(row, text=right, bg=BG, fg=SUB, font=window.f_small,
                 anchor='e').pack(side='right')
    return row


def bars(window, parent, rows, color=ACCENT, unit=None):
    """Eine Rangliste: Name links, Zahl rechts, darunter ein Balken.

    `rows`: (Name, Zahl). Der Balken misst am größten Eintrag, nicht an der
    Summe — sonst wären bei zehn ähnlichen Einträgen alle Balken winzig.
    `unit(zahl)` macht aus der Zahl den Text rechts."""
    if not rows:
        _body_text(parent, t('s_sx_nichts'), window.f_small, pady=(0, 4))
        return
    largest = max(count for _name, count in rows) or 1
    for name, count in rows:
        row = tk.Frame(parent, bg=BG)
        row.pack(fill='x', pady=(4, 0))
        tk.Label(row, text=name, bg=BG, fg=FG, font=window.f_base,
                 anchor='w').pack(side='left', fill='x', expand=True)
        tk.Label(row, text=unit(count) if unit else stats_count(count),
                 bg=BG, fg=SUB, font=window.f_small).pack(side='right')
        # ⚠ Über `place(relwidth=…)`, nicht über eine gemessene Pixelbreite:
        # Die Seite steht beim Bauen noch nicht fest (Falle 1 der
        # Projektregeln), der Anteil dagegen schon.
        track = tk.Frame(parent, bg=LINE, height=4)
        track.pack(fill='x', pady=(2, 4))
        tk.Frame(track, bg=color).place(relx=0, rely=0, relheight=1,
                                        relwidth=max(0.01, count / largest))


def table(window, parent, headers, rows, weights=None):
    """Eine schlichte Tabelle — Kopfzeile, Linien, kein Klick.

    `rows`: Zeilen aus (Text, Farbe oder None)."""
    from .main_window import round_frame
    box = round_frame(parent, SURFACE, LINE, radius=8, base_color=BG)
    box.holder.pack(fill='x', pady=(0, 4))
    grid = tk.Frame(box, bg=SURFACE)
    grid.pack(fill='x', padx=12, pady=8)
    for column, weight in enumerate(weights or [1] + [0] * (len(headers) - 1)):
        grid.columnconfigure(column, weight=weight)
    for column, header in enumerate(headers):
        tk.Label(grid, text=header, bg=SURFACE, fg=SUB, font=window.f_small,
                 anchor='w').grid(row=0, column=column, sticky='we',
                                  padx=(0, 14), pady=(0, 4))
    for index, cells in enumerate(rows, start=1):
        for column, (text, color) in enumerate(cells):
            tk.Label(grid, text=text, bg=SURFACE, fg=color or FG,
                     font=window.f_small, anchor='w', justify='left',
                     wraplength=0).grid(row=index, column=column, sticky='we',
                                        padx=(0, 14), pady=2)


def notes(window, parent, key):
    """„Zu den Zahlen" — was die Zahlen heißen und was sie nicht wissen."""
    section(window, parent, t('s_sx_zahlen'))
    _body_text(parent, t(key), window.f_small, pady=(0, 20))


def live_page(window, frame, page_id, title, lead, render, names=False):
    """Der gemeinsame Rahmen jeder Statistik-Seite.

    `render(holder)` baut den Inhalt; er wird beim Öffnen neu gebaut, nachdem
    im Hintergrund die laufende `Game.log` nachgelesen wurde. `names=True`
    holt vorher die Anzeigenamen des Spiels (einmalig rund eine Sekunde)."""
    from . import play_stats
    _heading(window, frame, title, lead)
    inner = _scroll_area(frame)
    holder = tk.Frame(inner, bg=BG)
    holder.pack(fill='x', pady=(0, 20))

    def draw():
        for child in holder.winfo_children():
            child.destroy()
        try:
            render(holder)
        except Exception as exception:
            errors.record('stats_pages.%s' % page_id, exception)
            _body_text(holder, t('s_sx_nichts'), window.f_small, pady=(4, 8))

    def refresh():
        def work():
            try:
                if names:
                    play_stats.load_names(fetch=True)
                if play_stats.auto_enabled():
                    game = paths.game_folder()
                    running = os.path.join(game, 'Game.log') if game else ''
                    if running and os.path.isfile(running):
                        play_stats.catch_up([running])
            except Exception as exception:
                errors.record('stats_pages.%s.refresh' % page_id, exception)
            try:
                window.root.after(0, lambda: holder.winfo_exists() and draw())
            except (RuntimeError, tk.TclError):
                pass            # Fenster ist weg — dann gibt es nichts zu zeigen
        threading.Thread(target=work, daemon=True).start()

    draw()
    window.on_show[page_id] = refresh
    return inner


def own_account():
    from . import logsource
    return logsource.own_account()


# ------------------------------------------------------------------ Seiten
def evaluation(window, frame):
    """Auswertung: Stand, Knöpfe, Automatik, Speichern.

    ⚠ Ganz oben in der Gruppe (Wunsch vom 27.09.2026): Was man einstellt,
    steht vor dem, was man liest — wie bei den übrigen Gruppen."""
    from . import play_stats
    _heading(window, frame, t('hf_st_auswertung'), t('s_sa_lead'))
    inner = _scroll_area(frame)
    status = tk.Frame(inner, bg=BG)
    status.pack(fill='x')

    def draw_status():
        for child in status.winfo_children():
            child.destroy()
        info = play_stats.state()
        when = info['stand']
        size = info['groesse']
        size_text = ('%s KB' % stats_count(round(size / 1024.0))) if size \
            else '—'
        tiles(window, status, [
            (t('s_sa_zuletzt'), local_moment(when) if when else t('s_sa_nie'),
             ''),
            (t('s_sa_sitzungen'), stats_count(info['sitzungen']), ''),
            (t('s_sa_dateien'), stats_count(info['dateien']), ''),
            (t('s_sa_groesse'), size_text, ''),
        ])

    buttons = tk.Frame(inner, bg=BG)
    buttons.pack(fill='x', pady=(10, 0))
    busy = [False]

    def run(everything):
        if busy[0]:
            return
        busy[0] = True
        window.say(t('s_sa_laeuft'))

        def work():
            added = 0
            try:
                added = (play_stats.rescan() if everything
                         else play_stats.catch_up(play_stats.log_files()))
            except Exception as exception:
                errors.record('stats_pages.evaluation.run', exception)

            def done():
                busy[0] = False
                if status.winfo_exists():
                    draw_status()
                window.say(t('s_sa_fertig') % stats_count(added))
            try:
                window.root.after(0, done)
            except (RuntimeError, tk.TclError):
                pass
        threading.Thread(target=work, daemon=True).start()

    _button(window, buttons, t('s_sa_jetzt'), lambda: run(False),
            strong=True).pack(side='left')
    _button(window, buttons, t('s_sa_neu'), lambda: run(True)).pack(
        side='left', padx=(10, 0))
    _body_text(inner, t('s_sa_knoepfe_h'), window.f_small, pady=(8, 0))

    from .main_window import toggle_switch
    target = _setting_row(window, inner, t('s_sa_auto'), t('s_sa_auto_h'))

    def switch_auto():
        new_value = not play_stats.auto_enabled()
        paths.set_setting(play_stats.AUTO_SETTING, new_value)
        window.say('%s: %s' % (t('s_sa_auto'),
                               t('e_an') if new_value else t('e_aus')))
        return new_value

    toggle_switch(target, play_stats.auto_enabled(), switch_auto).pack()

    def export():
        from . import file_picker, export as export_module
        target_file = file_picker.save_file(
            t('s_sx_export'),
            suggestion='versekit-statistik-%s.json' % time.strftime('%Y-%m-%d'),
            extension='.json', start=export_module.archive_folder())
        if not target_file:
            return
        if play_stats.export(target_file, own_account(),
                             getattr(window, 'version', '') or ''):
            window.say(t('s_sx_exportiert') % os.path.basename(target_file))
        else:
            window.say(t('s_sx_export_fehler'))

    target = _setting_row(window, inner, t('s_sx_export').rstrip(' …'),
                          t('s_sa_export_h'))
    _button(window, target, t('s_sx_export'), export).pack()
    _body_text(inner, t('s_sx_hinweis'), window.f_small, pady=(24, 20))

    draw_status()
    window.on_show['statistik_auswertung'] = draw_status


def overview(window, frame):
    """Übersicht: wie viel, wie oft, wann gespielt wird.

    ⚠ Eine Zahl, die nicht aus dem Log belegt ist, steht hier nicht. Deshalb
    keine Tode, keine Verletzungen, keine Zonen — siehe `play_stats`."""
    from . import play_stats, playtime

    def render(holder):
        data = play_stats.summary(own_account())
        if not data['spielzeit']:
            _body_text(holder, t('s_sx_leer'), window.f_small, pady=(4, 8))
            return
        since = t('s_sx_seit') % local_day(data['seit']) if data['seit'] else ''
        tiles(window, holder, [
            (t('s_sx_spielzeit'), playtime.as_text(data['spielzeit']), since),
            (t('s_sx_sitzungen'), stats_count(data['sitzungen']),
             t('s_sx_schnitt') % (playtime.as_text(data['schnitt']),
                                  playtime.as_text(data['laengste']))),
            (t('s_sx_auftraege'), stats_count(data['auftraege']),
             t('s_sx_fehl') % stats_count(data['fehlgeschlagen'])),
            (t('s_sx_spruenge'), stats_count(data['spruenge']),
             t('s_sx_je') % stats_count(
                 round(data['spruenge'] / data['erfasst'])
                 if data['erfasst'] else 0)),
        ])
        tk.Label(holder, text=t('s_sx_wann'), bg=BG, fg=FG,
                 font=window.f_title, anchor='w').pack(fill='x', pady=(20, 6))
        heatmap(window, holder, data['waermekarte'])
        _body_text(holder, t('s_sx_hinweis'), window.f_small, pady=(16, 10))

    live_page(window, frame, 'statistik', t('hf_st_uebersicht'),
              t('s_sx_lead'), render)


def heatmap(window, parent, grid_values):
    """Wochentag × Stunde, je heller desto mehr Spielzeit."""
    days = t('s_sx_tage').split(',')
    # ⚠ Die Höhe der Stundenzeile aus der Schrift, nicht geschätzt: Mit
    # festen 22 px war die Beschriftung unten im ersten Bild angeschnitten.
    height = 7 * STATS_CELL_HEIGHT + 4 + window.f_small.metrics('linespace') + 4
    canvas = tk.Canvas(parent, bg=BG, highlightthickness=0, height=height)
    canvas.pack(fill='x', pady=(4, 0))
    highest = max((max(row) for row in grid_values), default=0) or 1

    def color(share):
        # ⚠ Wurzel statt linear: Sonst ist alles außer den zwei, drei
        # stärksten Stunden fast gleich dunkel, und die Karte sagt nichts.
        share = share ** 0.5
        low, high = (0x1b, 0x22, 0x30), (0x9c, 0xe4, 0x30)
        parts = [int(a + (b - a) * share) for a, b in zip(low, high)]
        return '#%02x%02x%02x' % tuple(parts)

    def draw(_event=None):
        canvas.delete('all')
        # ⚠⚠ Die Spalte der Wochentage wird aus dem LÄNGSTEN Namen gemessen,
        # bei jedem Zeichnen neu: Mit festen 34 px ragten „Mon" und „Wed" auf
        # Englisch in die Karte hinein. Und gemessen wird erst hier, nicht
        # beim Bauen — die Schriftbreite steht erst fest, wenn das Fenster da
        # ist (Falle 1 der Projektregeln).
        column = max([STATS_DAY_WIDTH] + [window.f_small.measure(n) + 10
                                          for n in days])
        width = max(canvas.winfo_width(), column + 24 * 8)
        cell = (width - column) / 24.0
        for day, row in enumerate(grid_values):
            y = day * STATS_CELL_HEIGHT
            canvas.create_text(0, y + STATS_CELL_HEIGHT / 2, anchor='w',
                               text=days[day] if day < len(days) else '',
                               fill=SUB, font=window.f_small)
            for hour, seconds in enumerate(row):
                x = column + hour * cell
                canvas.create_rectangle(
                    x + 1, y + 1, x + cell - 1, y + STATS_CELL_HEIGHT - 1,
                    width=0,
                    fill=color(seconds / highest) if seconds else LINE)
        # Alle drei Stunden beschriftet (Wunsch vom 27.09.2026) — alle sechs
        # waren zu grob, um eine Zelle ohne Abzählen zu treffen.
        for hour in range(0, 24, 3):
            canvas.create_text(column + hour * cell, 7 * STATS_CELL_HEIGHT + 4,
                               anchor='nw', text='%02d' % hour, fill=SUB,
                               font=window.f_small)

    canvas.bind('<Configure>', draw)
    draw()


def ships_page(window, frame):
    """Schiffe & Ausrüstung."""
    from . import play_stats

    def render(holder):
        data = play_stats.ships(own_account())
        top = data['meist']
        if top:
            name, sessions = top
            tiles(window, holder, [
                (t('s_ss_meist'), play_stats.display_name(name, vehicle=True),
                 ''),
                (t('s_ss_sitzungen_mit'), stats_count(sessions), ''),
                (t('s_ss_schiffe'), stats_count(data['schiffe']), ''),
                (t('s_ss_anteil'),
                 '%s %%' % percent(sessions, data['sitzungen_mit_schiff']),
                 ''),
            ])
        times = lambda n: t('s_sx_mal') % stats_count(n)
        section(window, holder, t('s_ss_genutzt'))
        bars(window, holder,
             [(play_stats.display_name(n, vehicle=True), c)
              for n, c in data['genutzt']], unit=times)
        section(window, holder, t('s_ss_verloren'),
                stats_count(data['verloren_gesamt']))
        bars(window, holder,
             [(play_stats.display_name(n, vehicle=True), c)
              for n, c in data['verloren']], color=RED, unit=times)
        for slot, key in (('ruecken', 's_ss_ruecken'), ('seite', 's_ss_seite'),
                          ('hand', 's_ss_hand')):
            rows, total = data['waffen'].get(slot, ([], 0))
            section(window, holder, t(key))
            bars(window, holder,
                 [(play_stats.display_name(n), c) for n, c in rows],
                 color=GOLD,
                 unit=lambda n, total=total: '%s %%' % percent(n, total))
        notes(window, holder, 's_ss_hinweis')

    live_page(window, frame, 'statistik_schiffe', t('hf_st_schiffe'),
              t('s_ss_lead'), render, names=True)


def missions_page(window, frame):
    """Aufträge — nur zum Lesen; das Protokoll mit Suche steht bei den
    Bauplänen („Auftragsprotokoll") und bleibt dort."""
    from . import play_stats, mission_log

    def render(holder):
        data = play_stats.missions(own_account())
        ended = data['abgeschlossen'] + data['fehlgeschlagen'] \
            + data['abgebrochen']
        tiles(window, holder, [
            (t('s_sm_fertig'), stats_count(data['abgeschlossen']),
             t('s_sm_quote') % percent(data['abgeschlossen'], ended)),
            (t('s_sm_fehl'), stats_count(data['fehlgeschlagen']),
             t('s_sm_quote') % percent(data['fehlgeschlagen'], ended)),
            (t('s_sm_abbruch'), stats_count(data['abgebrochen']),
             t('s_sm_quote') % percent(data['abgebrochen'], ended)),
        ], columns=3)
        section(window, holder, t('s_sm_haeufig'))
        bars(window, holder, data['haeufigste'],
             unit=lambda n: t('s_sx_mal') % stats_count(n))
        section(window, holder, t('s_sm_letzte'))
        words = {mission_log.COMPLETED: ('s_al_fertig', ACCENT),
                 mission_log.ABORTED: ('s_al_abbruch', RED_PALE),
                 mission_log.FAILED: ('s_al_fehl', RED_PALE),
                 mission_log.EXPIRED: ('s_al_verfallen', SUB)}
        rows = []
        for name, stamp, state in data['letzte']:
            seconds = log_seconds(stamp)
            key, color = words.get(state, ('s_al_fertig', ACCENT))
            rows.append([(name, None),
                         (local_moment(seconds) if seconds else '—', SUB),
                         (t(key), color)])
        if rows:
            table(window, holder, [t('s_sm_name'), t('s_sx_datum'),
                                   t('s_sm_ausgang')], rows)
        else:
            _body_text(holder, t('s_sx_nichts'), window.f_small)
        notes(window, holder, 's_sm_hinweis')

    live_page(window, frame, 'statistik_auftraege', t('hf_st_auftraege'),
              t('s_sm_lead'), render)


def quantum_page(window, frame):
    """Quantenreisen."""
    from . import play_stats

    def render(holder):
        data = play_stats.quantum(own_account())
        tiles(window, holder, [
            (t('s_sq_angekommen'), stats_count(data['spruenge']),
             t('s_sq_angekommen_u')),
            (t('s_sq_wahl'), stats_count(data['zielwahlen']),
             t('s_sq_wahl_u')),
            (t('s_sq_quote'),
             '%s %%' % percent(data['spruenge'], data['zielwahlen']),
             t('s_sq_quote_u')),
        ], columns=3)
        section(window, holder, t('s_sq_ziele'))
        bars(window, holder,
             [(t(play_stats.PLACE_LABELS[p]) if p in play_stats.PLACE_LABELS
               else p, c) for p, c in data['ziele']],
             unit=lambda n: t('s_sx_mal') % stats_count(n))
        notes(window, holder, 's_sq_hinweis')

    live_page(window, frame, 'statistik_quantum', t('hf_st_quantum'),
              t('s_sq_lead'), render)


def stability_page(window, frame):
    """Stabilität: Enden, Abstürze, Verbindungsabbrüche."""
    from . import play_stats, playtime

    def render(holder):
        data = play_stats.stability(own_account())
        total = data['sitzungen']
        tiles(window, holder, [
            (t('s_sb_sauber'), stats_count(data['sauber']),
             t('s_sx_anteil') % percent(data['sauber'], total)),
            (t('s_sb_absturz'), stats_count(data['absturz']),
             t('s_sx_anteil') % percent(data['absturz'], total)),
            (t('s_sb_offen'), stats_count(data['ohne_ende']),
             t('s_sx_anteil') % percent(data['ohne_ende'], total)),
        ], columns=3)
        section(window, holder, t('s_sb_gruende'),
                stats_count(data['abbrueche']))
        bars(window, holder,
             [(t(DISCONNECT_REASONS[r]) if r in DISCONNECT_REASONS else r, c)
              for r, c in data['gruende']])
        section(window, holder, t('s_sb_letzte'))
        rows = []
        for start, duration, end, drops in data['letzte']:
            key, color = SESSION_ENDS.get(end, SESSION_ENDS['offen'])
            rows.append([(local_moment(start) if start else '—', None),
                         (playtime.as_text(max(0, duration)), SUB),
                         (t(key), color),
                         (stats_count(drops), SUB)])
        if rows:
            table(window, holder, [t('s_sx_datum'), t('s_sb_dauer'),
                                   t('s_sb_ende'), t('s_sb_abbrueche')], rows,
                  weights=[1, 0, 1, 0])
        else:
            _body_text(holder, t('s_sx_nichts'), window.f_small)
        notes(window, holder, 's_sb_hinweis')

    live_page(window, frame, 'statistik_stabil', t('hf_st_stabil'),
              t('s_sb_lead'), render)
