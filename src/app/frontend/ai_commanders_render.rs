//! AI commander setup through the shared shell composition.

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
use crate::ui::ai_commanders::{
    AiCommanderControl, AiCommanderSetupEffect, AiCommanderSetupLayout,
};
use crate::ui::shell::geom::RectPx;
use winit::keyboard::KeyCode;

pub(crate) const ENTRY: u16 = 0xF230;

pub(crate) fn layout(state: &AppState) -> AiCommanderSetupLayout {
    AiCommanderSetupLayout::new(state.render_width(), state.render_height())
}

pub(crate) fn entry_rect(state: &AppState) -> Option<RectPx> {
    if !crate::app::ai_commanders::setup_available(state)
        || state.frontend.skirmish_shell_state.top_dialog()
            != Some(crate::ui::skirmish_shell::SkirmishShellDialog::Skirmish)
        || state.frontend.ai_series_setup.is_some()
        || dialog_active(state)
    {
        return None;
    }
    let anchor = crate::app::frontend::shell_layout::skirmish_layout(state).choose_map_button;
    Some(RectPx::new(
        anchor.x,
        anchor.y + 2 * (anchor.h + 8),
        anchor.w,
        anchor.h.max(28),
    ))
}

pub(crate) fn dialog_active(state: &AppState) -> bool {
    state.frontend.ai_commanders_setup.is_some()
}

pub(crate) fn mouse_down(state: &mut AppState, point: (i32, i32)) -> AiCommanderSetupEffect {
    let layout = layout(state);
    if let Some(setup) = state.frontend.ai_commanders_setup.as_mut() {
        setup.align_page(layout.rows.len());
        let hit = layout.control_at(setup, point);
        setup.buttons.press(hit);
        if let Some(control) = hit {
            setup.focused_control = control;
        }
    }
    AiCommanderSetupEffect::None
}

pub(crate) fn mouse_move(state: &mut AppState, point: (i32, i32)) -> bool {
    let layout = layout(state);
    if let Some(setup) = state.frontend.ai_commanders_setup.as_mut() {
        setup.align_page(layout.rows.len());
        setup.buttons.hovered = layout.control_at(setup, point);
    }
    true
}

pub(crate) fn mouse_up(state: &mut AppState, point: (i32, i32)) -> AiCommanderSetupEffect {
    let layout = layout(state);
    if let Some(setup) = state.frontend.ai_commanders_setup.as_mut() {
        setup.align_page(layout.rows.len());
        let hit = layout.control_at(setup, point);
        return setup
            .buttons
            .release(hit)
            .map_or(AiCommanderSetupEffect::None, |control| {
                setup.activate(control, layout.rows.len())
            });
    }
    AiCommanderSetupEffect::None
}

pub(crate) fn key(
    state: &mut AppState,
    code: Option<KeyCode>,
    shift: bool,
) -> AiCommanderSetupEffect {
    let rows = layout(state).rows.len();
    state
        .frontend
        .ai_commanders_setup
        .as_mut()
        .map_or(AiCommanderSetupEffect::None, |setup| {
            setup.key(code, shift, rows)
        })
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

fn wrapped_labels(
    state: &AppState,
    labels: &mut Vec<PaintLabel<'static>>,
    text: &str,
    rect: RectPx,
) {
    let wrapped = state
        .renderer
        .bit_font
        .wrap_layout(text, rect.w.max(1) as u32);
    let line_height = 18;
    for (index, line) in wrapped
        .lines
        .iter()
        .take((rect.h / line_height).max(1) as usize)
        .enumerate()
    {
        labels.push(label(
            text[line.start_byte..line.end_byte].to_owned(),
            RectPx::new(
                rect.x,
                rect.y + index as i32 * line_height,
                rect.w,
                line_height,
            ),
            false,
        ));
    }
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

pub(crate) fn render_ai_commanders_setup(
    state: &mut AppState,
    encoder: &mut wgpu::CommandEncoder,
    destination: &wgpu::Texture,
) -> anyhow::Result<bool> {
    if !dialog_active(state) {
        return Ok(false);
    }
    let layout = layout(state);
    if let Some(setup) = state.frontend.ai_commanders_setup.as_mut() {
        setup.align_page(layout.rows.len());
    }
    let Some(chrome) = state.frontend.main_menu_shell_chrome.as_ref() else {
        return Ok(false);
    };
    let Some(pixel) = chrome.white_pixel else {
        return Ok(false);
    };
    let Some(setup) = state.frontend.ai_commanders_setup.as_ref() else {
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
        localized(state, "VERA:AICommandersTitle", "AI Commanders"),
        layout.title,
        true,
    )];
    let summary = setup.choosing().map_or_else(
        || {
            localized(
                state,
                "VERA:AICommandersHelp",
                "Choose a commander for each AI. Standard keeps ordinary AI behavior.",
            )
        },
        |row| {
            format!(
                "{} / {} / {} — choose a compatible commander",
                row.label, row.country, row.difficulty
            )
        },
    );
    wrapped_labels(state, &mut labels, &summary, layout.summary);
    let mut controls = Vec::new();
    for visible in 0..layout.rows.len() {
        let (control, caption, description) = if let Some(row) = setup.choosing() {
            let Some(choice) = row.choices.get(setup.choice_top + visible) else {
                continue;
            };
            let selected = choice.name.eq_ignore_ascii_case(&row.selected);
            (
                AiCommanderControl::Choice(visible),
                format!("[{}] {}", if selected { "x" } else { " " }, choice.label),
                choice.description.clone(),
            )
        } else {
            let Some(row) = setup.rows.get(setup.page_top + visible) else {
                continue;
            };
            let choice = row.selected_choice();
            (
                AiCommanderControl::Row(visible),
                format!(
                    "{} / {} / {}\n{}: {} >",
                    row.label,
                    row.country,
                    row.difficulty,
                    localized(state, "VERA:Commander", "Commander"),
                    choice.map_or(row.selected.as_str(), |choice| choice.label.as_str())
                ),
                choice
                    .map_or("Standard AI behavior.", |choice| {
                        choice.description.as_str()
                    })
                    .to_owned(),
            )
        };
        controls.push((control, caption));
        if let Some(rect) = layout.description_rect(visible) {
            wrapped_labels(state, &mut labels, &description, rect);
        }
    }
    controls.extend([
        (
            AiCommanderControl::Previous,
            localized(state, "VERA:Previous", "Previous"),
        ),
        (
            AiCommanderControl::Next,
            localized(state, "VERA:Next", "Next"),
        ),
    ]);
    if setup.choosing_row.is_some() {
        controls.push((
            AiCommanderControl::Back,
            localized(state, "VERA:Back", "Back"),
        ));
    } else {
        controls.extend([
            (
                AiCommanderControl::AllStandard,
                localized(state, "VERA:AICommandersAllStandard", "All: Standard"),
            ),
            (
                AiCommanderControl::AllRandom,
                localized(
                    state,
                    "VERA:AICommandersAllRandom",
                    "All: Random Compatible",
                ),
            ),
            (
                AiCommanderControl::Feedback,
                format!(
                    "[{}] {}",
                    if setup.feedback_enabled { "x" } else { " " },
                    localized(
                        state,
                        "VERA:CommanderFeedback",
                        "Commander feedback audio/video"
                    )
                ),
            ),
            (
                AiCommanderControl::Apply,
                localized(state, "VERA:Apply", "Apply"),
            ),
            (
                AiCommanderControl::Cancel,
                localized(state, "VERA:Cancel", "Cancel"),
            ),
        ]);
    }
    let mut buttons = Vec::new();
    for (control, caption) in controls {
        let Some(rect) = layout.control_rect(control) else {
            continue;
        };
        let pressed = setup.buttons.is_pressed(control);
        let enabled = setup.enabled(control, layout.rows.len());
        buttons.push(PaintButton {
            rect,
            pressed,
            enabled,
            hovered: setup.focused_control == control || setup.buttons.hovered == Some(control),
        });
        let mut caption = label(caption, owner_draw_button_label_rect(rect, pressed), true);
        if !enabled {
            caption.rgb = [0.4, 0.4, 0.3];
        }
        labels.push(caption);
    }
    labels.push(label(
        format!(
            "Page {} / {}",
            setup.list_top() / layout.rows.len().max(1) + 1,
            setup.list_len().div_ceil(layout.rows.len().max(1)).max(1)
        ),
        layout.page,
        true,
    ));
    let status = setup.notice.clone().unwrap_or_else(|| {
        if !setup.has_custom_choices() {
            localized(
                state,
                "VERA:AICommandersUnavailable",
                "No custom commanders available with these game rules.",
            )
        } else if setup.rows.is_empty() {
            localized(
                state,
                "VERA:AICommandersNoAI",
                "Add a computer opponent in Skirmish setup first.",
            )
        } else {
            localized(
                state,
                "VERA:AICommandersFeedbackHelp",
                "Muting feedback keeps each commander's strategy active.",
            )
        }
    });
    wrapped_labels(state, &mut labels, &status, layout.status);
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
        "AI Commander Setup",
        ShellComposition {
            draws: &draws,
            text: &text,
            cursor: software_cursor(state, CURSOR_DEPTH),
            effects: None,
        },
    );
    Ok(true)
}
