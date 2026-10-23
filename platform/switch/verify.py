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
                  "theater_tile_loader", "retail_theater_load",
                  "retail_master_archives_loaded", "retail_terrain_readback",
                  "file_map_load", "retail_rules_art_load", "retail_object_preview",
                  "retail_shp_atlas", "retail_shp_readback", "retail_vxl_readback",
                  "gpu_probe_passed", "normal_exit"]
    for field in fields:
        require(re.search(r"(?m)^" + field + r"=true\b", report), f"Missing/failed runtime check: {field}")
    if args.probe == "core":
        presses = re.search(r"(?m)^a_button_presses=(\d+)$", report)
        require(presses and int(presses.group(1)) > 0, "Input not tested")
    if args.probe == "gpu":
        touch_presses = re.search(r"(?m)^live_touch_presses=(\d+)$", report)
        require(touch_presses and int(touch_presses.group(1)) > 0,
                "Touch input not tested")
        require(re.search(r"(?m)^live_scenario_load=true\b", report),
                "Production live scenario did not load")
        require(re.search(r"(?m)^live_navigation=true\b", report),
                "Live scenario did not publish navigation")
        require(re.search(r"(?m)^live_simulation_tick_ms=22$", report),
                "Live simulation did not use the client 22 ms tick")
        live_frames = re.search(r"(?m)^live_simulation_frames=(\d+)$", report)
        require(live_frames and int(live_frames.group(1)) > 0,
                "Live simulation did not advance")
        live_moves = re.search(r"(?m)^live_move_commands=(\d+)$", report)
        require(live_moves and int(live_moves.group(1)) > 0,
                "Live scenario did not accept a move order")
        moved = re.search(r"(?m)^live_moved_entities=(\d+)$", report)
        require(moved and int(moved.group(1)) > 0,
                "Live move order did not change any entity position")
        require(re.search(r"(?m)^live_voxel_parts_readback=true$", report),
                "Live turret/barrel GPU readback did not pass")
        require(re.search(r"(?m)^sidebar_loaded=true$", report),
                "Retail sidebar module did not load")
        require(re.search(r"(?m)^live_sidebar_production_started=true$", report),
                "Sidebar did not start production")
        require(re.search(r"(?m)^sidebar_gclock_seen=true$", report),
                "Sidebar production progress clock was never rendered")
        for field in ("sidebar_chrome_instances", "sidebar_cameo_instances",
                      "sidebar_text_instances"):
            match = re.search(r"(?m)^" + field + r"=(\d+)$", report)
            require(match and int(match.group(1)) > 0,
                    f"Retail sidebar emitted no {field}: {field}")
        for field, expected in (
            ("live_vxl_instances", 6),
            ("live_turret_instances", 1),
            ("live_barrel_instances", 1),
        ):
            match = re.search(r"(?m)^" + field + r"=(\d+)$", report)
            require(match and int(match.group(1)) == expected,
                    f"Live voxel parts mismatch: {field} (expected {expected})")
        frames = re.search(r"(?m)^frames_presented=(\d+)$", report)
        require(frames and int(frames.group(1)) > 0, "No GPU frames presented")
        sprites = re.search(r"(?m)^sprites_per_frame=(\d+)$", report)
        require(sprites and int(sprites.group(1)) > 1, "Sprite instancing not tested")
        terrain_tiles = re.search(r"(?m)^terrain_tile_count=(\d+)$", report)
        require(terrain_tiles and int(terrain_tiles.group(1)) >= 3, "TMP atlas not tested")
        terrain_instances = re.search(r"(?m)^terrain_instance_count=(\d+)$", report)
        require(terrain_instances and int(terrain_instances.group(1)) == 15,
                "Expected all 15 production Size-diamond cells in the main render pass")
        production_map = re.search(r"(?m)^production_map_parse=true\b", report)
        require(production_map, "Production map parser not tested")
        resolved_terrain = re.search(r"(?m)^production_resolved_terrain=true\b", report)
        require(resolved_terrain, "Production resolved terrain not tested")
        map_cells = re.search(r"(?m)^map_cell_count=(\d+)$", report)
        map_lookups = re.search(r"(?m)^map_lookup_count=(\d+)$", report)
        require(map_cells and int(map_cells.group(1)) == 9,
                "IsoMapPack5 fixture did not produce nine records")
        require(map_lookups and int(map_lookups.group(1)) == 9,
                "IsoMapPack5 fixture did not retain nine lookups")
        resolved_cells = re.search(r"(?m)^resolved_cell_count=(\d+)$", report)
        render_instances = re.search(r"(?m)^terrain_render_instance_count=(\d+)$", report)
        require(resolved_cells and int(resolved_cells.group(1)) == 15,
                "Production Size diamond did not materialize 15 cells")
        require(render_instances and int(render_instances.group(1)) == 15,
                "Production terrain grid did not contain 15 cells")
        payload = re.search(r"(?m)^file_map_payload_bytes=(\d+)$", report)
        require(payload and int(payload.group(1)) >= 70_000,
                "File-backed map payload was not read")
        for field, expected, label in (
            ("file_map_cells", 4560, "IsoMapPack5 cells"),
            ("file_map_lookups", 4560, "IsoMapPack5 lookups"),
            ("file_resolved_cell_count", 4560, "resolved terrain cells"),
            ("file_terrain_render_instance_count", 4560, "terrain grid cells"),
            ("file_fallback_tile_cells", 0, "retail theater fallback tiles"),
        ):
            match = re.search(r"(?m)^" + field + r"=(\d+)$", report)
            require(match and int(match.group(1)) == expected,
                    f"File-backed map {label} mismatch")
        file_visible = re.search(r"(?m)^file_terrain_visible_instance_count=(\d+)$", report)
        require(file_visible and int(file_visible.group(1)) > 0,
                "File-backed map produced no visible terrain instances")
        require(re.search(r"(?m)^file_map_source=loose$", report),
                "File-backed map was not loaded from a loose file")
        require(re.search(r"(?m)^file_map_name=ai_dred_bombard\.map$", report),
                "Unexpected file-backed map identity")
        require(re.search(r"(?m)^theater_loaded=TEMPERATE$", report),
                "Production theater identity missing")
        require(re.search(r"(?m)^theater_ini_source=temperat\.mix$", report),
                "Theater INI was not loaded through the activated MIX")
        theater_images = re.search(r"(?m)^theater_tile_images_loaded=(\d+)$", report)
        require(theater_images and int(theater_images.group(1)) >= 5,
                "Production load_tile_images did not decode the requested TMP set")
        require(terrain_tiles and int(terrain_tiles.group(1)) == int(theater_images.group(1)),
                "Atlas and theater loader disagree on decoded TMP count")
        retail_slots = re.search(r"(?m)^retail_theater_tile_slots=(\d+)$", report)
        require(retail_slots and int(retail_slots.group(1)) > 322,
                "Retail TEMPERATE theater does not contain tile 322")
        require(re.search(r"(?m)^retail_tile_322=water09\.tem$", report),
                "Retail TEMPERATE tile 322 is not water09.tem")
        retail_images = re.search(r"(?m)^retail_tile_images_loaded=(\d+)$", report)
        require(retail_images and int(retail_images.group(1)) >= 2,
                "Retail atlas did not decode land and water09 TMP tiles")
        for field, expected in (
            ("retail_objects_constructed", 9),
            ("retail_voxel_entities", 3),
            ("retail_shp_building_instances", 5),
        ):
            match = re.search(r"(?m)^" + field + r"=(\d+)$", report)
            require(match and int(match.group(1)) == expected,
                    f"Retail object preview mismatch: {field} (expected {expected})")
        shp_entries = re.search(r"(?m)^retail_shp_atlas_entries=(\d+)$", report)
        shp_pages = re.search(r"(?m)^retail_shp_atlas_pages=(\d+)$", report)
        require(shp_entries and int(shp_entries.group(1)) >= 5,
                "Retail SHP atlas is missing building bodies")
        require(shp_pages and int(shp_pages.group(1)) > 0,
                "Retail SHP atlas has no GPU pages")
        for field, expected in (
            ("retail_shp_instances", 6),
            ("retail_vxl_instances", 3),
            ("retail_vxl_atlas_entries", 3840),
        ):
            match = re.search(r"(?m)^" + field + r"=(\d+)$", report)
            require(match and int(match.group(1)) == expected,
                    f"Retail sprite preview mismatch: {field} (expected {expected})")
        elapsed = re.search(r"(?m)^render_elapsed_ms=(\d+)$", report)
        require(elapsed and int(elapsed.group(1)) > 0, "Missing rendering duration")
        if "exit_reason=timeout" in report:
            require(int(elapsed.group(1)) >= 30000, "Timed rendering stopped before 30 seconds")
        print(f"GPU {args.format.upper()}: {len(artifact)} bytes; ARM64 + TLS + production GpuContext + "
              f"BatchRenderer + {sprites.group(1)} textured instances + "
              f"pixel readback + camera scroll/zoom + {terrain_tiles.group(1)} parsed TMP tiles + "
              f"{terrain_instances.group(1)} fixture terrain instances + per-pixel Z occlusion + "
              f"{file_visible.group(1)} file-backed terrain instances + "
              "5 retail SHP buildings + 1 SHP unit + 3 VXL ships with individual "
              "GPU pixel readbacks + a live HTNK hull/turret/barrel + "
              f"{frames.group(1)} frames + input + normal exit passed")
        print("Scope: synthetic RGBA sprites plus nine IsoMapPack5 records over a 15-cell "
              "production Size diamond, and a 4560-cell loose authored map loaded through "
              "the production map source, parser and terrain path. The same map also loads "
              f"through the retail ra2md.mix/ra2.mix stack; {retail_slots.group(1)} theater "
              "slots resolve tile 322 to water09.tem, and land/water pixels are distinct. "
              "The synthetic fixture retains its pixel/Z regression. Retail rules/art and "
              "nine diagnostic object constructions also run; five healthy building bodies "
              "and one SQD SHP body use the production SHP atlas, while three visible "
              "DRED/HYD bodies use the production VXL atlas, palette, and voxel draw path. "
              "Every object is compared individually against a terrain-only GPU image. "
              "A production live scenario also loads with navigation, advances at the client's "
              "22 ms cadence, accepts selection and a move order, and moves an entity. "
              "The remaining client gaps are the shell/HUD, full tactical pick masks and "
              "actions, building animations/bibs/BUILDNGZ, audio and persistence.")
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
