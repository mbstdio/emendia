#!/usr/bin/env bash
set -euo pipefail
umask 022

label="${1:?Usage: bash packaging/linux/package.sh BUILD_LABEL [BINARY]}"
binary="${2:-${CARGO_TARGET_DIR:-target}/release/emendia}"
if [[ ! "$label" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
    printf 'Invalid build label: %s\n' "$label" >&2
    exit 1
fi
name="Emendia-$label-linux-x64"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/$name" target/dist
install -m 755 "$binary" "$stage/$name/emendia"
install -m 644 LICENSE "$stage/$name/LICENSE"
install -m 644 packaging/linux/emendia.desktop "$stage/$name/emendia.desktop"
install -m 644 src/ressources/app-logo-light.png "$stage/$name/emendia.png"
install -m 644 packaging/linux/README.md "$stage/$name/README.md"
desktop-file-validate "$stage/$name/emendia.desktop"
tar -czf "target/dist/$name.tar.gz" -C "$stage" "$name"

# Debian prereleases must sort before the corresponding stable version.
version="${label#v}"
version="${version/-/\~}"
root="$stage/debian/emendia"
mkdir -p "$root/DEBIAN" "$root/usr/bin" "$root/usr/share/applications" \
    "$root/usr/share/icons/hicolor/256x256/apps" "$root/usr/share/doc/emendia" \
    "$stage/debian"
install -m 755 "$binary" "$root/usr/bin/emendia"
install -m 644 packaging/linux/emendia.desktop "$root/usr/share/applications/emendia.desktop"
install -m 644 src/ressources/app-logo-light.png "$root/usr/share/icons/hicolor/256x256/apps/emendia.png"
install -m 644 LICENSE "$root/usr/share/doc/emendia/copyright"
install -m 644 packaging/linux/README.md "$root/usr/share/doc/emendia/README.md"

# Resolve minimum library versions from the actual executable and the build host.
# dpkg-shlibdeps expects a Debian source control file in its working directory.
printf 'Source: emendia\n\nPackage: emendia\nArchitecture: amd64\n' > "$stage/debian/control"
dependencies=$(
    cd "$stage"
    dpkg-shlibdeps -O -e "$root/usr/bin/emendia"
)
dependencies="${dependencies#shlibs:Depends=}"
# Include runtime libraries loaded dynamically, which shlibdeps cannot discover.
cat > "$root/DEBIAN/control" <<EOF
Package: emendia
Version: $version
Section: utils
Priority: optional
Architecture: amd64
Maintainer: Emendia contributors <mbstdio@users.noreply.github.com>
Homepage: https://github.com/mbstdio/emendia
Installed-Size: $(du -sk "$root/usr" | cut -f1)
Depends: $dependencies, libayatana-appindicator3-1, libvulkan1, libxcb-randr0, libxcb-xfixes0, libxcb-render0, libxcb-shape0, libxcb-xkb1, libfontconfig1, libfreetype6, libwayland-client0
Recommends: gnome-keyring, mesa-vulkan-drivers
Description: Contextual translation and proofreading
 Translate and improve selected text using OpenAI-compatible providers.
 Runs in the notification area and requires an Xorg/X11 desktop session.
EOF
dpkg-deb --build --root-owner-group "$root" "target/dist/$name.deb"
sha256sum "target/dist/$name.tar.gz" "target/dist/$name.deb" \
    | sed 's|  target/dist/|  |' > target/dist/SHA256SUMS-linux.txt
