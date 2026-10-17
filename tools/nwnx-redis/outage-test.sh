#!/usr/bin/env bash
# Uses the existing marker/PID-identity guarded private harness only.
# UDP5126, Redis6392, ~/mglive-events; no demo/system Redis or app build.
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
exec python3 "$repo/tools/live/test-events.py" --phase redis_restart
