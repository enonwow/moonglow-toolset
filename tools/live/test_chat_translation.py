"""Acceptance and latency assay on the existing private NWNX server helper.

NPC listeners execute real OnConversation callbacks. Explicit fixture-only
adapters enumerate PC/DM surrogates; this does not prove connected client UI.
"""
import heapq
import json
import os
import subprocess
from pathlib import Path
import select
import threading
import time


def prepare(module):
    path = module / "mglive_xl.nss"
    text = path.read_text()
    text = text.replace('return GetIsPC(o) || GetIsDM(o) || GetIsDMPossessed(o);',
                        'return GetLocalInt(o,"MGET_XL_PLAYER");')
    text = text.replace('return GetIsDM(o) || GetIsDMPossessed(o);',
                        'return GetLocalInt(o,"MGET_XL_DM");')
    begin = text.index('    json clients=JsonArray(); object pc=GetFirstPC();')
    end = text.index('\n}\n', begin)
    text = text[:begin] + '    return GetLocalJson(GetModule(),"MGET_XL_CLIENTS");' + text[end:]
    # Capture time is from the actual interception, not command submission or Redis receipt.
    needle = 'JsonObjectSetInplace(pending,"at",JsonInt(GetMicrosecondCounter()));'
    assert needle in text
    text = text.replace(needle, needle + '\n    SetLocalInt(m,"MGET_XL_AT_"+GetStringLeft(text,FindSubString(text," ")),JsonGetInt(JsonObjectGet(pending,"at")));')
    # Fixture-only delivery-boundary trace, including delayed original fallback.
    # It observes real MgXlSend calls without changing the production audience.
    needle = '    NWNX_Chat_SendMessage(channel,text,speaker,listener);'
    assert needle in text
    text = text.replace(needle, '''    string sample=GetStringLeft(text,FindSubString(text," "));
    if(GetStringLeft(sample,7)=="budget_")
    {
        string trace="MGET_XL_SENT_"+sample;
        json sent=GetLocalJson(m,trace);
        if(JsonGetType(sent)!=JSON_TYPE_ARRAY) sent=JsonArray();
        JsonArrayInsertInplace(sent,JsonString(ObjectToString(listener)));
        SetLocalJson(m,trace,sent);
    }
''' + needle)
    path.write_text(text)


class Translator:
    def __init__(self, redis):
        self.sub, self.pub = redis(), redis()
        self.sub.send("SUBSCRIBE", "mglive-events.xl.req", "mglive-events.xl.proof")
        self.sub.read(); self.sub.read()
        self.bridge = None
        binary = os.environ.get("MGLIVE_XL_BRIDGE")
        if binary:
            executable = Path(binary).resolve()
            expected = Path(__file__).resolve().parents[2] / "target/debug/examples/translation_engine.exe"
            assert executable == expected.resolve() and executable.is_file()
            self.bridge = subprocess.Popen([str(executable)], stdin=subprocess.PIPE, stdout=subprocess.PIPE)
            try:
                assert select.select([self.bridge.stdout], [], [], 15)[0], "bridge readiness timeout"
                assert self.bridge.stdout.readline().strip() == b"translation bridge ready"
            except BaseException:
                if self.bridge.poll() is None:
                    self.bridge.stdin.write(b"stop\n"); self.bridge.stdin.flush()
                    try:
                        self.bridge.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        self.bridge.terminate(); self.bridge.wait(timeout=5)
                self.sub.close(); self.pub.close()
                raise
        self.requests, self.deliveries, self.errors = [], [], []
        self.stop = threading.Event()
        self.thread = threading.Thread(target=self.run, daemon=True)
        self.thread.start()

    def run(self):
        scheduled = []
        sequence = 0
        try:
            while not self.stop.is_set():
                now = time.monotonic()
                while scheduled and scheduled[0][0] <= now:
                    _, _, response = heapq.heappop(scheduled)
                    # Duplicate answers must not duplicate native delivery.
                    for _ in range(2):
                        self.pub.send("PUBLISH", "mglive-events.xl", json.dumps(response))
                        assert self.pub.read() > 0
                if not select.select([self.sub.sock], [], [], .005)[0]:
                    continue
                message = self.sub.read()
                row = json.loads(message[2])
                if message[1].endswith(".proof"):
                    self.deliveries.append(row)
                    continue
                self.requests.append(row)
                if self.bridge:
                    continue
                text = row["text"]
                if text.startswith("xl_drop"):
                    continue
                delay = int(text.split("_", 2)[1][1:]) if text.startswith("xl_b") else 0
                if text.startswith("xl_late"):
                    delay = 1200
                if text.startswith("xl_move"):
                    delay = 600
                translations = {lang: text + " translated-" + lang for lang in row["to"]}
                if text.startswith("xl_command"):
                    translations = {lang: " /danger" for lang in row["to"]}
                if text.startswith("xl_markup"):
                    translations = {lang: text.replace("*", "") for lang in row["to"]}
                sequence += 1
                heapq.heappush(scheduled, (time.monotonic() + delay / 1000, sequence,
                                          {"slot": row["slot"], "req": row["req"], "to": translations}))
        except Exception as exc:
            self.errors.append(repr(exc))

    def close(self):
        self.stop.set(); self.thread.join(timeout=3)
        self.sub.close(); self.pub.close()
        if self.bridge:
            self.bridge.stdin.write(b"stop\n"); self.bridge.stdin.flush()
            assert self.bridge.wait(timeout=10) == 0
        assert not self.thread.is_alive() and not self.errors, self.errors


def run(p, stress=False):
    stub = Translator(type(p.pub))
    try:
        p.chunk('SetLocalInt(GetModule(),"MGET_XL_MODE",1);')
        p.command(op="run_script", script="mget_xl")
        assert p.native('JsonBool(GetIsListening(GetObjectByTag("XL_NEAR")))'), "listener lost its listening flag"
        assert p.native('JsonString(GetEventScript(GetObjectByTag("XL_NEAR"),5007))') == "mget_xlhear"
        diagnostic = p.native('GetLocalJson(GetModule(),"MGET_XL_DIAGNOSTIC")')
        assert diagnostic["safe"] and diagnostic["cr_refused"] and diagnostic["colour_refused"], diagnostic
        assert diagnostic["language_supported"]
        window = json.dumps(diagnostic["language_window"])
        assert all(label in window for label in ["English", "Polski", "Deutsch", "Francais", "Espanol", "Italiano"])
        assert p.native('JsonString(GetEventScript(GetModule(),EVENT_SCRIPT_MODULE_ON_CLIENT_ENTER))') == "mglive_xlent"
        assert p.native('JsonInt(GetLocalInt(GetModule(),"MGLIVE_XL"))') == 0
        p.chunk('SetLocalInt(GetModule(),"MGLIVE_XL",TRUE);SetLocalInt(GetModule(),"MGET_XL_MODE",0);')
        near = {"XL_SAME", "XL_NEAR", "XL_MID", "XL_DM", "XL_NPC"}
        all_ = near | {"XL_FAR", "XL_OTHER"}
        expected = {1: near, 2: all_, 3: near - {"XL_MID"}, 4: {"XL_NEAR"},
                    6: {"XL_NEAR", "XL_OTHER"}, 14: {"XL_DM"},
                    17: near, 18: all_, 19: near - {"XL_MID"}, 20: {"XL_NEAR"},
                    22: {"XL_NEAR", "XL_OTHER"}, 30: {"XL_DM"}}

        def line(ident, channel=1):
            code = f'SetLocalInt(GetModule(),"MGET_XL_MODE",0);SetLocalInt(GetModule(),"MGET_XL_CHANNEL",{channel});SetLocalString(GetModule(),"MGET_XL_TEXT","{ident} hello *waves*");'
            p.chunk(code); p.command(op="run_script", script="mget_xl")

        def rows(ident):
            return [row for row in stub.deliveries if row["id"] == ident]

        table = []
        if not stress:
            line("xl_native", 99)
            p.until(lambda: len(rows("xl_native")) >= len(near), timeout=8)
            assert len(rows("xl_native")) == len(near), rows("xl_native")
            p.chunk('SetLocalInt(GetModule(),"MGLIVE_XL",FALSE);')
            before = len(stub.requests)
            line("xl_off", 99)
            p.until(lambda: len(rows("xl_off")) >= len(near), timeout=8)
            assert len(stub.requests) == before
            assert len(rows("xl_off")) == len(near), rows("xl_off")
            p.chunk('SetLocalInt(GetModule(),"MGLIVE_XL",TRUE);')
            for channel, listeners in expected.items():
                ident = f"xl_c{channel}"
                line(ident, channel)
                p.until(lambda: len(rows(ident)) >= len(listeners), timeout=8)
                heard = rows(ident)
                assert {x["listener"] for x in heard} == listeners, (channel, heard)
                assert len(heard) == len(listeners), (channel, heard)
                table.append({"channel": channel, "heard": sorted(listeners)})
            for ident in ["xl_drop", "xl_late", "xl_command", "xl_markup"]:
                line(ident)
                p.until(lambda: len(rows(ident)) >= len(near), timeout=8)
                assert len(rows(ident)) == len(near) and not any(x["translated"] for x in rows(ident)), rows(ident)
            p.chunk('SetLocalInt(GetModule(),"MGET_XL_MODE",5);')
            p.command(op="run_script", script="mget_xl")
            p.until(lambda: len(rows("xl_move")) >= len(near), timeout=8)
            moved = next(x for x in rows("xl_move") if x["listener"] == "XL_NEAR")
            assert "translated-pl" in moved["text"], moved
            assert p.native('JsonString(GetResRef(GetArea(GetObjectByTag("XL_NEAR"))))') == "events2"
            p.chunk('SetLocalInt(GetModule(),"MGET_XL_MODE",6);')
            p.command(op="run_script", script="mget_xl")
            old = len(stub.requests)
            p.chunk('SetLocalInt(GetModule(),"MGET_XL_MODE",3);')
            p.command(op="run_script", script="mget_xl")
            assert len(stub.requests) == old, "guarded native send re-entered interception"
        else:
            p.chunk('SetLocalInt(GetModule(),"MGLIVE_RESYNC",TRUE);')
            p.until(lambda: all(a.get("full") for a in p.state["areas"]))
            counts = {a["area"]: sum(len(a.get(k, [])) for k in ["cre", "plc", "doors", "items"]) for a in p.state["areas"]}
            assert len(counts) == 5 and sum(counts.values()) >= 1000, counts
            p.command(op="run_script", script="st_setup")
            deadline = time.monotonic() + 120
            while not p.native('JsonInt(GetLocalInt(GetModule(),"ST_READY"))'):
                assert time.monotonic() < deadline
            p.chunk('SetLocalInt(GetModule(),"ST_RUNNING",TRUE);ExecuteScript("st_frame",GetModule());ExecuteScript("st_drive",GetModule());')
            for ident in ["xl_drop", "xl_late"]:
                line(ident)
                p.until(lambda: len(rows(ident)) >= len(near), timeout=8)
                assert len(rows(ident)) == len(near) and not any(x["translated"] for x in rows(ident)), rows(ident)
        measurements = []
        for delay in [0, 200, 600]:
            count = 60
            p.chunk(f'SetLocalInt(GetModule(),"MGET_XL_MODE",2);SetLocalInt(GetModule(),"MGET_XL_COUNT",{count});SetLocalInt(GetModule(),"MGET_XL_DELAY",{delay});')
            p.command(op="run_script", script="mget_xl")
            wanted = {f"xl_b{delay}_{i}" for i in range(count)}
            p.until(lambda: sum(x["id"] in wanted for x in stub.deliveries) >= count * len(near), timeout=30)
            measured = [x for x in stub.deliveries if x["id"] in wanted]
            assert len(measured) == count * len(near)
            assert len({(x["id"], x["listener"]) for x in measured}) == len(measured)
            assert all({x["listener"] for x in measured if x["id"] == ident} == near for ident in wanted)
            translated_targets = [x for x in measured if x["listener"] not in ("XL_SAME", "XL_NPC")]
            values = sorted(x["us"] / 1000 for x in translated_targets)
            assert any(x["translated"] for x in translated_targets), "stub returned no usable translations"
            captures = sorted(x["at"] for x in stub.requests if x["text"].split(" ", 1)[0] in wanted)
            assert len(captures) == count
            measurements.append({"stub_ms": delay, "lines": count, "held_deliveries": len(values),
                                 "captured_lines_per_second": round((count - 1) * 1_000_000 / (captures[-1] - captures[0]), 2),
                                 "p50_ms": values[len(values)//2], "p95_ms": values[int(len(values)*.95)],
                                 "max_ms": values[-1], "translated": sum(x["translated"] for x in translated_targets),
                                 "original": sum(not x["translated"] for x in translated_targets)})
        p.chunk('SetLocalInt(GetModule(),"ST_RUNNING",FALSE);')
        # Drain all fallbacks and late/duplicate stub answers before uniqueness verdict.
        end = time.monotonic() + 2
        while time.monotonic() < end: p.poll(.1)
        keys = [(x["id"], x["listener"]) for x in stub.deliveries if x["id"] != "xl_guard"]
        assert len(keys) == len(set(keys)), "duplicate native listener callback"
        cases = {}
        for ident in ["xl_native", "xl_off", "xl_drop", "xl_late", "xl_command", "xl_markup", "xl_move"]:
            delivered = rows(ident)
            if delivered:
                cases[ident] = {"delivered": len(delivered), "translated": sum(x["translated"] for x in delivered),
                                "max_ms": max(x["us"] / 1000 for x in delivered)}
        return {"table": table, "measurements": measurements, "stress": stress,
                "transport": "production Bridge + TranslationPipeline" if stub.bridge else "Python stub",
                "cases": cases,
                "native_listener_callbacks": len(keys), "requests": len(stub.requests),
                "limitations": "NPC OnConversation delivery; fixture-only PC/DM enumeration adapters. No connected PC/DM UI or native player-ID tell echo proof."}
    finally:
        stub.close()
        (Path(__file__).resolve().parents[2] / "target/test-output/live-events/xl-observations.json").write_text(
            json.dumps({"requests": stub.requests, "deliveries": stub.deliveries, "errors": stub.errors}, indent=2))
