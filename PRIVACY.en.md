# Privacy

[Deutsch](PRIVACY.md) · **English**

Verse-Kit runs on your computer. There is no account, no advertising and no
tracking. Once a day the program reports that it is running — without any ID,
and you can switch it off (see *Count usage*). This page states in full
what the program reads, what it fetches from the internet and what it sends.

## What is read on your computer

Star Citizen's log files (`Game.log` and `logbackups/`), your control files and
the game's text file. From them come your blueprint list, your contract log and
your statistics. The logs also contain your RSI handle; Verse-Kit uses it to
count only the blueprints of your own account. Everything stays in your
Verse-Kit data folder.

## What is fetched from the internet

Only reads of public data — nothing about you is sent along. As with any
request, the other side sees your IP address.

| For | From |
|---|---|
| New versions, catalogue and contract data | GitHub |
| Crafting data | scmdb.net |
| Prices, shops, places | UEX (uexcorp.space) |
| Ship values | erkul.games |
| Server status | status.robertsspaceindustries.com |
| Translations you choose | the respective source (such as StarStrings, Luftwerft, rjcncpt) |

With the environment variable `SC_BP_NO_NET=1` Verse-Kit fetches nothing.

## Count usage

Once a day Verse-Kit sends exactly these details to `nutzung-versekit.xharig.com`:

| Detail | Example |
|---|---|
| Program version | `3.65.0` |
| System | Windows or Linux |
| Language of the interface and of the game | `de`, `en` |
| whether you are offered test versions | yes / no |
| which areas are switched on | Ships, Workshop, Trade … |
| how the overlay runs | always visible / only on new blueprints |
| whether Verse-Kit starts with your computer | yes / no |
| whether new versions are installed automatically | yes / no |

Plus the **country** that Cloudflare detects on every request by itself — only
the country code, no city, no region. No ID, no name, no RSI handle, no paths.
All that is stored there is one counter per day and value — your IP address is
not kept. This counts how many people use Verse-Kit on a given day and which
parts, but not who, and no single installation can be followed across days.
Only the developer sees the numbers.

Switch it off: *Settings → General → Count usage*. `SC_BP_NO_NET=1` stops it
as well.

## Downloads from the website

The download buttons on versekit.xharig.com lead through `xharig.com/windows`
and `xharig.com/linux` to the file on GitHub. On that path we count: the day,
the system and the **country** that Cloudflare detects by itself. No IP
address, no city, no ID, no cookie. Downloading directly from GitHub is not
counted here.

## Problem report

The report is **only sent when you press "Send"** — after you have seen its
full text. It goes to `bericht.xharig.com`, a relay into the project's report
channel on Discord. User name, paths and anything that looks like a credential
are replaced beforehand.

## KRT Profit Basetool

Only if you explicitly connect Verse-Kit **and** turn syncing on. Both are off
by default.

**What goes there** — depending on the areas you turn on: your blueprints
(name, the Basetool's key, time, and how Verse-Kit knows them), your stock lots
from raw-material and trade storage (material, place, quality, amount,
"stolen") and your ships (type and insurance). Plus the name you give this
installation — never your computer's name — and, for the account check only,
your RSI handle. Never purchase data (price, purchase date, package), never
other members' data.

**What comes back:** your own blueprints, stock lots and ships from the
Basetool.

**What is stored:** the synced state in your data folder
(`basetool-<id>.json`, next to it the previous version
`basetool-<id>.bak.json`), the credentials only in your system's key store
(see [Security](SECURITY.en.md)). Data from the Basetool stays on your
computer and is not passed on to anyone.

This state file holds **your RSI handle** (from the account check) and the
**list of your synced blueprints**, stock lots and ships. It sits in the data
folder and therefore goes into the **backup zip** (*Settings → Backup &
reset*). If your data folder is in a cloud (OneDrive, iCloud …) or on a shared
drive, the file is there too. The credentials themselves are never in the data
folder and never in the backup.

**Disconnect** revokes the connection at the Basetool and deletes the
credentials (refresh token and this installation's key) and the synced state —
`basetool-<id>.json` **and** `basetool-<id>.bak.json`. Whatever is already in a
backup zip stays there until you delete it.

**Problem report:** It can contain the names of blueprints Verse-Kit cannot
match — including ones that came from the Basetool. Your RSI handle and the
credentials are not in it, and it only goes out when you send it yourself. What the Basetool itself does with your data
is governed by its own privacy policy at
[profit-base.online](https://profit-base.online).

## scmdb.net

Only if you turn on *Connect to scmdb.net* — off by default. Verse-Kit then
opens a small port **on your own computer only** (`127.0.0.1:23456`).
Verse-Kit itself sends nothing over the internet: the scmdb page in **your**
browser picks the data up there and enters it into the scmdb account you are
signed in with.

**What the page gets:** the names of newly received blueprints (in English, as
in the catalogue) and, for running missions, the IDs from the game log, the
internal contract name, start, end and how it ended — only from the current
game session and with the game channel (LIVE, PTU …). No handle, no paths, no
storage data. Other websites get no access. What scmdb does with it is
governed by its own privacy policy at [scmdb.net](https://scmdb.net).

## Contact

Privacy questions via the [issues](https://github.com/Xharig/VerseKit/issues),
security problems privately via the
[security advisory form](https://github.com/Xharig/VerseKit/security/advisories/new).
