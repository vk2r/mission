# Development notes

English | [Español](DEVELOPMENT.es.md)

Usage is in [README.md](README.md); this file covers what you need to know to change the plugin.

Mission is a bar widget that puts a team of AI agents in the Omarchy bar: their states, their work and their spend. The data comes from the agent app the widget is paired with — today that app is [MausBot](https://mausbot.com/) — and the engine reads only that app's local files. Where a behaviour depends on the app's own format or facts, this document says so; nothing else in the design assumes a particular app.

## Naming and identifiers

The app's brand is **MausBot** (window title, https://mausbot.com/), and every text the user reads says so. The technical identifiers still say `openmausbot` on purpose: the Hyprland class `com.openmausbot.app`, the launcher `openmausbot.desktop`, `~/.openmausbot` and `~/.config/openmausbot`, the special workspace `openmausbot`. They are how the plugin finds, opens and hides the app and where it keeps its config, so renaming them would break detection. The plugin id is `io.github.vk2r.mission` and its runtime folder is `$XDG_RUNTIME_DIR/mission`.

## Setup

The build tools are pinned in `mise.toml`: the Rust toolchain, `cargo-deny` for the dependency audit and `cargo-about` for the third-party licence notice.

```sh
mise install            # Rust, cargo-deny, cargo-about
mise run build          # rebuild bin/engine
mise run test           # engine tests + the translation check
mise run audit          # advisories, bans, licences and sources of every crate
mise run licenses       # regenerate licenses/THIRD-PARTY.txt (after changing dependencies)
mise run validate       # omarchy plugin validate .
```

```sh
mise run fixtures        # regenerate engine/tests/fixtures (timestamps relative to now)
mise run i18n            # only the translation check
UPDATE_GOLDEN=1 cargo test --bin engine golden   # from engine/: regenerate engine/tests/golden after an intended drawing change
```

- Running from a checkout: symlink the project into the plugins folder under the plugin's own id — `ln -s ~/Projects/Software/mission ~/.config/omarchy/plugins/io.github.vk2r.mission` — then add **Mission** to the bar from Omarchy's bar settings and restart the shell once (`omarchy restart shell`). Omarchy installs plugins with a plain `git clone` and never builds, so this is the development path only.
- Cargo builds into `~/.cache/openmausbot-engine/target`, outside the plugin folder, because Omarchy reloads a plugin whenever a file in it changes.
- `bin/` holds only `engine`. Omarchy installs plugins with a plain `git clone` and never builds, so the binary is committed and CI builds it (see [Binaries and releases](#binaries-and-releases)); a local `mise run build` is only for testing. Run `git config core.hooksPath .githooks` once per clone: its `pre-commit` hook refuses commits that touch `bin/engine`, `bin/SHA256SUMS` or `bin/provenance.sigstore.json`, so drop a local build with `git restore --staged --worktree bin/engine`.
- Commit `engine/Cargo.lock`, and `licenses/THIRD-PARTY.txt` after `mise run licenses`, when the dependencies change.
- The JSON the engine prints is the contract with the QML (`EngineHub.qml` → `BarWidget.qml` / `Panel.qml`); keep its shape when changing the engine. `engine/tests/daemon.rs` asserts it.

## The pieces

| file                                | what it is                                                                                                  |
| ----------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `manifest.json`                     | plugin manifest (`bar-widget`)                                                                              |
| `qmldir`                            | registers the one engine hub singleton for the plugin's own directory                                       |
| `BarWidget.qml`                     | the icon, its breathing and the panel loader; it draws, it never polls                                      |
| `EngineHub.qml`                     | `pragma Singleton`: one `bin/engine` for the whole shell, and the one snapshot every widget reads           |
| `Panel.qml`                         | the panel host: the card, the keyboard, the page in view and the words every page uses                      |
| `panel/Home.qml`                    | the team page: the hierarchy, and the footer (the spend in one sentence, and the costs button)              |
| `panel/Settings.qml`                | language, the cost period, notifications and the widget options                                             |
| `panel/Costs.qml`                   | the cost of the chosen period: total, sparkline, 100 % split bar and the list by bot or by model            |
| `components/BotRow.qml`             | one bot in the team page: mascot, name, current work, state dot and word                                    |
| `components/RangePicker.qml`        | the shared period selector (Settings page and Gauge page header)                                            |
| `mascot/motion.js`                  | the app's per-face mascot motion table and blink timing (the sidebar's), used by `BotRow.qml`               |
| `components/BrandIcon.qml`          | the mark, tinted from the SVG in `assets/` for the bar and the panel                                        |
| `components/CompactDropdown.qml`    | the small selector the settings page uses                                                                   |
| `lang/index.js`, `lang/*.js`        | the eight languages (`index.js` is the loader); `en`, `es`, `de`, `fr`, `it`, `nl`, `pl`, `pt`              |
| `mascot/mascot.json`                | the app's own mascot catalogue: ten bodies and twenty-five faces, extracted from the installed app          |
| `engine/`                           | the Rust sources of `bin/engine` (daemon, roster, mascots, Hyprland, log follower, spend)                   |
| `bin/engine`                        | the binary the bar runs: one JSON line per change, orders on stdin                                          |

## The engine

`bin/engine` without arguments is the daemon the bar runs: one JSON line per change on stdout, orders (`refresh`, `show`, `hide`, `toggle`, `ack`) on stdin, and EOF on stdin ends it — which is how a widget taken off the bar leaves no process behind. One line carries `v`, `ok`, `window`, `alert`, `busy`, `working`, `bots` and `costs`.

- **One read per source.** A tick costs a `PRAGMA data_version` on the app's database and a few `stat()` calls; the answers are rebuilt only for the source that changed, and a tick that finds nothing says nothing.
- **Spend.** `usage.rs` sums `usage/YYYY-MM.jsonl` over three windows that all end today: `day`, `week` (the last 7 days) and `month` (the last 30), with a two-day margin when it picks the files to read, because a record stamped in UTC can land on another local date in a zone ahead of it. Each range also carries `series`, one bar per hour of the day or per day of the window, filled by the same read so the bars and the total cannot disagree, and two splits of the same money, `byBot` and `byModel`: the model is the record's `model` field, trimmed, stripped of control characters and cut to 60 bytes, so a malformed line cannot stretch a row; each `byModel` row also names the live bot that spent most on it (`bot`, absent when only deleted or unnamed bots used it), which the costs page turns into the row's colour. A corrupt line is skipped, never fatal.
- **The log.** `logwatch.rs` follows the app's log for the one thing that has to arrive at once: a turn between bots, which stamps the alert and raises the notification. The daemon keeps no list of what it read; a restart still starts at the end of the file, so nothing old is announced again.
- **Mascots.** Every row is drawn per read from the bot's record; `engine/tests/golden` holds a digest per drawing, the pivots and ten sample SVGs; `UPDATE_GOLDEN=1 cargo test --bin engine golden` regenerates them after an intended change.
- **No unsafe code.** The crate is `#![forbid(unsafe_code)]`; `mise run audit` covers advisories, licences and sources.

- **Hyprland through its socket.** `hypr.rs` asks Hyprland's own request socket (`$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`, the one `hyprctl` uses) for `j/clients` and `j/monitors`, and falls back to `hyprctl -j` when the socket cannot be reached or does not answer with a list. The daemon connects only as a client; it listens on nothing.

At rest the daemon costs about 0.012 % of one core and 7.8 MiB, with no child process: Hyprland is asked through its socket, statements are cached and a tick that found nothing builds no line.

To watch the daemon yourself:

```sh
bin/engine --once                     # the line, once, in a terminal
bin/engine                            # lines as things change; Ctrl-D ends it
printf 'hide\nshow\n' | bin/engine     # orders go in on stdin, and the pipe closing ends it
```

The clock can be moved for a test — `bin/engine --once --now 2026-11-02T00:00:05-03:00` — which is how midnight and the five-minute `failed` grace are checked without waiting for them.

## The usage ledger

- **How the app writes `usage/YYYY-MM.jsonl`:** it only appends. The app's `usage-ledger.js` writes every record with `appendFile(file, JSON.stringify(record) + "\n", { mode: 0o600 })`, through one write queue per data directory, and nothing else in the installed app opens those files for writing (`fleet-cli` and `group-thread-usage` only read). So `ledger.rs` trusts the inode, the length and the 4 KiB guard before the old end; the time-based full reread (`RECHECK`) is now 30 minutes and is only a net for a file someone edited by hand further back than the guard sees. Before, it was 60 s counted from the last *full* read, so about half of the real changes (one record every ~42 s) reread the whole month. Measured with a 6 MB month, 12 appends 42 s apart: 6 of 12 changes read 6.1 MB before, 0 of 12 now (61 KB each, all of it the Hyprland poll).
- **Deliberately not done (debt):** a block-hash check of the part already read (not needed while the app only appends); #7 caching the time zone per tick (`TimeZone::system()` is microseconds, and `TZ` changes are then picked up at once); #9 a page registry in `Panel.qml` (adding a page still touches `page`, `goTo`, `contentHeight` and the visibility of each page); N4 per-record local date/time (the aggregation walks the month, ~3 ms with 6 MB); #8 caching the `costs` JSON.
- **The line:** `series` bars are rounded to six decimals, `variants[0]` (the row's own drawing) goes out empty, and `byBot`/`byModel` rows carry `botId` so the panel colours by id, not by name. Net effect on `--once`: 14.7 KB to 15.2 KB.
- **Avatars:** the URL of a drawing is remembered next to its SVG and forgotten when `prune` has removed any file; a file already named like a drawing is reused only if it is a regular file of the same length.
- **Settings:** `notifyOn` reads `0`, `"no"`, `"off"` and `null` as off, `EngineHub.save` copies the live entry through JSON (an own `__proto__` key survives), and a stamp left readable by others is narrowed to 0600 when read. The keyboard-layout probe lives once in `EngineHub`, not in every widget.

## One engine for every screen

`EngineHub.qml` is a QML singleton (registered in `qmldir`). This shell runs one bar, and that bar builds one widget per monitor inside a single QML engine, so the singleton is one object for every screen, and it owns the one `bin/engine` process: `snapshot`, `state`, `alerting`, `teamBusy`, `costs` and the animation clock `phase` are read by both bars from the same values, and a second engine never starts.

The widgets register with the hub (`attach`/`detach`); when the last one goes the engine stops, so a widget taken off the bar leaves no process behind. The orders the widgets send — `show`, `hide`, `toggle`, `ack`, `refresh` — go back over the engine's stdin, so a click starts no process at all.

Where the window is, and whether an alert is waiting, are answered by runtime files under `$XDG_RUNTIME_DIR/mission/`: `alert` holds the newest event and `seen` the one you clicked away, and those stamps are what set the alert state while they differ. The directory survives a shell restart; the hub is what makes two screens agree without asking twice.

The follower starts at the **end** of `server.log`, because a shell restart must not replay the log's history as a burst of notifications. If the log is rotated or truncated it skips to the end of the new file rather than re-reading it from the top.

## Pages

`Panel.qml` is the host: it owns the card, the keyboard, the version footer, the data every page reads and the words. A page is a file in `panel/` that takes the host as `required property var host` and is instantiated in `Panel.qml`'s stack. `page` holds the name in view (`home`, `settings`, `costs`); a control opens a page with `root.goTo("…")` — Settings from the gear in the header, the costs page from the footer's button — and Esc or Backspace return to `home` from anywhere else.

Home is drawn all along and only fades, and the card keeps Home's height whichever page is showing, so nothing jumps when you move between pages. The costs page fills that height instead of scrolling the card: its total, sparkline and split bar are fixed and only the list is a `Flickable`, in the height the fixed part leaves. The version footer is drawn on Settings alone. To add a page: a new file in `panel/`, an entry in the stack in `Panel.qml`, a glyph (`String.fromCodePoint`, looked up by name in the installed Nerd Font) and its keys in `lang/*.js`.

A page that shows data the engine sends reads it through the host, never from the line itself: `rangeReport()`, `costText()`, `costSentence()`, `costSeries()`, and `costBreakdown()` are the host's, so the footer and the costs page always speak of the same range, and the list of the split picked on its LIST row.

## Clicks, alerts and spend (what the user sees)

- **Left click** opens the panel; another click closes it, and opening it also clears the alert (`Panel.qml` `open()` acks while `alerting`).
- **Right click** sends the engine's `show` order: on screen the window is raised and focused; parked it is moved back onto the workspace you are looking at and focused; not running, the app is launched from its `.desktop` entry. **Middle click** sends `toggle`, which parks the window on a hidden special workspace silently (whatever you were typing keeps the keyboard) and brings it back otherwise; it launches the app too when it is absent. The IPC command `toggle` opens and closes the panel — use the IPC `show` or `hide` to move the window.
- **Alert:** when a bot answers another bot, or a sub-bot's result comes back to its parent, a desktop notification carries the first line of that turn and the tooltip says there is something new. The mark itself carries no badge: while the team works it breathes, and that is the only change on the bar. A click is how you say you saw it, so clicking also clears the alert. Turns the user starts themselves are not announced or listed: you already know what you typed.
- **Spend figures** come from the app's usage log (`~/.openmausbot/usage/YYYY-MM.jsonl`, one record per turn). The range is the local calendar day, week or month; the log is stamped and filed in UTC, so the months either side are read too and each record is placed by its own local date. The cost sums `costUsd` over every record, background work included; a task is a turn whose outcome is `ok`, whoever started it. Nothing is cached, so costs another script fills in later show up on the next change.
- The notification text is stripped of control and bidirectional characters and cut to 120 bytes on a UTF-8 boundary, so a long turn cannot break the notification or the line protocol.
- Nothing touches `~/.config/hypr/windows.lua` or `hyprland.lua`: the special workspace is used through dispatches, with no window rules.

## Drawing the brand mark

The bar icon is the plugin's own mark, `assets/mission-mark.svg` (the Apollo "A" patch, bar version: a four-point star and a tilted orbit, no rings), drawn by `components/BrandIcon.qml` with the method the shell's own brand icons use: the SVG is read as text, its two colour tokens are replaced and the result goes to an `Image` as a data URL — no second file per theme and no GPU effect. `currentColor` (stars) takes the theme's text colour and `accentColor` (the orbit) takes the theme's accent, in `fill` and `stroke` alike, so the mark follows a theme change live. The replacement is global on purpose: `String.replace` with a plain pattern stops at the first hit, and in a commented SVG the first hit is the comment. Alpha is one group opacity around the drawing, not a per-element attribute, so it never collides with the `opacity` the artwork carries for its hierarchy. Both colours step back to 45% while the window is parked. The drawing is 14 px with 1 px of air on each side, inside a 16 px `viewBox`: the mark occupies the whole 16 px box the shell gives every bar icon, so the slot keeps its 27 px and the neighbours keep their place. The panel header draws `assets/mission-mark-hero.svg`, the 48 px version with the finer strokes, at 36 px under the title «Mission» and the line «A MausBot Companion».

While a bot is on a turn the mark also breathes — a soft opacity pulse, 650 ms each way, from the engine's `busy` count — and stands still when the team is idle. The pulse reads the hub's `phase`, so both monitors breathe on the same beat instead of each keeping a copy of the animation. With the panel open the panel's own rows answer instead, so the hub never asks a second time.

The crown beside the chief's name, the robot a row falls back to when the mascot catalogue cannot draw a bot, and the gear, history and arrow in the header are glyphs built with `String.fromCodePoint`, because Material Design codepoints live in Plane 15 and a `"\uXXXX"` escape takes exactly four hex digits: `"\uf037d"` is U+F037 plus the letter `d`, which is what used to draw as garbage. Every codepoint here was looked up by name in the installed JetBrainsMono Nerd Font, not in an MDI table.

## The team hierarchy

- Who leads is the app's own fact, not a name kept here: the app marks its Chief of Staff with `chiefOfStaff: true` on that bot's record in `bots.json`, and the engine sorts on it — chief first, then the rest of the team in the order `bots.json` has them, so the panel never disagrees with the app's own roster. Move that mark in the app and the panel follows: with it on Designer, Designer takes the card and Boss falls in under the trunk. The mark is a per-bot flag, not a "the" chief: if more than one record carries it, each one gets its own card above the trunk. A roster with no chief at all stays in roster order, with no trunk and no heading.
- The group under the chief is the department the app itself stores: the field is `section` on the bot's own record (an older `department` is still accepted, then it stops), and a bot with no section sits in the app's default department, whose name is the app's own fallback label — "General" (its strings: _Uncheck a bot to move it to General_). The app keeps its chief inside that section — its own backup check refuses two visible chiefs _in one section_.
- No heading is drawn over either group, on purpose. The first version labelled the lower group with the department and how many bots sit in it (the app's map reads `GENERAL · 5 bots`); VK found that redundant under the chief's own row. The `CHIEF OF STAFF` label above the card went next, for a structural reason: the flag can be on more than one bot, so a label naming the role would repeat. The hierarchy is now carried by the crown, the frame and the trunk alone — and the trunk starts on the outer bottom edge of the chief's card, so the team hangs from the head instead of floating below it. Each row still carries its department from `bots.json`, so a heading can come back from the data if it is ever wanted again.

## Mascot rendering

- Each row's mascot is drawn on every read from that bot's record in `bots.json`: `mascotBody` is the body, `color` the fill, `mascotExpression` the face, and a bot with no expression wears the face its live state calls for, or the one its role suggests. Change a bot's look in the app and the bar follows at the next change — there is no picture to regenerate and no manifest to keep in step. An unknown body falls back to the catalogue's own default cursor in silver; with the catalogue missing altogether every row falls back to the robot glyph and the roster still renders.
- Each mascot is two pictures on one aim: the body with its mouth, and the eyes alone, both drawn from the same `face_geometry()` on the same viewBox, plus the eyes' centre as a fraction of that square. That is what lets the bar blink the eyes — squashed on their own line — without moving the mouth or the body, and it is why the two files composite back into exactly the picture the row drew before the split (checked pixel by pixel with `rsvg-convert`). A reading that carries no eyes layer makes the row skip the blink.
- The drawing is a port of the app's own renderer, not an imitation: same viewBox and 15-unit margin, same three-stop gradient, same eye rings and mouth curve, same placement transform. The twenty-five expressions were diffed against the app's own functions run straight from the bundle and come out character-identical; `engine/tests/golden` keeps a digest of each drawing.
- Pictures are written to `$XDG_RUNTIME_DIR/mission/team-avatars/` under a name made of their content. A bot that changed is a changed URL, so Qt cannot serve a stale copy, and a bot that did not change keeps its file. Files nothing points at are deleted after a minute, which leaves the one the panel may still be drawing alone.

## Avatar motion

A working (or notifying) bot moves like in the app sidebar: `mascot/motion.js` holds the app's own per-face table (bundle 0.1.136, `BD`/`Dze`) and its blink pauses; `BotRow.qml` samples it with a `Timer` only while the panel is open. `scripts/check-motion.mjs` pins the formulas and that every face in `mascot.json` has an entry; `scripts/check-settings.mjs` pins that `EngineHub.save` keeps the other keys of the live entry. The face also changes within its family as in the app (`oSe` dwell times in `FACE_DWELL`; the engine publishes each row's `variants[]`, a fade stands in for the app's spring morph). `scripts/check-silent.sh` proves `--no-notify` makes zero `notify-send` calls.

## Settings and languages

Translations live in `lang/`, one file per language, and `lang/index.js` imports them and holds the list. QML cannot resolve `import "lang"` to an `index.js` (a folder import looks for a QML module with a `qmldir`), so the imports are `import "lang/index.js" as I18n` (and `"../lang/index.js"` from `panel/`). Likewise `mascot/motion.js` sits beside `mascot.json`. Without a choice the language follows the Hyprland keyboard layout, then `LC_ALL` / `LC_MESSAGES` / `LANG`, then English.

To add a language: copy `lang/en.js`, translate every value, add the `.import` and the `LANGUAGES` entry in `lang/index.js`, map its keyboard layouts in `KEYBOARD_LANGUAGES`, and add the code to `expectedCodes` in `scripts/check-i18n.mjs`. Then `mise run i18n`, which checks that every language has the same keys, that every key is a string (or the three forms of a plural, as Polish has), that the plural rules pick the right one, and that every key is actually read by the QML — a key nobody reads fails the check.

Settings are stored in the widget's entry in `~/.config/omarchy/shell.json`: `language` (`auto` is the default), `notify` (only stored as `false`) and `costRange` (`day`, `week` or `month`; `day` is the default), set in the Settings page, and `costGroup` (`bot` or `model`; `bot` is the default), set on the costs page's LIST row. A default is stored as nothing, so an entry nobody touched stays `{"id": …}`, and Omarchy keeps the entry across a shell restart.

A sentence that counts something is one translation, not pieces glued together: `costSentenceDay`, `costSentenceWeek` and `costSentenceMonth` are plurals (three forms in Polish) with the named markers `{cost}` and `{tasks}`, and `I18n.trp()` picks the form and fills them. The footer writes them in capitals and short («$36.65 HOY · 96 TAREAS», at most 31 characters in any language) so the sentence and the costs button share one line. `scripts/check-i18n.mjs` fails when a form loses a marker. Each language also carries `monthsShort`, its twelve short month names, so any date the panel writes never borrows the machine's locale (the costs page's sparkline has no date axis, so nothing uses them today).

## Behaviour worth knowing

- **The card's text is the brief first.** For a delegated bot, `roster.rs::task_for` shows the brief its delegator wrote (the chief's order), and `origin.quote` (VK's own words) only when there is no brief. The chief (Boss) comes from VK directly and has no brief, so its card is unchanged.
- **Deleted bots in the cost list.** `usage.rs` receives the ids and current names in `bots.json` (rows are grouped by `botId` and named as the app names the bot today, so a rename does not split a row); spend whose `botId` is not there is added into one `deleted: true` row (the panel names it "Deleted bots", grey dot) and a row that rounds to $0.00 is left out, its money reported as `hiddenUsd` (models get the same rule: `hiddenModelUsd`). Visible rows plus `hiddenUsd` are the total. If `bots.json` cannot be read or is empty nobody counts as deleted. The roster's stamp (already watched) re-runs the cost read when it changes; nothing new is polled.

## Why it is written this way

- `ledger.rs` opens month files with `O_NOFOLLOW | O_NONBLOCK` and requires a regular file, so a symlink or a FIFO planted in `usage/` can neither redirect nor block a read.
- The ledger trusts inode, length and a 4 KiB guard before the old end because the app only appends; the 30-minute full reread is a net for hand edits.
- `paths::write_atomically` writes a temporary file and renames it, so a reader never sees half of a stamp or an avatar.
- Files the engine reads from the app's folders are opened with `paths::READ_FLAGS` (`O_NOFOLLOW | O_NONBLOCK`), so a planted symlink or FIFO is refused or harmless.
- `launch.rs` starts the app from the desktop file's `Exec` as an argument vector, never through a shell, in its own process group.
- `logwatch.rs` takes a lock in the runtime directory so two engines never announce the same turn, and narrows an old lock to 0600.
- `usage::clean` strips control and bidirectional characters from every name and text that came from a file, before it reaches a notification or the panel.
- `stream.rs` runs each tick under `catch_unwind` and reports a panic as an error line, so one bad read does not end the daemon.
- The release profile keeps `panic = "unwind"` so that `catch_unwind` has something to catch.
- `Panel.qml` and `Costs.qml` index data-driven maps with `Object.create(null)`, so a bot or model named `__proto__` is only a name.

## Binaries and releases

`bin/engine` is built and signed by [`build-engine.yml`](.github/workflows/build-engine.yml) on every push to `main` that changes `engine/`, `mise.toml`, `scripts/build-engine.sh` or the workflow itself. It runs the dependency audit and the tests first — if either fails, nothing is signed — then signs a build provenance attestation, commits `bin/engine` with `SHA256SUMS` and the attestation as `provenance.sigstore.json`, and starts [`verify-binaries.yml`](.github/workflows/verify-binaries.yml). [`release.yml`](.github/workflows/release.yml) publishes those files on a `v*` tag without rebuilding.

To check the binary yourself:

```sh
(cd bin && sha256sum -c SHA256SUMS)
gh attestation verify bin/engine --repo vk2r/mission --bundle bin/provenance.sigstore.json
```
