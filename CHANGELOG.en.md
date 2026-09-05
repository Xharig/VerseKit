# Changelog

[Deutsch](CHANGELOG.md) · **English**

All notable changes to this project are documented here.

The project follows SemVer: `MAJOR.MINOR.PATCH`.

## v3.15.9 - 2026-09-05

> **Your play time now sits in the top bar** — the total, and while you play
> the current session next to it. Counted from the game's own logs and kept for
> good: Star Citizen clears out its old logs, the count stays.

### New

- **Play time in the title bar.** Next to "Backup" you now see how long you
  have played, with the running session in brackets while you play. Refreshes
  once a minute.

  Every session where you actually made it into the game counts — a start that
  never got that far is not play time. Short sessions count too; drawing a line
  there would be a claim about what "real" playing is.

  The count starts with the oldest log the tool can find, which on a grown
  install is several weeks back. Hover the entry and it tells you since when.

- **Its own database, and it is in the backup.** Star Citizen only keeps a
  limited number of logs; what disappears from those stays here. Moving to
  another machine, the backup button takes it along with no extra step.

### Improved

- **README tidied up:** the note under the screenshots still claimed v3.0.0,
  play time and "report a problem" were missing from the feature list, and two
  entries carried a text character instead of an icon.

## v3.15.8 - 2026-09-05

> **Stale entries now clear themselves even without a new contract.** Log out
> without handing in, then fly a session with no contracts at all, and the old
> entry used to sit in your log forever. Not any more.

### Fixed

- **A contract that was long gone stayed marked as open.** Until now such an
  entry was only cleared once a later session mentioned *other* contracts. Play
  a long session and take on none at all, and that moment never came.

  Now the silence counts too: if you are **in game for an hour and a half** and
  have no contract in your journal that whole time, you have none.

  A short failed start deliberately does **not** count — those often mention no
  contract even though one is still running. The threshold is measured against
  every log on file: from one hour onward no contract is ever closed wrongly;
  an hour and a half is what we use, for margin.

## v3.15.7 - 2026-09-05

> **No more "in progress" while the game is closed.** Log out without handing
> in or abandoning, and your last contract stayed marked as running forever.
> Now it reads "still open" — which is still true the next morning.

### Fixed

- **The last contract before logging out stayed on "in progress" for good.**
  The game writes no ending to the log when you log out, and such a case is
  only cleared up once a later session no longer mentions the contract — which
  never happens for the last one. While the game is not running it now reads
  **"still open"**.

  The contract is deliberately **not** closed: it is still accepted in game,
  and Star Citizen reports it again next time you log in. Only the wording was
  wrong — "in progress" claims "right now".

## v3.15.6 - 2026-09-05

> **The report page is made for writing now.** The field for your description
> is four lines tall and spans the full width, so you can read back what you
> are reporting before you send it. Paragraphs are kept.

### Improved

- **A proper text box for "What happened?"** — four lines, full width, below
  the description instead of squeezed in beside it. Before, you only saw a
  snippet while typing.
- **Lists stay lists.** Write three steps on separate lines and they appear on
  separate lines in the report — previously they were run together into one
  paragraph, and the sequence is exactly what makes them useful.
- **The note "You see exactly what you send" now sits above the buttons**
  instead of below them. It answers the question you ask yourself before
  clicking — and it can no longer drop off the bottom edge.
- **The "Info" group can no longer be collapsed.** That is where "Report a
  problem" lives — collapse it and you hide the way to get a problem off your
  hands, then go looking for it exactly when something is already wrong. Every
  other group stays collapsible. If you had it collapsed, it is back.

## v3.15.5 - 2026-09-05

> **The error report gives away nothing it shouldn't.** And the "What
> happened?" field is now empty again whichever way you send it, not just after
> pressing Send.

### Fixed

- **"Copy details" and "Report" clear the message field too.** Previously only
  Send did — anyone reporting another way would have had today's sentence
  quietly attached to their next report. If sending fails, it stays put.
- **Credentials no longer reach the error report.** If sending failed, the
  reporting address could end up in the report via the error log — and reports
  get shared publicly. The same now goes for access keys and tokens in
  addresses. The data lookup addresses stay readable: they tell you which
  request went wrong, which is exactly what you need.

## v3.15.4 - 2026-09-05

> **Blueprint found, count right.** The tool announced the new blueprint, but
> the list kept showing the old number without a green tick. Fixed — and the
> checkboxes in game now keep up while you play instead of hours later.

### Fixed

- **A new blueprint appears in the list straight away** — with the right count
  and the green tick. Until now the list showed the state from the moment you
  first opened it, and switching to another page and back did not help either.
  Reported by **Bushwick4712**.
- **Four more pages were just as stale**: "How far am I", "Crafting", "Backup &
  transfer" and "About" showed blueprint counts from startup. So did the error
  report — it carried the inventory from back then.
- **The checkboxes in game no longer lag behind.** They were only refreshed
  every six hours, together with the network lookups. Play a session and stop,
  and a blueprint you had just received still showed as missing in the mission
  text. Now it is checked every 30 seconds.

### Improved

- **The "What happened?" field is empty again after sending.** Otherwise your
  sentence from today would hang on next week's report. If sending fails, it
  stays put.

## v3.15.3 - 2026-09-05

> **Say what happened in the error report.** Next to your name there is now a
> field for one sentence — it appears at the top of the report, where it
> belongs.

### New

- **"What happened?" field** on the *Report a problem* page. One sentence is
  enough; it lands at the very top of the report, ahead of all the technical
  details. Not stored — it belongs to this one report.
  Idea by **Bushwick4712**.

### Improved

- **Two buttons removed** from the report page: "Save as a file" and "Open own
  folder". Nobody has used them in over a year, and both created work instead
  of saving it. "Copy details" does the same in one step less.

### Fixed

- **The mission log did not refresh when opened for the first time.** Start the
  watcher in the morning, hand in contracts at noon, then switch here for the
  first time — you saw the state from startup.
  Reported by **Bushwick4712**.

## v3.15.2 - 2026-09-05

> **Routes: the inputs stay put when you scroll down to the runs.** Starting
> point, cargo space and ship used to scroll out of view — on smaller windows
> immediately.

### Fixed

- **Routes: starting point, cargo space and ship scrolled away.** They now stay
  fixed; only the runs below them scroll. Reported by **Morkhan**.
- **Routes: both ends of a run are now named in the row** — "Aluminum · buy at
  Nyx Gateway → sell at Terra Gateway". It used to show only the destination
  behind an arrow; where to buy followed from the heading above it alone.
- **Nothing was shown next to "Find best routes anywhere"** while no trade posts
  had been collected. It now reads "0 of 184 trade posts" — so it is at least
  visible that the button collects something.
- **The error report shows more pages** (24 lines instead of 12): fetching the
  report means clicking through the info pages first, which pushed out exactly
  the page in question.

## v3.15.1 - 2026-09-05

> A small follow-up to v3.15.0.

### Fixed

- **Selling: the text cursor stayed in the amount field** even after clicking
  elsewhere — as if you were still typing.

## v3.15.0 - 2026-09-05

> **Shops now lists everything somebody sells — not just what you can craft.**
> 1,528 parts instead of 893: missiles, bombs, ammunition, weapon attachments,
> plus 174 ships to buy or rent. Every row shows class, size, grade and
> manufacturer, and you can filter by them — so you find the quantum drive that
> fits your ship without knowing all 44 names.
>
> Round trips finally work in Routes, and the best chains can be searched across
> every trade post. Plus a dozen small things that matter more day to day than
> they sound: lists that close again, dropdowns that wrap instead of being cut
> off, and data that only reloads after a patch instead of every week.

### New

- **Shops lists everything you can buy, not just what you can craft.** 1,528
  parts across 38 item groups — missiles, torpedo tubes, bombs, attachments,
  ammunition. The Boomtube Rocket shows up with its 19 shops in Pyro.
- **Ships to buy and rent.** 174 ships with purchase price, daily rental and
  cargo capacity, grouped by manufacturer, cheapest first.
- **Class, size and grade per item.** Military, civilian, industrial, stealth,
  competition, medical, mining, salvage — and grades A to D. As dropdowns and
  on every row, alongside the manufacturer.
- **The search finds item groups too.** "Radar", "missiles", "coolers" — no need
  to know whether something sits under Systems or Ship weapons.
- **Best chains anywhere in the verse.** Stop count, round trip and "short
  distance" apply after the full sweep across every trade post.
- **Manufacturer picker for ships (Routes).** 134 ships with cargo space; if you
  do not know the name, click your way there and the capacity fills itself in.
- **Enter the amount right in Selling.** Every picked commodity has its own SCU
  field — no detour through the cargo hold.
- **"Reset" in Shops and Routes.** One click and every dropdown, search field
  and input is back to its starting state.

### Improved

- **The list is grouped by item group** — coolers, quantum drives, power plants
  and shield generators with a heading and a count, instead of 176 names in a
  row. A chosen group is shown in full.
- **"My stock" is now "Material storage"** — matching the cargo hold.
- **Shops and ships only reload after a patch**, not every week. The catalogue
  is fetched in the background at startup so nobody waits on an empty list.
- **Picker fields open when you click into them** and close again when you click
  elsewhere or press Escape.
- **Search field and dropdowns stay put while you scroll.**
- **Chains show profit and outlay** — and what you need up front.
- **Clearer section names** throughout.
- **The change history groups patch versions** — v3.13.0 to .3 appear as one
  entry "v3.13".

### Fixed

- **Round trips never found a route.** Whether 2, 3 or 4 stops, it always said
  "no route". Now it tells you what to buy for the way back.
- **Dropdowns are no longer cut off** — they wrap instead.
- **HTML entities in names** — "Grey&apos;s Market" is back to "Grey's Market".
- **Controls: "Reset to default" was cut off** and sat next to four harmless
  buttons. It now has its own line.
- **Controls: mouse, keyboard and gamepad disappeared** when the window was made
  smaller.
- **Routes: large empty area above the inputs**, and the location list appeared
  at the bottom instead of under the search field.
- **Shops: clicking a part left the screen blank.**

## v3.15.0-rc13 - 2026-09-05

> **The amount now sits on the commodity itself.** Every picked commodity has
> its own SCU field — nothing left to mix up.

### Fixed

- **Amounts jumped between commodities.** A single amount field had to guess
  which one you meant, and guessed wrong as soon as you typed the number
  before picking. Every chip now has its own field.
- **Clicking empty space closes the picker list.** It used to close only when
  you clicked into another input field.
- **A test build appeared in the change history** (v3.11.1-rc2) even though
  that version never existed. Its content is in v3.12.0.

## v3.15.0-rc12 - 2026-09-05

> **The amount in Selling now takes effect immediately** — even when you type
> it after picking the commodity. And the dropdowns wrap instead of being cut
> off on the right.

### Fixed

- **The SCU amount was ignored** when the commodity had already been picked —
  the normal case. It now counts right away: the chip reads "Gold · 100 SCU"
  and the table calculates with 100 instead of 1.
- **Typing a commodity in full and pressing Enter picks it** — no need to click
  it in the list.
- **Dropdowns are no longer cut off.** With five of them the last one read
  "All gra…"; now it moves to a second row. Applies everywhere there are
  filter dropdowns.

### Improved

- The label on the amount field names the commodity: "How many SCU of Gold do
  you have?"

## v3.15.0-rc11 - 2026-09-05

> **"My stock" is now "Material storage"** — that is what is in it, and it
> matches the cargo hold next to it. Plus: in Selling you can enter the amount
> directly, without storing anything first.

### New

- **Enter the amount right in Selling.** An SCU field next to the search box:
  whatever is in it applies to the commodity you pick next. If you have 120 SCU
  of gold in your hold, you no longer have to record it first. Leaving it empty
  still works — then the cargo hold counts as before.

### Improved

- **"My stock" is now called "Material storage".**
- **Open picker lists close again** when you click elsewhere or press Escape.

## v3.15.0-rc10 - 2026-09-05

> **Reset buttons for Shops and Routes — and the shop catalogue loads at
> startup, not when you look.** Open the tab and the data is already there.

### New

- **"Reset" in Shops and Routes.** One click and every dropdown, search field
  and input is back to its starting state. What has already been fetched (the
  catalogue, collected trade posts) stays — throwing that away would mean
  waiting for nothing.

### Improved

- **The shop catalogue is fetched in the background at startup.** It used to
  run only when you opened the tab, leaving you a minute in front of an empty
  list. Thanks to the patch binding this happens at most once per game version.

## v3.15.0-rc9 - 2026-09-05

> **Chains now say what the number means.** Profit, what you have to put up
> front, and the outlay for each step.

### Improved

- **Chains show profit and outlay.** A tour used to be headed by a bare number.
  Now: "Profit 544,620 aUEC" and below it "You need 495,900 aUEC up front" —
  only the first run comes out of your own pocket, after that you buy from the
  previous run's proceeds.
- **Every step states its own outlay** — so you can see which leg ties up the
  money.

## v3.15.0-rc8 - 2026-09-05

> **Names with apostrophes read properly again.** "Grey&apos;s Market" is back
> to "Grey's Market" — everywhere names come from the price source.

### Fixed

- **HTML entities in names.** Apostrophes, quotation marks and ampersands
  arrived as `&apos;`, `&quot;` and `&amp;`. This affected manufacturers,
  items, terminals and locations.

## v3.15.0-rc7 - 2026-09-05

> **The switches now apply to "find best routes anywhere" too.** Round trip
> across three stops, short hops — it works that out across every trade post
> instead of still showing the single runs from before.

### New

- **Best chains anywhere in the verse.** Stop count, round trip and "short
  distance" apply after the full sweep as well. Calculated from what has
  already been collected — in under a second.

### Fixed

- **The location list appeared at the bottom instead of under the search
  field.** Typing "sera" put the suggestion far away from the input.
- **The location field opens the trade post list when you click into it** — no
  need to pick a system on the right first.

## v3.15.0-rc6 - 2026-09-05

> **Click the field and the list opens.** Until now you had to find the small
> arrow on the right — nobody looks there.

### Improved

- **Picker fields open when you click into them.** Cargo hold (commodity,
  location) and Selling: the full list first, narrowed down as you type, and
  back in full when you clear the field.

## v3.15.0-rc5 - 2026-09-05

> **Round trips work.** Every stop count used to return "no route" — now you
> get the run back to your start, with the cargo for it. And the lists only
> reload when a patch has actually landed, instead of every week.

### Fixed

- **Round trips never found a route.** Whether 2, 3 or 4 stops, it always said
  "no route found for that". Now it tells you what to buy for the way back.
- **Routes: the search text stayed put when switching manufacturer** and
  blocked the new selection.
- **Shops showed the wrong list during the first fetch** — the old blueprint
  types instead of item groups, with no ships. Now only the notice is shown
  until the data has arrived.

### Improved

- **Shops and ships only reload after a patch.** Shop prices, ship prices and
  the item-group catalogue change with a new game version, not with the
  calendar — a still-valid copy used to be thrown away every week, costing that
  minute of waiting for nothing.
- The ship dropdown is now called "manufacturer", as everywhere else.

## v3.15.0-rc4 - 2026-09-05

> **Class and grade have arrived — now you can find the part that fits.** Every
> row shows class, size, grade and manufacturer, and the first three have their
> own dropdowns. Plus a manufacturer picker for ships under Routes.

### New

- **Class and grade per item.** Military, civilian, industrial, stealth,
  competition, medical, mining, salvage — and grades A to D. Both as dropdowns
  and on every row, alongside size and manufacturer.
- **Manufacturer picker for ships (Routes).** 134 ships with cargo space across
  15 manufacturers. If you do not know the name, click your way there; the cargo
  capacity is on every row and fills itself in when you pick one.

### Fixed

- **Routes: large empty area above the inputs.** The suggestion lists did not
  release their space when empty.
- **Routes: the hint now names both ways** — type a starting point *or* press
  "Find best routes anywhere".

## v3.15.0-rc3 - 2026-09-05

> **Now you can find things you do not know by name.** Every row shows size and
> manufacturer, there is a size filter, and a chosen item group is listed in
> full instead of stopping at 40 rows. Plus two spots in the Controls tab where
> buttons were being cut off.

### New

- **Size and manufacturer on every row.** A list of 44 invented names tells
  nobody anything — "Size 1 · Wen-Cassel" does. That is how you find the quantum
  drive that fits your ship.
- **Filter by size.** Size 3 guns, size 1 coolers — what does not fit is not
  listed in the first place.

### Improved

- **A chosen item group is shown in full.** All 87 guns, all 201 helmets — no
  more cut-off at 40 rows.
- **"… 38 more in this group" is clickable** and opens the group.
- **Twelve rows per group** in the overview instead of six.
- No manufacturer dropdown any more — it sits on the row, and the search finds
  it.

### Fixed

- **Controls: "Reset to default" was cut off.** The button now sits alone on its
  own line — visible, and no longer right next to four harmless ones.
- **Controls: mouse, keyboard and gamepad disappeared when the window was made
  smaller.** The button rows now wrap instead of being cut off on the right.

## v3.15.0-rc2 - 2026-09-05

> **Shops is actually usable now.** The list is grouped by item group, and the
> search finds group names too — type "radar" and you get the radars, without
> knowing where they are filed. Search field and dropdowns stay put while you
> scroll. And **ships** have arrived: purchase price, daily rental and cargo
> capacity, sorted by manufacturer.

### New

- **Ships to buy and rent.** 174 ships with purchase price, daily rental rate
  and cargo capacity — grouped by manufacturer, cheapest seller first.
- **The search finds item groups too.** "Radar", "missiles", "coolers" — you no
  longer need to know whether something sits under Systems or Ship weapons.
  German and English names both work.

### Improved

- **The list is grouped by item group.** Instead of 176 names in a row, coolers,
  quantum drives, power plants and shield generators each get a heading with a
  count — a few rows from each, and how many more follow.
- **Search field and dropdowns stay put while you scroll.**
- **The selection stays coherent.** Item groups that do not exist in the chosen
  section are no longer offered.
- **Clearer section names** throughout.

### Fixed

- **Typing clears the previous answer.** It used to stay on screen, making the
  search field look unresponsive.
- **A truncated list says so.** It used to stop silently at 40 rows, looking
  like there was nothing more.

## v3.15.0-rc1 - 2026-09-04

> The **Shops** tab used to list only parts that have a blueprint. Anyone
> looking for missiles, bombs or railgun ammo faced an empty list — even though
> the shops were known all along. Now it lists what is actually sold: **1,528
> parts instead of 893**, sorted by section and item group.

### New

- **Shops lists everything you can buy, not just what you can craft.** Missiles,
  torpedo tubes, bombs, weapon attachments, ammunition — 1,528 parts across 38
  item groups. The Boomtube Rocket now shows up with its 19 shops in Pyro, right
  where you look for it.
- **Two dropdowns instead of one**: first the section (Armor, Ship weapons,
  Systems …), then the item group within it. Largest group first.
- Below the filter it now says **how many parts are available** — instead of an
  empty area that looks like something went wrong.

### Fixed

- **Shops: clicking a part left the screen blank.** The suggestion list did not
  release its space, so the shops were drawn below it — outside the window. The
  view now jumps back to the top after you pick a part.
- **Parts without an entity ID are reachable.** About a third of the catalogue
  has none; for those there were never any shop prices at all.

## v3.14.0 - 2026-09-04

> The biggest release so far. Your **controls** now belong in the tool: view
> your bindings, change them, save them as a profile — and the backup finally
> takes them along. Plus four new answers about money: what a part costs in a
> shop, whether building it yourself pays off, which trade route earns most,
> and where your next ship is parked.
>
> And above all a rule that was missing before: the tool now says so when it is
> not sure — after a patch, with ageing prices, where the data has gaps.

### Added

- **Controls in the tool.** Which stick has which number, what is bound to it,
  what is still free — with readable names instead of `v_eject`. Bind by
  pressing the button, conflict warnings, reset to defaults.
- **Save bindings as a profile.** Under a name you choose, right where Star
  Citizen looks for it — loadable in game with `pp_rebindkeys load <name>`.
  Importing works from the same list, no file hunting.
- **The backup takes your controls along.** Active bindings and every profile.
  They are only restored when you ask for it.
- **"Shops" tab.** Where a finished part sits on the shelf and what it costs,
  listing only what is sold somewhere. In crafting the shop price stands next
  to the ingredient costs — so "is building it worth it?" answers itself.
- **"Routes" tab.** Enter your location, cargo hold and money and get the runs
  that pay off — single, chained across up to four stations, or as a round
  trip. Sorted by profit or by short distance. Suggested by **YoshimitsuDE**.
- **A ship instead of a number.** Pick your ship and the cargo hold fills
  itself in; you also see where to buy and rent one.
- **Reputation points, cooldown, shareability and blueprint chance in the
  mission text.** The figures were in the data but appeared nowhere in game.
  Suggested by **Bushwick4712**.
- **Warning after a patch.** If the prices still come from the version before,
  it says so — with both version numbers.
- **Full stock is shown.** A terminal with no demand will not take your cargo,
  even though the price is still listed.

### Improved

- **Every figure says what it is.** Profit, outlay, available amount — and what
  caps the amount: more money, a bigger ship, or simply the stock on site.
- **The pickers work the same everywhere.** Search field with suggestions from
  two letters, dropdowns sorted by size.

### Fixed

- **Text next to an icon was black on a dark background.**
- **Exported bindings could not be loaded in game** — they did not have the
  format Star Citizen expects for profiles.
- **Shop prices were missing for many parts** although the game sells them.
- **The error log reported two problems that did not exist.**

## v3.14.0-rc20 - 2026-09-04

> "Only lists what is for sale" — and then every item was there anyway. The
> filter worked correctly but needed a minute, and meanwhile nothing said it
> was still running.

### Fixed

- **"Shops" still listed items that are sold nowhere.** The watcher checks
  this once on first opening and that takes about a minute — until then the
  full list stayed, with no visible sign why. The notice now sits above the
  list instead of below it, and the counts in the dropdowns are corrected
  afterwards.

## v3.14.0-rc19 - 2026-09-04

> Two errors that were not errors: the error log reported two made-up problems
> on every price fetch. A log full of false alarms soon goes unread — and the
> real error disappears in it.

### Fixed

- **The error log reported two problems that did not exist.** Every fetch of
  the sell data added two entries about supposedly truncated responses — they
  were complete all along.

## v3.14.0-rc18 - 2026-09-04

> The mission text in game now tells you what a mission pays and when it is
> available again. And the "Shops" tab only lists what is genuinely for sale —
> with ship weapons and FPS weapons as separate groups.

### Added

- **Reputation points, cooldown, shareability and blueprint chance now appear
  in the mission text.** The figures were in the data all along but showed up
  nowhere in game — before accepting you now see what the mission pays, when it
  is available again, whether it can be shared and how likely a blueprint
  drops. Suggested by **Bushwick4712**.
- **Ship weapons and FPS weapons are separate groups.** Before, 270 items sat
  in one bucket called "Weapons" — someone looking for a ship gun is not
  browsing the same list as someone after a rifle.

### Improved

- **"Shops" only lists what is sold somewhere.** Of 1,597 items, 893 remain —
  the rest is not traded anywhere and would only have sent you clicking into
  nothing. On first opening, the watcher checks once; that takes about a minute
  and shows its progress.
- **The dropdowns are sorted by size.** The largest groups were last
  alphabetically and only reachable by scrolling — so they looked missing.

## v3.14.0-rc17 - 2026-09-04

> A route now tells you not only what you earn but what you have to put down —
> and what caps the amount. At 69 of 120 SCU that is the difference between
> "buy a bigger ship" and "bring more money".

### Added

- **The outlay is shown for every run.** What you have to pay up front, and how
  much is actually available at that place.
- **What caps the amount.** Instead of just "69 SCU" it now says whether more
  money, a bigger ship or simply the stock on site is the limit.

### Improved

- **The columns are labelled.** The big number is the profit — that was written
  nowhere and was easy to mistake for the purchase price.
- **"Buy at …" instead of just "from …"**, so it is clear which of the two
  places is where you buy.

### Fixed

- **It said "187 of 184 trade posts"** — more than there are. Places left in
  the cache from an earlier version, which are not trade posts at all, were
  counted too.

## v3.14.0-rc16 - 2026-09-04

> The search for the best route now actually shows its result — until now it
> ran through and left the page unchanged. Plus a system picker for routes, the
> ship as a search field instead of a separate window, and a top list under
> Sell that no longer stays empty.

### Added

- **System picker for "Where are you right now?".** Open Stanton, Pyro or Nyx
  and you see the trade posts inside, without typing anything.

### Improved

- **You now pick your ship in a search field**, no longer in a separate window
  — with suggestions from two letters, like everywhere else in the tool. The
  cargo hold is shown on every suggestion.
- **The switches for profit, stops and round trip are visible from the
  start.** They used to appear only once a location was picked, so anyone
  opening the page for the first time never saw they existed.

### Fixed

- **The search for the best route showed nothing.** It collected every trade
  post and afterwards still said "Type where you are above".
- **Under Sell, the "what pays best" list stayed empty.** The heading was
  there, the rows were not.
- **The ship window only showed seven of 134 ships** and silently hid the rest.

## v3.14.0-rc15 - 2026-09-04

> A route now tells you where to buy what — before it only named the
> destination and you had to work out the rest. Plus a search for the best
> route anywhere, and two pages that no longer start out empty.

### Added

- **The best route anywhere, no matter where from or to.** One button collects
  the runs from every trade post and shows the most rewarding ones. It takes
  about a minute and a half and only runs when you start it — with progress.
- **The "Shops" tab now has dropdowns** for type and manufacturer, the same
  ones you know from the blueprint list. Without a search term they fill the
  list.
- **"Sell" now shows what pays best when nothing is picked** — the twelve
  best-paying goods, click to select.

### Improved

- **Every step of a route names both places.** "At Seraphim: buy 120 SCU
  Copper → sell at Rat's Nest" instead of just "Copper → Rat's Nest". The whole
  path is shown above the route as well.

### Fixed

- **A gap appeared at the top after picking a location.** The suggestion list
  vanished but the view stayed where it was.

## v3.14.0-rc14 - 2026-09-04

> The location picker for routes now only lists places that actually trade in
> goods. Seraphim Station used to fill sixteen rows — fifteen of them were
> clothing shops, food stands and fuel points.

### Fixed

- **The location picker listed shops, food stands and fuel points.** Picking
  one gave you no route — there is nothing to trade there. Only places that
  really buy and sell goods are offered now: 184 instead of 826.

## v3.14.0-rc13 - 2026-09-04

> Just pick your ship — the cargo hold fills itself in. And if you do not own
> it yet, you see right away where to buy or rent one.

### Added

- **Pick a ship instead of typing the cargo hold.** 134 ships with a hold, the
  SCU figure right there in the list. Once chosen, you also see where that ship
  is cheapest to buy and to rent.

## v3.14.0-rc12 - 2026-09-04

> Routes can now run across three or four stations — and, if you like, lead
> back to where you started. Plus four fixes to places that gave away too
> little on the first attempt.

### Added

- **Routes across several stations, round trips included.** Pick two, three or
  four stations — and whether the route should end where it began. A round
  trip can be repeated instead of leaving you somewhere with an empty hold.

### Improved

- **Amounts now come with their unit.** Before it was a bare number between
  SCU amounts and distances — you could only guess what was meant.
- **Buyers can be told apart.** A station has many terminals, and they trade in
  different things. Before, the same station name appeared eight times in a
  row; now the terminal comes first. Both are searched.
- **The chosen location stays put.** It used to vanish from the field the
  moment you clicked it.

### Fixed

- **Shop prices were missing for many parts although the game sells them.** Our
  price source lists some items under a different identifier than the game —
  for those we now also search by the full name. For the CF repeaters, six of
  nine now have a price instead of two.

## v3.14.0-rc11 - 2026-09-04

> Tell the watcher where you are and what fits in your hold — and it tells you
> what the next run is worth. Over two stations if you like, sorted either by
> best profit or by short distance.

### Added

- **New "Routes" tab.** Enter your location, your cargo hold and your money,
  and you get the runs that pay off — single, or chained across two stations.
  Switch between best profit and short distance. Suggested by **YoshimitsuDE**.
- **The maths uses what is actually possible.** Not only your hold limits the
  amount, but also the stock at the buying end and your money — otherwise you
  would see a profit for a load that cannot be bought at all.

## v3.14.0-rc10 - 2026-09-04

> Build it or buy it? The watcher now answers that itself. Open a blueprint and
> you see what the same part costs finished in a shop — and the new **Shops**
> tab tells you where it sits on the shelf.

### Added

- **"Buy it finished" in crafting.** Above the ingredients you now see what the
  part costs in a shop and where it is cheapest. Together with the ingredient
  costs below, that answers by itself whether the effort is worth it.
- **New "Shops" tab.** Type a name, click the part — and you see every outlet
  with price, location and the condition of the goods. The cheapest is on top.

### Improved

- **Matching goes by the item's identifier, not by its name.** No similarly
  named part can slip in that way.

## v3.14.0-rc9 - 2026-09-04

> The sell tab now tells you what it does not know. After a patch you can see
> at a glance that the prices still come from the version before — and a
> terminal whose stock is filling up says so before you fly there.

### Added

- **Warning after a game patch.** Star Citizen shuffles prices with every
  patch. If the numbers still come from the version before, it now says so —
  with both version numbers, instead of just "outdated".
- **Full stock is shown.** A terminal whose stock is filling up, or already
  full, takes your cargo badly or not at all — even though the price is still
  listed. That now appears next to the affected commodity.

### Improved

- **The display stays quiet when there is nothing to say.** More than nine out
  of ten buyers have room — no marker appears there on purpose. A hint that is
  always present does not get read.

## v3.14.0-rc8 - 2026-09-04

> Getting to your profiles is short now: when saving you see what is already
> there, and one click reuses the name. When importing you pick your profile
> from a list instead of hunting for it in the file system.

### Added

- **Import a profile without hunting for it.** When importing you can pick from
  your saved profiles — one click is enough. For bindings someone sent you, the
  file picker is still right there.

### Improved

- **The name prompt now matches the program.** It used to appear as a grey
  system dialog with English labels. Now it has the same colours as everything
  else — and existing profiles are listed one per line instead of a single row
  that ran off the edge of the window. Clicking a name reuses it.

## v3.14.0-rc7 - 2026-09-04

> Your bindings stay yours now. Save them as a profile under a name you pick —
> right where Star Citizen looks for it, and loadable in game with a single
> line. And the big backup finally takes them along: the active bindings and
> every saved profile.

### Added

- **Save your bindings as a named profile.** The profile lands where Star
  Citizen looks for it — load it in game with `pp_rebindkeys load <name>`.
  Previously you only got a copy you had to move back by hand.

### Improved

- **The backup now includes your controls** — the active bindings and every
  saved profile. They are only restored when you ask for it, and the active
  bindings only on a separate confirmation, with the previous state set aside
  first.

### Fixed

- **An exported binding could not be loaded in game.** The file did not have
  the format Star Citizen expects for profiles. Importing a binding is
  converted correctly now as well.
- **Binding profiles were missed when the folder existed in two spellings.**
  Some installations have both `controls` and `Controls`; profiles from both
  are now merged.

- **Text next to an icon was black on a dark background and barely readable.**
  Visible in a blueprint's source block on the line "n more ways to this
  blueprint". Every row showing an icon with a word beside it was affected.

## v3.14.0-rc6 - 2026-09-04

> A bug that swallowed half the defaults: binding an action to your stick made
> its key disappear from the list. Headlights, respawn, crouch and left mouse
> button ended up under "not bound yet" although they have been bound all
> along.

### Fixed

- **Your own binding pushed the default off every device instead of just its
  own.** The game does it differently: put "respawn" on a stick button and the
  `F` key still works. Now it does here too. The combined view therefore shows
  **572 instead of 326** bindings, and the "not bound yet" list shrank from 444
  to 310 — the rest was never free.

## v3.14.0-rc5 - 2026-09-04

> Export, import, reset: your bindings can now be written to a file and read
> back — no detour through the game console needed.

### New

- **Export and import bindings.** As `actionmaps.xml` to keep or pass on, or
  as a **CSV list** to look up and print. Until now that only worked via
  `pp_rebindkeys export` in the game console.
- **Reset to defaults.** One button, red and with an explicit confirmation
  that also says how many of your own bindings are affected. Deadzones, curves
  and sensitivity stay untouched — those are device settings, not bindings. A
  backup is written first.

### Improved

- Axis wording now follows each language's own habit.

## v3.14.0-rc4 - 2026-09-04

> The list now reads like a sentence instead of a data dump: instead of
> `js2 · x · v_boost` you get your stick's name, "Axis X" and "Boost". And the
> 444 actions that have no key at all are finally reachable.

### New

- **Actions with no binding at all** got their own view — *Not bound yet*.
  Without it you could not get at them: what is bound nowhere appears in no
  list, and what appears in no list cannot be clicked to bind it. That covers
  **444 actions** — emotes, mining details, emergency commands.

### Improved

- **The device name is in the list, not `js1`.** The number only means
  something to the game; whoever sits in front of the list wants to know
  whether that is the left or the right stick.
- **Inputs in plain language.** `x` becomes "Axis X", `button23` "Button 23",
  `hat1_up` "Hat 1 ↑", `lshift` "Left Shift". This matters most for axes: `x`
  on its own read like the X key.
- **More actions with names.** Where the German wording is missing, the
  English one is used instead of none. For the 382 actions the game itself
  gives no name to, a tidied-up name is shown (`v_boost` → "Boost") — in grey,
  so the difference from a real name stays visible.
- The same binding no longer appears twice when the action sits in several of
  the game's groups.

## v3.14.0-rc3 - 2026-09-04

> Now you can rebind as well: click a row, press the button or key, apply.
> Stick, keyboard and mouse alike — and if the input is already taken, you see
> that beforehand instead of mid-fight.

### New

- **Rebinding by pressing.** Click a row in the list, then press the key or
  button you want — no need to know its number. Joystick buttons and axes,
  keyboard, mouse and mouse wheel are all recognised.
- **Warning on double bindings.** If something else already sits on that
  input, the window says so **before** you apply. You may still double-bind —
  sometimes that is what you want.
- **Remove a binding.** The game treats this as its own state: a removed
  binding stays removed and is not replaced by the default.

### Improved

- A backup is written next to the binding file before every change. The way
  back is a rename, not rebuilding your whole control setup.

## v3.14.0-rc2 - 2026-09-04

> The joystick page now speaks plain language: instead of `v_eject` it says
> "Eject", in whichever language you run the tool in. Keyboard, mouse and
> gamepad are included, and you can switch between your own changes, the
> defaults, and both together.

### New

- **Actions in plain language, German and English.** The binding list no
  longer shows `v_eject` but "Eject" or "Schleudersitz" — depending on the
  language set in the tool (not in the game). The wording comes from the game
  itself; nothing is translated and nothing is guessed: where the game gives
  no name, the technical one stays. Search covers both.
- **Keyboard, mouse and gamepad in the same list.** Look up what a key does
  without switching to the game. *Requested by Morkhan.*
- **Three views to switch between** — *Changed by me* · *Everything* ·
  *Default*. That answers two different questions: "what did I change" and
  "what does this key actually do". Your own bindings are marked as such in
  the combined view.

### Fixed

- **The search field lost the cursor after every character** — you had to
  click back in for each further letter. The page rebuilt itself completely on
  every keystroke, including the field being typed into.

## v3.14.0-rc1 - 2026-09-04

> New in this one: a page for your joysticks. It shows which stick has which
> number, whether your bindings still point at a device that is actually
> there — and what is bound to which button. All straight from the game's own
> files, for any stick, with no device profile to install first.

### New

- **Joysticks** — a new tab under "Settings". Three things on one page:
  - **Which stick is which number.** Star Citizen ties bindings to a number
    (`js1`, `js2`), not to a device. The page puts both side by side: what the
    game last connected, and what your `actionmaps.xml` says.
  - **Is everything still there?** A bound device that is missing shows up in
    red. If one reports under a new identifier — different port, new firmware,
    replacement unit — its old bindings can be carried over with one click. A
    backup is always written next to the original file first.
  - **What is bound to which button.** Your complete binding list, searchable
    and filterable by device. This works for **any** stick, pedal or gamepad —
    no device needs to be known in advance.

### Improved

- The device list comes from the game's own startup log instead of a system
  device query. That makes it identical on Windows and Linux, and it needs no
  extra packages at all.

## v3.13.3 - 2026-09-04

> Two things that get in the way day to day: missions that stayed listed as
> running long after they were gone, and an overlay that got smaller with every
> start.

### Fixed

- **Missions stayed listed as running when they were already gone.** If another
  player takes a mission you just accepted, the game reports no completion — it
  only writes the ending into a technical line. The watcher did not count that
  as an ending, so the mission stayed in the overlay for the rest of the
  session. Such silent endings are now recognised, via the mission's id.
- **The mission log now tells completed and abandoned apart.** Because the
  silent endings carry the reason as well, an abandoned mission no longer shows
  as "in progress" or "no longer open" but as **abandoned**. In a real log with
  400 runs this corrected 60 entries.
- **The overlay got smaller with every start.** With a fixed screen corner set,
  it was squeezed to the minimum size on startup — 620×316 became 564×150. Your
  remembered size applies again.

## v3.13.2 - 2026-09-04

> Two places where the tool quietly stopped being up to date: the check marks
> in the mission texts now follow your inventory, and the mission log grows
> along instead of only growing at startup.

### Fixed

- **The check marks in mission texts stayed put when your inventory grew.**
  They were only rewritten for a new translation, new contract data, or when a
  game patch replaced the file — your own inventory was not on that list. From
  the first blueprint you unlocked afterwards, the game showed too few check
  marks with nothing pointing at it. The tool now remembers the state of your
  inventory and rewrites as soon as it changes.
  ⚠ For the new check marks to show up in game you have to log in again — Star
  Citizen only reads the texts when you sign in.
- **The mission log only grew at startup.** Anyone starting the watcher in the
  morning and finishing a mission at noon did not find it there. The page now
  re-reads every time you open it.

## v3.13.1 - 2026-09-04

> The tool starts noticeably faster — the more blueprints you have, the more
> so. It also finds blueprints again whose names were rewritten by MrKraken's
> StarStrings.

### Improved

- **Startup no longer takes seconds.** On start, your inventory is checked
  against the catalogue once — and the catalogue file was re-read from disk for
  **every single blueprint**. With 406 blueprints that meant 3.6 seconds on
  every start, without anything changing. It is 11 milliseconds now. With few
  blueprints you hardly notice; with many, clearly.

### Fixed

- **Blueprints counted as "not in the catalogue" when StarStrings is in use.**
  MrKraken's translation puts class, size and grade in front of the name
  (`Ind/2/B Citadel`), while the catalogue knows `Citadel` — so the blueprint
  was not found. Affects 465 items: coolers, shields and power plants.
  Reported by Haldjas

### Thanks

- **Haldjas** — for the report both points came out of. Without his note that
  he uses StarStrings, the second one would have been given the wrong cause.

## v3.13.0 - 2026-09-04

> The most requested button is here: **Backup**. Everything only you have, in a
> single file — and back again with the same button. Meant for moving to
> another PC: file onto a stick, read it in on the new machine, keep playing.

### New

- **Backup** (new button at the top, next to "Run setup"). Everything only you
  have, in **one file**: blueprint inventory, workshop stock, trade hold,
  mission log, watchlist and settings. The same button restores such a file —
  meant for moving to another PC: file onto a stick, read it in on the new
  machine, keep playing.
  The downloaded reference data stays out, the program fetches that on its own;
  this keeps the backup small.
  When restoring, your current data is put aside first, and paths from the old
  machine are cleared so the program searches on the new one instead of
  pointing nowhere.

## v3.12.0 - 2026-09-04

> New is the **mission log**: it shows which missions you played when, how often
> — and which blueprint came out of it. That answers the question everyone ends
> up asking: which mission was it again that dropped the helmet? It is already
> filled on first start, because the game's kept logs reach back weeks.
>
> On top of that, the blueprint list opens noticeably faster now.

### New

- **Mission log** (new tab under "Blueprints"). The most recent mission sits on
  top, a search field finds them by name. If a blueprint came out of a mission,
  it is listed underneath. At the end you see how often you ran the same
  mission.
- **Already filled on first start.** The game's kept logs are read once, so you
  see the past weeks right away. From then on the log grows with you and stays,
  long after the game has dropped its own.
- **The log goes into the export folder** — next to your inventory and the other
  lists. Whoever backs up their data has it along.

### Improved

- **The blueprint list opens faster** — noticeable with large inventories.
  Reported by Haldjas

### Fixed

- **Missions stayed on "in progress" forever.** They are now marked **no longer
  open** as soon as a later play session no longer knows them.
- **Blueprints hung on the wrong mission.** A mission yields at most one
  blueprint; seemingly running old ones pulled in every new find anyway. A
  blueprint now only goes to a mission that actually ran in the same session —
  and rather to none at all than to the wrong one.
- **Foreign markers appeared inside mission names.** They come from other
  translation tools and are now stripped.
- **Long mission names were cut off on the right** — they now wrap, as does the
  blueprint line below them.
- **A self-chosen data folder was lost on restart.** The change was saved, but
  to a place that is not read at startup.

### Thanks

- **Haldjas** — for pointing out the stuttering blueprint list, which is where
  this whole section ended up coming from.

## v3.12.0-rc4 - 2026-09-04

> **Test build.** The mission log listed missions as in progress that were long
> over — and because they looked active, they collected every blueprint found
> later. Both are fixed, and the log is rebuilt once on first start.

### Fixed

- **Missions stayed on "in progress" forever.** A mission with no recorded
  ending stayed listed as running, in some cases for months. It is now marked
  **no longer open** as soon as a later play session no longer knows it — the
  game re-reports every running mission when you log in.
- **Blueprints hung on the wrong mission.** A mission yields at most one
  blueprint; the seemingly running old ones pulled in every new find anyway. A
  blueprint now only goes to a mission that actually ran in the same session
  and has none yet — and rather to none at all than to the wrong one.
- **The log is rebuilt once** so the old mis-assignments disappear. This makes
  the first start take a few seconds longer.

## v3.12.0-rc3 - 2026-09-04

> **Test build.** The mission log reads cleanly now: foreign markers are gone
> from the names, and long mission names are no longer cut off at the right
> edge.

### Fixed

- **The mission log showed markers inside mission names** — such as
  `Blackbox Retrieval <EM4>[BP]?</EM4>`. They come from other translation tools
  that flag the same missions. They are now stripped, including retroactively in
  a log that already exists.
- **Long mission names were cut off on the right.** Names like "Verified Bounty:
  … | HRT (Large multi-crew ship, medium support)" now wrap, as does the
  blueprint line below when a mission dropped several. Both follow the window
  width.

## v3.12.0-rc2 - 2026-09-04

> **Test build.** A fix for rc1: the mission log stayed empty when you opened
> it right after startup.

### Fixed

- **The mission log stayed empty.** On startup the game's kept logs are read
  first — anyone opening the page during that moment saw "no missions recorded"
  and never anything else, because the page stayed as it was built. It now
  refreshes every time you open it.
  Affects everyone who tried rc1.

## v3.12.0-rc1 - 2026-09-04

> **Test build.** New is the mission log: it shows which missions you played
> when, how often — and which blueprint came out of it. Feedback is welcome
> through the error report inside the program.

### New

- **Mission log** (new tab under "Blueprints"). The most recent mission sits on
  top, a search field finds them by name. Missions in progress show how far they
  are ("3 of 5 objectives"), finished ones take a single line. If a blueprint
  came out of a mission, it is listed underneath — the answer to "which mission
  was it again that dropped the helmet?".
- **It is already filled on first start.** The game's kept logs are read once,
  so you see the past weeks right away. From then on the log grows with you and
  stays, long after the game has dropped its own.
- **The log goes into the export folder** — next to your inventory and the other
  lists. Whoever backs up their data has it along.

### Fixed

- **The blueprint list opens faster.** Affects everyone for whom opening it felt
  sluggish.
  Reported by Haldjas

## v3.11.0 - 2026-09-03

> **Renaming your LIVE folder to HOTFIX no longer leaves you stranded.** When a
> patched build appears alongside LIVE, hardly anyone downloads the game again —
> you rename the folder you already have so the launcher only fetches the
> differences. That made the game folder you had set disappear, and the watcher
> reported "Star Citizen not found" even though a path was sitting right there
> in the settings. Now it notices and asks you briefly.

### New

- **The watcher notices when your game folder has moved.** If the folder you set
  is gone and another game channel sits next to it, a short question appears
  soon after startup: switch over, or leave things as they are. With several
  channels you see them all, each with the time it was last played — the most
  recent one on top. You are asked **once per program start**; choosing "Not
  now" keeps it quiet.
  Reported by Haldjas

### Fixed

- **A HOTFIX game folder was not found.** If you had renamed your folder to
  download only the differences, the watcher no longer found the game on its own
  — no blueprints, no logs. Picking the folder by hand always worked.
  Affects everyone playing a patched build.
  Reported by Haldjas
- **A game folder sitting right next to the one you set was overlooked.**
  Affects everyone who did not install Star Citizen at the default location.
- **The setup wizard showed unreadable button labels.** In step 4 of 5, and in
  the settings under "Notes in game", the three text-source buttons were not
  labelled but showed internal names instead.
  Affects everyone since 26 August.
  Reported by Haldjas

### Thanks

- **Haldjas** — for spotting the wrong button labels in the setup wizard, and
  for describing the renamed game folder precisely enough that two bugs and one
  new feature came out of it.

## v3.10.0 - 2026-09-03

> **The refinery now tells you up front which method pays off.** Nine refining
> methods sit in the terminal, and the game shows a single thin line for each —
> the only way to compare them is to click through all nine. The mining section
> asks what matters to you instead, and names the method. Two of the nine are
> never worth taking.

### New

- **Which refining method?** In the mining section: pick what matters most —
  yield, cost or speed — and what comes second. You get one of the nine
  methods, plus all nine side by side.
- **Two methods now come with a warning.** `Dinyx Solventation` and
  `XCR Reaction` are beaten by another method on every count. The tool works
  that out on its own.
- The recommendation needs **no network** and no downloaded mining data.

### Improved

- The per-ore refinery comparison is described correctly now: it compares
  **stations**, not methods.

## v3.9.8 - 2026-09-03

> **My Stock shows everything again.** If you had expanded the „Enter refinery
> yield" section, the page only came up half-built the next time you opened it —
> the list of entered items was missing. The data was never affected, only the
> display.

> [!important]
> Affects every release since v3.4.1. If your stock list looked empty although
> you had items entered: they were always there. After updating they are
> visible again, with nothing for you to do.

### Fixed

- **The stock page broke off while building when the refinery section was
  expanded.** A yes/no setting was read as a file path; that threw an error and
  took the rest of the page with it. Collapsing the section did not help,
  because a page is only built once — only a restart brought it back.

## v3.9.7 - 2026-09-03

> **Contracts give up their blueprints more reliably.** Multi-step contract
> chains only showed the blueprint note on the first step — open a later one in
> game and you found nothing, even though there was something to be had. Every
> step now carries its list. The green strip also stays where the overlay
> actually sits after a restart, and crafting finally explains why its headline
> number is sometimes smaller than your own collection.

### Fixed

- **The green strip stayed behind in the old corner after a restart.** The
  overlay moved to its place, the strip did not — it sat where nothing was any
  more and opened the window somewhere else. Only happened on the first start
  after changing corners. Reported by Haldjas (pr0)
- **In multi-step contract chains, the blueprint note only appeared on the
  first step.** Opening a later step in game showed nothing about it, even
  though the overlay reported the blueprint. Affects the Battaglia chains; the
  note now appears on every step.

### Improved

- **Crafting now says when blueprints are unclear.** A blueprint whose name is
  shared by several items deliberately does not count as craftable — which made
  the headline number smaller than your own collection, with nothing to explain
  the gap. It now names the unclear ones alongside.

## v3.9.6 - 2026-09-03

> **The window listens again.** Once you had pulled it wider, you could not
> make it shorter afterwards — whatever height it happened to have became its
> floor. Width and height can now be adjusted independently again.

### Fixed

- **The window could no longer be made shorter once it had grown wider.**
  Opening a page whose button row needed more room turned the window's current
  height into its minimum height from then on.

## v3.9.5 - 2026-09-02

> **Less waiting, and the dropdowns are back where they belong.** The window
> opens noticeably faster — it only builds the page you asked for, and shows
> itself once that page is ready. Plus three fewer bugs: white title bars,
> dropdowns at the screen edge, and a blueprint list that took its time on
> first open.

### Improved

- **Settings open noticeably faster.** The window always built the blueprint
  list first and hid it again right away — even when a different page was
  wanted. Now only the requested page is built. Reported by Haldjas (pr0)
- **The window appears finished** instead of assembling itself in front of you.
  Reported by Haldjas (pr0)

### Fixed

- **Windows: the watcher wrote the game's text file back with different line endings.** Every line of the file changed even though nothing differed in content — other tools' markers were affected too. The content itself was never altered.
- **Dropdowns opened at the left screen edge** instead of below their field.
- **The blueprint list was slow to open the first time.** It asked the file
  system about every single blueprint instead of looking once.
  Reported by Haldjas (pr0)
- **The main window kept a white title bar** while other windows got a dark
  one. Which window it hit depended on timing, and once light it stayed light
  until the program restarted.

## v3.9.4 - 2026-09-02

> **Two follow-ups on the corners.** The chosen corner now takes effect at
> startup too, and the overlay no longer hides behind the taskbar.

### Fixed

- **The chosen corner was ignored at startup.** The overlay came back where it
  last was after every start; the setting only applied once you collapsed and
  expanded it. Reported by Haldjas (pr0)
- **In a bottom corner the overlay slipped behind the taskbar.** The narrow
  green strip was then hard to hit and hovering did nothing. The usable screen
  area is now used instead of the full one. Reported by Haldjas (pr0)

### Improved

- **The error report shows how long each page takes to build** — and names the
  exact Tk release instead of just the major version. Anyone reporting a sluggish
  window now supplies the numbers the cause can be pinned down with.

## v3.9.2 - 2026-09-02

> **The overlay stays where you put it.** Put it in a corner and collapse it and
> depending on the corner you got nothing out of it — it sat off-screen and
> reported nothing. On top of that, the blueprint block now survives runs of
> Smart Citizen.

> [!important]
> **Can't reach your overlay in an older release?** With "pass mouse clicks
> through" and one of the top right, bottom left or bottom right corners active
> together, the collapsed overlay slid off the screen — taking the lock with it,
> the only way back. Installing this release and restarting is enough.

### New

- **A collapsed overlay reports new blueprints.** It expands on a find and
  collapses again after the configured time. Before there was only the tone.

### Improved

- **The title bar attaches to the side matching the corner.** With the overlay
  at the bottom it sits at the bottom. Reported by Haldjas (pr0)
- **The resize handle points with an arrow to where you can drag** — and no
  longer covers the status line.
- **Switching corners takes effect at once**, in fade mode too.

### Fixed

- **The collapsed overlay sat off-screen in three corners out of four.**
  Reported by Haldjas (pr0)
- **The blueprint block survives Smart Citizen.** Anyone using both tools lost
  every marked blueprint on each of its runs.
- **The window could no longer be resized in a bottom or right-hand corner** —
  dragging worked against the screen edge.
- **Resetting without the marker file works again.** Losing it meant the
  blueprint block could never be removed from the translation file.
- **The "pass mouse clicks through" switch showed the old state** when the lock
  on the overlay was used.
- **The chosen corner only took effect when collapsing**, not at startup.
- **The blueprint list's search field would not accept typing.**
- **Two patches both showed as "4.10.0" in the filter.**
- **The test release notice linked to the wrong file.**

## v3.9.2-rc12 - 2026-09-02

The resize handle now points where there is room, no longer covers any text, and
is a proper icon instead of a typed character. And switching corners in fade mode
moves the green strip right away.

### Fixed

- ⭐ **Switching corners takes effect immediately, in fade mode too.** There the
  overlay is hidden and only the green strip is visible — but it stayed at the
  old spot until you moved the mouse over it once. It now moves along at the
  same moment, in all four corners.
- **The resize handle points the way you drag.** It was fixed on a triangle
  pointing down-right and therefore aimed at the screen edge in three corners
  out of four — where there is nothing to drag towards.
- **The resize handle no longer covers text.** In a bottom corner it sits at the
  top and lay on the status line: "405 blueprints" became "5 blueprints". That
  line now indents on the side where the handle sits.

### Changed

- **The resize handle is an icon from the icon set**, no longer a typed
  character. Typed characters look different on every system and sometimes
  ignore the configured colour; the same rule has long applied to every other
  icon in the program. There are four arrows, one per drag direction.

## v3.9.2-rc11 - 2026-09-02

The overlay can be resized again in every corner, and the green handle in fade
mode finally sits where the overlay is — vertically too. Both were consequences
of a corner having two directions while only one of them was taken into account.

### Fixed

- ⭐ **Window resizing works again in every corner.** With the overlay stuck to
  the bottom or right edge of the screen, dragging worked against that very
  edge — it could only be pulled in a direction with no room left. The edges
  that sit on the screen border now stay put and the window grows where there is
  space: upwards in a bottom corner, leftwards in a right-hand one. The resize
  handle moves to the free corner of the window accordingly, where it also stops
  covering the title bar icons.
- ⭐ **The green handle in fade mode is now correct vertically as well.** It was
  placed at the top edge of the last remembered window. Because an overlay in a
  bottom corner grows upwards, that top edge sat near the top of the screen — so
  the handle ended up halfway up instead of at the bottom. The lock beside it
  followed the same wrong path and is now aligned to the handle.

## v3.9.2-rc10 - 2026-09-02

With the overlay at the bottom of the screen, the title bar now sits at its
bottom edge too — where you expect it, instead of a whole window height above.
That settles the last point from Haldjas' feedback.

> [!important]
> **If an older release left you unable to reach your overlay, you are not
> locked out.** With "pass mouse clicks through" enabled together with the top
> right, bottom left or bottom right corner, the collapsed overlay slid off the
> screen — **taking the lock with it, which is the only way back.** Clicks went
> to the game and there was nothing left to click.
>
> From this release on that can no longer happen. If you are still stuck: quit
> the tool (tray icon), install this release and start it again — the corner is
> then calculated correctly and the lock is back.

### Changed

- ⭐ **The title bar attaches to the side that matches the chosen corner.** With
  a bottom corner it sits at the bottom edge of the window, with a top corner it
  stays on top as before. Previously it always clung to the top: place the
  overlay at the bottom and bar and lock ended up a window height above the
  screen edge — visible, but in the wrong place. The resize handle moves to the
  opposite side so it does not cover the close button. Reported by Haldjas (pr0)

### Fixed

- **The "pass mouse clicks to the game" switch now keeps up.** Click-through can
  be toggled in two places: with the lock on the overlay and with the switch on
  the "Display" page. Anyone who had that page open and used the lock kept
  seeing the old state — it only corrected itself after closing and reopening
  the page. Two displays of the same state that contradict each other are worse
  than one.
- **For four attempts the title bar rebuild was considered impossible** — it
  never was. While repacking, a collapsed window grew "from 22 to 120 pixels"
  and hung "86 pixels below the screen edge"; both numbers came from the
  window's minimum size, which stayed put when collapsing. With that fixed in
  the previous release, the rebuild worked on the first try. Six new checks
  guard the places where it could tip over.

## v3.9.2-rc9 - 2026-09-02

Put the overlay in a corner and you will find it there again — collapsed just as
much as expanded. Until now the narrow strip slid off the screen edge in three
corners out of four, leaving a green sliver that was of no use to anyone. The
handle and the lock now move to the side the chosen corner belongs to as well.

### Fixed

- ⭐ **A collapsed overlay reports new blueprints again.** With "always visible"
  selected and the bar collapsed, a find produced nothing but the signal tone —
  the window stayed put and the blueprint sat unseen in the list. With
  click-through enabled this was doubly annoying: first hit the lock, then
  expand, and all that mid-fight. The overlay now expands on its own when a find
  comes in and collapses again after the configured time. Anyone who prefers to
  work collapsed stays that way — the state is not overwritten, and it stays
  open as long as the mouse pointer rests on it.
- ⭐ **The collapsed overlay stays on screen in all four corners.** In the top
  right, bottom left and bottom right corners it sat partly or entirely outside
  the screen — 252 pixels off to the side or 86 off the bottom, depending on the
  corner. The cause was not the corner calculation but the window's minimum
  size: it held the window at full width and height while the position had
  already been worked out for the narrow strip. The two now move together.
  Reported by Haldjas (pr0)
- **The fade-mode handle sits on the chosen side.** The green strip that brings
  a hidden overlay back was always stuck in the middle regardless of the
  setting — with a bottom left corner it therefore sat in the middle of the
  screen. It now follows the corner, and the lock goes the same way: with a
  right-hand corner it sits to the left of the strip so it does not slide off
  the edge itself. Reported by Haldjas (pr0)
- **Missing spaces on the thanks page** — it read "setup andupdating" instead of
  "setup and updating".

### Changed

- **A check for the corners** is included: it builds the overlay invisibly,
  collapses and expands it in every corner and compares the actual position with
  the screen edge. This very bug would have surfaced on the first attempt
  instead of surviving several releases.

## v3.9.2-rc8 - 2026-09-02

### Fixed

- ⭐ **The blueprint block now survives other tools.** If you also run
  **Smart Citizen** alongside the watcher, every run of it wiped the blueprint
  entries — measured against the real translation file: **398 out of 398**
  affected contract texts. The cause is not that tool: before each run it
  clears its own text block by locating where it starts and discarding
  everything from there on — and our block sat behind it. It is now inserted
  **in front** and therefore stays. What gets recognised is the **shape** of
  such blocks, not their name, so a rename on the other side breaks nothing.
- **Restoring without the memo file works again.** Anyone who applied the
  injection on one machine and lost the memo file (fresh install, tidied
  folder) could never get the blueprint block back out of the translation file
  — the fallback detection did not match it at all. This bug had been in from
  the very first release and only surfaced through the new check.
- **The link for a test build pointed at the wrong file.** The version
  announcement always linked to the newest *stable* release — with a
  pre-release you ended up at the stable one before it and never saw the test
  build at all.

## v3.9.2-rc7 - 2026-09-02

### Fixed

- **The chosen corner is now applied at startup.** Until now it only took
  effect when collapsing or expanding — set a corner, restart the tool, and the
  overlay reappeared wherever it last *stood*.

### Reverted

- **The attempt to move the title bar to the bottom edge for the bottom
  corners (rc3–rc6) has been rolled back.** It failed against Tk: the required
  repacking makes the window size recalculate, so a **collapsed** window grew
  from 22 to 120 pixels and hung below the screen edge — taking the bar with
  it. And while collapsed, the bar is the only way to operate the tool: once
  it's gone, nothing is reachable, not even the setting you would need to undo
  it. Four attempts, all measured, all failed. The request stays valid and will
  be done on a machine where the overlay can be watched in real use. A
  self-test check pins the rollback down so the next attempt measures the
  collapsed state too.

## v3.9.2-rc6 - 2026-09-02

### Fixed

- **The blueprint list's search box no longer accepted any typing.** Clicking
  looked like it worked, but no text arrived and the caret was missing. The
  cause was the change from rc4: while pre-building pages in the background,
  the search box calls `focus_set()` — just as it does when opened normally —
  and pulled the input focus onto an **invisible** page. Pre-building now
  hands it back.
  ⚠️ A self-inflicted bug born from an improvement: what is right when showing
  a page is wrong when building it in the background.

## v3.9.2-rc5 - 2026-09-02

### Fixed

- **Two patches were both labelled "4.10.0" in the filter.**
  `4.10.0-live.12519617` and `4.10.0-live.12545750` shorten to the same
  number — the dropdown then showed two identically labelled entries with
  different counts, "4.10.0 (34)" and "4.10.0 (24)", with no way to tell
  them apart. The full version is now shown whenever the short form appears
  twice. The same fix had already landed **in the report** in v3.9.1 but not
  in the menu — the self-test now covers both places.
  ⚠️ Why there are two 4.10.0: a hotfix went into the live channel. Values on
  existing blueprints change, the data source picks them up again — so they
  carry the new patch's stamp although the game has had them for a while.

### Changed

- **The filter "can close" is now called "lost when ranking up"** — and
  explains itself. Nobody understood the old name, and a three-word button
  cannot explain a game mechanic that few players know about. A warning line
  now sits **above the list** whenever the filter is on: "These blueprints
  only come from contracts that disappear once your reputation is too high."
  That answers the "why?" where it comes up, instead of in a tooltip nobody
  finds.

## v3.9.2-rc4 - 2026-09-02

### Changed

- **The window accepts clicks right away, even the first time a page opens.**
  Each page used to be built only when clicked — which takes up to a second
  with nothing responding. The program's own startup trace says it plainly:
  `Seite wasistneu: bauen beginnt` at 00:51:48, `steht` at 00:51:49. The
  remaining pages are now built during idle time once the first one is up, one
  after another so the interface stays responsive in between.
  ⚠️ This is **not a speed-up**: the same work still happens, just before
  anyone is waiting for it.

## v3.9.2-rc3 - 2026-09-02

### Fixed

- **The title bar now really moves into the chosen corner.** Until now the
  window went there but the bar stayed at its top edge — with a bottom corner
  it therefore sat a full window height above the screen edge. It now attaches
  to the bottom edge for the bottom corners, together with the title, the lock
  and the close button.
  Reported by **Haldjas (pr0)** — twice, because the first attempt missed the
  actual point and only moved the lock along.

## v3.9.2-rc2 - 2026-09-02

A test build: the selectable corner now takes the lock with it, plus two
small wording fixes.

### Fixed

- **The lock stayed in the old corner.** When a corner is chosen for the
  overlay, the window moves there — but the lock stayed where it was. It showed
  most clearly while collapsed: the narrow strip sat in the corner, the lock
  somewhere in the middle of the screen.
  The reason: the lock is a **separate window** placed exactly on top of the
  lock in the title bar (necessary because a click-through overlay no longer
  accepts its own buttons either). Until now it was only repositioned on moves
  and resizes — not when collapsing, and not when switching corners. Both now
  do it.
  Reported by **Haldjas (pr0)**, with a screenshot showing where it belongs.
- **Singular in the report.** It said "1 blueprints from them" and "1 logs".
  The report is what users send in — a wrong plural in it looks careless.

### Changed

- **The box on the update page is now called "Test version · for testing".**
  Previously "Test versions too" — as the only box without a suffix it read
  like an afterthought rather than a choice. Now it matches "Stable version ·
  recommended" next to it.

## v3.9.1 - 2026-09-01

Three things the diagnostic report itself brought to light. Two of them were in
the report: it showed figures that could not be matched up, or claimed a wrong
origin — sending the very troubleshooting it exists for down the wrong path. The
third sat deeper and could do real damage.

### Fixed

- **The patch history in the report could not be matched up.** Two game
  versions sharing a number — `4.10.0-live.12519617` and
  `4.10.0-live.12545750` — both showed up as "4.10.0". The report then read
  `4.10.0 (24), 4.10.0 (34)`, with no way to tell which count belonged to which
  patch. Of all lines, this was the one added for exactly that purpose, after a
  bug had hidden there for three weeks. The short form is now used only while
  it is unambiguous — otherwise the full version is shown.

- **The report claimed a wrong origin for the search phrases.** A single source
  was printed behind the whole list — "from the game's global.ini" — even
  though the list is mixed: phrases confirmed from the language files, plus the
  built-in fallback table. Looking for one of the others in `global.ini` is a
  dead end; "Bauplan überchoo", for instance, is Swiss German from the table
  and cannot appear there at all. Each group is now labelled separately.

- ⭐ **A single unreadable log threw away the entire catch-up run.** If reading
  stumbled on one file — or the running `Game.log` dropped out at the last
  step, say because a drive went away — the read state was **never saved**.
  Every log read during that run counted as unread again, and the next start
  began all over: silently, with no error, every single time. Measured against
  the old build: **0 of 23** logs recorded. Now that one file is skipped and
  counted, the rest is recorded as usual, and the skipped one stays pending for
  the next run instead of counting as done.

## v3.9.0 - 2026-08-31

The contract bar now shows only what is actually running — logging out drops
your contracts, and the tool knows that now. And "What pays off most?" no
longer stops at a number: one click on a contract shows which blueprints are
behind it, and where to pick it up.

### Fixed

- ⭐⭐ **Contracts from two days ago showed up as "active".** Reported on
  2026-08-31: Star Citizen had not even been launched, and the bar read
  "Welcome to the system". The cause is a gap in the log — **on leaving the
  game world, the game reports no contract ending at all.** Log out and your
  contracts are gone silently; the tool only listened for endings and so kept
  books on a state the game had long since dropped. Dismissing the line did not
  help: on the next start it was back, recomputed from the same log.

  The tool now also reads the marker the game writes on leaving — language
  neutral, and it covers both cases: back to the main menu and quitting the
  game.

  ⚠ This is **not** the blanket clearing from v3.4.4. Back then, an ending that
  could not be matched to a contract wiped the list — a guess, in other words.
  Here the game itself says the player is out. Measured across 23 logs: 39
  markers, 19 acceptances, 3 real endings, 87 objectives — **not a single
  contract survived a logout.** Contracts accepted after the last logout stay
  put.

- **"Nonsense" was reported as a system error on Wayland.** When registering a
  shortcut, the tool checked the system before the input. On Wayland the input
  check was therefore never reached: a typo got you "not available on Wayland",
  even though what you had typed was not a valid combination in the first
  place. The input is checked first now.

### Changed

- ⭐⭐ **"What pays off most?" now answers both follow-up questions.** The page
  named a contract and a number — and left you there. Where do you pick it up?
  And **which** blueprints are those, anyway?

  Both had been there all along, just never connected: the pickup location came
  with the data from the start and was thrown away, and the blueprint list has
  always been able to filter on a single contract — you just had to type the
  name into the search box by hand and then hit the contract row.

  The location now sits under every row ("Pick up in Stanton: Hurston, Arial,
  Aberdeen, Magda and 12 more"), and **clicking the row opens the blueprint
  list filtered to that exact contract** — every single blueprint in it, ticked
  off for the ones you already own.

- ⭐ **The number is labelled reward pool now, not payout.** It used to read
  "44" next to a claim that the contract pays "the most in one go". That
  promised more than the data holds: the 44 are the pool of a **mission type**
  — that many different blueprints can drop from it, not that many land in your
  hands per run. The blueprint window had this same claim toned down after a
  report from Morkhan; this page still carried it. The number stays, it is
  correct — it is just labelled as what it is now.

## v3.8.1 - 2026-08-31

Ctrl+Alt+B really does bring the blueprint list to the front now. The shortcut
has been there since v3.7.0 — on Windows it never once worked, on any machine.
If you want to know mid-game whether you already have a blueprint, that is two
keys now, instead of tabbing out and hunting for the window blind.

### Fixed

- ⭐⭐ **Ctrl+Alt+B did not bring the blueprint list to the front.** The shortcut
  was registered with the system correctly — the key press simply never reached
  the program, on every Windows machine, right from the start.

  Windows delivers a shortcut registered this way as a **thread message** to
  exactly the part of the program that registered it. Until now that was the
  same part that draws the window — and it clears its own messages before the
  300-millisecond poll takes a look. By then the press was long gone. Measured:
  without a window 3 out of 3 arrived, with a window 0 out of 3.

  A dedicated part of the program now waits for the press, and nothing clears
  its messages behind its back. Nothing changes about the essentials: exactly
  **one** combination is registered, and nothing is listened in on.

## v3.8.0 - 2026-08-31

The overlay can finally be put where you want it — and collapsed it is actually
small. Reported by **Haldjas (pr0)**.

### New

- ⭐⭐ **A screen corner can be chosen** (Display → "Where the overlay sits"):
  top left, top right, bottom left, bottom right — or free to move as before.

  ⚠ **In pop-up mode this is the only way.** There the overlay passes mouse
  clicks through so it does not get in the way during a fight — and what passes
  clicks through cannot be dragged either. Those users could not position the
  overlay **at all** until now.

  ⚠ It is calculated on the screen the window is **currently on** — with three
  monitors side by side, "top right" would otherwise always be the left one.

### Fixed

- ⭐ **Collapsed, the overlay stayed as wide as when open.** Only the height
  shrank; at 1160 pixels a bar remained stretched across half the screen, which
  fits into no corner. Reported: "the bar sits centred on the watcher window."

  Now the width shrinks too — to what the title bar actually needs. ⚠ Measured,
  not guessed: a fixed value would be wrong at a different font size and in the
  other language. Expanding restores the old width.

## v3.7.0 - 2026-08-31

A keyboard shortcut that works inside the game. Plus the white title bar — it
was not really gone in v3.6.0.

### New

- ⭐⭐ **Ctrl+Alt+B brings the blueprint list to the front — from inside the
  game.**

  Star Citizen runs full screen and hides the mouse pointer: to check whether
  you already have a blueprint you had to alt-tab and then hunt for the window
  **blind**. User request, 2026-08-31.

  Configurable under **Display**. Ctrl, Alt and Shift combine with a letter, a
  digit or F1 to F12.

  ⚠ **Nothing is being listened to.** Exactly **one** combination is
  registered, and the system only wakes the tool for that one. It never sees
  anything else — no logging, no access to what you type in the game. That is
  the difference from a keyboard hook, and the reason only this route was
  considered.

  ⚠ **A modifier is required.** Claiming a bare key system-wide would make it
  useless in the game.

  ⚠ **On Wayland no program can do this** — the system does not allow it, for
  good reason. Instead of a dead input field you get the explanation and the
  route via your desktop's own shortcut settings.

- ⭐ **Recipe properties now read in German** when German is selected — all 24
  of them. Reported: "some people do not speak English and do not understand
  this, and report that it does not help them."

  ⚠ Translated via the **language-neutral key**, not the English text —
  otherwise half of it would quietly fall back to English on the next patch.
  Anything not yet covered stays as the game names it.

### Fixed

- ⭐ **The title bar was still white.** In v3.6.0 Windows reported "setting
  applied" — and did not redraw the frame anyway. On top of that, the attempt
  **before** the first display went nowhere, because the window handle does not
  exist yet at that point. Both measured and fixed.

- ⭐ **You can now see which blueprint is open in Crafting.** Reported: "not
  clear enough which blueprint is selected, it is not stated anywhere." The row
  stands out **and** the name is repeated above the recipe — the box is long,
  and by the time you have scrolled to the ingredients the row is gone.

### Changed

- **The folder under "Paths" is now called "Folder for your data"** and says
  what lives in it: blueprint inventory, watchlist, **workshop stock, trade
  stock**, settings and exported files. Plus the sentence that was missing:
  point both machines at the same folder and both work from the same state.
  ⚠ Switching only changes the folder, it does not copy.

## v3.6.0 - 2026-08-31

From crafting straight to the blueprint — and the way there can finally be
found. Both requested by **Bushwick4712 (KRT)**. Plus the dark title bar.

### New

- ⭐⭐ **"Where do I get the blueprint?" — a button in Crafting.**

  You open a recipe, you are missing the blueprint for it, and the next
  question is always the same: *which mission do I have to run?* The answer was
  already in the tool — but on another page. You had to know it existed and
  retype the name by hand.

  Now it is one click: the blueprint list jumps to exactly that blueprint with
  its origin opened — faction, mission, required reputation, reward.

  ⚠ **Only where it leads somewhere.** The button appears only when you are
  missing the blueprint **and** the catalogue knows where to get it. The
  catalogue holds 738 blueprints, the recipes number 1,607 — a button onto an
  empty list would be worse than none. If the blueprint is unexpectedly not
  found, the list stays as it was instead of jumping to nothing.

### Changed

- ⭐ **The origin button in the blueprint list now reads "Where from?".**
  It used to be a symbol at the right edge of the row, without a word —
  Bushwick simply did not find it. A symbol explains itself only to whoever
  built it.

- ⭐ **The title bar is dark** (Windows). The window was fully dark inside and
  carried a **white** bar with the title and the three buttons on top. That bar
  belongs to Windows, not to the program — anyone running the light system
  theme got it light, however dark the content.

  ⚠ On Linux the window manager handles this; nothing changes there. And if
  Windows will not do it, the bar stays light — ugly, but no reason to crash
  while building a window.

## v3.5.3 - 2026-08-31

The result of "Read the logs again" was lost in the status line.

### Changed

- ⭐ **The result now arrives as a window, not as a line.**

  ```
  Read the logs again
  152 logs read again, 2 blueprints added.
                                              [ Got it ]
  ```

  ⚠ **The status line was the wrong place.** It shows a line for four seconds
  and is then empty again — and during those four seconds nobody is looking
  there who has just started a run across hundreds of logs. You pressed the
  button and you are waiting. Reported on 2026-08-31: "in the bar it is there
  too briefly or not at all".

  ⚠ **The bar still gets it.** The button also exists on the overlay; with the
  main window closed there is no window for a dialog to sit above. Then the
  line remains the way. The result is never swallowed.

  ⚠ **A window for a result only**, not for every message. A tool that keeps
  throwing up windows gets clicked away unread. Blueprint finds stay where they
  are.

## v3.5.2 - 2026-08-31

Two buttons for the same job — one of them could do less and was red on top of
that, although it cannot break anything. Reported by **Haldjas**.

### Changed

- ⭐ **"Read the logs again" is no longer red.** The button cannot break
  anything: it **adds**, nothing else. Nothing is removed, nothing overwritten,
  and duplicates cannot happen. The worst case is "takes a moment".

  ⚠ **And that was the point.** Right below it sits "Reset inventory" — which
  really does delete. With both red, red only said "something important"
  instead of "this will be gone". Exactly what happened on 2026-08-31: Haldjas
  pressed the harmless one and needed a shout afterwards. **Red is now reserved
  for what actually takes something away.**

- ⭐ **The second "read the logs again" button under "Detection" is gone.**
  There were two, and they were not equals:

  | Where | What it did |
  |---|---|
  | Detection → "Read from the start" | took effect **on the next start** |
  | Inventory → "Read the logs again" | takes effect **now** and reports what came of it |

  The second does everything the first did: it ignores the read position just
  the same and goes through every kept session **and** the running `Game.log` —
  only without a restart and with feedback. Haldjas: "the former is probably
  not that useful any more then?" He was right.

  ⚠ Two buttons for one job are worse than one: whoever hits the weaker one
  concludes the tool cannot do it.

## v3.5.1 - 2026-08-31

"Reset inventory" did nothing at all for some people — and did not say why.
And the bug report now answers for itself the question you would otherwise have
to ask back: does log detection work at all?

### New

- ⭐ **The report now says for itself whether log detection works.** The "Kept
  logs" line carries three numbers instead of one:

  ```
  Kept logs   462 logs · 462 read · 0 blueprints from them
  ```

  ⚠ **Because asking back is often impossible.** The report behind this release
  arrived with no sender and no message — just "462 logs" and "0 blueprints".
  Whether detection fails for that person or they are simply new to the game
  was **not** visible. Yet that is the difference between "all fine" and "the
  tool is worthless to them".

  | What it says | What it means |
  |---|---|
  | 462 · 462 read · **0** from them | detection finds nothing |
  | 462 · **0** read · 0 from them | the catch-up never ran |
  | 462 · 462 read · 380 from them | all fine |

  Only finds **from logs** are counted. What came from the launcher, by hand or
  from the starter blueprints says nothing about log detection.


### Fixed

- ⭐ **The "Reset inventory" button stayed silent.** Red button, warning
  confirmed — and then nothing happened. No tick, no message, no error.
  Indistinguishable from a broken button.

  Cause: the tool deleted the inventory file without allowing for the case that
  **there is none**. The error went quietly into the diagnostics.

  ⚠ That does not hit the edge case, it hits the beginning: **anyone without a
  single blueprint has no inventory file either.** Exactly as in the report —
  "Inventory 0 blueprints". And anyone who presses twice.

  "Already gone" now counts as what it is: the desired result. The button
  reports the same thing either way.

- ⚠ **And when it really does fail, it says so on screen.** No permission, file
  locked — that used to live in the diagnostics only, where it is found by
  those who know it exists. The status line says it now.

## v3.5.0 - 2026-08-31

Every running contract now shows **what to do next**. And the contract itself no
longer vanishes while it is still running in the game — that was a side effect
of v3.4.4 and is fixed, along with the wrong assumption behind it.

### New

- ⭐⭐ **Open objectives are listed under their contract.**

  ```
  Active contracts (per log)
  Contract accepted: Retake Platforms From Nine Tails  →  3 blueprints
     ◆ Disable the Hartmoore inverter
     ◆ Locate and reset the node
  ```

  The contract tells you whether blueprints are in it. The objective tells you
  what you are flying for right now. Both are in the log — only one half was
  being used. Finish an objective and it drops off, the next one moves up.

  Where the data comes from, deliberately kept apart:

  | | Source | language-neutral? |
  |---|---|---|
  | State (running / done / gone) | `<ObjectiveUpserted> … state …` | yes |
  | Wording | the in-game notification, matched by `ObjectiveId` | no |

  ⚠ **State never comes from the wording.** In German the objective
  notification reads "Neuer Auftrag" — word for word the same as a contract
  notification. Go by that and you count objectives as contracts.

  ⚠ **Only what the game itself writes into the contract log.** A contract also
  runs a pile of internal objectives: counters, triggers, zone watchers.
  Measured across all 153 logs: of 2832 objectives, 456 carry the `ShowInLog`
  flag but no wording — **not one** has wording without that flag. No wording
  means no line, rather than a guess.

  ⚠ **Six lines at most**, the rest is counted. Measured, a contract almost
  always has exactly **one** open objective (182 of 226); the outlier had six.
  The cap only catches the unknown case — the overlay must not push the
  blueprint list off screen. A truncated list that passes itself off as
  complete would be worse than none.

### Fixed

- ⭐⭐ **The running contract was gone.** Since v3.4.4 every ending that could
  not be matched to a contract wiped the whole list. Reported on 2026-08-31 with
  a screenshot: "Retake Platforms From Nine Tails" was plainly visible in the
  game, the contract bar was empty.

  The assumption behind v3.4.4 was wrong. It claimed Star Citizen reports the
  active objective instead of the contract when you withdraw. In fact **the game
  reports both — on two separate levels.** Every notification carries a
  `MissionId` and an `ObjectiveId` at the end of the line:

  ```
  "Contract Accepted: Retake Platforms From Nine Tails: "
      MissionId: [916223dd…]  ObjectiveId: []
  "Contract Withdrawn: Reach the upper platform: "
      MissionId: [916223dd…]  ObjectiveId: [40418b42…]
  ```

  The second line drops **an objective**, not the contract — the log shows the
  next objective right after it. v3.4.4 read such lines as contract endings and
  cleared the list.

  An ending now only counts as a contract ending when no `ObjectiveId` is
  present. Measured across all 153 logs: of 473 endings, **111** are mere
  objectives, and in all 111 cases the mission demonstrably continued.

- ⚠ **A genuinely aborted contract still disappears** — the case v3.4.4 was
  written for (reported by Morkhan, KRT). When the ending title does not match
  the accepted title, the `MissionId` decides. It is present on **all** 1102
  measured acceptances and on **all** 362 real contract endings.

  ⚠ The v3.4.4 measurement ("no mission id on acceptance in 26 of 28 cases") was
  a measurement error — it only searched the notification text, not the end of
  the line, where the id actually sits.

  Result: **not one** of the 362 contract endings stays unmatched. So there is
  no need to guess and none to wipe — both are gone.

## v3.4.5 - 2026-08-31

An accepted contract appeared twice in the overlay — once in the contract bar
and word for word again below it. It now appears once.

### Fixed

- ⚠ **The same contract was shown twice.** The contract bar ("Running
  contracts") and the note line below it carried the same sentence. Both
  messages were correct on their own — only together did they duplicate, and in
  the source that was invisible.

  If a contract is already in the bar, the note line is dropped. Without the bar
  — or after the contract has been dismissed there — it still appears.

## v3.4.4 - 2026-08-31

A withdrawn contract finally goes away — until now it came back as running
after every start. And switching to the blueprint list is instant again instead
of taking half a second.

### Fixed

- ⭐⭐ **A withdrawn contract came back after every start.** Withdraw a contract,
  and the tool showed it as running again on the next start — and the one after
  that.

  The reason is in the game: **when you withdraw, Star Citizen reports the
  active objective, not the contract.** You accept "Secure Our Airspace", you
  withdraw "reach the outer area of an asteroid base and find target". Measured
  across 152 logs: of 112 withdrawals, exactly **two** carry a title that also
  appears as an acceptance. So the watcher found nothing to strike out.

  An ending that cannot be matched to an open contract now clears the list. The
  next accepted contract shows up again immediately.

  ⚠ **Deliberately not guessed.** The obvious move would be to strike the most
  recently accepted contract — but the numbers do not support it: with an
  unmatched ending, only 36 of 172 cases had exactly one contract open, mostly
  it was three to eight. That would drop a contract you still have and keep the
  withdrawn one. The mission id does not help either: it is in the log at the
  end, but not at acceptance in 26 of 28 cases. Better one line short than one
  line wrong.

  Reported by **Morkhan (KRT)**.

- **Two missing spaces on the credits page.** It read "launching Star Citizen
  from thetool" and "797 blueprints nobodyever got to see".

### Changed

- ⭐ **Switching to the blueprint list is instant again.** It measured **642 ms**
  even though the page had long been built — it is **0.4 ms** now.

  The culprit was the routine that resets the filters when the page is shown
  again: it redrew all 738 rows every time, even with no filter set. Same on the
  crafting page with its 1597 rows. It now only resets when something actually
  was set — with a filter, everything happens exactly as before.

  ⚠ What remains: opening a large page for the first time still takes its time,
  and the window system re-renders the many rows when the page is shown. That is
  sheer volume, not a bug.

### Thanks

- **Morkhan (KRT)** — for the withdrawn contract that would not go away.

## v3.4.3 - 2026-08-31

The cargo hold shows its table again. In v3.4.2 the page only built the form —
the list of your goods, the total and the per-entry delete were missing. If you
are on v3.4.2, grab this build.

### Fixed

- ⚠ **The cargo hold lost its table.** Since v3.4.2 the page only built the
  form; the list of entered goods, the total and the per-entry delete were
  missing. The cause was a name clash Python resolves silently: two functions
  were called `_leeren` — one clears a frame, the other (newly added) empties
  the whole storage. The later one wins, and building the list died with a type
  error.

## v3.4.2 - 2026-08-31

Drag the window to the size you need once — it will be there again next time.
And the guide finally shows what the tool does today: all screenshots are new,
with the workshop and the trading section instead of last week's interface.

### Changed

- ⭐ **The window keeps the size you set.** Anyone who dragged it larger —
  because long lists do not fit otherwise — found it back at the minimum size
  on the next start and resized it every single time. The size is now
  remembered.

  The **minimum size is unchanged**, and only the *size* is remembered, not the
  position: a stored position points into nowhere on a different machine, so
  the window still opens centred. A size from the big screen is capped to the
  smaller one.

## v3.4.1 - 2026-08-31

Your cargo hold can now be backed up — and after a patch that wipes all goods,
cleared with one click instead of entry by entry. On top of that, the project
now speaks German first: the guide, the changelog and this announcement lead in
German, with English one click below.

> [!important]
> **The documentation files were renamed.** `README.md`, `CHANGELOG.md` and
> `ROADMAP.md` are now the **German** versions; the English ones sit beside them
> as `README.en.md`, `CHANGELOG.en.md` and `ROADMAP.en.md`. Bookmarks pointing
> at `README.de.md` no longer resolve — go via the project page once.

### Added

- ⭐ **The trade storage can now be backed up, restored and cleared in one go** —
  the same four controls the workshop storage already had, in the same place
  and with the same words: *As backup (.json)*, *As spreadsheet (.csv)*, *Load
  backup* and *Clear stock* in red.

  The reason is the same as over there: the trade storage is hand-typed work
  that exists nowhere else. And after a patch that wipes all cargo, the hold is
  empty in the game but still full in the tool. Nobody deletes entries one by
  one, so a wrong storage stayed put and the selling maths lied. One click now
  clears it, after a confirmation that names the number of entries.

  The spreadsheet lists commodity, amount, the *stolen* mark and the storage
  location — semicolon and comma, the way a German spreadsheet program expects
  them.

  ⚠ **A backup from the other storage is rejected.** Both files look the same
  from the outside; without this check the trade storage would have accepted a
  workshop backup happily, discarded everything in it and saved an **empty**
  storage — reporting “0 entries loaded”. It now says which backup belongs
  where instead.

### Changed

- **The refinery yield is collapsed until you need it.** It was the longest
  block on the storage page — unit, location, a seven-line typing field,
  preview and button. Anyone just adding a single entry by hand scrolled past
  all of it, with their own storage list out of sight below. One click on the
  heading opens it; the state is remembered, so whoever types up every refinery
  run finds it open.

- ⭐ **German is the project's primary language.** This covers everything you
  see on GitHub: the guide, the changelog, the outlook, the release notes
  (German on top, English one click below) and the project page's info box.
  **English stays fully maintained** — it simply no longer comes first.

- **The security policy and the code of conduct are finally available in
  German.** Both existed in English **only**. In a project whose primary
  language is German, the security page is the wrong place to economise.

- **The footer of the guide now matches the other projects.** The author block
  used to sit in the middle of the text; it now stands at the bottom, centred,
  with a picture — and the Ko-fi link is where someone arrives who has read to
  the end.

- Safe writing now lives in **one** place, `pfade.json_sichern()`, instead of
  being rebuilt in every module. Two copies of the same rule drift apart
  eventually — which is exactly what had happened here.

### Fixed

- **Both storages now keep a previous version.** The workshop storage and the
  trade storage already wrote atomically — to a side file first, then rename —
  but **without a fallback**: a storage accidentally emptied or corrupted was
  gone for good. The blueprint inventory had this safeguard from the start, the
  two storages never did. They now produce `rohstoffe.bak.json` and
  `handelslager.bak.json`, just like `bestand.bak.json`.

  It matters more here than for the inventory: unlocked blueprints can be
  rebuilt from `Game.log`, stored cargo cannot — those are hand-typed entries
  that exist nowhere else.

- **The interface check never built half the application.** Six pages were
  missing from its page list — the entire workshop (crafting, mining, my stock)
  and the entire trade section (selling, trade storage). It still reported
  “no German text in the English interface” every time. It now visits all
  eighteen and flags a new page that gets added without being listed.

## v3.4.0 - 2026-08-30

Cargo hold full — now what? The new **Trading** section tells you where to
offload your goods and what they pay per SCU. Several commodities at once,
sorted by how many of them a place actually takes: one stop usually beats three.

Also in: a separate hold for trade goods, the **"Can close"** filter for
blueprints you might lock yourself out of, and refinery yields typed in one go
instead of across 24 fields.

### Added
- ⭐⭐ **"Can close" — the filter for what you quietly lock yourself out of.**
  280 of the 353 contracts have a **reputation cap**: rank up past it with that
  faction and they are no longer offered — and their blueprints are gone for
  that save. The game says nothing about it.

  The new filter in the blueprint list shows exactly the blueprints you are
  missing that are **only** available from such contracts. In a real inventory
  that was **199 of 738**.

  For every affected blueprint the limit now also appears under its origin:
  *"⚠ Closes at Elite Contractor (95,250 reputation)"*.

  ⚠ **One open route is enough.** If five contracts lead to a blueprint and one
  of them has no cap, there is no warning — otherwise it would appear everywhere
  and nobody would take it seriously.

  ⚠ **What the tool does NOT say: how far away you are.** Your own standing is
  not in the `Game.log` — measured across 22 logs, reputation appears there only
  as a connection line to CIG's service. So it says "closes at", not "you have
  4,200 left".

- ⭐ **Enter a whole refinery yield at once.** Under "My stock" there is now a
  field where you type the rows exactly as they appear in game:

  ```
  Titanium 295 188
  Aslarite 287 8
  Heart of the Woods 500 12
  ```

  As you type, the tool works out what would go in; broken lines are listed
  individually with the reason. One button, all entries added — the storage
  location applies to all. Six entries used to be **24 inputs** through the form
  above; now they are six lines.

  **Unit switchable: cSCU or SCU.** The refinery terminal counts in cSCU
  ("GEWONNENE MATERIALIEN (cSCU)"), the inventory tooltip in SCU (`0.889 SCU`).
  Both screens can be typed off this way. Cross-checked: 272 cSCU from the
  terminal are the same 2.728 SCU as the seven stacks in the inventory.

  **And the amount field itself now speaks cSCU.** A checkbox sits right of it:
  tick it and the row reads "Amount (cSCU)" — then you type the number straight
  off the refinery screen without dividing by 100. Unticked it stays SCU, the
  way the inventory tooltip shows it.

  That is the more convenient route: at the refinery terminal all rows are
  listed below each other, in the inventory you have to hover every stack with
  the mouse. The setting is remembered, and the label always states which unit
  applies — otherwise every entry would silently be off by a factor of 100.

  **The storage location stays** — after adding entries and across restarts.
  Whoever enters a yield enters six items at the same place; picking it again
  every time was pure typing.

  **A mistyped name shows the most likely match next to it** — "Aslerite" is
  not a resource. Did you mean: Aslarite? Nothing is matched automatically
  though; the decision stays with the human.

  ⚠ **Automatic is not possible, and that is measured, not assumed.** The
  refinery job is **not** in the `Game.log` — checked across 22 logs:
  `Refinery` appears 58 times, only as a load line for the deck's 3D models;
  `Aslarite`, `Agricium` and `cSCU` **not once**. Image recognition would need
  extra packages and is therefore out.

- ⭐ **"What pays off most?" — the next sensible step, below your progress.**
  The percentage tells you where you stand, not what moves you forward. Below it
  are now the ten contracts you are still missing the most blueprints from —
  with faction, payout and required rank.

  The top one gives **44 missing blueprints in one go** in a real inventory.
  Calculated on data that is loaded anyway.

- **Two details on every blueprint that exist nowhere else:**

  | | |
  |---|---|
  | 👥 **Shareable in a group** | "you can run this as five, everyone gets the blueprints". ⚠ Only shown when **all** routes are shareable — otherwise the group would line up at the wrong contract |
  | ⏱ **Repeat lockout** | "Available again after 2 h 30 min". Values from one minute to a week; the shortest is shown |

  Both come from CIG's own contract data (`canBeShared`,
  `personalCooldownTime`) and were only visible in the raw files until now.


- ⭐⭐ **Trading — two new tabs: "Cargo hold" and "Selling".**
  The question that was missing: *where do I offload my cargo, and what does it
  pay per SCU?*

  **The selling tab** answers it for **several commodities at once**. It sorts
  not by the highest price, but by **how many of your goods a place actually
  takes** — because that is the difference that matters. Measured on 30 Aug
  2026 for 100 SCU gold, 40 copper and 25 iron:

  | Route | Revenue |
  |---|---|
  | everything at **one** place | 3,533,000 aUEC |
  | each commodity at its own best place | 3,566,000 aUEC |

  **One percent more for two extra approaches.** The known trading sites do not
  give that answer, because they only ever look at one commodity.

  **The cargo hold** is deliberately kept apart from the workshop stock: one is
  building material you keep, the other is cargo you want gone. A button in the
  selling tab pulls the whole hold into the selection.

  Both lists are proper tables — commodity · location · SCU · price per SCU ·
  total, figures right-aligned below each other.

- **Stolen cargo.** Instead of a quality (which changes nothing when selling,
  and looted cargo is always Q 0 anyway) the cargo hold has a *"marked as
  stolen"* tick. The selling tab then narrows down to the **15 terminals** that
  ask no questions (`is_nqa` at UEX) — seven of them with buy offers.

- **Refresh prices yourself.** A button fetches prices right away instead of
  waiting for the daily update — **once per hour**. While the lock is running
  the button counts down itself and changes colour as it goes (grey → gold →
  green). No red: the button is locked *because* the fetch succeeded.

- **The amount field does maths** — `100+5` makes 105. Click a row and its
  amount appears in the field, ready to be adjusted with `+5` or `−12`. Same
  behaviour as in the workshop stock.

### Changed
- **Commodity and storage location come from closed lists** — as in the
  workshop stock. Only what UEX knows can be entered; near misses get
  suggestions.

- `preise.py` used to rule out "prices per terminal" explicitly ("another 2.1 MB
  of data and a different tool"). That was no longer true: the full pull is
  1.04 MB, and 293 KB once tidied up. The file header now says where the line
  actually runs.

- ⭐ **Sidebar groups can be collapsed** — Blueprints, Workshop, Trading,
  Settings, Info. One click on the heading; the state is remembered until the
  next start.

  This is the third lever against window height: collapsing Workshop, Trading
  and Settings cuts the sidebar's space requirement from **1020 to 696
  pixels**, and the window's minimum height follows. Suggested by
  **Morkhan (KRT)**.

  ⚠ Opening a tab from a collapsed group expands it automatically — otherwise
  you would stand on a page whose entry is nowhere to be seen.

- **The sidebar now looks the same throughout.** "Advanced" carries the same
  collapse arrow as the groups and sits inside the "Settings" group instead of
  clinging to the bottom on its own — behind it are paths, detection and the
  blueprint stock, things you set. All collapse arrows use the same icon as
  the rest of the program — previously they were text characters that looked
  different depending on the system font.

- **"Launch Star Citizen", Coffee and Discord are pinned to the foot of the
  sidebar** and no longer scroll away. The sidebar also has a visible scrollbar:
  without it an expanded group looked empty whenever its entries sat below the
  window edge.

- ⭐ **"Blueprint stock" now sits behind "Advanced".** The page writes to your
  own stock — reading in, overwriting, resetting — yet stood among harmless
  settings and got clicked in passing. It stays reachable, just not by accident.

- **The "Read the logs again" button is red.** It starts a run across hundreds
  of logs and writes to the blueprint stock while doing so. Red **permanently**,
  not only on hover — a button that warns once the mouse is already on it warns
  nobody. Both found by **Morkhan (KRT)** after pressing it by accident.

- **"My stock" now works like the cargo hold.** Resource and storage location
  are dropdown fields: type **or** click the arrow and pick. Labels sit above
  the fields rather than beside them, so an expanded list shifts nothing. Same
  handling in both places — learn one, know the other.

- ⭐⭐ **The refinery yield lost its storage location.** Anyone who had picked
  "Levski" got the whole yield booked in **without a location**, and therefore as
  separate stacks next to the existing stock. Cause: the location name was run
  through a function that maps input onto a known **resource**; a location name
  is never in that list, so nothing came back.

  The block now also has its own **"Storage location for this yield"** field.
  Previously the location from the form further up applied silently — neither
  visible nor changeable without scrolling back.

- **"Please download the new version yourself" appeared at the wrong moment.**
  Clicking "get" while GitHub is still building the files sent you to the
  releases page — where they are not yet either. It now says what is actually
  going on: *"This version is still being built."*

- ⭐ **The window could no longer be made smaller.** Minimum height was derived
  from the sidebar's space requirement — it grew with every new tab and ended up
  at **1028 pixels**. It is now **380**: since the sidebar scrolls and its groups
  collapse, a shorter window loses nothing.

- **Deleting an entry jumped the list back to the top.** Removing an item far
  down meant finding your place again — the list is redrawn on delete, and the
  scroll position reset. It is now kept, in both the workshop and cargo stock.

### Fixed
- **The README said something wrong about the SC Deutsch Launcher.** It claimed
  the launcher "confirms finds" — that intermediate state has been gone since
  v3.0.0: what is in the `Game.log` is in the game, there is nothing to confirm.
  The English version had been right for a while, the German one had not.

  What stands there instead is the point that actually matters: **both write
  into the same game text file.** That is not a problem — the watcher replaces
  the launcher's list with the same list plus checkboxes instead of adding a
  second one, and undoing the notes brings the launcher's state back. But if the
  launcher runs afterwards, the checkboxes are gone until the watcher has been
  through again (six hours at the latest, immediately via *Refresh*).

- ⚠⚠ **With the "Original" text source, the wrong file was written.** If your
  game is set to German, the details went into the **English** `global.ini` —
  which the game never reads. Writing succeeded, nothing ever arrived, and the
  status line reported success anyway.

  The reason: the tool walked a fixed order — `english` first, then
  `german_(germany)` — and took the first file that existed. Both almost always
  exist, so **English always won**. Which language the game actually reads is in
  `user.cfg` (`g_language`) — a line the tool has always **written** itself, but
  never read.

  Now `g_language` decides. If it is not set, English stays the default — that
  is how Star Citizen starts without it anyway.

  This probably explains why changes to the contract texts did not arrive for
  months. Only "Original" was affected; the **German** and **StarStrings**
  sources carry their own language.


- A button relabelled at runtime went back to its old colour once the mouse had
  passed over it.


- ⭐ **The window no longer fit on a 1920×1080 screen.** With the "Trading"
  group the sidebar needed 1020 pixels, which produced a **minimum height
  larger than the monitor** — Tk then holds that against any attempt to shrink
  the window, it extended past the taskbar and everything below became
  unreachable. Found by **Morkhan (KRT)** on the first day of testing.

  Two changes: the minimum height is now capped to the screen, and the
  **sidebar scrolls** when it does not fit — otherwise the lower tabs would
  simply have been cut off.

- The **diagnostic report** now states window size and minimum size. When the
  above was found the report contained not a single figure about it, although
  those were exactly what mattered.

### Thanks
- **Morkhan (KRT)** for the idea behind this tab, for spotting that the
  window no longer fit the screen, and for the thought that one
  place taking the whole cargo beats the best single price.

## v3.3.5 - 2026-08-30

### Fixed

- ⚠ **Three blueprints could never find each other.** Quotation marks are
  levelled when names are compared — straight, typographic, French. The table
  was missing the **opening** typographic one: `SW16BR1 “Buzzsaw” Repeater`
  became `sw16br1 “buzzsaw' repeater`, closing levelled, opening not.

  Affected are the three `SW16BR…` repeaters. Anyone who had them from another
  source — log, launcher, import — saw them as permanently **missing**, even
  though they were in the inventory.

  Found by comparing a hand-kept list against the catalogue, not through a
  report: nobody suspects a quotation mark. The self-test now runs every common
  quotation mark through the comparison form and requires the same result.

## v3.3.4 - 2026-08-30

### Fixed

- **Blueprints that were once named differently in game are recognised again.**
  The translation occasionally renames items. Anyone who got the blueprint before
  carried the old name in their inventory forever — and the catalogue did not
  know it.

  | In the inventory | In the catalogue today |
  |---|---|
  | `BlackFire Racing Flight Suit` | `Neutrino Racing Flight Suit BlackFire` |
  | `BlueFlame Racing Helmet` | `Neutrino Racing Helmet BlueFlame` |

  Same words, different order, one series name more — a string comparison never
  catches that.

  ⚠ **A match is only made when it is unambiguous:** when **exactly one**
  catalogue entry contains all the words of the old name, and the name has at
  least two words. `Parallax` alone sits inside five entries and therefore stays
  as it is. A wrongly matched blueprint would be worse than one openly listed as
  unknown.

  The existing inventory is corrected on the next start.

  Found in the data of **Morkhan (KRT)** 🙏

## v3.3.3 - 2026-08-30

### Fixed

- ⚠⚠ **The tool was spoiling its own detection.** Anyone with the in-game item
  details switched on — class, size and grade in the item name — had every
  newly unlocked blueprint **stored wrongly** from then on.

  The reason: the game reports a blueprint under the name that currently sits in
  its text file. And since the insertion that is no longer "Balandin" but
  **"Balandin (S3 B Military)"**. That is exactly what got stored. The catalogue
  does not know that name — the blueprint counted as **not owned**, the tick was
  missing from the list, progress stayed too low, and every further find made it
  worse.

  For one reporter it was **twelve** blueprints. It only came to light because
  since v3.3.2 the report says which names the catalogue does not know — the
  list read like an excerpt from the game, only with a suffix.

  **Fixed both ways:** new finds are stored under their catalogue name, and the
  existing inventory is corrected once on the next start. Nothing is lost and
  nobody has to do anything.

  ⚠ The bracket is only removed **when it is the cause**: 39 blueprints are
  named that way themselves ("A03 Sniper Rifle Magazine (15 cap)", "Artimex
  Arms (Modified)"). The rule applies only when the full name is unknown and the
  shortened one is known — so it also covers a suffix that does not exist yet.

  Reported by **Morkhan (KRT)** 🙏

### Thanks

**Morkhan (KRT)** found three things that day, and the last was the heaviest: a
bug that affects every user with the item details switched on, and that drifts
further apart over time. Thank you 🙏

## v3.3.2 - 2026-08-30

### Added

- **The report now also says *which* blueprints the catalogue does not know** —
  not just how many. Up to twelve names, then "… and N more".

  The number alone only says that something does not line up. The names usually
  say why as well: a whole armour set the catalogue does not carry yet, or a
  different spelling. Without them somebody would have to compare the file with
  the catalogue by hand — which makes the line in the report worthless.

## v3.3.1 - 2026-08-30

### Fixed

- ⚠⚠ **The name you typed did not come along.** Enter your name in the problem
  report and you see it in the box right away — but what was sent, copied and
  saved was still the earlier version, so "From: not given".

  The reason: the four buttons worked with the report built when the **page was
  opened**; redrawing only changed the display. Above the box it says "You see
  exactly what you are sending" — then exactly that has to go out. The text now
  comes from the box.

  Reported by **Morkhan (KRT)** 🙏 — *"it's there for me, but apparently not
  when I send it."*

- ⚠ **Two numbers for the same inventory.** The problem report said 315
  blueprints, the blueprint list showed 292 — and both were right: the report
  counts the stored entries, the list walks the catalogue and ticks off what you
  have. A blueprint the catalogue does not know is missing from the second
  number.

  The report now states the difference itself: "315 blueprints · 292 of them in
  the catalogue, 23 unknown". That turns a contradiction into information — and
  the more useful kind.

  Reported by **Morkhan (KRT)** 🙏

### Thanks

**Morkhan (KRT)** found both bugs on release day, with screenshots and a
description that explained the fault straight away. Thank you 🙏

## v3.3.0 - 2026-08-30


### Added

- ⭐⭐ **The workshop — three new pages.** The blueprint used to be where the
  answers stopped: "you have it" or "you are missing it". Now the tool answers
  what comes after that.

  | Page | The question it answers |
  |---|---|
  | **Crafting** | What does this blueprint need — and what comes out? Ingredients, craft time and the stats of the finished item, for **1,597** craftable things |
  | **My stock** | What do I have? Material, amount, quality and location, kept by hand. The recipe then shows what is missing |
  | **Mining** | Where do I get it? Type a resource → where it is found. Type a location → what is found there. **48 locations, 38 ores** |

  **And quality counts.** One slider per ingredient shows what *your* material
  makes of the values — the data carries it for **1,524 of the 1,597**
  blueprints. If you hold quality 900 iron and quality 500 riccite, you see
  exactly what that yields.

- **The author of the German translation is now credited** — with name,
  repository and licence. It is by **rjcncpt**
  ([StarCitizen-Deutsch-INI](https://github.com/rjcncpt/StarCitizen-Deutsch-INI))
  under **CC BY-NC-SA 4.0**, which requires exactly that. Until now only the SC
  Deutsch Launcher was named — the distributor, not the author.

  Shown under **Thanks & Licenses** and in both readmes.

  The watcher does **not bundle** the translation and never passes on a modified
  copy: it only extends the file on your own machine, and the **source note in
  its first line is left untouched** — the author asks for that, so anyone can
  find their way back to the original translation.

- ⭐⭐ **Only things that actually exist in the game can be stored** — resource
  **and** location. The "Add anyway" button is gone.

  The reason is not tidiness: a free text field means somebody can enter slurs,
  religious or political text, take a screenshot and spread it. In the end nobody
  asks who typed it — it stands in this tool.

  | Field | Choice | Source |
  |---|---|---|
  | Resource | **52 names** — 39 minerals, 13 plants | game data |
  | Location | **158 stations, cities and outposts** | UEX Corp |
  | Quality | 0–1000, anything else is rejected | |

  The location stays **optional** — empty is still fine. And if no location list
  has arrived yet (first start without a connection), the field does not block.

- ⭐ **The 13 plants are new** — Flareweed, Heart of the Woods, Sunset Berry,
  Golden Medmon and the rest. The watcher did not know them: they are not listed
  with the minerals but as deposits at the locations. They are hand-harvested and
  can now be stored with a quality.

- ⭐ **Crafting search now finds the ingredient too.** "ric" returned "Lo**ric**a"
  and "Fab**ric**ation" — accidents — and never the 83 blueprints using Riccite.
  And where nothing comes of it, it now says so: **26 of the 52** resources appear
  in no recipe, all plants among them. The search box is therefore labelled
  "Blueprint or resource …" instead of "Search …".

- ⭐ **"Buy or mine?" — the question that follows "you are missing".** Next to
  every missing ingredient it now says what buying it would cost — or that it
  **cannot be bought at all**.

  The finding behind it is the real gain: of the 26 resources used in recipes,
  **seven cannot be bought anywhere** — Aslarite, Lindinium, Ouratite,
  Quantainium, Riccite, Savrilium, Torite. And **five of those are also on the
  dismantle blacklist**: neither purchasable nor recoverable from a dismantled
  item. Those are the real bottlenecks in crafting, and until now nothing said
  so.

  > ⚠ "Cannot be bought" is written exactly that way — never as "0 aUEC".
  > Otherwise somebody hunts a terminal for a bargain that never existed.

  > ⭐ **Goods bought at a terminal are always quality 500** — the base point.
  > An item made from them gets exactly ×1.000 on **every** property. It only
  > gets better with self-mined ore above that. That is why the quality now
  > stands next to the price: without it "buy" reads like an equivalent route
  > that merely costs money instead of time — and it is not.
  >
  > Measured across every recipe in build 4.10.0: **5,025 of 5,219** quality
  > effects have their base point at exactly Q 500.


  Prices come from the [UEX Corp](https://uexcorp.space) API, **at most once a
  day** and in the background. ⚠ They are **not bundled** — the same rule as
  for scmdb. Without a connection the last state stays; with none at all the
  line simply does not appear, and the page looks exactly as before.

  No trade routes, no per-terminal prices, no cargo planning: the watcher
  answers "buy or mine?", not "where do I sell highest?".

- ⭐⭐ **Scan signature — turning the scanner's number into a name.** The mining
  scanner in game shows a value and does not say what is behind it. Type it into
  the mining page and the watcher tells you **which ore** it is and **how many
  rocks** the deposit holds.

  | Input | Meaning |
  |---|---|
  | `8600` | this exact value |
  | `~8600` | ±10 % tolerance |
  | `12000-13000` | anything in between |

  > ⚠ Without the tilde **nothing** is rounded. If you are off, you get "no ore
  > has this signature" rather than a match that sends you to the wrong rock.

  Rarity limits how many rocks a deposit can hold — Quantainium is legendary, so
  at most two. A deposit of three cannot exist, and the tool does not claim one.

- ⭐ **Which refinery gets you the most** — every ore now lists all twenty
  refineries with their bonus, best first, plus the spread. And the spread is no
  rounding error: **Bexalite differs by 18 percentage points** between the best
  and the worst choice, Quartz by 16, Titanium by 15.

  Stations sharing a profile appear on one line (`CRU-L1 +1 others`). Ores where
  it makes no difference say so instead of showing ten zero rows.

- **What dismantling will NOT give back.** Six resources are on CIG's blacklist —
  Lindinium, Quantainium, Riccite, Ouratite, Stileron, Savrilium. Everything else
  returns at half. If a recipe uses one of them it now says so: a part made from
  it is a one-way street.

- **Percentage and range on every quality effect.** `× 0.867` has to be converted
  in your head — `−13.28 %` now stands next to it. And below it, what is
  achievable at all: `Q 0–1000 · ×1.2–0.8 · base 500`. Without that a factor does
  not tell you whether there is much left to gain.

- **Star Citizen Fan Content** — the official "Made by the Community" badge from
  the Fankit is now in the readme, and the full notice per the Fankit Agreement
  is also **inside the program** under "Thanks & Licenses". People who use a tool
  rarely read its readme.

- **One quality slider per material instead of one for all.** There used to be
  a single slider giving every ingredient the same quality — a situation you
  practically never have. Each material now has its own, starting at your
  actual stock value.

  That makes the real question askable: "I have 500 Iron — what do I get with
  900, and what does that change about the Riccite value?" A material that
  raises three properties still has just **one** slider; its three rows move
  together.

- **The stock list shows how a material is mined** — hand, vehicle or ship, as
  its own column.

- **The stock list is searchable** — the search box is always there now, not
  only from five entries on.

- **Delete a single entry** while editing it — a red button next to "Save
  change", with a confirmation naming the entry and amount.

- **Crafting filters by material:** "have the material" or "material missing",
  calculated against your stock. With 1597 blueprints that is 19 against 1573 —
  which is what makes the list usable.

  > ⚠ Calculated from **your list**, not your cargo hold. The watcher does not
  > know the latter.

- **A red "Clear stock" button** — with a confirmation, so nobody loses their
  stock by accident. The question names **how many entries** will go. Your
  stock is handwork that exists nowhere else, and the export button sits right
  next to it.

- ⭐ **Search by contract.** "Retake" used to find nothing although six
  blueprints come from contracts with that word. The search now also covers
  **contract name, faction and contract type** — "nine tails" finds three
  blueprints, "headhunters" 141.

  Above the results an overview answers the actual question: **what does this
  quest hold?** For "retake" that is `Retake Platforms From Nine Tails — 3
  blueprints` and `Need multiple CFP outposts retaken — 3 blueprints`.

  > **And the contracts are clickable.** One click narrows the list to that
  > contract's blueprints only; clicking it again releases the filter.

- ⭐⭐ **Two levels instead of one long list — category and subtype.** The type
  dropdown had thirty entries: "Armour (arms)", "Armour (legs)", "Helmet",
  "Backpack", "Clothing (jacket)" … Assembling a full set of armour meant
  hunting through all of them.

  There are now **seven groups** — ship weapons, ship modules, ship tools, FPS
  weapons, gear, armour, clothing — each with its own subtypes: ship weapons
  split into laser cannon (22), laser repeater (15), ballistic cannon (13),
  ballistic gatling (9), scattergun (6) and the rest; armour into helmet (84),
  torso (70), arms (69), legs (69), undersuit (11).

  > **What cannot be grouped stays on its own** — docking collars and the other
  > one-offs do not vanish into a catch-all.

  **Blueprint list and crafting share one grouping** — same blueprints, so the same way to search.

- **The subtype field now says that it is one**: instead of "All subtypes" it
  reads "12 subtypes — refine here" whenever there is something to pick.

- **Your own watches can be removed** — every row has an ×.

- ⚠ **When a watched item becomes available, you now see it.** The "watching"
  filter only checked clicked names — a match on a search pattern stayed
  invisible, so you watched something and were never told it had arrived. It
  now shows as an ordinary row with its info icon, drop-off and reputation.

- **The watcher now shows which contracts are running** — and keeps them across
  a restart. Until now an accepted contract was only a line in the log view;
  restarting the watcher lost it.

  This works because Star Citizen writes not just the acceptance to its log but
  every ending too. Across the logs of a single machine: 701 acceptances, 303
  completions, 112 withdrawals, 57 failures — each with the same mission id.
  The watcher walks the running log once and keeps score: accepted with no
  ending after it means still open.

  > **Finished ones disappear.** Someone running ten contracts in an evening
  > should not have to look at ten dead lines. Completion, withdrawal and
  > failure remove the contract from the display, immediately and while
  > running.

  **Shared** contracts count as well: if someone in your group passes one to
  you, you see just as clearly whether it holds blueprints for you.

  Two things the log cannot know, so they are not claimed: restarting the
  **game** starts a fresh log, and nothing is asserted about what ran before.
  And if a contract is lost to a bug, the game says nothing — for exactly that
  case every line can be dismissed with a click on the ×.

### Changed

- **Data now comes from the official SCMDB mirror.** Krovax set up a public
  repository for exactly this purpose
  ([KrovaxCode/SCMDB_DATA](https://github.com/KrovaxCode/SCMDB_DATA)) — "for
  programmatic consumers". That is steadier than going through the website,
  which sits behind bot protection. **scmdb.net stays as a fallback** should the
  mirror ever be unavailable. Thanks to Krovax 🙏
- **"Progress" is now "Blueprint progress".** With the new pages the old name
  would have been ambiguous.

### Fixed

- ⚠⚠ **Clicking "read the old logs again" brought back the full setup wizard on
  the next start** — on a tool that had been set up for weeks. And closing that
  wizard left you with nothing at all: the program quit **silently**, no overlay,
  no message, not a line in the problem report.

  Two mistakes in a chain:

  | | |
  |---|---|
  | How "first start" was detected | by the missing **read position** (`logstand.json`) — the very file that button deletes on purpose |
  | What cancelling did | quit the program, **always** — even with the setup complete |

  The tool now records a completed setup itself, and cancelling only quits on a
  **genuine** first start. Someone who dismisses the wizard wants to keep
  working, not to stop.

- ⚠⚠ **"Buy me a coffee" and "Discord" did nothing at all.** Both buttons at the
  bottom left said "opening", and then nothing ever happened — not even a line
  in the problem report.

  The cause sits in the Linux build: inside the AppImage the library paths point
  into our own unpacked bundle. Any system program started from there loads our
  libraries instead of its own and dies immediately. Python's `webbrowser`
  reports success anyway — it only checks that it **started** something, not
  that it survived.

  Half the links in the program already had the countermeasure, the other half
  did not. They all go through one place now: clean environment, `xdg-open`
  first, `webbrowser` only as a fallback — and if it really fails, the address
  appears in the status line instead of the button staying silent. The self-test
  no longer lets a direct `webbrowser` call through.

- ⚠⚠ **`SC_BP_NO_NET=1` did not switch off everything it promised.** The
  catalogue, prices, storage locations, server status and the update check
  honoured it — the **translation sources** and the **contract data** did not.
  Anyone setting that switch does not want half an assurance. Every fetch now
  honours it; the one exception remains the problem report, which only goes out
  on a button press anyway. The self-test no longer lets a module with network
  access pass that does not know the switch.

  The README also names **every** connection individually now, with how often
  it happens — it used to say "two things", and there are five.

- ⚠ **The numbers in the README were a patch old** — "655 of 722 blueprints"
  instead of the actual **670 of 738**. Numbers like that go stale with every
  game patch without anything noticing; the self-test now compares them against
  the real data.

- ⚠ **"Reset inventory" sat under "Report a problem" — nobody looks for it
  there.** It now sits at the end of the **Blueprint inventory** page, right
  below "Read the logs again". Side by side, the difference that matters also
  becomes visible: reading again **adds** what is missing. Resetting **throws
  away** and rebuilds from the logs.

- ⚠ **In a recipe you could no longer tell which range belonged to which
  value.** The lines "Q 0–1000 · ×0.9–1.1" piled up under the last value
  instead of sitting under their own — with three materials that meant three
  near-identical lines with no visible link to anything.

- ⚠ **Backticks showed up in the middle of on-screen text** — "`8600` for an
  exact match" instead of "8600". They come from the markup in the text file;
  Tk simply displays them. Affected were the scanner-reading help text and
  paragraphs under "What's new". The interface check now also trips on
  backticks, not just on asterisks.

- ⚠⚠ **The startup trace in the diagnostic report had become useless.** Instead
  of the startup steps it showed the same line twelve times, "Liste: zeichnen
  beginnt" — and that section is the only thing left after a hard crash: its
  last line says how far the program got.

  Two causes, both fixed:

  | What | Before | Now |
  |---|---|---|
  | Splitting startup ↔ usage | anything not starting with "Seite " counted as a startup step | split at the line that ends the startup |
  | Repetitions | every line on its own | summarised as "(12×)" |

  The old way was a list of prefixes — it broke the moment a new trace entry was
  added anywhere in the program. The new one cannot: whatever happens after
  startup is necessarily behind the boundary line.

- ⚠ **No contract at all was recognised in Swiss German.** The `live-CH`
  edition writes "**Uftrag** angenommen", "Uftrag abgschlosse", "Uftrag
  fehlgschlage" — without the "A". Read straight from the source, not guessed.
  Without those entries the watcher stayed silent there: no message, no skipped
  file, simply no contracts.

- ⚠⚠ **The percentages were cut off** — "× 1.047  +4.(" instead of "+4.70 %".
  The label had a fixed width of nine characters; when the percentage was added,
  Tk truncated it silently. Percentage now has its own column, and the self-test
  measures **every** label in a recipe against the width it gets.

- ⚠ **Same material, same quality, same location is now added up** instead of
  becoming a second row. Adding after every mining run otherwise left ten rows of
  the same pile within a week.

- ⚠ **"Remove" in the stock table was cut off** ("move"). It was packed after the
  columns and only got the leftovers.

- ⚠ **An open dropdown stayed put when switching pages** — opened in Crafting,
  then a click on "My stock", and the list kept floating above the new page. It
  now listens for its field being hidden.

- ⚠ **The scrollbar was practically invisible** — contrast **1.6 : 1** on an open
  list. Now 2.9 : 1 there, 3.6 : 1 on a page, plus a visible track and 10 instead
  of 8 pixels. Applies to every scroll area.

- ⚠ **Dropdown fields were as wide as their longest entry.** Among the 64
  manufacturers stands "Musashi Industrial & Starflight Concern" — the field grew
  to 314 pixels and the fourth filter no longer fitted the row. Now capped; the
  open list stays full width.

- ⚠ **The window left the monitor at large font sizes.** With two stacked
  monitors it ran into the second one. It now stays on its monitor unless you drag
  it. **"Very large" has been removed** as a font size — that step made the window
  taller than a screen.

- ⚠ **The stock amount could not be edited the way people do it.** When editing,
  the amount is already in the field; to add three you append `+3` and end up with
  `1.04+3` — which was rejected. Both work now and give the same result. Next to
  the field it shows what comes out while you type: "makes 4.04 SCU".

- ⚠ **The name suggestion sat 557 pixels below the input field**, down by the
  buttons. Now 15 pixels next to it — both measured.

- ⚠⚠ **In the stock list the amount could not be edited the way people do it.**
  When editing, the current amount is already in the field — to add three you
  append `+3` and end up with `1.04+3`. That was rejected ("enter an amount,
  for example 12.5") because only a **leading** sign counted.

  **Both** now work, and both give the same result: `+3` and `1.04+3` each turn
  1.04 into 4.04. Nobody has to know which form is meant.

- ⚠ **The hint about it was a punishment.** "Overwrite the amount — or type +5
  or -2 to add or subtract" described a mechanism in accountant's language
  without saying where the signs belong.

  The real explanation is no longer text: **next to the field it now shows what
  comes out** while you type — "makes 4.04 SCU", "makes 0 — the entry will be
  removed", "more than you have (1.04 SCU)". The hint shrank to one line with
  an example.

- ⚠ **The name suggestion sat 557 pixels below the input field** — down by the
  buttons while you type at the top. A suggestion you have to hunt for is not
  one. It now stands right next to the field (15 pixels); both measured.

- ⚠⚠ **The entire quality block had vanished** — sliders, effects, even the
  value behind "craft time". Affected rc37 and rc38.

  Cause: while adding the dismantle blacklist a variable was named `_dauer` and
  thereby shadowed the **function** of the same name in that file. A few lines
  later `_dauer(stufe['zeit'])` raised `TypeError: 'int' object is not
  callable`, aborting the build mid-recipe: everything from the craft time
  onwards was simply missing.

  > ⚠ The self-test missed it because it **built** the page but never
  > **expanded** a recipe row — which is where that code runs. It now does, and
  > additionally checks that no local name shadows a function of the same file.
  > Measured against the shipped rc38: both checks fire there, at exactly the
  > right line.

- ⚠ **Swiss German went unrecognised.** There is a separate variant of the
  German translation (`live-CH`) that says "**Bauplan überchoo**" instead of
  "Bauplan erhalten". Without the entry the watcher found **zero blueprints in
  silence** there — no error, no skipped file, just nothing.

  Only affects the fallback: a readable `global.ini` always wins. For a vanilla
  English install, whose text file sits inside `Data.p4k`, that list is all
  there is.

- ⚠ **A reordered translation would have blinded the watcher silently.** Only
  the part **before** the placeholder was taken from the game's text file. For
  "Received Blueprint: %s" that is right. Were CIG ever to reorder it — "%s has
  arrived" — nothing would stand in front, and detection would fall back to the
  bundled list, which then no longer fits. Again without any hint.

  No language phrases it that way today; the branch costs nothing and covers the
  day it happens.

  > ⚠ This is the path every blueprint find runs on. The self-test therefore
  > first proves that without a reordered phrasing the search pattern is
  > **character-identical** to the old one — measured, not claimed.

  Both findings come from the blueprint reader of the **KRT Basetool**
  (GPL-3.0), which reads the same `Game.log`. Thanks for that!

- ⚠⚠ **The ingredient list lied for more than one unit.** Typing 10 into the
  quantity box still showed the requirement for a single unit — "1.16 SCU" and
  "missing 1.16" while 11.6 were needed. The deduction was right, only the
  display was not. It now recalculates as you type and shows where the figure
  comes from: `11.6 SCU (1.16 × 10)`.

- ⚠⚠ **If material is short, NOTHING is deducted any more.** Previously it took
  what it could and reported the rest. Clicking with "quantity 10" while having
  material for three left you with an emptied stock and none of the ten items.

  If an ingredient is missing the item was never craftable — the click was a
  slip or a typo. The **shortfall** is now reported, not just the name, and the
  quantity you typed stays so you can correct it. (Stock could never go
  negative, but "swept to zero" is nearly as bad.)

- ⚠⚠ **Good values were shown in the warning colour.** The display coloured by
  the bare number: green from `× 1.000` up, gold below. For **852 of the 6524**
  quality effects in build 4.10.0 that is exactly backwards — there better
  quality lowers the number, and that is the improvement:

  | Property | Cases |
  |---|---|
  | Recoil Smoothness / Handling / Kick | 245 each |
  | Quantum Fuel Burn | 114 |
  | Damage Mitigation | 3 |

  On the FS-9 LMG the best possible recoil (`× 0.800`) sat in the warning
  colour and the worst (`× 1.200`) in green. The direction is now read **from
  the game data itself** rather than guessed from property names, so it holds
  even where the same property runs both ways. Rows where lower is better now
  say so.

  Cross-check: at quality 0 every value is now gold, at quality 1000 every
  value is green.

- ⚠ **"Power Pips" are not multipliers.** They appeared as `× -1.000` — a
  factor that cannot exist. They are in fact counts from **−3 to +3** in fixed
  quality bands, and they affect every power plant (598 of 6524 effects). They
  now read `-1` and `+3`, with sign. Detected by the value, not the name: a
  multiplier is always above zero.

- ⚠⚠ **The open dropdown lists could not be scrolled** — turning the wheel left
  the list where it was and moved the **page behind it** instead. As the field
  slid away, the list closed. The lower entries were therefore **unreachable**:
  everything past "microTech" among the 48 mining locations, everything past
  "Greycat Industrial" among the manufacturers.

  Cause: the mouse wheel is handled in one place for the whole program and finds
  its scroll area by walking up the parent chain from whatever sits under the
  pointer. The open list is a window of its own, but its parent is the dropdown
  field — which sits inside the scrollable page. So the chain walked out of the
  list and into the page behind it.

  The wheel is now caught at the list window itself and stops there; the page
  never sees it. Measured: against the old build the page moves by 10.3%, against
  the new one by 0.0%, and the list scrolls through to the last entry. Scrolling
  **next to** the list still closes it.

- ⚠ **The open list was too long.** It reached from the field to well below the
  window edge, and was clipped at the screen edge when the window sat low. Until
  now it was only limited by available *space* — which is vast on a large display.

  It now shows at most **15 rows**, anything beyond scrolls. That also makes the
  scrollbar visible, so you can tell there is more. For the 48 mining locations
  that is 497 pixels instead of 1090.

  On top of that a hard ceiling: a dropdown never grows taller than the
  **smallest possible** window (760 pixels). Otherwise enlarging the window
  would produce a list that no longer fits once you shrink it again.

- ⚠ **A completed contract kept showing as "accepted".** Reported on
  2026-08-30 for "Retake Platforms From Nine Tails": accepted in game at 01:18,
  completed at 01:59 — and when the watcher started at 02:22 it announced it as
  freshly accepted.

  Two faults propping each other up:

  1. At startup the watcher reads `Game.log` once, only to learn where it left
     off. In doing so it also collects every contract event. If nothing new had
     been written by the next pass, that collection was **not cleared** — it was
     evaluated a second time.
  2. The evaluation took *all* endings first and *all* acceptances second. In a
     section containing both, the ending therefore hit nothing and the acceptance
     put the contract back afterwards.

  Point 2 hits anyone starting the watcher while the game is already running:
  the first section then catches up on everything since the last run.

  The lists are now cleared before every read, and the events are walked **in log
  order**. Whatever is open at the end is shown. Cancelling a contract and taking
  it again straight away still shows it.

- ⚠ **The contract row in the list could not be dismissed.** The red marker only
  existed in the contract bar, not on the row below it — there was no way to get
  rid of the message.

  The row now belongs to its contract: it carries the same red marker, disappears
  by itself once the game reports the end, and can be dismissed by hand.

- ⚠ **The overlay always started at its smallest size**, however large you had
  dragged it. The size was saved — it was overwritten immediately.

  The cause was the minimum-width check from rc10: shortly after startup Tk
  reports width **1** for a window that is not shown yet, so the comparison
  always matched and the overlay was set to the minimum. It now only acts once
  the window is actually up. Verified: 900×400 stays 900×400, and a window
  remembered too narrow is still widened.

- ⚠ **In the stock list the search box lost the cursor after every keystroke.**
  The field was built **inside** the redraw routine, which clears the whole list
  area on every change — so each typed character destroyed the field itself. It
  is now created once, outside. Anything that can hold a cursor belongs outside
  the redraw.

- ⚠⚠ **"Report a problem" could not be opened without internet** — the window
  froze until a network timeout expired. Precisely the page you need when
  something is wrong. The diagnostic report asked scmdb.net for the current game
  version while being built, on the main thread. It now shows the **stored**
  catalogue version. Measured: **6.1 seconds down to 0.1**.

- ⚠⚠ **The server status page could crash the window** with no internet. The
  fetch runs in the background and calls back into the window; switching pages
  or closing the window meanwhile crashed in a thread where no error hook
  catches it. Every callback is now guarded.

- **Without a connection the status page says so** instead of "nothing fetched
  yet, click Check now" — advice that leads nowhere offline.

- **The dropdown no longer runs off screen.** It limited itself to the display
  but not to the window; with 38 materials and the window low on screen it was
  cut off. It is now at most as tall as the window and scrolls.

- ⚠ **The overlay's resize grip was missing entirely.** It hung on the
  blueprint list — fine while the list got the rest of the window. Since the
  active-contracts bar sits above it, the list can end up **shorter than the
  grip itself**. It now hangs on the window and is present at any height —
  checked at 190, 130 and 110 pixels — while still disappearing when collapsed.

- **The dismiss control on a contract line is now a crossed-out circle**, red
  and clearly larger. A cross means "close the window" everywhere else in the
  program; removing a single line is a different thing.

- ⚠⚠ **The overlay said 405 blueprints, the progress page 382 of 738.** Two
  numbers for the same thing, and neither explained the other.

  The cause was the catalogue on disk: written months ago, when magazines were
  still keyed as `FS-9 Magazine (75 cap)`. The inventory has long used
  `FS-9 Magazine (75)` — matching the quantity wording came later. **23
  magazines and batteries** counted as missing everywhere although they were
  owned.

  Catalogue keys are now rebuilt from the name on load. If everything already
  matches, nothing is touched.

- ⚠ **The open dropdown stayed put while scrolling.** It floats as its own
  window above the page, so scrolling the list underneath left it lying across
  unrelated rows. No focus change happens there, and focus was all it watched.

  It now also closes on **scrolling**, on **moving or resizing the window**, and
  on **Esc**. Scrolling inside the list itself still works.

- ⚠⚠ **"Nothing found" as soon as a category was selected** — now really fixed.
  The filter was right, but **a second place** discarded whole groups in
  advance, comparing catalogue type against the new top-level category. That
  never matches, so every group fell out. The shortcut is gone; the check now
  happens in exactly one place.

- ⭐ **The watcher now notices such cases itself.** If the dropdown says
  "Ship modules (157)" and the list stays empty, that is a contradiction — one
  number comes from the catalogue, the other from the filter. It is written to
  the error log and appears in the diagnostic report, instead of someone having
  to send a screenshot.

- **Mining lists materials first.** It used to show the 48 locations by
  default, but you arrive asking "where do I find titanium?", not "where am I?".

- ⚠⚠ **"Nothing found" as soon as a category was selected.** The list showed
  `0 of 738` although a category and subtype were chosen. The filter itself was
  right — the **drawing** aborted: rebuilding the dropdowns left the old layout
  callback pointing at destroyed widgets (`TclError: bad window path name`).
  Dead elements are now skipped. Found through the error log that recorded the crash while the screen showed only an empty list.

- ⚠ **The subtype could not be selected on the crafting page.** The check
  "does this subtype belong to the chosen category?" compared against a list of
  **pairs** rather than values, so it never matched and the selection was
  cleared immediately.

- **The button row on "Report a problem" now claims the space it needs**
  instead of wrapping — up to the screen width. Two fixed minimum widths had
  failed: how wide a button really gets is only known once it is drawn, and
  that differs per system.

- ⚠⚠ **Watch patterns matched inside words — and reported the wrong item.**
  The pattern `arden backpack` matched *W**arden** Backpack Purgatory Camo*:
  the watcher announced a piece of armour as available that has nothing to do
  with the one being watched.

  Patterns now match at **word boundaries** only — no letter or digit directly
  before or after. Hyphens and spaces count as boundaries, so `abc-mk4 legs
  grey` still matches.

  > **Why this matters here:** a squadron armour set means exactly one item per
  > slot. The colours were tested for months for camouflage; an "almost right"
  > piece is worthless.

  Found while proof-reading the stored watches.

- ⚠ **On "Report a problem" the five buttons stacked vertically.** The window's
  minimum width was 1100 px while the button row needs 869 px in German plus
  sidebar and margins. It is now 1160 px.

- **The armour role filter is gone again** — nobody searches by it.

- ⭐ **Filter by subtype — you can finally tell the weapons apart.** The
  blueprint list lumped all ship weapons together. There is now an extra
  dropdown: for ship weapons **Ballistic (32) · Laser (40) · Distortion (6) ·
  Neutron (6) · Tachyon (3)**, for armour the **roles** (combat, engineer,
  hunter, stealth, miner …).

  > **It only appears when there is something to choose.** Coolers would offer
  > sizes only, and those have their own field.

  This works by joining two sources: the catalogue knows the armour body parts,
  the recipe data knows the weapon type. Joined by name — **738 of 738**
  blueprints match.

- ⭐ **Crafting now has the same filters**: type, subtype or armour role,
  manufacturer, and "blueprint owned / missing". It previously had a search box
  only, and without knowing what to search for you paged through 1597 rows.

- **Mining gets dropdowns** for material and location — 38 and 48 entries you
  previously had to know by heart in order to type them.

  > All three pages use the same controls as the blueprint list: the way you
  > operate this tool should not change from page to page.

- ⚠ **"You are not watching anything" while nine watches were stored.** The
  watchlist holds two kinds: blueprints clicked in the catalogue — and your own
  watches with search patterns. The view showed only the first kind while the
  diagnostic report counted both. Your own watches now appear at the top of the
  view with their patterns.

- **Crafting takes a quantity.** Building ten in a row meant ten clicks — and on
  the eleventh the stock was wrong without anyone noticing. There is now a field
  next to the button: enter the number, click once, done. It resets to 1
  afterwards so the next click does not quietly deduct ten again.

- **The stock list can be exported and loaded back.** As a backup (`.json`),
  which loads again here, or as a spreadsheet (`.csv`) for reading and sharing.

  > Your stock is handwork that exists nowhere else: no log, no data source,
  > only what you typed. Without an export it is gone at the next machine.

- **The "Inventory" tab is now "Blueprint inventory".** With "My stock" next to
  it, one of the two names had to say which is which.

- **Two new pages: "Crafting" and "Mining".** They answer the question that
  comes after the blueprint — *what do I need, and where do I get it?*

  **Crafting** lists all **1,597** craftable items. One click shows the
  ingredients with amounts and the craft time. And because the watcher knows
  your collection, every row says whether you have the blueprint — 403 ticks out
  of 404 blueprints.

  > **Two** rows show a `?` instead of a tick. Three names cover several
  > different items ("BroadSpec" exists in S02 and S03, "Main Powerplant" for
  > Idris and Reclaimer). Your collection only knows the name, not the variant —
  > so we claim nothing.

  **Mining** answers both directions in one search: type a resource and you get
  its locations (Iron: 27). Type a location and you get everything found there
  (Daymar: 14 ores). Each entry says whether it is FPS, vehicle or ship mining.

  **The two are linked:** in a recipe every resource is clickable and jumps
  straight to its locations.

  ⚠ **What the watcher does not say: whether you can craft it.** It knows your
  blueprints, not your cargo hold. "Needs 0.3 SCU Iron" — yes. "You can build
  this now" — never.

  For probabilities and the refinery comparison **scmdb.net** remains the better
  place; the page links there.

- **My stock — and what your material quality makes of the product.**
  Suggested by **Horthy (KRT)** 🙏

  You enter what resources you have: **material, amount, quality, location**.
  Every ingredient in a recipe then shows whether it is there or how much is
  missing — and a button **"Crafting this now"** subtracts the ingredients, so
  you do not have to do the arithmetic.

  **And quality genuinely matters.** The recipes carry how strongly it changes
  the values of the finished item — for **1,524 of the 1,597 blueprints**. So
  the recipe shows what *your* material would produce:

  ```
  With your material
     Damage Mitigation    × 1.044     Ouratite · Q 720
     Min Temp             × 1.088     Aslarite · Q 800
  ```

  If material is on hand but below the required quality, it says so — otherwise
  you would read "missing 0.3" while 12 SCU sit in your stock.

  **When adding, the watcher suggests the materials that actually exist** — 26
  of them, from the recipes. Type "Aslerite" and you are offered "Aslarite",
  instead of silently never getting a match.

  **The stock is a sortable table**: column headers for material, amount,
  quality and location sort on click, and from six entries on a filter appears.
  Two entries of the same material in different places stay cleanly apart.

  **The recipe shows what you already have** — not just what is missing: "have
  0.02 of 0.09 · missing 0.07". Otherwise you set off to fetch 0.09 when 0.07
  would do.

  **And you can try out a quality.** A slider from 0 to 1000 shows what better
  or worse ore would yield — the same question you would otherwise ask by hand
  on scmdb.net, only with your stock as the starting point.

  ⚠ **The stock is kept by hand**, because the game gives nothing away: 17 MB of
  logs contain not one word about resources or crafting. That is why the watcher
  never says "you cannot build this", only "you are missing Iron". A stock that
  lags two entries behind must not become a liar.

- **Your name in the bug report.** On the "Report a problem" page you can enter
  a name that appears at the top of the report, so follow-up questions can be
  matched to you. **Optional** — empty stays empty, and nothing is ever
  pre-filled.

- ⚠ **With many sources for one blueprint you could not scroll to the bottom.**
  Expanding the origins — the "Hart Scraper Module" has twelve — left the lower
  entries out of view and out of reach.

  > Cause: the scroll length is built from **estimated** row heights. That holds
  > while every row is the same height; an expanded blueprint is several times
  > taller, and the estimate knew nothing about it.

  The list now re-measures whenever a built section differs from the estimate,
  shifts the following ones and extends the scroll area. It does this by itself
  rather than relying on someone remembering it at each click site.

- ⚠ **The reset control in the blueprint list could not be found.** It existed —
  as a small grey underlined text at the bottom right, next to the result count.
  It was missed entirely and filters were cleared by hand instead. What you
  cannot find is not there.

  It now sits **at the top**, in the row with "all / owned / new in game", far
  right and set apart, as a framed button with an ×. It still appears only when
  something is actually narrowed down — and now clears **everything**: the
  dropdowns, the search box and the state selection.

- **The blueprint list starts without filters.** Setting "docking collar, size 2,
  grade A" and returning to the tab later showed "Nothing found" — easily
  mistaken for an empty inventory. Filters and search box are cleared on
  reopening.

- ⚠ **"With your material" was shown even when none of it was in stock.** The
  line on the right said "you are missing 1.2", yet a factor was calculated
  below — from the slider default, not from your material. Anyone reading that
  takes the factor for their own result. The heading now says what it shows:
  "What quality 500 would give", whenever a value is being tried or the stock
  holds nothing.

- **The search fields on Crafting and Mining kept their contents.** Searching
  for "titan" and returning to the tab later still showed only titanium — easily
  mistaken for the whole list. They are empty again on reopening.

  > Cause: a page is built **once** and only shown and hidden after that.
  > Anything that should be fresh has to register for it.

- **Both search fields have a × to clear them**, shown only when something is in.

- ⚠ **Buttons cut off their own labels** — one read "e change" instead of "Save
  change", and in the overlay the contract line ended mid-word. That is not
  cosmetic: someone reading half a word goes looking for a bug that does not
  exist.

  Cause: the surface was sized with `measure()` but drawn with whatever font the
  system provides — and under **Wayland** that is only settled once the window
  is shown. Every button now measures itself three times: when built, when first
  shown, and once when idle. If it grows, its frame grows with it. Applies to
  all buttons including filter rows.

- ⚠ **The overlay could be dragged narrower than its own icon bar**, hiding the
  bell and the icons on the right — at 290 px not one of them was visible. It
  now has a minimum width derived from that bar (measured: 520 px for the title
  and ten icons), and a too-small saved size is raised on startup.

  > The first attempt did nothing because it asked the bar for its requested
  > width — but that bar runs with `pack_propagate(False)`, deliberately not
  > passing on its children's size, and reported **1 pixel**. The elements are
  > now added up individually.

 

- **The contract line in the overlay wraps instead of being cut off.**

- ⚠ **An open window would not come to the front under Wayland.** Clicking the
  overlay appeared to do nothing and only restarting helped. Under Wayland a
  window may not raise itself; what the compositor does accept is a window that
  **re-registers** itself, and that is what now happens — only under Wayland,
  and only when the window really is covered. Keyboard focus stays with the game.

- ⚠ **Buttons cut off their own labels.** One button read "e change" instead of
  "Save change". Cause: the surface was sized with `measure()` but drawn with
  whatever font the system actually provides — where those differ, the text
  runs past the edge and is clipped on both sides. Every button now measures
  itself after its text is set. This affected all buttons, not just one.
 

- ⚠ **Stock entries could not be corrected.** After a typo or after handing
  material to someone else, the only option was to delete the entry and retype
  it — which easily created a second name for the same material. Now **clicking
  a row** opens it in the fields above: change amount, quality and storage
  location, save, done.

  > **Add and subtract instead of doing the maths.** With an entry open you can
  > type `+5` or `-2` to add or remove. Handed everything over? Type the full
  > amount with a minus and the entry disappears. You cannot subtract more than
  > you have; the available amount is shown instead.

- ⚠ **A typo in a material name quietly broke your stock.** The suggestions
  could be ignored: enter `Aslerite` and the list looked right — but no recipe
  found the stock, and nobody learned why. Names are now **matched**: case,
  the mining spelling with brackets (`Aslarite (Raw)`), `Aluminium` versus
  `Aluminum` and a close typo are pulled onto the correct name and reported. A
  completely unknown name is **queried** rather than stored — with an "Add
  anyway" button for the case where you really do have something no recipe
  lists.

- **The location field said "Location".** That belongs to mining. This is where
  your material **sits**, so it now says "Storage location" — and stays
  optional, since not everyone uses several places. **Amount and quality are
  required:** without quality the watcher cannot work out what your material
  does to the finished item, which is the whole point of the stock list.

- **Comma and full stop both work for amounts.** Some type `12.5`, others
  `12,5`. The comma used to raise an error.

- **Clicking the overlay now really brings an open window to the front.** It
  used to stay behind the game, and the click seemed to do nothing. Cause:
  `lift()` alone is ignored under **Wayland** — a window may not raise itself
  there. Now "always on top" is set briefly and switched off again, which the
  compositor accepts. A **minimised** window is restored too; it used to stay
  collapsed. Affects the blueprint list, settings and "What's new".

  > **Your game keeps the keyboard.** The window comes forward but does not
  > grab input focus — if you are flying, you keep flying. Click into the
  > window when you want to type in it. Only at startup does it take focus,
  > because you started it yourself.

- ⚠ **The quality slider stuttered because 4 MB were read from disk on every
  mouse move.** The recipe file was re-read on **every** access — 22 ms per
  call, and the slider fires on every pixel. That came to over 600 ms of
  computing per second. The data now stays in memory and is only re-read when
  the file actually changes: **0.33 ms instead of 21.9 ms**. On top of that,
  dragging now only relabels the values instead of rebuilding them, which took
  care of the remaining flicker.

- ⚠ **The new data never arrived for anyone who already had a catalogue.**
  The fetch stopped as soon as the blueprint catalogue was current — which it is
  for every existing user. Crafting, Mining and Stock would have stayed empty
  until the next Star Citizen patch. Both fetches are now **always** checked;
  they carry their own "already current?" test and load nothing twice.
- **The quality scale was shown wrongly.** In the stock the field read
  "Quality %" and values appeared as "720 %". The recipes work with **0 to
  1000**. Anyone reading "72" in game and entering that would have got wrong
  results throughout — their ore would count as unusable when it is good.

- **"Network error" where the site had simply refused the request.** A 403 is a
  refusal, not a loose cable: the tool now says so plainly, keeps working with
  the data it already has — and no longer retries three times (which cost six
  seconds for nothing).

### Thanks

The idea for the resource stock came from **Horthy (KRT)** — and out of it grew the quality calculation that now shows what your own material makes of a blueprint. Thank you 🙏

And **Krovax** (SCMDB), who set up a public data mirror on request so tools like this one have a dependable source.

## v3.2.1 - 2026-08-29

### Fixed

- **Other tools are no longer written over.** Three programs mark blueprint
  contracts in the game, and all three use the same `[BP]` mark: this one,
  **MrKraken's StarStrings** and the **SC Deutsch Launcher** (watcher and
  launcher even draw on the same data source, so they write word-identical
  lists). Until now the watcher did not tell its own marks from anyone else's.
  All counted against the real 29 Aug 2026 release:

  - **17** of MrKraken's marks were **deleted** when details were written — and
    because the watcher then remembered the already-trimmed wording as the
    original, they never came back on reset either.
  - **297** more ended up **twice**.
  - **136** item names got their tag twice:
    `[CS1] Spark-G Missile (CS1)`.
  - Anyone running the **SC Deutsch Launcher** alongside would have read the
    blueprint list twice over on **336** contracts, and lost the launcher's
    state on reset.

  **The new rule is simple: where a mark already stands, no second one is
  added.** And whatever was there before our first insertion belongs to the
  player — it is restored on reset, even when another tool put it there.

  With the launcher the watcher goes one step further: its list **replaces** the
  launcher's instead of sitting next to it. Because it is the same list — only
  with **tick boxes**, the comparison against your own blueprints. Take the
  details back out and the launcher's list is there again, character for
  character.

  If an item name already carries a tag in square brackets, it is left alone.

  **Thanks to MrKraken** for [StarStrings](https://github.com/MrKraken/StarStrings)
  and to the **SC Deutsch Launcher** team — and sorry for writing over your
  work. 🙏

- **The watcher reported "details are in the game" where none of its own were.**
  It recognised the injection by the `[BP]` mark and by the blueprint list
  heading — both of which the other two tools write as well. Now only what is
  unique to the watcher counts: the **tick box**.

- **Tick boxes appeared in front of regions and delivery points.** In the game
  you read `[  ] Stanton System - Danger 4-6/10`, as if a region were something
  you could own. Cause: the blueprint blocks are structured with headings, and
  three of them carry lists — `# Blueprints` (4,379 lines), `# Delivery` (323)
  and `# Region` (239). Every one of them got ticked. Now only what sits under
  **Blueprints** gets a box; that removes **838** wrong boxes from a finished
  file. Same in German (`# Baupläne`, `# Abgabe`).

- **Installing a new base clears the original-wording file.** It belonged to the
  old file and would have written back an outdated state. The same note also
  protects the fresh file: the watcher has never written into something just
  installed, so there is nothing of its own to remove there.

### Changed

- **MrKraken is now credited in the readme.** He had long been on the "Thanks &
  Licences" page in the tool, but was missing from the readme.
- **The licence stated for StarStrings is corrected.** It said "CC BY-NC-SA
  4.0". The project states no licence at all — not in the repository, not in its
  readme. Attributing a licence the author never granted is wrong; it now says
  "no licence stated".

## v3.2.0 - 2026-08-29

### Added

- **When you accept a contract, the watcher now tells you whether blueprints are
  part of it — and which of those you are still missing.** Until now you only
  found out once the blueprint arrived. It appears in the list the moment you
  accept:

  ```
  Contract accepted: Retake Platforms From Nine Tails
    →  3 blueprints · you are missing: H4-PBF Ammo Carrier
  ```

  This is deliberately **not** contract management: no list, no tab, no second
  window. Just a line, like a blueprint find. The tool does not take on a second
  job — it answers its own question earlier.

  **If the catalogue does not know the contract, it stays quiet.** A wrong
  promise about blueprints would be worse than no message at all.

  Acceptance is detected through the key `mobiGlas_ui_MissionEvent_Activated`
  from the game's own files rather than through the wording — in German the
  **sub-objectives** are also called "Neuer Auftrag", so wording alone would fire
  at every step. It works the same way if your game runs in English.

### Changed

- **The thanks to testers no longer sit in the readme.** They belong in the
  changelog and on the "Thanks & Licenses" page inside the tool, where they
  remain in full.

## v3.1.0 - 2026-08-29

### Added

- **Caught-up blueprints are now reported, not just added silently.** When the
  watcher finds something in the logs — on startup or at the push of the button
  — it appears in the list, marked *caught up* so it doesn't look like a fresh
  find.

  Up to ten individually; above that it stays with the summary in the status
  bar. The reason for that limit: on the very first start the catch-up goes
  through **every** stored session — on a well-used machine that is over a
  hundred, and nobody wants to dismiss those one by one. Day to day it is zero
  to three, and those are exactly the ones you want to see.

### Fixed

- **The same blueprint counted twice when the game runs in German.** The SC
  Deutsch Launcher reads the **English** catalogue and writes
  `Ravager-212 Twin Shotgun Magazine (16 cap)`. Re-reading the logs picks up the
  same crate in whatever language Star Citizen runs in — in German
  `… (16 Schuss)`. To the Watcher those were two different blueprints.

  Measured against a real inventory: **405 shown, 403 actually held.** The bug is
  silent — nothing breaks, the number is simply too high.

  The quantity in brackets is now language-neutral: `(16 Schuss)` and `(16 cap)`
  are the same blueprint. **The number stays** — a 40-round and a 60-round
  magazine are different blueprints and must remain so. Brackets that do not
  start with a digit are untouched, so `Singe Cannon (S2)` keeps its name.

  An inventory already on disk is migrated on the next start: duplicates are
  merged into one entry, and the **older** find wins.

- **"Start with the system" never worked on Linux.** The Watcher wrote the
  AppImage's **temporary mount point** (`/tmp/.mount_SC-BP-ji95vH/…`) into the
  autostart file. That path gets a new random name on every launch, so after a
  reboot the entry pointed nowhere and the Watcher did not come up — with no
  error message, because the file looked perfectly fine.

  The cause was the order in the code: an AppImage also counts as "frozen", so
  that branch won and the real AppImage path was never reached. Now reversed.

  Found on 29 Aug 2026 on a machine where the entry had been dead ever since the
  move to Linux.


- **The floating lock sat seven pixels too far right.** The offset for it came
  from a measurement on a **different screen** (5120×1440 instead of 4096×1152)
  — symbols are 24 px wide there instead of 22, and an offset measured in pixels
  applies to exactly the one screen it was measured on.

  Measured again on the running program: without the offset it sits exactly on
  target. It is back to zero.

## v3.0.3 - 2026-08-28

### Fixed

- **Three places showed the key name instead of the text.** Most visibly on the
  rocket icon: its tooltip literally read `s_sp_start`. It now says what was
  meant — "Launch Star Citizen".

  The other two would have surfaced on the next failed download and in the
  version window.

  The cause is a fallback that hides too well: if the language table does not
  know a key, it returns **the key**. That beats crashing — but the fault stays
  invisible until someone sees it in the running program.

  The self-test now checks this: it collects **every** call with a fixed key
  across the program and matches it against the table. With over 600 entries
  that cannot be done by hand — and it was this check, not a person, that found
  all three.

  Reported by **der Autor** on 2026-08-28.

### Changed

- **It said "check daily for new versions", but checked hourly.** The interval
  has always been one hour; the text beside it said otherwise. It only came up
  once the check actually started repeating.

## v3.0.2 - 2026-08-28

### Fixed

- **A running watcher never learned about a new version.** The notice only
  appeared after a restart — anyone leaving the program running for days never
  saw it.

  It looked **exactly once**, two seconds after startup. The hourly interval in
  the check only limits how often it *may* ask; someone still has to ask. That
  now happens every hour.

  Reported by **der Autor** on 2026-08-28: v3.0.1 was out and the running watcher
  stayed quiet — even though it had already fetched it and had it in its cache.

- **An expected error made the problem report useless.** While downloading,
  progress arrives every second; if the window closes during that, every single
  update fails — caught, but logged each time.

  In one report that filled **50 of 50** slots with the same line, all within
  eight seconds. Every real error had been pushed out. This message is now only
  recorded the first time.

## v3.0.1 - 2026-08-28

### Fixed

> [!important]
> **If the watcher was closed while Star Citizen kept running, that session's
> blueprints were lost** — permanently. If that sounds familiar: press the new
> **Read the logs again** button once and they are back.

- **The running `Game.log` was only read on the very first start.** After that
  it counted as done: live reading resumed at the remembered position, and
  everything before it was unreachable. The file only moves to the backup folder
  on the next game start — until then the blueprint was missing with nothing to
  hint at it.

  Measured: the blueprint sat at byte 11,987,664, the read position at
  12,759,872. It would never have been found.

  The running file is now read in full on every start. That costs a fraction of
  a second — the catch-up goes through every stored log anyway — and duplicates
  cannot happen, the inventory checks every name.

  Reported by **der Autor**, hours after v3.0.0.

- **After a game restart the read position jumped to the end of the file instead
  of the start.** When Star Citizen creates a fresh `Game.log`, it is shorter
  than the remembered position. The comment there correctly says "a new game
  session has run" — but the code set the position to the **end** of the new
  file instead of reading from the beginning. Everything the fresh session had
  already reported was skipped.

### Added

- **A "Read the logs again" button** — in the overlay's title bar and in the
  settings under *Inventory*. It goes through every stored session again,
  including the ones already read, and fills in what is missing.

  It also helps when the game language was not yet detected on the first run:
  the logs were then searched with the wrong wording and still marked as read.

### Changed

- **Two texts that were no longer true.** The lock's hint described it as
  sitting "at the top right of the overlay" — it hasn't since v3.0.0. And the
  settings text still pointed to a second program start as the way back, even
  though the lock exists for exactly that.

## v3.0.0 - 2026-08-28

> [!important]
> **On Windows there is now an installer instead of a single `.exe`.** Updating
> therefore opens an installation window once — that is correct and not foreign
> software. The watcher restarts by itself afterwards. On Linux it stays one
> file: the AppImage.
>
> **The SC Deutsch Launcher is no longer required.** Blueprints come from Star
> Citizen's own `Game.log`. With the launcher you keep German names and a few
> extra details — without it (always the case on Linux) nothing essential is
> missing.

A year after the first build, the narrow notification bar has grown into a tool
that fully answers „which blueprint do I have, and where do I get the rest?" —
without leaving the game.

### The main points

- **One window with everything in it.** Blueprint list to search and tick off,
  progress by area, settings, server status, „What's new" — instead of scattered
  little windows.
- **Where each blueprint drops.** One click shows the faction, the contract, the
  standing required and the payout — for **655 of 722** blueprints, sorted by
  the easiest route. „I'm missing X" is half the information; „X drops at
  Foxwell from Veteran" is all of it.
- **New in the game.** A filter shows what the current patch brought, and a
  dropdown next to it every earlier patch. Every blueprint carries the game
  version it first appeared in.
- **Details inside the game.** The watcher writes into contract texts **which**
  blueprints a contract hands out — with `[x]` for the ones you already have.
  And on request class, size and grade onto item names, so the tractor beam
  reads „Glacier (Mil/1/A)" rather than just „Glacier".
- **The overlay gets out of the way.** On request it only pops up briefly when a
  blueprint arrives; mouse clicks can be passed through to the game, and a lock
  in the bar brings it back. It can also fold down to just its title bar.
- **Reporting problems without guesswork.** A red button collects system,
  version, game state and the last errors into one report — no names, no paths.
  That is why the bugs in this changelog are described so precisely.
- **German and English, completely.** Switchable in the program. The blueprint
  message in the log is recognised in **any** game language — the watcher works
  out the wording by itself.
- **Windows and Linux from one codebase**, with autostart, self-update and a
  tray icon on both.

### Thanks

Without these three, v3.0.0 would be markedly worse. They tested on their own
machines and described faults well enough to find them:

- **Bomb20** (pr0) — that the tool could not be kept up to date on Linux, plus
  the crash on the very first start and a morning with four finds that would
  otherwise have hit every user.
- **Haldjas** (pr0) — pop-up mode and click-through go back to him; so does the
  way **there and back** for click-through, the installer that failed on the
  running file, and the console windows during updates.
- **Morkhan** — the item details in game, and the find that
  several reward tiers of one contract were overwriting each other in the
  catalogue: **797 blueprints** nobody had ever seen before.

The complete list of every single change is in the `v3.0.0-rc1` to `v3.0.0-rc99`
sections below.

## v3.0.0-rc99 - 2026-08-28

### Fixed

- **The green lock did not sit exactly on the lock in the bar.** A narrow edge
  of the symbol underneath showed on the right — it looked like two locks
  instead of one changing colour.

  The offset was **measured** from a screenshot, not estimated: the upper lock
  sat at x=1068–1091, of the lower one only x=1094–1098 was visible. At 24 px
  wide the lower one therefore starts at 1075 — **7 px further right**. The
  upper one now moves by exactly that.

  ⚠ The value is measured, its **cause is not known**: in a rebuild with the
  same Tk version and the same symbols, the lock sits exactly right without any
  offset. It is therefore a named constant in one place, and applies only to the
  visible state — pop-up mode calculates differently and is left alone.

## v3.0.0-rc98 - 2026-08-28

### Fixed

- **The lock was more opaque than the overlay beneath it.** With transparency
  turned down, passing clicks through showed two locks of different saturation
  on top of each other — the one in the bar showed through, the one above it did
  not.

  A separate window does **not** inherit the main window's transparency; it has
  to be given its own. Both now carry the same value, and it looks like one lock
  changing colour — as intended.

## v3.0.0-rc97 - 2026-08-28

### Fixed

- **On a second screen, the strip and lock jumped to the wrong monitor.** This
  affected pop-up mode: if the overlay sits on a monitor **above** the main
  screen, the green strip and its lock reappeared at the top edge of the main
  monitor.

  A monitor above the main screen works with **negative** Y values — that is a
  valid position, not a broken one. Remembering the position accounted for it;
  displaying it threw it away again: a `max(0, …)` clamped every height below
  zero to the top edge of the main monitor.

  The strip carried that line from the start; the lock inherited it when it
  moved next to the strip in rc94. Both are rid of it.

## v3.0.0-rc96 - 2026-08-28

### Fixed

- **On hiding, the lock took three seconds to return to its place.** When the
  overlay hides itself in pop-up mode, the lock belongs back at the handle
  strip — instead it stayed where the bar had just been.

  It was **exactly** the ten 300 ms retries from rc92. Those are meant for
  startup, where the bar is about to appear: while it is still being drawn, the
  lock waits instead of jumping to a guessed spot. But that waiting also ran
  when the overlay had **deliberately** gone away — waiting for something that
  is not coming.

  Both cases look the same at the button, but not at the window. Measured:

  | Case | Window | Button |
  |---|---|---|
  | startup, still being drawn | 1 | 0 |
  | deliberately hidden | 0 | 0 |

  The window is now asked. If it is gone, the lock moves at once.

  Reported by **Haldjas (pr0)** on 2026-08-28, including the exact separation
  from the six seconds the overlay itself stays up.

## v3.0.0-rc95 - 2026-08-28

### Changed

> [!important]
> **A found blueprint is green from now on — no more yellow „provisional".**
> Anyone with the SC Deutsch Launcher installed saw every find from the
> `Game.log` in yellow first, until the launcher confirmed it. That confirmation
> no longer exists, and neither does the yellow waiting.

- **The waiting state is gone, not just the colour.** The yellow dot meant „read
  from the Game.log, waiting for the launcher to confirm". Since the `Game.log`
  is the source and the launcher only adds to it, that confirmation can never
  arrive.

  What remained was a state with no way out: with the launcher you saw permanent
  yellow — without it permanent green, at **exactly the same certainty**. Two
  colours for one statement are not information, they are a dead end.

  The whole mechanism went, not just the display: the register of unconfirmed
  rows, the matching of log names to launcher keys, the after-the-fact
  confirming of a row, the word „provisional" — and the yellow dot in the
  documentation, so nobody hunts for a symbol that does not exist.

  The launcher stays what it is: an addition. German names, maintained details
  for type, size and grade, and it reports anything the log missed.

## v3.0.0-rc94 - 2026-08-28

### Improved

- **In pop-up mode the lock now sits by the handle strip.** It sat at the top
  right corner of the remembered overlay position — correctly calculated, but
  on its own: the strip that shows where the overlay is waiting sits centred,
  with the lock a good two hundred pixels further right, where there is nothing
  to see.

  Two markers for the same thing belong together. It now reads as one: this is
  where the overlay waits, and this is the lock.

  Reported by **Haldjas (pr0)** on 2026-08-28.

## v3.0.0-rc93 - 2026-08-28

### Fixed

- **In pop-up mode the lock floated beside the overlay.** The rc92 fix worked
  for everyone who keeps the overlay visible — in „only on a new blueprint"
  mode the old behaviour remained.

  The reason: there the overlay is **hidden** at startup, before it has ever
  been drawn. That leaves no bar for the lock to align with, and the fallback
  used the position of an invisible window — measured, a never-drawn window
  reports width 1 and position 0. The lock ended up somewhere beside the
  overlay.

  It now hangs off the same remembered position as the handle strip, which in
  pop-up mode already shows where the overlay is waiting — and moves onto the
  bar as soon as the overlay pops up.

  Reported by **Haldjas (pr0)** on 2026-08-28. His problem report settled it:
  without the line `overlay_modus=popup` in it, why this hit him and not others
  would still be guesswork.

## v3.0.0-rc92 - 2026-08-28

### Fixed

- **After a restart the lock sat beside the overlay instead of on it.** Anyone
  who had click-through saved as on saw **two** locks after every start: one in
  the wrong place next to the window, one in the title bar. Only the first
  toggle moved it into place — and the next start began the same thing again.

  The cause is an old `tkinter` trap: the state is applied immediately before
  the window loop starts. The bar is already in the tree by then, but Tk has
  drawn nothing yet — neither „is visible" nor the measurements are true at that
  moment. So the lock went to a guessed position.

  It now **waits instead of guessing**: while the bar is not yet drawn, no lock
  is built at all; it retries until the bar is there. A briefly flashing lock in
  the wrong place would only have been half a fix.

  Reported by **Haldjas (pr0)** on 2026-08-28, with the full steps to reproduce.

## v3.0.0-rc91 - 2026-08-28

### Improved

- **One lock instead of two.** The green lock used to sit in the overlay's
  corner while the title bar still showed an open one — two locks, one of them
  stating the opposite of the truth.

  The green lock now sits **exactly on top of** the one in the title bar: same
  place, same size, same component. To the player it is one lock changing
  colour — closed and green means „clicks go to the game", open and grey means
  „the overlay catches them". You unlock where you locked.

  It remains a **separate window**, and that cannot change: passing clicks
  through applies to the whole window — a button in the bar would be just as
  unreachable as the rest. If the bar is collapsed or the overlay hidden in
  pop-up mode, the lock falls back to its old place in the corner.

## v3.0.0-rc90 - 2026-08-28

### Improved

- **The lock now sits permanently in the overlay's title bar.** Passing clicks
  through to the game was only reachable via Settings → Overlay; getting back
  was comfortable, through the lock that appears while it is active.

  A way there and back belongs in the same place. The title bar therefore
  carries an **open** lock — it means „the overlay catches clicks". One click
  closes it, and from then on the floating lock at the top right takes over, as
  before. No more detour through the settings.

  The button only appears where the system can pass clicks through at all —
  under native Wayland it would do nothing. Should it fail against expectation,
  the setting is rolled back rather than storing an „on" that has no effect.

  Suggested by **Haldjas (pr0)** on 2026-08-28.

## v3.0.0-rc89 - 2026-08-28

### Fixed

- **The dropdown promised more than the list showed.** After the patch-history
  fix it read „4.10.0 (24)" — with three rows below it.

  Two causes, both the same kind of mistake:

  **Two sources for one question.** The dropdown counted the history, the
  filter checks the `seit` stamp in the catalogue. But the number in brackets
  is a promise about how many rows will appear. It now counts the catalogue —
  what is not stamped cannot be shown anyway.

  **And the stamp arrived too late.** It was only caught up during the network
  tick, which runs at some point after startup in its own thread. Measured on
  2026-08-28: window built at 10:44:02, catalogue stamped at 10:44:03 — one
  second too late, and the list stayed wrong until the next opening. The window
  now catches the stamp up itself, **before** it reads the catalogue. This hits
  every user on the first start after a build with new history.

## v3.0.0-rc88 - 2026-08-28

### Fixed

- **The patch filter lost almost the entire patch.** The dropdown read
  „4.10.0 (3)" and the list showed three ship weapons. In truth 4.10.0 brought
  **24** blueprints — the 21 shipped ones had vanished from the view.

  Cause: the program layered its own observed history on top of the shipped
  one. For the same game version, the local one won outright. But what the
  program records itself is only ever the **increase since the last run** —
  here three weapons the source added two days later. Read as a complete patch
  list, that is bound to be wrong.

  Both lists are now **merged** rather than replaced, and the earlier date
  wins. The same applied to two local findings in a row: the second erased the
  first. That is fixed as well.

### Improved

- **The diagnostic report now states the patch history.** A new line below the
  catalogue state: which game versions the history holds, and with how many
  blueprints — for example `4.10.0 (24)`.

  The bug above could hide because the report only showed the catalogue state.
  That was perfectly fine; the history below it was not. Anyone reporting „the
  patch filter shows almost nothing" now has the numbers right there, with no
  need to open a file first.

## v3.0.0-rc87 - 2026-08-28

### Improved

- **Confirmation dialogs now look like the rest of the program.** Three
  places still showed Tk's grey system box: a light panel inside a dark window,
  a foreign font — and narrow and tall, turning a longer sentence into a column.

  It is now a dialog of its own, in the same colours and with the same buttons
  as everywhere else, **wide rather than tall** (620 px), centred over the
  window. Enter means yes, Escape means no.

  Affects: switching the text source · sending a problem report · resetting the
  inventory.

  The requirement behind it: the dialog should carry the program's own design —
  and be wide rather than tall.


- **The "In-game text" page now follows the order you read it in.** The text
  source first — where the base text comes from — then what gets written into
  it: blueprint details first, then the details on the item itself. Previously
  the write switch sat above the source it depends on.

### Fixed

- **Dialogs had German text but English buttons.** Switching the text source
  showed "Einsetzen?" above buttons labelled **Yes** and **No**.

  Those buttons do not come from the program's own language file but from Tk's
  own table — which is incomplete on many Linux systems. Measured on
  2026-08-28: Tk's locale was already set correctly to `de_de`, yet the German
  words were simply missing from the installation. On Windows Tk ships them,
  which is why it never showed up there.

  The program now supplies the words itself, and updates them on a language
  switch instead of setting them once at startup.

## v3.0.0-rc86 - 2026-08-28

### Fixed

- **Asterisks showed up as plain text on the "In-game text" page.** The
  explanation of the text source read "after that the `**entire game**` is in
  that language" — asterisks included.

  The `**bold**` markup in the language file is meant for whoever reads that
  file; a Tk label cannot mix formats and simply displays it. The credits page
  already stripped it, the settings rows did not — the same job in two places,
  one of them forgotten. Both now go through the same function.

  Spotted in a screenshot of rc85. The self-test had missed it: it looked for German text in the English interface, not for
  markup. **It now checks for this too** — and the check was verified by
  putting the bug back in.

## v3.0.0-rc85 - 2026-08-28

### Fixed

- **On Linux, description texts were cut off instead of wrapping — pushing the
  switches out of the window.** Every page with body text next to a control was
  affected: "In-game text", "Inventory", "Report a problem". At small window
  sizes sentences ended mid-word, and the switches on the right could not be
  reached at all.

  The cause sits one level deeper than it looks. The function that ties line
  wrapping to the window width asks the label for its own border size. Depending
  on the build, Tk returns such a measurement as a number, as text, **or as a
  Tcl object** — and on the last one `int()` raises a `TypeError`. Only
  `TclError` and `ValueError` were caught, and a `TypeError` is neither. So the
  error escaped and ended the function **before** it could set the wrap width.
  The text stayed on one long line — exactly the state this function exists to
  prevent.

  Why it surfaced only now: the Tk in the Windows build returns these values as
  numbers, the Tk in the Linux AppImage as Tcl objects. The bug could not occur
  on Windows.

  Spotted during the first Linux test round after updating to rc84 — first by
  the cut-off text, then confirmed in the problem report: **50 out of 50** recorded errors came from this single line.

  Measurements are now read with Tk's own converter, which understands all three
  forms. The same trap was present at two further points in the wrapping code
  and was removed there as well.

- **Uninstalling left the autostart entry behind.** The registry kept pointing
  at a file that no longer existed — Windows tried to start it at every sign-in
  and failed silently.

  The reason: the entry is written in **two** places. The installer creates it
  when you tick "Start with Windows" during setup, and it cleans up exactly that
  case. But turning autostart on **inside the program** writes the same value —
  and the uninstaller knew nothing about it.

  Spotted while cleaning up after a test run. It is the same autostart that made the update fail earlier that morning (code 5) —
  it was only half handled at both ends.

  The uninstaller now always removes the value, no matter who set it. Only that
  one value — autostart entries of other programs are left alone.

## v3.0.0-rc84 - 2026-08-28

### Fixed

- **Updating failed when autostart cut in halfway through.**
  Measured while updating rc75 → rc83: the installer got halfway and then stopped with

      An error occurred while trying to replace the existing file:
      DeleteFile failed; code 5. Access is denied.

  The Windows Restart Manager was **not** at fault — it had done its job. The
  setup log shows the whole chain:

      05:43:47  Shutting down applications using our files. (forced)
      05:43:55  << the watcher is running again — parent process explorer.exe >>
      05:44:17  DeleteFile: The existing file appears to be in use (5).

  Eight seconds after the shutdown, **autostart** brought the program back up.
  Windows processes autostart entries with a delay after `explorer.exe` starts;
  if the shell had restarted shortly before (a crash, a fresh sign-in), that
  delay lands right inside the running installation. The proof is the **parent
  process**: `explorer.exe` — had the watcher restarted itself, something else
  would be there.

  Deleting the running program cannot win that race: the installer closes it
  **once**, and it never sees what comes back afterwards. On its own it only
  retries four times, one second apart.

  The installer now follows up immediately before copying and terminates a
  program that has come back — three times in short succession, so it also
  catches an autostart firing at that very moment. Only on **updates**; a fresh
  installation waits no longer than before.

### Changed

- **A switch that says "off" now actually turns things off.** Both switches on
  the "In-game text" page only stored the setting — the text file was left
  untouched until someone pressed "Write now" under "By hand". Anyone who
  turned the details off, restarted the game and found everything unchanged
  concluded the tool was broken.

  The status box above made it worse: it promised "changes take effect the next
  time you start the game" — precisely what was not true.

  Measured while testing: switch off, status line reported "off", and **1,217**
  details were still sitting in the text file. The same trap caught a second
  switch, even though the note sat right next to it — the bold part gets read,
  the smaller one does not. That
  settled it: a note in the small print is not a fix.

  Flipping a switch now takes effect immediately — off means gone, on means
  there. Nothing is lost: the original wording is remembered and restored
  exactly when the details are removed. If something does remain, the status
  box now says so instead of reporting "nothing is being written".


- **"Launch Star Citizen" no longer appears twice.** The "In-game text" page had
  its own section for it — even though the button sits permanently in the
  bottom left of the sidebar, reachable from every page. The section is gone;
  the sidebar button is unchanged.

## v3.0.0-rc83 - 2026-08-28

### Fixed

- **The report now says whether the blueprint notes are in the game.**
  The most common support case is "I can't see your notes in the game any
  more". Behind it is almost always the same thing: a translation update or a
  game patch rewrote the game's text file and silently threw the notes out.
  The tool has no way of noticing.

  Until now the report only said which text source was selected — whether
  anything was actually in place could not be read from it, only guessed. That
  is exactly what happened with **Morkhan** on 28 Aug 2026.

  Two lines are new: whether the notes are in place, whether writing them is
  switched on at all, whether they are refreshed automatically — and which text
  file is meant. Anyone playing on Linux without a translation gets **no**
  warning: there is no such file there, and that is the normal state, not a
  fault.

- **Text was cut off instead of wrapped — everywhere it got tight.**
  It showed up in one place: the English warning line on the Game page ("Every
  translation update and every game patch wipes the details.") stuck out by
  5 pixels and was silently clipped.

  The cause was not the text but a sum with a missing term. The wrap limit
  bounds the **text** only; what a label ends up occupying is text plus border
  plus padding. With the limit set to the full available width, the label
  needed a few pixels more than it was given — and Tk clips an oversized child
  at its parent without an error or any other sign.

  The border is now read from the widget itself rather than guessed, and
  subtracted. This applies to **every** place that wraps automatically,
  including those that just barely fit today and would have tipped over with
  the next longer string. Measured afterwards: nothing is clipped any more,
  across 11 pages × 2 languages × 2 window sizes.

## v3.0.0-rc82 - 2026-08-28

### Fixed

- **A contract with several payout tiers lost nearly all its blueprints.**
  Contracts sharing a text key overwrote each other while the catalogue was
  built — the last one read won, the rest were dropped. Measured against game
  build 4.10.0: **123 of 353** contract keys are shared, **319** contracts were
  dropped, and **797 blueprint entries** were never shown to anyone. The bounty
  contract listed 8 blueprints instead of 25.

  Found by **Morkhan**, who kept pushing: "I still don't get shown which
  blueprints I can get from the beginner contract, only the ones from the
  highest tier." It wasn't the highest tier — it was the last one read. All
  tiers are now merged.

- **A catalogue already on disk would never have picked up this rebuild.** It
  was only refreshed when Star Citizen shipped a new version. It now carries
  its own build number — if its structure changes, it is rebuilt, patch or no
  patch.

### Changed

- **The heading now reads "POSSIBLE BLUEPRINTS FOR THIS MISSION TYPE".** It
  previously said "BLUEPRINTS FROM THIS CONTRACT" — promising more than the data
  can deliver. Read literally, you accept the contract and get nothing. Morkhan
  on 28 Aug 2026: "it's confusing no matter how you turn it." He was right, and
  the confusion sat in the heading, not in the list.

  The SC Deutsch Launcher words it the same way for the same reason — 367 times
  in its data file.


- **The `[BP 3/12]` count in the title is gone; it now reads just `[BP]`.** The
  number looked useful but was not true: a contract's list merges all payout
  tiers, and which of them your own tier grants cannot be resolved — 123 of 353
  contracts share their text key across tiers. "3 of 12" really meant "3 of 12
  that someone, somewhere, can get". The same number is gone from the list
  heading too.

  What remains is the honest part: **ticked means you have it** — regardless of
  whether this tier grants it, or where it came from.

- **Where tiers differ, the required rank is shown behind the blueprint.** For
  example "needs Head Contractor (38,000 XP)" next to plans only available far
  up, while others from the same contract drop from 800 XP. Shown only where it
  actually tells blueprints apart — if they all need the same rank, it is
  already stated above under "Min. reputation".

- **Contracts with tiers that grant nothing now say so.** "Note: 1 of the 3
  tiers of this contract give no blueprints at all."


### Changed

- **The „Diagnostics" tab is now called „Report a problem" and carries red.**
  Nobody looks under „Diagnostics" when something is stuck — least of all
  inside a collapsed menu, where it used to sit.

  The red works in two stages so that it means something: **the word is always
  red**, so the tab can be found. **The icon only turns red when errors have
  actually been recorded** — otherwise the watcher would sit on permanent alert
  while everything is fine, and nobody would take the colour seriously.

### Fixed

- **Revisiting a page left no trace in the report.** It was only written while a
  page was first built; if something went wrong on a later visit, the line was
  missing entirely rather than half — and the report promises that the last line
  without „ready" is where it stopped. It now says „showing", so you can tell
  „died while building" from „died while showing".
- **The error report only scrolled once the page was at the bottom.** The mouse
  wheel went to the page behind instead of the text field under the pointer, so
  you had to push the whole diagnostics page down before anything moved inside
  the report. Now whatever sits under the pointer scrolls, the way browsers do
  it. Reported by **Morkhan**.
- **The send button is red all the time**, not only on hover — a warning button
  you only see once the mouse is on it warns nobody.
- **The second reporting route is now called „GitHub issue"** instead of
  „Report a problem". Two buttons promised the same thing, while one opens the
  browser and needs a GitHub account.

## v3.0.0-rc81 - 2026-08-28

> **One button instead of nine steps: send the error report.**

### Added

- **The diagnostics page now sits in the main sidebar**, right below
  „Server status“ — no longer inside the collapsed „Advanced“ menu. Anyone
  who needs it has a problem, and will not look for it under a heading that
  reads „not for me“.
- **A red „Send error report" button.** If something is stuck, you press it —
  and the report is with the developer. No copying, no hunting for the right
  channel, no „message too long".

  It used to take nine steps: expand, copy, find Discord, paste, discover it is
  too long, save as a file, find that file again, upload, send. Now it takes
  one.

  **You see exactly what goes out beforehand** — the same text shown on the
  page, in a window to read through, and only then are you asked. Names, paths
  and credentials have already been stripped. Nothing happens without your
  yes.

## v3.0.0-rc80 - 2026-08-28

> **Blueprints from the launcher get ticked off again — existing collections migrate themselves.**

### Fixed

- **Blueprints from the launcher or a backup were not ticked off.** Anyone
  bringing their collection over from the SC Deutsch Launcher, the KRT Profit
  Basetool, scmdb.net or their own backup saw empty boxes in the list — even
  though the blueprints were in the collection.

  The reason: names from those sources often carry the class suffix
  (`XL-1 (Mil/2/A)`), but it was only stripped when reading the game logs. So
  `xl-1 (mil/2/a)` and `xl-1` stood there as two separate entries and never
  found each other. That now happens centrally, no matter where a name comes
  from.

  This hit precisely those who have been playing longer and bring their
  collection with them. Found while following up a report from **Morkhan**.

  **Existing collections migrate themselves on first start.** The keys are
  rebuilt once and duplicate entries merged — the older find wins, because when
  a blueprint first turned up is the date that matters. Nothing is lost, nothing
  has to be done by hand.

- **The tool did not say that changes only take effect the next time the game
  starts.** Star Citizen reads the text file **once, while launching**. Anyone
  with the game running would install the details, read „in place (1608
  spots)" — and see nothing in game. The obvious conclusion: broken. The note
  now sits in the success message itself and in the status box under *In-game
  text*.

## v3.0.0-rc79 - 2026-08-28

> **Three finds from Morkhan's questions — one would have silently swallowed blueprints.**

### Fixed

- **Blueprints whose name carries a suffix stopped being ticked off.** Now that
  item details are written in, the game puts the name **including the suffix**
  into its log — `Blueprint received: Spectre (Sth/1/A)`. Only the five faction
  suffixes were stripped; everything new stayed stuck to the name, and the
  blueprint went into the collection under the wrong one. **344 weapons and 62
  missiles** would have been affected — and nobody would have noticed, because
  something was still being displayed. Found while following up a question from
  **Morkhan**.

- **A mission promised „12 blueprints" in its title and showed none below.**
  A mission has **more descriptions** in game than the catalogue knows —
  different destinations and cargo for the same mission. Measured:
  `Covalex_HaulCargo_SingleToMulti` lists three descriptions in the catalogue,
  the game's text file holds **eight**. Anyone hitting one of the other five saw
  the counter and nothing underneath. The route via the SCDL team's contract
  data had long solved this; our own route via the blueprint catalogue had not.
  Reported by **Morkhan**.

### Added

- **An exclamation mark in the contract title when blueprints come with
  conditions.** `[BP 0/19!]` instead of `[BP 0/19]`. In **332 of 818 contracts**
  (41 %) blueprints only drop at certain payout tiers or from a given rank —
  „only for the 256,500 / 264,000 aUEC mission", „only from Master rank". That
  was in the description text, but the contract list only showed the counter,
  and that is what you decide on. Reported by **Morkhan**, who flew a hauling
  mission repeatedly in which none could ever drop.

  ⚠️ Why it cannot be cleaner: all payout tiers of a mission share **one**
  description text in the game. Star Citizen shows the small variant the same
  text as the large one — there is no way to tell them apart.

## v3.0.0-rc78 - 2026-08-28

> **Passing clicks through to the game is no longer a one-way street.**

### Added

- **A lock on the overlay brings you back when clicks pass through to the
  game.** Until now this was a one-way street: turning the setting on made the
  overlay unreachable — no button, no bar, and certainly not the settings
  themselves. The only way back was starting the program a second time. Which
  means leaving the game — exactly what the setting is meant to avoid.

  There is now a small lock at the top right of the overlay, the one thing that
  stays clickable. One click and the overlay catches clicks again. It only
  appears when clicks really do pass through, and disappears by itself — also
  when you switch it over in the settings.

## v3.0.0-rc77 - 2026-08-27

> **„Original texts from the game" now works without a helper program.**

### Fixed

- **Choosing the „Original" text source often ran into a wall.** That source
  takes the English `global.ini` straight from your own `Data.p4k` — no
  download, no third-party translation. CIG compresses that file with **zstd**,
  though, and the bundled Python could not handle it. What was left was a
  message asking you to install 7-Zip — quite something for a tool you just
  download and run.

  The program now brings the decompressor along itself. This mainly affected
  anyone **playing in English who only wants the item details**, without a
  translation: for them this route was the only one.

  If you installed 7-Zip solely for this — you no longer need it.

## v3.0.0-rc76 - 2026-08-27

> **The tractor beam now tells you what you are looking at — and on Windows
> there is only one route left.**

> [!important]
> **Windows: the installer is the only download now.** The standalone
> `SC-BP-Watcher.exe` is no longer attached to releases as of this version.
>
> The reason concerns you, not us: an update used to place the new version
> **beside** the old file instead of replacing it. Anyone clicking their usual
> shortcut afterwards kept using the old version for months without noticing.
> With the installer that cannot happen.
>
> **If you have been using the standalone file:** download
> `SC-BP-Watcher-Setup.exe` once and install over it — your blueprint
> collection stays, it lives elsewhere anyway. You can delete the old file
> afterwards. Nothing changes on Linux.

### Fixed

- **On Windows there is only one download now: the installer.** The standalone
  `SC-BP-Watcher.exe` is gone.

  **What you get out of it:** no more wondering which of the two files is the
  right one. The watcher ends up in your start menu instead of sitting
  somewhere in your downloads folder. Updates genuinely replace the program
  rather than putting a second copy next to it — the most common reason someone
  keeps using an old version for months without noticing. Autostart is a
  checkbox during setup, and *Apps & Features* removes everything cleanly.

  The standalone file dates from the early days: an unsigned program without an
  installer looks less alarming, and the point back then was to earn trust at
  all. That is done — and two routes side by side mean twice as many places
  where something can go wrong. Better one route that works.

  Nothing changes on Linux: the AppImage stays.
- **Anyone still on v2.0.0 comes along anyway.** Their update path picks the
  first file ending in `.exe` — which is now the installer — and starts it
  afterwards. So it runs by itself and sets everything up properly. The
  blueprint collection moves across automatically on first start.
- **An update now installs where the program already is** — instead of putting a
  second copy beside it. v2.0.0 shipped only as a bare `.exe`, so all of its
  users run „portable" without ever choosing to. Without this, the installer
  would have gone to `%LOCALAPPDATA%\Programs` on the update after next and left
  the old file behind — anyone starting it from a shortcut would have kept
  using the old version forever.

### Added

- **Details on the item — class, size and grade now sit next to the name.**
  Aiming at something with the tractor beam used to show just „Glacier". It now
  reads **„Glacier (Mil/1/A)"** — military, size 1, grade A. Missiles are judged
  by something else, so they carry their seeker instead: **„'Arrow' I Missile
  (IR1)"** for infrared, `EM` for electromagnetic, `CS` for cross-section.
  Nobody expands a description mid-fight.

  **856 items** get such a note: 450 with class, size and grade, 344 weapons
  with their class (ballistic, laser, plasma …) and 62 missiles.

  The details come from the game's **own** text file — they have always been
  there, just inside the description you have to open first. The tool merely
  moves them to where you can actually see them.

  Suggested by **Morkhan**.

  Can be switched off under *In-game text → Details on the item*. To undo it,
  use „Remove again" — the original names come back to the character.

## v3.0.0-rc75 - 2026-08-27

> **The startup trace is back in the report.**

### Fixed

- **Usage pushed the startup trace out of the report.** rc74 wrote startup steps
  and page switches into one list, and the report only shows the last twelve
  lines — five clicks were enough to hide the entire startup. Precisely the part
  the trace was built for. Both now appear as **two separate sections**, each
  capped on its own; trimming the file keeps the startup part as well. Found in
  the first rc74 report, fifteen minutes after release.
- **The diagnostics page was the last line of its own report.** The report is
  built while that page is being drawn, so every trace ended with "Page
  diagnostics: building" and looked as if that was where it stopped. Those lines
  are now left out.

## v3.0.0-rc74 - 2026-08-27

> **A crash now leaves a trace.**

### Added

- **Hard crashes are recorded.** Until now the tool only caught Python errors.
  A crash that kills the process mid-instruction (from inside the Tk library,
  say) left **nothing behind**: no entry, no message, nothing to attach. From
  now on a handler writes the call path of every thread to a file, and the next
  diagnostic report shows it under "Hard crash during the previous run".
- **The trace now covers usage, not just startup.** It stopped after the last
  startup step — which page someone opened was recorded nowhere. Every page
  switch now writes two lines. If the second one is missing, it broke while
  building exactly that page. The file is capped so it cannot grow forever.

### Notes

- **The crash Bomb20 reported when opening "What's new" is not fixed by this,
  it is measurable.** It could not be reproduced here, and his report could not
  show it at all — that is the gap rc74 closes. If it happens again, it will be
  in the next report.

### Thanks

- **Bomb20** (pr0) — for a report that turned out to be about something
  bigger than a single crash: the tool was blind at that spot. And for sending
  it even though it looked like a false alarm.
- **Haldjas** (pr0) — for the counter-test on Windows: the
  update from rc71 to rc73 and the interface since rc61, both without findings.

## v3.0.0-rc73 - 2026-08-27

> **The thanks page now says what actually happened today.**

### Changed

- **The "Thanks & licences" page in the tool lists Bomb20's findings from
  today.** It still showed only his contribution from 25 Aug, while over this one
  morning he uncovered three bugs that would have hit **every** user on release
  day: the launch button for Star Citizen, the aborted download, and the restart
  that never came.
  - The thanks were properly recorded in both changelogs — but nobody sees those
    inside the tool. **Anyone missing from the tool has not been thanked.** The
    release checklist now names this third place explicitly.

### Confirmed

- **The restart after an update works** — verified on a second machine (CachyOS),
  from rc71 to rc72, without a single entry in the error log. So it does not
  depend on any quirk of one installation.

### Thanks

- **Bomb20** (pr0) — for a morning in which he sent three reports even
  though he actually had to work, and for his patience while his reports were
  first taken for user error. They never were.


## v3.0.0-rc72 - 2026-08-27

> **The update page now tells the truth** — it checks by itself, and the route to
> the stable version is no longer a dead end.

### Fixed

- **The page showed an outdated version number as long as it stayed open.** It
  asked **once per page build**. Anyone with the page open while a new version
  appeared kept seeing the old number on the button — and assumed they were up to
  date. Reported by **Bomb20** (pr0): "I still get 67 shown", while rc68
  had been published minutes earlier. It now checks every five minutes while the
  page is open.
  - Five minutes is the compromise: often enough that nobody misses a version,
    rare enough for GitHub's limit of 60 requests per hour.
- **The "Stable version" box was a dead end.** Instead of a button it said "First
  press 'Check now' above" — anyone wanting the stable version saw no route, just
  homework.
  - **The cause was too small a query:** the last **20** releases were fetched,
    and among 83 published releases not a single one of those was stable — only
    test versions. Now 100 are fetched (the most GitHub returns in one query),
    and it stays **one** request: the hourly limit counts requests, not entries.
  - Measured: 20 releases → 0 stable, 100 releases → 3.

### Thanks

- **Bomb20** (pr0) — for "I still get 67 shown". It sounded like a
  triviality and pointed at two bugs at once.


## v3.0.0-rc71 - 2026-08-27

> **The restart after an update works** — the cause was entirely different from
> what everyone assumed.

### Fixed

- **After an update the watcher shut down and never came back.** Reported by
  **Bomb20** (pr0) in the morning, reproduced here all through
  the day. Three attempts (rc67, rc68, rc70) failed to solve it, because they
  assumed the new version was crashing.
  - **It was not a crash.** The new version starts, finds the single-instance
    guard still occupied, considers itself the **second** instance and exits as
    designed — with return code 0. A cleanly exited process looks exactly like a
    crashed one afterwards, until someone reads the return code.
  - **Why the port stayed occupied:** the guard is closed with `close()` before
    the restart. But that does not wake the thread waiting in `accept()` — it
    stays blocked, the descriptor stays valid, the port stays taken.
    `shutdown()` aborts the waiting `accept()`; only then does `close()` actually
    release the port.
  - Proven, not assumed: the probe previously failed with `Address already in
    use` and now goes through. Self-test section 24 keeps it that way.

### Thanks

- **Bomb20** (pr0) — for the first report and for not letting go when it
  looked like a user error. He was right, we were not.


## v3.0.0-rc70 - 2026-08-27

> **If the restart fails, the report will now say why.**

### Fixed

- **`'Overlay' object has no attribute '_dx'` when dragging the overlay.** Tk
  does not always deliver a mouse motion after a click on the same window:
  press the button outside and drag into the overlay, and only the motion
  fires — leaving no starting point. Dragging did nothing once, and the error
  landed silently in the log. Reported by **Bomb20** (pr0, 25 Aug 2026 on
  rc18) and again on 27 Aug 2026 on rc69 — never fixed in between, because
  it breaks nothing you can see.

### Changed

- **A failed restart now leaves a trace.** The error output of the freshly
  started version used to go to `/dev/null` — which is why "it shuts down and
  never comes back" could not be diagnosed: the report contained **nothing** about
  it. It is now captured, and if the new version does not come up, its last words
  are attached to the error log and thus to the report.
  - This is not a fix but a measurement. After two attempts that did not solve
    the restart, there will be no third guess.

### Thanks

- **Bomb20** (pr0) — for the drag error that sat in reports for two days
  without anyone taking it seriously.


## v3.0.0-rc69 - 2026-08-27

> **For some, the update was never downloaded at all** — the progress display
> was to blame.

### Fixed

- **Click "get version", and nothing happened.** No progress, no restart, no
  message — after a restart the old version was still running. Reported by
  **Bomb20** (pr0): "I clicked get 68, but nothing came up about restart
  or install."
  - **The cause was the display, not the download.** Downloading runs in its own
    thread that reports progress to the window. That call can throw
    (`RuntimeError: main thread is not in main loop`) — and the exception took
    the **entire thread** with it, on the very first percent step. Bomb20's
    report showed the error three times, once per click.
  - Drawing is incidental, downloading is the point. Every display call in the
    update thread is now wrapped: if it fails, that is recorded and the work
    carries on.
- **"Check for updates" wrongly gave the all-clear.** Bomb20 was told "you have
  the latest, rc67" while rc68 had been published two minutes earlier. GitHub
  allows only **60 requests per hour per address** anonymously; anyone clicking a
  lot in one morning runs into it. The request failed — and was swallowed
  silently, so the old state was used instead.
  - "Nothing new" and "could not check" are opposites and are now kept apart.
    When the hourly limit is reached, the message says so and that it will work
    again within the hour.
  - **A check button that wrongly gives the all-clear is worse than none.**

### Thanks

- **Bomb20** (pr0) — for the third diagnostic report of the morning, sent
  at exactly the right moment. Without it, "nothing came up" could not have been
  told apart from "the download is stuck"; with it, the cause was there in one
  line.


## v3.0.0-rc68 - 2026-08-27

> **The update button is where you look for it** — and "Fassung" is now called
> "Version" throughout the German interface.

### Changed

- **The "Get the latest version" button now sits at the very top**, right below
  the version card. Previously it came after the button row and the daily
  toggle, which put it **below the edge** at the window's minimum size — someone
  who cannot find it will not update.
  - Making the window taller would have been the wrong answer: on a 1366×768
    laptop it would no longer fit at all. The most important button belongs at
    the top, not the window in the sky.
- **Both channel boxes are fully visible at minimum size too** — they hold the
  button that fetches the stable version specifically. The daily toggle moved
  below them; it is a side setting, the boxes are the point of the page.
- **"Finished versions only" is now "Stable version".** "Finished" sounds like
  something that is done — this tool is under continuous development.
- **"rcXX is already there" is now "rcXX is already installed"** — clearer, and
  the English string already said so.

### Thanks



## v3.0.0-rc67 - 2026-08-27

> **The restart after an update works on Linux** — and can no longer fail
> silently.

### Fixed

- **After an update the watcher shut down and never came back.** It downloaded
  the new version, installed it, closed itself — and stayed closed. Reported by
  **Bomb20** (pr0) with the decisive sentence "it does shut down but
  doesn't start", reproduced the same day on a second machine.
  - **The cause:** when starting the new version, only `APPIMAGE`, `APPDIR`,
    `OWD` and `ARGV0` were removed from the environment — `LD_LIBRARY_PATH`,
    `PYTHONHOME` and `PYTHONPATH` stayed. Inside an AppImage those point into the
    **extracted mount of the old version**. Two seconds later the old one exits,
    its mount disappears, and the new one looks for its libraries in a directory
    that no longer exists. It dies before a window appears.
  - The proper cleanup already existed (`saubere_umgebung`); the restart just
    carried its own incomplete copy. Both now live in `scbp/pfade.py` — **one**
    cleanup, used by everyone.
- **And it can no longer fail silently.** The old version only steps aside once
  the new one has survived its first seconds. If it dies, the watcher stays open
  and says so: "The new version did not come up." Previously the old one closed
  dutifully while the new one was already dead — leaving the machine without a
  watcher and without a word of explanation.
  - Same lesson as the launch button in rc65: **starting a program does not mean
    it is running.** `Popen` reports success as soon as the process exists.

### Thanks

- **Bomb20** (pr0) — for sticking with it. His matter-of-fact "it does
  shut down but doesn't start" pinned down the bug after it had first been
  dismissed as a user error. He was right, we were not.

## v3.0.0-rc66 - 2026-08-27

> **The export files keep themselves up to date** — and the file chooser finally
> looks like the system it runs on.

### Added

- **The export folder is updated with every new blueprint.** Until now the three
  files (KRT Profit Basetool, scmdb.net, full backup) were only written on a
  button press — anyone who had clicked once assumed they were current, while
  they stayed frozen at the moment of that click. Writing is now tied to the
  inventory itself: every find in the game, every catch-up at startup, every
  confirmation from the launcher and every import carries the files along.
  - **Fixed file names in the folder.** With a date in the name, three new files
    would appear there every day and nobody would know which one is current. The
    save dialog still suggests a name with a date — saving by hand means
    deliberately preserving a state.
  - **Previously stored dated files move to `Ältere/`** — moved, not deleted.
    Anything else in the folder is left alone.
- **A save button per format**, right next to the format, instead of one shared
  button further down.

### Fixed

- **"Save individually …" always saved the Basetool format.** The format was
  hard-coded; scmdb and the full backup were not reachable through the dialog at
  all.
- **The file chooser on Linux was the old Tk box** — a column list showing every
  hidden folder, no sorting, no preview. It now opens the desktop's own dialog
  (`kdialog` on KDE, otherwise `zenity`), everywhere a file or folder is chosen:
  import inventory, save inventory, game folder, launcher folder, own folder and
  the setup assistant. If neither is present, the Tk dialog remains as a
  fallback — **nothing depends on it.** Nothing changes on Windows and macOS,
  where Tk already passes through the real system dialog.
  - Folders already had this path; files did not. Both now live in one place
    (`scbp/dateiwahl.py`) instead of three.


### Thanks


## v3.0.0-rc65 - 2026-08-27

> **The launch button called the wrong program on Linux.**

### Fixed

- **The "Launch Star Citizen" button started nothing on Linux.** It said
  "Launching Star Citizen …" and then nothing happened — without any error. It
  called `lug-helper`, which **cannot launch the game at all**: it manages the
  Wine prefix, runners and DXVK, and has no launch option. The watcher now uses
  the `sc-launch.sh` launch script the helper creates inside the prefix, and
  finds it via the game folder (one level above `drive_c`) — no matter where
  someone installed it. Reported by **Bomb20** (pr0).
  - No more fallback to `lug-helper`: it would be found, the button would
    appear, and it would do nothing again. Anyone playing through Lutris or
    Heroic still enters their launch command in the `spielstarter` setting.


### Thanks

- **Bomb20** (pr0) — for reporting that Star Citizen could not be launched
  from the tool, and for the patience of sending two diagnostic reports in one
  morning. Without the second one it would not have come out that `lug-helper`
  cannot launch the game at all.

## v3.0.0-rc64 - 2026-08-27

> **The rebuild eats the message** — the same trap three times, in three
> different places.

### Fixed

- **"Check for updates" still reported nothing.** The rc63 crash was gone but no
  answer appeared: the button stayed on "Looking for a new version …".
  `neu_aufbauen()` destroys **every** child of the window — including the footer
  the message lives in. It was set and torn down milliseconds later. It now
  rebuilds first and reports afterwards.

- **Same trap after updating on Linux.** "Ready — restart now" was said at
  `after(0)` and swept away at `after(50)`. Order swapped.

- **At "very large" half the sidebar was missing.** "Launch Star Citizen", "Buy
  me a coffee" and "Discord" dropped out of the window — they are packed from
  the bottom, and whatever does not fit between tabs and footer falls out. The
  window's minimum size depends on the sidebar height, which depends on the
  font. The program always calculated this correctly; the calculation simply
  never ran after a font or language change. It is now part of the rebuild.

- **The two boxes under "What do you want to hear about?" were unequal.**
  `pack(expand=True)` distributes only the **surplus** evenly — whichever has
  more text stays wider. They now sit in a `grid` with `uniform`, the only
  guarantee in Tk that makes two columns truly equal; measured 545 px to
  545 px, same height.

- **At "very large" the buttons were cut off.** A named Tk font applies to every
  text instantly — but the drawn round buttons fix their canvas to the measured
  text width **once**, at build time. Measured on the overlay choice: canvas
  177 px, text 206 px, **29 px short**. Changing the font size now rebuilds the
  interface — as the language switch has always done — so every canvas measures
  anew.

### Notes

- **Self-test section 21.** Checks both halves: that a finished round button
  really does not grow on its own (otherwise the second check would pass
  vacuously), and that the font switch rebuilds **and then** reports.

## v3.0.0-rc63 - 2026-08-27

> **"Check for updates" checks again** — and the notice before an update finally
> shows up.

### Fixed

- **"Check for updates" answered with `name 'datei' is not defined`.** The
  button did not hold the *look* routine but the *fetch* one — download,
  install, step aside — using two variables that never existed in that
  function. Whether a new version was out or not, the status line said it had
  not worked. The button now reports what it finds: the version — or **"You
  have the latest version."** That sentence existed all along; nothing ever
  showed it.

- **The notice before an update never appeared, not once.** Since rc52 the
  watcher is meant to announce that it will close, run the installer and needs
  a double-click afterwards — a program that vanishes without a word looks like
  a crash. The dialog sat in that same dead function. It now runs in the real
  update, before installing, and the installer waits until it has been read.

- **The export folder never opened.** `os.startfile()` in the inventory window
  used an `os` that was never imported there, and the error fell silently into
  an `except Exception`. During the folder migration `t(...)` was used instead
  of `sprache.t(...)`, so the success message went missing. Both found by the
  new check below, not by hand.

### Notes

- **The self-test now looks for names that do not exist** (section 20, via
  `pyflakes`). This class of bug otherwise surfaces only on a **click**: Python
  resolves names at runtime, and when the callback ends in an `except`, nobody
  sees it. The check found three cases straight away. It runs in the build
  pipeline before every release; if `pyflakes` is missing on a dev machine it
  is skipped rather than failing.

### Changed

- **The ⓘ at the right edge of the blueprint list is bigger** — it opens the
  origin panel and was hard to recognise as a control at pure line size. New
  size set `ANTIPPBAR`, one step above the other in-line marks: 16 px instead
  of 14 at "normal", 22 instead of 18 at "very large". The status dots in the
  overlay are unchanged — nobody clicks those.

## v3.0.0-rc62 - 2026-08-27

> **The patch filter shows again what the patch brought.**

### Fixed

- **The patch filter found nothing and "new in game" stayed empty.** Anyone who
  used the Watcher before rc55 has a catalogue without origin stamps — stamping
  only happened on a rebuild, and a rebuild only happens on a new game version.
  So the dropdown showed "4.10.0 (21)" (it reads the history directly) while the
  list below said "Nothing found". The stamps are now filled in at startup, with
  no rebuild and no network needed.
- **The next patch would have been silent.** The comparison baseline
  (`bauplaene-gesehen.json`) also arrived only with rc55. Without it the rule
  "very first catalogue build — nothing is new" kicked in, and the next patch
  would have reported **zero** additions. If the file is missing, the existing
  catalogue is now used as the baseline: whatever is in it was in the game
  before.

### Notes

- **The self-test now covers this case** (section 19, eleven new checks). It paid
  off immediately: the catch-up ran *behind* the `SC_BP_NO_NET` network switch at
  first — anyone starting without a network would never have got a stamp, even
  though both history and catalogue sit on disk.

## v3.0.0-rc61 - 2026-08-27

> **The Discord announcement now says what it is about.**

### Added

- **The Discord release announcement is now a readable card.** Instead of
  `[Repo] New release published: v3.0.0-rc60` it shows the changelog section for
  **this** build — the same text the tool shows under "What's new". Test builds
  in gold with a "less thoroughly tested" note, finished ones in Xharig green,
  plus the program icon. after comparing with the
  StarStrings channel. Without a stored key nothing happens and the build stays
  green — a chat message must never turn a finished release red.

## v3.0.0-rc60 - 2026-08-27

> **What the diagnostics report revealed.** An invisible cross, eight errors per
> page switch — and a new check that finds both in advance from now on.

### Fixed

- **Eight log entries on every page switch.** `invalid command name …!label` —
  callbacks that adjust the line wrapping ran after their label had been
  destroyed. Nothing was visible: the hook in `fehler.py` caught them, they only
  filled up the report and buried what actually mattered. The same trap sat in
  the button row and in the drawn-border entry field; all three now check whether
  their widget still exists. Measured: 39 page switches, **0** errors.

- **The cross that closes the source box was invisible.** In the blueprint list
  it left an empty gap: the `schliessen` symbol only existed at button size while
  it was used at row size. `zeichen.bild()` silently returns `None` for a missing
  file — deliberately, so a missing symbol never halts the program, which is
  exactly what hid the bug. `tools/oberflaeche_pruefen.py` now checks for it.

## v3.0.0-rc59 - 2026-08-27

> **The readme is accurate again.** All screenshots redone, a separate set
> per language, and every symbol in them comes from the program's own set.

### Added

- **The coloured dots were still emoji in the running text.** The symbol key
  already showed the real images while the description below it kept using
  `🟢 🟡 🔵 ⭐` — two different renderings of the same symbol on one page.

- **The English readme now shows the English interface.** Until now it presented
  German screenshots — with eleven images, and a tool whose Linux users mostly
  run the English client, that is not a detail. `tools/sprachen_pruefen.py` now
  checks for it: it only counted sections and never looked at images.

- **Every screenshot in the readme is new.** The old ones were from v3.0.0-rc11
  and showed not just the replaced symbols but a build without the server status
  tab and without the patch filter. Two pages got their first screenshot at all:
  **Server status** and **Thanks & Licenses**.

- **The feature table in the readme used emoji instead of the real symbols.**
  `⚡ 📋 🧭 ⭐ 🔔 …` have nothing to do with the program's icon set and look
  different on every system. All sixteen now come from the same set as the
  interface.

- **A screenshot exposed the author's home path.** `screenshot-pfade.png` had
  been in the repo since v3.0.0-rc11, showing `/home/<user>/` three times — the
  very thing `pfade.kuerzen()` strips from error reports. Removed; the folder
  page gets no screenshot at all, since it necessarily shows paths. The server
  status tab took its place.

### Fixed

- **The filter buttons on "What's new" stayed German in English.** "Alles / Neu /
  Verbessert / Behoben" were hard-coded instead of living in `sprache.py` — right
  next to a properly translated changelog. Spotted on a screenshot of the English
  interface.

## v3.0.0-rc58 - 2026-08-27

> **What belongs to whom — in one place.** A new "Thanks & Licenses" tab that
> brings the licences and the people together. Plus names and symbols that
> finally match what they do.

### Added

- **The "Mission text" tab is now "In-game text".** The old name did not say
  **where** those texts appear.
- **The program icon now sits next to the version on "Update & About".** The page
  had no image at all after the author block moved to "Thanks & Licenses".

- **The readme showed symbols the tool no longer has.** The button legend in
  both readmes listed `☰`, `ⓘ`, `⟳`, `⏻` and `🗑` — two of them are long gone,
  the others look different now. It now shows the **actual image files** from
  `assets/symbole/`, so it can no longer go stale: swapping a symbol updates the
  readme picture by itself. Same for the message symbol key.
- **"Who built this" suddenly appeared twice.** The block naming the author,
  scmdb, the SC Deutsch Launcher and StarStrings sat on "Update & About" — and
  the new "Thanks & Licenses" page listed the same projects again. It now lives
  only on "Thanks & Licenses", with the author **at the top**: a page listing
  other people's work has to name its own first.

- **The donation link was nowhere to be seen on GitHub.** The "Buy me a coffee"
  button has been in the tool for a long time — but the project page itself had
  nothing: no sponsor button, no mention in the readme. Anyone who had not
  installed the tool yet could not find it at all. Both are there now.

- **New "Thanks & Licenses" tab** under *Info*. Until now the program showed
  **no licence information at all** — neither its own (GPL-3.0) nor that of the
  bundled symbols, and third-party projects were only mentioned in passing where
  they happened to be used. There is now one place stating what belongs to whom:
  the program itself, the Lucide symbols, the scmdb data, StarStrings and the SC
  Deutsch Launcher — each with its licence and a clickable link. Plus thanks to
  the people whose feedback turned into something.

## v3.0.0-rc57 - 2026-08-27

> **One icon set instead of fourteen glyphs.** The symbols in the notification
> bar had different sizes, mixed styles, and looked different on every operating
> system. Replaced with rendered images from a single, consistently drawn set.

### Changed

- **All symbols are the same size now — and come from one set.** The glyphs in
  the notification bar had different sizes, the bell being the largest. Three
  causes with the same root: *the font decided, not the program.* A glyph fills
  only 50–70 % of its box, each one differently; `🗑` and `▶` are solid shapes
  while `⚙ ⟳ ✕` are thin strokes; and every operating system picks a different
  fallback font. Replaced with rendered images from the **Lucide** set — all
  drawn on a 24×24 grid with the same stroke width.
- **The interface now looks identical on Windows, Linux and macOS.** It did not
  before: Windows used `Segoe UI Symbol`, other systems something else. Anyone
  developing on a Mac saw different glyphs than their users on Windows.
- **The coloured dots in front of blueprints are no longer emoji.** `🟢 🟡 🔵 ⭐`
  live outside the basic plane; Windows rendered them through the colour emoji
  font as coloured blocks that **ignored** the configured colour — in the very
  place you look at most often.
- **Launching Star Citizen now shows a rocket instead of a play arrow.** A `▶`
  means "play video" everywhere, not "start a program".
- **Clearing messages now shows an eraser instead of a bin.** The button deletes
  nothing — it only tidies the display, the blueprints stay. A bin promises
  destruction and puts people off clicking it.
- **"Setup" is now "Run setup".** A verb says something is about to happen; the
  noun alone sounded like a place to look things up.
- The height of the notification bar now grows with the configured font size. It
  was fixed at 26 pixels, which made symbols stick out at "large".

### Removed

- **The autostart switch is gone from the notification bar.** A power symbol
  means "turn the device off" everywhere, and it sat right next to the cross
  that really does close the program — two buttons that both looked like "off".
  The setting is unchanged under "General".
- **The setup assistant button is gone from the notification bar.** It remains
  available in the main window, top right — the settings are where everyone goes anyway once they notice something is off.

### Fixed

- **A help text pointed at a glyph that no longer existed.** "Use ☰ to open the
  blueprint list at any time" was still in the setup assistant, even though `☰`
  had been replaced by the clipboard back in v3.0.0-rc55. All texts now name the
  symbols in words instead of depicting them.

### Thanks


## v3.0.0 - 2026-08-29

> **One window for everything.** The blueprint list and the settings used to live in
> two separate windows, and you had to know which one held what. They are now together —
> tabs on the left, a visible folder for your files, and an installer instead of
> dragging a file somewhere by hand.

### The short version

- **The list shows what the patch brought into the game.** Next to "watching"
  there is now **🔵 new in game**. The catalogue stamps every blueprint with the
  game version it first appeared in; the filter shows the current patch. When the
  next one lands, the new ones move in and the old ones drop out — but the stamp
  stays, so you can still tell which patch a blueprint came with. A **patch
  dropdown** next to the other filters lets you look up any earlier patch, and it
  extends itself as patches arrive. 4.10.0 added 21.
- **A patch history of its own**, so that number is actually right. Comparison
  now runs against **every blueprint ever seen**, not against last week's
  catalogue. The first attempt reported 74 additions, 53 of which had been in
  the game for ages — the data source simply had not listed them for a while.
  And it could not be checked afterwards: scmdb only keeps the current game
  version, and the 4.9.0 data was already gone the same day. So the tool now
  records what each patch brought (`daten/patch-historie.json`, readable in the
  repo) — additions only, never the whole catalogue.
- **An installer for Windows** — download, run, done. No more moving files around.
- **One window instead of two**, with tabs on the left. Plus a tray icon to bring
  it back whenever you need it.
- **The overlay can step aside** and only appears when something is found — a
  narrow green strip stays at the edge, and the mouse brings it back.
- **Self-update now works on Linux too.** It used to fail there **every single
  time**; anyone on the AppImage had to fetch each version by hand.
- **Star Citizen can be launched from the tool**, and a diagnostic report collects
  everything a bug report needs at the press of a button — no names, no paths.

### Upgrading from v2.0.0

- **Your blueprint collection moves along by itself.** It used to sit hidden in
  `%APPDATA%`, now it lives visibly in `Documents\SC BP Watcher`. On the first
  start it is **copied**, not moved — the old folder stays untouched in case
  something is missing after all.
- **For this one update, use the setup rather than the button in the program.**
  The button works, but it still runs v2.0.0's update path — and on Windows
  that leaves a console window sitting there until you quit the program. A bug
  in the update path cannot fix itself; from v3.0.0 on it is sorted and the
  button is enough.
- **If you put the `.exe` somewhere by hand, delete it after installing.** The
  setup places the program in `%LOCALAPPDATA%\Programs\SC BP Watcher`. The old
  file would otherwise stay behind, and one day you would start the old version
  by accident.
- **On Linux there is nothing to do** — the AppImage replaces itself.

### Added

- **A "Server status" tab of its own.** Is Star Citizen up? If you cannot get
  into the game, you look for the fault on your own machine first — this
  answers that beforehand. It shows what CIG reports on its status page: the
  state of all three systems, plus the incidents of the last two months in full,
  update lines included. The layout follows the status page, and the states stay
  **in CIG's own wording** (`operational`, `maintenance`) — translating them
  would be a statement RSI never made. While the tab is open it checks once a
  minute; that costs almost nothing because it asks with `ETag` and an unchanged
  page is answered without content. The source is linked below it.
  ⚠️ These entries are **maintained by hand, not measured** — the page says so
  too, so nobody mistakes it for a measurement.
- **A button for „just give me the latest".** Until now you first had to
  understand what a channel is and pick the right one of the two boxes — anyone
  choosing the wrong one was offered nothing at all. There is now a full-width
  button above them that immediately fetches whatever is available, including a
  test build. It changes nothing about the setting below.

- **Star Citizen can be launched from the tool.** The „In-game details" page
  has a button that starts the game the way you already do: the RSI Launcher on
  Windows, `lug-helper` on Linux. If neither is found the button does not appear
  at all — anyone using a different route (Lutris, Heroic) sets `spielstarter`
  in the settings file. Suggested by Morkhan.

- **The mouse brings the overlay back.** In pop-up mode just move to where it sits — it
  reappears by itself and stays as long as the pointer is on it. Previously you had to
  restart the program for that, which no other overlay asks of you.

- **Restart right after an update.** It used to say „the new version runs on next start" —
  you had to quit and start it yourself. The fetch button now turns into **„⟳ Restart now"**
  once the download is done. The single-instance guard is closed first, otherwise the new
  copy would think it is the second one and quit immediately.

- **Start trace in the problem report.** A crash ends the program instantly — no report gets
  written, and all that remains is „it crashes". Every startup step is now written straight
  to disk; the last line in the report shows how far it got.

- **Get a release straight from the window.** Under each of the two cards („Stable
  releases only" / „Test builds too") there is a full-width button that downloads and
  installs the latest release of that channel — including going back from a test build to
  the last stable one.

- **Application menu entry (Linux).** The wizard offers it at the end, the settings any
  time. On Windows the installer handles this — on Linux the AppImage sat in the downloads
  folder and appeared in no menu. You can also put a keyboard shortcut on the entry to
  bring the overlay back.
- **Notification area icon (Windows).** Left click brings the window back, right click
  opens a small menu. The switch for it was already in the settings; the icon itself never
  existed.

- **The overlay can hold back.** Now selectable: permanently visible as before, or only
  popping up briefly when a blueprint actually arrives. You bring it back by starting the
  program again — you can put a system keyboard shortcut on the shortcut. Suggested by
  Haldjas (pr0): „when I get into the overlay with my mouse during combat, that
  will be unpleasant."
- **Mouse clicks can be passed through to the game.** The overlay stays visible but no
  longer catches clicks. On Windows via `WS_EX_TRANSPARENT`, on Linux via the XShape
  extension; under native Wayland it is not possible, and the setting says so instead of
  showing a switch that does nothing.
- **Starting the program a second time no longer opens a second copy** — it brings the
  running one to the front.

- **One window with tabs.** Blueprints on top, settings below, and everything only
  advanced users need collapsed at the bottom. The overlay stays as small as before; this
  window is what opens behind it.
- **An installer for Windows.** Start menu entry, optional desktop icon, optional
  autostart — and a proper uninstall. If you would rather not install anything, the plain
  `.exe` is still in the release.
- **Your files are now visible** under `Documents\SC BP Watcher`, split into blueprints,
  exports, settings and diagnostics. They used to sit hidden in the system — nobody looks
  there for their blueprint inventory. On first start they are **copied**, the old folder
  stays as a way back.
- **Import an existing inventory** — from the KRT Profit Basetool, from scmdb.net, from
  the launcher file or from your own backup. The format is recognised by its content, you
  just pick a file. Merged, never replaced.
- **Report a problem with one click.** "Report a problem" opens a pre-filled form; all
  you add is what happened. The report contains no names and no paths with your user name.
- **Test versions on request.** If you want to help checking, turn them on under *About*
  and get new versions before everyone else — through the same update notice.
- **Text size in four steps**, affecting text, icons and buttons alike.
- **Where blueprints without a contract come from.** 55 blueprints are not handed out by
  any regular contract — they come from named pools such as XenoThreat, RDC-Boss or
  RedWind. Instead of a question mark the source is shown, and you can filter by it.
- **What's new** as its own tab, split into new, improved and fixed.
- **Starter blueprints** are detected and entered — the eight everyone has from the
  start, marked with ◆.
- **Export your inventory** in three formats: KRT Profit Basetool, scmdb.net and a full
  backup.

### Changed

- **"Paths" moved to the advanced section.** The game folder and the launcher
  are found automatically; anyone who does need to step in is guided by the
  setup assistant, which explains what the page only shows as fields. A tab
  almost nobody needs was just in the way at the top.

- **Launching Star Citizen now sits at the bottom left**, in the accent green
  above "Advanced". The button used to live on the "Mission text" page — where
  blueprint wording is handled — and after that only in the overlay, so only
  while that was visible. Now it is there on **every** page.

- **A Discord button** below it, deliberately quieter: launching the game is what
  you keep this window open for, the Discord link is an offer. Two equally loud
  buttons cancel each other out.

- **"Check now" is now "Check for updates".** The old label never said what it
  checked for. "Update" would have been wrong — the button only looks, it
  fetches nothing.

- **„No release known yet" sounded like an error.** The button did not say what
  to do — it now reads „Press ‚Check now' above first". And the „Finished
  versions only" box is marked „recommended", so nobody has to guess what to
  pick. Both came up during Morkhan's test.

- **The tab is now called „Update & About".** Nobody looking for an update finds
  it under „About" — not even the author looked there.

- **The „launch Star Citizen" button sat where nobody would look for it.** It
  was on the „In-game details" page, which is about mission text — even the
  author could not find it again. It now sits as a green „▶" in the overlay's
  top bar with the other icons: anyone who wants to start the game does not have
  the main window open anyway. Hovering it explains what the click does.

- **You are asked before a translation is installed.** „German" and
  „StarStrings" replace the game’s text file completely — after that the whole
  game is in that language, not just the blueprint details. That was documented
  nowhere; now the help text says so, and a prompt appears before the first
  install. Confirmed once, it does not ask again. „Original" does not ask,
  because it does not change the language.

- **In pop-up mode the overlay leaves a narrow green strip behind.** Hover it and the
  overlay is back. The first attempt polled the mouse position — which cannot work under
  Wayland: measured, Tk reported the same coordinates twelve times in a row while the mouse
  moved across the screen. An application only learns the pointer position there while it is
  over one of **its own** windows. The strip is such a window — and it is more honest than
  an invisible magic zone: you can see where the overlay is waiting.

- **The problem report says which version an error came from** — and marks those from an
  older one. The store keeps the last ten across restarts; after an update it listed errors
  that had long been fixed, making the report look like nothing worked.

- **Up to twelve sources per blueprint** instead of three. Measured: more than half of
  all blueprints had sources cut off before. The easiest route is still shown first, the
  rest unfolds.
- **The source details appear on click** and can be closed again — in a small window they
  used to eat a third of the list.
- **Filter by type, class, size, grade and source**, on top of search and the
  "watched / owned / still missing" lists.
- **Collapse the overlay** (▾): it folds into its title bar.
- **No more save button** — changes take effect right away.

### Fixed

- **A collapsed overlay could not be opened again.** The button toggled, but
  nothing happened on screen — the tool was shut and stayed shut. Cause: on
  collapsing, the current window height was stored as the "open" height. Once
  the stored state and the actual geometry drifted apart, the next collapse
  wrote the **title bar height** as the open height; from then on the window
  "expanded" to its own size. The height is now only remembered while the window
  really is open, and expanding enforces a minimum height.
- **The resize grip covered the ✕ while collapsed.** It sits at the bottom
  right — on a window shrunk to title bar height that is the same spot as the
  top right, and you had to aim to close the tool at all. It now belongs to the
  **list** rather than the window — when the list is collapsed it has no height,
  so the grip is necessarily gone with it. Hiding it in time instead failed
  three times: a state that follows from how things are built is more reliable
  than one restored afterwards.
- **Blueprint names were unreadable without the launcher** — "Golemmc4Orepod"
  instead of "GOLEM MC-4 Ore Pod". The fallback ran `.title()` on the comparison
  key, which has no word boundaries left; the readable name sat right next to it
  in the cache the whole time. This affected **every Linux user**, because there
  is never a launcher there.
- **Self-update never arrived on Windows.** Clicking "get it" produced a warning
  and then nothing at all — except an orphaned 14 MB file in the program folder,
  once per attempt. Two separate bugs were behind it, either of which would have
  been enough on its own:

  The **wrong file** was fetched. Every release carries three assets, and the
  code took the first one ending in `.exe`. GitHub sorts alphabetically and a
  `-` sorts before a `.`, so `SC-BP-Watcher-Setup.exe` came first. The installer
  was moved on top of the program file without ever being run: opening the
  watcher afterwards gave you a setup window.

  And the swap could not have happened anyway. After the app exits, the
  bootloader stays alive to clean up its folder under `%TEMP%`; when a file
  there stayed locked it sat in a "Failed to remove temporary directory" dialog
  — holding the very `.exe` the helper script was waiting to be released. After
  two minutes it gave up. The user would have had to dismiss a warning nobody
  knew was part of the update.

  **On Windows the installer is now launched** instead of the program swapping
  its own file. It closes the running watcher itself, replaces it, keeps the
  "Apps & Features" entry current and starts it back up. On Linux the proven
  AppImage swap stays as it was.

- **The tray icon never appeared on Windows.** It was created on every start and
  failed at the same spot every time, visible only in the error report:
  `argument 11: OverflowError: int too long to convert`. The call that creates
  the window had no type declarations, and without them Python passes every
  value as a 32-bit number — the handle involved is wider than that on 64-bit
  Windows. The same mistake sat in the window procedure's return type. Shutdown
  now cleans the icon up for real, too: the previous route was not allowed to
  work from outside and failed silently.

- **The version shown in "Apps & Features" stayed put.** Only the per-user
  registry branch was checked. Anyone who picked "for all users" during install
  has their entry in the machine branch, which was never updated — so Windows
  kept showing a version that no longer existed. Both branches are searched now.
  On top of that the installer no longer asks "just me" or "all users": the
  program lands in your own user folder either way, which removes the question
  and any administrator prompt when updating.

- **The icons in the bar looked mangled on Windows.** `Segoe UI` contains
  **not one** of the fourteen glyphs — Windows picked a fallback per character
  and reached for **Segoe UI Emoji**: colourful, square emoji images in a slim
  dark bar, at uneven widths (10 to 21 pixels at the same size). That is also
  why the icons could never be evened out via the font size — they came from
  different font files. Windows now explicitly asks for **Segoe UI Symbol**:
  all fourteen glyphs monochrome, in the configured text colour, with half the
  spread. On Linux this was never a problem and nothing changes.

- **The overlay stayed German when you switched to English.** Changing the
  language gave you an English window and a German status bar:
  „8 Baupläne · Log ✓ · ohne Launcher · geprüft", plus the waiting message and
  the autostart text. English versions of those strings had existed all along —
  nobody used them, the code kept assembling the German ones. On top of that
  the overlay never heard about a language change at all; only the settings
  window relabelled itself.
  The catalogue watch message „newly craftable in game“ had the same
  problem. Messages **already sitting in the bar** when you switched stayed
  German too — „Keine Log-Sicherungen gefunden", for one. They had been written
  into the line as finished sentences, frozen in the language of the moment;
  only a restart cleared them. Messages now carry their text key along and are
  rewritten on a language change — including the date, which reads differently
  in English (2026-08-22 rather than 22.08.2026).

- **The hint on the ▶ launch button overwrote the status bar.** It was the only
  one of the ten icons without a tooltip; instead it wrote into the status bar
  and afterwards restored a value that was never kept up to date — so a
  blueprint message was gone after the mouse passed over the icon.

- **The logo was missing from the finished build.** On „Update & About" the
  program loaded `assets/xharig.png`, but the build never packed that file — it
  never showed when starting from source, where the file is present.

- **The „ⓘ" on the overlay opened a separate window with its own update logic** —
  and that one had no restart button. Anyone going that way downloaded the new
  version and was then left with a sentence instead of a button. It now opens the
  main window on „What's new", with the „Update & About" tab right beside it.
  **One route instead of two.** Reported by Morkhan.
- **Stretched buttons only filled half the width.** Mostly affected the buttons
  below the two update boxes. Reported by Morkhan.

- **Updating through the info window never arrived.** Anyone using the green
  „ⓘ" on the overlay instead of the settings page only got the line „the new
  version runs on next start" — **and no button for it**. On Windows that line
  is not even true: a helper script only swaps the file once the program has
  quit, and gives up after two minutes. Anyone who kept playing ended up with no
  update at all. The same „⟳ Restart now" button as in the settings is now
  there. Reported by Morkhan.
- **A console window flashed up briefly during updates.** The helper script has
  run invisibly since v3.0.0 — the `taskkill` before it, which clears away an
  already running script, was overlooked. Reported by Morkhan.

- **Five failures used to happen silently.** If the settings, the watchlist, the
  „new" markers, the autostart entry or a saved report could not be written,
  nothing happened at all — the setting was simply back to its old value after a
  restart, and the error report said nothing. Those places now report.

- **The error report left the game language empty.** It showed only a dash even
  though detection worked perfectly — the query returned two values, the report
  expected one, and the error was swallowed silently. It now states what is being
  searched for in the log **and where the wording comes from**: the game's
  `global.ini` or the built-in table. That is the first question whenever someone
  says „it doesn't detect my blueprints".
- **Truncated descriptions in three places.** On a narrow window a few pixels
  were missing and the last characters fell off. Affected were the update
  channels, „Write details into mission text" and „How often to look".

- **The setup wizard did not remember the chosen text source.** It fetched and
  installed the texts but never stored the choice — afterwards none of the three
  sources was selected under „In-game details". Reported by Haldjas.
- **Updating on Windows spawned console windows.** The helper script that
  swaps the running `.exe` looped forever while the file was locked — and it
  stays locked until the program quits. Every further click on „get" started
  another window. It now gives up after two minutes, stays invisible, and an
  already running helper is stopped first.
- **„Check now" did not check.** The button showed „Looking for a new version …" and did
  nothing else. Anyone with a stale cache could not get out of it — one tester was still
  offered rc12 while running rc18. It now really asks, reports the result and updates the
  display.
- **Self-update took the Windows path on Linux** and reported „[Errno 2] No such file or
  directory: 'cmd'". The guard against foreign programs compared our own code against
  `APPDIR` — but PyInstaller extracts into a directory of its own, so the comparison always
  failed. The filename decides now.
- **Self-update could have overwritten other programs.** It treated any file the `APPIMAGE`
  environment variable pointed at as its own — and that variable is set in **every** program
  started from an AppImage. Now our own code must come from the matching `APPDIR`, and a
  second guard rejects any target whose filename does not belong to this program.
- **Self-update always failed on Linux.** The download went to `/tmp` and was installed
  with `os.replace()` — and on virtually every Linux `/tmp` is a separate filesystem.
  `os.replace` cannot move across filesystems; it ends in „[Errno 18] Invalid cross-device
  link". The comment in the code always promised „next to the running program" — now the
  code does too, and installing became atomic along the way.
- **Crash on the very first start** (`SIGSEGV`), reported by Bomb20. The wizard created its
  **own** Tk instance and destroyed it at the end; the overlay then created a second one.
  After the first is destroyed, fonts, images and pending callbacks live on pointing at a
  dead interpreter — whether that goes well is a matter of timing. His „it ran fine with
  debugging on" is the fingerprint of exactly that. There is now only **one** Tk instance in
  the whole program.
- **The `[SCBPW]` markers were visible in game.** The contract title read „Security
  Patrol**[SCBPW]** [BP 3/6]**[/SCBPW]**". They made sure inserted text could be removed
  exactly — but nobody wants to read that in their game. There is no marker in the text at
  all now: the **wording before the insertion** is remembered, and removing restores it.
  That is more precise than before. Verified with `tools/injektion_pruefen.py` against the
  real file: inserting and removing leaves all 743 passages character-for-character as they
  were.
- **In game only the number showed, not which blueprints.** A contract has one title but
  often a dozen descriptions — one for „to the ruin station", one for „to the distribution
  centre" and so on. The contract data names only **one** of them; the rest stayed empty.
  The title said „[BP 0/12]", and anyone opening the description to see *which* twelve
  found nothing. Measured: 51 Covalex descriptions in the game, 7 of them with details.
  They are now filled via the shared key prefix.
- **„Personal weapon" and „FPS weapon" were two groups for the same thing** — 87 under one
  key, two under the other.
- **„Rows in the overlay" had no effect.** The setting was saved and never read; the
  overlay used a fixed 200. The configured value now applies, with 20 as the default — no
  one collects 200 blueprints in one session anyway.
- **„Browse" opened no dialog** — neither for the Star Citizen folder nor for your own
  files. Both do now, and on Linux with the system's dialog instead of Tk's grey one.
- **The last blueprints in the list overlapped.** X11 uses 16-bit window coordinates; all
  722 in one frame come to about 33000 pixels, putting 16 rows past the limit. The list is
  now shown in blocks when needed — nothing is hidden.
- **The scrollbar could not be grabbed.** The handle was drawn with a minimum height but
  tested against the calculated one — hitting its lower half counted as „beside it".
- **The window started off-screen.** With no remembered position Tk placed it at `+0+0`;
  with a portrait monitor on the left there is no picture there. Startup and „Reset window
  position" now centre it on the main screen.
- **Autostart was out of sync between overlay and settings.** Both read their state only
  when drawn.
- **The window icon was missing from every finished build** — on both systems. The file
  was not shipped with the program at all.

### Thanks

This release owes a great deal to two testers who took the trouble not just to
notice problems, but to describe them precisely enough to be found:

- **Haldjas** (pr0) — the pop-up mode suggestion; plus the setup that
  failed on the running file, the console windows during updates, the missing
  tray icon, the crash after restarting, the font size that never reached the
  overlay, the text source the wizard forgot — and the observation that
  explained everything: „it stays on rc25".
- **Bomb20** (pr0) — the crash on the very first start (a bug only new users
  would ever have hit), the „check now" button that did nothing, and the note
  that the „German" text source translates the entire game.
- **Morkhan** (KRT) — the suggestion to launch Star Citizen straight from
  the tool.

The blueprint details are based on the openly published contract data of the
**SC Deutsch Launcher team** and on **scmdb.net**.

## v2.0.0 - 2026-08-24

**The Windows overlay has become a standalone tool for Windows and Linux — and on
request it writes blueprint details straight into the game.**

The SC Deutsch Launcher is no longer required. Verified against a real Star Citizen
installation, with both a German **and** an English client.

### Without the launcher

- **`Game.log` is the source.** Your collection is maintained by the tool itself; on
  first start the stored session logs are read. If a gap remains, the tool says so
  instead of presenting an incomplete list as complete.
- **The game language works itself out.** The in-game blueprint message is localised;
  the tool derives the wording from your own logs — it knows over 700 blueprint names,
  and where one appears in a log line, the text before it is the phrase. German and
  English are measured; other languages it figures out by itself.
- **If the launcher is present it is still used** — including when it sits on a mounted
  Windows drive, which is the normal case on dual-boot systems.

### Blueprint list

- **Every blueprint to look up**, with search, filters and progress. Search covers name,
  category, class (`military`, `stealth`, `civilian`, …), manufacturer and grade.
- **Where each blueprint comes from** — faction, contract, required standing, payout
  **and where the contract can be picked up**.
- **Four sections** to show and hide: ship parts, FPS weapons, armor & clothing, other.
  Ordered by section rather than alphabetically.
- **Watchlist by click.** When a watched blueprint shows up the tool says so loudly —
  and removes the fulfilled wish by itself.

### Blueprint details in game

- **Every contract that awards blueprints** gets the list inside its mission text — with
  tick boxes: ticked for what you own, empty for what you lack. Plus a marker in the
  title (`[BP 2/3]`), visible in the contract list itself. **681 text spots**, German and
  English.
- **Three ways to get the base text:** the German translation by
  [rjcncpt](https://github.com/rjcncpt/StarCitizen-Deutsch-INI),
  [StarStrings](https://github.com/MrKraken/StarStrings) by MrKraken — or the English
  originals from your own `Data.p4k`, with no download at all.
- **Undo is byte-exact.** StarStrings users keep it: its markup stays, ours is added.
- You are **asked**, never surprised. Nothing is preselected.
- **It stays current by itself.** On startup and every six hours after, the tool checks
  for a newer translation, newer blueprint data — or a `global.ini` that a game patch
  has replaced. All three are re-applied automatically.
  - **Why this is not a nicety:** every translation update and every patch rewrites the
    file, so the details are simply **gone** — and after a patch, contracts award
    different blueprints. Neither is noticeable, because the game runs fine either way.
    Without this check you eventually play on wrong data.
  - Only what the player set up themselves is ever touched.

### Using it

- **Setup wizard** in five steps, repeatable at any time — and a **settings window** for
  everything at once.
- **German and English**, switchable, effective immediately.
- Hover explanations on every icon, adjustable opacity (which matters with a single
  screen), sound, autostart.
- **Update notice with a version history** — including releases you skipped.

### Distribution

- **Ready-made files for both systems**, built by GitHub on every version tag. The
  AppImage is built in an Ubuntu 22.04 container so it starts on common systems.
- ⚠️ **Important for Arch, Fedora and openSUSE:** that same container was also a trap.
  The bundled Python looked for its certificate store under the Ubuntu path
  `/usr/lib/ssl`, which does not exist there — **every** HTTPS connection failed
  silently. No blueprint catalogue, no translation, no update notice; the program
  started but could load nothing. The launcher now looks for the store in all the usual
  places. On Ubuntu and Debian this never showed up.
- **Nothing third-party is bundled.** The blueprint catalogue (scmdb), the translation
  and StarStrings are fetched at runtime, from their own addresses, on your machine.

### Thanks

The in-game blueprint details build on the openly published contract data of the
**SC Deutsch Launcher team** (813 contracts, German and English) and on **scmdb.net**.
Without either, this release would not exist.

## v2.0.0-rc1 - 2026-08-24

> **A pre-release for testing.** Feature-complete and thoroughly tested, but never
> yet run against a real Star Citizen installation other than the author's — that
> is what testers help with. Feedback welcome as an [issue](../../issues).

**The Windows overlay has become a standalone tool for Windows and Linux.** The
SC Deutsch Launcher is no longer required, the blueprint inventory is kept by the
tool itself, and for most blueprints it now says where to get them.

### Added

- **Runs on Linux.** One codebase for both systems, not a second branch. Where files live is decided in one place (`scbp/pfade.py`): `%APPDATA%` and `C:\Program Files` on Windows, `~/.config` and the Wine prefix on Linux (searched where lug-helper, Lutris, Bottles and Heroic put their installations).
- **Its own blueprint inventory** (`bestand.json`), with a note where each entry came from. Written via a temporary file and a rename, so a crash mid-write cannot corrupt it; the previous state is kept as a backup.
- **Catch-up on start.** The stored logs of earlier sessions are read and quietly added — nothing is lost if you played without the watcher running. On the very first start the *current* log is read from the beginning too, otherwise the session in progress would be the one gap.
- **An honest gap notice.** If the stored logs do not reach back to the last known state, the watcher says so as its own line (ℹ) instead of passing off an incomplete list as your inventory. That is what the tick-off list is for.
- **Blueprint catalogue with origins** (`scbp/katalog.py`). 714 blueprints; for 655 of them it lists faction, contract, required standing with reputation points, payout in aUEC and reputation gain — sorted by the easiest route, at most three sources each. The 12 MB source dump is not kept but boiled down to 347 KB, fetched once per game version with retries.
- **Management window** (`scbp/bestandsfenster.py`): searchable list grouped by type, filters *all / owned / missing*, progress count, tick entries with a click, expand origins with a click.
- **Watchlist by click** (`scbp/merkliste.py`). The star turns any entry into a wish — when it appears the watcher announces it in gold. **Fulfilled wishes remove themselves** once the blueprint reaches your inventory. Externally added patterns keep working.
- **Setup wizard** (`scbp/assistent.py`) — four steps, **repeatable at any time** from the title bar. Language, finding Star Citizen (with a browse button and validation *as you type* — any level works, even the `Game.log` itself), collecting past blueprints, done. Repeatability is deliberate: someone who is not comfortable with computers should be able to redo something without knowing which menu it hides in.
- **German and English, switchable** (`scbp/sprache.py`). The default follows the system, but the `sprache` field in `einstellungen.json` overrides it — running an English system and still wanting to read German is a legitimate choice. Switching takes effect immediately.
- **The tool works out the in-game language by itself.** The blueprint message in the log is localised; only the German wording had ever been measured, the English ones were guesses and other languages were not covered at all. It now derives the phrase from your own logs: it knows over 700 blueprint names — if a log line contains one, the text in front of it is the phrase. Two distinct matches are required so coincidence is ruled out. Verified against an invented French build.
- **Update notice and version history** (`scbp/aktualisierung.py`, `scbp/versionsfenster.py`). The tool checks at most once a day; when something new exists, ⓘ in the title bar turns green. Behind it is the version history — **including older releases**, so you can read what you skipped. Downloads come from `github.com` only; anything else is refused.
- **Ready-made files for both systems, built by GitHub** on every version tag. The Linux build runs in an Ubuntu 22.04 container (glibc 2.35) — built against a newer glibc it would not start on common systems at all. The build aborts if the tag and `__version__` disagree.
- **Own paths can be entered** (`einstellungen.json`), and the file is created automatically with the searched locations listed next to each field. Check interval and sound are configurable too.
- **Start script for Linux** (`SC-BP-Watcher starten.sh`), which checks for `tkinter` first and names the right package per distribution.
- **Self-test** (`tools/selbsttest.py`) that reconstructs an installation in a throwaway folder and works through the known pitfalls.
- **Project page in English and German** — English is the default page, German is one click away at the top.

### Fixed

- **The watcher would have crashed on start under Linux.** The `size_nw_se` mouse cursor on the resize handle only exists on Windows; elsewhere Tk raises an error before the window ever appears.
- **Window position from someone else's machine.** The remembered position was applied unchecked. On a machine with a different monitor setup the window sat outside every screen — invisible, and on macOS it took the program down with it. It is now checked for plausibility, and the built-in default carries **no position at all**, only a size. Where the overlay belongs is something everyone drags into place themselves.
- **Endless loop without the launcher.** On start the watcher waited until the launcher file became readable — without a launcher, forever. Under Linux it would never have come up.
- **The catalogue watch did nothing without the launcher.** "What became newly craftable" depended on a launcher file. Without it, the scmdb data now takes over.
- **Sound without `winsound`.** That module does not exist on Linux; tkinter rings the bell there instead.

### Changed

- **The status line shows your own inventory**, not the launcher's count, and whether it is working with or without the launcher. Reason: the launcher demonstrably counts too low — the P4-AR Rifle is missing from it although the Fabricator lists it as owned. Starter blueprints were never "received" and appear in no log. Its number is a lower bound, not an inventory.
- **The SC Deutsch Launcher is optional.** If present it still confirms finds (🟡 → 🟢) and supplies its maintained catalogue. Without it only that falls away — the log is the actual source either way.
- **Starting no longer requires the launcher file**, only that Star Citizen itself is found. If it is not, the wizard **asks** — instead of showing a message and quitting, which would have meant editing a JSON file by hand and restarting. Nobody does that.
- **Brand colour** moved to `#9ce430`; the overlay was still running on the pre-logo-change green.

### Removed

- **The "build the EXE yourself" script.** Since GitHub builds the files, nobody needs it — and it had already gone stale: built without `--add-data`, the resulting executable would have had neither the changelog nor the catalogue data.

## v1.5.0 - 2026-08-11

### Added

- **Value fallback via scmdb.net.** When the launcher catalogue does not know an item, the watcher now takes type, size, grade, class and manufacturer from scmdb's crafting data (`versions.json` → `crafting_items-<version>.json`). Blueprints missing from the catalogue finally get a tag too — QuadraCell, FR-66 and the skin variants among them. Plain `urllib` from the standard library, no extra package.
  - Cached locally; refetched only when a **new game version** appears (checked every 6 hours).
  - Without a connection the last state applies, without a cache everything behaves as before v1.5.0 — the watcher never aborts over it.
  - Can be switched off with `SC_BP_NO_NET=1`.
- **Start with Windows — voluntarily.** New `⏻` switch in the title bar (green = on, grey = off). It adds or removes an entry under `HKCU\…\CurrentVersion\Run`. Nothing is enabled without asking, and the state lives only in the registry — there is no second source of truth to drift apart from.
  - Started from source it registers `pythonw.exe`, not `python.exe`: otherwise a console window would sit open after every login and steal focus from the game.
- **New app icon.** Dark round emblem in Xharig green: segmented scanner ring, blueprint sheet with a cube, horizontal scan beam. Built from two artworks — a detailed one from 40 pixels up and a **simplified one for 16–32 pixels** (solid cube instead of wireframe, no corner brackets). A single motif across all sizes would have turned to mush when small.

### Worth knowing

- **Order of precedence:** `bp-overrides.json` → launcher catalogue / game data → scmdb. scmdb only fills gaps and never overrides. Reason: a comparison against 56 messages from the game log produced **55 exact matches** on size, grade and class — but for the *Elsen* cooler scmdb says grade A while both the game log *and* `components.ini` agree on B (the manufacturer is wrong there too). An excellent source, but not an infallible one.
- **The scmdb data is deliberately NOT bundled.** It is fetched on the user's machine directly from scmdb.net, the way a browser would. scmdb is licensed CC BY-NC-ND 4.0; shipping a copy would be redistribution and would conflict with that licence as well as this project's GPL. Requests carry an honest identifier so the operator can see who is asking.
- **Armour and FPS weapons still get no tag.** scmdb assigns `size` and `grade` to every item, helmets included — taken at face value, every piece of armour would carry an invented "Grade A, Size 1". Class and grade are therefore only used when scmdb lists a `componentClass` (actual ship components); ship weapons get size only.

## v1.4.0 - 2026-08-02

### Changed

- **Licence changed from MIT to GNU GPL v3.0** (version 3 only, `SPDX-License-Identifier: GPL-3.0-only`). The source is being opened: a single public repository instead of the planned split into a private source and a public distribution repository. The GPL lets anyone use and modify the code, but requires the source to come along under the same licence when distributed.
- `README.en.md`: new **"Star Citizen Fan Content"** section with the wording required by RSI and a link to the official page — a prerequisite for public distribution.

### Fixed

- **Hard-coded local path removed.** `OVERRIDES_FILE` pointed at a directory that only exists on the developer's machine — for everybody else it led nowhere, and opening the source would have made the path public. The optional overrides file is now looked for in the user's own folder; a different location can be given via `SC_BP_OVERRIDES`. With neither, the launcher catalogue applies unchanged.

## v1.3.0 - 2026-07-31

### Added

- **Catalogue watch — reports what became NEWLY craftable in the game.** Until now the watcher only reported what *you* unlocked. It now also keeps an eye on `bp_item_types.json`, the list of everything that has a blueprint at all. The SC Deutsch Launcher refreshes it with each patch; when something is added it appears as 🔵 **newly craftable**. That way you notice when CIG adds an item that simply had no blueprint before.
- **Watchlist for wanted items:** if `watchlist.json` exists, matches from it are announced prominently in gold with ⭐ and their own sound (`<title> — now craftable!`). Format: `{"eintraege": [{"titel": "…", "muster": ["substring", …]}]}`, patterns lowercase, matched as substrings. Without the file the watcher simply reports every addition.
- The comparison state lives in `catalog-seen.json` and **survives restarts** — otherwise half the catalogue would arrive as "new" after every start. The very first start only establishes the baseline and reports nothing.

### Fixed

- **Widening the window did nothing:** the list width was hard-coded at `312` pixels. Dragging the window wider still gave you the same narrow content — long blueprint names stayed cut off. The list now follows every resize; long subtitles wrap instead of disappearing off the edge.
- **Default size** raised from `341x1098` to `440x1098` (the right edge stays put) so the longer catalogue-watch messages fit without wrapping.

### Notes

- The catalogue file is read only **once a minute**, and even then only if its timestamp changed — it only ever changes with patches.
- Catalogue lines are notifications only: they are never confirmed to 🟢, because they have nothing to do with your own unlocks.
- The watcher keeps its catalogue state in a **separate** file — so a second tool working on the same data cannot steal its notification.

## v1.2.0 - 2026-07-30

### Added

- **Instant reporting from `Game.log`:** the watcher now reads Star Citizen's log itself and shows a new blueprint **within seconds** instead of waiting for the launcher's export. Background: the SC Deutsch Launcher rewrites `sc_bp_erledigt.json` only every few minutes — measured on 2026-07-30, **2.5 minutes** passed between the unlock in game (21:23:49) and the launcher export (21:26:24). Reading the log closes exactly that gap.
- **Two-stage display:** blueprints freshly read from the log appear as 🟡 **provisional**; once the launcher catches up, the line is confirmed to 🟢 and refreshed with its data. The launcher file remains the authoritative source — type, size, grade and class still come from its catalogue.
- **Name matching between log and launcher:** ship components appear in the log with a suffix (`7CA 'Nargun' (Civ/3/A)`) and without it in the launcher — the suffix is stripped (and doubles as a fallback for the `M/A/1` tag if an item is not yet in the catalogue after a patch). Genuine name brackets such as `(30 cap)` or `Singe Cannon (S2)` are left alone. Where translations differ (seen: `(12 Schuss)` in the log versus `(12 cap)` in the launcher), a fallback match without the bracket applies — but only when it is unambiguous. Verified against all 127 stored log backups: 148 blueprint messages, 147 exact matches, the remaining one via the fallback.
- **Automatic log discovery** and detection of a game restart (rotated log).
- **Status line** now also shows whether the log is being read.

### Fixed

- **"Newest on top" never worked:** new lines were inserted using `winfo_children()` — that is the order of *creation*, not the order in the window. From the third entry on, every new arrival ended up **below** the older ones. `pack_slaves()` is used now.
- **`MAX_ROWS` had no effect:** the setting was documented in the README but never applied in the code — the list grew without limit. The oldest lines beyond `MAX_ROWS` (default 200) are now dropped.
- **Type lookup refreshes itself:** if a just-unlocked item is not yet in `bp_item_types.json`, the file is reloaded once instead of immediately showing `—`.

### Notes

- Log reading recognises the **German** in-game message. With another game language it does not apply — the tool then behaves as before. *(Resolved in v2.0.0: the wording is now worked out automatically.)*
- Still read-only: `Game.log` is only ever read, never modified.

## v1.1.0 - 2026-07-19

### Added

- **Size / grade / class per blueprint** as a compact `class/grade/size` tag, e.g. `M/A/1` (Military · Grade A · Size 1). Letters: **M** Military, **S** Stealth, **I** Industrial, **C** Civilian, **K** Competition. Ship weapons only have a size → `–/–/2`; FPS weapons and armour have none of it → no tag. Data from the launcher catalogue plus manual corrections from `bp-overrides.json` (which take precedence).
- **The window remembers position and size:** on moving, resizing and closing, the geometry is saved and restored on the next start.

### Changed

- **Default start position** is now the upper monitor rather than the gaming monitor, so you no longer tab out of Star Citizen by accident. *(Removed again in v2.0.0 — a fixed position from someone else's setup is invisible on yours.)*

## v1.0.3 - 2026-06-29

### Added

- **GitHub release** with the finished `SC-BP-Watcher.exe` attached — download, double-click, done (no Python, no building it yourself)

### Changed

- README: "download the ready-made `.exe`" is now the **recommended** way to start

## v1.0.2 - 2026-06-29

### Added

- **App icon** in the Xharig style (dark background, Xharig green, scope ring with a "new" dot) — `icon.ico` for the executable, `assets/icon.png` as a preview
- The executable is now built with the icon
- The window and taskbar icon is also set when starting from source
- Reproducible icon generator (needs Pillow, which the tool itself does not)

## v1.0.1 - 2026-06-29

### Added

- **Thanks and credits** to the SC Deutsch Launcher (the tool's data source at the time), including a note that SC BP Watcher is an independent, unofficial companion tool
- Official link to the **[SC Deutsch Launcher](https://www.sc-deutsch-launcher.de/)**

### Changed

- The mandatory prerequisite (SC Deutsch Launcher) highlighted at the top of the README

## v1.0.0 - 2026-06-29

First release.

### Added

- Live overlay (borderless, always on top, translucent) showing new Star Citizen blueprints in real time
- Background monitoring of `sc_bp_erledigt.json` (3-second interval, its own thread)
- Per arrival: 🟢 name · type · time, newest on top
- Sound on every new blueprint
- Window movable (title bar) and resizable (◢ handle), clear the list (🗑), close (✕)
- Type shown in whichever language the source provides
- Automatic path discovery
- Start via a batch file (no console window) or as a standalone executable
