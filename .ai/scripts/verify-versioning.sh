#!/usr/bin/env bash
set -euo pipefail

BASE_SHA="${BASE_SHA:-}"
HEAD_SHA="${HEAD_SHA:-HEAD}"
VERSION_DECISION_FILE="${VERSION_DECISION_FILE:-.ai/artifacts/run/version-decision.json}"
UNIFIED_VERSION_FILE="meta-secret/VERSION"

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
  printf "%s\n" "${CHANGED_FILES[@]-}" | grep -Eq "^${target}$"
}

web_code_changed=false
server_code_changed=false
unified_version_changed=false

while IFS= read -r file; do
  [[ -z "${file}" ]] && continue
  if [[ "${file}" == "meta-secret/web-cli/ui/"* ]] && [[ "${file}" != "${UNIFIED_VERSION_FILE}" ]]; then
    web_code_changed=true
  fi
  if [[ "${file}" == "meta-secret/meta-server/web-server/"* ]] && [[ "${file}" != "${UNIFIED_VERSION_FILE}" ]]; then
    server_code_changed=true
  fi
done <<< "${CHANGED_TEXT}"

if contains_file "${UNIFIED_VERSION_FILE}"; then
  unified_version_changed=true
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

target_has_unified=false
if printf "%s" "${TARGETS_PIPE}" | grep -Eq "(^|\\|)${UNIFIED_VERSION_FILE}(\\||$)"; then
  target_has_unified=true
fi

if [[ "${target_has_unified}" == false ]]; then
  echo "ERROR: target_version_files must include ${UNIFIED_VERSION_FILE}."
  exit 1
fi

if [[ ("${web_code_changed}" == true || "${server_code_changed}" == true) && "${unified_version_changed}" == false ]]; then
  echo "ERROR: code changed, but version file was not updated. Update ${UNIFIED_VERSION_FILE}."
  exit 1
fi

if [[ "${target_has_unified}" == true && "${unified_version_changed}" == false ]]; then
  echo "ERROR: ${UNIFIED_VERSION_FILE} is listed in target_version_files but not changed."
  exit 1
fi

if [[ "${target_has_unified}" == false && "${unified_version_changed}" == true ]]; then
  echo "ERROR: ${UNIFIED_VERSION_FILE} changed but not listed in target_version_files."
  exit 1
fi

if [[ "${EXCLUSION_REASON}" != "" ]]; then
  echo "ERROR: single_target_exclusion_reason is not used with unified versioning. Remove this field."
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

extract_text_version() {
  local ref="$1"
  local path="$2"
  if git cat-file -e "${ref}:${path}" 2>/dev/null; then
    git show "${ref}:${path}" | tr -d '[:space:]'
  else
    echo "0.0.0"
  fi
}

if [[ "${unified_version_changed}" == true ]]; then
  unified_old="$(extract_text_version "${BASE_SHA}" "${UNIFIED_VERSION_FILE}")"
  unified_new="$(extract_text_version "${HEAD_SHA}" "${UNIFIED_VERSION_FILE}")"
  if [[ -z "${unified_old}" || -z "${unified_new}" ]]; then
    echo "ERROR: Cannot read version from ${UNIFIED_VERSION_FILE}."
    exit 1
  fi
  check_semver_increment "${UNIFIED_VERSION_FILE}" "${BUMP_TYPE}" "${unified_old}" "${unified_new}"
fi

if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  {
    echo "## Versioning Check"
    echo ""
    echo "- bump_type: ${BUMP_TYPE}"
    echo "- web_code_changed: ${web_code_changed}"
    echo "- server_code_changed: ${server_code_changed}"
    echo "- unified_version_changed: ${unified_version_changed}"
    echo "- unified_version_file: ${UNIFIED_VERSION_FILE}"
    echo "- decision_file: ${VERSION_DECISION_FILE}"
  } >> "${GITHUB_STEP_SUMMARY}"
fi

echo "Versioning check passed."
