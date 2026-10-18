---
type: Implementation
title: Live library for headless consumers
description: Dependency boundary and public API contracts for embedding the Live store.
tags: [live, library, gateway]
generated: { by: codex, at: 2026-10-18T12:00:00Z }
sources: [../crates/mg-live/src/lib.rs, ../crates/mg-core/src/object_kind.rs]
---

# Live library for headless consumers

## Object kinds and dependency boundary

`mg_core::ObjectKind` owns the nine area object kinds. `mg_area::ObjectKind`
re-exports the same type so editor callers keep their existing imports.
The core GIT list table deliberately duplicates the nine `mg_module::instances::git_list`
rows to keep the dependency direction acyclic. The area unit test
`object_kind_lists_match_module_instances` checks every row against that table.

`mg-live` imports the core type directly. Run `python tools/live/check-library.py`
from the workspace root to reject normal dependencies on `mg-area`, `mg-render`,
`mg-preview` or `wgpu`. This runs `cargo +1.98.0 tree -p mg-live -e normal`.
Run the mapping regression with
`cargo +1.98.0 test -p mg-area --lib object_kind_lists_match_module_instances -j 2`.
Use a nonexistent `NWN_ROOT` and `CARGO_INCREMENTAL=0` for these checks; no game
server is required. Passing tests establish library behavior, not game or owner acceptance.
