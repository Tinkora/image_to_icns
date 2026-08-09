#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'Release validation failed: %s\n' "$1" >&2
  exit 1
}

tag="${1:-}"
repo_root="${2:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"

if [[ ! "${tag}" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  fail "tag must be stable SemVer in the form vX.Y.Z without leading zeroes"
fi

if [[ ! -d "${repo_root}" ]]; then
  fail "repository root does not exist: ${repo_root}"
fi

for command_name in cargo jq ruby; do
  if ! command -v "${command_name}" >/dev/null 2>&1; then
    fail "required command is unavailable: ${command_name}"
  fi
done

cd "${repo_root}"
version="${tag#v}"

workspace_versions="$({
  cargo metadata --locked --no-deps --format-version 1 \
    --manifest-path Cargo.toml
  cargo metadata --locked --no-deps --format-version 1 \
    --manifest-path crates/image_to_icns_worker/Cargo.toml
} | jq -r '.packages[].version' | sort -u)"

if [[ "${workspace_versions}" != "${version}" ]]; then
  fail "Cargo package versions must all equal ${version}; found: ${workspace_versions//$'\n'/, }"
fi

escaped_version="${version//./\\.}"
changelog_lines="$(
  grep -E "^## \\[${escaped_version}\\] - [0-9]{4}-[0-9]{2}-[0-9]{2}$" \
    CHANGELOG.md || true
)"
changelog_count="$(
  printf '%s\n' "${changelog_lines}" | sed '/^$/d' | wc -l | tr -d ' '
)"

if [[ "${changelog_count}" != "1" ]]; then
  fail "CHANGELOG.md must contain exactly one dated section for ${version}"
fi

release_date="${changelog_lines##* - }"
if ! ruby -rdate -e \
  'value = ARGV.fetch(0); abort unless Date.iso8601(value).to_s == value' \
  "${release_date}"; then
  fail "CHANGELOG.md release date is not a valid ISO date: ${release_date}"
fi

if ! citation_values="$(ruby -ryaml -rdate -e '
  data = YAML.safe_load_file(ARGV.fetch(0), aliases: false)
  version = data.fetch("version").to_s
  release_date = data.fetch("date-released").to_s
  abort unless Date.iso8601(release_date).to_s == release_date
  print [version, release_date].join("\t")
' CITATION.cff)"; then
  fail "CITATION.cff must contain a version and valid ISO date-released"
fi

IFS=$'\t' read -r citation_version citation_date <<< "${citation_values}"
if [[ "${citation_version}" != "${version}" ]]; then
  fail "CITATION.cff version must equal ${version}; found: ${citation_version}"
fi
if [[ "${citation_date}" != "${release_date}" ]]; then
  fail "CITATION.cff date-released must equal ${release_date}; found: ${citation_date}"
fi

printf 'Release metadata is consistent for %s (%s).\n' "${tag}" "${release_date}"
