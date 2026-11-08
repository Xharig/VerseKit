# VerseKit: Manual

[Deutsch](MANUAL.md) · **English** · [Back to the overview](README.en.md)

This is the technical part: how VerseKit spots blueprints, which files it creates and what you can set by hand. You need none of it to use the tool; the wizard and the settings inside the program are enough.

## How VerseKit works

1. **On start** VerseKit goes through the stored logs of earlier sessions (`logbackups/`) and quietly adds everything it finds to your collection. Nothing is lost if you played without VerseKit running. Those blueprints are **not** reported as new. If the stored logs don't reach far enough back, VerseKit says so as an <img src="assets/symbole/16/hinweiszeile-grau.png" width="16" alt=""> line instead of passing off an incomplete list as complete.
2. **In the background** the **`Game.log`** is read, every 3 seconds, adjustable. *(The wording of the blueprint message depends on your game language; VerseKit works it out by itself, see below.)* When the game writes `Added notification "Blueprint Received: <name>: "` on unlock, the blueprint is in the list **immediately** (<img src="assets/symbole/16/bestaetigt-gruen.png" width="16" alt="">) and in your collection. There is no intermediate stage: what the `Game.log` says is what the game did. There is nothing to confirm.
3. Every new line is inserted at the top (name · type · `M/1/A` · time) and a short sound plays.
   - **Once a minute** the craftable catalogue is checked. If it grew, CIG made something **newly craftable** with a patch, and a blue line appears. This has nothing to do with your own unlocks. The comparison state is kept as `catalog-seen.json` in its own folder and survives restarts; on the very first start only the baseline is set.
4. **Type, size, grade and class** are read from the game data of your own installation and cross-checked against scmdb.net; where the game data cannot be read, scmdb.net steps in. Above all of it are your own corrections from `bp-overrides.json`.
5. **Your collection** grows along and stays in `bestand.json`, with a note where each blueprint came from (log, catch-up, import). That is the "which ones do I have" list.

> **Why read the log directly?** Reading it yourself gets you there in seconds, with nobody in between.

Files watched:

```text
…\StarCitizen\LIVE\Game.log                 (the game, the actual source)
…\StarCitizen\LIVE\logbackups\              (earlier sessions, read on start)
```

Its own files (collection, settings, cache) live in a folder of their own under *Documents*. Where exactly is shown under **Settings → Installation & paths**, which can also move it. For a single run the `SC_BP_HOME` environment variable does the same.

### Game language

The blueprint message in the log is translated, and VerseKit **works out by itself** how it reads in your client. It knows over 700 blueprint names; if a log line contains one of them, the text in front of it is the phrase it was looking for. That works for languages nobody planned for: French and Spanish just as well as English.

German and English are additionally built in, and you can add your own in `phrasen.json` in its own folder:

```json
{ "phrasen": ["Blueprint Received"] }
```

### Setting your own paths

If Star Citizen isn't in one of the usual places, you enter the folder yourself, in `einstellungen.json` in its own folder:

```json
{
  "spiel_ordner": "D:\\Games\\StarCitizen\\LIVE"
}
```

`spiel_ordner` is the folder containing `Game.log` (usually `LIVE`). An empty field means "search automatically". Restart VerseKit after changing it.

> If VerseKit can't find the game, it creates this file **by itself** on start and tells you where it is. You don't have to create it by hand. The file lists the places that were searched next to each field, as does the window. So you can see what such a path looks like on your system instead of guessing.

### Windows and Linux side by side? One folder for both

If you boot the same machine into Windows sometimes and Linux other times, you otherwise keep **two separate blueprint collections** without noticing. Each system reads the game logs it can see and writes into its own folder under *Documents*. You only find out months later, when blueprints are missing on one side.

**The fix is a setting, not a sync:** put the folder for your data on a disk **both systems can see**, and point both systems at the same path, under *Settings → Installation & paths → Folder for your data*. Linux reads NTFS, so a shared data drive is enough.

After that there is only **one** collection. Nothing is copied and nothing is synced, so nothing can drift apart or overwrite anything.

> When you switch, VerseKit asks whether your existing data should come along, and checks every copied file against the original. The old folder is left completely intact, nothing is deleted.

The same works across **two machines** if both use the same cloud or network folder. ⚠️ There, only one VerseKit should run at a time.

### On Linux

**Where the game is looked for.** In these Wine prefixes, each below `drive_c/Program Files`, `drive_c/Program Files (x86)` and `drive_c`:

| From | Folder |
|---|---|
| lug-helper | `~/Games/star-citizen`, `~/Games/star-citizen-live` |
| Wine | `~/.wine` |
| Lutris | `~/.local/share/lutris/prefixes/*`, as a Flatpak `~/.var/app/net.lutris.Lutris/data/lutris/prefixes/*` |
| Bottles | `~/.local/share/bottles/bottles/*`, as a Flatpak `~/.var/app/com.usebottles.bottles/data/bottles/bottles/*` |
| Own folders | `~/Games/*` |
| Heroic | `~/Games/Heroic/Prefixes/*`, with Proton `…/pfx` |
| Steam with Proton | `~/.local/share/Steam/steamapps/compatdata/*/pfx`, as a Flatpak `~/.var/app/com.valvesoftware.Steam/data/Steam/steamapps/compatdata/*/pfx` |

If your prefix lies elsewhere, enter the folder with the `Game.log` in the assistant or in `spiel_ordner` (see above).

**Application menu.** *Settings → General → Entry in the application menu* writes `~/.local/share/applications/versekit.desktop` (or below `$XDG_DATA_HOME`) and the icon next to it in `icons/hicolor/256x256/apps/versekit.png`. It starts the AppImage file itself, not the unpacked temporary folder. An update brings name and icon of an existing entry along; a deleted one is not made again. The entry of the previous version (4.x) is taken over.

**Start at login** writes `~/.config/autostart/versekit.desktop` (or below `$XDG_CONFIG_HOME`). If you switch the entry off in the settings of your desktop, it counts as off.

**Sound and system message.** The signal sound plays through `canberra-gtk-play`, `paplay` or `pw-play` with the freedesktop sounds, whichever is there; without any of them it stays silent. The system message needs `notify-send` (package `libnotify`); if it is missing, the setting says "not available here".

**Shortcut.** On X11 it works everywhere. On Wayland no program can claim a key for the whole desktop; VerseKit runs through XWayland there, and the key works while the game is in front. To have it work everywhere, *Settings → Overlay* shows a ready command below the shortcut that you put on a shortcut of your own in the system (KDE Plasma: *System Settings → Keyboard → Shortcuts → Add New → Command or Script*). If VerseKit is already running, the command opens the main window instead of starting VerseKit a second time.

**Windows only:** the icon in the tray next to the clock, the signature and the refinery scanner, and Anti-AFK. The settings say so where it matters.

### Waiting for specific items

Waiting for one particular blueprint? Click the **star** next to its name in the blueprint list. The search box finds it in seconds, and the **watchlist** filter shows what you're waiting for.

When a watched blueprint appears, VerseKit announces it in gold with a star and its own sound and **ticks it off on the watchlist**. It stays there so the "Watchlist only" progress counts it; the star takes it off again.

<details>
<summary>For advanced users: patterns instead of names</summary>

Sometimes you wait for something whose exact name does not exist yet, such as "any helmet for the heavy armour". For that, `watchlist.json` in its own folder knows **patterns** as well as the names you clicked:

```json
{
  "namen": ["Attrition-5 Repeater"],
  "eintraege": [
    { "titel": "Helmet for the heavy armour", "muster": ["manticore helmet"] },
    { "titel": "Any cooler", "muster": ["cooler"] }
  ]
}
```

A pattern entry has a **title** of your choice (it shows up later in the message) and any number of **patterns**, which are checked in lower case as a part of every new catalogue entry. So `cooler` hits every cooler, `manticore helmet` only that one.

You don't need this by hand: for one particular blueprint the star in the list is enough.

</details>

## Settings

In `einstellungen.json` in its own folder, a text file, not code. Restart VerseKit after changing it. The file is created on first start and explains every field itself.

| Field | Meaning | Default |
|---|---|---|
| `sprache` | Interface language: `auto`, `de` or `en` | `auto` |
| `spiel_ordner` | Where Star Citizen is (empty = search automatically) | empty |
| `pruefintervall_sekunden` | How often `Game.log` is checked, 1 to 60 allowed | `3` |
| `signalton` | Short sound on a find | `true` |

> VerseKit remembers the position and size of the overlay when you move it and when it closes (`watcher.json` in the same folder). Just drag it where you want it. The program deliberately sets **no** fixed starting position: where an overlay sits well depends on the monitor setup. Delete the file to reset it.

> **Your own corrections:** If the class, size or grade of a blueprint is wrong, you can override it in `bp-overrides.json` in its own folder. It takes precedence over every other source. If the file lives elsewhere, give its path in the `SC_BP_OVERRIDES` environment variable.

**Environment variables**, for a one-off case, without changing anything permanently:

| Variable | Effect |
|---|---|
| `SC_BP_HOME` | different folder for collection and settings |
| `SC_INSTALL_DIR` | different game folder |
| `SC_BP_NO_NET=1` | **no** network access, neither game data nor update check nor messages |
| `SC_BP_SPRACHE` | language for this run (`de` / `en`) |
