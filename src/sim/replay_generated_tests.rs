use super::*;
use crate::map::source::GeneratedMapRecipe;
use crate::map::waypoints::Waypoint;
use crate::sim::world::{FrameEffects, TickLane};

#[test]
fn generated_recipe_rejects_duplicate_starts_and_missing_seed_bytes() {
    let mut recipe = GeneratedMapRecipe {
        seed_bytes: b"[RandomMap]\nSeed=7\n".to_vec(),
        start_waypoints: vec![Waypoint {
            index: 0,
            rx: 20,
            ry: 24,
        }],
    };
    recipe.validate().unwrap();
    recipe.start_waypoints.push(recipe.start_waypoints[0]);
    assert!(
        recipe
            .validate()
            .unwrap_err()
            .to_string()
            .contains("start slots")
    );
    recipe.start_waypoints.pop();
    recipe.seed_bytes.clear();
    recipe.validate().unwrap();
    let mut serialized = serde_json::to_value(&recipe).unwrap();
    serialized.as_object_mut().unwrap().remove("seed_bytes");
    assert!(serde_json::from_value::<GeneratedMapRecipe>(serialized).is_err());
}

#[test]
#[ignore = "slow; retail generated Giant Free For All with 29 AI replay reconstruction"]
fn generated_ffa_replay_reconstructs_accepted_bytes_and_all_29_ai_hashes() {
    let (root, mut assets) =
        crate::rules::retail_ini_fixture::retail_assets().expect("retail assets");
    let rules_owner = crate::rules::retail_ini_fixture::retail_rules_owner(&assets);
    let mut options = crate::map::rmg::RmgOptions {
        width: 5,
        height: 5,
        num_players: 30,
        seed: 16692,
        map_type: 1,
        region_size: 33,
        ruggedness: 74,
        accessibility: 64,
        water_amount: 15,
        tiberium: 20,
        tiberium_layout: 97,
        vegetation: 80,
        urban_presence: 56,
        ..Default::default()
    };
    options.normalize();
    let bytes = options.to_sed_bytes();
    let generated = crate::map::rmg::load::load(
        &root,
        "ReplayGiant.SED",
        Some(&bytes),
        &mut assets,
        Some(&rules_owner),
    )
    .unwrap()
    .generated;
    assert_eq!(generated.unfilled_start_slots, 0);
    assert_eq!(generated.start_waypoints.len(), 30);
    let recipe = GeneratedMapRecipe {
        seed_bytes: bytes.clone(),
        start_waypoints: generated
            .start_waypoints
            .iter()
            .map(|&(index, rx, ry)| Waypoint {
                index: u32::from(index),
                rx,
                ry,
            })
            .collect(),
    };
    drop(generated);
    let opponents = vec![
        (
            crate::skirmish_launch::LaunchCountry::Russia,
            crate::skirmish_launch::AiDifficulty::Normal
        );
        29
    ];
    let mut launch = crate::headless_scenario::battle_session("ReplayGiant.SED", &opponents);
    launch.mode = crate::skirmish_launch::SkirmishLaunchMode::stock(2).unwrap();
    launch.humans[0].set_observer(true);
    launch.rebuild_house_roster();
    let mut selector_cache = crate::map::tile_variant_selector::TileVariantSelectorCache::default();
    let mut selector_rng = crate::sim::rng::SimRng::new(0x5eed);
    let mut draw = || selector_rng.next_u32();
    selector_cache
        .begin_load(&mut draw)
        .select_variant(0, 0, 0, 1, 1, 2);
    let selector_prefix = selector_cache.replay_prefix();
    let mut scene = crate::headless_scenario::load_with_launch_recipe_and_ini(
        &root,
        "ReplayGiant.SED",
        0x5eed_006,
        crate::sim::scenario_bootstrap::MatchLaunchDescriptor::from_resolved(launch.clone())
            .unwrap(),
        &Default::default(),
        crate::sim::scenario_session::ScenarioGameMode::Skirmish,
        None,
        None,
        Some(&recipe),
        selector_prefix.as_deref(),
    )
    .unwrap();
    scene.runtime.simulation.batch_path_searches();
    scene
        .runtime
        .simulation
        .hash_state_every(std::num::NonZeroU64::MIN);
    let initial_hash = scene.sim().state_hash();
    let startup = ReplayStartup::capture(
        &scene.runtime,
        &launch,
        &scene.source,
        scene.map.ini.content_hash(),
        22,
        false,
    )
    .unwrap()
    .with_selector_prefix(selector_prefix.clone())
    .unwrap();
    assert_eq!(startup.generated_map().unwrap(), &recipe);
    let mut log = ReplayLog::new(ReplayHeader {
        version: REPLAY_VERSION,
        tick_hz: 45,
        seed: 0x5eed_006,
        map_name: "ReplayGiant.SED".into(),
        rules_hash: scene.runtime.resources.rules.simulation_config_hash(),
        pixel_conversion_bounds: scene.sim().session().pixel_conversion_bounds(),
    });
    log.startup = Some(Box::new(startup));
    for _ in 0..30 {
        let mut output = scene
            .runtime
            .advance_frame(&[], 22, TickLane::Ordinary, FrameEffects::empty())
            .unwrap();
        let commands = output.take_admitted_commands();
        log.record_tick(output.tick.tick, commands, output.tick.state_hash);
    }
    let final_hash = scene.sim().state_hash();
    let encoded = serde_json::to_vec(&log).unwrap();
    let decoded: ReplayLog = serde_json::from_slice(&encoded).unwrap();
    let replay = std::sync::Arc::new(decoded);
    let (mut candidate, mut playback) =
        ReplayRunner::reconstruct(&root, replay.clone(), None).unwrap();
    assert_eq!(candidate.sim().state_hash(), initial_hash);
    assert!(candidate.runtime.resources.terrain_template.is_some());
    assert!(candidate.variant_selector_cache.is_initialized());
    assert_eq!(
        (playback.position(), playback.len(), playback.next_tick()),
        (0, 30, Some(1))
    );
    while playback.can_advance() {
        playback
            .advance(&mut candidate.runtime, FrameEffects::empty())
            .unwrap();
    }
    assert_eq!(
        playback
            .verification(&candidate.runtime)
            .unwrap()
            .final_state_hash,
        final_hash
    );
    assert_eq!(playback.position(), 30);
    assert!(std::sync::Arc::ptr_eq(playback.replay(), &replay));
    assert_eq!(
        candidate.variant_selector_cache.replay_prefix(),
        selector_prefix
    );
    let mut corrupted = (*replay).clone();
    corrupted
        .startup
        .as_mut()
        .unwrap()
        .generated_map
        .as_mut()
        .unwrap()
        .seed_bytes
        .push(b' ');
    assert!(
        corrupted
            .standalone_startup()
            .unwrap_err()
            .to_string()
            .contains("recorded identity")
    );
}
