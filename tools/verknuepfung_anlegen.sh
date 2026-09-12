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

# ⚠⚠ Der Name der ALTEN Fassung. Unter macOS steckt der sichtbare Name im
# DATEINAMEN (`$NAME.command`) — nach der Umbenennung entstünde also ein
# zweiter Startknopf neben dem alten. Unter Linux ist der Dateiname fest
# (`sc-bp-watcher-quellcode.desktop`), dort gibt es das Problem nicht.
ALT_NAME="SC BP Watcher (Quellcode)"

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

  # ⭐ Den alten Startknopf wegräumen — aber NUR, wenn er nachweislich von
  # diesem Skript stammt. Ein Dateiname allein belegt das nicht: Der Nutzer
  # darf eine eigene Datei genauso nennen.
  #
  # Der Beleg ist die Zeile, die nur wir hineinschreiben. Fehlt sie, bleibt
  # die Datei stehen — ein übrig gebliebener Startknopf ist harmlos, eine
  # gelöschte fremde Datei nicht.
  #
  # ⚠ Und erst NACH dem erfolgreichen Schreiben des neuen: entfernt wird nur,
  # wofür ein Ersatz dasteht.
  ALT_ZIEL="$HOME/Desktop/$ALT_NAME.command"
  if [[ -f "$ALT_ZIEL" && "$ALT_ZIEL" != "$ZIEL" ]] \
     && grep -q 'tools/probe_daten.py' "$ALT_ZIEL" 2>/dev/null; then
    rm -f "$ALT_ZIEL"
    echo "Alten Startknopf entfernt: $ALT_ZIEL"
  elif [[ -f "$ALT_ZIEL" ]]; then
    echo "Hinweis: $ALT_ZIEL bleibt stehen — stammt nicht von diesem Skript."
  fi
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
