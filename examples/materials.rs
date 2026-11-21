//! Interactive materials and a bounded, machine-readable cache experiment.
//! No assets, background render thread, implicit clock, or external Rust crates.
use std::fmt::Write as _;
use std::time::{Duration, Instant};
use zgpui::effects::{
    CacheKey, EffectBudget, EffectCapabilities, EffectOutcome, EffectStats, Fold, FoldAxis,
    Material, Quality, ShaderHandle,
};
use zgpui::native::{ButtonStyle, Canvas, Event, Rect, Ui, Window};

const BG: u32 = 0xf8f9fbff;
const INK: u32 = 0x20252dff;
const MUTED: u32 = 0x687382ff;
const BLUE: u32 = 0x1a73e8ff;
const LINE: u32 = 0xe3e7edff;
const LABELS: [&str; 7] = [
    "Frosted",
    "Progressive blur",
    "Mica style",
    "Liquid glass",
    "Paper",
    "Spatial fold",
    "Custom shader",
];
const NAMES: [&str; 7] = [
    "frosted", "blur", "mica", "glass", "paper", "fold", "custom",
];
const SCENARIOS: [&str; 8] = [
    "frosted",
    "blur",
    "mica",
    "glass",
    "paper",
    "fold",
    "custom",
    "fold_content",
];
const CUSTOM_SHADER: &str = r#"
vec4 effect(vec2 local_uv, vec2 screen_uv, vec4 base_color) {
    float ribbon = 0.5 + 0.5 * sin(local_uv.y * 48.0 + u_params[1]);
    float light = pow(max(0.0, 1.0 - abs(local_uv.x - 0.45) * 1.8), 4.0);
    vec3 ink = mix(base_color.rgb, vec3(0.42, 0.51, 0.88), ribbon * u_params[0] * 0.32);
    return vec4(ink + light * u_params[0] * 0.08, base_color.a);
}
"#;

struct Options {
    bench: bool,
    iterations: usize,
    warmup: usize,
    quality: Quality,
    sample: usize,
    strength: f32,
    openness: f32,
    local_content: bool,
    duration: Option<Duration>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            bench: false,
            iterations: 120,
            warmup: 12,
            quality: Quality::Balanced,
            sample: 0,
            strength: 0.75,
            openness: 1.0,
            local_content: false,
            duration: None,
        }
    }
}

fn options() -> Result<Option<Options>, String> {
    let mut result = Options::default();
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--help" || argument == "-h" {
            println!(
                "Materials gallery\n\n  --sample frosted|blur|mica|glass|paper|fold|custom\n  --strength 0..1  --openness 0..1  --local-content\n  --quality disabled|low|balanced|high\n  --duration 1..120  (bounded gallery for screenshots/idle observation)\n  --bench --iterations 1..2000 --warmup 0..200\n\nMouse: select a material, drag sliders, choose quality.\nA: animate one fold; Escape: cancel; 1..7: select; Q: close.\nBenchmark prints JSON; normal gallery only redraws on input or a bounded animation."
            );
            return Ok(None);
        }
        if argument == "--bench" {
            result.bench = true;
            continue;
        }
        if argument == "--local-content" {
            result.local_content = true;
            continue;
        }
        let value = arguments
            .next()
            .ok_or_else(|| format!("{argument} needs a value"))?;
        match argument.as_str() {
            "--iterations" => {
                result.iterations = value.parse().map_err(|_| "invalid iteration count")?
            }
            "--warmup" => result.warmup = value.parse().map_err(|_| "invalid warmup count")?,
            "--quality" => {
                result.quality = match value.as_str() {
                    "disabled" => Quality::Off,
                    "low" => Quality::Low,
                    "balanced" => Quality::Balanced,
                    "high" => Quality::High,
                    _ => return Err("quality must be disabled, low, balanced or high".into()),
                }
            }
            "--sample" => {
                result.sample = NAMES
                    .iter()
                    .position(|name| *name == value)
                    .ok_or_else(|| format!("unknown sample: {value}"))?
            }
            "--strength" | "--openness" => {
                let parsed: f32 = value
                    .parse()
                    .map_err(|_| "expected a finite number from 0 to 1")?;
                if !parsed.is_finite() || !(0.0..=1.0).contains(&parsed) {
                    return Err("strength and openness must be finite numbers from 0 to 1".into());
                }
                if argument == "--strength" {
                    result.strength = parsed;
                } else {
                    result.openness = parsed;
                }
            }
            "--duration" => {
                let seconds: u64 = value
                    .parse()
                    .map_err(|_| "duration must be 1..120 seconds")?;
                if !(1..=120).contains(&seconds) {
                    return Err("duration must be 1..120 seconds".into());
                }
                result.duration = Some(Duration::from_secs(seconds));
            }
            _ => return Err(format!("unknown argument: {argument}")),
        }
    }
    if !(1..=2000).contains(&result.iterations) || result.warmup > 200 {
        return Err("iterations must be 1..2000 and warmup 0..200".into());
    }
    Ok(Some(result))
}

fn quality_name(quality: Quality) -> &'static str {
    match quality {
        Quality::Off => "disabled",
        Quality::Low => "low",
        Quality::Balanced => "balanced",
        Quality::High => "high",
    }
}

fn quoted(value: &str) -> String {
    let mut result = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            c if c < ' ' => {
                let _ = write!(result, "\\u{:04x}", c as u32);
            }
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

fn capabilities_json(c: EffectCapabilities) -> String {
    format!(
        "{{\"gpu\":{},\"software\":{},\"backdrop_capture\":{},\"blur\":{},\"progressive_blur\":{},\"projective_fold\":{},\"custom_shaders\":{},\"max_texture_size\":{},\"max_cache_entries\":{},\"max_shader_slots\":{},\"max_shader_source_bytes\":{},\"max_cache_bytes\":{}}}",
        c.gpu,
        c.software,
        c.backdrop_capture,
        c.blur,
        c.progressive_blur,
        c.projective_fold,
        c.custom_shaders,
        c.max_texture_size,
        c.max_cache_entries,
        c.max_shader_slots,
        c.max_shader_source_bytes,
        c.max_cache_bytes
    )
}

fn stats_delta(after: EffectStats, before: EffectStats) -> String {
    format!(
        "{{\"captures\":{},\"cache_hits\":{},\"cache_misses\":{},\"blur_passes\":{},\"draw_calls\":{},\"fallbacks\":{},\"evictions\":{},\"shader_compiles\":{}}}",
        after.captures.saturating_sub(before.captures),
        after.cache_hits.saturating_sub(before.cache_hits),
        after.cache_misses.saturating_sub(before.cache_misses),
        after.blur_passes.saturating_sub(before.blur_passes),
        after.draw_calls.saturating_sub(before.draw_calls),
        after.fallbacks.saturating_sub(before.fallbacks),
        after.evictions.saturating_sub(before.evictions),
        after.shader_compiles.saturating_sub(before.shader_compiles)
    )
}

fn backdrop(canvas: &mut Canvas<'_>, area: Rect, variant: u64) {
    let cool = variant.is_multiple_of(2);
    canvas.rect(area, 22.0, if cool { 0xe8eff8ff } else { 0xf5ece2ff });
    let size = area.w.min(area.h);
    canvas.rect(
        Rect::new(
            area.x + area.w * 0.08,
            area.y + area.h * 0.13,
            size * 0.68,
            size * 0.68,
        ),
        size * 0.34,
        if cool { 0xa8c9f4ff } else { 0xf0bca4ff },
    );
    canvas.rect(
        Rect::new(
            area.x + area.w * 0.55,
            area.y + area.h * 0.2,
            size * 0.42,
            size * 0.65,
        ),
        size * 0.21,
        if cool { 0xbedbceff } else { 0xc3ccedff },
    );
    canvas.rect(
        Rect::new(
            area.x + area.w * 0.2,
            area.y + area.h * 0.67,
            area.w * 0.6,
            22.0,
        ),
        11.0,
        0xffffffb0,
    );
    for index in 0..9 {
        let x = area.x + 24.0 + index as f32 * (area.w - 48.0) / 8.0;
        canvas.line(
            (x, area.y + 24.0),
            (x, area.y + area.h - 24.0),
            1.0,
            0xffffff78,
        );
    }
    canvas.text(
        "A scene owned by your window",
        area.x + 22.0,
        area.y + area.h - 40.0,
        13,
        false,
        0x42566fff,
    );
}

fn content_card(canvas: &mut Canvas<'_>, area: Rect) {
    canvas.rect(area, 22.0, 0xfafcffff);
    canvas.rect(
        Rect::new(area.x + 24.0, area.y + 24.0, 46.0, 5.0),
        2.5,
        BLUE,
    );
    canvas.text(
        "A captured layer",
        area.x + 24.0,
        area.y + 48.0,
        14,
        false,
        MUTED,
    );
    canvas.text(
        "Keep your place.",
        area.x + 24.0,
        area.y + 82.0,
        27,
        true,
        INK,
    );
    canvas.text(
        "Text and surface move together.",
        area.x + 24.0,
        area.y + 126.0,
        15,
        false,
        MUTED,
    );
    for row in 0..3 {
        canvas.rect(
            Rect::new(
                area.x + 24.0,
                area.y + 168.0 + row as f32 * 19.0,
                (area.w - 48.0) * (1.0 - row as f32 * 0.18),
                6.0,
            ),
            3.0,
            LINE,
        );
    }
}

fn material(sample: usize, strength: f32, openness: f32, shader: Option<ShaderHandle>) -> Material {
    let mut value = match sample {
        1 => Material::blur(),
        2 => Material::mica(),
        3 | 5 => Material::liquid_glass(),
        4 => Material::paper(),
        6 => shader.map_or_else(Material::solid, Material::custom),
        _ => Material::frosted(),
    };
    value.blur_radius *= strength;
    value.noise *= strength;
    value.refraction *= strength;
    value.highlight *= strength;
    value.tint_mix *= strength;
    if sample == 1 {
        value.progressive_blur = 0.8;
    }
    value.parameters[0] = strength;
    value.parameters[1] = openness * std::f32::consts::TAU;
    value
}

fn fold(openness: f32) -> Fold {
    Fold {
        axis: FoldAxis::Vertical,
        angle_degrees: 75.0 * (1.0 - openness),
        anchor: 0.0,
        perspective: 1000.0,
        dimming: 0.15,
        ..Fold::default()
    }
}

fn register_custom(window: &mut Window, capabilities: EffectCapabilities) -> Option<ShaderHandle> {
    if !capabilities.custom_shaders {
        return None;
    }
    match window.register_shader(CUSTOM_SHADER) {
        Ok(handle) => Some(handle),
        Err(error) => {
            eprintln!("Custom shader is unavailable: {error}");
            None
        }
    }
}

fn percentile(values: &mut [f64], fraction: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * fraction).ceil() as usize]
}

fn benchmark(options: &Options) -> Result<(), String> {
    let mut window = Window::new("zgpui materials benchmark", 960, 640)?;
    window.set_effects(options.quality, EffectBudget::default())?;
    let capabilities = window.effect_capabilities()?;
    let renderer = window.gpu();
    let shader = register_custom(&mut window, capabilities);
    let initial_size = window.size();
    let scene = Rect::new(24.0, 24.0, 912.0, 592.0);
    let card = Rect::new(250.0, 170.0, 460.0, 280.0);
    let mut rows = Vec::new();
    let mut skipped = Vec::new();
    for (sample, name) in SCENARIOS.into_iter().enumerate() {
        let local = sample == 7;
        if (sample == 6 && shader.is_none())
            || (local && (!capabilities.backdrop_capture || options.quality == Quality::Off))
        {
            skipped.push(format!("{{\"material\":{},\"reason\":\"required capture or shader capability is unavailable\"}}", quoted(name)));
            continue;
        }
        for invalidated in [false, true] {
            window.clear_effects()?;
            let mut submit = Vec::with_capacity(options.iterations);
            let mut completed = Vec::with_capacity(options.iterations);
            let mut before = window.effect_stats()?;
            let mut local_revision = None;
            for index in 0..options.warmup + options.iterations {
                if matches!(window.event(0), Some(Event::Close)) {
                    return Err("benchmark cancelled".into());
                }
                if window.size() != initial_size {
                    return Err("window resized during a fixed-size benchmark".into());
                }
                if index == options.warmup {
                    before = window.effect_stats()?;
                }
                let revision = if invalidated { index as u64 + 1 } else { 1 };
                let key = CacheKey::new(200, revision);
                let start = Instant::now();
                if local && local_revision != Some(revision) {
                    let mut canvas = window.begin(BG);
                    content_card(&mut canvas, card);
                    canvas.capture_effect(card, key)?;
                    local_revision = Some(revision);
                }
                {
                    let mut canvas = window.begin(BG);
                    backdrop(&mut canvas, scene, 0);
                    if local {
                        let mut plain = Material::blur();
                        plain.blur_radius = 0.0;
                        canvas.cached_material(card, key, &plain, &fold(0.6))?;
                    } else {
                        let effect = material(sample, 0.75, 0.6, shader);
                        let transform = if sample == 5 {
                            fold(0.6)
                        } else {
                            Fold::default()
                        };
                        canvas.material(card, key, &effect, &transform)?;
                    }
                }
                let submitted = start.elapsed().as_secs_f64() * 1_000_000.0;
                window.finish_effects()?;
                let finished = start.elapsed().as_secs_f64() * 1_000_000.0;
                if index >= options.warmup {
                    submit.push(submitted);
                    completed.push(finished);
                }
                window.present();
            }
            let after = window.effect_stats()?;
            let submit_median = percentile(&mut submit, 0.5);
            let submit_p95 = percentile(&mut submit, 0.95);
            let completed_median = percentile(&mut completed, 0.5);
            let completed_p95 = percentile(&mut completed, 0.95);
            rows.push(format!("{{\"material\":{},\"case\":{},\"submit_median_us\":{submit_median:.3},\"submit_p95_us\":{submit_p95:.3},\"completed_median_us\":{completed_median:.3},\"completed_p95_us\":{completed_p95:.3},\"texture_bytes\":{},\"peak_texture_bytes\":{},\"stats_delta\":{}}}",
                quoted(name), quoted(if invalidated { "invalidated" } else { "cached" }),
                after.texture_bytes, after.peak_texture_bytes, stats_delta(after, before)));
        }
    }
    if let Some(shader) = shader {
        window.unregister_shader(shader)?;
    }
    println!(
        "{{\"schema_version\":1,\"renderer\":{},\"quality\":{},\"iterations\":{},\"warmup\":{},\"size\":[{},{}],\"clock\":\"CPU submit and glFinish-synchronized frame wall time; presentation excluded\",\"capabilities\":{},\"results\":[{}],\"skipped\":[{}]}}",
        quoted(&renderer),
        quoted(quality_name(options.quality)),
        options.iterations,
        options.warmup,
        initial_size.0,
        initial_size.1,
        capabilities_json(capabilities),
        rows.join(","),
        skipped.join(",")
    );
    Ok(())
}

#[derive(Clone, Copy)]
enum Slider {
    Strength,
    Openness,
}

struct Animation {
    started: Instant,
    from: f32,
    to: f32,
}

struct Gallery {
    sample: usize,
    strength: f32,
    openness: f32,
    quality: Quality,
    local: bool,
    revision: u64,
    captured: Option<u64>,
    variant: u64,
    dragging: Option<Slider>,
    animation: Option<Animation>,
    frames: u64,
    notice: String,
}

impl Gallery {
    fn animate(&mut self) {
        self.sample = 5;
        self.animation = Some(Animation {
            started: Instant::now(),
            from: self.openness,
            to: if self.openness > 0.5 { 0.1 } else { 1.0 },
        });
    }
    fn advance(&mut self) -> bool {
        let Some(animation) = &self.animation else {
            return false;
        };
        let progress = (animation.started.elapsed().as_secs_f32() / 1.2).min(1.0);
        let eased = progress * progress * (3.0 - 2.0 * progress);
        self.openness = animation.from + (animation.to - animation.from) * eased;
        if progress == 1.0 {
            self.animation = None;
        }
        true
    }
    fn select(&mut self, sample: usize) {
        self.sample = sample;
        self.animation = None;
        if sample == 5 && self.openness == 1.0 {
            self.openness = 0.65;
        }
    }
}

struct Placement {
    preview: Rect,
    card: Rect,
    inspector_x: f32,
    strength: Rect,
    openness: Rect,
}

fn placement(width: f32, height: f32) -> Placement {
    let inspector_x = width - 280.0;
    let preview = Rect::new(
        224.0,
        144.0,
        (inspector_x - 252.0).max(120.0),
        (height - 350.0).max(280.0),
    );
    let card = Rect::new(
        preview.x + preview.w * 0.14,
        preview.y + (preview.h - 260.0) / 2.0,
        preview.w * 0.72,
        260.0,
    );
    Placement {
        preview,
        card,
        inspector_x,
        strength: Rect::new(inspector_x, 222.0, 244.0, 26.0),
        openness: Rect::new(inspector_x, 316.0, 244.0, 26.0),
    }
}

fn slider(canvas: &mut Canvas<'_>, rect: Rect, value: f32) {
    canvas.rect(Rect::new(rect.x, rect.y + 10.0, rect.w, 5.0), 2.5, LINE);
    canvas.rect(
        Rect::new(rect.x, rect.y + 10.0, rect.w * value, 5.0),
        2.5,
        BLUE,
    );
    canvas.rect(
        Rect::new(rect.x + rect.w * value - 7.0, rect.y + 5.0, 14.0, 14.0),
        7.0,
        BLUE,
    );
}

fn adjust(gallery: &mut Gallery, target: Slider, x: f32, layout: &Placement) {
    let rect = match target {
        Slider::Strength => layout.strength,
        Slider::Openness => layout.openness,
    };
    let value = ((x - rect.x) / rect.w).clamp(0.0, 1.0);
    match target {
        Slider::Strength => gallery.strength = value,
        Slider::Openness => gallery.openness = value,
    }
    gallery.animation = None;
}

enum Action {
    Select(usize),
    Quality(Quality),
    Backdrop,
    Clear,
    Animate,
    Cancel,
    Local,
}

fn controls(
    canvas: &mut Canvas<'_>,
    ui: &mut Ui,
    state: &Gallery,
    layout: &Placement,
    capabilities: EffectCapabilities,
    custom: bool,
) -> Option<Action> {
    let mut action = None;
    canvas.text("Surface", 32.0, 147.0, 13, false, MUTED);
    for (index, label) in LABELS.iter().enumerate() {
        if ui.button(
            canvas,
            index as u64 + 1,
            Rect::new(24.0, 180.0 + index as f32 * 45.0, 174.0, 38.0),
            label,
            if index == state.sample {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Quiet
            },
            index != 6 || custom,
        ) {
            action = Some(Action::Select(index));
        }
    }
    let x = layout.inspector_x;
    canvas.text("Adjust", x, 147.0, 13, false, MUTED);
    canvas.text(
        &format!("Strength  {:.0}%", state.strength * 100.0),
        x,
        192.0,
        16,
        false,
        INK,
    );
    slider(canvas, layout.strength, state.strength);
    canvas.text(
        &format!("Openness  {:.0}%", state.openness * 100.0),
        x,
        286.0,
        16,
        false,
        INK,
    );
    slider(canvas, layout.openness, state.openness);
    canvas.text("Quality", x, 376.0, 14, false, MUTED);
    for (index, (quality, label)) in [
        (Quality::Off, "Off"),
        (Quality::Low, "Low"),
        (Quality::Balanced, "Med"),
        (Quality::High, "High"),
    ]
    .into_iter()
    .enumerate()
    {
        if ui.button(
            canvas,
            20 + index as u64,
            Rect::new(x + index as f32 * 62.0, 402.0, 58.0, 34.0),
            label,
            if quality == state.quality {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Secondary
            },
            true,
        ) {
            action = Some(Action::Quality(quality));
        }
    }
    if ui.button(
        canvas,
        30,
        Rect::new(x, 454.0, 244.0, 38.0),
        if state.local {
            "Captured content follows fold"
        } else {
            "Backdrop locked to screen"
        },
        ButtonStyle::Secondary,
        state.sample == 5 && capabilities.backdrop_capture && state.quality != Quality::Off,
    ) {
        action = Some(Action::Local);
    }
    if ui.button(
        canvas,
        31,
        Rect::new(x, 504.0, 244.0, 38.0),
        if state.animation.is_some() {
            "Cancel animation"
        } else {
            "Animate once  /  1.2 s"
        },
        ButtonStyle::Secondary,
        true,
    ) {
        action = Some(if state.animation.is_some() {
            Action::Cancel
        } else {
            Action::Animate
        });
    }
    let y = layout.preview.y + layout.preview.h + 20.0;
    if ui.button(
        canvas,
        32,
        Rect::new(layout.preview.x, y, 174.0, 36.0),
        "Change backdrop",
        ButtonStyle::Secondary,
        true,
    ) {
        action = Some(Action::Backdrop);
    }
    if ui.button(
        canvas,
        33,
        Rect::new(layout.preview.x + 188.0, y, 136.0, 36.0),
        "Clear cache",
        ButtonStyle::Quiet,
        true,
    ) {
        action = Some(Action::Clear);
    }
    action
}

fn gallery(options: &Options) -> Result<(), String> {
    let mut window = Window::new("zgpui Materials", 1180, 760)?;
    window.set_effects(options.quality, EffectBudget::default())?;
    let capabilities = window.effect_capabilities()?;
    let renderer = window.gpu();
    let shader = register_custom(&mut window, capabilities);
    let mut state = Gallery {
        sample: options.sample,
        strength: options.strength,
        openness: options.openness,
        quality: options.quality,
        local: options.local_content,
        revision: 1,
        captured: None,
        variant: 0,
        dragging: None,
        animation: None,
        frames: 0,
        notice: String::new(),
    };
    if state.sample == 6 && shader.is_none() {
        state.notice = "Custom shaders are unavailable on this renderer.".into();
    }
    let mut ui = Ui::default();
    let start = Instant::now();
    let mut pending = None;
    let mut redraw = true;
    let mut next_animation_frame = Instant::now();
    loop {
        if options
            .duration
            .is_some_and(|duration| start.elapsed() >= duration)
        {
            break;
        }
        let (width, height) = window.size();
        let layout = placement(width, height);
        if redraw {
            let local = state.sample == 5
                && state.local
                && capabilities.backdrop_capture
                && state.quality != Quality::Off
                && width >= 1020.0
                && height >= 700.0;
            if local && state.captured != Some(state.revision) {
                let mut canvas = window.begin(BG);
                content_card(&mut canvas, layout.card);
                canvas.capture_effect(layout.card, CacheKey::new(100, state.revision))?;
                state.captured = Some(state.revision);
            }
            ui.begin(pending.take());
            let action;
            {
                let mut canvas = window.begin(BG);
                if width < 1020.0 || height < 700.0 {
                    canvas.text("Materials", 32.0, 36.0, 32, true, INK);
                    canvas.text(
                        "Resize to at least 1020 x 700 to use the gallery.",
                        32.0,
                        104.0,
                        16,
                        false,
                        MUTED,
                    );
                    action = None;
                } else {
                    canvas.text("zgpui / native renderer", 32.0, 28.0, 13, false, MUTED);
                    canvas.text("Materials", 32.0, 57.0, 32, true, INK);
                    canvas.text(
                        "Your scene. Explicit effects. Bounded resources.",
                        224.0,
                        71.0,
                        17,
                        false,
                        MUTED,
                    );
                    action = controls(
                        &mut canvas,
                        &mut ui,
                        &state,
                        &layout,
                        capabilities,
                        shader.is_some(),
                    );
                    backdrop(&mut canvas, layout.preview, state.variant);
                    canvas.clip(layout.preview);
                    let outcome = if local {
                        let mut plain = Material::blur();
                        plain.blur_radius = 0.0;
                        canvas.cached_material(
                            layout.card,
                            CacheKey::new(100, state.revision),
                            &plain,
                            &fold(state.openness),
                        )
                    } else {
                        let value = material(state.sample, state.strength, state.openness, shader);
                        let transform = if state.sample == 5 {
                            fold(state.openness)
                        } else {
                            Fold::default()
                        };
                        canvas.material(
                            layout.card,
                            CacheKey::new(10 + state.sample as u64, state.revision),
                            &value,
                            &transform,
                        )
                    };
                    canvas.unclip();
                    match outcome {
                        Ok(EffectOutcome::Fallback) => {
                            state.notice =
                                "Flat fallback active; GPU material is not being rendered.".into()
                        }
                        Ok(EffectOutcome::Rendered) => {
                            state.notice = if local { "The captured text and surface share one projective transform." }
                                else if state.sample == 5 { "The pane tilts while backdrop samples stay in screen space." }
                                else { "This is an in-window material, not an operating-system compositor surface." }.into();
                        }
                        Err(error) => state.notice = error,
                    }
                    let stats = canvas.effect_stats()?;
                    let footer = height - 104.0;
                    canvas.line(
                        (32.0, footer - 18.0),
                        (width - 32.0, footer - 18.0),
                        1.0,
                        LINE,
                    );
                    canvas.ellipsis(&state.notice, 32.0, footer, width - 64.0, 14, false, INK);
                    canvas.text(&format!("Effect textures {:.2} MiB   /   captures {}   hits {}   blur passes {}   fallbacks {}   frames {}",
                        stats.texture_bytes as f64 / 1_048_576.0, stats.captures, stats.cache_hits,
                        stats.blur_passes, stats.fallbacks, state.frames + 1), 32.0, footer + 26.0, 13, false, MUTED);
                    canvas.ellipsis(&format!("{}  |  capture:{}  blur:{}  fold:{}  shader:{}  |  A animate · Esc cancel · Q close",
                        renderer, capabilities.backdrop_capture, capabilities.blur, capabilities.projective_fold,
                        capabilities.custom_shaders), 32.0, footer + 52.0, width - 64.0, 12, false, MUTED);
                }
            }
            ui.end();
            window.present();
            state.frames += 1;
            redraw = false;
            if let Some(action) = action {
                match action {
                    Action::Select(sample) => state.select(sample),
                    Action::Quality(quality) => {
                        window.set_effects(quality, EffectBudget::default())?;
                        state.quality = quality;
                        state.captured = None;
                    }
                    Action::Backdrop => {
                        state.variant += 1;
                        state.revision += 1;
                        state.captured = None;
                    }
                    Action::Clear => {
                        window.clear_effects()?;
                        state.captured = None;
                    }
                    Action::Animate => state.animate(),
                    Action::Cancel => state.animation = None,
                    Action::Local => {
                        state.local = !state.local;
                        state.captured = None;
                    }
                }
                redraw = true;
            }
        }
        if redraw {
            continue;
        }
        let event = window.event(if state.animation.is_some() { 8 } else { 1000 });
        match event {
            Some(Event::Close) => break,
            Some(Event::Key { code: 81 | 113, .. }) => break,
            Some(Event::Key { code: 27, .. }) => {
                state.animation = None;
                redraw = true;
            }
            Some(Event::Key { code: 65 | 97, .. }) => {
                state.animate();
                redraw = true;
            }
            Some(Event::Key {
                code: code @ 49..=55,
                ..
            }) => {
                state.select((code - 49) as usize);
                redraw = true;
            }
            Some(Event::Key {
                code: code @ (1001 | 1002),
                ..
            }) => {
                state.openness =
                    (state.openness + if code == 1001 { -0.05 } else { 0.05 }).clamp(0.0, 1.0);
                state.animation = None;
                redraw = true;
            }
            Some(Event::Key {
                code: code @ (43 | 45 | 61),
                ..
            }) => {
                state.strength =
                    (state.strength + if code == 45 { -0.05 } else { 0.05 }).clamp(0.0, 1.0);
                state.animation = None;
                redraw = true;
            }
            Some(Event::Resize) => {
                state.revision += 1;
                state.captured = None;
                redraw = true;
            }
            Some(Event::Down(x, y)) => {
                state.dragging = if layout.strength.contains(x, y) {
                    Some(Slider::Strength)
                } else if layout.openness.contains(x, y) {
                    Some(Slider::Openness)
                } else {
                    None
                };
                if let Some(slider) = state.dragging {
                    adjust(&mut state, slider, x, &layout);
                }
                pending = Some(Event::Down(x, y));
                redraw = true;
            }
            Some(Event::Move(x, y)) => {
                if let Some(slider) = state.dragging {
                    adjust(&mut state, slider, x, &layout);
                    pending = Some(Event::Move(x, y));
                    redraw = true;
                } else if ui.pointer_changed(x, y) {
                    pending = Some(Event::Move(x, y));
                    redraw = true;
                }
            }
            Some(Event::Up(x, y)) => {
                state.dragging = None;
                pending = Some(Event::Up(x, y));
                redraw = true;
            }
            Some(event) => {
                pending = Some(event);
                redraw = true;
            }
            None => {}
        }
        if state.animation.is_some() && Instant::now() >= next_animation_frame {
            redraw |= state.advance();
            next_animation_frame = Instant::now() + Duration::from_millis(16);
        }
    }
    let stats = window.effect_stats()?;
    println!(
        "{{\"schema_version\":1,\"mode\":\"gallery\",\"renderer\":{},\"presented_frames\":{},\"wall_seconds\":{:.3},\"texture_bytes\":{},\"captures\":{},\"blur_passes\":{},\"capabilities\":{}}}",
        quoted(&renderer),
        state.frames,
        start.elapsed().as_secs_f64(),
        stats.texture_bytes,
        stats.captures,
        stats.blur_passes,
        capabilities_json(capabilities)
    );
    if let Some(shader) = shader {
        window.unregister_shader(shader)?;
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let Some(options) = options()? else {
        return Ok(());
    };
    if options.bench {
        benchmark(&options)
    } else {
        gallery(&options)
    }
}
