//! Pixel layout shared by native, embedded and offscreen hosts.
use crate::{Error, Kind, MAX_DEPTH, Node, Scene, validate_scene};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn contains(self, x: i32, y: i32) -> bool {
        self.width > 0
            && self.height > 0
            && x >= self.x
            && y >= self.y
            && i64::from(x) < i64::from(self.x) + i64::from(self.width)
            && i64::from(y) < i64::from(self.y) + i64::from(self.height)
    }
}

/// Hosts choose the logical image unit and text line height in pixels.
/// These metrics do not assume a terminal, font API, operating system or GPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metrics {
    pub line_height: i32,
    pub column_gap: i32,
    pub row_gap: i32,
    pub button_extra: i32,
    pub progress_extra: i32,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            line_height: 18,
            column_gap: 10,
            row_gap: 12,
            button_extra: 16,
            progress_extra: 14,
        }
    }
}

impl Metrics {
    fn valid(self) -> bool {
        (1..=65536).contains(&self.line_height)
            && [
                self.column_gap,
                self.row_gap,
                self.button_extra,
                self.progress_extra,
            ]
            .into_iter()
            .all(|value| (0..=65536).contains(&value))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item {
    pub index: usize,
    pub rect: Rect,
}

/// A retained geometry snapshot tied to one scene revision.
#[derive(Clone, Debug)]
pub struct Layout {
    pub surface: u64,
    pub revision: u64,
    pub bounds: Rect,
    pub items: Vec<Item>,
    button_height: i32,
}

fn measure(scene: &Scene, id: u32, metrics: Metrics, depth: usize) -> i32 {
    if depth > MAX_DEPTH {
        return 0;
    }
    let Some(node) = scene.node(id) else { return 0 };
    match &node.kind {
        Kind::Column { children } => {
            children
                .iter()
                .enumerate()
                .fold(0i32, |sum, (position, id)| {
                    sum.saturating_add(if position == 0 { 0 } else { metrics.column_gap })
                        .saturating_add(measure(scene, *id, metrics, depth + 1))
                })
        }
        Kind::Row { children } => children
            .iter()
            .map(|id| measure(scene, *id, metrics, depth + 1))
            .max()
            .unwrap_or(0),
        Kind::Text { text, .. } => {
            (text.split('\n').count().min(32) as i32).saturating_mul(metrics.line_height)
        }
        Kind::Progress { .. } => metrics.line_height.saturating_add(metrics.progress_extra),
        Kind::Button { .. } => metrics.line_height.saturating_add(metrics.button_extra),
        Kind::Image { rows, .. } => i32::from(*rows).saturating_mul(metrics.line_height),
    }
}

/// Natural content height before clipping. The host supplies its own outer chrome.
pub fn natural_height(scene: &Scene, metrics: Metrics) -> Result<i32, Error> {
    validate_scene(scene)?;
    if !metrics.valid() {
        return Err(Error::Value);
    }
    Ok(measure(scene, scene.root, metrics, 1))
}

fn place(scene: &Scene, id: u32, rect: Rect, metrics: Metrics, depth: usize, out: &mut Vec<Item>) {
    if depth > MAX_DEPTH || rect.width <= 0 || rect.height <= 0 {
        return;
    }
    let Ok(index) = scene.nodes.binary_search_by_key(&id, |node| node.id) else {
        return;
    };
    match &scene.nodes[index].kind {
        Kind::Column { children } => {
            let mut used = 0i32;
            for id in children {
                let remaining = rect.height.saturating_sub(used);
                let height = measure(scene, *id, metrics, depth + 1).min(remaining);
                if remaining <= 0 {
                    break;
                }
                if height <= 0 {
                    used = used.saturating_add(metrics.column_gap);
                    continue;
                }
                place(
                    scene,
                    *id,
                    Rect {
                        x: rect.x,
                        y: rect.y.saturating_add(used),
                        width: rect.width,
                        height,
                    },
                    metrics,
                    depth + 1,
                    out,
                );
                used = used
                    .saturating_add(height)
                    .saturating_add(metrics.column_gap);
            }
        }
        Kind::Row { children } => {
            let count = children.len() as i64;
            if count == 0 {
                return;
            }
            let available =
                (i64::from(rect.width) - i64::from(metrics.row_gap) * (count - 1)).max(0);
            for (column, id) in children.iter().enumerate() {
                let column = column as i64;
                let left = available * column / count;
                let right = available * (column + 1) / count;
                let x = i64::from(rect.x) + left + column * i64::from(metrics.row_gap);
                if x > i64::from(i32::MAX) {
                    break;
                }
                place(
                    scene,
                    *id,
                    Rect {
                        x: x as i32,
                        y: rect.y,
                        width: (right - left) as i32,
                        height: rect.height,
                    },
                    metrics,
                    depth + 1,
                    out,
                );
            }
        }
        _ => out.push(Item { index, rect }),
    }
}

impl Layout {
    pub fn new(scene: &Scene, bounds: Rect, metrics: Metrics) -> Result<Self, Error> {
        validate_scene(scene)?;
        if !metrics.valid()
            || bounds.width < 0
            || bounds.height < 0
            || i64::from(bounds.x) + i64::from(bounds.width) > i64::from(i32::MAX)
            || i64::from(bounds.y) + i64::from(bounds.height) > i64::from(i32::MAX)
        {
            return Err(Error::Value);
        }
        let mut items = Vec::with_capacity(scene.nodes.len());
        place(scene, scene.root, bounds, metrics, 1, &mut items);
        Ok(Self {
            surface: scene.surface,
            revision: scene.revision,
            bounds,
            items,
            button_height: metrics.line_height.saturating_add(metrics.button_extra),
        })
    }

    /// Identity check within the same owning State/host namespace, not cross-host authentication.
    pub fn matches(&self, scene: &Scene) -> bool {
        self.surface == scene.surface && self.revision == scene.revision
    }

    pub fn button_at(&self, scene: &Scene, x: i32, y: i32) -> Option<u32> {
        if !self.matches(scene) {
            return None;
        }
        self.items.iter().find_map(|item| {
            let node = scene.nodes.get(item.index)?;
            (item.rect.height >= self.button_height
                && item.rect.contains(x, y)
                && matches!(node.kind, Kind::Button { enabled: true, .. }))
            .then_some(node.id)
        })
    }

    /// A renderer receives data only. An incomplete frame is explicitly aborted.
    pub fn paint<R: Renderer>(
        &self,
        scene: &Scene,
        renderer: &mut R,
    ) -> Result<(), PaintError<R::Error>> {
        if !self.matches(scene) {
            return Err(PaintError::Stale);
        }
        if let Err(error) = renderer.begin(self.bounds) {
            renderer.abort();
            return Err(PaintError::Renderer(error));
        }
        for item in &self.items {
            let Some(node) = scene.nodes.get(item.index) else {
                renderer.abort();
                return Err(PaintError::Stale);
            };
            if let Err(error) = renderer.draw(node, item.rect) {
                renderer.abort();
                return Err(PaintError::Renderer(error));
            }
        }
        if let Err(error) = renderer.end() {
            renderer.abort();
            return Err(PaintError::Renderer(error));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintError<E> {
    Stale,
    Renderer(E),
}

/// Backend boundary for software, GPU, native or test renderers.
/// The host owns windows, fonts, images, input, scheduling and system calls.
pub trait Renderer {
    type Error;
    fn begin(&mut self, bounds: Rect) -> Result<(), Self::Error>;
    fn draw(&mut self, node: &Node, rect: Rect) -> Result<(), Self::Error>;
    fn end(&mut self) -> Result<(), Self::Error>;
    fn abort(&mut self);
}
