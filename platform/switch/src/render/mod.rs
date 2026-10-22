//! Production renderer modules compiled into the Switch NRO.
//!
//! The files are shared with the desktop client through `#[path]`, so the
//! Switch path exercises the same GPU and batch implementations instead of a
//! separate renderer. Only the desktop app/orchestration modules are omitted.

#[path = "../../../../src/render/atlas_growth.rs"]
pub(crate) mod atlas_growth;
#[path = "../../../../src/render/batch.rs"]
pub mod batch;
#[path = "../../../../src/render/draw_state.rs"]
pub mod draw_state;
pub mod gpu {
    pub use crate::gpu::*;
}
#[path = "../../../../src/render/native_surface_format.rs"]
pub mod native_surface_format;
#[path = "../../../../src/render/native_z.rs"]
pub mod native_z;
#[path = "../../../../src/render/palette_light.rs"]
pub mod palette_light;
#[path = "../../../../src/render/shroud_buffer.rs"]
pub mod shroud_buffer;
#[path = "../../../../src/render/tactical_draw_plan.rs"]
pub mod tactical_draw_plan;
#[path = "../../../../src/render/tactical_shader.rs"]
pub(crate) mod tactical_shader;
#[path = "../../../../src/render/terrain_instances.rs"]
pub mod terrain_instances;
#[path = "../../../../src/render/tile_atlas.rs"]
pub mod tile_atlas;
