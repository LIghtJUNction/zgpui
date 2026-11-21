//! Allocation-free descriptions for opt-in native materials.
//!
//! These types describe rendering work; constructing or validating them performs
//! no allocation, timing, operating-system call, or GPU work. A host controls
//! redraws and increments a [`CacheKey`]'s revision when captured content changes.

use core::fmt;

pub const MAX_CACHE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_CACHE_ENTRIES: u32 = 16;
pub const MAX_SHADER_SLOTS: usize = 8;
pub const MAX_SHADER_SOURCE_BYTES: usize = 16 * 1024;

/// Shader-based quality choices. `Off` requests the reported flat fallback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum Quality {
    #[default]
    Off = 0,
    Low = 1,
    Balanced = 2,
    High = 3,
}

/// Hard limits for cached capture textures and their derived blur textures.
///
/// The renderer may evict old entries to remain within these limits. This is a
/// texture-cache budget, not a promise about driver memory or shader run time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectBudget {
    pub cache_bytes: u64,
    pub max_entries: u32,
    pub max_shader_slots: u32,
}

impl Default for EffectBudget {
    fn default() -> Self {
        Self {
            cache_bytes: 32 * 1024 * 1024,
            max_entries: 16,
            max_shader_slots: MAX_SHADER_SLOTS as u32,
        }
    }
}

impl EffectBudget {
    pub const fn new(cache_bytes: u64, max_entries: u32) -> Self {
        Self {
            cache_bytes,
            max_entries,
            max_shader_slots: MAX_SHADER_SLOTS as u32,
        }
    }

    pub fn validate(self) -> Result<Self, EffectError> {
        if self.cache_bytes == 0 || self.cache_bytes > MAX_CACHE_BYTES {
            return Err(EffectError::OutOfRange("cache_bytes"));
        }
        if self.max_entries == 0 || self.max_entries > MAX_CACHE_ENTRIES {
            return Err(EffectError::OutOfRange("max_entries"));
        }
        if self.max_shader_slots > MAX_SHADER_SLOTS as u32 {
            return Err(EffectError::OutOfRange("max_shader_slots"));
        }
        Ok(self)
    }
}

/// A stable caller-owned identity and content revision.
///
/// An unchanged key permits reuse of the captured source. Increase `revision`
/// when the content behind or inside the capture changes. Style, target position
/// and fold changes do not require a new content revision. Revisions are compared
/// for equality; this is a rendering cache, not a replay-protection protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub id: u64,
    pub revision: u64,
}

impl CacheKey {
    pub const fn new(id: u64, revision: u64) -> Self {
        Self { id, revision }
    }

    pub fn validate(self) -> Result<Self, EffectError> {
        if self.id == 0 {
            return Err(EffectError::OutOfRange("cache id"));
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum MaterialPreset {
    Solid = 0,
    Blur = 1,
    Frosted = 2,
    Mica = 3,
    LiquidGlass = 4,
    Paper = 5,
    Custom = 6,
}

/// An opaque registration belonging to one native window and generation.
///
/// A copied handle does not keep its shader or window alive. Native drawing
/// rejects handles after removal and handles from a different window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShaderHandle {
    pub(crate) owner: u64,
    pub(crate) generation: u64,
    pub(crate) native: u32,
}

/// A caller-customizable material. Colors use literal `0xRRGGBBAA` values.
///
/// Material colors do not pass through the native widget theme palette. Blur and
/// refraction refer to pixels in the captured source. A positive progressive
/// blur grows from top to bottom; a negative value reverses that gradient.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub preset: MaterialPreset,
    pub tint: u32,
    /// Multiplies the tint color's alpha; independent from surface opacity.
    pub tint_mix: f32,
    pub opacity: f32,
    pub corner_radius: f32,
    pub blur_radius: f32,
    pub saturation: f32,
    pub noise: f32,
    pub refraction: f32,
    pub highlight: f32,
    pub border_width: f32,
    pub progressive_blur: f32,
    pub shader: Option<ShaderHandle>,
    /// Eight explicit finite custom uniforms; no implicit clock is supplied.
    pub parameters: [f32; 8],
}

impl Default for Material {
    fn default() -> Self {
        Self::solid()
    }
}

impl Material {
    pub const fn solid() -> Self {
        Self {
            preset: MaterialPreset::Solid,
            tint: 0xf4f5f7ff,
            tint_mix: 1.0,
            opacity: 1.0,
            corner_radius: 20.0,
            blur_radius: 0.0,
            saturation: 1.0,
            noise: 0.0,
            refraction: 0.0,
            highlight: 0.0,
            border_width: 0.0,
            progressive_blur: 0.0,
            shader: None,
            parameters: [0.0; 8],
        }
    }

    pub const fn blur() -> Self {
        Self {
            preset: MaterialPreset::Blur,
            tint: 0xffffff00,
            blur_radius: 24.0,
            ..Self::solid()
        }
    }

    pub const fn frosted() -> Self {
        Self {
            preset: MaterialPreset::Frosted,
            tint: 0xeaf1ff70,
            blur_radius: 28.0,
            saturation: 1.15,
            noise: 0.025,
            highlight: 0.35,
            border_width: 1.0,
            ..Self::solid()
        }
    }

    /// An application-owned mica-style material, not an OS wallpaper surface.
    pub const fn mica() -> Self {
        Self {
            preset: MaterialPreset::Mica,
            tint: 0xe3e9f2d8,
            blur_radius: 36.0,
            saturation: 0.65,
            noise: 0.035,
            highlight: 0.15,
            border_width: 1.0,
            ..Self::solid()
        }
    }

    pub const fn liquid_glass() -> Self {
        Self {
            preset: MaterialPreset::LiquidGlass,
            tint: 0xeaf6ff28,
            corner_radius: 28.0,
            blur_radius: 10.0,
            saturation: 1.2,
            refraction: 10.0,
            highlight: 0.7,
            border_width: 1.5,
            ..Self::solid()
        }
    }

    pub const fn paper() -> Self {
        Self {
            preset: MaterialPreset::Paper,
            tint: 0xf6f0e4ff,
            corner_radius: 8.0,
            noise: 0.055,
            highlight: 0.1,
            border_width: 0.5,
            ..Self::solid()
        }
    }

    pub const fn custom(shader: ShaderHandle) -> Self {
        Self {
            preset: MaterialPreset::Custom,
            tint: 0xffffff00,
            shader: Some(shader),
            ..Self::solid()
        }
    }

    /// Validate every parameter before renderer state can be mutated.
    pub fn validate(self) -> Result<Self, EffectError> {
        bounded("opacity", self.opacity, 0.0, 1.0)?;
        bounded("tint_mix", self.tint_mix, 0.0, 1.0)?;
        bounded("corner_radius", self.corner_radius, 0.0, 256.0)?;
        bounded("blur_radius", self.blur_radius, 0.0, 64.0)?;
        bounded("saturation", self.saturation, 0.0, 2.0)?;
        bounded("noise", self.noise, 0.0, 1.0)?;
        bounded("refraction", self.refraction, 0.0, 32.0)?;
        bounded("highlight", self.highlight, 0.0, 1.0)?;
        bounded("border_width", self.border_width, 0.0, 4.0)?;
        bounded("progressive_blur", self.progressive_blur, -1.0, 1.0)?;
        for parameter in self.parameters {
            bounded("shader parameter", parameter, -1_000_000.0, 1_000_000.0)?;
        }
        if (self.preset == MaterialPreset::Custom) != self.shader.is_some() {
            return Err(EffectError::ShaderMaterial);
        }
        if let Some(shader) = self.shader
            && (shader.owner == 0
                || shader.generation == 0
                || shader.native == 0
                || shader.native > MAX_SHADER_SLOTS as u32)
        {
            return Err(EffectError::ShaderMaterial);
        }
        Ok(self)
    }
}

/// The hinge direction in the untransformed source plane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum FoldAxis {
    #[default]
    Horizontal = 0,
    Vertical = 1,
}

/// A perspective fold of the captured plane. The caller owns animation time.
///
/// `anchor` is the hinge position from 0 to 1 along the axis perpendicular to the
/// hinge. `perspective` is the viewing distance in pixels. The renderer reports
/// when a backend cannot provide the requested projective transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fold {
    pub axis: FoldAxis,
    pub angle_degrees: f32,
    pub anchor: f32,
    pub perspective: f32,
    pub dimming: f32,
    /// Independent angle multiplier in 0..=1, with no implicit animation timer.
    pub progress: f32,
}

impl Default for Fold {
    fn default() -> Self {
        Self {
            axis: FoldAxis::Horizontal,
            angle_degrees: 0.0,
            anchor: 0.5,
            perspective: 1200.0,
            dimming: 0.0,
            progress: 1.0,
        }
    }
}

impl Fold {
    pub fn validate(self) -> Result<Self, EffectError> {
        bounded("fold angle", self.angle_degrees, -85.0, 85.0)?;
        bounded("fold anchor", self.anchor, 0.0, 1.0)?;
        bounded("fold perspective", self.perspective, 64.0, 16_384.0)?;
        bounded("fold dimming", self.dimming, 0.0, 1.0)?;
        bounded("fold progress", self.progress, 0.0, 1.0)?;
        Ok(self)
    }
}

/// Actual renderer features for the current window, not an OS name guess.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectCapabilities {
    pub gpu: bool,
    pub software: bool,
    pub backdrop_capture: bool,
    pub blur: bool,
    pub progressive_blur: bool,
    pub projective_fold: bool,
    pub custom_shaders: bool,
    pub max_texture_size: u32,
    pub max_cache_entries: u32,
    pub max_shader_slots: u32,
    pub max_shader_source_bytes: u32,
    pub max_cache_bytes: u64,
}

/// Cumulative counters and current resource use for the owning window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectStats {
    pub texture_bytes: u64,
    pub peak_texture_bytes: u64,
    pub captures: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub evictions: u64,
    pub blur_passes: u64,
    pub draw_calls: u64,
    pub shader_compiles: u64,
    pub fallbacks: u64,
    pub live_entries: u32,
    pub live_programs: u32,
    pub last_status: u32,
    pub quality: Quality,
}

/// A fallback is an observable result, never reported as the GPU effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectOutcome {
    Rendered,
    Fallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectError {
    NonFinite(&'static str),
    OutOfRange(&'static str),
    ShaderMaterial,
}

impl fmt::Display for EffectError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite(field) => write!(output, "{field} must be finite"),
            Self::OutOfRange(field) => write!(output, "{field} is outside its rendering limit"),
            Self::ShaderMaterial => {
                output.write_str("only a custom material may carry a shader handle")
            }
        }
    }
}

fn bounded(field: &'static str, value: f32, minimum: f32, maximum: f32) -> Result<(), EffectError> {
    if !value.is_finite() {
        Err(EffectError::NonFinite(field))
    } else if !(minimum..=maximum).contains(&value) {
        Err(EffectError::OutOfRange(field))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_valid_and_do_not_require_registration() {
        for material in [
            Material::solid(),
            Material::blur(),
            Material::frosted(),
            Material::mica(),
            Material::liquid_glass(),
            Material::paper(),
        ] {
            assert_eq!(material.validate(), Ok(material));
            assert!(material.shader.is_none());
        }
        assert_eq!(Fold::default().validate(), Ok(Fold::default()));
        assert_eq!(
            EffectBudget::default().validate(),
            Ok(EffectBudget::default())
        );
        assert_eq!(Quality::default(), Quality::Off);
    }

    #[test]
    fn all_material_float_inputs_reject_nonfinite_values() {
        let fields: [fn(&mut Material) -> &mut f32; 11] = [
            |m| &mut m.opacity,
            |m| &mut m.tint_mix,
            |m| &mut m.corner_radius,
            |m| &mut m.blur_radius,
            |m| &mut m.saturation,
            |m| &mut m.noise,
            |m| &mut m.refraction,
            |m| &mut m.highlight,
            |m| &mut m.border_width,
            |m| &mut m.progressive_blur,
            |m| &mut m.parameters[0],
        ];
        for field in fields {
            for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut material = Material::frosted();
                *field(&mut material) = value;
                assert!(matches!(
                    material.validate(),
                    Err(EffectError::NonFinite(_))
                ));
            }
        }
        for index in 1..8 {
            let mut material = Material::frosted();
            material.parameters[index] = f32::NAN;
            assert!(material.validate().is_err());
        }
    }

    #[test]
    fn numeric_limits_are_enforced_without_silent_clamping() {
        for material in [
            Material {
                opacity: 1.01,
                ..Material::solid()
            },
            Material {
                blur_radius: 64.01,
                ..Material::blur()
            },
            Material {
                corner_radius: -0.01,
                ..Material::solid()
            },
            Material {
                saturation: 2.01,
                ..Material::frosted()
            },
            Material {
                noise: -0.01,
                ..Material::paper()
            },
            Material {
                refraction: 32.01,
                ..Material::liquid_glass()
            },
            Material {
                highlight: 1.01,
                ..Material::liquid_glass()
            },
            Material {
                border_width: 4.01,
                ..Material::solid()
            },
            Material {
                progressive_blur: -1.01,
                ..Material::blur()
            },
            Material {
                parameters: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1_000_001.0],
                ..Material::solid()
            },
        ] {
            assert!(matches!(
                material.validate(),
                Err(EffectError::OutOfRange(_))
            ));
        }
        assert!(
            Material {
                progressive_blur: -1.0,
                ..Material::blur()
            }
            .validate()
            .is_ok()
        );
        assert!(
            Material {
                progressive_blur: 1.0,
                ..Material::blur()
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn custom_material_identity_is_explicit() {
        let handle = ShaderHandle {
            owner: 1,
            generation: 1,
            native: 1,
        };
        assert!(Material::custom(handle).validate().is_ok());
        assert_eq!(
            Material {
                preset: MaterialPreset::Custom,
                ..Material::solid()
            }
            .validate(),
            Err(EffectError::ShaderMaterial)
        );
        assert_eq!(
            Material {
                shader: Some(handle),
                ..Material::solid()
            }
            .validate(),
            Err(EffectError::ShaderMaterial)
        );
    }

    #[test]
    fn budgets_and_keys_reject_unbounded_or_reserved_values() {
        for budget in [
            EffectBudget::new(0, 1),
            EffectBudget::new(MAX_CACHE_BYTES + 1, 1),
            EffectBudget::new(1024, 0),
            EffectBudget::new(1024, MAX_CACHE_ENTRIES + 1),
            EffectBudget {
                max_shader_slots: MAX_SHADER_SLOTS as u32 + 1,
                ..EffectBudget::default()
            },
        ] {
            assert!(budget.validate().is_err());
        }
        assert!(
            EffectBudget::new(MAX_CACHE_BYTES, MAX_CACHE_ENTRIES)
                .validate()
                .is_ok()
        );
        assert!(CacheKey::new(0, 1).validate().is_err());
        assert!(CacheKey::new(1, 0).validate().is_ok());
        assert!(CacheKey::new(u64::MAX, u64::MAX).validate().is_ok());
    }

    #[test]
    fn fold_bounds_prevent_nonfinite_and_degenerate_inputs() {
        for fold in [
            Fold {
                angle_degrees: 85.01,
                ..Fold::default()
            },
            Fold {
                angle_degrees: f32::NAN,
                ..Fold::default()
            },
            Fold {
                anchor: -0.01,
                ..Fold::default()
            },
            Fold {
                perspective: 63.99,
                ..Fold::default()
            },
            Fold {
                perspective: f32::INFINITY,
                ..Fold::default()
            },
            Fold {
                dimming: 1.01,
                ..Fold::default()
            },
            Fold {
                progress: f32::NAN,
                ..Fold::default()
            },
            Fold {
                progress: 1.01,
                ..Fold::default()
            },
        ] {
            assert!(fold.validate().is_err());
        }
        for angle_degrees in [-85.0, 0.0, 85.0] {
            assert!(
                Fold {
                    angle_degrees,
                    ..Fold::default()
                }
                .validate()
                .is_ok()
            );
        }
    }
}
