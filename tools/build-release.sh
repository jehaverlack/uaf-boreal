#!/usr/bin/env bash
set -euo pipefail

APP="uaf-boreal"
OUT="dist"

mkdir -p "$OUT"

echo "==> Building Linux x86_64"
cargo build --release --target x86_64-unknown-linux-gnu
cp "target/x86_64-unknown-linux-gnu/release/$APP" \
   "$OUT/boreal-linux-x86_64"

echo "==> Building Windows x86_64"
cargo build --release --target x86_64-pc-windows-gnu
cp "target/x86_64-pc-windows-gnu/release/$APP.exe" \
   "$OUT/boreal-windows-x86_64.exe"

echo
echo "Build complete:"
ls -lh "$OUT"