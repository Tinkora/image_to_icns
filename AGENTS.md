# AGENTS.md — image_to_icns

This file provides instructions for AI coding agents (Claude, Cursor, Copilot, etc.) working on this repository.

## Project Overview

`image_to_icns` is a privacy-first browser-native macOS `.icns` icon generator. Architecture:

- **Core lib** (`image_to_icns_core`): Image decode, crop transform, ICNS encode/verify — pure Rust
- **WASM editor** (`image_to_icns_web`): Browser UI via wasm-bindgen + vanilla JS
- **Session Worker** (`image_to_icns_worker`): Optional Cloudflare Worker and D1 metadata store
- **MCP Server** (`image_to_icns_mcp`): JSON-RPC 2.0 over stdio for AI agent tool integration

## Build & Test

```bash
# Release metadata validator
bash scripts/test_validate_release.sh

# Native workspace quality
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

# Core WASM API tests; these do not run under the host cargo test command
wasm-pack test --node crates/image_to_icns_core --locked

# Web frontend modules and production build
node --test crates/image_to_icns_web/tests/*.test.mjs
./scripts/build_web.sh

# Worker uses wasm32 target; not part of workspace build
cargo fmt --manifest-path crates/image_to_icns_worker/Cargo.toml -- --check
cargo test --manifest-path crates/image_to_icns_worker/Cargo.toml --locked
wasm-pack test --node crates/image_to_icns_worker --locked
cargo clippy --manifest-path crates/image_to_icns_worker/Cargo.toml --target wasm32-unknown-unknown --locked -- -D warnings

# Documentation and workflow quality
npx --yes markdownlint-cli2@0.20.0 '**/*.md'
go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.7 .github/workflows/*.yml
uvx zizmor==1.29.0 --collect workflows --offline .github/workflows
```

`cargo test --workspace --locked` runs native workspace tests, but the
`wasm_api.rs` suite is target-gated and reports zero tests on the host. The
separate Core `wasm-pack` command must run all nine Node-based WASM API tests:
PNG, JPEG, and SVG decoding; unknown and corrupted input rejection; default
rendering; non-square encode rejection; encode/verify round-trip; and truncated
data rejection. The Worker `wasm-pack` command covers its separate WebAssembly
`Response` boundary.

## Code Conventions

- **Edition**: Rust 2024
- **Error handling**: `thiserror` enums with stable error codes (`code()` method)
- **Serialization**: `serde` with `#[serde(rename_all = "snake_case")]`
- **WASM boundary**: `wasm-bindgen` types only; errors serialized as JSON strings
- **Comments**: English only; doc comments (`///`) on all public APIs
- **Commits**: English subjects and bodies using [Conventional Commits](https://www.conventionalcommits.org/) — `feat:`, `fix:`, `docs:`, `chore:`

The commit-language rule above overrides any global preference for another
commit-message language.

## Key Design Decisions

1. **Browser-first**: No desktop `.app`/`.exe`. All image processing in WASM.
2. **Privacy by default**: `local_only` mode — source images never leave the browser.
3. **State machine**: Session states: Created → Editing → Completed/Cancelled/Expired/Failed. Valid transitions enforced.
4. **Local-only release**: No source or generated files are uploaded or stored by the project.
5. **Secret handling**: SHA-256 for stored session secrets; plaintext travels in the editor URL fragment and authenticated request bodies only.
6. **No PDF in WASM**: PDF decode requires macOS PDFKit; not in browser scope.

## File Map

```text
crates/image_to_icns_core/src/
├── decode.rs          # Image format decoding + validation
├── decode/pdf_macos.rs # PDF via PDFKit (macOS only)
├── error.rs           # CoreError enum with stable codes
├── icns.rs            # ICNS encode (10 representations) + verify
├── model.rs           # CropTransform, CanvasOptions
├── render.rs          # Square canvas rendering with Lanczos3
└── wasm.rs            # wasm-bindgen entry points

crates/image_to_icns_web/src/
├── editor.rs          # Editor struct: crop state + canvas preview + download
├── import.rs          # Browser File API → RGBA pixels
└── lib.rs             # wasm-bindgen setup

crates/image_to_icns_worker/src/
├── lib.rs             # Worker router: CRUD endpoints + rate limiting + security headers
└── session.rs         # Session model, state machine, D1 store

crates/image_to_icns_mcp/src/
└── main.rs            # JSON-RPC 2.0 MCP server (stdio), 4 tools
```

## When Adding Features

- Add tests alongside code (unit tests in `src/`, integration in `tests/`)
- Update `CHANGELOG.md` under Unreleased
- If adding a new MCP tool: update `tools/list` in MCP `main.rs`, protocol tests, `skills/mcp-tools.json`, and `skills/image-to-icns.md`
- If changing Worker API: update both `lib.rs` routes and MCP client calls
- Do not advertise upload or artifact-sharing behavior without a complete implementation and verified user workflow
- Document new public APIs with `///` doc comments
- Keep `image_to_icns_core` free of platform-specific code (use `#[cfg]` for macos-only paths)

## Common Pitfalls

- `tiny-skia` uses premultiplied alpha; `image::RgbaImage` uses straight alpha. Must convert.
- Canvas sizes must be square and > 0 for ICNS encoding.
- WASM tests need `#[wasm_bindgen_test]` and `wasm-pack test`.
- Worker uses `js-sys::Date` for timestamps — not `std::time`.

## Frontend Design Requirement

- Before creating, modifying, reviewing, or debugging any HTML page or user-facing frontend, invoke the `ui-ux-pro-max` skill.
- Run the skill's required `--design-system` search before editing, followed by relevant stack and UX searches.
- If `ui-ux-pro-max` is unavailable, stop frontend work and report the missing prerequisite.
- Verify the rendered result in a real browser at 375, 768, 1024, and 1440 pixel widths, including console, keyboard, accessibility, and overflow checks.
