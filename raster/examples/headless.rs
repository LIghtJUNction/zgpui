#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{BufWriter, Write};
    use zgpui_raster::{Canvas, Color, Font};

    let mut font = Font::new("")?;
    let width = 480;
    let height = 88;
    let mut pixels = vec![0; width * height];
    let mut canvas = Canvas::new(&mut pixels, width, height, width)?;
    canvas.clear(Color::rgb(250, 250, 250));
    canvas.rect(0, 0, width as u32, 1, Color::rgb(218, 220, 224));
    let raw = font.rasterize("nihaoshijie", 13, false)?;
    canvas.mask(&raw, 12, 9, Color::rgb(95, 99, 104));
    let candidates = ["1 你好世界", "2 你好", "3 世界", "4 かな"];
    let mut x = 12;
    for (index, candidate) in candidates.iter().enumerate() {
        let mask = font.rasterize(candidate, 16, false)?;
        let item_width = font.measure(candidate, 16, false)?.ceil() as u32 + 16;
        if index == 0 {
            canvas.rect(x, 42, item_width, 32, Color::rgb(232, 240, 254));
        }
        canvas.mask(&mask, x + 8, 44, Color::rgb(32, 33, 36));
        x += item_width as i32 + 6;
    }
    // Optional PPM export is a plain diagnostic artifact, not a presenter.
    if let Some(path) = std::env::args_os().nth(1) {
        let mut output = BufWriter::new(std::fs::File::create(path)?);
        writeln!(output, "P6\n{width} {height}\n255")?;
        for pixel in &pixels {
            output.write_all(&[(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8])?;
        }
        output.flush()?;
    }
    let ink = pixels
        .iter()
        .filter(|&&pixel| pixel != Color::rgb(250, 250, 250).argb())
        .count();
    assert!(ink > 100);
    println!("headless ARGB smoke: {width}x{height}, {ink} non-background pixels, Latin/CJK/kana");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("The system-font headless example requires Linux.");
    std::process::exit(1);
}
