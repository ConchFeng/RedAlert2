//! A slow display must not slow ordinary retail tank/infantry movement.
//! Only presentation scheduling varies; both runs use the same rules, seed,
//! actors, commands and production frame transaction.

use super::LocalFramePacer;
use crate::rules::ruleset::RuleSet;
use crate::sim::command::{Command, CommandEnvelope};
use crate::sim::house_state::HouseState;
use crate::sim::runtime::{SimResources, SimRuntime};
use crate::sim::world::{FrameEffects, Simulation, TickLane};

fn moving_scene(rules: RuleSet) -> (SimRuntime, [u64; 2], Vec<CommandEnvelope>) {
    let mut sim = Simulation::with_seed(72);
    sim.intern_rule_type_ids(&rules);
    sim.resolve_type_handles(&rules);
    sim.session_mut_for_test()
        .set_game_mode(crate::sim::scenario_session::ScenarioGameMode::from_legacy_nonzero(true));
    sim.session_mut_for_test()
        .game_options_mut_for_test()
        .game_speed = 0;
    crate::sim::arena_fixture::flat_ground(&mut sim, &rules);
    for (country, cell) in [("Americans", (4, 4)), ("British", (24, 24))] {
        let owner = sim.intern(country);
        let side = rules.country_side_index(country).unwrap().0;
        let mut house = HouseState::new(owner, side, Some(owner), true, 50_000, 10);
        house.project_country_mults(&rules, sim.interner());
        sim.houses_mut_for_test().register(house);
        sim.spawn_object("GACNST", country, cell.0, cell.1, 0, &rules)
            .unwrap();
    }
    let owner = sim.interner().get("Americans").unwrap();
    sim.session_mut_for_test()
        .set_current_house_for_test(Some(owner));
    sim.session_mut_for_test().publish_started();
    let actors = [("MTNK", 8), ("E1", 12)].map(|(name, y)| {
        sim.spawn_object(name, "Americans", 2, y, 0, &rules)
            .unwrap()
    });
    let commands = actors
        .into_iter()
        .zip([8, 12])
        .map(|(entity_id, y)| {
            CommandEnvelope::new(
                owner,
                sim.session().tick() + 1,
                Command::Move {
                    entity_id,
                    target_rx: 29,
                    target_ry: y,
                },
            )
        })
        .collect();
    (
        SimRuntime {
            simulation: sim,
            resources: SimResources::new(rules),
        },
        actors,
        commands,
    )
}

fn run(
    rules: RuleSet,
    presents_per_second: u64,
    pace_fastest: bool,
) -> (SimRuntime, [[i32; 3]; 2]) {
    let (mut rt, actors, commands) = moving_scene(rules);
    let mut pacer = LocalFramePacer::with_fastest_pacing(pace_fastest);
    let mut sent = false;
    // Include the one-second endpoint in both schedules, avoiding a different
    // final wall instant for 25 Hz and 60 Hz.
    for present in 0..=presents_per_second {
        let now_ms = present * 1_000 / presents_per_second;
        for pass in 0..5 {
            if (pass != 0 && pacer.budget_ms(0, None).is_none()) || !pacer.should_admit(now_ms, 0, None, false)
            {
                break;
            }
            let before = rt.simulation.session().tick();
            rt.advance_frame(
                if sent { &[] } else { &commands },
                pacer.frame_tick_ms(0),
                TickLane::Ordinary,
                FrameEffects::default(),
            )
            .unwrap();
            assert_eq!(rt.simulation.session().tick(), before + 1);
            sent = true;
            pacer.record_admitted_frame(now_ms, 0);
        }
    }
    let positions = actors.map(|id| {
        let entity = rt.simulation.entities().get(id).unwrap();
        let coord = crate::sim::movement::ground_pose::object_location(
            entity,
            rt.simulation.resolved_terrain(),
        );
        assert!(
            coord.x > 2 * 256 + 128,
            "retail mover must actually leave its start"
        );
        [coord.x, coord.y, coord.z]
    });
    (rt, positions)
}

#[test]
fn tank_and_infantry_cover_the_same_distance_at_twenty_five_and_sixty_presents() {
    let Some(retail) = crate::rules::retail_ini_fixture::retail_battle_rules_for_map("XMP03T4.MAP")
    else {
        return;
    };
    let (pc, pc_positions) = run(retail.rules.clone(), 60, true);
    let (switch, switch_positions) = run(retail.rules.clone(), 25, true);
    assert_eq!(
        pc.simulation.session().tick(),
        switch.simulation.session().tick()
    );
    assert_eq!(
        pc.simulation.session().total_sim_ms(),
        switch.simulation.session().total_sim_ms()
    );
    assert_eq!(pc_positions, switch_positions);
    assert_eq!(pc.simulation.state_hash(), switch.simulation.state_hash());
    eprintln!(
        "25/60 presents: ticks={}, sim_ms={}, positions={switch_positions:?}",
        switch.simulation.session().tick(),
        switch.simulation.session().total_sim_ms()
    );

    // Reproduce the old fastest path: at a slow display it visits Logic less
    // often and both real movers travel a shorter distance.
    let (old, old_positions) = run(retail.rules, 25, false);
    assert!(old.simulation.session().tick() * 2 < switch.simulation.session().tick());
    for (before, after) in old_positions.into_iter().zip(switch_positions) {
        assert!(before[0] < after[0]);
    }
}
