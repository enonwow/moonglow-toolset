#!/usr/bin/env bash
# Dedicated private engine test. Never invokes demo launchers or touches their scratch.
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/../.." && pwd)
exec python3 "$repo/tools/live/test-events.py" "$@"
