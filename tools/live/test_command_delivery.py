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
