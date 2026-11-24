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

The custom engine foundation lives in crates/riftbound-core. It currently contains deterministic fixed-step timing, logical input state, vector primitives, generational entity identifiers, and 3D transforms.

The native application shell lives in crates/riftbound-runtime; GPU surface setup and frame presentation are isolated in crates/riftbound-renderer. The native runtime opens a window and renders a 3D cube from CPU-authored mesh vertices uploaded to a reusable GPU vertex buffer, with a perspective camera and depth testing. The renderer now has a standard position-and-color vertex layout and a reusable non-indexed Mesh abstraction. This is an early geometry milestone, not yet a complete world renderer or playable 3D game. The game is not yet running on this Rust engine; the browser mission is a separate playable prototype.

See [the engine architecture](./docs/ENGINE_ARCHITECTURE.md) for module boundaries and the delivery sequence.

## Development checks

Install a current stable Rust toolchain, then run:

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

GitHub Actions runs these checks on pushes and pull requests.

## Roadmap

1. Complete core simulation contracts and integration tests.
2. Expand the native shell with platform input and lifecycle handling.
3. Build the project-owned renderer: depth buffer, indexed meshes, camera, materials, and shader management.
4. Port the player controller and Shardfall mission into the Rust runtime.
5. Expand traversal, vehicles, larger environments, audio, save data, and multiplayer after the single-player foundation is stable.

## Quality principles

- No fake feature claims: unfinished systems are marked as unfinished.
- Small modules, explicit APIs, deterministic tests where practical.
- Third-party crates are infrastructure dependencies, not a substitute game engine.
- Keep assets licensed, documented, and reproducible.
