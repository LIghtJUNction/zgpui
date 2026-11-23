//! Exercise the public RGBA API with generated pixels or a caller-supplied raw frame.
//! Usage: image_probe OUTPUT_DIRECTORY [WIDTH HEIGHT RGBA_FILE]
use std::{fs, io::Write, path::Path};
use zgpui::native::{Palette, Rect, Theme, Window, colors};

const WIDTH: usize = 640;
const HEIGHT: usize = 480;
const BACKGROUND: [u8; 3] = [0x22, 0x33, 0x44];

#[cfg(all(target_os = "linux", feature = "linux-software"))]
unsafe extern "C" {
    fn zgpui_software_pixels(width: *mut i32, height: *mut i32) -> *const u32;
}
#[cfg(not(all(target_os = "linux", feature = "linux-software")))]
unsafe extern "system" {
    fn glFinish();
    fn glReadBuffer(buffer: u32);
    fn glReadPixels(x: i32, y: i32, w: i32, h: i32, format: u32, ty: u32, data: *mut u8);
    fn glGetError() -> u32;
}

fn pixels() -> Vec<u8> {
    let mut result = vec![0; WIDTH * HEIGHT * 3];
    #[cfg(all(target_os = "linux", feature = "linux-software"))]
    unsafe {
        let (mut width, mut height) = (0, 0);
        let data = zgpui_software_pixels(&mut width, &mut height);
        assert_eq!((width, height), (WIDTH as i32, HEIGHT as i32));
        assert!(!data.is_null());
        for (pixel, rgb) in std::slice::from_raw_parts(data, WIDTH * HEIGHT)
            .iter()
            .zip(result.as_chunks_mut::<3>().0)
        {
            rgb.copy_from_slice(&[(*pixel >> 16) as u8, (*pixel >> 8) as u8, *pixel as u8]);
        }
    }
    #[cfg(not(all(target_os = "linux", feature = "linux-software")))]
    unsafe {
        let mut raw = vec![0; WIDTH * HEIGHT * 4];
        glFinish();
        glReadBuffer(0x0405); // GL_BACK before presentation.
        glReadPixels(
            0,
            0,
            WIDTH as i32,
            HEIGHT as i32,
            0x1908,
            0x1401,
            raw.as_mut_ptr(),
        );
        assert_eq!(glGetError(), 0);
        for row in 0..HEIGHT {
            for col in 0..WIDTH {
                let from = ((HEIGHT - 1 - row) * WIDTH + col) * 4;
                let to = (row * WIDTH + col) * 3;
                result[to..to + 3].copy_from_slice(&raw[from..from + 3]);
            }
        }
    }
    result
}

fn source() -> (u32, u32, Vec<u8>) {
    let (width, height) = (96, 54);
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        for col in 0..width {
            let color = match (col / 24, row / 18) {
                (0, 0) => [255, 0, 0, 255],
                (1, 0) => [0, 255, 0, 255],
                (2, 0) => [0, 0, 255, 255],
                (3, 0) => [255, 255, 255, 255],
                (_, 1) => [(col * 255 / 95) as u8, 0x73, 0xe8, 255],
                (0, 2) => [255, 255, 255, 0],
                (1, 2) => [255, 255, 255, 128],
                (2, 2) => [0x1a, 0x73, 0xe8, 255],
                _ => [0x20, 0x21, 0x24, 255],
            };
            rgba.extend_from_slice(&color);
        }
    }
    (width as u32, height as u32, rgba)
}

fn scene(
    window: &mut Window,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<(Rect, Vec<u8>), String> {
    let scale = (592. / width as f32).min(333. / height as f32);
    let rect = Rect::new(
        24. + (592. - width as f32 * scale) * 0.5,
        64. + (333. - height as f32 * scale) * 0.5,
        width as f32 * scale,
        height as f32 * scale,
    );
    {
        let mut canvas = window.begin(colors::BG);
        canvas.text("RGBA8 / native image", 24., 22., 20, true, colors::TEXT);
        canvas.rect(Rect::new(24., 64., 592., 333.), 0., 0x223344ff);
        canvas.clip(Rect::new(24., 64., 592., 333.));
        canvas.image(rect, width, height, rgba)?;
        canvas.unclip();
        canvas.text(
            "Top-down  /  straight alpha  /  original colors",
            24.,
            427.,
            16,
            false,
            colors::MUTED,
        );
        assert!(canvas.image(Rect::new(0., 0., 1., 1.), 1, 1, &[]).is_err());
    }
    Ok((rect, pixels()))
}

fn check_samples(frame: &[u8], rect: Rect, width: u32, height: u32, rgba: &[u8]) {
    for row in 1..9 {
        for col in 1..9 {
            let x = (rect.x + rect.w * col as f32 / 10.) as usize;
            let y = (rect.y + rect.h * row as f32 / 10.) as usize;
            let sx = ((x as f64 + 0.5 - f64::from(rect.x)) * f64::from(width) / f64::from(rect.w))
                as usize;
            let sy = ((y as f64 + 0.5 - f64::from(rect.y)) * f64::from(height) / f64::from(rect.h))
                as usize;
            let source = &rgba[(sy * width as usize + sx) * 4..][..4];
            let actual = &frame[(y * WIDTH + x) * 3..][..3];
            for channel in 0..3 {
                let alpha = u32::from(source[3]);
                let expected = (u32::from(source[channel]) * alpha
                    + u32::from(BACKGROUND[channel]) * (255 - alpha)
                    + 127)
                    / 255;
                assert!(
                    (i32::from(actual[channel]) - expected as i32).abs() <= 1,
                    "image pixel mismatch at {x},{y}, channel {channel}: {:?}",
                    actual
                );
            }
        }
    }
}

fn save(directory: &Path, name: &str, rgb: &[u8]) -> std::io::Result<()> {
    let mut file = fs::File::create(directory.join(name))?;
    write!(file, "P6\n{WIDTH} {HEIGHT}\n255\n")?;
    file.write_all(rgb)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 2 && args.len() != 5 {
        return Err("usage: image_probe OUTPUT_DIRECTORY [WIDTH HEIGHT RGBA_FILE]".into());
    }
    let directory = Path::new(&args[1]);
    fs::create_dir_all(directory)?;
    let (width, height, rgba) = if args.len() == 5 {
        let width: u32 = args[2].parse()?;
        let height: u32 = args[3].parse()?;
        if width == 0 || height == 0 || width > 16384 || height > 16384 {
            return Err("invalid packed RGBA8 input dimensions".into());
        }
        let size = u64::from(width) * u64::from(height) * 4;
        if size > 256 * 1024 * 1024 || fs::metadata(&args[4])?.len() != size {
            return Err("invalid packed RGBA8 input dimensions or file length".into());
        }
        (width, height, fs::read(&args[4])?)
    } else {
        source()
    };
    let mut window = Window::new("zgpui-image-probe", WIDTH as i32, HEIGHT as i32)?;
    let (rect, light) = scene(&mut window, width, height, &rgba)?;
    check_samples(&light, rect, width, height, &rgba);
    save(directory, "light.ppm", &light)?;
    window.set_theme(Theme::Dark);
    let mut palette = Palette::for_theme(Theme::Dark);
    palette.surface = 0xaa00aaff;
    palette.accent = 0xffff00ff;
    palette.text = 0x00ffffff;
    window.set_palette(palette);
    let (rect, dark) = scene(&mut window, width, height, &rgba)?;
    check_samples(&dark, rect, width, height, &rgba);
    for row in 64..397 {
        let from = (row * WIDTH + 24) * 3;
        let to = (row * WIDTH + 616) * 3;
        assert_eq!(
            &light[from..to],
            &dark[from..to],
            "theme transformed image pixels"
        );
    }
    save(directory, "dark.ppm", &dark)?;
    window.present();
    println!(
        "{}",
        serde_json::json!({"ready":true,"width":WIDTH,"height":HEIGHT,
        "source_width":width,"source_height":height,"source_bytes":rgba.len(),
        "renderer":window.gpu(),"sample_checks":128,"theme_image_pixels_identical":true})
    );
    std::io::stdout().flush()?;
    if std::env::var_os("ZGPUI_IMAGE_PROBE_WAIT").is_some() {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
    }
    Ok(())
}
