use super::*;
use crate::layout::{Layout, Metrics, PaintError, Rect, Renderer};
use alloc::{string::ToString, vec};

fn scene() -> Scene {
    Scene {
        surface: 7,
        revision: 1,
        root: 1,
        events: true,
        nodes: vec![
            Node {
                id: 1,
                kind: Kind::Row {
                    children: vec![2, 3],
                },
            },
            Node {
                id: 2,
                kind: Kind::Button {
                    label: "Run".into(),
                    action: "run".into(),
                    enabled: true,
                },
            },
            Node {
                id: 3,
                kind: Kind::Button {
                    label: "Stop".into(),
                    action: "stop".into(),
                    enabled: false,
                },
            },
        ],
    }
}
fn bounds() -> Rect {
    Rect {
        x: 10,
        y: 20,
        width: 212,
        height: 40,
    }
}

#[test]
fn typed_scene_matches_protocol_and_failed_update_is_atomic() {
    let mut state = State::new();
    state.replace_scene(scene()).unwrap();
    assert!(state.action(2, 7, 1).is_some());
    let before = state.scene().unwrap().clone();
    let mut invalid = scene();
    invalid.revision = 2;
    invalid.nodes[1].kind = Kind::Button {
        label: "Run".into(),
        action: "shell command".into(),
        enabled: true,
    };
    assert_eq!(state.replace_scene(invalid), Err(Error::Value));
    assert_eq!(state.scene(), Some(&before));
    assert_eq!(state.last_revision(), 1);
}

#[test]
fn typed_scene_sorts_ids_and_checks_values() {
    let mut value = scene();
    value.nodes.swap(0, 2);
    let mut state = State::new();
    state.replace_scene(value).unwrap();
    assert_eq!(state.scene().unwrap().nodes[0].id, 1);
    let mut value = scene();
    value.revision = 2;
    value.nodes[1].kind = Kind::Progress {
        value: 10001,
        label: String::new(),
    };
    assert_eq!(state.replace_scene(value), Err(Error::Value));
}

#[test]
fn typed_scene_checks_strings_images_and_graphs() {
    let variants = [
        Kind::Text {
            text: "\u{1b}escape".into(),
            style: TextStyle::Body,
        },
        Kind::Text {
            text: "x".repeat(MAX_STRING + 1),
            style: TextStyle::Body,
        },
        Kind::Image {
            image_id: 0,
            generation: None,
            alt: "".into(),
            cols: 1,
            rows: 1,
        },
        Kind::Image {
            image_id: 1,
            generation: None,
            alt: "".into(),
            cols: 0,
            rows: 1,
        },
    ];
    for kind in variants {
        let mut value = scene();
        value.nodes[1].kind = kind;
        assert!(State::new().replace_scene(value).is_err());
    }
    let mut value = scene();
    value.nodes[0].kind = Kind::Row {
        children: vec![1, 3],
    };
    assert_eq!(State::new().replace_scene(value), Err(Error::Graph));
}

#[test]
fn portable_layout_uses_host_pixels_and_disabled_hit_is_ignored() {
    let value = scene();
    let layout = Layout::new(&value, bounds(), Metrics::default()).unwrap();
    assert_eq!(
        layout.items[0].rect,
        Rect {
            x: 10,
            y: 20,
            width: 100,
            height: 40
        }
    );
    assert_eq!(
        layout.items[1].rect,
        Rect {
            x: 122,
            y: 20,
            width: 100,
            height: 40
        }
    );
    assert_eq!(layout.button_at(&value, 20, 30), Some(2));
    assert_eq!(layout.button_at(&value, 150, 30), None);
    let mut replacement = value.clone();
    replacement.revision = 2;
    assert_eq!(layout.button_at(&replacement, 20, 30), None);
}

#[derive(Default)]
struct Probe {
    begun: usize,
    draws: usize,
    ended: usize,
    aborted: usize,
    fail: bool,
}
impl Renderer for Probe {
    type Error = u8;
    fn begin(&mut self, _: Rect) -> Result<(), u8> {
        self.begun += 1;
        Ok(())
    }
    fn draw(&mut self, _: &Node, _: Rect) -> Result<(), u8> {
        self.draws += 1;
        if self.fail { Err(7) } else { Ok(()) }
    }
    fn end(&mut self) -> Result<(), u8> {
        self.ended += 1;
        Ok(())
    }
    fn abort(&mut self) {
        self.aborted += 1;
    }
}

#[test]
fn renderer_failure_aborts_without_committing_partial_frame() {
    let value = scene();
    let layout = Layout::new(&value, bounds(), Metrics::default()).unwrap();
    let mut host = Probe {
        fail: true,
        ..Probe::default()
    };
    assert_eq!(
        layout.paint(&value, &mut host),
        Err(PaintError::Renderer(7))
    );
    assert_eq!(
        (host.begun, host.draws, host.ended, host.aborted),
        (1, 1, 0, 1)
    );
    let mut next = value;
    next.revision = 2;
    assert_eq!(layout.paint(&next, &mut host), Err(PaintError::Stale));
    assert_eq!(host.begun, 1);
}

#[test]
fn geometry_checks_overflow_and_invalid_metrics() {
    let value = scene();
    let metrics = Metrics {
        line_height: 0,
        ..Metrics::default()
    };
    assert!(Layout::new(&value, bounds(), metrics).is_err());
    assert!(
        Layout::new(
            &value,
            Rect {
                x: i32::MAX,
                width: 10,
                ..bounds()
            },
            Metrics::default()
        )
        .is_err()
    );
    assert!(
        !Rect {
            x: i32::MAX - 2,
            y: 0,
            width: 10,
            height: 1
        }
        .contains(i32::MIN, 0)
    );
}

#[test]
fn reset_rejects_replayed_typed_scene_and_preserves_revision_floor() {
    let mut state = State::new();
    state.replace_scene(scene()).unwrap();
    state.reset();
    assert_eq!(state.replace_scene(scene()), Err(Error::Revision));
    assert!(state.scene().is_none());
}

#[test]
fn typed_strings_accept_line_breaks_and_tabs() {
    let mut value = scene();
    value.nodes[1].kind = Kind::Text {
        text: "line\n\t第二行".to_string(),
        style: TextStyle::Body,
    };
    let mut state = State::new();
    state.replace_scene(value).unwrap();
    assert!(layout::natural_height(state.scene().unwrap(), Metrics::default()).unwrap() > 0);
}

#[test]
fn empty_container_does_not_hide_later_siblings() {
    let mut value = scene();
    value.nodes[0].kind = Kind::Column {
        children: vec![2, 3],
    };
    value.nodes[1].kind = Kind::Row { children: vec![] };
    value.nodes[2].kind = Kind::Text {
        text: "visible".into(),
        style: TextStyle::Body,
    };
    let layout = Layout::new(&value, bounds(), Metrics::default()).unwrap();
    assert_eq!(layout.items.len(), 1);
    assert_eq!(layout.items[0].index, 2);
    assert_eq!(
        layout.items[0].rect.y,
        bounds().y + Metrics::default().column_gap
    );
}

#[test]
fn clipped_button_below_full_height_cannot_activate() {
    let value = scene();
    let layout = Layout::new(
        &value,
        Rect {
            height: 1,
            ..bounds()
        },
        Metrics::default(),
    )
    .unwrap();
    assert_eq!(layout.items[0].rect.height, 1);
    assert_eq!(layout.button_at(&value, 20, 20), None);
}
