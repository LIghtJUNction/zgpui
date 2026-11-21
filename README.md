# zgpui

A portable UI core with optional native GPU and CPU windows for Project Z.

zgpui turns bounded scenes into layout and renderer calls. Default builds use
`no_std` + `alloc`, have **zero Cargo dependencies**, and forbid unsafe code.
The optional `native` module supplies windows and immediate widgets through
platform libraries; `linux-software` selects a CPU renderer instead of OpenGL
on Linux. Applications own their state and event loop.

## Core

- Typed Rust scenes with atomic validation through `State::replace_scene`.
- Rows, columns, text, progress indicators, buttons and image references.
- Shared pixel layout, clipping bounds and revision-aware button hit testing.
- A `Renderer` contract with `begin`, `draw`, `end` and `abort`.
- Bounded JSON updates and a compatible ZUI/1 terminal transport adapter.

Native hosts can use the typed API without parsing JSON or using a terminal.

## Use

From a checkout:

```sh
cargo add zgpui --git https://github.com/LIghtJUNction/zgpui.git
cargo run --example host
```

```rust
use zgpui::{Kind, Node, Scene, State};

let mut state = State::new();
state.replace_scene(Scene {
    surface: 1,
    revision: 1,
    root: 1,
    events: true,
    nodes: vec![Node {
        id: 1,
        kind: Kind::Button {
            label: "Run".into(),
            action: "run".into(),
            enabled: true,
        },
    }],
}).expect("valid scene");
```

Build a `layout::Layout` from the accepted scene, a pixel rectangle and host
metrics. `Layout::paint` sends the visible nodes to your renderer.
The [host example](examples/host.rs) shows the complete scene, rendering and
button-action path.

## Host boundary

The same core supports Linux, macOS, Windows, WebAssembly and bare-metal Rust
targets. A host supplies an allocator and implements rendering and input.
Desktop hosts can opt into `zgpui::native` for window creation and immediate
widgets, or keep their own backend. Drivers and application policy stay outside
the scene core. Native text rendering has its own platform limitations.

ztty uses zgpui for its scene state and layout, with its own X11/GPU rendering
and PTY input routing. Other hosts can use software rendering, native controls
or a GPU backend through the same contract.
See [platform integration](docs/platforms.md).

## Native windows

Enable `native` for the platform OpenGL backend, or `linux-software` for Linux
CPU rendering. Import windows and widgets from `zgpui::native`. The portable
scene API and its default features remain unchanged.

[Native integration and migration](docs/native.md) covers features, system
libraries, examples and the move from the former native UI package.

Linux font fallback also supports fixed-size bitmap strikes, scaled to the
requested text size. Color bitmap fonts such as Noto Color Emoji render as
grayscale coverage through the shared native and headless font implementation.
Strike selection and scaling follow the [FreeType sizing contract](https://freetype.org/freetype2/docs/reference/ft2-sizing_and_scaling.html#ft_select_size);
the [FreeType glyph loading contract](https://freetype.org/freetype2/docs/reference/ft2-glyph_retrieval.html#ft_load_xxx)
provides the grayscale bitmap when color output is not requested.

### Materials and folding layers

Native hosts can opt into cached in-window materials: frosted glass, progressive
blur, mica style, liquid glass, paper and custom GLSL fragment functions.
Captured layers can include text and use a perspective fold. Source revisions,
quality, texture budgets and redraw timing remain under application control;
the API reports actual capabilities and flat fallbacks.

```sh
cargo run --locked --release --features native --example materials
```

The gallery provides strength, quality and openness controls, two sampling modes
for folding, live cache counters and a cancellable 1.2-second animation. It waits
for events when idle. These are application-window effects, not native Windows
DWM Mica or Apple compositor materials. [Materials and measurement](docs/materials.md)
explains the cache contract, shader extension point, GPU/fallback distinction
and reproducible cached-versus-invalidated experiment.

## Headless pixels

The source-only `zgpui-raster` companion in the repository's `raster/` directory draws bounded
premultiplied ARGB pixels into caller-owned buffers and reuses the Linux system
font implementation without linking X11 or OpenGL. It is consumed from a pinned
repository checkout and is not independently published. The root crate remains
the default workspace member; applications opt into the companion separately.

## Terminal adapter

The existing **ZUI/1** wire prefix and revision rules are preserved.
Terminal applications can probe capabilities, replace or update a scene, and
receive opted-in action events. Images are references resolved by the host.
The complete format, examples and limits are in [the protocol guide](docs/protocol.md).

## Validate and measure

```sh
cargo test --locked --lib
cargo test --locked --doc
cargo run --locked --example host
cargo bench --bench protocol
```

The [validation workflow](.github/workflows/ci.yml) runs the core tests on Linux,
macOS and Windows, and checks WebAssembly and bare-metal targets. It can run
manually or as part of a release.

Benchmarks emit timing and allocation records for named workloads. Record the
toolchain, machine and repeated samples when comparing runs; allocation totals
are distinct from peak live memory and process RSS.

[Manual version bump and publishing](docs/releasing.md) describes the
`CARGO_REGISTRY_TOKEN` release workflow.

MIT © LIghtJUNction
