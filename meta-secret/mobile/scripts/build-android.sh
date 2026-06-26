#!/bin/bash
set -e

cd "$(dirname "$0")/.."

if [ -z "$ANDROID_NDK_HOME" ]; then
    echo "Searching for Android NDK..."
    POSSIBLE_NDKS=(
        "$HOME/Library/Android/sdk/ndk"
        "$HOME/Android/Sdk/ndk"
    )

    for path in "${POSSIBLE_NDKS[@]}"; do
        if [ -d "$path" ]; then
            latest_ndk=$(ls -d "$path"/* 2>/dev/null | sort -V | tail -n 1)
            if [ -n "$latest_ndk" ]; then
                export ANDROID_NDK_HOME="$latest_ndk"
                echo "Found NDK at: $ANDROID_NDK_HOME"
                break
            fi
        fi
    done

    if [ -z "$ANDROID_NDK_HOME" ]; then
        echo "❌ NDK not found. Set ANDROID_NDK_HOME or install Android NDK"
        exit 1
    fi
fi

echo "🔨 Installing Rust Android targets..."
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android

# Detect host OS for NDK prebuilt path
if [[ "$OSTYPE" == "darwin"* ]]; then
    NDK_PREBUILT="darwin-x86_64"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    NDK_PREBUILT="linux-x86_64"
else
    echo "❌ Unsupported OS: $OSTYPE"
    exit 1
fi

# Setup NDK toolchain
LLVM_AR="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$NDK_PREBUILT/bin/llvm-ar"
export PATH=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$NDK_PREBUILT/bin:$PATH

# Find clang compilers in llvm/prebuilt
LLVM_BIN_DIR="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$NDK_PREBUILT/bin"

# Find latest API level clang (NDK has versioned clang: aarch64-linux-android34-clang, etc)
find_clang() {
    local pattern="$1"
    # Find all matching clang versions and pick the latest one
    local clang=$(ls -1 "$LLVM_BIN_DIR"/${pattern}*-clang 2>/dev/null | sort -V | tail -n 1)
    echo "$clang"
}

CLANG_AARCH64=$(find_clang "aarch64-linux-android")
CLANG_ARMV7=$(find_clang "armv7a-linux-androideabi")
CLANG_X86_64=$(find_clang "x86_64-linux-android")

# Verify compilers exist
if [ -z "$CLANG_AARCH64" ] || [ -z "$CLANG_ARMV7" ] || [ -z "$CLANG_X86_64" ]; then
    echo "❌ Could not find clang compilers in: $LLVM_BIN_DIR"
    echo "   Available: $(ls "$LLVM_BIN_DIR"/*-clang 2>/dev/null | head -3)"
    exit 1
fi

# Set up environment variables
export AR_aarch64_linux_android="$LLVM_AR"
export CC_aarch64_linux_android="$CLANG_AARCH64"
export AR_armv7_linux_androideabi="$LLVM_AR"
export CC_armv7_linux_androideabi="$CLANG_ARMV7"
export AR_x86_64_linux_android="$LLVM_AR"
export CC_x86_64_linux_android="$CLANG_X86_64"

export CARGO_BUILD_RUSTFLAGS="-C target-feature=+crt-static"

ROOT_DIR="$(cd .. && pwd)"
COMPOSE_ROOT="${ROOT_DIR}/../../meta-secret-compose"
UNIFFI_CRATE_DIR="${ROOT_DIR}/uniffi"
ANDROID_BINDINGS_DIR="${COMPOSE_ROOT}/composeApp/src/androidMain/kotlin/com/metasecret/core/uniffi"

echo "🤖 Compiling for Android arm64-v8a (aarch64)..."
cargo build --package mobile-uniffi --target aarch64-linux-android --release

echo "🤖 Compiling for Android armeabi-v7a (armv7)..."
cargo build --package mobile-uniffi --target armv7-linux-androideabi --release

echo "🤖 Compiling for Android x86_64..."
cargo build --package mobile-uniffi --target x86_64-linux-android --release

# Generate Kotlin UniFFI bindings for the Android consumer.
mkdir -p "$ANDROID_BINDINGS_DIR"
cargo run -p uniffi-bindgen-runner --bin uniffi-bindgen -- \
  generate "$UNIFFI_CRATE_DIR/src/mobile_uniffi.udl" \
  --language kotlin \
  --no-format \
  --out-dir "$ANDROID_BINDINGS_DIR"

# Convert .a to .so and copy to compose project
COMPOSE_JNILIBS="${COMPOSE_ROOT}/composeApp/src/androidMain/jniLibs"

build_so() {
    local triple="$1"
    local abi="$2"
    local clang="$3"

    local static_lib="${ROOT_DIR}/target/${triple}/release/libmetasecret_mobile.a"
    local so_dir="${COMPOSE_JNILIBS}/${abi}"
    local so_file="${so_dir}/libmetasecret_mobile.so"

    mkdir -p "$so_dir"

    echo "  Creating $abi .so..."
    "$clang" -shared \
      -o "$so_file" \
      -Wl,--whole-archive "$static_lib" \
      -Wl,--no-whole-archive \
      -lm
}

echo ""
echo "📦 Creating .so files for compose project..."
build_so "aarch64-linux-android" "arm64-v8a" "$CLANG_AARCH64"
build_so "armv7-linux-androideabi" "armeabi-v7a" "$CLANG_ARMV7"
build_so "x86_64-linux-android" "x86_64" "$CLANG_X86_64"

echo ""
echo "✅ Android libraries ready:"
echo "   📦 arm64-v8a:   ${COMPOSE_JNILIBS}/arm64-v8a/libmetasecret_mobile.so"
echo "   📦 armeabi-v7a: ${COMPOSE_JNILIBS}/armeabi-v7a/libmetasecret_mobile.so"
echo "   📦 x86_64:      ${COMPOSE_JNILIBS}/x86_64/libmetasecret_mobile.so"
echo "   🔗 UniFFI:      ${ANDROID_BINDINGS_DIR}/uniffi/mobile_uniffi/mobile_uniffi.kt"
