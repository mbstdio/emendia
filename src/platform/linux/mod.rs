mod clipboard;
pub(crate) mod hotkey;
mod startup;
pub mod x11;
pub use startup::{configure, migrate_legacy};

pub fn initialize() -> anyhow::Result<()> {
    use anyhow::{Context, bail};
    if std::env::var("XDG_SESSION_TYPE").is_ok_and(|value| value == "wayland")
        || std::env::var_os("WAYLAND_DISPLAY").is_some()
    {
        bail!("Emendia currently requires an X11 session. Log in using an Xorg/X11 session.");
    }
    x11rb::connect(None)
        .context("Unable to connect to X11; check DISPLAY and your desktop session")?;
    // GPUI chooses Wayland when WAYLAND_DISPLAY is present; reject that session
    // above rather than silently providing shortcuts only to XWayland clients.
    Ok(())
}
