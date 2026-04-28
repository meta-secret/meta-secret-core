#!/usr/bin/env bash
set -euo pipefail

BASE_SHA="${BASE_SHA:-}"
HEAD_SHA="${HEAD_SHA:-HEAD}"
VERSION_DECISION_FILE="${VERSION_DECISION_FILE:-.ai/artifacts/run/version-decision.json}"
WEB_VERSION_FILE="meta-secret/web-cli/ui/package.json"
SERVER_VERSION_FILE="meta-secret/meta-server/web-server/Cargo.toml"

if [[ -z "${BASE_SHA}" ]]; then
  echo "ERROR: BASE_SHA is required."
  exit 1
fi

if [[ ! -f "${VERSION_DECISION_FILE}" ]]; then
  echo "ERROR: Missing version decision file: ${VERSION_DECISION_FILE}"
  exit 1
fi

CHANGED_FILES=()
while IFS= read -r line; do
  CHANGED_FILES+=("${line}")
done < <(git diff --name-only "${BASE_SHA}...${HEAD_SHA}")
CHANGED_TEXT="$(printf "%s\n" "${CHANGED_FILES[@]-}")"

contains_file() {
  local target="$1"
  printf "%s\n" "${CHANGED_FILES[@]-}" | rg -q "^${target}$"
}

web_code_changed=false
server_code_changed=false
web_version_changed=false
server_version_changed=false

while IFS= read -r file; do
  [[ -z "${file}" ]] && continue
  if [[ "${file}" == "meta-secret/web-cli/ui/"* ]] && [[ "${file}" != "${WEB_VERSION_FILE}" ]]; then
    web_code_changed=true
  fi
  if [[ "${file}" == "meta-secret/meta-server/web-server/"* ]] && [[ "${file}" != "${SERVER_VERSION_FILE}" ]]; then
    server_code_changed=true
  fi
done <<< "${CHANGED_TEXT}"

if contains_file "${WEB_VERSION_FILE}"; then
  web_version_changed=true
fi
if contains_file "${SERVER_VERSION_FILE}"; then
  server_version_changed=true
fi

DECISION=()
while IFS= read -r line; do
  DECISION+=("${line}")
done < <(python3 - "${VERSION_DECISION_FILE}" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as f:
    data = json.load(f)

bump_type = data.get("bump_type", "")
rationale = data.get("bump_rationale", "")
targets = data.get("target_version_files", [])
exclusion = data.get("single_target_exclusion_reason", "")

if not isinstance(targets, list):
    targets = []

print(bump_type)
print(rationale)
print("|".join(targets))
print(exclusion)
PY
)

BUMP_TYPE="${DECISION[0]:-}"
BUMP_RATIONALE="${DECISION[1]:-}"
TARGETS_PIPE="${DECISION[2]:-}"
EXCLUSION_REASON="${DECISION[3]:-}"

case "${BUMP_TYPE}" in
  patch|minor|major) ;;
  *)
    echo "ERROR: bump_type must be one of patch/minor/major."
    exit 1
    ;;
esac

if [[ -z "${BUMP_RATIONALE}" ]]; then
  echo "ERROR: bump_rationale must be provided."
  exit 1
fi

target_has_web=false
target_has_server=false
if printf "%s" "${TARGETS_PIPE}" | rg -q "(^|\|)${WEB_VERSION_FILE}(\||$)"; then
  target_has_web=true
fi
if printf "%s" "${TARGETS_PIPE}" | rg -q "(^|\|)${SERVER_VERSION_FILE}(\||$)"; then
  target_has_server=true
fi

if [[ "${target_has_web}" == false && "${target_has_server}" == false ]]; then
  echo "ERROR: target_version_files must include at least one version file."
  exit 1
fi

if [[ "${web_code_changed}" == true && "${server_code_changed}" == true ]]; then
  if [[ "${target_has_web}" == false || "${target_has_server}" == false ]]; then
    echo "ERROR: both web and server code changed, both version targets are required."
    exit 1
  fi
fi

if [[ "${target_has_web}" == true && "${web_version_changed}" == false ]]; then
  echo "ERROR: ${WEB_VERSION_FILE} is listed in target_version_files but not changed."
  exit 1
fi
if [[ "${target_has_server}" == true && "${server_version_changed}" == false ]]; then
  echo "ERROR: ${SERVER_VERSION_FILE} is listed in target_version_files but not changed."
  exit 1
fi

if [[ "${target_has_web}" == false && "${web_version_changed}" == true ]]; then
  echo "ERROR: ${WEB_VERSION_FILE} changed but not listed in target_version_files."
  exit 1
fi
if [[ "${target_has_server}" == false && "${server_version_changed}" == true ]]; then
  echo "ERROR: ${SERVER_VERSION_FILE} changed but not listed in target_version_files."
  exit 1
fi

if [[ ("${target_has_web}" == false || "${target_has_server}" == false) && -z "${EXCLUSION_REASON}" ]]; then
  echo "ERROR: single_target_exclusion_reason is required when only one target is bumped."
  exit 1
fi

check_semver_increment() {
  local file="$1"
  local kind="$2"
  local old_version="$3"
  local new_version="$4"

  python3 - "${file}" "${kind}" "${old_version}" "${new_version}" <<'PY'
import re
import sys

file, kind, old_v, new_v = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
pattern = re.compile(r"^(\d+)\.(\d+)\.(\d+)$")

old_m = pattern.match(old_v)
new_m = pattern.match(new_v)
if not old_m or not new_m:
    print(f"ERROR: {file} has non-SemVer version format ({old_v} -> {new_v}).")
    sys.exit(1)

old = tuple(int(x) for x in old_m.groups())
new = tuple(int(x) for x in new_m.groups())

if kind == "patch":
    ok = (new[0] == old[0] and new[1] == old[1] and new[2] > old[2])
elif kind == "minor":
    ok = (new[0] == old[0] and new[1] > old[1])
elif kind == "major":
    ok = (new[0] > old[0])
else:
    ok = False

if not ok:
    print(f"ERROR: {file} version bump does not match {kind} ({old_v} -> {new_v}).")
    sys.exit(1)
PY
}

extract_json_version() {
  local ref="$1"
  local path="$2"
  git show "${ref}:${path}" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("version",""))'
}

extract_toml_version() {
  local ref="$1"
  local path="$2"
  git show "${ref}:${path}" | python3 -c 'import re,sys; text=sys.stdin.read(); m=re.search(r"^\s*version\s*=\s*\"([^\"]+)\"", text, re.MULTILINE); print(m.group(1) if m else "")'
}

if [[ "${web_version_changed}" == true ]]; then
  web_old="$(extract_json_version "${BASE_SHA}" "${WEB_VERSION_FILE}")"
  web_new="$(extract_json_version "${HEAD_SHA}" "${WEB_VERSION_FILE}")"
  if [[ -z "${web_old}" || -z "${web_new}" ]]; then
    echo "ERROR: Cannot read version from ${WEB_VERSION_FILE}."
    exit 1
  fi
  check_semver_increment "${WEB_VERSION_FILE}" "${BUMP_TYPE}" "${web_old}" "${web_new}"
fi

if [[ "${server_version_changed}" == true ]]; then
  server_old="$(extract_toml_version "${BASE_SHA}" "${SERVER_VERSION_FILE}")"
  server_new="$(extract_toml_version "${HEAD_SHA}" "${SERVER_VERSION_FILE}")"
  if [[ -z "${server_old}" || -z "${server_new}" ]]; then
    echo "ERROR: Cannot read version from ${SERVER_VERSION_FILE}."
    exit 1
  fi
  check_semver_increment "${SERVER_VERSION_FILE}" "${BUMP_TYPE}" "${server_old}" "${server_new}"
fi

if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  {
    echo "## Versioning Check"
    echo ""
    echo "- bump_type: ${BUMP_TYPE}"
    echo "- web_code_changed: ${web_code_changed}"
    echo "- server_code_changed: ${server_code_changed}"
    echo "- web_version_changed: ${web_version_changed}"
    echo "- server_version_changed: ${server_version_changed}"
    echo "- decision_file: ${VERSION_DECISION_FILE}"
  } >> "${GITHUB_STEP_SUMMARY}"
fi

echo "Versioning check passed."
