# Riftbound

A custom Rust engine project with a playable browser mission while the engine is developed.

## Play the current build

Open [index.html](./index.html) from the repository in a modern browser, or serve the repository root with any static file server. The game has no external runtime dependencies.

**Mission: The Shardfall**
- Recover all five rift shards.
- Avoid or phase-dash through sentinel enemies.
- Return to the extraction beacon after collecting every shard.
- Keyboard: WASD / arrow keys to move, Space to phase-dash.
- Touch: on-screen directional pad and Phase button.
- Press R after a result screen to restart.

The browser mission is an early playable vertical slice, not the final open-world game.

## Rust engine

The custom engine foundation lives in `crates/riftbound-core`. It currently contains deterministic fixed-step timing, logical input state, and vector primitives. The game is not yet running on this Rust engine; the browser mission is a separate playable prototype.

## Development checks

Install a current stable Rust toolchain, then run:

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

GitHub Actions runs these checks on pushes and pull requests.

## Roadmap

1. Verify the Rust core in CI and add integration tests.
2. Add window creation and GPU presentation through low-level platform crates.
3. Build a project-owned renderer, scene graph, camera, and asset pipeline.
4. Port the player controller and the Shardfall mission into the Rust runtime.
5. Expand into traversal, vehicles, larger environments, audio, save data, and multiplayer only after the single-player foundation is stable.

## Quality principles

- No fake feature claims: unfinished systems are marked as unfinished.
- Small modules, explicit APIs, deterministic tests where practical.
- Third-party crates are infrastructure dependencies, not a substitute game engine.
- Keep assets licensed, documented, and reproducible.
