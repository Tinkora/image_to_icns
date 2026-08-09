#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
validator="${script_dir}/validate_release.sh"
release_workflow="${script_dir}/../.github/workflows/release.yml"
fixture_root="$(mktemp -d)"
trap 'rm -rf "${fixture_root}"' EXIT

write_package() {
  local package_dir="$1"
  local package_name="$2"
  local package_version="$3"

  mkdir -p "${package_dir}/src"
  cat > "${package_dir}/Cargo.toml" <<EOF
[package]
name = "${package_name}"
version = "${package_version}"
edition = "2021"
EOF
  : > "${package_dir}/src/lib.rs"
}

reset_fixture() {
  rm -rf "${fixture_root:?}/"*
  mkdir -p "${fixture_root}/crates"

  cat > "${fixture_root}/Cargo.toml" <<'EOF'
[workspace]
members = [
  "crates/image_to_icns_core",
  "crates/image_to_icns_web",
  "crates/image_to_icns_mcp",
]
exclude = ["crates/image_to_icns_worker"]
resolver = "2"
EOF

  write_package "${fixture_root}/crates/image_to_icns_core" "image_to_icns_core" "0.1.0"
  write_package "${fixture_root}/crates/image_to_icns_web" "image_to_icns_web" "0.1.0"
  write_package "${fixture_root}/crates/image_to_icns_mcp" "image_to_icns_mcp" "0.1.0"
  write_package "${fixture_root}/crates/image_to_icns_worker" "image_to_icns_worker" "0.1.0"

  cat > "${fixture_root}/CHANGELOG.md" <<'EOF'
# Changelog

## [0.1.0] - 2026-08-09
EOF

  cat > "${fixture_root}/CITATION.cff" <<'EOF'
cff-version: 1.2.0
title: image_to_icns
version: 0.1.0
date-released: '2026-08-09'
EOF

  cargo generate-lockfile --quiet --manifest-path "${fixture_root}/Cargo.toml"
  cargo generate-lockfile --quiet --manifest-path \
    "${fixture_root}/crates/image_to_icns_worker/Cargo.toml"
}

expect_failure() {
  local expected_message="$1"
  shift
  local output

  if output="$("$@" 2>&1)"; then
    printf 'Expected command to fail: %s\n' "$*" >&2
    exit 1
  fi
  if [[ "${output}" != *"${expected_message}"* ]]; then
    printf 'Expected failure containing %q, got:\n%s\n' \
      "${expected_message}" "${output}" >&2
    exit 1
  fi
}

reset_fixture
"${validator}" v0.1.0 "${fixture_root}"

expect_failure "stable SemVer" \
  "${validator}" v01.1.0 "${fixture_root}"

reset_fixture
sed -i.bak 's/version = "0.1.0"/version = "0.2.0"/' \
  "${fixture_root}/crates/image_to_icns_mcp/Cargo.toml"
rm "${fixture_root}/crates/image_to_icns_mcp/Cargo.toml.bak"
cargo generate-lockfile --quiet --manifest-path "${fixture_root}/Cargo.toml"
expect_failure "Cargo package versions" \
  "${validator}" v0.1.0 "${fixture_root}"

reset_fixture
sed -i.bak 's/\[0.1.0\]/[0.2.0]/' "${fixture_root}/CHANGELOG.md"
rm "${fixture_root}/CHANGELOG.md.bak"
expect_failure "CHANGELOG.md" \
  "${validator}" v0.1.0 "${fixture_root}"

reset_fixture
sed -i.bak 's/version: 0.1.0/version: 0.2.0/' "${fixture_root}/CITATION.cff"
rm "${fixture_root}/CITATION.cff.bak"
expect_failure "CITATION.cff version" \
  "${validator}" v0.1.0 "${fixture_root}"

reset_fixture
sed -i.bak 's/2026-08-09/2026-08-10/' "${fixture_root}/CITATION.cff"
rm "${fixture_root}/CITATION.cff.bak"
expect_failure "CITATION.cff date-released" \
  "${validator}" v0.1.0 "${fixture_root}"

WORKFLOW_PATH="${release_workflow}" ruby -ryaml <<'RUBY'
workflow_path = ENV.fetch("WORKFLOW_PATH")
workflow = YAML.safe_load_file(workflow_path, aliases: false)
jobs = workflow.fetch("jobs")

def assert_contract(condition, message)
  abort "Release workflow contract failed: #{message}" unless condition
end

assert_contract(
  workflow.fetch("permissions") == { "contents" => "read" },
  "workflow default permissions must be contents: read"
)

concurrency = workflow.fetch("concurrency")
assert_contract(
  concurrency.fetch("cancel-in-progress") == false,
  "release runs must serialize without cancellation"
)
assert_contract(
  concurrency.fetch("group").include?("github.ref_name"),
  "release concurrency must be scoped by tag"
)

triggers = workflow.key?("on") ? workflow.fetch("on") : workflow.fetch(true)
dispatch_input = triggers.dig("workflow_dispatch", "inputs", "release_tag")
assert_contract(
  dispatch_input.is_a?(Hash) &&
    dispatch_input.fetch("required") == true &&
    dispatch_input.fetch("type") == "string",
  "manual release canaries must require an explicit release_tag"
)
assert_contract(
  workflow.dig("env", "RELEASE_TAG").to_s.include?("inputs.release_tag") &&
    workflow.dig("env", "RELEASE_TAG").to_s.include?("github.ref_name"),
  "release metadata must resolve consistently for tag pushes and canaries"
)

expected_job_permissions = {
  "preflight" => { "contents" => "read" },
  "validate" => { "contents" => "read" },
  "quality" => { "contents" => "read" },
  "build" => { "contents" => "read" },
  "sbom" => { "contents" => "read" },
  "attest" => {
    "contents" => "read",
    "id-token" => "write",
    "attestations" => "write"
  },
  "verify" => { "contents" => "read" },
  "release" => { "contents" => "write" }
}

assert_contract(
  jobs.keys.sort == expected_job_permissions.keys.sort,
  "release workflow job inventory changed"
)
expected_job_permissions.each do |job_name, permissions|
  assert_contract(
    jobs.fetch(job_name).fetch("permissions") == permissions,
    "#{job_name} permissions are not least-privilege"
  )
end

content_writers = jobs.filter_map do |job_name, job|
  job_name if job.fetch("permissions").fetch("contents", nil) == "write"
end
assert_contract(
  content_writers == ["release"],
  "only the final release job may write repository contents"
)
assert_contract(
  jobs.fetch("release").fetch("environment") == "release",
  "publication must use the dedicated release environment"
)

quality_prerequisites = %w[preflight validate quality].sort
%w[build sbom].each do |job_name|
  assert_contract(
    jobs.fetch(job_name).fetch("needs").sort == quality_prerequisites,
    "#{job_name} must depend on preflight, metadata, and quality gates"
  )
end
assert_contract(
  jobs.fetch("attest").fetch("needs").sort == %w[build sbom],
  "attestations must use completed archives and SBOMs"
)
assert_contract(
  jobs.fetch("verify").fetch("needs") == "attest",
  "asset verification must depend on all attestations"
)
assert_contract(
  jobs.fetch("release").fetch("needs") == "verify",
  "publication must depend on the complete asset verification"
)
assert_contract(
  jobs.fetch("release").fetch("if") ==
    "${{ github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v') }}",
  "only a v* tag push may publish a release"
)

verify_steps = jobs.fetch("verify").fetch("steps")
verify_download = verify_steps.find do |step|
  step["uses"]&.start_with?("actions/download-artifact@")
end
assert_contract(
  verify_download&.dig("with", "path") == "release-assets" &&
    verify_download&.dig("with", "merge-multiple") == true,
  "asset verification must merge every build and SBOM artifact"
)
verify_run = verify_steps.find { |step| step["name"] == "Verify checksums" }&.fetch("run", "")
assert_contract(
  verify_run.include?('"${asset_count}" != 16') &&
    verify_run.include?("sha256sum --check --strict -- ./*.sha256"),
  "asset verification must require 16 files and validate every checksum"
)

release_steps = jobs.fetch("release").fetch("steps")
release_download = release_steps.find do |step|
  step["uses"]&.start_with?("actions/download-artifact@")
end
assert_contract(
  release_download&.dig("with", "path") == "release-assets" &&
    release_download&.dig("with", "merge-multiple") == true,
  "publication must download the complete verified artifact set"
)
release_verify_run = release_steps.find do |step|
  step["name"] == "Reverify checksums"
end&.fetch("run", "")
assert_contract(
  release_verify_run.include?('"${asset_count}" != 16') &&
    release_verify_run.include?("sha256sum --check --strict -- ./*.sha256"),
  "publication must independently reverify all 16 release files"
)

expected_archives = %w[
  image_to_icns_mcp-linux-x86_64.tar.gz
  image_to_icns_mcp-macos-arm64.tar.gz
  image_to_icns_mcp-macos-x86_64.tar.gz
  image_to_icns_mcp-windows-x86_64.zip
].sort
build_archives = jobs.fetch("build").dig("strategy", "matrix", "include")
  .map { |entry| entry.fetch("archive") }
  .sort
attest_archives = jobs.fetch("attest").dig("strategy", "matrix", "include")
  .map { |entry| entry.fetch("archive") }
  .sort
assert_contract(build_archives == expected_archives, "build matrix must cover four archives")
assert_contract(attest_archives == expected_archives, "attest matrix must cover four archives")

remote_actions = jobs.values.flat_map { |job| job.fetch("steps", []) }
  .filter_map { |step| step["uses"] }
  .reject { |action| action.start_with?("./") }
remote_actions.each do |action|
  assert_contract(
    action.match?(%r{\Aactions/[A-Za-z0-9_.-]+@[0-9a-f]{40}\z}),
    "remote action is not GitHub-owned and pinned to a full SHA: #{action}"
  )
end

provenance_action =
  "actions/attest-build-provenance@977bb373ede98d70efdf65b84cb5f73e068dcc2a"
sbom_action =
  "actions/attest-sbom@4651f806c01d8637787e274ac3bdf724ef169f34"
assert_contract(
  remote_actions.count(provenance_action) == 1,
  "expected pinned build provenance action is missing"
)
assert_contract(
  remote_actions.count(sbom_action) == 1,
  "expected pinned SBOM attestation action is missing"
)

workflow_text = File.read(workflow_path, encoding: "UTF-8")
assert_contract(!workflow_text.include?("--clobber"), "release assets must never be overwritten")
assert_contract(!workflow_text.include?("gh release upload"), "existing releases must not be updated")
assert_contract(
  workflow_text.scan("/releases/tags/").length == 2,
  "release absence must be checked before building and before publishing"
)
assert_contract(
  workflow_text.include?("cargo install cargo-cyclonedx --version 0.5.9 --locked"),
  "cargo-cyclonedx installation must be versioned and locked"
)
assert_contract(
  workflow_text.include?("cargo --locked cyclonedx") &&
    workflow_text.include?("--describe binaries") &&
    workflow_text.include?("--spec-version 1.5"),
  "SBOM generation must use the locked binary-target CycloneDX 1.5 command"
)
assert_contract(
  workflow_text.include?("uuid.uuid5") &&
    workflow_text.include?(".serialNumber = $serial_number"),
  "CycloneDX serial numbers must be deterministic and attestation-compatible"
)
assert_contract(
  workflow_text.include?(".cdx.json.sha256"),
  "published SBOMs must have SHA-256 checksum files"
)
publish_run = release_steps.find { |step| step["name"] == "Publish release" }&.fetch("run", "")
create_index = publish_run.index("gh release create")
edit_index = publish_run.index("gh release edit")
assert_contract(
  create_index && edit_index && create_index < edit_index &&
    publish_run.include?("--draft") &&
    publish_run.include?("--generate-notes") &&
    publish_run.include?("--verify-tag"),
  "publication must create a verified-tag draft before attaching assets"
)
assert_contract(
  publish_run.include?("gh release edit") &&
    publish_run.include?("--draft=false"),
  "publication must publish only after the complete draft is assembled"
)

puts "Release workflow supply-chain contract passed."
RUBY

printf 'Release validation tests passed.\n'
