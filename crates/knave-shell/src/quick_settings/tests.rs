use super::*;
use knave_ui::{TextSpan, TextStyle};
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

fn app() -> QuickSettings {
    let mut app = QuickSettings::new();
    app.apply(QuickSettingsSnapshot {
        open: true,
        network: Some(Network::default()),
        output: Some(Volume {
            percent: 65,
            muted: false,
        }),
        input: Some(Volume {
            percent: 40,
            muted: false,
        }),
        brightness: Some(80),
        power: Some(Power {
            active: "balanced".into(),
            available: vec![
                "power-saver".into(),
                "balanced".into(),
                "performance".into(),
            ],
        }),
        battery: Some(Battery {
            name: "BAT1".into(),
            percent: 80,
            status: "Not charging".into(),
            limit: Some(80),
        }),
        ..Default::default()
    });
    app
}
#[test]
fn all_pages_fit_narrow_and_short_outputs() {
    for size in [[320.0, 240.0], [320.0, 568.0], [1280.0, 800.0]] {
        for page in [Page::Main, Page::Network, Page::Night, Page::Battery] {
            let mut app = app();
            app.page = page;
            let list = app.frame(size, &mut Metrics).clone();
            assert_eq!(list.clear_color.alpha, 0);
            let panel = app.scene.bounds(PANEL).unwrap();
            assert!(panel.x >= 0.0 && panel.y >= 0.0);
            assert!(panel.x + panel.width <= size[0]);
            assert!(panel.y + panel.height <= size[1]);
            {
                let id = CLOSE;
                let r = app.scene.bounds(ElementId(id)).unwrap();
                assert!(r.x >= panel.x && r.x + r.width <= panel.x + panel.width + 0.1);
                assert!(r.y + r.height <= panel.y + panel.height + 0.1);
            }
        }
    }
}
#[test]
fn ethernet_is_preferred_and_volume_values_remain_local_until_commit() {
    let mut app = app();
    let mut n = Network {
        ethernet: Some("enp1s0".into()),
        wireless: Some("Home".into()),
        ..Default::default()
    };
    assert!(n.summary().starts_with("Ethernet"));
    n.ethernet = None;
    assert!(n.summary().starts_with("Home"));
    app.value(ElementId(OUTPUT), 72.0, false);
    assert_eq!(app.state.output.unwrap().percent, 72);
    app.page = Page::Night;
    app.night.enabled = true;
    app.value(ElementId(TEMPERATURE), 3500.0, false);
    assert_eq!(app.night.temperature, 3500);
}

#[test]
fn close_intent_suppresses_late_open_snapshots_until_acknowledged() {
    let mut app = app();
    app.send(QuickSettingsCommand::Close);
    let late = QuickSettingsSnapshot {
        open: true,
        ..Default::default()
    };
    app.apply(late.clone());
    assert!(!app.surface_visible());
    app.apply(QuickSettingsSnapshot::default());
    app.apply(late);
    assert!(app.surface_visible());
}

fn click(app: &mut QuickSettings, id: u64) {
    let r = app.scene.bounds(ElementId(id)).unwrap();
    let p = [r.x + r.width / 2.0, r.y + r.height / 2.0];
    app.input(Input::PointerMove(p));
    app.input(Input::PointerDown(p));
    app.input(Input::PointerUp(p));
    app.frame(app.size, &mut Metrics);
}
#[test]
fn power_is_main_header_only_and_refresh_is_absent() {
    for page in [Page::Main, Page::Network, Page::Night, Page::Battery] {
        let mut app = app();
        app.page = page;
        let list = app.frame([1280.0, 800.0], &mut Metrics).clone();
        assert!(
            !list
                .commands
                .iter()
                .any(|c| matches!(c,knave_ui::DisplayCommand::Text{text,..} if text=="Refresh"))
        );
        assert_eq!(
            app.scene.bounds(ElementId(POWER)).is_some(),
            page == Page::Main
        );
        let close = app.scene.bounds(ElementId(CLOSE)).unwrap();
        if page == Page::Main {
            let power = app.scene.bounds(ElementId(POWER)).unwrap();
            assert_eq!(power.y, close.y);
            assert_eq!(power.height, close.height);
        } else {
            let back = app.scene.bounds(ElementId(BACK)).unwrap();
            assert_eq!(back.y, close.y);
            assert_eq!(back.height, close.height);
        }
    }
}
#[test]
fn power_confirmation_is_modal_survives_updates_and_escape_cancels() {
    let mut app = app();
    app.frame([1280.0, 800.0], &mut Metrics);
    app.select_power(2);
    app.frame(app.size, &mut Metrics);
    assert!(app.confirmation.is_some());
    let dialog = app.scene.bounds(ElementId(CONFIRMATION)).unwrap();
    assert!(dialog.width > 0.0 && dialog.height > 0.0);
    let latest = app.state.clone();
    app.apply(latest);
    app.frame(app.size, &mut Metrics);
    assert!(app.scene.bounds(ElementId(CONFIRMATION)).unwrap().height > 0.0);
    assert!(app.scene.set_focus(Some(ElementId(CLOSE))).is_err());
    app.input(Input::Key {
        key: Key::Escape,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    });
    app.frame(app.size, &mut Metrics);
    assert!(app.confirmation.is_none());
    assert!(app.surface_visible());
    app.select_power(3);
    app.frame(app.size, &mut Metrics);
    click(&mut app, CANCEL_CONFIRMATION);
    assert!(app.confirmation.is_none());
    assert!(app.surface_visible());
}
#[test]
fn brightness_home_reaches_one_percent_and_authentication_releases_input() {
    let mut app = app();
    app.frame([1280.0, 800.0], &mut Metrics);
    app.scene.set_focus(Some(ElementId(BRIGHTNESS))).unwrap();
    app.input(Input::Key {
        key: Key::Home,
        pressed: true,
        repeat: false,
        modifiers: Default::default(),
    });
    assert_eq!(app.state.brightness, Some(1));
    app.authenticating = true;
    assert!(!app.surface_visible());
    assert_eq!(app.keyboard_mode(), Some(KeyboardMode::None));
    assert!(app.input_region().unwrap().is_empty());
    app.input(Input::FocusLost);
    assert!(app.state.open);
}
