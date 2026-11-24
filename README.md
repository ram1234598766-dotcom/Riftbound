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

## PC-hosted LAN play and browser multiplayer

Launching the native Riftbound runtime also starts a local HTTP/WebSocket host on TCP port 7878. The host binds to all network interfaces so devices on the same trusted Wi-Fi/LAN can join without installing an app.

1. Install Rust and launch the native runtime from the repository with `cargo run -p riftbound-runtime`.
2. Read the LAN URL printed in the runtime log (for example, `http://192.168.1.20:7878`).
3. Open that URL in a PC browser or on a phone/tablet connected to the same Wi-Fi.
4. If the phone cannot connect, allow the Riftbound executable through the PC firewall for private networks and check that the router does not isolate wireless clients.

The browser build includes touch controls and automatically connects to the host's WebSocket endpoint. Connected browser clients see one another's explorer markers update in the shared arena. The PC can also join as a browser client using `http://127.0.0.1:7878`.

**Multiplayer status:** this is the first LAN networking milestone: the PC hosts the web page and relays bounded player-position snapshots to connected browser clients. Movement now sends directional input to the PC host, which advances player positions using a capped server-side timestep and fixed movement speed; clients no longer submit their own coordinates. Shard claims are checked against host-side pickup locations and extraction state is shared. Enemy simulation, obstacle collision, player speed validation, and full progression authority are not yet host-authoritative. The browser now has a project-owned WebGL low-poly 3D presentation layer for the arena, explorer, shards, sentinels, and remote players, with a 2D fallback when WebGL is unavailable. Its underlying mission rules and collision simulation are still the separate Shardfall prototype; it is not yet rendering the native Rust scene or sharing the native runtime's full 3D simulation. The WebGL layer has not yet been visually or performance-tested on physical phones. A later milestone will move gameplay simulation to the host and bring the native and browser clients onto the same authoritative game state.

## Rust engine

The custom engine foundation lives in crates/riftbound-core. It currently contains deterministic fixed-step timing, logical input state, vector primitives, generational entity identifiers, 3D transforms, and a scene object store with safe spawn/despawn, transform access, and cycle-safe parent/child hierarchies.

The native application shell lives in crates/riftbound-runtime; GPU surface setup and frame presentation are isolated in crates/riftbound-renderer. The native runtime opens a window and builds a bounded 3D arena with a floor, perimeter walls, solid amber obstacles, and a controllable, articulated sci-fi explorer. The explorer is assembled from parented scene parts (torso, head, limbs, backpack, and visor); its arms and legs animate with a camera-relative locomotion gait. The arena now includes cyan corner pylons with amber caps, thin cyan floor inlays, and an amber-framed extraction landing pad, repeating perimeter fins, and an open extraction gateway. WASD or arrow keys move the player relative to the camera's facing direction, hold either Shift key to sprint, Space jumps, F collects nearby rift shards; cyan shards bob and rotate and the amber extraction beacon pulses; after recovering all five, return to the amber extraction beacon and press F to complete the native mission; an in-game HUD shows shard progress, the current objective, and contextual F interaction prompts; the player turns toward horizontal travel, and the third-person camera eases after the player, Q/E orbit the camera, and -/= zoom it. The core controller exposes idle, moving, jumping, and falling locomotion states for future animation systems, camera-relative movement keeps forward/strafe controls aligned with the orbit camera, and holding Shift temporarily raises the movement speed. The core collision world resolves movement against the arena bounds and obstacle footprints, supports sliding along walls, and uses substeps plus boundary refinement to prevent tunneling. The runtime also retains the original starter scene's indexed cube and low-poly pyramid meshes. Scene objects have generational IDs, a mesh asset ID, and editable local position, Euler rotation, and per-axis scale. Objects default to the cube mesh or can select a built-in cube, pyramid, faceted crystal, or subdivided terrain mesh explicitly. The arena floor now uses a 17-by-17 vertex-coloured terrain grid with deterministic surface variation; the current terrain remains flat and uses the existing horizontal collision plane. Each object also has a renderer-independent material ID with default, rift-cyan, and amber presets; material tint is passed per instance, so objects sharing a mesh can still use different presets in one draw batch. The shader applies ambient plus directional diffuse lighting using a face normal derived from world-position gradients, then blends distant geometry into a deep-blue atmospheric fog using camera-to-fragment distance. The renderer groups objects by mesh and submits one instanced indexed draw per non-empty mesh batch. The built-in asset catalog includes a faceted octahedral crystal mesh used by the arena's cyan landmark clusters. Parent transforms are composed into world matrices for rendering; invalid or cyclic parent links are rejected, and removing a parent safely detaches its children. Replacing the renderer scene uploads the composed transforms to the GPU. Mesh vertices and 16-bit triangle indices are uploaded to reusable GPU buffers; per-instance model transforms are supplied through an instance buffer. The renderer includes a perspective camera, depth testing, and a following camera target. This is an early native gameplay milestone, not yet a complete world renderer or full mission. Collision is currently a deterministic horizontal-plane system with static rectangular obstacles. Vertical movement now includes a configurable gravity/jump controller and ground-plane landing; terrain slopes, ramps, dynamic rigid bodies, and multi-surface ground detection are not implemented. The game is not yet running as the complete Shardfall mission on this Rust engine; the browser mission remains a separate playable prototype.

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
2. Expand the native shell with platform input, lifecycle handling, and a contextual mission HUD.
3. Expand the project-owned renderer: reusable scene mesh handles, materials, lighting, and shader management.
4. Port the player controller and Shardfall mission into the Rust runtime.
5. Expand traversal, vehicles, larger environments, audio, save data, and authoritative cross-device multiplayer.

## Quality principles

- No fake feature claims: unfinished systems are marked as unfinished.
- Small modules, explicit APIs, deterministic tests where practical.
- Third-party crates are infrastructure dependencies, not a substitute game engine.
- Keep assets licensed, documented, and reproducible.
