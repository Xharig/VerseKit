// SPDX-License-Identifier: GPL-3.0-only
//
// Die Seiten des Verse-Kit-Hauptfensters: Kennung → Gruppe und Name, in der
// Reihenfolge der Seitenleiste. Der Worker nimmt in den Seitenzählern nur
// diese Kennungen an; die Übersicht zeigt sie mit diesen Namen, auch mit 0.
//
// ⚠ Muss zu `scbp/pages.py` (`page_ids()`) und den Reitern in
// `scbp/main_window.py` passen — der Selbsttest vergleicht beides.

export const PAGES = {
  uebersicht: ['Start', 'Übersicht'],
  liste: ['Baupläne', 'Bauplan-Liste'],
  fortschritt: ['Baupläne', 'Bauplan-Fortschritt'],
  auftragslog: ['Baupläne', 'Aufträge & Protokoll'],
  hangar: ['Flotte & Logistik', 'Mein Hangar'],
  wunschliste: ['Flotte & Logistik', 'Wunschliste'],
  asop: ['Flotte & Logistik', 'Schiffe benennen'],
  meinlager: ['Flotte & Logistik', 'Mein Lager'],
  staffellager: ['Flotte & Logistik', 'Lager der Einheit'],
  // Alte Seitennamen, die ältere Fassungen melden (siehe MERGED).
  lager: ['Flotte & Logistik', 'Rohstofflager'],
  handelslager: ['Flotte & Logistik', 'Handelslager'],
  herstellung: ['Herstellung', 'Herstellung'],
  farmliste: ['Herstellung', 'Was ich farmen muss'],
  bergbau: ['Abbau & Bergung', 'Bergbau'],
  raffinerien: ['Abbau & Bergung', 'Raffinerien'],
  bergung: ['Abbau & Bergung', 'Was steckt drin?'],
  zerlegen: ['Abbau & Bergung', 'Lohnt das Zerlegen?'],
  laeden: ['Handel', 'Shops'],
  verkauf: ['Handel', 'Kaufen & Verkaufen'],
  routen: ['Handel', 'Routen'],
  bedarf: ['Beschaffung', 'Bedarf der Einheit'],
  einkaufsliste: ['Beschaffung', 'Einkaufsliste'],
  statistik_auswertung: ['Statistik', 'Auswertung'],
  statistik: ['Statistik', 'Übersicht'],
  statistik_schiffe: ['Statistik', 'Schiffe & Ausrüstung'],
  statistik_auftraege: ['Statistik', 'Aufträge'],
  statistik_quantum: ['Statistik', 'Quantenreisen'],
  statistik_stabil: ['Statistik', 'Stabilität'],
  allgemein: ['Einstellungen', 'Allgemein'],
  anzeige: ['Einstellungen', 'Overlay'],
  darstellung: ['Einstellungen', 'Darstellung'],
  ordner: ['Einstellungen', 'Installation & Pfade'],
  module: ['Einstellungen', 'Module'],
  basetool: ['Einstellungen', 'Verbindung & Abgleich'],
  bestand: ['Einstellungen', 'Sichern & Zurücksetzen'],
  spiel: ['Spiel-Einstellungen', 'Spiel'],
  uebersetzung: ['Spiel-Einstellungen', 'Übersetzung'],
  grafik: ['Spiel-Einstellungen', 'Grafik'],
  blickwinkel: ['Spiel-Einstellungen', 'FOV'],
  joysticks: ['Spiel-Einstellungen', 'Steuerung'],
  achsen: ['Spiel-Einstellungen', 'Achsen & Kurven'],
  shader: ['Spiel-Einstellungen', 'Shader-Cache'],
  wasistneu: ['Info', 'Was ist neu'],
  patchaenderungen: ['Info', 'Geänderte Spielwerte'],
  ueber: ['Info', 'Update & Über'],
  serverstatus: ['Info', 'Serverstatus'],
  diagnose: ['Info', 'Fehler melden'],
  danke: ['Info', 'Danke & Lizenzen'],
  erkennung: ['Für Fortgeschrittene', 'Erkennung'],
  startprogramme: ['Für Fortgeschrittene', 'Startprogramme'],
  // Bereichsseiten der Kartenansicht (`cards.area_page_id`).
  bereich_bauplaene: ['Karten', 'Bereich Baupläne'],
  bereich_logistik: ['Karten', 'Bereich Flotte & Logistik'],
  bereich_schiffe: ['Karten', 'Bereich Schiffe'],
  bereich_werkstatt: ['Karten', 'Bereich Herstellung'],
  bereich_bergung: ['Karten', 'Bereich Abbau & Bergung'],
  bereich_handel: ['Karten', 'Bereich Handel'],
  bereich_beschaffung: ['Karten', 'Bereich Beschaffung'],
  bereich_statistiken: ['Karten', 'Bereich Statistik'],
  bereich_einstellungen: ['Karten', 'Bereich Einstellungen'],
  bereich_spiel_einst: ['Karten', 'Bereich Spiel-Einstellungen'],
  bereich_basetool: ['Karten', 'Bereich Basetool'],
  bereich_info: ['Karten', 'Bereich Info'],
};

// Seiten ohne eigenen Reiter: Der Worker nimmt sie weiter an (ältere
// Fassungen melden sie), die Übersicht zählt sie bei der genannten Seite mit
// und zeigt sie nicht als eigene Zeile.
export const MERGED = {
  asop: 'hangar',
  lager: 'meinlager',
  handelslager: 'meinlager',
  bereich_schiffe: 'bereich_logistik',
};

// Die Wege, auf denen eine Seite geöffnet wird (`page_usage.ROUTES`).
export const ROUTES = {
  seitenleiste: 'Seitenleiste',
  sprung: 'Sprung im Fenster',
  overlay: 'Overlay',
  tray: 'Tray-Menü',
  start: 'Programmstart',
  karte: 'Karte',
  pfad: 'Pfadzeile',
};

// Die Handlungen, die das Programm zählt (`page_usage.ACTIONS`) — nur wie
// oft, nie womit. Andere Kennungen lehnt der Worker ab.
export const ACTIONS = {
  lager_hand: 'Mein Lager: Rohstoff von Hand eingetragen',
  lager_raffinerie: 'Mein Lager: Raffinerie-Ausbeute eingetragen',
  lager_scan: 'Mein Lager: Bildschirm gelesen',
  lager_sync: 'Mein Lager: Jetzt übertragen',
  abgleich_karte: 'Basetool: Karte „Jetzt abgleichen"',
  karte_raffinerie: 'Karte: Raffinerie-Scanner',
  karte_scanner: 'Karte: Mining-Scanner an/aus',
  karte_tr_pruefen: 'Karte: Übersetzung prüfen',
  karte_tr_neu: 'Karte: Übersetzung neu einlesen',
  galerie_offen: 'Galerie „Was ist neu" geöffnet',
  galerie_sprung: 'Galerie: zur Seite gesprungen',
  galerie_karten: 'Galerie: Kartenansicht ausprobiert',
};

// Die Kacheln der Startseite (`overview.TILES`) — für das Merkmal der
// ausgeblendeten Kacheln. Andere Kennungen lehnt der Worker ab.
export const TILES = {
  spiel: 'Spiel', standort: 'Standort', bauplaene: 'Baupläne',
  uebersetzung: 'Übersetzung', spielzeit: 'Spielzeit', rechner: 'Rechner',
  dienste: 'Dienste', raffinerie: 'Raffinerie', lager: 'Lager',
  auftraege: 'Aufträge', neu_patch: 'Neu im Patch', merkliste: 'Merkliste',
  bedarf: 'Bedarf der Einheit', funde: 'Letzte Funde', sicherung: 'Sicherung',
};

// Die Stufe MAX_CLICKS steht für so viele Klicks und mehr (`page_usage.MAX_CLICKS`).
export const MAX_CLICKS = 10;
export const MAX_COUNT = 9999;
export const MAX_MISS_PAIRS = 20;
