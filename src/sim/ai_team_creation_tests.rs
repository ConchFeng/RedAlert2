//! Replay of `tools/ai_team_oracle.json` against the computer's team
//! creation: the team block, the selector, the eligibility arithmetic and the
//! Iron Curtain readiness. Its chooser rows replay in `ai_unit_choice`'s
//! tests, which share these readers.
//!
//! Each row carries what the native run's stubs answered (the selector's
//! eligibility test, the draws); the Rust owner must ask in the same order
//! and reach the same result. The eligibility gates these rows leave open
//! rest on instruction reading.

use std::hash::{DefaultHasher, Hasher};

use super::*;
use crate::rules::ini_parser::IniFile;
use crate::sim::house_state::{HouseDifficulty, HouseState};
use crate::sim::rng::SimRng;
use crate::sim::superweapon::charge_nearly_full;
use crate::sim::team_script_vm::{TeamScriptDefinition, TeamTaskForceDefinition};
use crate::sim::timer::CdTimer;
use crate::util::native_x87::{NativeF32Bits, NativeF64Bits};
use serde_json::Value;

pub(crate) fn rows(section: &str) -> Vec<Value> {
    let oracle: Value =
        serde_json::from_str(crate::test_fixture::text("tools/ai_team_oracle.json")).unwrap();
    oracle[section].as_array().unwrap().clone()
}

/// A dword as the original held it; the stubs record arguments unsigned.
pub(crate) fn dword(value: &Value) -> i32 {
    value.as_i64().unwrap() as u32 as i32
}

pub(crate) fn dwords(value: &Value) -> Vec<i32> {
    value.as_array().unwrap().iter().map(dword).collect()
}

pub(crate) fn flag(value: &Value) -> bool {
    value.as_bool().unwrap()
}

pub(crate) fn index(value: &Value) -> usize {
    usize::try_from(value.as_u64().unwrap()).unwrap()
}

pub(crate) fn events<'a>(row: &'a Value, kind: &str) -> Vec<&'a Value> {
    row["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event[0] == kind)
        .collect()
}

/// The original's draws, `(low, high, answer)`, all on the Scenario RNG
/// (`Scenario+0x218`).
pub(crate) fn native_draws(row: &Value) -> Vec<(i32, i32, i32)> {
    events(row, "draw")
        .into_iter()
        .map(|draw| {
            assert_eq!(draw[1], 0x218, "a draw off the Scenario RNG");
            (dword(&draw[2]), dword(&draw[3]), dword(&draw[4]))
        })
        .collect()
}

/// `RandomRanged` answering a row's `answers` in order.
pub(crate) struct Draws {
    answers: std::vec::IntoIter<i32>,
    pub(crate) asked: Vec<(i32, i32, i32)>,
}

impl Draws {
    pub(crate) fn new(row: &Value) -> Self {
        Self {
            answers: dwords(&row["answers"]).into_iter(),
            asked: Vec::new(),
        }
    }

    pub(crate) fn draw(&mut self, low: i32, high: i32) -> i32 {
        let value = self.answers.next().expect("an answer for every draw");
        self.asked.push((low, high, value));
        value
    }
}

/// An AI trigger `id` of `weight` whose bounds pin that weight, so the
/// feedback of an evicted team (`TeamClass::~TeamClass`, which the selector
/// rows do not run) leaves it.
pub(crate) fn ai_trigger(
    id: InternedId,
    primary: InternedId,
    secondary: Option<InternedId>,
    weight: NativeF64Bits,
) -> TeamAiTriggerDefinition {
    TeamAiTriggerDefinition {
        id,
        tokens: Default::default(),
        display_name: String::new(),
        enabled: true,
        primary_team_type: Some(primary),
        owner: Some(TeamAiTriggerOwner::All),
        threshold: 0,
        condition: -1,
        object_type: None,
        comparison_mask: [0; 32],
        weights: [weight; 3],
        multiplayer: true,
        side: 0,
        storage_flag_d1: false,
        secondary_team_type: secondary,
        difficulty_enabled: [true; 3],
        source: TeamAiDefinitionSource::FixedAimd,
    }
}

/// A selector row's world: the house `H0` and its enemy `H1`, a TeamType
/// per `team_types` entry (its `IsBaseDefense=`), the row's teams in order
/// (forming unless `formed`) and its AI triggers.
struct SelectorFixture {
    sim: Simulation,
    rules: RuleSet,
    owner: InternedId,
    enemy: InternedId,
    team_types: Vec<InternedId>,
    triggers: Vec<InternedId>,
    teams: Vec<u64>,
}

fn selector_fixture(row: &Value) -> SelectorFixture {
    let mut rules = RuleSet::from_ini(&IniFile::from_str("")).unwrap();
    rules.general.total_ai_team_cap = dwords(&row["cap"]);
    rules.general.maximum_ai_defensive_teams = dwords(&row["max_defense"]);
    let mut sim = Simulation::new();
    sim.session
        .set_game_mode(crate::sim::scenario_session::ScenarioGameMode::from_legacy_nonzero(true));
    let owner = sim.interner.intern("H0");
    let enemy = sim.interner.intern("H1");
    for name in [owner, enemy] {
        sim.houses
            .register(HouseState::new(name, 0, None, false, 0, 10));
    }
    sim.houses.reorder_for_test(&[owner, enemy]);
    let house = sim.houses.get_mut(&owner).unwrap();
    house.set_difficulty_for_test(HouseDifficulty::from_native(dword(&row["difficulty"])).unwrap());
    house.team_creation_mut().ratio = dword(&row["ratio"]);
    house.set_ai_triggers_active(flag(&row["active"]));
    house.set_enemy_house_for_test(flag(&row["enemy"]).then_some(enemy));
    let script = sim.interner.intern("SCRIPT");
    let task_force = sim.interner.intern("TASKFORCE");
    sim.team_script_vm.register_script(TeamScriptDefinition {
        id: script,
        actions: Vec::new(),
        source: TeamAiDefinitionSource::FixedAimd,
    });
    sim.team_script_vm
        .register_task_force(TeamTaskForceDefinition {
            id: task_force,
            group: -1,
            entries: Vec::new(),
            source: TeamAiDefinitionSource::FixedAimd,
        });
    let mut team_types = Vec::new();
    for (slot, base_defense) in row["team_types"].as_array().unwrap().iter().enumerate() {
        let id = sim.interner.intern(&format!("TEAMTYPE{slot}"));
        sim.team_script_vm.register_team_type(TeamTypeDefinition {
            id,
            script_id: script,
            task_force_id: task_force,
            priority: 0,
            is_base_defense: flag(base_defense),
            suicide: false,
            aggressive: false,
            combined_movement_zone: MovementZone::Normal,
            base_zone_relation_enforced: false,
            transport_crossing_required: false,
        });
        team_types.push(id);
    }
    let mut teams = Vec::new();
    for team in row["teams"].as_array().unwrap() {
        let team_owner = if team.get("ours").is_none_or(flag) {
            owner
        } else {
            enemy
        };
        let team_type = team_types[index(&team["type"])];
        let created = dword(&team["created"]);
        let vm = &mut sim.team_script_vm;
        teams.push(if team.get("formed").is_some_and(flag) {
            vm.create_team_from_type(team_owner, team_type, &[], created)
        } else {
            vm.construct_team(team_type, team_owner, true, created, false, None)
                .unwrap()
        });
    }
    let mut triggers = Vec::new();
    for (slot, trigger) in row["triggers"].as_array().unwrap().iter().enumerate() {
        let id = sim.interner.intern(&format!("TRIGGER{slot}"));
        let weight = u64::from_str_radix(trigger[2].as_str().unwrap(), 16).unwrap();
        sim.team_script_vm.register_ai_trigger(ai_trigger(
            id,
            team_types[index(&trigger[0])],
            trigger[1]
                .as_u64()
                .map(|second| team_types[second as usize]),
            NativeF64Bits::from_bits(weight),
        ));
        triggers.push(id);
    }
    SelectorFixture {
        sim,
        rules,
        owner,
        enemy,
        team_types,
        triggers,
        teams,
    }
}

#[test]
fn the_selector_matches_the_original() {
    for (number, row) in rows("selector").iter().enumerate() {
        let context = format!("{} selector row {number}", row["name"]);
        let SelectorFixture {
            mut sim,
            rules,
            owner,
            enemy,
            team_types,
            triggers,
            teams,
        } = selector_fixture(row);
        let answers: Vec<bool> = row["eligible"]
            .as_array()
            .unwrap()
            .iter()
            .map(flag)
            .collect();
        let mut asked = Vec::new();
        let mut draws = Draws::new(row);
        let picked = select_team_types_with(
            &mut sim,
            &rules,
            owner,
            |_, trigger, asked_enemy, defense_full| {
                let slot = triggers.iter().position(|&id| id == trigger.id).unwrap();
                asked.push((slot, asked_enemy, defense_full));
                answers[slot]
            },
            |_, low, high| draws.draw(low, high),
            crate::sim::world::FrameEffects::default(),
        );
        let slot_of = |id: InternedId| team_types.iter().position(|&tt| tt == id).unwrap();
        let picked: Vec<usize> = picked.into_iter().map(slot_of).collect();
        let native_picked: Vec<usize> = row["picked"]
            .as_array()
            .unwrap()
            .iter()
            .map(index)
            .collect();
        assert_eq!(picked, native_picked, "{context}: picked");
        let autocreate: Vec<i32> = team_types
            .iter()
            .map(|&id| i32::from(sim.team_script_vm.team_type_ini(id).unwrap().autocreate))
            .collect();
        assert_eq!(
            autocreate,
            dwords(&row["autocreate"]),
            "{context}: Autocreate="
        );
        let native_enemy = flag(&row["enemy"]).then_some(enemy);
        let native_asked: Vec<(usize, Option<InternedId>, bool)> = events(row, "eligible")
            .into_iter()
            .map(|call| (index(&call[1]), native_enemy, dword(&call[2]) != 0))
            .collect();
        assert_eq!(asked, native_asked, "{context}: eligibility asked");
        let live: Vec<u64> = sim
            .team_script_vm
            .teams_in_order()
            .map(|team| team.id())
            .collect();
        let destroyed: Vec<usize> = events(row, "destroy")
            .into_iter()
            .map(|call| index(&call[1]))
            .collect();
        let native_live: Vec<u64> = teams
            .iter()
            .enumerate()
            .filter(|(slot, _)| !destroyed.contains(slot))
            .map(|(_, &id)| id)
            .collect();
        assert_eq!(live, native_live, "{context}: teams left");
        assert_eq!(draws.asked, native_draws(row), "{context}: draws");
    }
}

#[test]
fn the_eligibility_arithmetic_matches_the_original() {
    for (number, row) in rows("eligibility").iter().enumerate() {
        let context = format!("{} eligibility row {number}", row["name"]);
        let eligible = flag(&row["eligible"]);
        match row["name"].as_str().unwrap() {
            "defense" => {
                let base_defense =
                    flag(&row["base_defense"]) || row["second"].as_bool() == Some(true);
                // The house plays Normal (index 1).
                let admitted = defense_gate(
                    flag(&row["enemy"]),
                    flag(&row["use_min_rule"]),
                    dword(&row["defense_teams"]),
                    dword(&row["min_defense"][1]),
                    base_defense,
                    flag(&row["flag"]),
                );
                assert_eq!(admitted, eligible, "{context}");
            }
            "compare" => {
                let mut mask = [0u8; 32];
                mask[..4].copy_from_slice(&dword(&row["amount"]).to_le_bytes());
                mask[4..8].copy_from_slice(&dword(&row["comparator"]).to_le_bytes());
                assert_eq!(compare(&mask, dword(&row["money"])), eligible, "{context}");
            }
            "power" => {
                let holds = power_condition_holds(
                    dword(&row["condition"]),
                    dword(&row["output"]),
                    dword(&row["drain"]),
                );
                assert_eq!(holds, eligible, "{context}");
            }
            other => panic!("no eligibility section {other}"),
        }
    }
}

#[test]
fn the_super_weapon_readiness_matches_the_original() {
    for (number, row) in rows("charge").iter().enumerate() {
        let remaining = CdTimer::from_raw(dword(&row["start"]), dword(&row["duration"]))
            .remaining(dword(&row["frame"]));
        let percent = NativeF32Bits::from_bits(dword(&row["percent"]) as u32);
        assert_eq!(
            charge_nearly_full(remaining, dword(&row["recharge"]), percent),
            flag(&row["ready"]),
            "charge row {number}"
        );
    }
}

fn rng_digest(rng: &SimRng) -> u64 {
    let mut hasher = DefaultHasher::new();
    rng.hash_state(&mut hasher);
    hasher.finish()
}

#[test]
fn the_team_block_matches_the_original() {
    for (number, row) in rows("team_block").iter().enumerate() {
        let mut rules = RuleSet::from_ini(&IniFile::from_str("")).unwrap();
        rules.general.team_delays = dwords(&row["delays"]);
        let mut sim = Simulation::new();
        sim.session.set_game_mode(
            crate::sim::scenario_session::ScenarioGameMode::from_legacy_nonzero(
                dword(&row["game_mode"]) != 0,
            ),
        );
        sim.session
            .set_binary_frame_for_test(dword(&row["frame"]) as u32);
        let owner = sim.interner.intern("H0");
        let mut house = HouseState::new(owner, 0, None, flag(&row["human"]), 0, 10);
        house.set_player_control_for_test(flag(&row["control"]));
        house.set_multiplay_passive_for_test(flag(&row["passive"]));
        house.set_difficulty_for_test(
            HouseDifficulty::from_native(dword(&row["difficulty"])).unwrap(),
        );
        house.team_creation_mut().timer =
            CdTimer::from_raw(dword(&row["start"]), dword(&row["duration"]));
        sim.houses.register(house);
        let before = rng_digest(&sim.scenario_rng);
        update_team_creation(
            &mut sim,
            &rules,
            owner,
            crate::sim::world::FrameEffects::default(),
        );
        let timer = sim.houses[&owner].team_creation().timer;
        assert_eq!(
            vec![timer.start_frame(), timer.duration()],
            dwords(&row["timer"]),
            "team block row {number}: timer"
        );
        // The selector's first draw, `RandomRanged(1, 100)`, marks its call.
        assert_eq!(
            rng_digest(&sim.scenario_rng) != before,
            dword(&row["selected"]) == 1,
            "team block row {number}: selector"
        );
    }
}

#[cfg(feature = "ares")]
#[test]
fn ares_trigger_side_tokens_admit_any_registered_side() {
    assert!(super::side_admits(0, 3), "token 0 is unrestricted");
    assert!(super::side_admits(4, 3), "token 4 selects side index 3");
    assert!(!super::side_admits(4, 2));
    assert!(!super::side_admits(1, 3));
    assert!(super::side_admits(3, 2));
}

fn open_ai_fixture() -> (Simulation, RuleSet, InternedId, InternedId, [InternedId; 3]) {
    let rules = RuleSet::from_ini(&IniFile::from_str(
        "[InfantryTypes]\n0=E1\n[E1]\nStrength=100\nTechLevel=1\n\
         [VehicleTypes]\n0=SHIP\n[SHIP]\nStrength=100\nNaval=yes\nTechLevel=1\n\
         [AircraftTypes]\n0=JET\n[JET]\nStrength=100\nTechLevel=1\n",
    ))
    .unwrap();
    let registry = crate::rules::team_ai_ini::TeamAiIniRegistry::from_sources(
        &IniFile::from_str(
            "[TeamTypes]\n0=GROUND\n1=SEA\n2=AIR\n\
             [GROUND]\nScript=S\nTaskForce=FG\n[SEA]\nScript=S\nTaskForce=FS\n[AIR]\nScript=S\nTaskForce=FA\n\
             [ScriptTypes]\n0=S\n[S]\n0=5,1\n[TaskForces]\n0=FG\n1=FS\n2=FA\n\
             [FG]\n0=1,E1\n[FS]\n0=1,SHIP\n[FA]\n0=1,JET\n",
        ),
        &IniFile::from_str(""),
        true,
    );
    let mut sim = Simulation::new();
    crate::sim::arena_fixture::flat_ground(&mut sim, &rules);
    sim.fog = crate::sim::vision::FogState::new(32, 32);
    sim.intern_rule_type_ids(&rules);
    sim.resolve_type_handles(&rules);
    sim.install_team_ai_registry(&registry, &rules).unwrap();
    let owner = sim.interner.intern("Computer");
    let enemy = sim.interner.intern("Enemy");
    for id in [owner, enemy] {
        sim.houses
            .register(HouseState::new(id, 0, None, false, 0, 10));
    }
    sim.houses
        .get_mut(&owner)
        .unwrap()
        .set_ai_strategy(crate::sim::house_state::AiStrategy::OpenAi);
    let team_types = ["GROUND", "SEA", "AIR"].map(|name| sim.interner.get(name).unwrap());
    for (index, &team_type) in team_types.iter().enumerate() {
        let id = sim.interner.intern(&format!("DOMAIN{index}"));
        sim.team_script_vm.register_ai_trigger(ai_trigger(
            id,
            team_type,
            None,
            NativeF64Bits::from_bits(100.0_f64.to_bits()),
        ));
    }
    (sim, rules, owner, enemy, team_types)
}

#[test]
fn open_ai_prefers_domains_not_already_fielded() {
    let (mut sim, rules, owner, enemy, teams) = open_ai_fixture();
    sim.team_script_vm
        .create_team_from_type(owner, teams[0], &[], 0);
    let selected = draw_ai_trigger(
        &mut sim,
        &rules,
        owner,
        Some(enemy),
        false,
        |_, _, _, _| true,
        |_, low, high| {
            assert_eq!((low, high), (1, 1000));
            201
        },
    );
    assert_eq!(selected, [teams[1]]);
}

#[test]
fn open_ai_does_not_spend_a_selection_on_an_already_forming_team() {
    let (mut sim, rules, owner, enemy, teams) = open_ai_fixture();
    sim.team_script_vm
        .construct_team(teams[0], owner, true, 0, false, None)
        .unwrap();
    let selected = draw_ai_trigger(
        &mut sim,
        &rules,
        owner,
        Some(enemy),
        false,
        |_, _, _, _| true,
        |_, low, high| {
            assert_eq!((low, high), (1, 800));
            1
        },
    );
    assert_eq!(selected, [teams[1]]);
}

#[test]
fn fair_ai_conditions_use_observations_and_reject_private_enemy_economy() {
    let (mut sim, rules, owner, enemy, teams) = open_ai_fixture();
    let unit = sim.spawn_object("E1", "Enemy", 12, 12, 0, &rules).unwrap();
    let mut trigger = ai_trigger(
        sim.interner.intern("OBSERVED"),
        teams[0],
        None,
        NativeF64Bits::ONE,
    );
    trigger.condition = 0;
    trigger.object_type = Some(crate::sim::team_script_vm::member_type_identity(
        sim.substrate.entities.get(unit).unwrap(),
    ));
    trigger.comparison_mask[..4].copy_from_slice(&1_i32.to_le_bytes());
    trigger.comparison_mask[4..8].copy_from_slice(&3_i32.to_le_bytes());
    assert!(!condition_holds(&sim, &rules, &trigger, owner, Some(enemy)));
    sim.fog.mark_visible_for_owner(owner, 12, 12);
    assert!(condition_holds(&sim, &rules, &trigger, owner, Some(enemy)));
    sim.substrate.entities.get_mut(unit).unwrap().cloak =
        Some(crate::sim::cloak_disguise::CloakRuntime::new(0));
    sim.substrate
        .entities
        .get_mut(unit)
        .unwrap()
        .cloak
        .as_mut()
        .unwrap()
        .set_state_for_test(2);
    assert!(!condition_holds(&sim, &rules, &trigger, owner, Some(enemy)));
    trigger.condition = 4;
    trigger.comparison_mask[..4].copy_from_slice(&0_i32.to_le_bytes());
    assert!(!condition_holds(&sim, &rules, &trigger, owner, Some(enemy)));
    sim.houses
        .get_mut(&owner)
        .unwrap()
        .set_ai_strategy(crate::sim::house_state::AiStrategy::Native);
    assert!(condition_holds(&sim, &rules, &trigger, owner, Some(enemy)));
}

#[test]
fn combined_arms_counts_retail_jumpjet_units_as_air_forces() {
    let Some(rules) = crate::rules::retail_ini_fixture::retail_rulesmd_rules() else {
        return;
    };
    let registry = crate::rules::team_ai_ini::TeamAiIniRegistry::from_sources(
        &IniFile::from_str(
            "[TeamTypes]\n0=KIROVS\n1=ROCKETEERS\n2=JETS\n\
             [KIROVS]\nScript=S\nTaskForce=FK\n[ROCKETEERS]\nScript=S\nTaskForce=FR\n[JETS]\nScript=S\nTaskForce=FJ\n\
             [ScriptTypes]\n0=S\n[S]\n0=0,1\n[TaskForces]\n0=FK\n1=FR\n2=FJ\n\
             [FK]\n0=1,ZEP\n[FR]\n0=1,JUMPJET\n[FJ]\n0=1,ORCA\n",
        ),
        &IniFile::from_str(""),
        true,
    );
    let mut sim = Simulation::new();
    sim.intern_rule_type_ids(&rules);
    sim.resolve_type_handles(&rules);
    sim.install_team_ai_registry(&registry, &rules).unwrap();
    for name in ["KIROVS", "ROCKETEERS", "JETS"] {
        let id = sim.interner.get(name).unwrap();
        assert_eq!(
            team_domains(&sim, &rules, id),
            [false, false, true],
            "{name}"
        );
    }
}
