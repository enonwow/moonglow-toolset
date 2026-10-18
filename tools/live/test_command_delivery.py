"""Command transport regressions on the existing private WSL engine harness."""
import json


def publish(p, commands=(), received=()):
    p.pub.send("PUBLISH", "mglive-events.cmd", json.dumps({
        "v": 1, "commands": list(commands), "received": list(received)}))
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
