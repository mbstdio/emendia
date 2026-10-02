use anyhow::{Context, Result, bail};
use global_hotkey::{
    GlobalHotKeyManager,
    hotkey::{HotKey, Modifiers},
};

pub fn parse(value: &str) -> Result<HotKey> {
    let hotkey: HotKey = value
        .parse()
        .context("Raccourci invalide (exemple : Ctrl+Alt+KeyT)")?;
    if !hotkey
        .mods
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
    {
        bail!("Le raccourci doit inclure Ctrl, Alt ou Win.");
    }
    Ok(hotkey)
}

// This object lives on GPUI's main thread, which owns the Win32 message loop.
pub struct HotkeyRegistration {
    manager: GlobalHotKeyManager,
    hotkey: Option<HotKey>,
}

impl HotkeyRegistration {
    pub fn new() -> Result<Self> {
        Ok(Self {
            manager: GlobalHotKeyManager::new()?,
            hotkey: None,
        })
    }

    pub fn change(&mut self, value: &str) -> Result<()> {
        let next = parse(value)?;
        if self.hotkey.as_ref() == Some(&next) {
            return Ok(());
        }
        // Reserve the new shortcut first; a conflict must not disable the old one.
        self.manager
            .register(next)
            .context("Raccourci déjà utilisé par une autre application")?;
        if let Some(old) = self.hotkey
            && let Err(error) = self.manager.unregister(old)
        {
            let _ = self.manager.unregister(next);
            return Err(error.into());
        }
        self.hotkey = Some(next);
        Ok(())
    }

    pub fn disable(&mut self) -> Result<()> {
        if let Some(hotkey) = self.hotkey {
            self.manager.unregister(hotkey)?;
            self.hotkey = None;
        }
        Ok(())
    }

    pub fn matches(&self, id: u32) -> bool {
        self.hotkey.is_some_and(|key| key.id() == id)
    }

    pub fn enabled(&self) -> bool {
        self.hotkey.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcut_requires_a_non_shift_modifier() {
        assert!(parse("Ctrl+Alt+KeyT").is_ok());
        assert!(parse("Shift+KeyT").is_err());
        assert!(parse("nonsense").is_err());
    }
}
