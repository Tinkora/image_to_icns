# CLAUDE.md

You are working on `image_to_icns`, a Rust/WASM privacy-first macOS icon generator.

Reference `AGENTS.md` for architecture and conventions. Key notes for Claude:

- Always run `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` before claiming completion.
- Worker crate not in workspace members (wasm32 target). Don't try to `cargo check` it.
- Core library must remain platform-agnostic. Use `#[cfg(target_os = "macos")]` for PDFKit.
- Error messages and comments must be in English.
- Follow Conventional Commits: `feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`.
- Include changelog entry for user-facing changes.
