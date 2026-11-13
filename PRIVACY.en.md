# Privacy

[Deutsch](PRIVACY.md) · **English**

VerseKit runs on your computer. There is no account, no advertising and no
tracking. Once a day the program reports that it is running and which pages
were used, without any ID, on by default and switchable off under Settings › General › Privacy (see *Count usage*). This page states in full
what the program reads, what it writes, what it fetches from the internet and what it sends.
The same details, with links to the source code, are on
[versekit.xharig.com/en/deine-daten](https://versekit.xharig.com/en/deine-daten/).

## What is read and written on your computer

### Game folder

VerseKit reads Star Citizen's log files (`Game.log` and `logbackups/`), your
control files and the game's text file. From them come your blueprint list,
your contract log and your statistics.

The logs also contain your RSI handle and your RSI account number. VerseKit
remembers both in your settings (`eigener_account`, `eigene_account_kennung`)
and per log file (`konten-je-log.json`). It uses them to count only the
blueprints of your own account and to assign the statistics to your account.
The handle also prefills the reporter field of the problem report, and with
the Basetool it serves the account check and your own entry in unit storage.

VerseKit also **writes** into the game folder, but only files you could also
edit by hand: the game's text file (`global.ini`, for translation and
blueprint details), the controls (`actionmaps.xml`), the settings
(`attributes.xml`), the graphics settings (`GraphicsSettings.json`) and
`user.cfg`. It puts a copy next to the file first
(`<file>.versekit-<time>`), and it writes nothing while the game runs. Once you
have set up the blueprint details, VerseKit puts them back by itself after a
patch. It clears the game's shader cache only on your click. VerseKit does not
access the memory of the running game.

### Your data folder

Out of the box the folder `Verse-Kit` in your documents; you change the place
under Settings › Installation & paths. If your documents are in OneDrive,
iCloud or another cloud, the folder is there too. It holds everything VerseKit
knows about you: blueprint collection, watchlist, settings, contract log,
statistics, play time, inventory, hangar, relations, error log and the scanner
pictures. The backup zip (*Settings › Backup & reset*) takes all of that along.

Outside the data folder VerseKit only keeps: a small pointer to the data
folder's place and lock files in your user profile, the update files, the
Basetool credentials (see [Security](SECURITY.en.md)) and, only if you switch
it on, the autostart entry. On Linux, at the push of a button, a start menu
entry.

### Keyboard

On Windows, Windows reports every key press to VerseKit (Raw Input). Shortcuts
cannot work in the game any other way, because Star Citizen switches off the
ordinary shortcuts of other programs. This runs as long as the overlay runs,
also without anti-AFK. VerseKit only compares each press with its own
shortcuts (Alt+Shift with V, R, B, H or F out of the box) and keeps in memory
only which keys are held right now and when the last input came. What you type
is neither stored nor sent, and nothing is intercepted: the game gets the same
keys. On Linux VerseKit only reserves its own shortcuts (V, H and F) and does
not see other keys.

### Screen

Windows only, and only while Star Citizen is the window in front; if something
else is in front, VerseKit takes no picture. If another window lies on top of
the game, it is in the picture too. On Linux VerseKit does not read the screen.

| What | When | What it reads | What stays |
|---|---|---|---|
| Signature scanner | only when switched on, off by default | twice a second the area in the middle of the game picture, after a find only around the number | only the crop around the number: taught ones up to 200 (`signatur-bilder`), uncertain ones up to 12 (`signatur-gemerkt`) |
| Frame rate from the game | only when switched on, off by default | the top right corner every three seconds | nothing |
| Refinery (Alt+Shift+R) | on your key press | the whole game window once | only after an uncertain reading the table, otherwise the game picture made smaller (at most 2560 pixels wide), the last three (`raffinerie-bilder`); plus the number cells until My Inventory takes them over (`raffinerie-lesung.json`) |
| Game picture for the signature window (Alt+Shift+B) | on your key press | the whole game window once | one file, at most 1280 pixels, the next picture replaces it (`Intern/signatur-spielbild.bin`) |

Whole game pictures can show your handle and your aUEC balance. All pictures
lie in the data folder and go into the backup zip. They are only sent in a
problem report, and only with the tick for scan images (see *Problem report*).

### Anti-AFK

Only while you switch on anti-AFK (off by default, Windows only) the program also
sees which program is in the foreground, when keyboard and mouse were last used
and whether anything moved on a joystick, HOTAS or gamepad (only *whether*, not
what). The switch is never stored: after every start anti-AFK is off.

For the main window a working file (`Intern/anti-afk.json`) holds the state,
plus the time and kind of the last input (keyboard, mouse or joystick), never
which key. It is reset to "off" at every start and is not part of the backup.
If you send a problem report while anti-AFK is on, it contains the seconds
since the last input and its kind, the key for "wipe visor" and the number of
sticks. Nothing of it is sent otherwise.

## What is fetched from the internet

Only reads of public data. All that goes along from you is what a search
needs: for the handle search the handle or org name you look for, for prices
the number of the commodity or terminal. As with any request, the other side
sees your IP address. All requests use https.

| For | From | When |
|---|---|---|
| New versions and test versions | GitHub (api.github.com, github.com) | at start and every ten minutes |
| Build provenance of a downloaded version (see [Security](SECURITY.en.md)) | api.github.com, the provenance bundle from GitHub's storage at Microsoft Azure (blob.core.windows.net) | after every update download |
| Crafting, catalogue, contract and mining data | first the public mirror of the scmdb data on GitHub (raw.githubusercontent.com/KrovaxCode/SCMDB_DATA), otherwise scmdb.net | by itself |
| Prices, shops, places, list of items and goods | UEX (api.uexcorp.uk) | by itself and on the pages |
| Server status | status.robertsspaceindustries.com | on its page and for the start page tile |
| Ship values | cdn.erkul.games | when you open the hangar or a blueprint without data |
| Handle search: profile, organizations and pictures of the handle or org you search for | robertsspaceindustries.com | only when you search |
| Cycle of the Executive Hangar (Contested Zones) | contestedzonetimers.com | on its page, at most every six hours |
| Translations you choose | the respective source (such as StarStrings, Luftwerft, Dymerz) or the address you enter yourself | when you choose it, then refreshed by itself |
| Shared relations list ("Load list" in the handle search) | the address you enter yourself | only on your click |

Every request carries the identifier `VerseKit/<version> (+https://versekit.xharig.com)`;
the usage count sends `VerseKit/<version>`, the problem report `Verse-Kit`.

VerseKit loads a shared relations list only when you click "Load list", and
only over https. All that goes there is the usual identifier. The list joins
your relations in your data folder (`beziehungen.json`), and your own marks
stay as they are. VerseKit keeps the address in your settings
(`beziehungen_liste_adresse`).

VerseKit fetches and installs new versions by itself, at the earliest two
minutes after they appear and never while Star Citizen runs. There is no switch
for it.

With the environment variable `SC_BP_NO_NET=1` (any value except empty and
`0`) VerseKit fetches and sends nothing at all.

## Count usage

Once a day VerseKit sends exactly these details to `nutzung-versekit.xharig.com`:

| Detail | Example |
|---|---|
| Program version | `5.2.10` |
| System | Windows or Linux |
| Language of the interface and of the game | `de`, `en` |
| whether you are offered test versions | yes / no |
| which areas are switched on | Ships, Workshop, Trade, Basetool … |
| how the overlay runs | always visible / only on new blueprints |
| whether VerseKit starts with your computer | yes / no |
| the update switch from settings of older versions (no effect in VerseKit 5) | yes / no |
| which view the main window has (always the same in VerseKit 5) | tabs |
| which tiles of the start page you have hidden | Play time, Computer … |
| which pages of the main window you opened, how often | Blueprint list 12×, Shops 3× |
| how a page was opened | sidebar, jump, overlay, program start, menu filter |
| how many clicks it took to reach a page | Shops: 5× after 2 clicks |
| which page you left again after a few seconds, and where you stayed next | Selling → Shops: 7× |
| how often you added to My Inventory by hand, from a refinery yield or with the refinery scanner | added by hand: 2× |
| how often you opened the "What's new" gallery and opened a page from it | gallery opened: 1× |
| how often you used the menu filter and how often it found nothing, never the word searched for | used: 4×, no match: 1× |
| the age of the installation in whole days, without an ID (from 31 days always 31) | 3 days |

The age is counted on its own, not together with version, system or country.
This shows how many people are new and how many are still there after a week,
but not who. Only the number of days is sent, never the date. VerseKit keeps
the date of the installation once, as a day without a time, in your settings
(`installiert_am`). For an installation that already existed it is the oldest
day your data folder proves, otherwise the day of the first start.

The page and action details are only counters since the last report, using
fixed IDs: no content, no amounts, no resources, no search terms, no names, no
times of day. They stay
in your data folder (`seitennutzung.json`) until the next report and are
cleared afterwards. Where the report is not sent (see below), nothing is counted either.

It is sent at start; after that VerseKit looks every hour whether the day's
report has arrived. It only remembers the day once the receiver has accepted
it (`nutzung_zuletzt`).

Plus the **country** that Cloudflare detects on every request by itself, only
the country code, no city, no region. No ID, no name, no RSI handle, no paths.
All that is stored there are counters per day: version, system and country
together, every other detail on its own, the age apart from everything else.
Details that are not on the list are refused by the receiver. Your IP address
only serves the rate limit for a moment and is not stored.
This counts how many people use VerseKit on a given day and which
parts, but not who, and no single installation can be followed across days.
Only the developer sees the numbers.

The report is on by default. Under Settings › General › Privacy you switch off
"Count usage" (key `nutzung_melden`); then nothing is sent and nothing more is
counted, and VerseKit deletes the counters so far at the next counting attempt.
`SC_BP_NO_NET=1` stops it as well.

## Downloads from the website

The download buttons on versekit.xharig.com lead through `xharig.com/windows`
and `xharig.com/linux` to the file on GitHub. On that path we count: the day,
the system and the **country** that Cloudflare detects by itself. If the link
carries a channel tag (such as `xharig.com/windows?ref=discord`, to show which
post brought downloads), that is counted per day as well, on its own and not
together with system or country. No IP address, no city, no ID, no cookie.
Downloading directly from GitHub is not counted here.

The same tag applies to the website itself: if you open it with a tag (such as
`versekit.xharig.com/?ref=discord`), the page reports that once to
`nutzung-versekit.xharig.com`, and only the tag is counted per day. Again no IP
address (it only serves the rate limit for a moment and is not stored), no
country, no ID, no cookie. Without a tag on the link the page sends nothing.

## Problem report

The report is **only sent when you tick "I want to send the report to the
developer" and press "Send"**, after you have seen its full text. Exactly the
text shown is sent; if the view is out of date, nothing is sent. After a crash
VerseKit says so once at the next start and opens *Report a problem* if you
want; nothing goes out by itself.

It goes to `bericht.xharig.com`, a relay into the project's report channel on
Discord that keeps nothing itself. Replaced beforehand are: your user name,
your home and documents folder in every path, the computer name, IP addresses,
mail addresses, handles and player numbers from the logs, and anything that
looks like a credential. Paths outside your home folder (such as a game folder
on `D:\`) stay as they are; you see them before sending.

At the top the report says **who is reporting**, so follow-up questions reach
you. As long as you enter nothing of your own there, the field is pre-filled
with your game account, if one is chosen on the *Detection* page. You see it
before sending and can change or clear it; cleared, no name goes out. The
same name also labels the zip with the images and its folder. Otherwise it
appears nowhere in the report.

Instead of sending, you can also copy the report or have the **GitHub issue …**
button put it into a form. An issue is **public**, including the name at the
top, if you leave it in.

Images you attach yourself (up to four) go along; their metadata is removed
first. The scan images only go along if you set a tick of their own for them,
and it counts for this one report only: for signatures the taught crops around
the number (up to 200) and your own digit files, for the refinery scanner the
terminal's table from a reading that was not reliable (the last three). Only if
the table was not found at all is it the game picture, scaled down; it can show
your handle and your aUEC balance. The game picture for the signature window
and the uncertain signature crops (`signatur-gemerkt`) never go along.

## KRT Profit Basetool

Only if you explicitly connect VerseKit **and** turn syncing on. Both are off
by default.

**What goes there**, depending on the areas you turn on: your blueprints
(name, the Basetool's key, time, and how VerseKit knows them), your stock lots
from My Inventory (materials, goods for sale and items; material, place, quality, amount,
"stolen"; when you put a lot into unit storage or take it back, also the
chosen unit and the amount) and your ships (type, insurance, own name, "ready" and the unit a
ship belongs to). Plus the name you give this
installation, never your computer's name, and, for the account check only,
your RSI handle. Never purchase data (price, purchase date, package). Other
members' data only when you book in the **unit storage** as a logistician:
then the booking itself, that is which entry, amount, location, note, reservation,
sale proceeds and the owner as a pseudonym the Basetool issues for VerseKit
alone. Before booking in, VerseKit looks the goods up at the Basetool: for
that the name or the UEX number of the goods goes there, even if nothing is
booked afterwards.
With the **Sync refinery orders** area, the refinery jobs you recorded in VerseKit
go there: the refinery, the end time, per good the raw material, the yield in
cSCU and the quality, plus the chosen unit and a random key VerseKit gives the
order. The game does not show the input amount, so it goes as "unknown". When
the end time changes, the new one goes there; when you book the yield into My
Inventory, only the note "picked up", without amounts (the stock itself goes
through the stock sync). To match them, VerseKit looks the names of the raw
materials up at the Basetool.

**What comes back:** your own blueprints, stock lots and ships from the
Basetool, per stock lot how much of it is personal and how much sits in which
unit's storage, and for hangar and storage the list of units you are a member
of (kept in memory only, for the choice on the ship card and when sharing). With the *unit storage* area
the shared storage of your units (as a logistician including owners and
notes), and with the demand, as a logistician, your unit's orders with
requester and comment. Both in memory only, for at most 7 days, never in a
file or backup. With the refinery orders, your open refinery orders, including
the ones you created in the Basetool, with everything the Basetool sends for
them (place, method, times, status, goods and, if entered there, costs and
proceeds). VerseKit takes over orders created in the Basetool so the overlay
reports them.

**Cartel Bank:** Only with its own switch *Show the Cartel Bank*. VerseKit reads the
accounts you may view in the Basetool (name, number, kind, unit, balance,
target balance, change and curve of the last 30 days, whether and up to which
limit you may raise requests there) and your own requests with status,
reviewer and reason for a rejection. When you open an account, its last 100
bookings (with the counter account and, if recorded and not hidden, the name
of the other party) and its balance of the last 90 days come with it. Only
with the second switch *Queue as bank staff*, and only if you are bank staff in the
Basetool, the queue comes too: open requests and requests to approve from
other members with amount, accounts, justification, note and the requester's
name, unless they hide it. Only what you send yourself on the Cartel Bank page
goes there: a new request (kind, account, target account, amount, note,
justification), a correction of your pending request (amount, note,
justification, target account) or withdrawing it. Confirming, rejecting and
approving never go through VerseKit, only in the Basetool. **Everything from
the Cartel Bank stays in memory only**, for at most 7 days, never in a file, a
backup or a problem report; the error log holds at most the error code,
without account or request. The overlay gets the balances and the number of
pending requests straight from the main window, without a file. Only the two
switches are stored, in your settings.

**What is stored:** the synced state in your data folder
(`basetool-<id>.json`, next to it the previous version
`basetool-<id>.bak.json`), the credentials only in your system's key store
(see [Security](SECURITY.en.md)). Data from the Basetool stays on your
computer and is not passed on to anyone.

This state file holds **your RSI handle** (from the account check) and the
**list of your synced blueprints**, stock lots and ships, and with the
refinery orders your open refinery orders as the Basetool sends them. In your
refinery jobs (`raffinerie-auftraege.json`) VerseKit keeps per job the key or
the id of the Basetool order; jobs taken over sit there with material, yield,
place and end time like your own. The state file sits in the data
folder and therefore goes into the **backup zip** (*Settings → Backup &
reset*). If your data folder is in a cloud (OneDrive, iCloud …) or on a shared
drive, the file is there too. The credentials themselves are never in the data
folder and never in the backup.

**Disconnect** revokes the connection at the Basetool and deletes the
credentials (refresh token and this installation's key) and the synced state:
`basetool-<id>.json` **and** `basetool-<id>.bak.json`, and empties the Cartel
Bank in memory. Your refinery jobs keep their keys until they drop off the list
like any job. Whatever is already in a backup zip stays there until you delete
it.

**Problem report:** It can contain the names of blueprints VerseKit cannot
match, including ones that came from the Basetool. The credentials are never
in it, your RSI handle only at the top as the reporter unless you change or
clear that field (see [Problem report](#problem-report)), and it only goes out when you send it yourself. What the Basetool itself does with your data
is governed by its own privacy policy at
[profit-base.online](https://profit-base.online).

## Hangar-Exporter

So that the **Hangar-Exporter** (a browser add-on by Drachennarr) can send your
hangar straight to VerseKit, VerseKit accepts deliveries on a small port **on
your own computer only** (`127.0.0.1`; the hangar page shows the port next to
the pairing code). This exists only with your own data, not in the preview. It
is on by default. You switch it off under *My hangar → Manage pairing*; the
port then stays open but refuses every delivery. VerseKit itself sends nothing
over the internet for it.

**Who may deliver:** only whoever knows the **pairing code**. You paste it into
the add-on once. It stays the same until you renew it; after that the old one
no longer works. The code is kept in your settings file in the data folder and
never appears in a message, a log or an answer from VerseKit, in the problem
report only as dots. With the settings file it goes into the backup zip; when
a backup is restored, VerseKit does not take it over, the code this
installation already has stays in force.

**What arrives:** the ship list of your hangar, as the add-on reads it on the
RSI site. VerseKit takes it into your hangar (`hangar.json`) in the data folder
like an imported file, and it stays on your computer. If a delivery removes a
ship, VerseKit first writes `hangar.bak.json`. What the add-on itself does is
governed by its own privacy policy in the
[Hangar-Exporter repository](https://github.com/Drachennarr/Hangar-Exporter-Releases).

## scmdb.net

Only if you turn on *Connect to scmdb.net*, off by default. VerseKit then
opens a small port **on your own computer only** (`127.0.0.1:23456`).
VerseKit itself sends nothing over the internet: the scmdb page in **your**
browser picks the data up there and enters it into the scmdb account you are
signed in with.

**What the page gets:** the names of newly received blueprints (in English, as
in the catalogue) and, for running missions, the IDs from the game log, the
internal contract name, start, end and how it ended, only from the current
game session and with the game channel (LIVE, PTU …). No handle, no paths, no
storage data.

**Who else can get at it:** the port cannot be reached from other computers. It
only answers requests to `127.0.0.1` or `localhost`, so no website can get at
it through a name redirected to your computer, and browsers only let scmdb.net
read the answer. Other programs on your own computer can fetch the same
details. What scmdb does with it is governed by its own privacy policy at
[scmdb.net](https://scmdb.net).

## Contact

Privacy questions via the [issues](https://github.com/Xharig/VerseKit/issues),
security problems privately via the
[security advisory form](https://github.com/Xharig/VerseKit/security/advisories/new).
