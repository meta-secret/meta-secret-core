#!/bin/bash
set -e

cd "$(dirname "$0")/.."

if ! command -v cbindgen &> /dev/null; then
    echo "Installing cbindgen..."
    cargo install cbindgen
fi

echo "Adding Android targets..."
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android

if [ -z "$ANDROID_NDK_HOME" ]; then
    echo "ANDROID_NDK_HOME not set. Searching..."

    POSSIBLE_NDKS=(
        "$HOME/Library/Android/sdk/ndk"
        "$HOME/Android/Sdk/ndk"
    )

    for path in "${POSSIBLE_NDKS[@]}"; do
        if [ -d "$path" ]; then
            latest_ndk=$(ls -d "$path"/* | sort -V | tail -n 1)
            export ANDROID_NDK_HOME="$latest_ndk"
            echo "Found NDK at: $ANDROID_NDK_HOME"
            break
        fi
    done

    if [ -z "$ANDROID_NDK_HOME" ]; then
        echo "❌ NDK not found. Please install it or set ANDROID_NDK_HOME."
        exit 1
    fi
fi

CLANG_AARCH64="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/aarch64-linux-android29-clang"
CLANG_ARMV7="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/armv7a-linux-androideabi29-clang" 
CLANG_I686="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/i686-linux-android29-clang"
CLANG_X86_64="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/x86_64-linux-android29-clang"

if [ ! -f "$CLANG_AARCH64" ]; then
    echo "Searching for aarch64 clang..."
    CLANG_AARCH64=$(find "$ANDROID_NDK_HOME" -name "aarch64-linux-android*-clang" | head -n 1)
    if [ -z "$CLANG_AARCH64" ]; then
        echo "❌ Could not find aarch64-linux-android clang. Please check your NDK installation."
        exit 1
    fi
    echo "Found: $CLANG_AARCH64"
fi

if [ ! -f "$CLANG_ARMV7" ]; then
    echo "Searching for armv7 clang..."
    CLANG_ARMV7=$(find "$ANDROID_NDK_HOME" -name "armv7a-linux-androideabi*-clang" | head -n 1)
    if [ -z "$CLANG_ARMV7" ]; then
        echo "❌ Could not find armv7a-linux-androideabi clang. Please check your NDK installation."
        exit 1
    fi
    echo "Found: $CLANG_ARMV7"
fi

if [ ! -f "$CLANG_I686" ]; then
    echo "Searching for i686 clang..."
    CLANG_I686=$(find "$ANDROID_NDK_HOME" -name "i686-linux-android*-clang" | head -n 1)
    if [ -z "$CLANG_I686" ]; then
        echo "❌ Could not find i686-linux-android clang. Please check your NDK installation."
        exit 1
    fi
    echo "Found: $CLANG_I686"
fi

if [ ! -f "$CLANG_X86_64" ]; then
    echo "Searching for x86_64 clang..."
    CLANG_X86_64=$(find "$ANDROID_NDK_HOME" -name "x86_64-linux-android*-clang" | head -n 1)
    if [ -z "$CLANG_X86_64" ]; then
        echo "❌ Could not find x86_64-linux-android clang. Please check your NDK installation."
        exit 1
    fi
    echo "Found: $CLANG_X86_64"
fi

export PATH=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin:$PATH
export AR_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-ar"
export CC_aarch64_linux_android="$CLANG_AARCH64"
export CXX_aarch64_linux_android="${CLANG_AARCH64}++"

export AR_armv7_linux_androideabi="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-ar"
export CC_armv7_linux_androideabi="$CLANG_ARMV7"
export CXX_armv7_linux_androideabi="${CLANG_ARMV7}++"

export AR_i686_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-ar"
export CC_i686_linux_android="$CLANG_I686"
export CXX_i686_linux_android="${CLANG_I686}++"

export AR_x86_64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin/llvm-ar"
export CC_x86_64_linux_android="$CLANG_X86_64"
export CXX_x86_64_linux_android="${CLANG_X86_64}++"

export CARGO_BUILD_RUSTFLAGS="-C target-feature=+crt-static"
export CARGO_HTTP_CHECK_REVOKE=false
export CARGO_TERM_PROGRESS_WHEN=never

ROOT_DIR="$(cd .. && pwd)"

JNILIBS_DIR="${ROOT_DIR}/target/android/jniLibs"
mkdir -p "$JNILIBS_DIR"

echo "🛠 Building for Android targets..."

cd android

HEADER_FILE="${ROOT_DIR}/target/android/metasecret-mobile.h"
echo "📄 Generating header file: $HEADER_FILE"
mkdir -p "$(dirname "$HEADER_FILE")"
cbindgen --crate mobile-android --output "$HEADER_FILE" --lang c

echo "Building for aarch64-linux-android (arm64-v8a)..."
cargo build --package mobile-android --target aarch64-linux-android --release --features mobile_only

mkdir -p "$JNILIBS_DIR/arm64-v8a"
echo "Creating shared library for arm64-v8a..."
$CLANG_AARCH64 -shared -o "$JNILIBS_DIR/arm64-v8a/libmetasecret_mobile.so" -Wl,--whole-archive "${ROOT_DIR}/target/aarch64-linux-android/release/libmobile.a" -Wl,--no-whole-archive -lm
echo "Done building for aarch64-linux-android"

echo "Building for armv7-linux-androideabi (armeabi-v7a)..."
cargo build --package mobile-android --target armv7-linux-androideabi --release --features mobile_only

mkdir -p "$JNILIBS_DIR/armeabi-v7a"
echo "Creating shared library for armeabi-v7a..."
$CLANG_ARMV7 -shared -o "$JNILIBS_DIR/armeabi-v7a/libmetasecret_mobile.so" -Wl,--whole-archive "${ROOT_DIR}/target/armv7-linux-androideabi/release/libmobile.a" -Wl,--no-whole-archive -lm
echo "Done building for armv7-linux-androideabi"

echo "Building for i686-linux-android (x86)..."
cargo build --package mobile-android --target i686-linux-android --release --features mobile_only

mkdir -p "$JNILIBS_DIR/x86"
echo "Creating shared library for x86..."
$CLANG_I686 -shared -o "$JNILIBS_DIR/x86/libmetasecret_mobile.so" -Wl,--whole-archive "${ROOT_DIR}/target/i686-linux-android/release/libmobile.a" -Wl,--no-whole-archive -lm
echo "Done building for i686-linux-android"

echo "Building for x86_64-linux-android (x86_64)..."
cargo build --package mobile-android --target x86_64-linux-android --release --features mobile_only

mkdir -p "$JNILIBS_DIR/x86_64"
echo "Creating shared library for x86_64..."
$CLANG_X86_64 -shared -o "$JNILIBS_DIR/x86_64/libmetasecret_mobile.so" -Wl,--whole-archive "${ROOT_DIR}/target/x86_64-linux-android/release/libmobile.a" -Wl,--no-whole-archive -lm
echo "Done building for x86_64-linux-android"

echo "✅ Done!"
echo "Shared libraries are in: $JNILIBS_DIR/*"
echo "Header file is in: $HEADER_FILE"
