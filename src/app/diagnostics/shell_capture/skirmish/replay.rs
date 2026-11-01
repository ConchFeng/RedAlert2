//! A bounded generated-match recording replayed through the ordinary client.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::*;
use crate::app::input::camera;
use crate::app::replay_navigation;
use crate::sim::replay::ReplayLog;
use crate::ui::replay_tools::ReplayToolAction;
use winit::event::MouseButton;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum CaptureGoal {
    #[default]
    FullRoute,
    Bookmarks,
    Seeking,
    Tracking,
}

#[derive(Default)]
pub(super) struct ReplayCapture {
    stage: u8,
    goal: CaptureGoal,
    screenshot_ready: bool,
    replay: Option<Arc<ReplayLog>>,
    recording_identity: Option<String>,
    hashes: BTreeMap<u64, u64>,
    starting_mcvs: Vec<u64>,
    tracked: Option<u64>,
    final_tick: u64,
    final_hash: u64,
    bookmark_base: usize,
    bookmark_ticks: [u64; 2],
    held_identity: Option<(u64, u32, u64)>,
    held_frames: u32,
    tracking_released: bool,
    artifact: Option<PathBuf>,
}

fn button(state: &mut AppState, action: ReplayToolAction) -> Result<()> {
    let point = crate::app::presentation::replay_tools::action_center(state, action)
        .with_context(|| format!("replay control {action:?} is unavailable"))?;
    let commands = state
        .match_state
        .sim_runtime
        .as_ref()
        .map(|runtime| runtime.simulation.pending_command_snapshot());
    SkirmishCapture::spectator_pointer_input(state, point, MouseButton::Left)?;
    ensure!(
        commands
            == state
                .match_state
                .sim_runtime
                .as_ref()
                .map(|runtime| runtime.simulation.pending_command_snapshot()),
        "replay UI queued gameplay commands"
    );
    Ok(())
}

fn begin_playback(state: &mut AppState, replay: Arc<ReplayLog>, identity: String) -> Result<()> {
    replay_navigation::begin_log_playback(state, replay, identity)?;
    SkirmishCapture::set_pointer(state, (EXPECTED_CURSOR_X as i32, EXPECTED_CURSOR_Y as i32));
    Ok(())
}

fn select_bookmark(state: &mut AppState, index: usize) -> Result<()> {
    let layout = crate::app::presentation::replay_tools::layout(state)
        .context("replay highlights layout missing")?;
    for _ in 0..layout.page {
        button(state, ReplayToolAction::PreviousPage)?;
    }
    for _ in 0..layout.pages {
        if crate::app::presentation::replay_tools::action_center(
            state,
            ReplayToolAction::SelectBookmark(index),
        )
        .is_some()
        {
            return button(state, ReplayToolAction::SelectBookmark(index));
        }
        if crate::app::presentation::replay_tools::action_center(state, ReplayToolAction::NextPage)
            .is_some()
        {
            button(state, ReplayToolAction::NextPage)?;
        } else {
            break;
        }
    }
    bail!("replay bookmark {index} was not reachable through highlights paging");
}

impl ReplayCapture {
    pub(super) fn with_goal(goal: CaptureGoal) -> Self {
        Self {
            goal,
            ..Self::default()
        }
    }

    fn record(&mut self, state: &mut AppState, frame: u32, route: &mut Vec<Value>) -> Result<()> {
        let runtime = state
            .match_state
            .sim_runtime
            .as_ref()
            .context("recording runtime missing")?;
        let sim = &runtime.simulation;
        if self.starting_mcvs.is_empty() {
            self.starting_mcvs = sim
                .entities()
                .values_sorted()
                .filter(|entity| {
                    entity.is_active()
                        && sim
                            .object_type(entity.type_ref(), &runtime.resources.rules)
                            .and_then(|object| object.deploys_into.as_deref())
                            .and_then(|name| runtime.resources.rules.object(name))
                            .is_some_and(|object| object.construction_yard)
                })
                .map(|entity| entity.stable_id())
                .collect();
        }
        if sim.session().tick() < 600 {
            return Ok(());
        }
        ensure!(
            matches!(
                state.match_state.loaded_map_source,
                Some(crate::map::source::LoadedMapSource::Generated { .. })
            ),
            "replay capture did not use the generated map loader"
        );
        self.starting_mcvs.retain(|id| {
            sim.entities()
                .get(*id)
                .is_none_or(|entity| !entity.is_active())
        });
        ensure!(
            !self.starting_mcvs.is_empty(),
            "no starting MCV naturally detached during the recorded match"
        );
        self.final_tick = sim.session().tick();
        self.final_hash = sim.state_hash();
        let mut replay = state
            .match_state
            .match_diagnostics
            .replay_log
            .as_ref()
            .context("production match did not record its replay")?
            .clone();
        let last = replay
            .ticks
            .last_mut()
            .context("production replay contains no frames")?;
        ensure!(
            last.tick == self.final_tick,
            "recording cursor differs from the installed match"
        );
        last.state_hash = Some(self.final_hash);
        let startup = replay.standalone_startup()?;
        ensure!(
            startup.launch_session().ai_player_count == 29
                && startup.launch_session().opponents.len() == 29,
            "replay lost one of its 29 AI slots"
        );
        let recipe = startup
            .generated_map()
            .context("generated replay omitted its accepted SED recipe")?;
        let seed_bytes = recipe.seed_bytes.len();
        let mut mismatched = serde_json::to_value(&replay)?;
        mismatched["startup"]["map_sha256"] = json!("0".repeat(64));
        let mismatched: ReplayLog = serde_json::from_value(mismatched)?;
        ensure!(
            mismatched.standalone_startup().is_err(),
            "replay admitted generated bytes with a mismatched recorded identity"
        );
        self.hashes = replay
            .ticks
            .iter()
            .filter_map(|entry| entry.state_hash.map(|hash| (entry.tick, hash)))
            .collect();
        let directory = state
            .platform
            .game_config
            .as_ref()
            .context("capture configuration missing")?
            .paths
            .settings_dir()
            .join("replays");
        fs::create_dir_all(&directory)?;
        let stamp = crate::util::time::SystemTime::now()
            .duration_since(crate::util::time::UNIX_EPOCH)?
            .as_nanos();
        let path = directory.join(format!("highlights-capture-{stamp}.json"));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        replay.write_json(&mut file)?;
        drop(file);
        let (replay, identity) = ReplayLog::load_bound(&path)?;
        let replay = Arc::new(replay);
        route.push(json!({"screen":"in game", "frame":frame, "action":"RecordGeneratedFfaReplay", "recorded_ticks":replay.ticks.len(), "final_tick":self.final_tick, "final_hash":self.final_hash, "initial_hash":replay.standalone_startup()?.initial_hash(), "ai_slots":29, "generated_input_retained":true, "seed_bytes":seed_bytes, "mismatched_generated_identity_rejected":true, "artifact":path, "recording_sha256":identity}));
        self.recording_identity = Some(identity.clone());
        begin_playback(state, replay.clone(), identity)?;
        self.replay = Some(replay);
        self.artifact = Some(path);
        self.stage = 1;
        Ok(())
    }

    fn assert_recorded_hash(&self, state: &AppState, tick: u64) -> Result<()> {
        let actual = SkirmishCapture::simulation_identity(state)?;
        let expected = self
            .hashes
            .get(&tick)
            .context("requested replay tick has no recorded hash")?;
        ensure!(
            actual.0 == tick && actual.2 == *expected,
            "replay destination tick/hash differ: tick {}, expected {tick}, hash {:#x}, expected {expected:#x}",
            actual.0,
            actual.2
        );
        Ok(())
    }

    fn select_starting_mcv(&mut self, state: &mut AppState) -> Result<()> {
        for &id in &self.starting_mcvs {
            let point = {
                let runtime = state
                    .match_state
                    .sim_runtime
                    .as_ref()
                    .context("playback runtime missing")?;
                let Some(entity) = runtime
                    .simulation
                    .entities()
                    .get(id)
                    .filter(|entity| entity.is_active())
                else {
                    continue;
                };
                crate::app::presentation::interpolation::screen_position(state, entity)
            };
            let mut camera_view = camera::current_view(state);
            camera_view.center = [point.0, point.1];
            camera::set_view(state, camera_view);
            let input = &state.match_state.input;
            let screen = (
                ((point.0 - input.camera_x) * input.zoom_level) as i32,
                ((point.1 - input.camera_y) * input.zoom_level) as i32,
            );
            let identity = SkirmishCapture::simulation_identity(state)?;
            let commands = state
                .match_state
                .sim_runtime
                .as_ref()
                .unwrap()
                .simulation
                .pending_command_snapshot();
            SkirmishCapture::spectator_pointer_input(state, screen, MouseButton::Left)?;
            ensure!(
                commands
                    == state
                        .match_state
                        .sim_runtime
                        .as_ref()
                        .unwrap()
                        .simulation
                        .pending_command_snapshot(),
                "read-only replay selection queued commands"
            );
            ensure!(
                identity == SkirmishCapture::simulation_identity(state)?,
                "read-only selection changed playback state"
            );
            if replay_navigation::view(state).is_some_and(|view| view.selected_unit == Some(id)) {
                button(state, ReplayToolAction::ToggleFollow)?;
                ensure!(
                    replay_navigation::view(state).is_some_and(|view| view.following),
                    "Follow Unit did not latch the read-only selection"
                );
                self.tracked = Some(id);
                return Ok(());
            }
        }
        bail!("no reconstructed starting MCV was selectable through replay input");
    }

    pub(super) fn advance(
        &mut self,
        state: &mut AppState,
        frame: u32,
        route: &mut Vec<Value>,
    ) -> Result<bool> {
        if self.screenshot_ready {
            return Ok(true);
        }
        if self.stage == 0 {
            self.record(state, frame, route)?;
            return Ok(false);
        }
        let Some(view) = replay_navigation::view(state) else {
            return Ok(false);
        };
        if self.goal == CaptureGoal::Seeking
            && self.stage == 4
            && view
                .seeking
                .as_ref()
                .is_some_and(|progress| !progress.loading)
        {
            route.push(json!({"screen":"replay seeking", "frame":frame, "action":"CaptureSeekProgress", "original_tick":view.tick, "target_tick":view.seeking.as_ref().unwrap().target_tick}));
            self.screenshot_ready = true;
            return Ok(true);
        }
        let status = view.status.as_deref().unwrap_or_default();
        if self.stage == 9 {
            if view.browser.is_none() {
                return Ok(false);
            }
            ensure!(
                status.contains("rules/art differ"),
                "mismatched rules did not produce a readable browser error: {status}"
            );
            route.push(json!({"screen":"replay browser", "frame":frame, "action":"MismatchedRulesReturnToBrowser", "status":status}));
            let mut missing = serde_json::to_value(
                self.replay
                    .as_ref()
                    .context("capture replay missing")?
                    .as_ref(),
            )?;
            let name = "VERA-missing-replay-map.MAP";
            missing["header"]["map_name"] = json!(name);
            missing["startup"]["launch"]["selected_map_file"] = json!(name);
            missing["startup"]["session"]["map_name"] = json!(name);
            missing["startup"]["generated_map"] = Value::Null;
            let missing: ReplayLog = serde_json::from_value(missing)?;
            missing.standalone_startup()?;
            let identity = crate::util::sha256::sha256_hex(&serde_json::to_vec(&missing)?);
            begin_playback(state, Arc::new(missing), identity)?;
            self.stage = 10;
            return Ok(false);
        }
        if self.stage == 10 {
            if view.browser.is_none() {
                return Ok(false);
            }
            ensure!(
                status.contains("Replay loading failed"),
                "missing map did not return a loading failure: {status}"
            );
            ensure!(
                state.frontend.screen == GameScreen::InGame,
                "replay browser is not on its rendered screen"
            );
            route.push(json!({"screen":"replay browser", "frame":frame, "action":"MissingMapReturnsToUsableBrowser", "status":status}));
            replay_navigation::close_browser(state);
            ensure!(
                !state.match_state.match_diagnostics.is_replay_playback(),
                "closing failed playback retained its input gate"
            );
            begin_playback(
                state,
                self.replay.as_ref().unwrap().clone(),
                self.recording_identity.as_ref().unwrap().clone(),
            )?;
            self.stage = 11;
            return Ok(false);
        }
        if self.stage == 11 {
            if !view.ready || view.validating {
                return Ok(false);
            }
            ensure!(
                view.tick == 0 && view.paused,
                "recovered playback did not retain frame zero"
            );
            ensure!(
                view.bookmarks.len() == self.bookmark_base + 2,
                "saved bookmarks did not survive browser recovery"
            );
            button(state, ReplayToolAction::ToggleHighlights)?;
            route.push(json!({"screen":"replay highlights", "frame":frame, "action":"RecoverPlaybackAndSavedHighlights", "bookmarks":view.bookmarks.len(), "tick":view.tick}));
            self.stage = 12;
            return Ok(false);
        }
        if self.stage == 12 {
            return Ok(true);
        }
        ensure!(
            !status.to_ascii_lowercase().contains("failed"),
            "replay capture failed: {status}"
        );
        if !view.ready || view.validating {
            return Ok(false);
        }
        self.tracking_released |= status.contains("Tracked unit no longer available");
        match self.stage {
            1 => {
                let expected = self
                    .replay
                    .as_ref()
                    .context("immutable capture replay missing")?
                    .standalone_startup()?
                    .initial_hash();
                let identity = SkirmishCapture::simulation_identity(state)?;
                ensure!(
                    identity.0 == 0 && identity.2 == expected && view.paused,
                    "reconstructed replay did not retain its paused initial hash"
                );
                self.bookmark_base = view.bookmarks.len();
                self.select_starting_mcv(state)?;
                if self.goal == CaptureGoal::Tracking {
                    route.push(json!({"screen":"replay tracking", "frame":frame, "action":"CaptureReadOnlyUnitTracking", "tracked_unit":self.tracked}));
                    self.screenshot_ready = true;
                    return Ok(true);
                }
                button(state, ReplayToolAction::CycleSpeed)?;
                ensure!(
                    replay_navigation::view(state).is_some_and(|view| view.speed > 1),
                    "replay speed control did not change playback speed"
                );
                button(state, ReplayToolAction::TogglePause)?;
                route.push(json!({"screen":"replay", "frame":frame, "action":"ReplayInitialHashAndReadOnlyFollow", "initial_hash":expected, "tracked_mcv":self.tracked, "simulation_commands_unchanged":true, "prevalidation_completed":true}));
                self.stage = 2;
            }
            2 | 3 => {
                let ordinal = usize::from(self.stage - 2);
                let target = if ordinal == 0 { 150 } else { 450 };
                if view.tick < target || !self.hashes.contains_key(&view.tick) {
                    return Ok(false);
                }
                ensure!(
                    view.tick < self.final_tick,
                    "playback ended before both bookmarks were created"
                );
                button(state, ReplayToolAction::TogglePause)?;
                self.assert_recorded_hash(state, view.tick)?;
                self.bookmark_ticks[ordinal] = view.tick;
                if crate::app::presentation::replay_tools::action_center(
                    state,
                    ReplayToolAction::AddBookmark,
                )
                .is_none()
                {
                    button(state, ReplayToolAction::ToggleHighlights)?;
                }
                button(state, ReplayToolAction::AddBookmark)?;
                ensure!(
                    replay_navigation::view(state).is_some_and(
                        |view| view.bookmarks.len() == self.bookmark_base + ordinal + 1
                    ),
                    "Add Bookmark did not append presentation metadata"
                );
                route.push(json!({"screen":"replay highlights", "frame":frame, "action":"AddTimestampedBookmark", "tick":view.tick, "state_hash":self.hashes[&view.tick], "bookmark_index":self.bookmark_base + ordinal}));
                if ordinal == 0 {
                    ensure!(
                        view.speed == 2,
                        "accelerated playback did not retain 2x speed"
                    );
                    for _ in 0..8 {
                        if replay_navigation::view(state).is_some_and(|view| view.speed == 1) {
                            break;
                        }
                        button(state, ReplayToolAction::CycleSpeed)?;
                    }
                    ensure!(
                        replay_navigation::view(state).is_some_and(|view| view.speed == 1),
                        "replay speed did not cycle back to normal"
                    );
                    route.push(json!({"screen":"replay", "frame":frame, "action":"AcceleratedPlaybackMatchesRecordedHash", "tick":view.tick, "speed":view.speed}));
                    button(state, ReplayToolAction::ClosePanel)?;
                    button(state, ReplayToolAction::TogglePause)?;
                    self.stage = 3;
                } else {
                    select_bookmark(state, self.bookmark_base)?;
                    if self.goal == CaptureGoal::Bookmarks {
                        route.push(json!({"screen":"replay highlights", "frame":frame, "action":"CaptureTimestampedHighlights", "bookmarks":self.bookmark_ticks}));
                        self.screenshot_ready = true;
                        return Ok(true);
                    }
                    button(state, ReplayToolAction::SeekSelected)?;
                    self.stage = 4;
                }
            }
            4 | 5 => {
                if view.seeking.is_some() {
                    return Ok(false);
                }
                let ordinal = usize::from(self.stage - 4);
                let tick = self.bookmark_ticks[ordinal];
                self.assert_recorded_hash(state, tick)?;
                ensure!(
                    view.paused,
                    "bookmark seek resumed playback without permission"
                );
                route.push(json!({"screen":"replay highlights", "frame":frame, "action":if ordinal == 0 {"SeekBackwardToRecordedHash"} else {"SeekForwardToRecordedHash"}, "tick":tick, "state_hash":self.hashes[&tick]}));
                if ordinal == 0 {
                    select_bookmark(state, self.bookmark_base + 1)?;
                    button(state, ReplayToolAction::SeekSelected)?;
                    self.stage = 5;
                } else {
                    let identity = SkirmishCapture::simulation_identity(state)?;
                    select_bookmark(state, self.bookmark_base)?;
                    button(state, ReplayToolAction::SeekSelected)?;
                    button(state, ReplayToolAction::CancelSeek)?;
                    ensure!(
                        identity == SkirmishCapture::simulation_identity(state)?,
                        "canceling a seek replaced the retained world"
                    );
                    self.held_identity = Some(identity);
                    self.held_frames = 0;
                    self.stage = 6;
                }
            }
            6 => {
                ensure!(
                    Some(SkirmishCapture::simulation_identity(state)?) == self.held_identity,
                    "a canceled seek later installed its worker result"
                );
                self.held_frames += 1;
                if self.held_frames < 10 {
                    return Ok(false);
                }
                route.push(json!({"screen":"replay highlights", "frame":frame, "action":"CanceledSeekPreservesOriginalWorld", "tick":view.tick, "state_hash":self.hashes[&view.tick], "held_frames":self.held_frames}));
                button(state, ReplayToolAction::ClosePanel)?;
                button(state, ReplayToolAction::TogglePause)?;
                self.stage = 7;
            }
            7 => {
                if view.tick < self.final_tick {
                    return Ok(false);
                }
                self.assert_recorded_hash(state, self.final_tick)?;
                ensure!(
                    self.tracking_released,
                    "the naturally detached tracked MCV did not report release"
                );
                button(state, ReplayToolAction::ToggleHighlights)?;
                self.held_identity = Some(SkirmishCapture::simulation_identity(state)?);
                self.held_frames = 0;
                self.stage = 8;
            }
            8 => {
                ensure!(
                    Some(SkirmishCapture::simulation_identity(state)?) == self.held_identity,
                    "finished replay advanced beyond its final recorded frame"
                );
                self.held_frames += 1;
                if self.held_frames < 10 {
                    return Ok(false);
                }
                route.push(json!({"screen":"replay highlights", "frame":frame, "action":"FinalReplayHashAndFrozenInspection", "tick":self.final_tick, "state_hash":self.final_hash, "bookmarks":self.bookmark_ticks, "tracked_unit_release_seen":self.tracking_released, "recording_artifact":self.artifact, "frozen_presentations":self.held_frames}));
                let mut mismatched = self
                    .replay
                    .as_ref()
                    .context("capture replay missing")?
                    .as_ref()
                    .clone();
                mismatched.header.rules_hash ^= 1;
                let identity = crate::util::sha256::sha256_hex(&serde_json::to_vec(&mismatched)?);
                begin_playback(state, Arc::new(mismatched), identity)?;
                self.stage = 9;
            }
            _ => return Ok(true),
        }
        Ok(false)
    }
}
