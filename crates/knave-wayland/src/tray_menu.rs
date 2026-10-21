//! A transient dbusmenu presentation process, owned and reaped by the tray host.
use crate::{Application, KeyboardMode, SurfaceLayer, SurfaceOptions, Waker};
use futures_util::StreamExt;
use knave_ui::{Color, DisplayList, Rect, ShapePaint, toolkit::*};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
use zbus::{
    Proxy,
    zvariant::{OwnedValue, Value},
};

type WireLayout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);
#[derive(Clone, Debug)]
struct Row {
    id: i32,
    label: String,
    enabled: bool,
    separator: bool,
    submenu: bool,
}
#[derive(Clone, Default)]
struct Page {
    rows: Vec<Row>,
    parent: Option<i32>,
    error: Option<String>,
    close: bool,
}
enum Job {
    Open(i32),
    Click(i32),
}
struct Client {
    jobs: mpsc::Sender<Job>,
    update: Arc<Mutex<Option<Page>>>,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}
impl Client {
    fn start(owner: String, path: String, waker: Waker) -> Self {
        let (jobs, mut input) = mpsc::channel(4);
        let update = Arc::new(Mutex::new(None));
        let output = update.clone();
        let (stop, mut stopped) = oneshot::channel();
        let thread = std::thread::spawn(move || {
            let publish = |page| {
                *output.lock().unwrap() = Some(page);
                waker.wake();
            };
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            let result = match result {
                Err(error) => Err(error.to_string()),
                Ok(runtime) => runtime.block_on(async {
                    let work = async {
                        let conn = zbus::connection::Builder::session()?.method_timeout(Duration::from_secs(2)).build().await?;
                        let proxy = Proxy::new(&conn, owner.as_str(), path.as_str(), "com.canonical.dbusmenu").await?;
                        let mut changes = proxy.receive_all_signals().await?;
                        let mut owners = proxy.receive_owner_changed().await?;
                        let mut stack = vec![0];
                        let mut dirty = true;
                        loop {
                            if dirty {
                                let parent = *stack.last().unwrap();
                                // Menus may populate their children only after AboutToShow.
                                let _: bool = proxy.call("AboutToShow", &(parent,)).await.unwrap_or(false);
                                let (_, layout): (u32, WireLayout) = proxy.call("GetLayout", &(parent, 1_i32, Vec::<&str>::new())).await?;
                                publish(Page { rows: rows(layout)?, parent: (stack.len() > 1).then_some(stack[stack.len().saturating_sub(2)]), ..Default::default() });
                                dirty = false;
                            }
                            tokio::select! {
                                job = input.recv() => match job {
                                    Some(Job::Open(id)) => {
                                        if stack.len() > 1 && stack[stack.len()-2] == id { stack.pop(); }
                                        else if stack.len() < 16 { stack.push(id); }
                                        dirty = true;
                                    },
                                    Some(Job::Click(id)) => {
                                        proxy.call::<_,_,()>("Event", &(id, "clicked", Value::from(0_i32), 0_u32)).await?;
                                        publish(Page { close: true, ..Default::default() });
                                        return Ok::<_,zbus::Error>(());
                                    },
                                    None => return Ok(()),
                                },
                                signal = changes.next() => {
                                    if signal.is_none() { return Err(zbus::Error::Failure("menu disconnected".into())); }
                                    // Coalesce bursts; there is no timer while the menu is idle.
                                    tokio::time::sleep(Duration::from_millis(40)).await;
                                    dirty = true;
                                },
                                owner = owners.next() => if owner.is_none_or(|o| o.is_none()) { return Err(zbus::Error::Failure("application closed".into())); },
                            }
                        }
                    };
                    tokio::select! { _ = &mut stopped => Ok(()), result = work => result.map_err(|e| e.to_string()) }
                }),
            };
            if let Err(error) = result {
                publish(Page {
                    error: Some(error),
                    ..Default::default()
                });
            }
        });
        Self {
            jobs,
            update,
            stop: Some(stop),
            thread: Some(thread),
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn text(props: &HashMap<String, OwnedValue>, key: &str) -> String {
    props
        .get(key)
        .and_then(|v| <&str>::try_from(v).ok())
        .unwrap_or("")
        .chars()
        .take(160)
        .collect()
}
fn rows(layout: WireLayout) -> zbus::Result<Vec<Row>> {
    layout
        .2
        .into_iter()
        .take(128)
        .filter_map(|value| {
            let result = (|| {
                let (id, props, children): WireLayout = value.try_into()?;
                if props.get("visible").and_then(|v| bool::try_from(v).ok()) == Some(false) {
                    return Ok(None);
                }
                let toggle = text(&props, "toggle-type");
                let checked = props
                    .get("toggle-state")
                    .and_then(|v| i32::try_from(v).ok())
                    == Some(1);
                let label = text(&props, "label")
                    .replace("__", "\0")
                    .replace('_', "")
                    .replace('\0', "_");
                let label = if toggle.is_empty() {
                    label
                } else {
                    format!("{}  {label}", if checked { "✓" } else { "○" })
                };
                Ok(Some(Row {
                    id,
                    label,
                    enabled: props
                        .get("enabled")
                        .and_then(|v| bool::try_from(v).ok())
                        .unwrap_or(true),
                    separator: text(&props, "type") == "separator",
                    submenu: text(&props, "children-display") == "submenu" || !children.is_empty(),
                }))
            })();
            match result {
                Ok(Some(row)) => Some(Ok(row)),
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            }
        })
        .collect()
}
struct Menu {
    owner: String,
    path: String,
    x: f32,
    client: Option<Client>,
    page: Page,
    scene: Scene,
    display: DisplayList,
    dirty: bool,
    close: bool,
    size: [f32; 2],
    panel: Rect,
}
impl Menu {
    fn new(owner: String, path: String, x: f32) -> Self {
        Self {
            owner,
            path,
            x,
            client: None,
            page: Page::default(),
            scene: Scene::new(Element::new(0, Widget::Panel)).unwrap(),
            display: DisplayList::default(),
            dirty: true,
            close: false,
            size: [0.0; 2],
            panel: Rect::new(0.0, 0.0, 0.0, 0.0),
        }
    }
    fn send(&mut self, job: Job) {
        if self
            .client
            .as_ref()
            .is_none_or(|c| c.jobs.try_send(job).is_err())
        {
            self.page.error = Some("Menu busy; try again".into());
            self.dirty = true;
        }
    }
}
impl Application for Menu {
    fn set_waker(&mut self, waker: Waker) {
        self.client = Some(Client::start(self.owner.clone(), self.path.clone(), waker));
    }
    fn update(&mut self) {
        if let Some(page) = self
            .client
            .as_ref()
            .and_then(|c| c.update.lock().unwrap().take())
        {
            self.close |= page.close;
            self.page = page;
            self.dirty = true;
        }
    }
    fn input(&mut self, event: Input) {
        if matches!(
            event,
            Input::FocusLost
                | Input::Key {
                    key: Key::Escape,
                    pressed: true,
                    ..
                }
        ) {
            self.close = true;
            return;
        }
        if let Input::PointerDown(p) = event
            && !(p[0] >= self.panel.x
                && p[0] < self.panel.x + self.panel.width
                && p[1] >= self.panel.y
                && p[1] < self.panel.y + self.panel.height)
        {
            self.close = true;
            return;
        }
        if let Some(Action::Activated(id)) = self.scene.event(event).action {
            if id.0 == 3 {
                self.close = true;
            } else if id.0 == 4 {
                if let Some(parent) = self.page.parent {
                    self.send(Job::Open(parent));
                }
            } else if let Some(row) = self.page.rows.get(id.0.saturating_sub(100) as usize) {
                self.send(if row.submenu {
                    Job::Open(row.id)
                } else {
                    Job::Click(row.id)
                });
            }
        }
    }
    fn needs_frame(&self) -> bool {
        self.dirty || self.scene.needs_frame()
    }
    fn should_close(&self) -> bool {
        self.close
    }
    fn frame(&mut self, size: [f32; 2], text: &mut dyn TextMeasurer) -> &DisplayList {
        if self.dirty || size != self.size {
            self.size = size;
            let width = 280.0_f32.min(size[0]);
            let height = (self
                .page
                .rows
                .iter()
                .map(|r| if r.separator { 10.0 } else { 32.0 })
                .sum::<f32>()
                + 64.0)
                .clamp(96.0, (size[1] - 44.0).max(96.0));
            self.panel = Rect::new(
                (self.x - width / 2.0).clamp(0.0, (size[0] - width).max(0.0)),
                40.0,
                width,
                height,
            );
            let mut root = Element::new(0, Widget::Panel).layout(Layout {
                flow: Flow::Overlay,
                height: Length::Fill,
                gap: 0.0,
                ..Default::default()
            });
            let mut panel = Element::new(1, Widget::Panel).layout(Layout {
                width: Length::Px(width),
                height: Length::Px(height),
                offset: [self.panel.x, self.panel.y],
                padding: 8.0,
                gap: 4.0,
                ..Default::default()
            });
            panel.style.background = Some(ShapePaint::fill(Color::BACKGROUND));
            let mut header = Element::new(2, Widget::Panel).layout(Layout {
                flow: Flow::Row,
                height: Length::Px(28.0),
                ..Default::default()
            });
            if self.page.parent.is_some() {
                header
                    .children
                    .push(Element::new(4, Widget::Button("Back".into())));
            }
            header
                .children
                .push(Element::new(3, Widget::Button("Close".into())));
            panel.children.push(header);
            let mut list = Element::new(5, Widget::Scroll).layout(Layout {
                height: Length::Fill,
                gap: 0.0,
                ..Default::default()
            });
            if let Some(error) = &self.page.error {
                list.children.push(Element::new(
                    6,
                    Widget::Text(format!("Menu unavailable: {error}")),
                ));
            } else if self.page.rows.is_empty() {
                list.children
                    .push(Element::new(6, Widget::Text("No menu actions".into())));
            }
            for (index, row) in self.page.rows.iter().enumerate() {
                let label = if row.submenu {
                    format!("{}  ›", row.label)
                } else {
                    row.label.clone()
                };
                let mut item = Element::new(
                    100 + index as u64,
                    if row.separator {
                        Widget::Text("────────────────".into())
                    } else {
                        Widget::Button(label)
                    },
                )
                .layout(Layout {
                    height: Length::Px(if row.separator { 10.0 } else { 32.0 }),
                    ..Default::default()
                });
                item.enabled = row.enabled && !row.separator;
                list.children.push(item);
            }
            panel.children.push(list);
            root.children.push(panel);
            if let Err(error) = self.scene.replace(root) {
                eprintln!("knave-shell: tray menu layout: {error}");
                self.close = true;
            }
            self.scene.layout(size, text);
            self.dirty = false;
        }
        self.display = self.scene.display_list(size, text).clone();
        self.display.clear_color = Color::rgba(0, 0, 0, 0);
        &self.display
    }
}
/// Private shell role; endpoints are passed by the tray worker, never interpreted as commands.
pub fn run(owner: String, path: String, x: f32) -> Result<(), String> {
    zbus::names::BusName::try_from(owner.as_str()).map_err(|e| e.to_string())?;
    zbus::zvariant::ObjectPath::try_from(path.as_str()).map_err(|e| e.to_string())?;
    if !x.is_finite() {
        return Err("invalid menu position".into());
    }
    crate::run_application(
        SurfaceOptions {
            keyboard: KeyboardMode::Exclusive,
            layer: SurfaceLayer::Overlay,
            namespace: "knave-shell-tray-menu".into(),
            ..Default::default()
        },
        Menu::new(owner, path, x),
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn child(id: i32, properties: HashMap<String, OwnedValue>) -> OwnedValue {
        Value::from((id, properties, Vec::<OwnedValue>::new()))
            .try_into()
            .unwrap()
    }
    #[test]
    fn menu_preserves_disabled_checked_and_nested_actions_and_hides_invisible_rows() {
        let mut checked = HashMap::new();
        checked.insert(
            "label".into(),
            Value::from("_Keep __open").try_into().unwrap(),
        );
        checked.insert(
            "toggle-type".into(),
            Value::from("checkmark").try_into().unwrap(),
        );
        checked.insert("toggle-state".into(), OwnedValue::from(1_i32));
        checked.insert("enabled".into(), OwnedValue::from(false));
        let mut hidden = HashMap::new();
        hidden.insert("visible".into(), OwnedValue::from(false));
        let mut nested = HashMap::new();
        nested.insert(
            "children-display".into(),
            Value::from("submenu").try_into().unwrap(),
        );
        let parsed = rows((
            0,
            HashMap::new(),
            vec![child(7, checked), child(8, hidden), child(9, nested)],
        ))
        .unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, 7);
        assert_eq!(parsed[0].label, "✓  Keep _open");
        assert!(!parsed[0].enabled);
        assert!(parsed[1].submenu);
    }
}
