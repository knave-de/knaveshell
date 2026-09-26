//! Knave-owned Wayland layer-shell and wgpu runtime.

use std::{
    io::Cursor,
    num::NonZeroU32,
    ptr::NonNull,
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use base64::Engine;
use knave_desktop_api::{
    DesktopClient, DesktopCommand, DesktopQuery, DesktopRequest, DesktopResponse, DesktopSnapshot,
    WorkspaceId, WorkspacePreview,
};
use knave_renderer::{RenderList, WgpuPainter, WgpuRenderer};
use knave_ui::{MAX_SEARCH_QUERY, UiAction, UiImage, UiScene, WorkspacePreviewImage};
use smithay_client_toolkit::reexports::{
    calloop::{EventLoop, channel},
    calloop_wayland_source::WaylandSource,
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Modifiers, RawModifiers},
        pointer::{BTN_LEFT, PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::{
        WaylandSurface,
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
    },
};
use wayland_client::{
    Connection, Proxy, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
};
use wgpu::rwh::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellRole {
    Bar,
    Overview,
}

impl ShellRole {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "bar" => Some(Self::Bar),
            "overview" => Some(Self::Overview),
            _ => None,
        }
    }

    fn layer(self) -> Layer {
        match self {
            Self::Bar => Layer::Top,
            Self::Overview => Layer::Overlay,
        }
    }

    fn namespace(self) -> &'static str {
        match self {
            Self::Bar => "knave-shell-bar",
            Self::Overview => "knave-shell-overview",
        }
    }

    fn keyboard_interactivity(self) -> KeyboardInteractivity {
        match self {
            Self::Bar => KeyboardInteractivity::None,
            Self::Overview => KeyboardInteractivity::Exclusive,
        }
    }

    fn requested_size(self) -> (u32, u32) {
        match self {
            Self::Bar => (0, 36),
            Self::Overview => (0, 0),
        }
    }

    fn anchors(self) -> Anchor {
        match self {
            Self::Bar => Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
            Self::Overview => Anchor::TOP | Anchor::RIGHT | Anchor::BOTTOM | Anchor::LEFT,
        }
    }

    fn exclusive_zone(self) -> i32 {
        match self {
            Self::Bar => 36,
            Self::Overview => -1,
        }
    }

    fn scene(self, revision: u64, width: f32, height: f32, input: SceneInput<'_>) -> UiScene {
        match self {
            Self::Bar => UiScene::bar_with_snapshot(revision, width, height, input.snapshot),
            Self::Overview => UiScene::overview_with_snapshot_and_search(
                revision,
                width,
                height,
                input.snapshot,
                input.query,
                input.selected,
                input.previews,
            ),
        }
    }
}

#[derive(Debug)]
struct SceneInput<'a> {
    snapshot: Option<&'a DesktopSnapshot>,
    query: &'a str,
    selected: usize,
    previews: &'a [WorkspacePreviewImage],
}

#[derive(Debug, thiserror::Error)]
pub enum WaylandError {
    #[error("could not connect to the Knave Wayland compositor: {0}")]
    Connect(String),
    #[error("could not enumerate Wayland globals: {0}")]
    Globals(String),
    #[error("wl_compositor is unavailable: {0}")]
    Compositor(String),
    #[error("wlr-layer-shell is unavailable: {0}")]
    LayerShell(String),
    #[error("no compatible GPU adapter was found: {0}")]
    Adapter(String),
    #[error("could not create the GPU device: {0}")]
    Device(String),
    #[error("the Wayland surface has no supported configuration")]
    SurfaceConfiguration,
    #[error("Wayland dispatch failed: {0}")]
    Dispatch(String),
}

const SNAPSHOT_REFRESH: Duration = Duration::from_millis(500);
const SNAPSHOT_MAX_BACKOFF: Duration = Duration::from_secs(5);
const MAX_PREVIEWS: usize = 10;
const PREVIEW_WIDTH: u32 = 320;
const PREVIEW_HEIGHT: u32 = 180;
const MAX_PREVIEW_PIXELS: u64 = (PREVIEW_WIDTH as u64) * (PREVIEW_HEIGHT as u64);
const MAX_PREVIEW_BASE64_LENGTH: usize = 512 * 1024;

enum RuntimeWake {
    Redraw,
}

type WakeSender = channel::SyncSender<RuntimeWake>;

enum SnapshotCommand {
    Refresh,
    Stop,
}

struct SnapshotWorker {
    commands: SyncSender<SnapshotCommand>,
    updates: Receiver<DesktopSnapshot>,
    thread: Option<JoinHandle<()>>,
}

impl SnapshotWorker {
    fn start(wake: WakeSender) -> Self {
        let (commands, command_rx) = mpsc::sync_channel(1);
        let (updates, update_rx) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            let mut client = None;
            let mut backoff = SNAPSHOT_REFRESH;
            let mut last_generation = None;
            loop {
                match query_snapshot(&mut client) {
                    Ok(snapshot) => {
                        backoff = SNAPSHOT_REFRESH;
                        if last_generation != Some(snapshot.generation) {
                            last_generation = Some(snapshot.generation);
                            let _ = updates.try_send(snapshot);
                            let _ = wake.try_send(RuntimeWake::Redraw);
                        }
                    }
                    Err(()) => {
                        client = None;
                        backoff = backoff.saturating_mul(2).min(SNAPSHOT_MAX_BACKOFF);
                    }
                }
                match command_rx.recv_timeout(backoff) {
                    Ok(SnapshotCommand::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Ok(SnapshotCommand::Refresh) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
        });
        Self {
            commands,
            updates: update_rx,
            thread: Some(thread),
        }
    }

    fn request_refresh(&self) {
        let _ = self.commands.try_send(SnapshotCommand::Refresh);
    }

    fn latest(&self) -> Option<DesktopSnapshot> {
        self.updates.try_iter().last()
    }
}

impl Drop for SnapshotWorker {
    fn drop(&mut self) {
        let _ = self.commands.try_send(SnapshotCommand::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Debug)]
struct PreviewUpdate {
    generation: u64,
    previews: Vec<WorkspacePreviewImage>,
}

enum PreviewCommand {
    Refresh {
        generation: u64,
        workspaces: Vec<WorkspaceId>,
    },
    Stop,
}

struct PreviewWorker {
    commands: SyncSender<PreviewCommand>,
    updates: Arc<Mutex<Option<PreviewUpdate>>>,
    thread: Option<JoinHandle<()>>,
}

impl PreviewWorker {
    fn start(wake: WakeSender) -> Self {
        let (commands, command_rx) = mpsc::sync_channel(1);
        let updates = Arc::new(Mutex::new(None));
        let update_slot = Arc::clone(&updates);
        let thread = thread::spawn(move || {
            let mut client = None;
            while let Ok(command) = command_rx.recv() {
                match command {
                    PreviewCommand::Stop => break,
                    PreviewCommand::Refresh {
                        generation,
                        mut workspaces,
                    } => {
                        workspaces.truncate(MAX_PREVIEWS);
                        let previews = query_previews(&mut client, &workspaces);
                        if let Ok(mut slot) = update_slot.lock() {
                            *slot = Some(PreviewUpdate {
                                generation,
                                previews,
                            });
                            let _ = wake.try_send(RuntimeWake::Redraw);
                        }
                    }
                }
            }
        });
        Self {
            commands,
            updates,
            thread: Some(thread),
        }
    }

    fn request_refresh(&self, generation: u64, workspaces: Vec<WorkspaceId>) {
        let _ = self.commands.try_send(PreviewCommand::Refresh {
            generation,
            workspaces,
        });
    }

    fn latest(&self) -> Option<PreviewUpdate> {
        self.updates.lock().ok()?.take()
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        let _ = self.commands.try_send(PreviewCommand::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn query_previews(
    client: &mut Option<DesktopClient>,
    workspaces: &[WorkspaceId],
) -> Vec<WorkspacePreviewImage> {
    if workspaces.is_empty() {
        return Vec::new();
    }
    if client.is_none() {
        *client = DesktopClient::connect().ok();
    }
    let Some(_) = client.as_mut() else {
        return Vec::new();
    };

    let mut previews = Vec::with_capacity(workspaces.len().min(MAX_PREVIEWS));
    let mut failed = false;
    for workspace in workspaces.iter().take(MAX_PREVIEWS).copied() {
        let response = client
            .as_mut()
            .expect("preview client was initialized")
            .request(&DesktopRequest::Query(DesktopQuery::WorkspacePreview {
                workspace,
                width: PREVIEW_WIDTH,
                height: PREVIEW_HEIGHT,
            }));
        match response {
            Ok(DesktopResponse::WorkspacePreview(preview)) => {
                if let Some(preview) = decode_preview(preview) {
                    previews.push(preview);
                }
            }
            Ok(_) => {}
            Err(_) => {
                failed = true;
                break;
            }
        }
    }
    if failed {
        *client = None;
    }
    previews
}

fn decode_preview(preview: WorkspacePreview) -> Option<WorkspacePreviewImage> {
    if preview.width != PREVIEW_WIDTH || preview.height != PREVIEW_HEIGHT {
        return None;
    }
    if preview.png_base64.len() > MAX_PREVIEW_BASE64_LENGTH {
        return None;
    }
    let encoded = base64::engine::general_purpose::STANDARD
        .decode(preview.png_base64)
        .ok()?;
    let mut decoder = png::Decoder::new(Cursor::new(encoded));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let pixel_count =
        u64::from(reader.info().width).checked_mul(u64::from(reader.info().height))?;
    if pixel_count == 0 || pixel_count > MAX_PREVIEW_PIXELS {
        return None;
    }
    let max_bytes = usize::try_from(pixel_count.checked_mul(4)?).ok()?;
    let output_size = reader.output_buffer_size()?;
    if output_size > max_bytes {
        return None;
    }
    let mut buffer = vec![0; output_size];
    let info = reader.next_frame(&mut buffer).ok()?;
    if info.width != preview.width || info.height != preview.height {
        return None;
    }
    let data = &buffer[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .chunks(3)
            .filter(|pixel| pixel.len() == 3)
            .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
            .collect(),
        png::ColorType::Grayscale => data
            .iter()
            .flat_map(|value| [*value, *value, *value, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .chunks(2)
            .filter(|pixel| pixel.len() == 2)
            .flat_map(|pixel| [pixel[0], pixel[0], pixel[0], pixel[1]])
            .collect(),
        png::ColorType::Indexed => return None,
    };
    Some(WorkspacePreviewImage {
        workspace: preview.workspace,
        image: UiImage::from_rgba(info.width, info.height, rgba)?,
    })
}

enum ActionCommand {
    Dispatch(DesktopCommand),
    Stop,
}

struct ActionWorker {
    commands: SyncSender<ActionCommand>,
    thread: Option<JoinHandle<()>>,
}

impl ActionWorker {
    fn start() -> Self {
        let (commands, command_rx) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            let mut client = None;
            while let Ok(command) = command_rx.recv() {
                match command {
                    ActionCommand::Stop => break,
                    ActionCommand::Dispatch(command) => {
                        if let Err(error) = dispatch_action(&mut client, command) {
                            eprintln!("knave-shell: shell action failed: {error}");
                            client = None;
                        }
                    }
                }
            }
        });
        Self {
            commands,
            thread: Some(thread),
        }
    }

    fn dispatch(&self, command: DesktopCommand) {
        if self
            .commands
            .try_send(ActionCommand::Dispatch(command))
            .is_err()
        {
            eprintln!("knave-shell: shell action queue is full; dropping action");
        }
    }
}

impl Drop for ActionWorker {
    fn drop(&mut self) {
        let _ = self.commands.try_send(ActionCommand::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn dispatch_action(
    client: &mut Option<DesktopClient>,
    command: DesktopCommand,
) -> Result<(), knave_desktop_api::ClientError> {
    if client.is_none() {
        *client = Some(DesktopClient::connect()?);
    }
    let client = client.as_mut().expect("desktop client was initialized");
    client.request(&DesktopRequest::Dispatch(command))?;
    Ok(())
}

fn query_snapshot(client: &mut Option<DesktopClient>) -> Result<DesktopSnapshot, ()> {
    if client.is_none() {
        *client = Some(DesktopClient::connect().map_err(|_| ())?);
    }
    let response = client
        .as_mut()
        .ok_or(())?
        .request(&DesktopRequest::Query(DesktopQuery::Snapshot))
        .map_err(|_| ())?;
    match response {
        DesktopResponse::Snapshot(snapshot) => Ok(snapshot),
        DesktopResponse::Ok
        | DesktopResponse::Windows(_)
        | DesktopResponse::Workspaces(_)
        | DesktopResponse::ActiveWindow(_)
        | DesktopResponse::ActiveWorkspace(_)
        | DesktopResponse::WorkspacePreview(_)
        | DesktopResponse::Version { .. }
        | DesktopResponse::Error(_) => Err(()),
    }
}

pub fn run(role: ShellRole) -> Result<(), WaylandError> {
    let connection = Connection::connect_to_env().map_err(|error| {
        WaylandError::Connect(format!(
            "{error}; start the Knave compositor and use its WAYLAND_DISPLAY"
        ))
    })?;
    let (globals, event_queue) = registry_queue_init(&connection)
        .map_err(|error| WaylandError::Globals(error.to_string()))?;
    let queue_handle = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &queue_handle)
        .map_err(|error| WaylandError::Compositor(error.to_string()))?;
    let layer_shell = LayerShell::bind(&globals, &queue_handle)
        .map_err(|error| WaylandError::LayerShell(error.to_string()))?;

    let surface = compositor.create_surface(&queue_handle);
    let layer = layer_shell.create_layer_surface(
        &queue_handle,
        surface,
        role.layer(),
        Some(role.namespace()),
        None,
    );
    layer.set_anchor(role.anchors());
    layer.set_keyboard_interactivity(role.keyboard_interactivity());
    layer.set_exclusive_zone(role.exclusive_zone());
    let (width, height) = role.requested_size();
    layer.set_size(width, height);
    layer.commit();

    let renderer = WgpuRenderer::new();
    let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
        NonNull::new(connection.backend().display_ptr() as *mut _)
            .expect("Wayland connection returned a null display pointer"),
    ));
    let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
        NonNull::new(layer.wl_surface().id().as_ptr() as *mut _)
            .expect("Wayland surface returned a null object pointer"),
    ));
    let surface = unsafe {
        renderer
            .instance()
            .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(raw_display_handle),
                raw_window_handle,
            })
            .map_err(|error| WaylandError::Connect(error.to_string()))?
    };

    let adapter = pollster::block_on(renderer.instance().request_adapter(
        &wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        },
    ))
    .map_err(|error| WaylandError::Adapter(error.to_string()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))
        .map_err(|error| WaylandError::Device(error.to_string()))?;

    let mut event_loop: EventLoop<Runtime> =
        EventLoop::try_new().map_err(|error| WaylandError::Dispatch(error.to_string()))?;
    WaylandSource::new(connection.clone(), event_queue)
        .insert(event_loop.handle())
        .map_err(|error| WaylandError::Dispatch(error.to_string()))?;
    let (wake_sender, wake_channel) = channel::sync_channel(1);
    let wake_queue_handle = queue_handle.clone();
    event_loop
        .handle()
        .insert_source(wake_channel, move |event, _, state| match event {
            channel::Event::Msg(RuntimeWake::Redraw) => state.request_draw(&wake_queue_handle),
            channel::Event::Closed => state.exit = true,
        })
        .map_err(|error| WaylandError::Dispatch(error.to_string()))?;

    let mut state = Runtime {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &queue_handle),
        seat_state: SeatState::new(&globals, &queue_handle),
        keyboard: None,
        pointer: None,
        role,
        layer,
        renderer,
        surface,
        adapter,
        device,
        queue,
        width: 1,
        height: 1,
        revision: 0,
        frame_pending: false,
        snapshot_worker: SnapshotWorker::start(wake_sender.clone()),
        action_worker: ActionWorker::start(),
        preview_worker: (role == ShellRole::Overview).then(|| PreviewWorker::start(wake_sender)),
        snapshot: None,
        previews: Vec::new(),
        preview_generation: 0,
        search_query: String::new(),
        search_index: 0,
        scene: UiScene::new(0),
        render_list: RenderList::default(),
        scene_dirty: true,
        painter: None,
        configured: false,
        exit: false,
    };

    while !state.exit {
        event_loop
            .dispatch(None, &mut state)
            .map_err(|error| WaylandError::Dispatch(error.to_string()))?;
    }

    Ok(())
}

struct Runtime {
    registry_state: RegistryState,
    output_state: OutputState,
    seat_state: SeatState,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    role: ShellRole,
    layer: LayerSurface,
    renderer: WgpuRenderer,
    surface: wgpu::Surface<'static>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    painter: Option<WgpuPainter>,
    snapshot_worker: SnapshotWorker,
    action_worker: ActionWorker,
    preview_worker: Option<PreviewWorker>,
    snapshot: Option<DesktopSnapshot>,
    previews: Vec<WorkspacePreviewImage>,
    preview_generation: u64,
    search_query: String,
    search_index: usize,
    scene: UiScene,
    render_list: RenderList,
    scene_dirty: bool,
    width: u32,
    height: u32,
    revision: u64,
    frame_pending: bool,
    configured: bool,
    exit: bool,
}

impl Runtime {
    fn configure_surface(&self, width: u32, height: u32) -> Option<wgpu::SurfaceConfiguration> {
        self.surface
            .get_default_config(&self.adapter, width.max(1), height.max(1))
    }

    fn request_draw(&mut self, qh: &QueueHandle<Self>) {
        if self.configured && !self.frame_pending {
            self.draw(qh);
        }
    }

    fn draw(&mut self, qh: &QueueHandle<Self>) {
        if !self.configured {
            return;
        }
        let mut should_render = self.scene_dirty;
        if let Some(snapshot) = self.snapshot_worker.latest()
            && self.snapshot.as_ref() != Some(&snapshot)
        {
            let generation = self.preview_generation.wrapping_add(1);
            let workspaces = snapshot
                .workspaces
                .iter()
                .take(MAX_PREVIEWS)
                .map(|workspace| workspace.workspace)
                .collect();
            self.snapshot = Some(snapshot);
            self.previews.clear();
            self.preview_generation = generation;
            if let Some(worker) = &self.preview_worker {
                worker.request_refresh(generation, workspaces);
            }
            self.scene_dirty = true;
            should_render = true;
        }
        if let Some(worker) = &self.preview_worker
            && let Some(update) = worker.latest()
            && self.preview_generation == update.generation
        {
            self.previews = update.previews;
            self.scene_dirty = true;
            should_render = true;
        }
        if self.scene_dirty {
            self.scene = self.role.scene(
                self.revision,
                self.width as f32,
                self.height as f32,
                SceneInput {
                    snapshot: self.snapshot.as_ref(),
                    query: &self.search_query,
                    selected: self.search_index,
                    previews: &self.previews,
                },
            );
            self.render_list = self.renderer.prepare(&self.scene);
            self.scene_dirty = false;
        }
        if !should_render {
            return;
        }
        let render_list = &self.render_list;
        let clear_color = wgpu::Color {
            r: f64::from(render_list.clear_color.red) / 255.0,
            g: f64::from(render_list.clear_color.green) / 255.0,
            b: f64::from(render_list.clear_color.blue) / 255.0,
            a: f64::from(render_list.clear_color.alpha) / 255.0,
        };

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame)
            | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => frame,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                if let Some(config) = self.configure_surface(self.width, self.height) {
                    self.surface.configure(&self.device, &config);
                }
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Validation => return,
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("knave-shell-frame"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("knave-shell-clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_color),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        if let Some(painter) = &mut self.painter {
            painter.encode(
                &self.device,
                &self.queue,
                &mut encoder,
                &view,
                (self.width, self.height),
                render_list,
            );
        }
        self.frame_pending = true;
        self.layer
            .wl_surface()
            .frame(qh, FrameCallbackData(self.layer.wl_surface().clone()));
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        self.revision = self.revision.wrapping_add(1);
    }
}

impl Runtime {
    fn handle_key(&mut self, qh: &QueueHandle<Self>, event: KeyEvent) {
        if self.role != ShellRole::Overview {
            return;
        }

        let raw_keysym = event.keysym.raw();
        match raw_keysym {
            0xff1b => {
                self.exit = true;
                return;
            }
            0xff0d => {
                if let Some(action) = self.scene.search_action(self.search_index) {
                    self.dispatch_ui_action(action);
                }
                return;
            }
            0xff51 | 0xff52 => {
                self.search_index = self.search_index.saturating_sub(1);
                self.scene_dirty = true;
                self.request_draw(qh);
                return;
            }
            0xff53 | 0xff54 => {
                let result_count = self.scene.search_result_count();
                if result_count > 0 {
                    self.search_index = (self.search_index + 1).min(result_count - 1);
                }
                self.scene_dirty = true;
                self.request_draw(qh);
                return;
            }
            0xff08 => {
                if self.search_query.pop().is_some() {
                    self.search_index = 0;
                    self.scene_dirty = true;
                    self.request_draw(qh);
                }
                return;
            }
            0x31..=0x39 if self.search_query.is_empty() => {
                self.dispatch_ui_action(UiAction::FocusWorkspace(WorkspaceId(raw_keysym - 0x30)));
                return;
            }
            0x30 if self.search_query.is_empty() => {
                self.dispatch_ui_action(UiAction::FocusWorkspace(WorkspaceId(10)));
                return;
            }
            _ => {}
        }

        if let Some(text) = event.utf8 {
            let mut changed = false;
            for character in text.chars().filter(|character| !character.is_control()) {
                if self.search_query.chars().count() >= MAX_SEARCH_QUERY {
                    break;
                }
                self.search_query.push(character);
                changed = true;
            }
            if changed {
                self.search_index = 0;
                self.scene_dirty = true;
                self.request_draw(qh);
            }
        }
    }
}

impl Runtime {
    fn dispatch_ui_action(&mut self, action: UiAction) {
        match action {
            UiAction::CloseOverview => self.exit = true,
            UiAction::FocusWorkspace(workspace) => {
                self.action_worker
                    .dispatch(DesktopCommand::FocusWorkspace { workspace });
                if self.role == ShellRole::Overview {
                    self.exit = true;
                }
            }
            UiAction::FocusWindow(window) => {
                self.action_worker
                    .dispatch(DesktopCommand::FocusWindow { window });
                if self.role == ShellRole::Overview {
                    self.exit = true;
                }
            }
            UiAction::RestoreWindow(window) => {
                self.action_worker
                    .dispatch(DesktopCommand::RestoreWindow { window });
                if self.role == ShellRole::Overview {
                    self.exit = true;
                }
            }
        }
    }

    fn handle_pointer(&mut self, x: f32, y: f32) {
        if let Some(action) = self.scene.hit_test(x, y) {
            self.dispatch_ui_action(action);
        }
    }
}

impl SeatHandler for Runtime {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            match self.seat_state.get_keyboard(qh, &seat, None) {
                Ok(keyboard) => self.keyboard = Some(keyboard),
                Err(error) => eprintln!("knave-shell: could not acquire shell keyboard: {error}"),
            }
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            match self.seat_state.get_pointer(qh, &seat) {
                Ok(pointer) => self.pointer = Some(pointer),
                Err(error) => eprintln!("knave-shell: could not acquire shell pointer: {error}"),
            }
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard
            && let Some(keyboard) = self.keyboard.take()
        {
            keyboard.release();
        }
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            pointer.release();
        }
    }

    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {
    }
}

impl KeyboardHandler for Runtime {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw: &[u32],
        _keysyms: &[smithay_client_toolkit::seat::keyboard::Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        self.handle_key(qh, event);
    }

    fn repeat_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _event: KeyEvent,
    ) {
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _event: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        _modifiers: Modifiers,
        _raw_modifiers: RawModifiers,
        _layout: u32,
    ) {
    }
}

impl PointerHandler for Runtime {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let surface = self.layer.wl_surface().clone();
        for event in events {
            if event.surface != surface {
                continue;
            }
            if let PointerEventKind::Press { button, .. } = &event.kind
                && *button == BTN_LEFT
            {
                self.handle_pointer(event.position.0 as f32, event.position.1 as f32);
            }
        }
    }
}

impl CompositorHandler for Runtime {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        self.frame_pending = false;
        self.draw(qh);
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Runtime {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for Runtime {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let requested = self.role.requested_size();
        self.width =
            NonZeroU32::new(configure.new_size.0).map_or(requested.0.max(1), NonZeroU32::get);
        self.height =
            NonZeroU32::new(configure.new_size.1).map_or(requested.1.max(1), NonZeroU32::get);

        let Some(config) = self.configure_surface(self.width, self.height) else {
            self.exit = true;
            return;
        };
        self.surface.configure(&self.device, &config);
        self.painter = Some(WgpuPainter::new(&self.device, config.format));
        self.scene_dirty = true;
        self.configured = true;
        self.snapshot_worker.request_refresh();
        self.draw(qh);
    }
}

delegate_registry!(Runtime);

impl ProvidesRegistryState for Runtime {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}

smithay_client_toolkit::delegate_dispatch2!(Runtime);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_have_independent_layer_contracts() {
        assert_ne!(ShellRole::Bar.layer(), ShellRole::Overview.layer());
        assert_eq!(ShellRole::Bar.exclusive_zone(), 36);
        assert_eq!(ShellRole::Overview.exclusive_zone(), -1);
        assert_eq!(ShellRole::parse("bar"), Some(ShellRole::Bar));
        assert_eq!(ShellRole::parse("unknown"), None);
    }

    #[test]
    fn preview_decoder_accepts_bounded_rgba_png() {
        let mut encoded = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut encoded, PREVIEW_WIDTH, PREVIEW_HEIGHT);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer
                .write_image_data(&vec![17; (PREVIEW_WIDTH * PREVIEW_HEIGHT * 4) as usize])
                .unwrap();
        }
        let preview = decode_preview(WorkspacePreview {
            workspace: WorkspaceId(1),
            width: PREVIEW_WIDTH,
            height: PREVIEW_HEIGHT,
            png_base64: base64::engine::general_purpose::STANDARD.encode(encoded),
        })
        .unwrap();

        assert_eq!(preview.workspace, WorkspaceId(1));
        assert_eq!(preview.image.width(), PREVIEW_WIDTH);
        assert_eq!(preview.image.height(), PREVIEW_HEIGHT);
        assert_eq!(
            preview.image.pixels().len(),
            (PREVIEW_WIDTH * PREVIEW_HEIGHT * 4) as usize
        );
    }

    #[test]
    fn preview_decoder_rejects_unrequested_dimensions() {
        assert!(
            decode_preview(WorkspacePreview {
                workspace: WorkspaceId(1),
                width: 64,
                height: 36,
                png_base64: String::new(),
            })
            .is_none()
        );
    }
}
