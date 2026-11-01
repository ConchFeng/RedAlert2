---
layout: default
title: AI battle and spectator feature design
---

# AI battle and spectator feature design

This specification preserves the eight original ideas in delivery order. The
current implementation scope is features **1–6**; the maintainer deferred
features **7 and 8**. The automatic battle camera, live AI standings, and AI
series are implemented in current source. Commander selection and the second battlefield view are implemented;
feature 6 remains planned. Features
7–8 are design references outside the current implementation scope. These
extensions are not included in the 1.0.1 release.
Each numbered feature must finish its production path and validation before the
next implementation starts. Prerequisites discovered below belong to that feature.

The product focus is **30 local setup slots**: one player against up to 29 AIs,
or one full-map Observer watching up to 29 AIs. These features should make the
battle enjoyable to follow without covering the battlefield with controls.

## Existing baseline

Version 1.0.1 already supplies the offline 29-AI Observer default, Giant random
maps, opponent paging, continued AI combat after local elimination, and retained
results with View Battlefield, Play Again, and Return to Menu. See
[large skirmish](large-skirmish.md). Ordinary matches still wait for an explicit
result action. Network and campaign behavior retain their current owners.

The existing [commander personality system](ini-commander-personalities.md) has
strategy hooks and feedback, but no setup selector. Existing unit Follow and four
camera-position bookmarks are not a replay timeline. Standalone replay capture
currently admits Battle on authored loose/MIX maps; default Free For All and
generated Giant-map startup need support before promising their replay seeking.

## Shared interface contract

- Reuse the game's shell, font, button feedback, colors, and Options behavior.
  Use words for primary actions; icons may supplement labels.
- Spectator tools appear only for an offline Observer or an eliminated local
  player who can continue watching. They do not expose enemy information during
  active play. Playback tools use the replay's existing admission policy.
- Keep the default view compact. Open one auxiliary panel at a time. Panels
  never cover the minimap, Options, or the other primary spectator controls.
- Use color swatches together with names and text status. Color alone must not
  identify an AI or communicate elimination, tracking, or errors.
- Match screen hit boxes to drawn buttons in the current UI coordinate space.
  Controls need at least a 28-pixel hit height at normal scale and must remain
  inside the tactical viewport. Compact widths wrap controls into two rows.
- Support mouse operation without requiring hotkeys. Tab, Enter, Space, and Esc
  work in an opened spectator panel without also issuing tactical commands.
  Esc closes a tool panel first; otherwise it retains Options/Results behavior.
- Explain unavailable actions beside their controls. Loading, seeking, and
  generation show progress or an honest indeterminate state and a usable Cancel.
- Do not introduce per-shot popups, constant camera jumps, or audio spam. Keep
  an optional event list; coalesce bursts and preserve important notifications.
- Use localization keys with readable fallbacks. Respect existing UI scaling,
  whole-frame zoom, window size, cursor transforms, and focus/pause behavior.

Proposed compact toolbar, with future controls added only when implemented:

```text
Spectating   AIs: 29 / 29   [Watch the Action] [Standings] [More]
```

An expanded standings panel should remain readable rather than squeeze 29 rows
into the screen:

```text
AI Standings                    [Sort: Army] [Close]
Color  Commander / Faction       Army       Status
Blue   Computer 08 / Soviet      12,400     Fighting
Red    Computer 03 / Allied      10,200     Building
... seven visible rows ...
[Previous]                  Page 1 of 5       [Next]
```

A playing human sees none of these enemy standings. The existing retail sidebar
continues to own build controls and the minimap.

## 1. Automatic battle camera

**Current source: implemented.** The opt-in toolbar and camera director share the
existing camera and committed-frame event owners. The explicit
`skirmish-observer-camera` shell capture exercises startup, real AI combat, manual
minimap takeover, Resume Now, and a combat focus inside the viewport. Pointer
transactions check that simulation tick, frame, and hash remain unchanged.

### Purpose and entry

Make a large AI match watchable without continuously dragging around the map.
The spectator toolbar offers **Watch the Action**. Automatic movement starts
only after that action; entering a match never takes the camera away by surprise.
The active button becomes **Auto Camera: On**, with a clear Off action.

### Behavior

- Rank actual recent combat clusters, attacks involving structures, and actual
  superweapon activations. Use simulation facts, never decorative explosions as
  proof of damage or a weapon launch. Show a short reason such as **Base under
  attack** or **Heavy fighting** when changing focus.
- Prefer meaningful encounters between hostile sides. Ignore repairs, healing,
  allied fire, and long-expired events. Merge nearby events into one encounter.
- Hold a view for at least six wall-clock seconds before choosing another
  ordinary encounter. A genuine superweapon strike may interrupt the hold.
  Move smoothly through the existing camera owner and playable-area clamp.
- Keep the user's zoom. Version one does not automatically zoom in and out.
- Arrow keys, edge scrolling, right-drag panning, minimap navigation, camera
  bookmarks, Home, and explicit Follow give the user control immediately.
  Display **Manual control — resume in 10s** after the last manual navigation,
  with **Resume Now** and **Turn Off** available. Wheel zoom also counts as
  manual navigation. Clicking a spectator button does not.
- If nothing is fighting, show **Waiting for action** and retain the current
  view. Do not repeatedly tour empty bases or invent a battle.
- Options, loss of focus, finished results, and frozen battlefield inspection
  suspend automatic movement. A new match or restored world clears all outgoing
  targets; no stale camera target may survive world replacement.

### Ownership and acceptance

The app camera owner, [input/camera.rs](../src/app/input/camera.rs), remains the
only writer of the committed camera view. A bounded presentation director may
retain event scores and target intent; it never changes simulation, AI targets,
visibility, gameplay RNG, or match speed. Feed it the existing committed-frame
outputs, including firing and superweapon facts, without consuming other readers'
streams. Prune expired clusters and cap retained candidates; do not scan every
entity every rendered frame.

Accept when a real offline spectator match follows an active fight, manual
navigation wins immediately, the 10-second resume is visible and cancelable,
pausing/final results stop movement, and enabling the tool leaves simulation
hashes unchanged. Capture the actual toolbar and movement in the GPU client.

## 2. Live AI standings and battle events

### Purpose and entry

Answer **who is still alive, who has the strongest army, and where should I look?**
Show **AIs: alive / initial** in the compact toolbar. **Standings** opens a paged
panel, with seven rows and stable Previous/Next controls.

### Behavior

- A row shows color, stable player label, faction, commander when assigned,
  army value, building count, credits, and kills. Secondary columns may be
  selected in a Details view on small windows; never silently truncate values.
- Initial/alive AI counts include participating computer combatants only.
  Exclude the Observer, civilian/passive houses, and unused setup slots.
- Army value means the current configured cost of live, active combat units;
  exclude buildings, civilians, cargo counted through its carrier, and removed
  objects. Use the existing rules cost owner and document adjustments before
  shipping; do not label a health-weighted estimate as an exact army cost.
- Credits means current spendable balance, not lifetime income. Kills use the
  existing score/house counters. A tooltip explains every numeric column.
- Default to alive players first and army value descending. Allow sorting by
  name, army, buildings, credits, and kills. Stable entrant order breaks ties.
  Keep a selected player anchored while values refresh.
- Clicking a row centers on that house's live base through the camera owner;
  fall back to one live unit if the base is gone. An eliminated house with no
  valid target says **No surviving units** and does not move the camera.
- An optional event list shows eliminations and significant attacks with a
  time, player color/name, and **Go To** action. An expired target remains a
  readable historical event; it cannot dereference a removed object.

### Ownership and acceptance

Derive rows from the existing Houses, entity, rules, and score owners. Cache a
read-only projection at a bounded update rate, invalidate it on world replacement,
and retain no competing credits, kill totals, or defeat flags. Reuse the same
participant projection for the toolbar count and panel. Display changes never
reorder simulation objects or consume RNG.

Accept with 29 AIs, tied values, multiple simultaneous eliminations, a lost base,
all players on later pages, a resized/zoomed window, and an active-human match
where the panel is unavailable. Verify rendered text, hit testing, and source
counter consistency.

### Current implementation

The compact spectator toolbar shows the live AI count and opens **Standings**.
The panel displays up to seven color-coded entrants per page, with fewer rows
when the window is shorter. **Sort** cycles through army value, buildings,
credits, kills, and name; surviving AIs appear first and stable entrant order
breaks ties. Clicking an entrant visits its current live base or a surviving
unit and reveals its country, faction, commander, and all numeric statistics.
The selected entrant stays on screen while rankings refresh. Escape closes the
panel; the next Escape follows the normal Options path. Hover over a row or
its details for metric definitions.

The read-only projection refreshes at most twice per second. With the panel
closed it reads houses only; opening it immediately samples live objects.
Army value uses the existing per-house replacement-cost calculation for armed,
scoring unit types, including temporal, mind-control, and parasite weapons.
It excludes civilians, insignificant/unscored objects, cargo in limbo, unarmed
resource units, and healing-only support. An armed harvester can contribute;
unit damage does not reduce replacement cost. Buildings count live, on-map
structures; credits and kills come directly from the house's existing owners.
No display action changes simulation ordering, RNG, or hashes.

The optional historical event list is reserved for a later extension. The
`skirmish-observer-standings` production capture exercises all 29 entries,
Previous/Next, every sort choice, and base navigation through actual input.
These controls are current-source features and are absent from the 1.0.1 ZIPs.

## 3. AI league and automatic rematches

### Purpose and entry

Run a finite local series and see which AI entrants win most often. Offline
Skirmish gains **AI Series...**, available for Observer + AI matches. Normal
single-match behavior is unchanged.

### Setup, round flow, and scoring

- Setup contains a series name, match count (default 5; range 1–100), entrants,
  map rotation, seed policy, and **Start Series**. Show **29 AIs + Observer** and
  the resolved map capacity before starting. Keep custom seed lists in Advanced.
- Pin entrant identities independently of house labels, colors, and result-row
  sorting. Resolve random countries/commanders once for a series by default;
  an explicit **Randomize each round** option records each resolution.
- Use normal Skirmish launch, map acceptance, rules loading, and retirement for
  every match. A generated map retains its actual accepted bytes and seed; a
  rotation cannot silently regenerate a different map under the same identity.
- Results show **Match 3 of 5**, the match result, series standings, **Next Match
  in 10s**, **Start Now**, **Pause Series**, and **End Series**. Inspecting the
  battlefield pauses the countdown; returning to results can resume it.
- Record wins, draws, and completed matches. A natural winner receives one win;
  a timeout is labeled a draw, not awarded to the largest displayed army.
  Version one is Free For All; team leagues need their own scoring contract.
- **Stop After This Match** keeps the current battle running and prevents the
  next launch. **End Series** on results returns through ordinary teardown.
- A missing map, invalid rules, loading failure, crash, or abort never counts
  as an AI defeat. Show **Retry / Skip / End Series** with the affected round.
  Never loop automatic retries. Completed rounds remain recoverable.

### Ownership and acceptance

One app-level coordinator owns the series schedule and durable ledger beneath
`GameConfig.paths.settings_dir()`. Store a round result exactly once before
launching the next round, keyed by series and round identity. Recover interrupted
rounds as incomplete. Reuse [terminal score snapshots](../src/sim/score.rs),
[post-match actions](../src/app/match_runtime/post_match.rs), and
[Skirmish launch](../src/app/shell_skirmish.rs). Do not use localized result labels
as identities or add league scoring to ordinary simulation.

Accept cancellation at the countdown boundary, duplicate result delivery,
application restart, map-load failure, intended seeds, and restoration of the
user's prior ordinary Skirmish setup. A series is finished only after its ledger
and displayed standings agree.

### Current implementation

Choose **Observer**, **Free For All**, and at least two computer opponents in
ordinary Skirmish setup, then open **AI Series...**. The form defaults to five
matches and the current map. Select maps in rotation order, choose 1–100 matches,
and use Advanced for the first simulation seed or an explicit seed per match.
The normal setup remains available unchanged after Cancel or End Series.

Entrants keep their accepted countries and colors by default. **Randomize
countries each round** uses the existing country-assignment owner while keeping
series colors stable. Each accepted round recipe is saved before loading; a
retry or recovery reuses that recipe. Generated maps retain their exact accepted
SED input and start staging, then regenerate through the ordinary loading owner.
Authored map fingerprints are checked before every load. Maps that skip their
results screen cannot participate.

Natural results are recorded once before the next match is admitted. The
existing score screen supplies **Start Now**, **Pause Series**, **End Series**,
and **Series Standings / Match Results**. The standings reuse the score table's
paging with Wins, Draws, Played, and Losses columns. The next match starts after
ten seconds unless paused. View Battlefield, focus loss, and manual Pause Series
suspend the countdown. **Stop After This Match** in the spectator toolbar lets
the ongoing battle finish and retains its final results.

**Recover Last Series** reads the saved descriptor, accepted launches, and
completed rounds under the configured settings directory's `ai-series` folder.
Interrupted rounds remain incomplete; Retry restarts the accepted round rather
than restoring a mid-match world. Historical rows use neutral text colors until
a match is loaded. A normal failure offers Retry, Skip Match,
or End Series; skipping never records a win, draw, or defeat. Invalid immutable
result files are preserved and offer End Series instead of an automatic retry
that could overwrite history. Loading another scenario or successfully
restoring a save ends the active series attempt, preserves its durable history,
and keeps the unrelated game outside series scoring.

The app coordinator is [`ai_series.rs`](../src/app/ai_series.rs), with its
schedule/countdown model and immutable result repository beside it. The
`skirmish-ai-series-setup`, `skirmish-ai-series-start`, and
`skirmish-ai-series-results` capture routes exercise the real setup, full-map
29-AI launch, and natural results/rematch/standings paths. These are current-source
features and are absent from the published 1.0.1 packages.

## 4. Named AI commanders with different styles

### Purpose and entry

Make opponents recognizable and explain their intended behavior. Add an
**AI Commanders...** setup panel instead of another narrow column in the retail
seven-row setup. Show country, difficulty, commander, and a one-line description.

### Behavior

- Offer **Standard**, **Random compatible commander**, and named definitions
  admitted by the existing country-filtered personality catalog.
- Standard remains the default. It retains the current AI behavior and plays no
  commander clips. **Apply to All: Standard / Random** avoids 29 repetitive edits.
- No custom catalog means **No custom commanders available with these game
  rules**. The match remains playable with Standard.
- Retain a commander after a country change only when compatible; otherwise
  reset to Standard and show the change. Missing saved names receive the same
  visible fallback rather than a silently different identity.
- Persist names, not catalog indices. Carry the choice through setup, accepted
  launch slots, replay startup, saves, and the installed House personality.
- Feedback audio/video has its own switch. Muting feedback does not disable the
  commander's strategy. Never advertise tank-rush/air/defensive behavior based
  solely on a display name or portrait: validate actual compatible trigger
  preferences against the loaded rules first.

### Ownership and acceptance

Extend [the existing personality owner](../src/rules/personality.rs),
[launch slots](../src/skirmish_launch.rs), bootstrap, and setup persistence.
Keep its stable-key selection without extra gameplay RNG draws. Do not create a
second AI planner or call an online language model every tick.

Accept paging/persistence, country compatibility, normal production launch of a
named profile, unchanged Standard behavior, and independent feedback controls.
Document the real validated strategy of every shipped named profile.

### Current implementation

**AI Commanders...** opens a separate paged setup panel for all active computer
slots. Rows show country, difficulty, the selected name, and a strategy summary
derived from the profile settings. The chooser offers Standard, Random
Compatible, and named profiles admitted by the current country. Random countries
offer unrestricted named profiles or resolve Random Compatible after country
assignment. Bulk Standard/Random, independent feedback, Apply/Cancel, and
keyboard navigation use the existing shell presentation and input owners.

Names persist in setup, accepted launch slots, supported replay startup, AI
series recipes, and saved Houses. Incompatible or missing setup names visibly
reset to Standard. Runtime catalog indices are derived from saved names, and
restoration does not reapply initial strategy timers. Standard leaves the House
without a personality and preserves ordinary simulation hashes and RNG draws.

Retail rules contain no custom profiles; the panel explains their absence and
keeps Standard available. No named strategies are bundled. The panel reports
configured team delay, trigger chance, and preference count rather than making
unvalidated unit-specialization claims. See the
[commander guide](ini-commander-personalities.md) for rules and persistence.

The `skirmish-ai-commanders` capture covers every AI slot, Apply/reopen, named
selection with custom rules, and compact controls at 800×600 and 640×480.
These source features are absent from the published 1.0.1 packages.

## 5. A second battlefield window

### Purpose and entry

Watch two fronts without losing the main view. **More → Second View** opens one
small floating battlefield panel. It starts closed, with an explicit Close and
**Swap Views** action; dragging/resizing never issues world commands.

### Behavior

- The second view has its own camera target and user zoom. It can stay on a
  chosen location, follow a chosen player, or follow current action.
- Clicking its **Swap Views** button exchanges camera centers/zoom through the
  existing camera owner. Ordinary clicking inside the panel selects a location
  for viewing only; it cannot command units.
- Start with one panel, bounded to the tactical viewport, with a readable title
  and a capped refresh rate of 15 FPS. Hidden/minimized panels do not render.
  Expose **Low / Normal** refresh quality if measurements justify it.
- Reuse loaded assets, atlases, and immutable scene resources. The extra view
  must not advance simulation a second time or overwrite main-view culling,
  fog, clipping, input coordinates, or retained render transactions.
- Options/results stop automatic following; world replacement clears panel
  targets. The view respects the same admitted spectator visibility.

### Ownership and acceptance

Add a second presentation view through the existing rendering pipeline, sharing
GPU resources. Profile the added CPU/GPU cost on a 29-AI map before calling the
feature usable. Accept independent views, swapping, clipping, resize, HUD input
capture, world replacement, and unchanged simulation hashes. Render/readback
checks must establish that both cameras show their intended scene regions.

### Current implementation

**More → Second View** opens a draggable, resizable panel below the spectator
controls. Fixed, Player, and Action modes have their own center and zoom.
**Swap Views** uses the existing camera clamp and exchanges both centers and
zooms. Minimize retains the panel without rendering its scene; Close releases
its input capture. Mouse, wheel, keyboard, and touch gestures remain viewing
operations. Unsupported small viewports show a larger-window notice.

The secondary projection uses the ordinary world builders and draw dispatcher,
sharing atlases, parsed voxel models, retained barrel/sinking history, and
immutable effect inputs. Camera buffers, posed-image pages, fog projection,
depth attachments, and destination-edit scratch remain view-local. Additional
beam projections do not advance effect lifetimes or process Main RNG. Bolts
outside the main view use stable presentation-local noise; one-pixel water/ore
sparkles remain in the main view. World replacement resets the auxiliary view.

Scene refresh is capped at 15 FPS. The
`skirmish-observer-second-view` production capture exercises a generated 29-AI
Observer match, pointer transactions, swapping, independent player focus and
zoom, drag/resize bounds, close/reopen, and minimize/restore. GPU readback checks
both distinct scene regions and the panel's exact composition placement;
capture-only before/after hashes guard actual secondary renders. Two focused
GPU regressions protect pending camera uniforms and destination-edit isolation.

A local release-profile observation on an Apple M4 Max, at a 356×164 secondary
scene and 125% zoom, measured roughly 6.2 ms of CPU preparation and 0.48 ms of GPU
scene work per refresh. This is a 29-AI scenario observation, not a cross-device
or 20,000-unit performance guarantee. Linux and Windows runtime captures remain
unmeasured. The feature is present in current source and absent from 1.0.1 ZIPs.

## 6. Replay bookmarks and unit tracking

### Purpose and entry

Return to meaningful battle moments and follow interesting units. Provide a
**Highlights** list with timestamps, optional short labels, **Add Bookmark**, and
**Go To**. Do not confuse the four existing spatial camera bookmarks with time.

### Required replay foundation and behavior

- First extend the existing replay startup owner to reproduce Free For All,
  29-AI participants, generated-map bytes, accepted settings, rules, and seed.
  Verify initial and final hashes before enabling their timeline UI.
- A bookmark stores recording identity, committed tick, camera center/zoom,
  optional stable tracked-object ID, and label. This is presentation metadata;
  it never enters simulation commands, hashes, or gameplay RNG.
- Go To performs real temporal navigation. Restore a validated checkpoint or
  reconstruct from startup and fast-forward through the existing frame owner.
  Show **Seeking to 08:44...** and Cancel. Suppress intermediate audio and
  transient presentation; rebuild presentation at the validated destination.
- Version one needs pause, resume, forward speed, and bookmark jumps. Smooth
  reverse playback is not required. Never expose a jump button that only moves
  the camera while leaving the recording at the wrong time.
- **Follow Unit** reuses the existing camera target owner with read-only observer
  selection. A destroyed, detached, or unavailable target releases cleanly and
  shows **Tracked unit no longer available**. No replay input queues gameplay.
- Invalid recording/map/rules combinations show a readable failure and return
  to the browser. Finished recordings retain their final view for inspection.

### Ownership and acceptance

Extend [ReplayStartup/ReplayPlayback](../src/sim/replay.rs),
[match diagnostics](../src/app/match_diagnostics.rs), and the existing camera
owner. Respect replay input admission rather than injecting selection commands.
Accept a generated Giant/FFA recording through production startup and playback,
backward/forward jumps to matching hashes, canceled seeks, dead unit tracking,
and mismatched resources. Exported recordings need the required map identity and
bytes, not personal configuration or original retail assets.

## 7. Survival battles with an escalating enemy

**Deferred at the maintainer’s request.** Preserve this design for reference;
do not implement this mode as part of the current work.

### Purpose and entry

Offer a separate **Survival** custom mode with **Play / Watch**, **Difficulty**,
**Waves**, and **Preparation Time**. Version one uses one authored compatible map
with validated entry points and objectives, existing retail units, ten configured
waves by default, and a final assault. Arbitrary random-map support follows later.

### Behavior

- Show the actual participant allocation before launch. The hostile wave house
  consumes a slot: at most player/Observer + 28 defender AIs + one hostile wave
  house. Do not promise an extra faction beyond the 30-slot setup limit.
- HUD: **Wave 3 / 10 · Enemies: 42 · Next wave: 00:35**. Preparation and wave
  timers use committed simulation time and freeze under a simulation pause.
- A wave spawns once through existing team/object constructors, normal placement,
  and lifecycle. Authored valid unit lists define escalation; do not invent a
  giant boss requiring unavailable artwork. The final assault is previewed as
  such in setup and the HUD.
- Preparation/pending waves keep the hostile side eligible. Ordinary short-game
  defeat must not end the battle before the first wave or between waves.
- Blocked entries retry deterministically for a bounded configured interval;
  report a failed wave clearly if no valid spawn remains. Never bypass terrain
  or place objects inside occupied footprints.
- Victory waits for all configured waves and their active enemies to clear.
  Defeat follows the defended objective/side's explicit loss condition.

### Ownership and acceptance

Use a simulation-owned custom-mode controller integrated in the named frame
phase, serialized and hashed with its wave index, timers, pending spawns, and
terminal state. Reuse [team reinforcements](../src/sim/team_script_vm/reinforcements.rs),
[world spawning](../src/sim/world/world_spawn.rs), scenario configuration, and
existing loss/score owners. Keep retail Battle/FFA semantics unchanged.

Accept exact-once waves, pause/save/load, occupied entries, no premature defeat,
final victory, replay reconstruction, and unchanged mode-off hashes. Keep new
mode rules explicit and opt-in.

## 8. Undead Battlefield

**Deferred at the maintainer’s request.** Preserve this design for reference;
do not implement this mode as part of the current work.

### Purpose and entry

A separate optional custom mode explains its rule in one sentence:
**Destroyed ground combat units return once as enemies after a delay.**
Use existing unit art with a distinct undead color/name. Advanced contains
revival delay, active-undead cap, and strength; ordinary Skirmish never enables it.

### Behavior

- Reserve a hostile undead house within the 30-slot limit. Setup shows the
  reduced ordinary-AI capacity. Undead units do not become additional AI players.
- Version-one eligibility is ordinary ground combat vehicles and infantry.
  Exclude structures, aircraft, naval units, cargo, spawned minions, terrain,
  sales, off-map cleanup, and units already marked as undead.
- Capture one qualifying authoritative combat-death record before deletion.
  Let the original object finish normal death, detach, and kill accounting.
  Revival constructs a new stable object ID; it never revives old pointers,
  passengers, selections, capture links, or production ownership.
- Each original unit can produce at most one undead unit. Prior undead never
  creates another generation. A deterministic queue applies the active cap;
  blocked nearby placement retries for a bounded interval and then expires.
- Group notifications by battle location: **Undead rising in 00:05** and
  **Undead: active / cap**. Avoid one popup or sound per corpse.
- Final victory requires the remaining original side to clear active and pending
  undead. If every original side dies, finish with the documented undead-loss
  result after outstanding bookkeeping; do not leave a match running forever.

### Ownership and acceptance

Use the simulation custom-mode owner, the shared qualifying-death boundary, and
normal fresh spawning. The retail mission `Zombie=` flag means frozen mission
behavior and must not be reused for revival. Persist/hash queue, frame deadlines,
provenance, generations, and terminal state. Preserve ordinary native death and
RNG behavior when disabled.

Accept simultaneous deaths, crush versus damage where eligible, excluded cleanup,
cap/blocked cells, no repeated generations, correct kill credit, save/load,
replay hashes, and mode-off behavior. Presentation effects never schedule spawns.

## Delivery and validation

Implement in the numbered order, one complete player-visible chain at a time.
For each feature: extend its existing owners, add one to three meaningful focused
regressions per cohesive behavior, validate its direct consumers, exercise the
real screen, and record measured limitations. Do not advertise a design as shipped.

For presentation features, compare simulation hashes with the tool enabled and
disabled. For custom gameplay modes, new state and RNG must be intentional,
snapshot-safe, and deterministic across supported architectures. Feature-off
behavior retains the ordinary game path. Do not build the entire project or
publish a new release without an explicit request.

Each feature commit states its actual validation. Keep documentation and
implementation commits separate. A feature stays open when a required production dependency is missing.

## Inspiration

- [Beyond All Reason: activity camera and picture-in-picture](https://www.beyondallreason.info/microblogs/179).
- [CaptureAge: live statistics and graphs](https://captureage.com/cade/docs/cade-latest/match-statistics).
- [AI Arena: round-robin leagues](https://aiarena.net/wiki/ladders/).
- [CaptureAge: replay bookmarks and tracking](https://captureage.com/cade/docs/cade-latest/pro).
- [Zero-K: Chicken Defense](https://zero-k.info/mediawiki/Chicken_Defense).
- [Beyond All Reason: Scavengers](https://www.beyondallreason.info/microblogs/128).
- [Beyond All Reason: Zombies](https://www.beyondallreason.info/microblogs/209).
