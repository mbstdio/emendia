# Experimental Wayland support

[Back to README](../README.md)

The first Wayland milestone implements native windows, portal global shortcuts,
selected-text capture, editable previews and copying results. It requires no
GNOME Shell extension or KWin script. GNOME/KDE real-desktop acceptance testing
is pending; portal protocol tests do not certify a compositor implementation.

## Setup

1. Build with `cargo build --locked` and install the `emendia.desktop` launcher
   and icon as described in the [Linux distribution guide](../packaging/linux/README.md).
2. Run Emendia in a Wayland session and complete setup. Approve the desktop's
   global-shortcut dialog when it appears.
3. In **Settings → Shortcuts**, select **Authorize Wayland capture** and approve
   keyboard and clipboard access. Wait for the capture-ready status.
4. Return to your editor, select text, press a shortcut and release all its keys.
5. Review the result and choose **Copy**, then return to the editor and paste.

Actual shortcut combinations appear in Settings. Saved combinations are only
preferred triggers. The desktop may remember previous assignments. Recreating a
session on settings save does not force the desktop to change them; use
**Configure desktop shortcuts** where GlobalShortcuts version 2 is available.
Authorization must be repeated after restart or session revocation.

### GNOME versions and missing GlobalShortcuts

GNOME introduced global shortcuts in **GNOME 48**. GNOME 46, shipped with
Ubuntu 24.04, does not provide this portal even when `xdg-desktop-portal` and
`xdg-desktop-portal-gnome` are installed. The Ubuntu 24.04 binary/build baseline
does not mean its default GNOME desktop supports Emendia's Wayland workflow.
Use GNOME 48 or newer with the matching portal backend; the keyboard/clipboard
interfaces must also be available. See the [GNOME 48 release notes](https://release.gnome.org/48/).

If Emendia reports a missing GlobalShortcuts portal, inspect the active session
from a terminal in the graphical desktop:

```sh
busctl --user introspect org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop
```

The output must include `org.freedesktop.portal.GlobalShortcuts`. Merely restarting
or reinstalling a portal backend that predates this feature cannot add it.
On a supported desktop, verify that its matching backend is installed and that
the session selects it through `XDG_CURRENT_DESKTOP` and `portals.conf`.

## Required capabilities

| Feature | Requirement |
| --- | --- |
| Native windows | GPUI Wayland backend and compatible graphics drivers |
| Global actions | `org.freedesktop.portal.GlobalShortcuts` |
| Desktop shortcut configuration button | GlobalShortcuts version 2 |
| Copying selected text | RemoteDesktop with keyboard permission and Clipboard with granted access |
| Processing status | `org.freedesktop.Notifications` (optional) |
| Tray | StatusNotifierWatcher/AppIndicator host (optional) |
| Credentials | Unlocked Secret Service |

Clipboard access is requested before starting the RemoteDesktop session. Emendia
requests only keyboard access, without pointer control or screen sharing, and
does not store restore tokens. Missing portals, cancelled permissions and closed
sessions are reported in Settings; they do not prevent native windows opening.

Sway's `xdg-desktop-portal-wlr` implementation provides Screenshot and ScreenCast,
which are insufficient for this workflow. Hyprland provides global shortcuts,
but capture still needs the other interfaces. Emendia does not substitute X11
grabs or XTest for missing Wayland capabilities.

## Current limits

- **Replace is disabled.** Standard portals cannot identify and reactivate the
  original window. Quick actions also open an already-processed preview for copying.
- Capture relies on the document retaining focus and supporting Ctrl+C. No
  portable window/control identity check is available. Release all shortcut keys;
  a held Shift/Alt/Super modifier can alter the editor's copy command.
- Clipboard preservation supports up to 32 transferable formats and 16 MiB total.
  Transfers are nonblocking and have a three-second timeout. Selected text is
  limited to 100,000 UTF-16 units. PRIMARY remains untouched.
- Owner-change events are checked during transfers and before restoration.
  The portal has no atomic compare-and-set operation: a writer racing the final
  SetSelection request can still be overwritten.
- The compositor controls preview placement. Processing uses notifications,
  rather than an ordinary window which might take keyboard focus.
  GPUI does not expose an API for forwarding the shortcut portal's activation
  token to an existing Wayland connection, so automatic preview activation is
  compositor-dependent; select Emendia from the task switcher if needed.
- Shortcut approval is asynchronous. Preferences are saved independently of
  desktop authorization; a refused registration is reported afterward.

## Automated verification

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
dbus-run-session -- env EMENDIA_PORTAL_TEST=1 cargo test --locked --lib \
  wayland_portal_round_trip -- --ignored --nocapture --test-threads=1
```

The portal test runs a fake implementation on an isolated session bus. It verifies
all four shortcut descriptors, desktop-overridden labels, release dispatch,
session cleanup, keyboard-only permissions, UTF-8 capture, binary clipboard
restoration and serving, failed selection validation, concurrent writers, result
copying, revocation and cancelled authorization. Unit tests verify XKB trigger
encoding and bounded nonblocking descriptor transfers.

The existing `packaging/linux/test-x11.sh` verifies X11 non-regression. Its graphical
shortcut, status-window and quick-replacement diagnostics assume X11 integration.

On a disposable native Wayland session, the onboarding, translation-preview and
correction-preview diagnostics also work:

```sh
cargo build --locked
./target/debug/emendia --smoke-test-onboarding --ui-language=en
./target/debug/emendia --smoke-test --ui-language=en
./target/debug/emendia --smoke-test-correction --ui-language=en
```

These three flows have passed in English and French on an isolated headless
GNOME 46 Wayland compositor. They verify opening, onboarding navigation, mock
provider results and preview layout; they do not exercise real pointer dragging
or portal permission dialogs. Client-decorated windows render GPUI's TitleBar
with dragging and supported min/max/close controls, inside GPUI Kit's existing
window border. Server-decorated windows retain the compositor's native title bar.
The Wayland display is selected from advertised outputs because this GPUI backend
does not expose a primary display, and the output list is refreshed before capture.

## GNOME and KDE acceptance checks

Run on disposable native sessions and record distribution, compositor and portal
package versions:

- Start without DISPLAY/XWayland; verify onboarding, settings, previews, scaling,
  launcher/icon association and tray/no-tray lifetime.
- Approve/refuse shortcuts; verify all four actions, desktop changes to bindings,
  disabling/re-enabling, settings save and the version-2 configuration dialog.
- Approve/refuse keyboard/clipboard permissions; verify no screen/pointer permission
  is requested and revocation can be recovered through Settings.
- Capture in GTK, Qt and browser text fields, including copying unchanged text
  twice and releasing modifiers slowly.
- Check previous clipboard text, HTML and binary/image data after capture, failed
  capture and concurrent clipboard writes.
- Verify translation and correction previews, quick-action previews, result copying,
  disabled replacement and notifications without stealing focus during capture.
- Check provider errors, restart, unlocked keyring and multiple display scales.
