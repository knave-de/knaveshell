//! Session-scoped authentication; secrets stay outside retained scenes and IME state.
mod backend;
use knave_ui::{Color, DisplayList, ShapePaint, TextWrap, toolkit::*};
use knave_wayland::{Application, KeyboardMode, SurfaceLayer, SurfaceOptions, Waker};
use std::sync::{Arc, Mutex};
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Default)]
struct State {
    generation: u64,
    visible: bool,
    message: String,
    action: String,
    identities: Vec<String>,
    prompt: Option<(String, bool)>,
    notice: String,
}
enum Command {
    Select(u64, usize),
    Response(u64, Zeroizing<String>),
    Cancel(u64),
}
#[derive(Default)]
struct Shared {
    state: State,
    changed: bool,
    waker: Option<Waker>,
}
impl Shared {
    fn change(&mut self, f: impl FnOnce(&mut State)) {
        f(&mut self.state);
        self.changed = true;
        if let Some(waker) = &self.waker {
            waker.wake();
        }
    }
}
const FIELD: ElementId = ElementId(4);
const CANCEL: ElementId = ElementId(5);
const CONFIRM: ElementId = ElementId(6);
const IDENTITY: ElementId = ElementId(7);
struct Agent {
    shared: Arc<Mutex<Shared>>,
    commands: Option<async_channel::Sender<Command>>,
    worker: Option<std::thread::JoinHandle<()>>,
    state: State,
    secret: Zeroizing<String>,
    selected_all: bool,
    identity: usize,
    scene: Scene,
    dirty: bool,
    focus_field: bool,
}
impl Agent {
    fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(Shared::default())),
            commands: None,
            worker: None,
            state: State::default(),
            secret: Zeroizing::new(String::new()),
            selected_all: false,
            identity: 0,
            scene: Scene::new(Element::new(0, Widget::Panel)).expect("valid empty scene"),
            dirty: true,
            focus_field: false,
        }
    }
    fn send(&mut self, command: Command) {
        if let Some(sender) = &self.commands {
            // At most one reply is outstanding. Disable the controls until backend progress.
            if sender.try_send(command).is_ok() {
                self.secret.zeroize();
                self.state.prompt = None;
                self.state.identities.clear();
                self.dirty = true;
            }
        }
    }
    fn confirm(&mut self) {
        if self.state.prompt.is_some() {
            let response = std::mem::replace(&mut self.secret, Zeroizing::new(String::new()));
            self.send(Command::Response(self.state.generation, response));
        } else if !self.state.identities.is_empty() {
            self.send(Command::Select(self.state.generation, self.identity));
        }
    }
    fn compose(&self) -> Element {
        let mut root = Element::new(0, Widget::Panel).layout(Layout {
            padding: 28.0,
            gap: 14.0,
            ..Default::default()
        });
        root.style.background = Some(ShapePaint::fill(Color::rgba(20, 29, 42, 255)));
        let text = |id, value: String| {
            let mut element = Element::new(id, Widget::Text(value));
            element.style.text.wrap = TextWrap::WordOrGlyph;
            element
        };
        root.children
            .push(text(1, "Authentication required".into()));
        let mut scroll = Element::new(11, Widget::Scroll).layout(Layout {
            height: Length::Fill,
            ..Default::default()
        });
        let mut content = Element::new(12, Widget::Panel).layout(Layout {
            gap: 12.0,
            ..Default::default()
        });
        content.children.push(text(2, self.state.message.clone()));
        content.children.push(text(3, self.state.action.clone()));
        if !self.state.identities.is_empty() {
            content.children.push(
                Element::new(
                    IDENTITY.0,
                    Widget::Dropdown {
                        label: "Authenticate as".into(),
                        items: self
                            .state
                            .identities
                            .iter()
                            .enumerate()
                            .map(|(i, label)| MenuItem::option(i as u64, label))
                            .collect(),
                        selected: Some(self.identity as u64),
                    },
                )
                .layout(Layout {
                    height: Length::Px(44.0),
                    ..Default::default()
                }),
            );
        }
        if let Some((prompt, echo)) = &self.state.prompt {
            content.children.push(text(8, prompt.clone()));
            let value = if *echo {
                self.secret.to_string()
            } else {
                "•".repeat(self.secret.chars().count())
            };
            let mut field = Element::new(
                FIELD.0,
                Widget::Button(if value.is_empty() { " ".into() } else { value }),
            );
            field.layout.height = Length::Px(44.0);
            field.style.background = Some(ShapePaint::fill(Color::rgba(31, 43, 60, 255)));
            content.children.push(field);
        }
        content.children.push(text(9, self.state.notice.clone()));
        scroll.children.push(content);
        root.children.push(scroll);
        let mut row = Element::new(10, Widget::Panel).layout(Layout {
            flow: Flow::Row,
            gap: 12.0,
            height: Length::Px(44.0),
            ..Default::default()
        });
        for (id, label) in [(CANCEL, "Cancel"), (CONFIRM, "Authenticate")] {
            let mut button = Element::new(id.0, Widget::Button(label.into()));
            button.layout.height = Length::Px(44.0);
            button.enabled =
                id == CANCEL || self.state.prompt.is_some() || !self.state.identities.is_empty();
            row.children.push(button);
        }
        root.children.push(row);
        root
    }
}
impl Application for Agent {
    fn set_waker(&mut self, waker: Waker) {
        self.shared.lock().unwrap().waker = Some(waker);
        let (sender, receiver) = async_channel::bounded(1);
        self.commands = Some(sender);
        let shared = self.shared.clone();
        self.worker = Some(std::thread::spawn(move || backend::run(shared, receiver)));
    }
    fn update(&mut self) {
        let mut shared = self.shared.lock().unwrap();
        if shared.changed {
            self.secret.zeroize();
            self.selected_all = false;
            if shared.state.generation != self.state.generation {
                self.identity = 0;
            }
            self.state = shared.state.clone();
            shared.changed = false;
            self.focus_field = self.state.prompt.is_some();
            self.dirty = true;
        }
    }
    fn input(&mut self, event: Input) {
        match event {
            Input::Key {
                key: Key::Escape,
                pressed: true,
                ..
            }
            | Input::Cancel => {
                self.send(Command::Cancel(self.state.generation));
            }
            Input::Key {
                key: Key::Enter,
                pressed: true,
                repeat: false,
                ..
            } => self.confirm(),
            Input::Key {
                key: Key::A,
                pressed: true,
                modifiers,
                ..
            } if modifiers.control && self.scene.focus() == Some(FIELD) => self.selected_all = true,
            Input::Key {
                key: Key::Backspace | Key::Delete,
                pressed: true,
                ..
            } if self.scene.focus() == Some(FIELD) => {
                if self.selected_all {
                    self.secret.zeroize();
                } else {
                    let mut shortened = Zeroizing::new(self.secret.to_string());
                    shortened.pop();
                    self.secret = shortened;
                }
                self.selected_all = false;
                self.dirty = true;
            }
            Input::Text(mut text)
                if self.state.prompt.is_some() && self.scene.focus() == Some(FIELD) =>
            {
                if self.selected_all {
                    self.secret.zeroize();
                }
                self.selected_all = false;
                if self.secret.len() + text.len() <= 4096 {
                    self.secret.extend(text.chars().filter(|c| !c.is_control()));
                }
                text.zeroize();
                self.dirty = true;
            }
            Input::FocusLost => {
                self.secret.zeroize();
                self.dirty = true;
                self.scene.event(Input::FocusLost);
            }
            event => match self.scene.event(event).action {
                Some(Action::Activated(id)) if id == CANCEL => {
                    self.send(Command::Cancel(self.state.generation))
                }
                Some(Action::Activated(id)) if id == CONFIRM => self.confirm(),
                Some(Action::Selected(IDENTITY, id)) => self.identity = id as usize,
                _ => {}
            },
        }
    }
    fn surface_visible(&self) -> bool {
        self.state.visible
    }
    fn keyboard_mode(&self) -> Option<KeyboardMode> {
        Some(if self.state.visible {
            KeyboardMode::Exclusive
        } else {
            KeyboardMode::None
        })
    }
    fn needs_frame(&self) -> bool {
        self.dirty || self.scene.needs_frame()
    }
    fn frame(&mut self, size: [f32; 2], text: &mut dyn TextMeasurer) -> &DisplayList {
        if self.dirty {
            self.scene
                .replace(self.compose())
                .expect("bounded authentication scene");
            self.scene.layout(size, text);
            if self.focus_field {
                let _ = self.scene.set_focus(Some(FIELD));
                self.focus_field = false;
            }
            self.dirty = false;
        }
        self.scene.display_list(size, text)
    }
}
impl Drop for Agent {
    fn drop(&mut self) {
        self.commands.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
pub fn run() -> Result<(), String> {
    knave_wayland::run_application(
        SurfaceOptions {
            size: Some([560, 440]),
            keyboard: KeyboardMode::None,
            layer: SurfaceLayer::Overlay,
            namespace: "knave-shell-polkit".into(),
            ..Default::default()
        },
        Agent::new(),
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
