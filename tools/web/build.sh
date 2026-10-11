#!/usr/bin/env bash
# Build the web (WebAssembly) version into a folder ready to host as static files.
#
#   tools/web/build.sh [OUT_DIR]        # default: dist
#
# Needs the wasm32-unknown-unknown target and the wasm-bindgen CLI at the same
# version as the wasm-bindgen crate in Cargo.lock. wasm-opt (binaryen) shrinks
# the result further when it's installed.
set -euo pipefail
cd "$(dirname "$0")/../.."
OUT="${1:-dist}"

cargo build --profile web-release --target wasm32-unknown-unknown

rm -rf "$OUT"
mkdir -p "$OUT"
wasm-bindgen --target web --no-typescript --out-dir "$OUT" --out-name paw_and_claw \
    target/wasm32-unknown-unknown/web-release/paw-and-claw.wasm

if command -v wasm-opt >/dev/null; then
    wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext \
        --enable-reference-types --enable-mutable-globals \
        "$OUT/paw_and_claw_bg.wasm" -o "$OUT/paw_and_claw_bg.wasm" \
        || echo "wasm-opt failed; keeping the unoptimized module"
else
    echo "wasm-opt not found; skipping size optimization"
fi

cp web/index.html "$OUT/"
# Runtime assets only: sprite originals and style anchors are source material,
# and the game data is embedded in the binary
rsync -a --exclude 'sprites/originals' --exclude 'sprites/anchors' --exclude 'sprites/manifest.json' \
    --exclude '*.md' assets/ "$OUT/assets/"

echo "Web build in $OUT ($(du -sh "$OUT" | cut -f1))"
