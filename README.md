![Emendia](src/ressources/onboard-hero.jpg)

# Emendia

**Translate. Proofread. Refine.**

Emendia is a native Windows and Linux application that translates and improves selected text in the application you are already using. Built with Rust and GPUI, it runs in the notification area and supports OpenAI, LM Studio, Ollama and other OpenAI-compatible providers. Linux supports X11 and an experimental Wayland capture-and-copy workflow.

Select text → press a shortcut → review the suggestion → **Replace** or **Copy**.

## Getting started

1. Download a Windows x64 build from [Releases](https://github.com/mbstdio/emendia/releases):
   - **Portable:** run `Emendia-<version>-windows-x64.exe` directly, without installation.
   - **Installer:** run `Emendia-<version>-windows-x64-setup.exe` to install for your Windows user, with shortcuts and an uninstaller. No administrator rights are required.
   - Alternatively, [build Emendia](#building-on-windows) and run `target/release/emendia.exe`.
2. Follow the setup assistant to choose your AI provider, model, languages and shortcuts.
3. Select text in an editor or text field, press a shortcut, then release its keys.

Click the tray icon to open Settings. Closing windows keeps Emendia running; choose **Quit** in the tray menu to exit.

Windows release builds require Windows 10 version 1903 or newer (x64) and compatible graphics drivers. Both Windows downloads use the same executable. Portable builds still store settings in your Windows user profile and API keys in Windows Credential Manager. `SHA256SUMS.txt` is available with each release to verify downloads.

### Linux x64

Download `Emendia-<version>-linux-x64.tar.gz` from Releases, extract it into a permanent directory, and run `./emendia`. Builds target Ubuntu 24.04 x64 (glibc 2.39 or newer). See the included README or [Linux distribution guide](packaging/linux/README.md) for runtime dependencies and launcher installation.

API keys use Secret Service (GNOME Keyring or a compatible unlocked keyring). If the desktop has no tray host, Settings opens normally and closing the last window quits Emendia.

On **Wayland**, approve the desktop's global-shortcut dialog, then open **Settings → Shortcuts → Authorize Wayland capture** and grant keyboard/clipboard access. Return to your document, select text, press the shortcut and release all its keys. Results open in a preview with **Copy**; **Replace** is unavailable, and quick actions also open a preview. Capture requires desktop implementations of the GlobalShortcuts, RemoteDesktop and Clipboard portals. GNOME/KDE real-desktop validation is pending; see [Wayland support](docs/wayland.md).

**GNOME requires version 48 or newer** with its matching portal backend for global shortcuts. Ubuntu 24.04's default GNOME 46 does not provide this feature.

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
- API keys are stored in the system keyring, separately from settings.
- Replacement inserts plain text and may not work in every application. Use **Copy** for manual pasting when needed.
- The interface supports English and French, with Light, Dark and System themes.

## Building on Windows

Install [Rustup](https://rustup.rs/) with Rust **1.99 or newer**, and [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with **Desktop development with C++**, MSVC, a recent Windows SDK and CMake. Graphics drivers must support GPUI's GPU rendering.

From the repository root:

```powershell
cargo build --release --locked
```

Run `target/release/emendia.exe`. See the [development guide](docs/development.md) for setup details, troubleshooting and tests.

## Building on Linux

Install Rust **1.99 or newer** and the native dependencies listed in the [development guide](docs/development.md#linux-x11-build-environment), then:

```sh
cargo build --release --locked
./target/release/emendia
```

Create the distribution with `bash packaging/linux/package.sh v0.2.0`. X11 integration requires XTest and XFixes; Wayland integration uses XDG desktop portals.

## Documentation

- [Usage](docs/usage.md): providers, previews, proofreading styles and preferences.
- [Configuration and limitations](docs/configuration.md): data storage, privacy, migration and application integration.
- [Development](docs/development.md): build environment, project layout and verification.
- [Releases](docs/releasing.md): automated builds, portable and installer downloads, release notes and version tags.

## Roadmap

- [x] Linux X11 build
- [x] Experimental Linux Wayland capture, preview and copy via portals
- [ ] Linux Wayland desktop validation and automatic replacement
- [ ] Mac build? `¯\_(ツ)_/¯`
- [ ] Better model selection
- [ ] Overall interface improvements

## License

[MIT](LICENSE)
