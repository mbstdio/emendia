use crate::i18n::t;
use anyhow::Result;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
};

pub struct Tray {
    _icon: TrayIcon,
    pub open: MenuItem,
    pub settings: MenuItem,
    pub enabled: CheckMenuItem,
    pub quit: MenuItem,
}

impl Tray {
    pub fn new(icon: Icon) -> Result<Self> {
        let menu = Menu::new();
        let open = MenuItem::new(t("Open Emendia"), true, None);
        let settings = MenuItem::new(t("Settings"), true, None);
        let enabled = CheckMenuItem::new(t("Shortcuts enabled"), true, true, None);
        let quit = MenuItem::new(t("Quit"), true, None);
        menu.append_items(&[
            &open,
            &settings,
            &enabled,
            &PredefinedMenuItem::separator(),
            &quit,
        ])?;
        let icon = TrayIconBuilder::new()
            .with_tooltip(t("Emendia — translation and proofreading"))
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()?;
        Ok(Self {
            _icon: icon,
            open,
            settings,
            enabled,
            quit,
        })
    }

    pub fn localize(&self) {
        self.open.set_text(t("Open Emendia"));
        self.settings.set_text(t("Settings"));
        self.enabled.set_text(t("Shortcuts enabled"));
        self.quit.set_text(t("Quit"));
        let _ = self
            ._icon
            .set_tooltip(Some(t("Emendia — translation and proofreading")));
    }

    pub fn set_icon(&self, icon: Icon) -> Result<()> {
        self._icon.set_icon(Some(icon))?;
        Ok(())
    }
}
