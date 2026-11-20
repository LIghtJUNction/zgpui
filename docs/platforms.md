# Platform integration

zgpui owns scene validation, scene revisions, layout and action identity.
A host chooses its operating-system integration and decides when a completed
frame becomes visible. The optional `native` module provides the shared window
and immediate-widget backend documented in [native integration](native.md).

## Responsibilities

| zgpui | Host |
| --- | --- |
| Typed `Scene` and bounded JSON updates | Window or surface creation |
| Graph, text and property validation | Allocator and event loop |
| Row/column layout in pixels | Fonts, shaping, theme and clipping |
| Scene/revision-aware hit testing | Pointer and keyboard events |
| Action intents | Permission checks and application commands |
| Image identifiers and extents | Image decoding, storage and rendering |

The core has no Cargo dependencies or platform FFI. It uses `alloc` for variable
storage. A final `no_std` executable supplies an allocator and its platform's
startup and panic handling.

## Layout

Accept a scene through `State::replace_scene`, then construct
`Layout::new(scene, bounds, metrics)`.

- `Rect` expresses pixel position and dimensions.
- `Metrics` sets line height, row/column gaps and control padding.
- Columns stack their children; rows share available width.
- The layout retains visible leaf rectangles and the scene identity.
- `natural_height` reports the content height before host clipping.

Keep each layout with the same State and host that created it. Matching checks
only surface and revision; host/session identity, ownership and generation
checks remain the caller's responsibility.

Rebuild layout when the scene or viewport changes. The host may apply its own
outer panel, scrolling or window chrome. Fonts, DPI conversion and image
resolution stay in the host.

## Renderer contract

```rust
pub trait Renderer {
    type Error;
    fn begin(&mut self, bounds: Rect) -> Result<(), Self::Error>;
    fn draw(&mut self, node: &Node, rect: Rect) -> Result<(), Self::Error>;
    fn end(&mut self) -> Result<(), Self::Error>;
    fn abort(&mut self);
}
```

`Layout::paint` checks the scene identity, calls `begin`, draws the visible
leaves in order and calls `end`. An error from any rendering phase calls
`abort` and returns `PaintError::Renderer`. A stale layout returns
`PaintError::Stale`.

Implement `end` to commit a prepared frame and `abort` to discard it.
The host is responsible for preventing partially drawn content from reaching
the display. The trait is suitable for software, GPU, native and test renderers.

## Input

Use `Layout::button_at(scene, x, y)` for an enabled button in the displayed
layout. Pass the returned node together with that layout's surface and revision
to `State::action`. Only scenes with `events: true` yield actions.

The host checks the complete input gesture and the application's current
authority before executing an action. The library returns an intent; it does
not write to a PTY, execute a command or open a URL.

## Platform scope

| Target | Core validation configured in CI | Host integration |
| --- | --- | --- |
| Linux | Native unit tests, doctests and the host example | ztty supplies X11/GPU rendering |
| macOS | Native unit tests, doctests and the host example | Supply an AppKit or other native host |
| Windows | Native unit tests, doctests and the host example | Supply a Win32 or other native host |
| `wasm32-unknown-unknown` | Library compilation | Supply a WebAssembly host and allocator |
| `x86_64-unknown-none` | Library compilation | Supply an embedded or bare-metal host |

This matrix describes the default core. Optional native builds additionally
validate Linux OpenGL and CPU backends, Windows WGL and macOS NSOpenGLContext.
Release jobs run the backend checks on their actual platform; configured jobs
are not a claim that a particular source revision passed. ztty can keep its own
window renderer and its native Wayland host independently of these features.

## Examples and compatibility

[examples/host.rs](../examples/host.rs) implements a small renderer and routes
an action through the typed API. The [ZUI/1 adapter](protocol.md) supports
terminal applications that already send bounded JSON scenes.
