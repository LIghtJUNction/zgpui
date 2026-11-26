use zgpui::native::{ButtonStyle, Event, PaneLayout, Rect, Ui, Window, colors};

fn main() -> Result<(), String> {
    let mut window = Window::new("zgpui native window", 900, 600)?;
    window.set_min_size(220, 140)?;
    eprintln!("GPU: {}", window.gpu());
    let mut ui = Ui::default();
    let mut name = String::new();
    let mut event = None;
    let mut scroll_y = 0.0;
    loop {
        match event {
            Some(Event::Close) => break,
            Some(Event::Scroll(delta)) => scroll_y += delta as f32 * 36.,
            _ => {}
        }
        ui.begin(event);
        let mut canvas = window.begin(colors::BG);
        let viewport = Rect::new(0., 0., canvas.width, canvas.height).inset(16.);
        let layout = PaneLayout::new(viewport, 250., 300., 16.);
        let panes = layout.arrange(148., 160., scroll_y, false);
        scroll_y = panes.scroll_y;

        let controls = panes.sidebar;
        canvas.text("Text", controls.x, controls.y + 8., 19, true, colors::TEXT);
        ui.input(
            &mut canvas,
            1,
            Rect::new(controls.x, controls.y + 44., controls.w, 40.),
            &mut name,
            "Type here",
            false,
            true,
        );
        if ui.button(
            &mut canvas,
            2,
            Rect::new(controls.x, controls.y + 100., 110., 40.),
            "Close",
            ButtonStyle::Secondary,
            true,
        ) {
            break;
        }

        let preview = panes.main;
        canvas.rect(preview, 8., colors::SURFACE);
        canvas.text(
            "Preview",
            preview.x + 12.,
            preview.y + 12.,
            18,
            true,
            colors::TEXT,
        );
        canvas.ellipsis(
            &name,
            preview.x + 12.,
            preview.y + 48.,
            (preview.w - 24.).max(0.),
            20,
            false,
            colors::TEXT,
        );
        ui.end();
        drop(canvas);
        window.present();
        if let Some(text) = ui.clipboard.take() {
            window.copy(&text);
        }
        event = loop {
            if let Some(e) = window.event(1000) {
                break Some(e);
            }
        };
    }
    Ok(())
}
