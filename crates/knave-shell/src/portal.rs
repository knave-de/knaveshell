//! Native portal selection and sharing controls on Knave's retained UI toolkit.
use base64::{Engine, engine::general_purpose::STANDARD};
use knave_portal_api::{MAX_REQUEST, Operation, Reply, Request, VERSION};
use knave_ui::{Color, DisplayList, ShapePaint, UiImage, toolkit::*};
use knave_wayland::{Application, KeyboardMode, SurfaceLayer, SurfaceOptions};
use std::{
    collections::BTreeSet,
    io::{self, Read},
};
const CANCEL: ElementId = ElementId(2);
const CONFIRM: ElementId = ElementId(3);
const SOURCE: u64 = 100;
pub struct Picker {
    request: Request,
    selected: BTreeSet<u32>,
    images: Vec<Option<UiImage>>,
    scene: Scene,
    dirty: bool,
    closed: bool,
    size: [f32; 2],
}
impl Picker {
    pub fn new(request: Request) -> Result<Self, String> {
        request.validate()?;
        let images = request
            .sources
            .iter()
            .map(|source| {
                let encoded = source.preview_png.as_ref()?;
                let data = STANDARD.decode(encoded).ok()?;
                let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
                decoder.set_limits(png::Limits { bytes: 1024 * 1024 });
                let mut reader = decoder.read_info().ok()?;
                if reader.info().width > 320 || reader.info().height > 180 {
                    return None;
                }
                let mut pixels = vec![0; reader.output_buffer_size()?];
                let info = reader.next_frame(&mut pixels).ok()?;
                if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight
                {
                    return None;
                }
                pixels.truncate(info.buffer_size());
                UiImage::from_rgba(info.width, info.height, pixels)
            })
            .collect();
        Ok(Self {
            request,
            images,
            selected: BTreeSet::new(),
            scene: Scene::new(Element::new(0, Widget::Panel)).map_err(|e| e.to_string())?,
            dirty: true,
            closed: false,
            size: [0.0; 2],
        })
    }
    fn respond(&mut self, selected: Vec<u32>) {
        if self.closed {
            return;
        }
        let reply = Reply {
            version: VERSION,
            request_id: self.request.request_id.clone(),
            selected,
        };
        if let Ok(json) = serde_json::to_string(&reply) {
            println!("{json}");
        }
        self.closed = true;
    }
    fn compose(&self) -> Element {
        let mut root = Element::new(0, Widget::Panel).layout(Layout {
            padding: 24.0,
            gap: 16.0,
            ..Default::default()
        });
        root.style.background = Some(ShapePaint::fill(Color::rgba(24, 31, 43, 255)));
        let title = match self.request.operation {
            Operation::Share => "Choose a screen to share",
            Operation::Screenshot => "Choose a screen to capture",
            Operation::Sharing => "Screen sharing is active",
        };
        root.children
            .push(Element::new(4, Widget::Text(title.into())).layout(Layout {
                height: Length::Px(32.0),
                ..Default::default()
            }));
        let app = if self.request.app_id.is_empty() {
            "An application"
        } else {
            &self.request.app_id
        };
        root.children.push(
            Element::new(
                5,
                Widget::Text(format!(
                    "{app} — {}",
                    if self.request.operation == Operation::Sharing {
                        "can see the selected screens"
                    } else {
                        "will see only what you approve"
                    }
                )),
            )
            .layout(Layout {
                height: Length::Px(48.0),
                ..Default::default()
            }),
        );
        if self.request.operation != Operation::Sharing {
            let mut grid = Element::new(6, Widget::Panel).layout(Layout {
                flow: Flow::Grid { columns: 2 },
                gap: 12.0,
                height: Length::Auto,
                ..Default::default()
            });
            for (index, source) in self.request.sources.iter().enumerate() {
                let selected = self.selected.contains(&source.id);
                let mut card = Element::new(SOURCE + index as u64, Widget::Button(String::new()))
                    .layout(Layout {
                        height: Length::Px(190.0),
                        padding: 10.0,
                        gap: 8.0,
                        ..Default::default()
                    });
                card.style.background = Some(ShapePaint::fill(if selected {
                    Color::rgba(48, 82, 126, 255)
                } else {
                    Color::rgba(38, 49, 65, 255)
                }));
                if let Some(image) = &self.images[index] {
                    card.children.push(
                        Element::new(
                            1000 + index as u64,
                            Widget::Image(image.clone(), Default::default()),
                        )
                        .layout(Layout {
                            height: Length::Px(120.0),
                            ..Default::default()
                        }),
                    );
                }
                card.children.push(
                    Element::new(
                        2000 + index as u64,
                        Widget::Text(format!(
                            "{}{}\n{} × {}",
                            if selected { "✓ " } else { "" },
                            source.name,
                            source.width,
                            source.height
                        )),
                    )
                    .layout(Layout {
                        height: Length::Px(44.0),
                        ..Default::default()
                    }),
                );
                grid.children.push(card);
            }
            root.children
                .push(Element::new(7, Widget::Scroll).children(vec![grid]));
        }
        let mut buttons = Element::new(8, Widget::Panel).layout(Layout {
            flow: Flow::Row,
            height: Length::Px(44.0),
            gap: 12.0,
            ..Default::default()
        });
        if self.request.operation != Operation::Sharing {
            buttons
                .children
                .push(Element::new(CANCEL.0, Widget::Button("Cancel".into())));
        }
        let mut confirm = Element::new(
            CONFIRM.0,
            Widget::Button(
                match self.request.operation {
                    Operation::Share => "Share",
                    Operation::Screenshot => "Capture",
                    Operation::Sharing => "Stop sharing",
                }
                .into(),
            ),
        );
        confirm.enabled = self.request.operation == Operation::Sharing || !self.selected.is_empty();
        buttons.children.push(confirm);
        root.children.push(buttons);
        root
    }
}
impl Application for Picker {
    fn cursor(&self) -> CursorShape {
        self.scene.cursor()
    }
    fn input(&mut self, event: Input) {
        if matches!(
            event,
            Input::Key {
                key: Key::Escape,
                pressed: true,
                repeat: false,
                ..
            }
        ) && self.request.operation != Operation::Sharing
        {
            self.respond(Vec::new());
            return;
        }
        let result = self.scene.event(event);
        if let Some(Action::Activated(id)) = result.action {
            if id == CANCEL {
                self.respond(Vec::new());
            } else if id == CONFIRM {
                if self.request.operation == Operation::Sharing || !self.selected.is_empty() {
                    self.respond(self.selected.iter().copied().collect());
                }
            } else if let Some(source) =
                id.0.checked_sub(SOURCE)
                    .and_then(|i| self.request.sources.get(i as usize))
            {
                let source_id = source.id;
                if !self.selected.remove(&source_id) {
                    if !self.request.multiple {
                        self.selected.clear();
                    }
                    self.selected.insert(source_id);
                }
                self.dirty = true;
            }
        }
    }
    fn needs_frame(&self) -> bool {
        self.dirty || self.scene.needs_frame()
    }
    fn frame(&mut self, size: [f32; 2], text: &mut dyn TextMeasurer) -> &DisplayList {
        if self.dirty || self.size != size {
            let focus = self.scene.focus();
            if let Ok(mut scene) = Scene::new(self.compose()) {
                scene.layout(size, text);
                let _ = scene.set_focus(focus.or(Some(CANCEL)));
                self.scene = scene;
            }
            self.size = size;
            self.dirty = false;
        }
        self.scene.display_list(size, text)
    }
    fn should_close(&self) -> bool {
        self.closed
    }
}
pub fn run() -> Result<(), String> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(MAX_REQUEST as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_REQUEST {
        return Err("picker request exceeds budget".into());
    }
    let request: Request = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let sharing = request.operation == Operation::Sharing;
    knave_wayland::run_application(
        SurfaceOptions {
            size: Some(if sharing { [520, 220] } else { [820, 640] }),
            keyboard: if sharing {
                KeyboardMode::OnDemand
            } else {
                KeyboardMode::Exclusive
            },
            layer: SurfaceLayer::Overlay,
            namespace: if sharing {
                "knave-portal-sharing"
            } else {
                "knave-portal-picker"
            }
            .into(),
        },
        Picker::new(request)?,
    )
    .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Request {
        Request {
            version: VERSION,
            request_id: "test".into(),
            app_id: "app".into(),
            operation: Operation::Share,
            multiple: false,
            sources: vec![knave_portal_api::Source {
                id: 3,
                name: "monitor".into(),
                description: "".into(),
                width: 1920,
                height: 1080,
                preview_png: None,
            }],
        }
    }
    #[test]
    fn share_starts_disabled_and_requires_explicit_selection() {
        let mut picker = Picker::new(request()).unwrap();
        assert!(picker.selected.is_empty());
        let root = picker.compose();
        assert!(
            !root
                .children
                .last()
                .unwrap()
                .children
                .last()
                .unwrap()
                .enabled
        );
        picker.selected.insert(3);
        assert!(
            picker
                .compose()
                .children
                .last()
                .unwrap()
                .children
                .last()
                .unwrap()
                .enabled
        );
    }
    struct Metrics;
    impl TextMeasurer for Metrics {
        fn measure(&mut self, _: &str, style: &knave_ui::TextStyle, width: f32) -> TextLayout {
            TextLayout {
                size: [width.min(160.0), style.line_height],
                carets: Vec::new(),
            }
        }
        fn measure_spans(
            &mut self,
            _: &[knave_ui::TextSpan],
            style: &knave_ui::TextStyle,
            width: f32,
        ) -> TextLayout {
            self.measure("", style, width)
        }
    }
    fn click(picker: &mut Picker, id: ElementId) {
        let rect = picker.scene.bounds(id).unwrap();
        let point = [rect.x + rect.width / 2.0, rect.y + rect.height / 2.0];
        picker.input(Input::PointerMove(point));
        picker.input(Input::PointerDown(point));
        // A frame between press and release must retain toolkit pointer capture.
        picker.frame(picker.size, &mut Metrics);
        picker.input(Input::PointerUp(point));
    }
    #[test]
    fn actual_pointer_selection_toggles_and_confirmation_closes() {
        let mut picker = Picker::new(request()).unwrap();
        picker.frame([820.0, 640.0], &mut Metrics);
        click(&mut picker, ElementId(SOURCE));
        assert_eq!(picker.selected.iter().copied().collect::<Vec<_>>(), vec![3]);
        picker.frame([820.0, 640.0], &mut Metrics);
        click(&mut picker, ElementId(SOURCE));
        assert!(picker.selected.is_empty());
        picker.frame([820.0, 640.0], &mut Metrics);
        click(&mut picker, ElementId(SOURCE));
        picker.frame([820.0, 640.0], &mut Metrics);
        click(&mut picker, CONFIRM);
        assert!(picker.should_close());
    }
    #[test]
    fn sharing_stop_button_closes_without_source_selection() {
        let mut req = request();
        req.operation = Operation::Sharing;
        let mut picker = Picker::new(req).unwrap();
        picker.frame([520.0, 220.0], &mut Metrics);
        click(&mut picker, CONFIRM);
        assert!(picker.should_close());
    }
    #[test]
    fn escape_cancels() {
        let mut picker = Picker::new(request()).unwrap();
        picker.input(Input::Key {
            key: Key::Escape,
            pressed: true,
            repeat: false,
            modifiers: KeyModifiers::default(),
        });
        assert!(picker.should_close());
    }
}
