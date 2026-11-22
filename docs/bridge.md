# Application bridge

The `bridge` feature lets a zgpui host consume zbridge state through an explicit
transport. It is separate from the bounded scene and layout core: default builds
remain `no_std + alloc`, while `bridge` enables `std` and the optional zbridge
dependency. The model and protocol leave windows and listeners to the host.

## Enable the adapter

Add the feature to the host's zgpui dependency:

```toml
zgpui = { git = "https://github.com/LIghtJUNction/zgpui.git", features = ["bridge"] }
```

`zgpui::bridge::protocol` re-exports the actual zbridge types and codecs. A host
can therefore construct a `protocol::Request` and inspect its response without
maintaining a second wire schema. `BridgeModel` stores the latest capability set,
one state response, one event page and one command record; it does not accumulate
conversation history or write files.

## Connect and exchange

The host owns the authorized connection and passes one transport callback for
each request. Start with `Operation::Capabilities {}`, then use the returned
generation and advertised resource or command IDs for subsequent requests.
Request IDs correlate replies; they do not grant access or promise idempotency.

```rust,ignore
use zgpui::bridge::{BridgeModel, protocol};

let mut model = BridgeModel::default();
let request = protocol::Request {
    version: protocol::VERSION,
    id: protocol::Id::new("discover-1")?,
    operation: protocol::Operation::Capabilities {},
};

// The application supplies an already connected transport with this signature.
let reply = model.request_with(request, |request| connection.request(request)).await?;
```

`request_with` accepts a typed callback returning
`Future<Output = Result<protocol::Response, protocol::BridgeError>>`. This matches
ssmcp's `session::attach::RemoteFrontend::request`; an ssmcp host acquires and
closes that connection through its existing private attach interface. zgpui
depends on zbridge alone and keeps connection credentials outside the protocol.

`request_json_with` is the optional byte transport entry point. It encodes the
request and decodes the response with the shared zbridge codecs, then performs
the same request, generation, kind and resource checks as the typed path. The
transport must cap incoming bytes before allocating a complete response frame.
Both entry points invoke the callback once and create no executor or retry loop.

## Keep results tied to their request

Command records retain their original request ID, generation and command ID.
The model preserves a successful native receipt without interpreting it as task
completion. A transport error, invalid reply or cancellation after the callback
starts marks the command as `MayHaveOccurred`; reconnecting or discovering new
capabilities never resends it. The caller must resolve an uncertain result using
the native command's contract before deciding on another operation.

Read-only capabilities exclude commands and the model rejects those attempts
before transport. These local checks help present available actions; the host
still checks current authority on every operation. Responses cannot supply a
caller identity or use a new generation to rebind an old command record.

Capability refreshes clear cached state and event pages, including refreshes
within one generation. A retained-event gap clears cached state so the host can
explicitly request a fresh snapshot. The model replaces event pages rather than
appending an unbounded log. Event cursors remain `u64` values in Rust and decimal
strings on the wire, including values above JavaScript's exact integer range.

## Validate the integration

The adapter has source-level tests for shared `Host` dispatch, observer rejection,
single-send native receipts, interrupted or mismatched commands, cancelled
futures, generation isolation, event gaps, large cursors, and advertised byte and
schema limits. The same-generation capability refresh case also checks that old
state is discarded. Run both default and feature-enabled checks:

```sh
cargo test --locked --lib
cargo test --locked --doc
cargo test --locked --features bridge --lib
cargo test --locked --features bridge --doc
```

This change supplies the consumer API and tests; it has not yet been executed
against a native platform. The typed model and fixtures do not validate a desktop
window, Android or iOS package, WebAssembly transport, signing, or app lifecycle.
Each application must report those platform checks separately from protocol
tests. The existing scene and layout platform contract remains independent of
this optional standard-library integration.
