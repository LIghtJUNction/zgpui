# Native materials

The optional native renderer can capture a region of its own window, reuse that
image, and draw it through a material or a perspective fold. Applications control
the cache identity, quality, texture budget, custom shader parameters and redraw
schedule. The portable scene core still builds without native effects.

These are **in-window effects**. The mica-style preset does not request Windows
DWM Mica or read the desktop wallpaper. Liquid glass does not call an Apple
compositor material API. A material samples content already drawn by the
application inside its window; it does not capture other applications.

## Run the gallery

Use the platform libraries listed in [native integration](native.md), then run:

```sh
cargo run --locked --release --features native --example materials
```

The gallery offers frosted glass, progressive blur, mica style, liquid glass,
paper, a spatial fold and an application-defined shader. Drag the strength and
openness sliders, select a quality level, or change the backdrop. The footer
shows the actual renderer, capabilities and material resource counters. A
fallback is identified explicitly in the status text.

The fold has two distinct sampling modes. **Backdrop locked to screen** keeps
the background in window coordinates while the pane tilts. **Captured content
follows fold** rotates a captured card, including its text, as one plane. The
hinge stays at the same target position. This is a perspective transform of a
flat layer, not a general 3D scene graph or a physically based optical model.

The interface requires at least 1020 × 700 pixels. Keyboard shortcuts are
`1`–`7` to select a material, left/right arrows to adjust openness, `-`/`+` to
adjust strength, `A` to animate one fold, `Escape` to cancel and `Q` to close.
The animation lasts 1.2 seconds and has no automatic repeat. Pointer movement
outside changed widget targets does not trigger a new frame. When no animation
or relevant input is pending, the loop waits for events and does not redraw.

For a bounded screenshot or idle observation:

```sh
cargo run --locked --release --features native --example materials -- \
  --sample fold --openness 0.6 --duration 5

cargo run --locked --release --features native --example materials -- \
  --sample fold --openness 0.6 --local-content --duration 5
```

The process prints its actual frame count and resource counters when it exits.
Initial expose/resize events and user input can add frames; the timer itself
does not request frames. The duration check runs after bounded event waits, so
exit can lag the requested duration by up to approximately one second.

## Enable effects explicitly

The descriptions live in [`zgpui::effects`](../src/effects.rs). Construction and
validation of these values perform no GPU work. The native window methods are
implemented in [`zgpui::native`](../src/native.rs).

```rust,no_run
use zgpui::effects::{CacheKey, EffectBudget, Fold, Material, Quality};
use zgpui::native::{Rect, Window};

let mut window = Window::new("Material", 960, 640)?;
window.set_effects(Quality::Balanced, EffectBudget::default())?;
let capabilities = window.effect_capabilities()?;

{
    let mut canvas = window.begin(0xf8f9fbff);
    canvas.rect(Rect::new(80., 80., 340., 260.), 30., 0x9dc6efff);
    let outcome = canvas.material(
        Rect::new(180., 140., 420., 280.),
        CacheKey::new(1, 1),
        &Material::frosted(),
        &Fold::default(),
    )?;
    // Inspect outcome: Rendered and Fallback are different results.
    let submitted = canvas.effect_stats()?;
}
window.present();
# Ok::<(), String>(())
```

`Quality::Off` requests the flat fallback. `Low`, `Balanced` and `High` select
the backend's quality settings. The gallery starts at `Balanced`; applications
choose their own policy. A quality label is not proof of hardware acceleration:
an OpenGL implementation such as Mesa llvmpipe may execute shaders on a CPU.
Keep the reported driver string alongside measurements.

`EffectCapabilities` reports capture, blur, progressive blur, custom shader and
projective-fold availability for the current renderer. Its limits describe
supported texture dimensions, cache entries and shader registrations. Check the
returned `EffectOutcome` as well: a supported effect can still fall back when
its current configuration or allocation cannot be served.

| Runtime path | Meaning |
| --- | --- |
| Shader/FBO capabilities available | Materials can use the OpenGL effect pipeline. The driver string identifies the implementation; this alone does not establish physical GPU execution. |
| Legacy OpenGL without required capabilities | The renderer reports the unavailable features and uses a flat fallback where supported. |
| Linux software feature | The CPU window renderer remains available; callers must inspect its capabilities and fallback result instead of assuming GPU effects. |
| Explicit cached-layer operation without capture support | The operation cannot create or use the required captured layer; callers handle the returned error. The benchmark records this scenario as skipped. |

## Keep source identity separate from appearance

A `CacheKey` contains a nonzero application-owned ID and a content revision.
Keep the key stable while the captured pixels are unchanged. Increase its
revision when the backdrop or captured content changes. A source rectangle
change also changes the capture identity. Reusing a revision after changing
pixels intentionally asks the renderer to reuse the old image.

Target position, fold angle and material parameters are separate from source
identity. They can change without recapturing the backdrop. A blur configuration
change may require new derived blur work even when the source capture is reused;
the capture and blur-pass counters expose those different operations.

`Canvas::material` captures and renders at the same rectangle.
`Canvas::material_at` accepts separate source and target rectangles, with
screen-space backdrop sampling. `Window::invalidate_effect(id)` removes a
specific captured identity. `Window::clear_effects()` releases cached capture
and blur textures while keeping registered custom shaders alive.

For a complete folding layer, capture its text and surface first, clear/repaint
the window, then draw the saved layer with `cached_material`:

```rust,no_run
# use zgpui::effects::{CacheKey, EffectBudget, Fold, FoldAxis, Material, Quality};
# use zgpui::native::{Rect, Window};
# let mut window = Window::new("Layer", 960, 640)?;
# window.set_effects(Quality::Balanced, EffectBudget::default())?;
let source = Rect::new(160., 140., 420., 260.);
let key = CacheKey::new(7, 1);
{
    let mut canvas = window.begin(0xf8f9fbff);
    canvas.rect(source, 20., 0xfafcffff);
    canvas.text("The text folds too", 190., 190., 24, true, 0x20252dff);
    canvas.capture_effect(source, key)?;
}
{
    let mut canvas = window.begin(0xf8f9fbff);
    let mut image = Material::blur();
    image.blur_radius = 0.;
    let fold = Fold {
        axis: FoldAxis::Vertical,
        angle_degrees: 35.,
        anchor: 0.,
        ..Fold::default()
    };
    canvas.cached_material(source, key, &image, &fold)?;
}
window.present();
# Ok::<(), String>(())
```

`cached_material` is cache-only and attaches texture coordinates to the folded
layer. It never silently captures a replacement. Use a material with transparent
tint, such as the zero-radius `Material::blur()` above, when preserving the
captured text. An opaque paper tint would cover that image. Recapture after a
resize or an explicit cache clear before drawing the layer again.

## Customize the material and shader

The built-in constructors produce editable `Material` values. Fields include
tint, tint mix, opacity, corner and blur radii, saturation, grain, refraction,
highlight, border width and progressive blur. Colors are literal `0xRRGGBBAA`
values and do not pass through the widget theme palette. Validation rejects
nonfinite and out-of-range parameters before native renderer state changes.

A custom shader is a GLSL 1.20 fragment function. The renderer supplies `main`,
rounded clipping, opacity and the projective vertex transform. The input
`base_color` is the sampled material color; `local_uv` belongs to the surface,
and `screen_uv` refers to the screen sampling convention. Eight explicit floats
are exposed as `u_params`. There is no implicit time uniform.

```glsl
vec4 effect(vec2 local_uv, vec2 screen_uv, vec4 base_color) {
    float stripe = 0.5 + 0.5 * sin(local_uv.y * 48.0 + u_params[1]);
    vec3 ink = mix(base_color.rgb, vec3(0.42, 0.51, 0.88),
                   stripe * u_params[0] * 0.32);
    return vec4(ink, base_color.a);
}
```

Register source once with `Window::register_shader`, construct
`Material::custom(handle)`, and update `material.parameters` when application
state changes. Registration is synchronous and returns compilation diagnostics;
it belongs in preparation or an explicit editor action, not an ordinary frame.
`unregister_shader` releases the registration. Removed handles and handles from
another window are rejected rather than treated as raw native program IDs.

Shader source is application code. Source length, registration count and
parameter bounds are enforced; these checks do not establish a shader's execution
time. Measure the custom workload on its intended driver. The gallery's ribbon
shader is included as source and has no downloaded images, models or other
binary assets.

## Resource limits and counters

The default effect budget is 32 MiB of owned effect textures, 16 cache entries
and 8 custom shader slots. The hard cache ceiling is 256 MiB and shader source
is bounded to 16 KiB per registration. Applications can select smaller budgets
with `EffectBudget`; the renderer evicts cached textures to respect those
limits. A cached-content caller must be ready to recapture an evicted layer.

`EffectStats` separates current texture bytes, lifetime peak texture bytes,
capture count, cache hits/misses, blur passes, effect draws, evictions, shader
compilations and fallback draws. `Canvas::effect_stats` reads those counters
without forcing GPU completion, so a gallery can display this frame's submitted
operations without adding a redraw loop or a synchronization stall.

Texture bytes cover this effect cache's owned texture storage. They are not
process RSS, total GPU memory, driver overhead, font-cache memory, backbuffer
storage or compositor memory. Peak bytes are cumulative for the window, not a
fresh per-scenario memory measurement. Cache hits also do not imply zero draw
cost: the saved texture still has to be shaded and composited.

## Reproduce cached and invalidated workloads

Build the example once, then pass the resulting executable to the repetition
runner. Set an absolute target directory so generated files stay outside source
history. On a shared Project Z development machine, use its prescribed target
directory and outer Cargo lock instead.

```sh
export CARGO_TARGET_DIR=/tmp/zgpui-materials-build
cargo build --locked --release --features native --example materials
python3 scripts/bench_materials.py \
  --binary "$CARGO_TARGET_DIR/release/examples/materials" \
  --output /tmp/zgpui-materials-balanced \
  --quality balanced --iterations 120 --warmup 12 --repeats 3 \
  --label "Record the GPU, driver, display and power mode here"
```

Windows executables have an `.exe` suffix. Native Linux runs need a usable X11
display. `xvfb-run` can provide a display for automated checks, but its OpenGL
driver may be software-backed; record that outcome rather than calling it a
hardware-GPU measurement. A separate build with `--features linux-software`
tests the CPU fallback path. Use a distinct output directory for each quality,
renderer or build.

The fixed-size probe renders a 960 × 640 window with a 460 × 280 material region.
Each scenario uses strength 0.75 and a fixed geometric backdrop. The spatial
and captured-content folds use openness 0.6. Cached iterations retain the same
source key; invalidated iterations change its revision each frame. The
captured-content case includes repainting the layer when its revision changes.
Unsupported custom-shader or captured-layer scenarios are listed under
`skipped`, rather than being relabelled as successful effect measurements.
With `--warmup 0`, the first cached iteration includes its initial capture;
use a nonzero warmup for a steady-state cache comparison.

Each measured iteration records CPU submission time from `begin` through effect
drawing, then calls `Window::finish_effects()` and records synchronized frame
wall time. On the OpenGL backend this explicitly finishes submitted GL work;
the CPU fallback is already synchronous. The second interval includes CPU work, driver overhead and waiting
for GPU completion; it is **not a GPU timestamp-query duration**. Presentation
is outside both intervals to avoid directly timing a swap/VSync wait. Warmup
frames are excluded. The example does not call this synchronization method
during normal interactive drawing.

The runner preserves every raw JSON result and stderr log, the executable
SHA-256, machine annotation and command line. It checks scenario identity,
capability consistency and finite nonnegative measurements before reporting the
median of each repetition's median and p95 values. A median of per-run p95s is
not a pooled-sample percentile. Capture/pass/fallback counter deltas remain
available per repetition instead of being averaged away.

Use the evidence to answer separate questions:

| Question | Relevant evidence |
| --- | --- |
| Did a stable source avoid recapture? | Cached `stats_delta.captures`, alongside actual capabilities and fallback count. |
| Did invalidation request fresh source work? | Invalidated capture and blur-pass deltas for the same material, size and driver. |
| How much texture storage remained live? | `texture_bytes`; compare with the configured budget and lifetime `peak_texture_bytes`. |
| How expensive was submitting the frame? | `submit_median_us` and `submit_p95_us`. |
| How long until the submitted frame completed? | `completed_median_us` and `completed_p95_us`, with the synchronization definition above. |
| Did the renderer produce the requested effect? | Capabilities, returned fallback/error state, and visual capture on that backend. Timing alone cannot establish visual correctness. |

## Validation scope

The example and measurement runner define reproducible workloads; they do not
establish a universal performance advantage over another UI framework or an
operating-system material. Publish comparisons only with matched dimensions,
quality, driver, build, power state and visible output. Include software-driver
runs as their own category.

Interactive visual appearance, GPU timing and real memory behavior require a
native run. Rust parameter tests can establish input validation, and CPU tests
can establish fallback behavior, but neither substitutes for a real shader/FBO
run. Cross-platform compile success also does not establish visual parity.
Keep actual platform screenshots and raw measurement artifacts outside Git;
record the tested commit and remaining untested platforms with the results.

When a fold is active, positive progressive blur increases with distance from
the selected hinge, along its fold axis; negative values reverse that gradient.
Fold dimming also increases away from the hinge. A surface without a fold
retains the vertical blur gradient and uniform dimming.
