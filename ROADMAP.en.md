# Roadmap

[Deutsch](ROADMAP.md) · **English**

## What it is for

SC BP Watcher is a small overlay that shows, while you play Star Citizen, when a new blueprint is unlocked — on **Windows and Linux**, from a shared codebase.

Since v3.3.0 there is a **workshop** on top of that: what you can craft from a blueprint, what you are missing for it, where the materials are mined and what sits in your storage. The blueprint is still the starting point — the workshop answers the question that follows it: *and now what?*

Four things are deliberate and will stay that way:

- **Lightweight.** Plain Python standard library, no extra packages. What has no dependency cannot lose one.
- **It's yours.** No account, no sign-in, no cloud. What the tool knows sits in files on your disk.
- **It reads, it does not write.** Nothing about the game installation is changed.
- **Honest over pretty.** If something might be missing, it says so — an uncomfortable answer beats a nice number you cannot rely on.

## What it does today

| | |
|---|---|
| ✅ | Live detection of new blueprints from the game log, shown in the overlay |
| ✅ | **Its own blueprint inventory** — the SC Deutsch Launcher is not needed |
| ✅ | **Catch-up**: earlier play sessions are read on start |
| ✅ | **Blueprint list** to look up, filter and tick off, with progress |
| ✅ | **Where each blueprint drops** — faction, contract, required standing, payout; a button in Crafting leads straight there |
| ✅ | **When you accept a contract**: does it carry blueprints, and which are you missing? |
| ✅ | **What to do next** — the open objectives are listed under their contract |
| ✅ | **Mission log** — which missions were played when, how often, and which blueprint came out of it |
| ✅ | **Backup** — everything of yours into one file and back again, for moving to another PC |
| ✅ | **Keyboard shortcut** — brings the blueprint list up from inside the running game (Windows and Linux/X11) |
| ✅ | **Pin the overlay to a screen corner** — required in pop-up mode where it cannot be dragged; collapsed it shrinks to strip width |
| ✅ | Catalogue watch: reports what becomes **newly craftable** in the game, plus a watchlist |
| ✅ | **New in game** filter plus a patch dropdown: see what each patch added |
| ✅ | **Server status**: a tab of its own with CIG's live status, refreshing itself |
| ✅ | Class, size and grade tag (`M/1/A`) |
| ✅ | Setup wizard, repeatable at any time |
| ✅ | German and English, switchable |
| ✅ | Windows and Linux, with autostart on both |
| ✅ | Export your inventory — for the KRT Profit Basetool, for scmdb.net and as a full backup |
| ✅ | Collapse the overlay, for anyone on a single screen |
| ✅ | **Tray icon**: next to the clock on Windows, in the application menu on Linux — the way back to the list and the settings while the overlay stays out of sight |
| ✅ | **Item details in game** — class, size and grade at the tractor beam, seeker type for missiles |
| ✅ | **Crafting**: for every craftable item the ingredients, the duration and the stats — including whether you own the blueprint for it |
| ✅ | **Material quality matters** — one slider per ingredient shows what *your* material would yield, and the range the value can reach at all |
| ✅ | **My storage**: record material, amount, quality and location; the recipe then shows what is missing, and a button deducts the ingredients |
| ✅ | **Mining** both ways: material → where it is found, location → what is found there, with mining type, refinery comparison and scan signature |
| ✅ | **Refining method recommendation** — say whether yield, cost or speed matters most and the tool names one of the nine methods |
| ✅ | **Prices** from UEX Corp — what a material costs and what it sells for, so "what am I missing" also answers "what will it cost me" |
| ✅ | **Cargo hold**: what you carry to sell — kept apart from the workshop stock, with a marker for cargo flagged as stolen |
| ✅ | **Back up and restore both storages** (.json), export them as a spreadsheet (.csv) and clear them in one go after a patch wipe |
| ✅ | The window **keeps the size** you set |
| ✅ | **Selling**: where to offload your goods and what they pay per SCU — for several commodities at once, sorted by how many a place takes, with a signal for places that are already full |
| ✅ | **Shops**: where a finished part sits on the shelf and what it costs there — the counter-check to "is building it worth it?" |
| ✅ | **Everything for sale, not just what you can craft**: 1,528 parts across 38 item groups, plus 174 ships to buy or rent |
| ✅ | **Class, size, grade and manufacturer** on every row — and as dropdowns, so you find the right part without knowing its name |
| ✅ | **Material storage** says what it holds — matching the cargo hold next to it |
| ✅ | **Routes**: trade routes with buy price, sell price and real profit — across several stops, as a round trip, or the best route anywhere in the verse |
| ✅ | **Ship data**: cargo capacity, purchase and rental price — pick your ship in the route planner and the cargo hold fills itself in |
| ✅ | **Bindings included in backups**: keyboard and joystick bindings go into the backup file and can be saved as a named profile where Star Citizen finds it |

## What is being worked on

No schedule and no fixed order — the state of things is in [`CHANGELOG.en.md`](CHANGELOG.en.md), and the **ⓘ "What's new"** window inside the tool shows what each build brought.

What comes next follows from what people report. The tool is in daily use, and most changes started as someone's message.

## Relationship to the SC Deutsch Launcher

**Freedom of choice, not replacement.**

This page used to argue that a self-kept inventory is necessarily inaccurate because it could only be filled "from today on". Two measurements disproved that:

- The watcher reads the **stored logs** on start. Having played without it running does not tear a hole, as long as Star Citizen still has the backup. If a gap remains anyway, it is **stated** rather than hidden.
- The launcher itself counts **too low**: it is missing the P4-AR Rifle although the Fabricator lists it as owned. Starter blueprints were never "received" and appear in no log. Its number is a lower bound, not an inventory.

The launcher remains useful all the same: it confirms finds and maintains a catalogue with German names. If it is there, it is used. If it is not — always the case on Linux — the watcher works anyway.

## Getting involved

Wishes, bug reports and ideas are welcome as an [issue](../../issues).
