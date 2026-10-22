//! Exercise production sprite batching through libnx's native window and NVK.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::fs::File;
use std::io::Write;
use std::num::NonZeroUsize;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use wgpu::rwh::{
    DisplayHandle, HasDisplayHandle, HasWindowHandle, HorizonWindowHandle, WindowHandle,
};

use crate::assets::asset_manager::AssetManager;
use crate::assets::mix_hash::mix_hash;
use crate::gpu::{GpuContext, NativeWindow};
use crate::map::bridge_facts::BridgeCellFacts;
use crate::map::resolved_terrain::{ResolvedTerrainCell, ResolvedTerrainGrid};
use crate::map::terrain::{TerrainGrid, TilePlacement, build_terrain_grid_from_resolved};
use crate::map::theater::{TileImage, TileKey, collect_used_tiles, load_theater, load_tile_images};
use crate::render::batch::{
    BatchRenderer, BatchTexture, DepthAxis, InstanceBufferPool, SpriteInstance,
};
use crate::render::palette_light::PaletteLight;
use crate::render::terrain_instances::build_visible_instances;
use crate::render::tile_atlas::{TileAtlas, build_atlas};

#[derive(Debug, Clone, Copy)]
enum Expect {
    Background,
    Red,
    Green,
    Blue,
    Yellow,
}

impl Expect {
    fn matches(self, actual: [u8; 4], background: [u8; 4]) -> bool {
        let expected = match self {
            Self::Background => background,
            Self::Red => [255, 13, 4, 255],
            Self::Green => [4, 255, 13, 255],
            Self::Blue => [4, 13, 255, 255],
            Self::Yellow => [255, 190, 4, 255],
        };
        actual
            .iter()
            .zip(expected)
            .all(|(&channel, expected)| channel.abs_diff(expected) <= 8)
    }
}

#[derive(Debug, Clone, Copy)]
enum TerrainExpect {
    Clear,
    Elevated,
    Occluder,
}

impl TerrainExpect {
    fn matches(self, actual: [u8; 4], background: [u8; 4]) -> bool {
        let expected = match self {
            // Palette bytes enter through an Rgba8UnormSrgb atlas, so the
            // shader's linear output is stored in the Rgba8Unorm target.
            Self::Clear => [0, 239, 0, 255],
            Self::Elevated => [239, 239, 0, 255],
            Self::Occluder => [239, 0, 239, 255],
        };
        actual
            .iter()
            .zip(expected)
            .all(|(&channel, expected)| channel.abs_diff(expected) <= 8)
    }
}

fn diamond_pixel_count(tile_height: u32) -> usize {
    let mut total = 0;
    let mut width = 4_u32;
    let half_minus_one = tile_height / 2 - 1;
    for row in 0..tile_height {
        total += width as usize;
        if row < half_minus_one {
            width += 4;
        } else {
            width = width.saturating_sub(4);
        }
    }
    total
}

/// Build a valid 1x1 TMP through the bytes interface. The returned pixel bytes
/// are palette indices 1 and 2 split at the diamond midpoint, and every
/// interior pixel carries a caller-supplied Z value.
fn make_tmp(left: u8, right: u8, depth: u8) -> Vec<u8> {
    const HEADER: usize = 16;
    const CELL_HEADER: usize = 52;
    let (tile_width, tile_height) = (60_u32, 30_u32);
    let pixels = diamond_pixel_count(tile_height);
    let mut data = Vec::new();
    data.extend_from_slice(&1_u32.to_le_bytes());
    data.extend_from_slice(&1_u32.to_le_bytes());
    data.extend_from_slice(&tile_width.to_le_bytes());
    data.extend_from_slice(&tile_height.to_le_bytes());
    data.extend_from_slice(&((HEADER + 4) as u32).to_le_bytes());
    data.resize(HEADER + 4 + CELL_HEADER, 0);
    let cell = HEADER + 4;
    data[cell + 4..cell + 8].copy_from_slice(&(tile_height as i32).to_le_bytes());
    data[cell + 12..cell + 16].copy_from_slice(&(CELL_HEADER as u32).to_le_bytes());
    data[cell + 20..cell + 24].copy_from_slice(&0_i32.to_le_bytes());
    data[cell + 24..cell + 28].copy_from_slice(&(tile_height as i32).to_le_bytes());
    data[cell + 28..cell + 32].copy_from_slice(&0_u32.to_le_bytes());
    data[cell + 32..cell + 36].copy_from_slice(&0_u32.to_le_bytes());
    data[cell + 36..cell + 40].copy_from_slice(&2_u32.to_le_bytes());
    data[cell + 40] = 0;
    data[cell + 41] = 0;
    data[cell + 42] = 0;
    data[cell + 43..cell + 49].copy_from_slice(&[20, 10, 5, 10, 20, 5]);

    let mut row_width = 4_u32;
    let half_minus_one = tile_height / 2 - 1;
    for row in 0..tile_height {
        for column in 0..row_width {
            let x = (tile_width - row_width) / 2 + column;
            data.push(if x < tile_width / 2 { left } else { right });
        }
        if row < half_minus_one {
            row_width += 4;
        } else {
            row_width = row_width.saturating_sub(4);
        }
    }
    debug_assert_eq!(data.len(), cell + CELL_HEADER + pixels);
    data.extend(std::iter::repeat_n(depth, pixels));
    data
}

fn fixture_palette_bytes() -> Vec<u8> {
    (0..256)
        .flat_map(|index| match index {
            1 => [63, 0, 0],
            2 => [0, 62, 0],
            3 => [0, 0, 62],
            4 => [62, 62, 0],
            5 => [62, 0, 62],
            _ => [0, 0, 0],
        })
        .collect()
}

fn fixture_mix(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let file_count = u16::try_from(entries.len()).context("fixture MIX has too many entries")?;
    let body_size: usize = entries.iter().map(|(_, bytes)| bytes.len()).sum();
    let mut data = Vec::with_capacity(6 + entries.len() * 12 + body_size);
    data.extend_from_slice(&file_count.to_le_bytes());
    data.extend_from_slice(
        &u32::try_from(body_size)
            .context("fixture MIX body is too large")?
            .to_le_bytes(),
    );
    let mut offset = 0_u32;
    for (name, bytes) in entries {
        data.extend_from_slice(&mix_hash(name).to_le_bytes());
        data.extend_from_slice(&offset.to_le_bytes());
        data.extend_from_slice(
            &u32::try_from(bytes.len())
                .context("fixture MIX entry is too large")?
                .to_le_bytes(),
        );
        offset += u32::try_from(bytes.len()).expect("entry size checked above");
    }
    for (_, bytes) in entries {
        data.extend_from_slice(bytes);
    }
    Ok(data)
}

/// Write one active-YR theater resource root and keep every resource in the
/// production theater MIX. `load_theater` activates the archive before reading
/// its INI, palettes, or TMPs, so this exercises the same lookup order as the
/// desktop client instead of constructing TileImages directly.
fn write_theater_fixture() -> Result<()> {
    const ROOT: &str = "sdmc:/switch/vera20k/theater-fixture";
    std::fs::create_dir_all(ROOT)?;
    let ini = b"[General]\n\
                [TileSet0000]\n\
                SetName=Grass\n\
                FileName=clear\n\
                TilesInSet=5\n";
    let palette = fixture_palette_bytes();
    let mut entries = vec![
        ("temperatmd.ini".to_string(), ini.to_vec()),
        ("isotem.pal".to_string(), palette.clone()),
        ("unittem.pal".to_string(), palette.clone()),
        ("temperat.pal".to_string(), palette),
    ];
    for (tile_id, (left, right, depth)) in
        [(2, 2, 2), (4, 4, 28), (5, 5, 30), (5, 5, 0), (5, 5, 60)]
            .into_iter()
            .enumerate()
    {
        entries.push((
            format!("clear{:02}.tem", tile_id + 1),
            make_tmp(left, right, depth),
        ));
    }
    std::fs::write(Path::new(ROOT).join("temperat.mix"), fixture_mix(&entries)?)?;
    Ok(())
}

struct LoadedTheaterTiles {
    images: HashMap<TileKey, TileImage>,
    ini_source: String,
    tile_slots: usize,
    tilesets: usize,
}

fn load_theater_fixture() -> Result<LoadedTheaterTiles> {
    const ROOT: &str = "sdmc:/switch/vera20k/theater-fixture";
    let mut assets = AssetManager::from_ini_probe_root(Path::new(ROOT))?;
    let theater =
        load_theater(&mut assets, "TEMPERATE").context("production theater load failed")?;
    let (_, ini_source) = assets
        .get_with_source("temperatmd.ini")
        .context("theater INI did not resolve after archive activation")?;
    anyhow::ensure!(
        ini_source.eq_ignore_ascii_case("temperat.mix"),
        "theater INI resolved from {ini_source}, expected temperat.mix"
    );
    anyhow::ensure!(
        theater
            .lookup
            .filename(0)
            .is_some_and(|name| name == "clear01.tem"),
        "theater tileset did not publish clear01.tem at tile_id 0"
    );
    // Tiles 3 and 4 are deliberately outside the map grid because the
    // occlusion probes request them later. Include them in the production
    // preloader's requested set, as a real map atlas would.
    let needed: HashSet<TileKey> = collect_used_tiles(&[(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)]);
    let images = load_tile_images(&assets, &theater.lookup, &theater.iso_palette, &needed);
    anyhow::ensure!(
        images.len() == needed.len(),
        "production theater loader decoded {} of {} requested TMP tiles",
        images.len(),
        needed.len()
    );
    Ok(LoadedTheaterTiles {
        images,
        ini_source,
        tile_slots: theater.lookup.len(),
        tilesets: theater.lookup.bounds().len(),
    })
}

fn terrain_grid_fixture() -> (ResolvedTerrainGrid, TerrainGrid) {
    let mut cells = Vec::new();
    for ry in 0..3_u16 {
        for rx in 0..3_u16 {
            let tile_id = match (rx, ry) {
                (0, 0) | (1, 0) | (0, 1) | (1, 1) => 0,
                (2, 0) | (2, 1) => 1,
                (2, 2) => 2,
                _ => 0,
            };
            let level = u8::from(rx == 2 && ry != 2);
            cells.push(ResolvedTerrainCell {
                rx,
                ry,
                source_tile_index: i32::from(tile_id),
                source_sub_tile: 0,
                final_tile_index: i32::from(tile_id),
                final_sub_tile: 0,
                is_wood_bridge_repair_tile: false,
                level,
                filled_clear: false,
                tileset_index: Some(0),
                land_type: 0,
                slope_type: 0,
                template_height: 0,
                height_in_pixels: 0,
                render_offset_x: 0,
                render_offset_y: 0,
                speed_costs: Default::default(),
                accepts_smudge: false,
                allows_tiberium: false,
                variant: 0,
                has_ramp: false,
                canonical_ramp: None,
                ground_walk_blocked: false,
                terrain_object_blocks: false,
                terrain_object_occupation: None,
                overlay_blocks: false,
                overlay_zone_type: None,
                outside_playfield: false,
                zone_type: 0,
                base_ground_walk_blocked: false,
                base_build_blocked: false,
                base_land_type: 0,
                base_speed_costs: Default::default(),
                has_bridge_deck: false,
                bridge_walkable: false,
                bridge_transition: false,
                bridge_deck_level: 0,
                bridge_layer: None,
                bridge_facts: BridgeCellFacts::default(),
                tube_index: None,
                radar_left: [20, 10, 5],
                radar_right: [10, 20, 5],
                has_damaged_data: false,
            });
        }
    }
    let resolved = ResolvedTerrainGrid::from_cells(3, 3, cells);
    let grid = build_terrain_grid_from_resolved(&resolved, None);
    (resolved, grid)
}

fn terrain_sample_at_center(
    instances: &[SpriteInstance],
    camera: [f32; 2],
    position_x: f32,
) -> (u32, u32) {
    let instance = instances
        .iter()
        .find(|instance| instance.position[0] == position_x && instance.position[1] >= 30.0)
        .expect("terrain instance");
    (
        (instance.position[0] + instance.size[0] / 2.0 - camera[0]).round() as u32,
        (instance.position[1] + instance.size[1] / 2.0 - camera[1]).round() as u32,
    )
}

unsafe extern "C" {
    fn platform_console_init();
    fn platform_window_init() -> *mut c_void;
    fn platform_running() -> bool;
    fn platform_buttons() -> u64;
}

const DIRECTORY: &str = "sdmc:/switch/vera20k";
static LOGGER: FileLogger = FileLogger(Mutex::new(None));

struct FileLogger(Mutex<Option<File>>);

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            if let Some(file) = self.0.lock().unwrap().as_mut() {
                let _ = writeln!(
                    file,
                    "{} {}: {}",
                    record.level(),
                    record.target(),
                    record.args()
                );
                let _ = file.flush();
            }
        }
    }

    fn flush(&self) {
        if let Some(file) = self.0.lock().unwrap().as_mut() {
            let _ = file.flush();
        }
    }
}

/// libnx owns this process-wide window until main returns. The handle token is
/// shared with wgpu; only the rendering thread creates or reconfigures a surface.
#[derive(Clone)]
struct HorizonWindow {
    address: NonZeroUsize,
}

impl HasDisplayHandle for HorizonWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, wgpu::rwh::HandleError> {
        Ok(DisplayHandle::horizon())
    }
}

impl HasWindowHandle for HorizonWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, wgpu::rwh::HandleError> {
        let pointer = NonNull::new(self.address.get() as *mut c_void).unwrap();
        let handle = HorizonWindowHandle::new(pointer);
        // SAFETY: libnx's default NWindow outlives the surface and all its owned
        // HorizonWindow clones. No code destroys or replaces it while wgpu runs.
        Ok(unsafe { WindowHandle::borrow_raw(handle.into()) })
    }
}

impl NativeWindow for HorizonWindow {
    type Handle = Self;

    fn surface_handle(&self) -> Self::Handle {
        self.clone()
    }

    fn inner_size(&self) -> (u32, u32) {
        (1280, 720)
    }
}

/// A submitted frame can still be blank when the emulator rejects NVK shaders.
/// Check real GPU output before reporting texture/instance success.
fn validate_sprite_output(
    gpu: &GpuContext,
    batch: &BatchRenderer,
    texture: &BatchTexture,
    pool: &InstanceBufferPool,
    depth: &wgpu::TextureView,
    samples: &[(u32, u32, Expect)],
) -> Result<()> {
    anyhow::ensure!(
        gpu.surface_format == wgpu::TextureFormat::Rgba8Unorm,
        "pixel probe expects the Horizon RGBA8 surface"
    );
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Switch sprite pixel probe"),
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
    // Magenta clear colour distinguishes "the quad did not rasterize" from
    // "the shader resolved black", and from a real sprite pixel.
    let clear = wgpu::Color {
        r: 0.5,
        g: 0.0,
        b: 0.5,
        a: 1.0,
    };
    let clear_bytes = [128_u8, 0, 128, 255];
    let read = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Switch sprite pixel readback"),
        size: (samples.len() * 256) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut commands = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = commands.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
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
        let (buffer, count) = pool.get("synthetic").context("sprite buffer missing")?;
        batch.draw_with_buffer_passthrough(&mut pass, texture, buffer, count);
    }
    for (index, &(x, y, _)) in samples.iter().enumerate() {
        commands.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &read,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: (index * 256) as u64,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
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
            timeout: Some(Duration::from_secs(15)),
        })
        .map_err(|error| anyhow::anyhow!("pixel readback polling failed: {error:?}"))?;
    rx.recv_timeout(Duration::from_secs(1))??;
    let bytes = read.slice(..).get_mapped_range();
    for (index, &(x, y, expected)) in samples.iter().enumerate() {
        let actual: [u8; 4] = bytes[index * 256..index * 256 + 4].try_into().unwrap();
        log::info!("pixel ({x},{y})={actual:?} expected={expected:?}");
        anyhow::ensure!(
            expected.matches(actual, clear_bytes),
            "sprite pixel ({x},{y})={actual:?}, expected {expected:?}"
        );
    }
    drop(bytes);
    read.unmap();
    log::info!("gpu_pixel_readback=true samples={}", samples.len());
    Ok(())
}

fn validate_terrain_output(
    gpu: &GpuContext,
    batch: &BatchRenderer,
    atlas: &TileAtlas,
    pool: &InstanceBufferPool,
    pool_key: &'static str,
    depth: &wgpu::TextureView,
    camera: [f32; 2],
    samples: &[(u32, u32, TerrainExpect)],
) -> Result<()> {
    let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Switch TMP terrain pixel probe"),
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
    let clear_bytes = [128_u8, 0, 128, 255];
    let read = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Switch TMP terrain pixel readback"),
        size: (samples.len() * 256) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    batch.update_camera(
        &gpu,
        1280.0,
        720.0,
        camera[0],
        camera[1],
        1.0,
        DepthAxis::NONE,
    );
    let mut commands = gpu.device.create_command_encoder(&Default::default());
    {
        let mut pass = commands.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.5,
                        g: 0.0,
                        b: 0.5,
                        a: 1.0,
                    }),
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
        let (buffer, count) = pool.get(pool_key).context("terrain buffer missing")?;
        batch.draw_with_buffer_zdepth(&mut pass, &atlas.zdepth_bind_group, buffer, count);
    }
    for (index, &(x, y, _)) in samples.iter().enumerate() {
        commands.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &read,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: (index * 256) as u64,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(1),
                },
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
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
            timeout: Some(Duration::from_secs(15)),
        })
        .map_err(|error| anyhow::anyhow!("terrain pixel readback polling failed: {error:?}"))?;
    rx.recv_timeout(Duration::from_secs(1))??;
    let bytes = read.slice(..).get_mapped_range();
    for (index, &(x, y, expected)) in samples.iter().enumerate() {
        let actual: [u8; 4] = bytes[index * 256..index * 256 + 4].try_into().unwrap();
        log::info!("terrain pixel ({x},{y})={actual:?} expected={expected:?}");
        anyhow::ensure!(
            expected.matches(actual, clear_bytes),
            "terrain pixel ({x},{y})={actual:?}, expected {expected:?}"
        );
    }
    drop(bytes);
    read.unmap();
    Ok(())
}

fn run() -> Result<String> {
    unsafe { platform_console_init() };
    let address = NonZeroUsize::new(unsafe { platform_window_init() } as usize)
        .context("libnx default window initialization failed")?;
    log::info!("native_window_ready");
    let gpu = GpuContext::new_with_native_window(HorizonWindow { address })?;
    log::info!("production_gpu_context_ready");
    let batch = BatchRenderer::new(&gpu);
    let depth = gpu.create_depth_texture();
    let mut pool = InstanceBufferPool::new();
    // Synthetic RGBA with transparent corners and four colour quadrants.
    // All texture upload, sampling and instancing use the desktop renderer.
    let mut rgba = Vec::with_capacity(64 * 64 * 4);
    for y in 0_i32..64 {
        for x in 0_i32..64 {
            let colour = match (x >= 32, y >= 32) {
                (false, false) => [255, 64, 32],
                (true, false) => [32, 255, 64],
                (false, true) => [32, 64, 255],
                (true, true) => [255, 224, 32],
            };
            rgba.extend_from_slice(&colour);
            rgba.push(if (x - 32).abs() + (y - 32).abs() <= 30 {
                255
            } else {
                0
            });
        }
    }
    let texture = batch.create_texture(&gpu, &rgba, 64, 64);
    let mut instances = [SpriteInstance::default(); 12];
    for (index, instance) in instances.iter_mut().enumerate() {
        *instance = SpriteInstance {
            position: [
                192.0 + (index % 4) as f32 * 256.0,
                80.0 + (index / 4) as f32 * 216.0,
            ],
            size: [128.0, 128.0],
            uv_size: [1.0, 1.0],
            depth: 0.5,
            tint: [1.0; 3],
            alpha: 1.0,
            // Zero selects the shared renderer's precomposed RGBA path.
            palette_light: PaletteLight::default(),
            ..Default::default()
        };
    }
    batch.update_camera(&gpu, 1280.0, 720.0, 0.0, 0.0, 1.0, DepthAxis::NONE);
    log::info!(
        "production_batch_renderer_ready sprites={}",
        instances.len()
    );
    pool.upload(&gpu, "synthetic", &instances);
    let mut samples = vec![
        (16, 16, Expect::Background),
        (194, 82, Expect::Background),
        (232, 120, Expect::Red),
        (280, 120, Expect::Green),
        (232, 168, Expect::Blue),
        (280, 168, Expect::Yellow),
    ];
    samples.extend(instances[1..].iter().map(|instance| {
        (
            instance.position[0] as u32 + 40,
            instance.position[1] as u32 + 40,
            Expect::Red,
        )
    }));
    validate_sprite_output(&gpu, &batch, &texture, &pool, &depth, &samples)?;
    // Use the same production camera and draw path with scroll and 2x zoom.
    // These points must move with the camera; a fixed-coordinate shader fails.
    batch.update_camera(&gpu, 1280.0, 720.0, 128.0, 32.0, 2.0, DepthAxis::NONE);
    validate_sprite_output(
        &gpu,
        &batch,
        &texture,
        &pool,
        &depth,
        &[
            (16, 16, Expect::Background),
            (130, 98, Expect::Background),
            (208, 176, Expect::Red),
            (304, 176, Expect::Green),
            (208, 272, Expect::Blue),
            (304, 272, Expect::Yellow),
        ],
    )?;
    log::info!("camera_transform_readback=true");

    write_theater_fixture()?;
    let theater = load_theater_fixture()?;
    let terrain_images = theater.images;
    log::info!(
        "production_theater_loaded=TEMPERATE source={} slots={} tilesets={} images={}",
        theater.ini_source,
        theater.tile_slots,
        theater.tilesets,
        terrain_images.len()
    );
    let (resolved, terrain_grid) = terrain_grid_fixture();
    let atlas = build_atlas(&gpu.device, &gpu.queue, &batch, &terrain_images);
    anyhow::ensure!(
        atlas.tile_count() == terrain_images.len(),
        "production atlas lost TMP tiles"
    );
    let lookup = |tile_id, sub_tile, variant| {
        atlas
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
    let camera = [terrain_grid.origin_x - 1.0, terrain_grid.origin_y - 1.0];
    let visible = build_visible_instances(
        &terrain_grid,
        None,
        camera[0],
        camera[1],
        1280.0,
        720.0,
        Some(&lookup),
        Some(&resolved),
    );
    anyhow::ensure!(
        visible.normal.len() == terrain_grid.cells.len(),
        "production terrain culling lost visible TMP instances"
    );
    for instance in &visible.normal {
        log::info!(
            "terrain_instance pos={:?} size={:?} uv={:?}+{:?} z_adjust={}",
            instance.position,
            instance.size,
            instance.uv_origin,
            instance.uv_size,
            instance.z_adjust,
        );
    }
    pool.upload(&gpu, "terrain", &visible.normal);
    let elevated = visible
        .normal
        .iter()
        .find(|instance| instance.position[0] == 30.0 && instance.position[1] >= 30.0)
        .expect("elevated terrain instance");
    anyhow::ensure!(
        elevated.uv_origin[1] != 0.0,
        "elevated terrain did not select its own atlas tile"
    );
    let ground_pixel = terrain_sample_at_center(&visible.normal, camera, -30.0);
    let elevated_pixel = terrain_sample_at_center(&visible.normal, camera, 30.0);
    let samples = [
        (ground_pixel.0, ground_pixel.1, TerrainExpect::Clear),
        (elevated_pixel.0, elevated_pixel.1, TerrainExpect::Elevated),
    ];
    validate_terrain_output(
        &gpu, &batch, &atlas, &pool, "terrain", &depth, camera, &samples,
    )?;
    log::info!(
        "terrain_tmp_atlas_readback=true tiles={}",
        atlas.tile_count()
    );

    let ground = visible
        .normal
        .iter()
        .find(|instance| instance.position[0] == -30.0 && instance.position[1] >= 15.0)
        .expect("ground terrain instance");
    let elevated = visible
        .normal
        .iter()
        .find(|instance| instance.position[0] == 30.0 && instance.position[1] >= 30.0)
        .expect("elevated terrain instance");
    let mut depth_instances = vec![ground.clone()];
    let mut near_occluder = ground.clone();
    let near_uv = atlas
        .get_uv(TileKey {
            tile_id: 3,
            sub_tile: 0,
            variant: 0,
        })
        .expect("near occluder tile");
    let near_image = terrain_images
        .get(&TileKey {
            tile_id: 3,
            sub_tile: 0,
            variant: 0,
        })
        .expect("near occluder image");
    let near_center =
        ((near_image.height / 2 * near_image.width + near_image.width / 2) * 4) as usize;
    log::info!(
        "near_occluder uv={:?}+{:?} size={:?} pixel={:?}",
        near_uv.uv_origin,
        near_uv.uv_size,
        [near_image.width, near_image.height],
        &near_image.rgba[near_center..near_center + 4],
    );
    near_occluder.position = [
        ground_pixel.0 as f32 + camera[0] - 30.0,
        ground_pixel.1 as f32 + camera[1] - 15.0,
    ];
    near_occluder.uv_origin = near_uv.uv_origin;
    near_occluder.uv_size = near_uv.uv_size;
    pool.upload(&gpu, "terrain-near", std::slice::from_ref(&near_occluder));
    validate_terrain_output(
        &gpu,
        &batch,
        &atlas,
        &pool,
        "terrain-near",
        &depth,
        camera,
        &[(ground_pixel.0, ground_pixel.1, TerrainExpect::Occluder)],
    )?;
    depth_instances.push(near_occluder);

    depth_instances.push(elevated.clone());
    let mut far_occluder = ground.clone();
    let far_uv = atlas
        .get_uv(TileKey {
            tile_id: 4,
            sub_tile: 0,
            variant: 0,
        })
        .expect("far occluder tile");
    far_occluder.uv_origin = far_uv.uv_origin;
    far_occluder.uv_size = far_uv.uv_size;
    depth_instances.push(far_occluder);
    pool.upload(&gpu, "terrain-depth", &depth_instances);
    validate_terrain_output(
        &gpu,
        &batch,
        &atlas,
        &pool,
        "terrain-depth",
        &depth,
        camera,
        &[
            (ground_pixel.0, ground_pixel.1, TerrainExpect::Occluder),
            (elevated_pixel.0, elevated_pixel.1, TerrainExpect::Elevated),
        ],
    )?;
    log::info!("terrain_tmp_zdepth_occlusion=true near_passes=true far_rejected=true");

    batch.update_camera(&gpu, 1280.0, 720.0, 0.0, 0.0, 1.0, DepthAxis::NONE);
    let start = Instant::now();
    let mut frames = 0_u32;
    let mut presses = 0_u32;
    let mut exit_reason = "applet";
    while unsafe { platform_running() } {
        if start.elapsed() >= Duration::from_secs(30) {
            exit_reason = "timeout";
            break;
        }
        let edges = crate::input::PadEdges::from_mask(unsafe { platform_buttons() });
        if edges.plus {
            exit_reason = "plus";
            break;
        }
        presses += u32::from(edges.a);
        let shift = (start.elapsed().as_secs_f32() * 2.0).sin() * 32.0;
        for (index, instance) in instances.iter_mut().enumerate() {
            instance.position[0] = 192.0 + (index % 4) as f32 * 256.0 + shift;
        }
        pool.upload(&gpu, "synthetic", &instances);
        let Some(frame) = gpu.acquire_frame()? else {
            continue;
        };
        let view = frame.texture.create_view(&Default::default());
        let mut commands = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = commands.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.012,
                            g: 0.055,
                            b: 0.085,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            let (buffer, count) = pool.get("synthetic").context("sprite buffer missing")?;
            batch.draw_with_buffer_passthrough(&mut pass, &texture, buffer, count);
        }
        gpu.queue.submit([commands.finish()]);
        frame.present();
        frames += 1;
        if frames == 1 || frames % 300 == 0 {
            log::info!("frames_presented={frames}");
        }
    }
    anyhow::ensure!(frames > 0, "no frames presented");
    let render_elapsed_ms = start.elapsed().as_millis();
    Ok(format!(
        "binary_format=nro\nproduction_gpu_context=true\nwgsl_pipeline=true\n\
         batch_renderer=true\ntextured_sprite=true\nsprites_per_frame={}\n\
         gpu_pixel_readback=true\ncamera_transform_readback=true\n\
         terrain_tmp_atlas_readback=true\nterrain_tmp_zdepth_occlusion=true\n\
         production_theater_load=true\ntheater_tile_loader=true\n\
         theater_loaded=TEMPERATE\ntheater_ini_source={}\n\
         theater_tile_slots={}\ntheater_tilesets={}\n\
         theater_tile_images_loaded={}\n\
         terrain_tile_count={}\nterrain_instance_count=9\n\
         frames_presented={frames}\nrender_elapsed_ms={render_elapsed_ms}\n\
         a_button_presses={presses}\nexit_reason={exit_reason}\n\
         normal_exit=true\ngpu_probe_passed=true\n",
        instances.len(),
        theater.ini_source,
        theater.tile_slots,
        theater.tilesets,
        terrain_images.len(),
        terrain_images.len(),
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_main() -> i32 {
    if std::fs::create_dir_all(DIRECTORY).is_err() {
        return 1;
    }
    let Ok(file) = File::create(format!("{DIRECTORY}/gpu-wgpu.log")) else {
        return 1;
    };
    *LOGGER.0.lock().unwrap() = Some(file);
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);
    std::panic::set_hook(Box::new(|info| {
        log::error!("{info}");
        let _ = std::fs::write(
            format!("{DIRECTORY}/gpu-probe-nro.txt"),
            format!("gpu_probe_passed=false\npanic={info}\n"),
        );
    }));
    let (status, report) = match run() {
        Ok(report) => (0, report),
        Err(error) => {
            log::error!("{error:#}");
            (1, format!("gpu_probe_passed=false\nerror={error:#}\n"))
        }
    };
    if std::fs::write(format!("{DIRECTORY}/gpu-probe-nro.txt"), report).is_err() {
        return 1;
    }
    status
}
