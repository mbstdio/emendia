#!/usr/bin/env bash
set -euo pipefail

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
sha256sum "target/dist/$name.tar.gz" | sed 's|  target/dist/|  |' > target/dist/SHA256SUMS-linux.txt
