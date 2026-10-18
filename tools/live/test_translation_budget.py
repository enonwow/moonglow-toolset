"""Native instruction-budget proof; run only through test-events.py's private helper."""


def measure(p, channel, label):
    # NWNX calls are disabled by default inside ExecuteScriptChunk. Run the
    # measurement in a compiled fixture, under the server's unchanged VM limit.
    assert channel in [1, 2, 3, 4, 6, 14]
    assert label and all(c.isascii() and (c.isalnum() or c == "_") for c in label)
    p.chunk('DeleteLocalJson(GetModule(),"MGET_XL_BUDGET");'
            f'SetLocalInt(GetModule(),"MGET_XL_CHANNEL",{channel});'
            f'SetLocalString(GetModule(),"MGET_XL_LABEL","{label}");')
    p.command(op="run_script", script="mget_xlbudget")
    row = p.native('GetLocalJson(GetModule(),"MGET_XL_BUDGET")')
    assert isinstance(row, dict) and row.get("label") == label and row.get("channel") == channel, (
        "compiled budget fixture did not produce a fresh measurement", row)
    assert row.get("captured") in [0, 1], row
    assert isinstance(row.get("instructions"), int) and isinstance(row.get("limit"), int), row
    assert 0 < row["instructions"] < row["limit"] // 2, row
    return row


def run(p):
    p.chunk('SetLocalInt(GetModule(),"MGET_XL_MODE",1);')
    p.command(op="run_script", script="mget_xl")
    p.chunk('SetLocalInt(GetModule(),"MGLIVE_XL",TRUE);')


    baseline = {c: measure(p, c, f"budget_before_{c}") for c in [1, 2, 3, 4, 6, 14]}
    # Authored fixture objects; creation is split across VM invocations. No game
    # install/user files are changed and the helper removes its scratch module.
    for _ in range(32):
        p.chunk('''object speaker=GetLocalObject(GetModule(),"MGET_XL_SPEAKER");
        int i; for(i=0;i<128;i++) {
            object o=CreateObject(OBJECT_TYPE_PLACEABLE,"mget_polish",GetLocation(speaker),FALSE,"XL_BUDGET_FILLER");
            if(!GetIsObjectValid(o)) { SetLocalInt(GetModule(),"MGET_XL_SETUP_FAILED",TRUE); break; }
        }''')
    assert not p.native('JsonBool(GetLocalInt(GetModule(),"MGET_XL_SETUP_FAILED"))')
    loaded = {c: measure(p, c, f"budget_loaded_{c}") for c in baseline}
    for c in baseline:
        assert loaded[c]["captured"] == baseline[c]["captured"] == 1, (c, loaded[c])
        assert loaded[c]["instructions"] <= baseline[c]["instructions"] + 5000, (c, baseline[c], loaded[c])
    # An overcrowded creature audience must return control to native delivery
    # before any manual deliveries/pending slot are installed, never hit TMI.
    for _ in range(9):
        p.chunk('''object speaker=GetLocalObject(GetModule(),"MGET_XL_SPEAKER");
        int i;for(i=0;i<32;i++) {
            object o=CreateObject(OBJECT_TYPE_CREATURE,"nw_bartender",GetLocation(speaker),FALSE,"XL_BUDGET_NPC");
            int h;for(h=5000;h<=5012;h++) SetEventScript(o,h,"");
            SetListening(o,TRUE);
        }''')
    crowded = measure(p, 1, "budget_crowded")
    assert crowded["captured"] == 0, crowded
    # Tell and party do not scan unrelated NPCs, even in a crowded area.
    for c in [4, 6, 14]:
        row = measure(p, c, f"budget_crowded_{c}")
        assert row["captured"] == 1 and row["instructions"] <= baseline[c]["instructions"] + 5000, row
    return {"placeables": 4096, "extra_creatures": 288, "baseline": baseline,
            "loaded": loaded, "crowded": crowded,
            "proof": "native NWNX instruction counter; no instruction-limit increase"}
