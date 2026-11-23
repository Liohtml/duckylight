#!/bin/sh
# Builds both release files into host/dist/.
#   Windows: cross-built here (needs the x86_64-pc-windows-gnu target and mingw-w64).
#   Linux:   built on a machine that has the desktop development libraries; pass it as
#            LINUX_BUILD_HOST=user@host (WSL2 without sudo cannot build the Linux desktop parts).
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
mkdir -p dist

cargo build --release --target x86_64-pc-windows-gnu
cp target/x86_64-pc-windows-gnu/release/duckylight.exe "dist/duckylight-$version-windows.exe"

if [ -n "${LINUX_BUILD_HOST:-}" ]; then
    rsync -a --delete --exclude target --exclude dist ./ "$LINUX_BUILD_HOST:duckylight-build/host/"
    rsync -a ../keyboards/ducky/one3/sf_rgb/keyboard.json "$LINUX_BUILD_HOST:duckylight-build/keyboards/ducky/one3/sf_rgb/"
    ssh "$LINUX_BUILD_HOST" 'cd duckylight-build/host && cargo build --release'
    stage=$(mktemp -d)
    mkdir "$stage/duckylight-$version"
    scp -q "$LINUX_BUILD_HOST:duckylight-build/host/target/release/duckylight" "$stage/duckylight-$version/"
    cp packaging/linux/install.sh packaging/linux/70-duckylight.rules packaging/linux/duckylight.desktop packaging/linux/duckylight.png "$stage/duckylight-$version/"
    tar -C "$stage" -czf "dist/duckylight-$version-linux-x86_64.tar.gz" "duckylight-$version"
    rm -rf "$stage"
else
    echo "LINUX_BUILD_HOST nicht gesetzt: nur die Windows-Datei gebaut."
fi
ls -la dist/
