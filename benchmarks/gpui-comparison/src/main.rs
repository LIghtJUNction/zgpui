use gpui::{
    App, AvailableSpace, Bounds, Display, FlexDirection, LayoutId, Length, Pixels, Style,
    TaffyLayoutEngine, TestAppContext, Window, point, px, relative, size,
};
use serde::Serialize;
use std::{hint::black_box, time::Instant};
use zgpui::{
    Kind, Node, Scene,
    layout::{Layout, Metrics, Rect},
};

#[cfg(feature = "allocation-metrics")]
mod allocations;

#[cfg(feature = "allocation-metrics")]
#[global_allocator]
static ALLOCATOR: allocations::Counter = allocations::Counter;

const GAP: i32 = 4;
const HEIGHT: i32 = 24;

#[derive(Clone, Copy)]
struct Case {
    rows: usize,
    columns: usize,
}

impl Case {
    fn width(self, variant: usize) -> i32 {
        self.columns as i32 * (32 + variant as i32) + (self.columns as i32 - 1) * GAP
    }
    fn height(self) -> i32 {
        self.rows as i32 * HEIGHT + (self.rows as i32 - 1) * GAP
    }
    fn expected(self, variant: usize) -> Vec<[i32; 4]> {
        (0..self.rows)
            .flat_map(|row| {
                (0..self.columns).map(move |column| {
                    [
                        column as i32 * (32 + variant as i32 + GAP),
                        row as i32 * (HEIGHT + GAP),
                        32 + variant as i32,
                        HEIGHT,
                    ]
                })
            })
            .collect()
    }
}

fn z_scene(case: Case) -> Scene {
    let mut nodes = Vec::with_capacity(1 + case.rows + case.rows * case.columns);
    nodes.push(Node {
        id: 1,
        kind: Kind::Column {
            children: (0..case.rows).map(|row| row as u32 + 2).collect(),
        },
    });
    let first_leaf = case.rows as u32 + 2;
    for row in 0..case.rows {
        nodes.push(Node {
            id: row as u32 + 2,
            kind: Kind::Row {
                children: (0..case.columns)
                    .map(|column| first_leaf + (row * case.columns + column) as u32)
                    .collect(),
            },
        });
    }
    for leaf in 0..case.rows * case.columns {
        nodes.push(Node {
            id: first_leaf + leaf as u32,
            kind: Kind::Button {
                label: String::new(),
                action: "press".to_string(),
                enabled: true,
            },
        });
    }
    Scene {
        surface: 1,
        revision: 1,
        root: 1,
        nodes,
        events: true,
    }
}

fn z_layout(scene: &Scene, case: Case, variant: usize) -> Layout {
    Layout::new(
        scene,
        Rect {
            x: 0,
            y: 0,
            width: case.width(variant),
            height: case.height(),
        },
        Metrics {
            line_height: 16,
            column_gap: GAP,
            row_gap: GAP,
            button_extra: 8,
            progress_extra: 8,
        },
    )
    .expect("valid bounded fixture")
}

fn z_rectangles(layout: &Layout, result: &mut Vec<[i32; 4]>) {
    result.clear();
    result.extend(
        layout
            .items
            .iter()
            .map(|item| [item.rect.x, item.rect.y, item.rect.width, item.rect.height]),
    );
}

struct GFixture {
    engine: TaffyLayoutEngine,
    root: LayoutId,
    leaves: Vec<LayoutId>,
    case: Case,
}

impl GFixture {
    fn new(case: Case) -> Self {
        let mut engine = TaffyLayoutEngine::new();
        let mut leaves = Vec::with_capacity(case.rows * case.columns);
        let mut rows = Vec::with_capacity(case.rows);
        for _ in 0..case.rows {
            let start = leaves.len();
            for _ in 0..case.columns {
                let style = Style {
                    size: size(Length::Auto, px(HEIGHT as f32).into()),
                    min_size: size(px(0.).into(), px(HEIGHT as f32).into()),
                    flex_basis: px(0.).into(),
                    flex_grow: 1.,
                    flex_shrink: 1.,
                    ..Default::default()
                };
                leaves.push(engine.request_layout(style, px(16.), 1., &[]));
            }
            let style = Style {
                display: Display::Flex,
                size: size(Length::Auto, px(HEIGHT as f32).into()),
                flex_direction: FlexDirection::Row,
                flex_shrink: 0.,
                gap: size(px(GAP as f32).into(), px(0.).into()),
                ..Default::default()
            };
            rows.push(engine.request_layout(style, px(16.), 1., &leaves[start..]));
        }
        let style = Style {
            display: Display::Flex,
            size: size(relative(1.).into(), px(case.height() as f32).into()),
            flex_direction: FlexDirection::Column,
            gap: size(px(0.).into(), px(GAP as f32).into()),
            ..Default::default()
        };
        let root = engine.request_layout(style, px(16.), 1., &rows);
        Self {
            engine,
            root,
            leaves,
            case,
        }
    }

    fn layout(
        &mut self,
        variant: usize,
        window: &mut Window,
        app: &mut App,
        result: &mut Vec<[i32; 4]>,
    ) {
        self.engine.compute_layout(
            self.root,
            size(
                AvailableSpace::Definite(px(self.case.width(variant) as f32)),
                AvailableSpace::Definite(px(self.case.height() as f32)),
            ),
            window,
            app,
        );
        result.clear();
        result.extend(self.leaves.iter().map(|id| {
            let bounds = self.engine.layout_bounds(*id, 1.);
            [
                f32::from(bounds.origin.x) as i32,
                f32::from(bounds.origin.y) as i32,
                f32::from(bounds.size.width) as i32,
                f32::from(bounds.size.height) as i32,
            ]
        }));
    }
}

#[derive(Serialize)]
struct Measurements {
    backend: String,
    workload: String,
    rows: usize,
    columns: usize,
    nodes: usize,
    samples_ns: Vec<u64>,
    checksum: u64,
    fixture_setup_ns: u64,
    warmup: usize,
    batch_thread_cpu_ns: Option<u64>,
    batch_runqueue_wait_ns: Option<u64>,
    batch_scheduling_slices: Option<u64>,
    #[cfg(feature = "allocation-metrics")]
    allocation_totals: allocations::Stats,
}

fn thread_runtime() -> Option<[u64; 3]> {
    let source = std::fs::read_to_string("/proc/thread-self/schedstat").ok()?;
    let mut values = source.split_whitespace();
    Some([
        values.next()?.parse().ok()?,
        values.next()?.parse().ok()?,
        values.next()?.parse().ok()?,
    ])
}

fn sample(
    backend: &str,
    workload: &str,
    case: Case,
    samples: usize,
    setup: u64,
    mut operation: impl FnMut(usize) -> u64,
) -> Measurements {
    let warmup = 256;
    let mut checksum = 0u64;
    for iteration in 0..warmup {
        checksum = checksum.wrapping_add(black_box(operation(black_box(iteration))));
    }
    let mut times = Vec::with_capacity(samples);
    let runtime_start = thread_runtime();
    #[cfg(feature = "allocation-metrics")]
    allocations::start();
    for iteration in 0..samples {
        let start = Instant::now();
        let value = black_box(operation(black_box(iteration)));
        let elapsed = start.elapsed().as_nanos() as u64;
        checksum = checksum.wrapping_add(value);
        times.push(elapsed);
    }
    #[cfg(feature = "allocation-metrics")]
    let allocation_totals = allocations::finish();
    let runtime = runtime_start.zip(thread_runtime()).map(|(before, after)| {
        [
            after[0].saturating_sub(before[0]),
            after[1].saturating_sub(before[1]),
            after[2].saturating_sub(before[2]),
        ]
    });
    Measurements {
        backend: backend.into(),
        workload: workload.into(),
        rows: case.rows,
        columns: case.columns,
        nodes: 1 + case.rows + case.rows * case.columns,
        samples_ns: times,
        checksum,
        fixture_setup_ns: setup,
        warmup,
        batch_thread_cpu_ns: runtime.map(|values| values[0]),
        batch_runqueue_wait_ns: runtime.map(|values| values[1]),
        batch_scheduling_slices: runtime.map(|values| values[2]),
        #[cfg(feature = "allocation-metrics")]
        allocation_totals,
    }
}

fn checksum(rectangles: &[[i32; 4]]) -> u64 {
    rectangles
        .iter()
        .flatten()
        .fold(0u64, |sum, value| sum.wrapping_add(*value as u64))
}

fn gpui_geometry_scan(bounds: &[Bounds<Pixels>], x: i32, y: i32) -> u64 {
    let point = point(px(x as f32), px(y as f32));
    bounds
        .iter()
        .position(|bounds| {
            bounds.contains(&point)
                && point.x < bounds.origin.x + bounds.size.width
                && point.y < bounds.origin.y + bounds.size.height
        })
        .map_or(0, |index| index as u64 + 1)
}

fn verify(case: Case) {
    let scene = z_scene(case);
    let mut app = TestAppContext::single();
    let window = app.add_empty_window();
    window.update(|window, app| {
        window.set_scale_factor(1.);
        let mut gpui = GFixture::new(case);
        for variant in [0, 1, 0, 2, 1] {
            let expected = case.expected(variant);
            let mut zgpui_rectangles = Vec::new();
            z_rectangles(&z_layout(&scene, case, variant), &mut zgpui_rectangles);
            let mut gpui_rectangles = Vec::new();
            gpui.layout(variant, window, app, &mut gpui_rectangles);
            assert_eq!(
                zgpui_rectangles, expected,
                "zgpui must match the independently derived geometry"
            );
            assert_eq!(
                gpui_rectangles, expected,
                "GPUI must match the independently derived geometry"
            );
            let layout = z_layout(&scene, case, variant);
            let bounds: Vec<Bounds<Pixels>> = expected
                .iter()
                .map(|rect| {
                    Bounds::new(
                        point(px(rect[0] as f32), px(rect[1] as f32)),
                        size(px(rect[2] as f32), px(rect[3] as f32)),
                    )
                })
                .collect();
            for iteration in 0..2048 {
                let x = ((iteration * 17) % (case.width(variant) as usize + 8)) as i32;
                let y = ((iteration * 29) % (case.height() as usize + 8)) as i32;
                let oracle = expected
                    .iter()
                    .position(|rect| {
                        x >= rect[0]
                            && x < rect[0] + rect[2]
                            && y >= rect[1]
                            && y < rect[1] + rect[3]
                    })
                    .map_or(0, |index| index as u64 + 1);
                let z_hit = layout
                    .button_at(&scene, x, y)
                    .map_or(0, |id| (id - case.rows as u32 - 1) as u64);
                assert_eq!(z_hit, oracle);
                assert_eq!(gpui_geometry_scan(&bounds, x, y), oracle);
            }
        }
    });
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.get(1).map(String::as_str) == Some("verify") {
        for side in [1, 4, 8, 15] {
            verify(Case {
                rows: side,
                columns: side,
            });
        }
        println!(
            "{}",
            serde_json::json!({"verified": true, "cases": 20, "scope": "every fixed-metric leaf rectangle; no GPU rendering"})
        );
        return Ok(());
    }
    if arguments.len() != 5 {
        return Err(
            "usage: comparison <zgpui|gpui> <fresh|retained|resize|geometry-scan> <side> <samples>"
                .into(),
        );
    }
    let backend = &arguments[1];
    let workload = &arguments[2];
    if !["zgpui", "gpui"].contains(&backend.as_str())
        || !["fresh", "retained", "resize", "geometry-scan"].contains(&workload.as_str())
    {
        return Err("unknown backend or workload".into());
    }
    let side: usize = arguments[3].parse()?;
    let samples: usize = arguments[4].parse()?;
    if ![1, 4, 8, 15].contains(&side) || samples == 0 {
        return Err("invalid bounded fixture or sample count".into());
    }
    let case = Case {
        rows: side,
        columns: side,
    };
    let start = Instant::now();
    let mut rectangles = Vec::with_capacity(side * side);
    let measured = if backend == "zgpui" {
        let scene = z_scene(case);
        let layout = z_layout(&scene, case, 0);
        let setup = start.elapsed().as_nanos() as u64;
        sample(backend, workload, case, samples, setup, |iteration| {
            if workload == "geometry-scan" {
                let x = ((iteration * 17) % (case.width(0) as usize + 8)) as i32;
                let y = ((iteration * 29) % (case.height() as usize + 8)) as i32;
                return layout
                    .button_at(&scene, x, y)
                    .map_or(0, |id| (id - case.rows as u32 - 1) as u64);
            }
            let variant = if workload == "resize" {
                iteration % 2
            } else {
                0
            };
            let output = if workload == "fresh" {
                z_layout(&z_scene(black_box(case)), case, black_box(variant))
            } else {
                z_layout(black_box(&scene), case, black_box(variant))
            };
            z_rectangles(&output, &mut rectangles);
            checksum(black_box(&rectangles))
        })
    } else {
        let mut app = TestAppContext::single();
        let window = app.add_empty_window();
        window.update(|window, app| {
            window.set_scale_factor(1.);
            let mut fixture = GFixture::new(case);
            fixture.layout(0, window, app, &mut rectangles);
            let hit_bounds: Vec<Bounds<Pixels>> = rectangles
                .iter()
                .map(|rect| {
                    Bounds::new(
                        point(px(rect[0] as f32), px(rect[1] as f32)),
                        size(px(rect[2] as f32), px(rect[3] as f32)),
                    )
                })
                .collect();
            let setup = start.elapsed().as_nanos() as u64;
            sample(backend, workload, case, samples, setup, |iteration| {
                if workload == "geometry-scan" {
                    let x = ((iteration * 17) % (case.width(0) as usize + 8)) as i32;
                    let y = ((iteration * 29) % (case.height() as usize + 8)) as i32;
                    return gpui_geometry_scan(&hit_bounds, x, y);
                }
                let variant = if workload == "resize" {
                    iteration % 2
                } else {
                    0
                };
                if workload == "fresh" {
                    GFixture::new(black_box(case)).layout(
                        black_box(variant),
                        window,
                        app,
                        &mut rectangles,
                    );
                } else {
                    fixture.layout(black_box(variant), window, app, &mut rectangles);
                }
                checksum(black_box(&rectangles))
            })
        })
    };
    println!("{}", serde_json::to_string(&measured)?);
    Ok(())
}
