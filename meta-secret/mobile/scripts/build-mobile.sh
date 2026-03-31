#!/usr/bin/env bash
# Single entry for mobile native builds (issue #81). Delegates to existing scripts.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
MODE="${1:-all}"
case "$MODE" in
  ios)     exec "$ROOT/build-ios.sh" ;;
  android) exec "$ROOT/build-android.sh" ;;
  all)
    "$ROOT/build-ios.sh"
    "$ROOT/build-android.sh"
    ;;
  *)
    echo "Usage: $0 [ios|android|all]" >&2
    exit 1
    ;;
esac
