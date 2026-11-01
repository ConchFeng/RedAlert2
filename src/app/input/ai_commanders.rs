//! Input capture for the offline AI commander setup.

use crate::app::frontend::ai_commanders_render;
use crate::app::{AppState, ai_commanders};
use winit::event::{KeyEvent, MouseButton};
use winit::keyboard::PhysicalKey;

use crate::app::frontend::ai_commanders_render::ENTRY;

pub(crate) fn mouse(state: &mut AppState, button: MouseButton, pressed: bool) -> bool {
    let point = state.match_state.input.cursor_px();
    if ai_commanders_render::dialog_active(state) {
        if button == MouseButton::Left {
            let effect = if pressed {
                ai_commanders_render::mouse_down(state, point)
            } else {
                ai_commanders_render::mouse_up(state, point)
            };
            ai_commanders::apply_effect(state, effect);
        }
        state.platform.window.request_redraw();
        return true;
    }
    let hit = ai_commanders_render::entry_rect(state)
        .filter(|rect| rect.contains(point.0, point.1))
        .map(|_| ENTRY);
    let captured = state.frontend.ai_commanders_buttons.pressed == Some(ENTRY);
    if button != MouseButton::Left || (!captured && hit.is_none()) {
        return false;
    }
    if pressed {
        state.frontend.ai_commanders_buttons.press(hit);
    } else if state.frontend.ai_commanders_buttons.release(hit) == Some(ENTRY) {
        ai_commanders::open_setup(state);
    }
    state.platform.window.request_redraw();
    true
}

pub(crate) fn cursor_moved(state: &mut AppState) -> bool {
    let point = state.match_state.input.cursor_px();
    if ai_commanders_render::dialog_active(state) {
        ai_commanders_render::mouse_move(state, point);
        state.platform.window.request_redraw();
        return true;
    }
    let hit = ai_commanders_render::entry_rect(state)
        .filter(|rect| rect.contains(point.0, point.1))
        .map(|_| ENTRY);
    if hit.is_some() || state.frontend.ai_commanders_buttons.pressed == Some(ENTRY) {
        state.frontend.ai_commanders_buttons.hovered = hit;
        state.platform.window.request_redraw();
    }
    false
}

pub(crate) fn key(state: &mut AppState, event: &KeyEvent) -> bool {
    let dialog = ai_commanders_render::dialog_active(state);
    let code = match event.physical_key {
        PhysicalKey::Code(code) => Some(code),
        _ => None,
    };
    if !dialog {
        return false;
    }
    if event.state.is_pressed() && (!event.repeat || dialog) {
        let effect =
            ai_commanders_render::key(state, code, state.platform.live_modifiers.shift_key());
        ai_commanders::apply_effect(state, effect);
        state.platform.window.request_redraw();
    }
    true
}
