//! Exercise the production GPU context through libnx's native window and NVK.

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

use crate::gpu::{GpuContext, NativeWindow};

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

fn run() -> Result<String> {
    unsafe { platform_console_init() };
    let address = NonZeroUsize::new(unsafe { platform_window_init() } as usize)
        .context("libnx default window initialization failed")?;
    log::info!("native_window_ready");
    let gpu = GpuContext::new_with_native_window(HorizonWindow { address })?;
    log::info!("production_gpu_context_ready");
    let shader = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Switch native-window validation"),
            source: wgpu::ShaderSource::Wgsl(
                r#"
            struct Vertex { @builtin(position) position: vec4<f32>, @location(0) colour: vec3<f32> }
            @vertex fn vertex(@builtin(vertex_index) index: u32) -> Vertex {
                let positions = array(vec2(0.0, 0.65), vec2(-0.65, -0.65), vec2(0.65, -0.65));
                let colours = array(vec3(1.0, 0.0, 0.0), vec3(0.0, 1.0, 0.0), vec3(0.0, 0.0, 1.0));
                return Vertex(vec4(positions[index], 0.0, 1.0), colours[index]);
            }
            @fragment fn fragment(input: Vertex) -> @location(0) vec4<f32> {
                return vec4(input.colour, 1.0);
            }
            "#
                .into(),
            ),
        });
    let pipeline = gpu
        .device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Switch native-window triangle"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(gpu.surface_format.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
    log::info!("wgsl_pipeline_ready");
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
                ..Default::default()
            });
            pass.set_pipeline(&pipeline);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit([commands.finish()]);
        frame.present();
        frames += 1;
        if frames == 1 || frames % 300 == 0 {
            log::info!("frames_presented={frames}");
        }
    }
    anyhow::ensure!(frames > 0, "no frames presented");
    Ok(format!(
        "binary_format=nro\nproduction_gpu_context=true\nwgsl_pipeline=true\n\
         frames_presented={frames}\na_button_presses={presses}\nexit_reason={exit_reason}\n\
         normal_exit=true\ngpu_probe_passed=true\n"
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
