//! Presentation-only camera and pointer transactions for a floating spectator view.

use crate::sim::intern::InternedId;
use crate::ui::gadget::GadgetRect;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

const PAD: i32 = 8;
const GAP: i32 = 6;
const BUTTON_HEIGHT: i32 = 28;
const MIN_WIDTH: i32 = 280;
const MIN_HEIGHT: i32 = 208;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct CameraView {
    pub center: [f32; 2],
    pub zoom: f32,
}

impl Default for CameraView {
    fn default() -> Self {
        Self {
            center: [0.0; 2],
            zoom: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FollowMode {
    #[default]
    Fixed,
    Player(Option<InternedId>),
    Action,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SecondViewControl {
    Close,
    Minimize,
    SwapViews,
    Mode,
    PreviousPlayer,
    NextPlayer,
    ZoomOut,
    ZoomIn,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SecondViewEffect {
    #[default]
    None,
    SwapViews,
    CyclePlayer(i32),
    CameraChanged,
}

#[derive(Clone, Copy, Debug)]
enum CaptureKind {
    Button(SecondViewControl),
    Move,
    Resize,
    Inspect,
    Block,
}

#[derive(Clone, Copy, Debug)]
struct Capture {
    button: MouseButton,
    kind: CaptureKind,
    pointer: (i32, i32),
    panel: GadgetRect,
    moved: bool,
}

#[derive(Default)]
pub(crate) struct SecondViewState {
    pub camera: CameraView,
    pub mode: FollowMode,
    pub minimized: bool,
    open: bool,
    panel: Option<GadgetRect>,
    capture: Option<Capture>,
    canceled_button: Option<MouseButton>,
    hovered: Option<SecondViewControl>,
    focused: Option<SecondViewControl>,
    keyboard_focus: bool,
}

impl SecondViewState {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn supported(viewport: GadgetRect) -> bool {
        viewport.w >= MIN_WIDTH && viewport.h >= MIN_HEIGHT
    }

    pub(crate) fn open(&mut self, viewport: GadgetRect, camera: CameraView) -> bool {
        if !Self::supported(viewport) {
            return false;
        }
        self.camera = CameraView {
            center: camera
                .center
                .map(|value| if value.is_finite() { value } else { 0.0 }),
            zoom: if camera.zoom.is_finite() {
                camera.zoom.clamp(0.5, 3.0)
            } else {
                1.0
            },
        };
        self.open = true;
        self.minimized = false;
        self.mode = FollowMode::Fixed;
        self.capture = None;
        self.canceled_button = None;
        self.hovered = None;
        self.focused = Some(SecondViewControl::SwapViews);
        self.keyboard_focus = true;
        let width = 360.min(viewport.w);
        let height = 300.min(viewport.h);
        self.panel = Some(clamp_panel(
            self.panel.unwrap_or_else(|| {
                GadgetRect::new(
                    viewport.x + viewport.w - width,
                    viewport.y + 8,
                    width,
                    height,
                )
            }),
            viewport,
            false,
        ));
        true
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
        self.suspend();
    }

    pub(crate) fn suspend(&mut self) {
        if let Some(capture) = self.capture.take() {
            self.canceled_button = Some(capture.button);
        }
        self.hovered = None;
        self.focused = None;
        self.keyboard_focus = false;
    }

    pub(crate) fn consume_canceled_release(&mut self, button: MouseButton, pressed: bool) -> bool {
        if self.canceled_button != Some(button) {
            return false;
        }
        self.canceled_button = None;
        !pressed
    }

    pub(crate) fn layout(&self, viewport: GadgetRect) -> Option<SecondViewLayout> {
        if !self.open || viewport.w < MIN_WIDTH || viewport.h < MIN_HEIGHT {
            return None;
        }
        Some(SecondViewLayout::new(
            clamp_panel(self.panel?, viewport, self.minimized),
            self.minimized,
            self.mode,
        ))
    }

    pub(crate) fn has_keyboard_focus(&self) -> bool {
        self.keyboard_focus && self.open
    }

    pub(crate) fn has_capture(&self) -> bool {
        self.capture.is_some()
    }

    pub(crate) fn focused(&self, control: SecondViewControl) -> bool {
        self.keyboard_focus && self.focused == Some(control)
    }

    pub(crate) fn hovered(&self, control: SecondViewControl) -> bool {
        self.hovered == Some(control)
    }

    pub(crate) fn pressed(&self, control: SecondViewControl) -> bool {
        self.capture.is_some_and(
            |capture| matches!(capture.kind, CaptureKind::Button(button) if button == control),
        ) && self.hovered == Some(control)
    }

    pub(crate) fn swap_camera(&mut self, main: CameraView) -> CameraView {
        self.mode = FollowMode::Fixed;
        std::mem::replace(&mut self.camera, main)
    }

    pub(crate) fn activate(
        &mut self,
        control: SecondViewControl,
        viewport: GadgetRect,
    ) -> SecondViewEffect {
        use SecondViewControl::*;
        self.focused = Some(control);
        match control {
            Close => self.close(),
            Minimize => {
                self.minimized = !self.minimized;
                self.capture = None;
                if let Some(layout) = self.layout(viewport) {
                    self.panel = Some(clamp_panel(
                        self.panel.unwrap_or(layout.panel),
                        viewport,
                        self.minimized,
                    ));
                }
            }
            SwapViews => return SecondViewEffect::SwapViews,
            Mode => {
                self.mode = match self.mode {
                    FollowMode::Fixed => FollowMode::Player(None),
                    FollowMode::Player(_) => FollowMode::Action,
                    FollowMode::Action => FollowMode::Fixed,
                };
                if matches!(self.mode, FollowMode::Player(_)) {
                    return SecondViewEffect::CyclePlayer(1);
                }
                return SecondViewEffect::CameraChanged;
            }
            PreviousPlayer | NextPlayer => {
                if matches!(self.mode, FollowMode::Player(_)) {
                    return SecondViewEffect::CyclePlayer(if control == PreviousPlayer {
                        -1
                    } else {
                        1
                    });
                }
            }
            ZoomOut | ZoomIn => {
                self.camera.zoom =
                    (self.camera.zoom * if control == ZoomIn { 1.25 } else { 0.8 }).clamp(0.5, 3.0);
                return SecondViewEffect::CameraChanged;
            }
        }
        SecondViewEffect::None
    }

    pub(crate) fn mouse(
        &mut self,
        point: (i32, i32),
        button: MouseButton,
        pressed: bool,
        viewport: GadgetRect,
    ) -> (bool, SecondViewEffect) {
        let layout = self.layout(viewport);
        if let Some(capture) = self.capture {
            if pressed || button != capture.button {
                return (true, SecondViewEffect::None);
            }
            self.capture = None;
            let Some(layout) = layout else {
                return (true, SecondViewEffect::None);
            };
            let effect = match capture.kind {
                CaptureKind::Button(control) if layout.hit(point) == Some(control) => {
                    self.activate(control, viewport)
                }
                CaptureKind::Inspect if !capture.moved => {
                    if let Some(content) = layout
                        .content
                        .filter(|rect| rect.contains(point.0, point.1))
                    {
                        self.camera.center[0] +=
                            (point.0 as f32 - content.x as f32 - content.w as f32 / 2.0)
                                / self.camera.zoom;
                        self.camera.center[1] +=
                            (point.1 as f32 - content.y as f32 - content.h as f32 / 2.0)
                                / self.camera.zoom;
                        self.mode = FollowMode::Fixed;
                        SecondViewEffect::CameraChanged
                    } else {
                        SecondViewEffect::None
                    }
                }
                _ => SecondViewEffect::None,
            };
            return (true, effect);
        }
        let Some(layout) = layout else {
            return (false, SecondViewEffect::None);
        };
        let inside = layout.panel.contains(point.0, point.1);
        if !pressed {
            return (inside, SecondViewEffect::None);
        }
        if !inside {
            self.keyboard_focus = false;
            self.focused = None;
            return (false, SecondViewEffect::None);
        }
        self.keyboard_focus = true;
        let hit = layout.hit(point);
        self.hovered = hit;
        self.focused = hit.or(self.focused);
        let kind = if button != MouseButton::Left {
            CaptureKind::Block
        } else if let Some(control) = hit {
            CaptureKind::Button(control)
        } else if layout
            .resize
            .is_some_and(|rect| rect.contains(point.0, point.1))
        {
            CaptureKind::Resize
        } else if layout.title.contains(point.0, point.1) {
            CaptureKind::Move
        } else if layout
            .content
            .is_some_and(|rect| rect.contains(point.0, point.1))
        {
            CaptureKind::Inspect
        } else {
            CaptureKind::Block
        };
        let panel = GadgetRect {
            h: if self.minimized {
                self.panel
                    .map_or(MIN_HEIGHT, |panel| panel.h)
                    .min(viewport.h)
            } else {
                layout.panel.h
            },
            ..layout.panel
        };
        self.capture = Some(Capture {
            button,
            kind,
            pointer: point,
            panel,
            moved: false,
        });
        (true, SecondViewEffect::None)
    }

    pub(crate) fn pointer_moved(&mut self, point: (i32, i32), viewport: GadgetRect) -> bool {
        let Some(layout) = self.layout(viewport) else {
            return self.capture.is_some();
        };
        self.hovered = layout.hit(point);
        let Some(capture) = self.capture.as_mut() else {
            return layout.panel.contains(point.0, point.1);
        };
        let dx = point.0.saturating_sub(capture.pointer.0);
        let dy = point.1.saturating_sub(capture.pointer.1);
        capture.moved |= dx.unsigned_abs() > 4 || dy.unsigned_abs() > 4;
        let mut panel = capture.panel;
        match capture.kind {
            CaptureKind::Move => {
                panel.x = panel.x.saturating_add(dx);
                panel.y = panel.y.saturating_add(dy);
            }
            CaptureKind::Resize => {
                panel.w = panel.w.saturating_add(dx);
                panel.h = panel.h.saturating_add(dy);
            }
            _ => return true,
        }
        self.panel = Some(clamp_panel(panel, viewport, self.minimized));
        true
    }

    pub(crate) fn wheel(&mut self, point: (i32, i32), delta: f32, viewport: GadgetRect) -> bool {
        if !self
            .layout(viewport)
            .is_some_and(|layout| layout.panel.contains(point.0, point.1))
        {
            return self.has_capture();
        }
        if delta.is_finite() && delta != 0.0 && !self.minimized {
            self.activate(
                if delta > 0.0 {
                    SecondViewControl::ZoomIn
                } else {
                    SecondViewControl::ZoomOut
                },
                viewport,
            );
        }
        true
    }

    pub(crate) fn key(
        &mut self,
        code: KeyCode,
        shift: bool,
        viewport: GadgetRect,
    ) -> (bool, SecondViewEffect) {
        if !self.keyboard_focus {
            return (false, SecondViewEffect::None);
        }
        let Some(layout) = self.layout(viewport) else {
            return (false, SecondViewEffect::None);
        };
        match code {
            KeyCode::Escape => {
                self.close();
                (true, SecondViewEffect::None)
            }
            KeyCode::Tab => {
                let controls: Vec<_> = layout
                    .controls
                    .iter()
                    .map(|(control, _)| *control)
                    .collect();
                let current = self
                    .focused
                    .and_then(|focus| controls.iter().position(|control| *control == focus));
                let next = match current {
                    Some(index) if shift => (index + controls.len() - 1) % controls.len(),
                    Some(index) => (index + 1) % controls.len(),
                    None => 0,
                };
                self.focused = Some(controls[next]);
                (true, SecondViewEffect::None)
            }
            KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space => (
                true,
                self.focused.map_or(SecondViewEffect::None, |control| {
                    self.activate(control, viewport)
                }),
            ),
            _ => (false, SecondViewEffect::None),
        }
    }
}

fn clamp_panel(mut panel: GadgetRect, viewport: GadgetRect, minimized: bool) -> GadgetRect {
    panel.w = panel.w.clamp(MIN_WIDTH.min(viewport.w), viewport.w);
    panel.h = panel.h.clamp(MIN_HEIGHT.min(viewport.h), viewport.h);
    panel.x = panel.x.clamp(viewport.x, viewport.x + viewport.w - panel.w);
    let visible_height = if minimized { 36 } else { panel.h };
    panel.y = panel
        .y
        .clamp(viewport.y, viewport.y + viewport.h - visible_height);
    panel
}

pub(crate) struct SecondViewLayout {
    pub panel: GadgetRect,
    pub title: GadgetRect,
    pub content: Option<GadgetRect>,
    pub player_label: Option<GadgetRect>,
    pub status: Option<GadgetRect>,
    pub resize: Option<GadgetRect>,
    pub controls: Vec<(SecondViewControl, GadgetRect)>,
}

impl SecondViewLayout {
    fn new(mut panel: GadgetRect, minimized: bool, mode: FollowMode) -> Self {
        use SecondViewControl::*;
        let x = panel.x + PAD;
        let inner = panel.w - 2 * PAD;
        let title = GadgetRect::new(x, panel.y + 4, inner - 144, BUTTON_HEIGHT);
        let close = GadgetRect::new(panel.x + panel.w - PAD - 64, title.y, 64, BUTTON_HEIGHT);
        let minimize = GadgetRect::new(close.x - GAP - 68, title.y, 68, BUTTON_HEIGHT);
        let mut controls = vec![(Minimize, minimize), (Close, close)];
        if minimized {
            panel.h = 36;
            return Self {
                panel,
                title,
                content: None,
                player_label: None,
                status: None,
                resize: None,
                controls,
            };
        }
        let swap = GadgetRect::new(x, panel.y + 38, 116, BUTTON_HEIGHT);
        let mode_button = GadgetRect::new(
            swap.x + swap.w + GAP,
            swap.y,
            inner - swap.w - GAP,
            BUTTON_HEIGHT,
        );
        let zoom_in = GadgetRect::new(
            panel.x + panel.w - PAD - 32,
            swap.y + BUTTON_HEIGHT + GAP,
            32,
            BUTTON_HEIGHT,
        );
        let zoom_out = GadgetRect::new(zoom_in.x - GAP - 32, zoom_in.y, 32, BUTTON_HEIGHT);
        let mut player_label = GadgetRect::new(x, zoom_in.y, zoom_out.x - GAP - x, BUTTON_HEIGHT);
        if matches!(mode, FollowMode::Player(_)) {
            controls.push((
                PreviousPlayer,
                GadgetRect::new(x, zoom_in.y, 28, BUTTON_HEIGHT),
            ));
            controls.push((
                NextPlayer,
                GadgetRect::new(zoom_out.x - GAP - 28, zoom_in.y, 28, BUTTON_HEIGHT),
            ));
            player_label.x += 28 + GAP;
            player_label.w -= 2 * (28 + GAP);
        }
        controls.extend([
            (SwapViews, swap),
            (Mode, mode_button),
            (ZoomOut, zoom_out),
            (ZoomIn, zoom_in),
        ]);
        let content = GadgetRect::new(
            panel.x + 2,
            zoom_in.y + BUTTON_HEIGHT + GAP,
            panel.w - 4,
            panel.y + panel.h - 30 - (zoom_in.y + BUTTON_HEIGHT + GAP),
        );
        let resize = GadgetRect::new(panel.x + panel.w - 28, panel.y + panel.h - 28, 28, 28);
        let status = GadgetRect::new(x, content.y + content.h + 2, inner - 28, 24);
        Self {
            panel,
            title,
            content: Some(content),
            player_label: Some(player_label),
            status: Some(status),
            resize: Some(resize),
            controls,
        }
    }

    pub(crate) fn control_rect(&self, control: SecondViewControl) -> Option<GadgetRect> {
        self.controls
            .iter()
            .find_map(|(candidate, rect)| (*candidate == control).then_some(*rect))
    }

    pub(crate) fn hit(&self, point: (i32, i32)) -> Option<SecondViewControl> {
        self.controls
            .iter()
            .find_map(|(control, rect)| rect.contains(point.0, point.1).then_some(*control))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> GadgetRect {
        GadgetRect::new(0, 132, 632, 420)
    }
    fn center(rect: GadgetRect) -> (i32, i32) {
        (rect.x + rect.w / 2, rect.y + rect.h / 2)
    }

    #[test]
    fn pointer_capture_blocks_world_input_until_the_matching_release() {
        let viewport = viewport();
        let mut state = SecondViewState::default();
        assert!(!state.is_open());
        assert!(state.open(viewport, CameraView::default()));
        let point = center(state.layout(viewport).unwrap().content.unwrap());
        assert!(state.mouse(point, MouseButton::Right, true, viewport).0);
        assert!(state.pointer_moved((-20, -20), viewport));
        assert!(
            state
                .mouse((-20, -20), MouseButton::Left, false, viewport)
                .0
        );
        assert!(state.has_capture());
        assert_eq!(
            state.mouse((-20, -20), MouseButton::Right, false, viewport),
            (true, SecondViewEffect::None)
        );
        assert!(!state.has_capture());
        assert_eq!(state.camera, CameraView::default());
        assert!(state.mouse(point, MouseButton::Left, true, viewport).0);
        assert_eq!(
            state.mouse(point, MouseButton::Left, false, viewport).1,
            SecondViewEffect::CameraChanged
        );
    }

    #[test]
    fn drag_resize_and_swap_preserve_camera_units_and_viewport_bounds() {
        let viewport = viewport();
        let mut state = SecondViewState::default();
        state.open(
            viewport,
            CameraView {
                center: [1200.0, 450.0],
                zoom: 2.0,
            },
        );
        let title = center(state.layout(viewport).unwrap().title);
        state.mouse(title, MouseButton::Left, true, viewport);
        state.pointer_moved((-1000, -1000), viewport);
        state.mouse((-1000, -1000), MouseButton::Left, false, viewport);
        let rect = state.layout(viewport).unwrap().panel;
        assert_eq!((rect.x, rect.y), (viewport.x, viewport.y));
        let handle = center(state.layout(viewport).unwrap().resize.unwrap());
        state.mouse(handle, MouseButton::Left, true, viewport);
        state.pointer_moved((3000, 3000), viewport);
        state.mouse((3000, 3000), MouseButton::Left, false, viewport);
        assert_eq!(state.layout(viewport).unwrap().panel, viewport);
        let previous = state.swap_camera(CameraView {
            center: [200.0, 300.0],
            zoom: 1.0,
        });
        assert_eq!(
            previous,
            CameraView {
                center: [1200.0, 450.0],
                zoom: 2.0
            }
        );
        assert_eq!(state.camera.center, [200.0, 300.0]);
        assert_eq!(state.mode, FollowMode::Fixed);
    }

    #[test]
    fn minimized_view_has_no_content_and_keyboard_leaves_unfocused_world_input_alone() {
        let viewport = viewport();
        let mut state = SecondViewState::default();
        state.open(viewport, CameraView::default());
        assert_eq!(
            state.activate(SecondViewControl::Mode, viewport),
            SecondViewEffect::CyclePlayer(1)
        );
        state.activate(SecondViewControl::Minimize, viewport);
        let layout = state.layout(viewport).unwrap();
        assert!(layout.content.is_none());
        assert_eq!(layout.controls.len(), 2);
        assert!(state.key(KeyCode::Tab, false, viewport).0);
        state.mouse((-1, -1), MouseButton::Left, true, viewport);
        assert_eq!(
            state.key(KeyCode::Space, false, viewport),
            (false, SecondViewEffect::None)
        );
        assert!(state.is_open());
        state.activate(SecondViewControl::Minimize, viewport);
        let compact = GadgetRect::new(0, 0, MIN_WIDTH, MIN_HEIGHT);
        let layout = state.layout(compact).unwrap();
        for (control, rect) in &layout.controls {
            assert!(rect.h >= 28 && rect.w >= 28);
            assert!(
                rect.x >= 0
                    && rect.y >= 0
                    && rect.x + rect.w <= compact.w
                    && rect.y + rect.h <= compact.h
            );
            assert_eq!(layout.hit(center(*rect)), Some(*control));
        }
    }

    #[test]
    fn suspension_preserves_view_and_consumes_only_the_canceled_pointer_release() {
        let viewport = viewport();
        let mut state = SecondViewState::default();
        state.open(
            viewport,
            CameraView {
                center: [450.0, 800.0],
                zoom: 1.5,
            },
        );
        let content = center(state.layout(viewport).unwrap().content.unwrap());
        let before = state.camera;
        state.mouse(content, MouseButton::Left, true, viewport);
        state.suspend();
        assert!(state.is_open());
        assert_eq!(state.camera, before);
        assert!(!state.has_capture());
        assert!(!state.has_keyboard_focus());
        assert!(!state.consume_canceled_release(MouseButton::Right, false));
        assert!(state.consume_canceled_release(MouseButton::Left, false));
        assert!(!state.consume_canceled_release(MouseButton::Left, false));
        state.mouse(content, MouseButton::Right, true, viewport);
        state.suspend();
        assert!(!state.consume_canceled_release(MouseButton::Right, true));
        assert!(!state.consume_canceled_release(MouseButton::Right, false));
        state.close();
        assert!(!state.is_open());
        assert!(state.layout(viewport).is_none());
    }
}
