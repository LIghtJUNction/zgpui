//! Bounded, headless pixel drawing for a caller-owned surface.
//!
//! Pixels are native Rust `u32` values in premultiplied `0xAARRGGBB` format.
//! Convert them to the presenter's required byte order at that boundary.
//! Drawing neither opens a window nor owns the presenter's buffer lifetime.
//! The default `font` feature adds Linux FreeType/fontconfig text; disable
//! default features for an allocation-free `no_std` pixel canvas.

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]

use core::fmt;

/// Maximum width, height, or row stride accepted by a canvas or input mask.
pub const MAX_DIMENSION: usize = 16_384;
/// Maximum backing area, including row padding (64 MiB of ARGB pixels).
pub const MAX_PIXELS: usize = 16 * 1024 * 1024;

#[cfg(all(feature = "font", target_os = "linux"))]
mod font;
#[cfg(all(feature = "font", target_os = "linux"))]
pub use font::{Font, MAX_FONT_SIZE, MAX_TEXT_BYTES, Mask};

/// Invalid input or an unavailable system font resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidDimensions,
    InvalidStride,
    BufferTooSmall,
    InvalidFontFamily,
    InvalidFontSize,
    TextTooLong,
    NulByte,
    FontUnavailable,
    RasterUnavailable,
    AllocationFailed,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidDimensions => "pixel dimensions exceed the canvas bounds",
            Self::InvalidStride => "pixel stride is smaller than the width or exceeds the limit",
            Self::BufferTooSmall => "pixel buffer is shorter than stride times height",
            Self::InvalidFontFamily => {
                "font family must contain at most 255 bytes without control characters"
            }
            Self::InvalidFontSize => "font size must be between 1 and 96 pixels",
            Self::TextTooLong => "text exceeds the 4096-byte font input limit",
            Self::NulByte => "text contains a NUL byte",
            Self::FontUnavailable => "the system could not prepare the font",
            Self::RasterUnavailable => {
                "text does not fit the font bitmap bounds or could not be rasterized"
            }
            Self::AllocationFailed => "could not allocate the font mask",
        })
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

/// A premultiplied color; constructors accept ordinary, unassociated channels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Color(u32);

impl Color {
    pub const TRANSPARENT: Self = Self(0);

    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self::rgba(red, green, blue, 255)
    }

    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        let a = alpha as u32;
        let r = (red as u32 * a + 127) / 255;
        let g = (green as u32 * a + 127) / 255;
        let b = (blue as u32 * a + 127) / 255;
        Self(a << 24 | r << 16 | g << 8 | b)
    }

    pub const fn argb(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy)]
struct Clip {
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
}

/// A bounded view of caller-owned premultiplied ARGB pixels.
///
/// Row stride is measured in `u32` pixels, not bytes. Existing pixels and
/// padding are preserved by construction. Call `clear` before drawing on a
/// fresh surface; source-over operations expect premultiplied destination
/// pixels. The presenter must retain the backing storage until it is released.
pub struct Canvas<'a> {
    pixels: &'a mut [u32],
    width: usize,
    height: usize,
    stride: usize,
    clip: Clip,
}

impl<'a> Canvas<'a> {
    pub fn new(
        pixels: &'a mut [u32],
        width: usize,
        height: usize,
        stride: usize,
    ) -> Result<Self, Error> {
        let length = checked_area(width, height, stride)?;
        if pixels.len() < length {
            return Err(Error::BufferTooSmall);
        }
        Ok(Self {
            pixels,
            width,
            height,
            stride,
            clip: Clip {
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            },
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }

    /// Replace every visible pixel, ignoring the clip and preserving row padding.
    pub fn clear(&mut self, color: Color) {
        for row in self.pixels.chunks_mut(self.stride).take(self.height) {
            row[..self.width].fill(color.0);
        }
    }

    /// Replace the clip rectangle, intersected with the canvas bounds.
    pub fn clip(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.clip = bounds(
            x,
            y,
            width as usize,
            height as usize,
            Clip {
                left: 0,
                top: 0,
                right: self.width,
                bottom: self.height,
            },
        );
    }

    pub fn reset_clip(&mut self) {
        self.clip = Clip {
            left: 0,
            top: 0,
            right: self.width,
            bottom: self.height,
        };
    }

    /// Blend a rectangle with source-over compositing, clipped to the canvas.
    pub fn rect(&mut self, x: i32, y: i32, width: u32, height: u32, color: Color) {
        let clip = bounds(x, y, width as usize, height as usize, self.clip);
        for row in clip.top..clip.bottom {
            let pixels =
                &mut self.pixels[row * self.stride + clip.left..row * self.stride + clip.right];
            if color.0 >> 24 == 255 {
                pixels.fill(color.0);
            } else {
                for pixel in pixels {
                    blend(pixel, color, 255);
                }
            }
        }
    }

    /// Blend a tightly bounded 8-bit coverage mask. Invalid input changes nothing.
    ///
    /// `stride` is measured in mask bytes. A zero-width or zero-height mask
    /// is a no-op; nonempty masks use the same area limits as the canvas.
    #[allow(clippy::too_many_arguments)]
    pub fn blend_mask(
        &mut self,
        pixels: &[u8],
        width: usize,
        height: usize,
        stride: usize,
        x: i32,
        y: i32,
        color: Color,
    ) -> Result<(), Error> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        let length = checked_area(width, height, stride)?;
        if pixels.len() < length {
            return Err(Error::BufferTooSmall);
        }
        self.blit(pixels, width, height, stride, x, y, color);
        Ok(())
    }

    #[cfg(all(feature = "font", target_os = "linux"))]
    pub fn mask(&mut self, mask: &Mask, x: i32, y: i32, color: Color) {
        self.blit(
            mask.pixels(),
            mask.width(),
            mask.height(),
            mask.width(),
            x,
            y,
            color,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn blit(
        &mut self,
        pixels: &[u8],
        width: usize,
        height: usize,
        stride: usize,
        x: i32,
        y: i32,
        color: Color,
    ) {
        let clip = bounds(x, y, width, height, self.clip);
        for row in clip.top..clip.bottom {
            let sy = (row as i64 - i64::from(y)) as usize;
            for column in clip.left..clip.right {
                let sx = (column as i64 - i64::from(x)) as usize;
                blend(
                    &mut self.pixels[row * self.stride + column],
                    color,
                    pixels[sy * stride + sx],
                );
            }
        }
    }
}

fn checked_area(width: usize, height: usize, stride: usize) -> Result<usize, Error> {
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(Error::InvalidDimensions);
    }
    if stride < width || stride > MAX_DIMENSION {
        return Err(Error::InvalidStride);
    }
    let length = stride.checked_mul(height).ok_or(Error::InvalidDimensions)?;
    if length > MAX_PIXELS {
        return Err(Error::InvalidDimensions);
    }
    Ok(length)
}

fn bounds(x: i32, y: i32, width: usize, height: usize, clip: Clip) -> Clip {
    let left = i64::from(x).clamp(clip.left as i64, clip.right as i64) as usize;
    let top = i64::from(y).clamp(clip.top as i64, clip.bottom as i64) as usize;
    let right = (i64::from(x) + width as i64).clamp(clip.left as i64, clip.right as i64) as usize;
    let bottom = (i64::from(y) + height as i64).clamp(clip.top as i64, clip.bottom as i64) as usize;
    Clip {
        left,
        top,
        right,
        bottom,
    }
}

fn blend(destination: &mut u32, color: Color, coverage: u8) {
    if coverage == 0 || color.0 >> 24 == 0 {
        return;
    }
    if coverage == 255 && color.0 >> 24 == 255 {
        *destination = color.0;
        return;
    }
    let scale = |value: u32| (value * u32::from(coverage) + 127) / 255;
    let alpha = scale(color.0 >> 24);
    let inverse = 255 - alpha;
    let channel = |shift: u32| {
        let source = scale((color.0 >> shift) & 255);
        let target = (*destination >> shift) & 255;
        (source + (target * inverse + 127) / 255).min(255)
    };
    *destination = channel(24) << 24 | channel(16) << 16 | channel(8) << 8 | channel(0);
}

#[cfg(test)]
mod tests;
