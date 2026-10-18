"""Dense-area paging proof, exclusively through the private WSL events helper."""

KINDS = ("plc", "doors", "items", "waypoints", "sounds", "encounters", "stores", "triggers")
MAX_FULL_REPORTS = 16


def prepare(module):
    # Inject faults and PC/DM surrogates only into the scratch module. The real
    # native creature query and both priority passes remain production code.
    path = module / "mglive_inc.nss"
    text = path.read_text()
    needle = "return GetIsPC(o) || GetIsDM(o);"
    assert text.count(needle) == 1
    text = text.replace(needle, 'return GetIsPC(o) || GetIsDM(o) || GetLocalInt(o,"MGET_DENSE_PC");')
    needle = "    return players;"
    assert text.count(needle) == 1
    text = text.replace(needle, '''    json actors=GetLocalJson(GetModule(),"MGET_DENSE_ACTORS");int ai;
    for(ai=0;ai<JsonGetLength(actors);ai++) {
        object actor=StringToObject(JsonGetString(JsonArrayGet(actors,ai)));
        if(GetArea(actor)==area && GetLocalInt(actor,"MGET_DENSE_PC"))
            JsonArrayInsertInplace(players,JsonString(ObjectToString(actor)));
    }
''' + needle)
    text = text.replace('JsonBool(GetIsPC(o))', 'JsonBool(GetIsPC(o) || GetLocalInt(o,"MGET_DENSE_PC"))')
    text = text.replace('JsonBool(GetIsDM(o))', 'JsonBool(GetIsDM(o) || GetLocalInt(o,"MGET_DENSE_DM"))')
    start = text.index("json MgLiveArea(object area, int bFull)")
    end = text.index("// Selection by resref", start)
    area = text[start:end].replace("if(!bFull) return result;", "if(!bFull) { return result; }")
    area = area.replace("return result;", 'JsonObjectSetInplace(result,"proof_instructions",JsonInt(NWNX_Util_GetInstructionsExecuted()));JsonObjectSetInplace(result,"proof_limit",JsonInt(NWNX_Util_GetInstructionLimit()));return result;')
    text = text[:start] + area + text[end:]
    path.write_text(text)
    path = module / "mglive_report.nss"
    text = path.read_text()
    needle = "            area=MgLiveArea(GetLocalObject(m,key),full);"
    assert text.count(needle) == 1
    text = text.replace(needle, '''            if(GetLocalInt(m,"MGET_REPORT_ABORT"))
            {
                DeleteLocalInt(m,"MGET_REPORT_ABORT");
                int spin=0;while(TRUE) { spin++; }
            }
''' + needle)
    needle = "    string data = JsonDump(body);"
    assert text.count(needle) == 1
    text = text.replace(needle, '''    if(GetLocalInt(m,"MGET_PUBLISH_ABORT"))
    {
        DeleteLocalInt(m,"MGET_PUBLISH_ABORT");
        int spin=0;while(TRUE) { spin++; }
    }
''' + needle)
    path.write_text(text)


def check_frame(frame, actors):
    areas = {a["area"]: a for a in frame["areas"]}
    assert {"events", "events2"} <= areas.keys(), areas.keys()
    dense = areas["events"]
    assert dense.get("paged") is True
    assert 0 < dense["proof_instructions"] < dense["proof_limit"] * 3 // 4, dense
    assert not dense.get("cre_partial"), dense
    creatures = dense.get("cre", [])
    assert actors <= {o["id"] for o in creatures}, (actors, creatures)
    # Both surrogate PCs must precede all ordinary NPCs.
    priority = [bool(o.get("pc") or o.get("dm")) for o in creatures]
    assert priority == sorted(priority, reverse=True), priority
    return dense


def completed_scan(frames, actors, expected):
    generation = None
    next_index = 0
    ids = set()
    pages = 0
    for frame in frames:
        area = check_frame(frame, actors)
        page = area.get("static_page")
        if page is None:
            continue
        if page["index"] == 0:
            generation, next_index, ids, pages = page["generation"], 0, set(), 0
        if page["generation"] != generation or page["index"] != next_index:
            generation = None
            continue
        next_index += 1
        pages += 1
        ids.update(o["id"] for k in KINDS for o in area.get(k, []) if o.get("tag") == "DENSE_FILLER")
        if page["complete"] and len(ids) == expected:
            assert pages <= MAX_FULL_REPORTS, pages
            return {"objects": len(ids), "full_reports": pages, "generation": generation}
    return None


def add_population(p, count):
    for offset in range(0, count, 100):
        batch = min(100, count - offset)
        p.chunk(f'''object area=GetObjectByTag("EVENTS");int i;
            for(i=0;i<{batch};i++) {{
                object o=CreateObject(OBJECT_TYPE_PLACEABLE,"mget_polish",Location(area,Vector(15.0,15.0,0.0),0.0),FALSE,"DENSE_FILLER");
                if(!GetIsObjectValid(o)) break;
                SetLocalInt(GetModule(),"MGET_DENSE_CREATED",GetLocalInt(GetModule(),"MGET_DENSE_CREATED")+1);
            }}''')


def prove_scan(p, actors, expected):
    start = p.state["seq"]
    recovered = p.state.get("report_recovered", 0)
    p.chunk('SetLocalInt(GetModule(),"MGLIVE_RESYNC",TRUE);')
    def frames():
        return [f for f in p.frames if f["seq"] > start]
    p.until(lambda: completed_scan(frames(), actors, expected) is not None)
    result = completed_scan(frames(), actors, expected)
    assert all(f.get("report_recovered", 0) == recovered for f in frames()), "ordinary collection hit the watchdog"
    assert all(p.raw_sizes[f["seq"]] < 5 * 1024 * 1024 for f in frames())
    # Quick reports must also contain the entire creature set, without statics.
    done = p.state["seq"]
    p.until(lambda: p.state["seq"] >= done + 2 and "base_seq" in p.state)
    for f in frames():
        a = check_frame(f, actors)
        if not a.get("full"):
            assert "static_page" not in a
            assert not any(a.get(k) for k in KINDS)
    result["max_bytes"] = max(p.raw_sizes[f["seq"]] for f in frames())
    result["max_collection_instructions"] = max(check_frame(f, actors)["proof_instructions"] for f in frames())
    return result


def run(p):
    p.chunk('''SetLocalString(GetModule(),"MGLIVE_AREAS","events,events2");
        SetLocalInt(GetModule(),"MGLIVE_DELTA",TRUE);
        DeleteLocalInt(GetModule(),"MGET_DENSE_CREATED");''')
    add_population(p, 300)
    # Create actors after the clutter: their visibility cannot depend on the
    # native all-object insertion order. No actual client or player credentials.
    actor_start = p.state["seq"]
    p.chunk('''object area=GetObjectByTag("EVENTS");json ids=JsonArray();int i;
        for(i=0;i<3;i++) {
            object c=CreateObject(OBJECT_TYPE_CREATURE,"nw_bartender",Location(area,Vector(5.0+IntToFloat(i),5.0,0.0),0.0),FALSE,"DENSE_ACTOR");
            int h;for(h=5000;h<=5012;h++) SetEventScript(c,h,"");
            SetLocalInt(c,"MGET_DENSE_PC",i<2);SetLocalInt(c,"MGET_DENSE_DM",i==1);
            AssignCommand(c,ClearAllActions(TRUE));SetCommandable(FALSE,c);
            JsonArrayInsertInplace(ids,JsonString(ObjectToString(c)));
        }
        SetLocalJson(GetModule(),"MGET_DENSE_ACTORS",ids);''')
    actors = set(p.native('GetLocalJson(GetModule(),"MGET_DENSE_ACTORS")'))
    assert len(actors) == 3 and "7f000000" not in actors
    ordinary = prove_scan(p, actors, 300)
    cold_frames = [a for f in p.frames if f["seq"] > actor_start for a in f.get("areas", [])
                   if a["area"] == "events" and actors <= {o["id"] for o in a.get("cre", [])}
                   and any(o.get("look_pending") for o in a.get("cre", []) if o["id"] in actors)]
    assert cold_frames, "cold NPCs must appear with look_pending without hiding the rest"
    add_population(p, 852)
    assert p.native('JsonInt(GetLocalInt(GetModule(),"MGET_DENSE_CREATED"))') == 1152
    dense = prove_scan(p, actors, 1152)
    for flag in ("MGET_REPORT_ABORT", "MGET_PUBLISH_ABORT"):
        recovered = p.state.get("report_recovered", 0)
        p.chunk(f'SetLocalInt(GetModule(),"{flag}",TRUE);')
        p.until(lambda: p.state.get("report_recovered", 0) > recovered)
        # The receipt survived a real VM abort; a fresh scan must then converge.
        prove_scan(p, actors, 1152)
    # Clean only this phase's population. A new complete scan must show removal.
    for _ in range(12):
        p.chunk('''int i;for(i=99;i>=0;i--) {
            object o=GetObjectByTag("DENSE_FILLER",i);
            if(GetIsObjectValid(o)) DestroyObject(o);
        }''')
    assert not p.native('JsonBool(GetIsObjectValid(GetObjectByTag("DENSE_FILLER")))')
    empty = prove_scan(p, actors, 0)
    p.chunk('''int i;for(i=2;i>=0;i--) {
        object o=GetObjectByTag("DENSE_ACTOR",i);if(GetIsObjectValid(o)) DestroyObject(o);
    }''')
    return {"ordinary": ordinary, "dense": dense, "after_removal": empty,
            "max_full_reports": MAX_FULL_REPORTS, "pc_dm_surrogates": 2, "cold_frames": len(cold_frames),
            "recovered": p.state["report_recovered"], "resumed_seq": p.state["seq"]}
