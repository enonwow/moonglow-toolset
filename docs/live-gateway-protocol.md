---
type: Reference
title: Live gateway protocol
description: Normative moonglow-gw/1 JSON envelopes, finished-state mirror and session delivery contract.
tags: [live, gateway, protocol]
generated: { by: codex, at: 2026-10-18T12:00:00Z }
sources: [../crates/mg-live/src/gateway_frames.rs, ../crates/mg-live/src/store_gateway.rs, ../crates/mg-gateway/src/lib.rs, ../crates/mg-gateway/src/avatars.rs]
---

# moonglow-gw/1: the Moonglow ↔ gateway protocol (draft 1)

Status: draft for P1, written 2026-10-18 from the architecture (v6) and the
two code audits. Normative words: MUST, SHOULD, MAY. This public protocol describes the mirror and gateway core. Identity providers,
the management panel and translations are host concerns.

## 1. Model

- The **gateway** owns the only Redis link to the game and the authoritative
  `LiveStore` (today's mg-live code). It validates and attributes every
  command and creates every result and event.
- Each **Moonglow** keeps a **passive mirror**: a `LiveStore` in mirror mode
  that holds exactly the data the gateway sends and computes nothing itself
  (no report parsing, deltas, derived events, expiry, drafts or journal). The
  GUI keeps reading it through today's API (`area`, `find`, `locate`,
  `revision`, `events`, `result`, `status`, `factions`, `policy`, …).
- The mirror receives **finished state**, not game frames: whole areas as
  `AreaLive` (with `has_full`, `full_ms`, `seq`), meta, results and events.
  This keeps `reconcile()` and the motion code correct without replaying the
  gateway's decisions. Object-level diffs are a later optimisation (§9).

## 2. Transport

- `wss://<host>/ws/v1`, subprotocol `moonglow-gw.1`. One WebSocket per
  Moonglow. JSON text frames, UTF-8, one object per frame.
- Auth on the upgrade: `Authorization: Bearer mgs_<token>`. 401 without
  upgrade on a bad token; 426 with `{"min_client": "<version>"}` when the
  client's `hello.proto` or `hello.mirror` is not the gateway's (v1 requires
  equality). A request with an `Origin` header is refused (403).
- Frame limits: client → gateway 64 KiB (`book.put` 256 KiB); gateway →
  client 16 MiB. A larger frame closes with code 1009 (message too big).
- Keep-alive: WebSocket ping every 15 s from the gateway; idle 45 s closes.
- Unknown fields MUST be ignored. Unknown `t` or `topic` from the gateway
  MUST be logged and skipped. Unknown `op` from a client → `err unsupported`.

Implementation note (P2a): HTTP status is final after upgrade. The public
transport accepts optional `?proto=1&mirror=2` on the upgrade URL and returns
426 with `min_client` for a mismatch there. It always validates the actual
hello too; a mismatch after upgrade sends `bye upgrade`. Omitting the query
is supported. This is the transport resolution of the draft's 426 sequencing
ambiguity, not a change to hello or welcome JSON.

## 3. Envelope

```text
C→S {"t":"hello", ...}                         first frame, once
S→C {"t":"welcome", ...}                       reply to hello
C→S {"t":"req","id":<u32>,"op":"<op>","a":{...}}
S→C {"t":"res","id":<u32>,"ok":true,"r":{...}}
S→C {"t":"res","id":<u32>,"ok":false,"err":{"code":"<code>","msg":"<text>"}}
S→C {"t":"push","topic":"<topic>","body":{...}}
C→S {"t":"ack", ...}                           see §6.3
S→C {"t":"bye","code":"<code>","msg":"<text>"} before the gateway closes
```

- `req.id` is chosen by the client, unique per connection, ≤ 64 in flight.
- Error codes: today's `ErrorCode` values (`invalid`, `forbidden`,
  `not_found`, `expired`, `conflict`, `rate_limited`, …) plus `unsupported`,
  `upgrade`, `unknown_outcome`.
- `bye.code`: `revoked`, `role_changed`, `overloaded`, `shutdown`,
  `upgrade`, `replaced` (the same session connected again).

## 4. Handshake

```json
{"t":"hello","proto":1,"mirror":2,"client":"moonglow/1.33.0",
 "resume":{"gw_epoch":"e81f…","boot":"b-17…","event_n":4120,
           "areas":{"harbor":57,"market":12}},
 "watch":["harbor","market"]}
```

```json
{"t":"welcome","proto":1,"mirror":2,"gw_epoch":"e81f…","now_ms":1792230000000,
 "server":{"id":"main","name":"Main server"},
 "user":{"id":"dm:123456789","kind":"dm","name":"Operator","avatar":"https://cdn.discordapp.com/…"},
 "groups":["read","read_chat","read_tells","variables","npc","speech","spawn","spawn_scripted","players","focus","approve_drafts","allow_llm","manage_ai"],
 "policy":{"players":true,"scripts":false,"chunks":false},
 "id_prefix":"d7k2.",
 "limits":{"frame":65536,"book_frame":262144,"in_flight":64,"watch":16},
 "features":["drafts","nwsync","translate"],
 "nwsync":{"haks_url":"https://…","haks_manifest":"<sha1>","module_manifest":"<sha1>"},
 "resumed":true}
```

- `resume` is omitted on a first connection. `resumed` tells whether the
  gateway honoured it (same `gw_epoch`, same game `boot`, cursors still in
  retention). When `resumed` is false the client MUST reset its mirror
  (`mirror_reset`) before applying the pushes that follow.
- After `welcome` the gateway sends, in this order: `meta`, every area the
  client lacks or holds at an older `rev` (`area`), the client's outstanding
  results (`result`), events after `event_n` (`events`), then `drafts`,
  `presence` and `book` snapshots. Live pushes may interleave after that.

## 5. Pushes (gateway → client)

| topic | body | mirror action |
|---|---|---|
| `meta` | `{"boot","rev","factions","watched":{"desired":[],"reported":[]},"policy","game":{"connected":bool,"age_ms":u64},"redis":{...}}` | `mirror_meta` |
| `area` | `{"boot","area","rev","age_ms","live":<AreaLive>}` | `mirror_area(area, live, now-age_ms)` |
| `area_gone` | `{"boot","area"}` | remove the area |
| `boot` | `{"boot","prev"}` | `mirror_reset("boot")` then wait for `meta`/`area` |
| `events` | `{"rows":[<Event>...],"through":u64,"lost":u64}` | `mirror_events(rows, through)` |
| `result` | `{"id","rev","state","result":<CommandResult>\|null}` | `mirror_result` |
| `drafts` | `{"rev","items":[<Draft>...]}` | LinkService state (not the mirror) |
| `presence` | `{"rev","actors":[...],"focus":{"<object id>":["dm:…","ai:…"]}}` | LinkService state |
| `book` | `{"kind":"npc\|gm\|note","key","rev"}` | invalidate; GUI re-reads via `call` |
| `session` | `{"groups","policy","limits"}` | update gating |
| `notice` | `{"level":"info\|warn","text"}` | show in the Live Log |

- `area.rev` increases each time the gateway's copy of that area changes
  (new report applied, object added/removed). `AreaLive` is serialised with
  its Rust field names; `received_ms` is replaced by `age_ms` relative to the
  gateway clock. The mirror stores `received_ms = now_local - age_ms`.
- The gateway sends an area to a session when the session watches it, when
  it holds a PC, or when a session's Focus or follow target is in it
  (§7). It MAY send every reported area to admins.
- Coalescing: per session and area the gateway keeps only the newest pending
  `area` push; at most 4 pushes per area per second.
- `meta` is pushed when one of its fields changes. `game.age_ms` changes every
  millisecond, so it counts only once it has moved by a second since the `meta`
  the session last received (a gateway that compared it exactly sent every
  session a `meta` frame on each 50 ms poll). The bridge's running counters
  (`redis.received`, `redis.published`, `redis.lastStateMs`, `redis.events.applied`,
  `.backlog`, `.last_id`) change with every report and event and are treated the
  same way: they ride with the next `meta` pushed for another reason (a
  connection, `lost`, policy, watch or boot change, or a second of age). A
  client must not rely on them for anything but a diagnostic display.
- `area.live` is the same text for every session watching the same `rev` (the
  gateway serializes an area once per revision and shares it); frames of two
  sessions differ only in `age_ms`. The bytes are those a per-session encoding
  would produce.

## 6. Commands and results

### 6.1 `cmds` (client → gateway)

```json
{"t":"req","id":17,"op":"cmds","a":{"commands":[
  {"id":"d7k2.9f3a01c2.41","area":"harbor","op":"npc_say","target":"7f00012a",
   "text":"Halt!","boot":"b-17…","ttl_ms":30000}]}}
```

```json
{"t":"res","id":17,"ok":true,"r":{"accepted":["d7k2.9f3a01c2.41"],
  "rejected":[{"id":"d7k2.9f3a01c2.42","code":"forbidden","msg":"…"}]}}
```

- Every command MUST carry `boot` (the game boot the client validated it
  against) and SHOULD carry `ttl_ms` (default 30 000). The gateway rejects a
  command whose `boot` is not the current one (`expired`) and the game
  refuses one whose boot is not its own.
- `id` MUST start with the session's `id_prefix`, then 8 random hex
  characters chosen once per Moonglow start, then a counter. Ids are the
  idempotency key: the same id with the same command is accepted once.
- The gateway authorises each command through the same path as the HTTP
  API (`operations::command_scoped`): the operation's group, the policy,
  player protection and owner checks. `watch_areas` is refused here and on
  `/agent/v1` (watching is `watch.set`, §7). `resync` is never accepted.
- Rejected ids come back only in `res.rejected`; the client writes a failure
  result into its mirror at once (`mirror_reject`) and removes the command
  from its queue.

### 6.2 Result states

```text
accepted ──► pending ──► final
   │            │
   └────────────┴──► unknown   (gateway restart, lost game reply, retention)
```

- `result.state` is `pending` (for example a skill check waiting for the
  player's roll) or `final` or `unknown`. `rev` increases per state change of
  that id; a `final` never changes again.
- `unknown` means the gateway cannot tell whether the game ran the command
  (its reply was lost or the gateway restarted). It is never shown as
  success, and the client never retries it automatically.

### 6.3 `ack` (client → gateway)

```json
{"t":"ack","results":[["d7k2.9f3a01c2.41",2]],"event_n":4130}
```

- The client acknowledges results by `(id, rev)` and events by cursor, at
  most once a second or every 32 items. The gateway keeps each session's
  unacknowledged results (≤ 256 per session, ≤ 10 min) and resends them on
  resume; an acknowledged `final` is dropped.

## 7. Other requests

| op | args | result | notes |
|---|---|---|---|
| `watch.set` | `{"areas":[…]}` | `{"accepted":[…],"waiting":[…]}` | absolute set for this session; the gateway unions sessions (game limit 64 areas) and drives `watch_areas` itself |
| `focus.set` | `{"objects":[{"area","id"}…]}` | `{}` | ≤ 256 entries; absolute set |
| `call` | `{"op","input"}` | operation result | read-only operations only (allow-list from the catalogue: `find`, `status`, `npc_card`, `gm_card`, `npc_memory`, `area_notes`, `events` without waiting, …) |
| `book.put` | `{"kind","key","value","if_rev"}` | `{"rev"}` | needs a DM with `npc` or `admin` (`read` alone → `forbidden`: the cards and notes feed every AI prompt); stale `if_rev` → `conflict`; the author and new revision enter the core's action log (`Live: book.put <kind> <key> rev <n> by <principal>`) |
| `draft.decide` | `{"id","text"\|null}` | `{"result"}` | only drafts of the caller's own AIs, or admin |
| `ai.list` / `ai.create` / `ai.update` / `ai.revoke` | create/update: `{"name","avatar"?}` (+id for update/revoke) | list: `[{id,name,owner,avatar,llm}]` | HTTPS avatar or null; create returns `{ai,token}` once |
| `llm.allow` | `{"id"?,"on":bool}` | `{"ids":[...]}` | one/all owned AIs; admin may select another owner's AI; global switch belongs to the host |
| `events.since` | `{"n","limit","kinds"}` | `{"rows","through","lost"}` | paging older events |

An `ai.update` that changes an AI's groups ends that AI's open sessions at once (the AI reconnects with the new scope). Every request and push re-checks the whole group list against the AI's current groups, `read_tells` and `focus` included, so a narrowed AI loses tells and focus immediately.

## 8. Resume and restarts

- **Link drop**: the client reconnects with back-off 1, 2, 4 … 10 s and sends
  `resume`. Same `gw_epoch` and `boot`: the gateway sends only areas whose
  `rev` changed, results not yet acknowledged, events after `event_n`.
- **Game restart** (new `boot`): the gateway pushes `boot`; queued and
  pending commands of every session end as `final` with code `expired` (the
  command never ran) or `unknown` (it may have run).
- **Gateway restart** (new `gw_epoch`): `resumed:false`; the client resets the
  mirror; its queued commands end locally as `unknown` and are not resent
  (the gateway's id journal refuses old ids).
- **Offline queue**: commands queued while the link is down keep a `sent`
  flag; unsent ones expire locally after their `ttl_ms`.

## 9. Later (not v1)

Object-level area diffs keyed by object hash (`lookHash` already exists),
application-level deflate for large frames, protocol N/N−1 compatibility,
binary encoding after measurement.

## 10. Versioning and tests

Current finished-state schema is **mirror 2**: area paging coverage and NPC
`look_pending` are part of mirrored state. Host and client versions must match;
the WebSocket `proto` remains 1. See [dense-area reporting](live-ops-wire.md#dense-area-reporting-stays-live)
for the game-side paging contract and native acceptance command.

- `proto` changes with any incompatible frame change; `mirror` (the constant
  `MIRROR_VERSION` in mg-live) changes with any change to `AreaLive`,
  `LiveObject`, `CommandResult`, `Event` or what the mirror computes.
- Golden JSON fixtures for every frame, truncated/oversized/garbled frames
  (errors, never panics), and a differential test: the gateway store and a
  mirror fed by this protocol give the same `area()`, `find()`, `locate()`,
  `events()`, `result()` and `reconcile()` output after random report
  sequences, reconnects, gaps, game restarts and gateway restarts.

Implementation decisions and reproduction: [Live library](live-library.md#in-process-gateway-core).


## Resource generation discovery

`welcome.nwsync.generation` is an optional complete resource descriptor. Request
`files` with `a:{}` returns that descriptor in `res.r`, bound to the current game
boot. It is available only to authenticated DMs with `read`; AI sessions receive
`forbidden`. Unconfigured or not-yet-published resources return `unavailable`.
`GET /files/v1/generation` is the authenticated HTTP equivalent. The module-only
manifest/blobs are available under `/files/v1/manifests/<sha1>` and
`/files/v1/data/sha1/aa/bb/<sha1>` with the same authorization on every request.
The complete field definitions, identity calculation, download limits and atomic
activation rules are in [Live resource generations](live-resource-generations.md#descriptor-and-wire-extension).
This additive optional field does not change protocol/mirror versions or the game wire.

## Avatars

An avatar URL (`avatar`) is set by an AI's owner and is display text only: a client
MUST NOT resolve or connect to it. The gateway fetches it and serves the image.
Every actor in `presence` and every row of `ai.list`/`ai.create`/`ai.update` that has an
avatar also carries `avatar_path`, the gateway route that serves the image
(`"/avatars/ai:scribe"`; the principal id is one percent-encoded path segment). The
field is `null` (presence) or absent (`ai.*` rows) without an avatar, and clients
that do not know it ignore it.

`GET /avatars/<principal id>` with `Authorization: Bearer ...` is available to
authenticated DM and AI bearers, never to players (401, as for `/files/v1`; a request
with an `Origin` header is refused, 403). Responses:

| status | meaning |
|---|---|
| 200 | `Content-Type: image/png`, at most 128 by 128 pixels, `Cache-Control: private, max-age=300` |
| 404 | unknown principal, or no avatar |
| 429 | too many concurrent fetches (per principal or global) |
| 502 | the origin failed: unreachable, not public, not 200, too large, or not a PNG/JPEG |

The gateway applies the URL rules of `mg_live::avatar` (public HTTPS with a DNS name,
no credentials), resolves with every non-public address dropped at connect time, follows
no redirects, uses short timeouts, accepts at most 512 KiB of body, decodes PNG or
JPEG with dimension (1024) and memory (8 MiB) limits and re-encodes a PNG of at most
128 by 128. Results are cached per (principal, URL) for five minutes in a bounded cache
(256 entries, 8 MiB). A client loads the image with its session bearer, from the
gateway origin it is connected to and the same certificate pin it uses for the
WebSocket; it MUST NOT send that bearer anywhere else.


## Audited gateway boundaries and identity wire

The shared command/skill action log is an allowlist of `actor`, `op`, `target`,
`area`, `time` and `outcome`. Other readers receive no result value, error text,
draft text or nested payload. Full events remain available to their issuer,
the owning DM with `approve_drafts`, and admins. Pushes, `events.since`, scoped
HTTP event reads and `call/events` use the same projection. Text matching happens
**after** projection, so searching private text cannot reveal that it exists.
Events without action metadata use null for unavailable fields. Command result
and draft producers should call `EventLog::command_action` with the accepted
command to preserve its operation and target without copying command text.
The game's result wire and owner-scoped result outbox are unchanged.

`PolicyResolver::command_limit` is copied into the store's scoped acceptance
path for HTTP, WebSocket commands and draft approvals. Host authentication
refreshes it; the approval path refreshes the proposing AI's limit. This does
not replace the shared game-operation budget or reset counters on reconnect.

File downloads reserve global and per-principal slots until the response body
finishes or is dropped. Defaults are 16 global, four per principal and 64 MiB
of outstanding payload bytes. A saturated budget returns HTTP 429. Bodies use
32 KiB asynchronous reads with transport backpressure and a fixed file length;
no complete 17 MiB blob is buffered per request. Hosts may configure
`Publisher::with_download_limits` with positive `DownloadLimits`.

AI capacity counts active credentials: at most 16 per owner and 256 globally.
Revoked records are retained for at most one day and at most 64 records; expired
records are removed. Deleted token digests are unknown and cannot authenticate.
Static `install_ai` import is a bootstrap operation: an existing persisted
registry or any owner API mutation seals it. This prevents a configured old token
from being reimported after its revoked record has been pruned; new credentials
then come from `ai.create`. Operators must remove obsolete static credentials
from host configuration rather than reset the registry to bypass revocation.

The identity request contract is:

- `ai.list` returns the array `[{id,name,owner,avatar,avatar_path?,llm}]` directly
  (`avatar_path`: see [Avatars](#avatars)).
- `ai.create` and `ai.update` accept optional `avatar`: an HTTPS URL or null.
  Omitting it on update preserves the previous value; null clears it.
  `ai.create` returns `{ai,token}` once; only the token digest is persisted.
- `llm.allow {id?,on}` returns `{ids}` for the affected active identities.
  With no id it selects the caller's own AIs. An admin can select another
  owner's AI explicitly. A new AI starts off; the host global switch still
  overrides per-AI consent.
- Presence and welcome copy the managed AI avatar; presence also carries `avatar_path`.
  Updates invalidate presence.

Hosts register `Gateway::set_identity_observer(Arc<dyn IdentityObserver>)`
through `CoreHandle::with_gateway`. `IdentityObserver::disabled(principal, reason)`
receives `IdentityDisabled::Revoked` or `IdentityDisabled::ConsentWithdrawn` after
a successful registry save and after the store lock is released. Implementations
may call `LiveStore::revoke_owner` directly to cancel unpublished work; they must
not retain credentials or log private draft text. The callback runs on the
blocking request worker. The dev host supplies this cancellation observer.

Regression commands use Rust 1.98.0, `CARGO_INCREMENTAL=0`, a nonexistent
`NWN_ROOT` and at most two jobs:

```text
cargo +1.98.0 test -p mg-live -p mg-gateway -p mg-live-link -p mg-gateway-dev --lib --bins --tests -j 2
cargo +1.98.0 clippy -p mg-live -p mg-gateway -p mg-live-link -p mg-gateway-dev --all-targets -j 2 -- -D warnings
cargo +1.98.0 test --workspace --exclude mg-corpus-tests --lib --bins --tests -j 2
cargo +1.98.0 clippy --workspace --all-targets -j 2 -- -D warnings
cargo +1.98.0 fmt --check
```

On restricted Windows runners, the curl/Schannel integration may fail with
`SEC_E_NO_CREDENTIALS`. Record this as a failed gate, separately from a diagnostic
rerun using `-- --skip api::tests::real_tls_status_command_and_stop`; that rerun
is not a substitute for the full test on a runner with working credentials.
These changes require no game server. Native transport handoff verification is
owned separately and **needs WSL run**; it is not claimed by these in-process tests.
