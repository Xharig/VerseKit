/* Der Rundgang.
   ⚠ Gruppen und Reihenfolge sind KEINE eigene Erfindung — sie stehen genau
   so in `scbp/main_window.py` (die `_tab(...)`-Aufrufe je Gruppe). Wer dort
   einen Reiter verschiebt oder umbenennt, zieht diese Liste mit.
   Aufgenommen sind nur Bereiche, von denen es ein Bild gibt.
   Die englischen Bilder heissen wie die deutschen, nur mit "-en" vor der
   Endung — so erzeugt sie tools/bilder_machen.py. */
// Site root, taken from this script's own address, so that "/" and "/en/"
// load the same assets and files.
var WURZEL = document.currentScript.src.replace(/assets\/[^\/]*$/, "");

var GRUPPEN = [
  {de:"Overlay", en:"Overlay", eintraege:[
    {reiter:"overlay", bild:"screenshot-overlay.png", de:"Overlay", en:"Overlay",
     bde:"Das kleine Fenster über dem Spiel: schmal, immer im Vordergrund, durchscheinend. Auf Wunsch lässt es Mausklicks durch.",
     ben:"The small window over the game: narrow, always on top, see-through, and it can let mouse clicks pass through."}
  ]},
  {de:"Übersicht", en:"Overview", eintraege:[
    {reiter:"uebersicht", bild:"screenshot-uebersicht.png", de:"Übersicht", en:"Overview",
     bde:"Das Wichtigste auf einen Blick: Baupläne, Übersetzung je Kanal, dein Rechner, Server-Lage. Kacheln ein- und ausblenden und verschieben.",
     ben:"The essentials at a glance: blueprints, translation per channel, your PC, server status. Show, hide and move the tiles."}
  ]},
  {de:"Baupläne", en:"Blueprints", eintraege:[
    {reiter:"liste", bild:"screenshot-liste.png", de:"Bauplan-Liste", en:"Blueprint list",
     bde:"Alle Baupläne durchsuchen, nach Art gruppiert, mit fünf Filtern und Häkchen per Klick.",
     ben:"Search every blueprint, grouped by type, with five filters and tick boxes."},
    {reiter:"fortschritt", bild:"screenshot-fortschritt.png", de:"Bauplan-Fortschritt", en:"Blueprint progress",
     bde:"Je Art siehst du, wie weit du bist und was dir bis zur Vollständigkeit fehlt.",
     ben:"Per type you see how far along you are and what is missing to complete it."},
    {reiter:"auftragslog", bild:"screenshot-auftragslog.png", de:"Aufträge & Protokoll", en:"Missions & log",
     bde:"Welchen Auftrag du wann gespielt hast und welcher Bauplan dabei herauskam.",
     ben:"Which contract you played when, and which blueprint came out of it."}
  ]},
  {de:"Flotte & Logistik", en:"Fleet & Logistics", eintraege:[
    {reiter:"hangar", bild:"screenshot-hangar.png", de:"Mein Hangar", en:"My hangar",
     bde:"Deine Flotte, in einem Zug aus dem Pledge-Store geholt. Danach weiß das Werkzeug, was in welches Schiff passt. An jedem Schiff vergibst du auch seinen Namen im Abrufterminal.",
     ben:"Your fleet, pulled in from the pledge store in one go. After that the tool knows what fits into which ship. Each ship also gets its name for the retrieval terminal right there."},
    {reiter:"wunschliste", bild:"screenshot-wunschliste.png", de:"Wunschliste", en:"Wishlist",
     bde:"Was du dir vornimmst, mit Preis und Ort. Die Ausstattung planst du, bevor du das Schiff hast.",
     ben:"What you are aiming for, with price and location. Plan the loadout before you own the ship."},
    {reiter:"meinlager", bild:"screenshot-meinlager.png", de:"Mein Lager", en:"My Inventory",
     bde:"Rohstoffe, Verkaufsware und Items an einer Stelle: mit Art, Ort, Güte und Menge, gefiltert nach Material oder Items und zum Herstellen oder Verkaufen.",
     ben:"Materials, goods for sale and items in one place: with type, location, quality and amount, filtered by material or items and for crafting or selling."},
    {reiter:"staffellager", bild:"screenshot-staffellager.png", de:"Lager der Einheit", en:"Unit storage",
     bde:"Mit dem KRT Profit Basetool verbunden: was im gemeinsamen Lager deiner Einheit liegt. Logistiker sehen, wem was gehört, und buchen direkt.",
     ben:"Connected to the KRT Profit Basetool: what is in your unit's shared storage. Logisticians see who owns what and book directly."}
  ]},
  {de:"Herstellung", en:"Crafting", eintraege:[
    {reiter:"herstellung", bild:"screenshot-herstellung.png", de:"Herstellung", en:"Crafting",
     bde:"Zutaten, Mengen und Herstellzeit zu jedem Gegenstand, und ob du den Bauplan dafür hast.",
     ben:"Ingredients, amounts and crafting time for every item, and whether you have the blueprint."},
    {reiter:"farmliste", bild:"screenshot-farmliste.png", de:"Was ich farmen muss", en:"What to farm",
     bde:"Dein Lager gegen alles gerechnet, was du bauen willst, über alle Posten zusammen.",
     ben:"Your storage measured against everything you want to build, across all items at once."}
  ]},
  {de:"Abbau & Bergung", en:"Mining & Salvage", eintraege:[
    {reiter:"bergbau", bild:"screenshot-bergbau.png", de:"Bergbau", en:"Mining",
     bde:"Rohstoff eintippen und die Fundorte sehen, oder den Ort eintippen und sehen, was es dort gibt.",
     ben:"Type a material to see where it is found, or type a location to see what is there."},
    {reiter:"raffinerien", bild:"screenshot-raffinerien.png", de:"Raffinerien", en:"Refineries",
     bde:"Was jede Raffinerie aus deinem Erz macht: Aufschlag und Abschlag je Material, der beste Ort grün.",
     ben:"What every refinery makes of your ore: bonus and penalty per material, the best place in green."},
    {reiter:"bergung", bild:"screenshot-bergung.png", de:"Was steckt drin?", en:"What's inside?",
     bde:"Was ein Wrack ab Werk an Bord hat und was die Teile im Laden wert sind.",
     ben:"What a wreck carries from the factory, and what those parts are worth in a shop."},
    {reiter:"zerlegen", bild:"screenshot-zerlegen.png", de:"Lohnt das Zerlegen?", en:"Worth dismantling?",
     bde:"Was der Fabricator zurückgibt, bevor du den Schneidbrenner ansetzt.",
     ben:"What the fabricator gives back, before you fire up the cutter."}
  ]},
  {de:"Handel", en:"Trade", eintraege:[
    {reiter:"laeden", bild:"screenshot-laeden.png", de:"Shops", en:"Shops",
     bde:"Wo das fertige Teil im Regal steht und was es dort kostet. Der günstigste Laden steht oben.",
     ben:"Where the finished part sits on the shelf and what it costs. Cheapest shop first."},
    {reiter:"verkauf", bild:"screenshot-verkauf.png", de:"Kaufen & Verkaufen", en:"Buy & Sell",
     bde:"Wo du deine Ware los wirst, sortiert danach, wie viel ein Ort auf einmal abnimmt, und wo du sie am günstigsten bekommst.",
     ben:"Where to sell your cargo, sorted by how much a location takes at once, and where to buy it cheapest."},
    {reiter:"routen", bild:"screenshot-routen.png", de:"Routen", en:"Routes",
     bde:"Sag, wo du stehst, wie viel reinpasst und was du ausgeben willst. Dann rechnet VerseKit, welche Fahrt sich lohnt.",
     ben:"Say where you are, how much fits and what you want to spend. VerseKit works out which run pays."}
  ]},
  {de:"Beschaffung", en:"Procurement", eintraege:[
    {reiter:"bedarf", bild:"screenshot-bedarf.png", de:"Bedarf der Einheit", en:"Unit demand",
     bde:"Mit dem KRT Profit Basetool verbunden: was die offenen Aufträge deiner Einheit noch brauchen, und was du davon schon hast.",
     ben:"Connected to the KRT Profit Basetool: what your unit's open orders still need, and what of it you already have."},
    {reiter:"einkaufsliste", bild:"screenshot-einkaufsliste.png", de:"Einkaufsliste", en:"Shopping list",
     bde:"Die Rechnung über alle Schiffe: kaufen oder selbst bauen, mit Summe und Einkaufsroute.",
     ben:"The bill across all your ships: buy or build, with a total and a shopping route."}
  ]},
  {de:"Statistik", en:"Statistics", eintraege:[
    {reiter:"statistik_auswertung", bild:"screenshot-statistik-auswertung.png", de:"Auswertung", en:"Evaluation",
     bde:"Hier entsteht die Statistik: Jedes Spielprotokoll wird einmal gelesen und festgehalten, auch wenn Star Citizen es später löscht.",
     ben:"This is where the statistics come from: every game log is read once and kept, even after Star Citizen deletes it."},
    {reiter:"statistik", bild:"screenshot-statistik.png", de:"Übersicht", en:"Overview",
     bde:"Spielzeit, Sitzungen, abgeschlossene Aufträge und Quantensprünge, dazu eine Wärmekarte, wann du spielst. Bleibt erhalten, auch wenn die Logs gelöscht werden.",
     ben:"Play time, sessions, completed contracts and quantum jumps, plus a heat map of when you play. Kept even when the logs are deleted."},
    {reiter:"statistik_schiffe", bild:"screenshot-statistik-schiffe.png", de:"Schiffe & Ausrüstung", en:"Ships & equipment",
     bde:"Womit du fliegst, was du verlierst und welche Waffen du trägst.",
     ben:"What you fly, what you lose and which weapons you carry."},
    {reiter:"statistik_auftraege", bild:"screenshot-statistik-auftraege.png", de:"Aufträge", en:"Contracts",
     bde:"Wie deine Aufträge ausgingen (abgeschlossen, fehlgeschlagen, abgebrochen) und welche du am häufigsten spielst.",
     ben:"How your contracts ended (completed, failed, abandoned) and which ones you play most."},
    {reiter:"statistik_quantum", bild:"screenshot-statistik-quantum.png", de:"Quantenreisen", en:"Quantum travel",
     bde:"Deine Quantensprünge: wohin du am häufigsten springst und wo deine Reisen beginnen.",
     ben:"Your quantum jumps: where you travel most and where your trips begin."},
    {reiter:"statistik_stabil", bild:"screenshot-statistik-stabil.png", de:"Stabilität", en:"Stability",
     bde:"Wie deine Sitzungen endeten: sauber beendet, abgestürzt oder die Verbindung verloren, und warum.",
     ben:"How your sessions ended: closed cleanly, crashed or disconnected, and why."}
  ]},
  {de:"Einstellungen", en:"Settings", eintraege:[
    {reiter:"allgemein", bild:"screenshot-allgemein.png", de:"Allgemein", en:"General",
     bde:"Start mit Windows, Signalton, Symbol in der Ablage: wie sich VerseKit im Hintergrund verhält.",
     ben:"Start with Windows, alert sound, tray icon: how VerseKit behaves in the background."},
    {reiter:"anzeige", bild:"screenshot-anzeige.png", de:"Overlay", en:"Overlay",
     bde:"Alles zum kleinen Fenster über dem Spiel: wann und wo es erscheint, wie durchsichtig es ist, ob Klicks durchgehen.",
     ben:"Everything about the small window above the game: when and where it shows, how transparent it is, whether clicks pass through."},
    {reiter:"darstellung", bild:"screenshot-darstellung.png", de:"Darstellung", en:"Appearance",
     bde:"Sechs Farbschemata, Schriftgröße stufenlos: So sieht VerseKit aus, wie du es magst.",
     ben:"Six colour schemes, text size without steps: VerseKit looks the way you like it."},
    {reiter:"ordner", bild:"screenshot-ordner.png", de:"Installation & Pfade", en:"Installation & paths",
     bde:"Wo Star Citizen liegt und wohin VerseKit deine Daten schreibt. Beim Umziehen kommt alles mit.",
     ben:"Where Star Citizen lives and where VerseKit keeps your data. Moving it takes everything along."},
    {reiter:"module", bild:"screenshot-module.png", de:"Module", en:"Modules",
     bde:"Blende Seiten aus, die du nicht brauchst, je Modul. Eine leere Gruppe verschwindet. Gelöscht wird nichts.",
     ben:"Hide pages you do not need, per module. An empty group disappears. Nothing gets deleted."},
    {reiter:"basetool", bild:"screenshot-basetool.png", de:"Verbindung & Abgleich", en:"Connection & sync",
     bde:"Baupläne, Lager und Hangar mit dem KRT Profit Basetool abgleichen: in beide Richtungen, nur wenn du es verbindest.",
     ben:"Sync blueprints, storage and hangar with the KRT Profit Basetool: both ways, only if you connect it."},
    {reiter:"bestand", bild:"screenshot-bestand.png", de:"Sichern & Zurücksetzen", en:"Backup & reset",
     bde:"Alles sichern und zurückholen, deinen Bestand austauschen, auch ohne fremde Werkzeuge.",
     ben:"Back up and restore everything, exchange your inventory, without any other tool."},
    {reiter:"erkennung", bild:"screenshot-erkennung.png", de:"Erkennung", en:"Detection",
     bde:"Wie VerseKit einen neuen Bauplan erkennt und welcher Account zählt, wenn mehrere auf einem Rechner spielen.",
     ben:"How VerseKit spots a new blueprint and which account counts when several play on one PC."},
    {reiter:"startprogramme", bild:"screenshot-startprogramme.png", de:"Startprogramme", en:"Startup programs",
     bde:"Startet weitere Programme mit Star Citizen, etwa ein Sprachprogramm oder ein Werkzeug für die Steuerung.",
     ben:"Starts other programs along with Star Citizen, such as a voice tool or a controls utility."}
  ]},
  {de:"Spiel-Einstellungen", en:"Game settings", eintraege:[
    {reiter:"spiel", bild:"screenshot-auftragstexte.png", de:"Spiel", en:"Game",
     bde:"VerseKit schreibt in die Auftragstexte, welche Baupläne dabei sind und welche dir fehlen. Dazu zeigt es dir deine user.cfg.",
     ben:"VerseKit writes into the contract texts which blueprints are included and which you lack. It also shows you your user.cfg."},
    {reiter:"uebersetzung", bild:"screenshot-uebersetzung.png", de:"Übersetzung", en:"Translation",
     bde:"Deutsch, Englisch, Français, Italiano, Español, Português, Türkçe oder eine eigene Adresse. Je Kanal, also LIVE und PTU getrennt.",
     ben:"German, English, French, Italian, Spanish, Portuguese, Turkish or an address of your own. Per channel, so LIVE and PTU separately."},
    {reiter:"grafik", bild:"screenshot-grafik.png", de:"Grafik", en:"Graphics",
     bde:"Fenstermodus, Upscaling, Bewegungsunschärfe, Schärfen: die wichtigsten Grafik-Schalter, ohne ins Spiel zu müssen. Vorher wird gesichert.",
     ben:"Window mode, upscaling, motion blur, sharpening: the most important graphics switches without starting the game. Backed up first."},
    {reiter:"blickwinkel", bild:"screenshot-blickwinkel.png", de:"FOV", en:"Field of view",
     bde:"Sitzt du richtig vor deinem Bildschirm? Breite mit einer Bankkarte messen, Abstand angeben, fertig.",
     ben:"Are you sitting right in front of your screen? Measure the width with a bank card, enter the distance, done."},
    {reiter:"joysticks", bild:"screenshot-joysticks.png", de:"Steuerung", en:"Controls",
     bde:"Welcher Stick welche Nummer hat, ob alle verbunden sind, und was auf welcher Taste liegt.",
     ben:"Which stick has which number, whether all are connected, and what is bound to which button."},
    {reiter:"achsen", bild:"screenshot-achsen.png", de:"Achsen & Kurven", en:"Axes & curves",
     bde:"Totzone, Sättigung und Kurve je Achse, mit Vorschau, was davon im Spiel wirklich ankommt.",
     ben:"Dead zone, saturation and curve per axis, with a preview of what actually reaches the game."},
    {reiter:"shader", bild:"screenshot-shader.png", de:"Shader-Cache", en:"Shader cache",
     bde:"Grafikfehler oder Ruckler nach einem Patch? Ein Knopf leert die Shader-Speicher von Star Citizen und dem Grafiktreiber.",
     ben:"Graphics glitches or stutter after a patch? One button empties the shader caches of Star Citizen and the graphics driver."}
  ]},
  {de:"Info", en:"Info", eintraege:[
    {reiter:"patchaenderungen", bild:"screenshot-patchaenderungen.png", de:"Geänderte Spielwerte", en:"Changed game values",
     bde:"Was ein Spiel-Patch an Werten geändert hat: Schiffe, Waffen, Komponenten, Lackierungen.",
     ben:"What a game patch changed in the numbers: ships, weapons, components, paints."},
    {reiter:"serverstatus", bild:"screenshot-serverstatus.png", de:"Serverstatus", en:"Server status",
     bde:"Läuft Star Citizen gerade? Was CIG meldet, frischt sich jede Minute selbst auf.",
     ben:"Is Star Citizen up? What CIG reports, refreshing itself every minute."},
    {reiter:"danke", bild:"screenshot-danke.png", de:"Danke & Lizenzen", en:"Thanks & Licenses",
     bde:"Wer geholfen hat, steht im Programm. Wer einen Fehler meldet, wird namentlich genannt.",
     ben:"Everyone who helped is named in the program. Whoever reports a bug is named."}
  ]}
];

/* Die Farbschemata aus `scbp/theme.py` (SCHEMES) — Namen wie im Programm,
   Punkt in Akzentfarbe auf Grundfarbe. Bilder: tools/bilder_machen.py --schema=<kennung>.
   Prüfung 217 hält fest, dass jedes Schema hier steht und seine Bilder hat. */
var FARBSCHEMATA = [
  {schema:"original", de:"VerseKit (Original)", en:"VerseKit (original)", akzent:"#9ce430", grund:"#10141c"},
  {schema:"krt", de:"KRT (Orange)", en:"KRT (orange)", akzent:"#e77e23", grund:"#000000"},
  {schema:"eis", de:"Eis (Cyan)", en:"Ice (cyan)", akzent:"#38d6f5", grund:"#0b1219"},
  {schema:"nebel", de:"Nebel (Violett)", en:"Nebula (violet)", akzent:"#a78bfa", grund:"#131020"},
  {schema:"glut", de:"Glut (Rot)", en:"Ember (red)", akzent:"#ff5a4f", grund:"#151515"},
  {schema:"kontrast", de:"Hoher Kontrast", en:"High contrast", akzent:"#ffd400", grund:"#000000"}
];
var farbLage = "original";

var sprLage = "de";
var lage = 0;
var flach = [];

function bauen(){
  var ziel = document.getElementById("reiter");
  if (!ziel) return;       // the tour lives on rundgang/ only
  ziel.innerHTML = "";
  flach = [];
  var nr = 0;
  for (var g = 0; g < GRUPPEN.length; g++){
    var kopf = document.createElement("div");
    kopf.className = "gruppe";
    kopf.textContent = GRUPPEN[g][sprLage];
    ziel.appendChild(kopf);
    for (var e = 0; e < GRUPPEN[g].eintraege.length; e++){
      var eintrag = GRUPPEN[g].eintraege[e];
      flach.push(eintrag);
      var knopf = document.createElement("button");
      knopf.type = "button";
      knopf.textContent = eintrag[sprLage];
      knopf.setAttribute("data-nr", nr);
      knopf.onclick = (function(i){ return function(){ zeigen(i); }; })(nr);
      ziel.appendChild(knopf);
      nr++;
    }
  }
  zeigen(lage);
}

function zeigen(i){
  lage = i;
  var eintrag = flach[i];
  var datei = eintrag.bild;
  if (sprLage === "en"){
    datei = datei.replace(/\.(png|jpg)$/, "-en.$1");
  }
  // „Darstellung" zeigt das gewählte Farbschema, darunter die Auswahl.
  var farben = document.getElementById("farben");
  farben.hidden = (eintrag.reiter !== "darstellung");
  if (eintrag.reiter === "darstellung"){
    datei = "screenshot-farbschema-" + farbLage + (sprLage === "en" ? "-en" : "") + ".png";
    farbenBauen();
  }
  var bild = document.getElementById("tourbild");
  bild.src = WURZEL + "assets/" + datei;
  bild.alt = eintrag[sprLage] + " in VerseKit";
  document.getElementById("tourtext").textContent =
    eintrag[sprLage === "de" ? "bde" : "ben"];
  var knoepfe = document.querySelectorAll("#reiter button");
  for (var k = 0; k < knoepfe.length; k++){
    knoepfe[k].className = (k === i) ? "an" : "";
    knoepfe[k].setAttribute("aria-pressed", k === i ? "true" : "false");
  }
}

function farbenBauen(){
  var ziel = document.getElementById("farben");
  ziel.innerHTML = "";
  for (var f = 0; f < FARBSCHEMATA.length; f++){
    var schema = FARBSCHEMATA[f];
    var knopf = document.createElement("button");
    knopf.type = "button";
    knopf.className = (schema.schema === farbLage) ? "an" : "";
    knopf.setAttribute("aria-pressed", schema.schema === farbLage ? "true" : "false");
    var punkt = document.createElement("span");
    punkt.className = "punkt";
    punkt.style.background = schema.akzent;
    punkt.style.boxShadow = "0 0 0 3px " + schema.grund + ", 0 0 0 4px #555";
    knopf.appendChild(punkt);
    knopf.appendChild(document.createTextNode(schema[sprLage]));
    knopf.onclick = (function(k){ return function(){ farbLage = k; zeigen(lage); }; })(schema.schema);
    ziel.appendChild(knopf);
  }
}

function sprache(s){
  sprLage = s;
  document.documentElement.lang = s;
  document.getElementById("knopf-de").className = (s === "de") ? "an" : "";
  document.getElementById("knopf-en").className = (s === "en") ? "an" : "";
  var texte = document.querySelectorAll("[data-de]");
  for (var i = 0; i < texte.length; i++){
    texte[i].textContent = texte[i].getAttribute("data-" + s);
  }
  // Feste Bilder (Abschnitt „Zwei Ansichten“): englisch mit "-en" vor der Endung.
  var bilder = document.querySelectorAll("img[data-bild]");
  for (var b = 0; b < bilder.length; b++){
    var name = bilder[b].getAttribute("data-bild");
    bilder[b].src = WURZEL + "assets/" + (s === "en" ? name.replace(/\.png$/, "-en.png") : name);
  }
  zahlZeigen();
  filmSprache();
  anderesZeigen();
  bauen();
  verlaufLaden();
}

/* Der große Knopf oben lädt direkt die Datei für das System des Besuchers.
   Nur Windows und Linux gibt es — Mac, Telefon und Unbekanntes springen wie
   bisher zum Abschnitt „Herunterladen", wo beide Dateien stehen.
   Die Texte laufen über data-de/data-en, damit `sprache()` sie mitnimmt. */
var system = (function(){
  var p = ((navigator.userAgentData && navigator.userAgentData.platform) ||
           navigator.platform || "") + " " + navigator.userAgent;
  if (/Android|iPhone|iPad/i.test(p)) return null;
  if (/Win/i.test(p)) return "windows";
  if (/Linux|X11/i.test(p)) return "linux";
  return null;
})();

// Every `.knopf-system` button (top of the page and the closing band) gets
// the file for the visitor's system; its text sits in `.knopf-text`, next to
// the icon.
function knopfRichten(){
  if (!system) return;
  var name = system === "windows" ? "Windows" : "Linux";
  var knoepfe = document.querySelectorAll(".knopf-system");
  for (var i = 0; i < knoepfe.length; i++){
    var text = knoepfe[i].querySelector(".knopf-text");
    knoepfe[i].href = "https://xharig.com/" + system;
    text.setAttribute("data-de", "Für " + name + " herunterladen");
    text.setAttribute("data-en", "Download for " + name);
    text.textContent = text.getAttribute("data-" + sprLage);
  }
  anderesZeigen();
}

function anderesZeigen(){
  if (!system || !document.getElementById("anderes")) return;
  var andere = system === "windows" ? "Linux" : "Windows";
  document.getElementById("anderes-link").textContent = sprLage === "de"
    ? "Andere Systeme: " + andere : "Other systems: " + andere;
  document.getElementById("anderes").hidden = false;
}

/* Das Video in der Sprache der Seite. Es startet nie von selbst: Vorschaubild,
   abgespielt wird erst auf Klick über die Bedienleiste des Browsers. */
var film = document.getElementById("film");
var FILM_STAND = "?v=2";

function filmSprache(){
  if (!film) return;       // the video is on the start page only
  var ziel = WURZEL + "assets/video/versekit-" + sprLage + ".mp4" + FILM_STAND;
  if (film.getAttribute("src") === ziel) return;
  film.pause();
  film.setAttribute("poster", WURZEL + "assets/video/versekit-" + sprLage + ".jpg" + FILM_STAND);
  film.setAttribute("src", ziel);
  film.setAttribute("aria-label", sprLage === "de"
    ? "VerseKit in 41 Sekunden" : "VerseKit in 41 seconds");
}
knopfRichten();

/* Downloadzahl: Summe aller Release-Dateien aller Versionen — dieselbe Zahl,
   die der Discord-Kanal zeigt. Ueber 100 Releases → alle Seiten lesen.
   Zehn Minuten im sessionStorage, damit Blaettern nicht jedes Mal fragt
   (GitHub erlaubt 60 Abfragen je Stunde und Besucher). */
var downloads = null;
var fassung = null;      // Nummer der neuesten fertigen Fassung, z. B. "v3.53.2"
var groessen = null;     // {windows: Bytes, linux: Bytes} der neuesten Fassung
var vtLink = null;       // VirusTotal-Bericht der neuesten Setup.exe

/* GitHub nennt zu jedem Anhang die SHA-256 (`digest`), und VirusTotal fuehrt
   den Bericht unter genau dieser Pruefsumme. Fehlt sie, bleibt der Link auf
   der Release-Seite stehen — dort steht die Zeile aus dem Bau. */
function vtAus(release){
  for (var k = 0; k < release.assets.length; k++){
    var a = release.assets[k];
    if (a.name === "VerseKit-Setup.exe" && /^sha256:[0-9a-f]{64}$/.test(a.digest || ""))
      return "https://www.virustotal.com/gui/file/" + a.digest.slice(7);
  }
  return null;
}

// Sets an element's text if the page has it; the numbers appear on the start
// page (fact sheet) and on herunterladen/ (version line, VirusTotal link).
function textSetzen(id, text){
  var el = document.getElementById(id);
  if (el) el.textContent = text;
  return el;
}

function zeigeZeile(id){
  var el = document.getElementById(id);
  if (el) el.hidden = false;
}

function vtZeigen(){
  var link = document.getElementById("vt-bericht");
  if (vtLink && link) link.href = vtLink;
}

function groessenAus(release){
  var g = {};
  for (var k = 0; k < release.assets.length; k++){
    var n = release.assets[k].name;
    if (/\.exe$/i.test(n)) g.windows = release.assets[k].size;
    if (/\.AppImage$/i.test(n)) g.linux = release.assets[k].size;
  }
  return (g.windows || g.linux) ? g : null;
}

function mb(bytes){
  var z = Math.round(bytes / 1048576);
  return z.toLocaleString(sprLage === "de" ? "de-DE" : "en-US") + " MB";
}

function kennzahlenZeigen(){
  if (fassung){
    textSetzen("kz-version-wert", fassung);
    textSetzen("fassung", fassung);
    zeigeZeile("kz-version");
  }
  if (groessen){
    var teile = [];
    if (groessen.windows) teile.push("Windows " + mb(groessen.windows));
    if (groessen.linux) teile.push("Linux " + mb(groessen.linux));
    textSetzen("kz-groesse-wert", teile.join(", "));
    zeigeZeile("kz-groesse");
  }
}

function zahlZeigen(){
  kennzahlenZeigen();
  if (downloads === null) return;
  textSetzen("downloads-zahl", downloads.toLocaleString(sprLage === "de" ? "de-DE" : "en-US"));
  zeigeZeile("downloads");
}

function zahlHolen(seite, summe){
  /* Never from the browser cache: Safari on an iPhone kept showing a two
     weeks old version (v3.70.1) from it, a private tab showed the right one.
     The ten minutes in the sessionStorage still spare the GitHub limit. */
  fetch("https://api.github.com/repos/Xharig/VerseKit/releases?per_page=100&page=" + seite, {cache: "no-store"})
    .then(function(r){ if (!r.ok) throw r.status; return r.json(); })
    .then(function(liste){
      for (var i = 0; i < liste.length; i++){
        /* Die Liste kommt neueste zuerst. Testfassungen (-rc) ueberspringen —
           die Knoepfe laden ebenfalls nur fertige Fassungen. */
        if (fassung === null && !liste[i].prerelease && !liste[i].draft){
          fassung = liste[i].tag_name;
          groessen = groessenAus(liste[i]);
          vtLink = vtAus(liste[i]);
          vtZeigen();
        }
        for (var k = 0; k < liste[i].assets.length; k++){
          summe += liste[i].assets[k].download_count;
        }
      }
      if (liste.length === 100) return zahlHolen(seite + 1, summe);
      downloads = summe;
      try { sessionStorage.setItem("vk-downloads", JSON.stringify({n:summe, v:fassung, g:groessen, vt:vtLink, t:Date.now()})); } catch (e) {}
      zahlZeigen();
    })
    .catch(function(){});
}

try {
  var gemerkt = JSON.parse(sessionStorage.getItem("vk-downloads") || "null");
  if (gemerkt && Date.now() - gemerkt.t < 600000){
    downloads = gemerkt.n;
    if (gemerkt.v) fassung = gemerkt.v;
    if (gemerkt.g) groessen = gemerkt.g;
    if (gemerkt.vt){
      vtLink = gemerkt.vt;
      vtZeigen();
    }
  }
} catch (e) {}
if (downloads === null) zahlHolen(1, 0); else zahlZeigen();

/* Der Verlauf.
   ⚠ Es gibt KEINE Kopie der Changelogs auf dieser Seite. Gelesen werden
   CHANGELOG.md (deutsch) und CHANGELOG.en.md (englisch) aus dem Repo — GitHub
   Pages liefert sie vom selben Server aus wie diese Seite, es geht also
   nichts an Dritte. Eine abgeschriebene Liste waere die zweite Wahrheit und
   veraltete still beim naechsten Release.
   ⚠ Der Umwandler kennt nur, was die Changelogs wirklich benutzen:
   "## Version", "### Abschnitt", Aufzaehlungen (zwei Ebenen), "> " als
   Vorspann, "> [!important]" als Kasten, **fett**, `Code` und [Links](...).
   Alles wird VOR dem Einsetzen maskiert — kein Text aus der Datei wird je
   als HTML ausgefuehrt. */
var VERLAUF_DATEI = {de:"CHANGELOG.md", en:"CHANGELOG.en.md"};
var VERLAUF_ERSTE = 15;          // so viele Versionen stehen sofort da
var verlaufDaten = {};           // Sprache -> Liste der Versionen
var verlaufAlle = false;
var verlaufGeladen = null;

function maske(s){
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
          .replace(/"/g, "&quot;");
}

function zeile(s){
  var t = maske(s);
  t = t.replace(/`([^`]+)`/g, "<code>$1</code>");
  // Fett darf Kursives enthalten: „**Eigener Bereich *Schiffe***" (v3.19.0-rc2)
  t = t.replace(/\*\*((?:[^*]|\*[^*]+\*)+)\*\*/g, "<strong>$1</strong>");
  t = t.replace(/\*([^*\s][^*]*)\*/g, "<em>$1</em>");
  t = t.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, function(_, text, ziel){
    // Nur echte Adressen und Verweise auf Dateien im Repo — kein "javascript:".
    if (/^https:\/\//.test(ziel)) return '<a href="' + ziel + '">' + text + "</a>";
    if (/^[\w.\/#-]+$/.test(ziel))
      return '<a href="https://github.com/Xharig/VerseKit/blob/main/' + ziel + '">' + text + "</a>";
    return text;
  });
  return t;
}

function umwandeln(zeilen){
  var html = "", absatz = [], liste = null, zitat = null;

  function absatzEnde(){
    if (absatz.length) html += "<p>" + zeile(absatz.join(" ")) + "</p>";
    absatz = [];
  }
  function listeEnde(){
    if (!liste) return;
    html += "<ul>";
    for (var i = 0; i < liste.length; i++){
      html += "<li>" + zeile(liste[i].text.join(" "));
      if (liste[i].unter.length){
        html += "<ul>";
        for (var k = 0; k < liste[i].unter.length; k++)
          html += "<li>" + zeile(liste[i].unter[k].join(" ")) + "</li>";
        html += "</ul>";
      }
      html += "</li>";
    }
    html += "</ul>";
    liste = null;
  }
  function zitatEnde(){
    if (!zitat) return;
    var teile = [], jetzt = [];
    for (var i = 0; i < zitat.zeilen.length; i++){
      if (zitat.zeilen[i] === "") { if (jetzt.length) teile.push(jetzt); jetzt = []; }
      else jetzt.push(zitat.zeilen[i]);
    }
    if (jetzt.length) teile.push(jetzt);
    var innen = "";
    for (var t = 0; t < teile.length; t++) innen += "<p>" + zeile(teile[t].join(" ")) + "</p>";
    html += '<div class="' + (zitat.kasten ? "kasten" : "vorspann") + '">' + innen + "</div>";
    zitat = null;
  }
  function allesEnde(){ absatzEnde(); listeEnde(); zitatEnde(); }

  for (var n = 0; n < zeilen.length; n++){
    var z = zeilen[n];
    if (/^>/.test(z)){
      absatzEnde(); listeEnde();
      var rest = z.replace(/^> ?/, "");
      if (/^\[!\w+\]/.test(rest)){
        zitatEnde();
        zitat = {kasten:true, zeilen:[]};
        rest = rest.replace(/^\[!\w+\]\s*/, "");
        if (rest) zitat.zeilen.push(rest);
        continue;
      }
      if (!zitat) zitat = {kasten:false, zeilen:[]};
      zitat.zeilen.push(rest.trim());
      continue;
    }
    zitatEnde();
    if (/^### /.test(z)){ allesEnde(); html += "<h3>" + zeile(z.slice(4)) + "</h3>"; continue; }
    if (/^- /.test(z)){
      absatzEnde();
      if (!liste) liste = [];
      liste.push({text:[z.slice(2)], unter:[]});
      continue;
    }
    if (/^\s{2,}- /.test(z) && liste){
      liste[liste.length - 1].unter.push([z.replace(/^\s+- /, "")]);
      continue;
    }
    if (/^\s+\S/.test(z) && liste){
      // Fortsetzung des letzten Punkts (oder Unterpunkts)
      var letzter = liste[liste.length - 1];
      var ziel = letzter.unter.length ? letzter.unter[letzter.unter.length - 1] : letzter.text;
      ziel.push(z.trim());
      continue;
    }
    if (z.trim() === ""){ absatzEnde(); continue; }
    listeEnde();
    absatz.push(z.trim());
  }
  allesEnde();
  return html;
}

// Release dates as "19.10.2026" and "19 Oct 2026"; UTC, so no time zone
// moves a date by one day.
var DATUM = {
  de: new Intl.DateTimeFormat("de-DE", {dateStyle: "medium", timeZone: "UTC"}),
  en: new Intl.DateTimeFormat("en-GB", {dateStyle: "medium", timeZone: "UTC"})
};

function zerlegen(text){
  var bloecke = text.split(/\n(?=## )/), liste = [];
  for (var i = 0; i < bloecke.length; i++){
    var kopf = bloecke[i].split("\n", 1)[0];
    var m = /^## (v[\w.\-]+)\s*-\s*(\d{4})-(\d{2})-(\d{2})/.exec(kopf);
    if (!m) continue;      // Titel der Datei und "Unveroeffentlicht" fallen weg
    liste.push({
      nummer: m[1],
      datum: DATUM[sprLage].format(new Date(Date.UTC(+m[2], m[3] - 1, +m[4]))),
      test: /-(rc|beta|alpha|dev)/.test(m[1]),
      roh: bloecke[i].toLowerCase(),
      zeilen: bloecke[i].split("\n").slice(1),
      html: null
    });
  }
  return liste;
}

function verlaufLaden(){
  if (!document.getElementById("verlauf")) return;   // aenderungen/ only
  if (verlaufGeladen === sprLage) return;
  document.getElementById("verlauf-suche").placeholder =
    sprLage === "de" ? "Suchen, z. B. Hangar oder 3.50 …" : "Search, e.g. hangar or 3.50 …";
  document.getElementById("verlauf-suche").setAttribute("aria-label", sprLage === "de" ? "Änderungen durchsuchen" : "Search the changes");
  var leeren = document.getElementById("verlauf-leeren");
  leeren.title = sprLage === "de" ? "Suche leeren" : "Clear search";
  leeren.setAttribute("aria-label", leeren.title);
  if (verlaufDaten[sprLage]){ verlaufGeladen = sprLage; verlaufZeigen(); return; }
  var spr = sprLage;
  document.getElementById("verlauf-zahl").textContent = spr === "de" ? "Wird geladen …" : "Loading …";
  fetch(WURZEL + VERLAUF_DATEI[spr])
    .then(function(r){ if (!r.ok) throw r.status; return r.text(); })
    .then(function(text){
      verlaufDaten[spr] = zerlegen(text);
      if (spr === sprLage){ verlaufGeladen = spr; verlaufZeigen(); }
    })
    .catch(function(){
      // Ohne Datei bleibt der Weg zu GitHub — die Seite bricht nicht.
      document.getElementById("verlauf-zahl").innerHTML =
        (spr === "de" ? "Der Verlauf ließ sich nicht laden. " : "The history could not be loaded. ") +
        '<a href="https://github.com/Xharig/VerseKit/blob/main/' + VERLAUF_DATEI[spr] + '">' +
        (spr === "de" ? "Auf GitHub lesen" : "Read it on GitHub") + "</a>";
    });
}

function sucheLeeren(){
  var feld = document.getElementById("verlauf-suche");
  feld.value = "";
  verlaufZeigen();
  feld.focus();
}

function verlaufZeigen(){
  var feld = document.getElementById("verlauf-suche");
  document.getElementById("verlauf-leeren").hidden = !feld.value;
  var alle = verlaufDaten[sprLage];
  if (!alle) return;
  var suche = feld.value.trim().toLowerCase();
  var mitTest = document.getElementById("verlauf-test").checked;
  var treffer = [], sichtbar = 0;
  for (var i = 0; i < alle.length; i++){
    if (alle[i].test && !mitTest) continue;
    sichtbar++;            // Grundmenge fuer „X von Y" — ohne ausgeblendete Testfassungen
    if (suche && alle[i].roh.indexOf(suche) < 0) continue;
    treffer.push(alle[i]);
  }
  // While searching every match, otherwise the newest ones first.
  var zeigen = (suche || verlaufAlle) ? treffer : treffer.slice(0, VERLAUF_ERSTE);
  var ziel = document.getElementById("verlauf");
  ziel.innerHTML = "";
  for (var k = 0; k < zeigen.length; k++){
    var v = zeigen[k];
    var kasten = document.createElement("details");
    kasten.className = "version";
    var kopf = document.createElement("summary");
    kopf.innerHTML = '<span class="nummer">' + maske(v.nummer) + '</span><span class="datum">' +
      v.datum + "</span>" + (v.test ? '<span class="test">' + (sprLage === "de" ? "Testfassung" : "Test build") + "</span>" : "");
    kasten.appendChild(kopf);
    var inhalt = document.createElement("div");
    inhalt.className = "inhalt";
    kasten.appendChild(inhalt);
    // Erst beim Aufklappen umwandeln — 290 Versionen auf einmal braucht niemand.
    kasten.addEventListener("toggle", (function(v, inhalt){
      return function(){
        if (!this.open || inhalt.innerHTML) return;
        if (v.html === null) v.html = umwandeln(v.zeilen);
        inhalt.innerHTML = v.html;
      };
    })(v, inhalt));
    if (k === 0 && !suche) kasten.open = true;
    ziel.appendChild(kasten);
  }
  var zahl = sprLage === "de"
    ? (suche ? treffer.length + " von " + sichtbar + " Versionen passen"
             : "Zeigt " + zeigen.length + " von " + treffer.length + " Versionen")
    : (suche ? treffer.length + " of " + sichtbar + " versions match"
             : "Showing " + zeigen.length + " of " + treffer.length + " versions");
  document.getElementById("verlauf-zahl").textContent = zahl;
  document.getElementById("verlauf-mehr").hidden =
    !!suche || zeigen.length >= treffer.length;
}

/* Addresses of the single-page site before 23.10.2026 lead to the pages
   that hold those parts now, so old links keep working. */
var ALTE_ADRESSEN = {
  "#alle-aenderungen": "aenderungen/", "#aenderungen": "aenderungen/",
  "#fragen-alle": "fragen/", "#fragen": "fragen/",
  "#laden": "herunterladen/", "#sicher": "herunterladen/#sicher",
  "#tour": "rundgang/", "#ansichten": "rundgang/#ansichten"
};

/* Each language has its own address: German at "/", English at "/en/", and
   every sub-page the same below it ("/rundgang/", "/en/rundgang/"; generated
   by tools/seite_en_bauen.py). The page language comes from <html lang>,
   never from the browser: search engines read the page with an English
   browser and would otherwise never see the German text. A language picked
   once (button, bar or closing the bar) is remembered and opens directly
   from then on. Without a choice, a bar offers the other version when the
   browser prefers it. */
var seitenSprache = document.documentElement.lang === "en" ? "en" : "de";
// This page's place below the language root, e.g. "" or "rundgang/".
var seitenPfad = location.href.split("#")[0].split("?")[0]
  .slice(WURZEL.length).replace(/^en\//, "").replace(/index\.html$/, "");
function alteAdresseUmleiten(){
  if (seitenPfad === "" && ALTE_ADRESSEN[location.hash]){
    location.replace(WURZEL + (seitenSprache === "en" ? "en/" : "") + ALTE_ADRESSEN[location.hash]);
  }
}
alteAdresseUmleiten();
window.addEventListener("hashchange", alteAdresseUmleiten);
var ADRESSE = {de: WURZEL + seitenPfad, en: WURZEL + "en/" + seitenPfad};
var gewaehlt = null;
try { gewaehlt = localStorage.getItem("vk-sprache"); } catch (e) {}

function spracheMerken(s){
  try { localStorage.setItem("vk-sprache", s); } catch (e) {}
}

function spracheWechseln(s, event){
  spracheMerken(s);
  if (event) event.preventDefault();
  if (s === seitenSprache) return;
  location.href = ADRESSE[s] + location.hash;
}

if ((gewaehlt === "de" || gewaehlt === "en") && gewaehlt !== seitenSprache){
  location.replace(ADRESSE[gewaehlt] + location.hash);
}

var browserSprache = ((navigator.languages && navigator.languages[0]) ||
                      navigator.language || "de").toLowerCase();
var browserWill = browserSprache.indexOf("de") === 0 ? "de" : "en";
if (!gewaehlt && browserWill !== seitenSprache){
  document.getElementById("hinweis-sprache-text").textContent = browserWill === "en"
    ? "This page is also available in English." : "Diese Seite gibt es auch auf Deutsch.";
  var hinweisLink = document.getElementById("hinweis-sprache-link");
  hinweisLink.textContent = browserWill === "en" ? "English version" : "Zur deutschen Fassung";
  hinweisLink.href = ADRESSE[browserWill];
  hinweisLink.lang = browserWill;
  hinweisLink.addEventListener("click", function(event){ spracheWechseln(browserWill, event); });
  var hinweisZu = document.getElementById("hinweis-sprache-zu");
  hinweisZu.setAttribute("aria-label", seitenSprache === "de" ? "Hinweis schließen" : "Close notice");
  hinweisZu.title = hinweisZu.getAttribute("aria-label");
  hinweisZu.addEventListener("click", function(){
    spracheMerken(seitenSprache);
    document.getElementById("hinweis-sprache").hidden = true;
  });
  document.getElementById("hinweis-sprache").hidden = false;
}

sprache(seitenSprache);

/* The referral "Pass the link on" button is assets/teilen.js, the same file
   as on xharig.com. */

/* Event handlers instead of onclick and the like in the HTML: the content
   security policy of the page allows no inline code. */
function beiEreignis(id, art, handler){
  var el = document.getElementById(id);
  if (el) el.addEventListener(art, handler);
}
beiEreignis("knopf-de", "click", function(event){ spracheWechseln("de", event); });
beiEreignis("knopf-en", "click", function(event){ spracheWechseln("en", event); });
beiEreignis("knopf-aeltere", "click", function(){ verlaufAlle = true; verlaufZeigen(); });
beiEreignis("verlauf-suche", "input", verlaufZeigen);
beiEreignis("verlauf-suche", "keydown", function(event){ if (event.key === "Escape") sucheLeeren(); });
beiEreignis("verlauf-leeren", "click", sucheLeeren);
beiEreignis("verlauf-test", "change", verlaufZeigen);
