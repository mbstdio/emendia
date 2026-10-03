use anyhow::{Context, Result, bail};
use directories::BaseDirs;
use std::{fs, path::Path};

pub fn configure(enabled: bool, persist: impl FnOnce() -> Result<()>) -> Result<()> {
    let dirs = BaseDirs::new().context("Unable to find the configuration directory")?;
    let path = dirs.config_dir().join("autostart/emendia.desktop");
    let executable = std::env::current_exe().context("Unable to locate the executable")?;
    configure_at(&path, enabled.then_some(executable.as_path()), persist)
}

pub fn migrate_legacy() -> Result<()> {
    Ok(())
}

fn desktop_entry(executable: &Path) -> Result<String> {
    let path = executable
        .to_str()
        .context("The executable path must be valid UTF-8")?;
    if path.contains(['\n', '\r', '\0']) {
        bail!("The executable path contains an unsupported control character");
    }
    // Exec has its own quoting rules, followed by Desktop Entry string escaping.
    // Percent signs must also be escaped so they cannot introduce field codes.
    let quoted = path
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    let exec = format!("\"{quoted}\"").replace('\\', "\\\\");
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=Emendia\nComment=Translation and proofreading\nExec={exec}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
    ))
}

fn configure_at(
    path: &Path,
    executable: Option<&Path>,
    persist: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let previous = match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let next = executable.map(desktop_entry).transpose()?;
    write_entry(path, next.as_deref().map(str::as_bytes))?;
    if let Err(error) = persist() {
        write_entry(path, previous.as_deref()).context("Unable to restore startup registration")?;
        return Err(error);
    }
    Ok(())
}

fn write_entry(path: &Path, bytes: Option<&[u8]>) -> Result<()> {
    if let Some(bytes) = bytes {
        fs::create_dir_all(path.parent().context("Missing autostart directory")?)?;
        let temporary = path.with_extension("desktop.tmp");
        fs::write(&temporary, bytes)?;
        fs::rename(temporary, path)?;
    } else {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_registration_rolls_back_failed_saves() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("autostart/emendia.desktop");
        let executable = Path::new("/home/user/Applications/Traduction été/emendia");
        configure_at(&path, Some(executable), || anyhow::bail!("save failed")).unwrap_err();
        assert!(!path.exists());
        configure_at(&path, Some(executable), || Ok(())).unwrap();
        let previous = fs::read(&path).unwrap();
        configure_at(&path, None, || anyhow::bail!("save failed")).unwrap_err();
        assert_eq!(fs::read(&path).unwrap(), previous);
        configure_at(&path, None, || Ok(())).unwrap();
        configure_at(&path, None, || Ok(())).unwrap();
    }

    #[test]
    fn exec_quotes_spaces_unicode_and_special_characters() {
        let entry = desktop_entry(Path::new("/tmp/été $HOME `app` \\\"100%/emendia")).unwrap();
        assert!(
            entry.contains(
                "Exec=\"/tmp/été \\\\$HOME \\\\`app\\\\` \\\\\\\\\\\\\"100%%/emendia\"\n"
            )
        );
        assert!(desktop_entry(Path::new("/tmp/app\nName=Other")).is_err());
    }
}
