//! Shared .SED reader and retail-backed random-map construction.

use std::path::Path;

use anyhow::Result;

use crate::assets::asset_manager::AssetManager;
use crate::map::source::LoadedMapSource;
use crate::rules::process_owner::NativeRulesProcessOwner;

pub struct LoadedGeneratedMap {
    pub generated: super::GeneratedMap,
    pub source: LoadedMapSource,
}

/// ReadScenario684620 dispatches .SED generation before the scenario Full_Init.
/// Retain the consumed bytes with the identity; replay must never reread a changed seed file.
pub fn load(
    retail_dir: &Path,
    seed_name: &str,
    frozen_seed_bytes: Option<&[u8]>,
    assets: &mut AssetManager,
    rules_owner: Option<&NativeRulesProcessOwner>,
) -> Result<LoadedGeneratedMap> {
    anyhow::ensure!(
        super::is_seed_selection(seed_name),
        "random-map input requires a .SED selection"
    );
    let mut options = crate::map::rmg::RmgOptions::default();
    let seed_path = retail_dir.join(seed_name);
    let mut seed_source = None;
    let seed_bytes = match frozen_seed_bytes {
        Some(bytes) => Ok(std::borrow::Cow::Borrowed(bytes)),
        None => crate::util::file_system::read(&seed_path).map(std::borrow::Cow::Owned),
    };
    match seed_bytes {
        Ok(bytes) => {
            seed_source = Some(
                crate::map::source::GeneratedSeedSource::from_consumed_bytes(seed_path, &bytes),
            );
            match crate::rules::ini_parser::IniFile::from_bytes(&bytes) {
                Ok(ini) => options.apply_sed(&ini),
                Err(err) => {
                    log::warn!("random map: {seed_name} is not valid INI ({err}); using defaults")
                }
            }
        }
        // Missing seed file is not fatal: the original's options object
        // keeps its constructor defaults when a key (or the file) is absent.
        Err(err) => log::warn!("random map: cannot read {seed_name} ({err}); using defaults"),
    }
    options.normalize();

    let settings = crate::map::rmg::RmgSettings::load(assets);
    let theater_name = crate::map::rmg::emit::theater_name(options.theater);
    let theater = crate::map::theater::load_theater(assets, theater_name)
        .ok_or_else(|| anyhow::anyhow!("random map: theater {theater_name} unavailable"))?;

    // Terrain rules feed the zone classifier's wheel-impassable table. The
    // table is observably inert during generation (the classifier runs at
    // start-placement time, before any rock/rough/cliff terrain exists), but
    // it is resolved faithfully from `rulesmd.ini` when present; a missing
    // file falls back to the passable defaults.
    let terrain_rules = rules_owner
        .map(|owner| {
            crate::rules::terrain_rules::TerrainRules::from_ini(owner.selected_rules_root())
        })
        .unwrap_or_default();
    let resolved =
        crate::map::rmg::build::ResolvedTheaterInputs::from_theater(&theater, &terrain_rules);

    // Tile-block layouts (sub-cell height/terrain grids) from the theater's
    // real TMP data — the shore tiler and zone classifier read these.
    let blocks =
        crate::map::rmg::theater_blocks::TheaterTileBlocks::build(&theater.lookup, |name| {
            assets.get(name)
        });

    // `[AI] NeutralTechBuildings` plus each type's `Foundation=`. The phase
    // runs for every map type except 0, so an empty list here would both
    // strip the buildings and skip the draws the original consumes.
    let tech_types = crate::map::rmg::tech_catalog::load(rules_owner);
    let generated =
        crate::map::rmg::build::generate_map(&options, &settings, &resolved, &blocks, &tech_types);
    log::info!(
        "Random map generated: theater={}, {}x{}, seed={}, players={}",
        generated.map_file.header.theater,
        generated.map_file.header.width,
        generated.map_file.header.height,
        options.seed,
        options.num_players
    );
    if generated.unfilled_start_slots > 0 {
        log::warn!(
            "Random map is short of spawns: {} start slot(s) could not be \
                 filled (seed={}, players={}); those players have no start position",
            generated.unfilled_start_slots,
            options.seed,
            options.num_players
        );
    }

    Ok(LoadedGeneratedMap {
        generated,
        source: LoadedMapSource::Generated {
            seed_name: seed_name.to_owned(),
            seed_source,
        },
    })
}
