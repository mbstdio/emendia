//! These tests use a private D-Bus daemon and a fake portal, never the user's desktop.
use super::*;
use dbus::{
    arg::{PropMap, Variant},
    channel::{MatchingReceiver, Sender},
    message::MatchRule,
};
use std::{
    os::{fd::IntoRawFd, unix::net::UnixStream},
    sync::mpsc,
    thread,
    time::Instant,
};

const DESKTOP: &str = "/org/freedesktop/portal/desktop";
const CLIPBOARD: &str = "org.freedesktop.portal.Clipboard";
const SHORTCUTS: &str = "org.freedesktop.portal.GlobalShortcuts";

enum Command {
    Release(String),
    External(Formats, String),
    Transfer(String),
    Close,
    Quit,
}

#[derive(Default)]
struct Fixture {
    session: String,
    shortcuts_session: String,
    current: Formats,
    selected: String,
    copied: usize,
    transfers: Vec<Vec<u8>>,
    closed: usize,
    concurrent_on_read: bool,
    deny_start: bool,
}

struct MockPortal {
    commands: mpsc::Sender<Command>,
    fixture: Arc<Mutex<Fixture>>,
    thread: Option<thread::JoinHandle<()>>,
}

fn property<T: dbus::arg::RefArg + 'static>(value: T) -> Variant<Box<dyn dbus::arg::RefArg>> {
    Variant(Box::new(value))
}

fn owner(connection: &dbus::blocking::Connection, session: &str, ours: bool, formats: Vec<String>) {
    let options = PropMap::from([
        ("mime_types".into(), property(formats)),
        ("session_is_owner".into(), property(ours)),
    ]);
    connection
        .send(
            dbus::Message::new_signal(DESKTOP, CLIPBOARD, "SelectionOwnerChanged")
                .unwrap()
                .append2(dbus::Path::new(session.to_owned()).unwrap(), options),
        )
        .unwrap();
}

impl MockPortal {
    fn new() -> Result<Self> {
        let connection = dbus::blocking::Connection::new_session()?;
        connection.request_name("org.freedesktop.portal.Desktop", false, true, false)?;
        let fixture = Arc::new(Mutex::new(Fixture {
            current: original(),
            selected: "Bonjour".into(),
            ..Default::default()
        }));
        let data = fixture.clone();
        connection.start_receive(
            MatchRule::new_method_call(),
            Box::new(move |message, connection| {
                let method = message.member().unwrap().to_string();
                let interface = message.interface().unwrap().to_string();
                let mut fixture = data.lock().unwrap();
                let mut results = PropMap::new();
                let options = match method.as_str() {
                    "CreateSession" => {
                        let options: PropMap = message.read1().unwrap();
                        let token = options["session_handle_token"].0.as_str().unwrap();
                        let sender = message
                            .sender()
                            .unwrap()
                            .trim_start_matches(':')
                            .replace('.', "_");
                        let path = format!("{DESKTOP}/session/{sender}/{token}");
                        if interface == SHORTCUTS {
                            fixture.shortcuts_session = path.clone();
                        } else {
                            fixture.session = path.clone();
                        }
                        results.insert("session_handle".into(), property(path));
                        Some(options)
                    }
                    "SelectDevices" => {
                        let (_, options): (dbus::Path, PropMap) = message.read2().unwrap();
                        assert_eq!(
                            options["types"].0.as_u64(),
                            Some(1),
                            "Only request the keyboard"
                        );
                        Some(options)
                    }
                    "Start" => {
                        let (_, _, options): (dbus::Path, String, PropMap) =
                            message.read3().unwrap();
                        results.insert("devices".into(), property(1_u32));
                        results.insert("clipboard_enabled".into(), property(true));
                        owner(
                            connection,
                            &fixture.session,
                            false,
                            fixture.current.keys().cloned().collect(),
                        );
                        Some(options)
                    }
                    "BindShortcuts" => {
                        let (_, shortcuts, _, options): (
                            dbus::Path,
                            Vec<(String, PropMap)>,
                            String,
                            PropMap,
                        ) = message.read4().unwrap();
                        assert_eq!(shortcuts.len(), 4);
                        let bound = shortcuts
                            .into_iter()
                            .map(|(id, info)| {
                                assert!(
                                    info["preferred_trigger"]
                                        .0
                                        .as_str()
                                        .unwrap()
                                        .starts_with("CTRL+")
                                );
                                (
                                    id.clone(),
                                    PropMap::from([
                                        ("description".into(), property(id)),
                                        (
                                            "trigger_description".into(),
                                            property("Desktop override".to_owned()),
                                        ),
                                    ]),
                                )
                            })
                            .collect::<Vec<_>>();
                        results.insert("shortcuts".into(), property(bound));
                        Some(options)
                    }
                    _ => None,
                };
                if let Some(options) = options {
                    let sender = message
                        .sender()
                        .unwrap()
                        .trim_start_matches(':')
                        .replace('.', "_");
                    let token = options["handle_token"].0.as_str().unwrap();
                    let request = format!("{DESKTOP}/request/{sender}/{token}");
                    let cancelled = method == "Start" && fixture.deny_start;
                    if method == "Start" {
                        fixture.deny_start = false;
                    }
                    connection
                        .send(
                            message
                                .method_return()
                                .append1(dbus::Path::new(request.clone()).unwrap()),
                        )
                        .unwrap();
                    connection
                        .send(
                            dbus::Message::new_signal(
                                request,
                                "org.freedesktop.portal.Request",
                                "Response",
                            )
                            .unwrap()
                            .append2(if cancelled { 1_u32 } else { 0_u32 }, results),
                        )
                        .unwrap();
                    return true;
                }
                let reply = match method.as_str() {
                    "Get" => message.method_return().append1(Variant(2_u32)),
                    "GetAll" => message
                        .method_return()
                        .append1(PropMap::from([("version".into(), property(2_u32))])),
                    "SelectionRead" => {
                        let (_, mime): (dbus::Path, String) = message.read2().unwrap();
                        let bytes = fixture.current[&mime].clone();
                        let concurrent = fixture.concurrent_on_read
                            && fixture.copied > 0
                            && fixture.current.len() == 1;
                        if concurrent {
                            fixture.concurrent_on_read = false;
                            fixture.current = HashMap::from([(
                                "text/plain;charset=utf-8".into(),
                                Arc::new(b"concurrent writer".to_vec()),
                            )]);
                            owner(
                                connection,
                                &fixture.session,
                                false,
                                fixture.current.keys().cloned().collect(),
                            );
                        }
                        let (read, mut write) = UnixStream::pair().unwrap();
                        thread::spawn(move || {
                            if concurrent {
                                thread::sleep(Duration::from_millis(100));
                            }
                            write.write_all(&bytes).unwrap();
                        });
                        // Ownership of the descriptor is transferred into the D-Bus message.
                        let fd = unsafe { dbus::arg::OwnedFd::new(read.into_raw_fd()) };
                        message.method_return().append1(fd)
                    }
                    "SetSelection" => {
                        let (_, options): (dbus::Path, PropMap) = message.read2().unwrap();
                        let formats = options["mime_types"]
                            .0
                            .as_iter()
                            .unwrap()
                            .map(|value| value.as_str().unwrap().to_owned())
                            .collect();
                        owner(connection, &fixture.session, true, formats);
                        message.method_return()
                    }
                    "NotifyKeyboardKeysym" => {
                        let (_, _, keysym, state): (dbus::Path, PropMap, i32, u32) =
                            message.read4().unwrap();
                        if keysym == 'c' as i32 && state == 0 {
                            fixture.copied += 1;
                            fixture.current = HashMap::from([(
                                "text/plain;charset=utf-8".into(),
                                Arc::new(fixture.selected.as_bytes().to_vec()),
                            )]);
                            owner(
                                connection,
                                &fixture.session,
                                false,
                                fixture.current.keys().cloned().collect(),
                            );
                        }
                        message.method_return()
                    }
                    "SelectionWrite" => {
                        let (mut read, write) = UnixStream::pair().unwrap();
                        let data = data.clone();
                        thread::spawn(move || {
                            let mut bytes = Vec::new();
                            read.read_to_end(&mut bytes).unwrap();
                            data.lock().unwrap().transfers.push(bytes);
                        });
                        let fd = unsafe { dbus::arg::OwnedFd::new(write.into_raw_fd()) };
                        message.method_return().append1(fd)
                    }
                    "Close" => {
                        fixture.closed += 1;
                        message.method_return()
                    }
                    _ => message.method_return(),
                };
                connection.send(reply).unwrap();
                true
            }),
        );
        let (commands, receiver) = mpsc::channel();
        let data = fixture.clone();
        let thread = thread::spawn(move || {
            loop {
                connection.process(Duration::from_millis(10)).unwrap();
                while let Ok(command) = receiver.try_recv() {
                    let mut fixture = data.lock().unwrap();
                    match command {
                        Command::Release(id) => {
                            connection
                                .send(
                                    dbus::Message::new_signal(DESKTOP, SHORTCUTS, "Deactivated")
                                        .unwrap()
                                        .append3(
                                            dbus::Path::new(fixture.shortcuts_session.clone())
                                                .unwrap(),
                                            id,
                                            0_u64,
                                        )
                                        .append1(PropMap::new()),
                                )
                                .unwrap();
                        }
                        Command::External(formats, selected) => {
                            fixture.current = formats;
                            fixture.selected = selected;
                            owner(
                                &connection,
                                &fixture.session,
                                false,
                                fixture.current.keys().cloned().collect(),
                            );
                        }
                        Command::Transfer(mime) => {
                            connection
                                .send(
                                    dbus::Message::new_signal(
                                        DESKTOP,
                                        CLIPBOARD,
                                        "SelectionTransfer",
                                    )
                                    .unwrap()
                                    .append3(
                                        dbus::Path::new(fixture.session.clone()).unwrap(),
                                        mime,
                                        42_u32,
                                    ),
                                )
                                .unwrap();
                        }
                        Command::Close => {
                            connection
                                .send(
                                    dbus::Message::new_signal(
                                        fixture.session.clone(),
                                        "org.freedesktop.portal.Session",
                                        "Closed",
                                    )
                                    .unwrap()
                                    .append1(PropMap::new()),
                                )
                                .unwrap();
                        }
                        Command::Quit => return,
                    }
                }
            }
        });
        Ok(Self {
            commands,
            fixture,
            thread: Some(thread),
        })
    }
}

impl Drop for MockPortal {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn original() -> Formats {
    HashMap::from([
        (
            "text/plain;charset=utf-8".into(),
            Arc::new(b"previous clipboard".to_vec()),
        ),
        (
            "application/octet-stream".into(),
            Arc::new(vec![0, 255, 10, 128]),
        ),
    ])
}

fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "Mock portal timed out");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "run with dbus-run-session on an isolated session bus"]
fn wayland_portal_round_trip() -> Result<()> {
    assert_eq!(
        std::env::var("EMENDIA_PORTAL_TEST").as_deref(),
        Ok("1"),
        "Do not replace the user's portal"
    );
    let mock = MockPortal::new()?;
    initialize()?;
    let keys =
        crate::platform::hotkey::parse_shortcuts(crate::settings::Settings::default().shortcuts())?;
    set_shortcuts(Some(keys))?;
    wait(|| status().contains("Desktop override"));
    assert!(shortcuts_active());
    let closed = mock.fixture.lock().unwrap().closed;
    set_shortcuts(Some(keys))?;
    assert_eq!(
        mock.fixture.lock().unwrap().closed,
        closed,
        "Unchanged preferences retain the session"
    );
    mock.commands.send(Command::Release("proofread".into()))?;
    let mut event = None;
    wait(|| {
        event = next_event();
        event.is_some()
    });
    assert_eq!(event.unwrap().id, keys[2].id());
    set_shortcuts(None)?;
    assert!(!shortcuts_active());
    wait(|| mock.fixture.lock().unwrap().closed >= 1);
    assert!(next_event().is_none());

    let service = runtime().block_on(ClipboardService::new())?;
    assert_eq!(runtime().block_on(service.capture())?, "Bonjour");
    assert_eq!(
        *service.contents.lock().unwrap(),
        original(),
        "Restore every advertised format"
    );
    mock.commands
        .send(Command::Transfer("application/octet-stream".into()))?;
    wait(|| !mock.fixture.lock().unwrap().transfers.is_empty());
    assert_eq!(
        mock.fixture.lock().unwrap().transfers[0],
        vec![0, 255, 10, 128]
    );
    let generation = service.owner.borrow().generation;
    mock.commands
        .send(Command::External(HashMap::new(), "Bonjour".into()))?;
    wait(|| service.owner.borrow().generation > generation);
    assert_eq!(runtime().block_on(service.capture())?, "Bonjour");
    assert!(
        service.contents.lock().unwrap().is_empty(),
        "An initially empty clipboard is cleared after capture"
    );

    for selected in [" ".to_owned(), "😀".repeat(50_001)] {
        let generation = service.owner.borrow().generation;
        mock.commands
            .send(Command::External(original(), selected))?;
        wait(|| service.owner.borrow().generation > generation);
        assert!(runtime().block_on(service.capture()).is_err());
        assert_eq!(
            *service.contents.lock().unwrap(),
            original(),
            "Failed validation restores clipboard too"
        );
    }
    let generation = service.owner.borrow().generation;
    mock.commands
        .send(Command::External(original(), "Bonjour".into()))?;
    wait(|| service.owner.borrow().generation > generation);
    mock.fixture.lock().unwrap().concurrent_on_read = true;
    assert!(runtime().block_on(service.capture()).is_err());
    assert_eq!(
        mock.fixture.lock().unwrap().current["text/plain;charset=utf-8"].as_slice(),
        b"concurrent writer"
    );
    runtime().block_on(service.publish(HashMap::from([(
        "text/plain;charset=utf-8".into(),
        Arc::new(b"result".to_vec()),
    )])))?;
    mock.commands
        .send(Command::Transfer("text/plain;charset=utf-8".into()))?;
    wait(|| mock.fixture.lock().unwrap().transfers.len() == 2);
    assert_eq!(mock.fixture.lock().unwrap().transfers[1], b"result");
    mock.commands.send(Command::Close)?;
    wait(|| service.closed.load(Ordering::Acquire));
    assert!(runtime().block_on(service.capture()).is_err());
    let closed = mock.fixture.lock().unwrap().closed;
    mock.fixture.lock().unwrap().deny_start = true;
    assert!(runtime().block_on(ClipboardService::new()).is_err());
    wait(|| mock.fixture.lock().unwrap().closed > closed);
    Ok(())
}
