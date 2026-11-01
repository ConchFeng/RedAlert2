//! Production pointer transactions and GPU observations for the spectator panel.

use super::{SkirmishCapture, *};
use crate::app::input::camera;
use crate::app::presentation::{second_view, spectator_tools::SpectatorToolAction};
use crate::ui::gadget::GadgetRect;
use crate::ui::second_view::{CameraView, FollowMode, SecondViewControl};
use winit::event::{ElementState, MouseButton};

#[derive(Default)]
pub(super) struct SecondViewCapture {
    stage: u8,
    samples: u32,
    last_render: u64,
    cpu_us: u64,
    started: Option<Instant>,
    minimized_frames: u32,
}

fn center(rect: GadgetRect) -> (i32, i32) {
    (rect.x + rect.w / 2, rect.y + rect.h / 2)
}

fn button(state: &mut AppState, control: SecondViewControl) -> Result<()> {
    let rect = second_view::layout(state)
        .and_then(|layout| layout.control_rect(control))
        .with_context(|| format!("second view control {control:?} is unavailable"))?;
    SkirmishCapture::spectator_pointer_input(state, center(rect), MouseButton::Left)
}

fn drag(state: &mut AppState, from: (i32, i32), to: (i32, i32)) -> Result<()> {
    let identity = SkirmishCapture::simulation_identity(state)?;
    SkirmishCapture::set_pointer(state, from);
    crate::app::input::dispatch::handle_cursor_moved_in_game(state);
    crate::app::input::dispatch::handle_mouse_input(
        state,
        MouseButton::Left,
        ElementState::Pressed,
    );
    SkirmishCapture::set_pointer(state, to);
    crate::app::input::dispatch::handle_cursor_moved_in_game(state);
    crate::app::input::dispatch::handle_mouse_input(
        state,
        MouseButton::Left,
        ElementState::Released,
    );
    SkirmishCapture::set_pointer(state, (EXPECTED_CURSOR_X as i32, EXPECTED_CURSOR_Y as i32));
    crate::app::input::dispatch::handle_cursor_moved_in_game(state);
    ensure!(
        identity == SkirmishCapture::simulation_identity(state)?,
        "panel drag changed simulation"
    );
    Ok(())
}

fn independent_player_view(state: &mut AppState) -> Result<()> {
    let main = camera::current_view(state);
    button(state, SecondViewControl::Mode)?;
    for _ in 0..29 {
        let second = state.match_state.match_presentation.second_view.camera;
        if (main.center[0] - second.center[0]).abs() + (main.center[1] - second.center[1]).abs()
            > 512.0
        {
            break;
        }
        button(state, SecondViewControl::NextPlayer)?;
    }
    let second = state.match_state.match_presentation.second_view.camera;
    ensure!(
        camera::current_view(state) == main,
        "player follow moved the main view"
    );
    ensure!(
        (main.center[0] - second.center[0]).abs() + (main.center[1] - second.center[1]).abs()
            > 512.0,
        "no distinct AI base found for second view"
    );
    button(state, SecondViewControl::Mode)?;
    button(state, SecondViewControl::Mode)?;
    ensure!(
        state.match_state.match_presentation.second_view.mode == FollowMode::Fixed,
        "fixed view was not restored"
    );
    Ok(())
}

impl SecondViewCapture {
    pub(super) fn advance(
        &mut self,
        state: &mut AppState,
        frame: u32,
        route: &mut Vec<Value>,
    ) -> Result<bool> {
        let tick = SkirmishCapture::simulation_identity(state)?.0;
        if tick < 3_000 {
            return Ok(false);
        }
        match self.stage {
            0 => {
                let identity = SkirmishCapture::simulation_identity(state)?;
                ensure!(
                    !state.match_state.match_presentation.second_view.is_open(),
                    "second view started open"
                );
                SkirmishCapture::spectator_button(state, SpectatorToolAction::ToggleMore)?;
                SkirmishCapture::spectator_button(state, SpectatorToolAction::OpenSecondView)?;
                let initial_main = camera::current_view(state);
                independent_player_view(state)?;
                button(state, SecondViewControl::ZoomIn)?;
                ensure!(
                    camera::current_view(state) == initial_main,
                    "second zoom moved main view"
                );
                let incoming = state.match_state.match_presentation.second_view.camera;
                button(state, SecondViewControl::SwapViews)?;
                let main = camera::current_view(state);
                let second = state.match_state.match_presentation.second_view.camera;
                ensure!(
                    main.zoom == incoming.zoom && second.zoom == initial_main.zoom,
                    "swap did not exchange zooms"
                );
                let extent = camera::tactical_viewport_size(state);
                let expected = camera::clamp_view_center(
                    state,
                    incoming.center,
                    incoming.zoom,
                    [extent.0 as f32, extent.1 as f32],
                );
                ensure!(
                    (main.center[0] - expected[0]).abs() < 1.0
                        && (main.center[1] - expected[1]).abs() < 1.0,
                    "swap did not move through camera bounds owner"
                );
                let viewport = second_view::viewport(state);
                let title = second_view::layout(state)
                    .context("second view layout missing")?
                    .title;
                drag(state, center(title), (viewport.x - 1000, viewport.y - 1000))?;
                let panel = second_view::layout(state)
                    .context("dragged panel missing")?
                    .panel;
                ensure!(
                    panel.x == viewport.x && panel.y == viewport.y,
                    "drag escaped tactical bounds"
                );
                let handle = second_view::layout(state)
                    .and_then(|layout| layout.resize)
                    .context("resize handle missing")?;
                drag(
                    state,
                    center(handle),
                    (
                        viewport.x + viewport.w + 1000,
                        viewport.y + viewport.h + 1000,
                    ),
                )?;
                let panel = second_view::layout(state)
                    .context("resized panel missing")?
                    .panel;
                ensure!(
                    panel.w <= viewport.w && panel.h <= viewport.h,
                    "resize escaped tactical bounds"
                );
                let handle = second_view::layout(state)
                    .and_then(|layout| layout.resize)
                    .context("resize handle missing")?;
                let desired = (360.min(viewport.w), 300.min(viewport.h));
                drag(
                    state,
                    center(handle),
                    (
                        center(handle).0 + desired.0 - panel.w,
                        center(handle).1 + desired.1 - panel.h,
                    ),
                )?;
                button(state, SecondViewControl::Close)?;
                ensure!(
                    !state.match_state.match_presentation.second_view.is_open(),
                    "Close kept panel open"
                );
                SkirmishCapture::spectator_button(state, SpectatorToolAction::ToggleMore)?;
                SkirmishCapture::spectator_button(state, SpectatorToolAction::OpenSecondView)?;
                independent_player_view(state)?;
                ensure!(
                    identity == SkirmishCapture::simulation_identity(state)?,
                    "second view controls changed simulation tick/frame/hash"
                );
                route.push(json!({"screen":"in game", "frame":frame, "tick":tick, "action":"SecondViewInputTransactions", "main_before":initial_main.center, "swapped_main":main.center, "main_zoom":main.zoom, "second_zoom":second.zoom, "drag_resize_clamped":true, "closed_reopened":true, "simulation_unchanged":true}));
                self.started = Some(Instant::now());
                self.last_render = state
                    .renderer
                    .second_view
                    .as_ref()
                    .map_or(0, |renderer| renderer.observation().renders);
                self.stage = 1;
            }
            1 => {
                let Some(renderer) = state.renderer.second_view.as_ref() else {
                    return Ok(false);
                };
                let observation = renderer.observation();
                if observation.renders == self.last_render {
                    return Ok(false);
                }
                self.last_render = observation.renders;
                self.samples += 1;
                self.cpu_us += observation.cpu_us;
                if self.samples < 20 {
                    return Ok(false);
                }
                let elapsed = self
                    .started
                    .context("second view sampling clock missing")?
                    .elapsed();
                ensure!(
                    u128::from(self.samples) <= elapsed.as_millis() * 15 / 1000 + 2,
                    "second view exceeded its 15 FPS refresh cap"
                );
                ensure!(
                    observation.simulation_checks >= 20 && observation.simulation_changes == 0,
                    "second-view renders changed the simulation hash"
                );
                let view = observation
                    .view
                    .context("second view never recorded its rendered camera")?;
                route.push(json!({"screen":"in game", "frame":frame, "tick":tick, "action":"SecondView29AiRenderProfile", "samples":self.samples, "elapsed_ms":elapsed.as_millis(), "second_view_cpu_mean_us":self.cpu_us / u64::from(self.samples), "second_view_cpu_max_us":observation.max_cpu_us, "camera":view.camera, "size":view.size, "zoom":view.zoom, "gpu_draw_duration_measured":observation.gpu_samples > 0, "gpu_samples":observation.gpu_samples, "render_hash_checks":observation.simulation_checks, "render_hash_changes":observation.simulation_changes, "gpu_mean_us":(observation.gpu_samples > 0).then(|| observation.gpu_total_us / observation.gpu_samples), "gpu_max_us":observation.gpu_max_us}));
                button(state, SecondViewControl::Minimize)?;
                ensure!(
                    second_view::layout(state).is_some_and(|layout| layout.content.is_none()),
                    "minimized panel retained scene content"
                );
                self.stage = 2;
            }
            2 => {
                let renders = state
                    .renderer
                    .second_view
                    .as_ref()
                    .map_or(0, |renderer| renderer.observation().renders);
                ensure!(
                    renders == self.last_render,
                    "minimized second view rendered another scene"
                );
                self.minimized_frames += 1;
                if self.minimized_frames < 10 {
                    return Ok(false);
                }
                button(state, SecondViewControl::Minimize)?;
                self.stage = 3;
            }
            3 => {
                let Some(renderer) = state.renderer.second_view.as_ref() else {
                    return Ok(false);
                };
                let observation = renderer.observation();
                if observation.renders == self.last_render {
                    return Ok(false);
                }
                route.push(json!({"screen":"in game", "frame":frame, "tick":tick, "action":"SecondViewMinimizedAndRestored", "skipped_scene_frames":self.minimized_frames, "main":camera::current_view(state).center, "second":state.match_state.match_presentation.second_view.camera.center, "whole_map_revealed":true}));
                self.stage = 4;
            }
            _ => return Ok(true),
        }
        Ok(false)
    }
}

pub(super) fn validate_pixels(
    state: &AppState,
    pixels: &[u8],
    width: u32,
    height: u32,
    route: &mut Vec<Value>,
) -> Result<()> {
    let rect = second_view::layout(state)
        .and_then(|layout| layout.content)
        .context("final second view content missing")?;
    let renderer = state
        .renderer
        .second_view
        .as_ref()
        .context("final second view renderer missing")?;
    ensure!(
        renderer.ready(),
        "final second view texture is uninitialized"
    );
    let source = renderer.texture();
    ensure!(
        source.width() == rect.w as u32 && source.height() == rect.h as u32,
        "second view texture does not match content size"
    );
    ensure!(
        rect.x >= 0
            && rect.y >= 0
            && rect.x + rect.w <= width as i32
            && rect.y + rect.h <= height as i32,
        "second view lies outside capture"
    );
    let gpu = &state.renderer.gpu;
    let started = Instant::now();
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    let readback = crate::render::frame_readback::PendingBgra8Readback::encode(
        &gpu.device,
        &mut encoder,
        source,
        gpu.surface_format,
        source.width(),
        source.height(),
    )?;
    let submission = gpu.queue.submit([encoder.finish()]);
    let texture_pixels = readback.finish(&gpu.device, submission, Duration::from_secs(15))?;
    let mut samples = 0;
    let mut matching = 0;
    let mut colors = std::collections::HashSet::new();
    for y in (8..rect.h - 8).step_by(7) {
        for x in (8..rect.w - 8).step_by(7) {
            let target = (((rect.y + y) as u32 * width + (rect.x + x) as u32) * 4) as usize;
            let source = ((y as u32 * rect.w as u32 + x as u32) * 4) as usize;
            let expected = &texture_pixels[source..source + 3];
            let actual = &pixels[target..target + 3];
            colors.insert([actual[0], actual[1], actual[2]]);
            matching += u32::from(
                expected
                    .iter()
                    .zip(actual)
                    .all(|(a, b)| a.abs_diff(*b) <= 12),
            );
            samples += 1;
        }
    }
    ensure!(
        samples > 0 && matching * 100 >= samples * 98,
        "second view texture did not composite into its drawn content rectangle: {matching}/{samples}"
    );
    ensure!(
        colors.len() > 8,
        "second view contains no meaningful battlefield image"
    );
    let main: CameraView = camera::current_view(state);
    let second = state.match_state.match_presentation.second_view.camera;
    ensure!(
        (main.center[0] - second.center[0]).abs() + (main.center[1] - second.center[1]).abs()
            > 256.0,
        "final cameras are not spatially distinct"
    );
    let observation = renderer.observation();
    route.push(json!({"action":"SecondViewGpuReadback", "content":[rect.x,rect.y,rect.w,rect.h], "sampled_pixels":samples, "matching_pixels":matching, "rgb565_tolerance":12, "distinct_colors":colors.len(), "gpu_readback_sync_wall_us":started.elapsed().as_micros(), "main_center":main.center, "second_center":second.center, "gpu_draw_duration_measured":observation.gpu_samples > 0, "gpu_samples":observation.gpu_samples, "render_hash_checks":observation.simulation_checks, "render_hash_changes":observation.simulation_changes, "gpu_mean_us":(observation.gpu_samples > 0).then(|| observation.gpu_total_us / observation.gpu_samples), "gpu_max_us":observation.gpu_max_us}));
    Ok(())
}
