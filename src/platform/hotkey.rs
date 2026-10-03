#[cfg(target_os = "linux")]
use super::linux::hotkey::Manager as GlobalHotKeyManager;
use anyhow::{Context, Result, bail};
#[cfg(not(target_os = "linux"))]
use global_hotkey::GlobalHotKeyManager;
use global_hotkey::hotkey::{HotKey, Modifiers};

pub(crate) fn next_event() -> Option<global_hotkey::GlobalHotKeyEvent> {
    #[cfg(target_os = "linux")]
    {
        super::linux::hotkey::next_event()
    }
    #[cfg(not(target_os = "linux"))]
    {
        global_hotkey::GlobalHotKeyEvent::receiver().try_recv().ok()
    }
}

pub fn parse(value: &str) -> Result<HotKey> {
    let hotkey: HotKey = value
        .parse()
        .context(crate::i18n::t("Invalid shortcut (example: Ctrl+Alt+KeyT)"))?;
    if !hotkey
        .mods
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
    {
        bail!(crate::i18n::t(
            "The shortcut must include Ctrl, Alt or Super/Win."
        ));
    }
    Ok(hotkey)
}

pub fn parse_pair(preview: &str, quick: &str) -> Result<[HotKey; 2]> {
    let keys = [parse(preview)?, parse(quick)?];
    if keys[0] == keys[1] {
        bail!(crate::i18n::t(
            "Preview and Quick Translate shortcuts must be different."
        ));
    }
    Ok(keys)
}

pub fn parse_shortcuts(values: [&str; 4]) -> Result<[HotKey; 4]> {
    if values.iter().any(|value| value.trim().is_empty()) {
        bail!(crate::i18n::t(
            "Assign a shortcut to every action before saving."
        ));
    }
    let keys = [
        parse(values[0])?,
        parse(values[1])?,
        parse(values[2])?,
        parse(values[3])?,
    ];
    for (index, key) in keys.iter().enumerate() {
        if keys[..index].contains(key) {
            bail!(crate::i18n::t("All four shortcuts must be different."));
        }
    }
    Ok(keys)
}

// Lives on GPUI's main thread; Windows registration uses its Win32 message loop.
pub struct HotkeyRegistration {
    manager: GlobalHotKeyManager,
    hotkeys: Option<[HotKey; 4]>,
    registered: Vec<HotKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranslationMode {
    Preview,
    Quick,
    CorrectionPreview,
    CorrectionQuick,
}

impl TranslationMode {
    pub const ALL: [Self; 4] = [
        Self::Preview,
        Self::Quick,
        Self::CorrectionPreview,
        Self::CorrectionQuick,
    ];

    pub fn index(self) -> usize {
        match self {
            Self::Preview => 0,
            Self::Quick => 1,
            Self::CorrectionPreview => 2,
            Self::CorrectionQuick => 3,
        }
    }

    pub fn label(self) -> &'static str {
        crate::i18n::t(match self {
            Self::Preview => "Translate with preview",
            Self::Quick => "Quick Translate",
            Self::CorrectionPreview => "Proofread with preview",
            Self::CorrectionQuick => "Quick Check",
        })
    }

    pub fn operation(self) -> crate::settings::Operation {
        match self {
            Self::Preview | Self::Quick => crate::settings::Operation::Translation,
            Self::CorrectionPreview | Self::CorrectionQuick => {
                crate::settings::Operation::Correction
            }
        }
    }

    pub fn quick(self) -> bool {
        matches!(self, Self::Quick | Self::CorrectionQuick)
    }
}

impl HotkeyRegistration {
    pub fn new() -> Result<Self> {
        Ok(Self {
            manager: GlobalHotKeyManager::new()?,
            hotkeys: None,
            registered: Vec::new(),
        })
    }

    pub fn change(&mut self, values: [&str; 4]) -> Result<()> {
        self.change_with(values, || Ok(()))
    }

    pub fn change_with(
        &mut self,
        values: [&str; 4],
        persist: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        let next = parse_shortcuts(values)?;
        update_registration(
            &mut self.registered,
            &next,
            |key| {
                self.manager.register(key).context(crate::i18n::t(
                    "Shortcut already used by another application",
                ))
            },
            |key| {
                self.manager
                    .unregister(key)
                    .context(crate::i18n::t("Unable to release shortcut"))
            },
            persist,
        )?;
        self.hotkeys = Some(next);
        Ok(())
    }

    pub fn disable(&mut self) -> Result<()> {
        update_registration(
            &mut self.registered,
            &[],
            |key| {
                self.manager.register(key).context(crate::i18n::t(
                    "Shortcut already used by another application",
                ))
            },
            |key| {
                self.manager
                    .unregister(key)
                    .context(crate::i18n::t("Unable to release shortcut"))
            },
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
        } else if keys[2].id() == id {
            Some(TranslationMode::CorrectionPreview)
        } else if keys[3].id() == id {
            Some(TranslationMode::CorrectionQuick)
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
                "{}: {}",
                crate::i18n::t("Incomplete cleanup of new shortcuts"),
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
                tracing::warn!(%error, "Old shortcut retained for a later cleanup attempt");
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
    fn all_four_actions_have_distinct_valid_defaults() {
        let settings = crate::settings::Settings::default();
        let keys = parse_shortcuts(settings.shortcuts()).unwrap();
        for index in 0..4 {
            let mut values = settings.shortcuts();
            values[index] = "Alt+Ctrl+KeyT";
            values[(index + 1) % 4] = "Ctrl+Alt+KeyT";
            assert!(parse_shortcuts(values).is_err());
        }
        assert_eq!(keys[0], parse("Ctrl+F12").unwrap());
        assert_eq!(keys[3], parse("Ctrl+Shift+F11").unwrap());
    }

    #[test]
    fn fourth_shortcut_conflict_rolls_back_all_new_reservations() {
        let old = parse_shortcuts(crate::settings::Settings::default().shortcuts()).unwrap();
        let next = parse_shortcuts(["Ctrl+F1", "Ctrl+F2", "Ctrl+F3", "Ctrl+F4"]).unwrap();
        let mut registered = old.to_vec();
        let result = update_registration(
            &mut registered,
            &next,
            |key| {
                if key == next[3] {
                    bail!("conflict")
                } else {
                    Ok(())
                }
            },
            |key| {
                assert!(!old.contains(&key));
                Ok(())
            },
            || panic!("cannot persist"),
        );
        assert!(result.is_err());
        assert_eq!(registered, old);
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
