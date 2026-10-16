---
title: "Module tree: sorting, waypoint tags, scrolling, and stacking objects"
author: August
date: 2026-10-16
branch: worktree-builder-remarks
cost: { minutes: 21, tokens: 301017 }
touches:
  - crates/mg-ui/src/area_view.rs
  - crates/mg-ui/src/keys.rs
  - crates/mg-ui/src/lib.rs
  - crates/mg-ui/src/tree.rs
  - crates/mg-ui/tests/app.rs
  - docs/deferred.md
  - docs/manual/02-main-window.md
  - docs/manual/04-areas.md
  - docs/manual/09-build-and-test.md
---

## Decided
- Module tree (crates/mg-ui/src/tree.rs): an area's objects are listed sorted by their row text; waypoints show their tag (name in the tooltip), as Aurora does. `KindContents.order` holds GIT indices in display order, so the keys, `goes` and `sought` logic keep working on GIT indices.
- Tree scrolling never re-centers: every reveal uses `scroll_to_rect(.., None)` (only as far as needed). A click on an object row sets `tree_object` itself, so the tree does not jump between the two clicks of a double click.
- Stacking is a command, not a placement mode: Drop onto Object (Shift+G) casts a ray down from 2 m above each selected object against placeables' and doors' models (`scene.ray_hit`), else the ground.
- A per-object "lock, not selectable" already exists (right-click › Lock, field `MG_Locked`).

## Tried and ruled out
- Stacking while placing via a modifier: Shift (place another), Ctrl (camera) and Alt (rectangle) are all taken during placement.

## Watch out
- The jump to the top of an area's tree came from `Action::OpenTab(Tab::Area)` setting `tree_reveal`, which scrolled the area's row (far above its objects) to the center. When the area is opened out and has a marked object, scroll to the object row instead.
- An egui CollapsingHeader opened this frame does not lay out its body until its animation starts, so keep `tree_object_pending` until the row has really been scrolled to.
- A UI test's double click right after a single click (within 0.3 s) becomes a triple click: wait ~30 steps first.
