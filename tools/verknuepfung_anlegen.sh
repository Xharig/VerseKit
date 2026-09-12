#!/usr/bin/env bash
#
# Legt eine Start-Verknüpfung an — für Testläufe aus dem Quellcode.
#
# Wozu: Wer eine Fassung prüfen will, soll sie nicht jedes Mal im Dateibaum
# suchen. Unter Linux entsteht ein Eintrag auf dem Schreibtisch und im
# Startmenü, unter macOS eine .command-Datei auf dem Schreibtisch.
#
# Das ist NICHT der Weg für Nutzer — die bekommen den Installer bzw. das
# AppImage. Hier geht es um den aktuellen Quellcode zum Ausprobieren.
#
#   bash tools/verknuepfung_anlegen.sh
#
set -euo pipefail

WURZEL="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# ⚠ Sichtbarer Name — wandert mit der Umbenennung (12.09.2026). Der Zusatz
# „(Quellcode)" unterscheidet ihn vom echten Eintrag des Installers bzw. des
# AppImage, damit im Menü nicht zwei gleich heißende Einträge stehen.
NAME="VerseKit (Quellcode)"

if [[ "$(uname -s)" == "Darwin" ]]; then
  ZIEL="$HOME/Desktop/$NAME.command"
  cat > "$ZIEL" <<EOF
#!/bin/bash
cd "$WURZEL" || exit 1
git pull --quiet 2>/dev/null
PY=/opt/homebrew/bin/python3; [ -x "\$PY" ] || PY=python3
export SC_BP_NO_NET=1
export SC_BP_HOME="\${TMPDIR:-/tmp}/sc-bp-watcher-test"
mkdir -p "\$SC_BP_HOME"
"\$PY" "$WURZEL/tools/probe_daten.py" "\$SC_BP_HOME"
"\$PY" -c "import sys; sys.path.insert(0,'.'); from scbp import hauptfenster; f=hauptfenster.Hauptfenster(version='3.0.0-test'); f.root.geometry('1040x760+60+60'); f.run()"
EOF
  chmod +x "$ZIEL"
  echo "Angelegt: $ZIEL"
  exit 0
fi

# --- Linux ---
SCHREIBTISCH="$(xdg-user-dir DESKTOP 2>/dev/null || echo "$HOME/Schreibtisch")"
[ -d "$SCHREIBTISCH" ] || SCHREIBTISCH="$HOME/Desktop"
[ -d "$SCHREIBTISCH" ] || SCHREIBTISCH="$HOME"

INHALT="[Desktop Entry]
Type=Application
Name=$NAME
Comment=Startet den aktuellen Quellcode zum Testen
Exec=bash -c 'cd \"$WURZEL\" && bash \"SC-BP-Watcher starten.sh\"'
Icon=$WURZEL/assets/icon.png
Terminal=false
Categories=Game;Utility;
"

for ORT in "$SCHREIBTISCH" "$HOME/.local/share/applications"; do
  mkdir -p "$ORT"
  printf '%s' "$INHALT" > "$ORT/sc-bp-watcher-quellcode.desktop"
  chmod +x "$ORT/sc-bp-watcher-quellcode.desktop"
  echo "Angelegt: $ORT/sc-bp-watcher-quellcode.desktop"
done

# KDE und GNOME wollen das ausdrücklich erlaubt haben, sonst bleibt das Symbol tot.
if command -v gio >/dev/null 2>&1; then
  gio set "$SCHREIBTISCH/sc-bp-watcher-quellcode.desktop" \
      metadata::trusted true 2>/dev/null || true
fi
echo
echo "Falls das Symbol auf dem Schreibtisch nicht startet: einmal rechts"
echo "anklicken und „Ausführen erlauben\" (KDE) bzw. „Starten erlauben\" (GNOME)."
