//! Screen-space layout for responsive native windows.
use super::Rect;

/// Measures the two widths before the host measures wrapping controls and text.
#[derive(Clone, Copy, Debug)]
pub struct PaneLayout {
    viewport: Rect,
    sidebar_width: f32,
    gap: f32,
    stacked: bool,
}

/// Screen-space rectangles for drawing and hit testing in the current frame.
#[derive(Clone, Copy, Debug)]
pub struct PaneAreas {
    pub sidebar: Rect,
    pub main: Rect,
    pub content_height: f32,
    pub scroll_y: f32,
    pub max_scroll: f32,
}

fn extent(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0., i32::MAX as f32)
    } else {
        0.
    }
}

impl PaneLayout {
    /// Stack when the sidebar, gap and requested main width cannot fit side by side.
    /// Supply the inner viewport after subtracting margins and fixed chrome.
    pub fn new(viewport: Rect, sidebar_width: f32, main_min_width: f32, gap: f32) -> Self {
        let viewport = Rect::new(
            viewport.x,
            viewport.y,
            extent(viewport.w),
            extent(viewport.h),
        );
        let sidebar_width = extent(sidebar_width);
        let gap = extent(gap);
        let stacked = viewport.w < sidebar_width + gap + extent(main_min_width);
        Self {
            viewport,
            sidebar_width,
            gap,
            stacked,
        }
    }

    pub fn stacked(&self) -> bool {
        self.stacked
    }

    /// Sidebar width followed by main width. In the stacked layout both equal the viewport width.
    pub fn widths(&self) -> (f32, f32) {
        if self.stacked {
            (self.viewport.w, self.viewport.w)
        } else {
            (
                self.sidebar_width,
                (self.viewport.w - self.sidebar_width - self.gap).max(0.),
            )
        }
    }

    /// Allocate the heights measured at `widths()` and clamp the previous scroll offset.
    /// Side-by-side panes share a height. Stacked panes give spare height to the main pane.
    /// `main_first` changes only their order in the stacked layout.
    pub fn arrange(
        &self,
        sidebar_min_height: f32,
        main_min_height: f32,
        scroll_y: f32,
        main_first: bool,
    ) -> PaneAreas {
        let sidebar_min = extent(sidebar_min_height);
        let main_min = extent(main_min_height);
        let v = self.viewport;
        let (sw, mw) = self.widths();
        let natural = if self.stacked {
            sidebar_min + self.gap + main_min
        } else {
            sidebar_min.max(main_min)
        };
        let content_height = natural.max(v.h);
        let max_scroll = (content_height - v.h).max(0.);
        let scroll_y = scroll_y.max(0.).min(max_scroll);
        let y = v.y - scroll_y;
        let (sidebar, main) = if self.stacked {
            let main_h = main_min + (content_height - natural);
            let (side_y, main_y) = if main_first {
                (y + main_h + self.gap, y)
            } else {
                (y, y + sidebar_min + self.gap)
            };
            (
                Rect::new(v.x, side_y, sw, sidebar_min),
                Rect::new(v.x, main_y, mw, main_h),
            )
        } else {
            (
                Rect::new(v.x, y, sw, content_height),
                Rect::new(v.x + sw + self.gap, y, mw, content_height),
            )
        };
        PaneAreas {
            sidebar,
            main,
            content_height,
            scroll_y,
            max_scroll,
        }
    }
}

/// Equal-width cells in row-major order. The container width determines how many fit.
#[derive(Clone, Copy, Debug)]
pub struct GridLayout {
    x: f32,
    y: f32,
    width: f32,
    gap: f32,
    columns: usize,
}

impl GridLayout {
    /// Fit at least one column, up to `max_cols`, including gaps between cells.
    pub fn new(area: Rect, min_cell_width: f32, max_cols: usize, gap: f32) -> Self {
        let gap = extent(gap);
        let width = extent(area.w);
        let min_width = extent(min_cell_width).max(1.);
        let columns =
            (((width + gap) / (min_width + gap)).floor() as usize).clamp(1, max_cols.max(1));
        let cell_width = ((width - gap * (columns - 1) as f32) / columns as f32).max(0.);
        Self {
            x: area.x,
            y: area.y,
            width: cell_width,
            gap,
            columns,
        }
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    /// Return one cell in the same window coordinates as the supplied rectangle.
    pub fn cell(&self, index: usize, row_height: f32) -> Rect {
        let row_height = extent(row_height);
        let col = index % self.columns;
        let row = index / self.columns;
        Rect::new(
            self.x + col as f32 * (self.width + self.gap),
            self.y + row as f32 * (row_height + self.gap),
            self.width,
            row_height,
        )
    }

    /// Content height including only the gaps between nonempty rows.
    pub fn height(&self, count: usize, row_height: f32) -> f32 {
        let rows = count.div_ceil(self.columns);
        rows as f32 * extent(row_height) + rows.saturating_sub(1) as f32 * self.gap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::{Event, Ui};

    fn dims(r: Rect) -> [f32; 4] {
        [r.x, r.y, r.w, r.h]
    }

    #[test]
    fn grid_wraps_at_the_measured_minimum_and_preserves_window_origin() {
        let grid = |w| GridLayout::new(Rect::new(5., -30., w, 0.), 100., 3, 8.);
        assert_eq!(grid(0.).columns(), 1);
        assert_eq!(grid(207.).columns(), 1);
        let g = grid(208.);
        assert_eq!(g.columns(), 2);
        assert_eq!(dims(g.cell(3, 32.)), [113., 10., 100., 32.]);
        assert_eq!(g.height(0, 32.), 0.);
        assert_eq!(g.height(5, 32.), 112.);
        assert_eq!(grid(720.).columns(), 3);
    }

    #[test]
    fn tall_wide_frame_grows_and_drops_old_scroll() {
        let layout = PaneLayout::new(Rect::new(24., 72., 1150., 1428.), 310., 460., 30.);
        assert!(!layout.stacked());
        let a = layout.arrange(510., 720., 320., true);
        assert_eq!(dims(a.sidebar), [24., 72., 310., 1428.]);
        assert_eq!(dims(a.main), [364., 72., 810., 1428.]);
        assert_eq!(
            (a.content_height, a.scroll_y, a.max_scroll),
            (1428., 0., 0.)
        );
        let short = PaneLayout::new(Rect::new(24., 72., 1150., 340.), 310., 460., 30.)
            .arrange(510., 720., 700., false);
        assert_eq!(short.main.y, -308.);
        assert_eq!(short.main.y + short.main.h, 412.);
        assert_eq!(short.max_scroll, 380.);
    }

    #[test]
    fn narrow_order_overflow_and_extra_height_use_container_width() {
        let layout = PaneLayout::new(Rect::new(8., 30., 340., 500.), 180., 240., 12.);
        assert_eq!(layout.widths(), (340., 340.));
        let a = layout.arrange(230., 400., 999., false);
        assert_eq!(a.max_scroll, 142.);
        assert_eq!(dims(a.sidebar), [8., -112., 340., 230.]);
        assert_eq!(dims(a.main), [8., 130., 340., 400.]);
        let b = layout.arrange(230., 400., 142., true);
        assert_eq!(dims(b.main), [8., -112., 340., 400.]);
        assert_eq!(dims(b.sidebar), [8., 300., 340., 230.]);
        let tall = PaneLayout::new(Rect::new(8., 30., 340., 1000.), 180., 240., 12.)
            .arrange(230., 400., 142., false);
        assert_eq!(dims(tall.main), [8., 272., 340., 758.]);
        assert_eq!(tall.scroll_y, 0.);
        assert!(!PaneLayout::new(Rect::new(0., 0., 432., 1.), 180., 240., 12.).stacked());
        assert!(PaneLayout::new(Rect::new(0., 0., 431., 1.), 180., 240., 12.).stacked());
    }

    #[test]
    fn focus_follows_widget_id_and_hits_follow_current_scrolled_rectangles() {
        let wide = PaneLayout::new(Rect::new(0., 0., 900., 400.), 200., 400., 20.)
            .arrange(220., 500., 0., false);
        let narrow = PaneLayout::new(Rect::new(0., 0., 340., 400.), 200., 400., 20.)
            .arrange(220., 500., 60., false);
        let field = |pane: Rect| Rect::new(pane.x + 8., pane.y + 12., 96., 36.);
        let mut ui = Ui::default();
        ui.begin(Some(Event::Down(235., 20.)));
        assert!(ui.register(41, field(wide.main)));
        ui.end();
        ui.begin(Some(Event::Resize));
        assert!(!ui.register(41, field(narrow.main)));
        ui.end();
        assert_eq!(ui.focus, Some(41));
        assert!(ui.pointer_changed(20., 202.));
        ui.begin(Some(Event::Up(20., 202.)));
        assert!(ui.register(41, field(narrow.main)));
        ui.end();
        assert_eq!(ui.focus, Some(41));
    }
}
