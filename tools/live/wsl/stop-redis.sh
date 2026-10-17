#!/usr/bin/env bash
source "$(dirname -- "$0")/owned.sh"
port=${1:-6380}
[[ "$port" =~ ^[0-9]+$ && "$port" -ge 1 && "$port" -le 65535 ]] || exit 2
# System Redis is never an outage target, even if an old marker exists.
[[ "$port" != 6379 ]] || { echo 'Refusing to stop the system Redis port' >&2; exit 1; }
stop_owned "redis-$port"
