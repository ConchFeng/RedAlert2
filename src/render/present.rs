//! One reporting point for all successfully submitted surface frames.

pub(crate) fn present(output: crate::render::gpu::GpuFrame) {
    #[cfg(target_os = "horizon")]
    let size = (output.texture.width(), output.texture.height());
    output.present();
    #[cfg(target_os = "horizon")]
    unsafe {
        unsafe extern "C" {
            fn platform_report_present(width: u32, height: u32);
        }
        platform_report_present(size.0, size.1);
    }
}
