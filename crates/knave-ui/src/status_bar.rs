//! Complete status bar presentation shared by desktop and overview hosts.
use crate::{DisplayList, UiScene};
use knave_desktop_api::{DesktopSnapshot, notifications::NotificationSnapshot};

pub const STATUS_BAR_HEIGHT: f32 = 36.0;

/// Presentation data only; D-Bus endpoints remain owned by the tray worker.
#[derive(Clone, Debug, PartialEq)]
pub struct Applet {
    pub id: u64,
    pub title: String,
    pub image: Option<crate::UiImage>,
    pub attention: bool,
}

/// Keep the notification hit area clear and reserve the left-hand workspace label.
pub fn applet_targets(
    width: f32,
    applets: &[Applet],
) -> impl Iterator<Item = (crate::Rect, &Applet)> {
    let count = (((width - 210.0).max(0.0) / 32.0) as usize).min(applets.len());
    let start = width - 60.0 - count as f32 * 32.0;
    applets
        .iter()
        .take(count)
        .enumerate()
        .map(move |(i, item)| {
            (
                crate::Rect::new(start + i as f32 * 32.0, 0.0, 32.0, STATUS_BAR_HEIGHT),
                item,
            )
        })
}

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
        applets: &[Applet],
    ) -> Self {
        let mut scene = UiScene::bar_with_snapshot(revision, width, height, desktop);
        if !notifications_enabled {
            scene.disable_notifications();
        }
        let targets: Vec<_> = applet_targets(width, applets).collect();
        if let Some((first, _)) = targets.first() {
            scene.nodes.retain(|node| match node {
                crate::UiNode::Label { id, bounds, .. } if id.0 >= 10 => {
                    bounds.x + bounds.width <= first.x
                }
                _ => true,
            });
            scene.targets.retain(|target| {
                !matches!(target.action, crate::UiAction::FocusWorkspace(_))
                    || target.bounds.x + target.bounds.width <= first.x
            });
        }
        for (bounds, item) in targets {
            scene.target(bounds, crate::UiAction::Applet(item.id));
            let icon = crate::Rect::new(bounds.x + 5.0, 6.0, 22.0, 22.0);
            if let Some(image) = &item.image {
                scene.push(crate::UiNode::Image {
                    id: crate::NodeId(20_000 + item.id),
                    bounds: icon,
                    image: image.clone(),
                });
            } else {
                scene.push(crate::UiNode::Label {
                    id: crate::NodeId(20_000 + item.id),
                    bounds: icon,
                    color: crate::Color::TEXT,
                    text: item.title.chars().next().unwrap_or('?').to_string(),
                });
            }
            if item.attention {
                scene.push(crate::UiNode::Panel {
                    id: crate::NodeId(30_000 + item.id),
                    bounds: crate::Rect::new(bounds.x + 12.0, 31.0, 8.0, 2.0),
                    color: crate::Color::ACCENT,
                });
            }
        }
        let mut display = DisplayList::from_scene(&scene);
        // Keep the strip opaque at surface edges even when the host clears to
        // transparent: clip an oversized fill rather than antialiasing its edge.
        if let Some(crate::DisplayCommand::Shape { bounds, clip, .. }) =
            display.commands.first_mut()
        {
            *bounds = crate::Rect::new(-2.0, -2.0, width + 4.0, height + 4.0);
            *clip = Some(crate::Rect::new(0.0, 0.0, width, height));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::UiAction;
    #[test]
    fn applets_stay_left_of_bell_and_fit_small_outputs() {
        let applets: Vec<_> = (1..=32)
            .map(|id| Applet {
                id,
                title: "App".into(),
                image: None,
                attention: false,
            })
            .collect();
        for width in [1.0, 180.0, 250.0, 800.0, 1920.0] {
            let bar = StatusBar::new(0, width, 36.0, None, true, None, &applets);
            for (rect, item) in applet_targets(width, &applets) {
                assert!(rect.x >= 150.0);
                assert!(rect.x + rect.width <= width - 60.0);
                assert_eq!(
                    bar.scene.hit_test(rect.x + 16.0, 18.0),
                    Some(UiAction::Applet(item.id))
                );
            }
            assert_eq!(
                bar.scene.hit_test((width - 28.0).max(0.0), 18.0),
                Some(UiAction::ToggleNotifications)
            );
        }
        let bar = StatusBar::new(0, 800.0, 36.0, None, false, None, &applets);
        assert_eq!(bar.scene.hit_test(772.0, 18.0), None);
        assert!(
            bar.scene
                .hit_targets()
                .any(|(_, a)| matches!(a, UiAction::Applet(_)))
        );
    }
}
