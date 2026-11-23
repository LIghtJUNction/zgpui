#!/usr/bin/env python3
"""Keep the portable core dependency-free while explicitly validating the std bridge."""
import json
import subprocess


FEATURES = {"default": [], "std": [], "native": ["std"],
            "linux-software": ["native"], "bridge": ["std", "dep:zbridge"]}


def validate_metadata(data):
    packages = {package["name"]: package for package in data["packages"]}
    if set(packages) != {"zgpui", "zgpui-raster"}:
        raise ValueError("Expected the core and source-only raster companion")
    core, raster = packages["zgpui"], packages["zgpui-raster"]
    if core["features"] != FEATURES:
        raise ValueError("Default/std/native features must remain independent of the optional std bridge")
    runtime = [dependency for dependency in core["dependencies"] if dependency["kind"] != "dev"]
    if len(runtime) != 1:
        raise ValueError("Only the explicit optional zbridge runtime dependency is permitted")
    bridge = runtime[0]
    if (bridge["name"] != "zbridge" or bridge.get("rename") is not None
            or bridge["kind"] is not None or bridge.get("target") is not None
            or not bridge["optional"] or bridge["req"] != "*"
            or bridge["source"] != "git+https://github.com/LIghtJUNction/zbridge.git"):
        raise ValueError("zbridge must be optional, Git-only, selector-free and enabled only by bridge")
    for dependency in core["dependencies"]:
        if dependency["kind"] == "dev" and (dependency["name"] != "serde_json"
                or dependency["source"] != "registry+https://github.com/rust-lang/crates.io-index"):
            raise ValueError("Unexpected development dependency; review the core contract explicitly")
    if raster["dependencies"] or raster["publish"] != []:
        raise ValueError("The raster companion must remain dependency-free and source-only")
    if data["workspace_default_members"] != [core["id"]]:
        raise ValueError("Default workspace commands must select only the core")


def main():
    data = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], text=True))
    validate_metadata(data)
    tree = subprocess.check_output(
        ["cargo", "tree", "--locked", "--package", "zgpui", "--no-default-features",
         "--edges", "normal", "--prefix", "none", "--format", "{p}"], text=True).splitlines()
    if len(tree) != 1 or not tree[0].startswith("zgpui v"):
        raise ValueError("The actual default core resolves a runtime Cargo dependency")
    print("Zero default-core runtime dependencies; explicit optional std bridge; source-only raster")


if __name__ == "__main__":
    main()
