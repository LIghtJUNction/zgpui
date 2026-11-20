use std::time::{Duration, Instant};
use zgpui::native::{Event, Window, colors};

fn main() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let title = args.first().map(String::as_str).unwrap_or("zgpui-probe");
    let dimension = |at: usize, default: i32| -> Result<i32, String> {
        args.get(at)
            .map(|v| v.parse::<i32>().map_err(|e| e.to_string()))
            .unwrap_or(Ok(default))
    };
    let mut window = Window::new(title, dimension(1, 660)?, dimension(2, 264)?)?;
    {
        let mut canvas = window.begin(colors::SURFACE);
        canvas.text(title, 18., 18., 18, true, colors::TEXT);
    }
    window.present();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(15) {
        match window.event(500) {
            Some(Event::Close) => break,
            Some(Event::Resize) => {
                {
                    let mut canvas = window.begin(colors::SURFACE);
                    canvas.text(title, 18., 18., 18, true, colors::TEXT);
                }
                window.present();
            }
            _ => {}
        }
    }
    Ok(())
}
