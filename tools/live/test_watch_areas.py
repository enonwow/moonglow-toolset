"""Native area subscription proof; invoked by test-events.sh --phase watch_areas."""
import time


def run(proof):
    def names():
        return [area["area"] for area in proof.state["areas"]]

    assert names() == [] and proof.state["watched_areas"] == [], proof.state
    checks = {"cold_start_areas": names()}
    for areas, on, error in [([], True, "invalid"), (["events", "EVENTS"], True, "invalid"),
                             (["events", "missing"], True, "not_found"), (["../bad"], True, "invalid"),
                             (["events"], 1, "invalid"), (["events"]*65, True, "invalid")]:
        proof.command(op="watch_areas", area=None, areas=areas, on=on, expect=error)
        assert names() == [] and proof.state["watched_areas"] == [], proof.state
    placed = proof.command(op="spawn", area="events2", kind="placeable", r="mget_chest", x=5, y=5, z=0, f=0)["obj"]
    assert names() == []
    before = proof.state["seq"]
    started = time.monotonic()
    proof.command(op="watch_areas", area=None, areas=["EVENTS2"], on=True)
    proof.until(lambda: names() == ["events2"])
    checks["watch_ms"] = round((time.monotonic()-started)*1000, 2)
    checks["watch_ticks"] = proof.state["seq"] - before
    assert checks["watch_ticks"] <= 2  # in-flight report may precede the first new tick
    assert proof.state["areas"][0]["full"], proof.state
    assert any(o["id"] == placed for o in proof.state["areas"][0]["plc"]), proof.state
    checks["first_report_full_placeable"] = placed
    proof.command(op="watch_areas", area=None, areas=["events2"], on=True)  # idempotent flag
    assert proof.state["watched_areas"] == ["events2"]
    proof.command(op="watch_areas", area=None, areas=["events2"], on=False)
    proof.until(lambda: names() == [] and proof.state["watched_areas"] == [])
    stopped = proof.state["seq"]
    proof.until(lambda: proof.state["seq"] >= stopped + 3)
    assert names() == [], proof.state
    checks["unwatch_empty_ticks"] = 3
    # Explicit launcher extras remain independent of MGLIVE_WATCHED.
    proof.command(op="run_chunk", code='SetLocalString(GetModule(),"MGLIVE_AREAS","events");')
    proof.until(lambda: names() == ["events"])
    proof.command(op="watch_areas", area=None, areas=["events"], on=False)
    proof.until(lambda: names() == ["events"] and proof.state["watched_areas"] == [])
    checks["explicit_extra_survives_unwatch"] = True
    actor = proof.command(op="spawn",kind="creature",r="nw_bartender",x=5,y=5,z=0,f=0)["obj"]
    proof.command(op="teleport",target=actor,area="events2",x=7,y=7,z=0)
    proof.until(lambda: any(o["id"] == actor for a in proof.state["areas"] if a["area"] == "events2" for o in a["cre"]))
    handoff = proof.state["seq"]
    assert proof.state["watched_areas"] == []
    proof.until(lambda: proof.state["seq"] > handoff + 5)
    assert names() == ["events"], proof.state
    checks["teleport_handoff_expires"] = True
    proof.command(op="run_chunk", code='MgReturn(JsonDump(JsonBool(GetIsObjectValid(GetFirstPC()))));', **{"return": True})
    checks["pc_limit"] = "No connected private PC. Existing GetFirstPC/GetNextPC occupancy predicate is unchanged; actual PC occupancy needs a connected client."
    return checks
