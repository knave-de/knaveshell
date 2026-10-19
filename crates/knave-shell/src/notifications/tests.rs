use super::*;
use knave_ui::{DisplayCommand, TextSpan, TextStyle};
struct Metrics;
impl TextMeasurer for Metrics {
    fn measure(&mut self, text: &str, style: &TextStyle, width: f32) -> TextLayout {
        let glyph = style.font_size * 0.6;
        let columns = (width / glyph).floor().max(1.0) as usize;
        let lines: usize = text
            .split('\n')
            .map(|s| s.chars().count().max(1).div_ceil(columns))
            .sum();
        TextLayout {
            size: [
                (text.chars().count() as f32 * glyph).min(width),
                lines as f32 * style.line_height,
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
fn notification(id: u64) -> Notification {
    Notification {
        id,
        app: "Example application with a long name".into(),
        app_id: "org.example.App".into(),
        summary: "A notification title that wraps safely".into(),
        body: "A message with Unicode: 日本語 العربية café. ".repeat(100),
        unread: true,
        urgency: 1,
        live: true,
        actions: (0..8)
            .map(|i| NotificationAction {
                key: format!("action{i}"),
                label: "A very long action label that must wrap without covering other buttons"
                    .repeat(3),
            })
            .collect(),
        progress: Some(100),
        ..Default::default()
    }
}
fn app(count: u64) -> Notifications {
    let mut app = Notifications::new();
    let state = NotificationSnapshot {
        owned: true,
        center_open: true,
        history_days: 7,
        history_limit: 500,
        notifications: (1..=count).rev().map(notification).collect(),
        ..Default::default()
    };
    app.apply(state);
    app
}
fn click(app: &mut Notifications, id: ElementId) {
    let r = app.scene.bounds(id).unwrap();
    let point = [r.x + r.width / 2.0, r.y + r.height / 2.0];
    app.input(Input::PointerMove(point));
    app.input(Input::PointerDown(point));
    app.frame(app.size, &mut Metrics);
    app.input(Input::PointerUp(point));
}
#[test]
fn narrow_short_and_large_views_keep_panel_and_navigation_in_bounds() {
    for size in [
        [320.0, 240.0],
        [320.0, 568.0],
        [400.0, 800.0],
        [1280.0, 800.0],
        [1920.0, 1080.0],
    ] {
        for light in [false, true] {
            for contrast in [false, true] {
                let mut app = app(500);
                app.state.light = light;
                app.state.high_contrast = contrast;
                let list = app.frame(size, &mut Metrics).clone();
                assert_eq!(list.clear_color.alpha, 0);
                let panel = app.scene.bounds(PANEL).unwrap();
                assert!(panel.x >= 0.0 && panel.y >= 0.0);
                assert!(panel.x + panel.width <= size[0]);
                assert!(panel.y + panel.height <= size[1]);
                for id in [CLOSE, PREVIOUS, NEXT] {
                    let r = app.scene.bounds(ElementId(id)).unwrap();
                    assert!(r.x >= panel.x && r.y >= panel.y);
                    assert!(
                        r.x + r.width <= panel.x + panel.width + 0.1,
                        "{size:?} {id} {r:?}"
                    );
                    assert!(
                        r.y + r.height <= panel.y + panel.height + 0.1,
                        "{size:?} {id} {r:?}"
                    );
                }
                let scroll = app.scene.bounds(ElementId(SCROLL)).unwrap();
                let footer = app.scene.bounds(ElementId(NEXT)).unwrap();
                assert!(scroll.y + scroll.height <= footer.y + 0.1);
                assert!(app.error.is_none(), "{:?}", app.error);
                for c in &list.commands {
                    let (r, clip) = match c {
                        DisplayCommand::Shape { bounds, clip, .. }
                        | DisplayCommand::Text { bounds, clip, .. }
                        | DisplayCommand::RichText { bounds, clip, .. }
                        | DisplayCommand::Image { bounds, clip, .. } => (*bounds, *clip),
                    };
                    assert!(
                        r.x.is_finite() && r.y.is_finite() && r.width >= 0.0 && r.height >= 0.0
                    );
                    if let Some(clip) = clip {
                        assert!(clip.x >= 0.0 && clip.y >= 0.0);
                        assert!(clip.x + clip.width <= size[0] + 0.1);
                        assert!(clip.y + clip.height <= size[1] + 0.1);
                    }
                }
            }
        }
    }
}
#[test]
fn paging_reaches_all_history_without_exceeding_scene_budget() {
    let mut app = app(500);
    app.frame([400.0, 800.0], &mut Metrics);
    assert!(app.scene.bounds(ElementId(view::card_id(500))).is_some());
    assert!(app.scene.bounds(ElementId(view::card_id(480))).is_none());
    for _ in 0..24 {
        click(&mut app, ElementId(NEXT));
        app.frame(app.size, &mut Metrics);
    }
    assert_eq!(app.page, 24);
    assert!(app.scene.bounds(ElementId(view::card_id(1))).is_some());
    assert!(app.scene.bounds(ElementId(view::card_id(500))).is_none());
}
#[test]
fn banner_input_regions_exclude_desktop_and_never_request_keyboard() {
    let mut app = app(3);
    app.state.center_open = false;
    for n in &mut app.state.notifications {
        n.banner = true;
    }
    app.frame([1280.0, 800.0], &mut Metrics);
    assert_eq!(app.keyboard_mode(), Some(KeyboardMode::None));
    for r in app.input_region().unwrap() {
        assert!(r.x >= 850.0);
        assert!(r.y >= 44.0);
        assert!(r.y + r.height <= 800.0);
    }
    assert!(
        !app.input_region()
            .unwrap()
            .iter()
            .any(|r| contains(*r, [100.0, 100.0]))
    );
    assert!(app.surface_visible());
}
#[test]
fn empty_external_and_disconnected_states_are_honest() {
    let mut app = app(0);
    app.state.owned = false;
    app.frame([320.0, 568.0], &mut Metrics);
    assert!(!app.scene.widget(ElementId(DND)).is_none());
    assert!(
        app.display
            .commands
            .iter()
            .any(|c| matches!(c,DisplayCommand::Text{text,..}if text.contains("external service")))
    );
    app.connected = false;
    assert!(!app.surface_visible());
}
#[test]
fn replacement_preserves_pointer_press_and_does_not_activate_a_neighbor() {
    let mut app = app(1);
    app.frame([400.0, 800.0], &mut Metrics);
    let dismiss = ElementId(view::card_id(1) + 4);
    let r = app.scene.bounds(dismiss).unwrap();
    let point = [r.x + 10.0, r.y + 10.0];
    app.input(Input::PointerDown(point));
    app.state.notifications[0].summary = "Updated while held".into();
    app.dirty = true;
    app.frame(app.size, &mut Metrics);
    assert_eq!(app.scene.capture(), Some(dismiss));
    app.input(Input::PointerUp(point));
    assert_eq!(app.scene.capture(), None);
}
#[test]
fn transient_is_not_in_center_or_badge() {
    let mut app = app(1);
    app.state.notifications[0].transient = true;
    app.frame([400.0, 800.0], &mut Metrics);
    assert_eq!(app.state.unread(), 0);
    assert!(app.scene.bounds(ElementId(view::card_id(1))).is_none());
}

#[test]
fn reopening_hidden_center_resets_keyboard_focus_to_close() {
    let mut app = app(2);
    app.frame([400.0, 800.0], &mut Metrics);
    app.scene.set_focus(Some(ElementId(DND))).unwrap();
    let mut state = app.state.clone();
    state.center_open = false;
    app.apply(state.clone());
    state.center_open = true;
    app.apply(state);
    app.frame(app.size, &mut Metrics);
    assert_eq!(app.scene.focus(), Some(ElementId(CLOSE)));
}
