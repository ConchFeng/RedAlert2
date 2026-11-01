//! Fair skirmish strategy extensions: Claude AI and Open AI.
//!
//! Both keep Hard's AI tables and ordinary production, teams and scripts,
//! while using the player's Normal stat row, opening credits, prerequisite
//! checks and build limits. Neither receives MultiplayerAICM, virtual
//! purifiers or CompEasyBonus advantages. Live enemy targeting requires
//! current sight through [`target_cell_known`] and the shared Greatest_Threat
//! candidate evaluator; explored terrain alone conveys no current object state.
//!
//! The shared macro owners fund the first refinery before combat factories,
//! then bring Battle Lab prerequisites forward (`base_plan_generation`).
//! `ai_base_building` inserts affordable extra refineries toward known nearby
//! resources while retaining a cash reserve; `ai_base_site` ranks legal sites
//! toward those resources. [`team_delay`] increases attack opportunities as
//! available credits grow. Open AI additionally favors underrepresented land,
//! naval and air task forces through `ai_team_creation`, without bypassing
//! normal factory, prerequisite, cost or terrain admission.
//!
//! Expansion currently extends a legal base perimeter with refineries; remote
//! MCV outposts and coordinated transport landings remain separate mechanisms.

use crate::rules::object_type::ObjectType;
use crate::rules::ruleset::RuleSet;
use crate::sim::intern::InternedId;
use crate::sim::vision::FogState;

/// Banks at or above these credits restart the team timer at the paired
/// percent of the native delay; below the first, the delay is native.
const RICH_TEAM_DELAY_PERCENT: [(i32, u16); 3] = [(10_000, 25), (5_000, 40), (2_500, 65)];

/// The team-timer delay of a Claude house holding `credits`.
pub(crate) fn team_delay(native_delay: i32, credits: i32) -> i32 {
    RICH_TEAM_DELAY_PERCENT
        .iter()
        .find(|&&(threshold, _)| credits >= threshold)
        .map_or(native_delay, |&(_, percent)| {
            crate::rules::personality::scale_team_delay(native_delay, percent)
        })
}

/// Whether a fair strategy house may inspect a live target at this cell.
/// Current sight is required even for structures: explored terrain does not
/// reveal later construction, destruction or replacement. An allied view counts.
pub(crate) fn target_cell_known(fog: &FogState, owner: InternedId, (rx, ry): (i16, i16)) -> bool {
    let (Ok(rx), Ok(ry)) = (u16::try_from(rx), u16::try_from(ry)) else {
        return false;
    };
    fog.is_cell_visible(owner, rx, ry)
}

/// Reorder a native base plan's tail, `priority[seeds..]`, so the first
/// `BuildRefinery=` entry and then the first `BuildTech=` entry come first,
/// each preceded by the entries it needs. Every other entry keeps its native
/// relative order, and an entry moves only once the ones before it satisfy
/// its prerequisites, so the plan stays buildable in order. A target the
/// tail cannot reach leaves the rest unchanged.
pub(crate) fn pull_tech_forward(priority: &mut Vec<&ObjectType>, seeds: usize, rules: &RuleSet) {
    use crate::sim::base_plan_generation::prerequisites_satisfied;

    let seeds = seeds.min(priority.len());
    let mut tail: Vec<&ObjectType> = priority.split_off(seeds);
    let in_family = |ty: &ObjectType, family: &[String]| {
        family.iter().any(|id| id.eq_ignore_ascii_case(&ty.id))
    };
    for family in [&rules.build_refinery_types, &rules.build_tech_types] {
        let Some(goal) = tail.iter().position(|ty| in_family(ty, family)) else {
            continue;
        };
        let goal = tail[goal];
        while !prerequisites_satisfied(goal, priority, rules) {
            // The first entry that is buildable now and that `goal` needs,
            // directly or through another entry still in the tail.
            let Some(next) = tail.iter().position(|&candidate| {
                prerequisites_satisfied(candidate, priority, rules)
                    && needed_for(candidate, goal, &tail, priority, rules, PULL_DEPTH)
            }) else {
                break;
            };
            priority.push(tail.remove(next));
        }
        if prerequisites_satisfied(goal, priority, rules)
            && let Some(index) = tail.iter().position(|&ty| std::ptr::eq(ty, goal))
        {
            priority.push(tail.remove(index));
        }
    }
    priority.append(&mut tail);
}

/// How many prerequisite levels [`pull_tech_forward`] looks through; retail
/// tech buildings sit two levels above the native seeds.
const PULL_DEPTH: u32 = 4;

/// Whether adding `candidate` satisfies a prerequisite `goal` still lacks
/// against `present`, or one of a tail entry that does so, `depth` levels down.
fn needed_for(
    candidate: &ObjectType,
    goal: &ObjectType,
    tail: &[&ObjectType],
    present: &[&ObjectType],
    rules: &RuleSet,
    depth: u32,
) -> bool {
    use crate::sim::base_plan_generation::prerequisite_satisfied;

    goal.prerequisite.iter().any(|entry| {
        if prerequisite_satisfied(entry, present, rules) {
            return false;
        }
        if prerequisite_satisfied(entry, &[candidate], rules) {
            return true;
        }
        depth > 0
            && tail.iter().any(|&provider| {
                !std::ptr::eq(provider, candidate)
                    && prerequisite_satisfied(entry, &[provider], rules)
                    && needed_for(candidate, provider, tail, present, rules, depth - 1)
            })
    })
}

#[cfg(test)]
mod tests {
    use super::team_delay;
    use crate::rules::object_type::{ObjectCategory, ObjectType};
    use crate::rules::retail_ini_fixture::retail_rulesmd_rules;
    use crate::rules::ruleset::RuleSet;
    use crate::sim::base_plan_generation::{prerequisites_satisfied, recalc_base_plan};
    use crate::sim::house_state::{AiStrategy, HouseDifficulty, HouseState};

    #[test]
    fn a_richer_bank_restarts_the_team_timer_sooner() {
        assert_eq!(team_delay(2000, 2_499), 2000);
        assert_eq!(team_delay(2000, 2_500), 1300);
        assert_eq!(team_delay(2000, 10_000), 500);
    }

    #[test]
    fn explored_terrain_does_not_reveal_live_buildings() {
        let owner = crate::sim::intern::test_intern("Americans");
        let mut fog = crate::sim::vision::FogState::new(20, 20);
        fog.reveal_all_for_owner(owner);
        assert!(fog.is_cell_revealed(owner, 10, 10));
        assert!(!super::target_cell_known(&fog, owner, (10, 10)));
        fog.mark_visible_for_owner(owner, 10, 10);
        assert!(super::target_cell_known(&fog, owner, (10, 10)));
    }

    /// Retail `[Easy]`, the row a Hard computer plays, has ROF=.8; a Claude
    /// house keeps Hard's AI tables but plays the human's `[Normal]` row.
    #[test]
    fn fair_ai_plays_the_normal_stat_row_at_hard_tables() {
        let Some(rules) = retail_rulesmd_rules() else {
            return;
        };
        let house = |strategy, difficulty| {
            let mut house = HouseState::new(Default::default(), 0, None, false, 0, 10);
            house.set_ai_strategy(strategy);
            house.set_difficulty(
                difficulty,
                &rules.general,
                rules.country_difficulty_biases("Americans"),
                true,
                0,
                0,
            );
            house
        };
        let normal = house(AiStrategy::Native, HouseDifficulty::Normal).rof_bias();
        assert_ne!(
            house(AiStrategy::Native, HouseDifficulty::Hard).rof_bias(),
            normal
        );
        for strategy in [AiStrategy::Claude, AiStrategy::OpenAi] {
            let fair = house(strategy, HouseDifficulty::Hard);
            assert_eq!(fair.rof_bias(), normal);
            assert_eq!(fair.difficulty(), HouseDifficulty::Hard);
        }
    }

    /// Retail `MultiplayerAICM=400,0,0` quintuples a Hard computer's bank at
    /// the start; a Claude house keeps the credits a human starts with.
    #[test]
    fn fair_ai_gets_no_opening_credit_grant() {
        let Some(rules) = retail_rulesmd_rules() else {
            return;
        };
        let mut houses = crate::sim::houses::Houses::default();
        let mut interner = crate::sim::intern::StringInterner::default();
        for (name, strategy) in [
            ("Native", AiStrategy::Native),
            ("Claude", AiStrategy::Claude),
            ("OpenAi", AiStrategy::OpenAi),
        ] {
            let mut house = HouseState::new(interner.intern(name), 0, None, false, 0, 10);
            house.set_ai_strategy(strategy);
            house.set_difficulty_for_test(HouseDifficulty::Hard);
            house.economy_mut().add_credits(10_000);
            houses.register(house);
        }
        crate::sim::scenario_bootstrap::apply_skirmish_ai_opening_credits(&mut houses, &rules);
        let (native, claude) = (interner.intern("Native"), interner.intern("Claude"));
        assert!(houses[&native].economy().available_money() > 10_000);
        assert_eq!(houses[&claude].economy().available_money(), 10_000);
        assert_eq!(
            houses[&interner.intern("OpenAi")]
                .economy()
                .available_money(),
            10_000
        );
    }

    /// The retail Allied plan: a Claude house plans the Battle Lab sooner
    /// than the native order, every entry still follows its prerequisites,
    /// and the plan holds the same entries from the same Scenario draws.
    #[test]
    fn a_claude_plan_pulls_the_battle_lab_forward_and_stays_buildable() {
        let Some(rules) = retail_rulesmd_rules() else {
            return;
        };
        let plan = |claude_ai| {
            let mut plan = crate::sim::base_plan::BasePlanState::default();
            let mut rng = crate::sim::rng::SimRng::new(7);
            recalc_base_plan(
                &mut plan,
                &rules,
                &crate::sim::world::TypeTechLevels::default(),
                "Americans",
                0,
                HouseDifficulty::Hard,
                claude_ai,
                10,
                true,
                &mut rng,
            );
            let values: Vec<i32> = plan
                .nodes()
                .iter()
                .map(|node| node.type_or_control)
                .collect();
            (values, rng.logical_state())
        };
        let (native, native_rng) = plan(false);
        let (claude, claude_rng) = plan(true);
        assert_eq!(claude_rng, native_rng);
        let mut sorted = (native.clone(), claude.clone());
        sorted.0.sort_unstable();
        sorted.1.sort_unstable();
        assert_eq!(sorted.0, sorted.1);

        let building = |value: i32| -> &ObjectType {
            rules
                .building_ids
                .iter()
                .filter_map(|id| rules.object_in_category(ObjectCategory::Building, id))
                .find(|ty| ty.base_plan_type_index == value)
                .expect("planned building type")
        };
        let lab = |values: &[i32]| {
            values
                .iter()
                .position(|&value| value >= 0 && building(value).id == "GATECH")
                .expect("the Allied plan holds the Battle Lab")
        };
        assert!(lab(&claude) < lab(&native), "{claude:?} vs {native:?}");
        assert_buildable_in_order(&claude, &rules, building);
    }

    fn assert_buildable_in_order<'a>(
        values: &[i32],
        rules: &RuleSet,
        building: impl Fn(i32) -> &'a ObjectType,
    ) {
        let mut present: Vec<&ObjectType> = Vec::new();
        for &value in values.iter().filter(|&&value| value >= 0) {
            let ty = building(value);
            assert!(
                present.len() < 4 || prerequisites_satisfied(ty, &present, rules),
                "{} precedes a prerequisite",
                ty.id
            );
            present.push(ty);
        }
    }
}
