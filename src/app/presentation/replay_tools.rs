//! Replay controls are presentation-only clients of the playback navigation owner.

use crate::app::AppState;
use crate::app::input::keyboard::InGameKeyEdge;
use crate::app::replay_navigation::{self, ReplayNavigationView};
use crate::render::batch::SpriteInstance;
use crate::ui::gadget::GadgetRect;
use crate::ui::replay_tools::{ReplayToolAction, ReplayToolsLayout};
use winit::event::{KeyEvent, MouseButton};
use winit::keyboard::{KeyCode, PhysicalKey};

pub(crate) fn available(state: &AppState) -> bool {
    replay_navigation::view(state).is_some()
}

fn layout_for(state: &AppState, view: &ReplayNavigationView) -> Option<ReplayToolsLayout> {
    let tools = &state.match_state.match_presentation.replay_tools;
    let browser = view.browser.as_ref();
    let mut layout = ReplayToolsLayout::new(
        crate::app::input::camera::tactical_viewport_size(state),
        tools.highlights,
        browser.is_some(),
        view.seeking.is_some() || view.validating,
        view.ready,
        tools.page,
        browser.map_or(view.bookmarks.len(), Vec::len),
        if let Some(rows) = browser {
            tools
                .selected_recording
                .is_some_and(|index| index < rows.len())
        } else {
            tools
                .selected_bookmark
                .is_some_and(|index| index < view.bookmarks.len())
        },
        view.selected_unit.is_some() || view.following,
    )?;
    if view.finished {
        for control in &mut layout.controls {
            if matches!(
                control.action,
                ReplayToolAction::TogglePause | ReplayToolAction::CycleSpeed
            ) {
                control.enabled = false;
            }
        }
    }
    Some(layout)
}

pub(crate) fn layout(state: &AppState) -> Option<ReplayToolsLayout> {
    layout_for(state, &replay_navigation::view(state)?)
}

pub(crate) fn action_center(state: &AppState, action: ReplayToolAction) -> Option<(i32, i32)> {
    layout(state)?.action_center(action)
}

pub(crate) fn owns_pointer(state: &AppState) -> bool {
    state
        .match_state
        .match_presentation
        .replay_tools
        .has_capture()
        || layout(state).is_some_and(|layout| layout.contains(point(state)))
}

fn point(state: &AppState) -> (i32, i32) {
    let point = state.cursor_position();
    (point.0.floor() as i32, point.1.floor() as i32)
}

pub(crate) fn mouse(state: &mut AppState, button: MouseButton, pressed: bool) -> bool {
    let Some(view) = replay_navigation::view(state) else {
        return false;
    };
    let Some(layout) = layout_for(state, &view) else {
        return true;
    };
    let point = point(state);
    let (consumed, action) = state
        .match_state
        .match_presentation
        .replay_tools
        .mouse(&layout, point, button, pressed);
    state.match_state.input.cancel_tactical_drag();
    if let Some(action) = action {
        activate(state, action, &view);
    }
    if !consumed
        && !pressed
        && button == MouseButton::Left
        && view.ready
        && view.browser.is_none()
        && view.seeking.is_none()
    {
        let viewport = crate::app::input::camera::tactical_viewport_size(state);
        if point.0 >= 0
            && point.1 >= 0
            && point.0 < viewport.0 as i32
            && point.1 < viewport.1 as i32
        {
            replay_navigation::select_unit(state, (point.0 as f32, point.1 as f32));
        }
    }
    true
}

pub(crate) fn pointer_moved(state: &mut AppState) {
    let hovered = layout(state).and_then(|layout| layout.hit(point(state)));
    state.match_state.match_presentation.replay_tools.hovered = hovered;
}

pub(crate) fn wheel(state: &mut AppState, delta: f32) -> bool {
    let Some(layout) = layout(state) else {
        return false;
    };
    if !layout.contains(point(state))
        && !state
            .match_state
            .match_presentation
            .replay_tools
            .has_capture()
    {
        return false;
    }
    if delta.is_finite() && delta != 0.0 && layout.panel.is_some() && !layout.seeking {
        state.match_state.match_presentation.replay_tools.page = if delta > 0.0 {
            layout.page.saturating_sub(1)
        } else {
            (layout.page + 1).min(layout.pages - 1)
        };
    }
    true
}

pub(crate) fn text_input(state: &mut AppState, text: &str) {
    state
        .match_state
        .match_presentation
        .replay_tools
        .insert_text(text);
}

pub(crate) fn key_event(state: &mut AppState, event: &KeyEvent) {
    if let PhysicalKey::Code(code) = event.physical_key {
        key(
            state,
            &InGameKeyEdge {
                physical: code,
                logical: &event.logical_key,
                unmodified: &event.logical_key,
                location: event.location,
                state: event.state,
                repeat: event.repeat,
                text: event.text.as_deref(),
            },
        );
    }
}

pub(crate) fn key(state: &mut AppState, edge: &InGameKeyEdge<'_>) -> bool {
    let Some(view) = replay_navigation::view(state) else {
        return false;
    };
    let Some(layout) = layout_for(state, &view) else {
        if edge.physical == KeyCode::Escape && edge.state.is_pressed() && !edge.repeat {
            activate(state, ReplayToolAction::ClosePanel, &view);
        }
        return true;
    };
    let focus = state.match_state.match_presentation.replay_tools.focused;
    if matches!(
        edge.physical,
        KeyCode::ArrowLeft | KeyCode::ArrowRight | KeyCode::ArrowUp | KeyCode::ArrowDown
    ) && focus.is_none()
        && view.browser.is_none()
        && view.seeking.is_none()
        && !view.validating
    {
        if edge.state.is_pressed() {
            state.match_state.input.keys_held.insert(edge.physical);
        } else {
            state.match_state.input.keys_held.remove(&edge.physical);
        }
        return true;
    }
    if !edge.state.is_pressed() {
        state.match_state.input.keys_held.remove(&edge.physical);
    }
    let action = state.match_state.match_presentation.replay_tools.key(
        &layout,
        edge.physical,
        edge.text,
        state.match_state.input.hotkey_modifiers.shift_key(),
        edge.state.is_pressed(),
        edge.repeat,
    );
    if let Some(action) = action {
        activate(state, action, &view);
    }
    true
}

fn activate(state: &mut AppState, action: ReplayToolAction, view: &ReplayNavigationView) {
    use ReplayToolAction::*;
    match action {
        TogglePause if view.ready && view.seeking.is_none() => {
            replay_navigation::toggle_pause(state)
        }
        CycleSpeed if view.ready && view.seeking.is_none() => replay_navigation::cycle_speed(state),
        ToggleFollow if view.ready && view.seeking.is_none() => {
            replay_navigation::toggle_follow(state)
        }
        OpenBrowser => {
            state.match_state.match_presentation.replay_tools.page = 0;
            replay_navigation::return_to_browser(state);
        }
        ClosePanel if view.seeking.is_some() || view.validating => {
            replay_navigation::cancel_seek(state)
        }
        ClosePanel if view.browser.is_some() => replay_navigation::close_browser(state),
        ClosePanel if state.match_state.match_presentation.replay_tools.highlights => {
            state.match_state.match_presentation.replay_tools.highlights = false;
            state.match_state.match_presentation.replay_tools.focused = None;
        }
        ClosePanel => replay_navigation::return_to_browser(state),
        ToggleHighlights if view.ready => {
            if view.browser.is_some() {
                replay_navigation::close_browser(state);
            }
            let tools = &mut state.match_state.match_presentation.replay_tools;
            tools.highlights = !tools.highlights;
            tools.page = 0;
            tools.focused = tools.highlights.then_some(EditLabel);
        }
        AddBookmark if view.ready && view.seeking.is_none() => {
            let label =
                std::mem::take(&mut state.match_state.match_presentation.replay_tools.label);
            replay_navigation::add_bookmark(state, label);
        }
        SelectBookmark(index) => {
            state
                .match_state
                .match_presentation
                .replay_tools
                .selected_bookmark = Some(index)
        }
        SelectRecording(index) => {
            state
                .match_state
                .match_presentation
                .replay_tools
                .selected_recording = Some(index)
        }
        SeekSelected if view.ready => {
            if let Some(index) = state
                .match_state
                .match_presentation
                .replay_tools
                .selected_bookmark
            {
                replay_navigation::seek_bookmark(state, index);
            }
        }
        OpenSelectedRecording => {
            if let Some(index) = state
                .match_state
                .match_presentation
                .replay_tools
                .selected_recording
            {
                replay_navigation::open_recording(state, index);
            }
        }
        CancelSeek => replay_navigation::cancel_seek(state),
        PreviousPage | NextPage => {
            if let Some(layout) = layout_for(state, view) {
                state.match_state.match_presentation.replay_tools.page = if action == PreviousPage {
                    layout.page.saturating_sub(1)
                } else {
                    (layout.page + 1).min(layout.pages - 1)
                };
            }
        }
        _ => {}
    }
    state.platform.window.request_redraw();
}

fn localized(state: &AppState, key: &str, fallback: &str) -> String {
    super::spectator_tools::localized(state, key, fallback)
}

fn timestamp(tick: u64, tick_ms: u32) -> String {
    let seconds = tick.saturating_mul(u64::from(tick_ms)) / 1000;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn caption(state: &AppState, view: &ReplayNavigationView, action: ReplayToolAction) -> String {
    use ReplayToolAction::*;
    match action {
        TogglePause if view.finished => localized(state, "VERA:ReplayFinished", "Finished"),
        TogglePause if view.paused => localized(state, "VERA:ReplayResume", "Resume"),
        TogglePause => localized(state, "VERA:ReplayPause", "Pause"),
        CycleSpeed => format!("{}x", view.speed),
        ToggleHighlights => localized(state, "VERA:ReplayHighlights", "Highlights"),
        ToggleFollow if view.following => {
            localized(state, "VERA:ReplayStopFollow", "Stop Following")
        }
        ToggleFollow => localized(state, "VERA:ReplayFollow", "Follow Unit"),
        OpenBrowser => localized(state, "VERA:ReplayBrowser", "Recordings"),
        ClosePanel if view.browser.is_some() => localized(state, "VERA:ReplayBack", "Back"),
        ClosePanel => localized(state, "VERA:ReplayClose", "Close"),
        AddBookmark => localized(state, "VERA:ReplayAddBookmark", "Add Bookmark"),
        EditLabel
            if state
                .match_state
                .match_presentation
                .replay_tools
                .label
                .is_empty() =>
        {
            localized(state, "VERA:ReplayLabel", "Optional bookmark label")
        }
        EditLabel => state
            .match_state
            .match_presentation
            .replay_tools
            .label
            .clone(),
        PreviousPage => localized(state, "VERA:ReplayPrevious", "Previous"),
        NextPage => localized(state, "VERA:ReplayNext", "Next"),
        SelectBookmark(index) => view
            .bookmarks
            .get(index)
            .map(|bookmark| {
                format!(
                    "{}  {}",
                    timestamp(bookmark.tick, view.tick_ms),
                    if bookmark.label.is_empty() {
                        localized(state, "VERA:ReplayBookmark", "Bookmark")
                    } else {
                        bookmark.label.clone()
                    }
                )
            })
            .unwrap_or_default(),
        SeekSelected => localized(state, "VERA:ReplayGoTo", "Go To"),
        CancelSeek => localized(state, "VERA:ReplayCancel", "Cancel"),
        SelectRecording(index) => view
            .browser
            .as_ref()
            .and_then(|rows| rows.get(index))
            .map(|row| row.label.clone())
            .unwrap_or_default(),
        OpenSelectedRecording => localized(state, "VERA:ReplayOpen", "Open Recording"),
    }
}

fn text(
    state: &AppState,
    out: &mut Vec<SpriteInstance>,
    label: &str,
    rect: GadgetRect,
    centered: bool,
    color: [f32; 3],
) {
    let font = &state.renderer.bit_font;
    let width = (font.text_width(label) as f32).max(1.0);
    let scale = ((rect.w - 10).max(1) as f32 / width)
        .min((rect.h - 4).max(1) as f32 / font.cell_height())
        .min(1.0);
    let x = rect.x as f32
        + if centered {
            (rect.w as f32 - width * scale) / 2.0
        } else {
            5.0
        };
    out.extend(font.build_text(
        label,
        x,
        rect.y as f32 + (rect.h as f32 - font.cell_height() * scale) / 2.0,
        scale,
        0.0,
        color,
        [
            state.match_state.input.camera_x,
            state.match_state.input.camera_y,
        ],
    ));
}

pub(crate) fn build_instances(state: &AppState) -> (Vec<SpriteInstance>, Vec<SpriteInstance>) {
    let Some(view) = replay_navigation::view(state) else {
        return (Vec::new(), Vec::new());
    };
    let Some(layout) = layout_for(state, &view) else {
        return (Vec::new(), Vec::new());
    };
    let tools = &state.match_state.match_presentation.replay_tools;
    let camera = [
        state.match_state.input.camera_x,
        state.match_state.input.camera_y,
    ];
    let fill = |rect: GadgetRect, tint: [f32; 3]| SpriteInstance {
        position: [rect.x as f32 + camera[0], rect.y as f32 + camera[1]],
        size: [rect.w as f32, rect.h as f32],
        uv_origin: [0.0, 0.0],
        uv_size: [1.0, 1.0],
        tint,
        alpha: 1.0,
        ..Default::default()
    };
    let mut fills = vec![fill(layout.toolbar, [0.035, 0.045, 0.055])];
    let mut texts = Vec::new();
    if let Some(panel) = layout.panel {
        fills.push(fill(panel, [0.035, 0.045, 0.055]));
    }
    let position = localized(state, "VERA:ReplayPosition", "Replay  {current} / {end}")
        .replace("{current}", &timestamp(view.tick, view.tick_ms))
        .replace("{end}", &timestamp(view.final_tick, view.tick_ms));
    let position = if view.finished {
        format!(
            "{position} | {}",
            localized(state, "VERA:ReplayFinished", "Finished")
        )
    } else {
        position
    };
    text(
        state,
        &mut texts,
        &position,
        layout.position,
        false,
        [0.9, 0.94, 0.96],
    );
    let status = if layout.compact {
        Some(localized(
            state,
            "VERA:ReplayLargerWindow",
            "Enlarge the window to use replay controls.",
        ))
    } else {
        view.status.clone()
    }
    .unwrap_or_else(|| {
        if let Some(rows) = &view.browser {
            return if rows.is_empty() {
                localized(
                    state,
                    "VERA:ReplayNoRecordings",
                    "No recordings found in the replay folder.",
                )
            } else {
                localized(
                    state,
                    "VERA:ReplayChooseRecording",
                    "Select a recording, then choose Open Recording.",
                )
            };
        }
        if tools.highlights {
            return localized(
                state,
                "VERA:ReplayHighlightHelp",
                "Add Bookmark records this moment. Select a highlight, then Go To.",
            );
        }
        match view.selected_unit {
            Some(id) if view.following => {
                localized(state, "VERA:ReplayFollowingUnit", "Following unit #{unit}")
                    .replace("{unit}", &id.to_string())
            }
            Some(id) => localized(
                state,
                "VERA:ReplaySelectedUnit",
                "Unit #{unit} selected. Choose Follow Unit to track it.",
            )
            .replace("{unit}", &id.to_string()),
            None if view.ready => localized(
                state,
                "VERA:ReplayChooseUnit",
                "Click a visible unit, then choose Follow Unit.",
            ),
            None => localized(
                state,
                "VERA:ReplayValidating",
                "Validating recording before enabling playback controls...",
            ),
        }
    });
    text(
        state,
        &mut texts,
        &status,
        layout.status,
        false,
        [0.9, 0.94, 0.96],
    );
    if let Some(title) = layout.title {
        let title_text = if let Some(progress) = &view.seeking {
            if progress.loading {
                localized(
                    state,
                    "VERA:ReplayLoading",
                    "Preparing replay... Cancel is available.",
                )
            } else {
                localized(
                    state,
                    "VERA:ReplaySeeking",
                    "Seeking to {target}... reached {current}",
                )
                .replace("{target}", &timestamp(progress.target_tick, view.tick_ms))
                .replace("{current}", &timestamp(progress.reached_tick, view.tick_ms))
            }
        } else if view.validating {
            localized(
                state,
                "VERA:ReplayValidating",
                "Validating recording... Cancel is available.",
            )
        } else if view.browser.is_some() {
            localized(state, "VERA:ReplayBrowserTitle", "Replay Recordings")
        } else {
            localized(state, "VERA:ReplayHighlights", "Highlights")
        };
        text(
            state,
            &mut texts,
            &title_text,
            title,
            false,
            [1.0, 0.96, 0.76],
        );
    }
    if let Some(rect) = layout.page_label {
        text(
            state,
            &mut texts,
            &format!("{} / {}", layout.page + 1, layout.pages),
            rect,
            true,
            [0.9, 0.94, 0.96],
        );
    }
    for control in &layout.controls {
        let selected = tools.focused == Some(control.action)
            || tools.hovered == Some(control.action)
            || matches!(control.action, ReplayToolAction::SelectBookmark(index) if tools.selected_bookmark == Some(index))
            || matches!(control.action, ReplayToolAction::SelectRecording(index) if tools.selected_recording == Some(index));
        fills.push(fill(
            control.rect,
            if selected && control.enabled {
                [0.75, 0.8, 0.4]
            } else {
                [0.32, 0.38, 0.42]
            },
        ));
        fills.push(fill(
            GadgetRect::new(
                control.rect.x + 1,
                control.rect.y + 1,
                control.rect.w - 2,
                control.rect.h - 2,
            ),
            if tools.pressed(control.action) {
                [0.12, 0.17, 0.2]
            } else {
                [0.18, 0.24, 0.28]
            },
        ));
        text(
            state,
            &mut texts,
            &caption(state, &view, control.action),
            control.rect,
            !matches!(
                control.action,
                ReplayToolAction::EditLabel
                    | ReplayToolAction::SelectBookmark(_)
                    | ReplayToolAction::SelectRecording(_)
            ),
            if control.enabled {
                [1.0, 0.96, 0.76]
            } else {
                [0.5, 0.53, 0.55]
            },
        );
    }
    (fills, texts)
}
