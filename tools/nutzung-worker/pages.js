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
  hangar: ['Schiffe', 'Mein Hangar'],
  wunschliste: ['Schiffe', 'Wunschliste'],
  einkaufsliste: ['Schiffe', 'Was noch fehlt'],
  asop: ['Schiffe', 'Schiffe benennen'],
  lager: ['Werkstatt', 'Rohstofflager'],
  herstellung: ['Werkstatt', 'Herstellung'],
  bergbau: ['Werkstatt', 'Bergbau'],
  raffinerien: ['Werkstatt', 'Raffinerien'],
  laeden: ['Werkstatt', 'Shops'],
  farmliste: ['Werkstatt', 'Was ich farmen muss'],
  bedarf: ['Basetool', 'Bedarf der Einheit'],
  bergung: ['Bergung', 'Was steckt drin?'],
  zerlegen: ['Bergung', 'Lohnt das Zerlegen?'],
  handelslager: ['Handel', 'Handelslager'],
  verkauf: ['Handel', 'Kaufen & Verkaufen'],
  routen: ['Handel', 'Routen'],
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
  basetool: ['Basetool', 'Verbindung & Abgleich'],
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
  bereich_schiffe: ['Karten', 'Bereich Schiffe'],
  bereich_werkstatt: ['Karten', 'Bereich Werkstatt'],
  bereich_bergung: ['Karten', 'Bereich Bergung'],
  bereich_handel: ['Karten', 'Bereich Handel & Einkauf'],
  bereich_statistiken: ['Karten', 'Bereich Statistik'],
  bereich_spiel_einst: ['Karten', 'Bereich Spiel-Einstellungen'],
  bereich_einstellungen: ['Karten', 'Bereich Einstellungen'],
  bereich_basetool: ['Karten', 'Bereich Basetool'],
  bereich_info: ['Karten', 'Bereich Info'],
};

// Seiten ohne eigenen Reiter: Der Worker nimmt sie weiter an (ältere
// Fassungen melden sie), die Übersicht zählt sie bei der genannten Seite mit
// und zeigt sie nicht als eigene Zeile.
export const MERGED = {
  asop: 'hangar',
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
  lager_hand: 'Rohstofflager: von Hand eingetragen',
  lager_raffinerie: 'Rohstofflager: Raffinerie-Ausbeute eingetragen',
  lager_scan: 'Rohstofflager: Bildschirm gelesen',
  lager_sync: 'Rohstofflager: Jetzt übertragen',
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
