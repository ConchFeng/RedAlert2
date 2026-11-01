//! Compact viewing controls for the offline spectator camera.

use super::spectator_standings_ui;
use crate::app::AppState;
use crate::app::input::camera::{spectator, tactical_viewport_size};
use crate::app::input::keyboard::InGameKeyEdge;
use crate::app::match_runtime::frame_pacer::wall_clock_ms;
use crate::render::batch::SpriteInstance;
use crate::sim::intern::InternedId;
use crate::ui::gadget::GadgetRect;
use winit::event::MouseButton;
use winit::keyboard::{Key, NamedKey};

const MARGIN: i32 = 12;
const TOP: i32 = 40;
const PAD: i32 = 8;
const GAP: i32 = 8;
const BUTTON_HEIGHT: i32 = 28;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpectatorToolAction {
    ToggleCamera,
    ResumeCamera,
    ToggleStandings,
    StopSeriesAfterMatch,
    SortStandings,
    PreviousStandings,
    NextStandings,
    VisitHouse(InternedId),
}

#[derive(Default)]
pub(crate) struct SpectatorTools {
    pub(crate) standings: spectator_standings_ui::StandingsView,
    pressed: Option<SpectatorToolAction>,
    captured: Option<MouseButton>,
    focused: Option<SpectatorToolAction>,
}

impl SpectatorTools {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn has_capture(&self) -> bool {
        self.captured.is_some()
    }

    fn pointer_edge(
        &mut self,
        hit: Option<SpectatorToolAction>,
        inside: bool,
        button: MouseButton,
        pressed: bool,
    ) -> (bool, Option<SpectatorToolAction>) {
        if let Some(captured) = self.captured {
            if pressed || button != captured {
                return (true, None);
            }
            self.captured = None;
            let action = self.pressed.take().filter(|control| Some(*control) == hit);
            return (true, action);
        }
        if pressed {
            if inside {
                self.captured = Some(button);
                self.pressed = (button == MouseButton::Left).then_some(hit).flatten();
                self.focused = self.pressed;
                return (true, None);
            }
            self.focused = None;
            return (false, None);
        }
        (inside, None)
    }
}

struct ToolbarLayout {
    panel: GadgetRect,
    toggle: GadgetRect,
    standings: GadgetRect,
    stop_series: Option<GadgetRect>,
    resume: Option<GadgetRect>,
    status: GadgetRect,
}

impl ToolbarLayout {
    fn new(
        view: (u32, u32),
        widths: [i32; 4],
        status_width: i32,
        text_height: i32,
        resume: bool,
        series: bool,
    ) -> Option<Self> {
        let available_width = (view.0 as i32 - 2 * MARGIN).min(720);
        let inside_width = available_width - 2 * PAD;
        if inside_width < 80 {
            return None;
        }
        let button_height = BUTTON_HEIGHT.max(text_height + 2 * PAD);
        let toggle_width = widths[0].max(120).min(inside_width);
        let resume_width = widths[1].max(104).min(inside_width);
        let mut toggle = GadgetRect::new(MARGIN + PAD, TOP + PAD, toggle_width, button_height);
        let mut next_x = toggle.x + toggle.w + GAP;
        let mut next_y = toggle.y;
        let standings_width = widths[2].max(104).min(inside_width);
        if next_x + standings_width > MARGIN + PAD + inside_width {
            next_x = MARGIN + PAD;
            next_y += button_height + GAP;
        }
        let standings = GadgetRect::new(next_x, next_y, standings_width, button_height);
        next_x += standings_width + GAP;
        let stop_series = series.then(|| {
            let width = widths[3].max(160).min(inside_width);
            if next_x + width > MARGIN + PAD + inside_width {
                next_x = MARGIN + PAD;
                next_y += button_height + GAP;
            }
            let rect = GadgetRect::new(next_x, next_y, width, button_height);
            next_x += width + GAP;
            rect
        });
        let resume_rect = resume.then(|| {
            if next_x + resume_width > MARGIN + PAD + inside_width {
                next_x = MARGIN + PAD;
                next_y += button_height + GAP;
            }
            let rect = GadgetRect::new(next_x, next_y, resume_width, button_height);
            next_x += resume_width + GAP;
            rect
        });
        let status_width = status_width.max(1);
        let status = if next_x + status_width <= MARGIN + PAD + inside_width {
            GadgetRect::new(next_x, next_y, status_width, button_height)
        } else {
            GadgetRect::new(
                MARGIN + PAD,
                next_y + button_height + GAP,
                inside_width,
                text_height + 2 * PAD,
            )
        };
        let panel_width = (status.x + status.w - MARGIN + PAD)
            .max(resume_rect.map_or(0, |rect| rect.x + rect.w - MARGIN + PAD))
            .max(toggle.x + toggle.w - MARGIN + PAD)
            .max(standings.x + standings.w - MARGIN + PAD)
            .max(stop_series.map_or(0, |rect| rect.x + rect.w - MARGIN + PAD))
            .min(available_width);
        let panel_height = status.y + status.h - TOP + PAD;
        if TOP + panel_height > view.1 as i32 - MARGIN {
            return None;
        }
        toggle.w = toggle.w.min(panel_width - 2 * PAD);
        Some(Self {
            panel: GadgetRect::new(MARGIN, TOP, panel_width, panel_height),
            toggle,
            standings,
            stop_series,
            resume: resume_rect,
            status,
        })
    }

    fn hit(&self, point: (i32, i32)) -> Option<SpectatorToolAction> {
        if self.toggle.contains(point.0, point.1) {
            Some(SpectatorToolAction::ToggleCamera)
        } else if self.standings.contains(point.0, point.1) {
            Some(SpectatorToolAction::ToggleStandings)
        } else if self
            .stop_series
            .is_some_and(|rect| rect.contains(point.0, point.1))
        {
            Some(SpectatorToolAction::StopSeriesAfterMatch)
        } else if self
            .resume
            .is_some_and(|rect| rect.contains(point.0, point.1))
        {
            Some(SpectatorToolAction::ResumeCamera)
        } else {
            None
        }
    }
}

pub(super) fn localized(state: &AppState, key: &str, fallback: &str) -> String {
    state
        .process_assets
        .csf
        .as_ref()
        .and_then(|csf| csf.get(key))
        .unwrap_or(fallback)
        .to_owned()
}

struct ToolbarContent {
    toggle: String,
    resume: String,
    standings: String,
    stop_series: String,
    status: String,
    layout: ToolbarLayout,
}

fn content(state: &AppState, now: u64) -> Option<ToolbarContent> {
    if !spectator::available(state) {
        return None;
    }
    let director = &state.match_state.input.spectator_camera;
    let toggle = if director.enabled() {
        localized(state, "VERA:SpectatorCameraOff", "Turn Off")
    } else {
        localized(state, "VERA:WatchTheAction", "Watch the Action")
    };
    let resume = localized(state, "VERA:SpectatorResumeNow", "Resume Now");
    let status = if !director.enabled() {
        localized(
            state,
            "VERA:SpectatorCameraManual",
            "Spectating - manual camera",
        )
    } else if director.manual_remaining(now) > 0 {
        localized(
            state,
            "VERA:SpectatorManualCountdown",
            "Manual control - resume in {seconds}s",
        )
        .replace("{seconds}", &director.manual_remaining(now).to_string())
    } else {
        let fallback = director.status(now);
        let key = match fallback.as_str() {
            "Auto Camera: On - Heavy fighting" => "VERA:SpectatorHeavyFighting",
            "Auto Camera: On - Base under attack" => "VERA:SpectatorBaseAttack",
            "Auto Camera: On - Superweapon launched" => "VERA:SpectatorSuperweapon",
            _ => "VERA:SpectatorWaitingForAction",
        };
        localized(state, key, &fallback)
    };
    let standings = localized(state, "VERA:SpectatorStandings", "Standings");
    let stop_series = if crate::app::ai_series::stop_requested(state) {
        localized(
            state,
            "VERA:SeriesStoppingAfterMatch",
            "Stopping After This Match",
        )
    } else {
        localized(state, "VERA:SeriesStopAfterMatch", "Stop After This Match")
    };
    let status = format!("{} | {status}", spectator_standings_ui::count_label(state));
    let series_status = crate::app::ai_series::toolbar_status(state);
    let status = if series_status.is_empty() {
        status
    } else {
        format!("{status} | {series_status}")
    };
    let font = &state.renderer.bit_font;
    let layout = ToolbarLayout::new(
        tactical_viewport_size(state),
        [
            font.text_width(&toggle) as i32 + 2 * PAD,
            font.text_width(&resume) as i32 + 2 * PAD,
            font.text_width(&standings) as i32 + 2 * PAD,
            font.text_width(&stop_series) as i32 + 2 * PAD,
        ],
        font.text_width(&status) as i32,
        font.cell_height() as i32,
        director.enabled() && director.manual_remaining(now) > 0,
        crate::app::ai_series::in_progress(state),
    )?;
    Some(ToolbarContent {
        toggle,
        resume,
        standings,
        stop_series,
        status,
        layout,
    })
}

pub(crate) fn action_center(state: &AppState, action: SpectatorToolAction) -> Option<(i32, i32)> {
    let layout = content(state, wall_clock_ms())?.layout;
    let rect = match action {
        SpectatorToolAction::ToggleCamera => layout.toggle,
        SpectatorToolAction::ResumeCamera => layout.resume?,
        SpectatorToolAction::ToggleStandings => layout.standings,
        SpectatorToolAction::StopSeriesAfterMatch => layout.stop_series?,
        _ => {
            return spectator_standings_ui::layout(state, layout.panel.y + layout.panel.h + GAP)?
                .center(action);
        }
    };
    Some((rect.x + rect.w / 2, rect.y + rect.h / 2))
}

pub(crate) fn panel_layout(state: &AppState) -> Option<spectator_standings_ui::StandingsLayout> {
    let toolbar = content(state, wall_clock_ms())?.layout;
    spectator_standings_ui::layout(state, toolbar.panel.y + toolbar.panel.h + GAP)
}

fn activate(state: &mut AppState, control: SpectatorToolAction, now: u64) {
    match control {
        SpectatorToolAction::ToggleCamera => {
            state.match_state.input.spectator_camera.toggle(now);
            if state.match_state.input.spectator_camera.enabled() {
                state.match_state.input.follow_target = None;
            }
        }
        SpectatorToolAction::ResumeCamera => {
            state.match_state.input.spectator_camera.resume(now);
            state.match_state.input.follow_target = None;
        }
        SpectatorToolAction::StopSeriesAfterMatch => {
            crate::app::ai_series::stop_after_current(state)
        }
        _ => spectator_standings_ui::activate(state, control),
    }
}

pub(crate) fn owns_pointer(state: &AppState) -> bool {
    let Some(content) = content(state, wall_clock_ms()) else {
        return false;
    };
    let panel = spectator_standings_ui::layout(
        state,
        content.layout.panel.y + content.layout.panel.h + GAP,
    );
    let pointer = state.cursor_position();
    state
        .match_state
        .match_presentation
        .spectator_tools
        .captured
        .is_some()
        || content
            .layout
            .panel
            .contains(pointer.0.floor() as i32, pointer.1.floor() as i32)
        || panel.is_some_and(|panel| {
            panel.contains((pointer.0.floor() as i32, pointer.1.floor() as i32))
        })
}

pub(crate) fn mouse(state: &mut AppState, button: MouseButton, pressed: bool) -> bool {
    let now = wall_clock_ms();
    let Some(content) = content(state, now) else {
        state.match_state.match_presentation.spectator_tools.reset();
        return false;
    };
    let pointer = state.cursor_position();
    let point = (pointer.0.floor() as i32, pointer.1.floor() as i32);
    let panel = spectator_standings_ui::layout(
        state,
        content.layout.panel.y + content.layout.panel.h + GAP,
    );
    let (consumed, action) = state
        .match_state
        .match_presentation
        .spectator_tools
        .pointer_edge(
            content
                .layout
                .hit(point)
                .or_else(|| panel.as_ref().and_then(|panel| panel.hit(point))),
            content.layout.panel.contains(point.0, point.1)
                || panel.as_ref().is_some_and(|panel| panel.contains(point)),
            button,
            pressed,
        );
    if consumed {
        state.match_state.input.cancel_tactical_drag();
    }
    if let Some(action) = action {
        activate(state, action, now);
    }
    consumed
}

pub(crate) fn key(state: &mut AppState, edge: &InGameKeyEdge<'_>) -> bool {
    let now = wall_clock_ms();
    let Some(content) = content(state, now) else {
        state.match_state.match_presentation.spectator_tools.reset();
        return false;
    };
    let modifiers = state.match_state.input.hotkey_modifiers;
    if modifiers.control_key() || modifiers.alt_key() || modifiers.super_key() {
        return false;
    }
    if matches!(edge.logical, Key::Named(NamedKey::Escape))
        && edge.state.is_pressed()
        && !edge.repeat
        && spectator_standings_ui::close(state)
    {
        state.match_state.match_presentation.spectator_tools.focused = None;
        return true;
    }
    let mut actions = vec![
        SpectatorToolAction::ToggleCamera,
        SpectatorToolAction::ToggleStandings,
    ];
    if content.layout.stop_series.is_some() {
        actions.push(SpectatorToolAction::StopSeriesAfterMatch);
    }
    if content.layout.resume.is_some() {
        actions.push(SpectatorToolAction::ResumeCamera);
    }
    if let Some(panel) =
        spectator_standings_ui::layout(state, content.layout.panel.y + content.layout.panel.h + GAP)
    {
        actions.extend(panel.controls());
    }
    let tools = &mut state.match_state.match_presentation.spectator_tools;
    let action = match edge.logical {
        Key::Named(NamedKey::Tab) => {
            if edge.state.is_pressed() && !edge.repeat {
                let current = tools
                    .focused
                    .and_then(|focus| actions.iter().position(|action| *action == focus));
                let index = if modifiers.shift_key() {
                    current.map_or(actions.len() - 1, |index| {
                        (index + actions.len() - 1) % actions.len()
                    })
                } else {
                    current.map_or(0, |index| (index + 1) % actions.len())
                };
                tools.focused = Some(actions[index]);
            }
            return true;
        }
        Key::Named(NamedKey::Escape) => {
            tools.focused = None;
            return false;
        }
        Key::Named(NamedKey::Enter | NamedKey::Space) if tools.focused.is_some() => {
            if edge.state.is_pressed() && !edge.repeat {
                tools.focused
            } else {
                None
            }
        }
        _ => {
            if edge.state.is_pressed() {
                tools.focused = None;
            }
            return false;
        }
    };
    if let Some(action) = action {
        activate(state, action, now);
    }
    true
}

pub(crate) fn build_instances(state: &AppState) -> (Vec<SpriteInstance>, Vec<SpriteInstance>) {
    let Some(content) = content(state, wall_clock_ms()) else {
        return (Vec::new(), Vec::new());
    };
    let camera = [
        state.match_state.input.camera_x,
        state.match_state.input.camera_y,
    ];
    let font = &state.renderer.bit_font;
    let pointer = state.cursor_position();
    let hovered = content
        .layout
        .hit((pointer.0.floor() as i32, pointer.1.floor() as i32));
    let tools = &state.match_state.match_presentation.spectator_tools;
    let fill = |rect: GadgetRect, tint: [f32; 3]| SpriteInstance {
        position: [rect.x as f32 + camera[0], rect.y as f32 + camera[1]],
        size: [rect.w as f32, rect.h as f32],
        uv_origin: [0.0, 0.0],
        uv_size: [1.0, 1.0],
        tint,
        alpha: 1.0,
        ..Default::default()
    };
    let mut fills = vec![fill(content.layout.panel, [0.035, 0.045, 0.055])];
    let mut texts = Vec::new();
    for (control, label, rect) in [
        (
            SpectatorToolAction::ToggleCamera,
            content.toggle.as_str(),
            Some(content.layout.toggle),
        ),
        (
            SpectatorToolAction::ToggleStandings,
            content.standings.as_str(),
            Some(content.layout.standings),
        ),
        (
            SpectatorToolAction::StopSeriesAfterMatch,
            content.stop_series.as_str(),
            content.layout.stop_series,
        ),
        (
            SpectatorToolAction::ResumeCamera,
            content.resume.as_str(),
            content.layout.resume,
        ),
    ] {
        let Some(rect) = rect else { continue };
        let selected = hovered == Some(control) || tools.focused == Some(control);
        let pressed = tools.pressed == Some(control) && hovered == Some(control);
        let border = if selected {
            [0.75, 0.8, 0.4]
        } else {
            [0.32, 0.38, 0.42]
        };
        let body = if pressed {
            [0.12, 0.17, 0.2]
        } else {
            [0.18, 0.24, 0.28]
        };
        fills.push(fill(rect, border));
        fills.push(fill(
            GadgetRect::new(rect.x + 1, rect.y + 1, rect.w - 2, rect.h - 2),
            body,
        ));
        let width = (font.text_width(label) as f32).max(1.0);
        let scale = ((rect.w - 2 * PAD) as f32 / width).min(1.0);
        texts.extend(font.build_text(
            label,
            rect.x as f32 + (rect.w as f32 - width * scale) / 2.0,
            rect.y as f32 + (rect.h as f32 - font.cell_height() * scale) / 2.0,
            scale,
            0.0,
            [1.0, 0.96, 0.76],
            camera,
        ));
    }
    let status = content.layout.status;
    let width = (font.text_width(&content.status) as f32).max(1.0);
    let scale = (status.w as f32 / width).min(1.0);
    texts.extend(font.build_text(
        &content.status,
        status.x as f32,
        status.y as f32 + (status.h as f32 - font.cell_height() * scale) / 2.0,
        scale,
        0.0,
        [0.9, 0.94, 0.96],
        camera,
    ));
    if let Some(panel) =
        spectator_standings_ui::layout(state, content.layout.panel.y + content.layout.panel.h + GAP)
    {
        let panel_hovered = panel.hit((pointer.0.floor() as i32, pointer.1.floor() as i32));
        let (panel_fills, panel_texts) = spectator_standings_ui::build_instances(
            state,
            &panel,
            panel_hovered,
            tools.focused,
            tools.pressed,
        );
        fills.extend(panel_fills);
        texts.extend(panel_texts);
    }
    (fills, texts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_toolbar_wraps_with_shared_hit_rectangles_inside_the_view() {
        let layout =
            ToolbarLayout::new((452, 480), [160, 120, 104, 168], 300, 12, true, false).unwrap();
        assert!(layout.status.y > layout.toggle.y);
        assert_eq!(
            layout.hit((layout.toggle.x, layout.toggle.y)),
            Some(SpectatorToolAction::ToggleCamera)
        );
        let resume = layout.resume.unwrap();
        assert_eq!(
            layout.hit((resume.x, resume.y)),
            Some(SpectatorToolAction::ResumeCamera)
        );
        for rect in [layout.toggle, resume, layout.status] {
            assert!(rect.x >= layout.panel.x && rect.y >= layout.panel.y);
            assert!(rect.x + rect.w <= layout.panel.x + layout.panel.w);
            assert!(rect.y + rect.h <= layout.panel.y + layout.panel.h);
        }
        assert!(layout.toggle.h >= 28 && resume.h >= 28);
        assert!(layout.panel.x + layout.panel.w <= 452);
        assert!(layout.panel.y + layout.panel.h < 480);
    }

    #[test]
    fn release_outside_cancels_the_action_and_still_consumes_the_capture() {
        let mut tools = SpectatorTools::default();
        assert_eq!(
            tools.pointer_edge(
                Some(SpectatorToolAction::ToggleCamera),
                true,
                MouseButton::Left,
                true
            ),
            (true, None)
        );
        assert_eq!(
            tools.pointer_edge(None, false, MouseButton::Left, false),
            (true, None)
        );
        assert_eq!(tools.captured, None);
        tools.pointer_edge(
            Some(SpectatorToolAction::ToggleCamera),
            true,
            MouseButton::Left,
            true,
        );
        assert_eq!(
            tools.pointer_edge(None, false, MouseButton::Right, true),
            (true, None)
        );
        assert_eq!(tools.captured, Some(MouseButton::Left));
        assert_eq!(
            tools.pointer_edge(None, false, MouseButton::Right, false),
            (true, None)
        );
        assert_eq!(
            tools.pointer_edge(None, false, MouseButton::Left, false),
            (true, None)
        );
        assert_eq!(
            tools.pointer_edge(
                Some(SpectatorToolAction::ToggleCamera),
                true,
                MouseButton::Left,
                false
            ),
            (true, None)
        );
        tools.pointer_edge(
            Some(SpectatorToolAction::ResumeCamera),
            true,
            MouseButton::Left,
            true,
        );
        assert_eq!(
            tools.pointer_edge(
                Some(SpectatorToolAction::ResumeCamera),
                true,
                MouseButton::Left,
                false
            ),
            (true, Some(SpectatorToolAction::ResumeCamera))
        );
        tools.pointer_edge(
            Some(SpectatorToolAction::ToggleCamera),
            true,
            MouseButton::Right,
            true,
        );
        assert_eq!(
            tools.pointer_edge(
                Some(SpectatorToolAction::ToggleCamera),
                true,
                MouseButton::Right,
                false
            ),
            (true, None)
        );
    }

    #[test]
    fn resume_layout_is_absent_outside_manual_cooldown_and_tiny_views_stay_safe() {
        let layout =
            ToolbarLayout::new((800, 480), [160, 120, 104, 168], 200, 12, false, false).unwrap();
        assert!(layout.resume.is_none());
        assert!(layout.status.y >= layout.toggle.y);
        let stacked =
            ToolbarLayout::new((220, 320), [160, 120, 104, 168], 200, 12, true, false).unwrap();
        assert!(stacked.resume.unwrap().y > stacked.toggle.y);
        assert!(stacked.panel.x + stacked.panel.w <= 220);
        assert!(
            ToolbarLayout::new((80, 480), [160, 120, 104, 168], 200, 12, true, false).is_none()
        );
        assert!(
            ToolbarLayout::new((452, 60), [160, 120, 104, 168], 200, 12, true, false).is_none()
        );
    }
}
