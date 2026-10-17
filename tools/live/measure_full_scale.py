"""Strict no-TMI assay with explicit native scale and cumulative VM headroom."""
import json
import os
import re
import time


def run(proof, output, scratch):
    area = os.environ.get("MGLIVE_DEMO_AREA", "demo_area")
    def command(**args):
        return proof.command(area=area, **args)
    def chunk(code, **args):
        return command(op="run_chunk", code=code, **args)
    def value(expr):
        return chunk('MgReturn(JsonDump('+expr+'));', **{"return": True})["value"]
    log = scratch / "logs.0/nwserverLog1.txt"
    engine = scratch / "logs.0/nwengineLog.txt"
    assert len(proof.state["factions"]) >= 256
    first = proof.frames[0]
    assert first["seq"] == 1, "Cold module load must publish its first report"
    crowd = []
    for i in range(15):
        result = command(op="spawn", kind="creature", r="nw_bartender", count=10,
                         faction=2, x=5+(i%5)*5, y=5+(i//5)*5, z=0, f=0)
        crowd.extend(result["objects"])
    actors, looks = [], []
    for i in range(3):
        actor = command(op="spawn", kind="creature", r="nw_bandit001", faction=2,
                        x=3+i*3, y=3, z=0, f=0)["obj"]
        actors.append(actor)
        chunk('SetLocalInt(GetModule(),"MP_SCALE_PHASE",0);')
        command(op="run_script", target=actor, script="mp_scale")
        for _ in range(10):
            chunk(f'object a=StringToObject("{actor}"); int i; for(i=0;i<20;i++) CreateItemOnObject("nw_it_mring001",a);')
        command(op="run_script", target=actor, script="mp_stress")
        look = value('GetLocalJson(GetModule(),"MP_LOOK_STRESS")')
        assert look["raw_bytes"] >= 250000 and look["inventory"] >= 200, look
        assert look["look_equipment"] == 5 and look["look_bytes"] <= 32768, look
        looks.append(look)
    chunk('SetLocalInt(GetModule(),"MGLIVE_DELTA",1); SetLocalInt(GetModule(),"MGLIVE_RESYNC",1);')
    full = proof.state
    objects = sum(len(a[k]) for a in full["areas"] for k in ("cre", "plc", "doors", "items"))
    assert objects >= 218 and all(a["full"] for a in full["areas"]), (objects, full["seq"])
    samples = [first, full]
    checks = []
    for actor in actors:
        inventory = command(op="view_inventory", target=actor)["value"]
        assert inventory["truncated"] and 0 < len(inventory["items"]) <= 128
        locals_ = command(op="dump_locals", target=actor)["value"]
        assert locals_["truncated"], locals_
        result = command(op="capture_object", target=actor, expect="invalid")
        checks.append({"actor": actor, "inventory_rows": len(inventory["items"]),
                       "locals": locals_, "large_capture_code": result["code"]})
    # A legal, substantial capture must also exercise the recursive GFF validator.
    small = command(op="spawn", kind="creature", r="nw_bandit001", faction=2,
                    x=6, y=7, z=0, f=0)["obj"]
    chunk(f'object a=StringToObject("{small}"); int i; for(i=0;i<50;i++) CreateItemOnObject("nw_it_mring001",a);')
    captured = command(op="capture_object", target=small)["object_json"]
    captured_bytes = len(json.dumps(captured, separators=(",", ":")).encode())
    assert 50000 < captured_bytes <= 262144, captured_bytes
    clone = command(op="spawn_json", json=captured, x=9, y=7, z=0, f=0)["obj"]
    command(op="view_inventory", target=clone)
    command(op="take_gold", target=small, amount=1)
    command(op="run_script", target=small, script="mglive_probe")
    command(op="patrol", target=small, points=[{"x":10,"y":7,"z":0},{"x":12,"y":7,"z":0}], loop=True, run=False)
    proof.until(lambda: any(e.get("id") == small for e in proof.matching("patrol")))
    command(op="stop_patrol", target=small)
    command(op="play_creature_animation", target=small, animation="talknormal", loop=True, speed=1, seconds=2)
    end = time.monotonic()+3
    while time.monotonic() < end:
        proof.poll(.25)
    chunk('SetLocalInt(GetModule(),"MP_SCALE_PHASE",1);')
    prior_damage = {e["seq"] for e in proof.matching("damage")}
    command(op="run_script", target=actors[0], script="mp_scale")
    def burst_damage():
        return {e["seq"] for e in proof.matching("damage") if e["seq"] not in prior_damage and e.get("target")==actors[0]}
    proof.until(lambda:len(burst_damage())>=256,timeout=30)
    assert len(burst_damage()) == 256, "Need every event from the real damage burst"
    assert proof.state.get("event_stream") and not proof.state.get("events")
    samples.append(proof.state)
    chunk('SetLocalInt(GetModule(),"MP_SCALE_PHASE",2);')
    command(op="run_script", target=small, script="mp_scale")
    proof.until(lambda: bool(proof.matching("spell")), timeout=30)
    command(op="kill", target=clone)
    proof.until(lambda: any(e.get("id") == clone for e in proof.matching("death")))
    # Another full snapshot with all three uncached looks and the large crowd.
    seq = proof.state["seq"]
    chunk('SetLocalInt(GetModule(),"MGLIVE_RESYNC",1);')
    samples.append(proof.state)
    proof.until(lambda: proof.state["seq"] >= seq+3)
    lines = log.read_text(errors="replace").splitlines()
    errors = [line for line in engine.read_text(errors="replace").splitlines() if "TOO MANY INSTRUCTIONS" in line]
    assert not errors, errors
    assert not any("MP_SCALE_EQUIP_FAILED" in line for line in lines)
    budgets = {}
    for line in lines:
        match = re.search(r"MP_HEADROOM script=(\w+) before=(\d+) after=(\d+) limit=(\d+)", line)
        if not match:
            continue
        name, before, after, limit = match.groups()
        before, after, limit = int(before), int(after), int(limit)
        row = budgets.setdefault(name, {"calls":0, "max_cumulative":0, "max_own_and_children":0, "limit":limit})
        row["calls"] += 1
        row["max_cumulative"] = max(row["max_cumulative"], after)
        row["max_own_and_children"] = max(row["max_own_and_children"], after-before)
        row["headroom"] = limit-row["max_cumulative"]
        assert 0 < after < limit
    assert {"mglive_load", "mglive_look", "mp_native", "mglive_ps", "mglive_damage", "mglive_chat", "mglive_spell", "mglive_gold", "mglive_probe", "mglive_evflush"} <= budgets.keys(), budgets.keys()
    result = {"schema":"live-full-scale-v1", "connected_players":0, "objects":objects,
              "pc_path_surrogates":actors, "looks":looks, "factions":len(full["factions"]),
              "checks":checks, "valid_capture_bytes":captured_bytes,
              "burst_events":len(burst_damage()), "budgets":budgets, "tmi":errors,
              "limits":"Five visible equipment slots on NPC surrogates; no genuine player login/disconnect/NUI-click execution. Other command branches have their own phase proofs."}
    (output/"engine.json").write_text(json.dumps(result, indent=2))
    (output/"full-scale-reports.json").write_text(json.dumps(samples,separators=(",",":")))
    print(f"full scale: {objects} objects,3 large looks,256 factions and256 damage events; no TMI", flush=True)
    return result
