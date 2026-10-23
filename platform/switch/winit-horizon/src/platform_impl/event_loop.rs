use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use crate::dpi::PhysicalPosition;
use crate::error::EventLoopError;
use crate::event::{ElementState, Event, KeyEvent, MouseButton, StartCause, WindowEvent};
use crate::event_loop::{ControlFlow, DeviceEvents, EventLoopClosed};
use crate::keyboard::{Key, KeyCode, KeyLocation, NamedKey, PhysicalKey};
use crate::window::{CustomCursor, CustomCursorSource, Theme};

use super::{
    DeviceId, KeyEventExtra, MonitorHandle, PlatformSpecificEventLoopAttributes, WindowId,
};

unsafe extern "C" {
    fn platform_running() -> bool;
    fn platform_buttons() -> u64;
    fn platform_buttons_held() -> u64;
    fn platform_touch(x: *mut u32, y: *mut u32) -> bool;
    fn platform_sticks(lx: *mut i32, ly: *mut i32, rx: *mut i32, ry: *mut i32);
    fn platform_focused() -> bool;
}

pub struct Shared {
    pub redraw: AtomicBool,
    pub window_created: AtomicBool,
    pub cursor_position: Mutex<PhysicalPosition<f64>>,
    pub focused: AtomicBool,
    exiting: AtomicBool,
    alive: AtomicBool,
    control_flow: Mutex<ControlFlow>,
}

pub struct ActiveEventLoop {
    pub shared: Arc<Shared>,
}

impl ActiveEventLoop {
    pub fn set_control_flow(&self, flow: ControlFlow) {
        *self.shared.control_flow.lock().unwrap() = flow;
    }
    pub fn control_flow(&self) -> ControlFlow {
        *self.shared.control_flow.lock().unwrap()
    }
    pub fn exit(&self) {
        self.shared.exiting.store(true, Ordering::Relaxed);
    }
    pub fn exiting(&self) -> bool {
        self.shared.exiting.load(Ordering::Relaxed)
    }
    pub fn listen_device_events(&self, _: DeviceEvents) {}
    pub fn system_theme(&self) -> Option<Theme> {
        None
    }
    pub fn available_monitors(&self) -> Vec<MonitorHandle> {
        vec![MonitorHandle]
    }
    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        Some(MonitorHandle)
    }
    pub fn owned_display_handle(&self) -> OwnedDisplayHandle {
        OwnedDisplayHandle
    }
    pub fn create_custom_cursor(&self, source: CustomCursorSource) -> CustomCursor {
        super::custom_cursor(source)
    }
    pub fn raw_display_handle_rwh_06(
        &self,
    ) -> Result<rwh_06::RawDisplayHandle, rwh_06::HandleError> {
        OwnedDisplayHandle.raw_display_handle_rwh_06()
    }
}

#[derive(Clone)]
pub struct OwnedDisplayHandle;
impl OwnedDisplayHandle {
    pub fn raw_display_handle_rwh_06(
        &self,
    ) -> Result<rwh_06::RawDisplayHandle, rwh_06::HandleError> {
        Ok(rwh_06::HorizonDisplayHandle::new().into())
    }
}

pub struct EventLoop<T: 'static> {
    active: crate::event_loop::ActiveEventLoop,
    sender: mpsc::Sender<T>,
    receiver: mpsc::Receiver<T>,
}

impl<T> EventLoop<T> {
    pub fn new(_: &mut PlatformSpecificEventLoopAttributes) -> Result<Self, EventLoopError> {
        let shared = Arc::new(Shared {
            redraw: AtomicBool::new(false),
            window_created: AtomicBool::new(false),
            cursor_position: Mutex::new(PhysicalPosition::new(640.0, 360.0)),
            focused: AtomicBool::new(true),
            exiting: AtomicBool::new(false),
            alive: AtomicBool::new(true),
            control_flow: Mutex::new(ControlFlow::Wait),
        });
        let (sender, receiver) = mpsc::channel();
        Ok(Self {
            active: crate::event_loop::ActiveEventLoop {
                p: ActiveEventLoop { shared },
                _marker: PhantomData,
            },
            sender,
            receiver,
        })
    }

    pub fn window_target(&self) -> &crate::event_loop::ActiveEventLoop {
        &self.active
    }
    pub fn create_proxy(&self) -> EventLoopProxy<T> {
        EventLoopProxy {
            sender: self.sender.clone(),
            shared: self.active.p.shared.clone(),
        }
    }

    pub fn run(
        self,
        mut callback: impl FnMut(Event<T>, &crate::event_loop::ActiveEventLoop),
    ) -> Result<(), EventLoopError> {
        let active = &self.active;
        callback(Event::NewEvents(StartCause::Init), active);
        callback(Event::Resumed, active);
        let mut input = Input::new();
        active.p.shared.focused.store(input.focused, Ordering::Relaxed);
        if active.p.shared.window_created.load(Ordering::Relaxed) && !active.exiting() {
            for event in [
                WindowEvent::Focused(input.focused),
                WindowEvent::CursorEntered {
                    device_id: crate::event::DeviceId(DeviceId(1)),
                },
                WindowEvent::CursorMoved {
                    device_id: crate::event::DeviceId(DeviceId(1)),
                    position: *active.p.shared.cursor_position.lock().unwrap(),
                },
            ] {
                callback(window_event(event), active);
            }
        }
        let mut wait_start = Instant::now();
        while !active.exiting() {
            if !unsafe { platform_running() } {
                // The OS has requested exit, so a modal close confirmation
                // cannot retain the process past its lifecycle deadline.
                active.exit();
                break;
            }
            let events = input.poll(&active.p.shared);
            let user_events: Vec<T> = self.receiver.try_iter().collect();
            let now = Instant::now();
            let flow = active.control_flow();
            let timer_ready = matches!(flow, ControlFlow::WaitUntil(deadline) if now >= deadline);
            let redraw = active.p.shared.redraw.load(Ordering::Relaxed);
            if events.is_empty()
                && user_events.is_empty()
                && !timer_ready
                && !redraw
                && flow != ControlFlow::Poll
            {
                let wait = match flow {
                    ControlFlow::WaitUntil(deadline) => deadline.saturating_duration_since(now),
                    _ => Duration::from_millis(4),
                };
                std::thread::sleep(wait.min(Duration::from_millis(4)));
                continue;
            }
            let cause = match flow {
                ControlFlow::Poll => StartCause::Poll,
                ControlFlow::WaitUntil(requested_resume) if timer_ready => {
                    StartCause::ResumeTimeReached {
                        start: wait_start,
                        requested_resume,
                    }
                }
                flow => StartCause::WaitCancelled {
                    start: wait_start,
                    requested_resume: match flow {
                        ControlFlow::WaitUntil(deadline) => Some(deadline),
                        _ => None,
                    },
                },
            };
            callback(Event::NewEvents(cause), active);
            for event in events {
                if active.exiting() {
                    break;
                }
                callback(window_event(event), active);
            }
            for event in user_events {
                if active.exiting() {
                    break;
                }
                callback(Event::UserEvent(event), active);
            }
            if !active.exiting() && active.p.shared.redraw.swap(false, Ordering::Relaxed) {
                callback(window_event(WindowEvent::RedrawRequested), active);
            }
            if !active.exiting() {
                callback(Event::AboutToWait, active);
            }
            wait_start = Instant::now();
        }
        callback(Event::LoopExiting, active);
        Ok(())
    }
}

impl<T> Drop for EventLoop<T> {
    fn drop(&mut self) {
        self.active.p.shared.alive.store(false, Ordering::Release);
    }
}

pub struct EventLoopProxy<T> {
    sender: mpsc::Sender<T>,
    shared: Arc<Shared>,
}
impl<T> Clone for EventLoopProxy<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            shared: self.shared.clone(),
        }
    }
}
impl<T> EventLoopProxy<T> {
    pub fn send_event(&self, value: T) -> Result<(), EventLoopClosed<T>> {
        if !self.shared.alive.load(Ordering::Acquire) {
            return Err(EventLoopClosed(value));
        }
        self.sender
            .send(value)
            .map_err(|error| EventLoopClosed(error.0))
    }
}

fn window_event<T>(event: WindowEvent) -> Event<T> {
    Event::WindowEvent {
        window_id: crate::window::WindowId(WindowId(1)),
        event,
    }
}

struct Input {
    held: u64,
    left: bool,
    right: bool,
    position: PhysicalPosition<f64>,
    focused: bool,
    last_sample: Instant,
}

impl Input {
    fn new() -> Self {
        Self {
            held: 0,
            left: false,
            right: false,
            position: PhysicalPosition::new(640.0, 360.0),
            focused: unsafe { platform_focused() },
            last_sample: Instant::now(),
        }
    }

    fn poll(&mut self, shared: &Shared) -> Vec<WindowEvent> {
        let mut events = Vec::new();
        let device_id = crate::event::DeviceId(DeviceId(1));
        let now = Instant::now();
        let dt = now.duration_since(self.last_sample).as_secs_f64().min(0.05);
        self.last_sample = now;
        unsafe { platform_buttons() };
        let focused = unsafe { platform_focused() };
        shared.focused.store(focused, Ordering::Relaxed);
        let held = if focused {
            unsafe { platform_buttons_held() }
        } else {
            0
        };
        if focused != self.focused {
            events.push(WindowEvent::Focused(focused));
            self.focused = focused;
        }
        let (mut x, mut y) = (0, 0);
        let touched = focused && unsafe { platform_touch(&mut x, &mut y) };
        {
            let mut position = shared.cursor_position.lock().unwrap();
            if touched {
                *position = PhysicalPosition::new(x.min(1279) as f64, y.min(719) as f64);
            } else if focused {
                let (mut lx, mut ly, mut rx, mut ry) = (0, 0, 0, 0);
                unsafe { platform_sticks(&mut lx, &mut ly, &mut rx, &mut ry) };
                position.x = (position.x + stick_axis(lx) * 850.0 * dt).clamp(0.0, 1279.0);
                position.y = (position.y - stick_axis(ly) * 850.0 * dt).clamp(0.0, 719.0);
            }
            if *position != self.position {
                self.position = *position;
                events.push(WindowEvent::CursorMoved {
                    device_id,
                    position: *position,
                });
            }
        }
        let left = touched || held & 1 != 0;
        let right = held & (1 << 1) != 0;
        for (previous, current, button) in [
            (self.left, left, MouseButton::Left),
            (self.right, right, MouseButton::Right),
        ] {
            if previous != current {
                events.push(WindowEvent::MouseInput {
                    device_id,
                    state: if current {
                        ElementState::Pressed
                    } else {
                        ElementState::Released
                    },
                    button,
                });
            }
        }
        self.left = left;
        self.right = right;
        for (bit, code, logical) in [
            (2, KeyCode::KeyD, Key::Character("d".into())),
            (3, KeyCode::KeyA, Key::Character("a".into())),
            (6, KeyCode::ShiftLeft, Key::Named(NamedKey::Shift)),
            (7, KeyCode::ControlLeft, Key::Named(NamedKey::Control)),
            (8, KeyCode::Tab, Key::Named(NamedKey::Tab)),
            (9, KeyCode::Space, Key::Named(NamedKey::Space)),
            (10, KeyCode::Escape, Key::Named(NamedKey::Escape)),
            (11, KeyCode::F10, Key::Named(NamedKey::F10)),
            (12, KeyCode::ArrowLeft, Key::Named(NamedKey::ArrowLeft)),
            (13, KeyCode::ArrowUp, Key::Named(NamedKey::ArrowUp)),
            (14, KeyCode::ArrowRight, Key::Named(NamedKey::ArrowRight)),
            (15, KeyCode::ArrowDown, Key::Named(NamedKey::ArrowDown)),
        ] {
            if (self.held ^ held) & (1 << bit) != 0 {
                events.push(WindowEvent::KeyboardInput {
                    device_id,
                    event: KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        logical_key: logical.clone(),
                        text: None,
                        location: KeyLocation::Standard,
                        state: if held & (1 << bit) != 0 {
                            ElementState::Pressed
                        } else {
                            ElementState::Released
                        },
                        repeat: false,
                        platform_specific: KeyEventExtra {
                            key_without_modifiers: logical,
                            text_with_all_modifiers: None,
                        },
                    },
                    is_synthetic: false,
                });
            }
        }
        self.held = held;
        events
    }
}

fn stick_axis(value: i32) -> f64 {
    let axis = (f64::from(value) / 32767.0).clamp(-1.0, 1.0);
    if axis.abs() <= 0.15 {
        0.0
    } else {
        axis.signum() * (axis.abs() - 0.15) / 0.85
    }
}
