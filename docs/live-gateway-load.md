---
type: Reference
title: 'Live gateway load test and tuning'
description: A reproducible Docker load test of the Live gateway core (30 to 120 desktops, 10 AI agents, 50 areas, Redis restarts), the bottlenecks it found and fixed with before/after numbers, and deployment notes for the Ubuntu 22.04 production box.
tags: [live, gateway, load, performance, deployment, redis]
generated: { by: claude, at: 2026-10-19T00:00:00Z }
sources: [../tools/live/gateway_load/run.py, ../tools/live/gateway_load/game.py, ../tools/live/gateway_load/clients.py, ../crates/mg-live/examples/gateway_bench.rs, ../crates/mg-live/src/store_gateway.rs, ../crates/mg-gateway/src/socket.rs, live-gateway-protocol.md]
---

# Live gateway load test and tuning

The gateway core (`mg-gateway` over `mg-live`: `/ws/v1` for DM desktops, `/agent/v1` for AI
agents) was run under the load of an evening as the production box will see it and tuned until it
met these targets on `--cpus=6`: area-push p95 below 150 ms, command round trip (the gateway's
part) p95 below 300 ms, average gateway CPU below 2 cores, a flat resident size, no lost or
repeated results, and a knee above 100 desktop sessions.

## Result

On `--cpus=6` the evening scenario meets every target, and 120 desktop sessions (the dev host's
connection limit is 128) still do: p95 86.1 ms and 0.93 cores at 120. The one qualification
is the resident size, which is flat (+1.6 % in 10 minutes) once the event log is full but grows
while an empty gateway fills it (see "Memory"). Before the work the gateway could not run
the scenario at all: the unmodified code deadlocked within a minute, and with that fixed it
still failed the p95 target at 30 desktops, used 1.5 cores on average, jumped to 1.6 GB
resident when the gateway restarted, and with a 4 GB cap it died within half a minute of the
ramp reaching 120 sessions (2.4 GB resident and climbing at its last sample; the container was gone).

| 30 desktops + 10 agents, 10 min, Redis and gateway restarted | target | before | after |
|---|---|---|---|
| area push, p50 / p95 / p99 (ms) | p95 < 150 | 97 / 195.5 / 312.2 | 41.1 / 55.5 / 122.7 |
| command round trip without the game's wait, p50 / p95 / p99 (ms) | p95 < 300 | 106.1 / 217.7 / 329.6 | 42.5 / 58.3 / 112.1 |
| chat event to desktop, p50 / p95 / p99 (ms) | | 28.5 / 87.2 / 159.2 | 8 / 16.1 / 41.5 |
| agent long-poll wake, p50 / p95 / p99 (ms) | | 3.3 / 25 / 67.6 | 2.9 / 10.3 / 12.2 |
| gateway CPU, cores (mean, peak of 5 s) | mean < 2 | 1.54 (peak 3.61) | 0.29 (peak 0.32) |
| gateway resident size (MB, first -> last, slope) | flat | 371.9 -> 1655.7 (+178 MB/min) | 328.1 -> 516.4 (+12 MB/min) |
| results missing (sent, neither final nor unknown) / repeated | 0 / 0 | 0 / 0 | 0 / 0 |
| results ended `unknown` (in flight at the gateway restart) | allowed | 8 of 3802 | 3 of 3731 |
| longest gap without any area push after the Redis / gateway restart (s) | | - | 2.5 / 2.5 |

"Before" is `958ef84` (the revision with only the deadlock fix: without it the run stops
within a minute); "after" is the final tree. The command round trip also contains the
synthetic game's own wait for its next one-second report (p50 562.8 ms, p95
1019.2 ms in the end-to-end figure), which is subtracted in the figures above.


## What is measured

The system under test is `apps/mg-gateway-dev`, the loopback host of the same core the private
production binary wraps with identity; it is built in a `rust:1.98` container (release) and
run with `--cpus=6` and a memory cap (4 GB; the box has 128). Everything else is stdlib Python
in containers on a private Docker network (`tools/live/gateway_load/`):

* **Redis** `redis:7.4-alpine`, AOF on (`appendfsync everysec`).
* **`game.py`, the synthetic game**, drives Redis the way the game scripts do, without an NWN
  server: a state report on `<prefix>.state` every second (a full snapshot first, then deltas
  with `base_seq`, a full report every 30 ticks and a full area when it is first watched, the
  same rules as `mglive_report.nss`), the `watch_areas` and `resync` commands the gateway
  publishes on `<prefix>.cmd` (it reports exactly the areas the gateway asks for, plus three areas
  with players in them), results retained until their receipt is echoed, and durable events
  through the producer script `event-stream.lua`: 10 chat lines a second and a 5-second combat
  burst of 40 damage events a second every 30 seconds. 50 areas of 200 objects, five of them with
  1100; a creature in each area is a *probe* whose name carries the publish time, and every
  chat line carries its own, so clients measure latency from the raw frames.
* **`clients.py`**: DM desktops on `/ws/v1` (a hand-written WebSocket client: hello with
  resume, 3-5 watched areas of which 40 % are dense, a `focus.set` or `watch.set` every 10-30 s,
  a command about every 5 s (`npc_say`), acks every second, reconnect with resume) and AI
  agents on `/agent/v1` (a long poll of `live_events`, `live_find`/`live_status`/
  `live_area_state` calls, a command about every 10 s that the owning desktop approves with
  `draft.decide` as a DM would). Credentials are random per run and exist only in the
  containers.
* **`run.py`** starts and samples everything (cgroup CPU and throttling, resident size and
  threads of the gateway, Redis CPU, generator CPU), restarts Redis and the gateway at set
  times, and writes `target/test-output/gateway-load/<label>/summary.json`. The loopback-only
  dev host is reached from the clients through a shared network namespace (an idle
  `mgload-net` container owns it, so the gateway can be restarted without moving the clients).

Metrics: *area push* is the time from the synthetic game publishing a report (its probe
name) to the DM client having read the whole `area` frame; *command RTT* is `cmds` sent to the
`final` result push, and because the game answers in its next one-second report it also
contains the game's own wait, which the harness measures (command arrival at the game to the
publish of the report that carries its result) and subtracts: *RTT minus game wait* is the
gateway's share (client to Redis and back); *agent wake* is the time from a chat event
being produced to the agent's long poll returning it. Samples taken in the first 60 s and in
the 5 s before to 45 s after a restart are left out of the percentiles; the restarts are
measured on their own (`longest_push_gap_s`).

Not covered: TLS and Caddy, a real game server's timing, a real network, and real DMs'
patterns. The host that produced the numbers below is a desktop Core Ultra 7 under Docker
Desktop (WSL2); its cores are about twice as fast as a 2016 Broadwell core, so CPU figures
should be doubled for the production box until the harness has been run on it (the
commands work unchanged on Linux).

## Reproduction

Docker with BuildKit and Python 3 are all it needs, on Windows, macOS or Linux (the images are
Linux); about 20 GB of disk for the Rust build cache.

```sh
# images: the working tree, and (to compare) the same gateway from an earlier revision
python tools/live/gateway_load/run.py build --tag after
python tools/live/gateway_load/run.py build --tag baseline --rev 958ef84   # only the deadlock fix

# the evening: 30 desktops + 10 agents, 10 minutes; Redis restarted at 4:00, the gateway at 7:00
python tools/live/gateway_load/run.py evening --tag after --label after-evening
# the ramp: 30 -> 60 -> 120 desktops, 4 minutes each (the first 30 speak, the others watch)
python tools/live/gateway_load/run.py ramp --tag after --label after-ramp
# the knee: the same ramp on one CPU
python tools/live/gateway_load/run.py ramp --tag after --cpus 1 --sizes 30,60,90,120 --stage-seconds 200
# resident size with a full event log, and the allocator setting
python tools/live/gateway_load/run.py evening --tag after --no-outages --warm 150 --env MALLOC_ARENA_MAX=6
# the baseline at 60 and 120 desktops needs more memory than the 4 GB default
python tools/live/gateway_load/run.py ramp --tag baseline --memory 12g --sizes 60,120 --stage-seconds 220
python tools/live/gateway_load/show.py after-evening baseline-evening      # tables
python tools/live/gateway_load/run.py down                                  # remove every mgload-* container

cargo run --release -p mg-live --example gateway_bench -- 120 20 focus     # in-process CPU per second
(cd tools/live/gateway_load && python -m unittest -q)                       # the tools' own checks
```

`run.py evening --ptrace` installs gdb in the gateway container and writes
`stall-stacks.txt` if the gateway stops using CPU for 15 s, which is how the deadlock below
was found. `--perf` lets `perf record -e cpu-clock` run inside it (combine with `--keep`, then
`docker exec mgload-gw perf ...`; `stacks.py` is a gdb-based sampler for when perf is not
allowed). `--env MALLOC_ARENA_MAX=6`, `--memory` and `--cpus` pass to the gateway container.
The gateway must be built from the same revision as the harness for `--tag` to mean
anything; `--rev` builds an earlier tree with the current Dockerfile.

## What the load found

1. **A deadlock** (fixed first, because without it nothing could be measured): the Redis
   bridge's command publisher holds the bridge mutex and takes the store lock to hand commands
   off, while `Gateway::pushes` (one call per desktop, 20 a second) and `LiveStore::status`
   read the bridge status, which takes the bridge mutex, while holding the store lock. With
   5 desktops and 2 agents the gateway stopped within a minute (all 32 threads blocked; gdb
   showed the pump in `bridge::status` and the publisher in `handoff_commands`). Every
   desktop, agent poll and Redis thread then waited forever. The status is now read before
   the store is locked; `lock_order_tests` runs the publisher's lock order against
   `pushes`/`status` and fails (by timeout) on the old code.
2. **Per-session copies and encodings of every area.** Each push cloned the whole
   `AreaLive` (1100 objects for a dense area) while holding the store lock, serialized it
   through `serde_json::Value` twice and regrew a multi-hundred-KiB string. A report
   therefore queued 30 sessions x 4 areas of work behind one mutex. An area is now copied once per
   revision and shared (`SharedLive`), its JSON is built once, and the frame is
   assembled around that text (`encode_server`, identical bytes).
3. **A `meta` frame on every poll.** `game.age_ms` and the bridge's counters were compared
   exactly, so every 50 ms poll of every session pushed a `meta` (600 a second at 30 desktops,
   then still 16 a second per desktop after the age was fixed, because the event queue's
   `applied` counter moves with every event). They now count only when they matter
   (see the protocol note in [live-gateway-protocol.md](live-gateway-protocol.md#5-pushes-gateway--client)).
4. **Work done and thrown away on every poll**: the presence list (a JSON value per online
   session, so quadratic in sessions) and the draft list were built and dropped unless due;
   `has a PC` was recomputed for every area on every poll.
5. **A gateway restart ate gigabytes.** A catching-up desktop took each page of 256 events
   from a copy of the whole retained log (up to 50,000 rows). After a restart all 30
   desktops reconnected at once: 200 pages each, resident size 0.5 GB to 1.6 GB in five
   seconds (2.3 GB with a 24,000-event log). Pages now merge the per-area rings without
   copying (`EventLog::rows_after`).
6. **Poll latency.** Pumps polled every 50 ms (0-50 ms of every push). They now wake when the
   Redis bridge applies data and poll every 100 ms for deadlines.
7. `live_find` lowercased three strings per object under the store lock (about 15,000
   objects at this size); it now compares without allocating.
8. **Focus lookups scanned every object of every area on every poll.** A desktop that follows
   an object (`focus.set`) is sent whichever area holds it, and the check was
   `AreaLive::find` over all 50 areas per session per poll. Found with `perf` inside the
   container (`run.py ... --perf`): 23 % of the gateway's CPU at 120 desktops, almost all of it
   `memcmp` of object ids. The ids of an area are now indexed once per revision.
9. **tungstenite zero-fills its read buffer (128 KiB) on every poll of the reader**, and the
   session task polls it whenever anything wakes the task: 5.6 % of the CPU at 120 desktops.
   Clients send small frames, so the buffer is 8 KiB.

## Results

### Ramp, 30 to 120 desktops

Four minutes per stage, 10 agents throughout; the first 30 desktops speak (6 commands a
second, below the gateway's own budget of 10) and the others only watch. "Before" at 60 and 120
desktops had to be run with a 12 GB memory cap: with the 4 GB cap of the other runs the baseline
gateway held 2.2 GB at 60 desktops and disappeared (every client reconnecting in vain) when
the stage to 120 began.

| desktops | | area push p50 / p95 / p99 (ms) | command RTT without game wait p50 / p95 (ms) | agent wake p95 (ms) | gateway cores mean (peak) | CPU throttled (s per s) | resident MB |
|---|---|---|---|---|---|---|---|
| 30 | before | 76.5 / 125.7 / 207.8 | 81 / 151.7 | 11.7 | 0.87 (peak 0.94) | 0.003 | 365.7 -> 447.4 |
| 30 | after | 45.5 / 60.8 / 125 | 46.9 / 62.6 | 3.9 | 0.29 (peak 0.32) | 0.0 | 305 -> 412.8 |
| 60 | before | 139.8 / 224.4 / 285.5 | 157 / 296.5 | 46.9 | 1.75 (peak 1.91) | 0.081 | 581.3 -> 687.2 |
| 60 | after | 50.2 / 64.8 / 138.1 | 50.5 / 65.7 | 3.9 | 0.48 (peak 0.52) | 0.0 | 519 -> 613.1 |
| 120 | before | 252.7 / 454.7 / 523.2 | 409.3 / 738.5 | 221.1 | 3.34 (peak 3.52) | 0.401 | 2951.4 -> 2957.9 |
| 120 | after | 56.4 / 86.1 / 166.5 | 60.1 / 99 | 4 | 0.93 (peak 1.09) | 0.0 | 781.8 -> 919.5 |

### The knee

The same ramp with the gateway limited to one CPU (`--cpus 1`), to see where it saturates:

| desktops on 1 CPU | area push p50 / p95 / p99 (ms) | command RTT without game wait p95 (ms) | agent wake p95 (ms) | gateway cores mean | CPU throttled (s per s) |
|---|---|---|---|---|---|
| 30 | 77 / 157.7 / 951.1 | 155.3 | 3.9 | 0.24 | 0.006 |
| 60 | 112.4 / 230.1 / 1000.8 | 238.9 | 24.9 | 0.4 | 0.052 |
| 90 | 125.6 / 243.1 / 1707.8 | 253.4 | 36.9 | 0.55 | 0.148 |
| 120 | 144.6 / 358 / 1766.7 | 559.9 | 97.3 | 0.72 | 0.395 |

On one CPU the area-push p95 crosses 150 ms between 30 and 60 desktops and the quota is nearly
exhausted at 120. This regime is noisy (one worker thread, a CFS quota that stalls every thread
for the rest of a 100 ms period once it is spent): the same final build measured 158 ms at 30 desktops
in the ramp above and 111 ms in a repeat, and the build before the last round of fixes 94 and 191 ms
at 30 and 60 desktops against the final build's 111 and 166 ms with 30 % less CPU. With six CPUs there is no knee below the dev host's 128-connection limit: 120
desktops cost 0.93 cores on average (1.09 at the busiest 5 s) and the p95 is
86.1 ms. The fit used for the deployment sizing is
`p95 = 40 ms + 2.8 ms x desktops / CPUs`.

### Restarts

In the final evening run Redis was restarted at 4:00 (0.5 s) and the gateway at 7:00 (0.8 s).
The Redis restart cost no session: the bridge reconnected, re-sent the watched set and the
longest silence on any desktop was 2.5 s. After the gateway restart all 30 desktops reconnected
within 2.5 s with a new epoch (`resumed: false`, mirror reset), all 10 agents saw
`reset: true` on their next poll and carried on, and 3 commands that were in flight ended `unknown` on
the desktop; the other 3728 of 3731 ended `final`, none was lost or repeated.

### Memory

* The resident size is about twice the live heap. After a 3-minute run, `malloc_trim(0)` (via
  gdb) took a 394 MB process to 198 MB; the rest was free memory glibc kept in per-thread
  arenas. `MALLOC_ARENA_MAX=6` (the number of cores) cut the creep by 60 % for about 10 %
  more area-push latency; `MALLOC_ARENA_MAX=2` cut it by 65 % and doubled the p50.
  The measurement behind these (30 desktops, 4 minutes, no restarts, an earlier build of the
  same delivery code): default 322 -> 424 MB (+25 MB/min), `MALLOC_ARENA_MAX=2` 174 -> 210 MB
  (+9), `=6` 204 -> 254 MB (+10), `MALLOC_MMAP_THRESHOLD_=131072` 233 -> 318 MB (+22).
* The final tree with `MALLOC_ARENA_MAX=6`, 30 desktops, 10 minutes, no restart, **from an
  empty gateway**: 199.9 -> 284.3 MB, +8.9 MB/min; area push p50 / p95 / p99 51.5 / 68.3 / 138.2 ms, gateway 0.33 cores. That is not flat, and the cause is the event log: it keeps the
  last 1,000 events of each area (up to 50,000 rows, the log the desktops replay), and an
  empty gateway fills it at 17 events a second for about 50 minutes. Each retained event costs
  about 3 KB of live heap (the bench counts it exactly) and 6-10 KB of resident size once
  the allocator's fragmentation is included.
* The same run **with the log already full** (150 s of 400 chat lines a second before the
  desktops connect, `run.py ... --warm 150`): 1080.1 -> 1097.3 MB, +0.3 MB/min; area push p50 / p95 / p99 45.4 / 61 / 128.7 ms, gateway 0.34 cores. Over 10 minutes the resident size moves by
  less than 2 %; the plateau is about 1.1 GB, which a 128 GB box does not notice, but a
  4 GB cap fills only after the gateway has had a bad few hours.
* A gateway restart no longer costs memory: before, resident size went from 0.5 to 1.6 GB in five
  seconds (0.6 to 2.3 GB with a 24,000-event log); after, the new process starts at
  about 300 MB and grows like the first one.

### Where the CPU goes

`perf record -e cpu-clock` inside the container (`run.py ramp --perf --keep`, 120 desktops), flat
profile, share of the gateway's samples:

| | after the first fixes (1.3 cores) | final (0.9 cores) |
|---|---|---|
| `memcmp` (focus lookups over every object of every area) | 23.5 % | 4.8 % |
| `memmove` (assembling and writing 70 MB/s of frames) | 7.8 % | 12.5 % |
| `malloc`/`free` family | 17 % | 21 % |
| `memset` (tungstenite zeroing its read buffer) | 5.7 % | gone |
| `AreaLive::find` | 2.1 % | gone |
| serde_json (an area's JSON, once per revision) | 3.1 % | 3.5 % |

What is left is the cost of moving 70 MB/s of JSON to 120 sockets: copies and allocations of
frames of 110 to 600 KiB, and the kernel's send path. Sending only the objects that changed
(protocol section 9) is the next lever; nothing in the store is a bottleneck any more.

### In-process cost

`cargo run --release -p mg-live --example gateway_bench -- 120 20 focus` (single thread, no
sockets: store ingest, `pushes()` for every session every 50 ms, frame encoding; the live heap is counted):

| sessions | | CPU per simulated second | of which `pushes()` / frames | meta frames per second |
|---|---|---|---|---|
| 30 | before (`958ef84`) | 243 ms | 32 / 202 ms | 600 |
| 30 | after | 87 ms | 14 / 66 ms | 2 |
| 120 | after | 111 ms | 35 / 67 ms | 6 |

(The "before" row ran without focus; with every session following an object the old code also
scanned every object of every area on every poll.)


## Limits that remain

* **p99 is 120-170 ms, not 50 ms.** Every 30 s the game sends a full report (about 3 MB here);
  the gateway parses it outside the store lock but applies it inside, and every desktop's
  pump waits for that. Splitting the apply by area would flatten it.
* **The command round trip is dominated by the game**: results ride the next one-second
  report, so the whole round trip is about 0.5 s median and 1 s p95; the gateway's part is
  below 120 ms p95. A game that flushed results as soon as it ran a command would feel
  faster than any gateway tuning can make it.
* **A new session still receives the whole retained event log** (up to 50,000 rows, about
  20 MB) as a series of `events` pushes. It no longer costs a copy of the log per page, but a
  gateway restart makes every desktop download it at once. A bound on the initial replay
  (the last N events, or a resume cursor that survives a restart) would be a protocol
  change.
* **Event numbers are not stable across a gateway restart.** The events derived from
  state changes (entered, left, combat start) are not in the durable inbox, so the
  restored log numbers differ from the previous process' by the derived events. An
  agent's cursor survives (the page says `reset: true` when it is ahead of the log), but it
  must clear what it remembers about seen numbers.
* **The global command budget is 600 a minute for everyone** (`GLOBAL_COMMANDS_PER_WINDOW`)
  and 60 a minute per client by default: 60 desktops speaking every five seconds already
  trip it (`rate_limited`, "Live global command limit exceeded"), by design, to protect the
  game's main thread. The ramp lets only the first 30 desktops speak.
* **Dev host ceilings**: 128 WebSocket connections (`Config::connections`), 16 concurrent
  agent requests (every long poll holds one), 1024 sessions. The production binary sets its
  own; the 16 agent slots matter first (10 agents polling and calling use 10-14).
* **Resident size** is about twice the live heap because glibc keeps freed pages in per-thread
  arenas (the blocking pool has dozens of threads); see "Memory". The event log is the only
  state that grows, and only until it holds 50,000 rows.
* **An agent command waiting when the gateway restarts has an unknown outcome**, as the
  protocol says for any command: in the final evening run 553 of 554 agent commands got their
  result; the 554th was in `live_wait` when the gateway went down (connection refused) and
  the harness' agent does not ask again. Desktops are covered by the id journal and `unknown`
  results (3 of 3731 commands, none missing).
* The Redis address is resolved once when the bridge starts; a Redis container that comes
  back on a different IP is never reached again until the gateway restarts. Docker gives a
  restarted container its old address on a user-defined network (the harness relies on it);
  a compose file can pin it with `ipv4_address`, or the gateway can be restarted with Redis.

## Deploying on the production box (Ubuntu 22.04, Xeon E5-1650 v4, 128 GB)

The box has 6 cores / 12 threads (Broadwell, AVX2, no AVX-512; nothing here needs it), so a
Linux CPU number below is a *logical CPU* unless it says "core". Hyper-thread siblings are
`cpu N` and `cpu N+6` on this model: confirm with `lscpu -e=CPU,CORE,SOCKET`.

### Docker Engine and Compose

```sh
sudo apt-get update && sudo apt-get install -y ca-certificates curl
sudo install -m 0755 -d /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] \
https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo $VERSION_CODENAME) stable" |
  sudo tee /etc/apt/sources.list.d/docker.list
sudo apt-get update
sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
sudo systemctl enable --now docker
docker compose version      # v2.x
```

Use the `json-file` log driver with `max-size`/`max-file` (below) so a chatty service cannot fill
the disk, and keep the Docker data root on the disk that also holds Redis' AOF.

### Kernel settings

`/etc/sysctl.d/90-moonglow-live.conf`, then `sudo sysctl --system`:

```text
vm.overcommit_memory = 1            # Redis: a fork for AOF rewrite must not fail under heuristic overcommit
vm.swappiness = 1                   # 128 GB; never page Redis or the gateway
net.core.somaxconn = 4096           # listen backlog (Caddy, the gateway, Redis' tcp-backlog 511 needs >= 511)
net.ipv4.tcp_max_syn_backlog = 4096
net.core.netdev_max_backlog = 4096
net.ipv4.tcp_keepalive_time = 60    # dead desktops and mobile links leave sockets quickly
net.ipv4.tcp_keepalive_intvl = 15
net.ipv4.tcp_keepalive_probes = 4
net.ipv4.tcp_slow_start_after_idle = 0   # WebSocket bursts after idle keep their window
fs.file-max = 1048576
```

* Transparent huge pages: Redis warns about them (latency spikes while copying on fork).
  `echo never | sudo tee /sys/kernel/mm/transparent_hugepage/enabled` at boot (a small
  systemd unit, or `transparent_hugepage=never` on the kernel command line).
* File limits: Docker's own daemon is unlimited; give each container an explicit
  `ulimits: { nofile: { soft: 65536, hard: 65536 } }` (gateway, Redis, Caddy). A desktop uses
  one descriptor, a game server connection or an agent poll one more; the dev host's
  128-connection limit and 1024-session table are far below that.
* Clock: `chrony` on the host. Event timestamps, result retention (10 min) and the
  gateway's `age_ms` all assume one clock.
* Redis: `appendonly yes`, `appendfsync everysec`, `save ""`, `maxmemory 2gb`,
  `maxmemory-policy noeviction`, `tcp-backlog 511`,
  `client-output-buffer-limit pubsub 64mb 16mb 60` (the gateway is a pub/sub subscriber of 1 Hz
  reports of up to several MB; the default 32 mb hard limit is too close), `io-threads 1`
  (default), no password-less published port (internal network only).
* Gateway container environment: `MALLOC_ARENA_MAX=6`. See "Memory" above; without it glibc
  keeps up to 8 arenas per logical CPU and the resident size crept 60 % higher in the
  measurements. A jemalloc/mimalloc build of the host binary is the better fix.
* Docker log rotation: `/etc/docker/daemon.json`
  `{"log-driver":"json-file","log-opts":{"max-size":"20m","max-file":"5"}}`.

### CPU budget

The game server's main thread is the one thing that must never wait for a CPU: NWN runs the
module's scripts, the AI and the Redis plugin's callbacks on it, and the report tick is
only as steady as that thread. Reserve one physical core for it and keep everything else off
that core, including the hyper-thread sibling.

| consumer | logical CPUs (this box) | compose | measured need |
|---|---|---|---|
| `nwserver` (main thread + helpers) | `0,6` (core 0 and its sibling, nothing else scheduled there) | host process: `taskset -c 0,6`, then move the main thread to `0` | one core, whatever the module does |
| Redis | `1` | `cpuset: "1"`, `cpus: 1` | 0.01 core in every run (single-threaded, AOF `everysec`) |
| gateway | `2,3,4,8,9` | `cpuset: "2,3,4,8,9"`, `cpus: 5` | 0.4 host cores at 30 desktops, 1.3 at 120 (about twice that on a Broadwell core) |
| OPUS-MT translator | `5,10,11` | `cpuset: "5,10,11"`, `cpus: 3` | bursty, CPU-bound |
| OS, Caddy, Docker, monitoring | `7` | unconstrained | |

Sizing the gateway: delivery latency is set by the burst, not by the average. A report fans
out to every desktop at once, so the area-push p95 follows `40 ms + 2.8 ms x desktops / CPUs`
(CPUs in units of this host's cores; fitted to the `--cpus 1` ramp and checked against the six-CPU
ramp: 96 ms predicted, 86 to 107 ms measured at 120 desktops). A Broadwell thread pair is worth
about 1.25 Broadwell cores and a Broadwell core about half of this host's, so the five
logical CPUs above are roughly 1.5 host CPUs: that holds a p95 under 150 ms up to about 60 desktops,
and 100 desktops need roughly 2.6 host CPUs (eight logical CPUs). That is an estimate from a
faster machine; run `run.py evening` and `run.py ramp --cpus 5` on the box before relying on it.
The lever beyond CPUs is the protocol: an area is pushed whole (about 110 KiB for 200 objects,
600 KiB for 1100) to every desktop watching it at every revision, and a delta of the changed
objects (live-gateway-protocol.md section 9, "Later") would cut the fan-out roughly tenfold.

To keep the rest of the system off the game server's core: `systemd` slices with
`AllowedCPUs=1-5,7-11` for `system.slice`, `user.slice` and `docker.service`, or
`isolcpus=0,6` plus `nohz_full=0,6` on the kernel command line if even kernel threads should
stay away (then `taskset` the server explicitly). The translator is the only thing that can use
all the CPU it is given for seconds at a time; `cpus: 3` plus a `cpuset` that excludes the
game's core is what keeps it from stealing the main thread's time. Do not give the gateway more
than it needs: when the translator and the gateway share a hyper-thread pair the gateway's tail
latency doubles.

### What to monitor

* Gateway: process CPU and `cpu.stat` `nr_throttled`/`throttled_usec` of its cgroup (any growth
  means the quota is too tight), resident size (a drift of more than a few MB per minute after
  the first hour), thread count, open sockets (`ss -s`), session count, and the 4xx/5xx and
  `bye` codes (`overloaded`, `idle`, `revoked`) in its log.
* Delivery latency: a probe desktop (or the load tools' `clients.py` with one session) that
  prints publish-to-push latency; alert on p95 above 150 ms or a `meta.game.age_ms` above 3 s.
* Redis: `redis-cli INFO` `used_memory`, `aof_last_bgrewrite_status`, `aof_delayed_fsync`,
  `rdb_last_bgsave_status`, `latest_fork_usec`, `connected_clients`,
  `redis-cli --latency` (> 5 ms is a problem), and the events stream backlog the gateway
  reports in `live_status.redis.events.backlog`.
* Game: report age (`live_status.ageMs`), `dropped` events in the status, the
  server log's `TMI`/script errors.
* Host: `vmstat` run queue and steal, `iostat` for the AOF disk, free disk (AOF, the event
  inbox in the gateway's data directory, Docker logs), `dmesg` OOM kills, chrony offset.


## Verification

* `cargo +1.98.0 test --workspace --exclude mg-corpus-tests --lib --bins --tests` passes (1891
  tests, 24 ignored), golden frames (`crates/mg-live/tests/fixtures/gateway-v1.json`)
  included; `cargo +1.98.0 clippy --workspace --all-targets -- -D warnings` and
  `cargo +1.98.0 fmt --check` are clean. (`mg-ui`'s `live_inventory_grid` writes a picture into
  `target/test-output/live-accept6-astra2/` without creating it, so it fails in a fresh `target`
  directory; unrelated to this work.)
* New tests: `lock_order_tests` (deadlock), `sessions_share_one_serialized_copy_of_an_area`
  (shared copy, identical frame text), `meta_is_pushed_for_state_changes_not_for_the_ageing_of_the_last_report`
  and `the_bridge_s_running_counters_alone_do_not_make_a_meta_push`,
  `rows_after_merges_the_areas_like_the_copying_reader`, `find_matches_like_to_lowercase_contains`,
  and `tools/live/gateway_load/test_gateway_load.py` for the harness arithmetic and framing.
* The scenario numbers above come from `target/test-output/gateway-load/*/summary.json`;
  they are not committed.
