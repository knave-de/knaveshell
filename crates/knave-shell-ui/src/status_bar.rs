//! Complete status bar presentation shared by desktop and overview hosts.
use crate::{DisplayList, UiScene};
use knave_desktop_api::{DesktopSnapshot, notifications::NotificationSnapshot};

pub const STATUS_BAR_HEIGHT: f32 = 36.0;

const EDGE: f32 = 8.0;
const ISLAND_Y: f32 = 4.0;
const ISLAND_HEIGHT: f32 = 28.0;
const GROUP_GAP: f32 = 6.0;
const WORKSPACE_WIDTH: f32 = 22.0;
const WORKSPACE_GAP: f32 = 3.0;
const APPLET_PITCH: f32 = 25.0;
const APPLET_WIDTH: f32 = 22.0;
const SYSTEM_WIDTH: f32 = 48.0;
const BELL_WIDTH: f32 = 28.0;

/// Presentation data only; D-Bus endpoints remain owned by the tray worker.
#[derive(Clone, Debug, PartialEq)]
pub struct Applet {
    pub id: u64,
    pub title: String,
    pub image: Option<knave_ui::UiImage>,
    pub attention: bool,
}

/// Applet hit rectangles are also used by the runtime for pointer and popover placement.
pub fn applet_targets(
    width: f32,
    applets: &[Applet],
) -> impl Iterator<Item = (knave_ui::Rect, &Applet)> {
    applet_targets_with_notifications(width, applets, true)
}

/// Variant used by the bar runtime when notification support is unavailable.
pub fn applet_targets_with_notifications(
    width: f32,
    applets: &[Applet],
    notifications_enabled: bool,
) -> impl Iterator<Item = (knave_ui::Rect, &Applet)> {
    let bell_x = width - EDGE - SYSTEM_WIDTH - GROUP_GAP - BELL_WIDTH;
    let bell_visible = notifications_enabled && bell_x >= EDGE + 28.0 + GROUP_GAP;
    let bell_width = if bell_visible {
        BELL_WIDTH + GROUP_GAP
    } else {
        0.0
    };
    let bell_x = width - EDGE - SYSTEM_WIDTH - bell_width;
    let right = bell_x - GROUP_GAP - 4.0;
    let left_limit = EDGE + 28.0 + GROUP_GAP + 8.0 + WORKSPACE_WIDTH + GROUP_GAP + 4.0;
    let capacity = (((right - left_limit + WORKSPACE_GAP) / APPLET_PITCH)
        .floor()
        .max(0.0)) as usize;
    let count = capacity.min(applets.len());
    let start = right - count as f32 * APPLET_PITCH + WORKSPACE_GAP;
    applets
        .iter()
        .take(count)
        .enumerate()
        .map(move |(index, applet)| {
            (
                knave_ui::Rect::new(
                    start + index as f32 * APPLET_PITCH,
                    ISLAND_Y,
                    APPLET_WIDTH,
                    ISLAND_HEIGHT,
                ),
                applet,
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

    #[allow(clippy::too_many_arguments)]
    pub fn with_focused_icon(
        revision: u64,
        width: f32,
        height: f32,
        desktop: Option<&DesktopSnapshot>,
        notifications_enabled: bool,
        notifications: Option<&NotificationSnapshot>,
        applets: &[Applet],
        focused_icon: Option<&knave_ui::UiImage>,
    ) -> Self {
        let mut scene = UiScene::bar(revision, width, height);
        if let Some(crate::UiNode::Panel { color, .. }) = scene.nodes.first_mut() {
            *color = knave_ui::Color::rgba(21, 24, 27, 255);
        }
        scene.targets.clear();
        scene.target(
            knave_ui::Rect::new(EDGE, ISLAND_Y, 28.0, ISLAND_HEIGHT),
            crate::UiAction::OpenOverview,
        );

        let applets: Vec<_> =
            applet_targets_with_notifications(width, applets, notifications_enabled).collect();
        let system_x = width - EDGE - SYSTEM_WIDTH;
        let bell_x = system_x - GROUP_GAP - BELL_WIDTH;
        let workspace_start = EDGE + 28.0 + GROUP_GAP;
        let bell_visible = notifications_enabled && bell_x >= EDGE + 28.0 + GROUP_GAP;
        let controls_left = if bell_visible {
            bell_x - GROUP_GAP
        } else {
            system_x - GROUP_GAP
        };
        let workspace_limit = applets
            .first()
            .map_or(controls_left, |(rect, _)| rect.x - 4.0 - GROUP_GAP)
            .min(controls_left);

        let workspaces = visible_workspaces(desktop, workspace_start, workspace_limit);
        if !workspaces.is_empty() {
            let group_x = workspace_start;
            let group_width = 8.0
                + workspaces.len() as f32 * WORKSPACE_WIDTH
                + workspaces.len().saturating_sub(1) as f32 * WORKSPACE_GAP;
            for (index, workspace) in workspaces.iter().enumerate() {
                let x = group_x + 4.0 + index as f32 * (WORKSPACE_WIDTH + WORKSPACE_GAP);
                let bounds = knave_ui::Rect::new(x, ISLAND_Y + 3.0, WORKSPACE_WIDTH, 22.0);
                scene.push(crate::UiNode::Label {
                    id: crate::NodeId(10 + u64::from(workspace.workspace.0)),
                    bounds,
                    color: if workspace.active {
                        knave_ui::Color::rgba(186, 213, 255, 255)
                    } else if workspace.window_count > 0 {
                        knave_ui::Color::rgba(185, 192, 198, 255)
                    } else {
                        knave_ui::Color::rgba(125, 133, 140, 255)
                    },
                    text: if workspace.workspace.0 == 10 {
                        "0".into()
                    } else {
                        workspace.workspace.0.to_string()
                    },
                });
                scene.target(bounds, crate::UiAction::FocusWorkspace(workspace.workspace));
            }
            // The pill is painted by the display list; this transparent panel gives the group a stable id.
            scene.push(crate::UiNode::Panel {
                id: crate::NodeId(4_999),
                bounds: knave_ui::Rect::new(group_x, ISLAND_Y, group_width, ISLAND_HEIGHT),
                color: knave_ui::Color::rgba(0, 0, 0, 0),
            });
        }

        let workspace_group_width = if workspaces.is_empty() {
            0.0
        } else {
            8.0 + workspaces.len() as f32 * WORKSPACE_WIDTH
                + workspaces.len().saturating_sub(1) as f32 * WORKSPACE_GAP
        };
        let focus_x = workspace_start
            + workspace_group_width
            + if workspaces.is_empty() {
                0.0
            } else {
                GROUP_GAP
            };
        let focus_right = applets
            .first()
            .map_or(controls_left, |(rect, _)| rect.x - 4.0 - GROUP_GAP);
        let focus = desktop.and_then(focused_window);
        let focus_available = focus_right - focus_x;
        let mut focus_island = None;
        // Focus is the first optional section to give way to workspace and control targets.
        if let Some(window) = focus.filter(|window| {
            focus_available >= 34.0 && (!window.title.is_empty() || focused_icon.is_some())
        }) {
            let icon_bounds = knave_ui::Rect::new(focus_x + 6.0, 10.0, 16.0, 16.0);
            if let Some(image) = focused_icon {
                scene.push(crate::UiNode::Image {
                    id: crate::NodeId(9),
                    bounds: icon_bounds,
                    image: image.clone(),
                });
            }
            let content_start = focus_x + 6.0;
            let text_x = if focused_icon.is_some() {
                icon_bounds.x + icon_bounds.width + 7.0
            } else {
                content_start
            };
            let max_text_width = if focused_icon.is_some() { 203.0 } else { 226.0 };
            let text_width = (focus_right - text_x - 6.0).clamp(0.0, max_text_width);
            let mut content_right = if focused_icon.is_some() {
                icon_bounds.x + icon_bounds.width
            } else {
                content_start
            };
            if !window.title.is_empty() && text_width > 0.0 {
                let text = ellipsize(&window.title.replace(['\n', '\r'], " "), text_width);
                if !text.is_empty() {
                    let rendered_width = (estimated_text_width(&text) + 2.0).min(text_width);
                    content_right = text_x + rendered_width;
                    scene.push(crate::UiNode::Label {
                        id: crate::NodeId(8),
                        bounds: knave_ui::Rect::new(text_x, 0.0, rendered_width, STATUS_BAR_HEIGHT),
                        color: knave_ui::Color::TEXT,
                        text,
                    });
                }
            }
            let island = knave_ui::Rect::new(
                focus_x,
                ISLAND_Y,
                (content_right + 6.0 - focus_x)
                    .max(28.0)
                    .min(focus_right - focus_x),
                ISLAND_HEIGHT,
            );
            if island.width >= 28.0 {
                scene.push(crate::UiNode::Panel {
                    id: crate::NodeId(7_000),
                    bounds: island,
                    color: knave_ui::Color::rgba(0, 0, 0, 0),
                });
                focus_island = Some(island);
            }
        }

        if !applets.is_empty() {
            let first = applets.first().unwrap().0;
            let last = applets.last().unwrap().0;
            scene.push(crate::UiNode::Panel {
                id: crate::NodeId(6_000),
                bounds: knave_ui::Rect::new(
                    first.x - 4.0,
                    ISLAND_Y,
                    last.x + last.width - first.x + 8.0,
                    ISLAND_HEIGHT,
                ),
                color: knave_ui::Color::rgba(0, 0, 0, 0),
            });
            for (bounds, item) in &applets {
                scene.target(*bounds, crate::UiAction::Applet(item.id));
                if let Some(image) = &item.image {
                    scene.push(crate::UiNode::Image {
                        id: crate::NodeId(20_000 + item.id),
                        bounds: knave_ui::Rect::new(bounds.x + 3.0, 10.0, 16.0, 16.0),
                        image: image.clone(),
                    });
                } else {
                    scene.push(crate::UiNode::Label {
                        id: crate::NodeId(20_000 + item.id),
                        bounds: knave_ui::Rect::new(bounds.x + 3.0, 6.0, 16.0, 24.0),
                        color: knave_ui::Color::TEXT,
                        text: item.title.chars().next().unwrap_or('?').to_string(),
                    });
                }
                if item.attention {
                    scene.push(crate::UiNode::Panel {
                        id: crate::NodeId(30_000 + item.id),
                        bounds: knave_ui::Rect::new(bounds.x + 7.0, 29.0, 8.0, 2.0),
                        color: knave_ui::Color::ACCENT,
                    });
                }
            }
        }

        let controls_visible = system_x >= EDGE + 28.0 + GROUP_GAP;
        if controls_visible {
            scene.target(
                knave_ui::Rect::new(system_x, ISLAND_Y, SYSTEM_WIDTH, ISLAND_HEIGHT),
                crate::UiAction::ToggleQuickSettings,
            );
        }
        if bell_visible {
            scene.target(
                knave_ui::Rect::new(bell_x, ISLAND_Y, BELL_WIDTH, ISLAND_HEIGHT),
                crate::UiAction::ToggleNotifications,
            );
            if let Some(image) = crate::status_icons::bell() {
                scene.push(crate::UiNode::Image {
                    id: crate::NodeId(40_000),
                    bounds: knave_ui::Rect::new(bell_x + 6.0, 10.0, 16.0, 16.0),
                    image,
                });
            }
        }
        if controls_visible {
            append_system_icons(&mut scene, system_x);
        }

        let mut display = scene.display_list();
        let mut islands =
            knave_ui::DisplayListBuilder::new(revision, knave_ui::Color::rgba(0, 0, 0, 0));
        let pill = |color: knave_ui::Color| knave_ui::ShapePaint {
            radii: knave_ui::CornerRadii::uniform(6.0),
            ..knave_ui::ShapePaint::fill(color)
        };
        islands.shape(
            knave_ui::Rect::new(EDGE, ISLAND_Y, 28.0, ISLAND_HEIGHT),
            pill(knave_ui::Color::rgba(32, 38, 43, 255)),
        );
        if !workspaces.is_empty() {
            let group_width = 8.0
                + workspaces.len() as f32 * WORKSPACE_WIDTH
                + workspaces.len().saturating_sub(1) as f32 * WORKSPACE_GAP;
            islands.shape(
                knave_ui::Rect::new(workspace_start, ISLAND_Y, group_width, ISLAND_HEIGHT),
                pill(knave_ui::Color::rgba(32, 38, 43, 255)),
            );
            for (index, _) in workspaces.iter().enumerate().filter(|(_, w)| w.active) {
                let x = workspace_start + 4.0 + index as f32 * (WORKSPACE_WIDTH + WORKSPACE_GAP);
                islands.shape(
                    knave_ui::Rect::new(x, ISLAND_Y + 3.0, WORKSPACE_WIDTH, 22.0),
                    knave_ui::ShapePaint {
                        radii: knave_ui::CornerRadii::uniform(4.0),
                        ..knave_ui::ShapePaint::fill(knave_ui::Color::rgba(49, 60, 73, 255))
                    },
                );
            }
            for (index, workspace) in workspaces
                .iter()
                .enumerate()
                .filter(|(_, workspace)| workspace.window_count > 0)
            {
                let x = workspace_start + 4.0 + index as f32 * (WORKSPACE_WIDTH + WORKSPACE_GAP);
                islands.shape(
                    knave_ui::Rect::new(x + 9.5, ISLAND_Y + 22.0, 3.0, 3.0),
                    knave_ui::ShapePaint {
                        radii: knave_ui::CornerRadii::uniform(1.5),
                        ..knave_ui::ShapePaint::fill(if workspace.active {
                            knave_ui::Color::rgba(255, 255, 255, 255)
                        } else {
                            knave_ui::Color::ACCENT
                        })
                    },
                );
            }
        }
        if let Some(bounds) = focus_island {
            islands.shape(bounds, pill(knave_ui::Color::rgba(32, 38, 43, 255)));
        }
        if !applets.is_empty() {
            let first = applets.first().unwrap().0;
            let last = applets.last().unwrap().0;
            islands.shape(
                knave_ui::Rect::new(
                    first.x - 4.0,
                    ISLAND_Y,
                    last.x + last.width - first.x + 8.0,
                    ISLAND_HEIGHT,
                ),
                pill(knave_ui::Color::rgba(32, 38, 43, 255)),
            );
        }
        if controls_visible {
            islands.shape(
                knave_ui::Rect::new(system_x, ISLAND_Y, SYSTEM_WIDTH, ISLAND_HEIGHT),
                pill(knave_ui::Color::rgba(32, 38, 43, 255)),
            );
        }
        if bell_visible {
            islands.shape(
                knave_ui::Rect::new(bell_x, ISLAND_Y, BELL_WIDTH, ISLAND_HEIGHT),
                pill(knave_ui::Color::rgba(32, 38, 43, 255)),
            );
        }
        let mut island_commands = islands
            .finish()
            .expect("status bar geometry is valid")
            .commands;
        // Paint island backplates above the opaque strip and below its contents.
        let insert_at = display
            .commands
            .iter()
            .position(|command| matches!(command, knave_ui::DisplayCommand::Shape { .. }))
            .map_or(1, |index| index + 1);
        display
            .commands
            .splice(insert_at..insert_at, island_commands.drain(..));
        set_status_text_style(&mut display);
        if bell_visible {
            append_bell(&mut display, bell_x + BELL_WIDTH / 2.0, notifications);
        }
        if let Some(knave_ui::DisplayCommand::Shape { bounds, clip, .. }) =
            display.commands.first_mut()
        {
            *bounds = knave_ui::Rect::new(-2.0, -2.0, width + 4.0, height + 4.0);
            *clip = Some(knave_ui::Rect::new(0.0, 0.0, width, height));
        }
        Self { scene, display }
    }
}

fn visible_workspaces(
    desktop: Option<&DesktopSnapshot>,
    start: f32,
    limit: f32,
) -> Vec<&knave_desktop_api::WorkspaceSummary> {
    let Some(snapshot) = desktop else {
        return Vec::new();
    };
    let capacity = (((limit - start - 8.0 + WORKSPACE_GAP) / (WORKSPACE_WIDTH + WORKSPACE_GAP))
        .floor()
        .max(0.0)) as usize;
    if capacity == 0 {
        return Vec::new();
    }
    let mut workspaces: Vec<_> = snapshot.workspaces.iter().collect();
    workspaces.sort_by_key(|workspace| workspace.workspace.0);
    let mut visible: Vec<_> = workspaces.into_iter().take(capacity).collect();
    if !visible.iter().any(|workspace| workspace.active)
        && let Some(active) = snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.active)
    {
        if visible.is_empty() {
            visible.push(active);
        } else {
            *visible.last_mut().unwrap() = active;
        }
        visible.sort_by_key(|workspace| workspace.workspace.0);
    }
    visible
}

fn focused_window(snapshot: &DesktopSnapshot) -> Option<&knave_desktop_api::WindowSummary> {
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.active)?;
    snapshot.windows.iter().find(|window| {
        window.focused && !window.minimized && window.workspace == workspace.workspace
    })
}

fn set_status_text_style(display: &mut DisplayList) {
    for command in &mut display.commands {
        if let knave_ui::DisplayCommand::Text {
            bounds,
            style,
            clip,
            ..
        } = command
        {
            style.font_size = 11.0;
            style.line_height = 14.0;
            style.wrap = knave_ui::TextWrap::None;
            if bounds.width <= WORKSPACE_WIDTH {
                style.align = knave_ui::TextAlign::Center;
            }
            *clip = Some(*bounds);
        }
    }
}

fn append_system_icons(scene: &mut UiScene, x: f32) {
    if let Some(image) = crate::status_icons::wifi() {
        scene.push(crate::UiNode::Image {
            id: crate::NodeId(40_001),
            bounds: knave_ui::Rect::new(x + 5.0, 10.0, 16.0, 16.0),
            image,
        });
    }
    if let Some(image) = crate::status_icons::volume() {
        scene.push(crate::UiNode::Image {
            id: crate::NodeId(40_002),
            bounds: knave_ui::Rect::new(x + 27.0, 10.0, 16.0, 16.0),
            image,
        });
    }
}

fn ellipsize(text: &str, width: f32) -> String {
    const ELLIPSIS_WIDTH: f32 = 12.0;
    if estimated_text_width(text) <= width {
        return text.to_owned();
    }
    if width < ELLIPSIS_WIDTH {
        return String::new();
    }
    let mut result = String::new();
    let mut used = 0.0;
    for character in text.chars() {
        let next = estimated_glyph_width(character);
        if used + next + ELLIPSIS_WIDTH > width {
            break;
        }
        used += next;
        result.push(character);
    }
    result.push('…');
    result
}

fn estimated_text_width(text: &str) -> f32 {
    text.chars().map(estimated_glyph_width).sum()
}

fn estimated_glyph_width(character: char) -> f32 {
    // Conservative bounds for the 11px bar font; the renderer also clips the line.
    match character {
        ' ' | 'i' | 'l' | '.' | ',' | ':' | ';' => 4.0,
        '!' | '|' | '(' | ')' | '[' | ']' => 5.0,
        'm' | 'w' | 'M' | 'W' | '@' | '%' | '&' | '…' => 12.0,
        'A'..='Z' => 9.0,
        '0'..='9' => 7.5,
        _ if character.is_ascii() => 7.0,
        _ => 12.0,
    }
}

/// Stable right-edge bell and count. Counts describe unread history, independent of banner lifetime.
fn append_bell(
    list: &mut knave_ui::DisplayList,
    midpoint: f32,
    snapshot: Option<&NotificationSnapshot>,
) {
    use knave_ui::{
        Color, CornerRadii, DisplayListBuilder, Rect, ShapePaint, TextAlign, TextStyle,
    };
    let x = midpoint - 14.0;
    let mut b = DisplayListBuilder::new(list.revision, Color::rgba(0, 0, 0, 0));
    let count = snapshot
        .filter(|s| s.owned)
        .map_or(0, NotificationSnapshot::unread);
    if count > 0 {
        let text = if count > 99 {
            "99+".to_owned()
        } else {
            count.to_string()
        };
        let w = if count > 99 { 16.0 } else { 14.0 };
        b.shape(
            Rect::new(x + 14.0, 4.0, w, 14.0),
            ShapePaint {
                radii: CornerRadii::uniform(8.0),
                ..ShapePaint::fill(Color::rgba(174, 220, 242, 255))
            },
        );
        b.text(
            Rect::new(x + 14.0, 4.0, w, 14.0),
            text,
            TextStyle {
                color: Color::rgba(21, 24, 27, 255),
                font_size: 8.0,
                line_height: 10.0,
                align: TextAlign::Center,
                ..Default::default()
            },
        );
    }
    if snapshot.is_some_and(|s| s.dnd) {
        b.text(
            Rect::new(x + 1.0, 22.0, 5.0, 10.0),
            "z".into(),
            TextStyle {
                color: Color::ACCENT,
                font_size: 8.0,
                line_height: 10.0,
                ..Default::default()
            },
        );
    } else if snapshot.is_none_or(|s| !s.owned) {
        b.text(
            Rect::new(x + 22.0, 22.0, 6.0, 10.0),
            if snapshot.is_some() { "E" } else { "!" }.into(),
            TextStyle {
                color: Color::rgba(174, 220, 242, 255),
                font_size: 8.0,
                line_height: 10.0,
                ..Default::default()
            },
        );
    }
    list.commands.extend(b.finish().unwrap().commands);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UiAction, UiNode};
    use knave_desktop_api::{WorkspaceId, WorkspaceSummary};

    fn desktop() -> DesktopSnapshot {
        DesktopSnapshot {
            generation: 1,
            overview_visible: false,
            workspaces: vec![
                WorkspaceSummary {
                    workspace: WorkspaceId(0),
                    active: false,
                    window_count: 0,
                    visible_window_count: 0,
                },
                WorkspaceSummary {
                    workspace: WorkspaceId(1),
                    active: true,
                    window_count: 2,
                    visible_window_count: 1,
                },
                WorkspaceSummary {
                    workspace: WorkspaceId(2),
                    active: false,
                    window_count: 1,
                    visible_window_count: 1,
                },
            ],
            windows: Vec::new(),
        }
    }

    fn applets(count: u64) -> Vec<Applet> {
        (1..=count)
            .map(|id| Applet {
                id,
                title: "Applet".into(),
                image: None,
                attention: false,
            })
            .collect()
    }

    #[test]
    fn compact_bar_orders_workspaces_applets_bell_and_system_controls() {
        let applets = applets(3);
        let bar = StatusBar::new(1, 1280.0, 36.0, Some(&desktop()), true, None, &applets);
        let workspace = bar
            .scene
            .hit_targets()
            .find_map(|(bounds, action)| {
                matches!(action, UiAction::FocusWorkspace(_)).then_some(bounds)
            })
            .unwrap();
        let tray = applet_targets(1280.0, &applets).next().unwrap().0;
        let bell = bar
            .scene
            .hit_targets()
            .find_map(|(bounds, action)| {
                (action == UiAction::ToggleNotifications).then_some(bounds)
            })
            .unwrap();
        let system = bar
            .scene
            .hit_targets()
            .find_map(|(bounds, action)| {
                (action == UiAction::ToggleQuickSettings).then_some(bounds)
            })
            .unwrap();
        assert!(workspace.x < tray.x && tray.x < bell.x && bell.x < system.x);
        assert_eq!(system.x, 1224.0);
        assert_eq!(bell.x, 1190.0);
        assert_eq!(system.width, 48.0);
        assert!(bar.scene.nodes().iter().any(|node| matches!(node,
            UiNode::Image { id, bounds, .. } if id.0 == 40_001 && bounds.width == 16.0)));
        assert!(bar.scene.nodes().iter().any(|node| matches!(node,
            UiNode::Image { id, bounds, .. } if id.0 == 40_002 && bounds.width == 16.0)));
        assert!(bar.scene.nodes().iter().any(|node| matches!(node,
            UiNode::Image { id, bounds, .. } if id.0 == 40_000 && bounds.width == 16.0)));
        assert!(!bar.scene.nodes().iter().any(|node| matches!(node,
            UiNode::Label { text, .. } if text == "Settings")));
    }

    #[test]
    fn workspace_zero_and_occupancy_dots_render_with_active_chip() {
        let bar = StatusBar::new(1, 600.0, 36.0, Some(&desktop()), true, None, &[]);
        assert!(bar.scene.nodes().iter().any(|node| matches!(node,
            UiNode::Label { text, .. } if text == "0")));
        assert!(bar.display.commands.iter().any(|command| matches!(command,
            knave_ui::DisplayCommand::Shape { bounds, paint, .. }
                if bounds.width == 3.0 && bounds.height == 3.0 && paint.radii.top_left == 1.5)));
        assert!(bar.display.commands.iter().any(|command| matches!(command,
            knave_ui::DisplayCommand::Shape { paint, .. } if paint.radii.top_left >= 4.0)));
    }

    #[test]
    fn focused_title_uses_a_bounded_clipped_island_and_one_line_text() {
        let mut desktop = desktop();
        desktop.windows.push(knave_desktop_api::WindowSummary {
            id: knave_desktop_api::WindowId(9),
            title: "文書作成 — a very long focused application title".into(),
            app_id: "org.example.Editor".into(),
            workspace: WorkspaceId(1),
            focused: true,
            minimized: false,
            floating: false,
            fullscreen: false,
            maximized: false,
        });
        let image = knave_ui::UiImage::from_rgba(1, 1, vec![1, 2, 3, 255]).unwrap();
        let bar = StatusBar::with_focused_icon(
            1,
            800.0,
            36.0,
            Some(&desktop),
            true,
            None,
            &[],
            Some(&image),
        );
        let (bounds, text) = bar
            .scene
            .nodes()
            .iter()
            .find_map(|node| match node {
                UiNode::Label {
                    id, bounds, text, ..
                } if id.0 == 8 => Some((*bounds, text)),
                _ => None,
            })
            .expect("focused title");
        assert!(text.ends_with('…'));
        assert!(estimated_text_width(text) <= bounds.width);
        assert!(bounds.width <= 205.0);
        assert!(bar.display.commands.iter().any(|command| matches!(command,
            knave_ui::DisplayCommand::Text { bounds: clip_bounds, style, clip: Some(clip), text: rendered, .. }
                if clip_bounds == &bounds && clip == &bounds && rendered == text
                    && style.wrap == knave_ui::TextWrap::None && style.font_size == 11.0)));
        assert!(bar.scene.nodes().iter().any(|node| matches!(node,
            UiNode::Image { id, bounds, .. } if id.0 == 9 && bounds.width == 16.0)));
    }

    #[test]
    fn focused_section_yields_first_and_controls_do_not_overlap_at_narrow_widths() {
        let applets = applets(32);
        for width in [64.0, 80.0, 100.0, 140.0, 180.0, 250.0] {
            let bar = StatusBar::new(1, width, 36.0, Some(&desktop()), true, None, &applets);
            let mut targets: Vec<_> = bar.scene.hit_targets().map(|(bounds, _)| bounds).collect();
            targets.sort_by(|left, right| left.x.total_cmp(&right.x));
            assert!(
                targets
                    .iter()
                    .all(|bounds| bounds.x >= 0.0 && bounds.x + bounds.width <= width)
            );
            assert!(
                targets
                    .windows(2)
                    .all(|pair| pair[0].x + pair[0].width <= pair[1].x)
            );
            if width >= 200.0 {
                assert!(
                    bar.scene
                        .hit_targets()
                        .any(|(_, action)| { action == UiAction::FocusWorkspace(WorkspaceId(1)) })
                );
            }
            if width < 250.0 {
                assert!(!bar.scene.nodes().iter().any(|node| matches!(node,
                    UiNode::Label { id, .. } if id.0 == 8)));
            }
        }
    }

    #[test]
    fn disabled_notifications_release_the_bell_slot_for_applets() {
        let applets = applets(32);
        let with_bell: Vec<_> = applet_targets(800.0, &applets).collect();
        let without_bell: Vec<_> =
            applet_targets_with_notifications(800.0, &applets, false).collect();
        assert!(without_bell.len() >= with_bell.len());
        let bar = StatusBar::new(1, 800.0, 36.0, None, false, None, &applets);
        assert!(
            !bar.scene
                .hit_targets()
                .any(|(_, action)| action == UiAction::ToggleNotifications)
        );
        assert!(
            bar.scene
                .hit_targets()
                .any(|(_, action)| action == UiAction::ToggleQuickSettings)
        );
    }

    #[test]
    fn untitled_focus_keeps_its_icon_and_missing_focus_clears_the_island() {
        let mut snapshot = desktop();
        snapshot.windows.push(knave_desktop_api::WindowSummary {
            id: knave_desktop_api::WindowId(9),
            title: String::new(),
            app_id: String::new(),
            workspace: WorkspaceId(1),
            focused: true,
            minimized: false,
            floating: false,
            fullscreen: false,
            maximized: false,
        });
        let image = knave_ui::UiImage::from_rgba(1, 1, vec![1, 2, 3, 255]).unwrap();
        let compose = |snapshot: &DesktopSnapshot| {
            StatusBar::with_focused_icon(
                1,
                800.0,
                36.0,
                Some(snapshot),
                true,
                None,
                &[],
                Some(&image),
            )
        };
        let bar = compose(&snapshot);
        assert!(bar.scene.nodes().iter().any(|node| matches!(node,
            UiNode::Image { id, .. } if id.0 == 9)));
        assert!(!bar.scene.nodes().iter().any(|node| matches!(node,
            UiNode::Label { id, .. } if id.0 == 8)));
        snapshot.windows[0].focused = false;
        let bar = compose(&snapshot);
        assert!(!bar.scene.nodes().iter().any(|node| matches!(node,
            UiNode::Image { id, .. } if id.0 == 9)));
        assert!(!bar.scene.nodes().iter().any(|node| matches!(node,
            UiNode::Panel { id, .. } if id.0 == 7_000)));
    }
}
