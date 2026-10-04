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

The [release workflow](releasing.md) builds this executable with a statically linked Visual C++ runtime, distributes it directly as the portable download, and packages the same binary with Inno Setup. See that guide for local packaging and build triggers.

For a temporary interface-language override, use `--ui-language=en` or `--ui-language=fr`. This does not save the preference unless you change it in Settings.

## Linux X11 build environment

The reference distribution is **Ubuntu 24.04 x64**, with Rust **1.99 or newer**.
GPUI can render on Linux, but Emendia's desktop integration currently requires an
Xorg/X11 session with an EWMH-compatible window manager, XTest and XFixes.

```sh
sudo apt install build-essential clang cmake pkg-config \
  libgtk-3-dev libayatana-appindicator3-dev libdbus-1-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev \
  libfontconfig1-dev libfreetype6-dev libwayland-dev libvulkan-dev \
  mesa-vulkan-drivers
cargo build --release --locked
./target/release/emendia
```

The executable is `target/release/emendia`. See the [Linux distribution guide](../packaging/linux/README.md)
for runtime dependencies and launcher installation. Saved API keys use Secret
Service; a D-Bus session and an unlocked compatible keyring must be available.
System language follows `LC_ALL`, `LC_MESSAGES`, then `LANG`.

GPUI 0.7 uses one scale factor per X screen. `GPUI_X11_SCALE_FACTOR=1.25` can
override it; RandR monitor bounds and the desktop work area constrain popups.
Independent fractional scales on different outputs are not supported by this GPUI
backend. Wayland sessions are detected and rejected, including XWayland-only use.

## Source conventions

The project's source language is **English**. French UI translations live in `src/i18n.rs`. Add languages to `UiLanguage` and the localization catalog. Translation-language values are stable English names; localized labels and legacy French names are converted independently from the interface language. Proofreading prompts use English style names and instructions.

## Project layout

```text
src/
  main.rs                  Startup, runtime and application lifetime
  app.rs                   Window, tray, shortcut and capture coordination
  i18n.rs                  Interface language preference and translation catalog
  settings.rs              JSON configuration, migration and system credentials
  translation.rs           OpenAI-compatible client and translation/proofreading prompts
  smoke.rs                 Local mock provider for graphical smoke tests
  ui/
    settings.rs            Settings categories and shortcut capture
    preview.rs             Editable preview and cancellable tasks
    status.rs              Non-activating status popup
    theme.rs               System theme and native title bar synchronization
    mod.rs                 Shared controls and localized language selectors
  platform/
    desktop.rs             Shared desktop integration entry point
    icons.rs               Embedded PNG logos, tray variants and X11 ARGB icon data
    windows.rs             Capture, identity, placement, clipboard and replacement
    windows/               Native Windows startup, tray and desktop tests
    linux/                 X11 capture, clipboard service, GTK tray, autostart and tests
    hotkey.rs              Transactional registration of four global shortcuts
    startup.rs             Windows sign-in registration and migration
    tray.rs                Notification icon and menu
```

Emendia uses GPUI Kit 0.7 for native views, Reqwest and Tokio for background HTTP requests, Serde for JSON, `global-hotkey` for shared shortcut parsing and Windows registration, `tray-icon` for desktop integration, `windows` for Win32/UI Automation, `x11rb` for X11/XTest/XFixes/RandR and Linux shortcut registration, `keyring` for credentials, and `tracing` for diagnostics. Clipboard operations are serialized; unsafe code is confined to platform integration. Setup images are embedded in the executable.

The Linux clipboard owner, shortcut service and GTK tray run on separate threads so native events
continue to be served while GPUI and capture workers are busy. GTK objects never
cross threads; the controller exchanges menu IDs, events and commands.

X11 shortcut registration and release are acknowledged only after checked server
round trips. This prevents a released combination remaining grabbed in a client
buffer, including when disabling the final shortcut. Lock modifiers are discovered
from the server mapping, and unregistering uses the original registered keycode.

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

Smoke diagnostics return a nonzero exit code if desktop initialization fails,
another instance owns the profile, or the application exits before all checks
complete. A normal second launch still exits successfully. Shortcut diagnostics
require Linux X11.

An additional Windows test launches an isolated native editor and checks capture, paste, focus preservation and clipboard restoration for text and bitmap. It requires an interactive desktop and temporarily uses focus and the clipboard; do not interact with another application during the test:

```powershell
cargo test --locked --lib clipboard_and_native_edit_round_trip -- --ignored --nocapture --test-threads=1
```

Manual checks should cover language changes with another window open, persistence after restart, unsaved form edits during theme/language changes, all five proofreading styles, shortcut conflicts, startup registration, changed selections during processing, offline providers, multi-monitor placement and 125–150% DPI.

### Linux verification

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --locked
sudo apt install xvfb xauth openbox desktop-file-utils gnome-keyring
bash packaging/linux/test-x11.sh
```

The script creates a disposable Xvfb/Openbox desktop, D-Bus session and XDG profile.
It tests a real GTK text editor in a separate process: capture, replacement, focus
preservation, concurrent clipboard writers, multi-format restoration, incremental
clipboard transfers and oversized selections. Another test registers a real GTK
tray item with a test StatusNotifierWatcher and checks global shortcut delivery
and conflicts. `--smoke-test-shortcuts` injects registered Ctrl+M via XTest into
the active GPUI form and verifies all 12 source/target action transfers, warnings,
empty bindings and unsaved reverse transfers. Preview diagnostics also check that
X11 advertises the preview as a movable managed window rather than a notification.
Icon tests cover PNG decoding, RGBA-to-ARGB conversion and the fixed light window
logo at multiple resolutions. An opt-in X11 test reads `_NET_WM_ICON` directly
without an installed launcher; onboarding and preview diagnostics verify the
same property on actual GPUI windows. The tray test reads the exported native
indicator PNG through D-Bus and verifies light → dark → light switching without
rewriting the icon when the system appearance is unchanged.
Graphical smoke tests use a mock keyring and local HTTP provider,
then cover English/French, onboarding, previews, quick-flow recovery and 125–150%
scaling. A separate isolated GNOME Keyring verifies credential persistence between
processes. The tests do not read or write the user's keyring.

To check the release executable, set `EMENDIA_BINARY=target/release/emendia` when
running the script. A real-desktop acceptance check should also cover Qt editors,
browsers, GNOME/KDE tray integration, startup and keyring unlock behavior.

## References

- [GPUI](https://www.gpui.rs/)
- [GPUI Kit](https://github.com/longbridge/gpui-kit)
- [Zed's Windows build guide](https://github.com/zed-industries/zed/blob/main/docs/src/development/windows.md)
- [Windows SendInput documentation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)
- [OleDuplicateData](https://learn.microsoft.com/en-us/windows/win32/api/ole2/nf-ole2-oleduplicatedata)
