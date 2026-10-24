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
    let count = (((width - 290.0).max(0.0) / 32.0) as usize).min(applets.len());
    let start = width - 140.0 - count as f32 * 32.0;
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
        Self::with_focused_icon(
            revision,
            width,
            height,
            desktop,
            notifications_enabled,
            notifications,
            applets,
            None,
        )
    }

    pub fn with_focused_icon(
        revision: u64,
        width: f32,
        height: f32,
        desktop: Option<&DesktopSnapshot>,
        notifications_enabled: bool,
        notifications: Option<&NotificationSnapshot>,
        applets: &[Applet],
        focused_icon: Option<&crate::UiImage>,
    ) -> Self {
        let mut scene = UiScene::bar_with_snapshot(revision, width, height, desktop);
        if !notifications_enabled {
            scene.disable_notifications();
        }
        if width >= 240.0 {
            let bounds = crate::Rect::new(width - 140.0, 0.0, 80.0, STATUS_BAR_HEIGHT);
            scene.targets.retain(|t| {
                !matches!(t.action, crate::UiAction::FocusWorkspace(_))
                    || t.bounds.x + t.bounds.width <= bounds.x
            });
            scene.nodes.retain(|n| !matches!(n,crate::UiNode::Label{id,bounds:b,..} if id.0>=10 && b.x+b.width>bounds.x));
            scene.target(bounds, crate::UiAction::ToggleQuickSettings);
            scene.push(crate::UiNode::Label {
                id: crate::NodeId(40_000),
                bounds: crate::Rect::new(bounds.x + 6.0, 8.0, 74.0, 22.0),
                color: crate::Color::TEXT,
                text: "Settings".into(),
            });
        }
        let targets: Vec<_> = applet_targets(width, applets).collect();
        if let Some((first, _)) = targets.first() {
            scene.nodes.retain(|node| match node {
                crate::UiNode::Label { id, bounds, .. } if (10..100).contains(&id.0) => {
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
        // The workspace chips and the existing brand area own the left edge;
        // applets, quick settings and the bell own the right edge.
        if let Some(snapshot) = desktop {
            scene.nodes.retain(|node| {
                !matches!(
                    node,
                    crate::UiNode::Label {
                        id: crate::NodeId(2), text, ..
                    } if text.starts_with("Workspace ")
                )
            });
            let workspace_end = scene
                .targets
                .iter()
                .filter(|target| matches!(target.action, crate::UiAction::FocusWorkspace(_)))
                .map(|target| target.bounds.x + target.bounds.width)
                .fold(48.0_f32, f32::max);
            let applet_start = applet_targets(width, applets)
                .next()
                .map(|(bounds, _)| bounds.x)
                .unwrap_or(width);
            let reserved_start = scene
                .targets
                .iter()
                .filter(|target| {
                    matches!(
                        target.action,
                        crate::UiAction::ToggleQuickSettings | crate::UiAction::ToggleNotifications
                    )
                })
                .map(|target| target.bounds.x)
                .fold(width, f32::min);
            let left = workspace_end + 8.0;
            let right = applet_start.min(reserved_start) - 8.0;
            let focused = snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.active)
                .and_then(|workspace| {
                    snapshot.windows.iter().find(|window| {
                        window.focused
                            && !window.minimized
                            && window.workspace == workspace.workspace
                    })
                });
            if let Some(window) = focused.filter(|_| left + 22.0 <= right) {
                let icon = crate::Rect::new(left, 7.0, 22.0, 22.0);
                if let Some(image) = focused_icon {
                    scene.push(crate::UiNode::Image {
                        id: crate::NodeId(9),
                        bounds: icon,
                        image: image.clone(),
                    });
                }
                let text_left = icon.x + icon.width + 6.0;
                let text_width = (right - text_left).max(0.0);
                if !window.title.is_empty() {
                    let text = ellipsize(&window.title.replace(['\n', '\r'], " "), text_width);
                    if !text.is_empty() {
                        scene.push(crate::UiNode::Label {
                            id: crate::NodeId(8),
                            bounds: crate::Rect::new(text_left, 0.0, text_width, STATUS_BAR_HEIGHT),
                            color: crate::Color::TEXT,
                            text,
                        });
                    }
                }
            }
        }
        let mut display = DisplayList::from_scene(&scene);
        // Scene labels default to word wrapping. The focused title owns exactly
        // one line and must never paint into adjacent controls.
        if let Some(crate::UiNode::Label { bounds, text, .. }) = scene.nodes().iter().find(|node| {
            matches!(
                node,
                crate::UiNode::Label {
                    id: crate::NodeId(8),
                    ..
                }
            )
        }) {
            display.commands.retain(|command| {
                !matches!(command,
                crate::DisplayCommand::Text { bounds: b, text: t, .. }
                    if b == bounds && t == text)
            });
            display.commands.push(crate::DisplayCommand::Text {
                bounds: *bounds,
                text: text.clone(),
                style: crate::TextStyle {
                    wrap: crate::TextWrap::None,
                    ..Default::default()
                },
                transform: crate::Transform2D::IDENTITY,
                clip: Some(*bounds),
            });
        }
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

fn ellipsize(text: &str, width: f32) -> String {
    // Composition has no renderer/font system. Reserve more than one em per
    // scalar (including the ellipsis); the display clip remains authoritative
    // for unusual fonts and shaping sequences.
    const ELLIPSIS_WIDTH: f32 = 18.0;
    let glyph_width = |_: char| 18.0;
    if text.chars().map(glyph_width).sum::<f32>() <= width {
        return text.to_owned();
    }
    let mut used = 0.0;
    let mut prefix = String::new();
    for ch in text.chars() {
        let next_width = glyph_width(ch);
        if used + next_width + ELLIPSIS_WIDTH > width {
            break;
        }
        used += next_width;
        prefix.push(ch);
    }
    if width < ELLIPSIS_WIDTH {
        return String::new();
    }
    prefix.push('…');
    prefix
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
    use knave_desktop_api::{WindowId, WindowSummary, WorkspaceId, WorkspaceSummary};

    fn desktop(title: &str) -> DesktopSnapshot {
        DesktopSnapshot {
            generation: 1,
            overview_visible: true,
            workspaces: vec![WorkspaceSummary {
                workspace: WorkspaceId(1),
                active: true,
                window_count: 1,
                visible_window_count: 1,
            }],
            windows: vec![WindowSummary {
                id: WindowId(1),
                title: title.into(),
                app_id: "org.example.App".into(),
                workspace: WorkspaceId(1),
                focused: true,
                minimized: false,
                floating: false,
                fullscreen: false,
                maximized: false,
            }],
        }
    }

    fn node_label(bar: &StatusBar, id: u64) -> Option<&str> {
        bar.scene.nodes().iter().find_map(|node| match node {
            crate::UiNode::Label {
                id: node_id, text, ..
            } if node_id.0 == id => Some(text.as_str()),
            _ => None,
        })
    }

    #[test]
    fn focused_title_is_unicode_safe_and_ellipsized_before_reserved_targets() {
        let long = "文書作成 — very long application title 📄📄";
        let image = crate::UiImage::from_rgba(1, 1, vec![1, 2, 3, 255]).unwrap();
        let bar = StatusBar::with_focused_icon(
            0,
            500.0,
            36.0,
            Some(&desktop(long)),
            true,
            None,
            &[],
            Some(&image),
        );
        let label = node_label(&bar, 8).expect("focused title");
        assert!(label.ends_with('…'));
        assert!(label.is_char_boundary(label.len()));
        let bounds = bar
            .scene
            .nodes()
            .iter()
            .find_map(|node| match node {
                crate::UiNode::Label { id, bounds, .. } if id.0 == 8 => Some(*bounds),
                _ => None,
            })
            .unwrap();
        assert!(bounds.x + bounds.width <= 352.0);
        assert!(bar.scene.nodes().iter().any(|node| matches!(node,
            crate::UiNode::Image { id, .. } if id.0 == 9)));
        assert_eq!(
            bar.scene.hit_test(490.0, 18.0),
            Some(UiAction::ToggleNotifications)
        );
        assert_eq!(
            bar.scene.hit_test(380.0, 18.0),
            Some(UiAction::ToggleQuickSettings)
        );
    }

    #[test]
    fn missing_focus_clears_the_title_and_icon_and_tiny_widths_stay_clamped() {
        let image = crate::UiImage::from_rgba(1, 1, vec![1, 2, 3, 255]).unwrap();
        let empty = desktop("");
        let bar = StatusBar::with_focused_icon(
            0,
            180.0,
            36.0,
            Some(&empty),
            true,
            None,
            &[],
            Some(&image),
        );
        assert!(node_label(&bar, 8).is_none());
        assert!(!bar.scene.nodes().iter().any(|node| matches!(node,
            crate::UiNode::Image { id, .. } if id.0 == 9)));
        let disconnected = StatusBar::new(0, 1.0, 36.0, None, true, None, &[]);
        assert!(node_label(&disconnected, 8).is_none());
        assert_eq!(
            disconnected.scene.hit_test(0.0, 18.0),
            Some(UiAction::ToggleNotifications)
        );
        let narrow = StatusBar::with_focused_icon(
            0,
            350.0,
            36.0,
            Some(&desktop("Window title")),
            true,
            None,
            &[],
            Some(&image),
        );
        assert!(node_label(&narrow, 8).is_none());
        assert!(!narrow.scene.nodes().iter().any(|node| matches!(node,
            crate::UiNode::Image { id, .. } if id.0 == 9)));
        assert_eq!(
            narrow.scene.hit_test(340.0, 18.0),
            Some(UiAction::ToggleNotifications)
        );
    }

    #[test]
    fn focused_app_icon_is_shown_when_window_title_is_empty() {
        let image = crate::UiImage::from_rgba(1, 1, vec![1, 2, 3, 255]).unwrap();
        let bar = StatusBar::with_focused_icon(
            0,
            500.0,
            36.0,
            Some(&desktop("")),
            true,
            None,
            &[],
            Some(&image),
        );

        assert!(node_label(&bar, 8).is_none());
        assert!(bar.scene.nodes().iter().any(|node| matches!(node,
            crate::UiNode::Image { id, image: rendered, .. }
                if id.0 == 9 && rendered == &image)));
    }

    #[test]
    fn focused_title_ends_before_the_first_applet() {
        let applets: Vec<_> = (1..=2)
            .map(|id| Applet {
                id,
                title: "App".into(),
                image: None,
                attention: false,
            })
            .collect();
        let bar = StatusBar::with_focused_icon(
            0,
            800.0,
            36.0,
            Some(&desktop("A lengthy focused application title")),
            true,
            None,
            &applets,
            None,
        );
        let title = bar
            .scene
            .nodes()
            .iter()
            .find_map(|node| match node {
                crate::UiNode::Label {
                    id, bounds, text, ..
                } if id.0 == 8 => Some((*bounds, text.as_str())),
                _ => None,
            })
            .expect("focused title fits");
        assert!(title.0.x + title.0.width <= 588.0);
        assert!(title.1.ends_with('…'));
        let first_applet = applet_targets(800.0, &applets).next().unwrap().0;
        assert_eq!(first_applet.x, 596.0);
        assert!(title.0.x + title.0.width < first_applet.x);
    }
    #[test]
    fn the_brand_mark_opens_overview_without_clashing_with_bar_controls() {
        for width in [100.0, 180.0, 250.0, 800.0, 1920.0] {
            let bar = StatusBar::new(0, width, 36.0, None, true, None, &[]);
            assert_eq!(bar.scene.hit_test(20.0, 18.0), Some(UiAction::OpenOverview));
            // Workspace chips and the bell keep their zones remote from the brand.
            assert_ne!(
                bar.scene.hit_test(160.0, 18.0),
                Some(UiAction::OpenOverview)
            );
            assert_eq!(
                bar.scene.hit_test((width - 28.0).max(0.0), 18.0),
                Some(UiAction::ToggleNotifications)
            );
        }
    }

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
