# Headless rendering comparison: source preparation

**Status: prepared source, not compiled or measured.** Local Cargo and GPU work is
paused because the development machine has less than the reserved memory
headroom. There is no headless result, no performance winner, and no new remote
benchmark workflow. The runnable CPU fixture remains the separate checkpoint
`83369dbcea2ca7bca943a050dc2fa06869723f37`.

## Question and comparison boundary

Can the two layout frontends produce the same bounded grid, and what does each
frontend contribute to the cost of producing that grid on the same renderer?

The first rendering experiment uses zgpui's `Layout::paint` contract and GPUI's
production Taffy layout engine. Both construct the same GPUI quad scene and use
one actual `WgpuHeadlessRenderer` instance. This compares frontend layout and
scene construction, plus the common backend's completed rendering path. It does
not compare two independent GPU engines: zgpui 0.1.0 delegates rendering to its
host and does not implement a GPU renderer.

This scope excludes text shaping, fonts, images, native window presentation,
platform input dispatch, application startup and compositor latency. Empty
button labels are rendered as alternating opaque red and blue rectangles. No
omitted text or window service can be counted as a speed advantage.

## Fixed sources and separate dependency graph

- zgpui 0.1.0: `1d796d1fc63808dc099ed7b10c92e75835ae2814`.
- Zed GPUI 0.2.2: `a246423a3b24a08e0d0c8574eab3a9de422747e0`.
- CPU source checkpoint: `83369dbcea2ca7bca943a050dc2fa06869723f37`.
- CPU Cargo.lock SHA-256:
  `23dcd455e3326f9836ea9da927d3014376db6656e56228f55e6421b5ae7ff490`
  (424 package entries, including the fixture; 423 dependencies).
- Compilation settings remain Rust 1.100.0, release opt-level 3, thin LTO and one
  codegen unit for both frontends.

The pinned upstream renderer file has SHA-256
`2d237ad9032cea4469c809e87eb2f89c82f3ee66b5893a7a6254577992224257`.
The observability patch has SHA-256
`41df78c530102e3361061a80397f826b7767a02707be4d6ddafe9e4887b7e0d9`.

The prepared module is deliberately outside `src/` and is not referenced by the
current Cargo manifest. Adding `gpui_wgpu` with its `bench-support` feature is a
new dependency graph. It must receive its own resolved, committed lock and exact
source provenance before compilation or measurement. It must not silently
replace the CPU lock.

`renderer.rs` implements the real zgpui renderer adapter, an independent grid
oracle, scene and pixel validation, actual completed GPU readback, and adapter
metadata capture. It is not yet a complete timing driver. It needs the same
pinned `gpui`, `gpui_wgpu`, `zgpui`, `image`, and `serde` dependencies when
integrated into that separate fixture.

`gpui-headless-observability.patch` adds two read-only inspection methods to the
headless renderer. It exposes adapter identity and the actual target format; it
does not change adapter selection, scene construction, shaders, allocation,
submission, synchronization or rendering. Record this patch separately from the
CPU fixture's existing Taffy visibility patch. Verify the original file and
reverse patch before building.

## Independent correctness gate

The scene specification is fixed by arithmetic independent of either layout
engine: side lengths 1, 4, 8 and 15; 24-pixel leaf height; four-pixel gaps; leaf
width 32 through 36; origin (0, 0); scale 1; opaque black background; alternating
opaque red and blue leaves; no borders or rounded corners. All dimensions are
integer physical pixels, and every rectangle is non-overlapping.

Before timing, each frontend's scene must separately match the oracle's
primitive count, order, bounds, clip, color, border and corner settings. Both
images must separately match every oracle pixel, including the black gaps, and
must be byte-identical to each other. The per-channel oracle tolerance is at
most three byte values, matching the upstream renderer tests; mutual image
equality remains exact. An empty scene or a backend that discards drawing cannot
pass. Neither frontend is treated as the other's expected answer.

The adapter reports the actual selected device, vendor/device IDs, backend,
driver, driver information and target format. A CPU/software adapter is rejected
by the default hardware gate. If deliberately tested, it must be a separately
labelled software-rendering run. Unknown adapter types cannot substantiate a
hardware-GPU claim.

## Completion boundary and measurements

At the pinned source, `render_scene` returns after submitting rendering work.
It is not a GPU completion fence.
[`render_scene_to_image`](https://github.com/zed-industries/zed/blob/a246423a3b24a08e0d0c8574eab3a9de422747e0/crates/gpui_wgpu/src/wgpu_renderer.rs)
renders, submits a copy, waits for that submission, maps the buffer, removes row
padding, and returns RGBA pixels. The prepared module times that entire call.
The result must be labelled **completed render plus readback**, including its
buffer allocation and copy costs. It is not GPU execution time or presented
frame latency.

The eventual driver must report these boundaries separately:

| Measurement | Included work |
| --- | --- |
| Layout plus scene construction | Frontend layout and adapter work through `Scene::finish` |
| Completed render plus readback | The same Wgpu renderer call for each prevalidated scene |
| Full path | One continuous timer around both preceding operations |
| Device initialization | The first renderer construction in a fresh process |
| First frame | First target/pipeline use, distinct from warmed frames |

Do not obtain the full-path metric by adding independently sampled percentiles.
Scene creation must be inside or outside the timer consistently on both sides.
Image comparison, checksum calculation, JSON output and file writing belong
outside the measured rendering interval.

One renderer guarantees a common device and pipeline configuration, but also
shares pipeline and resource caches. Warm both frontends and all measured sizes
first, then use paired, interleaved A/B order with a recorded random seed. Retain
every raw sample; preserve each frontend's production cache behavior. Record
common CPU affinity/niceness, source and binary hashes, GPU metadata, load,
frequency observations, scheduling wait and process memory. Do not reuse busy
host samples as a final performance result. Any GPU memory metric needs an
actual measurement source; process RSS is not GPU memory.

## Remaining execution gates

1. Keep the existing CPU baseline immutable; resolve the separate GPU graph.
2. Compile the prepared module, run geometry/scene/pixel gates, and fix actual
   compiler or verifier findings before recording timing.
3. Add the paired driver and raw-output schema with the boundaries above.
4. Run only after the resource owner authorizes sufficient memory and GPU
   headroom; a source checkpoint is not an execution approval.
5. Publish raw samples and a reproducible report that distinguishes frontend
   results, common-backend costs, and untested full-application capabilities.

No benchmark workflow has been added. The existing repository CI and crate
publication workflows are not substituted for a benchmark artifact pipeline.
