#!/usr/bin/env bash
# Shared scratch/process ownership checks. Never infer ownership from a port.
set -euo pipefail
case "${MGLIVE_INSTANCE:-demo}" in
    demo) scratch="$HOME/mglive-home"; marker=.moonglow-live-demo; owner=moonglow-live-demo ;;
    events) scratch="$HOME/mglive-events"; marker=.moonglow-live-events; owner=moonglow-live-events-v1 ;;
    *) echo 'Unknown live instance' >&2; exit 2 ;;
esac
[[ ! -L "$scratch" ]] || { echo 'Scratch directory is a symlink' >&2; exit 1; }
if [[ -e "$scratch" ]]; then
    [[ -f "$scratch/$marker" && ! -L "$scratch/$marker" &&
       "$(cat "$scratch/$marker")" == "$owner" ]] || {
        echo 'Existing scratch directory is not owned by this live instance' >&2; exit 1;
    }
else
    mkdir -- "$scratch"
    printf '%s\n' "$owner" > "$scratch/$marker"
fi
[[ "$(realpath -- "$scratch")" == "$scratch" ]] || exit 1

safe_path() {
    [[ "$1" == "$scratch/"* && ! -L "$1" && "$(realpath -m -- "$1")" == "$1" ]] || {
        echo 'Unsafe scratch path' >&2; exit 1;
    }
}

record_process() {
    local name=$1 pid=$2 suffix
    for suffix in pid start exe; do safe_path "$scratch/$name.$suffix"; done
    printf '%s\n' "$pid" > "$scratch/$name.pid"
    awk '{print $22}' "/proc/$pid/stat" > "$scratch/$name.start"
    readlink -f "/proc/$pid/exe" > "$scratch/$name.exe"
}

owned_pid() {
    local name=$1 pid suffix
    for suffix in pid start exe; do
        safe_path "$scratch/$name.$suffix"
        [[ -f "$scratch/$name.$suffix" ]] || return 1
    done
    pid=$(cat "$scratch/$name.pid")
    [[ "$pid" =~ ^[0-9]+$ && -r "/proc/$pid/stat" ]] || return 1
    [[ "$(awk '{print $22}' "/proc/$pid/stat")" == "$(cat "$scratch/$name.start")" &&
       "$(readlink -f "/proc/$pid/exe")" == "$(cat "$scratch/$name.exe")" ]] || {
        echo 'Recorded PID identity changed; refusing to signal it' >&2; exit 1;
    }
    printf '%s\n' "$pid"
}

stop_owned() {
    local name=$1 pid attempt stat
    pid=$(owned_pid "$name") || return 0
    kill -TERM "$pid"
    for ((attempt=0; attempt<50; attempt++)); do
        stat=$(cat "/proc/$pid/stat" 2>/dev/null) || return 0
        [[ "$(awk '{print $3}' <<< "$stat")" == Z ]] && return 0
        [[ "$(awk '{print $22}' <<< "$stat")" == "$(cat "$scratch/$name.start")" ]] || return 0
        sleep 0.1
    done
    echo 'Owned process has not exited after TERM; left for inspection' >&2
    return 1
}
