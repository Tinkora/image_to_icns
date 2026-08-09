# Copilot Instructions

You are working on `image_to_icns`, a Rust/WASM privacy-first macOS icon generator.

## Quick Reference

- **Language**: Rust edition 2024
- **Test**: `cargo test --workspace`
- **Lint**: `cargo clippy --workspace --all-targets -- -D warnings`
- **Format**: `cargo fmt --all`
- **Commit**: Conventional Commits (`feat:`, `fix:`, `docs:`)

Read and follow `AGENTS.md`, including its mandatory frontend design and browser
verification requirements.

## Architecture

- `image_to_icns_core/` — Pure Rust: decode, crop, ICNS codec. Keep platform-agnostic.
- `image_to_icns_web/` — WASM editor + static assets. Build with `wasm-pack`.
- `image_to_icns_worker/` — Cloudflare Worker (wasm32). Not in workspace build.
- `image_to_icns_mcp/` — JSON-RPC 2.0 MCP server over stdio.

## Conventions

- Error types: `thiserror` with stable `code()` method.
- WASM boundary: `wasm-bindgen` types only.
- Comments and strings: English only.
- New features: add tests, update CHANGELOG.md.
