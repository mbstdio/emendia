# Development

[Back to README](../README.md)

## Windows build environment

- [Rustup](https://rustup.rs/) with **Rust 1.99 or newer**. `rust-toolchain.toml` selects stable and `Cargo.lock` pins dependencies.
- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with **Desktop development with C++**, MSVC x64/x86, a recent Windows SDK and CMake for Windows.
- Graphics drivers capable of GPUI's GPU rendering.

```powershell
rustup update stable
rustup component add rustfmt clippy
```

Use **Developer PowerShell for VS** if Cargo cannot find the Microsoft linker. A `link.exe` from Git/MSYS is not the MSVC linker. If Build Tools' CMake is missing from `PATH`, add its `bin` directory through Windows environment settings. Install Spectre libraries through Visual Studio Installer if the linker requests them.

From the repository root:

```powershell
cargo run --locked
cargo build --release --locked
```

The release executable is `target/release/emendia.exe`, built without a console window. The initial build takes longer because GPUI has many dependencies. No Node.js, embedded browser or web server is required.

For a temporary interface-language override, use `--ui-language=en` or `--ui-language=fr`. This does not save the preference unless you change it in Settings.

## Source conventions

The project's source language is **English**. French UI translations live in `src/i18n.rs`. Add languages to `UiLanguage` and the localization catalog. Translation-language values are stable English names; localized labels and legacy French names are converted independently from the interface language. Proofreading prompts use English style names and instructions.

## Project layout

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

Emendia uses GPUI Kit 0.7 for native views, Reqwest and Tokio for background HTTP requests, Serde for JSON, `global-hotkey` and `tray-icon` for Windows integration, `windows` for Win32/UI Automation, `keyring` for credentials, and `tracing` for diagnostics. Clipboard operations are serialized; unsafe code is confined to platform integration. Setup images are embedded in the executable.

## Verification

```powershell
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked

foreach ($language in 'en', 'fr') {
    cargo run --locked -- --smoke-test-onboarding "--ui-language=$language"
    cargo run --locked -- --smoke-test-onboarding "--ui-language=$language" --smoke-theme=dark
    cargo run --locked -- --smoke-test "--ui-language=$language"
    cargo run --locked -- --smoke-test-quick "--ui-language=$language"
    cargo run --locked -- --smoke-test-correction "--ui-language=$language"
    cargo run --locked -- --smoke-test-quick-check "--ui-language=$language"
}
```

HTTP tests use a real mock server on localhost and require no API key or external provider.

Onboarding smoke diagnostics verify that only the setup assistant opens, walk through all five steps, then return to welcome without saving configuration. They also verify that closing incomplete setup terminates the application.

Other smoke tests open settings, a preview with sample text and the tray, then quit after three seconds, or seven seconds for quick flows. They verify provider results, compact preview layout, non-activating status behavior, wrapping and automatic error-popup closure. Quick flows also verify recovery after a simulated paste failure without another request. These diagnostics do not have a real replacement destination or save form settings.

An additional Windows test launches an isolated native editor and checks capture, paste, focus preservation and clipboard restoration for text and bitmap. It requires an interactive desktop and temporarily uses focus and the clipboard; do not interact with another application during the test:

```powershell
cargo test --locked --lib clipboard_and_native_edit_round_trip -- --ignored --nocapture --test-threads=1
```

Manual checks should cover language changes with another window open, persistence after restart, unsaved form edits during theme/language changes, all five proofreading styles, shortcut conflicts, startup registration, changed selections during processing, offline providers, multi-monitor placement and 125–150% DPI.

## References

- [GPUI](https://www.gpui.rs/)
- [GPUI Kit](https://github.com/longbridge/gpui-kit)
- [Zed's Windows build guide](https://github.com/zed-industries/zed/blob/main/docs/src/development/windows.md)
- [Windows SendInput documentation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)
- [OleDuplicateData](https://learn.microsoft.com/en-us/windows/win32/api/ole2/nf-ole2-oleduplicatedata)
