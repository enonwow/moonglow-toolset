"""Load clients: DM desktops on /ws/v1 and AI agents on /agent/v1 (asyncio, stdlib only).

One process runs a slice of the principals (`--role dm|agent --first N --count M`) from the
credentials file written by run.py; the orchestrator starts several processes. Latencies come
from clocks stamped by game.py inside the raw frames: area pushes carry the probe creature name
`P<ns>`, chat events carry `c<ns>`; command round trips are measured client side and the
game's dwell time is subtracted later by the orchestrator.

The output (`--out`) holds timestamped samples so the orchestrator can cut windows (ramp stages,
outage windows) and the counters that prove no result was lost or repeated.
"""
import argparse
import asyncio
import json
import random
import re
import time
import uuid
from collections import defaultdict
from pathlib import Path

from wsclient import Closed, Http, WebSocket

AREA_HEAD = re.compile(rb'"area":"([^"]+)","rev":(\d+),"age_ms":(\d+)')
PROBE = b'"name":"P1'
CHAT = re.compile(rb'"text":"c(1\d{18}) ')


class Recorder:
    def __init__(self, t0):
        self.t0 = t0
        self.samples = defaultdict(list)
        self.counts = defaultdict(int)
        self.errors = []
        self.dups = []
        self.lag_max = 0.0

    def sample(self, name, ms, tag=None):
        row = [round(time.time() - self.t0, 2), round(ms, 2)]
        if tag is not None:
            row.append(tag)
        self.samples[name].append(row)

    def count(self, name, n=1):
        self.counts[name] += n

    def error(self, who, text):
        self.counts["errors"] += 1
        if len(self.errors) < 200:
            self.errors.append([round(time.time() - self.t0, 1), who, str(text)[:200]])


def watch_set(rng, areas, dense):
    k = rng.choice([3, 4, 5])
    picked = []
    while len(picked) < k:
        pool = areas[:dense] if rng.random() < 0.4 else areas[dense:]
        a = rng.choice(pool)
        if a not in picked:
            picked.append(a)
    return picked


class Dm:
    def __init__(self, cred, a, rec, stop_at, seed):
        self.cred, self.a, self.rec, self.stop_at = cred, a, rec, stop_at
        self.rng = random.Random(seed)
        self.areas = ["a%02d" % i for i in range(a.areas)]
        self.watch = watch_set(self.rng, self.areas, a.dense)
        self.resume = None
        self.event_n = 0
        self.rev = {}
        self.boot = ""
        self.id_prefix = ""
        self.proc = uuid.uuid4().hex[:8]
        self.counter = 0
        self.req_id = 0
        self.inflight = {}  # command id -> sent time
        self.final = {}  # command id -> terminal state seen
        self.acked = {}  # command id -> rev acknowledged
        self.unacked = {}  # id -> rev not yet acknowledged
        self.last_probe = {}
        self.seen_events = set()
        self.ws = None
        self.owned_ai = cred.get("owns", [])
        self.drafted = set()
        self.gw_epoch = None

    async def run(self):
        backoff = 1
        while time.time() < self.stop_at + self.a.drain:
            try:
                await self.session()
                backoff = 1
            except (Closed, OSError, asyncio.TimeoutError) as e:
                self.rec.count("dm_disconnects")
                self.rec.error(self.cred["id"], f"session ended: {e}")
            await asyncio.sleep(min(backoff, 10))
            backoff = min(backoff * 2, 10)
        self.finish()

    def finish(self):
        # Commands with no terminal state at the end are "lost" unless the link broke with a new gateway epoch.
        for cid in self.inflight:
            self.rec.count("cmd_no_terminal_state")

    async def send_req(self, op, args):
        self.req_id += 1
        await self.ws.send(json.dumps({"t": "req", "id": self.req_id, "op": op, "a": args}, separators=(",", ":")))
        return self.req_id

    async def session(self):
        a = self.a
        self.ws = ws = await WebSocket.connect(a.host, a.port, "/ws/v1?proto=1&mirror=2", self.cred["token"])
        hello = {"t": "hello", "proto": 1, "mirror": 2, "client": "gateway-load/1", "watch": self.watch}
        if self.resume:
            hello["resume"] = {"gw_epoch": self.resume, "boot": self.boot, "event_n": self.event_n, "areas": self.rev}
        await ws.send(json.dumps(hello, separators=(",", ":")))
        self.req_id = 0
        pending_res = {}
        self.pending_res = pending_res
        tasks = [asyncio.create_task(self.commands()), asyncio.create_task(self.churn()), asyncio.create_task(self.acker())]
        reader = asyncio.create_task(self.read_loop(ws))
        try:
            done, _ = await asyncio.wait({reader}, timeout=max(0.1, self.stop_at + self.a.drain - time.time()))
            if reader in done:
                reader.result()
        finally:
            for t in tasks + [reader]:
                t.cancel()
            ws.close()
            self.ws = None

    async def read_loop(self, ws):
        while True:
            self.handle(await ws.recv())

    def handle(self, data):
        now = time.time_ns()
        if data.startswith(b'{"t":"push","topic":"area"'):
            m = AREA_HEAD.search(data, 0, 400)
            if not m:
                return
            area, rev = m.group(1).decode(), int(m.group(2))
            self.rev[area] = rev
            i = data.find(PROBE)
            self.rec.count("area_pushes")
            self.rec.count("area_bytes", len(data))
            if int(m.group(3)) > 2500:
                # State of an area the game stopped reporting (nobody watched it): not a delivery latency.
                self.rec.count("area_pushes_stale")
            elif i >= 0:
                stamp = int(data[i + 9:i + 28])
                if self.last_probe.get(area) != stamp:
                    self.last_probe[area] = stamp
                    self.rec.sample("area_push", (now - stamp) / 1e6, len(data) // 1024)
            return
        msg = json.loads(data)
        t = msg.get("t")
        if t == "welcome":
            self.id_prefix = msg["id_prefix"]
            resumed = msg.get("resumed", False)
            self.rec.count("resumed" if resumed else "fresh_sessions")
            if not resumed:
                self.rev.clear()
                self.last_probe.clear()
                self.event_n = 0
                # A new gateway epoch ends commands we could not see finish as unknown.
                for cid in list(self.inflight):
                    self.terminal(cid, "unknown_local")
                self.seen_events.clear()
            self.resume = msg["gw_epoch"]
            self.gw_epoch = msg["gw_epoch"]
        elif t == "res":
            if not msg.get("ok"):
                self.rec.count("res_errors")
                self.rec.error(self.cred["id"], msg.get("err"))
            else:
                r = msg["r"]
                for rej in r.get("rejected", []) if isinstance(r, dict) else []:
                    self.rec.error(self.cred["id"], rej)
                    self.terminal(rej["id"], "rejected")
        elif t == "push":
            self.push(msg["topic"], msg["body"], now)
        elif t == "bye":
            raise Closed("bye " + str(msg.get("code")))

    def push(self, topic, body, now):
        if topic == "meta":
            self.boot = body["boot"]
            self.rec.count("meta_pushes")
        elif topic == "boot":
            self.seen_events.clear()
        elif topic == "events":
            self.event_n = max(self.event_n, body["through"])
            self.rec.count("events_pushes")
            for row in body["rows"]:
                if row["n"] in self.seen_events:
                    self.rec.count("events_duplicate")
                    if len(self.rec.dups) < 40:
                        self.rec.dups.append([round(time.time() - self.rec.t0, 1), self.cred["id"], row["n"], self.event_n, body["through"], len(body["rows"])])
                    continue
                self.seen_events.add(row["n"])
                text = (row.get("data") or {}).get("text", "")
                if row["kind"] == "chat" and text.startswith("c1"):
                    self.rec.sample("chat_event", (now - int(text[1:20])) / 1e6)
            if len(self.seen_events) > 50_000:
                self.seen_events = set(sorted(self.seen_events)[-10_000:])
        elif topic == "result":
            cid, rev, state = body["id"], body["rev"], body["state"]
            self.rec.count("result_pushes")
            if self.acked.get(cid, 0) >= rev and state != "pending":
                self.rec.count("result_after_ack")
            self.unacked[cid] = rev
            if state in ("final", "unknown"):
                if cid in self.inflight:
                    sent = self.inflight[cid]
                    self.rec.sample("command_rtt", (now / 1e9 - sent) * 1000, cid)
                    self.terminal(cid, state)
                elif cid in self.final and self.final[cid] == "final" and state == "final":
                    self.rec.count("result_duplicate")
        elif topic == "drafts":
            for item in body["items"]:
                cmd = item.get("command", {})
                cid = cmd.get("id")
                if cid and cid not in self.drafted and item.get("client") in self.owned_ai:
                    self.drafted.add(cid)
                    asyncio.create_task(self.decide(cid, cmd.get("text", "ok")))

    def terminal(self, cid, state):
        self.inflight.pop(cid, None)
        self.final[cid] = state
        self.rec.count("cmd_" + state)

    async def decide(self, cid, text):
        try:
            await asyncio.sleep(self.rng.uniform(0.05, 0.4))  # a DM clicking Approve
            if self.ws:
                t = time.time()
                await self.send_req("draft.decide", {"id": cid, "text": text})
                self.rec.sample("draft_approve_sent", (time.time() - t) * 1000)
                self.rec.count("drafts_approved")
        except Exception as e:  # noqa: BLE001
            self.rec.error(self.cred["id"], f"decide: {e}")

    async def commands(self):
        if self.a.dm_cmd_rate <= 0:
            return  # this desktop only watches
        while time.time() < self.stop_at:
            await asyncio.sleep(self.rng.expovariate(self.a.dm_cmd_rate))
            if time.time() >= self.stop_at:
                return
            if not self.boot or not self.id_prefix:
                continue
            known = [x for x in self.watch if x in self.rev]
            if not known:
                continue
            area = self.rng.choice(known)
            ano = int(area[1:])
            self.counter += 1
            cid = "%s%s.%d" % (self.id_prefix, self.proc, self.counter)
            tgt = "%02x1%05x" % (ano, self.rng.randrange(10, 40))  # players are the first few creatures
            cmd = {"id": cid, "area": area, "op": "npc_say", "target": tgt, "text": "Greetings %d" % self.counter,
                   "boot": self.boot, "ttl_ms": 30000}
            self.inflight[cid] = time.time()
            self.rec.count("cmd_sent")
            try:
                await self.send_req("cmds", {"commands": [cmd]})
            except (OSError, ConnectionError):
                return

    async def churn(self):
        """Focus changes about every 20 s, a watch-set swap about every minute."""
        while True:
            await asyncio.sleep(self.rng.uniform(10, 30))
            try:
                if self.rng.random() < 0.3:
                    self.watch = watch_set(self.rng, self.areas, self.a.dense)
                    await self.send_req("watch.set", {"areas": self.watch})
                else:
                    area = self.rng.choice(self.watch)
                    await self.send_req("focus.set", {"objects": [{"area": area, "id": "%02x1%05x" % (int(area[1:]), self.rng.randrange(10, 40))}]})
            except (OSError, ConnectionError):
                return

    async def acker(self):
        while True:
            await asyncio.sleep(1.0)
            if self.unacked or self.event_n:
                results = [[cid, rev] for cid, rev in self.unacked.items()]
                try:
                    await self.ws.send(json.dumps({"t": "ack", "results": results, "event_n": self.event_n}, separators=(",", ":")))
                except (OSError, ConnectionError):
                    return
                for cid, rev in results:
                    self.acked[cid] = max(self.acked.get(cid, 0), rev)
                self.unacked.clear()


class Agent:
    def __init__(self, cred, a, rec, stop_at, seed):
        self.cred, self.a, self.rec, self.stop_at = cred, a, rec, stop_at
        self.rng = random.Random(seed)
        self.cursor = None
        self.areas = ["a%02d" % i for i in range(a.areas)]
        self.counter = 0
        self.issued = {}
        self.seen = set()
        self.reported = ["a00"]  # areas the game reports, learnt from live_status

    async def poll_lane(self):
        http = Http(self.a.host, self.a.port, self.cred["token"])
        while time.time() < self.stop_at:
            try:
                if self.cursor is None:
                    st, r = await http.post("/agent/v1/call/live_events", {"limit": 1})
                    if st != 200:
                        raise RuntimeError(f"events {st} {str(r)[:100]}")
                    self.cursor = r["result"]["last"]
                body = {"since": self.cursor, "wait_ms": 10000, "limit": 100}
                st, r = await http.post("/agent/v1/call/live_events", body)
                now = time.time_ns()
                if st == 429:
                    self.rec.count("agent_429")
                    await asyncio.sleep(0.5)
                    continue
                if st != 200:
                    raise RuntimeError(f"events {st} {str(r)[:100]}")
                page = r["result"]
                self.rec.count("agent_polls")
                for e in page["events"]:
                    n = e["n"]
                    if n in self.seen:
                        self.rec.count("agent_event_duplicate")
                        if len(self.rec.dups) < 40:
                            self.rec.dups.append([round(time.time() - self.rec.t0, 1), self.cred["id"], n, self.cursor, page["last"], len(page["events"]), page.get("boot")])
                        continue
                    self.seen.add(n)
                    text = (e.get("data") or {}).get("text", "")
                    if e["kind"] == "chat" and text.startswith("c1"):
                        self.rec.sample("agent_wake", (now - int(text[1:20])) / 1e6)
                if len(self.seen) > 50_000:
                    self.seen = set(sorted(self.seen)[-10_000:])
                if page.get("reset"):  # the gateway's log restarted below our cursor
                    self.rec.count("agent_cursor_reset")
                    self.cursor = page["last"]
                    self.seen.clear()  # event numbers are reused by the new log
                else:
                    self.cursor = max(self.cursor, page["last"])
            except Exception as e:  # noqa: BLE001
                self.rec.error(self.cred["id"], f"poll: {type(e).__name__} {e}")
                http.close()
                await asyncio.sleep(1)

    async def action_lane(self):
        http = Http(self.a.host, self.a.port, self.cred["token"])
        next_cmd = time.time() + self.rng.expovariate(self.a.agent_cmd_rate)
        while time.time() < self.stop_at:
            await asyncio.sleep(self.rng.uniform(1.0, 4.0))
            try:
                kind = self.rng.random()
                t = time.time()
                if time.time() >= next_cmd:
                    next_cmd = time.time() + self.rng.expovariate(self.a.agent_cmd_rate)
                    area = self.rng.choice(self.reported)
                    ano = int(area[1:])
                    self.counter += 1
                    cid = "%s-%d" % (self.cred["id"].replace(":", ""), self.counter)
                    body = {"id": cid, "area": area, "target": "%02x1%05x" % (ano, self.rng.randrange(10, 40)),
                            "text": "Agent line %d" % self.counter}
                    st, r = await http.post("/agent/v1/call/live_npc_say", body)
                    self.rec.sample("agent_call", (time.time() - t) * 1000, "npc_say")
                    if st != 200:
                        self.rec.count("agent_cmd_refused")
                        self.rec.error(self.cred["id"], f"npc_say {st} {str(r)[:160]}")
                        continue
                    self.rec.count("agent_cmd_queued")
                    queued = r["result"]["queued"]
                    self.issued[queued] = time.time()
                    asyncio.create_task(self.await_result(queued))
                elif kind < 0.5:
                    st, r = await http.post("/agent/v1/call/live_find", {"text": "probe", "limit": 20})
                    self.rec.sample("agent_call", (time.time() - t) * 1000, "find")
                elif kind < 0.8:
                    st, r = await http.post("/agent/v1/call/live_status", {})
                    self.rec.sample("agent_call", (time.time() - t) * 1000, "status")
                    if st == 200:
                        self.reported = [x["area"] for x in r["result"].get("areas", [])] or self.reported
                else:
                    st, r = await http.post("/agent/v1/call/live_area_state", {"area": self.rng.choice(self.reported)})
                    self.rec.sample("agent_call", (time.time() - t) * 1000, "area_state")
                if st != 200:
                    self.rec.count("agent_call_errors")
                    self.rec.error(self.cred["id"], f"call {st} {str(r)[:160]}")
            except Exception as e:  # noqa: BLE001
                self.rec.error(self.cred["id"], f"call: {type(e).__name__} {e}")
                http.close()

    async def await_result(self, cid):
        http = Http(self.a.host, self.a.port, self.cred["token"])
        deadline = time.time() + 90
        try:
            while time.time() < deadline:
                st, r = await http.post("/agent/v1/call/live_wait", {"id": cid, "timeout_ms": 5000})
                res = (r or {}).get("result") or {}
                if st == 200 and not res.get("pending"):
                    self.rec.count("agent_cmd_final" if res.get("ok") else "agent_cmd_failed")
                    if not res.get("ok"):
                        self.rec.error(self.cred["id"], f"wait {str(res)[:160]}")
                    self.rec.sample("agent_cmd_rtt", (time.time() - self.issued[cid]) * 1000, cid)
                    return
                await asyncio.sleep(0.2)
            self.rec.count("agent_cmd_no_result")
        except Exception as e:  # noqa: BLE001
            self.rec.error(self.cred["id"], f"wait: {type(e).__name__} {e}")
        finally:
            http.close()

    async def run(self):
        await asyncio.gather(self.poll_lane(), self.action_lane())


async def lag_probe(rec, stop_at):
    while time.time() < stop_at:
        t = time.perf_counter()
        await asyncio.sleep(0.1)
        rec.lag_max = max(rec.lag_max, (time.perf_counter() - t - 0.1) * 1000)


async def main(a):
    creds = json.loads(Path(a.creds).read_text())[a.role]
    creds = creds[a.first:a.first + a.count]
    rec = Recorder(a.t0)
    stop_at = a.t0 + a.duration
    start_in = a.t0 + a.delay - time.time()
    if start_in > 0:
        await asyncio.sleep(start_in)
    cls = Dm if a.role == "dm" else Agent
    actors = [cls(c, a, rec, stop_at, a.seed + i) for i, c in enumerate(creds)]

    async def staggered(actor, i):
        await asyncio.sleep(i * a.stagger)
        await actor.run()

    await asyncio.gather(lag_probe(rec, stop_at + 5), *(staggered(x, i) for i, x in enumerate(actors)))
    # Drain: let in-flight commands finish before counting what was lost.
    out = {"role": a.role, "first": a.first, "count": len(actors), "counts": rec.counts, "errors": rec.errors,
           "samples": rec.samples, "lag_max_ms": rec.lag_max, "dups": rec.dups}
    if a.role == "dm":
        out["inflight_at_end"] = sum(len(x.inflight) for x in actors)
        out["commands"] = {cid: s for x in actors for cid, s in x.final.items()}
    Path(a.out).write_text(json.dumps(out))


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--role", choices=["dm", "agent"], required=True)
    p.add_argument("--creds", required=True)
    p.add_argument("--first", type=int, default=0)
    p.add_argument("--count", type=int, default=1)
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, default=8080)
    p.add_argument("--t0", type=float, required=True, help="scenario start (epoch seconds)")
    p.add_argument("--delay", type=float, default=0.0)
    p.add_argument("--duration", type=float, required=True)
    p.add_argument("--drain", type=float, default=20.0)
    p.add_argument("--stagger", type=float, default=0.15)
    p.add_argument("--areas", type=int, default=50)
    p.add_argument("--dense", type=int, default=5)
    p.add_argument("--dm-cmd-rate", type=float, default=0.2)
    p.add_argument("--agent-cmd-rate", type=float, default=0.1)
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--out", required=True)
    asyncio.run(main(p.parse_args()))
