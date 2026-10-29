//! Selling44A16B's refused Unit Unlimbo and the ShortGame defeat gate
//! (issue #2965). The same synthetic Track-on-Water refusal for both
//! capabilities: stock keeps the never-placed MCV counted, npbug removes it
//! so the emptied house can be defeated. No retail-map claim.

use super::*;
use crate::map::resolved_terrain::{ResolvedTerrainGrid, test_clear_cell};
use crate::rules::ini_parser::IniFile;
use crate::rules::ruleset::RuleSet;
use crate::rules::terrain_rules::{LandType, SpeedCostProfile};
use crate::sim::house_state::HouseState;
use crate::sim::world::FrameEffects;

fn rules() -> RuleSet {
    // `Foundation=` is an ART read; the synthetic rules carry the fixed art
    // section that gives the yard its 4x4 footprint.
    let ini = IniFile::from_str(
        "[Countries]\n0=Country0\n[Country0]\n[Sides]\nSide0=Country0\n\
         [General]\nBaseUnit=MCV\n\
         [VehicleTypes]\n0=MCV\n\
         [BuildingTypes]\n0=YARD\n\
         [MCV]\nStrength=1000\nSpeedType=Track\n\
         Locomotor={4A582741-9839-11d1-B709-00A024DDAFD1}\n\
         [YARD]\nStrength=1000\nCost=1500\nUndeploysInto=MCV\n\
         ConstructionYard=yes\nPlaceAnywhere=yes\n",
    );
    let art = IniFile::from_str("[YARD]\nFoundation=4x4\n");
    RuleSet::from_ini_with_fixed_art_for_test(&ini, &art).expect("undeploy defeat rules")
}

/// A 16x16 clear plane whose cell (11, 11) is water. That is the repack
/// cell of a 4x4 yard standing at (10, 10): the same >2-foundation
/// `+1, +1` step the retail fixture exercises.
fn terrain() -> ResolvedTerrainGrid {
    const WIDTH: u16 = 16;
    const HEIGHT: u16 = 16;
    let mut cells: Vec<_> = (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| test_clear_cell(x, y)))
        .collect();
    let blocked = &mut cells[usize::from(11 * WIDTH + 11)];
    blocked.land_type = LandType::Water.as_index();
    blocked.speed_costs = SpeedCostProfile::ZERO;
    assert_eq!(blocked.speed_costs.track, Some(0));
    ResolvedTerrainGrid::from_cells_with_tubes(WIDTH, HEIGHT, cells, Vec::new())
}

/// A redeploy-enabled short-game skirmish whose single human house holds
/// one yard at (10, 10), with the pack-up control installed for the
/// Selling mission. Returns the rules, the sim, the yard and the cached
/// refund.
fn scene() -> (RuleSet, Simulation, InternedId, u64, i32) {
    let mut rules = rules();
    let mut sim = Simulation::with_seed(0x2965);
    sim.session.set_binary_frame_for_test(100);
    sim.session
        .set_game_mode(crate::sim::scenario_session::ScenarioGameMode::from_legacy_nonzero(true));
    sim.session.game_options_mut_for_test().mcv_redeploy = true;
    sim.session.game_options_mut_for_test().short_game = true;
    let owner = sim.interner.intern("house0");
    let country = sim.interner.intern("Country0");
    sim.houses
        .register(HouseState::new(owner, 0, Some(country), true, 0, 10));
    sim.session.set_current_house_for_test(Some(owner));
    sim.resolved_terrain = Some(terrain());
    sim.intern_rule_type_ids(&rules);
    sim.resolve_type_handles(&rules);
    // The conversion runs through the Selling mission, which needs the
    // pack-up control a real yard carries.
    rules.set_buildup_control_for_test("YARD", [0, 2, 2]);
    let yard = sim
        .spawn_object_at_height("YARD", "house0", 10, 10, 0, 0, &rules)
        .expect("the PlaceAnywhere yard stands on its clear origin");
    assert!(sim.entities().get(yard).unwrap().lifecycle.cell_marked());
    let refund =
        production::techno_refund(&sim, &rules, yard).expect("the conversion caches a refund");
    (rules, sim, owner, yard, refund)
}

/// Drive the player's repack order through the Selling mission until the
/// conversion retires the yard.
fn run_repack(sim: &mut Simulation, rules: &RuleSet, yard: u64) {
    assert!(sim.undeploy_building(yard, rules, None, FrameEffects::default()));
    let [_, frames, rate] = rules.buildup_control("YARD");
    for _ in 0..(frames * rate * 4 + 240) {
        sim.advance_tick(&[], Some(rules), None, None, 67);
        if sim.entities().get(yard).is_none() {
            return;
        }
    }
    panic!("the Selling mission never completed the conversion");
}

fn base_units<'a>(sim: &'a Simulation, rules: &'a RuleSet) -> Vec<Option<InternedId>> {
    rules
        .general
        .base_unit_types
        .iter()
        .map(|name| sim.interner().get(name))
        .collect()
}

/// Native pin, synthetic: the refused repack refunds the yard, uninits it
/// and keeps the constructed MCV in limbo with its tracking intact, so the
/// ShortGame BaseUnit sum holds the emptied house alive.
#[test]
#[cfg(not(feature = "npbug"))]
fn refused_repack_keeps_the_limbo_mcv_counted_and_the_house_alive() {
    let (rules, mut sim, owner, yard, refund) = scene();
    let initial = i32::MAX - 1000;
    sim.houses
        .get_mut(&owner)
        .unwrap()
        .economy_mut()
        .set_credits_for_test(initial);
    run_repack(&mut sim, &rules, yard);
    assert_eq!(
        sim.houses.get(&owner).unwrap().economy().credits(),
        initial.wrapping_add(refund),
        "44A16B paid the cached refund to the live owner"
    );
    let mcv = sim
        .entities()
        .values()
        .find(|entity| sim.interner().resolve(entity.type_ref()) == "MCV")
        .expect("the refused placement keeps its constructed MCV");
    let mcv_type = mcv.type_ref();
    assert!(mcv.is_object_alive() && mcv.in_limbo());
    assert!(!mcv.lifecycle.cell_marked());
    let tracked = sim.houses.get(&owner).unwrap().tracking();
    assert_eq!(tracked.owned_count(EntityCategory::Unit, mcv_type), 1);
    assert_eq!(tracked.active_count(EntityCategory::Unit, mcv_type), 0);
    assert_eq!(tracked.buildings(), 0);
    let units = base_units(&sim, &rules);
    assert!(
        tracked.game_alive(true, units.iter().copied(), None, |_| false),
        "ShortGame's BaseUnit sum keeps the house alive"
    );
    sim.check_defeat(Some(&rules), None);
    assert!(
        !sim.houses.get(&owner).unwrap().is_defeated(),
        "the counted phantom defeats every defeat check"
    );
}

/// NPBUG #2965 (NPExt changelog item 6): the refused repack deletes the
/// never-placed MCV, so its BaseUnit count goes with it and the emptied
/// house is defeated. The refund is untouched.
#[test]
#[cfg(feature = "npbug")]
fn npbug_refused_repack_stops_counting_the_phantom_and_the_house_falls() {
    let (rules, mut sim, owner, yard, refund) = scene();
    let initial = i32::MAX - 1000;
    sim.houses
        .get_mut(&owner)
        .unwrap()
        .economy_mut()
        .set_credits_for_test(initial);
    run_repack(&mut sim, &rules, yard);
    assert_eq!(
        sim.houses.get(&owner).unwrap().economy().credits(),
        initial.wrapping_add(refund),
        "the fix changes the accounting, not the refund"
    );
    assert!(
        sim.entities()
            .values()
            .all(|entity| sim.interner().resolve(entity.type_ref()) != "MCV"),
        "the unreachable limbo constructor is gone"
    );
    let mcv_type = sim.interner().get("MCV").unwrap();
    let tracked = sim.houses.get(&owner).unwrap().tracking();
    assert_eq!(tracked.owned_count(EntityCategory::Unit, mcv_type), 0);
    assert_eq!(tracked.buildings(), 0);
    let units = base_units(&sim, &rules);
    assert!(
        !tracked.game_alive(true, units.iter().copied(), None, |_| false),
        "the defeat gate's counts say the house holds nothing"
    );
    sim.check_defeat(Some(&rules), None);
    assert!(
        sim.houses.get(&owner).unwrap().is_defeated(),
        "the emptied house is no longer immortal"
    );
}
