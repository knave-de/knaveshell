use super::{Command, Shared, State};
use gio::prelude::*;
use glib::variant::ToVariant;
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{Arc, Mutex},
};

const PATH: &str = "/org/knave/AuthenticationAgent";
const XML: &str = r#"<node><interface name="org.freedesktop.PolicyKit1.AuthenticationAgent">
<method name="BeginAuthentication"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="a{ss}" direction="in"/><arg type="s" direction="in"/><arg type="a(sa{sv})" direction="in"/></method>
<method name="CancelAuthentication"><arg type="s" direction="in"/></method>
</interface></node>"#;
type Identity = (String, HashMap<String, glib::Variant>);
type Begin = (
    String,
    String,
    String,
    HashMap<String, String>,
    String,
    Vec<Identity>,
);
struct Pending {
    generation: u64,
    cookie: String,
    identities: Vec<polkit::Identity>,
    invocation: gio::DBusMethodInvocation,
    session: Option<polkit_agent::Session>,
    waiting: bool,
}
struct Backend {
    shared: Arc<Mutex<Shared>>,
    pending: Option<Pending>,
    generation: u64,
}
impl Backend {
    fn publish(&self, f: impl FnOnce(&mut State)) {
        self.shared.lock().unwrap().change(f);
    }
    fn begin(&mut self, parameters: &glib::Variant, invocation: gio::DBusMethodInvocation) {
        if self.pending.is_some() {
            invocation.return_dbus_error(
                "org.freedesktop.PolicyKit1.Error.Failed",
                "Authentication already in progress",
            );
            return;
        }
        let Some((action, message, _, _, cookie, identities)) = parameters.get::<Begin>() else {
            invocation.return_dbus_error(
                "org.freedesktop.DBus.Error.InvalidArgs",
                "Invalid authentication request",
            );
            return;
        };
        if action.len() > 512
            || message.len() > 4096
            || cookie.len() > 4096
            || identities.is_empty()
            || identities.len() > 32
        {
            invocation.return_dbus_error(
                "org.freedesktop.DBus.Error.InvalidArgs",
                "Authentication request exceeds limits",
            );
            return;
        }
        let mut users = Vec::new();
        let mut names = Vec::new();
        for (kind, details) in identities {
            if kind != "unix-user" {
                continue;
            }
            let Some(uid) = details.get("uid").and_then(|v| v.get::<u32>()) else {
                continue;
            };
            if let Ok(uid) = i32::try_from(uid) {
                let user = polkit::UnixUser::new(uid);
                names.push(
                    user.downcast_ref::<polkit::UnixUser>()
                        .unwrap()
                        .name()
                        .map_or_else(|| format!("User {uid}"), |name| name.to_string()),
                );
                users.push(user.upcast());
            }
        }
        if users.is_empty() {
            invocation.return_dbus_error(
                "org.freedesktop.PolicyKit1.Error.Failed",
                "No supported Unix user identity",
            );
            return;
        }
        self.generation += 1;
        self.pending = Some(Pending {
            generation: self.generation,
            cookie,
            identities: users,
            invocation,
            session: None,
            waiting: false,
        });
        self.publish(|state| {
            *state = State {
                generation: self.generation,
                visible: true,
                message,
                action,
                identities: names,
                ..Default::default()
            }
        });
    }
}
fn select(backend: &Rc<RefCell<Backend>>, generation: u64, index: usize) {
    let session = {
        let mut b = backend.borrow_mut();
        let Some(p) = b
            .pending
            .as_mut()
            .filter(|p| p.generation == generation && p.session.is_none())
        else {
            return;
        };
        let Some(identity) = p.identities.get(index) else {
            return;
        };
        let session = polkit_agent::Session::new(identity, &p.cookie);
        p.session = Some(session.clone());
        session
    };
    let weak = Rc::downgrade(backend);
    session.connect_request(move |_, prompt, echo| {
        if let Some(backend) = weak.upgrade() {
            let mut b = backend.borrow_mut();
            if let Some(p) = b.pending.as_mut().filter(|p| p.generation == generation) {
                p.waiting = true;
                b.publish(|s| {
                    s.prompt = Some((prompt.chars().take(1024).collect(), echo));
                    s.identities.clear();
                });
            }
        }
    });
    let weak = Rc::downgrade(backend);
    session.connect_completed(move |_, _authorized| {
        if let Some(backend) = weak.upgrade() {
            let mut b = backend.borrow_mut();
            if b.pending
                .as_ref()
                .is_some_and(|p| p.generation == generation)
            {
                let p = b.pending.take().unwrap();
                // The privileged helper reports authorization to polkitd. This reply only
                // completes the conversation and cannot itself authorize the action.
                p.invocation.return_value(Some(&().to_variant()));
                b.publish(|s| *s = State::default());
            }
        }
    });
    for error in [false, true] {
        let weak = Rc::downgrade(backend);
        let show = move |_: &polkit_agent::Session, message: &str| {
            if let Some(backend) = weak.upgrade() {
                let b = backend.borrow();
                if b.pending
                    .as_ref()
                    .is_some_and(|p| p.generation == generation)
                {
                    b.publish(|s| s.notice = message.chars().take(1024).collect());
                }
            }
        };
        if error {
            session.connect_show_error(show);
        } else {
            session.connect_show_info(show);
        }
    }
    session.initiate();
}

pub(super) fn run(shared: Arc<Mutex<Shared>>, commands: async_channel::Receiver<Command>) {
    let context = glib::MainContext::new();
    let result = context.with_thread_default(|| {
        let mut previous_error = String::new();
        loop {
            match serve(shared.clone(), commands.clone()) {
                Ok(()) => break,
                Err(error) => {
                    if error != previous_error {
                        eprintln!("knave-shell: Polkit agent unavailable: {error}");
                        previous_error = error;
                    }
                }
            }
            let retry = context.block_on(futures_lite::future::race(
                async { commands.recv().await.is_ok() },
                async {
                    glib::timeout_future_seconds(5).await;
                    true
                },
            ));
            if !retry {
                break;
            }
        }
    });
    if let Err(error) = result {
        eprintln!("knave-shell: Polkit context unavailable: {error}");
    }
}
fn serve(
    shared: Arc<Mutex<Shared>>,
    commands: async_channel::Receiver<Command>,
) -> Result<(), String> {
    let connection = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE)
        .map_err(|e| e.to_string())?;
    connection.set_exit_on_close(false);
    let authority = polkit::Authority::sync(gio::Cancellable::NONE).map_err(|e| e.to_string())?;
    let subject = polkit::UnixSession::new_for_process_sync(
        std::process::id() as i32,
        gio::Cancellable::NONE,
    )
    .map_err(|e| e.to_string())?
    .ok_or("No logind session for authentication agent")?;
    let backend = Rc::new(RefCell::new(Backend {
        shared,
        pending: None,
        generation: 0,
    }));
    let info = gio::DBusNodeInfo::for_xml(XML).map_err(|e| e.to_string())?;
    let b = backend.clone();
    let a = authority.clone();
    let registration = connection
        .register_object(
            PATH,
            &info.interfaces()[0],
            move |_, sender, _, _, method, parameters, invocation| {
                dispatch(
                    &b,
                    a.owner().as_deref(),
                    sender,
                    method,
                    &parameters,
                    invocation,
                );
            },
            |_, _, _, _, _| ().to_variant(),
            |_, _, _, _, _, _| false,
        )
        .map_err(|e| e.to_string())?;
    let locale = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_else(|_| "C".into());
    // An owner change invalidates all outstanding cookies. Register again after polkitd restarts.
    let b = backend.clone();
    let s = subject.clone();
    let l = locale.clone();
    let owner_handler = authority.connect_owner_notify(move |a| {
        cancel(&b);
        if a.owner().is_some() {
            a.register_authentication_agent(&s, &l, PATH, gio::Cancellable::NONE, |result| {
                if let Err(e) = result {
                    eprintln!("knave-shell: Polkit registration failed: {e}");
                }
            });
        }
    });
    if let Err(error) = authority.register_authentication_agent_sync(
        &subject,
        &locale,
        PATH,
        gio::Cancellable::NONE,
    ) {
        eprintln!("knave-shell: Polkit registration failed: {error}");
    }
    let main_loop = glib::MainLoop::new(Some(&glib::MainContext::ref_thread_default()), false);
    let done = main_loop.clone();
    let b = backend.clone();
    let command_task = glib::MainContext::ref_thread_default().spawn_local(async move {
        while let Ok(command) = commands.recv().await {
            match command {
                Command::Select(g, i) => select(&b, g, i),
                Command::Cancel(g) => {
                    let matches = b
                        .borrow()
                        .pending
                        .as_ref()
                        .is_some_and(|p| p.generation == g);
                    if matches {
                        cancel(&b);
                    }
                }
                Command::Response(g, response) => {
                    let session = {
                        let mut b = b.borrow_mut();
                        b.pending
                            .as_mut()
                            .filter(|p| p.generation == g && p.waiting)
                            .and_then(|p| {
                                p.waiting = false;
                                p.session.clone()
                            })
                    };
                    if let Some(session) = session {
                        session.response(&response);
                    }
                }
            }
        }
        cancel(&b);
        done.quit();
    });
    let closed_loop = main_loop.clone();
    let closed_handler = connection.connect_closed(move |_, _, _| closed_loop.quit());
    main_loop.run();
    command_task.abort();
    cancel(&backend);
    connection.disconnect(closed_handler);
    authority.disconnect(owner_handler);
    let _ = connection.unregister_object(registration);
    if connection.is_closed() {
        return Err("System bus disconnected".into());
    }
    let _ = authority.unregister_authentication_agent_sync(&subject, PATH, gio::Cancellable::NONE);
    Ok(())
}
fn cancel(backend: &Rc<RefCell<Backend>>) {
    let pending = backend.borrow_mut().pending.take();
    if let Some(pending) = pending {
        if let Some(session) = pending.session {
            session.cancel();
        }
        pending.invocation.return_dbus_error(
            "org.freedesktop.PolicyKit1.Error.Cancelled",
            "Authentication cancelled",
        );
    }
    backend.borrow().publish(|s| *s = State::default());
}

fn dispatch(
    backend: &Rc<RefCell<Backend>>,
    owner: Option<&str>,
    sender: &str,
    method: &str,
    parameters: &glib::Variant,
    invocation: gio::DBusMethodInvocation,
) {
    // System bus access alone is insufficient: only the current polkitd may prompt.
    if Some(sender) != owner {
        invocation.return_dbus_error(
            "org.freedesktop.DBus.Error.AccessDenied",
            "Only polkitd may invoke the authentication agent",
        );
        return;
    }
    match method {
        "BeginAuthentication" => {
            let available = backend.borrow().pending.is_none();
            backend.borrow_mut().begin(parameters, invocation);
            let single = backend
                .borrow()
                .pending
                .as_ref()
                .filter(|p| available && p.identities.len() == 1)
                .map(|p| p.generation);
            if let Some(generation) = single {
                select(backend, generation, 0);
            }
        }
        "CancelAuthentication" => {
            let cookie = parameters.get::<(String,)>().map(|v| v.0);
            let matches = backend
                .borrow()
                .pending
                .as_ref()
                .is_some_and(|p| Some(&p.cookie) == cookie.as_ref());
            if matches {
                // Cancel outside the backend borrow: session signals may be synchronous.
                cancel(backend);
                invocation.return_value(Some(&().to_variant()));
            } else {
                invocation.return_dbus_error(
                    "org.freedesktop.PolicyKit1.Error.Failed",
                    "Unknown authentication cookie",
                );
            }
        }
        _ => invocation
            .return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", "Unknown method"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
    };
    struct Bus(Child);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn private_bus_rejects_spoofing_overlapping_and_invalid_requests_and_cancels() {
        let mut bus = Bus(Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("private dbus-daemon"));
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let context = glib::MainContext::new();
        context
            .with_thread_default(|| {
                let connect = || {
                    gio::DBusConnection::for_address_sync(
                        address.trim(),
                        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
                        None,
                        gio::Cancellable::NONE,
                    )
                    .unwrap()
                };
                let server = connect();
                let client = connect();
                let stranger = connect();
                let shared = Arc::new(Mutex::new(Shared::default()));
                let backend = Rc::new(RefCell::new(Backend {
                    shared: shared.clone(),
                    pending: None,
                    generation: 0,
                }));
                let info = gio::DBusNodeInfo::for_xml(XML).unwrap();
                let owner = client.unique_name().unwrap().to_string();
                let b = backend.clone();
                let registration = server
                    .register_object(
                        PATH,
                        &info.interfaces()[0],
                        move |_, sender, _, _, method, parameters, invocation| {
                            dispatch(&b, Some(&owner), sender, method, &parameters, invocation)
                        },
                        |_, _, _, _, _| ().to_variant(),
                        |_, _, _, _, _, _| false,
                    )
                    .unwrap();
                let target = server.unique_name().unwrap();
                let request = |count: usize, kind: &str| -> glib::Variant {
                    (
                        "org.knave.test".to_owned(),
                        "Test authentication".to_owned(),
                        String::new(),
                        HashMap::<String, String>::new(),
                        "cookie".to_owned(),
                        (0..count)
                            .map(|_| {
                                (
                                    kind.to_owned(),
                                    HashMap::from([("uid".to_owned(), 0u32.to_variant())]),
                                )
                            })
                            .collect::<Vec<_>>(),
                    )
                        .to_variant()
                };
                let call =
                    |connection: &gio::DBusConnection, method: &str, parameters: &glib::Variant| {
                        context.block_on(connection.call_future(
                            Some(&target),
                            PATH,
                            "org.freedesktop.PolicyKit1.AuthenticationAgent",
                            method,
                            Some(parameters),
                            None,
                            gio::DBusCallFlags::NONE,
                            2000,
                        ))
                    };
                assert!(
                    call(&stranger, "BeginAuthentication", &request(1, "unix-user"))
                        .unwrap_err()
                        .to_string()
                        .contains("AccessDenied")
                );
                assert!(!shared.lock().unwrap().state.visible);
                for invalid in [
                    request(0, "unix-user"),
                    request(33, "unix-user"),
                    request(1, "unix-group"),
                ] {
                    assert!(call(&client, "BeginAuthentication", &invalid).is_err());
                    assert!(!shared.lock().unwrap().state.visible);
                }
                let (sender, receiver) = async_channel::bounded(1);
                client.call(
                    Some(&target),
                    PATH,
                    "org.freedesktop.PolicyKit1.AuthenticationAgent",
                    "BeginAuthentication",
                    Some(&request(2, "unix-user")),
                    None,
                    gio::DBusCallFlags::NONE,
                    2000,
                    gio::Cancellable::NONE,
                    move |result| {
                        sender.try_send(result).unwrap();
                    },
                );
                assert!(
                    call(&client, "BeginAuthentication", &request(2, "unix-user"))
                        .unwrap_err()
                        .to_string()
                        .contains("already in progress")
                );
                assert!(shared.lock().unwrap().state.visible);
                assert_eq!(shared.lock().unwrap().state.identities.len(), 2);
                assert!(call(&client, "CancelAuthentication", &("wrong",).to_variant()).is_err());
                assert!(backend.borrow().pending.is_some());
                assert!(
                    call(&stranger, "CancelAuthentication", &("cookie",).to_variant()).is_err()
                );
                call(&client, "CancelAuthentication", &("cookie",).to_variant()).unwrap();
                let completion = context.block_on(receiver.recv()).unwrap().unwrap_err();
                assert!(completion.to_string().contains("Cancelled"));
                assert!(backend.borrow().pending.is_none());
                assert!(!shared.lock().unwrap().state.visible);
                server.unregister_object(registration).unwrap();
                for connection in [server, client, stranger] {
                    connection.close_sync(gio::Cancellable::NONE).unwrap();
                }
            })
            .unwrap();
    }
}
