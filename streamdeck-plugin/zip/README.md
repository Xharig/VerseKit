# VerseKit for Stream Deck: profiles

Deutsch: LIESMICH.md

### What you need to know
Profiles for VerseKit 5.7.0.

- **Game keys work right away.** Every key sends the combination Star Citizen uses by default. Nothing to set up.
- **Some combinations you must enter in the game yourself**, because the game has no default key for them (list below). Without that, pressing the key does nothing.
- **VerseKit keys F13 to F24** (overlay, read refinery, handle search, game picture, main window, capture blueprint, scan my stock, scan unit stock, anti-AFK, mining scanner, clear overlay, teach signature) work once you set the hotkeys in VerseKit (table below). Clear overlay (F23) and Teach signature (F24) have no hotkey by default: set them in VerseKit under Settings › Keys & Stream Deck. Teach signature only exists on Windows (VerseKit has to read the screen for it).
- **Page keys in the middle of the start page** (My hangar, My inventory, Mining, Blueprint list, Contested Zones and others) open the pages in VerseKit. They need the **VerseKit plugin**, which VerseKit installs itself ("Set up Stream Deck" button on "Keys & Stream Deck"). Contested Zones also shows the Executive Hangar cycle. "aUEC" stays dimmed and arrives with 5.8.
- **Overlay, Main window on/off, Anti-AFK and Mining Scanner** (start page, Mining Scanner also on Mining) are plugin keys too and show their state (ON/OFF, OPEN/CLOSED). Without the plugin, or when VerseKit is not running, they show a plug icon and do nothing.
- On the Info page, "VerseKit website" and "GitHub" open the website and the GitHub project in the browser, "Open guide" opens the Stream Deck page of the VerseKit website with this guide and "FAQ" its section with the frequent questions.

### Which file for which device?
Every file comes in German (German keyboard layout, German game texts) and English (US keyboard layout, English game texts). Take the language of your game.

| Device | File |
|---|---|
| Stream Deck XL (8 × 4 keys) | `VerseKit-StreamDeck-XL-EN.streamDeckProfile` / `VerseKit-StreamDeck-XL-DE.streamDeckProfile` |
| Stream Deck Mobile on iPad or iPhone (8 × 4 keys) | `VerseKit-StreamDeck-Mobile-EN.streamDeckProfile` / `VerseKit-StreamDeck-Mobile-DE.streamDeckProfile` |
| Stream Deck MK.2 and standard (15 keys) | `VerseKit-StreamDeck-MK2-EN.streamDeckProfile` / `VerseKit-StreamDeck-MK2-DE.streamDeckProfile` |
| Stream Deck Neo (8 keys) | `VerseKit-StreamDeck-Neo-EN.streamDeckProfile` / `VerseKit-StreamDeck-Neo-DE.streamDeckProfile` |
| Stream Deck Mini (6 keys, only the most important keys) | `VerseKit-StreamDeck-Mini-EN.streamDeckProfile` / `VerseKit-StreamDeck-Mini-DE.streamDeckProfile` |
| Stream Deck + XL (9 × 4 keys and 6 dials) | `VerseKit-StreamDeck-PlusXL-EN.streamDeckProfile` / `VerseKit-StreamDeck-PlusXL-DE.streamDeckProfile` |

The Stream Deck + XL is a different device from the Stream Deck XL. Its profile has exactly the XL keys in columns 1 to 8, plus a ninth column and the six dials (section "Stream Deck + XL" below). Not yet tried on the device.

**What is where (in the ZIP)?**
- `Profile/English/<device>/` and `Profile/Deutsch/<device>/`: the profile file for your device.
- `LIZENZEN.md`: origin and licence of the icons and the font.

### Set up
1. **Import the profile:** double-click the file; the Stream Deck software opens and adds the profile. If nothing happens, open the gear icon › "Profiles" tab in the Stream Deck software and import it there.
2. **Install the plugin:** VerseKit installs it itself ("Set up Stream Deck" button on "Keys & Stream Deck"). Then the page keys, Contested Zones and the state of overlay, main window and anti-AFK work.
3. **Set the hotkeys in VerseKit:** VerseKit › Settings › Overlay › Hotkeys, without Ctrl, Alt or Shift. Overlay (F13), main window (F17), anti-AFK (F21) and mining scanner (F22) are plugin keys in the profile; you only need their hotkeys for the dials of the Stream Deck + XL or for your own keys:

| Function | Hotkey |
|---|---|
| Show and hide overlay | F13 |
| Read refinery | F14 |
| Handle search | F15 |
| Game picture | F16 |
| Main window on/off | F17 |
| Capture blueprint | F18 |
| Scan my stock | F19 |
| Scan unit stock | F20 |
| Anti-AFK on/off | F21 |
| Mining scanner on/off | F22 |
| Clear overlay | F23 |
| Teach signature | F24 |

4. **Enter the keys in the game** that have no default key (next section).
5. **Set "Back" (XL, + XL and Mobile only):** "Back" bottom left on the seven main pages is meant to leave the VerseKit profile and switch to your own profile. Its target is empty because we do not know your profile: click the key in the Stream Deck software and pick your own profile under "Profile".
6. **Open with the game:** the German profiles for XL, + XL and Mobile are linked to the default install location `C:\Program Files\Roberts Space Industries\StarCitizen\LIVE\Bin64\StarCitizen.exe` and open with the game. All others are not linked. If your game is installed elsewhere, or you want another profile, set it in the Stream Deck software under gear icon › "Profiles" › "Application" dropdown. Only one profile should be linked to `StarCitizen.exe`.
   If the deck jumps to another profile whenever you change windows outside the game, one of your own profiles is set as "Default" for all programs. In the game VerseKit stays (the German 8 × 4 profile is linked to `StarCitizen.exe`). Fix: remove "Default" from your own profile, or pick VerseKit by hand.

### Keys you must enter in the game yourself
Search the function under Options › Keybindings and enter the combination:

| Game name | Enter |
|---|---|
| Switch Mining Laser (Toggle) | Left Alt+U |
| Toggle Guns Operator Mode | Right Alt+F |
| Toggle Missile Operator Mode | Right Alt+M |
| Lock Selected Target | Right Alt+T |
| Open/Close Doors (Toggle) | Left Alt+O |
| Lock/Unlock Doors (Toggle) | Left Alt+V |
| Toggle Quantum Operator Mode | Right Alt+Q |
| Jump Drive - Request Jump | Right Alt+J |
| Staggered Fire - Toggle On / Off | Right Alt+B |
| Turret E.S.P. Toggle On / Off | Right Alt+E |
| Turret - Speed Limiter - On/Off (Hold/Toggle) | Right Alt+V |
| Suppress Aim Assists (Hold) | Right Alt+X |

**Emotes** (Options › Keybindings › Social - Emotes): only Yes, No and Stop have a default key (Numpad 4, 6, 2). Enter all others yourself:

| Emote | Enter |
|---|---|
| Wave / Salute / Greet / Bow | Left Ctrl+F1 / F2 / F3 / F4 |
| Attention / At Ease | Left Ctrl+F5 / F6 |
| Come / Wait | Left Ctrl+F7 / F8 |
| Agree / Disagree / Point | Left Ctrl+F9 / F10 / F11 |
| Sit | Left Ctrl+F12 |
| Sleep / Laugh / Cheer / Clap | Left Shift+F1 / F2 / F3 / F4 |
| Dance / Cry / Angry | Left Shift+F5 / F6 / F7 |
| Gasp / Bored / Whistle | Left Shift+F10 / F11 / F12 |
| Flex | Left Alt+F12 |

Bored and Flex are only on the MK.2 and Neo; the Mini has eight emotes.

### Stream Deck + XL
Columns 1 to 8 are the same keys as on the XL and the iPad. New are the ninth column and the six dials (turn left, turn right, press; each dial's strip says what it does).

Ninth column: **Mining** Increase / Decrease Mining Laser Power, Increase / Decrease Scanning Angle; **Salvage** Increase / Decrease Beam Spacing, Focus Disintegration tool, Cycle Focused Salvage Modifiers; **Dogfight** Weapons, Shields, Engines - Decrease, Reset Assignments; **On Foot** Reload Secondary Fire, FPS Underbarrel Attachment Action; **Vehicle** Overlay, Game picture, Main window. Free places carry a placeholder.

These functions have no default key; for the dials and the ninth column enter them in the game:

| Game name | Enter |
|---|---|
| Increase / Decrease Mining Laser Power | Left Alt+5 / Left Alt+6 |
| Increase / Decrease Scanning Angle | Left Alt+7 / Left Alt+8 |
| Increase / Decrease Beam Spacing | Right Alt+1 / Right Alt+2 |
| Focus Disintegration tool | Right Alt+3 |
| Cycle Focused Salvage Modifiers | Right Alt+4 |
| Cycle Lock - Hostiles - Back | Right Alt+5 |

Not yet tried on the device: whether the dials send left and right the right way round and whether pressing the dial arrives. Please report if something is swapped.

### Where am I?
In the bottom bar the key of the current page has a filled bar. On sub-pages (Emotes, Turret, Info) "Back" is named after its target ("‹ On Foot", "‹ Flight", "‹ Start"). On Mini and Neo the Pages key shows page name and part, on the MK.2 the name plate does. The white "Info" key on the start page opens the Info page with the colour legend and the profile version.

### Frequently asked questions

#### A key does nothing in the game
The Stream Deck only sends the key combination. Check in this order: is the Stream Deck software running? Does the function have the same combination in the game under Options › Keybindings as the key? Is it in the list of keys you must enter in the game yourself? Did you change a combination in the game without changing the key in the Stream Deck?

#### A key shows a plug icon
The key needs the VerseKit plugin and VerseKit running. Start VerseKit. If the plug stays, click "Set up Stream Deck" once more in VerseKit on "Keys & Stream Deck".

#### "Exit seat" presses Z instead of Y
Star Citizen names keys after the US keyboard. The German profile sends Z for the key the game calls Y, because that is where it sits on a German keyboard. If you play with a US keyboard, take the English profile.

#### The iPad shows old key images
Switch the profile once and back. If that does not help, close and reopen the Stream Deck app on the iPad and check that the computer is on the same network.

#### Why do weapons not fire from the deck?
A Stream Deck key sends keyboard combinations, not mouse clicks. Firing sits on the mouse in the game and is therefore missing from the profile.

#### Can I change keys myself?
Yes, in the Stream Deck software on the computer. Always change a combination the same way in the Stream Deck and in the game, otherwise the key does nothing. Export your changed profile (gear icon › Profiles › right-click › Export), then you can get it back at any time after a new import.

#### Does it work on Mac or Linux?
The profiles and the plugin are built for Windows and tried there.

### Version history

#### 5.7.0
- First version: profiles for Stream Deck Mini, Neo, MK.2, XL, + XL and Mobile, each in English and German.
- The VerseKit plugin opens pages in VerseKit, shows Contested Zones and the state of overlay, main window, anti-AFK and mining scanner.

### Feedback
Please report bugs, missing keys or wishes in the VerseKit Discord, ideally with device, profile language and a picture.
