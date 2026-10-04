//! Wayland desktop services. Portal dialogs and transfers never block GPUI.
use anyhow::{Context, Result, bail};
use ashpd::desktop::{
    Session,
    clipboard::{Clipboard, SetSelectionOptions},
    global_shortcuts::{GlobalShortcuts, NewShortcut, Shortcut},
    remote_desktop::{DeviceType, KeyState, RemoteDesktop, SelectDevicesOptions},
};
use futures_util::StreamExt;
use global_hotkey::{
    GlobalHotKeyEvent, HotKeyState,
    hotkey::{HotKey, Modifiers},
};
use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    io::{Read, Write},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{io::unix::AsyncFd, runtime::Runtime, sync::watch, task::JoinHandle};

const ACTIONS: [&str; 4] = ["translate", "quick_translate", "proofread", "quick_check"];
const TRANSFER_LIMIT: usize = 16 * 1024 * 1024;
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(3);
const DIALOG_TIMEOUT: Duration = Duration::from_secs(120);
type Formats = HashMap<String, Arc<Vec<u8>>>;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static SHARED: OnceLock<Mutex<State>> = OnceLock::new();
static REVISION: AtomicU64 = AtomicU64::new(0);
static CHANGED: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct State {
    shortcuts_task: Option<JoinHandle<()>>,
    shortcuts_session: Option<Arc<Session<GlobalShortcuts>>>,
    requested_keys: Option<[HotKey; 4]>,
    capture_task: Option<JoinHandle<()>>,
    clipboard: Option<Arc<ClipboardService>>,
    events: VecDeque<GlobalHotKeyEvent>,
    errors: VecDeque<String>,
    shortcuts: String,
    bindings: Option<[Option<String>; 4]>,
    shortcuts_version: u32,
    capture: String,
}

fn state() -> &'static Mutex<State> {
    SHARED.get_or_init(|| Mutex::new(State::default()))
}

pub(crate) fn initialize() -> Result<()> {
    if RUNTIME.get().is_none() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("emendia-portals")
            .enable_all()
            .build()?;
        let _ = RUNTIME.set(runtime);
    }
    Ok(())
}

fn runtime() -> &'static Runtime {
    RUNTIME.get().expect("Wayland portal runtime initialized")
}

fn report(error: anyhow::Error) {
    let message = format!("{error:#}");
    tracing::warn!(%message, "Wayland portal operation failed");
    let mut state = state().lock().unwrap();
    if state.errors.len() == 8 {
        state.errors.pop_front();
    }
    state.errors.push_back(message);
    CHANGED.store(true, Ordering::Release);
}

pub(crate) fn next_error() -> Option<String> {
    state().lock().unwrap().errors.pop_front()
}
pub(crate) fn next_event() -> Option<GlobalHotKeyEvent> {
    state().lock().unwrap().events.pop_front()
}
pub(crate) fn take_changed() -> bool {
    CHANGED.swap(false, Ordering::AcqRel)
}

static NOTIFICATION: OnceLock<tokio::sync::Mutex<u32>> = OnceLock::new();

pub(crate) fn notify_status(message: String) {
    runtime().spawn(async move {
        let mut id = NOTIFICATION
            .get_or_init(|| tokio::sync::Mutex::new(0))
            .lock()
            .await;
        let result = async {
            let connection = ashpd::zbus::Connection::session().await?;
            let proxy = ashpd::zbus::Proxy::new(
                &connection,
                "org.freedesktop.Notifications",
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
            )
            .await?;
            let hints = HashMap::from([("transient", ashpd::zbus::zvariant::Value::from(true))]);
            proxy
                .call::<_, _, u32>(
                    "Notify",
                    &(
                        "Emendia",
                        *id,
                        "emendia",
                        "Emendia",
                        message,
                        Vec::<String>::new(),
                        hints,
                        4000_i32,
                    ),
                )
                .await
        };
        if let Ok(Ok(next)) = tokio::time::timeout(Duration::from_secs(1), result).await {
            *id = next;
        }
    });
}

pub(crate) fn close_notification() {
    runtime().spawn(async {
        let mut id = NOTIFICATION
            .get_or_init(|| tokio::sync::Mutex::new(0))
            .lock()
            .await;
        if *id == 0 {
            return;
        }
        let result = async {
            let connection = ashpd::zbus::Connection::session().await?;
            let proxy = ashpd::zbus::Proxy::new(
                &connection,
                "org.freedesktop.Notifications",
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
            )
            .await?;
            proxy.call::<_, _, ()>("CloseNotification", &(*id,)).await
        };
        let _ = tokio::time::timeout(Duration::from_secs(1), result).await;
        *id = 0;
    });
}

pub(crate) fn status() -> String {
    let state = state().lock().unwrap();
    let shortcuts = state
        .bindings
        .as_ref()
        .map(|bindings| {
            bindings
                .iter()
                .enumerate()
                .map(|(index, trigger)| {
                    format!(
                        "{}: {}",
                        crate::platform::hotkey::TranslationMode::ALL[index].label(),
                        trigger.as_deref().unwrap_or(crate::i18n::t("Unassigned"))
                    )
                })
                .collect::<Vec<_>>()
                .join(" · ")
        })
        .unwrap_or_else(|| crate::i18n::localize_message(&state.shortcuts));
    format!(
        "{}\n{}",
        shortcuts,
        if state.capture.is_empty() {
            crate::i18n::t("Wayland capture: authorization required.").to_owned()
        } else {
            crate::i18n::localize_message(&state.capture)
        }
    )
}

pub(crate) fn shortcuts_active() -> bool {
    state()
        .lock()
        .unwrap()
        .bindings
        .as_ref()
        .is_some_and(|bindings| bindings.iter().any(Option::is_some))
}

pub(crate) fn can_configure_shortcuts() -> bool {
    let shared = state().lock().unwrap();
    shared.shortcuts_version >= 2 && shared.bindings.is_some()
}

/// Dropping a cancelled worker must close its portal session, even during a dialog.
struct SessionGuard<T: ashpd::desktop::SessionPortal + Send + Sync + 'static>(Arc<Session<T>>);
fn session_path<T: ashpd::desktop::SessionPortal>(session: &Session<T>) -> Result<String> {
    // Session serializes as a D-Bus object path; ashpd keeps its path accessor private.
    serde_json::to_value(session)?
        .as_str()
        .map(str::to_owned)
        .context("Invalid portal session path")
}
impl<T: ashpd::desktop::SessionPortal + Send + Sync + 'static> Drop for SessionGuard<T> {
    fn drop(&mut self) {
        let session = self.0.clone();
        runtime().spawn(async move {
            let _ = session.close().await;
        });
    }
}

pub(crate) fn set_shortcuts(keys: Option<[HotKey; 4]>) -> Result<()> {
    initialize()?;
    let triggers = keys
        .map(|keys| {
            keys.map(shortcut_trigger)
                .into_iter()
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?;
    let mut shared = state().lock().unwrap();
    if keys.is_some()
        && shared.requested_keys == keys
        && shared
            .shortcuts_task
            .as_ref()
            .is_some_and(|task| !task.is_finished())
    {
        return Ok(());
    }
    let revision = REVISION.fetch_add(1, Ordering::AcqRel) + 1;
    if let Some(task) = shared.shortcuts_task.take() {
        task.abort();
    }
    shared.events.clear();
    shared.requested_keys = keys;
    let previous = shared.shortcuts_session.take();
    shared.bindings = None;
    shared.shortcuts_version = 0;
    CONFIGURE.store(false, Ordering::Release);
    shared.shortcuts = crate::i18n::t(if keys.is_some() {
        "Wayland shortcuts: waiting for desktop approval."
    } else {
        "Wayland shortcuts: disabled."
    })
    .into();
    if let Some(keys) = keys {
        let triggers = triggers.unwrap();
        shared.shortcuts_task = Some(runtime().spawn(async move {
            if let Some(previous) = previous {
                let _ = tokio::time::timeout(TRANSFER_TIMEOUT, previous.close()).await;
            }
            if let Err(error) = run_shortcuts(keys, triggers, revision).await
                && REVISION.load(Ordering::Acquire) == revision
            {
                state().lock().unwrap().shortcuts =
                    crate::i18n::t("Wayland shortcuts: unavailable.").into();
                state().lock().unwrap().bindings = None;
                report(error.context(crate::i18n::t("Wayland global shortcuts")));
            }
        }));
    } else if let Some(previous) = previous {
        runtime().spawn(async move {
            let _ = tokio::time::timeout(TRANSFER_TIMEOUT, previous.close()).await;
        });
    }
    CHANGED.store(true, Ordering::Release);
    Ok(())
}

pub(crate) fn validate_shortcuts(keys: [HotKey; 4]) -> Result<()> {
    keys.into_iter()
        .try_for_each(|key| shortcut_trigger(key).map(|_| ()))
}

pub(crate) fn configure_shortcuts() {
    // The session worker services this request without exposing D-Bus objects to GPUI.
    if state()
        .lock()
        .unwrap()
        .shortcuts_task
        .as_ref()
        .is_none_or(|task| task.is_finished())
    {
        report(anyhow::anyhow!(crate::i18n::t(
            "Save your shortcuts or enable them before opening desktop configuration."
        )));
    } else {
        CONFIGURE.store(true, Ordering::Release);
    }
}
static CONFIGURE: AtomicBool = AtomicBool::new(false);

fn describe_shortcuts(shortcuts: &[Shortcut]) {
    state().lock().unwrap().bindings = Some(ACTIONS.map(|id| {
        shortcuts
            .iter()
            .find(|shortcut| shortcut.id() == id)
            .map(|shortcut| shortcut.trigger_description().to_owned())
    }));
    CHANGED.store(true, Ordering::Release);
}

async fn run_shortcuts(keys: [HotKey; 4], triggers: Vec<String>, revision: u64) -> Result<()> {
    let portal = GlobalShortcuts::new().await.map_err(|error| {
        if matches!(error, ashpd::Error::PortalNotFound(_)) {
            tracing::warn!(%error, "GlobalShortcuts is not exposed by the session portal");
            anyhow::anyhow!(crate::i18n::t(
                "Your desktop does not provide the GlobalShortcuts portal. GNOME requires version 48 or newer with its matching desktop portal; Ubuntu 24.04's GNOME 46 does not support it. On other desktops, check the installed portal backend and session configuration."
            ))
        } else {
            error.into()
        }
    })?;
    state().lock().unwrap().shortcuts_version = portal.version();
    let session = Arc::new(portal.create_session(Default::default()).await?);
    let _guard = SessionGuard(session.clone());
    {
        let mut shared = state().lock().unwrap();
        if REVISION.load(Ordering::Acquire) != revision {
            return Ok(());
        }
        shared.shortcuts_session = Some(session.clone());
    }
    let mut released = Box::pin(portal.receive_deactivated().await?);
    let mut changed = Box::pin(portal.receive_shortcuts_changed().await?);
    let mut closed = Box::pin(session.receive_closed().await?);
    let shortcuts = ACTIONS
        .iter()
        .enumerate()
        .map(|(index, id)| {
            NewShortcut::new(
                *id,
                crate::platform::hotkey::TranslationMode::ALL[index].label(),
            )
            .preferred_trigger(triggers[index].as_str())
        })
        .collect::<Vec<_>>();
    let response = tokio::time::timeout(
        DIALOG_TIMEOUT,
        portal.bind_shortcuts(&session, &shortcuts, None, Default::default()),
    )
    .await??
    .response()?;
    describe_shortcuts(response.shortcuts());
    let path = session_path(&session)?;
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            event = released.next() => {
                let event = event.context("GlobalShortcuts signal stream ended")?;
                if event.session_handle().as_str() == path && REVISION.load(Ordering::Acquire) == revision
                    && let Some(index) = ACTIONS.iter().position(|id| *id == event.shortcut_id()) {
                        let mut shared = state().lock().unwrap();
                        if REVISION.load(Ordering::Acquire) == revision && shared.events.len() < 8 {
                            // Capture only on release: modifiers must not turn Ctrl+C into another command.
                            shared.events.push_back(GlobalHotKeyEvent { id: keys[index].id(), state: HotKeyState::Pressed });
                        }
                }
            }
            event = changed.next() => {
                let event = event.context("GlobalShortcuts signal stream ended")?;
                if event.session_handle().as_str() == path { describe_shortcuts(event.shortcuts()); }
            }
            _ = closed.next() => bail!(crate::i18n::t("The desktop closed the Wayland session. Authorize it again in Settings.")),
            _ = tick.tick() => {
                if CONFIGURE.swap(false, Ordering::AcqRel)
                    && let Err(error) = portal.configure_shortcuts(&session, None, Default::default()).await {
                        report(error.into());
                }
            }
        }
    }
}

fn shortcut_trigger(key: HotKey) -> Result<String> {
    let code = key.key.to_string();
    let symbol = if let Some(letter) = code.strip_prefix("Key") {
        letter.to_lowercase()
    } else if let Some(digit) = code.strip_prefix("Digit") {
        digit.to_owned()
    } else if code.starts_with('F')
        && code[1..]
            .parse::<u8>()
            .is_ok_and(|number| (1..=35).contains(&number))
    {
        code.clone()
    } else {
        match code.as_str() {
            "Space" => "space",
            "Enter" => "Return",
            "Escape" => "Escape",
            "Tab" => "Tab",
            "Backspace" => "BackSpace",
            "Delete" => "Delete",
            "Insert" => "Insert",
            "Home" => "Home",
            "End" => "End",
            "PageUp" => "Page_Up",
            "PageDown" => "Page_Down",
            "ArrowUp" => "Up",
            "ArrowDown" => "Down",
            "ArrowLeft" => "Left",
            "ArrowRight" => "Right",
            "Minus" => "minus",
            "Equal" => "equal",
            "Comma" => "comma",
            "Period" => "period",
            "Slash" => "slash",
            "Backslash" => "backslash",
            "Semicolon" => "semicolon",
            "Quote" => "apostrophe",
            "Backquote" => "grave",
            "BracketLeft" => "bracketleft",
            "BracketRight" => "bracketright",
            _ => bail!("Unsupported Wayland shortcut key: {code}"),
        }
        .to_owned()
    };
    let mut parts = Vec::new();
    for (modifier, name) in [
        (Modifiers::CONTROL, "CTRL"),
        (Modifiers::ALT, "ALT"),
        (Modifiers::SHIFT, "SHIFT"),
        (Modifiers::SUPER, "LOGO"),
    ] {
        if key.mods.contains(modifier) {
            parts.push(name.to_owned());
        }
    }
    parts.push(symbol);
    Ok(parts.join("+"))
}

#[derive(Clone, Debug, Default)]
struct Owner {
    generation: u64,
    ours: bool,
    formats: Vec<String>,
}

struct ClipboardService {
    portal: Clipboard,
    remote: RemoteDesktop,
    session: Arc<Session<RemoteDesktop>>,
    owner: watch::Receiver<Owner>,
    contents: Arc<Mutex<Formats>>,
    operation: tokio::sync::Mutex<()>,
    closed: AtomicBool,
}

pub(crate) fn authorize_capture() {
    if let Err(error) = initialize() {
        report(error);
        return;
    }
    let mut shared = state().lock().unwrap();
    if shared
        .clipboard
        .as_ref()
        .is_some_and(|service| !service.closed.load(Ordering::Acquire))
    {
        return;
    }
    if shared
        .capture_task
        .as_ref()
        .is_some_and(|task| !task.is_finished())
    {
        return;
    }
    shared.capture = crate::i18n::t("Wayland capture: waiting for desktop approval.").into();
    shared.capture_task = Some(runtime().spawn(async {
        match tokio::time::timeout(DIALOG_TIMEOUT, ClipboardService::new()).await {
            Ok(Ok(service)) => {
                let mut shared = state().lock().unwrap();
                shared.clipboard = Some(service);
                shared.capture = crate::i18n::t(
                    "Wayland capture: ready. Return to your document before using a shortcut.",
                )
                .into();
                CHANGED.store(true, Ordering::Release);
            }
            result => {
                let error = match result {
                    Ok(Err(error)) => error,
                    Err(error) => error.into(),
                    _ => unreachable!(),
                };
                state().lock().unwrap().capture =
                    crate::i18n::t("Wayland capture: unavailable.").into();
                report(error.context(crate::i18n::t("Wayland keyboard and clipboard access")));
            }
        }
    }));
    CHANGED.store(true, Ordering::Release);
}

fn clipboard() -> Result<Arc<ClipboardService>> {
    let service = state()
        .lock()
        .unwrap()
        .clipboard
        .clone()
        .context(crate::i18n::t(
            "Authorize Wayland capture in Settings → Shortcuts, then return to your document.",
        ))?;
    if service.closed.load(Ordering::Acquire) {
        bail!(crate::i18n::t(
            "The desktop closed the Wayland session. Authorize it again in Settings."
        ));
    }
    Ok(service)
}
pub(crate) fn ensure_capture_ready() -> Result<()> {
    clipboard().map(|_| ())
}
pub(crate) fn capture() -> Result<String> {
    let service = clipboard()?;
    runtime().block_on(service.capture())
}
pub(crate) fn copy_text(text: &str) -> Result<()> {
    let service = clipboard()?;
    runtime().block_on(async {
        let _operation = service.operation.lock().await;
        service
            .publish(HashMap::from([(
                "text/plain;charset=utf-8".into(),
                Arc::new(text.as_bytes().to_vec()),
            )]))
            .await?;
        Ok(())
    })
}

impl ClipboardService {
    async fn new() -> Result<Arc<Self>> {
        let remote = RemoteDesktop::new().await?;
        let portal = Clipboard::new().await?;
        let session = Arc::new(remote.create_session(Default::default()).await?);
        let guard = SessionGuard(session.clone());
        remote
            .select_devices(
                &session,
                SelectDevicesOptions::default().set_devices(Some(DeviceType::Keyboard.into())),
            )
            .await?
            .response()?;
        portal.request(&session, Default::default()).await?;
        let (sender, owner) = watch::channel(Owner::default());
        let service = Arc::new(Self {
            portal,
            remote,
            session,
            owner,
            contents: Arc::new(Mutex::new(HashMap::new())),
            operation: tokio::sync::Mutex::new(()),
            closed: AtomicBool::new(false),
        });
        let instance = service.clone();
        let path = session_path(&service.session)?;
        let (ready, initialized) = tokio::sync::oneshot::channel();
        let listener = runtime().spawn(async move {
            let _guard = guard;
            let streams = async {
                Ok::<_, anyhow::Error>((
                    instance.portal.receive_selection_owner_changed::<RemoteDesktop>().await?,
                    instance.portal.receive_selection_transfer::<RemoteDesktop>().await?,
                    instance.session.receive_closed().await?,
                ))
            }.await;
            let (owners, transfers, closed) = match streams {
                Ok(streams) => { let _ = ready.send(Ok(())); streams }
                Err(error) => { let _ = ready.send(Err(error)); return; }
            };
            let mut owners = Box::pin(owners);
            let mut transfers = Box::pin(transfers);
            let mut closed = Box::pin(closed);
            loop {
                tokio::select! {
                    change = owners.next() => {
                        let Some((session, change)) = change else { break; };
                        if session_path(&session).is_ok_and(|session| session == path) {
                            sender.send_modify(|owner| {
                                owner.generation += 1;
                                owner.ours = change.session_is_owner() == Some(true);
                                owner.formats = change.mime_types().to_vec();
                            });
                        }
                    }
                    transfer = transfers.next() => {
                        let Some((session, mime, serial)) = transfer else { break; };
                        if session_path(&session).is_ok_and(|session| session == path) {
                            let data = instance.contents.lock().unwrap().get(&mime).cloned();
                            let instance = instance.clone();
                            runtime().spawn(async move {
                                let result = async {
                                    let data = data.context("Unavailable clipboard format")?;
                                    let fd = instance.portal.selection_write(&instance.session, serial).await?;
                                    write_pipe(File::from(std::os::fd::OwnedFd::from(fd)), &data).await
                                }.await;
                                let _ = instance.portal.selection_write_done(&instance.session, serial, result.is_ok()).await;
                            });
                        }
                    }
                    _ = closed.next() => break,
                }
            }
            instance.closed.store(true, Ordering::Release);
            state().lock().unwrap().capture = crate::i18n::t("Wayland capture: unavailable.").into();
            report(anyhow::anyhow!(crate::i18n::t("The desktop closed the Wayland session. Authorize it again in Settings.")));
        });
        // Cancel this listener (and close the session) if Start fails or is timed out.
        struct AbortOnDrop(Option<JoinHandle<()>>);
        impl Drop for AbortOnDrop {
            fn drop(&mut self) {
                if let Some(task) = self.0.take() {
                    task.abort();
                }
            }
        }
        let mut listener = AbortOnDrop(Some(listener));
        initialized.await??;
        let response = service
            .remote
            .start(&service.session, None, Default::default())
            .await?
            .response()?;
        if !response.devices().contains(DeviceType::Keyboard) || !response.is_clipboard_enabled() {
            bail!(crate::i18n::t(
                "The desktop did not grant keyboard and clipboard access."
            ));
        }
        listener.0.take();
        // A successful Start must supply the initial clipboard state before capture.
        Ok(service)
    }

    async fn read(&self, mime: &str) -> Result<Vec<u8>> {
        let fd = tokio::time::timeout(
            TRANSFER_TIMEOUT,
            self.portal.selection_read(&self.session, mime),
        )
        .await??;
        read_pipe(File::from(std::os::fd::OwnedFd::from(fd))).await
    }

    async fn wait_owner(&self, generation: u64, ours: bool) -> Result<Owner> {
        let mut receiver = self.owner.clone();
        tokio::time::timeout(TRANSFER_TIMEOUT, async {
            loop {
                let owner = receiver.borrow_and_update().clone();
                if owner.generation > generation {
                    if owner.ours != ours {
                        bail!("The clipboard changed during the operation");
                    }
                    return Ok(owner);
                }
                receiver
                    .changed()
                    .await
                    .context("Clipboard session ended")?;
            }
        })
        .await?
    }

    async fn publish(&self, data: Formats) -> Result<Owner> {
        let generation = self.owner.borrow().generation;
        let formats = data.keys().map(String::as_str).collect::<Vec<_>>();
        *self.contents.lock().unwrap() = data.clone();
        if formats.is_empty() {
            // ashpd omits an empty mime_types list; send it explicitly to clear
            // the clipboard, rather than silently retaining the copied text.
            let options = HashMap::from([(
                "mime_types",
                ashpd::zbus::zvariant::Value::from(Vec::<&str>::new()),
            )]);
            tokio::time::timeout(
                TRANSFER_TIMEOUT,
                self.portal
                    .call_method("SetSelection", &(&*self.session, options)),
            )
            .await??;
            let mut receiver = self.owner.clone();
            return tokio::time::timeout(TRANSFER_TIMEOUT, async {
                loop {
                    let owner = receiver.borrow_and_update().clone();
                    if owner.generation > generation {
                        if !owner.formats.is_empty() {
                            bail!("The clipboard changed during the operation");
                        }
                        return Ok(owner);
                    }
                    receiver
                        .changed()
                        .await
                        .context("Clipboard session ended")?;
                }
            })
            .await?;
        }
        tokio::time::timeout(
            TRANSFER_TIMEOUT,
            self.portal.set_selection(
                &self.session,
                SetSelectionOptions::default().set_mime_types(&formats),
            ),
        )
        .await??;
        self.wait_owner(generation, true).await
    }

    async fn capture(&self) -> Result<String> {
        let _operation = self.operation.lock().await;
        // Deactivated can arrive before the user has lifted all modifier keys.
        tokio::time::sleep(Duration::from_millis(150)).await;
        if self.owner.borrow().generation == 0 {
            self.wait_initial_owner().await?;
        }
        let before = self.owner.borrow().clone();
        if before.formats.len() > 32 {
            bail!("Too many clipboard formats to preserve");
        }
        let mut saved = HashMap::new();
        let mut size = 0;
        for mime in &before.formats {
            let bytes = self.read(mime).await?;
            size += bytes.len();
            if size > TRANSFER_LIMIT {
                bail!("Clipboard too large to preserve (maximum 16 MiB)");
            }
            saved.insert(mime.clone(), Arc::new(bytes));
        }
        self.check_owner(before.generation)?;
        // Reserve the old formats so copying even the same text must produce a fresh owner event.
        let reservation = if saved.is_empty() {
            HashMap::from([("text/plain;charset=utf-8".into(), Arc::new(Vec::new()))])
        } else {
            saved.clone()
        };
        let reserved = self.publish(reservation).await?;
        let mut copied_generation = None;
        let result = async {
            self.check_owner(reserved.generation)?;
            self.send_copy().await?;
            let copied = self.wait_owner(reserved.generation, false).await?;
            copied_generation = Some(copied.generation);
            let mime = [
                "text/plain;charset=utf-8",
                "text/plain;charset=UTF-8",
                "text/plain",
                "UTF8_STRING",
            ]
            .into_iter()
            .find(|mime| copied.formats.iter().any(|format| format == mime))
            .context("The selection has no supported text format")?;
            let bytes = self.read(mime).await?;
            self.check_owner(copied.generation)?;
            let text = String::from_utf8(bytes).context("The selection is not UTF-8 text")?;
            validate_selection(&text)?;
            Ok::<_, anyhow::Error>((text, copied.generation))
        }
        .await;
        match result {
            Ok((text, generation)) => {
                self.check_owner(generation)?;
                self.publish(saved)
                    .await
                    .context("Unable to restore the previous clipboard")?;
                Ok(text)
            }
            Err(error) => {
                // Restore failed reads/validation too, but not a subsequent concurrent writer.
                let generation = copied_generation.unwrap_or(reserved.generation);
                if self.check_owner(generation).is_ok() {
                    let _ = self.publish(saved).await;
                }
                Err(error)
            }
        }
    }

    fn check_owner(&self, generation: u64) -> Result<()> {
        if self.closed.load(Ordering::Acquire) || self.owner.borrow().generation != generation {
            bail!("The clipboard changed during the operation");
        }
        Ok(())
    }

    async fn wait_initial_owner(&self) -> Result<()> {
        let mut receiver = self.owner.clone();
        tokio::time::timeout(TRANSFER_TIMEOUT, async {
            while receiver.borrow_and_update().generation == 0 {
                receiver.changed().await?;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await?
        .context("The desktop did not provide the initial clipboard state")
    }

    async fn send_copy(&self) -> Result<()> {
        // Keysyms honor the active layout. Always release injected keys, even on failure.
        const CONTROL_L: i32 = 0xffe3;
        let result = async {
            self.send_key(CONTROL_L, KeyState::Pressed).await?;
            self.send_key('c' as i32, KeyState::Pressed).await?;
            Ok::<_, anyhow::Error>(())
        }
        .await;
        let release_c = self.send_key('c' as i32, KeyState::Released).await;
        let release_ctrl = self.send_key(CONTROL_L, KeyState::Released).await;
        result?;
        release_c?;
        release_ctrl?;
        Ok(())
    }

    async fn send_key(&self, keysym: i32, state: KeyState) -> Result<()> {
        tokio::time::timeout(
            TRANSFER_TIMEOUT,
            self.remote
                .notify_keyboard_keysym(&self.session, keysym, state, Default::default()),
        )
        .await??;
        Ok(())
    }
}

fn validate_selection(text: &str) -> Result<()> {
    if text.trim().is_empty() {
        bail!(crate::i18n::t("The selection contains no text."));
    }
    if text.encode_utf16().count() > 100_000 {
        bail!(crate::i18n::t(
            "Selection too long (maximum 100,000 UTF-16 code units)."
        ));
    }
    Ok(())
}

fn nonblocking(file: File) -> Result<AsyncFd<File>> {
    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
    fcntl_setfl(&file, fcntl_getfl(&file)? | OFlags::NONBLOCK)?;
    Ok(AsyncFd::new(file)?)
}

async fn read_pipe(file: File) -> Result<Vec<u8>> {
    let file = nonblocking(file)?;
    tokio::time::timeout(TRANSFER_TIMEOUT, async {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let mut ready = file.readable().await?;
            match ready.try_io(|fd| (&mut fd.get_ref()).read(&mut buffer)) {
                Ok(Ok(0)) => return Ok(bytes),
                Ok(Ok(size)) => {
                    if bytes.len() + size > TRANSFER_LIMIT {
                        bail!("Clipboard transfer exceeds 16 MiB");
                    }
                    bytes.extend_from_slice(&buffer[..size]);
                }
                Ok(Err(error)) => return Err(error.into()),
                Err(_) => continue,
            }
        }
    })
    .await?
}

async fn write_pipe(file: File, bytes: &[u8]) -> Result<()> {
    let file = nonblocking(file)?;
    tokio::time::timeout(TRANSFER_TIMEOUT, async {
        let mut remaining = bytes;
        while !remaining.is_empty() {
            let mut ready = file.writable().await?;
            match ready.try_io(|fd| (&mut fd.get_ref()).write(remaining)) {
                Ok(Ok(0)) => bail!("Clipboard transfer closed"),
                Ok(Ok(size)) => remaining = &remaining[size..],
                Ok(Err(error)) => return Err(error.into()),
                Err(_) => continue,
            }
        }
        Ok(())
    })
    .await?
}

#[cfg(test)]
#[path = "portal_tests.rs"]
mod portal_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_triggers_use_xkb_names() {
        for (input, expected) in [
            ("Ctrl+Shift+F12", "CTRL+SHIFT+F12"),
            ("Super+Alt+KeyT", "ALT+LOGO+t"),
            ("Ctrl+Enter", "CTRL+Return"),
            ("Ctrl+Digit1", "CTRL+1"),
            ("Alt+ArrowLeft", "ALT+Left"),
        ] {
            assert_eq!(shortcut_trigger(input.parse().unwrap()).unwrap(), expected);
        }
    }

    #[test]
    fn capture_limit_counts_utf16_not_bytes_or_characters() {
        assert!(validate_selection(" \n").is_err());
        assert!(validate_selection(&"é".repeat(100_000)).is_ok());
        assert!(validate_selection(&"😀".repeat(50_000)).is_ok());
        assert!(validate_selection(&"😀".repeat(50_001)).is_err());
    }

    #[tokio::test]
    async fn bounded_transfer_handles_large_binary_data_without_blocking() {
        use std::os::unix::net::UnixStream;
        let (read, write) = UnixStream::pair().unwrap();
        let bytes = (0..512 * 1024)
            .map(|index| (index % 256) as u8)
            .collect::<Vec<_>>();
        let (received, sent) = tokio::join!(
            read_pipe(File::from(std::os::fd::OwnedFd::from(read))),
            write_pipe(File::from(std::os::fd::OwnedFd::from(write)), &bytes)
        );
        sent.unwrap();
        assert_eq!(received.unwrap(), bytes);
    }

    #[tokio::test]
    async fn stalled_transfer_times_out_and_closes_the_descriptor() {
        use std::os::unix::net::UnixStream;
        let (read, mut write) = UnixStream::pair().unwrap();
        assert!(
            read_pipe(File::from(std::os::fd::OwnedFd::from(read)))
                .await
                .is_err()
        );
        assert!(write.write_all(b"too late").is_err());
    }
}
