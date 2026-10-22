"""Package the core probe as a standalone installable NSP, using local device keys."""
import os
from pathlib import Path
import hashlib
import re
import shutil
import subprocess
import tempfile

TITLE_ID = "0100f52556455241"


def verify_package(path: Path, keys: Path, sdk: Path, title_id: str, log_directory: Path):
    """Verify the actual NSP's three NCAs, allowing only homebrew signature failures."""
    log_directory.mkdir(parents=True, exist_ok=True)
    hactool = sdk / "tools/bin/hactool"
    with tempfile.TemporaryDirectory(prefix="nsp-verify-", dir=log_directory) as temporary:
        extracted = Path(temporary)
        listing = subprocess.run([str(hactool), "-t", "pfs0", "-x", f"--outdir={extracted}", str(path)],
                                 stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, check=True)
        (log_directory / "pfs0.log").write_text(listing.stdout)
        files = list(extracted.iterdir())
        if len(files) != 3 or any(not file.name.endswith(".nca") for file in files):
            raise SystemExit("NSP does not contain exactly three NCAs")
        types = set()
        for file in files:
            if file.name.split(".")[0] != hashlib.sha256(file.read_bytes()).hexdigest()[:32]:
                raise SystemExit("NCA content ID/hash mismatch")
            result = subprocess.run([str(hactool), "-k", str(keys), "--suppresskeys", "--disablekeywarns",
                                     "-t", "nca", "-y", str(file)], stdout=subprocess.PIPE,
                                    stderr=subprocess.STDOUT, text=True, check=True)
            (log_directory / (file.name + ".log")).write_text(result.stdout)
            content = re.search(r"Content Type:\s+(\w+)", result.stdout)
            identifier = re.search(r"Title ID:\s+([0-9A-Fa-f]+)", result.stdout)
            if not content or not identifier or int(identifier.group(1), 16) != int(title_id, 16):
                raise SystemExit("Unable to decrypt NCA or wrong title ID")
            for line in result.stdout.splitlines():
                if "(FAIL)" in line and not re.match(r"\s*(Fixed-Key Signature|Signature)\s+\(FAIL\):", line):
                    raise SystemExit("NCA integrity check failed; see verification logs")
            if "Superblock Hash (GOOD)" not in result.stdout:
                raise SystemExit("NCA data hashes were not verified")
            if content.group(1) == "Program" and "NPDM Signature (GOOD)" not in result.stdout:
                raise SystemExit("Program NPDM signature did not verify")
            types.add(content.group(1))
        if types != {"Program", "Control", "Meta"}:
            raise SystemExit("Missing Program/Control/Meta NCA")
    print(f"NSP verified: Program/Control/Meta, encrypted headers, content hashes, NPDM, title ID {title_id}")


def verify_runtime_file(runtime: Path):
    """Reject an unexpectedly large runtime image before packaging."""
    size = runtime.stat().st_size
    if size > 32 * 1024 * 1024:
        raise SystemExit(f"Refusing oversized Switch NSO runtime: {size} bytes")
    header = runtime.read_bytes()[:4]
    if header != b"NSO0":
        raise SystemExit("Expected an NSO runtime for NSP packaging")


def package(root: Path, sdk: Path, elf: Path, nacp: Path):
    keys = Path(os.environ.get("SWITCH_KEYS", Path.home() / ".switch/prod.keys"))
    if not keys.is_file():
        raise SystemExit("NRO built. Set SWITCH_KEYS to a local device key file for NSP packaging.")
    work = root / "target/switch-build/nsp"
    exefs, control, output = work / "exefs", work / "control", work / "output"
    for directory in [exefs, control, output]:
        directory.mkdir(parents=True, exist_ok=True)
    subprocess.run([str(sdk / "tools/bin/elf2nso"), str(elf), str(exefs / "main")], check=True)
    verify_runtime_file(exefs / "main")
    subprocess.run([str(sdk / "tools/bin/npdmtool"), str(root / "npdm.json"),
                    str(exefs / "main.npdm")], check=True)
    shutil.copyfile(nacp, control / "control.nacp")
    shutil.copyfile(sdk / "libnx/default_icon.jpg", control / "icon_AmericanEnglish.dat")
    packer = Path(os.environ.get("HACBREWPACK", root.parents[3] / "toolchains/switch/hacbrewpack/hacbrewpack"))
    arguments = [str(packer), "-k", str(keys.resolve()), "--titleid", TITLE_ID,
                 "--exefsdir", str(exefs), "--controldir", str(control), "--noromfs", "--nologo",
                 "--nspdir", str(output), "--ncadir", str(work / "nca"),
                 "--tempdir", str(work / "temp"), "--backupdir", str(work / "backup"),
                 "--keygeneration", "1", "--keepncadir"]
    process = subprocess.run(arguments, cwd=root, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    for line in process.stdout.splitlines():
        print("Key area key: [redacted]" if line.startswith("Key area key") else line)
    process.check_returncode()
    target = root / "dist/vera20k-core.nsp"
    produced = output / f"{TITLE_ID}.nsp"
    verify_package(produced, keys, sdk, TITLE_ID, work / "verification")
    produced.replace(target)
    print(f"Built standalone NSP: {target} (title ID {TITLE_ID})")
