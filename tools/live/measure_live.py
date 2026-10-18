"""Opt-in private measurement fixture, invoked by test-events.py --phase measure.

Never patches production scripts. The two tick copies share one module: native
report work and a detailed observer. Inclusive child timings are not additive.
"""
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import time

REPO = Path(__file__).resolve().parents[2]
OUT = REPO / "target/test-output/live-measure"
# The demo module's source folder (a WSL path), from this machine's environment.
# The private demo module's main area and file name; generic defaults, the real
# names come from tools/live/local.ps1 like MGLIVE_DEMO_SOURCE.
DEMO_AREA = os.environ.get("MGLIVE_DEMO_AREA", "demo_area")
DEMO_MODULE = os.environ.get("MGLIVE_DEMO_MODULE", "demo.mod")


def demo_source():
    folder = os.environ.get("MGLIVE_DEMO_SOURCE")
    if not folder:
        raise SystemExit("Set MGLIVE_DEMO_SOURCE to the demo module's source folder")
    return Path(folder)
SCRATCH = Path.home() / "mglive-events"
PREFLIGHT = '''    if (!MgLiveLooksReady())
    {
        DelayCommand(0.1, ExecuteScript("mglive_tick", m));
        return;
    }
'''


def observations():
    label = os.environ.get("MGLIVE_MEASURE_RUN", "")
    if not label:
        return OUT
    assert re.fullmatch(r"[a-z0-9-]{1,48}", label), "Invalid measurement label"
    path = OUT / "runs" / label
    path.mkdir(parents=True, exist_ok=True)
    return path


def replace(text, before, after):
    assert text.count(before) == 1, (before, text.count(before))
    return text.replace(before, after)


def instrument(inc, tick, row_timers=True):
    names = ["cre", "plc", "doors", "items", "cre_build", "plc_build", "doors_build", "items_build", "effects", "relations", "text", "common", "look", "snapshot", "look_bytes", "objects", "npc_builds", "npc_hits", "npc_rebuild"]
    if not row_timers:
        names = [n for n in names if not n.endswith("_build")]
    inc = "\n".join("int MP_" + n + " = 0;" for n in names) + "\n" + inc
    inc = inc.replace("j = MgLiveTextField(", "j = MPTextField(")
    inc = replace(inc, "int MgLiveOriginalAppearance(", '''json MPTextField(json j, string key, string value)
{
    int start = GetMicrosecondCounter();
    json result = MgLiveTextField(j, key, value);
    MP_text += GetMicrosecondCounter() - start;
    return result;
}

int MgLiveOriginalAppearance(''')
    inc = replace(inc, "json MgLiveObject(object o, int bFull)\n{", "json MgLiveObject(object o, int bFull)\n{\n    int mpCommon = GetMicrosecondCounter();")
    inc = replace(inc, "    if (t == OBJECT_TYPE_CREATURE)\n    {", "    MP_common += GetMicrosecondCounter() - mpCommon;\n    if (t == OBJECT_TYPE_CREATURE)\n    {")
    inc = replace(inc, "    json look = ObjectToJson(pc);", "    int mpSnap = GetMicrosecondCounter();\n    json look = ObjectToJson(pc);\n    MP_snapshot += GetMicrosecondCounter() - mpSnap;")
    inc = replace(inc, "    int bytes = GetStringLength(JsonDump(look));", "    int bytes = GetStringLength(JsonDump(look));\n    MP_look_bytes += bytes;")
    inc = replace(inc, "        json vfx = JsonArray();", "        int mpEffects = GetMicrosecondCounter();\n        json vfx = JsonArray();")
    inc = replace(inc, "        j = JsonObjectSet(j, \"pc\",", "        MP_effects += GetMicrosecondCounter() - mpEffects;\n        j = JsonObjectSet(j, \"pc\",")
    inc = replace(inc, "        json hostile = JsonArray();", "        int mpRelations = GetMicrosecondCounter();\n        json hostile = JsonArray();")
    inc = replace(inc, "        if (!GetIsDM(o) && (!GetIsPC(o) || bFull))", "        MP_relations += GetMicrosecondCounter() - mpRelations;\n        if (!GetIsDM(o) && (!GetIsPC(o) || bFull))")
    inc = replace(inc, "            json look;", "            int mpLook = GetMicrosecondCounter();\n            json look;")
    # Extra NPCs exercise the uncached PC snapshot path; every ordinary NPC
    # keeps its production compact equipment cache. Never claim these are PCs.
    inc = replace(inc, "            if (GetIsPC(o)) look = MgLiveLook(o);", '            if (GetIsPC(o) || (bFull && GetLocalInt(o, "MP_LOOK"))) look = MgLiveLook(o);')
    inc = replace(inc, 'JsonInt(GetIsPC(o) ? HashString(JsonDump(look))', 'JsonInt((GetIsPC(o) || (bFull && GetLocalInt(o, "MP_LOOK"))) ? HashString(JsonDump(look))')
    inc = replace(inc, '                j = JsonObjectSet(j, "lookHash", JsonInt((GetIsPC(o) || (bFull && GetLocalInt(o, "MP_LOOK"))) ? HashString(JsonDump(look)) : GetLocalInt(o, "MGLIVE_NPC_LOOK_HASH")));\n            }', '                j = JsonObjectSet(j, "lookHash", JsonInt((GetIsPC(o) || (bFull && GetLocalInt(o, "MP_LOOK"))) ? HashString(JsonDump(look)) : GetLocalInt(o, "MGLIVE_NPC_LOOK_HASH")));\n            }\n            MP_look += GetMicrosecondCounter() - mpLook;')
    inc = replace(inc, "        json look = MgLiveLook(o);", "        int mpRebuild = GetMicrosecondCounter();\n        MP_npc_builds++;\n        json look = MgLiveLook(o);")
    refresh = '        SetLocalInt(o, "MGLIVE_LOOK_REFRESH", tick + 30);'
    if refresh not in inc:
        refresh = '        SetLocalInt(o, "MGLIVE_LOOK_REFRESH", MgLiveLookRefreshAt(o, tick));'
    inc = replace(inc, refresh, refresh+'\n        MP_npc_rebuild += GetMicrosecondCounter() - mpRebuild;')
    inc = replace(inc, '    return GetLocalJson(o, "MGLIVE_NPC_LOOK");', '    else MP_npc_hits++;\n    return GetLocalJson(o, "MGLIVE_NPC_LOOK");')
    for kind, typ, full in [("cre", "CREATURE", "bFull"), ("plc", "PLACEABLE", "TRUE"), ("doors", "DOOR", "TRUE"), ("items", "ITEM", "TRUE")]:
        old = f"            {kind} = JsonArrayInsert({kind}, MgLiveObject(o, {full}));"
        if old not in inc:
            old = f'            {kind} += JsonDump(MgLiveObject(o, {full})) + ",";'
        if old not in inc:
            old = f"            JsonArrayInsertInplace({kind}, MgLiveObject(o, {full}));"
        inc = replace(inc, old, "        {\n            int mpRow = GetMicrosecondCounter();\n" + old + f"\n            MP_{kind} += GetMicrosecondCounter() - mpRow;\n        }}")
    inc = replace(inc, "        o = GetNextObjectInArea(area);", "        MP_objects++;\n        o = GetNextObjectInArea(area);")
    if row_timers:
        start, end = inc.index("json MgLiveObject("), inc.index("json MgLiveArea(")
        row_function = inc[start:end]
        row_function = replace(row_function, "    return j;", "    int mpBuilt = GetMicrosecondCounter() - mpCommon;\n" + "\n".join(f"    if(t == OBJECT_TYPE_{typ}) MP_{kind}_build += mpBuilt;" for kind, typ in [("cre", "CREATURE"), ("plc", "PLACEABLE"), ("doors", "DOOR"), ("items", "ITEM")]) + "\n    return j;")
        inc = inc[:start] + row_function + inc[end:]
    tick = replace(tick, '#include "mglive_inc"', '#include "mp_detail"')
    tick = replace(tick, PREFLIGHT, '')
    tick = replace(tick, '    DelayCommand(MGLIVE_PERIOD, ExecuteScript("mglive_tick", m));', '')
    tick = replace(tick, "    json areas = MgLiveAreas(full || delta);", "    int mpAreas = GetMicrosecondCounter();\n    json areas = MgLiveAreas(full || delta);\n    int areasUs = GetMicrosecondCounter() - mpAreas;\n    int mpHash = GetMicrosecondCounter();")
    tick = replace(tick, '    json ack = GetLocalJson(m, "MGLIVE_ACK");', '    int hashUs = GetMicrosecondCounter() - mpHash;\n    int mpBody = GetMicrosecondCounter();\n    json ack = GetLocalJson(m, "MGLIVE_ACK");')
    events_read = '    json events = GetLocalJson(m, "MGLIVE_EVENTS");'
    if events_read not in tick:
        events_read = '    json events = MgLivePendingEvents();'
    tick = replace(tick, events_read, '    int bodyUs = GetMicrosecondCounter() - mpBody;\n    int mpEvents = GetMicrosecondCounter();\n' + events_read)
    tick = replace(tick, '    string data = JsonDump(body);', '    int eventsUs = GetMicrosecondCounter() - mpEvents;\n    int mpDump = GetMicrosecondCounter();\n    string data = JsonDump(body);\n    int dumpUs = GetMicrosecondCounter() - mpDump;\n    int mpRedis = GetMicrosecondCounter();')
    tick = replace(tick, '    // Pub/sub has no durable delivery:', '    int redisUs = GetMicrosecondCounter() - mpRedis;\n    int mpClear = GetMicrosecondCounter();\n    // Pub/sub has no durable delivery:')
    tick = replace(tick, '    if (seq == 1 || seq % 60 == 0)', '    int clearUs = GetMicrosecondCounter() - mpClear;\n    int totalUs = GetMicrosecondCounter() - start;\n    if (seq == 1 || seq % 60 == 0)')
    fields = {"seq": "seq", "full": "full", "delta": "delta", "us": "totalUs", "bytes": "GetStringLength(data)", "areas": "areasUs", "hash": "hashUs", "body": "bodyUs", "events": "eventsUs", "dump": "dumpUs", "redis": "redisUs", "clear": "clearUs"}
    fields.update({n: "MP_" + n for n in names})
    fields["event_count"] = "JsonGetLength(events)"
    fields["heard"] = "heard"
    log = '    WriteTimestampedLogEntry("MP_TICK"' + ''.join(' + " '+k+'=" + IntToString('+v+')' for k, v in fields.items()) + ');\n'
    tick = tick.rstrip()[:-1] + log + "}\n"
    return inc, tick


def prepare():
    # Import utilities without calling the runner a second time.
    spec = importlib.util.spec_from_file_location("event_helpers", REPO / "tools/live/test-events.py")
    helpers = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helpers)
    OUT.mkdir(exist_ok=True)
    marker = OUT / ".owner"
    if marker.exists():
        assert marker.read_text().strip() == "live-measure-v1"
    else:
        marker.write_text("live-measure-v1\n")
    module = OUT / "module"
    module.mkdir(exist_ok=True)
    source = demo_source() / DEMO_MODULE
    inputs = {"module_windows": helpers.win(source), "module_sha256": helpers.sha(source), "haks": [], "production_scripts": {}, "helper_sha256": helpers.sha(Path(__file__)), "full_scale": os.environ.get("MGLIVE_FULL_SCALE") == "1"}
    # Reuse the already extracted demo inputs read-only; one necessary private
    # server copy of each HAK, never an iteration-per-run asset tree.
    hakdir = SCRATCH / "hak"
    hakdir.mkdir(exist_ok=True)
    for name in ["test_snow.hak", "grass_textures.hak", "npc_sounds.hak"]:
        p = REPO / "target/test-output/live-demo/user/hak" / name
        dest = hakdir / p.name
        if dest.exists():
            assert helpers.sha(dest) == helpers.sha(p)
        else:
            shutil.copyfile(p, dest)
        inputs["haks"].append({"source": str(p), "path": str(dest), "sha256": helpers.sha(p), "bytes": p.stat().st_size})
    assert len(inputs["haks"]) == 3
    subprocess.run(["python3", str(REPO / "tools/live/wsl/extract-includes.py"), str(helpers.PLUGINS / "NWScript.zip"), str(module)], check=True, stdout=subprocess.DEVNULL)
    # Reuse one owned fixture, without compiling retired scripts left by a
    # previous measurement. Never delete resources from the source/demo MOD.
    for stem in ["mglive_colour", "mglive_material", "mglive_shader"]:
        for suffix in ["nss", "ncs", "ndb"]:
            (module / (stem + "." + suffix)).unlink(missing_ok=True)
    for p in (REPO / "tools/live/nwn").glob("*.nss"):
        shutil.copyfile(p, module / p.name)
        revision = os.environ.get("MGLIVE_MEASURE_INC_REV", "")
        if revision and p.name == "mglive_inc.nss":
            assert re.fullmatch(r"[0-9a-f]{40}", revision), "Use an immutable baseline commit"
            (module / p.name).write_bytes(subprocess.check_output(["git", "show", revision + ":tools/live/nwn/" + p.name], cwd=REPO))
            inputs["inc_baseline_commit"] = revision
        inputs["production_scripts"][p.name] = helpers.sha(module / p.name)
    inc = (module / "mglive_inc.nss").read_text()
    tick = (module / "mglive_tick.nss").read_text()
    detail_source = (REPO / "tools/live/fixtures/measure-report.nss").read_text() if (module / "mglive_report.nss").exists() else tick
    detail, detail_tick = instrument(inc, detail_source, os.environ.get("MGLIVE_MEASURE_ROW_TIMERS", "1") != "0")
    (module / "mp_detail.nss").write_text(detail)
    (module / "mp_tick.nss").write_text(detail_tick)
    if os.environ.get("MGLIVE_LOOK_STRESS") == "1" or os.environ.get("MGLIVE_FULL_SCALE") == "1":
        # Exercise the actual uncached PC look branch on an explicit NPC surrogate.
        # Native GetIsPC remains false; no connected-client claim is fabricated.
        native_inc = replace(inc, '            if (GetIsPC(o)) look = MgLiveLook(o);', '            if (GetIsPC(o) || GetLocalInt(o,"MP_LOOK")) look = MgLiveLook(o);')
        native_inc = replace(native_inc, 'JsonInt(GetIsPC(o) ? HashString(JsonDump(look))', 'JsonInt((GetIsPC(o) || GetLocalInt(o,"MP_LOOK")) ? HashString(JsonDump(look))')
        (module / "mglive_inc.nss").write_text(native_inc)
    staged = (module / "mglive_report.nss").exists()
    if staged:
        # Native timings use the real bounded pipeline, not the sequential
        # component observer. Let Begin own the next-tick schedule.
        native = tick
        report = module / "mglive_report.nss"
        report.write_text(report.read_text().replace('if(seq==1 || seq%60==0)', 'if(TRUE)'))
    else:
        native = replace(tick, '    DelayCommand(MGLIVE_PERIOD, ExecuteScript("mglive_tick", m));', '')
        native = replace(native, PREFLIGHT, '')
        native = replace(native, '    if (seq == 1 || seq % 60 == 0)', '    if (TRUE)')
    (module / "mp_native.nss").write_text(native)
    measured_tick = '''#include "mglive_inc"
void main() {
object m=GetModule(); if(!GetLocalInt(m,"MGLIVE_STARTED")) return;
if(!MgLiveLooksReady()) { DelayCommand(0.1,ExecuteScript("mglive_tick",m)); return; }
if(GetLocalInt(m,"MP_LIVE_OFF")) { DelayCommand(1.0,ExecuteScript("mglive_tick",m)); return; }
if(GetLocalInt(m,"MP_DETAIL")) {
DelayCommand(1.0,ExecuteScript("mglive_tick",m));
if(!GetLocalInt(m,"MGLIVE_REPORT_BUSY")) ExecuteScript("mp_tick",m);
}
else { NATIVE_SCHEDULE ExecuteScript("mp_native",m); }
}'''.replace("NATIVE_SCHEDULE", "" if staged else 'DelayCommand(1.0,ExecuteScript("mglive_tick",m));')
    inputs["detail_observer"] = "sequential component reference; native stage uses production scheduling"
    # A cold-start proof uses the actual production tick, not the observer wrapper.
    (module / "mglive_tick.nss").write_text(tick if os.environ.get("MGLIVE_MEASURE_COLD") == "1" else measured_tick)
    (module / "mp_load.nss").write_text('''#include "nwnx_util"
void main() {
ExecuteScript("city_boot",OBJECT_SELF);
WriteTimestampedLogEntry("MP_LOAD instructions="+IntToString(NWNX_Util_GetInstructionsExecuted())+" limit="+IntToString(NWNX_Util_GetInstructionLimit()));
ExecuteScript("mglive_load",OBJECT_SELF);
ExecuteScript("mp_frame",OBJECT_SELF);
}''')
    # Scheduling-gap observer, not a rendered-client FPS measurement.
    (module / "mp_frame.nss").write_text('''#include "nwnx_util"
void main() {
object m=GetModule(); struct NWNX_Util_HighResTimestamp stamp=NWNX_Util_GetHighResTimeStamp();
int gap=(stamp.seconds-GetLocalInt(m,"MP_FRAME_SECONDS"))*1000000+stamp.microseconds-GetLocalInt(m,"MP_FRAME_MICROS");
SetLocalInt(m,"MP_FRAME_SECONDS",stamp.seconds); SetLocalInt(m,"MP_FRAME_MICROS",stamp.microseconds);
int n=GetLocalInt(m,"MP_FRAME_N")+1; SetLocalInt(m,"MP_FRAME_N",n);
if(gap>GetLocalInt(m,"MP_FRAME_MAX")&&gap<1000000) SetLocalInt(m,"MP_FRAME_MAX",gap);
if(n>=20) { WriteTimestampedLogEntry("MP_FRAME max_us="+IntToString(GetLocalInt(m,"MP_FRAME_MAX")));
SetLocalInt(m,"MP_FRAME_N",0); SetLocalInt(m,"MP_FRAME_MAX",0); }
DelayCommand(0.05,ExecuteScript("mp_frame",m));
}''')
    city = (demo_source() / "src/city_beat.nss").read_text()
    city = replace(city, "void main()", "void MP_original()")
    city += '''\nvoid main() {
if(GetLocalInt(GetModule(),"MP_ROUTINES_OFF")) return;
int start=GetMicrosecondCounter(); MP_original();
WriteTimestampedLogEntry("MP_CITY us="+IntToString(GetMicrosecondCounter()-start)+" count="+IntToString(GetLocalInt(GetModule(),"CITY_COUNT")));
}\n'''
    (module / "city_beat.nss").write_text(city)
    if os.environ.get("MGLIVE_LOOK_STRESS") == "1":
        city_tick = (demo_source()/"src/city_tick.nss").read_text().replace("void main()", "void MP_original()")
        city_tick += '''\nvoid main(){int before=NWNX_Util_GetInstructionsExecuted(); MP_original(); WriteTimestampedLogEntry("MP_CITY_VM tag="+GetTag(OBJECT_SELF)+" before="+IntToString(before)+" after="+IntToString(NWNX_Util_GetInstructionsExecuted()));}\n'''
        (module/"city_tick.nss").write_text('#include "nwnx_util"\n'+city_tick)
        for source, target in [("look-stress","stress"),("vm","vm"),("vm-child","vmchild"),("vm-fail","vmfail")]:
            shutil.copyfile(REPO/f"tools/live/measure-{source}.nss",module/f"mp_{target}.nss")
    (module / "mp_burst.nss").write_text('''#include "mglive_evt"
void main() {
object m=GetModule(); int n=GetLocalInt(m,"MP_BURST_N"); int start=GetMicrosecondCounter(); int i;
for(i=0;i<n;i++) { json e=MgLiveEvent("chat"); e=JsonObjectSet(e,"text",JsonString("measurement event")); MgLiveQueueEvent(e); }
WriteTimestampedLogEntry("MP_BURST count="+IntToString(n)+" us="+IntToString(GetMicrosecondCounter()-start));
}''')
    shutil.copyfile(REPO / "tools/live/measure-look-identity.nss", module / "mp_identity.nss")
    contract = (REPO / "tools/live/measure-events.nss").read_text()
    if "json MgLivePendingEvents()" in (module / "mglive_evt.nss").read_text():
        contract = contract.replace('GetLocalJson(m, "MGLIVE_EVENTS")', 'MgLivePendingEvents()')
    (module / "mp_events.nss").write_text(contract)
    shutil.copyfile(REPO / "tools/live/measure-report-reference.nss", module / "mp_reference.nss")
    shutil.copyfile(REPO / "tools/live/measure-report.nss", module / "mp_report.nss")
    shutil.copyfile(REPO / "tools/live/measure-collections.nss", module / "mp_collect.nss")
    if os.environ.get("MGLIVE_LOOK_TEXT_PROBE") == "1" or os.environ.get("MGLIVE_LOOK_STRESS") == "1":
        shutil.copyfile(REPO / "tools/live/measure-look.nss", module / "mp_look.nss")
    if os.environ.get("MGLIVE_FULL_SCALE") == "1":
        shutil.copyfile(REPO / "tools/live/measure-look-stress.nss", module / "mp_stress.nss")
        shutil.copyfile(REPO / "tools/live/measure-full-scale.nss", module / "mp_scale.nss")
        # Measure each real entry's cumulative VM use including nested hooks.
        # Production source remains untouched; the wrapper adds a fixed small cost.
        for path in list(module.glob("mglive_*.nss")) + [module / "mp_native.nss", module / "mp_scale.nss", module / "mp_stress.nss"]:
            source = path.read_text()
            if not re.search(r"(?m)^void[ \t]+main[ \t]*\([ \t]*\)", source):
                continue
            source = re.sub(r"(?m)^void[ \t]+main[ \t]*\([ \t]*\)", "void MpObservedMain()", source, count=1)
            source = '#include "nwnx_util"\n' + source
            source += '\nvoid main(){ int before=NWNX_Util_GetInstructionsExecuted(); MpObservedMain(); int after=NWNX_Util_GetInstructionsExecuted(); WriteTimestampedLogEntry("MP_HEADROOM script='+path.stem+' before="+IntToString(before)+" after="+IntToString(after)+" limit="+IntToString(NWNX_Util_GetInstructionLimit())); }\n'
            path.write_text(source)
    inputs["fixture_scripts"] = {p.name: helpers.sha(p) for p in module.glob("*.nss")}
    helpers.dump(OUT / "inputs.json", inputs)
    if observations() != OUT:
        helpers.dump(observations() / "inputs.json", inputs)
    reuse = os.environ.get("MGLIVE_MEASURE_REUSE", "")
    if reuse:
        assert re.fullmatch(r"[a-z0-9-]{1,48}", reuse)
        previous = OUT / "runs" / reuse
        recorded = json.loads((previous / "inputs.json").read_text())
        for key in ["module_sha256", "haks", "production_scripts", "fixture_scripts"]:
            assert inputs[key] == recorded[key], f"Stale reusable fixture: {key}"
        receipt = json.loads((previous / "measure.json").read_text())
        assert receipt["passed"] and helpers.sha(OUT / "events.mod") == receipt["module_sha256"]
        return OUT / "events.mod"
    env = dict(os.environ, MGLIVE_PREPARE_MEASURE="1", MGLIVE_RESOURCE_ROOT=helpers.GAME)
    env["WSLENV"] = env.get("WSLENV", "") + ":MGLIVE_PREPARE_MEASURE:MGLIVE_RESOURCE_ROOT"
    subprocess.run([str(helpers.PWSH), "-NoProfile", "-File", helpers.win(REPO / "tools/live/check.ps1"), "-Phase", "5", "-Step", "measure-prepare", "-Package", "mg-live", "-Filter", "prepare_private_measure_fixture", "-Jobs", "1", "-NoUiDebug", "-UiUnoptimized"], env=env, check=True)
    return OUT / "events.mod"


def stats(values):
    values = sorted(values)
    if not values:
        return {"n": 0}
    return {"n": len(values), "median": statistics.median(values), "p95": values[min(len(values)-1, int(len(values)*.95))], "min": values[0], "max": values[-1], "mean": statistics.mean(values)}


def cpu(pid):
    fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
    return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")


def run(proof, pid):
    if os.environ.get("MGLIVE_FULL_SCALE") == "1":
        from measure_full_scale import run as full_scale
        return full_scale(proof, observations(), SCRATCH)
    if os.environ.get("MGLIVE_LOOK_STRESS") == "1":
        from measure_look_stress import run as stress
        return stress(proof, observations(), SCRATCH)
    if os.environ.get("MGLIVE_MEASURE_COLD") == "1":
        return cold_start(proof)
    # Commands affect only this owned module. No client is fabricated.
    def chunk(code):
        return proof.command(op="run_chunk", area=DEMO_AREA, code=code)
    seconds = int(os.environ.get("MGLIVE_MEASURE_SECONDS", "60"))
    assert 15 <= seconds <= 300
    summary = {"schema": "live-measure-v1", "seconds_per_stage": seconds, "connected_players": 0, "stages": []}
    output = observations()
    if os.environ.get("MGLIVE_LOOK_TEXT_PROBE") == "1":
        result = proof.command(op="run_chunk", area=DEMO_AREA,
            code='ExecuteScript("mp_look",GetModule()); MgReturn(JsonDump(GetLocalJson(GetModule(),"MP_LOOK_PROBE")));',
            **{"return": True})
        (output / "look-text-probe.json").write_text(json.dumps(result["value"], indent=2))
    log = SCRATCH / "logs.0/nwserverLog1.txt"
    stages = [
        ("native", 0, 0, 0, 0, 0),
        ("detail", 1, 0, 0, 0, 0),
        ("routines_off", 0, 0, 1, 0, 0),
        ("both_off", 0, 0, 1, 1, 0),
        ("live_off", 0, 0, 0, 1, 0),
        ("delta", 1, 1, 0, 0, 0),
        ("one_look_surrogate", 1, 0, 0, 0, 1),
        ("one_look_delta", 1, 1, 0, 0, 1),
        ("three_look_surrogates", 1, 0, 0, 0, 3),
        ("effects_16", 1, 0, 0, 0, 0),
    ]
    selected = os.environ.get("MGLIVE_MEASURE_STAGES", "")
    if selected:
        requested = selected.split(",")
        assert set(requested) <= {s[0] for s in stages}
        stages = [s for s in stages if s[0] in requested]
        summary["selected_stages"] = requested
    for name, detail, delta, routines_off, live_off, looks in stages:
        # A live-off stage still accepts pubsub commands but cannot ack them;
        # restore it directly before the next command waits for a result.
        if summary["stages"] and summary["stages"][-1]["live_off"]:
            proof.pub.send("PUBLISH", "mglive-events.cmd", json.dumps({"v": 1, "script_version": 2, "boot": proof.state["boot"], "prefix": "mglive-events", "commands": [{"id": "resume-"+name, "area": DEMO_AREA, "op": "run_chunk", "code": 'SetLocalInt(GetModule(),"MP_LIVE_OFF",0);'}]}))
            proof.pub.read()
        code = 'object m=GetModule();' + ''.join(f'SetLocalInt(m,"{key}",{value});' for key, value in [("MP_DETAIL", detail), ("MGLIVE_DELTA", delta), ("MGLIVE_RESYNC", 1), ("MP_ROUTINES_OFF", routines_off), ("MP_LIVE_OFF", 0)])
        code += 'object a=GetObjectByTag("'+DEMO_AREA+'"); if(!GetIsObjectValid(a)) a=GetFirstArea();'
        # Find by resref: this module's area tag has a different spelling.
        selected_looks = 0 if os.environ.get("MGLIVE_WALKING_LOOKS") == "1" else looks
        code += 'a=GetFirstArea(); while(GetIsObjectValid(a)&&GetResRef(a)!="'+DEMO_AREA+'") a=GetNextArea(); int n=0; object o=GetFirstObjectInArea(a); while(GetIsObjectValid(o)){if(GetObjectType(o)==OBJECT_TYPE_CREATURE){SetLocalInt(o,"MP_LOOK",n<'+str(selected_looks)+');n++;} o=GetNextObjectInArea(a);}'
        if os.environ.get("MGLIVE_WALKING_LOOKS") == "1" and looks:
            # Three known walking queue actors from the immutable city fixture.
            # Exercise uncached PC serialization, without pretending they are PCs.
            code += ''.join('SetLocalInt(GetObjectByTag("'+tag+'"),"MP_LOOK",1);' for tag in ["C15_ACTOR", "C15_EXTRA1", "C15_EXTRA2"])
        chunk(code)
        if name == "effects_16":
            chunk('object a=GetFirstArea(); while(GetIsObjectValid(a)&&GetResRef(a)!="'+DEMO_AREA+'") a=GetNextArea(); object o=GetFirstObjectInArea(a); while(GetIsObjectValid(o)){ if(GetObjectType(o)==OBJECT_TYPE_CREATURE) {int i;for(i=0;i<16;i++) ApplyEffectToObject(DURATION_TYPE_PERMANENT,EffectVisualEffect(411),o);} o=GetNextObjectInArea(a);}')
        end = time.monotonic() + 3
        while time.monotonic() < end:
            proof.poll(.25)
        if live_off:
            proof.pub.send("PUBLISH", "mglive-events.cmd", json.dumps({"v": 1, "script_version": 2, "boot": proof.state["boot"], "prefix": "mglive-events", "commands": [{"id": "pause-"+name, "area": DEMO_AREA, "op": "run_chunk", "code": 'SetLocalInt(GetModule(),"MP_LIVE_OFF",1);'}]}))
            proof.pub.read()
        offset = log.stat().st_size
        first = len(proof.frames)
        begin = time.monotonic()
        c0 = cpu(pid)
        while time.monotonic() - begin < seconds:
            proof.poll(.25)
        elapsed = time.monotonic() - begin
        cpu_percent = 100 * (cpu(pid) - c0) / elapsed
        with log.open("rb") as f:
            f.seek(offset)
            lines = f.read().decode("utf-8", errors="replace").splitlines()
        samples = []
        city, gaps = [], []
        for line in lines:
            if "MP_TICK " in line or (not detail and "MGLIVE: seq=" in line):
                row = {k: int(v) for k, v in re.findall(r"(\w+)=(-?\d+)", line)}
                if "tick_us" in row: row["us"] = row.pop("tick_us")
                samples.append(row)
            if "MP_CITY " in line: city.append(int(re.search(r"us=(\d+)", line)[1]))
            if "MP_FRAME " in line: gaps.append(int(re.search(r"max_us=(\d+)", line)[1]))
        frames = proof.frames[first:]
        row = {"name": name, "live_off": bool(live_off), "detail": bool(detail), "delta": bool(delta), "routines_off": bool(routines_off), "surrogate_looks": looks, "elapsed": elapsed, "cpu_one_core_percent": cpu_percent, "samples": samples, "city_us": stats(city), "frame_batch_max_us": stats(gaps), "reports": len(frames), "bytes": stats([proof.raw_sizes[f["seq"]] for f in frames]), "groups": {}}
        from measure_look_hashes import summarize
        row["look_traffic"] = summarize(frames)
        row["reported_rows"] = [{"seq": f["seq"], **{k: sum(len(a.get(k, [])) for a in f["areas"]) for k in ["cre", "plc", "doors", "items"]}} for f in frames]
        for full in [0, 1]:
            chosen = [s for s in samples if s["full"] == full]
            row["groups"][str(full)] = {k: stats([s[k] for s in chosen if k in s]) for k in sorted({k for s in chosen for k in s})}
        # Minimal complete sequence for Rust replay; no unrelated player data.
        (output / (name + "-reports.json")).write_text(json.dumps(frames, separators=(",", ":")))
        summary["stages"].append(row)
        (output / "engine.json").write_text(json.dumps(summary, indent=2))
        print(f"measure {name}: {len(samples)} ticks, {cpu_percent:.2f}% of one CPU", flush=True)
    if os.environ.get("MGLIVE_LOOK_IDENTITY_CHECK") == "1":
        result = proof.command(op="run_chunk", area=DEMO_AREA, code='ExecuteScript("mp_identity",GetObjectByTag("C15_ACTOR")); MgReturn(JsonDump(GetLocalJson(GetModule(),"MP_LOOK_IDENTITY")));', **{"return": True})["value"]
        assert result["first"] == result["turned"] == result["restored"] != result["changed"], result
        assert result["metadata"] == "UTC " and result["equipment"] > 0, result
        summary["look_identity"] = result
        assert all(s["look_traffic"]["hash_invalidations_after_initial"] == 0 for s in summary["stages"])
        assert any(s["look_traffic"]["moving_creatures"] >= 3 for s in summary["stages"])
    offset = log.stat().st_size
    bursts = int(os.environ.get("MGLIVE_MEASURE_BURSTS", "0" if selected else "1"))
    assert 0 <= bursts <= 20
    for n in [10, 256] * bursts:
        chunk(f'SetLocalInt(GetModule(),"MP_BURST_N",{n}); SetLocalInt(GetModule(),"MGLIVE_RESYNC",1); ExecuteScript("mp_burst",GetModule());')
    if bursts:
        # PUBLISH delivers the result before the tick finishes buffer clearing.
        # Wait for its final timing record rather than racing the log read.
        expected = f'MP_TICK seq={proof.state["seq"]} '
        end = time.monotonic() + 2
        while expected not in log.read_text(errors="replace") and time.monotonic() < end:
            time.sleep(.01)
        assert expected in log.read_text(errors="replace"), "Last tick timing missing"
    with log.open("rb") as f:
        f.seek(offset)
        summary["event_burst_log"] = [line for line in f.read().decode(errors="replace").splitlines() if "MP_" in line]
    offset = log.stat().st_size
    chunk('ExecuteScript("mp_events",GetModule());')
    with log.open("rb") as f:
        f.seek(offset)
        summary["event_contract"] = [line for line in f.read().decode(errors="replace").splitlines() if "MP_EVENT_CHECK" in line]
    assert len(summary["event_contract"]) == 1 and "pass=1" in summary["event_contract"][0], summary["event_contract"]
    offset = log.stat().st_size
    chunk('ExecuteScript("mp_report",GetModule());')
    with log.open("rb") as f:
        f.seek(offset)
        report_lines = f.read().decode(errors="replace").splitlines()
        summary["report_contract"] = [line for line in report_lines if "MP_REPORT_CHECK" in line]

    assert len(summary["report_contract"]) == 1 and "pass=1" in summary["report_contract"][0], summary["report_contract"]
    offset = log.stat().st_size
    for _ in range(12):
        chunk('ExecuteScript("mp_collect",GetModule());')
    with log.open("rb") as f:
        summary["collection_pairs"] = [dict((k, int(v)) for k, v in re.findall(r"(\w+)=(\d+)", line)) for line in f.read().decode(errors="replace").splitlines() if "MP_COLLECTION " in line]
    assert len(summary["collection_pairs"]) == 12 and all(row["pass"] == 1 for row in summary["collection_pairs"])
    if bursts:
        # No live subscriber: two ticks must retain the same events for reconnect.
        proof.sub.close()
        offset = log.stat().st_size
        proof.pub.send("PUBLISH", "mglive-events.cmd", json.dumps({"v": 1, "script_version": 2, "boot": proof.state["boot"], "prefix": "mglive-events", "commands": [{
            "id": "measure-no-listener", "area": DEMO_AREA, "op": "run_chunk",
            "code": 'SetLocalInt(GetModule(),"MP_BURST_N",10); ExecuteScript("mp_burst",GetModule());'}]}))
        assert proof.pub.read() > 0
        time.sleep(3)
        with log.open("rb") as f:
            lines = f.read().decode(errors="replace").splitlines()
        retained = [{k: int(v) for k, v in re.findall(r"(\w+)=(-?\d+)", line)} for line in lines if "MP_TICK " in line]
        assert len([s for s in retained if s["heard"] == 0 and s["event_count"] >= 10]) >= 2, retained
        proof.sub = type(proof.pub)()
        proof.sub.send("SUBSCRIBE", "mglive-events.state", "mglive-events.log")
        proof.sub.read()
        proof.sub.read()
        proof.until(lambda: any(r["id"] == "measure-no-listener" for r in proof.state["results"]))
        events = proof.state["events"]
        # The living-city fixture can speak while disconnected. Identify our
        # ten events without rejecting independently generated NPC events.
        measured = [e for e in events if e.get("text") == "measurement event"]
        assert len(measured) == 10, [(e.get("seq"), e.get("kind")) for e in events]
        assert [e["seq"] for e in events] == sorted({e["seq"] for e in events})
        summary["zero_subscriber"] = {"ticks": retained, "delivered": events}
        seq = proof.state["seq"]
        proof.until(lambda: proof.state["seq"] > seq)
        acknowledged = {e["seq"] for e in events}
        assert not any(e["seq"] in acknowledged for e in proof.state["events"]), "Acknowledged events redelivered"
    engine_log = SCRATCH / "logs.0/nwengineLog.txt"
    summary["vm_instruction_errors"] = [line for line in engine_log.read_text(errors="replace").splitlines() if "TOO MANY INSTRUCTIONS" in line] if engine_log.exists() else []
    summary["load_instruction_budget"] = [line for line in log.read_text(errors="replace").splitlines() if "MP_LOAD " in line]
    (output / "engine.json").write_text(json.dumps(summary, indent=2))
    assert all(not any(o.get("pc") for a in f["areas"] for o in a["cre"]) for f in proof.frames), "Unexpected connected player; label/run must be updated"
    return summary


def cold_start(proof):
    """Production cold start and a synchronized expiration, with no VM limit override."""
    first = proof.frames[0]
    assert first["seq"] == 1, "Cold start must publish seq 1, never recover at seq 2 after a TMI"
    creatures = [o for a in first["areas"] for o in a["cre"]]
    assert len(creatures) >= 37
    assert all(o.get("look", {}).get("__data_type") == "UTC " for o in creatures if not o.get("dm"))
    # Force every look to expire at once, as can happen after a resync/module load.
    forced = proof.command(op="run_chunk", area=DEMO_AREA, code='int n=0; object o=GetFirstObjectInArea(OBJECT_SELF); while(GetIsObjectValid(o)){if(GetObjectType(o)==OBJECT_TYPE_CREATURE){DeleteLocalInt(o,"MGLIVE_LOOK_REFRESH"); n++;} o=GetNextObjectInArea(OBJECT_SELF);} SetLocalInt(GetModule(),"MGLIVE_RESYNC",1); MgReturn(JsonDump(JsonInt(n)));', **{"return": True})
    assert forced["value"] == len(creatures), forced
    refreshed = [o for a in proof.state["areas"] for o in a["cre"]]
    assert len(refreshed) == len(creatures) and all("look" in o for o in refreshed)
    proof.until(lambda: proof.state["seq"] >= 35, timeout=90)
    frames = proof.frames
    assert [f["seq"] for f in frames] == list(range(1, frames[-1]["seq"] + 1))
    log = (SCRATCH / "logs.0/nwserverLog1.txt").read_text(errors="replace")
    engine = (SCRATCH / "logs.0/nwengineLog.txt").read_text(errors="replace")
    errors = [s for s in engine.splitlines() if "TOO MANY INSTRUCTIONS" in s]
    summary = {"schema": "live-cold-start-v1", "connected_players": 0,
               "first_seq": first["seq"], "last_seq": frames[-1]["seq"],
               "first_creatures": len(creatures), "first_objects": sum(len(a.get(k, [])) for a in first["areas"] for k in ["cre", "plc", "doors", "items"]),
               "forced_expirations": forced["value"],
               "vm_instruction_errors": errors,
               "load_instruction_budget": [s for s in log.splitlines() if "MP_LOAD " in s],
               "tick_log": [s for s in log.splitlines() if "MGLIVE: seq=" in s]}
    (observations() / "engine.json").write_text(json.dumps(summary, indent=2))
    (observations() / "cold-reports.json").write_text(json.dumps(frames, separators=(",", ":")))
    assert not errors, errors
    return summary
