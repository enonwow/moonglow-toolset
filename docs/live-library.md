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

## Optional module preparation

The default feature set is empty. `build` enables `mg_live::build` and its
optional `mg-module`, `mg-resman`, `mg-tlk` and `zip` dependencies. The CLI and
UI explicitly enable it. Fixture preparation tests (`live_build`,
`measure_fixture`, `private_fixture`) require the feature; ordinary runtime
integration tests remain available without it. The existing `tools/live/check.ps1`
Live test routes explicitly enable `build` so preparation filters still run.
Run the build unit tests with
`cargo +1.98.0 test -p mg-live --features build -j 2`.

`python tools/live/check-library.py --check-features` checks all targets both
without features and with `build`, and checks both normal dependency trees.
The feature-free tree additionally rejects the four preparation dependencies.
This must run separately from workspace checks, whose feature unification
would otherwise hide a missing dependency in the runtime-only variant.

## Restricted external scopes

`Scope::restricted(name, groups, delivery)` always stores an explicit group list.
An empty list, all groups, or the names `default` and `local` never confer local
unrestricted privileges. `name()`, `has()`, `allows()` and `unrestricted()` are
public queries. Fields remain crate-private and the local `default()` constructor
is crate-private (no public `Default` implementation). A compile-fail doctest
checks that external callers cannot construct the local default.

The store's `enqueue_scoped`, `status_scoped`, `events_scoped` and operation
functions `call_scoped`, `command_scoped` are public without changing their
behavior. The `scope` integration tests exercise this API from outside the
crate and check every subset of groups, both delivery modes and reserved-looking
names. Run `cargo +1.98.0 test -p mg-live --test scope -j 2` and
`cargo +1.98.0 test -p mg-live --doc -j 2`.

## Event transport

`events::Event` supports both Serde serialization and deserialization with its
existing field names and types. `event_roundtrip` checks exact round trips with
optional object identities, a global event, Unicode, maximum sequence values
and nested arbitrary payloads. Run
`cargo +1.98.0 test -p mg-live --test event_roundtrip -j 2`.

## Observing synchronization decisions

`LiveStore::sync_tapped(body, now_ms)` returns `Result<Synced, String>`.
`Synced` contains the original `reply`, `outcome: SyncOutcome` and `boot_changed`.
`sync` delegates to it and returns only `reply`; existing sync tests stay unchanged.
The outcome is assigned in the existing gap and state-application branches under
the same lock, not inferred later from the resulting sequence number:

| Outcome | Existing decision |
|---|---|
| `Full` | A frame without `base_seq` applies state, including an equal-sequence full frame and the initial sequence zero. |
| `Delta` | A fresh delta extends the exact baseline and applies state. |
| `Gap` | A fresh delta cannot extend the baseline (including pending resync or an unknown partial area). |
| `Stale` | State is not applied: an older frame or an equal-sequence delta. |

`boot_changed` compares the request boot to the store boot before resetting it,
including the first report. A new-boot delta can therefore return both `Gap` and
`boot_changed = true`. Errors retain the existing error path and have no `Synced`
value. Events and command receipts retain their existing processing even when
state is not applied. Forwarding only accepted state and stripping side channels
is the responsibility of the consuming transport, outside this library change.

Run `cargo +1.98.0 test -p mg-live --test sync_tapped -j 2`. The tests check every
outcome, equal/older sequences, initial zero, restart, resync recovery, unknown
partial areas, reply equality and unchanged validation errors. Run the existing
store regressions with `cargo +1.98.0 test -p mg-live --lib -j 2`.

## Versioned finished state

`AreaLive`, `LiveObject`, `CommandResult` and `Event` serialize and deserialize
without changing the existing game keys. `MIRROR_VERSION` starts at 1.
The versioned fixture in `crates/mg-live/tests/fixtures/mirror-v1.json` captures
the declarations, including Serde attributes and optional fields that a default
value would omit. Any schema change requires a version bump and a new fixture;
retain old fixtures as compatibility evidence. Comments and formatting are ignored.
Run `cargo +1.98.0 test -p mg-live --test mirror_schema -j 2`.
Pending progress, final receipts, unknown outcomes and explicit null values round trip.

## Passive mirror

`LiveStore::new_mirror()` accepts only finished state through `mirror_area`,
`mirror_area_gone`, `mirror_meta`, `mirror_events`, `mirror_result` and
`mirror_reject`. Game reports and event ingestion are refused. Mirror events
retain gateway numbers, advance through filtered rows, deduplicate replay and
notify once per batch. Reset preserves the waiter signal, kick and policy.
No derived events, drafts, ID journal or sent-command timeout runs in this mode.

Set the welcome prefix with `set_id_prefix`. Generated IDs append eight random
hex digits shared by the process and a process-wide counter. Queue envelopes
carry boot, TTL and a local sent flag; only unsent commands expire. A reset
ends queued/pending commands as unknown and never requeues them. Every applied
mirror change increments `revision()`, including metadata, results and events.
`mirror_commands`, `mirror_mark_sent` and `mirror_set_ttl` support the transport.
Run `cargo +1.98.0 test -p mg-live --test passive_mirror -j 2` and the library
unit test `reset_keeps_waiters_signal_and_cannot_open_a_journal`.

## In-process gateway core

`mg_live::gateway::Gateway` owns session views of an authoritative `Arc<LiveStore>`.
The host supplies authenticated `SessionConfig` values; no network or identity
provider is included. `connect` validates resume epoch, boot, area revisions,
event retention and result retention. `pushes` emits boot/meta/changed areas,
results and filtered events in protocol order. Area updates coalesce to the
latest report, at most once per 250 ms per area/session. Focus targets and PCs
also make areas visible. The mirror applies store topics through `Push::apply`.

Per-area revisions advance only when reports apply. `received_ms` is excluded
from the area push's nested snapshot; `age_ms` reconstructs local receipt time.
`full_ms` stays an authoritative baseline identity, not a local receipt clock.
Events preserve `n`; filtered rows still advance `through`. Chat requires
`read_chat`; tell channels 4 and 20 additionally require `read_tells`. Command
and skill events are limited to the owning session.

Each session retains up to 256 result entries for ten minutes from acceptance.
A full outbox refuses new commands with `rate_limited`; it never evicts an
unacknowledged result to admit work. Versioned ACKs cannot acknowledge a newer
result. Final results are immutable; an unknown result can be resolved by a
late authoritative reply. Expired pending retention emits unknown once and
invalidates resume. The ordinary 64-result UI ring does not constrain outboxes.

Commands use `operations::command_scoped`, including policy, ownership and
player checks. Prefix/random/counter syntax, boot and TTL are checked first;
boot is checked again under the enqueue lock. TTL is 1..30000 ms, starts at
gateway acceptance and does not renew on retries. `watch_areas` and `resync`
are refused. Watch sets are absolute, at most the configured session limit;
the sorted union selects the first 64 and reports the remainder as waiting.
`spawn_scripted` grants scripted spawning, not standalone `run_script`.

The host can publish already authorized auxiliary drafts, presence, book
invalidations, session updates and notices with `publish_session`; these drain
after core state pushes. Identity ownership of AIs and book persistence belong
to the host, so the core does not infer those permissions.
Run `cargo +1.98.0 test -p mg-live --test gateway_core -j 2`.
