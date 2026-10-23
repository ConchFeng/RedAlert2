//! Retail SHP/VXL body and gun presentation for the Horizon frontend.
//!
//! The diagnostic loader retains static construction for pixel regressions;
//! live matches pass the production scenario's Simulation and bound rules.
//! Atlases are built once and instance poses are refreshed after each tick.
//! Attached building animations, bibs and full tactical ordering remain open.

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result};

use crate::assets::asset_manager::AssetManager;
use crate::gpu::GpuContext;
use crate::map::entities::EntityCategory;
use crate::map::houses::HouseColorMap;
use crate::map::map_file::MapFile;
use crate::map::resolved_terrain::ResolvedTerrainGrid;
use crate::map::terrain::TerrainGrid;
use crate::map::theater::TheaterData;
use crate::render::batch::{BatchRenderer, BatchTexture, InstanceBufferPool, SpriteInstance};
use crate::render::locomotor_visual::{building_art_anchor, screen_lift_px, screen_position};
use crate::render::native_z;
use crate::render::object_shp_frame::resolve_object_shp_frame_with_facing;
use crate::render::palette_light::PaletteLight;
use crate::render::palette_textures::PaletteSet;
use crate::render::sprite_atlas::{
    ShpPaletteContext, ShpSpriteKey, SpriteAtlas, build_sprite_atlas,
};
use crate::render::tile_atlas::TileAtlas;
use crate::render::unit_atlas::{
    BarrelImagePitches, UnitAtlas, UnitSpriteKey, VxlLayer, atlas_get_with_frame_fallback,
    build_unit_atlas, canonical_turret_facing, canonical_unit_facing, unit_animation_frames,
};
use crate::render::unit_parts::{composite_draw_bounds, native_turret_barrel_order};
use crate::rules::art_data::ArtRegistry;
use crate::rules::house_colors::NO_REMAP;
use crate::rules::ruleset::RuleSet;
use crate::rules::voxel_frame_catalog::{draws_turret_parts, voxel_turret_index};
use crate::sim::world::Simulation;

pub struct RetailObjects {
    pub atlas: SpriteAtlas,
    pub instances: Vec<Vec<SpriteInstance>>,
    pub unit_atlas: UnitAtlas,
    pub unit_instances: Vec<Vec<SpriteInstance>>,
    palettes: PaletteSet,
    colors: HouseColorMap,
    barrel_image_pitches: BarrelImagePitches,
    // Page-local instance ranges, retained in entity/part insertion order.
    unit_draws: Vec<(usize, u32, u32)>,
    pub constructed: usize,
    pub voxel_entities: usize,
    pub building_instances: usize,
    pub turret_instances: usize,
    pub barrel_instances: usize,
}

impl RetailObjects {
    pub fn load(
        gpu: &GpuContext,
        batch: &BatchRenderer,
        assets: &AssetManager,
        theater: &TheaterData,
        map: &MapFile,
        resolved: &ResolvedTerrainGrid,
        grid: &TerrainGrid,
    ) -> Result<Self> {
        let audio = crate::rules::audio_sources::AudioDefinitions::select(assets);
        let mut owner = crate::rules::retail_sources::RetailRulesSources::select(assets)
            .and_then(|sources| {
                sources.into_owner(Arc::clone(audio.sounds()), Arc::clone(audio.eva()))
            })
            .map_err(anyhow::Error::msg)?;
        let (mut rules, rules_ini, art_ini, _receipt) = owner
            .load_scenario(
                crate::rules::process_owner::NativeScenarioRulesPrefix::NonCampaign(None),
                &map.ini,
            )
            .map_err(anyhow::Error::msg)?
            .into_parts();
        rules.install_art_data(ArtRegistry::from_ini(&art_ini));
        rules.bind_animation_sequences(
            &crate::rules::infantry_sequence::parse_infantry_sequence_registry(&art_ini),
        );
        let overlays =
            crate::rules::overlay_types::OverlayTypeRegistry::from_ini(&rules_ini, Some(&art_ini));
        let roster = crate::map::houses::parse_house_roster(
            &map.ini,
            rules.color_schemes.as_slice(),
            Some(&rules),
        );
        let colors = roster.color_map();
        // Use the production diagnostic constructor without a live navigation
        // world. This preview has no scenario House named "Computer" and does
        // not run placement/gameplay admission; retain the authored Location Z
        // explicitly instead. The full scenario loader must own those gates.
        let mut sim = Simulation::with_seed(0x5a17_2026);
        for placement in &map.entities {
            let level = resolved
                .cell(placement.cell_x, placement.cell_y)
                .context("retail object cell missing from terrain")?
                .level;
            let id = sim
                .spawn_object_at_height_with_overlay_registry(
                    &placement.type_id,
                    &placement.owner,
                    placement.cell_x,
                    placement.cell_y,
                    placement.facing,
                    level,
                    &rules,
                    &overlays,
                )
                .with_context(|| {
                    format!("construct retail preview object {}", placement.type_id)
                })?;
            log::info!(
                "retail_object_constructed id={id} type={} owner={} cell=({}, {}) facing={} category={:?} voxel={}",
                placement.type_id,
                placement.owner,
                placement.cell_x,
                placement.cell_y,
                placement.facing,
                sim.entities().get(id).unwrap().category(),
                sim.entities().get(id).unwrap().is_voxel(),
            );
        }
        Self::from_simulation(
            gpu,
            batch,
            assets,
            theater,
            &map.header.theater,
            &sim,
            &rules,
            colors,
            grid,
        )
    }

    pub fn from_simulation(
        gpu: &GpuContext,
        batch: &BatchRenderer,
        assets: &AssetManager,
        theater: &TheaterData,
        theater_name: &str,
        sim: &Simulation,
        rules: &RuleSet,
        colors: HouseColorMap,
        grid: &TerrainGrid,
    ) -> Result<Self> {
        let atlas = build_sprite_atlas(
            &gpu.device,
            &gpu.queue,
            batch,
            sim.entities(),
            assets,
            &theater.unit_palette,
            theater.extension,
            theater_name,
            Some(rules),
            Some(rules.art()),
            &colors,
            &[],
            &HashSet::new(),
            &HashSet::new(),
            Some(&theater.iso_palette),
            None,
            Some(sim.interner()),
        )
        .context("retail SHP atlas produced no sprites")?;
        let unit_atlas = build_unit_atlas(
            &gpu.device,
            &gpu.queue,
            batch,
            sim.entities(),
            assets,
            Some(rules),
            None,
            Some(sim.interner()),
        )
        .context("retail VXL atlas produced no sprites")?;
        let global_palette = crate::assets::pal_file::Palette::from_bytes(
            assets
                .get_ref("unitsno.pal")
                .context("global voxel palette missing")?,
        )?;
        let house_colors: Vec<_> = colors.values().copied().collect();
        let palettes = PaletteSet::new(
            gpu,
            &theater.unit_palette,
            &global_palette,
            &rules.house_color_ramps,
            &house_colors,
        );
        let mut objects = Self {
            instances: vec![Vec::new(); atlas.page_count()],
            unit_instances: vec![Vec::new(); unit_atlas.pages.len()],
            atlas,
            unit_atlas,
            palettes,
            colors,
            barrel_image_pitches: BarrelImagePitches::default(),
            unit_draws: Vec::new(),
            turret_instances: 0,
            barrel_instances: 0,
            constructed: sim
                .entities()
                .values()
                .filter(|entity| !entity.in_limbo())
                .count(),
            voxel_entities: sim
                .entities()
                .values()
                .filter(|entity| !entity.in_limbo() && entity.is_voxel())
                .count(),
            building_instances: sim
                .entities()
                .values()
                .filter(|entity| {
                    !entity.in_limbo() && entity.category() == EntityCategory::Structure
                })
                .count(),
        };
        objects.refresh(sim, rules, grid, None)?;
        log::info!(
            "retail_objects_loaded constructed={} shp_instances={} vxl_instances={}",
            objects.constructed,
            objects.instance_count(),
            objects.unit_instance_count()
        );
        Ok(objects)
    }

    pub fn update(
        &mut self,
        runtime: &crate::sim::runtime::SimRuntime,
        grid: &TerrainGrid,
    ) -> Result<()> {
        self.refresh(
            &runtime.simulation,
            &runtime.resources.rules,
            grid,
            Some(runtime),
        )
    }

    fn refresh(
        &mut self,
        sim: &Simulation,
        rules: &RuleSet,
        grid: &TerrainGrid,
        runtime: Option<&crate::sim::runtime::SimRuntime>,
    ) -> Result<()> {
        for page in self.instances.iter_mut().chain(&mut self.unit_instances) {
            page.clear();
        }
        self.unit_draws.clear();
        self.turret_instances = 0;
        self.barrel_instances = 0;
        for entity in sim
            .entities()
            .values()
            .filter(|entity| !entity.in_limbo() && entity.is_alive())
        {
            if entity.is_voxel() {
                continue;
            }
            let type_id = sim.interner().resolve(entity.type_ref());
            let building = entity.category() == EntityCategory::Structure;
            let frame = if building {
                0
            } else {
                resolve_object_shp_frame_with_facing(
                    runtime,
                    Some(rules),
                    type_id,
                    entity,
                    entity.body_facing_current(sim.session().binary_frame()),
                    None,
                )
                .with_context(|| format!("SHP frame not representable for {type_id}"))?
            };
            let house_color = self
                .colors
                .get(sim.interner().resolve(entity.owner()))
                .copied()
                .unwrap_or(NO_REMAP);
            let key = ShpSpriteKey {
                palette_context: ShpPaletteContext::Legacy,
                type_id: type_id.to_string(),
                facing: 0,
                frame,
                house_color,
            };
            let entry = self
                .atlas
                .get(&key)
                .with_context(|| format!("retail SHP body missing: {key:?}"))?;
            anyhow::ensure!(
                entry.source_palette != 0 && entry.uv_size[0] > 0.0 && entry.uv_size[1] > 0.0,
                "retail SHP atlas entry for {type_id} is empty"
            );
            let (x, y) = screen_position(entity);
            let (x, y) = if building {
                building_art_anchor(x, y)
            } else {
                (x, y)
            };
            let lift = screen_lift_px(entity);
            let object = rules
                .object(type_id)
                .context("constructed type missing from rules")?;
            let normal_z_adjust = rules
                .art()
                .resolve_metadata_entry(type_id, &object.image)
                .map_or(0, |art| art.normal_z_adjust);
            // Same DrawBody/DrawSHP parameters as the production SHP consumer
            // (src/app/presentation/instances/shp.rs). BUILDNGZ, bibs and
            // attached animations still belong to the future full draw path.
            let instance = SpriteInstance {
                position: [x + entry.offset_x, y + entry.offset_y],
                size: entry.pixel_size,
                uv_origin: entry.uv_origin,
                uv_size: entry.uv_size,
                source_palette: entry.source_palette,
                draw_state: crate::render::draw_state::DrawState {
                    remap_row: crate::render::draw_state::house_color_to_remap_row(house_color),
                    ..Default::default()
                },
                depth: native_z::depth_for_row(
                    if building {
                        y + lift as f32
                    } else {
                        y + entry.canvas_rect[1] + entry.canvas_rect[3] + lift as f32
                    },
                    grid.origin_y,
                    grid.world_height,
                ),
                tint: [1.0; 3],
                alpha: 1.0,
                palette_light: PaletteLight::color_scheme([1000; 3], 1000),
                z_adjust: (normal_z_adjust - 2 - lift) as f32,
                z_gradient: if building {
                    native_z::pack_building_z_gradient(false, entry.extended)
                } else {
                    native_z::pack_z_gradient(native_z::ZGradient::Vertical, false)
                },
                ..Default::default()
            };
            self.instances[entry.page as usize].push(instance);
        }
        for entity in sim
            .entities()
            .values()
            .filter(|entity| entity.is_voxel() && !entity.in_limbo() && entity.is_alive())
        {
            let type_id = sim.interner().resolve(entity.type_ref());
            let current = entity.current_turret_index();
            let turret_index = voxel_turret_index(type_id, Some(rules), current);
            let (body_frame, turret_frame) =
                unit_animation_frames(entity, type_id, turret_index, &self.unit_atlas.frame_counts);
            let frame = sim.session().binary_frame();
            let body_facing = entity.body_facing_byte(frame);
            let turret_facing = entity.barrel_facing.as_ref().map_or_else(
                || entity.body_facing_current(frame),
                |facing| facing.current(frame),
            );
            let layer = if draws_turret_parts(type_id, Some(rules), 0) {
                VxlLayer::Body
            } else {
                VxlLayer::Composite
            };
            let body_key = UnitSpriteKey {
                type_id: type_id.to_string(),
                turret_index: 0,
                facing: canonical_unit_facing(body_facing),
                layer,
                frame: body_frame,
                // This frontend currently presents the flat observation map.
                // Dynamic slope/rocking poses remain with the desktop consumer.
                slope_type: 0,
                barrel_pitch: 0,
            };
            let body = *atlas_get_with_frame_fallback(&self.unit_atlas, &body_key)
                .with_context(|| format!("retail VXL body missing: {body_key:?}"))?;
            let mut parts = vec![(VxlLayer::Body, body, [0.0, 0.0])];
            if draws_turret_parts(type_id, Some(rules), current) {
                let turret_key = UnitSpriteKey {
                    turret_index,
                    facing: canonical_turret_facing(turret_facing),
                    layer: VxlLayer::Turret,
                    frame: turret_frame,
                    ..body_key.clone()
                };
                let art_offset = rules.art().get(type_id).map_or(0, |art| art.turret_offset);
                let pitch = crate::render::vxl_raster::voxel_facing_step_u16(
                    entity.barrel_elevation().current(frame),
                ) as i8
                    - 8;
                let pitch = self.barrel_image_pitches.pitch(
                    &turret_key,
                    (art_offset != 0).then_some(canonical_unit_facing(body_facing)),
                    pitch,
                );
                let barrel_key = UnitSpriteKey {
                    layer: VxlLayer::Barrel,
                    frame: body_frame,
                    barrel_pitch: pitch,
                    ..turret_key.clone()
                };
                let (tx, ty) =
                    crate::render::vxl_raster::turret_pivot_screen_offset_for_slope_state(
                        art_offset,
                        body_facing,
                        0,
                        None,
                        crate::render::vxl_raster::VxlRenderParams::default().scale,
                    );
                let (bx, by) = crate::render::vxl_raster::barrel_pivot_screen_offset(
                    art_offset,
                    body_facing,
                    barrel_key.facing,
                    0,
                    None,
                    pitch,
                );
                for (key, offset) in native_turret_barrel_order(
                    turret_facing,
                    (&turret_key, [tx, ty]),
                    (&barrel_key, [tx + bx, ty + by]),
                ) {
                    // Optional absent gun models follow the production atlas
                    // catalog; a body must still remain visible.
                    if let Some(entry) = atlas_get_with_frame_fallback(&self.unit_atlas, key) {
                        parts.push((key.layer, *entry, offset));
                    }
                }
            }
            let (x, y) = screen_position(entity);
            let lift = screen_lift_px(entity);
            let depth = native_z::depth_for_row(
                y + body.offset_y + body.pixel_size[1] + lift as f32,
                grid.origin_y,
                grid.world_height,
            );
            // UnitClass::DrawVoxelBody blits the hull and selected gun parts
            // as one rectangle (0x0073B140), with one entry-2 depth seed.
            let (composite_rect, _) = composite_draw_bounds(
                parts
                    .iter()
                    .map(|(_, entry, offset)| (*entry, [x + offset[0], y + offset[1]])),
            )
            .depth_rect(false);
            let hc = self
                .colors
                .get(sim.interner().resolve(entity.owner()))
                .copied()
                .unwrap_or(NO_REMAP);
            for (layer, entry, offset) in parts {
                self.turret_instances += usize::from(layer == VxlLayer::Turret);
                self.barrel_instances += usize::from(layer == VxlLayer::Barrel);
                let instance = SpriteInstance {
                    position: [
                        x + entry.offset_x + offset[0],
                        y + entry.offset_y + offset[1],
                    ],
                    size: entry.pixel_size,
                    uv_origin: entry.uv_origin,
                    uv_size: entry.uv_size,
                    source_palette: 0,
                    draw_state: crate::render::draw_state::DrawState {
                        remap_row: crate::render::draw_state::house_color_to_remap_row(hc),
                        ..Default::default()
                    },
                    depth,
                    tint: [1.0; 3],
                    alpha: 1.0,
                    palette_light: PaletteLight::color_scheme([1000; 3], 1000),
                    z_adjust: (-2 - lift) as f32,
                    z_gradient: native_z::pack_voxel_z_gradient(
                        native_z::ZGradient::Vertical,
                        false,
                    ),
                    zshape_origin: composite_rect,
                };
                let first = self.unit_instances[entry.page].len() as u32;
                self.unit_instances[entry.page].push(instance);
                if let Some((page, start, count)) = self.unit_draws.last_mut()
                    && *page == entry.page
                    && *start + *count == first
                {
                    *count += 1;
                } else {
                    self.unit_draws.push((entry.page, first, 1));
                }
            }
        }
        Ok(())
    }

    pub fn instance_count(&self) -> usize {
        self.instances.iter().map(Vec::len).sum()
    }

    pub fn unit_instance_count(&self) -> usize {
        self.unit_instances.iter().map(Vec::len).sum()
    }

    pub fn camera(&self) -> [f32; 2] {
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for instance in self.instances.iter().chain(&self.unit_instances).flatten() {
            for axis in 0..2 {
                min[axis] = min[axis].min(instance.position[axis]);
                max[axis] = max[axis].max(instance.position[axis] + instance.size[axis]);
            }
        }
        [
            (min[0] + max[0] - 1280.0) / 2.0,
            (min[1] + max[1] - 720.0) / 2.0,
        ]
    }

    pub fn upload(&self, gpu: &GpuContext, pool: &mut InstanceBufferPool) {
        for (page, instances) in self.instances.iter().enumerate() {
            pool.upload_page(gpu, "retail-shp", page, instances);
        }
        for (page, instances) in self.unit_instances.iter().enumerate() {
            pool.upload_page(gpu, "retail-vxl", page, instances);
        }
    }

    pub fn draw<'a>(
        &'a self,
        batch: &'a BatchRenderer,
        pool: &'a InstanceBufferPool,
        pass: &mut wgpu::RenderPass<'a>,
    ) {
        for page in 0..self.atlas.page_count() {
            if let Some((buffer, count)) = pool.get_page("retail-shp", page) {
                batch.draw_zsprite_range(
                    pass,
                    &self.atlas.page(page).expect("atlas page").texture,
                    batch.default_zshape_bind_group(),
                    buffer,
                    0,
                    count,
                    true,
                );
            }
        }
        for &(page, first, count) in &self.unit_draws {
            if let Some((buffer, _)) = pool.get_page("retail-vxl", page) {
                batch.draw_voxel_sprites_range(
                    pass,
                    self.unit_atlas.page_texture(page).expect("VXL atlas page"),
                    &self.palettes.bind_group,
                    buffer,
                    first,
                    count,
                    crate::render::tactical_draw_plan::RenderZPolicy::ReadOnly,
                );
            }
        }
    }

    /// Compare each individual body against a terrain-only GPU image.
    /// An atlas entry or successful draw call alone does not prove visible
    /// pixels: every body must survive palette lookup, projection and Z test.
    pub fn validate(
        &self,
        gpu: &GpuContext,
        batch: &BatchRenderer,
        terrain: &TileAtlas,
        pool: &InstanceBufferPool,
        depth: &wgpu::TextureView,
        camera: [f32; 2],
    ) -> Result<()> {
        use std::time::Duration;

        // Keep this source-texel comparison on integral screen coordinates;
        // half-pixel viewport rounding is a separate projection concern.
        let camera = camera.map(f32::floor);
        let objects: Vec<_> = [(false, &self.instances), (true, &self.unit_instances)]
            .into_iter()
            .flat_map(|(voxel, pages)| {
                pages.iter().enumerate().flat_map(move |(page, instances)| {
                    instances
                        .iter()
                        .enumerate()
                        .map(move |(index, instance)| (voxel, page, index, instance))
                })
            })
            .collect();
        let sources: Vec<_> = objects
            .iter()
            .map(|&(voxel, page, _, instance)| {
                let texture = if voxel {
                    self.unit_atlas.page_texture(page).expect("VXL page")
                } else {
                    &self.atlas.page(page).expect("SHP page").texture
                };
                (texture, instance)
            })
            .collect();
        let source_indices = read_body_indices(gpu, &sources)?;
        const ROW_BYTES: u32 = 1280 * 4;
        const IMAGE_BYTES: usize = ROW_BYTES as usize * 720;
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Retail SHP body validation"),
            size: wgpu::Extent3d {
                width: 1280,
                height: 720,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: gpu.surface_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let read = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Retail SHP body readback"),
            size: (IMAGE_BYTES * (objects.len() * 2 + 1)) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        batch.update_camera(
            gpu,
            1280.0,
            720.0,
            camera[0],
            camera[1],
            1.0,
            crate::render::batch::DepthAxis::NONE,
        );
        let mut commands = gpu.device.create_command_encoder(&Default::default());
        for capture in 0..=objects.len() * 2 {
            {
                let mut pass = commands.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    }),
                    ..Default::default()
                });
                let (buffer, count) = pool.get("terrain").context("terrain buffer missing")?;
                batch.draw_with_buffer_zdepth(&mut pass, &terrain.zdepth_bind_group, buffer, count);
                if capture > 0 {
                    let bypass_depth = capture > objects.len();
                    let (voxel, page, index, _) = objects[(capture - 1) % objects.len()];
                    let (buffer, _) = pool
                        .get_page(if voxel { "retail-vxl" } else { "retail-shp" }, page)
                        .context("object page buffer missing")?;
                    if voxel {
                        batch.draw_voxel_sprites_range(
                            &mut pass,
                            self.unit_atlas
                                .page_texture(page)
                                .context("VXL page missing")?,
                            &self.palettes.bind_group,
                            buffer,
                            index as u32,
                            1,
                            if bypass_depth {
                                crate::render::tactical_draw_plan::RenderZPolicy::None
                            } else {
                                crate::render::tactical_draw_plan::RenderZPolicy::ReadOnly
                            },
                        );
                    } else if bypass_depth {
                        batch.draw_passthrough_range(
                            &mut pass,
                            &self.atlas.page(page).context("SHP page missing")?.texture,
                            buffer,
                            index as u32,
                            1,
                        );
                    } else {
                        batch.draw_zsprite_range(
                            &mut pass,
                            &self.atlas.page(page).context("SHP page missing")?.texture,
                            batch.default_zshape_bind_group(),
                            buffer,
                            index as u32,
                            1,
                            true,
                        );
                    }
                }
            }
            commands.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &target,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &read,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: (capture * IMAGE_BYTES) as u64,
                        bytes_per_row: Some(ROW_BYTES),
                        rows_per_image: Some(720),
                    },
                },
                wgpu::Extent3d {
                    width: 1280,
                    height: 720,
                    depth_or_array_layers: 1,
                },
            );
        }
        let submission = gpu.queue.submit([commands.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        read.slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        gpu.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(30)),
            })
            .map_err(|error| anyhow::anyhow!("SHP readback polling failed: {error:?}"))?;
        rx.recv_timeout(Duration::from_secs(1))??;
        let bytes = read.slice(..).get_mapped_range();
        let baseline = &bytes[..IMAGE_BYTES];
        let mut total_transparency_leaks = 0;
        for (capture, &(voxel, page, index, instance)) in objects.iter().enumerate() {
            let start = (capture + 1) * IMAGE_BYTES;
            let body = &bytes[start..start + IMAGE_BYTES];
            let diagnostic_start = start + objects.len() * IMAGE_BYTES;
            let diagnostic = &bytes[diagnostic_start..diagnostic_start + IMAGE_BYTES];
            let mut changed = 0;
            let mut without_depth = 0;
            let mut colors = HashSet::new();
            let mut transparent = 0;
            let mut transparency_leaks = 0;
            let mut opaque_black = 0;
            let mut black_indices = HashSet::new();
            // Match vs_main's 1x floor(raw_pos + 0.5) snapping, including
            // half-pixel camera positions and negative/offscreen rectangles.
            let left = (instance.position[0] - camera[0] + 0.5).floor() as i32;
            let top = (instance.position[1] - camera[1] + 0.5).floor() as i32;
            let width = instance.size[0] as usize;
            let min_x = left.clamp(0, 1280) as usize;
            let min_y = top.clamp(0, 720) as usize;
            let max_x = (left + width as i32).clamp(0, 1280) as usize;
            let max_y = (top + instance.size[1] as i32).clamp(0, 720) as usize;
            for y in min_y..max_y {
                for x in min_x..max_x {
                    let offset = y * ROW_BYTES as usize + x * 4;
                    let pixel: [u8; 4] = body[offset..offset + 4].try_into().unwrap();
                    // At the validation camera's 1x zoom, each stored source
                    // texel covers one screen pixel. Compare index zero with
                    // the terrain-only image, including holes and outer edges.
                    // Nonzero black artwork must remain opaque.
                    let source = (y as i32 - top) as usize * width + (x as i32 - left) as usize;
                    let palette_index = source_indices[capture][source];
                    if palette_index == 0 {
                        transparent += 1;
                        transparency_leaks += usize::from(pixel != baseline[offset..offset + 4]);
                    } else if pixel == [0, 0, 0, 255]
                        && body[offset..offset + 4] != baseline[offset..offset + 4]
                    {
                        opaque_black += 1;
                        black_indices.insert(palette_index);
                    }
                    if body[offset..offset + 4] != baseline[offset..offset + 4] {
                        changed += 1;
                        colors.insert(pixel);
                    }
                    without_depth +=
                        usize::from(diagnostic[offset..offset + 4] != baseline[offset..offset + 4]);
                }
            }
            log::info!(
                "retail_body_pixels voxel={voxel} page={page} instance={index} changed={changed} without_depth={without_depth} colors={}",
                colors.len(),
            );
            log::info!(
                "retail_body_transparency voxel={voxel} page={page} instance={index} transparent={transparent} leaks={transparency_leaks} opaque_black={opaque_black} black_indices={black_indices:?}"
            );
            anyhow::ensure!(
                transparent > 0,
                "retail body has no transparent source pixels"
            );
            total_transparency_leaks += transparency_leaks;
            // A narrow gun part has fewer painted texels than a whole hull.
            // Still require it to change the terrain under the real depth test.
            let painted = source_indices[capture]
                .iter()
                .filter(|&&index| index != 0)
                .count();
            let small_part = voxel && painted < 128;
            let required_pixels = if small_part { (painted / 2).max(1) } else { 64 };
            anyhow::ensure!(
                changed >= required_pixels && colors.len() >= if small_part { 2 } else { 8 },
                "retail body voxel={voxel} {page}/{index} is blank, clipped or incorrectly paletted: \
                 changed={changed} colors={}",
                colors.len()
            );
        }
        drop(bytes);
        read.unmap();
        anyhow::ensure!(
            total_transparency_leaks == 0,
            "transparent source pixels changed the terrain: {total_transparency_leaks}"
        );
        log::info!("retail_shp_readback=true bodies={}", objects.len());
        log::info!(
            "retail_vxl_readback=true bodies={}",
            self.unit_instance_count()
        );
        log::info!("retail_body_transparency=true bodies={}", objects.len());
        Ok(())
    }
}

/// Read the actual uploaded R8Uint texels, independently of the production
/// fragment shaders. This keeps transparency checks sensitive to a missing
/// discard or palette alpha, without retaining another CPU atlas/mask cache.
fn read_body_indices(
    gpu: &GpuContext,
    sources: &[(&BatchTexture, &SpriteInstance)],
) -> Result<Vec<Vec<u32>>> {
    use wgpu::util::DeviceExt;

    let shader = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Retail body source indices"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
            struct Rect {
                origin: vec2u,
                size: vec2u,
                output_offset: u32,
            };
            @group(0) @binding(0) var source: texture_2d<u32>;
            @group(0) @binding(1) var<uniform> rect: Rect;
            @group(0) @binding(2) var<storage, read_write> indices: array<u32>;
            @compute @workgroup_size(8, 8)
            fn main(@builtin(global_invocation_id) id: vec3u) {
                if (any(id.xy >= rect.size)) { return; }
                indices[rect.output_offset + id.y * rect.size.x + id.x] =
                    textureLoad(source, vec2i(rect.origin + id.xy), 0).r;
            }
        "#
                .into(),
            ),
        });
    let pipeline = gpu
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Retail body source indices"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
    let count: u64 = sources
        .iter()
        .map(|(_, sprite)| (sprite.size[0] as u64) * (sprite.size[1] as u64))
        .sum();
    let output = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Retail body source indices"),
        size: count * 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let read = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Retail body source index readback"),
        size: count * 4,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut commands = gpu.device.create_command_encoder(&Default::default());
    let mut output_offset = 0;
    for &(texture, sprite) in sources {
        let width = sprite.size[0] as u32;
        let height = sprite.size[1] as u32;
        let rect = [
            (sprite.uv_origin[0] * texture.width as f32).round() as u32,
            (sprite.uv_origin[1] * texture.height as f32).round() as u32,
            width,
            height,
            output_offset,
            0,
            0,
            0,
        ];
        let uniform = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Retail body source rectangle"),
                contents: bytemuck::cast_slice(&rect),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let bindings = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Retail body source indices"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        {
            let mut pass = commands.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bindings, &[]);
            pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
        }
        output_offset += width * height;
    }
    commands.copy_buffer_to_buffer(&output, 0, &read, 0, count * 4);
    let submission = gpu.queue.submit([commands.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    read.slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(std::time::Duration::from_secs(30)),
        })
        .map_err(|error| anyhow::anyhow!("source index readback failed: {error:?}"))?;
    rx.recv_timeout(std::time::Duration::from_secs(1))??;
    let bytes = read.slice(..).get_mapped_range();
    let mut offset = 0;
    let indices = sources
        .iter()
        .map(|(_, sprite)| {
            let length = sprite.size[0] as usize * sprite.size[1] as usize * 4;
            let result = bytes[offset..offset + length]
                .chunks_exact(4)
                .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
                .collect();
            offset += length;
            result
        })
        .collect();
    drop(bytes);
    read.unmap();
    Ok(indices)
}
