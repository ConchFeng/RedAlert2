//! Read-only, bounded-rate standings for participating computer houses.

use std::collections::BTreeMap;

use crate::map::entities::EntityCategory;
use crate::rules::object_type::ObjectType;
use crate::rules::ruleset::RuleSet;
use crate::sim::game_entity::GameEntity;
use crate::sim::house_state::HouseState;
use crate::sim::intern::InternedId;
use crate::sim::world::Simulation;

const REFRESH_INTERVAL_MS: u64 = 500;

#[derive(Debug)]
pub(crate) struct StandingsRow {
    owner: InternedId,
    ordinal: usize,
    alive: bool,
    army_value: i64,
    buildings: u32,
    credits: i32,
    kills: u32,
}

impl StandingsRow {
    pub(crate) fn owner(&self) -> InternedId {
        self.owner
    }

    pub(crate) fn ordinal(&self) -> usize {
        self.ordinal
    }

    pub(crate) fn alive(&self) -> bool {
        self.alive
    }

    pub(crate) fn army_value(&self) -> i64 {
        self.army_value
    }

    pub(crate) fn buildings(&self) -> u32 {
        self.buildings
    }

    pub(crate) fn credits(&self) -> i32 {
        self.credits
    }

    pub(crate) fn kills(&self) -> u32 {
        self.kills
    }
}

#[derive(Default)]
pub(crate) struct StandingsProjection {
    rows: Vec<StandingsRow>,
    refreshed_ms: Option<u64>,
    requested_details: bool,
}

impl StandingsProjection {
    pub(crate) fn rows(&self) -> &[StandingsRow] {
        &self.rows
    }

    pub(crate) fn initial_count(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn alive_count(&self) -> usize {
        self.rows.iter().filter(|row| row.alive).count()
    }

    pub(crate) fn refresh(
        &mut self,
        sim: &Simulation,
        rules: &RuleSet,
        now_ms: u64,
        expanded: bool,
    ) {
        let opening = expanded && !self.requested_details;
        self.requested_details = expanded;
        if !opening
            && self
                .refreshed_ms
                .is_some_and(|last| now_ms.wrapping_sub(last) < REFRESH_INTERVAL_MS)
        {
            return;
        }
        self.refreshed_ms = Some(now_ms);
        self.rows = sim
            .houses()
            .in_order()
            .filter(|house| participating(sim, house))
            .enumerate()
            .map(|(index, house)| StandingsRow {
                owner: house.name(),
                ordinal: index + 1,
                alive: !house.is_defeated(),
                army_value: 0,
                buildings: 0,
                credits: house.economy().available_money(),
                kills: house.stats().kills(),
            })
            .collect();
        if !expanded {
            return;
        }
        let owner_rows: BTreeMap<_, _> = self
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| (row.owner, index))
            .collect();
        let mut costs = BTreeMap::new();
        for entity in sim.entities().values() {
            if !live_on_map(entity) {
                continue;
            }
            let Some(&index) = owner_rows.get(&entity.owner()) else {
                continue;
            };
            let row = &mut self.rows[index];
            if entity.category() == EntityCategory::Structure {
                row.buildings = row.buildings.saturating_add(1);
                continue;
            }
            let cost = *costs
                .entry((entity.owner(), entity.type_ref()))
                .or_insert_with(|| {
                    sim.interner()
                        .object_type(entity.type_ref(), rules)
                        .filter(|object| combat_type(object, rules))
                        .map_or(0, |object| {
                            i64::from(sim.cost_of(entity.owner(), object, rules).max(0))
                        })
                });
            row.army_value = row.army_value.saturating_add(cost);
        }
    }

    pub(crate) fn focus_target(&self, sim: &Simulation, owner: InternedId) -> Option<[i32; 3]> {
        if !self.rows.iter().any(|row| row.owner == owner)
            || !sim
                .houses()
                .get(&owner)
                .is_some_and(|house| participating(sim, house))
        {
            return None;
        }
        let target = sim
            .entities()
            .values()
            .filter(|entity| entity.owner() == owner && live_on_map(entity))
            .min_by_key(|entity| {
                (
                    entity.category() != EntityCategory::Structure,
                    entity.stable_id(),
                )
            })?;
        let point =
            crate::sim::movement::ground_pose::object_location(target, sim.resolved_terrain());
        Some([point.x, point.y, point.z])
    }
}

fn participating(sim: &Simulation, house: &HouseState) -> bool {
    !house.is_human()
        && !house.multiplay_passive()
        && !sim.session().is_observer_house(house.name())
}

fn live_on_map(entity: &GameEntity) -> bool {
    entity.is_active() && entity.health.current() > 0 && !entity.in_limbo()
}

/// Presentation metric: current replacement cost of armed scoring unit types,
/// without health scaling. Native force-value counters have different retained
/// add/remove semantics, so those counters cannot answer this current-cost query.
/// Nominal civilians, insignificant/unscored missiles, unarmed resource units,
/// and heal-only support have no contribution. Armed harvesters remain eligible.
fn combat_type(object: &ObjectType, rules: &RuleSet) -> bool {
    !object.nominal()
        && !object.insignificant
        && !object.dont_score
        && (0..object.weapon_record_count()).any(|index| {
            [object.weapon_at(index), object.elite_weapon_at(index)]
                .into_iter()
                .flatten()
                .filter_map(|name| rules.weapon(name))
                .any(|weapon| {
                    weapon.damage > 0
                        || weapon
                            .warhead
                            .as_deref()
                            .and_then(|name| rules.warhead(name))
                            .is_some_and(|warhead| {
                                warhead.temporal || warhead.mind_control || warhead.parasite
                            })
                })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::combat::Health;
    use crate::sim::house_state::MatchStatistics;

    fn house(sim: &mut Simulation, name: &str, rules: &RuleSet) -> InternedId {
        let owner = sim.intern(name);
        let country = sim.intern("Americans");
        let mut house = HouseState::new(owner, 0, Some(country), false, 2_000, 10);
        house.project_country_mults(rules, sim.interner());
        sim.register_house(house);
        owner
    }

    fn entity(sim: &mut Simulation, owner: InternedId, name: &str, id: u64, cell: (u16, u16)) {
        let type_id = sim.intern(name);
        let category = if name == "GAPOWR" {
            EntityCategory::Structure
        } else {
            EntityCategory::Unit
        };
        let mut entity = GameEntity::new_at_frame_zero_for_test(
            id,
            cell.0,
            cell.1,
            0,
            0,
            owner,
            Health::new(100),
            type_id,
            category,
            0,
            5,
            false,
        );
        entity.lifecycle.set_in_limbo_for_test(false);
        sim.entities_mut_for_test().insert(entity);
    }

    #[test]
    fn twenty_nine_ai_rows_keep_defeated_entrants_and_throttle_closed_updates() {
        let retail = crate::rules::retail_ini_fixture::retail_battle_rules_for_map("XMP03T4.MAP")
            .expect("retail rules");
        let rules = &retail.rules;
        let mut sim = Simulation::with_seed(1);
        let human = house(&mut sim, "Observer", rules);
        sim.houses_mut_for_test()
            .get_mut(&human)
            .unwrap()
            .set_is_human_for_test(true);
        let passive = house(&mut sim, "Neutral", rules);
        sim.houses_mut_for_test()
            .get_mut(&passive)
            .unwrap()
            .set_multiplay_passive_for_test(true);
        let entrants: Vec<_> = (1..=29)
            .map(|index| house(&mut sim, &format!("Computer{index}"), rules))
            .collect();
        let last = *entrants.last().unwrap();
        sim.houses_mut_for_test()
            .get_mut(&last)
            .unwrap()
            .set_is_defeated_for_test(true);
        let mut projection = StandingsProjection::default();
        let hash = sim.state_hash();
        projection.refresh(&sim, rules, 100, false);
        assert_eq!(sim.state_hash(), hash);
        assert_eq!(
            (projection.initial_count(), projection.alive_count()),
            (29, 28)
        );
        assert_eq!(
            projection
                .rows()
                .iter()
                .map(StandingsRow::owner)
                .collect::<Vec<_>>(),
            entrants
        );
        let first = entrants[0];
        let house = sim.houses_mut_for_test().get_mut(&first).unwrap();
        house.economy_mut_for_test().set_credits_for_test(7_500);
        house.set_stats_for_test(MatchStatistics::from_totals_for_test(3, 2, 0, 0, 0));
        projection.refresh(&sim, rules, 599, false);
        assert_eq!(projection.rows()[0].credits(), 2_000);
        projection.refresh(&sim, rules, 600, false);
        assert_eq!(projection.rows()[0].credits(), 7_500);
        assert_eq!(projection.rows()[0].kills(), 5);
        assert_eq!(projection.rows()[28].ordinal(), 29);
        assert!(!projection.rows()[28].alive());
        let hash = sim.state_hash();
        projection.refresh(&sim, rules, 0, false);
        assert_eq!(sim.state_hash(), hash);
        projection = StandingsProjection::default();
        assert!(projection.rows().is_empty());
    }

    #[test]
    fn opening_refresh_prices_live_combat_units_through_retail_cost_owner() {
        let retail = crate::rules::retail_ini_fixture::retail_battle_rules_for_map("XMP03T4.MAP")
            .expect("retail rules");
        let rules = &retail.rules;
        let mut sim = Simulation::with_seed(2);
        let owner = house(&mut sim, "Computer1", rules);
        for (id, name) in [
            (1, "MTNK"),
            (2, "MTNK"),
            (3, "CMIN"),
            (4, "CIVA"),
            (5, "V3ROCKET"),
            (6, "GAPOWR"),
            (7, "MTNK"),
            (8, "MTNK"),
        ] {
            entity(&mut sim, owner, name, id, (20 + id as u16, 20));
        }
        sim.entities_mut_for_test()
            .get_mut(2)
            .unwrap()
            .health
            .set_for_test(50);
        sim.entities_mut_for_test()
            .get_mut(7)
            .unwrap()
            .health
            .set_for_test(0);
        sim.entities_mut_for_test()
            .get_mut(8)
            .unwrap()
            .lifecycle
            .set_in_limbo_for_test(true);
        let mut projection = StandingsProjection::default();
        projection.refresh(&sim, rules, 100, false);
        assert_eq!(projection.rows()[0].army_value(), 0);
        let hash = sim.state_hash();
        projection.refresh(&sim, rules, 101, true);
        let row = &projection.rows()[0];
        assert_eq!(
            row.army_value(),
            2 * i64::from(sim.cost_of(owner, rules.object("MTNK").unwrap(), rules))
        );
        assert_eq!(row.buildings(), 1);
        assert_eq!(sim.state_hash(), hash);
    }

    #[test]
    fn focusing_resolves_live_objects_and_falls_back_without_retained_pointers() {
        let retail = crate::rules::retail_ini_fixture::retail_battle_rules_for_map("XMP03T4.MAP")
            .expect("retail rules");
        let rules = &retail.rules;
        let mut sim = Simulation::with_seed(3);
        let owner = house(&mut sim, "Computer1", rules);
        entity(&mut sim, owner, "MTNK", 1, (20, 20));
        entity(&mut sim, owner, "GAPOWR", 2, (40, 40));
        let mut projection = StandingsProjection::default();
        projection.refresh(&sim, rules, 100, true);
        let expected = |sim: &Simulation, id| {
            let point = crate::sim::movement::ground_pose::object_location(
                sim.entities().get(id).unwrap(),
                sim.resolved_terrain(),
            );
            [point.x, point.y, point.z]
        };
        let hash = sim.state_hash();
        assert_eq!(
            projection.focus_target(&sim, owner),
            Some(expected(&sim, 2))
        );
        assert_eq!(sim.state_hash(), hash);
        sim.houses_mut_for_test()
            .get_mut(&owner)
            .unwrap()
            .set_is_defeated_for_test(true);
        sim.entities_mut_for_test().remove(2);
        assert_eq!(
            projection.focus_target(&sim, owner),
            Some(expected(&sim, 1))
        );
        sim.entities_mut_for_test()
            .get_mut(1)
            .unwrap()
            .health
            .set_for_test(0);
        assert!(projection.focus_target(&sim, owner).is_none());
        let other = sim.intern("Missing");
        assert!(projection.focus_target(&sim, other).is_none());
    }
}
