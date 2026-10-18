---
type: Implementation
title: Prepare a module for Live
description: Shared Rust preparation pipeline, source preservation, deployment files and reproducible private proofs.
tags: [live, module, compiler, nwnx, cli, gui]
generated: { by: codex, at: 2026-10-16T12:00:00Z }
---

# Prepare a module for Live

The canonical implementation is `mg_live::build::prepare(Request, GameInstall)`,
enabled by the `mg-live/build` Cargo feature. The CLI and UI opt in explicitly;
headless runtime consumers use the empty default feature set. See the
[library feature checks](live-library.md#optional-module-preparation).
It belongs in mg-live because Live owns the generated content and this layer may
depend on mg-module, mg-resman and the built-in compiler (PLAN §4). Neither the
module layer nor the CLI depends on the GUI. `mg live-build` and `Tab::LivePrepare`
call this same function. Settings keeps `live_nwnx` separately from the running
Live service configuration; changing the build folder does not restart Live.

## Contract and preservation

Inputs are a MOD, folder or nasher project, the NWNX directory containing
`Binaries/NWScript.zip`, and the installed game resources/dependencies.
`--capture-dialogue` defaults off. The demo explicitly defaults it on.
The module is opened once into an isolated in-memory copy. All `mglive_*.nss`
sources are embedded with generated `include_str!` entries at Cargo build time;
the released executable never reads repository files. A catalogue test catches
omitted scripts. Required NWNX includes are read from the user's zip, with size
bounds and no writes/extraction into their installation.

`mglive_start` calls the original `Mod_OnModLoad` before `mglive_load`.
`mglive_build.txt` in the output records original load and generated source hashes.
It permits safe repeat preparation without wrapper nesting. The earlier demo's
known generated wrapper is also recognized; arbitrary code with this reserved
name is refused. Other script name clashes are warnings and affect only the copy.
The optional dialogue pass validates with mg-gff, appends replacement action
tables and updates the GFF header. Every original byte after the header remains
intact, including text, unknown fields and action-parameter payloads. Parsed GFFs
are compared after restoring action fields, so a non-action change is an error.
Generated node actions call the collector before the original action and forward
its parameters. Already owned instrumentation is preserved rather than nested.

Every NSS is compiled serially using the built-in compiler. `mg verify`'s reference
and content-doctor checks run before publishing; optional external plugin checks
are not run. Errors return a failed report without writing outputs. Missing HAKs,
script collisions and non-fatal findings appear as warnings. The archive and its
two sidecars use exclusive creation: existing files, including the source, are
never replaced. A failed publication removes only files created by that call.

Default output: `<input-stem>-live.mod`, `<input-stem>-live.live.env`, and
`<input-stem>-live.start.sh`. The latter is run with bash, so executable permission
is unnecessary on Windows/WSL. It owns only a marked adjacent `<stem>-server`
runtime folder. It checks the UDP port and never stops another process, uses
`-publicserver 0`, no player/DM passwords and a random unprinted admin password.
Redis must already exist. Sidecars explain each plugin switch, charset, channel,
prefix, area selection and players/chunks switches; HAK/TLK requirements are listed.
They do not deploy dependencies or alter the user's installed game.

Windows cannot open an absolute Linux symlink through the WSL UNC provider.
Use the real NWNX include-archive directory (the demo resolves it with read-only
`realpath` before calling Rust). The runtime NWNX/server paths can then be edited
for the actual Linux host. The test uses `~/unified/Binaries/NWScript.zip`
for compilation and `~/nwnx-redis-reconnect/Binaries` for the private engine.

## Demo and test harness migration

`tools/live/demo/prepare.ps1` now only stages the demo's existing HAKs and the
optional owner-approved, hash-bound base repair. `build.ps1` calls `mg live-build`
into one marked staging folder and promotes the verified result. `compile.ps1`
is a compatibility delegate. The Python dialogue instrumenter was removed;
the H11 fixture uses `mg_live::build::capture_dialogue` too. The small include
extractor remains only for private engine experiments which intentionally compile
edited NSS rather than a release's embedded source set; the demo does not call it.
Pre-existing demo scratch folders are not deleted by this migration.

## Reproduction and evidence

Use `tools/live/check.ps1 -Phase 8 -Package mg-live -Jobs 1 -NoUiDebug
-UiUnoptimized` for the Rust tests (NWN_ROOT is guarded). The authored full-build
test is explicit: set `MGLIVE_BUILD_RESOURCE_ROOT` to the read-only EE installation
and `MGLIVE_BUILD_NWNX` to the real NWNX archive directory, then filter
`prepare_private_live_build`. It reuses `target/test-output/live-build/`, verifies
source bytes, compiled scripts, the load chain, dialogue bytes and repeat updates.
It obtains the small authored area's ARE/IFO from the existing private-events
fixture; create that fixture through `tools/live/test-events.sh` when absent.
With the same variables, the `mg` test group filter `live_build` checks real CLI
builds of the MOD, folder and nasher project as well as discovery/error behavior.

After preparation, run in WSL:

```sh
python3 tools/live/test-live-build.py
```

Only the owned `~/mglive-events` server (UDP 5126, Redis 6392) is used. The helper
checks ownership, port vacancy and PID identity; no demo lifecycle call is made.
It checks the original handler's marker before Live initialization, a state report,
and a spawned creature in a subsequent report, then stops only its own children.
Evidence is `target/test-output/live-build/{build-report,engine-proof,engine-state}.json`.
`live_prepare_uses_saved_source_browses_without_replacing_workspace` exercises the
window without a server. `live_prepare_snapshot` in screens.rs renders its report.

The task ledger lives in `target/test-output/live-build/ledger.md`; session
`01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`. Retain only the current authored input,
prepared MOD, compact proof/report, and one inspected window picture for review.
No history JSONL is modified; its retention remains a separate owner-approved task.

## Verified in this change

The private build compiled 21 entry scripts, added 78 resources and instrumented
one authored node, with zero warnings/errors. Updating that output kept the same
original-first wrapper and added no nested node wrappers. The native server ran
the original load with `MGLIVE_STARTED=0`, retained its local value 123, published
Live state and reported the commanded bartender at (3, 3). Both owned processes
exited. Source SHA256 `b9336735d4399c45cc08fef906abd8ac324261cbab68123713b1cd2e56edda4c`;
prepared SHA256 `b9d98748f160daa145d6f39c02dd5a13d37608738f05ce4be694c8024c1b95e4`.
NWNX include archive SHA256 `0d57d9549e93ee5bd8cd80371b31efee23b88ff6b6a410d3055b38143e61bd14`.
The engine proof records the server binary hash and all owned PID/start identities.

The CLI successfully built the same fixture from a MOD, loose folder and nasher
project, checking the input after each. The app test checked default output,
browse, capture checkbox and source/workspace preservation. The demo packaging wrapper also passed two identical builds and capture off
using only the authored fixture (`tools/live/test-build-demo.ps1`); the enabled
outputs were byte-identical and the source hash stayed unchanged. The window
picture was inspected at `target/test-output/screens/live-prepare.png`.
The running owner's demo was not rebuilt or used for these checks. Client rendering and deployment
on another operating system are not claimed by this headless proof.

Final shared-tree gates: mg-live library 169 passed / 4 ignored, CLI build
checks 2 passed, app flow 1 passed, picture 1 passed and inspected. Clippy
`-D warnings` passed for mg-live, mg and mg-ui; workspace `cargo fmt --check`
passed. All Cargo checks used the guarded launcher, Jobs 1, no UI debug info
and unoptimized mg-ui. No release app or demo build was performed.

## Retained outputs and cleanup

`target/test-output/live-build/current/copy-0.{mod,live.env,start.sh}` is the
current CLI output for review. `prepared.mod` is the separate immutable candidate
used for the recorded native proof. Its earlier sidecars are superseded; use the
ones under `current/`. Source code, authored `source.mod`, compact reports and the
inspected picture are retained. No demo or shared Cargo output was removed.

Automatic approval review rejected deletion of 11 identified temporary output
paths with ?blocked by policy?; nothing was deleted and zero space was reclaimed.
`target/test-output/live-build/cleanup-blocked.json` records the exact paths,
ownership/purpose and 3,746,796 logical bytes. This is not a disk-allocation
measurement. These test copies remain pending a separately permitted cleanup;
no alternate deletion route was attempted. Active session history is untouched.
