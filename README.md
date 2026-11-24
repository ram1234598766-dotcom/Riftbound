# Riftbound Engine

A custom Rust game-engine project for Riftbound. The engine is built from project-owned systems; low-level crates provide platform and GPU access rather than an existing game engine.

## Status

Early foundation. The current milestone is a tested engine core and a minimal window/render loop. This is not yet a playable open-world game.

## Planned milestones

1. Core types, fixed-step timing, input state, and deterministic tests.
2. Window and GPU initialization with a minimal renderer.
3. Scene representation, camera, and basic geometry.
4. Character controller, collision, and traversal test course.
5. Asset pipeline, audio, vehicles, and multiplayer after the single-player foundation is stable.

## Quality bar

- Keep modules small and APIs explicit.
- Prefer tests for deterministic systems.
- Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` in CI.
- Avoid placeholder features and unverified claims.

## Development

Install a current stable Rust toolchain. Then run:

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```
