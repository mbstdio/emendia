# Configuration and limitations

[Back to README](../README.md)

## Data and credentials

- Selected text and, for alternative suggestions, the previous result are sent to **your configured provider**. Local-only processing is possible with a locally hosted model; actual handling depends on that provider.
- Settings are stored at `%APPDATA%\Emendia\Emendia\config\settings.json`.
- API keys are stored separately in **Windows Credential Manager**, under the `emendia` service, with one credential per normalized endpoint.
- On Linux, settings follow XDG_CONFIG_HOME (normally `~/.config/emendia/settings.json`) and credentials use **Secret Service** with the same service and endpoint identifiers. The session needs D-Bus and an unlocked compatible keyring.
- Selected text, generated results and API keys are not written to the settings JSON. Diagnostic logs do not include selected text, keys or raw provider error bodies.
- After **Copy** or successful **Replace**, the result remains in the clipboard.

Keyring access and settings writes run in background workers. Provider URL edits
wait for a short pause before loading a saved key; late loads cannot overwrite a
different provider's key or a manually edited draft. Saving keeps the previous
shortcuts reserved until persistence succeeds, and restores the previous state
on failure. Settings controls and window closure are temporarily disabled while
a save completes.

Credential reads, legacy migration, writes and save rollbacks are serialized
within the process, including calls from previews and quick actions. A migration
that was started before a save cannot overwrite the newly committed key.

## Single instance

On Windows and Linux, only one Emendia instance can run per configuration profile.
A second launch exits silently with a successful exit code, leaving the existing
instance running. This also applies to `--settings`; use the tray icon to open
Settings when Emendia is already running. Windows portable and installed builds
share the same profile and instance lock.

An exclusive operating-system lock on `instance.lock`, beside `settings.json`,
is acquired before desktop initialization, configuration migration and shortcut
registration. The lock is automatically released on exit, including after a crash
or forced termination. The file remains on disk and does not prevent restarting;
do not delete it while Emendia is running. If the lock cannot be opened or acquired
because of an unexpected error, startup fails rather than allowing another instance.

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
on launch. A native X11 login session is required; Wayland/XWayland sessions are
not supported yet.

Application windows publish an embedded, multi-size `_NET_WM_ICON` using
`app-logo-light.png`, so the taskbar and Alt+Tab can display the logo even when
the executable is launched directly without installing its `.desktop` launcher.
The launcher uses the same fixed base logo.

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

Provider responses are limited to 4 MiB, including responses without a declared
length. Generated and edited results must contain no null character and must not
exceed 100,000 UTF-16 code units on either platform. Invalid results are rejected
before clipboard publication or replacement. Results explicitly reported as
truncated, filtered or incomplete by the provider are also rejected.

### Linux X11 behavior

Capture uses a fresh **Ctrl+C** publication of CLIPBOARD, rather than PRIMARY,
which may contain an old selection. PRIMARY remains untouched. A dedicated owner
thread preserves transferable formats, including HTML and binary data, and serves
incremental (INCR) transfers. Snapshots are limited to 32 formats and 16 MiB total;
capture stops if a format cannot be preserved. Restoration is conditional on the
clipboard owner and selection timestamp remaining unchanged.

Capture requires X-Resource 1.2 and a server-reported local process identity for
the source window and clipboard owner. Publications from another process,
including a clipboard manager relaying a copy, are rejected rather than sent to
the provider. If the source cannot be verified, capture is cancelled.

Before **Replace**, Emendia requests activation of the original window and checks
the window ID, PID when exposed, title, focused X window and selected text. Quick
actions cancel if focus has changed. The result remains available in the preview
when replacement fails. X11 cannot distinguish identical text selected at another
position in the same control; use Copy when that distinction matters. Applications
must support Ctrl+C/V, so terminal-specific Ctrl+Shift+C/V is not supported.

Placement uses RandR monitor geometry, the EWMH work area and GPUI's X-screen scale
factor. Status windows are non-activating override-redirect windows. Mixed
per-output fractional scaling remains limited by GPUI's X11 backend.
