# NWNX_Redis reconnect

Stock NWNX_Redis never reconnects: after Redis restarts, `NWNX_Redis_PUBLISH`
uses dead pooled clients and pub/sub stays unsubscribed until the game server
restarts. `reconnect.patch` (against nwnxee/unified 3f504d0f14) drops pooled
clients that lost their connection and adds a watchdog that re-subscribes
pub/sub with back-off (2 s up to 30 s).

    build.sh ~/unified ~/nwnx-redis-reconnect     # copy, patch, build; the checkout is untouched
    NWNX_ROOT=~/nwnx-redis-reconnect ./outage-test.sh

`outage-test.sh` delegates to `tools/live/test-events.py --phase redis_restart`:
owned `~/mglive-events`, UDP5126, Redis6392, identity-checked child cleanup and
single-job guarded fixture compilation. It never copies/stops/builds the demo,
uses the shared game user directory, or kills processes by name. Three 10-second
outages pass with unchanged game PID/boot, state recovery in 0.95-1.02 seconds,
command subscription recovery in 2.63-3.45 seconds, verified command effects and
buffered native chat (2026-10-15). The plugin needed no further patch.

Exact build/source hashes and proof limitations are in
[the canonical Redis document](../../docs/live-mode-redis.md#patched-redis-restart-proof-audit-h2-2026-10-15).
Upstream PR only with the owner's consent; none opened.
