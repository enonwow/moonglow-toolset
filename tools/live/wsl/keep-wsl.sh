#!/usr/bin/env bash
# Remains attached to wsl.exe until Stop-LiveDemo; survives GUI/server restarts.
source "$(dirname -- "$0")/owned.sh"
if [[ "${1:-}" == --status ]]; then owned_pid keeper || true; exit 0; fi
safe_path "$scratch/keeper.lock"
exec 9> "$scratch/keeper.lock"
flock -n 9 || exit 0
record_process keeper "$$"
child=''
cleanup() { if [[ -n "$child" ]]; then kill "$child" 2>/dev/null || true; wait "$child" 2>/dev/null || true; fi; }
trap 'exit 0' TERM INT
trap cleanup EXIT
while true; do sleep 3600 & child=$!; wait "$child" || true; done
