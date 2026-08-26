#!/bin/bash
set -e

export IPHONEOS_DEPLOYMENT_TARGET=16.0
cd "$(dirname "$0")/.."

ROOT_DIR="$(cd .. && pwd)"
OUT_DIR="${ROOT_DIR}/target/ios"
COMPOSE_ROOT="${ROOT_DIR}/../../meta-secret-compose"
UNIFFI_CRATE_DIR="${ROOT_DIR}/mobile/uniffi"
IOS_BINDINGS_STAGING_DIR="${OUT_DIR}/ios-bindings"
IOS_BINDINGS_DIR_APP="${COMPOSE_ROOT}/iosApp/iosApp/UniffiGenerated"
IOS_BINDINGS_DIR_SERVICE="${COMPOSE_ROOT}/iosApp/iosApp/MetaSecretCoreService/UniffiGenerated"
mkdir -p "${OUT_DIR}"

echo "🔨 Installing Rust iOS targets..."
rustup target add aarch64-apple-ios aarch64-apple-ios-sim

echo "📱 Compiling for iOS device (arm64)..."
cargo build --package mobile-uniffi --target aarch64-apple-ios --release

echo "📱 Compiling for iOS simulator (arm64 Apple Silicon)..."
cargo build --package mobile-uniffi --target aarch64-apple-ios-sim --release

# Generate Swift UniFFI bindings once, then sync them to both iOS consumers.
rm -rf "$IOS_BINDINGS_STAGING_DIR"
mkdir -p "$IOS_BINDINGS_STAGING_DIR"
cargo run -p uniffi-bindgen-runner --bin uniffi-bindgen -- \
  generate "$UNIFFI_CRATE_DIR/src/mobile_uniffi.udl" \
  --language swift \
  --no-format \
  --out-dir "$IOS_BINDINGS_STAGING_DIR"

mkdir -p "$IOS_BINDINGS_DIR_APP" "$IOS_BINDINGS_DIR_SERVICE"
rsync -a --delete --exclude 'mobile_uniffi_compat.swift' "$IOS_BINDINGS_STAGING_DIR"/ "$IOS_BINDINGS_DIR_APP"/
rsync -a --delete --exclude 'mobile_uniffi_compat.swift' "$IOS_BINDINGS_STAGING_DIR"/ "$IOS_BINDINGS_DIR_SERVICE"/

# Path for compose project
COMPOSE_LIBS_DIR="${COMPOSE_ROOT}/iosApp/Libs"
mkdir -p "${COMPOSE_LIBS_DIR}"

# Copy libraries for compose project
DEVICE_LIB="${ROOT_DIR}/target/aarch64-apple-ios/release/libmetasecret_mobile.a"
SIM_ARM64_LIB="${ROOT_DIR}/target/aarch64-apple-ios-sim/release/libmetasecret_mobile.a"

cp "${DEVICE_LIB}" "${COMPOSE_LIBS_DIR}/libmetasecret_mobile-ios-arm64.a"
cp "${SIM_ARM64_LIB}" "${COMPOSE_LIBS_DIR}/libmetasecret_mobile-ios-simulator-arm64.a"

echo ""
echo "✅ iOS libraries ready:"
echo "   📦 Device:   ${COMPOSE_LIBS_DIR}/libmetasecret_mobile-ios-arm64.a"
echo "   📦 Simulator: ${COMPOSE_LIBS_DIR}/libmetasecret_mobile-ios-simulator-arm64.a"
echo "   🔗 UniFFI:   ${IOS_BINDINGS_DIR_APP}/mobile_uniffi.swift"
