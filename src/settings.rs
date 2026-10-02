use anyhow::{Context, Result, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const LANGUAGES: &[&str] = &[
    "Français",
    "Anglais",
    "Allemand",
    "Espagnol",
    "Italien",
    "Portugais",
    "Néerlandais",
    "Japonais",
    "Chinois",
    "Coréen",
    "Arabe",
    "Ukrainien",
];
pub const AUTO: &str = "Automatique";

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    Light,
    Dark,
    #[default]
    System,
}

impl ThemePreference {
    pub const ALL: [Self; 3] = [Self::Light, Self::Dark, Self::System];

    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Clair",
            Self::Dark => "Sombre",
            Self::System => "Système",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|theme| theme.label() == label)
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum CorrectionStyle {
    #[default]
    Faithful,
    Fluent,
    Professional,
    Casual,
    Concise,
}

impl CorrectionStyle {
    pub const ALL: [Self; 5] = [
        Self::Faithful,
        Self::Fluent,
        Self::Professional,
        Self::Casual,
        Self::Concise,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Faithful => "Correction fidèle",
            Self::Fluent => "Plus fluide",
            Self::Professional => "Professionnel",
            Self::Casual => "Décontracté",
            Self::Concise => "Concis",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|style| style.label() == label)
    }

    pub fn instruction(self) -> &'static str {
        match self {
            Self::Faithful => {
                "Only fix spelling, grammar and punctuation. Preserve the original tone, register and wording as much as possible. Do not rephrase correct passages."
            }
            Self::Fluent => {
                "After correcting errors, rewrite awkward or unnatural phrasing for smoother flow and readability. Preserve the original tone, but do not simply copy sentences that need improvement."
            }
            Self::Professional => {
                "After correcting errors, rewrite the text in a polished professional tone suitable for workplace communication. Replace casual greetings, colloquial expressions and overly familiar wording with professional equivalents. Preserve the meaning, not the original register."
            }
            Self::Casual => {
                "After correcting errors, rewrite formal or stiff wording in a natural, relaxed and conversational tone. Use simple everyday expressions without adding slang, invented familiarity or information. Preserve the meaning, not the original register."
            }
            Self::Concise => {
                "After correcting errors, shorten verbose wording and remove repetition and filler. Produce a more concise text without losing any essential information."
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Operation {
    #[default]
    Translation,
    Correction,
}

impl Operation {
    pub fn title(self) -> &'static str {
        match self {
            Self::Translation => "Traduction",
            Self::Correction => "Correction",
        }
    }

    pub fn pending(self) -> &'static str {
        match self {
            Self::Translation => "Traduction en cours…",
            Self::Correction => "Correction en cours…",
        }
    }

    pub fn quick_title(self) -> &'static str {
        match self {
            Self::Translation => "Quick Translate",
            Self::Correction => "Quick Check",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub base_url: String,
    pub model: String,
    pub source_language: String,
    pub target_language: String,
    pub hotkey: String,
    pub quick_hotkey: String,
    pub correction_hotkey: String,
    pub quick_correction_hotkey: String,
    pub correction_style: CorrectionStyle,
    pub quick_correction_style: CorrectionStyle,
    pub launch_at_startup: bool,
    pub theme: ThemePreference,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4.1-mini".into(),
            source_language: AUTO.into(),
            target_language: "Anglais".into(),
            hotkey: "Ctrl+F12".into(),
            quick_hotkey: "Ctrl+Shift+F12".into(),
            correction_hotkey: "Ctrl+F11".into(),
            quick_correction_hotkey: "Ctrl+Shift+F11".into(),
            correction_style: CorrectionStyle::Faithful,
            quick_correction_style: CorrectionStyle::Faithful,
            launch_at_startup: false,
            theme: ThemePreference::System,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        let url = reqwest::Url::parse(&self.base_url).context("URL du provider invalide")?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            bail!("L’URL doit commencer par http:// ou https://.");
        }
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            bail!("L’URL ne doit contenir ni identifiants, ni paramètres, ni fragment.");
        }
        if self.model.trim().is_empty() {
            bail!("Renseigne le nom du modèle.");
        }
        if self.source_language.trim().is_empty()
            || self.target_language.trim().is_empty()
            || self.target_language == AUTO
        {
            bail!("Choisis une langue source et une langue cible explicite.");
        }
        Ok(())
    }

    pub fn validate_hotkeys(&self) -> Result<()> {
        crate::platform::hotkey::parse_shortcuts(self.shortcuts())?;
        Ok(())
    }

    pub fn shortcuts(&self) -> [&str; 4] {
        [
            &self.hotkey,
            &self.quick_hotkey,
            &self.correction_hotkey,
            &self.quick_correction_hotkey,
        ]
    }

    pub fn endpoint(&self) -> Result<reqwest::Url> {
        self.validate()?;
        Ok(reqwest::Url::parse(&format!(
            "{}/chat/completions",
            self.base_url.trim_end_matches('/')
        ))?)
    }
}

pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new() -> Result<Self> {
        let dirs = ProjectDirs::from("dev", "TranslationTool", "TranslationTool")
            .context("Impossible de trouver le dossier de configuration Windows")?;
        Ok(Self {
            path: dirs.config_dir().join("settings.json"),
        })
    }

    pub fn load(&self) -> Result<Option<Settings>> {
        match fs::read(&self.path) {
            Ok(bytes) => Ok(Some(
                serde_json::from_slice(&bytes).context("Configuration illisible")?,
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).context("Impossible de lire la configuration"),
        }
    }

    pub fn save(&self, settings: &Settings) -> Result<()> {
        settings.validate()?;
        settings.validate_hotkeys()?;
        self.write(settings)
    }

    pub fn save_theme(&self, saved: &Settings, theme: ThemePreference) -> Result<Settings> {
        let mut next = saved.clone();
        next.theme = theme;
        // Appearance must remain editable while repairing an invalid provider or shortcut.
        self.write(&next)?;
        Ok(next)
    }

    fn write(&self, settings: &Settings) -> Result<()> {
        fs::create_dir_all(
            self.path
                .parent()
                .context("Dossier de configuration absent")?,
        )?;
        // Keep a valid previous file if writing the new one fails.
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(settings)?)?;
        fs::rename(&temporary, &self.path).context("Impossible d’enregistrer la configuration")
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

// Use a separate credential per endpoint, so switching providers cannot reuse an OpenAI key.
fn credential(base_url: &str) -> Result<keyring::Entry> {
    keyring::Entry::new("translation-tool", normalize_endpoint(base_url))
        .context("Accès au gestionnaire d’identifiants impossible")
}

pub fn normalize_endpoint(base_url: &str) -> &str {
    base_url.trim().trim_end_matches('/')
}

pub fn load_api_key(base_url: &str) -> Result<String> {
    match credential(base_url)?.get_password() {
        Ok(key) => Ok(key),
        Err(keyring::Error::NoEntry) => Ok(String::new()),
        Err(e) => Err(e).context("Impossible de lire la clé API"),
    }
}

pub fn save_api_key(base_url: &str, key: &str) -> Result<()> {
    let entry = credential(base_url)?;
    if key.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    } else {
        entry
            .set_password(key)
            .context("Impossible d’enregistrer la clé API")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_only_save_preserves_settings_awaiting_repair() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore {
            path: dir.path().join("settings.json"),
        };
        let saved = Settings {
            model: String::new(),
            hotkey: "invalid".into(),
            ..Settings::default()
        };
        assert!(store.save(&saved).is_err());
        let next = store.save_theme(&saved, ThemePreference::Dark).unwrap();
        assert_eq!(
            next,
            Settings {
                theme: ThemePreference::Dark,
                ..saved.clone()
            }
        );
        assert_eq!(store.load().unwrap().unwrap(), next);
        assert_eq!(saved.theme, ThemePreference::System);
    }

    #[test]
    fn configuration_round_trip_and_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore {
            path: dir.path().join("nested/settings.json"),
        };
        assert!(store.load().unwrap().is_none());
        let mut settings = Settings::default();
        store.save(&settings).unwrap();
        settings.source_language = "Français".into();
        settings.launch_at_startup = true;
        settings.theme = ThemePreference::Dark;
        store.save(&settings).unwrap();
        assert_eq!(store.load().unwrap().unwrap(), settings);
        assert!(
            !fs::read_to_string(store.path())
                .unwrap()
                .contains("api_key")
        );
    }

    #[test]
    fn endpoint_validation_preserves_local_prefix() {
        let mut settings = Settings {
            base_url: "http://localhost:1234/v1/".into(),
            ..Settings::default()
        };
        assert_eq!(
            settings.endpoint().unwrap().as_str(),
            "http://localhost:1234/v1/chat/completions"
        );
        for invalid in [
            "file:///tmp",
            "https://user:secret@example.com/v1",
            "https://example.com?key=secret",
        ] {
            settings.base_url = invalid.into();
            assert!(settings.validate().is_err());
        }
    }

    #[test]
    fn credential_endpoint_normalization_matches_settings_input() {
        assert_eq!(
            normalize_endpoint(" https://api.openai.com/v1/ \n"),
            "https://api.openai.com/v1"
        );
    }

    #[test]
    fn old_configuration_gets_a_quick_shortcut_and_preserves_languages() {
        let settings: Settings = serde_json::from_str(
            r#"{"hotkey":"Ctrl+Alt+KeyY","source_language":"Allemand","target_language":"Français"}"#,
        ).unwrap();
        assert_eq!(settings.quick_hotkey, "Ctrl+Shift+F12");
        assert_eq!(settings.correction_hotkey, "Ctrl+F11");
        assert_eq!(settings.quick_correction_hotkey, "Ctrl+Shift+F11");
        assert_eq!(settings.correction_style, CorrectionStyle::Faithful);
        assert_eq!(settings.quick_correction_style, CorrectionStyle::Faithful);
        assert!(!settings.launch_at_startup);
        assert_eq!(settings.theme, ThemePreference::System);
        assert_eq!(settings.hotkey, "Ctrl+Alt+KeyY");
        assert_eq!(settings.source_language, "Allemand");
        assert_eq!(settings.target_language, "Français");
        settings.validate_hotkeys().unwrap();
    }

    #[test]
    fn shortcuts_must_be_valid_and_distinct() {
        let mut settings = Settings {
            quick_hotkey: "Ctrl+F12".into(),
            ..Settings::default()
        };
        assert!(settings.validate_hotkeys().is_err());
        settings.quick_hotkey = "Shift+KeyQ".into();
        assert!(settings.validate_hotkeys().is_err());
        settings.quick_hotkey = "Ctrl+Alt+KeyY".into();
        settings.validate_hotkeys().unwrap();
    }

    #[test]
    fn existing_translation_shortcuts_and_independent_styles_are_preserved() {
        let settings: Settings = serde_json::from_str(r#"{"hotkey":"Ctrl+Alt+KeyT","quick_hotkey":"Ctrl+Alt+KeyQ","correction_style":"Professional","quick_correction_style":"Concise"}"#).unwrap();
        assert_eq!(settings.hotkey, "Ctrl+Alt+KeyT");
        assert_eq!(settings.quick_hotkey, "Ctrl+Alt+KeyQ");
        assert_eq!(settings.correction_style, CorrectionStyle::Professional);
        assert_eq!(settings.quick_correction_style, CorrectionStyle::Concise);
        settings.validate_hotkeys().unwrap();
        assert_eq!(
            serde_json::from_str::<Settings>(&serde_json::to_string(&settings).unwrap()).unwrap(),
            settings
        );
    }
}
