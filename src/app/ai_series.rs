mod model;
mod repository;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::app::match_bootstrap::{MatchSeedClock, MatchSeedSource};
use crate::app::shell_random_map::FrozenRandomMapAcceptance;
use crate::app::{App, AppState, GameScreen};
use crate::skirmish_launch::SkirmishLaunchSession;
use crate::ui::ai_series::{
    AiSeriesMapChoice, AiSeriesSetup, AiSeriesSetupEffect, AiSeriesSetupState,
};
use crate::ui::score_shell::{ScorePage, ScoreRow, ScoreScreenModel, ScoreTexts};
use crate::ui::skirmish_shell::SkirmishShellState;
use crate::util::time::Instant;
use model::{
    RoundOutcome, RoundRecord, SeriesCountdown, SeriesDescriptor, SeriesEntrant, SeriesLedger,
    SeriesMap,
};
use repository::SeriesRepository;

pub(crate) const START_NEXT: u16 = 0xF210;
pub(crate) const PAUSE_SERIES: u16 = 0xF211;
pub(crate) const END_SERIES: u16 = 0xF212;
pub(crate) const RETRY_ROUND: u16 = 0xF213;
pub(crate) const SKIP_ROUND: u16 = 0xF214;
pub(crate) const TOGGLE_STANDINGS: u16 = 0xF215;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeriesMapRecipe {
    identity: String,
    frozen: Option<FrozenRandomMapAcceptance>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LaunchRecipe {
    template: SkirmishLaunchSession,
    fixed: SkirmishLaunchSession,
    randomize_each_round: bool,
    maps: Vec<SeriesMapRecipe>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RoundLaunchRecipe {
    session: SkirmishLaunchSession,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SeriesPhase {
    Loading,
    Running,
    Results,
    Failed,
    Complete,
}

pub(crate) struct AiSeriesRuntime {
    repository: SeriesRepository,
    ledger: SeriesLedger,
    recipe: LaunchRecipe,
    prior_setup: SkirmishShellState,
    phase: SeriesPhase,
    round: u16,
    current_launch: Option<SkirmishLaunchSession>,
    countdown: Option<SeriesCountdown>,
    stop_after_current: bool,
    manually_paused: bool,
    inspecting: bool,
    focus_paused: bool,
    clock_origin: Instant,
    failure: String,
    pending_result: Option<RoundRecord>,
    match_page: Option<ScorePage>,
    showing_standings: bool,
    recover_has_integrity_errors: bool,
}

impl AiSeriesRuntime {
    fn now_ms(&self) -> u64 {
        self.clock_origin
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }

    fn pause_countdown(&mut self) {
        let now = self.now_ms();
        if let Some(countdown) = &mut self.countdown {
            countdown.pause(now);
        }
    }

    fn resume_countdown(&mut self) {
        if !self.manually_paused
            && !self.inspecting
            && !self.focus_paused
            && !self.stop_after_current
        {
            let now = self.now_ms();
            if let Some(countdown) = &mut self.countdown {
                countdown.resume(now);
            }
        }
    }
}

fn settings_root(state: &AppState) -> Result<PathBuf> {
    Ok(state
        .platform
        .game_config
        .as_ref()
        .context("Game settings are unavailable")?
        .paths
        .settings_dir()
        .to_owned())
}

pub(crate) fn setup_available(state: &AppState) -> bool {
    let setup = &state.frontend.skirmish_shell_state;
    state.frontend.ai_series.is_none()
        && !state.frontend.external_launch
        && state.frontend.screen == GameScreen::MainMenu
        && state.frontend.shell_route.skirmish()
        && setup.player_role == crate::skirmish_launch::LaunchHumanRole::Observer
        && setup
            .opponents
            .iter()
            .filter(|opponent| opponent.is_active())
            .count()
            >= 2
        && state.frontend.skirmish_modes.iter().any(|mode| {
            mode.id == setup.selected_mode_id
                && mode.class == crate::skirmish_modes::MpModeClass::FreeForAll
        })
}

pub(crate) fn open_setup(state: &mut AppState) {
    if !setup_available(state) {
        return;
    }
    let entrants = state
        .frontend
        .skirmish_shell_state
        .opponents
        .iter()
        .filter(|opponent| opponent.is_active())
        .count();
    let choices = state
        .frontend
        .scenario_catalog
        .shell_maps()
        .iter()
        .enumerate()
        .map(|(record_index, map)| AiSeriesMapChoice {
            record_index,
            label: map.display_name.clone(),
            capacity: usize::try_from(map.player_capacity).unwrap_or(0),
        })
        .filter(|choice| choice.capacity >= entrants + 1)
        .collect();
    let first_seed = crate::app::match_bootstrap::OrdinaryMatchSeedClock.low_u32();
    let mut setup = AiSeriesSetupState::new(
        choices,
        state.frontend.skirmish_shell_state.selected_map_idx,
        entrants,
        first_seed,
    );
    let recoverable = settings_root(state)
        .and_then(SeriesRepository::list)
        .is_ok_and(|series| !series.is_empty());
    setup.set_recovery_available(recoverable);
    state.frontend.ai_series_setup = Some(setup);
    state.platform.window.request_redraw();
}

pub(crate) fn apply_setup_effect(state: &mut AppState, effect: AiSeriesSetupEffect) {
    let result = match effect {
        AiSeriesSetupEffect::None => return,
        AiSeriesSetupEffect::Cancel => {
            state.frontend.ai_series_setup = None;
            state.platform.window.request_redraw();
            return;
        }
        AiSeriesSetupEffect::Start(setup) => start_series(state, setup),
        AiSeriesSetupEffect::RecoverLast => recover_last(state),
        AiSeriesSetupEffect::ActivateSeriesAction(id) => {
            activate_score_action(state, id);
            return;
        }
    };
    if let Err(error) = result {
        let message = format!("{error:#}");
        if let Some(setup) = &mut state.frontend.ai_series_setup {
            setup.error = Some(message);
        } else {
            let _ = loading_failed(state, &message);
        }
    }
    state.platform.window.request_redraw();
}

fn raw_launch(state: &AppState) -> Result<SkirmishLaunchSession> {
    crate::ui::skirmish_shell::launch_session(
        &state.frontend.skirmish_shell_state,
        state.frontend.scenario_catalog.shell_maps(),
        &state.frontend.skirmish_modes,
        state.persistence.options_profile.solo_skirmish,
    )
    .map_err(|error| anyhow::anyhow!("Skirmish setup is not ready: {error:?}"))
}

fn map_fingerprint(state: &AppState, recipe: &SeriesMapRecipe) -> Result<(String, Option<u32>)> {
    if let Some(frozen) = &recipe.frozen {
        ensure!(
            frozen
                .selected_map_file()
                .eq_ignore_ascii_case(&recipe.identity),
            "Accepted random map identity differs"
        );
        let ini = crate::rules::ini_parser::IniFile::from_bytes(frozen.seed_bytes())?;
        let mut options = crate::map::rmg::RmgOptions::default();
        options.apply_sed(&ini);
        return Ok((
            crate::util::sha256::sha256_hex(frozen.seed_bytes()),
            Some(u32::from(options.seed_u16())),
        ));
    }
    ensure!(
        !recipe.identity.to_ascii_lowercase().ends_with(".sed"),
        "Generate and accept this random map before adding it to a series"
    );
    let root = &state
        .platform
        .game_config
        .as_ref()
        .context("Game resources are unavailable")?
        .paths
        .ra2_dir;
    let assets = state
        .process_assets
        .manager()
        .context("Game resources are busy")?;
    let loaded = crate::map::source::load_map_ini_by_name_or_path_with_assets(
        root,
        &recipe.identity,
        assets,
    )?;
    ensure!(
        crate::map::basic::parse_basic_section(&loaded.ini).skip_score != Some(true),
        "This map skips the results screen and cannot run in an AI series"
    );
    let (_, digest) = loaded
        .source
        .physical_identity()
        .context("Map has no stable source identity")?;
    Ok((digest.to_owned(), None))
}

fn start_series(state: &mut AppState, setup: AiSeriesSetup) -> Result<()> {
    ensure!(
        setup_available(state),
        "Choose Observer and at least two AIs in Free For All first"
    );
    let prior_setup = state.frontend.skirmish_shell_state.clone();
    let template = raw_launch(state)?;
    let mut recipes = Vec::new();
    let mut maps = Vec::new();
    for index in &setup.map_indices {
        let map = state
            .frontend
            .scenario_catalog
            .shell_maps()
            .get(*index)
            .context("A selected map is no longer available")?;
        ensure!(
            usize::try_from(map.player_capacity).unwrap_or(0) >= template.opponents.len() + 1,
            "A selected map cannot seat every AI entrant"
        );
        let recipe = SeriesMapRecipe {
            identity: map.file_name.clone(),
            frozen: state
                .frontend
                .random_map_retention
                .frozen_acceptance(Some(&map.file_name)),
        };
        let (content_sha256, map_seed) = map_fingerprint(state, &recipe)?;
        maps.push(SeriesMap {
            identity: recipe.identity.clone(),
            content_sha256,
            map_seed,
        });
        recipes.push(recipe);
    }
    let fixed = state
        .frontend
        .multiplayer_shell_runtime
        .close_shell_transaction(
            &state.frontend.skirmish_shell_state,
            state.frontend.scenario_catalog.shell_maps(),
            &state.frontend.skirmish_modes,
            &template,
        )?;
    let normalized = crate::sim::scenario_bootstrap::normalized_launch_slots(&fixed);
    let entrants = fixed
        .opponents
        .iter()
        .zip(normalized.iter().filter(|slot| !slot.is_human))
        .map(|(ai, slot)| SeriesEntrant {
            slot_index: ai.raw_slot_index,
            name: slot.owner_name.clone(),
            country: state
                .rules()
                .and_then(|rules| {
                    rules.country_name(crate::rules::ruleset::CountryIdx(ai.country.raw_index()))
                })
                .unwrap_or("Unknown country")
                .to_owned(),
            commander: slot.commander.clone(),
            color: ai.color_index.to_string(),
        })
        .collect();
    let seeds = match setup.custom_seeds {
        Some(seeds) => seeds,
        None => (0..u32::from(setup.match_count))
            .map(|offset| {
                setup
                    .first_seed
                    .checked_add(offset)
                    .context("Seed sequence exceeds 4294967295")
            })
            .collect::<Result<Vec<_>>>()?,
    };
    let timestamp = crate::util::time::SystemTime::now()
        .duration_since(crate::util::time::UNIX_EPOCH)
        .context("The system clock is before 1970")?
        .as_nanos();
    let descriptor = SeriesDescriptor {
        id: format!("series-{timestamp}"),
        name: setup.name,
        round_limit: u16::from(setup.match_count),
        entrants,
        maps,
        seeds,
    };
    let ledger = SeriesLedger::new(descriptor)?;
    let repository = SeriesRepository::at(settings_root(state)?, &ledger.descriptor().id)?;
    let recipe = LaunchRecipe {
        template,
        fixed,
        randomize_each_round: setup.randomize_each_round,
        maps: recipes,
    };
    repository.create(ledger.descriptor())?;
    repository.save_launch(&recipe)?;
    state.frontend.multiplayer_shell_runtime.persist_snapshot();
    state.frontend.ai_series_setup = None;
    state.frontend.ai_series = Some(AiSeriesRuntime {
        repository,
        ledger,
        recipe,
        prior_setup,
        phase: SeriesPhase::Loading,
        round: 1,
        current_launch: None,
        countdown: None,
        stop_after_current: false,
        manually_paused: false,
        inspecting: false,
        focus_paused: false,
        clock_origin: Instant::now(),
        failure: String::new(),
        pending_result: None,
        match_page: None,
        showing_standings: false,
        recover_has_integrity_errors: false,
    });
    launch_round(state, 1)
}

fn country_randomized_template(recipe: &LaunchRecipe) -> Result<SkirmishLaunchSession> {
    let mut template = recipe.template.clone();
    for (index, human) in template.humans.iter_mut().enumerate() {
        let fixed = match human.raw_slot_index {
            Some(slot) => recipe
                .fixed
                .humans
                .iter()
                .find(|fixed| fixed.raw_slot_index == Some(slot)),
            None => recipe.fixed.humans.get(index),
        }
        .context("A series human slot is missing from the accepted roster")?;
        human.slot.color_index = fixed.slot.color_index;
        human.slot.color_random = false;
    }
    for ai in &mut template.opponents {
        let fixed = recipe
            .fixed
            .opponents
            .iter()
            .find(|fixed| fixed.raw_slot_index == ai.raw_slot_index)
            .context("An AI slot is missing from the accepted series roster")?;
        ai.color_index = fixed.color_index;
        ai.color_random = false;
    }
    Ok(template)
}

struct SeriesSeedClock(u32);

impl MatchSeedClock for SeriesSeedClock {
    fn low_u32(&mut self) -> u32 {
        self.0
    }
    fn source(&self) -> MatchSeedSource {
        MatchSeedSource::Controlled
    }
    fn seed_authority_certifying(&self) -> bool {
        false
    }
}

fn launch_round(state: &mut AppState, round: u16) -> Result<()> {
    let mut runtime = state
        .frontend
        .ai_series
        .take()
        .context("No active AI series")?;
    runtime.round = round;
    runtime.phase = SeriesPhase::Loading;
    let result = (|| {
        let descriptor = runtime.ledger.descriptor();
        let map_index = descriptor
            .map_index(round)
            .context("Match is outside the series schedule")?;
        let map = runtime
            .recipe
            .maps
            .get(map_index)
            .context("Series map recipe is missing")?;
        let (fingerprint, _) = map_fingerprint(state, map)?;
        ensure!(
            fingerprint == descriptor.maps[map_index].content_sha256,
            "Map contents changed since the series was created"
        );
        let launch = match runtime
            .repository
            .load_round_launch::<RoundLaunchRecipe>(descriptor, round)?
        {
            Some(recorded) => recorded.session,
            None => {
                let mut session = if runtime.recipe.randomize_each_round && round > 1 {
                    let template = country_randomized_template(&runtime.recipe)?;
                    state
                        .frontend
                        .multiplayer_shell_runtime
                        .close_shell_transaction(
                            &runtime.prior_setup,
                            state.frontend.scenario_catalog.shell_maps(),
                            &state.frontend.skirmish_modes,
                            &template,
                        )?
                } else {
                    runtime.recipe.fixed.clone()
                };
                session.selected_map_file = Some(map.identity.clone());
                let recorded = RoundLaunchRecipe {
                    session: session.clone(),
                };
                runtime
                    .repository
                    .save_round_launch(descriptor, round, &recorded)?;
                session
            }
        };
        ensure!(
            launch.selected_map_file.as_deref() == Some(map.identity.as_str()),
            "Saved match uses another map"
        );
        ensure!(
            launch
                .opponents
                .iter()
                .map(|ai| ai.raw_slot_index)
                .eq(runtime
                    .recipe
                    .fixed
                    .opponents
                    .iter()
                    .map(|ai| ai.raw_slot_index)),
            "Saved match entrants differ from the series"
        );
        let seed = descriptor.seed(round).context("Match seed is missing")?;
        runtime.round = round;
        runtime.current_launch = Some(launch.clone());
        runtime.countdown = None;
        runtime.pending_result = None;
        runtime.phase = SeriesPhase::Loading;
        runtime.inspecting = false;
        runtime.showing_standings = false;
        runtime.match_page = None;
        runtime.failure.clear();
        if state.frontend.score_page.is_some() {
            App::close_finished_scenario(state);
            state.audio.stop_theme();
            App::resume_shell_after_match(state);
        }
        App::teardown_skirmish_shell_for_start(state);
        App::start_skirmish_session_with_frozen(
            state,
            launch,
            &mut SeriesSeedClock(seed),
            map.frozen.as_ref(),
        )
    })();
    state.frontend.ai_series = Some(runtime);
    if let Err(error) = &result {
        let _ = loading_failed(state, &format!("{error:#}"));
    }
    result
}

pub(crate) fn in_progress(state: &AppState) -> bool {
    state.frontend.ai_series.is_some()
}

pub(crate) fn toolbar_status(state: &AppState) -> String {
    let Some(runtime) = &state.frontend.ai_series else {
        return String::new();
    };
    let progress = format!(
        "Series {}/{}",
        runtime.round,
        runtime.ledger.descriptor().round_limit
    );
    if runtime.stop_after_current {
        format!("{progress} - stopping after this match")
    } else {
        progress
    }
}

pub(crate) fn stop_requested(state: &AppState) -> bool {
    state
        .frontend
        .ai_series
        .as_ref()
        .is_some_and(|runtime| runtime.stop_after_current)
}

pub(crate) fn status_text(state: &AppState) -> String {
    let Some(runtime) = &state.frontend.ai_series else {
        return String::new();
    };
    let descriptor = runtime.ledger.descriptor();
    let prefix = format!(
        "{} - Match {} of {}",
        descriptor.name, runtime.round, descriptor.round_limit
    );
    match runtime.phase {
        SeriesPhase::Loading => format!("{prefix} - Loading"),
        SeriesPhase::Running if runtime.stop_after_current => {
            format!("{prefix} - stopping after this match")
        }
        SeriesPhase::Running => prefix,
        SeriesPhase::Failed => format!("{prefix} - Incomplete: {}", runtime.failure),
        SeriesPhase::Complete if runtime.ledger.is_finished() => {
            format!("{} - Series complete", descriptor.name)
        }
        SeriesPhase::Complete => format!(
            "{} - Series stopped; completed results saved",
            descriptor.name
        ),
        SeriesPhase::Results => {
            let countdown = runtime.countdown.as_ref();
            if runtime.manually_paused || runtime.inspecting || runtime.focus_paused {
                format!("{prefix} - Series paused")
            } else {
                format!(
                    "{prefix} - Next match in {}s",
                    countdown.map_or(0, |clock| clock.remaining_seconds(runtime.now_ms()))
                )
            }
        }
    }
}

pub(crate) fn score_actions(state: &AppState) -> Vec<(u16, String)> {
    let Some(runtime) = &state.frontend.ai_series else {
        return Vec::new();
    };
    if runtime.phase == SeriesPhase::Failed && runtime.recover_has_integrity_errors {
        return vec![(END_SERIES, "End Series".into())];
    }
    let mut actions = match runtime.phase {
        SeriesPhase::Results => vec![
            (START_NEXT, "Start Now".into()),
            (
                PAUSE_SERIES,
                if runtime.manually_paused {
                    "Resume Series"
                } else {
                    "Pause Series"
                }
                .into(),
            ),
        ],
        SeriesPhase::Failed => vec![
            (RETRY_ROUND, "Retry".into()),
            (SKIP_ROUND, "Skip Match".into()),
        ],
        SeriesPhase::Complete => Vec::new(),
        SeriesPhase::Loading | SeriesPhase::Running => return Vec::new(),
    };
    if (runtime.phase != SeriesPhase::Failed || state.frontend.score_page.is_some())
        && (!runtime.showing_standings || runtime.match_page.is_some())
    {
        actions.push((
            TOGGLE_STANDINGS,
            if runtime.showing_standings {
                "Match Results"
            } else {
                "Series Standings"
            }
            .into(),
        ));
    }
    actions.push((END_SERIES, "End Series".into()));
    actions
}

pub(crate) fn activate_score_action(state: &mut AppState, id: u16) {
    if !score_actions(state).iter().any(|(action, _)| *action == id) {
        return;
    }
    match id {
        END_SERIES => end_series(state),
        PAUSE_SERIES => {
            if let Some(runtime) = &mut state.frontend.ai_series {
                runtime.manually_paused = !runtime.manually_paused;
                if runtime.manually_paused {
                    runtime.pause_countdown()
                } else {
                    runtime.resume_countdown()
                }
            }
        }
        START_NEXT => {
            let round = state
                .frontend
                .ai_series
                .as_mut()
                .and_then(|runtime| runtime.countdown.as_mut())
                .and_then(SeriesCountdown::take_now);
            if let Some(round) = round {
                let _ = launch_round(state, round);
            }
        }
        RETRY_ROUND => {
            let pending = state
                .frontend
                .ai_series
                .as_ref()
                .is_some_and(|runtime| runtime.pending_result.is_some());
            if pending {
                persist_pending_result(state);
            } else if let Some(round) = state
                .frontend
                .ai_series
                .as_ref()
                .map(|runtime| runtime.round)
            {
                let _ = launch_round(state, round);
            }
        }
        SKIP_ROUND => skip_round(state),
        TOGGLE_STANDINGS => toggle_standings(state),
        _ => {}
    }
    state.platform.window.request_redraw();
}

pub(crate) fn installed(state: &mut AppState) {
    if let Some(runtime) = &mut state.frontend.ai_series {
        if runtime.phase == SeriesPhase::Loading {
            runtime.phase = SeriesPhase::Running;
            runtime.failure.clear();
        }
    }
}

fn show_failure(state: &mut AppState, message: String) {
    let Some(runtime) = &mut state.frontend.ai_series else {
        return;
    };
    runtime.phase = SeriesPhase::Failed;
    runtime.failure = message.clone();
    if let Some(countdown) = &mut runtime.countdown {
        countdown.cancel()
    }
    state.frontend.screen = GameScreen::MissionResult {
        title: "AI Series Paused".into(),
        detail: message,
    };
    state.frontend.shell_first_paint_slide = None;
    state.platform.window.request_redraw();
}

pub(crate) fn loading_failed(state: &mut AppState, message: &str) -> bool {
    if !state.frontend.ai_series.as_ref().is_some_and(|runtime| {
        runtime.phase == SeriesPhase::Loading || runtime.phase == SeriesPhase::Failed
    }) {
        return false;
    }
    show_failure(state, message.to_owned());
    true
}

pub(crate) fn interrupted(state: &mut AppState) {
    if state
        .frontend
        .ai_series
        .as_ref()
        .is_some_and(|runtime| matches!(runtime.phase, SeriesPhase::Loading | SeriesPhase::Running))
    {
        show_failure(
            state,
            "The match was interrupted. Its result was not counted.".into(),
        );
    }
}

pub(crate) fn external_world_replacement(state: &mut AppState) {
    let Some(mut runtime) = state.frontend.ai_series.take() else {
        return;
    };
    if let Some(countdown) = &mut runtime.countdown {
        countdown.cancel();
    }
    state.frontend.ai_series_setup = None;
    state.frontend.skirmish_shell_state = runtime.prior_setup;
    state.frontend.multiplayer_shell_runtime.pack_durable_state(
        &state.frontend.skirmish_shell_state,
        state.frontend.scenario_catalog.shell_maps(),
        &state.frontend.skirmish_modes,
    );
    state.frontend.multiplayer_shell_runtime.persist_snapshot();
    log::info!(
        "An external game load ended the active AI series attempt; saved series history remains recoverable"
    );
}

pub(crate) fn round_completed(state: &mut AppState) {
    let Some(runtime) = state
        .frontend
        .ai_series
        .as_ref()
        .filter(|runtime| runtime.phase == SeriesPhase::Running)
    else {
        return;
    };
    let Some(sim) = state
        .match_state
        .sim_runtime
        .as_ref()
        .map(|runtime| &runtime.simulation)
    else {
        interrupted(state);
        return;
    };
    let Some(snapshot) = sim.terminal_score_snapshot() else {
        interrupted(state);
        return;
    };
    let Some(launch) = runtime.current_launch.as_ref() else {
        show_failure(state, "The accepted match identity is missing.".into());
        return;
    };
    let outcome = outcome_from_terminal(launch, snapshot, sim.interner());
    let descriptor = runtime.ledger.descriptor();
    let Some(seed) = descriptor.seed(runtime.round) else {
        return;
    };
    let Some(map_index) = descriptor.map_index(runtime.round) else {
        return;
    };
    let record = RoundRecord {
        series_id: descriptor.id.clone(),
        round: runtime.round,
        seed,
        map_index,
        outcome,
    };
    if let Some(runtime) = &mut state.frontend.ai_series {
        runtime.pending_result = Some(record);
        runtime.match_page = state.frontend.score_page.clone();
    }
    persist_pending_result(state);
}

fn outcome_from_terminal(
    launch: &SkirmishLaunchSession,
    snapshot: &crate::sim::score::TerminalScoreSnapshot,
    interner: &crate::sim::intern::StringInterner,
) -> RoundOutcome {
    let slots = crate::sim::scenario_bootstrap::normalized_launch_slots(launch);
    let survivors: Vec<_> = launch
        .opponents
        .iter()
        .zip(slots.iter().filter(|slot| !slot.is_human))
        .filter_map(|(ai, slot)| {
            let owner = interner.get(&slot.owner_name)?;
            snapshot
                .rows
                .iter()
                .any(|row| row.owner == owner && row.survived)
                .then_some(ai.raw_slot_index)
        })
        .collect();
    match survivors.as_slice() {
        [slot_index] => RoundOutcome::Winner {
            slot_index: *slot_index,
        },
        _ => RoundOutcome::Draw,
    }
}

fn persist_pending_result(state: &mut AppState) {
    let Some(mut runtime) = state.frontend.ai_series.take() else {
        return;
    };
    let result = (|| {
        let record = runtime
            .pending_result
            .as_ref()
            .context("No finished match result is pending")?;
        runtime
            .repository
            .record(runtime.ledger.descriptor(), record)?;
        runtime.ledger.record(record.clone())?;
        runtime.pending_result = None;
        runtime.failure.clear();
        if let Some(next) = runtime
            .ledger
            .next_round()
            .filter(|_| !runtime.stop_after_current)
        {
            runtime.phase = SeriesPhase::Results;
            runtime.countdown = Some(SeriesCountdown::new(next, runtime.now_ms()));
            if runtime.manually_paused || runtime.inspecting || runtime.focus_paused {
                runtime.pause_countdown();
            }
        } else {
            runtime.phase = SeriesPhase::Complete;
            runtime.countdown = None;
        }
        Ok::<(), anyhow::Error>(())
    })();
    state.frontend.ai_series = Some(runtime);
    if let Err(error) = result {
        show_failure(state, format!("Could not save this result: {error:#}"))
    }
    state.platform.window.request_redraw();
}

fn skip_round(state: &mut AppState) {
    let Some(runtime) = &mut state.frontend.ai_series else {
        return;
    };
    if runtime.pending_result.is_some() {
        persist_pending_result(state);
        return;
    }
    let descriptor = runtime.ledger.descriptor();
    let Some(seed) = descriptor.seed(runtime.round) else {
        return;
    };
    let Some(map_index) = descriptor.map_index(runtime.round) else {
        return;
    };
    runtime.pending_result = Some(RoundRecord {
        series_id: descriptor.id.clone(),
        round: runtime.round,
        seed,
        map_index,
        outcome: RoundOutcome::Skipped,
    });
    persist_pending_result(state);
}

pub(crate) fn pause_for_inspection(state: &mut AppState) {
    if let Some(runtime) = &mut state.frontend.ai_series {
        runtime.inspecting = true;
        runtime.pause_countdown();
    }
}

pub(crate) fn resume_results(state: &mut AppState) {
    if let Some(runtime) = &mut state.frontend.ai_series {
        runtime.inspecting = false;
        runtime.resume_countdown();
    }
}

pub(crate) fn stop_after_current(state: &mut AppState) {
    if let Some(runtime) = &mut state.frontend.ai_series {
        runtime.stop_after_current = true;
        if let Some(countdown) = &mut runtime.countdown {
            countdown.stop_after_current()
        }
        if runtime.phase == SeriesPhase::Results {
            runtime.phase = SeriesPhase::Complete
        }
    }
    state.platform.window.request_redraw();
}

pub(crate) fn poll(state: &mut AppState) -> Option<Instant> {
    let inactive = !state.platform.window_active || state.focus_freeze_active();
    let runtime = state.frontend.ai_series.as_mut()?;
    if runtime.phase != SeriesPhase::Results {
        return None;
    }
    if inactive {
        if !runtime.focus_paused {
            runtime.focus_paused = true;
            runtime.pause_countdown();
        }
        return None;
    }
    if runtime.focus_paused {
        runtime.focus_paused = false;
        runtime.resume_countdown();
    }
    if runtime.inspecting
        || runtime.manually_paused
        || runtime.stop_after_current
        || !matches!(state.frontend.screen, GameScreen::MissionResult { .. })
    {
        return None;
    }
    let now_ms = runtime.now_ms();
    let ready = runtime
        .countdown
        .as_mut()
        .and_then(|countdown| countdown.take_ready(now_ms));
    if let Some(round) = ready {
        let _ = launch_round(state, round);
        None
    } else {
        state.platform.window.request_redraw();
        Some(Instant::now() + Duration::from_millis(250))
    }
}

fn end_series(state: &mut AppState) {
    let Some(mut runtime) = state.frontend.ai_series.take() else {
        return;
    };
    if let Some(countdown) = &mut runtime.countdown {
        countdown.cancel()
    }
    if let Some(page) = runtime.match_page.take() {
        state.frontend.score_page = Some(page)
    }
    App::close_finished_scenario(state);
    state.audio.stop_theme();
    App::resume_shell_after_match(state);
    state.frontend.screen = GameScreen::MainMenu;
    state.frontend.skirmish_shell_state = runtime.prior_setup;
    state.frontend.ai_series_setup = None;
    state.frontend.shell_route = crate::app::shell_route::ShellRoute::Skirmish;
    state.frontend.multiplayer_shell_runtime.pack_durable_state(
        &state.frontend.skirmish_shell_state,
        state.frontend.scenario_catalog.shell_maps(),
        &state.frontend.skirmish_modes,
    );
    state.frontend.multiplayer_shell_runtime.persist_snapshot();
    state.platform.window.request_redraw();
}

fn toggle_standings(state: &mut AppState) {
    let Some(mut runtime) = state.frontend.ai_series.take() else {
        return;
    };
    if runtime.showing_standings {
        state.frontend.score_page = runtime.match_page.clone();
        runtime.showing_standings = false;
        state.frontend.ai_series = Some(runtime);
        return;
    }
    let existing_visible_page = state.frontend.score_page.is_some();
    if let Some(page) = &state.frontend.score_page {
        runtime.match_page = Some(page.clone());
    }
    let mut colors = BTreeMap::new();
    if let (Some(launch), Some(world)) = (&runtime.current_launch, &state.match_state.sim_runtime) {
        let sim = &world.simulation;
        let sources = crate::app::presentation::house_ui::HouseUiSources::from_state(state, "");
        let slots = crate::sim::scenario_bootstrap::normalized_launch_slots(launch);
        for (ai, slot) in launch
            .opponents
            .iter()
            .zip(slots.iter().filter(|slot| !slot.is_human))
        {
            if let Some(owner) = sim.interner().get(&slot.owner_name) {
                colors.insert(ai.raw_slot_index, sources.text_rgb(sim, owner));
            }
        }
    }
    let descriptor = runtime.ledger.descriptor();
    let base = runtime.match_page.as_ref().map(|page| &page.model);
    let rows = runtime
        .ledger
        .standings()
        .into_iter()
        .filter_map(|standing| {
            let entrant = descriptor
                .entrants
                .iter()
                .find(|entrant| entrant.slot_index == standing.slot_index)?;
            let name = entrant.name.clone();
            Some(ScoreRow {
                name,
                rgb: colors
                    .get(&standing.slot_index)
                    .copied()
                    .unwrap_or([220, 220, 220]),
                kills: u32::from(standing.wins),
                losses: u32::from(standing.draws),
                built: u32::from(standing.completed),
                score: i32::from(
                    standing
                        .completed
                        .saturating_sub(standing.wins + standing.draws),
                ),
            })
        })
        .collect();
    let model = ScoreScreenModel {
        title_key: "GUI:SkirmishScore",
        game_number: u32::from(runtime.round),
        elapsed_seconds: 0,
        side: base.map_or(0, |model| model.side),
        theme: base.and_then(|model| model.theme.clone()),
        rows,
    };
    let texts = ScoreTexts {
        game: descriptor.name.clone(),
        time: format!(
            "{} of {} matches recorded",
            runtime.ledger.records().len(),
            descriptor.round_limit
        ),
        headers: ["Player", "Wins", "Draws", "Played", "Losses"].map(str::to_owned),
    };
    if !existing_visible_page || state.frontend.score_art.is_none() {
        App::open_score_page(
            state,
            model.clone(),
            descriptor.name.clone(),
            "Saved AI series standings".into(),
        );
    }
    if state.frontend.score_art.is_some() {
        state.frontend.score_page = Some(standings_page(
            model,
            &texts,
            Instant::now(),
            existing_visible_page,
        ));
        runtime.showing_standings = true;
    } else {
        state.frontend.score_page = None;
        runtime.failure =
            "Could not load the results artwork. Completed series records remain saved.".into();
        state.frontend.screen = GameScreen::MissionResult {
            title: "AI Series Standings".into(),
            detail: runtime.failure.clone(),
        };
    }
    state.frontend.ai_series = Some(runtime);
}

fn standings_page(
    model: ScoreScreenModel,
    texts: &ScoreTexts,
    now: Instant,
    already_shown: bool,
) -> ScorePage {
    let mut page = ScorePage::open(model, texts, now);
    if already_shown {
        page.start_reveals(now);
    }
    page
}

fn recover_last(state: &mut AppState) -> Result<()> {
    ensure!(
        state.frontend.ai_series.is_none(),
        "End the current series before recovering another"
    );
    let root = settings_root(state)?;
    let descriptor = SeriesRepository::list(&root)?
        .pop()
        .context("No saved AI series is available")?;
    let repository = SeriesRepository::at(&root, &descriptor.id)?;
    let recovered = repository.recover()?;
    let recipe: LaunchRecipe = repository.load_launch()?;
    ensure!(
        recipe.maps.len() == descriptor.maps.len(),
        "Saved map rotation is incomplete"
    );
    ensure!(
        recipe.fixed.mode.class == crate::skirmish_modes::MpModeClass::FreeForAll
            && recipe.fixed.local_player().is_observer(),
        "Saved series is not an observer Free For All match"
    );
    let round = recovered
        .ledger
        .next_round()
        .unwrap_or(descriptor.round_limit);
    let finished = recovered.ledger.is_finished();
    let failure = if recovered.issues.is_empty() {
        "The previous match was not completed. Retry it or skip it without scoring a defeat."
            .to_owned()
    } else {
        recovered
            .issues
            .iter()
            .map(|issue| format!("Match {}: {}", issue.round, issue.message))
            .collect::<Vec<_>>()
            .join("; ")
    };
    state.frontend.ai_series_setup = None;
    state.frontend.ai_series = Some(AiSeriesRuntime {
        repository,
        ledger: recovered.ledger,
        recipe,
        prior_setup: state.frontend.skirmish_shell_state.clone(),
        phase: if finished {
            SeriesPhase::Complete
        } else {
            SeriesPhase::Failed
        },
        round,
        current_launch: None,
        countdown: None,
        stop_after_current: false,
        manually_paused: true,
        inspecting: false,
        focus_paused: false,
        clock_origin: Instant::now(),
        failure: failure.clone(),
        pending_result: None,
        match_page: None,
        showing_standings: false,
        recover_has_integrity_errors: !recovered.issues.is_empty(),
    });
    state.frontend.screen = GameScreen::MissionResult {
        title: "AI Series Recovery".into(),
        detail: failure,
    };
    state.frontend.shell_first_paint_slide = None;
    if finished {
        toggle_standings(state)
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skirmish_launch::{
        AiDifficulty, LaunchCountry, LaunchHumanRole, LaunchStartPosition, LaunchTeam,
        PreFillAiHouseSlot, PreFillHouseRoster, PreFillHumanHouse, SkirmishAiSlot,
        SkirmishHumanSlot, SkirmishLaunchMode, SkirmishLaunchOptions, SkirmishLocalSlot,
    };

    fn launch_fixture() -> SkirmishLaunchSession {
        SkirmishLaunchSession {
            mode: SkirmishLaunchMode::stock(2).unwrap(),
            selected_map_file: Some("arena.map".into()),
            humans: vec![SkirmishHumanSlot {
                raw_slot_index: Some(0),
                name: "Computer1".into(),
                role: LaunchHumanRole::Observer,
                slot: SkirmishLocalSlot {
                    country: LaunchCountry::America,
                    country_random: false,
                    color_index: 0,
                    color_random: false,
                    start_position: LaunchStartPosition::Auto,
                    team: LaunchTeam::None,
                },
            }],
            local_player_index: 0,
            ai_player_count: 2,
            opponents: [3, 29]
                .into_iter()
                .enumerate()
                .map(|(index, raw_slot_index)| SkirmishAiSlot {
                    raw_slot_index,
                    country: LaunchCountry::Russia,
                    country_random: false,
                    color_index: index as u8 + 1,
                    color_random: false,
                    start_position: LaunchStartPosition::Auto,
                    team: LaunchTeam::None,
                    difficulty: AiDifficulty::Normal,
                })
                .collect(),
            pre_fill_house_roster: PreFillHouseRoster::new(
                vec![PreFillHumanHouse {
                    priority: 0,
                    source_order: 0,
                    observer: true,
                }],
                [3, 29]
                    .into_iter()
                    .map(|slot_index| PreFillAiHouseSlot {
                        slot_index,
                        valid: true,
                    })
                    .collect(),
            ),
            options: SkirmishLaunchOptions::default(),
        }
    }

    #[test]
    fn terminal_results_map_compact_ai_owners_to_stable_slots_and_exclude_the_observer() {
        let launch = launch_fixture();
        let slots = crate::sim::scenario_bootstrap::normalized_launch_slots(&launch);
        let mut interner = crate::sim::intern::StringInterner::new();
        let mut snapshot = crate::sim::score::TerminalScoreSnapshot {
            rows: slots
                .iter()
                .map(|slot| crate::sim::score::TerminalScoreRowSnapshot {
                    owner: interner.intern(&slot.owner_name),
                    country: None,
                    survived: slot.observer,
                    kills: 0,
                    losses: 0,
                    built: 0,
                    raw_score: 0,
                    score: 0,
                })
                .collect(),
        };
        let ai_rows: Vec<_> = slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| (!slot.is_human).then_some(index))
            .collect();
        snapshot.rows[ai_rows[1]].survived = true;
        assert_eq!(
            outcome_from_terminal(&launch, &snapshot, &interner),
            RoundOutcome::Winner { slot_index: 29 }
        );
        snapshot.rows[ai_rows[0]].survived = true;
        assert_eq!(
            outcome_from_terminal(&launch, &snapshot, &interner),
            RoundOutcome::Draw
        );
        snapshot.rows[ai_rows[1]].survived = false;
        assert_eq!(
            outcome_from_terminal(&launch, &snapshot, &interner),
            RoundOutcome::Winner { slot_index: 3 }
        );
        snapshot.rows[ai_rows[0]].survived = false;
        assert_eq!(
            outcome_from_terminal(&launch, &snapshot, &interner),
            RoundOutcome::Draw
        );
    }
    #[test]
    fn randomized_countries_keep_fixed_colors_by_stable_slot_without_changing_country_flags() {
        let mut template = launch_fixture();
        template.humans[0].slot.country_random = true;
        template.humans[0].slot.color_random = true;
        for ai in &mut template.opponents {
            ai.country_random = true;
            ai.color_random = true;
        }
        let mut fixed = launch_fixture();
        fixed.humans[0].slot.color_index = 7;
        fixed.opponents[0].color_index = 21;
        fixed.opponents[1].color_index = 59;
        fixed.opponents.reverse();
        let recipe = LaunchRecipe {
            template,
            fixed,
            randomize_each_round: true,
            maps: Vec::new(),
        };
        let round = country_randomized_template(&recipe).unwrap();
        assert_eq!(round.humans[0].slot.color_index, 7);
        assert!(!round.humans[0].slot.color_random);
        assert!(round.humans[0].slot.country_random);
        assert_eq!(
            round
                .opponents
                .iter()
                .map(|ai| (ai.raw_slot_index, ai.color_index))
                .collect::<Vec<_>>(),
            [(3, 21), (29, 59)]
        );
        assert!(
            round
                .opponents
                .iter()
                .all(|ai| ai.country_random && !ai.color_random)
        );
        assert!(recipe.template.opponents.iter().all(|ai| ai.color_random));
    }
    #[test]
    fn replacing_an_already_visible_score_table_starts_its_reveals_and_keeps_match_reveals() {
        let now = Instant::now();
        let model = ScoreScreenModel {
            rows: vec![ScoreRow {
                name: "AI 1".into(),
                rgb: [20, 60, 120],
                kills: 1,
                losses: 0,
                built: 1,
                score: 0,
            }],
            ..Default::default()
        };
        let texts = ScoreTexts {
            game: "AI Championship".into(),
            time: "Match 1".into(),
            headers: ["Player", "Wins", "Draws", "Played", "Losses"].map(str::to_owned),
        };
        let mut original = ScorePage::open(model.clone(), &texts, now);
        original.start_reveals(now);
        assert!(
            original
                .table_mut()
                .iter_mut()
                .all(|text| text.paint(now).is_some())
        );
        original.commit_presented();
        let mut retained_match = original.clone();
        let mut series = standings_page(model.clone(), &texts, now, true);
        assert!(
            series
                .table_mut()
                .iter_mut()
                .all(|text| text.paint(now).is_some())
        );
        assert!(
            retained_match
                .table_mut()
                .iter_mut()
                .all(|text| text.paint(now).is_some())
        );
        let mut recovery = standings_page(model, &texts, now, false);
        assert!(
            recovery
                .table_mut()
                .iter_mut()
                .all(|text| text.paint(now).is_none())
        );
    }
}
