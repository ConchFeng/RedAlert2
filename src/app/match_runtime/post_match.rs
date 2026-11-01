//! Offline Skirmish result actions over the retained score and world owners.

use crate::app::match_runtime::session::SessionMode;
use crate::app::{App, AppState, GameScreen};
use crate::sim::house_state::HouseOutcomeKind;

pub(crate) fn available(state: &AppState) -> bool {
    state.match_state.session_mode() == SessionMode::Skirmish
        && !state.frontend.external_launch
        && state.frontend.score_page.is_some()
        && state.match_state.sim_runtime.is_some()
}

pub(crate) fn viewing_battlefield(state: &AppState) -> bool {
    state.frontend.screen == GameScreen::InGame && available(state)
}

pub(crate) fn replay_available(state: &AppState) -> bool {
    available(state)
        && state.frontend.shell_route.skirmish()
        && !state.match_state.startup.restored()
}

pub(crate) fn result_text(state: &AppState, kind: HouseOutcomeKind) -> (String, String) {
    let observing = state
        .match_state
        .sim_runtime
        .as_ref()
        .is_some_and(|runtime| {
            let sim = &runtime.simulation;
            sim.session()
                .current_house()
                .is_some_and(|owner| sim.session().is_observer_house(owner))
        });
    if observing {
        (
            "Battle Finished".into(),
            "The AI battle has ended. Review the results or inspect the battlefield.".into(),
        )
    } else {
        (
            super::scenario_exit::outcome_title(kind).into(),
            super::scenario_exit::outcome_detail(kind).into(),
        )
    }
}

pub(crate) fn view_battlefield(state: &mut AppState) {
    if !available(state) {
        return;
    }
    crate::app::ai_series::pause_for_inspection(state);
    state.frontend.shell_first_paint_slide = None;
    state
        .match_state
        .input
        .cancel_tactical_gesture_and_targeting();
    state.match_state.input.clear_command_latches();
    state.match_state.input.keys_held.clear();
    state.match_state.sandbox_full_visibility = true;
    state.frontend.screen = GameScreen::InGame;
    state.platform.window.request_redraw();
}

pub(crate) fn show_results(state: &mut AppState) {
    if !viewing_battlefield(state) {
        return;
    }
    crate::app::ai_series::resume_results(state);
    let kind = state
        .match_state
        .sim_runtime
        .as_ref()
        .and_then(|runtime| {
            let sim = &runtime.simulation;
            sim.session()
                .current_house()
                .and_then(|owner| sim.ready_outcome_for_owner(owner))
        })
        .map_or(HouseOutcomeKind::Defeat, |outcome| outcome.kind);
    let (title, detail) = result_text(state, kind);
    state.match_state.input.keys_held.clear();
    state
        .match_state
        .input
        .cancel_tactical_gesture_and_targeting();
    state.frontend.screen = GameScreen::MissionResult { title, detail };
    state.frontend.shell_first_paint_slide = None;
    state
        .frontend
        .shell_controller
        .reset_to(crate::ui::score_shell::SCORE_DIALOG, false);
    state.platform.window.request_redraw();
}

pub(crate) fn play_again(state: &mut AppState) {
    if !replay_available(state) {
        return;
    }
    App::close_finished_scenario(state);
    state.audio.stop_theme();
    App::resume_shell_after_match(state);
    App::start_game_from_shell(state);
}

pub(crate) fn service_battlefield(state: &mut AppState) {
    if !state.platform.window_active || state.focus_freeze_active() {
        return;
    }
    crate::app::input::camera::queue_keyboard_scroll(state);
    let mouse = crate::app::input::camera::poll_mouse_scroll(state);
    crate::app::input::camera::commit_camera_scroll(state, mouse);
    crate::app::input::camera::animate_zoom(state);
}

pub(crate) fn status(state: &AppState) -> Option<&'static str> {
    if viewing_battlefield(state) {
        return Some("Battle finished - Esc: Results");
    }
    let sim = &state.match_state.sim_runtime.as_ref()?.simulation;
    let owner = sim.session().current_house()?;
    sim.can_continue_watching(owner)
        .then_some("Watching the remaining AIs - Esc: Options")
}
