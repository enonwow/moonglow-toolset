---
type: Reference
title: Live gateway protocol
description: Normative moonglow-gw/1 JSON envelopes, finished-state mirror and session delivery contract.
tags: [live, gateway, protocol]
generated: { by: codex, at: 2026-10-18T12:00:00Z }
sources: [../crates/mg-live/src/gateway_frames.rs, ../crates/mg-live/src/store_gateway.rs, ../crates/mg-gateway/src/lib.rs]
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
transport accepts optional `?proto=1&mirror=1` on the upgrade URL and returns
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
{"t":"hello","proto":1,"mirror":1,"client":"moonglow/1.33.0",
 "resume":{"gw_epoch":"e81f…","boot":"b-17…","event_n":4120,
           "areas":{"harbor":57,"market":12}},
 "watch":["harbor","market"]}
```

```json
{"t":"welcome","proto":1,"mirror":1,"gw_epoch":"e81f…","now_ms":1792230000000,
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
| `book.put` | `{"kind","key","value","if_rev"}` | `{"rev"}` | stale `if_rev` → `conflict` |
| `draft.decide` | `{"id","text"\|null}` | `{"result"}` | only drafts of the caller's own AIs, or admin |
| `ai.list` / `ai.create` / `ai.update` / `ai.revoke` | | | `ai.create` returns the `mgai_` token once |
| `llm.allow` | `{"on":bool}` | `{}` | the caller's own AIs; the global switch is the panel's |
| `events.since` | `{"n","limit","kinds"}` | `{"rows","through","lost"}` | paging older events |

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

- `proto` changes with any incompatible frame change; `mirror` (the constant
  `MIRROR_VERSION` in mg-live) changes with any change to `AreaLive`,
  `LiveObject`, `CommandResult`, `Event` or what the mirror computes.
- Golden JSON fixtures for every frame, truncated/oversized/garbled frames
  (errors, never panics), and a differential test: the gateway store and a
  mirror fed by this protocol give the same `area()`, `find()`, `locate()`,
  `events()`, `result()` and `reconcile()` output after random report
  sequences, reconnects, gaps, game restarts and gateway restarts.

Implementation decisions and reproduction: [Live library](live-library.md#in-process-gateway-core).
