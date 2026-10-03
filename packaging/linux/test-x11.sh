#!/usr/bin/env bash
set -euo pipefail

# Always use an isolated display/session/profile: tests temporarily own focus
# and the clipboard, and must never run against the user's working desktop.
if [[ "${1:-}" != --inside ]]; then
    exec dbus-run-session -- xvfb-run -a -s '-screen 0 1920x1080x24' bash "$0" --inside
fi

profile=$(mktemp -d)
export XDG_CONFIG_HOME="$profile/config"
export XDG_DATA_HOME="$profile/data"
export XDG_CACHE_HOME="$profile/cache"
export XDG_RUNTIME_DIR="$profile/runtime"
mkdir -m 700 "$XDG_RUNTIME_DIR"
export XDG_SESSION_TYPE=x11
export GDK_BACKEND=x11
unset WAYLAND_DISPLAY GNOME_KEYRING_CONTROL
openbox &
wm=$!
trap 'kill "$wm" 2>/dev/null || true; rm -rf "$profile"' EXIT

gnome-keyring-daemon --foreground --unlock --components=secrets \
    --control-directory="$profile/keyring" <<< 'emendia-test-only' &
keyring=$!
trap 'kill "$wm" "$keyring" 2>/dev/null || true; rm -rf "$profile"' EXIT

cargo_command=(cargo)
if [[ -n "${RUST_TOOLCHAIN:-}" ]]; then cargo_command+=("+$RUST_TOOLCHAIN"); fi
"${cargo_command[@]}" test --locked --lib x11_clipboard_and_native_edit_round_trip -- --ignored --nocapture --test-threads=1
"${cargo_command[@]}" test --locked --lib x11_tray_and_global_shortcuts -- --ignored --nocapture --test-threads=1
"${cargo_command[@]}" test --locked --lib x11_taskbar_icon_is_embedded_without_an_installed_launcher -- --ignored --nocapture --test-threads=1
"${cargo_command[@]}" test --locked --lib linux_keyring_persists_across_processes -- --ignored --nocapture --test-threads=1

binary="${EMENDIA_BINARY:-target/debug/emendia}"
for language in en fr; do
    for mode in --smoke-test-onboarding --smoke-test-shortcuts --smoke-test --smoke-test-quick --smoke-test-correction --smoke-test-quick-check; do
        timeout 30s "$binary" "$mode" "--ui-language=$language"
    done
    timeout 30s "$binary" --smoke-test-onboarding --smoke-theme=dark "--ui-language=$language"
done
for scale in 1.25 1.5; do
    GPUI_X11_SCALE_FACTOR="$scale" timeout 30s "$binary" --smoke-test
done
