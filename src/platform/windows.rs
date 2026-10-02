//! Windows-specific selection capture. All COM calls and clipboard transactions run on a
//! dedicated worker; no sleep or cross-process accessibility call blocks GPUI's UI thread.
use anyhow::{Context, Result, bail};
use std::{
    mem::size_of,
    sync::{Mutex, OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, POINT, WPARAM},
        Graphics::Gdi::{
            DeleteEnhMetaFile, DeleteMetaFile, DeleteObject, GetMonitorInfoW, HENHMETAFILE,
            HGDIOBJ, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
            MonitorFromWindow,
        },
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize,
            },
            DataExchange::{
                CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
                GetClipboardOwner, GetClipboardSequenceNumber, METAFILEPICT, OpenClipboard,
                SetClipboardData,
            },
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
            Ole::{
                CLIPBOARD_FORMAT, OleDuplicateData, SafeArrayDestroy, SafeArrayGetElement,
                SafeArrayGetLBound, SafeArrayGetUBound,
            },
            Threading::GetCurrentProcessId,
        },
        UI::{
            Accessibility::{
                CUIAutomation8, IUIAutomation, IUIAutomation2, IUIAutomationTextPattern,
                TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start, UIA_TextPatternId,
            },
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
                SendInput, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
            },
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, DispatchMessageW, GUITHREADINFO, GWL_EXSTYLE,
                GetCursorPos, GetForegroundWindow, GetGUIThreadInfo, GetMessageW,
                GetWindowLongPtrW, GetWindowTextW, GetWindowThreadProcessId, HWND_MESSAGE,
                HWND_TOPMOST, IsWindow, MA_NOACTIVATE, MSG, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED,
                SWP_NOACTIVATE, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
                TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_MOUSEACTIVATE, WM_NCDESTROY,
                WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
            },
        },
    },
    core::{Interface, w},
};

const UNICODE_TEXT: u32 = 13;
const MAX_TEXT_UNITS: usize = 100_000;

pub(crate) fn foreground_window() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
}

pub(crate) fn status_is_nonactivating(window: isize) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{IsWindowVisible, SendMessageW};
    unsafe {
        let handle = hwnd(window);
        let style = GetWindowLongPtrW(handle, GWL_EXSTYLE);
        let expected = (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW).0 as isize;
        style & expected == expected
            && IsWindowVisible(handle).as_bool()
            && SendMessageW(handle, WM_MOUSEACTIVATE, WPARAM(0), LPARAM(0)).0
                == MA_NOACTIVATE as isize
    }
}

#[cfg(test)]
mod desktop_tests;
// Serializes complete clipboard operations, including an explicit Copy after closing
// an old preview and a capture started from the next hotkey.
static CLIPBOARD_OPERATION: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
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

#[derive(Clone, Debug)]
pub struct Selection {
    pub text: String,
    window: isize,
    process: u32,
    destination: Destination,
    accessibility: Option<AccessibilityIdentity>,
    pub placement: Placement,
}

impl Selection {
    /// A diagnostic preview has no replacement destination and never reads a document.
    pub(crate) fn smoke_fixture() -> Result<Self> {
        let mut cursor = POINT::default();
        unsafe {
            GetCursorPos(&mut cursor)?;
        }
        Ok(Self {
            text: "Bonjour, ceci est un test de traduction.".into(),
            window: 0,
            process: 0,
            destination: Destination {
                title: String::new(),
                focused_control: 0,
            },
            accessibility: None,
            placement: placement(None, cursor)?,
        })
    }
}

// Called on the main thread at the exact shortcut event, before anything changes focus.
#[derive(Clone)]
pub struct CaptureTarget {
    window: isize,
    process: u32,
    cursor: POINT,
    destination: Destination,
}

#[derive(Clone, Debug, PartialEq)]
struct Destination {
    title: String,
    focused_control: isize,
}

fn destination(window: isize) -> Result<Destination> {
    unsafe {
        let mut title = [0u16; 1024];
        let length = GetWindowTextW(hwnd(window), &mut title) as usize;
        let thread_id = GetWindowThreadProcessId(hwnd(window), None);
        let mut info = GUITHREADINFO {
            cbSize: size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        GetGUIThreadInfo(thread_id, &mut info)?;
        Ok(Destination {
            title: String::from_utf16_lossy(&title[..length]),
            focused_control: info.hwndFocus.0 as isize,
        })
    }
}

pub fn capture_target() -> Result<CaptureTarget> {
    unsafe {
        let window = GetForegroundWindow();
        if window.0.is_null() {
            bail!("Aucune fenêtre active.");
        }
        let mut process = 0;
        GetWindowThreadProcessId(window, Some(&mut process));
        if process == GetCurrentProcessId() {
            bail!("Sélectionne du texte dans une autre application.");
        }
        let mut cursor = POINT::default();
        GetCursorPos(&mut cursor)?;
        Ok(CaptureTarget {
            window: window.0 as isize,
            process,
            cursor,
            destination: destination(window.0 as isize)?,
        })
    }
}

pub fn status_placement(target: &CaptureTarget) -> Result<Placement> {
    status_placement_for_window(target.window)
}

pub(crate) fn status_placement_for_window(window: isize) -> Result<Placement> {
    unsafe {
        let monitor = if window != 0 {
            MonitorFromWindow(hwnd(window), MONITOR_DEFAULTTONEAREST)
        } else {
            let mut cursor = POINT::default();
            GetCursorPos(&mut cursor)?;
            MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST)
        };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(monitor, &mut info).ok()?;
        let (mut dpi_x, mut dpi_y) = (96, 96);
        GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y)?;
        let scale = dpi_x as f32 / 96.;
        let width = 340_f32.min((info.rcWork.right - info.rcWork.left) as f32 / scale);
        let height = 160_f32.min((info.rcWork.bottom - info.rcWork.top) as f32 / scale);
        Ok(Placement {
            monitor: monitor.0 as u64,
            x: (info.rcWork.right as f32 / scale - width - 16.)
                .max(info.rcWork.left as f32 / scale),
            y: (info.rcWork.bottom as f32 / scale - height - 16.)
                .max(info.rcWork.top as f32 / scale),
            width,
            height,
            used_selection: false,
        })
    }
}

/// Called while the GPUI window is still hidden, on its owning UI thread.
pub fn show_status_without_activation(window: isize, placement: Placement) -> Result<()> {
    let (x, y, width, height) = status_physical_bounds(placement)?;
    unsafe {
        let handle = hwnd(window);
        if !IsWindow(handle).as_bool() {
            bail!("Fenêtre d’état invalide.");
        }
        // GPUI handles WM_MOUSEACTIVATE with MA_ACTIVATE even for WS_EX_NOACTIVATE.
        // Intercept it before GPUI so clicking the status cannot activate the popup.
        SetWindowSubclass(handle, Some(status_subclass), 1, 0).ok()?;
        let style = GetWindowLongPtrW(handle, GWL_EXSTYLE);
        SetWindowLongPtrW(
            handle,
            GWL_EXSTYLE,
            style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW).0 as isize,
        );
        SetWindowPos(
            handle,
            HWND_TOPMOST,
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_FRAMECHANGED,
        )?;
        let _ = ShowWindow(handle, SW_SHOWNOACTIVATE);
        Ok(())
    }
}

fn status_physical_bounds(placement: Placement) -> Result<(i32, i32, i32, i32)> {
    let (mut dpi_x, mut dpi_y) = (96, 96);
    unsafe {
        GetDpiForMonitor(
            HMONITOR(placement.monitor as *mut _),
            MDT_EFFECTIVE_DPI,
            &mut dpi_x,
            &mut dpi_y,
        )?;
    }
    let scale = dpi_x as f32 / 96.;
    Ok((
        (placement.x * scale).round() as i32,
        (placement.y * scale).round() as i32,
        (placement.width * scale).round() as i32,
        (placement.height * scale).round() as i32,
    ))
}

pub(crate) fn status_has_expected_bounds(window: isize, placement: Placement) -> bool {
    use windows::Win32::{Foundation::RECT, UI::WindowsAndMessaging::GetWindowRect};
    let Ok((x, y, width, height)) = status_physical_bounds(placement) else {
        return false;
    };
    let mut rect = RECT::default();
    (unsafe { GetWindowRect(hwnd(window), &mut rect).is_ok() })
        && (rect.left - x).abs() <= 1
        && (rect.top - y).abs() <= 1
        && (rect.right - rect.left - width).abs() <= 1
        && (rect.bottom - rect.top - height).abs() <= 1
}

unsafe extern "system" fn status_subclass(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    if message == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    unsafe {
        if message == WM_NCDESTROY {
            let _ = RemoveWindowSubclass(window, Some(status_subclass), id);
        }
        DefSubclassProc(window, message, wparam, lparam)
    }
}

pub fn capture(target: CaptureTarget) -> Result<Selection> {
    let _operation = CLIPBOARD_OPERATION
        .lock()
        .map_err(|_| anyhow::anyhow!("Service presse-papiers interrompu"))?;
    let _com = ComApartment::new()?;
    wait_for_modifiers()?;
    check_foreground(target.window, target.process)?;
    if destination(target.window)? != target.destination {
        bail!("Le document ou le contrôle actif a changé ; capture annulée.");
    }
    let accessible = accessible_selection().ok();
    let anchor = accessible.as_ref().and_then(|a| a.rect);
    check_foreground(target.window, target.process)?;
    let text = copy_selection(
        target.window,
        target.process,
        accessible.as_ref().map(|a| a.text.as_str()),
    )?;
    if destination(target.window)? != target.destination {
        bail!("Le document ou le contrôle actif a changé ; capture annulée.");
    }
    if text.trim().is_empty() {
        bail!("La sélection ne contient pas de texte.");
    }
    if text.encode_utf16().count() > MAX_TEXT_UNITS {
        bail!("Sélection trop longue (100 000 caractères UTF-16 maximum).");
    }
    Ok(Selection {
        text,
        window: target.window,
        process: target.process,
        destination: target.destination,
        accessibility: accessible.map(|a| a.identity),
        placement: placement(anchor, target.cursor)?,
    })
}

pub fn replace(selection: &Selection, text: &str) -> Result<()> {
    replace_inner(selection, text, false)
}

pub fn replace_quick(selection: &Selection, text: &str) -> Result<()> {
    replace_inner(selection, text, true)
}

fn replace_inner(selection: &Selection, text: &str, quick: bool) -> Result<()> {
    let _operation = CLIPBOARD_OPERATION
        .lock()
        .map_err(|_| anyhow::anyhow!("Service presse-papiers interrompu"))?;
    if text.trim().is_empty() {
        bail!("Le résultat est vide.");
    }
    let _com = ComApartment::new()?;
    wait_for_modifiers()?;
    if quick {
        // Background translation must not steal focus if the user switched applications.
        check_foreground(selection.window, selection.process)?;
    }
    unsafe {
        let window = hwnd(selection.window);
        if !IsWindow(window).as_bool() {
            bail!("La fenêtre d’origine a été fermée. Utilise Copier.");
        }
        // SetForegroundWindow is subject to Windows focus rules; never paste if it failed.
        if !quick {
            let _ = SetForegroundWindow(window);
        }
    }
    let deadline = Instant::now() + Duration::from_millis(600);
    while check_foreground(selection.window, selection.process).is_err() {
        if Instant::now() >= deadline {
            bail!("Impossible de retrouver le focus de la fenêtre d’origine. Utilise Copier.");
        }
        thread::sleep(Duration::from_millis(15));
    }
    if destination(selection.window)? != selection.destination {
        bail!("Le document ou le contrôle d’origine a changé. Utilise Copier.");
    }
    let accessible = accessible_selection().ok();
    if let Some(identity) = &selection.accessibility
        && accessible.as_ref().is_none_or(|a| &a.identity != identity)
    {
        bail!("La position de la sélection d’origine a changé. Utilise Copier.");
    }
    let current = copy_selection(
        selection.window,
        selection.process,
        accessible.as_ref().map(|a| a.text.as_str()),
    )?;
    if current != selection.text {
        bail!("La sélection a changé. Aucun remplacement effectué ; utilise Copier.");
    }
    check_destination(selection)?;
    let sequence = write_text(text)?;
    check_destination(selection)?;
    if unsafe { GetClipboardSequenceNumber() } != sequence {
        bail!("Le presse-papiers a changé avant le collage. Utilise Copier.");
    }
    send_ctrl(b'V')?;
    // Keep the translation in the clipboard: a fixed-delay restoration can cause
    // a slow editor to paste the OLD clipboard. SendInput only acknowledges enqueue.
    Ok(())
}

pub fn copy_text(text: &str) -> Result<()> {
    let _operation = CLIPBOARD_OPERATION
        .lock()
        .map_err(|_| anyhow::anyhow!("Service presse-papiers interrompu"))?;
    write_text(text).map(|_| ())
}

fn hwnd(value: isize) -> HWND {
    HWND(value as *mut _)
}

fn check_foreground(window: isize, expected_process: u32) -> Result<()> {
    unsafe {
        let foreground = GetForegroundWindow();
        let mut process = 0;
        GetWindowThreadProcessId(foreground, Some(&mut process));
        if foreground != hwnd(window) || process != expected_process {
            bail!("La fenêtre active a changé ; opération annulée.");
        }
    }
    Ok(())
}

fn check_destination(selection: &Selection) -> Result<()> {
    check_foreground(selection.window, selection.process)?;
    if destination(selection.window)? != selection.destination {
        bail!("Le document ou le contrôle actif a changé avant le collage. Utilise Copier.");
    }
    if let Some(identity) = &selection.accessibility {
        let current =
            accessible_selection().context("La sélection accessible a disparu ; utilise Copier")?;
        if &current.identity != identity || current.text != selection.text {
            bail!("La position ou le contenu de la sélection a changé. Utilise Copier.");
        }
    }
    check_foreground(selection.window, selection.process)
}

fn wait_for_modifiers() -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
        .iter()
        .any(|key| unsafe { GetAsyncKeyState(key.0 as i32) } < 0)
    {
        if Instant::now() >= deadline {
            bail!("Relâche les touches du raccourci, puis réessaie.");
        }
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

fn send_ctrl(key: u8) -> Result<()> {
    let input = |key, flags| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                dwFlags: flags,
                ..Default::default()
            },
        },
    };
    let inputs = [
        input(VK_CONTROL, Default::default()),
        input(VIRTUAL_KEY(key as u16), Default::default()),
        input(VIRTUAL_KEY(key as u16), KEYEVENTF_KEYUP),
        input(VK_CONTROL, KEYEVENTF_KEYUP),
    ];
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent != inputs.len() as u32 {
        // Release injected keys after partial insertion, to avoid a stuck Ctrl.
        let releases = [
            input(VIRTUAL_KEY(key as u16), KEYEVENTF_KEYUP),
            input(VK_CONTROL, KEYEVENTF_KEYUP),
        ];
        unsafe {
            SendInput(&releases, size_of::<INPUT>() as i32);
        }
        bail!(
            "Windows a bloqué la simulation clavier (application administrateur ou saisie protégée)."
        );
    }
    Ok(())
}

fn copy_selection(window: isize, process: u32, accessible_text: Option<&str>) -> Result<String> {
    let snapshot = match ClipboardSnapshot::take() {
        Ok(snapshot) => snapshot,
        // A private clipboard format must not be destroyed just to read a selection.
        Err(_) if accessible_text.is_some() => return Ok(accessible_text.unwrap().to_owned()),
        Err(error) => return Err(error),
    };
    let before = snapshot.sequence;
    check_foreground(window, process)?;
    send_ctrl(b'C')?;
    let deadline = Instant::now() + Duration::from_millis(1200);
    let result = loop {
        if let Err(error) = check_foreground(window, process) {
            break Err(error);
        }
        if unsafe { GetClipboardSequenceNumber() } != before {
            match read_capture(process) {
                Ok((text, sequence)) => break Ok((text, sequence)),
                Err(_) if Instant::now() < deadline => {}
                Err(error) => break Err(error),
            }
        }
        if Instant::now() >= deadline {
            break Err(anyhow::anyhow!(
                "Aucun texte copié. Vérifie la sélection et le support de Ctrl+C."
            ));
        }
        thread::sleep(Duration::from_millis(15));
    };
    // This sequence was read under the SAME clipboard lock as the text. A subsequent
    // writer cannot accidentally grant us permission to overwrite their new content.
    let (text, sequence) = result?;
    if check_foreground(window, process).is_ok() {
        snapshot.restore_if_unchanged(sequence)?;
    }
    text
}

struct ClipboardLock;
struct ClipboardOwner(HWND);
impl Drop for ClipboardOwner {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}
static CLIPBOARD_WINDOW: OnceLock<std::result::Result<isize, String>> = OnceLock::new();

fn clipboard_window() -> Result<HWND> {
    let owner = CLIPBOARD_WINDOW.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("clipboard-owner".into())
            .spawn(move || {
                let window = unsafe {
                    CreateWindowExW(
                        WINDOW_EX_STYLE::default(),
                        w!("STATIC"),
                        w!("TranslationTool clipboard"),
                        WINDOW_STYLE::default(),
                        0,
                        0,
                        0,
                        0,
                        HWND_MESSAGE,
                        None,
                        None,
                        None,
                    )
                };
                match window {
                    Ok(window) => {
                        let _owner = ClipboardOwner(window);
                        if sender.send(Ok(window.0 as isize)).is_err() {
                            return;
                        }
                        // Clipboard owners receive synchronous WM_DESTROYCLIPBOARD from
                        // other applications. Pump independently of sleeping capture jobs,
                        // otherwise their Ctrl+C can deadlock trying to empty the clipboard.
                        unsafe {
                            let mut message = MSG::default();
                            while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                                let _ = TranslateMessage(&message);
                                DispatchMessageW(&message);
                            }
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error.to_string()));
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        receiver.recv().map_err(|e| e.to_string())?
    });
    match owner {
        Ok(window) => Ok(hwnd(*window)),
        Err(error) => bail!("Fenêtre presse-papiers : {error}"),
    }
}

impl ClipboardLock {
    fn acquire() -> Result<Self> {
        let owner = clipboard_window()?;
        let deadline = Instant::now() + Duration::from_millis(300);
        loop {
            if unsafe { OpenClipboard(owner) }.is_ok() {
                return Ok(Self);
            }
            if Instant::now() >= deadline {
                bail!("Le presse-papiers est occupé par une autre application.");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for ClipboardLock {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

// OleDuplicateData duplicates global memory AND format-specific GDI objects. Never
// proceed with a partial snapshot: capture must not erase unsupported private data.
struct ClipboardSnapshot {
    formats: Vec<ClipboardFormat>,
    sequence: u32,
}

struct ClipboardFormat {
    format: u32,
    handle: HANDLE,
}
impl Drop for ClipboardFormat {
    fn drop(&mut self) {
        if self.handle.0.is_null() {
            return;
        }
        unsafe {
            match self.format {
                2 | 9 => {
                    let _ = DeleteObject(HGDIOBJ(self.handle.0));
                }
                14 => {
                    let _ = DeleteEnhMetaFile(HENHMETAFILE(self.handle.0));
                }
                3 => {
                    let global = HGLOBAL(self.handle.0);
                    let pointer = GlobalLock(global).cast::<METAFILEPICT>();
                    if !pointer.is_null() {
                        let _ = DeleteMetaFile((*pointer).hMF);
                        let _ = GlobalUnlock(global);
                    }
                    let _ = windows::Win32::Foundation::GlobalFree(global);
                }
                _ => {
                    let _ = windows::Win32::Foundation::GlobalFree(HGLOBAL(self.handle.0));
                }
            }
        }
    }
}
impl ClipboardSnapshot {
    fn take() -> Result<Self> {
        let _lock = ClipboardLock::acquire()?;
        let mut formats = Vec::new();
        let mut format = 0;
        unsafe {
            loop {
                format = EnumClipboardFormats(format);
                if format == 0 {
                    break;
                }
                if matches!(format, 0x80..=0x8e | 0x200..=0x3ff) {
                    bail!(
                        "Le presse-papiers contient un format privé non restaurable. Copie d’abord du texte, puis réessaie."
                    );
                }
                let original = GetClipboardData(format)
                    .context("Impossible de préserver le presse-papiers")?;
                let duplicate =
                    OleDuplicateData(original, CLIPBOARD_FORMAT(format as u16), GMEM_MOVEABLE);
                if duplicate.0.is_null() {
                    bail!("Impossible de préserver un format du presse-papiers ; capture annulée.");
                }
                formats.push(ClipboardFormat {
                    format,
                    handle: duplicate,
                });
            }
        }
        Ok(Self {
            formats,
            sequence: unsafe { GetClipboardSequenceNumber() },
        })
    }

    fn restore_if_unchanged(mut self, sequence: u32) -> Result<()> {
        let _lock = ClipboardLock::acquire()?;
        // The sequence comparison is inside the clipboard lock to avoid a check/write race.
        if unsafe { GetClipboardSequenceNumber() } != sequence {
            return Ok(());
        }
        unsafe {
            EmptyClipboard()?;
        }
        for data in &mut self.formats {
            unsafe {
                SetClipboardData(data.format, data.handle)?;
            }
            data.handle = HANDLE::default(); // ownership transferred to Windows
        }
        Ok(())
    }
}

fn read_text(expected_process: u32) -> Result<(String, u32)> {
    let (text, sequence) = read_capture(expected_process)?;
    text.map(|text| (text, sequence))
}

// The outer error means we could not establish the owner/generation. A decoding
// error belongs to the inner result, so even invalid copied text can be restored.
fn read_capture(expected_process: u32) -> Result<(Result<String>, u32)> {
    let _lock = ClipboardLock::acquire()?;
    unsafe {
        let owner = GetClipboardOwner()?;
        let mut process = 0;
        GetWindowThreadProcessId(owner, Some(&mut process));
        if process != expected_process {
            bail!("Le presse-papiers a été modifié par une autre application.");
        }
        let result = (|| -> Result<String> {
            let handle = GetClipboardData(UNICODE_TEXT)
                .context("La sélection copiée n’est pas du texte Unicode")?;
            let global = HGLOBAL(handle.0);
            let length = GlobalSize(global) / 2;
            let pointer = GlobalLock(global).cast::<u16>();
            if pointer.is_null() {
                bail!("Impossible de lire le presse-papiers.");
            }
            let units = std::slice::from_raw_parts(pointer, length);
            let end = units.iter().position(|c| *c == 0).unwrap_or(units.len());
            let result = if end > MAX_TEXT_UNITS {
                Err(anyhow::anyhow!(
                    "Sélection trop longue (100 000 caractères UTF-16 maximum)."
                ))
            } else {
                String::from_utf16(&units[..end])
                    .context("Le texte copié n’est pas un Unicode valide")
            };
            let _ = GlobalUnlock(global);
            result
        })();
        Ok((result, GetClipboardSequenceNumber()))
    }
}

fn write_text(text: &str) -> Result<u32> {
    if text.contains('\0') || text.encode_utf16().count() > MAX_TEXT_UNITS {
        bail!("Le texte à copier contient un caractère nul ou dépasse 100 000 caractères UTF-16.");
    }
    let bytes: Vec<u8> = text
        .encode_utf16()
        .chain(Some(0))
        .flat_map(u16::to_ne_bytes)
        .collect();
    let lock = ClipboardLock::acquire()?;
    unsafe {
        EmptyClipboard()?;
    }
    set_format(UNICODE_TEXT, &bytes)?;
    // Closing publishes synthesized text formats and can increment the sequence.
    // Re-open and verify our owner/content before returning the committed generation.
    drop(lock);
    let (committed, sequence) = read_text(unsafe { GetCurrentProcessId() })?;
    if committed != text {
        bail!("Le presse-papiers a changé pendant l’écriture.");
    }
    Ok(sequence)
}

fn set_format(format: u32, bytes: &[u8]) -> Result<()> {
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes.len())?;
        let pointer = GlobalLock(memory);
        if pointer.is_null() {
            let _ = windows::Win32::Foundation::GlobalFree(memory);
            bail!("Allocation presse-papiers impossible.");
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast::<u8>(), bytes.len());
        let _ = GlobalUnlock(memory);
        if let Err(error) = SetClipboardData(format, HANDLE(memory.0)) {
            let _ = windows::Win32::Foundation::GlobalFree(memory);
            return Err(error.into());
        }
        // SetClipboardData now owns the global allocation.
    }
    Ok(())
}

struct ComApartment;
impl ComApartment {
    fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        Ok(Self)
    }
}
impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct AccessibilityIdentity {
    runtime_id: Vec<i32>,
    prefix_units: Option<usize>,
}

struct AccessibleSelection {
    text: String,
    rect: Option<Rect>,
    identity: AccessibilityIdentity,
}

struct SafeArray(*mut windows::Win32::System::Com::SAFEARRAY);
impl Drop for SafeArray {
    fn drop(&mut self) {
        unsafe {
            let _ = SafeArrayDestroy(self.0);
        }
    }
}

fn accessible_selection() -> Result<AccessibleSelection> {
    unsafe {
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)?;
        let options: IUIAutomation2 = automation.cast()?;
        options.SetConnectionTimeout(300)?;
        options.SetTransactionTimeout(300)?;
        let element = automation.GetFocusedElement()?;
        let pattern: IUIAutomationTextPattern = element.GetCurrentPatternAs(UIA_TextPatternId)?;
        let selection = pattern.GetSelection()?;
        if selection.Length()? != 1 {
            bail!("Sélection absente ou multiple.");
        }
        let range = selection.GetElement(0)?;
        let text = range.GetText(MAX_TEXT_UNITS as i32 + 1)?.to_string();
        if text.is_empty() {
            bail!("Aucune sélection accessible.");
        }
        let ids = SafeArray(element.GetRuntimeId()?);
        if ids.0.is_null() {
            bail!("Identifiant accessible absent.");
        }
        let lower = SafeArrayGetLBound(ids.0, 1)?;
        let upper = SafeArrayGetUBound(ids.0, 1)?;
        if !(1..=64).contains(&(upper - lower + 1)) {
            bail!("Identifiant accessible invalide.");
        }
        let mut runtime_id = Vec::new();
        for index in lower..=upper {
            let mut value = 0i32;
            SafeArrayGetElement(ids.0, &index, (&mut value as *mut i32).cast())?;
            runtime_id.push(value);
        }
        let prefix_units = (|| -> Result<usize> {
            let prefix = pattern.DocumentRange()?;
            prefix.MoveEndpointByRange(
                TextPatternRangeEndpoint_End,
                &range,
                TextPatternRangeEndpoint_Start,
            )?;
            let text = prefix.GetText(1_000_001)?;
            let length = text.len();
            if length > 1_000_000 {
                bail!("Document trop long pour identifier la position.");
            }
            Ok(length)
        })()
        .ok();
        let rect = (|| -> Result<Rect> {
            let rects = SafeArray(range.GetBoundingRectangles()?);
            if rects.0.is_null() {
                bail!("Rectangle absent.");
            }
            let lower = SafeArrayGetLBound(rects.0, 1)?;
            let upper = SafeArrayGetUBound(rects.0, 1)?;
            if upper - lower + 1 < 4 {
                bail!("Coordonnées de sélection indisponibles.");
            }
            let mut values = [0_f64; 4];
            for (offset, value) in values.iter_mut().enumerate() {
                let index = lower + offset as i32;
                SafeArrayGetElement(rects.0, &index, (value as *mut f64).cast())?;
            }
            if values.iter().any(|v| !v.is_finite()) || values[2] <= 0. || values[3] <= 0. {
                bail!("Rectangle invalide.");
            }
            Ok(Rect {
                left: values[0] as f32,
                top: values[1] as f32,
                right: (values[0] + values[2]) as f32,
                bottom: (values[1] + values[3]) as f32,
            })
        })()
        .ok();
        Ok(AccessibleSelection {
            text,
            rect,
            identity: AccessibilityIdentity {
                runtime_id,
                prefix_units,
            },
        })
    }
}

fn placement(anchor: Option<Rect>, cursor: POINT) -> Result<Placement> {
    let rect = anchor.unwrap_or(Rect {
        left: cursor.x as f32,
        right: cursor.x as f32,
        top: cursor.y as f32,
        bottom: cursor.y as f32,
    });
    unsafe {
        let monitor = MonitorFromPoint(
            POINT {
                x: rect.left as i32,
                y: rect.top as i32,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(monitor, &mut info).ok()?;
        let (mut dpi_x, mut dpi_y) = (96, 96);
        GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y)?;
        let scale = dpi_x as f32 / 96.;
        let area = Rect {
            left: info.rcWork.left as f32 / scale,
            top: info.rcWork.top as f32 / scale,
            right: info.rcWork.right as f32 / scale,
            bottom: info.rcWork.bottom as f32 / scale,
        };
        let logical = Rect {
            left: rect.left / scale,
            right: rect.right / scale,
            top: rect.top / scale,
            bottom: rect.bottom / scale,
        };
        let width = 520_f32.min(area.right - area.left);
        let height = 420_f32.min(area.bottom - area.top);
        let (x, y) = place_popup(logical, area, width, height);
        Ok(Placement {
            monitor: monitor.0 as u64,
            x,
            y,
            width,
            height,
            used_selection: anchor.is_some(),
        })
    }
}

pub fn place_popup(anchor: Rect, area: Rect, width: f32, height: f32) -> (f32, f32) {
    let x = anchor
        .left
        .clamp(area.left, (area.right - width).max(area.left));
    let below = anchor.bottom + 10.;
    let y = if below + height <= area.bottom {
        below
    } else {
        anchor.top - height - 10.
    };
    (x, y.clamp(area.top, (area.bottom - height).max(area.top)))
}

pub fn set_dark_titlebar(hwnd: isize, dark: bool) -> Result<()> {
    use windows::Win32::{
        Foundation::{BOOL, HWND},
        Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute},
    };
    let enabled = BOOL::from(dark);
    // The handle belongs to a live GPUI window; DWM reads the BOOL during this call.
    unsafe {
        DwmSetWindowAttribute(
            HWND(hwnd as *mut std::ffi::c_void),
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&enabled as *const BOOL).cast(),
            std::mem::size_of::<BOOL>() as u32,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popup_stays_in_work_area_including_negative_monitor_coordinates() {
        let area = Rect {
            left: -1920.,
            top: 0.,
            right: 0.,
            bottom: 1040.,
        };
        let anchor = Rect {
            left: -50.,
            top: 990.,
            right: -10.,
            bottom: 1010.,
        };
        assert_eq!(place_popup(anchor, area, 520., 420.), (-520., 560.));
        let anchor = Rect {
            left: -1800.,
            top: 30.,
            right: -1700.,
            bottom: 50.,
        };
        assert_eq!(place_popup(anchor, area, 520., 420.), (-1800., 60.));
    }
}
