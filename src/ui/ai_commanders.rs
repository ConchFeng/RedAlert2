//! Draft commander choices for the offline Skirmish setup.

use crate::ui::shell::button::ShellButtonInteraction;
use crate::ui::shell::geom::RectPx;
use winit::keyboard::KeyCode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiCommanderChoice {
    pub name: String,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiCommanderRow {
    pub slot_index: usize,
    pub label: String,
    pub country: String,
    pub difficulty: String,
    pub choices: Vec<AiCommanderChoice>,
    pub selected: String,
}

impl AiCommanderRow {
    pub(crate) fn selected_choice(&self) -> Option<&AiCommanderChoice> {
        self.choices
            .iter()
            .find(|choice| choice.name.eq_ignore_ascii_case(&self.selected))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AiCommanderSetupEffect {
    None,
    Apply {
        choices: Vec<(usize, String)>,
        feedback_enabled: bool,
    },
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiCommanderControl {
    Row(usize),
    Choice(usize),
    Previous,
    Next,
    AllStandard,
    AllRandom,
    Feedback,
    Apply,
    Cancel,
    Back,
}

#[derive(Debug, Clone)]
pub(crate) struct AiCommanderSetupState {
    pub rows: Vec<AiCommanderRow>,
    pub page_top: usize,
    pub feedback_enabled: bool,
    pub notice: Option<String>,
    pub focused_control: AiCommanderControl,
    pub buttons: ShellButtonInteraction<AiCommanderControl>,
    pub choosing_row: Option<usize>,
    pub choice_top: usize,
}

impl AiCommanderSetupState {
    pub(crate) fn new(
        rows: Vec<AiCommanderRow>,
        feedback_enabled: bool,
        notice: Option<String>,
    ) -> Self {
        Self {
            rows,
            page_top: 0,
            feedback_enabled,
            notice,
            focused_control: AiCommanderControl::Apply,
            buttons: ShellButtonInteraction::default(),
            choosing_row: None,
            choice_top: 0,
        }
    }

    pub(crate) fn choosing(&self) -> Option<&AiCommanderRow> {
        self.choosing_row.and_then(|index| self.rows.get(index))
    }

    pub(crate) fn list_len(&self) -> usize {
        self.choosing()
            .map_or(self.rows.len(), |row| row.choices.len())
    }

    pub(crate) fn list_top(&self) -> usize {
        if self.choosing_row.is_some() {
            self.choice_top
        } else {
            self.page_top
        }
    }

    pub(crate) fn align_page(&mut self, visible_rows: usize) {
        let count = visible_rows.max(1);
        let top = self.list_top().min(self.list_len().saturating_sub(1)) / count * count;
        if self.choosing_row.is_some() {
            self.choice_top = top;
        } else {
            self.page_top = top;
        }
    }

    pub(crate) fn has_custom_choices(&self) -> bool {
        self.rows.iter().any(|row| {
            row.choices
                .iter()
                .any(|choice| !choice.name.eq_ignore_ascii_case("Standard"))
        })
    }

    pub(crate) fn enabled(&self, control: AiCommanderControl, visible_rows: usize) -> bool {
        use AiCommanderControl::*;
        match control {
            Previous => self.list_top() > 0,
            Next => self.list_top() + visible_rows.max(1) < self.list_len(),
            AllRandom => self.has_custom_choices(),
            Row(index) => index < visible_rows && self.page_top + index < self.rows.len(),
            Choice(index) => {
                index < visible_rows
                    && self
                        .choosing()
                        .is_some_and(|row| self.choice_top + index < row.choices.len())
            }
            _ => true,
        }
    }

    pub(crate) fn controls(&self, visible_rows: usize) -> Vec<AiCommanderControl> {
        use AiCommanderControl::*;
        let mut controls: Vec<_> = (0..visible_rows)
            .map(|index| {
                if self.choosing_row.is_some() {
                    Choice(index)
                } else {
                    Row(index)
                }
            })
            .chain([Previous, Next])
            .collect();
        if self.choosing_row.is_some() {
            controls.push(Back);
        } else {
            controls.extend([AllStandard, AllRandom, Feedback, Apply, Cancel]);
        }
        controls.retain(|control| self.enabled(*control, visible_rows));
        controls
    }

    pub(crate) fn activate(
        &mut self,
        control: AiCommanderControl,
        visible_rows: usize,
    ) -> AiCommanderSetupEffect {
        use AiCommanderControl::*;
        self.align_page(visible_rows);
        if !self.enabled(control, visible_rows) {
            return AiCommanderSetupEffect::None;
        }
        self.focused_control = control;
        match control {
            Row(index) => {
                self.choosing_row = Some(self.page_top + index);
                self.choice_top = 0;
                self.focused_control = Choice(0);
            }
            Choice(index) => {
                if let Some(row) = self.choosing_row.and_then(|index| self.rows.get_mut(index)) {
                    if let Some(choice) = row.choices.get(self.choice_top + index) {
                        row.selected = choice.name.clone();
                    }
                }
                self.close_choices();
            }
            Previous | Next => {
                let top = if control == Previous {
                    self.list_top().saturating_sub(visible_rows.max(1))
                } else {
                    self.list_top() + visible_rows.max(1)
                };
                if self.choosing_row.is_some() {
                    self.choice_top = top;
                } else {
                    self.page_top = top;
                }
            }
            AllStandard | AllRandom => {
                let requested = if control == AllStandard {
                    "Standard"
                } else {
                    "Random"
                };
                for row in &mut self.rows {
                    row.selected = row
                        .choices
                        .iter()
                        .find(|choice| choice.name.eq_ignore_ascii_case(requested))
                        .map_or_else(|| "Standard".to_owned(), |choice| choice.name.clone());
                }
            }
            Feedback => self.feedback_enabled = !self.feedback_enabled,
            Back => self.close_choices(),
            Apply => {
                return AiCommanderSetupEffect::Apply {
                    choices: self
                        .rows
                        .iter()
                        .map(|row| (row.slot_index, row.selected.clone()))
                        .collect(),
                    feedback_enabled: self.feedback_enabled,
                };
            }
            Cancel => return AiCommanderSetupEffect::Cancel,
        }
        AiCommanderSetupEffect::None
    }

    fn close_choices(&mut self) {
        let row = self.choosing_row.take().unwrap_or(self.page_top);
        self.focused_control = AiCommanderControl::Row(row.saturating_sub(self.page_top));
        self.buttons = ShellButtonInteraction::default();
    }

    pub(crate) fn key(
        &mut self,
        code: Option<KeyCode>,
        shift: bool,
        visible_rows: usize,
    ) -> AiCommanderSetupEffect {
        self.align_page(visible_rows);
        if code == Some(KeyCode::Escape) {
            if self.choosing_row.is_some() {
                self.close_choices();
                return AiCommanderSetupEffect::None;
            }
            return AiCommanderSetupEffect::Cancel;
        }
        if code == Some(KeyCode::Tab) {
            let controls = self.controls(visible_rows);
            let current = controls
                .iter()
                .position(|control| *control == self.focused_control);
            let next = match current {
                Some(index) if shift => (index + controls.len() - 1) % controls.len(),
                Some(index) => (index + 1) % controls.len(),
                None => 0,
            };
            self.focused_control = controls[next];
        }
        if matches!(
            code,
            Some(KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space)
        ) {
            return self.activate(self.focused_control, visible_rows);
        }
        AiCommanderSetupEffect::None
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AiCommanderSetupLayout {
    pub panel: RectPx,
    pub title: RectPx,
    pub summary: RectPx,
    pub rows: Vec<RectPx>,
    pub previous: RectPx,
    pub page: RectPx,
    pub next: RectPx,
    pub all_standard: RectPx,
    pub all_random: RectPx,
    pub feedback: RectPx,
    pub status: RectPx,
    pub apply: RectPx,
    pub cancel: RectPx,
    pub back: RectPx,
}

impl AiCommanderSetupLayout {
    pub(crate) fn new(width: u32, height: u32) -> Self {
        let width = width.min(i32::MAX as u32) as i32;
        let height = height.min(i32::MAX as u32) as i32;
        let w = (width - 24).clamp(1, 760);
        let h = (height - 24).clamp(1, 816);
        let panel = RectPx::new((width - w) / 2, (height - h) / 2, w, h);
        let x = panel.x + 16;
        let inner = (panel.w - 32).max(1);
        let title = RectPx::new(x, panel.y + 10, inner, 28);
        let summary = RectPx::new(x, title.y + 30, inner, 40);
        let bottom = panel.y + panel.h - 16;
        let apply = RectPx::new(x, bottom - 32, (inner - 12) / 2, 32);
        let cancel = RectPx::new(apply.x + apply.w + 12, apply.y, apply.w, 32);
        let status = RectPx::new(x, apply.y - 44, inner, 40);
        let feedback = RectPx::new(x, status.y - 36, inner, 30);
        let all_standard = RectPx::new(x, feedback.y - 38, apply.w, 32);
        let all_random = RectPx::new(cancel.x, all_standard.y, cancel.w, 32);
        let previous = RectPx::new(x, all_standard.y - 38, 108, 30);
        let next = RectPx::new(x + inner - 108, previous.y, 108, 30);
        let page = RectPx::new(previous.x + 116, previous.y, (inner - 232).max(1), 30);
        let row_top = summary.y + summary.h + 4;
        let count = ((previous.y - row_top) / 84).clamp(1, 7) as usize;
        let rows = (0..count)
            .map(|index| RectPx::new(x, row_top + index as i32 * 84, inner, 80))
            .collect();
        Self {
            panel,
            title,
            summary,
            rows,
            previous,
            page,
            next,
            all_standard,
            all_random,
            feedback,
            status,
            apply,
            cancel,
            back: RectPx::new(x, apply.y, inner, 32),
        }
    }

    pub(crate) fn control_rect(&self, control: AiCommanderControl) -> Option<RectPx> {
        use AiCommanderControl::*;
        Some(match control {
            Row(index) | Choice(index) => {
                let row = *self.rows.get(index)?;
                RectPx::new(row.x, row.y, row.w, 42)
            }
            Previous => self.previous,
            Next => self.next,
            AllStandard => self.all_standard,
            AllRandom => self.all_random,
            Feedback => self.feedback,
            Apply => self.apply,
            Cancel => self.cancel,
            Back => self.back,
        })
    }

    pub(crate) fn description_rect(&self, row: usize) -> Option<RectPx> {
        let row = *self.rows.get(row)?;
        Some(RectPx::new(row.x + 8, row.y + 44, row.w - 16, row.h - 44))
    }

    pub(crate) fn control_at(
        &self,
        state: &AiCommanderSetupState,
        point: (i32, i32),
    ) -> Option<AiCommanderControl> {
        state.controls(self.rows.len()).into_iter().find(|control| {
            self.control_rect(*control)
                .is_some_and(|rect| rect.contains(point.0, point.1))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> AiCommanderSetupState {
        AiCommanderSetupState::new(
            (0..29)
                .map(|slot_index| AiCommanderRow {
                    slot_index,
                    label: format!("Computer {}", slot_index + 1),
                    country: "America".into(),
                    difficulty: "Hard".into(),
                    selected: "Standard".into(),
                    choices: ["Standard", "Random", "Warren"]
                        .into_iter()
                        .map(|name| AiCommanderChoice {
                            name: name.into(),
                            label: name.into(),
                            description: "Unchanged trigger weights.".into(),
                        })
                        .collect(),
                })
                .collect(),
            true,
            None,
        )
    }

    #[test]
    fn paging_and_chooser_apply_to_stable_slots_without_touching_feedback() {
        let mut random_country = setup();
        for row in &mut random_country.rows {
            row.choices
                .retain(|choice| choice.name == "Standard" || choice.name == "Random");
        }
        assert!(random_country.has_custom_choices());
        assert!(random_country.enabled(AiCommanderControl::AllRandom, 3));
        random_country.activate(AiCommanderControl::AllRandom, 3);
        assert!(
            random_country
                .rows
                .iter()
                .all(|row| row.selected == "Random")
        );

        let mut setup = setup();
        setup.activate(AiCommanderControl::AllRandom, 4);
        setup.activate(AiCommanderControl::Next, 4);
        setup.activate(AiCommanderControl::Row(3), 4);
        setup.activate(AiCommanderControl::Choice(2), 4);
        let AiCommanderSetupEffect::Apply {
            choices,
            feedback_enabled,
        } = setup.activate(AiCommanderControl::Apply, 4)
        else {
            panic!("apply result")
        };
        assert_eq!(choices.len(), 29);
        assert_eq!(choices[7], (7, "Warren".into()));
        assert_eq!(choices[28], (28, "Random".into()));
        assert!(feedback_enabled);
    }

    #[test]
    fn escape_closes_choice_list_before_canceling_draft_and_feedback_is_independent() {
        let mut setup = setup();
        setup.activate(AiCommanderControl::Feedback, 4);
        setup.activate(AiCommanderControl::Row(0), 4);
        assert_eq!(
            setup.key(Some(KeyCode::Escape), false, 4),
            AiCommanderSetupEffect::None
        );
        assert!(setup.choosing_row.is_none());
        assert!(!setup.feedback_enabled);
        assert!(setup.rows.iter().all(|row| row.selected == "Standard"));
        assert_eq!(
            setup.key(Some(KeyCode::Escape), false, 4),
            AiCommanderSetupEffect::Cancel
        );
    }

    #[test]
    fn compact_geometry_uses_the_same_controls_for_paint_and_hits() {
        let layout = AiCommanderSetupLayout::new(640, 480);
        let setup = setup();
        let controls = setup.controls(layout.rows.len());
        for (index, control) in controls.iter().enumerate() {
            let rect = layout.control_rect(*control).unwrap();
            assert!(rect.h >= 28);
            assert!(rect.x >= 0 && rect.y >= 0 && rect.x + rect.w <= 640 && rect.y + rect.h <= 480);
            assert_eq!(
                layout.control_at(&setup, (rect.x + rect.w / 2, rect.y + rect.h / 2)),
                Some(*control)
            );
            for other in controls.iter().skip(index + 1) {
                let other = layout.control_rect(*other).unwrap();
                assert!(
                    !(rect.x < other.x + other.w
                        && other.x < rect.x + rect.w
                        && rect.y < other.y + other.h
                        && other.y < rect.y + rect.h)
                );
            }
        }
    }
}
