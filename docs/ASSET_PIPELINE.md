# Riftbound Asset Production Standards

This document defines the production contract for vehicle, character, environment, animation, and audio assets. The game targets a high-quality open-world driving RPG, with scalable presentation for lower-end PCs and mobile-class hardware.

## 1. Core principles

- Establish one consistent art direction before producing large asset batches.
- Prefer strong silhouettes, believable proportions, coherent materials, and purposeful detail over raw polygon count.
- Every asset must have a documented source, license/provenance, intended runtime, scale, bounds, and fallback.
- Keep source assets separate from runtime-ready exports. Do not commit generated caches or unlicensed third-party content.
- High, medium, and low quality tiers must preserve gameplay readability and the game's visual identity.

## 2. Vehicle model requirements

A vehicle asset should be authored as a reusable assembly, not one undifferentiated mesh:

- Main body and exterior trim.
- Four independently transformable wheel assemblies, with steering pivots on the front axle.
- Glass, lamps, mirrors, bumpers, and readable material separation.
- Optional interior assembly for close camera views; interior detail may be omitted at distant LODs.
- Named attachment points for camera, lights, audio emitters, damage, and interaction.
- Collision representation separated from render geometry.
- LOD0/LOD1/LOD2 meshes with stable silhouettes and matching pivots.
- Material slots kept bounded; use shared materials and texture atlases where practical.

Initial authoring targets (guidelines, not hard limits):
- LOD0: approximately 25k–60k triangles for hero vehicles.
- LOD1: approximately 8k–20k triangles.
- LOD2: approximately 2k–6k triangles.
- Collision: simple convex primitives/hulls; never use detailed render triangles as the default physics collider.
- Texture sets: prefer a small number of shared 1K/2K maps; reserve larger maps for close-up hero assets after profiling.

## 3. Player and NPC model requirements

- Use a consistent humanoid scale and shared skeleton across compatible characters.
- Keep deformation topology clean around shoulders, elbows, hips, knees, and hands.
- Separate reusable clothing, gear, and cosmetic variations where this does not create excessive draw calls.
- Provide facial and silhouette variation for important NPCs without requiring every ambient NPC to use hero-level assets.
- Include collision capsules and interaction sockets independently from the render mesh.
- LOD must preserve silhouette, animation readability, and colour identity.

Suggested tiers:
- Hero: full rig, facial/upper-body layers, high-detail materials.
- Standard NPC: shared rig and reduced material/mesh complexity.
- Distant crowd: simplified rig or baked animation where appropriate; update at a lower frequency.

## 4. Animation production contract

Character state coverage:
- Idle, start/stop, walk, jog, run, turns, jump, fall, land, interaction, enter vehicle, exit vehicle, and seated driving.
- Blend locomotion by speed and direction; avoid abrupt state snapping.
- Foot contact should be stable on slopes and uneven terrain.
- Animation events should expose footsteps, interaction timing, and other gameplay cues without coupling gameplay correctness to a visual animation.

Vehicle state coverage:
- Wheel spin and front-wheel steering.
- Suspension compression/rebound and body pitch/roll.
- Brake, reverse, indicator, and headlight states.
- Door/hood/trunk interactions where supported.
- Damage response should be cosmetic first and must not destabilize physics.

Environment animation:
- Wind response, water motion, lightweight ambient props, and weather effects.
- Use distance-aware update rates and quality-scalable effect density.

## 5. Audio production contract

Required categories:
- Vehicle engine layers, starter/idle, throttle/load, gear changes, exhaust, tire roll by surface, braking, handbrake, collision, suspension, horn, and vehicle UI.
- Footsteps by surface, player movement, interaction, NPC dialogue cues, and world interaction.
- Biome ambience, wind, rain, water, wildlife/settlement ambience, and activity-specific stingers.
- Adaptive music states for exploration, discovery, racing, danger, and story moments.
- UI feedback with restrained, consistent sound design.

Implementation requirements:
- Drive engine layers from normalized RPM/load/speed/gear parameters, not from a single looping clip.
- Crossfade surface-specific tire layers and avoid hard switching at biome boundaries.
- Use spatial attenuation and a bounded voice budget; prioritise nearby gameplay cues.
- Stream long music and ambience; keep only short, frequently reused effects resident when budget allows.
- Provide independent master, music, effects, ambience, and dialogue volume controls, plus mute.
- Captions/subtitles must communicate important dialogue and non-obvious gameplay audio cues.
- All audio must be original, properly licensed, or otherwise cleared for distribution.

## 6. Runtime performance budgets

Budgets are provisional until measured on target devices. Record actual resident and peak memory, CPU time, GPU time, draw calls, and audio voices for each representative scene.

- Avoid unbounded per-frame allocations in animation, audio, and asset streaming.
- Share materials and meshes; batch where it does not harm culling.
- Use compressed textures, mipmaps, mesh LOD, frustum/occlusion culling, and streamed world cells.
- Throttle distant animation and ambient simulation.
- Cap concurrent audio voices and use priority-based voice stealing.
- Every high-cost effect needs a lower-cost or disabled fallback.
- Never claim mobile/low-end compatibility until real hardware tests and repeatable benchmark captures exist.

## 7. Validation checklist

For each imported asset:
- [ ] Source file and license/provenance recorded.
- [ ] Scale, origin, pivots, naming, and bounds validated.
- [ ] Collision mesh separated from render mesh.
- [ ] LODs and fallback material assigned.
- [ ] Texture dimensions/formats and memory footprint recorded.
- [ ] Animation clips and event names validated.
- [ ] Audio sample rate, channel layout, loudness, loop points, and streaming policy checked.
- [ ] Missing/corrupt assets fail gracefully with a clear diagnostic.
- [ ] Performance checked in a representative gameplay scene.

## 8. Integration sequence

1. Define runtime asset handles and import metadata.
2. Add a validated vehicle assembly with body, wheels, pivots, collision proxy, and camera socket.
3. Integrate the first vehicle with the existing vehicle physics core.
4. Add parameter-driven engine and tire audio plus volume controls.
5. Add vehicle animation and a rigged player enter/exit/drive transition.
6. Extend the same standards to NPCs, props, biomes, and world ambience.
7. Add streaming, LOD, profiling, and low-memory fallbacks before expanding asset quantity.
