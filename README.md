# zgpui

A platform-independent UI core for native applications, terminals and embedded hosts.

zgpui turns a bounded scene tree into pixel layout and renderer calls. The host
owns its window, graphics API, fonts, images and event loop. The library uses
`no_std` + `alloc`, has **zero Cargo dependencies**, and forbids unsafe code.

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
cargo add zgpui --path /path/to/zgpui
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
Desktop window creation, GPU drivers and text shaping belong to the host.

ztty uses zgpui for its scene state and layout, with its own X11/GPU rendering
and PTY input routing. Other hosts can use software rendering, native controls
or a GPU backend through the same contract.
See [platform integration](docs/platforms.md).

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
