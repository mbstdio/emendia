use crate::i18n::t;
use anyhow::Result;
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
};

pub struct Tray {
    _icon: TrayIcon,
    pub settings: MenuItem,
    pub enabled: CheckMenuItem,
    pub quit: MenuItem,
}

impl Tray {
    pub fn new() -> Result<Self> {
        let menu = Menu::new();
        let settings = MenuItem::new(t("Settings"), true, None);
        let enabled = CheckMenuItem::new(t("Shortcuts enabled"), true, true, None);
        let quit = MenuItem::new(t("Quit"), true, None);
        menu.append_items(&[&settings, &enabled, &PredefinedMenuItem::separator(), &quit])?;
        let mut rgba = Vec::with_capacity(32 * 32 * 4);
        for y in 0..32 {
            for x in 0..32 {
                let letter = (7..11).contains(&x) && (7..26).contains(&y)
                    || (7..25).contains(&x)
                        && ((7..11).contains(&y) || (14..18).contains(&y) || (22..26).contains(&y));
                rgba.extend_from_slice(if letter {
                    &[255, 255, 255, 255]
                } else {
                    &[52, 104, 235, 255]
                });
            }
        }
        let icon = TrayIconBuilder::new()
            .with_tooltip(t("Emendia — translation and proofreading"))
            .with_icon(Icon::from_rgba(rgba, 32, 32)?)
            .with_menu(Box::new(menu))
            .build()?;
        Ok(Self {
            _icon: icon,
            settings,
            enabled,
            quit,
        })
    }

    pub fn localize(&self) {
        self.settings.set_text(t("Settings"));
        self.enabled.set_text(t("Shortcuts enabled"));
        self.quit.set_text(t("Quit"));
        let _ = self
            ._icon
            .set_tooltip(Some(t("Emendia — translation and proofreading")));
    }
}
