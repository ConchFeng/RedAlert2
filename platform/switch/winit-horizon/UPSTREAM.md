# Horizon window backend

The public API files under `src/` are copied from winit 0.30.12, distributed under
the included Apache-2.0 license. The local Cargo registry source was used; this
fork keeps its window, event, keyboard, monitor and application types so the
existing VERA20k `App` compiles without a second frontend. `dpi` remains a normal
upstream dependency.

The manifest, build script, platform module selection and `src/platform_impl/`
are local Horizon integration. Desktop X11/Wayland dependencies are excluded.
This crate is selected only by the standalone Switch build's generated Cargo
patch; desktop builds continue using upstream winit.

The backend supports one process-owned libnx default NWindow at 1280x720,
raw-window-handle 0.6 with the existing Horizon patch, redraw requests, timed
wakes, user events, focus changes and OS exit. Input is sampled through the
Switch runtime's C bridge. The left stick moves the software pointer; touch
and A produce left mouse input, B produces right mouse input, and the D-pad
produces arrow keys. X/Y map to D/A, L/R to Shift/Ctrl, ZL/ZR to Tab/Space,
and Plus/Minus to Escape/F10.

The ordinary shared client handles the resulting events. Production, selection,
commands, menus and save state remain owned by the existing client and engine.
Desktop window movement, resizing, decorations and hardware cursors do not
apply to this fixed native surface. This backend has no probe timeout.
