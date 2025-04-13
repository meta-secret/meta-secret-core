#!/bin/bash
set -e

rustup target add aarch64-apple-ios x86_64-apple-ios

echo "Compilation for iOS (arm64)..."
cargo build --package meta-secret-core --target aarch64-apple-ios --release --features mobile

echo "Compilation for iOS simulator (x86_64)..."
cargo build --package meta-secret-core --target x86_64-apple-ios --release --features mobile

echo "Creating universal lib..."
mkdir -p target/universal/release
lipo -create \
  target/aarch64-apple-ios/release/libmeta_secret_core.a \
  target/x86_64-apple-ios/release/libmeta_secret_core.a \
  -output target/universal/release/libmeta_secret_core.a

echo "Done! The lib is in target/universal/release/libmeta_secret_core.a"
