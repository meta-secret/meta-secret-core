#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: task set-env -- local|remote" >&2
}

mode="${1:-}"
case "$mode" in
  local)
    rust_variant="Local"
    env_value="local"
    ;;
  remote)
    rust_variant="Remote"
    env_value="remote"
    ;;
  *)
    usage
    exit 2
    ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
compose_root="$(cd "$repo_root/.." && pwd)/meta-secret-compose"

replace_or_append_property() {
  local file="$1"
  local key="$2"
  local value="$3"

  touch "$file"
  if grep -q "^${key}=" "$file"; then
    perl -0pi -e "s/^${key}=.*$/${key}=${value}/m" "$file"
  else
    printf "\n%s=%s\n" "$key" "$value" >> "$file"
  fi
}

perl -0pi -e \
  "s/pub const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::(?:Local|Remote);/pub const SELECTED_SERVER_ENVIRONMENT: ServerEnvironment = ServerEnvironment::${rust_variant};/" \
  "$repo_root/meta-secret/core/src/node/app/sync/environment.rs"

replace_or_append_property "$repo_root/meta-secret/web-cli/ui/.env.local" "VITE_META_SECRET_ENV" "$env_value"

if [ -d "$compose_root" ]; then
  replace_or_append_property "$compose_root/gradle.properties" "META_SECRET_ENV" "$env_value"
  replace_or_append_property "$compose_root/iosApp/Configuration/Config.xcconfig" "META_SECRET_ENV" "$env_value"
else
  echo "compose repo not found at $compose_root; skipped mobile app env files" >&2
fi

echo "MetaSecret environment set to: $env_value"

echo "Rebuilding mobile Rust libraries..."
bash "$repo_root/meta-secret/mobile/scripts/build-mobile.sh" all
