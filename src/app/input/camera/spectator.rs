//! Optional presentation-only direction of offline AI battles.

use crate::app::match_runtime::frame_pacer::wall_clock_ms;
use crate::app::match_runtime::session::SessionMode;
use crate::app::{AppState, GameScreen};
use crate::map::entities::EntityCategory;
use crate::rules::ruleset::RuleSet;
use crate::rules::superweapon_type::SuperWeaponKind;
use crate::sim::combat::TargetKind;
use crate::sim::world::{SimFireEvent, SimSoundEvent, Simulation};

const EVENT_LIFETIME_MS: u64 = 8_000;
const VIEW_HOLD_MS: u64 = 6_000;
const MANUAL_HOLD_MS: u64 = 10_000;
const MAX_ENCOUNTERS: usize = 64;
const CLUSTER_RADIUS: f32 = 240.0;

fn clock_elapsed(now_ms: u64, since_ms: u64) -> u64 {
    if now_ms < since_ms {
        u64::from((now_ms as u32).wrapping_sub(since_ms as u32))
    } else {
        now_ms - since_ms
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ActionKind {
    Fighting,
    BaseAttack,
    Superweapon,
}

#[derive(Clone, Copy)]
struct Encounter {
    id: u64,
    point: (f32, f32),
    kind: ActionKind,
    weight: u32,
    seen_ms: u64,
}

#[derive(Default)]
pub(crate) struct SpectatorCamera {
    enabled: bool,
    encounters: Vec<Encounter>,
    next_id: u64,
    focused: Option<(u64, u64)>,
    manual_started_ms: Option<u64>,
    last_step_ms: Option<u64>,
}

impl SpectatorCamera {
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn toggle(&mut self, now_ms: u64) {
        self.enabled = !self.enabled;
        self.resume(now_ms);
        if !self.enabled {
            self.encounters.clear();
        }
    }

    pub(crate) fn resume(&mut self, now_ms: u64) {
        self.manual_started_ms = None;
        self.focused = None;
        self.last_step_ms = Some(now_ms);
    }

    pub(crate) fn manual_remaining(&self, now_ms: u64) -> u64 {
        self.manual_started_ms.map_or(0, |start| {
            MANUAL_HOLD_MS
                .saturating_sub(clock_elapsed(now_ms, start))
                .div_ceil(1_000)
        })
    }

    pub(crate) fn manual_navigation(&mut self, now_ms: u64) {
        if self.enabled {
            self.manual_started_ms = Some(now_ms);
            self.focused = None;
            self.last_step_ms = None;
        }
    }

    pub(crate) fn focus_point(&self, now_ms: u64) -> Option<(f32, f32)> {
        let (id, _) = self.focused?;
        self.encounters
            .iter()
            .find(|event| {
                event.id == id && clock_elapsed(now_ms, event.seen_ms) < EVENT_LIFETIME_MS
            })
            .map(|event| event.point)
    }

    pub(crate) fn status(&self, now_ms: u64) -> String {
        let remaining = self.manual_remaining(now_ms);
        if remaining > 0 {
            return format!("Manual control - resume in {remaining}s");
        }
        let kind = self.focused.and_then(|(id, _)| {
            self.encounters
                .iter()
                .find(|event| {
                    event.id == id && clock_elapsed(now_ms, event.seen_ms) < EVENT_LIFETIME_MS
                })
                .map(|event| event.kind)
        });
        match kind {
            Some(ActionKind::Fighting) => "Auto Camera: On - Heavy fighting",
            Some(ActionKind::BaseAttack) => "Auto Camera: On - Base under attack",
            Some(ActionKind::Superweapon) => "Auto Camera: On - Superweapon launched",
            None => "Auto Camera: On - Waiting for action",
        }
        .into()
    }

    fn prune(&mut self, now_ms: u64) {
        self.encounters
            .retain(|event| clock_elapsed(now_ms, event.seen_ms) < EVENT_LIFETIME_MS);
    }

    fn record(&mut self, point: (f32, f32), kind: ActionKind, weight: u32, now_ms: u64) {
        self.prune(now_ms);
        if let Some(event) = self.encounters.iter_mut().find(|event| {
            event.kind == kind
                && (event.point.0 - point.0).abs() <= CLUSTER_RADIUS
                && (event.point.1 - point.1).abs() <= CLUSTER_RADIUS
        }) {
            event.point = (
                (event.point.0 + point.0) * 0.5,
                (event.point.1 + point.1) * 0.5,
            );
            event.weight = event.weight.saturating_add(weight).min(10_000);
            event.seen_ms = now_ms;
            return;
        }
        if self.encounters.len() == MAX_ENCOUNTERS {
            let oldest = self
                .encounters
                .iter()
                .enumerate()
                .max_by_key(|(_, event)| clock_elapsed(now_ms, event.seen_ms))
                .unwrap()
                .0;
            self.encounters.swap_remove(oldest);
        }
        self.next_id = self.next_id.wrapping_add(1);
        self.encounters.push(Encounter {
            id: self.next_id,
            point,
            kind,
            weight,
            seen_ms: now_ms,
        });
    }

    pub(crate) fn observe_frame(
        &mut self,
        sim: &Simulation,
        rules: &RuleSet,
        shots: &[SimFireEvent],
        sounds: &[SimSoundEvent],
        now_ms: u64,
    ) {
        if !self.enabled {
            return;
        }
        self.prune(now_ms);
        for shot in shots {
            if !rules
                .weapon(sim.interner().resolve(shot.weapon_id))
                .is_some_and(|weapon| weapon.damage > 0)
            {
                continue;
            }
            let TargetKind::Entity(target_id) = shot.target else {
                continue;
            };
            let Some(attacker) = sim.entities().get(shot.attacker_id) else {
                continue;
            };
            let Some(target) = sim.entities().get(target_id) else {
                continue;
            };
            let source = sim.interner().resolve(attacker.owner());
            let receiver = sim.interner().resolve(target.owner());
            if attacker.owner() == target.owner()
                || target.in_limbo()
                || crate::map::houses::is_allied_with(sim.house_alliances(), source, receiver)
                || crate::map::houses::is_allied_with(sim.house_alliances(), receiver, source)
                || sim
                    .houses()
                    .get(&target.owner())
                    .is_none_or(|house| house.multiplay_passive())
            {
                continue;
            }
            let coord =
                crate::sim::movement::ground_pose::object_location(target, sim.resolved_terrain());
            let point = crate::util::lepton::absolute_leptons_to_screen(coord.x, coord.y, coord.z);
            let kind = if target.category() == EntityCategory::Structure {
                ActionKind::BaseAttack
            } else {
                ActionKind::Fighting
            };
            self.record(point, kind, 1, now_ms);
        }
        for event in sounds {
            if let SimSoundEvent::SuperWeaponLaunched { sw_type, world, .. } = event
                && rules
                    .super_weapon(sim.interner().resolve(*sw_type))
                    .is_some_and(|weapon| {
                        matches!(
                            weapon.kind,
                            SuperWeaponKind::MultiMissile
                                | SuperWeaponKind::LightningStorm
                                | SuperWeaponKind::PsychicDominator
                                | SuperWeaponKind::GeneticConverter
                                | SuperWeaponKind::GenericWarhead
                                | SuperWeaponKind::HunterSeeker
                        )
                    })
            {
                let point =
                    crate::util::lepton::absolute_leptons_to_screen(world.x, world.y, world.z);
                self.record(point, ActionKind::Superweapon, 100, now_ms);
            }
        }
    }

    fn target(&mut self, now_ms: u64) -> Option<(f32, f32)> {
        self.prune(now_ms);
        if !self.enabled || self.manual_remaining(now_ms) > 0 {
            return None;
        }
        let best = self
            .encounters
            .iter()
            .max_by_key(|event| {
                (
                    event.kind,
                    event.weight,
                    std::cmp::Reverse(clock_elapsed(now_ms, event.seen_ms)),
                    std::cmp::Reverse(event.id),
                )
            })
            .copied()?;
        let selected = if let Some((id, since)) = self.focused {
            let previous = self.encounters.iter().find(|event| event.id == id).copied();
            if clock_elapsed(now_ms, since) < VIEW_HOLD_MS && best.kind != ActionKind::Superweapon {
                previous.unwrap_or(best)
            } else {
                best
            }
        } else {
            best
        };
        if self.focused.is_none_or(|(id, _)| id != selected.id) {
            self.focused = Some((selected.id, now_ms));
        }
        Some(selected.point)
    }
}

pub(crate) fn available(state: &AppState) -> bool {
    state.frontend.screen == GameScreen::InGame
        && state.match_state.session_mode() == SessionMode::Skirmish
        && state.match_state.live_network.is_none()
        && !state.frontend.external_launch
        && !state.match_state.paused()
        && !state.match_state.main_tick_frozen()
        && state.platform.window_active
        && !state.focus_freeze_active()
        && !state.match_state.match_diagnostics.is_replay_playback()
        && !crate::app::match_runtime::post_match::viewing_battlefield(state)
        && state
            .match_state
            .sim_runtime
            .as_ref()
            .is_some_and(|runtime| {
                let sim = &runtime.simulation;
                sim.session().current_house().is_some_and(|owner| {
                    sim.session().is_observer_house(owner) || sim.can_continue_watching(owner)
                })
            })
}

pub(crate) fn manual_navigation(state: &mut AppState) {
    state
        .match_state
        .input
        .spectator_camera
        .manual_navigation(wall_clock_ms());
}

pub(crate) fn update(state: &mut AppState) {
    if !available(state) || !state.match_state.input.spectator_camera.enabled() {
        state.match_state.input.spectator_camera.last_step_ms = None;
        return;
    }
    if state.match_state.input.follow_target.is_some()
        || state
            .match_state
            .input
            .pending_camera_scroll
            .blocks_tactical_tooltip()
    {
        manual_navigation(state);
        return;
    }
    let now = wall_clock_ms();
    let director = &mut state.match_state.input.spectator_camera;
    let elapsed = director
        .last_step_ms
        .replace(now)
        .map_or(16, |last| clock_elapsed(now, last).min(100));
    let Some(target) = director.target(now) else {
        return;
    };
    let (width, height) = super::tactical_viewport_size(state);
    let target = super::tactical_camera_top_left(
        target,
        width as f32,
        height as f32,
        state.match_state.input.zoom_level,
    );
    let amount = 1.0 - (-(elapsed as f32) / 400.0).exp();
    let input = &mut state.match_state.input;
    let point = (
        input.camera_x + (target.0 - input.camera_x) * amount,
        input.camera_y + (target.1 - input.camera_y) * amount,
    );
    super::set_camera_position(input, point);
    super::clamp_camera_to_playable_area(
        state,
        state.render_width() as f32,
        state.render_height() as f32,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_navigation_defers_and_explicit_resume_restores_direction() {
        let mut camera = SpectatorCamera::default();
        camera.record((20.0, 40.0), ActionKind::Fighting, 1, 0);
        assert!(camera.target(0).is_none());
        camera.toggle(0);
        assert_eq!(camera.target(0), Some((20.0, 40.0)));
        camera.manual_navigation(100);
        assert_eq!(camera.manual_remaining(101), 10);
        assert!(camera.target(101).is_none());
        camera.manual_navigation(200);
        assert_eq!(camera.manual_remaining(10_199), 1);
        camera.record((40.0, 80.0), ActionKind::BaseAttack, 1, 10_199);
        assert!(camera.target(10_199).is_none());
        camera.resume(10_199);
        assert_eq!(camera.manual_remaining(10_199), 0);
        assert_eq!(camera.target(10_199), Some((40.0, 80.0)));
        camera.toggle(10_200);
        assert!(camera.target(10_200).is_none());
        assert!(camera.encounters.is_empty());
        camera.toggle(u64::from(u32::MAX) - 100);
        camera.manual_navigation(u64::from(u32::MAX) - 100);
        assert_eq!(camera.manual_remaining(99), 10);
        assert_eq!(camera.manual_remaining(10_000), 0);
    }

    #[test]
    fn encounters_hold_the_view_but_real_superweapons_interrupt_and_expire() {
        let mut camera = SpectatorCamera::default();
        camera.toggle(0);
        camera.record((0.0, 0.0), ActionKind::Fighting, 1, 0);
        assert_eq!(camera.target(0), Some((0.0, 0.0)));
        camera.record((1000.0, 1000.0), ActionKind::BaseAttack, 1, 10);
        assert_eq!(camera.target(5_999), Some((0.0, 0.0)));
        assert_eq!(camera.target(6_000), Some((1000.0, 1000.0)));
        camera.record((2000.0, 2000.0), ActionKind::Superweapon, 100, 6_001);
        assert_eq!(camera.target(6_001), Some((2000.0, 2000.0)));
        assert!(camera.status(6_001).contains("Superweapon"));
        assert!(camera.target(14_001).is_none());
        assert!(camera.status(14_001).contains("Waiting for action"));
        for index in 0..MAX_ENCOUNTERS + 1 {
            camera.record((index as f32 * 500.0, 0.0), ActionKind::Fighting, 1, 15_000);
        }
        assert_eq!(camera.encounters.len(), MAX_ENCOUNTERS);
    }

    #[test]
    fn retail_fire_facts_ignore_allies_and_leave_simulation_hash_unchanged() {
        use crate::sim::{combat::Health, game_entity::GameEntity, house_state::HouseState};
        let retail = crate::rules::retail_ini_fixture::retail_battle_rules_for_map("XMP03T4.MAP")
            .expect("retail rules");
        let rules = &retail.rules;
        let mut sim = Simulation::with_seed(123);
        let attacker = sim.intern("Computer1");
        let defender = sim.intern("Computer2");
        sim.register_house(HouseState::new(attacker, 0, None, false, 0, 10));
        sim.register_house(HouseState::new(defender, 0, None, false, 0, 10));
        for (id, owner, category, name) in [
            (1, attacker, EntityCategory::Unit, "MTNK"),
            (2, defender, EntityCategory::Structure, "GAPOWR"),
        ] {
            let type_id = sim.intern(name);
            let mut entity = GameEntity::new_at_frame_zero_for_test(
                id,
                20,
                20,
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
        let mut shot = SimFireEvent::for_test(1);
        shot.weapon_id = sim.intern(rules.object("MTNK").unwrap().primary().unwrap());
        shot.target = TargetKind::Entity(2);
        let mut camera = SpectatorCamera::default();
        camera.toggle(0);
        let hash = sim.state_hash();
        camera.observe_frame(&sim, rules, &[shot.clone()], &[], 0);
        assert!(camera.target(0).is_some());
        assert!(camera.status(0).contains("Base under attack"));
        assert_eq!(sim.state_hash(), hash);
        camera.encounters.clear();
        sim.house_alliances_mut_for_test()
            .entry("COMPUTER1".into())
            .or_default()
            .insert("COMPUTER2".into());
        camera.observe_frame(&sim, rules, &[shot], &[], 1);
        assert!(camera.target(1).is_none());
        let nuke = sim.intern("NukeSpecial");
        let hash = sim.state_hash();
        camera.observe_frame(
            &sim,
            rules,
            &[],
            &[SimSoundEvent::SuperWeaponLaunched {
                owner: attacker,
                sw_type: nuke,
                world: crate::sim::anim_class::AnimWorldCoord {
                    x: 8000,
                    y: 8000,
                    z: 0,
                },
            }],
            2,
        );
        assert!(camera.target(2).is_some());
        assert!(camera.status(2).contains("Superweapon"));
        assert_eq!(sim.state_hash(), hash);
    }
}
