---
type: Plan
title: Live optimization plan
description: The order in which Live's measured costs are cut, in the game (tick, events, JSON), on the wire and in Moonglow (build, posing, repaints, queries), with the gate each step must pass.
tags: [live, performance, plan]
generated: { by: claude, at: 2026-10-14 }
sources:
  - live-game-performance.md
  - live-gui-performance.md
---

# Live optimization plan

Made from the two measurement reports: [game and wire](live-game-performance.md)
and [Moonglow](live-gui-performance.md). Every step is measured again with the
same helpers before and after, keeps behaviour (tests, and pictures for view
changes), and lands as its own commit. A step whose gain does not show in the
numbers is reverted, not kept "for later".

## What the numbers say

* **Moonglow:** a moving crowd costs 84% of a core in the debug build the demo
  ships and ~51% in an optimized build; each redraw spends ~9 ms posing models
  and ~6 ms on particles. A hidden area view is already cheap (0.7%). The
  919 MiB is private commit; resident memory is ~125 MiB.
* **Game:** a full tick costs ~8.8 ms on the 218-object area; copying JSON
  while assembling the collections costs 2–15 ms (0–3 player looks); a burst of
  256 events costs 37 ms clearing and 44 ms enqueueing. Deltas save 80% of the
  bytes but cost more ticks today.
* **Moonglow's store** costs fractions of a millisecond per report: not a target.

## Steps

| # | Step | Where | Measured gain | Risk |
|---|---|---|---|---|
| 1 | The demo launcher builds and runs an optimized Moonglow | tools/live, Cargo profile | ~34 points of a core | low |
| 2 | Clear the event buffer's prefixes once per tick | mglive_evt.nss | 37 ms at 256 events | low |
| 3 | Chat/Log/Panel: limit before cloning events; cache the Panel rows per report | mg-live events, mg-ui | 2–4 points | low |
| 4 | Merge deltas once, fewer clones in the store | mg-live store.rs | 0.03–0.27 ms per merge | low |
| 5 | Assemble report collections without re-copying JSON | mglive_tick.nss | 2–15 ms per tick | medium |
| 6 | Enqueue events without re-copying the buffer | mglive_evt.nss | up to 44 ms at 256 events | medium |
| 7 | Skip posing emitters that do not change | mg-ui live_view, mg-area | 3–5 ms per redraw | medium |
| 8 | Coalesce repaint requests (one per report, bounded while moving) | mg-ui, mg-live wake | 1–5 points | medium |
| 9 | Measure again; then only if still needed: look cache by appearance, shared poses, delta threshold, spreading a tick over frames, native memory trace | both | unknown until 1–8 land | high |

Steps 1–4 are small and independent; 5–8 follow, one at a time per file.
Step 9 is decided from the numbers after 8, not before.

## Also

* The viewer test broken by the menu regrouping (`tests/app.rs`, Run Script
  now under Scripts) is fixed with step 3.
* Diagnostic build files and empty logs left by the measurements (~507 MiB)
  are removed once the steps that need them are done.
