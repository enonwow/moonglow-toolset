---
title: See-through ghosts that seem missing from pictures
author: August
date: 2026-10-16
branch: claude/relaxed-hopper-283551
cost: { minutes: 51, tokens: 326883 }
touches:
  - crates/mg-ui/src/area_view.rs
  - crates/mg-ui/tests/app.rs
  - crates/mg-ui/tests/screens.rs
  - docs/deferred.md
  - docs/log.md
  - tools/aurora/headless.sh
---

## Decided
- A ghost "missing" from the egui_kittest wgpu pictures was drawn and blended correctly: at 0.6 it added ~0.55-0.64 of the opaque model's difference per pixel. The cause was the frame (a dark statue half inside a wall, before a banner) and low opacity. GHOST_OPACITY is 0.8 (the tile preview is 0.85).
- Hold a ghost to the picture, not only to counts: render with and without it and count changed pixels around it, skipping the cyan outline (blue minus red > 60). See a_blueprint_about_to_be_placed_shows_in_the_views_picture.

## Tried and ruled out
- Drawing the ghost unlit (Instance::unlit): less luminance contrast than lit, not more.
- Suspecting the Blend/Fringe split, the batcher or PREDICTABLE renderer options: all fine.

## Watch out
- Before calling a ghost invisible, compare against opacity 0 and 1 pixel by pixel. Check that the spot under the pointer isn't inside a wall (depth hides it even when opaque).
- Real GUI on tools/aurora/headless.sh: run with WGPU_BACKEND=gl, because Vulkan windows give stale xdrive screenshots. Approach a click in several small pointer moves, or egui misses it. Use scratch XDG dirs and reset them between runs.
