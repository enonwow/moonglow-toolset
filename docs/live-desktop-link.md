---
type: Reference
title: Live desktop connection and authentication
description: Desktop authentication, gateway transport, shared books, owned draft review and AI credential management.
tags: [live, gateway, authentication]
generated: { by: codex, at: 2026-10-18T14:30:21Z }
sources:
  - ../crates/mg-live-link/src/lib.rs
  - ../crates/mg-live-link/src/login.rs
  - ../crates/mg-live-link/src/credentials.rs
  - ../crates/mg-live-link/src/link.rs
  - ../crates/mg-live-link/src/facade.rs
  - ../crates/mg-live-link/src/state.rs
  - ../crates/mg-live-link/src/websocket.rs
  - ../crates/mg-live-link/src/worker.rs
  - ../crates/mg-live-link/tests/link.rs
  - ../crates/mg-live-link/tests/tls_config.rs
  - ../crates/mg-ui/src/live_backend.rs
  - ../crates/mg-ui/src/live_book.rs
  - ../crates/mg-ui/src/live_ai.rs
  - ../crates/mg-ui/src/live_drafts.rs
  - ../crates/mg-ui/tests/gateway_book.rs
  - ../crates/mg-ui/tests/gateway_ai.rs
  - ../crates/mg-ui/tests/gateway_drafts.rs
  - ../crates/mg-ui/tests/gateway_backend.rs
  - ../crates/mg-ui/tests/support/gateway_ui.rs
  - ../crates/mg-ui/tests/support/gateway_ops.rs
  - ../crates/mg-ui/tests/screens.rs
  - live-gateway-protocol.md
  - ../crates/mg-ui/src/options.rs
  - ../crates/mg-ui/src/settings.rs
  - ../crates/mg-ui/src/options_live.rs
  - ../crates/mg-ui/tests/live_connection.rs
  - ../crates/mg-ui/tests/screens.rs
---

# Live desktop link

`mg-live-link` owns desktop authentication, credentials and gateway connections. It never opens a browser or starts
a game server. The GUI supplies the gateway origin and optional SHA-256 leaf
certificate pin, starts `Login` on a worker, opens `browser_url`, then calls
`finish` on that worker. The attempt closes its listener on success, timeout, the probe limit or drop.
Invalid callbacks do not consume the login; all I/O shares the original deadline.

## Login and trust

The OS random source generates independent 32-byte PKCE verifier and state.
The verifier is unpadded base64url; the challenge is S256. `/auth/start` receives
`challenge`, `port` and `state`. Only `127.0.0.1:0` is bound. The single GET
`/cb?grant=...&state=...` must contain exactly one matching state and nonempty
grant. A text-only response has no external resources, no-referrer, no-store,
and a restrictive content policy. The listener closes before token redemption.

`POST /auth/token` sends `{grant, verifier}` and expects `{token}`. Errors and
Debug output never include the grant, verifier or token. Responses are bounded
to 8 KiB; redirects are refused. HTTPS uses ureq 2.12.1 and rustls 0.23 with
WebPKI roots. A saved pin authenticates the exact DER leaf certificate and
handshake signatures. An IP origin without a pin returns `TrustRequired`
with origin and SHA-256 fingerprint, after an aborting TLS discovery handshake
that sends no HTTP data. The caller must obtain explicit trust before retrying.
WSS origins are normalized to HTTPS for login; credentials, paths, queries and
fragments in configured origins are rejected.

## Reproduce

Use Rust 1.98.0, `CARGO_INCREMENTAL=0`, and `NWN_ROOT` pointing to a nonexistent
folder. Run `cargo +1.98.0 test -p mg-live-link -j 2`. No game install, external
service or real credentials are required. The fake gateway uses an ephemeral
loopback TLS listener and a generated certificate. Tests cover PKCE redemption,
callback response policy, wrong state, timeout, second callback refusal, pin
mismatch and TOFU without HTTP. This is local protocol proof, not acceptance
against a deployed Discord gateway.

Windows accepted sockets inherited nonblocking mode in the first fixture run,
causing `WouldBlock` during the handshake. Both accepted callback and fixture
sockets now explicitly switch to blocking mode before deadline-bound reads.
The happy-path TLS regression checks that behavior.

TLS API references: [ureq 2.12.1](https://docs.rs/ureq/2.12.1/ureq/),
[rustls](https://docs.rs/rustls/0.23/rustls/).

## Credential storage

`credentials::TokenStore` uses keyring 3.6.3 with Windows Credential Manager,
macOS Keychain or Linux Secret Service, one entry per server ID under
`org.moonglow.live`. Its backend is injectable. Only `Unavailable` permits a
fallback in the application's `live-credentials` directory; denied access fails
closed. IDs are hashed to filenames, so they cannot escape that directory.
Files are replaced atomically and synced. Unix uses directory 0700 / file 0600;
Windows uses a protected DACL granting only the current user full control,
applied through a hidden PowerShell process. Symbolic links/reparse points are
refused. The fallback is imported back into the keyring when it recovers;
replacement and sign-out remove obsolete fallback files. No session token is
serialized into settings. Tests use mock backends and temporary directories,
never the user's OS credential store. Unix modes require a Unix test run;
Windows fallback creation, replacement, migration and deletion are tested here.
Copying a complete existing Windows security descriptor through `Set-Acl`
required an unrelated audit privilege on this workstation. The implementation
instead builds a fresh access-only descriptor and applies it with .NET's
`SetAccessControl`; it requires no privilege elevation.

Keyring API: [keyring 3.6](https://docs.rs/keyring/3.6.3/keyring/).

## Settings and migration

`Settings.live_servers` is version 1 with stable IDs, names, gateway origins,
optional certificate pins and exactly one selection when nonempty. Missing
legacy data defaults to `Local (dev)`; Redis host, port and prefix remain in the
legacy `live` block until the development transport is retired. Redis ports
are not gateway HTTP ports, so they are never guessed into a gateway URL.
The initial development origin is `https://127.0.0.1:8080`; edit it or supply
`MOONGLOW_GATEWAY_URL` for the actual development gateway.

`Settings::migrate_live` imports the old token into the `local-dev` credential
entry and clears the in-memory plaintext only after success. Saving old
settings also imports an outstanding legacy token before serializing the
non-secret fields; storage failure refuses serialization. An existing newer
credential wins over an old legacy token. The emitted JSON/RON omits the old
`token` field. JSON round trips, migration with a mock store and desktop RON
round trips (including `keep_wsl: Some(false)`) are regression-tested.

Named legacy clients now use their unique case-insensitive name as the client
ID, with credential keys `local-dev/client/<id>`. Settings retain the name,
groups, delivery mode and rate limit, never the bearer field. Explicit migration
imports each old secret and clears it only after storage succeeds; newer stored
credentials win. Saving an unmigrated or rotated client imports its current
secret before emitting the reference. Local Live startup resolves these names
from secure storage; a missing credential fails startup instead of granting access.

The `named_client_migration_saves_references_without_plaintext` regression first
failed because JSON contained the client secret. It covers two separate client
entries, secret-free JSON, RON references and restoring credentials for runtime
use with an injected memory backend.

Opening Options resolves named-client references into the editable draft as
well. This is necessary even while the legacy service is disabled: otherwise
empty tokens would fail client validation and disable Apply after a restart.
The original settings remain unchanged until Apply. A storage error is shown
and leaves incomplete clients invalid. A loader-injected unit regression covers
successful restoration and denial without accessing the system credential store.

The existing client UI round-trip test now serializes only public configuration;
the mock-backed migration test owns secret persistence coverage.
`LiveServers::resolve` takes `MOONGLOW_GATEWAY_URL` and
`MOONGLOW_GATEWAY_TOKEN` over saved settings, without persisting either.
A URL override uses a distinct environment identity and cannot inherit a saved
server's certificate pin or credential. `resolve_with` tests this without
mutating process-global environment variables. Unsupported schema versions
and empty/duplicate IDs fail deserialization; selection is normalized.

## Certificate discovery deadline

TLS certificate discovery has one absolute deadline spanning connection and
handshake I/O. Socket timeouts alone renew on each received fragment; a peer
sending an incomplete record slowly could otherwise hold a login worker.
The `slow_certificate_discovery_cannot_extend_the_deadline` loopback regression
trickles an incomplete TLS record and verifies bounded termination. All seven
original login/TLS tests passed, including this case.

## Options and visual proof

Options > Live opens the server list with Add, Remove and selection, name and
URL editors, connection/account status and Discord sign-in/sign-out actions.
Authentication and credential I/O run on a worker; the UI opens the returned
browser URL through egui `open_url`. An IP gateway presents its origin and leaf
fingerprint for an explicit Trust or Reject decision. Existing saved sessions
are shown without exposing their token. Sign-out deletes the local credential;
remote revocation is outside this endpoint contract. Account actions are
immediate, while Apply saves the server list. A saved session locks the URL until
sign-out. Environment overrides are displayed as development credentials and
never saved by the account controls.

`Local development (legacy)` retains the prior local tools during the P2
transition. The default panel does not start Redis, a game server or a Live
WebSocket. HAK/module cache and My AI are explicit placeholders. P2 must wire
the authenticated link to the [gateway protocol](live-gateway-protocol.md);
P7 supplies the cache implementation. Account names and roles will come from
the gateway welcome message, rather than being guessed from token presence.

The two `live_connection` kittest regressions use an injected memory credential
backend and the same fake TLS gateway as the protocol tests. They click
Add/Select/Remove, apply the model, capture egui's browser URL, deliver the
callback, verify secure token persistence and click Sign out.

Render with `cargo +1.98.0 test -p mg-ui --test screens -j 2
live_gateway_options -- --ignored` (one command). The inspected image is
`target/test-output/screens/live-gateway-options.png`: the actual Options
window, isolated settings, no game install and no real account. The list,
fields, buttons and placeholder lines fit without clipping. This is visual
verification, separate from user acceptance.

Initial implementation verification (before the M2 audit fixes): `cargo +1.98.0 test -p mg-live-link -p mg-ui --lib
--test login --test credentials --test live_connection -j 2` passed 7 login,
4 credential, 271 UI library and 2 UI integration tests (one existing ignored
library test). The manual screen test passed. `cargo +1.98.0 fmt --check` passed.
The final workspace gate passed 1690 tests with 24 ignored:
`cargo +1.98.0 test --workspace --exclude mg-corpus-tests --lib --bins --tests -j 2`.
`cargo +1.98.0 clippy --workspace --all-targets -j 2 -- -D warnings` and the
separate `cargo +1.98.0 clippy -p mg-ui --test screens -j 2 -- -D warnings`
passed without warnings. An earlier shared-checkout gate stopped on three
Live authorization/receipt regressions; their owning changes resolved them
before this successful gate. No game runtime, deployed Discord integration,
macOS Keychain or Linux Secret Service acceptance is claimed.

The OKF executable was unavailable; the index was updated manually and local
source/document links checked. Keep sources, the lockfile and the one PNG as
reproduction inputs/evidence. Disposable verification logs are summarized in
the private task ledger and removed at closure; shared Cargo outputs remain
in use by concurrent work and are not deleted by this task.

## Loopback probe tolerance

The listener accepts at most 32 connections until one valid callback arrives.
Each connection has a 250 ms absolute read budget, capped by the overall login
deadline. Invalid complete requests receive 400 without redeeming a grant; idle
connections expire independently. Only a valid state/grant closes the listener
and begins token redemption. Reaching the attempt limit fails the login.

The garbage-then-real-callback regression failed with ConnectionRefused before
the fix. All ten login/TLS tests now pass, including a wrong-state probe followed
by success, an idle probe, the 32-attempt cap, the shared deadline and refusal
of a second callback after successful acceptance.

## Windows credential path handling

The hidden PowerShell ACL helper receives the directory/file path exclusively
through the child process environment variable `MG_ACL_PATH`; script text uses
`$env:MG_ACL_PATH` with `Get-Item -LiteralPath`. Escaping ASCII apostrophes was
insufficient because PowerShell also treats Unicode smart quotes as delimiters.
The Windows-only regression creates a directory containing U+2019 and exercises
fallback set/get/delete. It failed with `Permissions` before the fix and passes
afterwards. Paths are never interpolated into executable script text.

## UI menu frame scheduling

The menu regression opens the actual Live Options panel and checks its server
list. Background account loading can request an additional frame, so this test
uses three explicit harness steps instead of requiring repaint quiescence within
`Harness::run`'s four-step limit. The gate reproduced that limit before the fix;
the corrected menu regression passes in the 750-test scoped suite.

## M2 desktop verification

With Rust 1.98.0, `CARGO_INCREMENTAL=0` and `NWN_ROOT=/nonexistent`, the final
scoped gate passes 750 tests, with 19 ignored and no failures:

```sh
cargo +1.98.0 test -p mg-live-link -p mg-ui --lib --bins --tests -j 2
cargo +1.98.0 clippy -p mg-live-link -p mg-ui --all-targets -j 2 -- -D warnings
cargo +1.98.0 clippy -p mg-ui --test screens -j 2 -- -D warnings
cargo +1.98.0 fmt -p mg-live-link -p mg-ui --check
cargo +1.98.0 test -p mg-ui --test screens -j 2 live_gateway_options -- --ignored
```

Both clippy commands and the formatting check pass. The manual screen test
passes and both output images were inspected: `target/test-output/screens/`
contains `live-gateway-options.png` and `live-gateway-credential-error.png`.
The latter shows the denied-store message and disabled OK/Apply controls without
clipping. Six new M2 regressions cover named-client migration, draft restoration,
garbage and idle callback probes, the attempt cap and Windows smart-quote paths.
The three audit reproductions and the draft-restoration test failed before their
fixes. Existing wrong-state and UI round-trip/menu tests match the new contract.

The first scoped gate exposed the old UI expectation that a client secret
round-trips through settings. Its generated, namespaced fixture credential was
removed after checking its exact identity and write time; the revised test
never writes that credential to the OS store. A second gate exposed the menu's
repaint-quiescence assumption. Both are resolved in the final gate above.

This verifies local fake-gateway behavior and Windows fallback ACLs. It does not
claim a deployed Discord login, macOS Keychain, Linux Secret Service or a new
full-workspace gate. Temporary logs and rejected history-repair metadata are
summarized in the private task ledger and removed; the two PNGs remain as proof.
The shared build cache remains in use by other tasks.

## Desktop WebSocket transport (P2b part 1)

`LinkService::start(LinkConfig, &TokenStore, wake)` owns one server's passive
`LiveStore` and one background Tokio worker. Run startup on a worker too: the
credential backend and first-use IP certificate discovery can block. The token
comes from the entry identified by `server_id`; it is never a URL parameter or
serialized configuration field. `store()` returns the shared mirror; `snapshot()`
returns connection, account, drafts, presence, notices and book invalidations.
`PendingCall::try_recv` and the wake callback support egui without blocking it.
The GUI uses the unified backend described below; local servers retain `LiveService`.

The normative [gateway protocol](live-gateway-protocol.md) distinguishes `u32`
request IDs, unique on each connection, from game command IDs. The passive store
creates the latter from the session prefix, eight random process hex digits and
a monotonic counter. Commands retain their IDs across a dropped socket within
the same gateway epoch. A fresh snapshot resets queued commands to `unknown`;
unknown commands are never automatically resubmitted. Arbitrary RPCs are never
replayed after losing their response. A response timeout closes the connection
before releasing in-flight slots, because the peer might still be processing
those IDs. It does not replace a result already delivered by a push.

The client uses `/ws/v1?proto=1&mirror=1`, subprotocol `moonglow-gw.1`, an
Authorization bearer and no Origin header. TLS uses exactly the login rustls
configuration: WebPKI roots or a previously accepted leaf fingerprint, including
signature verification. IP TOFU returns the existing login trust prompt before
any HTTP data. Redirects and WebSocket extensions are refused. The bounded HTTP
upgrade and RFC 6455 framing are local code: a generic WebSocket implementation
was rejected because its trace logging includes raw Authorization bytes, even
when the original header is marked sensitive. Binary, masked server, malformed,
invalid UTF-8 and oversized frames fail without exposing remote error text.
Fragments and interleaved ping/pong are supported; the assembled server message
limit is 16 MiB. Client frames are at most 64 KiB, or 256 KiB for `book.put`,
further restricted by welcome/session limits. Queued and outstanding requests
share 64 permits; a negotiated smaller limit also bounds wire requests.

The shared `mg_live::gateway_frames` decoder handles known JSON variants after
redaction. Unknown variants are skipped with a static warning, avoiding its
raw-value compatibility log. Incoming strings and map keys redact the current
session token before entering snapshots, replies or notices; transport errors
contain no HTTP bodies, remote messages or credentials. This also applies when
all log levels are enabled.

All eleven push topics are handled. Meta/area/area_gone/boot/events/result feed
P1 mirror APIs; drafts/presence/book/session/notice update auxiliary GUI state.
Area revisions, boot, gateway epoch and the server event `through` cursor form
resume state. Duplicate area revisions are ignored; P1 keeps original event
numbers and deduplicates replay. Result ACKs contain `(id, rev)` and the event
cursor, sent once per second or after 32 received items. Notices retain the last
128 entries; every book notification advances a local invalidation revision.
Revoked/role_changed/upgrade/replaced stop the service for explicit user action;
overload/shutdown/socket failures reconnect with 0.8-1.2 jitter around
1/2/4/8 seconds, capped at ten seconds. A cancelled service drops its private
socket and reader task; it never starts a local API, Redis or a game server.

### Transport reproduction and limits

Use Rust 1.98.0 with `CARGO_INCREMENTAL=0` and `NWN_ROOT=/nonexistent` (ensure the
folder does not exist). Run:

```text
cargo +1.98.0 test -p mg-live-link -j 2
cargo +1.98.0 clippy -p mg-live-link --all-targets -j 2 -- -D warnings
cargo +1.98.0 fmt -p mg-live-link --check
```

The integration fixture embeds `mg-gateway::router` and `CoreHandle` as a dev
dependency, behind an ephemeral loopback TLS proxy with a generated certificate.
It uses synthetic credentials and scripted reports, not Redis or a game engine.
The proxy observes only post-upgrade client JSON (never Authorization headers)
and can drop sockets or inject malformed server frames. Tests compare mirrored
areas, factions and events with the authoritative store, verify resume cursors
and no duplicate chat events, and observe result ACK revisions. Pin mismatch,
frame limits, fragmentation, request admission, timeout and trace-level secret
redaction have separate regressions. This proves interoperability with the
in-process development host; production DNS certificates, deployed gateway auth
and a real game session are outside this verification.

## GUI facade and migration inventory

`LinkService` supplies synchronous mirror/snapshot reads and asynchronous RPC
handles. `set_watched_areas`, `set_focus`, `call`, `decide_draft`, the card/note
save methods, `set_allow_agents` and AI management methods construct normative
requests. Desired area watches also update while offline, so closing a view
changes the next hello without touching global campaign data. Book writes require an explicit gateway key and `if_rev`; a conflict
is a typed `LinkError::Conflict`. `allow_agents()` is unknown until a successful
`llm.allow` response, and a failed request does not change the confirmed value.
`drafts()` reconstructs the shared `Draft` type from pushes; malformed drafts
return a protocol error. `agents()` preserves gateway actor records, rather than
pretending they are observations of a local API. `take_notices()` drains the
bounded notification queue into the future GUI log without inventing events.

The part 1 checkpoint had 23 direct `LiveService`/`live_service` references plus
two startup operation blocks: the 25 historical migration sites below. Part 2
now routes GUI callers through `Moonglow::live_backend()`. The public local
service field remains an embedding/test injection point; widgets do not branch
on both service types.

| Site under `crates/mg-ui/src/` | P2b part 2 mapping |
|---|---|
| `commands.rs:625` | Service lifetime controls the Stop Live label. |
| `commands.rs:680` | Enable LLM action using connection/account state. |
| `commands.rs:692` | LiveInArea uses connection state plus an open view. |
| `commands.rs:887` | Start/stop the selected server's link. |
| `lib.rs:602` | Keep local injection compatibility; the backend also owns a gateway runtime. |
| `lib.rs:861` | Preserve the initially absent service. |
| `live_agents.rs:57` | `agents_revision()` and pushed `agents()` snapshot. |
| `live_agents.rs:72` | `set_allow_agents()` returns a polled request handle. |
| `live_agents.rs:90` | Hint reads confirmed `allow_agents()`; unknown is not consent. |
| `live_agents.rs:118` | Toolbar checks `is_connected()`. |
| `live_assistant.rs:52` | Cache identity uses `server_id()`. |
| `live_notes.rs:19` | Note editor cache uses `server_id()`. |
| `live_targets.rs:19` | Enable targets from connection state. |
| `live_view.rs:252` | Enable Live controls from connection state. |
| `live_view.rs:991` | Offline state comes from the link snapshot. |
| `live_view.rs:1255` | Offline state comes from the link snapshot. |
| `live_view.rs:1347` | Gate controls using connection, groups and mirrored policy. |
| `options.rs:305` | Dropping the old link stops only its own worker/socket. |
| `options.rs:306` | Startup returns `Result<Option<LinkService>, StartError>`. |
| `options.rs:317` | Worker calls `LinkService::start` with selected server and TokenStore. |
| `options.rs:323` | Remove local API descriptor publication; gateway owns agent access. |
| `options.rs:329,341` | Read selected-server config/status; no local API token generation or Redis log. |
| `options.rs:346` | Install the new service and its own `store()` mirror. |
| `options.rs:562` | `connection_status()` feeds diagnostics. |
| `options.rs:563` | `config()` supplies non-secret link settings. |

Additional direct-store operations must move with those sites; ordinary
`area`, `find`, `locate`, `events`, `result`, reconciliation, queue and scene
operations continue to use the passive `LiveStore` returned by `store()`.

| Direct-store GUI sites | Facade replacement |
|---|---|
| `live_layout.rs:96` | `set_watched_areas` sends `watch.set`; never enqueue native watch commands. |
| `live_drafts.rs:20,29,119` | `draft_revision`, typed `drafts`, async `decide_draft`. |
| `live_assistant.rs:102,104,193` | Async `npc_card` / `gm_card`; consume the gateway response. |
| `live_assistant.rs:116,118` | `save_npc_card` / `save_gm_card` with gateway key and fetched revision. |
| `live_assistant.rs:201,204,215` | `assistant_revision` invalidates local views; async `npc_memory` carries persistence errors in its response. |
| `live_notes.rs:12,28,31,57` | `object_note_key`, `book_revision`, async `object_note` (area note response), `save_object_note`. |
| `live_view.rs:178,180` | Book revision plus async `area_notes` cache. |
| `live_assistant.rs:65,67` | Remove local assistant context setup/clear. Closing a module only updates subscriptions; campaign data belongs to the gateway. |

`assistant_revision()` combines local book invalidation and mirrored event
cursor for GUI caching. It is not a server book revision for optimistic writes.
The mirror's server policy is read-only from the desktop; no invented policy
write operation or local campaign-directory API is exposed.

### Protocol contracts and pending host operations

At the P2b transport boundary the development host implemented `watch.set`,
`focus.set`, `cmds`, read `call` and `events.since`, and returned `unsupported`
for shared writes and AI controls. Newer host handlers now exist (see
[live-library.md](live-library.md#gateway-shared-requests)). The facade regression
therefore checks explicit refusals: `Forbidden` for consent without permission,
`Rejected` for a write without a configured book, and `Unsupported` for an
unknown operation. These refusals never become local success. The P3 desktop integration below adds revision-aware shared books and draft decisions.
Adding a host handler alone never enables an unfinished desktop workflow.

Book identities are now defined: NPC keys are the store's `tag:`/`resref:` ids,
the GM key is empty, and note keys are derived per object. Actors carry `id`,
`kind` (`dm`/`ai`), `name`, nullable `avatar`, `owner` and `area`. Focus maps object
ids to actor ids. LLM consent starts off. The P3 GUI uses the revision-bearing reads and asynchronous decisions described
below. Missing per-AI consent/avatar hooks remain explicit. No action silently writes a local book
while displaying a gateway mirror.

### P2b desktop verification (2026-10-18)

Desktop source commits `586209e` and `b0224c5` passed the scoped gate above on
Windows with Rust 1.98.0: **33 passed, 0 failed, 0 ignored** (12 unit tests,
6 gateway integration tests, 5 credential tests and 10 login tests). Eighteen
of these tests are new transport/facade regressions. Clippy `--all-targets`
with `-D warnings` and `fmt -p mg-live-link --check` both passed. This is a
scoped desktop gate, not a claim that the complete workspace or a deployed
game/gateway was accepted. The production GUI was not switched or rendered.

Two fixture details matter when reproducing the tests: event cursor convergence
can precede the coalesced area update, so equality waits for both. A socket-drop
notification must be marked observed when a new proxy connection starts;
otherwise the fixture accidentally drops every reconnect as well as the intended
old socket. Both conditions are encoded in the retained integration fixture.

Canonical frontmatter sources and relative Markdown targets were checked to
exist. The OKF executable was unavailable, so no OKF CLI validation is claimed.
The reproduction needs only the committed sources and Cargo.lock; ephemeral
listeners/certificates and temporary credential directories are test-owned.
The shared Cargo build cache remains in place for other workspace consumers.

## Unified GUI backend (P2b part 2)

Select a saved server in Options > Live, sign in, enable Live, and Apply. The
`local-dev` identity keeps existing local service, descriptor and Redis behavior.
Other entries start `LinkService` on a background thread using the same
injected/system `TokenStore` as sign-in. Environment URL/token resolution keeps
precedence; environment credentials are never persisted. Changing server settings
restarts only that backend. Startup completion installs its own passive mirror;
a stale startup receiver cannot install a previous server's link.

`LiveBackend` centralizes runtime selection, configuration/status, commands,
presence and draft snapshots. `app.live` remains the shared `LiveStore` read
surface for areas, Panel, Log, Chat, Focus and results. Commands pass through
the existing scene recorder and link `cmds` requests with stable ids, boot and
bounded TTL. Absolute area/Focus subscriptions change only when needed and are
republished after reconnect. Focus persistence uses server identity rather than
the current module's local campaign book. Sign-out stops the matching runtime.

Connection state appears in the Live menu, Panel, Log and Options. Unknown
outcomes remain visible in mirror result history after per-frame logging drains;
the message explains that the command may have run. Presence chips show names
and DM/AI roles in Focus and on map markers. They fetch no external avatars and
never represent locks. Gateway tools open a wider default right pane so Focus
keeps its two columns and composer visible; the local split is unchanged.
The local global AI switch is disabled in gateway mode. Live > AI agents owns
remote AI management; per-AI consent requires an explicit host capability so an
older owner-wide endpoint cannot silently grant access to every AI.

The integration test uses `mg-gateway` in-process behind an ephemeral loopback
TLS proxy, sharing the authored `mg-live-link` fixture. A temporary authored
module and mock credentials require no game process, installed assets, Discord
account or real credentials. Reproduce with Rust 1.98.0,
`NWN_ROOT=/nonexistent` and `CARGO_INCREMENTAL=0`:

```sh
cargo +1.98.0 test -p mg-ui --test gateway_backend -j 2
cargo +1.98.0 test -p mg-ui --test screens -j 2 live_gateway -- --ignored
cargo +1.98.0 test --workspace --exclude mg-corpus-tests --lib --bins --tests -j 2
cargo +1.98.0 clippy --workspace --all-targets -j 2 -- -D warnings
cargo +1.98.0 clippy -p mg-ui --test screens -j 2 -- -D warnings
cargo +1.98.0 fmt --check
```

The before images are `target/test-output/screens/live-gateway-before.png`
and `live-gateway-focus-before.png` (the clipped narrow composer).
The screen harness writes Options, Focus, map-presence, connected, reconnecting
and unknown-result PNGs in
that same directory. Source fixtures/tests are reproduction inputs; PNGs are
local visual evidence. Shared Cargo outputs remain in use by concurrent work;
only owned disposable logs and temporary fixtures are removed after recording
final verification results.

### GUI fixture synchronization

The presence fixture waits for the host's own `Test` DM chip after `focus.set`,
then publishes the authored DM/AI actors and waits for their mirror delivery.
Watching only the outgoing request raced the automatic presence snapshot added
in `7ddbb10`; the snapshot could replace the visual fixture before rendering.
This barrier tests actual host-to-mirror delivery without fixed settling sleeps.
The authored NPC keeps a stable tag/name across reports: changing an untagged
NPC's identity correctly invalidates Focus, so it must not be used to simulate
an ordinary command result. Marker rendering is asserted through its accessible
label before capturing the image.

### Reusing gateway TLS

`Gateway::tls_config()` exposes the existing certificate policy to other
workspace clients without opening a socket or sending credentials. Literal IPv4
and IPv6 origins without a saved pin return `Error::UntrustedGateway`. Discover
and explicitly approve the fingerprint through the existing trust flow first.
DNS origins without a pin use public WebPKI roots; a saved pin takes precedence.
The returned `Arc<rustls::ClientConfig>` contains no session credential. Callers
remain responsible for HTTP limits, redirects, timeouts and credential handling.

The `tls_config` integration regression checks missing IP trust, a matching pin,
a mismatched pin and rejection of a self-signed certificate by public roots:

```sh
cargo +1.98.0 test -p mg-live-link --test tls_config -j 2
```

### GUI verification (2026-10-18)

Rust 1.98.0 on Windows, `NWN_ROOT=/nonexistent`, `CARGO_INCREMENTAL=0`, at most
`-j 2`:

| Check | Result |
| --- | --- |
| `mg-ui --lib --tests` | 738 passed, 0 failed, 19 ignored; existing local UI tests unchanged |
| `mg-live-link --lib --tests` | 34 final passing cases; the six transport tests were rerun after updating the obsolete unsupported-operation expectation |
| `screens live_gateway -- --ignored` | 2 passed; all generated states visually inspected |
| Clippy for both crates, all targets, `-D warnings` | Passed |
| Clippy for manually named UI screen/performance tests | Passed |
| Workspace `fmt --check` and scoped format checks | Passed |

The exact workspace test command was also run. Its concurrent source snapshot
stopped in `mg-live`: 427 passed, 3 failed, 4 ignored. The failing tests were
`restart_rejects_reserved_ids_and_scopes_them_to_server_and_boot`,
`ai_registry_is_checked_before_poll` and
`audit_m5_readers_share_command_log_but_results_remain_private`. Workspace Clippy
stopped on precedence in `store_history_tests.rs` and unused `Journal::contains`.
The owners were notified and the subsequent full-workspace gate belongs to their
ongoing changes; this record does not claim final workspace acceptance.

The new GUI unit tests cover isolated gateway consent, sign-out retaining only a
passive mirror, stable server Focus identity, unchanged local panel defaults and
bounded actor labels. The in-process UI test covers mirror equality, the real
Focus composer, boot/TTL command requests, result delivery, reconnect/recovery,
DM/AI chips and persistent unknown outcomes. P3 enables the shared-card entry and
verifies revisioned editing separately below. The screen
harness also asserts the marker's accessible presence label before capture.

Inspected after-images under `target/test-output/screens/`:
`live-gateway-options.png`, `live-gateway-credential-error.png`,
`live-gateway-focus.png`, `live-gateway-connected.png`,
`live-gateway-reconnecting.png`, `live-gateway-unknown.png`, and
`live-gateway-markers.png`. The wider gateway pane keeps the composer visible;
connection/unknown text fits the menu and the map chip identifies both actors.
The authored area intentionally has no game tiles or models, so these pictures
prove GUI/mirror behavior, not game-asset rendering or a deployed server.
No real Discord account or game server was used.

Reproduce the final scoped checks with the environment above:

```sh
cargo +1.98.0 test -p mg-live-link -p mg-ui --lib --tests -j 2
cargo +1.98.0 clippy -p mg-live-link -p mg-ui --all-targets -j 2 -- -D warnings
cargo +1.98.0 fmt -p mg-live-link -p mg-ui --check
```

Source fixtures, the before/after PNGs and compact verification records are
retained for review. Superseded owned diagnostic logs are disposable after this
record; the shared build cache and shared failed-gate evidence remain in use.
No private data or rendered artifacts are committed. Frontmatter source paths
and Markdown targets were checked; the OKF executable was unavailable, so no
OKF CLI validation is claimed.

## Unknown command outcomes

A result whose code or state is `unknown` is a warning even if an inconsistent
payload also sets `ok`. Both the Live event log and application output say
"outcome unknown - it may have run; not retried". Neither path claims success
or failure. Regression tests exercise both values of `ok`:
`cargo +1.98.0 test -p mg-ui --lib -j 2 unknown_` (with `NWN_ROOT=/nonexistent`
and `CARGO_INCREMENTAL=0`) passes four matching tests, including both new cases.

## Gateway shared editors (P3)

`live_book.rs` keeps server-scoped editor state separately from the passive game
mirror. NPC and GM reads return a card and revision; area notes return text plus
canonical per-object keys and revisions. An absent note text is empty, not a
missing key. All saves use `book.put` with the last read `if_rev`. No gateway
editor writes the local assistant directory.

A book push or a new connection invalidates the read cache. Clean editors reload;
dirty editors keep their text and show both versions if the key or revision has
changed. A CAS rejection also fetches the newer version. **Keep mine** submits
against that newer revision (and can conflict again); **Take newer** discards
only the current unsaved edit. Save errors are displayed and never automatically
retried. **Reload to verify** permits an explicit read after an uncertain result.

The `gateway_book` UI regression passes against the in-process TLS host. It
covers push invalidation, keeping the local edit, an actual stale-revision CAS
rejection, taking the newer version, canonical NPC keys and saving an initially
empty note from Focus. Two unit cases cover dirty-version preservation and note
response validation. The fixture keeps the Live area watched and closes its
floating chat before editing; otherwise that unrelated window occludes card
buttons. Render assertions also require the conflict to disappear after resolution.

Draft decisions use the [gateway review flow](#gateway-draft-review).

## Gateway AI management

Live > AI agents (also the gateway toolbar and Options > My AI) lists authorized
AIs, creates them, updates names and revokes credentials. Creation displays the
credential once with Copy and an explicit reminder; acknowledging or closing
the window drops the in-memory display. List/refresh cannot retrieve it. No
credential is written to settings or application logs. New AIs start with consent
off. The current host's `llm.allow {on}` affects every AI of the owner, so the
per-AI checkbox stays disabled until the host explicitly advertises an id-scoped
operation. The current identity schema also lacks an avatar update field; this
requires a host contract before the desktop can safely expose it.

The `gateway_ai` in-process UI regression passes: create, copy, close/reopen
without redisplaying the token, update and revoke. The initial test caught an
automatic list refresh clearing the saved-status message; refresh now preserves
that message. Credentials never enter the application log.

## Readable Focus presence

Focus chips now use the same font policy as map chips (the normal small font,
with an 11-point readability floor), white text, blue fill, rounded corners and
four-by-two-point padding. The in-process `gateway_backend` regression checks
the accessible chip height as well as both DM/AI labels; it passes against the
development host. Presence conveys focus only and never locks an object.

## Gateway draft review

The gateway filters draft pushes to each DM's own AIs; administrators receive all
permitted drafts. The GUI sends approve/edit/reject through `draft.decide`, keeps
an asynchronous pending state, and avoids duplicate clicks while a decision is
outstanding. Local draft decisions retain their existing store path.

`gateway_drafts` exercises two DM identities and their managed AIs plus an admin:
owner-only versus admin visibility, edited approval, admin rejection and owner
rejection. The in-process TLS host regression passes. No real AI account, game
server or credential store is needed.

## P3 verification and limits

Run with Rust 1.98.0, `NWN_ROOT=/nonexistent`, `CARGO_INCREMENTAL=0` and at most
two Cargo build jobs. The tests use the workspace development host, temporary
book directories, injected credentials and pinned loopback TLS; no game install,
Discord account or production gateway is required.

```text
cargo +1.98.0 test -p mg-ui --lib --tests --no-fail-fast -j 2
cargo +1.98.0 test -p mg-live-link --lib --test credentials --test link --test login --test tls_config -j 2
cargo +1.98.0 clippy -p mg-ui --all-targets -j 2 -- -D warnings
cargo +1.98.0 clippy -p mg-ui --test screens --test frame_perf --test world_perf --test campaign_perf --test editor_perf --test nui_perf -j 2 -- -D warnings
cargo +1.98.0 test -p mg-ui --test screens -j 2 live_gateway -- --ignored --test-threads=1
cargo +1.98.0 fmt -p mg-ui --check
```

The before proofs reuse the accepted P2b Focus/unknown images, copied to
`target/test-output/screens/live-p3-focus-before.png` and
`target/test-output/screens/live-p3-unknown-before.png`. Current outputs use
`live-gateway-focus.png`, `live-gateway-unknown.png`, `live-p3-book-conflict.png`,
`live-p3-book-resolved.png` and `live-p3-ai-agents.png` in the same directory.
The AI capture follows dismissal of the one-time credential. The conflict view
uses read-only fields for the saved version, preserving ordinary field spacing;
an accessibility-rectangle assertion checks corresponding rows. A shared wait
helper uses a non-panicking multi-result query while async controls are absent.
The stale-write regression checks CAS refusal directly: a book invalidation may
legitimately arrive before the UI save click, so the test does not require a
redundant stale request to appear in the frame trace.

The exact workspace test and Clippy commands were attempted during concurrent
host work. Test compilation stopped on an unfinished translation-test delimiter;
Clippy stopped on a missing JSON type annotation in the bridge. Their owners
subsequently corrected those transient source errors. Workspace formatting
passed. These snapshots do not establish a clean final workspace gate; the host
owners coordinate its next run after their sources freeze. The P3 scope is
verified separately. Per-AI consent and avatar editing/presence remain blocked
on the explicit host contracts described above, and have no passing UI claim.

The retained evidence is the before/after PNG set and a compact local verification
record. Superseded diagnostic logs and the one-off gate runner were removed
after their failure causes were recorded. Shared build outputs remain available
to the other workspace tasks. No application or agent history was removed.
