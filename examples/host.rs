use zgpui::{
    Kind, Node, Scene, State,
    layout::{Layout, Metrics, Rect, Renderer},
};

#[derive(Default)]
struct Host {
    drawn: usize,
}
impl Renderer for Host {
    type Error = core::convert::Infallible;
    fn begin(&mut self, _: Rect) -> Result<(), Self::Error> {
        self.drawn = 0;
        Ok(())
    }
    fn draw(&mut self, _: &Node, _: Rect) -> Result<(), Self::Error> {
        self.drawn += 1;
        Ok(())
    }
    fn end(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn abort(&mut self) {
        self.drawn = 0;
    }
}

fn main() {
    let mut state = State::new();
    state
        .replace_scene(Scene {
            surface: 1,
            revision: 1,
            root: 1,
            events: true,
            nodes: vec![Node {
                id: 1,
                kind: Kind::Button {
                    label: "Run".into(),
                    action: "run".into(),
                    enabled: true,
                },
            }],
        })
        .unwrap();
    let scene = state.scene().unwrap();
    let layout = Layout::new(
        scene,
        Rect {
            x: 0,
            y: 0,
            width: 320,
            height: 40,
        },
        Metrics::default(),
    )
    .unwrap();
    let mut host = Host::default();
    layout.paint(scene, &mut host).unwrap();
    let node = layout.button_at(scene, 10, 10).unwrap();
    let action = state.action(node, layout.surface, layout.revision).unwrap();
    println!("drawn={} action={}", host.drawn, action.action);
}
