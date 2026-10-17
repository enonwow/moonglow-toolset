#!/usr/bin/env bash
source "$(dirname -- "$0")/owned.sh"
server_pid=$(owned_pid server) || exit 0
# In addition to PID/start/executable, require our scratch user directory.
grep -zFxq -- "$scratch" "/proc/$server_pid/cmdline" || { echo 'Not the demo user directory' >&2; exit 1; }
stop_owned server
