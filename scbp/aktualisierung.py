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
Neue Versionen bemerken, nachlesen und holen.

Niemand geht regelmäßig auf GitHub nachsehen, ob es etwas Neues gibt. Also
schaut das Programm selbst nach, sagt Bescheid und holt die neue Version auf
Knopfdruck. Und weil „es gibt eine neue Version" allein nichts wert ist, kann
man **nachlesen, was sich geändert hat** — auch bei älteren Versionen.

Drei Teile:

  **Nachsehen.** Einmal am Tag gegen die GitHub-API, im Hintergrund. Ohne Netz
  passiert nichts und es wird auch nichts gemeldet.
  **Nachlesen.** Das Änderungsprotokoll liegt als `CHANGELOG.en.md` bei; neuere
  Einträge kommen aus den Release-Texten. Beides zusammen ergibt die Historie.
  **Holen.** Die zur eigenen Verpackung passende Datei (`.exe` oder AppImage)
  wird geladen und ersetzt die laufende.

> ⚠️ **Das Ersetzen ist heikel und je System verschieden.** Unter Windows kann
> sich eine laufende `.exe` nicht selbst überschreiben — die neue Datei wird
> daneben abgelegt und ein winziges Hilfsskript tauscht sie nach dem Beenden.
> Ein AppImage darf sich ersetzen, solange man die Datei austauscht statt in sie
> hineinzuschreiben. Wer aus dem Quellcode startet, bekommt keinen Selbstersatz
> angeboten — dort ist `git pull` der richtige Weg, und alles andere würde
> lokale Änderungen überfahren.

Geladen wird ausschließlich von `github.com`; eine Datei von woanders wird
abgelehnt, selbst wenn die API sie nennen würde.
"""
import json
import errno
import os
import re
import sys
import tempfile
import time
import urllib.request

from . import fehler
from . import pfade

REPO = 'Xharig/SC-BP-Watcher'
API = 'https://api.github.com/repos/%s/releases' % REPO
# ⚠ **Kein `/releases/latest` mehr (28.08.2026).** Der Link führte auf
# **v2.0.0**: GitHub blendet dort Vorabversionen aus, und alle rc-Fassungen
# sind welche. Wer ihn weitergab, schickte Leute auf einen Stand von vor
# Monaten — und bekam prompt Fehler gemeldet, die längst behoben waren.
# Die Übersicht zeigt alles, auch die Vorabversionen.
SEITE = 'https://github.com/%s/releases' % REPO
KENNUNG = 'SC-BP-Watcher (+https://github.com/%s)' % REPO
CACHE = 'versionen.json'
# Wie lange ein Blick auf GitHub gilt. Früher standen hier 24 Stunden — „einmal
# am Tag reicht". Tut es nicht: Wer das Programm mehrmals startet, bekam beim
# zweiten Mal nichts mehr zu sehen, obwohl inzwischen eine neue Version
# vorlag. Gemeldet am 24.08.2026, an einem Tag mit zehn Vorabversionen.
#
# Eine Stunde ist der Kompromiss: Beim Starten wird praktisch immer nachgesehen,
# im Dauerbetrieb bleibt es bei ein paar Abfragen am Tag.
ABSTAND = 3600
AUS = os.environ.get('SC_BP_NO_NET', '') not in ('', '0')
ERLAUBTE_HOSTS = ('github.com', 'objects.githubusercontent.com')


# ------------------------------------------------------------ Versionsvergleich
def _teile(version):
    """'v2.0.1-fork.3' -> (2, 0, 1). Vorspann und Zusatz werden ignoriert."""
    m = re.search(r'(\d+)\.(\d+)\.(\d+)', str(version or ''))
    return tuple(int(x) for x in m.groups()) if m else (0, 0, 0)


def _vorab(version):
    """Trägt die Version einen Vorab-Zusatz (-dev, -rc1, -beta, -alpha)?"""
    return bool(re.search(r'-(rc|beta|alpha|dev)', str(version or '')))


def _vorab_nummer(version):
    """Die Zahl hinter dem Zusatz: aus '2.0.0-rc2' wird 2, aus '-dev' wird 0."""
    m = re.search(r'-(?:rc|beta|alpha|dev)\.?(\d*)', str(version or ''))
    if not m:
        return 0
    return int(m.group(1)) if m.group(1) else 0


def _fassungsschluessel(version):
    """Eindeutig **je Fassung** — `-rc1` und `-rc13` sind nicht dasselbe.

    ⚠⚠ **Nicht `_teile` benutzen, wo Fassungen unterschieden werden müssen.**
    `_teile('v3.15.0-rc13')` ergibt `(3, 15, 0)` — genau wie `-rc1`. Im
    Änderungsverlauf galten dadurch alle dreizehn Testfassungen als dieselbe
    Version, und zwölf davon flogen als Doppelung heraus. Am 05.09.2026
    gemeldet: „Dann müsste man auch jeden rc anzeigen, nicht nur den letzten."

    Die fertige Version steht über ihren Vorabfassungen — deshalb `9999`.
    """
    gross, klein, patch = _teile(version)
    stufe = _vorab_nummer(version) if _vorab(version) else 9999
    return (gross, klein, patch, stufe)


def ist_neuer(fremd, eigen):
    """Ist `fremd` eine höhere Version als `eigen`?

    Ein `-dev`-Zusatz gilt als **älter** als dieselbe Zahl ohne Zusatz: Wer eine
    Entwicklerfassung von 1.6.0 fährt, soll das fertige 1.6.0 angeboten bekommen."""
    a, b = _teile(fremd), _teile(eigen)
    if a != b:
        return a > b
    # Gleiche Zahl: Eine Version mit Zusatz (-dev, -rc1, -beta) gilt als älter
    # als dieselbe Zahl ohne. Wer 2.0.0-rc1 fährt, soll 2.0.0 angeboten bekommen.
    va, vb = _vorab(fremd), _vorab(eigen)
    if va != vb:
        return vb           # nur die fertige Version ist neuer
    if not va:
        return False        # beide fertig und gleiche Zahl
    # Beide Vorabversionen: nach der Nummer dahinter (rc1 < rc2 < rc10)
    return _vorab_nummer(fremd) > _vorab_nummer(eigen)


# ------------------------------------------------------------------- Nachsehen
def _hole(url, zeitlimit=20):
    req = urllib.request.Request(url, headers={
        'User-Agent': KENNUNG, 'Accept': 'application/vnd.github+json'})
    with urllib.request.urlopen(req, timeout=zeitlimit) as r:
        return json.loads(r.read().decode('utf-8'))


def _cache_lesen():
    try:
        with open(pfade.app_datei(CACHE), encoding='utf-8') as f:
            return json.load(f)
    except Exception:
        return {}


def _cache_schreiben(daten):
    try:
        with open(pfade.app_datei(CACHE), 'w', encoding='utf-8') as f:
            json.dump(daten, f, ensure_ascii=False)
    except OSError:
        pass


# Hat der letzte erzwungene Blick zu GitHub geklappt? `None` = noch nicht
# versucht, `False` = Abruf gescheitert (Netz weg, Grenze erreicht).
_ABRUF = {'ok': None, 'grenze': False}


def abruf_geglueckt():
    """Hat der letzte Blick zu GitHub wirklich stattgefunden?

    ⚠ **Ohne das kann „nichts Neues" zweierlei heißen** — und die zwei sind das
    Gegenteil voneinander: entweder „du bist aktuell" oder „ich konnte gar nicht
    nachsehen". Der Prüfknopf meldete bisher in beiden Fällen Entwarnung.

    Aufgefallen am 27.08.2026: Bomb20 drückte „Auf Aktualität prüfen", bekam „du
    hast die neueste rc67" — und rc68 war seit zwei Minuten draußen. GitHub
    erlaubt anonym nur **60 Abfragen pro Stunde und Adresse**; wer an einem
    Vormittag viel klickt, läuft dagegen. Der Abruf scheiterte, der Code fing das
    still ab und rechnete mit dem alten Stand weiter.

    Ein Prüfknopf, der fälschlich Entwarnung gibt, ist schlimmer als keiner.
    """
    return _ABRUF['ok']


def grenze_erreicht():
    """War der letzte Fehlschlag die Stundengrenze von GitHub?"""
    return _ABRUF['grenze']


def nachsehen(eigene_version, erzwingen=False):
    """Gibt es etwas Neues? Rückgabe: dict mit Angaben oder None.

    Gefragt wird höchstens einmal je `ABSTAND` (eine Stunde); dazwischen gilt
    der gemerkte Stand.

    ⚠ Der Abstand allein macht noch keine Wiederholung: Bis v3.0.1 rief diese
    Funktion **niemand** ein zweites Mal, und ein laufender Watcher erfuhr nie
    von einer neuen Fassung. Wer den Takt ändert, ändert ihn an **zwei** Stellen
    — hier und in `Overlay.VERSION_TAKT`.
    Fehler sind kein Drama — ohne Netz meldet sich das Programm einfach nicht."""
    zwischen = _cache_lesen()
    # `SC_BP_NO_NET` verbietet das **Abfragen**, nicht das Wissen: Was schon
    # bekannt ist, darf weiter gemeldet werden — das ist keine Netzverbindung.
    alt_genug = time.time() - zwischen.get('geprueft', 0) > ABSTAND
    if not AUS and (erzwingen or alt_genug):
        try:
            # ⚠ **20 reicht längst nicht.** Bei 83 Freigaben und einer
            # Testversion nach der anderen war unter den letzten 20 **keine
            # einzige stabile** — `neueste(False)` fand nichts, und im Kasten
            # „Stabile Version" stand statt eines Knopfes „Erst oben auf ‚Jetzt
            # nachsehen' drücken". Eine Sackgasse: Wer die stabile Version wollte,
            # sah keinen Weg, sondern eine Hausaufgabe. Gemessen am 27.08.2026:
            # 20 Freigaben → 0 stabile, 100 Freigaben → 3.
            #
            # 100 ist das Höchste, was GitHub in einer Abfrage hergibt, und es
            # bleibt **eine** Abfrage — die Stundengrenze zählt Anfragen, nicht
            # Einträge.
            freigaben = _hole(API + '?per_page=100')
            zwischen = {
                'geprueft': time.time(),
                'freigaben': [{
                    'version': f.get('tag_name'),
                    'name': f.get('name'),
                    'datum': (f.get('published_at') or '')[:10],
                    'text': f.get('body') or '',
                    'vorab': bool(f.get('prerelease')),
                    'dateien': [{'name': a.get('name'), 'url':
                                 a.get('browser_download_url'),
                                 'groesse': a.get('size')}
                                for a in (f.get('assets') or [])],
                } for f in freigaben if not f.get('draft')],
            }
            _cache_schreiben(zwischen)
            _ABRUF['ok'] = True
            _ABRUF['grenze'] = False
        except Exception as ausnahme:
            # ⚠ Nicht mehr stillschweigend: Ob der Blick stattgefunden hat, ist
            # eine andere Auskunft als „es gibt nichts Neues". Siehe
            # `abruf_geglueckt()`.
            _ABRUF['ok'] = False
            _ABRUF['grenze'] = '403' in str(ausnahme) or 'rate limit' in str(
                ausnahme).lower()
            fehler.merken('aktualisierung.nachsehen', ausnahme)
            # Der letzte bekannte Stand gilt weiter — ohne Netz ist das besser
            # als gar nichts.

    # Vorabversionen bekommt **niemand ungefragt** angeboten — eine Vorabfassung
    # ist zum Prüfen da, nicht zum Verteilen. Angeboten werden sie in drei Fällen:
    #
    #   1. Der Spieler hat es in den Einstellungen ausdrücklich verlangt
    #      (`vorabversionen`, Standard aus). Das ist der Testkanal: Wer mithelfen
    #      will, bekommt die Versionen vor allen anderen — wer Ruhe will, merkt
    #      von ihnen nichts und bleibt auf den fertigen Versionen.
    #   2. Er fährt selbst schon eine Vorabfassung; dann wäre es unsinnig, ihm
    #      die nächste zu verschweigen.
    #   3. Die fertige Version zur selben Nummer erscheint — die ist ohnehin
    #      "neuer" als jede Vorabfassung (siehe `ist_neuer`), also endet der
    #      Testkanal nie in einer Sackgasse.
    eigene_ist_vorab = (_vorab(eigene_version)
                        or pfade.einstellung_wahrheit('vorabversionen', False))
    # ⚠ **Nicht** den ersten Treffer nehmen, sondern den höchsten.
    # GitHub gibt die Freigaben nach Erstellungszeit des Tags zurück, nicht nach
    # Versionsnummer — und das ist nicht dasselbe: In der Liste stand `rc10`
    # hinter `rc9`, weshalb Nutzern die **vorletzte** Version als „neu" gemeldet
    # wurde. Gemeldet am 24.08.2026.
    bester = None
    for f in zwischen.get('freigaben') or []:
        if not f.get('version'):
            continue
        if f.get('vorab') and not eigene_ist_vorab:
            continue
        if not ist_neuer(f['version'], eigene_version):
            continue
        if bester is None or ist_neuer(f['version'], bester['version']):
            bester = f
    return bester


def neueste(mit_vorab):
    """Die neueste bekannte Freigabe eines Kanals — unabhängig von der eigenen Version.

    ⚠ Nicht dasselbe wie `nachsehen()`. Das meldet nur, was **neuer** ist als die
    laufende Version — richtig für eine Update-Meldung, unbrauchbar für einen
    Knopf „hol mir die letzte fertige Version". Wer eine Testfassung fährt, will
    ja gerade zurück auf die fertige können.
    """
    bester = None
    for f in freigaben():
        if not f.get('version'):
            continue
        if f.get('vorab') and not mit_vorab:
            continue
        if bester is None or ist_neuer(f['version'], bester['version']):
            bester = f
    return bester


def freigaben():
    """Alle bekannten Freigaben, neueste zuerst — für das Änderungsprotokoll."""
    return _cache_lesen().get('freigaben') or []


# --------------------------------------------------------- Änderungsprotokoll
def _changelog_datei():
    """Die mitgelieferte Änderungsliste in der Sprache des Nutzers finden.

    Es gibt zwei: `CHANGELOG.en.md` (englisch) und `CHANGELOG.md` (deutsch).
    Wer die Oberfläche auf Deutsch stehen hat, soll auch die Einträge auf
    Deutsch lesen — sonst wäre die Zweisprachigkeit an der Stelle nur behauptet.
    Fehlt die eigene Sprache, gilt die andere: eine fremdsprachige Auskunft ist
    besser als gar keine."""
    from . import sprache
    namen = (['CHANGELOG.md', 'CHANGELOG.en.md'] if sprache.aktuelle() == 'de'
             else ['CHANGELOG.en.md', 'CHANGELOG.md'])
    ordner = []
    if getattr(sys, 'frozen', False):        # PyInstaller legt Beigaben hierhin
        ordner.append(getattr(sys, '_MEIPASS', ''))
        ordner.append(os.path.dirname(sys.executable))
    ordner.append(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    for name in namen:
        for ordner_p in ordner:
            if not ordner_p:
                continue
            voll = os.path.join(ordner_p, name)
            if os.path.isfile(voll):
                return voll
    return None


# Welche Überschrift im CHANGELOG bedeutet was. Die Zuordnung ist bewusst
# großzügig — englische wie deutsche Version, und wer eine neue Überschrift
# erfindet, landet unter „neu" statt im Nichts.
_ARTEN = (
    ('fix',  ('behoben', 'fixed', 'korrigiert', 'bugfix')),
    ('bess', ('geändert', 'changed', 'verbessert', 'improved', 'entfernt',
              'removed', 'bedienung')),
    ('neu',  ('hinzugefügt', 'added', 'neu', 'new')),
)


def einleitung(text):
    """Der Satz, mit dem eine Version vorgestellt wird — das Zitat im Changelog.

    Im Markdown steht er als `> …`-Block über den Aufzählungen: „Ein Fenster für
    alles." Er sagt in einem Satz, worum es in der Version ging, und war bisher
    nirgends zu sehen — `punkte_nach_art` wirft alles weg, was keine Aufzählung
    ist. Genau dieser Satz gehört aber unter die Version, wenn man sie aufklappt.
    """
    zeilen = []
    for zeile in (text or '').split('\n'):
        blank = zeile.strip()
        if blank.startswith('>'):
            zeilen.append(blank.lstrip('>').strip())
        elif zeilen and not blank:
            break                      # der Block ist zu Ende
        elif zeilen:
            break
    satz = ' '.join(z for z in zeilen if z)
    # Die Auszeichnungen sind im Fenster nur Zeichen — sie stören mehr, als sie
    # helfen.
    return satz.replace('**', '').replace('`', '').strip()


def punkte_nach_art(text):
    """Zerlegt einen Änderungstext in (art, zeile) — für Filter und Marken.

    Der Text ist Markdown mit Zwischenüberschriften (`### Behoben`). Alles
    darunter gilt als diese Art, bis die nächste Überschrift kommt. Ohne
    Überschrift gilt „neu": Lieber falsch einsortiert als unsichtbar.
    """
    art = 'neu'
    heraus = []
    im_block = False
    for zeile in (text or '').split('\n'):
        # ⚠ Eingezäunte Codeblöcke (```) überspringen. Sie zeigen im
        # Änderungsprotokoll, wie etwas auf dem Bildschirm aussieht — als
        # Fortsetzungszeile an einen Punkt geklebt ergibt das Kauderwelsch,
        # und die Zaunzeichen selbst standen bis rc42 sichtbar im Fenster.
        if zeile.strip().startswith('```'):
            im_block = not im_block
            continue
        if im_block:
            continue
        # ⚠ Die Einrückung muss VOR dem Abschneiden geprüft werden — sonst sind
        # Unterpunkte nicht mehr von Hauptpunkten zu unterscheiden.
        eingerueckt = zeile[:1].isspace()
        blank = zeile.strip()
        if blank.startswith('#'):
            klein = blank.lstrip('#').strip().lower()
            for kennung, woerter in _ARTEN:
                if any(w in klein for w in woerter):
                    art = kennung
                    break
            continue
        if not eingerueckt and blank.startswith(('- ', '* ')):
            heraus.append([art, blank[2:].strip()])
        elif eingerueckt and blank and not blank.startswith(('- ', '* ')) and heraus:
            # Eine eingerückte Zeile **ohne** Aufzählungszeichen ist die
            # Fortsetzung des Punktes darüber — im Markdown umgebrochen, im
            # Fenster gehört sie an denselben Satz. Wer sie verwirft, zeigt
            # abgeschnittene Sätze („… ganz unten") und merkt es nicht, weil
            # es wie ein Zeilenumbruch aussieht.
            heraus[-1][1] = (heraus[-1][1] + ' ' + blank).strip()
    return [(a, z) for a, z in heraus]


def protokoll():
    """Die Versionsgeschichte als Liste, neueste zuerst.

    Zusammengesetzt aus zwei Quellen: der **mitgelieferten** `CHANGELOG.md`
    bzw. `CHANGELOG.en.md` — je nach eingestellter Sprache — und den Release-Texten
    von GitHub, die auch Versionen kennen, die neuer sind als die eigene.

    ⚠ Der mitgelieferte Changelog hat Vorrang, **weil nur er die Sprache kennt**.
    Der Release-Text auf GitHub ist bewusst zweisprachig aufgebaut: Englisch oben,
    Deutsch in einem aufklappbaren Block darunter. Auf der Release-Seite ist das
    richtig — im Fenster wurde daraus eine englische Liste für jemanden, der die
    Oberfläche auf Deutsch stehen hat. Genau so gemeldet.

    GitHub springt nur dort ein, wo der Changelog nichts hat: bei Versionen, die
    neuer sind als die eigene.
    """
    eintraege, gesehen = [], {}
    datei = _changelog_datei()
    if datei:
        try:
            with open(datei, encoding='utf-8') as f:
                roh = f.read()
        except OSError:
            roh = ''
        for block in re.split(r'^## ', roh, flags=re.M)[1:]:
            kopf, _, rest = block.partition('\n')
            version = kopf.split('—')[0].split(' - ')[0].strip()
            if _teile(version) == (0, 0, 0):
                continue        # „Unveröffentlicht" ist nichts für Nutzer
            # ⚠ Je **Fassung**, nicht je Versionsnummer — sonst gilt rc1 als
            # dasselbe wie rc13. Siehe `_fassungsschluessel`.
            schluessel = _fassungsschluessel(version)
            datum = ''
            m = re.search(r'(\d{4}-\d{2}-\d{2})', kopf)
            if m:
                datum = m.group(1)
            if schluessel in gesehen:
                continue
            eintrag = {'version': version, 'datum': datum,
                       'text': rest.strip(), 'quelle': 'changelog'}
            gesehen[schluessel] = eintrag
            eintraege.append(eintrag)

    # Und nun alles, was der mitgelieferte Changelog noch nicht kennt — das sind
    # die Versionen, die nach dieser hier erschienen sind.
    for f in freigaben():
        schluessel = _fassungsschluessel(f.get('version'))
        if schluessel in gesehen or not f.get('version'):
            continue
        eintrag = {'version': f.get('version'),
                   'datum': f.get('datum') or '',
                   'text': (f.get('text') or '').strip(),
                   'quelle': 'github'}
        gesehen[schluessel] = eintrag
        eintraege.append(eintrag)

    # ⚠ Nach der **Fassung** sortieren, sonst stünden rc1 bis rc13 in
    # zufälliger Reihenfolge — sie tragen alle dasselbe Datum.
    eintraege.sort(key=lambda e: (_fassungsschluessel(e['version']),
                                  e['datum']), reverse=True)
    return eintraege


def protokoll_gebuendelt():
    """Dasselbe wie `protokoll()`, aber Patch-Versionen unter ihrer Reihe.

    ⭐⭐ **Aus v3.13.0, .1, .2 und .3 wird ein Eintrag „v3.13".** Am 05.09.2026
    gefragt: „Ist es nicht sinnvoller, 3.13.x, 3.14.x zusammenzufassen?" Ja —
    die vier standen alle vom selben Tag untereinander, zusammen neun Punkte.
    Vier Zeilen für das, was eine Sache ist, ist Buchführung, kein
    Änderungsprotokoll; und niemand denkt in „3.13.2", sondern in „3.13".

    ⚠ **Vorabversionen bleiben einzeln.** Wer eine Testfassung fährt, will
    sehen, was **diese** gebracht hat — gebündelt stünden dreizehn rc als ein
    Klumpen da, und die Rückmeldung „was ist seit gestern anders?" wäre nicht
    mehr zu beantworten. Sobald die fertige Version erscheint, verschwinden
    sie ohnehin aus der Liste.

    ⚠ Der Vorspann kommt von der **neuesten** Fassung der Reihe; die Punkte
    aller Fassungen stehen darunter, in derselben Reihenfolge wie bisher.
    """
    alle = protokoll()
    # ⚠⚠ **Eine Testfassung verschwindet, sobald ihre fertige Version da ist.**
    # Sonst stünden 91 rc-Blöcke im Verlauf und verdrängten alles andere. Am
    # 05.09.2026: „Sobald es live ist, kommen die eh weg." Genau so — und
    # solange es die fertige noch nicht gibt, ist jede einzelne sichtbar, denn
    # dann testet gerade jemand und will wissen, was seine Fassung gebracht hat.
    fertig_da = set(_teile(e['version']) for e in alle
                    if not _vorab(e.get('version') or ''))
    raus, nach_reihe = [], {}
    for e in alle:
        version = e.get('version') or ''
        if _vorab(version):
            if _teile(version) in fertig_da:
                continue
            raus.append(e)
            continue
        gross, klein, _patch = _teile(version)
        reihe = (gross, klein)
        vorhanden = nach_reihe.get(reihe)
        if vorhanden is None:
            gebuendelt = dict(e)
            gebuendelt['version'] = 'v%d.%d' % (gross, klein)
            nach_reihe[reihe] = gebuendelt
            raus.append(gebuendelt)
            continue
        # Ältere Fassung derselben Reihe: nur ihre Punkte anhängen. Vorspann
        # und Datum bleiben die der neuesten — `protokoll()` liefert absteigend.
        vorhanden['text'] = (vorhanden.get('text') or '') + '\n\n' \
            + (e.get('text') or '')
    return raus


# ------------------------------------------------------------------- Holen
def eigenes_appimage():
    """Der Pfad **unseres** AppImage — oder None.

    ⚠ `APPIMAGE` allein genügt nicht. Die Variable steht in der Umgebung
    **jedes** Programms, das aus einem AppImage heraus gestartet wurde — auch in
    einem Terminal, das man daraus öffnet, und in allem, was von dort aus läuft.
    Wer nur auf sie schaut, hält jedes beliebige Programm für sich selbst.

    Das ist am 25.08.2026 teuer geworden: Ein Testlauf des Selbst-Updates lief in
    einer Umgebung, in der `APPIMAGE` auf eine **fremde** Anwendung zeigte — und
    das Update hat prompt diese fremde Datei überschrieben (234 MB durch 12 MB
    ersetzt). Zurückzuholen war sie nur, weil das fremde Programm noch lief und
    die alte Inode über `/proc/<pid>/exe` offen hielt.

    Verlässlich ist erst der zweite Teil: Zu einem AppImage gehört `APPDIR`, der
    Ort, an dem es entpackt eingehängt ist. Nur wenn **unser eigener Code** von
    dort kommt, laufen wir wirklich in diesem AppImage.
    """
    pfad = os.environ.get('APPIMAGE')
    if not pfad or not os.path.isfile(pfad):
        return None
    # ⚠ Der erste Anlauf verglich den eigenen Code mit `APPDIR`. Das ging schief:
    # PyInstaller entpackt sich in ein **eigenes** Verzeichnis (`sys._MEIPASS`,
    # etwa `/tmp/_MEIabc123`), nicht in den AppImage-Einhängepunkt. Der Vergleich
    # schlug also **immer** fehl — das Programm hielt sich für eine `.exe`, ging in
    # den Windows-Zweig und meldete „[Errno 2] No such file or directory: 'cmd'".
    #
    # Maßgeblich ist stattdessen der Dateiname: Zeigt `APPIMAGE` auf eine Datei,
    # die nach diesem Programm heißt, ist es unsere. Ein fremdes AppImage — der
    # Unfall, um den es hier geht — heißt anders und fällt durch.
    if 'sc-bp-watcher' not in os.path.basename(pfad).lower():
        return None
    return pfad


def verpackung():
    """Wie läuft dieses Programm gerade? 'exe', 'appimage' oder 'quellcode'."""
    if eigenes_appimage():
        return 'appimage'
    if getattr(sys, 'frozen', False):
        return 'exe'
    return 'quellcode'


# Welcher Anhang unter Windows geholt wird — und warum es der Installer ist.
#
# Seit v3.0.0 hängen nur noch **zwei** Dateien an einer Freigabe:
#
#     SC-BP-Watcher-Setup.exe          der Installer  ← der einzige Windows-Weg
#     SC-BP-Watcher-x86_64.AppImage    Linux
#
# ⚠ Die nackte `SC-BP-Watcher.exe` ist bewusst weg (bewusste Entscheidung,
# 27.08.2026: „ich will die exe ohne install loswerden … sie belastet mich
# nur"). Sie war eine Maßnahme aus der Anfangszeit — ein unsigniertes Programm
# ohne Installer wirkt harmloser, und es ging darum, Vertrauen aufzubauen. Das
# ist erreicht; zwei Auslieferungswege heißen ab jetzt nur noch zwei
# Fehlerquellen und doppelte Unterstützung. „Nun wollen wir es funktionierend
# und einfach."
#
# **Und v2.0.0, die es nur als nackte .exe gab?** Deren Update-Logik nimmt die
# erste Datei auf `.exe` — jetzt also den Installer — und ihr Hilfsskript
# **startet** die getauschte Datei anschließend (`start "" "<ziel>"`). Der
# Installer läuft damit von selbst und richtet alles ordentlich ein. Was früher
# der Fehler war (der Installer landete unter dem Namen des Programms), ist
# damit genau der Weg hinaus.
#
# ⚠ Bis rc39 wurde hier nach der **ersten** Datei auf `.exe` gesucht. GitHub
# liefert sie alphabetisch, ein `-` (0x2D) steht vor einem `.` (0x2E), also kam
# `-Setup.exe` zuerst — und die alte `einspielen()` schob diesen Fund roh über
# die laufende `SC-BP-Watcher.exe`, ohne ihn je auszuführen. Am 26.08.2026 im
# Test bestätigt: geladen wurden 14.812.324 Bytes statt 13.015.189.
#
# Seitdem ist es **Absicht**, den Installer zu holen — er wird gestartet statt
# kopiert. Inno beendet das laufende Programm selbst
# (`CloseApplications=force`), ersetzt die Datei, pflegt den Eintrag in
# „Apps & Features" und startet den Watcher danach wieder. Denselben Weg geht
# der SC-Deutsch-Launcher.
#
# Unter Linux bleibt es beim Tausch des AppImage — dort gibt es keinen
# Installer, und ein laufendes AppImage darf ersetzt werden.
#
# ⚠ `-setup.exe` steht vorn und bleibt: Genau danach suchen die Testfassungen
# rc39–rc75. Wird der Installer je umbenannt, bekommen sie nie wieder ein
# Update angeboten.
WINDOWS_INSTALLER = ('-setup.exe', '-installer.exe', '_setup.exe')


def passende_datei(freigabe, art=None):
    """Die zur eigenen Verpackung passende Datei aus einer Freigabe — oder None."""
    art = art or verpackung()
    if art == 'quellcode':
        return None                      # dort ist `git pull` der richtige Weg
    if art == 'appimage':
        for datei in freigabe.get('dateien') or []:
            name = (datei.get('name') or '').lower()
            if name.endswith('.appimage') and _url_ok(datei.get('url') or ''):
                return datei
        return None
    # Windows: **nur** der Installer. Findet sich keiner, gibt es lieber gar
    # kein Update als das falsche — die nackte .exe wäre hier wertlos, weil
    # niemand mehr da ist, der sie an ihren Platz legt.
    for datei in freigabe.get('dateien') or []:
        name = (datei.get('name') or '').lower()
        if name.endswith(WINDOWS_INSTALLER) and _url_ok(datei.get('url') or ''):
            return datei
    return None


def _url_ok(url):
    """Nur Dateien von GitHub — egal, was die Antwort sonst behauptet."""
    try:
        from urllib.parse import urlparse
        teile = urlparse(url)
        return teile.scheme == 'https' and (
            teile.hostname in ERLAUBTE_HOSTS
            or (teile.hostname or '').endswith('.github.com'))
    except Exception:
        return False


def _ablageort_fuer_update(name):
    """Wohin die neue Version geladen wird.

    **Windows** — in den Temp-Ordner. Geholt wird dort ein Installer, der nur
    einmal gestartet und danach nie wieder gebraucht wird; Windows räumt den
    Ordner von selbst auf. Früher lag er neben dem Programm, und wenn das Update
    scheiterte, blieben dort 14 MB liegen, die niemand zuordnen konnte.

    **Linux** — **neben** das laufende AppImage.

    ⚠ Hier lag ein Fehler, der jedes Selbst-Update unter Linux scheitern ließ:
    Geladen wurde nach `/tmp`, eingespielt mit `os.replace()`. Auf so gut wie
    jedem Linux ist `/tmp` ein eigenes Dateisystem (tmpfs), und `os.replace` kann
    nicht über Dateisystemgrenzen verschieben — es endet mit
    „[Errno 18] Invalid cross-device link". Gemeldet am 25.08.2026 direkt aus dem
    Fenster.

    Nebenbei ist das Einspielen dadurch **atomar**: Innerhalb eines Dateisystems
    ist `os.replace` unteilbar, es gibt keinen Moment, in dem die Datei halb da
    ist. Ist der Zielordner nicht beschreibbar, bleibt `/tmp` als Rückfall — dann
    greift beim Einspielen der Umweg über `shutil.move`.
    """
    if sys.platform.startswith('win'):
        return os.path.join(tempfile.gettempdir(), name or 'update.bin')
    laufende = eigenes_appimage() or sys.executable
    ordner = os.path.dirname(os.path.abspath(laufende))
    if os.access(ordner, os.W_OK):
        return os.path.join(ordner, '.' + (name or 'update.bin') + '.neu')
    return os.path.join(tempfile.gettempdir(), name or 'update.bin')
    return os.path.join(tempfile.gettempdir(), name or 'update.bin')


def herunterladen(datei, fortschritt=None):
    """Lädt die neue Version in eine Nebendatei. Gibt deren Pfad zurück.

    Geladen wird **neben** das laufende Programm, nicht darüber: Bricht die
    Leitung ab, ist die alte Version noch vollständig da."""
    url = datei.get('url')
    if not _url_ok(url):
        # ⚠ Der Text dieser Ausnahme landet über `str(fehler)` sichtbar
        # beim Nutzer (siehe `return False, str(fehler)` weiter unten).
        from . import sprache
        raise ValueError(sprache.t('up_fremde_quelle'))
    ziel = _ablageort_fuer_update(datei.get('name'))
    req = urllib.request.Request(url, headers={'User-Agent': KENNUNG})
    with urllib.request.urlopen(req, timeout=120) as r, open(ziel, 'wb') as f:
        gesamt = int(r.headers.get('Content-Length') or 0)
        geladen = 0
        while True:
            block = r.read(256 * 1024)
            if not block:
                break
            f.write(block)
            geladen += len(block)
            if fortschritt and gesamt:
                # ⚠ **Die Anzeige darf den Download nicht umbringen.** Sie ist
                # Beiwerk, das Herunterladen ist der Zweck — und genau
                # andersherum lief es: Der Rückruf zeichnet ins Fenster, und
                # wenn das aus einem Nebenfaden schiefgeht (`RuntimeError: main
                # thread is not in main loop`), riss die Ausnahme den ganzen
                # Faden mit. Der Nutzer sah: nichts. Kein Fortschritt, kein
                # Update, keine Meldung.
                #
                # Bomb20 am 27.08.2026, dreimal in Folge im Diagnosebericht:
                # „und ich habe auf get 68 geklickt, aber da kam nix mit restart
                # oder install." Es wurde nie etwas geladen.
                try:
                    fortschritt(round(100 * geladen / gesamt))
                except Exception:
                    fortschritt = None      # einmal daneben, nie wieder fragen
    return ziel


# ⚠ Hier stand einmal ein Hilfsskript, das die laufende `.exe` selbst tauschte —
# und das ist am 26.08.2026 im Test auf eine Verklemmung gelaufen, die niemand
# vorhergesehen hatte:
#
#   * Die App beendet sich, aber der PyInstaller-Bootloader lebt weiter: Er
#     räumt seinen Ordner unter `%TEMP%` auf. Zwei Laufzeit-Bibliotheken
#     (`VCRUNTIME140.dll`, `VCRUNTIME140_1.dll`) blieben gesperrt, und er stand
#     im Fenster "Failed to remove temporary directory" still.
#   * Solange dieses Fenster steht, hält der Bootloader die `.exe`.
#   * Das Skript wartete also auf eine Freigabe, die erst kam, wenn der Nutzer
#     eine Warnung wegklickte, von der er nicht wusste, dass sie zum Update
#     gehört. Nach zwei Minuten gab es auf — und im Programmordner blieb eine
#     verwaiste 14-MB-Datei liegen. **Bei jedem Versuch aufs Neue.**
#
# Der Eigenbau ist deshalb weg. Unter Windows startet jetzt der Installer, und
# der kann all das, was hier mühsam nachgebaut war: Er beendet das laufende
# Programm über den Restart Manager (`CloseApplications=force` in
# `installer.iss`), ersetzt die Datei und pflegt den Eintrag in
# „Apps & Features“.
#
# ⚠ **Zwei Angaben standen hier falsch** und haben am 28.08.2026 die Fehlersuche
# in die Irre geschickt. Wer hier nachliest, soll den Stand aus `installer.iss`
# bekommen, nicht den von vorgestern:
#
#   * „erkannt über `AppMutex`“ — **nein.** Der Restart Manager erkennt ein
#     laufendes Programm an den Dateien, die es offen hält; einen Mutex braucht
#     er dafür nicht. `AppMutex` stand einmal in `installer.iss` und hat den
#     Update-Weg am 26.08.2026 vollständig blockiert — die Begründung steht
#     ausführlich dort.
#   * „startet den Watcher danach wieder (`RestartApplications=yes`)“ —
#     **nein.** Dort steht `RestartApplications=no`, mit Absicht: Der Restart
#     Manager fährt nur wieder hoch, was er selbst *sanft* geschlossen hat, und
#     `force` schliesst hart. Nach einem stillen Update startet **niemand** den
#     Watcher — der Nutzer macht das selbst, angesagt über
#     `s_ub_hinweis_neustart`.
#
# ⚠ Der Installer hält das Programm auch nicht *unten*: Ein Autostart-Eintrag
# kann es mitten in der Installation wieder hochfahren — gemessen am 28.08.2026,
# `DeleteFile ... in use (5)`. Dagegen steht `PrepareToInstall` im
# `[Code]`-Abschnitt von `installer.iss`.


# ⚠ Unter Windows übernimmt der Installer auch den **Neustart**. `neu_starten()`
# darf dann NICHT auch noch starten — sonst kommen zwei Versionen hoch, und die
# zweite legt sich über die Arbeit der ersten. Unter Linux bleibt es beim
# Tausch, dort startet das Programm sich selbst neu.
_TAUSCH_LAEUFT = [False]


def einspielen(neue_datei):
    """Die laufende Version durch die neue ersetzen.

    Zwei Wege, je nach Verpackung:

    * **Linux** — das AppImage wird getauscht. Danach genügt ein Neustart.
    * **Windows** — der Installer wird gestartet. Er beendet das laufende
      Programm selbst, ersetzt die Datei und fährt den Watcher wieder hoch.

    Gibt (True, '') zurück, wenn der Weg angetreten ist. Bei (False, Grund) muss
    der Nutzer selbst ran."""
    art = verpackung()
    if art == 'quellcode':
        return False, 'quellcode'
    ziel = eigenes_appimage() or sys.executable

    # ⚠ Letzter Riegel vor dem Überschreiben: Der Dateiname muss zu uns gehören.
    # Selbst wenn die Erkennung oben irgendwann wieder danebenliegt, wird dadurch
    # keine fremde Datei ersetzt. Genau dieser Riegel hätte den Unfall vom
    # 25.08.2026 verhindert, bei dem ein fremdes AppImage überschrieben wurde,
    # weil `APPIMAGE` auf ein anderes Programm zeigte.
    if 'sc-bp-watcher' not in os.path.basename(ziel).lower():
        from . import sprache
        return False, sprache.t('up_fremde_datei', os.path.basename(ziel))
    try:
        if art == 'appimage':
            # Unter Linux darf die laufende Datei ersetzt werden, solange man sie
            # austauscht statt hineinzuschreiben: Der laufende Prozess hält die
            # alte Inode, die neue liegt sofort am Platz.
            #
            # ⚠ `os.replace` schafft das nur **innerhalb eines Dateisystems**.
            # Deshalb wird gleich daneben geladen (siehe `_ablageort_fuer_update`).
            # Liegt die Datei doch woanders — Zielordner schreibgeschützt, eigener
            # Pfad über `SC_BP_APPIMAGE` —, tut es `shutil.move`: Das kopiert bei
            # Bedarf und räumt danach auf. Langsamer, aber es funktioniert.
            try:
                os.replace(neue_datei, ziel)
            except OSError as grund:
                if getattr(grund, 'errno', None) != errno.EXDEV:
                    raise
                import shutil
                shutil.move(neue_datei, ziel)
            os.chmod(ziel, 0o755)
            return True, ''
        # Windows: den Installer starten. Er bringt alles mit, was hier frueher
        # von Hand nachgebaut war — siehe die Erklaerung oben.
        #
        # `/SILENT` zeigt nur einen Fortschrittsbalken statt des ganzen
        # Assistenten, `/NORESTART` verbietet ihm, den Rechner neu zu starten,
        # und `/CLOSEAPPLICATIONS` laesst ihn den laufenden Watcher schliessen.
        #
        # ⚠ **Kein `/RESTARTAPPLICATIONS`.** Das war ein Fehler und hat am
        # 26.08.2026 den Selbststart zerschossen. Der Schalter uebersteuert
        # `RestartApplications=no` aus `installer.iss` — und dann starten
        # **zwei** Wege den Watcher: der Restart Manager und der
        # `[Run]`-Abschnitt. Im Protokoll steht beides direkt untereinander:
        #
        #     Attempting to restart applications.
        #     -- Run entry --   Filename: ...\SC-BP-Watcher.exe
        #
        # Wird das Setup dabei aus dem Watcher heraus gestartet, passt die
        # Prozesskette fuer den Neustart des Restart Managers nicht, und Inno
        # bricht mit „Security validation failure: parent process has different
        # executable!" ab. Aus einer PowerShell heraus faellt das nicht auf —
        # deshalb war der Fehler zuerst nicht nachstellbar. Den Neustart macht
        # allein `[Run]`.
        import subprocess
        _TAUSCH_LAEUFT[0] = True

        # ⚠ Die Umgebung MUSS gesaeubert werden, das Arbeitsverzeichnis ebenso.
        # Sonst erbt der Installer die PyInstaller-Variablen der laufenden
        # Version und sucht seine Bibliotheken in dem Ordner unter `%TEMP%`, den
        # der Bootloader gleich aufraeumen will. Genau diese Falle steht
        # ausfuehrlich bei `neu_starten()`.
        umgebung = dict(os.environ)
        for name in ('_MEIPASS', '_MEIPASS2', 'TCL_LIBRARY', 'TK_LIBRARY',
                     'TIX_LIBRARY', 'MATPLOTLIBDATA'):
            umgebung.pop(name, None)

        # ⚠ **`__COMPAT_LAYER` muss weg** — daran hing die Meldung
        #
        #     Security validation failure: parent process has different
        #     executable!
        #
        # die vier Anläufe gekostet hat. Der Weg dahin, weil er sich sonst nicht
        # wiederfinden lässt:
        #
        # Windows führt einen Kompatibilitäts-Speicher über Programme, die ihm
        # auffällig vorkommen. `SC-BP-Watcher.exe` steht dort — kein Wunder, der
        # Watcher wird bei jedem Update über `CloseApplications=force` hart
        # beendet. Windows setzt dem Prozess daraufhin `__COMPAT_LAYER` in die
        # Umgebung, und **jeder Kindprozess erbt die Variable**. Das Setup lief
        # damit unter einem Shim, Inno erkannte einen fremden Zwischenprozess und
        # brach ab.
        #
        # Nachgestellt und belegt (26.08.2026) — derselbe Aufruf, dieselbe Datei,
        # derselbe Pfad, nur die Variable unterschiedlich:
        #
        #     gesetzt   →  Compatibility mode: Yes (DetectorsAppHealth)  → Fehler
        #     entfernt  →  keine solche Zeile                            → läuft
        #
        # Genau deshalb war der Fehler aus einer PowerShell oder aus Python heraus
        # **nie** nachstellbar: Dort ist die Variable nicht gesetzt. Drei frühere
        # Erklärungen klangen schlüssig und wurden alle durch Messläufe widerlegt.
        # Gefunden hat es erst das Setup-Protokoll aus rc48.
        umgebung.pop('__COMPAT_LAYER', None)
        # ⚠ **Das Setup schreibt ein Protokoll**, und zwar immer — nicht nur im
        # Fehlerfall. Der Grund steht in der Geschichte dieser Funktion: Am
        # 26.08.2026 meldete Inno beim Update
        #
        #     Security validation failure: parent process has different
        #     executable!
        #
        # und **drei** Erklärungsversuche lagen daneben (vererbtes
        # Arbeitsverzeichnis, `/RESTARTAPPLICATIONS`, sterbender
        # Elternprozess). Jeder klang schlüssig, jeder wurde durch einen
        # Messlauf widerlegt. Nachstellen ließ sich der Fehler nie: aus einer
        # PowerShell oder aus Python heraus lief derselbe Aufruf sauber durch,
        # mit lebendem wie mit sterbendem Elternprozess.
        #
        # Ohne Protokoll bleibt in so einem Fall nur Raten — und Raten hat hier
        # drei Versionen gekostet. Mit Protokoll beantwortet der nächste
        # Fehlerfall die Frage selbst, auch wenn er bei einem Nutzer auftritt,
        # dessen Rechner niemand ansehen kann.
        #
        # Es landet neben dem Fehlerbericht, wird also vom Diagnose-Bericht
        # miterfasst. Eine Datei pro Lauf, die alte wird überschrieben — es geht
        # um den letzten Versuch, nicht um ein Tagebuch.
        protokoll_datei = ''
        try:
            from . import pfade
            protokoll_datei = pfade.app_datei('update-setup.txt')
        except Exception:
            pass                     # ohne Protokoll ist der Weg derselbe

        # ⚠ Der Umweg über `cmd` hält einen Elternprozess am Leben, solange das
        # Setup läuft. Nachgemessen ist er **nicht** nötig — auch ein sofort
        # abtretender Vater stört Inno nicht. Er bleibt trotzdem, weil er nichts
        # kostet und der Fehler oben noch ungeklärt ist; fällt er weg, wäre es
        # eine Änderung an einer Stelle, die gerade untersucht wird.
        #
        # Die doppelten Anführungszeichen sind cmd-Eigenart: `cmd /c "..."`
        # streicht das äußere Paar, deshalb braucht ein Pfad mit Leerzeichen ein
        # eigenes. Ohne das scheitert jeder Benutzername mit Leerzeichen —
        # geprüft mit `C:\Users\Max Mustermann\...`.
        schalter = '/SILENT /NORESTART /CLOSEAPPLICATIONS'
        # ⚠ Dorthin installieren, wo das laufende Programm liegt — sonst gibt es
        # zwei Kopien.
        #
        # v2.0.0 wurde **nur** als nackte `SC-BP-Watcher.exe` ausgeliefert; alle
        # ihre Nutzer laufen zwangsläufig „portabel", ohne es gewollt zu haben.
        # Gemeldet am 27.08.2026: „niemand nutzt sowas portabel … niemand
        # schiebt es auf nen Stick, um an nem anderen PC SC zu spielen."
        #
        # Ohne `/DIR` nimmt Inno seinen Standardordner
        # (`%LOCALAPPDATA%\Programs\…`) — die alte Datei bliebe daneben liegen,
        # und wer sie per Verknüpfung startet, benutzt für immer die alte
        # Fassung. Mit `/DIR` wird ersetzt statt danebengelegt.
        #
        # Läuft das Programm bereits installiert, zeigt `sys.executable` auf den
        # Installationsordner — dann ist es derselbe Wert, den Inno ohnehin
        # gewählt hätte. Ein Fall, zwei Wege, dieselbe Zeile.
        eigener_ordner = os.path.dirname(os.path.abspath(sys.executable))
        if eigener_ordner:
            schalter += ' /DIR="%s"' % eigener_ordner
        if protokoll_datei:
            schalter += ' /LOG="%s"' % protokoll_datei
        befehl = 'cmd /c ""%s" %s"' % (neue_datei, schalter)
        # ⚠ `DETACHED_PROCESS` **und** eine eigene Prozessgruppe. Ohne das bleibt
        # das Setup an uns gebunden — und wir treten gleich ab, damit der Restart
        # Manager nicht 30 Sekunden auf uns wartet. Inno prueft aber seinen
        # Elternprozess und meldet dann
        #
        #     Security validation failure: failed to obtain executable path for
        #     parent process!
        #
        # Das ist die Schwester der Meldung ueber `__COMPAT_LAYER` weiter oben:
        # einmal traegt der Elternprozess einen Shim, einmal ist er zum
        # Pruefzeitpunkt gar nicht mehr da. Beide Male geht es um denselben
        # Zusammenhang — wer ein Setup startet und sich sofort verabschiedet,
        # muss es vorher **loesen**.
        flags = getattr(subprocess, 'CREATE_NO_WINDOW', 0)
        flags |= getattr(subprocess, 'DETACHED_PROCESS', 0)
        flags |= getattr(subprocess, 'CREATE_NEW_PROCESS_GROUP', 0)
        subprocess.Popen(befehl, env=umgebung, cwd=tempfile.gettempdir(),
                         creationflags=flags)
        return True, ''
    except Exception as fehler:
        return False, str(fehler)


if __name__ == '__main__':
    eigene = '1.6.0-dev'
    print('Verpackung:', verpackung())
    neu = nachsehen(eigene, erzwingen='--jetzt' in sys.argv)
    print('Neuere Version:', (neu or {}).get('version') or 'keine')
    print('\nÄnderungsprotokoll:')
    for e in protokoll()[:6]:
        kopf = '%s %s' % (e['version'], ('(%s)' % e['datum']) if e['datum'] else '')
        print('  %-28s %s  %d Zeichen' % (kopf, e['quelle'], len(e['text'])))


# --------------------------------------------------- Eintrag in "Apps & Features"
# Der Installer trägt die Version dort ein. Ersetzt sich das Programm danach
# selbst, bleibt der Eintrag auf dem alten Stand stehen — Windows zeigt dann
# eine Nummer, die es gar nicht mehr gibt. Dazu: „Die Versionsanzeige
# wäre schon wichtig, da User ja sonst nicht sehen ob sie aktuell sind."
#
# Der Schlüssel liegt unter HKCU (der Installer schreibt in den Benutzerzweig),
# deshalb braucht das **keine Administratorrechte**.
INNO_KENNUNG = '{7C4B1E93-2A6F-4D58-B0E1-9F3A5C8D2461}_is1'


# Die frisch gestartete Version. Gebraucht wird sie nur, um **nachzusehen, ob
# sie noch lebt** — siehe `neue_fassung_laeuft()`.
_GESTARTET = [None]

# Wohin die Fehlerausgabe der frisch gestarteten Fassung läuft, und die offene
# Datei dazu. Ohne das ist ein gescheiterter Neustart nicht aufzuklären.
START_AUSGABE = 'neustart-ausgabe.txt'
_AUSGABE = [None]


def neue_fassung_laeuft(wartezeit=3.0):
    """Lebt die eben gestartete Version noch? Erst danach darf die alte gehen.

    ⚠ **Ein Programm zu starten heißt nicht, dass es läuft.** `Popen` meldet
    Erfolg, sobald der Prozess angelegt ist; ob er eine Sekunde später an einer
    fehlenden Bibliothek stirbt, erfährt niemand — `stdout` und `stderr` gehen
    nach `/dev/null`, und auf den Rückgabewert wartete bisher keiner.

    Genau so ist der Neustart unter Linux monatelang **stumm** gescheitert: Die
    alte Version trat pflichtschuldig ab, die neue war da schon tot, und übrig
    blieb ein Rechner ohne Watcher. Der Nutzer sieht nur „es geht aus und kommt
    nicht wieder" und kann nicht einmal sagen, woran es lag.

    Diese Prüfung kostet ein paar Sekunden Warten — sie gehört deshalb in einen
    eigenen Faden, nicht in den Tk-Faden.

    Gibt `True` zurück, wenn die neue Version die Wartezeit überlebt hat oder es
    gar keinen eigenen Prozess gibt (Windows: dort startet der Installer neu).
    """
    prozess = _GESTARTET[0]
    if prozess is None:
        return True
    import time
    ende = time.monotonic() + wartezeit
    while time.monotonic() < ende:
        if prozess.poll() is not None:
            _tot_melden(prozess.returncode)
            return False
        time.sleep(0.15)
    return True


def _tot_melden(rueckgabe):
    """Warum die neue Fassung gestorben ist — ins Fehlerprotokoll damit.

    ⚠ Ohne das steht im Diagnosebericht **gar nichts**: Bis rc69 wurde nur die
    Meldung ins Fenster geschrieben, und wer den Bericht schickte, hatte keinen
    einzigen Eintrag dazu. Am 27.08.2026: Neustart klappte nicht,
    Protokoll leer, Ursache im Dunkeln.
    """
    text = ''
    datei = _AUSGABE[0]
    if datei is not None:
        try:
            datei.flush()
            datei.seek(0)
            text = (datei.read() or '').strip()[-800:]
        except Exception:
            text = ''
    fehler.merken('aktualisierung.neustart_tot',
                  RuntimeError('Rückgabewert %s%s' % (
                      rueckgabe, (' — ' + text) if text else
                      ' — keine Ausgabe')))


def neu_starten():
    """Das Programm durch die frisch eingespielte Version ersetzen.

    Nach einem Update läuft weiter die alte Version — der Prozess hält seine alte
    Inode. „Beim nächsten Start läuft die neue" stimmt zwar, heißt aber: selbst
    beenden und selbst wieder starten. Das nimmt dieser Weg ab.

    ⚠ Reihenfolge: erst den Einzelinstanz-Wächter schließen, dann starten. Sonst
    sieht die neue Version den belegten Port, hält sich für die zweite Instanz und
    beendet sich sofort wieder.
    """
    import subprocess
    from . import overlay as overlay_modul
    ziel = eigenes_appimage() or sys.executable
    if verpackung() == 'quellcode':
        return False
    try:
        overlay_modul.waechter_stoppen()

        # Wartet ein Hilfsskript darauf, die Datei zu tauschen, ist unser Teil
        # hier **erledigt** — es startet die neue Version selbst. Siehe die
        # Erklärung über `_TAUSCH_LAEUFT`.
        if _TAUSCH_LAEUFT[0]:
            return True

        # ⚠ **Hier stand `dict(os.environ)`** — und genau daran ist der Neustart
        # unter Linux gescheitert. Entfernt wurden nur `APPIMAGE` und Freunde;
        # `LD_LIBRARY_PATH`, `PYTHONHOME` und `PYTHONPATH` blieben stehen, und
        # die zeigen im AppImage in den **entpackten Mount der alten Version**.
        # Zwei Sekunden später beendet sich die alte, ihr Mount verschwindet —
        # und die neue sucht ihre Bibliotheken in einem Verzeichnis, das es nicht
        # mehr gibt. Sie stirbt, bevor ein Fenster kommt.
        #
        # Für den Nutzer sah das so aus: „es geht dann aus aber startet nicht"
        # (Bomb20, 27.08.2026), am selben Tag nachgestellt.
        #
        # `pfade.saubere_umgebung()` macht genau diese Wäsche — sie war längst da,
        # nur benutzte der Neustart eine eigene, unvollständige Version davon.
        # Zwei Wäschen sind eine zu viel.
        from . import pfade as pfade_modul
        umgebung = pfade_modul.saubere_umgebung()
        # Die Variablen des laufenden AppImage gehören der **alten** Version.
        for name in ('APPIMAGE', 'APPDIR', 'OWD', 'ARGV0'):
            umgebung.pop(name, None)

        # ⚠ Dasselbe gilt unter Windows für die Variablen von PyInstaller — und
        # dort ist es schlimmer, weil das Programm gar nicht mehr startet.
        #
        # Eine mit PyInstaller gebaute `.exe` entpackt sich beim Start nach
        # `%TEMP%\_MEIxxxxxx` und zeigt mit `TCL_LIBRARY` und `TK_LIBRARY`
        # dorthin. Erbt die neue Version diese Variablen, sucht sie ihre
        # Tcl-Dateien im Ordner der **alten** — den die alte beim Beenden
        # gerade aufräumt. Ergebnis, so beim Testen gemeldet (Haldjas,
        # 25.08.2026):
        #
        #     Failed to execute script 'sc_bp_watcher' due to unhandled
        #     exception: Can't find a usable init.tcl in the following
        #     directories: C:\Users\…\AppData\Local\Temp\_MEI000067b42…
        #
        # Und gleich hinterher die Gegenseite: „Failed to remove temporary
        # directory" — die alte Version kommt an ihren eigenen Ordner nicht
        # mehr heran, weil die neue darin liest.
        for name in ('_MEIPASS', '_MEIPASS2', 'TCL_LIBRARY', 'TK_LIBRARY',
                     'TIX_LIBRARY', 'MATPLOTLIBDATA'):
            umgebung.pop(name, None)
        # ⚠ **`stderr` NICHT wegwerfen.** Hier stand `DEVNULL` — und genau
        # deshalb war der gescheiterte Neustart unter Linux monatelang nicht
        # aufzuklären: Die neue Fassung schrieb ihren Grund brav auf die
        # Fehlerausgabe, und wir haben ihn ins Nichts geleitet. Übrig blieb
        # „geht aus, kommt nicht wieder" und Raten.
        #
        # Jetzt läuft die Ausgabe in eine Datei neben den Diagnosebericht. Kommt
        # die neue Fassung nicht hoch, steht dort, woran es lag — und
        # `neue_fassung_laeuft()` hängt es ins Fehlerprotokoll, wo es im Bericht
        # auftaucht.
        try:
            _AUSGABE[0] = open(pfade_modul.app_datei(START_AUSGABE), 'w+',
                               encoding='utf-8', errors='replace')
        except Exception:
            _AUSGABE[0] = None
        _GESTARTET[0] = subprocess.Popen(
            [ziel], env=umgebung, start_new_session=True,
            stdout=subprocess.DEVNULL,
            stderr=_AUSGABE[0] or subprocess.DEVNULL)
        return True
    except Exception as ausnahme:
        fehler.merken('aktualisierung.neu_starten', ausnahme)
        return False


def windows_eintrag_pflegen(eigene_version):
    """Die angezeigte Version in Windows nachziehen. True, wenn geändert.

    ⚠ Der Schlüssel liegt **nicht** immer unter HKCU. Der Kommentar über
    `INNO_KENNUNG` behauptete das jahrelang, und die Funktion suchte nur dort —
    also fand sie am 26.08.2026 auf dem Testrechner gar nichts, obwohl der
    Eintrag existierte. Er lag unter **HKLM**.

    Grund: `installer.iss` hat zwar `PrivilegesRequired=lowest`, dazu aber
    `PrivilegesRequiredOverridesAllowed=dialog`. Inno fragt damit beim
    Installieren nach, und wer "für alle Nutzer" wählt, bekommt seinen Eintrag
    im Maschinenzweig. Gesucht wird deshalb in beiden.

    Schreiben klappt in HKLM ohne Administratorrechte nicht — das ist in Ordnung
    und **kein Fehler**: Dann bleibt die angezeigte Nummer eben stehen, statt
    dass eine Ausnahme das Update aufhält.
    """
    if not sys.platform.startswith('win') or not eigene_version:
        return False
    try:
        import winreg
    except ImportError:
        return False
    pfad = (r'Software\Microsoft\Windows\CurrentVersion\Uninstall\%s'
            % INNO_KENNUNG)
    for zweig in (winreg.HKEY_CURRENT_USER, winreg.HKEY_LOCAL_MACHINE):
        try:
            with winreg.OpenKey(zweig, pfad, 0,
                                winreg.KEY_READ | winreg.KEY_WRITE) as schluessel:
                try:
                    steht_da = winreg.QueryValueEx(schluessel, 'DisplayVersion')[0]
                except FileNotFoundError:
                    steht_da = ''
                if str(steht_da) == str(eigene_version):
                    return False
                winreg.SetValueEx(schluessel, 'DisplayVersion', 0, winreg.REG_SZ,
                                  str(eigene_version))
                return True
        except FileNotFoundError:
            continue          # in diesem Zweig nicht installiert
        except PermissionError:
            continue          # HKLM ohne Administratorrechte — hinnehmen
        except Exception as ausnahme:
            from . import fehler
            fehler.merken('aktualisierung.windows_eintrag', ausnahme)
            return False
    return False
