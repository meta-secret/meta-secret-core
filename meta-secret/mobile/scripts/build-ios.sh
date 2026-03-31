#!/bin/bash
set -e

export IPHONEOS_DEPLOYMENT_TARGET=16.0
cd "$(dirname "$0")/.."

ROOT_DIR="$(cd .. && pwd)"

rustup target add aarch64-apple-ios x86_64-apple-ios

echo "Compilation for iOS (arm64)..."
cargo build --package mobile-uniffi --target aarch64-apple-ios --release

echo "Compilation for iOS simulator (x86_64)..."
cargo build --package mobile-uniffi --target x86_64-apple-ios --release

echo "Creating universal lib..."
mkdir -p "${ROOT_DIR}/target/ios/universal/release"
lipo -create \
  "${ROOT_DIR}/target/aarch64-apple-ios/release/libmetasecret_mobile.a" \
  "${ROOT_DIR}/target/x86_64-apple-ios/release/libmetasecret_mobile.a" \
  -output "${ROOT_DIR}/target/ios/universal/release/metasecret-mobile.a"

echo "✅ Done!"
echo "Universal library is in: ${ROOT_DIR}/target/ios/universal/release/metasecret-mobile.a"
