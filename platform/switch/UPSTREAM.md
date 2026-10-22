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
