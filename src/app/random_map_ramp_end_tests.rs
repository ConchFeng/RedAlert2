//! Retail Generate witness for the ramp-end transport dependency.
//!
//! Original598960's type3/4 gate reaches58EF10 ->5905D0/590970 and the
//! successful carvers. They stamp SlopeSetPieces/ABC1F8 +0..9 through5A6C10;
//! RampBase/ABC1D8 separately supplies the stepped edge. Native reader and
//! publication evidence lives in tools/rules_oracle/theater_general_reader.*.

use super::*;
use crate::map::map_file::MapCell;
use crate::map::rmg::build::{GenerationPoint, generate_map_observed};
use crate::map::rmg::phases::shore::TileBlocks;
use crate::map::rmg::{RmgOptions, Stage};
use std::collections::HashMap;

fn ordinary_recipe(seed: i32, map_type: i32) -> RmgOptions {
    RmgOptions {
        theater: 0,
        map_type,
        resources: 2,
        ruggedness: 35,
        water_amount: 50,
        num_players: 2,
        tiberium: 35,
        vegetation: 30,
        urban_presence: 40,
        seed,
        ..Default::default()
    }
}

fn retail_generation_inputs(
    options: &RmgOptions,
) -> Option<(
    crate::assets::asset_manager::AssetManager,
    PreparedRandomMapGeneration,
    crate::rules::process_owner::NativeRulesProcessOwner,
)> {
    let Some((retail_dir, _)) = crate::rules::retail_ini_fixture::retail_assets() else {
        return None;
    };
    let mut assets = crate::assets::asset_manager::AssetManager::new(
        &retail_dir,
        crate::assets::asset_manager::MediaArchiveMode::STOCK_DIGITAL,
    )
    .expect("retail Generate assets");
    let audio = crate::rules::audio_sources::AudioDefinitions::select(&assets);
    let (_, _, rules) = crate::rules::retail_sources::load_startup_rules(
        &assets,
        std::sync::Arc::clone(audio.sounds()),
        std::sync::Arc::clone(audio.eva()),
    )
    .expect("retail Generate rules")
    .into_parts();
    let prepared = prepare_random_map_generation(&mut assets, options, Some(&rules), None)
        .expect("production Generate preparation");
    Some((assets, prepared, rules))
}

/// Read-only classification of actual stage outputs; no cell writes or
/// candidate/geometry decisions are replayed here.
fn introduced_ramp_cells<'a>(
    cells: &'a [MapCell],
    before: &HashMap<(u16, u16), (i32, u8, u8)>,
    ramp_family: &std::ops::Range<i32>,
    end_family: &std::ops::Range<i32>,
    blocks: &dyn TileBlocks,
) -> (Vec<&'a MapCell>, Vec<&'a MapCell>) {
    let changed = |cell: &MapCell| {
        before.get(&(cell.rx, cell.ry)) != Some(&(cell.tile_index, cell.sub_tile, cell.z))
    };
    let stepped = cells
        .iter()
        .filter(|cell| {
            changed(cell)
                && ramp_family.contains(&cell.tile_index)
                && blocks
                    .block(cell.tile_index)
                    .and_then(|block| block.subtiles.get(usize::from(cell.sub_tile)))
                    .and_then(|sub| sub.as_ref())
                    .is_some_and(|sub| sub.slope != 0)
        })
        .collect();
    let ends = cells
        .iter()
        .filter(|cell| changed(cell) && end_family.contains(&cell.tile_index))
        .collect();
    (stepped, ends)
}

#[test]
fn generated_connector_uses_native_slope_piece_tileset() {
    // Ordinary .SED lifecycle parameters with a seed selected for actual
    // connector admission, after the dialog reroll. The precondition below
    // must remain positive; do not inject geometry to repair this witness.
    let options = ordinary_recipe(11, 3);
    let Some((mut assets, prepared, rules)) = retail_generation_inputs(&options) else {
        return;
    };
    let native = crate::test_fixture::json("tools/rules_oracle/theater_general_reader.json");
    let read = native["read_contract"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["key"] == "SlopeSetPieces")
        .unwrap();
    assert_eq!(read["call"], "0x545776");
    assert_eq!(read["default"], -1);
    assert_eq!(read["resolved_global"], "0xabc1f8");
    let physical = native["physical"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["file"] == "temperatmd.ini")
        .unwrap();
    assert_eq!(
        crate::util::sha256::sha256_hex(&prepared.theater.ini_data),
        physical["sha256"],
        "the actual theater winner must match the original reader's physical input",
    );
    let ordinal = prepared
        .theater
        .slope_set_pieces
        .expect("native SlopeSetPieces ordinal");
    assert_eq!(u64::from(ordinal), physical["values"]["SlopeSetPieces"]);
    // Original545E1D/545E26 publishes the cumulative base for this ordinal.
    // Read it from the production lookup; the ordinal itself is not a tile.
    let end_set = prepared
        .theater
        .lookup
        .bounds()
        .get(usize::from(ordinal))
        .expect("registered physical SlopeSetPieces tileset");
    let end_base = i32::from(end_set.start);
    assert!(
        end_set.count >= 10,
        "the native +0..9 end-piece family is physical"
    );
    let end_family = end_base..end_base + 10;
    for tile in end_family.clone() {
        assert!(
            prepared.blocks.block(tile).is_some(),
            "physical end block {tile}"
        );
    }
    let ramp_base = prepared.resolved_inputs.ids.ramp_base;
    assert!(ramp_base >= 0, "physical stepped RampBase is registered");
    let ramp_family = ramp_base..ramp_base + 8;
    assert!(!end_family.contains(&ramp_base));

    let mut before_islands = None;
    let mut introduced_ends = Vec::new();
    let generated = generate_map_observed(
        &options,
        &prepared.settings,
        &prepared.resolved_inputs,
        &prepared.blocks,
        &prepared.tech_types,
        &mut |view| match view.point() {
            GenerationPoint::After(Stage::Regions) => {
                before_islands = Some(
                    view.snapshot()
                        .map_file
                        .cells
                        .into_iter()
                        .map(|cell| ((cell.rx, cell.ry), (cell.tile_index, cell.sub_tile, cell.z)))
                        .collect::<HashMap<_, _>>(),
                );
            }
            GenerationPoint::After(Stage::IslandPasses) => {
                let map = view.snapshot().map_file;
                let before = before_islands
                    .as_ref()
                    .expect("ordinary Regions precedes Islands");
                // Island repair itself only clears tiles to0; the connector
                // carvers introduce these physical stepped RampBase subtiles.
                // Observe here, before Hills can independently create slopes.
                let (stepped, ends) = introduced_ramp_cells(
                    &map.cells,
                    before,
                    &ramp_family,
                    &end_family,
                    &prepared.blocks,
                );
                assert!(
                    !stepped.is_empty(),
                    "seed11/type3 has no admitted positive ramp connector at AfterIslandPasses; endpoint parity is not tested"
                );
                introduced_ends = ends.into_iter().cloned().collect();
                eprintln!(
                    "retail seed11/type3 AfterIslandPasses: stepped_cells={}, native_end_cells={}, end_base={end_base}",
                    stepped.len(),
                    introduced_ends.len()
                );
                assert!(
                    !introduced_ends.is_empty(),
                    "an admitted connector must stamp the original SlopeSetPieces base{end_base} +0..9; passing -1 is not that native base"
                );
            }
            _ => {}
        },
    );
    assert!(before_islands.is_some(), "ordinary Regions observation");
    assert!(!introduced_ends.is_empty(), "ordinary Islands observation");
    // Continue through real emit and the existing preview consumer. No map
    // cells or loader/RNG state are rewritten between Generate and preview.
    let end = generated
        .map_file
        .cells
        .iter()
        .find(|cell| {
            introduced_ends.iter().any(|painted| {
                (cell.rx, cell.ry, cell.tile_index, cell.sub_tile, cell.z)
                    == (
                        painted.rx,
                        painted.ry,
                        painted.tile_index,
                        painted.sub_tile,
                        painted.z,
                    )
            }) && crate::map::playfield::PlayfieldBounds::from_map_header(
                &generated.map_file.header,
            )
            .contains_geometry_packed(i32::from(cell.rx), i32::from(cell.ry))
        })
        .expect("a generated native ramp end survives later stages into the emitted playable map");
    let mut main_rng = crate::sim::rng::MainRng::new(0x0401_599d);
    let mut main = crate::app::state::process_main_draws(None, &mut main_rng);
    let mut selector = crate::map::tile_variant_selector::TileVariantSelectorCache::default();
    let resolved = build_random_map_preview_grid(
        &generated.map_file,
        Some(&prepared.theater),
        Some(&assets),
        Some(&prepared.terrain_rules),
        &mut main,
        &mut selector,
    );
    let live = resolved
        .cell(end.rx, end.ry)
        .expect("emitted endpoint reaches preview terrain");
    assert_eq!(
        resolved.presentation_tile(live),
        (end.tile_index as u16, end.sub_tile)
    );
    assert_eq!(live.level, end.z);
    let preview = crate::map::rmg::preview::preview_cells_from_map(
        &generated.map_file,
        &resolved,
        &|_, _, _| None,
        &|_, _, _| None,
    );
    assert!(
        preview
            .iter()
            .any(|cell| (cell.x, cell.y) == (end.rx, end.ry)),
        "the playable emitted endpoint reaches the ordinary preview cell consumer"
    );
    // Follow the same recipe through the ordinary selected-.SED loader. The
    // identity belongs to the consumed buffer even if the file later changes.
    let seed_dir = crate::map::source::test_support::TestDirectory::new("ramp-end");
    let seed_name = "CellRamp11.SED";
    let seed_bytes = options.to_sed_bytes();
    seed_dir.write(seed_name, &seed_bytes);
    struct SilentProgress;
    impl crate::app::loading::pump::LoadingProgressSink for SilentProgress {
        fn milestone(&mut self, _percent: u32) -> anyhow::Result<()> {
            Ok(())
        }
    }
    let loaded = crate::app::loading::init::load_map_initial_with_assets(
        seed_dir.path().to_path_buf(),
        &mut assets,
        Some(&rules),
        Some(seed_name),
        None,
        &mut SilentProgress,
    )
    .expect("ordinary selected-.SED load");
    seed_dir.write(seed_name, b"[FinalAlert]\r\nSeed=42\r\n");
    let crate::map::source::LoadedMapSource::Generated {
        seed_name: consumed_name,
        seed_source: Some(source),
    } = loaded.map_source()
    else {
        panic!("ordinary selected-.SED load must retain consumed seed identity");
    };
    assert_eq!(consumed_name, seed_name);
    assert_eq!(source.path, seed_dir.path().join(seed_name));
    assert_eq!(source.payload_len, seed_bytes.len());
    assert_eq!(
        source.source_sha256,
        crate::util::sha256::sha256_hex(&seed_bytes)
    );
    assert_ne!(
        source.source_sha256,
        crate::util::sha256::sha256_hex(&std::fs::read(&source.path).unwrap()),
        "source identity must not be recomputed from a subsequently changed seed file"
    );
    let emitted = loaded
        .map_data()
        .cells
        .iter()
        .find(|cell| (cell.rx, cell.ry) == (end.rx, end.ry))
        .expect("ordinary selected-.SED endpoint");
    assert_eq!(
        (emitted.tile_index, emitted.sub_tile, emitted.z),
        (end.tile_index, end.sub_tile, end.z),
        "normal selected-.SED loading preserves the generated endpoint"
    );
    // This is source/read/publication and production consumer evidence. Native
    // full-Generate pixels, dialog reroll, and skirmish start remain separate.
}

#[test]
fn generated_raised_ridge_materializes_native_cliff_faces() {
    let options = ordinary_recipe(11, 3);
    let Some((_assets, prepared, _rules)) = retail_generation_inputs(&options) else {
        return;
    };
    let native = crate::test_fixture::json("tools/rules_oracle/theater_general_reader.json");
    let physical = native["physical"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["file"] == "temperatmd.ini")
        .unwrap();
    assert_eq!(
        crate::util::sha256::sha256_hex(&prepared.theater.ini_data),
        physical["sha256"]
    );
    let ordinal = physical["values"]["CliffSet"].as_u64().unwrap() as usize;
    let registration = &prepared.theater.lookup.bounds()[ordinal];
    let base = i32::from(
        prepared
            .theater
            .cliff_ranges
            .cliff_set
            .expect("physical CliffSet"),
    );
    assert_eq!(base, i32::from(registration.start));
    assert!(
        registration.count >= 40,
        "native CliffSet40-tile band is physical"
    );
    for tile in base..base + 40 {
        assert!(
            prepared.blocks.block(tile).is_some(),
            "physical cliff block {tile}"
        );
    }
    let mut ordinary_steps = 0;
    let mut materialized = Vec::new();
    let generated = generate_map_observed(
        &options,
        &prepared.settings,
        &prepared.resolved_inputs,
        &prepared.blocks,
        &prepared.tech_types,
        &mut |view| {
            if view.point() == GenerationPoint::After(Stage::Regions) {
                let cells = view.snapshot().map_file.cells;
                let coordinates: HashMap<_, _> = cells
                    .iter()
                    .map(|cell| ((cell.rx, cell.ry), cell))
                    .collect();
                ordinary_steps = cells
                    .iter()
                    .filter(|cell| {
                        prepared.resolved_inputs.ids.is_clear(cell.tile_index)
                            && coordinates
                                .get(&(cell.rx, cell.ry + 1))
                                .is_some_and(|neighbor| {
                                    prepared.resolved_inputs.ids.is_clear(neighbor.tile_index)
                                        && cell.z == neighbor.z.saturating_add(4)
                                })
                    })
                    .count();
            }
            if view.point() == GenerationPoint::After(Stage::IslandPasses) {
                materialized = view
                    .snapshot()
                    .map_file
                    .cells
                    .into_iter()
                    .filter(|cell| (base..base + 40).contains(&cell.tile_index))
                    .collect();
            }
        },
    );
    assert!(
        ordinary_steps > 0,
        "ordinary seed11/type3 must admit a four-level raised-ground boundary before the connector/cliff stages"
    );
    assert!(
        !materialized.is_empty(),
        "native type3/4 gate598D6E/598D7B/598D82 runs cliff materialization after connectors; ordinary raised ground must acquire physical CliffSet faces"
    );
    assert!(
        generated.map_file.cells.iter().any(|cell| {
            materialized.iter().any(|face| {
                (cell.rx, cell.ry, cell.tile_index, cell.sub_tile, cell.z)
                    == (face.rx, face.ry, face.tile_index, face.sub_tile, face.z)
            })
        }),
        "a materialized cliff face must survive later phases into the emitted map"
    );
}

/// VERA's thirty-player Huge and Giant maps, generated from the retail
/// temperate tiles, place every start on each dialog map type.
#[test]
#[ignore = "slow; run when changing what it covers"]
fn thirty_player_huge_and_giant_maps_seat_every_start() {
    for size in [4, 5] {
        for map_type in [1, 2, 3, 4] {
            let options = RmgOptions {
                num_players: 30,
                width: size,
                height: size,
                ..ordinary_recipe(4242, map_type)
            };
            let Some((_assets, prepared, _rules)) = retail_generation_inputs(&options) else {
                return;
            };
            let generated = crate::map::rmg::build::generate_map(
                &options,
                &prepared.settings,
                &prepared.resolved_inputs,
                &prepared.blocks,
                &prepared.tech_types,
            );
            let cell = format!("size={size} map_type={map_type}");
            assert_eq!(generated.unfilled_start_slots, 0, "{cell}");
            assert_eq!(generated.start_waypoints.len(), 30, "{cell}");
        }
    }
}

fn generated_movement_zones(
    options: &RmgOptions,
    prepared: &PreparedRandomMapGeneration,
    map: &crate::map::map_file::MapFile,
) -> (
    crate::map::rmg::RmgGrid,
    crate::map::rmg::phases::zones::ZoneField,
) {
    use crate::map::rmg::phases::zones::{self, ZoneKind, ZoneParams};
    use crate::map::rmg::{MapGeometry, RmgGrid};
    let geometry = MapGeometry::from_options(options);
    let mut grid = RmgGrid::new(geometry.stride, geometry.diamond_min, geometry.diamond_max);
    for cell in &map.cells {
        let target = grid
            .get_mut(i32::from(cell.rx), i32::from(cell.ry))
            .unwrap();
        target.tile = cell.tile_index;
        target.sub_tile = cell.sub_tile;
        target.level = cell.z;
        target.slope = prepared
            .blocks
            .block(cell.tile_index)
            .and_then(|block| block.subtiles.get(usize::from(cell.sub_tile)))
            .and_then(|subtile| subtile.as_ref())
            .map_or(0, |subtile| subtile.slope);
    }
    let zones = zones::compute(
        &grid,
        &prepared.blocks,
        &ZoneParams {
            map_w: geometry.map_w,
            local_rect: [2, 5, geometry.gen_w, geometry.gen_h],
            wheel_impassable: prepared.resolved_inputs.wheel_impassable,
        },
        ZoneKind::Normal,
    );
    (grid, zones)
}

#[test]
#[ignore = "slow; run when changing Island War generation"]
fn island_war_separates_islands_and_balances_starts_on_retail_terrain() {
    use std::collections::BTreeMap;

    for (players, islands, size, seed) in [(30, 8, 4, 4242), (8, 30, 0, 137), (30, 2, 0, 11)] {
        let options = RmgOptions {
            num_players: players,
            island_count: islands,
            width: size,
            height: size,
            ..ordinary_recipe(seed, 5)
        };
        let Some((_assets, prepared, _rules)) = retail_generation_inputs(&options) else {
            return;
        };
        let generated = crate::map::rmg::build::generate_map(
            &options,
            &prepared.settings,
            &prepared.resolved_inputs,
            &prepared.blocks,
            &prepared.tech_types,
        );
        let (grid, zones) = generated_movement_zones(&options, &prepared, &generated.map_file);
        let mut land = BTreeMap::<_, usize>::new();
        for (x, y) in grid.native_cells() {
            if zones.is_clear_ground(x, y) {
                *land.entry(zones.zone(x, y)).or_default() += 1;
            }
        }
        land.retain(|_, cells| *cells >= 400);
        let mut counts: BTreeMap<_, usize> = land.keys().map(|&zone| (zone, 0)).collect();
        for &(_, x, y) in &generated.start_waypoints {
            let zone = zones.zone(i32::from(x), i32::from(y));
            *counts
                .get_mut(&zone)
                .expect("start must be on a buildable island") += 1;
        }
        let counts: Vec<_> = counts.into_values().collect();
        eprintln!("Island War: {players} players / {islands} islands: {counts:?}");
        assert_eq!(
            land.len(),
            islands as usize,
            "exact disconnected island count"
        );
        assert_eq!(generated.start_waypoints.len(), players as usize);
        assert_eq!(generated.unfilled_start_slots, 0);
        assert!(counts.iter().max().unwrap() - counts.iter().min().unwrap() <= 1);
        assert_eq!(
            generated
                .start_waypoints
                .iter()
                .map(|&(_, x, y)| (x, y))
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            players as usize
        );
    }
}

#[test]
#[ignore = "slow; run when changing Mountainous generation"]
fn giant_mountainous_map_has_ridges_and_plateau_ramps() {
    let mut options = RmgOptions::default();
    options.apply_sed(&crate::rules::ini_parser::IniFile::from_str(
        "[RandomMap]\nMapType=4\nVERAMountainRidges=yes\nWidth=5\nHeight=5\nNumPlayers=30\nSeed=36753\n\
         Ruggedness=96\nRegionSize=4\nAccessibility=0\nWaterAmount=93\nTiberium=40\n\
         TiberiumLayout=66\nVegetation=75\nResources=2\n",
    ));
    options.normalize();
    let Some((_assets, prepared, _rules)) = retail_generation_inputs(&options) else {
        return;
    };
    let cliff_base = i32::from(prepared.theater.cliff_ranges.cliff_set.unwrap());
    let ramp_base = prepared.resolved_inputs.ids.ramp_base;
    let mut plateau_ramps = 0;
    let generated = generate_map_observed(
        &options,
        &prepared.settings,
        &prepared.resolved_inputs,
        &prepared.blocks,
        &prepared.tech_types,
        &mut |view| {
            if view.point() == GenerationPoint::After(Stage::IslandPasses) {
                plateau_ramps = view
                    .snapshot()
                    .map_file
                    .cells
                    .iter()
                    .filter(|cell| (ramp_base..ramp_base + 8).contains(&cell.tile_index))
                    .count();
            }
        },
    );
    let cliffs = generated
        .map_file
        .cells
        .iter()
        .filter(|cell| (cliff_base..cliff_base + 40).contains(&cell.tile_index))
        .count();
    eprintln!(
        "Giant Mountainous: cliffs={cliffs}, ramp_cells={plateau_ramps}, starts={}",
        generated.start_waypoints.len()
    );
    assert!(
        cliffs >= 2_000,
        "a Giant mountain layout needs visible ridge boundaries"
    );
    assert!(
        plateau_ramps >= 64,
        "plateaus need real ramp tiles before the Hills phase"
    );
    assert_eq!(generated.start_waypoints.len(), 30);
    assert_eq!(generated.unfilled_start_slots, 0);
    let (grid, zones) = generated_movement_zones(&options, &prepared, &generated.map_file);
    let (_, start_x, start_y) = generated.start_waypoints[0];
    let start_zone = zones.zone(i32::from(start_x), i32::from(start_y));
    let raised: Vec<_> = grid
        .native_cells()
        .filter(|&(x, y)| zones.is_clear_ground(x, y) && grid.get(x, y).unwrap().level >= 8)
        .collect();
    assert!(
        raised.len() >= 30_000,
        "a Giant map needs substantial usable high ground"
    );
    assert!(
        raised.iter().all(|&(x, y)| zones.zone(x, y) == start_zone),
        "raised plateaus must connect to the starting lowland through ramps"
    );
}
