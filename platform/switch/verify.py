#!/usr/bin/env python3
"""Verify Switch core/GPU artifacts and their saved runtime reports."""
from pathlib import Path
import argparse
import re
from elftools.elf.elffile import ELFFile

ROOT = Path(__file__).resolve().parent


def require(condition, message):
    if not condition:
        raise SystemExit(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--probe", choices=["core", "gpu"], default="core")
    parser.add_argument("--format", choices=["nro", "nsp"], default="nro")
    parser.add_argument("--report", type=Path, help="Explicit runtime report")
    args = parser.parse_args()
    if args.probe == "gpu" and args.format != "nro":
        parser.error("The GPU probe currently produces NRO only")
    artifact = (ROOT / f"dist/vera20k-{args.probe}.{args.format}").read_bytes()
    if args.format == "nro":
        require(artifact[0x10:0x14] == b"NRO0", "Invalid NRO header")
    else:
        require(artifact[:4] == b"PFS0", "Invalid NSP header")
    with (ROOT / f"target/switch-build/vera20k-{args.probe}.elf").open("rb") as source:
        image = ELFFile(source)
        require(image["e_machine"] == "EM_AARCH64", "Expected ARM64 ELF")
        symbols = image.get_section_by_name(".symtab")
        require(symbols is not None, "Missing ELF symbol table")
        graphics_symbols = [
            "platform_graphics_init",
            "platform_graphics_present",
            "eglInitialize",
            "glDrawArrays",
            "nwindowGetDefault",
        ] if args.probe == "core" else [
            "platform_window_init",
            "vkGetInstanceProcAddr",
            "nwindowGetDefault",
        ]
        for symbol in ["rust_main", *graphics_symbols]:
            require(bool(symbols.get_symbol_by_name(symbol)), f"Missing linked graphics symbol: {symbol}")
        tls = next(segment for segment in image.iter_segments() if segment["p_type"] == "PT_TLS")
        require(tls["p_vaddr"] % tls["p_align"] == 0, "TLS misaligned")
        loads = [
            segment for segment in image.iter_segments()
            if segment["p_type"] == "PT_LOAD" and segment["p_filesz"] != 0
        ]
        graphics_markers = {
            b"eglInitialize",
            b"glDrawArrays",
            b"nwindowGetDefault",
        } if args.probe == "core" else {b"platform_window_init", b"nwindowGetDefault"}
        with (ROOT / "runtime.c").open("rb") as source:
            runtime = source.read()
        for marker in graphics_markers:
            require(marker in runtime, f"Missing native graphics path: {marker!r}")
        require(all(
            segment["p_align"] >= 0x1000
            and segment["p_offset"] % segment["p_align"]
                == segment["p_vaddr"] % segment["p_align"]
            for segment in loads
        ), "ELF load segments have incompatible page alignment")
    profile = "master-" if args.probe == "gpu" else ""
    report_path = args.report or ROOT / (
        f"target/emulator/{profile}xdg-data/eden/sdmc/switch/vera20k/"
        f"{args.probe}-probe-{args.format}.txt"
    )
    report = report_path.read_text()
    require(f"binary_format={args.format}" in report, "Report format mismatch")
    fields = ["mix_file_load", "ini_parse", "timestamp_format", "rayon", "simulation_construct",
              "core_probe_passed", "graphics_context", "normal_exit"] if args.probe == "core" else [
                  "production_gpu_context", "wgsl_pipeline", "batch_renderer", "textured_sprite",
                  "gpu_pixel_readback", "camera_transform_readback", "terrain_tmp_atlas_readback",
                  "terrain_tmp_zdepth_occlusion", "production_theater_load",
                  "theater_tile_loader", "gpu_probe_passed", "normal_exit"]
    for field in fields:
        require(re.search(r"(?m)^" + field + r"=true\b", report), f"Missing/failed runtime check: {field}")
    presses = re.search(r"(?m)^a_button_presses=(\d+)$", report)
    require(presses and int(presses.group(1)) > 0, "Input not tested")
    if args.probe == "gpu":
        frames = re.search(r"(?m)^frames_presented=(\d+)$", report)
        require(frames and int(frames.group(1)) > 0, "No GPU frames presented")
        sprites = re.search(r"(?m)^sprites_per_frame=(\d+)$", report)
        require(sprites and int(sprites.group(1)) > 1, "Sprite instancing not tested")
        terrain_tiles = re.search(r"(?m)^terrain_tile_count=(\d+)$", report)
        require(terrain_tiles and int(terrain_tiles.group(1)) >= 3, "TMP atlas not tested")
        terrain_instances = re.search(r"(?m)^terrain_instance_count=(\d+)$", report)
        require(terrain_instances and int(terrain_instances.group(1)) >= 9,
                "Terrain instance construction not tested")
        require(re.search(r"(?m)^theater_loaded=TEMPERATE$", report),
                "Production theater identity missing")
        require(re.search(r"(?m)^theater_ini_source=temperat\.mix$", report),
                "Theater INI was not loaded through the activated MIX")
        theater_images = re.search(r"(?m)^theater_tile_images_loaded=(\d+)$", report)
        require(theater_images and int(theater_images.group(1)) >= 5,
                "Production load_tile_images did not decode the requested TMP set")
        require(terrain_tiles and int(terrain_tiles.group(1)) == int(theater_images.group(1)),
                "Atlas and theater loader disagree on decoded TMP count")
        elapsed = re.search(r"(?m)^render_elapsed_ms=(\d+)$", report)
        require(elapsed and int(elapsed.group(1)) > 0, "Missing rendering duration")
        if "exit_reason=timeout" in report:
            require(int(elapsed.group(1)) >= 30000, "Timed rendering stopped before 30 seconds")
        print(f"GPU NRO: {len(artifact)} bytes; ARM64 + TLS + production GpuContext + "
              f"BatchRenderer + {sprites.group(1)} textured instances + "
              f"pixel readback + camera scroll/zoom + {terrain_tiles.group(1)} parsed TMP tiles + "
              f"{terrain_instances.group(1)} terrain instances + per-pixel Z occlusion + "
              f"{frames.group(1)} frames + input + normal exit passed")
        print("Scope: synthetic RGBA sprites and a 3x3 terrain grid whose TMP/palette/INI "
              "resources load through production theater and MIX APIs. Retail map streaming, "
              "SHP/voxel paths, full client, gameplay and audio remain unverified.")
        print(f"Report source: {report_path}")
        return
    require(re.search(r"(?m)^retail_assets_present=(true|false)\b", report),
            "Missing retail asset discovery result")
    require(re.search(r"(?m)^retail_rules_lookup=true\b", report)
            or "retail_assets_present=false" in report,
            "Retail archives found but rulesmd.ini lookup failed")
    host = (ROOT / "target/host-probe.log").read_text()
    require("test result: ok." in host, "Host probe did not pass")
    for stage in ["simulation_construct=true", "simulation_frames=16"]:
        pattern = re.escape(stage) + r" state_hash=([0-9a-f]{16})"
        actual, expected = re.search(pattern, report), re.search(pattern, host)
        require(actual and expected and actual.group(1) == expected.group(1), f"Host/ARM64 mismatch: {stage}")
        print(f"Host/ARM64 match: {stage} {actual.group(1)}")
    print(f"Core {args.format.upper()}: {len(artifact)} bytes; ARM64 + TLS + runtime report + input + clean exit passed")
    print("Scope: synthetic empty scene. Retail gameplay, graphics and audio remain unverified.")
    if not args.report:
        print("Report source: Eden. Physical Switch runtime remains unverified.")


if __name__ == "__main__":
    main()
