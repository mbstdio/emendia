# Emendia

**Translate. Proofread. Refine.**

Emendia is a native Windows desktop application that translates and improves selected text in the application you are already using. Built with **Rust and GPUI**, it runs in the notification area and works with cloud or local AI providers through an OpenAI-compatible API.

Select text → press a shortcut → review and edit the suggestion → **Replace** or **Copy**.

## Features

- **Translation** with automatic or explicit source language detection and a configurable target language.
- **Proofreading** in the original language, with five styles: Faithful correction, More fluent, Professional, Casual and Concise.
- **Quick Translate** and **Quick Check** for direct replacement without a review step.
- Editable previews near your selection, with alternative suggestions, original-text visibility, copying and replacement.
- Four configurable global shortcuts, with key capture and conflict detection.
- Non-activating status popups for capture, processing, replacement, success and errors.
- **English and French interfaces**, with an immediately saved language preference. System mode uses French when the Windows UI language is French and English otherwise.
- **Light, Dark and System themes**, applied and saved immediately across windows.
- OpenAI, LM Studio, Ollama and custom OpenAI-compatible providers.
- Local JSON settings and endpoint-specific API keys in Windows Credential Manager.
- Optional launch at Windows sign-in, disabled by default.
- DPI-aware placement within the monitor's usable area, with cursor fallback when selection coordinates are unavailable.

## Getting started

Build Emendia using the [Windows build instructions](#building-on-windows), then run **`emendia.exe`**. This version provides a standalone executable; there is no installer yet.

1. On first launch, **Settings** opens automatically. Later launches stay in the tray; click the tray icon or use its **Settings** menu item.
2. In **AI Provider**, choose your provider and enter the base URL, exact model name and API key if required.
3. Click **Test connection**, then **Save**.
4. In **Translation**, select the default source and target languages. In **Proofreading**, choose independent default styles for preview and Quick Check.
5. Select text in an editor or text field, press a shortcut, then **release its keys**.

### Default shortcuts

| Action | Shortcut |
| --- | --- |
| Translate with preview | **Ctrl+F12** |
| Quick Translate | **Ctrl+Shift+F12** |
| Proofread with preview | **Ctrl+F11** |
| Quick Check | **Ctrl+Shift+F11** |

Shortcuts can be changed in **Translation**, **Proofreading** or the centralized **Shortcuts** page. These pages share the same values. Save to activate changes; all four shortcuts must be distinct. If a new shortcut is already reserved, the previous shortcuts remain active. **Shortcuts enabled** in the tray menu toggles all four together.

### Providers

| Provider | Base URL | Model |
| --- | --- | --- |
| OpenAI | `https://api.openai.com/v1` | Default: `gpt-4.1-mini`; use a model available to your API account |
| LM Studio | `http://localhost:1234/v1` | The identifier of the model loaded in LM Studio |
| Ollama | `http://localhost:11434/v1` | The exact installed model name, such as `llama3.2` |
| Custom | Your compatible API's base URL, including its prefix | The identifier expected by your server |

The URL must be a **base URL**, without `/chat/completions` at the end. For local providers, start the HTTP service and load the model before testing. An API key is optional if your server does not require one.

**Test connection** makes a small real translation request using the current form values, without saving them. OpenAI charges according to your model and API account; a ChatGPT subscription does not provide API access or credits.

Provider presets supply an example URL and model; both remain editable. Changing the URL reloads the key associated with that endpoint. Clear the key and save to remove that endpoint's credential.

## Using Emendia

### Preview

The capture popup appears first, then the preview takes over. You can change translation languages or proofreading style, edit the result directly, or request a **New suggestion**. Language and style changes in the preview apply only to that session.

**Replace** returns to the original selection and attempts to paste the result. **Copy** puts it on the clipboard for manual pasting. **Cancel**, Escape or closing the preview cancels pending processing without replacing the text. Changing language or style cancels the previous request; stale responses cannot overwrite a newer result. The preview cannot be closed during the brief replacement operation.

### Quick actions

Quick Translate and Quick Check use a snapshot of your saved settings and replace the selection automatically. Stay in the original document with the same selection while processing. Status popups keep you informed without taking focus.

A success message closes after two seconds. An error popup offers **Settings** and **Close**, and closes automatically after five seconds. Long messages wrap and can be scrolled. If replacement fails, the existing result opens in a recovery preview so you can copy it or attempt manual replacement without another AI request. That preview remains after the error popup closes.

New shortcuts are ignored during capture and quick processing. When a preview is already open, any of the four shortcuts brings it to the foreground.

### Proofreading styles

| Style | Behavior |
| --- | --- |
| Faithful correction | Fix spelling, grammar and punctuation while preserving tone, register and wording; leave correct passages unchanged |
| More fluent | Light rewriting for smoother flow and readability, preserving the original tone |
| Professional | Polished wording suitable for workplace communication |
| Casual | Natural, informal wording |
| Concise | Shorter wording without losing essential information |

All styles ask the model to preserve meaning, paragraphs and formatting without inventing information. Proofreading detects the original language and does not translate. Results depend on the configured model. Preview and Quick Check have independent default styles; both start with Faithful correction.

### Appearance, language and startup

In **General**, select **Light / Dark / System** and **System / English / Français**. Both preferences take effect immediately and are saved independently of other form edits. System theme follows Windows appearance changes during execution. Interface language does not affect translation targets or the language of proofreading results.

Enable **Launch at Windows startup** and save to start Emendia in the tray at sign-in for the current user, without administrator rights. Disable it and save to remove the registration. This uses the `Emendia` value in `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`, pointing to the current executable. After moving the executable, save again from its new location. Enable startup from a release build to avoid a console window.

Closing every window leaves Emendia running in the tray. Choose **Quit** in the tray menu to exit.

## Data and configuration

- Selected text and, for alternative suggestions, the previous result are sent to **your configured provider**. Local-only processing is possible with a locally hosted model; actual handling depends on that provider.
- Settings are stored at `%APPDATA%\Emendia\Emendia\config\settings.json`.
- API keys are stored separately in **Windows Credential Manager**, under the `emendia` service, with one credential per normalized endpoint.
- Selected text, generated results and API keys are not written to the settings JSON. Diagnostic logs do not include selected text, keys or raw provider error bodies.
- After **Copy** or successful **Replace**, the result remains in the clipboard.

### Upgrading from Translation Tool

If Emendia has no configuration yet, it imports the old settings from `%APPDATA%\TranslationTool\TranslationTool\config\settings.json`, preserving shortcuts, languages, styles and theme. Existing users retain a French interface; new installations default to System. The legacy file remains available, and an existing Emendia configuration takes precedence.

Legacy endpoint credentials are migrated when accessed. An existing `TranslationTool` startup registration is migrated to `Emendia`, using the current executable, to avoid duplicate startup entries. Replacing or disabling startup registration restores the previous entries if saving fails.

## Integration and limitations

Capture and replacement use the standard **Ctrl+C / Ctrl+V** conventions and Windows UI Automation when available. Before replacement, Emendia checks the original window, process, document title, active control and selected text. Where accessible, it also verifies the control identifier and selection position. CR, LF and CRLF line endings are treated as equivalent while other characters and paragraph counts remain checked.

Capture saves clipboard formats including Unicode text, HTML, RTF and bitmap, and restores them only if the clipboard has not changed meanwhile. If a private format cannot be preserved, Emendia tries reading the selection through UI Automation; otherwise capture stops without modifying the clipboard.

Replacement is best-effort: some applications lose their selection after focus changes, do not expose accessible controls, or use different shortcuts. Read-only fields and applications running with elevated privileges may reject replacement. Use **Copy** in these cases. Replacement inserts **plain text**; rich document formatting is not preserved.

## Building on Windows

### Requirements

- [Rustup](https://rustup.rs/) with **Rust 1.99 or newer**. `rust-toolchain.toml` selects stable and `Cargo.lock` pins dependency versions.
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with **Desktop development with C++**, MSVC x64/x86, a recent Windows SDK and CMake for Windows.
- Graphics drivers capable of GPUI's GPU rendering.

```powershell
rustup update stable
rustup component add rustfmt clippy
```

Use **Developer PowerShell for VS** if Cargo cannot find the Microsoft linker. A `link.exe` from Git/MSYS is not the MSVC linker. If Build Tools' CMake is missing from `PATH`, add its `bin` directory through Windows environment settings. Spectre libraries may be needed depending on dependency and Build Tools versions; install them through Visual Studio Installer if the linker requests them.

### Run and build

From the repository root:

```powershell
cargo run --locked
cargo build --release --locked
```

The release executable is **`target/release/emendia.exe`**, built without a console window. The initial build takes longer because GPUI has many dependencies. No Node.js, embedded browser or web server is required to build the application.

For a temporary interface-language override, use `--ui-language=en` or `--ui-language=fr`. This does not save the preference unless you change it in Settings.

## Development

The project's source language is **English**. French UI translations live in `src/i18n.rs`. New languages can be added to `UiLanguage` and the localization catalog. Translation-language values are stable English names; localized labels and legacy French names are converted independently from the interface language. Proofreading prompts always use English style names and instructions.

### Project layout

```text
src/
  main.rs                  Startup, runtime and application lifetime
  app.rs                   Window, tray, shortcut and capture coordination
  i18n.rs                  Interface language preference and translation catalog
  settings.rs              JSON configuration, migration and Windows credentials
  translation.rs           OpenAI-compatible client and translation/proofreading prompts
  smoke.rs                 Local mock provider for graphical smoke tests
  ui/
    settings.rs            Settings categories and shortcut capture
    preview.rs             Editable preview and cancellable tasks
    status.rs              Non-activating status popup
    theme.rs               System theme and native title bar synchronization
    mod.rs                 Shared controls and localized language selectors
  platform/
    windows.rs             Capture, identity, placement, clipboard and replacement
    hotkey.rs              Transactional registration of four global shortcuts
    startup.rs             Windows sign-in registration and migration
    tray.rs                Notification icon and menu
```

The application uses GPUI Kit 0.7 for native views, Reqwest and Tokio for background HTTP requests, Serde for JSON, `global-hotkey` and `tray-icon` for Windows integration, `windows` for Win32/UI Automation, `keyring` for credentials, and `tracing` for diagnostics. Clipboard operations are serialized; unsafe code is confined to platform integration.

### Verification

```powershell
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked

foreach ($language in 'en', 'fr') {
    cargo run --locked -- --smoke-test "--ui-language=$language"
    cargo run --locked -- --smoke-test-quick "--ui-language=$language"
    cargo run --locked -- --smoke-test-correction "--ui-language=$language"
    cargo run --locked -- --smoke-test-quick-check "--ui-language=$language"
}
```

HTTP tests use a real mock server on localhost and require no API key or external provider. Smoke tests open settings, a preview with sample text and the tray, then quit after three seconds, or seven seconds for quick flows. They verify provider results, compact preview layout, non-activating status behavior, wrapping and automatic error-popup closure. Quick flows also verify recovery after a simulated paste failure without making another request. These diagnostics do not have a real replacement destination or save form settings.

An additional Windows test launches an isolated native editor and checks capture, paste, focus preservation and clipboard restoration for text and bitmap. It requires an interactive desktop and temporarily uses focus and the clipboard; do not interact with another application during the test:

```powershell
cargo test --locked --lib clipboard_and_native_edit_round_trip -- --ignored --nocapture --test-threads=1
```

Manual checks should cover language changes with another window open, persistence after restart, unsaved form edits during theme/language changes, all five proofreading styles, shortcut conflicts, startup registration, changed selections during processing, offline providers, multi-monitor placement and 125–150% DPI.

## License

Emendia is licensed under the [MIT License](LICENSE).

## References

- [GPUI](https://www.gpui.rs/)
- [GPUI Kit](https://github.com/longbridge/gpui-kit)
- [Zed's Windows build guide](https://github.com/zed-industries/zed/blob/main/docs/src/development/windows.md)
- [Windows SendInput documentation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)
- [OleDuplicateData](https://learn.microsoft.com/en-us/windows/win32/api/ole2/nf-ole2-oleduplicatedata)
