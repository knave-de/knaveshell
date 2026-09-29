//! Workspace-first overview with a separate minimized-window shelf.
mod icons;
mod loader;
mod view;
use knave_apps::{Catalog, Environment};
use knave_desktop_api::{
    DesktopCommand, DesktopSnapshot, OverviewPane, WindowId, WindowSummary, WorkspaceId,
};
use knave_icons::IconLookup;
use knave_ui::{DisplayList, toolkit::*};
use knave_wayland::{Application, HostRequest};
use loader::{Done, Job, Loader};
use std::collections::HashMap;

const SEARCH: ElementId = ElementId(2);
const MAX_WINDOWS: usize = 1024;
const MAX_WORKSPACES: usize = 128;
const PAGE_SIZE: usize = 5;
const MAX_RESULTS: usize = 64;

#[derive(Clone, Copy)]
enum Target {
    Workspace(WorkspaceId),
    EnterWorkspace(WorkspaceId),
    PreviewWorkspace(WorkspaceId),
    Window(WindowId),
    App(usize),
    Page(bool),
    ClearSearch,
}

/// The catalog loads on a worker after the first query, so neither startup nor
/// typing ever waits on the filesystem.
enum Apps {
    Unloaded,
    Loading,
    Ready(Catalog),
    Unavailable,
}

fn app_element(index: usize) -> ElementId {
    ElementId(0x300_0000_0000 + index as u64)
}

pub struct Overview {
    snapshot: Option<DesktopSnapshot>,
    panes: Vec<OverviewPane>,
    workspace: Option<WorkspaceId>,
    scene: Scene,
    targets: HashMap<ElementId, Target>,
    window_ids: HashMap<WindowId, ElementId>,
    next_id: u64,
    query: TextEdit,
    apps: Apps,
    icons: icons::Icons,
    loader: Option<Loader>,
    /// Consumed when the worker starts.
    lookup: Option<IconLookup>,
    catalog_env: Option<Environment>,
    waker: Option<knave_wayland::Waker>,
    /// Enter arrived while the catalog was still loading.
    launch_when_ready: bool,
    page: usize,
    result_page: usize,
    connected: bool,
    size: [f32; 2],
    dirty: bool,
    pending: bool,
    request: Option<HostRequest>,
    error: Option<String>,
    close: bool,
    surface_focused: bool,
    background_pressed: bool,
    pressed_pane: Option<WorkspaceId>,
    pointer: Option<[f32; 2]>,
}
impl Default for Overview {
    fn default() -> Self {
        Self::new()
    }
}
impl Overview {
    pub fn new() -> Self {
        Self {
            snapshot: None,
            panes: Vec::new(),
            workspace: None,
            scene: Scene::new(Element::new(0, Widget::Panel)).expect("valid empty scene"),
            targets: HashMap::new(),
            window_ids: HashMap::new(),
            next_id: 100_000,
            query: TextEdit::new("").expect("empty text"),
            apps: Apps::Unloaded,
            icons: icons::Icons::default(),
            loader: None,
            lookup: None,
            catalog_env: None,
            waker: None,
            launch_when_ready: false,
            page: 0,
            result_page: 0,
            connected: false,
            size: [0.0; 2],
            dirty: true,
            pending: false,
            request: None,
            error: None,
            close: false,
            surface_focused: true,
            background_pressed: false,
            pressed_pane: None,
            pointer: None,
        }
    }
    /// Use an already-loaded catalog instead of scanning the XDG directories.
    pub fn with_catalog(mut self, catalog: Catalog) -> Self {
        self.apps = Apps::Ready(catalog);
        self
    }
    pub fn with_icon_lookup(mut self, lookup: IconLookup) -> Self {
        self.lookup = Some(lookup);
        self
    }
    /// Scan these directories instead of the process's XDG environment.
    pub fn with_catalog_environment(mut self, env: Environment) -> Self {
        self.catalog_env = Some(env);
        self
    }
    /// True while the worker still owes the UI a catalog or icons.
    pub fn is_loading(&self) -> bool {
        matches!(self.apps, Apps::Loading) || self.icons.is_loading()
    }
    pub fn browsed_workspace(&self) -> Option<WorkspaceId> {
        self.workspace
    }
    pub fn snapshot(&self) -> Option<&DesktopSnapshot> {
        self.snapshot.as_ref()
    }
    pub fn scene(&self) -> &Scene {
        &self.scene
    }
    fn window(&self, id: WindowId) -> Option<&WindowSummary> {
        self.snapshot.as_ref()?.windows.iter().find(|w| w.id == id)
    }
    fn minimized(&self, ws: WorkspaceId) -> Vec<&WindowSummary> {
        self.snapshot.as_ref().map_or_else(Vec::new, |s| {
            s.windows
                .iter()
                .filter(|w| w.workspace == ws && w.minimized)
                .collect()
        })
    }
    fn browse(&mut self, ws: WorkspaceId) {
        if self.workspace != Some(ws)
            && self
                .snapshot
                .as_ref()
                .is_some_and(|s| s.workspaces.iter().any(|w| w.workspace == ws))
        {
            self.workspace = Some(ws);
            let _ = self.scene.set_focus(None);
            self.page = 0;
            self.dirty = true;
        }
    }
    fn adjacent(&mut self, next: bool) {
        let Some(s) = &self.snapshot else { return };
        let Some(index) = s
            .workspaces
            .iter()
            .position(|w| Some(w.workspace) == self.workspace)
        else {
            return;
        };
        let target = if next {
            index.saturating_add(1)
        } else {
            index.saturating_sub(1)
        };
        if let Some(w) = s.workspaces.get(target) {
            self.browse(w.workspace);
        }
    }
    fn dispatch(&mut self, command: DesktopCommand) {
        if !self.connected {
            self.error = Some("Desktop connection unavailable".into());
            self.dirty = true;
        } else if !self.pending {
            self.request = Some(HostRequest::Desktop(command));
            self.pending = true;
            self.error = None;
            self.dirty = true;
        }
    }
    fn activate(&mut self, id: ElementId) {
        if self.pending {
            return;
        }
        match self.targets.get(&id).copied() {
            Some(Target::Workspace(ws)) => self.browse(ws),
            Some(Target::EnterWorkspace(ws)) => {
                self.dispatch(DesktopCommand::FocusWorkspace { workspace: ws })
            }
            Some(Target::PreviewWorkspace(ws)) => {
                self.dispatch(DesktopCommand::FocusWorkspace { workspace: ws })
            }
            Some(Target::Window(id)) => {
                if let Some(window) = self.window(id) {
                    self.dispatch(if window.minimized {
                        DesktopCommand::RestoreAndFocusWindow { window: id }
                    } else {
                        DesktopCommand::FocusWindow { window: id }
                    });
                }
            }
            Some(Target::App(index)) => self.launch(index),
            Some(Target::ClearSearch) => self.clear_query(),
            Some(Target::Page(next)) => {
                let page = if self.query.text().is_empty() {
                    &mut self.page
                } else {
                    &mut self.result_page
                };
                *page = if next {
                    page.saturating_add(1)
                } else {
                    page.saturating_sub(1)
                };
                self.dirty = true;
            }
            None => {}
        }
    }
    fn launch(&mut self, index: usize) {
        if let Apps::Ready(catalog) = &self.apps
            && let Some(app) = catalog.get(index)
        {
            let argv = app.argv.clone();
            self.dispatch(DesktopCommand::Spawn { argv });
        }
    }
    fn clear_query(&mut self) {
        let _ = self.scene.set_focus(None);
        self.query = TextEdit::new("").expect("empty text");
        self.result_page = 0;
        self.dirty = true;
    }
    fn pane_at(&self, p: [f32; 2]) -> Option<WorkspaceId> {
        self.panes.iter().find_map(|pane| {
            (p[0] >= pane.x as f32
                && p[1] >= pane.y as f32
                && p[0] < pane.x as f32 + pane.width as f32
                && p[1] < pane.y as f32 + pane.height as f32)
                .then_some(pane.workspace)
        })
    }
    fn search_page_size(&self) -> usize {
        (((self.size[1]
            - 28.0f32.min(self.size[1] * 0.04)
            - 54.0f32.min(self.size[1] * 0.15)
            - 80.0)
            / 60.0)
            .floor() as usize)
            .clamp(1, 8)
    }
    /// Validate the composition without a Wayland connection or GPU.
    pub fn check(&mut self, size: [f32; 2]) -> Result<(), UiError> {
        self.size = size;
        Scene::new(self.compose()).map(|_| ())
    }
    fn loader(&mut self) -> &Loader {
        if self.loader.is_none() {
            let lookup = self.lookup.take().unwrap_or_else(IconLookup::from_process);
            self.loader = Some(Loader::start(lookup, self.waker.clone()));
        }
        self.loader.as_ref().expect("started above")
    }
    fn ensure_catalog(&mut self) {
        if matches!(self.apps, Apps::Unloaded) {
            let env = self
                .catalog_env
                .clone()
                .unwrap_or_else(Environment::from_process);
            self.apps = if self.loader().submit(Job::Catalog(env)) {
                Apps::Loading
            } else {
                Apps::Unavailable
            };
        }
    }
    /// Move finished worker results into UI state.
    fn absorb(&mut self) {
        let done = match &self.loader {
            Some(loader) => loader.take(),
            None => return,
        };
        for result in done {
            match result {
                Done::Catalog(Ok(catalog)) => {
                    if catalog.truncated() {
                        eprintln!("knave-shell: application catalog is incomplete (scan limit)");
                    }
                    self.apps = Apps::Ready(catalog);
                    self.result_page = 0;
                    if std::mem::take(&mut self.launch_when_ready)
                        && let Some(&index) = self.search().first()
                    {
                        self.launch(index);
                    }
                }
                Done::Catalog(Err(error)) => {
                    eprintln!("knave-shell: {error}");
                    self.apps = Apps::Unavailable;
                    self.launch_when_ready = false;
                }
                Done::Icon(name, image) => self.icons.finish(name, image),
            }
            self.dirty = true;
        }
    }
    /// Catalog indexes for the current query, best match first.
    fn search(&self) -> Vec<usize> {
        match &self.apps {
            Apps::Ready(catalog) => catalog.search(self.query.text(), MAX_RESULTS),
            Apps::Unloaded | Apps::Loading | Apps::Unavailable => Vec::new(),
        }
    }
    fn sync_query(&mut self) {
        if let Some(Widget::TextInput(edit)) = self.scene.widget(SEARCH) {
            let changed = self.query.text() != edit.text();
            self.query = edit.clone();
            if changed {
                self.launch_when_ready = false;
                if !self.query.text().is_empty() {
                    self.ensure_catalog();
                }
                self.result_page = 0;
                self.dirty = true;
            }
        }
    }
    fn rebuild(&mut self, text: &mut dyn TextMeasurer) {
        let focus = self.scene.focus();
        let root = self.compose();
        match Scene::new(root) {
            Ok(mut scene) => {
                scene.layout(self.size, text);
                if !self.surface_focused {
                    scene.event(Input::FocusLost);
                }
                if let Some(id) = focus {
                    let _ = scene.set_focus(Some(id));
                }
                if scene.focus().is_none() {
                    let fallback = if !self.query.text().is_empty() {
                        Some(SEARCH)
                    } else {
                        self.targets.iter().find_map(|(id, t)| {
                            matches!(t, Target::EnterWorkspace(_)).then_some(*id)
                        })
                    };
                    let _ = scene.set_focus(fallback);
                }
                if let Some(p) = self.pointer {
                    scene.event(Input::PointerMove(p));
                }
                self.scene = scene;
            }
            Err(error) => {
                self.error = Some(error.to_string());
            }
        }
        self.dirty = false;
    }
}
impl Application for Overview {
    fn uses_desktop(&self) -> bool {
        true
    }
    fn desktop_snapshot(&mut self, snapshot: &DesktopSnapshot) {
        if self.snapshot.as_ref() == Some(snapshot) && self.connected {
            return;
        }
        if snapshot.windows.len() > MAX_WINDOWS || snapshot.workspaces.len() > MAX_WORKSPACES {
            self.connected = false;
            // Cancel an activation not yet handed to the host against the rejected model.
            if matches!(self.request, Some(HostRequest::Desktop(_))) {
                self.request = None;
                self.pending = false;
            }
            self.host_error("Desktop exceeds the supported overview capacity");
            return;
        }
        self.connected = true;
        self.error = None;
        let previous = self.workspace;
        self.workspace = previous
            .filter(|id| snapshot.workspaces.iter().any(|w| w.workspace == *id))
            .or_else(|| {
                snapshot
                    .workspaces
                    .iter()
                    .find(|w| w.active)
                    .or(snapshot.workspaces.first())
                    .map(|w| w.workspace)
            });
        if self.workspace != previous {
            self.page = 0;
        }
        self.window_ids
            .retain(|id, _| snapshot.windows.iter().any(|w| w.id == *id));
        for w in &snapshot.windows {
            if !self.window_ids.contains_key(&w.id) {
                self.window_ids.insert(w.id, ElementId(self.next_id));
                self.next_id += 1;
            }
        }
        self.snapshot = Some(snapshot.clone());
        self.dirty = true;
    }
    fn desktop_unavailable(&mut self) {
        self.connected = false;
        self.host_error("Desktop connection unavailable — retrying");
    }
    fn overview_panes(&self) -> Option<Vec<OverviewPane>> {
        Some(self.panes.clone())
    }
    fn desktop_action_finished(&mut self, result: Result<(), String>) {
        if !self.pending {
            return;
        }
        self.pending = false;
        match result {
            Ok(()) => self.close = true,
            Err(error) => self.error = Some(error),
        }
        self.dirty = true;
    }
    fn host_error(&mut self, message: &str) {
        self.error = Some(message.chars().take(256).collect());
        self.dirty = true;
    }
    fn cursor(&self) -> CursorShape {
        self.scene.cursor()
    }
    fn text_input(&self) -> Option<TextInputState> {
        self.scene.text_input_state()
    }
    fn take_request(&mut self) -> Option<HostRequest> {
        self.request.take()
    }
    fn should_close(&self) -> bool {
        self.close
    }
    fn needs_frame(&self) -> bool {
        self.dirty
            || self.scene.needs_frame()
            || self.loader.as_ref().is_some_and(Loader::has_results)
    }
    fn set_waker(&mut self, waker: knave_wayland::Waker) {
        self.waker = Some(waker);
    }
    fn input(&mut self, event: Input) {
        match &event {
            Input::FocusGained => self.surface_focused = true,
            Input::FocusLost => {
                self.surface_focused = false;
                self.background_pressed = false;
                self.pressed_pane = None;
            }
            Input::PointerMove(p) => self.pointer = Some(*p),
            Input::PointerLeave => {
                self.pointer = None;
                self.background_pressed = false;
                self.pressed_pane = None;
            }
            Input::PointerDown(p) => {
                self.pressed_pane = self.pane_at(*p);
                self.background_pressed =
                    self.pressed_pane.is_none() && self.scene.hit_test(*p).is_none();
            }
            Input::PointerUp(p) => {
                if self.background_pressed && self.scene.hit_test(*p).is_none() {
                    self.close = true;
                }
                self.background_pressed = false;
            }
            Input::Key {
                key,
                pressed: true,
                repeat,
                modifiers,
            } => {
                if *key == Key::Escape && !repeat {
                    self.close = true;
                    return;
                }
                if *key == Key::K && modifiers.control {
                    let _ = self.scene.set_focus(Some(SEARCH));
                    return;
                }
                if matches!(key, Key::Left | Key::Right)
                    && (modifiers.alt
                        || (self.query.text().is_empty() && self.scene.focus() != Some(SEARCH)))
                {
                    self.adjacent(*key == Key::Right);
                    return;
                }
                if *key == Key::Enter && !repeat && self.scene.focus() == Some(SEARCH) {
                    if let Some(&index) = self
                        .search()
                        .get(self.result_page * self.search_page_size())
                    {
                        self.launch(index);
                    } else if matches!(self.apps, Apps::Loading) {
                        self.launch_when_ready = true;
                    }
                    return;
                }
                if *key == Key::Down
                    && self.scene.focus() == Some(SEARCH)
                    && !self.query.text().is_empty()
                {
                    if let Some(&index) = self
                        .search()
                        .get(self.result_page * self.search_page_size())
                    {
                        let _ = self.scene.set_focus(Some(app_element(index)));
                    }
                    return;
                }
            }
            Input::Text(value)
                if value.chars().any(|c| !c.is_control()) && self.surface_focused =>
            {
                let _ = self.scene.set_focus(Some(SEARCH));
            }
            _ => {}
        }
        let pane_click = if let Input::PointerUp(p) = &event {
            self.pressed_pane
                .take()
                .filter(|workspace| self.pane_at(*p) == Some(*workspace))
                .map(|workspace| (workspace, p[0].floor() as i32, p[1].floor() as i32))
        } else {
            None
        };
        let result = self.scene.event(event);
        self.sync_query();
        if let Some((workspace, x, y)) = pane_click {
            // A scene rebuild may drop button capture between press and release.
            // Pane selection belongs to the pointer gesture, not the card widget.
            self.dispatch(DesktopCommand::FocusOverviewPoint { workspace, x, y });
            return;
        }
        match result.action {
            Some(Action::Activated(id)) => self.activate(id),
            Some(Action::Copy(value) | Action::Cut { copied: value, .. }) => {
                if !self.pending {
                    self.request = Some(HostRequest::Copy(value));
                }
            }
            Some(Action::RequestPaste(id)) if !self.pending => {
                self.request = Some(HostRequest::Paste(id))
            }
            _ => {}
        }
    }
    fn frame(&mut self, size: [f32; 2], text: &mut dyn TextMeasurer) -> &DisplayList {
        self.absorb();
        if size != self.size {
            self.size = size;
            self.dirty = true;
        }
        if self.dirty {
            self.rebuild(text);
        }
        self.scene.display_list(size, text)
    }
}

#[cfg(test)]
mod tests;
