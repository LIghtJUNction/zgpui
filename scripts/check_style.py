#!/usr/bin/env python3
"""Reject blank/solid text masks and square corners in real probe frames."""
import argparse
import json
from pathlib import Path


def inspect_frame(path):
    header = path.read_bytes().split(b"\n", 3)
    if len(header) != 4 or header[0] != b"P6" or header[2] != b"255":
        raise ValueError("expected a bounded style_probe P6 frame")
    width, height = map(int, header[1].split())
    if (width, height) != (520, 260) or len(header[3]) != width * height * 3:
        raise ValueError("unexpected style_probe frame size")
    data = header[3]

    def pixel(x, y):
        offset = (y * width + x) * 3
        return tuple(data[offset:offset + 3])

    background = pixel(5, 5)

    def difference(color):
        return max(abs(a - b) for a, b in zip(color, background))

    foreground = [
        (x, y) for y in range(155, 185) for x in range(20, 500)
        if difference(pixel(x, y)) > 30
    ]
    issues = []
    area = 0
    if len(foreground) < 20:
        issues.append("text mask has no readable foreground")
    else:
        xs, ys = zip(*foreground)
        area = (max(xs) - min(xs) + 1) * (max(ys) - min(ys) + 1)
        if len(foreground) >= area * 0.8:
            issues.append("text mask is a solid rectangle instead of glyphs")
    if difference(pixel(21, 21)) > 3:
        issues.append("rounded corner does not expose the background")
    if difference(pixel(35, 35)) <= 20:
        issues.append("primary button interior is missing")
    return {
        "frame": path.name,
        "passed": not issues,
        "foreground_pixels": len(foreground),
        "glyph_bounds_pixels": area,
        "background_pixels_in_glyph_bounds": area - len(foreground),
        "issues": issues,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("frames", nargs="+", type=Path)
    args = parser.parse_args()
    results = [inspect_frame(path) for path in args.frames]
    passed = all(result["passed"] for result in results)
    print(json.dumps({"passed": passed, "frames": results}))
    raise SystemExit(0 if passed else 1)


if __name__ == "__main__":
    main()
