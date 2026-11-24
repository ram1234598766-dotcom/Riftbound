# Riftbound Upgrade Roadmap

## Product goals

Riftbound should feel responsive, cohesive, and polished while keeping memory use and frame cost predictable. The native Rust renderer and browser multiplayer prototype are separate targets; features must be implemented and tested in the correct runtime rather than assumed to transfer between them.

## Engineering guardrails

- Reuse CPU staging vectors and GPU buffers; grow capacities only when required.
- Keep draw calls and per-frame allocations bounded.
- Provide scalable quality settings and safe defaults for low-memory devices.
- Prefer deterministic gameplay and reproducible tests.
- Measure before and after optimisations; do not claim device support without profiling on that device.
- Keep native rendering and browser multiplayer feature parity explicit in documentation.

## Milestones

### 1. Performance foundation — in progress
- [x] Persistent native mesh instance buffers.
- [x] Reuse changed-entity and affected-mesh tracking buffers.
- [x] Skip uploads for unaffected mesh batches after transform changes.
- [x] Camera-frustum culling for native scene instances, recalculated only after camera, scene, viewport, or transform changes.
- [x] Add renderer counters for submitted, visible, and culled instances.
- [ ] Profile frame time and memory on representative low-, mid-, and high-tier hardware.
- [ ] Add a low-memory browser quality preset with explicit limits for DPR, effects, and particle counts.

### 2. Renderer and world quality
- [ ] Add camera-distance LOD tiers with hysteresis to prevent visual popping.
- [ ] Add frustum-aware terrain chunking so large worlds do not require one monolithic terrain mesh.
- [ ] Add a compact lighting model and tone-controlled environment palette.
- [ ] Add bounded ambient effects with quality-scalable density.
- [ ] Add asset/mesh validation and graceful fallback for missing resources.

### 3. Gameplay depth
- [ ] Improve movement responsiveness, camera follow smoothing, and collision edge cases.
- [ ] Add readable enemy state machines and telegraphed attack windows.
- [ ] Add interaction prompts, objectives, checkpoints, and mission completion flow.
- [ ] Add progression and rewards with deterministic save data.
- [ ] Add gameplay tests for movement, combat state transitions, and objective progression.

### 4. Content and world building
- [ ] Build distinct environment zones using shared terrain and reusable props.
- [ ] Add enemy and encounter variety through data-driven configuration.
- [ ] Add exploration rewards and optional challenges.
- [ ] Keep content modular so unused zones and assets are not loaded into memory.

### 5. Presentation and accessibility
- [ ] Improve HUD hierarchy, menus, pause flow, and settings.
- [ ] Add original sound effects, music controls, and volume settings.
- [ ] Improve animation transitions and hit/interaction feedback.
- [ ] Add remappable controls, text scaling, colour-safe indicators, and reduced-effects mode.

### 6. Multiplayer and persistence
- [ ] Harden host-authoritative state validation and reconnect handling.
- [ ] Add versioned save/load and recovery from invalid save data.
- [ ] Add multiplayer tests for disconnects, late joins, duplicate messages, and stale entity IDs.
- [ ] Document which gameplay systems are shared and which remain browser-only.

### 7. Release readiness
- [ ] Add low/medium/high graphics presets and an automatic conservative preset.
- [ ] Run repeatable performance and memory benchmarks on target devices.
- [ ] Add smoke tests for startup, resize, focus loss, reconnect, and save recovery.
- [ ] Package builds with clear controls, requirements, and known limitations.

## Current implementation note

The native renderer now retains instance buffers and only rebuilds visibility lists when scene transforms or camera/projection state changes. Frustum testing uses conservative world-space bounding spheres for the built-in meshes. This is an initial culling foundation, not a substitute for profiling or future mesh-specific bounds. Terrain chunking and LOD remain separate future milestones.
