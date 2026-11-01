//! Fair computer attack teams explore and patrol with their own sight before acquiring a quarry.

use super::*;
use crate::rules::locomotor_type::SpeedType;
use crate::sim::find_nearby_cell::PassabilityArgs;

impl Simulation {
    pub(super) fn team_fair_attack_or_scout(
        &mut self,
        team_id: u64,
        quarry: i32,
        rules: &RuleSet,
        registry: Option<&OverlayTypeRegistry>,
        effects: FrameEffects<'_>,
    ) -> bool {
        let Some(leader) = self.team_leader(team_id, rules) else {
            return false;
        };
        let Some(actor) = self.substrate.entities.get(leader) else {
            return false;
        };
        let owner = actor.owner();
        if !self.houses.get(&owner).is_some_and(|house| house.fair_ai())
            || self.team_out_of_ammo(team_id, rules)
        {
            return false;
        }
        if let Some(target) = self.team_quarry_threat(team_id, leader, quarry, rules, registry) {
            self.team_assign_mission_target(
                team_id,
                Some(TeamTarget::Object(target)),
                rules,
                registry,
                effects,
            );
            self.team_coordinate_attack(team_id, rules, registry, effects);
            return true;
        }
        let current = self
            .team_script_vm
            .team(team_id)
            .and_then(|team| team.mission_target);
        let target = current
            .filter(|target| match target {
                TeamTarget::Cell { x, y } => !self.fog.is_cell_visible(owner, *x as u16, *y as u16),
                _ => false,
            })
            .or_else(|| {
                let actor = self.substrate.entities.get(leader)?;
                let object = self.entity_object_type(actor, rules)?;
                let zone = (object.speed_type != SpeedType::Winged)
                    .then(|| {
                        self.zone_grid.as_ref()?.get_zone_id_native(
                            self.resolved_terrain.as_ref()?,
                            (actor.position.rx(), actor.position.ry()),
                            object.movement_zone,
                            actor.on_bridge(),
                        )
                    })
                    .flatten();
                let args = PassabilityArgs {
                    speed_type: object.speed_type,
                    required_zone_id: zone,
                    movement_zone: object.movement_zone,
                    bridge_aware_zone: actor.on_bridge(),
                };
                self.nearby_exploration_cell(
                    crate::sim::combat::TargetKind::Entity(leader),
                    owner,
                    Some(args),
                )
                .map(|target| TeamTarget::from(crate::sim::combat::TargetKind::from(target)))
            });
        let Some(target) = target else {
            self.team_assign_mission_target(team_id, None, rules, registry, effects);
            return false;
        };
        self.team_assign_mission_target(team_id, Some(target), rules, registry, effects);
        self.team_coordinate_move(team_id, rules, registry, effects);
        if let Some(team) = self.team_script_vm.teams.get_mut(&team_id) {
            team.advance_pending = false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::house_state::AiStrategy;
    use crate::sim::team_script_vm::packed_cell_tests::{frame, scene};

    #[test]
    fn fair_attack_team_explores_its_own_shroud_then_acquires_a_visible_enemy() {
        let Some((mut scene, team, member)) = scene("0=0,1", "E2", true, |scene| {
            let sim = &mut scene.runtime.simulation;
            let owner = Simulation::scenario_house_named(&sim.houses, "Computer").unwrap();
            sim.houses
                .get_mut(&owner)
                .unwrap()
                .set_ai_strategy(AiStrategy::OpenAi);
            let viewer = sim.session.current_house().unwrap();
            assert_ne!(owner, viewer);
            sim.fog.reveal_all_for_owner(viewer);
        }) else {
            return;
        };
        let sim = scene.sim();
        let state = sim.team_script_vm.team(team).unwrap();
        let owner = state.owner();
        let Some(TeamTarget::Cell { x, y }) = state.mission_target else {
            panic!("an attack team without observed enemies must explore");
        };
        assert!(!sim.fog.is_cell_revealed(owner, x as u16, y as u16));
        assert!(!state.advance_pending());
        assert!(
            sim.entities()
                .get(member)
                .unwrap()
                .navigation
                .nav_com()
                .is_some()
        );
        let start = sim
            .entities()
            .get(member)
            .map(|actor| (actor.position.rx(), actor.position.ry()))
            .unwrap();
        for _ in 0..60 {
            frame(&mut scene);
        }
        let sim = &mut scene.runtime.simulation;
        let rules = &scene.runtime.resources.rules;
        let actor = sim.entities().get(member).unwrap();
        assert_ne!(
            (actor.position.rx(), actor.position.ry()),
            start,
            "ordinary movement must carry the scout"
        );
        let cell = (actor.position.rx() + 2, actor.position.ry());
        sim.fog.reveal_all_for_owner(owner);
        sim.team_assign_mission_target(team, None, rules, None, FrameEffects::default());
        sim.team_action_attack_quarry(team, 1, rules, None, FrameEffects::default());
        let state = sim.team_script_vm.team(team).unwrap();
        let Some(TeamTarget::Cell { x, y }) = state.mission_target else {
            panic!("a fully explored map still needs patrols through fog");
        };
        assert!(sim.fog.is_cell_revealed(owner, x as u16, y as u16));
        assert!(!sim.fog.is_cell_visible(owner, x as u16, y as u16));
        assert!(!state.advance_pending());
        let enemy_owner = sim
            .interner
            .resolve(sim.session.current_house().unwrap())
            .to_owned();
        let enemy = sim
            .spawn_object("E1", &enemy_owner, cell.0, cell.1, 0, rules)
            .unwrap();
        sim.fog.mark_visible_for_owner(owner, cell.0, cell.1);
        sim.team_action_attack_quarry(team, 1, rules, None, FrameEffects::default());
        assert_eq!(
            sim.team_script_vm.team(team).unwrap().mission_target,
            Some(TeamTarget::Object(enemy))
        );
    }
}
