#!/usr/bin/env bash
# MGLIVE_INSTANCE=events MGLIVE_REDIS_PORT=6392 MGLIVE_PREFIX=mglive-events
# MGLIVE_BOOT=<server boot> bash log-tail.sh
# Existing lines are skipped on first attach. Run separately from the server.
source "$(dirname -- "$0")/owned.sh"
prefix=${MGLIVE_PREFIX:-mglive}
port=${MGLIVE_REDIS_PORT:-6379}
[[ "$prefix" =~ ^[A-Za-z0-9_:.-]{1,64}$ && "$port" =~ ^[0-9]+$ &&
   "$port" -ge 1 && "$port" -le 65535 ]] || { echo 'Invalid Redis configuration' >&2; exit 2; }
safe_path "$scratch/logs.0"
safe_path "$scratch/logs.0/nwserverLog1.txt"
safe_path "$scratch/log-tail.lock"
# One follower per owned scratch. No process inferred from a port or name.
exec 9>"$scratch/log-tail.lock"
flock -n 9 || { echo 'A log follower already owns this scratch' >&2; exit 1; }
exec python3 "$(dirname -- "$0")/log-tail.py" "$scratch" "$prefix" "$port" "${MGLIVE_BOOT:-}"
