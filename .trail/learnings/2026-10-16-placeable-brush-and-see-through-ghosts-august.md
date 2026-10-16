---
title: Placeable brush and see-through ghosts in test pictures
author: August
date: 2026-10-16
branch: claude/elegant-leavitt-ab5fdf
cost: { minutes: 35, tokens: 407402 }
touches:
  - crates/mg-ui/src/paint_mode.rs
  - crates/mg-ui/tests/app.rs
  - docs/deferred.md
  - docs/manual/04-areas.md
  - crates/mg-ui/src/area_view.rs
  - crates/mg-ui/tests/screens.rs
  - docs/log.md
  - crates/mg-area/src/edit.rs
  - crates/mg-area/src/walk.rs
  - crates/mg-ui/src/icons.rs
---

## Decided
- The area view's placeable brush lives in crates/mg-ui/src/paint_mode.rs (Paint on the toolbar, key B): Scatter and Row, one `Paint Placeables` command per stroke. Its settings are `settings.paint`, in whole centimeters, percents and degrees, because `Settings` derives `Eq` and can't hold f32.
- Dabs land only on walkable walkmesh faces (`Ground::height_on` with surfacemat.2da `Walk`), at the level nearest the pointer's. The circle is drawn on the same ground, so it doesn't climb towers.
- A row's piece length comes from the model's box (`AreaView::bounds_of`, through `AreaScene::ghost`), plus a Gap that may be negative.

## Tried and ruled out
- Drawing the brush circle with `on_ground` (nearest tile level): it ran up walls and towers beside the pointer.

## Watch out
- See-through ghosts (GHOST_OPACITY 0.6) don't show in the egui_kittest wgpu harness pictures at all. On develop, `area_ghost` in screens.rs shows only the statue's cyan box. Set the opacity to 1.0 to see them in a test picture, or shoot after the stroke is laid. Hold ghosts with counts (`paint.shown`), not pictures.
