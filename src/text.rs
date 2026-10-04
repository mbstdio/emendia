use anyhow::{Result, bail};

pub(crate) const MAX_TEXT_UNITS: usize = 100_000;

/// Validate before allocating clipboard formats or touching a replacement destination.
pub(crate) fn validate_clipboard_text(text: &str) -> Result<()> {
    if text.contains('\0') || text.encode_utf16().take(MAX_TEXT_UNITS + 1).count() > MAX_TEXT_UNITS
    {
        bail!(crate::i18n::t(
            "The text contains a null character or exceeds 100,000 UTF-16 code units."
        ));
    }
    Ok(())
}
