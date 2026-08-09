# image_to_icns

[English](README.md) | [简体中文](README.zh-CN.md)

Convert PNG, JPEG, or SVG artwork into a verified macOS `.icns` file in the
browser. Image decoding, cropping, resizing, encoding, and verification happen
locally through Rust and WebAssembly.

[![CI](https://github.com/Tinkora/image_to_icns/actions/workflows/test.yml/badge.svg)](https://github.com/Tinkora/image_to_icns/actions/workflows/test.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-15803d.svg)](LICENSE)
[![Rust 1.95+](https://img.shields.io/badge/rust-1.95%2B-000000.svg)](rust-toolchain.toml)

## Use the editor

Open [the hosted editor](https://tinkora.github.io/image_to_icns/), select an
image, adjust the square crop, generate the icon, and download `icon.icns`.
The source image never leaves the browser tab.

## Why this exists

A macOS icon is not a renamed PNG. An `.icns` file contains multiple logical
sizes and Retina representations. `image_to_icns` generates and verifies all
10 modern representations from one 1024 px master image:

- 16, 32, 64, 128, 256, 512, and 1024 physical pixels
- independent Lanczos3 scaling from the master image
- read-back verification before download
- transparent output and a visual square crop editor

## Current scope

The public editor is the primary product and works without an account or
backend. It supports PNG, JPEG, and SVG input.

The repository also includes an optional, self-hosted MCP session server. It
can create a short-lived editor link, report whether the user is editing or has
completed the conversion, and cancel a session. It stores metadata in
Cloudflare D1, never source images or generated files. The MCP flow does not
transfer the downloaded `.icns` back to an agent.

PDF input, server-side image processing, and artifact sharing are not part of
the browser release.

## Build locally

Requirements:

- Rust 1.95
- `wasm32-unknown-unknown` target
- `wasm-pack` 0.15
- Python 3 or another static file server

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --version 0.15.0 --locked
./scripts/build_web.sh
python3 -m http.server 4173 --directory dist
```

Then open `http://127.0.0.1:4173`.

## Project layout

| Path | Responsibility |
| --- | --- |
| `crates/image_to_icns_core` | Decode, crop, render, encode, and verify ICNS data |
| `crates/image_to_icns_web` | WASM bindings and the browser editor |
| `crates/image_to_icns_mcp` | MCP JSON-RPC server over stdio |
| `crates/image_to_icns_worker` | Optional Cloudflare Worker and D1 session store |
| `scripts/build_web.sh` | Reproducible production web build |

The static editor has no runtime dependency on the MCP server or Worker.

## Quality checks

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

`cargo test --workspace --locked` covers the native workspace, but the
`wasm_api.rs` suite is compiled only for `wasm32` and therefore runs zero tests
on the host target. The separate Core `wasm-pack` command runs nine tests in
Node: PNG, JPEG, and SVG decoding; rejection of unknown and corrupted input;
default rendering; non-square encode rejection; encode/verify round-trip; and
truncated-data verification failure. The Worker `wasm-pack` command separately
checks its WebAssembly `Response` boundary.

## Optional MCP sessions

The MCP binary requires a deployed Session Worker. It defaults to a local
Worker at `http://localhost:8787`. The editor deployment must pin that same
Worker origin in its trusted `config.js`; a Session link cannot select an
arbitrary request target.

```json
{
  "mcpServers": {
    "image_to_icns": {
      "command": "image_to_icns_mcp",
      "args": ["--worker-url", "https://your-worker.example.com"]
    }
  }
}
```

See [Self-hosting the Session Worker](docs/SELF_HOSTING.md) for the D1 schema,
local development, and deployment steps.

## Privacy and security

- Source images and generated `.icns` bytes remain in browser memory.
- Session records contain only an opaque ID, hashed secret, timestamps, state,
  source-format hint, output byte length, and representation count.
  A failed conversion may also record a non-sensitive error code.
- Session secrets use 64 random bytes and are placed in the URL fragment, which
  browsers do not send in HTTP requests or referrer headers.
- Session metadata expires after 30 minutes; terminal records are deleted after
  24 hours by the scheduled cleanup task.

Report vulnerabilities through [GitHub's private security advisory
form](https://github.com/Tinkora/image_to_icns/security/advisories/new). See
[SECURITY.md](SECURITY.md) for scope and disclosure guidance.

## Documentation

- [Contribution status and development guide](CONTRIBUTING.md)
- [Self-hosting guide](docs/SELF_HOSTING.md)
- [ADR-0001: Browser-first architecture](docs/decisions/0001-web-first-session-architecture.md)
- [ADR-0002: Local-only first release](docs/decisions/0002-local-only-first-release.md)
- [Changelog](CHANGELOG.md)

## License

MIT License. See [LICENSE](LICENSE).
