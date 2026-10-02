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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub base_url: String,
    pub model: String,
    pub source_language: String,
    pub target_language: String,
    pub hotkey: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4.1-mini".into(),
            source_language: AUTO.into(),
            target_language: "Anglais".into(),
            hotkey: "Ctrl+Alt+KeyT".into(),
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
    fn configuration_round_trip_and_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore {
            path: dir.path().join("nested/settings.json"),
        };
        assert!(store.load().unwrap().is_none());
        let mut settings = Settings::default();
        store.save(&settings).unwrap();
        settings.source_language = "Français".into();
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
}
