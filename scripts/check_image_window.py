#!/usr/bin/env python3
"""Capture image_probe's actual X11 window on a caller-provided private display."""
import argparse
import ctypes as c
import hashlib
import json
import os
from pathlib import Path
import selectors
import subprocess
import time

from PIL import Image, ImageChops


class XImage(c.Structure):
    _fields_ = [(name, c.c_int) for name in ("width", "height", "xoffset", "format")]
    _fields_ += [("data", c.c_void_p)]
    _fields_ += [(name, c.c_int) for name in (
        "byte_order", "bitmap_unit", "bitmap_bit_order", "bitmap_pad", "depth",
        "bytes_per_line", "bits_per_pixel")]
    _fields_ += [(name, c.c_ulong) for name in ("red_mask", "green_mask", "blue_mask")]


def capture(width, height):
    lib = c.CDLL("libX11.so.6")
    lib.XOpenDisplay.argtypes = [c.c_char_p]
    lib.XOpenDisplay.restype = c.c_void_p
    lib.XDefaultRootWindow.argtypes = [c.c_void_p]
    lib.XDefaultRootWindow.restype = c.c_ulong
    lib.XQueryTree.argtypes = [c.c_void_p, c.c_ulong, c.POINTER(c.c_ulong),
                              c.POINTER(c.c_ulong), c.POINTER(c.POINTER(c.c_ulong)),
                              c.POINTER(c.c_uint)]
    lib.XFetchName.argtypes = [c.c_void_p, c.c_ulong, c.POINTER(c.c_void_p)]
    lib.XFree.argtypes = [c.c_void_p]
    lib.XGetImage.argtypes = [c.c_void_p, c.c_ulong, c.c_int, c.c_int, c.c_uint, c.c_uint,
                             c.c_ulong, c.c_int]
    lib.XGetImage.restype = c.POINTER(XImage)
    lib.XDestroyImage.argtypes = [c.POINTER(XImage)]
    lib.XCloseDisplay.argtypes = [c.c_void_p]
    display = lib.XOpenDisplay(None)
    if not display:
        raise RuntimeError("cannot open caller-provided private X display")
    try:
        root = c.c_ulong()
        parent = c.c_ulong()
        children = c.POINTER(c.c_ulong)()
        count = c.c_uint()
        if not lib.XQueryTree(display, lib.XDefaultRootWindow(display), c.byref(root),
                              c.byref(parent), c.byref(children), c.byref(count)):
            raise RuntimeError("cannot query private X window tree")
        window = None
        try:
            for index in range(count.value):
                name = c.c_void_p()
                if lib.XFetchName(display, children[index], c.byref(name)) and name.value:
                    try:
                        if c.string_at(name.value) == b"zgpui-image-probe":
                            window = children[index]
                            break
                    finally:
                        lib.XFree(name)
        finally:
            if children:
                lib.XFree(children)
        if window is None:
            raise RuntimeError("image_probe client window was not found")
        pointer = lib.XGetImage(display, window, 0, 0, width, height, c.c_ulong(-1).value, 2)
        if not pointer:
            raise RuntimeError("XGetImage failed")
        try:
            data = pointer.contents
            if (data.width, data.height, data.bits_per_pixel, data.byte_order,
                data.red_mask, data.green_mask, data.blue_mask) != (
                    width, height, 32, 0, 0xff0000, 0xff00, 0xff):
                raise RuntimeError("capture fixture requires the standard 24-bit RGB Xvfb visual")
            raw = c.string_at(data.data, data.bytes_per_line * data.height)
            return Image.frombytes("RGB", (width, height), raw, "raw", "BGRX", data.bytes_per_line, 1)
        finally:
            lib.XDestroyImage(pointer)
    finally:
        lib.XCloseDisplay(display)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--rgba", type=Path)
    parser.add_argument("--width", type=int)
    parser.add_argument("--height", type=int)
    args = parser.parse_args()
    if not os.environ.get("DISPLAY"):
        parser.error("run on a private X11 display, for example through xvfb-run")
    if bool(args.rgba) != bool(args.width and args.height):
        parser.error("--rgba requires --width and --height together")
    args.output.mkdir(parents=True, exist_ok=True)
    command = [str(args.binary.resolve()), str(args.output.resolve())]
    if args.rgba:
        command.extend([str(args.width), str(args.height), str(args.rgba.resolve())])
    env = {**os.environ, "ZGPUI_IMAGE_PROBE_WAIT": "1"}
    with (args.output / "probe.stderr.log").open("w") as errors:
        process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=errors, text=True, env=env)
        try:
            with selectors.DefaultSelector() as selector:
                selector.register(process.stdout, selectors.EVENT_READ)
                if not selector.select(60):
                    raise RuntimeError("image_probe did not become ready within 60 seconds")
                line = process.stdout.readline()
            if not line:
                raise RuntimeError(f"image_probe failed before rendering; see {errors.name}")
            receipt = json.loads(line)
            if not receipt.get("ready"):
                raise RuntimeError("image_probe did not report a complete frame")
            expected = Image.open(args.output / "dark.ppm").convert("RGB")
            for attempt in range(10):
                actual = capture(receipt["width"], receipt["height"])
                delta = max(high for _, high in ImageChops.difference(expected, actual).getextrema())
                if delta == 0:
                    break
                time.sleep(0.05)
            actual.save(args.output / "window.png")
            receipt.update(command=command, screenshot="window.png",
                           screenshot_sha256=hashlib.sha256((args.output / "window.png").read_bytes()).hexdigest(),
                           screenshot_matches_rendered_rgb=delta == 0,
                           maximum_channel_delta=delta, capture_attempts=attempt + 1)
            if args.rgba:
                receipt.update(source=str(args.rgba.resolve()),
                               source_sha256=hashlib.sha256(args.rgba.read_bytes()).hexdigest())
            process.communicate(input="\n", timeout=10)
            receipt["exit_code"] = process.returncode
            (args.output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
            print(json.dumps(receipt))
            if delta != 0 or process.returncode:
                raise RuntimeError("actual X11 pixels or clean probe exit did not match")
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=5)


if __name__ == "__main__":
    main()
