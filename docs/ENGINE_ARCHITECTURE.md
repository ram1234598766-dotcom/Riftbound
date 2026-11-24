# Riftbound Engine Architecture

Riftbound is a project-owned engine. It does not embed Bevy, Godot, Unity, or another full game engine. Low-level crates are allowed where they provide platform or hardware access; engine policy, runtime systems, and game-facing APIs remain in this repository.

## Layer boundaries

| Crate | Responsibility | Dependency direction |
| --- | --- | --- |
| riftbound-core | Deterministic time, input abstractions, entity identity, transforms, math, hierarchical scene object storage, simulation contracts | No platform or GPU dependencies |
| riftbound-runtime | Application entry point, window lifecycle, event loop and runtime orchestration | Composes platform and renderer-facing APIs |
| riftbound-renderer | GPU device/surface management, frame submission, render graph, pipelines, materials, lighting, shadows, post-processing | May depend on low-level graphics APIs |
| riftbound-assets | Asset discovery, import, validation, caching, streaming and provenance | Uses project-owned asset formats/contracts |
| riftbound-audio | Audio scene, buses, spatial emitters and mixer policy | Platform audio backend stays behind an adapter |
| riftbound-net | Replication contracts, snapshots, prediction/reconciliation boundaries and transport adapters | Core types only; no rendering dependency |
| riftbound-game | Riftbound content, missions, traversal, vehicles, world rules and UI | Composes the engine crates |

The runtime currently owns the native window/event loop. The renderer owns the wgpu instance, surface, device, queue, surface configuration and frame presentation. The renderer API does not expose GPU internals to gameplay. A dedicated platform crate remains a future extraction for input mapping and cross-platform lifecycle services.

Dependencies point inward toward stable contracts. The renderer must not own gameplay rules, and the core must remain testable without a GPU or window.

## Runtime rules

- Simulation uses a fixed timestep through FixedClock; rendering may run at a separate variable rate.
- Entity handles are generational. A despawned handle must never silently refer to a later entity that reuses its slot.
- Input is captured at the platform boundary and converted to logical actions before gameplay consumes it.
- World transforms are explicit data, not hidden renderer state. Scene objects use generational IDs, so stale references cannot address newly spawned objects. Parent links are cycle-checked, and world matrices are composed from local transforms in ancestor order. Despawning a parent detaches its direct children.
- GPU work is submitted by the renderer; gameplay systems do not issue graphics API calls.
- Asset loading is asynchronous and budgeted. Large world regions must be streamable instead of loaded as one monolithic scene.
- Multiplayer authority is explicit. Clients may predict presentation, but shared world facts must be validated by the agreed authority.
- The event loop waits when idle and requests redraws in response to window lifecycle events. A paced simulation/render scheduler will replace this idle policy when continuous world simulation is introduced.

## Quality gates

1. Pure core behavior has deterministic unit tests and no unsafe code.
2. Every new system includes boundary and failure-state tests.
3. CI checks formatting, Clippy and workspace tests without modifying the checkout.
4. Renderer work is validated with compile checks first, then deterministic offscreen captures, then real-device visual review.
5. Performance changes require a reproducible scenario and measurements; no speculative optimization claims.
6. Mobile and desktop are separate tested targets. A successful desktop build is not evidence of mobile compatibility.

## Delivery sequence

1. **Core foundations:** fixed clock, logical input, vector math, generational entity arena and 3D transforms.
2. **Native shell:** window lifecycle, resize/suspend handling and logical input mapping.
3. **First GPU scene:** standalone WGSL shader, indexed cube mesh with reusable vertex/index buffers, per-instance model transforms, instanced drawing, perspective camera uniform, arrow-key camera panning, depth buffer, resize-aware render targets, and a core-owned scene object store with generational IDs, editable local transforms, cycle-safe parent links, and composed world matrices. The renderer currently uploads those world matrices as instances of the built-in cube mesh. Next: scene mesh handles, material bindings, lighting, and shader hot reload.
4. **Scene runtime:** transform hierarchy, visibility, mesh/material handles and deterministic simulation scheduling.
5. **World rendering:** physically based materials, image-based lighting, cascaded shadows, occlusion and post-processing.
6. **Content pipeline:** glTF import, texture compression, validation, provenance and asynchronous streaming.
7. **Game systems:** character controller, traversal, vehicle handling, collision, save/load and mission scripting.
8. **Online co-op:** transport adapter, room lifecycle, authoritative pickups/objectives, reconnect behavior and abuse-resistant validation.

The browser mission remains a playable prototype while the native runtime is built. It is not represented as running on the Rust engine until the migration is complete.
