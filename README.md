![Emendia](src/ressources/onboard-hero.jpg)

# Emendia

[![Latest release](https://img.shields.io/github/v/release/mbstdio/emendia?label=release)](https://github.com/mbstdio/emendia/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20Linux%20X11-blue)](#getting-started)

**Translate. Proofread. Refine.**

Emendia is a free and open-source desktop app that helps you translate and improve selected text without leaving the application you're using. Available for Windows and Linux X11, it works with cloud and locally hosted AI models.

- **Translate selected text** into your preferred language.
- **Fix spelling and grammar** while preserving your voice.
- **Refine your writing** with fluent, professional, casual or concise styles.
- **Review before replacing** — edit a suggestion, request another, or copy the result. Quick actions can replace your selection automatically.
- **Choose your AI provider** — OpenAI, LM Studio, Ollama or another OpenAI-compatible service.
- **Make it yours** with configurable shortcuts, Light, Dark and System themes, and an English or French interface.

Select text → press a shortcut → review the suggestion → **Replace** or **Copy**.

### Table of contents

- [Getting started](#getting-started)
- [Keyboard shortcuts](#keyboard-shortcuts)
- [AI providers](#ai-providers)
- [Good to know](#good-to-know)
- [Documentation](#documentation)
- [Contribute](#contribute)
- [Development](#development)
- [Roadmap](#roadmap)
- [License](#license)

---

## Getting started

### Download and install

Choose a download from the [latest release](https://github.com/mbstdio/emendia/releases/latest):

| Platform | Download | Installation |
| --- | --- | --- |
| Windows | **Installer** — `Emendia-<version>-windows-x64-setup.exe` | Run the installer to add shortcuts and an uninstaller. No administrator rights required. |
| Windows | **Portable** — `Emendia-<version>-windows-x64.exe` | Run directly, without installation. |
| Linux X11 | **Debian package** — `Emendia-<version>-linux-x64.deb` | Run `sudo apt install ./Emendia-<version>-linux-x64.deb` to install dependencies and the application launcher. |
| Linux X11 | **Archive** — `Emendia-<version>-linux-x64.tar.gz` | Extract into a permanent directory and run `./emendia`. |

**System requirements:** Windows 10 version 1903 or newer (x64), with compatible graphics drivers; or Linux x64 with glibc 2.39 or newer in an **Xorg/X11 login session**. Linux builds target Ubuntu 24.04. Wayland sessions are not supported yet.

<details>
<summary>More installation details</summary>

- Both Windows downloads use the same executable. Portable builds store settings in your Windows user profile and API keys in Windows Credential Manager.
- Linux API keys use Secret Service, such as GNOME Keyring, and require an unlocked compatible keyring.
- Debian 13 compatibility has not yet been validated; Debian 12 is not supported by this build. See the [Linux distribution guide](packaging/linux/README.md) for runtime dependencies and installation details.
- Each release includes `SHA256SUMS.txt` to verify downloads.

</details>

### Try your first correction

1. Launch Emendia and follow the setup assistant to choose your [AI provider](#ai-providers), model, languages and shortcuts.
2. Select a sentence in an editor or text field.
3. Press **Ctrl+F11**, then release the keys, to preview a correction. Use **Ctrl+F12** for a translation.
4. Review or edit the suggestion, then choose **Replace** or **Copy**.

Click the tray icon to open **Settings**. Closing windows keeps Emendia running; choose **Quit** in the tray menu to exit. On Linux desktops without a tray host, Settings opens normally and closing the last window quits Emendia.

## Keyboard shortcuts

| Action | Default shortcut | Result |
| --- | --- | --- |
| Translate with preview | **Ctrl+F12** | Review and edit before replacing or copying |
| Quick Translate | **Ctrl+Shift+F12** | Replace the selection automatically |
| Proofread with preview | **Ctrl+F11** | Review and edit before replacing or copying |
| Quick Check | **Ctrl+Shift+F11** | Replace the selection automatically |

All shortcuts are configurable in Settings. For quick actions, keep the original selection and stay in the same document while processing.

## AI providers

Use a cloud provider or run your own local model:

| Provider | What you need |
| --- | --- |
| **OpenAI** | An API key and a model available to your API account. Usage is billed by OpenAI; a ChatGPT subscription does not include API access or credits. |
| **LM Studio** | A loaded model and the local HTTP server running. |
| **Ollama** | An installed model and the local service running. |
| **Other compatible providers** | An OpenAI-compatible API base URL, model name and API key if required. |

The setup assistant helps you configure your provider. Use **Test connection** to check your settings with a small real translation request. See the [provider guide](docs/usage.md#providers) for URLs and configuration details.

## Good to know

- Selected text is sent to your configured provider. Use a locally hosted model for local processing.
- API keys are stored in the system keyring, separately from settings.
- Replacement inserts plain text and may not work in every application. Use **Copy** for manual pasting when needed.
- The interface supports English and French, with Light, Dark and System themes.

## Documentation

- [Usage](docs/usage.md): providers, previews, proofreading styles and preferences.
- [Configuration and limitations](docs/configuration.md): data storage, privacy, migration and application integration.
- [Development](docs/development.md): build environment, project layout and verification.
- [Releases](docs/releasing.md): automated builds, portable and installer downloads, release notes and version tags.

## Contribute

Feedback and contributions are welcome:

- [Report a bug](https://github.com/mbstdio/emendia/issues): describe what happened, what you expected, and your operating system. Include steps to reproduce when possible.
- [Suggest an improvement](https://github.com/mbstdio/emendia/issues): share your idea and how it would help your workflow.
- Improve the code or documentation: see the [development guide](docs/development.md) to get started.

## Development

Emendia is built with Rust and GPUI. To build from source, follow the [development guide](docs/development.md) for Windows or Linux X11 prerequisites, build commands and tests. Rust **1.99 or newer** is required.

## Roadmap

- [x] Linux X11 build
- [ ] Linux Wayland support
- [ ] Mac build? `¯\_(ツ)_/¯`
- [ ] Better model selection
- [ ] Overall interface improvements

## License

[MIT](LICENSE)
