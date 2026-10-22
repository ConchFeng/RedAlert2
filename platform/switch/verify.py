#!/usr/bin/env python3
"""Verify the core-port artifact and compare its emulator report with the host run."""
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
    parser.add_argument("--format", choices=["nro", "nsp"], default="nro")
    parser.add_argument("--report", type=Path, help="Explicit report downloaded from a physical Switch")
    args = parser.parse_args()
    artifact = (ROOT / f"dist/vera20k-core.{args.format}").read_bytes()
    if args.format == "nro":
        require(artifact[0x10:0x14] == b"NRO0", "Invalid NRO header")
    else:
        require(artifact[:4] == b"PFS0", "Invalid NSP header")
    with (ROOT / "target/switch-build/vera20k-core.elf").open("rb") as source:
        image = ELFFile(source)
        require(image["e_machine"] == "EM_AARCH64", "Expected ARM64 ELF")
        symbols = image.get_section_by_name(".symtab")
        require(symbols is not None, "Missing ELF symbol table")
        for symbol in [
            "platform_graphics_init",
            "platform_graphics_present",
            "rust_main",
        ]:
            require(bool(symbols.get_symbol_by_name(symbol)), f"Missing linked symbol: {symbol}")
        for symbol in [
            "eglInitialize",
            "glDrawArrays",
            "nwindowGetDefault",
        ]:
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
        }
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
    report_path = args.report or ROOT / f"target/emulator/xdg-data/eden/sdmc/switch/vera20k/core-probe-{args.format}.txt"
    report = report_path.read_text()
    require(f"binary_format={args.format}" in report, "Report format mismatch")
    for field in ["mix_file_load", "ini_parse", "timestamp_format", "rayon", "simulation_construct",
                  "core_probe_passed", "graphics_context", "normal_exit"]:
        require(re.search(r"(?m)^" + field + r"=true\b", report), f"Missing/failed runtime check: {field}")
    require(re.search(r"(?m)^retail_assets_present=(true|false)\b", report),
            "Missing retail asset discovery result")
    require(re.search(r"(?m)^retail_rules_lookup=true\b", report)
            or "retail_assets_present=false" in report,
            "Retail archives found but rulesmd.ini lookup failed")
    require(int(re.search(r"a_button_presses=(\d+)", report).group(1)) > 0, "Input not tested")
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
