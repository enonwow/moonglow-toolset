---
type: Implementation
title: Live library for headless consumers
description: Dependency boundary, passive mirrors, gateway transport and shared scoped HTTP routing.
tags: [live, library, gateway]
generated: { by: codex, at: 2026-10-18T12:00:00Z }
sources: [../crates/mg-live/src/lib.rs, ../crates/mg-live/src/access.rs, ../crates/mg-core/src/object_kind.rs, ../crates/mg-live/src/store_mirror.rs, ../crates/mg-live/src/store_gateway.rs, ../crates/mg-live/src/store_gateway_identity.rs, ../crates/mg-live/src/assistant_book.rs, ../crates/mg-live/src/assistant.rs, ../crates/mg-live/tests/gateway_requests.rs, ../crates/mg-live/src/gateway_frames.rs, ../crates/mg-live/tests/mirror_differential.rs, ../crates/mg-gateway/src/lib.rs, ../crates/mg-gateway/tests/transport.rs, ../apps/mg-gateway-dev/src/main.rs]
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
event retention and result retention. The host calls `tick` once per maintenance
interval, independently of session count. `pushes` emits boot/meta/changed areas,
results and filtered events in protocol order. Area updates coalesce to the
latest report, at most once per 250 ms per area/session. Focus targets and PCs
also make areas visible. The mirror applies store topics through `Push::apply`.

Per-area revisions advance only when reports apply. `received_ms` is excluded
from the area push's nested snapshot; `age_ms` reconstructs local receipt time.
`full_ms` stays an authoritative baseline identity, not a local receipt clock.
Event pushes contain at most 256 rows and 1 MiB of serialized JSON, continuing
on later pumps. Events preserve `n`; filtered rows still advance `through`. Chat requires
`read_chat`; tell channels 4 and 20 additionally require `read_tells`. Command
and skill events are shared by Read-capable sessions; result pushes remain owner-scoped.

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

## Gateway frames

The public [normative protocol](live-gateway-protocol.md) is implemented by
`gateway_frames`: typed hello/welcome, request/response, every push topic, ACK
and bye. Decoders check byte limits, UTF-8, JSON structure and response outcome;
unknown server types/topics are logged and skipped, unknown fields ignored.
The transport supplies close/HTTP behavior; this library returns errors.
`Connection` checks hello/version negotiation, request ID reuse and 64 in-flight
requests. The frame limit exception applies only to `req` with `op=book.put`.
Run `cargo +1.98.0 test -p mg-live --test gateway_frames -j 2` for all 18 golden
frame examples, every truncated prefix, malformed/deep/oversized input and
connection accounting.

## Differential replication proof

`mirror_differential` sends each authoritative push through the JSON frame
encoder/decoder before applying it to a passive mirror. It compares `area`,
`find`, `locate`, `events`, `result` and the shared GIT `reconcile` helper.
Proptest 1.11 uses ChaCha seeds 7, 19, 43 and 91 (each repeated to 32 bytes),
48 cases per seed, with 1..63 actions: full/delta reports, stale frames, gaps,
link drops/resume, game boot changes, gateway epoch changes and command results.
`fixtures/mirror-reports.json` records deterministic report shapes from the
existing store and `sync_tapped` regressions, including recovery and restart;
it is a public test corpus, not a native-server recording. No native server
or external data is required for these P1 checks.

Additional regressions exercise event-retention gaps, results arriving during
a link drop, arbitrary decoder bytes, explicit-null replies, scoped mirror
status and malformed focus requests. Shared meta revisions are identical for
sessions at the same snapshot; an acknowledged terminal command retry does
not recreate its outbox. Overlapping watch owners retain their own capacity.
Other regressions keep command result revisions independent of the UI
revision across boot reset, and reserve the shared 64-area capacity for PCs
and other watch owners before admitting gateway watches. Pending gateway
results can resolve a local reset's unknown outcome; local reset does not
invent a higher gateway result revision. Mirror status exposes the same
queued/result envelopes used by its read APIs, respecting local scopes.

Run `cargo +1.98.0 test -p mg-live --test mirror_differential -j 2`. Use Rust
1.98.0, `CARGO_INCREMENTAL=0`, a nonexistent `NWN_ROOT`, at most two jobs and
one workspace test runner at a time (shared UI fixtures are not concurrent).
Final gates are `cargo +1.98.0 test --workspace --exclude mg-corpus-tests --lib
--bins --tests -j 2`, `cargo +1.98.0 clippy --workspace --all-targets -j 2 --
-D warnings`, and `cargo +1.98.0 fmt --check`. Source and golden fixtures are
the retained reproduction artifacts; temporary gate output is local only.

The P1 in-process core tests use memory-only stores and do not prove persistence
across a new process or GUI acceptance. A host initializes durable storage
through the existing Live service or P2a's `CoreHandle::start_bridge` before
production use. The network tests below extend the P1 evidence to HTTP and WS.

P1 adds 27 passing unit/store regressions across schema, mirror, gateway,
frames and differential suites (including 192 fixed-seed report scenarios).
These checks establish the library contract, not owner or native-game acceptance.

## Gateway transport

`mg-gateway` embeds the P1 session core behind `CoreHandle`. All store work,
policy resolution and authentication execute on blocking workers. `/ws/v1`
requires a DM bearer principal and `moonglow-gw.1`; Origin requests are refused.
AI principals use `/agent/v1` and cannot upgrade a WebSocket, so they cannot
bypass the HTTP delivery/draft policy through the DM command channel.
Principal identifiers must be `dm:<id>` or `ai:<id>`; `default` and `local`
are reserved, including after the colon. Display names never become scopes.
`Authenticator`, `PolicyResolver` and `Extensions` separate host integration
from the public transport. Merge the host's `Extensions::router(core)` with
`mg_gateway::router(...)`; conflicting routes are a host configuration error.

The transport uses one resumable session per principal in v1; a second connection
replaces the first. Prefixes survive reconnects within one gateway epoch. The
session core limits area publication to four times per second. The bounded
output queue replaces older pending copies of the same area and boot. Its
default bounds are 256 frames and 32 MiB; overflow sends `bye overloaded`.
Individual output frames are limited to 16 MiB. Input is limited to 64 KiB,
with the protocol's 256 KiB exception for `book.put`. Oversize closes with 1009.
Ping is every 15 seconds, idle timeout 45 seconds, writes have a five-second
budget. Authentication is rechecked on requests and every ping interval;
revocation and role changes close the connection. Hosts must not log bearer
credentials. `/health` is an unauthenticated liveness endpoint.

HTTP cannot return 426 after WebSocket upgrade. Optional `?proto=1&mirror=1`
parameters support a pre-upgrade 426 with `min_client`; actual hello versions
are always checked, and a mismatch there sends `bye upgrade`. Missing the
required subprotocol also returns 426. This resolves the draft protocol's
otherwise impossible sequencing without changing its JSON frames.

Transport regression tests run with `cargo +1.98.0 test -p mg-gateway -j 2`,
`CARGO_INCREMENTAL=0` and a nonexistent `NWN_ROOT`. They bind ephemeral
loopback ports and require neither Redis nor game data.

## Shared agent routing

`mg_live::api::route(method, path, headers, body, scope, store)` is the synchronous
routing boundary for `/v1`. `route_with_context` adds the adapter's cancellation
flag, clock, presence snapshot and quota callback. Authentication and consent
remain adapter responsibilities; callers of the bare route must already have
validated the principal and provide rate limiting when exposing writes.
The existing TLS loopback API and `/agent/v1/*` both use this implementation.
The gateway requires an `ai:` principal and rejects Origin before body reads.
Gateway event responses additionally require `read_tells` for chat channels
4, 20 and `tell`, while retaining the unfiltered page cursor. This boundary
filter preserves the legacy loopback wire and its existing scope model.
`PolicyResolver` chooses delivery mode (AI defaults to drafts) and command
budget (60/minute by default). No client-supplied name can change attribution.

Catalogue ETags, If-Match/412, scope failures and command quota bodies remain
identical. A failed catalogue precondition does not spend quota. Long polls
retain the catalogue's 30-second cap; sixteen concurrent agent requests are
allowed, and body reads have a five-second budget. Quotas are shared by
principal across reconnects and include game queries. The private host is
responsible for its global AI consent and owner-role policy before returning
an authenticated principal. Host route integrations should use the context
variant to supply cancellation and metering rather than bypassing those hooks.

The existing eighteen loopback API tests exercise the extracted router.
`crates/mg-gateway/tests/agent.rs` compares discovery, reads, errors and ETags
against the pure route, verifies 401/403/429 and checks that a waiting event
request releases the async runtime and wakes on a subsequent report.

## Development gateway

Run `cargo +1.98.0 run -p mg-gateway-dev -j 2 -- --config private/gateway-dev.json`.
The config file is optional when `MOONGLOW_GATEWAY_TOKEN` is set. Use a fresh
random credential with 32..256 printable ASCII characters; the environment
value overrides the file's `token`. The default principal is `dm:dev`, with
all current DM groups except `scripts` and `chunks`. File `groups` replaces
that list explicitly. `tokens` optionally adds objects with `token` and
`principal: {id, name, groups}`, including `ai:` identities for agent tests.
Never commit the configuration or print tokens. Debug implementations redact
credentials and configuration parse errors do not echo input.

Supported config keys are `port` (8080), `token`, `groups`, `tokens` and
`data_dir` (`private/gateway-dev-data`). `--port` overrides the file port;
port zero is supported. There is no configurable bind address: it is always
127.0.0.1. `/health` can be used without a game or Redis connection. This host
uses plain local HTTP/WebSocket; a public deployment needs the host's TLS
reverse proxy and authentication integration.

Redis configuration uses `LiveConfig::from_env`, including
`MOONGLOW_LIVE_REDIS` and `MOONGLOW_LIVE_PREFIX`. `Bridge::start_persistent`
binds the endpoint and initializes the existing command-ID journal before
starting workers, then restores the durable event inbox. Keep `data_dir`
across restarts; it is required state, not a disposable cache. Core shutdown
signals WebSocket pumps and joins Redis workers off the async runtime.
The binary writes status to stdout and does not log requests or credentials.

Configuration regressions run with
`cargo +1.98.0 test -p mg-gateway-dev -j 2`. These tests do not connect to Redis.

## Gateway transport verification and limits

The socket tests bind `127.0.0.1:0`, use a tungstenite client, and feed game JSON
through `sync()` on a blocking worker. They cover upgrade authentication,
Origin and version refusal, restricted identities, command attribution, final
result ACK across resume, overload, oversize input, replacement and shutdown.
The public P1 report fixture is replayed through actual JSON WebSocket pushes,
including gaps and game boots; area, find, locate and event outputs match the
passive mirror after clock-local receive times are normalized. A new gateway
epoch refuses resume and clears the mirror's unsent queue as unknown. Agent
routing is checked both through the Axum service and an actual TCP request.

The outgoing writer also enforces 250 ms between sends of an area, so a stalled
socket cannot release a burst of old areas. The pending queue replaces the
area while waiting for that budget. Default connection capacity is 128;
connections close overloaded after 100,000 requests to bound duplicate-ID
tracking. The core retains at most 1,024 principal sessions at once. A blocking
maintenance job removes sessions after ten minutes without transport activity,
releasing their watches and outboxes. A fake-clock regression checks both
sides of that deadline, watch capacity recovery and refused resume.
The agent permit belongs to its blocking job, so aborting HTTP cannot bypass
the sixteen-poll limit. `CoreHandle::publish` accepts host-authorized auxiliary
pushes; `Config.features` and `Config.nwsync` populate the welcome advertisement.

Reproduction uses Rust 1.98.0 and the dependency versions in Cargo.lock
(Axum 0.8.9, Tokio 1.53.1 and tungstenite 0.29.0 at this implementation).
Run the targeted gateway and dev tests, then the three workspace gates listed
above. No test here launches Redis, a game server, Discord, a reverse proxy,
or an actual desktop link. A real-time test verifies two pings and that a silent client
is closed at the 45-second deadline. After tungstenite performs the upgrade,
that test reads raw frames: normal tungstenite reads automatically answer
pings, which can race the server close and invalidate a silent-client test.
Real cross-process journal recovery
is not proven here; the development host reuses the existing Live journal
and inbox implementation. Host-specific authentication,
role freshness, per-IP limits and authorization of auxiliary pushes remain
host integration responsibilities. Test success is not user acceptance or
approval to expose this development binary publicly.

P2a adds 23 gateway/dev tests, alongside 18 existing loopback
API tests (one manual TLS smoke remains ignored). `okf` is unavailable in both
the Windows and WSL environments used for this run; frontmatter, indexed
descriptions and all local source/link targets in the touched documents were
checked directly.

## Gateway audit invariants

Command authorization captures the session generation, principal groups, command
ID and game boot. `gateway::validate_accept` runs again under the final store
acceptance lock, before quota, identity or journal side effects. Session removal,
replacement, changed groups or a new boot invalidate an in-flight acceptance.
The binding survives independently of result delivery state. A failed retry
never removes the original pending result; cleanup checks the generation and
unique reservation created by that call.

The `audit_h1` store tests stop a command at the command-IO lock, then remove or
replace its session or restart the game; no stale command is accepted and no
missing-session error panics. A separate consent-revoked resend retains the
original outbox. Reproduce with `cargo +1.98.0 test -p mg-live --lib audit_h1 -j 2`
using the environment described above.

`spawn_scripted` is a separate capability: together with `spawn` it enables
`spawn_json`, but never `run_script`. The same Scope check protects command,
call and HTTP route dispatch; there is no gateway-only operation-name exception.
The `audit_m1` regression verifies all three paths and successful JSON spawning.

Request IDs use a 4,096-ID sliding replay window; IDs below its floor are refused,
including after their individual entries were evicted. Completed gateway result
IDs retain at most 4,096 entries for ten minutes. Mirror command history stores
32-byte fingerprints (at most 4,096 for ten minutes), not duplicate payloads.
The queued payloads and cached results each have a 4 MiB serialized byte budget;
results retain at most 256 entries for ten minutes. Reset releases completed
payloads and fingerprints, preserving bounded unknown outcomes for pending work.
Per-process counter high-water marks prevent retired IDs from becoming runnable
again, including across a reset; at most 64 processes share an ID prefix. A new
prefix requires pending work to be reset first. `audit_m4` tests exercise count,
byte and time limits, reset cleanup, owner-map cleanup and replay refusal.

Read-capable DM and AI sessions share command and skill-roll events, including
other principals' actions. Result delivery remains scoped to its owning session;
chat/tell capabilities remain separate. `audit_m5` checks both audiences with
two sessions and a terminal command result.

Acknowledging `unknown` confirms delivery, not command completion. Its outbox
entry remains until a late final result or the existing retention deadline,
so the unresolved-command registry can deliver a newer result revision. Only
ACKed `final` entries retire immediately. `audit_m3` checks this boundary; the
store acceptance suite covers late final delivery after the UI ring wraps.

The public host runs one `Gateway::tick` every 50 ms on a blocking worker, while
session pumps only build views. Expiry no longer runs once per viewer. Watch
admission is cached against the report boot/sequence, session watch/focus changes
and other watch owners; unchanged ticks do not rebuild or reapply the union. A
new gateway invalidates the cache and releases old session watches even when no
new client connects. `audit_e1` checks expiry ownership, unchanged tick/pump work,
watch changes and gateway restart. Direct core embedders must drive `tick`.

Event delivery advances only through scanned rows in each bounded batch. Hidden
rows still advance the cursor; gaps are counted only when crossed, not once per
remaining batch. A single event larger than 1 MiB is skipped and counted in
`lost`, allowing delivery to continue without an oversized frame or a stuck
cursor. `audit_e2` tests several byte-limited batches, hidden rows and an
oversized row. The differential harness drains these batches before comparing
stores and models reset's completed-result eviction; pending/unknown results
and newly replayed result revisions remain compared.

Run the audit unit tests with `cargo +1.98.0 test -p mg-live --lib audit_ -j 2`,
then `cargo +1.98.0 test -p mg-live --test gateway_core --test mirror_differential
-j 2` and the workspace gates above. All regressions use authored reports or
in-process sockets; no Redis or game server is needed.

## Gateway shared requests

`call` accepts read-only catalogue operations and `area_notes` (input `area`,
result `{notes: {object_id: text}, keys: {object_id: canonical_key}, revs: {object_id: u64}}`). Every read requires `read`.
`events.since` accepts `n`, optional `limit` (1..500, default 100), and `kinds`.
Pages preserve gateway event numbers, omit unauthorized chat/tells, and advance
`through` across filtered rows. Responses stay within 1 MiB; oversized rows count
as lost. Fetch the next page using `through`, never the last visible event alone.
Run `cargo +1.98.0 test -p mg-live --test gateway_requests -j 2` with
`CARGO_INCREMENTAL=0` and a nonexistent `NWN_ROOT`.

Presence is a revisioned snapshot of connected actors and object-to-principal focus.
A reconnect always gets a fresh snapshot; disconnected actors and stale focus are
removed while their result outbox remains resumable. Identity names are supplied by
the authenticated host, never by `focus.set`. Core and loopback WebSocket regressions
cover connect, focus, disconnect and reconnect.

`book.put` uses canonical `npc` (`tag:`/`resref:`), `gm` (empty key), and
`note` (store object-note key) identities. `if_rev: 0` creates; an existing or
changed revision returns `conflict`. Values and revisions are saved together
under the assistant writer lock, shared with local edits. Card reads include
`rev`; successful writes invalidate the key for every session. Existing saved
cards migrate to revision 1; an absent entry has revision 0. The dev host opens
one stable assistant context under its data directory. Unit persistence/CAS and
loopback two-session conflict tests require no game or Redis.

Managed AI identities live in the public core. `ai.create` takes `name`, optional
`groups` (default `read`, `read_chat`, `speech`) and `expires_ms` (default 30 days,
maximum one year); it returns `{ai, token}`. `ai.list` returns `{items}`;
`ai.update` takes `id` plus changed fields and returns `{ai}`; `ai.revoke` takes
`id` and returns `{}`. Creation always binds the caller DM and starts consent off.
Management needs `manage_ai`; a DM with `admin` can manage other owners' entries.
Client frames cannot set the owner or grant admin. v1 managed groups cover reads,
speech drafts and focus; other writes require typed proposals before delegation.

Tokens use `mgai_` plus OS randomness; only SHA-256 digests are persisted.
`Authenticator::principal(owner_id)` supplies the current DM groups on each AI
authentication. Missing owners fail closed; effective AI groups are intersected
with those groups. Revocation removes the credential and closes its sessions;
unknown managed tokens never fall through to host credentials. Both `/ws/v1`
and `/agent/v1` use this same path. The core rechecks consent and groups at final
command acceptance. `llm.allow {on}` needs `allow_llm` and changes only the caller's
existing AIs. The host's `set_llm_enabled` switch starts off and overrides it.
`identity_file` configures bounded persistence before serving connections (256 AIs,
1 MiB input). Imported host credentials also require an explicit owner and consent.
Tests cover permissions, current owner lookup, one-time token disclosure, persisted
digests, restart consent and rejection on both transports after revocation.

`draft.decide {id, text}` uses the store's existing review/send path. A string
approves (including edits); `null` rejects. The deciding DM needs `approve_drafts`
and must own the proposing AI; `admin` on a DM principal can decide any AI draft.
Ownership is checked under the draft lock, and approval revalidates the AI's
current permissions, consent, game boot and target before native enqueue. Draft
pushes are revisioned and filtered to the owner's sessions or admins. Local
assistant approval retains its original Access-based validation.

Note text, canonical keys and revisions come from one locked snapshot, so an edit
can send the exact `if_rev` it read. Book invalidations coalesce by key. A viewer
which falls behind more than 256 distinct auxiliary updates is closed as
`overloaded` and cannot resume that incomplete view; invalidations are never
silently discarded.

The loopback dev host persists managed identities in `gateway-ai.json` below its
configured `data_dir`, alongside the existing durable Live journal. It opens a
stable `gateway.mod` assistant context there (a logical identity, no game module
is required). Set `llm_enabled: true` in the private host configuration to enable
the global switch; new managed AIs still need their DM's `llm.allow` request.
Optional configured AI credentials must name an existing DM `owner` and explicitly
set `llm_allowed: true`; imports expire after 30 days. After first import, the
registry is authoritative. No generated token or token digest is logged.
Run `cargo +1.98.0 test -p mg-gateway-dev -j 2` for configuration checks and
`cargo +1.98.0 test -p mg-gateway --test transport --test agent -j 2` for the
in-process request and legacy HTTP parity tests.
