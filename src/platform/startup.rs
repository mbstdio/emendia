use anyhow::{Context, Result};
use std::{os::windows::ffi::OsStrExt, path::Path};
use windows::{
    Win32::{
        Foundation::ERROR_FILE_NOT_FOUND,
        System::Registry::{
            HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW,
            RegSetKeyValueW,
        },
    },
    core::{HSTRING, w},
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

// Apply only on Save, so launching the application never overrides a user's
// choice in Windows Startup Apps. Restore the exact previous entry if saving fails.
pub fn configure(enabled: bool, persist: impl FnOnce() -> Result<()>) -> Result<()> {
    let command = if enabled {
        Some(startup_command(
            &std::env::current_exe().context("Impossible de trouver l’exécutable")?,
        ))
    } else {
        None
    };
    configure_at(RUN_KEY, command.as_deref(), persist)
}

fn startup_command(executable: &Path) -> Vec<u16> {
    // Preserve Unicode paths and quote spaces for Windows command-line parsing.
    std::iter::once('"' as u16)
        .chain(executable.as_os_str().encode_wide())
        .chain(['"' as u16, 0])
        .collect()
}

fn configure_at(
    subkey: &str,
    command: Option<&[u16]>,
    persist: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let subkey = HSTRING::from(subkey);
    let previous = read_entry(&subkey)?;
    write_entry(&subkey, command)?;
    if let Err(error) = persist() {
        if let Err(rollback) = write_entry(&subkey, previous.as_deref()) {
            return Err(error.context(format!(
                "Impossible de restaurer le démarrage automatique : {rollback:#}"
            )));
        }
        return Err(error);
    }
    Ok(())
}

fn read_entry(subkey: &HSTRING) -> Result<Option<Vec<u16>>> {
    let mut bytes = 0;
    // SAFETY: The first call queries size only. The second writes at most the
    // allocated byte count to a live, aligned UTF-16 buffer.
    unsafe {
        let status = RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            w!("TranslationTool"),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut bytes),
        );
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        status
            .ok()
            .context("Impossible de lire le démarrage automatique Windows")?;
        let mut value = vec![0u16; (bytes as usize).div_ceil(2)];
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            w!("TranslationTool"),
            RRF_RT_REG_SZ,
            None,
            Some(value.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
        .ok()
        .context("Impossible de lire le démarrage automatique Windows")?;
        value.truncate((bytes as usize).div_ceil(2));
        Ok(Some(value))
    }
}

fn write_entry(subkey: &HSTRING, command: Option<&[u16]>) -> Result<()> {
    // SAFETY: Strings are null-terminated; command points to a live UTF-16
    // buffer of the supplied size. Only our value in the current user is changed.
    let status = unsafe {
        match command {
            Some(command) => RegSetKeyValueW(
                HKEY_CURRENT_USER,
                subkey,
                w!("TranslationTool"),
                REG_SZ.0,
                Some(command.as_ptr().cast()),
                std::mem::size_of_val(command) as u32,
            ),
            None => RegDeleteKeyValueW(HKEY_CURRENT_USER, subkey, w!("TranslationTool")),
        }
    };
    if command.is_none() && status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    status
        .ok()
        .context("Impossible de modifier le démarrage automatique Windows")
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Registry::RegDeleteKeyW;

    #[test]
    fn registration_enable_disable_and_failed_save_restore_previous_entry() {
        // Exercise the real registry API in an isolated key, never the Run key.
        let unique = tempfile::tempdir().unwrap();
        let key = format!(
            r"Software\TranslationToolTests\{}",
            unique.path().file_name().unwrap().to_string_lossy()
        );
        let subkey = HSTRING::from(key.as_str());
        let command = startup_command(Path::new(r"C:\Program Files\Traduction été\tool.exe"));
        assert_eq!(
            String::from_utf16(&command[..command.len() - 1]).unwrap(),
            "\"C:\\Program Files\\Traduction été\\tool.exe\""
        );
        assert!(read_entry(&subkey).unwrap().is_none());
        configure_at(&key, Some(&command), || anyhow::bail!("save failed")).unwrap_err();
        assert!(read_entry(&subkey).unwrap().is_none());
        configure_at(&key, Some(&command), || Ok(())).unwrap();
        assert_eq!(read_entry(&subkey).unwrap(), Some(command.clone()));
        configure_at(&key, None, || anyhow::bail!("save failed")).unwrap_err();
        assert_eq!(read_entry(&subkey).unwrap(), Some(command));
        configure_at(&key, None, || Ok(())).unwrap();
        configure_at(&key, None, || Ok(())).unwrap();
        assert!(read_entry(&subkey).unwrap().is_none());
        // SAFETY: Delete only the unique, empty test key created above.
        unsafe {
            RegDeleteKeyW(HKEY_CURRENT_USER, &subkey).ok().unwrap();
        }
    }
}
