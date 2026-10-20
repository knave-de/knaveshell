use super::*;
use knave_desktop_api::{WindowSummary, WorkspaceSummary};
use knave_ui::{Color, Rect, TextSpan, TextStyle, UiNode, UiScene};
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
        overview_visible: true,
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
/// Removes a fixture directory when the owning test thread ends.
struct TempDir(std::path::PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
thread_local! {
    static FIXTURES: std::cell::RefCell<Vec<TempDir>> = const { std::cell::RefCell::new(Vec::new()) };
}
fn fixture_dir(label: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let root = std::env::temp_dir().join(format!(
        "knave-shell-{label}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    FIXTURES.with(|f| f.borrow_mut().push(TempDir(root.clone())));
    root
}
/// Twelve "Tool NN" entries exercise paging; "Editor" checks Exec expansion.
/// Tool 12 names an icon that does not exist, to exercise the fallback.
fn apps_root() -> std::path::PathBuf {
    let root = fixture_dir("apps");
    let dir = root.join("applications");
    std::fs::create_dir_all(&dir).unwrap();
    let entry = |name: &str, exec: &str, icon: &str| {
        format!(
            "[Desktop Entry]\nType=Application\nName={name}\nExec={exec}\nIcon={icon}\nComment=About {name}\n"
        )
    };
    std::fs::write(
        dir.join("editor.desktop"),
        entry("Editor", "editor --new %U", "editor-icon"),
    )
    .unwrap();
    for n in 1..=12 {
        let icon = if n == 12 {
            "ghost".into()
        } else {
            format!("tool-icon-{n:02}")
        };
        std::fs::write(
            dir.join(format!("tool{n:02}.desktop")),
            entry(&format!("Tool {n:02}"), &format!("tool{n:02}"), &icon),
        )
        .unwrap();
    }
    root
}
fn apps_environment() -> knave_apps::Environment {
    knave_apps::Environment {
        data_dirs: vec![apps_root()],
        ..Default::default()
    }
}
fn catalog() -> knave_apps::Catalog {
    knave_apps::Catalog::load_with(&apps_environment()).unwrap()
}
fn png_bytes(rgba: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, 48, 48);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&(0..48 * 48).flat_map(|_| rgba).collect::<Vec<u8>>())
        .unwrap();
    writer.finish().unwrap();
    out
}
/// A one-directory theme holding an icon for each fixture app plus the fallback.
fn icon_lookup(with_fallback: bool) -> knave_icons::IconLookup {
    let root = fixture_dir("icons");
    let theme = root.join("hicolor");
    std::fs::create_dir_all(theme.join("48x48/apps")).unwrap();
    std::fs::write(
        theme.join("index.theme"),
        "[Icon Theme]\nDirectories=48x48/apps\n[48x48/apps]\nSize=48\nType=Fixed\n",
    )
    .unwrap();
    let mut names: Vec<String> = (1..=11).map(|n| format!("tool-icon-{n:02}")).collect();
    names.push("editor-icon".into());
    if with_fallback {
        names.push("application-x-executable".into());
    }
    for name in names {
        std::fs::write(
            theme.join(format!("48x48/apps/{name}.png")),
            png_bytes([0, 200, 0, 255]),
        )
        .unwrap();
    }
    knave_icons::IconLookup::new(vec![root], vec![])
}
fn app() -> Overview {
    let mut app = Overview::new()
        .with_catalog(catalog())
        .with_icon_lookup(icon_lookup(true));
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
    assert!(matches!(
        app.take_request(),
        Some(HostRequest::Desktop(DesktopCommand::SetOverviewVisible {
            visible: false
        }))
    ));
    assert!(!app.should_close());
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
fn search_is_unicode_editable_and_escape_hides_overview() {
    let mut app = app();
    app.input(Input::Text("Editor".into()));
    settle(&mut app, [1200.0, 800.0]);
    assert_eq!(app.query.text(), "Editor");
    assert_eq!(app.scene.focus(), Some(SEARCH));
    key(&mut app, Key::Escape, true);
    app.frame([1200.0, 800.0], &mut Metrics);
    assert!(matches!(
        app.take_request(),
        Some(HostRequest::Desktop(DesktopCommand::SetOverviewVisible {
            visible: false
        }))
    ));
    assert!(!app.should_close());
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
        app.input(Input::Text("Tool".into()));
        settle(&mut app, size);
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
    app.activate(id);
    assert!(app.take_request().is_none());
    app.desktop_snapshot(&snapshot());
    assert!(app.connected);
    assert!(app.error.is_none());
}

#[test]
fn disconnection_unmaps_and_cancels_unsent_actions_then_reconnects() {
    let mut app = app();
    assert!(app.surface_visible());
    app.dispatch(DesktopCommand::FocusWorkspace {
        workspace: WorkspaceId(3),
    });
    app.desktop_unavailable();
    assert!(!app.surface_visible());
    assert!(!app.should_close());
    assert!(app.take_request().is_none());
    assert!(!app.pending);
    app.desktop_action_finished(Ok(()));
    assert!(app.take_request().is_none());
    app.desktop_snapshot(&snapshot());
    assert!(app.surface_visible());
}

#[test]
fn completing_activation_while_hidden_does_not_hide_the_next_opening() {
    let mut app = app();
    app.dispatch(DesktopCommand::FocusWorkspace {
        workspace: WorkspaceId(3),
    });
    assert!(app.take_request().is_some());
    let mut hidden = snapshot();
    hidden.overview_visible = false;
    app.desktop_snapshot(&hidden);
    app.desktop_action_finished(Ok(()));
    assert!(!app.pending);
    assert!(app.take_request().is_none());
    app.desktop_snapshot(&snapshot());
    assert!(app.surface_visible());
    assert!(app.take_request().is_none());
}

#[test]
fn disconnected_inflight_action_is_completed_without_a_stale_hide() {
    let mut app = app();
    app.dispatch(DesktopCommand::FocusWorkspace {
        workspace: WorkspaceId(3),
    });
    assert!(app.take_request().is_some());
    app.desktop_unavailable();
    app.desktop_snapshot(&snapshot());
    assert!(app.pending);
    app.desktop_action_finished(Ok(()));
    assert!(!app.pending);
    assert!(app.take_request().is_none());
}

#[test]
fn activation_from_an_earlier_opening_cannot_hide_a_reopened_overview() {
    let mut app = app();
    app.dispatch(DesktopCommand::FocusWorkspace {
        workspace: WorkspaceId(3),
    });
    assert!(app.take_request().is_some());
    let mut hidden = snapshot();
    hidden.overview_visible = false;
    app.desktop_snapshot(&hidden);
    app.desktop_snapshot(&snapshot());
    app.desktop_action_finished(Ok(()));
    assert!(app.surface_visible());
    assert!(app.take_request().is_none());
    assert!(!app.pending);
}

/// Draw frames until the worker has delivered everything it owes; fails instead of hanging.
fn settle(app: &mut Overview, size: [f32; 2]) {
    for _ in 0..5000 {
        app.frame(size, &mut Metrics);
        if !app.is_loading() && !app.needs_frame() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("loader did not settle");
}
fn type_query(app: &mut Overview, query: &str) {
    app.input(Input::Text(query.into()));
    settle(app, [1200.0, 800.0]);
}
fn spawned(app: &mut Overview) -> Option<Vec<String>> {
    match app.take_request() {
        Some(HostRequest::Desktop(DesktopCommand::Spawn { argv })) => Some(argv),
        _ => None,
    }
}
#[test]
fn enter_launches_the_best_application_match_and_hides_after_ack() {
    let mut app = app();
    type_query(&mut app, "edit");
    key(&mut app, Key::Enter, true);
    key(&mut app, Key::Enter, false);
    assert_eq!(
        spawned(&mut app),
        Some(vec!["editor".into(), "--new".into()])
    );
    assert!(!app.should_close());
    app.desktop_action_finished(Ok(()));
    assert!(matches!(
        app.take_request(),
        Some(HostRequest::Desktop(DesktopCommand::SetOverviewVisible {
            visible: false
        }))
    ));
    app.desktop_action_finished(Ok(()));
    assert!(!app.should_close());
}
#[test]
fn clicking_a_result_launches_it_and_failure_keeps_overview_open() {
    let mut app = app();
    type_query(&mut app, "tool 03");
    let id = app_element(app.search()[0]);
    click(&mut app, id);
    assert_eq!(spawned(&mut app), Some(vec!["tool03".into()]));
    app.desktop_action_finished(Err("No such file".into()));
    assert!(!app.should_close());
    assert_eq!(app.error.as_deref(), Some("No such file"));
}
#[test]
fn results_are_applications_only_and_paginate() {
    {
        let mut app = app();
        // Window titles and app IDs from the snapshot must not appear as results.
        type_query(&mut app, "Window");
        assert!(app.search().is_empty());
    }
    // Use a fresh instance for a separate query, independently of visibility actions.
    let mut paginated = app();
    type_query(&mut paginated, "tool");
    assert_eq!(paginated.search().len(), 12);
    let size = paginated.search_page_size();
    assert_eq!(size, 8);
    let first_page: Vec<_> = paginated.search()[..size].to_vec();
    assert!(
        first_page
            .iter()
            .all(|&i| paginated.scene.bounds(app_element(i)).is_some())
    );
    click(&mut paginated, ElementId(31));
    paginated.frame([1200.0, 800.0], &mut Metrics);
    assert!(
        first_page
            .iter()
            .all(|&i| paginated.scene.bounds(app_element(i)).is_none())
    );
    assert!(
        paginated
            .scene
            .bounds(app_element(paginated.search()[size]))
            .is_some()
    );
}
#[test]
fn disconnected_overview_clears_search_without_launching() {
    let mut app = app();
    type_query(&mut app, "edit");
    app.desktop_unavailable();
    key(&mut app, Key::Enter, true);
    key(&mut app, Key::Enter, false);
    assert!(app.take_request().is_none());
    assert!(app.query.text().is_empty());
    assert!(
        app.error
            .as_deref()
            .unwrap()
            .starts_with("Desktop connection unavailable")
    );
}
#[test]
fn queries_without_matches_render_and_a_missing_catalog_is_reported() {
    let mut app = app();
    type_query(&mut app, "zzzz");
    assert!(app.search().is_empty());
    assert!(app.error.is_none());
    key(&mut app, Key::Enter, true);
    assert!(app.take_request().is_none());
    let mut broken = Overview::new();
    broken.apps = Apps::Unavailable;
    broken.desktop_snapshot(&snapshot());
    broken.frame([1200.0, 800.0], &mut Metrics);
    type_query(&mut broken, "edit");
    assert!(broken.search().is_empty());
    assert!(broken.error.is_none());
}
#[test]
fn enter_right_after_typing_launches_without_waiting_for_a_frame() {
    let mut app = app();
    app.input(Input::Text("edit".into()));
    key(&mut app, Key::Enter, true);
    assert_eq!(
        spawned(&mut app),
        Some(vec!["editor".into(), "--new".into()])
    );
}

fn icon_element(index: usize) -> ElementId {
    ElementId(0x400_0000_0000 + index as u64)
}
#[test]
fn rows_show_an_icon_and_keep_the_text_column_aligned() {
    let mut app = app();
    type_query(&mut app, "edit");
    let index = app.search()[0];
    let row = app.scene.bounds(app_element(index)).unwrap();
    let icon = app.scene.bounds(icon_element(index)).expect("icon element");
    assert_eq!((icon.width, icon.height), (32.0, 32.0));
    assert!(icon.x >= row.x && icon.x + icon.width <= row.x + row.width);
    assert!(icon.y >= row.y && icon.y + icon.height <= row.y + row.height);
    // Clicking the icon activates its row.
    let p = [icon.x + 16.0, icon.y + 16.0];
    app.input(Input::PointerDown(p));
    app.input(Input::PointerUp(p));
    assert_eq!(
        spawned(&mut app),
        Some(vec!["editor".into(), "--new".into()])
    );
}
#[test]
fn icons_are_delivered_by_the_worker_for_visible_rows_only() {
    let mut app = app();
    type_query(&mut app, "tool");
    let ready = |app: &Overview, i: usize| app.scene.bounds(icon_element(i)).is_some();
    let results = app.search();
    assert!(results[..8].iter().all(|&i| ready(&app, i)));
    // Rows on later pages were never requested.
    assert!(results[8..].iter().all(|&i| !ready(&app, i)));
    assert!(!app.is_loading());
    assert!(!app.needs_frame());
}
#[test]
fn enter_while_the_catalog_loads_launches_when_it_arrives() {
    let mut app = Overview::new()
        .with_catalog_environment(apps_environment())
        .with_icon_lookup(icon_lookup(true));
    app.desktop_snapshot(&snapshot());
    app.frame([1200.0, 800.0], &mut Metrics);
    app.input(Input::Text("edit".into()));
    // No frame has absorbed the worker's catalog yet, so Enter must not be lost.
    key(&mut app, Key::Enter, true);
    assert!(app.take_request().is_none());
    assert!(app.launch_when_ready);
    settle(&mut app, [1200.0, 800.0]);
    assert_eq!(
        spawned(&mut app),
        Some(vec!["editor".into(), "--new".into()])
    );
    assert!(!app.launch_when_ready);
    // Editing the query cancels a pending launch.
    let mut app = Overview::new()
        .with_catalog_environment(apps_environment())
        .with_icon_lookup(icon_lookup(true));
    app.desktop_snapshot(&snapshot());
    app.frame([1200.0, 800.0], &mut Metrics);
    app.input(Input::Text("edit".into()));
    key(&mut app, Key::Enter, true);
    app.input(Input::Text("x".into()));
    settle(&mut app, [1200.0, 800.0]);
    assert!(app.take_request().is_none());
}
#[test]
fn a_missing_catalog_directory_is_reported_by_the_worker() {
    let mut app = Overview::new().with_catalog_environment(knave_apps::Environment {
        data_dirs: vec![fixture_dir("empty").join("nowhere")],
        ..Default::default()
    });
    app.desktop_snapshot(&snapshot());
    app.frame([1200.0, 800.0], &mut Metrics);
    type_query(&mut app, "edit");
    assert!(matches!(app.apps, Apps::Unavailable));
    assert!(app.search().is_empty());
}
#[test]
fn missing_icons_use_the_fallback_and_never_fail_the_row() {
    let mut app = app();
    type_query(&mut app, "tool 12");
    let index = app.search()[0];
    assert!(app.scene.bounds(icon_element(index)).is_some());
    // The worker answers a missing name with the theme fallback.
    assert!(app.icons.get("ghost").is_some());
    let mut bare = Overview::new()
        .with_catalog(catalog())
        .with_icon_lookup(icon_lookup(false));
    bare.desktop_snapshot(&snapshot());
    bare.frame([1200.0, 800.0], &mut Metrics);
    type_query(&mut bare, "tool 12");
    let index = bare.search()[0];
    assert!(bare.scene.bounds(app_element(index)).is_some());
    assert!(bare.scene.bounds(icon_element(index)).is_none());
    assert!(bare.error.is_none());
    assert!(!bare.needs_frame());
}
#[test]
fn the_icon_cache_is_bounded_and_requests_are_deduplicated() {
    let mut icons = icons::Icons::default();
    assert!(icons.needs_request("a"));
    icons.requested("a");
    assert!(!icons.needs_request("a"));
    assert!(icons.is_loading());
    for n in 0..400 {
        icons.finish(format!("missing-{n}"), None);
        assert!(icons.len() <= 128);
    }
    icons.finish("a".into(), None);
    assert!(!icons.is_loading());
    assert!(!icons.needs_request("a"));
    assert_eq!(icons.len(), 128);
}
#[test]
fn icons_are_painted_unclipped_not_just_laid_out() {
    let mut app = app();
    type_query(&mut app, "edit");
    let list = app.frame([1200.0, 800.0], &mut Metrics).clone();
    let images: Vec<_> = list
        .commands
        .iter()
        .filter_map(|c| match c {
            knave_ui::DisplayCommand::Image { bounds, clip, .. } => Some((*bounds, *clip)),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 1);
    let (bounds, clip) = images[0];
    // A clip that excludes the image would draw nothing while layout still passed.
    let clip = clip.expect("row clips its children");
    assert!(
        bounds.x >= clip.x
            && bounds.y >= clip.y
            && bounds.x + bounds.width <= clip.x + clip.width
            && bounds.y + bounds.height <= clip.y + clip.height,
        "{bounds:?} outside {clip:?}"
    );
}

#[test]
fn launching_then_reopening_resets_the_search_without_reloading_apps() {
    let mut app = app();
    type_query(&mut app, "edit");
    key(&mut app, Key::Enter, true);
    assert!(spawned(&mut app).is_some());
    app.desktop_action_finished(Ok(()));
    assert!(app.take_request().is_some());
    let mut hidden = snapshot();
    hidden.overview_visible = false;
    app.desktop_snapshot(&hidden);
    app.desktop_action_finished(Ok(()));
    // Input queued for the old widget must not resurrect its text.
    key(&mut app, Key::Enter, false);
    app.input(Input::FocusLost);
    app.desktop_snapshot(&snapshot());
    app.input(Input::FocusGained);
    app.frame([1200.0, 800.0], &mut Metrics);
    assert!(app.query.text().is_empty());
    assert!(
        matches!(app.scene.widget(SEARCH), Some(Widget::TextInput(edit)) if edit.text().is_empty())
    );
    assert!(matches!(app.apps, Apps::Ready(_)));
    assert_eq!(app.workspace, Some(WorkspaceId(2)));
    assert!(!app.panes.is_empty());
    assert!(!app.should_close());
    assert!(app.take_request().is_none());
}

#[test]
fn external_hide_resets_browsing_pages_selection_and_errors() {
    let mut app = app();
    app.browse(WorkspaceId(3));
    type_query(&mut app, "tool");
    app.page = 2;
    app.result_page = 1;
    app.error = Some("Earlier launch failed".into());
    app.background_pressed = true;
    app.pointer = Some([10.0, 10.0]);
    let mut hidden = snapshot();
    hidden.overview_visible = false;
    app.desktop_snapshot(&hidden);
    assert!(app.query.text().is_empty());
    assert_eq!((app.page, app.result_page), (0, 0));
    assert!(app.scene.focus().is_none());
    assert!(app.targets.is_empty());
    assert!(app.error.is_none());
    assert!(!app.background_pressed);
    assert!(app.pointer.is_none());
    let mut reopened = snapshot();
    reopened.workspaces[1].active = false;
    reopened.workspaces[0].active = true;
    app.desktop_snapshot(&reopened);
    app.frame([1200.0, 800.0], &mut Metrics);
    assert_eq!(app.workspace, Some(WorkspaceId(1)));
    assert_eq!((app.page, app.result_page), (0, 0));
    assert_ne!(app.scene.focus(), Some(SEARCH));
}

#[test]
fn hiding_while_catalog_loads_cancels_the_deferred_launch() {
    let mut app = Overview::new()
        .with_catalog_environment(apps_environment())
        .with_icon_lookup(icon_lookup(true));
    app.desktop_snapshot(&snapshot());
    app.frame([1200.0, 800.0], &mut Metrics);
    app.input(Input::Text("edit".into()));
    key(&mut app, Key::Enter, true);
    assert!(app.launch_when_ready);
    let mut hidden = snapshot();
    hidden.overview_visible = false;
    app.desktop_snapshot(&hidden);
    assert!(!app.launch_when_ready);
    app.desktop_snapshot(&snapshot());
    settle(&mut app, [1200.0, 800.0]);
    assert!(app.query.text().is_empty());
    assert!(app.take_request().is_none());
    assert!(matches!(app.apps, Apps::Ready(_)));
}

#[test]
fn live_snapshots_preserve_the_query_until_the_view_is_hidden() {
    let mut app = app();
    type_query(&mut app, "edit");
    let mut next = snapshot();
    next.generation += 1;
    next.windows[0].title = "New title".into();
    app.desktop_snapshot(&next);
    app.frame([1200.0, 800.0], &mut Metrics);
    assert_eq!(app.query.text(), "edit");
    app.desktop_unavailable();
    assert!(app.query.text().is_empty());
    assert!(!app.surface_visible());
    app.desktop_snapshot(&next);
    app.frame([1200.0, 800.0], &mut Metrics);
    assert!(app.query.text().is_empty());
}

#[test]
fn overview_backdrop_preserves_wallpaper_visibility() {
    let mut app = app();
    let list = app.frame([1200.0, 800.0], &mut Metrics);
    assert_eq!(list.clear_color, knave_ui::Color::rgba(0, 0, 0, 0));
    let backdrop = list
        .commands
        .iter()
        .find_map(|command| match command {
            knave_ui::DisplayCommand::Shape { bounds, paint, .. }
                if bounds.width == 1200.0 && bounds.height == 800.0 =>
            {
                Some(paint)
            }
            _ => None,
        })
        .expect("full output backdrop");
    assert_eq!(backdrop.fill, Some(knave_ui::Color::rgba(0, 0, 0, 0)));
}

#[test]
fn workspace_panes_fill_cards_without_reserved_empty_shelf() {
    let mut app = app();
    app.frame([1280.0, 800.0], &mut Metrics);
    for pane in &app.panes {
        let id = ElementId(0x100_0000_0000 + u64::from(pane.workspace.0) * 32);
        let card = app.scene.bounds(id).unwrap();
        assert_eq!(pane.width, (card.width - 4.0).round() as u32);
        assert_eq!(pane.height, (card.height - 4.0).round() as u32);
        assert_eq!(pane.x, (card.x + 2.0).round() as i32);
        assert_eq!(pane.y, (card.y + 2.0).round() as i32);
    }
}

/// Retained controls follow the shared bar's hit-target order.
fn bar_element(node_id: u64) -> ElementId {
    ElementId(0x800_0000_0000 + node_id)
}
fn status_title(app: &Overview) -> Option<String> {
    app.display
        .commands
        .iter()
        .rev()
        .find_map(|command| match command {
            knave_ui::DisplayCommand::Text { text, bounds, .. }
                if bounds.x == 16.0 && bounds.y == 0.0 =>
            {
                Some(text.clone())
            }
            _ => None,
        })
}

#[test]
fn the_overview_routes_the_desktop_status_bar_targets() {
    let mut app = app();
    app.frame([1200.0, 800.0], &mut Metrics);
    // The bar component renders the active workspace, exactly as outside overview.
    assert_eq!(status_title(&app).as_deref(), Some("Workspace 2"));
    for (index, bounds) in [
        Rect::new(150.0, 5.0, 32.0, 26.0),
        Rect::new(188.0, 5.0, 32.0, 26.0),
        Rect::new(226.0, 5.0, 32.0, 26.0),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            app.scene.bounds(bar_element(10 + index as u64)),
            Some(bounds)
        );
    }
    // A chip click switches that workspace directly, like the desktop bar.
    click(&mut app, bar_element(12));
    assert!(matches!(
        app.take_request(),
        Some(HostRequest::Desktop(DesktopCommand::FocusWorkspace {
            workspace: WorkspaceId(3)
        }))
    ));
    app.desktop_action_finished(Ok(()));
    assert!(matches!(
        app.take_request(),
        Some(HostRequest::Desktop(DesktopCommand::SetOverviewVisible {
            visible: false
        }))
    ));
}

#[test]
fn the_shared_bar_title_ignores_overview_browsing() {
    let mut app = app();
    app.browse(WorkspaceId(3));
    app.frame([1200.0, 800.0], &mut Metrics);
    assert_eq!(status_title(&app).as_deref(), Some("Workspace 2"));
    // The bar, including its chips, stays visible while searching.
    type_query(&mut app, "edit");
    assert!(app.scene.bounds(bar_element(12)).is_some());
    let search = app.scene.bounds(SEARCH).unwrap();
    assert!(search.y >= 36.0, "search overlaps the status bar");
}

#[test]
fn shared_bar_text_matches_the_desktop_bar_display_list() {
    let mut app = app();
    let list = app.frame([1200.0, 800.0], &mut Metrics).clone();
    let bar = UiScene::bar_with_snapshot(0, 1200.0, 36.0, Some(app.snapshot().unwrap()));
    for node in bar.nodes().iter() {
        let (bounds, color, text) = match node {
            UiNode::Label {
                bounds,
                color,
                text,
                ..
            } => (*bounds, *color, text.clone()),
            _ => continue,
        };
        let projected = list.commands.iter().find_map(|command| match command {
            knave_ui::DisplayCommand::Text {
                bounds,
                text: candidate,
                style,
                ..
            } if candidate == &text => Some((*bounds, style.color)),
            _ => None,
        });
        assert_eq!(projected, Some((bounds, color)), "label {text:?}");
    }
}

#[test]
fn the_shared_bar_strip_paints_only_the_bar_component_shapes() {
    let mut app = app();
    let list = app.frame([1200.0, 800.0], &mut Metrics).clone();
    // Within the strip the only shapes are the transparent grouping container
    // and the bar component's panel; Button chrome would appear here too.
    let strip: Vec<_> = list
        .commands
        .iter()
        .filter_map(|command| match command {
            knave_ui::DisplayCommand::Shape { bounds, paint, .. }
                if bounds.y <= 0.0 && bounds.height <= 40.0 && bounds.width >= 1100.0 =>
            {
                Some(paint.fill)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        strip,
        vec![Some(Color::rgba(0, 0, 0, 0)), Some(Color::BACKGROUND)],
        "strip shapes"
    );
}

#[test]
fn without_a_snapshot_the_shared_bar_shows_the_session_title() {
    let mut app = Overview::new();
    app.frame([1200.0, 800.0], &mut Metrics);
    assert_eq!(status_title(&app).as_deref(), Some("Knave"));
    assert!(!app.targets.values().any(|target| matches!(
        target,
        Target::StatusBar(knave_ui::UiAction::FocusWorkspace(_))
    )));
}

#[test]
fn overview_renders_the_complete_shared_bar_and_routes_its_bell() {
    let mut app = app();
    app.frame([1200.0, 800.0], &mut Metrics);
    let shared = app.status_bar_component(1200.0);
    assert!(app.display.commands.ends_with(&shared.display.commands));
    let (index, _) = shared
        .scene
        .hit_targets()
        .enumerate()
        .find(|(_, (_, action))| *action == knave_ui::UiAction::ToggleNotifications)
        .unwrap();
    click(&mut app, bar_element(10 + index as u64));
    assert!(matches!(
        app.take_request(),
        Some(HostRequest::Desktop(DesktopCommand::SetOverviewVisible {
            visible: false
        }))
    ));
    app.desktop_action_finished(Ok(()));
    assert!(matches!(
        app.take_request(),
        Some(HostRequest::ToggleNotifications)
    ));
    app.notifications_enabled(false);
    app.frame([1200.0, 800.0], &mut Metrics);
    assert!(
        !app.status_bar_component(1200.0)
            .scene
            .hit_targets()
            .any(|(_, action)| action == knave_ui::UiAction::ToggleNotifications)
    );
    assert!(app.scene.bounds(bar_element(10 + index as u64)).is_none());
}

#[test]
fn overview_tracks_shared_notification_indicators_on_updates_and_disconnect() {
    use knave_desktop_api::notifications::{Notification, NotificationSnapshot};
    let mut app = app();
    let mut state = NotificationSnapshot {
        owned: true,
        dnd: true,
        ..Default::default()
    };
    state.notifications = vec![
        Notification {
            unread: true,
            ..Default::default()
        };
        100
    ];
    app.notification_snapshot(Some(&state));
    app.frame([1200.0, 800.0], &mut Metrics);
    let shared =
        knave_ui::status_bar::StatusBar::new(0, 1200.0, 36.0, app.snapshot(), true, Some(&state));
    assert!(app.display.commands.ends_with(&shared.display.commands));
    for expected in ["99+", "z"] {
        assert!(app.display.commands.iter().any(|command| matches!(command,
            knave_ui::DisplayCommand::Text {text, ..} if text == expected)));
    }
    app.notification_snapshot(None);
    app.frame([1200.0, 800.0], &mut Metrics);
    let shared = knave_ui::status_bar::StatusBar::new(0, 1200.0, 36.0, app.snapshot(), true, None);
    assert!(app.display.commands.ends_with(&shared.display.commands));
    assert!(!app.display.commands.iter().any(|command| matches!(command,
        knave_ui::DisplayCommand::Text {text, ..} if text == "99+" || text == "z")));
}

#[test]
fn failed_overview_hide_does_not_open_notifications_behind_it() {
    let mut app = app();
    app.frame([1200.0, 800.0], &mut Metrics);
    let bell = app
        .targets
        .iter()
        .find_map(|(id, target)| {
            matches!(
                target,
                Target::StatusBar(knave_ui::UiAction::ToggleNotifications)
            )
            .then_some(*id)
        })
        .unwrap();
    click(&mut app, bell);
    app.take_request();
    app.desktop_action_finished(Err("hide failed".into()));
    assert!(app.take_request().is_none());
    assert_eq!(app.error.as_deref(), Some("hide failed"));
}
