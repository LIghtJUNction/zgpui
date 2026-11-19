#!/usr/bin/env python3
"""Fetch the pinned upstream source and expose its unmodified layout implementation."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent
GPUI_SHA = "a246423a3b24a08e0d0c8574eab3a9de422747e0"
ZGPUI_SHA = "1d796d1fc63808dc099ed7b10c92e75835ae2814"


def run(*arguments, cwd=ROOT):
    return subprocess.check_output(arguments, cwd=cwd, text=True).strip()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, default=ROOT / ".cache/zed")
    args = parser.parse_args()
    source = args.source.resolve()
    if not source.exists():
        source.mkdir(parents=True)
        run("git", "init", "--quiet", cwd=source)
        run("git", "remote", "add", "origin", "https://github.com/zed-industries/zed.git", cwd=source)
        run("git", "fetch", "--depth", "1", "origin", GPUI_SHA, cwd=source)
        run("git", "checkout", "--detach", "FETCH_HEAD", cwd=source)
    if run("git", "rev-parse", "HEAD", cwd=source) != GPUI_SHA:
        raise SystemExit("GPUI source is not at the pinned revision; keep other checkouts intact")
    allowed = {"README.md", "crates/gpui/src/gpui.rs"}
    changed = set(run("git", "diff", "HEAD", "--name-only", cwd=source).splitlines())
    if changed - allowed:
        raise SystemExit("GPUI contains changes beyond the documented visibility shim")
    untracked = run("git", "ls-files", "--others", "--exclude-standard", cwd=source)
    if untracked:
        raise SystemExit("GPUI contains untracked source files; inspect them before benchmarking")
    zgpui_root = ROOT.parent.parent
    baseline_changes = run("git", "diff", ZGPUI_SHA, "--", "Cargo.toml", "src", cwd=zgpui_root)
    if baseline_changes:
        raise SystemExit("zgpui library differs from the fixed baseline; measure optimizations separately")
    baseline_untracked = run("git", "ls-files", "--others", "--exclude-standard", "--", "src", cwd=zgpui_root)
    if baseline_untracked:
        raise SystemExit("zgpui has untracked library source; inspect it before benchmarking")
    readme = source / "README.md"
    original_readme = run("git", "show", "HEAD:README.md", cwd=source) + "\n"
    notice = "> [!IMPORTANT]\n> Remove this line to confirm you've reviewed this PR before submitting.\n"
    if readme.read_text() not in (original_readme, notice + original_readme):
        raise SystemExit("unrecognized README changes in GPUI checkout")
    readme.write_text(notice + original_readme)
    library = source / "crates/gpui/src/gpui.rs"
    original = run("git", "show", "HEAD:crates/gpui/src/gpui.rs", cwd=source) + "\n"
    replacement = original.replace("pub use taffy::{AvailableSpace, LayoutId};",
                                   "pub use taffy::{AvailableSpace, LayoutId, TaffyLayoutEngine};")
    if original == replacement or library.read_text() not in (original, replacement):
        raise SystemExit("unexpected GPUI facade; visibility shim must be reviewed")
    library.write_text(replacement)
    patch = run("git", "diff", "--", "README.md", "crates/gpui/src/gpui.rs", cwd=source) + "\n"
    (ROOT / "gpui-layout-visibility.patch").write_text(patch)
    (ROOT / ".cache").mkdir(exist_ok=True)
    link = ROOT / ".cache/zed"
    if not link.exists():
        link.symlink_to(source, target_is_directory=True)
    if link.resolve() != source:
        raise SystemExit("Cargo source link does not name the inspected upstream checkout")
    record = {"gpui_upstream_sha": GPUI_SHA, "zgpui_baseline_sha": ZGPUI_SHA,
              "zgpui_library_tree": run("git", "rev-parse", ZGPUI_SHA + ":src", cwd=zgpui_root),
              "patch_sha256": hashlib.sha256(patch.encode()).hexdigest(),
              "patch_scope": "Public re-export of the production TaffyLayoutEngine; no algorithm changes. README notice required by upstream AGENTS.md.",
              "upstream_layout_sha256": hashlib.sha256((source / "crates/gpui/src/taffy.rs").read_bytes()).hexdigest()}
    (ROOT / "sources.json").write_text(json.dumps(record, indent=2) + "\n")
    build_project = ROOT / ".cache/build-project"
    build_project.mkdir(exist_ok=True)
    # Absolute dependency paths avoid Cargo treating a symlinked foreign workspace
    # as members of the comparison workspace and inheriting the wrong metadata.
    manifest = (ROOT / "Cargo.toml").read_text().replace('path = "../.."',
        'path = ' + json.dumps(str(ROOT.parent.parent)))
    manifest = manifest.replace('path = ".cache/zed/crates/gpui"',
        'path = ' + json.dumps(str(source / "crates/gpui")))
    manifest += '\n[[bin]]\nname = "zgpui-gpui-comparison"\npath = ' + json.dumps(str(ROOT / "src/main.rs")) + '\n'
    (build_project / "Cargo.toml").write_text(manifest)
    if (ROOT / "Cargo.lock").exists():
        (build_project / "Cargo.lock").write_bytes((ROOT / "Cargo.lock").read_bytes())
    print(json.dumps(record, indent=2))


if __name__ == "__main__":
    main()
