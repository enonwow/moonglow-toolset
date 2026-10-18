---
type: Proposal
title: 'Live mode over Redis: implementation plan'
description: The implementation plan for live mode's second version - clients (a DM's tools, an LLM) talk to Moonglow over its HTTPS API, Moonglow talks to the running NWNX server over Redis pub/sub, live mode is its own page in Options, commands can run a module's script or a chunk of NWScript, and an MCP server tied to no Moonglow version, fork, machine or LLM discovers what to offer - with the channels, every file to change, the NWScript side, the WSL setup, tests and the morning acceptance run.
tags: [proposal, live, redis, nwnx, dm, llm, options, nwscript]
generated: { by: codex/astra1, at: 2026-10-18T00:00:00Z }
sources: [live-ops-wire.md]
---

# Live mode over Redis: implementation plan

Status: **built**; accepted by the owner on 2026-10-12, implemented on branch
`live-mode` on top of [live mode v1](live-mode-proposal.md) (commits
`ff44aaa`..`f40f013`). This document is self-contained for whoever builds
it (Claude, Astra or a person). Where it says "v1" it means that recorded
base revision, before the Redis phases. Sizes: S (an hour or two), M (half a day), L (a day).

## What the owner wants to see this morning

From Moonglow's API, with the game server running in WSL:

1. an NPC **walks** where it was told (and runs);
2. an NPC **says** a line;
3. a module **script runs** on an object (`run_script`);
4. a **chunk of NWScript sent from outside runs** in the game
   (`run_chunk`), and a chunk that does not compile comes back with the
   compiler's error;
5. Moonglow's area view (Live on) shows it happen;
6. all of it switched on from **Options › Live** in Moonglow, not only
   from environment variables.

The [acceptance run](#morning-acceptance-run) at the end is the script
for that.

## Where it stands

2026-10-16 transport update: events now use Redis Streams with a durable local
inbox, per-instance acknowledgement and exact overflow diagnostics. Reports and
commands retain pub/sub. The original implementation history below is preserved;
see [the current event queue](#durable-event-queue-2026-10-16).

A–E, G and F are implemented on `live-mode`; G is `6cf66de`. The guarded
workspace gate passes (1,008 passed / 82 ignored), with warning-free clippy
and clean fmt. **Morning passes through curl and an actual MCP SDK client**,
including arrival, scripts, chunks, compiler diagnostics, the revised Redis
outage/restart acceptance and both chunk-off gates. The owner-approved
temporary bartender resolves the earlier routine-NPC blocker without changing
the destination, timeout or checks. Native-window observation and a game-client
join remain manual. Phase H is excluded; no push was performed.

## Decisions

| Question | Decision | Why |
|---|---|---|
| Clients → Moonglow | **HTTPS API** (v1's listener: `/live/v1/status`, `/live/v1/area/<resref>`, `/live/v1/command`), bearer token, **loopback only** (`127.0.0.1:5199`) | The owner: "into Moonglow over the API". Clients (Claude, Astra, curl, a later MCP server) run on this machine. |
| Moonglow ↔ game | **Redis pub/sub**: the game publishes its state, Moonglow publishes commands | The owner: "from Moonglow over Redis". NWNX_Redis is built in WSL; push both ways, no HTTPS client in the game. |
| v1's game route | `POST /live/v1/sync` and the NWNX_HTTPClient scripts are **removed** | One transport to keep working. `LiveStore::sync` stays: Redis messages carry the same JSON. |
| Where it is switched on | **Options › Live**, a page of its own, off by default, saved with Moonglow's settings; **Apply** starts, restarts or stops it | The owner: "a separate option in Moonglow itself". Environment variables still override it (scripts and tests). |
| Where the networking lives | A new crate **`mg-live`** (protocol core, HTTPS API, Redis client, the service that runs them) | `mg-ui` must start and stop it from Options; today the listener is in `apps/moonglow` and starts once from env. |
| Redis client | **Our own minimal RESP client** on `std::net` (SUBSCRIBE, PUBLISH, PING) | A few hundred lines with tests; the `redis` crate brings many dependencies for three commands. |
| Redis security | Redis **bound to 127.0.0.1 in WSL, no password** | NWNX_Redis has no password setting (only `NWNX_REDIS_HOST`/`PORT`); loopback keeps it off the network. Windows reaches it through WSL's localhost forwarding. |
| Redis restart (owner decision 2026-10-12) | Moonglow reconnects; **restart the game server after Redis restarts** | Installed NWNX_Redis connects once at startup: `Reconfigure()` is called only by its constructor, using old cpp_redis without auto-reconnect. Plugin repair is a separate future task. |
| Agents and other LLMs | **`moonglow-mcp`**, a separate small server that **knows no tool in advance**: it reads what a running Moonglow offers (`GET /v1/describe`, protocol `moonglow-agent/1`) and what an `mg` CLI offers (`mg describe`) and makes one flat tool per operation; stdio and Streamable HTTP; plus OpenAPI for LLMs without MCP | The owner: not tied to any Moonglow version, fork, machine or LLM. Discovery instead of a built-in list; a documented protocol any fork can serve; nothing machine-specific; schemas every tool-calling model accepts. |
| Scripts | `run_script` (a compiled script of the module, `ExecuteScript`) **on by default**; `run_chunk` (any NWScript, `ExecuteScriptChunk`) **off by default**, enabled in Options **and** by the server's `MGLIVE_ALLOW_CHUNK=1` | The owner wants to send scripts; a chunk is arbitrary code on the server, so it needs two explicit switches and is logged on both sides. |

## Facts checked on this machine (2026-10-12)

* **NWNX_Redis** (`$NWNX_ROOT/Plugins/Redis/README.md`, `Config.cpp`):
  settings `NWNX_REDIS_HOST` (required), `NWNX_REDIS_PORT` (6379),
  `NWNX_REDIS_PUBSUB_SCRIPT` (default `on_pubsub`),
  `NWNX_REDIS_PUBSUB_CHANNELS` (comma-separated, wildcards allowed). **No
  password setting.**
* **A pub/sub message runs the script** on the main thread
  (`PubSub.cpp`, `Tasks::QueueOnMainThread` → `RunScript`), only while a
  module is running; the script reads it with
  `NWNX_Redis_GetPubSubMessageData()` → `struct { string channel; string message; }`
  (`nwnx_redis_ps.nss`). One script run per message: publish commands
  only when there are some.
* **Publishing from the game**: `int NWNX_Redis_PUBLISH(string channel, string message)`
  (`nwnx_redis.nss`) returns a result id; `NWNX_Redis_GetResultAsInt(id)`
  gives Redis's reply, for PUBLISH the **number of subscribers that got
  it** (verify on first run; the plugin calls `NWNXCall("NWNX_Redis","Deferred")`
  and commits with `sync_commit`, `Connection.cpp:110`, so the reply is
  there when the call returns).
  Phase E's real engine run confirmed `heard=1` on the first report
  (seq 1, 12,531 bytes, 3,980 µs). Results also arrived through the client
  API before the outage test.
* **Outage recovery requires a game-server restart with this NWNX_Redis build.**
  Phase E restarted the owned Redis after 10 seconds; Windows PING
  recovered, but the game remained disconnected throughout the following
  25-second acceptance wait. This contradicts the assumption that both
  sides reconnect. The owner revised acceptance on 2026-10-12: verify
  Moonglow reconnects, then restart the game and verify new-boot reports
  and expiration of old commands. See the implementation record for the audit.
* **Includes**: `$NWNX_ROOT/Binaries/NWScript.zip` has
  `nwnx_redis.nss`, `nwnx_redis_lib.nss`, `nwnx_redis_ps.nss`,
  `nwnx_redis_short.nss`; they use the game's native `NWNXCall` (no old
  `nwnx.nss` needed). `nwnx_redis.nss` includes `nwnx_redis_lib`.
* **Scripts in the game** (`nwscript.nss` of the installed game):
  `void ExecuteScript(string sScript, object oTarget = OBJECT_SELF);` and
  `string ExecuteScriptChunk(string sScriptChunk, object oObject = OBJECT_SELF, int bWrapIntoMain = TRUE);`
  — "runs immediately, same as ExecuteScript()", "jitted in place and
  currently not cached", returns `""` on success or the compilation error.
* **Redis is installed in WSL** (the owner ran
  `wsl -e sudo apt-get install -y redis-server` on 2026-10-12; Windows'
  own `sudo` is disabled, so the command must go through `wsl -e`). It
  runs as Ubuntu's service: `bind 127.0.0.1 -::1`, `protected-mode yes`.
* **Checked across the boundary (2026-10-12)**: from Windows,
  `Test-NetConnection 127.0.0.1 -Port 6379` succeeds, a raw RESP `PING`
  answers `+PONG`, and a Windows subscriber on `mglive.state` received a
  message published in WSL (`redis-cli publish` answered `1`, the number
  of subscribers that got it). So Moonglow on Windows reaches WSL's Redis
  at `127.0.0.1:6379` with no firewall change.
* Windows → WSL TCP on `127.0.0.1` needs WSL's localhost forwarding.
  Verify connectivity before starting; machine configuration belongs in
  the ignored local settings, not in this portable implementation.
* **Owner audit (2026-10-12): WSL shuts down when idle.** Its Redis
  service then becomes unreachable from Windows. `Start-LiveDemo` and
  `test-demo` must hold a foreground WSL process for the whole run,
  including GUI/server restarts and the Redis outage. Options › Live
  must report `Redis unreachable — is WSL running?` on connect failures.
* Everything in [v1's facts](live-mode-proposal.md#facts-checked-on-this-machine)
  about WSL, the server binary, the scratch user directory and
  `-publicserver 0` still holds.

## What exists (v1) and what changes

| Piece (v1, branch `live-mode`) | Keep / change |
|---|---|
| `crates/mg-ui/src/live.rs` — protocol types, `LiveStore` (`sync`, `enqueue`, `expire`, `status`, `area_json`, `pending_targets`), `reconcile` | **Move** to `crates/mg-live/src/store.rs`; add the two script ops and the store's command policy; keep every test. |
| `apps/moonglow/src/live_http.rs` — TLS listener, client routes, `/live/v1/sync` | **Move** to `crates/mg-live/src/api.rs`; **delete the `/live/v1/sync` route** and its tests; add a stop switch. |
| `apps/moonglow/src/main.rs` — starts the listener from `MOONGLOW_LIVE_TOKEN` | **Remove** that block; `mg-ui` starts the service from settings (env overrides). |
| `crates/mg-ui/src/live_view.rs`, `area_view.rs` changes, `mg-area` `scene_without` | **Keep** (they read `LiveStore`); add Run Script… / Run NWScript… to the menu. |
| `tools/live/nwn/mglive_http.nss` (HTTPClient events) | **Replace** with `mglive_ps.nss` (Redis pub/sub). |
| `tools/live/nwn/mglive_tick.nss`, `mglive_load.nss`, `mglive_inc.nss`, `mglive_exec.nss` | **Change**: publish to Redis instead of HTTP; two new ops. |
| `tools/live/wsl/start-server.sh`, `extract-includes.py`, `Start-LiveDemo.ps1`, `test-demo.ps1`, `probe.ps1` | **Change** for Redis; add the Redis start/check. |
| `docs/manual/04-areas.md`, `09-build-and-test.md` | **Change**: Options › Live, Redis setup, script commands. |

## Architecture

```
 Claude / Astra / curl            Moonglow (Windows)                         WSL
 ─────────────────────            ──────────────────                         ───
 HTTPS 127.0.0.1:5199  ──────►  mg-live::api ─┐
  GET  /live/v1/status                         ├─► LiveStore ◄── area view (Live on: markers, actions)
  GET  /live/v1/area/<resref>                  │      ▲  │
  POST /live/v1/command                        │      │  │ commands (on each state, and at once on enqueue)
                                               │      │  ▼
                                 mg-live::bridge: SUBSCRIBE mglive.state     redis-server 127.0.0.1:6379
                                                  PUBLISH   mglive.cmd  ◄──►      ▲            │
                                                                                 │ PUBLISH    │ pub/sub → mglive_ps
                                                                     nwserver + NWNX_Redis (mglive_tick every 1 s)
```

Options › Live owns one `LiveService` (API listener + Redis bridge) and
one `LiveStore`. Turning it off stops both threads and closes their
sockets; the store is kept so the view shows the last report as stale.

## The channels (`mg-live.v1` over Redis)

Channel names are `<prefix>.state` and `<prefix>.cmd`; the prefix is
`mglive` unless set (Options › Live, `MGLIVE_PREFIX` on the server).
The v1 envelopes now include the delivery safeguards from
[the current wire contract](live-ops-wire.md#server-bound-commands-2026-10-18):

* **`<prefix>.state`** — the game, every second: v1's sync request body
  (`v`, `prefix`, `boot`, `seq`, `ack`, `results`, `areas`, every fifth one
  `full`). See [v1 protocol](live-mode-proposal.md#game--moonglow-post-livev1sync).
* **`<prefix>.cmd`** — Moonglow: v1's sync reply body
  `{"v":1,"script_version":2,"boot":"reported-boot","prefix":"server-prefix","commands":[…]}`, published
  * after each accepted state message, **only if commands or result receipts are pending** (each
    message runs a script in the game);
  * at once when a command is queued (from the API or the view), so an
    NPC moves within milliseconds, not at the next second.

Delivery uses bounded command deduplication and explicit result receipts; interrupted
execution is ambiguous and does not promise rollback:

* Moonglow publishes every unacknowledged command again after each
  state message until the game acknowledges it, and expires it after
  30 s (v1 rules).
* The game runs a command id once (its last 256 ids, `MGLIVE_DONE`) and
  acknowledges repeats without running them.
* The game retains results until Moonglow echoes their versioned receipts,
  subject to the [retention bounds](live-ops-wire.md#explicit-result-receipts-2026-10-18).
  PUBLISH subscriber counts are diagnostic only. ACK without a result becomes
  `ok:false, code:"unknown"`; a later actual result can resolve it.
* Each command's start/done boundary is persisted independently. An interrupted
  command becomes `ambiguous` and is not automatically rerun on retransmission.
* Commands are bound to the report's boot and configured prefix. Reconnects wait
  for a fresh report; changing endpoints invalidates the prior validated queue.
* A new `boot` (server restart) drops the areas and expires the queue
  (v1).

## New commands

Added to `Op` (Moonglow) and `MgLiveRun` (game), with the existing
validation pattern:

| `op` | Fields | The game does | Result |
|---|---|---|---|
| `run_script` | `script` (resref, 1-16 `[A-Za-z0-9_]`), `target` (object id, optional) | `ExecuteScript(script, target)`; without a target, on the command's **area** | `ok`; `err` "unknown script" when `ResManGetAliasFor(script, RESTYPE_NCS) == ""` |
| `run_chunk` | `code` (1-8000 characters), `target` (optional), `wrap` (bool, default true) | `ExecuteScriptChunk(code, target, wrap)`; without a target, on the area | `ok` when it returns `""`; else `ok:false`, `err` = the compiler's message |

Rules:

* `target`, when given, must be in the command's area (v1's check). The
  player/DM protection of v1 applies to `destroy`, `npc_move`,
  `npc_attack`, `npc_say`, `set_state` only: a script may be meant to act
  on a player (the DM's choice).
* **`run_chunk` needs two switches**: Moonglow refuses it unless
  Options › Live › *Allow NWScript chunks* is on (`enqueue` returns
  "NWScript chunks are off in Options › Live"); the game refuses it unless
  the server was started with `MGLIVE_ALLOW_CHUNK=1` (result
  "chunks are disabled on this server"). Off in both by default.
* Both script ops are **logged**: Moonglow's log (`Live: run_chunk c7 on
  demo_area: <first 120 characters>`), the server log
  (`WriteTimestampedLogEntry("MGLIVE: chunk c7 …")`).
* `run_script` can be switched off in Options (*Allow module scripts*,
  on by default).

## Implementation

### Implementation record

Work started on `live-mode` at `1602bbf`, session
`01a12c89-8b4b-7411-a18d-ee3a85981463`. Scope is A → B → C → D → E → G → F;
H is excluded. Each completed phase will record its checks and local commit.
Passing automated checks is separate from user acceptance. No push is authorized.

Prerequisites checked read-only: Windows and WSL Redis PING both returned PONG;
the configured server executable, Redis plugin and include archive exist; the
four required includes and native `ExecuteScriptChunk`, `ExecuteScript`,
`ResManGetAliasFor` and `RESTYPE_NCS` declarations are present. Plugin source
confirms its settings and synchronous publish-result commit. WSL has no `rg`,
so the source inspection used `grep`; no service was modified. The publish
subscriber count and recovery behavior still require the real engine test.
Machine paths and per-machine ports are kept in ignored `tools/live/local.ps1`.

Owned-file ledger (paths relative to the repository unless stated otherwise):

| Path | Purpose | Retention |
|---|---|---|
| `tools/live/local.ps1` | Per-machine settings, excluded from Git | Required local input; no secrets |
| `crates/mg-live/` | Moved v1 store/API and new Redis/service implementation | Required authored source |
| `target/test-output/live-redis/` | One reusable set of phase check logs and visual proof | Keep final evidence; remove superseded intermediates after recording results |
| `target/test-output/live-demo/` | Reuse the v1 demo working set and dependencies | Preserve current result/inputs; no full copies per iteration |
| `target/debug/` | Reusable Cargo output during all phases | Keep until final source-built executables are validated; then remove only proven disposable outputs |
| `tools/live/wsl/{owned,keep-wsl,stop-keeper,start-redis,stop-redis}.sh` | Phase E process ownership and WSL lifetime helpers | Required authored source; same session owner as above |
| `target/test-output/live-redis/visual/options-redis-{before,after}.png` | Connection-failure status before/after | Required visual proof |
| `~/mglive-home/{keeper,redis-6380}.{pid,start,exe,log}` | Owned runtime identity/logs, only files actually created | Keep identity while running; stop only matching PID/start/executable |
| `target/test-output/live-demo/previous-v1.mod` | Temporary v1 recovery point, 1,000,935 logical bytes | Removed after revised E engine acceptance and all gates passed; current verified Redis module retained |
| `target/test-output/live-redis/phaseE-reconnect-diagnosis.txt` | Read-only plugin/pool extract, publish proof and stopped-process identities | Keep while reconnect diagnosis is unresolved |

The retained v1 binaries, module and six proof pictures predate this task and
remain protected until their role is explicitly superseded. The active session
history is never edited or deleted; closure will report its size and the
canonical knowledge location for separately authorized cleanup.

Phase A implementation notes: the spec takes precedence over v1's HTTP sync
route, application-owned listener and silent removal of ack-only commands.
`mg-live` owns the store, API, RESP client, bridge and service. The former GUI
startup block is removed with the moved API; Options/service ownership follows
in B. API binding uses the spec's loopback-only shortcut. Script policy changes
also cancel still-queued operations that are now disabled, and compiler errors
have a separate 16 KiB limit (v1's 256-character object-field limit was too
short). A bounded audit queue lets the UI receive externally queued script logs.
Redis kicks publish the current queue, rather than a stale captured payload.

During validation, the fake Redis helper initially used a store-specific mutex
type; it was made generic. Windows accepted sockets inherited non-blocking mode,
causing TLS resets: workers now explicitly use blocking sockets. A stalled TLS
client also showed that shutting down a duplicated Windows socket did not
interrupt its existing five-second receive. Bounded 500 ms worker I/O fixed the
two-second stop regression; the passing test includes an idle handshake client.
Phase A passed `tools/live/check.ps1 -Phase A`: workspace tests, all-target
clippy with `-D warnings`, and `cargo fmt --check`, using Rust 1.98.0 and two
build jobs. The focused store/API/RESP/bridge/service suite passed 26 tests;
two environment tests are ignored in that ordinary suite. The separate
`cargo +1.98.0 test -p mg-live real_redis -- --ignored` passed against the
existing Redis using unique test channels, without modifying its service.
Logs are `target/test-output/live-redis/phaseA-*.log`. No visual behavior was
changed in A. The unavailable OKF CLI is recorded as a tooling limitation;
the existing index already lists this document, and a fallback check passed
frontmatter and local file links for the changed documents. `git diff --check`
passed. Phase A is the local commit titled `Implement Redis live service core
(phase A)`; its hash is recorded by the following phase rather than embedded
self-referentially in that commit.

Phase A commit: `5bf8be8`; workspace totals: 997 passed, 79 ignored (including
corpus/engine/picture checks that require their explicit runner). The separate
real-Redis test passed as recorded above.

Phase B's baseline was rendered from phase A with
`cargo +1.98.0 test --workspace --test screens options_window -- --ignored`
and inspected before changing the Options UI. The retained baseline is
`target/test-output/live-redis/visual/options-before.png`. This task created
the four files in `target/test-output/screens-options/` (the directory did
not exist beforehand); they are temporary, removable after the retained
before/after proof is complete. `live_options` generated
`target/test-output/live-redis/visual/options-live-after.png`; both pictures
were inspected. The page fits its original window without clipping and masks
the token. After the passing phase checks, the four redundant baseline
pictures were removed (547,604 logical bytes; physical allocation and net
free-space changes were not measured). Only the retained before/after pair
is required for this change.

Code/spec discrepancy: the existing Options window had OK and Cancel, with
no Apply button. B adds Apply and routes both acceptance buttons through the
existing unsaved-work-aware Options action. Stopping retains the store, but
the view refuses to queue commands while the service is off. The settings and
Options actions are debug-logged, so `LiveConfig` now redacts its token from
Debug while retaining the specified plaintext settings serialization.

B passed `tools/live/check.ps1 -Phase B` (workspace tests, clippy `-D warnings`,
fmt check). Explicit checks also passed: `live_options_apply` drives Enable,
Apply, restart on the same port, and Off through the UI, confirms settings
round-trip/token redaction, keeps the old report and checks the real HTTPS
400 response for chunks disabled; `live_viewer` with the configured game
assets still queues actions without changing the GIT; `live_options` renders
the page under the GPU guard. Commands use `cargo +1.98.0 test --workspace
--test app <filter>` and `--test screens live_options -- --ignored`, with
two build jobs. Logs are `target/test-output/live-redis/phaseB-*.log`.
The local commit is titled `Add Options Live service controls (phase B)`.

Phase B commit: `14d2779`; full workspace totals: 998 passed, 80 ignored.
Phase C's `live_mode` GPU picture harness reuses the one
authored `target/test-output/live-mode/viewer/fixture.mod` working module.
Its v1 pictures are preserved; new pictures go only under
`target/test-output/live-redis/visual/`. The unchanged menu was captured and
inspected as `scripts-menu-before.png` before implementing script controls.
The same harness generates the after-menu and both script dialogs.
The script-command fixture explicitly enables chunks only in its ephemeral
test service; the production default remains off. UI command ids are tracked
at application scope so closing an area or switching Live off does not lose
its result log. If a long pause outlasts the store's bounded result history,
the log explicitly reports that the result is unavailable rather than
inventing success or retaining the id forever.

C passed `tools/live/check.ps1 -Phase C`, plus the GPU `live_viewer` integration
test with game assets. It covers script-on-player protection rules, area
targeting from empty ground, hidden disabled operations, the default chunk
wrapper, audit logging and exactly one log entry for both success and compiler
failure even after switching Live off. The GIT and undo history stay unchanged.
`cargo +1.98.0 test --workspace --test screens live_mode -- --ignored` produced
`scripts-menu-after.png`, `run-script-after.png` and `run-chunk-after.png` beside
the retained baseline; all three were inspected, with no clipped controls.
These four pictures are the minimal C proof; no intermediate render remains.
The two-job, Rust 1.98.0 logs are `target/test-output/live-redis/phaseC-*.log`.
The local commit is titled `Add live script controls and result logging (phase C)`.

Phase C commit: `d031a93`. In phase D, `mglive_ps` replaces
the HTTP callback, configuration moves into the shared include, publish
subscriber counts govern acknowledgment removal, and scripts/chunks use the
native functions with the server's independent chunk gate. The existing
256-id run-once cache is retained. The shared include declares the command
runner implemented by `mglive_exec`, so only the pub/sub entry point needs
its implementation.

The demo migration is limited to the existing manifest-owned scratch set.
It inventories the read-only base and refuses to remove any resource found
there. Only its v1-added `mglive_http`, `nwnx_httpclient`, `nwnx_events` and
`nwnx_object` source/compiled resources are retired, with exact relative paths
and logical byte counts accumulated in the existing `inputs.json`. Links or
paths outside the scratch root are refused. The previous verified
`DEMO-Live-01.mod` remains intact until a new compile/verify/pack succeeds;
Git retains the authored v1 scripts and the original include archive remains
read-only. No new full module, HAK or asset-tree backup is created.

D passed compilation of 57 scripts with zero failures, verification of the
migrated scratch module with zero errors/warnings, and
`tools/live/check.ps1 -Phase D` (workspace tests, clippy `-D warnings`, fmt).
The compiler accepts the shared forward declaration without requiring unused
runner code in the load/tick entry points. The wrapper checks both the JSON
failure count and the four required compiled entry points. Before migration,
both scratch copies of the HTTP callback matched the committed v1 source.
Nine retired files were removed, totaling 321,370 logical bytes, with paths
and lengths retained in `target/test-output/live-demo/inputs.json`; no physical
allocation or net free-space claim is made. The compile report is the existing
`live-demo/compile.json`; verification and gates are `live-redis/phaseD-*`.
There are no remaining HTTPClient/sync/inflight references in the authored
NWScript. Real game execution, subscriber counts and outage recovery remain
E's acceptance checks; compilation alone does not prove those. No visual
change occurs in D. The local commit is titled `Move game live scripts to
Redis pubsub (phase D)`.

Phase D commit: `eef8fec`. **Phase E blocked at its read-only preflight on
2026-10-12.** UDP 5121 is already bound by an unrelated `nwserver-linux`,
PID 3541, `/proc` start tick 2313436, with `-publicserver 0`. Its executable
differs from the configured `NWN_SERVER`, and its module differs from this
demo. The task-owned scratch marker instead records PID 2734 / start tick
1732323; that recorded process is no longer running. This is not sufficient
authorization to stop the process holding the port. Per the owner's explicit
stop-on-blocker instruction, no alternative port, process termination or
other workaround was attempted, and no Phase E implementation was started.

Attempted only: inventory Moonglow processes (none found), check UDP 5121
with WSL `ss`, PING the existing Redis (PONG), check the separate outage port
(no Redis responded), and compare sanitized `/proc` process identity against
the scratch ownership marker. Passwords were omitted. Exact local executable
and module details are recorded only in ignored `tools/live/local.ps1`.
The owner must free UDP 5121 or explicitly resolve ownership before resuming
E → G → F. A–D are committed locally; E has no commit because its checks and
acceptance have not run. The morning run, MCP client proof, portability test,
LC experiment removal and final documentation phase are **not completed**.
No Redis service, unrelated game process, LC file or original module was
modified by this preflight. No push occurred.

Resumed on 2026-10-12 at the owner's request. First committed the pending
blocker record as `b9a7903`; then rechecked `/proc` and UDP 5121: no game
server and no listener remain. Existing Redis responds; the outage-test
port is unused. Phase E now includes the owner's WSL idle-lifetime finding,
an owned foreground keepalive, and the Options connection-failure hint.
The new before picture reproduces an actual connection timeout; its after
test asserts the requested text. Phase E checks and engine acceptance are
still pending; no completion is implied by these edits.

**Phase E blocked again on 2026-10-12: the installed NWNX Redis plugin
does not reconnect after the required outage.** Reproduction:
`tools/live/demo/build.ps1`, then `tools/live/test-demo.ps1 -SkipBuild`.
The scripts load ignored `tools/live/local.ps1`. Compilation and module
verification passed with zero warnings; `previous-v1.mod` preserves the
previous engine-verified result. The WSL server remained private with random
passwords and its scratch user directory. No original module, real user
directory, LC file or NWNX source/binary was changed.

Verified through the client API before the failure: 22 routine NPCs moved;
creature/placeable/item spawn, ownership flags, speech, requested movement,
door/chest state, attack dispatch and temporary-object cleanup passed.
`run_script mglive_probe` returned success and the target became `Probe ran`;
`run_chunk` succeeded and another NPC became `Chunk ran`. Invalid NWScript
returned `ok:false` with the compiler diagnostic; a nonexistent script
returned `unknown script`. The failure JSON retains each completed result.

The test stopped only its own Redis on the outage-test port, waited 10
seconds, confirmed stale state, queued speech and started Redis again.
Windows raw RESP PING succeeded. The same-boot, new-sequence, fresh-state
condition then timed out after 25 seconds. The game repeatedly logged
`Plugin 'NWNX_Redis' failed event 'Deferred'. Error: tcp_client is disconnected`;
seq 60 logged `heard=0`. Redis's log confirms readiness before that wait.
`target/test-output/live-demo/e2e-redis.json` records `"passed":false` and
the exact timeout. The test did not restart the game to hide failed reconnect.

Read-only dependency audit: NWNX source checkout revision
`3f504d0f1463e49d405639bd3d454ae7416b1a98`; installed
`Binaries/NWNX_Redis.so` SHA-256
`01ebd1a6ad5d9d1e55f600711927e65811df65c5ce7eee9d3f286317ef98f8e9`.
`Plugins/Redis/Config.cpp` connects command clients and pub/sub without
disconnect callbacks. Bundled `cpp_redis/sources/redis_client.cpp:171` and
`redis_subscriber.cpp:282` only notify an optional handler; they do not
reconnect themselves. `Pool.hpp:85` takes connections without a health
check. This source evidence supports the runtime diagnosis; binary/source
correspondence has not been independently rebuilt or proved.

The owner's stop-on-wrong-fact rule applies. The next step requires a
reconnect-capable NWNX_Redis dependency, or authorization to repair its
command pool and pub/sub resubscription; alternatively the owner must
explicitly revise outage acceptance. Do not advance G/F, change system
Redis, restart the game as an outage workaround, or weaken the test.
E remains uncommitted because runtime acceptance failed. GUI reconnect,
restart expiry and both runtime chunk-off gates occur later and were not
reached. `probe.ps1` migration is unfinished. G, LC removal, F and the
curl/MCP morning run have not started. Watching the native live window and
joining a game client remain manual and unaccepted.

The Options hint has before/after GPU pictures and a passing label assertion
(`cargo +1.98.0 test -p mg-ui --test screens live_redis_unreachable -- --ignored
--nocapture`). The owned WSL keeper preserved Windows Redis PING after 15
seconds without another WSL command and remained present through the run.
Shell syntax and PowerShell parse checks passed. A launcher quoting bug was
fixed before the engine run: `wsl.exe` option flags stay unquoted; argument
values are quoted separately. Shutdown emitted a benign `/proc` race
diagnostic while polling the exited keeper; harden that on resumption.
Final inventory found no owned server/keeper, GUI, UDP 5121 listener or Redis
on the outage port. System Redis still answered PONG.

Handoff validation of the pending edits passed `tools/live/check.ps1 -Phase E`
with Rust 1.98.0 and two jobs: workspace tests (998 passed, 81 ignored),
focused `mg-live` tests (26 passed, 2 ignored), all-target clippy with
`-D warnings`, formatting and `fmt --check`. This does not override the
failed engine acceptance or authorize an E commit. `okf` remains unavailable;
frontmatter and local Markdown file links were checked directly instead.

Closure retains pending sources, the two new required pictures, phase logs,
failure JSON, source diagnosis, the current demo working set/dependencies
and one v1 recovery module for the unresolved diagnosis. No additional
disposable artifact was removed in this resumed turn: 0 logical bytes
reclaimed. Earlier cleanup remains 868,974 logical bytes; physical allocation
and free-space changes are unmeasured. Active history was stat'ed at
46,555,177 logical bytes and remains untouched and growing. The session ID
above and this canonical record are the handoff for separately authorized
history cleanup.

Owner decision on 2026-10-12 resolves the external-plugin blocker by
revising acceptance, not by patching NWNX. The earlier failed run above is
retained as history. The revised E test checks `status.redis.connected`
(the same state Options uses), preserves GUI-restart/game deduplication
before the outage, and after Redis reconnect checks API deduplication before
restarting the server. It then requires fresh reports with a new boot and
explicit restart-expiry results for both old commands. Stale area contents
are not treated as fresh proof. All other checks, including both chunk-off
gates, remain required. E → G → F resumes under this decision.

Revised Phase E engine acceptance passed on 2026-10-12. The client-only
probe verifies status/authentication, `/live/v1/sync` removal (404), missing
area (404) and chunk refusal (400). The full private run passed all earlier
actions/scripts plus a 10-second GUI outage with exactly one surviving
spawn, Redis disconnection and Moonglow reconnection, API duplicate
suppression, required server restart with a changed boot and fresh reports
(97 ms old), and restart-expiry results for both queued commands. The
server-off chunk gate returned `chunks are disabled on this server`; the
Moonglow-off gate returned HTTP 400. `e2e-redis.json` now records this
current passing run, replacing the superseded failure JSON; the failure's
cause, source revision and diagnostic extract remain documented above.
`probe.ps1` is migrated and the keeper shutdown race diagnostic is fixed.
All owned runtime processes stopped; system Redis remained untouched.
The added `status.redis` object and Options share one bridge state, so
connection status cannot be confused with the retained game's `connected`
flag or old report age. This is an additive API field; original client
routes remain compatible. No NWNX patch was made.

Phase E's final `tools/live/check.ps1 -Phase E` passed: 998 workspace tests,
81 ignored; 26 focused live tests, 2 ignored; warning-free all-target clippy;
formatting and `fmt --check`, Rust 1.98.0/two jobs. PowerShell parsing, WSL
shell syntax, local Markdown links and diff whitespace also passed. The
existing before/after hint pictures remain the visual evidence. `okf` is
unavailable, so the topic's index entry was updated directly. Phase E commit
title: `Complete Redis demo and revised outage acceptance (phase E)`.
After that acceptance, removed the task-created v1 recovery module; it has
no runtime consumer. The original read-only input, current engine-verified
Redis module, dependencies, source and minimal evidence remain. This recovers
1,000,935 logical bytes (cumulative 1,869,909); physical allocation and free
space were not measured. G → F and the morning run remain to do.

Phase E commit: `3d09efb`. Phase G started with a single operation table,
describe/call routes, generated OpenAPI, descriptor lifecycle and clap-derived
`mg describe`. The operation/schema/OpenAPI tests passed (28 live tests,
2 ignored); the CLI classification/schema unit test also passed. These are
partial results, not Phase G acceptance. The independent MCP crate now has
an executable entry point; discovery and transport checks are in progress.

**Safety incident, 2026-10-12; owner authorized guarded resumption.**
An ad hoc `cargo +1.98.0 test -p mg -p mg-live` invocation set two jobs but
omitted the `NWN_ROOT` guard from `tools/live/check.ps1`. Automatic game
detection therefore entered `apps/mg/tests/serve.rs` and launched a Windows
`nwserver.exe`, violating the owner's explicit WSL-only rule. This was an
agent invocation error, not a newly discovered requirement or an NWNX blocker.
The test uses `mg serve`'s private scratch defaults; its scratch directory
was `target/test-output/mg-serve`, created by this run. The recorded chain was
Cargo PID 46772 → serve-test PID 14552 → mg PID 39836 → nwserver PID 46484.
The game test began at 10:14:06 local time and failed after 95.22 seconds.

On discovery, attempted termination only after checking these PID/name/parent
and creation-time identities. They had already exited; a subsequent complete
inventory confirmed none remained. No unrelated process was stopped. The
retired password event was removed from the task-owned test log rather than
copied into documentation. G implementation stopped and the owner was asked
whether to resume with the guarded check script for further Rust tests.
The owner answered **"Resume with guarded checks"**. A fresh process inventory
found no game server and UDP 5121 free. All further Rust tests run through
`tools/live/check.ps1`, including focused package checks and explicit GPU
picture filters; only that picture branch restores the configured game assets.
The default phase gate still runs the full guarded workspace and clippy/fmt.
At the incident halt there was no G commit, MCP acceptance, LC removal, F or
morning run. Do not interpret the mixed ad hoc log as a passing phase gate.

Pending G source ledger (same session owner): `apps/mg/src/describe.rs`,
`apps/moonglow-mcp/`, `crates/mg-live/src/{operations,descriptor}.rs`,
`docs/live-api.openapi.json`, and their tracked integration edits. Retain these
for resumption. `target/test-output/live-redis/phaseG-{live,discovery}.log`
contains the partial checks/incident evidence. New MCP source/tests, protocol
documentation and package integration belong to this same working set; focused
test logs overwrite `phaseG-focused.log`. The native-test scratch above
is task-created; preserve its small diagnostic evidence until the incident is
recorded and remove only confirmed disposable data. Other corpus scratch
directories may have pre-existing provenance and must not be blanket-deleted.

G resumption evidence/cleanup ledger (same session):

* `tools/live/mcp-client.mjs`: reproducible client harness using the installed
  `@modelcontextprotocol/client` package (configured only in `local.ps1`).
  The initial real-client discovery check passed on stdio and HTTP with 51
  identical adapter/CLI tools; final agent-backed proof follows the GUI build.
* `target/test-output/live-redis/phaseG-lc-cleanup.json`: hashes for the two
  removed LC experiment files and `server.mjs` before/after. Only
  `src/live-game-tools.mjs`, `test/live-game.test.mjs`, their import and
  registration were removed. All 60 other source/test hashes are unchanged;
  `node --check` passes. The configured fork root is `MGLIVE_LC_MCP` in the
  ignored local file. No other LC content was changed.
* `%TEMP%/agent-protocol-test-50684-1` and
  `%TEMP%/agent-protocol-test-44680-0`: disposable fixture binaries left by
  this session's failed MCP pipe tests. The assertion checked job termination
  too soon on Windows; cleanup during unwinding then encountered its still
  exiting fixture executable and caused a second panic. The test now waits
  for bounded termination and closes the adapter's stdin to let it reap jobs;
  the adapter's independent worker enforces each job's deadline even during
  another request. The focused suite passes. These two exact leftovers and
  the incident's `target/test-output/mg-serve` are eligible for removal after
  verifying that no process uses them; preserve the canonical diagnosis here.

G implementation clarifications: `moonglow_jobs` is adapter bookkeeping,
alongside `moonglow_sessions`; it has no application-operation knowledge.
Application instructions come from `/v1/describe`, so the adapter contains
no built-in live operation names. An explicit descriptor may identify a
tunnel, so only automatic OS-folder discovery filters by local PID liveness.
Stateless HTTP clients refresh with `tools/list`; stdio emits list-change
notifications. Packaging now includes the adapter beside `mg` and standalone
OS archives. Cross-platform package installers are configured here but need
their respective release runners; no release or push was performed.

**Phase G verification before the build-directory blocker (2026-10-12): passed.** The guarded workspace gate has
1,008 passing tests, 81 ignored; the focused live suite has 30 passing,
2 ignored. Clippy with `-D warnings`, formatting and `fmt --check` pass.
Pipe tests cover three MCP versions and fallback, invented agent operations
and CLI commands, disabled/multiple/stale sessions, bad pins, path traversal
and Windows junctions, write permission, long-run gates, timeouts, owned job
stops, HTTP authorization, garbage and every truncated request. The
portability test passes across the adapter, `mg-live`, protocol document and
`tools/live` (only ignored local configuration is excluded). OpenAPI is current.

Built `mg`, `moonglow` and `moonglow-mcp` with Rust 1.98.0/two jobs. The real
`@modelcontextprotocol/client@2.3.1` client initializes both stdio and HTTP
against the running GUI's descriptor: **61 identical tools**, pinned
`live_status` calls show Redis connected, and `live_run_chunk` is absent under
the default-off policy. `mg_describe` also succeeds through both transports.
Evidence: `target/test-output/live-redis/mcp-discovery.json`, phase G logs and
the tracked client harness. This meets the owner's later requirement of at
least one MCP client; separate Claude Code/Codex UI registration is manual,
not claimed as tested. The GUI-only check started no game server and stopped
its owned GUI/WSL keeper. The demo stop helper removes its exact verified
stale descriptor after forced termination; normal service stop/exit removes
the descriptor through its Rust owner.

The documented native-test scratch and the two exact failed-test fixtures
were removed after process/path/link checks: **5,847,639 logical bytes**.
The two authorized LC experiment files account for another **5,439 bytes**;
the registration lines were also removed but are not included in that sum.
Cleanup cumulative through G: **7,722,987 logical bytes**. No physical disk
allocation/free-space delta is claimed. Current sources, Redis module,
dependencies, final logs/client evidence and required pictures remain.
No active or foreign conversation history was modified. Phase F and Morning
acceptance still follow; Phase H remains excluded.

**Resolved blocker, 2026-10-12 11:05 local time: `target/debug` disappeared
during the final guarded recheck.** The preceding
full gate, executable build and real GUI/SDK verification passed as recorded
above. After adding exact cleanup of the demo's stale descriptor to
`Stop-LiveDemo.ps1` (verified against the exited owned GUI), the final
`check.ps1 -Phase G` rerun entered clippy, unexpectedly rebuilt foundational
dependencies and failed writing `target/debug/deps/libm-72b4caa7f9e2befa.d`
and `syn-b37b4f4fdbe687b3.syn.86af5087cee0404f-cgu.02.rcgu.o` with Windows
error 3 / no such file or directory. At 11:06:45, read-only inspection
confirmed the entire `target/debug` directory was absent; `target` had a
last-write time of 11:05:28. The cause/actor is unconfirmed.

Stopped implementation under the owner's stop-on-blocker rule. Attempts
were limited to reading the failing log, filesystem state and process
inventory. No Cargo/rustc, Moonglow, native game server, WSL game server,
WSL keeper or UDP 5121 listener remained. A filtered inventory did not
identify an active cleanup command other than the inspection itself. No
foreign process was stopped. This session's exact cleanup targets and bytes
are recorded above and in `phaseG-cleanup.json`; they did **not** include
`target/debug`. Do not attribute its disappearance or any associated free
space to this task's cleanup.

Preserved: all pending G source/docs/package changes, final E engine proof,
G workspace/live logs, successful build/client proof and the final clippy
failure log in `target/test-output/live-redis/`. No rebuild into another
directory or partial G commit was attempted. Required next fact: confirm
that any concurrent build-output cleanup has finished and the workspace can
retain Cargo outputs. Then rerun the guarded G gate/build, repeat real-client
discovery if binaries change, commit G locally, continue F and Morning.
F documentation, Morning curl/game/MCP actions and Morning pictures are
still pending. Phase H remains out of scope; no push occurred.
The active session JSONL was only stat'ed at this blocker: 49,553,268 logical
bytes, still growing. Session `01a12c89-8b4b-7411-a18d-ee3a85981463` and this
canonical document identify the handoff for separately authorized history
cleanup; no active history deletion/truncation or cleanup mechanism was used.

The owner subsequently confirmed **"Cleanup finished; rebuild and resume"**.
Fresh preflight found `live-mode`, no Cargo/Rust/Moonglow/game process and no
UDP 5121 listener. Rebuilding in the original target directory and rerunning
the complete guarded G gate is authorized; no alternate output-directory
workaround is used. G still needs its final successful checks and local commit.

**G rebuild closure (2026-10-12): passed.** The complete guarded rerun again
reports 1,008 passed / 81 ignored, live 30 passed / 2 ignored, warning-free
clippy and clean fmt. All three executables rebuilt successfully in the
original target directory. Fresh real-GUI discovery with
`@modelcontextprotocol/client@2.3.1` again found 61 identical stdio/HTTP tools,
with chunks absent under the default policy. The owned GUI and WSL keeper
were stopped and its descriptor removed. The build-directory blocker is
resolved; no alternate directory, native game test or push was used.

The same session owns the new reproducible acceptance harness
`tools/live/morning.ps1` (required source) and its output directory
`target/test-output/live-demo/morning/` (current acceptance evidence, retain).
It reruns the isolated Redis test, uses curl and the actual MCP SDK client,
requires the requested walk destination to appear in reports, and preserves
JSON results without copying credentials. `mcp-client.mjs` now waits for that
destination too. Native window observation and a player join remain manual.

**Phase F working set (same session):** the existing manual chapters 04, 09,
10 and 11, manual/index, deferred list and both live proposals are the
canonical documentation, not parallel reports. `crates/mg-ui/tests/screens.rs`
adds `live_morning_snapshot`, guarded by `mg_testkit::gpu::hold()` and selected
only through `tools/live/check.ps1`. Its `morning/{before,after}.png` and curl/MCP
JSON are required current proof. G is committed as `6cf66de`.

Documentation corrections to the accepted design: `test-demo.ps1` explicitly
enables both chunk gates for its positive cases and tests each off gate, so
the nonexistent `-AllowChunks` test argument is removed; that switch belongs
to `Start-LiveDemo.ps1`. All Rust commands below use the native-game guard.
VS Code uses `servers` in `.vscode/mcp.json`, while Desktop/Cursor examples
use `mcpServers`. The owner's later acceptance requires at least one actual
MCP client; the SDK client exercises both transports. Separate client UI
registrations are documented, not claimed as executed.

The first F gate caught an unsupported manual link: the embedded manual only
contains its chapters, so `../agent-protocol.md` was not navigable. The manual
now gives repository-path references for external source documents and keeps
its setup/registration instructions self-contained. The test was preserved;
the complete guarded gate passed after this correction.

**Phase F verification, 2026-10-12:** the corrected full guarded gate passed
with 1,008 tests / 82 ignored, live 30 / 2 ignored, clippy `-D warnings`, fmt
and fmt-check. The three executables rebuilt. The embedded-manual tests pass;
Local file links and required frontmatter were checked (the final scoped pass
checked 58 file links; existing index formatting was preserved). The OKF executable
remains unavailable, so a formal `okf lint`/index regeneration is not claimed;
the canonical indexes were updated directly. Evidence is in
`target/test-output/live-redis/phaseF-*.log` and `phaseF-docs.json`.

**Historical Morning blocker, resolved below; run started 2026-10-12 09:52:21 UTC:** fresh preflight
found UDP 5121 free and no native or WSL game server. The complete revised
engine acceptance passed, including all operations, GUI reconnect/dedup,
10-second owned-Redis outage, Moonglow reconnect, required game restart with a
new boot, old-command expiry and both independent chunk-off gates.
`target/test-output/live-demo/morning/e2e-redis.json` records `passed:true`.

The curl harness then chose the NPC nearest (25, 25): **`bc`, tag
  `C7_FOLLOW2`, "Patrol follower 2", blueprint `city_actor`, at (31, 31, 0)**.
`morning-curl-0`, `npc_move` with `run:true` to (20, 20, 0), returned
`ok:true`. During the following 30-second polling window, no report placed
that NPC within one metre of the destination. The harness failed instead of
counting dispatch as arrival. At that stop, `curl-before.json`,
`curl-morning.json` and `target/test-output/live-redis/morning-run.log` held
the starting report, hashes, acknowledgement, timeout and milestones. These
reusable output paths now hold the final successful run; the failed assumption
and observed details are preserved here. The last polled area/status were not saved before cleanup,
so the final position and cause are not established by captured reports.

Read-only inspection of the preserved module source found `CityR7` in
`target/test-output/live-demo/module/city_modes.nss`: when a follower is more
than 4.5 metres from its leader, it clears actions and queues a move back to
the leader. `city_tick.nss` dispatches mode 7 to this routine. This is a
plausible competing AI command, **not proof of the exact timeout cause**.
The harness's nearest-NPC selection assumed a routine NPC would leave this
move uninterrupted. That assumption was not verified. No AI script, target,
coordinates, timeout or acceptance threshold was changed after the failure.

At that point work stopped at the owner's requested blocker boundary. The Morning say/script/
chunk/compile-error sequence through curl, the corresponding MCP sequence,
final API status JSON and before/after GPU pictures were **not reached**.
The SDK discovery checks from G still pass; they are not represented as a
passed Morning run. Watching the live native window and a player joining
remained manual. F was pending, and its ignored picture test had not been run.

The requested decision was whether arrival acceptance should use an owned temporary
`nw_bartender` (the demo's already tested spawn blueprint), preserving the
same destination, timing and all other checks, or must control an existing
routine NPC (which needs an explicit decision about its competing AI).
Before another run, failure diagnostics needed to save the last real area/status
responses so a timeout retains its final position. No workaround was applied.

Cleanup after that stop was verified: owned GUI
exited, its descriptor removed, no native/WSL game server, UDP 5121 free,
keeper stopped, owned Redis 6380 stopped, system Redis 6379 still answers
PONG when WSL runs, and the read-only source module's SHA-256 is unchanged.
Sources, working module/dependencies, binaries/check logs and partial
Morning evidence were retained for the unresolved diagnosis. No additional
file deletion was claimed at that stop: cumulative prior cleanup was
**7,722,987 logical bytes**, with no physical-allocation/free-space claim.
Session `01a12c89-8b4b-7411-a18d-ee3a85981463` has this document as its durable
handoff; its active history measured 51,465,902 logical bytes and is still
growing. History cleanup remains a separately authorized task; no history
was edited, truncated or deleted.

**Owner decision after the movement stop:** use an owned temporary bartender
for both transports, retaining the destination, timeout and all acceptance
checks. Both harnesses now spawn `nw_bartender` at (25, 25), verify its reported
owned/non-player identity and run the same move/say/script/chunk sequence on
that exact object. These are runtime objects discarded when the owned server
stops; no source-module AI or blueprint is modified. Failed runs now preserve
the latest real area/status responses. This resolves actor selection only;
arrival at (20, 20) still had to pass in the engine.

The first run with the owner-selected actor passed both curl and MCP arrival
checks and all script results. It exposed a harness cleanup error: curl's
owned actor `200` remained at (25,25) when MCP's actor `20c` jumped there.
The captured report showed both named "Chunk ran", with the second at
(25.6000004,25.8999996), about 1.08 metres away, outside the unchanged one-metre
threshold. Both chunks returned success and the deliberate bad chunk returned
the compiler diagnostic. Each transport now destroys only its own verified
actor after saving evidence and waits for its disappearance before the next
sequence. This fixes test isolation without changing the destination, timeout
or acceptance threshold; neither the module nor its AI is modified.

**Final Morning engine/API acceptance, 2026-10-12: passed.** Run
`tools/live/morning.ps1 -SkipBuild` after the guarded F checks and building the
three executables. Configure paths only in ignored `tools/live/local.ps1`,
including `MGLIVE_MCP_CLIENT` for the installed SDK. The harness first runs
the isolated 6380 outage acceptance, then the curl/MCP sequence with the
normal Redis service and a WSL keeper for its whole lifetime.

* `morning/curl-morning.json`: `passed:true`, owned target `20b`, actual arrival
  within one metre of (20,20) within the unchanged 30-second timeout. Speech,
  module script, chunk name/jump and the deliberate compiler error pass.
* `morning/mcp-morning.json`: `passed:true`, actual
  `@modelcontextprotocol/client@2.3.1`, 62 identical tools over stdio and
  Streamable HTTP. The same sequence passes on owned target `20d`; its walk
  reaches (20.429779,20.429779). G's default-policy discovery had 61 tools;
  Morning explicitly enables chunks, adding `live_run_chunk`.
* Both transports verify each temporary actor's owned/non-player identity,
  save before/after reports, then destroy only that actor and verify its
  disappearance. Source-module SHA-256 is unchanged; evidence records source,
  working-module and executable hashes.
* `morning/e2e-redis.json`: `passed:true`. GUI reconnect/deduplication,
  ten-second owned-Redis outage, Moonglow reconnect, required game restart
  with a new boot, old-command expiration and both independent runtime
  chunk-off gates pass. No NWNX patch was made.
* `morning/status.json` records both connections healthy, boot
  `f393c102337ccc2d33659e6d118d49ba` and 14 results, including both successful
  teardowns. The two intentional invalid chunks are expected negative tests.
* The guarded `live_morning_snapshot` test passes (1 test, GPU lock held).
  Both `morning/before.png` and `morning/after.png` were inspected: the actual
  area viewer shows the selected green actor labelled "Bartender", then
  "Chunk ran". These render recorded engine reports; they do not document
  someone watching movement in the native window. The walk itself is checked
  by the curl/MCP polling assertions.

These paths are under `target/test-output/live-demo/`; final gate logs are
under `target/test-output/live-redis/`. API reports prove the game results;
watching the live native Moonglow window, hearing/seeing speech in a joined
game client and separate Claude Code/Codex client UI registration remain
manual. Cross-platform installers were not run on this Windows host. Formal
OKF CLI lint remains unavailable, as recorded above. Automated acceptance
has no remaining failed check; it is not a claim of owner visual acceptance.

Final process audit in `morning/cleanup.json`: owned GUI exited and descriptor
removed, no native/WSL game server, UDP 5121 free, keeper and owned Redis 6380
stopped, both temporary actors removed and source hash unchanged. System
Redis 6379 answers PONG while WSL runs; its service was never stopped. Once
the keeper exits, idle WSL may shut down as documented in Facts and Risks.

The four superseded failure captures were removed: exact paths
`target/test-output/live-demo/morning/{status-failed,curl-failed-area,mcp-failed-area,status-mcp-failed}.json`,
created by this session's failed harness runs, totaling 64,666 logical bytes.
Their diagnoses and resolution are recorded above; no current test reads
them. Preserve the final reports, required pictures, current module/HAKs,
source files, local configuration, binaries and final gate logs for review
and reproduction. No active or foreign conversation history is touched.

Final cache cleanup removed `target/debug/incremental`, 28,514 files /
12,941,962,804 logical bytes, recreated by this session's guarded clean rebuild
after the owner confirmed the external deletion of `target/debug`. This is
identified task output, not a claim of ownership over the entire target tree.
No Cargo/Rust/linker process remained; its resolved path and parents were
checked and neither the cache nor its parents contained reparse points.
Binaries and dependencies needed to use the current result were retained.
The cache is never an input to the demo or evidence tests. This closure removed
**12,942,027,470 logical bytes**, including the four failure captures; cumulative
documented task cleanup is **12,949,750,457 logical bytes**. Physical allocation
and free-space deltas were not measured. The external deletion that interrupted
G is not included in these totals.

Retained paths in the ledger exist and the successful Morning run verifies
their current use; a fresh-machine installation and the manual observations
are not claimed. Session `01a12c89-8b4b-7411-a18d-ee3a85981463` uses this document
as its durable closure record. Its active JSONL measured 58,909,804 logical
bytes before closure and continues growing. It was only stat'ed, never edited,
truncated or deleted. No supported history-cleanup mechanism was used or
approved for this active conversation; separately authorized cleanup must
identify this session and preserve this record.

The earlier Phase E ownership-blocker closure retained the four committed phase sources, the then-current
scratch module/includes/HAKs, the previous verified `.mod`, the phase check
logs and six required visual proof images. Completed cleanup totals 868,974
logical bytes (four redundant baseline PNGs and nine retired v1 resources);
physical allocation and free-space deltas were not measured. Active session
`01a12c89-8b4b-7411-a18d-ee3a85981463` remains untouched. No separate history
deletion has been authorized; this record is the handoff for resumption and
for any later, separately authorized history cleanup. Its active JSONL measured
45,132,590 logical bytes at the blocker inspection and will continue growing
as this turn is recorded; it was only stat'ed, never rewritten or truncated.

At the initial inventory, Cargo output from work between the v1 closure and
this request already existed in `target/debug`, with mixed ownership. This
task never blanket-deleted that directory or claimed its bytes. The external
deletion and subsequent task-owned rebuild are recorded above; only that
rebuild's disposable incremental cache was removed at final closure. Timestamps
alone are not evidence of ownership.

Phases in order: A, B, C, D, E, G, F; H after the morning demo (G needs the listener and descriptor from A and B). Work on branch `live-mode`. After each phase: its tests, `cargo +1.98.0
clippy --workspace --all-targets -- -D warnings`, `cargo +1.98.0 fmt
--check`, and a local commit. `tools/live/check.ps1` runs the Cargo gates
with bounded jobs (`CARGO_BUILD_JOBS=2`; the full build exhausted RAM at
default concurrency). Use the `1.98.0` toolchain (PATH's stable is
older than the workspace minimum).

### Phase A — crate `mg-live` and the Redis client (L)

1. **New crate `crates/mg-live`** (`description = "Live mode: a running
   game's state and commands — the store, the HTTPS API for clients and
   the Redis link to the server"`), workspace member, dependencies:
   `mg-area` (for `ObjectKind`), `glam`, `serde`, `serde_json`, `rustls`,
   `rcgen` (move both from `apps/moonglow/Cargo.toml`), `log`. Add it to
   `[workspace.dependencies]` and to `docs/PLAN.md`'s crate table (§4)
   after `mg-area`, before `mg-ui`. `mg-ui` depends on it.
2. **`src/store.rs`** ← `crates/mg-ui/src/live.rs` (git mv to keep
   history). Add:
   * `Op::RunScript { script: String, #[serde(default)] target: Option<String> }`
     and `Op::RunChunk { code: String, #[serde(default)] target: Option<String>, #[serde(default = "yes")] wrap: bool }`
     with validation (resref; code 1-8000 chars, not blank; target id
     form) and `target()` returning the optional target.
   * `pub struct Policy { pub scripts: bool, pub chunks: bool }`
     (`Default`: scripts true, chunks false), `LiveStore::set_policy`,
     and in `enqueue` refusals "Module scripts are off in Options › Live"
     / "NWScript chunks are off in Options › Live".
   * The protection check skips the two script ops (see Rules).
   * `LiveStore::set_kick(Sender<()>)`; `enqueue` sends a kick after
     queueing (ignore a closed channel).
   * A result for an acknowledged command without a result:
     `ok:false, code:"unknown", err:"unknown: acknowledged; result not received"`
     (the original success fallback was superseded on 2026-10-18).
   `crates/mg-ui/src/lib.rs`: `pub mod live;` becomes
   `pub use mg_live::store as live;` so `live_view.rs` and the tests keep
   compiling (or update their `use` lines).
3. **`src/api.rs`** ← `apps/moonglow/src/live_http.rs`. Remove the
   `/live/v1/sync` route (`PATH`, line 19, and its arms at 183 and in
   `answer`) and its tests; keep status/area/command, the token check,
   limits, truncation tests and the ignored real-TLS test (it now hits
   `/live/v1/status`). Replace `start(ctx, live)` with
   `pub fn serve(bind: &str, token: String, store: Arc<LiveStore>, wake: Arc<dyn Fn() + Send + Sync>, stop: Arc<AtomicBool>) -> Result<JoinHandle<()>, String>`:
   the listener is set **non-blocking** and the accept loop sleeps 50 ms
   between polls, ending when `stop` is set (no extra dependency, no
   self-connect trick). The default bind is `127.0.0.1:5199`; refuse any
   address that is not loopback unless `MOONGLOW_LIVE_ALLOW_REMOTE=1`
   (shortcut: loopback only; widen with separate tokens when clients
   leave this machine).
4. **`src/resp.rs`** — the Redis protocol, no dependency:
   * `fn encode(args: &[&[u8]]) -> Vec<u8>` (`*N\r\n$len\r\narg\r\n…`);
   * `enum Reply { Simple(String), Error(String), Int(i64), Bulk(Option<Vec<u8>>), Array(Option<Vec<Reply>>) }`
     and `fn read_reply(r: &mut impl BufRead) -> Result<Reply, String>` —
     bounded (bulk ≤ 16 MiB, array ≤ 1 M items, nesting ≤ 8), errors with
     context, never panics;
   * tests: every reply type, nested arrays, null bulk/array, a
     subscribe confirmation and a `message` push, every truncation of a
     valid reply (each prefix is an error, not a panic), oversize bulk,
     bad lengths, garbage.
5. **`src/bridge.rs`** — the Redis link:
   * `pub struct RedisConfig { host: String, port: u16, prefix: String }`;
   * thread **subscriber**: connect (2 s timeout), `SUBSCRIBE <prefix>.state`,
     read with a 1 s read timeout (to check `stop`); for each
     `["message", channel, body]`: `store.sync(&body, now)`; if the reply
     has commands, hand it to the publisher; on any error: status
     "disconnected: <why>", wait 1 s, 2 s, 4 s … 10 s, reconnect;
   * thread **publisher**: its own connection; waits on a channel
     (`recv_timeout(1 s)`) fed by the subscriber (a reply to publish) and
     by `LiveStore`'s kick (publish the current queue as
     `{"v":1,"script_version":2,"boot":"reported-boot","prefix":"server-prefix","commands":[…]}` if not empty); `PUBLISH <prefix>.cmd <json>`,
     reads the integer reply; reconnects like the subscriber; a `PING`
     every 10 s of silence;
   * `pub fn status() -> Value`: `connected`, `lastStateMs`, `lastError`,
     `published`, `received`.
   * Test with an in-process fake Redis (a `TcpListener` thread that
     speaks just enough RESP: answer SUBSCRIBE, push one `message`,
     record a PUBLISH): a state pushed → `store.area()` filled and the
     queued command published on `<prefix>.cmd`; kick → immediate
     publish; server closes → reconnect.
   * An **ignored** test against the real Redis in WSL
     (`127.0.0.1:6379`): publish a state with `redis`'s own PUBLISH from
     the test (through the same client), see the store fill.
6. **`src/service.rs`** — `pub struct LiveConfig { enabled, api_bind, api_port, token, redis_host, redis_port, prefix, policy }`
   (serde, `Default`: off, `127.0.0.1`, 5199, empty token, `127.0.0.1`,
   6379, `mglive`, scripts on / chunks off) and
   `pub struct LiveService` with `start(config, store, wake) -> Result<Self, String>`,
   `stop(self)` (sets the flag, joins the threads within 2 s), and
   `status()` combining API and bridge. `LiveConfig::from_env(self)`
   applies `MOONGLOW_LIVE_TOKEN`, `MOONGLOW_LIVE_BIND` (host:port),
   `MOONGLOW_LIVE_REDIS` (host:port), `MOONGLOW_LIVE_PREFIX`,
   `MOONGLOW_LIVE_CHUNKS=1`; any of them present turns `enabled` on.
   Starting with an empty token generates one: 32 bytes from the OS RNG,
   hex. `getrandom` is already in `Cargo.lock` (0.2/0.3/0.4 through other
   crates); add `getrandom = "0.3"` to `mg-live` (no new crate in the
   lock). Not `fastrand`: it is not a cryptographic RNG.

### Phase B — Options › Live (M)

1. **Settings** (`crates/mg-ui/src/settings.rs`, `pub struct Settings`,
   line 40, `#[serde(default)]`): `pub live: mg_live::LiveConfig`. The
   token is saved there in plain text, as Moonglow's other settings are;
   say so on the page.
2. **Options page** (`crates/mg-ui/src/options.rs`): `OptionsPage::Live`
   (enum at line 17) listed after `Sounds` (list at line 303), with the
   draft copy of `LiveConfig` in `OptionsDraft` (line 32) like the other
   options. The page:
   * **Enable live mode** — checkbox.
   * **API for clients**: address (fixed `127.0.0.1` text unless remote
     is allowed by env), port, token (masked; **Generate**, **Copy**),
     and the ready `curl -k` line for `/live/v1/status` (Copy).
   * **Game link (Redis)**: host, port, channel prefix; a note: "the
     server publishes to `<prefix>.state` and listens on `<prefix>.cmd`
     (`NWNX_REDIS_PUBSUB_CHANNELS`)".
   * **Commands**: *Allow module scripts (run_script)* (on);
     *Allow NWScript chunks (run_chunk)* (off) with a red warning "runs
     any code on the server; the server must also be started with
     MGLIVE_ALLOW_CHUNK=1".
   * **Status** (live): API listening on …/error; Redis connected /
     disconnected: why; last state N s ago; commands published/received.
   * **Apply** (the window's existing OK/Apply flow): stop the running
     service, start a new one with the new config, `set_policy`.
3. **Ownership**: `Moonglow` gets `pub live_service: Option<mg_live::LiveService>`
   next to `pub live` (`crates/mg-ui/src/lib.rs`); a method
   `Moonglow::apply_live(&mut self, ctx: &egui::Context)` that stops/starts
   it and logs "Live: API on https://127.0.0.1:5199, Redis 127.0.0.1:6379
   (mglive)" or the error. Call it once at start (after settings are
   loaded, with `from_env` applied) and on Apply.
4. **`apps/moonglow/src/main.rs`**: delete the v1 block (the
   `live_http::start` match) and `mod live_http;`; call
   `moonglow.apply_live(&cc.egui_ctx)`. Remove `rustls`/`rcgen` from
   `apps/moonglow/Cargo.toml`.
5. **Area view**: the **Live** toggle's enabled state becomes
   `app.live_service.is_some()` and its disabled hover text "Turn on live
   mode in Options › Live".
6. **Tests** (`crates/mg-ui/tests/app.rs`): Options › Live with Enable
   and Apply starts a service bound to an ephemeral loopback port (test
   config) and Off stops it (the port is free again); the
   chunks-off policy makes `/live/v1/command` with `run_chunk` answer 400.
   A screens test (`tests/screens.rs`, `shoot`) of the Live page for the
   manual. GPU tests call `mg_testkit::gpu::hold()` first.

### Phase C — script commands in the view (S)

`crates/mg-ui/src/live_view.rs` `menu()`: for any selected live object
and on empty ground (target = the area): **Run Script…** (a resref field;
OK queues `run_script`) and, when chunks are allowed, **Run NWScript…**
(a multi-line `TextEdit::multiline` with the script editor's monospace
font; OK queues `run_chunk`). The result (`ok`/`err`) appears in
Moonglow's log when it arrives: poll `store.status` results each frame
for ids this view queued and log them once (`Live: c7 failed: <err>`).

### Phase D — the NWScript side (M)

In `tools/live/nwn/`:

* **`mglive_inc.nss`**: config from env (`NWNX_Util_GetEnvironmentVariable`):
  `MGLIVE_PREFIX` (default `mglive`), `MGLIVE_AREAS`, `MGLIVE_BOOT`,
  `MGLIVE_ALLOW_CHUNK`; drop host/port/token. Move v1's command handling
  out of `mglive_http.nss` into
  `void MgLiveHandleReply(string message)`: parse, check `v` and the array
  limit, for each command run-once (`MGLIVE_DONE`, 256), append ack and
  result (as today).
* **`mglive_tick.nss`**: build the body as today, then
  `int r = NWNX_Redis_PUBLISH(prefix + ".state", JsonDump(body));`
  `int heard = NWNX_Redis_GetResultAsInt(r);` — if `heard > 0` remove the
  acks/results that were in this body (v1's "sent" logic moves here);
  retire legacy HTTP request markers, but preserve the current per-command
  `MGLIVE_INFLIGHT` interruption marker.
  Log once a minute: `seq`, `full`, tick µs, bytes, `heard`.
* **`mglive_ps.nss`** (new; replaces `mglive_http.nss`, delete that):
  `struct NWNX_Redis_PubSubMessageData d = NWNX_Redis_GetPubSubMessageData();`
  ignore channels other than `prefix + ".cmd"`; `MgLiveHandleReply(d.message)`.
* **`mglive_load.nss`**: no `NWNX_Events` subscriptions; start when
  `MGLIVE_PREFIX` or `NWNX_REDIS_HOST` is set (keep "does nothing when
  live mode is not configured").
* **`mglive_exec.nss`**: `run_script` and `run_chunk` per
  [New commands](#new-commands); without a target `OBJECT_SELF` is the
  area; `run_chunk` checks `MGLIVE_ALLOW_CHUNK == "1"` first, logs the id
  and the first 200 characters, and returns the compiler's message as
  `err`.
* **`mglive_probe.nss`** (new, a test script imported into the demo):
  `void main() { SetName(OBJECT_SELF, "Probe ran"); SpeakString("probe"); }`.
  It proves `run_script` from outside: the next state report shows the
  target's `name` as "Probe ran" (`GetName` of a renamed creature).
  `ResManGetAliasFor(script, RESTYPE_NCS)` (both in the game's
  `nwscript.nss`, lines 12822 and 6464 of the installed copy) finds it.
* **Includes**: `tools/live/wsl/extract-includes.py` copies
  `nwnx_redis.nss`, `nwnx_redis_lib.nss`, `nwnx_redis_ps.nss`,
  `nwnx_util.nss` (drop `nwnx_httpclient.nss`, `nwnx_events.nss`;
  `nwnx_object.nss` only if still used — v1 uses native `StringToObject`).
* `tools/live/compile.ps1` and `demo/build.ps1`/`prepare.ps1`: the new
  file list; the demo's OnModuleLoad hook is unchanged (`mglive_load`).

### Phase E — WSL and scripts (M)

* **`tools/live/wsl/start-redis.sh [PORT]`** (new, default 6379): checks
  `redis-server` exists (else prints the owner's install command and
  exits 2); if `redis-cli -h 127.0.0.1 -p PORT ping` answers, does
  nothing (on 6379 that is Ubuntu's service); else starts its own on PORT:
  `redis-server --bind 127.0.0.1 --port PORT --protected-mode yes --save "" --appendonly no --daemonize yes --pidfile ~/mglive-home/redis-PORT.pid --logfile ~/mglive-home/redis-PORT.log`
  (no persistence: it is a message bus). `stop-redis.sh` stops only the
  pid it started.
* **`tools/live/wsl/start-server.sh`**: arguments `MODULE_PATH [AREAS]`;
  environment: `NWNX_REDIS_SKIP=n`, `NWNX_REDIS_HOST=127.0.0.1`,
  `NWNX_REDIS_PORT=${MGLIVE_REDIS_PORT:-6379}`, `NWNX_REDIS_PUBSUB_SCRIPT=mglive_ps`,
  `NWNX_REDIS_PUBSUB_CHANNELS=${MGLIVE_PREFIX:-mglive}.cmd`,
  `NWNX_UTIL_SKIP=n`, `MGLIVE_PREFIX`, `MGLIVE_AREAS`, `MGLIVE_BOOT`,
  `MGLIVE_ALLOW_CHUNK` (passed through from the caller, default 0); drop
  the HTTPClient/Events/Object plugins and the host/port/token arguments.
  Everything else stays (scratch `~/mglive-home`, `-publicserver 0`,
  random passwords, `env -i`, the symlink and port guards).
* **`tools/live/Start-LiveDemo.ps1`**: start Redis in WSL, check
  `Test-NetConnection 127.0.0.1 -Port 6379` from Windows, start Moonglow
  with the demo module and `MOONGLOW_LIVE_TOKEN` (generated),
  `MOONGLOW_LIVE_REDIS=127.0.0.1:6379`, `MOONGLOW_LIVE_CHUNKS=1` when
  `-AllowChunks`, wait for "Live: API on" in its log, then the server
  (`MGLIVE_ALLOW_CHUNK=1` with `-AllowChunks`). Print the token and the
  curl lines. No firewall prompt any more (all loopback).
* **`tools/live/test-demo.ps1`**: the v1 end-to-end checks over the new
  transport (spawn, destroy, move, say, attack, door and chest state,
  reconnect without a double run, restart expiry), plus, each proved
  through the **client API only** (no log parsing):
  * `run_script mglive_probe` on an NPC → result `ok`, and the area
    report shows that NPC's `name` = "Probe ran";
  * `run_chunk` `SetName(OBJECT_SELF, "Chunk ran");` on another NPC →
    `ok`, and its `name` = "Chunk ran" in the report;
  * `run_chunk` `this is not nwscript` → `ok:false`, `err` not empty;
  * `run_script` of a script that does not exist → `ok:false`
    "unknown script";
  * `run_chunk` with Moonglow's chunk switch off → HTTP 400 at
    `/live/v1/command`; with the server started without
    `MGLIVE_ALLOW_CHUNK` → result "chunks are disabled on this server"
    (restart the server once for this case);
  * a Redis outage. The test runs on **its own Redis instance on
    `127.0.0.1:6380`** (`start-redis.sh 6380`, no sudo; Moonglow gets
    `MOONGLOW_LIVE_REDIS=127.0.0.1:6380`, the server
    `NWNX_REDIS_PORT=6380`), so it can stop that instance
    (`redis-cli -p 6380 shutdown nosave`) for 10 s and start it again
    without touching Ubuntu's service on 6379 (stopping that needs sudo).
    Moonglow's own `status.redis.connected` must become false during the
    outage and true again after Redis starts. Replaying an accepted command
    must not queue or record it twice. The installed NWNX_Redis does not
    reconnect: restart the private game server, require fresh area reports
    with a new boot, and require queued old commands to expire with
    `expired: the server restarted`. GUI-restart deduplication is proved
    separately before the Redis outage while game reports are fresh.
  Writes `target/test-output/live-demo/e2e-redis.json` with
  `"passed": true` and every result.
* **`tools/live/probe.ps1`**: probes only the client API (no sync).
* **No machine in the tooling.** V1's scripts carry machine-specific
  paths as defaults for the demo base, server and NWNX.
  Replace each with a parameter or environment variable and no default
  that names a user or a project: `-Base`/`MGLIVE_DEMO_BASE` (required;
  any module with an area and creatures), `NWN_SERVER` (the server
  binary; default: found under `NWN_ROOT`), `NWNX_ROOT` (the folder with
  `Binaries/`; required for the demo), `-Distro` (WSL distribution;
  default: WSL's default). `~/mglive-home` stays relative to the running
  user's home. The portability test covers `tools/live/`. This machine's
  values go in an untracked `tools/live/local.ps1` (in `.gitignore`)
  that the scripts read when present.

### Phase G — the Moonglow MCP server: independent of version, fork, machine and LLM (L)

Four requirements from the owner, each with the design that meets it:

| Must not be tied to | So |
|---|---|
| **a Moonglow version** | The server knows **no tool in advance**. It asks the Moonglow it talks to what it can do (`GET /v1/describe`) and asks the `mg` it is given for its commands (`mg describe`), and makes one tool per operation it is told about. A newer Moonglow with more operations gives more tools; an older one fewer; no MCP release needed. |
| **a fork** | The contract is a small, versioned, documented protocol (`moonglow-agent/1`, below), not Moonglow's code. Any build that serves `/v1/describe` and `/v1/call/<name>` — this repo, another fork after it adds those two routes, a future fork — works with the same MCP server. The server's own code depends on no Moonglow crate. |
| **this machine** | No path, user name, drive, distro or port is written in the server or its docs as a requirement. Everything comes from discovery (the OS's data folder, `PATH`, the server's own folder) or from arguments/env, with portable defaults. The demo tooling (phase E) gets the same treatment. |
| **an LLM** | Standard MCP, tools only (no resources, prompts, sampling or elicitation that some clients lack); protocol versions `2024-11-05`, `2025-03-26` and `2025-06-18` negotiated; **flat, simple schemas** every tool-calling model accepts (one object per tool, no top-level `oneOf`/`anyOf`/`$ref`, every property described, `additionalProperties:false`); stdio **and** Streamable HTTP transports; the same operations also as plain HTTPS + **OpenAPI** for models and apps that have no MCP at all. |

#### G.1 The protocol `moonglow-agent/1` (served by Moonglow)

On the same HTTPS listener as the client API (loopback, bearer token):

* `GET /v1/describe` →
  ```json
  {"protocol":"moonglow-agent/1",
   "moonglow":{"name":"Moonglow","version":"1.21.1","build":"<git describe or empty>","variant":"upstream"},
   "operations":[
     {"name":"live_npc_move","group":"live","title":"Move an NPC",
      "description":"Order a creature (not a player or DM) to walk or run to a point in its area. The game runs it within about a second; read the result with live_wait.",
      "readOnly":false,"destructive":false,
      "input":{"type":"object","additionalProperties":false,"required":["area","target","x","y","z"],
               "properties":{"area":{"type":"string","description":"The area's resref, as live_status lists it"},
                             "target":{"type":"string","description":"The creature's game object id (hex), from live_area_state"},
                             "x":{"type":"number","description":"Metres, area space"}, "…":"…"}}}]}
  ```
  `variant` names a fork (`"my-server"`); clients must not depend on it.
* `POST /v1/call/<name>` with the input object → `200 {"ok":true,"result":…}`
  or `4xx {"ok":false,"error":"…"}`.
* Rules: names `^[a-z][a-z0-9_]{0,48}$`; input schemas use only `type`
  (`object`, `string`, `number`, `integer`, `boolean`, `array` of those),
  `properties`, `required`, `items`, `enum`, `minimum`/`maximum`,
  `minLength`/`maxLength`, `description`, `default`,
  `additionalProperties:false` — the subset OpenAI, Gemini, Anthropic and
  local models' tool calling all accept; a test enforces it on every
  operation. An operation that is switched off (chunks, scripts) **is not
  listed**, so no client offers it.
* **The live operations** (replacing the single `live_command` with one
  flat tool per op — the reason: the closed list as one tool needs
  `oneOf`, which several tool-calling APIs reject): `live_status`,
  `live_area_state`, `live_spawn`, `live_destroy`, `live_npc_move`,
  `live_npc_say`, `live_npc_attack`, `live_set_state`, `live_run_script`,
  `live_run_chunk` (listed only when allowed), `live_wait`. Each command op
  returns `{"queued":"<id>"}`; `live_wait {id, timeout_ms}` returns the
  result. They map onto `LiveStore` exactly as the `/live/v1/command`
  bodies do; the v1 client routes stay for existing users.
* The protocol is documented in `docs/agent-protocol.md` (new, OKF) with
  the rules above, versioning (additive changes keep `/1`; a breaking
  change is `/2`, served beside `/1` for a release), and how a fork
  implements it (two routes and the describe document).
* `docs/live-api.openapi.json` (OpenAPI 3.1) is **generated** from
  `/v1/describe` by a test (`cargo test -p mg-live openapi`, which fails
  when the file is stale) and served at `GET /v1/openapi.json`, for
  models and apps without MCP.

#### G.2 The CLI's self-description (`mg describe`)

New `mg` subcommand: `mg describe --json` prints
`{"protocol":"moonglow-cli/1","version":…,"commands":[{"name":"verify","path":["verify"],"description":…,"writes":false,"longRunning":false,"input":<schema in the same subset>,"argv":<how each input maps to arguments>}]}`,
generated from clap's tree (`Cli::command()`), with `writes` and
`longRunning` from a small table kept beside the commands (a test fails
when a new command is missing from it). The MCP server turns each into an
`mg_<name>` tool and runs it as `<mg> --json <path…> <args…>`.
An `mg` without `describe` (an older version or a fork) gives **no
`mg_*` tools**, and the server says so in its `instructions` — it does not
guess from help text.

#### G.3 The server: `moonglow-mcp`

A **separate small binary**, `apps/moonglow-mcp` (crate `moonglow-mcp`),
depending only on `serde`, `serde_json` and `rustls` (+ `ring`; as
`mg-live`) — **no Moonglow crate**, so it builds and ships on its own
(packaging: beside `mg` in every package, and as its own archive per OS
on releases).

* **Sources it discovers** (each optional; at least one must be found):
  1. *Running Moonglow instances*: descriptors in the OS data folder,
     resolved like Moonglow does (Windows `%APPDATA%\Moonglow`, macOS
     `~/Library/Application Support/Moonglow`, elsewhere
     `$XDG_DATA_HOME/moonglow` or `~/.local/share/moonglow`) under
     `live/sessions/<pid>.json`; stale ones (pid gone) are ignored.
     `--descriptor <file>` / `MOONGLOW_AGENT_DESCRIPTOR` points at one
     explicitly (another machine's tunnel, a fork that writes elsewhere).
  2. *An `mg` CLI*: `--mg <path>` / `MOONGLOW_MG`, else `mg`
     (`mg.exe`) beside the server's own executable, else on `PATH`.
* **Tools**: `moonglow_sessions` (always; lists the instances found with
  their version and variant), then every operation from each instance's
  `/v1/describe` and every command from `mg describe`. With one instance
  the tools are named as described; with several, each live/editor tool
  gets a `session` parameter (enum of the found pids) instead of
  multiplying tools. The tool list is built at `initialize` and refreshed
  on `tools/list` (sessions come and go); the server announces
  `listChanged:true` and sends `notifications/tools/list_changed` when it
  changes, for clients that support it (others re-list).
* **Calls**: live/editor → `POST /v1/call/<name>` with TLS pinned to the
  descriptor's `certSha256` and its bearer token; CLI → the process, with
  timeouts (default 120 s, `timeout_ms` up to 30 min), long runners only
  with `background:true` and stoppable only by this server.
* **Safety, all by configuration, none by machine**: `--roots`/
  `MOONGLOW_MCP_ROOTS` (path arguments must resolve inside them; default:
  the working directory); `--allow-write`/`MOONGLOW_MCP_WRITE=1` for CLI
  commands that write; operations a Moonglow switched off are not listed
  at all; tool annotations `readOnlyHint`/`destructiveHint` from the
  describe data.
* **Transports**: stdio (default); `--http 127.0.0.1:<port>` for
  Streamable HTTP (`POST /mcp`, JSON responses, optional SSE not needed
  for tools), with `--http-token` required (sent as bearer) and loopback
  only unless `--http-allow-remote` (then the operator is responsible for
  TLS in front of it).
* **Protocol details**: JSON-RPC 2.0; methods `initialize` (negotiates
  the three versions above, else answers with its latest),
  `notifications/initialized`, `ping`, `tools/list`, `tools/call`;
  results carry a text block with the JSON **and** `structuredContent`
  (clients without it read the text); `isError` for tool failures;
  `-32700`/`-32600`/`-32601`/`-32602` for protocol faults; logs to
  stderr only; never panics on input.
* **Instructions** (sent at `initialize`, read by every LLM): what the
  groups are, that ids come from `live_area_state`, that commands are
  asynchronous (`live_wait`), and what is switched off.

#### G.4 Moonglow's side

* `mg-live` serves `/v1/describe` and `/v1/call/<name>` from one table of
  operations (name, group, description, schema, handler), the same table
  the OpenAPI file and the tests use. Live ops are in it now; the editor
  ops of phase H are added to the same table later.
* The descriptor moves to `live/sessions/<pid>.json` (several Moonglow
  windows can run), with `{"protocol":"moonglow-agent/1","url":"https://127.0.0.1:<port>","token":…,"certSha256":…,"pid":…,"startedUnixMs":…,"version":…}`;
  written atomically on start, removed on stop and exit.

#### G.5 Tests

* `moonglow-mcp` (`apps/moonglow-mcp/tests/`), driving the binary over
  pipes like any client: version negotiation (all three versions and an
  unknown one); tools from a **fake Moonglow** (a test HTTPS server that
  serves a describe document with two made-up operations — proving the
  server hardcodes nothing) and from a **fake `mg`** (a test script whose
  `describe` lists one made-up command); an `mg` without `describe` gives
  no `mg_*` tools; several descriptors → the `session` parameter; a stale
  descriptor is ignored; a pin mismatch is refused; roots and the write
  guard; the HTTP transport with and without its token; garbage and every
  truncation of a request line give `-32700`, never a panic.
* `mg-live`: every operation's schema is in the portable subset (no
  `oneOf`/`anyOf`/`$ref`, every property described); switched-off ops are
  not listed; OpenAPI file is current.
* `mg` (`apps/mg/tests/`): `mg describe --json` lists every subcommand and
  its schema validates against the subset; the writes/long-running table
  covers every command.
* **Portability check**: a test that greps the MCP crate, `mg-live` and
  `docs/agent-protocol.md` for machine-specific strings (a drive letter
  followed by a user folder, `/home/`, and the user's own private names from
  `MOONGLOW_PRIVATE_WORDS`) and fails
  on any.
* **Client checks** (recorded in the implementation record): the owner's
  later instruction requires at least one actual MCP client for Morning.
  The official SDK client checks stdio and Streamable HTTP with identical
  tool lists. Separate Claude Code, Codex and Inspector UI registration
  checks remain manual, not prerequisites to this task's automated acceptance.

#### G.6 Registration (documented per client; nothing machine-specific)

The command is `moonglow-mcp` (on `PATH` after installing Moonglow, or by
its full path); options as needed (`--roots`, `--allow-write`):

* Claude Code: `claude mcp add --scope user moonglow -- moonglow-mcp --roots <your projects folder>`
* Codex (`~/.codex/config.toml`): `[mcp_servers.moonglow]` `command = "moonglow-mcp"`, `args = ["--roots", "<folder>"]`
* Claude Desktop / Cursor: `{"command":"moonglow-mcp","args":["--roots","<folder>"]}` under `mcpServers`.
* VS Code: the same stdio server entry, with `"type":"stdio"`, under `servers` in `.vscode/mcp.json`.
* HTTP-only clients: run `moonglow-mcp --http 127.0.0.1:7801 --http-token <token>` and point the client at `http://127.0.0.1:7801/mcp`.
* No MCP: the OpenAPI file and the HTTPS API with the token from the descriptor.

An existing private `moonglow` entry in Codex points at a fork's
Node server; it can stay or be replaced by `moonglow-mcp` once the fork
serves `moonglow-agent/1` (two routes on its bridge; noted for the fork,
not done here). Remove the earlier experiment in the fork's MCP
(`src/live-game-tools.mjs`, `test/live-game.test.mjs`, its
`installLiveGameTools` line).

### Phase H — the editor operations (L, after the morning demo)

The fork's `editor_*` tools (open/read/apply/undo/save on the **unsaved**
editor state, area and model views, screenshots, jobs) become **editor
operations in the same `/v1/describe` table** (group `editor`), so
`moonglow-mcp` offers them with no change of its own:

1. Port the fork's in-process bridge logic (its `bridge/moonglow-mcp.patch`
   against 1.14.1: `crates/mg-ui/src/mcp*.rs`, `gpu_views.rs`,
   `preview_visibility.rs`, `crates/mg-core/src/write_guard.rs`) to
   today's types as handlers of `/v1/call/editor_*`, switched on in
   Options › Live › *Allow agents to drive the editor* (off by default);
   each handler runs on the UI thread through a bounded queue (the fork's
   `pump` pattern), guarded by `expected_version`.
2. Operations: status, read, apply (dry run), undo, redo, save, save_as,
   open_area, open_resource, area_view, model_view, jobs, documents,
   transaction — with the fork's schemas rewritten in the portable subset.
3. Not in this phase: the fork's game-window tools (`editor_game_*`).

This is several days; the morning acceptance run does not need it.

### Phase F — documentation (S)

* `docs/manual/04-areas.md` (Live mode section): turned on in Options ›
  Live; Run Script… / Run NWScript….
* `docs/manual/09-build-and-test.md` (Live mode with NWNX): Redis in WSL
  (the owner's install command, `start-redis.sh`), the server's
  environment, the channels, the client API with curl for every op
  including `run_script`/`run_chunk`, security: loopback only, the token,
  chunks off by default and why.
* `docs/manual/10-options.md`: the Live page.
* `docs/agent-protocol.md` (new, OKF frontmatter): `moonglow-agent/1` —
  describe/call, the schema subset, versioning, the descriptor, how a
  fork serves it; `docs/live-api.openapi.json` (generated).
* `docs/manual/11-command-line.md`: `mg describe`, and a new section
  "Agents and MCP": `moonglow-mcp`, its options, registration for each
  client kind (stdio, HTTP, none), the safety switches.
* This document: Status → built, and "Where it stands"; v1's proposal:
  a line that the game transport is now Redis (see this document).
* `docs/log.md` entry for the day; `docs/deferred.md`: what is left (see
  [Not in this version](#not-in-this-version)); `docs/index.md` lists
  this document (done with it).

## Before you start (the owner)

Done on 2026-10-12 (kept for another machine). One step needs your
password in WSL, run from PowerShell through `wsl -e` (Windows' own
`sudo` is disabled):

```powershell
wsl -e sudo apt-get install -y redis-server
```

Ubuntu then runs it as a service on `127.0.0.1:6379` (protected mode).
`tools/live/wsl/start-redis.sh` uses that one when it answers `PING`, and
otherwise starts its own on the same address. Check from Windows:

```powershell
Test-NetConnection 127.0.0.1 -Port 6379
```

`TcpTestSucceeded : True` means Moonglow will reach it. If it is False
while `wsl -e redis-cli ping` says `PONG`, WSL's localhost forwarding is
off: set `localhostForwarding=true` under `[wsl2]` in
`%UserProfile%\.wslconfig` and run `wsl --shutdown` (your decision; it
restarts WSL).

## Tests and verification

| Level | What | Command |
|---|---|---|
| Unit | RESP encode/decode, truncation, limits | `tools/live/check.ps1 -Phase F -Package mg-live -Filter resp` |
| Unit | store: new ops, policy, kick, ack-without-result | `tools/live/check.ps1 -Phase F -Package mg-live -Filter store` |
| Unit | API routes, no `/sync`, loopback-only bind | `tools/live/check.ps1 -Phase F -Package mg-live -Filter api` |
| Integration | bridge against an in-process fake Redis | `tools/live/check.ps1 -Phase F -Package mg-live -Filter bridge` |
| Integration | real WSL Redis bridge, outage and required game restart | `tools/live/test-demo.ps1 -SkipBuild` |
| UI | Options › Live start/stop, policy, view menu | `tools/live/check.ps1 -Phase F -Package mg-ui -Filter live` |
| Picture | Live options; recorded demo/Morning views | `tools/live/check.ps1 -Phase F -Picture live_morning_snapshot` (or `live_options`, `live_demo_snapshot`) |
| Game | compile the scripts | `tools/live/compile.ps1` |
| End to end | WSL server + Redis + Moonglow, all ops | `tools/live/test-demo.ps1` |
| MCP | `moonglow-mcp` over pipes against a fake Moonglow and a fake `mg`: discovery, versions, sessions, pinning, roots, write guard, HTTP transport, garbage | `tools/live/check.ps1 -Phase F -Package moonglow-mcp` |
| Protocol | describe/call, portable schema subset, switched-off ops hidden, OpenAPI current | `tools/live/check.ps1 -Phase F -Package mg-live` |
| CLI | `mg describe --json` covers every command | `tools/live/check.ps1 -Phase F -Package mg -Filter describe` |
| Portability | no machine-specific strings in the MCP, `mg-live`, the protocol doc, `tools/live` | `tools/live/check.ps1 -Phase F -Package moonglow-mcp -Filter portable` |
| Gates | workspace, clippy `-D warnings`, fmt | `tools/live/check.ps1 -Phase <n>` |

CLAUDE.md rules that apply: parsers never panic (a truncation test for
RESP); something visual is shown as a picture before and after; GPU tests
hold `mg_testkit::gpu::hold()`; the server only through the scratch user
directory with `-publicserver 0`; never write to the real NWN user
folder.

## Morning acceptance run

Run after phases A–E and G pass, with the owner watching
Moonglow (and, if they like, the game client joined to the WSL server):

The repeatable automated entry point is `tools/live/morning.ps1 -SkipBuild`.
It first reruns the revised Redis outage acceptance on owned port 6380, then
performs the sequence below via curl and the actual MCP SDK client on the
normal Redis service. It requires the NPC to reach (20, 20), the probe name,
the chunk name/position and the compiler error. It saves status and
`e2e-redis.json` under `target/test-output/live-demo/morning/`; it stops its
owned processes and does not stop the system service. Configure the SDK path
only in ignored `tools/live/local.ps1` as `MGLIVE_MCP_CLIENT`.

```bash
# Git Bash on Windows (curl -k: the API's certificate is self-signed).
# 1. Redis and the server (chunks allowed for this demo), Moonglow with the demo module:
powershell -File tools/live/Start-LiveDemo.ps1 -AllowChunks   # prints the token
export TOKEN=<the printed token>
api=https://127.0.0.1:5199/live/v1
say() { curl -sk -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" "$@"; echo; }

# 2. Spawn this run's temporary actor; use its result.obj as <id>.
# Wait for its successful result and reported owned/non-player identity.
say -d '{"area":"demo_area","op":"spawn","kind":"creature","r":"nw_bartender","x":25,"y":25,"z":0,"f":0}' "$api/command"
say "$api/status"
say "$api/area/demo_area"

# 3. Walk (running), speak
say -d '{"area":"demo_area","op":"npc_move","target":"<id>","x":20,"y":20,"z":0,"run":true}' "$api/command"
say -d '{"area":"demo_area","op":"npc_say","target":"<id>","text":"Good morning!"}' "$api/command"

# 4. A module script, a chunk, and a chunk that does not compile
say -d '{"area":"demo_area","op":"run_script","script":"mglive_probe","target":"<id>"}' "$api/command"
say -d '{"area":"demo_area","op":"run_chunk","target":"<id>","code":"SetName(OBJECT_SELF, \"Chunk ran\"); AssignCommand(OBJECT_SELF, ActionJumpToLocation(Location(GetArea(OBJECT_SELF), Vector(25.0, 25.0, 0.0), 0.0))); SpeakString(\"Sent from outside\");"}' "$api/command"
say -d '{"area":"demo_area","op":"run_chunk","code":"this is not nwscript"}' "$api/command"

# 5. Results, newest first: ok for 3 and 4, the compiler's error for the last
say "$api/status"

# 6. After saving evidence, remove only the actor created by this run.
# Wait for success and disappearance before the next transport's sequence.
say -d '{"area":"demo_area","op":"destroy","target":"<id>"}' "$api/command"
```

The same through MCP (`moonglow-mcp`, in any MCP client): `moonglow_sessions`,
then `live_spawn` with the same bartender parameters; use its result object's
id, verified through `live_area_state {"area":"demo_area"}`. Then `live_npc_move`, `live_npc_say`,
`live_run_script` and `live_run_chunk` with the same fields as the bodies
above minus `op` (one tool per operation; pass `area` in every call), and
`live_wait {"id":…}` for each result. Finish with `live_destroy` on only this
owned actor and verify its disappearance. The owner's final requirement is at
least one actual MCP client; the SDK harness verifies stdio and Streamable
HTTP tool lists match. Separate Claude Code/Codex UI checks remain manual.

Expected: in Moonglow's area view (Live on), the NPC's marker runs to
(20, 20); "Good morning!" over the NPC in the client (if joined); the
marker's name becomes "Probe ran", then "Chunk ran", and it jumps to
(25, 25) — the names are in `say "$api/area/demo_area"` too, so
this is provable without the client; the last command is `ok:false`
with an NWScript compile error. The live window is watched by the
owner; an agent without desktop control proves it through the API and
the guarded `live_morning_snapshot` picture test (renders recorded reports). Save
screenshots of the view before/after to `target/test-output/live-demo/morning/`
and the status JSON beside them.

## Risks and open questions

* **Redis install needs the owner** (sudo). Without it nothing in this
  version runs; v1 (HTTPS) stays in git history as a fallback.
* **PUBLISH's subscriber count** is not proof of result delivery. Phase E
  verified a count of 1, but the current implementation requires explicit
  versioned receipts; see the [receipt regression](live-ops-wire.md#explicit-result-receipts-2026-10-18).
* **One script run per Redis message** in the game: Moonglow publishes
  commands or receipt-only envelopes only when needed; a burst of API commands publishes
  one message per enqueue — acceptable for a DM's pace; batch (publish at
  most every 50 ms) if logs show load.
* **`run_chunk` is arbitrary code** on the server: two switches, logging,
  loopback-only API, token. Do not enable it on a public server.
* **Chunks are compiled each time** (the engine does not cache them):
  fine for a DM's commands, not for a loop.
* Windows → WSL localhost forwarding must be on (default); the check is
  in [Before you start](#before-you-start-the-owner).
* **WSL can stop when idle**, even with its Redis service configured.
  Keep an owned foreground WSL process for the entire demo/test lifetime,
  including restarts and outages; release it only during full demo shutdown.
  On a connection failure Options shows `Redis unreachable — is WSL running?`
  and retains the detailed connection error on hover. Never change the
  system Redis service to implement an outage test.
* **Redis recovery depends on the plugin build.** Stock NWNX_Redis in Phase E
  did not reconnect. The pinned local patch below passes repeated pool recovery
  and pub/sub resubscription without restarting the game. Pub/sub is not durable;
  the game retains its bounded event/result buffer until a subscriber hears it.

## Not in this version

* Phase H editor operations on unsaved documents. The independent adapter
  can discover them when an application implements them, but this task
  adds only the live operation group and CLI discovery.

* Upstreaming the local NWNX_Redis reconnect patch requires owner consent;
  no upstream PR has been opened. The local proof is complete below.
* Hot-loading newly compiled scripts into the running server (a chunk
  covers ad-hoc code; new `.ncs` files need a module rebuild or NWNX
  resource injection).
* Remote clients (separate game/client tokens, non-loopback API).
* Redis with a password (NWNX_Redis has no setting for it).
* Everything v1 already deferred (poses for open doors, more spawn
  kinds, several servers).

## Patched Redis restart proof (audit H2, 2026-10-15)

`~/nwnx-redis-reconnect` is unified
`3f504d0f1463e49d405639bd3d454ae7416b1a98` plus the four-file
[local patch](../tools/nwnx-redis/reconnect.patch). Loaded Redis plugin SHA256:
`7bb283967d01a65952c03204b0a16b26366056148f34da0f2a9295075816d30a`.
Server SHA256: `d6d952826b671fc62c376dca912e87f1aa45d4aa81c620e912ceb4d8e051111d`.
No plugin rebuild/replacement, shared-install change or demo interaction was needed.

Reproduce: `wsl -e python3 tools/live/test-events.py --phase redis_restart`.
`tools/nwnx-redis/outage-test.sh` now delegates to this marker/PID-start/executable
identity-checked harness: only owned `~/mglive-events`, UDP5126 and Redis6392.
It replaces obsolete broad process cleanup and demo-copy logic. Each cycle
stops its Redis child for 10 seconds, verifies the game identity remains alive,
starts an empty Redis and reconnects the observer. It waits for a pattern
subscription before publishing exactly one set_name command, then verifies the
reported name. Authored native `mget_outage.nss` emits chat during the outage;
it must arrive from the bounded fallback buffer. All child identities are reaped.

| Cycle | State after Redis launch | Subscription after launch | Same game boot/PID | Command effect / buffered chat |
| ---: | ---: | ---: | --- | --- |
| 1 | 1017 ms | 2628 ms | yes | passed / passed |
| 2 | 951 ms | 3455 ms | yes | passed / passed |
| 3 | 991 ms | 3410 ms | yes | passed / passed |

Scheduling speech from ExecuteScriptChunk reproduced the documented empty
chat-hook arguments; that was a fixture issue, unrelated to Redis recovery.
No connected-player or client-render claim is made. Exact source/plugin hashes
and compact proof: `target/test-output/live-events/redis_restart.json`;
packet sequence: `redis_restart-state.json`. The patch backs off from 2 to
30 seconds; these observed short-outage times are not a universal guarantee.
Long outages can overflow the 256-event buffer (reported dropped counter).
Keep normal boot/gap/resync handling. Upstream submission remains unapproved.


## Durable event queue (2026-10-16)

Owner scope: eliminate the 648 omissions observed around the Redis restart in
[the session stress proof](live-game-performance.md#delivery-is-not-a-redis-subscriber-count).
A faster reconnect alone was rejected because another subscriber could still
clear the game buffer first. The new transport uses one bounded Redis stream,
separate from self-healing state pub/sub. The complete
[wire and recovery contract](live-events-wire.md#durable-event-delivery-2026-10-16)
is canonical; producer Lua, generated NSS and the log follower share that contract.

Implementation: `crates/mg-live/src/event_stream.rs`, its regression tests,
bridge/service/store hooks, `tools/live/event-stream.lua`,
`tools/live/nwn/mglive_evflush.nss` and `mglive_stream.nss`. The service initializes
its consumer identity/inbox when the normal data directory is supplied through
`publish_descriptor`; it does not consume/acknowledge durable entries before that
storage is ready. It preserves existing Redis services and their configuration. Newly owned Redis
instances started by `tools/live/wsl/start-redis.sh` use AOF with
`appendfsync always` in their marked scratch `redis-<port>-data/` directory. Options and `/live/v1/status` show event backlog, overflow count and
stream errors. Redis without AOF gets an explicit persistence warning.

Verified prerequisites on this workstation: Rust 1.98.0; Redis 7.0.15; NWNX
unified `3f504d0f1463e49d405639bd3d454ae7416b1a98` with the existing local reconnect
patch. Inspection of `Plugins/Redis/NWScript/nwnx_redis.nss` established the raw
Deferred argument API and the generated XADD wrapper's missing MAXLEN argument.
No NWNX rebuild or installation change was needed. All native testing uses the
existing marked WSL scratch and private UDP5126/Redis6392, never the owner's demo.

Focused protocol proof uses a second owned Redis on loopback6393:

```powershell
wsl -e python3 tools/live/test_stream_redis.py
# In another shell, while the helper reports ready:
$env:NWN_ROOT='Z:/moonglow-no-native-game'
$env:CARGO_BUILD_JOBS='1'
$env:MGLIVE_STREAM_TEST_REDIS='6393'
cargo +1.98.0 test -p mg-live --lib bridge::event_stream -j 1 -- --nocapture
New-Item -ItemType File -Force target/test-output/live-stream/redis/stop
Remove-Item Env:MGLIVE_STREAM_TEST_REDIS
```

The helper owns/reaps only its child PID/start identity, refuses an occupied port,
and puts AOF under `target/test-output/live-stream/redis`. Tests use random stream
names and remove only those keys. Native tests never run from Windows Cargo.

Protocol proof: 10,204 generated rows; a duplicate producer batch added zero
rows; three pending rows from a dead consumer were reclaimed; one trimmed pending
row was handled; the next retained ordinal yielded exactly176 overflow losses;
backlog and pending both ended at0. A same-game Redis database reset then accepted
game sequence10,205 in a new queue epoch and emitted a reset notice. Six additional
stream tests cover crash-after-apply before ACK, inbox reload/torn tail/locking,
duplicate application, exact overflow, independent state boots, malformed/truncated
input and compaction failure before the receipt advances. The final full core gate
passed171 unit tests (four existing ignored assays) and five guarded integration
entrypoints. Core all-target and UI library Clippy passed with `-D warnings`;
five Python harness tests, source syntax and generated NSS parity passed.
The logger also has a 600-event final-drain regression: its disk sink must not use
the API's newest500-row limit.

Native Redis-restart reproduction, using the existing stress harness and one
current workset:

```powershell
wsl -e env MGLIVE_STREAM_PROOF=1 MGLIVE_STRESS_SECONDS=180 MGLIVE_STRESS_RUN=streams MGLIVE_WINDOWS_PYTHON=/mnt/c/Python312/python.exe python3 tools/live/test-events.py --phase stress
python tools/live/stress_analyze.py target/test-output/live-stress/streams
```

The durable variant runs private Redis with `appendonly yes`, `appendfsync always`
and its own `~/mglive-events/redis-stream/` state. It preserves the same native
PID/boot through the halfway ten-second Redis outage. An independent XREAD observer
captures every event kind; the existing HTTPS/MCP agents use the real service.
`delivery-barrier.json` records the inclusive native game sequence after the agent
writers stop and waits until the local inbox has committed it. Analysis compares
**every sequence1..barrier**, including startup, to the store logs and reports
missing and duplicated rows separately. Later autonomous AI activity remains in
the queue and is outside that closed delivery interval. The fixture instruments
every `mglive_*.nss` entry, including the new flush, for TMI headroom.

Proof artifacts/ownership are recorded in `target/test-output/live-stream/ledger.json`;
canonical native receipts remain in `target/test-output/live-stress/streams/`.
The closed180.030-second restart run delivered **8,902/8,902 game events**,
**zero lost, zero duplicated, zero observer holes** through the explicit final
barrier. State recovered1,383ms after Redis launch; the game PID/boot stayed the
same. No game-buffer drops, native script errors or TMI were observed. The HTTPS/MCP
clients completed533 commands, with zero missing heard lines, duplicate lines or
API errors. All owned game/service/client/Redis processes exited. This tests private
NPC surrogates, not connected players, client rendering or arbitrary-duration outages.

The flush ran3,073 times, maximum23,864/1,048,576 VM instructions (97.7% headroom);
the largest measured entry was `mglive_rpt`,328,776 instructions (68.6% headroom).
Redis/Core diagnostics were not error-free:618 Redis and601 other plugin lines
were collected around this forced-outage run; compact examples remain in
`native.json`. No wall-time speedup claim is made. The native manifest pins the
service binary, module and instrumented source hashes. One subsequent core-only
change (`815f59d`) moved compaction before appending: its simulated I/O failure
regression and the final full core gate verify that branch separately.

`wsl -e env MGLIVE_STREAM_PROOF=1 python3 tools/live/test-events.py --phase log`
passed actual log following, subscriber absence, rotation, truncation and classifier
fixture checks. Receipts remain at `target/test-output/live-events/log.json` and
`log-log.json`; the fixture deliberately writes classifier text, not a real VM fault.
`wsl -e python3 tools/live/test_redis_persistence.py` separately proved the production
startup helper on private6394: AOF/always defaults, existing-process reuse, one
stream row and its pending receipt surviving restart, reclaim/ACK, then owned stop.
It starts no game. Existing external Redis instances are reused unchanged and
warned about if AOF is off; their persistence provisioning remains the operator's job.

Existing native guards were adapted to observe the durable event wire. `--phase
chat` passed300 burst rows plus all2,400 rows produced without state subscribers,
zero dropped. The exact H3 full-scale command in
[the performance record](live-game-performance.md#full-scale-instruction-guard-audit-h3-2026-10-15),
with `MGLIVE_STREAM_PROOF=1 MGLIVE_MEASURE_RUN=streams-guard`, passed223 objects,
three large appearance surrogates,256 factions and256 damage events. Flush maximum:
23,839/1,048,576 instructions; worst entry `mglive_rpt`:561,936 (46.4% headroom).
No TMI; every owned child reaped. `target/test-output/live-measure/runs/streams-guard/`
retains source hashes, inputs, engine/cleanup receipts and full-scale look packets.
`MGLIVE_LOOK_PROOF_RUN=streams-guard cargo +1.98.0 test -p mg-live --test look_reports -j 1`
passed the actual GFF reader under default/cp1250 (always set the documented
nonexistent `NWN_ROOT`). One guard-result variable collision was fixed and the
entire guard repeated before recording this pass.

Readiness also exposed test-harness assumptions: reused durable history needs the
current MCP cursor before writers start, Redis can accept connections while AOF
is still LOADING, and the proof reader must drain bounded pages of older boots.
Those runs were withdrawn, their current workset was reused, and the final proof
above includes a live observer throughout. An expired15-minute Redis test helper
was restarted before the final successful gate. Earlier control:8,731/8,731; its
compact receipt is retained only to distinguish it from the final result.

Native readiness caught one implementation error before timing: evaluating
`MgLiveTextCodepage()` after Redis argument pushes invokes NWNX Util and clears
the shared plugin argument stack. Redis therefore received `cp1250` as its
command. Resolve every NWNX-backed helper before the first push. The fixed
producer ran3,057 flush calls in the control, maximum23,864 of1,048,576 VM
instructions. A separate review caught why queue epoch cannot be just game boot:
losing Redis while the game stays alive resets ordinals; Redis TIME now makes
that reset explicit. The actual Redis regression covers the same-boot case.
A compaction failure is now tested before a new inbox page advances its receipt;
otherwise a retry could skip store application and acknowledge that page.
This engineering verification is separate from owner acceptance. No push or demo
start/stop/build is authorized or performed by this task.


Task retention: session `01a1441b-229d-7572-8275-c6a7c7824670`, history at
the agent's local Codex session file.
This canonical section, the wire contract and the small `live-stream/proof.json`
and `ledger.json` retain the solution and evidence lineage. Sources, exact final
stress MOD (one2,037,790-byte immutable input before reuse of the shared workset),
gzip replay, native timings, compact store receipt and version/gate summaries
remain required pending owner review. Shared fixture/HAKs, Cargo outputs, peer
artifacts and histories are untouched. No supported/approved conversation-deletion
mechanism is available in this task; its history is registered for separate later
retention review, not truncated or archived as a purported disk saving.

Cleanup: the existing owned log-rotation helper removed its848-byte test copy
after all children exited. Automatic approval review then rejected the verified
local disposable-output deletion with only **"blocked by policy"**. No part of
that batch executed: `live-stream/cleanup-plan.json` lists49 files under21 exact
roots totaling60,467,984 logical bytes still present (temporary service data,
reproducible samples, superseded logs and the private Redis test AOF). Required
proof is excluded from that plan. No alternate deletion mechanism was used.
Only848 logical bytes of explicitly measured cleanup are credited so far;
physical allocation and shared-drive free-space change were not measured.

Additional closed WSL Redis proof outputs total30,209,849 logical bytes
under the exact own paths in `live-stream/cleanup-wsl.json`; they also remain.
The same review rejection was not bypassed. No running Redis/game process relies
on them. Next action is owner review of the retained result and a separately
approved cleanup mechanism for these listed outputs/history, not another build.
