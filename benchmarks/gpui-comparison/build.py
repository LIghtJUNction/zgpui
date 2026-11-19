#!/usr/bin/env python3
"""Build two otherwise identical executables, separating timing from allocation probes."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--timing-only", action="store_true")
    args = parser.parse_args()
    subprocess.run([sys.executable, str(ROOT / "prepare.py")], cwd=ROOT, check=True)
    environment = os.environ.copy()
    environment.update(CARGO_BUILD_JOBS="1", CARGO_TARGET_DIR=str(ROOT / ".cache/target"), RUSTFLAGS="",
                       RUSTC_WRAPPER="", RUSTC_WORKSPACE_WRAPPER="")
    manifest = ROOT / ".cache/build-project/Cargo.toml"
    binaries = ROOT / ".cache/bin"
    binaries.mkdir(exist_ok=True)
    for kind, feature in [("timing", []), *([] if args.timing_only else [("allocations", ["--features", "allocation-metrics"])])]:
        command = ["cargo", "+1.100.0", "build", "--release", "--manifest-path", str(manifest), *feature]
        if (ROOT / "Cargo.lock").exists():
            command.append("--locked")
        subprocess.run(command, cwd=ROOT, env=environment, check=True)
        suffix = ".exe" if os.name == "nt" else ""
        shutil.copyfile(ROOT / (".cache/target/release/zgpui-gpui-comparison" + suffix), binaries / (kind + suffix))
        shutil.copymode(ROOT / (".cache/target/release/zgpui-gpui-comparison" + suffix), binaries / (kind + suffix))
        shutil.copyfile(manifest.with_name("Cargo.lock"), ROOT / "Cargo.lock")


if __name__ == "__main__":
    main()
