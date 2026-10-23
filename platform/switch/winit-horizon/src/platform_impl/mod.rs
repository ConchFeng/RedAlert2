//! libnx backend for the winit 0.30 public event and window API.

#[cfg(not(target_os = "horizon"))]
compile_error!("this winit backend is only for Horizon");
#[cfg(any(feature = "rwh_04", feature = "rwh_05"))]
compile_error!("the Horizon backend requires raw-window-handle 0.6");

mod event_loop;
mod window;

use std::sync::Arc;

use crate::dpi::{PhysicalPosition, PhysicalSize};
use crate::monitor::{MonitorHandle as RootMonitorHandle, VideoModeHandle as RootVideoModeHandle};
use crate::window::Fullscreen as RootFullscreen;

pub(crate) use crate::cursor::{
    OnlyCursorImage as PlatformCustomCursor, OnlyCursorImageSource as PlatformCustomCursorSource,
};
pub(crate) use crate::icon::NoIcon as PlatformIcon;
pub use event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy, OwnedDisplayHandle, Shared};
pub use window::Window;
pub type OsError = std::io::Error;

#[derive(Default, Debug, Clone)]
pub struct PlatformSpecificWindowAttributes;
#[derive(Default)]
pub struct PlatformSpecificEventLoopAttributes;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(pub u64);
impl WindowId {
    pub const fn dummy() -> Self {
        Self(0)
    }
}
impl From<u64> for WindowId {
    fn from(value: u64) -> Self {
        Self(value)
    }
}
impl From<WindowId> for u64 {
    fn from(value: WindowId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DeviceId(pub u64);
impl DeviceId {
    pub const fn dummy() -> Self {
        Self(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyEventExtra {
    pub key_without_modifiers: crate::keyboard::Key,
    pub text_with_all_modifiers: Option<smol_str::SmolStr>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MonitorHandle;
impl MonitorHandle {
    pub fn name(&self) -> Option<String> {
        Some("Switch display".into())
    }
    pub fn size(&self) -> PhysicalSize<u32> {
        PhysicalSize::new(1280, 720)
    }
    pub fn position(&self) -> PhysicalPosition<i32> {
        PhysicalPosition::new(0, 0)
    }
    pub fn scale_factor(&self) -> f64 {
        1.0
    }
    pub fn refresh_rate_millihertz(&self) -> Option<u32> {
        Some(60_000)
    }
    pub fn video_modes(&self) -> impl Iterator<Item = VideoModeHandle> {
        std::iter::once(VideoModeHandle)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VideoModeHandle;
impl VideoModeHandle {
    pub fn size(&self) -> PhysicalSize<u32> {
        PhysicalSize::new(1280, 720)
    }
    pub fn bit_depth(&self) -> u16 {
        32
    }
    pub fn refresh_rate_millihertz(&self) -> u32 {
        60_000
    }
    pub fn monitor(&self) -> MonitorHandle {
        MonitorHandle
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fullscreen {
    Exclusive(VideoModeHandle),
    Borderless(Option<MonitorHandle>),
}
impl From<RootFullscreen> for Fullscreen {
    fn from(value: RootFullscreen) -> Self {
        match value {
            RootFullscreen::Exclusive(mode) => Self::Exclusive(mode.video_mode),
            RootFullscreen::Borderless(monitor) => Self::Borderless(monitor.map(|m| m.inner)),
        }
    }
}
impl From<Fullscreen> for RootFullscreen {
    fn from(value: Fullscreen) -> Self {
        match value {
            Fullscreen::Exclusive(video_mode) => {
                Self::Exclusive(RootVideoModeHandle { video_mode })
            }
            Fullscreen::Borderless(monitor) => {
                Self::Borderless(monitor.map(|inner| RootMonitorHandle { inner }))
            }
        }
    }
}

pub(crate) fn custom_cursor(
    source: crate::window::CustomCursorSource,
) -> crate::window::CustomCursor {
    crate::window::CustomCursor {
        inner: PlatformCustomCursor(Arc::new(source.inner.0)),
    }
}
