# zgpui / GPUI comparison

This harness compares the implementations on the same fixed-metric layout task.
It verifies every output rectangle against an independent integer geometry oracle
before recording any sample. It does **not** establish that the two libraries
provide equivalent desktop frameworks.

**Checkpoint status:** source `499b78324879cea97f8dfe9ea9f44111b938770d`
passed the cloud CPU release build and the unchanged independent geometry gate:
20 configurations and 40,960 deterministic pointer-query iterations. The
[source-bound receipt](resource-gates/20261120T032341Z/build-and-geometry.json)
records the executable, source, lockfile and resource-monitor hashes. The earlier
native build remains paused. The allocation executable has not yet been built;
no performance sample has been collected, and there are no measured winner,
frame-time or allocation claims.

The gate first exposed two fixture input mismatches. An auto-width GPUI root
produced a zero-width leaf; after declaring 100% root width, the test window's
default 2x scale produced 64 pixels instead of the independently expected 32.
Both failed binaries and source-bound receipts are retained in
`resource-gates/20261120T030008Z/build-and-geometry.json` and
`resource-gates/20261120T031258Z/build-and-geometry.json`. Declaring 100% root width
and initializing the test window at 1x scale corrected these inputs. The library
algorithms, dependency lock, optimization profile and independent oracle were
unchanged. Build wall time and the polling guard's stage wall time are not
operation latency measurements.

## Fixed sources and build

The baseline is zgpui `1d796d1fc63808dc099ed7b10c92e75835ae2814` (0.1.0), and
[Zed GPUI](https://github.com/zed-industries/zed/tree/a246423a3b24a08e0d0c8574eab3a9de422747e0/crates/gpui)
at `a246423a3b24a08e0d0c8574eab3a9de422747e0` (0.2.2).
Both are linked into the same executable with Rust 1.100.0, opt-level 3,
thin LTO and one code generation unit. Upstream's default GPUI features remain
enabled; `test-support` supplies the context required by its layout API.

`prepare.py` records one visibility-only change: public re-export of GPUI's
existing production `TaffyLayoutEngine`. It changes no layout algorithm,
optimization, style setting, renderer or dependency. The complete patch and
hash are recorded alongside the immutable upstream SHA. The README notice in
that patch follows the upstream repository's contributor instructions.
Preparation rejects additional tracked or untracked GPUI source changes and
checks zgpui's library source against the fixed baseline. Each run records the
exact harness file hashes in addition to the lockfile and executable hashes.

```sh
nice -n 19 python3 build.py
python3 run.py --cpu 2 --out results/baseline
python3 report.py results/baseline
```

The build script generates a machine-local manifest with resolved source paths
to preserve the foreign GPUI workspace's metadata inheritance. It uses the
committed dependency lockfile with `--locked` after the initial resolution.
Fetching dependencies and compilation are outside the measured
interval. A single build job and reduced scheduling priority avoid occupying
all cores on a shared development machine.

## Workloads and equivalence

Each scene contains a column of equally sized rows, with 1, 16, 64 or 225 leaf
boxes (3, 21, 73 or 241 total nodes). Every leaf is 24 pixels high. Gaps are
4 pixels. Widths are chosen so equal flex shares land on integer coordinates.
The viewport contains all leaves: clipping, scrolling and text measurement are
separate capabilities and are not silently omitted from a claimed full-frame task.

Both roots occupy the supplied viewport width. The zgpui adapter receives an
explicit root rectangle; the GPUI adapter uses 100% root width. Definite
available space alone does not force an auto-sized standalone flex root to fill
that width. GPUI's production
[window-root setup](https://github.com/zed-industries/zed/blob/a246423a3b24a08e0d0c8574eab3a9de422747e0/crates/gpui/src/window.rs)
handles this separately through `stretch_auto_size_to_fill`. Using a percentage
in this retained fixture also lets each resize resolve against the new viewport;
the independent expected rectangles and normal layout cache remain unchanged.

The CPU fixture uses scale factor 1, so its logical and device-pixel coordinates
coincide. GPUI's test window defaults to scale factor 2; its public
[test scale-factor API](https://github.com/zed-industries/zed/blob/a246423a3b24a08e0d0c8574eab3a9de422747e0/crates/gpui/src/window.rs)
sets the fixture window to 1 during setup, before any layout request or cached
geometry. This matches the scale already used for style conversion and bounds
readback. Verification and measurement setup use the same setting, outside the
timed iteration loop; the normal warmup precedes samples.

| Workload | Measured work | Boundary |
|---|---|---|
| `fresh` | Construct a new typed tree, calculate layout, read every leaf rectangle, then drop it | Includes construction and destruction on both sides |
| `retained` | Recompute layout on an existing tree and read every leaf rectangle | GPUI's normal layout cache stays enabled; zgpui's current `Layout::new` creates a new snapshot |
| `resize` | Alternate between two viewport widths, calculate layout and read every rectangle | Both must update the same coordinates |
| `geometry-scan` | Find the first matching enabled, non-overlapping box for deterministic points | zgpui `button_at`; GPUI `Bounds` with a documented linear host adapter. This is not GPUI event dispatch |

The GPUI adapter applies half-open right/bottom edges, matching zgpui. Parity
checks include 2,048 deterministic pointer positions per geometry. It cannot
stand in for GPUI's focus, propagation, accessibility, text input or native
event pipeline. No score aggregates these different scopes into an overall win.

## Measurement

The Linux runner pins each child to the requested permitted CPU, randomizes
backend order and case order using a recorded seed, and keeps seven independent
paired process rounds by default. It makes no governor, clock, desktop, affinity
or system-wide scheduling changes. Both children receive the same niceness.
Other processes are left running; load, core topology, frequency observations,
CPU consumption, maximum RSS and context switches are recorded per invocation.

Every operation has an individual `Instant` wall-time sample after 256 warmup
operations. p50/p95/p99 describe these operation samples, not an averaged batch.
All raw samples, including long tails, are retained. Allocations are measured
with a separate executable so the counting allocator does not change the timing
results. Allocation totals describe calls, requested byte traffic and peak
additional requested live bytes on the benchmark thread. They are distinct from
process peak RSS, allocator usable sizes and GPU memory.

Process CPU and RSS include fixture setup, warmup, sampling and JSON output.
The measurement batch also reads Linux's `/proc/thread-self/schedstat` CPU run
time and scheduler wait, following the
[kernel's field definitions](https://www.kernel.org/doc/html/latest/scheduler/sched-stats.html),
with no external FFI or system configuration change. The runner records whether
scheduler statistics are enabled; unavailable counters are not an operation's
wall-time replacement. CPU counters include the common sampling bookkeeping.
`fixture_setup_ns` includes the GPUI test context needed to exercise its engine;
it is recorded as harness setup and is **not native application startup**.

## Rendering coverage

zgpui defines a renderer callback contract; its host owns windows, fonts and GPU
work. GPUI includes those application services. Calling zgpui's renderer callback
and comparing it with GPUI's actual GPU rendering would measure unequal work.

At the pinned upstream revision,
[the platform factory](https://github.com/zed-industries/zed/blob/a246423a3b24a08e0d0c8574eab3a9de422747e0/crates/gpui_platform/src/gpui_platform.rs)
constructs a real `WgpuHeadlessRenderer` on Linux and a Metal renderer on macOS.
The older comment in `bench_context.rs` saying only macOS can render headlessly
does not describe this factory. This particular CPU fixture supplies no renderer;
its results therefore report CPU layout only. A native-frame comparison must use
actual rendering on both sides, with
the same device, pixels, glyphs, dimensions, completion boundary and presentation
mode, and verify the output before claiming a frame-time advantage.
