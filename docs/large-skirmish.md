---
layout: default
title: Large skirmish and random maps
---

# Large skirmish and random maps

VERA extends the retail eight-player Skirmish to 30 participants (one local player
or observer and up to 29 computers), and the Create Random Map dialog to 30
players with two larger sizes. Retail-sized setups keep the native behavior; each extension
activates only past the retail limits. This page records the owners, the design
decisions and the pitfalls found while building the extension, so later work
starts from them.

## What changed

| Area | Behavior | Owner |
| --- | --- | --- |
| Player count | A map whose `[Basic] MaxPlayers` names 9–30 authored starts seats that many players. | [waypoints.rs](../src/map/waypoints.rs) `skirmish_player_capacity`, [player_name.rs](../src/ui/skirmish_shell/state/player_name.rs) `selected_player_capacity` |
| Paging | The retail eight rows stay; **Previous** and **Next** page the 29 opponents seven at a time. The local row always shows in slot 0. | [player_name.rs](../src/ui/skirmish_shell/state/player_name.rs) `row_slot`, `slot_row`, `page_opponents`; [combos.rs](../src/ui/skirmish_shell/state/combos.rs) `combo_rect` |
| Colors | 22 VERA colors follow the retail eight, offered only on maps for more than eight players. Random colors take the first unused extended color once the retail eight are in use. | [color_scheme.rs](../src/rules/color_scheme.rs) `VERA_PLAYER_COLORS`, [skirmish_colors.rs](../src/rules/skirmish_colors.rs) `extended_player_priorities`, [skirmish_launch.rs](../src/skirmish_launch.rs) `draw_collision_free_color` |
| Persistence | `Slot01..07` stay in `[Skirmish]`; rows 8–29 go to `[SkirmishVERA]` in `VERA.INI`. LAN and WOL profiles keep only the native seven. | [persistence.rs](../src/app/frontend/skirmish_session/persistence.rs) |
| Random map players | Skirmish's Players slider reaches 30; network rooms keep 8. | [random_map_setup.rs](../src/ui/skirmish_shell/state/random_map_setup.rs) `with_player_limit`, [shell_skirmish.rs](../src/app/shell_skirmish.rs) |
| Random map sizes | **Huge** (option 4) and **Giant** (option 5) double and quadruple the Very Large side; more than eight players grow the side by `sqrt(players / 8)`. The padded map is capped at `map_w + map_h <= 1000`. | [rmg/mod.rs](../src/map/rmg/mod.rs) `generated_dimensions` |
| Tools | `tools.scale_map` writes `MaxPlayers` past eight starts; `sim-bench` seats up to 29 computers. | [scale_map.py](../tools/scale_map.py), [sim_bench_match.rs](../src/bin/sim_bench_match.rs) |

Examples, with 8 players → 30 players: Very Large is 120 → 232 cells a side,
Huge 240 → 464, Giant 480 → 492 (capped).

## Island War random maps

Choose **Island War** in Create Random Map, then set **Islands** to any number
from **2 through 30**. The count is independent of Players: extra islands can be
unoccupied. This is a VERA map type (`MapType=5`), separate from the retail types.
Saved `.SED` recipes retain the choice as `[RandomMap] VERAIslandCount` and include
it in the Island War map digest.

The water phase reuses the archipelago partition and land-blob code to separate
the requested islands with sea. Island War grows undersized map dimensions to
leave buildable interiors and water gaps. Huge and Giant remain useful for larger
naval battles.

Starting positions are split evenly across buildable islands, with at most one
start's difference. Larger islands receive any remainder. For example, 30 starts
on eight islands produce six islands with four starts and two with three. The
existing farthest-point selector spreads starts within each island, and the
existing clearing pass protects the land around them. This balance describes the
generated start slots; unused slots and explicit player position choices can
change the occupied distribution.

The owners are [options.rs](../src/map/rmg/options.rs),
[water.rs](../src/map/rmg/phases/water.rs),
[starts.rs](../src/map/rmg/phases/starts.rs), and the existing
[setup dialog](../src/ui/skirmish_shell/state/random_map_setup.rs). Unlike the
retail start filter, Island War admits clear land outside the reference island
and retains islands below 3% of the map area. The full retail-terrain regression
`island_war_separates_islands_and_balances_starts_on_retail_terrain` checks exact
disconnected island counts, balanced starts, and unique starting cells.

Original `Generate`
(`0x00598960`, type dispatch `0x00598AF3..0x00598B14`) sends both Inland and
Mountainous through the carved-water driver (`0x0059C580`), which admits at most
one successful river system and one lake. Legacy Mountainous recipes therefore
can look mostly like rolling land, including maps with no cliff tiles.

New Mountainous generations enable `[RandomMap] VERAMountainRidges=yes`. On maps
whose smaller interior side exceeds the retail maximum of 120 cells, the
[mountain layout](../src/map/rmg/phases/mountains.rs) seeds broad plateaus with
four-level boundaries and a clear apron. Plateau density grows with map area.
The existing region, ramp, cliff, start-clearing and Hills phases then produce
the finished terrain. The layout uses integer geometry and the existing MapGen
RNG, and runs before starting positions and resources are chosen. It is an
intentional VERA extension; retail-sized maps keep their original terrain path.
Older `.SED` files omit the key and retain the legacy mountain layout.

The shared ramp driver also uses the actual uphill region as the carve receiver,
matching `RmgRegion::CarveConnectorsOrBridges` (`0x005905D0`, receiver selection
`0x005907F8..0x005908EF`). This allows a higher-ID plateau to connect to a
lower-ID lowland. The Giant retail regression checks substantial cliff and ramp
tiles, starting slots, and movement-zone connectivity of the raised ground.

## Default AI spectator match

A fresh offline Skirmish defaults to one **Observer** and 29 **Normal** computers
in **Free For All**. This mode keeps the computers hostile to each other; ordinary
Battle may ally its computers through the retail paranoid-AI rules. If the current
map lacks enough starts, the first setup visit opens Create Random Map with
30 players and Giant selected. Choose **Generate Map**, then **Use Map**.
Generation remains explicit so opening Skirmish does not block on a large map.

The local Side menu offers **Observer** first. Choose Random or a country to play.
The observer has permanent full-map terrain visibility from loading, receives no
starting army, and does not consume an AI starting position. Its reveal does not
change the computers' visibility. `[SkirmishVERA] ObserverMode` remembers the role;
AI difficulty and other existing setup preferences retain their current owners.
A saved 29-AI observer preset requests a suitable random map once per process if
its generated map is no longer selected. Explicit smaller matches remain available.

Offline observers and eliminated players stay in the battlefield while hostile
AI sides remain. Escape opens Options, allowing them to leave. When the battle
ends, the existing paged score screen waits for an explicit action. **View
Battlefield** retains the frozen world with camera movement and zoom; Escape
returns to the same results. **Play Again** reuses the normal Skirmish setup and
launch transaction, while **Return to Menu** returns through the existing shell.
These result actions do not apply to campaign or network matches.

## Automatic battle camera in current source

This feature is available on main after 1.0.1; the 1.0.1 ZIPs do not include it.
An offline Observer or eliminated local player can choose **Watch the Action**
in the compact battlefield toolbar. The camera follows actual hostile fighting,
base attacks, and offensive superweapon launches, keeps the current zoom, and
holds ordinary encounters for at least six seconds. With no recent action it
keeps its position and says **Waiting for action**.

Manual scrolling, minimap navigation, view bookmarks, Follow, zoom, and touch
panning take control immediately. The toolbar shows a ten-second manual-control
countdown with **Resume Now**; **Turn Off** leaves the camera manual. Escape
retains Options behavior. Options, focus loss, final results, and a replaced
world stop or reset automatic movement. Campaign, network play, and an active
human player's view do not expose these tools.

The camera director lives in
[input/camera/spectator.rs](../src/app/input/camera/spectator.rs); its controls
live in [spectator_tools.rs](../src/app/presentation/spectator_tools.rs). It reads
existing frame facts and never changes the simulation or consumes gameplay RNG.
See the [feature design](ai-battle-features.md) for the remaining roadmap.

## Design decisions

- **Color priorities, not indices.** A shell color value is the priority gamemd
  passes to `Game_GetLinkedColor` (`0x0069A310`). Extended colors use priority
  `2 * entry + 1`, which the native tail passes through unchanged as that
  `[Colors]` entry's odd scheme. The VERA entries are appended after every rules
  entry, so no retail or map-named color changes index. The house ramp texture
  has 64 rows (`MAX_HOUSES` in [palette_textures.rs](../src/render/palette_textures.rs)).
- **Separate persistence section.** Writing rows 8–29 into `[Skirmish]` changed
  which duplicate key the native-sized lookup returned, breaking the read-back of
  a hand-edited file. Keeping them in `[SkirmishVERA]` leaves the native section
  untouched.
- **Network rooms stay at eight random-map players.** Their `'W'` start staging
  ([preview.rs](../src/map/preview.rs) `START_STAGING_SLOTS`) carries eight
  cells.
- **The 512-wide constants are the cell-array width.** The generator's `0x200`
  clamp extents and the `y * 512 + x` patch ids now come from
  `RmgGrid::cell_array_width()` ([grid.rs](../src/map/rmg/grid.rs)): 512 for
  every retail-sized map, 1024 for larger ones.

## Fixes found on large maps

| Symptom | Cause | Fix |
| --- | --- | --- |
| New size labels showed `MISSING:'VERA:…'`. | `CsfFile::text` returns `MISSING:` for an absent key, so a fallback is used only when no string table is loaded. | `localized_vera_label` in [text.rs](../src/app/frontend/skirmish_shell_render/text.rs) shows the fallback for missing `VERA:` keys only. |
| Huge and Giant maps aborted with a stack overflow. | The shore, island-pinch and hill-corner passes recurse cell by cell, as the original does. | `generate_map_observed` ([build.rs](../src/map/rmg/build.rs)) runs on a thread with a 1 GiB stack reservation, keeping the native visit order. |
| The Regions stage grew with the square of the area (23 s for one Huge map in a test build). | Each region rescanned every map cell to count and border itself. | [regions.rs](../src/map/rmg/phases/regions.rs) keeps each region's cells and sorts them into native scan order. 120 retail-sized maps were byte-identical before and after. |
| A 30-player Huge islands map placed no starts. | The sea became the largest zone component, so no clear cell was reference ground; the original selects from nothing and faults. | [zones.rs](../src/map/rmg/phases/zones.rs) falls back to the largest clear component when the largest one holds no clear ground. |
| Starting a Giant/30-player map aborted with a stack overflow in `flood_fill_node_index`. | The base-zone fill under native 56C510 recursed once per adjacent-row run, about 14,600 frames deep on that map, past the 8 MB main-thread stack. | [zone_build.rs](../src/sim/pathfinding/zone_build.rs) drives the same run order from an explicit stack. |

An optimized build generates a 30-player Huge map in about 3 s and a Giant one in
about 3.5 s; most of it is the Water stage, which reruns the whole-map shore pass
after each land blob.

## Residuals

- A generated Giant/30-participant observer match has reached 9,000 committed
  ticks through the production GPU client, with full-map reveal and all 29 Normal
  AIs still active and hostile. Extended headless runs cover AI development and
  combat; these bounded checks do not establish maximum-unit GPU performance or
  unlimited client stability. See the
  [screenshot walkthrough](30-player-showcase.md).
- The browser build has no threads, so a Huge or Giant random map generates on
  the caller's stack and may overflow.
- Network rooms keep eight retail colors past eight players and eight random-map
  players.
- Retail Phobos `SkirmishUnlimitedColors` hands colors to the spawner, so the
  Skirmish shell offers no extended colors under it.
- The paging buttons have no native art or strings; they reuse the right-panel
  button art and English captions.

## Working notes for agents

- `cargo test` builds the unoptimized test profile. Time generation with
  `--profile fast`; unoptimized timings are 10–20 times slower.
- Regression tests: `a_thirty_player_map_pages_its_opponents_through_the_seven_ai_rows`
  and `extended_colours_are_offered_and_launched_only_past_eight_players` in the
  shell tests; `vera_sizes_and_player_counts_extend_the_retail_dimensions` in
  `rmg`; `a_giant_map_serpentine_fills_one_zone_without_deep_recursion` in
  `zone_build`; and the slow retail-tile test
  `thirty_player_huge_and_giant_maps_seat_every_start` in
  [random_map_ramp_end_tests.rs](../src/app/random_map_ramp_end_tests.rs), run with
  `--profile fast -- thirty_player_huge_and_giant --ignored`.
- The synthetic one-by-one tile set in the `rmg` unit tests does not place starts
  like retail tiles; judge start placement on retail tiles.
- To check a 30-house AI-only match headlessly:
  `python -m tools.scale_map target/scale/match30.map --starts 30`, then run
  `sim-bench --scenario skirmish --observer --computers 29 --map <absolute path>
  --ticks 9000 --client-timing`. This uses the retail Free For All mode and an
  actual observer; `--frame-profile` requires a `telemetry` build. Add
  `--slow-frame-ms 50` with profiling to report slow ticks and their phase costs.
- `tools.cargo_run` stages the files it validates and skips the tests when the
  tree changes during a build ("Source changed during Cargo"); rerun the same
  command, which reuses the build. Check staged content before a commit.

### Live standings in current source

The observer toolbar shows **AIs: alive / initial**. Open **Standings** to browse
up to seven AIs at a time. Shorter windows show fewer rows so the controls and
selected player's details remain accessible. **Sort** cycles Army, Buildings,
Credits, Kills, and Name, keeping surviving players first. Click an AI to visit
its live base, or a surviving unit if its base is gone. The panel leaves part of the battlefield open, and
the camera keeps the watched base or fight visible beside it. A house with no live
target leaves the camera in place and shows **No surviving units**.

The selected AI's details include its country, faction, commander, army
replacement cost, live structures, spendable credits, and score kills. Hover
for metric definitions. Army replacement cost excludes civilians, cargo,
unarmed resource units, and healing-only support; damaged combat units retain
their full replacement cost. Standings refresh at most twice a second and are
available only to an offline observer or a player who has continued watching
after defeat. Escape closes the panel.

The projection is owned by
[`spectator_standings.rs`](../src/app/presentation/spectator_standings.rs), with
layout and navigation in
[`spectator_standings_ui.rs`](../src/app/presentation/spectator_standings_ui.rs).
This feature is available in current source, not the published 1.0.1 packages.

### AI series in current source

For an automatic local championship, select Observer and Free For All, retain at
least two AIs, and open **AI Series...**. Use a name, 1–100 matches (default five),
and one or more maps with enough setup slots. Selected maps repeat in the order
you choose. Advanced accepts a first simulation seed or exactly one custom seed
per match. The generated map's accepted seed input and start positions remain
fixed independently of the successive simulation seeds.

Countries and colors remain fixed by default. **Randomize countries each
round** changes the country assignments while preserving the entrants' colors.
Accepted recipes are stored before loading so Retry and recovery use the same
match identities.

After a natural finish, the result stays visible with a ten-second countdown.
Use **Pause Series** to study it, **Start Now** to advance immediately, or
**Series Standings** to see wins, draws, completed matches, and losses for all
entrants through the existing score pages. View Battlefield pauses the countdown.
During play, **Stop After This Match** lets the current battle finish without
starting another one. End Series restores the ordinary setup.

**Recover Last Series** restores saved history and leaves interrupted matches
incomplete. Loading failures provide Retry, Skip Match, and End Series; skipping
never counts as an AI defeat. Corrupt immutable results are retained and stop
advancement rather than replacing history. Loading another scenario or
successfully restoring a save ends the active attempt; saved series results
remain recoverable and the unrelated game is not scored. Authored map changes are reported
before loading; maps configured to skip results are unavailable for a series.

The coordinator lives in [`ai_series.rs`](../src/app/ai_series.rs). Series records
are stored beneath the configured settings directory, not in the game assets.
AI series are available in current source, not the published 1.0.1 packages.
