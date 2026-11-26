# Native windows and migration

zgpui provides the native window and widget implementation formerly shipped as
`zui-native`. The implementation now lives in this package under `zgpui::native`;
there is no dependency on an old package or a compatibility import alias.

## Choose a backend

| Cargo features | Platform code | Rendering |
| --- | --- | --- |
| Default features, or `default-features = false` | None | Host-provided scene renderer |
| `std` | None | The same scene core with the Rust standard library available |
| `native` | Linux X11, Windows Win32, macOS AppKit | OpenGL through the platform API |
| `linux-software` | Linux X11 | A CPU framebuffer presented through XPutImage |

`native` enables `std`; `linux-software` enables `native`. On other desktop
platforms `linux-software` keeps their existing native renderer. Features are
additive, so a Linux build with all features selects the CPU backend. Validate
`native` separately when the application ships the OpenGL backend.

The default scene core remains `no_std + alloc`, with zero Cargo dependencies.
Native builds use a C bridge and system libraries; unsafe FFI is confined to
the `native` module. Enabling a native window does not change the ZUI/1 scene
protocol or implement an application event loop.

## Resizing the window and its contents

A native window can open at desktop size and shrink to a compact layout. For example,
call `window.set_min_size(320, 240)?` after `Window::new` to give the window manager
a lower minimum client size. The initial size and minimum size are independent.

Measure the inner viewport on every frame, including the frame for `Event::Resize`.
Subtract fixed headers and margins from `canvas.width` and `canvas.height` first.
`PaneLayout::widths()` then supplies the widths at which each pane can measure
its text and controls. A `GridLayout` fits equal-width cells into either pane and
returns the height of all its rows.

```rust,ignore
let layout = PaneLayout::new(viewport, 260., 400., 16.);
let (_, main_width) = layout.widths();
let controls = GridLayout::new(Rect::new(0., 0., main_width, 0.), 140., 3, 8.);
let controls_height = controls.height(6, 36.);
let panes = layout.arrange(240., controls_height + 180., scroll_y, false);
scroll_y = panes.scroll_y;
let preview_height = panes.main.h - controls_height;
```

Wide windows give both panes the available height. Narrow windows stack the panes
and give extra height to the main pane. When their measured content exceeds the
viewport, `max_scroll` is the distance to the bottom. Pass wheel changes into the
next call to `arrange` and retain its clamped `scroll_y`. The returned rectangles
already include the scroll offset. Use those same coordinates for drawing and
widgets, and keep widget IDs stable across resizes. See the runnable
[window example](../examples/window.rs).

## Update a consumer

Replace the old dependency key, package selection and vendor gitlink together:

```toml
[dependencies]
zgpui = { path = "vendor/zgpui", version = "=0.1.5", default-features = false, features = ["native"] }
```

Use the `linux-software` feature for a Linux application that explicitly chooses
CPU rendering. An optional application GUI should name `dep:zgpui` in its own
feature declaration and mark this dependency `optional = true`.

```rust,no_run
# fn main() -> Result<(), String> {
# #[cfg(feature = "native")] {
use zgpui::native::{Window, colors};

let mut window = Window::new("example", 640, 400)?;
{
    let mut canvas = window.begin(colors::SURFACE);
    canvas.text("Hello", 20., 20., 18, false, colors::TEXT);
}
window.present();
# }
# Ok(())
# }
```

The public native types are `Window`, `Canvas`, `Event`, `Rect`, `Theme`, `Palette`, `Ui`,
`ButtonStyle` and `colors`. `Window` stays on its creating thread and the backend
allows one live window per process. `native::Rect` belongs to immediate drawing;
`layout::Rect` belongs to the portable scene layout. A host implements a
`layout::Renderer` adapter when it wants to paint scene nodes in a native window.

Update `.gitmodules` to `https://github.com/LIghtJUNction/zgpui.git`, move the
source directory to `vendor/zgpui`, pin the tested commit and regenerate the
consumer lockfile. Rust calls use `zgpui::native`; do not retain a `zui` alias.
Outer source bundles and installer repositories must update their own pins.

## RGBA8 images and preview frames

`Canvas::image` draws caller-owned decoded pixels through the shared native
renderer. A video or capture application supplies its latest frame and chooses
the destination rectangle; it does not need to turn every pixel into a widget.

```rust,no_run
# fn main() -> Result<(), String> {
# #[cfg(feature = "native")] {
use zgpui::native::{Rect, Window, colors};
let mut window = Window::new("image", 640, 400)?;
let rgba = [255, 0, 0, 255, 0, 255, 0, 128];
{
    let mut canvas = window.begin(colors::SURFACE);
    canvas.image(Rect::new(20., 20., 200., 100.), 2, 1, &rgba)?;
}
window.present();
# }
# Ok(())
# }
```

Input pixels are tightly packed, top-down RGBA8 with straight alpha. There is no
row padding, color-token interpretation, theme adjustment or retained reference
to the slice. Each call submits the current bytes, so the caller can reuse the
buffer when it returns. Premultiplied, BGRA and bottom-up sources require an
explicit conversion before this boundary.

Both backends stretch the source to the destination with nearest-neighbor
sampling at pixel centers. The current clip and window bounds apply without
changing the source mapping. Source alpha blends each sampled color over the
existing opaque window pixels. Filtering does not blur transparent RGB into
neighboring pixels. Hosts that need aspect preservation compute a fitted
rectangle themselves. This API does not manage capture, decoding, frame queues,
redraw scheduling, color profiles or smooth image filtering.

Dimensions must be nonzero and at most 16384 per side. The exact checked byte
length must be `width * height * 4`, at most 256 MiB. Destination coordinates,
width, height and right/bottom extents must be finite; dimensions must be positive
and produce nondegenerate extents. Invalid requests return `Err` before drawing.
A fully clipped valid image succeeds without drawing. A native allocation or
upload error can occur after earlier tiles were drawn; the caller should redraw
the complete next frame after an error. The software renderer also rejects image
drawing when no prepared framebuffer exists.

The OpenGL implementation keeps one reusable power-of-two RGBA upload texture,
limited by the context and to 1024 pixels per side: at most 4 MiB of RGBA texels.
Larger sources are streamed through that tile. Repeated frames and source size
changes do not accumulate texture objects. Window destruction deletes the texture;
failed allocation/upload releases it so the next call can retry. This accounting
does not bound driver memory or application frame storage. The CPU implementation
uses the existing window framebuffer and retains no image allocation.

Uploads use the OpenGL 1.1
[texture subimage](https://learn.microsoft.com/en-us/windows/win32/opengl/gltexsubimage2d)
and [pixel-store](https://learn.microsoft.com/en-us/windows/win32/opengl/glpixelstorei)
interfaces. Pixel-store, texture and blend state are restored after the call.
Tile draw bounds select whole destination pixels before rasterization, avoiding
subpixel rounding that could select a neighboring tile at a fractional edge.

The public API and its validation live in `src/native.rs`; the shared C boundary
validation is `native/image.h`. `native/draw.c` owns GL uploads and lifecycle;
`native/draw_software.c` uses the existing CPU blend operation. These are necessary
platform renderer implementations of one contract. The portable scene core,
`no_std` default, dependencies and `raster/` API remain unchanged. The initial
integration target is zobs's decoded `PreviewFrame`; the application owns that
integration and its capture/runtime acceptance.

### Image checks

```sh
python3 scripts/test_images.py -v
xvfb-run -a python3 scripts/test_images.py --gpu -v
cargo build --locked --features native --example image_probe
xvfb-run -a python3 scripts/check_image_window.py \
  target/debug/examples/image_probe /tmp/zgpui-image-native
```

Build `image_probe` with `linux-software` to check the CPU window. The capture
script requires Pillow and an X11 display owned by the test; it reads the actual
client window through XGetImage and compares every RGB byte to the renderer's
readback. Add `--rgba /path/frame.rgba --width 640 --height 360` to verify a real
caller frame. Captures and raw frames belong outside Git.

The C fixtures cover explicit RGBA/alpha colors, top-down orientation, cropped
source coordinates, fractional enlargement/reduction, exact lengths, invalid
dimensions, unchanged buffers after invalid requests, empty clips, and close /
reopen. GL checks additionally force a 64-pixel texture limit, use a real GL 1.1
context without NPOT support, inject generation/allocation/upload failures, restore
pixel-store state, and track one live texture over 300 source-size changes.
ASan, UBSan and LeakSanitizer are enabled by default. Restricted environments that
cannot run LSan may pass `--no-leak-scan` explicitly; that does not establish a leak
scan result, and address/undefined checks plus owned texture assertions still run.
The Rust probe checks the public API, 128 independently calculated image samples,
and identical image pixels across light/dark themes and a custom palette.

## Optional window colors and fonts

Version 0.1.3 adds a window-local palette and preferred font family. Existing
applications that do not call these setters retain their original theme and
font selection. The default `no_std` scene core and its dependency list are
unchanged.

```rust,no_run
# fn main() -> Result<(), String> {
# #[cfg(feature = "native")] {
use zgpui::native::{Palette, Theme, Window};

let mut window = Window::new("example", 640, 400)?;
window.set_theme(Theme::Dark);
let mut ink = Palette::for_theme(Theme::Dark);
ink.background = 0x161412ff;
ink.surface = 0x201c18ff;
ink.text = 0xe8ded2ff;
ink.accent = 0xd4a373ff;
ink.on_accent = 0x161412ff;
window.set_palette(ink);
window.set_font_family("JetBrains Mono")?;
# }
# Ok(())
# }
```

Palette fields use `0xRRGGBBAA`. The sixteen roles cover background, surface,
text, muted text, lines, accent, hovered accent, soft accent, hovered surface,
success and its soft surface, danger and its soft surface, disabled surface,
disabled text, and text on an accent. Canvas primitives and the existing button
and input widgets resolve canonical color tokens through the same palette.
The token's alpha multiplies the selected role's alpha. Other RGB values pass
through unchanged. Use `on_accent` for primary-button text; it is already
resolved and is not interpreted again as a surface token.

An explicit palette stays active when `set_theme` changes the built-in theme.
Call `clear_palette` to return to that theme. `Palette::for_theme` initializes
semantic roles from an existing theme; it groups legacy color aliases into
their corresponding role. Leaving the palette unset preserves the original
individual aliases exactly.

`set_font_family("monospace")` selects the platform's fixed-width family.
A missing named family falls back to that selection, and platform glyph fallback
remains available for characters outside the chosen font. Linux honors installed
Fontconfig aliases and language settings, Windows uses its installed family or
Consolas/Courier New fallback, and macOS uses the requested family or its system
monospaced font. The setter does not install fonts or change system preferences.

Names may contain at most 255 UTF-8 bytes and no control characters. Surrounding
whitespace is removed. An empty name resets the original default; repeating the
same normalized request does no native work. Native preparation finishes before
the old family is replaced. Failure returns an error and preserves the previous
font and text cache. Success invalidates only text masks or textures, leaving
geometry caches, framebuffer storage and the graphics context intact. Redraw
the complete frame after a font change.

Window access remains confined to its creating thread. The native bridge still
allows only one live window per process; these setters do not introduce concurrent
multiwindow support. Closing a window releases its font selection, so a later
window starts with the original defaults. Fontconfig matching uses private
temporary configurations and does not replace a host's process-wide configuration.

## Native build requirements

Linux needs a C compiler, pkg-config, X11, FreeType and fontconfig development
files. The `native` backend also needs OpenGL development files; the
`linux-software` backend neither creates a GLX context nor links OpenGL.
XWayland is required in Wayland sessions. Windows uses MSVC, Win32, WGL and GDI;
macOS uses the Xcode tools, AppKit, NSOpenGLContext and CoreText.

The CPU renderer owns one four-byte-per-pixel framebuffer and a bounded text
mask cache. X11 server storage, fonts and the process runtime consume additional
memory. This does not establish a whole-application memory limit. The renderer
requires a compatible RGB X11 visual and bounds its framebuffer allocation.

Linux releases temporary Fontconfig matching state after opening the required
FreeType faces. Matching retains system and user aliases and default-language
settings. Fonts added only to a host's in-memory Fontconfig configuration are
not inherited by the private matching configuration.

## OpenGL mask storage

The native GPU renderer uploads text and rounded-corner alpha masks with
power-of-two storage dimensions, including on legacy OpenGL 1.1 drivers.
This follows the platform's [texture size contract](https://learn.microsoft.com/en-us/windows/win32/opengl/glteximage2d).
Texture coordinates select only the populated mask. The renderer reads
`GL_MAX_TEXTURE_SIZE` and divides larger text masks into bounded tiles; adjacent
tiles include neighbour pixels so linear filtering at fractional positions does
not introduce a seam. Text advance and the original rasterization limits remain
unchanged.

The text cache allows at most 96 entries and 2 MiB of uploaded alpha texels,
counting padding and tile borders. This is an accounting bound on those uploads,
not a bound on total driver memory, fonts or the application. Rounded masks use
a separate fixed cache. A failed upload releases all tiles created for that
entry and does not cache an invalid texture. Closing the window releases both
texture caches and resets the remembered device limit.

## CPU frame lifetime

The Linux CPU backend keeps its framebuffer in a dedicated private anonymous
mapping. After ordinary XPutImage copies the frame and XFlush drains the Xlib
output, it releases the mapping's resident pages with MADV_DONTNEED. The virtual
address remains reserved. This path does not use XShm, a server-side renderer or
an additional process. See the [Xlib image-transfer contract](https://www.x.org/releases/X11R7.6/doc/libX11/specs/libX11/libX11.html)
and [Linux discard semantics](https://man7.org/linux/man-pages/man2/madvise.2.html).

A successful Window::begin clears every pixel before drawing. CPU present
consumes that frame; another present before a new begin is a no-op. Redraw the
complete frame after Event::Resize, which also represents an X11 Expose event.
The text-mask cache survives presentation. Drawing cannot write discarded frame
contents before the next successful begin.

Resize allocates a replacement mapping before releasing the old one. An
allocation failure preserves the previous storage and dimensions; a failed begin
does not expose a stale frame for presentation. Failure to discard pages retains
memory without changing rendering correctness. If XImage metadata allocation
fails, presentation retains the prepared frame for a later attempt.

Drawing still requires the full four-byte-per-pixel surface. At 1120x760 this is
3,404,800 bytes, rounded to complete pages by the mapping. Report drawing peaks
separately from idle residency; discarding this surface does not establish a
whole-application memory limit.

## Checks

```sh
cargo test --locked --lib
cargo test --locked --lib --features native
cargo test --locked --lib --features linux-software
cargo clippy --locked --all-targets --features native -- -D warnings
cargo clippy --locked --all-targets --features linux-software -- -D warnings
python3 scripts/test_software.py
python3 scripts/test_fonts.py -v
cargo build --release --locked --example window_probe --features linux-software
xvfb-run -a -s "-screen 0 1280x900x24 -nolisten tcp" python3 scripts/check_x11.py
```

Private X11 checks inspect the actual title, WM_CLASS, minimum size and client
geometry. Software rasterizer tests exercise clipping, alpha blending, text
cache lifecycle, discarded-page residency and allocation failures under AddressSanitizer and UBSan. Font
tests use installed Latin/CJK fonts to check locale matching, face reuse,
raster stability across sizes and configuration ownership under the same
sanitizers. Linux window checks also present multilingual regular and bold
text, reject an empty frame, preserve a PNG and require a clean exit.

The font checks require scalable Latin and CJK fonts, such as Liberation Sans
and Noto Sans CJK. The Linux window checks require Xvfb, xauth, xdotool,
x11-utils and ImageMagick in addition to the native development libraries.
These are validation requirements, separate from application runtime libraries.
Native window checks under Xvfb do not measure hardware GPU performance or
establish a whole-application memory limit.

The native release gate also runs `style_probe` on Linux, Windows and macOS.
It records the actual OpenGL version, renderer, extensions and maximum texture
size for GPU builds. Pixel checks require glyph foreground and background and
transparent rounded corners, so a solid text rectangle cannot pass as text.
It also reads actual pixels for primary-button text, disabled controls,
input focus and selection, alpha and custom colors. It changes font families,
checks fixed-width advances, rejects invalid names without changing the active
font, restores the original frame, and closes/reopens a window to check default
isolation. PPM frames accompany the result. The Linux sanitizer fixture also
forces font preparation to fail, verifies that cached text and framebuffer
storage survive, and repeatedly changes fonts while retaining host Fontconfig
ownership. These checks exercise rendering and resource lifetimes rather than
only comparing palette constants.

The Linux GPU gate additionally exercises a real GL 1.1 context with the NPOT
extension disabled. A separate fixture constrains texture limits to 256 and 64,
compares the same complete viewport at fractional text positions and scaling, verifies the
actual padded-upload budget, and injects an upload failure after the first tile
to check cleanup and recovery. These cases use a private software GL context;
they do not establish hardware GPU performance.

The frozen native import and original file hashes are recorded in
[native-import.json](native-import.json). Historical source names in that
receipt identify provenance; current dependency and import names are `zgpui`.

The formal native entry point is `scripts/check_native.py`. It accepts
`--features native` or `--features linux-software`; `--window --package`
adds native-window and packaged-source checks. Release jobs use those options.

[Local validation evidence](native-validation.json) records the frozen runtime
source, core and native checks, four real X11 cases per backend, and CPU driver
dependency inspection. It reports no hardware benchmark or whole-application
memory target.

The Linux CPU release gate also compares real window pixels with the pre-discard
implementation. Its source checkout must contain the frozen baseline commit. This test covers repeated presentation, resize/Expose redraw,
allocation-failure rollback and an antialiased text/geometry scene under
AddressSanitizer and UBSan:

    xvfb-run -a -s "-screen 0 1600x1000x24 -nolisten tcp" \
      python3 scripts/test_software_x11.py \
      --baseline cb50f2fe554622fdc984ae7fec9bcd844f175d67 \
      --output /var/tmp/zgpui-framebuffer-x11
