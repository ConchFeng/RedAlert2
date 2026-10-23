//! Native-window frontend adapter for the existing scenario and command owners.
//!
//! The initial interactive map shares the terrain of the observation map, with
//! the four ships and a turreted tank authored for the local House. No live object is reassigned or
//! constructed here: the production scenario loader owns their admission.

use std::path::Path;
use std::time::Duration;

use vera20k_core::headless_scenario::{HeadlessScenario, battle_session, load_session};
use vera20k_core::map::houses::HouseColorMap;
use vera20k_core::sim::command::{Command, CommandEnvelope};
use vera20k_core::sim::world::{FrameEffects, LifecycleOutput, TickLane};
use vera20k_core::skirmish_launch::{AiDifficulty, LaunchCountry};

// Match the desktop client's fixed step (src/app/types.rs), rather than the
// slower digest tool cadence in headless_scenario::tick.
pub const TICK_MS: u32 = 22;
const MAX_CATCH_UP: u32 = 8;

pub struct SwitchMatch {
    pub scenario: HeadlessScenario,
    pub colors: HouseColorMap,
    accumulated: Duration,
    pub frames_advanced: u64,
    pub bootstrap_frames: u64,
}

impl SwitchMatch {
    pub fn load(retail: &Path, map_path: &Path) -> Result<Self, String> {
        let map_name = map_path
            .strip_prefix(retail)
            .unwrap_or(map_path)
            .to_str()
            .ok_or("scenario map path is not UTF-8")?;
        let mut session =
            battle_session(map_name, &[(LaunchCountry::Russia, AiDifficulty::Normal)]);
        // A command-integration scenario, with authored starting forces. The
        // local base comes from deploying the authored MCV through the same
        // native command path a player uses; it is not constructed here.
        session.options.bases = false;
        session.options.unit_count = 0;
        session.options.short_game = false;
        session.options.crates = false;
        session.options.shroud = false;
        session.options.fog_of_war = false;
        let scenario = load_session(retail, map_name, 0x5a17_2026, session.clone())?;
        if scenario.sim().session().current_house().is_none() {
            return Err("scenario did not publish a local House".into());
        }
        let roster = vera20k_core::map::houses::parse_house_roster(
            &scenario.map.ini,
            scenario.runtime.resources.rules.color_schemes.as_slice(),
            Some(&scenario.runtime.resources.rules),
        );
        let colors = vera20k_core::map::houses::house_color_map_for_launch_session(
            &session,
            &roster,
            scenario.runtime.resources.rules.color_schemes.as_slice(),
        );
        let mut game = Self {
            scenario,
            colors,
            accumulated: Duration::ZERO,
            frames_advanced: 0,
            bootstrap_frames: 0,
        };
        game.bootstrap_frames = game.deploy_local_mcv()?;
        // Interactive reporting starts after the deterministic base bootstrap.
        game.frames_advanced = 0;
        Ok(game)
    }

    /// Deploy the authored local MCV through the production command receiver
    /// and advance until its Construction Yard is online. This is scenario
    /// bootstrap, not a shortcut around construction or factory ownership.
    fn deploy_local_mcv(&mut self) -> Result<u64, String> {
        let mcv = {
            let sim = self.scenario.sim();
            let owner = sim
                .session()
                .current_house()
                .ok_or("scenario has no local House")?;
            let rules = &self.scenario.runtime.resources.rules;
            sim.entities()
                .values()
                .find(|entity| {
                    entity.owner() == owner
                        && !entity.in_limbo()
                        && sim
                            .object_type(entity.type_ref(), rules)
                            .and_then(|object| object.deploys_into.as_deref())
                            .and_then(|target| rules.object(target))
                            .is_some_and(|building| building.construction_yard)
                })
                .map(|entity| entity.stable_id())
                .ok_or("interactive map has no local MCV")?
        };
        if !self.issue(Command::DeployMcv { entity_id: mcv }) {
            return Err("local MCV deploy command was not admitted".into());
        }
        for frames in 1..=900_u64 {
            self.advance(Duration::from_millis(u64::from(TICK_MS)))?;
            if self.local_construction_yard_online() {
                return Ok(frames);
            }
        }
        Err("local Construction Yard did not come online within 900 frames".into())
    }

    pub fn local_construction_yard_online(&self) -> bool {
        let sim = self.scenario.sim();
        let rules = &self.scenario.runtime.resources.rules;
        let Some(owner) = sim.session().current_house() else {
            return false;
        };
        sim.entities().values().any(|entity| {
            entity.owner() == owner
                && !entity.in_limbo()
                && entity.building_online()
                && sim
                    .object_type(entity.type_ref(), rules)
                    .is_some_and(|object| object.construction_yard)
        })
    }

    pub fn advance(&mut self, elapsed: Duration) -> Result<u32, String> {
        self.advance_with_lifecycle(elapsed)
            .map(|(advanced, _)| advanced)
    }

    pub fn advance_with_lifecycle(
        &mut self,
        elapsed: Duration,
    ) -> Result<(u32, Vec<LifecycleOutput>), String> {
        let step = Duration::from_millis(u64::from(TICK_MS));
        self.accumulated = (self.accumulated + elapsed).min(step * MAX_CATCH_UP);
        let mut advanced = 0;
        let mut lifecycle = Vec::new();
        while self.accumulated >= step {
            let due_commands = self.scenario.runtime.simulation.take_due_commands();
            let output = self
                .scenario
                .runtime
                .advance_frame(
                    &due_commands,
                    TICK_MS,
                    TickLane::Ordinary,
                    FrameEffects::default(),
                )
                .map_err(|error| format!("Switch simulation frame: {error:?}"))?;
            lifecycle.extend(output.lifecycle_outputs);
            self.accumulated -= step;
            self.frames_advanced += 1;
            advanced += 1;
        }
        Ok((advanced, lifecycle))
    }

    pub fn issue(&mut self, command: Command) -> bool {
        let sim = &mut self.scenario.runtime.simulation;
        let Some(owner) = sim.session().current_house() else {
            return false;
        };
        sim.issue_local_command(CommandEnvelope::new(owner, sim.session().tick(), command))
    }
}

pub fn write_interactive_map(directory: &Path) -> Result<std::path::PathBuf, std::io::Error> {
    std::fs::create_dir_all(directory)?;
    let source = include_str!("../../../tools/map_observation/examples/ai_dred_bombard.map");
    let mut section = None;
    let mut map = String::with_capacity(source.len());
    for line in source.lines() {
        if line.starts_with('[') {
            if section == Some("units") {
                // Land south of the harbor, clear of the building footprints.
                // An ordinary HTNK exercises independent hull, turret and barrel.
                // The local AMCV is the authored source of the local base; the
                // native reader admits it and `DeployMcv` converts it into a
                // Construction Yard.
                map.push_str("4=<human player>,AMCV,256,40,40,64,Guard,None,0,-1,0,-1,1,1\n");
                map.push_str("5=<human player>,HTNK,256,40,43,64,Guard,None,0,-1,0,-1,1,1\n");
            }
            section = line.eq_ignore_ascii_case("[Units]").then_some("units");
            section = section.or_else(|| {
                line.eq_ignore_ascii_case("[Structures]")
                    .then_some("structures")
            });
        }
        // FromName50C170 resolves the generated human House's scenario name,
        // "<human player>". "Player" is its internal key, not its map name.
        // Keep the authored Structures with the generated Computer House:
        // nonzero-game-mode loading intentionally skips current-House
        // buildings, and an empty AI House would reach the multiplayer defeat
        // gate during the command regression.
        if section == Some("units") {
            map.push_str(&line.replacen("=Computer,", "=<human player>,", 1));
        } else {
            map.push_str(line);
        }
        map.push('\n');
    }
    let path = directory.join("switch-command.map");
    std::fs::write(&path, map)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vera20k_core::assets::asset_manager::AssetManager;

    #[test]
    fn retail_scenario_commits_selection_and_moves_a_local_ship() {
        let Some(retail) = std::env::var_os("VERA_SWITCH_RETAIL_DIR") else {
            eprintln!("set VERA_SWITCH_RETAIL_DIR for the retail scenario regression");
            return;
        };
        let root = std::env::temp_dir().join(format!("vera-switch-match-{}", std::process::id()));
        let path = write_interactive_map(&root).unwrap();
        let mut game =
            SwitchMatch::load(Path::new(&retail), &path).expect("production scenario load");
        assert!(game.bootstrap_frames > 0, "the authored MCV must deploy");
        assert!(
            game.local_construction_yard_online(),
            "the deployed yard must be online"
        );
        let construction = vera20k_core::sim::production::initial_construction_options(
            game.scenario.sim(),
            &game.scenario.runtime.resources.rules,
        );
        assert!(
            !construction.is_empty(),
            "the online Construction Yard must publish build options"
        );
        let sim = game.scenario.sim();
        assert!(sim.path_grid().is_some(), "loader must publish navigation");
        let owner = sim.session().current_house().unwrap();
        let tank = sim
            .entities()
            .values()
            .find(|entity| {
                entity.owner() == owner && sim.interner().resolve(entity.type_ref()) == "HTNK"
            })
            .expect("authored turreted tank admitted by production loader");
        let rules = &game.scenario.runtime.resources.rules;
        use vera20k_core::rules::voxel_frame_catalog::{
            VxlLayer, draws_turret_parts, seed_layers_for,
        };
        assert!(draws_turret_parts(
            "HTNK",
            Some(rules),
            tank.current_turret_index()
        ));
        let assets = AssetManager::new(
            Path::new(&retail),
            vera20k_core::assets::asset_manager::MediaArchiveMode::STOCK_DIGITAL,
        )
        .unwrap();
        let layers = seed_layers_for(&assets, "HTNK", Some(rules));
        assert!(layers.contains(&(VxlLayer::Body, 0)));
        assert!(layers.contains(&(VxlLayer::Turret, 0)));
        assert!(layers.contains(&(VxlLayer::Barrel, 0)));
        let id = sim
            .entities()
            .values()
            .find(|entity| {
                entity.owner() == owner && sim.interner().resolve(entity.type_ref()) == "HYD"
            })
            .expect("authored local ship admitted by production loader")
            .stable_id();
        assert!(game.issue(Command::Select {
            entity_ids: vec![id],
            additive: false
        }));
        assert_eq!(game.advance(Duration::from_millis(21)).unwrap(), 0);
        assert_eq!(game.advance(Duration::from_millis(1)).unwrap(), 1);
        assert!(
            game.scenario
                .sim()
                .entities()
                .get(id)
                .unwrap()
                .selection
                .is_selected()
        );
        let before = game.scenario.sim().entities().get(id).unwrap().position;
        assert!(game.issue(Command::Move {
            entity_id: id,
            target_rx: 43,
            target_ry: 45
        }));
        for _ in 0..180 {
            game.advance(Duration::from_millis(u64::from(TICK_MS)))
                .unwrap();
        }
        let after = game.scenario.sim().entities().get(id).unwrap().position;
        assert_ne!(
            (after.rx(), after.ry()),
            (before.rx(), before.ry()),
            "a queued order must move the live ship"
        );
        assert_eq!(game.frames_advanced, 181);
        assert_eq!(
            game.scenario.sim().session().total_sim_ms(),
            (game.bootstrap_frames + 181) * u64::from(TICK_MS)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
