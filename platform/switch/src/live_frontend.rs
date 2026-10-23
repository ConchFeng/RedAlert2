//! Interactive native-window presentation over the production match runtime.
//!
//! This is the first command-integration view, pending the full tactical HUD.
//! Picking currently uses a small screen-space radius around object anchors.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use crate::assets::asset_manager::AssetManager;
use crate::gpu::GpuContext;
use crate::input::PadEdges;
use crate::map::terrain::{
    TerrainGrid, TilePlacement, build_terrain_grid_from_resolved, world_point_to_cell,
};
use crate::map::theater::{TheaterData, TileKey};
use crate::match_runtime::{SwitchMatch, write_interactive_map};
use crate::render::batch::{
    BatchRenderer, BatchTexture, DepthAxis, InstanceBufferPool, SpriteInstance,
};
use crate::render::locomotor_visual::screen_position;
use crate::render::terrain_instances::build_visible_instances;
use crate::render::tile_atlas::TileAtlas;
use crate::retail_objects::RetailObjects;
use crate::sidebar_frontend::{SidebarOutcome, SwitchSidebar};
use crate::sim::command::Command;
use anyhow::{Context, Result};

unsafe extern "C" {
    fn platform_buttons_held() -> u64;
    fn platform_touch(x: *mut u32, y: *mut u32) -> bool;
}

pub struct LiveMatch {
    pub game: SwitchMatch,
    grid: TerrainGrid,
    objects: RetailObjects,
    sidebar: SwitchSidebar,
    camera: [f32; 2],
    cursor: [f32; 2],
    cursor_texture: BatchTexture,
    touching: bool,
    initial_positions: HashMap<u64, (u16, u16)>,
    pub selection_commands: u32,
    pub move_commands: u32,
    pub attack_commands: u32,
    pub sidebar_tab_presses: u32,
    pub sidebar_cameo_presses: u32,
    pub sidebar_build_commands: u32,
    pub sidebar_queue_items: u32,
    pub sidebar_production_started: bool,
    pub repair_toggles: u32,
    pub sell_toggles: u32,
    pub placement_commands: u32,
    pub touch_samples: u32,
    pub touch_presses: u32,
    pub last_touch: [u32; 2],
    pub sidebar_gclock_seen: bool,
    armed_building: Option<crate::sim::intern::InternedId>,
}

impl LiveMatch {
    pub fn load(
        gpu: &GpuContext,
        batch: &BatchRenderer,
        assets: &mut AssetManager,
        theater: &TheaterData,
    ) -> Result<Self> {
        let map = write_interactive_map(Path::new("sdmc:/game"))?;
        log::info!("live_scenario_load_started map={}", map.display());
        let game = SwitchMatch::load(Path::new("sdmc:/game"), &map).map_err(anyhow::Error::msg)?;
        let sim = game.scenario.sim();
        let grid = build_terrain_grid_from_resolved(
            sim.resolved_terrain()
                .context("live scenario has no terrain")?,
            None,
        );
        let objects = RetailObjects::from_simulation(
            gpu,
            batch,
            assets,
            theater,
            &game.scenario.map.header.theater,
            sim,
            &game.scenario.runtime.resources.rules,
            game.colors.clone(),
            &grid,
        )?;
        let sidebar = SwitchSidebar::load(
            gpu,
            batch,
            assets,
            theater,
            sim,
            &game.scenario.runtime.resources.rules,
        )?;
        let camera = objects.camera();
        let local = sim.session().current_house();
        let first_local = sim
            .entities()
            .values()
            .find(|entity| Some(entity.owner()) == local && !entity.in_limbo() && entity.is_alive())
            .context("live map has no local controllable objects")?;
        let (x, y) = screen_position(first_local);
        let cursor = [x - camera[0], y - camera[1]];
        let initial_positions = sim
            .entities()
            .values()
            .filter(|entity| !entity.in_limbo())
            .map(|entity| {
                (
                    entity.stable_id(),
                    (entity.position.rx(), entity.position.ry()),
                )
            })
            .collect();
        // Small frontend crosshair, drawn through the production RGBA batch.
        let mut rgba = Vec::with_capacity(16 * 16 * 4);
        for y in 0..16_i32 {
            for x in 0..16_i32 {
                let cross = ((x == 7 || x == 8) && (y <= 4 || y >= 11))
                    || ((y == 7 || y == 8) && (x <= 4 || x >= 11));
                rgba.extend_from_slice(&[255, 255, 255, if cross { 255 } else { 0 }]);
            }
        }
        let cursor_texture = batch.create_texture(gpu, &rgba, 16, 16);
        log::info!(
            "live_scenario_ready local_owner={:?} entities={} terrain_cells={} cursor={cursor:?} camera={camera:?}",
            local.map(|owner| sim.interner().resolve(owner)),
            objects.constructed,
            grid.cells.len()
        );
        for entity in sim.entities().values().filter(|entity| !entity.in_limbo()) {
            let (x, y) = screen_position(entity);
            log::info!(
                "live_entity id={} type={} owner={} cell=({}, {}) screen=({}, {})",
                entity.stable_id(),
                sim.interner().resolve(entity.type_ref()),
                sim.interner().resolve(entity.owner()),
                entity.position.rx(),
                entity.position.ry(),
                x - camera[0],
                y - camera[1]
            );
        }
        Ok(Self {
            game,
            grid,
            objects,
            sidebar,
            camera,
            cursor,
            cursor_texture,
            touching: false,
            initial_positions,
            selection_commands: 0,
            move_commands: 0,
            attack_commands: 0,
            sidebar_tab_presses: 0,
            sidebar_cameo_presses: 0,
            sidebar_build_commands: 0,
            sidebar_queue_items: 0,
            sidebar_production_started: false,
            repair_toggles: 0,
            sell_toggles: 0,
            placement_commands: 0,
            touch_samples: 0,
            touch_presses: 0,
            last_touch: [0; 2],
            sidebar_gclock_seen: false,
            armed_building: None,
        })
    }

    pub fn update(
        &mut self,
        gpu: &GpuContext,
        batch: &BatchRenderer,
        pool: &mut InstanceBufferPool,
        terrain: &TileAtlas,
        elapsed: Duration,
        edges: PadEdges,
    ) -> Result<()> {
        let held = PadEdges::from_mask(unsafe { platform_buttons_held() });
        let speed = elapsed.as_secs_f32().min(0.1) * 320.0;
        let delta = [
            (i32::from(held.right) - i32::from(held.left)) as f32 * speed,
            (i32::from(held.down) - i32::from(held.up)) as f32 * speed,
        ];
        if held.l {
            self.camera[0] += delta[0];
            self.camera[1] += delta[1];
        } else {
            self.cursor[0] = (self.cursor[0] + delta[0]).clamp(0.0, 1279.0);
            self.cursor[1] = (self.cursor[1] + delta[1]).clamp(0.0, 719.0);
        }
        let (mut x, mut y) = (0, 0);
        let touching = unsafe { platform_touch(&mut x, &mut y) };
        if touching {
            let touch = [x.min(1279), y.min(719)];
            self.cursor = [touch[0] as f32, touch[1] as f32];
            self.touch_samples += 1;
            self.last_touch = touch;
        }
        if edges.a || touching && !self.touching {
            self.touch_presses += u32::from(touching && !self.touching);
            self.click();
        }
        self.touching = touching;
        if edges.b {
            self.game.issue(Command::Select {
                entity_ids: vec![],
                additive: false,
            });
        }
        let (advanced, lifecycle) = self
            .game
            .advance_with_lifecycle(elapsed)
            .map_err(anyhow::Error::msg)?;
        if advanced > 0 {
            self.objects
                .update(&self.game.scenario.runtime, &self.grid)?;
            self.sidebar.update(
                &self.game.scenario.runtime.simulation,
                &self.game.scenario.runtime.resources.rules,
                &lifecycle,
            )?;
        }
        self.refresh_queue_count();
        self.objects.upload(gpu, pool);
        self.sidebar.build_instances(gpu, pool, self.camera);
        let lookup = |tile_id, sub_tile, variant| {
            terrain
                .get_uv(TileKey {
                    tile_id,
                    sub_tile,
                    variant,
                })
                .map(|uv| TilePlacement {
                    uv_origin: uv.uv_origin,
                    uv_size: uv.uv_size,
                    pixel_size: uv.pixel_size,
                    draw_offset: uv.draw_offset,
                })
        };
        let visible = build_visible_instances(
            &self.grid,
            None,
            self.camera[0],
            self.camera[1],
            1280.0,
            720.0,
            Some(&lookup),
            self.game.scenario.sim().resolved_terrain(),
        );
        pool.upload(gpu, "terrain", &visible.normal);
        let mut markers = vec![SpriteInstance {
            position: [
                self.cursor[0] + self.camera[0] - 8.0,
                self.cursor[1] + self.camera[1] - 8.0,
            ],
            size: [16.0; 2],
            uv_size: [1.0; 2],
            tint: [1.0; 3],
            alpha: 1.0,
            ..Default::default()
        }];
        markers.extend(
            self.game
                .scenario
                .sim()
                .entities()
                .values()
                .filter(|entity| entity.selection.is_selected() && !entity.in_limbo())
                .map(|entity| {
                    let (x, y) = screen_position(entity);
                    SpriteInstance {
                        position: [x - 18.0, y - 18.0],
                        size: [36.0; 2],
                        uv_size: [1.0; 2],
                        tint: [0.2, 1.0, 0.2],
                        alpha: 1.0,
                        ..Default::default()
                    }
                }),
        );
        pool.upload(gpu, "live-markers", &markers);
        self.sidebar_gclock_seen |= self.sidebar.counts(pool).4 > 0;
        batch.update_camera(
            gpu,
            1280.0,
            720.0,
            self.camera[0],
            self.camera[1],
            1.0,
            DepthAxis::NONE,
        );
        Ok(())
    }

    pub fn validate(
        &self,
        gpu: &GpuContext,
        batch: &BatchRenderer,
        terrain: &TileAtlas,
        pool: &InstanceBufferPool,
        depth: &wgpu::TextureView,
    ) -> Result<()> {
        anyhow::ensure!(
            self.objects.turret_instances > 0 && self.objects.barrel_instances > 0,
            "live scene must draw an independent turret and barrel"
        );
        self.objects
            .validate(gpu, batch, terrain, pool, depth, self.camera)?;
        batch.update_camera(
            gpu,
            1280.0,
            720.0,
            self.camera[0],
            self.camera[1],
            1.0,
            crate::render::batch::DepthAxis::NONE,
        );
        log::info!(
            "live_voxel_parts_readback=true instances={} turrets={} barrels={}",
            self.objects.unit_instance_count(),
            self.objects.turret_instances,
            self.objects.barrel_instances,
        );
        Ok(())
    }

    pub fn voxel_part_counts(&self) -> [usize; 3] {
        [
            self.objects.unit_instance_count(),
            self.objects.turret_instances,
            self.objects.barrel_instances,
        ]
    }

    fn click(&mut self) {
        let sim = self.game.scenario.sim();
        let local = sim.session().current_house();
        let world = [
            self.cursor[0] + self.camera[0],
            self.cursor[1] + self.camera[1],
        ];
        if self.cursor[0] >= 1280.0 - crate::ui_sidebar::SIDEBAR_WIDTH_PX as f32 {
            let repair = self.sidebar.repair_mode();
            let sell = self.sidebar.sell_mode();
            let tab = self.sidebar.active_tab();
            let outcome = self.sidebar.click(
                sim,
                &self.game.scenario.runtime.resources.rules,
                self.cursor,
                false,
                false,
            );
            match outcome {
                SidebarOutcome::NotInSidebar => {}
                SidebarOutcome::Consumed => {
                    if repair != self.sidebar.repair_mode() {
                        self.repair_toggles += 1;
                    }
                    if sell != self.sidebar.sell_mode() {
                        self.sell_toggles += 1;
                    }
                    if tab != self.sidebar.active_tab() {
                        self.sidebar_tab_presses += 1;
                    }
                    return;
                }
                SidebarOutcome::Command(command) => {
                    if self.issue_sidebar_command(command) {
                        self.sidebar_cameo_presses += 1;
                        self.refresh_queue_count();
                    }
                    return;
                }
                SidebarOutcome::ArmBuildingPlacement(type_id) => {
                    self.armed_building = Some(type_id);
                    self.sidebar_cameo_presses += 1;
                    return;
                }
            }
        }
        if let Some(type_id) = self.armed_building {
            let (rx, ry) = world_point_to_cell(
                world[0],
                world[1],
                sim.resolved_terrain()
                    .map(|terrain| terrain as &dyn crate::map::terrain::TacticalCellLookup),
            );
            let type_name = sim.interner().resolve(type_id).to_string();
            let preview = crate::sim::production::placement_preview_for_owner_with_overlays(
                sim,
                &self.game.scenario.runtime.resources.rules,
                sim.interner().resolve(local.unwrap_or_default()),
                &type_name,
                rx,
                ry,
                Some(&self.game.scenario.runtime.resources.overlay_registry),
            );
            if preview.is_some_and(|preview| preview.valid && preview.in_build_area) {
                if self
                    .game
                    .issue(Command::PlaceReadyBuilding { type_id, rx, ry })
                {
                    self.placement_commands += 1;
                    self.armed_building = None;
                    self.sidebar.clear_armed_building();
                    log::info!("live_place type={type_name} cell=({rx}, {ry})");
                }
            }
            return;
        }
        if self.sidebar.repair_mode() || self.sidebar.sell_mode() {
            self.repair_or_sell_click(world);
            return;
        }
        // Initial command integration uses anchor-radius picking; pixel masks,
        // bridges and full tactical mouse actions remain with the client port.
        let picked = sim
            .entities()
            .values()
            .filter(|entity| !entity.in_limbo() && entity.is_alive())
            .filter_map(|entity| {
                let (x, y) = screen_position(entity);
                let distance = (x - world[0]).powi(2) + (y - world[1]).powi(2);
                (distance <= 28.0_f32.powi(2)).then_some((
                    distance,
                    entity.stable_id(),
                    entity.owner(),
                ))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, id, owner)) = picked
            && Some(owner) == local
        {
            if self.game.issue(Command::Select {
                entity_ids: vec![id],
                additive: false,
            }) {
                self.selection_commands += 1;
                log::info!("live_select id={id} cursor={:?}", self.cursor);
            }
            return;
        }
        let selected: Vec<_> = sim
            .entities()
            .values()
            .filter(|entity| entity.selection.is_selected() && Some(entity.owner()) == local)
            .map(|entity| entity.stable_id())
            .collect();
        let (rx, ry) = world_point_to_cell(
            world[0],
            world[1],
            sim.resolved_terrain()
                .map(|terrain| terrain as &dyn crate::map::terrain::TacticalCellLookup),
        );
        if sim
            .resolved_terrain()
            .and_then(|terrain| terrain.cell(rx, ry))
            .is_none()
        {
            return;
        }
        for id in selected {
            let command = if let Some((_, target_id, _)) = picked {
                Command::Attack {
                    attacker_id: id,
                    target_id,
                }
            } else {
                Command::Move {
                    entity_id: id,
                    target_rx: rx,
                    target_ry: ry,
                }
            };
            if self.game.issue(command) {
                if picked.is_some() {
                    self.attack_commands += 1;
                } else {
                    self.move_commands += 1;
                }
                log::info!(
                    "live_order id={id} target_cell=({rx}, {ry}) attack={}",
                    picked.is_some()
                );
            }
        }
    }

    fn issue_sidebar_command(&mut self, command: Command) -> bool {
        if matches!(
            &command,
            Command::QueueProduction { .. }
                | Command::SuspendProduction { .. }
                | Command::CancelProductionByType { .. }
        ) {
            let build_command = matches!(
                &command,
                Command::QueueProduction { .. }
                    | Command::SuspendProduction { .. }
                    | Command::CancelProductionByType { .. }
            );
            let accepted = self.game.issue(command);
            if accepted && build_command {
                self.sidebar_build_commands += 1;
            }
            accepted
        } else {
            self.game.issue(command)
        }
    }

    fn refresh_queue_count(&mut self) {
        let sim = self.game.scenario.sim();
        let Some(owner) = sim.session().current_house() else {
            return;
        };
        let queued = crate::sim::production::queue_view_for_owner(
            sim,
            &self.game.scenario.runtime.resources.rules,
            sim.interner().resolve(owner),
        )
        .len() as u32;
        self.sidebar_queue_items = self.sidebar_queue_items.max(queued);
        self.sidebar_production_started |= queued > 0;
    }

    fn repair_or_sell_click(&mut self, world: [f32; 2]) {
        let sim = self.game.scenario.sim();
        let Some(local) = sim.session().current_house() else {
            return;
        };
        let picked = sim
            .entities()
            .values()
            .filter(|entity| {
                entity.owner() == local
                    && entity.category() == crate::map::entities::EntityCategory::Structure
                    && !entity.in_limbo()
                    && entity.is_alive()
            })
            .filter_map(|entity| {
                let (x, y) = screen_position(entity);
                let distance = (x - world[0]).powi(2) + (y - world[1]).powi(2);
                (distance <= 28.0_f32.powi(2)).then_some((distance, entity.stable_id()))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id)| id);
        let Some(entity_id) = picked else {
            return;
        };
        let rules = &self.game.scenario.runtime.resources.rules;
        let command = if self.sidebar.repair_mode()
            && crate::sim::production::can_repair_building(sim, rules, entity_id)
        {
            Command::ToggleRepair { entity_id }
        } else if self.sidebar.sell_mode()
            && crate::sim::production::can_sell_building(sim, rules, entity_id)
        {
            Command::SellBuilding { entity_id }
        } else {
            return;
        };
        if self.game.issue(command) {
            log::info!("live_repair_sell entity={entity_id}");
        }
    }

    pub fn draw<'a>(
        &'a self,
        batch: &'a BatchRenderer,
        pool: &'a InstanceBufferPool,
        pass: &mut wgpu::RenderPass<'a>,
    ) {
        self.objects.draw(batch, pool, pass);
        self.sidebar.draw(batch, pool, pass);
        if let Some((buffer, count)) = pool.get("live-markers") {
            batch.draw_with_buffer_passthrough(pass, &self.cursor_texture, buffer, count);
        }
    }

    pub fn moved_entities(&self) -> usize {
        self.game
            .scenario
            .sim()
            .entities()
            .values()
            .filter(|entity| {
                self.initial_positions
                    .get(&entity.stable_id())
                    .is_some_and(|&initial| initial != (entity.position.rx(), entity.position.ry()))
            })
            .count()
    }

    pub fn sidebar_counts(&self, pool: &InstanceBufferPool) -> (usize, usize, usize, usize, usize) {
        self.sidebar.counts(pool)
    }

    pub fn sidebar_generated_font(&self) -> bool {
        self.sidebar.generated_font()
    }
}
