# Riftbound Upgrade Roadmap

## Product vision — locked direction

**Riftbound is an open-world driving RPG.** The arena, shards, and sentinel encounter are technology/gameplay prototypes, not the final product structure. Future work must move the project toward a connected explorable world where driving is a primary activity and RPG progression gives exploration a purpose.

### Pillars
1. **Open world:** connected regions, roads, trails, settlements, garages, landmarks, secrets, and discoverable activities. Stream world cells around the player instead of loading the entire world at once.
2. **Driving first:** responsive vehicle handling, camera follow, suspension/ground contact, collision, braking, reverse, handbrake, and distinct vehicle classes. Walking remains useful for exploration and interactions.
3. **Terrain matters:** asphalt, gravel, mud, sand, snow, and rock must change grip, acceleration, braking, rolling resistance, and traversal difficulty. Terrain must be shared by rendering, physics, and world queries.
4. **RPG progression:** quests, NPCs, dialogue, reputation/factions, rewards, inventory, skills/perks, and choices. Missions should naturally send players across the world.
5. **Vehicles and economy:** unlockable vehicle classes, garages, repairs, upgrades, tuning, and cosmetic customization.
6. **World activities:** races, deliveries, exploration challenges, recovery jobs, optional combat encounters, and hidden rewards. Combat supports the adventure; it does not replace driving/exploration.
7. **Accessible performance:** scalable world detail, LOD, culling, chunk streaming, pooled entities, bounded memory, and touch/gamepad/keyboard controls.

### Non-negotiable project rules
- Do not expand the small arena into the main game. Keep it as a combat/AI test location or repurpose its systems as isolated prototypes.
- Prioritize a playable driving loop and a traversable multi-surface region before adding more enemy types.
- Keep the browser multiplayer prototype and native Rust runtime distinctions explicit; do not claim feature parity.
- Build the world in streamed chunks with explicit memory budgets. Never attempt to keep a huge open world fully resident.
- Treat the current vehicle model as a physics foundation, not finished driving gameplay. Integrate collision, terrain sampling, suspension, vehicle visuals, camera, input and HUD before calling the car playable.
- Do not claim low-end compatibility or performance targets until representative hardware has been measured.

### Open-world implementation sequence
1. **Vehicle foundation:** deterministic vehicle state, throttle/brake/steering, reverse, speed limits, surface response and unit tests.
2. **Playable driving slice:** controllable car entity, third-person vehicle camera, terrain contact, collision, vehicle HUD and keyboard/touch/gamepad input.
3. **Terrain system:** data-driven surface regions and height sampling used consistently by renderer and physics; first road, gravel, mud and sand route.
4. **First explorable region:** a compact connected landscape with road network, off-road route, landmark, garage and a discoverable activity.
5. **RPG loop:** quest definitions, NPC interaction, rewards, inventory/upgrades, save/load and progression UI.
6. **World scale:** chunk streaming, asynchronous asset preparation, LOD/culling, region transitions and memory budgets.
7. **Content expansion:** more biomes, vehicles, races, deliveries, side quests and optional encounters.
8. **Multiplayer/release:** only after the single-player driving/RPG loop is cohesive; then authoritative world state, persistence, profiling and packaging.

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

### 3. Gameplay depth and vehicle-first RPG
- [x] Add a deterministic, allocation-free vehicle handling model with throttle, braking, steering, reverse, speed caps, bounded timestep, and terrain-specific grip/rolling resistance.
- [ ] Integrate a drivable car entity into the native runtime with third-person vehicle camera and vehicle HUD.
- [ ] Add ground contact, suspension approximation, vehicle collision, and safe recovery.
- [ ] Connect terrain surface regions to vehicle physics and renderer queries.
- [ ] Build a first road-to-off-road driving route with asphalt, gravel, mud, sand, snow, and rock handling.
- [ ] Add RPG quest, NPC interaction, reward, inventory, and vehicle upgrade foundations.
- [ ] Add save/load for player, vehicle, quest, and world progression.
- [ ] Keep the arena enemy/mission prototype as optional encounter content rather than the main game loop.
- [ ] Improve movement responsiveness, camera follow smoothing, and collision edge cases.
- [x] Add an allocation-free deterministic enemy brain with patrol, chase, telegraph, recovery, and return states; validate inputs and expose transition events.
- [x] Spawn a native arena sentinel and connect its deterministic brain to the scene transform update loop.
- [x] Add a pulsing telegraph indicator and short-lived pulse visual; allow the player to disable the sentinel during its recovery window.
- [x] Advance the secondary defeat objective when the sentinel is disabled and surface encounter prompts/status in the native HUD.
- [x] Add an explicit deterministic mission phase machine for shard recovery, extraction unlock, and one-time mission completion; drive native HUD prompts and mission status from its phase.
- [x] Add validated checkpoint activation, remember the latest active safe position, and recover the native player there after falling out of the arena.
- [x] Add a deterministic secondary-objective model for collect, interact, visit, and defeat goals, with saturation and completion tests.
- [ ] Wire secondary objectives into native runtime encounters and HUD feedback.
- [ ] Add progression and rewards with deterministic save data.
- [x] Add a deterministic player health model with validated damage, bounded protection timing, lethal-health handling, and unit tests.
- [x] Apply sentinel pulse damage using a bounded proximity check; add temporary hit protection and checkpoint recovery after lethal damage.
- [x] Display current/max player health and a low-health warning bar in the native HUD.
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

The native renderer now retains instance buffers and only rebuilds visibility lists when scene transforms or camera/projection state changes. LOD state is keyed by generational entity IDs for constant-time lookups, all five compact-mesh batches are created even when empty at startup, and visibility rebuilds distribute objects into reusable batch staging buffers in a single scene traversal. Frustum testing uses conservative world-space bounding spheres for the built-in meshes. The arena floor is split into four independently culled quadrants; each chunk samples the shared world-space terrain function and deterministic global colour pattern so adjacent edges match. Cube LOD currently has one compact tier. Terrain chunks now have detailed 9×9 and simplified 5×5 grids; all four chunks switch together with hysteresis so adjacent edges use matching samples. The native world shader now adds low-cost cool fill, warm key accents, a view-dependent cyan rim, and deep-blue distance haze without adding textures or extra render passes. The native Shardfall mission now uses a deterministic core state machine so recovery, extraction unlock, and completion are explicit, tested phases rather than a runtime boolean. The native runtime also has a validated checkpoint zone; interacting activates its safe respawn position, and falling below the arena restores the player and resets movement velocity. Checkpoint progress is currently session-only and is not yet written to disk. These are initial scalability foundations, not substitutes for profiling or future mesh-specific bounds.


The core objective model now supports collect, interact, visit, and defeat counters with bounded progress, completion transitions, and zero-target handling. Runtime encounter wiring remains a separate gameplay task; the model is not yet presented as active in the arena.


The core contains a lightweight enemy state machine with bounded pursuit, a telegraphed close-range pulse, recovery, and return-to-home behaviour. The native arena spawns one amber sentinel and updates its brain alongside the scene transform. The sentinel follows a bounded, two-waypoint patrol near its home position when the player is outside awareness range, then switches to pursuit when the player is detected. A low-cost pulsing ground indicator marks the telegraph and briefly expands on pulse release; the player can disable the sentinel during its recovery window, advancing the secondary defeat objective. The encounter currently exists in the native runtime, not the separate browser multiplayer prototype.


The native sentinel pulse now checks player proximity when released and applies 25 damage on a hit. A one-second protection window prevents repeated hits while the player remains close. Player health is managed by a small core model with tests for damage, protection expiry, lethal damage, restoration, and invalid inputs. The native HUD displays current/max health and changes the bar to a warning colour at low health. Lethal damage restores the player at the active checkpoint (or mission start) and refills health. This combat milestone is native-runtime only; it does not yet affect the separate browser multiplayer prototype.


The enemy brain now moves between two deterministic waypoints around its home position while idle. The route uses the existing fixed-size state and vector math, allocates no per-frame memory, and is bounded by a configurable patrol radius. A core unit test verifies the sentinel begins moving along the route without leaving its patrol state. The native HUD health test fixture has also been updated for the new health fields.


Enemy encounter authoring now has three lightweight behaviour profiles: Sentinel (balanced), Skirmisher (faster pursuit and shorter attack wind-up), and Warden (slower pursuit, wider attack zone, longer recovery). Each can be combined with Easy, Standard, or Hard difficulty tuning. The native arena explicitly selects the Standard Sentinel profile, preserving the established encounter feel. Core tests cover archetype distinctions and difficulty ordering. Skirmisher and Warden are currently authoring presets for future encounters; they are not yet spawned as additional arena enemies.


The native arena now spawns the Skirmisher and Warden profiles alongside the original Sentinel. Each support enemy owns an independent, allocation-free EnemyBrain state and a compact ground telegraph marker. Their pulses apply a lower 15-point hit than the Sentinel's 25-point pulse and use the same player protection window. Support enemy state is stored in a pre-sized vector and updated in place; this is an initial three-enemy encounter, not yet a scalable crowd/AI scheduler. The support enemies are not yet individually disableable through the interaction objective.


## Current vehicle milestone

The platform-independent core now includes `VehicleState`, `VehicleInput`, `VehicleConfig`, and `VehicleSurface`. It models throttle, braking, steering, reverse, speed caps, bounded frame delta, and surface-specific grip/rolling resistance without per-frame allocation. Unit tests cover acceleration, braking, steering, terrain response, and invalid inputs. This is not yet a playable car: runtime entity integration, terrain contact, suspension, collision, camera, input mapping, and vehicle HUD remain outstanding.
