# Changelog

All notable changes to `image_to_icns` are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-08-09

### Added

- Browser-local PNG, JPEG, and SVG import through Rust and WebAssembly.
- Interactive square crop preview with pointer panning and zoom control.
- Generation and read-back verification of 10 modern ICNS representations.
- Direct browser download without source-image or artifact uploads.
- English interface with responsive and keyboard-accessible controls.
- Rust core library with decode, transform, render, encode, and verify tests.
- Optional Cloudflare D1 Session Worker with expiration and rate limiting.
- MCP stdio server with create, query, and cancel Session tools.
- English and Simplified Chinese README files.
- Pinned GitHub Actions for Rust checks, WASM builds, Pages, and releases.
- A non-publishing cross-platform release canary and native immutable Release
  publication through a fully assembled draft.

### Security

- Restricted optional Session callbacks to a deployment-configured Worker
  origin, rejected query-carried credentials, and validated fixed-format
  Session credentials before constructing a request URL.

[0.1.0]: https://github.com/Tinkora/image_to_icns/releases/tag/v0.1.0
