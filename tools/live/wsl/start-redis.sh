#!/usr/bin/env bash
source "$(dirname -- "$0")/owned.sh"
port=${1:-6379}
[[ "$port" =~ ^[0-9]+$ && "$port" -ge 1 && "$port" -le 65535 ]] || exit 2
command -v redis-server >/dev/null && command -v redis-cli >/dev/null || {
    echo 'Install Redis in WSL: sudo apt-get install -y redis-server' >&2; exit 2;
}
if [[ "$(redis-cli -h 127.0.0.1 -p "$port" ping 2>/dev/null || true)" == PONG ]]; then
    echo "Redis $port already available; its ownership is unchanged"
    if ! redis-cli -h 127.0.0.1 -p "$port" INFO persistence | tr -d '\r' | grep -qx 'aof_enabled:1'; then
        echo 'Warning: existing Redis has no AOF; queued Live events cannot survive its restart' >&2
    fi
    exit 0
fi
for suffix in pid start exe log; do safe_path "$scratch/redis-$port.$suffix"; done
queue_dir="$scratch/redis-$port-data"
safe_path "$queue_dir"
mkdir -p -- "$queue_dir"
redis-server --bind 127.0.0.1 --port "$port" --protected-mode yes --save '' \
    --appendonly yes --appendfsync always --dir "$queue_dir" \
    --daemonize yes --pidfile "$scratch/redis-$port.pid" --logfile "$scratch/redis-$port.log"
for ((attempt=0; attempt<300; attempt++)); do
    if [[ -f "$scratch/redis-$port.pid" ]]; then
        pid=$(cat "$scratch/redis-$port.pid")
        if [[ "$pid" =~ ^[0-9]+$ && -r "/proc/$pid/stat" &&
              "$(redis-cli -h 127.0.0.1 -p "$port" ping 2>/dev/null || true)" == PONG ]]; then
            record_process "redis-$port" "$pid"
            echo "Started owned Redis $port"
            exit 0
        fi
    fi
    sleep 0.1
done
echo 'Owned Redis did not become ready; inspect its scratch log' >&2
exit 1
