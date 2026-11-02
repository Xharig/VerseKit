#!/usr/bin/env bash
#
# Startet die Testfassung **neben** der laufenden Live-Version.
#
# Wozu: Beurteilen laesst sich nur, was man selbst benutzt — aber die eigene
# Installation dabei zu ueberschreiben ist keine gute Idee. Dieses Skript haelt
# beide getrennt:
#
#   Live                                  Test
#   ~/Programme/SC-BP-Watcher.AppImage    ~/Programme/SC-BP-Watcher-TEST.AppImage
#   ~/.local/share/sc-bp-watcher (o. ae.) ~/.local/share/sc-bp-watcher-TEST
#
# ⚠ **Der getrennte Datenordner ist der Punkt.** Ohne ihn teilen sich beide
# Fassungen Bestand, Einstellungen und Katalog — die Testfassung wuerde in
# deinen echten Bestand schreiben, und ein Fehler darin traefe die Live-Seite.
#
#   bash tools/testfassung_starten.sh              # gebaute Fassung holen + starten
#   bash tools/testfassung_starten.sh --quellcode  # den AKTUELLEN Stand starten
#   bash tools/testfassung_starten.sh --nur-start  # nur starten, nichts holen
#   bash tools/testfassung_starten.sh --bestand    # Bestand einmalig uebernehmen
#
# ⚠ `--quellcode` ist der Weg fuer Zwischenstaende: Er startet, was gerade im
# Arbeitsverzeichnis liegt — ohne Bau, ohne Actions-Lauf, ohne `gh`. Genau das,
# was man beim Entwickeln alle paar Minuten braucht. Die gebaute Fassung ist
# erst vor einer Veroeffentlichung interessant (dort steckt die Verpackung, die
# eigene Fehler haben kann).
#
set -euo pipefail

WURZEL="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_APP="${SC_BP_TEST_APPIMAGE:-$HOME/Programme/SC-BP-Watcher-TEST.AppImage}"
TEST_HOME="${SC_BP_TEST_HOME:-$HOME/.local/share/sc-bp-watcher-TEST}"

holen=ja
bestand=nein
quellcode=nein
for arg in "$@"; do
  case "$arg" in
    --nur-start) holen=nein ;;
    --bestand)   bestand=ja ;;
    --quellcode) quellcode=ja; holen=nein ;;
    *) echo "Unbekannte Option: $arg" >&2; exit 2 ;;
  esac
done

mkdir -p "$TEST_HOME" "$(dirname "$TEST_APP")"

# Aus dem Quellcode starten — kein Bau noetig.
if [ "$quellcode" = ja ]; then
  mkdir -p "$TEST_HOME"
  echo
  echo "Starte den AKTUELLEN Arbeitsstand als TESTFASSUNG"
  echo "  Quellcode: $WURZEL"
  echo "  Daten    : $TEST_HOME"
  echo "  (die Live-Version bleibt unberuehrt)"
  echo
  ( cd "$WURZEL" && cargo run --release -p versekit-app -- --daten "$TEST_HOME" ) &
  echo "Läuft im Hintergrund. Zum Beenden das Fenster schliessen."
  exit 0
fi

if [ "$holen" = ja ]; then
  echo "Hole die neueste Testfassung nach $TEST_APP …"
  SC_BP_APPIMAGE="$TEST_APP" bash "$WURZEL/tools/testfassung_holen.sh"
fi

if [ ! -x "$TEST_APP" ]; then
  echo "Es liegt keine Testfassung unter $TEST_APP." >&2
  echo "Einmal ohne --nur-start aufrufen, dann wird sie geholt." >&2
  exit 1
fi

# Den echten Bestand einmalig uebernehmen — praktisch, wenn man die Testfassung
# mit den eigenen Bauplaenen ansehen will statt mit einer leeren Liste.
# ⚠ Nur KOPIEREN, nie verlinken: Sonst schreibt die Testfassung doch wieder in
# die echte Datei, und genau das soll die Trennung verhindern.
if [ "$bestand" = ja ]; then
  for kandidat in "$HOME/.local/share/sc-bp-watcher" \
                  "$HOME/Dokumente/SC BP Watcher" \
                  "$HOME/Documents/SC BP Watcher"; do
    if [ -d "$kandidat" ]; then
      echo "Uebernehme Bestand aus: $kandidat"
      find "$kandidat" -name 'bestand.json' -exec cp {} "$TEST_HOME/" \; 2>/dev/null || true
      break
    fi
  done
fi

echo
echo "Starte die TESTFASSUNG"
echo "  Programm : $TEST_APP"
echo "  Daten    : $TEST_HOME"
echo "  (die Live-Version bleibt unberuehrt)"
echo

# SC_BP_TESTFASSUNG faerbt den Fenstertitel — damit man die beiden Fenster
# auseinanderhaelt. Ohne das aendert man Einstellungen im falschen.
SC_BP_HOME="$TEST_HOME" SC_BP_TESTFASSUNG=1 "$TEST_APP" "$@" &
echo "Läuft im Hintergrund. Zum Beenden das Fenster schliessen."
