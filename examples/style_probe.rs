//! Bounded real-window regression for optional semantic colors and fonts.
use std::{fs, io::Write, path::Path};
use zgpui::native::{ButtonStyle, Event, Palette, Rect, Theme, Ui, Window, colors};

const WIDTH: usize = 520;
const HEIGHT: usize = 260;

#[cfg(all(target_os = "linux", feature = "linux-software"))]
unsafe extern "C" {
    fn zgpui_software_pixels(width: *mut i32, height: *mut i32) -> *const u32;
}
#[cfg(not(all(target_os = "linux", feature = "linux-software")))]
unsafe extern "system" {
    fn glFinish();
    fn glReadBuffer(buffer: u32);
    fn glReadPixels(x: i32, y: i32, w: i32, h: i32, format: u32, ty: u32, data: *mut u8);
    fn glGetString(name: u32) -> *const u8;
    fn glGetIntegerv(name: u32, value: *mut i32);
    fn glGetError() -> u32;
}

#[cfg(not(all(target_os = "linux", feature = "linux-software")))]
fn record_graphics(directory: &Path) -> std::io::Result<()> {
    use std::ffi::CStr;
    fn quoted(value: &str) -> String {
        let mut out = String::from("\"");
        for c in value.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }
    let string = |name| unsafe {
        let pointer = glGetString(name);
        assert!(!pointer.is_null(), "missing OpenGL context capability");
        CStr::from_ptr(pointer.cast())
            .to_string_lossy()
            .into_owned()
    };
    let version = string(0x1f02);
    let renderer = string(0x1f01);
    let extensions = string(0x1f03);
    let mut maximum = 0;
    unsafe { glGetIntegerv(0x0d33, &mut maximum) };
    assert!(
        maximum >= 64,
        "OpenGL texture limit is below its required minimum"
    );
    let npot = extensions
        .split_whitespace()
        .any(|x| x == "GL_ARB_texture_non_power_of_two");
    let summary = format!(
        "{{\"version\":{},\"renderer\":{},\"max_texture_size\":{},\"npot_extension\":{}}}",
        quoted(&version),
        quoted(&renderer),
        maximum,
        npot
    );
    println!("{summary}");
    fs::write(
        directory.join("graphics.json"),
        format!(
            "{{\"version\":{},\"renderer\":{},\"max_texture_size\":{},\"extensions\":{}}}\n",
            quoted(&version),
            quoted(&renderer),
            maximum,
            quoted(&extensions)
        ),
    )
}

fn pixels() -> Vec<u8> {
    let mut result = vec![0; WIDTH * HEIGHT * 4];
    #[cfg(all(target_os = "linux", feature = "linux-software"))]
    unsafe {
        let (mut width, mut height) = (0, 0);
        let data = zgpui_software_pixels(&mut width, &mut height);
        assert_eq!((width, height), (WIDTH as i32, HEIGHT as i32));
        assert!(!data.is_null());
        for (pixel, out) in std::slice::from_raw_parts(data, WIDTH * HEIGHT)
            .iter()
            .zip(result.as_chunks_mut::<4>().0)
        {
            // The software framebuffer uses native XRGB, independent of host byte order.
            out.copy_from_slice(&[(*pixel >> 16) as u8, (*pixel >> 8) as u8, *pixel as u8, 255]);
        }
    }
    #[cfg(not(all(target_os = "linux", feature = "linux-software")))]
    unsafe {
        glFinish();
        glReadBuffer(0x0405); // GL_BACK, before presentation.
        let mut raw = vec![0; result.len()];
        glReadPixels(
            0,
            0,
            WIDTH as i32,
            HEIGHT as i32,
            0x1908,
            0x1401,
            raw.as_mut_ptr(),
        );
        assert_eq!(glGetError(), 0, "OpenGL rendering or readback failed");
        for y in 0..HEIGHT {
            result[y * WIDTH * 4..(y + 1) * WIDTH * 4]
                .copy_from_slice(&raw[(HEIGHT - 1 - y) * WIDTH * 4..(HEIGHT - y) * WIDTH * 4]);
        }
    }
    result
}

fn scene(window: &mut Window) -> Vec<u8> {
    let mut ui = Ui::default();
    let mut value = "iii WWW 0123".to_owned();
    for event in [
        None,
        Some(Event::Down(30., 100.)),
        Some(Event::Key {
            code: 65,
            control: true,
            shift: false,
            text: String::new(),
        }),
    ] {
        ui.begin(event);
        {
            let mut canvas = window.begin(colors::BG);
            ui.button(
                &mut canvas,
                1,
                Rect::new(20., 20., 140., 44.),
                "Primary",
                ButtonStyle::Primary,
                true,
            );
            ui.button(
                &mut canvas,
                2,
                Rect::new(180., 20., 140., 44.),
                "Disabled",
                ButtonStyle::Primary,
                false,
            );
            ui.button(
                &mut canvas,
                3,
                Rect::new(340., 20., 140., 44.),
                "Danger",
                ButtonStyle::Danger,
                true,
            );
            ui.input(
                &mut canvas,
                4,
                Rect::new(20., 90., 300., 44.),
                &mut value,
                "Input",
                false,
                true,
            );
            ui.input(
                &mut canvas,
                5,
                Rect::new(340., 90., 140., 44.),
                &mut String::new(),
                "Disabled",
                false,
                false,
            );
            canvas.text(
                "Regular / 中文 / Ελληνικά",
                20.,
                158.,
                18,
                false,
                colors::TEXT,
            );
            canvas.text("Bold / 中文 / Ελληνικά", 20., 188., 18, true, colors::TEXT);
            canvas.rect(Rect::new(20., 225., 80., 20.), 0., 0x1a73e880);
            canvas.rect(Rect::new(120., 225., 80., 20.), 0., 0x31a271ff);
        }
        ui.end();
    }
    pixels()
}

fn rgb(pixels: &[u8], x: usize, y: usize) -> [u8; 3] {
    pixels[(y * WIDTH + x) * 4..(y * WIDTH + x) * 4 + 3]
        .try_into()
        .unwrap()
}
fn near(actual: [u8; 3], expected: u32) {
    let expected = [
        (expected >> 24) as u8,
        (expected >> 16) as u8,
        (expected >> 8) as u8,
    ];
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
        "{actual:?} != {expected:?}"
    );
}
fn save(directory: &Path, name: &str, pixels: &[u8]) -> std::io::Result<()> {
    let mut file = fs::File::create(directory.join(name))?;
    write!(file, "P6\n{WIDTH} {HEIGHT}\n255\n")?;
    for pixel in pixels.as_chunks::<4>().0 {
        file.write_all(&pixel[..3])?;
    }
    Ok(())
}
fn widths(window: &mut Window) -> (f32, f32) {
    let canvas = window.begin(colors::BG);
    (
        canvas.measure("iiiiiiii", 20, false),
        canvas.measure("WWWWWWWW", 20, false),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("provide an output directory")?;
    let directory = Path::new(&output);
    fs::create_dir_all(directory)?;
    let mut window = Window::new("zgpui-style-probe", WIDTH as i32, HEIGHT as i32)?;
    #[cfg(not(all(target_os = "linux", feature = "linux-software")))]
    record_graphics(directory)?;
    assert!(Window::new("second window must be rejected", 100, 100).is_err());
    let default_widths = widths(&mut window);
    let baseline = scene(&mut window);
    near(rgb(&baseline, 5, 5), colors::BG);
    near(rgb(&baseline, 35, 35), colors::BLUE);
    save(directory, "default.ppm", &baseline)?;
    window.set_theme(Theme::Dark);
    let dark = scene(&mut window);
    near(rgb(&dark, 5, 5), Theme::Dark.color(colors::BG));

    let mut ink = Palette::for_theme(Theme::Dark);
    ink.background = 0x161412ff;
    ink.surface = 0x201c18ff;
    ink.text = 0xe8ded2ff;
    ink.muted = 0xa3978bff;
    ink.line = 0x383029ff;
    ink.accent = 0xd4a373ff;
    ink.accent_hover = 0xe2b88eff;
    ink.accent_soft = 0x45382eff;
    ink.danger = 0xe59a91ff;
    ink.danger_soft = 0x482b28ff;
    ink.disabled_surface = 0x2a2521ff;
    ink.disabled_text = 0x80756aff;
    // White is itself a canonical surface token: primary text must not remap it.
    ink.on_accent = 0xffffffff;
    window.set_palette(ink);
    window.set_font_family("monospace")?;
    let mono = widths(&mut window);
    assert!((mono.0 - mono.1).abs() <= 0.5, "monospace widths {mono:?}");
    let styled = scene(&mut window);
    near(rgb(&styled, 5, 5), ink.background);
    near(rgb(&styled, 35, 35), ink.accent);
    near(rgb(&styled, 195, 35), ink.disabled_surface);
    near(rgb(&styled, 21, 110), ink.accent);
    near(rgb(&styled, 40, 99), ink.accent_soft);
    near(rgb(&styled, 465, 120), ink.disabled_surface);
    near(rgb(&styled, 140, 232), 0x31a271ff);
    near(rgb(&styled, 40, 232), 0x755c43ff); // half-alpha copper over warm black
    let white_text = (25..60)
        .flat_map(|y| (30..150).map(move |x| (x, y)))
        .filter(|(x, y)| rgb(&styled, *x, *y).iter().all(|v| *v > 235))
        .count();
    assert!(white_text > 12, "primary text was remapped as a surface");
    save(directory, "custom.ppm", &styled)?;
    window.present();

    // A long line must retain metrics and visible pixels when the driver needs
    // several smaller textures. The prefix covers the complete viewport.
    let line = |window: &mut Window, count| {
        let advance = {
            let mut canvas = window.begin(colors::BG);
            let text = "M".repeat(count);
            let advance = canvas.measure(&text, 20, false);
            canvas.text(&text, 20., 150., 20, false, colors::TEXT);
            advance
        };
        (advance, pixels())
    };
    let prefix = line(&mut window, 80);
    let long = line(&mut window, 400);
    assert!(prefix.0 > WIDTH as f32 && long.0 > prefix.0);
    save(directory, "prefix-text.ppm", &prefix.1)?;
    save(directory, "long-text.ppm", &long.1)?;
    // Texture coordinates can round a filtered channel by one level on a
    // different texture size. Match the GPU tile fixture's 2/255 tolerance.
    let mut different_channels = 0;
    let mut max_delta = 0;
    for (actual, expected) in long.1.iter().zip(&prefix.1) {
        let delta = actual.abs_diff(*expected);
        different_channels += usize::from(delta != 0);
        max_delta = max_delta.max(delta);
    }
    let comparison = format!(
        "{{\"different_channels\":{different_channels},\"max_delta\":{max_delta},\"tolerance\":2}}"
    );
    println!("{comparison}");
    fs::write(directory.join("long-text-comparison.json"), comparison)?;
    assert!(
        max_delta <= 2,
        "long text differs across a texture boundary: {different_channels} channels, max delta {max_delta}/255"
    );

    for invalid in ["bad\nfamily", "bad\0family", &"x".repeat(256)] {
        assert!(window.set_font_family(invalid).is_err());
        assert_eq!(widths(&mut window), mono);
    }
    window.set_font_family("monospace")?;
    assert_eq!(scene(&mut window), styled);
    window.set_font_family("zgpui-no-such-installed-font-97493")?;
    let missing = widths(&mut window);
    assert!((missing.0 - missing.1).abs() <= 0.5);
    for _ in 0..6 {
        window.set_font_family("")?;
        assert_eq!(widths(&mut window), default_widths);
        window.set_font_family("monospace")?;
        assert_eq!(widths(&mut window), mono);
    }
    window.set_font_family("")?;
    window.clear_palette();
    assert_eq!(scene(&mut window), dark);
    window.set_theme(Theme::Light);
    assert_eq!(scene(&mut window), baseline);
    drop(window);
    let mut reopened = Window::new("zgpui-style-reopened", WIDTH as i32, HEIGHT as i32)?;
    assert_eq!(widths(&mut reopened), default_widths);
    assert_eq!(scene(&mut reopened), baseline);
    reopened.present();
    println!(
        "{{\"passed\":true,\"palette_pixels\":true,\"input_focus_selection\":true,\"primary_text\":true,\"font_switch_reset\":true,\"invalid_keeps_font\":true,\"sequential_window_isolation\":true,\"concurrent_window_support\":false}}"
    );
    Ok(())
}
