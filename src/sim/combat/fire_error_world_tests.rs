//! GetFireError through the production fire path: the facts the world
//! supplies and each class's reaction to the code.

use crate::map::entities::EntityCategory;
use crate::rules::ini_parser::IniFile;
use crate::rules::ruleset::RuleSet;
use crate::sim::entity_store::EntityStore;
use crate::sim::game_entity::GameEntity;
use crate::sim::intern::test_interner;

fn rules() -> RuleSet {
    RuleSet::from_ini(&IniFile::from_str(
        "[VehicleTypes]\n0=TANK\n1=MEDV\n2=HOUND\n3=BEAST\n4=CARR\n\
         [InfantryTypes]\n\
         [BuildingTypes]\n0=COIL\n1=BUNK\n\
         [AircraftTypes]\n\
         [TANK]\nStrength=300\nArmor=heavy\nPrimary=Gun\n\
         [MEDV]\nStrength=300\nArmor=heavy\nPrimary=Mend\n\
         [HOUND]\nStrength=300\nArmor=heavy\nPrimary=Gun\nNatural=yes\n\
         [BEAST]\nStrength=300\nArmor=heavy\nPrimary=Gun\nUnnatural=yes\n\
         [CARR]\nStrength=300\nArmor=heavy\nPrimary=Launch\n\
         [COIL]\nStrength=600\nArmor=concrete\nPrimary=Zap\nPowered=yes\nPower=-75\n\
         [BUNK]\nStrength=600\nArmor=concrete\nCanBeOccupied=yes\nCanOccupyFire=yes\n\
         MaxNumberOccupants=5\n\
         [Gun]\nDamage=50\nROF=30\nRange=6\nWarhead=AP\nOmniFire=yes\n\
         [Mend]\nDamage=-50\nROF=30\nRange=6\nWarhead=AP\nOmniFire=yes\n\
         [Launch]\nDamage=50\nROF=30\nRange=6\nWarhead=AP\nOmniFire=yes\nSpawner=yes\n\
         [Zap]\nDamage=100\nROF=60\nRange=6\nWarhead=AP\n\
         [AP]\nVerses=100%,100%,100%,100%,100%,100%,100%,100%,100%,100%,100%\n",
    ))
    .expect("fire-error fixture rules")
}

fn spawn(
    store: &mut EntityStore,
    id: u64,
    type_name: &str,
    category: EntityCategory,
    owner: &str,
    at: (u16, u16),
) {
    let mut entity = GameEntity::test_default(id, type_name, owner, at.0, at.1);
    entity.set_category_for_test(category);
    entity.health.set_for_test(300);
    entity.lifecycle.set_in_limbo_for_test(false);
    store.insert(entity);
}

/// A flat 16x16 map whose row `y = 10` from `x = 10` to `x = 12` is a bridge
/// (cell flag `0x100`).
fn bridge_row_terrain() -> crate::map::resolved_terrain::ResolvedTerrainGrid {
    let cells = (0..16u16)
        .flat_map(|ry| (0..16u16).map(move |rx| (rx, ry)))
        .map(|(rx, ry)| {
            let mut cell = crate::map::resolved_terrain::test_flat_cell(rx, ry);
            if ry == 10 && (10..=12).contains(&rx) {
                cell.bridge_facts.raw_flags = crate::map::bridge_facts::BRIDGE_FLAG_STRUCTURAL;
            }
            cell
        })
        .collect();
    crate::map::resolved_terrain::ResolvedTerrainGrid::from_cells(16, 16, cells)
}

fn bridge_scene(firer: &str) -> crate::sim::world::Simulation {
    let mut sim = crate::sim::world::Simulation::new();
    sim.resolved_terrain = Some(bridge_row_terrain());
    spawn(
        &mut sim.substrate.entities,
        1,
        firer,
        EntityCategory::Unit,
        "Soviet",
        (10, 10),
    );
    spawn(
        &mut sim.substrate.entities,
        2,
        "TANK",
        EntityCategory::Unit,
        "Americans",
        (12, 10),
    );
    // After the spawns: `test_default` interns their names.
    sim.interner = test_interner();
    sim
}

#[path = "fire_error_cell_tests.rs"]
mod cell_queries;

/// A world without resolved terrain asks the shared mapless range
/// (`in_range::mapless_in_range`): `Range=-2` is always in range, where the
/// flat fallback read -512 leptons as a two-cell reach, and MinimumRange
/// refuses a target inside it.
#[test]
fn a_mapless_fire_gate_takes_the_shared_range_answer() {
    let rules = RuleSet::from_ini(&IniFile::from_str(
        "[VehicleTypes]\n0=ASW\n1=ART\n2=TANK\n\
         [InfantryTypes]\n[BuildingTypes]\n[AircraftTypes]\n\
         [ASW]\nStrength=300\nArmor=heavy\nPrimary=Depth\n\
         [ART]\nStrength=300\nArmor=heavy\nPrimary=Shell\n\
         [TANK]\nStrength=300\nArmor=heavy\n\
         [Depth]\nDamage=50\nROF=30\nRange=-2\nWarhead=AP\nOmniFire=yes\n\
         [Shell]\nDamage=50\nROF=30\nRange=8\nMinimumRange=3\nWarhead=AP\nOmniFire=yes\n\
         [AP]\nVerses=100%,100%,100%,100%,100%,100%,100%,100%,100%,100%,100%\n",
    ))
    .expect("mapless fire fixture rules");
    let mut sim = crate::sim::world::Simulation::new();
    for (id, kind, owner, at) in [
        (1, "ASW", "Soviet", (5, 5)),
        (2, "ART", "Soviet", (5, 7)),
        (3, "TANK", "Americans", (6, 7)),
        (4, "TANK", "Americans", (10, 7)),
        (5, "TANK", "Americans", (40, 5)),
    ] {
        spawn(
            &mut sim.substrate.entities,
            id,
            kind,
            EntityCategory::Unit,
            owner,
            at,
        );
    }
    // After the spawns: `test_default` interns their names.
    sim.interner = test_interner();
    assert!(sim.resolved_terrain.is_none());
    let error = |firer: u64, kind: &str, target: u64| {
        crate::sim::combat::fire_error_world::FireSubject {
            world: &sim,
            rules: &rules,
            overlay_registry: None,
            fog: None,
            firer: sim.substrate.entities.get(firer).unwrap(),
            obj: rules.object(kind).unwrap(),
            target: Some(crate::sim::combat::TargetKind::Entity(target)),
            weapon_index: 0,
        }
        .fire_error(true)
    };
    use crate::sim::combat::fire_error::FireError;
    assert_ne!(error(1, "ASW", 5), FireError::Range, "35 cells away");
    assert_eq!(error(2, "ART", 3), FireError::Range, "inside MinimumRange");
    assert_ne!(error(2, "ART", 4), FireError::Range, "5 cells away");
    assert_eq!(error(2, "ART", 5), FireError::Range, "beyond Range");
}

/// NPBUG (NPExt bug-fix changelog item 72, issue #2961): a NavalTargeting=6
/// firer refuses a force-attack on open water — native T41 consulted no
/// naval selector for a Cell target, so the squid-class shot was admitted.
#[test]
fn npbug_naval_targeting_six_refuses_the_water_surface() {
    let rules = RuleSet::from_ini(&IniFile::from_str(
        "[VehicleTypes]\n0=TANK\n\
         [InfantryTypes]\n0=SQUID\n\
         [SQUID]\nStrength=300\nArmor=light\nPrimary=Tentacle\nNavalTargeting=6\n\
         [TANK]\nStrength=300\nArmor=heavy\nPrimary=Gun\n\
         [Tentacle]\nDamage=50\nROF=30\nRange=6\nWarhead=AP\nOmniFire=yes\n\
         [Gun]\nDamage=50\nROF=30\nRange=6\nWarhead=AP\nOmniFire=yes\n\
         [AP]\nVerses=100%,100%,100%,100%,100%,100%,100%,100%,100%,100%,100%\n",
    ))
    .unwrap();
    let cells = (0..16u16)
        .flat_map(|ry| (0..16u16).map(move |rx| (rx, ry)))
        .map(|(rx, ry)| {
            let mut cell = crate::map::resolved_terrain::test_flat_cell(rx, ry);
            if (10..=12).contains(&rx) && ry == 10 {
                cell.land_type = 2;
            }
            cell
        })
        .collect();
    let terrain = crate::map::resolved_terrain::ResolvedTerrainGrid::from_cells(16, 16, cells);
    let mut sim = crate::sim::world::Simulation::new();
    sim.resolved_terrain = Some(terrain);
    spawn(
        &mut sim.substrate.entities,
        1,
        "SQUID",
        EntityCategory::Infantry,
        "Soviet",
        (11, 10),
    );
    sim.interner = test_interner();
    let firer = sim.substrate.entities.get(1).unwrap();
    let subject = crate::sim::combat::fire_error_world::FireSubject {
        world: &sim,
        rules: &rules,
        overlay_registry: None,
        fog: None,
        firer,
        obj: rules.object("SQUID").unwrap(),
        target: Some(crate::sim::combat::TargetKind::Cell(12, 10)),
        weapon_index: 0,
    };
    let error = subject.fire_error(false);
    if crate::rules::NPBUG_ENABLED {
        assert_eq!(
            error,
            crate::sim::combat::fire_error::FireError::Illegal,
            "the water surface is refused under npbug"
        );
    } else {
        assert_ne!(
            error,
            crate::sim::combat::fire_error::FireError::Illegal,
            "stock admits the water force-attack"
        );
    }
}
