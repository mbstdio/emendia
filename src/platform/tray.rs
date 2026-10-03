use anyhow::Result;
use tray_icon::menu::MenuId;

#[cfg(target_os = "windows")]
#[path = "windows/tray.rs"]
mod backend;
#[cfg(target_os = "linux")]
#[path = "linux/tray.rs"]
mod backend;

pub struct Tray {
    inner: backend::Tray,
    dark_system: bool,
}

impl Tray {
    pub fn new(dark_system: bool) -> Result<Self> {
        Ok(Self {
            inner: backend::Tray::new(super::icons::tray_icon(dark_system)?)?,
            dark_system,
        })
    }

    pub fn settings_id(&self) -> &MenuId {
        #[cfg(target_os = "windows")]
        {
            self.inner.settings.id()
        }
        #[cfg(target_os = "linux")]
        {
            &self.inner.settings
        }
    }

    pub fn enabled_id(&self) -> &MenuId {
        #[cfg(target_os = "windows")]
        {
            self.inner.enabled.id()
        }
        #[cfg(target_os = "linux")]
        {
            &self.inner.enabled
        }
    }

    pub fn quit_id(&self) -> &MenuId {
        #[cfg(target_os = "windows")]
        {
            self.inner.quit.id()
        }
        #[cfg(target_os = "linux")]
        {
            &self.inner.quit
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        #[cfg(target_os = "windows")]
        self.inner.enabled.set_checked(enabled);
        #[cfg(target_os = "linux")]
        self.inner.set_enabled(enabled);
    }

    pub fn available(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            true
        }
        #[cfg(target_os = "linux")]
        {
            self.inner.available()
        }
    }

    pub fn localize(&self) {
        self.inner.localize();
    }

    pub fn set_system_theme(&mut self, dark_system: bool) -> Result<()> {
        if self.dark_system != dark_system {
            self.inner.set_icon(super::icons::tray_icon(dark_system)?)?;
            self.dark_system = dark_system;
        }
        Ok(())
    }
}
