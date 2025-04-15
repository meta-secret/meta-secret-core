#!/bin/bash
set -e

# Проверяем, установлен ли cbindgen, и устанавливаем его при необходимости
if ! command -v cbindgen &> /dev/null; then
    echo "Устанавливаем cbindgen..."
    cargo install cbindgen
fi

rustup target add aarch64-apple-ios x86_64-apple-ios

echo "Compilation for iOS (arm64)..."
cargo build --package mobile --target aarch64-apple-ios --release

echo "Compilation for iOS simulator (x86_64)..."
cargo build --package mobile --target x86_64-apple-ios --release

echo "Creating universal lib..."
mkdir -p target/universal/release
lipo -create \
  target/aarch64-apple-ios/release/libmobile.a \
  target/x86_64-apple-ios/release/libmobile.a \
  -output target/universal/release/libmobile.a

echo "Generating header file..."
MOBILE_PROJECT_DIR="$(pwd)/mobile/ios"
HEADER_OUTPUT_DIR="target/universal/release"
HEADER_FILE="${HEADER_OUTPUT_DIR}/mobile.h"

cbindgen --crate mobile --output "${HEADER_FILE}" --config "${MOBILE_PROJECT_DIR}/cbindgen.toml" --lang c

echo "Done! The library is in target/universal/release/libmobile.a"
echo "The header file is in target/universal/release/mobile.h"
