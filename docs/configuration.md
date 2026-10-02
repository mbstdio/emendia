# Configuration and limitations

[Back to README](../README.md)

## Data and credentials

- Selected text and, for alternative suggestions, the previous result are sent to **your configured provider**. Local-only processing is possible with a locally hosted model; actual handling depends on that provider.
- Settings are stored at `%APPDATA%\Emendia\Emendia\config\settings.json`.
- API keys are stored separately in **Windows Credential Manager**, under the `emendia` service, with one credential per normalized endpoint.
- Selected text, generated results and API keys are not written to the settings JSON. Diagnostic logs do not include selected text, keys or raw provider error bodies.
- After **Copy** or successful **Replace**, the result remains in the clipboard.

## Windows startup

Startup uses the `Emendia` value in `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`, pointing to the current executable. Disabling startup and saving removes this registration. After moving the executable, save again from its new location.

## Upgrading from Translation Tool

If Emendia has no configuration yet, it imports `%APPDATA%\TranslationTool\TranslationTool\config\settings.json`, preserving shortcuts, languages, styles and theme. Existing users retain a French interface; new installations default to System. The legacy file remains available, and an existing Emendia configuration takes precedence.

Legacy endpoint credentials are migrated when accessed. An existing `TranslationTool` startup registration is migrated to `Emendia`, using the current executable, to avoid duplicate entries. Replacing or disabling startup registration restores the previous entries if saving fails.

## Capture and replacement

Capture and replacement use standard **Ctrl+C / Ctrl+V** conventions and Windows UI Automation when available. Before replacement, Emendia checks the original window, process, document title, active control and selected text. Where accessible, it also verifies the control identifier and selection position. CR, LF and CRLF line endings are treated as equivalent; other characters and paragraph counts remain checked.

Capture saves clipboard formats including Unicode text, HTML, RTF and bitmap, and restores them only if the clipboard has not changed meanwhile. If a private format cannot be preserved, Emendia tries reading the selection through UI Automation; otherwise capture stops without modifying the clipboard.

Replacement is best-effort: some applications lose their selection after focus changes, do not expose accessible controls, or use different shortcuts. Read-only fields and applications running with elevated privileges may reject replacement. Use **Copy** in these cases.

Replacement inserts **plain text**; rich document formatting is not preserved. Popups are placed within the monitor's usable area, with cursor fallback when selection coordinates are unavailable.
