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

## Update a consumer

Replace the old dependency key, package selection and vendor gitlink together:

```toml
[dependencies]
zgpui = { path = "vendor/zgpui", version = "=0.1.1", default-features = false, features = ["native"] }
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

The public native types are `Window`, `Canvas`, `Event`, `Rect`, `Theme`, `Ui`,
`ButtonStyle` and `colors`. `Window` stays on its creating thread and the backend
allows one live window per process. `native::Rect` belongs to immediate drawing;
`layout::Rect` belongs to the portable scene layout. A host implements a
`layout::Renderer` adapter when it wants to paint scene nodes in a native window.

Update `.gitmodules` to `https://github.com/LIghtJUNction/zgpui.git`, move the
source directory to `vendor/zgpui`, pin the tested commit and regenerate the
consumer lockfile. Rust calls use `zgpui::native`; do not retain a `zui` alias.
Outer source bundles and installer repositories must update their own pins.

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

## Checks

```sh
cargo test --locked --lib
cargo test --locked --lib --features native
cargo test --locked --lib --features linux-software
cargo clippy --locked --all-targets --features native -- -D warnings
cargo clippy --locked --all-targets --features linux-software -- -D warnings
python3 scripts/test_software.py
cargo build --release --locked --example window_probe --features linux-software
xvfb-run -a -s "-screen 0 1280x900x24 -nolisten tcp" python3 scripts/check_x11.py
```

Private X11 checks inspect the actual title, WM_CLASS, minimum size and client
geometry. Software rasterizer tests exercise clipping, alpha blending, text
cache lifecycle and resize failures under AddressSanitizer and UBSan. Native
window checks under Xvfb do not measure hardware GPU performance.

The frozen native import and original file hashes are recorded in
[native-import.json](native-import.json). Historical source names in that
receipt identify provenance; current dependency and import names are `zgpui`.
