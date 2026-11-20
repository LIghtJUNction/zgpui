# zgpui-raster

This companion draws text and simple shapes into caller-owned premultiplied
ARGB pixels. A Wayland presenter can use it for a small candidate surface
without creating an X11 window or an OpenGL context. Surface roles, positioning,
buffer release and application state remain with the caller.

**Consume this package from a pinned zgpui repository checkout.** It is marked
`publish = false`: its Linux font implementation is shared with
[`../native/font_linux.h`](../native/font_linux.h), outside this package
directory. Independent crates.io packaging needs a separate module-ownership
change. The root `zgpui` crate keeps its existing version and API, and remains
the workspace's default member.

## Features and system requirements

| Configuration | Available API | System libraries |
| --- | --- | --- |
| Default `font` on Linux | `Font`, `Mask`, `Canvas`, `Color` | FreeType and Fontconfig |
| `default-features = false` | Allocation-free `no_std` `Canvas` and `Color` | None |
| `std` without `font` | Pixel canvas plus standard error integration | None |

The Linux font feature requires a C compiler, `ar`, `pkg-config`, and the
FreeType/Fontconfig development headers. Install scalable Latin and CJK fonts
for Chinese and kana. The crate has no Cargo dependencies and does not link
X11, Wayland or OpenGL. The font API is available on Linux; the pixel canvas
is portable Rust.

## Drawing a caller-owned frame

```toml
[dependencies]
zgpui-raster = { path = "vendor/zgpui/raster" }
```

```rust
use zgpui_raster::{Canvas, Color, Font};

let mut font = Font::new("")?; // Empty family selects the native "sans" default.
let width = font.measure("你好世界", 16, false)?.ceil() as usize + 24;
let height = 48;
let mut pixels = vec![0; width * height];
let mut canvas = Canvas::new(&mut pixels, width, height, width)?;
canvas.clear(Color::rgb(250, 250, 250));
let text = font.rasterize("你好世界", 16, false)?;
canvas.mask(&text, 12, 10, Color::rgb(32, 33, 36));
# Ok::<(), zgpui_raster::Error>(())
```

`Canvas` borrows a `&mut [u32]`; its stride is measured in pixels.
The numeric pixel format is premultiplied `0xAARRGGBB`. `Color::rgba`
accepts ordinary channels and performs the premultiplication. Initialize a
new frame with `clear` before source-over drawing. Convert each `u32`
to the presenter's required byte order when uploading; do not reinterpret
this documentation as a portable byte-layout guarantee.

`rect` and `mask` clip to the surface. `clip` replaces the clipping
rectangle; `reset_clip` restores the whole surface. `clear` ignores the
clip and preserves row padding. `blend_mask` accepts a borrowed 8-bit
coverage mask and validates its dimensions before changing any pixel.

The host owns buffer reuse. For a Wayland SHM surface, retain a submitted
buffer until the compositor releases it. Dropping the canvas only ends the
Rust borrow; it does not complete a display transaction.

## Bounds and font ownership

| Input | Limit |
| --- | --- |
| Canvas or borrowed mask width, height and stride | 16,384 |
| Backing area including row padding | 16,777,216 pixels |
| Font input | 4,096 UTF-8 bytes; no interior NUL |
| Font size | 1–96 physical pixels |
| Rasterized text mask | 8,192 × 256 pixels |
| Family name | 255 UTF-8 bytes; no control characters |

Oversized text can be measured within the input limit, even when it is too
wide to rasterize. The caller should shorten or page labels before requesting
a mask. Text measurement loads glyph metrics without generating a bitmap.
Rasterization returns an owned mask that remains valid after its font drops.

Each `Font` owns an independent FreeType context and bounded face cache.
It is neither `Send` nor `Sync`; construct and use it on the rendering
thread. It shares the implementation with the native backend, while keeping
its live font state separate. Fontconfig fallback follows the host's existing
configuration. Rendering handles scalar glyphs; complex-script shaping and
bidirectional layout are outside this API.

## Local verification

Run from the repository root:

```sh
cargo test -p zgpui-raster --offline
cargo test -p zgpui-raster --no-default-features --offline
cargo clippy -p zgpui-raster --all-targets --offline -- -D warnings
env -u DISPLAY -u WAYLAND_DISPLAY cargo run -p zgpui-raster --example headless --offline -- /tmp/zgpui-headless.ppm
python3 scripts/test_fonts.py -v
cargo build -p zgpui --features native --examples --offline
cargo build -p zgpui --features linux-software --examples --offline
```

The companion tests check clipped drawing, row padding, source-over alpha,
transactional validation, font bounds, independent font ownership, and real
Latin/CJK/kana coverage. The existing Linux font suite checks native matching
and resource ownership under AddressSanitizer and UndefinedBehaviorSanitizer.
The headless example exports a PPM frame without opening a display; it is a
drawing smoke test, not a Wayland popup integration test.
