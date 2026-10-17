"""New EE spawn kinds and trigger shape native readback."""
def run(p):
    rows=[]
    def report_has(ident):
        return any(o["id"]==ident for a in p.state["areas"] for key in ["waypoints","sounds","encounters","stores","triggers"] for o in a.get(key,[]))
    for kind,constant,key in [("waypoint",32,"waypoints"),("sound",32767,"sounds"),("encounter",256,"encounters"),("store",128,"stores"),("trigger",4,"triggers")]:
        result=p.command(op="spawn",kind=kind,r="bellrings" if kind=="sound" else "mget_"+kind,x=10,y=10,z=0,f=0,count=2)
        ids=result["objects"]
        assert len(ids)==2
        p.command(op="resync")
        p.until(lambda:all(report_has(oid) for oid in ids))
        positions=[o["x"] for a in p.state["areas"] for o in a.get(key,[]) if o["id"] in ids]
        native=[]
        for oid in ids:
            value=p.native('JsonArrayInsert(JsonArrayInsert(JsonArray(),JsonInt(GetObjectType(StringToObject("'+oid+'")))),JsonString(GetResRef(GetArea(StringToObject("'+oid+'")))))')
            assert value==[constant,"events"],(kind,value)
            native.append(value)
        assert len(set(positions))==2,(kind,positions)
        if kind=="encounter":
            for oid in ids:p.command(expect="disabled",op="destroy",target=oid)
            p.chunk('object o=StringToObject("'+ids[0]+'"); SetIsDestroyable(TRUE,TRUE,FALSE,o);DestroyObject(o);')
            import time
            end=time.monotonic()+20
            while time.monotonic()<end:p.poll(.5)
            remained=p.native('JsonBool(GetIsObjectValid(StringToObject("'+ids[0]+'")))')
            assert remained
            rows.append({"kind":kind,"created":result,"native":native,"removal_disabled":True,"native_destroy_still_valid_after_20s":remained})
            continue
        for oid in ids: p.command(op="destroy",target=oid)
        p.command(op="resync")
        p.until(lambda:all(not report_has(oid) for oid in ids))
        assert all(not p.native('JsonBool(GetIsObjectValid(StringToObject("'+oid+'")))') for oid in ids),(kind,ids)
        rows.append({"kind":kind,"created":result,"native":native,"removed":True})
    shapes=[]
    for args,vertices in [({"shape":"circle","radius":3},16),({"shape":"rectangle","width":6,"height":4,"r":"mget_trigger"},4)]:
        result=p.command(op="spawn_trigger",x=10,y=10,z=0,f=30,**args)
        assert result["value"]["contains_center"]
        geometry=result["value"]["geometry"]
        assert geometry.count("{")==vertices,geometry
        p.command(op="resync");p.until(lambda:report_has(result["obj"]))
        if args.get("r"):
            assert any(o["id"]==result["obj"] and o["tag"]==args["r"] for a in p.state["areas"] for o in a.get("triggers",[]))
        p.command(op="destroy",target=result["obj"])
        shapes.append(result)
    for args in [{"shape":"circle","radius":31},{"shape":"rectangle","width":3},{"shape":"circle","radius":3,"width":2},{"shape":"circle","radius":0},{"shape":"rectangle","width":30,"height":30}]:
        p.command(expect="invalid",op="spawn_trigger",x=5,y=5,z=0,f=0,**args)
    return {"kinds":rows,"shapes":shapes,"refusals":5,"limitations":"No client rendering/audio or encounter spawning behavior is claimed; native identity, area membership, shape containment, reports and destruction are proven."}
