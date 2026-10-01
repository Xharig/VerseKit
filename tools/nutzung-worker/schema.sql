-- Zähler je Tag (UTC). Keine Kennung, keine IP — nur „wie viele".
-- Einspielen: npx wrangler d1 execute versekit-nutzung --remote --file=schema.sql

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
