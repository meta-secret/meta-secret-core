#!/bin/bash
# Build native libraries for mobile (iOS and Android)
# Automatically copies ready-to-use libraries to compose project
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MODE="${1:-all}"

case "$MODE" in
  ios)
    echo "📱 Building iOS libraries and UniFFI bindings..."
    bash "$SCRIPT_DIR/build-ios.sh"
    ;;
  android)
    echo "🤖 Building Android libraries and UniFFI bindings..."
    bash "$SCRIPT_DIR/build-android.sh"
    ;;
  all)
    echo "📱 Building iOS libraries and UniFFI bindings..."
    bash "$SCRIPT_DIR/build-ios.sh"
    echo ""
    echo "🤖 Building Android libraries and UniFFI bindings..."
    bash "$SCRIPT_DIR/build-android.sh"
    echo ""
    echo "═══════════════════════════════════════════════════════════"
    echo "✅ All native libraries and UniFFI bindings built and copied to compose project!"
    echo "═══════════════════════════════════════════════════════════"
    ;;
  *)
    echo "Usage: $0 [ios|android|all]"
    echo ""
    echo "Builds and copies ready-to-use native libraries:"
    echo "  ios     - Builds iOS .a files and Swift UniFFI bindings"
    echo "  android - Builds Android .so files and Kotlin UniFFI bindings"
    echo "  all     - Both platforms (default)"
    exit 1
    ;;
esac
