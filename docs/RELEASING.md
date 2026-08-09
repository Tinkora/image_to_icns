# Release process

This document is for Tinkora maintainers. Public releases follow the
organization release policy: reviewed metadata and source, complete quality
checks, immutable assets, checksums, a software bill of materials (SBOM), and
signed provenance.

## Remote prerequisites

Before pushing a release tag, verify all of these repository controls:

- the repository is public, because artifact attestations on GitHub Free are
  available only to public repositories;
- the `release` environment exists and only permits `v*` tags; during the solo
  stage it records the publication boundary without pretending that self-review
  is independent approval;
- tag rules prevent deletion, update, and force-push of `v*` tags;
- the release commit is on protected `main` and all required checks passed;
- the current owner has explicitly authorized the tag after reviewing the exact
  commit and successful checks.

After the independent-second-owner gate is satisfied, configure the `release`
environment to require that owner's approval and verify the recovery and
revocation flow before enabling multi-maintainer publication.

## Prepare a version

1. Set the same stable Semantic Version in every Cargo package.
2. Move the applicable `CHANGELOG.md` entries into one dated version section.
3. Set the same version and release date in `CITATION.cff`.
4. Run `bash scripts/test_validate_release.sh` and all checks in
   [CONTRIBUTING.md](../CONTRIBUTING.md).
5. Review licenses, dependencies, security, privacy, compatibility, and any
   required upgrade, downgrade, rollback, or deprecation notes.
6. Obtain review of the exact release commit.
7. Create and push the immutable `vMAJOR.MINOR.PATCH` tag only after explicit
   release authorization.

The tag workflow first rejects an existing Release with the same tag. It then
runs metadata validation and the complete reusable quality workflow before
building or generating release material. A second absence check immediately
before publication prevents a concurrent or manual Release from being
overwritten.

## Release inventory

The workflow creates four MCP server archives:

- `image_to_icns_mcp-linux-x86_64.tar.gz`
- `image_to_icns_mcp-macos-arm64.tar.gz`
- `image_to_icns_mcp-macos-x86_64.tar.gz`
- `image_to_icns_mcp-windows-x86_64.zip`

Each archive has an adjacent `.sha256` file and a target-specific CycloneDX 1.5
JSON document named `<asset>.cdx.json`, which also has an adjacent `.sha256`
file. The workflow creates both SLSA provenance and CycloneDX SBOM attestations
for each archive through GitHub's artifact attestation API.

The SBOM generator is installed and run with locked inputs:

```bash
cargo install cargo-cyclonedx --version 0.5.9 --locked
SOURCE_DATE_EPOCH="$(git show -s --format=%ct HEAD)" \
  cargo --locked cyclonedx \
    --format json \
    --describe binaries \
    --target x86_64-unknown-linux-gnu \
    --target-in-filename \
    --spec-version 1.5
```

`cargo-cyclonedx` writes binary SBOMs next to their package manifests. The
release workflow selects the MCP SBOM, replaces checkout-local root references
with its Cargo package URL, and derives a deterministic UUIDv5 `serialNumber`
from the repository, release commit, and target. That serial is required by the
pinned GitHub SBOM attestation action and remains reproducible for identical
release inputs.

## Verify a published archive

Download a Release into a new directory and verify every published checksum:

```bash
release_dir="$(mktemp -d)"
gh release download v0.1.0 \
  --repo Tinkora/image_to_icns \
  --dir "${release_dir}"
(cd "${release_dir}" && sha256sum --check --strict -- ./*.sha256)
```

On macOS, replace `sha256sum` with `shasum -a 256 -c`.

Verify both signed claims for one archive and constrain the signer to this
workflow:

```bash
archive="${release_dir}/image_to_icns_mcp-linux-x86_64.tar.gz"
signer="Tinkora/image_to_icns/.github/workflows/release.yml"

gh attestation verify "${archive}" \
  --repo Tinkora/image_to_icns \
  --signer-workflow "${signer}"

gh attestation verify "${archive}" \
  --repo Tinkora/image_to_icns \
  --signer-workflow "${signer}" \
  --predicate-type https://cyclonedx.org/bom
```

Repeat attestation verification for the archive matching the consumer's
platform. Inspect the adjacent CycloneDX JSON when dependency inventory matters;
the signed SBOM attestation binds that document to the archive digest.

## Failure and rollback

Published tags and assets are immutable. Do not rerun the workflow to replace
an archive, move the tag, or use an upload overwrite option. Fix a broken build
or release note in source and publish a higher version that supersedes it.

If an artifact is compromised, remove access only as needed to protect users,
publish an incident notice that does not expose sensitive investigation data,
and issue a replacement version. Contract changes must include explicit
upgrade, downgrade, rollback, and deprecation notes in the release candidate.
