#!/usr/bin/env python3
"""Cross-build the production VERA20k core or GPU context with Horizon std/newlib."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from nsp import TITLE_ID, package

ROOT = Path(__file__).resolve().parent
WORKSPACE = ROOT.parents[3]
TOOLCHAIN = "nightly-2026-10-04"
UPSTREAM_COMMIT = "3eac779b232ee47cd0eb957676c64386ac7b0f1d"


def run(*arguments, **kwargs):
    print("+", " ".join(map(str, arguments)), flush=True)
    return subprocess.run(list(map(str, arguments)), cwd=ROOT, check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Compile-check without linking")
    parser.add_argument("--nsp", action="store_true", help="Also package a standalone NSP with local device keys")
    parser.add_argument("--gpu", action="store_true",
                        help="Build the native-window probe using Vera's production GpuContext and wgpu")
    parser.add_argument("--host-test", action="store_true", help="Run the same core probe on the host")
    parser.add_argument("--check-libc-abi", action="store_true", help="Compare patched libc against SDK headers")
    parser.add_argument("--check-client-deps", action="store_true",
                        help="Audit the desktop client's graphics/window/audio dependencies for Horizon")
    parser.add_argument("--without-winit", action="store_true",
                        help="Audit Vulkan/audio for a native Switch frontend instead of the desktop window stack")
    args = parser.parse_args()
    if args.gpu and args.nsp:
        parser.error("--gpu currently produces a separate NRO; omit --nsp")
    os.chdir(ROOT)
    rust_bin = Path.home() / ".cargo/bin"
    os.environ["PATH"] = str(rust_bin) + os.pathsep + os.environ["PATH"]
    pinned = WORKSPACE / "toolchains/switch/bevy-horizon-pinned"
    default_upstream = pinned if pinned.is_dir() else WORKSPACE / "toolchains/switch/bevy-horizon-reference"
    upstream = Path(os.environ.get("VERA_SWITCH_UPSTREAM", default_upstream))
    if not upstream.is_dir():
        raise SystemExit("Missing bevy-horizon reference; see README.md")
    revision = subprocess.check_output(["git", "-C", str(upstream), "rev-parse", "HEAD"], text=True).strip()
    if revision != UPSTREAM_COMMIT:
        raise SystemExit(f"Expected upstream {UPSTREAM_COMMIT}, found {revision}; preserving checkout")
    sdk = Path(os.environ.get("DEVKITPRO", WORKSPACE / "toolchains/switch/sdk/opt/devkitpro")).resolve()
    os.environ["DEVKITPRO"] = str(sdk)
    os.environ["PATH"] = f"{sdk}/devkitA64/bin:{sdk}/tools/bin:" + os.environ["PATH"]
    work = ROOT / "target/switch-build"
    work.mkdir(parents=True, exist_ok=True)
    config = work / "cargo.toml"
    patches = {name: upstream / "patches" / name for name in ["libc", "memmap2"]}
    if args.check_client_deps:
        patches["cpal"] = upstream / "patches/cpal"
    if args.check_client_deps or args.gpu:
        patches["raw-window-handle"] = upstream / "patches/raw-window-handle"
        for name in ["wgpu", "wgpu-core", "wgpu-hal", "wgpu-core-deps-windows-linux-android"]:
            patches[name] = upstream / "patches/bevy-0.18" / name
    config.write_text("[patch.crates-io]\n" + "\n".join(
        f'{name} = {{ path = {json.dumps(str(path))} }}' for name, path in patches.items()) + "\n")
    if args.check_libc_abi:
        # Reuse the pinned upstream checker rather than maintaining a second
        # list of C/Rust layouts. Run its Python body locally, without Docker.
        source = (upstream / "scripts/check_libc_abi.sh").read_text()
        source = source.split("python3 - <<'PYIN'\n", 1)[1].rsplit("\nPYIN", 1)[0]
        source = source.replace("/workspace", str(upstream)).replace("/opt/devkitpro", str(sdk))
        source = source.replace(str(upstream / "target/libc-abi-check"), str(ROOT / "target/libc-abi-check"))
        checker = work / "check_libc_abi.py"
        checker.write_text(source)
        run(sys.executable, checker, env=dict(os.environ, RUSTUP_TOOLCHAIN=TOOLCHAIN))
        return
    if args.host_test:
        run("cargo", f"+{TOOLCHAIN}", "test", "--lib", "--config", config, "--", "--nocapture")
        return
    sysroot = Path(subprocess.check_output(["rustc", f"+{TOOLCHAIN}", "--print", "sysroot"], text=True).strip())
    std_manifest = sysroot / "lib/rustlib/src/rust/library/Cargo.toml"
    # build-std uses a separate workspace; patching only the app would silently
    # compile std against libc's 3DS ABI. Its lock must use the ARM64 patch too.
    run("cargo", f"+{TOOLCHAIN}", "update", "--manifest-path", std_manifest,
        "--config", config, "-p", "libc")
    target = ROOT / "aarch64-nintendo-horizon.json"
    manifest = []
    if args.check_client_deps:
        probe = work / "client-deps"
        (probe / "src").mkdir(parents=True, exist_ok=True)
        desktop = (ROOT.parents[1] / "Cargo.toml").read_text()
        desktop_lock = (ROOT.parents[1] / "Cargo.lock").read_text()
        versions = {name: re.search(r'\nname = "' + name + r'"\nversion = "([^"]+)"',
                                    desktop_lock).group(1)
                    for name in ["wgpu", "winit", "rodio"]}
        dependencies = [line for line in desktop.splitlines()
                        if re.match(r"^(wgpu|winit|rodio)\s*=", line)]
        dependencies = [re.sub(r'(?<=")\d[^\"]*(?=")',
                               "=" + versions[line.split("=", 1)[0].strip()], line, count=1)
                        for line in dependencies]
        if args.without_winit:
            dependencies = [line for line in dependencies if not line.startswith("winit")]
            dependencies = [
                f'wgpu = {{ version = "={versions["wgpu"]}", default-features = false, features = ["vulkan", "wgsl"] }}'
                if line.startswith("wgpu") else line for line in dependencies]
        (probe / "Cargo.toml").write_text(
            '[package]\nname="vera20k-client-deps-probe"\nversion="0.0.0"\nedition="2024"\n'
            '[workspace]\n[dependencies]\n' + "\n".join(dependencies) + "\n")
        (probe / "src/lib.rs").write_text("// Compile the actual client dependency versions, without desktop app code.\n")
        manifest = ["--manifest-path", probe / "Cargo.toml"]
        os.environ["CARGO_TARGET_DIR"] = str(ROOT / "target")
    features = ["--features", "gpu"] if args.gpu else []
    run("cargo", f"+{TOOLCHAIN}", "check" if args.check or args.check_client_deps else "build", *manifest, *features,
        "--release", "--config", config, "--target", target,
        "-Zbuild-std=std,panic_abort", "-Zjson-target-spec", "--lib")
    if args.check or args.check_client_deps:
        return
    settings = json.loads(target.read_text())
    linker_script = work / "switch.ld"
    # Upstream uses LLD. GNU ld evaluates the self-referential ALIGNOF here
    # differently; page-align TLS explicitly and keep the alignment assertion.
    script = settings["link-script"].replace(
        ".tdata ALIGN(MAX(ALIGNOF(.tdata), ALIGNOF(.tbss)))",
        ".tdata ALIGN(0x1000)")
    # GNU ld can evaluate this assertion before its final relaxation/layout.
    # Check the actual PT_TLS in the finished ELF instead (below).
    script = "\n".join(line for line in script.splitlines()
                       if "ASSERT(ADDR(.tdata)" not in line) + "\n"
    linker_script.write_text(script)
    specs = work / "switch.specs"
    original = (sdk / "libnx/switch.specs").read_text()
    original = original.replace("%:getenv(DEVKITPRO /libnx/switch.ld)", str(linker_script))
    original = original.replace("--nx-module-name", "--nx-module-name --wrap=threadCreate")
    specs.write_text(original)
    cc = sdk / "devkitA64/bin/aarch64-none-elf-gcc"
    artifact = "vera20k-gpu" if args.gpu else "vera20k-core"
    graphics = ["-DVERA_WGPU"] if args.gpu else []
    architecture = ["-march=armv8-a+crc+crypto", "-mtune=cortex-a57", "-mtp=soft", "-fPIE"]
    run(cc, *architecture, *graphics, "-O2", "-g", "-Wall", "-Wextra", "-Werror", "-D__SWITCH__",
        "-ffunction-sections", "-fdata-sections", "-I", sdk / "libnx/include",
        "-I", sdk / "portlibs/switch/include",
        "-c", "runtime.c", "-o", work / "runtime.o")
    elf = work / f"{artifact}.elf"
    if args.gpu:
        nvk = Path(os.environ.get(
            "VERA_SWITCH_NVK_LIB", WORKSPACE / "toolchains/switch/bevy-horizon-reference/nvk/lib"))
        if not (nvk / "libvulkan_bh.a").is_file():
            raise SystemExit("Missing localized NVK archive; see HANDOFF.md")
        libraries = ["-L", nvk, "-Wl,--start-group", "-lvulkan_bh", "-lexpat", "-lzstd", "-lz",
                     "-lnx", "-Wl,--end-group"]
    else:
        libraries = ["-Wl,--start-group", "-lEGL", "-lGLESv2", "-lglapi", "-ldrm_nouveau", "-lnx",
                     "-Wl,--end-group"]
    run(cc, *architecture, f"-specs={specs}", "-g", work / "runtime.o",
        "target/aarch64-nintendo-horizon/release/libvera20k_switch.a",
        "-L", sdk / "libnx/lib", "-L", sdk / "portlibs/switch/lib",
        *libraries, "-lstdc++", "-lm", "-o", elf)
    from elftools.elf.elffile import ELFFile
    with elf.open("rb") as source:
        image = ELFFile(source)
        tls = next(segment for segment in image.iter_segments() if segment["p_type"] == "PT_TLS")
        alignment = tls["p_align"]
        if not alignment or tls["p_vaddr"] % alignment:
            raise SystemExit("Final ELF TLS is misaligned")
        print(f"Final ELF TLS verified: address={tls['p_vaddr']:#x} alignment={alignment}")
    dist = ROOT / "dist"
    dist.mkdir(exist_ok=True)
    nacp = work / f"{artifact}.nacp"
    title = "VERA20k GPU Probe" if args.gpu else "VERA20k Core Probe"
    run(sdk / "tools/bin/nacptool", "--create", title, "Homebrew", "0.1.0", nacp,
        f"--titleid={TITLE_ID}")
    run(sdk / "tools/bin/elf2nro", elf, dist / f"{artifact}.nro", f"--nacp={nacp}",
        f"--icon={sdk}/libnx/default_icon.jpg")
    if args.nsp:
        package(ROOT, sdk, elf, nacp)


if __name__ == "__main__":
    main()
