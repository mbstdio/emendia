//! X11 selection capture and replacement. Blocking work runs on Tokio workers.
use super::clipboard::{self, Reader, atom};
use crate::text::{MAX_TEXT_UNITS, validate_clipboard_text};
use anyhow::{Context, Result, bail};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicU32, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use x11rb::{
    CURRENT_TIME, NONE,
    connection::{Connection, RequestConnection},
    protocol::{
        randr::ConnectionExt as _,
        res::{ClientIdMask, ClientIdSpec, ConnectionExt as _},
        xproto::{
            self, AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConfigureWindowAux,
            ConnectionExt as _, EventMask, MapState, PropMode, StackMode, Window,
        },
        xtest::ConnectionExt as _,
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

#[cfg(test)]
#[path = "desktop_tests.rs"]
mod desktop_tests;

static OPERATION: Mutex<()> = Mutex::new(());
static SCALE: AtomicU32 = AtomicU32::new(1_f32.to_bits());

pub(crate) fn set_window_icon(window: isize) -> Result<()> {
    let desktop = Desktop::new()?;
    let property = atom(&desktop.connection, "_NET_WM_ICON")?;
    let values = crate::platform::icons::window_icon_argb()?;
    // Multi-size icons can exceed a core X11 request. Append chunks instead of
    // requiring the server to support large requests for a taskbar icon.
    let chunk_size = (desktop
        .connection
        .maximum_request_bytes()
        .saturating_sub(32)
        / 4)
    .max(1);
    for (index, chunk) in values.chunks(chunk_size).enumerate() {
        let mode = if index == 0 {
            PropMode::REPLACE
        } else {
            PropMode::APPEND
        };
        desktop
            .connection
            .change_property32(mode, window as Window, property, AtomEnum::CARDINAL, chunk)?
            .check()?;
    }
    Ok(())
}

pub(crate) fn smoke_window_has_icon(window: isize) -> bool {
    (|| -> Result<bool> {
        let desktop = Desktop::new()?;
        let property = atom(&desktop.connection, "_NET_WM_ICON")?;
        let reply = desktop
            .connection
            .get_property(
                false,
                window as Window,
                property,
                AtomEnum::CARDINAL,
                0,
                u32::MAX,
            )?
            .reply()?;
        let values: Vec<u32> = reply
            .value32()
            .map(|values| values.collect())
            .unwrap_or_default();
        Ok(reply.format == 32
            && reply.bytes_after == 0
            && values.as_slice() == crate::platform::icons::window_icon_argb()?)
    })()
    .unwrap_or(false)
}

#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub monitor: u64,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub used_selection: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Destination {
    window: Window,
    process: Option<u32>,
    title: Vec<u8>,
    focus: Window,
}

#[derive(Clone, Debug)]
pub struct CaptureTarget {
    destination: Destination,
    cursor: (i32, i32),
}

#[derive(Clone, Debug)]
pub struct Selection {
    pub text: String,
    destination: Option<Destination>,
    pub placement: Placement,
}

impl Selection {
    pub(crate) fn smoke_fixture() -> Result<Self> {
        let desktop = Desktop::new()?;
        Ok(Self {
            text: "Bonjour, ceci est un test de traduction.".into(),
            destination: None,
            placement: desktop.placement(desktop.cursor()?, false)?,
        })
    }
}

pub(crate) fn configure_display(cx: &gpui_kit::App) -> Result<()> {
    let desktop = Desktop::new()?;
    // GPUI 0.7 identifies X11 displays by X screen, not by RandR output. Use its
    // actual logical bounds to derive the same scale, then RandR for work areas.
    if let Some(display) = cx.find_display(gpui_kit::DisplayId::new(desktop.screen as u64)) {
        let width = display.bounds().size.width.as_f32();
        if width > 0. {
            SCALE.store(
                (desktop.connection.setup().roots[desktop.screen].width_in_pixels as f32 / width)
                    .to_bits(),
                Ordering::Relaxed,
            );
        }
    }
    Ok(())
}

fn scale() -> f32 {
    f32::from_bits(SCALE.load(Ordering::Relaxed))
}

pub(crate) fn smoke_send_shortcut() -> Result<()> {
    Desktop::new()?.send_ctrl(b'm' as u32)
}

pub(crate) fn smoke_preview_is_movable(window: isize) -> bool {
    (|| -> Result<bool> {
        let desktop = Desktop::new()?;
        let window = window as Window;
        let attributes = desktop.connection.get_window_attributes(window)?.reply()?;
        let notification = atom(&desktop.connection, "_NET_WM_WINDOW_TYPE_NOTIFICATION")?;
        let movable = atom(&desktop.connection, "_NET_WM_ACTION_MOVE")?;
        let types = desktop.values(window, "_NET_WM_WINDOW_TYPE")?;
        let allowed = desktop.values(window, "_NET_WM_ALLOWED_ACTIONS")?;
        Ok(!attributes.override_redirect
            && !types.contains(&notification)
            && (allowed.is_empty() || allowed.contains(&movable)))
    })()
    .unwrap_or(false)
}

struct Desktop {
    connection: RustConnection,
    screen: usize,
    root: Window,
}

impl Desktop {
    fn new() -> Result<Self> {
        let (connection, screen) = x11rb::connect(None)?;
        let root = connection.setup().roots[screen].root;
        Ok(Self {
            connection,
            screen,
            root,
        })
    }

    fn values(&self, window: Window, name: &str) -> Result<Vec<u32>> {
        let reply = self
            .connection
            .get_property(
                false,
                window,
                atom(&self.connection, name)?,
                AtomEnum::ANY,
                0,
                4096,
            )?
            .reply()?;
        Ok(reply
            .value32()
            .map(|values| values.collect())
            .unwrap_or_default())
    }

    fn active(&self) -> Result<Window> {
        let window = self
            .values(self.root, "_NET_ACTIVE_WINDOW")?
            .first()
            .copied()
            .unwrap_or(NONE);
        if window == NONE {
            bail!(crate::i18n::t("No active window."));
        }
        Ok(window)
    }

    fn cursor(&self) -> Result<(i32, i32)> {
        let pointer = self.connection.query_pointer(self.root)?.reply()?;
        Ok((pointer.root_x as i32, pointer.root_y as i32))
    }

    fn destination(&self, window: Window) -> Result<Destination> {
        let process = self.values(window, "_NET_WM_PID")?.first().copied();
        let mut title = self
            .connection
            .get_property(
                false,
                window,
                atom(&self.connection, "_NET_WM_NAME")?,
                AtomEnum::ANY,
                0,
                4096,
            )?
            .reply()?
            .value;
        if title.is_empty() {
            title = self
                .connection
                .get_property(false, window, AtomEnum::WM_NAME, AtomEnum::ANY, 0, 4096)?
                .reply()?
                .value;
        }
        Ok(Destination {
            window,
            process,
            title,
            focus: self.connection.get_input_focus()?.reply()?.focus,
        })
    }

    fn check(&self, expected: &Destination) -> Result<()> {
        if self.active()? != expected.window {
            bail!(crate::i18n::t(
                "The active window changed; operation cancelled."
            ));
        }
        if self.destination(expected.window)? != *expected {
            bail!(crate::i18n::t(
                "The original document or control changed. Use Copy."
            ));
        }
        Ok(())
    }

    fn client_process(&self, window: Window) -> Result<u32> {
        // Ask the server, not the client-supplied _NET_WM_PID property. Clipboard
        // owners are often hidden windows without any window-manager properties.
        let reply = self
            .connection
            .res_query_client_ids(&[ClientIdSpec {
                client: window,
                mask: ClientIdMask::LOCAL_CLIENT_PID,
            }])?
            .reply()?;
        reply
            .ids
            .iter()
            .find(|id| id.spec.mask == ClientIdMask::LOCAL_CLIENT_PID)
            .and_then(|id| id.value.first().copied())
            .filter(|process| *process != 0)
            .context(crate::i18n::t(
                "Unable to verify the clipboard source; capture cancelled.",
            ))
    }

    fn wait_modifiers(&self) -> Result<()> {
        let mapping = self.connection.get_modifier_mapping()?.reply()?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let pressed = self.connection.query_keymap()?.reply()?.keys;
            // Ignore Caps Lock/Num Lock, but wait for Shift, Ctrl, Alt and Super.
            let down = mapping
                .keycodes
                .chunks(mapping.keycodes.len() / 8)
                .enumerate()
                .filter(|(index, _)| [0, 2, 3, 6].contains(index))
                .flat_map(|(_, keys)| keys)
                .any(|&key| key != 0 && pressed[key as usize / 8] & (1 << (key % 8)) != 0);
            if !down {
                return Ok(());
            }
            if Instant::now() >= deadline {
                bail!(crate::i18n::t("Release the shortcut keys, then try again."));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn send_ctrl(&self, symbol: u32) -> Result<()> {
        let setup = self.connection.setup();
        let mapping = self
            .connection
            .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)?
            .reply()?;
        let key = |symbol| -> Result<u8> {
            // Only accept an unshifted symbol: shifted/AltGr layouts would inject
            // a different command. Keyboard mappings are queried per operation.
            let offset = mapping
                .keysyms
                .chunks(mapping.keysyms_per_keycode as usize)
                .position(|values| values.first() == Some(&symbol))
                .context("Required key is unavailable in this keyboard layout")?;
            Ok(setup.min_keycode + offset as u8)
        };
        let control = key(0xffe3)?; // Control_L
        let character = key(symbol)?;
        self.connection
            .xtest_get_version(2, 2)?
            .reply()
            .context("XTest is required for selection capture and replacement")?;
        let result = (|| -> Result<()> {
            for (type_, detail) in [
                (xproto::KEY_PRESS_EVENT, control),
                (xproto::KEY_PRESS_EVENT, character),
                (xproto::KEY_RELEASE_EVENT, character),
                (xproto::KEY_RELEASE_EVENT, control),
            ] {
                self.connection
                    .xtest_fake_input(type_, detail, CURRENT_TIME, self.root, 0, 0, 0)?
                    .check()?;
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = self
                .connection
                .xtest_fake_input(
                    xproto::KEY_RELEASE_EVENT,
                    character,
                    CURRENT_TIME,
                    self.root,
                    0,
                    0,
                    0,
                )?
                .check();
            let _ = self
                .connection
                .xtest_fake_input(
                    xproto::KEY_RELEASE_EVENT,
                    control,
                    CURRENT_TIME,
                    self.root,
                    0,
                    0,
                    0,
                )?
                .check();
        }
        result
    }

    fn activate(&self, window: Window) -> Result<()> {
        self.connection
            .get_window_attributes(window)?
            .reply()
            .context(crate::i18n::t("The original window was closed. Use Copy."))?;
        let event = ClientMessageEvent::new(
            32,
            window,
            atom(&self.connection, "_NET_ACTIVE_WINDOW")?,
            [2, CURRENT_TIME, 0, 0, 0],
        );
        self.connection
            .send_event(
                false,
                self.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )?
            .check()?;
        let deadline = Instant::now() + Duration::from_millis(600);
        while self.active().ok() != Some(window) {
            if Instant::now() >= deadline {
                bail!(crate::i18n::t(
                    "Unable to focus the original window. Use Copy."
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    fn area(&self, point: (i32, i32)) -> Result<(f32, f32, f32, f32)> {
        let screen = &self.connection.setup().roots[self.screen];
        let mut area = (
            0.,
            0.,
            screen.width_in_pixels as f32,
            screen.height_in_pixels as f32,
        );
        if let Ok(reply) = self.connection.randr_get_monitors(self.root, true)?.reply()
            && let Some(monitor) = reply.monitors.iter().find(|monitor| {
                point.0 >= monitor.x as i32
                    && point.1 >= monitor.y as i32
                    && point.0 < monitor.x as i32 + monitor.width as i32
                    && point.1 < monitor.y as i32 + monitor.height as i32
            })
        {
            area = (
                monitor.x as f32,
                monitor.y as f32,
                monitor.x as f32 + monitor.width as f32,
                monitor.y as f32 + monitor.height as f32,
            );
        }
        let desktop = self
            .values(self.root, "_NET_CURRENT_DESKTOP")?
            .first()
            .copied()
            .unwrap_or(0) as usize;
        let work = self.values(self.root, "_NET_WORKAREA")?;
        if let Some(rect) = work.get(desktop * 4..desktop * 4 + 4) {
            let clipped = (
                area.0.max(rect[0] as f32),
                area.1.max(rect[1] as f32),
                area.2.min((rect[0] + rect[2]) as f32),
                area.3.min((rect[1] + rect[3]) as f32),
            );
            if clipped.2 > clipped.0 && clipped.3 > clipped.1 {
                area = clipped;
            }
        }
        let scale = scale();
        Ok((
            area.0 / scale,
            area.1 / scale,
            area.2 / scale,
            area.3 / scale,
        ))
    }

    fn placement(&self, point: (i32, i32), status: bool) -> Result<Placement> {
        let (left, top, right, bottom) = self.area(point)?;
        let width = (if status { 340_f32 } else { 520_f32 }).min(right - left);
        let height = (if status { 160_f32 } else { 420_f32 }).min(bottom - top);
        let (x, y) = if status {
            (right - width - 16., bottom - height - 16.)
        } else {
            (point.0 as f32 / scale(), point.1 as f32 / scale() + 10.)
        };
        Ok(Placement {
            monitor: self.screen as u64,
            x: x.clamp(left, right - width),
            y: y.clamp(top, bottom - height),
            width,
            height,
            used_selection: false,
        })
    }
}

pub fn capture_target() -> Result<CaptureTarget> {
    let desktop = Desktop::new()?;
    let destination = desktop.destination(desktop.active()?)?;
    if destination.process == Some(std::process::id()) {
        bail!(crate::i18n::t("Select text in another application."));
    }
    Ok(CaptureTarget {
        destination,
        cursor: desktop.cursor()?,
    })
}

pub fn status_placement(target: &CaptureTarget) -> Result<Placement> {
    Desktop::new()?.placement(target.cursor, true)
}

pub(crate) fn foreground_window() -> isize {
    Desktop::new()
        .and_then(|desktop| desktop.active())
        .unwrap_or(NONE) as isize
}
pub(crate) fn cursor_monitor() -> Result<u64> {
    Ok(Desktop::new()?.screen as u64)
}

pub(crate) fn center_window(window: isize, width: f32, height: f32) -> Result<()> {
    let desktop = Desktop::new()?;
    let (left, top, right, bottom) = desktop.area(desktop.cursor()?)?;
    let width = width.min(right - left);
    let height = height.min(bottom - top);
    let x = ((left + (right - left - width) / 2.) * scale()).round() as i32;
    let y = ((top + (bottom - top - height) / 2.) * scale()).round() as i32;
    let width = (width * scale()).round() as u32;
    let height = (height * scale()).round() as u32;
    let moveresize = atom(&desktop.connection, "_NET_MOVERESIZE_WINDOW")?;
    if desktop
        .values(desktop.root, "_NET_SUPPORTED")?
        .contains(&moveresize)
    {
        // Let the window manager position a managed client (source=application,
        // all four bounds supplied), including its decorations.
        let flags = (1 << 12) | (0xf << 8);
        let event = ClientMessageEvent::new(
            32,
            window as Window,
            moveresize,
            [flags, x as u32, y as u32, width, height],
        );
        desktop
            .connection
            .send_event(
                false,
                desktop.root,
                EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
                event,
            )?
            .check()?;
    } else {
        desktop
            .connection
            .configure_window(
                window as Window,
                &ConfigureWindowAux::new()
                    .x(x)
                    .y(y)
                    .width(width)
                    .height(height),
            )?
            .check()?;
    }
    Ok(())
}

pub(crate) fn status_placement_for_window(_window: isize) -> Result<Placement> {
    let desktop = Desktop::new()?;
    desktop.placement(desktop.cursor()?, true)
}

fn copy_selection(desktop: &Desktop, destination: &Destination) -> Result<String> {
    // Fail before touching the clipboard when server-side attribution is unavailable.
    let attribution_error =
        crate::i18n::t("X-Resource 1.2 is required to verify the clipboard source.");
    let version = desktop
        .connection
        .res_query_version(1, 2)
        .context(attribution_error)?
        .reply()
        .context(attribution_error)?;
    if (version.server_major, version.server_minor) < (1, 2) {
        bail!(attribution_error);
    }
    let expected_process = desktop.client_process(destination.window)?;
    let mut reader = Reader::new()?;
    let (before, snapshot) = reader.snapshot()?;
    desktop.check(destination)?;
    // Preserve all transferable formats and take ownership first. A subsequent
    // Ctrl+C must publish a fresh selection, even when the editor was the old owner.
    let reserved = clipboard::put(snapshot.clone(), Some(before))?;
    let mut copied_stamp = None;
    let result = (|| -> Result<(String, clipboard::Stamp)> {
        desktop.check(destination)?;
        desktop.send_ctrl(b'c' as u32)?;
        let copied = reader.wait_for_copy(reserved, || desktop.check(destination))?;
        if desktop.client_process(copied.owner)? != expected_process {
            bail!(crate::i18n::t(
                "The clipboard was modified by another application."
            ));
        }
        if reader.stamp()? != copied {
            bail!("The clipboard changed while verifying the selection source");
        }
        // Retain the verified generation even if decoding or transfer fails.
        copied_stamp = Some(copied);
        let text = reader.text()?;
        desktop.check(destination)?;
        if reader.stamp()? != copied {
            bail!("The clipboard changed while reading the selection");
        }
        Ok((text, copied))
    })();
    match result {
        Ok((text, copied)) => {
            clipboard::put(snapshot, Some(copied))
                .context(crate::i18n::t("Unable to restore the previous clipboard"))?;
            Ok(text)
        }
        Err(error) => {
            // Restore our reservation or the verified copy, never a third-party
            // publication. The owner thread checks the generation again atomically.
            let expected = copied_stamp.unwrap_or(reserved);
            if reader.stamp().is_ok_and(|current| current != expected) {
                return Err(error);
            }
            if let Err(rollback) = clipboard::put(snapshot, Some(expected)) {
                let message = format!(
                    "{error:#}. {}: {rollback:#}",
                    crate::i18n::t("Unable to restore the previous clipboard")
                );
                return Err(error.context(message));
            }
            Err(error)
        }
    }
}

pub fn capture(target: CaptureTarget) -> Result<Selection> {
    let _operation = OPERATION
        .lock()
        .map_err(|_| anyhow::anyhow!("Clipboard service interrupted"))?;
    let desktop = Desktop::new()?;
    desktop.wait_modifiers()?;
    let text = copy_selection(&desktop, &target.destination)?;
    if text.trim().is_empty() {
        bail!(crate::i18n::t("The selection contains no text."));
    }
    if text.encode_utf16().count() > MAX_TEXT_UNITS {
        bail!(crate::i18n::t(
            "Selection too long (maximum 100,000 UTF-16 code units)."
        ));
    }
    Ok(Selection {
        text,
        destination: Some(target.destination),
        placement: desktop.placement(target.cursor, false)?,
    })
}

pub fn replace(selection: &Selection, text: &str) -> Result<()> {
    replace_inner(selection, text, false)
}
pub fn replace_quick(selection: &Selection, text: &str) -> Result<()> {
    replace_inner(selection, text, true)
}

fn replace_inner(selection: &Selection, text: &str, quick: bool) -> Result<()> {
    validate_clipboard_text(text)?;
    let _operation = OPERATION
        .lock()
        .map_err(|_| anyhow::anyhow!("Clipboard service interrupted"))?;
    let destination = selection
        .destination
        .as_ref()
        .context("No replacement destination. Use Copy.")?;
    if text.trim().is_empty() {
        bail!(crate::i18n::t("The result is empty."));
    }
    let desktop = Desktop::new()?;
    desktop.wait_modifiers()?;
    if !quick {
        desktop.activate(destination.window)?;
    }
    desktop.check(destination)?;
    let current = copy_selection(&desktop, destination)?;
    let normalize = |value: &str| value.replace("\r\n", "\n").replace('\r', "\n");
    if normalize(&current) != normalize(&selection.text) {
        bail!(crate::i18n::t(
            "The selection changed. No replacement performed; use Copy."
        ));
    }
    desktop.check(destination)?;
    let published = clipboard::put_text(text)?;
    let mut reader = Reader::new()?;
    desktop.check(destination)?;
    if reader.stamp()? != published {
        bail!(crate::i18n::t(
            "The clipboard changed before pasting. Use Copy."
        ));
    }
    desktop.send_ctrl(b'v' as u32)?;
    // Keep the result available; editors can request it after the keys are queued.
    Ok(())
}

pub fn copy_text(text: &str) -> Result<()> {
    let _operation = OPERATION
        .lock()
        .map_err(|_| anyhow::anyhow!("Clipboard service interrupted"))?;
    clipboard::put_text(text).map(|_| ())
}

pub fn show_status_without_activation(
    window: isize,
    placement: Placement,
    scale: f32,
) -> Result<()> {
    let desktop = Desktop::new()?;
    let window = window as Window;
    desktop
        .connection
        .change_window_attributes(
            window,
            &ChangeWindowAttributesAux::new().override_redirect(1),
        )?
        .check()?;
    // ICCCM InputHint=false and no WM_TAKE_FOCUS: clicking cannot activate it.
    desktop
        .connection
        .change_property32(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_HINTS,
            AtomEnum::WM_HINTS,
            &[1, 0, 0, 0, 0, 0, 0, 0, 0],
        )?
        .check()?;
    let protocols = atom(&desktop.connection, "WM_PROTOCOLS")?;
    desktop
        .connection
        .change_property32(PropMode::REPLACE, window, protocols, AtomEnum::ATOM, &[])?
        .check()?;
    desktop
        .connection
        .configure_window(
            window,
            &ConfigureWindowAux::new()
                .x((placement.x * scale).round() as i32)
                .y((placement.y * scale).round() as i32)
                .width((placement.width * scale).round() as u32)
                .height((placement.height * scale).round() as u32)
                .stack_mode(StackMode::ABOVE),
        )?
        .check()?;
    desktop.connection.map_window(window)?.check()?;
    Ok(())
}

pub(crate) fn status_is_nonactivating(window: isize) -> bool {
    (|| -> Result<bool> {
        let desktop = Desktop::new()?;
        let attributes = desktop
            .connection
            .get_window_attributes(window as Window)?
            .reply()?;
        Ok(attributes.override_redirect
            && attributes.map_state == MapState::VIEWABLE
            && desktop.active().unwrap_or(NONE) != window as Window)
    })()
    .unwrap_or(false)
}

pub(crate) fn status_has_expected_bounds(window: isize, placement: Placement, scale: f32) -> bool {
    (|| -> Result<bool> {
        let desktop = Desktop::new()?;
        let bounds = desktop.connection.get_geometry(window as Window)?.reply()?;
        Ok(bounds.x as i32 == (placement.x * scale).round() as i32
            && bounds.y as i32 == (placement.y * scale).round() as i32
            && bounds.width as u32 == (placement.width * scale).round() as u32
            && bounds.height as u32 == (placement.height * scale).round() as u32)
    })()
    .unwrap_or(false)
}
