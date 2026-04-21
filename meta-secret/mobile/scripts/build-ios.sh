#!/bin/bash
set -e

export IPHONEOS_DEPLOYMENT_TARGET=16.0
cd "$(dirname "$0")/.."

ROOT_DIR="$(cd .. && pwd)"
OUT_DIR="${ROOT_DIR}/target/ios"
mkdir -p "${OUT_DIR}"

echo "🔨 Installing Rust iOS targets..."
rustup target add aarch64-apple-ios aarch64-apple-ios-sim

echo "📱 Compiling for iOS device (arm64)..."
cargo build --package mobile-uniffi --target aarch64-apple-ios --release

echo "📱 Compiling for iOS simulator (arm64 Apple Silicon)..."
cargo build --package mobile-uniffi --target aarch64-apple-ios-sim --release

# Path for compose project
COMPOSE_LIBS_DIR="${ROOT_DIR}/../../meta-secret-compose/iosApp/Libs"
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
