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
Deutsch und Englisch — die Texte der Oberfläche.

Warum von Anfang an und nicht später: Unter Linux fahren die meisten
Star-Citizen-Spieler den englischen Client, und Windows-Spieler ohne den
SC Deutsch Launcher ebenso. Eine nur deutsche Oberfläche würde einen großen
Teil derer aussperren, für die dieses Werkzeug überhaupt gebaut wird. Und je
später man anfängt, desto mehr Textstellen sind es — hier waren es rund 40,
das ist ein Nachmittag; bei dreimal so vielen wäre es eine Plage.

**Der Spieler kann umschalten.** Standard ist `auto` (nach Systemsprache), aber
in `einstellungen.json` steht das Feld `sprache` — `de`, `en` oder `auto`.
Automatik allein reicht nicht: Wer ein englisches Windows fährt und trotzdem
Deutsch lesen will, soll das dürfen.

Benutzung:

    from .sprache import t
    t('bauplaene')                 -> 'Baupläne'  bzw.  'Blueprints'
    t('von_gesamt', 3, 714, 0)     -> '3 von 714 (0 %)'

Ein fehlender Schlüssel liefert den Schlüssel selbst zurück, statt abzustürzen —
eine vergessene Übersetzung soll auffallen, aber nichts kaputtmachen.
`python3 -m scbp.sprache` listet, was in einer Sprache fehlt.
"""
import locale
import os
import time

from . import pfade

SPRACHEN = ('de', 'en')
STANDARD = 'de'

# Alle Texte, beide Sprachen nebeneinander. Bewusst in einer Tabelle statt in
# getrennten Dateien: So sieht man beim Nachtragen sofort, ob etwas fehlt.
TEXTE = {
    # -- Verwaltungsfenster --
    'titel_bauplaene':   ('SC BP Watcher — Baupläne', 'SC BP Watcher — Blueprints'),
    'bauplaene':         ('Baupläne', 'Blueprints'),
    'filter_alle':       ('alle', 'all'),
    'filter_habe':       ('habe ich', 'owned'),
    'filter_fehlt':      ('fehlt mir', 'missing'),
    'nichts_gefunden':   ('Nichts gefunden.', 'Nothing found.'),
    'weitere_anzeigen':  ('… %d weitere anzeigen', '… show %d more'),
    'von_gesamt':        ('· %d von %d (%d %%)', '· %d of %d (%d %%)'),
    'kein_katalog':      ('Noch kein Bauplan-Katalog vorhanden.',
                          'No blueprint catalogue yet.'),
    'kein_katalog_hilfe': (
        'Er wird beim Start von scmdb.net geholt (etwa 12 MB,\n'
        'einmal je Spielversion). Ohne ihn läuft die Erkennung\n'
        'weiter, es fehlt nur diese Liste.',
        'It is fetched from scmdb.net on startup (about 12 MB,\n'
        'once per game version). Without it detection still works,\n'
        'only this list is missing.'),
    'katalog_holt':      ('Bauplan-Katalog wird geholt …',
                          'Fetching blueprint catalogue …'),
    'katalog_geholt':    ('Bauplan-Katalog geholt: %d Baupläne (%s)',
                          'Blueprint catalogue fetched: %d blueprints (%s)'),
    'ab_rang':           ('ab %s', 'from %s'),
    'annehmen_in':       ('Annehmen in', 'Available in'),
    'und_weitere':       (' und %d weiteren', ' and %d more'),
    'ruf_punkte':        ('(%s Ruf)', '(%s rep)'),
    'ruf_gewinn':        ('+%d Ruf', '+%d rep'),

    'export_ablage':     ('In die Ablage', 'To the export folder'),
    'export_einzeln':    ('Datei speichern …', 'Save file …'),
    'export_ablage_fertig': ('%d Dateien in der Ablage', '%d files in the folder'),
    'hinweis_export':    ('Bauplan-Bestand ausgeben — fürs Profit Basetool, für scmdb.net und als vollständige Sicherung',
                          'Export your blueprints — for the Profit Basetool, for scmdb.net and as a full backup'),
    'export_basetool':   ('Export fürs Basetool', 'Export for Basetool'),
    'export_alles':      ('Alles sichern', 'Export everything'),
    'export_fertig':     ('%s Baupläne gesichert', '%s blueprints saved'),
    'export_fehler':     ('Export fehlgeschlagen: %s', 'Export failed: %s'),
    'alle_dateien':      ('Alle Dateien', 'All files'),
    'filter_merk':       ('beobachtet', 'watching'),
    'filter_neu':        ('neu im Spiel', 'new in game'),
    # ⚠ Der Filter, der die unsichtbarste Falle sichtbar macht: 280 der 353
    # Auftraege haben eine Ruf-OBERGRENZE. Wer darueber steigt, bekommt sie nicht
    # mehr angeboten — und ihre Bauplaene sind fuer diesen Spielstand weg.
    # ⚠ Hiess bis 02.09.2026 „kann zugehen" — verstand niemand, auch der Autor
    # nicht: „nichtmal ich raffe was das sein soll". Der Knopf kann die
    # Spielmechanik auch nicht erklaeren, dafuer ist kein Platz. Deshalb zwei
    # Dinge: ein Name, der eine HANDLUNG nennt statt eines Zustands
    # („Aufsteigen" kennt jeder, „hoher Ruf" liest sich nicht als Gefahr) —
    # und die Warnzeile darunter, die das „wieso?" gleich mitbeantwortet.
    'filter_deckel':   ('weg beim Aufsteigen', 'lost when ranking up'),
    # Steht ueber der Liste, sobald der Filter an ist. Ohne sie bleibt die
    # Frage offen, die der Knopf ausloest.
    'deckel_warnung':  ('⚠️ Diese Baupläne gibt es nur bei Aufträgen, die '
                        'verschwinden, sobald dein Ruf zu hoch ist. Wer '
                        'aufsteigt, bekommt sie nicht mehr angeboten.',
                        '⚠️ These blueprints only come from contracts that '
                        'disappear once your reputation is too high. Rank up '
                        'and they are no longer offered to you.'),
    'deckel_leer':     ('Kein fehlender Bauplan hängt nur an Aufträgen mit '
                        'Ruf-Obergrenze. Nichts, was dir durch Aufsteigen '
                        'verloren gehen kann.',
                        'No missing blueprint depends solely on contracts with '
                        'a reputation cap. Nothing you can lose by ranking up.'),
    'deckel_zeile':    ('Zu ab %s (%s Ruf)', 'Closes at %s (%s reputation)'),
    'deckel_hilfe':    ('Aufträge haben oft eine Ruf-Obergrenze: Steigst du bei '
                        'der Fraktion darüber, werden sie dir nicht mehr '
                        'angeboten — und ihre Baupläne sind weg. Hier stehen die '
                        'Baupläne, die dir fehlen und **nur** über solche '
                        'Aufträge zu bekommen sind.',
                        'Contracts often have a reputation cap: rank up past it '
                        'with that faction and they are no longer offered — and '
                        'their blueprints are gone. Listed here are the '
                        'blueprints you are missing that are **only** available '
                        'from such contracts.'),
    # Die drei uebrigen Missions-Auskuenfte aus CIGs Vertragsdaten.
    'hk_teilbar':      ('Im Team teilbar — jeder bekommt die Baupläne',
                        'Shareable in a group — everyone gets the blueprints'),
    'hk_nicht_teilbar': ('Nicht teilbar — jeder muss ihn selbst laufen',
                         'Not shareable — everyone has to run it themselves'),
    'hk_sperre':       ('Wieder verfügbar nach %s',
                        'Available again after %s'),
    'zeit_min':        ('%d Min', '%d min'),
    'zeit_std':        ('%d Std', '%d h'),
    'zeit_std_min':    ('%d Std %d Min', '%d h %d min'),
    'zeit_tag':        ('%d Tag', '%d day'),
    'zeit_tage':       ('%d Tagen', '%d days'),
    # „Was lohnt sich" auf der Fortschritt-Seite
    's_fo_lohnt':      ('Was bringt am meisten?', 'What pays off most?'),
    's_fo_lohnt_hilfe': ('Missionstypen, aus deren Belohnungstopf dir noch die '
                         'meisten Baupläne fehlen. Die Zahl ist der Topf, '
                         'nicht die Ausbeute eines Abschlusses — je größer, '
                         'desto eher fällt etwas ab, das dir fehlt.',
                         'Mission types whose reward pool still holds the most '
                         'blueprints you are missing. The number is the pool, '
                         'not what one run pays out — the bigger it is, the '
                         'likelier something you need drops.'),
    's_fo_lohnt_zeile': ('%d fehlende Baupläne', '%d missing blueprints'),
    's_fo_lohnt_topf':  ('%d im Belohnungstopf', '%d in the reward pool'),
    's_fo_lohnt_klick': ('Anklicken zeigt, welche Baupläne das sind',
                         'Click to see which blueprints those are'),
    's_fo_lohnt_nichts': ('Zu diesem Auftrag steht kein Bauplan in der Liste.',
                          'No blueprint in the list names this contract.'),
    's_fo_lohnt_leer':  ('Kein Auftrag bringt dir noch einen Bauplan, den du '
                         'nicht hast.',
                         'No contract still holds a blueprint you do not have.'),
    # Raffinerie-Ausbeute in einem Rutsch eintragen
    's_rf_titel':      ('Raffinerie-Ausbeute eintragen',
                        'Enter refinery yield'),
    's_rf_hilfe':      ('Tipp die Zeilen so ab, wie sie im Terminal stehen — '
                        'Material, Qualität, Menge. Eine Zeile je Posten. Der '
                        'Lagerort darunter gilt für die ganze Ausbeute.',
                        'Type the rows as they appear in the terminal — '
                        'material, quality, amount. One line per entry. The '
                        'storage location below applies to the whole yield.'),
    's_rf_beispiel':   ('Titanium 295 188', 'Titanium 295 188'),
    's_rf_ort_unbekannt': ('Diesen Lagerort gibt es nicht. Nimm einen '
                           'Vorschlag — oder lass das Feld leer.',
                           'No such storage location. Pick a suggestion — or '
                           'leave the field empty.'),
    's_rf_ort':          ('Lagerort für diese Ausbeute',
                          'Storage location for this yield'),
    's_rf_einheit':    ('Menge in', 'Amount in'),
    's_rf_knopf':      ('%d Posten eintragen', 'Add %d entries'),
    's_rf_nichts':     ('Noch nichts eingetippt.', 'Nothing typed yet.'),
    's_rf_zu_kurz':    ('Zu wenig Angaben — es braucht Material, Qualität und '
                        'Menge.',
                        'Too few values — material, quality and amount needed.'),
    's_rf_keine_zahl': ('Qualität und Menge müssen Zahlen sein.',
                        'Quality and amount have to be numbers.'),
    's_rf_unbekannt':  ('„%s" gibt es nicht als Rohstoff.',
                        '"%s" is not a resource.'),
    's_rf_meintest':   ('Meintest du: %s?', 'Did you mean: %s?'),
    's_rf_qualitaet':  ('Qualität geht nur von 0 bis 1000.',
                        'Quality only goes from 0 to 1000.'),
    's_rf_menge':      ('Die Menge muss größer als null sein.',
                        'The amount has to be greater than zero.'),
    's_rf_fertig':     ('%d Posten eingetragen.', '%d entries added.'),
    'ff_alle_patches':   ('alle Patches', 'all patches'),
    'neu_leer':          ('Mit dem letzten Patch kam kein neuer Bauplan dazu. '
                          'Sobald CIG welche nachreicht, stehen sie hier.',
                          'The latest patch did not add any blueprints. As soon '
                          'as CIG adds some, they show up here.'),
    'merken':            ('Auf die Merkliste', 'Add to watchlist'),
    'nicht_mehr_merken': ('Von der Merkliste nehmen', 'Remove from watchlist'),
    'merkliste_leer':    ('Du beobachtest noch nichts. Tippe oben einen Namen '
                          'ein und klick auf den Stern.',
                          'You are not watching anything yet. Type a name above '
                          'and click the star.'),
    'merk_erledigt':     ('%s ist da — von der Merkliste genommen.',
                          '%s has arrived — removed from your watchlist.'),

    # -- Einstellungen --
    'einstellungen':     ('Einstellungen', 'Settings'),
    'status':            ('Status', 'Status'),
    'pfade':             ('Pfade', 'Paths'),
    'verhalten':         ('Verhalten', 'Behaviour'),
    'sprache':           ('Sprache', 'Language'),
    'sprache_auto':      ('automatisch (Systemsprache)', 'automatic (system language)'),
    'spielordner':       ('Spielordner (mit der Game.log darin)',
                          'Game folder (the one containing Game.log)'),
    'launcher_optional': ('SC Deutsch Launcher (optional)',
                          'SC Deutsch Launcher (optional)'),
    'durchsuchen':       ('Durchsuchen …', 'Browse …'),
    'leer_automatisch':  ('leer lassen = automatisch suchen. Gesucht wird hier:',
                          'leave empty = search automatically. Searched here:'),
    'gefunden':          ('gefunden', 'found'),
    'nicht_gefunden':    ('nicht gefunden', 'not found'),
    'pruefintervall':    ('Prüfintervall', 'Check interval'),
    'pruefintervall_hilfe': ('Wie oft die Game.log angesehen wird',
                             'How often Game.log is checked'),
    'sekunden':          ('Sek.', 'sec'),
    'signalton':         ('Signalton bei neuem Bauplan',
                          'Sound on new blueprint'),
    'signalton_hilfe':   ('Kurzer Ton, wenn etwas erscheint',
                          'Short beep when something appears'),
    'autostart_win':     ('Mit Windows starten', 'Start with Windows'),
    'autostart_linux':   ('Beim Anmelden starten', 'Start on login'),
    'autostart_hilfe':   ('Trägt den Watcher in den Autostart ein',
                          'Adds the watcher to autostart'),
    'netz_holen':        ('Craftdaten aus dem Netz holen',
                          'Fetch crafting data from the internet'),
    'netz_holen_hilfe':  ('Nur bei neuer Spielversion',
                          'Only when the game version changes'),
    'lage_zuruecksetzen': ('Fensterlage zurücksetzen', 'Reset window position'),
    'speichern':         ('Speichern', 'Save'),
    'abbrechen':         ('Abbrechen', 'Cancel'),

    # -- Erster Start --
    'einrichtung_erklaerung': (
        'Der Watcher liest die Game.log von Star Citizen — dort steht jeder '
        'freigeschaltete Bauplan. Ohne diese Datei kann er nichts anzeigen. '
        'Bitte such den Ordner heraus, in dem sie liegt (meist „LIVE"). Der '
        'Ordner darüber genügt auch, der Rest wird gefunden.',
        'The watcher reads Star Citizen\'s Game.log — every unlocked blueprint '
        'is written there. Without that file it cannot show anything. Please '
        'pick the folder it lives in (usually "LIVE"). The folder above works '
        'too, the rest is found automatically.'),
    'log_gefunden':      ('Game.log gefunden', 'Game.log found'),
    'keine_log_darin':   ('Dort liegt keine Game.log — auch nicht in den '
                          'Unterordnern.',
                          'No Game.log there — not in the subfolders either.'),
    'ordner_gedeutet':   ('Genommen wird: %s', 'Using: %s'),
    'weiter':            ('Weiter', 'Continue'),
    'sprache_erkannt':   ('Spielsprache erkannt — Baupläne werden an „%s" '
                          'erkannt.',
                          'Game language detected — blueprints are recognised '
                          'by „%s".'),
    'lese_logs_n':       ('%d aufgehobene Spielsitzungen werden gelesen …',
                          'Reading %d stored play sessions …'),
    'lese_logs':         ('Deine bisherigen Spielsitzungen werden gelesen …',
                          'Reading your previous play sessions …'),
    'nachgelesen_gross': ('%d Baupläne aus %d früheren Sitzungen übernommen.',
                          '%d blueprints taken from %d earlier sessions.'),
    'nachtragen_hinweis': (
        'Was älter ist, kannst du in der Liste von Hand abhaken — '
        'alles andere hat der Watcher schon erledigt.',
        'Anything older can be ticked off by hand in the list — '
        'the watcher has already done the rest.'),
    'liste_oeffnen':     ('Liste öffnen', 'Open list'),

    # -- Einrichtungsassistent --
    'assistent':         ('Einrichtung', 'Setup'),
    'schritt_von':       ('Schritt %d von %d', 'Step %d of %d'),
    'zurueck':           ('Zurück', 'Back'),
    'fertig':            ('Fertig', 'Done'),

    'schritt_sprache':   ('Sprache', 'Language'),
    'schritt_sprache_text': (
        'In welcher Sprache soll das Fenster mit dir reden?',
        'Which language should this window speak?'),

    'schritt_spiel':     ('Star Citizen finden', 'Find Star Citizen'),
    'schritt_spiel_text': (
        'Der Watcher liest die Game.log von Star Citizen — dort schreibt das '
        'Spiel jeden freigeschalteten Bauplan hinein. Ohne diese Datei kann er '
        'nichts anzeigen.',
        'The watcher reads Star Citizen\'s Game.log — the game writes every '
        'unlocked blueprint into it. Without that file it cannot show anything.'),
    'schritt_spiel_hilfe': (
        'Such den Ordner heraus, in dem die Game.log liegt (meist „LIVE"). '
        'Der Ordner darüber genügt auch — der Rest wird gefunden.',
        'Pick the folder containing Game.log (usually "LIVE"). The folder above '
        'works too — the rest is found automatically.'),

    'schritt_lesen':     ('Bisherige Baupläne holen', 'Collect past blueprints'),
    'schritt_lesen_text': (
        'Star Citizen hebt die Protokolle vergangener Spielsitzungen auf. Daraus '
        'holt sich der Watcher deinen bisherigen Bestand — du musst nichts '
        'eintippen.',
        'Star Citizen keeps logs of past play sessions. The watcher collects '
        'your existing blueprints from them — nothing to type in.'),

    'schritt_fertig':    ('Fertig', 'All set'),
    'schritt_fertig_text': (
        'Der Watcher läuft jetzt mit. Neue Baupläne erscheinen in der schmalen '
        'Leiste, sobald du sie im Spiel freischaltest.',
        'The watcher is running. New blueprints appear in the narrow bar as soon '
        'as you unlock them in the game.'),
    'tipp_liste':        ('Über das Klemmbrett in der Titelleiste öffnest du '
                          'jederzeit die Bauplan-Liste.',
                          'The clipboard in the title bar opens the blueprint '
                          'list at any time.'),
    'tipp_erneut':       ('Diese Einrichtung kannst du jederzeit wiederholen — '
                          'du musst dich durch keine Menüs klicken.',
                          'You can run this setup again at any time — no need to '
                          'dig through menus.'),

    # -- Neue Versionen --
    'was_ist_neu':       ('Was ist neu', 'What\'s new'),
    'neue_version_da':   ('Version %s ist da', 'Version %s is available'),
    'du_hast':           ('Du hast %s', 'You have %s'),
    'jetzt_holen':       ('Jetzt holen', 'Get it now'),
    'wird_geladen':      ('Wird geladen … %d %%', 'Downloading … %d %%'),
    # ⚠ „Beim nächsten Start" stimmt unter Windows NICHT: Dort tauscht ein
    # Hilfsskript die Datei erst, wenn das Programm beendet ist — wer
    # weiterspielt, bei dem gibt es nach zwei Minuten auf. Der Satz muss zum
    # Neustart auffordern, nicht vertrösten.
    'neustart_noetig':   ('Fertig geladen. Jetzt neu starten, damit die neue Version läuft.',
                          'Downloaded. Restart now so the new version takes over.'),
    'update_fehler':     ('Das hat nicht geklappt: %s',
                          'That did not work: %s'),
    's_ub_wird_gebaut':  ('Diese Fassung wird gerade noch gebaut — die Dateien '
                          'hängen in ein bis zwei Minuten dran. Dann noch '
                          'einmal versuchen.',
                          'This version is still being built — the files will '
                          'be attached in a minute or two. Try again then.'),
    'selbst_holen':      ('Bitte hol die neue Version selbst von der '
                          'Releases-Seite.',
                          'Please download the new version yourself from the '
                          'releases page.'),
    'update_quellcode':  ('Du startest aus dem Quellcode — hier ist „git pull" '
                          'der richtige Weg, sonst gingen deine Änderungen '
                          'verloren.',
                          'You are running from source — use "git pull" here, '
                          'otherwise your changes would be overwritten.'),
    'keine_versionen':   ('Noch keine Versionsangaben vorhanden.',
                          'No version information yet.'),
    'aktuelle_version':  ('Du hast die neueste Version.',
                          'You have the latest version.'),

    # -- Statuszeilen und Meldungen --
    'ueberwache':        ('%d Baupläne · Log %s · %s · geprüft %s',
                          '%d blueprints · log %s · %s · checked %s'),
    'mit_launcher':      ('mit Launcher', 'with launcher'),
    'craftdaten_neu':    ('scmdb-Craftdaten aktualisiert (%s, %d Gegenst\u00e4nde)',
                          'scmdb crafting data updated (%s, %d items)'),
    'ohne_launcher':     ('ohne Launcher', 'no launcher'),
    'neu_gelesen':       ('%d Protokolle noch einmal gelesen, %d Baupläne '
                          'dazugekommen.',
                          '%d logs read again, %d blueprints added.'),
    'neu_gelesen_fehler': ('Das erneute Einlesen hat nicht geklappt.',
                           'Reading the logs again did not work.'),
    'hinweis_neulesen':  ('Protokolle erneut einlesen — für den Fall, dass ein '
                          'Bauplan fehlt',
                          'Read the logs again — in case a blueprint is missing'),
    's_be_neu':          ('Protokolle erneut einlesen', 'Read the logs again'),
    's_be_neu_h':        ('Sieht jede aufgehobene Spielsitzung noch einmal durch, '
                          'auch die schon gelesenen, und trägt nach was fehlt. '
                          'Hilft, wenn der Watcher zu war, während Star Citizen '
                          'weiterlief: Die Baupläne dieser Sitzung stehen dann in '
                          'einer Datei, die er für erledigt hält. Doppelte können '
                          'dabei nicht entstehen.',
                          'Goes through every stored session again, including the '
                          'ones already read, and fills in what is missing. Helps '
                          'when the watcher was closed while Star Citizen kept '
                          'running: that session\'s blueprints then sit in a file '
                          'it considers done. Duplicates cannot happen.'),
    # ⚠⚠ Sagt jetzt „meldet sich", nicht „steht in der Leiste": Das Ergebnis
    # kommt als Fenster. Die alte Zusage stimmte nicht mehr — und sie stimmte
    # auch vorher nur vier Sekunden lang.
    's_be_neu_los':      ('Wird gelesen … das Ergebnis meldet sich, sobald es da ist.',
                          'Reading … the result will report back when it is ready.'),
    's_be_neu_kein':     ('Dafür muss der Watcher laufen.',
                          'The watcher needs to be running for this.'),
    'nachlese_marke':    ('nachgelesen', 'caught up'),
    # Angenommener Auftrag (ab v3.2.0) — die Zeile im Overlay.
    'auftrag_zeile':     ('Auftrag angenommen: %s',
                          'Contract accepted: %s'),
    'auftrag_fehlt':     ('%d Baupläne · dir fehlt: %s',
                          '%d blueprints · you are missing: %s'),
    'auftrag_fehlt_mehr': ('%d Baupläne · dir fehlen %d, darunter: %s',
                          '%d blueprints · you are missing %d, among them: %s'),
    # ⚠ „du hast alle", nicht „hast du alle" — das klingt sonst wie eine Frage.
    'auftrag_komplett':  ('%d Baupläne · du hast alle',
                          '%d blueprints · you have them all'),
    'nachgelesen':       ('Nachgelesen: %d Baupläne aus %d früheren Sitzungen '
                          'übernommen.',
                          'Caught up: %d blueprints from %d earlier sessions.'),
    'neu_craftbar':      ('neu im Spiel craftbar', 'newly craftable in game'),
    'jetzt_craftbar':    ('%s — jetzt craftbar!', '%s — now craftable!'),
    # -- Erklärtexte beim Überfahren mit der Maus --
    # Kurz halten: Sie stehen über dem Spiel und werden im Vorbeigehen gelesen.
    'hinweis_ziehen':    ('Ziehen verschiebt das Fenster',
                          'Drag to move the window'),
    'hinweis_groesse':   ('Ziehen ändert die Größe',
                          'Drag to resize'),
    'hinweis_einklappen': ('Auf die Titelleiste einklappen — gibt die Sicht frei',
                           'Collapse to the title bar — frees up the view'),
    'hinweis_ausklappen': ('Wieder aufklappen', 'Expand again'),
    'hinweis_schliessen': ('Watcher beenden', 'Quit the watcher'),
    'hinweis_leeren':    ('Angezeigte Meldungen wegräumen — die Baupläne bleiben',
                          'Clear the messages shown — your blueprints stay'),
    'hinweis_liste':     ('Alle Baupläne: suchen, filtern, abhaken',
                          'All blueprints: search, filter, tick off'),
    'hinweis_assistent': ('Einrichtung noch einmal durchgehen',
                          'Run through setup again'),
    'hinweis_versionen': ('Was ist neu — die Versionsgeschichte',
                          'What is new — the version history'),
    'hinweis_neue_version': ('Eine neuere Version ist da — hier steht, was sie bringt',
                             'A newer version is available — see what it brings'),
    'hinweis_autostart_an': ('Läuft beim Anmelden mit — Klick schaltet es aus',
                             'Starts on login — click to turn off'),
    'hinweis_autostart_aus': ('Startet nicht von selbst — Klick schaltet es ein',
                              'Does not start by itself — click to turn on'),
    # ⚠⚠ **Ein Symbol allein findet niemand.** Bis v3.5.3 stand hier nur ein
    # kleines Zeichen am rechten Rand der Zeile — Bushwick4712 (KRT) hat es am
    # 31.08.2026 schlicht nicht gefunden. Jetzt steht das Wort daneben.
    'hk_knopf':          ('Woher?', 'Where from?'),
    # Der Weg von der Herstellung zum Bauplan: „ich kann das nicht bauen — wo
    # kriege ich den Bauplan her?" Gewuenscht von Bushwick4712 (KRT).
    's_he_woher_bp':     ('Woher gibt es den Bauplan?',
                          'Where do I get the blueprint?'),
    's_he_woher_nichts': ('Zu diesem Bauplan ist keine Bezugsquelle bekannt.',
                          'No source is known for this blueprint.'),
    'hinweis_quellen':   ('Zeigt, woher es diesen Bauplan gibt',
                          'Shows where this blueprint comes from'),
    'start_eingetragen': ('%d Startbaupläne ergänzt — die hat jeder von Anfang an',
                          '%d starter blueprints added — everyone has these'),
    'hinweis_startbauplan': ('Startbauplan — den hat jeder Spieler von Anfang an',
                             'Starter blueprint — every player has this from the start'),
    'hinweis_ohne_quelle': ('Kein Auftrag bekannt, der diesen Bauplan gibt — meist eine Event-Belohnung',
                            'No known contract awards this blueprint — usually an event reward'),
    'hinweis_suche_leeren': ('Sucheingabe löschen', 'Clear the search'),

    # -- Einstellungsfenster --
    'titel_einstellungen': ('SC BP Watcher — Einstellungen',
                            'SC BP Watcher — Settings'),
    'einstellungen':     ('Einstellungen', 'Settings'),
    'hinweis_einstellungen': ('Einstellungen öffnen', 'Open settings'),
    'e_sprache':         ('Sprache', 'Language'),
    'e_sprache_hilfe':   ('Sprache dieses Fensters und aller Meldungen. Nicht zu '
                          'verwechseln mit der Sprache im Spiel — die findet der '
                          'Watcher selbst heraus.',
                          'Language of this window and all messages. Not the same '
                          'as your game language — the watcher works that one out '
                          'by itself.'),
    'e_sprache_auto':    ('Wie das System', 'Follow the system'),
    'e_spiel':           ('Star-Citizen-Ordner', 'Star Citizen folder'),
    'e_spiel_hilfe':     ('Der Ordner, in dem die Game.log liegt — meist „LIVE". '
                          'Leer lassen heißt: selbst suchen.',
                          'The folder holding Game.log — usually "LIVE". Leave '
                          'empty to search automatically.'),
    'e_launcher':        ('SC Deutsch Launcher', 'SC Deutsch Launcher'),
    'e_launcher_hilfe':  ('Optional, nur für Nutzer des Launchers: dessen Ordner '
                          '„blueprints". Ohne ihn läuft der Watcher genauso.',
                          'Optional, only for launcher users: its "blueprints" '
                          'folder. The watcher works just as well without it.'),
    'e_intervall':       ('Wie oft nachsehen', 'How often to check'),
    'e_intervall_hilfe': ('Sekunden zwischen zwei Blicken in die Game.log. '
                          'Erlaubt 1 bis 60.',
                          'Seconds between two looks at Game.log. 1 to 60 allowed.'),
    'e_deckkraft':       ('Durchsichtigkeit des Fensters', 'Window opacity'),
    'e_deckkraft_hilfe': ('100 = blickdicht, 30 = stark durchscheinend. Wer nur '
                          'einen Bildschirm hat, sieht so hindurch aufs Spiel. '
                          'Wirkt sofort.',
                          '100 = solid, 30 = strongly see-through. With a single '
                          'screen this lets you see the game underneath. Takes '
                          'effect immediately.'),
    'umzug_fertig':      ('%s Dateien in den neuen Ordner kopiert: %s',
                          '%s files copied to the new folder: %s'),
    # --- Seiten: alle sichtbaren Texte (ab v3.0.0) ---
    's_allg_lead':     ('Was fast jeder einmal einstellt und danach nie wieder anfasst.',
                          'What most people set once and never touch again.'),
    's_sprache_h':     ('Betrifft nur die Anzeige des Werkzeugs. Welche Sprache Star Citizen spricht, erkennt der Watcher selbst.',
                          'Affects only this tool. Which language Star Citizen speaks is detected on its own.'),
    's_ton_h':         ('Kurzer Ton, wenn ein Bauplan hereinkommt — hilfreich, wenn das Overlay verdeckt ist.',
                          'A short sound when a blueprint arrives — useful when the overlay is covered.'),
    's_autostart_h':   ('Der Watcher startet mit angemeldetem Benutzer und wartet im Hintergrund auf das Spiel.',
                          'The watcher starts with your session and waits in the background for the game.'),
    's_tray':          ('Symbol in der Ablage neben der Uhr',
                          'Icon in the tray next to the clock'),
    's_tray_h':        ('Beim Schließen verschwindet das Fenster in die Ablage statt zu beenden. Ein Klick holt es zurück.',
                          'Closing hides the window in the tray instead of quitting. One click brings it back.'),
    's_nur_win':       ('nur unter Windows',
                          'Windows only'),
    's_nicht_moegl':   ('hier nicht möglich',
                          'not available here'),
    's_anz_lead':      ('Wie das Overlay über dem Spiel liegt. Wer nur einen Bildschirm hat, findet hier das Wichtigste.',
                          'How the overlay sits above the game. If you only have one screen, this is where it matters.'),
    's_deck_h':        ('Weniger heißt durchsichtiger. Wird sofort vorgeführt, während du ziehst.',
                          'Less means more see-through. Shown live while you drag.'),
    's_klapp':         ('Eingeklappt starten',
                          'Start collapsed'),
    's_klapp_h':       ('Das Overlay schiebt sich beim Start auf die Titelleiste zusammen und gibt die Sicht frei. Der Pfeil in der Titelleiste klappt es jederzeit wieder auf.',
                          'The overlay folds into its title bar on start and frees the view. The arrow in the title bar unfolds it any time.'),
    's_vorne':         ('Immer im Vordergrund',
                          'Always on top'),
    's_vorne_h':       ('Bleibt über dem Spiel sichtbar. Ausschalten, wenn das Overlay im Weg ist.',
                          'Stays visible above the game. Turn off if the overlay gets in the way.'),
    'hinweis_anfasser': ('Hier wartet das Overlay — Maus darauf, dann kommt es',
                          'The overlay waits here — hover to bring it back'),
    's_ov_modus':      ('Wann das Overlay zu sehen ist',
                          'When the overlay is visible'),
    's_ov_modus_h':    ('Dauerhaft sichtbar, oder nur kurz aufblenden, wenn ein Bauplan dazukommt. Im Aufblend-Betrieb bleibt ein schmaler grüner Streifen stehen — Maus darauf, und das Overlay ist wieder da. Es bleibt, solange der Zeiger darauf ist.',
                          'Permanently visible, or briefly popping up when a blueprint arrives. In pop-up mode a narrow green strip stays behind — hover it and the overlay is back. It stays as long as the pointer is on it.'),
    's_ov_immer':      ('Immer sichtbar', 'Always visible'),
    's_ov_popup':      ('Nur bei einem Neuzugang', 'Only on a new blueprint'),
    's_ov_modus_sagen': ('Overlay: %s', 'Overlay: %s'),
    's_ov_popup_gleich': ('Overlay: nur bei einem Neuzugang — es verschwindet, sobald du dieses Fenster schließt.',
                          'Overlay: only on a new blueprint — it disappears as soon as you close this window.'),
    's_ov_dauer':      ('Wie lange es stehen bleibt (Sekunden)',
                          'How long it stays (seconds)'),
    's_ov_dauer_h':    ('Gilt nur für „Nur bei einem Neuzugang". Kommen mehrere Baupläne kurz nacheinander, zählt die Zeit von vorn.',
                          'Only applies to „Only on a new blueprint". If several arrive in a row, the time starts over.'),
    's_ov_dauer_sagen': ('Aufblenden für %d Sekunden', 'Popping up for %d seconds'),
    's_ov_durch':      ('Mausklicks ins Spiel durchreichen',
                          'Let mouse clicks through to the game'),
    's_ov_durch_h':    ('Das Overlay bleibt sichtbar, fängt aber keine Klicks mehr ab — im Kampf schießt du hindurch statt darauf. Verschieben und die Knöpfe gehen dann nicht mehr; zurück kommst du über das Schloss in der Titelleiste, das anklickbar bleibt.',
                          'The overlay stays visible but no longer catches clicks — in combat you shoot through it instead of at it. Moving it and its buttons stop working; you get back via the lock in the title bar, which stays clickable.'),
    's_ov_durch_sagen': ('Klicks durchreichen: %s', 'Clicks passed through: %s'),
    's_ov_durch_nein': ('Auf diesem System nicht möglich: Unter Wayland kann ein gewöhnliches Fenster keine Klicks weiterreichen.',
                          'Not possible on this system: under Wayland an ordinary window cannot pass clicks on.'),
    # --- Texte der Melde-Leiste (Overlay) ------------------------------------
    # Diese vier standen bis 26.08.2026 fest auf Deutsch im Code. Ergebnis: Wer
    # auf Englisch umstellte, bekam ein englisches Hauptfenster und ein
    # deutsches Overlay. Gemeldet.
    'ov_starte':       ('Starte \u2026', 'Starting \u2026'),
    # Die Anzeige der laufenden Auftraege. ⚠ „laut Log" steht bewusst dabei:
    # Geht ein Auftrag durch einen Fehler im Spiel verloren, meldet das Spiel
    # nichts — der Watcher wuerde ihn weiter fuehren. Also behaupten wir nicht,
    # dass er laeuft, sondern sagen, woher wir es haben.
    'ov_auftraege_kopf': ('Laufende Aufträge (laut Log)',
                          'Active contracts (per log)'),
    # Wegklicken von Hand, fuer genau den Fall oben — und fuer den, in dem man
    # ausloggen musste, um einen Fehler loszuwerden.
    'ov_auftrag_weg':    ('Diesen Auftrag ausblenden',
                          'Hide this contract'),
    # Die Zwischenziele stehen eingerueckt unter ihrem Auftrag. Passen nicht
    # alle hin, wird der Rest GEZAEHLT — eine abgeschnittene Liste, die sich
    # fuer vollstaendig ausgibt, waere schlimmer als gar keine.
    'ov_ziele_mehr':     ('… und %d weitere', '… and %d more'),
    # Das Kreuz im Suchfeld. ⚠ Ein Feld ohne sichtbaren Weg zurueck laesst
    # Leute den Text markieren und loeschen — oder sie glauben, die Liste sei
    # kurz, weil nichts da ist.
    's_suche_leeren':    ('Suche leeren', 'Clear search'),
    # Stueckzahl beim Herstellen. ⚠ Ohne sie klickt man zehnmal und verzaehlt
    # sich beim elften — dann stimmt der Bestand nicht mehr, ohne dass es
    # auffaellt. Am 29.08.2026 gemeldet.
    's_lg_anzahl':       ('Anzahl', 'How many'),
    # Lager sichern und zurueckholen.
    's_lg_ausgeben':     ('Lager ausgeben', 'Export stock'),
    's_lg_aus_json':     ('Als Sicherung (.json)', 'As backup (.json)'),
    's_lg_aus_csv':      ('Als Tabelle (.csv)', 'As spreadsheet (.csv)'),
    's_lg_einlesen':     ('Sicherung einlesen', 'Load backup'),
    's_lg_gespeichert':  ('Gespeichert: %s', 'Saved: %s'),
    's_lg_eingelesen':   ('%d Posten eingelesen — dein Lager wurde ersetzt.',
                          '%d entries loaded — your stock was replaced.'),
    's_lg_datei_falsch': ('Diese Datei ist keine Lager-Sicherung.',
                          'That file is not a stock backup.'),
    # ⚠ Rot und mit Rückfrage. Das Lager ist Handarbeit, die sonst nirgends
    # liegt — kein Log, keine Datenquelle, nur die eigenen Eingaben. Ein
    # versehentlicher Klick waere unwiederbringlich.
    's_lg_leeren':       ('Lager löschen', 'Clear stock'),
    # Abbauart im Lager: Wer „Iron" einträgt, will sehen, ob er dafür mit dem
    # Multi-Tool loszieht oder ein Schiff braucht.
    's_lg_sp_abbau':     ('Abbau', 'Mining'),
    's_lg_abbau_fps':    ('Hand', 'Hand'),
    's_lg_abbau_fahrzeug': ('Fahrzeug', 'Vehicle'),
    's_lg_abbau_schiff': ('Schiff', 'Ship'),
    's_lg_suche':        ('Im Lager suchen …', 'Search stock …'),
    's_lg_posten_weg':   ('Diesen Posten löschen', 'Delete this entry'),
    's_lg_posten_frage_t': ('Posten löschen?', 'Delete entry?'),
    's_lg_posten_frage': ('%s (%g SCU) wird aus dem Lager genommen.',
                          '%s (%g SCU) will be removed from your stock.'),
    's_lg_leeren_frage_t': ('Wirklich das ganze Lager löschen?',
                            'Really clear the whole stock?'),
    's_lg_leeren_frage': ('%d Posten werden entfernt. Das lässt sich nicht '
                          'rückgängig machen — sichere vorher, wenn du sie '
                          'noch brauchst.',
                          '%d entries will be removed. This cannot be undone — '
                          'export first if you still need them.'),
    's_lg_geleert':      ('Lager geleert — %d Posten entfernt.',
                          'Stock cleared — %d entries removed.'),
    's_lg_aus_hilfe':    ('Die Sicherung lässt sich hier wieder einlesen. Die '
                          'Tabelle ist zum Ansehen und Weitergeben — sie kann '
                          'nicht zurückgelesen werden.',
                          'The backup can be loaded here again. The '
                          'spreadsheet is for reading and sharing — it cannot '
                          'be loaded back.'),
    's_lg_abgezogen_n':  ('%d× hergestellt — Zutaten abgezogen.',
                          'Made %d× — materials deducted.'),
    'ov_warte':        ('Warte auf neue Baupl\u00e4ne \u2026',
                        'Waiting for new blueprints \u2026'),
    'ov_as_fehler':    ('Autostart lie\u00df sich nicht \u00e4ndern.',
                        'Could not change the autostart setting.'),
    'ov_durchklick_geht_nicht': ('Klicks durchreichen hat auf diesem System nicht geklappt.',
                          'Passing clicks through did not work on this system.'),
    's_zeilen':        ('Zeilen im Overlay',
                          'Rows in the overlay'),
    's_zeilen_h':      ('So viele Neuzugänge bleiben stehen, ältere rutschen heraus. Die vollständige Liste steht ohnehin im Bauplan-Fenster.',
                          'This many new entries stay; older ones drop off. The full list is in the blueprint window anyway.'),
    'as_menue_frage':  ('Soll das Werkzeug im Startmenü stehen? Dann findest du es wieder, ohne die Datei zu suchen — und kannst dem Eintrag eine Tastenkombination geben.',
                          'Should the tool appear in your application menu? Then you can find it again without hunting for the file — and give the entry a keyboard shortcut.'),
    'as_menue_knopf':  ('In das Startmenü eintragen', 'Add to the application menu'),
    'as_menue_da':     ('Eingetragen: %s', 'Added: %s'),
    'as_menue_nein':   ('Hat nicht geklappt: %s', 'Did not work: %s'),
    's_ub_holen':      ('%s holen', 'Get %s'),
    's_ub_neustart':   ('Jetzt neu starten', 'Restart now'),
    's_ub_bereit':     ('Fertig geladen — ein Neustart, dann läuft die neue Version.',
                          'Downloaded — one restart and the new version runs.'),
    's_ub_startet_neu': ('Startet neu …', 'Restarting …'),
    # ⚠ Der Fall, den es vorher gar nicht gab: Der Start hat geklappt, die neue
    # Version ist aber sofort wieder gestorben. Bis rc66 trat die alte trotzdem
    # ab, und der Rechner stand ohne Watcher da — ohne ein Wort dazu.
    's_ub_neustart_tot': ('Die neue Version ist nicht hochgekommen. Der Watcher '
                         'bleibt offen — bitte starte ihn von Hand neu.',
                         'The new version did not come up. The watcher stays '
                         'open — please restart it by hand.'),
    's_ub_neustart_nein': ('Neustart ging nicht — bitte von Hand beenden und starten.',
                          'Restart failed — please close and start it yourself.'),
    's_ub_holen_zurueck': ('zurück auf %s', 'back to %s'),
    's_ub_holen_gleich': ('%s ist schon installiert', '%s is already installed'),
    # ⚠ „Noch keine Version bekannt“ klingt nach einem Fehler und sagt nicht,
    # was zu tun ist. Genau dieser Knopf stand bei Morkhan da (26.08.2026).
    's_ub_holen_keine': ('Erst oben auf „Jetzt nachsehen“ drücken',
                        'Press “Check now” above first'),
    's_ub_holen_laeuft': ('%s wird geholt …', 'Fetching %s …'),
    's_ub_auf':        ('Im Browser geöffnet: %s', 'Opened in the browser: %s'),
    's_ub_auf_nein':   ('Ließ sich nicht öffnen: %s', 'Could not be opened: %s'),
    'b_spur':          ('Startverlauf des letzten Laufs (die letzte Zeile sagt, wie weit es kam)',
                          'Start trace of the last run (the last line shows how far it got)'),
    'b_spur_seiten':   ('Zuletzt geöffnete Seiten (die letzte Zeile ohne „steht“ ist die, an der es hing)',
                          'Pages opened last (the last line without "ready" is where it hung)'),
    'b_absturz':       ('Harter Abbruch beim vorigen Lauf — das Programm wurde mitten im Befehl beendet',
                          'Hard crash during the previous run — the program was killed mid-instruction'),
    'b_fehler_alt':    ('(aus einer älteren Version — vermutlich längst behoben)',
                          '(from an older version — most likely fixed since)'),
    's_sp_start_knopf': ('Star Citizen starten', 'Launch Star Citizen'),
    's_sp_start_lauft': ('Star Citizen wird gestartet …', 'Starting Star Citizen …'),
    's_sp_kein_starter': ('kein Starter gefunden', 'no launcher found'),
    'up_fremde_quelle': ('Datei kommt nicht von GitHub',
                          'File does not come from GitHub'),
    'b_woher_ini':     ('aus der global.ini des Spiels',
                          "from the game's global.ini"),
    'b_woher_eigen':   ('aus eigener Angabe', 'from your own entry'),
    'b_woher_tabelle': ('aus der eingebauten Tabelle',
                          'from the built-in table'),
    'up_fremde_datei': ('Zieldatei geh\u00f6rt nicht zu diesem Programm: %s',
                          'Target file does not belong to this program: %s'),
    's_sp_start_nein': ('Start nicht möglich: %s', 'Could not start: %s'),
    'tray_zeigen':     ('Fenster zeigen', 'Show window'),
    'tray_beenden':    ('Beenden', 'Quit'),
    's_menue':         ('Eintrag im Startmenü', 'Entry in the application menu'),
    's_menue_h':       ('Legt einen Eintrag für dich an — dort lässt sich auch eine Tastenkombination hinterlegen, mit der du das Fenster zurückholst.',
                          'Creates an entry for you — you can also put a keyboard shortcut on it to bring the window back.'),
    's_menue_anlegen': ('Eintragen', 'Add'),
    's_menue_weg':     ('Wieder entfernen', 'Remove again'),
    's_menue_steht':   ('Steht im Startmenü.', 'It is in the application menu.'),
    's_menue_weg_ok':  ('Aus dem Startmenü entfernt.', 'Removed from the menu.'),
    's_lage':          ('Fensterlage vergessen',
                          'Forget window position'),
    's_lage_h':        ('Setzt Größe und Position zurück, falls das Overlay einmal außerhalb des Bildschirms gelandet ist.',
                          'Resets size and position in case the overlay ended up off-screen.'),
    's_zuruecksetzen': ('Zurücksetzen',
                          'Reset'),
    's_ordner_lead':   ('Wo Star Citizen liegt und wohin das Werkzeug seine eigenen Dateien schreibt. Leer heißt: selbst suchen.',
                          'Where Star Citizen lives and where this tool writes its own files. Empty means: search automatically.'),
    's_sc_da':         ('Star Citizen gefunden.',
                          'Star Citizen found.'),
    's_sc_weg':        ('Star Citizen nicht gefunden.',
                          'Star Citizen not found.'),
    's_sc_weg_h':      ('Trag den Ordner unten ein — der LIVE-Ordner reicht, auch der darüber oder das Wine-Präfix.',
                          'Enter the folder below — the LIVE folder is enough, as is the one above it or the Wine prefix.'),
    # ⚠ Der Spielordner ist weg, ein Nachbarkanal steht daneben — siehe
    # `pfade.kanal_abweichung()`. Wer LIVE in HOTFIX umbenennt (der uebliche Weg
    # zu einer ausgebesserten Fassung), soll das nicht selbst nachtragen muessen.
    's_kn_titel':      ('Spielordner hat sich geändert',
                          'Game folder has changed'),
    's_kn_weg':        ('Der eingetragene Ordner „%s" ist nicht mehr da.',
                          'The folder you set, "%s", is gone.'),
    's_kn_da':         ('Daneben liegt „%s" — dort wurde zuletzt gespielt.',
                          'Next to it sits "%s" — that is where you played last.'),
    's_kn_frage':      ('Soll ich von jetzt an dort mitlesen?',
                          'Shall I read from there from now on?'),
    's_kn_mehrere':    ('Es liegen mehrere Spielordner nebeneinander. Welchen soll ich mitlesen?',
                          'Several game folders sit side by side. Which one shall I read?'),
    's_kn_zuletzt':    ('zuletzt gespielt: %s',
                          'last played: %s'),
    's_kn_umgestellt': ('Ab jetzt wird „%s" mitgelesen.',
                          'Reading from "%s" from now on.'),
    's_kn_spaeter':    ('Jetzt nicht',
                          'Not now'),
    # Das Auftrags-Protokoll — welche Auftraege wann gespielt wurden.
    'hf_auftragslog':  ('Auftrags-Protokoll', 'Mission log'),
    's_al_lead':       ('Welche Aufträge du wann gespielt hast — und wie oft.',
                          'Which missions you played when — and how often.'),
    's_al_hinweis':    ('Gelesen wird, was in den Protokollen des Spiels steht. '
                        'Das Spiel hebt nur wenige Sitzungen auf; hier bleiben '
                        'sie stehen, auch wenn das Spiel sie längst gelöscht hat.',
                          'Built from the game\'s own logs. The game keeps only a '
                          'few sessions; here they stay, long after the game has '
                          'dropped them.'),
    's_al_suche':      ('Auftrag suchen', 'Search missions'),
    's_al_leer':       ('Noch kein Auftrag aufgezeichnet. Sobald du einen '
                        'annimmst, steht er hier.',
                          'No missions recorded yet. As soon as you accept one, '
                          'it shows up here.'),
    's_al_nichts':     ('Kein Auftrag passt zu deiner Suche.',
                          'No mission matches your search.'),
    's_al_laeuft':     ('läuft', 'in progress'),
    's_al_fertig':     ('abgeschlossen', 'completed'),
    's_al_abbruch':    ('abgebrochen', 'abandoned'),
    # Kein Ende im Log, aber eine spätere Sitzung kannte ihn nicht mehr.
    # Bewusst nicht „abgebrochen“ — warum er endete, steht nirgends.
    's_al_verfallen':  ('nicht mehr offen', 'no longer open'),
    's_al_ziele':      ('%d von %d Zielen', '%d of %d objectives'),
    's_al_oft':        ('%d× gespielt · %d abgeschlossen',
                          'played %d× · %d completed'),
    's_al_anzahl':     ('%d Aufträge', '%d missions'),
    's_al_oft_kopf':   ('Mehrfach gespielt', 'Played more than once'),
    's_al_bp':         ('Bauplan: %s', 'Blueprint: %s'),
    's_al_bp_mehr':    ('Baupläne: %s', 'Blueprints: %s'),
    # Die Joystick-Seite — welcher Stick ist welche Nummer, und was liegt drauf.
    # ⚠ „Steuerung", nicht „Joysticks": Auf der Seite stehen auch Tastatur,
    # Maus und Gamepad. Wer sucht, wo seine Tastenbelegung ist, klickt keinen
    # Reiter namens „Joysticks" an. Der Schlüssel heißt aus Bestandsgründen
    # weiter `hf_joysticks`.
    'hf_joysticks':    ('Steuerung', 'Controls'),
    's_js_lead':       ('Welcher Stick welche Nummer hat — und was auf '
                        'welcher Taste liegt.',
                          'Which stick has which number — and what is bound to '
                          'which key.'),
    's_js_hinweis':    ('Alles hier kommt aus den Dateien des Spiels: die '
                        'Geräte aus dem Startprotokoll, deine Änderungen aus '
                        'der actionmaps.xml, die Werkseinstellung und die '
                        'Bezeichnungen aus dem Spiel selbst. Joystick, '
                        'Tastatur, Maus und Gamepad — alles in einer Liste.',
                          'Everything here comes from the game\'s own files: the '
                          'devices from the startup log, your changes from '
                          'actionmaps.xml, the defaults and the wording from the '
                          'game itself. Joystick, keyboard, mouse and gamepad — '
                          'all in one list.'),
    's_js_geraete':    ('Verbundene Geräte', 'Connected devices'),
    's_js_belegt':     ('Belegung in der actionmaps.xml', 'Bindings in actionmaps.xml'),
    's_js_leer':       ('Noch keine Geräte gefunden. Starte Star Citizen '
                        'einmal — danach steht hier, was das Spiel erkannt hat.',
                          'No devices found yet. Start Star Citizen once — after '
                          'that this shows what the game detected.'),
    's_js_keine_datei': ('Keine actionmaps.xml gefunden. Sie entsteht, sobald '
                         'du im Spiel eine Taste belegst.',
                           'No actionmaps.xml found. It appears once you bind a '
                           'key inside the game.'),
    's_js_passt':      ('Alles in Ordnung — jedes belegte Gerät ist verbunden.',
                          'All good — every bound device is connected.'),
    's_js_fehlt':      ('%d belegte(s) Gerät(e) sind gerade nicht verbunden. '
                        'Solange das so ist, kann das Spiel die Belegung neu '
                        'vergeben.',
                          '%d bound device(s) are not connected right now. While '
                          'that is the case, the game may reassign the bindings.'),
    's_js_ersetzt':    ('Ein Gerät meldet sich unter einer neuen Kennung. Die '
                        'alte Belegung hängt dadurch ins Leere.',
                          'A device reports under a new identifier. Its old '
                          'bindings now point nowhere.'),
    's_js_ersatz_frage': ('„%s" durch „%s" ersetzen? Die Belegung wird '
                          'übernommen; vorher entsteht eine Sicherung.',
                            'Replace "%s" with "%s"? The bindings are carried '
                            'over; a backup is written first.'),
    's_js_uebernehmen': ('Belegung übernehmen', 'Carry bindings over'),
    's_js_spiel_zu':   ('⚠ Nur bei geschlossenem Spiel — Star Citizen '
                        'schreibt die Datei beim Beenden selbst.',
                          '⚠ Only while the game is closed — Star Citizen writes '
                          'the file itself when it exits.'),
    's_js_fertig':     ('Übernommen. Sicherung: %s',
                          'Done. Backup: %s'),
    's_js_schief':     ('Nicht übernommen: %s', 'Not carried over: %s'),
    's_js_platz':      ('Platz %d', 'Slot %d'),
    's_js_ohne':       ('keine Belegung', 'no bindings'),
    's_js_bindungen':  ('%d Belegungen', '%d bindings'),
    's_js_suche':      ('Belegung suchen', 'Search bindings'),
    's_js_nichts':     ('Keine Belegung passt zu deiner Suche.',
                          'No binding matches your search.'),
    's_js_alle':       ('Alle Geräte', 'All devices'),
    # Die Gründe, warum ein Übernehmen nicht geklappt hat. `joysticks.py` gibt
    # diese Schlüssel zurück, keine fertigen Sätze — sonst stünde deutscher
    # Text in der englischen Oberfläche.
    's_js_f_datei':    ('Es wurde keine actionmaps.xml gefunden.',
                          'No actionmaps.xml was found.'),
    's_js_f_nichts':   ('Es gibt nichts zu übernehmen.',
                          'There is nothing to carry over.'),
    's_js_f_unbekannt': ('Die alte Kennung steht nicht in der Datei.',
                           'The old identifier is not in the file.'),
    's_js_f_gleich':   ('Die Datei ist bereits auf diesem Stand.',
                          'The file is already up to date.'),
    's_js_f_sicherung': ('Die Sicherung ließ sich nicht schreiben — es wurde '
                         'nichts geändert.',
                           'The backup could not be written — nothing was '
                           'changed.'),
    's_js_f_schreiben': ('Die Datei ließ sich nicht schreiben. Der alte Stand '
                         'wurde zurückgeholt.',
                           'The file could not be written. The previous state '
                           'was restored.'),
    's_js_f_lesen':    ('Die Datei ließ sich nicht lesen.',
                          'The file could not be read.'),
    # Der Profilname wird zum Dateinamen — und zu dem, was der Spieler im
    # Spiel eintippt (`pp_rebindkeys load <Name>`). Deshalb die Grenzen.
    's_js_f_name_leer': ('Gib dem Profil einen Namen.',
                           'Give the profile a name.'),
    's_js_f_name_zeichen': ('Im Namen sind nur Buchstaben, Ziffern, '
                            'Bindestrich und Unterstrich erlaubt — keine '
                            'Leerzeichen.',
                              'The name may only contain letters, digits, '
                              'hyphens and underscores — no spaces.'),
    's_js_f_name_lang': ('Der Name ist zu lang.',
                           'The name is too long.'),
    's_js_f_name_belegt': ('Ein Profil mit diesem Namen gibt es schon.',
                             'A profile with this name already exists.'),
    # Die drei Sichten auf die Belegung — dieselbe Einteilung, die auch das
    # Spiel selbst kennt.
    's_js_a_tastatur': ('Tastatur', 'Keyboard'),
    's_js_a_maus':     ('Maus', 'Mouse'),
    's_js_a_gamepad':  ('Gamepad', 'Gamepad'),
    's_js_s_meine':    ('Von mir geändert', 'Changed by me'),
    's_js_s_alles':    ('Alles', 'Everything'),
    's_js_s_standard': ('Werkseinstellung', 'Default'),
    's_js_sicht':      ('Anzeigen', 'Show'),
    's_js_q_meine':    ('geändert', 'changed'),
    # Eingaben im Klartext. ⚠ „Achse X" statt „x": Auf einer Tastatur ist `x`
    # ein Buchstabe — in der Liste eines Sticks las sich die Zeile falsch.
    # ⚠ Im Deutschen heißt es „X-Achse", nicht „Achse X" — im Englischen
    # umgekehrt „Axis X". Deshalb zwei verschiedene Satzstellungen.
    's_js_e_achse':     ('%s-Achse', 'Axis %s'),
    's_js_e_drehachse': ('%s-Drehachse', 'Rotation %s'),
    's_js_e_knopf':     ('Knopf %d', 'Button %d'),
    's_js_e_schieber':  ('Schieber %d', 'Slider %d'),
    's_js_e_hut':       ('Hut %d %s', 'Hat %d %s'),
    's_js_t_np':        ('Ziffernblock %s', 'Numpad %s'),
    's_js_t_lshift':    ('Umschalt links', 'Left Shift'),
    's_js_t_rshift':    ('Umschalt rechts', 'Right Shift'),
    's_js_t_lctrl':     ('Strg links', 'Left Ctrl'),
    's_js_t_rctrl':     ('Strg rechts', 'Right Ctrl'),
    's_js_t_lalt':      ('Alt links', 'Left Alt'),
    's_js_t_ralt':      ('Alt Gr', 'Right Alt'),
    's_js_t_space':     ('Leertaste', 'Space'),
    's_js_t_enter':     ('Eingabe', 'Enter'),
    's_js_t_escape':    ('Esc', 'Esc'),
    's_js_t_backspace': ('Rücktaste', 'Backspace'),
    's_js_t_tab':       ('Tab', 'Tab'),
    's_js_t_comma':     ('Komma', 'Comma'),
    's_js_t_period':    ('Punkt', 'Period'),
    's_js_t_slash':     ('Schrägstrich', 'Slash'),
    's_js_t_minus':     ('Minus', 'Minus'),
    's_js_t_equals':    ('Gleich', 'Equals'),
    's_js_t_up':        ('Pfeil ↑', 'Arrow ↑'),
    's_js_t_down':      ('Pfeil ↓', 'Arrow ↓'),
    's_js_t_left':      ('Pfeil ←', 'Arrow ←'),
    's_js_t_right':     ('Pfeil →', 'Arrow →'),
    's_js_t_home':      ('Pos 1', 'Home'),
    's_js_t_end':       ('Ende', 'End'),
    's_js_t_pgup':      ('Bild ↑', 'Page Up'),
    's_js_t_pgdn':      ('Bild ↓', 'Page Down'),
    's_js_t_insert':    ('Einfg', 'Insert'),
    's_js_t_delete':    ('Entf', 'Delete'),
    's_js_t_pause':     ('Pause', 'Pause'),
    's_js_t_lbracket':  ('Klammer auf', 'Left bracket'),
    's_js_t_rbracket':  ('Klammer zu', 'Right bracket'),
    's_js_t_mouse1':    ('Maus links', 'Mouse left'),
    's_js_t_mouse2':    ('Maus rechts', 'Mouse right'),
    's_js_t_mouse3':    ('Maus Mitte', 'Mouse middle'),
    's_js_t_mwheel_up': ('Mausrad ↑', 'Wheel ↑'),
    's_js_t_mwheel_down': ('Mausrad ↓', 'Wheel ↓'),
    # Die vierte Sicht — Aktionen, auf die noch nichts zeigt.
    's_js_s_frei':      ('Noch nicht belegt', 'Not bound yet'),
    's_js_frei_hinweis': ('Diese Aktionen haben noch keine Taste. Zeile '
                          'anklicken und drücken, was du dafür haben willst.',
                            'These actions have no key yet. Click a row and '
                            'press whatever you want for it.'),
    's_js_ohne_eingabe': ('—', '—'),
    # Zurücksetzen, Ausgeben, Einlesen.
    's_js_zurueck':    ('Auf Werkseinstellung zurücksetzen',
                          'Reset to defaults'),
    's_js_zurueck_frage': ('Wirklich alle eigenen Tastenbelegungen '
                           'verwerfen?\n\nDanach gilt überall wieder die '
                           'Werkseinstellung des Spiels — deine %d eigenen '
                           'Belegungen sind weg.\n\nTotzonen, Kurven und '
                           'Empfindlichkeit bleiben unangetastet. Eine '
                           'Sicherung wird vorher angelegt.',
                             'Really discard all your own key bindings?\n\nThe '
                             'game\'s defaults apply everywhere afterwards — '
                             'your %d own bindings will be gone.\n\nDeadzones, '
                             'curves and sensitivity stay untouched. A backup '
                             'is written first.'),
    's_js_zurueck_ok': ('Zurückgesetzt. Sicherung: %s',
                          'Reset. Backup: %s'),
    's_js_ausgeben':   ('Belegung sichern', 'Export bindings'),
    's_js_ausgeben_csv': ('Als Liste (CSV)', 'As a list (CSV)'),
    's_js_einlesen':   ('Belegung einspielen', 'Import bindings'),
    's_js_ausgabe_ok': ('Gesichert: %s', 'Exported: %s'),
    # Ein Profil ist etwas anderes als eine gesicherte Datei: Es liegt dort, wo
    # das Spiel es sucht, und laesst sich dort unter seinem Namen laden.
    's_js_profil':     ('Als Profil speichern', 'Save as profile'),
    's_js_profil_frage': ('Buchstaben, Ziffern, Bindestrich und Unterstrich — '
                          'keine Leerzeichen.\nIm Spiel lädst du das Profil '
                          'dann mit:  pp_rebindkeys load NAME',
                            'Letters, digits, hyphens and underscores — no '
                            'spaces.\nIn game you then load the profile with:  '
                            'pp_rebindkeys load NAME'),
    's_js_profil_liste': ('Vorhanden — anklicken übernimmt den Namen:',
                            'Existing — click one to reuse its name:'),
    's_js_profil_keine': ('noch keine', 'none yet'),
    's_js_einlesen_woher': ('Woher soll die Belegung kommen?',
                              'Where should the bindings come from?'),
    's_js_einlesen_profil': ('Aus meinen Profilen', 'From my profiles'),
    's_js_einlesen_datei': ('Aus einer Datei', 'From a file'),
    's_js_einlesen_waehlen': ('Welches Profil soll ins Spiel?',
                                'Which profile should go into the game?'),
    's_js_profil_ersetzen': ('Es gibt schon ein Profil „%s". Ersetzen?',
                               'A profile "%s" already exists. Replace it?'),
    's_js_profil_ok':  ('Gespeichert als „%s".\n\nIm Spiel laden mit:\n'
                        '  pp_rebindkeys load %s',
                          'Saved as "%s".\n\nLoad it in game with:\n'
                          '  pp_rebindkeys load %s'),
    's_js_einlesen_ok': ('Eingespielt — %d Gruppen. Sicherung: %s',
                           'Imported — %d groups. Backup: %s'),
    's_js_f_fremd':    ('Das sieht nicht nach einer Belegungsdatei von Star '
                        'Citizen aus.',
                          'That does not look like a Star Citizen bindings '
                          'file.'),
    's_js_einlesen_frage': ('Deine jetzige Belegung wird dabei ersetzt. Eine '
                            'Sicherung wird vorher angelegt. Fortfahren?',
                              'Your current bindings will be replaced. A backup '
                              'is written first. Continue?'),
    # Das Fenster zum Neubelegen.
    's_js_b_titel':    ('Neu belegen', 'Rebind'),
    's_js_b_druecke':  ('Drücke jetzt die Taste, den Knopf oder die Maustaste, '
                        'die du haben willst.',
                          'Now press the key, button or mouse button you want.'),
    's_js_b_nochmal':  ('Vertippt? Einfach nochmal drücken.',
                          'Wrong one? Just press again.'),
    's_js_b_nur_tastatur': ('Es wurde kein Joystick gefunden — Tastatur und '
                            'Maus gehen trotzdem.',
                              'No joystick found — keyboard and mouse still work.'),
    's_js_b_fremd':    ('Dieses Gerät steht in keiner Belegung. Starte Star '
                        'Citizen einmal damit, dann kennt das Spiel es.',
                          'This device is not in any binding yet. Start Star '
                          'Citizen once with it, then the game knows it.'),
    's_js_b_bisher':   ('Bisher: %s', 'Currently: %s'),
    's_js_b_konflikt': ('⚠ Liegt schon auf: %s', '⚠ Already used by: %s'),
    's_js_b_uebernehmen': ('Übernehmen', 'Apply'),
    's_js_b_loeschen': ('Belegung entfernen', 'Remove binding'),
    's_js_b_abbruch':  ('Abbrechen', 'Cancel'),
    's_js_b_hinweis':  ('Eine Zeile anklicken, um sie neu zu belegen.',
                          'Click a row to rebind it.'),
    's_js_keine_namen': ('Die Bezeichnungen des Spiels ließen sich nicht '
                         'lesen — es stehen die technischen Namen da.',
                           'The game\'s wording could not be read — technical '
                           'names are shown instead.'),
    # ⚠⚠ **„Eigene Dateien" sagte niemandem etwas.** Am 31.08.2026 gemeldet:
    # „da fehlt auch die Beschreibung, für was der Ordner ist, der Name sagt
    # nichts aus." Und der zweite Teil fehlte ganz: Dieser eine Ordner ist
    # der Weg, alles zwischen zwei Rechnern zu teilen — Bestand UND beide
    # Lager. Genau danach wurde am selben Tag gefragt, weil das Lager unter
    # Windows fehlte.
    's_eigene':        ('Ordner für deine Daten',
                          'Folder for your data'),
    's_eigene_h':      ('Alles, was das Werkzeug über dich weiß, liegt hier: '
                        'Bauplan-Bestand, Merkliste, Werkstatt-Lager, '
                        'Handelslager, Einstellungen und die ausgegebenen '
                        'Dateien — in getrennten Unterordnern.\n\n'
                        'Spielst du auf zwei Rechnern? Dann stell hier auf '
                        'beiden denselben Ordner ein (Cloud, Netzlaufwerk, '
                        'zweite Platte) — und beide arbeiten mit demselben '
                        'Stand. ⚠ Der Ordner wird dabei nur umgestellt, nicht '
                        'kopiert: Sichere deinen bisherigen Stand vorher unter '
                        '„Bauplan-Bestand → Bestand ausgeben".',
                          'Everything the tool knows about you lives here: '
                          'blueprint inventory, watchlist, workshop stock, '
                          'trade stock, settings and exported files — in '
                          'separate subfolders.\n\n'
                          'Playing on two machines? Point both at the same '
                          'folder here (cloud, network drive, second disk) and '
                          'both work from the same state. ⚠ Switching only '
                          'changes the folder, it does not copy: save your '
                          'current state first under "Blueprint inventory → '
                          'Export inventory".'),
    # ⚠⚠ **Der Grund fuer die Tastenkombination.** Star Citizen laeuft im
    # Vollbild und blendet den Mauszeiger aus: Wer nachsehen will, ob er einen
    # Bauplan schon hat, muss heraustabben und das Fenster dann BLIND suchen
    # und anklicken. Am 31.08.2026 als Nutzerwunsch gemeldet.
    # ⚠⚠ **Ziehen geht im Pop-up-Betrieb nicht.** Das Overlay ist dort
    # durchklickbar, damit es im Kampf nicht stoert — und was Mausklicks
    # durchreicht, laesst sich auch nicht anfassen. Ohne waehlbare Ecke gibt es
    # fuer diese Nutzer GAR KEINEN Weg, das Overlay zu positionieren.
    # Am 31.08.2026 gemeldet: „stoert mich irgendwie, dass es nicht komplett in
    # der Ecke sitzt."
    's_ov_ecke':       ('Wo das Overlay sitzt',
                          'Where the overlay sits'),
    's_ov_ecke_h':     ('Legt das Overlay fest in eine Bildschirmecke — auch '
                        'eingeklappt. ⚠ Im Pop-up-Betrieb ist das der '
                        'einzige Weg: Dort reicht das Overlay Mausklicks durch '
                        'und laesst sich deshalb nicht ziehen.',
                          'Pins the overlay to a screen corner — collapsed too. '
                          '⚠ In pop-up mode this is the only way: there '
                          'the overlay passes mouse clicks through and cannot '
                          'be dragged.'),
    's_ov_ecke_frei':  ('Frei verschiebbar', 'Free to move'),
    's_ov_ecke_ol':    ('Oben links', 'Top left'),
    's_ov_ecke_or':    ('Oben rechts', 'Top right'),
    's_ov_ecke_ul':    ('Unten links', 'Bottom left'),
    's_ov_ecke_ur':    ('Unten rechts', 'Bottom right'),
    's_hk':            ('Tastenkombination',
                          'Keyboard shortcut'),
    's_hk_h':          ('Holt die Bauplan-Liste nach vorn — auch aus dem '
                        'laufenden Spiel heraus. Strg, Alt und Umschalt lassen '
                        'sich mit einem Buchstaben, einer Ziffer oder F1 bis '
                        'F12 verbinden.\n\n'
                        '⚠ Ohne Modifikator geht es nicht: Eine nackte '
                        'Taste global zu belegen hieße, sie im Spiel '
                        'unbrauchbar zu machen.',
                          'Brings the blueprint list to the front — even from '
                          'inside the running game. Ctrl, Alt and Shift can be '
                          'combined with a letter, a digit or F1 to F12.\n\n'
                          '⚠ Without a modifier it will not work: '
                          'claiming a bare key system-wide would make it '
                          'useless in the game.'),
    's_hk_an':         ('Tastenkombination benutzen',
                          'Use the keyboard shortcut'),
    's_hk_ok':         ('%s ist angemeldet.', '%s is registered.'),
    's_hk_belegt':     ('%s hat schon ein anderes Programm. Nimm eine andere.',
                          '%s is already taken by another program. Pick a '
                          'different one.'),
    's_hk_falsch':     ('Das ergibt keine Kombination. Beispiel: Strg+Alt+B',
                          'That is not a valid combination. Example: Ctrl+Alt+B'),
    # ⚠⚠ **Wayland kann das nicht — und das ist Absicht des Systems.** Ein
    # Programm darf dort nicht mithoeren, was in einem anderen Fenster getippt
    # wird. Statt es zu verschweigen oder so zu tun, als laege es an uns:
    # sagen, was Sache ist, und den fertigen Weg danebenstellen.
    's_hk_wayland':    ('Unter Wayland kann kein Programm eine systemweite '
                        'Tastenkombination selbst belegen — das lässt das '
                        'System aus gutem Grund nicht zu.\n\n'
                        'Der Weg dorthin führt über die Tastenkombinationen '
                        'deines Schreibtischs: Leg eine auf den Startbefehl '
                        'unten. Läuft der Watcher schon, holt ein zweiter '
                        'Start ihn nur nach vorn.',
                          'On Wayland no program can claim a system-wide '
                          'shortcut by itself — the system does not allow it, '
                          'for good reason.\n\n'
                          'The way there is your own desktop shortcut '
                          'settings: put one on the start command below. If the '
                          'watcher is already running, a second start just '
                          'brings it to the front.'),
    's_optional':      ('optional',
                          'optional'),
    's_durchsuchen':   ('Durchsuchen …',
                          'Browse …'),
    's_oeffnen':       ('Öffnen',
                          'Open'),
    's_vorschau_leer': ('Noch keine Datei gewählt',
                        'No file chosen yet'),
    's_vorschau_leer_h': ('Sobald du eine Datei wählst, steht hier, was der Import '
                          'täte: wie viele Baupläne dazukämen, wie viele du schon '
                          'hast und ob welche im Katalog fehlen. Übernommen wird '
                          'erst auf Knopfdruck.',
                          'As soon as you pick a file, this shows what the import '
                          'would do: how many blueprints would be added, how many '
                          'you already have, and whether any are missing from the '
                          'catalogue. Nothing is taken over until you press the '
                          'button.'),
    # -- Seite „Angaben im Spiel" --
    's_sp_lead':       ('Der Watcher schreibt in die Auftragstexte des Spiels, welche Baupläne ein Auftrag ausschüttet — mit Haken für das, was du schon hast. Hier wählst du auch, aus welcher Quelle diese Texte kommen.',
                          'The watcher writes into the game\'s mission text which blueprints a mission hands out — with a tick for the ones you already have. This is also where you pick which source those texts come from.'),
    's_sp_quelle_ist': ('Quelle: %s', 'Source: %s'),
    's_sp_steht':      ('Die Bauplan-Angaben stehen in den Auftragstexten. Änderungen wirken beim nächsten Spielstart — Star Citizen liest die Textdatei nur beim Hochfahren.',
                          'The blueprint details are in the mission text. Changes take effect the next time the game starts — Star Citizen reads the text file only while launching.'),
    's_sp_hole':       ('Hole und setze ein: %s — das dauert einen Moment …',
                          'Fetching and installing: %s — this takes a moment …'),
    's_sp_nichts':     ('Noch keine Bauplan-Angaben in den Auftragstexten.',
                          'No details in the game at the moment.'),
    's_sp_nichts_h':   ('Wähle unten eine Textquelle — der Rest passiert von selbst.',
                          'Pick a text source below — the rest happens on its own.'),
    's_sp_quelle':     ('Textquelle', 'Text source'),
    # ⚠ Der Satz „übersetzt das ganze Spiel" MUSS hier stehen bleiben. Beim
    # Testen gemeldet (Bomb20, 25.08.2026): „übrigens tauscht das tool — wenn
    # auf deutsch gestellt — auch im Spiel alles englische gegen deutsches
    # aus." Das ist so gewollt (der Watcher braucht eine global.ini, in die er
    # schreibt), aber niemand rechnet damit: Wer einen Bauplan-Melder
    # installiert, erwartet keine Spielübersetzung.
    's_sp_quelle_h':   ('Woher die Grundlage kommt, in die geschrieben wird. ⚠ Deutsch und StarStrings ersetzen die Textdatei des Spiels vollständig — danach ist das **ganze Spiel** in dieser Sprache, nicht nur die Bauplan-Angaben. „Original" lässt deine Installation, wie sie ist. Übersetzung und StarStrings sind fremde Projekte und werden beim Klick von deren eigener Adresse geladen, nicht mitgeliefert.',
                          'Where the base text comes from that gets written into. ⚠ German and StarStrings replace the game’s text file completely — after that the **whole game** is in that language, not just the blueprint details. „Original" leaves your installation as it is. The translation and StarStrings are other projects and are fetched from their own address when you click, not shipped along.'),

    # Rückfrage, bevor die Textdatei des Spiels zum ersten Mal ersetzt wird.
    's_sp_warnung_titel': ('Das übersetzt das ganze Spiel',
                          'This translates the whole game'),
    's_sp_warnung':    ('„%s" ersetzt die Textdatei von Star Citizen vollständig. Danach ist das ganze Spiel in dieser Sprache — alle Menüs, alle Missionen, nicht nur die Bauplan-Angaben.\n\nDeine bisherige Textdatei wird vorher gesichert, und „Wieder entfernen" macht es rückgängig.\n\nEinsetzen?',
                          '„%s" replaces Star Citizen’s text file completely. After that the whole game is in that language — every menu, every mission, not just the blueprint details.\n\nYour current text file is backed up first, and „Remove again" undoes it.\n\nInstall it?'),
    # Die Quellen mit Urheber benennen — im Assistenten steht es auch dort, und
    # es sind fremde Projekte, keine eigene Übersetzung.
    's_sp_q_de':       ('Deutsch (rjcncpt)', 'German (rjcncpt)'),
    's_sp_q_ss':       ('StarStrings (MrKraken)', 'StarStrings (MrKraken)'),
    's_sp_q_or':       ('Original (aus dem Spiel)', 'Original (from the game)'),
    's_sp_an':         ('Angaben in die Auftragstexte schreiben',
                          'Write the details into the mission text'),
    's_sp_an_h':       ('Aus lassen, wenn du gerade auf PTU spielst oder die Textdatei in Ruhe lassen willst. Ausschalten nimmt vorhandene Angaben gleich wieder heraus, Einschalten trägt sie neu ein — der Wortlaut des Spiels wird dabei buchstabengenau wiederhergestellt.',
                          'Leave it off while you play on PTU, or when you want the text file left alone. Switching it off removes details that are already there; switching it on writes them again — the game’s original wording is restored exactly.'),
    's_sp_an_sagen':   ('Angaben schreiben: %s', 'Writing details: %s'),
    's_sp_aus_hinweis': ('Ausgeschaltet — es wird nichts geschrieben.',
                          'Switched off — nothing is being written.'),
    's_sp_aus_rest':   ('Ausgeschaltet — es stehen aber noch Angaben im Spiel.',
                          'Switched off — but details are still in the game.'),
    's_sp_aus_rest_h': ('Sie ließen sich nicht herausnehmen. „Wieder entfernen“ unter „Von Hand“ nimmt sie heraus.',
                          'They could not be removed. Use ‚Remove again‘ under ‚By hand‘.'),
    's_sp_auto':       ('Selbst aktuell halten', 'Keep up to date'),
    's_sp_auto_h':     ('Prüft beim Start und alle sechs Stunden. Ohne das sind die Angaben nach jedem Spiel-Patch still verschwunden — jedes Update schreibt die Textdatei neu.',
                          'Checks on start and every six hours. Without it the details are silently gone after every game patch — each update rewrites the text file.'),
    's_sp_auto_sagen': ('Selbst aktuell halten: %s', 'Keep up to date: %s'),
    'hinweis_schloss': ('Durchklicken beenden', 'Stop click-through'),
    'ov_schloss_offen': ('Das Overlay fängt wieder Klicks ab.',
                         'The overlay catches clicks again.'),
    'hinweis_schloss_zu': ('Durchklickbar machen', 'Make click-through'),
    'ov_schloss_zu':   ('Klicks gehen jetzt ins Spiel — das Schloss oben rechts holt das Overlay zurück.',
                        'Clicks now go to the game — the lock at the top right brings the overlay back.'),
    's_sp_angaben':    ('Angaben am Gegenstand', 'Details on the item'),
    's_sp_angaben_h':  ('Schreibt Klasse, Größe und Gütegrad hinter den Namen — bei Raketen stattdessen den Suchkopf (IR, EM, CS). Damit steht am Traktorstrahl „Glacier (Mil/1/A)" statt nur „Glacier", ohne dass man die Beschreibung aufklappen muss. Die Angaben stammen aus der Textdatei des Spiels selbst.',
                          'Adds class, size and grade after the name — for missiles the seeker type instead (IR, EM, CS). The tractor beam then shows „Glacier (Mil/1/A)" rather than just „Glacier", with no need to expand the description. The details come from the game\'s own text file.'),
    's_sp_angaben_sagen': ('Angaben am Gegenstand: %s', 'Details on the item: %s'),
    's_sp_hand':       ('Von Hand', 'By hand'),
    's_sp_hand_h':     ('Alles Eingefügte steht zwischen Marken und lässt sich auf den Buchstaben genau wieder entfernen.',
                          'Everything inserted sits between markers and can be removed again to the letter.'),
    's_sp_jetzt':      ('Jetzt auffrischen', 'Refresh now'),
    's_sp_pruefen':    ('Übersetzung prüfen', 'Check translation'),
    's_sp_weg':        ('Wieder entfernen', 'Remove again'),
    's_sp_warn':       ('Jedes Übersetzungs-Update und jeder Spiel-Patch löscht die Angaben.',
                          'Every translation update and every game patch wipes the details.'),
    's_sp_warn_h':     ('Beide schreiben die Textdatei neu. Deshalb gibt es „Jetzt auffrischen" und die Prüfung — ohne das denkt man, es funktioniere, und es ist längst weg.',
                          'Both rewrite the text file. That is why there is "Refresh now" and the check — without them you believe it works while it has long been gone.'),

    # -- Seite „Bestand" (ausgeben und einlesen) --
    's_be_lead':       ('Deinen Bauplan-Stand ausgeben — oder einen vorhandenen einlesen.',
                          'Export your blueprint inventory — or import an existing one.'),
    's_be_aus':        ('Bestand ausgeben', 'Export inventory'),
    's_be_aus_h':      ('Zum Hochladen oder als eigene Sicherung. Hochgeladen wird nichts — das machst du selbst.',
                          'For uploading or as your own backup. Nothing is uploaded — you do that yourself.'),
    's_be_n_bp':       ('%s Baupläne', '%s blueprints'),
    's_be_voll':       ('Vollständige Sicherung', 'Full backup'),
    's_be_voll_h':     ('mit Art, Klasse, Größe, Gütegrad',
                          'with type, class, size and grade'),
    's_be_alle_drei':  ('Alle drei in die Ablage', 'All three to the folder'),
    's_be_einzeln':    ('Einzeln speichern …', 'Save individually …'),
    # ⚠ Ein Knopf je Version, direkt an der Version. Vorher gab es nur
    # „Einzeln speichern …", und das schrieb **immer** die Basetool-Version —
    # scmdb und die Vollsicherung waren über den Dialog gar nicht erreichbar.
    # Aufgefallen, als der Autor das Werkzeug jemandem vorführte und selbst
    # suchen musste (27.08.2026).
    's_be_speichern_kurz': ('Speichern …', 'Save …'),
    's_be_fort':       ('Wird bei jedem neuen Bauplan mitgeschrieben.',
                        'Kept up to date with every new blueprint.'),
    's_be_ablage':     ('Ablage öffnen', 'Open folder'),
    's_be_geschrieben': ('%s Dateien in die Ablage geschrieben',
                          '%s files written to the folder'),
    's_be_schiefging': ('Ausgeben hat nicht geklappt', 'Export did not work'),
    's_be_speichern':  ('Bestand speichern', 'Save inventory'),
    's_be_gespeichert': ('Gespeichert: %s', 'Saved: %s'),
    's_be_ein':        ('Bestand einlesen', 'Import inventory'),
    's_be_ein_h':      ('Du hast deinen Stand schon woanders — im Basetool, bei scmdb, im SC Deutsch Launcher oder als Sicherung? Datei wählen, der Rest geht von selbst.',
                          'Already have your inventory elsewhere — in the Basetool, at scmdb, in the SC Deutsch Launcher or as a backup? Pick the file, the rest happens on its own.'),
    's_be_waehlen':    ('Datei wählen …', 'Choose file …'),
    's_be_erkannt':    ('Erkannt werden: eigene Sicherung · KRT Profit Basetool · scmdb.net · sc_bp_erledigt.json des Launchers. Welches Format vorliegt, findet das Werkzeug selbst heraus.',
                          'Recognised: your own backup · KRT Profit Basetool · scmdb.net · the launcher’s sc_bp_erledigt.json. Which format it is, the tool works out by itself.'),
    's_be_unbekannt':  ('Diese Datei kenne ich nicht.',
                          'I do not recognise this file.'),
    's_be_unbekannt_h': ('Erwartet werden: eigene Sicherung, KRT Profit Basetool, scmdb.net oder sc_bp_erledigt.json des Launchers.',
                          'Expected: your own backup, KRT Profit Basetool, scmdb.net or the launcher’s sc_bp_erledigt.json.'),
    's_be_vorschau':   ('Vorschau — nichts ist bisher übernommen',
                          'Preview — nothing has been taken over yet'),
    's_be_eigen':      ('Eigene Sicherung', 'Your own backup'),
    's_be_dazu':       ('kommen dazu', 'will be added'),
    's_be_schon':      ('hast du schon', 'you already have'),
    's_be_nicht_kat':  ('nicht im Katalog', 'not in the catalogue'),
    's_be_nicht_kat_h': ('Nicht im Katalog — kommen trotzdem mit:  ',
                          'Not in the catalogue — coming along anyway:  '),
    's_be_merge':      ('Vorhandenes bleibt unangetastet — es wird zusammengeführt, nie ersetzt.',
                          'What you already have stays untouched — it is merged, never replaced.'),
    's_be_nimm':       ('%d Baupläne übernehmen', 'Take over %d blueprints'),
    's_be_genommen':   ('%d Baupläne übernommen', '%d blueprints taken over'),

    # -- Seite „Erkennung" --
    's_er_lead':       ('Wie der Watcher merkt, dass ein Bauplan hereingekommen ist. Die Standardwerte passen für fast jeden — hier nur ändern, wenn etwas klemmt.',
                          'How the watcher notices that a blueprint has arrived. The defaults suit almost everyone — only change things here if something is stuck.'),
    's_er_takt':       ('Wie oft nachsehen', 'How often to look'),
    's_er_takt_h':     ('Sekunden zwischen zwei Blicken in die Protokolldatei. Kleiner heißt schneller und kostet etwas mehr Rechenzeit.',
                          'Seconds between two looks at the log file. Smaller means faster and costs a little more processing time.'),
    's_er_sek':        (' Sek.', ' sec.'),
    's_er_takt_sagen': ('Takt: %s Sekunden', 'Interval: %s seconds'),
    's_er_satz':       ('Erkannte Meldung', 'Detected message'),
    's_er_satz_h':     ('Der Satz, den das Spiel schreibt. Der Watcher leitet ihn selbst aus deinen Protokollen ab — hier steht, was gefunden wurde.',
                          'The sentence the game writes. The watcher derives it from your logs by itself — this is what it found.'),
    's_er_kat':        ('Katalog auffrischen', 'Refresh catalogue'),
    's_er_kat_h':      ('Welche Baupläne es gibt und woher sie kommen. Wird beim Start geholt, wenn eine neue Spielversion erschienen ist.',
                          'Which blueprints exist and where they come from. Fetched on start whenever a new game version has appeared.'),
    's_er_kat_holt':   ('Katalog wird geholt …', 'Fetching catalogue …'),
    's_er_kat_da':     ('Katalog aufgefrischt: %s Baupläne',
                          'Catalogue refreshed: %s blueprints'),
    's_er_kat_weg':    ('Katalog holen ging nicht', 'Could not fetch catalogue'),
    's_er_kat_jetzt':  ('Jetzt neu holen', 'Fetch again now'),

    # -- Seite „Diagnose" --
    's_di_lead':       ('Wenn etwas klemmt: Dieser Block sagt in einem Rutsch, woran es liegen könnte. Der rote Knopf schickt ihn dem Entwickler — mehr musst du nicht tun.',
                          'When something is stuck: this block says in one go what it might be. The red button sends it to the developer — that is all you need to do.'),
    # ⚠ „Auf GitHub" gehört in den Namen. Vorher hieß der Knopf „Fehler
    # melden …" und stand neben „Fehlerbericht absenden" — zwei Namen, die
    # dasselbe versprechen, während der eine den Browser aufmacht und ein
    # GitHub-Konto verlangt. Gemeldet am 28.08.2026: „woher weiß ein User, was
    # Fehler melden macht?"
    's_di_melden':     ('GitHub Issue …', 'GitHub issue …'),
    's_di_absenden':   ('Fehlerbericht absenden', 'Send error report'),
    's_di_ab_frage_t': ('Fehlerbericht absenden?', 'Send error report?'),
    's_di_ab_frage':   ('Der Bericht oben geht als Datei an den Entwickler — genau der Text, den du siehst, nichts weiter.\n\nEr enthält keine Namen, keine Pfade und keine Zugangsdaten; die sind bereits herausgenommen.\n\nAbsenden?',
                        'The report above goes to the developer as a file — exactly the text you see, nothing else.\n\nIt contains no names, no paths and no credentials; those have already been removed.\n\nSend it?'),
    's_di_ab_laeuft':  ('Wird gesendet …', 'Sending …'),
    's_di_ab_ok':      ('Bericht ist angekommen. Danke!', 'Report received. Thank you!'),
    's_di_ab_weg':     ('Senden ging nicht: %s', 'Sending did not work: %s'),
    'm_bericht_kein_ziel': ('In dieser Fassung ist kein Ziel eingebaut.',
                            'No destination is built into this version.'),
    'm_bericht_weg':   ('keine Verbindung', 'no connection'),
    's_di_kopieren':   ('Angaben kopieren', 'Copy details'),
    's_di_speichern':  ('Als Datei speichern …', 'Save as a file …'),
    's_di_ordner':     ('Eigenen Ordner öffnen', 'Open own folder'),
    's_di_browser_ok': ('Formular im Browser geöffnet', 'Form opened in the browser'),
    's_di_browser_weg': ('Browser ließ sich nicht öffnen', 'The browser would not open'),
    's_di_kopiert':    ('Angaben kopiert', 'Details copied'),
    's_di_gespeichert': ('Gespeichert: %s', 'Saved: %s'),
    's_di_speich_weg': ('Speichern ging nicht', 'Saving did not work'),
    's_di_sicher':     ('Du siehst vorher genau, was du verschickst.',
                          'You see exactly what you send before you send it.'),
    's_di_sicher_h':   ('Der Block oben ist der ganze Inhalt — nichts wird im Hintergrund übertragen, und Pfade sind gekürzt, damit kein Benutzername in einem öffentlichen Issue landet.',
                          'The block above is the entire content — nothing is transmitted in the background, and paths are shortened so no user name ends up in a public issue.'),
    's_di_mit':        ('Fehler mitschreiben', 'Record errors'),
    's_di_mit_h':      ('Hält die letzten 50 unerwarteten Fehler mit Zeitpunkt und Stelle fest. Kostet nichts und ist der Unterschied zwischen „geht nicht" und einer Behebung.',
                          'Keeps the last 50 unexpected errors with time and place. Costs nothing and is the difference between "it does not work" and a fix.'),
    's_be_reset':      ('Bestand zurücksetzen', 'Reset inventory'),
    's_be_reset_h':    ('Baut den Bauplan-Bestand aus den vorhandenen Spielprotokollen neu auf.',
                          'Rebuilds the blueprint inventory from the game logs you still have.'),
    's_be_reset_frage': ('Dein Bauplan-Stand wird gelöscht und aus den vorhandenen Protokollen neu aufgebaut.\n\nWas älter ist als deine Protokolle, kommt nicht zurück. Fortfahren?',
                          'Your blueprint inventory will be deleted and rebuilt from the logs you still have.\n\nAnything older than your logs will not come back. Continue?'),
    's_be_reset_ok':   ('Bestand zurückgesetzt — beim nächsten Start neu gelesen',
                          'Inventory reset — read afresh on the next start'),
    # ⚠⚠ **Auch das Misslingen muss ANKOMMEN.** Bis 3.5.0 wurde ein
    # Fehlschlag nur in die Diagnose geschrieben — der Nutzer druckte den roten
    # Knopf, bestaetigte die Warnung und bekam danach: nichts. Kein Haken, kein
    # Fehler. Am 31.08.2026 aus einem Nutzerbericht (Linux, CachyOS).
    's_be_reset_fehler': ('Zurücksetzen ging nicht: %s',
                          'Reset did not work: %s'),
    's_be_reset_warn': ('Zurücksetzen löscht deinen Bauplan-Stand.',
                          'Resetting deletes your blueprint inventory.'),
    's_be_reset_warn_h': ('Der Watcher liest ihn danach aus den noch vorhandenen Protokollen neu auf — was älter ist, ist weg. Vorher oben unter „Bestand ausgeben" sichern.',
                          'The watcher then rebuilds it from the logs that remain — anything older is gone. Save it above under "Export inventory" first.'),

    # -- Seiten „Fortschritt", „Allgemein", „Anzeige", „Ordner" (Reste) --
    's_fo_lead':       ('Zuerst der Stand je Bereich — klick einen an, um die Kategorien darin zu sehen.',
                          'The state of each area first — click one to see the categories inside.'),
    's_fo_von':        ('  von %d Bauplänen · %.0f %%',
                          '  of %d blueprints · %.0f %%'),
    's_al_autostart':  ('Autostart: %s', 'Autostart: %s'),
    's_an_vorne':      ('Immer im Vordergrund: %s', 'Always on top: %s'),
    's_an_zeilen':     ('Zeilen im Overlay: %s', 'Rows in the overlay: %s'),
    's_an_lage_weg':   ('Fensterlage zurückgesetzt — mittig auf dem Hauptbildschirm',
                          'Window position reset — centred on the main screen'),
    's_or_mitlesen':   ('Die Game.log wird mitgelesen: %s',
                          'The Game.log is being read along: %s'),
    's_or_geoeffnet':  ('Ordner geöffnet', 'Folder opened'),
    's_or_nicht_auf':  ('Der Ordner ließ sich nicht öffnen — Näheres steht in der Diagnose.',
                          'The folder could not be opened — see Diagnostics for details.'),
    # ⚠ Das Feld gab es als Einstellung `spielstarter` schon lange — aber
    # nirgends in der Oberfläche, nur von Hand in der `einstellungen.json`. Für
    # jemanden, der spielen und nicht schrauben will, heißt das: gibt es nicht.
    # Gemeldet am 27.08.2026: „einige kennen sich nicht aus und wollen nur was
    # funktionierendes."
    's_or_start':      ('Startbefehl für Star Citizen  —  optional',
                        'Launch command for Star Citizen  —  optional'),
    's_or_start_h':    ('Leer lassen, wenn der Knopf „Star Citizen starten" bei dir '
                        'funktioniert. Er findet das Startskript des LUG Helper von '
                        'allein. Wer über Lutris, Heroic oder Flatpak spielt, trägt '
                        'hier seinen eigenen Befehl ein — dann erscheint der Knopf '
                        'auch bei ihm.',
                        'Leave empty if the "Launch Star Citizen" button works for '
                        'you. It finds the LUG Helper launch script by itself. If you '
                        'play through Lutris, Heroic or Flatpak, enter your own '
                        'command here — then the button appears for you too.'),
    's_or_start_bsp':  ('Beispiele:  lutris rungame/star-citizen  ·  '
                        'flatpak run org.starcitizen-lug.Helper  ·  '
                        'oder der volle Pfad zu einem Startskript',
                        'Examples:  lutris rungame/star-citizen  ·  '
                        'flatpak run org.starcitizen-lug.Helper  ·  '
                        'or the full path to a launch script'),
    's_or_start_ok':   ('Startbefehl übernommen — der Knopf gilt ab sofort.',
                        'Launch command saved — the button applies from now on.'),
    's_or_start_weg':  ('Startbefehl entfernt — es gilt wieder der gefundene Weg.',
                        'Launch command removed — the detected route applies again.'),
    's_or_uebernehmen': ('Übernehmen', 'Apply'),
    's_or_leer':       ('leer — wird selbst gesucht',
                          'empty — found automatically'),

    # -- Seite „Was ist neu" --
    # ⚠ Diese vier standen bis v3.0.0-rc58 **fest im Code** (`seiten.py`) und
    # blieben deshalb auch auf Englisch deutsch — sichtbar auf dem Reiter
    # „Was ist neu", direkt neben einem sauber übersetzten Changelog.
    's_wn_f_alle':     ('Alles', 'All'),
    's_wn_f_neu':      ('Neu', 'New'),
    's_wn_f_bess':     ('Verbessert', 'Improved'),
    's_wn_f_fix':      ('Behoben', 'Fixed'),
    's_wn_lead':       ('Neu ist dazugekommen · Verbessert kann jetzt mehr · Behoben hat vorher geklemmt.',
                          'New was added · Improved can do more now · Fixed used to be broken.'),
    's_wn_nichts':     ('Nichts in dieser Auswahl.', 'Nothing in this selection.'),
    's_wn_aenderungen': ('  %d Änderungen', '  %d changes'),

    # -- Seite „Über" --
    # --- Danke & Lizenzen -------------------------------------------------
    # ⚠ Diese Seite gibt es seit v3.0.0-rc58. Vorher stand im ganzen Programm
    # **keine** Lizenzangabe — weder die eigene (GPL) noch die der Symbole. Und
    # fremde Projekte wurden nur nebenbei genannt, dort wo sie gerade gebraucht
    # wurden (StarStrings auf der Auftragstexte-Seite). Wer wissen wollte, wem
    # was gehört, fand es nur in der README auf GitHub.
    's_dk_lead':       ('Was hier drinsteckt, stammt nicht nur von mir. Diese Seite '
                        'sagt, wem was gehört — und bedankt sich bei denen, ohne '
                        'die es das Werkzeug nicht gäbe.',
                        'Not everything in here is mine. This page says what belongs '
                        'to whom — and thanks the people without whom this tool '
                        'would not exist.'),
    's_dk_selbst':     ('Dieses Programm', 'This program'),
    's_dk_selbst_h':   ('Frei benutzbar, veränderbar und weitergebbar. Wer es '
                        'weitergibt — verändert oder nicht —, muss den Quellcode '
                        'unter derselben Lizenz mitliefern. Es gibt keine Garantie.',
                        'Free to use, change and pass on. Anyone passing it on — '
                        'changed or not — must include the source code under the '
                        'same licence. There is no warranty.'),
    's_dk_dabei':      ('Mitgeliefert', 'Bundled'),
    's_dk_dabei_h':    ('Steckt in der Programmdatei und läuft ohne Internet.',
                        'Part of the program file, works without an internet '
                        'connection.'),
    's_dk_symbole':    ('Alle Symbole der Oberfläche. Ein Satz, von denselben '
                        'Leuten gezeichnet — deshalb sehen sie überall gleich aus.',
                        'Every symbol in the interface. One set, drawn by the same '
                        'people — which is why they look the same everywhere.'),
    's_dk_extern':     ('Wird geladen, nicht mitgeliefert',
                        'Fetched, not bundled'),
    's_dk_extern_h':   ('Fremde Projekte mit eigenen Lizenzen. Sie werden bei '
                        'Bedarf von ihrer eigenen Adresse geholt — eine '
                        'mitgelieferte Kopie wäre eine Weitergabe und damit nicht '
                        'erlaubt.',
                        'Separate projects with their own licences. They are '
                        'fetched from their own addresses when needed — bundling a '
                        'copy would count as redistribution and is not allowed.'),
    's_dk_scmdb':      ('Art, Größe, Gütegrad, Klasse und Herkunft je Bauplan. Ein '
                        'Hobbyprojekt, das die Spieldaten aufbereitet und frei '
                        'zugänglich macht. Abgerufen wird sparsam: nur bei einer '
                        'neuen Spielversion.',
                        'Type, size, grade, class and source for each blueprint. A '
                        'hobby project that prepares the game data and makes it '
                        'freely available. Fetched sparingly: only when a new game '
                        'version appears.'),
    # ⚠⚠ **Wer eine Quelle benutzt, nennt sie.** Die Rohstoffpreise kamen
    # ab v3.3.0-rc39 von UEX Corp, standen aber nirgends auf dieser Seite.
    # Am 30.08.2026 gemeldet: „UEX Corp liefert uns nun auch Daten. Sieht
    # aber niemand — nix sagen ist wie klauen." Genau so ist es.
    's_dk_uex':        ('Kauf- und Verkaufspreise der Rohstoffe. Damit steht '
                        'neben jeder fehlenden Zutat, was sie kostet — oder '
                        'dass sie sich gar nicht kaufen lässt. Ein von '
                        'Spielern gepflegtes Datenprojekt. Abgerufen wird '
                        'höchstens einmal am Tag.',
                        'Buy and sell prices for resources. That is what puts '
                        'a price next to every missing ingredient — or says it '
                        'cannot be bought at all. A data project maintained by '
                        'players. Fetched at most once a day.'),
    's_dk_ss':         ('Aufgeräumte englische Spieltexte — eine der Grundlagen, '
                        'in die die Bauplan-Angaben geschrieben werden können.',
                        'Cleaned-up English game text — one of the bases the '
                        'blueprint details can be written into.'),
    's_dk_scdl':       ('War anfangs die einzige Datenquelle — ohne ihn gäbe es '
                        'dieses Projekt nicht. Ist er installiert, bestätigt er die '
                        'Funde und liefert die deutschen Bezeichnungen.',
                        'Was the only data source at the start — without it this '
                        'project would not exist. If installed, it confirms finds '
                        'and supplies the German names.'),
    # ⚠⚠ Die deutsche Übersetzung selbst hat einen eigenen Urheber und eine
    # eigene Lizenz. Die verlangt ausdrücklich **Name UND Repository** — bis
    # v3.3.0-rc41 stand hier nur „SC Deutsch Launcher", also der Verteiler,
    # nicht der Autor. Am 30.08.2026 nachgereicht.
    's_dk_ini':        ('Die deutsche Übersetzung des Spiels selbst — die '
                        'Grundlage, in die der Watcher seine Bauplan-Angaben '
                        'schreibt. Es gibt sie auch auf Schweizerdeutsch; beide '
                        'Fassungen erkennt der Watcher.\n\n'
                        'Die Datei wird nur auf deinem Rechner ergänzt und '
                        'nirgendwohin weitergegeben. Die Quellenangabe in ihrer '
                        'ersten Zeile bleibt dabei unangetastet — so verlangt es '
                        'der Autor, und so findet jeder zur ursprünglichen '
                        'Übersetzung zurück.',
                        'The German translation of the game itself — the base the '
                        'watcher writes its blueprint notes into. There is a Swiss '
                        'German edition too; the watcher recognises both.\n\n'
                        'The file is only extended on your own machine and is never '
                        'passed on. The source note in its first line stays '
                        'untouched — the author asks for that, and it is how anyone '
                        'finds their way back to the original translation.'),
    's_dk_freiwillig': ('freiwillig', 'optional'),
    's_dk_keine_lizenz': ('keine Lizenzangabe', 'no licence stated'),
    's_dk_tester':     ('Tester', 'tester'),
    's_dk_leute':      ('Und Danke an', 'And thanks to'),
    's_dk_leute_h':    ('Wer einen Fehler findet oder einen guten Vorschlag macht, '
                        'steht namentlich im Änderungsprotokoll — hier stehen die, '
                        'aus deren Rückmeldung etwas geworden ist, das es sonst '
                        'nicht gäbe.',
                        'Anyone who finds a bug or makes a good suggestion is named '
                        'in the changelog — listed here are those whose feedback '
                        'became something that would not exist otherwise.'),
    's_dk_beitraege':  ('%d Beiträge', '%d contributions'),
    's_dk_aufklappen': ('Klick auf einen Namen zeigt, was daraus geworden ist.',
                        'Click a name to see what came of it.'),
    's_dk_haldjas_idee':     ('**Aufblend-Betrieb und durchgereichte Mausklicks** — damit ein '
                              'Overlay im Kampf hilft statt zu stören.',
                              '**Fade mode and click-through** — so an overlay helps in a '
                              'fight instead of getting in the way.'),
    's_dk_haldjas_bugs':     ('Dazu ein Dutzend Funde rund um Overlay, Einrichtung und '
                              'Update — darunter das eingeklappte Overlay, das in drei '
                              'von vier Ecken über den Bildschirmrand hinausstand, die '
                              'Titelleiste, die in einer unteren Ecke nach unten '
                              'gehört, der träge Fensteraufbau beim Öffnen der '
                              'Einstellungen — und der grüne Streifen, der nach dem '
                              'Start in der alten Ecke zurückblieb. Später dazu die '
                              'nackten Schlüsselnamen im Einrichtungsassistenten und '
                              'der umbenannte Spielordner, dem HOTFIX seine Erkennung '
                              'verdankt. Und die Bauplan-Liste, die beim Öffnen hakte — '
                              'sie geht jetzt in einem Drittel der Zeit auf. Sein '
                              'Bericht brachte zuletzt zwei Dinge auf einmal: den '
                              'trägen Programmstart und die Baupläne, die mit '
                              'StarStrings nicht mehr zum Katalog fanden.',
                              'Plus a dozen finds around the overlay, setup and updating — '
                              'among them the collapsed overlay hanging off the screen '
                              'edge in three corners out of four, the title bar that '
                              'belongs at the bottom when the overlay sits in a bottom '
                              'corner, the sluggish window build when opening '
                              'the settings — and the green strip left behind in the old '
                              'corner after a restart. Later also the raw key names in '
                              'the setup wizard, and the renamed game folder that led to '
                              'HOTFIX being recognised at all. And the blueprint list '
                              'that stuttered when opening — it now comes up in a third '
                              'of the time. His latest report brought two things at '
                              'once: the sluggish program start, and the blueprints '
                              'that no longer matched the catalogue with StarStrings.'),
    # ⚠ **Diese Seite mitziehen, nicht nur den CHANGELOG.** Am 27.08.2026 hat
    # Bomb20 an einem Vormittag vier Fehler gefunden, die alle am Samstag jeden
    # Nutzer getroffen hätten — und hier stand weiter nur sein Fund vom 25.08.
    # Der Dank im CHANGELOG ist das eine; diese Seite ist das, was die Leute im
    # Programm sehen. Wer einen Melder hier vergisst, hat ihm nicht gedankt.
    's_dk_bomb_idee':        ('**Updates kamen unter Linux nicht an** — er ist drangeblieben,'
                              'als es längst nach Bedienfehler aussah, bis der Grund gefunden'
                              'war.',
                              '**Updates never arrived on Linux** — he kept at it long after'
                              'it looked like user error, until the cause was found.'),
    # ⚠ Der vierte Fund ist kein behobener Fehler, und genau so steht er da.
    # Wer „behoben" schreibt, wo nur „sichtbar gemacht" stimmt, belügt den
    # nächsten Melder.
    's_dk_bomb_bugs':        ('Dazu der Absturz beim allerersten Start, harte Abbrüche, die'
                              'im Bericht gar nicht auftauchten — und ein Monat Discord Nitro'
                              'für den Server.',
                              'Plus the crash on the very first start, hard aborts that never'
                              'showed up in the report — and a month of Discord Nitro for the'
                              'server.'),
    # ⚠ Ein Geschenk, kein Fund — und trotzdem hierhin. Wer ein kostenloses
    # Werkzeug testet UND dem Autor etwas schenkt, gehört genannt.
    's_dk_yoshimitsu_idee':  ('**Handelsrouten** — sag, wo du stehst und was in '
                              'den Laderaum passt, und das Werkzeug rechnet, '
                              'womit sich die nächste Fahrt lohnt.',
                              '**Trade routes** — say where you are and what '
                              'fits in your hold, and the tool works out what '
                              'the next run is worth.'),
    's_dk_bushwick_idee':    ('**Von der Herstellung direkt zum Bauplan** — und der '
                              'Knopf dorthin heißt jetzt „Woher?", statt nur '
                              'ein Symbol zu sein.',
                              '**From crafting straight to the blueprint** — and '
                              'the button there now reads "Where from?" instead '
                              'of being just a symbol.'),
    's_dk_horthy_idee':      ('**Das eigene Rohstoff-Lager** — eintragen statt rechnen, und'
                              'beim Herstellen zieht das Werkzeug die Zutaten ab.',
                              '**Your own resource stock** — enter it instead of doing the'
                              'maths; crafting deducts the ingredients for you.'),
    's_dk_morkhan_idee':     ('**Angaben am Gegenstand im Spiel**, **Star Citizen aus '
                              'dem Werkzeug starten**, seit v3.4.0 der '
                              '**Verkaufs-Reiter** — und dass auf der '
                              'Joystick-Seite auch die **Tastatur** steht.',
                              '**Item details in game**, **launching Star Citizen from '
                              'the tool**, since v3.4.0 the **selling tab** — and '
                              'having the **keyboard** on the joystick page too.'),
    's_dk_morkhan_bugs':     ('Dazu über zwanzig Funde, darunter 797 Baupläne, die '
                              'niemand zu sehen bekam, ein Fenster, das nicht mehr auf '
                              'den Bildschirm passte, und der abgebrochene Auftrag, der '
                              'nach jedem Start wieder dastand.',
                              'Plus more than twenty finds, among them 797 blueprints '
                              'nobody ever got to see, a window that no longer fit the '
                              'screen, and the withdrawn contract that came back after '
                              'every start.'),
    's_dk_marken':     ('SC BP Watcher ist ein eigenständiges, inoffizielles '
                        'Zusatzwerkzeug und steht in keiner offiziellen Verbindung '
                        'zum SC Deutsch Launcher oder zu Cloud Imperium Games. Alle '
                        'Marken- und Projektnamen gehören ihren jeweiligen '
                        'Eigentümern.',
                        'SC BP Watcher is an independent, unofficial companion tool '
                        'with no official connection to the SC Deutsch Launcher or '
                        'Cloud Imperium Games. All trademarks and project names '
                        'belong to their respective owners.'),

    # ⚠ Der Fankit-Hinweis gehoert ins Programm, nicht nur in die README.
    # Wer das Werkzeug benutzt, liest die README meist nie. Der Wortlaut folgt
    # dem Fankit Agreement und dem UGC-Abschnitt der RSI-Nutzungsbedingungen.
    's_dk_fankit':     ('Dieses Werkzeug ist ein inoffizielles, '
                        'nicht-kommerzielles Fan-Projekt für die '
                        'Star-Citizen-Gemeinschaft. Es steht in keiner Verbindung '
                        'zu Cloud Imperium Rights LLC, Cloud Imperium Rights '
                        # ⚠ Den Herstellernamen NIE ueber einen Zeilenumbruch
                        # trennen. Die Klarnamen-Pruefung (52r) laesst ihn nur
                        # als Ganzes durch; halbiert bleibt ein Vorname stehen
                        # und sie schlaegt Alarm. Am 30.08.2026 passiert.
                        'Ltd. oder Roberts Space Industries und wird von ihnen '
                        'weder unterstützt noch gebilligt.\n\n'
                        'Es verwendet Material aus dem offiziellen Star Citizen '
                        'Fankit. Dieses Material ist für die Verwendung durch Fans '
                        'veröffentlicht und darf nur nach den Bedingungen des '
                        'Fankit Agreement, des Fan Style Guide und der '
                        'RSI-Nutzungsbedingungen verwendet werden — dort besonders '
                        'der Abschnitt über nutzergenerierte Inhalte (UGC).\n\n'
                        'Star Citizen®, Roberts Space Industries® und Cloud '
                        'Imperium® sind eingetragene Marken der Cloud Imperium '
                        'Rights LLC. Alle übrigen Star-Citizen-Inhalte, Grafiken, '
                        'Namen, Logos und Marken gehören ihren jeweiligen '
                        'Eigentümern. © 2025 Cloud Imperium Rights LLC und '
                        'Cloud Imperium Rights Ltd.',

                        'This tool is an unofficial, non-commercial fan project for '
                        'the Star Citizen community. It is not affiliated with, '
                        'endorsed, sponsored, or approved by Cloud Imperium '
                        'Rights LLC, Cloud Imperium Rights Ltd., or '
                        'Roberts Space Industries.\n\n'
                        'It makes use of assets from the official Star Citizen '
                        'Fankit. Those materials are published for fan use and may '
                        'only be used as explained by the terms of the Fankit '
                        'Agreement, the Fan Style Guide, and the '
                        'Roberts Space Industries Terms of Service — '
                        'specifically the section on User Generated Content '
                        '(UGC).\n\n'
                        'Star Citizen®, Roberts Space Industries® and Cloud '
                        'Imperium® are registered trademarks of Cloud Imperium '
                        'Rights LLC. All other Star Citizen content, artwork, '
                        'names, logos and trademarks are the property of their '
                        'respective owners. © 2025 Cloud Imperium Rights LLC and '
                        'Cloud Imperium Rights Ltd.'),
    's_dk_fankit_kopf': ('Star Citizen Fan Content', 'Star Citizen Fan Content'),

    's_ub_lead':       ('Welche Version läuft, wer sie gebaut hat — und ob du Neues vor allen anderen bekommen willst.',
                          'Which version is running, who built it — and whether you want new things before everyone else.'),
    # ⚠ „Jetzt nachsehen" sagte nicht, wonach. Und „Aktualisieren" waere
    # falsch: Der Knopf **prueft** nur, er holt nichts. Der
    # SC-Deutsch-Launcher loest dasselbe mit „SCDL auf Aktualitaet
    # pruefen" — Vorbild uebernommen (gemeldet, 26.08.2026).
    'hf_kofi':         ('Kaffee spendieren', 'Buy me a coffee'),
    'hf_kofi_auf':     ('Ko-fi wird im Browser geöffnet …',
                        'Opening Ko-fi in your browser …'),
    'hf_discord':      ('Discord', 'Discord'),
    'hf_discord_auf':  ('Discord wird im Browser geöffnet …',
                        'Opening Discord in your browser …'),
    's_ub_hinweis_titel': ('Neue Version einspielen',
                          'Install the new version'),
    's_ub_hinweis_neustart': (
        'Die neue Version wird jetzt eingespielt.\n\n'
        'Der Watcher schließt sich dabei und startet nicht von '
        'selbst wieder — bitte starte ihn danach über das Startmenü '
        'oder die Verknüpfung neu.\n\n'
        'Dein Bauplan-Bestand bleibt unangetastet.',
        'The new version is being installed now.\n\n'
        'The watcher will close and will not start again by '
        'itself — please launch it afterwards from the start menu or '
        'your shortcut.\n\n'
        'Your blueprint collection stays untouched.'),
    's_ub_nachsehen':  ('Auf Aktualität prüfen', 'Check for updates'),
    's_ub_aktuell':    ('Du hast die neueste Version.', 'You have the latest version.'),
    's_ub_gefunden':   ('Neue Version gefunden: %s', 'New version found: %s'),
    # ⚠ Der Unterschied zwischen „nichts Neues" und „konnte nicht nachsehen".
    # Bis rc68 meldete der Knopf in beiden Fällen Entwarnung — siehe
    # `aktualisierung.abruf_geglueckt`.
    's_ub_grenze':     ('GitHub lässt nur 60 Abfragen pro Stunde zu, und die '
                        'sind für den Moment aufgebraucht. In einer Stunde geht '
                        'es wieder — der Knopf zum Holen funktioniert weiter.',
                        'GitHub allows only 60 requests per hour, and those are '
                        'used up for now. It will work again in an hour — the '
                        'fetch button still works.'),
    's_ub_sucht_fehler': ('Nachsehen ging nicht — Näheres steht in der Diagnose.',
                          'Check failed — see Diagnostics for details.'),
    's_ub_sucht':      ('Suche nach einer neuen Version …',
                          'Looking for a new version …'),
    's_ub_einrichtung': ('Einrichtung wiederholen', 'Run setup again'),
    # ⚠ **Drei Schlüssel, die es nie gab.** Wer `t()` mit einem Schlüssel ruft,
    # den diese Tabelle nicht kennt, bekommt den **Schlüsselnamen** zurück — und
    # der steht dann in der Oberfläche. Am 28.08.2026 zeigte der Hinweis an der
    # Rakete wörtlich `s_sp_start`; die anderen beiden wären bei der nächsten
    # fehlgeschlagenen Übersetzung und im Versionsfenster aufgetaucht.
    #
    # Gefunden hat sie kein Mensch, sondern eine Prüfung: Sie sammelt jeden
    # `t()`/`Satz()`-Aufruf mit festem Schlüssel aus dem ganzen Programm und
    # gleicht ihn hier ab (Selbsttest, Abschnitt 49). Von Hand ist das nicht zu
    # halten — es sind über 600 Einträge.
    's_sp_start':      ('Star Citizen starten', 'Launch Star Citizen'),
    'm_keine_fassung': ('Keine Fassung zum Herunterladen gefunden.',
                        'No version found to download.'),
    'aktuelle_fassung': ('Du hast die neueste Fassung.',
                         'You have the latest version.'),
    's_ub_taeglich':   ('Nach neuen Versionen sehen',
                          'Check daily for new versions'),
    's_ub_taeglich_h': ('Einmal pro Stunde, ausschließlich bei GitHub. Ist etwas da, färbt sich die Glocke in der Titelleiste grün.',
                          'Once an hour, only at GitHub. If there is something, the bell in the title bar turns green.'),
    's_up_sofort':     ('Jetzt die neueste Version holen',
                        'Get the latest version now'),
    's_up_sofort_h':   ('Holt sofort, was es gerade gibt — auch eine Testversion. '
                        'An deiner Einstellung darunter ändert das nichts.',
                        'Fetches whatever is available right now — including a test '
                        'build. This does not change your setting below.'),
    's_ub_kanal':         ('Wovon willst du Bescheid bekommen?',
                                   'What should I tell you about?'),
    's_ub_kanal_h':       ('Beim Testen mithelfen oder lieber Ruhe haben — beides ist in Ordnung. Klick auf einen Kasten, um zu wechseln; der Knopf darin holt die Version sofort.',
                                     'Help with testing or rather have some quiet — both are fine. Click a box to switch; the button inside fetches that version right away.'),
    's_ub_wer_h':      ('Und woher die Daten kommen, ohne die es das Werkzeug nicht gäbe.',
                          'And where the data comes from, without which this tool would not exist.'),
    # ⚠ Hieß bis rc68 „Nur fertige Versionen". Das war falsch: Das Werkzeug wird
    # laufend weiterentwickelt, „fertig" klingt nach abgeschlossen. der Autor am
    # 27.08.2026: „nenn es Stable Version, nicht fertige Versionen, weil es ein
    # laufend bearbeitetes Projekt ist."
    's_ub_fertig':     ('Stabile Version  ·  empfohlen',
                        'Stable version  ·  recommended'),
    's_ub_fertig_h':      ('Das Übliche: eine Meldung, wenn eine geprüfte Version erscheint. Samstags, höchstens einmal die Woche.',
                                      'The usual: a notice when a tested version appears. Saturdays, at most once a week.'),
    # ⚠ Gleiches Muster wie `s_ub_fertig` direkt darüber: Name · Zweck, mit
    # denselben doppelten Leerzeichen um den Punkt. „Auch Testversionen" stand
    # als einziger Kasten ohne Zusatz da und las sich wie ein angehängter
    # Nachsatz statt wie eine Wahl. Geändert am 01.09.2026.
    's_ub_test':       ('Testversion  ·  zum Testen',
                        'Test version  ·  for testing'),
    's_ub_test_h':        ('Du siehst Neues zuerst und hilfst beim Prüfen. Läuft ganz normal, ist aber weniger lange erprobt — es kann mal klemmen.',
                                    'You see new things first and help with testing. Runs normally, but has been tried out for less time — it can occasionally hiccup.'),
    # Die Herkunftsangaben im Dank-Block. ⚠ Sie standen als Datentabelle im
    # Code und liefen über Variablen ins Fenster — `tools/texte_pruefen.py`
    # sieht so etwas nicht, weil dort kein fester Text an einem Bausteinargument
    # steht. Gefunden nur durch Hinsehen auf der englischen Seite.
    's_ub_q_katalog':  ('Bauplan-Katalog und Herkunft',
                          'blueprint catalogue and origins'),
    's_ub_q_uebersetzung': ('Übersetzung und Vertragsdaten',
                          'translation and mission data'),
    's_ub_q_vorbild':  ('Vorbild für die Einspielung ins Spiel',
                          'the model for writing into the game'),

    # -- Fehlerbericht (bericht.py) --
    # Der Bericht steht im Fenster und wird von dort in ein öffentliches Issue
    # kopiert. Er MUSS der Oberflächensprache folgen: Die Diagnose-Seite
    # verspricht „Du siehst vorher genau, was du verschickst" — auf Englisch
    # gilt das nur, wenn der Block darüber auch englisch ist.
    'b_kopf':          ('SC BP Watcher %s · Bericht vom %s',
                          'SC BP Watcher %s · report from %s'),
    'b_datum':         ('%d.%m.%Y, %H:%M', '%Y-%m-%d, %H:%M'),
    'b_system':        ('System', 'System'),
    'b_verpackung':    ('Verpackung', 'Packaging'),
    # ⚠ Nur für die Anzeige. Die Kennung selbst („quellcode", „exe",
    # „appimage") bleibt unübersetzt — `aktualisierung.py` vergleicht darauf,
    # und eine übersetzte Kennung würde die Update-Prüfung stillschweigend
    # ins Leere laufen lassen.
    'b_v_quellcode':   ('Quellcode', 'source code'),
    'b_v_exe':         ('exe', 'exe'),
    'b_v_appimage':    ('AppImage', 'AppImage'),
    'b_python':        ('Python / Tk', 'Python / Tk'),
    'b_bildschirm':    ('Bildschirm', 'Screen'),
    'b_skalierung':    ('%d×%d · Skalierung %d %%', '%d×%d · scaling %d %%'),
    'b_spiel':         ('Spiel', 'Game'),
    'b_gamelog':       ('Game.log', 'Game.log'),
    'b_sicherungen':   ('Sicherungen', 'Kept logs'),
    # ⚠⚠ **Die Zeile, die eine Rueckfrage erspart.** Am 31.08.2026 kam ein
    # Bericht mit „462 Protokolle" und „0 Baupläne" — und ohne Absender. Ob
    # die Erkennung bei dem Menschen nichts findet oder ob er schlicht neu im
    # Spiel ist, war daraus NICHT zu erkennen, und nachfragen ging nicht.
    # Jetzt steht beides drin: wie viele Protokolle durchgesehen wurden und
    # wie viele Bauplaene dabei herauskamen. 462 gelesen und 0 gefunden heisst
    # kaputt; 0 gelesen heisst, die Nachlese lief nie.
    'b_logs_gelesen':  ('%s durchgesehen', '%s read'),
    'b_logs_funde':    ('%s Baupläne daraus', '%s blueprints from them'),
    # ⚠ Einzahl-Fassungen. Im Bericht stand „1 Baupläne daraus" und
    # „1 Protokolle" — gemeldet am 02.09.2026 aus einem echten Bericht.
    # Der Bericht ist das, was Nutzer verschicken; ein falscher Plural darin
    # sieht nach Nachlässigkeit aus. `%s` bleibt, damit die Zahl an derselben
    # Stelle steht wie in der Mehrzahl-Fassung.
    'b_logs_funde_1':  ('%s Bauplan daraus', '%s blueprint from them'),
    'b_protokolle_1':  ('%s Protokoll', '%s log'),
    'b_protokolle':    ('%s Protokolle', '%s logs'),
    'b_launcher':      ('Launcher', 'Launcher'),
    # ⚠ Diese Zeile wäre am 27.08.2026 die halbe Diagnose gewesen: Bomb20
    # meldete „Star Citizen startet nicht aus dem Werkzeug", und niemand konnte
    # sehen, was das Werkzeug überhaupt gefunden hatte. Erst nach zwei Stunden
    # kam heraus, dass es den `lug-helper` aufrief — ein Programm, das das Spiel
    # gar nicht starten kann. Hätte hier gestanden „lug-helper (gefunden)",
    # wäre es in einer Minute klar gewesen.
    'b_starter':       ('Spielstarter', 'Game launcher'),
    'b_starter_eigen': ('%s  (selbst eingetragen)', '%s  (set by hand)'),
    'b_starter_kein':  ('keiner gefunden — der Startknopf erscheint nicht',
                        'none found — the launch button does not appear'),
    'b_spielsprache':  ('Spielsprache', 'Game language'),
    'b_bestand':       ('Bestand', 'Inventory'),
    'b_n_bauplaene':   ('%s Baupläne', '%s blueprints'),
    # ⚠ Zwei Zahlen für dieselbe Sache sind schlimmer als eine fehlende. Der
    # Bericht zählt die Datei, die Bauplan-Liste den Katalog — wer im Bericht
    # 315 liest und in der Liste 292 sieht, hält eins von beidem für kaputt
    # (30.08.2026 gemeldet). Jetzt steht die Differenz daneben, mit Grund.
    'b_n_bp_katalog':  ('%s Baupläne · %s davon im Katalog, %s unbekannt',
                        '%s blueprints · %s of them in the catalogue, %s unknown'),
    # ⚠ Die Zahl allein sagt „da stimmt was nicht" und sonst nichts. Erst die
    # Namen sagen, WAS fehlt — und ob es an einer Schreibweise liegt oder an
    # einem Katalog, der ein ganzes Rüstungsset nicht kennt.
    'b_unbekannt':     ('Nicht im Katalog', 'Not in the catalogue'),
    'b_und_weitere':   ('… und %s weitere', '… and %s more'),
    'b_merkliste':     ('Merkliste', 'Watchlist'),
    'b_n_eintraege':   ('%s Einträge', '%s entries'),
    'b_katalog':       ('Katalogstand', 'Catalogue state'),
    'b_historie':      ('Patch-Historie', 'Patch history'),
    'b_ordner':        ('Eigener Ordner', 'Own folder'),
    'b_einstellungen': ('Einstellungen', 'Settings'),
    'b_standard':      ('alle auf Standard', 'all at default'),
    'b_nicht_gefunden': ('nicht gefunden', 'not found'),
    'b_nicht_da':      ('nicht vorhanden', 'not present'),
    # ⚠ Die wichtigste Zeile für den häufigsten Support-Fall: „ich sehe deine
    # Angaben im Spiel nicht mehr". Ursache ist fast immer, dass ein
    # Übersetzungs-Update oder ein Spiel-Patch die `global.ini` neu geschrieben
    # und die Angaben dabei stillschweigend entfernt hat. Ohne diese Zeile war
    # das aus dem Bericht nicht abzulesen, sondern nur zu erraten.
    'b_inj':           ('Angaben im Spiel', 'Notes in game'),
    'b_inj_drin':      ('eingetragen', 'in place'),
    'b_inj_weg':       ('NICHT eingetragen', 'NOT in place'),
    'b_inj_aus':       ('Einspielen ist ausgeschaltet', 'writing them is switched off'),
    'b_inj_auto':      ('Auffrischen automatisch', 'refreshes automatically'),
    'b_inj_hand':      ('Auffrischen von Hand', 'refresh by hand'),
    'b_inj_datei':     ('Textdatei', 'Text file'),
    'b_inj_keine':     ('keine gefunden', 'none found'),
    'b_fehler':        ('Letzte Fehler (%s von %s aufgehoben)',
                          'Recent errors (%s of %s kept)'),
    'b_fehler_mehrfach': ('  (%d× dasselbe, bis %s)',
                          '  (%d× the same, until %s)'),
    'b_fehler_keine':  ('Letzte Fehler        keine aufgezeichnet',
                          'Recent errors       none recorded'),
    'b_fuss':          ('Pfade gekürzt (<heim>, <benutzer>) · keine Namen, keine Zugangsdaten',
                          'Paths shortened (<home>, <user>) · no names, no credentials'),

    # -- Kurzmeldungen aus den Bausteinen (Injektion, Übersetzung, Logs) --
    # Sie kommen als Rückgabewert aus einem Modul und landen über
    # `fenster.sagen()` in der Statuszeile — also sichtbar für den Nutzer.
    'm_keine_scdl':    ('keine SCDL-Bauplan-Daten', 'no SCDL blueprint data'),
    'm_keine_ini':     ('global.ini nicht gefunden', 'global.ini not found'),
    'm_keine_missionen': ('Katalog kennt keine Missionen',
                          'the catalogue knows no missions'),
    'm_kein_p4k':      ('Data.p4k nicht gefunden', 'Data.p4k not found'),
    'm_keine_ini_archiv': ('global.ini im Archiv nicht gefunden',
                          'global.ini not found in the archive'),
    'm_keine_version': ('Version nicht gefunden', 'version not found'),
    # ⚠ 403 ist KEIN Netzfehler. scmdb steht hinter Cloudflare, und dessen
    # Schutz weist Abrufe ohne eigene Kennung ab (die nackte
    # `Python-urllib`-Kennung laeuft auf 403, gemessen 29.08.2026). Ohne
    # eigene Meldung stand dort nur "Netzfehler", und man sucht an der
    # falschen Stelle — dieselbe Falle wie beim Zertifikatsfehler.
    # Rueckmeldungen der Herstellungs-Daten (scbp/herstellung.py).
    'm_h_aktuell':     ('Rezepte sind aktuell (%d Baupläne)',
                        'Recipes are up to date (%d blueprints)'),
    'm_h_geladen':     ('%d Baupläne geladen', '%d blueprints loaded'),
    'm_h_leer':        ('Die Datei enthält keine Baupläne.',
                        'The file contains no blueprints.'),
    'm_h_kein_netz':   ('Netzabrufe sind abgeschaltet (SC_BP_NO_NET).',
                        'Network access is switched off (SC_BP_NO_NET).'),
    'm_abgewiesen':    ('Die Seite hat den Abruf abgewiesen (403). Ihr Schutz '
                        'blockiert gerade Programme — das liegt nicht an dir. '
                        'Der Watcher arbeitet mit dem zuletzt geladenen Stand '
                        'weiter; versuch es später noch einmal.',
                        'The site refused the request (403). Its protection is '
                        'currently blocking programs — this is not your fault. '
                        'The watcher keeps working with the data it already '
                        'has; try again later.'),
    'm_kein_zertifikat': ('Sichere Verbindung fehlgeschlagen — die Zertifikate des Systems wurden nicht gefunden',
                          'Secure connection failed — the system certificates were not found'),
    'm_keine_logs':    ('Keine Log-Sicherungen gefunden — der bisherige Bestand lässt sich nicht nachlesen.',
                          'No kept logs found — the earlier inventory cannot be recovered.'),
    'm_erster_lauf':   ('Erster Lauf: nachgelesen wurde ab %s. Was davor freigeschaltet wurde, muss von Hand abgehakt werden.',
                          'First run: read back from %s. Anything unlocked before that has to be ticked off by hand.'),
    'm_luecke_logs':   ('Zwischen %s und %s hat Star Citizen Logs wegger\u00e4umt \u2014 Baupl\u00e4ne aus dieser Zeit fehlen m\u00f6glicherweise.',
                          'Star Citizen removed logs between %s and %s \u2014 blueprints from that period may be missing.'),
    'm_erster_datum':  ('%d.%m.%Y', '%Y-%m-%d'),
    'm_bericht_gekuerzt': ('\n\n… gekürzt. Der vollständige Bericht liegt unter "Als Datei speichern" und kann angehängt werden.',
                          '\n\n… shortened. The full report is available under "Save as a file" and can be attached.'),

    # -- Zwischenmeldungen beim Holen (Katalog, Spieltexte, Übersetzung) --
    # Sie stehen im Fenster, während etwas dauert. Ein stummes Programm sieht
    # aus wie ein hängendes — ein deutsch sprechendes auf einer englischen
    # Oberfläche aber auch wie ein halbfertiges.
    'z_werte':         ('Werte werden geholt …', 'Fetching values …'),
    'z_herkunft_datei': ('Bauplan-Herkunft wird aus %s gelesen …',
                          'Reading blueprint origins from %s …'),
    'z_herkunft_netz': ('Bauplan-Herkunft wird geholt (etwa 12 MB) …',
                          'Fetching blueprint origins (about 12 MB) …'),
    'z_auswerten':     ('Wird ausgewertet …', 'Evaluating …'),
    'z_startbp':       ('Startbaupläne werden geholt …',
                          'Fetching starting blueprints …'),
    'z_originaltexte': ('Originaltexte werden aus dem Spiel geholt …',
                          'Fetching the original texts from the game …'),
    'z_entpackt':      ('entpackt mit %s', 'unpacked with %s'),
    'z_laedt':         ('%s wird geladen (%.1f MB) …',
                          'Loading %s (%.1f MB) …'),
    'z_einsetzen':     ('wird eingesetzt …', 'installing …'),

    # -- Kennzahlen auf der Über-Seite --
    's_ub_version':    ('Version', 'Version'),
    's_ub_bekannt':    ('Baupläne bekannt', 'Blueprints known'),
    's_ub_davon':      ('Davon deine', 'Of those yours'),

    # -- Einrichtung ohne Spielordner --
    # Ohne diesen Ausweg sitzt fest, wer Star Citizen (noch) nicht auf diesem
    # Rechner hat: Der Weiter-Knopf blieb grau, und der Assistent kam bei jedem
    # Start wieder. Das Werkzeug kann auch ohne Spiel etwas — Liste ansehen,
    # Bestand einlesen, Merkliste pflegen.
    'ohne_spiel':      ('Erst mal ohne — ich richte das später ein',
                          'Continue without it — I will set this up later'),
    'ohne_spiel_titel': ('Ohne Spielordner eingerichtet',
                          'Set up without a game folder'),
    'ohne_spiel_text': ('Der Watcher kann jetzt nicht mitlesen, wenn ein Bauplan hereinkommt — dafür braucht er die Game.log. Alles andere geht: die Bauplan-Liste durchsehen, einen vorhandenen Bestand einlesen und die Merkliste pflegen.',
                          'The watcher cannot follow along when a blueprint arrives — that needs the Game.log. Everything else works: browsing the blueprint list, importing an existing inventory and keeping the watchlist.'),
    'ohne_spiel_wo':   ('Nachtragen kannst du den Ordner jederzeit unter Einstellungen → Ordner.',
                          'You can add the folder any time under Settings → Folders.'),

    # -- Herkunftsblock an fester Stelle unter der Liste --
    # Vorher hing er an jeder Zeile und klappte dort auf. Ein Bauplan hat bis
    # zu zwölf Bezugsquellen; der Block wurde über 700 Pixel hoch, während nur
    # 465 sichtbar sind — er schob die Liste komplett weg. Jetzt steht er fest
    # unten, zeigt den einfachsten Weg, und der Rest kommt auf Klick.
    'hk_ein_weg':      ('1 Weg', '1 way'),
    'hk_wege':         ('%d Wege', '%d ways'),
    'hk_leichtester':  ('leichtester Weg zuerst', 'easiest way first'),
    'hk_hast_du':      ('hast du', 'you have it'),
    'hk_fehlt_dir':    ('fehlt dir', 'you are missing it'),
    'hk_auftrag':      ('Auftrag', 'Mission'),
    'hk_fraktion':     ('Fraktion', 'Faction'),
    'hk_annahme':      ('Annahme', 'Pick up at'),
    'hk_rang':         ('Rang', 'Rank'),
    'hk_belohnung':    ('Belohnung', 'Reward'),
    'hk_weitere':      ('%d weitere Wege zu diesem Bauplan',
                          '%d more ways to this blueprint'),
    'hk_zu':           ('Schließen', 'Close'),
    'hk_nichts':       ('Klick auf das Info-Zeichen einer Zeile — hier steht dann, woher der Bauplan kommt.',
                          'Click the info sign on a row — this shows where the blueprint comes from.'),
    'hk_start':        ('Den hat jeder von Anfang an — es gibt keinen Auftrag, der ihn ausschüttet.',
                          'Everyone has this from the start — no mission hands it out.'),
    'hk_topf':         ('Sonderquelle', 'Special source'),
    'hk_topf_text':    ('Kein regulärer Auftrag schüttet ihn aus — er stammt aus diesem Belohnungstopf. Wann der wieder läuft, entscheidet CIG.',
                          'No regular mission hands it out — it comes from this reward pool. When that runs again is up to CIG.'),
    'hk_keine':        ('Zu diesem Bauplan ist keine Bezugsquelle bekannt.',
                          'No source is known for this blueprint.'),

    # -- Feinfilter über der Bauplan-Liste --
    # Vorher waren es vier Knöpfe, die Bereiche ausblendeten — also das
    # Gegenteil von dem, was man erwartet: Wer „nur FPS-Waffen" wollte, musste
    # drei andere Bereiche wegklicken. Jetzt wird ausgewählt, was man sehen
    # will, und zwar nach fünf Merkmalen.
    'ff_alle_arten':   ('Alle Arten', 'All types'),
    'ff_alle_klassen': ('Alle Klassen', 'All classes'),
    'ff_alle_groessen': ('Alle Größen', 'All sizes'),
    'ff_alle_quellen': ('Alle Quellen', 'All sources'),
    'ff_alle_grade':   ('Alle Grade', 'All grades'),
    # ⚠ Die Unterart heisst je nach Art etwas anderes: Bei Waffen ist es die
    # Waffenart (ballistisch, Laser), bei Ruestung die Rolle (Kampf, Technik).
    # Ein Feld, zwei Beschriftungen — sonst muesste man raten, was es filtert.
    # ⚠ Die Merkliste fuehrt ZWEI Sorten: angeklickte Bauplaene aus dem Katalog
    # und eigene Beobachtungen mit Suchmustern. Die Liste zeigte nur die erste
    # Sorte und meldete „Du beobachtest noch nichts", obwohl neun Eintraege
    # hinterlegt waren. Am 29.08.2026 gemeldet.
    'merk_eigene':      ('Eigene Beobachtungen', 'Your own watches'),
    'merk_wartet':      ('wartet auf: %s', 'waiting for: %s'),
    # ⚠ Abwaehlen muss gehen. Eine Beobachtung, die man nur anlegen, aber nicht
    # loswerden kann, wird zur Altlast: „falls wir die doch auswechseln, dann
    # muss ich die abwählen können." (29.08.2026)
    'merk_eigene_weg':  ('Diese Beobachtung entfernen', 'Remove this watch'),
    'merk_eigene_h':    ('Diese stehen in keinem Katalog — der Watcher hält '
                         'nach den Suchmustern Ausschau, sobald etwas im Spiel '
                         'freigeschaltet wird.',
                         'These are in no catalogue — the watcher looks out '
                         'for the search patterns whenever something is '
                         'unlocked in the game.'),
    'ff_alle_unterarten': ('Alle Unterarten', 'All subtypes'),
    # ⚠ Das leere Feld sagt, dass es etwas zu holen gibt — sonst findet es
    # niemand: „man muss irgendwie sichtbar machen, dass man die Unterarten
    # auswählen kann, niemand hat es auf Anhieb gefunden, erst nach Erklärung."
    # (29.08.2026) Ein Feld mit „Alle Unterarten" sieht aus wie eine Anzeige;
    # eines mit „12 Unterarten ▾" wie eine Einladung.
    'ff_unterart_waehlen': ('%d Unterarten — hier verfeinern',
                            '%d subtypes — refine here'),
    'ff_alle_rollen':   ('Alle Rüstungsrollen', 'All armour roles'),
    'ff_alle_hersteller': ('Alle Hersteller', 'All manufacturers'),
    # Anzeigenamen der Rezept-Arten und Unterarten. ⚠ Gehoeren hierher,
    # nicht ins Datenmodul: Es sind Oberflaechentexte, und der
    # Selbsttest besteht zu Recht darauf, dass jeder davon zweisprachig
    # an EINER Stelle steht.
    'he_art_weapons': ('Waffen', 'Weapons'),
    'he_art_armour': ('Rüstung', 'Armour'),
    'he_art_cooler': ('Kühler', 'Coolers'),
    'he_art_powerplant': ('Generatoren', 'Power plants'),
    'he_art_shield': ('Schilde', 'Shields'),
    'he_art_radar': ('Radar', 'Radar'),
    'he_art_quantumdrive': ('Quantenantriebe', 'Quantum drives'),
    'he_art_ammo': ('Munition', 'Ammunition'),
    'he_art_mininglaser': ('Bergbaulaser', 'Mining lasers'),
    'he_art_tractorbeam': ('Traktorstrahlen', 'Tractor beams'),
    'he_art_refuelling': ('Betankung', 'Refuelling'),
    'he_art_orepod': ('Erzbehälter', 'Ore pods'),
    'he_art_miningmodule': ('Bergbaumodule', 'Mining modules'),
    'he_art_salvage': ('Bergung', 'Salvage'),
    'he_sub_ballistic': ('Ballistisch', 'Ballistic'),
    'he_sub_laser': ('Laser', 'Laser'),
    'he_sub_distortion': ('Distortion', 'Distortion'),
    'he_sub_neutron': ('Neutron', 'Neutron'),
    'he_sub_plasma': ('Plasma', 'Plasma'),
    'he_sub_tachyon': ('Tachyon', 'Tachyon'),
    'he_sub_electron': ('Elektron', 'Electron'),
    'he_sub_pistol': ('Pistole', 'Pistol'),
    'he_sub_rifle': ('Gewehr', 'Rifle'),
    'he_sub_sniper': ('Scharfschütze', 'Sniper'),
    'he_sub_smg': ('Maschinenpistole', 'SMG'),
    'he_sub_shotgun': ('Schrotflinte', 'Shotgun'),
    'he_sub_lmg': ('Leichtes MG', 'LMG'),
    'he_sub_combat': ('Kampf', 'Combat'),
    'he_sub_engineer': ('Technik', 'Engineer'),
    'he_sub_hunter': ('Jagd', 'Hunter'),
    'he_sub_stealth': ('Tarnung', 'Stealth'),
    'he_sub_miner': ('Bergbau', 'Miner'),
    'he_sub_explorer': ('Erkundung', 'Explorer'),
    'he_sub_environment': ('Umwelt', 'Environment'),
    'he_sub_cosmonaut': ('Kosmonaut', 'Cosmonaut'),
    'he_sub_undersuit': ('Unteranzug', 'Undersuit'),
    'he_sub_flightsuit': ('Fluganzug', 'Flight suit'),
    'he_sub_medic': ('Sanitäter', 'Medic'),
    'he_sub_pilot': ('Pilot', 'Pilot'),
    'he_sub_utility': ('Allzweck', 'Utility'),
    'he_sub_heavy': ('Schwer', 'Heavy'),
    'he_sub_light': ('Leicht', 'Light'),
    'he_sub_medium': ('Mittel', 'Medium'),
    # --- Zwei Ebenen: Oberkategorie und Unterart ---------------------------
    # ⚠ Die Gliederung folgt einer erprobten Vergleichsliste, die
    # seit Monaten von Hand pflegt. Was sich dort bewaehrt hat, erfindet das
    # Werkzeug nicht neu.
    'kat_ober_schiffswaffe':    ('Schiffswaffen', 'Ship weapons'),
    'kat_ober_schiffsmodul':    ('Schiffsmodule', 'Ship modules'),
    'kat_ober_schiffswerkzeug': ('Schiffswerkzeuge', 'Ship tools'),
    'kat_ober_fpswaffe':        ('FPS-Waffen', 'FPS weapons'),
    'kat_ober_ausruestung':     ('Ausrüstung', 'Gear'),
    'kat_ober_ruestung':        ('Rüstung', 'Armour'),
    'kat_ober_kleidung':        ('Kleidung', 'Clothing'),
    'kat_ober_sonstiges':       ('Sonstiges', 'Other'),
    # Schiffswaffen
    'kat_unter_ballistic_cannon':   ('Ballistische Kanone', 'Ballistic cannon'),
    'kat_unter_ballistic_gatling':  ('Ballistische Gatling', 'Ballistic gatling'),
    'kat_unter_ballistic_repeater': ('Ballistischer Repeater', 'Ballistic repeater'),
    'kat_unter_laser_cannon':       ('Laserkanone', 'Laser cannon'),
    'kat_unter_laser_repeater':     ('Laser-Repeater', 'Laser repeater'),
    'kat_unter_dist_cannon':        ('Distortion-Kanone', 'Distortion cannon'),
    'kat_unter_dist_repeater':      ('Distortion-Repeater', 'Distortion repeater'),
    'kat_unter_neutron_cannon':     ('Neutronenkanone', 'Neutron cannon'),
    'kat_unter_neutron_repeater':   ('Neutronen-Repeater', 'Neutron repeater'),
    'kat_unter_tachyon_cannon':     ('Tachyonenkanone', 'Tachyon cannon'),
    'kat_unter_scatter_gun':        ('Scattergun', 'Scattergun'),
    'kat_unter_mass_driver':        ('Mass Driver', 'Mass driver'),
    # Schiffswerkzeuge
    'kat_unter_mining_laser':     ('Bergbaulaser', 'Mining laser'),
    'kat_unter_salvage_modifier': ('Salvage-Modifikator', 'Salvage modifier'),
    'kat_unter_salvage_head':     ('Salvage-Kopf', 'Salvage head'),
    'kat_unter_tractor_beam':     ('Traktorstrahl', 'Tractor beam'),
    'kat_unter_andockkragen':     ('Andockkragen', 'Docking collar'),
    'kat_unter_fuelnozzle':       ('Betankungsdüse', 'Fuel nozzle'),
    'kat_unter_frachtmodul':      ('Frachtmodul', 'Cargo module'),
    # Schiffsmodule
    'kat_unter_cooler':       ('Kühler', 'Cooler'),
    'kat_unter_powerplant':   ('Generator', 'Power plant'),
    'kat_unter_quantumdrive': ('Quantenantrieb', 'Quantum drive'),
    'kat_unter_schild':       ('Schild', 'Shield'),
    'kat_unter_radar':        ('Radar', 'Radar'),
    # FPS-Waffen
    'kat_unter_pistole':      ('Pistole', 'Pistol'),
    'kat_unter_gewehr':       ('Gewehr', 'Rifle'),
    'kat_unter_sniper':       ('Scharfschützengewehr', 'Sniper rifle'),
    'kat_unter_smg':          ('Maschinenpistole', 'SMG'),
    'kat_unter_schrotflinte': ('Schrotflinte', 'Shotgun'),
    'kat_unter_lmg':          ('Leichtes MG', 'LMG'),
    # Ausruestung
    'kat_unter_magazin':   ('Magazin', 'Magazine'),
    'kat_unter_munition':  ('Munition', 'Ammunition'),
    'kat_unter_rucksack':  ('Rucksack', 'Backpack'),
    'kat_unter_aufsatz':   ('Waffenaufsatz', 'Weapon attachment'),
    'kat_unter_behaelter': ('Behälter', 'Container'),
    # Ruestung und Kleidung
    'kat_unter_helm':        ('Helm', 'Helmet'),
    'kat_unter_torso':       ('Torso', 'Torso'),
    'kat_unter_arme':        ('Arme', 'Arms'),
    'kat_unter_beine':       ('Beine', 'Legs'),
    'kat_unter_unteranzug':  ('Unteranzug', 'Undersuit'),
    'kat_unter_oberkoerper': ('Oberkörper', 'Torso'),
    'kat_unter_jacke':       ('Jacke', 'Jacket'),
    'kat_unter_schuhe':      ('Schuhe', 'Shoes'),
    # ⭐ Suche nach dem Auftrag: „Retake" fand nichts, obwohl sechs Bauplaene
    # aus solchen Auftraegen stammen. Wer eine Quest fliegt, will wissen, was
    # dabei herausspringt.
    's_bp_auftrag_kopf': ('Aufträge mit „%s"', 'Contracts matching "%s"'),
    's_bp_auftrag_zeile': ('%s — %d Baupläne', '%s — %d blueprints'),
    # ⚠ Eine Zeile, die aussieht wie eine Antwort, aber nichts tut, ist eine
    # Sackgasse: „die Quest muss natürlich anklickbar sein, sonst bringt das
    # nichts." (29.08.2026)
    's_bp_auftrag_klick': ('Klick auf einen Auftrag zeigt nur seine Baupläne.',
                           'Click a contract to see only its blueprints.'),
    's_bp_auftrag_aktiv': ('Nur aus: %s', 'Only from: %s'),
    's_bp_auftrag_weg':   ('Auftrag lösen', 'Clear contract'),
    's_bg_alle_erze':    ('Alle Rohstoffe', 'All materials'),
    's_bg_alle_orte':    ('Alle Orte', 'All locations'),
    'ff_alle_zustaende': ('Bauplan: alle', 'Blueprint: all'),
    # ⭐ Zweiter Filter auf der Herstellung: Reicht mein Material?
    # ⚠ „laut deinem Lager" steht bewusst dabei — der Watcher kennt den
    # Frachtraum nicht, er kennt nur die eigene Liste.
    'ff_alle_material':  ('Material: alle', 'Material: all'),
    'ff_material_reicht': ('Material reicht', 'Have the material'),
    'ff_material_fehlt': ('Material fehlt', 'Material missing'),
    'ff_zustand_habe':  ('Bauplan vorhanden', 'Blueprint owned'),
    'ff_zustand_fehlt': ('Bauplan fehlt', 'Blueprint missing'),
    'ff_groesse':      ('Größe %s', 'Size %s'),
    'ff_grad':         ('Grad %s', 'Grade %s'),
    # ⚠ „Auswahl zurücksetzen", nicht nur „zurücksetzen". Auf einem Knopf
    # allein sagt „zurücksetzen" nicht, WAS zurückgeht — und der Knopf wurde
    # ohnehin schon einmal übersehen.
    'ff_zuruecksetzen': ('Auswahl zurücksetzen', 'Clear filters'),
    'ff_treffer':      ('%d von %d Bauplänen', '%d of %d blueprints'),
    'ff_alle_treffer': ('alle %d Baupläne', 'all %d blueprints'),

    # --- Hauptfenster: Reiter und Rahmen (ab v3.0.0) ---
    'hf_titel':          ('SC BP Watcher', 'SC BP Watcher'),
    # Zusatz im Fenstertitel, wenn die Testfassung laeuft (SC_BP_TESTFASSUNG).
    # ⚠ Zwei gleich aussehende Fenster nebeneinander sind eine Falle: Man
    # verstellt etwas in der falschen Fassung und sucht dann den Fehler.
    's_testfassung':     ('⚠ TESTFASSUNG', '⚠ TEST BUILD'),
    # --- Seite „Herstellung" -------------------------------------------------
    's_he_lead':         ('Was ein Gegenstand zum Herstellen braucht. Klick auf '
                          'eine Zeile zeigt die Zutaten.',
                          'What an item needs to be crafted. Click a row to see '
                          'the ingredients.'),
    # ⚠ „Suchen …" verschweigt, WONACH. Seit die Suche auch die Zutaten
    # kennt, ist das die halbe Funktion: Wer nicht weiss, dass er einen
    # Rohstoff eintippen darf, findet nie heraus, was daraus wird.
    # Vorbild ist der Bergbau, der seit jeher „Rohstoff oder Ort …" sagt.
    's_he_suche':        ('Bauplan oder Rohstoff …',
                          'Blueprint or resource …'),
    's_he_von':          (' von %d herstellbar — davon hast du den Bauplan',
                          ' of %d craftable — you have the blueprint for these'),
    # ⚠ Der Zusatz hinter der Kopfzahl, wenn Bauplaene wegen mehrdeutiger
    # Namen NICHT mitgezaehlt werden. Ohne ihn steht dort eine Zahl, die
    # kleiner ist als der eigene Bestand, und nichts erklaert die Luecke —
    # genau daran ist ein „404 von 1597" bei 405 Bauplaenen aufgefallen.
    's_he_dazu_unklar':  (' · %d weitere unklar',
                          ' · %d more unclear'),
    's_he_zeit':         ('Herstellzeit', 'Craft time'),
    # ⚠ Lesbar statt roh: 960 Sekunden sind 16 Minuten, und niemand rechnet
    # das im Kopf um. Unter einer Minute bleibt es bei Sekunden.
    's_he_sekunden':     ('%d s', '%d s'),
    's_he_minuten':      ('%d min', '%d min'),
    's_he_std_min':      ('%d h %d min', '%d h %d min'),
    's_he_menge':        ('%g SCU', '%g SCU'),
    # ⚠ Der unklare Fall — siehe herstellung.mit_bestand().
    's_he_unklar':       ('Bauplan vorhanden, aber es gibt mehrere Gegenstände '
                          'dieses Namens — welcher gemeint ist, geht aus den '
                          'Daten nicht hervor.',
                          'Blueprint present, but several items share this name '
                          '— the data does not say which one is meant.'),
    's_he_mehr':         ('… und %d weitere. Grenz die Suche ein.',
                          '… and %d more. Narrow your search.'),
    's_he_nichts':       ('Nichts gefunden.', 'Nothing found.'),
    # --- Lager (scbp/rohstoffe.py) ------------------------------------------
    'hf_lager':          ('Mein Lager', 'My stock'),
    # ---------------------------------------------------- Reiter „Verkauf"
    's_vk_lead':         ('Wo du deine Ware los wirst — und was sie je SCU '
                          'bringt. Mehrere Waren auf einmal: Orte, die alles '
                          'nehmen, stehen oben.',
                          'Where to sell your cargo — and what it pays per '
                          'SCU. Pick several goods at once: places that take '
                          'all of them come first.'),
    's_vk_ware':         ('Ware suchen', 'Search commodity'),
    's_vk_holen':        ('Preise aktualisieren', 'Refresh prices'),
    's_vk_holt':         ('holt …', 'fetching …'),
    's_vk_geholt':       ('Preise sind aktuell.', 'Prices are up to date.'),
    's_vk_gesperrt':     ('Gerade erst geholt — der Knopf zeigt, wann es '
                          'wieder geht.',
                          'Just fetched — the button shows when it is ready '
                          'again.'),
    's_vk_fehler':       ('Die Preise konnten nicht geholt werden. Der letzte '
                          'Stand bleibt stehen.',
                          'Could not fetch prices. The previous data is kept.'),
    's_vk_kein_netz_aus': ('Netzzugriff ist abgeschaltet (SC_BP_NO_NET).',
                           'Network access is switched off (SC_BP_NO_NET).'),
    's_vk_stand':        ('Stand: {alter}', 'Updated: {alter}'),
    # ⚠ Nennt beide Nummern. „Veraltet" allein lässt offen, wie schlimm es ist —
    # wer sieht, dass zwischen 4.10.0 und 4.11.0 ein Patch liegt, weiß es.
    's_vk_patch':        ('Zahlen aus {alt} — im Spiel läuft {neu}',
                            'Numbers from {alt} — the game is on {neu}'),
    # ⚠ Der Füllstand sagt, was zu TUN ist, nicht was gemessen wurde. „Stufe 6
    # von 7" hilft niemandem am Terminal; „nimmt kaum noch ab" schon.
    # ⚠ „Fertig kaufen" nennt Preis **und** Ort. Ein Preis ohne Ort lässt die
    # Frage offen, die als Nächstes kommt — und der Weg gehört zur Rechnung.
    's_he_fertig_kaufen': ('Fertig kaufen: %s  ·  %s',
                             'Buy it finished: %s  ·  %s'),
    # --- Reiter „Läden" ---
    'hf_laeden':         ('Läden', 'Shops'),
    's_ld_lead':         ('Wo ein fertiges Teil im Regal steht — und was es '
                          'dort kostet.',
                            'Where a finished part sits on the shelf — and '
                            'what it costs there.'),
    's_ld_sucht':        ('Wird nachgeschlagen …', 'Looking it up …'),
    # ⚠ „Nirgends im Handel" wäre eine Behauptung über das Spiel. Wir wissen
    # nur, dass unsere Quelle es nicht führt — und die hat Lücken.
    's_ld_unbekannt':    ('Zu diesem Teil liegen keine Ladenpreise vor. Das '
                          'heißt nicht, dass es niemand verkauft — unsere '
                          'Preisquelle kennt es nur nicht.',
                            'No shop prices are available for this part. That '
                            'does not mean nobody sells it — our price source '
                            'simply does not list it.'),
    's_ld_zustand':      ('Zustand %d %%', 'Condition %d%%'),
    # --- Reiter „Routen" ---
    'hf_routen':         ('Routen', 'Routes'),
    's_rt_lead':         ('Sag, wo du stehst und was reinpasst — dann rechne '
                          'ich, was sich lohnt.',
                            'Tell me where you are and what fits — then I work '
                            'out what pays off.'),
    's_rt_wo':           ('Wo stehst du gerade?', 'Where are you right now?'),
    's_rt_scu':          ('Frachtraum (SCU)', 'Cargo hold (SCU)'),
    's_rt_geld':         ('Geld (aUEC)', 'Money (aUEC)'),
    's_rt_kein_ort':     ('Tippe oben ein, wo du gerade bist — dann siehst du, '
                          'was sich von dort aus lohnt.',
                            'Type where you are above — then you will see what '
                            'pays off from there.'),
    's_rt_rechnet':      ('Wird nachgeschlagen …', 'Looking it up …'),
    's_rt_nichts':       ('Von hier aus lohnt sich gerade nichts — jedenfalls '
                          'nicht mit diesem Frachtraum und diesem Geld.',
                            'Nothing pays off from here right now — at least '
                            'not with this cargo hold and this money.'),
    's_rt_einzeln':      ('Eine Fahrt', 'A single run'),
    's_rt_ketten':       ('Zwei Fahrten hintereinander',
                            'Two runs in a row'),
    's_rt_nach_gewinn':  ('bester Gewinn', 'best profit'),
    's_rt_nach_strecke': ('kurze Strecke', 'short distance'),
    's_rt_strecke':      ('%d Gm', '%d Gm'),
    # ⚠ Auch „SCU" läuft durch `t()`. Die Einheit heißt zwar in beiden
    # Sprachen gleich — aber eine Ausnahme „das ist doch international" hebelt
    # die Regel aus, und die nächste Zeile ist dann keine Einheit mehr.
    's_rt_scu_menge':    ('%d SCU', '%d SCU'),
    's_rt_schritt':      ('   %d)  %d SCU %s  →  %s',
                            '   %d)  %d SCU %s  →  %s'),
    's_vk_fuellt':       ('· Lager füllt sich', '· stock filling up'),
    's_vk_voll':         ('· nimmt kaum noch ab', '· barely buying'),
    's_vk_kein_stand':   ('Noch keine Preise geholt.', 'No prices fetched yet.'),
    's_vk_nur_nqa':      ('nur Orte ohne Fragen (gestohlene Ware)',
                          'only no-questions-asked places (stolen cargo)'),
    's_vk_aus_lager':    ('Aus meinem Handelslager', 'From my cargo hold'),
    's_vk_lager_leer':   ('Im Handelslager liegt nichts, wofür es Preise gibt.',
                          'Nothing in the cargo hold has known prices.'),
    's_vk_leer_hinweis': ('Such oben eine Ware — oder übernimm gleich alles '
                          'aus deinem Handelslager.',
                          'Search for a commodity above — or take everything '
                          'from your cargo hold at once.'),
    's_vk_keine_orte':   ('Für diese Auswahl ist kein Ankäufer bekannt.',
                          'No buyer known for this selection.'),
    's_vk_nichts_gefunden': ('Keine Ware mit diesem Namen.',
                             'No commodity by that name.'),
    's_vk_nqa_marke':    ('keine Fragen', 'no questions'),
    's_vk_je_scu':       ('{preis} je SCU', '{preis} per SCU'),
    's_vk_aus_lager_zeile': ('{menge} SCU → {summe}', '{menge} SCU → {summe}'),
    's_vk_erloes':       ('Ladung hier: {summe}', 'Cargo here: {summe}'),
    's_vk_alter_frisch': ('gerade eben', 'just now'),
    's_vk_alter_stunden': ('vor {n} Std.', '{n}h ago'),
    's_vk_alter_tage':   ('vor {n} Tagen', '{n}d ago'),
    # ----------------------------------------------- Reiter „Handelslager"
    's_hl_lead':         ('Was du zum Verkauf im Laderaum hast. Getrennt vom '
                          'Werkstatt-Lager: Das hier willst du loswerden, '
                          'nicht verbauen.',
                          'What you carry to sell. Kept apart from the '
                          'workshop stock: this is cargo you want to get rid '
                          'of, not to build with.'),
    's_hl_hinweis':      ('Keine Qualität — der Ankaufpreis hängt nicht daran, '
                          'und erbeutete Ware hat ohnehin immer Q 0. Setz '
                          'stattdessen den Haken, wenn die Ladung als '
                          'gestohlen markiert ist.',
                          'No quality here — the buy price does not depend on '
                          'it, and looted cargo is always Q 0 anyway. Tick the '
                          'box instead if the cargo is marked as stolen.'),
    's_hl_ware':         ('Ware', 'Commodity'),
    's_hl_menge':        ('Menge in SCU', 'Amount in SCU'),
    's_hl_ort':          ('Lagerort (freiwillig)', 'Storage location (optional)'),
    's_hl_gestohlen':    ('als gestohlen markiert', 'marked as stolen'),
    's_hl_buchen':       ('Eintragen', 'Add'),
    's_hl_speichern':    ('Änderung speichern', 'Save change'),
    's_hl_abbrechen':    ('Abbrechen', 'Cancel'),
    's_hl_ergibt':       ('ergibt {menge} SCU', 'makes {menge} SCU'),
    's_hl_rechnung_kaputt': ('Das ist keine Rechnung, die ich verstehe. '
                             'Erlaubt sind + und −, zum Beispiel 100+5.',
                             'That is not a calculation I understand. Use + '
                             'and −, for example 100+5.'),
    's_hl_unter_null':   ('Das ergibt null oder weniger — trag eine Menge über '
                          'null ein.',
                          'That comes out at zero or less — enter an amount '
                          'above zero.'),
    's_hl_aendern_hinweis': ('Zeile anklicken zum Ändern. Im Mengenfeld darfst '
                             'du rechnen: 40+5 oder 40−12.',
                             'Click a row to edit it. The amount field does '
                             'maths: 40+5 or 40−12.'),
    's_hl_gebucht':      ('Eingetragen.', 'Added.'),
    's_hl_unbekannt':    ('Diese Ware kennt der Handel nicht. Nimm einen '
                          'Vorschlag aus der Liste.',
                          'That commodity is not traded. Pick one from the '
                          'suggestions.'),
    's_hl_ort_unbekannt': ('Diesen Lagerort gibt es nicht. Nimm einen '
                           'Vorschlag — oder lass das Feld leer.',
                           'No such storage location. Pick a suggestion — or '
                           'leave the field empty.'),
    's_hl_fehlt_ware':   ('Es fehlt die Ware.', 'The commodity is missing.'),
    's_hl_fehlt_menge':  ('Die Menge muss eine Zahl über null sein.',
                          'The amount must be a number above zero.'),
    's_hl_fehler':       ('Konnte nicht gespeichert werden.',
                          'Could not be saved.'),
    's_hl_leer':         ('Noch nichts eingetragen.', 'Nothing entered yet.'),
    's_hl_scu':          ('{menge} SCU', '{menge} SCU'),
    'b_fenstermass':     (' · Fenster %d×%d, mindestens %d×%d',
                          ' · window %d×%d, minimum %d×%d'),
    'b_fenster_zu_hoch': (' ⚠ Mindesthöhe größer als der Bildschirm',
                          ' ⚠ minimum height exceeds the screen'),
    's_af_weitere':      ('… und {n} weitere — tipp weiter, um einzugrenzen',
                          '… and {n} more — keep typing to narrow it down'),
    's_hl_sp_ware':      ('Ware', 'Commodity'),
    's_hl_sp_menge':     ('SCU', 'SCU'),
    's_hl_sp_ort':       ('Ort', 'Location'),
    's_hl_sp_je_scu':    ('Preis 1 SCU', 'Price per SCU'),
    's_hl_sp_gesamt':    ('Gesamtpreis', 'Total'),
    's_hl_marke':        ('gestohlen', 'stolen'),
    's_hl_wert':         ('höchstens {summe}', 'up to {summe}'),
    's_hl_gesamt':       ('Ladung höchstens: {summe}', 'Cargo up to: {summe}'),
    # Handelslager sichern und zurueckholen — dieselben Knopfnamen wie im
    # Werkstatt-Lager (`s_lg_aus_json`/`s_lg_aus_csv`/`s_lg_einlesen`/
    # `s_lg_leeren`), die werden wiederverwendet statt hier zu doppeln: Zwei
    # Fassungen derselben Beschriftung gehen mit der Zeit auseinander, und
    # dann heisst derselbe Knopf auf zwei Seiten verschieden. Eigene Texte
    # bekommt nur, was wirklich vom Handel spricht.
    's_hl_ausgeben':     ('Handelslager ausgeben', 'Export cargo'),
    's_hl_eingelesen':   ('%d Posten eingelesen — dein Handelslager wurde '
                          'ersetzt.',
                          '%d entries loaded — your cargo was replaced.'),
    # ⚠ Beide Lager schreiben `{"format": 1, "posten": […]}`. Wer die
    # falsche Sicherung waehlt, soll erfahren, welche hier hingehoert — sonst
    # sucht er den Fehler in der Datei.
    's_hl_datei_falsch': ('Das ist keine Handelslager-Sicherung. Die Sicherung '
                          'vom Werkstatt-Lager gehört unter „Mein Lager“.',
                          'That is not a cargo backup. A backup from your '
                          'workshop stock belongs under “My stock”.'),
    's_hl_leeren_frage_t': ('Wirklich das ganze Handelslager löschen?',
                            'Really clear the whole cargo?'),
    's_hl_leeren_frage': ('%d Posten werden entfernt. Das lässt sich nicht '
                          'rückgängig machen — sichere vorher, wenn du sie '
                          'noch brauchst.',
                          '%d entries will be removed. This cannot be undone — '
                          'export first if you still need them.'),
    's_hl_geleert':      ('Handelslager geleert — %d Posten entfernt.',
                          'Cargo cleared — %d entries removed.'),
    # ⭐ Der Patch-Hinweis steht hier und nicht im Werkstatt-Lager: Nach einem
    # Wischen ist der Laderaum leer, das Baumaterial aber oft nicht.
    's_hl_aus_hilfe':    ('Die Sicherung lässt sich hier wieder einlesen. Die '
                          'Tabelle ist zum Ansehen und Weitergeben — sie kann '
                          'nicht zurückgelesen werden. Nach einem Patch, der '
                          'alles zurücksetzt, räumt „Lager löschen“ den '
                          'Laderaum in einem Zug leer.',
                          'The backup can be loaded here again. The spreadsheet '
                          'is for reading and sharing — it cannot be loaded '
                          'back. After a patch wipe, “Clear stock” empties the '
                          'hold in one go.'),
    's_lg_lead':         ('Was du an Rohstoffen hast. Trag es selbst ein — das '
                          'Spiel verrät es nicht. Beim Herstellen zieht der '
                          'Watcher die Zutaten ab.',
                          'The resources you hold. Enter them yourself — the '
                          'game does not reveal them. When you craft, the '
                          'watcher deducts the ingredients.'),
    's_lg_material':     ('Rohstoff', 'Resource'),
    's_lg_menge':        ('Menge (SCU)', 'Amount (SCU)'),
    # ⚠ Die Beschriftung sagt immer, in welcher Einheit das Feld gerade
    # rechnet. Ein Kaestchen daneben schaltet um — stuende dort dauerhaft
    # „(SCU)", waere jede cSCU-Eingabe stillschweigend hundertfach zu gross.
    's_lg_menge_cscu':   ('Menge (cSCU)', 'Amount (cSCU)'),
    # Die Einheit selbst heisst in beiden Sprachen gleich — sie steht
    # trotzdem hier, weil jeder sichtbare Text durch `t()` laeuft.
    's_lg_cscu':         ('cSCU', 'cSCU'),
    # Die Skala der Rezepte laeuft 0 bis 1000, NICHT in Prozent. Stand hier
    # als 'Guete %' — wer im Spiel 72 abliest und eintraegt, haette danach
    # lauter falsche Ergebnisse bekommen: sein Erz gaelte als unbrauchbar.
    's_lg_qualitaet':    ('Qualität 0–1000', 'Quality 0–1000'),
    # Vorschlaege beim Eintippen — ein freies Feld fuer einen Namen, der exakt
    # passen muss, ist eine stille Fehlerquelle. Wer "Aslerite" schreibt,
    # bekommt nie einen Treffer und erfaehrt auch nicht, warum.
    # Ruecmeldungen beim Eintragen. ⚠ Vorher war das Feld stumm, wenn der
    # Name fehlte — Knopf gedrueckt, nichts passiert, kein Hinweis. Und bei
    # einer krummen Menge stand die Feldbeschriftung da statt einer Erklaerung.
    's_lg_kein_material': ('Trag zuerst ein Material ein.',
                           'Enter a material first.'),
    # ⚠ Beim Anlegen ist eine negative Menge keine Buchung, sondern
    # Unsinn — dort darf nicht „So viel ist nicht da" stehen.
    's_lg_nicht_negativ': ('Eine Menge kann nicht negativ sein.',
                          'An amount cannot be negative.'),
    's_lg_keine_menge':  ('Trag eine Menge ein, zum Beispiel 12,5',
                          'Enter an amount, for example 12.5'),
    's_lg_eingetragen':  ('Eingetragen: %s · %g SCU', 'Added: %s · %g SCU'),
    's_lg_summe_eins':   ('%d Posten · 1 Rohstoff', '%d entries · 1 material'),
    's_lg_meinst_du':    ('Meintest du:', 'Did you mean:'),
    # ⚠⚠ Dieser Satz versprach bis v3.3.0-rc42 „Du kannst es trotzdem
    # eintragen" — und war damit **falsch**, seit der Knopf dafuer weg ist. Der
    # Text stand an einer anderen Stelle als die Meldung `s_lg_name_fremd` und
    # blieb beim Aufraeumen stehen. Auf dem Bildschirm behauptete das Programm
    # also etwas, das es nicht tut. Am 30.08.2026 aufgefallen.
    #
    # ⚠ Wer eine Funktion entfernt, sucht nach ALLEN Stellen, die sie
    # beschreiben — nicht nur nach dem Knopf.
    's_lg_unbekannt':    ('Dieses Material gibt es in Star Citizen nicht. '
                          'Eintragen lassen sich nur Rohstoffe und Pflanzen aus '
                          'dem Spiel — tipp die ersten Buchstaben, dann kommt '
                          'der Vorschlag.',
                          'This material does not exist in Star Citizen. Only '
                          'resources and plants from the game can be entered — '
                          'type the first letters and a suggestion appears.'),
    's_lg_q_wert':       ('Q %g', 'Q %g'),
    's_lg_ort':          ('Lagerort (freiwillig)',
                          'Storage location (optional)'),
    's_lg_eintragen':    ('Eintragen', 'Add'),
    's_lg_leer':         ('Noch nichts eingetragen.', 'Nothing entered yet.'),
    's_lg_weg':          ('Löschen', 'Remove'),
    # --- Einen vorhandenen Posten berichtigen -----------------------------
    # ⚠ Eintragen ohne Berichtigen ist halb fertig: Wer sich vertippt oder
    # Material weitergegeben hat, stand vor einer Liste, die er nur noch
    # loeschen konnte. Am 29.08.2026 gemeldet: „wenn ich was korrigieren will
    # geht das gar nicht".
    's_lg_zeile_klick':  ('Klick auf eine Zeile, um sie zu ändern.',
                          'Click a row to change it.'),
    's_lg_bearbeite':    ('Du änderst diesen Posten: %s',
                          'You are changing this entry: %s'),
    's_lg_speichern':    ('Änderung speichern', 'Save change'),
    's_lg_abbrechen':    ('Abbrechen', 'Cancel'),
    's_lg_geaendert':    ('Geändert: %s · %g SCU', 'Changed: %s · %g SCU'),
    # Auf- und Abbuchen statt Kopfrechnen: Wer zwei SCU abgibt, soll „-2"
    # tippen koennen und nicht erst ausrechnen muessen, was uebrig bleibt.
    # ⚠⚠ Der alte Satz lautete „Menge überschreiben — oder +5 bzw. -2
    # tippen, dann wird auf- oder abgebucht." Er beschrieb eine Mechanik,
    # statt zu zeigen, was zu tun ist: „auf- und abbuchen" ist Buchhalter-
    # sprache, und WO die Zeichen hingehoeren stand nirgends. Am 30.08.2026
    # gemeldet: „wie genau es geht kapier ich nicht, steht da auch nicht"
    # und „didaktisch schon grausam".
    #
    # Jetzt: eine Handlung je Zeile, mit Beispiel. Die eigentliche
    # Erklaerung ist ohnehin die Vorschau neben dem Feld — sie zeigt beim
    # Tippen, was herauskommt.
    's_lg_rechnen':      ('Neue Menge eintippen — oder anhängen: '
                          '»+3« legt 3 dazu, »-3« nimmt 3 weg.',
                          'Type the new amount — or append: '
                          '»+3« adds 3, »-3« removes 3.'),
    # Die Vorschau neben dem Mengenfeld.
    's_lg_ergibt':       ('ergibt %g SCU', 'makes %g SCU'),
    's_lg_ergibt_null':  ('ergibt 0 — der Posten wird gelöscht',
                          'makes 0 — the entry will be removed'),
    's_lg_ergibt_minus': ('mehr als vorhanden (%g SCU)',
                          'more than you have (%g SCU)'),
    's_lg_zu_wenig':     ('So viel ist nicht da. Vorhanden: %g SCU',
                          'You do not have that much. Available: %g SCU'),
    's_lg_alles_weg':    ('%s ist aufgebraucht — der Posten ist weg.',
                          '%s is used up — the entry is gone.'),
    # ⚠ Der Name ist der Schluessel zwischen Lager und Rezept. Ein Vertipper
    # macht den Bestand still unbrauchbar: Die Liste sieht richtig aus, nur die
    # Haekchen bleiben aus. Deshalb wird abgeglichen, statt zu uebernehmen.
    # ⚠⚠ Es gibt keinen Ausweg mehr — der Text darf also nicht klingen,
    # als gaebe es einen. „Du kannst es trotzdem eintragen" stand hier
    # bis v3.3.0-rc40 und war die Einladung, ein freies Textfeld zu
    # benutzen. Grund fuer die Sperre: siehe `herstellung.einlagerbar()`.
    # ⚠ Auch der Lagerort ist eine geschlossene Liste — aus demselben Grund
    # wie der Rohstoffname. „Bei Oma im Keller ist eben keine Location mit
    # Lager in SC." (30.08.2026)
    's_lg_ort_fremd':    ('„%s" gibt es in Star Citizen nicht. Tipp die '
                          'ersten Buchstaben einer Station oder Stadt, dann '
                          'kommt der Vorschlag — oder lass das Feld leer.',
                          '„%s" does not exist in Star Citizen. Type the '
                          'first letters of a station or city and a '
                          'suggestion appears — or leave the field empty.'),
    's_lg_name_fremd':   ('„%s" gibt es im Spiel nicht. Es lassen sich nur '
                          'Rohstoffe und Pflanzen aus Star Citizen '
                          'eintragen — tipp die ersten Buchstaben, dann '
                          'kommt der Vorschlag.',
                          '„%s" does not exist in the game. Only Star '
                          'Citizen resources and plants can be entered — '
                          'type the first letters and a suggestion '
                          'appears.'),
    's_lg_keine_guete':  ('Trag die Qualität ein, eine Zahl von 0 bis 1000',
                          'Enter the quality, a number from 0 to 1000'),
    's_lg_berichtigt':   ('Name berichtigt: %s → %s',
                          'Name corrected: %s → %s'),
    's_lg_summe':        ('%d Posten · %d Rohstoffe', '%d entries · %d resources'),
    # ⚠ Bewusst „dir fehlt", nicht „du kannst nicht bauen" — das Lager wird von
    # Hand gepflegt und ist irgendwann lückenhaft. Ein Hinweis darf danebenliegen,
    # eine Behauptung nicht.
    # Wirkung der Materialqualitaet auf die Werte des Produkts.
    # 1540 der 1607 Bauplaene haben solche Angaben (gemessen 29.08.2026).
    's_he_werte':        ('Mit deinem Material', 'With your material'),
    # ⚠ Dieselbe Flaeche zeigt zwei verschiedene Dinge, also braucht sie zwei
    # Ueberschriften. Steht nichts im Lager, ist es kein „dein Material" —
    # dann wird ein Wert durchgespielt, und das muss dranstehen. Am 29.08.2026
    # gesehen: „dir fehlt: 1.2" bei Borase, darunter „Mit deinem Material".
    's_he_werte_probe':  ('Was Qualität %g bringen würde',
                          'What quality %g would give'),
    # ⚠ Seit es je Material einen eigenen Regler gibt, waere EINE Zahl in
    # der Ueberschrift eine Luege — es sind mehrere. Also nur der
    # Hinweis, dass gerechnet und nicht gemessen wird.
    's_he_werte_probe_je': ('Durchgespielt — nicht dein Lagerstand',
                          'Simulated — not your stock'),
    's_he_faktor':       ('× %.3f', '× %.3f'),
    # ⚠ Bei Rueckstoss und Treibstoffverbrauch ist WENIGER besser. Ohne
    # diesen Zusatz liest man „× 0.800" als Verschlechterung, obwohl es
    # der bestmoegliche Wert ist.
    's_he_weniger_gut':  ('weniger ist besser', 'lower is better'),
    # ⭐ Suche nach Zutat: „was kann ich aus X bauen?"
    's_he_aus':          ('Aus %s: %d Baupläne', 'From %s: %d blueprints'),
    's_he_aus_keine':    ('%s kommt in keinem Rezept vor — daraus lässt '
                          'sich nichts herstellen.',
                          '%s appears in no recipe — nothing can be made '
                          'from it.'),
    # ⚠ „kaufen oder abbauen?" — die Frage, die nach „dir fehlt X" kommt.
    # Ein Kaufpreis von 0 heisst NICHT kaufbar, nicht kostenlos.
    # ⚠ Die Qualitaet gehoert an den Preis. Ohne sie liest sich „kaufen"
    # wie ein gleichwertiger Weg — ist es nicht: Q 500 ist der Nullpunkt,
    # der Faktor also exakt 1,000 auf alles.
    's_he_kaufen':       ('kaufen: %s aUEC · Q %d',
                          'buy: %s aUEC · Q %d'),
    's_he_kauf_q':       ('Am Terminal gekaufte Ware hat immer Qualität %d '
                          '— den Nullpunkt. Ein daraus gebauter Gegenstand '
                          'bekommt auf jede Eigenschaft genau ×1,000. '
                          'Besser wird er ausschließlich mit selbst '
                          'abgebautem Erz darüber.',
                          'Goods bought at a terminal are always quality %d '
                          '— the base point. An item made from them gets '
                          'exactly ×1.000 on every property. It only gets '
                          'better with self-mined ore above that.'),
    's_he_nur_abbau':    ('nicht kaufbar — nur abbaubar',
                          'cannot be bought — mining only'),
    # Raffinerien — die Frage nach „wo baue ich das ab?" ist „und wohin
    # bringe ich es?"
    # ⭐ Scan-Signatur: der Scanner zeigt eine Zahl, aber nicht, was
    # dahintersteckt. Genau die Luecke schliesst das Feld.
    's_bg_sig_feld':     ('Scan-Wert vom Scanner', 'Scanner reading'),
    's_bg_sig_hilfe':    ('Der Scanner zeigt eine Zahl — hier steht, welches '
                          'Erz dahintersteckt und aus wie vielen Brocken das '
                          'Vorkommen besteht. „8600" für genau diesen Wert, '
                          '„~8600" mit 10 % Spielraum, „12000-13000" für alles '
                          'dazwischen.',
                          'The scanner shows a number — this tells you which ore '
                          'it is and how many rocks the deposit holds. "8600" for '
                          'an exact match, "~8600" with 10 % tolerance, '
                          '"12000-13000" for a range.'),
    's_bg_sig_treffer':  ('%d× %s', '%d× %s'),
    's_bg_sig_nichts':   ('Kein Erz hat diese Signatur. Mit „~" davor wird mit '
                          '10 %% Spielraum gesucht.',
                          'No ore has this signature. Put "~" in front to search '
                          'with 10 %% tolerance.'),
    's_bg_sig_anzahl':   ('%d mögliche Treffer', '%d possible matches'),
    's_bg_sig_genau':    ('genau', 'exact'),
    's_bg_raff_kopf':    ('Raffinerie — was am meisten herausholt',
                          'Refinery — where you get the most'),
    's_bg_raff_zeile':   ('%+d %%', '%+d %%'),
    # ⚠ Zehn Profile auf zwanzig Stationen — eines davon deckt acht ab.
    # Alle auszuschreiben ergibt eine Textwand; scmdb schreibt aus
    # demselben Grund „+7 others".
    's_bg_raff_weitere': ('%s  +%d weitere', '%s  +%d others'),
    's_bg_raff_egal':    ('Bei diesem Erz macht die Raffinerie keinen '
                          'Unterschied — überall 0 %.',
                          'The refinery makes no difference for this ore — '
                          '0 % everywhere.'),
    's_bg_raff_spanne':  ('%d Prozentpunkte zwischen bester und '
                          'schlechtester Wahl',
                          '%d percentage points between best and worst'),

    # -- Welche Verarbeitungsmethode? (`scbp/raffinerie.py`) --
    # Die dritte Frage der Kette: wo abbauen -> wohin bringen -> wie verarbeiten.
    's_rm_kopf':         ('Welche Verarbeitungsmethode?',
                          'Which refining method?'),
    's_rm_lead':         ('Das Terminal bietet neun Methoden an und zeigt zu '
                          'jeder nur eine Zeile. Sag, was dir wichtig ist.',
                          'The terminal offers nine methods and shows a single '
                          'line for each. Say what matters to you.'),
    's_rm_erste':        ('Am wichtigsten', 'Matters most'),
    's_rm_zweite':       ('Danach', 'Then'),
    's_rm_ertrag':       ('Ertrag', 'Yield'),
    's_rm_kosten':       ('Kosten', 'Cost'),
    's_rm_tempo':        ('Geschwindigkeit', 'Speed'),
    's_rm_nimm':         ('Nimm %s', 'Take %s'),
    # Ertrag · Tempo · Kosten in Worten, so wie das Spiel sie zeigt.
    's_rm_zeile':        ('Ertrag %s · %s · Kosten %s',
                          'Yield %s · %s · cost %s'),
    's_rm_s_gering':     ('gering', 'low'),
    's_rm_s_moderat':    ('moderat', 'moderate'),
    's_rm_s_hoch':       ('hoch', 'high'),
    's_rm_t_sehr':       ('sehr langsam', 'very slow'),
    's_rm_t_langsam':    ('langsam', 'slow'),
    's_rm_t_mittel':     ('mittleres Tempo', 'moderate speed'),
    's_rm_t_schnell':    ('schnell', 'fast'),
    's_rm_alle':         ('Alle neun im Vergleich', 'All nine compared'),
    # Der handfesteste Rat, den das Werkzeug geben kann.
    's_rm_unterlegen':   ('%s lohnt sich nie: %s bringt mehr und kostet '
                          'nicht mehr.',
                          '%s is never worth it: %s gives more and costs no '
                          'more.'),
    's_rm_zeit_laeuft':  ('Die Verarbeitungszeit läuft in Echtzeit weiter, '
                          'auch ausgeloggt — wer den Auftrag vor dem '
                          'Feierabend abschickt, zahlt sie mit nichts.',
                          'Processing time keeps running in real time, even '
                          'when logged out — submit the order before you quit '
                          'and it costs you nothing.'),
    's_rm_stand':        ('Im Spiel abgelesen, Alpha %s (%s). Nach einem '
                          'grösseren Patch prüfen.',
                          'Read in game, Alpha %s (%s). Check again after a '
                          'major patch.'),
    # Prozent neben dem Faktor — die Zahl, die man wirklich liest.
    's_he_prozent':      ('%+.2f %%', '%+.2f %%'),
    # Was mit diesem Material ueberhaupt erreichbar waere.
    's_he_spanne':       ('Q %g–%g · ×%g–%g · Nullpunkt %g',
                          'Q %g–%g · ×%g–%g · base %g'),
    's_he_spanne_ohne':  ('Q %g–%g · ×%g–%g', 'Q %g–%g · ×%g–%g'),
    # Zerlegen: Was NICHT zurueckkommt.
    's_he_zerlegen':     ('Beim Zerlegen kommt %.0f %% des Materials zurück — '
                          'aber nicht: %s',
                          'Dismantling returns %.0f %% of the material — '
                          'except: %s'),
    # ⚠ Power Pips sind Stueckzahlen, keine Multiplikatoren — „× -1.000"
    # war schlicht falsch. Mit Vorzeichen, damit man sieht, ob es
    # dazukommt oder abgeht.
    's_he_absolut':      ('%+g', '%+g'),
    's_he_absolut_null': ('±0', '±0'),
    's_he_woher':        ('%s · Q %g', '%s · Q %g'),
    # Durchspielen: „was käme mit besserem Erz heraus?" — dieselbe Frage,
    # die man auf scmdb.net von Hand stellt, nur mit dem eigenen Lager als
    # Ausgangspunkt.
    's_he_kein_lager':   ('Zieh am Regler, um zu sehen, was eine bestimmte '
                          'Qualität bringt — oder trag unter „Mein Lager" ein, '
                          'was du hast.',
                          'Drag the slider to see what a given quality yields — '
                          'or add what you have under "My stock".'),
    's_he_durchspielen': ('Durchspielen', 'Try a quality'),
    's_he_q_lager':      ('dein Lager', 'your stock'),
    's_he_q_gesetzt':    ('angenommen: Q %d — nicht dein Lagerstand',
                          'assumed: Q %d — not your stock'),
    's_he_zurueck_lager': ('zurück zu deinem Lager', 'back to your stock'),
    's_he_werte_hinweis': ('Was daraus wird, hängt an der Qualität des '
                           'Materials. Gerechnet wird mit dem besten Posten, '
                           'den dein Lager für diesen Bauplan hergibt.',
                           'What you get depends on the quality of the '
                           'material. This uses the best entry your stock has '
                           'for this blueprint.'),
    # ⚠ Bei einer TEILmenge muss beides dastehen. „dir fehlt 0,07" allein
    # verschweigt, dass 0,02 schon da sind — und genau das will man wissen,
    # bevor man losfliegt. (Frage von Xharig, 29.08.2026.)
    # Spaltenkoepfe der Lager-Tabelle — anklickbar zum Sortieren.
    's_lg_sp_material':  ('Material', 'Material'),
    's_lg_sp_menge':     ('Menge', 'Amount'),
    's_lg_sp_q':         ('Qualität', 'Quality'),
    's_lg_sp_ort':       ('Lagerort', 'Location'),
    's_lg_filter':       ('Filtern …', 'Filter …'),
    's_lg_nichts_da':    ('Nichts gefunden.', 'Nothing found.'),
    's_lg_teil':         ('hast %g von %g · fehlt %g',
                          'have %g of %g · missing %g'),
    's_lg_zu_schlecht':  ('%g SCU da, aber unter Q %g',
                          '%g SCU on hand, but below Q %g'),
    's_lg_da':           ('hast du: %g', 'you have: %g'),
    's_lg_fehlt':        ('dir fehlt: %g', 'you are missing: %g'),
    # ⚠ Der Knopf muss sagen, WAS PASSIERT. 'Das stelle ich jetzt her' klang
    # nach einer Aktion im Spiel; dass dabei das eigene Lager verrechnet wird,
    # stand nirgends. Xharig hat ihn am 29.08.2026 selbst nicht gefunden.
    's_lg_bauen':        ('Hergestellt — vom Lager abziehen',
                          'Crafted — deduct from stock'),
    's_lg_bauen_hilfe':  ('Du hast es gebaut? Dann nimmt der Watcher die Zutaten '
                          'aus deinem Lager.',
                          'Built it? Then the watcher takes the ingredients out '
                          'of your stock.'),
    's_lg_abgezogen':    ('Abgezogen.', 'Deducted.'),
    # ⚠ Nichts wird abgezogen, wenn etwas fehlt — der Text muss das sagen.
    # „Abgezogen, so weit vorhanden" stand hier bis v3.3.0-rc35 und
    # beschrieb ein Verhalten, das ein halb leeres Lager hinterliess.
    's_lg_teilweise':    ('Nichts abgezogen — es fehlt: %s',
                          'Nothing deducted — missing: %s'),
    's_lg_fehlt_paar':   ('%s (%g)', '%s (%g)'),
    # Die Mengen in der Zutatenliste, wenn mehr als ein Stueck gebaut wird.
    's_he_menge_n':      ('%g SCU  (%g × %d)', '%g SCU  (%g × %d)'),
    's_he_regler_kopf':  ('Qualität durchspielen — je Material einzeln',
                          'Try qualities — one per material'),
    's_he_regler_lager': ('aus deinem Lager', 'from your stock'),
    's_he_regler_ohne':  ('nichts im Lager', 'nothing in stock'),
    # ⚠ 589 Rezept-Slots haben ein Material OHNE jede Qualitaetswirkung.
    # Dort einen Regler anzubieten heisst: Man zieht, und nichts
    # passiert — ein Bedienelement, das nichts tut, ist schlimmer als
    # keines. Am 30.08.2026 beim Testen aufgefallen (Titanium in der
    # BUL-H4 Armor).
    's_he_ohne_wirkung': ('verändert keine Eigenschaft',
                          'changes no property'),
    's_lg_hinweis':      ('Der Watcher kennt deinen Frachtraum nicht — das hier '
                          'ist deine eigene Liste. Sie sagt dir, was fehlen '
                          'könnte, nicht ob du bauen kannst.',
                          'The watcher cannot see your cargo hold — this is your '
                          'own list. It tells you what might be missing, not '
                          'whether you can build.'),

    # --- Seite „Bergbau" -----------------------------------------------------
    'm_b_aktuell':       ('Bergbau-Daten sind aktuell (%d Orte)',
                          'Mining data is up to date (%d locations)'),
    'm_b_geladen':       ('%d Orte geladen', '%d locations loaded'),
    'm_b_leer':          ('Die Datei enthält keine Orte.',
                          'The file contains no locations.'),
    # ⚠ Der Ton: ein Hinweis, keine Behauptung. Siehe `inventar.py`.

    's_bg_lead':         ('Wo welches Erz abzubauen ist. Tipp einen Rohstoff ein '
                          'für seine Fundorte — oder einen Ort für alles, was es '
                          'dort gibt.',
                          'Where to mine what. Type a resource for its locations '
                          '— or a location for everything found there.'),
    's_bg_suche':        ('Rohstoff oder Ort …', 'Resource or location …'),
    's_bg_nur_orte':     ('%d Orte', '%d locations'),
    's_bg_orte':         ('%d Orte · %d Rohstoffe',
                          '%d locations · %d resources'),
    's_bg_art_fps':      ('FPS', 'FPS'),
    's_bg_art_schiff':   ('Schiff', 'Ship'),
    's_bg_art_schiff_selten': ('Schiff (selten)', 'Ship (rare)'),
    's_bg_art_fahrzeug': ('Fahrzeug', 'Vehicle'),
    's_bg_mehr_info':    ('Genauer — mit Wahrscheinlichkeiten und Refinery-'
                          'Vergleich — auf scmdb.net',
                          'More detail — probabilities and refinery comparison — '
                          'at scmdb.net'),
    's_bg_keine_daten':  ('Die Bergbau-Daten sind noch nicht geladen. Sie kommen '
                          'beim nächsten Katalog-Abruf dazu.',
                          'The mining data is not loaded yet. It arrives with the '
                          'next catalogue update.'),
    's_he_keine_daten':  ('Die Rezepte sind noch nicht geladen. Sie kommen beim '
                          'nächsten Katalog-Abruf dazu.',
                          'The recipes are not loaded yet. They arrive with the '
                          'next catalogue update.'),
    # Name des Melders im Fehlerbericht — **freiwillig**.
    # ⚠ Wird NIE vorausgefüllt (auch nicht mit dem Windows-/Linux-Benutzernamen).
    # Das Werkzeug sammelt sonst nichts über den Nutzer, und im Discord-Post
    # steht „no telemetry" — ein heimlich mitgeschickter Name wäre ein Bruch.
    # ⚠ Der Bericht muss mit jeder neuen Funktion mitwachsen. Ohne diese drei
    # Zeilen liesse sich eine Meldung wie "bei mir ist das Lager leer" nicht
    # beurteilen: Man wuesste weder, ob Posten da sind, noch ob die Rezept- und
    # Bergbaudaten ueberhaupt geladen wurden.
    'b_lager':           ('Mein Lager', 'My stock'),
    'b_n_posten':        ('%d Posten · %d Materialien',
                          '%d entries · %d materials'),
    'b_rezepte':         ('Rezepte', 'Recipes'),
    'b_bergbaudaten':    ('Bergbaudaten', 'Mining data'),
    'b_n_bauplaene_kurz': ('%d Baupläne · Stand %s', '%d blueprints · build %s'),
    'b_n_orte':          ('%d Orte · Stand %s', '%d locations · build %s'),
    'b_nicht_geladen':   ('noch nicht geladen', 'not loaded yet'),
    'b_melder':          ('Von', 'From'),
    's_melder':          ('Dein Name (freiwillig)', 'Your name (optional)'),
    's_melder_h':        ('Steht im Fehlerbericht, damit sich Rückfragen '
                          'zuordnen lassen. Am besten der Discord-Name. Leer '
                          'lassen ist völlig in Ordnung — dann wird nichts '
                          'mitgeschickt.',
                          'Appears in the report so follow-up questions can be '
                          'matched to you. Your Discord name works best. '
                          'Leaving it empty is perfectly fine — then nothing '
                          'is sent.'),
    's_melder_leer':     ('nicht angegeben', 'not given'),
    'hf_gruppe_bp':      ('Baupläne', 'Blueprints'),
    # Hiess frueher Herstellung & Bergbau. Das deckte das Lager nicht ab,
    # das seit v3.3.0 in derselben Gruppe sitzt; drei Woerter waeren als
    # Ueberschrift zu lang geworden.
    'hf_gruppe_herst':   ('Werkstatt', 'Workshop'),
    'hf_herstellung':    ('Herstellung', 'Crafting'),
    'hf_bergbau':        ('Bergbau', 'Mining'),
    # --- Gruppe „Handel" (v3.4.0) ---
    'hf_gruppe_handel':  ('Handel', 'Trading'),
    'hf_verkauf':        ('Verkauf', 'Selling'),
    'hf_handelslager':   ('Handelslager', 'Cargo hold'),
    'hf_gruppe_einst':   ('Einstellungen', 'Settings'),
    'hf_fortgeschritten':('Für Fortgeschrittene', 'For advanced users'),
    'hf_gruppe_info':    ('Info', 'Info'),
    'hf_liste':          ('Bauplan-Liste', 'Blueprint list'),
    # ⚠ „Fortschritt" allein reichte, solange das Fenster nur Baupläne kannte.
    # Mit den Sichten Herstellung und Bergbau ist es mehrdeutig — es könnte der
    # Herstellungs- oder Abbaufortschritt sein. (gemeldet 29.08.2026.)
    'hf_fortschritt':    ('Bauplan-Fortschritt', 'Blueprint progress'),
    'hf_allgemein':      ('Allgemein', 'General'),
    'hf_anzeige':        ('Anzeige', 'Display'),
    'hf_ordner':         ('Pfade', 'Paths'),
    # „Angaben im Spiel" sagte nicht, worum es geht — dahinter stecken die
    # Textquelle (Übersetzung, StarStrings oder Original) und das Eintragen der
    # Bauplan-Angaben in die Auftragstexte. Beides betrifft die Texte der
    # Aufträge, also heißt der Punkt jetzt danach.
    # ⚠ „Texte im Spiel", nicht mehr „Auftragstexte": Der alte Name sagte nicht,
    # **wo** diese Texte auftauchen. Gemeldet am 27.08.2026: „das bescheibt es
    # nicht gut genug".
    #
    # „Ingame-Texte" stand kurz zur Wahl und ist unter Spielern gängig — aber
    # jeder andere Reiter der Leiste ist deutsch (Bauplan-Liste, Fortschritt,
    # Anzeige, Bestand, Serverstatus …). Ein einzelner Anglizismus dazwischen
    # fällt auf, und Einheitlichkeit war der Grund für die ganze Überarbeitung.
    'hf_spiel':          ('Texte im Spiel', 'In-game text'),
    # ⚠ Nicht nur „Bestand". Seit es „Mein Lager" gibt, verwechseln Leute die
    # beiden: Der eine Reiter fuehrt die Bauplaene, der andere die Rohstoffe.
    # Der Name nennt deshalb, worum es geht — und passt zu den Nachbarn
    # „Bauplan-Liste" und „Bauplan-Fortschritt".
    'hf_bestand':        ('Bauplan-Bestand', 'Blueprint inventory'),
    # ⚠ „Über“ allein findet niemand, der ein Update sucht.
    # Gemeldet am 26.08.2026: „ich suche updates auch nicht bei Über“.
    'hf_ueber':          ('Update & Über', 'Update & About'),
    'hf_serverstatus':   ('Serverstatus', 'Server status'),
    'hf_danke':          ('Danke & Lizenzen', 'Thanks & Licenses'),
    's_st_lead':         ('Läuft Star Citizen gerade? Was CIG auf seiner '
                          'Statusseite meldet.',
                          'Is Star Citizen up? What CIG reports on its status page.'),
    's_st_gesamt':       ('Gesamtlage', 'Overall'),
    # Die Kopfzeile bildet nach, was oben auf der Statusseite steht:
    # „Last updated just now" links, „No issues detected" rechts.
    's_st_zuletzt':      ('Zuletzt aktualisiert %s', 'Last updated %s'),
    's_st_gerade':       ('gerade eben', 'just now'),
    's_st_vor_min':      ('vor %d Min.', '%d min ago'),
    's_st_vor_std':      ('vor %d Std.', '%dh ago'),
    # ⚠ Einzahl und Mehrzahl getrennt. „vor 1 Tagen" ist schlicht falsch, und
    # im Englischen ebenso („1 days ago").
    's_st_vor_tag':      ('vor %d Tagen', '%d days ago'),
    's_st_vor_tag_1':    ('vor 1 Tag', '1 day ago'),
    's_st_vor_monat':    ('vor %d Monaten', '%d months ago'),
    's_st_vor_monat_1':  ('vor 1 Monat', '1 month ago'),
    's_st_vor_min_1':    ('vor 1 Min.', '1 min ago'),
    's_st_vor_std_1':    ('vor 1 Std.', '1h ago'),
    's_st_ok':           ('Keine Störung gemeldet', 'No issues detected'),
    's_st_stoerung':     ('Störung gemeldet', 'Issues reported'),
    's_st_letzte':       ('Letzte Meldungen', 'Latest incidents'),
    's_st_erledigt_kurz': ('Erledigt', 'Resolved'),
    's_st_offen':        ('Offen', 'Open'),
    's_st_alle_zeigen':  ('Alle Meldungen auf der Statusseite ansehen',
                          'See all incidents on the status page'),
    's_st_stand':        ('Stand der Seite', 'Page updated'),
    's_st_geholt':       ('Abgerufen', 'Fetched'),
    's_st_quelle':       ('Quelle', 'Source'),
    's_st_nachsehen':    ('Jetzt aktualisieren', 'Refresh now'),
    's_st_laedt':        ('Serverstatus wird geholt …', 'Fetching server status …'),
    's_st_keine':        ('Keine offene Meldung.', 'No open incidents.'),
    's_st_leer':         ('Noch nichts abgerufen. Klick auf „Jetzt nachsehen".',
                          'Nothing fetched yet. Click "Check now".'),
    # ⚠ Ohne Verbindung hilft „Jetzt nachsehen" nicht — dann muss dastehen,
    # woran es liegt, sonst sucht man den Fehler bei sich.
    's_st_kein_netz':    ('Keine Internetverbindung — der Serverstatus lässt '
                          'sich gerade nicht abrufen.',
                          'No internet connection — the server status cannot '
                          'be fetched right now.'),
    's_st_alt_ohne_netz': ('Keine Internetverbindung — das ist der zuletzt '
                           'abgerufene Stand.',
                           'No internet connection — this is the last fetched '
                           'state.'),
    's_st_fehler':       ('Die Statusseite war nicht erreichbar.',
                          'The status page could not be reached.'),
    's_st_betroffen':    ('Betroffen', 'Affected'),
    's_st_seit':         ('seit', 'since'),
    's_st_erledigt':     ('erledigt', 'resolved'),
    # ⚠ Dieser Hinweis gehört unter jede Anzeige und darf nicht wegfallen:
    # Die Seite ist von Hand gepflegt. Ohne den Satz liest sich die Anzeige
    # wie eine Messung, und das wäre eine Aussage, die niemand gemacht hat.
    's_st_hinweis':      ('Diese Angaben stammen von CIG und werden von Hand '
                          'gepflegt — sie sind keine Messung. Läuft etwas '
                          'nicht, obwohl hier „operational" steht, kann beides '
                          'stimmen.',
                          'These entries come from CIG and are maintained by '
                          'hand — they are not a measurement. If something is '
                          'broken while this says "operational", both can be '
                          'true.'),
    'hf_erkennung':      ('Erkennung', 'Detection'),
    'hf_diagnose':     ('Fehler melden', 'Report a problem'),
    'hf_neu':            ('neu', 'new'),
    'hf_sofort':         ('Änderungen werden sofort gespeichert',
                          'Changes are saved right away'),
    'hf_schliessen':     ('Schließen', 'Close'),
    'hf_einrichtung':    ('Einrichtung starten', 'Run setup'),
    'hf_wasistneu':      ('Was ist neu', "What's new"),
    'hf_sicherung':      ('Sicherung', 'Backup'),
    'hf_hinweis_sich':   ('Alles Eigene in eine Datei — Baupläne, beide Lager, '
                          'Auftrags-Protokoll und Einstellungen. Für den '
                          'Rechnerwechsel, und zum Zurückholen',
                          'Everything of yours in one file — blueprints, both '
                          'inventories, mission log and settings. For moving to '
                          'another PC, and to restore'),
    # --- Der Sicherungs-Dialog ---
    'sich_titel':        ('Sicherung', 'Backup'),
    'sich_lead':         ('Alles, was nur du hast, in einer Datei — und wieder '
                          'zurück.',
                          'Everything only you have, in one file — and back again.'),
    'sich_was':          ('Mit dabei sind dein Bauplan-Bestand, das Lager der '
                          'Werkstatt, das Handelslager, das Auftrags-Protokoll, '
                          'die Merkliste, deine Einstellungen und deine '
                          'Steuerung — die aktive Belegung und alle '
                          'gespeicherten Profile. Die heruntergeladenen '
                          'Nachschlagewerke bleiben draußen — die holt sich '
                          'das Programm von allein zurück.',
                          'Included are your blueprint inventory, the workshop '
                          'stock, the trade hold, the mission log, your watchlist, '
                          'your settings and your controls — the active bindings '
                          'and every saved profile. The downloaded reference data '
                          'stays out — the program fetches that on its own.'),
    # ⚠ Die Steuerung wird getrennt gefragt: Sie liegt im Spielordner, nicht in
    # unserer Ablage — und eine falsch zurückgespielte Belegung setzt jemanden
    # vor ein Schiff, das auf nichts mehr reagiert.
    'sich_belegung_frage': ('In der Sicherung steckt auch deine Steuerung: %s.'
                            '\n\nDie gespeicherten Profile zurücklegen?',
                            'The backup also holds your controls: %s.\n\n'
                            'Put the saved profiles back?'),
    'sich_belegung_aktiv': ('Auch die AKTIVE Belegung ersetzen?\n\nDeine '
                            'jetzige wird vorher zur Seite gelegt — aber im '
                            'Spiel gilt danach die aus der Sicherung.',
                            'Replace the ACTIVE bindings as well?\n\nYour '
                            'current ones are set aside first — but in game '
                            'the ones from the backup will apply.'),
    'sich_belegung_ok':  ('Steuerung zurückgelegt: %d Dateien.',
                          'Controls restored: %d files.'),
    'sich_belegung_keine': ('In dieser Sicherung ist keine Steuerung.',
                            'This backup holds no controls.'),
    'sich_schreiben':    ('Sicherung erstellen', 'Create backup'),
    'sich_lesen':        ('Sicherung einspielen', 'Restore backup'),
    'sich_fertig':       ('Gesichert: %d Dateien in %s',
                          'Saved: %d files in %s'),
    'sich_leer':         ('Es gibt noch nichts zu sichern.',
                          'There is nothing to back up yet.'),
    'sich_fehler':       ('Sicherung fehlgeschlagen: %s',
                          'Backup failed: %s'),
    'sich_ungueltig':    ('Das ist keine Sicherung des SC BP Watchers.',
                          'That is not an SC BP Watcher backup.'),
    'sich_frage':        ('Sicherung vom %s mit %d Dateien einspielen?\n\n'
                          'Dein jetziger Stand wird dabei überschrieben — eine '
                          'Kopie davon wird vorher neben der Ablage abgelegt.\n\n'
                          'Danach startet das Programm neu.',
                          'Restore backup from %s with %d files?\n\n'
                          'Your current data will be overwritten — a copy of it '
                          'is put next to the data folder first.\n\n'
                          'The program restarts afterwards.'),
    'sich_zurueck_ok':   ('%d Dateien eingespielt. Das Programm startet neu.',
                          '%d files restored. The program is restarting.'),
    'sich_neustart_selbst': ('Eingespielt. Bitte das Programm einmal neu '
                             'starten — dann ist alles da.',
                             'Restored. Please restart the program once — then '
                             'everything is in place.'),
    # Die Beilage IN der Sicherungsdatei — wer sie in einem Jahr findet, soll
    # ohne das Programm erkennen, was er da hat.
    'sich_datei_info':   ('Erstellt am %s mit SC BP Watcher %s.\n'
                          'Enthaelt %d Dateien: Bauplan-Bestand, Lager,\n'
                          'Auftrags-Protokoll, Merkliste und Einstellungen.\n'
                          '\n'
                          'Zurueckholen: im Programm oben auf "Sicherung"\n'
                          'klicken und diese Datei auswaehlen. Die Ordner\n'
                          'darin entsprechen dem Ablage-Ordner des Programms;\n'
                          'im Notfall reicht auch Entpacken von Hand.',
                          'Created on %s with SC BP Watcher %s.\n'
                          'Contains %d files: blueprint inventory, stock,\n'
                          'mission log, watchlist and settings.\n'
                          '\n'
                          'To restore: click "Backup" at the top of the\n'
                          'program and pick this file. The folders inside\n'
                          'match the program data folder; in an emergency,\n'
                          'unpacking by hand works too.'),
    'hf_hinweis_einr':   ('Einrichtung wiederholen — führt dich noch einmal durch '
                          'Sprache, Spielordner und Bestand',
                          'Repeat setup — walks you through language, game folder '
                          'and inventory again'),
    'hf_hinweis_neu':    ('Was ist neu — die Änderungen dieser und älterer Versionen',
                          "What's new — the changes in this and earlier versions"),
    'hf_schrift':        ('Schriftgröße', 'Text size'),
    'hf_schrift_hilfe':  ('Vergrößert Schrift, Symbole und Knöpfe im ganzen Fenster. '
                          'Wirkt sofort.',
                          'Enlarges text, icons and buttons throughout the window. '
                          'Takes effect immediately.'),
    'hf_s_klein':        ('Klein', 'Small'),
    'hf_s_normal':       ('Normal', 'Normal'),
    'hf_s_gross':        ('Groß', 'Large'),
    'hf_s_sehrgross':    ('Sehr groß', 'Very large'),
    'hf_wer':            ('Wer das gebaut hat', 'Who built this'),
    'hf_dank':           ('Ohne diese Daten gäbe es das Werkzeug nicht',
                          'Without this data the tool would not exist'),
    'hf_nichts_dabei':   ('Alles wird zur Laufzeit von der Originaladresse geholt — '
                          'mitgeliefert wird nichts.',
                          'Everything is fetched from the original address at '
                          'runtime — nothing is bundled.'),
    'hf_fancontent':     ('Dies ist ein inoffizielles Star-Citizen-Fanprojekt und steht '
                          'in keiner Verbindung zur Cloud Imperium Games Corporation '
                          'oder ihren Tochterunternehmen. Alle Inhalte dieses '
                          'Werkzeugs, die nicht von Xharig stammen, sind Eigentum '
                          'ihrer jeweiligen Inhaber.',
                          'This is an unofficial Star Citizen fan project, not '
                          'affiliated with the Cloud Imperium Games Corporation or '
                          'its subsidiaries. All content of this tool that is not '
                          'by Xharig belongs to its respective owners.'),
    'e_vorab':           ('Auch Testversionen anbieten',
                          'Offer test versions too'),
    'e_ton':             ('Signalton', 'Sound'),
    'e_ton_hilfe':       ('Kurzer Ton, wenn ein Bauplan erscheint.',
                          'A short sound when a blueprint shows up.'),
    'e_ja':              ('Ja', 'Yes'),
    'e_ok':              ('Alles klar', 'Got it'),
    'e_nein':            ('Nein', 'No'),
    # ⚠ Für einen Eingabe-Dialog taugt „Alles klar" nicht: Dort wird etwas
    # getan, nicht etwas zur Kenntnis genommen. Der Knopf sagt, was passiert.
    'e_abbrechen':       ('Abbrechen', 'Cancel'),
    'e_speichern':       ('Speichern', 'Save'),
    'e_liste_mehr':      ('  … und %d weitere', '  … and %d more'),
    'e_an':              ('an', 'on'),
    'e_aus':             ('aus', 'off'),
    'e_durchsuchen':     ('Suchen …', 'Browse …'),
    'e_speichern':       ('Speichern', 'Save'),
    'e_neustart_noetig': ('Gespeichert — für Ordner und Prüfintervall den Watcher '
                          'einmal neu starten.',
                          'Saved — restart the watcher for folder and interval '
                          'changes to take effect.'),
    'e_pfad_fehlt':      ('Diesen Ordner gibt es nicht — bitte prüfen.',
                          'That folder does not exist — please check.'),

    # -- Bauplan-Angaben im Spiel (Injektion) --
    'schritt_spiel_texte': ('Bauplan-Angaben im Spiel', 'Blueprint notes in game'),
    'inj_text':          ('Der Watcher kann die Bauplan-Angaben direkt in die '
                          'Missionstexte des Spiels schreiben: welche Baupläne '
                          'ein Auftrag ausschüttet, mit Kästchen für die, die du '
                          'schon hast.',
                          'The watcher can write blueprint details straight into '
                          'the game\'s mission texts: which blueprints a contract '
                          'awards, with a tick box for the ones you already have.'),
    'inj_wie':           ('Dafür wird die Textdatei des Spiels verändert '
                          '(global.ini). Am Spiel selbst ändert sich sonst '
                          'nichts, und der Schritt lässt sich jederzeit '
                          'zurücknehmen.',
                          'This modifies the game\'s text file (global.ini). '
                          'Nothing else about the game changes, and it can be '
                          'undone at any time.'),
    # ⚠⚠ **Diese drei stehen NICHT direkt in einem `t(...)`-Aufruf.** Sie werden
    # im Assistenten und auf der Einstellungsseite über eine Schleifenvariable
    # geholt (`for schluessel, quelle in (('inj_quelle_de', …), …): t(schluessel)`).
    # Wer nach `t('inj_quelle_de')` sucht, findet nichts und hält sie für tot —
    # genau so sind sie am 26.08.2026 beim Aufräumen mitgegangen. Im Setup
    # standen danach acht Tage lang die nackten Schlüsselnamen als Knopfbeschriftung
    # (Schritt 4 von 5). Gemeldet von Haldjas, 03.09.2026: „setup ist ein klein
    # wenig kaputt". Prüfung 49 wacht seitdem auch über Schleifenvariablen.
    'inj_quelle_de':     ('Deutsch — Übersetzung von rjcncpt laden',
                          'German — fetch the rjcncpt translation'),
    'inj_quelle_ss':     ('Englisch — StarStrings von MrKraken laden',
                          'English — fetch StarStrings by MrKraken'),
    'inj_quelle_orig':   ('Englisch — Originaltexte aus dem Spiel',
                          'English — original texts from the game'),
    'inj_fremd':         ('Übersetzung und StarStrings sind fremde Projekte. Sie '
                          'werden beim Klick von deren eigener Adresse geladen, '
                          'nicht mitgeliefert.',
                          'The translation and StarStrings are separate projects. '
                          'They are fetched from their own pages on click, not '
                          'bundled with this tool.'),
    'inj_laeuft':        ('wird eingerichtet …', 'setting up …'),
    'inj_fehler':        ('Hat nicht geklappt: %s', 'Did not work: %s'),
    # ⚠ „Wirkt beim nächsten Spielstart" gehört an diese Stelle. Star Citizen
    # liest die Textdatei **einmal beim Hochfahren** — wer das Spiel offen hat,
    # sieht nach dem Einspielen nichts und hält es für kaputt. Morkhan am
    # 28.08.2026 genau so: „das is immer noch [da]" — er hatte das Spiel nie
    # neu gestartet.
    'inj_aktiv':         ('Bauplan-Angaben sind eingetragen (%d Stellen) — wirkt beim nächsten Spielstart',
                          'Blueprint notes are in place (%d spots) — takes effect the next time the game starts'),
    'inj_steht':         ('Bauplan-Angaben sind eingetragen',
                          'Blueprint notes are in place'),
    'inj_steht_nicht':   ('Bauplan-Angaben sind nicht eingetragen',
                          'Blueprint notes are not in place'),
    'inj_entfernen':     ('Angaben wieder entfernen', 'Remove the notes again'),
    'inj_erneuern':      ('Angaben auffrischen', 'Refresh the notes'),
    'inj_update_da':     ('Neue Version verfügbar: %s', 'New version available: %s'),
    'inj_aktuell':       ('Ist auf dem neuesten Stand', 'Up to date'),
    'inj_pruefen':       ('Auf Updates prüfen', 'Check for updates'),
    'texte_erneuert':    ('Übersetzung aktualisiert (%s)',
                          'Translation updated (%s)'),
    'bpdaten_erneuert':  ('Neue Bauplan-Daten (%s)',
                          'New blueprint data (%s)'),

    # -- Bereiche (Obergruppen der Kategorien) --
    'gruppe_schiff':     ('Schiffsteile', 'Ship parts'),
    'gruppe_fps':        ('FPS-Waffen', 'FPS weapons'),
    'gruppe_ruestung':   ('Rüstung & Kleidung', 'Armor & clothing'),
    'gruppe_sonstiges':  ('Sonstiges', 'Other'),
    'gesucht_wurde_hier': ('Gesucht wurde hier:', 'Searched here:'),

    # -- Bauplan-Arten (kommen als Rohbegriffe von scmdb) --
    'art_Char_Armor_Helmet':    ('Helm', 'Helmet'),
    'art_Char_Armor_Torso':     ('Rüstung (Torso)', 'Armor (torso)'),
    'art_Char_Armor_Legs':      ('Rüstung (Beine)', 'Armor (legs)'),
    'art_Char_Armor_Arms':      ('Rüstung (Arme)', 'Armor (arms)'),
    'art_Char_Armor_Backpack':  ('Rucksack', 'Backpack'),
    'art_Char_Armor_Undersuit': ('Unteranzug', 'Undersuit'),
    'art_QuantumDrive':         ('Quantum Drive', 'Quantum Drive'),
    'art_PowerPlant':           ('Power Plant', 'Power Plant'),
    'art_WeaponGun':            ('Schiffswaffe', 'Ship weapon'),
    'art_WeaponPersonal':       ('FPS-Waffe', 'FPS weapon'),
    'art_WeaponMining':         ('Mining-Laser', 'Mining laser'),
    # ⚠ Heißt „Magazin", nicht „Waffenaufsatz": Alle 32 Einträge dieser Art
    # tragen den Subtyp „Magazine", etwas anderes steckt nicht darin. Die
    # beiden Start-Magazine (Art `ammo`) werden über `katalog.ART_ZUSAMMEN`
    # hier eingereiht, damit alle 34 an einer Stelle stehen.
    'art_WeaponAttachment':     ('Magazin', 'Magazine'),
    'art_SalvageModifier':      ('Salvage-Modifikator', 'Salvage modifier'),
    'art_SalvageHead':          ('Salvage-Kopf', 'Salvage head'),
    'art_TractorBeam':          ('Traktorstrahl', 'Tractor beam'),
    'art_DockingCollar':        ('Andockkragen', 'Docking collar'),
    'art_Cooler':               ('Cooler', 'Cooler'),
    'art_Shield':               ('Schild', 'Shield'),
    'art_Radar':                ('Radar', 'Radar'),
    'art_Misc':                 ('Sonstiges', 'Other'),
    # scmdb führt einige Baupläne unter kleingeschriebenen Sammelbegriffen.
    # Ohne diese drei Zeilen stünde in der Liste wörtlich „weapons".
    'art_weapons':              ('Handfeuerwaffe', 'Personal weapon'),
    'art_ammo':                 ('Magazin', 'Magazine'),
    'art_armour':               ('Anzug', 'Suit'),
    'art_Cargo':                ('Frachtmodul', 'Cargo module'),
    'art_Char_Clothing_Torso_0': ('Kleidung (Oberkörper)', 'Clothing (torso)'),
    'art_Char_Clothing_Torso_1': ('Kleidung (Jacke)', 'Clothing (jacket)'),
    'art_Char_Clothing_Legs':   ('Kleidung (Beine)', 'Clothing (legs)'),
    'art_Char_Clothing_Feet':   ('Kleidung (Schuhe)', 'Clothing (shoes)'),
    'art_unbekannt':            ('Sonstiges', 'Other'),
}

_aktuell = [None]


def systemsprache():
    """Was das Betriebssystem sagt. Alles außer Deutsch gilt als Englisch."""
    for quelle in (os.environ.get('SC_BP_SPRACHE'),
                   os.environ.get('LANG'), os.environ.get('LC_ALL')):
        if quelle:
            return 'de' if quelle.lower().startswith('de') else 'en'
    try:
        kennung = locale.getdefaultlocale()[0] or ''
    except Exception:
        kennung = ''
    return 'de' if kennung.lower().startswith('de') else 'en'


def gewaehlt():
    """Was der Nutzer eingestellt hat: 'de', 'en' oder 'auto'."""
    wert = (pfade.einstellungen().get('sprache') or 'auto').strip().lower()
    return wert if wert in SPRACHEN + ('auto',) else 'auto'


def aktuelle():
    """Die Sprache, in der gerade geschrieben wird."""
    if _aktuell[0] is None:
        wahl = gewaehlt()
        _aktuell[0] = systemsprache() if wahl == 'auto' else wahl
    return _aktuell[0]


_zuhoerer = []


def anmelden(rueckruf):
    """Beim Sprachwechsel benachrichtigt werden.

    ⚠ Ein Fenster, das seine Texte **einmal** beim Bauen setzt, bleibt auf der
    alten Sprache stehen — es merkt vom Umschalten nichts. Das Einstellungs-
    fenster beschriftet sich selbst neu, das Overlay konnte das nicht: Wer auf
    Englisch stellte, hatte danach ein englisches Hauptfenster und eine
    deutsche Melde-Leiste. Wer hier anmeldet, wird mitgezogen.

    Dasselbe Muster wie `autostart.anzeige_anmelden()`."""
    if rueckruf not in _zuhoerer:
        _zuhoerer.append(rueckruf)


# Die Knopfbeschriftungen der System-Abfragen (`messagebox.askyesno`) kommen
# nicht aus dieser Datei, sondern aus Tks eigener Sprachtabelle `msgcat`.
#
# ⚠ Und die ist unvollständig: Auf Linux stand die Tk-Sprache bereits richtig
# auf `de_de`, die deutschen Texte fehlten der Installation aber schlicht —
# gemessen am 28.08.2026, `::msgcat::mc Yes` gab „Yes“ zurück. Ergebnis war
# eine Abfrage mit deutschem Text und den Knöpfen **Yes / No**, gefunden von
# der Autor beim Umstellen der Textquelle. Unter Windows fällt es nicht auf,
# weil Tk die Texte dort mitbringt.
#
# Also tragen wir sie selbst ein. Nur für Deutsch — im englischen Betrieb sind
# „Yes/No“ ja richtig.
_MSGCAT_DE = (('Yes', 'Ja'), ('No', 'Nein'), ('Cancel', 'Abbrechen'),
              ('OK', 'OK'), ('Retry', 'Wiederholen'), ('Abort', 'Abbrechen'),
              ('Ignore', 'Ignorieren'))


_msgcat_widget = [None]


def knoepfe_eindeutschen(widget):
    """Tks Abfrage-Knöpfe auf die Programmsprache bringen.

    Braucht ein beliebiges Tk-Widget (für den Zugang zum Interpreter) und
    wirkt auf alle späteren `messagebox`-Abfragen. Nach jedem Sprachwechsel
    erneut aufrufen.
    """
    if widget is None:
        return
    _msgcat_widget[0] = widget
    try:
        if aktuelle() == 'de':
            for schluessel, wort in _MSGCAT_DE:
                widget.tk.call('::msgcat::mcset', 'de', schluessel, wort)
            widget.tk.call('::msgcat::mclocale', 'de')
        else:
            widget.tk.call('::msgcat::mclocale', 'en')
    except Exception:
        # Ein Tk ohne msgcat ist denkbar — dann bleiben die Knöpfe englisch.
        # Das ist ein Schönheitsfehler, kein Grund, das Programm anzuhalten.
        pass


def setzen(sprache):
    """Sprache für diesen Lauf umstellen (ohne die Einstellung zu ändern).

    Das Speichern macht das Einstellungsfenster; hier geht es nur darum, dass
    ein Umschalten sofort sichtbar wird, ohne das Programm neu zu starten."""
    vorher = _aktuell[0]
    if sprache in SPRACHEN:
        _aktuell[0] = sprache
    elif sprache == 'auto':
        _aktuell[0] = systemsprache()
    if _aktuell[0] == vorher:
        return
    # ⚠ Auch die Knöpfe der System-Abfragen mitziehen — sie haengen an Tks
    # eigener Tabelle und wuerden sonst in der vorigen Sprache stehen bleiben.
    knoepfe_eindeutschen(_msgcat_widget[0])
    for rueckruf in list(_zuhoerer):
        try:
            rueckruf()
        except Exception as ausnahme:
            # Ein Fenster, das sich nicht neu beschriften lässt, darf die
            # anderen nicht mitreißen — und stumm verschwinden soll es auch
            # nicht.
            from . import fehler                # lokal: sonst Zirkelbezug
            fehler.merken('sprache.setzen', ausnahme)


# ---------------------------------------------------------------------------
# Die Eigenschaften aus den Rezeptdaten
# ---------------------------------------------------------------------------
#
# ⚠⚠ **Am 31.08.2026 gemeldet:** „die Beschreibung, welche Werte sich ändern,
# ist bei deutscher Einstellung englisch — einige können kein Englisch und
# verstehen das nun nicht, und melden, es würde ihnen nicht helfen." Genau der
# Zweck der Seite ging damit verloren: Wer nicht weiß, was „Damage Mitigation"
# heißt, liest eine Zahl ohne Bedeutung.
#
# ⚠ **Gehen über den sprachneutralen `propertyKey`**, nicht über den englischen
# Namen — sonst fällt beim nächsten Patch die Hälfte still auf Englisch zurück.
#
# ⚠ **Was fehlt, bleibt englisch.** Kommt mit einem Patch eine neue Eigenschaft
# dazu, steht sie so da, wie das Spiel sie nennt — immer noch besser als eine
# geratene Übersetzung. Gemessen am Datenstand 4.10.0-live.12519617: 24
# verschiedene Eigenschaften, alle 24 hier drin.
EIGENSCHAFTEN = {
    'armor_temperaturemin':            'Minimaltemperatur',
    'armor_temperaturemax':            'Maximaltemperatur',
    'armor_damagemitigation':          'Schadensminderung',
    'armor_radiationdissipation':      'Strahlungsabbau',
    'health_maxhealth':                'Integrität',
    'shield_maxhealth':                'Schildstärke',
    'itemresource_powergeneration':    'Energiestufen',
    'itemresource_coolantgeneration':  'Kühlleistung',
    'weapon_damage':                   'Aufprallwucht',
    'weapon_firerate':                 'Feuerrate',
    'weapon_recoil_smoothness':        'Rückstoß — Gleichmäßigkeit',
    'weapon_recoil_handling':          'Rückstoß — Beherrschbarkeit',
    'weapon_recoil_kick':              'Rückstoß — Stärke',
    'radar_minaimassistdistance':      'Zielhilfe ab',
    'radar_maxaimassistdistance':      'Zielhilfe bis',
    'quantum_speed':                   'Quantengeschwindigkeit',
    'quantum_fuelrequirement':         'Quantentreibstoff-Verbrauch',
    'weapon_tractor_fullstrengthdist': 'Volle Kraft bis',
    'weapon_tractor_maxdist':          'Reichweite',
    'weapon_tractor_force':            'Strahlkraft',
    'weapon_tractor_maxvolume':        'Größtes Volumen',
    'weapon_hullscraping_radius':      'Schürfradius',
    'weapon_hullscraping_speed':       'Schürfgeschwindigkeit',
    'weapon_hullscraping_efficiency':  'Ausbeute',
}


def eigenschaft(name, schluessel=None):
    """Der Name einer Rezept-Eigenschaft in der eingestellten Sprache.

    ⚠ Englisch bleibt englisch — dort ist der Name des Spiels der richtige.
    Übersetzt wird nur ins Deutsche, und nur was in der Tabelle steht.
    """
    try:
        if aktuelle() != 'de':
            return name
    except Exception:
        return name
    return EIGENSCHAFTEN.get(schluessel) or name


def t(schluessel, *werte):
    """Ein Text in der aktuellen Sprache, wahlweise mit eingesetzten Werten."""
    eintrag = TEXTE.get(schluessel)
    if not eintrag:
        return schluessel                       # fehlt: fällt auf, stürzt nicht ab
    text = eintrag[SPRACHEN.index(aktuelle())] or eintrag[0]
    return (text % werte) if werte else text


class Satz:
    """Ein Text, der erst **beim Anzeigen** in Sprache gegossen wird.

    ⚠ Der Unterschied zu `t()`: `t()` liefert einen fertigen Satz — wer den in
    ein Label schreibt, hat die Sprache von damals eingefroren. Stellt jemand
    später um, bleibt die Zeile stehen wie sie war. Genau so hatte am
    26.08.2026 jemand ein englisches Fenster mit einer deutschen Meldung
    „Keine Log-Sicherungen gefunden" darin.

    Ein `Satz` merkt sich stattdessen **Schlüssel und Werte** und setzt sich
    bei jedem `str(...)` neu zusammen. Wer ihn wegschreibt, kann ihn beim
    Sprachwechsel einfach noch einmal auswerten.

    Für den Empfänger ändert sich nichts: `str(satz)`, `'%s' % satz` und
    `print(satz)` liefern den Satz wie vorher.

        Satz('m_keine_logs')
        Satz('m_erster_lauf', Zeitpunkt(aeltester))
    """

    def __init__(self, schluessel, *werte):
        self.schluessel = schluessel
        self.werte = werte

    def __str__(self):
        # Werte können selbst Träger sein (ein Zeitpunkt, ein zweiter Satz) —
        # die müssen in derselben Sprache aufgelöst werden, nicht in der von
        # vorhin.
        werte = tuple(str(w) if isinstance(w, (Satz, Zeitpunkt, Kette)) else w
                      for w in self.werte)
        return t(self.schluessel, *werte)

    def __repr__(self):
        return 'Satz(%r%s)' % (self.schluessel,
                               ''.join(', %r' % w for w in self.werte))

    def __eq__(self, andere):
        # Damit sich ein Träger mit dem vergleichen lässt, was im Label steht.
        return str(self) == str(andere)

    def __hash__(self):
        return hash((self.schluessel, self.werte))


class Zeitpunkt:
    """Ein Datum, das seine **Schreibweise** erst beim Anzeigen wählt.

    ⚠ Nicht nur der Satz ist sprachabhängig, das Datum darin auch: Im
    Englischen steht das Jahr vorn (`m_erster_datum`). Ein fertig formatiertes
    Datum in einem übersetzten Satz liest sich falsch — deshalb wandert hier
    der rohe Zeitstempel weiter, nicht die fertige Zeichenkette."""

    def __init__(self, zeitstempel, schluessel='m_erster_datum'):
        self.zeitstempel = zeitstempel
        self.schluessel = schluessel

    def __str__(self):
        return time.strftime(t(self.schluessel),
                             time.localtime(self.zeitstempel))

    def __repr__(self):
        return 'Zeitpunkt(%r)' % (self.zeitstempel,)


class Kette:
    """Mehrere Träger hintereinander, mit einem Trennzeichen dazwischen.

    Für die seltenen Fälle, in denen zwei eigenständige Sätze eine Zeile
    bilden („Version 3.0.0 verfügbar — Was ist neu"). Bewusst kein eigener
    Sprachschlüssel: Das Trennzeichen ist Satzzeichen, kein Text."""

    def __init__(self, trenner, *teile):
        self.trenner = trenner
        self.teile = teile

    def __str__(self):
        return self.trenner.join(str(teil) for teil in self.teile)

    def __repr__(self):
        return 'Kette(%r, %s)' % (self.trenner,
                                  ', '.join(repr(x) for x in self.teile))


def verbinden(trenner, *teile):
    """Kurzschreibweise für `Kette`."""
    return Kette(trenner, *teile)


def auffrischbar(wert):
    """Ist das ein Träger, der sich beim Sprachwechsel neu auswerten lässt?

    Die Oberfläche fragt damit ab, ob eine bereits angezeigte Meldung
    mitgezogen werden kann — oder ob dort ein fertiger Text steht, den man
    besser stehen lässt, statt ihn zu erraten."""
    return isinstance(wert, (Satz, Zeitpunkt, Kette))


def art(roh):
    """Rohbegriff von scmdb -> Bezeichnung in der aktuellen Sprache."""
    return t('art_%s' % roh) if ('art_%s' % roh) in TEXTE else (roh or t('art_unbekannt'))


if __name__ == '__main__':
    print('Systemsprache:', systemsprache(), '· eingestellt:', gewaehlt(),
          '· aktiv:', aktuelle())
    luecken = [k for k, v in TEXTE.items() if len(v) != 2 or not all(v)]
    print('Einträge:', len(TEXTE), '· unvollständig:', len(luecken))
    for k in luecken:
        print('   fehlt:', k)
    for s in SPRACHEN:
        setzen(s)
        print('\n[%s] %s | %s | %s' % (s, t('bauplaene'), t('filter_fehlt'),
                                       t('von_gesamt', 3, 714, 0)))


def fenstertitel(text):
    """Der Fenstertitel, bei der Testfassung mit Warnhinweis.

    Gesetzt wird das ueber die Umgebungsvariable `SC_BP_TESTFASSUNG` — das tut
    `tools/testfassung_starten.sh`. Laeuft die normale Fassung, kommt der Text
    unveraendert zurueck.
    """
    import os
    if os.environ.get('SC_BP_TESTFASSUNG', '') not in ('', '0'):
        return '%s   %s' % (text, t('s_testfassung'))
    return text
