#!/usr/bin/env bash
#
# Eine Fassung von GitHub holen und hier einsetzen — beenden, tauschen, starten.
#
# Wozu ein eigenes Skript: Beim Einsetzen von Hand läuft leicht noch das zu
# ersetzende Programm weiter. Auf dem Bildschirm steht dann die vorige Nummer,
# obwohl die neue Datei längst auf der Platte liegt — und beim Suchen des
# Fehlers sucht man an der falschen Stelle.
#
# ⚠ Das Beenden ist die heikle Stelle. `pkill -f SC-BP-Watcher` erwischt auch die
# eigene Shell: Das Repo heißt selbst `SC-BP-Watcher`, der Ordnername steht also
# in jeder Kommandozeile, die hier drin läuft. Der Trick mit `[S]C-BP-Watcher`
# reicht deshalb nicht — auch er passt auf den Pfad. Gesucht wird stattdessen
# nach dem **Programm selbst**: dem AppImage-Pfad am Anfang der Kommandozeile
# oder dem entpackten Programm unter /tmp/.mount_*. Zusätzlich fliegen die eigene
# Prozessgruppe und alles ohne Zahl heraus.
#
#   bash tools/einspielen_lokal.sh v3.0.0-rc9
#
set -euo pipefail

TAG="${1:-}"
ZIEL="${SC_BP_APPIMAGE:-$HOME/Programme/SC-BP-Watcher.AppImage}"
[ -n "$TAG" ] || { echo "Aufruf: bash tools/einspielen_lokal.sh v3.0.0-rc9" >&2; exit 1; }

echo "1/4  Laufende Fassung beenden …"
MEINE_GRUPPE="$(ps -o pgid= -p $$ | tr -d ' ')"
beenden() {
  local gefunden=0
  while read -r pid pgid rest; do
    [ "$pgid" = "$MEINE_GRUPPE" ] && continue      # nie den eigenen Ablauf
    case "$rest" in
      "$ZIEL"*|/tmp/.mount_SC-BP*|*/usr/bin/SC-BP-Watcher*)
        kill "$pid" 2>/dev/null && gefunden=1 ;;
    esac
  done < <(ps -eo pid=,pgid=,args=)
  return $gefunden
}
beenden || true
sleep 2

echo "2/4  $TAG holen …"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
gh release download "$TAG" --pattern '*.AppImage' --dir "$TMP" >/dev/null

echo "3/4  Einsetzen …"
[ -f "$ZIEL" ] && cp -f "$ZIEL" "$ZIEL.vorher"
install -m 755 "$TMP"/*.AppImage "$ZIEL"

echo "4/4  Starten …"
setsid nohup "$ZIEL" >/dev/null 2>&1 < /dev/null &
sleep 8

ANZAHL="$(ps -eo args= | grep -c "^$ZIEL" || true)"
if [ "$ANZAHL" -ge 1 ]; then
  echo "Läuft: $TAG   ($ZIEL)"
else
  echo "FEHLER: startet nicht. Von Hand versuchen: \"$ZIEL\"" >&2
  exit 1
fi
