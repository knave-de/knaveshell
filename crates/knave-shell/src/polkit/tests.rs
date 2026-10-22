use super::*;
use knave_ui::{TextSpan, TextStyle};
struct Metrics;
impl TextMeasurer for Metrics {
    fn measure(&mut self, text: &str, style: &TextStyle, width: f32) -> TextLayout {
        let columns = (width / (style.font_size * 0.6)).floor().max(1.0) as usize;
        TextLayout {
            size: [
                (text.chars().count() as f32 * style.font_size * 0.6).min(width),
                text.chars().count().max(1).div_ceil(columns) as f32 * style.line_height,
            ],
            carets: Vec::new(),
        }
    }
    fn measure_spans(&mut self, spans: &[TextSpan], style: &TextStyle, width: f32) -> TextLayout {
        self.measure(
            &spans.iter().map(|s| s.text.as_str()).collect::<String>(),
            style,
            width,
        )
    }
}
fn agent(echo: bool) -> Agent {
    let mut a = Agent::new();
    a.state = State {
        generation: 42,
        visible: true,
        message: "An application requires administrator privileges".into(),
        prompt: Some(("Password:".into(), echo)),
        ..Default::default()
    };
    a.focus_field = true;
    a.frame([560.0, 440.0], &mut Metrics);
    a
}
fn key(a: &mut Agent, key: Key) {
    a.input(Input::Key {
        key,
        pressed: true,
        repeat: false,
        modifiers: KeyModifiers::default(),
    });
}
#[test]
fn hidden_response_never_enters_scene_clipboard_or_ime() {
    let mut a = agent(false);
    a.input(Input::Text("pāss🔑".into()));
    let display = format!("{:?}", a.frame([560.0, 440.0], &mut Metrics));
    assert!(!display.contains("pāss"));
    assert_eq!(a.scene.widget(FIELD), Some(&Widget::Button("•••••".into())));
    assert!(a.text_input().is_none());
    assert!(a.scene.text_input_state().is_none());
    for key in [Key::C, Key::X, Key::V] {
        a.input(Input::Key {
            key,
            pressed: true,
            repeat: false,
            modifiers: KeyModifiers {
                control: true,
                ..Default::default()
            },
        });
        assert!(a.take_request().is_none());
    }
    a.input(Input::FocusLost);
    assert!(a.secret.is_empty());
}
#[test]
fn reply_is_bounded_cleared_and_sent_once() {
    let mut a = agent(false);
    let (sender, receiver) = async_channel::bounded(1);
    a.commands = Some(sender);
    a.input(Input::Text("x".repeat(4097)));
    assert!(a.secret.is_empty());
    a.input(Input::Text("secret".into()));
    key(&mut a, Key::Enter);
    key(&mut a, Key::Enter);
    assert!(a.secret.is_empty());
    assert!(a.state.prompt.is_none());
    let Command::Response(42, response) = receiver.try_recv().unwrap() else {
        panic!("expected response")
    };
    assert_eq!(&*response, "secret");
    assert!(receiver.try_recv().is_err());
}
#[test]
fn escape_cancels_and_new_prompt_discards_old_secret() {
    let mut a = agent(false);
    let (sender, receiver) = async_channel::bounded(1);
    a.commands = Some(sender);
    a.input(Input::Text("secret".into()));
    key(&mut a, Key::Escape);
    assert!(matches!(receiver.try_recv(), Ok(Command::Cancel(42))));
    assert!(a.secret.is_empty());
    a.secret.push_str("old");
    a.shared.lock().unwrap().change(|s| {
        *s = State {
            generation: 43,
            ..Default::default()
        }
    });
    a.update();
    assert!(!a.surface_visible());
    assert!(a.secret.is_empty());
    assert_eq!(a.keyboard_mode(), Some(KeyboardMode::None));
}
#[test]
fn echo_prompt_and_unicode_deletion() {
    let mut a = agent(true);
    a.input(Input::Text("a🔑".into()));
    key(&mut a, Key::Backspace);
    a.frame([560.0, 440.0], &mut Metrics);
    assert_eq!(a.scene.widget(FIELD), Some(&Widget::Button("a".into())));
}

/// Run explicitly inside the isolated desktop harness; no system bus or PAM is used.
#[test]
#[ignore]
fn live_dialog() {
    struct Preview(Agent, async_channel::Receiver<Command>);
    impl Application for Preview {
        fn set_waker(&mut self, _waker: Waker) {}
        fn input(&mut self, event: Input) {
            self.0.input(event);
        }
        fn update(&mut self) {
            if let Ok(command) = self.1.try_recv() {
                match command {
                    Command::Response(_, response) => {
                        assert_eq!(&*response, "knave-test");
                        eprintln!(
                            "POLKIT_PREVIEW: masked response received; secret cleared={}",
                            self.0.secret.is_empty()
                        );
                        self.0.state.visible = false;
                    }
                    Command::Cancel(_) => {
                        eprintln!("POLKIT_PREVIEW: cancelled");
                        self.0.state.visible = false;
                    }
                    Command::Select(..) => unreachable!(),
                }
            }
        }
        fn needs_frame(&self) -> bool {
            self.0.needs_frame()
        }
        fn frame(&mut self, size: [f32; 2], text: &mut dyn TextMeasurer) -> &DisplayList {
            self.0.frame(size, text)
        }
        fn should_close(&self) -> bool {
            !self.0.state.visible
        }
    }
    let mut a = agent(false);
    let (sender, receiver) = async_channel::bounded(1);
    a.commands = Some(sender);
    a.dirty = true;
    knave_wayland::run_application(
        SurfaceOptions {
            size: Some([560, 440]),
            keyboard: KeyboardMode::Exclusive,
            layer: SurfaceLayer::Overlay,
            namespace: "knave-shell-polkit-preview".into(),
            ..Default::default()
        },
        Preview(a, receiver),
    )
    .unwrap();
}

#[test]
fn long_messages_keep_actions_visible_and_identity_selection_is_typed() {
    let mut a = agent(false);
    a.state.message = "Long authorization explanation ".repeat(130);
    a.state.prompt = None;
    a.state.identities = vec!["Administrator".into(), "Other admin".into()];
    a.dirty = true;
    a.frame([560.0, 440.0], &mut Metrics);
    for id in [CANCEL, CONFIRM] {
        let bounds = a.scene.bounds(id).unwrap();
        assert!(bounds.y >= 0.0 && bounds.y + bounds.height <= 440.0);
    }
    let (sender, receiver) = async_channel::bounded(1);
    a.commands = Some(sender);
    a.identity = 1;
    key(&mut a, Key::Enter);
    assert!(matches!(receiver.try_recv(), Ok(Command::Select(42, 1))));
}
