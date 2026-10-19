"""A poor man's profiler for the gateway container: sample every thread's stack with gdb.

    python run.py ramp --ptrace --keep ...      # installs gdb in the gateway container
    python stacks.py 40                         # 40 samples of the running mgload-gw

Prints how often each function is the innermost frame of a running thread (threads parked in a
futex, epoll or poll are idle and skipped) and which `mg_*`/serde/tungstenite function each running
thread is inside. Each sample stops the process for a few hundred ms, so use few and spread them.
"""
import collections
import os
import re
import subprocess
import sys
import time

SCRIPT = ('pid=$(for p in /proc/[0-9]*; do [ "$(cat $p/comm)" = mg-gateway-dev ] && echo ${p#/proc/}; done); '
          'gdb -p $pid -batch -ex "thread apply all bt 14" 2>&1')
IDLE = re.compile(r"futex|Condvar|epoll_wait|park|__GI___poll|ppoll|nanosleep|syscall \(\)|pthread_cond|recv|read \(")
INTERESTING = re.compile(r"mg_live|mg_gateway|serde_json|tungstenite|axum|memcpy|malloc|realloc|free|tokio::io|hyper|alloc::")


def name(frame):
    frame = re.sub(r"^#\d+\s+(0x[0-9a-f]+ in )?", "", frame)
    frame = re.sub(r"\[[0-9a-f]{16}\]", "", frame)
    return re.sub(r"\s+\(.*", "", frame)[:110]


def sample():
    out = subprocess.run(["docker", "exec", "mgload-gw", "sh", "-c", SCRIPT], capture_output=True, text=True,
                         env={**os.environ, "MSYS_NO_PATHCONV": "1"}).stdout
    threads = []
    for block in re.split(r"\n(?=Thread \d+ )", out)[1:]:
        frames = [line for line in block.split("\n")[1:] if line.startswith("#")]
        threads.append(frames)
    return threads


def main(n, gap):
    top, inside, running, total = collections.Counter(), collections.Counter(), 0, 0
    for _ in range(n):
        for frames in sample():
            total += 1
            if not frames or IDLE.search(frames[0]):
                continue
            running += 1
            top[name(frames[0])] += 1
            seen = set()
            for f in frames:
                if INTERESTING.search(f) and name(f) not in seen and "tokio::runtime" not in f:
                    seen.add(name(f))
                    inside[name(f)] += 1
        time.sleep(gap)
    print(f"{n} samples, {total} thread stacks, {running} running ({100 * running / max(total, 1):.0f} %)")
    print("\ninnermost frame of running threads:")
    for f, c in top.most_common(18):
        print(f"{100 * c / max(running, 1):5.1f} %  {f}")
    print("\nrunning threads inside (any depth):")
    for f, c in inside.most_common(28):
        print(f"{100 * c / max(running, 1):5.1f} %  {f}")


if __name__ == "__main__":
    main(int(sys.argv[1]) if len(sys.argv) > 1 else 30, float(sys.argv[2]) if len(sys.argv) > 2 else 0.5)
