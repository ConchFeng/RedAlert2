//! Presentation-only second camera using the spectator and camera owners.

use crate::app::AppState;
use crate::app::input::camera::{self, spectator};
use crate::app::input::keyboard::InGameKeyEdge;
use crate::app::match_runtime::frame_pacer::wall_clock_ms;
use crate::sim::intern::InternedId;
use crate::ui::gadget::GadgetRect;
use crate::ui::second_view::{FollowMode, SecondViewEffect, SecondViewLayout};
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

#[derive(Default)]
pub(crate) struct SecondViewRuntime {
    pub(crate) director: spectator::SpectatorCamera,
    last_follow_ms: Option<u64>,
    notice: Option<String>,
}

pub(crate) fn viewport(state: &AppState) -> GadgetRect {
    let (x, y, width, height) = camera::tactical_viewport_px(state);
    let top = super::spectator_tools::toolbar_bottom(state)
        .unwrap_or(y as i32)
        .max(y as i32);
    GadgetRect::new(
        x as i32,
        top,
        width as i32,
        (y as i32 + height as i32 - top).max(0),
    )
}

pub(crate) fn layout(state: &AppState) -> Option<SecondViewLayout> {
    if !spectator::available(state) {
        return None;
    }
    state
        .match_state
        .match_presentation
        .second_view
        .layout(viewport(state))
}

pub(crate) fn supported(state: &AppState) -> bool {
    crate::ui::second_view::SecondViewState::supported(viewport(state))
}

pub(crate) fn open(state: &mut AppState) -> bool {
    if !spectator::available(state) {
        return false;
    }
    let viewport = viewport(state);
    let view = camera::current_view(state);
    if !state
        .match_state
        .match_presentation
        .second_view
        .open(viewport, view)
    {
        return false;
    }
    super::spectator_standings_ui::close(state);
    state.match_state.match_presentation.second_view_runtime = Default::default();
    state.match_state.input.cancel_tactical_drag();
    true
}

fn point(state: &AppState) -> (i32, i32) {
    let pointer = state.cursor_position();
    (pointer.0.floor() as i32, pointer.1.floor() as i32)
}

pub(crate) fn owns_pointer(state: &AppState) -> bool {
    if !spectator::available(state) {
        return false;
    }
    let view = &state.match_state.match_presentation.second_view;
    view.has_capture()
        || layout(state).is_some_and(|layout| layout.panel.contains(point(state).0, point(state).1))
}

pub(crate) fn consume_canceled_release(
    state: &mut AppState,
    button: MouseButton,
    pressed: bool,
) -> bool {
    if !spectator::available(state) {
        state.match_state.match_presentation.second_view.suspend();
    }
    state
        .match_state
        .match_presentation
        .second_view
        .consume_canceled_release(button, pressed)
}

pub(crate) fn mouse(state: &mut AppState, button: MouseButton, pressed: bool) -> bool {
    if !spectator::available(state) {
        return false;
    }
    let viewport = viewport(state);
    let point = point(state);
    let (consumed, effect) = state
        .match_state
        .match_presentation
        .second_view
        .mouse(point, button, pressed, viewport);
    if consumed {
        state.match_state.input.cancel_tactical_drag();
        apply_effect(state, effect);
    }
    consumed
}

pub(crate) fn pointer_moved(state: &mut AppState) -> bool {
    if !spectator::available(state) {
        return false;
    }
    let viewport = viewport(state);
    let point = point(state);
    state
        .match_state
        .match_presentation
        .second_view
        .pointer_moved(point, viewport)
}

pub(crate) fn wheel(state: &mut AppState, delta: f32) -> bool {
    if !spectator::available(state) {
        return false;
    }
    let viewport = viewport(state);
    let point = point(state);
    let consumed = state
        .match_state
        .match_presentation
        .second_view
        .wheel(point, delta, viewport);
    if consumed {
        clamp(state);
    }
    consumed
}

pub(crate) fn key(state: &mut AppState, edge: &InGameKeyEdge<'_>) -> bool {
    if !spectator::available(state)
        || !matches!(
            edge.physical,
            KeyCode::Tab | KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space | KeyCode::Escape
        )
    {
        return false;
    }
    let viewport = viewport(state);
    if edge.physical == KeyCode::Escape
        && edge.state.is_pressed()
        && !edge.repeat
        && state.match_state.match_presentation.second_view.is_open()
    {
        state.match_state.match_presentation.second_view.close();
        return true;
    }
    if !edge.state.is_pressed() || edge.repeat {
        return state
            .match_state
            .match_presentation
            .second_view
            .has_keyboard_focus();
    }
    let (consumed, effect) = state.match_state.match_presentation.second_view.key(
        edge.physical,
        state.match_state.input.hotkey_modifiers.shift_key(),
        viewport,
    );
    if consumed {
        state.match_state.input.keys_held.remove(&edge.physical);
        apply_effect(state, effect);
    }
    consumed
}

fn cycle_player(state: &mut AppState, direction: i32) {
    let projection = state
        .match_state
        .match_presentation
        .spectator_tools
        .standings
        .projection();
    let owners: Vec<_> = projection.rows().iter().map(|row| row.owner()).collect();
    let current = match state.match_state.match_presentation.second_view.mode {
        FollowMode::Player(owner) => {
            owner.and_then(|owner| owners.iter().position(|candidate| *candidate == owner))
        }
        _ => None,
    };
    let next = if owners.is_empty() {
        None
    } else {
        Some(
            owners[match current {
                Some(index) if direction < 0 => (index + owners.len() - 1) % owners.len(),
                Some(index) => (index + 1) % owners.len(),
                None if direction < 0 => owners.len() - 1,
                None => 0,
            }],
        )
    };
    state.match_state.match_presentation.second_view.mode = FollowMode::Player(next);
}

fn apply_effect(state: &mut AppState, effect: SecondViewEffect) {
    match effect {
        SecondViewEffect::SwapViews => {
            let main = camera::current_view(state);
            let incoming = state
                .match_state
                .match_presentation
                .second_view
                .swap_camera(main);
            camera::set_view(state, incoming);
        }
        SecondViewEffect::CyclePlayer(direction) => cycle_player(state, direction),
        _ => {}
    }
    let runtime = &mut state.match_state.match_presentation.second_view_runtime;
    runtime.last_follow_ms = None;
    runtime.notice = None;
    let action = state.match_state.match_presentation.second_view.mode == FollowMode::Action;
    if action != runtime.director.enabled() {
        runtime.director.toggle(wall_clock_ms());
    }
    update(state);
}

fn clamp(state: &mut AppState) {
    let Some(content) = layout(state).and_then(|layout| layout.content) else {
        return;
    };
    let view = state.match_state.match_presentation.second_view.camera;
    let center = camera::clamp_view_center(
        state,
        view.center,
        view.zoom,
        [content.w as f32, content.h as f32],
    );
    state
        .match_state
        .match_presentation
        .second_view
        .camera
        .center = center;
}

pub(crate) fn update(state: &mut AppState) {
    if !spectator::available(state) {
        state.match_state.match_presentation.second_view.suspend();
        state
            .match_state
            .match_presentation
            .second_view_runtime
            .last_follow_ms = None;
        return;
    }
    if !state.match_state.match_presentation.second_view.is_open() {
        let runtime = &mut state.match_state.match_presentation.second_view_runtime;
        if runtime.director.enabled() {
            runtime.director.toggle(wall_clock_ms());
        }
        return;
    }
    if state.match_state.match_presentation.second_view.minimized {
        return;
    }
    let now = wall_clock_ms();
    let mode = state.match_state.match_presentation.second_view.mode;
    if matches!(mode, FollowMode::Player(_))
        && state
            .match_state
            .match_presentation
            .second_view_runtime
            .last_follow_ms
            .is_some_and(|last| now.wrapping_sub(last) < 500)
    {
        clamp(state);
        return;
    }
    let target = match mode {
        FollowMode::Fixed => None,
        FollowMode::Action => state
            .match_state
            .match_presentation
            .second_view_runtime
            .director
            .target(now)
            .map(|point| [point.0, point.1]),
        FollowMode::Player(owner) => {
            state
                .match_state
                .match_presentation
                .second_view_runtime
                .last_follow_ms = Some(now);
            owner.and_then(|owner| player_target(state, owner))
        }
    };
    let notice = if matches!(mode, FollowMode::Player(_)) && target.is_none() {
        Some(super::spectator_tools::localized(
            state,
            "VERA:SecondViewNoSurvivors",
            "No surviving units",
        ))
    } else if mode == FollowMode::Action && target.is_none() {
        Some(super::spectator_tools::localized(
            state,
            "VERA:SecondViewWaiting",
            "Waiting for action",
        ))
    } else {
        None
    };
    state
        .match_state
        .match_presentation
        .second_view_runtime
        .notice = notice;
    if let Some(target) = target {
        state
            .match_state
            .match_presentation
            .second_view
            .camera
            .center = target;
    }
    clamp(state);
}

fn player_target(state: &AppState, owner: InternedId) -> Option<[f32; 2]> {
    let runtime = state.match_state.sim_runtime.as_ref()?;
    let [x, y, z] = state
        .match_state
        .match_presentation
        .spectator_tools
        .standings
        .projection()
        .focus_target(&runtime.simulation, owner)?;
    let point = crate::util::lepton::absolute_leptons_to_screen(x, y, z);
    Some([point.0, point.1])
}

pub(crate) fn player_label(state: &AppState) -> String {
    match state.match_state.match_presentation.second_view.mode {
        FollowMode::Player(Some(owner)) => {
            super::house_ui::house_ui_name(state, owner).unwrap_or_else(|| "AI".into())
        }
        _ => String::new(),
    }
}

pub(crate) fn status(state: &AppState) -> Option<String> {
    state
        .match_state
        .match_presentation
        .second_view_runtime
        .notice
        .clone()
}
