# Pull Request

> Public contribution intake is not open until Tinkora's public-interaction
> gate is complete. This template documents the future review contract.

## Summary

<!-- Briefly describe what this PR does -->

## Type

- [ ] Bug fix
- [ ] New feature
- [ ] Performance improvement
- [ ] Documentation update
- [ ] Code refactor
- [ ] CI / build system
- [ ] Dependency update

## Testing

<!-- Describe how you tested your changes -->

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] Worker format, test, and wasm32 Clippy checks pass (when affected)
- [ ] `./scripts/build_web.sh` passes (when affected)
- [ ] Manual testing performed (describe below)

## Related context

<!-- Link relevant design notes or internal tracking when available. -->

## Screenshots (if applicable)

## Checklist

- [ ] I have read the [Contributing Guide](../CONTRIBUTING.md)
- [ ] My code follows the project's code style
- [ ] I have added tests for new functionality
- [ ] I have updated documentation as needed
- [ ] I tested frontend changes at 375, 768, 1024, and 1440 px (when affected)
- [ ] This PR is ready for review
