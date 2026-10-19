---
type: Proposal
title: 'Live mode: protocol and implementation'
description: Moonglow live mode v1 - the HTTPS protocol, area viewer and commands, NWScript reporting, private WSL demo, phase-by-phase verification, reproducibility, limitations and cleanup.
tags: [proposal, live, nwnx, dm, llm, area-viewer]
generated: { by: codex, at: 2026-10-11T22:18:51Z }
---

# Live mode: protocol and implementation

> **Superseded in part (2026-10-19).** The desktop no longer has a local Live mode: Moonglow reaches a game server only through a Live gateway. The local HTTPS API, its token and named clients, the session descriptors, `Allow LLM` consent, the Options › Live API/Redis/WSL/Logs controls and `moonglow-mcp`'s local discovery (`--descriptor`, `--sessions`) described here were removed; read it as the history of that design. Current behaviour: [Live mode](manual/19-live-mode.md), [desktop contract](live-desktop-link.md), [gateway protocol](live-gateway-protocol.md).

Status: **v1 implemented on `live-mode`**, with phases 1–5 verified and the
final documentation gate passed as recorded in [Where it stands](#where-it-stands).
The owner requested implementation; automated checks do not imply final user
acceptance. The branch was made from `develop` at `01a6543`. Every original line anchor below is
as of that commit; search for the quoted code if lines have moved. Sizes
are the project's: S (an hour or two), M (a day or so), L (several days).

This document is meant to be enough on its own: whoever implements it
needs nothing from the conversation it came from.

## Where it stands

**Superseded transport (2026-10-12):** the game ↔ Moonglow link now uses Redis pub/sub and live mode has its own saved Options page; see [Live mode over Redis](live-mode-redis.md) for the current implementation and acceptance. The game HTTP sync route and HTTPClient scripts are removed. The client API, store and viewer described here remain.

V1 includes the authenticated TLS listener/client API, live area models and
markers, all six command operations, retry/expiry handling, NWScript reports,
and a private WSL demo with reproducible build, launch, stop and test scripts.
The [area manual](manual/04-areas.md#live-mode) describes controls; the
[build/test manual](manual/09-build-and-test.md#live-mode-with-nwnx) covers
startup, configuration and client examples. Follow the cumulative
[implementation record](#implementation-record) for exact inputs and proof.

Local phase commits: 1 `ff44aaa`, 2 `25b2892`, 3 `e004d05`, 4 `d71cd12`.
Phase 5 completes this documentation; all five automated phase gates passed. Nothing was pushed,
and no commit was made to `develop`. Real WSL tests and rendered GPU evidence
passed. Desktop automation was unavailable; its absence is recorded, not
substituted by a claim of clicking the native window. The client join and blue
player marker remain a manual acceptance check. [Deferred work](deferred.md#live-mode)
includes animation poses, additional spawn kinds, multiple servers, remote
security and durable operation identity storage.

## Summary

A Dungeon Master, or an LLM working for one, should see a running game's
area as it is now and act in it:

* **See**: players and creatures where they stand (every second);
  placeables, doors and items lying on the ground, with HP, open/closed,
  locked, and which of the area's own were destroyed (every five seconds).
* **Act**: spawn a creature, placeable or item from the palette; destroy
  an object; order an NPC to walk somewhere, say a line or attack; open,
  close, lock or unlock a door or placeable.

How:

1. A module script on an **NWNX** server posts the state of the areas
   that have players (or are listed) to **Moonglow** once a second:
   `POST https://<host>:5199/live/v1/sync`, a bearer token, JSON.
2. Moonglow keeps the last report per area (`LiveStore`) and answers with
   the **commands** queued for the game. The game runs them and
   acknowledges them in its next post. One channel both ways: no Redis, no
   polling of another endpoint.
3. The **area viewer** gets a **Live** toggle: the module's own tiles and
   static objects as always, the game's creatures and changed objects over
   them, and clicks that become commands instead of GIT edits.
4. The same HTTPS listener has **read and command endpoints for an LLM**
   (or any client): status, an area's objects, queue a command.

The area itself (tiles, static objects) is never sent: Moonglow has it
from the module. Only what changes is.

## Decisions already taken

| Question | Decision | Why |
|---|---|---|
| Where the API lives | **In Moonglow** (the GUI process) | The owner's requirement. |
| Transport | **HTTPS** from NWNX_HTTPClient to Moonglow | NWNX_HTTPClient only speaks HTTPS (see [Facts](#facts-checked-on-this-machine)); Redis would put the API in a broker. |
| Certificate | **Self-signed, generated in memory at each start** (`rcgen`), served with `rustls` (`ring` provider) | NWNX does not verify server certificates; no files to manage. |
| Where the demo server runs | **WSL** (Ubuntu), with the NWNX already built there | NWNX is Linux-only; WSL already has it and a start script to copy. |
| Actions in v1 | Spawn (creature, placeable, item), destroy, NPC move/say/attack, set open/locked | The owner chose all four groups. |
| Commands | **A closed list**, validated in Moonglow and again in the game | No `ExecuteScriptChunk`, no arbitrary scripts: an LLM must not run code on the server. |
| LLM access | HTTPS endpoints on the same listener | This repo has no MCP bridge; any client (curl, an MCP server later) can use them. |
| Token | One `MOONGLOW_LIVE_TOKEN` for the game and for clients | shortcut: one token; split game/client tokens when the API leaves one machine. |

## Facts checked on this machine

* **NWNX_HTTPClient is HTTPS only.** `~/unified/Plugins/HTTPClient/HTTPClient.cpp:131`
  (in WSL) always builds `httplib::SSLClient(host, port)`.
* **It does not verify the server's certificate.** In the bundled
  `External/httplib.h:1479`, `server_certificate_verification_ = false`,
  and the plugin never turns it on. A self-signed certificate works.
* **Its events** (plugin README, `~/unified/Plugins/HTTPClient/README.md`):
  `NWNX_ON_HTTPCLIENT_SUCCESS` and `NWNX_ON_HTTPCLIENT_FAILED`; event data
  `REQUEST_ID`, `STATUS`, `RESPONSE` (the body; not `RESPONSE_BODY`). The
  only setting is `NWNX_HTTPCLIENT_REQUEST_TIMEOUT` (ms, default 2000).
  HTTP 429 fires `FAILED`. Requests run on an async thread; the call
  returns a request id at once.
* **Request struct** (`nwnx_httpclient.nss`): `nRequestMethod` (POST=1),
  `sTag`, `sHost`, `sPath`, `sData`, `nContentType` (JSON=2), `nAuthType`
  (BEARER_TOKEN=3), `sAuthUserOrToken`, `sAuthPassword`, `nPort`,
  `sHeaders`. `NWNX_HTTPClient_SendRequest(struct)` returns the id.
* **NWNX in WSL**: built plugins in `~/unified/Binaries/` (among
  them `NWNX_Core.so`, `NWNX_HTTPClient.so`, `NWNX_Events.so`,
  `NWNX_Util.so`, `NWNX_Object.so`); their NWScript includes in
  `~/unified/Binaries/NWScript.zip` and
  `~/unified/Plugins/*/NWScript/`. The server is
  `~/server/bin/linux-x86/nwserver-linux`.
* **A start script to copy**: a private server's WSL launcher script
  lines 184-210 - clears every `NWNX_*` variable, sets
  `NWNX_CORE_SKIP_ALL=y`, `NWNX_CORE_LOAD_PATH`, the per-plugin `*_SKIP=n`,
  `LD_PRELOAD=~/unified/Binaries/NWNX_Core.so`, then runs
  `nwserver-linux -userdirectory … -module … -port … -publicserver 0`.
  Do **not** use `~/test-env/mod-start.sh` (production MySQL).
* **WSL networking is NAT.** From WSL, Windows is the default gateway
  (`ip route show default`, e.g. `172.x.y.1`; it can change after a
  reboot). Moonglow must listen on that address (the `vEthernet (WSL…)`
  adapter's), not on `0.0.0.0` (that would face the LAN).
* **No script in the reference server uses HTTPClient yet**; the handler is
  written from scratch. LC's DM command dispatcher (`lib_dm_commands.nss`)
  is not reusable here: it takes its target from the DM's cursor and needs
  LC's MySQL.
* **Native JSON in NWScript** (`JsonObject`, `JsonObjectSet`, `JsonArray`,
  `JsonArrayInsert`, `JsonString/Int/Float/Bool`, `JsonDump`, `JsonParse`,
  `JsonObjectGet`, `JsonArrayGet`, `JsonGetLength`, `JsonGetString/Int/Float`)
  is used in LC already (`lib_dm_commands.nss`, `lib_bank.nss`). Do not
  use `ObjectToJson` for state: it dumps a whole GFF per object.
* **The renderer does not use a door's or placeable's `AnimationState`**
  to pose it (`AreaObject::state`, `crates/mg-area/src/lib.rs`, is read
  but the scene always plays the idle animation). Open/closed is shown as
  a label in v1.
* **`mg-serve`** (new on `develop`) runs the native `nwserver` privately;
  it has no NWNX, so the demo uses WSL. It may serve a non-NWNX smoke
  test later.

## Reference implementation already written (and tested)

A first version of the protocol core and the HTTPS endpoint was written
and tested in an earlier private Moonglow fork before work moved here. Copy
these two files as the starting point; they compile against this
workspace's types (`mg_area::ObjectKind`, `glam`, `serde_json`):

| File in the fork | Copy to | Tests that passed there |
|---|---|---|
| `<private fork>\crates\mg-ui\src\live.rs` | `crates/mg-ui/src/live.rs` | 8 (`cargo test -p mg-ui --lib live::`) |
| `<private fork>\apps\moonglow\src\live_http.rs` | `apps/moonglow/src/live_http.rs` | 3 (`cargo test -p moonglow live_http`) |

The fork's `mcp_network.rs` change (MCP `live_*` methods) does **not**
apply here (no MCP bridge in this repo); its role is taken by the client
endpoints below. `rustls 0.23` + `ring` + `rcgen 0.13` were confirmed to
build with the MSVC toolchain.

## Architecture

```
WSL: nwserver + NWNX (HTTPClient, Events, Util, Object)
  mglive_tick  ──POST /live/v1/sync (Bearer, JSON state + acks)──►  Moonglow: live_http thread (rustls)
  mglive_http  ◄────────── 200 {"commands":[…]} ────────────────            │
  mglive_exec  (runs the commands, records results)                         ▼
                                                              LiveStore (Arc<Mutex<…>>, mg-ui::live)
                          LLM / curl ──GET/POST /live/v1/…──►      ▲            ▲
                                                                   │            │
                                                     area viewer (Live toggle): overlay, ghosts, actions
```

* The listener never goes through the GUI frame: it writes to `LiveStore`
  and calls `ctx.request_repaint()`.
* The area viewer reads `LiveStore` each frame (a clone of one area's
  report) and pushes commands into it.

## The protocol: `mg-live.v1`

All bodies are UTF-8 JSON. Coordinates are metres in area space; facing
is degrees anticlockwise from east (`GetFacing`). Object ids are
`ObjectToString(o)` (hex, 1-16 digits). An area is named by its resref
(`GetResRef(oArea)`), which is the ARE's resref in the module.

### Game → Moonglow: `POST /live/v1/sync`

Every second. Every fifth post is **full** (adds placeables, doors and
items for that area).

```json
{"v":1,"boot":"8f3a21c0-17911","seq":812,
 "ack":["c41"],
 "results":[{"id":"c41","ok":true,"obj":"7f00012a"},{"id":"c40","ok":false,"err":"target is a player"}],
 "areas":[{"area":"demo_market","full":true,
   "cre":[{"id":"7f00012a","tag":"guard_01","r":"demo_guard","name":"Guard","pc":false,"dm":false,
           "x":31.2,"y":18.7,"z":0.0,"f":90.0,"hp":[22,30],"act":1,"own":false}],
   "plc":[{"id":"7f0000a1","r":"demo_barrel01","tag":"","x":12.0,"y":8.5,"z":0.0,"hp":[3,10],
           "open":false,"lock":false,"own":false}],
   "doors":[{"id":"7f0000b4","r":"demo_door_a","tag":"door_a","x":40.0,"y":20.0,"z":0.0,"open":true,"lock":false}],
   "items":[{"id":"7f0000c9","r":"nw_it_gold001","x":5.1,"y":6.2,"z":0.0}]}]}
```

| Field | Meaning |
|---|---|
| `v` | Always 1. Anything else is refused. |
| `boot` | Chosen at module load, unique per server start. A new value means a restart: Moonglow forgets every area and expires every queued command. |
| `seq` | Increases by one per post. A post with a lower `seq` than the last is answered but its areas are ignored. |
| `ack` | Ids of commands the game has run (or refused). They leave the queue. |
| `results` | What happened: `ok`, `err` (why not), `obj` (id of the object a spawn made). |
| `areas[].full` | `plc`, `doors` and `items` are present and complete. In a post that is not full they are left out and Moonglow keeps the last full lists. |
| `cre` | Creatures, players and DMs in the area. Always complete. |
| `own` | The object was made by a live command (local int `MGLIVE_OWN`). |
| `act` | `GetCurrentAction`. |

Limits (Moonglow refuses the whole post, 400): 64 areas, 4096 objects per
list, 256 characters per string field, body 4 MiB, head 16 KiB.

Reply `200`:

```json
{"v":1,"commands":[
 {"id":"c42","area":"demo_market","op":"spawn","kind":"creature","r":"demo_guard","x":10,"y":12,"z":0,"f":180},
 {"id":"c43","area":"demo_market","op":"npc_say","target":"7f00012a","text":"Halt!"}]}
```

Errors: `401` bad token, `404` other path, `405` other method, `411` no
`Content-Length`, `413` body too large, `431` head too large, `400`
anything else, with `{"error":"…"}`.

### Commands

| `op` | Fields | The game does |
|---|---|---|
| `spawn` | `kind` (`creature`/`placeable`/`item`), `r` (resref), `x`,`y`,`z`, `f` | `CreateObject` at that location; sets `MGLIVE_OWN`; result `obj`. |
| `destroy` | `target` | `DestroyObject`. |
| `npc_move` | `target`, `x`,`y`,`z`, `run` (default false) | `ClearAllActions` then `ActionMoveToLocation`. Creatures only. |
| `npc_say` | `target`, `text` (1-200 chars, not blank) | `SpeakString`. Creatures and placeables. |
| `npc_attack` | `target`, `victim` | `ClearAllActions` then `ActionAttack(victim)`. Both creatures in the area. |
| `set_state` | `target`, `open`?, `lock`? (at least one) | Door: `ActionOpenDoor`/`ActionCloseDoor`; placeable: `ANIMATION_PLACEABLE_OPEN`/`CLOSE`; `SetLocked`. |

Rules, in Moonglow **and** in the game:

* `target`, `victim` must be valid objects **in the command's area**.
* `target` must not be a player character, a DM or a DM-possessed
  creature (`GetIsPC`, `GetIsDM`, `GetIsDMPossessed`). A victim may be a
  player (the DM's choice).
* `r` is a resref: 1-16 of `[A-Za-z0-9_-]`. Coordinates are finite.

Delivery uses **retry with bounded duplicate suppression**: Moonglow sends a
command in every reply until it is acknowledged; the game keeps the last 256 ids it ran
(a module `LocalJson` array) and acknowledges a repeat without running it
again. A command not acknowledged within **30 s** expires (result
`expired: not acknowledged in time`); a new `boot` expires all of them
(`expired: the server restarted`).
Moonglow additionally retains up to 65,536 accepted operation identities for
its process lifetime and refuses conflicting id reuse. This is not a durable
exactly-once guarantee: after a GUI restart and eviction from the game's
256-id window, retrying an old explicit id can execute it again.

### Clients (LLM) → Moonglow

Same listener, same bearer token. `Content-Length` is optional on `GET`.

| Request | Reply |
|---|---|
| `GET /live/v1/status` | `LiveStore::status`: `connected`, `boot`, `seq`, `ageMs`, areas with counts and ages, `queued` commands, recent `results` (newest first, up to 64). |
| `GET /live/v1/area/<resref>` | `LiveStore::area_json`: the area's `creatures`, `placeables`, `doors`, `items`, `ageMs`, `fullAgeMs`; 404 if no report. |
| `POST /live/v1/command` | Body: one command as above, `id` optional (a client's operation id: a retry with the same id queues nothing new). Reply `200 {"queued":"<id>"}` or `400 {"error":…}` (also when no game has reported yet). |

Example (from Windows, `-k` because the certificate is self-signed):

```text
curl -k -H "Authorization: Bearer %MOONGLOW_LIVE_TOKEN%" https://<wsl-host>:5199/live/v1/area/demo_market
curl -k -H "Authorization: Bearer %MOONGLOW_LIVE_TOKEN%" -H "Content-Type: application/json" ^
  -d "{\"area\":\"demo_market\",\"op\":\"npc_say\",\"target\":\"7f00012a\",\"text\":\"Halt!\"}" ^
  https://<wsl-host>:5199/live/v1/command
```

## Matching the game's report to the module (GIT)

Moonglow identifies an object by `(ObjectKind, index in its GIT list)`;
indices shift on every insert and delete, so a match is worked out anew
for each frame, never stored. `live::reconcile(git, live)`:

* **Placeables, doors, items**: a GIT instance and a reported object are
  the same when their resrefs match (case-insensitive) and their positions
  are within **0.1 m** in the plane (they do not move). Nearest first,
  each reported object used once.
* A **non-static** GIT placeable/door/item with no match in a **full**
  report is **destroyed**: hidden. Static placeables are scenery and are
  never taken for destroyed (the game may not report them at all). Before
  the first full report nothing is hidden.
* Every reported object with no GIT match is **extra** (made while the
  game ran): drawn as a ghost.
* **Creatures**: all GIT creatures are hidden in live mode; every reported
  creature is drawn where the game says.
* Two identical objects at one spot cannot be told apart; give one a tag
  or move it.

## Implementation

### Implementation record

Work session: `01a12c89-8b4b-7411-a18d-ee3a85981463`, branch `live-mode`,
starting commit `1f50038`. This section is the cumulative implementation and
reproduction record. Phase checks are recorded below as they finish; passing
tests is not user acceptance. No pushes or writes to the LC fork are authorized.

Reference files were copied on 2026-10-11 from the exact paths above:
`live.rs` SHA-256 `786D78D0AED847A01A39D108407FCF80C94A9429D69369524E5F3862C3600AA0`;
`live_http.rs` SHA-256 `F44CFA0587582834625CB61B3AE2E127A8AB3B60CDA6054CDE913726B65D2D67`.
Use the installed `cargo +1.98.0` toolchain for all checks (PATH's stable is
1.97, below the workspace minimum). WSL needs execution outside the desktop
sandbox; the verified gateway is `<wsl-host>` and the NWNX zip and server
exist at the paths above. Re-discover the gateway for each run.

Owned-file ledger (all relative paths are rooted at
`C:\Projects\moonglow-toolset-original`; owner is the session above):

| Path | Purpose | Status / retention |
|---|---|---|
| `crates/mg-ui/src/live.rs` | Copied protocol core, extended here | Required source |
| `apps/moonglow/src/live_http.rs` | Copied HTTPS listener, extended here | Required source |
| `apps/mg/build.rs` | Reproducible 8 MiB Windows CLI stack reserve | Required source |
| `tools/live/check.ps1` | Reproduce each phase's Cargo gates with bounded concurrency | Required source |
| `tools/live/probe.ps1` | Exercise the actual GUI listener from Windows and WSL | Required source |
| `crates/mg-ui/src/live_view.rs` | Live rendering and input | Required source |
| `tools/live/nwn/`, `tools/live/compile.ps1`, `tools/live/common.ps1`, `tools/live/demo/prepare.ps1`, `tools/live/wsl/extract-includes.py` | Authored game scripts and reproducible compiler/input preparation | Required source |
| `target/test-output/live-demo/` | Single reusable module, NWNX includes, HAK cache, manifest, engine reports and proof | Retained for review/reproduction; import ERF, compiler stamp and superseded pictures/logs removed |
| `target/test-output/live-mode/` | One directory for checks and viewer proof | Final phase-5 logs, initial GUI probe result, fixture and before/after images retained; older phase logs removed |
| `target/debug/` | Cargo outputs created by this task (directory absent at start) | Only final `mg.exe` and `moonglow.exe` retained; disposable cache removed; existing `target/nui-native` and `target/nui-showcase` are not task-owned |
| WSL `~/mglive-home/` | One independent Linux runtime copy of the module/HAKs and scratch server state | Retained for manual demo acceptance; server stopped; remove only after that need ends |

History retention: this session's history must be handled by a separate,
explicitly authorized cleanup using a supported application mechanism. It is
not edited or deleted by this task; archiving alone does not reclaim its size.

Phase 1 implementation: the reference core and listener are wired into the GUI;
the status, area and command routes are implemented. Additional fixes validate
the target's area and kind and protect PCs, DMs and DM possession (`possessed`,
an optional report field, default false). Command identities survive ack-only
reports and eviction of the 64 displayed results; conflicting reuse is refused.
The listener retains at most 65,536 operation identities per process and then
refuses new commands rather than silently forgetting retry identities. Generated
ids have a random process prefix, preventing a GUI restart from reusing `c1`.
Expiry also runs when a client reads status or queues a command while the game
is offline. A full report at test time zero is correctly distinguished from
having received no full report. Request heads are bounded including their
terminator; authentication precedes body reading, and ambiguous framing is refused.

Reproduce the phase gates with `tools/live/check.ps1 -Phase 1` (then 2 through
5 as implemented). The script runs `cargo +1.98.0 fmt`, each targeted test,
`cargo +1.98.0 test --workspace`, clippy with `-D warnings`, and `fmt --check`.
On Windows it points `NWN_ROOT` to a nonexistent scratch install during these
checks to prevent corpus tests from launching a native server. Corpus/engine
skips are not game validation; that is a separate WSL gate. Visual tests will
explicitly use the actual game install. Current phase 1 results: core 11 passed;
listener/application 9 passed, WSL probe ignored by the default suite; area 25
passed. **Phase 1 passed:** workspace 987 tests completed with the explicit
corpus limitation above; clippy all-targets passed with `-D warnings`; formatting
and `fmt --check` passed. No visual behavior was changed in this phase.
The first workspace compilation
exhausted RAM at default concurrency; the check script now sets
`CARGO_BUILD_JOBS=2`. This is a resource failure, not a failing runtime assertion.

The explicit WSL check is
`cargo +1.98.0 test -p moonglow real_tls_from_wsl -- --ignored --nocapture`.
It binds the Windows default gateway seen by WSL on TCP 5199 and launches WSL
curl against its authenticated TLS status route. An initial probe on an
ephemeral port timed out before the TLS handshake; it was changed to 5199 to
match the specified deployment. No firewall settings were changed. The test
certificate and fixed test-only token do not belong to a running game.

The actual GUI smoke test `tools/live/probe.ps1 -Wsl` **passed** on
`<wsl-host>:5199`: Windows sync/status/command/delivery/ack, then status
from WSL. The earlier ephemeral-port timeout did not reproduce on the intended
application/port. The probe uses random credentials kept only in process memory,
scratch application/user data under `target/test-output/live-mode/`, and stops
only the GUI process it starts. It runs no game server and changes no firewall
rules. Rebuild its executable with `cargo +1.98.0 build -p moonglow` first.

Successful phase results are recorded here; final phase-5 logs supersede the
older phase logs at closure, and the GUI/WSL probe result remains under the
owned scratch directory. Superseded failure diagnostics and the probe's empty
user/application data were removed (2,631 logical bytes; allocated disk space
and net free-space change were not attributed). After the baseline picture
compiler finished, the task-owned `target/debug/incremental` cache was also
removed: 15,243,438,565 logical bytes, preserving compiled dependencies, both
applications and test executables. No compiler was using it and no reparse
points were present. Allocated disk space/free-space changes were not attributed;
future Cargo commands may recreate this disposable cache. The active history is
the agent's local Codex session file
(1,562,792 logical bytes at the phase-1 checkpoint, still growing).

Prerequisite discovery: the installed NWNX zip does **not** declare
`NWNX_Object_StringToObject` or `NWNX_Util_GetUtcTime`. The installed game's
`nwscript.nss` (read with `target/debug/mg.exe cat nwscript.nss --no-user-dir`)
does declare native `StringToObject(string)` and `GetMicrosecondCounter()`;
the latter documents elapsed time within one script. It also confirms
`JsonGetInt` reads booleans. The implemented scripts use these checked APIs.
The default demo base has six areas and entry area `demo_area`, and requires
`test_snow.hak` (877,350 bytes), `grass_textures.hak` (16,786,019 bytes), and
`npc_sounds.hak` (2,223,797 bytes). All three source HAKs exist in
the user's `Documents\Neverwinter Nights\hak` folder; this source is read-only.
The build must resolve these inputs in scratch storage; it must not ignore
missing-resource verification or write to that user directory.
The NWNX zip SHA-256 is
`0d57d9549e93ee5bd8cd80371b31efee23b88ff6b6a410d3055b38143e61bd14`.
It has no `nwnx.nss`: the four plugin includes contain no `#include`
dependencies and use native `NWNXCall`/`NWNXPush*`/`NWNXPop*`, verified in the
installed game's `nwscript.nss` from layer `key:nwn_retail`. The build must
import those four actual files, not fail looking for the old common include.

### Phase 1 - core and listener (M)

1. **Dependencies** (`Cargo.toml` `[workspace.dependencies]`, keep the
   alphabetical order):
   ```toml
   rcgen = { version = "0.13", default-features = false, features = ["crypto", "ring"] }
   rustls = { version = "0.23", default-features = false, features = ["ring", "std", "tls12"] }
   ```
   and `rcgen.workspace = true`, `rustls.workspace = true`,
   `serde_json.workspace = true` in `apps/moonglow/Cargo.toml`. (No
   `aws-lc-rs`: it needs CMake/NASM on Windows.) Licences: rustls
   Apache-2.0/MIT/ISC, ring Apache-2.0 AND ISC, rcgen MIT/Apache-2.0 -
   compatible with GPL-3.0.
2. **`crates/mg-ui/src/live.rs`** (copy, see above) and `pub mod live;` in
   `crates/mg-ui/src/lib.rs` (alphabetical, after `pub mod levelup_view;`).
   It holds: the protocol types (`LiveObject`, `Command`, `Op`,
   `SpawnKind`, `CommandResult`), `AreaLive`, `LiveStore`
   (`sync(body, now_ms)`, `enqueue(command, now_ms)`, `area`, `area_json`,
   `status`, `pending_targets`), `GitObject`, `Reconciled`, `reconcile`.
   All time is passed in (`now_ms`) so tests are deterministic.
3. **`Moonglow.live`**: in `crates/mg-ui/src/lib.rs`, after
   `pub var_set_dir: Option<PathBuf>,` (line 463):
   `pub live: Option<std::sync::Arc<live::LiveStore>>,` with a doc
   comment, and `live: None,` in the constructor after `var_set_dir: None,`.
4. **`apps/moonglow/src/live_http.rs`** (copy) and extend `answer()`:
   * route `GET /live/v1/status`, `GET /live/v1/area/<resref>`,
     `POST /live/v1/command` as specified above;
   * `Content-Length` optional for `GET` (treat as 0); still required for
     `POST`;
   * keep: token check before anything else (constant-time compare), one
     request per connection, `Connection: close`, 5 s timeouts, at most 16
     connections at once, `wake.request_repaint()` after each request.
   It starts only when `MOONGLOW_LIVE_TOKEN` is set (at least 32
   characters) and listens on `MOONGLOW_LIVE_BIND` (default
   `127.0.0.1:5199`).
5. **`apps/moonglow/src/main.rs`**: `mod live_http;` beside `mod speakers;`
   (line 10); before `Ok(Box::new(App { … }))` (line 241):
   ```rust
   // Live mode: a running game posts its state here (`live_http`).
   let live = std::sync::Arc::new(mg_ui::live::LiveStore::default());
   match live_http::start(&cc.egui_ctx, live.clone()) {
       Ok(Some(at)) => {
           moonglow.log.info(format!("Live: listening on https://{at}"));
           moonglow.live = Some(live);
       }
       Ok(None) => {}
       Err(e) => moonglow.log.error(format!("Live: {e}")),
   }
   ```
6. **Tests** (already in the copied files; extend for the new routes):
   `live.rs` - full vs quick reports, ack/resend, idempotent ids, TTL and
   restart, stale `seq`, bad and truncated bodies (every prefix), command
   validation and JSON shape, reconciliation. `live_http.rs` (over an
   in-memory duplex, no TLS) - sync, 401/404/405/411/413/431, every
   truncation of a request; add: status, area (found and 404), command
   (queued, refused before a game reported, idempotent id).

### Phase 2 - the area viewer (L)

**Phase 2 passed:** 988 workspace tests completed (same corpus limitation as
phase 1), all targeted gates, warning-free clippy and formatting; the real-game
live input regression passed again after the final toolbar adjustment.
`live_view.rs` supplies the toggle,
reconciliation, cached blueprint looks, markers, HP bars, stale-report age,
selection, pending indication and all v1 actions. PCs/DMs use markers only.
`AreaScene::scene_without` omits replaced/destroyed instances. Live input also
bypasses Ctrl+wheel scaling and cancels pending edit gestures. Blueprint looks
are invalidated with workspace revisions. Particles remain the documented v1
exception; open/locked is text, not an animation pose.

The real GPU baseline was captured and inspected before viewer changes at
`target/test-output/live-mode/viewer/live-before.png`. The first after image
revealed right-edge toolbar overflow; the paired Grid/Live toggles were moved
to the final toolbar row and the next image was inspected. Final proof is
`target/test-output/live-mode/viewer/live-on.png` (the test also regenerates
Live-off `live-off.png`), authored by `cargo +1.98.0 test -p mg-ui --test screens live_mode
-- --ignored --nocapture`, using the installed game and `mg_testkit::gpu::hold()`.
The single reusable fixture is `viewer/fixture.mod`; no game resources or
screenshots are committed. At closure the rejected `live-first.png`, redundant
`live-off.png`, and byte-identical `fixture.mod.bak` are removed. The original
before and final after pictures remain as the minimal visual evidence.

`cargo +1.98.0 test -p mg-ui --test app live_viewer -- --nocapture` exercises
palette click and drag/drop spawn, Delete, Walk Here with Shift/run, player
protection, unchanged GIT and unchanged undo history. The existing
`area_viewer_selects_moves_and_deletes` also passed with the real game (Live
off). These tests use the existing serialized GPU harness. Their task-owned
scratch folders are `target/test-output/ui-area-live`, `ui-area-move`, and
`ui-area-place`; retain only useful final evidence after the phase closes.
Phase closure removed the three redundant viewer files and the two existing
interaction-test folders (`ui-area-live`, `ui-area-move`): 1,757,566 logical
bytes. `ui-area-place` did not exist. Resolved paths and absence of reparse
points were checked; no allocated-size or free-space gain is inferred.

New module **`crates/mg-ui/src/live_view.rs`** (`mod live_view;` in
`lib.rs`), so that `area_view.rs` changes stay a few calls.

State, a field `pub live: crate::live_view::LiveView` on `AreaView`
(`area_view.rs:306`, initialised in `AreaView::new`, line 494):

```rust
#[derive(Debug, Default)]
pub struct LiveView {
    pub on: bool,
    /// The game object selected (its id).
    pub selected: Option<String>,
    /// The next click picks a place to walk to, or a creature to attack.
    pub aim: Option<Aim>,          // enum Aim { Move(String), Attack(String) }
    /// The line being typed for Say…: (target, text).
    pub say: Option<(String, String)>,
    /// Models for reported objects, by kind and resref (None: no blueprint).
    looks: HashMap<(ObjectKind, String), Option<mg_area::AreaObject>>,
}
```

Functions:

* **`toolbar(app, ui, view)`** - called in `toolbar()` (line 1471) beside
  the Grid toggle (line 1538): `ui.toggle_value(&mut view.live.on, "📡 Live")`,
  disabled with a hover text when `app.live` is `None` ("Start Moonglow
  with MOONGLOW_LIVE_TOKEN to use live mode").
* **`frame(app, view) -> Option<LiveFrame>`** - when `view.live.on` and
  the store has a report for `view.area` (its resref): clone the
  `AreaLive`, build `GitObject`s from `view.model.objects`
  (`template` as string, `position`, `is_static`), `reconcile`, and
  return `{ live: AreaLive, hidden: Vec<usize> /* indices in model.objects */, extras: Vec<AreaObject> }`.
  Extras: for each reported extra, its look from `looks` (made once per
  resref like `ghost()` does, line 3610: `instance_of(app, key, Placement{…}, &[])`
  then `mg_area::AreaObject::read(game, kind, usize::MAX, &item)`; make
  `instance_of` `pub(crate)`), positioned at the reported place, rotation
  `facing.to_radians() - FRAC_PI_2` (an `AreaObject`'s rotation is its
  facing less 90°). Players (`pc`) and objects whose blueprint is not
  found get no look: they are drawn as markers only.
* In **`viewport()`** (line 1770):
  * after the dragged objects are applied to `shown` (the `Cow`, line
    1898), compute `let live = crate::live_view::frame(app, view);`
    (before `model` is borrowed, or clone what is needed);
  * replace `scene.scene_hiding(&shown, &settings, &hidden)` (line 1928)
    with `scene.scene_without(&shown, &settings, &hidden, &live_hidden)`;
  * add the extras' instances with `scene.ghost(&vp.gpu, game, o, view.time, 1.0)`
    next to `frame.instances.extend(ghost_instances)` (line 1940)
    (opacity 1.0: they are real, not previews);
  * after `overlays(…)`, call `crate::live_view::overlay(ui, view, &live, &pending)`.
* **`mg-area` change** (`crates/mg-area/src/scene.rs:605`): rename the
  body of `scene_hiding` into
  `pub fn scene_without(&self, area, view, hidden_tiles: &[usize], hidden_objects: &[usize]) -> Scene`,
  skipping object `i` when `hidden_objects.contains(&i)` in the objects
  loop (`for (i, (o, shown)) in … .enumerate()`); `scene_hiding` calls it
  with `&[]`. Particles of hidden objects may still show: acceptable in
  v1 (note it).
* **`overlay(ui, view, frame, pending)`** - with `ui.painter_at(view.rect)`
  and `view.screen_pos(p)`: for every reported creature and every extra a
  marker (circle 6 pt): players blue, DMs violet, own (made live) green,
  others orange; the name above; an HP bar under when `hp` is known; a
  ring around `selected`; "⏳" beside objects in `pending_targets()`.
  For matched placeables and doors: a small label "open", "locked" when
  set (shortcut: a label, not the open/closed animation; pose them when
  the scene learns `AnimationState`). The age of the report in the
  view's corner ("Live: 1.2 s"), red when older than 3 s ("Live: no
  report for this area" when none).
* **`input(app, ui, view, response) -> bool`** - called in `input()`
  (line 2709) right after `camera_input(…)` (line 2721):
  `if view.live.on { crate::live_view::input(app, ui, view, response); return; }`.
  In live mode no GIT edit can happen: dragging, pasting, terrain, tiles
  and Delete-as-edit are all bypassed. It handles:
  * a blueprint dropped from the palette (`response.dnd_release_payload::<crate::palette_view::Dragged>()`)
    or a click while the palette has one chosen (`brush(app)`, make it
    `pub(crate)`) → `spawn` at `view.ground_at(pos, 0.0)` snapped with
    `view.snapped`, facing `view.ghost_turn` in degrees + 90 (make
    `ghost_turn` `pub(crate)`, line 403); kind from the restype: UTC →
    creature, UTP → placeable, UTI → item; any other restype → a notice
    "Live mode spawns creatures, placeables and items" (`view.notice`);
  * a click near a marker (nearest within 12 pt) → `selected`; on empty
    ground → none;
  * `aim`: the next click sends `npc_move` (ground point; Shift = run) or
    `npc_attack` (the marker clicked); Escape cancels;
  * Delete with a selection → `destroy`;
  * right-click → `response.context_menu(|ui| menu(app, view, ui))`:
    creature: **Walk Here…** (sets `aim`), **Say…** (opens the `say`
    window), **Attack…** (sets `aim`), **Destroy**; placeable/door:
    **Open**, **Close**, **Lock**, **Unlock**, **Destroy**; player: none
    of these (show "a player: nothing to do"). The **Say…** window: a
    one-line text field, OK sends `npc_say`.
  * Every command goes through `app.live.as_ref()?.enqueue(cmd, now_ms)`;
    an `Err` goes to `app.log.warn`.
* The palette's ghost (what would be placed) keeps showing in live mode:
  that is where a spawn will land.

### Phase 3 - the NWScript side (M)

Implementation: the five authored scripts implement full/quick reports,
one in-flight request with a five-tick timeout, retained acknowledgements,
bounded 256-id deduplication, protected targets and all six operations.
Native `StringToObject` replaces the nonexistent NWNX wrapper. The launcher
will supply `MGLIVE_BOOT`; without it the script uses random plus a counter.
`GetMicrosecondCounter` measures each first/60th serialization tick. Native
`JsonFind` avoids a quadratic scripted acknowledgement scan. Failures log at
most once per 60 ticks and do not log the token or request body.

Run `tools/live/compile.ps1`: it prepares the one working demo module under
`target/test-output/live-demo/module`, copies the four required includes
directly from the installed WSL zip, copies required HAKs once into `user/hak`,
imports an authored script ERF, and compiles every script. `inputs.json`
records hashes and original OnModuleLoad (`city_boot`); a changed base hash
refuses reuse rather than silently using stale inputs. The base SHA-256 is
`4FF92AFB1E650887E355D9F2A9CD8DF5A00FC81563C9D07CC2CA51316ADA8DDB`.
The compiler returned 55 scripts, zero failures. An initial reserved variable
name `event` was corrected; the wrapper checks JSON `failed`, because CLI
exit success alone does not prove compilation. NWNX includes and game assets
stay uncommitted. Full phase checks and engine behavior are recorded separately.
**Phase 3 passed:** all 55 scripts compiled; 988 workspace tests, targeted
11 core / 9 listener / 25 area tests, warning-free clippy and formatting passed.
PowerShell sources parse without diagnostics. This is compiler validation;
private WSL execution is phase 4. No new visual change or redundant full
binary copy was introduced. The working module and dependency cache are
retained for phase 4; the failed compile report was replaced by its passing result.

Files authored for this project, kept in **`tools/live/nwn/`** (they are
ours, not game data; `tools/aurora/` is the precedent for tooling):

* **`mglive_inc.nss`** - constants and helpers:
  `MGLIVE_PERIOD = 1.0`, `MGLIVE_FULL_EVERY = 5`, `MGLIVE_MAX_DONE = 256`;
  config from the environment with `NWNX_Util_GetEnvironmentVariable`:
  `MGLIVE_HOST`, `MGLIVE_PORT` (5199), `MGLIVE_TOKEN`, `MGLIVE_AREAS`
  (comma-separated resrefs always reported, for tests without a client).
  `json MgLiveObject(object o, int bFull)` builds one entry
  (`ObjectToString`, `GetTag`, `GetResRef`, `GetName`, `GetIsPC`,
  `GetIsDM`, `GetPosition`, `GetFacing`, `GetCurrentHitPoints`/`GetMaxHitPoints`,
  `GetCurrentAction`, `GetIsOpen`, `GetLocked`, `GetLocalInt(o,"MGLIVE_OWN")`).
  `json MgLiveArea(object oArea, int bFull)` walks
  `GetFirstObjectInArea`/`GetNextObjectInArea` and sorts by
  `GetObjectType` into `cre`, `plc`, `doors`, `items`.
* **`mglive_load.nss`** - called from the module's OnModuleLoad wrapper:
  chooses `boot` from the launcher's random `MGLIVE_BOOT`, with random plus
  `GetMicrosecondCounter()` as a fallback; subscribes
  `NWNX_Events_SubscribeEvent("NWNX_ON_HTTPCLIENT_SUCCESS", "mglive_http")`
  and `…_FAILED`, starts the tick; does nothing when `MGLIVE_TOKEN` is
  empty (live mode off).
* **`mglive_tick.nss`** - runs on the module every `MGLIVE_PERIOD` with
  `DelayCommand`. If a request is in flight (module int
  `MGLIVE_INFLIGHT` = its id) and younger than 5 s, skip this tick. Else:
  areas = those with a player (`GetFirstPC`/`GetNextPC`, `GetArea`) ∪
  `MGLIVE_AREAS`; build the body (`v`, `boot`, `seq`+1, `ack`, `results`
  from module `LocalJson` `MGLIVE_ACK`/`MGLIVE_RESULTS`, `areas`, `full`
  every fifth tick); `NWNX_HTTPClient_SendRequest` with
  `nRequestMethod=1`, `sHost=MGLIVE_HOST`, `nPort`, `sPath="/live/v1/sync"`,
  `nContentType=2`, `nAuthType=3`, `sAuthUserOrToken=MGLIVE_TOKEN`; store
  the id in `MGLIVE_INFLIGHT`. The acks and results sent stay pending
  until the post succeeds (cleared in `mglive_http`, so a failed post
  sends them again).
* **`mglive_http.nss`** - the event script: `NWNX_Events_GetCurrentEvent()`;
  ignore a `REQUEST_ID` other than `MGLIVE_INFLIGHT`; clear the in-flight
  mark. On `FAILED` or `STATUS` ≠ 200: log (`WriteTimestampedLogEntry`)
  once per minute at most and keep the pending acks. On success: clear the
  acks/results that were sent, `JsonParse(RESPONSE)`, and for each
  command: if its id is in `MGLIVE_DONE` (the last 256 ids) only ack it;
  else run it (`mglive_exec`), append the id to `MGLIVE_DONE` (drop the
  oldest past 256), add it to `MGLIVE_ACK` and its result to
  `MGLIVE_RESULTS`.
* **`mglive_exec.nss`** (an include with `json MgLiveRun(json jCommand)`
  returning the result object) - the table in [Commands](#commands):
  area by resref (`GetObjectByTag` will not do; loop
  `GetFirstArea`/`GetNextArea` comparing `GetResRef`), objects by
  native EE `StringToObject(id)`, every
  check from the rules, `Location(oArea, Vector(x,y,z), f)`, then the
  action. Errors are short English strings in `err`.

Keep each script's NWScript readable (the LC demo scripts' style) and
compile them with Moonglow's compiler (`mg compile`) against the NWNX
includes (copy `nwnx_httpclient.nss`, `nwnx_events.nss`,
`nwnx_util.nss`, `nwnx_object.nss` from `~/unified/Binaries/NWScript.zip`
into the demo module; they are NWNX's, MIT-licensed, and are not
committed here - the build script fetches them).

### Phase 4 - the WSL demo (M)

Implemented entry points: `tools/live/demo/build.ps1`,
`tools/live/Start-LiveDemo.ps1`, `tools/live/Stop-LiveDemo.ps1`,
`tools/live/wsl/{start-server,stop-server}.sh`, and `tools/live/test-demo.ps1`.
All are authored source owned by this session. The launcher waits for TLS
readiness and the first actual area report. Credentials stay in process memory
(the interactive launcher prints its generated API token); session metadata
contains only process identity, address and paths. Stop validates executable,
PID and start time, and never kills unrelated servers. The Linux workspace
`~/mglive-home` is task-owned (absent before this task); its separate
module/HAK copies are necessary runtime inputs, reused across starts.

Build result: 56 scripts, zero compile failures; `mg verify`: zero errors and
warnings. The wrapper calls the original `city_boot` then `mglive_load`.
The exact supplied base had obsolete `nw_ch_aca` rest hooks in `demo.git`;
neither the module nor installed game has that script. The build clears only
those hooks in the scratch copy, and only for the recorded base hash. No
reference errors are waived. The base and source HAKs remain unchanged.

Reproduction: build both binaries (`cargo +1.98.0 build -p mg -p moonglow`),
then `tools/live/test-demo.ps1 -Screens` (or `-SkipBuild -Screens` for the
current verified candidate). **Real engine/API proof passed:** 22 routine NPCs
moved in a two-second sample; report age was 82 ms at the sample. Creature,
placeable and item spawns appeared with `own`; creature move changed position;
say and attack dispatch returned success; door open/unlock and placeable
open/lock appeared in full reports; destroy removed the spawned creature.
Attack proof confirms dispatch, not combat damage. After killing the GUI for
10 seconds, the game reconnected and acknowledged a repeated spawn id without
creating another object. Restarting the server changed `boot` and expired a
command queued against the old process. `e2e.json` holds compact results.

Engine: NWN 8193.37-17 (`26c6e573`), NWNX 8193.37-17 (`3f504d0f14`). SHA-256:
server `d6d952826b671fc62c376dca912e87f1aa45d4aa81c620e912ceb4d8e051111d`;
Core `ee0d0bddb8fd434291954ad2d9868e734d67e0538113b49a66053f1dc16c2865`.
Observed full-report serialization: 3,619–5,127 microseconds for 37 creatures,
32 placeables and one door, about 12,531 bytes. These are small demo samples,
not a throughput benchmark. The server reported that public listing was
disabled; its fixed arguments include `-publicserver 0` and random passwords.

Visual proof: `live_demo_snapshot` renders the real area JSON captured from
the GUI's API, using the actual demo module, HAKs and GPU. Inspected pictures
under `target/test-output/live-demo/`: `move-before.png`, `move-after.png`,
`spawn-door.png`, `door-open.png`; their three input JSON files are retained.
The test calls `mg_testkit::gpu::hold()` and asserts drawable NPC geometry
(23 instances for `city_actor`, one for `nw_bartender`). An initially distant
camera obscured bodies behind gate geometry; the closer final camera shows
both bodies and markers. No renderer change was needed. These are captured
engine reports rendered through Moonglow's real viewer, **not desktop-window
interaction proof**: the Windows computer-use helper failed to write its
kernel assets before initialization, including after reset. The user's client
join/player-marker check remains manual; no native client was started.

Restart fixes: the first launcher compared a JSON-parsed date against a string,
leaving its own GUI alive. Numeric UTC tick comparison and `WaitForExit` fixed
that ownership check. The TLS handler now sends close-notify and briefly drains
the peer's TCP close, avoiding active-close TIME_WAIT during normal reconnects;
the real TLS test also asserts same-port rebind and refusal of a second active
listener. No `SO_REUSEADDR` sharing is enabled (see
[Microsoft's socket guidance](https://learn.microsoft.com/windows/win32/winsock/using-so-reuseaddr-and-so-exclusiveaddruse)).

The automated test stops its GUI/server in `finally`; the interactive launcher
leaves them running until `Stop-LiveDemo.ps1`. Phase 4 Cargo gates and closure
cleanup are recorded below after completion.

**Phase 4 passed:** 988 workspace tests, all targeted gates, clippy with
`-D warnings`, formatting, shell/PowerShell parsing, the final launcher
readiness/stop smoke, end-to-end engine tests, and inspected GPU evidence.
The final smoke reported 37 creatures and stopped cleanly; UDP 5121 and the
owned GUI were absent afterward. Closure removed the superseded camera
picture, old GUI logs and reproducible import ERF: 2,288,319 logical bytes.
The one candidate MOD, source working folder, required Windows/WSL HAK caches,
compact JSON/results, `engine.log`, latest GUI log and four final pictures are
retained for review/reproduction. No allocated/free-space gain is inferred.

In **`tools/live/`**:

* **`demo/build.ps1`** - makes `DEMO-Live-01.mod` in a scratch folder
  (`target/test-output/live-demo/`): starts from the module given by
  `-Base` (default: `<demo-source>\demo.mod`,
  six areas, including `demo_area` with walking NPCs; not committed), imports `tools/live/nwn/*.nss`
  and the NWNX includes, hooks `mglive_load` into OnModuleLoad (prepend a
  call to the existing script or set it when there is none), compiles,
  and runs `mg verify`. Use `mg` subcommands (`mg import`, `mg set`,
  `mg compile`, `mg pack` - see `docs/manual/11-command-line.md`).
* **`wsl/start-server.sh MODULE_PATH HOST PORT TOKEN [AREAS]`** - after
  the private launcher script: a scratch user directory `~/mglive-home`
  (never the real NWN folder), the module copied into its `modules/`,
  `env -i` with only: `LD_PRELOAD=~/unified/Binaries/NWNX_Core.so`,
  `NWNX_CORE_LOAD_PATH=~/unified/Binaries`, `NWNX_CORE_SKIP_ALL=y`,
  `NWNX_HTTPCLIENT_SKIP=n`, `NWNX_EVENTS_SKIP=n`, `NWNX_UTIL_SKIP=n`,
  `NWNX_OBJECT_SKIP=n`, `NWNX_HTTPCLIENT_REQUEST_TIMEOUT=2000`,
  `MGLIVE_HOST`, `MGLIVE_PORT`, `MGLIVE_TOKEN`, `MGLIVE_AREAS`; then
  `nwserver-linux -userdirectory ~/mglive-home -module DEMO-Live-01 -port 5121 -publicserver 0 -playerpassword <random> -dmpassword <random> -servervault 1`.
  **Never without `-publicserver 0`.**
* **`Start-LiveDemo.ps1`** - finds the Windows address WSL sees
  (`wsl -e sh -c "ip route show default"` → third field), makes a token
  (32 random bytes, hex), starts Moonglow with
  `MOONGLOW_LIVE_BIND=<ip>:5199` and `MOONGLOW_LIVE_TOKEN` set and the
  demo module open, waits for "Live: listening" in its log, then runs
  `start-server.sh` in WSL with the same host, port and token. Prints the
  token and a `curl -k` line for the client endpoints. The first time,
  Windows asks to allow Moonglow through the firewall on that network:
  the user must allow it (do not add firewall rules from the script).

### Phase 5 - documentation (S)

The two manual chapters now explain viewer controls, report ages, setup,
environment variables, client requests and local security assumptions. The
status/index/log and deferred list distinguish implementation, automated proof
and manual acceptance. The protocol's earlier "exactly once" wording has been
qualified to match its finite in-memory deduplication windows. The scoped
`tools/live/wsl/.gitattributes` (authored source, retained) pins shell scripts
to LF so a Windows checkout with `core.autocrlf` can still run them in WSL.
`okf` is not installed; both indexes were updated by hand. A fallback check
validated all 327 local file links in the changed documents and their required
frontmatter fields; built-in manual tests also run in the workspace gate.
**Initial phase 5 pass:** 988 workspace tests, 11 core / 9 listener / 25 area tests,
clippy with `-D warnings`, `cargo fmt` and `fmt --check`. The existing manual
frontmatter, chapter-link and table-layout tests passed. The remaining manual
acceptance checks are listed above; no claim of full corpus execution is made.

Final binary validation exposed a separate Windows debug-CLI issue: the default
MSVC main-thread stack is 1 MiB, and unoptimized `mg verify` overflowed it. The
test-profile CLI used in phases 3–4 was unaffected. A controlled PE-header probe
with an 8 MiB reserve verified the same candidate with zero errors/warnings.
The permanent fix is `apps/mg/build.rs`, passing `/STACK:8388608` only for MSVC.
The probe executable was replaced by a normal source-built binary. Its PE
header confirms the 8,388,608-byte stack reserve, and it verifies the retained
candidate with zero errors and warnings. All final Cargo gates were repeated:
988 workspace tests passed, targeted suites passed, and clippy with `-D warnings`
and formatting passed. The documentation's two-job build setting was included
in the final manual tests and followed by another clean clippy/format check.
This issue was found after the first cache cleanup, so the rebuild necessarily
regenerated disposable Cargo outputs; they are removed again after validation.

The final GUI build initially could not replace an executable opened outside
the recorded demo session. The user saved and closed that window; no unowned
process was stopped. Retrying the build succeeded. The new GUI passed an
authenticated TLS status/start/stop smoke test. No task compiler, repository
application process or WSL UDP 5121 listener remained at the closure check.

Final source-built artifacts (SHA-256; build with Rust 1.98.0 and two jobs):

| Path | Logical bytes | SHA-256 |
|---|---:|---|
| `target/debug/mg.exe` | 17,134,080 | `F782248DB0190AB8182614225629196A7202BB431C867EEE4E4B0F41CE02C851` |
| `target/debug/moonglow.exe` | 52,936,192 | `C87CD15BBB0CF64642082210A76997E5D1FFDFB60DB3A0FA6F0F33A6FF9A902F` |
| `target/test-output/live-demo/DEMO-Live-01.mod` | 1,000,935 | `A785422C8AE7ADF088B04A2C1896F3AE031B44EBA4CD3D82028799451B55BA72` |

Final retention: `target/debug/mg.exe` and `moonglow.exe`, the current
demo source/candidate/dependency sets, the six required before/after and demo
images, their captured engine inputs, final checks and compact results. The
remaining task-created Cargo cache, symbols, test executables and reproducible
test scratch directories were removed after final binary validation. Existing
`target/nui-native`, `target/nui-showcase`, and the pre-existing
`target/.rustc_info.json` are outside that cleanup. All deleted byte counts
below are logical file sizes, not allocated disk size or net free-space change.
The first final cleanup removed 29,241 files (36,112,014,108 logical bytes),
including all disposable Cargo output and other test scratch siblings. It
preserved both executables and the two live evidence directories. This is a
gross deletion count; recreated build files are not a net storage gain.

The final cleanup removed **36,239 files / 30,081,813,357 logical bytes**,
including regenerated Cargo outputs, 105 test scratch siblings, superseded
GUI logs and the compiler stamp. Its empty application-cache directory was
also removed. Resolved paths stayed under the owned output roots, no reparse
points were present, and no compiler or repository application was using them.
Only the two executables remain under `target/debug`; only `live-demo` and
`live-mode` remain under `target/test-output`. The executables were then tested
without Cargo's cache: candidate verification still returned zero errors and
warnings, and authenticated GUI TLS startup/stop passed. Removing the redundant
smoke log afterward did not affect either executable.

Retained Windows outputs total 104,145,539 logical bytes: executables
70,070,272; demo/current inputs and evidence 32,416,854; final checks and viewer
proof 1,658,413. All six required images still exist. The source-built artifact
hashes above identify the current result awaiting user review. Physical disk
allocation and net free-space recovery were not measured or attributed; previous
gross cleanup totals include caches that the final fix required rebuilding.

The active history identified in the implementation record was 37,516,315
logical bytes at the final inventory and is still growing. It was not modified.
It is reported for separate authorized cleanup with this document as the
durable knowledge source; no approved supported history-deletion mechanism is
available in this task. Client-join acceptance remains the next manual step.

* **Manual**: a "Live mode" section in `docs/manual/04-areas.md` (the
  toggle, what is shown, the actions, the age indicator) and a short
  section in `docs/manual/09-build-and-test.md` (starting a server for
  it, the environment variables, the client endpoints with the curl
  example, the security notes: private server, token, loopback/WSL
  address only). Keep each chapter's frontmatter (`crates/mg-ui/src/manual.rs`
  tests require it).
* **This proposal**: set Status and add "Where it stands" when built.
* **`docs/log.md`**: one entry under the day. **`docs/deferred.md`**:
  what v1 leaves (below). Run `okf index docs` and `okf lint docs --links`
  if the `okf` tool is installed; otherwise add this file to
  `docs/index.md` by hand next to the other proposals.

## Tests and verification

Run after each phase; all must pass before the next:

```text
cargo test -p mg-ui --lib live::
cargo test -p moonglow
cargo test -p mg-area
cargo test --workspace
cargo clippy --workspace --all-targets      (no warnings)
cargo fmt                                    (rustfmt.toml: width 100)
```

1. **Core** (phase 1): the unit tests listed above.
2. **Listener over real TLS** (phase 1): an ignored test or a manual
   check - start Moonglow with a token, `curl -k` a sync body, then
   `GET /live/v1/status` shows the area; a command posted to
   `/live/v1/command` appears in the next sync reply; an `ack` removes it.
   From WSL: `curl -k https://<ip>:5199/live/v1/status -H "Authorization: Bearer …"`
   proves the firewall and address.
3. **Viewer, picture first** (phase 2; CLAUDE.md: "something that looks
   wrong in a view is reproduced as a picture"): a screens test in
   `crates/mg-ui/tests/screens.rs` (pattern: `object_walkmeshes`, line
   744; `shoot`, line 16) that opens an area, fills `app.live` with a
   fixture report (two creatures, a player, a destroyed placeable, an
   extra placeable), turns Live on and writes the PNG. Look at it before
   and after each visual change. Tests that open a GPU device call
   `mg_testkit::gpu::hold()` first.
4. **Viewer behaviour** (phase 2), in `crates/mg-ui/tests/app.rs` with
   `area_harness` (line 5137), `screen` (5222), `press` (5226):
   * with Live on and a blueprint chosen in the palette, a click queues a
     `spawn` at the clicked ground point, and the GIT is unchanged
     (`ws.can_undo()` is still what it was; the creature list length is
     unchanged);
   * Delete with a live selection queues `destroy` and deletes nothing;
   * Walk Here… then a click queues `npc_move` to that point;
   * a GIT creature is hidden and a reported one is drawn (scene
     instance count or the reconcile result);
   * Live off: the viewer behaves exactly as before (an existing place /
     delete test still passes).
5. **NWScript** (phase 3): the scripts compile with `mg compile`; a
   module-level test script (pattern: `mg test`, `docs/manual/09-build-and-test.md`)
   may cover `MgLiveObject`'s JSON shape without NWNX.
6. **End to end in WSL** (phase 4), no client needed (`MGLIVE_AREAS` set
   to the demo area):
   * the NPCs move in Moonglow's view within a couple of seconds of each
     other step;
   * `POST /live/v1/command` spawn → the next report has a creature with
     `own: true`; the viewer draws it; destroy → it is gone;
   * `npc_say` → result `ok` (and the line in the server log if logged);
   * a door `set_state` open → the label "open" in the view;
   * kill Moonglow for 10 s and start it again: the game keeps posting,
     reconnects, nothing is run twice;
   * restart the server: `boot` changes, old commands expire.
7. **By hand**: the user's NWN client joins the WSL address
   (`wsl hostname -I`, first address) on port 5121 with the player
   password - not `127.0.0.1`: WSL forwards Windows' localhost for TCP,
   and the game connects over UDP. The player shows as a blue marker and
   moves.

## Risks and open questions

* **Firewall**: the first listen on the WSL adapter asks the user; if
  declined, WSL cannot reach Moonglow (check 2 above shows it).
* **The Windows address seen from WSL changes after a reboot**: the start
  script reads it each time.
* **Client/server build match**: the WSL `nwserver` must be the same EE
  build as the user's client for check 7.
* **NWScript cost**: building JSON for >100 objects every second; measure
  on the demo (log the tick's duration) and lower `full` frequency or
  report fewer fields if needed.
* **NWNX version changes**: recheck the four extracted includes against the
  installed zip and the game's native `nwscript.nss` when upgrading. V1's
  checked APIs and exact source hash are in the implementation record.
* **Static placeables**: whether `GetFirstObjectInArea` returns them is
  not known; the matching works either way.

## Not in v1 (for deferred.md)

* Diffs instead of whole reports; several servers at once.
* Doors and placeables posed open/closed (needs `AnimationState` in the
  scene); creatures animated walking (ghosts stand in their idle pose).
* Moving objects by dragging them in live mode; spawning doors,
  waypoints, triggers, encounters, sounds, stores.
* Separate tokens for the game and for clients; an MCP server over the
  client endpoints.
* LC's DM command catalogue (MySQL) as live commands.
* Running the demo through `mg-serve` (no NWNX there).

## Housekeeping outside this repo

The reference files were written in an earlier private Moonglow fork, whose
`source/` is an upstream checkout with local changes recorded as a patch
(`bridge/Refresh-Patch.py`). These live-mode edits there are **not** part
of that patch and should be reverted by the owner once the files are
copied here: `Cargo.toml`, `apps/moonglow/Cargo.toml`,
`apps/moonglow/src/{main.rs,mcp_bridge.rs,mcp_network.rs,live_http.rs}`,
`crates/mg-ui/src/{lib.rs,live.rs}`, and in `mcp/`:
`src/server.mjs`, `src/live-game-tools.mjs`, `test/live-game.test.mjs`.
