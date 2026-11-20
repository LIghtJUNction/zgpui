extern crate std;

use self::std::vec;
use super::*;

#[test]
fn premultiplied_source_over_and_mask_coverage() {
    let mut pixels = [0; 3];
    let mut canvas = Canvas::new(&mut pixels, 3, 1, 3).unwrap();
    canvas.clear(Color::TRANSPARENT);
    canvas.rect(0, 0, 1, 1, Color::rgba(255, 0, 0, 128));
    canvas.rect(0, 0, 1, 1, Color::rgba(0, 0, 255, 128));
    canvas
        .blend_mask(&[0, 128, 255], 3, 1, 3, 0, 0, Color::rgb(255, 255, 255))
        .unwrap();
    assert_eq!(pixels, [0xc0400080, 0x80808080, 0xffffffff]);
    assert_eq!(Color::rgba(200, 100, 50, 0).argb(), 0);
}

#[test]
fn clipping_preserves_stride_padding_and_outside_storage() {
    let mut pixels = [0x12345678; 14];
    {
        let mut canvas = Canvas::new(&mut pixels, 3, 2, 5).unwrap();
        canvas.clear(Color::TRANSPARENT);
        canvas.clip(1, 0, 2, 1);
        canvas.rect(-5, -5, 100, 100, Color::rgb(255, 255, 255));
        canvas.reset_clip();
        canvas.rect(i32::MIN, i32::MIN, 2, 2, Color::rgb(255, 0, 0));
    }
    assert_eq!(&pixels[..3], &[0, 0xffffffff, 0xffffffff]);
    assert_eq!(&pixels[5..8], &[0, 0, 0]);
    for index in [3, 4, 8, 9, 10, 11, 12, 13] {
        assert_eq!(pixels[index], 0x12345678);
    }
}

#[test]
fn negative_mask_origin_selects_the_correct_source_pixel() {
    let mut pixels = [0; 4];
    let mut canvas = Canvas::new(&mut pixels, 2, 2, 2).unwrap();
    canvas
        .blend_mask(
            &[10, 20, 30, 40],
            2,
            2,
            2,
            -1,
            -1,
            Color::rgb(255, 255, 255),
        )
        .unwrap();
    assert_eq!(pixels, [0x28282828, 0, 0, 0]);
}

#[test]
fn invalid_buffers_are_rejected_before_drawing() {
    let mut pixels = [0; 4];
    assert!(matches!(
        Canvas::new(&mut pixels, 0, 1, 1),
        Err(Error::InvalidDimensions)
    ));
    assert!(matches!(
        Canvas::new(&mut pixels, 2, 2, 1),
        Err(Error::InvalidStride)
    ));
    assert!(matches!(
        Canvas::new(&mut pixels, 2, 2, 3),
        Err(Error::BufferTooSmall)
    ));
    assert!(matches!(
        Canvas::new(&mut pixels, MAX_DIMENSION, MAX_DIMENSION, MAX_DIMENSION),
        Err(Error::InvalidDimensions)
    ));
    let mut canvas = Canvas::new(&mut pixels, 2, 2, 2).unwrap();
    canvas.clear(Color::rgb(5, 10, 15));
    assert_eq!(
        canvas.blend_mask(&[255], 2, 2, 2, 0, 0, Color::rgb(255, 0, 0)),
        Err(Error::BufferTooSmall)
    );
    assert_eq!(
        canvas.blend_mask(&[], usize::MAX, 1, usize::MAX, 0, 0, Color::rgb(255, 0, 0)),
        Err(Error::InvalidDimensions)
    );
    assert_eq!(pixels, [0xff050a0f; 4]);
}

#[test]
fn huge_rectangles_and_empty_clips_remain_bounded() {
    let mut pixels = [0; 4];
    let mut canvas = Canvas::new(&mut pixels, 2, 2, 2).unwrap();
    canvas.rect(i32::MIN, i32::MIN, u32::MAX, u32::MAX, Color::rgb(1, 2, 3));
    canvas.clip(i32::MAX, i32::MAX, u32::MAX, u32::MAX);
    canvas.rect(0, 0, 2, 2, Color::rgb(0, 0, 0));
    assert_eq!(pixels, [0xff010203; 4]);
}

#[test]
fn repeated_alpha_composition_preserves_premultiplied_channels() {
    let mut pixels = vec![0; 256];
    for alpha in 0..=255 {
        let color = Color::rgba(200, 100, 255, alpha);
        let mut canvas = Canvas::new(&mut pixels, 256, 1, 256).unwrap();
        canvas
            .blend_mask(
                &(0..=255).collect::<self::std::vec::Vec<_>>(),
                256,
                1,
                256,
                0,
                0,
                color,
            )
            .unwrap();
        for pixel in &pixels {
            let a = pixel >> 24;
            for shift in [0, 8, 16] {
                assert!((pixel >> shift) & 255 <= a);
            }
        }
    }
}

#[cfg(all(feature = "font", target_os = "linux"))]
mod fonts {
    use super::*;
    use crate::{Font, MAX_TEXT_BYTES};

    #[test]
    fn latin_chinese_and_kana_masks_are_owned_and_match_measurement() {
        let mut font = Font::new("").expect("installed scalable Latin/CJK fonts");
        for bold in [false, true] {
            for size in [12, 16, 32] {
                for text in ["nihao 123", "你好世界", "かなカナー。"] {
                    let advance = font.measure(text, size, bold).unwrap();
                    let mask = font.rasterize(text, size, bold).unwrap();
                    assert!(advance > 0.0);
                    assert_eq!(advance, mask.advance());
                    assert_eq!(mask.pixels().len(), mask.width() * mask.height());
                    assert!(mask.pixels().iter().any(|&value| value != 0));
                    assert!(mask.width() <= 8192 && mask.height() <= 256);
                }
            }
        }
        let retained = font.rasterize("你好", 16, false).unwrap();
        drop(font);
        let mut pixels = vec![0; retained.width() * retained.height()];
        let mut canvas = Canvas::new(
            &mut pixels,
            retained.width(),
            retained.height(),
            retained.width(),
        )
        .unwrap();
        canvas.mask(&retained, 0, 0, Color::rgb(255, 255, 255));
        assert!(pixels.iter().any(|&pixel| pixel != 0));
    }

    #[test]
    fn invalid_text_and_raster_bounds_do_not_poison_the_context() {
        let mut font = Font::new("").unwrap();
        assert_eq!(font.measure("abc", 0, false), Err(Error::InvalidFontSize));
        assert_eq!(font.measure("abc", 97, false), Err(Error::InvalidFontSize));
        assert_eq!(font.measure("a\0b", 16, false), Err(Error::NulByte));
        assert_eq!(
            font.measure(&"a".repeat(MAX_TEXT_BYTES + 1), 16, false),
            Err(Error::TextTooLong)
        );
        let wide = "W".repeat(MAX_TEXT_BYTES);
        assert!(font.measure(&wide, 96, false).unwrap() > 8192.0);
        assert!(matches!(
            font.rasterize(&wide, 96, false),
            Err(Error::RasterUnavailable)
        ));
        assert_eq!(font.measure("", 16, false).unwrap(), 0.0);
        assert!(font.rasterize("恢复", 16, false).unwrap().advance() > 0.0);
        assert!(matches!(
            Font::new("bad\nfont"),
            Err(Error::InvalidFontFamily)
        ));
        assert!(matches!(
            Font::new(&"a".repeat(256)),
            Err(Error::InvalidFontFamily)
        ));
    }

    #[test]
    fn independent_font_contexts_do_not_invalidate_each_other() {
        let mut first = Font::new("sans").unwrap();
        let expected = first.rasterize("共享字体", 20, false).unwrap();
        {
            let mut second = Font::new("monospace").unwrap();
            second.rasterize("other context", 32, true).unwrap();
        }
        let actual = first.rasterize("共享字体", 20, false).unwrap();
        assert_eq!(actual.pixels(), expected.pixels());
        assert_eq!(actual.advance(), expected.advance());
    }
}
