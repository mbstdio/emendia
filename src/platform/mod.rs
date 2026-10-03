pub mod desktop;
pub mod hotkey;
pub(crate) mod icons;
#[cfg(target_os = "linux")]
pub mod linux;
pub mod startup;
pub mod tray;
#[cfg(target_os = "windows")]
pub mod windows;

pub fn initialize() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    linux::initialize()?;
    Ok(())
}
