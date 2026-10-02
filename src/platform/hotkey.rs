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

pub fn parse_pair(preview: &str, quick: &str) -> Result<[HotKey; 2]> {
    let keys = [parse(preview)?, parse(quick)?];
    if keys[0] == keys[1] {
        bail!("Les raccourcis de l’aperçu et du Quick Translate doivent être différents.");
    }
    Ok(keys)
}

// This object lives on GPUI's main thread, which owns the Win32 message loop.
pub struct HotkeyRegistration {
    manager: GlobalHotKeyManager,
    hotkeys: Option<[HotKey; 2]>,
    registered: Vec<HotKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranslationMode {
    Preview,
    Quick,
}

impl HotkeyRegistration {
    pub fn new() -> Result<Self> {
        Ok(Self {
            manager: GlobalHotKeyManager::new()?,
            hotkeys: None,
            registered: Vec::new(),
        })
    }

    pub fn change(&mut self, preview: &str, quick: &str) -> Result<()> {
        self.change_with(preview, quick, || Ok(()))
    }

    pub fn change_with(
        &mut self,
        preview: &str,
        quick: &str,
        persist: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let next = parse_pair(preview, quick)?;
        update_registration(
            &mut self.registered,
            &next,
            |key| {
                self.manager
                    .register(key)
                    .context("Raccourci déjà utilisé par une autre application")
            },
            |key| self.manager.unregister(key).map_err(Into::into),
            persist,
        )?;
        self.hotkeys = Some(next);
        Ok(())
    }

    pub fn disable(&mut self) -> Result<()> {
        update_registration(
            &mut self.registered,
            &[],
            |key| self.manager.register(key).map_err(Into::into),
            |key| self.manager.unregister(key).map_err(Into::into),
            || Ok(()),
        )?;
        self.hotkeys = None;
        Ok(())
    }

    pub fn mode(&self, id: u32) -> Option<TranslationMode> {
        let keys = self.hotkeys.as_ref()?;
        if keys[0].id() == id {
            Some(TranslationMode::Preview)
        } else if keys[1].id() == id {
            Some(TranslationMode::Quick)
        } else {
            None
        }
    }

    pub fn enabled(&self) -> bool {
        self.hotkeys.is_some()
    }
}

// Keep the previous pair reserved until persistence succeeds. An obsolete key that
// cannot be released remains tracked for cleanup on the next change; it is not dispatched.
fn update_registration(
    registered: &mut Vec<HotKey>,
    next: &[HotKey],
    register: impl Fn(HotKey) -> Result<()>,
    unregister: impl Fn(HotKey) -> Result<()>,
    persist: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let old = registered.clone();
    let mut added = Vec::new();
    let prepared = (|| {
        for &key in next.iter().filter(|key| !old.contains(key)) {
            register(key)?;
            registered.push(key);
            added.push(key);
        }
        persist()
    })();
    if let Err(error) = prepared {
        let mut cleanup_errors = Vec::new();
        for key in added {
            match unregister(key) {
                Ok(()) => registered.retain(|reserved| *reserved != key),
                Err(cleanup) => cleanup_errors.push(cleanup.to_string()),
            }
        }
        if !cleanup_errors.is_empty() {
            return Err(error.context(format!(
                "Nettoyage des nouveaux raccourcis incomplet : {}",
                cleanup_errors.join(" ; ")
            )));
        }
        return Err(error);
    }
    for &key in old.iter().filter(|key| !next.contains(key)) {
        match unregister(key) {
            Ok(()) => registered.retain(|reserved| *reserved != key),
            Err(error) => {
                // New bindings and settings are already committed. Keep their mapping
                // valid instead of trying to re-reserve old keys another process may take.
                tracing::warn!(%error, "Ancien raccourci conservé pour une prochaine tentative de nettoyage");
            }
        }
    }
    Ok(())
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

    #[test]
    fn exchanging_shortcuts_keeps_their_reservations() {
        let old = [
            parse("Ctrl+Alt+KeyT").unwrap(),
            parse("Ctrl+Alt+KeyQ").unwrap(),
        ];
        let mut registered = old.to_vec();
        update_registration(
            &mut registered,
            &[old[1], old[0]],
            |_| panic!("already registered"),
            |_| panic!("still needed"),
            || Ok(()),
        )
        .unwrap();
    }

    #[test]
    fn second_shortcut_conflict_preserves_the_previous_pair() {
        use std::cell::RefCell;
        let old = [
            parse("Ctrl+Alt+KeyT").unwrap(),
            parse("Ctrl+Alt+KeyQ").unwrap(),
        ];
        let next = [
            parse("Ctrl+Alt+KeyY").unwrap(),
            parse("Ctrl+Alt+KeyU").unwrap(),
        ];
        let reserved = RefCell::new(old.to_vec());
        let mut registered = old.to_vec();
        let result = update_registration(
            &mut registered,
            &next,
            |key| {
                if key == next[1] {
                    bail!("conflict");
                }
                reserved.borrow_mut().push(key);
                Ok(())
            },
            |key| {
                reserved
                    .borrow_mut()
                    .retain(|registered| *registered != key);
                Ok(())
            },
            || Ok(()),
        );
        assert!(result.is_err());
        assert_eq!(*reserved.borrow(), old);
        assert_eq!(registered, old);
    }

    #[test]
    fn persistence_failure_never_releases_previous_shortcuts() {
        use std::cell::RefCell;
        let old = parse_pair("Ctrl+Alt+KeyT", "Ctrl+Alt+KeyQ").unwrap();
        let next = parse_pair("Ctrl+Alt+KeyY", "Ctrl+Alt+KeyU").unwrap();
        let reserved = RefCell::new(old.to_vec());
        let mut registered = old.to_vec();
        let result = update_registration(
            &mut registered,
            &next,
            |key| {
                reserved.borrow_mut().push(key);
                Ok(())
            },
            |key| {
                assert!(
                    !old.contains(&key),
                    "old keys must never be released on persistence failure"
                );
                reserved.borrow_mut().retain(|reserved| *reserved != key);
                Ok(())
            },
            || {
                assert!(
                    old.iter()
                        .chain(next.iter())
                        .all(|key| reserved.borrow().contains(key))
                );
                bail!("settings save failed")
            },
        );
        assert!(result.is_err());
        assert_eq!(registered, old);
        assert_eq!(*reserved.borrow(), old);
    }

    #[test]
    fn obsolete_keys_remain_tracked_when_cleanup_fails() {
        let old = parse_pair("Ctrl+Alt+KeyT", "Ctrl+Alt+KeyQ").unwrap();
        let next = parse_pair("Ctrl+Alt+KeyY", "Ctrl+Alt+KeyU").unwrap();
        let mut registered = old.to_vec();
        update_registration(
            &mut registered,
            &next,
            |_| Ok(()),
            |_| bail!("cleanup failed"),
            || Ok(()),
        )
        .unwrap();
        assert!(
            old.iter()
                .chain(next.iter())
                .all(|key| registered.contains(key))
        );
        update_registration(
            &mut registered,
            &next,
            |_| panic!("already reserved"),
            |_| Ok(()),
            || Ok(()),
        )
        .unwrap();
        assert_eq!(registered, next);
    }
}
