//! Dispatch desktop operations to the backend actually used by GPUI.
use super::{is_wayland, portal, x11};
use anyhow::{Result, bail};
use std::sync::{OnceLock, RwLock};

pub use x11::Placement;

#[derive(Clone, Debug)]
pub enum CaptureTarget {
    X11(x11::CaptureTarget),
    Wayland(Placement),
}

#[derive(Clone, Debug)]
pub struct Selection {
    pub text: String,
    pub placement: Placement,
    original: Option<x11::Selection>,
}

impl From<x11::Selection> for Selection {
    fn from(original: x11::Selection) -> Self {
        Self {
            text: original.text.clone(),
            placement: original.placement,
            original: Some(original),
        }
    }
}

impl Selection {
    pub(crate) fn smoke_fixture() -> Result<Self> {
        if !is_wayland() {
            return x11::Selection::smoke_fixture().map(Into::into);
        }
        Ok(Self {
            text: "Bonjour, ceci est un test de traduction.".into(),
            placement: placement()?,
            original: None,
        })
    }
}

static PLACEMENT: OnceLock<RwLock<Option<Placement>>> = OnceLock::new();

pub(crate) fn configure_display(cx: &gpui_kit::App) -> Result<()> {
    if !is_wayland() {
        return x11::configure_display(cx);
    }
    // GPUI's Wayland backend deliberately has no primary_display. Select an
    // advertised output instead; otherwise capture has no placement at all.
    let display = cx.primary_display().or_else(|| {
        let mut displays = cx.displays();
        displays.sort_by_key(|display| u64::from(display.id()));
        displays.into_iter().next()
    });
    if let Some(display) = display {
        let bounds = display.bounds();
        let width = 520_f32.min(bounds.size.width.as_f32());
        // Leave room for client-side title controls, frame and the copy-only hint.
        let height = 520_f32.min(bounds.size.height.as_f32());
        *PLACEMENT.get_or_init(|| RwLock::new(None)).write().unwrap() = Some(Placement {
            monitor: display.id().into(),
            x: bounds.origin.x.as_f32(),
            y: bounds.origin.y.as_f32(),
            width,
            height,
            used_selection: false,
        });
    } else {
        *PLACEMENT.get_or_init(|| RwLock::new(None)).write().unwrap() = None;
    }
    Ok(())
}

fn placement() -> Result<Placement> {
    PLACEMENT
        .get_or_init(|| RwLock::new(None))
        .read()
        .unwrap()
        .ok_or_else(|| anyhow::anyhow!("No Wayland display is available"))
}

pub fn capture_target() -> Result<CaptureTarget> {
    if is_wayland() {
        portal::ensure_capture_ready()?;
        Ok(CaptureTarget::Wayland(placement()?))
    } else {
        x11::capture_target().map(CaptureTarget::X11)
    }
}

pub fn capture(target: CaptureTarget) -> Result<Selection> {
    match target {
        CaptureTarget::X11(target) => x11::capture(target).map(Into::into),
        CaptureTarget::Wayland(placement) => Ok(Selection {
            text: portal::capture()?,
            placement,
            original: None,
        }),
    }
}

pub fn status_placement(target: &CaptureTarget) -> Result<Placement> {
    match target {
        CaptureTarget::X11(target) => x11::status_placement(target),
        CaptureTarget::Wayland(placement) => Ok(*placement),
    }
}

pub fn replace(selection: &Selection, text: &str) -> Result<()> {
    match &selection.original {
        Some(original) => x11::replace(original, text),
        None => bail!(crate::i18n::t(
            "Wayland cannot verify the original window. Use Copy."
        )),
    }
}

pub fn replace_quick(selection: &Selection, text: &str) -> Result<()> {
    match &selection.original {
        Some(original) => x11::replace_quick(original, text),
        None => bail!(crate::i18n::t(
            "Wayland cannot verify the original window. Use Copy."
        )),
    }
}

pub fn copy_text(text: &str) -> Result<()> {
    if is_wayland() {
        portal::copy_text(text)
    } else {
        x11::copy_text(text)
    }
}

pub(crate) fn foreground_window() -> isize {
    if is_wayland() {
        0
    } else {
        x11::foreground_window()
    }
}

pub(crate) fn cursor_monitor() -> Result<u64> {
    if is_wayland() {
        Ok(placement()?.monitor)
    } else {
        x11::cursor_monitor()
    }
}

pub(crate) fn center_window(window: isize, width: f32, height: f32) -> Result<()> {
    if is_wayland() {
        bail!("Wayland window placement is controlled by the compositor");
    }
    x11::center_window(window, width, height)
}

pub(crate) fn status_placement_for_window(window: isize) -> Result<Placement> {
    if is_wayland() {
        placement()
    } else {
        x11::status_placement_for_window(window)
    }
}

pub(crate) fn status_is_nonactivating(window: isize) -> bool {
    !is_wayland() && x11::status_is_nonactivating(window)
}
