//! Recording metadata and transactional temporal navigation through the replay owner.

use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::sim::replay::{ReplayLog, ReplayPlayback, ReplayRunner};
use crate::ui::second_view::CameraView;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ReplayBookmark {
    pub tick: u64,
    pub label: String,
    pub view: CameraView,
    pub tracked_unit: Option<u64>,
}

#[derive(Serialize, Deserialize)]
struct BookmarkFile {
    version: u32,
    recording: String,
    bookmarks: Vec<ReplayBookmark>,
}

pub(crate) struct ReplayRecordingRow {
    pub label: String,
}
pub(crate) struct ReplaySeekProgress {
    pub target_tick: u64,
    pub reached_tick: u64,
    pub loading: bool,
}
pub(crate) struct ReplayNavigationView {
    pub tick: u64,
    pub final_tick: u64,
    pub tick_ms: u32,
    pub paused: bool,
    pub speed: u32,
    pub finished: bool,
    pub seeking: Option<ReplaySeekProgress>,
    pub bookmarks: Vec<ReplayBookmark>,
    pub selected_unit: Option<u64>,
    pub following: bool,
    pub status: Option<String>,
    pub browser: Option<Vec<ReplayRecordingRow>>,
    pub ready: bool,
    pub validating: bool,
}

enum JobKind {
    Validate,
    Seek(ReplayBookmark),
}
enum JobOutput {
    Validated,
    Destination(crate::headless_scenario::HeadlessScenario, ReplayPlayback),
}
struct NavigationJob {
    kind: JobKind,
    target: u64,
    reached: Arc<AtomicU64>,
    loading: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    result: mpsc::Receiver<std::result::Result<JobOutput, String>>,
}
impl Drop for NavigationJob {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

pub(crate) struct ReplayNavigation {
    paused: bool,
    speed: u32,
    validated: bool,
    validation_started: bool,
    metadata_loaded: bool,
    bookmarks: Vec<ReplayBookmark>,
    selected: Option<u64>,
    notice: Option<String>,
    browser: bool,
    recording_paths: Vec<PathBuf>,
    map_beside: Option<String>,
    job: Option<NavigationJob>,
    was_paused: bool,
}
impl Default for ReplayNavigation {
    fn default() -> Self {
        Self {
            paused: false,
            speed: 1,
            validated: false,
            validation_started: false,
            metadata_loaded: false,
            bookmarks: Vec::new(),
            selected: None,
            notice: None,
            browser: false,
            recording_paths: Vec::new(),
            map_beside: None,
            job: None,
            was_paused: false,
        }
    }
}
impl ReplayNavigation {
    pub(crate) fn can_advance(&self) -> bool {
        self.validated && !self.paused && !self.browser && self.job.is_none()
    }
    pub(crate) fn selected_unit(&self) -> Option<u64> {
        self.selected
    }
}

fn current_tick(state: &AppState) -> u64 {
    state
        .match_state
        .sim_runtime
        .as_ref()
        .map_or(0, |runtime| runtime.simulation.session().tick())
}
fn final_tick(log: &ReplayLog) -> u64 {
    log.ticks.last().map_or(0, |entry| entry.tick)
}

pub(crate) fn view(state: &AppState) -> Option<ReplayNavigationView> {
    let diagnostics = &state.match_state.match_diagnostics;
    if !diagnostics.is_replay_playback() {
        return None;
    }
    let nav = &diagnostics.navigation;
    let log = diagnostics.replay_log_arc();
    let finished = diagnostics
        .replay_cursor()
        .is_some_and(ReplayPlayback::is_finished);
    Some(ReplayNavigationView {
        tick: current_tick(state),
        final_tick: log.as_ref().map_or(0, |log| final_tick(log)),
        tick_ms: log
            .as_ref()
            .and_then(|log| log.startup.as_deref())
            .map_or(66, |startup| startup.tick_ms()),
        paused: nav.paused,
        speed: nav.speed,
        finished,
        seeking: nav.job.as_ref().map(|job| ReplaySeekProgress {
            target_tick: job.target,
            reached_tick: job.reached.load(Ordering::Relaxed),
            loading: job.loading.load(Ordering::Relaxed),
        }),
        bookmarks: nav.bookmarks.clone(),
        selected_unit: nav.selected,
        following: state.match_state.input.follow_target.is_some(),
        status: nav.notice.clone().or_else(|| {
            if nav.validated {
                None
            } else {
                diagnostics.playback_status()
            }
        }),
        browser: nav.browser.then(|| {
            nav.recording_paths
                .iter()
                .map(|path| ReplayRecordingRow {
                    label: path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                })
                .collect()
        }),
        ready: nav.validated
            && diagnostics
                .replay_cursor()
                .is_some_and(|cursor| cursor.failure().is_none()),
        validating: nav
            .job
            .as_ref()
            .is_some_and(|job| matches!(job.kind, JobKind::Validate)),
    })
}

pub(crate) fn speed(state: &AppState) -> u32 {
    state.match_state.match_diagnostics.navigation.speed
}
pub(crate) fn toggle_pause(state: &mut AppState) {
    let nav = &mut state.match_state.match_diagnostics.navigation;
    if nav.validated && nav.job.is_none() && !nav.browser {
        nav.paused = !nav.paused;
    }
}
pub(crate) fn cycle_speed(state: &mut AppState) {
    let nav = &mut state.match_state.match_diagnostics.navigation;
    if nav.validated && nav.job.is_none() && !nav.browser {
        nav.speed = match nav.speed {
            1 => 2,
            2 => 4,
            _ => 1,
        };
    }
}

fn metadata_path(state: &AppState) -> Option<PathBuf> {
    let identity = state.match_state.match_diagnostics.replay_identity()?;
    let config = state.platform.game_config.as_ref()?;
    (identity.len() == 64 && identity.bytes().all(|byte| byte.is_ascii_hexdigit())).then(|| {
        config
            .paths
            .settings_dir()
            .join("replay-highlights")
            .join(format!("{identity}.json"))
    })
}
fn valid_bookmarks(file: &BookmarkFile, identity: &str, end: u64) -> bool {
    file.version == 1
        && file.recording == identity
        && file.bookmarks.len() <= 256
        && file.bookmarks.iter().all(|bookmark| {
            bookmark.tick <= end
                && bookmark.label.chars().count() <= 80
                && bookmark.view.center.iter().all(|value| value.is_finite())
                && crate::app::input::camera::supported_zoom(bookmark.view.zoom)
        })
}
fn load_metadata(state: &mut AppState, log: &ReplayLog) {
    state
        .match_state
        .match_diagnostics
        .navigation
        .metadata_loaded = true;
    let Some(path) = metadata_path(state) else {
        return;
    };
    match std::fs::read(&path) {
        Ok(bytes) if bytes.len() <= 256_000 => match serde_json::from_slice::<BookmarkFile>(&bytes)
        {
            Ok(file)
                if valid_bookmarks(
                    &file,
                    state
                        .match_state
                        .match_diagnostics
                        .replay_identity()
                        .unwrap_or_default(),
                    final_tick(log),
                ) =>
            {
                state.match_state.match_diagnostics.navigation.bookmarks = file.bookmarks
            }
            _ => {
                state.match_state.match_diagnostics.navigation.notice =
                    Some("Saved highlights do not match this recording.".into())
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        _ => {
            state.match_state.match_diagnostics.navigation.notice =
                Some("Saved highlights could not be read.".into())
        }
    }
}
fn save_metadata(state: &AppState) -> Result<()> {
    let path = metadata_path(state).context("Recording identity is unavailable")?;
    let file = BookmarkFile {
        version: 1,
        recording: state
            .match_state
            .match_diagnostics
            .replay_identity()
            .unwrap_or_default()
            .into(),
        bookmarks: state
            .match_state
            .match_diagnostics
            .navigation
            .bookmarks
            .clone(),
    };
    std::fs::create_dir_all(path.parent().context("Highlights directory")?)?;
    let bytes = serde_json::to_vec_pretty(&file)?;
    crate::util::file_system::write(&path, bytes).context("Save replay highlights")
}
pub(crate) fn add_bookmark(state: &mut AppState, label: String) {
    let camera = crate::app::input::camera::current_view(state);
    let tick = current_tick(state);
    let tracked_unit = state.match_state.input.follow_target;
    let nav = &mut state.match_state.match_diagnostics.navigation;
    if !nav.validated || nav.job.is_some() || nav.browser {
        return;
    }
    if nav.bookmarks.len() == 256 {
        nav.notice = Some("This recording already has 256 bookmarks.".into());
        return;
    }
    let label: String = label.trim().chars().take(80).collect();
    nav.bookmarks.push(ReplayBookmark {
        tick,
        label: if label.is_empty() {
            format!("Bookmark {}", nav.bookmarks.len() + 1)
        } else {
            label
        },
        view: camera,
        tracked_unit,
    });
    nav.notice = Some("Bookmark added.".into());
    if let Err(error) = save_metadata(state) {
        state.match_state.match_diagnostics.navigation.notice =
            Some(format!("Highlight save failed: {error:#}"));
    }
}

fn spawn_job(
    retail: PathBuf,
    log: Arc<ReplayLog>,
    map_beside: Option<String>,
    kind: JobKind,
) -> Result<NavigationJob> {
    let target = match &kind {
        JobKind::Validate => final_tick(&log),
        JobKind::Seek(bookmark) => bookmark.tick,
    };
    let reached = Arc::new(AtomicU64::new(0));
    let loading = Arc::new(AtomicBool::new(true));
    let cancel = Arc::new(AtomicBool::new(false));
    let (sender, result) = mpsc::channel();
    let progress = reached.clone();
    let preparing = loading.clone();
    let canceled = cancel.clone();
    let validating = matches!(kind, JobKind::Validate);
    std::thread::Builder::new()
        .name("replay-navigation".into())
        .spawn(move || {
            let work = || -> Result<JobOutput> {
                let (mut loaded, mut playback) =
                    ReplayRunner::reconstruct(&retail, log.clone(), map_beside.as_deref())?;
                preparing.store(false, Ordering::Relaxed);
                while loaded.runtime.simulation.session().tick() < target {
                    ensure!(
                        !canceled.load(Ordering::Relaxed),
                        "Replay navigation canceled"
                    );
                    ensure!(
                        playback.can_advance(),
                        "Recording ended before the requested tick"
                    );
                    playback.advance(
                        &mut loaded.runtime,
                        crate::sim::world::FrameEffects::empty(),
                    )?;
                    progress.store(
                        loaded.runtime.simulation.session().tick(),
                        Ordering::Relaxed,
                    );
                }
                ensure!(
                    !canceled.load(Ordering::Relaxed),
                    "Replay navigation canceled"
                );
                ensure!(
                    loaded.runtime.simulation.session().tick() == target,
                    "Requested tick is not a committed recording boundary"
                );
                if validating {
                    // Playback checked every recorded hash, the final frame's
                    // included, before that frame's services.
                    playback.verification(&loaded.runtime)?;
                    ensure!(
                        playback.recorded_final_hash().is_some(),
                        "Recording final hash is absent"
                    );
                    Ok(JobOutput::Validated)
                } else {
                    Ok(JobOutput::Destination(loaded, playback))
                }
            };
            let _ = sender.send(work().map_err(|error| format!("{error:#}")));
        })?;
    Ok(NavigationJob {
        kind,
        target,
        reached,
        loading,
        cancel,
        result,
    })
}

fn start_job(state: &mut AppState, kind: JobKind) -> Result<()> {
    let log = state
        .match_state
        .match_diagnostics
        .replay_log_arc()
        .context("Recording is unavailable")?;
    let nav = &mut state.match_state.match_diagnostics.navigation;
    nav.was_paused = nav.paused;
    let job = spawn_job(
        state
            .platform
            .game_config
            .as_ref()
            .context("Game settings are unavailable")?
            .paths
            .ra2_dir
            .clone(),
        log,
        nav.map_beside.clone(),
        kind,
    )?;
    nav.job = Some(job);
    nav.paused = true;
    state.match_state.input.cancel_tactical_drag();
    Ok(())
}
pub(crate) fn seek_bookmark(state: &mut AppState, index: usize) {
    let nav = &state.match_state.match_diagnostics.navigation;
    if !nav.validated || nav.job.is_some() || nav.browser {
        return;
    }
    let Some(bookmark) = nav.bookmarks.get(index).cloned() else {
        return;
    };
    if let Err(error) = start_job(state, JobKind::Seek(bookmark)) {
        state.match_state.match_diagnostics.navigation.notice =
            Some(format!("Seek failed: {error:#}"));
    }
}
pub(crate) fn cancel_seek(state: &mut AppState) {
    let nav = &mut state.match_state.match_diagnostics.navigation;
    if let Some(job) = nav.job.take() {
        let validating = matches!(job.kind, JobKind::Validate);
        drop(job);
        nav.paused = nav.was_paused;
        nav.notice = Some(
            if validating {
                "Recording validation canceled."
            } else {
                "Seek canceled; original position retained."
            }
            .into(),
        );
        if validating {
            nav.browser = true;
        }
    }
}

pub(crate) fn select_unit(state: &mut AppState, point: (f32, f32)) {
    if !state.match_state.match_diagnostics.navigation.validated
        || state.match_state.match_diagnostics.navigation.job.is_some()
    {
        return;
    }
    let id = state.match_state.sim_runtime.as_ref().and_then(|runtime| {
        let sim = &runtime.simulation;
        let order = crate::app::presentation::instances::tactical_entity_encounter_order(sim);
        let owner = crate::app::input::commands::preferred_local_owner_name(state);
        crate::app::input::entity_pick::pick_entity_at_point(
            crate::app::input::entity_pick::presented(state, sim),
            order.iter(),
            Some(sim.fog()),
            owner.as_deref(),
            point.0 / state.match_state.input.zoom_level + state.match_state.input.camera_x,
            point.1 / state.match_state.input.zoom_level + state.match_state.input.camera_y,
            Some(sim.interner()),
            sim.resolved_terrain(),
        )
    });
    state.match_state.match_diagnostics.navigation.selected = id;
    state.match_state.match_diagnostics.navigation.notice = None;
    state.match_state.input.follow_target = None;
}
pub(crate) fn toggle_follow(state: &mut AppState) {
    if state.match_state.input.follow_target.is_some() {
        state.match_state.input.follow_target = None;
    } else if let Some(id) = state.match_state.match_diagnostics.navigation.selected {
        state.match_state.input.follow_target = Some(id);
        crate::app::input::camera::update_follow_camera(state, &[]);
    }
}

pub(crate) fn target_released(state: &mut AppState, id: u64) {
    if state.match_state.match_diagnostics.is_replay_playback()
        && state.match_state.match_diagnostics.navigation.selected == Some(id)
    {
        state.match_state.match_diagnostics.navigation.notice =
            Some("Tracked unit no longer available".into());
    }
}

pub(crate) fn return_to_browser(state: &mut AppState) {
    cancel_seek(state);
    let mut paths = Vec::new();
    for directory in [
        state.platform.game_config.as_ref().map_or_else(
            || PathBuf::from("replays"),
            |config| config.paths.settings_dir().join("replays"),
        ),
        PathBuf::from("replays"),
    ] {
        if let Ok(entries) = std::fs::read_dir(directory) {
            paths.extend(
                entries
                    .filter_map(std::result::Result::ok)
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
                    })
                    .take(256),
            );
        }
    }
    paths.sort();
    paths.dedup();
    paths.truncate(256);
    let nav = &mut state.match_state.match_diagnostics.navigation;
    nav.recording_paths = paths;
    nav.browser = true;
    nav.paused = true;
}
pub(crate) fn close_browser(state: &mut AppState) {
    if state.match_state.match_diagnostics.navigation.validated {
        state.match_state.match_diagnostics.navigation.browser = false;
    } else {
        state.match_state.match_diagnostics.clear_replay_playback();
        crate::app::App::return_to_main_menu(state);
    }
}
pub(crate) fn open_recording(state: &mut AppState, index: usize) {
    let Some(path) = state
        .match_state
        .match_diagnostics
        .navigation
        .recording_paths
        .get(index)
        .cloned()
    else {
        return;
    };
    let request = || -> Result<_> {
        let (log, identity) = ReplayLog::load_bound(&path)?;
        let startup = log.standalone_startup()?;
        let map = super::frontend::launch::map_beside_replay(
            &path,
            &log.header.map_name,
            startup.map_sha256(),
        )?;
        Ok((Arc::new(log), identity, map))
    };
    match request() {
        Ok((log, identity, map)) => {
            if let Err(error) = begin_log_playback_with_map(state, log, identity, map) {
                state.match_state.match_diagnostics.navigation.notice =
                    Some(format!("Open recording failed: {error:#}"));
            }
        }
        Err(error) => {
            state.match_state.match_diagnostics.navigation.notice =
                Some(format!("Open recording failed: {error:#}"))
        }
    }
}
pub(crate) fn begin_log_playback(
    state: &mut AppState,
    log: Arc<ReplayLog>,
    identity: String,
) -> Result<()> {
    begin_log_playback_with_map(state, log, identity, None)
}
fn begin_log_playback_with_map(
    state: &mut AppState,
    log: Arc<ReplayLog>,
    identity: String,
    map_beside: Option<String>,
) -> Result<()> {
    let request = crate::app::loading::pump::LoadingRequest::replay(
        log.standalone_startup()?,
        map_beside.as_deref(),
    );
    crate::app::match_runtime::sim_tick::close_replay_segment_for_new_timeline(state);
    state
        .match_state
        .match_diagnostics
        .begin_playback(log, None, identity);
    state.match_state.match_diagnostics.navigation.paused = true;
    state.match_state.match_diagnostics.navigation.map_beside = map_beside;
    crate::app::loading::pump::begin_loading(state, request);
    Ok(())
}

pub(crate) fn service(state: &mut AppState) {
    if !state.match_state.match_diagnostics.is_replay_playback() {
        return;
    }
    let log = state.match_state.match_diagnostics.replay_log_arc();
    if let Some(log) = log.as_ref() {
        if !state
            .match_state
            .match_diagnostics
            .navigation
            .metadata_loaded
        {
            load_metadata(state, log);
        }
    }
    let should_validate = state
        .match_state
        .match_diagnostics
        .replay_cursor()
        .is_some()
        && !state
            .match_state
            .match_diagnostics
            .navigation
            .validation_started;
    if should_validate {
        state
            .match_state
            .match_diagnostics
            .navigation
            .validation_started = true;
        if let Err(error) = start_job(state, JobKind::Validate) {
            fail(state, format!("Replay validation failed: {error:#}"));
        }
    }
    let ready = state
        .match_state
        .match_diagnostics
        .navigation
        .job
        .as_ref()
        .and_then(|job| match job.result.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Replay worker stopped unexpectedly.".into()))
            }
        });
    if let Some(result) = ready {
        let job = state
            .match_state
            .match_diagnostics
            .navigation
            .job
            .take()
            .expect("completed replay job");
        match result {
            Ok(JobOutput::Validated) => {
                let nav = &mut state.match_state.match_diagnostics.navigation;
                nav.validated = true;
                nav.paused = nav.was_paused;
                nav.notice = Some("Recording validated.".into());
            }
            Ok(JobOutput::Destination(loaded, playback)) => {
                if let JobKind::Seek(bookmark) = &job.kind {
                    if let Err(error) = commit_destination(state, loaded, playback, bookmark) {
                        fail(state, format!("Seek failed: {error:#}"));
                    }
                }
            }
            Err(error) => fail(state, format!("Replay navigation failed: {error}")),
        }
    }
    if !state.match_state.match_diagnostics.navigation.browser
        && let Some(error) = state
            .match_state
            .match_diagnostics
            .replay_cursor()
            .and_then(|cursor| cursor.failure())
            .map(str::to_owned)
    {
        fail(state, format!("Replay failed: {error}"));
    }
    let selected = state.match_state.match_diagnostics.navigation.selected;
    if let Some(id) = selected {
        let alive = state
            .match_state
            .sim_runtime
            .as_ref()
            .and_then(|runtime| runtime.simulation.entities().get(id))
            .is_some_and(|entity| {
                entity.is_active() && entity.lifecycle.object_alive() && !entity.in_limbo()
            });
        if !alive {
            let nav = &mut state.match_state.match_diagnostics.navigation;
            nav.selected = None;
            state.match_state.input.follow_target = None;
        }
    }
    if state
        .match_state
        .match_diagnostics
        .replay_cursor()
        .is_none()
        && state.frontend.external_launch_error.is_some()
        && !state.match_state.match_diagnostics.navigation.browser
    {
        let error = state.frontend.external_launch_error.clone();
        return_to_browser(state);
        state.match_state.match_diagnostics.navigation.notice = error;
    }
}
pub(crate) fn loading_failed(state: &mut AppState, error: &str) {
    state.frontend.external_launch_error = None;
    state.frontend.screen = crate::ui::game_screen::GameScreen::InGame;
    crate::app::loading::transitions::reset_world_view_history(state);
    if state
        .match_state
        .match_presentation
        .selection_overlay
        .is_none()
    {
        state.match_state.match_presentation.selection_overlay =
            Some(crate::render::selection_overlay::SelectionOverlay::new(
                &state.renderer.gpu,
                &state.renderer.batch_renderer,
                None,
            ));
    }
    fail(state, format!("Replay loading failed: {error}"));
}

fn fail(state: &mut AppState, notice: String) {
    state.match_state.match_diagnostics.navigation.validated = false;
    return_to_browser(state);
    state.match_state.match_diagnostics.navigation.notice = Some(notice);
}

fn commit_destination(
    state: &mut AppState,
    mut loaded: crate::headless_scenario::HeadlessScenario,
    cursor: ReplayPlayback,
    bookmark: &ReplayBookmark,
) -> Result<()> {
    let hash = loaded.runtime.simulation.state_hash();
    state
        .process_assets
        .tile_variant_selector_cache
        .adopt_prepared(loaded.variant_selector_cache)
        .map_err(anyhow::Error::msg)?;
    let selector = state.process_assets.tile_variant_selector_cache.selector();
    let dummy = state.process_assets.shared_cell_dummy.clone();
    dummy.adopt_prepared_load_state(&loaded.runtime.simulation.effective_shared_cell_dummy());
    loaded
        .runtime
        .simulation
        .bind_shared_cell_dummy(dummy.clone());
    loaded
        .runtime
        .simulation
        .bind_tile_variant_selector(selector.clone());
    if let Some(template) = loaded.runtime.resources.terrain_template.as_mut() {
        template.bind_shared_cell_dummy(dummy);
        template.bind_tile_variant_selector(selector);
    }
    if let Some(sfx) = state.audio.sfx_player.as_mut() {
        sfx.stop_all();
    }
    state.match_state.match_audio.reset_for_new_match();
    loaded
        .runtime
        .simulation
        .install_fresh_process_main(&state.frontend.frontend_main_rng);
    loaded.runtime.simulation.publish_scenario_started();
    ensure!(
        loaded.runtime.simulation.state_hash() == hash,
        "Replay process adoption changed the destination hash"
    );
    state.match_state.sim_runtime = Some(loaded.runtime);
    state
        .match_state
        .match_diagnostics
        .replace_replay_cursor(cursor);
    let ui = std::mem::take(&mut state.match_state.match_presentation.replay_tools);
    crate::app::loading::transitions::reset_world_view_history(state);
    state.match_state.match_presentation.replay_tools = ui;
    rebuild_destination_view(state);
    crate::app::input::camera::set_view(state, bookmark.view);
    let nav = &mut state.match_state.match_diagnostics.navigation;
    nav.paused = true;
    nav.selected = bookmark.tracked_unit;
    nav.notice = Some("Bookmark reached.".into());
    state.match_state.input.follow_target = bookmark.tracked_unit;
    state.platform.frame_pacer.reset_for_new_world();
    crate::app::audio_runtime::bind_match_eva_context(state);
    crate::app::audio_runtime::bind_match_theme_context(state);
    Ok(())
}
fn rebuild_destination_view(state: &mut AppState) {
    let bridges = state
        .match_state
        .match_presentation
        .terrain_grid
        .as_ref()
        .and_then(|grid| grid.bridge_middle_tiles.clone());
    let config = state
        .match_state
        .match_presentation
        .lighting
        .saved_config()
        .clone();
    if let Some(runtime) = state.match_state.sim_runtime.as_ref() {
        let sim = &runtime.simulation;
        if let Some(terrain) = sim.resolved_terrain() {
            state.match_state.match_presentation.terrain_grid = Some(
                crate::map::terrain::build_terrain_grid_from_resolved(terrain, bridges),
            );
            let detail = state
                .match_state
                .match_presentation
                .in_game_options
                .detail_level;
            let lighting = crate::app::presentation::lighting::rebuild_lighting_grid_from_sim(
                terrain,
                &config,
                Some(sim),
                detail,
            );
            state.match_state.match_presentation.lighting.install(
                lighting,
                config,
                detail,
                Some((terrain, sim)),
                None,
            );
        }
        if let Some(overlays) = sim.overlay_grid() {
            state
                .match_state
                .match_presentation
                .overlays
                .replace_from_source(overlays.occupied_entries());
        }
    }
    if let Some(shroud) = state.match_state.match_presentation.shroud_buffer.as_mut() {
        shroud.mark_stale();
    }
    if let Some(minimap) = state.match_state.match_presentation.minimap.as_mut() {
        minimap.mark_stale();
    }
    crate::app::match_runtime::sim_tick::refresh_entity_atlases(state);
    crate::app::presentation::sidebar_render::refresh_sidebar_projection(state);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_with_zoom(zoom: f32) -> BookmarkFile {
        BookmarkFile {
            version: 1,
            recording: "r".into(),
            bookmarks: vec![ReplayBookmark {
                tick: 0,
                label: String::new(),
                view: CameraView {
                    center: [0.0; 2],
                    zoom,
                },
                tracked_unit: None,
            }],
        }
    }

    /// Bookmarks hold the main camera's view, so its whole wheel range loads;
    /// rejecting one bookmark drops the file and the next save overwrites it.
    #[test]
    fn bookmarks_accept_the_main_camera_zoom_range() {
        assert!(valid_bookmarks(&file_with_zoom(0.25), "r", 0));
        assert!(valid_bookmarks(&file_with_zoom(4.0), "r", 0));
        assert!(!valid_bookmarks(&file_with_zoom(4.5), "r", 0));
        assert!(!valid_bookmarks(&file_with_zoom(f32::NAN), "r", 0));
    }
}
