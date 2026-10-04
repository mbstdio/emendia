# Emendia for Linux x64

Built for Ubuntu 24.04 x64 (glibc 2.39 or newer), using GPUI's GPU rendering.
X11 supports capture and replacement. Wayland support is experimental: capture,
preview and copy use desktop portals; automatic replacement is unavailable.

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

## Experimental Wayland workflow

Install `xdg-desktop-portal` and the appropriate backend for your desktop. Capture
requires implementations of GlobalShortcuts, RemoteDesktop and Clipboard; the
available versions depend on your desktop and distribution. An installed portal
package alone does not guarantee all three interfaces are implemented.
GNOME global shortcuts require GNOME 48 or newer with the matching portal backend.
Ubuntu 24.04's default GNOME 46 does not support this Wayland workflow; the binary
can still be used under X11 or with a compatible newer desktop.

Install the launcher and icon as above so the desktop can associate native
Wayland windows with Emendia. Save settings and approve the global-shortcut
dialog. In **Settings → Shortcuts**, choose **Authorize Wayland capture** and
grant keyboard/clipboard access, then return to your document. Capture is triggered
after releasing a shortcut; release all its modifier keys too.

The settings page shows the actual combinations chosen by the desktop, which can
differ from Emendia's saved preferences. **Configure desktop shortcuts** requires
GlobalShortcuts version 2. Authorization must be repeated after application restart
or session revocation. No screen capture or pointer access is requested.

All four actions open results in a preview. Use **Copy**, return to your document
and paste manually. **Replace** is disabled because standard portals cannot
identify and reactivate the original window. Processing status uses desktop
notifications instead of mapping a window that might take focus.

Clipboard snapshots are bounded to 32 formats and 16 MiB. Owner-change signals
are checked during reads and before restoration; the portal has no atomic
compare-and-set operation, so it cannot provide X11's owner/timestamp guarantees.
Native GNOME/KDE desktop acceptance testing is still pending. Sway's wlr portal
alone does not supply the required interfaces; Hyprland's shortcut portal alone
is also insufficient for capture. XWayland is not used as a fallback.

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
