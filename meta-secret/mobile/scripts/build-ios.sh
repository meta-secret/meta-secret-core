#!/bin/bash
set -e

cd "$(dirname "$0")/.."

if ! command -v cbindgen &> /dev/null; then
    echo "Installing cbindgen..."
    cargo install cbindgen
fi

ROOT_DIR="$(cd .. && pwd)"

rustup target add aarch64-apple-ios x86_64-apple-ios

cd ios

echo "Compilation for iOS (arm64)..."
cargo build --package mobile-ios --target aarch64-apple-ios --release

echo "Compilation for iOS simulator (x86_64)..."
cargo build --package mobile-ios --target x86_64-apple-ios --release

echo "Creating universal lib..."
mkdir -p "${ROOT_DIR}/target/ios/universal/release"
lipo -create \
  "${ROOT_DIR}/target/aarch64-apple-ios/release/libmobile.a" \
  "${ROOT_DIR}/target/x86_64-apple-ios/release/libmobile.a" \
  -output "${ROOT_DIR}/target/ios/universal/release/metasecret-mobile.a"

echo "Generating header file..."
HEADER_OUTPUT_DIR="${ROOT_DIR}/target/ios/universal/release"
HEADER_FILE="${HEADER_OUTPUT_DIR}/metasecret-mobile.h"

mkdir -p "${HEADER_OUTPUT_DIR}"

if [ -f "cbindgen.toml" ]; then
    cbindgen --crate mobile-ios --output "${HEADER_FILE}" --config "cbindgen.toml" --lang c
else
    cbindgen --crate mobile-ios --output "${HEADER_FILE}" --lang c
fi

echo "✅ Done!"
echo "Universal library is in: ${ROOT_DIR}/target/ios/universal/release/metasecret-mobile.a"
echo "Header file is in: ${HEADER_FILE}"
