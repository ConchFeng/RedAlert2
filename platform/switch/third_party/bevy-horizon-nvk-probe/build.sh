#!/usr/bin/env bash
set -euo pipefail

# Usage: probes/nvk_probe/build.sh
#   Builds target/nvk_probe/nvk_probe.nro: a plain C program that drives NVK (mesa-switch's
#   Vulkan driver) directly and logs what it finds to sdmc:/switch/nvk_probe/log.txt, then draws
#   a spinning triangle. It answers "does NVK work on this console, and what does it support?"
#   without wgpu or Bevy in the way.
#
#   Needs nvk/lib/libvulkan_bh.a: scripts/build_nvk.sh, then scripts/localize_nvk.sh.
#   Set DEVKITPRO to use the current host's devkitA64 directly; otherwise the script uses the
#   bevy-horizon Docker image.
#   The shaders are prebuilt into tri_vert.h and tri_frag.h; to rebuild them after editing
#   tri.vert or tri.frag (glslang-tools, Khronos' reference compiler):
#     glslangValidator -V --vn tri_vert_spv -o tri_vert.h tri.vert
#     glslangValidator -V --vn tri_frag_spv -o tri_frag.h tri.frag
PROBE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "$PROBE_DIR/../.." && pwd)"
IMAGE="bevy-horizon-toolchain:local"
# Docker or Podman: CONTAINER_ENGINE picks one, otherwise docker if it's installed.
ENGINE="${CONTAINER_ENGINE:-$(command -v docker >/dev/null && echo docker || echo podman)}"
OUT="$WORKSPACE_DIR/target/nvk_probe"

if [ ! -f "$WORKSPACE_DIR/nvk/lib/libvulkan_bh.a" ]; then
    echo "error: nvk/lib/libvulkan_bh.a is missing; run scripts/build_nvk.sh and scripts/localize_nvk.sh" >&2
    exit 1
fi
mkdir -p "$OUT"

run_build() {
    bash -euo pipefail -c "$1"
}

BUILD_TEMPLATE='
    export PATH=@DEVKITPRO@/devkitA64/bin:@DEVKITPRO@/tools/bin:$PATH
    # The flags libnx'\''s template Makefile uses.
    ARCH="-march=armv8-a+crc+crypto -mtune=cortex-a57 -mtp=soft -fPIE"
    aarch64-none-elf-gcc $ARCH -D__SWITCH__ -O2 -g -Wall -Wextra -Wno-unused-parameter \
        -ffunction-sections -fdata-sections \
        -I@DEVKITPRO@/libnx/include -I@NVK@/include \
        -c @PROBE_DIR@/main.c -o @OUT@/main.o
    # --wrap=threadCreate: NAK is Rust and finds its thread-locals through TPIDR_EL0, which libnx
    # never sets; main.c sets it on every thread, as bevy_horizon does for Rust apps.
    aarch64-none-elf-gcc $ARCH -specs=@DEVKITPRO@/libnx/switch.specs -g \
        -Wl,--gc-sections -Wl,--wrap=threadCreate -Wl,-Map,@OUT@/nvk_probe.map \
        @OUT@/main.o \
        -L@NVK@/lib -lvulkan_bh \
        -L@DEVKITPRO@/portlibs/switch/lib -lexpat -lzstd -lz -lstdc++ \
        -L@DEVKITPRO@/libnx/lib -lnx -lm \
        -o @OUT@/nvk_probe.elf
    nacptool --create "nvk_probe" "bevy-horizon" "0.1.0" @OUT@/nvk_probe.nacp
    elf2nro @OUT@/nvk_probe.elf @OUT@/nvk_probe.nro --nacp=@OUT@/nvk_probe.nacp
'

if [ -n "${DEVKITPRO:-}" ]; then
    DEVKITPRO="$(cd "$DEVKITPRO" && pwd)"
    export DEVKITPRO DEVKITA64="$DEVKITPRO/devkitA64"
    BUILD_COMMAND="${BUILD_TEMPLATE//@DEVKITPRO@/$DEVKITPRO}"
    BUILD_COMMAND="${BUILD_COMMAND//@NVK@/$WORKSPACE_DIR/nvk}"
    BUILD_COMMAND="${BUILD_COMMAND//@PROBE_DIR@/$PROBE_DIR}"
    BUILD_COMMAND="${BUILD_COMMAND//@OUT@/$OUT}"
    run_build "$BUILD_COMMAND"
else
    BUILD_COMMAND="${BUILD_TEMPLATE//@DEVKITPRO@//opt/devkitpro}"
    BUILD_COMMAND="${BUILD_COMMAND//@NVK@//nvk}"
    BUILD_COMMAND="${BUILD_COMMAND//@PROBE_DIR@//probe}"
    BUILD_COMMAND="${BUILD_COMMAND//@OUT@//out}"
    "$ENGINE" run --rm \
        -v "$PROBE_DIR:/probe:ro,z" \
        -v "$WORKSPACE_DIR/nvk:/nvk:ro,z" \
        -v "$OUT:/out:z" \
        "$IMAGE" bash -euo pipefail -c "$BUILD_COMMAND"
fi
ls -la "$OUT/nvk_probe.nro"
