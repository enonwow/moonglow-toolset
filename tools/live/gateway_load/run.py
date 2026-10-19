"""Orchestrates a gateway load run with Docker (stdlib only). See docs/live-gateway-load.md.

    python tools/live/gateway_load/run.py build --tag baseline
    python tools/live/gateway_load/run.py evening --tag baseline --label baseline-evening
    python tools/live/gateway_load/run.py ramp    --tag baseline --label baseline-ramp
    python tools/live/gateway_load/run.py down

Containers (all named mgload-*): redis (AOF on, fixed IP), net (an idle container that owns the
network namespace so the loopback-only dev host can be reached and restarted), gw (the system
under test, --cpus=6), game (the synthetic game) and dmK/agents (the load clients).
"""
import argparse
import json
import os
import secrets
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

from report import flat, inside, rates, slope, steady, summary

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "target" / "test-output" / "gateway-load"
NET = "mgload"
REDIS_IP = ""  # Redis' address on the private network, read after it starts (the gateway resolves it once)
C_REDIS, C_NS, C_GW, C_GAME = "mgload-redis", "mgload-net", "mgload-gw", "mgload-game"
IMG_GW, IMG_TOOLS, IMG_REDIS = "moonglow-load-gateway", "moonglow-load-tools", "redis:7.4-alpine"
PORT = 8080
GROUPS = ["read", "read_chat", "read_tells", "variables", "npc", "speech", "spawn", "spawn_scripted", "players",
          "focus", "approve_drafts", "allow_llm", "manage_ai"]
AI_GROUPS = ["read", "read_chat", "npc", "speech"]
SAMPLE_GW = r"""cat /sys/fs/cgroup/cpu.stat
for p in /proc/[0-9]*; do
  if [ "$(cat $p/comm 2>/dev/null)" = mg-gateway-dev ]; then
    grep -E 'VmRSS|VmHWM|Threads' $p/status
    for t in $p/task/*; do echo "T $(cat $t/comm) $(cut -d' ' -f14,15 $t/stat)"; done
  fi
done"""
SAMPLE_PLAIN = "cat /sys/fs/cgroup/cpu.stat; grep -E 'VmRSS' /proc/1/status || true"


def run_retry(cmd, **kw):
    """subprocess.run that survives a host short of memory (Windows: WinError 1455) for a while."""
    for attempt in range(6):
        try:
            return subprocess.run(cmd, **kw)
        except OSError:
            if attempt == 5:
                raise
            time.sleep(10)


def docker(*args, check=True, timeout=600):
    r = run_retry(["docker", *map(str, args)], capture_output=True, text=True, timeout=timeout,
                  env={**os.environ, "MSYS_NO_PATHCONV": "1"})
    if check and r.returncode:
        raise RuntimeError("docker %s failed: %s" % (" ".join(map(str, args[:3])), r.stderr.strip()[:500]))
    return r.stdout


def exec_out(container, script, timeout=20):
    return docker("exec", container, "sh", "-c", script, check=False, timeout=timeout)


def parse_stat(text):
    d = {}
    threads = {}
    for line in text.splitlines():
        parts = line.split()
        if not parts:
            continue
        if parts[0] in ("usage_usec", "throttled_usec", "nr_throttled", "nr_periods"):
            d[parts[0]] = int(parts[1])
        elif parts[0] in ("VmRSS:", "VmHWM:", "Threads:"):
            d[parts[0][:-1].lower()] = int(parts[1])
        elif parts[0] == "T" and len(parts) >= 4:
            threads[parts[1]] = threads.get(parts[1], 0) + int(parts[2]) + int(parts[3])
    if threads:
        d["thread_ticks"] = threads
    return d


# ---- build and lifecycle ----------------------------------------------------------------------
def build(a):
    """Gateway image from the working tree, or from the tree of a git revision (--rev) to compare with."""
    env = {**os.environ, "DOCKER_BUILDKIT": "1", "MSYS_NO_PATHCONV": "1"}
    dockerfile = ROOT / "tools/live/gateway_load/Dockerfile"
    for target, image in (("gateway", IMG_GW), ("tools", IMG_TOOLS)):
        if target == "tools" and a.rev:
            continue
        tag = a.tag if target == "gateway" else "latest"
        print("building", image + ":" + tag, flush=True)
        cmd = ["build", "--target", target, "-t", f"{image}:{tag}"]
        if a.rustflags and target == "gateway":
            cmd += ["--build-arg", "RUSTFLAGS=" + a.rustflags]
        if not a.rev:
            subprocess.run(["docker", *cmd, "-f", str(dockerfile), "."], cwd=ROOT, check=True, env=env)
            continue
        with tempfile.TemporaryDirectory() as tree:
            archive = Path(tree) / "src.tar"
            subprocess.run(["git", "archive", "-o", str(archive), a.rev], cwd=ROOT, check=True)
            source = Path(tree) / "src"
            import tarfile

            with tarfile.open(archive) as tar:
                tar.extractall(source)
            shutil.copy(dockerfile, source / "Dockerfile")
            subprocess.run(["docker", *cmd, "-f", "Dockerfile", "."], cwd=source, check=True, env=env)


def down(_a=None):
    names = docker("ps", "-a", "--filter", "name=^mgload-", "--format", "{{.Names}}").split()
    if names:
        docker("rm", "-f", *names, check=False)
    docker("network", "rm", NET, check=False)


def credentials(dms, agents):
    cred = {"dm": [], "agent": []}
    tokens = []
    for i in range(dms):
        cred["dm"].append({"id": f"dm:load{i:03d}", "token": secrets.token_hex(24), "owns": []})
    for j in range(agents):
        owner = cred["dm"][j % dms]
        agent = {"id": f"ai:agent{j:02d}", "token": secrets.token_hex(24), "owner": owner["id"]}
        owner["owns"].append(agent["id"])
        cred["agent"].append(agent)
    for c in cred["dm"]:
        tokens.append({"token": c["token"], "principal": {"id": c["id"], "name": c["id"], "groups": GROUPS}})
    for c in cred["agent"]:
        tokens.append({"token": c["token"], "owner": c["owner"], "llm_allowed": True,
                       "principal": {"id": c["id"], "name": c["id"], "groups": AI_GROUPS}})
    config = {"port": PORT, "token": "", "groups": GROUPS, "tokens": tokens, "data_dir": "/data/gw", "llm_enabled": True}
    return cred, config


def put_file(container, text, dest):
    with tempfile.TemporaryDirectory() as d:
        p = Path(d) / Path(dest).name
        p.write_text(text)
        docker("cp", str(p), f"{container}:{dest}")


def wait_health(timeout=60):
    end = time.time() + timeout
    code = "import urllib.request,sys;urllib.request.urlopen('http://127.0.0.1:%d/health',timeout=2).read()" % PORT
    while time.time() < end:
        if subprocess.run(["docker", "exec", C_NS, "python", "-c", code], capture_output=True).returncode == 0:
            return True
        time.sleep(0.5)
    return False


def start_gateway(a, config):
    docker("rm", "-f", C_GW, check=False)
    docker("create", "--name", C_GW, "--network", f"container:{C_NS}", "--init", "--cpus", a.cpus, "--memory", a.memory,
           "--memory-swap", a.memory, *(["--cap-add", "SYS_PTRACE"] if a.ptrace else []),
           *(["--cap-add", "PERFMON", "--cap-add", "SYS_ADMIN", "--security-opt", "seccomp=unconfined"] if a.perf else []),
           *[x for kv in a.env for x in ("-e", kv)], "-e", f"MOONGLOW_LIVE_REDIS={REDIS_IP}:6379", "-e", "MOONGLOW_LIVE_PREFIX=mglive",
           f"{IMG_GW}:{a.tag}", "--config", "/data/config.json")
    put_file(C_GW, json.dumps(config), "/data/config.json")
    docker("start", C_GW)


def up(a, config):
    down()
    global REDIS_IP
    docker("network", "create", NET)
    docker("run", "-d", "--name", C_REDIS, "--network", NET, "--cpus", "2", "--memory", "2g", IMG_REDIS,
           "redis-server", "--appendonly", "yes", "--appendfsync", "everysec", "--save", "", "--protected-mode", "no")
    REDIS_IP = docker("inspect", "-f", "{{(index .NetworkSettings.Networks \"" + NET + "\").IPAddress}}", C_REDIS).strip()
    docker("run", "-d", "--name", C_NS, "--network", NET, "--init", "--cpus", "0.2", "--memory", "64m", f"{IMG_TOOLS}:latest",
           "python", "-c", "import time;time.sleep(10**9)")
    start_gateway(a, config)
    if a.ptrace or a.perf:
        exec_out(C_GW, "apt-get update -qq >/dev/null 2>&1 && apt-get install -y -qq gdb linux-perf >/dev/null 2>&1", timeout=300)
    if not wait_health():
        raise RuntimeError("gateway did not become healthy:\n" + docker("logs", C_GW, check=False))


def start_game(a):
    docker("rm", "-f", C_GAME, check=False)
    docker("run", "-d", "--name", C_GAME, "--network", NET, "--init", "--cpus", "2", "--memory", "1g", f"{IMG_TOOLS}:latest",
           "python", "game.py", "--host", REDIS_IP, "--out", "/tmp/game.json", "--areas", str(a.areas),
           "--dense", str(a.dense), "--chat-rate", str(a.chat_rate), "--warm-seconds", str(a.warm))


# ---- sampling ---------------------------------------------------------------------------------
def dump_stacks(dest):
    """gdb backtraces of every gateway thread (needs --ptrace); used when the gateway stalls."""
    script = ('pid=$(for p in /proc/[0-9]*; do [ "$(cat $p/comm)" = mg-gateway-dev ] && echo ${p#/proc/}; done); '
              'gdb -p $pid -batch -ex "thread apply all bt 40" 2>&1')
    (dest / "stall-stacks.txt").write_text(exec_out(C_GW, script, timeout=120))
    print("gateway stalled: stacks written to", dest / "stall-stacks.txt", flush=True)


class Sampler(threading.Thread):
    def __init__(self, t0, interval, clients, dest=None, watchdog=False):
        super().__init__(daemon=True)
        self.t0, self.interval, self.clients = t0, interval, clients
        self.dest, self.watchdog, self.quiet = dest, watchdog, 0
        self.rows = {"gw": [], "redis": [], "clients": {}}
        self.stop = threading.Event()

    def run(self):
        n = 0
        while not self.stop.is_set():
            t = time.time() - self.t0
            gw = parse_stat(exec_out(C_GW, SAMPLE_GW))
            if gw:
                gw["t"] = t
                prev = self.rows["gw"][-1] if self.rows["gw"] else None
                self.rows["gw"].append(gw)
                if self.watchdog and prev and 20 < t < 400:
                    busy = (gw["usage_usec"] - prev["usage_usec"]) / 1e6 / max(0.1, t - prev["t"])
                    self.quiet = self.quiet + 1 if busy < 0.03 else 0
                    if self.quiet == 3:
                        dump_stacks(self.dest)
            rd = parse_stat(exec_out(C_REDIS, SAMPLE_PLAIN))
            if rd:
                rd["t"] = t
                self.rows["redis"].append(rd)
            if n % 3 == 0:
                for name in self.clients + [C_GAME]:
                    s = parse_stat(exec_out(name, SAMPLE_PLAIN))
                    if s:
                        s["t"] = t
                        self.rows["clients"].setdefault(name, []).append(s)
            n += 1
            self.stop.wait(max(0.0, self.interval - (time.time() - self.t0 - t)))


# ---- clients ----------------------------------------------------------------------------------
def start_client(name, role, first, count, a, creds_text, t0, delay, duration, cpus="1.5", extra=()):
    docker("rm", "-f", name, check=False)
    docker("create", "--name", name, "--network", f"container:{C_NS}", "--init", "--cpus", cpus, "--memory", "1g",
           f"{IMG_TOOLS}:latest", "python", "clients.py", "--role", role, "--creds", "/tmp/creds.json", "--first", first,
           "--count", count, "--port", PORT, "--t0", f"{t0:.3f}", "--delay", delay, "--duration", duration,
           "--areas", a.areas, "--dense", a.dense, "--out", "/tmp/out.json", "--seed", first * 7 + 1, *extra)
    put_file(name, creds_text, "/tmp/creds.json")
    docker("start", name)


def collect(names, dest):
    out = {}
    for n in names:
        p = dest / (n + ".json")
        r = run_retry(["docker", "cp", f"{n}:/tmp/out.json", str(p)], capture_output=True, text=True)
        if r.returncode == 0:
            out[n] = json.loads(p.read_text())
        else:
            print("no output from", n, r.stderr.strip()[:200])
    return out


def wait_exit(names, deadline):
    while time.time() < deadline:
        if all(docker("inspect", "-f", "{{.State.Running}}", n, check=False).strip() == "false" for n in names):
            return True
        time.sleep(2)
    return False


# ---- scenarios --------------------------------------------------------------------------------
def run_scenario(a, plan):
    """plan: dict(dm_stages=[(count, start_s)], duration, events=[(at_s, kind)])"""
    label = a.label or f"{a.scenario}-{a.tag}"
    dest = OUT / label
    shutil.rmtree(dest, ignore_errors=True)
    dest.mkdir(parents=True)
    total_dm = sum(s[0] for s in plan["dm_stages"])
    cred, config = credentials(total_dm, a.agents)
    print(f"[{label}] starting containers", flush=True)
    up(a, config)
    start_game(a)
    time.sleep(max(a.settle, a.warm + 3))
    creds_text = json.dumps(cred)
    t0 = time.time() + 8
    duration = plan["duration"]
    names, first = [], 0
    per = a.per_proc
    for count, start, *rest in plan["dm_stages"]:
        done = 0
        rate = rest[0] if rest else 0.2
        while done < count:
            n = min(per, count - done)
            name = f"mgload-dm{len(names)}"
            # --duration is relative to t0 for every client, whenever it starts.
            start_client(name, "dm", first, n, a, creds_text, t0, start, duration,
                         extra=("--stagger", "0.15", "--dm-cmd-rate", rate))
            names.append(name)
            first += n
            done += n
    if a.agents:
        start_client("mgload-agents", "agent", 0, a.agents, a, creds_text, t0, 3, duration, cpus="1.0", extra=("--stagger", "0.3"))
        names.append("mgload-agents")
    sampler = Sampler(t0, a.sample, names, dest, a.ptrace)
    sampler.start()
    events = []

    def disruptor():
        for at, kind in plan["events"]:
            time.sleep(max(0, t0 + at - time.time()))
            begin = time.time() - t0
            if kind == "redis":
                docker("restart", "-t", "5", C_REDIS)
            elif kind == "gateway":
                docker("restart", "-t", "5", C_GW)
                wait_health()
            events.append({"kind": kind, "start": round(begin, 1), "end": round(time.time() - t0, 1)})
            print(f"[{label}] {kind} restart at {begin:.0f}s took {events[-1]['end'] - begin:.1f}s", flush=True)

    threading.Thread(target=disruptor, daemon=True).start()
    print(f"[{label}] running {duration}s", flush=True)
    wait_exit(names, t0 + duration + a.drain + 60)
    sampler.stop.set()
    sampler.join(timeout=30)
    (dest / "samples.json").write_text(json.dumps(sampler.rows))
    time.sleep(1)
    results = collect(names, dest)
    game = collect_game(dest)
    (dest / "gateway.log").write_text(docker("logs", C_GW, check=False))
    raw = {"label": label, "scenario": a.scenario, "plan": plan, "events": events, "samples": sampler.rows,
           "cpus": a.cpus, "memory": a.memory, "tag": a.tag, "dm_total": total_dm, "agents": a.agents, "game": game}
    (dest / "raw.json").write_text(json.dumps({**raw, "clients": {k: {**v, "samples": None} for k, v in results.items()}}))
    summarize(raw, results, dest)
    if not a.keep:
        down()


def collect_game(dest):
    docker("kill", "-s", "TERM", C_GAME, check=False)
    time.sleep(3)
    p = dest / "game.json"
    r = run_retry(["docker", "cp", f"{C_GAME}:/tmp/game.json", str(p)], capture_output=True, text=True)
    return json.loads(p.read_text()) if r.returncode == 0 else {}


def sample_names(results, name, role=None):
    out = []
    for c in results.values():
        if role and c["role"] != role:
            continue
        out.extend(c["samples"].get(name, []))
    return out


def summarize(raw, results, dest):
    plan, events = raw["plan"], raw["events"]
    duration = plan["duration"]
    warm = plan.get("warmup", 60)
    outage = [(e["start"] - 5, e["end"] + 45) for e in events]
    stages = plan.get("stages") or [{"name": "run", "start": warm, "end": duration}]
    out = {"label": raw["label"], "tag": raw["tag"], "cpus": raw["cpus"], "memory": raw["memory"], "dm_total": raw["dm_total"],
           "agents": raw["agents"], "events": events, "stages": []}
    gw, redis = raw["samples"]["gw"], raw["samples"]["redis"]
    for st in stages:
        s, e = st["start"], st["end"]
        # Outage windows only cut the run that contains them; stages skip them as well.
        row = {"name": st["name"], "window": [s, e], "dm_sessions": st.get("dm", raw["dm_total"])}
        ap = steady(sample_names(results, "area_push", "dm"), s, e, outage)
        row["area_push_ms"] = summary([r[1] for r in ap])
        row["area_push_large_ms"] = summary([r[1] for r in ap if len(r) > 2 and r[2] >= 300])
        row["area_push_normal_ms"] = summary([r[1] for r in ap if len(r) > 2 and r[2] < 300])
        row["chat_event_ms"] = summary([r[1] for r in steady(sample_names(results, "chat_event", "dm"), s, e, outage)])
        rtt = steady(sample_names(results, "command_rtt", "dm"), s, e, outage)
        dwell = {c: ms for c, ms in raw["game"].get("dwell_ms", [])}
        row["command_rtt_ms"] = summary([r[1] for r in rtt])
        row["command_rtt_minus_game_wait_ms"] = summary([r[1] - dwell[r[2]] for r in rtt if r[2] in dwell])
        row["agent_wake_ms"] = summary([r[1] for r in steady(sample_names(results, "agent_wake", "agent"), s, e, outage)])
        calls = steady(sample_names(results, "agent_call", "agent"), s, e, outage)
        row["agent_call_ms"] = summary([r[1] for r in calls])
        for kind in ("find", "status", "area_state", "npc_say"):
            row["agent_call_" + kind + "_ms"] = summary([r[1] for r in calls if r[2] == kind])
        cpu = rates(gw, "usage_usec", outage, s, e)
        row["gateway_cpu_cores_mean"] = round(sum(r for _, r in cpu) / len(cpu) / 1e6, 2) if cpu else None
        row["gateway_cpu_cores_max_5s"] = round(max((r for _, r in cpu), default=0) / 1e6, 2)
        thr = rates(gw, "throttled_usec", outage, s, e)
        row["gateway_throttled_s_per_s"] = round(sum(r for _, r in thr) / len(thr) / 1e6, 3) if thr else None
        rc = rates(redis, "usage_usec", outage, s, e)
        row["redis_cpu_cores_mean"] = round(sum(r for _, r in rc) / len(rc) / 1e6, 2) if rc else None
        rss = [r["vmrss"] / 1024 for r in gw if s <= r["t"] < e and "vmrss" in r and not inside(r["t"], outage)]
        row["gateway_rss_mb"] = {"min": round(min(rss), 1), "max": round(max(rss), 1), "last": round(rss[-1], 1),
                                 "slope_mb_per_min": round(slope([(r["t"] / 60, r["vmrss"] / 1024) for r in gw if s <= r["t"] < e and "vmrss" in r]), 2),
                                 "flat_10pct": flat(rss)} if rss else None
        out["stages"].append(row)
    counts = {}
    for c in results.values():
        for k, v in c["counts"].items():
            counts[k] = counts.get(k, 0) + v
    out["client_counts"] = counts
    out["client_lag_max_ms"] = max((c["lag_max_ms"] for c in results.values()), default=0)
    out["client_errors"] = [e for c in results.values() for e in c["errors"]][:40]
    sent = counts.get("cmd_sent", 0)
    terminal = sum(counts.get(k, 0) for k in ("cmd_final", "cmd_unknown", "cmd_rejected", "cmd_unknown_local"))
    out["commands"] = {"sent": sent, "terminal": terminal, "final": counts.get("cmd_final", 0), "unknown": counts.get("cmd_unknown", 0),
                       "unknown_local": counts.get("cmd_unknown_local", 0), "rejected": counts.get("cmd_rejected", 0),
                       "lost": sent - terminal, "duplicate_results": counts.get("result_duplicate", 0) + counts.get("result_after_ack", 0),
                       "duplicate_events": counts.get("events_duplicate", 0) + counts.get("agent_event_duplicate", 0),
                       "game_received": raw["game"].get("stats", {}).get("commands"), "game_redelivered": raw["game"].get("stats", {}).get("redelivered_commands")}
    out["game"] = raw["game"].get("stats")
    # Recovery: the longest stretch without any area push (any session) around each outage.
    times = sorted(r[0] for r in sample_names(results, "area_push", "dm"))
    for e in events:
        near = [t for t in times if e["start"] - 5 <= t <= e["end"] + 90]
        e["longest_push_gap_s"] = round(max((b - a for a, b in zip(near, near[1:])), default=0), 1)
    out["gateway_log_errors"] = sum(1 for line in (dest / "gateway.log").read_text().splitlines() if "panic" in line.lower())
    out["generator_cpu_cores"] = {}
    for name, rows in raw["samples"]["clients"].items():
        r = rates(rows, "usage_usec", [], warm, duration)
        if r:
            out["generator_cpu_cores"][name] = round(sum(x for _, x in r) / len(r) / 1e6, 2)
    (dest / "summary.json").write_text(json.dumps(out, indent=1))
    print(json.dumps(out, indent=1))


def evening(a):
    a.scenario = "evening"
    d = a.minutes * 60
    events = [(60 + d * 0.3, "redis"), (60 + d * 0.6, "gateway")]
    if a.outages:
        events = [(float(x.split("@")[1]), x.split("@")[0]) for x in a.outages.split(",")]
    plan = {"dm_stages": [(a.dm, 0)], "duration": d + 60, "warmup": 60, "events": [] if a.no_outages else events}
    run_scenario(a, plan)


def ramp(a):
    a.scenario = "ramp"
    stage = a.stage_seconds
    sizes = [int(x) for x in a.sizes.split(",")]
    stages, dm_stages, total, t = [], [], 0, 0
    for i, n in enumerate(sizes):
        # The game answers a few commands a second at most (the gateway refuses more than 600 a
        # minute in all): only the first group of desktops speaks, the later ones watch.
        dm_stages.append((n - total, t, 0.2 if i == 0 else 0.0))
        total = n
        stages.append({"name": f"{n} DMs", "dm": n, "start": t + 45, "end": t + stage})
        t += stage
    plan = {"dm_stages": dm_stages, "duration": t, "warmup": 45, "stages": stages, "events": []}
    run_scenario(a, plan)


def main():
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("--tag", default="latest")
    b.add_argument("--rustflags", default="")
    b.add_argument("--rev", default="", help="build the gateway from this git revision instead of the working tree")
    sub.add_parser("down")
    for name in ("evening", "ramp"):
        s = sub.add_parser(name)
        s.add_argument("--tag", default="latest")
        s.add_argument("--label", default="")
        s.add_argument("--cpus", default="6")
        s.add_argument("--memory", default="4g")
        s.add_argument("--dm", type=int, default=30)
        s.add_argument("--agents", type=int, default=10)
        s.add_argument("--areas", type=int, default=50)
        s.add_argument("--dense", type=int, default=5)
        s.add_argument("--chat-rate", type=float, default=10.0)
        s.add_argument("--minutes", type=float, default=10)
        s.add_argument("--no-outages", action="store_true")
        s.add_argument("--outages", default="", help="redis@SECONDS,gateway@SECONDS from the start of the measured part (default 30%% and 60%%)")
        s.add_argument("--sizes", default="30,60,120")
        s.add_argument("--stage-seconds", type=int, default=240)
        s.add_argument("--per-proc", type=int, default=15)
        s.add_argument("--settle", type=int, default=6)
        s.add_argument("--warm", type=int, default=0, help="seconds of heavy chat before the clients start, to fill the event log (and settle that long)")
        s.add_argument("--sample", type=float, default=5.0)
        s.add_argument("--drain", type=int, default=20)
        s.add_argument("--keep", action="store_true")
        s.add_argument("--env", action="append", default=[], metavar="KEY=VALUE", help="extra environment for the gateway container")
        s.add_argument("--ptrace", action="store_true", help="install gdb in the gateway container (stack dump when it stalls)")
        s.add_argument("--perf", action="store_true", help="let perf run inside the gateway container (see stacks.py for a gdb sampler)")
    a = p.parse_args()
    {"build": build, "down": down, "evening": evening, "ramp": ramp}[a.cmd](a)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        down()
        sys.exit(130)
