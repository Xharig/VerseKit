<div align="center">

<img src="assets/icon.png" alt="SC BP Watcher icon" width="128">

# SC BP Watcher

**Live overlay that shows new Star Citizen blueprints the moment you unlock them**

<sub>Windows · Linux · no account, no cloud — installer on Windows, single file on Linux</sub>

[![Version](https://img.shields.io/github/v/release/Xharig/SC-BP-Watcher?include_prereleases&label=Version&color=5fa522)](../../releases)
[![Downloads](https://img.shields.io/github/downloads/Xharig/SC-BP-Watcher/total?label=Downloads&color=5fa522)](../../releases)
[![License](https://img.shields.io/badge/License-GPL--3.0-5fa522)](LICENSE)
[![Discord](https://img.shields.io/badge/Discord-join-5fa522?logo=discord&logoColor=white)](https://discord.gg/g2E7e6XxZC)
[![Ko-fi](https://img.shields.io/badge/Ko--fi-buy%20me%20a%20coffee-5fa522?logo=kofi&logoColor=white)](https://ko-fi.com/xharig)
[![Python](https://img.shields.io/badge/Python-3.8%2B-0a4a7a?logo=python&logoColor=white)](https://www.python.org/)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%C2%B7%20Linux-0a4a7a)](#requirements)
[![Star Citizen](https://img.shields.io/badge/Star%20Citizen-compatible-0a4a7a)](https://robertsspaceindustries.com/)

[Deutsch](README.md) · **English**

</div>

---

A small, borderless overlay that tells you **in real time** when a new blueprint drops — name, type and time. No account, no cloud. Runs on **Windows and Linux**.

> 💬 **There is a Discord.** Questions, help with problems, new releases and a forum for bugs and wishes: **[discord.gg/g2E7e6XxZC](https://discord.gg/g2E7e6XxZC)**. If you would rather stay here, open an [issue](../../issues) — both are read.

> 🧪 **Trying a test build.** Before every release there are **pre-releases** (`-rc`) under [Releases](../../releases) — each one says what it brings and what changed since the previous one. They are **never offered as an update** to anyone: if you want one, you download it there. If you try one and find something, please open an [issue](../../issues) — that is exactly what they are for.

> ℹ️ **The SC Deutsch Launcher is no longer required.** The actual source is Star Citizen's own `Game.log` — every unlocked blueprint is written there in plain text. If the launcher is installed it is still used: it confirms finds and supplies German names. If it isn't (always the case on Linux), the watcher works anyway.

<table>
<tr>
<td width="32%" valign="top" align="center">
<img src="assets/screenshot-overlay-en.png" alt="The overlay while playing" width="100%"><br>
<sub>The overlay — narrow, always on top, opacity adjustable</sub>
</td>
<td width="68%" valign="top" align="center">
<img src="assets/screenshot-liste-en.png" alt="The window with the blueprint list" width="100%"><br>
<sub>The blueprint list — search, five filters, and where each blueprint comes from</sub>
</td>
</tr>
</table>

### In game, without tabbing out

The watcher writes into the game's mission text **which** blueprints a contract hands out — with `[x]` for the ones you already have. The count is in the title, the names are in the description.

<table>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-ingame-teils.jpg" alt="Contract with some blueprints already owned" width="100%"><br>
<sub><b>3 of 6</b> — <code>[x]</code> you have, <code>[&nbsp;&nbsp;]</code> still missing</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-ingame-keine.jpg" alt="Contract with none of the blueprints owned" width="100%"><br>
<sub><b>0 of 12</b> — nothing here that you already own</sub>
</td>
</tr>
</table>

### My ships

A new blueprint immediately raises the next question: **does this part even fit any of my ships?** For that the tool has to know what is in your hangar — the game records it nowhere.

<table>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-hangar-en.png" alt="My hangar with four ships added" width="100%"><br>
<sub><b>My hangar</b> — pulled from the pledge store in one go or added by hand; after that, crafting tells you which of your ships a blueprint fits</sub>
</td>
</tr>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-wunschliste-en.png" alt="Wishlist with price and planned loadout" width="100%"><br>
<sub><b>Wishlist</b> — what you are aiming for, with price and location; the loadout can be planned before you own the ship</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-einkaufsliste-en.png" alt="Still missing: the bill across all ships" width="100%"><br>
<sub><b>Still missing</b> — the bill across all ships: buy or build per item, with total and shopping route</sub>
</td>
</tr>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-bergung-en.png" alt="What is inside a wreck" width="100%"><br>
<sub><b>What is inside?</b> — what a wreck carries from the factory and what the parts are worth in a shop</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-zerlegen-en.png" alt="Worth dismantling: what the fabricator returns" width="100%"><br>
<sub><b>Worth dismantling?</b> — what the fabricator returns, and which materials vanish for good</sub>
</td>
</tr>
</table>

### The workshop

The blueprint is the start. The workshop answers what comes after it: **what do I need, do I have it, and what will it turn into?**

<table>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-herstellung-en.png" alt="Crafting with an expanded recipe" width="100%"><br>
<sub><b>Crafting</b> — ingredients, craft time and what <i>your</i> material does to the stats</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-lager-en.png" alt="Material storage with recorded resources" width="100%"><br>
<sub><b>Material storage</b> — material, amount, quality and storage location, kept by hand</sub>
</td>
</tr>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-bergbau-en.png" alt="Mining showing where Iron is found" width="100%"><br>
<sub><b>Mining</b> — type a resource and see where it sits; or enter the scanner reading and see what it found</sub>
</td>
</tr>
</table>

### Trading

Cargo hold full — now what? **Where do I offload it, and what does it pay per SCU?**

<table>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-handelslager-en.png" alt="Cargo hold with entered goods" width="100%"><br>
<sub><b>Cargo hold</b> — what you carry to sell, kept apart from your material storage</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-verkauf-en.png" alt="Selling tab with the best buyers" width="100%"><br>
<sub><b>Selling</b> — the best buyers, sorted by how many of your goods a place takes</sub>
</td>
</tr>
</table>

### The window

> [!NOTE]
> The screenshots are regenerated before every release and show this version. On an older version some of what is shown here does not exist yet.

<table>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-fortschritt-en.png" alt="Progress by area" width="100%"><br>
<sub><b>Progress</b> — per area, details on click</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-auftragstexte-en.png" alt="Mission text settings" width="100%"><br>
<sub><b>In-game text</b> — pick a text source, switch it on and off</sub>
</td>
</tr>
<tr>
<td valign="top" align="center">
<img src="assets/screenshot-bestand-en.png" alt="Export and import your inventory" width="100%"><br>
<sub><b>Inventory</b> — export for the basetool, or import an existing one</sub>
</td>
<td valign="top" align="center">
<img src="assets/screenshot-anzeige-en.png" alt="Display settings" width="100%"><br>
<sub><b>Display</b> — pop-up mode, click-through, font size</sub>
</td>
</tr>
<tr>
<td valign="top" align="center">
<img src="assets/screenshot-ueber-en.png" alt="About and update channel" width="100%"><br>
<sub><b>About</b> — stable releases or test builds, with a button to fetch one</sub>
</td>
<td valign="top" align="center">
<img src="assets/screenshot-wasistneu-en.png" alt="What's new" width="100%"><br>
<sub><b>What's new</b> — every release expandable, filtered by kind</sub>
</td>
</tr>
</tr>
<tr>
<td valign="top" align="center">
<img src="assets/screenshot-danke-en.png" alt="Thanks and licenses" width="100%"><br>
<sub><b>Thanks &amp; Licenses</b> — what belongs to whom, and who helped</sub>
</td>
<td valign="top" align="center">
<img src="assets/screenshot-serverstatus-en.png" alt="Server status" width="100%"><br>
<sub><b>Server status</b> — is Star Citizen up?</sub>
</td>
</table>

<details>
<summary>And the rest: General</summary>

<table>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-allgemein-en.png" alt="General settings" width="100%"><br>
<sub><b>General</b> — language, sound, autostart, menu entry</sub>
</td>
<td width="50%" valign="top" align="center">
</td>
</tr>
</table>

</details>

### What dead zone, saturation and sensitivity do

Under **Advanced → Axes & curves** you can set how sharply each stick axis responds. Three values, each bending the curve differently:

<img src="assets/erklaerung-kurve-en.png" alt="Four curves: nothing set, with dead zone, with saturation, with sensitivity" width="100%">

| Value | What it does | What for |
|---|---|---|
| **Dead zone** | The start of the travel does nothing | Against jittery sticks that will not sit still in the centre |
| **Saturation** | Full deflection before the mechanical stop | When the stick does not travel all the way, or full effect should come sooner |
| **Sensitivity** | Above 1 the curve sags, below 1 it bulges | Above 1: finer aiming around the centre. Below 1: more direct, sharper |

The dashed line shows how it would run with nothing set. The dark areas are the travel where nothing changes any more.

> [!NOTE]
> Star Citizen ties these values to an internal device number, not to its name. When a stick gets a new number — a different USB port, new firmware — the game treats it as a new device, and the old values stay in the file without any effect. The watcher finds such leftovers and clears them out on request.

## Why this tool

There are several blueprint lists. Four things make the difference day to day:

- **You never leave the game.** The overlay sits on top of Star Citizen. No second window, no alt-tab, no browser — the new blueprint is simply there while you keep playing.
- **It knows what you already have.** The watcher keeps your blueprint inventory itself and reads Star Citizen's stored session logs on first start — you get your existing collection for free, without typing anything. If a gap remains anyway, it says so instead of passing off an incomplete list as complete.
- **It tells you where to get what's missing.** For **670 of the 738** blueprints it shows which faction offers it, in which contract, from which standing, and what it pays — sorted by the easiest route. "I'm missing X" is half the information; "X drops at Foxwell from Veteran Contractor" is all of it.
- **Nothing leaves your machine.** No account, no sign-in, no cloud. It reads files that are already on your disk and writes nothing back into the game.

On top of that: class, size and grade are right there in the line (`M/1/A`), the interface speaks German and English, and the whole thing runs on the plain Python standard library — no extra packages, no dependencies that break tomorrow.

## Features

| | |
|---|---|
| <img src="assets/symbole/22/blitz-gruen.png" width="22" alt=""> **Instant** | Reads Star Citizen's `Game.log` → the blueprint is in the list **within seconds** |
| <img src="assets/symbole/22/liste-gruen.png" width="22" alt=""> **Keyboard shortcut** | **Ctrl+Alt+B** brings the blueprint list to the front — from inside the full-screen game, no blind hunting for the window. Exactly that one combination is registered; nothing else is listened to |
| <img src="assets/symbole/22/liste-gruen.png" width="22" alt=""> **Blueprint list** | Search everything, grouped by type, filters *all / owned / missing / watching / new in game*, with progress. Tick items with one click |
| <img src="assets/symbole/22/herkunft-gruen.png" width="22" alt=""> **Where it drops** | The **"Where from?"** button shows faction, contract, required standing and payout — for **670 of 738** blueprints, sorted by the easiest route. From **Crafting** a button leads straight there: missing the blueprint, one click tells you which contract to run |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **Contract accepted** | Accept a contract and you see right away whether blueprints are part of it — and **which of those you are still missing**. If the catalogue does not know the contract, it stays quiet rather than guessing |
| <img src="assets/symbole/22/liste-gruen.png" width="22" alt=""> **What to do next** | Every running contract lists its **open objectives** underneath — "Disable the Hartmoore inverter", "Locate and reset the node". They come from the same log and move on as soon as you finish one |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **Mission log** | Which missions you played when, how often — and **which blueprint came out of it**. The answer to "which mission was it again that dropped the helmet?". Already filled on first start: the game's kept logs reach back weeks. From then on it grows with you and stays, long after the game has dropped its own |
| <img src="assets/symbole/22/blitz-gruen.png" width="22" alt=""> **Crafting** | For each of the **1,597** craftable items: the ingredients with amounts and the craft time — and whether you have the blueprint. Clicking a resource jumps to where it can be mined |
| <img src="assets/symbole/22/herkunft-gruen.png" width="22" alt=""> **Mining** | Both directions in one search: type a resource → its locations (Iron: 27). Type a location → what is found there (Daymar: 14 ores). With mining type FPS · vehicle · ship, a **refinery comparison** per ore (which station gives the best bonus — 18 percentage points apart for Bexalite) and the **scan signature** to recognise it in game |
| <img src="assets/symbole/22/herkunft-gruen.png" width="22" alt=""> **Which refining method?** | The terminal offers nine methods and shows a single line for each. Tell it what matters to you instead — **yield, cost or speed** — and it names the method. Two of the nine are never worth taking: another one beats them on every count, and the tool says so |
| <img src="assets/symbole/22/bestand-gruen.png" width="22" alt=""> **Material storage** | Enter what resources you have — **material, amount, quality, location**. Recipes then show what is missing, and a button subtracts the ingredients when you craft. **And because the recipes carry how material quality changes the item\'s values, you see what *your* material would produce** |
| <img src="assets/symbole/22/bestand-gruen.png" width="22" alt=""> **Prices** | What a material costs at the terminal and what it sells for — the numbers come from **[UEX Corp](https://uexcorp.space/)** and refresh daily. That way crafting answers not only "what am I missing" but also "what will it cost me". Without a connection the column simply stays empty |
| <img src="assets/symbole/22/hangar-gruen.png" width="22" alt=""> **My hangar** | **Which ships you own — and whether a blueprint even fits in one.** You can pull your hangar out of the pledge store in one go: the browser add-on [Star Citizen Hangar XPLORer](https://github.com/dolkensp/HangarXPLOR) puts it into a file, and the watcher reads it along with LTI and pledge names. Ships bought in-game go in by hand right next to it — every ship remembers where it came from. After that, crafting shows under every blueprint **which of your ships the part fits** and how many slots it has there. Slot data comes from [erkul.games](https://erkul.games) and stays on your machine; only what you actually have in your hangar is fetched, and only once per game patch |
| <img src="assets/symbole/22/wunschliste-gruen.png" width="22" alt=""> **Wishlist** | **What you are aiming for — with price, location and a loadout you can plan in advance.** Add a ship and see what it costs in game and where it is sold. The loadout can be planned **before** you own it — while the total is still a decision, not a receipt. Nothing here shows up under "fits your ship": a wish is not a possession |
| <img src="assets/symbole/22/einkaufsliste-gruen.png" width="22" alt=""> **Still missing** | **The bill across all your ships.** Set what belongs in each slot — every part shows **grade and class** ("A · Military · blueprint only") so you can build towards a purpose without knowing 1,500 names. **Military parts** are included: no shop sells them, only blueprints. Per item you choose **buy or build**, and below stands the total with a shopping route of as few stops as possible. Tick off what you fitted — it leaves the list and the total |
| <img src="assets/symbole/22/farmliste-gruen.png" width="22" alt=""> **What to farm** | **Your stock weighed against everything you want to build.** Across all items at once, not recipe by recipe: two parts needing 2 Iron each with 3 Iron in stock — checked individually both say "enough", together one is missing. Ore below the required quality does not count as stock but is named rather than passed over |
| <img src="assets/symbole/22/sicherung-gruen.png" width="22" alt=""> **What is inside?** | **What a wreck carries from the factory — and what it is worth in a shop.** Type a ship and see every fitted part with its shop value. ⚠ NPC wrecks are lootable; player ships become useless once the insurance is claimed — stripped parts are then worthless too. That warning comes **before** any number |
| <img src="assets/symbole/22/zerlegen-gruen.png" width="22" alt=""> **Worth dismantling?** | **What the fabricator returns, before you start cutting.** 50 % of the materials — but **six never come back**, Quantainium and Stileron among them. Most parts contain at least one; anyone dismantling just for those hauled it for nothing. The figures come from the game data, not from the program |
| <img src="assets/symbole/22/blickwinkel-gruen.png" width="22" alt=""> **Field of view** | **Are you sitting right in front of your screen?** Measure your screen width with a bank card (they are standardised), enter your seating distance — and you get the field of view your setup actually delivers, with a rating |
| <img src="assets/symbole/22/laeden-gruen.png" width="22" alt=""> **Shops** | **Where a finished part sits on the shelf — and what it costs there.** The other direction to crafting: instead of "what do I need to build this", the question "is building it worth the effort at all". **1,528 parts across 38 item groups** — not just craftable ones, but missiles, bombs, ammunition and weapon attachments, plus **174 ships to buy or rent**. Every row shows **class, size, grade and manufacturer**, and you can filter by them: that is how you find the quantum drive that fits your ship without knowing all 44 names. Click a part and you get every shop with price, location and system — the cheapest one first, used stock with its condition. Anything that is not sold anywhere never shows up |
| <img src="assets/symbole/22/routen-gruen.png" width="22" alt=""> **Routes** | **Trade routes with real profit instead of a price list.** Tell it where you are, how much cargo space you have and how much money — and you get the run with buy price, sell price and what is left at the end. Optionally across several stops in a row, sorted by highest profit or shortest hop, and as a round trip back to your start. One button searches the best route **anywhere in the verse**, no starting point needed. Pick your ship and the cargo capacity fills itself in |
| <img src="assets/symbole/22/joysticks-gruen.png" width="22" alt=""> **Controls** | **What is bound to which key — and rebind without switching to the game.** Your complete binding list in plain language ("Eject" instead of `v_eject`, in the tool's own language), joystick, keyboard, mouse and gamepad in one searchable list, switchable between *changed by me*, *everything* and *default*. To **rebind**, click a row and press the button you mean — no need to know its number, and if the input is already taken you see it beforehand. Plus: which stick has which number, and whether your bindings still point at a connected device. Works for **any** device — nothing needs to be set up in advance |
| <img src="assets/symbole/22/einrichtung-gruen.png" width="22" alt=""> **Setup wizard** | Five steps on first start — and **repeatable any time**, no digging through menus |
| <img src="assets/symbole/22/sicherung-gruen.png" width="22" alt=""> **Backup** | Everything only you have, in **one file** — blueprint inventory, both stocks, mission log, watchlist and settings. The same button restores it: file onto a stick, read it in on the new machine, keep playing. The downloaded reference data stays out, the program fetches that itself |
| <img src="assets/symbole/18/punkt-blau.png" width="22" alt=""> **Catalogue watch** | Also reports when something becomes **newly craftable in the game** — a blueprint CIG added that did not exist before |
| <img src="assets/symbole/22/serverstatus-gruen.png" width="22" alt=""> **Server status** | A tab of its own: **is Star Citizen up?** Shows what CIG reports on its status page — all three systems plus the incidents of the last two months in full. Refreshes itself once a minute. States stay in CIG's own wording; the entries are maintained by hand, not measured |
| <img src="assets/symbole/18/punkt-blau.png" width="22" alt=""> **New in game** | Its own filter in the list: **only what the current patch added**. Every blueprint carries the game version it first appeared in; when the next patch lands, the new ones move in and the old ones drop out of the filter — the stamp stays |
| <img src="assets/symbole/18/gemerkt-gruen.png" width="22" alt=""> **Watchlist** | Click the star next to anything you are waiting for. When it shows up it is announced in gold — and **removed from the watchlist by itself** |
| <img src="assets/symbole/22/kuerzel-gruen.png" width="22" alt=""> **Class · size · grade** | Compact tag `class/size/grade` per blueprint, e.g. `M/1/A` (Military · Size 1 · Grade A) |
| <img src="assets/symbole/22/ton-gruen.png" width="22" alt=""> **Sound** | A short beep on every find — you don't have to watch the window |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **Always on top** | Borderless, slightly translucent overlay above the game |
| <img src="assets/symbole/22/schloss_auf-gruen.png" width="22" alt=""> **Pass clicks through** | One click on the lock in the bar and the overlay lets mouse clicks through — still in view, no longer in the way. The same lock turns green and brings it back with one click, with no detour through the settings |
| <img src="assets/symbole/22/verschieben-gruen.png" width="22" alt=""> **Movable & resizable** | Drag the title bar, resize at the ◢ handle — **position and size are remembered** |
| <img src="assets/symbole/22/sprachen-gruen.png" width="22" alt=""> **German and English** | Interface switchable; the in-game blueprint message is recognised in both languages |
| <img src="assets/symbole/22/abhaken-gruen.png" width="22" alt=""> **Tells you about updates** | Notices new versions by itself — with „What's new" to read up on, including older releases |
| <img src="assets/symbole/22/nurlesend-gruen.png" width="22" alt=""> **Read only** | Changes nothing in the game — reads `Game.log` and, if present, the launcher files |
| <img src="assets/symbole/22/eigenbuch-gruen.png" width="22" alt=""> **Own inventory** | Keeps track of which blueprints you have — without the SC Deutsch Launcher |
| <img src="assets/symbole/22/zeit-gruen.png" width="22" alt=""> **Play time** | The bar at the top shows **how long you have played** — in total, and while you are playing the current session next to it. Counted from the game's own logs, and **kept**: Star Citizen clears out its old logs, the count stays. The backup takes it along when you move machines |
| <img src="assets/symbole/22/diagnose-gruen.png" width="22" alt=""> **Report a problem in one click** | A field for one sentence, the finished report below it, a red button beside it — that is all. The report holds system, packaging, game state and the last errors, but **no names and no paths**, and you see all of it beforehand. Prefer to write it yourself? One button hands you a prepared issue |
| <img src="assets/symbole/22/zeit-gruen.png" width="22" alt=""> **Catch-up** | Reads stored logs of earlier sessions **and the running one** on start, picking up what was unlocked while it wasn't running — finds are reported, not added silently. A **Read the logs again** button (overlay and settings) goes through everything once more on demand |
| 🐧 **Windows and Linux** | One build for both systems, including autostart and log language detection |

## Requirements

- **Windows or Linux**
- **Star Citizen** installed — the folder containing `Game.log` is what's looked for. On Linux the usual Wine prefixes are searched (lug-helper, Lutris, Bottles, Heroic). If nothing is found, the wizard asks.

Nothing else. No Python, no account — and whether you install is your call (see below).

## Getting started

1. Download the file for your system from the **[releases page](../../releases)**:

   | System | File | What happens |
   |---|---|---|
   | **Windows** | `SC-BP-Watcher-Setup.exe` | Installs with a start menu entry, optional desktop icon and autostart — and uninstalls cleanly |
   | **Linux** | `SC-BP-Watcher-x86_64.AppImage` | A single file. The wizard offers an application menu entry if you want one |

2. Run it. Done.

No Python, no extra packages — the installer brings everything with it and can be removed again through *Apps & Features*.

> **Why the standalone `.exe` is gone** (as of v3.0.0): it existed for a long
> time as a second route, for anyone who did not want to install anything. That
> came at a price you only noticed later — an update put the new version
> **beside** the old file instead of replacing it. Anyone clicking their usual
> shortcut afterwards kept using the old version for months without noticing.
> With the installer that cannot happen: a start-menu entry, updates that
> genuinely replace, autostart as a checkbox, and a clean uninstall. On Linux
> the AppImage stays as it is. On Linux, make the AppImage executable once (right click → Properties → *Executable as program*, or `chmod +x SC-BP-Watcher-x86_64.AppImage`).

On first start a **wizard** walks you through setup: language, finding Star Citizen, collecting your existing blueprints. It takes a minute, and then your inventory is there.

### Code signing

This project has applied to the [SignPath Foundation](https://signpath.org/)
for free code signing for open source projects. Once approved, released
Windows binaries will be signed by SignPath, and Windows will show the
publisher name instead of "unknown publisher".

Builds are produced exclusively by a public GitHub Actions workflow — see
[SECURITY.en.md](SECURITY.en.md) for how releases are built and what the program
does and does not send.

### ⚠️ Windows says "Windows protected your PC"

This appears on the first launch, and it is **not a virus detection**:

> Microsoft Defender SmartScreen prevented an unrecognised app from starting.

**To run it anyway:** click **More info** → **Run anyway**. It will not ask again.

**Why this happens:** SmartScreen does not check whether a program is harmful — it checks whether it is **known**. A file becomes known through a purchased code-signing certificate (several hundred euros a year) or by being downloaded by very many people. A free fan tool has neither, and every new version starts from zero again.

**If you would rather not take my word for it — you don't have to:**

- The **source is open** ([here](../../)), and the file is not built by me but by **GitHub Actions** from exactly that source. The build is there to read: [`.github/workflows/release.yml`](.github/workflows/release.yml)
- Every file on the releases page carries its **SHA-256 checksum** — GitHub shows it directly
- Upload it to **[VirusTotal](https://www.virustotal.com)** if you like. Individual scanners are known to flag PyInstaller executables; that is a classic false positive

On **Linux** this message does not exist — the file just needs to be made executable once.

> ℹ️ Verified against a real Star Citizen installation, with both a **German and an English** game client. Feedback from other machines is still welcome — different install locations, different screen setups, Windows. As an [issue](../../issues).

<details>
<summary>Running from source (for the curious and for developers)</summary>

You need [Python 3.8+](https://www.python.org/downloads/) — on Windows tick **„Add Python to PATH"** during setup. No extra packages required.

```bash
git clone https://github.com/Xharig/SC-BP-Watcher.git
```

| System | Start with |
|---|---|
| Windows | `SC-BP-Watcher starten.bat` |
| Linux | `SC-BP-Watcher starten.sh` |

On Linux the `tk` package (Python's window library) is often missing. The start script tells you what it is called on your distribution — on Arch `sudo pacman -S tk`, on Debian and Ubuntu `sudo apt install python3-tk`.

The finished files are built by **GitHub** on every version tag ([`.github/workflows/release.yml`](.github/workflows/release.yml)) — nobody has to build by hand, not even the author.

</details>

## Using it

The narrow bar sits above the game and reports new finds. Everything else is behind the symbols in its title bar:

| Symbol | What it does |
|---|---|
| <img src="assets/symbole/22/glocke-grau.png" width="22" alt=""> | **Bell** — a new build is available; turns green as soon as there is one |
| <img src="assets/symbole/22/starten-grau.png" width="22" alt=""> | **Rocket** — launch Star Citizen. Only appears if a way to start it was found |
| <img src="assets/symbole/22/einstellungen-grau.png" width="22" alt=""> | **Gear** — open the settings |
| <img src="assets/symbole/22/liste-grau.png" width="22" alt=""> | **Clipboard** — blueprint list: search, filter, tick off, look up where things drop |
| <img src="assets/symbole/22/schloss_auf-grau.png" width="22" alt=""> | **Lock** — pass mouse clicks through to the game. It turns green and closed while they do; one click on it catches them again |
| <img src="assets/symbole/22/einklappen-grau.png" width="22" alt=""> | **Chevron** — fold the overlay down to just its bar |
| <img src="assets/symbole/22/leeren-grau.png" width="22" alt=""> | **Eraser** — clear the messages on screen. Your blueprints stay |
| <img src="assets/symbole/22/schliessen-grau.png" width="22" alt=""> | **Cross** — close |
| <img src="assets/symbole/22/handelslager-gruen.png" width="22" alt=""> **Cargo hold** | What you carry to sell — deliberately kept apart from your material storage: one is building material you keep, the other is cargo you want gone. Enter commodity, location and SCU; the amount field does maths (`100+5`). Instead of a quality there is a **"marked as stolen"** tick — quality does not affect selling, and looted cargo is always Q 0 anyway |
| <img src="assets/symbole/22/verkauf-gruen.png" width="22" alt=""> **Selling** | Where to offload your goods and what they pay **per SCU** — for **several commodities at once**. Sorted not by the highest price but by **how many of your goods a place takes**: 100 SCU gold, 40 copper and 25 iron pay 3,533,000 aUEC at one place, 3,566,000 spread over three — one percent more for two extra approaches. If the cargo is marked as stolen, the tab narrows down to the 15 terminals that ask no questions |

| Action | How |
|---|---|
| Move the window | Drag the bar at the top |
| Resize | Drag the **◢** handle at the bottom right |

## How it works

What the coloured dots mean:

| | |
|---|---|
| <img src="assets/symbole/18/bestaetigt-gruen.png" width="18" alt=""> | Blueprint unlocked — it's in your inventory |
| <img src="assets/symbole/18/punkt-blau.png" width="18" alt=""> | Became newly craftable **in the game** — not something *you* have yet |
| <img src="assets/symbole/18/gemerkt-gelb.png" width="18" alt=""> | Something from your watchlist has appeared |
| <img src="assets/symbole/18/hinweiszeile-grau.png" width="18" alt=""> | A note, not an unlock (e.g. a gap in your inventory) |

1. **On start** the tool goes through the stored logs of earlier sessions (`logbackups/`) and quietly adds everything it finds to your inventory — nothing is lost if you played without the watcher running. Those blueprints are **not** reported as new. If the stored logs don't reach far enough back, the watcher says so as an <img src="assets/symbole/16/hinweiszeile-grau.png" width="16" alt=""> line instead of passing off an incomplete list as complete.
2. **In the background** the **`Game.log`** is read — every 3 seconds, adjustable. *(The wording of the blueprint message depends on your game language — the watcher works it out by itself, see below.)* When the game writes `Added notification "Blueprint Received: <name>: "` on unlock, the blueprint is in the list **immediately** (<img src="assets/symbole/16/bestaetigt-gruen.png" width="16" alt="">) and in your inventory.
   - **If the SC Deutsch Launcher is installed as well**, it fills in the details (German names) and reports anything the log missed. There is no intermediate stage: what the `Game.log` says is what the game did — there is nothing to confirm.
3. Every new line is inserted at the top (name · type · `M/1/A` · time) and a short sound plays.
   - **Once a minute** the craftable catalogue is checked. If it grew, CIG made something **newly craftable** with a patch → a blue line. This has nothing to do with your own unlocks.
4. **Type, size, grade and class** come from scmdb.net's crafting data and from the bundled game data. If the SC Deutsch Launcher is present, its maintained catalogue takes precedence (German names). Above all of it are your own corrections from `bp-overrides.json`.
5. **Your inventory** grows along and stays in `bestand.json` — with a note where each blueprint came from (log, catch-up, launcher).

> **Why read the log directly?** The SC Deutsch Launcher reads the same file but only exports its own every few minutes. Measured on 2026-07-30: unlock in game **21:23:49** → launcher export **21:26:24** = **2.5 minutes** of delay. Reading it yourself gets you there in seconds — with nobody in between.

Files watched:

```text
…\StarCitizen\LIVE\Game.log                 (the game — the actual source)
…\StarCitizen\LIVE\logbackups\              (earlier sessions, read on start)
…\sc-deutsch-launcher\blueprints\           (optional: German names, fills gaps)
```

Its own files (inventory, settings, cache) live here:

| System | Folder |
|---|---|
| Windows | `%APPDATA%\sc-bp-watcher\` |
| Linux | `~/.config/sc-bp-watcher/` |

Both can be moved with the `SC_BP_HOME` environment variable.

### Game language

The blueprint message in the log is translated, and the watcher **works out by itself** how it reads in your client. It knows over 700 blueprint names; if a log line contains one of them, the text in front of it is the phrase it was looking for. That works for languages nobody planned for — French and Spanish just as well as English.

German and English are additionally built in, and you can add your own in `phrasen.json` in its own folder:

```json
{ "phrasen": ["Blueprint Received"] }
```

### Setting your own paths

If Star Citizen (or the SC Deutsch Launcher) isn't in one of the usual places, you enter the folder yourself — in `einstellungen.json` in the folder above:

```json
{
  "spiel_ordner": "D:\\Games\\StarCitizen\\LIVE",
  "launcher_ordner": ""
}
```

`spiel_ordner` is the folder containing `Game.log` (usually `LIVE`). An empty field means „search automatically". Restart the watcher after changing it.

> If the watcher can't find the game, it creates this file **by itself** on start and tells you where it is — you don't have to create it by hand. The file lists the places that were searched next to each field, as does the window. So you can see what such a path looks like on your system instead of guessing.

### Windows and Linux side by side? One folder for both

If you boot the same machine into Windows sometimes and Linux other times, you otherwise keep **two separate blueprint inventories** — without noticing. Each system reads the game logs it can see and writes into its own folder under *Documents*. You only find out months later, when blueprints are missing on one side.

**The fix is a setting, not a sync:** put the folder for your data on a disk **both systems can see**, and point both systems at the same path — under *Settings → Paths → Folder for your data*. Linux reads NTFS, so a shared data drive is enough.

After that there is only **one** inventory. Nothing is copied and nothing is synced, so nothing can drift apart or overwrite anything.

> When you switch, the watcher asks whether your existing data should come along, and checks every copied file against the original. The old folder is left completely intact — nothing is deleted.

The same works across **two machines** if both use the same cloud or network folder. ⚠️ There, only one watcher should run at a time.

### Waiting for specific items

Waiting for one particular blueprint? Click the **star** next to its name in the blueprint list. The search box finds it in seconds, and the **watching** filter shows what you're waiting for.

When a watched blueprint appears, the watcher announces it in gold with a star and its own sound — and then **removes it from the watchlist by itself**. What you have doesn't need to be on there.

## Settings

In `einstellungen.json` in its own folder — a text file, not code. Restart the watcher after changing it. The file is created on first start and explains every field itself.

| Field | Meaning | Default |
|---|---|---|
| `sprache` | Interface language: `auto`, `de` or `en` | `auto` |
| `spiel_ordner` | Where Star Citizen is (empty = search automatically) | empty |
| `launcher_ordner` | Where the SC Deutsch Launcher is (empty = search automatically) | empty |
| `pruefintervall_sekunden` | How often `Game.log` is checked — 1 to 60 allowed | `3` |
| `signalton` | Short sound on a find | `true` |

**Environment variables** — for a one-off case, without changing anything permanently:

| Variable | Effect |
|---|---|
| `SC_BP_HOME` | different folder for inventory and settings |
| `SC_INSTALL_DIR` | different game folder |
| `SC_BP_LAUNCHER` | different launcher folder |
| `SC_BP_NO_NET=1` | **no** network access — neither crafting data nor update check |
| `SC_BP_SPRACHE` | language for this run (`de` / `en`) |

## Helping to test

New versions appear **on Saturdays**. If you would rather not wait, you can get them earlier:

**Info → Update & About → "Offer test versions too"**

From then on the tool also reports test versions (recognisable by the `rc` in the number)
— through the same update notice as always. Nothing to download by hand, nothing to hunt for.

- **Test versions are fully built and runnable**, but have not been proven for long.
  Something may act up — that is exactly what they are for.
- **The way back is always open.** Switch it off again and you will be offered the next
  finished version: a finished version always counts as newer than any test version of
  the same number. So nobody gets stuck on the test channel by accident.
- **Without this setting you never see a test version.** If you want peace and quiet,
  do nothing — that is the default.

Found something? The quickest way is **Info → Report a problem**: there is a field
"What happened?" for one sentence, the finished report below it — and the red
**Send report** button sends it straight off. That is all you need to do. The report
holds everything needed to track a problem down and no personal information; you see
all of it beforehand.

Prefer to do it yourself? An [issue](../../issues) works just as well — the **GitHub
issue …** button on the same page fills the report in for you. Or the **Report a bug**
forum on [Discord](https://discord.gg/g2E7e6XxZC), if a screenshot is quicker than a description.

## Passing it on

> 🔒 **It's yours.** No account, no sign-in, no cloud. The tool reads files that are on your disk anyway and changes nothing about the game installation. It only reaches out to the network to **fetch** data — never to hand any over:
>
> | What for | How often |
> |---|---|
> | Values and origins from scmdb.net | once per game version |
> | Resource prices and storage locations from UEX Corp | at most once a day |
> | Contract texts and translation sources | when you switch them on |
> | Whether there is a new build | at start |
> | CIG's server status | while the page is open |
>
> **All of it** can be switched off with `SC_BP_NO_NET=1`. The one exception is the problem report — that only goes out when you press the button yourself, and you see the contents beforehand.

Just pass on the file from the [releases page](../../releases) — the recipient needs neither Python nor a launcher, only Star Citizen.

If you fork this project, please keep the credit in the footer or mention the original source.

> ℹ️ Windows SmartScreen reports „unknown publisher" for unsigned files → **More info → Run anyway**.

## Thanks & credits

This tool grew up with the **[SC Deutsch Launcher](https://www.sc-deutsch-launcher.de/)**: it was the only data source in the beginning, and without it this project would not exist. If it is installed, it is still used — it confirms finds and supplies German names. **Many thanks** to the team behind it! 🙏

The values for type, size, grade and class as well as the origin of each blueprint come from the **[Star Citizen Mission DataBase (scmdb.net)](https://scmdb.net)** — a hobby project that prepares the game data and makes it freely available. **Thank you** for that! 🙏

> The watcher **does not ship this data**; it fetches it on your machine directly from scmdb.net, the way a browser would. scmdb is licensed under [CC BY-NC-ND 4.0](https://creativecommons.org/licenses/by-nc-nd/4.0/); a bundled copy would be redistribution and would conflict with that licence as well as with this project's GPL. Fetching is sparing: only when a **new game version** is out.

As a base for the blueprint details you can pick **[StarStrings](https://github.com/MrKraken/StarStrings)** by **MrKraken** — cleaned-up English game text, used across many organisations. **Thanks** to MrKraken! 🙏

> StarStrings is **not bundled** either; it is fetched from its own address when you ask for it. The project states no licence — all the more reason the text stays his.

**The watcher gets along with other tools.** StarStrings and the SC Deutsch Launcher mark blueprint contracts too, with the same `[BP]` mark. So the watcher adds **no second mark where one already stands**, and leaves any item name alone that already carries a tag. With the launcher its blueprint list **replaces** the launcher's instead of sitting beside it — it is the same list, only with the **tick boxes** for your own collection. Take the details back out and the other tool's state is there again, character for character.

**The German translation of the game** is by **rjcncpt** — [StarCitizen-Deutsch-INI](https://github.com/rjcncpt/StarCitizen-Deutsch-INI), licensed under [CC BY-NC-SA 4.0](https://creativecommons.org/licenses/by-nc-sa/4.0/). It is distributed through the SC Deutsch Launcher and also exists in **Swiss German**; the watcher recognises both. The watcher does **not bundle** it and never passes on a modified copy — it only extends the file on your own machine, and the **source note in its first line is left untouched**. **Thanks** to rjcncpt! 🙏

**Resource prices** come from **[UEX Corp](https://uexcorp.space)** — a data project maintained by players. That is what puts a price next to every missing ingredient, or says it cannot be bought at all. These data are **not bundled** either; they are fetched on your machine, at most once a day. **Thanks** to UEX Corp! 🙏

**The ship slots** come from **[erkul.games](https://erkul.games)** — the loadout tool the Star Citizen community has used for years. Without that data the watcher could not answer the question that follows every new blueprint: *does this part even fit one of my ships?* These data are **not bundled** either; they are fetched on your machine — only for the ships you actually added, and only once per game patch. **Thank you** to erkul.games! 🙏

**The hangar import** is made possible by the browser extension **[Star Citizen Hangar XPLORer](https://github.com/dolkensp/HangarXPLOR)** by **dolkensp** (MIT licence): it saves your pledge store as a file the watcher reads. Without it every ship would have to be typed in by hand. **Thank you!** 🙏

The interface symbols come from the **[Lucide](https://lucide.dev)** set (ISC licence) — all drawn on the same grid with the same stroke width, which is why they look identical on Windows, Linux and macOS. **Thanks** to the Lucide community! 🙏 The licence text ships with the tool (`assets/symbole/LIZENZ.txt`) and is shown under **Thanks & Licenses**.

SC BP Watcher is an independent, unofficial companion tool with **no** official connection to the SC Deutsch Launcher or Cloud Imperium Games. All brand and project names belong to their respective owners.

## What's next

Work continues — what exactly is not on a list. What a build brought you can read in [`CHANGELOG.en.md`](CHANGELOG.en.md) or right in the tool under **„What's new"**.

**Which version is being worked on right now** is in the changelog of the working branch: [CHANGELOG on `arbeit`](https://github.com/Xharig/SC-BP-Watcher/blob/arbeit/CHANGELOG.en.md). That is where finished but unreleased work collects — if you try a [test build](https://github.com/Xharig/SC-BP-Watcher/releases), that is where you read what is in it. This page always shows the **released** version.

Wishes and bug reports are welcome as an [issue](../../issues) or on [Discord](https://discord.gg/g2E7e6XxZC) — suggestions make it into the next build more reliably than mind reading.

## Star Citizen Fan Content

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/fankit/MadeByTheCommunity_White.png">
    <img alt="Star Citizen — Made by the Community" src="assets/fankit/MadeByTheCommunity_Black.png" width="150" height="150">
  </picture>
</p>

> This is an unofficial Star Citizen fan site, not affiliated with the Cloud Imperium group of
> companies. All content on this site not authored by its host or users are property of their
> respective owners.

SC BP Watcher is an unofficial, non-commercial fan project for the *Star Citizen* community.
It is **not affiliated with, endorsed, sponsored, or approved by** Cloud Imperium Rights LLC,
Cloud Imperium Rights Ltd., or Roberts Space Industries.

This project makes use of assets from the official
[Star Citizen Fankit](https://robertsspaceindustries.com/fankit). Those materials are published
for fan use and may only be used as explained by the terms of the **Fankit Agreement**, the
**Fan Style Guide**, and the
[Roberts Space Industries Terms of Service](https://robertsspaceindustries.com/tos) —
specifically the section on User Generated Content (UGC).

> **Star Citizen®, Roberts Space Industries® and Cloud Imperium® are registered trademarks of
> Cloud Imperium Rights LLC.**

All other Star Citizen content, artwork, names, logos and trademarks are the property of their
respective owners. © 2025 Cloud Imperium Rights LLC and Cloud Imperium Rights Ltd.

Official site: **[robertsspaceindustries.com](https://robertsspaceindustries.com)**

## License

[GNU GPL v3.0](LICENSE) — free to use and modify; if you distribute it, the source has to come along under the same licence.

<div align="center">

[![Xharig](https://github.com/Xharig.png?size=80)](https://github.com/Xharig)

**Xharig** — development and design of this project

<sub>Like the tool? <a href="https://ko-fi.com/xharig">Ko-fi</a> ☕</sub>

</div>
