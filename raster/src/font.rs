use crate::Error;
use std::{
    ffi::{CString, c_char, c_int, c_void},
    marker::PhantomData,
    ptr::NonNull,
    rc::Rc,
};

pub const MAX_TEXT_BYTES: usize = 4096;
pub const MAX_FONT_SIZE: u32 = 96;

#[repr(C)]
struct RawMask {
    pixels: *mut u8,
    width: c_int,
    height: c_int,
    advance: f32,
}

unsafe extern "C" {
    fn zgpui_raster_font_create(family: *const c_char) -> *mut c_void;
    fn zgpui_raster_font_destroy(font: *mut c_void);
    fn zgpui_raster_font_measure(
        font: *mut c_void,
        text: *const c_char,
        size: c_int,
        bold: c_int,
    ) -> f32;
    fn zgpui_raster_font_mask(
        font: *mut c_void,
        text: *const c_char,
        size: c_int,
        bold: c_int,
    ) -> RawMask;
    fn zgpui_raster_mask_destroy(pixels: *mut u8);
}

/// An independently owned Linux FreeType/fontconfig font context.
///
/// An empty family uses the native renderer's `sans` default. Fallback follows
/// the host's font configuration. This renders scalar glyphs, not complex-script
/// shaping or bidirectional layout. The context is neither `Send` nor `Sync`;
/// create and use it on its owning rendering thread. Different contexts do not
/// share mutable FreeType faces or the native window renderer's text cache.
///
/// ```compile_fail
/// let font = zgpui_raster::Font::new("").unwrap();
/// std::thread::spawn(move || drop(font));
/// ```
pub struct Font {
    raw: NonNull<c_void>,
    _thread: PhantomData<Rc<()>>,
}

impl Font {
    pub fn new(family: &str) -> Result<Self, Error> {
        if family.len() > 255 || family.chars().any(char::is_control) {
            return Err(Error::InvalidFontFamily);
        }
        let family = CString::new(family).map_err(|_| Error::InvalidFontFamily)?;
        // SAFETY: the bridge copies a valid, bounded NUL-terminated family and
        // returns an independently owned FontState, or null on failure.
        let raw = NonNull::new(unsafe { zgpui_raster_font_create(family.as_ptr()) })
            .ok_or(Error::FontUnavailable)?;
        Ok(Self {
            raw,
            _thread: PhantomData,
        })
    }

    /// Measure the hinted advance without allocating or rendering a glyph bitmap.
    pub fn measure(&mut self, text: &str, size: u32, bold: bool) -> Result<f32, Error> {
        let text = checked_text(text, size)?;
        // SAFETY: self exclusively owns the live context; input is bounded UTF-8
        // and the bridge borrows the C string only during this call.
        let advance = unsafe {
            zgpui_raster_font_measure(
                self.raw.as_ptr(),
                text.as_ptr(),
                size as c_int,
                c_int::from(bold),
            )
        };
        if !advance.is_finite() || advance < 0.0 {
            return Err(Error::RasterUnavailable);
        }
        Ok(advance)
    }

    /// Produce an owned 8-bit coverage mask, at most 8192 by 256 pixels.
    ///
    /// The mask uses the same top-left origin and two-pixel padding as the
    /// existing native renderer. Its advance is the measured pen advance.
    pub fn rasterize(&mut self, text: &str, size: u32, bold: bool) -> Result<Mask, Error> {
        let text = checked_text(text, size)?;
        // SAFETY: same context/string ownership as measure. RawMask is a C ABI
        // structure and its allocation must be released with the bridge.
        let raw = unsafe {
            zgpui_raster_font_mask(
                self.raw.as_ptr(),
                text.as_ptr(),
                size as c_int,
                c_int::from(bold),
            )
        };
        let owned = OwnedRawMask(raw);
        let raw = &owned.0;
        if raw.pixels.is_null()
            || !(1..=8192).contains(&raw.width)
            || !(1..=256).contains(&raw.height)
            || !raw.advance.is_finite()
            || raw.advance < 0.0
        {
            return Err(Error::RasterUnavailable);
        }
        let width = raw.width as usize;
        let height = raw.height as usize;
        let length = width.checked_mul(height).ok_or(Error::RasterUnavailable)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(length)
            .map_err(|_| Error::AllocationFailed)?;
        // SAFETY: the shared C implementation allocates width * height bytes;
        // dimensions were checked above. Copy before the C-owned allocation drops.
        pixels.extend_from_slice(unsafe { std::slice::from_raw_parts(raw.pixels, length) });
        Ok(Mask {
            pixels,
            width,
            height,
            advance: raw.advance,
        })
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        // SAFETY: this context was created by the bridge and is uniquely owned.
        unsafe { zgpui_raster_font_destroy(self.raw.as_ptr()) };
    }
}

struct OwnedRawMask(RawMask);

impl Drop for OwnedRawMask {
    fn drop(&mut self) {
        // SAFETY: the bridge allocated this mask; its free also accepts null.
        unsafe { zgpui_raster_mask_destroy(self.0.pixels) };
    }
}

/// Owned glyph coverage, independent of the font context after rasterization.
#[derive(Debug)]
pub struct Mask {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
    advance: f32,
}

impl Mask {
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    pub fn width(&self) -> usize {
        self.width
    }
    pub fn height(&self) -> usize {
        self.height
    }
    pub fn advance(&self) -> f32 {
        self.advance
    }
}

fn checked_text(text: &str, size: u32) -> Result<CString, Error> {
    if size == 0 || size > MAX_FONT_SIZE {
        return Err(Error::InvalidFontSize);
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(Error::TextTooLong);
    }
    CString::new(text).map_err(|_| Error::NulByte)
}
