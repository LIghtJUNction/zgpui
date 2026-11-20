#!/usr/bin/env python3
"""Inspect native window identity and geometry on an explicitly private X display."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="target/release/examples/window_probe")
    parser.add_argument("--output", default="_validation/window-properties.json")
    parser.add_argument("--expect-software", action="store_true")
    args = parser.parse_args()
    if not os.environ.get("DISPLAY") or os.environ["DISPLAY"] == ":0":
        raise SystemExit("Run on a private X server, for example xvfb-run -a")
    binary = str(Path(args.binary).resolve())

    def xd(*arguments):
        return subprocess.check_output(["xdotool", *map(str, arguments)], text=True).strip()

    cases = [
        ("ztype", 660, 264, "ztype", "Ztype"),
        ("zcall-gui · window check", 1080, 740, "zcall-gui", "Zcall-gui"),
        ("Other.App - identity check", 512, 320, "other.app", "Other.app"),
        ("· native fallback", 320, 200, "zgpui", "Zgpui"),
    ]
    results = []
    for title, width, height, instance, cls in cases:
        proc = subprocess.Popen(
            [binary, title, str(width), str(height)],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        try:
            window = None
            for _ in range(100):
                if proc.poll() is not None:
                    raise AssertionError(proc.stderr.read())
                found = subprocess.run(
                    ["xdotool", "search", "--onlyvisible", "--class", "^" + re.escape(cls) + "$"],
                    capture_output=True, text=True,
                )
                if found.returncode == 0:
                    window = found.stdout.splitlines()[-1]
                    break
                time.sleep(.05)
            assert window, f"Native probe window did not appear: {title}; {found.stderr}"
            properties = subprocess.check_output(
                ["xprop", "-id", window, "WM_CLASS", "WM_NORMAL_HINTS", "_NET_WM_NAME"], text=True,
            )
            geometry = dict(line.split("=", 1) for line in xd(
                "getwindowgeometry", "--shell", window).splitlines() if "=" in line)
            assert xd("getwindowname", window) == title, properties
            assert f'"{instance}", "{cls}"' in properties, properties
            assert f"minimum size: {width} by {height}" in properties, properties
            assert (int(geometry["WIDTH"]), int(geometry["HEIGHT"])) == (width, height), geometry
            mapped_libraries = sorted({
                Path(line.split()[-1]).name
                for line in Path(f"/proc/{proc.pid}/maps").read_text().splitlines()
                if "/" in line and ".so" in line.split()[-1]
            })
            graphics = [name for name in mapped_libraries
                        if name.startswith(("libGL", "libLLVM", "libgallium"))]
            if args.expect_software:
                assert not graphics, f"CPU renderer loaded graphics driver libraries: {graphics}"
            result = {
                "title": title, "instance": instance, "class": cls,
                "requested": [width, height],
                "actual": [int(geometry["WIDTH"]), int(geometry["HEIGHT"])],
                "xprop": properties.strip(), "mapped_graphics_libraries": graphics,
            }
        finally:
            if proc.poll() is None:
                proc.terminate()
            try:
                stdout, stderr = proc.communicate(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                stdout, stderr = proc.communicate()
        result["renderer"] = stdout.strip()
        results.append(result)
    report = {
        "scope": "real X11 properties and geometry on a private X server",
        "expect_software": args.expect_software,
        "hardware_gpu_benchmark": False,
        "windows": results,
    }
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
