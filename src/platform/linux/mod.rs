mod clipboard;
pub mod desktop;
pub(crate) mod hotkey;
pub(crate) mod portal;
mod startup;
pub mod x11;
pub use startup::{configure, migrate_legacy};

pub fn initialize() -> anyhow::Result<()> {
    use anyhow::Context;
    if is_wayland() {
        portal::initialize()?;
        return Ok(());
    }
    x11rb::connect(None)
        .context("Unable to connect to X11; check DISPLAY and your desktop session")?;
    Ok(())
}

pub fn is_wayland() -> bool {
    // Match GPUI's backend selection, including explicitly selecting X11 from a
    // Wayland login session by removing WAYLAND_DISPLAY.
    gpui_kit::guess_compositor() == "Wayland"
}
