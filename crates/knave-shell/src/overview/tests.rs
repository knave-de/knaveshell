use super::*;
use knave_desktop_api::{WindowSummary, WorkspaceSummary};
use knave_ui::{TextSpan, TextStyle};
struct Metrics;
impl TextMeasurer for Metrics {
    fn measure(&mut self, text: &str, style: &TextStyle, width: f32) -> TextLayout {
        TextLayout {
            size: [(text.len() as f32 * 7.0).min(width), style.line_height],
            carets: vec![],
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
fn snapshot() -> DesktopSnapshot {
    DesktopSnapshot {
        generation: 7,
        workspaces: (1..=3)
            .map(|i| WorkspaceSummary {
                workspace: WorkspaceId(i),
                active: i == 2,
                window_count: 4,
                visible_window_count: 1,
            })
            .collect(),
        windows: (1..=14)
            .map(|id| WindowSummary {
                id: WindowId(id),
                title: format!("Window {id}"),
                app_id: "Editor".into(),
                workspace: WorkspaceId(if id == 14 { 3 } else { 2 }),
                focused: id == 1,
                minimized: id != 1,
                floating: false,
                fullscreen: false,
                maximized: false,
            })
            .collect(),
    }
}
fn app() -> Overview {
    let mut app = Overview::new();
    app.desktop_snapshot(&snapshot());
    app.frame([1200.0, 800.0], &mut Metrics);
    app
}
fn key(app: &mut Overview, key: Key, pressed: bool) {
    app.input(Input::Key {
        key,
        pressed,
        repeat: false,
        modifiers: KeyModifiers::default(),
    });
}
fn click(app: &mut Overview, id: ElementId) {
    let r = app.scene.bounds(id).unwrap();
    let p = [r.x + r.width / 2.0, r.y + r.height / 2.0];
    app.input(Input::PointerDown(p));
    app.input(Input::PointerUp(p));
}
#[test]
fn minimized_shelf_is_workspace_scoped_and_paginated() {
    let mut app = app();
    assert!(app.scene.bounds(app.window_ids[&WindowId(1)]).is_none());
    assert!(app.scene.bounds(app.window_ids[&WindowId(14)]).is_none());
    assert!(app.scene.bounds(app.window_ids[&WindowId(2)]).is_some());
    click(&mut app, ElementId(31));
    app.frame([1200.0, 800.0], &mut Metrics);
    assert!(app.scene.bounds(app.window_ids[&WindowId(2)]).is_none());
    assert!(app.scene.bounds(app.window_ids[&WindowId(7)]).is_some());
}
#[test]
fn browsing_does_not_dispatch_or_close_and_enter_targets_browsed_workspace() {
    let mut app = app();
    key(&mut app, Key::Right, true);
    app.frame([1200.0, 800.0], &mut Metrics);
    assert_eq!(app.workspace, Some(WorkspaceId(3)));
    assert!(app.take_request().is_none());
    assert!(!app.should_close());
    key(&mut app, Key::Enter, true);
    key(&mut app, Key::Enter, false);
    assert!(matches!(
        app.take_request(),
        Some(HostRequest::Desktop(DesktopCommand::FocusWorkspace {
            workspace: WorkspaceId(3)
        }))
    ));
    assert!(!app.should_close());
    app.desktop_action_finished(Ok(()));
    assert!(app.should_close());
}
#[test]
fn restore_waits_for_ack_and_failure_keeps_overview_open() {
    let mut app = app();
    let id = app.window_ids[&WindowId(2)];
    click(&mut app, id);
    assert!(matches!(
        app.take_request(),
        Some(HostRequest::Desktop(DesktopCommand::RestoreWindow {
            window: WindowId(2)
        }))
    ));
    assert!(!app.should_close());
    click(&mut app, id);
    assert!(app.take_request().is_none());
    app.desktop_action_finished(Err("Window no longer exists".into()));
    assert!(!app.should_close());
    assert!(!app.pending);
    assert!(app.error.is_some());
}
#[test]
fn snapshots_with_same_generation_update_membership_and_preserve_browsing() {
    let mut app = app();
    app.browse(WorkspaceId(3));
    let mut next = snapshot();
    next.windows.retain(|w| w.id != WindowId(2));
    next.workspaces[0].active = true;
    next.workspaces[1].active = false;
    app.desktop_snapshot(&next);
    app.frame([1200.0, 800.0], &mut Metrics);
    assert_eq!(app.workspace, Some(WorkspaceId(3)));
    assert!(!app.window_ids.contains_key(&WindowId(2)));
    next.workspaces.pop();
    app.desktop_snapshot(&next);
    assert_eq!(app.workspace, Some(WorkspaceId(1)));
}
#[test]
fn search_is_unicode_editable_and_escape_clears_before_closing() {
    let mut app = app();
    app.input(Input::Text("Editor".into()));
    app.frame([1200.0, 800.0], &mut Metrics);
    assert_eq!(app.query.text(), "Editor");
    assert_eq!(app.scene.focus(), Some(SEARCH));
    key(&mut app, Key::Escape, true);
    app.frame([1200.0, 800.0], &mut Metrics);
    assert!(!app.should_close());
    assert_eq!(app.query.text(), "");
    key(&mut app, Key::Escape, true);
    assert!(app.should_close());
}
#[test]
fn idle_frames_reuse_layout_and_panes_are_bounded_to_neighbors() {
    let mut app = app();
    let stats = app.scene.stats();
    assert!(!app.needs_frame());
    app.frame([1200.0, 800.0], &mut Metrics);
    assert_eq!(app.scene.stats(), stats);
    let panes = app.overview_panes().unwrap();
    assert_eq!(panes.len(), 3);
    assert_eq!(
        panes.iter().map(|pane| pane.workspace).collect::<Vec<_>>(),
        vec![WorkspaceId(1), WorkspaceId(3), WorkspaceId(2)]
    );
    assert!(panes.iter().all(|pane| pane.width > 0 && pane.height > 0));
    app.browse(WorkspaceId(3));
    app.frame([1200.0, 800.0], &mut Metrics);
    assert_eq!(
        app.overview_panes()
            .unwrap()
            .iter()
            .map(|pane| pane.workspace)
            .collect::<Vec<_>>(),
        vec![WorkspaceId(2), WorkspaceId(3)]
    );
}

#[test]
fn layouts_fit_compact_and_portrait_outputs_with_unique_control_ids() {
    for size in [
        [800.0, 600.0],
        [600.0, 900.0],
        [3840.0, 2160.0],
        [320.0, 240.0],
    ] {
        let mut app = app();
        app.frame(size, &mut Metrics);
        assert!(app.error.is_none(), "{size:?}: {:?}", app.error);
        app.input(Input::Text("Editor".into()));
        app.frame(size, &mut Metrics);
        assert!(app.error.is_none());
        click(&mut app, ElementId(31));
        app.frame(size, &mut Metrics);
        assert!(app.error.is_none());
    }
}
#[test]
fn disconnected_overview_cannot_dispatch_stale_window_actions() {
    let mut app = app();
    app.desktop_unavailable();
    let id = app.window_ids[&WindowId(2)];
    click(&mut app, id);
    assert!(app.take_request().is_none());
    app.desktop_snapshot(&snapshot());
    assert!(app.connected);
    assert!(app.error.is_none());
}
