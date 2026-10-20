//! Complete status bar presentation shared by desktop and overview hosts.
use crate::{DisplayList, UiScene};
use knave_desktop_api::{DesktopSnapshot, notifications::NotificationSnapshot};

pub const STATUS_BAR_HEIGHT: f32 = 36.0;

pub struct StatusBar {
    pub scene: UiScene,
    pub display: DisplayList,
}

impl StatusBar {
    pub fn new(
        revision: u64,
        width: f32,
        height: f32,
        desktop: Option<&DesktopSnapshot>,
        notifications_enabled: bool,
        notifications: Option<&NotificationSnapshot>,
    ) -> Self {
        let mut scene = UiScene::bar_with_snapshot(revision, width, height, desktop);
        if !notifications_enabled {
            scene.disable_notifications();
        }
        let mut display = DisplayList::from_scene(&scene);
        if notifications_enabled {
            append_bell(&mut display, width, notifications);
        }
        Self { scene, display }
    }
}

/// Stable right-edge bell and count. Counts describe unread history, independent of banner lifetime.
fn append_bell(list: &mut crate::DisplayList, width: f32, snapshot: Option<&NotificationSnapshot>) {
    use crate::{Color, CornerRadii, DisplayListBuilder, Rect, ShapePaint, TextAlign, TextStyle};
    let x = (width - 47.0).max(0.0);
    let mut b = DisplayListBuilder::new(list.revision, Color::rgba(0, 0, 0, 0));
    let color = if snapshot.is_some_and(|s| s.owned) {
        Color::TEXT
    } else {
        Color::rgba(155, 170, 188, 255)
    };
    b.shape(
        Rect::new(x + 5.0, 10.0, 16.0, 15.0),
        ShapePaint {
            radii: CornerRadii {
                top_left: 8.0,
                top_right: 8.0,
                ..Default::default()
            },
            ..ShapePaint::fill(color)
        },
    );
    b.shape(
        Rect::new(x + 2.0, 23.0, 22.0, 3.0),
        ShapePaint {
            radii: CornerRadii::uniform(1.5),
            ..ShapePaint::fill(color)
        },
    );
    for r in [
        Rect::new(x + 11.0, 7.0, 4.0, 5.0),
        Rect::new(x + 10.0, 27.0, 6.0, 3.0),
    ] {
        b.shape(
            r,
            ShapePaint {
                radii: CornerRadii::uniform(2.0),
                ..ShapePaint::fill(color)
            },
        );
    }
    let count = snapshot
        .filter(|s| s.owned)
        .map_or(0, NotificationSnapshot::unread);
    if count > 0 {
        let text = if count > 99 {
            "99+".into()
        } else {
            count.to_string()
        };
        let w = if count > 99 { 24.0 } else { 18.0 };
        b.shape(
            Rect::new(x + 16.0, 1.0, w, 18.0),
            ShapePaint {
                radii: CornerRadii::uniform(9.0),
                ..ShapePaint::fill(Color::rgba(180, 35, 49, 255))
            },
        );
        b.text(
            Rect::new(x + 16.0, 2.0, w, 16.0),
            text,
            TextStyle {
                font_size: 11.0,
                line_height: 14.0,
                align: TextAlign::Center,
                ..Default::default()
            },
        );
    }
    if snapshot.is_some_and(|s| s.dnd) {
        b.text(
            Rect::new(x - 6.0, 15.0, 12.0, 18.0),
            "z".into(),
            TextStyle {
                color: Color::ACCENT,
                ..Default::default()
            },
        );
    } else if snapshot.is_none_or(|s| !s.owned) {
        b.text(
            Rect::new(x + 26.0, 15.0, 16.0, 18.0),
            if snapshot.is_some() { "E" } else { "!" }.into(),
            TextStyle {
                font_size: 11.0,
                ..Default::default()
            },
        );
    }
    list.commands.extend(b.finish().unwrap().commands);
}
