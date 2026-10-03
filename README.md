![Emendia](src/ressources/onboard-hero.jpg)

# Emendia

**Translate. Proofread. Refine.**

Emendia is a native Windows application that translates and improves selected text in the application you are already using. Built with Rust and GPUI, it runs in the notification area and supports OpenAI, LM Studio, Ollama and other OpenAI-compatible providers.

Select text → press a shortcut → review the suggestion → **Replace** or **Copy**.

## Getting started

1. Download a Windows x64 build from [Releases](https://github.com/mbstdio/emendia/releases):
   - **Portable:** run `Emendia-<version>-windows-x64.exe` directly, without installation.
   - **Installer:** run `Emendia-<version>-windows-x64-setup.exe` to install for your Windows user, with shortcuts and an uninstaller. No administrator rights are required.
   - Alternatively, [build Emendia](#building-on-windows) and run `target/release/emendia.exe`.
2. Follow the setup assistant to choose your AI provider, model, languages and shortcuts.
3. Select text in an editor or text field, press a shortcut, then release its keys.

Click the tray icon to open Settings. Closing windows keeps Emendia running; choose **Quit** in the tray menu to exit.

Release builds require Windows 10 version 1903 or newer (x64) and compatible graphics drivers. Both downloads use the same executable. Portable builds still store settings in your Windows user profile and API keys in Windows Credential Manager. `SHA256SUMS.txt` is available with each release to verify downloads.

## Default shortcuts

| Action | Shortcut |
| --- | --- |
| Translate with preview | **Ctrl+F12** |
| Quick Translate | **Ctrl+Shift+F12** |
| Proofread with preview | **Ctrl+F11** |
| Quick Check | **Ctrl+Shift+F11** |

Preview actions let you edit the result before replacing or copying it. Quick actions replace the selection automatically. All shortcuts are configurable.

## Good to know

- Selected text is sent to your configured provider. Use a locally hosted model for local processing.
- API keys are stored in Windows Credential Manager, separately from settings.
- Replacement inserts plain text and may not work in every application. Use **Copy** for manual pasting when needed.
- The interface supports English and French, with Light, Dark and System themes.

## Building on Windows

Install [Rustup](https://rustup.rs/) with Rust **1.99 or newer**, and [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with **Desktop development with C++**, MSVC, a recent Windows SDK and CMake. Graphics drivers must support GPUI's GPU rendering.

From the repository root:

```powershell
cargo build --release --locked
```

Run `target/release/emendia.exe`. See the [development guide](docs/development.md) for setup details, troubleshooting and tests.

## Documentation

- [Usage](docs/usage.md): providers, previews, proofreading styles and preferences.
- [Configuration and limitations](docs/configuration.md): data storage, privacy, migration and application integration.
- [Development](docs/development.md): build environment, project layout and verification.
- [Releases](docs/releasing.md): automated builds, portable and installer downloads, release notes and version tags.

## Roadmap

- [ ] Linux build
- [ ] Mac build? `¯\_(ツ)_/¯`
- [ ] Better model selection
- [ ] Overall interface improvements

## License

[MIT](LICENSE)
