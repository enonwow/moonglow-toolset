"""Private demo assertions, launched only by test-next.ps1 after ownership preflight."""
import hashlib
import json
import os
from pathlib import Path
import socket
import ssl
import subprocess
import sys
import time
import urllib.error
import urllib.request


class Redis:
    def __init__(self, port):
        self.sock = socket.create_connection(("127.0.0.1", port), timeout=10)
        self.file = self.sock.makefile("rb")

    def send(self, *parts):
        parts = [p.encode("utf-8") for p in parts]
        self.sock.sendall(b"*%d\r\n" % len(parts) + b"".join(
            b"$%d\r\n" % len(p) + p + b"\r\n" for p in parts))

    def read(self):
        line = self.file.readline()
        if not line:
            raise EOFError("Redis closed")
        kind, value = line[:1], line[1:-2]
        if kind == b"*":
            return [self.read() for _ in range(int(value))]
        if kind == b"$":
            size = int(value)
            if size < 0:
                return None
            data = self.file.read(size)
            assert self.file.read(2) == b"\r\n"
            return data.decode("utf-8")
        if kind == b":":
            return int(value)
        if kind == b"+":
            return value.decode("utf-8")
        raise RuntimeError("Unexpected Redis reply")

    def close(self):
        self.file.close()
        self.sock.close()


def main():
    demo, proof = map(Path, sys.argv[1:3])
    phase = sys.argv[3] if len(sys.argv) > 3 else "1"
    assert phase in ("1", "1b", "2")
    stem = {"1": "engine", "1b": "engine-1b", "2": "engine-factions"}[phase]
    session = json.loads((demo / "session.json").read_text(encoding="utf-8-sig"))
    area = session["areas"].split(",")[0]
    prefix = session["prefix"]
    assert prefix == "mglive-p" + phase, "Only the dedicated test session is allowed"
    token = os.environ["MGLIVE_TEST_TOKEN"]
    uri = os.environ["MGLIVE_TEST_URI"]
    evidence = {"passed": False, "area": area, "startedUnix": time.time(),
                "session": {key: session[key] for key in ("owner", "guiPid", "guiStarted", "prefix")}}
    repo = Path(__file__).resolve().parents[2]
    inputs = [demo / "DEMO-Live-01.mod", demo / "inputs.json", repo / "target/debug/moonglow.exe",
              repo / "tools/live/nwn/mglive_inc.nss", repo / "tools/live/nwn/mglive_exec.nss"]
    evidence["inputs"] = {str(p.relative_to(repo)): hashlib.sha256(p.read_bytes()).hexdigest()
                          for p in inputs}
    opener = urllib.request.build_opener(
        urllib.request.ProxyHandler({}),
        urllib.request.HTTPSHandler(context=ssl._create_unverified_context()))

    def api(path, body=None):
        request = urllib.request.Request(uri + "/" + path,
            data=None if body is None else json.dumps(body).encode("utf-8"),
            headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
        try:
            response = opener.open(request, timeout=5)
        except urllib.error.HTTPError as response_error:
            response = response_error
        with response:
            return response.status, json.load(response)

    def wait(check, seconds=15):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            value = check()
            if value:
                return value
            time.sleep(0.1)
        raise TimeoutError("Phase 1 assertion timed out")

    def command(op, **fields):
        status, reply = api("command", {"area": area, "op": op, **fields})
        assert status == 200, reply
        result = wait(lambda: next((r["result"] for r in api("status")[1]["results"]
                                   if r["result"]["id"] == reply["queued"]), None))
        assert result["ok"], result
        return result

    def actor(object_id=None):
        return next((o for o in api("area/" + area)[1]["creatures"]
                     if o["id"] == (object_id or owned)), None)

    wsl = ["wsl"] + (["-d", session["distro"]] if session.get("distro") else [])

    def wsl_python(source):
        return subprocess.check_output(wsl + ["-e", "python3", "-c", source],
                                       text=True, encoding="utf-8", timeout=10)

    def log_values(marker):
        return json.loads(wsl_python(
            'from pathlib import Path;import json;'
            'p=Path.home()/"mglive-home/logs.0/nwserverLog1.txt";'
            'mark=' + repr("] " + marker + " ") + ';'
            'print(json.dumps([line.split(mark,1)[1] for line in p.read_text().splitlines() if mark in line]))'))

    probe_seq = 0

    def probe(expression):
        nonlocal probe_seq
        probe_seq += 1
        marker = "MGLIVE_TEST_" + str(probe_seq)
        command("run_chunk", code='void main(){WriteTimestampedLogEntry("'
                + marker + ' "+IntToString(' + expression + '));}', wrap=False)
        return int(log_values(marker)[-1])

    def faction(object_expression):
        # Native GFF serialization is independent of NWNX's chunk-call behavior.
        return probe('JsonGetInt(JsonObjectGet(JsonObjectGet(ObjectToJson('
                     + object_expression + '),"FactionID"),"value"))')

    subscriber, publisher = Redis(session["redisPort"]), Redis(session["redisPort"])
    subscriber.send("SUBSCRIBE", prefix + ".state")
    subscriber.read()
    owned = None
    made = []
    try:
        evidence["boot"] = api("status")[1]["boot"]
        owned = command("spawn", kind="creature", r="nw_bartender", x=25, y=25, z=0, f=0)["obj"]
        made.append(owned)
        original = wait(actor)
        assert original["own"] and not original["pc"] and not original["dm"]
        evidence["target"] = owned
        if phase == "2":
            object_expression = f'StringToObject("{owned}")'
            command("run_chunk", target=owned, code="SetImmortal(OBJECT_SELF,TRUE);")
            assert original.get("faction") == faction(object_expression), original
            assert isinstance(original.get("friendly"), list), original
            evidence["initialFaction"] = original["faction"]
            evidence["states"] = []
            for standard in ("HOSTILE", "COMMONER", "MERCHANT", "DEFENDER"):
                command("run_chunk", target=owned,
                        code=f"ChangeToStandardFaction(OBJECT_SELF,STANDARD_FACTION_{standard});")
                expected = faction(object_expression)
                reported = wait(lambda: o if (o := actor()) and o.get("faction") == expected else None)
                assert isinstance(reported.get("friendly"), list)
                assert isinstance(reported.get("hostile"), list)
                evidence["states"].append({"standard": standard, "gffFaction": expected,
                    "reportedFaction": reported["faction"], "combat": reported["combat"],
                    "friendly": reported["friendly"], "hostile": reported["hostile"]})
            command("destroy", target=owned)
            wait(lambda: actor() is None)
            made.remove(owned)
            evidence["passed"] = True
            evidence["relationshipLimit"] = "No player client in automated run; selected-player relationships require owner acceptance."
            print("Phase 2 factions engine passed; owned actor removed")
            return
        # The same trim function as a full PC report, exercised on our actor.
        source = ('#include "mglive_inc"\nvoid main(){'
                  'string data=JsonDump(MgLiveLook(OBJECT_SELF));int i;'
                  'for(i=0;i<GetStringLength(data);i+=600)'
                  'WriteTimestampedLogEntry("MGLIVE_LOOK "+GetSubString(data,i,600));}')
        command("run_chunk", target=owned, code=source, wrap=False)
        look = json.loads("".join(log_values("MGLIVE_LOOK")))
        (proof / (stem + "-look.json")).write_text(json.dumps(look, indent=2), encoding="utf-8")
        assert isinstance(look, dict) and "__data_type" in look, "Look is not nwn-lib GFF JSON; see engine-look.json"
        size = len(json.dumps(look, separators=(",", ":"), ensure_ascii=False).encode("utf-8"))
        assert size <= 32768
        for field in ("ItemList", "FeatList", "SkillList", "ClassList", "VarTable", "EffectList"):
            assert field not in look, field
        for item in look.get("Equip_ItemList", {}).get("value", []):
            assert item["__struct_id"] in (1, 2, 16, 32, 64)
            assert not {"PropertiesList", "ItemList", "VarTable"}.intersection(item)
        evidence["lookBytes"] = size
        if phase == "1b":
            # Read only the presence of password flags; never return credentials.
            flags = json.loads(wsl_python(
                'from pathlib import Path;import json;'
                'pid=(Path.home()/"mglive-home/server.pid").read_text().strip();'
                'args=(Path("/proc")/pid/"cmdline").read_bytes().decode().split(chr(0));'
                'print(json.dumps({k:k in args for k in ["-playerpassword","-dmpassword","-adminpassword"]}))'))
            assert flags == {"-playerpassword": False, "-dmpassword": False, "-adminpassword": True}, flags
            evidence["privatePasswordFlags"] = flags
            attacker_obj = f'StringToObject("{owned}")'
            command("npc_move", target=owned, x=35, y=25, z=0, hold=True, timeout=2)
            assert probe(f"GetCommandable({attacker_obj})") == 0
            time.sleep(2.2)
            assert probe(f"GetCommandable({attacker_obj})") == 1
            evidence["moveHoldTimeoutReleases"] = True
            command("npc_move", target=owned, x=25, y=25, z=0, hold=True, timeout=30)
            wait(lambda: o if (o := actor()) and (o["x"]-25)**2 + (o["y"]-25)**2 < 0.64 else None)
            wait(lambda: probe(f"GetCommandable({attacker_obj})") == 1)
            evidence["moveArrivalReleases"] = True
            victim = command("spawn", kind="creature", r="nw_bartender", x=27, y=25, z=0, f=180)["obj"]
            made.append(victim)
            other = wait(lambda: actor(victim))
            assert other["own"] and not other["pc"] and not other["dm"]
            attacker_obj, victim_obj = [f'StringToObject("{object_id}")' for object_id in (owned, victim)]
            assert probe(f"GetFactionEqual({attacker_obj},{victim_obj})") == 1
            original_faction = faction(attacker_obj)
            assert original_faction == look["FactionID"]["value"]
            evidence["originalFaction"] = original_faction
            evidence["sameFaction"] = True
            for object_id in (owned, victim):
                command("run_chunk", target=object_id, code="SetImmortal(OBJECT_SELF, TRUE);")
            start = time.monotonic()
            attack_result = command("npc_attack", target=owned, victim=victim, hold=True, timeout=30)
            assert attack_result.get("faction") == "hostile_temporarily", attack_result
            evidence["attackResult"] = attack_result
            try:
                fighting = wait(lambda: o if (o := actor()) and o.get("combat") and o.get("target") == victim else None,
                                seconds=max(0.1, 6 - (time.monotonic() - start)))
            except TimeoutError:
                evidence["attackFailure"] = {
                    "attacker": actor(), "victim": actor(victim),
                    "enemy": probe(f"GetIsEnemy({victim_obj},{attacker_obj})"),
                    "reputation": probe(f"GetReputation({attacker_obj},{victim_obj})"),
                    "commandable": probe(f"GetCommandable({attacker_obj})"),
                    "action": probe(f"GetCurrentAction({attacker_obj})"),
                    "hasTarget": probe(f"GetAttackTarget({attacker_obj})=={victim_obj}"),
                }
                raise
            evidence["attackObservedSeconds"] = round(time.monotonic() - start, 3)
            assert evidence["attackObservedSeconds"] <= 6
            evidence["fighting"] = {k: fighting[k] for k in ("id", "combat", "target")}
            assert probe(f"GetCommandable({attacker_obj})") == 0
            evidence["temporaryFaction"] = faction(attacker_obj)
            assert evidence["temporaryFaction"] != original_faction
            command("npc_move", target=owned, x=25, y=25, z=0)
            assert probe(f"GetCommandable({attacker_obj})") == 1
            assert faction(attacker_obj) == original_faction
            evidence["holdNewMoveReleases"] = True
            command("npc_attack", target=owned, victim=victim, hold=True, timeout=2)
            time.sleep(2.2)
            assert probe(f"GetCommandable({attacker_obj})") == 1
            assert faction(attacker_obj) == original_faction
            evidence["holdTimeoutReleases"] = True
            evidence["timeoutRestoresFaction"] = True
            command("npc_attack", target=owned, victim=victim, hold=True, timeout=4)
            command("npc_attack", target=owned, victim=victim, hold=True, timeout=30)
            time.sleep(4.2)
            assert probe(f"GetCommandable({attacker_obj})") == 0
            assert faction(attacker_obj) == evidence["temporaryFaction"]
            evidence["oldTimeoutKeepsNewHold"] = True
            command("run_chunk", target=victim, code="SetImmortal(OBJECT_SELF,FALSE);ApplyEffectToObject(DURATION_TYPE_INSTANT,EffectDeath(),OBJECT_SELF);")
            wait(lambda: actor(victim).get("dead"))
            assert probe(f"GetCommandable({attacker_obj})") == 1
            assert faction(attacker_obj) == original_faction
            evidence["victimDeathReleases"] = True
            command("destroy", target=victim)
            wait(lambda: actor(victim) is None)
            made.remove(victim)
            command("run_chunk", target=owned, code="SetImmortal(OBJECT_SELF,FALSE);")
        command("run_chunk", target=owned,
                code="ApplyEffectToObject(DURATION_TYPE_INSTANT, EffectDeath(), OBJECT_SELF);")
        corpse = wait(lambda: o if (o := actor()) and o.get("dead") else None)
        evidence["corpse"] = {k: corpse[k] for k in ("id", "own", "dead", "combat", "hp")}
        status, refusal = api("command", {"area": area, "op": "npc_move", "target": owned,
                                          "x": 20, "y": 20, "z": 0})
        assert status == 400 and refusal["code"] == "dead", refusal
        evidence["storeRefusal"] = refusal
        # Bypass the store deliberately to test the game's independent gate.
        raw = {"v": 1, "script_version": 2, "boot": api("status")[1]["boot"], "prefix": prefix, "commands": [{"id": "phase1-dead-game", "area": area,
            "op": "npc_move", "target": owned, "x": 20, "y": 20, "z": 0}]}
        publisher.send("PUBLISH", prefix + ".cmd", json.dumps(raw))
        assert publisher.read() > 0
        end = time.monotonic() + 15
        result = None
        while time.monotonic() < end:
            event = subscriber.read()
            if event[1] == prefix + ".state":
                result = next((r for r in json.loads(event[2]).get("results", [])
                               if r["id"] == "phase1-dead-game"), None)
                if result:
                    break
        assert result and not result["ok"] and result["code"] == "dead", result
        evidence["gameRefusal"] = result
        command("destroy", target=owned)
        wait(lambda: actor() is None)
        made.remove(owned)
        owned = None
        evidence["passed"] = True
    except Exception as error:
        evidence["error"] = repr(error)
        raise
    finally:
        for object_id in made:
            try:
                o = actor(object_id)
                if o and o["own"] and not o["pc"] and not o["dm"]:
                    command("destroy", target=object_id)
            except Exception as error:
                evidence["cleanupError"] = str(error)
        subscriber.close()
        publisher.close()
        (proof / (stem + ".json")).write_text(json.dumps(evidence, indent=2), encoding="utf-8")
    print(f"Phase {phase} engine passed; all owned actors removed")


if __name__ == "__main__":
    main()
