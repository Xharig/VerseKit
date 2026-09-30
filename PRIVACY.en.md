# Privacy

[Deutsch](PRIVACY.md) · **English**

Verse-Kit runs on your computer. There is no account, no telemetry, no
advertising and no tracking. This page states in full what the program reads,
what it fetches from the internet and what it sends.

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
(`basetool-<id>.json`), the credentials only in your system's key store (see
[Security](SECURITY.en.md)). Data from the Basetool stays on your computer and
is not passed on to anyone.

**Disconnect** revokes the connection at the Basetool and deletes the
credentials and the synced state. What the Basetool itself does with your data
is governed by its own privacy policy at
[profit-base.online](https://profit-base.online).

## Contact

Privacy questions via the [issues](https://github.com/Xharig/VerseKit/issues),
security problems privately via the
[security advisory form](https://github.com/Xharig/VerseKit/security/advisories/new).
