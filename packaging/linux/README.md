# Emendia for Linux x64 (X11)

Built for Ubuntu 24.04 x64 (glibc 2.39 or newer), using GPUI's GPU rendering.
Use an Xorg/X11 login session; Wayland sessions are not supported yet.

Debian 13 may also be compatible, but has not yet been validated. Debian 12's
glibc is too old for this build. The `.deb` format alone does not guarantee
compatibility with every Debian-based distribution.

## Debian package installation

Download the `.deb` from Releases, then install it with APT so dependencies are
resolved automatically:

```sh
sudo apt install ./Emendia-v0.2.0-linux-x64.deb
```

Use the downloaded filename for your version. Launch Emendia from the application
menu or run `emendia`. The package installs the executable in `/usr/bin`, along
with a system-wide launcher and icon. Install a newer `.deb` with the same command
to upgrade. To uninstall, disable automatic startup in Settings, quit Emendia,
then run `sudo apt remove emendia`. Settings and credentials are preserved.

## Archive installation

On Ubuntu 24.04, install the runtime dependencies:

```sh
sudo apt install libgtk-3-0t64 libayatana-appindicator3-1 libdbus-1-3 \
  libxkbcommon0 libxkbcommon-x11-0 libxcb1 libxcb-randr0 libxcb-xfixes0 \
  libxcb-render0 libxcb-shape0 libxcb-xkb1 libfontconfig1 libfreetype6 \
  libwayland-client0 libvulkan1 mesa-vulkan-drivers gnome-keyring
```

Keep the extracted directory in a permanent location, then run `./emendia`.
API keys are saved through Secret Service (GNOME Keyring or a compatible service).
The login session must provide D-Bus and an unlocked keyring. Local providers
can use an empty API key, but the keyring service must still be available.

To install the launcher for your user:

```sh
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications" \
  "$HOME/.local/share/icons/hicolor/256x256/apps"
ln -sfn "$PWD/emendia" "$HOME/.local/bin/emendia"
cp emendia.desktop "$HOME/.local/share/applications/"
cp emendia.png "$HOME/.local/share/icons/hicolor/256x256/apps/emendia.png"
```

Ensure `~/.local/bin` is in the graphical session's `PATH`. Use the tray menu to
open Settings or Quit. GNOME may require its AppIndicator extension; without a
StatusNotifierWatcher, Settings opens normally and closing the last window exits.

Window and launcher icons use the fixed `app-logo-light.png` logo. The executable
embeds this logo for X11 taskbars and Alt+Tab, including when launched directly.
The tray switches between `app-logo-light.png` for a light system theme and
`app-logo-dark.png` for a dark system theme; Emendia's own theme preference does
not affect that choice.

Default shortcuts: Ctrl+F12 (Translate), Ctrl+Shift+F12 (Quick Translate),
Ctrl+F11 (Proofread), Ctrl+Shift+F11 (Quick Check). Editors must support Ctrl+C/V.
Terminal-specific Ctrl+Shift+C/V shortcuts are not supported.

Capture preserves transferable clipboard formats (up to 16 MiB total and 32
formats), then restores them if no other application changed the clipboard.
PRIMARY is left untouched. Replacement inserts plain text and leaves the result
in CLIPBOARD. X11 checks the window, PID when exposed, title, focused X window and
selected text; identical text at another position in the same control cannot be
distinguished. Use Copy if replacement is unavailable.

Settings follow XDG_CONFIG_HOME (normally `~/.config/emendia/settings.json`).
Automatic startup uses `~/.config/autostart/emendia.desktop`; after moving the
executable, save the startup setting again. To remove the launcher, remove the
symlink, desktop entry and icon above. Disable automatic startup before deleting
the application directory. Settings and credentials can be kept for reinstalls.
