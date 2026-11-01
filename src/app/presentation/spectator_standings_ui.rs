//! Read-only, paged spectator standings and current-house camera navigation.

use super::spectator_standings::{StandingsProjection, StandingsRow};
use super::spectator_tools::{SpectatorToolAction, localized};
use crate::app::AppState;
use crate::app::input::camera::{self, spectator, tactical_viewport_size};
use crate::app::match_runtime::frame_pacer::wall_clock_ms;
use crate::render::batch::SpriteInstance;
use crate::sim::intern::InternedId;
use crate::ui::gadget::GadgetRect;

const PAGE_SIZE: usize = 7;
const PAD: i32 = 8;
const GAP: i32 = 4;
const ROW_HEIGHT: i32 = 28;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Sort {
    Name,
    #[default]
    Army,
    Buildings,
    Credits,
    Kills,
}

impl Sort {
    fn next(self) -> Self {
        match self {
            Self::Name => Self::Army,
            Self::Army => Self::Buildings,
            Self::Buildings => Self::Credits,
            Self::Credits => Self::Kills,
            Self::Kills => Self::Name,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Army => "Army",
            Self::Buildings => "Buildings",
            Self::Credits => "Credits",
            Self::Kills => "Kills",
        }
    }
}

#[derive(Default)]
pub(crate) struct StandingsView {
    projection: StandingsProjection,
    open: bool,
    page: usize,
    selected: Option<InternedId>,
    sort: Sort,
    notice: Option<String>,
    hover: Option<((i32, i32), u64)>,
}

impl StandingsView {
    pub(crate) fn projection(&self) -> &StandingsProjection {
        &self.projection
    }
}

pub(crate) fn update(state: &mut AppState) {
    if !spectator::available(state) {
        return;
    }
    let Some(runtime) = state.match_state.sim_runtime.as_ref() else {
        return;
    };
    let pointer = state.cursor_position();
    let point = (pointer.0 as i32, pointer.1 as i32);
    let tools = &mut state.match_state.match_presentation.spectator_tools;
    if tools.has_capture() {
        return;
    }
    let view = &mut tools.standings;
    if view.hover.is_none_or(|(previous, _)| previous != point) {
        view.hover = Some((point, wall_clock_ms()));
    }
    view.projection.refresh(
        &runtime.simulation,
        &runtime.resources.rules,
        wall_clock_ms(),
        view.open,
    );
}

pub(crate) fn count_label(state: &AppState) -> String {
    let projection = &state
        .match_state
        .match_presentation
        .spectator_tools
        .standings
        .projection;
    let (alive, initial) = (projection.alive_count(), projection.initial_count());
    localized(state, "VERA:StandingsAiCount", "AIs: {alive} / {initial}")
        .replace("{alive}", &alive.to_string())
        .replace("{initial}", &initial.to_string())
}

fn player_name(state: &AppState, owner: InternedId, ordinal: usize) -> String {
    let name = super::house_ui::house_ui_name(state, owner).unwrap_or_else(|| "AI".into());
    format!("{ordinal:02} {name}")
}

fn row_label(state: &AppState, row: &StandingsRow) -> String {
    let status = if row.alive() {
        localized(state, "VERA:StandingsAlive", "Alive")
    } else {
        localized(state, "VERA:StandingsOut", "Out")
    };
    localized(
        state,
        "VERA:StandingsRow",
        "{player} | {status} | Army {value}",
    )
    .replace("{player}", &player_name(state, row.owner(), row.ordinal()))
    .replace("{status}", &status)
    .replace("{value}", &row.army_value().to_string())
}

fn ordered_owners(state: &AppState) -> Vec<InternedId> {
    let view = &state
        .match_state
        .match_presentation
        .spectator_tools
        .standings;
    let mut rows: Vec<_> = view.projection.rows().iter().collect();
    rows.sort_by(|a, b| {
        b.alive()
            .cmp(&a.alive())
            .then_with(|| match view.sort {
                Sort::Name => super::house_ui::house_ui_name(state, a.owner())
                    .cmp(&super::house_ui::house_ui_name(state, b.owner())),
                Sort::Army => b.army_value().cmp(&a.army_value()),
                Sort::Buildings => b.buildings().cmp(&a.buildings()),
                Sort::Credits => b.credits().cmp(&a.credits()),
                Sort::Kills => b.kills().cmp(&a.kills()),
            })
            .then_with(|| a.ordinal().cmp(&b.ordinal()))
    });
    rows.into_iter().map(|row| row.owner()).collect()
}

struct Control {
    action: SpectatorToolAction,
    label: String,
    rect: GadgetRect,
    color: Option<[f32; 3]>,
}

pub(crate) struct StandingsLayout {
    panel: GadgetRect,
    controls: Vec<Control>,
    details: Vec<String>,
    detail_top: i32,
    page_label: String,
    help: GadgetRect,
    page: usize,
}

impl StandingsLayout {
    pub(crate) fn hit(&self, point: (i32, i32)) -> Option<SpectatorToolAction> {
        self.controls
            .iter()
            .find(|control| control.rect.contains(point.0, point.1))
            .map(|control| control.action)
    }

    pub(crate) fn right_edge(&self) -> i32 {
        self.panel.x + self.panel.w
    }

    pub(crate) fn contains(&self, point: (i32, i32)) -> bool {
        self.panel.contains(point.0, point.1)
    }

    pub(crate) fn controls(&self) -> Vec<SpectatorToolAction> {
        self.controls.iter().map(|control| control.action).collect()
    }

    pub(crate) fn center(&self, action: SpectatorToolAction) -> Option<(i32, i32)> {
        let rect = self
            .controls
            .iter()
            .find(|control| control.action == action)?
            .rect;
        Some((rect.x + rect.w / 2, rect.y + rect.h / 2))
    }
}

fn wrapped(state: &AppState, text: &str, width: i32) -> Vec<String> {
    let font = &state.renderer.bit_font;
    let mut lines = Vec::new();
    let mut line = String::new();
    for character in text.chars() {
        line.push(character);
        if font.text_width(&line) > width.max(1) as u32 && line.chars().count() > 1 {
            line.pop();
            lines.push(std::mem::take(&mut line));
            line.push(character);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn font_height(state: &AppState) -> i32 {
    state.renderer.bit_font.cell_height() as i32
}

fn page_capacity(available_height: i32, reserved: i32, row_height: i32) -> Option<usize> {
    let count = available_height.saturating_sub(reserved) / row_height.max(1);
    (count > 0).then_some((count as usize).min(PAGE_SIZE))
}

pub(crate) fn layout(state: &AppState, top: i32) -> Option<StandingsLayout> {
    let view = &state
        .match_state
        .match_presentation
        .spectator_tools
        .standings;
    if !view.open {
        return None;
    }
    let viewport = tactical_viewport_size(state);
    let width = (viewport.0 as i32 * 3 / 5)
        .clamp(256, 420)
        .min(viewport.0 as i32 - 24);
    let row_width = width - 2 * PAD;
    if row_width < 180 {
        return None;
    }
    let sim = &state.match_state.sim_runtime.as_ref()?.simulation;
    let mut details = Vec::new();
    if let Some(owner) = view.selected {
        if let Some(row) = view
            .projection
            .rows()
            .iter()
            .find(|row| row.owner() == owner)
        {
            if let Some(house) = sim.houses().get(&owner) {
                let country = house
                    .country()
                    .and_then(|country| super::house_ui::country_display_name(state, sim, country))
                    .unwrap_or_else(|| "Unknown country".into());
                let faction = u8::try_from(house.side_index())
                    .ok()
                    .and_then(|index| {
                        state
                            .rules()?
                            .side_name(crate::rules::ruleset::SideIdx(index))
                    })
                    .unwrap_or("Unknown faction");
                let faction = match faction.to_ascii_lowercase().as_str() {
                    "gdi" => localized(state, "VERA:StandingsAllied", "Allied"),
                    "nod" => localized(state, "VERA:StandingsSoviet", "Soviet"),
                    "thirdside" => localized(state, "VERA:StandingsYuri", "Yuri"),
                    _ => faction.to_owned(),
                };
                let commander = house
                    .personality()
                    .and_then(|index| state.rules()?.personalities().get(index))
                    .map(|personality| {
                        let name = personality
                            .display_name
                            .as_deref()
                            .unwrap_or(&personality.name);
                        state
                            .process_assets
                            .csf
                            .as_ref()
                            .and_then(|csf| csf.get(name))
                            .unwrap_or(name)
                            .to_owned()
                    })
                    .unwrap_or_else(|| "Standard".into());
                for text in [
                    format!(
                        "{} | {country} | {faction}",
                        player_name(state, owner, row.ordinal())
                    ),
                    format!("Commander: {commander}"),
                    format!(
                        "Army: {}   Buildings: {}   Credits: {}   Kills: {}",
                        row.army_value(),
                        row.buildings(),
                        row.credits(),
                        row.kills()
                    ),
                ] {
                    details.extend(wrapped(state, &text, row_width));
                }
            }
        }
    } else {
        details.extend(wrapped(
            state,
            "Select a player for faction, commander, buildings, credits, and kills.",
            row_width,
        ));
    }
    if let Some(notice) = &view.notice {
        details.extend(wrapped(state, notice, row_width));
    }
    let line_height = state.renderer.bit_font.cell_height() as i32 + 3;
    let owners = ordered_owners(state);
    let row_height = view
        .projection
        .rows()
        .iter()
        .map(|row| {
            let label = row_label(state, row);
            ROW_HEIGHT
                .max(wrapped(state, &label, row_width - 24).len() as i32 * font_height(state) + 8)
        })
        .max()
        .unwrap_or(ROW_HEIGHT);
    let footer_lines = wrapped(
        state,
        "Page 99 / 99 - click an AI to visit its base",
        row_width,
    )
    .len() as i32;
    let footer_height = footer_lines * font_height(state) + GAP;
    let reserved =
        2 * PAD + 2 * (ROW_HEIGHT + GAP) + footer_height + details.len() as i32 * line_height;
    let page_size = page_capacity(viewport.1 as i32 - top - 8, reserved, row_height + GAP)?;
    let pages = owners.len().div_ceil(page_size).max(1);
    let page = view
        .selected
        .and_then(|owner| owners.iter().position(|candidate| *candidate == owner))
        .map_or(view.page.min(pages - 1), |index| index / page_size);
    let mut controls = vec![Control {
        action: SpectatorToolAction::SortStandings,
        label: localized(state, "VERA:StandingsSort", "Sort: {metric}").replace(
            "{metric}",
            &localized(
                state,
                &format!("VERA:StandingsSort{}", view.sort.label()),
                view.sort.label(),
            ),
        ),
        rect: GadgetRect::new(12 + PAD, top + PAD, row_width, ROW_HEIGHT),
        color: None,
    }];
    let mut y = top + PAD + ROW_HEIGHT + GAP;
    let computer = crate::app::frontend::shell_pass::resolve_csf(state, "TXT_COMPUTER");
    let sources = super::house_ui::HouseUiSources::from_state(state, &computer);
    for owner in owners.iter().skip(page * page_size).take(page_size) {
        let row = view
            .projection
            .rows()
            .iter()
            .find(|row| row.owner() == *owner)?;
        let label = row_label(state, row);
        let height = row_height;
        let color = sources
            .text_rgb(sim, *owner)
            .map(|value| value as f32 / 255.0);
        controls.push(Control {
            action: SpectatorToolAction::VisitHouse(*owner),
            label,
            rect: GadgetRect::new(12 + PAD, y, row_width, height),
            color: Some(color),
        });
        y += height + GAP;
    }
    let half = (row_width - GAP) / 2;
    if page > 0 {
        controls.push(Control {
            action: SpectatorToolAction::PreviousStandings,
            label: localized(state, "VERA:StandingsPrevious", "Previous"),
            rect: GadgetRect::new(12 + PAD, y, half, ROW_HEIGHT),
            color: None,
        });
    }
    if page + 1 < pages {
        controls.push(Control {
            action: SpectatorToolAction::NextStandings,
            label: localized(state, "VERA:StandingsNext", "Next"),
            rect: GadgetRect::new(12 + PAD + half + GAP, y, half, ROW_HEIGHT),
            color: None,
        });
    }
    y += ROW_HEIGHT + GAP;
    let page_label = localized(
        state,
        "VERA:StandingsPage",
        "Page {page} / {pages} - click an AI to visit its base",
    )
    .replace("{page}", &(page + 1).to_string())
    .replace("{pages}", &pages.to_string());
    let help = GadgetRect::new(12 + PAD, y, row_width, footer_height);
    y += footer_height;
    let height = y + details.len() as i32 * line_height + PAD - top;
    if top + height > viewport.1 as i32 - 8 {
        return None;
    }
    Some(StandingsLayout {
        panel: GadgetRect::new(12, top, width, height),
        controls,
        details,
        detail_top: y,
        page_label,
        help,
        page,
    })
}

pub(crate) fn activate(state: &mut AppState, action: SpectatorToolAction) {
    let current_page = super::spectator_tools::panel_layout(state).map_or(0, |layout| layout.page);
    let no_survivors = localized(state, "VERA:StandingsNoSurvivors", "No surviving units");
    let view = &mut state
        .match_state
        .match_presentation
        .spectator_tools
        .standings;
    match action {
        SpectatorToolAction::ToggleStandings => {
            view.open = !view.open;
            view.notice = None;
        }
        SpectatorToolAction::SortStandings => view.sort = view.sort.next(),
        SpectatorToolAction::PreviousStandings => {
            view.selected = None;
            view.page = current_page.saturating_sub(1);
        }
        SpectatorToolAction::NextStandings => {
            view.selected = None;
            view.page = current_page + 1;
        }
        SpectatorToolAction::VisitHouse(owner) => {
            view.selected = Some(owner);
            let point = state
                .match_state
                .sim_runtime
                .as_ref()
                .and_then(|runtime| view.projection.focus_target(&runtime.simulation, owner));
            if let Some(point) = point {
                view.notice = None;
                state.match_state.input.follow_target = None;
                camera::center_camera_on_lepton_point(state, point[0], point[1], point[2]);
            } else {
                view.notice = Some(no_survivors);
            }
        }
        _ => {}
    }
    update(state);
}

pub(crate) fn close(state: &mut AppState) -> bool {
    let view = &mut state
        .match_state
        .match_presentation
        .spectator_tools
        .standings;
    std::mem::replace(&mut view.open, false)
}

pub(crate) fn build_instances(
    state: &AppState,
    layout: &StandingsLayout,
    hovered: Option<SpectatorToolAction>,
    focused: Option<SpectatorToolAction>,
    pressed: Option<SpectatorToolAction>,
) -> (Vec<SpriteInstance>, Vec<SpriteInstance>) {
    let camera = [
        state.match_state.input.camera_x,
        state.match_state.input.camera_y,
    ];
    let font = &state.renderer.bit_font;
    let fill = |rect: GadgetRect, tint: [f32; 3]| SpriteInstance {
        position: [rect.x as f32 + camera[0], rect.y as f32 + camera[1]],
        size: [rect.w as f32, rect.h as f32],
        uv_origin: [0.0, 0.0],
        uv_size: [1.0, 1.0],
        tint,
        alpha: 1.0,
        ..Default::default()
    };
    let mut fills = vec![fill(layout.panel, [0.035, 0.045, 0.055])];
    let mut texts = Vec::new();
    for control in &layout.controls {
        let selected = focused == Some(control.action)
            || hovered == Some(control.action)
            || matches!(control.action, SpectatorToolAction::VisitHouse(owner) if state.match_state.match_presentation.spectator_tools.standings.selected == Some(owner));
        let tint = if pressed == Some(control.action) {
            [0.12, 0.17, 0.2]
        } else if selected {
            [0.25, 0.31, 0.2]
        } else {
            [0.15, 0.2, 0.24]
        };
        fills.push(fill(control.rect, tint));
        if let Some(color) = control.color {
            fills.push(fill(
                GadgetRect::new(
                    control.rect.x + 3,
                    control.rect.y + 4,
                    6,
                    control.rect.h - 8,
                ),
                color,
            ));
        }
        let inset = if control.color.is_some() { 14 } else { 8 };
        for (index, line) in wrapped(state, &control.label, control.rect.w - inset - 8)
            .iter()
            .enumerate()
        {
            texts.extend(font.build_text(
                line,
                (control.rect.x + inset) as f32,
                (control.rect.y + 7 + index as i32 * font.cell_height() as i32) as f32,
                1.0,
                0.0,
                [0.94, 0.96, 0.9],
                camera,
            ));
        }
    }
    for (index, line) in wrapped(state, &layout.page_label, layout.help.w)
        .iter()
        .enumerate()
    {
        texts.extend(font.build_text(
            line,
            layout.help.x as f32,
            (layout.help.y + index as i32 * font.cell_height() as i32) as f32,
            1.0,
            0.0,
            [0.75, 0.83, 0.87],
            camera,
        ));
    }
    for (index, line) in layout.details.iter().enumerate() {
        texts.extend(font.build_text(
            line,
            (layout.panel.x + PAD) as f32,
            (layout.detail_top + index as i32 * (font.cell_height() as i32 + 3)) as f32,
            1.0,
            0.0,
            [0.94, 0.96, 0.9],
            camera,
        ));
    }
    let pointer = state.cursor_position();
    let point = (pointer.0 as i32, pointer.1 as i32);
    if layout.contains(point)
        && state
            .match_state
            .match_presentation
            .spectator_tools
            .standings
            .hover
            .is_some_and(|(position, since)| {
                position == point && wall_clock_ms().saturating_sub(since) >= 700
            })
    {
        let explanation = if layout.help.contains(point.0, point.1) {
            "AIs: surviving / initial computer combatants. Sort keeps surviving players first; entrant order breaks ties."
        } else if layout
            .controls
            .iter()
            .any(|control| control.color.is_some() && control.rect.contains(point.0, point.1))
        {
            "Army: current replacement cost of live combat units. Excludes buildings, civilians, cargo, and unarmed or support-only units."
        } else if point.1 >= layout.detail_top {
            "Buildings: live structures. Credits: spendable balance. Kills: the house score counter, including its native scoring rules."
        } else {
            "Sort cycles Army, Buildings, Credits, Kills, and Name. Click a player to inspect and visit its base."
        };
        let explanation =
            localized(
                state,
                if layout.help.contains(point.0, point.1) {
                    "VERA:StandingsCountHelp"
                } else if point.1 >= layout.detail_top {
                    "VERA:StandingsDetailsHelp"
                } else if layout.controls.iter().any(|control| {
                    control.color.is_some() && control.rect.contains(point.0, point.1)
                }) {
                    "VERA:StandingsArmyHelp"
                } else {
                    "VERA:StandingsSortHelp"
                },
                explanation,
            );
        let lines = wrapped(state, &explanation, layout.panel.w - 2 * PAD);
        let height = lines.len() as i32 * (font.cell_height() as i32 + 3) + 2 * PAD;
        let top = (point.1 + 24)
            .min(tactical_viewport_size(state).1 as i32 - height - 8)
            .max(0);
        fills.push(fill(
            GadgetRect::new(layout.panel.x, top, layout.panel.w, height),
            [0.05, 0.07, 0.09],
        ));
        for (index, line) in lines.iter().enumerate() {
            texts.extend(font.build_text(
                line,
                (layout.panel.x + PAD) as f32,
                (top + PAD + index as i32 * (font.cell_height() as i32 + 3)) as f32,
                1.0,
                0.0,
                [1.0, 0.97, 0.78],
                camera,
            ));
        }
    }
    (fills, texts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resized_standings_keep_rows_and_controls_inside_available_height() {
        assert_eq!(page_capacity(448, 208, 32), Some(7));
        assert_eq!(page_capacity(320, 208, 32), Some(3));
        assert_eq!(page_capacity(200, 208, 32), None);
    }
}
