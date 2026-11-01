<p align="center" dir="ltr">
  <a href="https://github.com/ConchFeng/RedAlert2/actions/workflows/macos.yml?query=branch%3Amain" title="Latest macOS library test run (run manually)"><img src="https://github.com/ConchFeng/RedAlert2/actions/workflows/macos.yml/badge.svg?branch=main" alt="macOS library tests" height="23" align="middle"></a>
  <a href="https://github.com/ConchFeng/RedAlert2/actions/workflows/linux.yml?query=branch%3Amain" title="Latest Linux library test run (run manually)"><img src="https://github.com/ConchFeng/RedAlert2/actions/workflows/linux.yml/badge.svg?branch=main" alt="Linux library tests" height="23" align="middle"></a>
  <a href="https://github.com/ConchFeng/RedAlert2/actions/workflows/windows.yml?query=branch%3Amain" title="Latest Windows library test run (run manually)"><img src="https://github.com/ConchFeng/RedAlert2/actions/workflows/windows.yml/badge.svg?branch=main" alt="Windows library tests" height="23" align="middle"></a>
  <a href="https://discord.gg/kmjRUn5m5F"><img src="https://img.shields.io/badge/Discord-Join-5865F2?style=flat&amp;logo=discord&amp;logoColor=white" alt="Join Discord" height="23" align="middle"></a>
  <img src="https://img.shields.io/badge/QQ_group-1097193563-1EBAFC?style=flat&amp;logo=qq&amp;logoColor=white" alt="QQ group" height="23" align="middle">
</p>

# VERA20k Conch Edit

**Red Alert 2: Yuri's Revenge rebuilt in Rust, with local skirmishes for up to 30 players.**

ConchFeng's edition of [VERA20K](https://github.com/YuriPlanet/vera20k) expands the
original eight-player setup to **30 local player slots**: fight **29 AI opponents**
yourself, or watch **29 AIs battle from a full-map Observer view**. **Huge and Giant
random maps** provide room for larger battles while keeping the original game's
assets and atmosphere. The current downloadable packages target **macOS on Apple Silicon**.

[Download 1.0.1](#download-and-play) · [30-player battles and AI spectating](#30-player-battles-and-ai-spectating) ·
[30-player walkthrough](docs/30-player-showcase.md) ·
[Architecture](#architecture) · [Developer documentation](docs/index.md)

<img src="docs/images/30-player-random-map-settings.jpg" alt="Current macOS random-map screen with Giant selected and Players set to 30" width="100%">

*Giant is the largest random-map option. These are actual application captures
using the supplied Chinese game localization.*

## 30-player battles and AI spectating

**Play against 29 AIs, or let the AIs fight while you watch.** Local Skirmish
supports 30 setup slots with two ways to enjoy a large battle:

| Mode | Setup | Your role |
| --- | --- | --- |
| Play the battle | 1 human player + up to 29 AI opponents | Build your base, command your army, and fight the computers. |
| Watch the AIs | 1 Observer + up to 29 AI players | Follow the AI battle with full-map visibility and no army of your own. |

A fresh offline Skirmish defaults to **Observer + 29 Normal AIs in Free For All**.
The AIs develop their bases, gather resources, and fight each other while you
watch. Your explicit role and settings choices are remembered; choose Player
in Skirmish setup when you want to join the fighting. The Observer does not occupy
a map starting position.

<img src="docs/images/ai-battle-spectator.jpg" alt="Live AI spectator view showing an AI base, infantry, vehicles, a multicolor minimap, and the WATCHING THE REMAINING AIS status message" width="100%">

*Actual gameplay capture: an AI base and its units during an ongoing spectator
session. The top-left message, “WATCHING THE REMAINING AIS — ESC: OPTIONS,”
identifies the spectator view; the minimap shows the different player colors.*

To set up a large match, open **Single Player → Skirmish**, choose Observer or
Player, and check the AI rows with **Previous** and **Next**. Use a map with enough
starting positions. If the selected map is too small, the game opens the
**30-player Giant random-map setup**: **Generate Map → Use Map → Start Game**.
AI difficulty remains adjustable.

If you play and are eliminated, the offline match continues while
hostile AI survivors remain, so you can watch the remaining computers fight.
Press **Esc** for options. At the end, the results screen stays open:
**View Battlefield** shows the paused final battlefield, **Esc** returns to the
results, **Play Again** starts the same setup with a new seed, and **Return to
Menu** leaves the match.

## What's new in Conch Edit

- **30-player local skirmish setup:** enable up to 29 computer opponents and
  page through their settings with **Previous** and **Next**.
- **Larger random maps:** **Huge** and **Giant** expand beyond the retail sizes;
  dimensions also grow with the selected player count, within the generator's cap.
- **More player colors:** 22 additional colors become available on maps with
  more than eight player slots.
- **Saved extended setups:** opponent settings past the original seven are
  retained in `VERA.INI`.
- **Startup settings and recovery:** configure window dimensions, including
  1920 × 1080 physical pixels, and use **Choose Folder** when game resources
  cannot load.

## 30-player showcase

The captured flow uses **Giant**, **30 players**, and all **29 computer opponents**
enabled. Selecting 30 in the generator creates the map capacity; enable the
opponents separately before starting the match.

| Generate the largest map | Configure the opponents |
| --- | --- |
| ![Giant random-map generation in progress with 30 players](docs/images/30-player-random-map-generating.jpg) | ![The generated random map selected with the first seven computer opponents enabled](docs/images/30-player-skirmish-setup.jpg) |
| ![Finished Giant random-map preview with the player count still set to 30](docs/images/30-player-random-map-preview.jpg) | ![The final opponent page reaches player 30 of 30](docs/images/30-player-opponents-final.jpg) |

See the [full screenshot walkthrough](docs/30-player-showcase.md) for the generated
preview, additional opponent pages, extended colors, and the current runtime limit.

## Project goals

1. Keep the gameplay, visuals and atmosphere of the original Red Alert 2: Yuri's Revenge.
2. Support bigger battles: up to **30 players** and **20,000 units** on 500×500 maps.
3. Incorporate known RTS features and some never seen before.
4. Integrated multiplayer client

## Current status

**In active development.**

- **Skirmish:** Playable on Windows, Linux and macOS, with original and random maps,
  base building, harvesting, combat and save/load.
- **Campaigns:** Missions can launch; support remains incomplete.
- **AI:** Parts of the original AI handle base building and scripted attacks.
- **Ongoing work:** Gameplay accuracy, visuals and performance.
- **Multiplayer:** LAN and online play are available in current source builds on
  macOS and Linux.
- **Large-map limit:** The captured Giant/30-player map generates and reaches
  Skirmish setup. Its earlier pathfinding zone-builder stack overflow is fixed,
  but a live match of that setup has not been captured; generation screenshots
  do not establish a completed match.
- **Scale:** 20,000 units remains a project target. Long-game stability and
  smooth performance at maximum map size and player count require further validation.

## Download and play

The [1.0.1 release](https://github.com/ConchFeng/RedAlert2/releases/tag/v1.0.1)
provides two ZIP downloads for **Apple Silicon Macs (M1 or newer), macOS 11.0 or later**:

| Package | Size | Contents |
| --- | --- | --- |
| [macOS Full ZIP](https://github.com/ConchFeng/RedAlert2/releases/download/v1.0.1/VERA20k-Conch-Edit-1.0.1-macos-arm64-Full.zip) | 418.60 MB | App, supplied Yuri resource files, maps, audio, and bilingual instructions. |
| [macOS app ZIP](https://github.com/ConchFeng/RedAlert2/releases/download/v1.0.1/VERA20k-Conch-Edit-1.0.1-macos-arm64.zip) | 19.03 MB | App and instructions; provide your own Yuri's Revenge resources. |

1. Extract the entire ZIP and open `VERA20K.app` from your preferred folder.
2. Choose the game-resource folder when prompted. For the Full package, select
   its `assets/` folder or the parent folder containing it.
3. Open **Single Player → Skirmish → Choose Map → Create Random Map**, select
   **Giant** and **30 players**, generate the map, and choose **Use Map**.
4. Choose **Observer** to watch the AIs, or **Player** to fight them. Check up to
   **29 AI opponents** across the setup pages, then choose **Start Game**. See
   [30-player battles and AI spectating](#30-player-battles-and-ai-spectating)
   and the [map walkthrough](docs/30-player-showcase.md) for details.

The ZIPs contain native ARM64 macOS executables; Intel Mac, Windows, and Linux
binaries are not included. The source remains cross-platform. The release/app
bundle version is `1.0.1`; the internal engine version is `0.2.4`.

The app is signed ad hoc and is not Apple-notarized. If macOS blocks its first
launch, review the source and use **System Settings → Privacy & Security →
Open Anyway**. Allow access to your selected game-resource folder when prompted.
Configuration, saves, logs, and replays normally live in
`~/Library/Application Support/VERA20k/`; existing custom paths remain in effect.
Checksums and bilingual instructions are included on the release page.

Use **Options** for sound and controls, and **Vera Options** for VERA-specific features.

To launch single-player YR skirmishes or campaigns from a CnCNet client, see the
[offline client setup](integrations/cncnet/README.md).

## Contributing

Help is welcome. You can write code, refactor the engine, test the game, share ideas, or play
it next to the original and tell us what feels wrong. Open a PR and we'll take it from there.

Contributors can take responsibility for engine areas, with broad freedom to make changes that
move VERA20k Conch Edit forward.

Maintainers will help with review, integration and merging. You'll keep full credit for your
contribution.

Discussion is on [Discord](https://discord.gg/kmjRUn5m5F) and in the QQ group (1097193563).

## Architecture

The workspace has four crates. The windowed client uses a GPU-free core;
the standalone relay and its shared protocol remain separate crates.

```mermaid
flowchart TB
    files["Game resources: MIX archives, INIs, maps"]

    subgraph client["vera20k — windowed client"]
        ui["Menus, input, window lifecycle"]
        app["App / MatchSession: loading and frame orchestration"]
        presentation["Presentation: scene and UI projection"]
        render["wgpu renderer"]
        audio["Audio playback"]
        ui --> app
        app --> presentation
        presentation --> render
        app --> audio
    end

    subgraph core["vera20k-core — GPU-free engine"]
        resources["Assets, native INI readers, rules, maps"]
        simulation["SimRuntime / SimWorld: authoritative gameplay state"]
        net["net: transport and command exchange"]
        resources --> simulation
    end

    relay["vera20k-relay: standalone UDP service"]
    protocol["vera20k-relay-protocol: authenticated datagrams"]
    files --> resources
    resources --> app
    app -->|"commands / advance_frame"| simulation
    simulation -->|"frame outputs"| app
    app <-->|"session commands"| net
    net <-->|"UDP"| relay
    net -.->|"uses"| protocol
    relay -.->|"uses"| protocol
```

Solid arrows show runtime data flow; dashed arrows show use of the shared
protocol crate. Simulation owns gameplay, RNG, and frame updates. It does not
depend on rendering, UI, audio, or networking. Presentation derives its output
from simulation state. See the [project guide](docs/project-guide.md) for source
owners and entry points.

## Build and run

An opt-in [WebAssembly single-player preview](docs/wasm.md) is also available.
It uses a separate build command; desktop builds keep their existing defaults.

You need:

- **Rust and Git:** [Rust](https://rustup.rs/) 1.97 or later, installed with rustup, and
  [Git](https://git-scm.com/).
- **Build tools:** on Windows, the Visual Studio C++ build tools, which the Rust installer offers
  to install; on macOS, `xcode-select --install`; on Debian and Ubuntu,
  `sudo apt install build-essential libasound2-dev libxkbcommon-x11-0 pkg-config`.

VERA20k Conch Edit has been played on Windows, Linux and macOS.

```sh
git clone https://github.com/ConchFeng/RedAlert2.git
cd RedAlert2
cp config.toml.example config.toml
# Edit config.toml and set ra2_dir to your game folder before running:
cargo run --release --bin vera20k
```

### Large maps

Desktop builds support map `Size` width + height up to 1024 without a feature flag.
A map whose sum is at most 512 keeps retail 512-stride cell addressing; larger
maps automatically select 1024-stride addressing. This is a bounded VERA extension.
Generate a literal 500×500 map with 30 starts using
`python -m tools.scale_map large500.map --size 500 --starts 30`.
A map whose `[Basic] MaxPlayers` names 9–30 authored starts seats that many players
in Skirmish: **Previous** and **Next** page the computer opponents seven at a time,
and players past the eighth choose from 22 extra colors.
Retail overlay packs keep their 512×512 layout; extended packs use 1024×1024.
Both map sizes use the same executable and versioned save machinery. Stored-layout
changes are checked by the existing snapshot version guard; this change does not
provide a migration for saves from the earlier 16-bit navigation layout.
Basic loading, movement and MCV deployment are checked separately from 30-player,
long-game and large-unit-count performance, which remain unvalidated.

## How we work

Most of the code is written by AI coding agents. They use Ghidra to study the original
engine, then port its behavior to Rust and check it with [comparison tools](tools/native_oracle.md)
and playtesting. Agents follow [AGENTS.md](AGENTS.md).

## Customization

VERA20k Conch Edit plays like the original game by default. Map capacity is selected automatically as
described above. Other new features, including extensions from mods such as Ares, stay off
until they are turned on with one of:

- **A VERA player option**, in **Options → Vera Options**, saved under `[VERA]` in `VERA.INI`.
- **A build option**, such as `--features ares` for the supported [Ares extensions](docs/ares.md).
- **An INI key**, set by a mod or a map.

Sound, graphics and controls are saved in `VERA.INI`. Gameplay rules come from lobby
settings, mods or maps.

## Upstream project support

These links support the upstream VERA20K project.

<a href="https://paypal.me/vera20k"><img src="https://img.shields.io/badge/Donate_via_PayPal-FFC439?style=flat&amp;logo=paypal&amp;logoColor=003087" alt="Support VERA20K via PayPal" height="23"></a>

All donations received are distributed
to contributors according to the [TIBTRE contribution ledger](https://github.com/YuriPlanet/tibtre-ledger).

Payouts to contributors:

<p><img src="https://img.shields.io/badge/PayPal-003087?style=flat&amp;logo=paypal&amp;logoColor=white" alt="PayPal" height="23"> <img src="https://img.shields.io/badge/China_UnionPay-E21836?style=flat&amp;logo=data:image/svg%2Bxml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9Ii02IC02IDI2OCAxNzIiPjxwYXRoIGQ9Ik01MC4wNDE2MzEsMC4wMDA2Njc1NjMzMDUgTDExMy44MjAyMTEsMC4wMDA2Njc1NjMzMDUgQzEyMi43MjMxMjcsMC4wMDA2Njc1NjMzMDUgMTI4LjI2MDQzOCw3LjI1NzMxMzQ1IDEyNi4xODM1NTgsMTYuMTg5NzY1NyBMOTYuNDkwMDcxNSwxNDMuNjc1NDE2IEM5NC4zOTQ1MzY1LDE1Mi41NzY3NzcgODUuNDgwNzM5LDE1OS44Mzk2NDEgNzYuNTcxNjA1LDE1OS44Mzk2NDEgTDEyLjc5OTI0MzMsMTU5LjgzOTY0MSBDMy45MDg3NjQsMTU5LjgzOTY0MSAtMS42NDA5ODQ0NCwxNTIuNTc2Nzc3IDAuNDM1ODk2MjA4LDE0My42NzU0MTYgTDMwLjE0MTgxODYsMTYuMTg5NzY1NyBDMzIuMjE4Njk5Myw3LjI1NzMxMzQ1IDQxLjEyNzgzMzMsMC4wMDA2Njc1NjMzMDUgNTAuMDQxNjMxLDAuMDAwNjY3NTYzMzA1IiBmaWxsPSIjRTIxODM2IiBzdHJva2U9IiNmZmYiIHN0cm9rZS13aWR0aD0iOCIgc3Ryb2tlLWxpbmVqb2luPSJyb3VuZCIvPjxwYXRoIGQ9Ik0xMDguNTEzMjgzLDAuMDAwNjY3NTYzMzA1IEwxODEuODU3MDE4LDAuMDAwNjY3NTYzMzA1IEMxOTAuNzU4Mzc4LDAuMDAwNjY3NTYzMzA1IDE4Ni43NDQ1MjcsNy4yNTczMTM0NSAxODQuNjUwNTQ2LDE2LjE4OTc2NTcgTDE1NC45NjE3MjQsMTQzLjY3NTQxNiBDMTUyLjg3ODYyNSwxNTIuNTc2Nzc3IDE1My41Mjg0MjcsMTU5LjgzOTY0MSAxNDQuNjA4NDExLDE1OS44Mzk2NDEgTDcxLjI2NDY3NzUsMTU5LjgzOTY0MSBDNjIuMzQ0NjYxNSwxNTkuODM5NjQxIDU2LjgyNDQ0OTUsMTUyLjU3Njc3NyA1OC45MTk5ODUsMTQzLjY3NTQxNiBMODguNjA3MjUyNSwxNi4xODk3NjU3IEM5MC43MTUyMjQsNy4yNTczMTM0NSA5OS42MDU3MDM1LDAuMDAwNjY3NTYzMzA1IDEwOC41MTMyODMsMC4wMDA2Njc1NjMzMDUiIGZpbGw9IiMwMDQ0N0MiIHN0cm9rZT0iI2ZmZiIgc3Ryb2tlLXdpZHRoPSI4IiBzdHJva2UtbGluZWpvaW49InJvdW5kIi8%2BPHBhdGggZD0iTTE3OC45NDc4MywwLjAwMDY2NzU2MzMwNSBMMjQyLjcyNjQwOSwwLjAwMDY2NzU2MzMwNSBDMjUxLjY0MTc2MiwwLjAwMDY2NzU2MzMwNSAyNTcuMTc5MDc0LDcuMjU3MzEzNDUgMjU1LjA4NTA5MywxNi4xODk3NjU3IEwyMjUuMzk2MjcxLDE0My42NzU0MTYgQzIyMy4zMDA3MzYsMTUyLjU3Njc3NyAyMTQuMzgwNzE5LDE1OS44Mzk2NDEgMjA1LjQ2NjkyMiwxNTkuODM5NjQxIEwxNDEuNzE3ODc5LDE1OS44Mzk2NDEgQzEzMi43OTc4NjMsMTU5LjgzOTY0MSAxMjcuMjY1MjE1LDE1Mi41NzY3NzcgMTI5LjM1NDUzMiwxNDMuNjc1NDE2IEwxNTkuMDQ4MDE3LDE2LjE4OTc2NTcgQzE2MS4xMjQ4OTksNy4yNTczMTM0NSAxNzAuMDI3ODE0LDAuMDAwNjY3NTYzMzA1IDE3OC45NDc4MywwLjAwMDY2NzU2MzMwNSIgZmlsbD0iIzAwN0I4NCIgc3Ryb2tlPSIjZmZmIiBzdHJva2Utd2lkdGg9IjgiIHN0cm9rZS1saW5lam9pbj0icm91bmQiLz48L3N2Zz4%3D" alt="China UnionPay" height="23"> <img src="https://img.shields.io/badge/Alipay-1677FF?style=flat&amp;logo=alipay&amp;logoColor=white" alt="Alipay" height="23"> <img src="https://img.shields.io/badge/Vipps-FF5B24?style=flat&amp;logo=data:image/svg%2Bxml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjQzLjUgMjEuNSAyNyAyNyI%2BPHBhdGggZD0iTTU3LjMsNDAuNmMzLjcsMCw1LjgtMS44LDcuOC00LjRjMS4xLTEuNCwyLjUtMS43LDMuNS0wLjlzMS4xLDIuMywwLDMuN2MtMi45LDMuOC02LjYsNi4xLTExLjMsNi4xIGMtNS4xLDAtOS42LTIuOC0xMi43LTcuN2MtMC45LTEuMy0wLjctMi43LDAuMy0zLjRzMi41LTAuNCwzLjQsMUM1MC41LDM4LjMsNTMuNSw0MC42LDU3LjMsNDAuNnogTTY0LjIsMjguM2MwLDEuOC0xLjQsMy0zLDMgcy0zLTEuMi0zLTNzMS40LTMsMy0zQzYyLjgsMjUuMyw2NC4yLDI2LjYsNjQuMiwyOC4zeiIgZmlsbD0iI2ZmZiIvPjwvc3ZnPg%3D%3D" alt="Vipps" height="23"> <img src="https://img.shields.io/badge/SEPA-2D4B9A?style=flat&amp;logo=sepa&amp;logoColor=white" alt="SEPA" height="23"> <img src="https://img.shields.io/badge/Bitcoin-F7931A?style=flat&amp;logo=bitcoin&amp;logoColor=white" alt="Bitcoin" height="23"></p>

## Credits and legal

Thanks to OpenRA, XCC Mixer, the ModEnc wiki, Project Perfect Mod, EA's GPL source release of
Command & Conquer and Red Alert, World-Altering Editor, Final Alert, YRpp, Ares, Phobos and
many others.

Copyright © 2026 Halvor Nikolai Rusten and VERA20K contributors.
Licensed under the [GPLv3](LICENSE-GPL). This repository contains no game files. Command &
Conquer and Red Alert are trademarks of Electronic Arts Inc., and the screenshots and the app
icon, Red Alert 2's red star, show game art owned by Electronic Arts. VERA20k Conch Edit is not
affiliated with or endorsed by Electronic Arts.

## Documentation

- [Build and run](#build-and-run): requirements and building from source.
- [30-player screenshot walkthrough](docs/30-player-showcase.md): Giant map generation, opponent pages, and runtime status.
- [Large skirmish and random maps](docs/large-skirmish.md): implementation owners, sizing rules, and limitations.
- [INI profiles](docs/ini-roots.md): choosing rules, art and AI files.
- [Ares](docs/ares.md), [Phobos](docs/phobos.md), [NPatch](docs/npatch.md) and [VeraX](docs/verax.md): extension support and limitations.
- [Documentation and agent index](docs/index.md): starting points, owner guides and collaboration links.
- [Project guide](docs/project-guide.md): code layout, production flow and authoritative state owners.
- [Developer tools](tools/README.md): builds, tests and asset inspection.
- [Map testing](tools/map_observation.md): scripted scenarios, screenshots and match observations.
- [Native comparisons](tools/native_oracle.md): comparing behavior with the original game.
- [Ghidra workflow](docs/ghidra-workflow.md): inspecting the original engine.
