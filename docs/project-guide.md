# Project guide

This repository, `ConchFeng/RedAlert2`, develops VERA20k: a Rust reimplementation
of the Red Alert 2: Yuri's Revenge engine. Players supply the original game assets.
The project aims to reproduce native behavior while supporting larger battles;
its scale targets do not establish a performance guarantee or complete parity.

Use this guide to locate the owner of a change. The current code is authoritative;
[AGENTS.md](../AGENTS.md) defines the engineering and collaboration contract.
[The documentation index](index.md) routes to detailed guides.

## Workspace and entry points

The [workspace manifest](../Cargo.toml) contains four crates. Its default members
are the client and core; the relay crates are separate workspace members.

| Crate | Manifest and library entry | Responsibility |
| --- | --- | --- |
| `vera20k` | [Cargo.toml](../Cargo.toml), [src/lib.rs](../src/lib.rs) | Windowed application, rendering, UI, audio and asset tools; re-exports core modules. |
| `vera20k-core` | [src/Cargo.toml](../src/Cargo.toml), [core_lib.rs](../src/core_lib.rs) | GPU-free assets, rules, maps, simulation, transport and headless scenario loading. |
| `vera20k-relay-protocol` | [Cargo.toml](../services/relay-protocol/Cargo.toml), [lib.rs](../services/relay-protocol/src/lib.rs) | Shared authenticated datagrams and opaque command ordering; owns no gameplay simulation. |
| `vera20k-relay` | [Cargo.toml](../services/relay/Cargo.toml), [main.rs](../services/relay/src/main.rs) | Standalone UDP relay using the shared protocol and an encrypted journal. |

The ordinary desktop executable starts in [src/main.rs](../src/main.rs).
`main()` handles probes and diagnostics, then `run()` parses the launch mode,
creates `App` and enters the winit event loop.
[handler.rs](../src/app/handler.rs) implements `ApplicationHandler`;
[initialize.rs](../src/app/initialize.rs) implements `App::initialize()`.

Other entry points share existing engine owners:

- [headless_scenario.rs](../src/headless_scenario.rs) loads a scenario without a
  window or GPU through `load()`, `load_session()` or the explicit INI-root API.
- [asset.rs](../src/bin/asset.rs) exposes the headless asset browser implemented
  in [asset_tools](../src/asset_tools/mod.rs).
- [web.rs](../src/web.rs) exports browser initialization and launch functions.
  Browser packages require the explicit `wasm` feature and browser target.
- [cncnet_spawn.rs](../src/app/frontend/cncnet_spawn.rs) parses external `-SPAWN`
  requests; the [CnCNet installer profile](../integrations/cncnet/README.md)
  configures only a bounded single-player client workflow.

Desktop skirmish, campaign launch, save/restore and networking have production
paths. Their existence does not establish complete campaign gameplay, arbitrary
mod compatibility or cross-platform runtime validation. Browser single-player,
online and LAN packages have distinct feature and transport requirements; see
[WASM](wasm.md), [networking](Networking.md) and [capabilities](capabilities.md).

## State ownership and frame flow

[AppState](../src/app/state.rs) composes application domains. It coordinates
loading, sessions, presentation and persistence rather than owning another copy
of each gameplay decision.

[ProcessAssets](../src/app/process_assets.rs) retains the process asset manager,
selected INI roots, audio definitions and `NativeRulesProcessOwner`. Loading
leases the asset manager and returns it on success, failure or cancellation.
[The loading domain](../src/app/loading/mod.rs) prepares a scenario and installs
it into the match runtime; its [pump](../src/app/loading/pump.rs) owns progress
and continuation handling.

[Simulation](../src/sim/world/mod.rs) owns live gameplay state.
[SimRuntime](../src/sim/runtime.rs) binds that simulation to immutable match
resources. Production callers advance through `SimRuntime::advance_frame()`;
[master_frame.rs](../src/sim/world/master_frame.rs) owns the authoritative frame
transaction, phase order and suspended-frame continuations. `advance_tick()` in
that file is a fixture adapter, not a second production frame entry point.

The application submits commands and consumes frame outputs through
[sim_tick.rs](../src/app/match_runtime/sim_tick.rs). Input admission, synchronized
commands, gameplay updates and output collection must retain their existing
ordering. Network or outcome waits may suspend a frame; resume its continuation
rather than repeating already executed phases.

[MatchSession](../src/app/match_runtime/session.rs) owns front-end session state
and accepted participants. Its game-mode authority transfers to the installed
world's `ScenarioSession`; consumers use the existing accessors instead of
keeping competing mode fields. [LanService](../src/app/lan_service.rs) owns LAN
physical service, while [net](../src/net/mod.rs) owns transport, reliability and
command exchange. Network traffic carries commands; simulation behavior remains
with its gameplay owners.

[Presentation](../src/app/presentation/mod.rs) derives scene instances and UI
output from the current world. [render](../src/render/mod.rs) owns GPU resources
and drawing; [ui](../src/ui/mod.rs) owns render-independent models and controls;
[audio](../src/audio/mod.rs) owns playback. `sim/` depends on none of these domains
or on `net/`. Presentation and transport must not introduce alternate gameplay
state, RNG decisions or simulation clocks.

## Configuration and retail sources

[GameConfig](../src/util/config.rs) loads `config.toml` from the working directory
first, then beside the executable. Relative configured paths resolve beside that
config file. `[paths] ra2_dir` selects retail assets; writable settings have a
separate root. Start with [config.toml.example](../config.toml.example).

[profile_file.rs](../src/app/persistence/profile_file.rs) imports legacy options
and hotkeys once into `VERA.INI`; that file then owns writable preferences.
[rules_profile.rs](../src/app/persistence/rules_profile.rs) creates the inactive
`VERARULES.INI` starter. [Personal rules](personal-rules.md) describes precedence,
restart requirements and network agreement.

[INI root configuration](../src/util/ini_roots.rs) chooses YR, MO, automatic or
custom root names. [retail_sources.rs](../src/rules/retail_sources.rs) selects
physical bytes; [NativeRulesProcessOwner](../src/rules/process_owner.rs) owns
ordered registry processing. Rules, optional language rules, mode/campaign
passes and map overrides are processed through those owners. Fixed art and
standalone audio catalogs have different lifetimes; avoid a generic merge that
collapses their ordering. See [INI profiles](ini-roots.md) and
[directives](ini-directives.md) before changing readers or source selection.

## Find the owner for a task

| Task | Start here |
| --- | --- |
| Movement, combat, production or lifecycle | [sim/world](../src/sim/world/mod.rs), then the affected mechanism and its callers. |
| Unit/weapon keys, defaults or rule layers | [rules](../src/rules/mod.rs), [process_owner.rs](../src/rules/process_owner.rs). |
| MIX/SHP/VXL decoding or resource precedence | [assets](../src/assets/mod.rs), [asset manager](../src/assets/asset_manager.rs). |
| Scenario setup, transitions or loading failures | [loading](../src/app/loading/mod.rs), [match startup](../src/app/match_runtime/startup.rs). |
| Input or a menu/sidebar control | [input](../src/app/input/mod.rs), [frontend](../src/app/frontend/mod.rs), [ui](../src/ui/mod.rs). |
| Draw order, palettes or GPU output | [presentation](../src/app/presentation/mod.rs), [render](../src/render/mod.rs). |
| Saves, settings or restoration | [persistence](../src/app/persistence/mod.rs), [simulation snapshots](../src/sim/snapshot.rs). |
| LAN, lobby or command synchronization | [LanService](../src/app/lan_service.rs), [shell_wol](../src/app/shell_wol.rs), [net](../src/net/mod.rs). |
| Repository tooling or a reproducible probe | [tools index](../tools/README.md), [asset browser](../tools/asset_browser/README.md). |

## Validation and collaboration

Choose the check for the changed owner; these are examples, not a required suite:

```sh
python -m tools.cargo_run -- test -p vera20k-core --lib rules::personal_rules::
python -m tools.cargo_run -- test -p vera20k --lib app::frontend::cncnet_spawn::tests::
python tools/skill_sync.py --check
```

Read [AGENTS.md](../AGENTS.md) for retail-input requirements, allowed native
comparisons, focused formatting and the checks each change requires. Run a full
build only when explicitly requested. [Developer tools](../tools/README.md)
explains executable resolution, builds and captures.

Before collaborating, verify the checkout and assign nonoverlapping files or
owners. Re-read a shared file before editing it; another agent's working tree and
staging are live state. Keep source stable during a build because the runner
validates its input identity. Reviews report defects and evidence; they do not
grant implementation, commit or publication authority. Preserve scope and the
current handoff instead of starting an unrelated cleanup.
