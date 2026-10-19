//! One cancellable D-Bus subscriber and bounded command queue per presentation host.
use crate::Waker;
use futures_util::StreamExt;
use knave_desktop_api::notifications::*;
use std::{
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

pub struct NotificationClient {
    commands: mpsc::Sender<NotificationCommand>,
    updates: Arc<Mutex<Option<Result<NotificationSnapshot, String>>>>,
    errors: Arc<Mutex<Option<String>>>,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}
impl NotificationClient {
    pub fn start(waker: Waker) -> Self {
        let (commands, mut jobs) = mpsc::channel::<NotificationCommand>(16);
        let updates = Arc::new(Mutex::new(None));
        let output = updates.clone();
        let errors = Arc::new(Mutex::new(None));
        let failures = errors.clone();
        let (stop, mut stopped) = oneshot::channel();
        let thread = thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(r) => r,
                Err(e) => {
                    *output.lock().unwrap() = Some(Err(e.to_string()));
                    waker.wake();
                    return;
                }
            };
            runtime.block_on(async move {
                let mut backoff=Duration::from_millis(250);
                loop {
                    let run=async {
                        let connection=zbus::connection::Builder::session()?.method_timeout(Duration::from_secs(2)).build().await?;
                        let proxy=zbus::Proxy::new(&connection,BUS_NAME,OBJECT_PATH,INTERFACE).await?;
                        let mut changes=proxy.receive_signal("Changed").await?;
                        let mut owners=proxy.receive_owner_changed().await?;
                        async fn snapshot(proxy:&zbus::Proxy<'_>)->Result<NotificationSnapshot,String> {
                            let json:String=proxy.call("Snapshot",&()).await.map_err(|e|e.to_string())?;
                            if json.len()>4*1024*1024 {return Err("notification snapshot exceeds budget".into())}
                            serde_json::from_str(&json).map_err(|e|e.to_string())
                        }
                        let initial=snapshot(&proxy).await.map_err(zbus::Error::Failure)?;
                        *output.lock().unwrap()=Some(Ok(initial));waker.wake();backoff=Duration::from_millis(250);
                        loop {
                            tokio::select!{
                                signal=changes.next()=>{
                                    if signal.is_none(){return Err(zbus::Error::Failure("notification service disconnected".into()))}
                                    let state=snapshot(&proxy).await.map_err(zbus::Error::Failure)?;
                                    *output.lock().unwrap()=Some(Ok(state));waker.wake();
                                },
                                owner=owners.next()=>{if owner.is_none_or(|o|o.is_none()){return Err(zbus::Error::Failure("notification service stopped".into()))}},
                                job=jobs.recv()=>{
                                    let Some(command)=job else {return Ok(())};
                                    match proxy.call::<_,_,()>("Command",&(serde_json::to_string(&command).unwrap(),)).await {
                                        Ok(())=>{},Err(e)=>{*failures.lock().unwrap()=Some(e.to_string());waker.wake();}
                                    }
                                }
                            }
                        }
                    };
                    let result:Result<(),zbus::Error>=tokio::select!{_=&mut stopped=>break,result=run=>result};
                    if let Err(error)=result {*output.lock().unwrap()=Some(Err(error.to_string()));waker.wake();}
                    tokio::select!{_=&mut stopped=>break,_=tokio::time::sleep(backoff)=>{}}
                    backoff=(backoff*2).min(Duration::from_secs(5));
                }
            });
        });
        Self {
            commands,
            updates,
            errors,
            stop: Some(stop),
            thread: Some(thread),
        }
    }
    pub fn command(&self, command: NotificationCommand) -> Result<(), String> {
        self.commands
            .try_send(command)
            .map_err(|e| format!("notification action queue unavailable: {e}"))
    }
    pub fn latest(&self) -> Option<Result<NotificationSnapshot, String>> {
        self.updates.lock().unwrap().take()
    }
    pub fn error(&self) -> Option<String> {
        self.errors.lock().unwrap().take()
    }
    pub fn has_update(&self) -> bool {
        self.updates.lock().unwrap().is_some() || self.errors.lock().unwrap().is_some()
    }
}
impl Drop for NotificationClient {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Stable right-edge bell and count. Counts describe unread history, independent of banner lifetime.
pub(super) fn append_bell(
    list: &mut knave_ui::DisplayList,
    width: f32,
    snapshot: Option<&NotificationSnapshot>,
) {
    use knave_ui::{
        Color, CornerRadii, DisplayListBuilder, Rect, ShapePaint, TextAlign, TextStyle,
    };
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
