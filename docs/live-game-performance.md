---
type: Report
title: 'Live game and wire performance: measurement phase'
description: Reproducible private-engine and Rust replay baseline, observer limits, ranked candidates and separately measured optimization follow-ups.
tags: [live, performance, nwnx, redis, measurement]
generated: { by: codex/astra2, at: 2026-10-15 }
sources:
  - ../tools/live/measure_live.py
  - ../tools/live/measure_look_stress.py
  - ../tools/live/stress_run.py
  - ../tools/live/stress_analyze.py
  - ../crates/mg-live/src/measure.rs
  - https://nwnxee.github.io/unified/group__dotnet.html
---

# Game, wire and mg-live measurements

The initial phase below measured only. Authorized implementation results are
recorded separately under Optimization follow-up. Wire semantics, cadence,
player behavior and demo lifecycle remain unchanged. The separate GUI measurements
belong to `live-gui-performance.md` when that report is available.

## Fixture and reproducibility

Production game and mg-live sources were measured at `e6fe4cb`; the opt-in
helpers are committed in `9eb7ee1`. Source line references below include the
three-line test-module hook in `store.rs` from that helper commit.

The read-only source is
`<demo-source>/demo.mod`,
SHA-256 `4ff92afb1e650887e355d9f2a9cd8df5a00fc81563c9d07cc2ca51316ada8ddb`.
Its `demo_area` area has 218 authored objects: 37 creatures, 32 placeables,
one door, 147 waypoints and one store. Live sends the first three groups: 70
reportable rows, before any runtime creation/death/removal. Calling this a
218-row wire report would overstate the workload. The Rust benchmark also has
an explicitly synthetic 218-reportable-row stress case.

The measurement copies use the existing private harness: UDP 5126, Redis 6392,
prefix `mglive-events`, owned WSL scratch `~/mglive-events`,
`-publicserver 0`, random unprinted admin password, no player/DM password.
The patched Redis plugin is under `~/nwnx-redis-reconnect/Binaries`;
the engine is `~/server/bin/linux-x86/nwserver-linux`.
The three HAK inputs are read from the already extracted project fixture
`target/test-output/live-demo/user/hak`, copied once to private scratch and
hash-checked. No source module, real user folder, installation or demo runtime
is written. Retained `inputs.json` and `measure.json` bind source, generated
scripts, HAKs, compiler, engine and plugin hashes.

```powershell
wsl.exe -e env MGLIVE_MEASURE_ROW_TIMERS=0 python3 /mnt/c/Projects/moonglow-toolset-original/tools/live/test-events.py --phase measure
$env:MGLIVE_MEASURE_REPLAY = '1'
pwsh -NoProfile -File tools/live/check.ps1 -Phase 5 -Step measure-replay -Package mg-live -Filter measure_live_replay -Jobs 1 -NoUiDebug -UiUnoptimized
Remove-Item Env:MGLIVE_MEASURE_REPLAY
python tools/live/test_measure_live.py
```

The supplementary row-vs-array run used
`wsl.exe -e env MGLIVE_MEASURE_SECONDS=30 MGLIVE_MEASURE_STAGES=detail,one_look_surrogate,three_look_surrogates python3 /mnt/c/Projects/moonglow-toolset-original/tools/live/test-events.py --phase measure`.
Before this supplementary run, the small first-run `engine.json`, `measure.json`
and `inputs.json` observations were retained as `*-baseline.json`; the same
MOD/source/HAK working set was reused. The current generator reproduces the
recorded `mp_detail.nss` and `mp_tick.nss` hashes exactly in both timer modes.

For the unoptimized mg-live comparison, set `NWN_ROOT` to
`C:/Projects/moonglow-toolset-original/target/test-output/live-measure/no-native-game`,
`MGLIVE_MEASURE_REPLAY=1`, `MGLIVE_MEASURE_PROFILE=dev0`, `CARGO_BUILD_JOBS=1`, then
run `cargo +1.98.0 test -p mg-live --lib measure_live_replay --config profile.test.package.mg-live.opt-level=0 -j 1`.
This changes only the test crate's optimization level, not the running app.
Clear those process-local variables afterwards. The test records `rust-test1.json`
or `rust-dev0.json`; neither timing is a release build or an in-process trace
of the owner's running GUI.

The engine run takes about eleven minutes after compilation. Each stage has
three seconds of settling and sixty seconds of observation. Override the latter
with `MGLIVE_MEASURE_SECONDS=15..300` in the WSL environment. Tests use the main
target, one build job and Rust 1.98.0 through `check.ps1`; its missing NWN_ROOT
guard prevents any Windows engine startup. Neither application is built.

## Method and interpretation

- `mp_native` is production tick work with the existing diagnostic printed each
  tick; scheduling belongs to a shared private wrapper. `mp_tick`/`mp_detail`
  add section timers to private copies. `GetMicrosecondCounter` is used only
  within one execution. Counters and log construction outside the measurement
  window still add observer work; native versus detailed stages quantify it.
- Creature/placeable/door/item timings include the row's getters, JSON building
  and insertion into its area array. Effects, relations, common fields, text
  encoding and look timings are nested subsets, **not additional totals**.
  Native getters and every individual JSON allocation are not independently
  timed: wrapping every native operation would materially change this workload.
- `areas` includes scanning all 218 objects, row work and area assembly. `hash`
  includes the full-report size check or the delta pass. `body`, `events`,
  `dump`, `redis` and `clear` cover the envelope, event insertion, final JSON
  serialization, synchronous PUBLISH/result retrieval and acknowledged-buffer
  clearing. Logging the observation is outside `us`.
- `city_beat` is wrapped in the private module. Its inclusive time contains
  the synchronous `city_tick` dispatches. Turning that wrapper off suppresses
  future routine heartbeat dispatches, not already queued native actions,
  pathfinding, perception, stock AI, or other event scripts. CPU differences
  are a bounded A/B observation, not an exhaustive NPC cost attribution.
- CPU is process user+system seconds from `/proc/<owned-pid>/stat` divided by
  monotonic elapsed seconds: 100% means one logical core. It is not a lifetime
  average. The shared host remains busy with the owner's apps and peer builds;
  no affinity, priority or unrelated process settings are changed.
- A 50 ms DelayCommand observer records the maximum scheduling gap in each
  batch of twenty callbacks, using NWNX seconds+microseconds across calls.
  This is a scheduler observation, **not client FPS or a player-perception test**.
- One/three look surrogates call the real `MgLiveLook` on existing NPCs, leaving
  `pc=false`. These isolate serialization demand; they are not connected PCs,
  do not reproduce PC network traffic, and do not measure relation-list growth.
- Rust replay runs production `LiveStore::sync` on captured packet sequences,
  with one warm-up and twenty measured passes. Separately timed parse, text
  boundary, typed deserialization, delta merge and RESP decode isolate costs;
  setup cloning is outside each component timer. Event-ring and disk append
  assays use explicit 0/10/256-event workloads. The disk writer is a separate
  app thread; its latency is not added to synchronous report ingestion.

## Delta and look cadence, verified from code and read-only status

The running demo returned `delta=false` at sequence **13476**, report age
528 ms, with no resync pending. This is not a 30-tick baseline boundary, so it
confirms a legacy report at that instant. This was a read-only `live_status`
request; no Redis game command, start, stop or rebuild was issued. Its current
area had 29 creatures, 32 placeables and one door (62 rows after owner edits).
Minimal evidence: `target/test-output/live-measure/demo-status.json`.

Current automatic delta activation requires `JsonDump(areas)` to reach
**100,000 bytes on a full report**. Below that threshold, creatures arrive every
second; placeables/doors/items and PC looks arrive every fifth report. Once
delta is enabled, all rows and PC looks are rebuilt each tick, then hashed;
unchanged rows are omitted. A full baseline is sent every 30 ticks, on first
report, or following `resync`; a newly watched/revisited area also gets a full
area baseline. A boot change or missed delta needs an immediate baseline.

The production diagnostic prints only sequence 1 and multiples of 60. Every
such tick is full in both modes. `full=1` on those log lines alone says nothing
about the intervening packets.

A stable delta look can be omitted on the wire while still costing a native
ObjectToJson, pruning, serializing and hashing every second. If any other
field of that PC's row changes, the complete row including `look` is resent.
The current protocol does not separate a small motion delta from the look.
The 30-tick baseline is a recovery policy, not an engine requirement; no lower
or higher recommended interval is established without loss/reconnect and
state-staleness testing.

## Measurements

Host: Intel Core Ultra 7 265F, 20 WSL-visible logical CPUs, WSL2 kernel
6.6.87.2. Main run: 60 s per stage, **zero connected players throughout**.
The table uses median/p95 elapsed milliseconds and median UTF-8 wire bytes.
Only the first two rows have no detailed row probes. The detailed observer adds
about 6.2% to the median full tick and 6.2% to the median creature-only tick.
This is one sequential shared-host run, not a randomized isolated experiment.

| Workload / packet | n | Tick median / p95 ms | Median bytes |
| --- | ---: | ---: | ---: |
| Production work, legacy creature-only | 48 | 5.418 / 6.346 | 14,696 |
| Production work, legacy full | 12 | 8.759 / 9.611 | 22,097 |
| Detailed observer, legacy full | 12 | 9.300 / 11.822 | 22,097 |
| Forced delta, changed rows | 58 | 10.211 / 11.961 | 2,629 |
| Forced delta, full baseline | 2 | 16.515 / 17.377 | 22,149 |
| One 12,577-byte NPC look, legacy full | 12 | 19.573 / 21.764 | 34,725 |
| One NPC look, changed-row delta | 58 | 21.404 / 23.500 | 3,068 |
| One NPC look, delta full baseline | 2 | 32.885 / 32.969 | 34,742 |
| Three NPC looks, legacy full | 12 | 38.843 / 41.099 | 59,914 |
| 16 lasting VFX on each of 37 NPCs, legacy full | 12 | 11.319 / 12.248 | 24,469 |

Two baseline samples cannot establish a useful tail distribution; their p95 is
simply the larger sample. Rows/packets and individual observations are retained
in `engine-baseline.json`. Delta reduced average bytes from **16,198 to 3,232
per report (80.0%)**, while increasing per-tick work. Do not enable it globally
as a CPU optimization in its current form. Both runs used the live routines;
the changing rows come from the running module heartbeat and native actions.

### Tick breakdown

Median microseconds from the 60 s detailed no-look stage. Nested rows overlap
their parents; independently calculated medians need not sum to the median
total. The row-vs-array follow-up is recorded separately below.

| Section | Creature-only | Full | Production source |
| --- | ---: | ---: | --- |
| Complete area collection | 4,965 | 8,206 | `mglive_inc.nss:355`, `:389` |
| Creatures: build + append | 4,567 | 4,771 | `mglive_inc.nss:219`, `:355` |
| Placeables: build + append | 0 | 2,191 | `mglive_inc.nss:303`, `:355` |
| Door: build + append | 0 | 237 | `mglive_inc.nss:314`, `:328` |
| Ground items | 0 | 0 | No ground items in this area; path not exercised |
| Common fields, nested | 770 | 1,415 | `mglive_inc.nss:219` |
| Name/tag text conversion, nested | 280 | 510 | `mglive_text.nss:17` |
| Effect loops, nested | 191 | 201 | `mglive_inc.nss:254` |
| PC relation lists, nested, zero PCs | 208 | 216 | `mglive_inc.nss:277` |
| Size check (no delta) | 0 | 97 | `mglive_tick.nss:19` |
| Envelope through areas | 92 | 143 | `mglive_tick.nss:26` |
| Add events/dropped to envelope | 244 | 387 | `mglive_tick.nss:36` |
| Final JsonDump | 69 | 85 | `mglive_tick.nss:39` |
| Redis PUBLISH + result | 299 | 352 | `mglive_tick.nss:40` |
| Clear acknowledged buffers | 6 | 8 | `mglive_tick.nss:43` |

The no-look full report spends much more time collecting/building/copying JSON
than serializing its final bytes or publishing them. A door currently calls
ObjectToJson twice, for appearance and useability. Its entire 237 us cost is
small beside the creature and placeable collections.

With 16 lasting VFX on every creature, the effect-loop median rose to
**1,118 us** on full reports (1,085 us on creature-only reports). VFX rows
appear in the captured reports; this was a real effects workload. This modest
cost does not explain the one-look full-report increase.

### Looks and events

The one-look helper costs **3,700 us**, of which ObjectToJson is **725 us**;
the rest includes pruning, budgets, JSON copies and look hashing. The three-look
helper total is **10,626 us**, with **2,048 us** inside ObjectToJson. Full tick
growth is much larger: roughly +10.3 ms for one look and +29.5 ms for three,
relative to the detailed no-look full. Repeated copying outside the helper is
therefore a candidate that needs the row-vs-array follow-up, not an assumption
that native ObjectToJson alone consumes the increase.

Packet inspection: the stable look was sent on all **12** legacy full reports.
In the delta minute it was built **60** times but sent only **2** times. Of the
three NPC looks, one changed hashes because `AnimationTime`, `XOrientation`,
`XPosition`, `YOrientation` and `YPosition` changed; the other two were stable.
Those are not appearance changes. This is proven for the NPC ObjectToJson
surrogates; the exact connected-PC snapshot field set still needs a client.

The explicit burst assay ran after the lasting-effects stage:

| Events in report | Build + enqueue burst | Clear sent prefix | Whole report tick | Wire bytes |
| ---: | ---: | ---: | ---: | ---: |
| 10 | 352 us | 129 us | 11,183 us | 25,664 |
| 256 | 44,123 us | 36,918 us | 48,840 us | 54,391 |

These are one-shot stress samples, not a normal traffic rate. Enqueue time
occurs in the event-producing script, outside the report tick. The latter
includes clearing, so **do not add clear time to whole tick again**.
`mglive_evt.nss:29` repeatedly gets/appends/sets a module JSON array;
`:46` repeatedly deletes its first entry. A fast all-sent clear and a single
construction of any unsent tail are promising without changing delivery rules.

### NPC routines, server CPU and scheduling

| 60 s stage | Process CPU (% of one core) | 50 ms callback batch-max median / p95 (ms) |
| --- | ---: | ---: |
| Native Live + routines | 9.03 | 56.90 / 60.58 |
| Detailed Live + routines | 9.38 | 57.30 / 60.87 |
| Native Live, routine heartbeat suppressed | 9.43 | 57.23 / 60.84 |
| Live and routine heartbeat suppressed | 8.76 | 51.40 / 51.61 |
| Live suppressed, routines enabled | 8.89 | 51.49 / 51.97 |
| Delta + routines | 9.86 | 59.80 / 62.44 |
| One look, legacy | 9.85 | 57.58 / 71.51 |
| One look, delta | 11.36 | 71.93 / 75.80 |
| Three looks, legacy | 10.26 | 57.66 / 72.57 |

An already in-flight report appeared at the beginning of each Live-off window;
it is recorded, not silently discarded. The production-work tick sum is about
**0.61 core-percent averaged over its 60 s window**, while the process uses
9.03%. The paired CPU differences range from 0.14 to 0.67 percentage points,
showing why a single subtraction is not an exact attribution. Most of the
observed 9% persists with Live suppressed. The owner's lifetime CPU observation
with a connected player is not comparable to these fixed no-player windows.

The module's inclusive `city_beat.nss:2` costs median **509 us**, p95 **666 us**,
ten calls in sixty seconds, with 34 registered routine actors. It dispatches
`city_tick.nss:2` and the `city_modes` functions. That script time alone is about
0.009 core-percent. Disabling its dispatch does not disable native AI/pathing
already queued by the routines or other scripts in the six-area module.

The scheduler probe shows increased callback delay while reporting is enabled.
The 256-event burst produced a **107.5 ms** batch maximum; the three-look stage
reached **91.2 ms**. These prove scheduling stalls in the private server, not
that a connected player necessarily notices every 8–20 ms report. A client
motion/latency comparison remains necessary for the perceptual answer.

### Row-vs-array follow-up

Thirty seconds per stage, six full reports each. This adds a timer at the
beginning/end of `MgLiveObject` without changing expression/iteration order.
The residual is computed per sample before taking its median. It includes
array argument/insertion, row-return and probe/VM overhead, so it is not a
heap-allocation trace. Its growth with large looks isolates collection work
outside the object's own getters/JSON construction.

| Full report | Entire tick (ms) | Creature row construction (ms) | Creature collection residual (ms) | Placeable row construction / residual (ms) |
| --- | ---: | ---: | ---: | ---: |
| No look | 8.911 | 2.354 | 2.082 | 1.276 / 0.939 |
| One look | 19.985 | 6.344 | 6.968 | 1.357 / 0.907 |
| Three looks | 36.741 | 13.084 | 14.666 | 1.529 / 1.067 |

Sources: `mglive_inc.nss:219` (row build), `:363` (creature array insertion),
`:374` (area assembly). These explain why optimizing only ObjectToJson would
miss most of the added one-look cost. Delta baseline assembly has a second
row-by-row array construction in `mglive_inc.nss:441`: its hash/collection
phase is **6.802 ms without a look**, **13.094 ms with one**, compared with
1.545/2.303 ms on their ordinary deltas. A full baseline can potentially reuse
the original complete rows while still updating its hash cache.

### Rust store, bridge and logs

Latest measured medians/p95 in **microseconds**. `test1` is the ordinary
workspace test profile; `dev0` explicitly leaves mg-live unoptimized to
approximate that aspect of the debug app. Dependencies and other execution
conditions remain as described above; these are not release measurements.

| Packet/workload | Inclusive sync test1 median / p95 | Inclusive sync dev0 median / p95 |
| --- | ---: | ---: |
| Actual 70-row full | 337 / 476 | 593 / 763 |
| Legacy 37-creature partial | 214 / 287 | 373 / 492 |
| Changed-row delta, retained 70 rows | 100 / 124 | 184 / 230 |
| One look, full | 638 / 853 | 1,114 / 1,332 |
| One retained look, changed-row delta | 434 / 555 | 747 / 982 |
| Three looks, full | 1,406 / 1,947 | 2,378 / 2,966 |
| Synthetic 218 reportable rows, ~77 KB full | 1,166 / 1,469 | 2,372 / 2,983 |

Component assays below are independently timed and have different cache/setup
conditions; do not subtract or sum their medians as an exact decomposition.

| Component | 70-row full test1 / dev0 (us) | Synthetic 218-row full test1 / dev0 (us) |
| --- | ---: | ---: |
| JSON bytes to Value | 230 / 210 | 911 / 859 |
| Text-byte normalization | 16 / 26 | 50 / 92 |
| Value to typed request | 65 / 250 | 241 / 1,123 |
| RESP frame decode from memory | 1.9 / 5.0 | 4.5 / 13.8 |

`store.rs:1811` is the full sync path; `:1817` is text normalization and `:1818`
the second deserialization. `bridge.rs:217` reads RESP and `:227` calls sync on
its subscriber thread. Socket wait, GUI wake/render and lock contention with
GUI/API readers are excluded from the isolated replay.

One `merge_delta` costs **27.5/47.2 us** without a look and **158.4/265.3 us**
with one retained look (test1/dev0). It currently runs twice, for size validation
at `store.rs:1994` and application at `:2062`. `merge_delta` (`:2680`) first
clones the old area and then rebuilds its vectors; `:2060` additionally clones
the old creatures for derived events. These are measured candidates, although
the server-side JSON work dominates the present end-to-end budget.

The event ring's empty/10/256-event append assays cost **<0.1/12/328 us** in
test1 and **<0.1/20/565 us** in dev0. Deriving unchanged 37-creature state costs
**3/12 us**; changing all their combat flags costs **11/28 us**. These are
bounded ring operations, distinct from the game's expensive JSON buffer.

`logs.rs:44` appending 1/10/256 events costs median **487/896/951 us** in test1
and **302/329/729 us** in dev0, on this Windows volume. These are cached writes
with file open/close and no fsync. Filesystem/host variation dominates any
profile comparison. `logs.rs:103` runs the writer on its own once-per-second
thread; it is not synchronous work inside `LiveStore::sync`. An earlier replay
on the same host had 271 us for a no-look test1 full sync versus the final
337 us, illustrating shared-host variability; no narrow speedup claim relies
on the last decimal place.

## Ranked candidates for a later implementation phase

The gain column names measured work that could be reduced; these are engineering
estimates and upper bounds, **not measured speedups of an implemented change**.
Effort is relative developer time including tests and private engine proof.

| Rank | Candidate | Expected gain / measured budget | Risk | Effort |
| ---: | --- | --- | --- | --- |
| 1 | Avoid repeated JSON collection copies; reuse complete rows on delta baselines | Creature collection alone costs 2.1/7.0/14.7 ms for 0/1/3 looks; additional area and full-delta assembly also grows with looks. Benchmark bulk construction without changing parsed wire values. | Medium: byte/codepage fidelity, ordering, hash stability and complete/delta equivalence must hold. | 1–2 days |
| 2 | Clear acknowledged event prefixes once; then address repeated per-event array copies separately | Clear currently costs 36.9 ms at 256 events; enqueue costs 44.1 ms outside the tick. Fast all-sent clear should remove most clear work. | Low for all-sent fast path; medium for queue redesign. Retain unsent/reentrant events, sequence/dropped and zero-listener behavior. | Half a day for clear; 1–2 days for queue |
| 3 | Appearance-only identity and cached looks, with verified invalidation | ~3.7 ms per unchanged look build, every second in delta; reduce 12.6 KB look resends and nonvisual hash churn. Array copying still needs rank 1. | Medium/high: equipment/body/tint changes and external scripts must invalidate; bound stale-cache recovery. | 1–3 days |
| 4 | Spread remaining collection work over bounded frame-sized batches | Targets peak stalls rather than promising lower total CPU; begin with a measured 2–3 ms batch budget after ranks 1–3. | High: mixed-time snapshots, deleted objects, boot/resync, events and no overlapping jobs. | 2–3 days |
| 5 | Single delta merge and fewer retained-look clones; measure optimized mg-live builds | Two merges consume ~55/94 us without a look and ~317/531 us with one (test1/dev0); most normal sync is already below 1 ms. | Low/medium: validate size before mutation and preserve derived events. | Half to one day |
| 6 | Revisit delta activation/cadence after reducing construction cost | Already saves 80% wire bytes here, but current implementation raises game CPU/tick time; changing only the threshold is not a CPU fix. | Medium: reconnect/gap recovery and stale-object limits. | Half to one day of experiments |

Effects, Redis publication, the single door and `city_beat` are lower priorities
for this measured workload. Static metadata/name caching can be evaluated later;
it must retain external-script changes and the repaired mixed-encoding path.

## Limits and cleanup

Connected-player 1/several cases and whether a player feels a hitch require
private clients. NPC look surrogates are explicitly separate. NWNX documents
the main-loop callback as approximately/ideally 10 ms; an 8–20 ms synchronous
script can occupy roughly one or two such intervals, but this alone does not
prove visible stutter. Splitting a report into bounded delayed batches is
possible as a future design, with explicit consistency, object-deletion,
boot/resync and command-latency risks. It is not implemented here.
[NWNX main-loop documentation](https://nwnxee.github.io/unified/group__dotnet.html).

Task ledger: `target/test-output/live-measure/files.json`; session
`01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`. GUI-worker artifacts in this shared output
folder have independent ownership and are retained. Active conversation history
is untouched and requires a separate authorized retention action.

Validation: both private runs passed and all four owned child identities exited;
UDP 5126/Redis 6392 were idle afterwards. The five wrapped/adjacent routine sources
match their MOD resources after newline normalization; the fixture compiler now
refuses stale sources. Both probe variants reproduce their exact recorded source
hashes. Adding the source guard rebuilt the identical final MOD, SHA-256
`e0e4312ca3fdf270e1095a2814e855504e8d06c1fdc7f66f6891d6d32f6fe841`.
mg-live checks: 112 unit tests passed, four explicitly ignored existing tests;
three guarded fixture tests, three offline Python probe tests and all-target
Clippy with warnings denied passed. The opt-in replay also ran successfully in
both profiles. `moonglow.exe` and `mg.exe` still match their pre-task hashes.

Cleanup completed: **8,445,977 cumulative logical bytes** of own redundant
aggregate frames, empty output, Python bytecode, duplicated manifest details and
four temporary log-writing assays were removed or compacted. The log assays
reused one path; this is not a claim that 8.45 MB of net free disk space appeared.
Allocated size and net free-space change were not measured on the shared host.
Keep the current MOD/source working set, required HAK inputs, per-stage replay
packets, timing/identity summaries and minimal burst proof for review. No shared
Cargo cache, peer output, source module, original game asset or active session
history was removed. No artifact cleanup remains blocked; separate authorized
history retention and the connected-player/perception measurements remain open.

## Optimization follow-up

Implements [plan steps 2, 4, 5 and 6](live-optimization-plan.md) in that order,
one measured commit each. Tests continue to use only private UDP 5126/Redis 6392.
The same fixture/HAK working set is reused. Small labelled observations live in
`target/test-output/live-measure/runs/`; `ledger.json` records ownership.
Each run retains input/source hashes, engine/plugin identity and process closure.
Original baseline packet files remain available for identical Rust replays.

### Step 2: clear acknowledged event prefixes once

Before/after source: plan baseline `8bb7f78` versus the prefix-clear change in
`mglive_evt.nss`. Seven alternating 10/256-event bursts per run, after a 15 s
detailed no-look stage, with the same production measurement helper:

```powershell
wsl.exe -e env PYTHONDONTWRITEBYTECODE=1 MGLIVE_MEASURE_RUN=step2-before MGLIVE_MEASURE_SECONDS=15 MGLIVE_MEASURE_STAGES=detail MGLIVE_MEASURE_BURSTS=7 python3 /mnt/c/Projects/moonglow-toolset-original/tools/live/test-events.py --phase measure
# Run the identical command after the change with MGLIVE_MEASURE_RUN=step2-after.
```

| Events | Clear median before → after | Whole tick median before → after |
| ---: | ---: | ---: |
| 10 | 78 → 20 us | 8,681 → 8,884 us |
| 256 | 33,565 → 21 us | 42,940 → 9,397 us |

The ordinary small tick varies with host/game work; the burst improvement is
large and directly localized to clearing. Median 256-event payload size is
52,131 bytes on both sides. No wire fields/order semantics changed. The common
all-sent case empties the array once; a reentrant unsent suffix is assembled
once from native JsonDump fragments, with native quoting preserved.

Both native contract runs passed: empty/repeated clear, partial prefix,
events added after the sent snapshot, newest-256 overflow order/counter and
escaped text preservation. The new private `measure-events.nss` fixture is
never installed by the production launcher. Both owned process pairs exited.
Guarded mg-live tests passed (112 unit tests); no app build or demo operation.

### Step 4: prepare each delta once, borrow previous rows for events

The validated merged area is retained until every area passes size checks,
then applied once. Constructing it no longer clones the old area before
rebuilding its vectors. Derived events borrow the previous creature rows
before replacement, including legacy/full reports. Request validation,
acknowledgement ordering, gap handling and all-or-nothing area updates remain.

Identical original captured packet sequences, 20 measured passes after warm-up,
ordinary test1 profile; outputs `runs/step4-before/rust-test1.json` and
`runs/step4-after/rust-test1.json`. Run the original replay command with
`MGLIVE_MEASURE_REPLAY=1` and `MGLIVE_MEASURE_RUN=step4-before` / `step4-after`.

| Workload | Inclusive sync median before → after | Isolated one merge before → after |
| --- | ---: | ---: |
| Ordinary delta | 87.1 → 48.9 us | 24.0 → 14.2 us |
| Delta with retained look | 388.6 → 110.0 us | 142.6 → 71.7 us |
| Full, no look | 265.3 → 263.7 us | n/a |
| Full, three looks | 1,260.0 → 997.8 us | n/a |

The before/after private engine runs used 30 s each of `delta,one_look_delta`.
Their ordinary tick medians were 9.596 → 9.553 ms and 19.273 → 19.175 ms:
effectively unchanged, as expected for a Rust-only implementation change.
Native event contracts passed in both and owned processes exited. The after
run reused the exact hash-verified compiled fixture from `step4-before` via
`MGLIVE_MEASURE_REUSE=step4-before`, avoiding a redundant compile while another
worker held Cargo. Reuse verifies module/HAK/production/generated script hashes
and the successful prior module receipt; it never reuses stale source.

All 114 unit tests pass, including later-area overflow rejecting earlier
prepared updates/acks/events, object-kind movement, stable row order,
metadata preservation and derived combat events. All-target Clippy passes.
No `events.rs` API or query implementation was changed by this step.

### Step 5: construct report collections once

`MgLiveArea`/`MgLiveAreas` now accumulate complete native JsonDump fragments,
then parse each collection once. Delta construction similarly assembles the
hash map, changed rows, removed IDs and area list once; full deltas reuse the
complete rows instead of rebuilding a second array. The tick attaches `areas`
after its small envelope fields. User/game strings are never manually quoted.
This changes construction, not cadence, row hashing, schema or recovery.

Before source `13fadfa`; after source is this step's committed scripts.
Run the step 2 command with `MGLIVE_MEASURE_RUN=step5-before` / `step5-after`,
`MGLIVE_MEASURE_SECONDS=30`, and
`MGLIVE_MEASURE_STAGES=detail,delta,one_look_surrogate,one_look_delta,three_look_surrogates`
(omit `MGLIVE_MEASURE_BURSTS`). The original probes accept both append forms.
Both runs have zero PCs; the look cases remain NPC serialization surrogates.

| Packet/workload | Tick median before → after | Reduction |
| --- | ---: | ---: |
| Legacy creature-only | 5.452 → 3.646 ms | 33% |
| Legacy full, no look | 8.641 → 5.503 ms | 36% |
| Legacy full, one look | 18.097 → 9.454 ms | 48% |
| Legacy full, three looks | 34.966 → 18.118 ms | 48% |
| Ordinary delta, no look | 9.510 → 6.160 ms | 35% |
| Ordinary delta, one look | 19.040 → 10.440 ms | 45% |
| Delta baseline, no look | 13.604 → 6.524 ms | 52% |
| Delta baseline, one look | 29.515 → 11.760 ms | 60% |

Legacy full rows have six samples each; ordinary deltas have 29. Each delta
baseline row has only one sample, so it supports neither tail statistics nor
a precise expected speedup. Similar byte sizes differ slightly with NPC motion;
the equivalence gate is a native same-execution comparison, not two moving
worlds having byte-identical packets.

`measure-report-reference.nss` freezes the pre-change native collection code.
The private engine compares canonical JSON against it for full/partial area
reports, baseline/unchanged/changed deltas, removals, collection moves and a
missed-area baseline. The after proof additionally includes Polish text-byte
companions, quotes, backslashes and newlines. Both report/event contracts pass;
owned processes exited. New captured packets are also replayed through real
`LiveStore::sync` by setting `MGLIVE_MEASURE_INPUT_RUN=step5-after` together with
`MGLIVE_MEASURE_RUN=step5-after MGLIVE_MEASURE_REPLAY=1`; a detailed legacy
capture serves as the native packet input when no uninstrumented stage exists.

All 114 mg-live unit tests, new-packet replay, three offline probe contracts and
all-target Clippy passed. No application binary was built or demo process changed.

### Step 6: enqueue into bounded event slots

The module now holds at most 256 event JSON values in individual ring slots,
with a head/count. Enqueue replaces only one slot; the tick assembles an ordered
snapshot once. Sequence allocation, newest-256 overflow, cumulative `dropped`,
no-listener retention and clearing only acknowledged sequence prefixes remain
unchanged. Cleared slots are deleted so their JSON values do not linger.
These are private module locals, not new wire fields. Compile/deploy all scripts
using the include together on the next authorized server update.

Before source `6851326`; final after observations in `runs/step6-final`, with
the additional wraparound/slot-release checks. The first candidate measurement
is retained as the small `step6-after` timing/identity receipt. Commands match
step 2 with labels `step6-before` / `step6-final`: 15 s detail, seven alternating
10/256-event bursts. All tests run on the same owned private server allocation.

| Events | Enqueue median before → after | Tick median before → after | Clear inside tick before → after |
| ---: | ---: | ---: | ---: |
| 10 | 230 → 180 us | 5,451 → 5,559 us | 18 → 35 us |
| 256 | 35,508 → 3,133 us | 6,449 → 7,781 us | 21 → 292 us |

At 256 events, enqueue improves **91%**. Snapshot assembly and individual slot
release cost about **1.33 ms extra in the tick**; enqueue plus tick falls from
41.957 to 10.914 ms (**74%**). This sum describes work in two executions, not one
contiguous frame. Ten-event combined work is slightly higher (5.681 → 5.739 ms),
so this is a burst-stall improvement, not a claim that every workload is faster.
The first after run measured enqueue 3.249 ms and tick 7.741 ms, supporting the
same conclusion. Empty and ordinary object reporting still use step 5's path.

Native checks pass for empty/partial/repeated clears, later arrivals, overflow,
clearing a sent snapshot after its entire prefix was overwritten, ring wrap,
Polish byte companions and escaped text. All 256 slots read back null after
clearing. Before and after, two or more ticks with zero state subscribers retain
ten events; reconnect delivers them in sequence once, and the following report
is empty. The native full/delta reference comparison also remains green.
Only headless native behavior is claimed; connected-player perception remains
unmeasured. Owned process pairs exited after every run.

Final step 6 validation: 114 mg-live unit tests and all three offline probe tests
pass. A final hash-verified cached-fixture run (`step6-receipt`) also passes both
native contracts and verifies the clarified measurement compiler/fixture receipt.
Earlier receipts inherited the general harness's `prepare_private_events_fixture`
label; the executed route was `prepare_private_measure_fixture`, as recorded by
the check logs and input manifests. Cached reuse now explicitly inherits the
original compiler hash rather than rehashing a subsequently rebuilt executable.

Optimization-round cleanup: **2,615,027 cumulative logical bytes** removed:
three temporary log assays (790,480 bytes each, one reused path) and the first
step 6 candidate's superseded packet capture (243,587 bytes). Its small timing,
source and process receipts remain, alongside final before/after packets, original
baseline inputs, current fixture and dependencies. This is not net disk-space
recovery; allocated/free-space changes were not measured. No peer artifacts,
shared build caches, source modules or active conversation history were removed.

## Post-sixth-acceptance optimization (2026-10-15)

Baseline production source: `b249a4c`. The `9eb7ee1` measurement helpers now
recognize the NPC equipment cache: `npc_hits`, `npc_builds` and `npc_rebuild`
separate cached reads from rebuilds. The 0/1/3 cases mean **additional uncached
PC-path look surrogates**, still zero connected clients; all 37 NPCs retain
their production compact looks. No NPC look reporting was suppressed to make
the baseline smaller. The original 218 authored objects still produce 70
reportable rows. Private ports, process ownership and module workset are unchanged.

Reproduce the initial run with `MGLIVE_DEMO_SOURCE` set in WSL to
`<demo-source>`,
`MGLIVE_MEASURE_RUN=opt2-before MGLIVE_MEASURE_SECONDS=35
MGLIVE_MEASURE_ROW_TIMERS=0 MGLIVE_MEASURE_BURSTS=5` and
`MGLIVE_MEASURE_STAGES=native,detail,delta,one_look_surrogate,one_look_delta,three_look_surrogates`
on the existing `test-events.py --phase measure` command. Engine receipts and
packets are in `target/test-output/live-measure/runs/opt2-before/`. An external
owner/coordinator release GUI compilation overlapped this run; it was neither
started nor stopped by this worker. These are preliminary regression measurements;
optimization claims require a subsequent paired comparison without that build.

| Initial workload | Ordinary delta median / p95 | Full median bytes |
| --- | ---: | ---: |
| Production, no extra look | 41.599 / 60.494 ms | 310,578 |
| Detailed, no extra look | 43.457 / 60.440 ms | 310,603 |
| One uncached look | 45.786 / 63.865 ms | 315,384 |
| Three uncached looks | 56.578 / 73.457 ms | 325,033 |

The NPC equipment fix adds about **288,304 bytes of look/hash fields** to a full
report (37 looks), versus 592 bytes of portrait fields and a 129-byte faction
catalogue. The enlarged full report automatically crosses the 100,000-byte
delta threshold. A no-extra-look detailed tick spends median 26.158 ms building
areas, 15.568 ms hashing/filtering, and 5.568 ms inside the nested look path.
Ordinary ticks hit all 37 NPC caches; rebuilds are occasional spikes, not the
main steady cost. The old optimized no-look ordinary delta was 6.160 ms;
that old workload did not report NPC equipment, so it is not an equivalent
performance target. 256-event enqueue remains bounded (3.712 ms median); its
75.297 ms report tick includes the new three-look workload. Native event,
full/delta equivalence and no-subscriber retention contracts passed.

Rust measurement: set `MGLIVE_MEASURE_REPLAY=1`,
`MGLIVE_MEASURE_INPUT_RUN=opt2-before`, `MGLIVE_MEASURE_RUN=opt2-before`, then run
`tools/live/check.ps1 -Phase 5 -Step opt2-before -Package mg-live
-Filter measure_live_ -Jobs 1 -NoUiDebug -UiUnoptimized`. The additional
`rust-dispatch.json` assay measures both production command entrypoints, valid
idempotent retries and invalid fields, plus UI-style revision/cursor polling.
There are no CI timing thresholds or actual queued game execution in that assay.

The ownership ledger for this round is `target/test-output/live-opt2/ledger.json`;
only small named observation sets are added, with one reused module/HAK workset.
No application build, demo lifecycle operation, client-render proof or
connected-player performance claim is part of this run.

### Immutable operation catalogues and shared API gate

Quiet paired Rust replay uses `runs/opt2-rust-before-quiet` and
`runs/opt2-rust-after`, both reading exactly `opt2-before` packets, serial tests
(`RUST_TEST_THREADS=1`). The before run executes the already-built baseline
mg-live test binary; its SHA-256 is in `live-opt2/rust-before-binary.json`.
`operations.rs::table` now lazily retains one immutable catalogue per each of
8 policy combinations. Both API entrypoints share the scope/schema gate;
legacy disabled-command precedence, API quota ordering and direct-command
semantic validation remain unchanged. The latter also protects UI callers and
is not duplicate schema validation.

| Dispatch, 500 warmed calls | Before median | After median |
| --- | ---: | ---: |
| Legacy valid / invalid | 831.7 / 624.0 ?s | 2.2 / 1.3 ?s |
| Named call valid / invalid | 497.8 / 489.9 ?s | 2.1 / 1.3 ?s |

All eight cached catalogues equal freshly built catalogues; existing API,
scope, refusal, campaign-example and exact OpenAPI tests pass. No wire schema
or operation was added or removed. These are in-process dispatch times, not
HTTP/network latency. The full guarded run passed 123 library tests (4 ignored)
and the two opt-in fixture guards; real fixture preparation is separate.

### Cheap local revision polling

`LiveStore::revision()` reads `(boot, seq)` under one lock without constructing
status JSON; `event_cursor()` returns the log revision without filtering/cloning
an event. Same-run medians: status 3.6 ?s vs revision 0.1 ?s, limit-one events
1.9 ?s vs cursor below the timer's 0.1 ?s resolution (not zero-cost). The getters
are pure: GUI callers still run their existing `expire(now)` poll. Tests cover
boot changes, report revisions and log lines arriving between reports. Public
status/events endpoints retain their existing expiry and scope behavior.

### Delta preflight plans and moved rows

`store_delta.rs` replaces the old deep-cloned prepared area with a plan of old
and new row indices. Every area's retained counts pass before acknowledgements,
events or rows are applied. Derived events inspect borrowed projected rows;
application moves each owned row once. Unchanged NPC look JSON is not copied.
This separates the merge algorithm from the oversized `LiveStore::sync` body.

Quiet replay of identical packets: native-area delta `store.sync` median
**2,796.8 -> 996.8 ?s**; one-extra-look delta 3,091.6 -> 1,301.4 ?s;
three-extra-look delta 3,827.7 -> 907.6 ?s. The inclusive production entrypoint
is the comparison boundary. The isolated merge assay prepares owned inputs
outside its timer and drops its result inside the timer on both versions;
it is diagnostic rather than the acceptance metric.

The existing size-overflow/atomicity, gaps/resync, order, kind-change, metadata,
equipment-look and derived-event tests pass. A frozen pre-change merge oracle
also checks update/removal precedence and unchanged look allocation identity.
The latter is evidence of ownership reuse, not an allocator timing assertion.

### Retired-feature audit

The current mg-live/MCP/native catalogue has no remaining shader, texture-replace
or coloured-effect handlers. The only obsolete diagnostic was the private
fixture's `SHADER_UNIFORM_` declaration filter: removed, and its general excerpt
renamed `native-script-api.txt` (wire documentation updated). Retired names in
absence tests and owned-fixture/HAK migration guards are intentional safeguards,
not reachable feature code. No game asset or peer GUI file was deleted.

### Report validation boundary

`store_report.rs::SyncRequest::parse` contains the exact 142 pre-lock parsing and
validation lines formerly inside `LiveStore::sync`; the store method now starts
with one validated request. A byte-for-byte extraction check is recorded in
`live-opt2/report-extraction.json`. This is a readability change, not a speed
claim: limits, error text, text normalization and ordering are unchanged. The
full guarded suite and captured-look reader pass after extraction.

### Native collections in place

`mglive_inc.nss:490` now assembles fresh area/collection JSON with native in-place
inserts. It no longer dumps each row, concatenates strings, then parses them
back. The delta wire, full cadence, hashes and fields are unchanged by this step.
The frozen oracle in `measure-report-reference.nss` uses the preceding string
builders (`b249a4c`); its independent delta oracle remains from `93f90bc`.

Serial private runs `opt2-native-before-quiet` -> `opt2-inplace` (35 s/stage,
5 bursts of each size, no worker Cargo/GPU timing overlap):

| Ordinary tick | Before median | After median |
| --- | ---: | ---: |
| Native, no extra look | 45.107 ms | 39.647 ms |
| Detailed, no extra look | 48.472 ms | 38.261 ms |
| One extra uncached look | 55.882 ms | 45.037 ms |
| Three extra uncached looks | 67.146 ms | 62.241 ms |

No-extra-look area assembly fell 29.621 -> 18.219 ms. Tail latency did not improve
consistently (native p95 64.203 -> 71.026 ms; three-look p95 78.770 -> 93.376 ms),
and total server CPU varied: this is a steady-work reduction, not a proven hitch
or whole-server CPU fix. Enqueuing 256 events remains ~4 ms (3.940/4.408 ms in
these two runs); this step changes collection construction, not the event ring.

The coordinator then requested an urgent [look decoder repair](live-events-wire.md#decoder-boundary-repair-2026-10-15).
Its text pruning changes look sizes, so those new packets are not compared with
the table above. `opt2-look-repair` instead invokes both collection builders on
the **same corrected state**, 12 separate VM calls with alternating order:
median **27.392 ->16.852 ms**, identical 277,214-byte area JSON in every pair.
`measure-collections.nss` warms caches before the pair; timers exclude equality
checks and serialization. Twelve pairs in one VM invocation exceeded the engine
instruction budget, so that rejected helper was split into separate calls.

Full/partial/delta equality, kind moves, disappearing IDs, skipped-area baselines,
ring wrap/overflow/ack and reconnect retention pass in the engine. Reconnect
validation now identifies its ten tagged events: the city can independently
speak while disconnected (observed 11 total events); newly arriving speech must
not be mistaken for redelivery. Every previously acknowledged sequence remains
forbidden. Private server/Redis cleanup receipts confirm only owned processes
exited; the demo was not operated.

### Current limits and next decision

Current hotspots: `mglive_inc.nss:219` (uncached look construction),`:308` (NPC
cache/refresh),`:490` (collections),`:551` (delta hashing). The pre-repair detailed
sample spends 17.246 ms hashing; portraits contribute 592 bytes and the faction
catalogue 129 bytes to a full report. Ten captured chat events contribute a median
~101 bytes (max 117) of recipient/audience/position fields each, measured by compact
UTF-8 JSON reserialization. No dialogue-node event occurred in this fixture:
connected conversation and player cost remain unmeasured.

Step 9 appearance-identity caching is **not added**. Ordinary ticks already hit
all 37 NPC look caches; the uncached PC path retains dynamic/unknown GFF fields,
so a cache keyed only by appearance would change report behavior. Three extra
looks still cost roughly 12 ms before pruning. A safe follow-up needs explicit
field/invalidation semantics. Also investigate the recorded cold OnLoad instruction
overflow before expanding per-tick work; ordinary final refreshes did not overflow.
No claim is made about connected-client rendering, real 1/3-player load or perceived
frame hitches. `store_delta.rs:14/61` and `operations.rs:49` are the measured Rust
merge/catalogue changes; the wire, quota and scope contracts remain covered.

Final validation: guarded mg-live 123 library tests passed, 4 ignored; the actual
captured-look reader and 2 opt-in fixture guards passed. All-target mg-live Clippy
`-D warnings`, scoped `cargo +1.98.0 fmt -p mg-live -p moonglow-mcp --check`, and
3 Python measurement-helper tests passed. Final replay (`opt2-rust-final`) also
passed after the exact validation extraction; its diagnostic native delta sync
median is 0.915 ms. The quiet paired comparison above remains the acceptance
measurement. No mg-ui or application binary was built by this worker.

### Closeout and retained evidence

Completed source commits: `d0196cc` (measurement refresh), `a208775` (catalogue),
`58a330e` (revision getters), `7952029` (move-only deltas), `331218c` (retired
fixture diagnostic), `0e60415` (coordinator's urgent look repair), `47ef2ad`
(validation extraction), `0feac15` (native collections). Astra1's independent GUI
work and decoder defence are outside these performance claims.

Cleanup is recorded in `target/test-output/live-opt2/ledger.json`: eight
superseded pre-repair packet captures removed (17,663,029 logical bytes), four
throwaway log assays removed by the replay helper (3,161,920 bytes), and two
one-use cleanup manifests consolidated (2,684 bytes): **20,827,633 logical bytes**
removed cumulatively. Allocated disk usage and net free-space change were not
measured, so no disk-space gain is attributed against concurrent builds.

Retained: original `opt2-before` replay inputs; corrected `opt2-look-repair`
packets/Unicode probe; small timing, source/plugin/module-hash, binary-hash and
owned-process receipts; one current private fixture workset and its required
HAKs. `opt2-native-before-quiet` and `opt2-inplace` now retain summaries/receipts,
not redundant packet bodies. No shared target/cache, peer artifact, source module,
real NWN folder or installed game was deleted. No temporary target/build tree
was created; the shared compiled helpers remain needed by ongoing work.

Session `01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a` history remains untouched at
the agent's local Codex session file
(130,063,759 logical bytes at closeout inspection, still active). Its retention
must be handled separately after this session under an approved application
mechanism; no history deletion was authorized or performed here.

### G1 cold-start instruction budget

The formerly recorded cold-start TMI is resolved. The stack was
`mp_load -> mglive_load -> mglive_tick -> mp_native`: the first full report
serialized all 37 cold NPC looks in a single VM budget. The module-load
observer reported only 100 instructions before Live, with the normal limit
1,048,576. A partial cache survived the failed script and obscured the defect
on subsequent reports.

`MgLiveLooksReady` in `mglive_inc.nss` now checks the same watched-area selection
as reporting. It schedules at most four `mglive_look` invocations, each with a
fresh VM budget, and the tick resumes after 0.1 seconds. This covers initial,
equipment/signature and 30-tick expiry refreshes. The tick advances its sequence,
consumes resync and publishes only after all current looks are ready. Destroyed
objects and changed areas are re-enumerated; there is no stale engine iterator
or pending counter. This bounds appearance work, not arbitrary unrelated
module scripts, and preserves report/delta fields.

Reproduction (private UDP 5126 / Redis 6392, existing owned `~/mglive-events`):

```sh
MGLIVE_DEMO_SOURCE='<demo-source>' \
MGLIVE_MEASURE_RUN=g1-cold MGLIVE_MEASURE_COLD=1 PYTHONDONTWRITEBYTECODE=1 \
python3 tools/live/test-events.py --phase measure
```

This mode uses the production tick rather than the observer wrapper. On
2026-10-15: first seq1, continuous seq1..35, 37 complete NPC looks, 70 supported
report rows in the 218-object native area; all 37 looks were explicitly expired
together and re-reported. **Zero TMI**, unchanged instruction limit. Cold load
logged at02:02:56, first report02:02:57 (timestamp resolution one second),
277,518 bytes; the warmed report itself took39,722us. This is a correctness
proof, not a new quiet CPU/tail-latency comparison. No player was connected.

`target/test-output/live-measure/runs/g1-cold/` retains `inputs.json`,
`measure.json` (owned process cleanup), `engine.json`, `cold-reports.json` and
`look-parse.json`. The guarded `native_look_reports_parse` test with
`MGLIVE_LOOK_PROOF_RUN=g1-cold` checks those native looks using the actual GFF
reader and writer, in default/CP1250. One fixture workset and HAK set were
reused; superseded same-run outputs were overwritten, not copied.

### Large-look and routine VM regression (2026-10-15)

Coordinator fix `c51cec4` removes the recursive unknown-text fallback from
`MgLiveLookNoText`. That fallback in `0e60415` was not safe for large PC
snapshots; the earlier small-look decoder check missed it. Native code now
cleans a fixed list of root/equipment keys; GUI defence `93dc6bc` handles
remaining prose. This regression adds no production look transformation.

Run from the repository in WSL, reusing the private owned fixture and ports:

```sh
MGLIVE_DEMO_SOURCE='<demo-source>' \
MGLIVE_MEASURE_RUN=look-vm-regression MGLIVE_LOOK_STRESS=1 PYTHONDONTWRITEBYTECODE=1 \
python3 tools/live/test-events.py --phase measure
```

`measure_look_stress.py` creates an NPC with 200 inventory items and two equipped
items. Its native `ObjectToJson` is **515,345 bytes**. The actual `MgLiveLook`
returns **11,223 bytes**, retains both visible items and costs **13,958
instructions** at the unchanged 1,048,576 limit. A second native probe includes
1,000 unknown list rows with 20 numeric fields each plus unknown prose: the
fixed cleaner costs **570 instructions**, retaining the unknown list. This
specifically rejects reintroducing a recursive scan. Native JSON in-place edits
can share children: the synthetic Unicode negative control takes an explicit
deep copy before pruning its independent equipment sample.

Only the private fixture's PC-look selector accepts a marked NPC as well as a
real PC. The tick then calls the uncached PC look path on this large object;
normal NPCs retain their production cache. Three 18-second stages enable,
pause and resume reporting. Reports and commands continue, with no unexpected
TMI. This is a **PC-sized NPC surrogate**, not a connected-player/render proof.
Pausing reports leaves event hooks registered; it is not a complete Live-off
benchmark. Captured native reports are decoded/serialized by the real GFF
reader in default and CP1250 with the guarded test:

```powershell
$env:MGLIVE_LOOK_PROOF_RUN='look-vm-regression'
./tools/live/check.ps1 -Phase 5 -Step look-vm-regression -Package mg-live `
  -Filter native_look_reports_parse -Jobs 1 -NoUiDebug -UiUnoptimized
```

The private `measure-vm*.nss` probes use native `run_script`. Synchronous
descendants of `ExecuteScriptChunk` returned zero from the NWNX instruction
counter on this engine, so that route cannot measure this budget. Native
`ExecuteScript` does share its parent's counter: parent **81,130**, child
**81,154**, parent after child **81,183**. `DelayCommand` starts a fresh callback
at **12**. A deliberate infinite-loop negative control raises TMI in
`mp_vmfail` and its immediate `mglive_ps` caller. Its previously scheduled
callback still starts at **12**; later reports and `city_beat` continue with
fresh budgets. It does not poison later independent engine callbacks.

`C7_FOLLOW1`, `C7_FOLLOW2` and `C6_ACTOR` are creature **tags**, not script names.
Read-only source is `<demo-source>/src/`:
`city_beat.nss` loops through registered actors using `ExecuteScript("city_tick",o)`;
`city_tick.nss` calls the mode functions; `city_modes.nss:43` maps C6 to patrol
and the following C7 function handles the leader/followers. Thus all actors in
one heartbeat share a budget with each other. In on/off/on reporting stages,
C6 consumed **269–338** instructions per call and each follower **330**;
entries were below 7,000. No routine TMI reproduced. The tick/look paths do not
call these routine scripts. Native chat/event hooks may run inside other
scripts and add to that caller's budget; these tests do not exclude such
overhead in an unobserved connected-player case.

Conclusion: the old recursive Live look overflow is established and fixed by
the coordinator. Sharing that spent counter with a later delayed heartbeat is
**not** supported by the engine proof. The cause of the owner's simultaneous
routine errors remains unverified: retain the exact failing script stack,
timestamp, object tag, module/script version and connected-player conditions
before attributing them to Live. The coordinator mailbox requests that excerpt;
the running demo was not touched.

Minimal evidence is under `target/test-output/live-measure/runs/look-vm-regression/`:
`inputs.json` (source/module/plugin hashes), `measure.json` (engine and owned
process cleanup), `engine.json` (counters/routine samples/negative-control stack),
`one_look_surrogate-reports.json`, `look-text-probe.json` and `look-parse.json`.
The task ledger is `target/test-output/live-regression-patrol/ledger.json`;
the existing fixture/HAK workset is reused, never cloned for each attempt.

Final guarded checks: 125 mg-live library tests passed, four opt-in tests
ignored; the separate native-look gate parsed 111 NPC and two uncached PC-path
looks in each codepage, including the corrected Unicode negative control.
All-target mg-live Clippy `-D warnings`, scoped Rust fmt and three offline
measurement-helper tests passed. No app build or demo lifecycle action ran.

## Chat response latency (2026-10-15)

Private test only: UDP 5126, Redis 6392, owned `~/mglive-events`; the running
demo was neither queried nor rebuilt/restarted. The same tiny authored module,
NWNX plugins and real Windows `LiveService` HTTPS/Redis bridge were used in both
runs. `mget_latency.nss` emits 20 native NPC talk messages spaced by 0.413 s,
covering different positions in the one-second report period. The baseline
uses the immutable chat hook from `0664e4c` and one-second API polling; the
current run uses immediate `.event` delivery and `wait_ms:30000`. Both await
`live_wait`, require exactly 20 unique native sequences, and wait another 1.5 s
to ensure the report fallback cannot duplicate the last fast message.

| Measured interval, ms | Baseline median / p95 / max | Current median / p95 / max |
| --- | --- | --- |
| Native pre-chat marker receipt -> store ingestion | 585 / 1045 / 1111 | 0 / 1 / 3 |
| Store ingestion -> HTTPS long-poll/poll result | 548 / 888 / 888 | 0 / 1 / 1 |
| Native pre-chat marker receipt -> HTTPS result | 1133.691 / 1668.370 / 1923.412 | 0.770 / 0.971 / 4.251 |

These are 20-sample private-server measurements, not an estimate of LLM
inference time or the owner's 218-object/connected-player workload. Millisecond
store timestamps quantize sub-millisecond results to zero. The total interval
uses `Instant`, not a wall clock. A fixture-only Redis marker is published
immediately before native chat and timestamped on Windows, like store ingestion;
therefore the table excludes the marker's first Redis/network hop. Warm Redis
PING RTT was 0.171..0.258 ms before and 0.109..0.181 ms after (first cold probes
1.882 / 2.025 ms). Independent consumer scheduling also bounds fine-grained
interpretation: this proves removal of tick/poll waiting, not a universal
sub-millisecond guarantee.

Rejected measurement method: direct subtraction of NWNX's WSL system-clock
`time` and Windows receive time gave negative latencies. Even Redis TIME
calibration at startup or per API receipt was unreliable across a buffered
interval: independent clock slew changed offsets by seconds during one run.
Those trial timings are not evidence. The final marker method keeps both
measurement ends on the same host and stores raw per-message samples.

Useful sources: `tools/live/fixtures/events/mget_latency.nss`,
`crates/mg-live/examples/live_ops/chat_latency.rs`, `tools/live/test-events.py`
(`chat_latency`), `mglive_chat.nss` (immediate publish), `store_events.rs`
(wait/context), `events.rs` (exact bounded deduplication), and `api.rs`
(persistent TLS plus conditional catalogue dispatch). Command enqueue/publish
is unchanged. The fallback state wire is retained; fast packets never make an
old state appear fresh. NPC speech uses the real native hook; native PC/possessed
identity and five-neighbour enrichment are covered by store tests. There was
no connected PC or LLM in this headless proof.

Reproduce from the main repository, with no concurrent Cargo build:

```powershell
./tools/live/check.ps1 -Phase 5 -Step chat-latency-client -ProofExample live_ops_proof -Jobs 1 -NoUiDebug -UiUnoptimized
wsl -e env MGLIVE_LATENCY_BASELINE=1 python3 tools/live/test-events.py --phase chat_latency
Copy-Item target/test-output/live-events/chat_latency.json target/test-output/live-chat-latency/before.json
wsl -e python3 tools/live/test-events.py --phase chat_latency
Copy-Item target/test-output/live-events/chat_latency.json target/test-output/live-chat-latency/after.json
./tools/live/check.ps1 -Phase 5 -Step chat-latency-tests -Package mg-live -Jobs 1 -NoUiDebug -UiUnoptimized
./tools/live/check.ps1 -Phase 5 -Step chat-latency-lint -Package mg-live -Lint -Jobs 1 -NoUiDebug -UiUnoptimized
cargo +1.98.0 fmt -p mg-live --check
```

The harness recompiles only its owned fixture through a guarded library test;
it never builds an app. Receipts preserve module/plugin/source hashes and owned
process exit evidence. The working set is reused across attempts. Minimal final
evidence: `target/test-output/live-chat-latency/{before,after,ledger}.json`.
Final guarded checks passed: 134 library tests (four opt-in tests ignored),
three integration/doc gates, all-target Clippy `-D warnings`, and scoped fmt.
This includes empty-boot-change and shutdown wakeups, scope filtering, exact
deduplication, history-loss signalling and real persistent TLS tests.

HTTP support for the cooperating MCP worker: explicit `Connection: keep-alive`
permits sequential requests with a 32-second idle lifetime, TCP_NODELAY and
bounded shutdown. Each request re-authenticates and checks Allow LLM. Describe
returns a scoped catalogue ETag; a conditional call with an obsolete If-Match
returns 412 with the current tag before quota/dispatch. No uncertain-write retry
is introduced. The MCP worker's separate fake-agent shell benchmark is recorded
in the agent protocol; it is not mixed into the engine table above.

Closeout retention (session `01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`): after
verifying process exits and receipt hashes, removed five own redundant captures
(two full state captures, two empty log captures, one duplicate after receipt),
**295,013 logical bytes**. Kept before/after receipts, the native `pc_reply.json`,
final gate logs, source and the one reusable fixture workset. No shared build
cache or other task's files were removed. Allocated disk savings and free-space
change were not measured or attributed on the shared disk. The small ledger is
`target/test-output/live-chat-latency/ledger.json`; it also records the active
Codex history path/size (136,383,100 logical bytes at closeout). That history is
untouched and registered for separate retention review, not deletion during
this session. No approved automatic history-cleanup mechanism was used.

## Stable appearance hashes (audit H1, 2026-10-15)

Private UDP 5126 / Redis 6392, owned `~/mglive-events`, 70 reported objects (37 creatures,33 other objects); city source
and HAK hashes recorded in `target/test-output/live-measure/runs/h1-walk-{before,after}/inputs.json`.
The three walking queue actors `C15_ACTOR`, `C15_EXTRA1`, `C15_EXTRA2` exercise
the uncached PC serialization path; **no connected PC or client rendering is claimed**.
Each sample is 40 seconds; 39 packets, 22 moving creatures. The immutable
before include is `8efc1a0a628a9fb5049fa38ef198c92331eae1ca`.

| Measure | Before | After |
| --- | ---: | ---: |
| False appearance cache invalidations after initial look | 42 | 0 |
| Look payloads / bytes across sample | 248 / 1,919,947 | 37 / 168,868 |
| Report bytes, median / mean | 42,567 / 52,622 | 2,508 / 7,722 |
| Detailed delta tick, median / p95 us | 36,107 / 43,666 | 26,256 / 33,591 |
| Full tick us (one sample only) | 41,691 | 34,608 |

The warmed native NPC cache still records34 hits and **zero rebuilds per tick**
in both samples; the three explicit PC-path surrogates remain uncached and are
serialized each tick. H1 removes42 false downstream look-cache invalidations and
redundant wire payloads; it does not claim to eliminate those PC-path builds.


`MgLiveLookFields` in `mglive_inc.nss` retains model/part/colour fields, GFF
metadata, EE wide twins and the five visible equipment slots for both look
paths. It drops position/orientation, animation timing, combat/HP and other
nonappearance state before hashing. It scans only the root and those five
item structs; it never traverses an arbitrary snapshot tree. The observed false
changes were X/Y position/orientation, ZPosition, AnimationDay and AnimationTime.
`MgLiveDeltaAreas` hashes the complete row but omits an unchanged look from a
moving delta row; `store_delta.rs` moves the retained look allocation when the
same object has exactly the same non-null lookHash. Full/resync reports still
carry every look, so a new consumer or gap can recover without an appearance cache.

Native `measure-look-identity.nss` runs on the actor: turning preserves the
hash, changing appearance changes it, and restoring appearance restores it.
Both look paths parse and serialize with `mg_gff::from_json` under default and
cp1250 (34 NPC + 3 uncached surrogate looks each); no TMI in either timed run.
Guarded mg-live:135 tests plus native-look/doc integration gates pass; four
explicit opt-in tests remain ignored. A unit test proves exact-hash reuse moves
the allocation and rejects different/missing hashes. The frozen native delta
oracle is compared after rehydrating unchanged omitted looks.

Reproduce from the repository in WSL (the source module is read-only):

```sh
MGLIVE_DEMO_SOURCE='<demo-source>' \
MGLIVE_MEASURE_RUN=h1-walk-after MGLIVE_MEASURE_STAGES=three_look_surrogates \
MGLIVE_WALKING_LOOKS=1 MGLIVE_LOOK_IDENTITY_CHECK=1 MGLIVE_MEASURE_SECONDS=40 \
python3 tools/live/test-events.py --phase measure
```

For before, use `MGLIVE_MEASURE_RUN=h1-walk-before`, omit the identity check,
and set `MGLIVE_MEASURE_INC_REV=8efc1a0a628a9fb5049fa38ef198c92331eae1ca`.
The same reusable fixture is rebuilt; only compact receipts/report samples vary.
`measure_look_hashes.py <report-file>` independently summarizes traffic/churn.
Set `MGLIVE_LOOK_PROOF_RUN=h1-walk-after` for the guarded mg-live test gate.
Evidence retained: inputs, engine/ownership receipt, report sequence and GFF
parse counts in the two run directories. Initial stationary samples are not
used as evidence of churn. No demo process, app binary or installed asset changed.

## Full-scale instruction guard (audit H3, 2026-10-15)

The strict private assay passes a cold start and subsequent full/delta reports
with **223 reported objects**, three NPC surrogates each carrying200 inventory
items and all five visible equipment slots, and256 factions in the loaded
private `repute.fac`. Raw ObjectToJson sizes529,674/531,002/531,734bytes become
5,756-byte looks. These are connected-PC-sized serialization surrogates, not
connected players. The fixture edits only its output MOD; source MOD/HAKs and
the demo remain untouched.

A native loop applies256 damage effects, invoking the real Damage plugin
callback256 times. The next report contains256 events. Subsequent attack,
spell, projectile and death events arrive; chat, a native script probe, gold,
patrol stop and a timed custom animation also execute. Each large inventory
returns61 rows with truncation; locals return the bounded128-row result.
Oversized captures refuse with `invalid`. A legal141,562-byte capture passes
the native GFF validator and spawn_json, followed by inventory readback.

| Measured stack | Calls | Maximum cumulative instructions | Remaining of1,048,576 |
| --- | ---: | ---: | ---: |
| Tick wrapper (includes preflight + native tick) | 303 | 543,134 | 505,442 |
| Pub/sub command dispatcher, including capture validation | 86 | 505,619 | 542,957 |
| Native256-damage burst | 5 fixture entries | 336,738 | 711,838 |
| Damage collector inside burst/other combat | 273 | 336,480 | 712,096 |
| Deferred look builder | 691 | 55,856 | 992,720 |
| Cold Live module-load chain, including256 factions | 1 | 51,651 | 996,925 |
| Chat | 32 | 4,061 | 1,044,515 |

Counts include nested native calls; **do not add these rows together**. The
observer adds a fixed small instruction overhead. No budget override is used,
no deliberate TMI negative control runs in this phase, and both native logs
are checked for `TOO MANY INSTRUCTIONS`. All exercised entries complete.
`look_reports.rs` parses the retained full-scale snapshots through the actual
GFF reader under default and cp1250. No production fix was required here.

Coverage is explicit: every instrumented production main is eligible for a
receipt, but this run observes load/evload/tick/look/pubsub/chat/attack/damage/
spell/projectile/gold/probe and the listed command/include paths. It does **not**
claim exhaustive validation of every operation branch, actual PC join/leave,
NUI clicks or dialogue-node callbacks at this scale. Those require a connected
client or their dedicated dialogue fixture; the existing per-operation proofs
remain separate. The game may take substantial wall time at this stress scale;
passing the instruction guard is not a smooth-frame claim.

Reproduce in WSL:

```sh
MGLIVE_DEMO_SOURCE='<demo-source>' \
MGLIVE_MEASURE_RUN=h3-full-scale MGLIVE_FULL_SCALE=1 \
python3 tools/live/test-events.py --phase measure
```

`measure_full_scale.py`, `measure-full-scale.nss` and the opt-in
`measure_fixture.rs` build retain source/fixture/plugin hashes, ownership and
cleanup receipts in `target/test-output/live-measure/runs/h3-full-scale/`.
Keep `engine.json`, `inputs.json`, `measure.json`, `full-scale-reports.json`,
and `look-parse.json`; they contain the exact counts and all observed headroom.
Set `MGLIVE_LOOK_PROOF_RUN=h3-full-scale` for the guarded native-look parse test.
The old large-look helper now identifies its declared surrogate by ID, since
H1 deliberately removed CombatRoundData from all look payloads.

### Durable identity enqueue cost (H4)

32 paired in-memory/durable enqueues on the local data drive: median10/740us,
p9535/1248us, maximum36/1265us. Each durable ID is appended and flushed before
queue publication; identical in-process retries avoid another write. This cost
is intentionally paid for restart safety. [Storage contract and reproduction](agent-protocol.md#durable-command-ids-audit-h4-2026-10-15).

## Audit H final native guard and handoff (2026-10-15)

After H6/H8 report extensions and H15 authorship (`1e75d9e`), reran the exact
H3 assay in `target/test-output/live-measure/runs/h3-full-scale-final/`, reusing
the same native module/HAK/scratch workset. The earlier compact H3 receipts are
the single comparison point; no second full asset/build tree was made.
Both runs:223 objects, three uncached PC-sized NPC surrogates (529,674?531,734
raw ObjectToJson bytes,200 inventory rows and five visible slots each),256
factions,256 native damage callbacks, bounded inventory/locals, rejected large
capture and141,562-byte legal capture/spawn. No TMI in either native log.

| Cumulative native stack | Earlier H3 maximum | Final maximum | Final remaining instructions |
| --- | ---: | ---: | ---: |
| Tick wrapper | 543,134 | 638,614 | 409,962 |
| Pub/sub command dispatcher | 505,619 | 505,741 | 542,835 |
| Burst fixture | 336,738 | 363,954 | 684,622 |
| Damage collector | 336,480 | 363,696 | 684,880 |
| Deferred look builder | 55,856 | 55,856 | 992,720 |
| Cold module-load chain | 51,651 | 51,651 | 996,925 |

The larger report catalogue increases tick work; this is a measured regression
of95,480 instructions (17.6%) against the pre-extension H3 maximum, not a claimed
optimization. It remains below the unchanged1,048,576 limit. Nested stack counts
are cumulative and must not be summed. This stress test does not establish smooth
client frames or genuine connected-PC/NUI/interactive-dialogue behavior.

Reproduce the H3 command above with `MGLIVE_MEASURE_RUN=h3-full-scale-final`.
Then set `MGLIVE_LOOK_PROOF_RUN=h3-full-scale-final` and run:

```powershell
tools/live/check.ps1 -Phase 5 -Step audit-h-final-readers -Package mg-live -Jobs 1 -NoUiDebug -UiUnoptimized
tools/live/check.ps1 -Phase 5 -Step audit-h-final-lint -Package mg-live -Lint -Jobs 1 -NoUiDebug -UiUnoptimized
tools/live/check.ps1 -Phase 5 -Step audit-h-portable -Package moonglow-mcp -Filter portable_sources_and_independent_dependencies -Jobs 1 -NoUiDebug -UiUnoptimized
```

Final core gate:156 unit tests passed, four opt-in assays ignored; the native look
reader was explicitly enabled and parsed602 NPC snapshots and9 PC-path snapshots
**per codepage** (default/cp1250), zero errors. The other two fixture gates passed
without their opt-in engine preparation flags. All-target mg-live Clippy with
`-D warnings`, owned Rustfmt checks and MCP portability pass. Canonical document
file links resolve. No app or demo rebuild/start/stop occurred; every owned
private server/Redis child is recorded exited in its phase receipt.

H6 atmosphere, H8 world spawns, H11 automatic dialogue capture and H15 attribution
have separate final native receipts under `target/test-output/live-events/`:
`atmosphere`, `spawn_world`, `capture_dialog`, `attribution` (`.json` and
`-state.json`). Contracts and native limits live in [operations](live-ops-wire.md),
[events](live-events-wire.md) and [agent protocol](agent-protocol.md). In particular,
encounter destruction is explicitly disabled after a negative native proof;
sound iteration exposes only Live-created sounds; H11 covers base-MOD conversations,
not HAK-only/one-liner dialogue; arbitrary later autonomous AI has no causal author.
These are test results and known limits, not owner/client acceptance.

Ownership/cleanup: session `01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a`, compact ledger
and exact removal receipt in `target/test-output/live-audit-h/{ledger,cleanup}.json`.
Removed **3,795,746 logical bytes** of this audit's superseded initial H1 sample,
one-time staging patch, reproducible API declaration dump and own helper bytecode.
The first recursive cleanup request was automatically blocked; explicit verified
file removals and removal of the empty directory completed safely. Allocated disk
and net free-space change were not measured or attributed to this task. Current
native evidence, required source inputs, the reused private workset and shared
Cargo outputs remain. No peer files or active Codex history were removed.
The active history the agent's local Codex session file
is recorded for a separate, explicitly approved retention action after the session;
no approved history-deletion mechanism was invoked here.


## Session stress: preparation and scope (2026-10-15)

The S1-S3 fixture is `tools/live/test-events.py --phase stress`, with native
workload scripts in `tools/live/fixtures/stress/`, the authored five-area
population in `crates/mg-live/tests/support/stress_fixture.rs`, the real service
host in `crates/mg-live/tests/stress_service.rs`, and Windows HTTPS/MCP agents in
`tools/live/stress_clients.py`. It reuses the owned private `events.mod` and WSL
`~/mglive-events` workset, UDP 5126 / Redis 6392; the running demo is excluded.

The fixture contains 250 creatures and 750 placeables across five 100 x 100 m areas,
200 objects each. Six NPC surrogates execute the uncached PC look serialization
branch, each with five visible equipment slots and 200 carried items. They remain
NPCs: this is **zero connected players**, not a six-player/client-rendering proof.
Twenty-five actors patrol continuously. A native combat pair and repeated
50 / 128 / 256 damage callbacks run alongside two NPC lines per 50 ms scheduled callback
(actual rate is measured), periodic instrumented dialogue, two HTTPS speech
clients (one requires the fixture DM's draft approval) and a real MCP watch client.
No LLM inference latency is included. A ten-second Redis outage occurs halfway
through the timed interval; the native server must retain its PID/boot.

An initial unmodified cold start could not reach this workload: `mglive_tick`
hit the 1,048,576 instruction ceiling twice and consumed a sequence before its
first full snapshot completed. `cold-before/failure.txt` and the compact receipt
under `target/test-output/live-stress/` preserve this negative proof. The bounded
report pipeline (`mglive_report.nss`, `mglive_rpt.nss`) builds one area per fresh
VM entry, retaining the existing report fields and at most 256 events per packet.
The short proof reached all 1,000 rows with no TMI; the completed long run is recorded below. This bound is per area: it does not establish that 1,000 objects
in a **single** area fit. `DelayCommand(0)` gives a new instruction budget, not a
guarantee of a separate rendered frame. Aggregate `tick_us` sums work across
entries; per-entry maxima and a 50 ms scheduling-gap observer are reported separately.

The original 9eb7ee1 detailed timing helper still uses a sequential component
observer (`tools/live/fixtures/measure-report.nss`). Its native stage now runs the
actual bounded production pipeline. Detailed inclusive component timings must
not be mistaken for production scheduling or summed with their child timings.

Reproduce from the main tree (PowerShell, after selecting an unused run label):

```powershell
wsl -e env MGLIVE_STRESS_SECONDS=1800 MGLIVE_STRESS_RUN=baseline MGLIVE_WINDOWS_PYTHON=/mnt/c/Python312/python.exe python3 tools/live/test-events.py --phase stress
```

Set the Windows Python executable path for the local machine. Fixture compilation
uses guarded `check.ps1`, +1.98.0, Jobs 1, the main target, and no app build. The
compiled service test runs with `NWN_ROOT` pointing to a nonexistent directory;
it never starts a Windows server. `manifest.json` binds module/source hashes,
while the phase receipt records engine/compiler/plugin hashes. Timing starts only
after setup and agent readiness, excluding Cargo/fixture preparation. Profiles
are ordinary `profile.test` opt-level 1, not the owner's release GUI.

Each run writes compressed raw envelopes `{at_ms,channel,data}` to
`replay.jsonl.gz`, plus real draft/decision sidecars including the approved native
ID. The capture has a 128 MiB uncompressed cap; native logs have a 64 MiB per-file guard.
No decompressed full capture or asset copy is needed. `stress_analyze.py` streams
it and extracts at most 100 state packets for the existing Rust ingestion assay
(`MGLIVE_MEASURE_SESSION=<run>`). These bounded samples are temporary benchmark
inputs and can be recreated from the retained gzip.

Measurement limits: marker-to-HTTPS starts when a separate native Redis marker
arrives on Windows, using the same monotonic clock as MCP output; it excludes
player networking and model inference. The two measurement subscribers count
as Redis listeners, so a positive PUBLISH count is not proof that Moonglow heard
that packet. Native capture, MCP observations and completed command effects are
reported separately. The service includes API, bridge, store and log writer CPU;
it has no GUI/rendering or configured persistent assistant memory book. Frame
observer gaps are server scheduling evidence, not client FPS. The replay goes to
Astra1 for a separate GUI soak.


### First 30-minute baseline

The timed baseline completed 1,800 seconds (1,802.9 including final drain), with
5,796 confirmed agent commands, 5,796 corresponding spoken lines, zero duplicate
agent lines and zero request errors. The complete 100 MiB-class raw trace compresses
to 7,002,691 bytes. It includes 28 moving creatures, 74 dialogue lines, 121 damage
waves and 48,534 generated short NPC lines (26.9/s over the timed duration).
Chat strings here are short ASCII markers, not maximum-length conversation text.

| Baseline measurement | p50 | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Native aggregate tick work, **retained log window only** |332.173 ms|421.320 ms|1,208.049 ms|
| 50 ms native callback scheduling gap, same retained window |54.726 ms|332.137 ms|1,255.848 ms|
| Report arrival interval, complete capture |1,008 ms|1,642 ms|10,994 ms|
| Full report interval |37.096 s|40.354 s|43.900 s|
| Full report bytes |826,226|828,753|889,571|
| Delta report bytes |21,449|46,414|87,575|
| HTTPS command enqueue |1.142 ms|4.474 ms|67.779 ms|
| Command result confirmation |989.322 ms|1,302.694 ms|22,388.980 ms|
| Native marker arrival to MCP event output |1.417 ms|2.064 ms|4,906.349 ms|

The retained native log window contains954 tick samples; NWN cycles four2.5MB
files and overwrote the beginning before the original end-only parser ran. **No
whole-run native-error or tick-maximum claim is made from this baseline.** The
measurement fix polls all rolling slots incrementally into a compact gzip and
has a regression for old-run exclusion, partial lines, slot reuse and duplicate
polls. The final soak uses that collector. The complete wire trace is unaffected.

The baseline observed80,028 unique native events and3,321 missing game sequence
numbers; the native cumulative buffer-discard counter reached7,706. These are
different measures: immediate chat can already have been delivered when its
buffered copy is discarded. The capture saw48,441 of48,534 generated NPC marker
lines. Damage observations include the combat pair, so they are not a direct
one-to-one comparison to the13,490 explicitly generated damage callbacks.
The ten-second Redis outage preserved game PID/boot; the first captured state
returned382ms after Redis restarted. This is capture recovery, not proof that
every subscriber received the first recovery packet.

Native CPU averaged42.43% of one core. The real test-profile service (API, bridge,
store and log writer together) averaged6.28%; private bytes grew from12.5MB during
startup to61.8MB, with63.6MB peak and54.9MB at five minutes. Command-identity/owner
retention grows with accepted IDs by design; this is not yet a flat-memory claim
for an indefinite session. No GUI or persistent assistant memory book is included.
Diagnostic compilation overlapped portions of the native run; timings are shared
workstation observations, not isolated release-build benchmarks.

The existing Rust assay replayed100 captured packets,21 rounds (first discarded).
Store sync including parse, text normalization, validation, merge and events:
full p50/p95 20.996/24.998ms, delta0.720/1.676ms. These are test opt-level1 numbers,
not release GUI CPU. GFF look validation parsed976 NPC and24 uncached PC-path
snapshots **per codepage** (default/cp1250), with no errors. Receipts are
`baseline/{native,analysis,clients,rust-test1,look-parse,engine,manifest}.json`
under `target/test-output/live-stress/`.


### S3: retain a bounded burst backlog

`mglive_evt.nss` now separates the2,048-entry storage capacity from the unchanged
256-event wire cap. A300-second repeat uses the same five-area workload, plus its
mid-run ten-second Redis restart. The first300seconds of the old baseline had
354 native sequence numbers still absent after final drain and970 buffer drops;
the new run has **zero gaps after drain and zero drops**, including its outage.
The full300second repeat confirmed928 agent commands, zero duplicate or missing
agent speech and zero request errors. The two initial fixture speech counters did
not produce chat-hook rows when started synchronously inside `run_chunk`; a deferred call from the same chunk still omitted them. Final setup instead uses
ordinary `run_script` commands and explicitly requires both initial lines before
starting the timed interval. This is a fixture startup correction, not a claim
about arbitrary NWNX calls from anonymous chunks.

The independently exercised native overflow contract retains2,048 of2,400 events
with352 drops and sends at most256 per report. Increasing capacity does not
change `seq`, `dropped`, report fields or duplicate handling. Baseline/repeat
comparison receipts are `baseline/first-300.json` and `backlog/first-300.json`;
the baseline outage occurs later, whereas this repeat includes the outage.
This is a **loss reduction**, not a CPU-speed claim: repeat aggregate tick
p50/p95/max350.561/402.942/629.562ms, native CPU44.09% of one core, service6.39%.
The retained baseline timing window and shared-workstation noise do not support
a small CPU-difference claim. The complete rolling collector observed zero TMI
or script errors in this repeat. Expected plugin connection errors during the
intentional outage are separate from script failures; final collection counts them.

The stress collector also counts NWNX plugin errors separately from VM failures,
redacts credential-related diagnostic lines and bounds retained examples. Its
pure regression covers Windows CRLF as well as slot reuse. `stress_analyze.py`
reproduces the first-300-second comparison from the first native `ST:` speech,
then consults the **complete** capture to exclude buffered events that arrive
after that window. Replaying both retained runs reproduced the exact comparison
counts above. Starting a final run requires observing `ST:1` and `ST:2` first.

### S3: spread periodic NPC look refreshes

`MgLiveLookRefreshAt` in `tools/live/nwn/mglive_inc.nss` assigns each object one
of 29 deterministic expiry phases. A preflight rebuild sets a deadline 2-30
report ticks ahead, strictly beyond the upcoming report. Appearance/equipment
signature changes still rebuild immediately. This avoids a cold crowd expiring
in one group while preserving the previous maximum refresh age.

The final repeat ran the same five-area workload for 1,800.008 seconds, with
8.2 seconds of drain/visual tail. A separate native `equipment_look` regression
then passed equip, unequip and external equipment-change detection. Both native
look variants still parse through `mg_gff` in default and cp1250 codepages
(976 NPC and 24 uncached PC-path samples per codepage, zero errors).

| Arrival measurement | Baseline, 30 min | Backlog only, 5 min | Staggered, 30 min |
| --- | ---: | ---: | ---: |
| Contiguous report gap p50 |1,008 ms|1,007 ms|1,252 ms|
| Contiguous report gap p95 |1,632 ms|1,315 ms|1,432 ms|
| Contiguous report gap max |9,026 ms|7,403 ms|2,553 ms|
| Full-report interval p50 |37.096 s|36.795 s|37.888 s|

A contiguous pair has adjacent report sequence numbers; the deliberately lost
Redis interval is reported separately. The maximum gap fell 65.5% from the
backlog-only repeat (71.7% from the original baseline). The median rises because
rebuild work is now distributed across ordinary reports. **This is a reduction
in long freshness gaps, not a claim of faster median reports or higher FPS.**
Full snapshots still occur every 30 report cycles, rather than every exact
30 wall-clock seconds under this load. The native per-area assembly bound is
unchanged. The legacy detailed timing instrument also accepts the new refresh
expression, and its instrumentation-anchor check passes.

The final complete collector saw aggregate tick p50/p95 326.337/372.078 ms,
maximum 1,446.877 ms, and zero TMI/script failures. Shared-workstation timing and
the earlier baseline's truncated native log prevent a small CPU-speed claim.
The game/wire/API measurements and remaining transport limitation follow below.

### Final session stress result and supported limits

The private final run completed **1,800.008 timed seconds**. This establishes a
headless functional bound for **five areas with 200 reported objects each**
(250 creatures, 750 placeables), six equipped 522 KB raw-look NPC surrogates,
25 patrols, a combat pair, 123 damage waves of 50/128/256 callbacks, 27.35 generated
NPC speech lines/s, native conversation capture, and three concurrent agents.
It processed 84,635 unique native events (about 47/s overall), including 82 dialogue
lines, and preserved the native PID/boot through a ten-second Redis restart.
There were zero connected PCs. Player networking, player consent branches,
`reply_to_player` on an actual PC, LLM inference, client FPS, and 1,000 objects in
one area are **not established** by this surrogate run. These are measured
limits, not owner acceptance of gameplay smoothness or GUI rendering.

| Final measurement | p50 | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Native aggregate tick work |326.337 ms|372.078 ms|1,446.877 ms|
| 50 ms callback scheduling gap |55.511 ms|335.313 ms|1,523.830 ms|
| Report arrival, including outage |1,252 ms|1,433 ms|11,722 ms|
| Full report bytes |830,004|841,410|850,555|
| Delta report bytes |25,642|37,855|83,847|
| HTTPS command enqueue |1.186 ms|4.465 ms|31.750 ms|
| Result confirmation through `live_wait` |1,245.824 ms|1,431.834 ms|19,206.207 ms|
| Native marker arrival to MCP output |1.468 ms|2.111 ms|1,106.860 ms|

The callback probe is a scheduling probe, not a frame-rate measurement. Even
with the instruction overflow fixed, this heavy workload can delay callbacks
for hundreds of milliseconds. `DelayCommand(0)` does not prove that the separate
area jobs land in different rendered frames. The 19.2-second command maximum
includes the Redis outage/reconnect interval.

All 5,672 requested agent speech effects were seen by the real MCP watch, with
zero duplicate agent lines and zero API errors. The sampler completed 5,669
`live_wait` calls; three final waits were cancelled by harness shutdown after
their spoken effects had already been observed. There were 2,836 real draft
approvals and zero approval refusals. Marker latency has 48,793 matched samples;
it does not imply that every generated line reached that subscriber.

Native CPU averaged 41.37% of one core (`getconf CLK_TCK` verified100 ticks/s
on this host; new helper runs record the value explicitly). Native RSS started/ended at
148,520/148,204 KiB, with 149,352 KiB peak. The Windows service averaged 6.84% of
one core, including bridge, store, API and log writer. Its private bytes were
12,038,144 initially, 50,827,264 after five minutes, 57,647,104 at the end and
60,170,240 at peak. These include bounded retained command identities/owners;
30 minutes does not prove a flat indefinite-session memory curve. The service
uses test opt-level 1, not release. Other diagnostic compilation/smokes overlapped
parts of the native run, so small CPU differences are not attributed to the change.

The replay contains 12,000 look rows / 26,182,656 look JSON bytes, mostly on the
48 full packets. The 6 large surrogates exercise uncached PC conversion; ordinary
NPCs use the phased cache. The unchanged full-only look resend policy still
accounts for substantial bandwidth. Actual look payloads were validated through
`mg_gff::from_json`, not only accepted as JSON.

The 100-packet Rust replay (21 rounds, first discarded) measured store sync
including parse, normalization, validation, merge and events at **22.232/23.761 ms
full p50/p95**, and **0.955/1.424 ms delta**. Full JSON parse alone was
10.229/11.593 ms; delta parse 0.277/0.408 ms. A single area's delta merge was
0.254/0.386 ms (five areas per packet). These inclusive values must not be summed
with their child timers. `final/rust-test1.json` retains all component distributions.

#### Instruction budgets and native diagnostics

The collector followed every rotating native log slot from startup through
shutdown: 1,439 tick samples and 24,598 callback samples. There were **zero TMI
or script failures**. The following entry wrappers record the remaining shared
VM stack budget; nested wall times overlap and must not be added together.

| Exercised script path | Calls | Maximum instructions consumed | Minimum headroom of 1,048,576 |
| --- | ---: | ---: | ---: |
|`mglive_evload`|1|2,092|1,046,484|
|`mglive_tick`|5,075|108,293|940,283|
|`mglive_load`|1|3,918|1,044,658|
|`mglive_look`|12,655|55,751|992,825|
|`mglive_rpt`|8,644|328,825|719,751|
|`st_setup`|7|77,205|971,371|
|`mglive_ps`|5,686|77,361|971,215|
|`mglive_chat`|54,906|3,449|1,045,127|
|`st_say`|49,234|3,484|1,045,092|
|`st_drive`|24,618|232,020|816,556|
|`st_frame`|24,601|1,098|1,047,478|
|`mglive_damage`|14,200|231,909|816,667|
|`st_wave`|123|231,957|816,619|
|`mglive_attack`|986|747|1,047,829|

The largest report entry used 328,825 instructions, leaving 719,751 (68.6%).
Hot entry points are `MgLiveReportStep`/`MgLiveReportPublish` in
`tools/live/nwn/mglive_report.nss`, `MgLiveObject`/`MgLiveLook` in
`mglive_inc.nss`, and the burst enqueue in `mglive_evt.nss`. Maximum single report
entry wall time was 352.491 ms. This table covers the paths actually invoked by
the scenario, not every possible Live operation or every module script.

The plugin console was **not error-free**: 568 Redis-plugin error lines and
560 Core missing-return-value lines were counted. Retained examples show failed
Redis connects and attempts to read unavailable results. Those are consistent
with the forced outage; the compact diagnostic collector did not retain each
error's timestamp, so it cannot assign every line to that interval. Counts and
bounded redacted examples are in `final/native.json`.

#### Delivery is not a Redis subscriber count

Historical pub/sub finding; the [durable Streams follow-up](live-mode-redis.md#durable-event-queue-2026-10-16)
now passes the same private restart scenario with8,902/8,902 events, zero losses
and zero duplicates. The original648-event gap below remains the baseline.

The native observer saw all 49,234 generated marker lines, zero native sequence
gaps and zero game-buffer drops. A separate comparison with the real store's
current-boot JSONL found **648 native events unlogged around reconnect** (440 chat,
139 patrol, 58 damage, 11 attack), plus 12 in the last two capture seconds, when
the logger shuts down without a final flush. There were zero duplicate native
rows in the store logs. The baseline/backlog-only runs had 434/523 interior
unlogged events, respectively. These counts are separate from native buffer loss.

The current bridge subscriber backs off 1/2/4/8 seconds up to 10 seconds;
the measurement observer reconnects sooner. `MgLiveReportPublish` clears the sent
buffer when Redis says **any** subscriber heard the publication. That permits the
observer to receive and clear backlog before Moonglow resubscribes. Faster retries
could narrow the window but cannot provide delivery guarantees. A per-consumer
acknowledgement or durable replay protocol is required for that claim. No such
protocol change or unmeasured retry tweak is smuggled into this performance patch.

`stress_analyze.py` compares native IDs with current-boot log IDs, records shutdown
boundary counts separately, and writes a compact, source-hashed
`store-native-seqs.json.gz`. It can repeat the calculation from that receipt after
the disposable service logs are removed. Its regression covers late buffered
arrival, duplicate log rows, an unrelated older boot and receipt-only reproduction.

#### Ranked changes and next candidates

| Priority | Change | Measured gain / expected opportunity | Risk | Effort |
| --- | --- | --- | --- | --- |
| Done 1 | Bound report assembly by area | Cold 1,000-object TMI -> all five areas reported; 68.6% worst observed stack headroom | Medium: asynchronous snapshot timing | Medium |
| Done 2 | Keep 2,048 events behind the 256-row wire cap | First 300s native holes354 ->0, buffer drops970 ->0; overflow remains explicitly counted | Low: bounded extra module locals | Small |
| Done 3 | Phase NPC refresh deadlines | Worst contiguous gap7.403 ->2.553s; higher median gap is explicit | Low: maximum age and dirty-equipment path retained | Small |
| Done 4 (2026-10-16) | Redis Streams, per-instance acknowledgement and local replay | 8,902/8,902 events through a ten-second Redis restart; zero loss/duplicates | Requires Redis persistence and bounded queue capacity; [proof](live-mode-redis.md#durable-event-queue-2026-10-16) | Complete |
| Next 2 | Reduce repeated native look/ObjectToJson work with a verified appearance identity | Targets the dominant326ms report workload and26.2MB look resend total; safe gain not yet measured | High: missing equipment/body-part invalidations can show stale looks | Medium/large |
| Next 3 | Spread costly area work across actual frames / bounded object batches | Targets335ms p95 callback delay and352ms maximum single report entry; freshness tradeoff required | Medium/high: sampling consistency and added report latency | Medium |
| Next 4 | Reduce full-packet Rust parsing/copies | Full ingestion22.2ms versus0.96ms delta; periodic rather than sustained cost | Medium: preserve validation and old-report behavior | Medium |

The three retained production changes have native before/after evidence. No
unmeasured production optimization was retained. GUI/query candidates belong to
Astra1's separate replay soak and are not claimed here.

#### Reproduction, handoff and verification

Use the private-server command above with `MGLIVE_STRESS_RUN=final` and
`MGLIVE_STRESS_VISUAL_TAIL=1`. The original final trace added stock row7 to
`events:ST_0_10` at about1790 seconds; its final ten seconds are explicitly a
visual control. After the timer, rows411/412 were applied; the final native report
shows `[7,411]` on `events:a6` and `[412]` on `events2:171`. Future helper versions
put all three in the optional post-timer tail. No client rendering is claimed.

The closed replay is `target/test-output/live-stress/final/replay.jsonl.gz`
(**7,352,639 bytes**), with `drafts.jsonl`, `manifest.json`, `engine.json`,
`model-tail.json` and `visual-tail.json`. It is complete, below its raw cap, and
requires no decompressed copy. The manifest pins module SHA-256
`663e56437f0c9ba8820fa67de3406d1bd33699b529e545b27f4b35812bf8885c`, stock resources,
no HAK, source hashes and the service executable hash. `engine.json` pins native
server/compiler/plugin versions and process cleanup. Astra1 received this replay
and pinned the fixture before the equipment regression reused the one working MOD.
Their GUI soak is observer input -> isolated store/view, not proof of Redis delivery.

Recreate analysis and bounded Rust input with:

```powershell
C:/Python312/python.exe tools/live/stress_analyze.py target/test-output/live-stress/final
$env:MGLIVE_MEASURE_REPLAY='1'
$env:MGLIVE_MEASURE_SESSION='final'
pwsh -File tools/live/check.ps1 -Phase 5 -Step stress-replay -Package mg-live -Filter measure_live_replay -Jobs 1 -NoUiDebug -UiUnoptimized
```

Unset the two measurement variables before normal tests. For GFF proof, use
`MGLIVE_LOOK_PROOF_SESSION=final` and filter `native_look_reports_parse`.
The final normal gate passed156 core tests (four ignored timing assays), guarded
integration entrypoints, focused actual replay/GFF proofs, private native
`equipment_look`, three pure stress-collector/analyzer regressions, mg-live
all-target Clippy with `-D warnings`, and workspace `cargo +1.98.0 fmt --all -- --check`.
No app binary or demo lifecycle operation was invoked.

The task-owned artifact ledger is `target/test-output/live-stress/ledger.json`.
Cleanup retains the baseline/backlog/final gzip replays, draft mappings, native
and Rust summaries, source/version manifests, compact store-log sequence receipts,
negative cold-start evidence and equipment/VFX proofs. Temporary service data
(audit/event JSONL, identities, credentials), bounded decompressed replay samples,
the obsolete lighter smoke capture, and the one temporary service executable are
reproducible or superseded and are listed for removal after saving the receipts.
The single current private fixture/workset and Astra1's own immutable GUI input
are preserved; shared Cargo outputs and other agents' files are outside cleanup.

Session `01a1307a-d0d3-7ec0-ba41-d6b9500c1d4a` is recorded for later, separately
approved history retention. Its active JSONL under
the agent's local Codex session folder is not altered. There is no authorization
here to delete this conversation or shared application history.

Cleanup outcome: automatic approval review rejected both the verified batch and
then the single explicit task-owned `live-stress/smoke/data` directory, with only
"blocked by policy" as the reason. **No deletion ran; 0 bytes were reclaimed.**
`cleanup-plan.json` lists57 verified own targets totaling180,405,487 logical bytes
(180.4 MB) that remain, including the disposable logs and executable copy. No
physical-allocation or shared-drive free-space gain is claimed. Re-running this
cleanup needs an approved deletion path; it must not bypass the review by switching
tools. Required evidence is separate from this list.
