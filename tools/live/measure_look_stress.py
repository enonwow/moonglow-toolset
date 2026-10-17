"""Private large-look and VM-budget causality regression, via measure_live.py."""
import json
import os
import re
import time


def run(proof, output, scratch):
    area = os.environ.get("MGLIVE_DEMO_AREA", "demo_area")
    def chunk(code, **kw):
        return proof.command(op="run_chunk", area=area, code=code, **kw)
    def value(expr):
        return chunk('MgReturn(JsonDump('+expr+'));', **{"return": True})["value"]
    log=scratch/"logs.0/nwserverLog1.txt"
    def vm_read():
        return {key:int(number) for key,number in re.findall(r"MP_VM (\w+)=(\d+)",log.read_text(errors="replace"))}
    # ExecuteScriptChunk's synchronous descendants expose zero through this
    # NWNX counter. Use the native script route, as normal report/routine VMs do.
    proof.command(op="run_script", area=area, script="mp_vm")
    proof.until(lambda:"delayed" in vm_read())
    vm=vm_read()
    assert vm["after"]>vm["nested"]>vm["before"]>10000 and 0<vm["delayed"]<1000, vm
    chunk('ExecuteScript("mp_look",GetModule());')
    (output/"look-text-probe.json").write_text(json.dumps(value('GetLocalJson(GetModule(),"MP_LOOK_PROBE")'),indent=2))
    actor=proof.command(op="spawn",area=area,kind="creature",r="nw_bandit001",x=4,y=4,z=0,f=0)["obj"]
    proof.command(op="set_faction",area=area,target=actor,faction=2)
    for _ in range(10):
        chunk(f'object o=StringToObject("{actor}"); int i; for(i=0;i<20;i++) CreateItemOnObject("nw_it_mring001",o);')
    proof.command(op="run_script",area=area,target=actor,script="mp_stress")
    look=value('GetLocalJson(GetModule(),"MP_LOOK_STRESS")')
    assert look["raw_bytes"] >= 250_000 and look["inventory"] >= 200 and look["equipment"] >= 1, look
    assert 0 < look["look_bytes"] <= 32768 and look["look_equipment"] >= 1, look
    assert look["fixed_pass_instructions"] < 10000 and look["unknown_list_rows"] == 1000, look
    (output/"stress-progress.json").write_text(json.dumps({"look":look},indent=2))
    chunk(f'SetLocalInt(StringToObject("{actor}"),"MP_LOOK",1); SetLocalInt(GetModule(),"MGLIVE_RESYNC",1);')
    # Native full PC branch (selector only), command liveness, and routine budgets.
    stages=[]
    for live_off in [False, True, False]:
        code=f'SetLocalInt(GetModule(),"MP_LIVE_OFF",{int(live_off)});'
        proof.pub.send("PUBLISH","mglive-events.cmd",json.dumps({"v":1,"commands":[{"id":"stress-toggle-"+str(len(stages)),"op":"run_chunk","area":area,"code":code}]})); proof.pub.read()
        offset=log.stat().st_size
        start_seq=proof.state["seq"]
        end=time.monotonic()+18
        while time.monotonic()<end: proof.poll(.25)
        lines=log.read_bytes()[offset:].decode(errors="replace").splitlines()
        routines=[line for line in lines if "MP_CITY_VM " in line]
        assert routines, "The module routines must run in both stages"
        if not live_off: assert proof.state["seq"]>=start_seq+10
        stages.append({"reports_paused":live_off,"first_seq":start_seq,"last_seq":proof.state["seq"],"routine_samples":routines})
    full=[o for f in proof.frames for a in f["areas"] for o in a["cre"] if o["id"]==actor and "look" in o]
    assert full and all(len(o["look"]["Equip_ItemList"]["value"])>=1 for o in full)
    (output/"stress-progress.json").write_text(json.dumps({"look":look,"vm":vm,"stages":stages},indent=2))
    engine=scratch/"logs.0/nwengineLog.txt"
    errors=[s for s in engine.read_text(errors="replace").splitlines() if "TOO MANY INSTRUCTIONS" in s]
    assert not errors, errors
    # A known failing private child is the sole negative control, after positive gates.
    offset=engine.stat().st_size
    routine_offset=log.stat().st_size
    proof.pub.send("PUBLISH","mglive-events.cmd",json.dumps({"v":1,"commands":[{"id":"vm-negative","op":"run_script","area":area,"script":"mp_vmfail"}]}));proof.pub.read()
    seq=proof.state["seq"];proof.until(lambda:proof.state["seq"]>=seq+3)
    vm=vm_read()
    assert 0<vm["after_tmi"]<1000,vm
    def later_routines():
        return [s for s in log.read_bytes()[routine_offset:].decode(errors="replace").splitlines() if "MP_CITY_VM " in s]
    proof.until(lambda:bool(later_routines()),20)
    post_tmi=later_routines()
    assert int(re.search(r"before=(\d+)",post_tmi[0]).group(1))<1000,post_tmi[0]
    negative=engine.read_bytes()[offset:].decode(errors="replace")
    assert "mp_vmfail" in negative and "TOO MANY INSTRUCTIONS" in negative
    assert all("mp_vmfail" in s or "mglive_ps" in s for s in negative.splitlines() if "TOO MANY INSTRUCTIONS" in s),negative
    result={"connected_players":0,"surrogate":actor,"look":look,"full_pc_path_rows":len(full),"vm":vm,"stages":stages,"post_tmi_routines":post_tmi,"unexpected_vm_errors":errors,"deliberate_negative_control":negative}
    (output/"engine.json").write_text(json.dumps(result,indent=2))
    # Keep a cold crowd report and three actual uncached-look reports, not a
    # redundant copy of every second of the fixture's routine traffic.
    samples=[proof.frames[0]]
    for packet in proof.frames:
        if any(o["id"]==actor and "look" in o
               for a in packet["areas"] for o in a["cre"]):
            samples.append(packet)
            if len(samples)==4: break
    (output/"one_look_surrogate-reports.json").write_text(json.dumps(samples,separators=(",",":")))
    return result
