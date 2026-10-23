//! Production renderer modules compiled into the Switch NRO.
//!
//! The files are shared with the desktop client through `#[path]`, so the
//! Switch path exercises the same GPU and batch implementations instead of a
//! separate renderer. Only the desktop app/orchestration modules are omitted.

#[path = "../../../../src/render/atlas_growth.rs"]
pub(crate) mod atlas_growth;
#[path = "../../../../src/render/batch.rs"]
pub mod batch;
#[path = "../../../../src/render/bit_font.rs"]
pub mod bit_font;
#[path = "../../../../src/render/draw_state.rs"]
pub mod draw_state;
pub mod gpu {
    pub use crate::gpu::*;
}
#[path = "../../../../src/render/locomotor_visual.rs"]
pub mod locomotor_visual;
#[path = "../../../../src/render/native_surface_format.rs"]
pub mod native_surface_format;
#[path = "../../../../src/render/native_z.rs"]
pub mod native_z;
#[path = "../../../../src/render/object_shp_frame.rs"]
pub(crate) mod object_shp_frame;
#[path = "../../../../src/render/palette_light.rs"]
pub mod palette_light;
#[path = "../../../../src/render/palette_textures.rs"]
pub mod palette_textures;
#[path = "../../../../src/render/shroud_buffer.rs"]
pub mod shroud_buffer;
#[path = "../../../../src/render/shell_text_reveal.rs"]
pub mod shell_text_reveal;
#[path = "../../../../src/render/sprite_atlas.rs"]
pub mod sprite_atlas;
#[path = "../../../../src/render/sidebar_cameo_atlas.rs"]
pub mod sidebar_cameo_atlas;
#[path = "../../../../src/render/sidebar_chrome.rs"]
pub mod sidebar_chrome;
#[path = "../../../../src/render/sidebar_text.rs"]
pub mod sidebar_text;
#[path = "../../../../src/render/tactical_draw_plan.rs"]
pub mod tactical_draw_plan;
#[path = "../../../../src/render/tactical_shader.rs"]
pub(crate) mod tactical_shader;
#[path = "../../../../src/render/terrain_instances.rs"]
pub mod terrain_instances;
#[path = "../../../../src/render/tile_atlas.rs"]
pub mod tile_atlas;
#[path = "../../../../src/render/unit_atlas.rs"]
pub mod unit_atlas;
#[path = "../../../../src/render/unit_parts.rs"]
pub(crate) mod unit_parts;
#[path = "../../../../src/render/vxl_normals.rs"]
pub mod vxl_normals;
#[path = "../../../../src/render/vxl_raster.rs"]
pub mod vxl_raster;
