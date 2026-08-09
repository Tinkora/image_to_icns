# Contributing to image_to_icns

## Current contribution status

Tinkora currently publishes this repository for independent use and evaluation.
Public contribution intake is not open yet: Issues and Discussions are disabled,
and maintainers are not accepting public pull requests until the documented
public-interaction gate is complete.

Do not open an Issue, Discussion, or pull request during this stage. Report
vulnerabilities privately as described in [SECURITY.md](SECURITY.md). The
development and review rules below document the workflow that maintainers use
now and that public contributors will use after contribution intake opens.

Feature proposals should start with a real user workflow. Explain who has the
problem, how they handle it today, and why the added maintenance cost is
justified.

## Development setup

Install Rust 1.95, the WASM target, and `wasm-pack` 0.15:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --version 0.15.0 --locked
```

Run the browser editor:

```bash
./scripts/build_web.sh
python3 -m http.server 4173 --directory dist
```

The optional Worker has separate setup instructions in
[docs/SELF_HOSTING.md](docs/SELF_HOSTING.md).

## Change workflow

1. Fork the repository and create a focused branch from `main`.
2. Add outcome-focused tests for behavior changes.
3. Update public documentation and `CHANGELOG.md` when behavior changes.
4. Run the relevant quality checks below.
5. Complete the pull request template when the public contribution gate opens.

Write commit subjects and bodies in English and use
[Conventional Commits](https://www.conventionalcommits.org/), for example
`fix: preserve the MCP request id`. This repository-level rule overrides any
global preference for another commit-message language.

## Required checks

```bash
bash scripts/test_validate_release.sh

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
wasm-pack test --node crates/image_to_icns_core --locked

node --test crates/image_to_icns_web/tests/*.test.mjs
./scripts/build_web.sh

cargo fmt --manifest-path crates/image_to_icns_worker/Cargo.toml -- --check
cargo test --manifest-path crates/image_to_icns_worker/Cargo.toml --locked
wasm-pack test --node crates/image_to_icns_worker --locked
cargo clippy \
  --manifest-path crates/image_to_icns_worker/Cargo.toml \
  --target wasm32-unknown-unknown \
  --locked -- -D warnings

npx --yes markdownlint-cli2@0.20.0 '**/*.md'
go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.7 \
  .github/workflows/*.yml
uvx zizmor==1.29.0 --collect workflows --offline .github/workflows
```

The host workspace command does not execute `wasm_api.rs`, because that suite
is compiled only for `wasm32`. The Core `wasm-pack` command is therefore a
required, separate gate: it runs nine Node-based tests covering PNG, JPEG, and
SVG decoding; invalid-input rejection; rendering; ICNS encoding; and ICNS
verification. The Worker `wasm-pack` command exercises its separate
WebAssembly `Response` boundary.

Maintainers preparing a version must also follow the immutable artifact,
SBOM, provenance, approval, and consumer verification requirements in the
[release process](docs/RELEASING.md).

For browser changes, also verify the rendered editor at 375, 768, 1024, and
1440 pixel widths. Test keyboard navigation, file import, crop interaction,
generation, download, the browser console, and horizontal overflow.

## Code conventions

- Keep `image_to_icns_core` independent of browser and Worker APIs.
- Confine platform-specific behavior behind explicit `cfg` boundaries.
- Treat WASM, JSON-RPC, HTTP, file input, and D1 as untrusted boundaries.
- Use stable machine-readable error codes where an API exposes errors.
- Write code comments and public API documentation in English.
- Comment intent and invariants, not self-explanatory syntax.

## Review expectations

Maintainers evaluate correctness, privacy, accessibility, portability,
maintenance cost, and demonstrated user value. A passing test suite is
required but does not replace review of the user-facing behavior.

By participating, you agree to follow the
[Code of Conduct](CODE_OF_CONDUCT.md).
