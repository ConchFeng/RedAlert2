//! Replay controls retain local focus and paging without owning playback state.

use crate::ui::gadget::GadgetRect;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReplayToolAction {
    TogglePause,
    CycleSpeed,
    ToggleHighlights,
    ToggleFollow,
    OpenBrowser,
    ClosePanel,
    AddBookmark,
    EditLabel,
    PreviousPage,
    NextPage,
    SelectBookmark(usize),
    SeekSelected,
    CancelSeek,
    SelectRecording(usize),
    OpenSelectedRecording,
}

#[derive(Default)]
pub(crate) struct ReplayToolsState {
    pub highlights: bool,
    pub page: usize,
    pub selected_bookmark: Option<usize>,
    pub selected_recording: Option<usize>,
    pub label: String,
    pub focused: Option<ReplayToolAction>,
    pub hovered: Option<ReplayToolAction>,
    pressed: Option<ReplayToolAction>,
    captured: Option<MouseButton>,
}

impl ReplayToolsState {
    pub(crate) fn has_capture(&self) -> bool {
        self.captured.is_some()
    }

    pub(crate) fn suspend(&mut self) {
        self.pressed = None;
        self.captured = None;
        self.hovered = None;
        self.focused = None;
    }

    pub(crate) fn pressed(&self, action: ReplayToolAction) -> bool {
        self.pressed == Some(action) && self.hovered == Some(action)
    }

    pub(crate) fn mouse(
        &mut self,
        layout: &ReplayToolsLayout,
        point: (i32, i32),
        button: MouseButton,
        pressed: bool,
    ) -> (bool, Option<ReplayToolAction>) {
        let hit = layout.hit(point);
        self.hovered = hit;
        if let Some(captured) = self.captured {
            if pressed || captured != button {
                return (true, None);
            }
            self.captured = None;
            return (
                true,
                self.pressed.take().filter(|action| Some(*action) == hit),
            );
        }
        let inside = layout.contains(point);
        if pressed {
            self.focused = hit;
            if inside {
                self.captured = Some(button);
                self.pressed = (button == MouseButton::Left).then_some(hit).flatten();
            }
        }
        (inside, None)
    }

    pub(crate) fn insert_text(&mut self, text: &str) {
        if self.focused == Some(ReplayToolAction::EditLabel) {
            self.label.extend(
                text.chars()
                    .filter(|character| !character.is_control())
                    .take(48usize.saturating_sub(self.label.chars().count())),
            );
        }
    }

    pub(crate) fn key(
        &mut self,
        layout: &ReplayToolsLayout,
        code: KeyCode,
        text: Option<&str>,
        shift: bool,
        pressed: bool,
        repeat: bool,
    ) -> Option<ReplayToolAction> {
        if !pressed {
            return None;
        }
        if code == KeyCode::Tab && !repeat {
            let controls: Vec<_> = layout
                .controls
                .iter()
                .filter(|control| control.enabled)
                .map(|control| control.action)
                .collect();
            if controls.is_empty() {
                return None;
            }
            let current = self
                .focused
                .and_then(|focus| controls.iter().position(|control| *control == focus));
            let next = match current {
                Some(index) if shift => (index + controls.len() - 1) % controls.len(),
                Some(index) => (index + 1) % controls.len(),
                None if shift => controls.len() - 1,
                None => 0,
            };
            self.focused = Some(controls[next]);
            return None;
        }
        if code == KeyCode::Escape && !repeat {
            return Some(ReplayToolAction::ClosePanel);
        }
        if self.focused == Some(ReplayToolAction::EditLabel) {
            if code == KeyCode::Backspace {
                self.label.pop();
            } else if matches!(code, KeyCode::Enter | KeyCode::NumpadEnter) && !repeat {
                return Some(ReplayToolAction::AddBookmark);
            } else if let Some(text) = text {
                self.insert_text(text);
            }
            return None;
        }
        if !repeat && matches!(code, KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space) {
            return self
                .focused
                .or((code == KeyCode::Space).then_some(ReplayToolAction::TogglePause))
                .filter(|action| {
                    layout
                        .controls
                        .iter()
                        .any(|control| control.action == *action && control.enabled)
                });
        }
        None
    }
}

pub(crate) struct ReplayControl {
    pub action: ReplayToolAction,
    pub rect: GadgetRect,
    pub enabled: bool,
}

pub(crate) struct ReplayToolsLayout {
    pub toolbar: GadgetRect,
    pub position: GadgetRect,
    pub status: GadgetRect,
    pub panel: Option<GadgetRect>,
    pub title: Option<GadgetRect>,
    pub page_label: Option<GadgetRect>,
    pub page: usize,
    pub pages: usize,
    pub controls: Vec<ReplayControl>,
    pub seeking: bool,
    pub compact: bool,
    viewport: GadgetRect,
}

impl ReplayToolsLayout {
    pub(crate) fn new(
        viewport: (u32, u32),
        expanded: bool,
        browser: bool,
        seeking: bool,
        ready: bool,
        page: usize,
        rows: usize,
        selected: bool,
        selectable: bool,
    ) -> Option<Self> {
        use ReplayToolAction::*;
        let viewport = GadgetRect::new(0, 0, viewport.0 as i32, viewport.1 as i32);
        let width = (viewport.w - 24).min(720);
        if width < 240 || viewport.h < 180 {
            return None;
        }
        let mut controls = Vec::new();
        let left = 20;
        let right = 12 + width - 8;
        let mut x = left;
        let mut y = 48;
        for (action, desired, enabled) in [
            (TogglePause, 88, ready),
            (CycleSpeed, 64, ready),
            (ToggleHighlights, 100, ready),
            (ToggleFollow, 108, ready && selectable),
            (OpenBrowser, 104, !seeking),
        ] {
            let w = desired.min(width - 16);
            if x + w > right {
                x = left;
                y += 34;
            }
            controls.push(ReplayControl {
                action,
                rect: GadgetRect::new(x, y, w, 28),
                enabled: enabled && !seeking && (!browser || action == OpenBrowser),
            });
            x += w + 6;
        }
        let position = GadgetRect::new(left, y + 32, width - 16, 22);
        let status = GadgetRect::new(left, y + 56, width - 16, 24);
        let toolbar = GadgetRect::new(12, 40, width, status.y + status.h + 8 - 40);
        let mut result = Self {
            toolbar,
            position,
            status,
            panel: None,
            title: None,
            page_label: None,
            page: 0,
            pages: 1,
            controls,
            seeking,
            compact: false,
            viewport,
        };
        let required_panel_height = if seeking {
            82
        } else if browser {
            140
        } else if expanded {
            174
        } else {
            0
        };
        if toolbar.y + toolbar.h + 8 + required_panel_height > viewport.h - 12 {
            result.compact = true;
            result.toolbar = GadgetRect::new(12, 40, width, 116);
            result.position = GadgetRect::new(left, 48, width - 16, 22);
            result.status = GadgetRect::new(left, 76, width - 16, 36);
            result.controls = vec![ReplayControl {
                action: if seeking { CancelSeek } else { ClosePanel },
                rect: GadgetRect::new(left, 120, 120, 28),
                enabled: true,
            }];
            return Some(result);
        }
        if seeking {
            let panel = GadgetRect::new(12, toolbar.y + toolbar.h + 8, width, 82);
            result.title = Some(GadgetRect::new(left, panel.y + 6, width - 16, 30));
            result.controls.push(ReplayControl {
                action: CancelSeek,
                rect: GadgetRect::new(left, panel.y + 42, 120, 28),
                enabled: true,
            });
            result.panel = Some(panel);
            return Some(result);
        }
        if !expanded && !browser {
            return Some(result);
        }
        let top = toolbar.y + toolbar.h + 8;
        let overhead = if browser { 112 } else { 148 };
        let page_size = ((viewport.h - 12 - top - overhead) / 34).clamp(1, 7) as usize;
        let pages = rows.max(1).div_ceil(page_size);
        result.page = page.min(pages - 1);
        result.pages = pages;
        result.title = Some(GadgetRect::new(left, top + 6, width - 16, 24));
        let mut row_y = top + 36;
        if !browser {
            result.controls.push(ReplayControl {
                action: EditLabel,
                rect: GadgetRect::new(left, row_y, width - 134, 28),
                enabled: ready,
            });
            result.controls.push(ReplayControl {
                action: AddBookmark,
                rect: GadgetRect::new(right - 104, row_y, 104, 28),
                enabled: ready,
            });
            row_y += 34;
        }
        for row in result.page * page_size..((result.page + 1) * page_size).min(rows) {
            result.controls.push(ReplayControl {
                action: if browser {
                    SelectRecording(row)
                } else {
                    SelectBookmark(row)
                },
                rect: GadgetRect::new(left, row_y, width - 16, 28),
                enabled: ready || browser,
            });
            row_y += 34;
        }
        if rows == 0 {
            row_y += 34;
        }
        let previous = GadgetRect::new(left, row_y, 88, 28);
        let next = GadgetRect::new(right - 72, row_y, 72, 28);
        result.page_label = Some(GadgetRect::new(
            previous.x + previous.w + 6,
            row_y,
            next.x - previous.x - previous.w - 12,
            28,
        ));
        result.controls.extend([
            ReplayControl {
                action: PreviousPage,
                rect: previous,
                enabled: result.page > 0,
            },
            ReplayControl {
                action: NextPage,
                rect: next,
                enabled: result.page + 1 < pages,
            },
            ReplayControl {
                action: if browser {
                    OpenSelectedRecording
                } else {
                    SeekSelected
                },
                rect: GadgetRect::new(left, row_y + 34, 120, 28),
                enabled: selected && (ready || browser),
            },
            ReplayControl {
                action: ClosePanel,
                rect: GadgetRect::new(right - 72, row_y + 34, 72, 28),
                enabled: true,
            },
        ]);
        result.panel = Some(GadgetRect::new(12, top, width, row_y + 70 - top));
        Some(result)
    }

    pub(crate) fn hit(&self, point: (i32, i32)) -> Option<ReplayToolAction> {
        self.controls.iter().find_map(|control| {
            (control.enabled && control.rect.contains(point.0, point.1)).then_some(control.action)
        })
    }

    pub(crate) fn contains(&self, point: (i32, i32)) -> bool {
        (self.seeking && self.viewport.contains(point.0, point.1))
            || self.toolbar.contains(point.0, point.1)
            || self
                .panel
                .is_some_and(|panel| panel.contains(point.0, point.1))
    }

    pub(crate) fn action_center(&self, action: ReplayToolAction) -> Option<(i32, i32)> {
        self.controls
            .iter()
            .find(|control| control.action == action)
            .map(|control| {
                (
                    control.rect.x + control.rect.w / 2,
                    control.rect.y + control.rect.h / 2,
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_highlights_page_keeps_controls_inside_the_tactical_view() {
        let layout =
            ReplayToolsLayout::new((452, 480), true, false, false, true, 99, 29, true, true)
                .unwrap();
        assert_eq!(layout.page, layout.pages - 1);
        for control in &layout.controls {
            assert!(control.rect.h >= 28);
            assert!(control.rect.x >= 0 && control.rect.y >= 0);
            assert!(control.rect.x + control.rect.w <= 452);
            assert!(control.rect.y + control.rect.h <= 480);
            if control.enabled {
                assert_eq!(
                    layout.hit(layout.action_center(control.action).unwrap()),
                    Some(control.action)
                );
            }
        }
    }

    #[test]
    fn short_replay_viewports_keep_the_exit_or_cancel_control_reachable() {
        for seeking in [false, true] {
            let layout =
                ReplayToolsLayout::new((320, 240), true, false, seeking, true, 0, 2, true, true)
                    .unwrap();
            for control in &layout.controls {
                assert!(
                    control.rect.y + control.rect.h <= 240,
                    "{:?} extends below the viewport",
                    control.action
                );
            }
            assert!(layout.controls.iter().any(|control| control.enabled
                && matches!(
                    control.action,
                    ReplayToolAction::ClosePanel | ReplayToolAction::CancelSeek
                )));
        }
    }

    #[test]
    fn pointer_capture_cancels_outside_and_bookmark_labels_keep_unicode_without_commands() {
        let layout =
            ReplayToolsLayout::new((632, 600), true, false, false, true, 0, 1, true, true).unwrap();
        let mut state = ReplayToolsState::default();
        let point = layout.action_center(ReplayToolAction::AddBookmark).unwrap();
        assert_eq!(
            state.mouse(&layout, point, MouseButton::Left, true),
            (true, None)
        );
        assert_eq!(
            state.mouse(&layout, (-1, -1), MouseButton::Left, false),
            (true, None)
        );
        state.focused = Some(ReplayToolAction::EditLabel);
        state.insert_text("North attack\n山");
        assert_eq!(state.label, "North attack山");
        assert_eq!(
            state.key(&layout, KeyCode::Space, Some(" "), false, true, false),
            None
        );
        assert_eq!(state.label, "North attack山 ");
        assert_eq!(
            state.key(&layout, KeyCode::Enter, None, false, true, false),
            Some(ReplayToolAction::AddBookmark)
        );
    }

    #[test]
    fn seeking_captures_world_clicks_and_exposes_only_cancel() {
        let layout =
            ReplayToolsLayout::new((632, 480), true, false, true, true, 0, 3, true, true).unwrap();
        let enabled: Vec<_> = layout
            .controls
            .iter()
            .filter(|control| control.enabled)
            .map(|control| control.action)
            .collect();
        assert_eq!(enabled, [ReplayToolAction::CancelSeek]);
        assert!(layout.contains((620, 470)));
        let mut state = ReplayToolsState::default();
        state.key(&layout, KeyCode::Tab, None, false, true, false);
        assert_eq!(state.focused, Some(ReplayToolAction::CancelSeek));
    }
}
