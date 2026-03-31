#!/bin/bash
set -e

cd "$(dirname "$0")/.."

# Check if rustup is installed
if ! command -v rustup &> /dev/null; then
    echo "❌ rustup not found. Please install Rust toolchain first."
    exit 1
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

# Detect host OS for NDK prebuilt path
if [[ "$OSTYPE" == "darwin"* ]]; then
    NDK_PREBUILT="darwin-x86_64"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    NDK_PREBUILT="linux-x86_64"
else
    echo "❌ Unsupported OS: $OSTYPE"
    exit 1
fi

CLANG_AARCH64="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$NDK_PREBUILT/bin/aarch64-linux-android29-clang"
CLANG_ARMV7="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$NDK_PREBUILT/bin/armv7a-linux-androideabi29-clang"
CLANG_I686="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$NDK_PREBUILT/bin/i686-linux-android29-clang"
CLANG_X86_64="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$NDK_PREBUILT/bin/x86_64-linux-android29-clang"

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

LLVM_AR="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$NDK_PREBUILT/bin/llvm-ar"

export PATH=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$NDK_PREBUILT/bin:$PATH
export AR_aarch64_linux_android="$LLVM_AR"
export CC_aarch64_linux_android="$CLANG_AARCH64"
export CXX_aarch64_linux_android="${CLANG_AARCH64}++"

export AR_armv7_linux_androideabi="$LLVM_AR"
export CC_armv7_linux_androideabi="$CLANG_ARMV7"
export CXX_armv7_linux_androideabi="${CLANG_ARMV7}++"

export AR_i686_linux_android="$LLVM_AR"
export CC_i686_linux_android="$CLANG_I686"
export CXX_i686_linux_android="${CLANG_I686}++"

export AR_x86_64_linux_android="$LLVM_AR"
export CC_x86_64_linux_android="$CLANG_X86_64"
export CXX_x86_64_linux_android="${CLANG_X86_64}++"

export CARGO_BUILD_RUSTFLAGS="-C target-feature=+crt-static"
export CARGO_HTTP_CHECK_REVOKE=false
export CARGO_TERM_PROGRESS_WHEN=never

ROOT_DIR="$(cd .. && pwd)"

JNILIBS_DIR="${ROOT_DIR}/target/android/jniLibs"
mkdir -p "$JNILIBS_DIR"

echo "🛠 Building mobile-uniffi for Android targets..."

cd "$ROOT_DIR"

copy_so() {
    local triple="$1"
    local abi="$2"
    echo "Building for $triple ($abi)..."
    cargo build --package mobile-uniffi --target "$triple" --release
    mkdir -p "$JNILIBS_DIR/$abi"
    cp "${ROOT_DIR}/target/${triple}/release/libmetasecret_mobile.so" "$JNILIBS_DIR/$abi/libmetasecret_mobile.so"
    echo "Done: $abi"
}

copy_so aarch64-linux-android arm64-v8a
copy_so armv7-linux-androideabi armeabi-v7a
copy_so i686-linux-android x86
copy_so x86_64-linux-android x86_64

echo "✅ Done!"
echo "Shared libraries are in: $JNILIBS_DIR/*"
