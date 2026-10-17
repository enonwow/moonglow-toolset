---
type: Report
title: 'Live GUI performance: measurement phase'
description: Native replay and read-only process measurements for the running Live GUI, with profile comparisons, repaint causes, frame stages, memory limits and proposed optimizations.
tags: [live, performance, gui, measurement]
generated: { by: codex/astra, at: 2026-10-14 }
sources:
  - ../tools/live/Run-GuiMeasure.ps1
  - ../tools/live/Measure-GuiProcess.ps1
  - ../crates/mg-ui/examples/live_measure.rs
  - https://learn.microsoft.com/en-us/windows/win32/memory/memory-protection-constants
---

# Live GUI measurement

The baseline below is retained for comparison. Implemented changes and their
individual gates are recorded in **Optimization follow-up** at the end.

Phase 1 is measurement only. No production optimization, cadence change, cache
policy change or launcher change is part of this work. The owner demo was sampled read-only; no demo lifecycle command was issued and
its application binaries were neither built nor replaced. The baseline was
collected overnight. On resuming at 08:47 on 14 October, its process was no
longer present; this agent did not restart it.
The companion game/wire report is [Live game performance](live-game-performance.md).

## Method and reproduction

Starting tree: `e6fe4cb` on `live-mode`, Rust 1.98.0, Windows, Intel Core Ultra 7
265F (20 logical processors), NVIDIA RTX 5070 driver 617.42, Vulkan. The helper
records the adapter rather than assuming a graphics backend. Each native replay
uses eframe/winit presentation, a 1600 by 1000 point window, the normal Live UI,
and an in-memory LiveStore. It opens the existing scratch module read-only,
never connects a LiveService and has no command sender. No settings, plugins,
recovery state or speaker startup from the owner's application is loaded.
Startup figures therefore measure this bounded replay, not a complete cold
installation/application launch.

Inputs are the existing `target/test-output/live-demo/DEMO-Live-01.mod`, its
scratch `user` resources, and `target/test-output/live-next/review-profile-fixture.json`.
The replay fixture has **219 rows: 217 creatures, one green lightshaft placeable
and one off-camera door**; the original crowd screenshot has 218 objects.
This synthetic crowd is different from the authored demo's 218 GIT objects,
which include waypoints and a store that do not cross the Live wire. The effects
case attaches visualeffects row 7 to the first 16 creatures in memory; the
helper verifies that Full attaches all 16 models. Source game data is not
committed. Input and binary hashes are retained in the output records.

Run in PowerShell 7, from the repo root:

```powershell
# Each command builds only mg-ui's diagnostic example, through the NWN_ROOT guard.
# Run serially; do not compile or run another GPU check during a timed sample.
tools/live/Run-GuiMeasure.ps1 -Profile Debug -Seconds 12
tools/live/Run-GuiMeasure.ps1 -Profile DevOpt -Seconds 12
tools/live/Run-GuiMeasure.ps1 -Profile Release -Seconds 12
# Separate run: allocator atomics must not distort the CPU comparisons.
tools/live/Run-GuiMeasure.ps1 -Profile Debug -Cases moving,effects-full,log-loaded -Heap -Seconds 12
```

All builds use `tools/live/check.ps1 -MeasureGui -Jobs 1 -NoUiDebug`. Debug means
workspace dev opt-level 0 (third-party dependencies already use opt-level 2).
DevOpt overrides workspace dev opt-level to 2; Release uses the release profile
(opt-level 3). `mg-ui` debug symbols are disabled in both dev variants to stay
within this workstation's commit limit. Neither `-Build` nor `-GuiBuild` is used.
`mg_testkit::gpu::hold()` is held for the helper lifetime (an in-process lock);
separate test binaries must still be scheduled serially. Its isolated
APPDATA/NWN_HOME are under `target/test-output/live-measure`; it uses the real
game installation only for resource reads. Binary startup may warm OS file
caches; these are fresh processes, not forced cold-disk measurements.

The helper waits for the first Live frame, then two seconds of settling, and
samples for twelve seconds. CPU is total process user+kernel time divided by
elapsed wall time: **100% means one logical core**, not the whole 20-core machine.
This includes worker/driver threads and final egui rendering. Frame spans are
inclusive UI-thread wall time, not sampled CPU stacks or GPU timestamps; nested
spans overlap. Queue submit time is CPU submission, not GPU execution latency.
The existing offscreen `frame_perf` helper omits final egui painting, so its
numbers are not substituted for native process CPU.

Cases:

- `static`: the same report, objects hidden and tiles removed only from the
  helper's in-memory drawing. No reports; isolates age/reload/backup timers.
- `reports-only`: the same static drawing control with unchanged object data in
  one report per second; isolates report wake/invalidation.
- `quiet`: original visible scene, Pause objects on, no reports. Tiles/materials
  still animate under the production rules, so this is not fully static.
- `reports`: stationary original scene, Pause objects, one report per second.
- `idle`: stationary NPCs playing idle animations, one report per second.
- `moving`: NPC positions oscillate in one report per second; normal interpolation.
- `effects-markers` / `effects-full`: moving replay plus 16 lasting effects.
- `hidden`: Module Properties explicitly docked over the area; the area stays open.
- `minimized`: minimize only the helper and verify IsIconic for its own PID.
- `panel` / `log`: moving replay, corresponding tool selected instead of Chat.
- `chat-loaded` / `log-loaded`: moving replay with a preloaded 1,000-event
  synthetic chat ring (one area); each selected window shows its bounded cache.
  The events are seeded once, not resent with every report.
- `cues` / `speech`: Pause objects, one damage/chat event per second; counters
  verify that the cue/bubble is actually displayed.

To recreate the retained crowd fixture, use the existing guarded picture recipe
in [the view implementation record](live-mode-next.md):
`MGLIVE_PICTURE_STAGE=after`, then `tools/live/check.ps1 -Phase 8 -Next
-Step review-profile -Jobs 1 -NoUiDebug -UiUnoptimized -Picture
live_review_profile_snapshot`. It checks an existing fixture rather than
silently replacing it. The private demo module/resources must already exist;
this measurement runner does not prepare or start a server.

The script rejects controls that redraw a hidden/minimized/static area, missing
Full effect models and missing cue/bubble observations. Each cause count is an
egui repaint request recorded per UI pass, **not an independent OS wake**: several
requests coalesce, and a long timer requested each frame does not wake each frame.

## Preliminary read-only demo sample

PID 42460, the existing `target/debug/moonglow.exe`, sampled for 30 seconds:
**64.22% of one core**, consistent with the owner's 66%. The read-only status
sample contained 29 creatures, 32 placeables and one door; it is not the
synthetic crowd. A separate 30.5-second SUBSCRIBE received 28 state messages
(345,764 bytes total) and two log messages (532 bytes), including two reported
chat events. The script stores counts only, no chat text. This approximately
one-message-per-second input does not explain continuous 8-13 FPS drawing by
itself: the area animation path schedules those draws.

`tools/live/Measure-GuiProcess.ps1` reads identity, CPU, working set, private
commit and VirtualQueryEx regions; optional `-GpuMemory` reads WDDM counters.
`tools/live/measure-redis-wakes.py --out <scratch.json>` is SUBSCRIBE-only.
Neither sends an operation or changes the running process. The original binary
hashes and process start identity are retained in the ownership ledger.

## Results

The final matrix passed **48/48 control validations** (16 cases, three profiles).
Each cell is CPU percent of one core / UI passes per second / area 3D redraws
per second. Native frame counters matched UI-pass counts in these samples;
these are not display scanout FPS. The area render target was 821 by 621 pixels
within the 1600 by 1000 point UI, at scale 1.

Cells: CPU % of one core / UI passes/s / 3D redraws/s.

| Case | Debug | DevOpt | Release |
| --- | ---: | ---: | ---: |
| static | 0.26% / 1.00 / 0.00 | 0.13% / 1.00 / 0.00 | 0.39% / 1.08 / 0.00 |
| reports-only | 1.81% / 2.98 / 0.99 | 1.69% / 3.00 / 1.00 | 1.04% / 3.00 / 1.00 |
| quiet | 69.64% / 13.33 / 13.33 | 41.78% / 59.98 / 20.74 | 39.96% / 60.06 / 20.07 |
| reports | 72.61% / 13.77 / 13.77 | 42.01% / 59.85 / 21.72 | 43.41% / 60.14 / 20.55 |
| idle | 80.91% / 9.79 / 9.79 | 49.41% / 28.30 / 23.30 | 51.57% / 28.93 / 23.94 |
| moving | 84.40% / 9.38 / 9.38 | 50.46% / 27.80 / 22.80 | 50.56% / 28.12 / 23.21 |
| effects-markers | 85.18% / 9.46 / 9.46 | 50.06% / 27.88 / 23.05 | 51.56% / 27.85 / 23.11 |
| effects-full | 81.41% / 9.46 / 9.46 | 49.29% / 27.96 / 23.05 | 50.63% / 27.92 / 22.93 |
| hidden | 1.16% / 1.99 / 0.00 | 0.78% / 2.00 / 0.00 | 0.26% / 2.00 / 0.00 |
| minimized | 0.52% / 0.00 / 0.00 | 0.26% / 0.00 / 0.00 | 0.78% / 0.00 / 0.00 |
| panel | 85.57% / 9.46 / 9.46 | 57.73% / 27.55 / 22.80 | 56.51% / 27.85 / 23.11 |
| log | 82.07% / 9.46 / 9.46 | 50.45% / 27.54 / 22.55 | 46.52% / 28.03 / 23.04 |
| chat-loaded | 81.16% / 9.38 / 9.38 | 50.58% / 27.38 / 22.55 | 52.10% / 27.94 / 22.95 |
| log-loaded | 83.91% / 9.45 / 9.45 | 53.97% / 27.38 / 22.64 | 53.68% / 27.20 / 22.88 |
| cues | 73.91% / 13.86 / 13.86 | 40.71% / 60.18 / 21.81 | 36.16% / 60.01 / 20.48 |
| speech | 73.91% / 13.94 / 13.94 | 37.84% / 59.84 / 21.39 | 40.57% / 60.01 / 20.72 |

Validated 48 native samples; no speed threshold imposed.

**Build choice:** the moving crowd drops from 84.40% to about 50.5% of one core
and rises from 9.38 to about 23 scene redraws/s. That is about 34 core-percentage
points less CPU (40% relative) and 2.5 times the redraw rate. DevOpt and Release
are too close in these short samples to rank reliably. Yes: a demo intended to
represent user-facing performance should launch an optimized GUI build. The
current launcher explicitly selects the unoptimized debug executable; this
phase does not change it. Compare the optimized result with the bounded 25 Hz
scene animation cadence, not an assumed 60 Hz scene target.

These are single 12-second samples after two seconds of settling, without
confidence intervals. Release was sampled overnight; the interrupted DevOpt
and Debug matrices were refreshed in the morning on the same workstation.
The peer's intervening `9eb7ee1` commit adds game measurement code/tests, not a
production GUI change. No cargo build ran during a timed sample. OS scheduling,
boost and driver variation remain; differences of a few percentage points,
including Full sometimes below Markers, are not causal evidence. All 16 lasting
models were observed in Full. This fixture does not establish the cost of every
VFX, a large number of unique looks, or a full-screen 3D viewport.

### CPU stages and source locations

Mean milliseconds per invocation, moving case. UI/frame/overlay rows run every
UI pass; scene/pose/particle/render rows run only on an actual 3D redraw; rebuild
and reconciliation run only on a report (12 times). The measured spans are
inclusive and must not be summed across parent/child rows.

| Span | Debug ms | DevOpt ms | Release ms |
| --- | ---: | ---: | ---: |
| live.frame | 0.438 | 0.082 | 0.079 |
| live.report | 0.047 | 0.012 | 0.011 |
| live.rebuild | 1.466 | 0.776 | 0.767 |
| live.reconcile | 0.027 | 0.008 | 0.005 |
| live.cues | 0.002 | 0.001 | 0.000 |
| area.live_poses | 36.391 | 8.728 | 8.376 |
| area.particles | 30.761 | 5.776 | 5.643 |
| area.scene | 2.659 | 0.514 | 0.481 |
| area.encode_submit | 13.513 | 4.234 | 4.049 |
| renderer.queue_submit | 0.137 | 0.140 | 0.136 |
| live.overlay | 0.822 | 0.295 | 0.263 |
| overlay.label_placement | 0.101 | 0.020 | 0.019 |
| overlay.legend | 0.080 | 0.053 | 0.043 |
| area.ui | 85.914 | 16.862 | 16.329 |

Release p95 per redraw: area.live_poses 9.75 ms, area.particles 6.49 ms, area.encode_submit 4.86 ms.
The 0.136 ms queue-submit observation is CPU time spent submitting commands;
GPU execution, present wait and final egui paint are not individually timed.
Total process CPU includes them where they consume CPU. It is not valid to
subtract UI wall spans from process CPU and call the remainder GPU time.

| Hot path | Source at this measurement revision | Implication |
| --- | --- | --- |
| Live poses | `crates/mg-ui/src/area_view.rs:2033`, `crates/mg-area/src/scene.rs:383` | Every visible extra gets model instances on each redraw, including poses at time zero when objects are paused. |
| Particle preparation | `crates/mg-area/src/scene.rs:692` and `:724`, `crates/mg-preview/src/compose.rs:309` and `:317` | `all_emitters` builds the base pose even if no attached part emits; potential duplicate pose work. |
| Report/looks/cache miss | `crates/mg-ui/src/live_view.rs:138`, `:416`, `:525` | Rebuild including look resolution costs 0.77 ms once per report; reconciliation alone 0.005 ms. Warm looks are cached. |
| Markers/labels/legend | `crates/mg-ui/src/live_view.rs:875`, `:727`, `:771` | All overlay drawing 0.26 ms per UI pass; not the leading cost. |
| Scene encode/submit | `crates/mg-ui/src/area_view.rs:2112`, `crates/mg-render/src/renderer.rs:1722` | About 4.05 ms per redraw; queue submission itself is a small part. |
| Chat/Log revision query | `crates/mg-ui/src/live_chat.rs:129`, `crates/mg-ui/src/live_log.rs:302`, `crates/mg-live/src/events.rs:233` and `:249` | Query clones matching events before applying even a limit of one. |
| Panel | `crates/mg-ui/src/live_panel.rs:267` | Full `store.area` clone every drawn pass plus row sort/drawing. |
| Launcher | `tools/live/Start-LiveDemo.ps1:30` | Explicit debug application path; benchmark profiles do not change it. |

Window means per UI pass (includes each window's query and drawing):

| Window/control | Debug ms | DevOpt ms | Release ms |
| --- | ---: | ---: | ---: |
| Chat, empty | 0.059 | 0.040 | 0.033 |
| Log, empty | 0.097 | 0.057 | 0.055 |
| Chat, 1000-event ring | 1.632 | 1.020 | 0.987 |
| Log, 1000-event ring | 1.298 | 0.708 | 0.720 |
| Live Panel, 219 rows | 2.079 | 1.620 | 1.262 |

### Repaint accounting

Representative Release counts over twelve seconds:

| Source | Moving | Fully static | Hidden area | Meaning |
| --- | ---: | ---: | ---: | --- |
| Report callback, helper equivalent of Redis wake | 12 | 0 | 12 | One explicit request per accepted injected report. |
| Area animation, `area_view.rs:2213` | 338 | 0 | 0 | Same 40 ms deadline can be requested by several UI passes. |
| Live age, `area_view.rs:2216` | 0 | 13 | 0 | Once-per-second age display while no animation is active. |
| Resource reload, `lib.rs:3358/3364` | 338 | 13 | 24 | 3-second timer requested/coalesced per pass, not that many reloads. |
| Conversation backup, `lib.rs:3472` | 338 | 13 | 24 | 5-minute deadline requested per pass, not that many backups. |
| Legend | 0 | 0 | 0 | Draws on existing passes, no own repaint request. |

With one damage event/s the cue timer (`live_cues.rs:385`) appears on 288 of
721 UI passes; speech expiry (`:264`) appears on 720 of 721 passes. Those are
request-source counts, not distinct OS wakes. Bubble expiry is four seconds.
The stationary paused scene still has animated tiles/materials and reaches
about 60 UI passes but only 20-22 scene redraws/s. Hidden areas have **zero
scene redraws and zero report reads**; minimized helpers have zero UI passes.
The fully static control retains a visible age label, so its approximately
1 Hz UI wake is an actual visible text change. Unchanged 1 Hz reports still
invalidate the static drawing about once per second; that is a separate future
candidate from animation scheduling.

The bridge's accepted state/log callbacks (`crates/mg-live/src/bridge.rs:239/254`)
call the global repaint callback (`crates/mg-ui/src/options.rs:269`). The real
read-only subscription counted 30 messages over 30.5 seconds. Connection,
disconnection and command-result wakes are additional paths; they were not
exercised by the fake-store replay. Motion and lasting-effect animation use the
area animation path rather than a separate wake source. Synthetic report
production took 16.72 ms total in the Release moving sample, included in
process CPU (wall-time upper-order contribution about 0.14% of one core).

### Startup

Fresh helper process, warm filesystem/shader caches; timings start at Rust
`main`, excluding earlier OS process/DLL loader time. First Live frame means
scene work and GPU commands have been queued by the UI, not presentation on a
physical display. The helper intentionally omits normal settings/plugins,
recovery and LiveService connection setup. A complete cold owner-app startup
remains unmeasured under the no-restart restriction.

| Moving replay startup | Debug ms | DevOpt ms | Release ms |
| --- | ---: | ---: | ---: |
| GPU/window setup from main | 594.1 | 557.5 | 565.4 |
| Resource/app/module initialization interval | 500.9 | 113.4 | 57.2 |
| Main to first queued Live frame | 1923.8 | 859.2 | 796.5 |


## Memory interpretation

The existing demo's sample held **919.14 MiB private commit** and
**124.91-124.94 MiB working set**. These are different quantities:
[Microsoft's process counters](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex)
define private usage as commit, not resident RAM. VirtualQueryEx classified
499.64 MiB as private read/write plus WRITE_COMBINE, 360.58 MiB as ordinary
private read/write, and about 6 MiB in guard/no-access/no-cache/other private
regions. This address-map view is not a disjoint partition of PrivateUsage;
copy-on-write image pages and other accounting differences remain.
[Protection flags](https://learn.microsoft.com/en-us/windows/win32/memory/memory-protection-constants)
do not identify the owning cache or allocator.

A separate WDDM counter sample reported 529.78 MiB dedicated GPU usage and
52.71 MiB shared GPU usage for the NVIDIA adapter. Those are a separate view of
video-memory accounting, not amounts to add to private commit or working set.
[Microsoft describes these counters](https://devblogs.microsoft.com/directx/gpus-in-the-task-manager/)
and also documents a
[per-process counter issue](https://learn.microsoft.com/en-us/troubleshoot/windows-client/performance/gpu-process-memory-counters-report-wrong-value)
on affected Windows versions; they do not by themselves prove a memory leak.

The demo's current serialized area snapshot is only 30,209 bytes for 62
objects, with **zero PCs and no player-look JSON** at that observation. Its
newest 500 events serialize to 122,001 bytes (312 server-log, 127 chat, 33
attack, 22 damage and six combat/target events); these are payload sizes, not
heap sizes. The event store retains 1,000 events per area, including a separate
area-less/server-log ring. On disk, 31 retained JSONL files total 6,940,786
logical bytes across demo runs. The logger appends new batches; it does not
load those disk files into the GUI as a retained history. No chat text, names
or credentials are copied into the measurement report.

The native replay allocation run below separates Rust-requested live bytes
from process commit. It cannot reconstruct ownership of every existing
allocation in the hours-old owner process without restarting or attaching a
heap trace. The read-only editor-cache endpoint belonged to another process;
it was not used as evidence for this demo. Attribution of all 919 MiB to
models, player looks or log files would therefore be unsupported.

### Separate allocation experiment

Debug helper with `measure-alloc`; its CPU numbers are deliberately excluded
from the profile comparison. The wrapper counts successful Rust System
allocator requests and their live/high-water requested sizes. It excludes
allocator padding, freed-but-retained blocks, C/C++ libraries and driver
allocations. Peak is the high-water mark of requested live bytes, not total
allocation traffic. Snapshots themselves add a small amount of diagnostic data.

| Moving replay stage | Private MiB | Live Rust MiB | Peak Rust MiB |
| --- | ---: | ---: | ---: |
| Before GPU/window | 3.60 | 0.88 | 1.69 |
| GPU/window ready | 362.88 | 2.31 | 2.58 |
| Game resources ready | 398.45 | 26.56 | 38.73 |
| Module ready | 406.88 | 28.59 | 38.73 |
| First queued Live frame | 490.97 | 87.31 | 91.33 |
| End: moving | 500.28 | 91.01 | 93.54 |
| End: effects-full | 520.77 | 107.73 | 110.76 |
| End: log-loaded | 502.21 | 90.94 | 98.03 |

GPU/window initialization accounts for a **359.3 MiB commit increase** while
Rust live requests increase only **1.43 MiB**. That interval already contains
234.23 MiB of private WRITE_COMBINE regions. This locates a major commitment
phase outside the scene/Live cache and mostly outside Rust-requested memory;
it does not identify individual driver allocations. Loading game resources
adds 24.25 MiB live Rust, opening the module adds 2.03 MiB, and preparing the
first scene adds 58.72 MiB (models/animations/mesh/UI data combined, not an
exclusive model-cache ownership estimate).

Full with sixteen lasting models ends about **16.72 MiB higher in live Rust**
and **20.50 MiB higher in commit** than the moving case. The preloaded event-ring
case adds about **1.05 MiB live Rust before GPU initialization**, where this is
a clean controlled input difference. Later snapshots vary with transient frame
allocations; do not infer event memory from the near-equal end totals. The
newest 500 synthetic events serialize to 93,504 bytes; nested objects and strings
cost more in memory than the wire representation.

Resource census near the first warm Live frame (logical payload, excludes
native alignment/allocator overhead; these rows are **not additive** with Rust,
private-commit or WDDM totals):

| Resource | Moving | 16 lasting effects, Full |
| --- | ---: | ---: |
| Cached Live looks | 3 | 3 |
| Player looks | 0 | 0 |
| Object preview cache entries | 11 | 13 |
| Tile cache entries | 19 | 19 |
| Unique GPU models | 52 | 78 |
| Model vertex/index payload | 1.11 MiB | 1.29 MiB |
| Texture entries | 83 | 84 |
| Texture payload | 9.04 MiB | 9.06 MiB |
| Renderer buffers | 4.23 MiB | 4.42 MiB |
| Area color/MSAA/depth targets | 17.50 MiB | 17.50 MiB |
| CPU uniform/bone/vertex capacities | 3.50 MiB | 3.50 MiB |

The crowd shares three looks; 218 objects therefore do not imply 218 independent
creature model loads. The fresh replay is also a different resource mix/history
from the long-running demo. About 4.5 GiB of file mappings appear in the address
map, largely game resource archives (`mg-key/src/lib.rs:193`,
`mg-resman/src/container.rs:75`); this is mapped virtual address space, not that
much resident/private RAM. The cache counts do not prove a retention leak.

**Remaining attribution limit:** the owner process was not restarted or traced
from allocation time. We can explain its resident-vs-commit distinction,
address categories, small current wire/log payloads and the fresh replay's
large context initialization commitment. We cannot honestly allocate all
919 MiB among named caches. A future native heap/GPU allocation trace on an
owned reproduction, plus repeated open/close cycles with stable inputs, is
needed before an eviction policy or leak diagnosis.

## Ranked candidates for the implementation phase

These are proposals, not implemented changes. Except for the profile comparison,
gains are estimates bounded by measured work and require before/after replay.
Savings overlap where particle preparation repeats posing.

| Rank | Candidate and expected gain | Risk | Effort |
| --- | --- | --- | --- |
| 1 | Launch an optimized GUI (Release or explicit optimized-dev artifact). Measured crowd: ~34 core-percentage points saved, ~2.5x scene redraw rate; main-to-first-Live ~1.9 s to ~0.8-0.9 s. | Low; make artifact/profile selection explicit and preserve diagnostic builds. | Small launcher/build packaging change plus acceptance. |
| 2 | Avoid emitter traversal/base posing for models with no emitters; cache emitter presence. Current particle preparation is 5.64 ms/redraw in Release. A 3-5 ms reduction is a hypothesis, roughly 7-12 core points at 23 Hz. | Low-medium; must retain intrinsic placeable emitters, attached effect transforms and arrival warming. | Medium; focused compose/particle tests and pictures. |
| 3 | Reuse immutable/paused poses and share local poses for equal look/animation/time keys. Current live posing is 8.38 ms/redraw; target a 3-6 ms reduction (about 7-14 core points at current cadence), to be proved. | Medium-high; death, equipment, movement blend, per-object effects and transform keys must remain correct. | Medium-large. |
| 4 | Align/coalesce animation deadlines and avoid unchanged-report picture invalidation. Paused animated scene currently has ~60 UI passes vs ~20 redraws/s; reports-only redraws at 1 Hz with unchanged visible drawing. Expected small CPU benefit (order 1-5 core points with ordinary panes); preserve bounded motion and visible age/expiry updates. | Medium; responsiveness, timer expiry, hidden/minimized wake correctness. | Medium. |
| 5 | Cheap event revision accessor/bounded queries and revision-cached Panel reports. Loaded Chat/Log cost ~1.0/0.7 ms per UI pass, Panel ~1.3 ms: roughly 2-4 core points per active pane at ~28 Hz, with query savings only part of that. | Low-medium; filters, incoming-event visibility and cursor semantics. | Small-medium, coordinate store API ownership. |
| 6 | Trace native/context commitment and cache lifetimes before changing memory policy. Current confirmed GPU payloads are tens of MiB; most initial private commit precedes scene loading. No honest memory-saving estimate yet. | Low for tracing; premature eviction has correctness/stutter risk. | Medium measurement follow-up; implementation depends on attribution. |

Report caching, label placement and legend work rank below posing/particles:
their measured costs do not justify starting there. Do not switch the default
effects mode or claim all Full effects are cheap from this one 16-model case.

## Verification and retained artifacts

Reusable helper/instrumentation commit: `fa09cda`. All production probes are
behind the opt-in `measure` feature; allocation atomics are additionally behind
`measure-alloc`. Normal GUI defaults, cache policies, scheduling and launcher
behavior are unchanged. Only the separate example executable was built.

- `python tools/live/summarize-gui-measure.py`: **48/48** native controls pass,
  matching input hashes and one binary hash per profile, no queued commands.
- Separate Debug allocation controls: **3/3** pass; report reads/reconciliations
  match all twelve injections; live counts do not exceed requested-size peaks.
- Guarded `check.ps1 -Phase 8 -Next -Step measure-lint -MeasureGui -MeasureHeap
  -Lint -Jobs 1 -NoUiDebug`: Clippy passes with `-D warnings`.
- Normal-feature guarded `-UiTest live_motion -Jobs 1 -NoUiDebug
  -UiUnoptimized`: **1 passed**.
- The extra normal-feature `-UiTest live_viewer` **failed** at
  `crates/mg-ui/tests/app.rs:6467`: it still queries top-level `Run Script?`,
  whereas `880c0b1` moved that action into Scripts
  (`crates/mg-ui/src/live_context.rs:323`). The same legacy test also contains
  old player-menu expectations. This pre-existing menu-test mismatch is
  reported for the coordinator; no production behavior or unrelated test was
  changed to hide it. Its failure log is retained.
- Own Rust files pass rustfmt `--check`; PowerShell parsing, Python AST and
  TOML parsing pass. Report local links/frontmatter were checked. OKF is not
  installed, so no OKF-lint pass is claimed; index/log were maintained by hand.
- Protected `moonglow.exe` and `mg.exe` hashes, sizes and modification times
  match the initial ledger. No app build, demo start/stop or game operation.
  No new visual behavior was introduced, so no before/after UI picture change
  is claimed. Native controls exercise rendering directly.

The 51 final sample JSON files, read-only baseline/census records, compact
`gui-checks.json` and guarded logs are retained under
`target/test-output/live-measure/` and `target/test-output/live-next/`.
Rejected/interrupted trials were overwritten in the same owned output paths,
not copied into successive output trees. The original private fixture/module
remain required and unchanged:

- Fixture SHA-256: `7766E5F7189107CE837492FD3993A3E5D1C935B5E10F5CAA1DB951F1F87DBF0B`.
- Module SHA-256: `2F2D8C88BF52AF8B25C2485BB9718234E2BF4AE4E16EF7D960A4003D4AD8D084`.

Cleanup of **six diagnostic build files and 102 empty capture logs** was
attempted after recording the results. Automatic approval review rejected the
command with **"blocked by policy"**, without a more specific explanation.
Nothing was deleted or retried through another tool. The candidates total
**532,060,230 logical bytes (507.41 MiB)**; recovered space is **0 bytes**.
Allocated disk bytes were not measured and no global free-space change is
attributed to this task. Their exact paths and blocked status remain in the
ownership ledger. Shared cargo caches, peer artifacts, input resources,
required evidence and active history are preserved.

## Evidence ownership and limits

Session `01a12fe8-1458-7551-99fc-b277832def84`; exact GUI file inventory and
protected app hashes: `target/test-output/live-measure/owned-files.json`.
That directory is shared with Astra2's measurement work: ownership applies only
to the listed files. Required input/module/HAK files are reused, never copied per
sample. Current JSON evidence and reusable source stay available for review.
Active history: the agent's local Codex session file.
The active session history is not modified; its later cleanup is a separate
owner-approved task. Native driver allocation ownership cannot be inferred
exactly from VirtualQuery protection flags or Rust allocator counters.

## Optimization follow-up

Implementation follows [the agreed plan](live-optimization-plan.md), steps
1, 3, 7, 8. Each uses the same native replay helper and inputs; tagged JSONs
under `target/test-output/live-measure/` retain before/after evidence without
copying resource/build trees. The demo application is not built, started or
stopped during this implementation turn. Shared store.rs/game code belongs to
Astra2. New source/evidence ownership: `optimization-owned-files.json`.

### Step 1 - optimized demo launcher

`Start-LiveDemo.ps1` now requests `check.ps1 -GuiBuild -Jobs 1 -NoUiDebug`
before starting services, and selects `target/release/moonglow.exe`. `-GuiBuild`
builds only the GUI using Release and forces one compiler job. Ordinary dev
and multi-app development builds retain their debug profile. `-SkipBuild` now
reuses both the module and an already-built Release GUI; it never falls back to
an unoptimized executable. It gives the guarded build command if Release is
missing. Incompatible `-UiUnoptimized -GuiBuild` is rejected.

The session records `guiProfile=release`; the stop helper accepts only the
known debug/release paths plus the existing PID/start-time checks. Old records
without a profile retain their debug identity. This edit is necessary so a
future owned Release session can be stopped correctly; no stop was executed.

Re-measurement (20 s, moving crowd): **83.52% -> 49.93% of one core**, scene
**9.41 -> 23.42 redraws/s**. Sources:
`opt1-before-debug-moving.json`, `opt1-after-release-moving.json`.
This compares the launch profiles in the native helper, not a restarted demo.
It confirms a 33.59-point reduction and 2.49x redraw rate, so the step is kept.
The four edited PowerShell scripts parse successfully; profile paths and legacy
session fallback were reviewed. There is no visual/UI layout change.

The actual application link/launch remains deliberately unexecuted under the
coordinator's no-build/no-lifecycle instruction. On the next authorized restart,
use the default launcher (builds Release) or first the guarded `-GuiBuild`
command and then `Start-LiveDemo.ps1 -SkipBuild`. No manual unguarded cargo.

### Step 3 - bounded event copies; Panel cache rejected

EventLog now filters/orders borrowed events, applies the result limit, and only
then clones payloads. The existing query API, scope checks, ring retention and
oldest-first result order stay unchanged; store.rs was not edited. This makes
Chat/Log's limit-one revision queries cheap even with a full 1,000-event ring.

Release native replay, 20 s each, tagged `opt3-before` / `opt3-after`:

| Case | Window CPU ms/pass before -> after | Process CPU % one core before -> after | Scene redraws/s before -> after |
| --- | ---: | ---: | ---: |
| Chat, full ring | 0.946 -> 0.394 | 50.80 -> 51.72 | 23.12 -> 23.22 |
| Log, full ring | 0.666 -> 0.118 | 53.05 -> 49.16 | 23.02 -> 23.27 |
| Panel cache trial | 1.218 -> 1.254 | 51.41 -> 54.54 | 23.17 -> 23.12 |

The event change removes about 0.55 ms per UI pass (58% Chat / 82% Log),
equivalent to about 1.5 core-points at 28 passes/s. Whole-process CPU is noisy:
Chat does not show an overall reduction in this sample; no such claim is made.
Astra2's intervening 13fadfa improves producer-side report merging. The window
spans isolate this GUI change; producer work was only about 20 ms over each
20-second sample.

The proposed Panel cache (report identity + cached row strings) **was reverted**:
a second 20 s run, `opt3-repeat-release-panel.json`, still costs 1.247 ms/pass
and 56.25% process CPU. Keeping that complexity without a measured gain would
violate the plan. The original Panel implementation remains. Its original and
trial pictures (`visual/panel-opt3-before.png`, `panel-opt3-after.png`) were
inspected and match visually; the snapshot helper now waits for the default
Live layout and verifies that the Panel is actually selected before capture.

Regression from 880c0b1 repaired: the viewer test opens Scripts before Run Script,
asserts disabled NWScript with chunks off, and follows the current Character /
Give Gold submenu. Player-off disabling and player-on queued target/amount are
asserted. Guarded viewer (1), Panel (2), Chat (1), bounded-query unit and complete
mg-live tests pass; mg-live/mg-ui all-target clippy is clean. The rejected cache
unit also passed, but was removed with its implementation. No app binary or
running demo was touched.

### Step 7 - avoid unused emitter attachment poses

`Composed::all_emitters` used to pose the base skeleton even when no emitting
part needed an attachment transform. It now evaluates that pose lazily, once
per call, only for an emitting part attached to a base node. Plain creatures
and unattached root effects avoid the duplicate pose. Emitter ordering, node
transforms, animation times, simulation IDs and warming are unchanged. No
cross-frame pose cache or animation approximation was introduced.

Release native replay, 20 s, `opt7-before` / `opt7-after`:

| Case | Particles CPU ms/redraw before -> after | Process CPU % one core before -> after | Scene redraws/s before -> after |
| --- | ---: | ---: | ---: |
| Moving crowd | 5.609 -> 2.297 | 49.55 -> 46.89 | 23.07 -> 24.47 |
| Effects Markers | 5.548 -> 2.310 | 48.30 -> 46.58 | 23.17 -> 24.47 |
| Full, 16 lasting effects | 5.834 -> 2.796 | 52.43 -> 45.40 | 23.12 -> 24.46 |

The measured particle-stage saving is 3.0-3.3 ms/redraw, so the step is kept.
All 16 effect models remain attached in the Full native control. The after
run also records static, reports-only, quiet, cues, speech, hidden and minimized
controls; those exact files are step 8's baseline. Sources and fixtures are the
same; Astra2's game-only 590ce91 does not affect this fake-store replay.

Four guarded `live_effects` app tests pass, including player lasting VFX,
clearing effects and base-emitter warming versus fresh root-effect arrival.
Full-mode before/after pictures use `full-effects-opt7-baseline-after.png` and
`full-effects-opt7-after.png` (the original code was temporarily restored for
the baseline, then the exact owned edit restored). Both show darkness and
glow markers with unchanged layout. Note: the older snapshot helper interprets
any tag ending in `before` as Markers mode; that image is not a Full baseline.
The 218-object/16-effect offscreen screenshot hit a GPU wait timeout; native
16-effect rendering/count gates pass, and the smaller Full fixture is used for
visual comparison. No heavy offscreen screenshot success is claimed.

Workspace all-target clippy passes. Fmt found three pre-existing unformatted
VFX test assignments in `tests/support/live_windows.rs`; only their whitespace
was corrected, then the guarded lint/fmt gate was repeated.

### Step 8 - one data wake and bounded animation deadlines

The Live service callback and native replay now use the same `live_repaint::wake`.
egui 0.36.2 gives a zero-delay request an extra settling pass and subtracts
`predicted_dt` from delayed requests (`context.rs:138-152`). The helper uses a
nonzero request argument with that lead time compensated. egui still coalesces
pending requests; ordinary widget/input/discard behavior is unchanged.
Animation scheduling also subtracts work already spent in the frame. The same
40 ms scene budget, speech expiry and age-label updates remain. Caller locations
are preserved for the existing repaint-cause measurements.

Release, 20 s each; the step 7 after files are the immediate unchanged-code
baseline (`opt7-after-release-*.json`), compared with `opt8-after-release-*.json`:

| Case | CPU % one core before -> after | UI passes/s before -> after | Scene redraws/s before -> after |
| --- | ---: | ---: | ---: |
| moving | 46.89 -> 44.55 | 36.90 -> 24.92 | 24.47 -> 24.92 |
| effects-markers | 46.58 -> 46.50 | 37.75 -> 25.02 | 24.47 -> 25.02 |
| effects-full | 45.40 -> 46.81 | 32.95 -> 24.97 | 24.46 -> 24.97 |
| static | 0.70 -> 0.55 | 1.05 -> 1.00 | 0.00 -> 0.00 |
| reports-only | 1.01 -> 0.62 | 3.00 -> 1.20 | 1.00 -> 1.00 |
| quiet | 34.52 -> 35.30 | 59.98 -> 24.44 | 20.39 -> 24.44 |
| cues | 39.57 -> 36.91 | 59.98 -> 24.97 | 20.88 -> 24.97 |
| speech | 38.47 -> 38.31 | 60.03 -> 25.02 | 20.73 -> 25.02 |
| hidden | 0.31 -> 0.62 | 2.00 -> 1.00 | 0.00 -> 0.00 |
| minimized | 0.70 -> 0.39 | 0.00 -> 0.00 | 0.00 -> 0.00 |

Kept: moving CPU falls 2.34 core-points and cue CPU 2.65 points; redundant
passes fall sharply. Hidden reports now trigger one UI pass each, with no 3D
work. Quiet animation reaches 24.4 redraws/s instead of 20.4 with almost the
same CPU (+0.78 point). Full mode is +1.41 CPU points in this sample, so this is
not an across-the-board process-CPU reduction. The small hidden CPU differences
are below one core-point; pass counts give the clearer result. No frame-rate cap
was reduced to obtain a saving. All 20 reports are read in the visible controls;
Full retains 16 effect models; cue/speech and actual minimized-state gates pass.

Two egui unit tests prove wake coalescing/no forced second pass and the exact
40 ms delay. Four guarded app tests pass: live_motion, live_events_view,
live_speech, an_area_s_picture. The Full-effects before image is step 7's final
image; `visual/full-effects-opt8-after.png` was inspected against it and keeps
the same effects/markers/layout. The native replay preserves the fixture and
module hashes and queues no game commands. Production app binaries/lifecycle
remain untouched; only the isolated diagnostic example was linked.

### Completion and owned diagnostic cleanup

All reproduction sources, native JSON evidence, minimal pictures, guarded logs,
input resources and build-profile receipts are retained. `optimization-cleanup.json`
lists the exact disposable files with SHA-256, logical size, FileStandardInfo
allocation and link count. Six diagnostic build files, 166 empty capture logs,
two temporary owned-source saves and the superseded Markers baseline image are
eligible (175 files, 533,904,600 logical bytes). They are not needed to reproduce:
run the retained `Run-GuiMeasure.ps1` without `-SkipBuild` to rebuild its example.
Shared Cargo caches and other workers' files are outside this cleanup. Removal
status and verified totals below supersede the previous turn's blocked attempt.

Cleanup **completed**: all 175 listed files are absent. Removed **533,904,600
logical bytes (509.17 MiB)**; their Windows FileStandardInfo reported allocation
was **533,922,280 bytes (509.19 MiB)**, with one link per file. Observed volume
free space increased **533,921,792 bytes**, but this shared-volume delta is not
claimed as exclusively attributable. The six diagnostic build files account
for 507.74 MiB logical; extra temporary files account for the remainder. No
shared cache, peer file, input, retained evidence or active history was removed.
There is no remaining blocker for this authorized diagnostic cleanup.

Final checks: normal workspace all-target clippy (`-D warnings`) and fmt pass;
Release measurement-feature/example clippy passes; both repaint unit tests pass
again; all four PowerShell scripts parse; report local links resolve. Protected
`moonglow.exe` and `mg.exe` retain the original hashes, sizes and timestamps.
The next demo Release build/launch is still unexecuted under the no-build rule.
The active session ID/history path remains the one recorded above, retained for
separate owner-authorized history cleanup outside this session.


## Post-sixth-acceptance GUI optimization (2026-10-15)

Scope: `mg-ui` Live windows/view code; no demo lifecycle or application build.
The baseline follows `b249a4c`; core measurement-only commit `d0196cc` does not
change GUI behavior. The diagnostic example retains the original 219-row
fixture (218 crowd objects plus the off-camera door) and its 16-effect control.
New cases exercise `focus-loaded`, `areas-loaded`, `legend` and `macros`.
`areas-loaded` adds two in-memory maps, with eight distinct NPCs each; the
source MOD and resource order remain read-only. It opens all three maps in
separate visible panes. Loaded cases seed 1,000 chat events once; Focus retains
its existing 500-row bound. `macros` creates 20 small saved macros and displays
20 editable steps, without sending commands. Its files stay in the single
owned `target/test-output/live-opt-ui/fixture-data` working set.

Reproduce with PowerShell 7 and `tools/live/Run-GuiMeasure.ps1 -Profile Release
-Seconds 15 -Tag ui6-before -Cases moving,effects-full,chat-loaded,log-loaded,
focus-loaded,areas-loaded,legend,macros,panel,static,hidden` (pass the case names
as one PowerShell array, without a line break inside a name). Repeat with
`-Profile Debug`. The runner builds only `live_measure` via the existing guard,
with one Cargo job; `-SkipBuild` verifies the profile/binary receipt. New spans
cover Focus update/drawing, shared area choices, Legend, Scenes and validation.
Every case asserts its populated control; samples queue zero commands.

The pre-existing owner/coordinator launcher independently rebuilt Release apps
while this task's example waited on Cargo's lock. That build was neither
started nor stopped by this task. Sampling is serial and after compilation;
normal demo background load remains uncontrolled. Private/shared process CPU
and small differences remain noise-sensitive; isolated window spans and
control counts accompany process CPU. The task ledger is
`target/test-output/live-opt-ui/owned-files.json`.

### Shared command UI helpers

Move the selection snapshot, runtime-enabled check and recorder-aware send
function from VFX into `live_targets.rs`; Inventory, Library, Skill, Factions and
Scenes use this common home. Identical DM/possessed/player eligibility and
living-creature predicates now serve menus, group orders, patrol, removal,
teleport and VFX. Each operation retains its additional conditions.
This is structural cleanup with no performance or behavior-change claim.
The guarded installed-resource `live_accept6` suite passed 14/14 after the
look repair; Chat, Focus, Scenes, Legend and NPC pictures were inspected.


### Isolate report-to-preview rebuilding

`live_view::frame` now delegates its unchanged report reconciliation, look-cache
refresh and effect attachment block to `live_view/looks.rs`. This removes 248
lines from the main view module and gives the rebuild one reviewable boundary.
Arrival is still tested against the raw engine report before interpolation. The
look decoding defence remains in the extracted reader. Palette construction is
shared with Legend, and the view reuses `mg_live::now_ms`. No visual, input or
performance change is claimed; the 14-case resource gate and inspected picture
set above cover the extraction.

Portrait drawing and area controls were already shared. The retired colour-row
filter remains because older module content can still contain those rows.

### Repeated baseline after sixth acceptance

Both profiles use the controlled replay, 15 s after warm-up. CPU is a percentage
of one logical core; redraw rate is the primary area, not the sum of all panes.
The Debug run precedes the urgent look/Focus fixes; its valid look inputs and
watched-speaker-only event fixture are unaffected by those fixes. Release was
rebuilt after them, using the current core catalog optimization. Physical input
is counted and discarded before egui; non-minimized controls verify OS state.

| Case | Debug CPU / redraw Hz | Release CPU / redraw Hz |
| --- | ---: | ---: |
| moving | 96.03 / 15.93 | 43.53 / 22.54 |
| effects-full | 97.35 / 14.60 | 46.08 / 24.96 |
| chat-loaded | 99.52 / 15.79 | 56.79 / 24.69 |
| log-loaded | 96.27 / 15.39 | 55.22 / 24.43 |
| focus-loaded | 98.56 / 14.72 | 50.86 / 24.76 |
| areas-loaded | 96.38 / 9.68 | 65.85 / 23.23 |
| legend | 99.45 / 16.06 | 48.47 / 24.90 |
| macros | 99.56 / 14.47 | 51.28 / 24.90 |
| panel | 96.54 / 16.59 | 55.86 / 24.17 |
| static | 0.62 / 0.00 | 0.73 / 0.00 |
| hidden | 0.83 / 0.00 | 0.62 / 0.00 |

Previous optimized Release moving was 44.55% / 24.92 Hz and Full 46.81% /
24.97 Hz. Current 43.53% / 22.54 Hz and 46.08% / 24.96 Hz show no broad CPU
regression, but moving has a noisy tail and lower throughput in this sample.
Debug remains saturated with the crowd; its redraw throughput increased from
the original 9.38 Hz to 15.93 Hz, but this does not justify a Debug demo.

New window spans (mean milliseconds per UI pass): Chat 0.652, Focus 0.782,
multi-area Focus 0.715, Log 0.179; shared area-name grouping alone is 0.047
in Chat. Macros is 1.219, including 0.420 of repeated validation/catalog work.
Legend is small; Panel still lays out the whole table. The prior Panel cache
candidate showed no gain, so it is not reintroduced without new evidence.
Raw JSON includes stage means/percentiles, hashes, controls and process metrics:
`target/test-output/live-measure/ui6-before-{debug,release}-<case>.json`.
Whole-process CPU varies with the owner's concurrently running demo; small
changes are evaluated primarily using the isolated spans and control counts.

The first two after-run Full samples were rejected because Windows minimized
the diagnostic window. They are not performance evidence and are overwritten
on the controlled retry. The runner also rejects minimization observed during
a sample, even if the window has been restored by its end. All retained baseline
cases pass both minimization controls. No demo window or process was controlled.

### Cache unchanged macro preflight

`live_scene.rs:280` retains the discovered catalog for its Policy; `:289` retains
the validation result for the exact name/steps/policy. Each step's operation,
JSON input, delay and wait participate in equality. The UI shares the immutable
result instead of cloning parsed JSON on every pass. Explicit save/import and
execution still validate normally; the validator and its messages are unchanged.

Release 15 s, `ui6-before-release-macros.json` -> `ui6-after-release-macros.json`:
Scenes mean **1.219 -> 0.499 ms/pass**, median **1.097 -> 0.476 ms**; unchanged
validation calls **374 -> 0** in the timed interval. Both retain 20 steps and
24.90 area redraws/s. Macro process CPU is 51.28 -> 46.71% of one core. The after
binary also contains the independent history/report-read candidates described
below, so whole-process CPU is a combined observation, not attribution to this
cache alone. The Scenes span is isolated from those other routines.

Kept after measured gain. Three scene unit tests include cache invalidation for
every input/policy; actual record/save/edit/play/stop and saved-macro app flows
pass. `live-opt-ui/after/scenes-after.png` was inspected against the accepted
baseline; labels, saved macros and controls are unchanged. The full guarded
workspace gate also passes on the combined candidate.

### Deduplicate borrowed area identities

`live_areas.rs:11` sorts/deduplicates borrowed IDs before creating the small
list of display rows. Chat and Focus no longer allocate a String for each of
their 500 retained events every pass. Names still resolve through the existing
module/resource cache, so resource changes do not leave stale tab labels. No
additional invalidation state or UI behavior is introduced.

Release shared-area grouping mean ms/pass, before -> after: **Chat 0.0470 ->
0.0195**, **Focus 0.0465 -> 0.0215**, **three-map Focus 0.0516 -> 0.0324**.
Log's already-small distinct list is 0.0055 -> 0.0048, below a useful individual
process-CPU claim. Kept for the measured populated-history gain and fewer
allocations. Raw pairs are `ui6-{before,after}-release-*-loaded.json`.
Shared-area/detached-chat/member selection tests pass in the 15-case acceptance
suite; inspected Chat/Focus images retain their labels, filtering and order.

### Poll report/event revisions without materializing payloads

`live_view.rs:139` uses the coordinated `LiveStore::revision()` API instead of
serializing status just to obtain boot/sequence. It still explicitly expires
commands as the old status call did. Chat/Log read `event_cursor()` instead of
querying/cloning an event for its cursor. `live_focus.rs:519` resolves identities
on report revision changes, skips unchanged history reads, and materializes
status/results only when a new successful command needs an acknowledgement echo.
Store replacement, boot handling, history bounds and receipt-time matching stay
unchanged. This is entirely a UI consumer change; Astra2 owns the core getters.

Release mean ms/pass, before -> after: **Focus update 0.03895 -> 0.00443**;
**three-map Focus update 0.04047 -> 0.00419**; moving-view report stamp
**0.01377 -> 0.00375** (three maps 0.01168 -> 0.00256, summed per UI pass).
Kept for the isolated gains; whole-frame poses/rendering still dominate.
Populated Focus retains 500 rows, all cases queue zero commands, and reply
acknowledgements still appear in the Focus regression. The full guarded gate
and 20 installed-resource/window app cases pass. The rendering cap is unchanged.

### Combined follow-up and limits

All eleven after controls pass on the same diagnostic binary. The two minimized
Full attempts were overwritten by the valid final sample. No candidate was kept
without an isolated gain; no renderer/pose approximation or frame-rate reduction
was introduced. Whole-process values include unrelated scheduling/render variance:

| Case | Release CPU % before -> after | Primary redraw Hz before -> after |
| --- | ---: | ---: |
| moving | 43.53 -> 51.49 | 22.54 -> 24.83 |
| effects-full | 46.08 -> 48.68 | 24.96 -> 24.90 |
| chat-loaded | 56.79 -> 46.50 | 24.69 -> 24.97 |
| log-loaded | 55.22 -> 47.53 | 24.43 -> 24.89 |
| focus-loaded | 50.86 -> 48.25 | 24.76 -> 24.29 |
| areas-loaded | 65.85 -> 66.26 | 23.23 -> 23.30 |
| legend | 48.47 -> 47.44 | 24.90 -> 24.90 |
| macros | 51.28 -> 46.71 | 24.90 -> 24.90 |
| panel | 55.86 -> 49.29 | 24.17 -> 24.89 |
| static | 0.73 -> 0.31 | 0.00 -> 0.00 |
| hidden | 0.62 -> 0.52 | 0.00 -> 0.00 |

Moving CPU rises while throughput recovers; Full CPU is +2.60 core-points at
essentially the same rate. These unmodified rendering cases show why the report
does not attribute all process-CPU differences to small UI changes. Multi-area
CPU is essentially flat. The isolated macro/history improvements above are the
supported gains; poses, particle work and renderer submission still dominate.
Keep the optimized demo profile. Larger rendering changes remain outside this
mg-ui-only pass, and the running demo was not restarted to claim owner acceptance.

Final gate: `tools/live/check.ps1 -Phase 8 -Next -Step opt-ui-final -CheckFmt
-Jobs 1 -NoUiDebug -UiUnoptimized` passes **1,641 workspace tests, zero failures,
27 ignored**, plus the separate mg-live package pass; workspace/all-target Clippy
with `-D warnings` and fmt check pass. Native engine cases remain guarded; this
is not a new connected-client acceptance. Focus/Chat/Scenes and the 15 acceptance
fixtures add 20 installed-resource/fake-store app passes. Scripts parse cleanly.
The gate tested the exact Rust content subsequently committed as the three
optimizations; later edits are documentation/cleanup only.

### Reproduction, retention and cleanup of this pass

Retain the original replay JSON, demo MOD/resource inputs, 33 final metric JSONs,
build receipts, final guarded logs and the minimal before/after picture set in
`live-opt-ui`. Rebuild the diagnostic with `Run-GuiMeasure.ps1` **without**
`-SkipBuild`; its disposable binaries were removed after verification. Generated
macro/watch and tiny area fixtures are reconstructible from their helpers/tests.

Actual removal: **24 owned files, 501,551,240 logical bytes (478.32 MiB)**: eight
diagnostic executables/PDBs/measure-only libraries, fourteen redundant pictures,
the temporary reverse/apply patch and the obsolete mixed shader/scene fixture.
The 23 manifest entries reported **501,600,256 allocated bytes**; allocation of
the separately removed 1,728-byte source fixture was not measured. No free-volume
delta is attributed on the shared disk. Required proof pictures and the stronger
scene test remain; ordinary shared Cargo caches and peer files were not removed.

Automatic approval review rejected both the manifest-based cleanup and a narrower
recursive diagnostic-cache cleanup as **"blocked by policy"**, without a specific
reason. Fixed-name nonrecursive file removal succeeded. **779 files / 349,533,583
logical bytes (333.34 MiB)** remain, fully itemized with SHA-256 and status in
`target/test-output/live-opt-ui/cleanup.json`. The main remaining directories are:

- `target/debug/incremental/mg_ui-1guc6rbdf0b83`: 313,596,931 logical bytes; an
  object hash exactly matches the owned measure-only rlib (receipt retained).
- `target/debug/incremental/live_measure-1ljdq6sxf816a`: 35,728,845 logical bytes.
- The other 207,807 bytes are fingerprint/dependency metadata, generated fixtures
  and empty captures listed individually. No blocked deletion is claimed done.

The owner/coordinator's app build was already active at task start; its observed
Release changes are recorded in the ledger. This task invoked no demo build or
lifecycle command. Authorized workspace tests rebuilt debug test dependencies.
No claim is made that all app files retain the initial hashes.

Session `01a12fe8-1458-7551-99fc-b277832def84` remains active; its history path in
the ownership ledger was 577,031,301 logical bytes at the cleanup checkpoint.
It was neither deleted nor rewritten and is registered for separate supported,
owner-authorized retention after closure. No supported history cleanup was run.
Canonical solution and limitations are this report plus the look/Focus records
in [Live windows](live-windows.md#visual-look-decoding-defence-2026-10-15).


## Stress replay T1-T3 (2026-10-15)

This section records the larger GUI replay work. The completed baseline and
matched follow-up measurements below supersede the preflight timings. The
earlier 218-object numbers above do not establish a 1,000-object session limit.

### Diagnostic contract and reproduction

`tools/live/Run-GuiStress.ps1` builds only the opt-in `live_measure` example via
`check.ps1 -MeasureGui -MeasureProfile Release -Jobs 1 -NoUiDebug`. It never
builds, starts or stops the demo applications. Inputs are Astra2's closed gzip
JSONL trace and matching MOD (stock resources, no HAK), authenticated by SHA-256.
The reader streams `{at_ms,channel,data}` envelopes in timestamp order, merging
the optional draft/decision sidecar. It uses the real store, scope checks, draft
approval, event queries, scene renderer and windows, without opening a network
socket or sending commands. Approved replay command IDs are translated from the
sidecar's `native_id` to the newly allocated local ID before consuming captured
acknowledgements. Missing mappings are not assumed to be delivery proof.

The helper opens five areas: two visible side by side, the rest in tabs, plus
Chat, a detached area Chat, Focus (five watches and a 50-member cross-area group),
and rotating Log/Panel/Selection panes. The first area's 50 creatures are
selected. Effects is Full. All five Live previews are visited before timing;
the normal unlit-view setting makes its geometry inspectable. The fixture's
ARE JSON omits lighting fields; the lit view was black in the smoke. The module and game installation are read-only;
APPDATA, NWN_HOME and cards/notes/focus/memory use the task's isolated directory.
The screenshot records the actual layout, not a mockup. The isolated fixture
also warns that its custom `mgt_polish` talk table is absent: reported chat text
and stock models are present, but this is not custom-TLK/HAK compatibility proof.
A restored control loads
the saved watches/cards/notes and preloads the capture, then reconstructs the
same diagnostic layout; this is not proof of production session restoration.

Run in PowerShell 7, with no concurrent compiler/timed native benchmark:

```powershell
./tools/live/Run-GuiStress.ps1 -Replay <closed-replay.jsonl.gz> `
  -Drafts <closed-drafts.jsonl> -Module <matching-events.mod> `
  -Seconds 1800 -Tag baseline
# Reuse the same binary/input for the persisted-state startup control:
./tools/live/Run-GuiStress.ps1 -Replay <closed-replay.jsonl.gz> `
  -Drafts <closed-drafts.jsonl> -Module <matching-events.mod> `
  -Seconds 60 -Tag restored -Restore -SkipBuild
```

Minute JSONL contains process CPU (100% = one logical core), private/resident
bytes, cache entry counts, file sizes, query cost and stage distributions.
Fixed 2,048-bin logarithmic histograms bound p50/p95 error to 1% + 1 us; maxima
and sums are exact. Stage samples are summed inclusive wall time per UI pass,
so nested stages overlap and cannot be added into an exclusive CPU breakdown.
Renderer submission spans are CPU time, not hardware GPU duration. The process
CPU includes the replay/parser and sampling threads; census/query overhead is
recorded separately. Minimized or failed runs are rejected. Captures may loop
only with monotonic sequence offsets, same boot/object IDs and an explicit loop
count: repeated traffic/outages are not additional unique native coverage.

For shorter optimization comparisons, the analyzer's `--first-minutes 5` uses
the same first five complete minute intervals from each run. Means are weighted
by their sample counts and CPU by interval duration. Its reported p95 range is
the range of the five minute p95 values, never a falsely pooled percentile.
The full-run summary retains the actual full histogram's p50/p95/max. Check the
aggregation with `python tools/live/test_analyze_gui_stress.py` (two tests).

The production ring keeps 1,000 events per area and 500 per query. Therefore a
five-area trace can process far more than 10,000 events while retaining only
5,000 area events (plus any area-less log ring). A separate synthetic 12-area
control measures queries over more than 10,000 retained events and conversation
memory eviction/restoration. Fake PC turns in that control are not connected
client evidence. Native capture kinds, counts, gaps and real-player coverage
must be listed with the final results, not inferred from the fixture's intent.

Task ownership and artifact lifecycle are tracked in
`target/test-output/live-gui-stress/owned-files.json`. One 2 MB immutable MOD
snapshot protects replay lineage while the peer rebuilds the shared native
fixture; it replaces the previous snapshot, rather than accumulating modules.


### Harness verification

Three diagnostic tests pass: bounded quantile accuracy, report/delta/fast-event
identity across repeat offsets, and real scope/edited-draft acknowledgement
mapping. Diagnostic Clippy with `-D warnings` passes. The installed-resource
10-second smoke shows all five populated Live caches (200 extras each), six
Focus entries and the 50-object selection, with no replay error. Its preliminary
draft sidecar lacks approved native IDs, so that smoke explicitly rejects drafts
instead of claiming faithful approved-draft delivery. The final native capture
adds those IDs. This smoke overlaps the native benchmark and is not a CPU
baseline. Superseded smoke pictures are disposable once the sustained before/after
proof is retained; the same smoke output names were reused during preflight.

The separate synthetic control successfully processes 30,000 addressed turns:
retained event rows reach 12,000 (12 real capped rings), conversation memory
stays at 10,000 turns across 60 pairs, and reloading the persisted memory verifies
the same 10,000 turns. Its three persisted content files total about 2.33 MB.
Repeat its performance sample only after the native timing interval is clear:

```powershell
# After the guarded Release diagnostic build above; no GPU or network involved.
$env:MGLIVE_STRESS_CONTROL = "$PWD/target/test-output/live-gui-stress/query-control"
try { & ./target/release/examples/live_measure.exe }
finally { Remove-Item Env:MGLIVE_STRESS_CONTROL }
```

The control's local IDs are valid hexadecimal game-object IDs. Batched synthetic
events use state reports: the actual fast-event endpoint requires exactly one
event. These preflight errors were fixed in the helper; neither was a production
failure. Hidden tabs are lazy in the real UI, so the startup driver explicitly
visits each before timing rather than treating an open tab as an initialized
Live preview. No production optimization has been made in this harness step.


The follow-up native-trace preflight completes 130 seconds with all five caches,
all Chat/Focus/Log/Panel/Selection spans, minute samples, and 194 actual recorded
draft submissions/decisions with no replay error. Watches now choose frequent
speakers from the trace, so five individual conversations and the group are
populated. `analyze_gui_stress.py <result.json> --output <summary.json>` reduces
minute counts and memory drift; its slope units and actual 1,000-preview smoke
bounds were checked. Drift is observational, not proof against all leaks.

One earlier 120-second attempt produced no final sample and is excluded; its
cause was not identified. The helper now writes an independent completion
checkpoint with the error, last UI activity and OS minimization state, and the
runner records the process exit code. A later valid run does not retroactively
validate the failed attempt. The completed sustained run below provides the
stronger evidence.

Normal-feature preflight: 1,699 workspace tests pass, none fail, 27 are ignored;
all-target Clippy with `-D warnings` and formatting checks pass. An earlier
attempt could not relink the integration test while Astra2 was executing it
(Windows LNK1104). No process was stopped. Her following run uses its own
ledgered service host, which released the shared test executable for this gate.
The final workspace gate after the optimizations is recorded below.

Final gate (2026-10-16, after 0ef8f17, 293951d, 16d4df9): `cargo test --workspace
--exclude mg-corpus-tests --lib --bins --tests` 1,555 passed, 0 failed, 23 ignored;
`clippy --workspace --all-targets -D warnings` and `fmt --check` clean. The soak
stopped at minute 21 of 30 when the session ended (private memory flat at about
1,000.8 MiB, all 1,000 objects present); its last nine minutes and the Full-effects
control were not rerun.

The diagnostic counts effect models, particles, transient cues and bubbles only
when a view submits a picture. It automatically captures the first submitted
model-backed effect, rather than assuming Full mode or a native event alone
proves that effect was drawn. Short native effect tails do not establish a
sustained high-effect-count workload.

### Closed native input and timing boundary

The reserved GUI baseline starts after Astra2 released native timing and Cargo
at 23:37 on 2026-10-15. It uses her closed `live-stress/final/replay.jsonl.gz`
(7,352,639 compressed / 101,820,155 raw bytes), SHA-256
`f98f63fb149ee5c92c6af5a8c67423bbe67f0d5ee2cb4dc7b647ad24ba37669b`, and
`drafts.jsonl` (1,078,924 bytes), SHA-256
`6daca1574f0add56ec5ed40ec55f9f1fff883a5f22992e83525b00e8de641be1`.
The single pinned MOD is 2,029,122 bytes, SHA-256
`663e56437f0c9ba8820fa67de3406d1bd33699b529e545b27f4b35812bf8885c`.
The task's `input-summary.json` and per-run owner receipts preserve this lineage.

Five areas contain 250 creatures and 750 placeables, with six large uncached
look surrogates; no real player is connected. There are 84,635 unique captured
native events: 54,906 chat, 14,461 patrol, 14,200 damage, 986 attack and 82 dialogue
events. The sidecar has 2,836 drafts and 2,836 recorded decisions. These rapid
approvals do not simulate a large queue waiting for human review. Effect row 7
appears about 1,795.7 seconds after priming; rows 411/412 follow in the final
tail. The 1,810-second GUI interval includes this short visual control, with
Full selected throughout.

This is **observer-captured input replay**, not lossless service delivery proof.
Astra2's independent audit found 648 missing native events around subscriber
reconnection and 12 at shutdown, although the faster observer captured all
84,635 with no holes. Replaying that observer capture cannot validate the
bridge's delivery guarantee. See [native stress results](live-game-performance.md).

Hardware: Intel Core Ultra 7 265F (20 logical processors), NVIDIA RTX 5070,
Vulkan, NVIDIA driver 617.42. CPU percentages are relative to one core. The
owner's demo remains running; no competing compiler/native benchmark was
scheduled during the GUI interval. Ordinary desktop load is not controlled.
Before the baseline, four disposable preflight watch/card/note/memory files
were removed (24,858 logical bytes, hashes in the ledger). They are regenerated
from this fixed input, so old fixture IDs do not accumulate in the baseline.

### Completed GUI baseline (2026-10-15/16)

`baseline.json`, its minute JSONL, `baseline-summary.json`, owner/driver receipts
and three PNGs in `target/test-output/live-gui-stress` record a successful
**1,810.069-second** run, with no repeats, replay errors or observed minimization. The
two maps submitted 37.84 scene pictures/s in total; the UI ran at 21.91 passes/s.
Process CPU averaged **82.63% of one core**. The renderer counter observed one
attached effect model and a peak of 212 live particles during the short tail;
the automatic effect image was inspected. This is not a sustained 212-particle
or 16-effect limit claim.

Inclusive stage wall times, in ms per UI pass where that stage ran (two visible
maps/Chat windows are summed within a pass; nested spans overlap):

| Stage | Mean | p50 | p95 | Max |
| --- | ---: | ---: | ---: | ---: |
| Entire UI | 37.078 | 39.621 | 61.387 | 706.094 |
| Chat, both panes | 21.934 | 30.589 | 39.621 | 557.185 |
| Log | 5.220 | 7.445 | 9.454 | 85.296 |
| Panel | 3.062 | 2.892 | 4.057 | 50.810 |
| Focus | 0.908 | 0.876 | 1.357 | 38.594 |
| Selection | 1.602 | 1.485 | 2.021 | 38.724 |
| Live poses | 2.488 | 2.347 | 3.010 | 56.501 |
| Particles | 2.021 | 1.904 | 2.442 | 71.156 |
| Overlay | 0.398 | 0.357 | 0.582 | 38.488 |
| Report access | 0.132 | 0.004 | 0.020 | 41.902 |
| Reconcile, on changes | 0.244 | 0.249 | 0.333 | 6.960 |
| Looks, on changes | 0.501 | 0.491 | 0.676 | 21.169 |
| Scene assembly | 0.090 | 0.084 | 0.116 | 18.639 |
| Encode/submit | 2.090 | 1.981 | 2.567 | 50.357 |

Private memory rose from 927.46 MiB at minute one to 994.92 MiB at the end;
resident memory from 471.14 to 546.68 MiB. Private memory stayed within
**993.40–994.92 MiB over the last ten minutes**. Its OLS drift after minute five
was +1.10 MiB/min, including earlier warm-up and the resynchronization; this
does not establish an unlimited-duration leak-free guarantee. All minute
samples retain 1,000 extras and 250 creature looks, with no bad looks. Events
cap at 5,000, seen game IDs at 4,096, Focus rows at 3,000, result rows at 64.
Five cards and 50 notes remain fixed; persisted files finish at 20,957 bytes
(the final memory flush adds an empty 25-byte memory file). Native memory turns
are zero because there are no real PCs in this trace.

Two approved commands remain in flight at the measurement cutoff; zero pending
drafts were observed at minute samples. Do not infer zero transient drafts or
complete draining from these snapshots. Accepted command/ownership records
reach 5,672; they intentionally persist for idempotency, with a 65,536-operation
session limit in `store.rs`. One resync occurs around the captured Redis gap and
is complete afterward. This checks the replay's state recovery, not the real
subscriber's missing-event count.

Warm-resource startup: scan 1,445 ms, prime 43 ms, resource setup 32 ms, module
open 2 ms; first Live frame 2,597 ms and all five views ready 3,077 ms. The first
frame/all-ready clocks include the scan, startup and initial rendering. The
desktop's `apps/moonglow/src/main.rs` saves Settings and the last area; the
diagnostic's restored control reconstructs the multi-area layout rather than
claiming a production full-session restore.

### Ranked changes to measure

This is the initial ranking; the first three measured changes are retained below.

1. **Resolve event display objects once per refresh** in Chat/Log. Baseline
   `live_chat.rs:201–220` resolves speakers/portraits repeatedly via `locate`,
   cloning their full looks; `live_log.rs:378–400` repeats this. Chat already
   spends 21.93 ms/pass, Log 5.22. Expected gain: several ms/pass; low risk if
   the short-lived cache preserves names, role colours, portraits and missing-ID
   fallback. Small implementation plus focused UI tests and a matched replay.
2. **Stop event queries after the newest matching page**. `events.rs:256–301`
   scans all retained rows before limiting, including exhausted tail queries
   and text searches. Expected gain: substantial for tail/text queries; smaller
   for the normal 500-row window. Moderate risk: scope, order, truncation and
   eviction semantics require comparison against a full-scan reference.
3. **Avoid Panel's unchanged report clone**, `live_panel.rs:281`. Its whole
   window costs 3.06 ms/pass, so the maximum possible gain is much smaller.
   Source/boot/area invalidation must remain correct. Measure after the first
   two changes before retaining another cache.
4. Variable-height Chat virtualization is a later candidate if layout still
   dominates. It carries more risk to wrapping, grouped portraits, selection
   and scrolling; no row truncation or appearance change is justified here.

The matched first five baseline minutes are 82.55% CPU, 25.00 UI passes/s,
31.970 ms UI, 18.715 ms Chat, 4.338 ms Log and 2.876 ms Panel. Candidate prefix
comparisons use those same captured events and layout rotations, not the
different workload average of the whole 30 minutes.

The separate restored-data control completes 60.07 seconds without error:
scan 1,436 ms, preload of the full capture 5,055 ms, first Live frame 7,648 ms,
all five views ready 8,217 ms. This includes a full event/state replay preload,
so it is not comparable to loading an empty store or to cold disk startup.

The isolated 12-area synthetic baseline processes 30,000 addressed turns and
retains 12,000 events plus 10,000 conversation turns across 60 pairs. The latter
reload successfully. At 12,000 retained events, 100 query repetitions give the
following means (ms): all 1.7148, chat 1.6016, common text 27.6983, one area
0.9454, one object 0.4808 (200 rows), missing object 0.1001, last-eight tail
0.0688, last-eight with text 0.1211, empty tail 0.0689. Results are in
`query-before.json`; these are local store-query times, not API latency. The
three generated content files are reset before controlled comparisons, while
the compact result JSON is retained.

The first 305-second cache-candidate attempt is excluded: its independent
checkpoint records the diagnostic minimized, with the last UI pass at about
69 seconds. Input ingestion continued without error, and the completion guard
exited with code 2. `invalid-people-minimized.json` retains the compact receipt;
the same working outputs are reused for the retry. The diagnostic must remain
unminimized and at the fixed size throughout a timing comparison. This does not
identify the cause of the earlier preflight failure without a checkpoint.

### Follow-up 1: event display lookup cache

`live_event_people.rs` resolves each object at most once while rebuilding one
Chat/Log page. It retains only the display name, portrait resref/appearance and
speaker role, then is discarded. Full reported GFF looks are no longer cloned
for every speaker/portrait/name use. Event filtering, page size, grouped
portraits, role colours and the existing event-cursor refresh policy are
unchanged. New refreshes see renamed, removed or reappearing objects.

The valid 305.12-second `people` run passes all replay/layout checks; compare
its first five complete minutes with the identical baseline prefix:

| Metric | Before | After |
| --- | ---: | ---: |
| Process CPU, % of one core | 82.55 | 59.98 |
| UI passes/s | 25.00 | 37.54 |
| Summed scene redraws/s | 38.77 | 44.97 |
| UI mean, ms/pass | 31.970 | 13.877 |
| Both Chat panes, ms/pass | 18.715 | 3.312 |
| Log, ms/pass when shown | 4.338 | 1.292 |
| Panel, unchanged | 2.876 | 2.681 |
| Focus, unchanged | 0.741 | 0.711 |

CPU falls 27.4% while the UI draws more often. This is a five-minute performance
comparison, not a substitute for the final sustained verification. Prefix
JSONs and binary/input receipts are retained. `baseline.group.png` and
`people.group.png` were inspected: portraits, names, colours, selection and
window layout remain consistent (timestamps and live traffic naturally differ).

Checks: the new lookup test verifies repeated resolution, fallback names,
portrait/appearance, NPC/player/DM/possessed roles and refresh after rename or
removal. It passes, as do the Chat app test, Log app test and two Log unit tests;
all-target mg-ui Clippy is clean. The earlier minimized attempt is not used in
these numbers.

### Follow-up 2: bounded newest-event queries

`EventLog::query_page` in `crates/mg-live/src/events.rs` now merges the newest
matching rows of the ordered area rings, stopping after the requested page
and one lookahead. Exhausted cursors return immediately. Scope, case-insensitive
area selection, object/kind/text filters, ascending output and conservative
eviction/truncation flags are preserved; no retention limit changes.

The identical synthetic control at 12,000 retained rows gives these mean local
query times (100 calls each, ms; `query-before.json` / `query-after.json`):

| Query | Before | After |
| --- | ---: | ---: |
| All, newest 500 | 1.7148 | 1.2789 |
| Chat, newest 500 | 1.6016 | 1.2797 |
| Common text, newest 500 | 27.6983 | 1.7665 |
| One area, newest 500 | 0.9454 | 1.3571 |
| Matching object, 200 rows | 0.4808 | 0.4503 |
| Missing object | 0.1001 | 0.0984 |
| Last eight | 0.0688 | 0.0155 |
| Last eight with text | 0.1211 | 0.0248 |
| Exhausted tail | 0.0689 | 0.0001 |

A repeat after the GUI interval (`query-repeat.json`, same event generator;
existing persisted memory was reloaded) gives 0.7516/0.7141/1.1720 ms for
all/chat/text and 0.7623 ms for one area. Thus the initial one-area slowdown
is not consistent across samples; no one-area speedup is claimed. Sparse or
missing matches still require scanning the retained rings. The clear retained
gains are bounded common-text work and tail queries, not every possible filter.
The synthetic memory remains capped at 10,000 turns and reloads successfully.

The 305.11-second `queries` GUI run passes the same layout/replay checks. Its
five complete minutes measure 57.26% CPU versus 59.98% after the display cache,
36.36 versus 37.54 UI passes/s, and UI mean 14.650 versus 13.877 ms. Wall-time
spikes also rise in unchanged renderer/Panel paths, so this small process-CPU
difference is not presented as a proven interactive-frame improvement. The
query microbenchmark is the reason to retain the change. The group screenshot
was inspected: the same names, portraits, selection and area tabs remain.

Checks: all 158 core tests pass (four ignored), including 600 combinations
against an independent exhaustive query reference and a bounded-work assertion;
all-target mg-live Clippy passes. A measurement-only Panel subspan attributes
0.998 ms/pass of its 2.764 ms window total to report copying in this prefix.
This separately motivates the next cache experiment; it changes no behavior.

### Follow-up 3: Panel report cache

`LivePanel::report` keeps one `Arc<AreaLive>` and refreshes it on source, boot,
area or report revision changes. It owns a separate cursor from the map, so
opening/drawing Panel cannot consume an update needed for map reconciliation.
Its bound is one area report. Sorting/filtering/selection/follow logic is unchanged.

The matched first five minutes of the 305.12-second `panel` replay compare with
`queries` (only this cache differs): process CPU **57.26 -> 53.93%** of one core,
UI **14.650 -> 12.793 ms/pass**, UI passes **36.36 -> 38.16/s**, summed scene
redraws **44.11 -> 45.29/s**. Panel falls **2.764 -> 1.250 ms/pass**; its report
access falls **0.998 -> 0.0287 ms/pass**. Every minute's report-access p95 is
below 0.001 ms after caching; occasional new reports still pay the clone cost.
The direct subspan confirms the gain even though desktop wall times fluctuate.
The group screenshot was inspected against the earlier prefixes, with unchanged
portraits, names, filters, area tabs and selection layout.

Four Panel unit tests and both installed-resource Panel app tests pass. The new
unit case covers unchanged-report reuse, rename, area switch, reboot, missing
report and replacement source with the same boot/sequence. The app gate found
an unrelated stale E2 expectation left after `b39d00e`: it expected following
to stop at an unopened area. Test correction `b83789e` instead checks Stop
following and verifies that a previously unopened destination opens and follows
correctly, then stops on reboot. No production follow behavior changed.

The final guarded gate passes: **1703 workspace tests, 0 failed, 27 ignored**; separate core/integration stage 162 passed, 4 ignored. Workspace all-target Clippy (`-D warnings`) and `fmt --check` pass. Command: `tools/live/check.ps1 -Phase 8 -Next -Step gui-stress-final -CheckFmt -Jobs 1 -NoUiDebug -UiUnoptimized`. No app/demo build or lifecycle command. Installed-resource app checks above are separate from this NWN_ROOT-guarded gate.
