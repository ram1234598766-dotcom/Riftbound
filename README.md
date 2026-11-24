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

The custom engine foundation lives in crates/riftbound-core. It currently contains deterministic fixed-step timing, logical input state, vector primitives, generational entity identifiers, 3D transforms, and a scene object store with safe spawn/despawn, transform access, and cycle-safe parent/child hierarchies.

The native application shell lives in crates/riftbound-runtime; GPU surface setup and frame presentation are isolated in crates/riftbound-renderer. The native runtime opens a window and renders three scene-managed indexed mesh instances (two cubes and one low-poly pyramid). Scene objects have generational IDs, a mesh asset ID, and editable local position, Euler rotation, and per-axis scale. Objects default to the cube mesh or can select a built-in mesh explicitly. The renderer groups objects by mesh and submits one instanced indexed draw per non-empty mesh batch. Parent transforms are composed into world matrices for rendering; invalid or cyclic parent links are rejected, and removing a parent safely detaches its children. Replacing the renderer scene uploads the composed transforms to the GPU. Mesh vertices and 16-bit triangle indices are uploaded to reusable GPU buffers; per-instance model transforms are supplied through an instance buffer. The renderer includes a perspective camera, depth testing, and arrow-key camera panning across the horizontal world plane. This is an early scene-rendering milestone, not yet a complete world renderer or playable 3D game. In the native window, use the arrow keys to pan the camera. The game is not yet running on this Rust engine; the browser mission is a separate playable prototype.

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
3. Expand the project-owned renderer: reusable scene mesh handles, materials, lighting, and shader management.
4. Port the player controller and Shardfall mission into the Rust runtime.
5. Expand traversal, vehicles, larger environments, audio, save data, and multiplayer after the single-player foundation is stable.

## Quality principles

- No fake feature claims: unfinished systems are marked as unfinished.
- Small modules, explicit APIs, deterministic tests where practical.
- Third-party crates are infrastructure dependencies, not a substitute game engine.
- Keep assets licensed, documented, and reproducible.
