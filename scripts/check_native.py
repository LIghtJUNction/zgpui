#!/usr/bin/env python3
"""Validate one native backend for local use and immutable release candidates."""
import argparse
import datetime as dt
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def environment():
    env = dict(os.environ)
    env.setdefault("CARGO_BUILD_JOBS", "1")
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    if os.name != "nt":
        return env
    # Capture the installed MSVC environment privately; never print its values.
    program_files = env.get("ProgramFiles(x86)", env.get("PROGRAMFILES(X86)", ""))
    vswhere = Path(program_files) / "Microsoft Visual Studio/Installer/vswhere.exe"
    installation = subprocess.check_output(
        [str(vswhere), "-latest", "-products", "*", "-requires",
         "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
         "-property", "installationPath"], text=True,
    ).strip()
    if not installation:
        raise RuntimeError("MSVC x64 build tools are required")
    batch = Path(installation) / "VC/Auxiliary/Build/vcvars64.bat"
    with tempfile.TemporaryDirectory(prefix="zgpui-msvc-") as directory:
        script = Path(directory) / "environment.cmd"
        script.write_text(
            '@echo off\nchcp 65001 >nul\ncall "' + str(batch) +
            '" >nul\nif errorlevel 1 exit /b %errorlevel%\nset\n',
            encoding="utf-8",
        )
        output = subprocess.check_output(
            ["cmd", "/d", "/c", script.name], cwd=directory,
            text=True, encoding="utf-8",
        )
    env = {key.upper(): value for key, value in env.items()}
    for line in output.splitlines():
        if "=" in line and not line.startswith("="):
            key, value = line.split("=", 1)
            env[key.upper()] = value
    return env


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--features", choices=("native", "linux-software"), default="native")
    parser.add_argument("--window", action="store_true", help="also open bounded native probe windows")
    parser.add_argument("--package", action="store_true", help="verify the source crate with this backend")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    env = environment()
    common = ["--locked", *(["--offline"] if args.offline else []), "--features", args.features]
    output = ROOT / "artifacts/native-checks"
    output.mkdir(parents=True, exist_ok=True)
    steps = []

    def run(*command, extra_env=None, capture=False):
        print("+ " + subprocess.list2cmdline(command), flush=True)
        result = subprocess.run(
            command, cwd=ROOT, env={**env, **(extra_env or {})},
            check=True, timeout=1800, text=True, encoding="utf-8",
            errors="replace", capture_output=capture,
        )
        steps.append(list(command))
        return result

    run("cargo", "clippy", *common, "--all-targets", "--", "-D", "warnings")
    run("cargo", "test", *common, "--lib")
    run("cargo", "test", *common, "--doc")
    run("cargo", "build", *common, "--release", "--examples")
    software = sys.platform == "linux" and args.features == "linux-software"
    if sys.platform == "linux":
        run(sys.executable, "scripts/test_fonts.py", "-v")
        run(sys.executable, "scripts/test_effects_backend.py")
    if software:
        run(sys.executable, "scripts/test_software.py", "-v")
        run(sys.executable, "scripts/test_software_rect.py")
    if args.package:
        run("cargo", "package", *common)
    target = Path(env.get("CARGO_TARGET_DIR", "target"))
    if not target.is_absolute():
        target = ROOT / target
    binary = target / "release/examples" / ("window_probe.exe" if os.name == "nt" else "window_probe")
    dependencies = []
    if software:
        dynamic = run("readelf", "-d", str(binary), capture=True, extra_env={"LC_ALL": "C"}).stdout
        dependencies = re.findall(r"\(NEEDED\).*\[([^\]]+)\]", dynamic)
        forbidden = [name for name in dependencies if name.startswith(("libGL", "libLLVM", "libgallium"))]
        if forbidden:
            raise RuntimeError("CPU renderer links graphics driver libraries: " + ", ".join(forbidden))
    window_scope = "not requested"
    if args.window:
        if sys.platform == "linux":
            run(
                "xvfb-run", "-a", "-s", "-screen 0 1280x900x24 -nolisten tcp",
                sys.executable, "scripts/check_x11.py", "--binary", str(binary),
                "--output", str(output / (args.features + "-windows.json")),
                *(["--expect-software"] if software else []),
                extra_env={"LIBGL_ALWAYS_SOFTWARE": "1"},
            )
            run(
                "xvfb-run", "-a", "-s", "-screen 0 1280x900x24 -nolisten tcp",
                sys.executable, "scripts/test_fonts.py", "--window",
                "--features", args.features,
                "--output", str(output / (args.features + "-fonts.json")),
                extra_env={"LIBGL_ALWAYS_SOFTWARE": "1"},
            )
            window_scope = (
                "private X11 geometry, title, minimum-size and mapped-library checks; "
                "real font presentation with a nonempty screenshot and clean exit"
            )
            if software:
                run(
                    "xvfb-run", "-a", "-s", "-screen 0 1600x1000x24 -nolisten tcp",
                    sys.executable, "scripts/test_software_x11.py",
                    "--output", str(output / "linux-software-pixels"),
                )
                window_scope += (
                    "; CPU baseline pixel equality, resize/Expose, repeated presentation "
                    "and allocation-failure rollback under ASan/UBSan"
                )
        else:
            run(str(binary), "zgpui-release-probe", "660", "264", capture=True)
            window_scope = "bounded native window, context and present smoke"
        style = binary.with_name("style_probe.exe" if os.name == "nt" else "style_probe")
        style_command = [str(style), str(output / (args.features + "-style"))]
        if sys.platform == "linux":
            style_command = ["xvfb-run", "-a", "-s", "-screen 0 1280x900x24 -nolisten tcp", *style_command]
        run(*style_command, extra_env={"LIBGL_ALWAYS_SOFTWARE": "1"})
        style_output = output / (args.features + "-style")
        run(sys.executable, "scripts/check_style.py",
            str(style_output / "default.ppm"), str(style_output / "custom.ppm"))
        if sys.platform == "linux" and not software:
            run(
                "xvfb-run", "-a", "-s", "-screen 0 1280x900x24 -nolisten tcp",
                sys.executable, "scripts/test_effects_backend.py", "--real",
                extra_env={"LIBGL_ALWAYS_SOFTWARE": "1"},
            )
            run(
                "xvfb-run", "-a", "-s", "-screen 0 1280x900x24 -nolisten tcp",
                sys.executable, "scripts/test_effects_backend.py", "--real",
                extra_env={"LIBGL_ALWAYS_SOFTWARE": "1", "MESA_GL_VERSION_OVERRIDE": "1.1"},
            )
            legacy_output = output / "native-legacy-style"
            run(
                "xvfb-run", "-a", "-s", "-screen 0 1280x900x24 -nolisten tcp",
                str(style), str(legacy_output), extra_env={
                    "LIBGL_ALWAYS_SOFTWARE": "1", "MESA_GL_VERSION_OVERRIDE": "1.1",
                    "MESA_EXTENSION_OVERRIDE": "-GL_ARB_texture_non_power_of_two",
                },
            )
            run(sys.executable, "scripts/check_style.py",
                str(legacy_output / "default.ppm"), str(legacy_output / "custom.ppm"))
            run(
                "xvfb-run", "-a", "-s", "-screen 0 1280x900x24 -nolisten tcp",
                sys.executable, "scripts/test_gpu_masks.py", "-v",
                extra_env={"LIBGL_ALWAYS_SOFTWARE": "1"},
            )
        window_scope += (
            "; real glyph foreground/background and rounded corners; palette/input pixels, "
            "long text, font switch/reset and sequential-window isolation"
        )
    report = {
        "utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "platform": platform.system(),
        "architecture": platform.machine(),
        "features": args.features,
        "steps": steps,
        "window_scope": window_scope,
        "direct_native_dependencies": dependencies,
        "hardware_gpu_benchmark": False,
    }
    (output / (args.features + ".json")).write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
