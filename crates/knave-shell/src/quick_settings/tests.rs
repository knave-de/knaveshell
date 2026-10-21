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
        for page in [
            Page::Main,
            Page::Network,
            Page::Night,
            Page::Battery,
            Page::Power,
        ] {
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
