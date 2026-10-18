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

## Module-only publishing

`nwsync::module_only_contents(module, drop_ncs)` never expands haks or rewrites
IFO bytes. NSS/NDB/GIC are always omitted; `drop_ncs` controls compiled scripts.
The Live publisher uses `true`. Authored fixtures check byte-for-byte IFO
preservation and deterministic manifests independent of input order.

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
resource generation. Authenticated downloads and atomic selection are implemented
by the [resource generation backend](live-resource-generations.md); renderer
invalidation remains GUI integration work.

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

The prototype is disabled unless the module local integer `MGLIVE_XL` is nonzero.
Each PC uses the string local `MGLIVE_LANG` (default `en`). The game snapshots
listeners and languages at capture time. Same-language listeners and listening
NPCs receive the original immediately; other listeners wait for one response on
`<prefix>.xl`. The request is published directly on `<prefix>.xl.req`, independent
of the event stream and periodic state report. Both server launch paths subscribe
to the reply channel. Pending requests are bounded to 128 slots, bound to the
boot and a sequence, and consumed before delivery. A full slot passes the original
through the native route. A module-owned `DelayCommand(0.9)` supplies the fallback.

`MGLIVE_XL_DELIVERING` guards the entire native send, so a callback from tell,
DM tell or an untargeted send cannot become another translation request or chat
event. Responses are rejected on a command prefix (including leading whitespace),
changed colour tags/emote delimiters, new line breaks or excessive length. The
original is delivered on rejection, a miss or expiry. Late and duplicate responses
cannot redeliver a consumed request. Script commands beginning with `/`, `!` or
characters in `MGLIVE_XL_COMMANDS` pass through without translation.

Acceptance runs through `tools/live/test-events.py --phase chat_translation` and
`--phase chat_translation_stress`, with the fake Redis translator in
`test_chat_translation.py`. It uses the existing private WSL server ownership,
scratch-user-directory and cleanup route. The fixture adapts PC enumeration and
DM identity only in its copied include, leaving production scripts unchanged.
Recipients are real listening NPCs; their **native OnConversation callbacks**
record capture-to-delivery microseconds. Tells call the same capture function
with an explicit NPC target, because an NPC has no connected player ID.

The channel table below is verified for those surrogates. A is a nearby English
listener; B a nearby Polish party member; C a German listener outside whisper
range; D a distant French listener; E a Polish party member in another area;
F a nearby Polish DM surrogate; N an ordinary nearby listening NPC.

| Channel | Native listener callbacks |
|---|---|
| Talk 1 / DM talk 17 | A, B, C, F, N |
| Whisper 3 / DM whisper 19 | A, B, F, N |
| Shout 2 / DM shout 18 | A, B, C, D, E, F, N |
| Party 6 / DM party 22 | B, E |
| Player-to-DM 14 / DM-to-DM 30 | F |
| Tell 4 / DM tell 20, target B | B |

The source is selected for its immediate original echo, but an NPC cannot receive
its own targeted chat through NWNX, so **connected sender echo is not verified**.
Tests also cover ordinary `SpeakString`, disabled interception, a native
untargeted send while the delivery guard is set, duplicate and late responses,
missing translator, hostile command/markup output, and a listener moving to
another area and changing language while the reply is pending. That listener
still receives exactly one line in its captured language. Native NPC delivery
is asserted independently of the return value of `NWNX_Chat_SendMessage`.

The original fixture's spawn script reset listening; clearing its event scripts
before registering the observer fixed the assay. A second native finding was
that NWScript's C-style carriage-return escape becomes the letter `r`; the guard
now uses byte 13 from the native ASCII table, with a regression for actual CR.

### Measurements (2026-10-18)

Final measured values are recorded below after the two owned runs. Each row uses
60 speech captures and 180 waiting listener deliveries (three language targets).
Two other listeners per line receive the original immediately. The stub answers
each request twice to exercise duplicate suppression. The stress configuration
is the existing **native** stress fixture: 1000 objects in five areas, six large
looks/inventories, 25 patrols, speech, damage and dialogue activity. It does not
include the separate HTTP/WebSocket client soak.

| Load | Stub delay | Captures/s | p50 | p95 | Max | Translated / original (waiting) |
|---|---:|---:|---:|---:|---:|---:|
| Idle | 0 ms | 18.66 | 11.8 ms | 12.3 ms | 16.3 ms | 180 / 0 |
| Idle | 200 ms | 18.71 | 212.5 ms | 214.3 ms | 218.2 ms | 180 / 0 |
| Idle | 600 ms | 18.67 | 612.1 ms | 615.4 ms | 616.2 ms | 180 / 0 |
| Stress | 0 ms | 14.57 | 13.3 ms | 15.1 ms | 16.2 ms | 180 / 0 |
| Stress | 200 ms | 14.62 | 210.2 ms | 475.0 ms | 496.5 ms | 180 / 0 |
| Stress | 600 ms | 14.45 | 610.5 ms | 869.8 ms | 901.8 ms | 177 / 3 |

For each load there are 900 measured listener callbacks: 540 waiting deliveries
and 360 immediate originals. Idle: 60% translated / 40% original overall; among
waiting listeners, 100% translated. Stress: 59.67% translated / 40.33% original
overall, and 99.44% translated among waiting listeners. The 600-ms stress row
returned 98.33% translations and 1.67% fallback originals. No listener duplicates
were observed. These ratios exclude unobservable sender echo.

With no translator answer, all five recipients received an original; maximum
observed capture-to-callback time was 910.5 ms idle and 908.3 ms under stress.
A 1200-ms reply likewise produced only the original, once (maximum
910.0 ms under stress). The delayed reply never added another line.

The native `DelayCommand` is a scheduled fallback, not a wall-clock guarantee.
The measured send rate is reported separately: scheduling a line every 50 ms
does not prove 20 captures/s. The 600-ms stress case can legitimately fall back
when engine scheduling exhausts the 900-ms response window; originals are counted
separately instead of treating every timely delivery as a translation.

### Reproduction and limits

1. Use Rust 1.98.0, `CARGO_INCREMENTAL=0` and a nonexistent `NWN_ROOT` for Rust
   checks. Build the guarded fixture with
   `cargo +1.98.0 test -p mg-live --test private_fixture --no-run -j 2` if needed.
2. In WSL, select that existing executable through `MGLIVE_FIXTURE_TEST` and run
   the two `tools/live/test-events.py` phases above. An existing `mg` compiler can
   alternatively run the small idle fixture through `MGLIVE_FIXTURE_MG`.
   `MGLIVE_FIXTURE_TEST` avoids requiring PowerShell 7 and does not build an app.
3. Keep the compact phase JSON files in `target/test-output/live-events/`.
   They bind module/compiler/server/plugin hashes, production scripts, the
   fixture adapter hash, measurements, assertions and owned-process termination.
   `xl-observations.json` contains the latest run's per-listener observations for
   diagnosis; it may be removed once the compact proof and results are retained.

Both surrogate server acceptances passed. No connected PC/DM clients were
available in this helper: actual player-ID tell echo, DM possession/client
identity, client rendering and parity with server-specific shout/party policy
remain **unverified**. No production module is enabled. This is not yet approval
to turn translation on for players. The next integration acceptance needs a
private connected-client fixture; the existing NPC fixture cannot establish
those claims. The stub uses ASCII authored text; translation quality and non-ASCII codec round trips are not established here. Authenticated HTTPS/cache transport,
whole-generation switching now belong to the [resource backend](live-resource-generations.md); renderer invalidation still needs GUI integration.

## Verification and retained evidence

The shared final gate uses Rust 1.98.0, `CARGO_INCREMENTAL=0`, a nonexistent
`NWN_ROOT`, and at most two Cargo jobs:

```text
cargo +1.98.0 test --workspace --exclude mg-corpus-tests --lib --bins --tests -j 2
cargo +1.98.0 clippy --workspace --all-targets -j 2 -- -D warnings
cargo +1.98.0 fmt --check
```

The shared workspace run containing all completed prototype source changes on
2026-10-18 passed: 1720 passed, zero failed, 24 ignored, including both clean-client
NWSync tests. Workspace Clippy with warnings denied and `fmt --check` both passed.
Native server tests run separately through the guarded helper; their two passing
acceptances are described above. Later concurrent gateway and desktop changes
require their own final verification; the prototype sources stayed unchanged.

At documentation closure, a newer shared-tree run failed to compile in-progress
`mg-live-link` tests (unresolved modules/dependencies), and Clippy rejected three
unreachable public methods in its new `state.rs`; formatting passed. Those
concurrent changes are outside the prototype files. Thus the earlier passing
run verifies the completed prototype sources, but does not establish a green
gate for the latest shared tree.

The reproducible sources and two compact native proof files are retained:
`target/test-output/live-events/chat_translation.json` and
`target/test-output/live-events/chat_translation_stress.json`. These ignored
proofs include native process cleanup; neither owned server nor Redis process
remained after the runs. Six owned intermediate state, log and observation files
were removed after the measurements were recorded (3,795,320 logical bytes).
Physical disk recovery is not attributed while concurrent workspace builds run.
Shared build outputs and the reusable fixture workset were preserved. The
authored NWSync acceptance removes its own temporary fixture directories.

