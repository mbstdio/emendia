//! Platform-neutral entry point for selection and desktop integration.
#[cfg(target_os = "linux")]
pub use super::linux::x11::*;
#[cfg(target_os = "windows")]
pub use super::windows::*;

#[cfg(target_os = "linux")]
pub(crate) use super::linux::x11::{
    center_window, cursor_monitor, foreground_window, status_is_nonactivating,
    status_placement_for_window,
};
#[cfg(target_os = "windows")]
pub(crate) use super::windows::{
    center_window, cursor_monitor, foreground_window, status_is_nonactivating,
    status_placement_for_window,
};

pub(crate) fn native_window(window: &gpui_kit::Window) -> anyhow::Result<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window)?;
    match handle.as_raw() {
        #[cfg(target_os = "windows")]
        RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get()),
        #[cfg(target_os = "linux")]
        RawWindowHandle::Xcb(handle) => Ok(handle.window.get() as isize),
        #[cfg(target_os = "linux")]
        RawWindowHandle::Xlib(handle) => Ok(handle.window as isize),
        _ => anyhow::bail!("Unsupported desktop window handle"),
    }
}

pub fn show_status_without_activation(
    window: isize,
    placement: Placement,
    scale: f32,
) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let _ = scale;
        super::windows::show_status_without_activation(window, placement)
    }
    #[cfg(target_os = "linux")]
    super::linux::x11::show_status_without_activation(window, placement, scale)
}

pub(crate) fn status_has_expected_bounds(window: isize, placement: Placement, scale: f32) -> bool {
    #[cfg(target_os = "windows")]
    {
        let _ = scale;
        super::windows::status_has_expected_bounds(window, placement)
    }
    #[cfg(target_os = "linux")]
    super::linux::x11::status_has_expected_bounds(window, placement, scale)
}
