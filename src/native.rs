//! Native GPU windows and small immediate widgets. No third-party Rust crates.
//! All window, event and OpenGL work stays on the thread that created the window.
use crate::effects::{
    CacheKey, EffectBudget, EffectCapabilities, EffectOutcome, EffectStats, Fold, FoldAxis,
    MAX_SHADER_SLOTS, MAX_SHADER_SOURCE_BYTES, Material, MaterialPreset, Quality, ShaderHandle,
};
use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    marker::PhantomData,
    rc::Rc,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
unsafe extern "C" {
    fn zgpui_open(
        title: *const c_char,
        width: c_int,
        height: c_int,
        error: *mut c_char,
        capacity: usize,
    ) -> *mut c_void;
    fn zgpui_close(window: *mut c_void);
    fn zgpui_event(window: *mut c_void, event: *mut NativeEvent, timeout: c_int) -> c_int;
    fn zgpui_size(window: *mut c_void, width: *mut c_int, height: *mut c_int);
    fn zgpui_swap(window: *mut c_void);
    fn zgpui_clipboard(window: *mut c_void, text: *const c_char);
    fn zgpui_set_font_family(
        window: *mut c_void,
        family: *const c_char,
        error: *mut c_char,
        capacity: usize,
    ) -> c_int;
    fn zgpui_begin(width: c_int, height: c_int, background: u32);
    fn zgpui_rect(x: f32, y: f32, width: f32, height: f32, radius: f32, color: u32);
    fn zgpui_image(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        image_width: u32,
        image_height: u32,
        rgba: *const u8,
        byte_len: usize,
    ) -> c_int;
    fn zgpui_polygon(points: *const f32, count: usize, color: u32);
    fn zgpui_line(x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: u32);
    fn zgpui_text(text: *const c_char, x: f32, y: f32, size: c_int, bold: c_int, color: u32)
    -> f32;
    fn zgpui_measure(text: *const c_char, size: c_int, bold: c_int) -> f32;
    fn zgpui_clip(x: c_int, y: c_int, width: c_int, height: c_int);
    fn zgpui_unclip();
    fn zgpui_gpu_info() -> *const c_char;
    fn zgpui_effect_configure(window: *mut c_void, config: *const NativeEffectConfig) -> c_int;
    fn zgpui_effect_capabilities(window: *mut c_void, out: *mut NativeEffectCapabilities) -> c_int;
    fn zgpui_effect_stats(window: *mut c_void, out: *mut NativeEffectStats) -> c_int;
    fn zgpui_draw_effect(window: *mut c_void, effect: *const NativeEffect) -> c_int;
    fn zgpui_effect_capture(window: *mut c_void, capture: *const NativeEffectCapture) -> c_int;
    fn zgpui_effect_invalidate(window: *mut c_void, id: u64) -> c_int;
    fn zgpui_effect_clear(window: *mut c_void) -> c_int;
    fn zgpui_effect_shader_register(
        window: *mut c_void,
        source: *const c_char,
        length: usize,
        slot: *mut u32,
        error: *mut c_char,
        capacity: usize,
    ) -> c_int;
    fn zgpui_effect_shader_remove(window: *mut c_void, slot: u32) -> c_int;
    fn zgpui_effect_status(status: c_int) -> *const c_char;
    fn zgpui_effect_finish(window: *mut c_void) -> c_int;
}

const EFFECT_ABI: u32 = 1;
const EFFECT_LAYER: u32 = 1;
const EFFECT_CACHE_ONLY: u32 = 2;

#[repr(C)]
struct NativeEffectConfig {
    abi_version: u32,
    quality: u32,
    max_entries: u32,
    max_shader_slots: u32,
    max_texture_bytes: u64,
}

#[derive(Default)]
#[repr(C)]
struct NativeEffectCapabilities {
    abi_version: u32,
    flags: u32,
    max_texture_size: u32,
    max_entries: u32,
    max_shader_slots: u32,
    max_shader_bytes: u32,
    max_texture_bytes: u64,
}

#[derive(Default)]
#[repr(C)]
struct NativeEffectStats {
    texture_bytes: u64,
    peak_texture_bytes: u64,
    captures: u64,
    cache_hits: u64,
    cache_misses: u64,
    evictions: u64,
    blur_passes: u64,
    draw_calls: u64,
    shader_compiles: u64,
    fallbacks: u64,
    live_entries: u32,
    live_programs: u32,
    last_status: u32,
    quality: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
struct NativeEffectCapture {
    id: u64,
    revision: u64,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[repr(C)]
struct NativeEffect {
    source: NativeEffectCapture,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
    blur_radius: f32,
    saturation: f32,
    opacity: f32,
    tint_rgba: u32,
    preset: u32,
    tint_mix: f32,
    noise: f32,
    refraction: f32,
    border: f32,
    highlight: f32,
    progressive_blur: f32,
    fold_progress: f32,
    fold_radians: f32,
    fold_perspective: f32,
    dim: f32,
    fold_anchor: f32,
    fold_axis: u32,
    flags: u32,
    shader_slot: u32,
    shader_params: [f32; 8],
}

fn effect_status(status: c_int) -> Result<(), String> {
    if status == 0 {
        return Ok(());
    }
    let message = unsafe { zgpui_effect_status(status) };
    Err(if message.is_null() {
        format!("native material operation failed ({status})")
    } else {
        unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    })
}

fn registered_shader(
    owner: u64,
    shaders: &[Option<ShaderHandle>; MAX_SHADER_SLOTS],
    handle: ShaderHandle,
) -> Result<u32, String> {
    if handle.owner != owner || !shaders.contains(&Some(handle)) {
        return Err("Shader handle belongs to another window or has been removed".into());
    }
    Ok(handle.native)
}

fn effect_rectangle(rect: Rect) -> Result<Rect, String> {
    if [rect.x, rect.y, rect.w, rect.h]
        .iter()
        .any(|value| !value.is_finite())
    {
        return Err("Material rectangles must be finite".into());
    }
    if rect.x.abs() > 65_536.0
        || rect.y.abs() > 65_536.0
        || !(1.0..=65_536.0).contains(&rect.w)
        || !(1.0..=65_536.0).contains(&rect.h)
    {
        return Err("Material rectangles require positive dimensions within 65536 pixels".into());
    }
    Ok(rect)
}

fn capture_request(rect: Rect, key: CacheKey) -> Result<NativeEffectCapture, String> {
    key.validate().map_err(|error| error.to_string())?;
    let rect = effect_rectangle(rect)?;
    Ok(NativeEffectCapture {
        id: key.id,
        revision: key.revision,
        x: rect.x,
        y: rect.y,
        width: rect.w,
        height: rect.h,
    })
}

fn shader_source(source: &str) -> Result<(), String> {
    if source.is_empty() || source.len() > MAX_SHADER_SOURCE_BYTES || source.as_bytes().contains(&0)
    {
        return Err("Custom shader source must contain 1..16384 UTF-8 bytes without NUL".into());
    }
    Ok(())
}

fn read_effect_stats(window: *mut c_void) -> Result<EffectStats, String> {
    let mut raw = NativeEffectStats::default();
    effect_status(unsafe { zgpui_effect_stats(window, &mut raw) })?;
    let quality = match raw.quality {
        0 => Quality::Off,
        1 => Quality::Low,
        2 => Quality::Balanced,
        3 => Quality::High,
        _ => return Err("Native material renderer reported an invalid quality".into()),
    };
    Ok(EffectStats {
        texture_bytes: raw.texture_bytes,
        peak_texture_bytes: raw.peak_texture_bytes,
        captures: raw.captures,
        cache_hits: raw.cache_hits,
        cache_misses: raw.cache_misses,
        evictions: raw.evictions,
        blur_passes: raw.blur_passes,
        draw_calls: raw.draw_calls,
        shader_compiles: raw.shader_compiles,
        fallbacks: raw.fallbacks,
        live_entries: raw.live_entries,
        live_programs: raw.live_programs,
        last_status: raw.last_status,
        quality,
    })
}

#[cfg(test)]
mod effect_tests {
    use super::*;
    use core::mem::{offset_of, size_of};

    #[test]
    fn effect_abi_matches_the_versioned_c_header() {
        assert_eq!(size_of::<NativeEffectConfig>(), 24);
        assert_eq!(size_of::<NativeEffectCapabilities>(), 32);
        assert_eq!(size_of::<NativeEffectStats>(), 96);
        assert_eq!(size_of::<NativeEffectCapture>(), 32);
        assert_eq!(size_of::<NativeEffect>(), 160);
        assert_eq!(offset_of!(NativeEffect, x), 32);
        assert_eq!(offset_of!(NativeEffect, tint_rgba), 64);
        assert_eq!(offset_of!(NativeEffect, highlight), 88);
        assert_eq!(offset_of!(NativeEffect, fold_progress), 96);
        assert_eq!(offset_of!(NativeEffect, shader_params), 128);
    }

    #[test]
    fn copied_shader_handles_do_not_survive_removal_or_window_reuse() {
        let old = ShaderHandle {
            owner: 1,
            generation: 1,
            native: 1,
        };
        let replacement = ShaderHandle {
            generation: 2,
            ..old
        };
        let other_window = ShaderHandle { owner: 2, ..old };
        let mut shaders = [None; MAX_SHADER_SLOTS];
        shaders[0] = Some(old);
        assert_eq!(registered_shader(1, &shaders, old).unwrap(), 1);
        assert!(registered_shader(2, &shaders, old).is_err());
        assert!(registered_shader(1, &shaders, other_window).is_err());
        shaders[0] = Some(replacement);
        assert!(registered_shader(1, &shaders, old).is_err());
        assert_eq!(registered_shader(1, &shaders, replacement).unwrap(), 1);
        shaders[0] = None;
        assert!(registered_shader(1, &shaders, replacement).is_err());
    }

    #[test]
    fn capture_keys_and_rectangles_are_validated_before_native_calls() {
        assert!(capture_request(Rect::new(1.0, 2.0, 64.0, 32.0), CacheKey::new(0, 1)).is_err());
        for rect in [
            Rect::new(f32::NAN, 0.0, 64.0, 32.0),
            Rect::new(0.0, 0.0, f32::INFINITY, 32.0),
            Rect::new(0.0, 0.0, 0.0, 32.0),
            Rect::new(0.0, 0.0, 65_537.0, 32.0),
        ] {
            assert!(capture_request(rect, CacheKey::new(1, 1)).is_err());
        }
        let capture =
            capture_request(Rect::new(1.0, 2.0, 64.0, 32.0), CacheKey::new(7, 9)).unwrap();
        assert_eq!((capture.id, capture.revision), (7, 9));
        assert_eq!(
            (capture.x, capture.y, capture.width, capture.height),
            (1.0, 2.0, 64.0, 32.0)
        );
    }

    #[test]
    fn custom_shader_source_is_bounded_and_never_nul_truncated() {
        assert!(shader_source("").is_err());
        assert!(shader_source("vec4 effect(){}\0ignored").is_err());
        assert!(shader_source(&" ".repeat(MAX_SHADER_SOURCE_BYTES + 1)).is_err());
        assert!(shader_source(&" ".repeat(MAX_SHADER_SOURCE_BYTES)).is_ok());
    }
}
#[repr(C)]
struct NativeEvent {
    kind: c_int,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    key: c_int,
    modifiers: c_int,
    text: [c_char; 4096],
}
#[derive(Clone, Debug)]
pub enum Event {
    Move(f32, f32),
    Down(f32, f32),
    Up(f32, f32),
    Key {
        code: i32,
        control: bool,
        shift: bool,
        text: String,
    },
    Text(String),
    Resize,
    Scroll(i32),
    Close,
}
static WINDOW_OPEN: AtomicBool = AtomicBool::new(false);
static EFFECT_OWNER: AtomicU64 = AtomicU64::new(1);

/// The built-in palette. Custom colors are preserved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}
impl Theme {
    pub fn color(self, value: u32) -> u32 {
        if self == Self::Light {
            return value;
        }
        match value {
            0xffffffff => 0x17191dff,
            0xf7f9fcff | 0xf1f3f4ff => 0x202329ff,
            0x202124ff => 0xe8eaedff,
            0x5f6368ff => 0xaeb4bfff,
            0xe8eaedff | 0xe8edf3ff => 0x363b43ff,
            0x1a73e8ff => 0x8ab4f8ff,
            0x1765ccff => 0xa8c7faff,
            0xe8f0feff | 0xf5f8ffff => 0x263650ff,
            0xf0f4faff => 0x2c3038ff,
            0x147d4eff => 0x81c995ff,
            0xe6f4eaff => 0x203c2dff,
            0xb42318ff => 0xf28b82ff,
            0xffe0deff | 0xffeeedff => 0x462b2bff,
            0xeff2f6ff => 0x252930ff,
            0x9aa5b5ff | 0x9aa0a6ff => 0x858c99ff,
            _ => value,
        }
    }
}

/// Window-local semantic colors, encoded as `0xRRGGBBAA`.
///
/// Canvas primitives and immediate widgets resolve the built-in color tokens
/// through this palette. Other custom colors are kept unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub background: u32,
    pub surface: u32,
    pub text: u32,
    pub muted: u32,
    pub line: u32,
    pub accent: u32,
    pub accent_hover: u32,
    pub accent_soft: u32,
    pub hover: u32,
    pub success: u32,
    pub success_soft: u32,
    pub danger: u32,
    pub danger_soft: u32,
    pub disabled_surface: u32,
    pub disabled_text: u32,
    pub on_accent: u32,
}
impl Palette {
    /// Start from the semantic roles of an existing theme, then change only
    /// the roles your application needs. Unset palettes retain the original
    /// theme behavior, including its legacy color aliases.
    pub fn for_theme(theme: Theme) -> Self {
        Self {
            background: theme.color(colors::BG),
            surface: theme.color(colors::SURFACE),
            text: theme.color(colors::TEXT),
            muted: theme.color(colors::MUTED),
            line: theme.color(colors::LINE),
            accent: theme.color(colors::BLUE),
            accent_hover: theme.color(0x1765ccff),
            accent_soft: theme.color(colors::BLUE_PALE),
            hover: theme.color(0xf0f4faff),
            success: theme.color(colors::GREEN),
            success_soft: theme.color(0xe6f4eaff),
            danger: theme.color(colors::RED),
            danger_soft: theme.color(0xffeeedff),
            disabled_surface: theme.color(0xeff2f6ff),
            disabled_text: theme.color(0x9aa5b5ff),
            on_accent: theme.color(colors::SURFACE),
        }
    }
    /// Resolve one canonical color token. The token's alpha multiplies the
    /// selected role's alpha; unrelated RGB values pass through unchanged.
    pub fn color(self, value: u32) -> u32 {
        let replacement = match value & 0xffffff00 {
            0xf7f9fc00 | 0xf1f3f400 => self.background,
            0xffffff00 => self.surface,
            0x20212400 => self.text,
            0x5f636800 => self.muted,
            0xe8eaed00 | 0xe8edf300 => self.line,
            0x1a73e800 => self.accent,
            0x1765cc00 => self.accent_hover,
            0xe8f0fe00 | 0xf5f8ff00 => self.accent_soft,
            0xf0f4fa00 => self.hover,
            0x147d4e00 => self.success,
            0xe6f4ea00 => self.success_soft,
            0xb4231800 => self.danger,
            0xffe0de00 | 0xffeeed00 => self.danger_soft,
            0xeff2f600 | 0xf2f4f700 => self.disabled_surface,
            0x9aa5b500 | 0x9aa0a600 => self.disabled_text,
            _ => return value,
        };
        (replacement & 0xffffff00) | (((replacement & 255) * (value & 255) + 127) / 255)
    }
}

fn resolve_color(theme: Theme, palette: Option<Palette>, value: u32) -> u32 {
    palette.map_or_else(|| theme.color(value), |palette| palette.color(value))
}

fn requested_font_family(value: &str) -> Result<String, String> {
    if value.len() > 255 || value.chars().any(char::is_control) {
        return Err(
            "font family must be at most 255 UTF-8 bytes without control characters".into(),
        );
    }
    Ok(value.trim().to_owned())
}

fn image_request(rect: Rect, width: u32, height: u32, byte_len: usize) -> Result<(), String> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or("RGBA8 image byte length overflows addressable memory")?;
    if width == 0
        || height == 0
        || width > 16_384
        || height > 16_384
        || expected > 256 * 1024 * 1024
    {
        return Err(
            "RGBA8 images require nonzero dimensions at most 16384 and at most 256 MiB".into(),
        );
    }
    if byte_len != expected {
        return Err("RGBA8 image length must equal width * height * 4 exactly".into());
    }
    let right = rect.x + rect.w;
    let bottom = rect.y + rect.h;
    if [rect.x, rect.y, rect.w, rect.h, right, bottom]
        .iter()
        .any(|value| !value.is_finite())
        || rect.w <= 0.0
        || rect.h <= 0.0
        || right <= rect.x
        || bottom <= rect.y
    {
        return Err(
            "Image rectangles require finite coordinates and positive finite extents".into(),
        );
    }
    Ok(())
}

pub struct Window {
    theme: Theme,
    palette: Option<Palette>,
    font_family: String,
    raw: *mut c_void,
    effect_owner: u64,
    effect_shaders: [Option<ShaderHandle>; MAX_SHADER_SLOTS],
    next_shader_generation: u64,
    _thread: PhantomData<Rc<()>>,
}
fn string(value: &str) -> CString {
    CString::new(value.replace('\0', "")).expect("interior NUL removed")
}
impl Window {
    /// Creates a caller-sized native window. Linux and macOS use this content
    /// size as the initial minimum. Linux derives WM_CLASS from the leading
    /// ASCII identifier in the title (letters, digits, dots, underscores, dashes).
    /// For example, "ztype - document" has instance "ztype" and class "Ztype".
    pub fn new(title: &str, width: i32, height: i32) -> Result<Self, String> {
        if WINDOW_OPEN
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("zgpui supports one live window per process".into());
        }
        let effect_owner =
            match EFFECT_OWNER.try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            }) {
                Ok(owner) => owner,
                Err(_) => {
                    WINDOW_OPEN.store(false, Ordering::Release);
                    return Err("Native window identity space exhausted".into());
                }
            };
        let title = string(title);
        let mut error = [0 as c_char; 1024];
        let raw = unsafe {
            zgpui_open(
                title.as_ptr(),
                width,
                height,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        if raw.is_null() {
            WINDOW_OPEN.store(false, Ordering::Release);
            return Err(unsafe { CStr::from_ptr(error.as_ptr()) }
                .to_string_lossy()
                .into_owned());
        }
        Ok(Self {
            theme: Theme::Light,
            palette: None,
            font_family: String::new(),
            raw,
            effect_owner,
            effect_shaders: [None; MAX_SHADER_SLOTS],
            next_shader_generation: 1,
            _thread: PhantomData,
        })
    }
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }
    /// Override semantic colors for this window until `clear_palette`.
    pub fn set_palette(&mut self, palette: Palette) {
        self.palette = Some(palette);
    }
    /// Restore this window's selected built-in theme.
    pub fn clear_palette(&mut self) {
        self.palette = None;
    }
    /// Configure opt-in materials. This never schedules a redraw or animation.
    /// Existing drawing primitives do not allocate material textures or programs.
    pub fn set_effects(&mut self, quality: Quality, budget: EffectBudget) -> Result<(), String> {
        let budget = budget.validate().map_err(|error| error.to_string())?;
        if self
            .effect_shaders
            .iter()
            .flatten()
            .any(|shader| shader.native > budget.max_shader_slots)
        {
            return Err("Remove registered shaders before reducing their slot budget".into());
        }
        let config = NativeEffectConfig {
            abi_version: EFFECT_ABI,
            quality: quality as u32,
            max_entries: budget.max_entries,
            max_shader_slots: budget.max_shader_slots,
            max_texture_bytes: budget.cache_bytes,
        };
        effect_status(unsafe { zgpui_effect_configure(self.raw, &config) })
    }

    /// Query actual current-context support. Software fallback remains explicit.
    pub fn effect_capabilities(&self) -> Result<EffectCapabilities, String> {
        let mut raw = NativeEffectCapabilities::default();
        effect_status(unsafe { zgpui_effect_capabilities(self.raw, &mut raw) })?;
        if raw.abi_version != EFFECT_ABI {
            return Err("Native material capability ABI differs from the Rust interface".into());
        }
        Ok(EffectCapabilities {
            gpu: raw.flags & 1 != 0,
            software: raw.flags & 32 != 0,
            backdrop_capture: raw.flags & 2 != 0,
            blur: raw.flags & 4 != 0,
            progressive_blur: raw.flags & 5 == 5,
            projective_fold: raw.flags & 16 != 0,
            custom_shaders: raw.flags & 8 != 0,
            max_texture_size: raw.max_texture_size,
            max_cache_entries: raw.max_entries,
            max_shader_slots: raw.max_shader_slots,
            max_shader_source_bytes: raw.max_shader_bytes,
            max_cache_bytes: raw.max_texture_bytes,
        })
    }

    /// Read cumulative work counters without waiting for GPU completion.
    pub fn effect_stats(&self) -> Result<EffectStats, String> {
        read_effect_stats(self.raw)
    }

    /// Discard all cached revisions of an ID, retaining registered shaders.
    pub fn invalidate_effect(&mut self, id: u64) -> Result<(), String> {
        CacheKey::new(id, 0)
            .validate()
            .map_err(|error| error.to_string())?;
        effect_status(unsafe { zgpui_effect_invalidate(self.raw, id) })
    }

    /// Discard capture and blur textures, retaining registered shaders and counters.
    pub fn clear_effects(&mut self) -> Result<(), String> {
        effect_status(unsafe { zgpui_effect_clear(self.raw) })
    }

    /// Register a bounded GLSL 1.20 `vec4 effect(vec2, vec2, vec4)` function.
    /// The renderer supplies `main` and `uniform float u_params[8]`. Compilation
    /// is explicit and synchronous; source is trusted application shader code.
    /// There is no shader execution-time sandbox or automatic CPU translation.
    pub fn register_shader(&mut self, source: &str) -> Result<ShaderHandle, String> {
        shader_source(source)?;
        let next = self
            .next_shader_generation
            .checked_add(1)
            .ok_or("Custom shader registration generation exhausted")?;
        let index = self
            .effect_shaders
            .iter()
            .position(Option::is_none)
            .ok_or("Custom shader registration slots are full")?;
        let capabilities = self.effect_capabilities()?;
        if !capabilities.custom_shaders {
            return Err("This renderer does not support custom material shaders".into());
        }
        let mut slot = 0;
        let mut diagnostic = [0u8; 4096];
        let status = unsafe {
            zgpui_effect_shader_register(
                self.raw,
                source.as_ptr().cast(),
                source.len(),
                &mut slot,
                diagnostic.as_mut_ptr().cast(),
                diagnostic.len(),
            )
        };
        if status != 0 {
            let size = diagnostic
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(diagnostic.len());
            let detail = String::from_utf8_lossy(&diagnostic[..size]);
            let message = effect_status(status).expect_err("nonzero native status");
            return Err(if detail.is_empty() {
                message
            } else {
                format!("{message}: {detail}")
            });
        }
        if slot == 0
            || slot > MAX_SHADER_SLOTS as u32
            || self
                .effect_shaders
                .iter()
                .flatten()
                .any(|shader| shader.native == slot)
        {
            return Err("Native renderer returned an invalid custom shader slot".into());
        }
        let handle = ShaderHandle {
            owner: self.effect_owner,
            generation: self.next_shader_generation,
            native: slot,
        };
        self.effect_shaders[index] = Some(handle);
        self.next_shader_generation = next;
        Ok(handle)
    }

    /// Remove a shader. Copies of its handle become invalid immediately.
    pub fn unregister_shader(&mut self, handle: ShaderHandle) -> Result<(), String> {
        let slot = registered_shader(self.effect_owner, &self.effect_shaders, handle)?;
        effect_status(unsafe { zgpui_effect_shader_remove(self.raw, slot) })?;
        for registered in &mut self.effect_shaders {
            if *registered == Some(handle) {
                *registered = None;
                break;
            }
        }
        Ok(())
    }

    /// Wait for submitted GPU work, for explicit benchmarks or synchronization.
    /// Normal material drawing and presentation never call this method.
    pub fn finish_effects(&mut self) -> Result<(), String> {
        effect_status(unsafe { zgpui_effect_finish(self.raw) })
    }
    /// Prefer a font family for this window. `monospace` selects the platform's
    /// fixed-width family; an empty string restores the original default.
    /// A missing named family falls back to monospace, with native glyph fallback.
    ///
    /// A successful change invalidates only text caches. Invalid names or native
    /// preparation failures preserve the old family and cached text. Redraw the
    /// window after changing its font. Repeating the same request is a no-op.
    pub fn set_font_family(&mut self, family: &str) -> Result<(), String> {
        let family = requested_font_family(family)?;
        if family == self.font_family {
            return Ok(());
        }
        let name = CString::new(family.as_str()).expect("validated font family");
        let mut error = [0 as c_char; 1024];
        if unsafe {
            zgpui_set_font_family(self.raw, name.as_ptr(), error.as_mut_ptr(), error.len())
        } == 0
        {
            let error = unsafe { CStr::from_ptr(error.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            return Err(if error.is_empty() {
                "native font preparation failed".into()
            } else {
                error
            });
        }
        self.font_family = family;
        Ok(())
    }
    pub fn size(&self) -> (f32, f32) {
        let (mut width, mut height) = (0, 0);
        unsafe { zgpui_size(self.raw, &mut width, &mut height) };
        (width as f32, height as f32)
    }
    pub fn event(&mut self, timeout_ms: i32) -> Option<Event> {
        let mut raw = NativeEvent {
            kind: 0,
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            key: 0,
            modifiers: 0,
            text: [0; 4096],
        };
        if unsafe { zgpui_event(self.raw, &mut raw, timeout_ms.clamp(0, 1000)) } == 0 {
            return None;
        }
        let text = unsafe { CStr::from_ptr(raw.text.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        Some(match raw.kind {
            1 => Event::Move(raw.x as f32, raw.y as f32),
            2 => Event::Down(raw.x as f32, raw.y as f32),
            3 => Event::Up(raw.x as f32, raw.y as f32),
            4 => Event::Key {
                code: raw.key,
                control: raw.modifiers & 1 != 0,
                shift: raw.modifiers & 2 != 0,
                text,
            },
            5 => Event::Text(text),
            6 => Event::Resize,
            7 => Event::Close,
            8 => Event::Scroll(raw.key),
            _ => return None,
        })
    }
    pub fn begin(&mut self, background: u32) -> Canvas<'_> {
        let (width, height) = self.size();
        unsafe {
            zgpui_begin(
                width as i32,
                height as i32,
                resolve_color(self.theme, self.palette, background),
            )
        };
        Canvas {
            theme: self.theme,
            palette: self.palette,
            width,
            height,
            raw: self.raw,
            effect_owner: self.effect_owner,
            effect_shaders: &self.effect_shaders,
            _window: PhantomData,
        }
    }
    pub fn present(&mut self) {
        unsafe { zgpui_swap(self.raw) }
    }
    pub fn copy(&mut self, text: &str) {
        let text = string(text);
        unsafe { zgpui_clipboard(self.raw, text.as_ptr()) }
    }
    pub fn gpu(&self) -> String {
        unsafe { CStr::from_ptr(zgpui_gpu_info()) }
            .to_string_lossy()
            .into_owned()
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        unsafe { zgpui_close(self.raw) };
        WINDOW_OPEN.store(false, Ordering::Release);
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
    pub fn inset(self, amount: f32) -> Self {
        Self::new(
            self.x + amount,
            self.y + amount,
            (self.w - amount * 2.).max(0.),
            (self.h - amount * 2.).max(0.),
        )
    }
}
pub struct Canvas<'a> {
    pub theme: Theme,
    palette: Option<Palette>,
    pub width: f32,
    pub height: f32,
    raw: *mut c_void,
    effect_owner: u64,
    effect_shaders: &'a [Option<ShaderHandle>; MAX_SHADER_SLOTS],
    _window: PhantomData<&'a mut Window>,
}
impl Canvas<'_> {
    fn color(&self, color: u32) -> u32 {
        resolve_color(self.theme, self.palette, color)
    }
    /// Read this frame's submitted work counters without a GPU fence or redraw.
    pub fn effect_stats(&self) -> Result<EffectStats, String> {
        read_effect_stats(self.raw)
    }
    /// Capture and render an application-owned backdrop at the same rectangle.
    /// An unchanged key and source rectangle reuse the previous capture. The
    /// backdrop UV remains in screen space while the material plane may fold.
    pub fn material(
        &mut self,
        rect: Rect,
        key: CacheKey,
        material: &Material,
        fold: &Fold,
    ) -> Result<EffectOutcome, String> {
        self.material_at(rect, rect, key, material, fold)
    }

    /// Render a stable captured source at independently changing target geometry.
    /// Only source changes, explicit invalidation or content revisions recapture.
    pub fn material_at(
        &mut self,
        source: Rect,
        target: Rect,
        key: CacheKey,
        material: &Material,
        fold: &Fold,
    ) -> Result<EffectOutcome, String> {
        let source = capture_request(source, key)?;
        self.draw_material(source, target, material, fold, 0)
    }

    /// Cache a complete already drawn region without presenting it. Draw content,
    /// capture it, then begin a fresh frame before calling `cached_material`.
    /// Resize and eviction may remove it; a cache-only draw reports that absence.
    pub fn capture_effect(&mut self, source: Rect, key: CacheKey) -> Result<(), String> {
        let source = capture_request(source, key)?;
        effect_status(unsafe { zgpui_effect_capture(self.raw, &source) })
    }

    /// Transform previously captured complete content, including its text.
    /// Sampling follows the folded plane; this call never silently recaptures.
    pub fn cached_material(
        &mut self,
        target: Rect,
        key: CacheKey,
        material: &Material,
        fold: &Fold,
    ) -> Result<EffectOutcome, String> {
        key.validate().map_err(|error| error.to_string())?;
        let source = NativeEffectCapture {
            id: key.id,
            revision: key.revision,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
        self.draw_material(
            source,
            target,
            material,
            fold,
            EFFECT_LAYER | EFFECT_CACHE_ONLY,
        )
    }

    fn draw_material(
        &mut self,
        source: NativeEffectCapture,
        target: Rect,
        material: &Material,
        fold: &Fold,
        flags: u32,
    ) -> Result<EffectOutcome, String> {
        let target = effect_rectangle(target)?;
        let material = material.validate().map_err(|error| error.to_string())?;
        let fold = fold.validate().map_err(|error| error.to_string())?;
        let shader_slot = match material.shader {
            Some(handle) => registered_shader(self.effect_owner, self.effect_shaders, handle)?,
            None => 0,
        };
        let effect = NativeEffect {
            source,
            x: target.x,
            y: target.y,
            width: target.w,
            height: target.h,
            radius: material.corner_radius,
            blur_radius: material.blur_radius,
            saturation: material.saturation,
            opacity: material.opacity,
            tint_rgba: material.tint,
            preset: match material.preset {
                MaterialPreset::Solid | MaterialPreset::Blur | MaterialPreset::Frosted => 0,
                MaterialPreset::Mica => 1,
                MaterialPreset::LiquidGlass => 2,
                MaterialPreset::Paper => 3,
                MaterialPreset::Custom => 4,
            },
            tint_mix: material.tint_mix,
            noise: material.noise,
            refraction: material.refraction,
            border: material.border_width,
            highlight: material.highlight,
            progressive_blur: material.progressive_blur,
            fold_progress: fold.progress,
            fold_radians: fold.angle_degrees.to_radians(),
            fold_perspective: fold.perspective,
            dim: fold.dimming,
            fold_anchor: fold.anchor,
            fold_axis: match fold.axis {
                FoldAxis::Vertical => 1,
                FoldAxis::Horizontal => 2,
            },
            flags,
            shader_slot,
            shader_params: material.parameters,
        };
        match unsafe { zgpui_draw_effect(self.raw, &effect) } {
            0 => Ok(EffectOutcome::Rendered),
            1 => Ok(EffectOutcome::Fallback),
            status => effect_status(status).map(|()| EffectOutcome::Rendered),
        }
    }
    pub fn rect(&mut self, rect: Rect, radius: f32, color: u32) {
        unsafe { zgpui_rect(rect.x, rect.y, rect.w, rect.h, radius, self.color(color)) }
    }
    /// Draw tightly packed, top-down RGBA8 pixels with straight (not premultiplied) alpha.
    ///
    /// The image is stretched to `rect` using nearest-neighbor samples at pixel centers,
    /// clipped to the current clip and window, and alpha-composited over the frame.
    /// Pixel colors bypass the theme and palette. This call does not retain `rgba`.
    ///
    /// Source dimensions must be nonzero, at most 16384 per side, and at most 256 MiB
    /// in total. The slice length must equal `width * height * 4` exactly. Destination
    /// coordinates and extents must be finite, with positive width and height.
    /// Invalid requests draw nothing. A backend failure can leave a partially drawn
    /// image; redraw the complete frame after an error.
    ///
    /// OpenGL reuses one power-of-two upload texture of at most 4 MiB of RGBA texels,
    /// released when the window closes. The software backend uses the existing frame.
    pub fn image(
        &mut self,
        rect: Rect,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<(), String> {
        image_request(rect, width, height, rgba.len())?;
        match unsafe {
            zgpui_image(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                width,
                height,
                rgba.as_ptr(),
                rgba.len(),
            )
        } {
            0 => Ok(()),
            -1 => Err("Native renderer rejected the RGBA8 image request".into()),
            -2 => Err("Native image drawing requires a prepared frame and renderer".into()),
            _ => Err("Native RGBA8 texture upload or drawing failed".into()),
        }
    }
    pub fn text(&mut self, text: &str, x: f32, y: f32, size: i32, bold: bool, color: u32) -> f32 {
        self.text_resolved(text, x, y, size, bold, self.color(color))
    }
    fn text_resolved(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        size: i32,
        bold: bool,
        color: u32,
    ) -> f32 {
        let text = string(text);
        unsafe {
            zgpui_text(
                text.as_ptr(),
                x.round(),
                y.round(),
                size,
                bold as i32,
                color,
            )
        }
    }
    /// Draw a convex polygon with at least three points.
    pub fn polygon(&mut self, points: &[(f32, f32)], color: u32) {
        if points.len() < 3 || points.len() > 4096 {
            return;
        }
        let mut coordinates = Vec::with_capacity(points.len() * 2);
        for &(x, y) in points {
            coordinates.extend([x, y]);
        }
        unsafe { zgpui_polygon(coordinates.as_ptr(), points.len(), self.color(color)) }
    }
    pub fn line(&mut self, from: (f32, f32), to: (f32, f32), width: f32, color: u32) {
        unsafe { zgpui_line(from.0, from.1, to.0, to.1, width.max(0.), self.color(color)) }
    }
    pub fn measure(&self, text: &str, size: i32, bold: bool) -> f32 {
        let text = string(text);
        unsafe { zgpui_measure(text.as_ptr(), size, bold as i32) }
    }
    pub fn clip(&mut self, rect: Rect) {
        unsafe { zgpui_clip(rect.x as i32, rect.y as i32, rect.w as i32, rect.h as i32) }
    }
    pub fn unclip(&mut self) {
        unsafe { zgpui_unclip() }
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "drawing primitives carry geometry and style"
    )]
    pub fn ellipsis(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        max_width: f32,
        size: i32,
        bold: bool,
        color: u32,
    ) {
        let fitted = fit_ellipsis(text, max_width, |value| self.measure(value, size, bold));
        self.text(&fitted, x, y, size, bold, color);
    }
}
/// Find a measured, UTF-8-safe prefix with logarithmically many width probes.
/// Shaping can make prefix widths nonmonotonic; the result is always a prefix
/// whose complete form including the ellipsis was measured to fit.
fn fit_ellipsis(
    text: &str,
    max_width: f32,
    mut measure: impl FnMut(&str) -> f32,
) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    if text.is_empty() || max_width.is_nan() || max_width <= 0. {
        return Cow::Borrowed("");
    }
    if measure(text) <= max_width {
        return Cow::Borrowed(text);
    }
    let ellipsis_width = measure("…");
    if ellipsis_width.is_nan() || ellipsis_width > max_width {
        return Cow::Borrowed("");
    }
    let (mut low, mut high) = (0, text.len());
    let mut candidate = String::with_capacity(text.len().min(256) + 3);
    while low < high {
        let mut end = low + (high - low).div_ceil(2);
        while !text.is_char_boundary(end) {
            end += 1;
        }
        candidate.clear();
        candidate.push_str(&text[..end]);
        candidate.push('…');
        if measure(&candidate) <= max_width {
            low = end;
        } else {
            high = text[..end].char_indices().next_back().map_or(0, |(i, _)| i);
        }
    }
    candidate.clear();
    candidate.push_str(&text[..low]);
    candidate.push('…');
    Cow::Owned(candidate)
}

#[cfg(test)]
mod ellipsis_tests {
    use super::fit_ellipsis;
    use std::borrow::Cow;

    #[test]
    fn fits_without_allocating_and_respects_tiny_widths() {
        let width = |value: &str| value.chars().count() as f32;
        assert!(matches!(
            fit_ellipsis("hello", 5., width),
            Cow::Borrowed("hello")
        ));
        assert_eq!(fit_ellipsis("hello", 0.5, width), "");
        assert_eq!(fit_ellipsis("hello", 1., width), "…");
        assert_eq!(fit_ellipsis("hello", -1., width), "");
        assert_eq!(fit_ellipsis("hello", f32::NAN, width), "");
    }

    #[test]
    fn bounds_long_text_probes_and_keeps_unicode_valid() {
        for text in ["a".repeat(100_000), "世界🚀e\u{301}".repeat(2_000)] {
            let mut calls = 0;
            let fitted = fit_ellipsis(&text, 18., |s| {
                calls += 1;
                s.chars().count() as f32
            });
            assert!(fitted.ends_with('…'));
            assert!(fitted.chars().count() <= 18);
            assert!(text.starts_with(fitted.trim_end_matches('…')));
            assert!(calls <= usize::BITS + 2, "{calls} width probes");
        }
    }

    #[test]
    fn returns_a_measured_fit_when_shaping_widths_are_not_monotonic() {
        let width = |value: &str| match value {
            "abcdefgh" => 10.,
            "abcdef…" => 2.,
            _ => value.chars().count() as f32,
        };
        let fitted = fit_ellipsis("abcdefgh", 3., width);
        assert!(width(&fitted) <= 3.);
        assert!(fitted.ends_with('…'));
    }
}

impl Drop for Canvas<'_> {
    fn drop(&mut self) {
        unsafe { zgpui_unclip() }
    }
}
pub mod colors {
    pub const BG: u32 = 0xf7f9fcff;
    pub const SURFACE: u32 = 0xffffffff;
    pub const TEXT: u32 = 0x202124ff;
    pub const MUTED: u32 = 0x5f6368ff;
    pub const LINE: u32 = 0xe8eaedff;
    pub const BLUE: u32 = 0x1a73e8ff;
    pub const BLUE_PALE: u32 = 0xe8f0feff;
    pub const GREEN: u32 = 0x147d4eff;
    pub const RED: u32 = 0xb42318ff;
}
#[derive(Clone, Copy)]
pub enum ButtonStyle {
    Primary,
    Secondary,
    Quiet,
    Danger,
}
#[derive(Default)]
pub struct Ui {
    mouse: (f32, f32),
    down: bool,
    released: bool,
    pressed: bool,
    event: Option<Event>,
    active: Option<u64>,
    focus: Option<u64>,
    order: Vec<u64>,
    previous: Vec<u64>,
    boxes: Vec<(u64, Rect)>,
    previous_boxes: Vec<(u64, Rect)>,
    cursor: usize,
    select_all: bool,
    pub clipboard: Option<String>,
}
impl Ui {
    /// Update pointer position and redraw only when its widget target changes.
    pub fn pointer_changed(&mut self, x: f32, y: f32) -> bool {
        let hit = |x, y| {
            self.previous_boxes
                .iter()
                .rev()
                .find(|(_, r)| r.contains(x, y))
                .map(|(id, _)| *id)
        };
        let changed = hit(self.mouse.0, self.mouse.1) != hit(x, y);
        self.mouse = (x, y);
        changed
    }
    pub fn begin(&mut self, event: Option<Event>) {
        self.released = false;
        self.pressed = false;
        match &event {
            Some(Event::Move(x, y)) => self.mouse = (*x, *y),
            Some(Event::Down(x, y)) => {
                self.mouse = (*x, *y);
                self.down = true;
                self.pressed = true;
            }
            Some(Event::Up(x, y)) => {
                self.mouse = (*x, *y);
                self.down = false;
                self.released = true;
            }
            Some(Event::Key { code: 9, shift, .. }) if !self.previous.is_empty() => {
                let pos = self
                    .focus
                    .and_then(|id| self.previous.iter().position(|v| *v == id));
                let next = if *shift {
                    (pos.unwrap_or(0) + self.previous.len() - 1) % self.previous.len()
                } else {
                    pos.map_or(0, |i| (i + 1) % self.previous.len())
                };
                self.focus = Some(self.previous[next]);
                self.cursor = usize::MAX;
                self.select_all = false;
            }
            _ => {}
        }
        self.event = event;
        self.order.clear();
        self.boxes.clear();
    }
    pub fn end(&mut self) {
        if self.released {
            self.active = None;
        }
        std::mem::swap(&mut self.order, &mut self.previous);
        std::mem::swap(&mut self.boxes, &mut self.previous_boxes);
        self.event = None;
    }
    fn register(&mut self, id: u64, rect: Rect) -> bool {
        self.order.push(id);
        self.boxes.push((id, rect));
        let hover = rect.contains(self.mouse.0, self.mouse.1);
        if hover && self.pressed {
            self.active = Some(id);
            if self.focus != Some(id) {
                self.cursor = usize::MAX;
                self.select_all = false;
            }
            self.focus = Some(id);
        }
        hover
    }
    pub fn button(
        &mut self,
        canvas: &mut Canvas<'_>,
        id: u64,
        rect: Rect,
        label: &str,
        style: ButtonStyle,
        enabled: bool,
    ) -> bool {
        use colors::*;
        let hover = if enabled {
            self.register(id, rect)
        } else {
            false
        };
        let focused = self.focus == Some(id) && enabled;
        let clicked = enabled
            && ((hover && self.released && self.active == Some(id))
                || (focused && matches!(self.event, Some(Event::Key { code: 13 | 32, .. }))));
        let (fill, ink) = if !enabled {
            (0xeff2f6ff, 0x9aa5b5ff)
        } else {
            match style {
                ButtonStyle::Primary => (if hover { 0x1765ccff } else { BLUE }, SURFACE),
                ButtonStyle::Danger => (if hover { 0xffe0deff } else { 0xffeeedff }, RED),
                ButtonStyle::Quiet => (
                    if hover || focused {
                        BLUE_PALE
                    } else {
                        0xffffff00
                    },
                    if focused { BLUE } else { TEXT },
                ),
                ButtonStyle::Secondary => (if hover { 0xf0f4faff } else { SURFACE }, TEXT),
            }
        };
        if matches!(style, ButtonStyle::Secondary) || focused {
            canvas.rect(rect, 12., if focused { BLUE } else { LINE });
            canvas.rect(rect.inset(if focused { 2. } else { 1. }), 11., fill);
        } else {
            canvas.rect(rect, 12., fill);
        }
        let width = canvas.measure(label, 15, false);
        let ink = if enabled && matches!(style, ButtonStyle::Primary) {
            canvas
                .palette
                .map_or_else(|| canvas.color(ink), |palette| palette.on_accent)
        } else {
            canvas.color(ink)
        };
        canvas.text_resolved(
            label,
            rect.x + (rect.w - width) / 2.,
            rect.y + (rect.h - 19.) / 2.,
            15,
            false,
            ink,
        );
        clicked
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "widget placement and state are explicit"
    )]
    pub fn input(
        &mut self,
        canvas: &mut Canvas<'_>,
        id: u64,
        rect: Rect,
        value: &mut String,
        placeholder: &str,
        secret: bool,
        enabled: bool,
    ) -> bool {
        use colors::*;
        if enabled {
            self.register(id, rect);
        }
        let focused = self.focus == Some(id) && enabled;
        let mut changed = false;
        if focused {
            self.cursor = self.cursor.min(value.len());
            while !value.is_char_boundary(self.cursor) {
                self.cursor -= 1;
            }
            if self.pressed && rect.contains(self.mouse.0, self.mouse.1) {
                self.cursor = value.len();
                self.select_all = false;
            }
            match self.event.clone() {
                Some(Event::Key {
                    code,
                    control,
                    shift: _,
                    text,
                }) => {
                    if control {
                        match code {
                            97 | 65 => self.select_all = true,
                            99 | 67 => {
                                if !secret {
                                    self.clipboard = Some(value.clone());
                                }
                            }
                            120 | 88 => {
                                if !secret {
                                    self.clipboard = Some(value.clone());
                                }
                                value.clear();
                                self.cursor = 0;
                                self.select_all = false;
                                changed = true;
                            }
                            _ => {}
                        }
                    } else {
                        match code {
                            8 => {
                                if self.select_all {
                                    value.clear();
                                    self.cursor = 0;
                                } else if self.cursor > 0 {
                                    let previous = value[..self.cursor]
                                        .char_indices()
                                        .last()
                                        .map_or(0, |(i, _)| i);
                                    value.replace_range(previous..self.cursor, "");
                                    self.cursor = previous;
                                }
                                self.select_all = false;
                                changed = true;
                            }
                            127 => {
                                if self.select_all {
                                    value.clear();
                                    self.cursor = 0;
                                } else if self.cursor < value.len() {
                                    let end = self.cursor
                                        + value[self.cursor..].chars().next().unwrap().len_utf8();
                                    value.replace_range(self.cursor..end, "");
                                }
                                self.select_all = false;
                                changed = true;
                            }
                            1001 => {
                                self.cursor = value[..self.cursor]
                                    .char_indices()
                                    .last()
                                    .map_or(0, |(i, _)| i);
                                self.select_all = false;
                            }
                            1002 => {
                                if let Some(c) = value[self.cursor..].chars().next() {
                                    self.cursor += c.len_utf8();
                                }
                                self.select_all = false;
                            }
                            1005 => {
                                self.cursor = 0;
                                self.select_all = false;
                            }
                            1006 => {
                                self.cursor = value.len();
                                self.select_all = false;
                            }
                            9 | 13 | 27 | 1003 | 1004 => {}
                            _ => changed |= self.insert(value, &text),
                        }
                    }
                }
                Some(Event::Text(text)) => changed |= self.insert(value, &text),
                _ => {}
            }
        }
        canvas.rect(rect, 12., if focused { BLUE } else { LINE });
        canvas.rect(
            rect.inset(if focused { 2. } else { 1. }),
            7.,
            if enabled { SURFACE } else { 0xf2f4f7ff },
        );
        let display = if secret {
            "•".repeat(value.chars().count())
        } else {
            value.clone()
        };
        let prefix = if !focused {
            String::new()
        } else if secret {
            "•".repeat(value[..self.cursor.min(value.len())].chars().count())
        } else {
            value
                .get(..self.cursor.min(value.len()))
                .unwrap_or("")
                .into()
        };
        let cursor_width = canvas.measure(&prefix, 15, false);
        let scroll = if focused {
            (cursor_width - rect.w + 30.).max(0.)
        } else {
            0.
        };
        canvas.clip(rect.inset(7.));
        if focused && self.select_all {
            canvas.rect(
                Rect::new(
                    rect.x + 10. - scroll,
                    rect.y + 8.,
                    canvas.measure(&display, 15, false),
                    22.,
                ),
                2.,
                BLUE_PALE,
            );
        }
        canvas.text(
            if value.is_empty() {
                placeholder
            } else {
                &display
            },
            rect.x + 11. - scroll,
            rect.y + 11.,
            15,
            false,
            if value.is_empty() { MUTED } else { TEXT },
        );
        if focused {
            canvas.rect(
                Rect::new(rect.x + 11. + cursor_width - scroll, rect.y + 9., 1.4, 22.),
                0.,
                BLUE,
            );
        }
        canvas.unclip();
        changed
    }
    fn insert(&mut self, value: &mut String, text: &str) -> bool {
        let filtered: String = text.chars().filter(|c| !c.is_control()).collect();
        if filtered.is_empty() || value.len() + filtered.len() > 4096 {
            return false;
        }
        if self.select_all {
            value.clear();
            self.cursor = 0;
            self.select_all = false;
        }
        self.cursor = self.cursor.min(value.len());
        value.insert_str(self.cursor, &filtered);
        self.cursor += filtered.len();
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_request_checks_exact_packed_length_and_source_bounds() {
        let rect = Rect::new(0., 0., 10., 10.);
        assert!(image_request(rect, 1, 1, 4).is_ok());
        assert!(image_request(rect, 3, 2, 24).is_ok());
        assert!(image_request(rect, 16_384, 4096, 256 * 1024 * 1024).is_ok());
        for (width, height, length) in [
            (0, 1, 0),
            (1, 0, 0),
            (1, 1, 0),
            (3, 2, 23),
            (3, 2, 25),
            (16_385, 1, 16_385 * 4),
            (16_384, 4097, 16_384 * 4097 * 4),
            (u32::MAX, u32::MAX, usize::MAX),
        ] {
            assert!(image_request(rect, width, height, length).is_err());
        }
    }
    #[test]
    fn image_request_rejects_nonfinite_and_degenerate_destinations() {
        for rect in [
            Rect::new(f32::NAN, 0., 1., 1.),
            Rect::new(0., f32::INFINITY, 1., 1.),
            Rect::new(0., 0., -1., 1.),
            Rect::new(0., 0., 1., 0.),
            Rect::new(0., 0., 1., f32::NAN),
            Rect::new(f32::MAX, 0., f32::MAX, 1.),
            Rect::new(f32::MAX, 0., 1., 1.),
        ] {
            assert!(image_request(rect, 1, 1, 4).is_err());
        }
        assert!(image_request(Rect::new(-10.25, -20.5, 0.5, 1.25), 1, 1, 4).is_ok());
    }
    #[test]
    fn unicode_input_preserves_boundaries() {
        let mut ui = Ui::default();
        let mut value = "模型".to_string();
        ui.cursor = value.len();
        assert!(ui.insert(&mut value, " a\nb"));
        assert_eq!(value, "模型 ab");
        ui.select_all = true;
        assert!(ui.insert(&mut value, "新"));
        assert_eq!(value, "新");
    }
    #[test]
    fn bounds_are_exclusive() {
        let r = Rect::new(10., 10., 20., 20.);
        assert!(r.contains(10., 10.));
        assert!(!r.contains(30., 20.));
    }
}
