//! Independent spectator projection sharing the world's atlases and draw pipeline.

use std::cell::RefCell;

use crate::app::AppState;
use crate::app::match_runtime::frame_pacer::wall_clock_ms;
use crate::render::batch::{BatchRenderer, InstanceBufferPool};
use crate::render::combat_light::CombatLightRenderer;
use crate::render::scaled_copy::ScaledSurface;
use crate::render::shroud_buffer::ShroudBuffer;
use crate::render::terrain_draw::TerrainDrawRenderer;
use crate::render::unit_pose_cache::VxlPoseFrameCache;
use crate::render::upscale_pass::WorldZoomPass;
use crate::ui::gadget::GadgetRect;
use crate::util::time::Instant;

use super::draw_passes::{DrawPassData, WorldDrawContext, dispatch_world_draw_passes};
use super::{TacticalView, WorldViewResources, build_instances, upload_world};

const REFRESH_MS: u64 = 1_000_u64.div_ceil(15);

#[derive(Clone, Copy, Default, Debug, serde::Serialize)]
pub(crate) struct SecondViewObservation {
    pub renders: u64,
    #[serde(skip)]
    pub view: Option<TacticalView>,
    pub cpu_us: u64,
    pub max_cpu_us: u64,
    pub gpu_samples: u64,
    pub gpu_total_us: u64,
    pub gpu_max_us: u64,
    pub simulation_checks: u64,
    pub simulation_changes: u64,
}

pub(crate) struct SecondViewRenderer {
    batch: BatchRenderer,
    pool: InstanceBufferPool,
    terrain: TerrainDrawRenderer,
    lights: CombatLightRenderer,
    zoom: WorldZoomPass,
    display: ScaledSurface,
    display_binding: Option<wgpu::BindGroup>,
    shroud: Option<ShroudBuffer>,
    pose_cache: RefCell<VxlPoseFrameCache>,
    detail: RefCell<crate::app::presentation::detail::DetailState>,
    cell_bodies: Vec<crate::render::batch::SpriteInstance>,
    next_refresh_ms: u64,
    observation: SecondViewObservation,
    timing: Option<GpuTiming>,
}

impl SecondViewRenderer {
    fn new(state: &AppState, size: [u32; 2], zoom: f32) -> Self {
        let renderer = &state.renderer;
        let gpu = &renderer.gpu;
        let batch = renderer.batch_renderer.fork_view(gpu);
        let terrain = renderer
            .terrain_draw_renderer
            .fork_view(&gpu.device, &gpu.queue, &batch);
        Self {
            batch,
            terrain,
            pool: InstanceBufferPool::new(),
            lights: renderer.combat_light_renderer.fork_view(gpu),
            zoom: WorldZoomPass::new(&gpu.device, &gpu.queue, gpu.surface_format, size, zoom),
            display: ScaledSurface::new(
                &gpu.device,
                "Second battlefield",
                gpu.surface_format,
                (size[0], size[1]),
                Some(wgpu::TextureFormat::Depth32Float),
            ),
            display_binding: None,
            shroud: state
                .match_state
                .match_presentation
                .shroud_buffer
                .as_ref()
                .map(|shroud| shroud.fork_view(&gpu.device, &gpu.queue, size)),
            pose_cache: RefCell::new(renderer.vxl_pose_frame_cache.borrow().fork_view()),
            detail: RefCell::new(state.match_state.match_presentation.detail.borrow().clone()),
            cell_bodies: Vec::new(),
            next_refresh_ms: 0,
            observation: SecondViewObservation::default(),
            timing: GpuTiming::new(gpu),
        }
    }

    pub(crate) fn observation(&self) -> SecondViewObservation {
        self.observation
    }

    pub(crate) fn ready(&self) -> bool {
        self.display_binding.is_some()
    }

    pub(crate) fn texture(&self) -> &wgpu::Texture {
        self.zoom.color_texture()
    }

    fn render(&mut self, state: &AppState, encoder: &mut wgpu::CommandEncoder, view: TacticalView) {
        let hash_before = state.platform.capture_client_size.and_then(|_| {
            state
                .match_state
                .sim_runtime
                .as_ref()
                .map(|runtime| runtime.simulation.state_hash())
        });
        let started = Instant::now();
        let gpu = &state.renderer.gpu;
        if let Some(timing) = self.timing.as_mut() {
            timing.collect(&mut self.observation);
        }
        self.detail
            .replace(state.match_state.match_presentation.detail.borrow().clone());
        self.zoom
            .set_view(&gpu.device, &gpu.queue, view.size, view.zoom);
        self.batch.update_camera(
            gpu,
            view.size[0] as f32,
            view.size[1] as f32,
            view.camera[0],
            view.camera[1],
            view.zoom,
            crate::app::presentation::instances::depth_axis(state),
        );
        self.batch
            .set_native_world_resolution(&gpu.queue, self.zoom.source_size());
        self.batch.update_native_z_origin(&gpu.queue, 0.0);
        if let (Some(shroud), Some(runtime), Some(owner)) = (
            self.shroud.as_mut(),
            state.match_state.sim_runtime.as_ref(),
            crate::app::input::commands::preferred_local_owner_name(state),
        ) {
            let sim = runtime.view();
            let owner = sim.interner().get(&owner).unwrap_or_default();
            let cells = sim
                .resolved_terrain()
                .map(crate::map::resolved_terrain::NativeCellQuery::isolated);
            shroud.set_translucent_fog(&gpu.queue, state.translucent_fog_enabled());
            shroud.rebuild_if_needed(
                &gpu.device,
                &gpu.queue,
                sim.fog(),
                owner,
                view.camera[0],
                view.camera[1],
                view.size[0],
                view.size[1],
                view.zoom,
                cells.as_ref(),
                crate::render::shroud_buffer::FogDrawOptions {
                    fog_of_war: sim.session().game_options().fog_of_war,
                    current_house_defeated: sim
                        .session()
                        .current_house()
                        .and_then(|house| sim.houses().get(&house))
                        .is_some_and(|house| house.is_defeated()),
                },
            );
        }
        let shroud = (!state.match_state.sandbox_full_visibility)
            .then_some(self.shroud.as_ref())
            .flatten();
        self.batch.bind_shroud(&gpu.device, shroud);
        let resources = WorldViewResources {
            pose_cache: &self.pose_cache,
            barrel_image_pitches: &state.match_state.match_presentation.barrel_image_pitches,
            sinking_waterlines: &state.match_state.match_presentation.sinking_waterlines,
            detail: &self.detail,
        };
        let mut world = build_instances::build_world_instances_for_view(
            state,
            view,
            &resources,
            std::mem::take(&mut self.cell_bodies),
            None,
            false,
        );
        upload_world(gpu, &mut self.pool, &world);
        let mut records = Vec::new();
        for layer in &mut world.object_layers {
            layer.light_record_base = records.len();
            records.extend_from_slice(&layer.light_records);
        }
        let persistent_start = records.len();
        records.extend(
            state
                .match_state
                .match_presentation
                .combat_lights
                .draw_records(&mut self.detail.borrow_mut()),
        );
        if let Some(runtime) = state.match_state.sim_runtime.as_ref() {
            self.lights
                .set_spotlight_radius(&gpu.queue, runtime.resources.rules.general.spotlight.radius);
        }
        let camera = self.batch.camera_uniform();
        self.lights
            .prepare(gpu, &records, camera.screen_size, view.camera, camera.zoom);
        self.terrain.prepare(
            &gpu.device,
            self.lights.composition_texture(),
            self.zoom.depth_view(),
            camera,
        );
        self.terrain.begin_surface_lines();
        let viewport = view.surface_line_viewport();
        let mut alpha = |point: [i32; 2]| {
            shroud
                .and_then(|buffer| {
                    buffer.sample_world(
                        point[0] as f32 + view.camera[0].floor(),
                        point[1] as f32 + view.camera[1].floor(),
                        view.camera[0],
                        view.camera[1],
                    )
                })
                .map_or(127, u16::from)
        };
        let presentation = &state.match_state.match_presentation;
        let bolt_lines = presentation.electric_bolts.project(viewport);
        let beam_lines = presentation.radiation_beams.project(viewport);
        let surface_effects = self.terrain.prepare_surface_lines(
            &gpu.device,
            &gpu.queue,
            state.match_state.match_presentation.lasers.draws(),
            bolt_lines.iter().copied(),
            state.match_state.match_presentation.line_trails.segments(),
            &beam_lines,
            &[],
            viewport,
            |_| {
                state
                    .match_state
                    .match_presentation
                    .in_game_options
                    .detail_level
                    != 0
            },
            &mut alpha,
        );
        for layer in &mut world.object_layers {
            layer.line_batches = layer
                .surface_draws
                .iter()
                .map(|lines| {
                    self.terrain.prepare_surface_lines(
                        &gpu.device,
                        &gpu.queue,
                        std::iter::empty(),
                        lines.iter().copied(),
                        &[],
                        &[],
                        &[],
                        viewport,
                        |_| false,
                        &mut alpha,
                    )
                })
                .collect();
        }
        self.terrain.prepare_ion_blasts(
            &gpu.device,
            &gpu.queue,
            &crate::app::presentation::ion_blasts::draws_through(
                state,
                view.camera,
                [
                    view.size[0] as f32 / view.zoom,
                    view.size[1] as f32 / view.zoom,
                ],
            ),
            camera.zoom,
        );
        let composition = self.lights.composition_view();
        let timed = self.timing.as_ref().is_some_and(|timing| !timing.pending);
        if timed {
            self.timing
                .as_ref()
                .unwrap()
                .marker(encoder, &composition, 0);
        }
        dispatch_world_draw_passes(
            encoder,
            &composition,
            &DrawPassData {
                cell_body_draws: &world.cell_body_draws,
                bridge_shadows: &world.bridge_body_shadow,
                object_layers: &world.object_layers,
                fogged_objects: &world.fogged_objects,
                building_status_text: &world.building_status_text,
                surface_effects,
                tactical_lines: Default::default(),
                persistent_lights: persistent_start..records.len(),
                sidebar_unavailable: &[],
                sidebar_flash: &[],
                sidebar_gclock: &[],
            },
            &mut WorldDrawContext {
                presentation: &state.match_state.match_presentation,
                gpu,
                batch: &self.batch,
                pool: &self.pool,
                pose_cache: &self.pose_cache,
                terrain: &mut self.terrain,
                lights: &self.lights,
                font: &state.renderer.bit_font,
                depth: self.zoom.depth_view(),
                zoom: Some(&self.zoom),
                shroud,
                view,
            },
        );
        if timed {
            self.timing
                .as_mut()
                .unwrap()
                .finish(encoder, self.zoom.color_view());
        }
        self.cell_bodies = world.cell_bodies;
        self.display_binding = Some(
            self.display
                .bind_group_for(&gpu.device, self.zoom.color_texture()),
        );
        self.observation.renders += 1;
        self.observation.view = Some(view);
        self.observation.cpu_us = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        self.observation.max_cpu_us = self.observation.max_cpu_us.max(self.observation.cpu_us);
        if let (Some(before), Some(runtime)) = (hash_before, state.match_state.sim_runtime.as_ref())
        {
            self.observation.simulation_checks += 1;
            self.observation.simulation_changes +=
                u64::from(before != runtime.simulation.state_hash());
        }
    }
}

struct GpuTiming {
    queries: wgpu::QuerySet,
    resolved: wgpu::Buffer,
    read: wgpu::Buffer,
    completion: std::sync::mpsc::Receiver<bool>,
    sender: std::sync::mpsc::Sender<bool>,
    period_ns: f64,
    pending: bool,
}

impl GpuTiming {
    fn new(gpu: &crate::render::gpu::GpuContext) -> Option<Self> {
        let features = wgpu::Features::TIMESTAMP_QUERY;
        if !gpu.device.features().contains(features) {
            return None;
        }
        let queries = gpu.device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("Second view GPU timing"),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        });
        let buffer = |usage| {
            gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Second view timestamp pair"),
                size: 16,
                usage,
                mapped_at_creation: false,
            })
        };
        let (sender, completion) = std::sync::mpsc::channel();
        Some(Self {
            queries,
            resolved: buffer(wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC),
            read: buffer(wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ),
            completion,
            sender,
            period_ns: f64::from(gpu.queue.get_timestamp_period()),
            pending: false,
        })
    }

    fn collect(&mut self, observation: &mut SecondViewObservation) {
        if let Ok(mapped) = self.completion.try_recv() {
            if mapped && let Ok(bytes) = self.read.slice(..).get_mapped_range() {
                let start = u64::from_le_bytes(bytes[..8].try_into().unwrap());
                let end = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
                if end >= start {
                    let us = ((end - start) as f64 * self.period_ns / 1000.0) as u64;
                    observation.gpu_samples += 1;
                    observation.gpu_total_us = observation.gpu_total_us.saturating_add(us);
                    observation.gpu_max_us = observation.gpu_max_us.max(us);
                }
                drop(bytes);
            }
            self.read.unmap();
            self.pending = false;
        }
    }

    fn marker(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, index: u32) {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Second view timestamp boundary"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            timestamp_writes: Some(wgpu::RenderPassTimestampWrites {
                query_set: &self.queries,
                beginning_of_pass_write_index: (index == 0).then_some(0),
                end_of_pass_write_index: (index == 1).then_some(1),
            }),
            ..Default::default()
        });
    }

    fn finish(&mut self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        self.marker(encoder, view, 1);
        encoder.resolve_query_set(&self.queries, 0..2, &self.resolved, 0);
        encoder.copy_buffer_to_buffer(&self.resolved, 0, &self.read, 0, 16);
        let sender = self.sender.clone();
        encoder.map_buffer_on_submit(&self.read, wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result.is_ok());
        });
        self.pending = true;
    }
}

fn tactical_view(state: &AppState, content: GadgetRect) -> TacticalView {
    let camera = state.match_state.match_presentation.second_view.camera;
    let size = [content.w.max(1) as u32, content.h.max(1) as u32];
    TacticalView {
        camera: [
            camera.center[0] - size[0] as f32 / (2.0 * camera.zoom),
            camera.center[1] - size[1] as f32 / (2.0 * camera.zoom),
        ],
        size,
        zoom: camera.zoom,
        viewport: [0, 0, size[0], size[1]],
    }
}

pub(crate) fn prepare(state: &mut AppState, encoder: &mut wgpu::CommandEncoder) {
    let Some(layout) = crate::app::presentation::second_view::layout(state) else {
        return;
    };
    let size = layout
        .content
        .map_or([1, 1], |rect| [rect.w.max(1) as u32, rect.h.max(1) as u32]);
    let mut renderer = state.renderer.second_view.take().unwrap_or_else(|| {
        SecondViewRenderer::new(
            state,
            size,
            state.match_state.match_presentation.second_view.camera.zoom,
        )
    });
    if let Some(content) = layout.content {
        let now = wall_clock_ms();
        if now >= renderer.next_refresh_ms {
            renderer.render(state, encoder, tactical_view(state, content));
            renderer.next_refresh_ms = now.saturating_add(REFRESH_MS);
        }
        renderer.display.place(
            &state.renderer.gpu.queue,
            (content.x as f32, content.y as f32),
            1.0,
            true,
        );
    }
    let (fill, text) = crate::app::presentation::second_view_ui::build_instances(
        state,
        &layout,
        &crate::app::presentation::second_view::player_label(state),
        crate::app::presentation::second_view::status(state).as_deref(),
        renderer.ready(),
    );
    renderer
        .pool
        .upload(&state.renderer.gpu, "second_view_fill", &fill);
    renderer
        .pool
        .upload(&state.renderer.gpu, "second_view_text", &text);
    state.renderer.second_view = Some(renderer);
}

pub(crate) fn draw_overlay<'a>(
    state: &'a AppState,
    renderer: &'a SecondViewRenderer,
    pass: &mut wgpu::RenderPass<'a>,
) {
    let Some(layout) = crate::app::presentation::second_view::layout(state) else {
        return;
    };
    if let (Some(content), Some(binding)) = (layout.content, renderer.display_binding.as_ref()) {
        renderer.display.draw(
            pass,
            Some(binding),
            [
                content.x as u32,
                content.y as u32,
                content.w as u32,
                content.h as u32,
            ],
        );
    }
    let batch = &state.renderer.batch_renderer;
    pass.set_scissor_rect(
        layout.panel.x as u32,
        layout.panel.y as u32,
        layout.panel.w as u32,
        layout.panel.h as u32,
    );
    if let Some((buffer, count)) = renderer.pool.get("second_view_fill") {
        batch.draw_with_buffer_ui_passthrough(
            pass,
            &state.renderer.status_screen_fill,
            buffer,
            count,
        );
    }
    if let Some((buffer, count)) = renderer.pool.get("second_view_text") {
        batch.draw_with_buffer_ui_passthrough(pass, state.renderer.bit_font.atlas(), buffer, count);
    }
    pass.set_scissor_rect(0, 0, state.render_width(), state.render_height());
}
