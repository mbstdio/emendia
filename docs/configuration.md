# Configuration and limitations

[Back to README](../README.md)

## Data and credentials

- Selected text and, for alternative suggestions, the previous result are sent to **your configured provider**. Local-only processing is possible with a locally hosted model; actual handling depends on that provider.
- Settings are stored at `%APPDATA%\Emendia\Emendia\config\settings.json`.
- API keys are stored separately in **Windows Credential Manager**, under the `emendia` service, with one credential per normalized endpoint.
- On Linux, settings follow XDG_CONFIG_HOME (normally `~/.config/emendia/settings.json`) and credentials use **Secret Service** with the same service and endpoint identifiers. The session needs D-Bus and an unlocked compatible keyring.
- Selected text, generated results and API keys are not written to the settings JSON. Diagnostic logs do not include selected text, keys or raw provider error bodies.
- After **Copy** or successful **Replace**, the result remains in the clipboard.

## Windows startup

Startup uses the `Emendia` value in `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`, pointing to the current executable. Disabling startup and saving removes this registration. After moving the executable, save again from its new location.

## Linux startup and tray

Startup writes `$XDG_CONFIG_HOME/autostart/emendia.desktop`, normally
`~/.config/autostart/emendia.desktop`, with the current executable's absolute path.
Disabling startup removes the entry. A failed settings save restores the previous
entry. Save again after moving the executable.

The tray uses AppIndicator/StatusNotifierWatcher. GNOME may require an indicator
extension. Without a tray host, Settings opens and closing the last window exits
instead of leaving an inaccessible background process. `--settings` opens Settings
on launch. Native Wayland support is experimental and depends on the desktop's
portal implementations; see [Wayland support](wayland.md).

X11 application windows publish an embedded, multi-size `_NET_WM_ICON` using
`app-logo-light.png`, so the taskbar and Alt+Tab can display the logo even when
the executable is launched directly without installing its `.desktop` launcher.
The launcher uses the same fixed base logo.
Wayland windows use the `emendia` app ID and the installed desktop launcher/icon.

The tray follows the **system appearance**, independently of the theme selected
inside Emendia: light system theme uses `app-logo-light.png`, dark system theme
uses `app-logo-dark.png`. Its icon is updated without restarting the application,
including while all windows are closed. This also applies to the Windows tray.

## Upgrading from Translation Tool

If Emendia has no configuration yet, it imports `%APPDATA%\TranslationTool\TranslationTool\config\settings.json`, preserving shortcuts, languages, styles and theme. Existing users retain a French interface; new installations default to System. The legacy file remains available, and an existing Emendia configuration takes precedence.

Legacy endpoint credentials are migrated when accessed. An existing `TranslationTool` startup registration is migrated to `Emendia`, using the current executable, to avoid duplicate entries. Replacing or disabling startup registration restores the previous entries if saving fails.

## Capture and replacement

Capture and replacement use standard **Ctrl+C / Ctrl+V** conventions and Windows UI Automation when available. Before replacement, Emendia checks the original window, process, document title, active control and selected text. Where accessible, it also verifies the control identifier and selection position. CR, LF and CRLF line endings are treated as equivalent; other characters and paragraph counts remain checked.

Capture saves clipboard formats including Unicode text, HTML, RTF and bitmap, and restores them only if the clipboard has not changed meanwhile. If a private format cannot be preserved, Emendia tries reading the selection through UI Automation; otherwise capture stops without modifying the clipboard.

Replacement is best-effort: some applications lose their selection after focus changes, do not expose accessible controls, or use different shortcuts. Read-only fields and applications running with elevated privileges may reject replacement. Use **Copy** in these cases.

Replacement inserts **plain text**; rich document formatting is not preserved. Popups are placed within the monitor's usable area, with cursor fallback when selection coordinates are unavailable.

### Linux X11 behavior

Capture uses a fresh **Ctrl+C** publication of CLIPBOARD, rather than PRIMARY,
which may contain an old selection. PRIMARY remains untouched. A dedicated owner
thread preserves transferable formats, including HTML and binary data, and serves
incremental (INCR) transfers. Snapshots are limited to 32 formats and 16 MiB total;
capture stops if a format cannot be preserved. Restoration is conditional on the
clipboard owner and selection timestamp remaining unchanged.

Before **Replace**, Emendia requests activation of the original window and checks
the window ID, PID when exposed, title, focused X window and selected text. Quick
actions cancel if focus has changed. The result remains available in the preview
when replacement fails. X11 cannot distinguish identical text selected at another
position in the same control; use Copy when that distinction matters. Applications
must support Ctrl+C/V, so terminal-specific Ctrl+Shift+C/V is not supported.

Placement uses RandR monitor geometry, the EWMH work area and GPUI's X-screen scale
factor. Status windows are non-activating override-redirect windows. Mixed
per-output fractional scaling remains limited by GPUI's X11 backend.

### Linux Wayland behavior

Global shortcuts are assigned by the desktop through the GlobalShortcuts portal.
Emendia's saved combinations are preferred triggers, not guaranteed assignments.
Registration and permission dialogs run asynchronously; saving preferences does
not mean the desktop has approved them. Actual assignments and capture status
appear in Settings → Shortcuts.

Capture requires explicit keyboard/clipboard authorization through RemoteDesktop
and Clipboard. The permission session lasts until application exit or revocation;
no permissions are persisted and no screen or pointer access is requested.
Capture injects Ctrl+C after shortcut release, reads a fresh clipboard publication
and restores the previous transferable formats. Limits are 100,000 UTF-16 units
for selected text, 32 formats and 16 MiB for snapshots, with bounded transfers.
Clipboard changes observed during processing cancel restoration. Because the
portal does not expose atomic conditional ownership, a writer racing the final
restoration request cannot be excluded. Window/document identity cannot be checked.

All actions, including quick actions, present an editable result with Copy.
Replace is disabled; Emendia does not inject Ctrl+V into an unverifiable target.
Window placement is controlled by the compositor and uses GPUI's native scaling.
Desktop notifications provide processing status without taking keyboard focus.
Real-desktop acceptance testing on GNOME/KDE is pending.
