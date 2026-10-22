# Switch runtime sources

The custom target and the TLS bridge are adapted from
[JohnUzoka/bevy-horizon](https://github.com/JohnUzoka/bevy-horizon), commit
`3eac779b232ee47cd0eb957676c64386ac7b0f1d`, under MIT (license in `third_party/`).
The original target includes a TLS-aligned libnx linker script. Only the
`threadCreate` wrapper is currently used; rendering and filesystem compatibility
wrappers have not been imported.

The local build links through devkitA64's GNU driver. It page-aligns `.tdata`
and checks the final ELF's PT_TLS alignment after linking, rather than relying
on the original LLD script's early alignment assertion. Main and worker thread
TLS were exercised by std, Rayon and the production core in Eden.

Builds use that checkout's `patches/libc` (ARM64 newlib ABI) and
`patches/memmap2` (heap-backed read-only mappings). Those directories retain
their own upstream license files. Patches are enabled only through the generated
Switch Cargo config, including for the `build-std` workspace.

The minimum C NVK probe under `third_party/bevy-horizon-nvk-probe/` comes from
the same project at local commit `7a656fd` on top of `3eac779b`, under MIT.
`7a656fd` adds direct devkitA64 builds without Docker and stage markers around
device, swapchain and pipeline creation. The probe is not part of the Vera20k
game; it separates NVK and emulator failures from wgpu and production renderer
failures.

`third_party/eden-local-ffmpeg.patch` is a local build compatibility patch for
Eden master `989768aa46720e3fcab9e2111e3ddd70819c382b` (GPL-3.0-or-later).
It compares the native host and target names as strings and uses the C compiler
driver for FFmpeg's link checks. It permits using Eden's pinned FFmpeg source
instead of the prebuilt FFmpeg package whose VA-API dependencies are unavailable
on this Ubuntu 22.04 workstation. This patch does not change emulated GPU behavior.

`third_party/eden-cli-lifecycle.patch` targets the same Eden commit. The source
file `src/yuzu_cmd/yuzu.cpp` declares GPL-2.0-or-later. This local patch retains
SDL3 callback state and the input subsystem, returns the continuation result
during startup and events, captures callback state by value, and handles core
exit requests and partial initialization during cleanup. Both Eden patches are
preserved here for reproduction; the Eden checkout remains uncommitted and
nothing has been submitted upstream. Neither patch adds GPU instruction support.

The resulting local CLI, with macro JIT and macro HLE disabled in its isolated
configuration, ran the NVK probe for 30 seconds with successful acquire/present
and automatic exit. The visible result was only the clear colour: Eden rejected
the shader with `Instruction IMUL (imm) is not implemented`, so this is not a
successful Vulkan triangle or production Vera renderer test.
