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
        network: Some(Network {
            ethernet: Some("enp1s0".into()),
            ..Default::default()
        }),
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

#[test]
fn trimmed_subtitles_keep_status_and_authentication_text() {
    let mut app = app();
    app.page = Page::Network;
    let network = app.frame([1280.0, 800.0], &mut Metrics).clone();
    assert!(!network.commands.iter().any(|command| matches!(command,
        knave_ui::DisplayCommand::Text { text, .. }
            if text.contains("Wi-Fi remains available"))));
    assert!(network.commands.iter().any(|command| matches!(command,
        knave_ui::DisplayCommand::Text { text, .. }
            if text.contains("Ethernet"))));
    app.page = Page::Battery;
    app.dirty = true;
    let battery = app.frame(app.size, &mut Metrics).clone();
    assert!(!battery.commands.iter().any(|command| matches!(command,
        knave_ui::DisplayCommand::Text { text, .. }
            if text.contains("Stop charging at this level"))));
    assert!(battery.commands.iter().any(|command| matches!(command,
        knave_ui::DisplayCommand::Text { text, .. }
            if text.contains("administrator authentication"))));
}

#[test]
fn compact_root_keeps_device_controls_visible_and_tiles_side_by_side() {
    let mut app = app();
    let list = app.frame([1280.0, 800.0], &mut Metrics).clone();
    let panel = app.scene.bounds(PANEL).unwrap();
    assert_eq!(panel.width, 340.0);
    let target = |predicate: fn(&Target) -> bool| {
        app.targets
            .iter()
            .find(|(_, t)| predicate(t))
            .map(|(id, _)| app.scene.bounds(*id).unwrap())
            .unwrap()
    };
    let network = target(|t| matches!(t, Target::Page(Page::Network)));
    let bluetooth = target(|t| {
        matches!(
            t,
            Target::Command(QuickSettingsCommand::SetBluetooth { .. })
        )
    });
    assert_eq!(network.y, bluetooth.y);
    assert!(network.x + network.width <= bluetooth.x);
    assert!(app.scene.bounds(ElementId(PROFILE)).unwrap().y < network.y);
    for rect in [
        app.scene.bounds(ElementId(OUTPUT)).unwrap(),
        app.scene.bounds(ElementId(INPUT)).unwrap(),
        app.scene.bounds(ElementId(BRIGHTNESS)).unwrap(),
        target(|t| matches!(t, Target::Page(Page::Battery))),
    ] {
        assert!(
            rect.y >= panel.y && rect.y + rect.height <= panel.y + panel.height,
            "{rect:?} outside {panel:?}"
        );
    }
    assert!(list.commands.iter().any(|c|matches!(c,knave_ui::DisplayCommand::Shape{paint,..} if paint.fill==Some(Color::rgba(32,36,39,255)))));
    click(&mut app, POWER);
    assert!(app.scene.interaction(ElementId(POWER)).unwrap().expanded);
}
#[test]
fn profile_menu_and_bundled_icons_have_bounded_reusable_state() {
    let mut app = app();
    app.frame([1280.0, 800.0], &mut Metrics);
    assert!(
        matches!(app.scene.widget(ElementId(PROFILE)),Some(Widget::Menu{items,..}) if items.len()==3)
    );
    for light in [false, true] {
        app.state.light = light;
        app.dirty = true;
        let keys = |list: &DisplayList| {
            list.commands
                .iter()
                .filter_map(|c| match c {
                    knave_ui::DisplayCommand::Image { image, .. } => {
                        assert_eq!(image.pixels().len(), 24 * 24 * 4);
                        Some(image.cache_key())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let first = keys(app.frame([1280.0, 800.0], &mut Metrics));
        app.dirty = true;
        let next = keys(app.frame([1280.0, 800.0], &mut Metrics));
        assert!(!first.is_empty());
        assert_eq!(first, next);
    }
}

#[test]
fn row_icons_are_vertically_centered() {
    let mut app = app();
    app.frame([1280.0, 800.0], &mut Metrics);
    let centered = |child: u64, parent: u64| {
        let child = app.scene.bounds(ElementId(child)).unwrap();
        let parent = app.scene.bounds(ElementId(parent)).unwrap();
        assert!((child.y + child.height / 2.0 - parent.y - parent.height / 2.0).abs() < 0.1);
    };
    centered(101, POWER);
    centered(109, 108);
    centered(114, 113);
}

#[test]
fn charge_limit_completion_requires_verified_result_and_preserves_page() {
    let mut app = app();
    app.page = Page::Battery;
    app.charge_feedback = ChargeFeedback::Applying(75);
    app.authenticating = true;
    app.finish_charge_limit(Ok(75));
    assert!(matches!(app.charge_feedback, ChargeFeedback::Applied(75)));
    assert_eq!(app.state.battery.as_ref().unwrap().limit, Some(75));
    assert_eq!(app.limit, 75);
    assert!(app.page == Page::Battery);
    assert!(!app.authenticating);
    let list = app.frame([1280.0, 800.0], &mut Metrics).clone();
    assert!(list.commands.iter().any(|c|matches!(c,knave_ui::DisplayCommand::Text{text,..} if text.contains("Confirmed by the system"))));
    app.charge_feedback = ChargeFeedback::Applying(80);
    app.authenticating = true;
    app.finish_charge_limit(Err("Authentication cancelled".into()));
    assert!(matches!(app.charge_feedback, ChargeFeedback::Failed(_)));
    assert_eq!(app.state.battery.as_ref().unwrap().limit, Some(75));
    assert!(!app.authenticating);
    assert!(app.surface_visible());
    app.charge_feedback = ChargeFeedback::Applying(80);
    app.finish_charge_limit(Ok(90));
    assert!(matches!(app.charge_feedback, ChargeFeedback::Failed(_)));
    assert_eq!(app.state.battery.as_ref().unwrap().limit, Some(75));
}
#[test]
fn charge_limit_without_service_never_claims_applied() {
    let mut app = app();
    app.page = Page::Battery;
    app.apply_charge_limit();
    assert!(matches!(app.charge_feedback, ChargeFeedback::Failed(_)));
    assert!(!app.authenticating);
    assert!(app.surface_visible());
}
