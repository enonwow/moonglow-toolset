"""Command transport regressions on the existing private WSL engine harness."""
import json


def publish(p, commands=(), received=()):
    p.pub.send("PUBLISH", "mglive-events.cmd", json.dumps({
        "v": 1, "boot": p.state["boot"], "prefix": "mglive-events", "commands": list(commands), "received": list(received)}))
    assert p.pub.read() > 0


def receipts(p):
    failed = p.command(expect="compile_error", op="run_chunk", code="this is not NWScript;")
    ident, receipt = failed["id"], failed["receipt"]
    start = p.state["seq"]
    p.until(lambda: p.state["seq"] >= start + 2)
    assert any(r["id"] == ident and not r["ok"] for r in p.state["results"])
    publish(p, received=[receipt + "-stale"])
    start = p.state["seq"]
    p.until(lambda: p.state["seq"] > start)
    assert any(r["id"] == ident for r in p.state["results"])
    publish(p, received=[receipt])
    p.until(lambda: all(r["id"] != ident for r in p.state["results"]))
    assert ident not in p.state["ack"]
    return {"failed_result_retained_with_subscriber": True,
            "wrong_receipt_retained": True, "explicit_receipt_cleared": True}


def result_budget(p):
    # Synthetic large rows exercise the native transport budget independently
    # of object capture size/content validation.
    code = '''string blob="x"; int i; for(i=0;i<18;i++) blob+=blob;
    blob=GetStringLeft(blob,180000); json rows=JsonArray();
    for(i=0;i<4;i++) {
        string id="budget-"+IntToString(i);
        json r=JsonObjectSet(JsonObject(),"id",JsonString(id));
        r=JsonObjectSet(r,"ok",JsonBool(TRUE));
        r=JsonObjectSet(r,"value",JsonString(blob));
        r=JsonObjectSet(r,"receipt",JsonString(GetLocalString(GetModule(),"MGLIVE_BOOT")+":"+id));
        r=JsonObjectSet(r,"expires",JsonInt(GetLocalInt(GetModule(),"MGLIVE_SEQ")+100));
        rows=JsonArrayInsert(rows,r);
    } SetLocalJson(GetModule(),"MGLIVE_RESULTS",rows);'''
    publish(p, [{"id": "budget-setup", "area": "events", "op": "run_chunk", "code": code}])
    seen = set()
    sizes = []
    while len(seen) < 4:
        p.until(lambda: any(r["id"].startswith("budget-") and r["id"] != "budget-setup"
                            for r in p.state.get("results", [])))
        batch = p.state["results"]
        size = len(json.dumps(batch, separators=(",", ":")).encode())
        assert size <= 524288, size
        sizes.append(size)
        seen.update(r["id"] for r in batch if r["id"] != "budget-setup")
        assert set(p.state["ack"]) == {r["id"] for r in batch}
        seq = p.state["seq"]
        publish(p, received=[r["receipt"] for r in batch])
        p.until(lambda: p.state["seq"] > seq)
    return {"report_result_bytes": sizes, "all_deferred_results_delivered": sorted(seen)}


def interrupted(p):
    commands = [
        {"id": "batch-first", "area": "events", "op": "run_chunk", "code":
         'SetLocalInt(GetModule(),"MGET_FIRST",GetLocalInt(GetModule(),"MGET_FIRST")+1);'},
        {"id": "batch-interrupted", "area": "events", "op": "run_chunk", "code":
         'SetLocalInt(GetModule(),"MGET_SECOND",GetLocalInt(GetModule(),"MGET_SECOND")+1); int i=0; while(TRUE) i++;'},
    ]
    publish(p, commands)
    p.until(lambda: any(r.get("code") == "ambiguous" and r["id"] == "batch-interrupted"
                        for r in p.state["results"]))
    first = next(r for r in p.state["results"] if r["id"] == "batch-first")
    assert first["ok"]
    publish(p, commands)
    p.until(lambda: p.state["seq"] >= 3)
    counts = p.native('JsonArrayInsert(JsonArrayInsert(JsonArray(),JsonInt(GetLocalInt(GetModule(),"MGET_FIRST"))),JsonInt(GetLocalInt(GetModule(),"MGET_SECOND")))')
    assert counts == [1, 1], counts
    answer = next(r for r in p.state["results"] if r["id"] == "batch-interrupted")
    assert not answer["ok"] and answer["code"] == "ambiguous", answer
    return {"counts_after_retransmission": counts, "interrupted": answer,
            "first_result_persisted": True}


def boot_bound(p):
    command = {"id": "bound-command", "op": "run_chunk", "area": "events",
               "code": 'SetLocalInt(GetModule(),"MGET_BOUND",GetLocalInt(GetModule(),"MGET_BOUND")+1);'}
    for boot, prefix in [("old-boot", "mglive-events"), (p.state["boot"], "other-server")]:
        p.pub.send("PUBLISH", "mglive-events.cmd", json.dumps({
            "v": 1, "boot": boot, "prefix": prefix, "commands": [command]}))
        assert p.pub.read() > 0
    start = p.state["seq"]
    p.until(lambda: p.state["seq"] >= start + 2)
    assert p.native('JsonInt(GetLocalInt(GetModule(),"MGET_BOUND"))') == 0
    publish(p, [command])
    p.until(lambda: any(r["id"] == command["id"] for r in p.state["results"]))
    assert p.native('JsonInt(GetLocalInt(GetModule(),"MGET_BOUND"))') == 1
    return {"wrong_boot_rejected": True, "wrong_prefix_rejected": True, "current_boot_count": 1}
