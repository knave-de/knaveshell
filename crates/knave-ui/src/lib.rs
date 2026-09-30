//! Renderer-independent primitives for the Knave shell UI.

mod display_list;
mod geometry;
mod paint;
pub mod toolkit;

use std::sync::Arc;

use knave_desktop_api::{DesktopSnapshot, WindowId, WindowSummary, WorkspaceId};

pub use display_list::{
    DisplayCommand, DisplayList, DisplayListBuildError, DisplayListBuilder, RenderCommand,
    RenderList,
};
pub use geometry::{Rect, Transform2D};
pub use paint::{
    Border, BoxShadow, Color, CornerRadii, ImageFit, ImageStyle, ShapePaint, TextAlign, TextSpan,
    TextStyle, TextWrap,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiImage {
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
}

impl UiImage {
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        if pixels.len() != expected || width == 0 || height == 0 {
            return None;
        }
        Some(Self {
            width,
            height,
            pixels: Arc::from(pixels.into_boxed_slice()),
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn cache_key(&self) -> usize {
        Arc::as_ptr(&self.pixels) as *const u8 as usize
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspacePreviewImage {
    pub workspace: WorkspaceId,
    pub image: UiImage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiAction {
    CloseOverview,
    FocusWorkspace(WorkspaceId),
    FocusWindow(WindowId),
    RestoreWindow(WindowId),
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct HitTarget {
    bounds: Rect,
    action: UiAction,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UiNode {
    Panel {
        id: NodeId,
        bounds: Rect,
        color: Color,
    },
    Label {
        id: NodeId,
        bounds: Rect,
        color: Color,
        text: String,
    },
    Image {
        id: NodeId,
        bounds: Rect,
        image: UiImage,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiScene {
    revision: u64,
    clear_color: Color,
    nodes: Vec<UiNode>,
    targets: Vec<HitTarget>,
    search_actions: Vec<UiAction>,
}

const MAX_OVERVIEW_WINDOWS: usize = 32;
const MAX_OVERVIEW_WORKSPACES: usize = 10;
const MAX_SEARCH_RESULTS: usize = 12;
pub const MAX_SEARCH_QUERY: usize = 64;
const OVERVIEW_COLUMNS: usize = 4;
const OVERVIEW_STATUS_NODE_ID_OFFSET: u64 = 10_000;

fn window_label(window: &WindowSummary) -> String {
    let label = if window.title.is_empty() {
        window.app_id.as_str()
    } else {
        window.title.as_str()
    };
    bounded_text(label, 48)
}

fn bounded_text(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

#[derive(Clone, Debug)]
struct SearchResult {
    title: String,
    detail: String,
    action: UiAction,
}

fn search_results(snapshot: &DesktopSnapshot, query: &str) -> Vec<SearchResult> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::with_capacity(MAX_SEARCH_RESULTS);
    for window in &snapshot.windows {
        let title = window_label(window);
        let app_id = if window.app_id.is_empty() {
            "Application".to_owned()
        } else {
            bounded_text(&window.app_id, 32)
        };
        let haystack =
            format!("{} {} workspace {}", title, app_id, window.workspace.0).to_lowercase();
        if haystack.contains(&needle) {
            let detail = if window.minimized {
                format!("{} · Restore minimized window", app_id)
            } else {
                format!("{} · Workspace {}", app_id, window.workspace.0)
            };
            results.push(SearchResult {
                title,
                detail: bounded_text(&detail, 64),
                action: if window.minimized {
                    UiAction::RestoreWindow(window.id)
                } else {
                    UiAction::FocusWindow(window.id)
                },
            });
            if results.len() == MAX_SEARCH_RESULTS {
                return results;
            }
        }
    }

    for workspace in &snapshot.workspaces {
        let title = format!("Workspace {}", workspace.workspace.0);
        if title.to_lowercase().contains(&needle) {
            results.push(SearchResult {
                title,
                detail: "Switch workspace".into(),
                action: UiAction::FocusWorkspace(workspace.workspace),
            });
            if results.len() == MAX_SEARCH_RESULTS {
                return results;
            }
        }
    }

    if "close overview".contains(&needle) || "escape".contains(&needle) {
        results.push(SearchResult {
            title: "Close overview".into(),
            detail: "Return to the desktop".into(),
            action: UiAction::CloseOverview,
        });
    }
    results
}

impl UiScene {
    pub fn new(revision: u64) -> Self {
        Self {
            revision,
            clear_color: Color::BACKGROUND,
            nodes: Vec::new(),
            targets: Vec::new(),
            search_actions: Vec::new(),
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn clear_color(&self) -> Color {
        self.clear_color
    }

    pub fn set_clear_color(&mut self, color: Color) {
        self.clear_color = color;
    }

    pub fn nodes(&self) -> &[UiNode] {
        &self.nodes
    }

    pub fn push(&mut self, node: UiNode) {
        self.nodes.push(node);
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<UiAction> {
        self.targets
            .iter()
            .rev()
            .find(|target| {
                x >= target.bounds.x
                    && x < target.bounds.x + target.bounds.width
                    && y >= target.bounds.y
                    && y < target.bounds.y + target.bounds.height
            })
            .map(|target| target.action)
    }

    pub fn search_action(&self, index: usize) -> Option<UiAction> {
        self.search_actions.get(index).copied()
    }

    pub fn search_result_count(&self) -> usize {
        self.search_actions.len()
    }

    fn target(&mut self, bounds: Rect, action: UiAction) {
        self.targets.push(HitTarget { bounds, action });
    }
    fn add_search_results(
        &mut self,
        snapshot: &DesktopSnapshot,
        query: &str,
        selected: usize,
        width: f32,
    ) {
        let results = search_results(snapshot, query);
        let panel_width = width.clamp(1.0, 690.0);
        let panel_x = ((width - panel_width) / 2.0).max(0.0);
        let row_height = 44.0;
        let panel_height = ((results.len().max(1) as f32) * row_height + 16.0).min(360.0);
        let panel = Rect::new(panel_x, 112.0, panel_width, panel_height);
        self.push(UiNode::Panel {
            id: NodeId(3_000),
            bounds: panel,
            color: Color::rgba(26, 36, 48, 245),
        });

        if results.is_empty() {
            let bounds = Rect::new(panel.x + 16.0, panel.y + 8.0, panel.width - 32.0, 28.0);
            self.push(UiNode::Label {
                id: NodeId(3_001),
                bounds,
                color: Color::TEXT,
                text: "No matching windows, workspaces, or actions".into(),
            });
            return;
        }

        let selected = selected.min(results.len() - 1);
        for (index, result) in results.into_iter().enumerate() {
            let bounds = Rect::new(
                panel.x + 8.0,
                panel.y + 8.0 + index as f32 * row_height,
                (panel.width - 16.0).max(1.0),
                40.0,
            );
            self.push(UiNode::Panel {
                id: NodeId(3_100 + index as u64),
                bounds,
                color: if index == selected {
                    Color::rgba(61, 90, 117, 255)
                } else {
                    Color::rgba(26, 36, 48, 0)
                },
            });
            let text = bounded_text(&format!("{} · {}", result.title, result.detail), 96);
            self.push(UiNode::Label {
                id: NodeId(3_200 + index as u64),
                bounds: Rect::new(
                    bounds.x + 10.0,
                    bounds.y + 4.0,
                    (bounds.width - 20.0).max(1.0),
                    32.0,
                ),
                color: Color::TEXT,
                text,
            });
            self.search_actions.push(result.action);
            self.target(bounds, result.action);
        }
    }

    pub fn bar(revision: u64, width: f32, height: f32) -> Self {
        let mut scene = Self::new(revision);
        scene.push(UiNode::Panel {
            id: NodeId(1),
            bounds: Rect::new(0.0, 0.0, width, height),
            color: Color::BACKGROUND,
        });
        scene.push(UiNode::Label {
            id: NodeId(2),
            bounds: Rect::new(16.0, 0.0, width - 32.0, height),
            color: Color::TEXT,
            text: "Knave".into(),
        });
        scene
    }

    pub fn bar_with_snapshot(
        revision: u64,
        width: f32,
        height: f32,
        snapshot: Option<&DesktopSnapshot>,
    ) -> Self {
        let mut scene = Self::bar(revision, width, height);
        if let Some(UiNode::Label { text, .. }) = scene.nodes.get_mut(1) {
            *text = snapshot
                .and_then(|snapshot| {
                    snapshot
                        .workspaces
                        .iter()
                        .find(|workspace| workspace.active)
                })
                .map_or_else(
                    || "Knave".into(),
                    |workspace| format!("Workspace {}", workspace.workspace.0),
                );
        }
        if let Some(snapshot) = snapshot {
            for (index, workspace) in snapshot.workspaces.iter().enumerate() {
                let bounds = Rect::new(150.0 + index as f32 * 38.0, 5.0, 32.0, height - 10.0);
                scene.push(UiNode::Label {
                    id: NodeId(10 + index as u64),
                    bounds,
                    color: if workspace.active {
                        Color::ACCENT
                    } else {
                        Color::TEXT
                    },
                    text: workspace.workspace.0.to_string(),
                });
                scene.target(bounds, UiAction::FocusWorkspace(workspace.workspace));
            }
        }
        scene
    }

    pub fn overview(revision: u64, width: f32, height: f32) -> Self {
        let mut scene = Self::overview_background(revision, width, height);
        scene.append_status_bar(width, None);
        scene
    }

    fn overview_background(revision: u64, width: f32, height: f32) -> Self {
        let mut scene = Self::new(revision);
        scene.set_clear_color(Color::rgba(8, 12, 18, 245));
        scene.push(UiNode::Panel {
            id: NodeId(1),
            bounds: Rect::new(0.0, 0.0, width, height),
            color: Color::BACKGROUND,
        });
        scene.target(Rect::new(0.0, 0.0, width, height), UiAction::CloseOverview);
        scene
    }

    fn append_status_bar(&mut self, width: f32, snapshot: Option<&DesktopSnapshot>) {
        let mut status_bar = Self::bar_with_snapshot(self.revision, width, 36.0, snapshot);
        for node in &mut status_bar.nodes {
            match node {
                UiNode::Panel { id, .. } | UiNode::Label { id, .. } | UiNode::Image { id, .. } => {
                    id.0 += OVERVIEW_STATUS_NODE_ID_OFFSET
                }
            }
        }
        self.nodes.append(&mut status_bar.nodes);
        self.targets.append(&mut status_bar.targets);
    }
    pub fn overview_with_snapshot(
        revision: u64,
        width: f32,
        height: f32,
        snapshot: Option<&DesktopSnapshot>,
    ) -> Self {
        Self::overview_with_snapshot_and_search(revision, width, height, snapshot, "", 0, &[])
    }

    pub fn overview_with_snapshot_and_search(
        revision: u64,
        width: f32,
        height: f32,
        snapshot: Option<&DesktopSnapshot>,
        query: &str,
        selected: usize,
        previews: &[WorkspacePreviewImage],
    ) -> Self {
        let mut scene = Self::overview_background(revision, width, height);
        scene.append_status_bar(width, snapshot);
        if let Some(snapshot) = snapshot
            && let Some(workspace) = snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.active)
        {
            scene.push(UiNode::Label {
                id: NodeId(100),
                bounds: Rect::new(32.0, 64.0, width - 64.0, height - 96.0),
                color: Color::TEXT,
                text: if query.is_empty() {
                    format!("Overview · {} windows", workspace.window_count)
                } else {
                    format!("Search: {}", bounded_text(query, MAX_SEARCH_QUERY))
                },
            });

            let gap = 16.0;
            let workspace_count = snapshot.workspaces.len().min(MAX_OVERVIEW_WORKSPACES);
            let mut window_top = 112.0;
            if workspace_count > 0 {
                let preview_width = ((width - 64.0 - gap * (workspace_count as f32 - 1.0))
                    / workspace_count as f32)
                    .max(1.0);
                let preview_height = (preview_width * 9.0 / 16.0).clamp(90.0, 260.0);
                for (index, workspace) in
                    snapshot.workspaces.iter().take(workspace_count).enumerate()
                {
                    let bounds = Rect::new(
                        32.0 + index as f32 * (preview_width + gap),
                        112.0,
                        preview_width,
                        preview_height,
                    );
                    scene.push(UiNode::Panel {
                        id: NodeId(500 + index as u64),
                        bounds,
                        color: if workspace.active {
                            Color::ACCENT
                        } else {
                            Color::rgba(35, 48, 64, 255)
                        },
                    });
                    if let Some(preview) = previews
                        .iter()
                        .find(|preview| preview.workspace == workspace.workspace)
                    {
                        scene.push(UiNode::Image {
                            id: NodeId(600 + index as u64),
                            bounds: Rect::new(
                                bounds.x + 2.0,
                                bounds.y + 2.0,
                                (bounds.width - 4.0).max(1.0),
                                (bounds.height - 28.0).max(1.0),
                            ),
                            image: preview.image.clone(),
                        });
                    }
                    scene.push(UiNode::Label {
                        id: NodeId(700 + index as u64),
                        bounds: Rect::new(
                            bounds.x + 8.0,
                            bounds.y + bounds.height - 24.0,
                            (bounds.width - 16.0).max(1.0),
                            20.0,
                        ),
                        color: Color::TEXT,
                        text: format!("Workspace {}", workspace.workspace.0),
                    });
                    scene.target(bounds, UiAction::FocusWorkspace(workspace.workspace));
                }
                window_top = 112.0 + preview_height + 40.0;
            }

            let card_width = ((width - 64.0 - gap * (OVERVIEW_COLUMNS as f32 - 1.0))
                / OVERVIEW_COLUMNS as f32)
                .max(1.0);
            let card_height = 72.0;
            for (index, window) in snapshot
                .windows
                .iter()
                .filter(|window| window.workspace == workspace.workspace)
                .take(MAX_OVERVIEW_WINDOWS)
                .enumerate()
            {
                let column = index % OVERVIEW_COLUMNS;
                let row = index / OVERVIEW_COLUMNS;
                let bounds = Rect::new(
                    32.0 + column as f32 * (card_width + gap),
                    window_top + row as f32 * (card_height + gap),
                    card_width,
                    card_height,
                );
                scene.push(UiNode::Panel {
                    id: NodeId(1_000 + index as u64),
                    bounds,
                    color: if window.focused {
                        Color::ACCENT
                    } else {
                        Color::rgba(35, 48, 64, 255)
                    },
                });
                scene.push(UiNode::Label {
                    id: NodeId(2_000 + index as u64),
                    bounds: Rect::new(
                        bounds.x + 12.0,
                        bounds.y + 8.0,
                        (bounds.width - 24.0).max(1.0),
                        (bounds.height - 16.0).max(1.0),
                    ),
                    color: Color::TEXT,
                    text: window_label(window),
                });
                scene.target(
                    bounds,
                    if window.minimized {
                        UiAction::RestoreWindow(window.id)
                    } else {
                        UiAction::FocusWindow(window.id)
                    },
                );
            }
            if !query.is_empty() {
                scene.add_search_results(snapshot, query, selected, width);
            }
        }
        scene
    }
}

impl Default for UiScene {
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_scene_is_renderer_independent() {
        let scene = UiScene::bar(7, 1920.0, 36.0);
        assert_eq!(scene.revision(), 7);
        assert_eq!(scene.nodes().len(), 2);
    }

    #[test]
    fn overview_scene_does_not_require_a_gpu() {
        let scene = UiScene::overview(1, 1920.0, 1080.0);
        assert_eq!(scene.clear_color(), Color::rgba(8, 12, 18, 245));
        assert_eq!(scene.nodes().len(), 3);
        assert_eq!(
            scene.nodes()[0],
            UiNode::Panel {
                id: NodeId(1),
                bounds: Rect::new(0.0, 0.0, 1920.0, 1080.0),
                color: Color::BACKGROUND,
            }
        );
        assert!(matches!(
            &scene.nodes()[1],
            UiNode::Panel { bounds, .. }
                if *bounds == Rect::new(0.0, 0.0, 1920.0, 36.0)
        ));
        assert!(matches!(
            &scene.nodes()[2],
            UiNode::Label { text, .. } if text == "Knave"
        ));
    }
    #[test]
    fn snapshot_scene_reflects_active_workspace() {
        let snapshot = DesktopSnapshot {
            generation: 4,
            overview_visible: true,
            workspaces: vec![knave_desktop_api::WorkspaceSummary {
                workspace: knave_desktop_api::WorkspaceId(2),
                active: true,
                window_count: 3,
                visible_window_count: 2,
            }],
            windows: vec![
                WindowSummary {
                    id: WindowId(41),
                    title: "Terminal".into(),
                    app_id: "foot".into(),
                    workspace: WorkspaceId(2),
                    focused: true,
                    minimized: false,
                    floating: false,
                    fullscreen: false,
                    maximized: false,
                },
                WindowSummary {
                    id: WindowId(42),
                    title: "Editor".into(),
                    app_id: "code".into(),
                    workspace: WorkspaceId(2),
                    focused: false,
                    minimized: true,
                    floating: false,
                    fullscreen: false,
                    maximized: false,
                },
            ],
        };
        let bar = UiScene::bar_with_snapshot(4, 1920.0, 36.0, Some(&snapshot));
        assert!(matches!(&bar.nodes()[1], UiNode::Label { text, .. } if text == "Workspace 2"));
        let overview = UiScene::overview_with_snapshot(4, 1920.0, 1080.0, Some(&snapshot));
        assert!(overview.nodes().iter().any(|node| matches!(node, UiNode::Label { bounds, text, .. } if *bounds == Rect::new(16.0, 0.0, 1888.0, 36.0) && text == "Workspace 2")));
        assert_eq!(
            overview.hit_test(40.0, 420.0),
            Some(UiAction::FocusWindow(WindowId(41)))
        );
        assert_eq!(
            overview.hit_test(500.0, 420.0),
            Some(UiAction::RestoreWindow(WindowId(42)))
        );
        assert_eq!(
            overview.hit_test(160.0, 15.0),
            Some(UiAction::FocusWorkspace(WorkspaceId(2)))
        );
        assert_eq!(overview.hit_test(40.0, 25.0), Some(UiAction::CloseOverview));
        assert_eq!(
            overview.hit_test(1000.0, 700.0),
            Some(UiAction::CloseOverview)
        );
    }

    #[test]
    fn overview_scene_includes_bounded_workspace_preview() {
        let snapshot = DesktopSnapshot {
            generation: 1,
            overview_visible: true,
            workspaces: vec![knave_desktop_api::WorkspaceSummary {
                workspace: WorkspaceId(1),
                active: true,
                window_count: 0,
                visible_window_count: 0,
            }],
            windows: Vec::new(),
        };
        let preview = WorkspacePreviewImage {
            workspace: WorkspaceId(1),
            image: UiImage::from_rgba(320, 180, vec![0; 320 * 180 * 4]).unwrap(),
        };
        let scene = UiScene::overview_with_snapshot_and_search(
            1,
            1920.0,
            1080.0,
            Some(&snapshot),
            "",
            0,
            &[preview],
        );

        assert!(
            scene
                .nodes()
                .iter()
                .any(|node| matches!(node, UiNode::Image {
            image,
            ..
        } if image.width() == 320 && image.height() == 180))
        );
    }

    #[test]
    fn search_results_provide_bounded_actions() {
        let snapshot = DesktopSnapshot {
            generation: 1,
            overview_visible: true,
            workspaces: vec![knave_desktop_api::WorkspaceSummary {
                workspace: WorkspaceId(1),
                active: true,
                window_count: 1,
                visible_window_count: 1,
            }],
            windows: vec![WindowSummary {
                id: WindowId(9),
                title: "Editor".into(),
                app_id: "code".into(),
                workspace: WorkspaceId(1),
                focused: false,
                minimized: true,
                floating: false,
                fullscreen: false,
                maximized: false,
            }],
        };
        let scene = UiScene::overview_with_snapshot_and_search(
            1,
            1920.0,
            1080.0,
            Some(&snapshot),
            "editor",
            0,
            &[],
        );
        assert_eq!(scene.search_result_count(), 1);
        assert_eq!(
            scene.search_action(0),
            Some(UiAction::RestoreWindow(WindowId(9)))
        );
        assert_eq!(
            scene.hit_test(960.0, 130.0),
            Some(UiAction::RestoreWindow(WindowId(9)))
        );
    }
}
