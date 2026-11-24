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
- [x] Use generational-entity hash lookups for native LOD state, avoiding repeated linear scans during visibility rebuilds.
- [x] Rebuild visible native instance batches in one scene traversal, reusing per-batch staging vectors instead of rescanning the scene once per mesh batch.
- [ ] Profile frame time and memory on representative low-, mid-, and high-tier hardware.
- [x] Add an automatic low-memory browser preset using memory/CPU/save-data hints, a 1.0 DPR cap, reduced decorative draw density, and lighter HUD surfaces.
- [ ] Profile browser frame time and memory on representative low-, mid-, and high-tier devices.

### 2. Renderer and world quality
- [x] Add an initial camera-distance cube LOD tier with hysteresis to prevent visual popping.
- [x] Add a simplified distance-based terrain LOD tier (5×5 instead of 9×9 samples), with hysteresis and synchronized chunk switching to keep shared edges aligned.
- [ ] Extend LOD tiers to environment props and additional mesh types.
- [x] Split the native arena floor into four frustum-culled terrain chunks with matching shared-edge height and colour samples.
- [x] Enhance the native compact lighting model with cool fill, warm key accents, view-dependent cyan rim light, and distance haze in the existing WGSL shader.
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

The native renderer now retains instance buffers and only rebuilds visibility lists when scene transforms or camera/projection state changes. LOD state is keyed by generational entity IDs for constant-time lookups, all five compact-mesh batches are created even when empty at startup, and visibility rebuilds distribute objects into reusable batch staging buffers in a single scene traversal. Frustum testing uses conservative world-space bounding spheres for the built-in meshes. The arena floor is split into four independently culled quadrants; each chunk samples the shared world-space terrain function and deterministic global colour pattern so adjacent edges match. Cube LOD currently has one compact tier. Terrain chunks now have detailed 9×9 and simplified 5×5 grids; all four chunks switch together with hysteresis so adjacent edges use matching samples. The native world shader now adds low-cost cool fill, warm key accents, a view-dependent cyan rim, and deep-blue distance haze without adding textures or extra render passes. These are initial scalability foundations, not substitutes for profiling or future mesh-specific bounds.
