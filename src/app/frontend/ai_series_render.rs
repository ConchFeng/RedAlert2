//! AI series setup and recovery cards using the shared shell composition.

use crate::app::AppState;
use crate::app::frontend::shell_pass::{
    ShellComposition, TexturedDraw, encode_shell_pass, owner_draw_button_label_rect,
    software_cursor,
};
use crate::render::batch::SpriteInstance;
use crate::render::main_menu_shell_chrome::MainMenuShellChromeEntry;
use crate::render::shell_paint::{
    self, CURSOR_DEPTH, PaintButton, PaintLabel, SHELL_TEXT_RGB_ENABLED,
};
use crate::render::shell_text::ShellAlign;
use crate::ui::ai_series::{AiSeriesControl, AiSeriesSetupEffect, AiSeriesSetupLayout};
use crate::ui::game_screen::GameScreen;
use crate::ui::shell::geom::RectPx;
use winit::keyboard::KeyCode;

pub(crate) fn layout(state: &AppState) -> AiSeriesSetupLayout {
    AiSeriesSetupLayout::new(
        state.render_width(),
        state.render_height(),
        state
            .frontend
            .ai_series_setup
            .as_ref()
            .is_some_and(|setup| setup.advanced),
    )
}

pub(crate) fn entry_rect(state: &AppState) -> Option<RectPx> {
    if !crate::app::ai_series::setup_available(state)
        || state.frontend.skirmish_shell_state.top_dialog()
            != Some(crate::ui::skirmish_shell::SkirmishShellDialog::Skirmish)
        || state.frontend.ai_series_setup.is_some()
    {
        return None;
    }
    let layout = crate::app::frontend::shell_layout::skirmish_layout(state);
    let anchor = layout.choose_map_button;
    Some(RectPx::new(
        anchor.x,
        anchor.y + anchor.h + 8,
        anchor.w,
        anchor.h.max(28),
    ))
}

pub(crate) fn score_action_rect(state: &AppState, id: u16) -> Option<RectPx> {
    let layout = crate::app::frontend::shell_layout::score_layout(state);
    score_action_rect_from_anchor(layout.continue_button(), id)
}

fn score_action_rect_from_anchor(anchor: RectPx, id: u16) -> Option<RectPx> {
    let row = match id {
        0xF210 => 4,
        0xF211 => 3,
        0xF212 => 0,
        0xF213 => 3,
        0xF214 => 4,
        0xF215 => 6,
        _ => return None,
    };
    let compact = matches!(id, 0xF211 | 0xF213);
    Some(RectPx::new(
        anchor.x,
        anchor.y - row * (anchor.h + 8) - if compact { 4 } else { 0 },
        anchor.w,
        if compact { 28 } else { anchor.h },
    ))
}

pub(crate) fn dialog_active(state: &AppState) -> bool {
    state.frontend.ai_series_setup.is_some()
        || (state.frontend.ai_series.is_some()
            && state.frontend.score_page.is_none()
            && !matches!(
                state.frontend.screen,
                GameScreen::InGame | GameScreen::Loading
            )
            && !crate::app::ai_series::score_actions(state).is_empty())
}

fn card_action_rect(state: &AppState, index: usize) -> RectPx {
    let layout = layout(state);
    RectPx::new(
        layout.panel.x + 20,
        layout.panel.y + layout.panel.h - 50 - index as i32 * 42,
        layout.panel.w - 40,
        34,
    )
}

fn card_control_at(state: &AppState, point: (i32, i32)) -> Option<u16> {
    crate::app::ai_series::score_actions(state)
        .iter()
        .enumerate()
        .find_map(|(index, (id, _))| {
            card_action_rect(state, index)
                .contains(point.0, point.1)
                .then_some(*id)
        })
}

pub(crate) fn mouse_down(state: &mut AppState, point: (i32, i32)) -> AiSeriesSetupEffect {
    let layout = layout(state);
    if let Some(setup) = state.frontend.ai_series_setup.as_mut() {
        let hit = layout
            .control_at(point.0, point.1)
            .filter(|control| *control != AiSeriesControl::RecoverLast || setup.recovery_available);
        setup.buttons.press(hit);
        if let Some(control) = hit {
            setup.focus(control);
        }
    } else {
        let hit = card_control_at(state, point);
        state.frontend.ai_series_dialog_buttons.press(hit);
    }
    AiSeriesSetupEffect::None
}

pub(crate) fn mouse_move(state: &mut AppState, point: (i32, i32)) -> bool {
    let layout = layout(state);
    if let Some(setup) = state.frontend.ai_series_setup.as_mut() {
        setup.buttons.hovered = layout
            .control_at(point.0, point.1)
            .filter(|control| *control != AiSeriesControl::RecoverLast || setup.recovery_available);
    } else {
        state.frontend.ai_series_dialog_buttons.hovered = card_control_at(state, point);
    }
    true
}

pub(crate) fn mouse_up(state: &mut AppState, point: (i32, i32)) -> AiSeriesSetupEffect {
    let layout = layout(state);
    if let Some(setup) = state.frontend.ai_series_setup.as_mut() {
        let hit = layout
            .control_at(point.0, point.1)
            .filter(|control| *control != AiSeriesControl::RecoverLast || setup.recovery_available);
        setup
            .buttons
            .release(hit)
            .map_or(AiSeriesSetupEffect::None, |control| {
                setup.activate(control, layout.maps.len())
            })
    } else {
        let hit = card_control_at(state, point);
        state.frontend.ai_series_dialog_buttons.release(hit).map_or(
            AiSeriesSetupEffect::None,
            AiSeriesSetupEffect::ActivateSeriesAction,
        )
    }
}

pub(crate) fn key(
    state: &mut AppState,
    code: Option<KeyCode>,
    entered: Option<&str>,
    shift: bool,
) -> AiSeriesSetupEffect {
    let rows = layout(state).maps.len();
    if let Some(setup) = state.frontend.ai_series_setup.as_mut() {
        return setup.key(code, entered, shift, rows);
    }
    let actions = crate::app::ai_series::score_actions(state);
    if code == Some(KeyCode::Tab) && !actions.is_empty() {
        let current = state
            .frontend
            .ai_series_dialog_buttons
            .hovered
            .and_then(|id| actions.iter().position(|(action, _)| *action == id));
        let next = match current {
            Some(current) if shift => (current + actions.len() - 1) % actions.len(),
            Some(current) => (current + 1) % actions.len(),
            None => 0,
        };
        state.frontend.ai_series_dialog_buttons.hovered = Some(actions[next].0);
    }
    if matches!(
        code,
        Some(KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space)
    ) {
        if let Some(id) = state
            .frontend
            .ai_series_dialog_buttons
            .hovered
            .or_else(|| actions.first().map(|(id, _)| *id))
        {
            return AiSeriesSetupEffect::ActivateSeriesAction(id);
        }
    }
    if code == Some(KeyCode::Escape) && actions.iter().any(|(id, _)| *id == 0xF212) {
        return AiSeriesSetupEffect::ActivateSeriesAction(0xF212);
    }
    AiSeriesSetupEffect::None
}

fn label(text: impl Into<String>, rect: RectPx, centered: bool) -> PaintLabel<'static> {
    PaintLabel {
        text: text.into().into(),
        rect,
        align: if centered {
            ShellAlign::H_CENTER | ShellAlign::V_CENTER
        } else {
            ShellAlign::V_CENTER
        },
        rgb: SHELL_TEXT_RGB_ENABLED,
        path_a_reveal: None,
    }
}

fn localized(state: &AppState, key: &str, fallback: &str) -> String {
    state
        .process_assets
        .csf
        .as_ref()
        .and_then(|csf| csf.get(key))
        .filter(|text| !text.starts_with("MISSING:"))
        .unwrap_or(fallback)
        .to_owned()
}

fn fill(
    out: &mut Vec<SpriteInstance>,
    pixel: MainMenuShellChromeEntry,
    rect: RectPx,
    tint: [f32; 3],
) {
    out.push(SpriteInstance {
        position: [rect.x as f32, rect.y as f32],
        size: [rect.w.max(0) as f32, rect.h.max(0) as f32],
        uv_origin: pixel.uv_origin,
        uv_size: pixel.uv_size,
        tint,
        depth: 0.0009,
        alpha: 1.0,
        ..Default::default()
    });
}

pub(crate) fn render_ai_series_setup(
    state: &mut AppState,
    encoder: &mut wgpu::CommandEncoder,
    destination: &wgpu::Texture,
) -> anyhow::Result<bool> {
    if !dialog_active(state) {
        return Ok(false);
    }
    let layout = layout(state);
    if let Some(setup) = state.frontend.ai_series_setup.as_mut() {
        setup.align_map_page(layout.maps.len());
        for (rect, edit) in [
            (Some(layout.name), &mut setup.name),
            (Some(layout.count), &mut setup.match_count),
            (layout.first_seed, &mut setup.first_seed),
            (layout.custom_seeds, &mut setup.custom_seeds),
        ] {
            if let Some(rect) = rect {
                edit.scroll_to_caret((rect.w - 24).max(0) as u32, |text| {
                    state.renderer.bit_font.text_width(text)
                });
            }
        }
    }
    let Some(chrome) = state.frontend.main_menu_shell_chrome.as_ref() else {
        return Ok(false);
    };
    let Some(pixel) = chrome.white_pixel else {
        return Ok(false);
    };
    let mut backgrounds = Vec::new();
    fill(
        &mut backgrounds,
        pixel,
        RectPx::new(
            0,
            0,
            state.render_width() as i32,
            state.render_height() as i32,
        ),
        [0.005, 0.008, 0.006],
    );
    fill(&mut backgrounds, pixel, layout.panel, [0.36, 0.035, 0.025]);
    fill(
        &mut backgrounds,
        pixel,
        RectPx::new(
            layout.panel.x + 2,
            layout.panel.y + 2,
            layout.panel.w - 4,
            layout.panel.h - 4,
        ),
        [0.025, 0.035, 0.027],
    );
    let mut labels = vec![label(
        localized(state, "VERA:AISeriesTitle", "AI Championship"),
        layout.title,
        true,
    )];
    let mut buttons = Vec::new();
    if let Some(setup) = state.frontend.ai_series_setup.as_ref() {
        labels.push(label(
            format!("{} AIs + Observer | Free For All", setup.entrant_count),
            layout.summary,
            false,
        ));
        labels.push(label(
            localized(state, "VERA:AISeriesName", "Series name"),
            layout.name_label,
            false,
        ));
        labels.push(label(
            localized(state, "VERA:AISeriesMatches", "Matches (1-100)"),
            layout.count_label,
            false,
        ));
        labels.push(label(
            format!(
                "Map rotation: {} selected (click to add/remove)",
                setup.selected_maps.len()
            ),
            layout.maps_label,
            false,
        ));
        let mut controls = vec![
            (AiSeriesControl::Decrease, "-".to_owned()),
            (AiSeriesControl::Increase, "+".to_owned()),
            (AiSeriesControl::PreviousMaps, "Previous".to_owned()),
            (AiSeriesControl::NextMaps, "Next".to_owned()),
            (
                AiSeriesControl::Randomize,
                format!(
                    "[{}] Randomize countries each round",
                    if setup.randomize_each_round { "x" } else { " " }
                ),
            ),
            (
                AiSeriesControl::Advanced,
                if setup.advanced { "Less" } else { "Advanced" }.to_owned(),
            ),
            (AiSeriesControl::Start, "Start Series".to_owned()),
            (AiSeriesControl::Cancel, "Cancel".to_owned()),
        ];
        if setup.recovery_available {
            controls.push((
                AiSeriesControl::RecoverLast,
                "Recover Last Series".to_owned(),
            ));
        }
        for (index, map) in setup.visible_maps(layout.maps.len()).enumerate() {
            let order = setup
                .selected_order(map)
                .map_or_else(|| " ".to_owned(), |order| order.to_string());
            controls.push((
                AiSeriesControl::MapRow(index),
                format!("[{order}] {} ({} starts)", map.label, map.capacity),
            ));
        }
        for (control, caption) in controls {
            let Some(rect) = layout.control_rect(control) else {
                continue;
            };
            let enabled = match control {
                AiSeriesControl::PreviousMaps => setup.map_top > 0,
                AiSeriesControl::NextMaps => setup.map_top + layout.maps.len() < setup.maps.len(),
                _ => true,
            };
            let pressed = setup.buttons.is_pressed(control);
            let highlighted =
                setup.focused_control == control || setup.buttons.hovered == Some(control);
            let list_row = matches!(
                control,
                AiSeriesControl::MapRow(_) | AiSeriesControl::Randomize
            );
            let label_rect = if list_row {
                let selected = match control {
                    AiSeriesControl::MapRow(row) => setup
                        .maps
                        .get(setup.map_top + row)
                        .is_some_and(|map| setup.selected(map)),
                    AiSeriesControl::Randomize => setup.randomize_each_round,
                    _ => false,
                };
                let border = if highlighted {
                    [0.68, 0.64, 0.22]
                } else if selected {
                    [0.40, 0.42, 0.18]
                } else {
                    [0.13, 0.18, 0.13]
                };
                let background = if pressed {
                    [0.13, 0.12, 0.04]
                } else if selected {
                    [0.055, 0.075, 0.038]
                } else {
                    [0.025, 0.035, 0.027]
                };
                fill(&mut backgrounds, pixel, rect, border);
                fill(
                    &mut backgrounds,
                    pixel,
                    RectPx::new(rect.x + 1, rect.y + 1, rect.w - 2, rect.h - 2),
                    background,
                );
                RectPx::new(
                    rect.x + 8 + i32::from(pressed),
                    rect.y + i32::from(pressed),
                    rect.w - 16,
                    rect.h - 1,
                )
            } else {
                buttons.push(PaintButton {
                    rect,
                    pressed,
                    hovered: highlighted,
                    enabled,
                });
                owner_draw_button_label_rect(rect, pressed)
            };
            let mut caption = label(caption, label_rect, !list_row);
            if !enabled {
                caption.rgb = [0.40, 0.40, 0.30];
            }
            labels.push(caption);
        }
        labels.push(label(
            format!(
                "Page {} / {}",
                setup.map_top / layout.maps.len().max(1) + 1,
                setup.page_count(layout.maps.len())
            ),
            layout.map_page,
            true,
        ));
        if let Some(rect) = layout.first_seed_label {
            labels.push(label("First match seed", rect, false));
        }
        if let Some(rect) = layout.custom_seeds_label {
            labels.push(label(
                "Custom seeds (optional: one per match, commas or spaces)",
                rect,
                false,
            ));
        }
        for (rect, edit) in [
            (Some(layout.name), &setup.name),
            (Some(layout.count), &setup.match_count),
            (layout.first_seed, &setup.first_seed),
            (layout.custom_seeds, &setup.custom_seeds),
        ] {
            let Some(rect) = rect else {
                continue;
            };
            fill(
                &mut backgrounds,
                pixel,
                rect,
                if edit.focused {
                    [0.50, 0.46, 0.12]
                } else {
                    [0.19, 0.22, 0.16]
                },
            );
            let field = RectPx::new(rect.x + 2, rect.y + 2, rect.w - 4, rect.h - 4);
            fill(&mut backgrounds, pixel, field, [0.01, 0.018, 0.012]);
            let start = edit
                .first_visible_unit
                .min(edit.caret)
                .min(edit.units.len());
            let value = if edit.focused {
                format!(
                    "{}|{}",
                    String::from_utf16_lossy(&edit.units[start..edit.caret]),
                    String::from_utf16_lossy(&edit.units[edit.caret..])
                )
            } else {
                String::from_utf16_lossy(&edit.units[start..])
            };
            labels.push(label(
                value,
                RectPx::new(field.x + 5, field.y, field.w - 10, field.h),
                false,
            ));
        }
        let status = setup.error.clone().unwrap_or_else(|| {
            if setup.maps.is_empty() {
                "No map has enough starting positions for these AIs.".into()
            } else if setup
                .custom_seeds
                .units
                .iter()
                .any(|unit| !matches!(*unit, 9 | 10 | 13 | 32))
            {
                "Maps repeat in selection order; using your custom seed list.".into()
            } else {
                "Maps repeat in selection order; match seeds increase by one.".into()
            }
        });
        labels.push(label(status, layout.status, false));
    } else {
        let status = crate::app::ai_series::status_text(state);
        let action_count = crate::app::ai_series::score_actions(state).len();
        let button_top = card_action_rect(state, action_count.saturating_sub(1)).y;
        let status_top = layout.title.y + 42;
        let visible_lines = ((button_top - status_top - 12) / 25).max(1) as usize;
        let wrapped = state
            .renderer
            .bit_font
            .wrap_layout(&status, (layout.panel.w - 40).max(1) as u32);
        for (index, line) in wrapped.lines.iter().take(visible_lines).enumerate() {
            labels.push(label(
                &status[line.start_byte..line.end_byte],
                RectPx::new(
                    layout.panel.x + 20,
                    layout.title.y + 42 + index as i32 * 25,
                    layout.panel.w - 40,
                    25,
                ),
                false,
            ));
        }
        for (index, (id, caption)) in crate::app::ai_series::score_actions(state)
            .into_iter()
            .enumerate()
        {
            let rect = card_action_rect(state, index);
            let pressed = state.frontend.ai_series_dialog_buttons.is_pressed(id);
            buttons.push(PaintButton {
                rect,
                pressed,
                hovered: state.frontend.ai_series_dialog_buttons.hovered == Some(id),
                enabled: true,
            });
            labels.push(label(
                caption,
                owner_draw_button_label_rect(rect, pressed),
                true,
            ));
        }
    }
    let art = shell_paint::paint_buttons(
        chrome,
        &buttons,
        crate::app::frontend::menu_page_render::MENU_PAGE_BUTTON_POLICY,
        crate::util::time::Instant::now(),
        None,
    );
    let text = shell_paint::paint_labels(&state.renderer.bit_font, &labels);
    let draws = [
        TexturedDraw {
            texture: &chrome.texture,
            instances: backgrounds,
        },
        TexturedDraw {
            texture: &chrome.texture,
            instances: art,
        },
    ];
    encode_shell_pass(
        state,
        encoder,
        destination,
        "AI Series Setup",
        ShellComposition {
            draws: &draws,
            text: &text,
            cursor: software_cursor(state, CURSOR_DEPTH),
            effects: None,
        },
    );
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_result_actions_preserve_native_paging_and_shell_chrome() {
        use crate::ui::score_shell::{NEXT_PAGE, PREVIOUS_PAGE, VIEW_BATTLEFIELD, compute_layout};

        fn overlaps(a: RectPx, b: RectPx) -> bool {
            a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
        }

        for (width, height) in [(640, 480), (800, 600)] {
            let layout = compute_layout(width, height);
            let anchor = layout.continue_button();
            for actions in [
                &[0xF210, 0xF211, 0xF212, 0xF215][..],
                &[0xF213, 0xF214, 0xF212][..],
            ] {
                let controls: Vec<_> = actions
                    .iter()
                    .map(|id| score_action_rect_from_anchor(anchor, *id).unwrap())
                    .chain([
                        layout.result_action_button(VIEW_BATTLEFIELD).unwrap(),
                        layout.paging_button(PREVIOUS_PAGE).unwrap(),
                        layout.paging_button(NEXT_PAGE).unwrap(),
                        crate::app::frontend::score_shell_render::score_page_indicator_rect(
                            &layout,
                        ),
                    ])
                    .collect();
                for (index, rect) in controls.iter().enumerate() {
                    assert!(
                        rect.x >= 0
                            && rect.y >= 0
                            && rect.x + rect.w <= width as i32
                            && rect.y + rect.h <= height as i32,
                        "{width}x{height}: {rect:?}"
                    );
                    assert!(
                        !overlaps(*rect, layout.page.title),
                        "action overlaps title: {rect:?}"
                    );
                    assert!(
                        !overlaps(*rect, layout.page.warning_monitor),
                        "action overlaps monitor: {rect:?}"
                    );
                    for other in controls.iter().skip(index + 1) {
                        assert!(
                            !overlaps(*rect, *other),
                            "result controls overlap: {rect:?}, {other:?}"
                        );
                    }
                }
            }
        }
    }
}
