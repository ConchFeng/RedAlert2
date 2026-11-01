//! Input routing for the optional series setup and retained results controls.

use crate::app::frontend::ai_series_render;
use crate::app::{AppState, ai_series};
use winit::event::{KeyEvent, MouseButton};
use winit::keyboard::{KeyCode, PhysicalKey};

const ENTRY: u16 = 0xF220;

pub(crate) fn mouse(state: &mut AppState, button: MouseButton, pressed: bool) -> bool {
    let point = state.match_state.input.cursor_px();
    if ai_series_render::dialog_active(state) {
        if button == MouseButton::Left {
            let effect = if pressed {
                ai_series_render::mouse_down(state, point)
            } else {
                ai_series_render::mouse_up(state, point)
            };
            ai_series::apply_setup_effect(state, effect);
        }
        state.platform.window.request_redraw();
        return true;
    }
    let hit = ai_series_render::entry_rect(state)
        .filter(|rect| rect.contains(point.0, point.1))
        .map(|_| ENTRY);
    let captured = state.frontend.ai_series_dialog_buttons.pressed == Some(ENTRY);
    if button != MouseButton::Left || (!captured && hit.is_none()) {
        return false;
    }
    if pressed {
        state.frontend.ai_series_dialog_buttons.press(hit);
    } else if state.frontend.ai_series_dialog_buttons.release(hit) == Some(ENTRY) {
        ai_series::open_setup(state);
    }
    state.platform.window.request_redraw();
    true
}

pub(crate) fn cursor_moved(state: &mut AppState) -> bool {
    let point = state.match_state.input.cursor_px();
    if ai_series_render::dialog_active(state) {
        ai_series_render::mouse_move(state, point);
        state.platform.window.request_redraw();
        return true;
    }
    let hit = ai_series_render::entry_rect(state)
        .filter(|rect| rect.contains(point.0, point.1))
        .map(|_| ENTRY);
    if hit.is_some() || state.frontend.ai_series_dialog_buttons.pressed == Some(ENTRY) {
        state.frontend.ai_series_dialog_buttons.hovered = hit;
        state.platform.window.request_redraw();
    }
    false
}

pub(crate) fn key(state: &mut AppState, event: &KeyEvent) -> bool {
    let dialog = ai_series_render::dialog_active(state);
    let score = crate::app::ai_series::in_progress(state)
        && state.frontend.score_page.is_some()
        && !crate::app::match_runtime::post_match::viewing_battlefield(state);
    let code = match event.physical_key {
        PhysicalKey::Code(code) => Some(code),
        _ => None,
    };
    if !dialog
        && !(score
            && matches!(
                code,
                Some(
                    KeyCode::Tab
                        | KeyCode::Enter
                        | KeyCode::NumpadEnter
                        | KeyCode::Space
                        | KeyCode::Escape
                )
            ))
    {
        return false;
    }
    if event.state.is_pressed() && (!event.repeat || dialog) {
        let effect = ai_series_render::key(
            state,
            code,
            event.text.as_deref(),
            state.platform.live_modifiers.shift_key(),
        );
        ai_series::apply_setup_effect(state, effect);
        state.platform.window.request_redraw();
    }
    true
}
