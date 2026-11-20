//! Prepared real headless rendering adapter; not part of the CPU fixture.
//! Requires the separately recorded headless observability patch.
//! No compilation or GPU execution has been performed for this module yet.

use gpui::{
    BorderStyle, Bounds, ContentMask, Corners, DevicePixels, Edges, Hsla, PlatformHeadlessRenderer,
    Point, Quad, ScaledPixels, Size,
};
use gpui_wgpu::{WgpuHeadlessRenderer, wgpu};
use serde::Serialize;
use std::time::Instant;
use zgpui::{
    Kind, Node,
    layout::{Layout, Rect, Renderer},
};

type Result<T> = std::result::Result<T, String>;

/// A physical-pixel oracle independent of either layout engine.
pub struct GridOracle {
    side: usize,
    leaf_width: i32,
}

impl GridOracle {
    pub fn new(side: usize, width_variant: usize) -> Result<Self> {
        if ![1, 4, 8, 15].contains(&side) || width_variant > 4 {
            return Err("case is outside the fixed benchmark specification".into());
        }
        Ok(Self {
            side,
            leaf_width: 32 + width_variant as i32,
        })
    }

    pub fn viewport(&self) -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: self.side as i32 * self.leaf_width + (self.side as i32 - 1) * 4,
            height: self.side as i32 * 24 + (self.side as i32 - 1) * 4,
        }
    }

    pub fn count(&self) -> usize {
        self.side * self.side
    }

    pub fn rectangle(&self, index: usize) -> [i32; 4] {
        [
            (index % self.side) as i32 * (self.leaf_width + 4),
            (index / self.side) as i32 * 28,
            self.leaf_width,
            24,
        ]
    }

    fn expected_pixel(&self, x: i32, y: i32) -> [u8; 4] {
        let column = x / (self.leaf_width + 4);
        let row = y / 28;
        if x % (self.leaf_width + 4) >= self.leaf_width || y % 28 >= 24 {
            return [0, 0, 0, 255];
        }
        if (row as usize * self.side + column as usize) % 2 == 0 {
            [255, 0, 0, 255]
        } else {
            [0, 0, 255, 255]
        }
    }

    /// This audit belongs outside the timed region.
    pub fn verify_scene(&self, actual: &gpui::Scene) -> Result<()> {
        if actual.quads.len() != self.count()
            || !actual.paths.is_empty()
            || !actual.shadows.is_empty()
            || !actual.underlines.is_empty()
            || !actual.monochrome_sprites.is_empty()
            || !actual.subpixel_sprites.is_empty()
            || !actual.polychrome_sprites.is_empty()
            || !actual.surfaces.is_empty()
        {
            return Err("unexpected primitive count or primitive kind".into());
        }
        let mut oracle = gpui::Scene::default();
        for index in 0..self.count() {
            oracle.insert_primitive(solid_quad(self.rectangle(index), index));
        }
        oracle.finish();
        for (index, (a, e)) in actual.quads.iter().zip(&oracle.quads).enumerate() {
            if a.order != e.order
                || a.bounds != e.bounds
                || a.content_mask.bounds != e.content_mask.bounds
                || a.background != e.background
                || a.border_style != e.border_style
                || a.border_color != e.border_color
                || a.corner_radii != e.corner_radii
                || a.border_widths != e.border_widths
            {
                return Err(format!(
                    "quad {index} differs from the independent grid oracle"
                ));
            }
        }
        Ok(())
    }

    pub fn verify_image(&self, image: &image::RgbaImage) -> Result<()> {
        let viewport = self.viewport();
        if image.dimensions() != (viewport.width as u32, viewport.height as u32) {
            return Err("readback dimensions differ from the specified viewport".into());
        }
        for y in 0..viewport.height {
            for x in 0..viewport.width {
                let expected = self.expected_pixel(x, y);
                let actual = image.get_pixel(x as u32, y as u32).0;
                if actual.iter().zip(expected).any(|(a, e)| a.abs_diff(e) > 3) {
                    return Err(format!(
                        "pixel ({x}, {y}) differs from oracle: {actual:?} != {expected:?}"
                    ));
                }
            }
        }
        Ok(())
    }
}

fn bounds(rectangle: [i32; 4]) -> Bounds<ScaledPixels> {
    Bounds {
        origin: Point {
            x: (rectangle[0] as f32).into(),
            y: (rectangle[1] as f32).into(),
        },
        size: Size {
            width: (rectangle[2] as f32).into(),
            height: (rectangle[3] as f32).into(),
        },
    }
}

fn solid_quad(rectangle: [i32; 4], index: usize) -> Quad {
    let bounds = bounds(rectangle);
    let color: Hsla = if index % 2 == 0 {
        gpui::red()
    } else {
        gpui::blue()
    };
    Quad {
        order: 0,
        border_style: BorderStyle::Solid,
        bounds,
        content_mask: ContentMask { bounds },
        background: color.into(),
        border_color: color,
        corner_radii: Corners::default(),
        border_widths: Edges::default(),
    }
}

/// Common scene sink. begin/draw/end include real GPUI scene insertion and finish.
pub struct QuadSink {
    pub scene: gpui::Scene,
    viewport: Rect,
    inserted: usize,
}

impl QuadSink {
    pub fn new(viewport: Rect) -> Self {
        Self {
            scene: gpui::Scene::default(),
            viewport,
            inserted: 0,
        }
    }

    fn push_rectangle(&mut self, rectangle: [i32; 4]) {
        self.scene
            .insert_primitive(solid_quad(rectangle, self.inserted));
        self.inserted += 1;
    }

    /// Retains scene vector capacity, just as the zgpui adapter does.
    /// The caller obtains these rectangles from GPUI's production Taffy engine.
    pub fn from_gpui_rectangles(&mut self, rectangles: &[[i32; 4]]) -> Result<()> {
        self.begin(self.viewport)?;
        for rectangle in rectangles {
            self.push_rectangle(*rectangle);
        }
        self.end()
    }

    pub fn from_zgpui_layout(&mut self, layout: &Layout, scene: &zgpui::Scene) -> Result<()> {
        layout
            .paint(scene, self)
            .map_err(|error| format!("{error:?}"))
    }
}

impl Renderer for QuadSink {
    type Error = String;

    fn begin(&mut self, viewport: Rect) -> Result<()> {
        self.scene.clear();
        self.inserted = 0;
        if viewport != self.viewport {
            return Err("frontend viewport differs from the fixed renderer viewport".into());
        }
        Ok(())
    }

    fn draw(&mut self, node: &Node, rect: Rect) -> Result<()> {
        match &node.kind {
            Kind::Button {
                label,
                enabled: true,
                ..
            } if label.is_empty() => {}
            _ => return Err("headless quad fixture requires enabled empty-label buttons".into()),
        }
        self.push_rectangle([rect.x, rect.y, rect.width, rect.height]);
        Ok(())
    }

    fn end(&mut self) -> Result<()> {
        self.scene.finish();
        Ok(())
    }

    fn abort(&mut self) {
        self.scene.clear();
        self.inserted = 0;
    }
}

#[derive(Debug, Serialize)]
pub struct AdapterMetadata {
    pub name: String,
    pub vendor: u32,
    pub device: u32,
    pub device_type: String,
    pub backend: String,
    pub driver: String,
    pub driver_info: String,
    pub target_format: String,
    pub hardware_type: bool,
}

pub struct CompletedFrame {
    pub image: image::RgbaImage,
    /// Includes rendering, submission wait, readback, row unpadding and RGBA creation.
    pub completed_render_and_readback_ns: u128,
}

/// A single shared renderer keeps both frontends on one actual device.
pub struct SharedRenderer {
    renderer: WgpuHeadlessRenderer,
    pub adapter: AdapterMetadata,
}

impl SharedRenderer {
    pub fn new(require_hardware: bool) -> Result<Self> {
        let renderer = WgpuHeadlessRenderer::new().map_err(|e| e.to_string())?;
        let info = renderer.benchmark_adapter_info();
        let hardware_type = matches!(
            info.device_type,
            wgpu::DeviceType::DiscreteGpu | wgpu::DeviceType::IntegratedGpu
        );
        if require_hardware && !hardware_type {
            return Err(format!(
                "hardware GPU gate rejected {:?} ({:?}); do not report this as hardware rendering",
                info.name, info.device_type
            ));
        }
        let adapter = AdapterMetadata {
            name: info.name,
            vendor: info.vendor,
            device: info.device,
            device_type: format!("{:?}", info.device_type),
            backend: format!("{:?}", info.backend),
            driver: info.driver,
            driver_info: info.driver_info,
            target_format: format!("{:?}", renderer.benchmark_target_format()),
            hardware_type,
        };
        Ok(Self { renderer, adapter })
    }

    /// The caller validates its scene before entering this measured call.
    /// Image comparison, checksum and output must be performed after this returns.
    pub fn capture(&mut self, scene: &gpui::Scene, oracle: &GridOracle) -> Result<CompletedFrame> {
        let viewport = oracle.viewport();
        let size = Size {
            width: DevicePixels(viewport.width),
            height: DevicePixels(viewport.height),
        };
        let start = Instant::now();
        let image = self
            .renderer
            .render_scene_to_image(scene, size)
            .map_err(|e| e.to_string())?;
        let completed_render_and_readback_ns = start.elapsed().as_nanos();
        Ok(CompletedFrame {
            image,
            completed_render_and_readback_ns,
        })
    }

    /// A separate, untimed correctness phase; neither frontend is its peer's oracle.
    pub fn verify_pair(
        &mut self,
        first: &gpui::Scene,
        second: &gpui::Scene,
        oracle: &GridOracle,
    ) -> Result<()> {
        oracle.verify_scene(first)?;
        oracle.verify_scene(second)?;
        let a = self.capture(first, oracle)?;
        let b = self.capture(second, oracle)?;
        oracle.verify_image(&a.image)?;
        oracle.verify_image(&b.image)?;
        if a.image.as_raw() != b.image.as_raw() {
            return Err(
                "identical scenes produced non-identical readbacks on the shared device".into(),
            );
        }
        Ok(())
    }
}
