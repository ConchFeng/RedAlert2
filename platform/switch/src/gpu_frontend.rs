//! Exercise production sprite batching through libnx's native window and NVK.

use std::ffi::c_void;
use std::fs::File;
use std::io::Write;
use std::num::NonZeroUsize;
use std::ptr::NonNull;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use wgpu::rwh::{
    DisplayHandle, HasDisplayHandle, HasWindowHandle, HorizonWindowHandle, WindowHandle,
};
use wgpu::util::DeviceExt;

use crate::gpu::{GpuContext, NativeWindow};
use crate::render::batch::{
    BatchRenderer, BatchTexture, DepthAxis, InstanceBufferPool, SpriteInstance,
};
use crate::render::palette_light::PaletteLight;

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
         frames_presented={frames}\nrender_elapsed_ms={render_elapsed_ms}\n\
         a_button_presses={presses}\nexit_reason={exit_reason}\n\
         normal_exit=true\ngpu_probe_passed=true\n",
        instances.len()
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
