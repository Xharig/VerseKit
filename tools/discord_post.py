# -*- coding: utf-8 -*-
"""
Baut aus dem CHANGELOG die **Versionsmeldung fürs Discord**.

Zum Projekt gehoert ein eigenes Discord, das wie ein Forum aufgebaut ist: ein Kanal
nur für Versionsmeldungen, daneben Kanäle für Fragen und Fehlermeldungen. Der
Meldungs-Kanal soll sauber bleiben — eine Nachricht je Version, immer gleich
aufgebaut, ohne Diskussion dazwischen.

Warum ein Skript und keine Handarbeit: Der CHANGELOG-Eintrag ist zu lang und zu
technisch für Discord (dort gibt es **keine Tabellen**, und bei 2000 Zeichen ist
Schluss). Von Hand kürzen heißt: jedes Mal neu entscheiden, was wegfällt — und
irgendwann bleibt die Meldung ganz aus.

    python3 tools/discord_post.py v2.1.0          # deutsch
    python3 tools/discord_post.py v2.1.0 --en     # englisch

Was herauskommt, ist zum **Kopieren** gedacht. Automatisch posten könnte man über
einen Discord-Webhook — das wäre ein Zugangsschlüssel mehr und eine Nachricht,
die ohne zweiten Blick rausgeht. Beides bewusst nicht.
"""
import os
import re
import sys

HIER = os.path.dirname(os.path.abspath(__file__))
WURZEL = os.path.dirname(HIER)
sys.path.insert(0, os.path.join(WURZEL, '.github', 'scripts'))

GRENZE = 2000            # Discord nimmt nicht mehr je Nachricht
PUNKTE = 6               # mehr liest im Vorbeiscrollen niemand
REPO = 'https://github.com/Xharig/SC-BP-Watcher'
# Eine Vorabfassung erkennt man am Anhängsel: v3.9.2-rc7, auch -beta / -alpha.
_VORAB = re.compile(r'-(?:rc|beta|alpha)', re.I)


def punkte_aus(block):
    """Die Überschriften der obersten Aufzählungsebene — ohne Unterpunkte.

    ⚠⚠ **Erst den Punkt zusammensetzen, dann kürzen.** Im CHANGELOG ist jeder
    Punkt über mehrere Zeilen umgebrochen; die fette Überschrift reicht oft bis
    in die zweite. Wer nur die erste Zeile nimmt, findet das schließende `**`
    nicht, fällt auf „erster Satz" zurück — und der endet dann mitten im Wort.
    Genau so sah die Meldung zu v3.3.0 aus: „Ins Lager kommt nur noch, was es
    im Spiel wirklich gibt — Rohstoff", „Die Suche findet auch die Zutat. »ric«
    brachte". Sechs von sechs Punkten abgeschnitten, in einer Nachricht, die an
    mehrere hundert Leute geht (30.08.2026 aufgefallen).
    """
    # ⚠ **Der Dank gehört nicht in die Ankündigung.** Er steht im Programm auf
    # der Seite „Danke & Lizenzen" — dort sucht ihn, wer ihn sehen will. In
    # einer Versionsmeldung ist er Ballast und verdrängt das, worum es geht:
    # was das Werkzeug jetzt kann.
    block = re.split(r'(?m)^### (?:Danke|Thanks)\s*$', block)[0]

    gefunden = []
    puffer = [None]

    def ablegen():
        text = puffer[0]
        puffer[0] = None
        if not text:
            return
        # „**Titel.** Erklärung …" -> nur der Titel; sonst der erste Satz.
        fett = re.match(r'\*\*(.+?)\*\*', text, re.S)
        kurz = fett.group(1) if fett else text
        kurz = re.sub(r'[`*_]', '', kurz)
        kurz = re.sub(r'\s+', ' ', kurz).strip()
        # ⚠ Auch eine fette Überschrift kann zwei Sätze lang sein. Für Discord
        # zählt der erste — der Rest steht im Änderungsprotokoll. Aber nur,
        # wenn dabei noch ein Satz übrig bleibt und kein Stummel.
        erster = re.split(r'(?<=[.!?])\s', kurz)[0]
        if len(erster) >= 25:
            kurz = erster
        kurz = kurz.strip(' .—-')
        if kurz:
            gefunden.append(kurz)

    for zeile in block.split('\n'):
        if zeile.startswith('- '):
            ablegen()
            puffer[0] = zeile[2:].strip()
            continue
        if puffer[0] is None:
            continue
        inhalt = zeile.strip()
        # Leerzeile, Überschrift oder ein neuer Block: der Punkt ist zu Ende.
        # Unterpunkte und Tabellenzeilen gehören nicht in die Kurzfassung.
        if (not inhalt or not zeile.startswith((' ', '\t'))
                or inhalt.startswith(('- ', '* ', '|', '>', '#'))):
            ablegen()
            continue
        puffer[0] += ' ' + inhalt
    ablegen()
    return gefunden


def vorspann_aus(block):
    """Der handgeschriebene Kurztext einer Version — oder `''`.

    ⭐⭐ **Der wichtigste Teil dieser Datei.**

    Eine Ankündigung ist kein Änderungsprotokoll. Das Protokoll ist
    Buchführung: vollständig, sachlich, mit Begründung. Die Ankündigung ist das
    Gegenteil — sie darf weglassen, zuspitzen und Lust auf mehr machen. Wer aus
    dem einen automatisch das andere erzeugt, bekommt beides halb: eine Liste,
    die zu lang zum Lesen und zu knapp zum Verstehen ist.

    Deshalb: **Steht direkt unter der Versionsüberschrift ein Absatz** (vor dem
    ersten `###`), ist das die Ankündigung, und der Rest des Protokolls bleibt
    für die, die es genau wissen wollen. Fehlt er, greift die Aufzählung der
    Überschriften.

    So bleibt der Text, was er sein muss: von einem Menschen geschrieben, für
    Menschen.

    ⚠⚠ **Der Vorspann ist im CHANGELOG ein Blockzitat** (`> …`) — so steht er
    dort seit jeher, in jeder Version. Diese Funktion hat `> `-Zeilen früher
    übersprungen und damit **nie einen Vorspann gefunden**: Jede Ankündigung
    fiel auf die Aufzählung der Überschriften zurück, obwohl der Text
    danebenstand. Aufgefallen am 03.09.2026, betroffen war jede bisherige
    Version.

    Ausgenommen bleiben die Hinweiskästen (`> [!important]`, `> [!warning]`):
    Die richten sich an Umsteiger und sind keine Ankündigung.
    """
    # Erst den Bereich vor dem ersten `###` nehmen — nur dort steht ein
    # Vorspann.
    kopf = []
    for zeile in block.splitlines():
        if zeile.startswith('### '):
            break
        if zeile.startswith('#'):
            continue
        kopf.append(zeile)

    # Dann die erste zusammenhängende Gruppe, die kein Hinweiskasten ist.
    gruppen, laufend = [], []
    for zeile in kopf:
        blank = not zeile.strip()
        if blank:
            if laufend:
                gruppen.append(laufend)
                laufend = []
            continue
        laufend.append(zeile)
    if laufend:
        gruppen.append(laufend)

    text = ''
    for gruppe in gruppen:
        erste = gruppe[0].strip()
        if erste.startswith('> [!'):
            continue
        sauber = [z[2:] if z.startswith('> ') else z.lstrip('>').strip()
                  for z in gruppe]
        kandidat = ' '.join(x.strip() for x in sauber).strip()
        if kandidat:
            text = kandidat
            break
    # Ein einzelner Aufzählungspunkt ist kein Vorspann, sondern ein verirrter
    # Protokolleintrag.
    #
    # ⚠ **Auf das Leerzeichen achten.** Geprüft wird `- ` und `* `, nicht `*`:
    # Ein Vorspann fängt fast immer mit einem fetten Satz an (`**Die Raffinerie
    # verrät …`), und der beginnt ebenfalls mit einem Stern. Ohne das
    # Leerzeichen verwarf diese Zeile genau die Texte, die sie schützen sollte
    # — zusammen mit der Blockzitat-Blindheit oben der Grund, warum bis zum
    # 03.09.2026 nie ein Vorspann im Discord landete.
    if text.startswith('- ') or text.startswith('* '):
        return ''
    return text


def herunterladen_link(tag):
    """Die Adresse, die im Post unter „Herunterladen" steht.

    ⚠ **Nicht immer `/releases/latest`.** GitHub überspringt dort jede
    Vorabfassung — wer den Post zu `v3.9.2-rc7` liest und darauf klickt, landet
    bei der letzten **stabilen** Version und bekommt die Testfassung nie zu
    sehen. Gemessen am 02.09.2026: sieben Testfassungen in einer Nacht,
    **ein** Download über alle zusammen.

    Bei einer Vorabfassung zeigt der Link deshalb direkt auf ihren Tag.
    """
    if _VORAB.search(tag):
        return '%s/releases/tag/%s' % (REPO, tag)
    return '%s/releases/latest' % REPO


def bauen(tag, sprache='de'):
    import release_text

    datei = release_text.DATEIEN['de' if sprache == 'de' else 'en']
    block = release_text.abschnitt(os.path.join(WURZEL, datei), tag)
    if not block:
        return ''
    holen = herunterladen_link(tag)

    # ⭐ Der handgeschriebene Kurztext hat Vorrang — siehe `vorspann_aus`.
    vorspann = vorspann_aus(block)
    if vorspann:
        if sprache == 'de':
            return ('## VerseKit %s ist da\n\n%s\n\n'
                    '**Herunterladen:** <%s>\n'
                    'Alle Änderungen im Einzelnen: '
                    '<%s/blob/main/CHANGELOG.md>'
                    % (tag, vorspann, holen, REPO))
        return ('## VerseKit %s is out\n\n%s\n\n'
                '**Download:** <%s>\n'
                'Every change in detail: <%s/blob/main/CHANGELOG.en.md>'
                % (tag, vorspann, holen, REPO))

    punkte = punkte_aus(block)[:PUNKTE]
    # ⚠ Eine reine Fehlerbehebung anzukündigen mit „Was diese Version bringt"
    # liest sich schief: Darunter stehen dann Sätze wie „Der eingetippte Name
    # kam nicht mit" — also das Problem, nicht die Neuerung. Bei einer Fassung
    # ohne neue Funktionen sagt die Zeile deshalb, was sie ist.
    nur_behoben = bool(re.search(r'(?m)^### (Behoben|Fixed)\s*$', block)) and \
        not re.search(r'(?m)^### (Neu|Added|Geändert|Changed)\s*$', block)
    if sprache == 'de':
        kopf = '## VerseKit %s ist da' % tag
        rest = (('Behoben in dieser Fassung:' if nur_behoben
                 else 'Was diese Version bringt:') if punkte else '')
        fuss = ('\n**Herunterladen:** <%s>\n'
                'Fehler gefunden oder eine Frage? Ab damit in die passenden Kanäle — '
                'hier bleibt es bei den Versionsmeldungen.' % holen)
    else:
        kopf = '## VerseKit %s is out' % tag
        rest = (('Fixed in this build:' if nur_behoben
                 else 'What this version brings:') if punkte else '')
        fuss = ('\n**Download:** <%s>\n'
                'Found a bug or have a question? Please use the matching channels — '
                'this one stays version announcements only.' % holen)

    text = kopf + '\n\n' + (rest + '\n' if rest else '')
    for p in punkte:
        text += '· %s\n' % p
    text += fuss

    if len(text) > GRENZE:
        # Lieber einen Punkt weniger als eine abgeschnittene Nachricht.
        while punkte and len(text) > GRENZE:
            punkte.pop()
            text = kopf + '\n\n' + (rest + '\n' if rest else '')
            for p in punkte:
                text += '· %s\n' % p
            text += fuss
    return text


def main():
    if len(sys.argv) < 2:
        print(__doc__.strip())
        return 2
    tag = sys.argv[1]
    sprache = 'en' if '--en' in sys.argv else 'de'
    text = bauen(tag, sprache)
    if not text:
        print('Kein CHANGELOG-Abschnitt zu %s gefunden.' % tag, file=sys.stderr)
        return 1
    print(text)
    print('\n---\n%d von %d Zeichen' % (len(text), GRENZE), file=sys.stderr)
    return 0


if __name__ == '__main__':
    sys.exit(main())
