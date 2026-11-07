<div align="center">

<img src="assets/icon.png" alt="VerseKit icon" width="128">

# VerseKit

**Your Star Citizen toolkit: blueprints, contracts, crafting, trade and more, live while you play**

<sub>Windows · Linux · free · no account, no cloud</sub>

<sub>formerly **SC BP Watcher** · your collection and settings are kept</sub>

[![Version](https://img.shields.io/github/v/release/Xharig/VerseKit?include_prereleases&label=Version&color=9ce430&labelColor=161c28)](https://github.com/Xharig/VerseKit/releases)
[![Downloads](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FXharig%2FVerseKit%2Fzaehler%2Fdownloads.json&label=Downloads&color=9ce430&labelColor=161c28)](https://github.com/Xharig/VerseKit/releases)
[![Website](https://img.shields.io/badge/Website-take%20the%20tour-9ce430?labelColor=161c28)](https://versekit.xharig.com/)
[![License](https://img.shields.io/badge/License-GPL--3.0-9ce430?labelColor=161c28)](LICENSE)
[![Discord](https://img.shields.io/badge/Discord-join-9ce430?labelColor=161c28&logo=discord&logoColor=9ce430)](https://xharig.com/discord)
[![Ko-fi](https://img.shields.io/badge/Ko--fi-buy%20me%20a%20coffee-9ce430?labelColor=161c28&logo=kofi&logoColor=9ce430)](https://ko-fi.com/xharig)
<br>
[![Rust](https://img.shields.io/badge/Rust-1.99-34415a?labelColor=161c28&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%C2%B7%20Linux-34415a?labelColor=161c28)](#download)
[![Star Citizen](https://img.shields.io/badge/Star%20Citizen-compatible-34415a?labelColor=161c28)](https://robertsspaceindustries.com/enlist?referral=STAR-YC7L-S4W2)

[Deutsch](https://github.com/Xharig/VerseKit/blob/main/README.md) · **English**

</div>

---

<img src="assets/readme-kopf-en.png" alt="The VerseKit website on a PC and a phone" width="100%">

VerseKit sits on top of Star Citizen as a narrow overlay and reports every new blueprint within seconds. The window behind it covers what comes next: where to get what you are missing, what you need to craft, where to find ore and where to sell your cargo.

## What VerseKit does

- **Blueprints live:** Every new blueprint shows up in the overlay within seconds. On first start VerseKit reads your existing collection from the game logs.
- **Where to get what you miss:** For every blueprint that comes from a contract: the faction, contract, rank and reward, easiest route first.
- **Handle search:** Who is behind a handle or an org? Name, organisations and rank, plus marking friend and foe, right from the info bar over the game.
- **Details in the game:** The contract texts show which blueprints a contract pays out and which you already have; every craftable item in the terminal says whether you own the blueprint, where it comes from and what crafting takes.
- **Crafting and inventory:** Ingredients, crafting time and what your material does to the stats, and under “For ship” only parts that fit one of your ships. Plus your inventory, what you still lack, your shop purchases and sales as a suggestion from the log, and moving home after a patch.
- **Mining and refining:** Locations for every resource, the best refinery and the right refining method. On Windows the scanner reads the signature while you scan, and Alt+Shift+R reads the refinery terminal, in the German game and for long lists too.
- **Trade:** Shops, the best buyers and trade routes with real profit.
- **Your hangar:** Pull your ships from the pledge store, fit every slot on its own (from the game data of your installation) and share the loadout as a code or a file.
- **Translation per channel:** German, French, Spanish, Bavarian, Dutch and more, set apart for LIVE, HOTFIX, PTU, EPTU and Tech Preview.
- **Game settings from outside:** Controls, graphics with the shader cache, axes and field of view, without switching into the game.

## Download

| System | File | |
|---|---|---|
| **Windows** | [`VerseKit-Setup.exe`](https://github.com/Xharig/VerseKit/releases/latest) | Installer with a start menu entry, uninstalls cleanly |
| **Linux** | [`VerseKit-x86_64.AppImage`](https://github.com/Xharig/VerseKit/releases/latest) | A single file, make it executable once |

Run it, and a wizard sets everything up in about a minute. VerseKit is free and open source, needs no account and keeps itself up to date: it checks every ten minutes and installs a new version as soon as Star Citizen is closed, with no switch to set. The Windows warning on first start is not a virus detection, more on that below under *Installation in detail*.

**[Tour on the website](https://versekit.xharig.com/)** · **[Discord](https://xharig.com/discord)** · **[Report a bug or wish](https://github.com/Xharig/VerseKit/issues)** · **[What's new](https://github.com/Xharig/VerseKit/blob/main/CHANGELOG.en.md)**

## Details

Everything else folds open below. Technical matters like files, settings and environment variables are in the **[manual](MANUAL.en.md)**.

### Pictures

<details>
<summary><b>Overlay and blueprint list</b></summary>

<table>
<tr>
<td width="32%" valign="top" align="center">
<img src="assets/screenshot-overlay-en.png" alt="The overlay while playing" width="100%"><br>
<sub>The overlay: narrow, sits on top of the game, opacity adjustable</sub>
</td>
<td width="68%" valign="top" align="center">
<img src="assets/screenshot-liste-en.png" alt="The window with the blueprint list" width="100%"><br>
<sub>The blueprint list: search, five filters, and where each blueprint comes from</sub>
</td>
</tr>
</table>

</details>

<details>
<summary><b>In game, without tabbing out</b></summary>

VerseKit writes into the game's mission text **which** blueprints a contract hands out, with `[x]` for the ones you already have. The title shows `[BP]` (where every tier has the same reputation threshold, with the reputation needed: `[BP 10,000 Rep]`), and the names are in the description, split by Stanton, Pyro and Nyx where a contract hands out different blueprints there.

<table>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-ingame-teils.jpg" alt="Contract with some blueprints already owned" width="100%"><br>
<sub><b>3 of 6</b>: <code>[x]</code> you have, <code>[&nbsp;&nbsp;]</code> still missing</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-ingame-keine.jpg" alt="Contract with none of the blueprints owned" width="100%"><br>
<sub><b>0 of 12</b>: nothing here that you already own</sub>
</td>
</tr>
</table>

</details>

<details>
<summary><b>My hangar</b></summary>

A new blueprint immediately raises the next question: **does this part even fit any of my ships?** For that the tool has to know what is in your hangar: the game records it nowhere.

> **How your hangar gets into the tool:** the browser add-on **Star Citizen: Hangar Extension** by AlyxOne adds export buttons to your pledge page, for [Firefox](https://addons.mozilla.org/en-US/firefox/addon/star-citizen-hangar-extension/), [Chrome](https://chromewebstore.google.com/detail/star-citizen-hangar-exten/jeoidbfcbdplammndhpflojejmpkjcfm) and [Edge](https://microsoftedge.microsoft.com/addons/detail/star-citizen-hangar-exte/agahppbhmogalnjlaolmjbbemlcjbioe). Download **both** files there and pick them one after the other under **My hangar → “Get or add ships” → “Or import a file”**: the JSON brings the bundle relations and the insurance, the CSV adds purchase date, price and whether a pledge is meltable. Nothing gets duplicated. No export knows about ships bought in-game; you add those by hand.

**Fleet analysis:** Above the ship list you can fold open which gameplay loops your ships cover, grouped into combat, support and travel, where there is a gap and which snubs have no mothership. Every pledged ship shows whether it can be melted or upgraded via CCU. The gameplay loops per ship come from Maluh's [Fleetmanifest](https://github.com/MaluhTV/SC_Fleetmanifest) (an assessment, not game data).

<table>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-hangar-en.png" alt="My hangar with four ships added" width="100%"><br>
<sub><b>My hangar</b>: pulled from the pledge store in one go or added by hand; after that, crafting tells you which of your ships a blueprint fits</sub>
</td>
</tr>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-asop-ingame-en.jpg" alt="The in-game fleet manager with three self-named ships" width="100%"><br>
<sub><b>Names in the game</b>: this is what it looks like <b>in game</b>: your own names at the retrieval terminal instead of the factory ones, given right in the hangar. The star does more than mark: since the fleet manager sorts alphabetically, starred ships move to the top</sub>
</td>
</tr>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-einkaufsliste-en.png" alt="Shopping list: the bill across all ships" width="100%"><br>
<sub><b>Shopping list</b>: the bill across all ships: buy or build per item, with total and shopping route</sub>
</td>
</tr>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-bergung-en.png" alt="What is inside a wreck" width="100%"><br>
<sub><b>What is inside?</b>: what a wreck carries from the factory and what the parts are worth in a shop</sub>
</td>
</tr>
</table>

</details>

<details>
<summary><b>The workshop</b></summary>

The blueprint is the start. The workshop answers what comes after it: **what do I need, do I have it, and what will it turn into?**

<table>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-herstellung-en.png" alt="Crafting with an expanded recipe" width="100%"><br>
<sub><b>Crafting</b>: ingredients, craft time and what <i>your</i> material does to the stats</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-meinlager-en.png" alt="My Inventory with recorded resources" width="100%"><br>
<sub><b>My Inventory</b>: materials, goods for sale and items in one place: type, location, quality and amount, kept by hand</sub>
</td>
</tr>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-bergbau-en.png" alt="Mining showing where Iron is found" width="100%"><br>
<sub><b>Mining</b>: type a resource and see where it sits; or enter the scanner reading and see what it found</sub>
</td>
</tr>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-farmliste-en.png" alt="What to farm" width="100%"><br>
<sub><b>What to farm</b>: the raw materials still missing for everything you plan to build, with where to find them</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-raffinerien-en.png" alt="Refineries" width="100%"><br>
<sub><b>Refineries</b>: what each refinery makes of your ore: bonus and penalty per material</sub>
</td>
</tr>
</table>

</details>

<details>
<summary><b>Trading & buying</b></summary>

Buying and selling: **where is a part or ship on the shelf, and where do I offload my cargo, and what does it pay per SCU?**

<table>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-laeden-en.png" alt="Shops" width="100%"><br>
<sub><b>Shops</b>: where a finished part is on the shelf and what it costs there</sub>
</td>
</tr>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-meinlager-verkauf-en.png" alt="My Inventory filtered to goods for selling" width="100%"><br>
<sub><b>My Inventory, for selling</b>: what is in your hold, with selling price and profit</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-verkauf-en.png" alt="Buy & Sell tab with the best buyers" width="100%"><br>
<sub><b>Buy & Sell</b>: the best buyers, sorted by how many of your goods a place takes; switched to Buy, the cheapest places to buy</sub>
</td>
</tr>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-routen-en.png" alt="Trade routes" width="100%"><br>
<sub><b>Routes</b>: say where you are and what fits in the hold; the best runs from there come out on top</sub>
</td>
</tr>
</table>

</details>

<details>
<summary><b>The window</b></summary>

> [!NOTE]
> The screenshots are regenerated before every release and show this version. On an older version some of what is shown here does not exist yet.

<table>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-statistik-en.png" alt="Statistics" width="100%"><br>
<sub><b>Statistics</b>: play time, sessions, contracts, quantum jumps, deaths and when you play</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-darstellung-en.png" alt="Appearance" width="100%"><br>
<sub><b>Appearance</b>: colour scheme with preview, language, interface size</sub>
</td>
</tr>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-fortschritt-en.png" alt="Progress by area" width="100%"><br>
<sub><b>Progress</b>: per area, details on click</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-auftragstexte-en.png" alt="Mission text settings" width="100%"><br>
<sub><b>Game</b>: pick a text source, switch it on and off, view the user.cfg</sub>
</td>
</tr>
<tr>
<td valign="top" align="center">
<img src="assets/screenshot-bestand-en.png" alt="Backup and reset" width="100%"><br>
<sub><b>Backup & reset</b>: backup, export your inventory for the basetool or import one</sub>
</td>
<td valign="top" align="center">
<img src="assets/screenshot-anzeige-en.png" alt="Overlay settings" width="100%"><br>
<sub><b>Overlay</b>: pop-up mode, click-through, opacity</sub>
</td>
</tr>
<tr>
<td valign="top" align="center">
<img src="assets/screenshot-ueber-en.png" alt="About and update channel" width="100%"><br>
<sub><b>About</b>: stable releases or test builds, with a button to fetch one</sub>
</td>
<td valign="top" align="center">
<img src="assets/screenshot-wasistneu-en.png" alt="What's new" width="100%"><br>
<sub><b>What's new</b>: every release expandable, filtered by kind</sub>
</td>
</tr>
</tr>
<tr>
<td valign="top" align="center">
<img src="assets/screenshot-danke-en.png" alt="Thanks and licenses" width="100%"><br>
<sub><b>Thanks &amp; Licenses</b>: what belongs to whom, and who helped</sub>
</td>
<td valign="top" align="center">
<img src="assets/screenshot-serverstatus-en.png" alt="Server status" width="100%"><br>
<sub><b>Server status</b>: is Star Citizen up?</sub>
</td>
</table>

<details>
<summary>And the rest: General and FOV</summary>

<table>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-allgemein-en.png" alt="General settings" width="100%"><br>
<sub><b>General</b>: language, sound, autostart, menu entry</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-blickwinkel-en.png" alt="Field of view" width="100%"><br>
<sub><b>FOV</b>: the field of view for your monitor, calculated instead of guessed</sub>
</td>
</tr>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-auftragslog-en.png" alt="Missions and log" width="100%"><br>
<sub><b>Missions &amp; log</b>: which mission you played when, kept even after the game deletes its logs</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-patchaenderungen-en.png" alt="Changed game values" width="100%"><br>
<sub><b>Changed game values</b>: what a patch changed on ships, weapons and components</sub>
</td>
</tr>
</table>

</details>

</details>

<details>
<summary><b>The colour schemes</b></summary>

Under **Settings → Appearance** you choose how VerseKit looks: the overlay and every window. The choice applies at once, without a restart.

<table>
<tr>
<td width="33%" valign="top" align="center">
<img src="assets/screenshot-farbschema-original-en.png" alt="VerseKit (original)" width="100%"><br>
<sub><b>VerseKit (original)</b></sub>
</td>
<td width="33%" valign="top" align="center">
<img src="assets/screenshot-farbschema-krt-en.png" alt="KRT (orange)" width="100%"><br>
<sub><b>KRT (orange)</b>: square, with corner marks</sub>
</td>
<td width="33%" valign="top" align="center">
<img src="assets/screenshot-farbschema-eis-en.png" alt="Ice (cyan)" width="100%"><br>
<sub><b>Ice (cyan)</b></sub>
</td>
</tr>
<tr>
<td valign="top" align="center">
<img src="assets/screenshot-farbschema-nebel-en.png" alt="Nebula (violet)" width="100%"><br>
<sub><b>Nebula (violet)</b></sub>
</td>
<td valign="top" align="center">
<img src="assets/screenshot-farbschema-glut-en.png" alt="Ember (red)" width="100%"><br>
<sub><b>Ember (red)</b>: warnings in yellow-orange</sub>
</td>
<td valign="top" align="center">
<img src="assets/screenshot-farbschema-kontrast-en.png" alt="High contrast" width="100%"><br>
<sub><b>High contrast</b>: white and yellow on black</sub>
</td>
</tr>
</table>

</details>

<details>
<summary><b>What dead zone, saturation and sensitivity do</b></summary>

Under **Game settings → Axes & curves** you can set how sharply each stick axis responds. Three values, each bending the curve differently:

<img src="assets/erklaerung-kurve-en.png" alt="Four curves: nothing set, with dead zone, with saturation, with sensitivity" width="100%">

| Value | What it does | What for |
|---|---|---|
| **Dead zone** | The start of the travel does nothing | Against jittery sticks that will not sit still in the centre |
| **Saturation** | Full deflection before the mechanical stop | When the stick does not travel all the way, or full effect should come sooner |
| **Sensitivity** | Above 1 the curve sags, below 1 it bulges | Above 1: finer aiming around the centre. Below 1: more direct, sharper |

The dashed line shows how it would run with nothing set. The dark areas are the travel where nothing changes any more.

> [!NOTE]
> Star Citizen ties these values to an internal device number, not to its name. When a stick gets a new number (a different USB port, new firmware), the game treats it as a new device, and the old values stay in the file without any effect. VerseKit finds such leftovers and clears them out on request.

</details>

### Learn more

<details>
<summary><b>Why VerseKit</b></summary>

There are several blueprint lists. Five things make the difference day to day:

- **You never leave the game.** The overlay sits on top of Star Citizen. No second window, no alt-tab, no browser: the new blueprint is simply there while you keep playing.
- **It knows what you already have.** VerseKit keeps your blueprint inventory itself and reads Star Citizen's stored session logs on first start: you get your existing collection for free, without typing anything. If a gap remains anyway, it says so instead of passing off an incomplete list as complete.
- **It tells you where to get what's missing.** For every blueprint that comes from a contract it shows which faction offers it, in which contract, from which standing, and what it pays, sorted by the easiest route. "I'm missing X" is half the information; "X drops at Foxwell from Veteran Contractor" is all of it.
- **It also reports what you cannot have yet.** The catalogue watch notices when CIG makes something **newly craftable** with a patch, regardless of your own unlocks; such lines are blue. If you are waiting for a particular part, put it on the watchlist and you will be pointed at it the moment it appears.
- **No account, no cloud.** The tool reads files that are already on your disk and only writes into the game what you explicitly switch on. What it fetches from the network and what it sends is under *Your data and the network*.

On top of that: class, size and grade are right there in the line (`M/1/A`), the interface speaks German and English, and the whole thing is a single program written in Rust: one file per system, nothing to install on top.

</details>

<details>
<summary><b>All features</b></summary>

| | |
|---|---|
| <img src="assets/symbole/22/karten-gruen.png" width="22" alt=""> **Overview** | The start page shows every area as a tile. Pin the pages you use all the time to the start page |
| <img src="assets/symbole/22/blitz-gruen.png" width="22" alt=""> **Instant** | Reads Star Citizen's `Game.log` → the blueprint is in the list **within seconds** |
| <img src="assets/symbole/22/liste-gruen.png" width="22" alt=""> **Keyboard shortcut** | **Alt+Shift+V** opens and folds the overlay with the blueprint list, from inside the full-screen game, no blind hunting for the window. **Alt+Shift+R** reads the refinery terminal, **Alt+Shift+H** puts the cursor into the handle search field of the info bar (Enter searches, Esc hands the keyboard back to the game). All three are on by default and can be changed or switched off under *Settings → Overlay*. A single key you never type works instead of a combination too: F1 to F24, Pause, Scroll Lock, Insert or Num / * - +, say F13 on its own. Exactly these combinations are registered; nothing else is listened to. **On Wayland** (Linux) no program can register a system-wide combination itself: there a ready command to copy stands under the setting, which you bind to a combination in your desktop, on KDE Plasma under *System Settings → Keyboard → Shortcuts → Add New → Command or Script*. If VerseKit is already running, the command opens the main window instead of starting it a second time |
| <img src="assets/symbole/22/liste-gruen.png" width="22" alt=""> **Blueprint list** | Search everything, grouped by type, filters *all / owned / missing / watchlist / farming / new in game*, with progress, for all blueprints or just your watchlist. Tick items with one click. The **Received** filter (today, 7 days, 14 days, this month) flattens the list, newest first, each line with date and source |
| <img src="assets/symbole/22/herkunft-gruen.png" width="22" alt=""> **Where it drops** | The **"Where from?"** button shows faction, contract, required standing and payout, for every blueprint that comes from a contract, sorted by the easiest route. From **Crafting** a button leads straight there: missing the blueprint, one click tells you which contract to run |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **Contract accepted** | Accept a contract and you see right away whether blueprints are part of it, and **which of those you are still missing**. If the catalogue does not know the contract, it stays quiet rather than guessing |
| <img src="assets/symbole/22/liste-gruen.png" width="22" alt=""> **What to do next** | Every running contract lists its **open objectives** underneath: "Disable the Hartmoore inverter", "Locate and reset the node". They come from the same log and move on as soon as you finish one |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **Missions & log** | Which missions you played when, how often, and **which blueprint came out of it**. The answer to "which mission was it again that dropped the helmet?". It works the other way round too: search for a mission, click it, and you see what it gives. Already filled on first start: the game's kept logs reach back weeks. From then on it grows with you and stays, long after the game has dropped its own. If a mission is bugged, the bug icon on its row searches the **Issue Council** for it, always with the English title, even if you play in another language |
| <img src="assets/symbole/22/blitz-gruen.png" width="22" alt=""> **Crafting** | For each craftable item: the ingredients with amounts and the craft time, and whether you have the blueprint. **At the top, the product stats as base, crafted and change** (damage, DPS, shield, cooling, armour), read from the game data of your own installation and cross-checked against scmdb.net. Weapons also show their integrity. Clicking a resource jumps to where it can be mined. Under **“For ship”** you pick one of your ships from My hangar, and the list shows only parts that fit a mount of that ship, with and without blueprint; on the ship card in the hangar, “Craftable parts” jumps straight there |
| <img src="assets/symbole/22/herkunft-gruen.png" width="22" alt=""> **Mining** | Both directions in one search: type a resource → its locations. Type a location → what is found there. Plus the **concentration** at every location: what share of the rocks there is that ore, from "barely any" to "almost all of it", with the list ordered by richness. A **"With what?"** selector separates ship, vehicle and hand mining, because the three see entirely different deposits. With a **refinery comparison** per ore (which station gives the best bonus) and the **scan signature** to recognise it in game. **On Windows VerseKit reads the signature by itself while scanning** and shows the ore in the overlay, also ROC/ground and hand mining; for asteroids it counts the rocks ("2× Asteroid (C-Type)"). Switched on and off with the eye in the bar. The **signature window** shows number and ore: under *Settings → Overlay* you choose whether it stands “Above the signature” in the game or “In the info bar”, drag it into place in the picture, set its size with a slider and decide how many seconds the display stays after the number. The **search frame** tells VerseKit where the signature stands in the ship HUD: "Get game picture", switch into the game, draw a frame in the picture, and "Show the frame in the game" lets you check it in the game itself |
| <img src="assets/symbole/22/herkunft-gruen.png" width="22" alt=""> **Which refining method?** | The terminal offers nine methods and shows a single line for each. Tell it what matters to you instead (**yield, cost or speed**) and it names the method. Two of the nine are never worth taking: another one beats them on every count, and the tool says so |
| <img src="assets/symbole/22/raffinerie-gruen.png" width="22" alt=""> **Refineries** | **Which station makes the most of your ore, as one table.** One row per material, one column per refinery, and in each cell the bonus or penalty that station applies. Green is the best value in the row, red costs you yield; the system is shown above the columns, so you can see at a glance whether the better price is worth the trip. The other direction to "Which refining method?": that one picks the method, this one picks the station |
| <img src="assets/symbole/22/bestand-gruen.png" width="22" alt=""> **My Inventory** | **Your whole inventory in one place**, in the **Fleet & Logistics** group next to My hangar and the unit storage: tabs **Material** and **Items** at the top, below them filters by **type** (ore, refined, mineral, commodity, vehicle component, armor …), **location** and **purpose** (for crafting or for selling) plus a search. One line per entry, with the type as a badge and, for items, the class/size/grade tag such as `(Mil/3/A)`: no internet and no Basetool needed. Enter what resources you have: **material, amount, quality, location**. The same material is grouped on one line with total amount and quality range; unfold it to see the individual entries. **Every entry is a crate, just like in the game:** crates of the same quality at the same place sit under one summary line, crafting takes each portion from a single crate that holds enough, and a crate too small for any recipe is marked **"Leftover: sell only"**. Recipes then show what is missing, and a button subtracts the ingredients when you craft, in exactly the quality you set: by slider, typed in, or like at the terminal with **Best / Worst material**. **And because the recipes carry how material quality changes the item's values, you see what *your* material would produce** |
| <img src="assets/symbole/22/bestand-gruen.png" width="22" alt=""> **Prices** | What a material costs at the terminal and what it sells for: the numbers come from **[UEX Corp](https://uexcorp.space/)** and refresh daily. That way crafting answers not only "what am I missing" but also "what will it cost me". Without a connection the column simply stays empty |
| <img src="assets/symbole/22/handelslager-gruen.png" width="22" alt=""> **My Inventory: for selling** | What you carry to sell: in the same inventory, but with the **Sale** badge and its own filter: one is building material you keep, the other is cargo you want gone. Enter commodity, location and SCU; the amount field does maths (`100+5`). Instead of a quality there is a **"marked as stolen"** tick: quality does not affect selling, and looted cargo is always Q 0 anyway. Enter what you **paid per SCU** and you see the **profit** per item and per selling location |
| <img src="assets/symbole/22/verkauf-gruen.png" width="22" alt=""> **Buy & Sell** | Switch between **Sell** and **Buy** at the top. **Buy:** search a commodity (e.g. gold) and see which terminals sell it, cheapest place first, with stock level (empty and nearly empty are flagged), box sizes and the age of the report. **Sell:** where to offload your goods and what they pay **per SCU**, for **several commodities at once**. Sorted not by the highest price but by **how many of your goods a place takes**: 100 SCU gold, 40 copper and 25 iron pay 3,533,000 aUEC at one place, 3,566,000 spread over three, one percent more for two extra approaches. If the cargo is marked as stolen, the tab narrows down to the 15 terminals that ask no questions |
| <img src="assets/symbole/22/hangar-gruen.png" width="22" alt=""> **My hangar** | **Which ships you own, and whether a blueprint even fits in one.** You can pull your hangar out of the pledge store in one go: the browser add-on [Star Citizen: Hangar Extension](https://robertsspaceindustries.com/community-hub/post/star-citizen-hangar-extension-browser-add-on-7xzYDnJDV6c2W) puts it into a file, and VerseKit reads it: JSON **and** CSV, with LTI or insurance duration per ship (the older [Hangar XPLORer](https://github.com/dolkensp/HangarXPLOR) is still read as well). The [Hangar-Exporter](https://github.com/Drachennarr/Hangar-Exporter-Releases/releases/latest) by Drachennarr reads the hangar from the RSI site too; the installer puts its Firefox add-on in place for you, and the hangar page and the setup wizard each have a button for Firefox, Chrome and Edge. Ship lists from **Fleetview** (CCU Game), **Fleetyards** and **StarJump FleetViewer** are read too: they only add, nothing gets removed. Ships bought in-game go in by hand right next to it: every ship remembers where it came from. After that, crafting shows under every blueprint **which of your ships the part fits** and how many slots it has there. VerseKit reads the slots from the game data of your own installation while Star Citizen is closed. Only for ships missing there does the data of [erkul.games](https://erkul.games) step in; only what you actually have in your hangar is fetched, and only once per game patch |
| <img src="assets/symbole/22/hangar-gruen.png" width="22" alt=""> **Loadout per slot** (in the hangar) | **Every ship shows its slots all the way down**: turret and weapons, rack and missiles, mining head and modules, the weapons of each turret on lines of their own. Equal slots share a line, the arrow at its end opens each slot on its own, and every list ends with “Leave empty”. Slots you cannot refit in the game show their part and “built in”, with no choice and no basket |
| <img src="assets/symbole/22/hangar-gruen.png" width="22" alt=""> **Share a loadout** (in the hangar) | **“Share loadout”** copies it as a code for Discord or a chat, **“Save as file”** puts all loadouts of a ship into a small text file. Nothing is uploaded. When you paste a code or open a file, you see first what arrives. The loadout of another ship of the same type can be taken over. Ships move between **Planned** and **My hangar** with their loadout, name and note |
| <img src="assets/symbole/22/hangar-gruen.png" width="22" alt=""> **Names in the game** (in the hangar) | **Your ships are called what you call them at the retrieval terminal (ASOP).** Instead of "Anvil F7C-M Super Hornet Mk II" it shows what you entered, and a star in front marks one without renaming it. The field sits right at each ship in the hangar, and the search there finds it by your own name too. ⚠ A name belongs to the **model**, not to the individual ship: two identical Hornets get the same name; variants like F7C, F7C-M, Mk I and Mk II can be told apart. The factory name comes back character for character at any time |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **Reputation thresholds** | **The reputation menu shows after every rank how much reputation it starts at**: "Guild Member [10,000+]". Only the game server fills the bar, but you see what comes next: bounty hunters, contractors, hauling, security, technicians, Battaglia and Wikelo. Can be switched off under "Game" |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **Blueprint details in item descriptions** | **Below the description of every craftable item the game shows whether you own the blueprint**, and if not, which contract it comes from, easiest first. Plus how many ingredients and how much time crafting takes and where the item is cheapest to buy. Text from other tools stays, the block goes below it. If the game has no name text for a craftable item, VerseKit writes a stand-in name from the class name. Text the game has stays untouched, and taking the details back removes the stand-in name again. Can be switched off under "Game" |
| <img src="assets/symbole/22/wunschliste-gruen.png" width="22" alt=""> **Planned** (in the hangar) | **What you are aiming for, with price, location and a loadout you can plan in advance.** Add a ship under *Planned* and see what it costs in game and where it is sold. The loadout can be planned **before** you own it, while the total is still a decision, not a receipt. Nothing here shows up under "fits your ship": a planned ship is not a possession. Once you buy it, move it with its loadout to *My hangar*, and after a wipe back with “Move to Planned” |
| <img src="assets/symbole/22/einkaufsliste-gruen.png" width="22" alt=""> **Shopping list** | **The bill across all your ships.** Set what belongs in each slot: every part shows **grade and class** ("A · Military · blueprint only") so you can build towards a purpose without knowing 1,500 names. **Military parts** are included: no shop sells them, only blueprints. Per item you choose **buy or build**, and below stands the total with a shopping route of as few stops as possible. Tick off what you fitted: it leaves the list and the total |
| <img src="assets/symbole/22/farmliste-gruen.png" width="22" alt=""> **What to farm** | **Your stock weighed against everything you want to build.** Across all items at once, not recipe by recipe: two parts needing 2 Iron each with 3 Iron in stock: checked individually both say "enough", together one is missing. Ore below the required quality does not count as stock but is named rather than passed over. Each missing material shows its **richest locations**, plus the places where you get **several at once**. Parts on your list get a **quantity**: two suits of armour mean twice the material. Clicking a material shows it in Mining with every place to find it |
| <img src="assets/symbole/22/sicherung-gruen.png" width="22" alt=""> **What is inside?** | **What a wreck carries from the factory, and what it is worth in a shop.** Type a ship and see every fitted part with its shop value. ⚠ NPC wrecks are lootable; player ships become useless once the insurance is claimed: stripped parts are then worthless too. That warning comes **before** any number |
| <img src="assets/symbole/22/zerlegen-gruen.png" width="22" alt=""> **Worth dismantling?** | **What the fabricator returns, before you start cutting.** 50 % of the materials, but **six never come back**, Quantainium and Stileron among them. Most parts contain at least one; anyone dismantling just for those hauled it for nothing. The figures come from the game data, not from the program |
| <img src="assets/symbole/22/blickwinkel-gruen.png" width="22" alt=""> **Field of view** | **Are you sitting right in front of your screen?** Measure your screen width with a bank card (they are standardised), enter your seating distance, and you get the field of view your setup actually delivers, with a rating |
| <img src="assets/symbole/22/laeden-gruen.png" width="22" alt=""> **Shops** | **Where a finished part sits on the shelf, and what it costs there.** The other direction to crafting: instead of "what do I need to build this", the question "is building it worth the effort at all". Parts of every item group: not just craftable ones, but missiles, bombs, ammunition and weapon attachments, plus ships to buy or rent. **Put any part into the shopping basket with an amount**, say a whole FPS loadout: the shopping list adds up the costs, shows every place to buy each part, plans the route with the fewest stops and lets you tick off each stop. You save a set as a template under a name. Every row shows **class, size, grade and manufacturer**, and you can filter by them: that is how you find the quantum drive that fits your ship without knowing all the names. The **Location** field shows only what is sold where you are right now (say, everything on the shelves in Levski). Click a part and you get every shop with price, location and system, the cheapest one first, used stock with its condition. Anything that is not sold anywhere never shows up |
| <img src="assets/symbole/22/routen-gruen.png" width="22" alt=""> **Routes** | **Trade routes with real profit instead of a price list.** Tell it where you are, how much cargo space you have and how much money, and you get the run with buy price, sell price and what is left at the end. Optionally across several stops in a row, sorted by highest profit or shortest hop, and as a round trip back to your start. One button searches the best route **anywhere in the verse**, no starting point needed. Pick your ship and the cargo capacity fills itself in |
| <img src="assets/symbole/22/joysticks-gruen.png" width="22" alt=""> **Controls** | **What is bound to which key, and rebind without switching to the game.** Your complete binding list in plain language ("Eject" instead of `v_eject`, in the tool's own language), joystick, keyboard, mouse and gamepad in one searchable list, switchable between *changed by me*, *everything* and *default*. To **rebind**, click a row and press the button you mean, no need to know its number, and if the input is already taken you see it beforehand. Plus: which stick has which number, and whether your bindings still point at a connected device. Works for **any** device: nothing needs to be set up in advance |
| <img src="assets/symbole/22/einrichtung-gruen.png" width="22" alt=""> **Setup wizard** | Walks you through everything important on first start, including the key settings, and **repeatable any time**, no digging through menus |
| <img src="assets/symbole/22/sicherung-gruen.png" width="22" alt=""> **Backup** | Everything only you have, in **one file**: blueprint inventory, both stocks, mission log, watchlist, settings and your controls (active mapping and saved profiles). The same button restores it: file onto a stick, read it in on the new machine, keep playing. On restoring, VerseKit asks separately whether the profiles are put back and whether the active mapping is replaced too; nothing happens while the game is running, and a copy is always made first. The downloaded reference data stays out, the program fetches that itself. **On the first start of VerseKit 5** it also backs up your 4.12 data with the controls into the "Sicherungen" folder of your data folder before it changes anything |
| <img src="assets/symbole/22/anzeige-gruen.png" width="22" alt=""> **Graphics** | Renderer, window mode, VSync, upscaling with technique and model, motion blur, film grain, sharpening and chromatic aberration, set without starting the game. Above it, what the game measures about your PC: performance index and its rating from Low to Ultra. VerseKit remembers the rating and says when it was last rated ("last rated on …"). It only writes while the game is closed, and backs up the file next to it first |
| <img src="assets/symbole/22/sicherung-gruen.png" width="22" alt=""> **Clear shader cache** | Graphics glitches or stutter after a patch? The *Clear shader cache* card on the page *Game settings → Graphics* empties the shader caches of Star Citizen and the graphics drivers plus old crash reports. It shows the size, when it was last cleared and whether it was cleared since the last patch. "Clear only old shaders" takes just the folders of older Star Citizen versions; installed and unclear folders stay. Clearing everything remains the second choice. The program finds the folders itself, on Linux inside the Wine prefix too. Your graphics settings stay, and nothing happens while the game is running |
| <img src="assets/symbole/18/punkt-blau.png" width="22" alt=""> **Catalogue watch** | Also reports when something becomes **newly craftable in the game**: a blueprint CIG added that did not exist before |
| <img src="assets/symbole/22/serverstatus-gruen.png" width="22" alt=""> **Server status** | A page of its own: **is Star Citizen up?** Shows what CIG reports on its status page: all three systems plus the incidents of the last two months in full. Refreshes itself once a minute. States stay in CIG's own wording; the entries are maintained by hand, not measured |
| <img src="assets/symbole/22/zeit-gruen.png" width="22" alt=""> **Changed game values** | A page of its own: **what a game patch changed about values**: ships, weapons, quantum drives, components, field by field with the old and the new value. Increased green, decreased red. The source only keeps the last ten patches; whatever you fetched once stays with you. |
| <img src="assets/symbole/18/punkt-blau.png" width="22" alt=""> **New in game** | Its own filter in the list: **only what the current patch added**. Every blueprint carries the game version it first appeared in; when the next patch lands, the new ones move in and the old ones drop out of the filter. The stamp stays |
| <img src="assets/symbole/18/gemerkt-gruen.png" width="22" alt=""> **Watchlist** | Click the star next to anything you are waiting for. When it shows up it is announced in gold, and **ticked off on the watchlist**, so the progress page shows how far along you are with your starred blueprints |
| <img src="assets/symbole/22/kuerzel-gruen.png" width="22" alt=""> **Class · size · grade** | Compact tag `class/size/grade` per blueprint, e.g. `M/1/A` (Military · Size 1 · Grade A) |
| <img src="assets/symbole/22/ton-gruen.png" width="22" alt=""> **Sound** | A short beep on every find: you don't have to watch the window |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **Info bar** | **A narrow bar of its own over the game**, on by default over the running game; you switch it off under *Settings → Overlay → Info bar*. It shows the server shard and, on request, three values with a switch each: **your FPS**, the **server FPS** and the **players** on the shard and on your server. VerseKit reads the values from the game picture; for that `r_DisplayInfo 1` (FPS) or `r_DisplayInfo 3` (players) must be on in the game. While mining, the scanner result stands on the right, or, if you set it so, in a small window above the signature. With the lock open you drag the bar into place (top left, middle or right), and VerseKit remembers it. **"Show without the game running too"** keeps it visible without Star Citizen |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **Handle search** | **Who is behind a handle or an org?** In the field of the info bar and on the *Handle search* page (group *Info*) with the tabs **Search** and **Relations**: **Enter** searches, **Ctrl+Enter** fetches the data anew from RSI, **Alt+Shift+H** takes you from the game into the search field of the info bar. The field takes a handle, the SID or the full name of an org. The card shows name, main organisation and further organisations, rank, SID and the profile at RSI. You mark handles and organisations as **friendly, neutral or hostile**; the **Relations** tab shows every mark filtered, with export and import, and an entry can be deleted on its own and brought back. For an organisation the card shows its **members** ("n of total loaded", with "Load more"); hidden members are missing from the list. **Searched lately** keeps the history. The **log monitor** shows kills, deaths and loading screens from the game log; a click on the handle searches it. VerseKit keeps found handles for as many days as you set. Modelled on the [Star Citizen Handle Query](https://github.com/Kuehlwagen/Star-Citizen-Handle-Query) by Kuehlwagen |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **On top of the game** | Borderless, slightly translucent overlay that follows the game: it sits on top of Star Citizen while you play |
| <img src="assets/symbole/22/schloss_auf-gruen.png" width="22" alt=""> **Pass clicks through** | One click on the lock in the bar and the overlay lets mouse clicks through: still in view, no longer in the way. The same lock turns green and brings it back with one click, with no detour through the settings |
| <img src="assets/symbole/22/verschieben-gruen.png" width="22" alt=""> **Movable & resizable** | Drag the title bar, resize at the ◢ handle: **position and size are remembered**. For more, set the **size of the overlay** and the **size of the info bar** from 100 to 200 percent under *Settings → Overlay*: text, spacing and windows grow with it |
| <img src="assets/symbole/22/darstellung-gruen.png" width="22" alt=""> **Six colour schemes** | Under **"Appearance"** with a small preview: **VerseKit (original), KRT (orange), Ice (cyan), Nebula (violet), Ember (red) and High contrast**, each with its own symbols, for the overlay and every window. The switch takes effect at once, without a restart. Pictures above under *The colour schemes* |
| <img src="assets/symbole/22/darstellung-gruen.png" width="22" alt=""> **Interface size** | Text, symbols and buttons **steplessly** with a slider, or the preset for your screen: Auto, Full HD, WQHD, UHD (125 %), UHD (150 %) |
| <img src="assets/symbole/22/darstellung-gruen.png" width="22" alt=""> **Sharp rendering** | Even on Windows with scaling above 100 %, such as 150 % on a 4K screen, VerseKit renders at full resolution itself instead of being upscaled and blurred by Windows. No switch, out of the box |
| <img src="assets/symbole/22/module-gruen.png" width="22" alt=""> **Modules** | Basetool, Ships, Crafting (with “What to farm”), Mining & refineries, Salvage, Trade, Procurement (shopping list and Contested Zones) and Statistics can be **hidden from the sidebar** if you don't need them (per module, whichever group its pages sit in); an empty group disappears. If you only want to mine, hide crafting and keep mining. Nothing is deleted |
| <img src="assets/symbole/22/startprogramme-gruen.png" width="22" alt=""> **Start programs** | Launch further programs **with the RSI Launcher**, once Star Citizen runs, or **instead** of the launcher. Optionally close them when the game ends |
| <img src="assets/symbole/22/startprogramme-gruen.png" width="22" alt=""> **Launch with MangoHud** (Linux) | A switch under *Installation & paths* turns on **MangoHud** when you start through "Launch RSI Launcher": frames per second, CPU and GPU load right in the game. MangoHud itself comes from your package manager; if it is missing, the page says where to get it |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **user.cfg** | On the **"Game"** page: what VerseKit writes, your own lines and the whole file, per game channel. The old file is backed up before the first change |
| <img src="assets/symbole/22/sprachen-gruen.png" width="22" alt=""> **German and English** | Interface switchable; the in-game blueprint message is recognised in both languages |
| <img src="assets/symbole/22/abhaken-gruen.png" width="22" alt=""> **Updates itself** | Checks for a new version every ten minutes, downloads it with a checksum and installs it as soon as Star Citizen is closed, **never while the game is running**. No switch to set. With „What's new" to read up on, including older releases |
| <img src="assets/symbole/22/nurlesend-gruen.png" width="22" alt=""> **Read only** | Reads `Game.log`. **One exception, and it asks first:** on request VerseKit writes the blueprint markers into `global.ini`. That can be undone at any time, and text from other tools is preserved. Plus the anti-AFK (Windows only, off by default and off again after every start): when switched on, it presses your key for “Wipe visor” after 5 minutes without input (you set the wait), just as you would yourself. Unless you rebound it, that is Alt+X |
| <img src="assets/symbole/22/eigenbuch-gruen.png" width="22" alt=""> **Own inventory** | Keeps track of which blueprints you have: no other program needed |
| <img src="assets/symbole/22/zeit-gruen.png" width="22" alt=""> **Play time** | The bar at the top shows **how long you have played**: in total, and while you are playing the current session next to it. Counted from the game's own logs, and **kept**: Star Citizen clears out its old logs, the count stays. The backup takes it along when you move machines |
| <img src="assets/symbole/22/statistik-gruen.png" width="22" alt=""> **Statistics** | A section with five tabs: the **overview** (play time, sessions, contracts and quantum jumps with a heat map of **when you play**), plus **ships & equipment** (most used and lost ships, weapons carried), **contracts**, **quantum travel** and **stability** (crashes, disconnects). It also shows what you spent in shops. Only what is reliably in the log in every game language is counted; each session is kept once and stays even when the logs are deleted. Can be saved as a file |
| <img src="assets/symbole/22/sprachen-gruen.png" width="22" alt=""> **Translation per channel** | Which text file Star Citizen reads is set **separately for each channel**: LIVE, HOTFIX, PTU, EPTU and Tech Preview, one card per channel with its language list and sources. Every channel gets text file, language and blueprint details, or nothing at all, if you would rather leave it alone. HOTFIX and the test channels can follow with “Same as LIVE”; the test channels then get the test version of the source. VerseKit fetches new releases for each channel by itself: it looks every five minutes, after a patch too, and the status line shows the state of the file and when it last fetched and checked. If another program puts its own text file in or switches the language, VerseKit puts the chosen one back and keeps the other one beside it. Available are **German** (Dymerz), **StarStrings** (MrKraken, PTU too), **French, Italian, Spanish, Portuguese and Turkish** (Dymerz), eight languages from the **SC Launch Configurator** (Luftwerft: German, English, French, Italian, Spanish, Bavarian, Dutch and Portuguese, for LIVE and PTU) and the game's own **English**, or **none at all**, leaving the text file untouched. Using a different one? Enter its **URL**: a zip or a `global.ini`, `https://` only; a GitHub file view is turned into the download for you, and anything that is not a game text file is rejected |
| <img src="assets/symbole/22/basetool-gruen.png" width="22" alt=""> **Sync with the KRT Profit Basetool** | Blueprints, My Inventory and your hangar sync with your Basetool, both ways. Plus the demand and your **unit's storage** (in the **Fleet & Logistics** group, one compact line per lot with type and tag); as a logistician you book there right from VerseKit, using the icons on each entry. Your own goods and items go into one of your units' storage with a button on their row in **My Inventory**, and come back the same way: you stay the owner; a switch **All / Personal / Shared** shows what lies where. You connect with a code you type in your browser; VerseKit never sees a password, and the credentials stay in your system's key store. Every area is off by default. Nothing is deleted silently, you decide where amounts differ, and your ships' purchase data never leaves. What is transferred is in the [privacy statement](PRIVACY.en.md). The connection sits under **Settings → Connection & sync**, the demand under **Procurement**, the unit storage under **Fleet & Logistics**, the same groups as in the Basetool's sidebar |
| <img src="assets/symbole/22/quellen-gruen.png" width="22" alt=""> **Connect to scmdb.net** | New blueprints and running missions land straight in your scmdb account: no export, no upload and no extra program. Turn it on under *Settings → General*, then enable "Watcher" on scmdb.net. Only LIVE and HOTFIX are taken over; it is off by default |
| <img src="assets/symbole/22/bestaetigt-gruen.png" width="22" alt=""> **All blueprints, or only the obtainable ones** | Star Citizen can craft a lot, but only some of the blueprints are handed out by contracts. The rest comes from kiosks, from events, or is tied to no mission at all. Out of the box only the obtainable ones count, so your progress is a number you can measure yourself against; under *Detection* you can add the rest |
| <img src="assets/symbole/22/diagnose-gruen.png" width="22" alt=""> **Report a problem in one click** | A field for one sentence, the finished report below it, a red button beside it. That is all. Up to four **screenshots** can go along if you like. The report holds system, packaging, game state and the last errors, but **neither your user name nor your paths** (a name appears only at the top as the reporter and can be cleared), and you see all of it beforehand. Prefer to write it yourself? One button hands you a prepared issue |
| <img src="assets/symbole/22/zeit-gruen.png" width="22" alt=""> **Catch-up** | Reads stored logs of earlier sessions **and the running one** on start, picking up what was unlocked while it wasn't running: finds are reported, not added silently. A **Read the logs again** button (overlay and settings) goes through everything once more on demand |
| <img src="assets/symbole/22/anzeige-gruen.png" width="22" alt=""> **Streamer mode** | Switch it on under *Settings → General → Privacy*: player name, paths, org and unit, Basetool name and device codes show as "[Redacted]" on every page and in the overlay. While it is on, the header shows the button "Streamer mode on". Off by default. The copy buttons still copy the plain text |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **Overlay unfolds** | Under *Settings → Overlay* you set per event whether it unfolds the overlay: blueprint, scanner hit, contract, finished refinery job and catalogue entry. By default only the blueprint unfolds it |
| <img src="assets/symbole/22/bestand-gruen.png" width="22" alt=""> **Purchases and sales as a suggestion** | What you buy in a shop in the game, *My Inventory* suggests under “New according to the log” for you to take over, with the price per piece; what you sell in a shop, for you to book out. You book or discard each entry singly or all at once with “Book all”, and you can undo it; nothing is booked by itself. Commodity sales at the trade terminal are not recognised. Can be switched off under *Settings → Detection* |
| <img src="assets/symbole/22/bestand-gruen.png" width="22" alt=""> **Moving home after a patch** | *My Inventory → “Move home”* takes every entry to your new home location and removes what the patch does not keep: food, drinks, pens and injectors, magazines. You choose beforehand which kinds go; ores and minerals stay by default. Both inventories are backed up first, afterwards VerseKit lists the removed entries to compare in the game, and the move can be undone |
| <img src="assets/symbole/22/raffinerie-gruen.png" width="22" alt=""> **Read the refinery** (Windows) | With the refinery terminal and its yield in front of you, **Alt+Shift+R** reads it, in the German game too. Then the inventory opens with the lines it read, for you to check and take over. Longer than the screen? Scan, scroll in the terminal, scan again: both scans become one job, duplicate lines count once. Under *Mining* your refinery jobs stand with each ore folding away, and the overlay shows them with the time left |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **What the overlay shows** | Under *Settings → Overlay* you switch server, location, running contracts, refinery jobs and the update notice on and off one by one. The head of the contracts counts running ones against the maximum (“3/10”) you set there. The overlay snaps to the screen edge, and so does the info bar |
| <img src="assets/symbole/22/eigenbuch-gruen.png" width="22" alt=""> **Notes** | At the top of the sidebar, below the overview: your notepad. **Bold, italic, underline, strikethrough, bullets, numbering and small tables** with a button or with Ctrl+B, Ctrl+I and Ctrl+U. You see the formatting straight away, no Markdown signs show up in the text. The first line is the name of the note |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **Own title bar** | VerseKit draws the bar at the top of the main window itself, with its own buttons. On Windows 11 the snap layouts appear at the maximise button |
| 🐧 **Windows and Linux** | One build for both systems, including autostart and log language detection. On Linux you add VerseKit to the application menu under *Settings → General*, the signal sound comes through your desktop's sounds and the system notice through `notify-send`. On **KDE** the overlay sits over the full-screen game without an extra start argument; the environment variable `VERSEKIT_X11_FENSTERTYP` picks another window type and takes precedence. Tray icon and scanner exist on Windows only |

</details>

<details>
<summary><b>Installation in detail</b></summary>

**Requirements:** Windows or Linux and an installed Star Citizen. VerseKit looks for the folder containing `Game.log`, on Linux in the usual Wine prefixes (lug-helper, Lutris, Bottles also as a Flatpak, Heroic, Steam with Proton). If nothing is found, the wizard asks. Nothing else is needed: no account, nothing to install on top.

**Windows:** `VerseKit-Setup.exe` installs with a start menu entry, optionally a desktop icon and autostart, and can be removed cleanly through *Apps & Features*. A standalone `.exe` without an installer has been gone since v3.0.0: an update put the new version beside the old file, and anyone clicking their usual shortcut kept using the old version without noticing. With the installer an update really replaces it.

**Linux:** `VerseKit-x86_64.AppImage` is a single file. Make it executable once (right click → Properties → *Executable as program*, or `chmod +x VerseKit-x86_64.AppImage`). The wizard offers an application menu entry if you want one; later you find it under *Settings → General*.

**On first start** a wizard walks you through setup: language, finding Star Citizen, collecting your existing blueprints, display, startup and details in the game. It takes a minute, and then your collection is there.

#### ⚠️ Windows says "Windows protected your PC"

This appears on the first launch, and it is **not a virus detection**:

> Microsoft Defender SmartScreen prevented an unrecognised app from starting.

**To run it anyway:** click **More info** → **Run anyway**. It will not ask again.

**Why this happens:** SmartScreen does not check whether a program is harmful: it checks whether it is **known**. A file becomes known through a purchased code-signing certificate (several hundred euros a year) or by being downloaded by very many people. A free fan tool has neither, and every new version starts from zero again.

**If you would rather not take my word for it, you don't have to:**

- The **source is open** ([here](https://github.com/Xharig/VerseKit)), and the file is not built by me but by **GitHub Actions** from exactly that source. The build is there to read: [`.github/workflows/`](.github/workflows/)
- Every file on the releases page carries its **SHA-256 checksum**: GitHub shows it directly
- The build has every version checked on **VirusTotal**: the result and a link to the report sit at the bottom of the [release notes](https://github.com/Xharig/VerseKit/releases/latest). Individual scanners occasionally flag new, still unknown programs through machine-learning heuristics, a known false positive

On **Linux** this message does not exist: the file just needs to be made executable once.

#### Code signing

This project has applied to the [SignPath Foundation](https://signpath.org/)
for free code signing for open source projects. Once approved, released
Windows binaries will be signed by SignPath, and Windows will show the
publisher name instead of "unknown publisher".

Builds are produced exclusively by a public GitHub Actions workflow. See
[SECURITY.en.md](https://github.com/Xharig/VerseKit/blob/main/SECURITY.en.md) for how releases are built, and
[PRIVACY.en.md](https://github.com/Xharig/VerseKit/blob/main/PRIVACY.en.md) for what the program reads, fetches and
sends (and what not).

> ℹ️ Verified against a real Star Citizen installation, with both a **German and an English** game client. Feedback from other machines is still welcome: different install locations, different screen setups, Windows. As an [issue](https://github.com/Xharig/VerseKit/issues).

</details>

<details>
<summary><b>Building from source</b></summary>

For the curious and for developers. You need [Rust](https://www.rust-lang.org/tools/install) 1.99 or newer.

```bash
git clone https://github.com/Xharig/VerseKit.git
```

```bash
cd VerseKit
```

```bash
cargo build --release
```

The finished files on the releases page are built by **GitHub Actions** on every version tag from exactly this source ([workflows](.github/workflows/)). Nobody builds them by hand, not even the author.

</details>

<details>
<summary><b>Using the overlay</b></summary>

The narrow bar sits above the game and reports new finds. Everything else is behind the symbols in its title bar:

| Symbol | What it does |
|---|---|
| <img src="assets/symbole/22/glocke-grau.png" width="22" alt=""> | **Bell**: a new build is available; turns green as soon as there is one |
| <img src="assets/symbole/22/starten-grau.png" width="22" alt=""> | **Rocket**: launch the RSI Launcher. Only appears if a way to start it was found |
| <img src="assets/symbole/22/einstellungen-grau.png" width="22" alt=""> | **Gear**: open or bring back the main window: blueprint list, hangar, workshop, settings |
| <img src="assets/symbole/22/schloss_auf-grau.png" width="22" alt=""> | **Lock**: pass mouse clicks through to the game. It turns green and closed while they do; one click on it catches them again |
| <img src="assets/symbole/22/einklappen-grau.png" width="22" alt=""> | **Chevron**: fold the overlay down to just its bar |
| <img src="assets/symbole/22/leeren-grau.png" width="22" alt=""> | **Eraser**: clear the messages on screen. Your blueprints stay |
| <img src="assets/symbole/22/schliessen-grau.png" width="22" alt=""> | **Cross**: ends VerseKit, with every open main window |

| Action | How |
|---|---|
| Move the window | Drag the bar at the top |
| Resize | Drag the **◢** handle at the bottom right |

The hints of the buttons appear in a small window of their own next to the pointer, even when the overlay is folded.

**The X of the main window only hides it**; VerseKit keeps running. Bring it back with the icon next to the clock, the gear in the overlay or a second start. VerseKit ends with the cross in the overlay. Without a running overlay, while a backup is being written, and on an update or restart, closing the main window ends the program.

What the coloured dots mean:

| | |
|---|---|
| <img src="assets/symbole/18/bestaetigt-gruen.png" width="18" alt=""> | Blueprint unlocked: it's in your inventory |
| <img src="assets/symbole/18/punkt-blau.png" width="18" alt=""> | Became newly craftable **in the game**: not something *you* have yet |
| <img src="assets/symbole/18/gemerkt-gelb.png" width="18" alt=""> | Something from your watchlist has appeared |
| <img src="assets/symbole/18/hinweiszeile-grau.png" width="18" alt=""> | A note, not an unlock (e.g. a gap in your inventory) |

</details>

<details>
<summary><b>Test versions and reporting problems</b></summary>

Before every release there are **test versions**, recognisable by the `rc` in the number. They are listed under [Releases](https://github.com/Xharig/VerseKit/releases), each with what it brings and what changed since the previous one. They are only offered if you switch them on: pick **Test version** on the **Update & About** page. From then on they arrive the same way as every update, nothing to download by hand.

- **Test versions are fully built and runnable**, but have not been proven for long.
  Something may act up: that is exactly what they are for.
- **The way back is always open.** Switch it off again and you will be offered the next
  finished version: a finished version always counts as newer than any test version of
  the same number. So nobody gets stuck on the test channel by accident.
- **Without this setting you never see a test version.** If you want peace and quiet,
  do nothing: that is the default.
- **If you already run a test version, you get further test versions by themselves.**

Found something? The quickest way is the **Report a problem** page: there is a field
"What happened?" for one sentence, the finished report below it, and the red
**Send error report** button sends it straight off. **Attach image** adds up to four
screenshots. That is all you need to do. The report
holds everything needed to track a problem down and no personal information; you see
all of it beforehand.

Prefer to do it yourself? An [issue](https://github.com/Xharig/VerseKit/issues) works just as well: the **GitHub
issue …** button on the same page fills the report in for you. Or the **Report a bug**
forum on [Discord](https://xharig.com/discord), if a screenshot is quicker than a description.

Work continues, and what exactly is not on a list. What a version brought you can read in [`CHANGELOG.en.md`](https://github.com/Xharig/VerseKit/blob/main/CHANGELOG.en.md) or right in the tool under **"What's new"**. What a test build contains is in its release notes; this page always shows the **released** version.

Wishes and bug reports are welcome as an [issue](https://github.com/Xharig/VerseKit/issues) or on [Discord](https://xharig.com/discord). Suggestions make it into the next build more reliably than mind reading.

</details>

<details>
<summary><b>Your data and the network</b></summary>

🔒 **It's yours.** No account, no sign-in, no cloud. VerseKit reads files that are on your disk anyway. It only changes the game installation where you explicitly switch something on (such as the details in the contract texts), and backs up the file first.

**What VerseKit fetches:**

| What for | How often |
|---|---|
| Values and origins from scmdb.net | once per game version |
| Resource prices and storage locations from UEX Corp | at most once a day |
| Reputation thresholds of ranks from scunpacked-data | once per game version |
| Slots from erkul.games, only for ships missing from the game data of your installation | once per game patch |
| Contract texts and translations | when you switch them on |
| Whether there is a new version | in the background |
| CIG's server status | while the page is open |
| A handle search at RSI | when you search, or anew with Ctrl+Enter |
| The start time of the Executive Hangar from contestedzonetimers.com | when the Contested Zones page opens, at most every six hours |

**What VerseKit sends:**

| What | When |
|---|---|
| Usage count: version, system, language and which pages were opened how often, without an ID | once a day, on by default, can be switched off |
| Sync with the KRT Profit Basetool or your scmdb.net account | only if you switch it on |
| Problem report | only when you press the button yourself, and you see its contents first |

What exactly each message contains is in the [privacy policy](https://github.com/Xharig/VerseKit/blob/main/PRIVACY.en.md). **All fetching and sending** can be switched off with `SC_BP_NO_NET=1`.

The info bar reads its values from the game picture on your own machine: every few seconds the top right corner, and only while Star Citizen is in front.

**Passing it on:** Just pass on the file from the [releases page](https://github.com/Xharig/VerseKit/releases). The recipient only needs Star Citizen.

</details>

📖 **[Manual](MANUAL.en.md):** how VerseKit spots blueprints, which files it creates, your own paths, Windows and Linux on one machine, the watchlist with patterns, settings and environment variables.

**New to Star Citizen?** Sign up with my referral code [`STAR-YC7L-S4W2`](https://robertsspaceindustries.com/enlist?referral=STAR-YC7L-S4W2) and you get 50,000 UEC of in-game credit after buying a game package.

## Star Citizen Fan Content

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/fankit/MadeByTheCommunity_White.png">
    <img alt="Star Citizen: Made by the Community" src="assets/fankit/MadeByTheCommunity_Black.png" width="150" height="150">
  </picture>
</p>

> This is an unofficial Star Citizen fan site, not affiliated with the Cloud Imperium group of
> companies. All content on this site not authored by its host or users are property of their
> respective owners.

VerseKit is an unofficial, non-commercial fan project for the *Star Citizen* community.
It is **not affiliated with, endorsed, sponsored, or approved by** Cloud Imperium Rights LLC,
Cloud Imperium Rights Ltd., or Roberts Space Industries.

This project makes use of assets from the official
[Star Citizen Fankit](https://robertsspaceindustries.com/fankit). Those materials are published
for fan use and may only be used as explained by the terms of the **Fankit Agreement**, the
**Fan Style Guide**, and the
[Roberts Space Industries Terms of Service](https://robertsspaceindustries.com/tos),
specifically the section on User Generated Content (UGC).

> **Star Citizen®, Roberts Space Industries® and Cloud Imperium® are registered trademarks of
> Cloud Imperium Rights LLC.**

All other Star Citizen content, artwork, names, logos and trademarks are the property of their
respective owners. © 2025 Cloud Imperium Rights LLC and Cloud Imperium Rights Ltd.

Official site: **[robertsspaceindustries.com](https://robertsspaceindustries.com)**

## License

**GNU General Public License v3.0**: full text in [LICENSE](LICENSE).

In short: you may use, modify and pass on the program. Whoever passes it on, modified or not, has to include the source under the same licence. There is no warranty. If you fork this project, please keep the credit in the footer or mention the original source.

## Thanks

- **[scmdb.net](https://scmdb.net)**: blueprint origins and values to cross-check against.
- **[UEX Corp](https://uexcorp.space)**: resource prices, shops and storage locations.
- **[erkul.games](https://erkul.games)**: slots of the ships missing from the game data.
- **[Star Citizen Wiki](https://github.com/StarCitizenWiki/scunpacked-data)**: reputation thresholds of ranks.
- **Maluh**: the [Fleetmanifest](https://github.com/MaluhTV/SC_Fleetmanifest) behind the fleet analysis.
- **[AlyxOne](https://robertsspaceindustries.com/community-hub/post/star-citizen-hangar-extension-browser-add-on-7xzYDnJDV6c2W)**, **[dolkensp](https://github.com/dolkensp/HangarXPLOR)** and **[Drachennarr](https://github.com/Drachennarr/Hangar-Exporter-Releases/releases/latest)**: the browser extensions for the hangar import.
- **[greluc](https://github.com/krt-profit/basetool)**: the Profit Basetool and the [DPoP reference](https://krt-profit.github.io/basetool/) for the sign-in.
- **[Luftwerft](https://www.luftwerft.com/)**, **[Dymerz](https://github.com/Dymerz/StarCitizen-Localization)**, **[Thord82](https://github.com/Thord82/Star_citizen_ES)** and **[MrKraken](https://starstrings.app)**: the translations and StarStrings.
- **[Kuehlwagen](https://github.com/Kuehlwagen/Star-Citizen-Handle-Query)**: the model for the handle search and the log monitor.
- **[Zane_DragonBorn](https://robertsspaceindustries.com/community-hub/post/accurate-pyro-contested-zone-maps-c-zs-7l8Ih9mXyPDpu)** and **[Kerast](https://www.reddit.com/r/starcitizen/comments/1i3iyq9/supervisor_exectutive_hangar_maps_pyamsupvisr345/)**: the Contested Zones maps.
- **[Appieman](https://contestedzonetimers.com/)**: the start time of the Executive Hangar from contestedzonetimers.com.
- **[Lucide](https://lucide.dev)**: the interface symbols.
- **[flag-icons](https://flagicons.lipis.dev)**: the flags of the language choice.

<details>
<summary><b>Sources and licences in detail</b></summary>

The origin of each blueprint comes from the **[Star Citizen Mission DataBase (scmdb.net)](https://scmdb.net)**, a hobby project that prepares the game data and makes it freely available. The values for type, size, grade and class and for crafting are read by VerseKit from the game data of your own installation first; scmdb.net serves as the cross-check and steps in where the game data cannot be read. **Thank you** for that! 🙏

> VerseKit **does not ship this data**; it fetches it on your machine directly from scmdb.net, the way a browser would. scmdb is licensed under [CC BY-NC-ND 4.0](https://creativecommons.org/licenses/by-nc-nd/4.0/); a bundled copy would be redistribution and would conflict with that licence as well as with this project's GPL. Fetching is sparing: only when a **new game version** is out.

As a base for the blueprint details you can pick **[StarStrings](https://github.com/MrKraken/StarStrings)** by **MrKraken**, cleaned-up English game text, used across many organisations. **Thanks** to MrKraken! 🙏

> StarStrings is **not bundled** either; it is fetched from its own address when you ask for it. The project states no licence: all the more reason the text stays his.

**VerseKit gets along with other tools.** StarStrings and other tools mark blueprint contracts too, with the same `[BP]` mark. So VerseKit adds **no second mark where one already stands**, and leaves any item name alone that already carries a tag. Where another tool's blueprint list already stands in a contract, VerseKit's own list **replaces** it instead of sitting beside it, with the **tick boxes** for your own collection. Take the details back out and the other tool's state is there again, character for character.

**The translations** come from **[Luftwerft](https://www.luftwerft.com/)**, **[Dymerz](https://github.com/Dymerz/StarCitizen-Localization)** (German, French, Italian, Spanish, Portuguese, Turkish) and **[MrKraken](https://starstrings.app)** (StarStrings, English with contract markers). These are **not bundled** either. VerseKit fetches the one you pick on your machine and keeps it current.

**Luftwerft** offers German, English, Français, Italiano, Español, Bavarian, Nederlands and Português for LIVE and PTU in the **[SC Launch Configurator](https://www.luftwerft.com/)**. Its developer has released them for VerseKit. You pick them under *Translation*. They are generated largely automatically from the game data and are still being polished; VerseKit shows the developer's note before you switch. **Thanks** to Luftwerft, Dymerz, Thord82 and MrKraken! 🙏

**Resource prices** come from **[UEX Corp](https://uexcorp.space)**, a data project maintained by players. That is what puts a price next to every missing ingredient, or says it cannot be bought at all. These data are **not bundled** either; they are fetched on your machine, at most once a day. **Thanks** to UEX Corp! 🙏

**Reputation thresholds of ranks** ("Guild Member [10,000+]") come from **[scunpacked-data](https://github.com/StarCitizenWiki/scunpacked-data)** by the **Star Citizen Wiki**, the game files, unpacked for every patch and made freely available. They are **not bundled** either; they are fetched on your machine, once per game version. **Thanks** to the Star Citizen Wiki! 🙏

**The ship slots** VerseKit reads from the game data of your own installation. Where those lack a ship or leave a slot without fitting parts, **[erkul.games](https://erkul.games)** steps in, the loadout tool the Star Citizen community has used for years. These data are **not bundled** either; they are fetched on your machine, only for the ships you actually added (plus the lists of racks, missiles, bombs and paints), and only once per game patch. **Thank you** to erkul.games! 🙏

**The hangar import** is made possible by browser extensions: the **[Hangar-Exporter](https://github.com/Drachennarr/Hangar-Exporter-Releases/releases/latest)** by **Drachennarr** (GPL-3.0), which saves the hangar as a file or sends it without a file straight to VerseKit on this machine, the **[Star Citizen: Hangar Extension](https://robertsspaceindustries.com/community-hub/post/star-citizen-hangar-extension-browser-add-on-7xzYDnJDV6c2W)** by **AlyxOne** (MIT licence, maintained, recommended) and the original **[Star Citizen Hangar XPLORer](https://github.com/dolkensp/HangarXPLOR)** by **dolkensp** (MIT licence). They fetch your pledge store, and VerseKit reads the files. Without them every ship would have to be typed in by hand. **Thank you!** 🙏

**The Profit Basetool** by **greluc** (GPL-3.0) syncs with VerseKit, and its **[DPoP reference](https://krt-profit.github.io/basetool/)** (MIT) signs the sign-in without the key ever leaving your machine. The reference is built into the program. **Thanks** to greluc! 🙏

**The handle search and the log monitor** are modelled on the **[Star Citizen Handle Query](https://github.com/Kuehlwagen/Star-Citizen-Handle-Query)** by **Kuehlwagen** (MIT), rebuilt with code of our own. **Thanks** for the idea and the model! 🙏

**The Contested Zones maps** are built into VerseKit: Checkmate, Orbituary and Ruin Station by **Zane_DragonBorn** (to the [post](https://robertsspaceindustries.com/community-hub/post/accurate-pyro-contested-zone-maps-c-zs-7l8Ih9mXyPDpu)), PYAM-EXHANG-0-1 and PYAM-SUPVISR-3-4/5 by **Kerast** (to the [post](https://www.reddit.com/r/starcitizen/comments/1i3iyq9/supervisor_exectutive_hangar_maps_pyamsupvisr345/)). Both released them for free use if they are named. VerseKit reads the start time of the Executive Hangar from **[contestedzonetimers.com](https://contestedzonetimers.com/)**; **Appieman** runs the site and allowed it. **Thanks** to all three! 🙏

The interface symbols come from the **[Lucide](https://lucide.dev)** set (ISC licence), all drawn on the same grid with the same stroke width, which is why they look identical on Windows, Linux and macOS. **Thanks** to the Lucide community! 🙏 The licence text ships with the tool (`assets/symbole/LIZENZ.txt`) and is shown under **Thanks & Licenses**.

The flags at the language choice come from **[flag-icons](https://flagicons.lipis.dev)** (MIT licence, © 2013 Panayiotis Lipiridis). **Thanks** for them! 🙏

VerseKit is an independent, unofficial companion tool with **no** official connection to Cloud Imperium Games. All brand and project names belong to their respective owners.

</details>

<div align="center">

[![Xharig](https://github.com/Xharig.png?size=80)](https://github.com/Xharig)

**Xharig**: development and design of this project

<sub>Like the tool? <a href="https://ko-fi.com/xharig">Ko-fi</a> ☕</sub>

</div>
