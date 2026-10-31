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
