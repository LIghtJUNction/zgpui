"""Prepare a registry manifest for a specific Git tree and verify the resulting Cargo archive."""

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile
import tomllib
from urllib.request import Request, urlopen

PROJECT_Z_REV = "f1e2243bb0d4f721b617b77d6f19924cd81c8303"
CRATES_INDEX = "registry+https://github.com/rust-lang/crates.io-index"


def git(repo, *args):
    return subprocess.check_output(["git", "-C", repo, *args], text=True).strip()


def checksum(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def unversioned_git(table):
    if isinstance(table, dict):
        return ("git" in table and "version" not in table) or any(
            unversioned_git(v) for v in table.values()
        )
    return False


def fetch(url):
    request = Request(
        url,
        headers={
            "User-Agent": "zgpui-release (+https://github.com/LIghtJUNction/zgpui)"
        },
    )
    with urlopen(request, timeout=30) as response:
        return response.read()


def registry_pin(conversion):
    name, version = conversion["package"], conversion["version"]
    lower = name.lower()
    prefix = (
        str(len(lower))
        if len(lower) <= 2
        else ("3/" + lower[0] if len(lower) == 3 else lower[:2] + "/" + lower[2:4])
    )
    versions = (
        json.loads(line)
        for line in fetch(f"https://index.crates.io/{prefix}/{lower}").splitlines()
    )
    entry = next((v for v in versions if v["vers"] == version), None)
    if (
        not entry
        or entry["yanked"]
        or not re.fullmatch(r"[a-f0-9]{64}", entry["cksum"])
    ):
        raise ValueError(f"{name} {version} is unavailable in the crates.io index")
    archive = fetch(f"https://static.crates.io/crates/{name}/{name}-{version}.crate")
    if hashlib.sha256(archive).hexdigest() != entry["cksum"]:
        raise ValueError(f"{name} {version} does not match its crates.io checksum")
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as crate:
        vcs = json.load(crate.extractfile(f"{name}-{version}/.cargo_vcs_info.json"))[
            "git"
        ]
    if vcs["sha1"] != conversion["git_sha"] or vcs.get("dirty"):
        raise ValueError(
            f"{name} {version} does not match the candidate's locked Git commit"
        )
    return entry["cksum"]


def projected_lock(original, conversions, resolve=registry_pin):
    # Retain every selected dependency version. Only the source and checksum of
    # each converted internal package change when it comes from the registry.
    pins = {(c["package"], c["version"], c["git_sha"]): c for c in conversions}
    chunks = original.split("[[package]]")
    matched = set()
    for i in range(1, len(chunks)):
        package = tomllib.loads(chunks[i])
        source = package.get("source", "")
        key = (package["name"], package["version"], source.rpartition("#")[2])
        if not source.startswith("git+") or key not in pins:
            continue
        conversion = pins[key]
        if (
            source[4:].partition("#")[0].removesuffix(".git").lower()
            != conversion["github"].lower()
        ):
            raise ValueError(f"Unexpected Git origin for {package['name']}")
        digest = resolve(conversion)
        line = f'source = "{source}"'
        if chunks[i].splitlines().count(line) != 1 or key in matched:
            raise ValueError("Ambiguous source in candidate lockfile")
        chunks[i] = chunks[i].replace(
            line, f'source = "{CRATES_INDEX}"\nchecksum = "{digest}"', 1
        )
        matched.add(key)
    if matched != pins.keys():
        raise ValueError("Project Z projection and candidate lockfile disagree")
    return "[[package]]".join(chunks)


def stage(repo, candidate, project_z, destination):
    repo = repo.resolve()
    if git(repo, "rev-parse", "HEAD") != candidate:
        raise ValueError("Source checkout differs from selected candidate")
    manifest = tomllib.loads(git(repo, "show", f"{candidate}:Cargo.toml"))
    if not unversioned_git(manifest):
        return repo  # Preserve the original Cargo packaging for historical releases.
    # The driver and its repository allowlist come from the same immutable commit,
    # even when a developer's Project Z checkout has moved on or has local edits.
    with tempfile.TemporaryDirectory(prefix="zgpui-projection-tool-") as work:
        pinned = Path(work)
        (pinned / "scripts").mkdir()
        for path in [
            ".gitmodules",
            "scripts/release_projection.py",
            "scripts/cargo_versions.py",
        ]:
            (pinned / path).write_bytes(
                subprocess.check_output(
                    ["git", "-C", project_z, "show", f"{PROJECT_Z_REV}:{path}"]
                )
            )
        subprocess.run(
            [
                sys.executable,
                pinned / "scripts/release_projection.py",
                "stage",
                "--repo",
                repo,
                "--commit",
                candidate,
                "--output",
                destination,
            ],
            check=True,
        )
    report = json.loads((destination / "projection.json").read_text(encoding="utf-8"))
    original = subprocess.check_output(
        ["git", "-C", repo, "show", f"{candidate}:Cargo.lock"]
    )
    if (
        report["source_commit"] != candidate
        or report["remaining_git_dependencies"]
        or hashlib.sha256(original).hexdigest() != report["source_lock_sha256"]
    ):
        raise ValueError("Projected source does not match the selected candidate")
    root = destination / "source"
    (root / "Cargo.lock").write_bytes(
        projected_lock(original.decode(), report["conversions"]).encode()
    )
    return root


def build_package(source, features=None, env=None):
    manifest = source / "Cargo.toml"
    pkg = tomllib.loads(manifest.read_text(encoding="utf-8"))["package"]
    command = [
        "cargo",
        "package",
        "--locked",
        "--manifest-path",
        str(manifest),
        "-p",
        pkg["name"],
    ]
    if features:
        command += ["--features", features]
    subprocess.run(command, cwd=source, env=env, check=True)
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
            cwd=source,
            env=env,
            text=True,
        )
    )
    return (
        Path(metadata["target_directory"])
        / "package"
        / f"{pkg['name']}-{pkg['version']}.crate"
    )


def verify_archive(archive, expected, repo, destination, features, env=None):
    if checksum(archive) != expected:
        raise ValueError("Archive checksum differs from the quality job")
    pkg = tomllib.loads((repo / "Cargo.toml").read_text(encoding="utf-8"))["package"]
    root = f"{pkg['name']}-{pkg['version']}"
    destination.mkdir()
    with tarfile.open(archive, "r:gz") as crate:
        for member in crate.getmembers():
            if not member.name.startswith(root + "/") or not (
                member.isfile() or member.isdir()
            ):
                raise ValueError("Unexpected file in packaged crate")
        crate.extractall(destination, filter="data")
    unpacked = destination / root
    manifest = tomllib.loads((unpacked / "Cargo.toml").read_text(encoding="utf-8"))
    if (
        manifest["package"]["name"] != pkg["name"]
        or manifest["package"]["version"] != pkg["version"]
    ):
        raise ValueError("Packaged crate identity differs from the candidate")
    # Keep the backend-specific Cargo packaging gate on every native platform.
    # Its temporary output cannot overwrite the archive supplied by the quality job.
    environment = os.environ if env is None else env
    with tempfile.TemporaryDirectory(prefix="zgpui-repack-", dir=destination) as target:
        build_package(unpacked, features, {**environment, "CARGO_TARGET_DIR": target})
    subprocess.run(
        ["cargo", "build", "--locked", "--examples", "--features", features],
        cwd=unpacked,
        env=env,
        check=True,
    )
    subprocess.run(
        ["cargo", "test", "--locked", "--lib", "--features", features],
        cwd=unpacked,
        env=env,
        check=True,
    )


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest="command", required=True)
    prepare = sub.add_parser("stage")
    prepare.add_argument("--project-z", type=Path, required=True)
    prepare.add_argument("--destination", type=Path, required=True)
    prepare.add_argument("--candidate", required=True)
    verify = sub.add_parser("verify")
    verify.add_argument("--directory", type=Path, required=True)
    verify.add_argument("--sha256", required=True)
    verify.add_argument("--destination", type=Path, required=True)
    verify.add_argument("--features", required=True)
    args = ap.parse_args()
    repo = Path.cwd()
    if args.command == "stage":
        root = stage(
            repo, args.candidate, args.project_z.resolve(), args.destination.resolve()
        )
        archive = build_package(root)
        results = {
            "source": str(root),
            "crate": str(archive),
            "sha256": checksum(archive),
        }
        print(json.dumps(results))
        if "GITHUB_OUTPUT" in os.environ:
            with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
                for name, value in results.items():
                    print(f"{name}={value}", file=output)
    else:
        # Use the candidate's normal MSVC discovery when running on Windows.
        sys.path.insert(0, str(repo / "scripts"))
        from check_native import environment

        archives = list(args.directory.glob("*.crate"))
        if len(archives) != 1:
            raise ValueError("Expected exactly one validated source crate")
        verify_archive(
            archives[0],
            args.sha256,
            repo,
            args.destination,
            args.features,
            environment(),
        )


if __name__ == "__main__":
    main()
