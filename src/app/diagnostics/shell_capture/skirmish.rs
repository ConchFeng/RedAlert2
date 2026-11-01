//! Production-only Skirmish capture route. No shell state or renderer is cloned.

use super::*;
use crate::app::App;
use serde_json::{Value, json};

mod commanders;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresentedShell {
    Other,
    MainMenu,
    SinglePlayer,
    MoviesAndCredits,
    MovieList,
    FullscreenMovie,
    CreditsRoll,
    Skirmish,
    Campaign,
    LoadSavedGame,
    Options,
    NetworkOptions,
    LanLobby,
    NetworkSetup,
    WolWelcome,
    WolLogin,
    WolLobby,
    WolOptions,
    WolSoundOptions,
    WolAutoLoginQuery,
    WolLocale,
    WolMyInformation,
    WolBuddyList,
    WolQuickMatch,
    WolAddBuddyQuery,
    Score,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    #[default]
    MainMenu,
    SinglePlayer,
    Skirmish,
    AiCommandersSetup,
    AiSeriesSetup,
    AiSeriesRecovery,
    /// Back pressed: the teardown slide runs toward the held tick.
    SlideOut,
    /// Choose Map pressed: `0x102` slides out and `0x6B` slides in.
    Chooser,
    /// Cancel pressed on `0x6B`: it slides out and `0x102` slides in again.
    ChooserReturn,
    /// Create Random Map pressed: `0x6B` slides out and `0x105` runs.
    RandomMap,
    /// Cancel pressed on `0x105`: it slides out and `0x6B` slides in again.
    RandomMapCancel,
    /// The hidden chooser's eject box after `0x105`'s Use Map was answered.
    RandomMapEject,
    /// Start Game pressed: `0x102` slides out and the scenario loads.
    Starting,
    /// The game's Leave was confirmed: the abort exit runs and the shell
    /// resumes.
    Quitting,
    /// In the game: the MCV deploys, then the Construction Yard is sold.
    Defeating,
    /// The score page `0x108` is up after the defeat.
    Scoring,
    BattlefieldInspection,
    Replaying,
    /// Tab opened Diplomacy `0x73` over the running game.
    Diplomacy,
    /// Continue was pressed on the settled score page.
    Continuing,
}

/// What a Start Game checkpoint captures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LoadingTarget {
    /// The black frame that replaces the closed shell.
    Blank,
    /// The loading screen's first frame; the map load then runs to the end.
    FirstFrame,
    /// The game runs [`QUIT_AFTER_FRAMES`] frames, then Leave through the
    /// in-game abort; the new `0x102` settled after its entry slide.
    AfterQuit,
    /// A real defeat: the local MCV deploys and its Construction Yard is sold
    /// through the production commands; the score page settled.
    DefeatScore,
    /// The same, then Continue: the new `0x102` settled.
    DefeatContinue,
    DefeatReplay,
    /// The game runs [`QUIT_AFTER_FRAMES`] frames, then Tab opens the
    /// Diplomacy dialog `0x73` through the production key path; it settled.
    Diplomacy,
}

/// In-game frames before the quit route presses Leave.
const QUIT_AFTER_FRAMES: u32 = 30;

/// Start Game's centre at 800x600 (the retail helper's `sk-hover-start.png`).
const START_GAME_POINT: (i32, i32) = (720, 262);

/// What a Choose Map checkpoint captures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChooserTarget {
    /// `0x6B` settled after its entry slide.
    Steady,
    /// `0x6B`'s entry slide held at a tick.
    Entry(u32),
    /// Cancel on `0x6B`, then `0x102` settled after its new entry slide.
    Return,
    /// Use Map on the list's first map while AI rows would not fit it: the
    /// eject box over the empty backdrop.
    Eject,
    /// Create Random Map, then the random-map dialog `0x105`.
    RandomMap(RandomMapTarget),
}

/// What a random-map dialog checkpoint captures. The route hovers Create
/// Random Map before pressing it, as the retail helper's click does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RandomMapTarget {
    /// `0x105` settled after its entry slide.
    Steady,
    /// `0x105`'s entry slide held at a tick.
    Entry(u32),
    /// `0x105` settled with the pointer resting here.
    Hover((i32, i32)),
    /// Cancel: `0x105` slides out and `0x6B`'s entry slide is held at a tick.
    CancelEntry(u32),
    /// Cancel: `0x6B` settled again.
    CancelSteady,
    /// Use Map: the map generates, `0x105` slides out, the chooser's Use Map
    /// selects it and `0x102` settles with the random map.
    UseMap,
    /// Generate Map, then Save Map opens the seed browser over `0x105`; its
    /// Back uncovers `0x105`, which must not slide in again.
    SeedBrowserReturn,
    /// Terrain Type's list open, `0x105` settled with the pointer below the
    /// list on Map Size's face. The list holds the mouse, so the status line
    /// keeps Terrain Type's help.
    ListOpen,
    /// AI row 2 takes Easy on `0x102` and Players 2 on `0x105`, then Generate
    /// Map and Use Map: `0x105` slides out and the hidden chooser's Use Map
    /// asks to eject AI players while no dialog shows or slides. Cancel slides
    /// the chooser in and it settles; OK closes it without a slide and `0x102`
    /// settles with the random map.
    Eject(crate::ui::skirmish_shell::EjectPromptButton),
    /// The same route's eject box, presented over no dialog.
    EjectBox,
}

/// Create Random Map on `0x6B` and Cancel on `0x105` at 800x600.
pub(super) const CREATE_RANDOM_MAP_POINT: (i32, i32) = (720, 262);
const RANDOM_MAP_CANCEL_POINT: (i32, i32) = (720, 556);
pub(super) const RANDOM_MAP_USE_MAP_POINT: (i32, i32) = (720, 220);
pub(super) const RANDOM_MAP_GENERATE_POINT: (i32, i32) = (432, 430);
/// Terrain Type's arrow, and Map Size's face below its open list, at 800x600
/// (the retail helper's `ej-list-open.png` click and `ej-list-off.png` point).
const RANDOM_MAP_TERRAIN_ARROW_POINT: (i32, i32) = (484, 79);
const RANDOM_MAP_MAP_SIZE_FACE_POINT: (i32, i32) = (350, 196);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AiSeriesTarget {
    Setup,
    Start,
    Results,
    Recovery,
}

#[derive(Debug, Clone, Copy)]
enum ObserverCameraStep {
    Enable,
    WaitingForBattle {
        initial_camera: (f32, f32),
    },
    Manual {
        camera: (f32, f32),
    },
    WaitingForResume {
        manual_camera: (f32, f32),
        resumed_ms: u64,
    },
    Done,
}

#[derive(Default)]
pub(super) struct SkirmishCapture {
    phase: Phase,
    route: Vec<Value>,
    selected_scene: Option<Value>,
    last_presented: Option<PresentedShell>,
    /// Capture Back's teardown slide held at this tick instead of the steady
    /// dialog.
    slide_out_tick: Option<u32>,
    /// Capture the entry slide held at this tick instead of the steady dialog.
    entry_tick: Option<u32>,
    entry_seen: bool,
    entry_held: bool,
    chooser: Option<ChooserTarget>,
    /// The steady chooser got the production mouse move at the resting
    /// pointer, as the retail helper's pointer rests over the map list.
    pointer_rested: bool,
    /// Once `0x102` settles, rest the pointer here through the production
    /// mouse move.
    hover: Option<(i32, i32)>,
    hovered: bool,
    /// The random-map routes rest the pointer on Create Random Map until the
    /// chooser's help has typed in before pressing it: the retail helper's
    /// click rests 100 ms before its press and 90 ms more before its release,
    /// and `rmg-cancel.png` shows that help whole.
    create_random_map_hovered: bool,
    /// The random-map Use Map or seed-browser route has pressed Generate Map.
    generate_pressed: bool,
    /// The seed-browser route has pressed Save Map.
    save_pressed: bool,
    /// The eject route has set AI row 2, then Players 2, and has seen its
    /// box with no dialog behind it.
    second_ai_added: bool,
    players_set: bool,
    eject_seen: bool,
    /// The dialog the eject box's answer shows got the production mouse move
    /// at the resting pointer once its slide had run.
    answer_rested: bool,
    loading: Option<LoadingTarget>,
    in_game_frames: u32,
    /// Frames Diplomacy has presented.
    diplomacy_frames: u32,
    deploy_sent: bool,
    sell_sent: bool,
    battlefield_inspection: Option<(crate::ui::score_shell::ScoreScreenModel, (u64, u32, u64))>,
    battlefield_inspected: bool,
    default_observer_setup: bool,
    replay_setup: Option<(u64, crate::skirmish_launch::SkirmishLaunchSession)>,
    replay_completed: bool,
    observer_client: bool,
    observer_initial_entities: Option<usize>,
    observer_focused: bool,
    observer_battlefield_ready: bool,
    observer_camera_step: Option<ObserverCameraStep>,
    observer_standings: bool,
    ai_commanders: Option<commanders::CommanderCapture>,
    ai_series_target: Option<AiSeriesTarget>,
    ai_series_setup_presented: bool,
    ai_series_map_identity: Option<(String, String, usize)>,
    ai_series_repository: Option<PathBuf>,
    ai_series_recovery_files: Option<Vec<(PathBuf, String)>>,
    ai_series_terminal_outcomes: Vec<Value>,
    ai_series_first_result_at: Option<Instant>,
    ai_series_rematch_cursor: Option<(f32, f32)>,
    ai_series_second_round_seen: bool,
}

#[derive(Clone, Copy)]
struct CaptureGuard {
    main_menu_screen: bool,
    surface: (u32, u32),
    expected_surface: (u32, u32),
    failed: bool,
    developer_shortcut: bool,
    software_cursor: bool,
    cursor: (f32, f32),
    expected_cursor: (f32, f32),
    interaction_active: bool,
}

impl CaptureGuard {
    fn validate(self) -> Result<()> {
        ensure!(self.main_menu_screen, "skirmish capture left the shell");
        ensure!(
            self.surface == self.expected_surface,
            "skirmish capture surface changed"
        );
        ensure!(
            !self.failed && !self.developer_shortcut,
            "skirmish capture encountered fallback or a developer shortcut"
        );
        ensure!(self.software_cursor, "software cursor unavailable");
        ensure!(
            self.cursor == self.expected_cursor,
            "capture cursor moved: actual={:?}, expected={:?}",
            self.cursor,
            self.expected_cursor
        );
        ensure!(
            !self.interaction_active,
            "skirmish capture encountered modal, editing or interaction state"
        );
        Ok(())
    }
}

fn guard(
    state: &AppState,
    chooser: Option<ChooserTarget>,
    loading: bool,
    expected_cursor: (f32, f32),
    expected_surface: (u32, u32),
) -> Result<()> {
    let shell = &state.frontend.skirmish_shell_state;
    CaptureGuard {
        main_menu_screen: state.frontend.screen == GameScreen::MainMenu
            || (loading
                && matches!(
                    state.frontend.screen,
                    GameScreen::Loading | GameScreen::InGame | GameScreen::MissionResult { .. }
                )),
        surface: (state.render_width(), state.render_height()),
        expected_surface,
        failed: state.frontend.main_menu_shell_error.is_some(),
        developer_shortcut: false,
        software_cursor: state.use_software_cursor(),
        // The game moves the pointer; the quit route puts it back at rest
        // when the shell returns (and its readiness requires that).
        cursor: if loading && state.frontend.screen != GameScreen::Loading {
            (EXPECTED_CURSOR_X as f32, EXPECTED_CURSOR_Y as f32)
        } else {
            (
                state.match_state.input.cursor_x,
                state.match_state.input.cursor_y,
            )
        },
        expected_cursor,
        interaction_active: state.main_menu_dialog_open()
            || state.frontend.quit_cascade.is_some()
            || (shell.choose_map_modal.is_some() && chooser.is_none())
            || (shell.random_map_setup_modal.is_some()
                && !matches!(chooser, Some(ChooserTarget::RandomMap(_))))
            || (!matches!(
                chooser,
                Some(
                    ChooserTarget::Eject
                        | ChooserTarget::RandomMap(
                            RandomMapTarget::Eject(_) | RandomMapTarget::EjectBox
                        )
                )
            ) && shell
                .choose_map_modal
                .as_ref()
                .is_some_and(|modal| modal.eject_prompt.is_some()))
            || shell.validation_modal.is_some()
            || (shell.saved_seed_browser.is_some()
                && chooser != Some(ChooserTarget::RandomMap(RandomMapTarget::SeedBrowserReturn)))
            || (state.frontend.random_map_generation.is_some()
                && !matches!(
                    chooser,
                    Some(ChooserTarget::RandomMap(
                        RandomMapTarget::UseMap
                            | RandomMapTarget::SeedBrowserReturn
                            | RandomMapTarget::Eject(_)
                            | RandomMapTarget::EjectBox
                    ))
                ))
            || shell.open_combo_dropdown.is_some()
            || shell.trackbar_hold.is_some()
            || shell.dropdown_scroll_drag.is_some()
            || shell.dropdown_scroll_press.is_some()
            || shell.pressed_owner_draw_button.is_some()
            || shell.player_name_edit.focused,
    }
    .validate()
}

fn selected_scene(state: &AppState) -> Result<Value> {
    let shell = &state.frontend.skirmish_shell_state;
    let map = state
        .frontend
        .scenario_catalog
        .shell_maps()
        .get(shell.selected_map_idx)
        .context("skirmish capture has no selected map")?;
    let preview = state
        .frontend
        .skirmish_preview_texture
        .as_ref()
        .context("selected map preview is unavailable for this capture checkpoint")?;
    ensure!(
        preview.selected_map_idx == shell.selected_map_idx
            && preview.setup_preview_generation.is_none(),
        "capture preview belongs to another selection"
    );
    Ok(json!({
        "map_file": map.file_name, "map_label": map.display_name,
        "map_capacity": map.player_capacity, "mode_id": shell.selected_mode_id,
        "preview": { "width": preview.width, "height": preview.height },
        "parsed_map_sha256": crate::util::sha256::sha256_hex(format!("{map:?}").as_bytes()),
        "parsed_map_hash_encoding": "Rust Debug representation; diagnostic identity only",
        "player": {
            "name": shell.player_name_edit.text, "country": format!("{:?}", shell.player_country),
            "country_random": shell.player_country_random, "color": shell.player_color_index,
            "color_claimed": shell.player_color_claimed,
            "start": format!("{:?}", shell.player_start_position), "team": shell.player_team
        },
        "opponents": shell.opponents.iter().enumerate().map(|(index, row)| json!({
            "visible": crate::ui::skirmish_shell::player_row_visible(shell, state.frontend.scenario_catalog.shell_maps(), index + 1),
            "enabled": row.enabled, "type": format!("{:?}", row.row_type),
            "country": format!("{:?}", row.country), "country_random": row.country_random,
            "color": row.color_index, "color_claimed": row.color_claimed,
            "start": format!("{:?}", row.start_position), "team": row.team
        })).collect::<Vec<_>>(),
        "options": {
            "credits": shell.starting_credits, "speed": shell.game_speed, "units": shell.unit_count,
            "short_game": shell.short_game, "super_weapons": shell.super_weapons,
            "build_off_ally": shell.build_off_ally, "crates": shell.crates,
            "mcv_redeploy": shell.mcv_redeploy
        }
    }))
}

/// The tick of `kind`'s entry slide, while it runs.
fn entry_wave_tick(state: &AppState, kind: ShellSlideKind) -> Option<u32> {
    if state.frontend.shell_slide_active_shell != Some(kind) {
        return None;
    }
    state
        .frontend
        .shell_first_paint_slide
        .as_ref()
        .and_then(|wave| wave.compatibility_tick())
}

impl SkirmishCapture {
    pub(super) fn slide_out(tick: u32) -> Self {
        Self {
            slide_out_tick: Some(tick),
            ..Self::default()
        }
    }

    pub(super) fn entry(tick: u32) -> Self {
        Self {
            entry_tick: Some(tick),
            ..Self::default()
        }
    }

    pub(super) fn chooser(target: ChooserTarget) -> Self {
        Self {
            chooser: Some(target),
            ..Self::default()
        }
    }

    pub(super) fn loading(target: LoadingTarget) -> Self {
        Self {
            loading: Some(target),
            ..Self::default()
        }
    }

    pub(super) fn hover(point: (i32, i32)) -> Self {
        Self {
            hover: Some(point),
            ..Self::default()
        }
    }

    pub(super) fn observer_generated_battlefield() -> Self {
        Self {
            observer_client: true,
            chooser: Some(ChooserTarget::RandomMap(RandomMapTarget::UseMap)),
            ..Self::default()
        }
    }

    pub(super) fn observer_camera() -> Self {
        Self {
            observer_camera_step: Some(ObserverCameraStep::Enable),
            ..Self::observer_generated_battlefield()
        }
    }

    pub(super) fn observer_standings() -> Self {
        Self {
            observer_standings: true,
            ..Self::observer_generated_battlefield()
        }
    }

    pub(super) fn ai_commanders(width: u32, height: u32) -> Self {
        Self {
            ai_commanders: Some(commanders::CommanderCapture::new((width, height))),
            ..Self::observer_generated_battlefield()
        }
    }

    pub(super) fn ai_series(target: AiSeriesTarget) -> Self {
        Self {
            ai_series_target: Some(target),
            ..Self::observer_generated_battlefield()
        }
    }

    fn ai_series_click(state: &mut AppState, point: (i32, i32)) -> Result<()> {
        Self::set_pointer(state, point);
        crate::app::input::ai_series::cursor_moved(state);
        for pressed in [true, false] {
            ensure!(
                crate::app::input::ai_series::mouse(
                    state,
                    winit::event::MouseButton::Left,
                    pressed
                ),
                "AI Series input owner did not consume its visible control"
            );
        }
        Self::set_pointer(state, (EXPECTED_CURSOR_X as i32, EXPECTED_CURSOR_Y as i32));
        crate::app::input::ai_series::cursor_moved(state);
        Ok(())
    }

    fn ai_series_control(
        state: &mut AppState,
        control: crate::ui::ai_series::AiSeriesControl,
    ) -> Result<(i32, i32)> {
        let layout = crate::app::frontend::ai_series_render::layout(state);
        let rect = layout
            .control_rect(control)
            .context("AI Series control is hidden")?;
        let point = (rect.x + rect.w / 2, rect.y + rect.h / 2);
        ensure!(
            layout.control_at(point.0, point.1) == Some(control),
            "AI Series controls overlap"
        );
        Self::ai_series_click(state, point)?;
        Ok(point)
    }

    fn open_ai_series(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        if self.ai_series_target == Some(AiSeriesTarget::Results) {
            let shell = &mut state.frontend.skirmish_shell_state;
            shell.unit_count = 0;
            shell.launch_options_base.bases = false;
            self.route.push(json!({"screen":"skirmish", "frame":frame, "action":"ConfigureNaturalEmptyArmyFixture", "unit_count":0, "bases":false, "outcome_override":false}));
        }
        let rect = crate::app::frontend::ai_series_render::entry_rect(state)
            .context("AI Series entry is unavailable after accepted observer map setup")?;
        Self::ai_series_click(state, (rect.x + rect.w / 2, rect.y + rect.h / 2))?;
        let setup = state
            .frontend
            .ai_series_setup
            .as_ref()
            .context("AI Series form did not open")?;
        let draft = setup.validated_setup().map_err(anyhow::Error::msg)?;
        let selected = state.frontend.skirmish_shell_state.selected_map_idx;
        let map = setup
            .maps
            .iter()
            .find(|map| map.record_index == selected)
            .context("current generated map is absent from AI Series choices")?;
        ensure!(
            setup.entrant_count == 29
                && draft.match_count == 5
                && map.capacity == 30
                && draft.map_indices == [selected],
            "AI Series defaults did not retain 29 AIs, 30 capacity, five matches, and current map"
        );
        let frozen = state
            .frontend
            .random_map_retention
            .frozen_acceptance(Some(
                &state.frontend.scenario_catalog.shell_maps()[selected].file_name,
            ))
            .context("accepted generated definition is unavailable to AI Series")?;
        let digest = crate::util::sha256::sha256_hex(frozen.seed_bytes());
        self.ai_series_map_identity = Some((
            frozen.selected_map_file().to_owned(),
            digest.clone(),
            frozen.seed_bytes().len(),
        ));
        self.route.push(json!({"screen":"AI Series setup", "frame":frame, "action":"OpenAiSeries", "entrants":setup.entrant_count, "capacity":map.capacity, "matches":draft.match_count, "map_label":map.label, "selected_maps":draft.map_indices, "seed_source_sha256":digest}));
        self.phase = Phase::AiSeriesSetup;
        Ok(())
    }

    fn start_ai_series(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        use crate::ui::ai_series::AiSeriesControl;
        for _ in 0..3 {
            Self::ai_series_control(state, AiSeriesControl::Decrease)?;
        }
        Self::ai_series_control(state, AiSeriesControl::Advanced)?;
        Self::ai_series_control(state, AiSeriesControl::FirstSeed)?;
        let setup = state
            .frontend
            .ai_series_setup
            .as_mut()
            .context("AI Series form closed before Start")?;
        setup
            .first_seed
            .set_units_limited("1234".encode_utf16(), 10);
        let draft = setup.validated_setup().map_err(anyhow::Error::msg)?;
        ensure!(
            draft.match_count == 2 && draft.first_seed == 1234,
            "AI Series input did not configure two matches and seed 1234"
        );
        let previous = if self.ai_series_target == Some(AiSeriesTarget::Results) {
            Self::series_directories(state)?
        } else {
            Vec::new()
        };
        let start = Self::ai_series_control(state, AiSeriesControl::Start)?;
        ensure!(
            crate::app::ai_series::in_progress(state) && state.frontend.ai_series_setup.is_none(),
            "Start Series did not create an active series: {:?}",
            state
                .frontend
                .ai_series_setup
                .as_ref()
                .and_then(|setup| setup.error.as_deref())
        );
        if self.ai_series_target == Some(AiSeriesTarget::Results) {
            let created: Vec<_> = Self::series_directories(state)?
                .into_iter()
                .filter(|path| !previous.contains(path))
                .collect();
            ensure!(
                created.len() == 1,
                "Start Series did not publish exactly one new durable series"
            );
            self.ai_series_repository = created.into_iter().next();
        }
        self.route.push(json!({"screen":"AI Series setup", "frame":frame, "action":"StartSeries", "matches":2, "first_seed":1234, "start_point":start, "count_input":"three Decrease press/release pairs", "seed_input":"focused UTF16 edit assignment", "status":crate::app::ai_series::status_text(state)}));
        self.phase = Phase::Starting;
        Ok(())
    }

    fn observe_ai_series_started(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        if self.observer_battlefield_ready {
            return Ok(());
        }
        self.observe_generated_battlefield(state, frame)?;
        let sim = &state
            .match_state
            .sim_runtime
            .as_ref()
            .context("AI Series world missing")?
            .simulation;
        if !self.observer_focused || sim.session().tick() < 30 {
            return Ok(());
        }
        ensure!(
            sim.session().started() && sim.session().seed() == 1234,
            "AI Series did not install its controlled first seed"
        );
        let (source_path, digest) = self.series_source_identity(state)?;
        let identity = Self::simulation_identity(state)?;
        let status_before = crate::app::ai_series::status_text(state);
        ensure!(
            status_before.contains("Match 1 of 2") && !status_before.contains("stopping"),
            "AI Series running status is incorrect: {status_before}"
        );
        let point = Self::spectator_button(
            state,
            crate::app::presentation::spectator_tools::SpectatorToolAction::StopSeriesAfterMatch,
        )?;
        let status = crate::app::ai_series::status_text(state);
        ensure!(
            status.contains("stopping after this match")
                && Self::simulation_identity(state)? == identity,
            "Stop After This Match changed gameplay or failed to update series state"
        );
        self.route.push(json!({"screen":"in game", "frame":frame, "action":"AiSeriesStartedAndStopAfterMatch", "matches":2, "seed":1234, "tick":identity.0, "binary_frame":identity.1, "state_hash":format!("{:016x}",identity.2), "seed_path":source_path, "seed_source_sha256":digest, "live_ai":29, "observer_full_map":true, "stop_point":point, "simulation_unchanged":true, "status":status}));
        self.observer_battlefield_ready = true;
        Ok(())
    }

    fn series_source_identity(&self, state: &AppState) -> Result<(String, String)> {
        let (expected_name, expected_digest, expected_len) =
            self.ai_series_map_identity
                .as_ref()
                .context("accepted series map identity missing")?;
        let Some(crate::map::source::LoadedMapSource::Generated {
            seed_name,
            seed_source: Some(source),
        }) = state.match_state.loaded_map_source.as_ref()
        else {
            bail!("AI Series did not retain a generated seed source");
        };
        ensure!(
            seed_name.eq_ignore_ascii_case(expected_name)
                && source.source_sha256 == *expected_digest
                && source.payload_len == *expected_len,
            "AI Series consumed a different generated definition"
        );
        let expected_path = state
            .platform
            .game_config
            .as_ref()
            .context("game config missing")?
            .paths
            .ra2_dir
            .join(expected_name);
        ensure!(
            source.path == expected_path,
            "AI Series generated source path changed"
        );
        let source_path = source.path.display().to_string();
        let digest = source.source_sha256.clone();
        Ok((source_path, digest))
    }

    fn series_directories(state: &AppState) -> Result<Vec<PathBuf>> {
        let root = state
            .platform
            .game_config
            .as_ref()
            .context("game config missing")?
            .paths
            .settings_dir()
            .join("ai-series");
        let entries = match fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        entries
            .map(|entry| Ok(entry?.path()))
            .filter(|entry: &Result<PathBuf>| entry.as_ref().map_or(true, |path| path.is_dir()))
            .collect()
    }

    fn read_finished_series_records(&self) -> Result<(Value, Vec<Value>)> {
        let directory = self
            .ai_series_repository
            .as_ref()
            .context("series repository missing")?;
        let descriptor: Value = serde_json::from_slice(&fs::read(directory.join("series.json"))?)?;
        let entrants = descriptor["entrants"]
            .as_array()
            .context("series entrants missing")?;
        ensure!(
            descriptor["round_limit"] == 2
                && descriptor["seeds"] == json!([1234, 1235])
                && entrants.len() == 29,
            "durable series schedule differs from the UI"
        );
        let files: Vec<_> = fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|name| {
                name.to_str().is_some_and(|name| {
                    name.starts_with("round-")
                        && name.ends_with(".json")
                        && !name.ends_with("-launch.json")
                })
            })
            .collect();
        ensure!(
            files.len() == 2,
            "series did not publish exactly two finished round records"
        );
        let mut records = Vec::new();
        for round in 1..=2 {
            let record: Value = serde_json::from_slice(&fs::read(
                directory.join(format!("round-{round:04}.json")),
            )?)?;
            ensure!(
                record["series_id"] == descriptor["id"]
                    && record["round"] == round
                    && record["seed"] == 1233 + round
                    && record["map_index"] == 0,
                "finished round identity differs from its natural terminal match"
            );
            records.push(record);
        }
        Ok((descriptor, records))
    }

    fn verify_finished_series_records(&self) -> Result<Vec<Value>> {
        let (descriptor, records) = self.read_finished_series_records()?;
        let entrants = descriptor["entrants"]
            .as_array()
            .context("series entrants missing")?;
        ensure!(
            self.ai_series_terminal_outcomes.len() == 2,
            "series did not produce two natural terminal snapshots"
        );
        for (record, terminal) in records.iter().zip(&self.ai_series_terminal_outcomes) {
            let survivors = terminal["survivors"]
                .as_array()
                .context("terminal survivors missing")?;
            match record["outcome"]["kind"].as_str() {
                Some("winner") => {
                    let slot = &record["outcome"]["slot_index"];
                    let winner = entrants
                        .iter()
                        .find(|entrant| entrant["slot_index"] == *slot)
                        .context("recorded winner is not a series entrant")?;
                    ensure!(
                        survivors.len() == 1 && survivors[0] == winner["name"],
                        "recorded winner differs from the actual terminal survivor"
                    );
                }
                Some("draw") => ensure!(
                    survivors.len() != 1,
                    "draw record discarded a sole natural winner"
                ),
                _ => bail!("an abort, skip, or incomplete result was counted as finished"),
            }
        }
        Ok(records)
    }

    fn series_disk_snapshot(state: &AppState) -> Result<Vec<(PathBuf, String)>> {
        let mut files = Vec::new();
        for directory in Self::series_directories(state)? {
            for entry in fs::read_dir(directory)? {
                let path = entry?.path();
                ensure!(path.is_file(), "unexpected nested series history directory");
                let digest = crate::util::sha256::sha256_hex(&fs::read(&path)?);
                files.push((path, digest));
            }
        }
        files.sort_unstable();
        Ok(files)
    }

    fn recover_ai_series(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        ensure!(
            state.match_state.sim_runtime.is_none(),
            "recovery capture is not a fresh process"
        );
        ensure!(
            state
                .frontend
                .ai_series_setup
                .as_ref()
                .is_some_and(|setup| setup.recovery_available),
            "Recover Last is unavailable for the injected completed history"
        );
        let directories = Self::series_directories(state)?;
        ensure!(
            directories.len() == 1,
            "recovery capture requires one existing completed series"
        );
        self.ai_series_repository = directories.into_iter().next();
        let (_, records) = self.read_finished_series_records()?;
        ensure!(
            records
                .iter()
                .all(|record| record["outcome"]["kind"] == "draw"),
            "recovery fixture must be the two naturally completed draw records"
        );
        self.ai_series_recovery_files = Some(Self::series_disk_snapshot(state)?);
        let point =
            Self::ai_series_control(state, crate::ui::ai_series::AiSeriesControl::RecoverLast)?;
        ensure!(
            state.frontend.ai_series_setup.is_none() && crate::app::ai_series::in_progress(state),
            "Recover Last did not restore the saved series"
        );
        self.route.push(json!({"screen":"AI Series setup", "frame":frame, "action":"RecoverLastSeries", "point":point, "records":records, "new_series_started":false}));
        self.phase = Phase::AiSeriesRecovery;
        Ok(())
    }

    fn observe_ai_series_recovery(&mut self, state: &AppState, frame: u32) -> Result<()> {
        ensure!(
            state.match_state.sim_runtime.is_none(),
            "history recovery installed a gameplay simulation"
        );
        let status = crate::app::ai_series::status_text(state);
        ensure!(
            status.contains("Series complete"),
            "completed series did not recover as complete: {status}"
        );
        ensure!(
            state.frontend.score_art.is_some() && App::score_shell_active(state),
            "recovered history did not use the native score artwork and renderer"
        );
        let page = state
            .frontend
            .score_page
            .as_ref()
            .context("recovered standings model missing")?;
        ensure!(
            page.model.rows.len() == 29
                && page.model.rows.iter().all(|row| row.built == 2
                    && row.losses == 2
                    && row.kills == 0
                    && row.score == 0),
            "recovered standings differ from two draws for every AI entrant"
        );
        let (_, records) = self.read_finished_series_records()?;
        ensure!(
            records
                .iter()
                .all(|record| record["outcome"]["kind"] == "draw"),
            "recovered records differ from the saved natural draws"
        );
        let files = Self::series_disk_snapshot(state)?;
        ensure!(
            self.ai_series_recovery_files.as_ref() == Some(&files),
            "Recover Last modified history or published a new launch"
        );
        if !Self::score_settled(state) || self.observer_battlefield_ready {
            return Ok(());
        }
        self.route.push(json!({"screen":"recovered series standings", "frame":frame, "action":"RecoveredCompletedSeriesStandings", "status":status, "records":records, "entrants":page.model.rows.len(), "history_files":files.len(), "history_unchanged":true, "simulation_present":false, "score_art_present":true}));
        self.observer_battlefield_ready = true;
        Ok(())
    }

    fn observe_ai_series_results(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        if self.observer_battlefield_ready {
            return Ok(());
        }
        let status = crate::app::ai_series::status_text(state);
        ensure!(
            !status.contains("Incomplete"),
            "natural series capture failed: {status}"
        );
        let sim = &state
            .match_state
            .sim_runtime
            .as_ref()
            .context("natural series world missing")?
            .simulation;
        let seed = sim.session().seed();
        ensure!(
            [1234, 1235].contains(&seed),
            "natural series installed an unexpected seed"
        );
        ensure!(
            sim.session().tick() < 1800 || sim.terminal_score_snapshot().is_some(),
            "zero-army match did not reach a natural terminal outcome before 1800 ticks"
        );
        ensure!(
            sim.session().game_options().unit_count == 0 && !sim.session().game_options().bases,
            "zero-army diagnostic options did not reach the active match"
        );
        let observer = sim
            .session()
            .current_house()
            .context("series observer missing")?;
        ensure!(
            sim.session().is_observer_house(observer)
                && sim.fog().whole_map_revealed_for_owner(observer),
            "natural series did not retain its full-map observer"
        );
        let (_, digest) = self.series_source_identity(state)?;
        if seed == 1235 && !self.ai_series_second_round_seen {
            let started = self
                .ai_series_first_result_at
                .context("round two began without first natural results")?;
            ensure!(
                started.elapsed() >= Duration::from_secs(9),
                "round two bypassed the actual countdown"
            );
            self.ai_series_second_round_seen = true;
            self.route.push(json!({"screen":"in game", "frame":frame, "action":"AutomaticSeriesRoundTwoInstalled", "seed":seed, "tick":sim.session().tick(), "seed_source_sha256":digest, "countdown_observed_ms":started.elapsed().as_millis(), "start_now_pressed":false}));
        }
        if seed == 1234
            && self
                .ai_series_first_result_at
                .is_some_and(|started| started.elapsed() > Duration::from_secs(60))
        {
            bail!("the real series countdown did not launch round two within 60 seconds: {status}");
        }
        if !App::score_shell_active(state) {
            return Ok(());
        }
        let round = if seed == 1234 { 1 } else { 2 };
        if self.ai_series_terminal_outcomes.len() < round {
            let snapshot = sim
                .terminal_score_snapshot()
                .context("score page appeared without a natural terminal snapshot")?;
            ensure!(
                snapshot.rows.len() == 29,
                "natural score snapshot lost AI entrants"
            );
            let survivors: Vec<_> = snapshot
                .rows
                .iter()
                .filter(|row| row.survived)
                .map(|row| sim.interner().resolve(row.owner))
                .collect();
            let fact = json!({"round":round, "seed":seed, "tick":sim.session().tick(), "binary_frame":sim.session().binary_frame(), "survivors":survivors, "terminal_snapshot":snapshot});
            self.route.push(json!({"screen":"natural match results", "frame":frame, "action":"NaturalSeriesRoundFinished", "terminal":fact, "status":status}));
            self.ai_series_terminal_outcomes.push(fact);
            if round == 1 {
                self.ai_series_rematch_cursor = Some((
                    state.match_state.input.cursor_x,
                    state.match_state.input.cursor_y,
                ));
                ensure!(
                    status.contains("Next match in"),
                    "first results did not enter automatic countdown: {status}"
                );
                self.ai_series_first_result_at = Some(Instant::now());
            }
        }
        if round != 2 || !status.contains("Series complete") || !Self::score_settled(state) {
            return Ok(());
        }
        ensure!(
            self.ai_series_second_round_seen,
            "round two was never installed"
        );
        let records = self.verify_finished_series_records()?;
        let identity = Self::simulation_identity(state)?;
        self.click_score_action(state, crate::app::ai_series::TOGGLE_STANDINGS)?;
        ensure!(
            Self::simulation_identity(state)? == identity,
            "opening series standings changed the terminal simulation"
        );
        let page = state
            .frontend
            .score_page
            .as_ref()
            .context("series standings did not use the score renderer")?;
        ensure!(
            page.model.rows.len() == 29
                && page.model.rows.iter().all(|row| row.built == 2)
                && crate::app::ai_series::score_actions(state)
                    .iter()
                    .any(|(id, label)| *id == crate::app::ai_series::TOGGLE_STANDINGS
                        && label == "Match Results"),
            "series standings did not display both completed matches for every entrant"
        );
        self.route.push(json!({"screen":"series standings", "frame":frame, "action":"OpenCompletedSeriesStandings", "records":records, "entrants":page.model.rows.len(), "simulation_unchanged":true, "status":status}));
        self.observer_battlefield_ready = true;
        Ok(())
    }

    fn observe_standings_controls(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        use crate::app::presentation::spectator_tools::{self, SpectatorToolAction};
        if self.observer_initial_entities.is_none() || !self.observer_focused {
            self.observe_generated_battlefield(state, frame)?;
        }
        let tick = state
            .match_state
            .sim_runtime
            .as_ref()
            .context("standings runtime missing")?
            .simulation
            .session()
            .tick();
        if tick < 3_000 || self.observer_battlefield_ready {
            return Ok(());
        }
        let before = Self::simulation_identity(state)?;
        let open_point = Self::spectator_button(state, SpectatorToolAction::ToggleStandings)?;
        let projection = state
            .match_state
            .match_presentation
            .spectator_tools
            .standings
            .projection();
        ensure!(
            projection.initial_count() == 29 && projection.alive_count() > 0,
            "standings participant counts are incorrect"
        );
        let rows: Vec<_> = projection.rows().iter().map(|row| json!({"owner":row.owner().index(), "entrant":row.ordinal(), "alive":row.alive(), "army":row.army_value(), "buildings":row.buildings(), "credits":row.credits(), "kills":row.kills()})).collect();
        let mut visited = std::collections::HashSet::new();
        for _ in 0..29 {
            let layout = spectator_tools::panel_layout(state)
                .context("standings panel does not fit the viewport")?;
            let controls = layout.controls();
            for control in &controls {
                if let SpectatorToolAction::VisitHouse(owner) = control {
                    visited.insert(*owner);
                }
            }
            if !controls.contains(&SpectatorToolAction::NextStandings) {
                break;
            }
            Self::spectator_button(state, SpectatorToolAction::NextStandings)?;
        }
        ensure!(
            visited.len() == 29,
            "standings pages did not expose all 29 AI entrants"
        );
        for _ in 0..29 {
            let layout = spectator_tools::panel_layout(state).context("standings panel missing")?;
            if !layout
                .controls()
                .contains(&SpectatorToolAction::PreviousStandings)
            {
                break;
            }
            Self::spectator_button(state, SpectatorToolAction::PreviousStandings)?;
        }
        let owner = spectator_tools::panel_layout(state)
            .context("standings panel missing")?
            .controls()
            .into_iter()
            .find_map(|action| match action {
                SpectatorToolAction::VisitHouse(owner) => Some(owner),
                _ => None,
            })
            .context("standings has no selectable row")?;
        Self::spectator_button(state, SpectatorToolAction::VisitHouse(owner))?;
        let point = state
            .match_state
            .sim_runtime
            .as_ref()
            .and_then(|runtime| {
                state
                    .match_state
                    .match_presentation
                    .spectator_tools
                    .standings
                    .projection()
                    .focus_target(&runtime.simulation, owner)
            })
            .context("selected AI has no live base or unit")?;
        let world = crate::util::lepton::absolute_leptons_to_screen(point[0], point[1], point[2]);
        let camera = Self::camera_position(state);
        let screen = (
            (world.0 - camera.0) * state.match_state.input.zoom_level,
            (world.1 - camera.1) * state.match_state.input.zoom_level,
        );
        let layout = spectator_tools::panel_layout(state).context("standings panel missing")?;
        let viewport = crate::app::input::camera::tactical_viewport_size(state);
        ensure!(
            screen.0 > layout.right_edge() as f32
                && screen.0 < viewport.0 as f32
                && screen.1 >= 0.0
                && screen.1 < viewport.1 as f32,
            "selected base is hidden behind the standings panel or outside the viewport"
        );
        for _ in 0..5 {
            Self::spectator_button(state, SpectatorToolAction::SortStandings)?;
        }
        ensure!(
            spectator_tools::panel_layout(state).is_some_and(|layout| layout
                .controls()
                .contains(&SpectatorToolAction::VisitHouse(owner))),
            "selected AI was not anchored after sorting"
        );
        ensure!(
            Self::simulation_identity(state)? == before,
            "standings controls changed simulation state"
        );
        self.route.push(json!({"screen":"in game", "frame":frame, "tick":tick, "action":"Standings29AiPagesAndBaseVisit", "open_point":open_point, "base_screen":screen, "panel_right":layout.right_edge(), "viewport":viewport, "visited_entrants":visited.len(), "selected_owner":owner.index(), "rows":rows, "simulation_unchanged":true}));
        self.observer_battlefield_ready = true;
        Ok(())
    }

    fn camera_position(state: &AppState) -> (f32, f32) {
        (
            state.match_state.input.camera_x,
            state.match_state.input.camera_y,
        )
    }

    fn spectator_pointer_input(
        state: &mut AppState,
        point: (i32, i32),
        button: winit::event::MouseButton,
    ) -> Result<()> {
        let identity = Self::simulation_identity(state)?;
        Self::set_pointer(state, point);
        crate::app::input::dispatch::handle_cursor_moved_in_game(state);
        for edge in [
            winit::event::ElementState::Pressed,
            winit::event::ElementState::Released,
        ] {
            crate::app::input::dispatch::handle_mouse_input(state, button, edge);
        }
        Self::set_pointer(state, (EXPECTED_CURSOR_X as i32, EXPECTED_CURSOR_Y as i32));
        crate::app::input::dispatch::handle_cursor_moved_in_game(state);
        ensure!(
            Self::simulation_identity(state)? == identity,
            "spectator pointer input changed simulation tick, frame, or state hash"
        );
        Ok(())
    }

    fn spectator_button(
        state: &mut AppState,
        action: crate::app::presentation::spectator_tools::SpectatorToolAction,
    ) -> Result<(i32, i32)> {
        let point = crate::app::presentation::spectator_tools::action_center(state, action)
            .with_context(|| {
                let viewer = state.match_state.sim_runtime.as_ref().map(|runtime| {
                    let sim = &runtime.simulation;
                    (sim.session().tick(), sim.session().current_house().map(|owner| {
                        (sim.session().is_observer_house(owner), sim.can_continue_watching(owner))
                    }))
                });
                format!(
                    "spectator camera button is not visible: active={}, paused={}, frozen={}, focus_frozen={}, viewport={:?}, viewer={viewer:?}",
                    state.platform.window_active,
                    state.match_state.paused(),
                    state.match_state.main_tick_frozen(),
                    state.focus_freeze_active(),
                    crate::app::input::camera::tactical_viewport_size(state),
                )
            })?;
        Self::spectator_pointer_input(state, point, winit::event::MouseButton::Left)?;
        Ok(point)
    }

    fn observe_camera_controls(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        use crate::app::match_runtime::frame_pacer::wall_clock_ms;
        use crate::app::presentation::spectator_tools::SpectatorToolAction;

        ensure!(
            state.frontend.screen == GameScreen::InGame,
            "AI camera capture reached results before its controls finished"
        );
        if self.observer_initial_entities.is_none() || !self.observer_focused {
            self.observe_generated_battlefield(state, frame)?;
        }
        let tick = state
            .match_state
            .sim_runtime
            .as_ref()
            .context("observer camera runtime missing")?
            .simulation
            .session()
            .tick();
        ensure!(
            tick < 18_000,
            "AI camera capture found no qualifying combat or resumed movement before 18,000 ticks"
        );
        let now = wall_clock_ms();
        let status = state.match_state.input.spectator_camera.status(now);
        match self
            .observer_camera_step
            .context("observer camera route is not enabled")?
        {
            ObserverCameraStep::Enable => {
                ensure!(
                    !state.match_state.input.spectator_camera.enabled(),
                    "automatic spectator camera did not start disabled"
                );
                let before = Self::camera_position(state);
                let point = Self::spectator_button(state, SpectatorToolAction::ToggleCamera)?;
                ensure!(
                    state.match_state.input.spectator_camera.enabled(),
                    "Watch the Action did not enable automatic camera"
                );
                self.route.push(json!({"screen":"in game", "frame":frame, "tick":tick, "action":"WatchTheAction", "point":point, "camera":before, "simulation_unchanged":true}));
                self.observer_camera_step = Some(ObserverCameraStep::WaitingForBattle {
                    initial_camera: before,
                });
            }
            ObserverCameraStep::WaitingForBattle { initial_camera } => {
                if !matches!(
                    status.as_str(),
                    "Auto Camera: On - Heavy fighting" | "Auto Camera: On - Base under attack"
                ) {
                    return Ok(());
                }
                let automatic = Self::camera_position(state);
                let rect = state.sidebar_frame().rect_to_frame(
                    crate::app::presentation::sidebar_render::active_minimap_content_screen_rect(
                        state,
                    ),
                );
                ensure!(
                    rect.w > 0.0 && rect.h > 0.0,
                    "observer camera has no live minimap click surface"
                );
                let mut manual = automatic;
                let mut chosen = None;
                for fraction in [0.25, 0.75] {
                    let point = (
                        (rect.x + rect.w * fraction) as i32,
                        (rect.y + rect.h * fraction) as i32,
                    );
                    Self::spectator_pointer_input(state, point, winit::event::MouseButton::Right)?;
                    manual = Self::camera_position(state);
                    if (manual.0 - automatic.0).abs() + (manual.1 - automatic.1).abs() > 1.0 {
                        chosen = Some(point);
                        break;
                    }
                }
                let point =
                    chosen.context("real minimap input did not move the spectator camera")?;
                ensure!(
                    state
                        .match_state
                        .input
                        .spectator_camera
                        .manual_remaining(wall_clock_ms())
                        > 0,
                    "manual minimap navigation did not suspend automatic camera"
                );
                self.route.push(json!({"screen":"in game", "frame":frame, "tick":tick, "action":"ManualCameraOverride", "point":point, "real_combat_status":status, "initial_camera":initial_camera, "automatic_camera":automatic, "manual_camera":manual, "manual_remaining_seconds":state.match_state.input.spectator_camera.manual_remaining(wall_clock_ms()), "simulation_unchanged":true}));
                self.observer_camera_step = Some(ObserverCameraStep::Manual { camera: manual });
            }
            ObserverCameraStep::Manual { camera } => {
                let held = Self::camera_position(state);
                ensure!(
                    state
                        .match_state
                        .input
                        .spectator_camera
                        .manual_remaining(now)
                        > 0,
                    "manual override expired before Resume Now was exercised"
                );
                ensure!(
                    (held.0 - camera.0).abs() + (held.1 - camera.1).abs() < 0.25,
                    "automatic camera moved during manual override"
                );
                let point = Self::spectator_button(state, SpectatorToolAction::ResumeCamera)?;
                ensure!(
                    state.match_state.input.spectator_camera.enabled()
                        && state
                            .match_state
                            .input
                            .spectator_camera
                            .manual_remaining(wall_clock_ms())
                            == 0,
                    "Resume Now did not restore automatic camera control"
                );
                self.route.push(json!({"screen":"in game", "frame":frame, "tick":tick, "action":"ResumeAutomaticCamera", "point":point, "manual_camera":camera, "simulation_unchanged":true}));
                self.observer_camera_step = Some(ObserverCameraStep::WaitingForResume {
                    manual_camera: camera,
                    resumed_ms: wall_clock_ms(),
                });
            }
            ObserverCameraStep::WaitingForResume {
                manual_camera,
                resumed_ms,
            } => {
                if now.saturating_sub(resumed_ms) < 2_500 {
                    return Ok(());
                }
                let Some(focus) = state.match_state.input.spectator_camera.focus_point(now) else {
                    return Ok(());
                };
                let current = Self::camera_position(state);
                let viewport = crate::app::input::camera::tactical_viewport_size(state);
                let zoom = state.match_state.input.zoom_level;
                let focus_screen = ((focus.0 - current.0) * zoom, (focus.1 - current.1) * zoom);
                if !(0.0..viewport.0 as f32).contains(&focus_screen.0)
                    || !(0.0..viewport.1 as f32).contains(&focus_screen.1)
                {
                    return Ok(());
                }
                if matches!(
                    status.as_str(),
                    "Auto Camera: On - Heavy fighting" | "Auto Camera: On - Base under attack"
                ) && (current.0 - manual_camera.0).abs() + (current.1 - manual_camera.1).abs()
                    > 1.0
                {
                    self.route.push(json!({"screen":"in game", "frame":frame, "tick":tick, "action":"AutomaticCameraResumed", "status":status, "manual_camera":manual_camera, "automatic_camera":current, "focus_world":focus, "focus_screen":focus_screen, "viewport":viewport, "whole_map_revealed":true}));
                    self.observer_camera_step = Some(ObserverCameraStep::Done);
                    self.observer_battlefield_ready = true;
                }
            }
            ObserverCameraStep::Done => {}
        }
        Ok(())
    }

    fn observe_generated_battlefield(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        let sim = &state
            .match_state
            .sim_runtime
            .as_ref()
            .context("observer simulation missing")?
            .simulation;
        let observer = sim
            .session()
            .current_house()
            .context("observer house missing")?;
        ensure!(
            sim.session().is_observer_house(observer),
            "client did not load the canonical observer"
        );
        let completed = matches!(state.frontend.screen, GameScreen::MissionResult { .. });
        if self.observer_battlefield_ready
            || (self.observer_initial_entities.is_some()
                && self.observer_focused
                && !completed
                && sim.session().tick() < 9_000)
        {
            return Ok(());
        }
        let ai: Vec<_> = sim
            .houses()
            .values()
            .filter(|house| !house.is_human() && !house.multiplay_passive() && !house.is_defeated())
            .collect();
        let observer_entities = sim
            .entities()
            .values_sorted()
            .filter(|entity| entity.owner() == observer)
            .count();
        ensure!(
            observer_entities == 0,
            "observer acquired gameplay entities"
        );
        ensure!(
            sim.houses()
                .get(&observer)
                .is_some_and(|house| house.map_is_clear())
                && sim.fog().whole_map_revealed_for_owner(observer),
            "observer client did not reveal the whole map"
        );
        if self.observer_initial_entities.is_none() {
            ensure!(
                ai.len() == 29
                    && ai.iter().all(|house| house.difficulty()
                        == crate::sim::house_state::HouseDifficulty::Normal),
                "observer client did not start 29 live Normal AI houses"
            );
            self.observer_initial_entities = Some(observer_entities);
            self.route.push(json!({"screen": "in game", "frame": frame, "action": "observer initial battlefield", "tick": sim.session().tick(), "live_ai": ai.len(), "ai_difficulty": "Normal", "observer_entities": observer_entities, "whole_map_revealed": true}));
        }
        if !self.observer_focused && !completed {
            let anchor = sim
                .entities()
                .values_sorted()
                .find(|entity| {
                    entity.is_active() && ai.iter().any(|house| house.name() == entity.owner())
                })
                .context("no live AI entity to observe")?;
            let cell = (anchor.position.rx(), anchor.position.ry());
            crate::app::input::camera::center_camera_on_cell(state, cell.0, cell.1);
            self.observer_focused = true;
            return Ok(());
        }
        if !completed && sim.session().tick() < 9_000 {
            return Ok(());
        }
        let hostile_ai = ai.iter().enumerate().any(|(index, left)| {
            ai[index + 1..].iter().any(|right| {
                !crate::map::houses::are_houses_mutually_allied(
                    sim.house_alliances(),
                    sim.interner().resolve(left.name()),
                    sim.interner().resolve(right.name()),
                )
            })
        });
        if completed {
            ensure!(
                !hostile_ai,
                "observer ended the match while hostile AIs remained"
            );
            if !Self::score_settled(state) {
                return Ok(());
            }
        } else {
            ensure!(
                sim.ready_outcome_for_owner(observer).is_none(),
                "observer prematurely received a terminal result"
            );
            if ai.len() < 2 || !hostile_ai {
                return Ok(());
            }
        }
        let samples = [(375.0, 495.0), (460.0, 540.0), (500.0, 500.0)].map(|screen| {
            let cell = crate::app::match_runtime::sim_tick::screen_point_to_world_cell(state, screen.0, screen.1);
            let terrain = sim.resolved_terrain().and_then(|terrain| terrain.cell(cell.0, cell.1));
            json!({"screen": [screen.0, screen.1], "cell": [cell.0, cell.1],
                "in_size": sim.map_cell_in_bounds((cell.0 as i16, cell.1 as i16)),
                "in_local_playfield": sim.playfield_bounds().zip(terrain).is_some_and(|(bounds, terrain)| bounds.contains_height_aware_packed(i32::from(cell.0), i32::from(cell.1), terrain.level as i8, terrain.slope_type)),
                "terrain_present": terrain.is_some(), "revealed": sim.fog().is_cell_revealed(observer, cell.0, cell.1),
                "gap_covered": sim.fog().is_cell_gap_covered(observer, cell.0, cell.1)})
        });
        self.route.push(json!({"screen": if completed {"results"} else {"in game"}, "frame": frame, "action": "observer battlefield", "target_ticks": 9000, "tick": sim.session().tick(), "binary_frame": sim.session().binary_frame(), "seed": sim.session().seed(), "map": sim.session().map_name(), "live_ai": ai.len(), "observer": sim.interner().resolve(observer), "observer_starting_entities": self.observer_initial_entities, "observer_entities": observer_entities, "whole_map_revealed": true, "hostile_ai": hostile_ai, "completed": completed, "camera": [state.match_state.input.camera_x, state.match_state.input.camera_y], "zoom": state.match_state.input.zoom_level, "playfield": sim.playfield_bounds(), "bottom_screen_samples": samples}));
        self.observer_battlefield_ready = true;
        Ok(())
    }

    pub(super) fn random_map(target: RandomMapTarget) -> Self {
        Self::chooser(ChooserTarget::RandomMap(target))
    }

    /// Where the pointer rests: on the hovered control once the route has
    /// moved it there, else at the capture's rest point.
    fn rest_point(&self) -> (i32, i32) {
        let hover = match self.chooser {
            Some(ChooserTarget::RandomMap(RandomMapTarget::Hover(point))) => Some(point),
            Some(ChooserTarget::RandomMap(RandomMapTarget::ListOpen)) => {
                Some(RANDOM_MAP_MAP_SIZE_FACE_POINT)
            }
            _ => self.hover,
        };
        hover
            .filter(|_| self.hovered)
            .unwrap_or((EXPECTED_CURSOR_X as i32, EXPECTED_CURSOR_Y as i32))
    }

    fn set_pointer(state: &mut AppState, point: (i32, i32)) {
        state.match_state.input.cursor_x = point.0 as f32;
        state.match_state.input.cursor_y = point.1 as f32;
    }

    fn expected_cursor(&self, loading_screen: bool) -> (f32, f32) {
        if loading_screen
            && self.phase == Phase::Starting
            && self.ai_series_target == Some(AiSeriesTarget::Results)
            && self.ai_series_terminal_outcomes.len() == 1
            && !self.ai_series_second_round_seen
            && let Some(cursor) = self.ai_series_rematch_cursor
        {
            return cursor;
        }
        let rest = self.rest_point();
        (rest.0 as f32, rest.1 as f32)
    }

    fn guard(&self, state: &AppState) -> Result<()> {
        let rest = self.rest_point();
        guard(
            state,
            self.chooser,
            matches!(
                self.phase,
                Phase::Starting
                    | Phase::AiSeriesRecovery
                    | Phase::Quitting
                    | Phase::Defeating
                    | Phase::Scoring
                    | Phase::BattlefieldInspection
                    | Phase::Replaying
                    | Phase::Continuing
                    | Phase::Diplomacy
            ),
            self.expected_cursor(state.frontend.screen == GameScreen::Loading),
            self.ai_commanders.as_ref().map_or((EXPECTED_WIDTH, EXPECTED_HEIGHT), |capture| capture.surface()),
        ).with_context(|| format!(
            "Skirmish capture guard: phase={:?}, screen={:?}, series_target={:?}, actual_cursor=({:?},{:?}), rest={rest:?}, hover={:?}, hovered={}, chooser={}, random_map={}, terminal_rounds={}",
            self.phase, state.frontend.screen, self.ai_series_target,
            state.match_state.input.cursor_x, state.match_state.input.cursor_y,
            self.hover, self.hovered,
            state.frontend.skirmish_shell_state.choose_map_modal.is_some(),
            state.frontend.skirmish_shell_state.random_map_setup_modal.is_some(),
            self.ai_series_terminal_outcomes.len(),
        ))
    }

    /// A press and release at `point` through the chooser's production
    /// handlers.
    fn click_chooser(state: &mut AppState, point: (i32, i32)) {
        state.match_state.input.cursor_x = point.0 as f32;
        state.match_state.input.cursor_y = point.1 as f32;
        App::handle_choose_map_modal_mouse_down(state);
        App::handle_choose_map_modal_mouse_up(state);
    }

    /// Track press above the map list's thumb (scrolls to the top), a press
    /// on the first row, then Use Map; the pointer returns to rest.
    fn press_use_map_on_first_map(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        let layout = crate::ui::skirmish_shell::compute_choose_map_modal_layout(
            EXPECTED_WIDTH,
            EXPECTED_HEIGHT,
        );
        let map_list = |state: &AppState| {
            state
                .frontend
                .skirmish_shell_state
                .choose_map_modal
                .as_ref()
                .map(|modal| modal.map_geometry(&layout))
                .context("chooser closed")
        };
        let bar = map_list(state)?
            .scrollbar
            .context("the map list needs a scrollbar")?;
        Self::click_chooser(state, (bar.x + 10, bar.y + 24));
        let row = map_list(state)?.row(0);
        Self::click_chooser(state, (row.x + 4, row.y + 4));
        let button = layout.use_map_button;
        Self::click_chooser(state, (button.x + button.w / 2, button.y + button.h / 2));
        state.match_state.input.cursor_x = EXPECTED_CURSOR_X as f32;
        state.match_state.input.cursor_y = EXPECTED_CURSOR_Y as f32;
        ensure!(
            state
                .frontend
                .skirmish_shell_state
                .choose_map_modal
                .as_ref()
                .is_some_and(|modal| modal.eject_prompt.is_some()),
            "Use Map on the first map did not ask to eject AI players"
        );
        self.route.push(json!({"dialog": 0x6b, "frame": frame,
            "action": "Use Map on the first map", "row": row.y}));
        Ok(())
    }

    fn defeat_route(&self) -> bool {
        matches!(
            self.loading,
            Some(
                LoadingTarget::DefeatScore
                    | LoadingTarget::DefeatContinue
                    | LoadingTarget::DefeatReplay
            )
        )
    }

    /// The local player's first entity (stable order) whose rules type
    /// matches.
    fn local_entity(
        state: &AppState,
        wanted: impl Fn(&crate::rules::object_type::ObjectType) -> bool,
    ) -> Option<u64> {
        let owner = crate::app::input::commands::preferred_local_owner_name(state)?;
        let sim = &state.match_state.sim_runtime.as_ref()?.simulation;
        let rules = state.rules()?;
        sim.entities()
            .values_sorted()
            .filter(|entity| sim.interner().resolve(entity.owner()) == owner)
            .find(|entity| {
                sim.interner()
                    .object_type(entity.type_ref(), rules)
                    .is_some_and(&wanted)
            })
            .map(|entity| entity.stable_id())
    }

    /// Deploy the MCV, then sell the Construction Yard it becomes: with Short
    /// Game on the house has no base left and loses. The score page follows
    /// through the production exit cascade.
    fn lose_the_game(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        if matches!(state.frontend.screen, GameScreen::MissionResult { .. }) {
            ensure!(
                App::score_shell_active(state),
                "the defeat reached a result screen without the score page"
            );
            let page = state
                .frontend
                .score_page
                .as_ref()
                .context("score page missing")?;
            let rows: Vec<Value> = page
                .model
                .rows
                .iter()
                .map(|row| {
                    json!({"name": row.name, "rgb": row.rgb, "kills": row.kills,
                        "losses": row.losses, "built": row.built, "score": row.score})
                })
                .collect();
            self.route.push(
                json!({"dialog": 0x108, "frame": frame, "action": "score page",
                "title_key": page.model.title_key, "game": page.model.game_number,
                "seconds": page.model.elapsed_seconds, "side": page.model.side, "rows": rows}),
            );
            self.phase = Phase::Scoring;
            return Ok(());
        }
        let owner = crate::app::input::commands::preferred_local_owner_name(state)
            .context("no local owner in the game")?;
        if !self.deploy_sent {
            let mcv = Self::local_entity(state, |object| object.deploys_into.is_some())
                .context("the local player has no MCV")?;
            crate::app::input::commands::schedule_command(
                state,
                &owner,
                crate::sim::command::Command::DeployMcv { entity_id: mcv },
            );
            self.route.push(json!({"screen": "in game", "frame": frame,
                "action": "deploy MCV", "entity": mcv}));
            self.deploy_sent = true;
        } else if !self.sell_sent
            && let Some(yard) = Self::local_entity(state, |object| object.construction_yard)
        {
            crate::app::input::commands::schedule_command(
                state,
                &owner,
                crate::sim::command::Command::SellBuilding { entity_id: yard },
            );
            self.route.push(json!({"screen": "in game", "frame": frame,
                "action": "sell Construction Yard", "entity": yard}));
            self.sell_sent = true;
        }
        Ok(())
    }

    fn click_score_action(&self, state: &mut AppState, id: u16) -> Result<()> {
        let button = crate::app::frontend::shell_layout::score_layout(state)
            .result_action_button(id)
            .or_else(|| crate::app::frontend::ai_series_render::score_action_rect(state, id))
            .context("score action button missing")?;
        Self::set_pointer(state, (button.x + button.w / 2, button.y + button.h / 2));
        App::handle_score_shell_mouse_move(state);
        App::handle_score_shell_mouse_down(state);
        App::handle_score_shell_mouse_up(state);
        Self::set_pointer(state, self.rest_point());
        Ok(())
    }

    fn current_launch_setup(
        state: &AppState,
    ) -> Result<crate::skirmish_launch::SkirmishLaunchSession> {
        crate::ui::skirmish_shell::pack_launch_session_without_start_validation(
            &state.frontend.skirmish_shell_state,
            state.frontend.scenario_catalog.shell_maps(),
            &state.frontend.skirmish_modes,
        )
        .map_err(|error| anyhow::anyhow!("replay setup is invalid: {error:?}"))
    }

    fn replay_the_game(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        if self.phase == Phase::Scoring {
            let seed = state
                .match_state
                .sim_runtime
                .as_ref()
                .context("finished simulation missing")?
                .simulation
                .session()
                .seed();
            self.replay_setup = Some((seed, Self::current_launch_setup(state)?));
            self.click_score_action(state, crate::ui::score_shell::PLAY_AGAIN)?;
            ensure!(
                state.frontend.shell_exit.is_some() || state.frontend.screen == GameScreen::Loading,
                "Play Again did not start the fresh match"
            );
            self.phase = Phase::Replaying;
            self.route.push(
                json!({"dialog": 0x108, "frame": frame, "action": "PlayAgain", "seed": seed}),
            );
        } else if !self.replay_completed && state.frontend.screen == GameScreen::InGame {
            let (old_seed, expected) =
                self.replay_setup.as_ref().context("replay setup missing")?;
            let sim = &state
                .match_state
                .sim_runtime
                .as_ref()
                .context("replayed simulation missing")?
                .simulation;
            ensure!(
                sim.session().started() && sim.session().seed() != *old_seed,
                "Play Again did not install a fresh started scenario with a new seed"
            );
            ensure!(
                Some(sim.session().map_name()) == expected.selected_map_file.as_deref(),
                "Play Again changed the selected map"
            );
            ensure!(
                Self::current_launch_setup(state)? == *expected,
                "Play Again changed the raw match setup"
            );
            ensure!(
                !state.match_state.sandbox_full_visibility,
                "finished battlefield visibility leaked into the replay"
            );
            self.replay_completed = true;
            self.route.push(json!({"screen": "in game", "frame": frame, "action": "ReplayStarted", "seed_before": old_seed, "seed_after": sim.session().seed(), "map": sim.session().map_name(), "tick": sim.session().tick(), "setup_unchanged": true, "visibility_override": state.match_state.sandbox_full_visibility}));
        }
        Ok(())
    }

    fn record_generated_selection(&mut self, state: &AppState, frame: u32) -> Result<()> {
        let selection = selected_scene(state)?;
        if self.default_observer_setup {
            let shell = &state.frontend.skirmish_shell_state;
            let active: Vec<_> = shell
                .opponents
                .iter()
                .filter(|opponent| opponent.is_active())
                .collect();
            ensure!(
                matches!(
                    shell.player_role,
                    crate::skirmish_launch::LaunchHumanRole::Observer
                ),
                "generated observer setup changed the local role"
            );
            ensure!(
                active.len() == 29
                    && active.iter().all(|opponent| opponent.row_type
                        == crate::ui::skirmish_shell::SkirmishAiRowType::Normal),
                "generated observer setup did not retain 29 Normal AIs"
            );
            ensure!(
                state
                    .frontend
                    .skirmish_modes
                    .iter()
                    .any(|mode| mode.id == shell.selected_mode_id
                        && mode.class == crate::skirmish_modes::MpModeClass::FreeForAll),
                "default observer setup did not retain Free For All"
            );
            self.route.push(json!({"dialog": 0x102, "frame": frame, "action": "generated observer setup", "observer": true, "active_ai": active.len(), "ai_difficulty": "Normal", "mode_class": "FreeForAll"}));
        }
        self.selected_scene = Some(selection);
        if self.observer_client {
            self.chooser = None;
            self.loading = Some(LoadingTarget::FirstFrame);
            self.phase = Phase::Skirmish;
        }
        Ok(())
    }

    fn simulation_identity(state: &AppState) -> Result<(u64, u32, u64)> {
        let sim = &state
            .match_state
            .sim_runtime
            .as_ref()
            .context("retained simulation missing")?
            .simulation;
        Ok((
            sim.session().tick(),
            sim.session().binary_frame(),
            sim.state_hash(),
        ))
    }

    fn begin_battlefield_inspection(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        let score = state
            .frontend
            .score_page
            .as_ref()
            .context("score page missing")?
            .model
            .clone();
        let identity = Self::simulation_identity(state)?;
        self.click_score_action(state, crate::ui::score_shell::VIEW_BATTLEFIELD)?;
        ensure!(
            crate::app::match_runtime::post_match::viewing_battlefield(state),
            "View Battlefield did not retain the finished game"
        );
        self.battlefield_inspection = Some((score, identity));
        self.phase = Phase::BattlefieldInspection;
        self.route.push(json!({"dialog": 0x108, "frame": frame, "action": "ViewBattlefield", "tick": identity.0, "binary_frame": identity.1, "visibility_override": state.match_state.sandbox_full_visibility}));
        Ok(())
    }

    fn finish_battlefield_inspection(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        ensure!(
            crate::app::match_runtime::post_match::viewing_battlefield(state),
            "finished battlefield did not present"
        );
        let (score, identity) = self
            .battlefield_inspection
            .take()
            .context("battlefield inspection missing")?;
        crate::app::match_runtime::sim_tick::advance_in_game_runtime(
            state,
            crate::app::match_runtime::frame_pacer::wall_clock_ms().saturating_add(1_000),
            false,
        );
        ensure!(
            Self::simulation_identity(state)? == identity,
            "finished battlefield advanced its retained simulation"
        );
        let escape = winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape);
        crate::app::input::keyboard::in_game_key_edge(
            state,
            crate::app::input::keyboard::InGameKeyEdge {
                physical: winit::keyboard::KeyCode::Escape,
                logical: &escape,
                unmodified: &escape,
                location: winit::keyboard::KeyLocation::Standard,
                state: winit::event::ElementState::Pressed,
                repeat: false,
                text: None,
            },
            false,
        );
        ensure!(
            App::score_shell_active(state),
            "Escape did not reopen the retained score page"
        );
        ensure!(
            state
                .frontend
                .score_page
                .as_ref()
                .is_some_and(|page| page.model == score),
            "battlefield inspection changed the result model"
        );
        ensure!(
            Self::simulation_identity(state)? == identity,
            "returning to results changed the retained simulation"
        );
        self.battlefield_inspected = true;
        self.phase = Phase::Scoring;
        self.route.push(json!({"dialog": 0x108, "frame": frame, "action": "EscapeToResults", "tick": identity.0, "binary_frame": identity.1, "state_hash": format!("{:016x}", identity.2), "score_unchanged": true}));
        Ok(())
    }

    /// The score page shows with no slide and every reveal finished.
    fn score_settled(state: &AppState) -> bool {
        state.frontend.shell_first_paint_slide.is_none()
            && state.frontend.shell_exit.is_none()
            && state.frontend.shell_page_title.is_terminal()
            && state.frontend.shell_status_line.is_terminal()
            && state
                .frontend
                .score_page
                .as_ref()
                .is_some_and(|page| page.reveals_terminal())
    }

    /// The route's time budget: the quit and defeat routes load and play a
    /// game.
    pub(super) fn timeout(&self) -> std::time::Duration {
        if self.observer_client {
            return std::time::Duration::from_secs(600);
        }
        if matches!(
            self.loading,
            Some(
                LoadingTarget::AfterQuit
                    | LoadingTarget::DefeatScore
                    | LoadingTarget::DefeatContinue
                    | LoadingTarget::DefeatReplay
                    | LoadingTarget::Diplomacy
            )
        ) {
            std::time::Duration::from_secs(180)
        } else {
            std::time::Duration::from_secs(60)
        }
    }

    /// `0x6B` shows with no slide and its heading and status line revealed.
    fn chooser_settled(state: &AppState) -> bool {
        state
            .frontend
            .skirmish_shell_state
            .choose_map_modal
            .is_some()
            && state.frontend.shell_first_paint_slide.is_none()
            && state.frontend.shell_exit.is_none()
            && state.frontend.shell_slide_active_shell == Some(ShellSlideKind::ChooseMap)
            && state
                .frontend
                .skirmish_shell_state
                .choose_map_modal
                .as_ref()
                .is_some_and(|modal| modal.statics.is_terminal())
    }

    /// `0x105` shows with no slide and its heading and status line revealed.
    fn random_map_settled(state: &AppState) -> bool {
        state.frontend.shell_first_paint_slide.is_none()
            && state.frontend.shell_exit.is_none()
            && state.frontend.shell_slide_active_shell == Some(ShellSlideKind::RandomMap)
            && state
                .frontend
                .skirmish_shell_state
                .random_map_setup_modal
                .as_ref()
                .is_some_and(|setup| setup.statics.is_terminal())
    }

    /// Generate Map, Save Map once the map exists, then the browser's Back
    /// (`0x686`); from then on `0x105` shows without an entry slide.
    fn seed_browser_step(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        let shell = &state.frontend.skirmish_shell_state;
        let browsing = shell.saved_seed_browser.is_some();
        if self.pointer_rested {
            ensure!(!browsing, "the seed browser stayed open after Back");
            ensure!(
                state.frontend.shell_first_paint_slide.is_none(),
                "closing the seed browser replayed 0x105's entry slide"
            );
            return Ok(());
        }
        let centre =
            |rect: crate::ui::skirmish_shell::RectPx| (rect.x + rect.w / 2, rect.y + rect.h / 2);
        // The covered `0x105` paints nothing, so its statics wait.
        if browsing {
            let back = crate::ui::skirmish_shell::compute_saved_seed_layout(
                crate::ui::skirmish_shell::SavedSeedMode::Save,
                state.render_width(),
                state.render_height(),
            )
            .back;
            Self::set_pointer(state, centre(back));
            App::handle_saved_seed_browser_mouse_down(state);
            App::handle_saved_seed_browser_mouse_up(state);
            Self::set_pointer(state, self.rest_point());
            self.pointer_rested = true;
            self.route
                .push(json!({"dialog": 0x2b4, "frame": frame, "action": "Back"}));
            return Ok(());
        }
        if !Self::random_map_settled(state) {
            return Ok(());
        }
        let generated = shell
            .random_map_setup_modal
            .as_ref()
            .is_some_and(|setup| setup.generated && !setup.generating);
        let press = if !self.generate_pressed {
            self.generate_pressed = true;
            Some((RANDOM_MAP_GENERATE_POINT, "GenerateMap"))
        } else if generated && !self.save_pressed {
            self.save_pressed = true;
            let save = crate::ui::skirmish_shell::compute_random_map_setup_layout(
                state.render_width(),
                state.render_height(),
            )
            .save;
            Some((centre(save), "SaveMap"))
        } else {
            None
        };
        if let Some((point, action)) = press {
            Self::set_pointer(state, point);
            App::handle_skirmish_shell_mouse_move(state);
            App::handle_random_map_setup_mouse_down(state);
            App::handle_random_map_setup_mouse_up(state);
            Self::set_pointer(state, self.rest_point());
            self.route
                .push(json!({"dialog": 0x105, "frame": frame, "action": action}));
            if action == "SaveMap" {
                ensure!(
                    state
                        .frontend
                        .skirmish_shell_state
                        .saved_seed_browser
                        .is_some(),
                    "Save Map did not open the seed browser"
                );
            }
        }
        Ok(())
    }

    /// Hold `kind`'s entry slide once it reaches `target`.
    /// Generate Map, then Use Map once the map exists (Use Map starts
    /// disabled, as retail's `rmg-steady.png`). With `min_players` the
    /// route first presses the Players track's left end, as the retail
    /// helper does until the value reaches its minimum, 2 (`ej-players.png`).
    fn use_map_step(&mut self, state: &mut AppState, frame: u32, min_players: bool) {
        let generated = state
            .frontend
            .skirmish_shell_state
            .random_map_setup_modal
            .as_ref()
            .is_some_and(|setup| setup.generated && !setup.generating);
        let press = if min_players && !self.players_set {
            self.players_set = true;
            let track = crate::ui::skirmish_shell::compute_random_map_setup_layout(
                state.render_width(),
                state.render_height(),
            )
            .control_rects[5];
            Some(((track.x + 3, track.y + track.h / 2), "Players 2"))
        } else if !self.generate_pressed {
            self.generate_pressed = true;
            if self.chooser == Some(ChooserTarget::RandomMap(RandomMapTarget::UseMap)) {
                self.selected_scene = None;
            }
            let point = if self.ai_commanders.is_some() {
                let rect =
                    crate::app::frontend::shell_layout::random_map_setup_layout(state).generate;
                (rect.x + rect.w / 2, rect.y + rect.h / 2)
            } else {
                RANDOM_MAP_GENERATE_POINT
            };
            Some((point, "GenerateMap"))
        } else if generated && !self.pointer_rested {
            self.pointer_rested = true;
            let point = if self.ai_commanders.is_some() {
                let rect =
                    crate::app::frontend::shell_layout::random_map_setup_layout(state).use_map;
                (rect.x + rect.w / 2, rect.y + rect.h / 2)
            } else {
                RANDOM_MAP_USE_MAP_POINT
            };
            Some((point, "UseMap"))
        } else {
            None
        };
        if let Some((point, action)) = press {
            Self::set_pointer(state, point);
            App::handle_skirmish_shell_mouse_move(state);
            App::handle_random_map_setup_mouse_down(state);
            App::handle_random_map_setup_mouse_up(state);
            Self::set_pointer(state, self.rest_point());
            self.route
                .push(json!({"dialog": 0x105, "frame": frame, "action": action}));
        }
    }

    /// AI row 2 takes Easy through its type combo's production presses: the
    /// arrow opens the list and its second row picks Easy (retail
    /// `ej-ai-open.png`, `ej-ai-easy.png`).
    fn add_second_ai(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        use crate::ui::skirmish_shell::{SkirmishAiRowType, SkirmishComboId, SkirmishComboItem};
        let id = SkirmishComboId::AiType(1);
        let layout =
            crate::ui::skirmish_shell::compute_layout(state.render_width(), state.render_height());
        let face = crate::ui::skirmish_shell::combo_rect(
            &state.frontend.skirmish_shell_state,
            &layout,
            id,
        )
        .context("AI row 2 has no type combo")?;
        Self::set_pointer(
            state,
            (
                face.x + face.w - crate::ui::skirmish_shell::COMBO_ARROW_RESERVE_W / 2,
                face.y + crate::ui::skirmish_shell::COMBO_FACE_H / 2,
            ),
        );
        App::handle_skirmish_shell_mouse_down(state);
        let shell = &state.frontend.skirmish_shell_state;
        let maps = state.frontend.scenario_catalog.shell_maps();
        let list = crate::ui::skirmish_shell::combo_dropdown_content_rect(shell, &layout, maps, id)
            .filter(|_| shell.open_combo_dropdown.is_some())
            .context("AI row 2's type list did not open")?;
        let row = crate::ui::skirmish_shell::combo_items(shell, maps, id)
            .iter()
            .position(|item| *item == SkirmishComboItem::AiType(SkirmishAiRowType::Easy))
            .context("the AI type list has no Easy row")? as i32;
        let row_h = crate::ui::skirmish_shell::COMBO_DROPDOWN_ROW_H;
        Self::set_pointer(
            state,
            (list.x + list.w / 2, list.y + row * row_h + row_h / 2),
        );
        App::handle_skirmish_shell_mouse_down(state);
        crate::ui::skirmish_shell::handle_option_mouse_up(&mut state.frontend.skirmish_shell_state);
        Self::set_pointer(state, self.rest_point());
        ensure!(
            state
                .frontend
                .skirmish_shell_state
                .opponents
                .get(1)
                .is_some_and(|row| row.row_type == SkirmishAiRowType::Easy),
            "AI row 2 did not take Easy"
        );
        self.second_ai_added = true;
        self.selected_scene = Some(selected_scene(state)?);
        self.route
            .push(json!({"dialog": 0x102, "frame": frame, "action": "AI row 2 Easy"}));
        Ok(())
    }

    /// The defeat routes need Short Game (`0x54E`), which the profile may
    /// have saved off: only then does selling the last building, with no
    /// `BaseUnit=` left, defeat the house (`HouseClass::Update`
    /// `0x004F8EBD..0x004F8F77`); otherwise the starting units keep it alive.
    /// The route checks the box through the production click.
    fn check_short_game(&mut self, state: &mut AppState, frame: u32) -> Result<()> {
        use crate::ui::skirmish_shell::SkirmishCheckboxId;
        let layout =
            crate::ui::skirmish_shell::compute_layout(state.render_width(), state.render_height());
        let rect = layout
            .checkboxes
            .iter()
            .find(|checkbox| checkbox.id == SkirmishCheckboxId::ShortGame0x54e)
            .context("0x102 has no Short Game box")?
            .rect;
        let icon = crate::ui::skirmish_shell::checkbox_icon_rect(rect, layout.scale);
        Self::set_pointer(state, (icon.x + icon.w / 2, icon.y + icon.h / 2));
        App::handle_skirmish_shell_mouse_down(state);
        App::handle_skirmish_shell_mouse_up(state);
        Self::set_pointer(state, self.rest_point());
        ensure!(
            state.frontend.skirmish_shell_state.short_game,
            "the Short Game box did not take the click"
        );
        self.selected_scene = Some(selected_scene(state)?);
        self.route
            .push(json!({"dialog": 0x102, "frame": frame, "action": "check Short Game"}));
        Ok(())
    }

    /// Players 2, Generate Map and Use Map; once `0x105` has slid out, the
    /// hidden chooser's eject box asks while no dialog shows or slides, and
    /// the route answers it a presented frame later (`None` leaves it up).
    fn eject_step(
        &mut self,
        state: &mut AppState,
        answer: Option<crate::ui::skirmish_shell::EjectPromptButton>,
        frame: u32,
    ) -> Result<()> {
        let shell = &state.frontend.skirmish_shell_state;
        // Until Use Map is pressed (`pointer_rested`) and `0x105` has gone.
        if !self.pointer_rested || shell.random_map_setup_modal.is_some() {
            if Self::random_map_settled(state) {
                self.use_map_step(state, frame, true);
                if let Some(setup) = state
                    .frontend
                    .skirmish_shell_state
                    .random_map_setup_modal
                    .as_ref()
                {
                    ensure!(setup.options.num_players == 2, "Players did not reach 2");
                }
            }
            return Ok(());
        }
        ensure!(
            shell
                .choose_map_modal
                .as_ref()
                .is_some_and(|modal| modal.eject_prompt.is_some()),
            "Use Map on the two-player random map did not ask to eject AI players"
        );
        ensure!(
            shell.top_dialog().is_none()
                && state.frontend.shell_first_paint_slide.is_none()
                && state.frontend.shell_exit.is_none(),
            "a dialog shows or slides behind the eject box"
        );
        if !self.eject_seen {
            self.eject_seen = true;
            self.route.push(json!({"dialog": 0x6b, "frame": frame,
                "action": "eject box over no dialog"}));
            return Ok(());
        }
        let Some(answer) = answer else {
            return Ok(());
        };
        let layout = crate::ui::shell::modal::quit_confirm_layout(
            state.render_width() as i32,
            state.render_height() as i32,
        );
        let button = match answer {
            crate::ui::skirmish_shell::EjectPromptButton::Ok => layout.ok,
            crate::ui::skirmish_shell::EjectPromptButton::Cancel => layout.cancel,
        };
        Self::set_pointer(state, (button.x + button.w / 2, button.y + button.h / 2));
        App::handle_choose_map_eject_mouse_down(state);
        App::handle_choose_map_eject_mouse_up(state);
        Self::set_pointer(state, self.rest_point());
        let shell = &state.frontend.skirmish_shell_state;
        match answer {
            crate::ui::skirmish_shell::EjectPromptButton::Ok => ensure!(
                shell.choose_map_modal.is_none() && state.frontend.shell_exit.is_none(),
                "OK on the eject box slid the hidden chooser out"
            ),
            crate::ui::skirmish_shell::EjectPromptButton::Cancel => ensure!(
                shell.top_dialog()
                    == Some(crate::ui::skirmish_shell::SkirmishShellDialog::ChooseMap),
                "Cancel on the eject box did not show the chooser"
            ),
        }
        self.route.push(json!({"dialog": 0x6b, "frame": frame,
            "action": format!("eject box {answer:?}")}));
        self.phase = Phase::RandomMapEject;
        Ok(())
    }

    fn hold_entry(
        &mut self,
        state: &mut AppState,
        kind: ShellSlideKind,
        target: u32,
        dialog: u16,
        frame: u32,
    ) -> Result<()> {
        match entry_wave_tick(state, kind) {
            Some(tick) => {
                self.entry_seen = true;
                ensure!(
                    tick <= target,
                    "{dialog:#x} entry slide passed tick {target}"
                );
                if tick == target && !self.entry_held {
                    if let Some(wave) = state.frontend.shell_first_paint_slide.as_mut() {
                        wave.hold_for_capture();
                    }
                    self.entry_held = true;
                    self.route.push(json!({"dialog": dialog, "frame": frame,
                        "action": "hold entry slide", "tick": tick}));
                }
            }
            None => ensure!(
                !self.entry_seen || state.frontend.shell_slide_active_shell != Some(kind),
                "{dialog:#x} entry slide ended before capture"
            ),
        }
        Ok(())
    }

    /// Called only after an ordinary production frame has been presented. Route
    /// actions run here, never between frame acquisition and shell dispatch.
    pub(super) fn after_present(
        &mut self,
        state: &mut AppState,
        rendered: PresentedShell,
        frame: u32,
    ) -> Result<()> {
        if matches!(self.phase, Phase::Quitting | Phase::Continuing)
            && state.frontend.screen == GameScreen::MainMenu
        {
            // Back in the shell the pointer rests at the centre again.
            state.match_state.input.cursor_x = EXPECTED_CURSOR_X as f32;
            state.match_state.input.cursor_y = EXPECTED_CURSOR_Y as f32;
        }
        self.guard(state)?;
        self.last_presented = Some(rendered);
        match (self.phase, rendered) {
            (Phase::MainMenu, PresentedShell::MainMenu) => {
                if steady_main_menu_capture_ready_for(
                    MainMenuCaptureSnapshot::from_state(state),
                    self.ai_commanders.is_some(),
                )? {
                    self.route
                        .push(json!({"dialog": 0xe2, "frame": frame, "action": "SinglePlayer"}));
                    App::handle_main_menu_shell_action(
                        state,
                        crate::ui::main_menu_shell::MainMenuShellAction::SinglePlayer,
                    );
                    self.phase = Phase::SinglePlayer;
                }
            }
            (Phase::SinglePlayer, PresentedShell::SinglePlayer) => {
                ensure!(
                    state.frontend.shell_route.single_player(),
                    "Single Player route changed"
                );
                ensure!(
                    state
                        .frontend
                        .main_menu_movie_identity
                        .is_some_and(
                            |identity| identity.owner() == Ra2tsDialogOwner::SinglePlayer0x100
                        ),
                    "Single Player movie identity unavailable"
                );
                if state.frontend.shell_first_paint_slide.is_none()
                    && state.frontend.shell_slide_active_shell == Some(ShellSlideKind::SinglePlayer)
                {
                    self.route
                        .push(json!({"dialog": 0x100, "frame": frame, "action": "Skirmish"}));
                    App::handle_single_player_shell_action(
                        state,
                        crate::ui::single_player_shell::SinglePlayerShellAction::Skirmish,
                    );
                    self.phase = Phase::Skirmish;
                }
            }
            (Phase::Skirmish, PresentedShell::Other | PresentedShell::Skirmish)
                if matches!(self.chooser, Some(ChooserTarget::RandomMap(_)))
                    && state
                        .frontend
                        .skirmish_shell_state
                        .random_map_setup_modal
                        .is_some() =>
            {
                let shell = &state.frontend.skirmish_shell_state;
                let setup = shell
                    .random_map_setup_modal
                    .as_ref()
                    .context("default random-map setup missing")?;
                self.route.push(json!({"dialog": 0x105, "frame": frame,
                    "action": "default observer map setup", "observer": matches!(shell.player_role, crate::skirmish_launch::LaunchHumanRole::Observer),
                    "mode_id": shell.selected_mode_id, "players": setup.options.num_players,
                    "width": setup.options.width, "height": setup.options.height}));
                self.default_observer_setup = true;
                self.phase = Phase::RandomMap;
            }
            // The first-paint slide frames present through the generic slide
            // renderer, so they arrive as `Other`.
            (Phase::Skirmish, PresentedShell::Other | PresentedShell::Skirmish)
                if self.entry_tick.is_some() =>
            {
                let target = self.entry_tick.context("entry phase without a tick")?;
                match entry_wave_tick(state, ShellSlideKind::Skirmish) {
                    Some(tick) => {
                        self.entry_seen = true;
                        ensure!(tick <= target, "entry slide passed tick {target}");
                        if tick == target && !self.entry_held {
                            if let Some(wave) = state.frontend.shell_first_paint_slide.as_mut() {
                                wave.hold_for_capture();
                            }
                            self.entry_held = true;
                            self.route.push(json!({"dialog": 0x102, "frame": frame,
                                "action": "hold entry slide", "tick": tick}));
                        }
                    }
                    None => ensure!(
                        !self.entry_seen,
                        "Skirmish entry slide ended before capture"
                    ),
                }
            }
            (Phase::Skirmish, PresentedShell::Skirmish) => {
                if self.settled(state)? && self.selected_scene.is_none() {
                    self.selected_scene = Some(selected_scene(state)?);
                } else if let Some(point) = self
                    .hover
                    .filter(|_| self.selected_scene.is_some() && !self.hovered)
                {
                    state.match_state.input.cursor_x = point.0 as f32;
                    state.match_state.input.cursor_y = point.1 as f32;
                    App::handle_skirmish_shell_mouse_move(state);
                    self.hovered = true;
                    self.route.push(json!({"dialog": 0x102, "frame": frame,
                        "action": "pointer rests", "point": [point.0, point.1]}));
                } else if self.selected_scene.is_some() && self.slide_out_tick.is_some() {
                    App::handle_skirmish_back(state);
                    ensure!(
                        state.frontend.shell_exit.is_some(),
                        "Skirmish Back did not start its teardown slide"
                    );
                    self.route
                        .push(json!({"dialog": 0x102, "frame": frame, "action": "Back"}));
                    self.phase = Phase::SlideOut;
                } else if self.selected_scene.is_some()
                    && self.defeat_route()
                    && !state.frontend.skirmish_shell_state.short_game
                {
                    self.check_short_game(state, frame)?;
                } else if self.selected_scene.is_some() && self.ai_commanders.is_some() {
                    self.ai_commanders
                        .as_mut()
                        .unwrap()
                        .open(state, &mut self.route, frame)?;
                    self.phase = Phase::AiCommandersSetup;
                } else if self.selected_scene.is_some() && self.ai_series_target.is_some() {
                    self.open_ai_series(state, frame)?;
                } else if self.selected_scene.is_some() && self.loading.is_some() {
                    // The pointer moves onto Start Game (`0x617`), whose help
                    // reaches the status line as it would before a click,
                    // then the production action runs and the pointer rests
                    // again.
                    state.match_state.input.cursor_x = START_GAME_POINT.0 as f32;
                    state.match_state.input.cursor_y = START_GAME_POINT.1 as f32;
                    App::handle_skirmish_shell_mouse_move(state);
                    App::start_game_from_shell(state);
                    state.match_state.input.cursor_x = EXPECTED_CURSOR_X as f32;
                    state.match_state.input.cursor_y = EXPECTED_CURSOR_Y as f32;
                    ensure!(
                        state.frontend.shell_exit.is_some(),
                        "Start Game did not start 0x102's teardown slide"
                    );
                    self.route.push(json!({"dialog": 0x102, "frame": frame,
                        "action": "StartGame", "hover": [START_GAME_POINT.0, START_GAME_POINT.1]}));
                    self.phase = Phase::Starting;
                } else if self.selected_scene.is_some()
                    && matches!(
                        self.chooser,
                        Some(ChooserTarget::RandomMap(
                            RandomMapTarget::Eject(_) | RandomMapTarget::EjectBox
                        ))
                    )
                    && !self.second_ai_added
                {
                    self.add_second_ai(state, frame)?;
                } else if self.selected_scene.is_some() && self.chooser.is_some() {
                    App::leave_shell_dialog(
                        state,
                        crate::app::frontend::shell_transition::ShellExitThen::SkirmishChooseMap,
                    );
                    ensure!(
                        state.frontend.shell_exit.is_some(),
                        "Choose Map did not start 0x102's teardown slide"
                    );
                    self.route
                        .push(json!({"dialog": 0x102, "frame": frame, "action": "ChooseMap"}));
                    self.phase = Phase::Chooser;
                }
            }
            (Phase::AiCommandersSetup, PresentedShell::Skirmish) => {
                self.ai_commanders
                    .as_mut()
                    .context("commander capture state missing")?
                    .after_present(state, &mut self.route, frame)?;
            }
            (Phase::AiSeriesSetup, PresentedShell::Skirmish) => {
                self.ai_series_setup_presented = true;
                if self.ai_series_target == Some(AiSeriesTarget::Recovery) {
                    self.recover_ai_series(state, frame)?;
                } else if matches!(
                    self.ai_series_target,
                    Some(AiSeriesTarget::Start | AiSeriesTarget::Results)
                ) {
                    self.start_ai_series(state, frame)?;
                }
            }
            (Phase::AiSeriesRecovery, PresentedShell::Score) => {
                self.observe_ai_series_recovery(state, frame)?;
            }
            (Phase::Chooser, PresentedShell::Other | PresentedShell::Skirmish) => {
                match self.chooser {
                    Some(ChooserTarget::Entry(target)) => {
                        match entry_wave_tick(state, ShellSlideKind::ChooseMap) {
                            Some(tick) => {
                                self.entry_seen = true;
                                ensure!(tick <= target, "0x6B entry slide passed tick {target}");
                                if tick == target && !self.entry_held {
                                    if let Some(wave) =
                                        state.frontend.shell_first_paint_slide.as_mut()
                                    {
                                        wave.hold_for_capture();
                                    }
                                    self.entry_held = true;
                                    self.route.push(json!({"dialog": 0x6b, "frame": frame,
                                        "action": "hold entry slide", "tick": tick}));
                                }
                            }
                            None => ensure!(
                                !self.entry_seen
                                    || state.frontend.shell_slide_active_shell
                                        != Some(ShellSlideKind::ChooseMap),
                                "0x6B entry slide ended before capture"
                            ),
                        }
                    }
                    Some(ChooserTarget::Steady)
                        if !self.pointer_rested
                            && state.frontend.shell_first_paint_slide.is_none()
                            && state.frontend.shell_slide_active_shell
                                == Some(ShellSlideKind::ChooseMap) =>
                    {
                        App::handle_skirmish_shell_mouse_move(state);
                        self.pointer_rested = true;
                        self.route.push(json!({"dialog": 0x6b, "frame": frame,
                            "action": "pointer rests", "point": [
                                state.match_state.input.cursor_x,
                                state.match_state.input.cursor_y]}));
                    }
                    Some(ChooserTarget::Eject)
                        if Self::chooser_settled(state) && !self.pointer_rested =>
                    {
                        self.press_use_map_on_first_map(state, frame)?;
                        self.pointer_rested = true;
                    }
                    Some(ChooserTarget::RandomMap(_))
                        if Self::chooser_settled(state) && !self.create_random_map_hovered =>
                    {
                        Self::set_pointer(state, CREATE_RANDOM_MAP_POINT);
                        App::handle_skirmish_shell_mouse_move(state);
                        Self::set_pointer(state, self.rest_point());
                        self.create_random_map_hovered = true;
                    }
                    Some(ChooserTarget::RandomMap(_)) if Self::chooser_settled(state) => {
                        // Through the chooser's production press and release.
                        Self::click_chooser(state, CREATE_RANDOM_MAP_POINT);
                        Self::set_pointer(state, self.rest_point());
                        ensure!(
                            state.frontend.shell_exit.is_some(),
                            "Create Random Map did not start 0x6B's teardown slide"
                        );
                        self.route.push(json!({"dialog": 0x6b, "frame": frame,
                            "action": "CreateRandomMap", "hover": [
                                CREATE_RANDOM_MAP_POINT.0, CREATE_RANDOM_MAP_POINT.1]}));
                        self.phase = Phase::RandomMap;
                    }
                    Some(ChooserTarget::Return) if Self::chooser_settled(state) => {
                        App::leave_shell_dialog(
                            state,
                            crate::app::frontend::shell_transition::ShellExitThen::ChooseMapCancel,
                        );
                        ensure!(
                            state.frontend.shell_exit.is_some(),
                            "Cancel did not start 0x6B's teardown slide"
                        );
                        self.route
                            .push(json!({"dialog": 0x6b, "frame": frame, "action": "Cancel"}));
                        self.phase = Phase::ChooserReturn;
                    }
                    _ => {}
                }
            }
            (Phase::RandomMap, PresentedShell::Other | PresentedShell::Skirmish) => {
                let Some(ChooserTarget::RandomMap(target)) = self.chooser else {
                    bail!("the random-map phase runs only for random-map targets");
                };
                match target {
                    RandomMapTarget::Entry(target) => {
                        self.hold_entry(state, ShellSlideKind::RandomMap, target, 0x105, frame)?;
                    }
                    RandomMapTarget::Hover(point)
                        if Self::random_map_settled(state) && !self.hovered =>
                    {
                        Self::set_pointer(state, point);
                        App::handle_skirmish_shell_mouse_move(state);
                        self.hovered = true;
                        self.route.push(json!({"dialog": 0x105, "frame": frame,
                            "action": "pointer rests", "point": [point.0, point.1]}));
                    }
                    RandomMapTarget::ListOpen
                        if Self::random_map_settled(state) && !self.hovered =>
                    {
                        // Open the list through the dialog's production
                        // handlers, then rest the pointer below it.
                        Self::set_pointer(state, RANDOM_MAP_TERRAIN_ARROW_POINT);
                        App::handle_skirmish_shell_mouse_move(state);
                        App::handle_random_map_setup_mouse_down(state);
                        App::handle_random_map_setup_mouse_up(state);
                        ensure!(
                            state
                                .frontend
                                .skirmish_shell_state
                                .random_map_setup_modal
                                .as_ref()
                                .is_some_and(|setup| setup.open_combo.is_some()),
                            "Terrain Type's list did not open"
                        );
                        Self::set_pointer(state, RANDOM_MAP_MAP_SIZE_FACE_POINT);
                        App::handle_skirmish_shell_mouse_move(state);
                        self.hovered = true;
                        self.route.push(json!({"dialog": 0x105, "frame": frame,
                            "action": "open Terrain Type, pointer rests", "point": [
                                RANDOM_MAP_MAP_SIZE_FACE_POINT.0,
                                RANDOM_MAP_MAP_SIZE_FACE_POINT.1]}));
                    }
                    RandomMapTarget::CancelEntry(_) | RandomMapTarget::CancelSteady
                        if Self::random_map_settled(state) =>
                    {
                        // Cancel through the dialog's production handlers.
                        Self::set_pointer(state, RANDOM_MAP_CANCEL_POINT);
                        App::handle_skirmish_shell_mouse_move(state);
                        App::handle_random_map_setup_mouse_down(state);
                        App::handle_random_map_setup_mouse_up(state);
                        Self::set_pointer(state, self.rest_point());
                        ensure!(
                            state.frontend.shell_exit.is_some(),
                            "Cancel did not start 0x105's teardown slide"
                        );
                        self.route
                            .push(json!({"dialog": 0x105, "frame": frame, "action": "Cancel"}));
                        self.phase = Phase::RandomMapCancel;
                    }
                    RandomMapTarget::UseMap if Self::random_map_settled(state) => {
                        self.use_map_step(state, frame, false);
                    }
                    RandomMapTarget::UseMap
                        if self.pointer_rested
                            && self.selected_scene.is_none()
                            && state
                                .frontend
                                .skirmish_shell_state
                                .choose_map_modal
                                .is_none()
                            && self.settled(state)? =>
                    {
                        self.record_generated_selection(state, frame)?;
                    }
                    RandomMapTarget::SeedBrowserReturn => self.seed_browser_step(state, frame)?,
                    RandomMapTarget::Eject(answer) => {
                        self.eject_step(state, Some(answer), frame)?
                    }
                    RandomMapTarget::EjectBox => self.eject_step(state, None, frame)?,
                    _ => {}
                }
            }
            (Phase::RandomMapCancel, PresentedShell::Other | PresentedShell::Skirmish) => {
                match self.chooser {
                    Some(ChooserTarget::RandomMap(RandomMapTarget::CancelEntry(target)))
                        if state.frontend.shell_exit.is_none() =>
                    {
                        self.hold_entry(state, ShellSlideKind::ChooseMap, target, 0x6b, frame)?;
                    }
                    // The retail helper's pointer goes back to rest after its
                    // click; the queued move reaches the chooser once its
                    // slide has run (`rmg-cancel-b.png`).
                    Some(ChooserTarget::RandomMap(RandomMapTarget::CancelSteady))
                        if !self.pointer_rested
                            && state.frontend.shell_exit.is_none()
                            && state.frontend.shell_first_paint_slide.is_none()
                            && state.frontend.shell_slide_active_shell
                                == Some(ShellSlideKind::ChooseMap) =>
                    {
                        App::handle_skirmish_shell_mouse_move(state);
                        self.pointer_rested = true;
                        self.route.push(json!({"dialog": 0x6b, "frame": frame,
                            "action": "pointer rests", "point": [
                                state.match_state.input.cursor_x,
                                state.match_state.input.cursor_y]}));
                    }
                    _ => {}
                }
            }
            (Phase::RandomMapEject, PresentedShell::Other | PresentedShell::Skirmish) => {
                let Some(ChooserTarget::RandomMap(RandomMapTarget::Eject(answer))) = self.chooser
                else {
                    bail!("the eject phase runs only for eject targets");
                };
                ensure!(
                    crate::app::frontend::shell_transition::shell_exit_wave(
                        state,
                        ShellSlideKind::ChooseMap
                    )
                    .is_none(),
                    "the chooser slid out after the eject box"
                );
                // Cancel shows the chooser, whose first paint slides it in
                // (`0x005E6B47`); OK shows `0x102` again.
                let kind = match answer {
                    crate::ui::skirmish_shell::EjectPromptButton::Cancel => {
                        ShellSlideKind::ChooseMap
                    }
                    crate::ui::skirmish_shell::EjectPromptButton::Ok => ShellSlideKind::Skirmish,
                };
                if entry_wave_tick(state, kind).is_some() {
                    self.entry_seen = true;
                }
                // The retail helper's pointer goes back to rest after its
                // click; the queued move reaches the dialog once its slide has
                // run.
                if self.entry_seen
                    && !self.answer_rested
                    && state.frontend.shell_first_paint_slide.is_none()
                    && state.frontend.shell_slide_active_shell == Some(kind)
                {
                    App::handle_skirmish_shell_mouse_move(state);
                    self.answer_rested = true;
                    let dialog = if kind == ShellSlideKind::ChooseMap {
                        0x6b
                    } else {
                        0x102
                    };
                    self.route.push(json!({"dialog": dialog, "frame": frame,
                        "action": "pointer rests", "point": [
                            state.match_state.input.cursor_x,
                            state.match_state.input.cursor_y]}));
                }
            }
            (Phase::Starting, _)
                if self.observer_client
                    && (state.frontend.screen == GameScreen::InGame
                        || App::score_shell_active(state)) =>
            {
                if self.ai_series_target == Some(AiSeriesTarget::Results) {
                    self.observe_ai_series_results(state, frame)?;
                } else if self.ai_series_target == Some(AiSeriesTarget::Start) {
                    self.observe_ai_series_started(state, frame)?;
                } else if self.observer_standings {
                    self.observe_standings_controls(state, frame)?;
                } else if self.observer_camera_step.is_some() {
                    self.observe_camera_controls(state, frame)?;
                } else {
                    self.observe_generated_battlefield(state, frame)?;
                }
            }
            (Phase::Starting, _)
                if self.loading == Some(LoadingTarget::AfterQuit)
                    && state.frontend.screen == GameScreen::InGame =>
            {
                self.in_game_frames += 1;
                if self.in_game_frames == QUIT_AFTER_FRAMES {
                    // The abort box's Leave through the production action
                    // (queues the EXIT event, `0x004F192C`).
                    crate::app::input::abort::activate(
                        state,
                        crate::ui::shell::abort::AbortButton::Leave,
                    );
                    self.route.push(json!({"screen": "in game", "frame": frame,
                        "action": "Leave", "in_game_frames": self.in_game_frames}));
                    self.phase = Phase::Quitting;
                }
            }
            (Phase::Starting, _)
                if self.loading == Some(LoadingTarget::Diplomacy)
                    && state.frontend.screen == GameScreen::InGame =>
            {
                self.in_game_frames += 1;
                if self.in_game_frames == QUIT_AFTER_FRAMES {
                    press_diplomacy_tab(state)?;
                    self.route.push(json!({"screen": "in game", "frame": frame,
                        "action": "Tab", "dialog": 0x73, "in_game_frames": self.in_game_frames}));
                    // Rest on row 1's name, right of its text, so the status
                    // line shows that static's help.
                    let shell =
                        crate::app::frontend::skirmish_shell_render::current_in_game_shell_layout(
                            state,
                        )
                        .context("Diplomacy has no loaded geometry")?;
                    let name = crate::ui::shell::diplomacy::diplomacy_layout(
                        shell.width,
                        shell.height,
                        shell.layout,
                        shell.button_size,
                    )
                    .names[1];
                    let point = (name.x + name.w - 8, name.y + name.h / 2);
                    Self::set_pointer(state, point);
                    crate::app::input::dispatch::handle_cursor_moved_in_game(state);
                    ensure!(
                        state.match_state.match_presentation.diplomacy.hovered
                            == Some(crate::ui::shell::diplomacy::DiplomacyControl::Name(1)),
                        "the pointer missed row 1's name"
                    );
                    self.route
                        .push(json!({"frame": frame, "action": "pointer rests",
                        "dialog": 0x73, "point": [point.0, point.1]}));
                    self.phase = Phase::Diplomacy;
                }
            }
            (Phase::Diplomacy, _) => {
                ensure!(
                    state.match_state.match_presentation.in_game_menu
                        == crate::ui::pause_menu::InGameMenuState::Diplomacy
                        && crate::app::frontend::skirmish_shell_render::native_in_game_shell_active(
                            state
                        ),
                    "Diplomacy did not present with its required loaded artwork"
                );
                self.diplomacy_frames += 1;
            }
            (Phase::Starting, _)
                if self.defeat_route() && state.frontend.screen == GameScreen::InGame =>
            {
                self.in_game_frames += 1;
                if self.in_game_frames == QUIT_AFTER_FRAMES {
                    self.phase = Phase::Defeating;
                }
            }
            (Phase::Defeating, _) => self.lose_the_game(state, frame)?,
            (Phase::BattlefieldInspection, _) => {
                self.finish_battlefield_inspection(state, frame)?
            }
            (Phase::Replaying, _) => self.replay_the_game(state, frame)?,
            (Phase::Scoring, PresentedShell::Score) if Self::score_settled(state) => {
                if matches!(
                    self.loading,
                    Some(LoadingTarget::DefeatScore | LoadingTarget::DefeatReplay)
                ) && !self.battlefield_inspected
                {
                    self.begin_battlefield_inspection(state, frame)?;
                } else if self.loading == Some(LoadingTarget::DefeatReplay) {
                    self.replay_the_game(state, frame)?;
                } else if self.loading == Some(LoadingTarget::DefeatContinue) {
                    // Continue through the page's production handlers.
                    state.match_state.input.cursor_x = 720.0;
                    state.match_state.input.cursor_y = 556.0;
                    App::handle_score_shell_mouse_move(state);
                    App::handle_score_shell_mouse_down(state);
                    App::handle_score_shell_mouse_up(state);
                    state.match_state.input.cursor_x = EXPECTED_CURSOR_X as f32;
                    state.match_state.input.cursor_y = EXPECTED_CURSOR_Y as f32;
                    ensure!(
                        state.frontend.shell_exit.is_some(),
                        "Continue did not start the 0x108 teardown slide"
                    );
                    self.route
                        .push(json!({"dialog": 0x108, "frame": frame, "action": "Continue"}));
                    self.phase = Phase::Continuing;
                }
            }
            (Phase::SlideOut, PresentedShell::Skirmish) => {
                let target = self
                    .slide_out_tick
                    .context("slide-out phase without a tick")?;
                let tick = crate::app::frontend::shell_transition::shell_exit_wave(
                    state,
                    ShellSlideKind::Skirmish,
                )
                .context("Skirmish teardown ended before capture")?
                .compatibility_tick()
                .context("teardown slide without a tick clock")?;
                ensure!(tick <= target, "teardown slide passed tick {target}");
                if tick == target {
                    crate::app::frontend::shell_transition::hold_shell_exit_for_capture(state);
                    self.route.push(json!({"dialog": 0x102, "frame": frame,
                        "action": "hold teardown slide", "tick": tick}));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn settled(&self, state: &AppState) -> Result<bool> {
        self.guard(state)?;
        ensure!(
            state.frontend.shell_route.skirmish(),
            "skirmish capture did not reach the Skirmish shell"
        );
        ensure!(
            state.frontend.skirmish_shell_chrome.is_some(),
            "skirmish chrome unavailable"
        );
        ensure!(
            state.frontend.main_menu_movie.is_none(),
            "prior shell movie survived skirmish entry"
        );
        Ok(state.frontend.shell_first_paint_slide.is_none()
            && state.frontend.shell_slide_active_shell == Some(ShellSlideKind::Skirmish)
            && state.frontend.skirmish_shell_state.statics.is_terminal())
    }

    pub(super) fn ready(&self, state: &AppState) -> Result<bool> {
        self.guard(state)?;
        if let Some(commanders) = &self.ai_commanders {
            return Ok(self.phase == Phase::AiCommandersSetup
                && self.last_presented == Some(PresentedShell::Skirmish)
                && commanders.ready(state));
        }
        if self.ai_series_target == Some(AiSeriesTarget::Recovery) {
            return Ok(self.phase == Phase::AiSeriesRecovery
                && self.observer_battlefield_ready
                && self.last_presented == Some(PresentedShell::Score)
                && state.match_state.sim_runtime.is_none()
                && Self::score_settled(state));
        }
        if self.ai_series_target == Some(AiSeriesTarget::Setup) {
            return Ok(self.phase == Phase::AiSeriesSetup
                && self.ai_series_setup_presented
                && self.last_presented == Some(PresentedShell::Skirmish)
                && state.frontend.ai_series_setup.is_some());
        }
        if self.observer_client {
            return Ok(self.observer_battlefield_ready
                && (state.frontend.screen == GameScreen::InGame
                    || (App::score_shell_active(state) && Self::score_settled(state))));
        }
        if let Some(target) = self.entry_tick {
            return Ok(
                self.entry_held && entry_wave_tick(state, ShellSlideKind::Skirmish) == Some(target)
            );
        }
        match self.chooser {
            Some(ChooserTarget::Steady) => {
                return Ok(self.phase == Phase::Chooser
                    && self.pointer_rested
                    && Self::chooser_settled(state));
            }
            Some(ChooserTarget::Eject) => {
                return Ok(self.phase == Phase::Chooser
                    && self.pointer_rested
                    && state
                        .frontend
                        .skirmish_shell_state
                        .choose_map_modal
                        .as_ref()
                        .is_some_and(|modal| modal.eject_prompt.is_some()));
            }
            Some(ChooserTarget::Entry(target)) => {
                return Ok(self.phase == Phase::Chooser
                    && self.entry_held
                    && entry_wave_tick(state, ShellSlideKind::ChooseMap) == Some(target));
            }
            Some(ChooserTarget::RandomMap(target)) => {
                return Ok(match target {
                    RandomMapTarget::Steady => {
                        self.phase == Phase::RandomMap && Self::random_map_settled(state)
                    }
                    RandomMapTarget::Entry(tick) => {
                        self.phase == Phase::RandomMap
                            && self.entry_held
                            && entry_wave_tick(state, ShellSlideKind::RandomMap) == Some(tick)
                    }
                    RandomMapTarget::Hover(_) | RandomMapTarget::ListOpen => {
                        self.phase == Phase::RandomMap
                            && self.hovered
                            && Self::random_map_settled(state)
                    }
                    RandomMapTarget::CancelEntry(tick) => {
                        self.phase == Phase::RandomMapCancel
                            && self.entry_held
                            && entry_wave_tick(state, ShellSlideKind::ChooseMap) == Some(tick)
                    }
                    RandomMapTarget::UseMap => {
                        self.selected_scene.is_some()
                            && self.pointer_rested
                            && state.frontend.random_map_generation.is_none()
                            && state
                                .frontend
                                .skirmish_shell_state
                                .choose_map_modal
                                .is_none()
                            && self.last_presented == Some(PresentedShell::Skirmish)
                            && self.settled(state)?
                    }
                    RandomMapTarget::SeedBrowserReturn => {
                        self.phase == Phase::RandomMap
                            && self.pointer_rested
                            && state
                                .frontend
                                .skirmish_shell_state
                                .saved_seed_browser
                                .is_none()
                            && Self::random_map_settled(state)
                    }
                    RandomMapTarget::CancelSteady => {
                        self.phase == Phase::RandomMapCancel
                            && self.pointer_rested
                            && state
                                .frontend
                                .skirmish_shell_state
                                .random_map_setup_modal
                                .is_none()
                            && Self::chooser_settled(state)
                    }
                    RandomMapTarget::Eject(
                        crate::ui::skirmish_shell::EjectPromptButton::Cancel,
                    ) => {
                        self.phase == Phase::RandomMapEject
                            && self.answer_rested
                            && Self::chooser_settled(state)
                    }
                    RandomMapTarget::EjectBox => self.phase == Phase::RandomMap && self.eject_seen,
                    RandomMapTarget::Eject(crate::ui::skirmish_shell::EjectPromptButton::Ok) => {
                        self.phase == Phase::RandomMapEject
                            && self.answer_rested
                            && state
                                .frontend
                                .skirmish_shell_state
                                .choose_map_modal
                                .is_none()
                            && self.last_presented == Some(PresentedShell::Skirmish)
                            && self.settled(state)?
                    }
                });
            }
            Some(ChooserTarget::Return) => {
                return Ok(self.phase == Phase::ChooserReturn
                    && state
                        .frontend
                        .skirmish_shell_state
                        .choose_map_modal
                        .is_none()
                    && self.last_presented == Some(PresentedShell::Skirmish)
                    && self.settled(state)?);
            }
            None => {}
        }
        if self.loading == Some(LoadingTarget::DefeatReplay) {
            return Ok(self.phase == Phase::Replaying
                && self.replay_completed
                && state.frontend.screen == GameScreen::InGame);
        }
        if self.loading == Some(LoadingTarget::DefeatScore) {
            return Ok(self.battlefield_inspected
                && self.phase == Phase::Scoring
                && self.last_presented == Some(PresentedShell::Score)
                && App::score_shell_active(state)
                && Self::score_settled(state));
        }
        if self.loading == Some(LoadingTarget::DefeatContinue) {
            return Ok(self.phase == Phase::Continuing
                && state.frontend.screen == GameScreen::MainMenu
                && self.last_presented == Some(PresentedShell::Skirmish)
                && self.settled(state)?);
        }
        if self.loading == Some(LoadingTarget::Diplomacy) {
            return Ok(self.phase == Phase::Diplomacy && self.diplomacy_frames >= 2);
        }
        if self.loading == Some(LoadingTarget::AfterQuit) {
            return Ok(self.phase == Phase::Quitting
                && state.frontend.screen == GameScreen::MainMenu
                && (
                    state.match_state.input.cursor_x,
                    state.match_state.input.cursor_y,
                ) == (EXPECTED_CURSOR_X as f32, EXPECTED_CURSOR_Y as f32)
                && self.last_presented == Some(PresentedShell::Skirmish)
                && self.settled(state)?);
        }
        if let Some(target) = self.loading {
            let next = match target {
                LoadingTarget::Blank => crate::app::loading::pump::NextLoadingFrame::Blank,
                LoadingTarget::FirstFrame
                | LoadingTarget::AfterQuit
                | LoadingTarget::DefeatScore
                | LoadingTarget::DefeatContinue
                | LoadingTarget::DefeatReplay
                | LoadingTarget::Diplomacy => crate::app::loading::pump::NextLoadingFrame::First,
            };
            return Ok(self.phase == Phase::Starting
                && state.frontend.screen == GameScreen::Loading
                && crate::app::loading::pump::next_native_loading_frame(state) == Some(next));
        }
        if let Some(target) = self.slide_out_tick {
            return Ok(self.phase == Phase::SlideOut
                && self.last_presented == Some(PresentedShell::Skirmish)
                && crate::app::frontend::shell_transition::shell_exit_wave(
                    state,
                    ShellSlideKind::Skirmish,
                )
                .and_then(|wave| wave.compatibility_tick())
                    == Some(target));
        }
        if self.phase != Phase::Skirmish
            || self.last_presented != Some(PresentedShell::Skirmish)
            || self.selected_scene.is_none()
            || self.hover.is_some() != self.hovered
        {
            return Ok(false);
        }
        if !self.settled(state)? {
            return Ok(false);
        }
        ensure!(
            self.selected_scene.as_ref() == Some(&selected_scene(state)?),
            "selected skirmish state changed during capture"
        );
        Ok(true)
    }

    pub(super) fn manifest(
        &self,
        request: &ShellCaptureRequest,
        format: wgpu::TextureFormat,
        pixels: &[u8],
        frame: u32,
    ) -> Value {
        let mut manifest = json!({
            "schema_version": "vera20k.skirmish-shell-capture.v1", "checkpoint": request.checkpoint.as_str(),
            "parity_certification": "NONE", "presenter_domain": "final-swapchain-after-rgb565",
            "surface": {"width": request.width, "height": request.height, "format": format!("{format:?}"),
                "pixel_layout": "BGRA8", "row_order": "top-left", "row_stride": request.width * 4},
            "cursor": {"x": request.cursor_x, "y": request.cursor_y, "policy": "software-composited"},
            "route": self.route,
            "dialog_resource_id": if self.ai_series_target.is_some() || self.ai_commanders.is_some() { None } else { match (self.chooser, self.loading) {
                (
                    Some(
                        ChooserTarget::Steady
                        | ChooserTarget::Entry(_)
                        | ChooserTarget::Eject
                        | ChooserTarget::RandomMap(
                            RandomMapTarget::CancelEntry(_)
                            | RandomMapTarget::CancelSteady
                            | RandomMapTarget::Eject(
                                crate::ui::skirmish_shell::EjectPromptButton::Cancel,
                            )
                            | RandomMapTarget::EjectBox,
                        ),
                    ),
                    _,
                ) => Some(0x6b),
                (
                    Some(ChooserTarget::RandomMap(
                        RandomMapTarget::UseMap
                        | RandomMapTarget::Eject(crate::ui::skirmish_shell::EjectPromptButton::Ok),
                    )),
                    _,
                ) => Some(0x102),
                (Some(ChooserTarget::RandomMap(_)), _) => Some(0x105),
                (_, Some(_)) => None,
                _ => Some(0x102),
            } },
            "capture_frame": frame,
            "selection": self.selected_scene, "reveals_completed": self.entry_tick.is_none(),
            "ordinary_skirmish_frame": true,
            "input_enrollment": "UNENROLLED; asset/profile bytes require enrollment before native comparison",
            "frame": {"path": FRAME_FILE_NAME, "byte_length": pixels.len(),
                "sha256": crate::util::sha256::sha256_hex(pixels)}
        });
        // Only loading checkpoints carry the key; the validator
        // (`tools/shell_certification/skirmish.py`) pins the others' key set.
        if let Some(target) = self.loading {
            manifest["loading_frame"] = json!(format!("{target:?}"));
        }
        manifest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_series_loading_preserves_only_the_recorded_results_cursor() {
        let mut capture = SkirmishCapture::ai_series(AiSeriesTarget::Results);
        capture.phase = Phase::Starting;
        let guard = |cursor, expected_cursor| CaptureGuard {
            main_menu_screen: true,
            surface: (800, 600),
            expected_surface: (800, 600),
            failed: false,
            developer_shortcut: false,
            software_cursor: true,
            cursor,
            expected_cursor,
            interaction_active: false,
        };
        let tactical_center = (316.0, 284.0);
        let neutral = (400.0, 300.0);
        assert!(
            guard(tactical_center, capture.expected_cursor(true))
                .validate()
                .is_err()
        );
        capture.ai_series_terminal_outcomes.push(json!({"round":1}));
        capture.ai_series_rematch_cursor = Some(tactical_center);
        assert!(
            guard(tactical_center, capture.expected_cursor(true))
                .validate()
                .is_ok()
        );
        assert!(
            guard((317.0, 284.0), capture.expected_cursor(true))
                .validate()
                .is_err()
        );
        assert_eq!(capture.expected_cursor(false), neutral);
        capture.ai_series_second_round_seen = true;
        assert_eq!(capture.expected_cursor(true), neutral);
        capture.ai_series_second_round_seen = false;
        capture.ai_series_target = Some(AiSeriesTarget::Start);
        assert_eq!(capture.expected_cursor(true), neutral);
    }

    #[test]
    fn capture_guard_rejects_each_invalid_boundary() {
        let valid = CaptureGuard {
            main_menu_screen: true,
            surface: (800, 600),
            expected_surface: (800, 600),
            failed: false,
            developer_shortcut: false,
            software_cursor: true,
            cursor: (400.0, 300.0),
            expected_cursor: (400.0, 300.0),
            interaction_active: false,
        };
        assert!(valid.validate().is_ok());
        let compact = CaptureGuard {
            surface: (640, 480),
            expected_surface: (640, 480),
            ..valid
        };
        assert!(compact.validate().is_ok());
        assert!(
            CaptureGuard {
                surface: (800, 600),
                ..compact
            }
            .validate()
            .is_err()
        );
        for invalid in [
            CaptureGuard {
                main_menu_screen: false,
                ..valid
            },
            CaptureGuard {
                surface: (640, 480),
                ..valid
            },
            CaptureGuard {
                failed: true,
                ..valid
            },
            CaptureGuard {
                developer_shortcut: true,
                ..valid
            },
            CaptureGuard {
                software_cursor: false,
                ..valid
            },
            CaptureGuard {
                cursor: (401.0, 300.0),
                ..valid
            },
            CaptureGuard {
                cursor: (400.0, f32::NAN),
                ..valid
            },
            CaptureGuard {
                interaction_active: true,
                ..valid
            },
        ] {
            assert!(invalid.validate().is_err());
        }
    }
}

/// Tab through the ordinary key edge: the binding lookup falls back to
/// Request_Diplomacy_Menu (0x0055E08A), which must open 0x73.
pub(super) fn press_diplomacy_tab(state: &mut AppState) -> Result<()> {
    let tab = winit::keyboard::Key::Named(winit::keyboard::NamedKey::Tab);
    for edge in [
        winit::event::ElementState::Pressed,
        winit::event::ElementState::Released,
    ] {
        let paused_at_event = state.match_state.paused();
        crate::app::input::keyboard::in_game_key_edge(
            state,
            crate::app::input::keyboard::InGameKeyEdge {
                physical: winit::keyboard::KeyCode::Tab,
                logical: &tab,
                unmodified: &tab,
                location: winit::keyboard::KeyLocation::Standard,
                state: edge,
                repeat: false,
                text: None,
            },
            paused_at_event,
        );
    }
    ensure!(
        state.match_state.match_presentation.in_game_menu
            == crate::ui::pause_menu::InGameMenuState::Diplomacy,
        "Tab did not open the Diplomacy dialog"
    );
    Ok(())
}
