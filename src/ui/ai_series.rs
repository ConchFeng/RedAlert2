//! Draft-only setup for a finite offline AI series.

use crate::ui::shell::button::ShellButtonInteraction;
use crate::ui::shell::geom::RectPx;
use crate::ui::skirmish_shell::SavedSeedDescriptionEdit;
use winit::keyboard::KeyCode;

const NAME_LIMIT: usize = 48;
const MAP_ROWS: usize = 7;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiSeriesMapChoice {
    pub record_index: usize,
    pub label: String,
    pub capacity: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiSeriesSetup {
    pub name: String,
    pub match_count: u8,
    pub map_indices: Vec<usize>,
    pub randomize_each_round: bool,
    pub first_seed: u32,
    pub custom_seeds: Option<Vec<u32>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AiSeriesSetupEffect {
    None,
    Start(AiSeriesSetup),
    Cancel,
    RecoverLast,
    ActivateSeriesAction(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiSeriesControl {
    Name,
    MatchCount,
    Decrease,
    Increase,
    MapRow(usize),
    PreviousMaps,
    NextMaps,
    Randomize,
    Advanced,
    FirstSeed,
    CustomSeeds,
    Start,
    Cancel,
    RecoverLast,
}

#[derive(Debug, Clone)]
pub(crate) struct AiSeriesSetupState {
    pub name: SavedSeedDescriptionEdit,
    pub match_count: SavedSeedDescriptionEdit,
    pub first_seed: SavedSeedDescriptionEdit,
    pub custom_seeds: SavedSeedDescriptionEdit,
    pub maps: Vec<AiSeriesMapChoice>,
    pub selected_maps: Vec<usize>,
    pub entrant_count: usize,
    pub map_top: usize,
    pub randomize_each_round: bool,
    pub advanced: bool,
    pub focused_control: AiSeriesControl,
    pub buttons: ShellButtonInteraction<AiSeriesControl>,
    pub error: Option<String>,
    pub recovery_available: bool,
}

impl AiSeriesSetupState {
    pub(crate) fn new(
        mut maps: Vec<AiSeriesMapChoice>,
        selected_map: usize,
        entrant_count: usize,
        first_seed: u32,
    ) -> Self {
        maps.retain(|map| map.capacity >= entrant_count);
        let selected = maps
            .iter()
            .find(|map| map.record_index == selected_map)
            .or_else(|| maps.first())
            .map(|map| map.record_index);
        let mut state = Self {
            name: edit("AI Championship", NAME_LIMIT),
            match_count: edit("5", 3),
            first_seed: edit(&first_seed.to_string(), 10),
            custom_seeds: SavedSeedDescriptionEdit::default(),
            maps,
            selected_maps: selected.into_iter().collect(),
            entrant_count,
            map_top: 0,
            randomize_each_round: false,
            advanced: false,
            focused_control: AiSeriesControl::Name,
            buttons: ShellButtonInteraction::default(),
            error: None,
            recovery_available: false,
        };
        state.focus(AiSeriesControl::Name);
        state
    }

    pub(crate) fn set_recovery_available(&mut self, available: bool) {
        self.recovery_available = available;
    }

    pub(crate) fn focus(&mut self, control: AiSeriesControl) {
        self.focused_control = control;
        self.name.focused = control == AiSeriesControl::Name;
        self.match_count.focused = control == AiSeriesControl::MatchCount;
        self.first_seed.focused = control == AiSeriesControl::FirstSeed;
        self.custom_seeds.focused = control == AiSeriesControl::CustomSeeds;
    }

    pub(crate) fn focused_edit_mut(&mut self) -> Option<&mut SavedSeedDescriptionEdit> {
        match self.focused_control {
            AiSeriesControl::Name => Some(&mut self.name),
            AiSeriesControl::MatchCount => Some(&mut self.match_count),
            AiSeriesControl::FirstSeed => Some(&mut self.first_seed),
            AiSeriesControl::CustomSeeds => Some(&mut self.custom_seeds),
            _ => None,
        }
    }

    pub(crate) fn align_map_page(&mut self, rows: usize) {
        let rows = rows.max(1);
        self.map_top = (self.map_top.min(self.maps.len().saturating_sub(1)) / rows) * rows;
    }

    pub(crate) fn page_count(&self, rows: usize) -> usize {
        self.maps.len().div_ceil(rows.max(1)).max(1)
    }

    pub(crate) fn visible_maps(&self, rows: usize) -> impl Iterator<Item = &AiSeriesMapChoice> {
        self.maps.iter().skip(self.map_top).take(rows)
    }

    pub(crate) fn selected(&self, map: &AiSeriesMapChoice) -> bool {
        self.selected_maps.contains(&map.record_index)
    }

    pub(crate) fn selected_order(&self, map: &AiSeriesMapChoice) -> Option<usize> {
        self.selected_maps
            .iter()
            .position(|index| *index == map.record_index)
            .map(|index| index + 1)
    }

    pub(crate) fn activate(
        &mut self,
        control: AiSeriesControl,
        rows: usize,
    ) -> AiSeriesSetupEffect {
        use AiSeriesControl::*;
        self.error = None;
        self.align_map_page(rows);
        self.focus(control);
        match control {
            Decrease | Increase => {
                let count = text(&self.match_count).parse::<i32>().unwrap_or(5);
                let count = (count + if control == Increase { 1 } else { -1 }).clamp(1, 100);
                self.match_count = edit(&count.to_string(), 3);
                self.focus(MatchCount);
            }
            MapRow(row) => {
                if let Some(map) = self.maps.get(self.map_top + row).filter(|_| row < rows) {
                    let index = map.record_index;
                    if let Some(position) = self.selected_maps.iter().position(|id| *id == index) {
                        self.selected_maps.remove(position);
                    } else {
                        self.selected_maps.push(index);
                    }
                }
            }
            PreviousMaps => self.map_top = self.map_top.saturating_sub(rows.max(1)),
            NextMaps => {
                if self.map_top + rows.max(1) < self.maps.len() {
                    self.map_top += rows.max(1);
                }
            }
            Randomize => self.randomize_each_round = !self.randomize_each_round,
            Advanced => {
                self.advanced = !self.advanced;
                self.map_top = 0;
            }
            Start => match self.validated_setup() {
                Ok(setup) => return AiSeriesSetupEffect::Start(setup),
                Err(error) => self.error = Some(error),
            },
            Cancel => return AiSeriesSetupEffect::Cancel,
            RecoverLast if self.recovery_available => return AiSeriesSetupEffect::RecoverLast,
            RecoverLast => {}
            Name | MatchCount | FirstSeed | CustomSeeds => {}
        }
        AiSeriesSetupEffect::None
    }

    pub(crate) fn validated_setup(&self) -> Result<AiSeriesSetup, String> {
        let name = text(&self.name).trim().to_owned();
        if name.is_empty() {
            return Err("Enter a series name.".into());
        }
        let match_count = text(&self.match_count)
            .trim()
            .parse::<u8>()
            .ok()
            .filter(|count| (1..=100).contains(count))
            .ok_or("Choose 1 to 100 matches.")?;
        if self.selected_maps.is_empty() {
            return Err("Select at least one map for the rotation.".into());
        }
        for index in &self.selected_maps {
            if !self
                .maps
                .iter()
                .any(|map| map.record_index == *index && map.capacity >= self.entrant_count)
            {
                return Err("A selected map cannot seat every AI entrant.".into());
            }
        }
        let first_seed = text(&self.first_seed)
            .trim()
            .parse::<u32>()
            .map_err(|_| "Enter a first seed from 0 to 4294967295.".to_owned())?;
        let custom_text = text(&self.custom_seeds);
        let custom_seeds = if custom_text.trim().is_empty() {
            None
        } else {
            let seeds = custom_text
                .split(|ch: char| ch == ',' || ch.is_whitespace())
                .filter(|item| !item.is_empty())
                .map(|item| item.parse::<u32>())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "Use whole-number seeds separated by commas or spaces.".to_owned())?;
            if seeds.len() != usize::from(match_count) {
                return Err(format!(
                    "Enter exactly {match_count} seeds, or leave the list empty."
                ));
            }
            Some(seeds)
        };
        if custom_seeds.is_none() && first_seed.checked_add(u32::from(match_count) - 1).is_none() {
            return Err(
                "The seed sequence exceeds 4294967295; choose a smaller first seed.".into(),
            );
        }
        Ok(AiSeriesSetup {
            name,
            match_count,
            map_indices: self.selected_maps.clone(),
            randomize_each_round: self.randomize_each_round,
            first_seed,
            custom_seeds,
        })
    }

    pub(crate) fn key(
        &mut self,
        code: Option<KeyCode>,
        entered: Option<&str>,
        shift: bool,
        rows: usize,
    ) -> AiSeriesSetupEffect {
        if code == Some(KeyCode::Escape) {
            return AiSeriesSetupEffect::Cancel;
        }
        if code == Some(KeyCode::Tab) {
            let mut order = vec![AiSeriesControl::Name, AiSeriesControl::MatchCount];
            order.extend(
                (0..rows.min(self.maps.len().saturating_sub(self.map_top)))
                    .map(AiSeriesControl::MapRow),
            );
            order.extend([
                AiSeriesControl::PreviousMaps,
                AiSeriesControl::NextMaps,
                AiSeriesControl::Randomize,
                AiSeriesControl::Advanced,
            ]);
            if self.advanced {
                order.extend([AiSeriesControl::FirstSeed, AiSeriesControl::CustomSeeds]);
            }
            if self.recovery_available {
                order.push(AiSeriesControl::RecoverLast);
            }
            order.extend([AiSeriesControl::Start, AiSeriesControl::Cancel]);
            let current = order
                .iter()
                .position(|control| *control == self.focused_control)
                .unwrap_or(0);
            let next = if shift {
                (current + order.len() - 1) % order.len()
            } else {
                (current + 1) % order.len()
            };
            self.focus(order[next]);
            return AiSeriesSetupEffect::None;
        }
        if matches!(code, Some(KeyCode::Enter | KeyCode::NumpadEnter)) {
            let control = if self.focused_edit_mut().is_some() {
                AiSeriesControl::Start
            } else {
                self.focused_control
            };
            return self.activate(control, rows);
        }
        if code == Some(KeyCode::Space) && self.focused_edit_mut().is_none() {
            return self.activate(self.focused_control, rows);
        }
        if self.focused_control == AiSeriesControl::MatchCount {
            if code == Some(KeyCode::ArrowUp) {
                return self.activate(AiSeriesControl::Increase, rows);
            }
            if code == Some(KeyCode::ArrowDown) {
                return self.activate(AiSeriesControl::Decrease, rows);
            }
        }
        let numeric = matches!(
            self.focused_control,
            AiSeriesControl::MatchCount | AiSeriesControl::FirstSeed
        );
        let limit = match self.focused_control {
            AiSeriesControl::Name => NAME_LIMIT,
            AiSeriesControl::MatchCount => 3,
            AiSeriesControl::FirstSeed => 10,
            AiSeriesControl::CustomSeeds => 1100,
            _ => 0,
        };
        if let Some(edit) = self.focused_edit_mut() {
            match code {
                Some(KeyCode::Backspace) => edit.backspace(),
                Some(KeyCode::Delete) => edit.delete(),
                Some(KeyCode::ArrowLeft) => edit.left(),
                Some(KeyCode::ArrowRight) => edit.right(),
                Some(KeyCode::Home) => edit.home(),
                Some(KeyCode::End) => edit.end(),
                _ => {
                    if let Some(entered) = entered {
                        let filtered: String = entered
                            .chars()
                            .filter(|ch| !numeric || ch.is_ascii_digit())
                            .collect();
                        edit.insert_text_limited(&filtered, limit);
                    }
                }
            }
            self.error = None;
        }
        AiSeriesSetupEffect::None
    }
}

pub(crate) fn text(edit: &SavedSeedDescriptionEdit) -> String {
    String::from_utf16_lossy(&edit.units)
}

fn edit(value: &str, limit: usize) -> SavedSeedDescriptionEdit {
    let mut edit = SavedSeedDescriptionEdit::default();
    edit.set_units_limited(value.encode_utf16(), limit);
    edit
}

#[derive(Debug, Clone)]
pub(crate) struct AiSeriesSetupLayout {
    pub panel: RectPx,
    pub title: RectPx,
    pub summary: RectPx,
    pub name_label: RectPx,
    pub name: RectPx,
    pub count_label: RectPx,
    pub count: RectPx,
    pub decrease: RectPx,
    pub increase: RectPx,
    pub maps_label: RectPx,
    pub maps: Vec<RectPx>,
    pub previous_maps: RectPx,
    pub map_page: RectPx,
    pub next_maps: RectPx,
    pub randomize: RectPx,
    pub advanced: RectPx,
    pub first_seed_label: Option<RectPx>,
    pub first_seed: Option<RectPx>,
    pub custom_seeds_label: Option<RectPx>,
    pub custom_seeds: Option<RectPx>,
    pub status: RectPx,
    pub start: RectPx,
    pub cancel: RectPx,
    pub recover: RectPx,
}

impl AiSeriesSetupLayout {
    pub(crate) fn new(width: u32, height: u32, advanced: bool) -> Self {
        let width = width.min(i32::MAX as u32) as i32;
        let height = height.min(i32::MAX as u32) as i32;
        let w = (width - 24).clamp(1, 720);
        let h = (height - 24).clamp(1, if advanced { 680 } else { 592 });
        let panel = RectPx::new((width - w) / 2, (height - h) / 2, w, h);
        let x = panel.x + 16;
        let right = panel.x + panel.w - 16;
        let inner = (right - x).max(1);
        let title = RectPx::new(x, panel.y + 12, inner, 28);
        let summary = RectPx::new(x, title.y + 30, inner, 24);
        let name_label = RectPx::new(x, summary.y + 26, (inner - 180).max(1), 20);
        let name = RectPx::new(x, name_label.y + 20, name_label.w, 32);
        let count_label = RectPx::new(right - 168, name_label.y, 168, 20);
        let decrease = RectPx::new(count_label.x, name.y, 36, 32);
        let count = RectPx::new(decrease.x + 42, name.y, 80, 32);
        let increase = RectPx::new(right - 36, name.y, 36, 32);
        let maps_label = RectPx::new(x, name.y + 40, inner, 22);
        let reserved_bottom = if advanced { 242 } else { 134 };
        let row_count = ((panel.y + panel.h - reserved_bottom - maps_label.y - 24) / 32)
            .clamp(1, MAP_ROWS as i32) as usize;
        let maps: Vec<_> = (0..row_count)
            .map(|row| RectPx::new(x, maps_label.y + 24 + row as i32 * 32, inner, 30))
            .collect();
        let pager_y = maps.last().map_or(maps_label.y + 24, |row| row.y + 34);
        let previous_maps = RectPx::new(x, pager_y, 110, 30);
        let next_maps = RectPx::new(right - 110, pager_y, 110, 30);
        let map_page = RectPx::new(x + 116, pager_y, (inner - 232).max(1), 30);
        let randomize = RectPx::new(x, pager_y + 36, (inner - 138).max(1), 30);
        let advanced_button = RectPx::new(right - 132, randomize.y, 132, 30);
        let first_seed_label = advanced.then(|| RectPx::new(x, randomize.y + 36, 170, 24));
        let first_seed =
            first_seed_label.map(|rect| RectPx::new(x + 176, rect.y, (inner - 176).max(1), 30));
        let custom_seeds_label = advanced.then(|| RectPx::new(x, randomize.y + 70, inner, 20));
        let custom_seeds = custom_seeds_label.map(|rect| RectPx::new(x, rect.y + 20, inner, 30));
        let footer_y = panel.y + panel.h - 44;
        let status = RectPx::new(x, footer_y - 30, inner, 26);
        let start = RectPx::new(right - 164, footer_y, 164, 32);
        let cancel = RectPx::new(x, footer_y, 110, 32);
        let recover = RectPx::new(x + 118, footer_y, (inner - 290).max(1), 32);
        Self {
            panel,
            title,
            summary,
            name_label,
            name,
            count_label,
            count,
            decrease,
            increase,
            maps_label,
            maps,
            previous_maps,
            map_page,
            next_maps,
            randomize,
            advanced: advanced_button,
            first_seed_label,
            first_seed,
            custom_seeds_label,
            custom_seeds,
            status,
            start,
            cancel,
            recover,
        }
    }

    pub(crate) fn control_rect(&self, control: AiSeriesControl) -> Option<RectPx> {
        use AiSeriesControl::*;
        Some(match control {
            Name => self.name,
            MatchCount => self.count,
            Decrease => self.decrease,
            Increase => self.increase,
            MapRow(row) => *self.maps.get(row)?,
            PreviousMaps => self.previous_maps,
            NextMaps => self.next_maps,
            Randomize => self.randomize,
            Advanced => self.advanced,
            FirstSeed => self.first_seed?,
            CustomSeeds => self.custom_seeds?,
            Start => self.start,
            Cancel => self.cancel,
            RecoverLast => self.recover,
        })
    }

    pub(crate) fn control_at(&self, x: i32, y: i32) -> Option<AiSeriesControl> {
        use AiSeriesControl::*;
        [
            Name,
            MatchCount,
            Decrease,
            Increase,
            PreviousMaps,
            NextMaps,
            Randomize,
            Advanced,
            FirstSeed,
            CustomSeeds,
            Start,
            Cancel,
            RecoverLast,
        ]
        .into_iter()
        .chain((0..self.maps.len()).map(MapRow))
        .find(|control| {
            self.control_rect(*control)
                .is_some_and(|rect| rect.contains(x, y))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> AiSeriesSetupState {
        AiSeriesSetupState::new(
            (0..12)
                .map(|index| AiSeriesMapChoice {
                    record_index: index,
                    label: format!("Map {index}"),
                    capacity: 30,
                })
                .collect(),
            9,
            29,
            100,
        )
    }

    #[test]
    fn draft_preserves_rotation_order_and_cancel_emits_no_setup() {
        let mut state = fresh();
        state.activate(AiSeriesControl::MapRow(1), 7);
        state.activate(AiSeriesControl::NextMaps, 7);
        state.activate(AiSeriesControl::MapRow(4), 7);
        let setup = state.validated_setup().unwrap();
        assert_eq!(setup.name, "AI Championship");
        assert_eq!(setup.match_count, 5);
        assert_eq!(setup.map_indices, [9, 1, 11]);
        assert!(!setup.randomize_each_round);
        assert_eq!(
            state.key(Some(KeyCode::Escape), None, false, 7),
            AiSeriesSetupEffect::Cancel
        );
    }

    #[test]
    fn start_rejects_empty_rotation_and_seed_overflow_without_closing() {
        let mut state = fresh();
        state.selected_maps.clear();
        assert_eq!(
            state.activate(AiSeriesControl::Start, 7),
            AiSeriesSetupEffect::None
        );
        assert!(
            state
                .error
                .as_ref()
                .unwrap()
                .contains("Select at least one map")
        );
        state.selected_maps.push(9);
        state.first_seed = edit("4294967295", 10);
        assert!(
            state
                .validated_setup()
                .unwrap_err()
                .contains("seed sequence")
        );
        state.match_count = edit("1", 3);
        assert_eq!(state.validated_setup().unwrap().first_seed, u32::MAX);
        state.match_count = edit("2", 3);
        state.custom_seeds = edit("19, 7", 1100);
        assert_eq!(
            state.validated_setup().unwrap().custom_seeds,
            Some(vec![19, 7])
        );
    }

    #[test]
    fn compact_layout_matches_hits_and_keeps_advanced_controls_separate() {
        let layout = AiSeriesSetupLayout::new(640, 480, true);
        for control in [
            AiSeriesControl::Name,
            AiSeriesControl::MatchCount,
            AiSeriesControl::FirstSeed,
            AiSeriesControl::CustomSeeds,
            AiSeriesControl::Start,
            AiSeriesControl::Cancel,
        ] {
            let rect = layout.control_rect(control).unwrap();
            assert!(rect.h >= 28);
            assert_eq!(
                layout.control_at(rect.x + rect.w / 2, rect.y + rect.h / 2),
                Some(control)
            );
            assert!(rect.x >= 0 && rect.y >= 0 && rect.x + rect.w <= 640 && rect.y + rect.h <= 480);
        }
        assert!(layout.maps.last().unwrap().y + 30 <= layout.previous_maps.y);
        assert!(layout.custom_seeds.unwrap().y + 30 <= layout.status.y);
    }
}
