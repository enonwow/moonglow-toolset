"""Synthetic game: drives Redis exactly like the Live game scripts (no NWN server needed).

* state report on `<prefix>.state` at 1 Hz: full snapshot first, then deltas (`base_seq`),
  a full report every 30 ticks, a full area when it is newly watched or a resync is asked;
* commands from `<prefix>.cmd` (`watch_areas` and `resync` are honoured, anything else gets an
  ok result), results retained until the envelope echoes their receipt, ACK ids in the report;
* durable events: the producer Lua script (`event-stream.lua`) XADDs chat lines and combat
  bursts to `<prefix>.events` the way `mglive_stream.nss` does.

Every area's creature 0 is a probe whose name carries the publish time (`P<ns>`), and every chat
line carries its own (`c<ns>`), so clients measure latency from the raw frames.
"""
import argparse
import json
import os
import random
import signal
import threading
import time
from pathlib import Path

from resp import Redis, RedisError, connect_retry

FULL_EVERY = 30
KINDS = {"cre": "1", "plc": "2", "doors": "3", "items": "4"}
HERE = Path(__file__).resolve().parent
LUA = next(p for p in (HERE / "event-stream.lua", HERE.parent / "event-stream.lua") if p.exists()).read_text()


def row(kind, area_no, n, rng, pc=False):
    oid = "%02x%s%05x" % (area_no, KINDS[kind], n)
    x, y = rng.uniform(1, 150), rng.uniform(1, 150)
    o = {"id": oid, "tag": "T%02d_%s%d" % (area_no, kind, n), "r": "res_%s_%d" % (kind, n % 40),
         "name": "%s %d" % (kind.upper(), n), "x": round(x, 3), "y": round(y, 3), "z": 0.0,
         "f": round(rng.uniform(0, 360), 1), "hp": [30, 30], "act": 0, "own": False}
    if kind == "cre":
        o.update({"pc": pc, "faction": n % 5, "level": 1 + n % 20, "lookHash": rng.randrange(1 << 30),
                  "portrait": "po_human_%d_" % (n % 9)})
        if pc:
            o["name"] = "Player %d" % n
            o["look"] = {"gender": 0, "app": 6, "equip": [rng.randrange(1 << 20) for _ in range(180)]}
    else:
        o.update({"open": False, "lock": False, "useable": True, "app": n % 300})
    return o


class Area:
    def __init__(self, no, name, count, rng, pcs):
        self.name, self.no, self.rng = name, no, rng
        self.objs, self.enc, self.dirty = {}, {}, set()
        self.cre, self.static = [], []
        shares = {"cre": 0.25, "plc": 0.60, "doors": 0.05, "items": 0.10}
        n = 0
        for kind, share in shares.items():
            for i in range(max(2, int(count * share))):
                o = row(kind, no, n, rng, pc=(kind == "cre" and i >= 1 and i <= pcs))
                n += 1
                self.objs[o["id"]] = (kind, o)
                (self.cre if kind == "cre" else self.static).append(o)
        for oid, (kind, o) in self.objs.items():
            self.enc[oid] = json.dumps(o, separators=(",", ":"))

    def touch(self, o):
        kind = self.objs[o["id"]][0]
        self.enc[o["id"]] = json.dumps(o, separators=(",", ":"))
        self.dirty.add((kind, o["id"]))

    def tick(self, stamp, hot):
        probe = self.cre[0]
        probe["name"] = "P%d" % stamp
        self.touch(probe)
        for o in self.rng.sample(self.cre, max(1, len(self.cre) // 8)):
            o["x"] = round(o["x"] + self.rng.uniform(-2, 2), 3)
            o["y"] = round(o["y"] + self.rng.uniform(-2, 2), 3)
            if hot:
                o["hp"] = [max(0, o["hp"][0] - self.rng.randrange(5)) or 30, 30]
                o["combat"] = True
            self.touch(o)
        if self.rng.random() < 0.05:  # a door or chest changes now and then
            o = self.rng.choice(self.static)
            o["open"] = not o.get("open", False)
            self.touch(o)

    def report(self, full):
        lists = {k: [] for k in KINDS}
        if full:
            for oid, (kind, _) in self.objs.items():
                lists[kind].append(self.enc[oid])
        else:
            for kind, oid in self.dirty:
                lists[kind].append(self.enc[oid])
        self.dirty.clear()
        body = ",".join('"%s":[%s]' % (k, ",".join(v)) for k, v in lists.items())
        return '{"area":"%s","full":%s,%s,"size":[10,10],"removed":[]}' % (
            self.name, "true" if full else "false", body)


class Game:
    def __init__(self, a):
        self.a, self.prefix = a, a.prefix
        rng = random.Random(7)
        self.boot = "load-%d" % int(time.time())
        self.areas = {}
        pc_areas = {3: 3, 7: 3, 12: 2}
        for i in range(a.areas):
            name = "a%02d" % i
            count = a.dense_objects if i < a.dense else a.objects
            self.areas[name] = Area(i, name, count, rng, pc_areas.get(i, 0))
        self.pc_areas = {"a%02d" % i for i in pc_areas}
        self.lock = threading.Lock()
        self.watched = set()
        self.known = set()  # areas already sent full in this baseline
        self.results = {}  # id -> result row (retained until the receipt is echoed)
        self.dwell = []  # [command id, received ns, first report ns]
        self.recv = {}
        self.pending_ids = {}
        self.force_full = True
        self.seq = 0
        self.stop = threading.Event()
        self.stats = {"reports": 0, "report_bytes": 0, "full_reports": 0, "publish_errors": 0,
                      "sub_reconnects": 0, "commands": 0, "redelivered_commands": 0, "events": 0, "event_errors": 0,
                      "max_report_ms": 0.0, "hot_until": 0.0}
        self.event_seq = 0
        self.started = time.time()
        self.hot_area = None
        self.rng = random.Random(11)

    # ---- commands -------------------------------------------------------------------------
    def handle_command_message(self, body):
        try:
            msg = json.loads(body)
        except ValueError:
            return
        if msg.get("boot") != self.boot:
            return
        now = time.time_ns()
        with self.lock:
            for receipt in msg.get("received", []):
                for cid, r in list(self.results.items()):
                    if r["receipt"] == receipt:
                        del self.results[cid]
            for c in msg.get("commands", []):
                cid = c.get("id")
                if not cid:
                    continue
                if cid in self.recv:
                    self.stats["redelivered_commands"] += 1
                    continue
                self.recv[cid] = now
                self.stats["commands"] += 1
                op = c.get("op")
                if op == "watch_areas":
                    areas = {x.lower() for x in c.get("areas", [])}
                    if c.get("on"):
                        self.watched |= areas
                    else:
                        self.watched -= areas
                        self.known -= areas
                elif op == "resync":
                    self.force_full = True
                self.results[cid] = {"id": cid, "ok": True, "obj": "", "receipt": "%s:%d" % (self.boot, len(self.recv))}

    def subscribe_loop(self):
        channel = self.prefix + ".cmd"
        while not self.stop.is_set():
            r = connect_retry(self.a.host, self.a.port, self.stop)
            if r is None:
                return
            try:
                r.send("SUBSCRIBE", channel)
                r.read()
                r.sock.settimeout(1.0)
                while not self.stop.is_set():
                    try:
                        m = r.read()
                    except TimeoutError:
                        continue
                    if m and m[0] == b"message":
                        self.handle_command_message(m[2])
            except (OSError, RedisError, ConnectionError):
                self.stats["sub_reconnects"] += 1
            finally:
                r.close()

    # ---- state reports --------------------------------------------------------------------
    def build_report(self):
        self.seq += 1
        stamp = time.time_ns()
        with self.lock:
            selected = sorted(self.watched | self.pc_areas)
            selected = [s for s in selected if s in self.areas]
            full_all = self.force_full or self.seq == 1 or self.seq % FULL_EVERY == 0
            if full_all:
                self.known = set()
            hot = self.hot_area if time.time() < self.stats["hot_until"] else None
            rows = []
            for name in selected:
                area = self.areas[name]
                area.tick(stamp, name == hot)
                rows.append(area.report(full_all or name not in self.known))
                self.known.add(name)
            results = list(self.results.values())
            first = [r["id"] for r in results if r["id"] in self.recv and r["id"] not in self.pending_ids]
            watched = sorted(self.watched)
            self.force_full = False
        for cid in first:
            self.pending_ids[cid] = True
        body = ['"v":1,"script_version":2,"text_codepage":"cp1250"', '"prefix":"%s"' % self.prefix,
                '"boot":"%s"' % self.boot, '"seq":%d' % self.seq, '"watched_areas":%s' % json.dumps(watched),
                '"ack":%s' % json.dumps([r["id"] for r in results]), '"results":%s' % json.dumps(results),
                '"event_stream":true', '"dropped":0']
        if full_all:
            body.append('"factions":[{"id":0,"name":"PC"},{"id":1,"name":"Hostile"},{"id":2,"name":"Commoner"}]')
        else:
            body.append('"base_seq":%d' % (self.seq - 1))
        body.append('"areas":[%s]' % ",".join(rows))
        text = "{" + ",".join(body) + "}"
        return text, first, full_all, stamp

    def publish_loop(self):
        r = None
        next_at = time.monotonic()
        while not self.stop.is_set():
            next_at += 1.0
            try:
                if r is None:
                    r = connect_retry(self.a.host, self.a.port, self.stop)
                    if r is None:
                        return
                    next_at = time.monotonic()
                    self.force_full = True
                t0 = time.perf_counter()
                text, first, full, _ = self.build_report()
                t_pub = time.time_ns()
                r.call("PUBLISH", self.prefix + ".state", text)
                self.stats["max_report_ms"] = max(self.stats["max_report_ms"], (time.perf_counter() - t0) * 1000)
                self.stats["reports"] += 1
                self.stats["report_bytes"] += len(text)
                self.stats["full_reports"] += full
                for cid in first:
                    self.dwell.append([cid, self.recv[cid], t_pub])
            except (OSError, RedisError, ConnectionError):
                self.stats["publish_errors"] += 1
                if r:
                    r.close()
                r = None
                self.force_full = True
            self.stop.wait(max(0.0, next_at - time.monotonic()))

    # ---- durable events -------------------------------------------------------------------
    def make_events(self, tick):
        rows, now = [], time.time_ns()
        rate = self.a.warm_rate if time.time() - self.started < self.a.warm_seconds else self.a.chat_rate
        chat = rate * tick
        n_chat = int(chat) + (1 if self.rng.random() < chat - int(chat) else 0)
        names = list(self.areas)
        for _ in range(n_chat):
            self.event_seq += 1
            area = self.rng.choice(names)
            rows.append({"seq": self.event_seq, "kind": "chat", "time": "%d" % (now // 10**9), "id": "%02x100000" % int(area[1:]),
                         "name": "Speaker", "area": area, "target": None, "channel": 1,
                         "text": "c%d line %d" % (now, self.event_seq)})
        if time.time() < self.stats["hot_until"]:
            for _ in range(int(40 * tick)):
                self.event_seq += 1
                rows.append({"seq": self.event_seq, "kind": "damage", "time": "%d" % (now // 10**9), "area": self.hot_area,
                             "id": "%02x100001" % int(self.hot_area[1:]), "target": "%02x100002" % int(self.hot_area[1:]),
                             "amount": self.rng.randrange(1, 20), "text": "hit"})
        return rows

    def event_loop(self):
        r, tick = None, 0.1
        next_at, next_burst = time.monotonic(), time.monotonic() + 20
        while not self.stop.is_set():
            next_at += tick
            if time.monotonic() >= next_burst:
                self.hot_area = self.rng.choice(sorted(self.watched) or ["a00"])
                self.stats["hot_until"] = time.time() + 5
                next_burst = time.monotonic() + 30
            rows = self.make_events(tick)
            try:
                if rows:
                    if r is None:
                        r = connect_retry(self.a.host, self.a.port, self.stop)
                        if r is None:
                            return
                    args = ["EVAL", LUA, 3, self.prefix + ".events", self.prefix + ".events.meta",
                            "%s.events.last.%s" % (self.prefix, self.boot), self.boot, "game", "cp1250"]
                    for e in rows:
                        args += [e["seq"], json.dumps(e, separators=(",", ":"))]
                    r.call(*args)
                    self.stats["events"] += len(rows)
            except (OSError, RedisError, ConnectionError):
                self.stats["event_errors"] += 1
                # Rows made while Redis was down are lost, as in the game when XADD fails.
                if r:
                    r.close()
                r = None
            self.stop.wait(max(0.0, next_at - time.monotonic()))

    def run(self):
        threads = [threading.Thread(target=f, daemon=True) for f in (self.subscribe_loop, self.publish_loop, self.event_loop)]
        for t in threads:
            t.start()
        signal.signal(signal.SIGTERM, lambda *_: self.stop.set())
        signal.signal(signal.SIGINT, lambda *_: self.stop.set())
        started = time.time()
        while not self.stop.is_set():
            self.stop.wait(1.0)
            if self.a.out:
                self.write(started)
        for t in threads:
            t.join(timeout=3)
        self.write(started)

    def write(self, started):
        data = {"seconds": time.time() - started, "boot": self.boot, "stats": self.stats,
                "dwell_ms": [[cid, (t1 - t0) / 1e6] for cid, t0, t1 in self.dwell],
                "watched": sorted(self.watched)}
        tmp = self.a.out + ".tmp"
        Path(tmp).write_text(json.dumps(data))
        os.replace(tmp, self.a.out)


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--host", default="redis")
    p.add_argument("--port", type=int, default=6379)
    p.add_argument("--prefix", default="mglive")
    p.add_argument("--areas", type=int, default=50)
    p.add_argument("--dense", type=int, default=5)
    p.add_argument("--objects", type=int, default=200)
    p.add_argument("--dense-objects", type=int, default=1100)
    p.add_argument("--chat-rate", type=float, default=10.0)
    p.add_argument("--warm-seconds", type=float, default=0.0, help="start with --warm-rate chat lines a second for this long, to fill the event log")
    p.add_argument("--warm-rate", type=float, default=400.0)
    p.add_argument("--out", default="")
    args = p.parse_args()
    args.dense_objects = max(args.dense_objects, args.objects)
    Game(args).run()
