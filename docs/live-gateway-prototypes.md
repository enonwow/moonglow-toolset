---
type: Reference
title: 'Live gateway feasibility prototypes'
description: Authored clean-client NWSync acceptance and private native chat interception measurements before gateway integration.
tags: [live, gateway, nwsync, chat, prototype]
generated: { by: codex/astra1, at: 2026-10-18T00:00:00Z }
sources: [live-ops-wire.md, live-game-performance.md]
---

# Live gateway feasibility prototypes

These prototypes test the resource and chat contracts independently of the gateway
HTTP/UI integration. They do not enable translation on existing modules.

## Clean-client NWSync

`nwsync::private_module_contents` publishes only the module's own resources,
retaining `Mod_HakList` byte-for-byte and omitting NSS, NCS, NDB and GIC. The
separate normal hak manifest resolves collisions in the module's hak order.
`NwSyncCache::fetch` accepts an exact lowercase SHA-1 and a streaming transport
callback. Repository paths are generated from that digest; SHA-1 is checked for
both manifest and decoded resources. Downloads, decoded sizes, total size and
entry count are bounded. Cache objects use hex-only names and are synced then
renamed atomically. Existing blobs are rechecked, including on resource reads.
The callback is where the later authenticated HTTPS client belongs; this
prototype's acceptance uses a local authored repository, without an HTTP service.

`open_module` creates a separate module folder, marks its files read-only and
returns a module whose save API refuses writes. `Module::stack_onto` mounts the
verified hak cache as `LayerClass::NwSync` instead of searching local hak folders.
Only validated portable resource names become files; scripts in an incoming
private module manifest are refused. The attached manifest represents one exact
resource generation; choosing/replacing generations and renderer invalidation
remain integration work.

Acceptance: `cargo +1.98.0 test -p mg-module --test nwsync_clean -j 2`, with
`CARGO_INCREMENTAL=0` and a nonexistent `NWN_ROOT`. On 2026-10-18 both tests passed
(0.07 s test execution). The authored fixture creates two tiny haks on the
publisher side, two manifests and an empty-install client. It proves a hak-only
resource resolves, the first hak wins collisions (including over the module),
reversing hak order reverses the winner, the IFO list survives, scripts are absent,
saves are refused, and tampering in a download or existing cache is rejected.
The second test covers digest/path rejection, declared sizes, compressed output
larger than its declared size, truncated manifests and unsafe module names.
Test-owned files are removed by the fixture. No game data is committed.

## Chat interception

Implementation and native measurements are in progress; no acceptance claim yet.
