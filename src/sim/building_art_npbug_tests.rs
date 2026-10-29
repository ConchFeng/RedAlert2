//! NPBUG regressions for the Grand_Opening active-animation slots.

use super::*;
use crate::rules::ini_parser::IniFile;
use crate::sim::house_state::HouseState;

/// NPBUG (NPExt bug-fix changelog item 95, issue #2971): a tech building's
/// active animations start when a real house owns it, not at first opening.
/// Native fills Idle18 + Active3..6 unconditionally
/// (`0x0044632E..0x00446374`), so an un-owned oil derrick pumps from
/// scenario placement and keeps pumping after the damaged-art swap; the
/// capture's `Grand_Opening` re-run (Building `0x00448CEF`) starts it.
#[test]
#[cfg(feature = "npbug")]
fn npbug_tech_derrick_pump_starts_on_capture_not_placement() {
    let art_ini = IniFile::from_str(
        "[CAOILD]\nActiveAnim=N\nActiveAnimDamaged=D\n\
         [CIVAN]\nActiveAnim=N\nActiveAnimDamaged=D\n\
         [N]\nLoopCount=-1\nRate=150\n[D]\nLoopCount=-1\nRate=300\n",
    );
    let mut rules = RuleSet::from_ini_with_fixed_art_for_test(
        &IniFile::from_str(
            "[BuildingTypes]\n0=CAOILD\n1=CIVAN\n\
             [CAOILD]\nStrength=1000\nNeedsEngineer=yes\n\
             [CIVAN]\nStrength=1000\n\
             [Animations]\n0=N\n1=D\n",
        ),
        &art_ini,
    )
    .unwrap();
    let mut art = crate::rules::art_data::ArtRegistry::from_ini(&art_ini);
    for name in ["N", "D"] {
        art.bind_anim_frame_count_for_test(name, 40);
    }
    rules.install_art_data(art);

    let mut sim = Simulation::with_seed(3);
    let neutral = sim.interner.intern("Neutral");
    let americans = sim.interner.intern("Americans");
    sim.houses
        .register(HouseState::new(neutral, 0, None, false, 0, 10));
    sim.houses
        .register(HouseState::new(americans, 1, None, true, 0, 10));
    sim.houses
        .get_mut(&neutral)
        .unwrap()
        .make_multiplay_passive();

    let pump =
        |sim: &Simulation, id: u64| sim.entities().get(id).unwrap().building_art.anim_slots[3];

    let derrick = sim
        .spawn_object("CAOILD", "Neutral", 10, 10, 0, &rules)
        .unwrap();
    // Scenario placement (`discover_techno_in_views`'s authored-map call).
    sim.grand_opening(
        derrick,
        false,
        true,
        &rules,
        None,
        crate::sim::world::FrameEffects::default(),
    );
    assert!(
        pump(&sim, derrick).is_none(),
        "uncaptured derrick stays still"
    );
    // Damage below yellow must not introduce the damaged pump either.
    sim.substrate
        .entities
        .get_mut(derrick)
        .unwrap()
        .health
        .set_for_test(250);
    sim.refresh_building_damage_state(derrick, &rules);
    assert!(
        pump(&sim, derrick).is_none(),
        "damage alone does not start the pump"
    );
    sim.substrate
        .entities
        .get_mut(derrick)
        .unwrap()
        .health
        .set_for_test(1000);
    sim.refresh_building_damage_state(derrick, &rules);
    // An engineer's capture through the ownership chokepoint starts it.
    sim.change_owner_with_rules(
        derrick,
        americans,
        &rules,
        None,
        crate::sim::world::FrameEffects::default(),
    );
    let anim = pump(&sim, derrick).expect("capture starts the pump");
    assert_eq!(
        sim.interner.resolve(sim.anim(anim).unwrap().type_id()),
        "N",
        "the normal pump, not the damaged variant"
    );

    // The gate is scoped to `NeedsEngineer=`: a non-tech building still
    // animates from its first opening while neutral-owned.
    let civilian = sim
        .spawn_object("CIVAN", "Neutral", 14, 10, 0, &rules)
        .unwrap();
    sim.grand_opening(
        civilian,
        false,
        true,
        &rules,
        None,
        crate::sim::world::FrameEffects::default(),
    );
    assert!(pump(&sim, civilian).is_some(), "non-tech keeps its anim");
}
