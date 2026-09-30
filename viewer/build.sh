#!/bin/sh
# Build the viewer's WebAssembly baker into viewer/web/pkg, so the page can
# open .xodr files itself. Run it from any folder, and again after changing
# the crate.
#
#     sh viewer/build.sh
#
# Installs the wasm32 target and the wasm-bindgen CLI the first time.
set -eu

cd "$(dirname "$0")/.."
version=$(sed -n 's/^wasm-bindgen = "=\(.*\)"/\1/p' viewer/Cargo.toml)

rustup target list --installed | grep -qx wasm32-unknown-unknown ||
    rustup target add wasm32-unknown-unknown
[ "$(wasm-bindgen --version 2>/dev/null)" = "wasm-bindgen $version" ] ||
    cargo install -q --locked wasm-bindgen-cli --version "$version"

cargo build -q --release -p libopendrive-viewer --lib --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir viewer/web/pkg \
    target/wasm32-unknown-unknown/release/libopendrive_viewer.wasm
