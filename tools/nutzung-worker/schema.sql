-- Zähler je Tag (UTC). Keine Kennung, keine IP — nur „wie viele".
-- Einspielen: npx wrangler d1 execute versekit-nutzung --remote --file=schema.sql
-- ⚠ Jede Tabelle mit IF NOT EXISTS und nur CREATE: Die Datei lässt sich
-- jederzeit erneut einspielen, vorhandene Tabellen und Zeilen bleiben
-- unberührt (Prüfung in worker.test.mjs).

-- Aktive Installationen je Tag, Programmversion, System und Land. Das Land ist
-- das Kürzel, das Cloudflare selbst aus der Anfrage ableitet
-- (`request.cf.country`) — keine Stadt, keine Region.
CREATE TABLE IF NOT EXISTS tage (
  tag     TEXT    NOT NULL,
  version TEXT    NOT NULL,
  system  TEXT    NOT NULL,
  land    TEXT    NOT NULL,
  n       INTEGER NOT NULL,
  PRIMARY KEY (tag, version, system, land)
);

-- Downloads über die Kurzlinks xharig.com/windows und xharig.com/linux, je
-- Tag, System und Land. Downloads direkt auf GitHub und Auto-Updates sieht
-- hier niemand — GitHub verrät kein Land.
CREATE TABLE IF NOT EXISTS downloads (
  tag     TEXT    NOT NULL,
  system  TEXT    NOT NULL,
  land    TEXT    NOT NULL,
  n       INTEGER NOT NULL,
  PRIMARY KEY (tag, system, land)
);

-- Weitere Merkmale, je eines für sich gezählt (Sprache, Testversionen,
-- Bereiche, Overlay, Autostart). Bewusst NICHT mit der Tabelle oben
-- verknüpft: Kombinationen würden kleine Gruppen erkennbar machen.
CREATE TABLE IF NOT EXISTS merkmale (
  tag     TEXT    NOT NULL,
  merkmal TEXT    NOT NULL,
  wert    TEXT    NOT NULL,
  n       INTEGER NOT NULL,
  PRIMARY KEY (tag, merkmal, wert)
);

-- Zuletzt geholte GitHub-Zahlen (je Seite mit ETag, dazu die Gesamtliste).
-- Antwortet GitHub nicht, zeigt die Übersicht diese — mit ihrer Uhrzeit.
CREATE TABLE IF NOT EXISTS ablage (
  schluessel TEXT    PRIMARY KEY,
  inhalt     TEXT    NOT NULL,
  zeit       INTEGER NOT NULL
);

-- Höchster je gesehener Stand je Version — sinkt nie, auch wenn ein Release
-- gelöscht wird (Prinzip von _Tools/downloads-mitschreiben.py, dessen Bestand
-- seit 06.09.2026 hier übernommen ist). `gesamt` kann größer sein als
-- windows + linux: Für übernommene Altdaten ist die Aufteilung unbekannt.
CREATE TABLE IF NOT EXISTS download_bestand (
  version         TEXT    PRIMARY KEY,
  windows         INTEGER NOT NULL DEFAULT 0,
  linux           INTEGER NOT NULL DEFAULT 0,
  gesamt          INTEGER NOT NULL DEFAULT 0,
  veroeffentlicht TEXT,
  vorab           INTEGER NOT NULL DEFAULT 0,
  erstmals        TEXT    NOT NULL,
  zuletzt         TEXT    NOT NULL
);

-- Ein Eintrag je Tag: Summen wie in verlauf.csv. `je_gesehen` sinkt nie —
-- daraus ergeben sich die Downloads je Tag.
CREATE TABLE IF NOT EXISTS download_verlauf (
  tag            TEXT    PRIMARY KEY,
  aktiv          INTEGER NOT NULL,
  je_gesehen     INTEGER NOT NULL,
  windows        INTEGER NOT NULL,
  linux          INTEGER NOT NULL,
  releases_aktiv INTEGER NOT NULL,
  releases_je    INTEGER NOT NULL
);

-- Seiten des Hauptfensters, je Tag zusammengezählt aus den Meldungen. Nur
-- Seiten-Kennungen aus pages.js — keine Inhalte, keine Zeitpunkte. Bewusst
-- NICHT mit `tage` verknüpft (keine Version, kein Land, kein System).

-- Aufrufe je Seite.
CREATE TABLE IF NOT EXISTS seiten (
  tag   TEXT    NOT NULL,
  seite TEXT    NOT NULL,
  n     INTEGER NOT NULL,
  PRIMARY KEY (tag, seite)
);

-- Auf welchem Weg eine Seite geöffnet wurde (Seitenleiste, Sprung, Overlay,
-- Tray-Menü, Programmstart).
CREATE TABLE IF NOT EXISTS seiten_wege (
  tag   TEXT    NOT NULL,
  seite TEXT    NOT NULL,
  weg   TEXT    NOT NULL,
  n     INTEGER NOT NULL,
  PRIMARY KEY (tag, seite, weg)
);

-- Klicks bis zur Zielseite: wie oft eine Seite nach `klicks` Klicks erreicht
-- wurde. `klicks` = 10 heißt „10 oder mehr".
CREATE TABLE IF NOT EXISTS seiten_klicks (
  tag    TEXT    NOT NULL,
  seite  TEXT    NOT NULL,
  klicks INTEGER NOT NULL,
  n      INTEGER NOT NULL,
  PRIMARY KEY (tag, seite, klicks)
);

-- Fehlgriffe: Seite `von` nach wenigen Sekunden wieder verlassen, danach
-- auf Seite `nach` geblieben.
CREATE TABLE IF NOT EXISTS seiten_fehlgriffe (
  tag  TEXT    NOT NULL,
  von  TEXT    NOT NULL,
  nach TEXT    NOT NULL,
  n    INTEGER NOT NULL,
  PRIMARY KEY (tag, von, nach)
);

-- Handlungen: wie oft eine Handlung aus ACTIONS (pages.js) genutzt wurde —
-- nur die Anzahl, keine Mengen, keine Namen.
CREATE TABLE IF NOT EXISTS seiten_handlungen (
  tag      TEXT    NOT NULL,
  handlung TEXT    NOT NULL,
  n        INTEGER NOT NULL,
  PRIMARY KEY (tag, handlung)
);
