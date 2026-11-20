use zgpui::native::{ButtonStyle, Canvas, Event, Rect, Ui, Window, colors};
fn main() -> Result<(), String> {
    let mut window = Window::new("zgpui native window", 900, 600)?;
    eprintln!("GPU: {}", window.gpu());
    let mut ui = Ui::default();
    let mut name = String::new();
    let mut event = None;
    loop {
        if matches!(event, Some(Event::Close)) {
            break;
        }
        ui.begin(event);
        let mut canvas: Canvas<'_> = window.begin(colors::BG);
        canvas.text("Native. Small. Yours.", 60., 55., 32, true, colors::TEXT);
        canvas.text(
            "A Project Z window, drawn by your graphics driver.",
            60.,
            110.,
            17,
            false,
            colors::MUTED,
        );
        ui.input(
            &mut canvas,
            1,
            Rect::new(60., 175., 500., 44.),
            &mut name,
            "Type here — Unicode and clipboard work",
            false,
            true,
        );
        if ui.button(
            &mut canvas,
            2,
            Rect::new(60., 245., 140., 42.),
            "Close",
            ButtonStyle::Primary,
            true,
        ) {
            break;
        }
        canvas.text(&name, 60., 320., 20, false, colors::TEXT);
        ui.end();
        drop(canvas);
        window.present();
        if let Some(text) = ui.clipboard.take() {
            window.copy(&text);
        }
        event = loop {
            if let Some(event) = window.event(1000) {
                break Some(event);
            }
        };
    }
    Ok(())
}
