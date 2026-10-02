#!/usr/bin/env bash
#
# Holt die neueste **unveroeffentlichte** Fassung als AppImage und setzt sie ein.
#
# Wozu: Zum Ausprobieren braucht es keinen Release. Ein Tag ist fuer Nutzer da —
# er loest bei allen die Update-Meldung aus. Wer selbst testen will, nimmt das
# AppImage, das GitHub bei jedem Bau-Lauf ohnehin anhaengt.
#
# Voraussetzung: die GitHub-Kommandozeile `gh`, einmal eingeloggt.
#   Arch/EndeavourOS:  sudo pacman -S github-cli && gh auth login
#
# Der Zielpfad laesst sich mit SC_BP_APPIMAGE ueberschreiben.
#
#   bash tools/testfassung_holen.sh
#
set -euo pipefail

ZIEL="${SC_BP_APPIMAGE:-$HOME/Programme/SC-BP-Watcher.AppImage}"
ABLAUF='release.yml'
WURZEL="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v gh >/dev/null 2>&1; then
  echo "Die GitHub-Kommandozeile 'gh' fehlt." >&2
  echo "  sudo pacman -S github-cli   (danach einmal: gh auth login)" >&2
  exit 1
fi

if ! gh auth status >/dev/null 2>&1; then
  echo "'gh' ist noch nicht angemeldet — bitte einmal 'gh auth login' ausfuehren." >&2
  exit 1
fi

cd "$WURZEL"

# Der Bau haengt am Zweig: Waehrend an v2.2 gearbeitet wird, liegt auf main der
# eingefrorene Stand fuer die naechste Veroeffentlichung. Genommen wird deshalb der
# Zweig, der hier gerade ausgecheckt ist — oder was in SC_BP_ZWEIG steht.
ZWEIG="${SC_BP_ZWEIG:-$(git -C "$WURZEL" rev-parse --abbrev-ref HEAD)}"

echo "Suche den letzten erfolgreichen Bau auf Zweig '$ZWEIG' …"
LAUF="$(gh run list --workflow "$ABLAUF" --branch "$ZWEIG" --status success --limit 1 \
        --json databaseId --jq '.[0].databaseId')"
if [ -z "${LAUF:-}" ]; then
  # ⚠ Ein Bau, der von einem **Tag** ausgeloest wurde, haengt am Tag — nicht am
  # Zweig. Nach jedem Release oder jeder Testfassung (v3.3.0-rc1 usw.) findet
  # die Suche nach dem Zweig deshalb nichts, obwohl die Dateien fertig
  # danebenliegen. Dann den neuesten erfolgreichen Lauf ueberhaupt nehmen und
  # dazusagen, woher er kommt.
  echo "  Auf '$ZWEIG' kein Bau — nehme den neuesten erfolgreichen."
  LAUF="$(gh run list --workflow "$ABLAUF" --status success --limit 1 \
          --json databaseId --jq '.[0].databaseId')"
  HER="$(gh run list --workflow "$ABLAUF" --status success --limit 1 \
         --json headBranch --jq '.[0].headBranch')"
  if [ -z "${LAUF:-}" ]; then
    echo "Es gibt gar keinen erfolgreichen Bau. Zuerst einen anstossen:" >&2
    echo "  gh workflow run $ABLAUF --ref $ZWEIG" >&2
    exit 1
  fi
  echo "  Lauf $LAUF (von '$HER')"
else
  echo "  Lauf $LAUF"
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "Lade das AppImage …"
gh run download "$LAUF" --name linux --dir "$TMP"

NEU="$(find "$TMP" -name '*.AppImage' -print -quit)"
[ -n "$NEU" ] || { echo "Im Anhang war kein AppImage." >&2; exit 1; }

mkdir -p "$(dirname "$ZIEL")"

# Die zuletzt eingesetzte Datei bleibt als .vorher liegen — falls die neue klemmt,
# ist der Rueckweg ein Umbenennen und kein neuer Download.
if [ -f "$ZIEL" ]; then
  cp -f "$ZIEL" "$ZIEL.vorher"
  echo "Bisherige Fassung gesichert: $ZIEL.vorher"
fi

install -m 755 "$NEU" "$ZIEL"
echo "Eingesetzt: $ZIEL"

VER="$(grep -m1 '^__version__' "$WURZEL/sc_bp_watcher.py" | cut -d"'" -f2 || true)"
echo "Quellstand dieser Fassung: ${VER:-unbekannt} (unveroeffentlicht)"
echo
echo "Starten:  \"$ZIEL\""
