# CnCNet single-player launch adapter

Let a dedicated **YR CnCNet client copy** launch VERA20k builds that support `-SPAWN`. The client selects maps and settings; VERA runs the game. This directory contains no client programs, retail assets or game executables.

Supported: YR Battle skirmishes with one local player and up to seven computer players; all seven Allied and seven Soviet YR campaign missions at Easy, Normal or Hard difficulty; and ordinary custom maps using VERA's existing campaign mechanisms. Online play, LAN, spectators, base RA2 campaigns and original saves or replays are outside this adapter's scope. This installer and its supplied profile hide the unsupported entry points. The broader `-SPAWN` adapter separately supports network launches and network spectators; those paths are outside this single-player setup.

Default builds still restrict skirmish country indexes to the original 0–9. With `--features ares`, `[Settings] Side` and `[HouseCountries]` can carry additional countries' actual registered indexes. These are neither side indexes nor menu positions, and random countries are not selected again. The launch flow validates them against the final loaded Rules country table and rejects missing indexes. Campaign `Side=0/1` retains its Allied/Soviet meaning.

The configuration script targets the standard YR client. It neither installs mods nor adds their country options. Admitting an extended country does not establish support for that mod's loading screens, sidebar, EVA, campaigns or all Ares/Phobos features. VERA does not load their DLLs; assets and gameplay mechanisms still require separate VERA support.

## Setup

1. Prepare a **standalone, working copy of the official YR CnCNet client**, such as `E:\Games\VERA-CnCNet`. Use a dedicated copy and close its programs before setup.
2. Prepare a `vera20k.exe` build containing this adapter and your own retail Red Alert 2/Yuri's Revenge asset directory.
3. From the repository root in PowerShell, run the following command with `-WhatIf` to preview the changes. Check the paths, then remove `-WhatIf` and run it again:

```powershell
& .\integrations\cncnet\Configure-VERA20k.ps1 `
  -ClientDirectory 'E:\Games\VERA-CnCNet' `
  -VeraExecutable 'E:\Builds\vera20k.exe' `
  -RetailDirectory 'D:\steam\steamapps\common\Command & Conquer Red Alert II' `
  -WhatIf
```

4. Open `CnCNetYRLauncher.exe` from that copy and choose **Skirmish** or **Campaign**. Use **Battle** for skirmishes and keep the local player out of spectator mode. For campaigns, select a mission and set its difficulty with the slider.

The script supports Windows PowerShell 5.1 or newer. It requires no Python or runtime installation and makes no registry changes. It copies the selected VERA executable into the client root and writes a `config.toml` pointing to the retail directory, leaving retail assets in place. Older VERA builds without this adapter reject `-SPAWN`.

The client launches `vera20k.exe -SPAWN`, clears Syringe/Ares/Spawner/Phobos injection arguments, and disables QRes and single-core affinity. `ModMode=true` disables official updates for this copy so they cannot overwrite the adapter. Generated `config.toml` uses `settings_dir="."`, keeping VERA's `VERA.INI` in the dedicated client copy. On first startup, VERA imports legacy `RA2MD.INI` options and `KeyboardMD.INI` hotkeys if `VERA.INI` is absent. Later CnCNet and VERA preference changes are stored separately. The legacy and retail settings files remain untouched. Only settings VERA already supports take effect; the client's original compatibility options do not add VERA features.

## Files changed

The script creates or updates these files in the selected client:

- `vera20k.exe` and `config.toml`.
- `Resources/ClientDefinitions.ini`, `Renderers.ini`, `GameOptions.ini` and INIs for the main menu and options panels. Theme menus also hide unsupported entry points.
- `INI/VERA20kMaps.ini`: Battle maps selected from the existing map directories, retaining descriptions, previews and starting positions. Maps referencing additional forced settings are excluded rather than silently losing their rules.
- `INI/VERA20kBattles.ini`: a separate catalog of the 14 official YR missions. Mission maps still come from retail MIX archives.
- `INI/Map Code/VERA20k.ini` and `GlobalCode.ini`: empty rule-patch entry points so ordinary Battle games use VERA's retail rules-loading path.

The skirmish panel retains starting credits, starting units, game speed, Short Game, MCV Redeploy, Destroyable Bridges, Build Off Allies, Multi Engineer, crates and superweapons. The generated `spawnmap.ini` preserves the selected map's own rules. Custom extension keys still require implementation in VERA.

Each existing managed file is first copied into `VERA20k-client-backup/`, with SHA-256 values recorded before and after installation. An existing `ddraw.dll` is also backed up because switching the client to native rendering can delete it. The script refuses to overwrite an existing backup and does not clean up game assets or other programs. The backup covers managed configuration, not the entire client.

## Custom campaigns

Campaign speed adjustment uses the original `-SPEEDCONTROL` launch argument. Pass it when launching VERA directly, or add it to the existing `ExtraCommandLineParams` in the client copy's `ClientDefinitions.ini`, to enable speed changes in the paused game's options. Campaigns started directly from VERA's menu retain the selected speed too. `GameSpeed=0` disables deliberate throttling, so actual speed depends on runtime cost. Without the argument, ordinary campaigns retain the original fixed speed. This feature needs no Ares build option and does not override an explicitly supplied client campaign speed.

Copy an ordinary YR campaign map into `Maps/VERA20kMissions/MyMission.map` in the client copy, then edit `INI/VERA20kBattles.ini`:

```ini
[Battles]
; Keep the existing entries and add this one.
14=MY_MISSION

[MY_MISSION]
Scenario=Maps/VERA20kMissions/MyMission.map
Description=My custom mission
Side=0
SideName=Allied
RequiredAddon=true
```

Use `Side=0` with `SideName=Allied`, or `Side=1` with `SideName=Soviet`. Save and restart the client. Add the entry to the existing `[Battles]` section instead of creating a duplicate section.

This path provides **map selection and launch**. The map's `[Basic]` section, triggers and assets must already work in VERA. Campaign packs requiring Spawner `[GameMissionConfig]`/`ReadMissionSection`, Ares/Phobos or additional custom art/sound installation are unsupported. Progression between custom missions follows VERA's own campaign rules; this tool does not guarantee compatibility with arbitrary third-party campaign packs.

## Restore

Close the client and VERA, then run:

```powershell
& .\integrations\cncnet\Configure-VERA20k.ps1 -Action Restore -ClientDirectory 'E:\Games\VERA-CnCNet'
```

Restore first checks every managed file. If a file changed after installation, such as a campaign catalog you edited, the script stops and preserves those edits. Save your changes and manually retrieve the corresponding original from the backup. Successful automatic restoration retains the backup and restoration receipt, plus user settings, logs and saves produced by the game. Move the entire backup directory elsewhere before reinstalling so the previous records remain intact.

## Validation limits

Recorded validation includes script syntax checks and isolated fixtures using official configuration files to check installation, configuration fields and original backups. The initial tool also passed byte-for-byte restoration checks. The final installation fixture used release yr-9.3.5 configuration and covered Chinese paths, paths containing spaces, 307 Battle maps, 14 campaign missions, the settings directory and installed-file SHA-256 values. Restoration after the client deletes `ddraw.dll` was not exercised. These checks cover the installer and generated configuration; **actual client launches, matches and campaign completion still require runtime testing**.

Client versions can change controls and configuration semantics. The adapter uses the following source versions and does not guarantee compatibility with every third-party theme or modified client:

- [XNA CnCNet Client 65732ba3](https://github.com/CnCNet/xna-cncnet-client/tree/65732ba38b37c2fdd8639f9abe98f72048007b5e): [direct process launch](https://github.com/CnCNet/xna-cncnet-client/blob/65732ba38b37c2fdd8639f9abe98f72048007b5e/ClientGUI/GameProcessLogic.cs), [campaign arguments and difficulty](https://github.com/CnCNet/xna-cncnet-client/blob/65732ba38b37c2fdd8639f9abe98f72048007b5e/DXMainClient/DXGUI/Campaign/CampaignSelector.cs), and [rendering compatibility defaults](https://github.com/CnCNet/xna-cncnet-client/blob/65732ba38b37c2fdd8639f9abe98f72048007b5e/DXMainClient/Domain/DirectDrawWrapper.cs).
- [Official client yr-9.3.5](https://github.com/CnCNet/cncnet-yr-client-package/releases/tag/yr-9.3.5): release configuration was checked against the develop version below. Additional replay fields in ClientDefinitions do not affect this adapter.
- [Official YR client package 01fecbdd](https://github.com/CnCNet/cncnet-yr-client-package/tree/01fecbdd1c62cbce49a9aaea1aafefd0a97571ae): client layout, country/color IDs, map catalogs and mission identifiers.

VERA does not produce match results in CnCNet's original format. Configuration points result reading at a dedicated absent log path so old logs cannot be mistaken for VERA results; no fabricated statistics file is generated. The client's exit callback permits the missing log, but a skirmish lasting over a minute may still leave incomplete local history. That history does not establish victory, defeat or campaign progress.

The client and its configuration system come from Rampastring, The Dawn of the Tiberium Age and CnCNet. This directory supplies a VERA20k adapter without redistributing their programs or changing their licenses.
