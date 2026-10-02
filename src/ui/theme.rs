use crate::{platform::windows, settings::ThemePreference};
use gpui_kit::{
    component::{ActiveTheme, Theme, ThemeMode},
    *,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

impl Global for ThemePreference {}

pub fn apply(preference: ThemePreference, cx: &mut App) {
    cx.set_global(preference);
    let mode = match preference {
        ThemePreference::Light => ThemeMode::Light,
        ThemePreference::Dark => ThemeMode::Dark,
        ThemePreference::System => cx.window_appearance().into(),
    };
    if cx.theme().mode != mode {
        Theme::change(mode, None, cx);
    }
}

pub fn configure_window(window: &mut Window, cx: &mut App) {
    // Read Windows' appearance when opening a window, including after time in the tray.
    sync_system(window, cx);
    update_titlebar(window, cx);
    window
        .observe_window_appearance(|window, cx| {
            sync_system(window, cx);
            // Windows can reset the native title bar even when a fixed theme is selected.
            update_titlebar(window, cx);
        })
        .detach();
    window.observe_global::<Theme>(cx, update_titlebar).detach();
    window
        .observe_global::<ThemePreference>(cx, update_titlebar)
        .detach();
}

fn sync_system(window: &mut Window, cx: &mut App) {
    if *cx.global::<ThemePreference>() == ThemePreference::System {
        let mode = ThemeMode::from(window.appearance());
        if cx.theme().mode != mode {
            Theme::change(mode, None, cx);
        }
    }
}

fn update_titlebar(window: &mut Window, cx: &mut App) {
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::Win32(handle) = handle.as_raw()
        && let Err(error) = windows::set_dark_titlebar(handle.hwnd.get(), cx.theme().mode.is_dark())
    {
        tracing::debug!(%error, "Unable to apply the title bar theme");
    }
}
