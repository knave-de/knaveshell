//! Workspace-first overview with a separate minimized-window shelf.
mod view;
use knave_desktop_api::{DesktopCommand, DesktopSnapshot, WindowId, WindowSummary, WorkspaceId};
use knave_ui::{DisplayList, WorkspacePreviewImage, toolkit::*};
use knave_wayland::{Application, HostRequest};
use std::collections::HashMap;

const SEARCH: ElementId = ElementId(2);
const MAX_WINDOWS: usize = 1024;
const MAX_WORKSPACES: usize = 128;
const PAGE_SIZE: usize = 5;

#[derive(Clone, Copy)]
enum Target {
    Workspace(WorkspaceId),
    EnterWorkspace(WorkspaceId),
    Window(WindowId),
    Page(bool),
}

pub struct Overview {
    snapshot: Option<DesktopSnapshot>,
    previews: Vec<WorkspacePreviewImage>,
    workspace: Option<WorkspaceId>,
    scene: Scene,
    targets: HashMap<ElementId, Target>,
    window_ids: HashMap<WindowId, ElementId>,
    next_id: u64,
    query: TextEdit,
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
            previews: Vec::new(),
            workspace: None,
            scene: Scene::new(Element::new(0, Widget::Panel)).expect("valid empty scene"),
            targets: HashMap::new(),
            window_ids: HashMap::new(),
            next_id: 100_000,
            query: TextEdit::new("").expect("empty text"),
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
            pointer: None,
        }
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
        if !self.pending && self.connected {
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
            Some(Target::Window(id)) => {
                if let Some(window) = self.window(id) {
                    self.dispatch(if window.minimized {
                        DesktopCommand::RestoreWindow { window: id }
                    } else {
                        DesktopCommand::FocusWindow { window: id }
                    });
                }
            }
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
    fn clear_query(&mut self) {
        let _ = self.scene.set_focus(None);
        self.query = TextEdit::new("").expect("empty text");
        self.result_page = 0;
        self.dirty = true;
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
    fn search(&self) -> Vec<&WindowSummary> {
        let needle = self.query.text().trim().to_lowercase();
        self.snapshot.as_ref().map_or_else(Vec::new, |s| {
            s.windows
                .iter()
                .filter(|w| {
                    w.title.to_lowercase().contains(&needle)
                        || w.app_id.to_lowercase().contains(&needle)
                        || format!("workspace {}", w.workspace.0).contains(&needle)
                })
                .collect()
        })
    }
    fn sync_query(&mut self) {
        if let Some(Widget::TextInput(edit)) = self.scene.widget(SEARCH) {
            let changed = self.query.text() != edit.text();
            self.query = edit.clone();
            if changed {
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
        self.connected = true;
        self.error = None;
        if snapshot.windows.len() > MAX_WINDOWS || snapshot.workspaces.len() > MAX_WORKSPACES {
            self.host_error("Desktop exceeds the supported overview capacity");
            return;
        }
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
    fn preview_workspaces(&self) -> Option<Vec<WorkspaceId>> {
        let Some(s) = &self.snapshot else {
            return Some(Vec::new());
        };
        let Some(index) = s
            .workspaces
            .iter()
            .position(|w| Some(w.workspace) == self.workspace)
        else {
            return Some(Vec::new());
        };
        let mut spaces = vec![s.workspaces[index].workspace];
        for other in [index.checked_sub(1), Some(index + 1)]
            .into_iter()
            .flatten()
        {
            if let Some(w) = s.workspaces.get(other) {
                spaces.push(w.workspace);
            }
        }
        Some(spaces)
    }
    fn workspace_previews(&mut self, previews: &[WorkspacePreviewImage]) {
        self.previews = previews.to_vec();
        self.dirty = true;
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
        self.dirty || self.scene.needs_frame()
    }
    fn input(&mut self, event: Input) {
        match &event {
            Input::FocusGained => self.surface_focused = true,
            Input::FocusLost => {
                self.surface_focused = false;
                self.background_pressed = false;
            }
            Input::PointerMove(p) => self.pointer = Some(*p),
            Input::PointerLeave => {
                self.pointer = None;
                self.background_pressed = false;
            }
            Input::PointerDown(p) => self.background_pressed = self.scene.hit_test(*p).is_none(),
            Input::PointerUp(p) => {
                if self.background_pressed && self.scene.hit_test(*p).is_none() {
                    if self.query.text().is_empty() {
                        self.close = true
                    } else {
                        self.clear_query();
                    }
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
                    if self.query.text().is_empty() {
                        self.close = true
                    } else {
                        self.clear_query();
                    }
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
                    if let Some(w) = self
                        .search()
                        .get(self.result_page * self.search_page_size())
                    {
                        let id = self.window_ids[&w.id];
                        self.activate(id);
                    }
                    return;
                }
                if *key == Key::Down
                    && self.scene.focus() == Some(SEARCH)
                    && !self.query.text().is_empty()
                {
                    if let Some(w) = self
                        .search()
                        .get(self.result_page * self.search_page_size())
                    {
                        let _ = self.scene.set_focus(Some(self.window_ids[&w.id]));
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
        let result = self.scene.event(event);
        self.sync_query();
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
