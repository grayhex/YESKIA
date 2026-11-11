#!/usr/bin/env python3
"""Stage only YESKIA-owned helper sources beside the pinned SDK, without SDK redistribution."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parents[2]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument("workspace", type=Path)
args = p.parse_args()
base = args.workspace.resolve()
lock = json.loads((root / "docs/agent/upstream.lock.json").read_text())
if subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=base / "SDK", text=True).strip() != lock["epoc"]:
    raise SystemExit("Wrong SDK revision; prepare pinned sources first")
dest = base / "YESKIA-Agent"
protocol = base / "yeskia-agent-protocol"
if dest.exists() or protocol.exists():
    raise SystemExit("Helper/protocol destination exists; preserve artifacts and use a fresh workspace")
shutil.copytree(root / "agent/yeskia-agent", dest)
shutil.copytree(root / "crates/yeskia-agent-protocol", protocol)
# The protocol crate has explicit package versions and no inherited workspace values.
with (protocol / "Cargo.toml").open("a") as f:
    f.write("\n[workspace]\n")
(dest / "device/Cargo.toml").write_text('''[workspace]
[package]
name="yeskia-agent-device"
version="0.1.0"
edition="2021"
license="MIT"
[lib]
crate-type=["staticlib"]
[dependencies]
yeskia-agent-protocol={path="../../yeskia-agent-protocol"}
symbian={path="../../SDK/crates/symbian"}
symbian-app={path="../../SDK/crates/symbian-app"}
[profile.release]
opt-level="z"
lto=true
codegen-units=1
panic="abort"
''')
digest = hashlib.sha256()
files = []
for directory in [dest, protocol]:
    for path in sorted(directory.rglob("*")):
        if path.is_file():
            digest.update(str(path.relative_to(base)).encode())
            digest.update(path.read_bytes())
            files.append({"file": str(path.relative_to(base)), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
(base / "yeskia-helper-source.json").write_text(json.dumps({"source_sha256":digest.hexdigest(),"files":files},indent=2)+"\n")
print(f"Staged helper source hash: {digest.hexdigest()}")
print(f"Build on the prepared ARM64 host: python3 scripts/agent/build.py build-helper {base}")
print("Device build and hardware service remain unverified until their separate checks pass.")
