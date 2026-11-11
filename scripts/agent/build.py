#!/usr/bin/env python3
"""Pinned external checkouts; no downloads of Nokia SDK, keys or firmware."""
import argparse
import hashlib
import json
import platform
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
LOCK = json.loads((ROOT / "docs/agent/upstream.lock.json").read_text())


def run(*args, cwd=None):
    return subprocess.check_output(args, cwd=cwd, text=True).strip()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("action", choices=["prepare", "check", "build"])
    p.add_argument("workspace", type=Path, help="external build directory (SDK + ADBian siblings)")
    args = p.parse_args()
    base = args.workspace.resolve()
    base.mkdir(parents=True, exist_ok=True)
    for name, repo, key in [("SDK", "epoc", "epoc"), ("ADBian", "ADBian", "adbian")]:
        dest = base / name
        if args.action == "prepare" and not dest.exists():
            run("git", "clone", "--no-checkout", f"https://github.com/pizzaria-foundation/{repo}.git", str(dest))
            run("git", "checkout", "--detach", LOCK[key], cwd=dest)
        if not dest.exists() or run("git", "rev-parse", "HEAD", cwd=dest) != LOCK[key]:
            raise RuntimeError(f"{name}: missing or wrong revision; use a fresh workspace and prepare")
        if run("git", "status", "--porcelain", "--untracked-files=no", cwd=dest):
            raise RuntimeError(f"{name}: tracked sources are modified; use a clean checkout")
    print("Pinned checkouts verified; this does not verify a SIS or a phone.")
    if args.action == "prepare":
        return
    sdk = base / "SDK"
    required = [sdk / "sdk/epoc32/include/e32base.h", sdk / "toolchain/cross/bin/arm-none-symbianelf-g++"]
    required += [sdk / f"toolchain/host/bin/{tool}" for tool in ["elf2e32", "makesis", "signsis", "rcomp", "uidcrc"]]
    missing = [str(path.relative_to(sdk)) for path in required if not path.exists()]
    if platform.system() != "Linux" or platform.machine() not in ["aarch64", "arm64"]:
        missing.append("Linux aarch64 build host (upstream supported host)")
    if missing:
        raise RuntimeError("Missing prerequisites: " + ", ".join(missing))
    version = run("rustup", "run", LOCK["rust_device_toolchain"], "rustc", "-Vv")
    if args.action == "check":
        print(version)
        return
    # Upstream hardcodes +nightly. A separate untracked launcher selects the pinned nightly;
    # its location keeps upstream's SDK-relative resolution intact. Never change tracked sources.
    source = (sdk / "tools/symbuild").read_text()
    if source.count("cargo +nightly build") != 1:
        raise RuntimeError("Upstream build entry changed; audit it before updating the lock")
    launcher = sdk / "tools/.yeskia-symbuild"
    launcher.write_text(source.replace("cargo +nightly build", f"cargo +{LOCK['rust_device_toolchain']} build"))
    manifest = {"schema": LOCK["schema"], "pins": LOCK, "rustc": version, "artifacts": [], "phone_verified": False}
    try:
        for app, suffix in [("rshelld", "sisx"), ("rshell", "sis"), ("rfprobe", "sis")]:
            appdir = base / "ADBian/apps" / app
            subprocess.run(["bash", str(launcher), str(appdir), "clean"], check=True)
            subprocess.run(["bash", str(launcher), str(appdir)], check=True)
            artifact = appdir / f"build/{app}.{suffix}"
            manifest["artifacts"].append({"file": str(artifact.relative_to(base)), "bytes": artifact.stat().st_size,
                                           "sha256": hashlib.sha256(artifact.read_bytes()).hexdigest()})
        (base / "yeskia-build.json").write_text(json.dumps(manifest, indent=2) + "\n")
        print("Built all three packages; hashes: yeskia-build.json. Installation remains unverified.")
    finally:
        launcher.unlink(missing_ok=True)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, subprocess.CalledProcessError) as exc:
        print(str(exc), file=sys.stderr)
        sys.exit(1)
