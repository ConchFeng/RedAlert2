//! Floating spectator-view chrome; the tactical image has its own GPU target.

use crate::app::AppState;
use crate::render::batch::SpriteInstance;
use crate::ui::gadget::GadgetRect;
use crate::ui::second_view::{FollowMode, SecondViewControl, SecondViewLayout};

use super::spectator_tools::localized;

fn caption(state: &AppState, control: SecondViewControl) -> String {
    use SecondViewControl::*;
    let view = &state.match_state.match_presentation.second_view;
    match control {
        Close => localized(state, "VERA:SecondViewClose", "Close"),
        Minimize if view.minimized => localized(state, "VERA:SecondViewRestore", "Restore"),
        Minimize => localized(state, "VERA:SecondViewMinimize", "Minimize"),
        SwapViews => localized(state, "VERA:SecondViewSwap", "Swap Views"),
        Mode => match view.mode {
            FollowMode::Fixed => localized(state, "VERA:SecondViewFixed", "View: Fixed"),
            FollowMode::Player(_) => localized(state, "VERA:SecondViewPlayer", "View: Player"),
            FollowMode::Action => localized(state, "VERA:SecondViewAction", "View: Action"),
        },
        PreviousPlayer => "<".to_owned(),
        NextPlayer => ">".to_owned(),
        ZoomOut => "-".to_owned(),
        ZoomIn => "+".to_owned(),
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
    let scale = ((rect.w - 12).max(1) as f32 / width)
        .min((rect.h - 4).max(1) as f32 / font.cell_height())
        .min(1.0);
    let x = if centered {
        rect.x as f32 + (rect.w as f32 - width * scale) / 2.0
    } else {
        rect.x as f32 + 6.0
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

pub(crate) fn build_instances(
    state: &AppState,
    layout: &SecondViewLayout,
    player_label: &str,
    status: Option<&str>,
    texture_ready: bool,
) -> (Vec<SpriteInstance>, Vec<SpriteInstance>) {
    let view = &state.match_state.match_presentation.second_view;
    let camera = [
        state.match_state.input.camera_x,
        state.match_state.input.camera_y,
    ];
    let fill = |rect: GadgetRect, tint: [f32; 3]| SpriteInstance {
        position: [rect.x as f32 + camera[0], rect.y as f32 + camera[1]],
        size: [rect.w.max(0) as f32, rect.h.max(0) as f32],
        uv_origin: [0.0, 0.0],
        uv_size: [1.0, 1.0],
        tint,
        alpha: 1.0,
        ..Default::default()
    };
    let mut fills = Vec::new();
    let mut texts = Vec::new();
    let panel = layout.panel;
    let panel_color = [0.035, 0.045, 0.055];
    if let Some(content) = layout.content {
        for rect in [
            GadgetRect::new(panel.x, panel.y, panel.w, content.y - panel.y),
            GadgetRect::new(panel.x, content.y, content.x - panel.x, content.h),
            GadgetRect::new(
                content.x + content.w,
                content.y,
                panel.x + panel.w - content.x - content.w,
                content.h,
            ),
            GadgetRect::new(
                panel.x,
                content.y + content.h,
                panel.w,
                panel.y + panel.h - content.y - content.h,
            ),
        ] {
            fills.push(fill(rect, panel_color));
        }
        if !texture_ready {
            fills.push(fill(content, [0.015, 0.02, 0.025]));
            text(
                state,
                &mut texts,
                &localized(
                    state,
                    "VERA:SecondViewPreparing",
                    "Preparing second view...",
                ),
                content,
                true,
                [0.9, 0.94, 0.96],
            );
        }
    } else {
        fills.push(fill(panel, panel_color));
    }
    for rect in [
        GadgetRect::new(panel.x, panel.y, panel.w, 1),
        GadgetRect::new(panel.x, panel.y + panel.h - 1, panel.w, 1),
        GadgetRect::new(panel.x, panel.y, 1, panel.h),
        GadgetRect::new(panel.x + panel.w - 1, panel.y, 1, panel.h),
    ] {
        fills.push(fill(rect, [0.32, 0.38, 0.42]));
    }
    text(
        state,
        &mut texts,
        &localized(state, "VERA:SecondViewTitle", "Second View"),
        layout.title,
        false,
        [0.9, 0.94, 0.96],
    );
    for (control, rect) in &layout.controls {
        let highlighted = view.hovered(*control) || view.focused(*control);
        fills.push(fill(
            *rect,
            if highlighted {
                [0.75, 0.8, 0.4]
            } else {
                [0.32, 0.38, 0.42]
            },
        ));
        fills.push(fill(
            GadgetRect::new(rect.x + 1, rect.y + 1, rect.w - 2, rect.h - 2),
            if view.pressed(*control) {
                [0.12, 0.17, 0.2]
            } else {
                [0.18, 0.24, 0.28]
            },
        ));
        text(
            state,
            &mut texts,
            &caption(state, *control),
            *rect,
            true,
            [1.0, 0.96, 0.76],
        );
    }
    if let Some(rect) = layout.player_label {
        let label = match view.mode {
            FollowMode::Fixed => localized(state, "VERA:SecondViewLocation", "Fixed location"),
            FollowMode::Action => {
                localized(state, "VERA:SecondViewFollowingAction", "Following action")
            }
            FollowMode::Player(_) if player_label.is_empty() => {
                localized(state, "VERA:SecondViewChoosePlayer", "Choose player")
            }
            FollowMode::Player(_) => player_label.to_owned(),
        };
        text(state, &mut texts, &label, rect, true, [0.9, 0.94, 0.96]);
    }
    if let Some(rect) = layout.status {
        let default_status = format!("{:.0}% | 15 FPS maximum", view.camera.zoom * 100.0);
        text(
            state,
            &mut texts,
            status.unwrap_or(&default_status),
            rect,
            false,
            [0.9, 0.94, 0.96],
        );
    }
    if let Some(rect) = layout.resize {
        for step in 0..3 {
            fills.push(fill(
                GadgetRect::new(
                    rect.x + 9 + step * 4,
                    rect.y + 21 - step * 4,
                    16 - step * 4,
                    2,
                ),
                [0.65, 0.7, 0.74],
            ));
        }
    }
    (fills, texts)
}
